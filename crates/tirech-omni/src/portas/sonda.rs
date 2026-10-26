//! A sonda, a parte que só olha bytes (M58, seção 3.5 do plano): quem abre o canal e espera
//! está em `descobrir.rs`. Aqui ficam o que dizer ("GET /"), como ler o que volta e como
//! decidir o que é.

use super::catalogo::{self, App, Tipo};
use super::{Atalho, Servico};

/// O que uma resposta HTTP trouxe (ou o alerta TLS que a substituiu).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RespostaHttp {
    pub codigo: u16,
    pub servidor: Option<String>,
    pub location: Option<String>,
    pub titulo: Option<String>,
    /// O servidor fala TLS nesta porta (alerta TLS ou um `400` que fala em HTTPS).
    pub tls: bool,
}

impl RespostaHttp {
    /// O `Servico` que a resposta revela.
    #[must_use]
    pub fn servico(&self, https: bool) -> Servico {
        Servico::Web {
            https: https || self.tls,
            titulo: self.titulo.clone(),
            servidor: self.servidor.clone(),
            codigo: self.codigo,
        }
    }
}

/// Se a porta aceita o `GET /` da sonda (os bancos do catálogo não levam).
#[must_use]
pub fn manda_get(porta: u16) -> bool {
    catalogo::aceita_get(porta)
}

/// O pedido da sonda. `host_http` vazio usa `localhost:{porta}`.
#[must_use]
pub fn pedido_http(porta: u16, caminho: &str, host_http: &str) -> Vec<u8> {
    let host = if host_http.trim().is_empty() {
        format!("localhost:{porta}")
    } else {
        host_http.trim().to_owned()
    };
    let caminho = if caminho.is_empty() { "/" } else { caminho };
    format!(
        "GET {caminho} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: Tirech Omni\r\nAccept: text/html,*/*\r\n\r\n"
    )
    .into_bytes()
}

/// Se os bytes começam com um alerta TLS (`0x15 0x03`) ou um `ServerHello`/registro TLS.
#[must_use]
pub fn parece_tls(bytes: &[u8]) -> bool {
    matches!(bytes, [0x15 | 0x16, 0x03, ..])
}

fn conhecido(nome: &str, tipo: Tipo, versao: Option<String>) -> Servico {
    Servico::Conhecido {
        nome: nome.to_owned(),
        tipo,
        versao,
    }
}

/// O serviço que se apresenta sozinho, antes de qualquer pedido (e o Redis, que responde `-ERR`
/// ao `GET`).
#[must_use]
pub fn classificar_saudacao(bytes: &[u8]) -> Option<Servico> {
    let texto = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
    let primeira = texto.lines().next().unwrap_or("").trim();
    if primeira.starts_with("SSH-") {
        return Some(conhecido("SSH", Tipo::Sistema, Some(primeira.to_owned())));
    }
    if bytes.len() > 5 && bytes[4] == 0x0a && bytes[3] == 0 {
        let versao: String = bytes[5..]
            .iter()
            .take_while(|&&b| b != 0)
            .map(|&b| char::from(b))
            .collect();
        if versao.chars().next().is_some_and(|c| c.is_ascii_digit())
            && versao.chars().all(|c| c.is_ascii_graphic())
        {
            return Some(conhecido("MySQL/MariaDB", Tipo::Banco, Some(versao)));
        }
    }
    if primeira.starts_with("220") {
        let maiusculo = primeira.to_ascii_uppercase();
        let nome = if maiusculo.contains("SMTP") {
            "SMTP"
        } else if maiusculo.contains("FTP") {
            "FTP"
        } else {
            "FTP ou SMTP"
        };
        return Some(conhecido(nome, Tipo::Outro, None));
    }
    if primeira.starts_with("+OK") {
        return Some(conhecido("POP3", Tipo::Outro, None));
    }
    if primeira.starts_with("* OK") {
        return Some(conhecido("IMAP", Tipo::Outro, None));
    }
    if let Some(versao) = primeira.strip_prefix("RFB ") {
        return Some(conhecido(
            "VNC",
            Tipo::Outro,
            Some(versao.trim().to_owned()),
        ));
    }
    if primeira.starts_with("-ERR")
        || primeira.starts_with("-DENIED")
        || primeira.starts_with("-NOAUTH")
    {
        return Some(conhecido("Redis", Tipo::Banco, None));
    }
    None
}

/// Lê a resposta HTTP, mesmo cortada no meio do corpo. `None` se não for HTTP nem TLS.
#[must_use]
pub fn ler_resposta_http(bytes: &[u8]) -> Option<RespostaHttp> {
    if parece_tls(bytes) {
        return Some(RespostaHttp {
            tls: true,
            ..RespostaHttp::default()
        });
    }
    if !bytes.starts_with(b"HTTP/1.") && !bytes.starts_with(b"HTTP/2") {
        return None;
    }
    let fim = super::http::fim_da_cabeca(bytes);
    let (cabeca, corpo) = match fim {
        Some(i) => (&bytes[..i], &bytes[i..]),
        None => (bytes, &bytes[bytes.len()..]),
    };
    let cabeca = String::from_utf8_lossy(cabeca);
    let mut linhas = cabeca.lines();
    let codigo = linhas
        .next()?
        .split_whitespace()
        .nth(1)?
        .parse::<u16>()
        .ok()?;
    let mut resposta = RespostaHttp {
        codigo,
        ..RespostaHttp::default()
    };
    for linha in linhas {
        let Some((nome, valor)) = linha.split_once(':') else {
            continue;
        };
        let valor = valor.trim();
        if nome.eq_ignore_ascii_case("server") && !valor.is_empty() {
            resposta.servidor = Some(valor.to_owned());
        } else if nome.eq_ignore_ascii_case("location") && !valor.is_empty() {
            resposta.location = Some(valor.to_owned());
        }
    }
    let corpo = String::from_utf8_lossy(corpo);
    resposta.titulo = titulo_do_html(&corpo);
    if codigo == 400 {
        let minusculo = corpo.to_ascii_lowercase();
        if minusculo.contains("https") || minusculo.contains("ssl") || minusculo.contains("tls") {
            resposta.tls = true;
        }
    }
    Some(resposta)
}

/// O texto do `<title>`, com as entidades decodificadas e os espaços normalizados. Sem o
/// `</title>` ainda, não há título.
#[must_use]
pub fn titulo_do_html(html: &str) -> Option<String> {
    let minusculo = html.to_ascii_lowercase();
    let abre = minusculo.find("<title")?;
    let inicio = abre + minusculo[abre..].find('>')? + 1;
    let fim = inicio + minusculo[inicio..].find("</title")?;
    let titulo = decodificar_entidades(&html[inicio..fim]);
    let titulo = titulo.split_whitespace().collect::<Vec<_>>().join(" ");
    (!titulo.is_empty()).then_some(titulo)
}

fn decodificar_entidades(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut resto = texto;
    while let Some(i) = resto.find('&') {
        saida.push_str(&resto[..i]);
        resto = &resto[i..];
        let decodificada = resto
            .find(';')
            .filter(|&fim| fim <= 10)
            .and_then(|fim| entidade(&resto[1..fim]).map(|c| (c, fim + 1)));
        match decodificada {
            Some((c, tamanho)) => {
                saida.push(c);
                resto = &resto[tamanho..];
            }
            None => {
                saida.push('&');
                resto = &resto[1..];
            }
        }
    }
    saida.push_str(resto);
    saida
}

fn entidade(nome: &str) -> Option<char> {
    match nome {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => {
            let numero = nome.strip_prefix('#')?;
            let codigo = match numero.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => numero.parse().ok()?,
            };
            char::from_u32(codigo)
        }
    }
}

/// Se o título da resposta é do app.
#[must_use]
pub fn titulo_bate(app: &App, resposta: &RespostaHttp) -> bool {
    resposta
        .titulo
        .as_deref()
        .is_some_and(|t| t.to_lowercase().contains(app.titulo))
}

/// O atalho "Abrir {app}" para o `caminho`, se a resposta mostra o app: um 200 com o título do
/// app, ou um 30x que aponta para o mesmo caminho (`/phpmyadmin` → `/phpmyadmin/`).
#[must_use]
pub fn atalho(app: &App, caminho: &str, resposta: &RespostaHttp) -> Option<Atalho> {
    let vale = match resposta.codigo {
        200 => titulo_bate(app, resposta),
        300..=399 => resposta
            .location
            .as_deref()
            .is_some_and(|l| mesmo_caminho(l, caminho)),
        _ => false,
    };
    vale.then(|| Atalho {
        nome: app.nome.to_owned(),
        caminho: caminho.to_owned(),
    })
}

/// Se o `Location` (absoluto ou relativo) cai no `caminho`, sem diferenciar a barra do fim.
fn mesmo_caminho(location: &str, caminho: &str) -> bool {
    let sem_esquema = location
        .split_once("://")
        .map_or(location, |(_, r)| r.find('/').map_or("/", |i| &r[i..]));
    let so_caminho = sem_esquema.split(['?', '#']).next().unwrap_or("");
    so_caminho
        .trim_end_matches('/')
        .eq_ignore_ascii_case(caminho.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nome_de(servico: Option<Servico>) -> (String, Option<String>) {
        match servico {
            Some(Servico::Conhecido { nome, versao, .. }) => (nome, versao),
            outro => panic!("esperava um serviço conhecido: {outro:?}"),
        }
    }

    #[test]
    fn as_saudacoes_dos_servicos_que_falam_primeiro() {
        let (nome, versao) = nome_de(classificar_saudacao(b"SSH-2.0-OpenSSH_9.6\r\n"));
        assert_eq!(
            (nome.as_str(), versao.as_deref()),
            ("SSH", Some("SSH-2.0-OpenSSH_9.6"))
        );
        let mut mysql = vec![0x4a, 0, 0, 0, 0x0a];
        mysql.extend_from_slice(b"8.0.36-0ubuntu0\0");
        mysql.extend_from_slice(&[1, 2, 3]);
        let (nome, versao) = nome_de(classificar_saudacao(&mysql));
        assert_eq!(
            (nome.as_str(), versao.as_deref()),
            ("MySQL/MariaDB", Some("8.0.36-0ubuntu0"))
        );
        let (nome, _) = nome_de(classificar_saudacao(
            b"220 mail.exemplo.com ESMTP Postfix\r\n",
        ));
        assert_eq!(nome, "SMTP");
        let (nome, _) = nome_de(classificar_saudacao(b"220 (vsFTPd 3.0.5)\r\n"));
        assert_eq!(nome, "FTP");
        let (nome, versao) = nome_de(classificar_saudacao(b"RFB 003.008\n"));
        assert_eq!((nome.as_str(), versao.as_deref()), ("VNC", Some("003.008")));
        let (nome, _) = nome_de(classificar_saudacao(b"+OK Dovecot ready.\r\n"));
        assert_eq!(nome, "POP3");
        let (nome, _) = nome_de(classificar_saudacao(
            b"* OK [CAPABILITY IMAP4rev1] ready\r\n",
        ));
        assert_eq!(nome, "IMAP");
    }

    #[test]
    fn o_redis_responde_err_ao_get() {
        let (nome, _) = nome_de(classificar_saudacao(b"-ERR unknown command 'GET'\r\n"));
        assert_eq!(nome, "Redis");
    }

    #[test]
    fn silencio_e_http_nao_sao_saudacao() {
        assert_eq!(classificar_saudacao(b""), None);
        assert_eq!(classificar_saudacao(b"HTTP/1.1 200 OK\r\n\r\n"), None);
    }

    #[test]
    fn o_pedido_usa_o_host_local_ou_o_do_site() {
        let pedido = String::from_utf8(pedido_http(8080, "/", "")).unwrap();
        assert!(
            pedido.starts_with("GET / HTTP/1.1\r\nHost: localhost:8080\r\n"),
            "{pedido}"
        );
        assert!(pedido.contains("Connection: close\r\n"));
        assert!(pedido.contains("User-Agent: Tirech Omni\r\n"));
        assert!(pedido.ends_with("\r\n\r\n"));
        let pedido = String::from_utf8(pedido_http(80, "/phpmyadmin/", "loja.com")).unwrap();
        assert!(pedido.starts_with("GET /phpmyadmin/ HTTP/1.1\r\nHost: loja.com\r\n"));
    }

    #[test]
    fn a_resposta_http_traz_codigo_servidor_e_titulo_em_varias_linhas() {
        let bytes = b"HTTP/1.1 200 OK\r\nServer: nginx/1.24\r\nContent-Type: text/html\r\n\r\n<html><head>\n<TITLE>\n  Meu &amp; seu\n  &#233;&#xe9; site &nbsp;novo\n</TITLE></head>";
        let r = ler_resposta_http(bytes).unwrap();
        assert_eq!(r.codigo, 200);
        assert_eq!(r.servidor.as_deref(), Some("nginx/1.24"));
        assert_eq!(r.titulo.as_deref(), Some("Meu & seu éé site novo"));
        assert!(!r.tls);
    }

    #[test]
    fn a_resposta_cortada_no_meio_do_corpo_ainda_serve() {
        let r = ler_resposta_http(
            b"HTTP/1.1 302 Found\r\nLocation: /entrar\r\n\r\n<html><head><title>Ent",
        )
        .unwrap();
        assert_eq!(r.codigo, 302);
        assert_eq!(r.location.as_deref(), Some("/entrar"));
        assert_eq!(r.titulo, None, "sem o `</title>` ainda não há título");
        let r = ler_resposta_http(b"HTTP/1.0 200 OK\r\nServer: x\r\n").unwrap();
        assert_eq!(r.codigo, 200);
        assert_eq!(r.servidor.as_deref(), Some("x"));
    }

    #[test]
    fn o_alerta_tls_e_o_400_de_https_marcam_tls() {
        let r = ler_resposta_http(&[0x15, 0x03, 0x01, 0x00, 0x02, 0x02, 0x0a]).unwrap();
        assert!(r.tls);
        let r = ler_resposta_http(
            b"HTTP/1.1 400 Bad Request\r\nServer: nginx\r\n\r\n<html><head><title>400 The plain HTTP request was sent to HTTPS port</title>",
        )
        .unwrap();
        assert!(r.tls);
        assert_eq!(r.codigo, 400);
        let r = ler_resposta_http(b"HTTP/1.1 400 Bad Request\r\n\r\nroto").unwrap();
        assert!(!r.tls);
    }

    #[test]
    fn o_que_nao_e_http_nao_vira_resposta() {
        assert_eq!(ler_resposta_http(b"-ERR nope\r\n"), None);
        assert_eq!(ler_resposta_http(b""), None);
        assert_eq!(ler_resposta_http(b"HTTP/1.1 abc\r\n\r\n"), None);
    }

    #[test]
    fn o_atalho_do_phpmyadmin_vem_do_titulo_ou_do_redirecionamento() {
        let app = catalogo::APPS
            .iter()
            .find(|a| a.nome == "phpMyAdmin")
            .unwrap();
        let ok = RespostaHttp {
            codigo: 200,
            titulo: Some("phpMyAdmin".into()),
            ..RespostaHttp::default()
        };
        assert_eq!(
            atalho(app, "/phpmyadmin/", &ok),
            Some(Atalho {
                nome: "phpMyAdmin".into(),
                caminho: "/phpmyadmin/".into()
            })
        );
        let outro = RespostaHttp {
            codigo: 200,
            titulo: Some("Bem-vindo".into()),
            ..RespostaHttp::default()
        };
        assert_eq!(atalho(app, "/phpmyadmin/", &outro), None);
        let redirecao = RespostaHttp {
            codigo: 301,
            location: Some("http://srv/phpmyadmin".into()),
            ..RespostaHttp::default()
        };
        assert!(atalho(app, "/phpmyadmin/", &redirecao).is_some());
        let para_longe = RespostaHttp {
            codigo: 302,
            location: Some("/login".into()),
            ..RespostaHttp::default()
        };
        assert_eq!(atalho(app, "/phpmyadmin/", &para_longe), None);
        let nao_achou = RespostaHttp {
            codigo: 404,
            titulo: Some("phpMyAdmin".into()),
            ..RespostaHttp::default()
        };
        assert_eq!(atalho(app, "/phpmyadmin/", &nao_achou), None);
    }

    #[test]
    fn o_get_nao_sai_para_as_portas_que_nao_sao_http() {
        for porta in [3306, 5432, 6379, 27017, 11211, 1433, 1521] {
            assert!(!manda_get(porta), "{porta}");
        }
        assert!(manda_get(80));
        assert!(manda_get(4567), "fora do catálogo vale");
    }

    #[test]
    fn a_resposta_vira_servico_web() {
        let r = RespostaHttp {
            codigo: 200,
            titulo: Some("X".into()),
            tls: true,
            ..RespostaHttp::default()
        };
        assert!(matches!(
            r.servico(false),
            Servico::Web {
                https: true,
                codigo: 200,
                ..
            }
        ));
    }
}
