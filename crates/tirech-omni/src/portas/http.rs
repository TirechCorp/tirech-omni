//! A reescrita das cabeças HTTP que o relé faz (M58, seção 3.8 do plano). Só bytes: o relé lê
//! até `fim_da_cabeca`, passa a cabeça por aqui e deixa o resto seguir direto.

use super::local::apelido_do_site;

/// Onde a cabeça termina (depois do `\r\n\r\n`), se os bytes já a trazem inteira.
#[must_use]
pub fn fim_da_cabeca(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|janela| janela == b"\r\n\r\n")
        .map(|i| i + 4)
}

/// Se a linha é o cabeçalho `nome` (sem diferenciar maiúsculas); devolve o valor sem espaços.
fn valor_de<'a>(linha: &'a [u8], nome: &str) -> Option<&'a str> {
    let n = nome.len();
    if linha.len() <= n || !linha[..n].eq_ignore_ascii_case(nome.as_bytes()) || linha[n] != b':' {
        return None;
    }
    std::str::from_utf8(&linha[n + 1..]).ok().map(str::trim)
}

/// As linhas da cabeça, sem o `\r\n` do fim de cada uma e sem a linha vazia final.
fn linhas(cabeca: &[u8]) -> Vec<&[u8]> {
    let corpo = cabeca.strip_suffix(b"\r\n\r\n").unwrap_or(cabeca);
    corpo
        .split(|&b| b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .collect()
}

fn montar(linhas: &[Vec<u8>]) -> Vec<u8> {
    let mut saida = Vec::new();
    for linha in linhas {
        saida.extend_from_slice(linha);
        saida.extend_from_slice(b"\r\n");
    }
    saida.extend_from_slice(b"\r\n");
    saida
}

/// A cabeça do pedido que sai para o servidor: o `Host` vira o nome do site (se houver) e a
/// conexão fecha depois da resposta (`Connection: close`, sem `Keep-Alive`), de modo que o relé
/// não precisa achar o fim do corpo. Quem pede `Upgrade` (WebSocket) mantém a `Connection`.
#[must_use]
pub fn reescrever_pedido(cabeca: &[u8], host_http: &str) -> Vec<u8> {
    let entrada = linhas(cabeca);
    let upgrade = entrada.iter().any(|l| valor_de(l, "Upgrade").is_some());
    let mut saida: Vec<Vec<u8>> = Vec::new();
    for (i, linha) in entrada.iter().enumerate() {
        if i == 0 {
            saida.push(linha.to_vec());
        } else if valor_de(linha, "Host").is_some() && !host_http.trim().is_empty() {
            saida.push(format!("Host: {}", host_http.trim()).into_bytes());
        } else if valor_de(linha, "Keep-Alive").is_some()
            || valor_de(linha, "Proxy-Connection").is_some()
        {
            continue;
        } else if valor_de(linha, "Connection").is_some() {
            if upgrade {
                saida.push(linha.to_vec());
            }
        } else {
            saida.push(linha.to_vec());
        }
    }
    if !upgrade {
        saida.push(b"Connection: close".to_vec());
    }
    montar(&saida)
}

/// A cabeça da resposta que volta ao navegador: o `Location` absoluto do site passa a apontar
/// para `http://{apelido}.localhost:{porta_local}`, o `Domain=` do `Set-Cookie` sai (o cookie
/// fica no host local) e o resto passa como veio. Sem nome do site, só o `Location` que o
/// servidor mandou para `https://localhost:{porta_local}` cai para `http`.
#[must_use]
pub fn reescrever_resposta(
    cabeca: &[u8],
    host_http: &str,
    apelido: &str,
    porta_local: u16,
) -> Vec<u8> {
    let saida: Vec<Vec<u8>> = linhas(cabeca)
        .into_iter()
        .map(|linha| {
            if let Some(valor) = valor_de(linha, "Location") {
                let novo = reescrever_location(valor, host_http, apelido, porta_local);
                return format!("Location: {novo}").into_bytes();
            }
            if let Some(valor) = valor_de(linha, "Set-Cookie")
                && !host_http.trim().is_empty()
            {
                return format!("Set-Cookie: {}", tirar_dominio(valor)).into_bytes();
            }
            linha.to_vec()
        })
        .collect();
    montar(&saida)
}

fn reescrever_location(valor: &str, host_http: &str, apelido: &str, porta_local: u16) -> String {
    let Some((esquema, resto)) = valor.split_once("://") else {
        return valor.to_owned();
    };
    let (autoridade, caminho) = resto
        .find(['/', '?', '#'])
        .map_or((resto, ""), |i| resto.split_at(i));
    let (host, porta) = match autoridade.rsplit_once(':') {
        Some((h, p)) if p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => {
            (h, p.parse::<u16>().ok())
        }
        _ => (autoridade, None),
    };
    let site = host_http.trim();
    if !site.is_empty() && host.eq_ignore_ascii_case(site) {
        let apelido = if apelido.is_empty() {
            apelido_do_site(site)
        } else {
            apelido.to_owned()
        };
        return format!("http://{apelido}.localhost:{porta_local}{caminho}");
    }
    let daqui = host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1";
    if esquema.eq_ignore_ascii_case("https") && daqui && porta == Some(porta_local) {
        return format!("http://{autoridade}{caminho}");
    }
    valor.to_owned()
}

/// O `Set-Cookie` sem o atributo `Domain=`; os outros atributos ficam.
fn tirar_dominio(valor: &str) -> String {
    valor
        .split(';')
        .filter(|parte| {
            let parte = parte.trim();
            !(parte.len() >= 7 && parte[..7].eq_ignore_ascii_case("domain="))
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texto(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn o_fim_da_cabeca_e_depois_da_linha_vazia() {
        assert_eq!(
            fim_da_cabeca(b"GET / HTTP/1.1\r\nA: b\r\n\r\ncorpo"),
            Some(24)
        );
        assert_eq!(fim_da_cabeca(b"GET / HTTP/1.1\r\nA: b\r\n"), None);
    }

    #[test]
    fn a_cabeca_partida_em_dois_pedacos_so_fecha_com_o_segundo() {
        let mut bytes = b"GET / HTTP/1.1\r\nHost: localhost:8080\r\n\r".to_vec();
        assert_eq!(fim_da_cabeca(&bytes), None);
        bytes.extend_from_slice(b"\n");
        assert_eq!(fim_da_cabeca(&bytes), Some(bytes.len()));
    }

    #[test]
    fn o_pedido_troca_o_host_e_fecha_a_conexao() {
        let cabeca = b"GET /x HTTP/1.1\r\nhost: localhost:8080\r\nConnection: keep-alive\r\nKeep-Alive: timeout=5\r\nAccept: */*\r\n\r\n";
        let saida = texto(&reescrever_pedido(cabeca, "loja.exemplo.com.br"));
        assert_eq!(
            saida,
            "GET /x HTTP/1.1\r\nHost: loja.exemplo.com.br\r\nAccept: */*\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn o_pedido_sem_nome_do_site_deixa_o_host() {
        let cabeca = b"GET / HTTP/1.1\r\nHost: localhost:8443\r\n\r\n";
        let saida = texto(&reescrever_pedido(cabeca, ""));
        assert_eq!(
            saida,
            "GET / HTTP/1.1\r\nHost: localhost:8443\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn o_upgrade_mantem_a_conexao() {
        let cabeca =
            b"GET /ws HTTP/1.1\r\nHost: a:1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n";
        let saida = texto(&reescrever_pedido(cabeca, "site.com"));
        assert_eq!(
            saida,
            "GET /ws HTTP/1.1\r\nHost: site.com\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n"
        );
    }

    #[test]
    fn o_location_absoluto_do_site_vira_o_apelido() {
        let cabeca = b"HTTP/1.1 302 Found\r\nLocation: https://loja.exemplo.com.br/entrar?x=1\r\nContent-Length: 0\r\n\r\n";
        let saida = texto(&reescrever_resposta(
            cabeca,
            "loja.exemplo.com.br",
            "loja-exemplo-com-br",
            8080,
        ));
        assert!(
            saida.contains("Location: http://loja-exemplo-com-br.localhost:8080/entrar?x=1\r\n"),
            "{saida}"
        );
        assert!(saida.contains("Content-Length: 0\r\n"));
        assert!(saida.ends_with("\r\n\r\n"));
    }

    #[test]
    fn o_location_com_porta_e_o_relativo() {
        let site = "loja.exemplo.com.br";
        let ap = "loja-exemplo-com-br";
        let com_porta = b"HTTP/1.1 301 X\r\nLocation: http://loja.exemplo.com.br:8443/a\r\n\r\n";
        assert!(
            texto(&reescrever_resposta(com_porta, site, ap, 9000))
                .contains("Location: http://loja-exemplo-com-br.localhost:9000/a\r\n")
        );
        let relativo = b"HTTP/1.1 301 X\r\nLocation: /dentro\r\n\r\n";
        assert!(
            texto(&reescrever_resposta(relativo, site, ap, 9000)).contains("Location: /dentro\r\n")
        );
        let outro = b"HTTP/1.1 301 X\r\nLocation: https://outro.com/x\r\n\r\n";
        assert!(
            texto(&reescrever_resposta(outro, site, ap, 9000))
                .contains("Location: https://outro.com/x\r\n")
        );
    }

    #[test]
    fn sem_nome_do_site_o_https_local_cai_para_http() {
        let cabeca = b"HTTP/1.1 301 X\r\nLocation: https://localhost:8443/painel\r\n\r\n";
        assert!(
            texto(&reescrever_resposta(cabeca, "", "", 8443))
                .contains("Location: http://localhost:8443/painel\r\n")
        );
    }

    #[test]
    fn o_domain_do_cookie_sai_e_o_resto_fica() {
        let cabeca = b"HTTP/1.1 200 OK\r\nSet-Cookie: sid=abc; Path=/; Domain=.exemplo.com.br; Secure; HttpOnly\r\nSet-Cookie: b=2\r\n\r\n";
        let saida = texto(&reescrever_resposta(cabeca, "loja.exemplo.com.br", "x", 80));
        assert!(
            saida.contains("Set-Cookie: sid=abc; Path=/; Secure; HttpOnly\r\n"),
            "{saida}"
        );
        assert!(saida.contains("Set-Cookie: b=2\r\n"));
    }
}
