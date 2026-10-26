//! O `WinSCP.ini` (e a exportação de sessões do WinSCP): seções `[Sessions\Pasta/Nome]`, com
//! nome e pastas em percentual. Lê SFTP, SCP (como SFTP), FTP, FTPS e S3; a senha embaralhada
//! (`Password`) é recuperada com a chave `usuário + servidor`.

use std::path::{Path, PathBuf};

use super::cripto::winscp_desembaralhar;
use super::{
    Formato, Ignorado, Importacao, Secao, Trazida, avisar_ppk, decodificar_percentual, ler_ini,
    nao_suportadas, nome_da_conexao, opcoes_s3, perfil_de_proxy, porta_ou,
};
use crate::modelo::{Connection, FtpSeguranca, Jump, Kind};
use crate::redes::TipoProxy;
use crate::t;

/// O formato WinSCP.
pub struct WinScp;

impl Formato for WinScp {
    fn id(&self) -> &'static str {
        "winscp"
    }

    fn nome(&self) -> &'static str {
        "WinSCP"
    }

    fn arquivo_padrao(&self) -> &'static str {
        "WinSCP.ini"
    }

    fn extensoes(&self) -> &'static [&'static str] {
        &["ini"]
    }

    fn locais_provaveis(&self, home: &Path) -> Vec<PathBuf> {
        vec![
            home.join("AppData/Roaming/WinSCP.ini"),
            home.join("Documents/WinSCP.ini"),
        ]
    }

    fn analisar(&self, texto: &str, _senha_mestra: Option<&str>) -> Result<Importacao, String> {
        analisar(texto)
    }
}

const PREFIXO: &str = "Sessions\\";

/// Desembaralha o `campo` de `secao` com a chave `usuario + servidor`. Em branco, torto ou de
/// outra sessão: `None` (e conta em `ilegiveis`).
fn senha_de(secao: &Secao, campo: &str, chave: &str, ilegiveis: &mut usize) -> Option<String> {
    let embaralhada = secao
        .texto(campo)
        .map(str::trim)
        .filter(|t| !t.is_empty())?;
    match winscp_desembaralhar(embaralhada, chave) {
        Ok(Some(senha)) => Some(senha),
        _ => {
            *ilegiveis += 1;
            None
        }
    }
}

fn analisar(texto: &str) -> Result<Importacao, String> {
    let secoes = ler_ini(texto);
    let sessoes: Vec<&Secao> = secoes
        .iter()
        .filter(|s| {
            s.nome.len() > PREFIXO.len() && s.nome[..PREFIXO.len()].eq_ignore_ascii_case(PREFIXO)
        })
        .collect();
    if sessoes.is_empty() {
        return Err(t!("importar-winscp-sem-sessoes"));
    }
    let mut importacao = Importacao::default();
    let mut ilegiveis = 0usize;
    let mut protocolos: Vec<(String, usize)> = Vec::new();
    let mut scp = 0usize;
    for secao in sessoes {
        let nome_completo = &secao.nome[PREFIXO.len()..];
        // `Pasta/Sub/Nome`, cada trecho em percentual; `Pasta/` (barra no fim) é uma pasta vazia.
        let mut partes: Vec<String> = nome_completo
            .split('/')
            .map(decodificar_percentual)
            .collect();
        let nome = partes.pop().unwrap_or_default();
        if nome.is_empty() {
            partes.retain(|p| !p.is_empty());
            if !partes.is_empty() {
                importacao.pasta(partes);
            }
            continue;
        }
        if nome.eq_ignore_ascii_case("Default Settings") {
            continue;
        }
        if secao.numero("IsWorkspace") == Some(1) {
            importacao.avisar(t!("importar-winscp-workspaces"));
            continue;
        }
        for (i, _) in partes.iter().enumerate() {
            importacao.pasta(partes[..=i].to_vec());
        }
        let host = secao.texto_ou_vazio("HostName").trim().to_owned();
        if host.is_empty() {
            // Uma sessão sem servidor é só o molde de outras (ou está quebrada).
            continue;
        }
        let usuario = decodificar_percentual(secao.texto_ou_vazio("UserName").trim());
        let protocolo = secao.numero("FSProtocol").unwrap_or(1);
        let porta_texto = secao.texto_ou_vazio("PortNumber");
        let nome = nome_da_conexao(&nome, &host);
        let mut c = match protocolo {
            0..=2 => {
                if protocolo == 0 {
                    scp += 1;
                }
                let mut c = Connection::new(nome, Kind::Sftp);
                c.port = porta_ou(porta_texto, 22);
                c
            }
            5 => {
                let mut c = Connection::new(nome, Kind::Ftp);
                c.ftp_security = match secao.numero("Ftps").unwrap_or(0) {
                    1 => FtpSeguranca::Implicita,
                    2 | 3 => FtpSeguranca::Explicita,
                    _ => FtpSeguranca::Nenhuma,
                };
                c.port = porta_ou(porta_texto, c.ftp_security.porta_padrao());
                c
            }
            7 => {
                let mut c = Connection::new(nome, Kind::S3);
                let porta = porta_ou(porta_texto, 443);
                // O WinSCP não tem campo de bucket: ele fica no primeiro trecho da pasta remota.
                let bucket = decodificar_percentual(secao.texto_ou_vazio("RemoteDirectory"));
                let bucket = bucket
                    .trim_matches('/')
                    .split('/')
                    .next()
                    .unwrap_or_default();
                c.s3 = opcoes_s3(&host, porta, secao.numero("Ftps").unwrap_or(1) != 0, bucket);
                let regiao = secao.texto_ou_vazio("S3DefaultRegion").trim();
                if !regiao.is_empty() {
                    c.s3.region = regiao.to_owned();
                }
                c.port = 0;
                c
            }
            outro => {
                let nome_do_protocolo = match outro {
                    6 => "WebDAV".to_owned(),
                    n => t!("importar-de-codigo", n = n),
                };
                match protocolos.iter_mut().find(|(n, _)| *n == nome_do_protocolo) {
                    Some((_, quantos)) => *quantos += 1,
                    None => protocolos.push((nome_do_protocolo, 1)),
                }
                continue;
            }
        };
        if protocolo != 7 {
            c.host = host.clone();
        }
        c.user = usuario.clone();
        c.pasta_local = decodificar_percentual(secao.texto_ou_vazio("LocalDirectory").trim());
        if protocolo != 7 {
            // No S3 a pasta remota do WinSCP é o bucket, já lido acima.
            c.pasta_remota = decodificar_percentual(secao.texto_ou_vazio("RemoteDirectory").trim());
        }
        let chave = decodificar_percentual(secao.texto_ou_vazio("PublicKeyFile").trim());
        if !chave.is_empty() {
            avisar_ppk(&mut importacao, &chave);
            c.key_file = Some(PathBuf::from(chave));
        }
        let mut trazida = Trazida::nova(c, partes);
        // A senha: embaralhada (chave = usuário + servidor) ou, nos arquivos feitos à mão, em claro.
        trazida.senha = senha_de(
            secao,
            "Password",
            &format!("{usuario}{host}"),
            &mut ilegiveis,
        )
        .or_else(|| {
            secao
                .texto("PasswordPlain")
                .filter(|t| !t.is_empty())
                .map(decodificar_percentual)
        });
        trazida.token_s3 = secao
            .texto("S3SessionToken")
            .filter(|t| !t.is_empty())
            .map(str::to_owned);
        proxy_e_tunel(secao, &mut trazida, &mut importacao, &mut ilegiveis);
        let nota = decodificar_percentual(secao.texto_ou_vazio("Note"));
        if !nota.trim().is_empty() {
            trazida.conexao.descricao = nota;
        }
        importacao.conexao(trazida);
    }
    for (nome, quantos) in protocolos {
        importacao.avisar(nao_suportadas(
            quantos,
            Ignorado::Sessao,
            &t!("importar-com-protocolo", nome = nome),
        ));
    }
    if scp > 0 {
        importacao.avisar(t!("importar-winscp-scp", n = scp as i64));
    }
    if ilegiveis > 0 {
        importacao.avisar(t!("importar-winscp-ilegiveis", n = ilegiveis as i64));
    }
    Ok(importacao)
}

/// O proxy (`ProxyMethod` 2 SOCKS5 e 3 HTTP) vira perfil de rede; o túnel SSH (`Tunnel=1`) vira
/// o salto.
fn proxy_e_tunel(
    secao: &Secao,
    trazida: &mut Trazida,
    importacao: &mut Importacao,
    ilegiveis: &mut usize,
) {
    match secao.numero("ProxyMethod").unwrap_or(0) {
        0 => {}
        metodo @ (2 | 3) => {
            let tipo = if metodo == 2 {
                TipoProxy::Socks5
            } else {
                TipoProxy::Http
            };
            let host = secao.texto_ou_vazio("ProxyHost").trim();
            if !host.is_empty() {
                let porta = porta_ou(secao.texto_ou_vazio("ProxyPort"), 1080);
                let usuario = decodificar_percentual(secao.texto_ou_vazio("ProxyUsername"));
                let senha = senha_de(
                    secao,
                    "ProxyPassword",
                    &format!("{usuario}{host}"),
                    ilegiveis,
                );
                trazida.perfil = Some(perfil_de_proxy(tipo, host, porta, &usuario, senha));
            }
        }
        _ => importacao.avisar(t!("importar-proxy-sem-suporte")),
    }
    if secao.numero("Tunnel") == Some(1) {
        let host = secao.texto_ou_vazio("TunnelHostName").trim();
        if !host.is_empty() {
            let usuario = decodificar_percentual(secao.texto_ou_vazio("TunnelUserName"));
            trazida.conexao.jump = Some(Jump {
                host: host.to_owned(),
                port: porta_ou(secao.texto_ou_vazio("TunnelPortNumber"), 22),
                user: usuario.clone(),
                credencial: None,
            });
            trazida.senha_salto = senha_de(
                secao,
                "TunnelPassword",
                &format!("{usuario}{host}"),
                ilegiveis,
            );
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::importar::cripto::winscp_embaralhar;

    fn embaralhada(usuario: &str, host: &str, senha: &str) -> String {
        winscp_embaralhar(&format!("{usuario}{host}"), senha, &mut || 7).unwrap()
    }

    fn amostra() -> String {
        format!(
            "[Configuration]\nRandomSeed=abc\n\n\
             [Sessions\\Default%20Settings]\nHostName=\n\n\
             [Sessions\\Clientes/ACME/Web%201]\nHostName=srv.exemplo.com\nPortNumber=2222\n\
             UserName=ana\nPassword={}\nFSProtocol=1\nNote=servidor%20do%20site\n\
             LocalDirectory=C%3A%5Cprojetos%5Cmeu%20site\nRemoteDirectory=/var/www/meu%20site\n\
             PublicKeyFile=C%3A%5Cchaves%5Cid.ppk\nTunnel=1\nTunnelHostName=salto.exemplo.com\n\
             TunnelUserName=bia\nTunnelPassword={}\nProxyMethod=2\nProxyHost=proxy.exemplo.com\n\
             ProxyPort=1081\nProxyUsername=pedro\nProxyPassword={}\n\n\
             [Sessions\\Clientes/Vazia/]\nHostName=\n\n\
             [Sessions\\Publico]\nHostName=ftp.exemplo.org\nFSProtocol=5\nFtps=1\nUserName=anonymous\n\
             PasswordPlain=segredo\n\n\
             [Sessions\\Dav]\nHostName=dav.exemplo.org\nFSProtocol=6\nUserName=x\n\n\
             [Sessions\\Antiga]\nHostName=velho.exemplo.org\nFSProtocol=0\nUserName=root\n\
             Password=00zz\n",
            embaralhada("ana", "srv.exemplo.com", "s3nha-ç"),
            embaralhada("bia", "salto.exemplo.com", "salto!"),
            embaralhada("pedro", "proxy.exemplo.com", "p4ss"),
        )
    }

    #[test]
    fn le_pastas_senhas_salto_proxy_e_avisos() {
        let i = WinScp.analisar(&amostra(), None).unwrap();
        assert_eq!(i.total(), 3);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.name, "Web 1");
        assert_eq!(c[0].pasta, ["Clientes", "ACME"]);
        assert_eq!(c[0].conexao.kind, Kind::Sftp);
        assert_eq!(c[0].conexao.port, 2222);
        assert_eq!(c[0].conexao.user, "ana");
        assert_eq!(c[0].senha.as_deref(), Some("s3nha-ç"));
        assert_eq!(c[0].conexao.descricao, "servidor do site");
        assert_eq!(
            c[0].conexao.key_file,
            Some(PathBuf::from("C:\\chaves\\id.ppk"))
        );
        let salto = c[0].conexao.jump.as_ref().unwrap();
        assert_eq!(
            (salto.host.as_str(), salto.port, salto.user.as_str()),
            ("salto.exemplo.com", 22, "bia")
        );
        assert_eq!(c[0].senha_salto.as_deref(), Some("salto!"));
        let perfil = c[0].perfil.as_ref().unwrap();
        assert_eq!(perfil.usuario, "pedro");
        assert_eq!(perfil.senha.as_deref(), Some("p4ss"));
        assert_eq!(
            perfil.perfil.name,
            "Proxy importado (proxy.exemplo.com:1081)"
        );
        assert_eq!(c[1].conexao.kind, Kind::Ftp);
        assert_eq!(c[1].conexao.ftp_security, FtpSeguranca::Implicita);
        assert_eq!(c[1].conexao.port, 990);
        assert_eq!(c[1].senha.as_deref(), Some("segredo"));
        assert!(c[1].pasta.is_empty());
        assert_eq!(c[2].conexao.name, "Antiga");
        assert!(c[2].senha.is_none());
        assert!(
            i.avisos
                .iter()
                .any(|a| a.contains("1 sessão com protocolo WebDAV"))
        );
        assert!(i.avisos.iter().any(|a| a.contains("virou SFTP")));
        assert!(i.avisos.iter().any(|a| a.contains("1 senha do WinSCP")));
        assert!(i.avisos.iter().any(|a| a.contains(".ppk")));
    }

    #[test]
    fn le_as_pastas_padrao() {
        let i = WinScp.analisar(&amostra(), None).unwrap();
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.pasta_local, "C:\\projetos\\meu site");
        assert_eq!(c[0].conexao.pasta_remota, "/var/www/meu site");
        assert_eq!(c[1].conexao.pasta_local, "");
        assert_eq!(c[1].conexao.pasta_remota, "");
    }

    #[test]
    fn pasta_vazia_aparece() {
        let i = WinScp.analisar(&amostra(), None).unwrap();
        let pastas: Vec<Vec<String>> = i
            .entradas
            .iter()
            .filter_map(|e| match e {
                crate::importar::Entrada::Pasta(p) => Some(p.caminho.clone()),
                _ => None,
            })
            .collect();
        assert!(pastas.contains(&vec!["Clientes".to_owned(), "Vazia".to_owned()]));
        assert!(pastas.contains(&vec!["Clientes".to_owned(), "ACME".to_owned()]));
    }

    #[test]
    fn sem_sessoes_vira_erro() {
        assert!(WinScp.analisar("[Configuration]\nA=1\n", None).is_err());
    }
}
