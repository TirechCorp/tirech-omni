//! Sessões do PuTTY e do KiTTY: o `.reg` exportado do registro do Windows
//! (`…\SimonTatham\PuTTY\Sessions\Nome` e `…\9bis.com\KiTTY\Sessions\Nome`) e a pasta `Sessions`
//! (um arquivo por sessão, `Chave=valor` no PuTTY do Unix e `Chave\valor\` no KiTTY).

use std::path::{Path, PathBuf};

use super::{
    ChaveReg, Formato, Ignorado, Importacao, Trazida, ValorReg, avisar_ppk, decodificar_percentual,
    nao_suportadas, nome_da_conexao, perfil_de_proxy,
};
use crate::modelo::{Connection, Kind, ModoX11, TipoTunel, Tunel};
use crate::redes::TipoProxy;
use crate::t;

/// O formato PuTTY e KiTTY.
pub struct Putty;

impl Formato for Putty {
    fn id(&self) -> &'static str {
        "putty"
    }

    fn nome(&self) -> &'static str {
        "PuTTY / KiTTY"
    }

    fn arquivo_padrao(&self) -> &'static str {
        "putty.reg"
    }

    fn extensoes(&self) -> &'static [&'static str] {
        &["reg"]
    }

    fn locais_provaveis(&self, home: &Path) -> Vec<PathBuf> {
        // `~/.putty/sessions` é uma pasta (o `analisar_pasta` a lê).
        vec![home.join(".putty/sessions")]
    }

    fn analisar(&self, texto: &str, _senha_mestra: Option<&str>) -> Result<Importacao, String> {
        analisar_reg(texto)
    }
}

/// Uma sessão achada: onde está (pastas), o nome e os valores.
struct Sessao {
    pasta: Vec<String>,
    nome: String,
    valores: ChaveReg,
}

/// Lê o `.reg` de um PuTTY ou KiTTY.
fn analisar_reg(texto: &str) -> Result<Importacao, String> {
    let chaves = super::ler_reg(texto);
    let mut sessoes = Vec::new();
    let mut viu_programa = false;
    for chave in chaves {
        let minusculo = chave.caminho.to_ascii_lowercase();
        let kitty = minusculo.contains("\\9bis.com\\kitty\\");
        if !kitty && !minusculo.contains("\\simontatham\\putty\\") {
            continue;
        }
        viu_programa = true;
        let Some(posicao) = minusculo.find("\\sessions\\") else {
            continue;
        };
        let resto = &chave.caminho[posicao + "\\sessions\\".len()..];
        if resto.is_empty() || resto.contains('\\') {
            continue;
        }
        let decodificado = decodificar_percentual(resto);
        // A barra no nome é o jeito de pôr a sessão numa pasta (o KiTTY e o export do Tirech Omni).
        let mut partes: Vec<String> = decodificado.split(['/', '\\']).map(str::to_owned).collect();
        let nome = partes.pop().unwrap_or_default();
        sessoes.push(Sessao {
            pasta: partes,
            nome,
            valores: chave,
        });
    }
    if !viu_programa {
        return Err(t!("importar-putty-reg-sem-sessoes"));
    }
    Ok(converter(sessoes))
}

/// Lê a pasta `Sessions`: cada item é o caminho relativo (`Pasta/Nome%20da%20sessao`) e o
/// conteúdo do arquivo.
pub fn analisar_pasta(arquivos: &[(String, String)]) -> Result<Importacao, String> {
    let mut sessoes = Vec::new();
    for (relativo, texto) in arquivos {
        let mut partes: Vec<String> = relativo
            .split(['/', '\\'])
            .filter(|p| !p.is_empty())
            .map(decodificar_percentual)
            .collect();
        let Some(nome) = partes.pop() else {
            continue;
        };
        let valores = ler_arquivo_de_sessao(texto);
        if valores.valores.is_empty() {
            continue;
        }
        sessoes.push(Sessao {
            pasta: partes,
            nome,
            valores,
        });
    }
    if sessoes.is_empty() {
        return Err(t!("importar-putty-pasta-sem-sessoes"));
    }
    Ok(converter(sessoes))
}

/// `Chave=valor` (PuTTY do Unix) ou `Chave\valor\` (KiTTY).
fn ler_arquivo_de_sessao(texto: &str) -> ChaveReg {
    let mut chave = ChaveReg::default();
    for linha in texto.lines() {
        let linha = linha.trim_end_matches('\r');
        let (nome, valor) = if let Some((nome, resto)) = linha.split_once('=') {
            (nome, resto)
        } else if let (Some(resto), true) =
            (linha.strip_suffix('\\'), linha.matches('\\').count() >= 2)
        {
            match resto.split_once('\\') {
                Some(par) => par,
                None => continue,
            }
        } else {
            continue;
        };
        if nome.is_empty() {
            continue;
        }
        chave
            .valores
            .push((nome.to_owned(), ValorReg::Texto(valor.to_owned())));
    }
    chave
}

fn converter(sessoes: Vec<Sessao>) -> Importacao {
    let mut importacao = Importacao::default();
    let mut outros: Vec<(String, usize)> = Vec::new();
    let mut proxies_sem_suporte = false;
    for sessao in sessoes {
        if sessao.nome.eq_ignore_ascii_case("Default Settings") {
            continue;
        }
        let v = &sessao.valores;
        let mut host = v.texto("HostName").unwrap_or_default().trim().to_owned();
        if host.is_empty() {
            continue;
        }
        let mut usuario = v.texto("UserName").unwrap_or_default().trim().to_owned();
        // O PuTTY aceita `usuario@servidor` no campo do servidor.
        if let Some((u, h)) = host.clone().rsplit_once('@') {
            if usuario.is_empty() {
                usuario = u.to_owned();
            }
            host = h.to_owned();
        }
        let protocolo = v
            .texto("Protocol")
            .unwrap_or("ssh")
            .trim()
            .to_ascii_lowercase();
        let nome = nome_da_conexao(&sessao.nome, &host);
        let mut c = match protocolo.as_str() {
            "ssh" | "" => {
                let mut c = Connection::new(nome, Kind::Ssh);
                c.port = porta_do(v, "PortNumber", 22);
                c
            }
            "telnet" => {
                let mut c = Connection::new(nome, Kind::Telnet);
                c.port = porta_do(v, "PortNumber", 23);
                c
            }
            outro => {
                match outros.iter_mut().find(|(n, _)| n == outro) {
                    Some((_, quantas)) => *quantas += 1,
                    None => outros.push((outro.to_owned(), 1)),
                }
                continue;
            }
        };
        c.host = host;
        c.user = usuario;
        if c.kind == Kind::Ssh {
            if let Some(chave) = v.texto("PublicKeyFile").filter(|t| !t.trim().is_empty()) {
                avisar_ppk(&mut importacao, chave);
                c.key_file = Some(PathBuf::from(chave.trim()));
            }
            c.x11 = match v.numero("X11Forward") {
                Some(0) => ModoX11::Desligado,
                Some(_) => ModoX11::Confiavel,
                None => ModoX11::Padrao,
            };
            c.tuneis = tuneis(v);
            if let Some(segundos) = v.numero("PingIntervalSecs") {
                c.keepalive_secs = segundos;
            }
        }
        if let Some(terminal) = v.texto("TerminalType").filter(|t| !t.trim().is_empty()) {
            c.term = terminal.trim().to_owned();
        }
        let mut trazida = Trazida::nova(c, sessao.pasta.clone());
        let metodo = proxy_metodo(v);
        match metodo {
            Some(2) | Some(3) => {
                let proxy_host = v.texto("ProxyHost").unwrap_or_default().trim();
                if !proxy_host.is_empty() {
                    let tipo = if metodo == Some(2) {
                        TipoProxy::Socks5
                    } else {
                        TipoProxy::Http
                    };
                    let senha = v
                        .texto("ProxyPassword")
                        .filter(|t| !t.is_empty())
                        .map(str::to_owned);
                    trazida.perfil = Some(perfil_de_proxy(
                        tipo,
                        proxy_host,
                        porta_do(v, "ProxyPort", 1080),
                        v.texto("ProxyUsername").unwrap_or_default(),
                        senha,
                    ));
                }
            }
            Some(0) | None => {}
            Some(_) => proxies_sem_suporte = true,
        }
        for pasta in 1..=sessao.pasta.len() {
            importacao.pasta(sessao.pasta[..pasta].to_vec());
        }
        importacao.conexao(trazida);
    }
    for (protocolo, quantas) in outros {
        importacao.avisar(nao_suportadas(
            quantas,
            Ignorado::Sessao,
            &t!(
                "importar-com-protocolo",
                nome = protocolo.to_ascii_uppercase()
            ),
        ));
    }
    if proxies_sem_suporte {
        importacao.avisar(t!("importar-proxy-sem-suporte"));
    }
    importacao
}

/// A porta de `PortNumber` ou `ProxyPort` (um dword no registro, texto na pasta).
fn porta_do(v: &ChaveReg, nome: &str, padrao: u16) -> u16 {
    v.numero(nome)
        .and_then(|n| u16::try_from(n).ok())
        .filter(|n| *n != 0)
        .unwrap_or(padrao)
}

/// O `ProxyMethod`: número (versões novas) ou o texto das antigas.
fn proxy_metodo(v: &ChaveReg) -> Option<u32> {
    if let Some(n) = v.numero("ProxyMethod") {
        return Some(n);
    }
    let texto = v.texto("ProxyMethod")?.trim().to_ascii_lowercase();
    Some(match texto.as_str() {
        "none" | "" => 0,
        "socks 4" | "socks4" => 1,
        "socks 5" | "socks5" => 2,
        "http" => 3,
        _ => 5,
    })
}

/// O `PortForwardings` (`L8080=servidor:80,R2222=maquina:22,D1080`).
fn tuneis(v: &ChaveReg) -> Vec<Tunel> {
    let todas_locais = v.numero("LocalPortAcceptAll") == Some(1);
    let todas_remotas = v.numero("RemotePortAcceptAll") == Some(1);
    let mut lista = Vec::new();
    for item in v.texto("PortForwardings").unwrap_or_default().split(',') {
        let item = item.trim();
        // Uma família de endereços (`4L…`, `6L…`) vem na frente em versões novas.
        let item = match item.as_bytes() {
            [b'4' | b'6', b'L' | b'R' | b'D', ..] => &item[1..],
            _ => item,
        };
        let Some(tipo_letra) = item.chars().next() else {
            continue;
        };
        let tipo = match tipo_letra {
            'L' => TipoTunel::Local,
            'R' => TipoTunel::Remoto,
            'D' => TipoTunel::Dinamico,
            _ => continue,
        };
        let (origem, destino) = match item[1..].split_once('=') {
            Some((o, d)) => (o, Some(d)),
            None => (&item[1..], None),
        };
        let (endereco, porta) = match origem.rsplit_once(':') {
            Some((e, p)) => (e, p),
            None => ("", origem),
        };
        let Ok(porta) = porta.trim().parse::<u16>() else {
            continue;
        };
        let aberto = matches!(endereco, "*" | "0.0.0.0" | "::")
            || match tipo {
                TipoTunel::Remoto => todas_remotas,
                _ => todas_locais,
            };
        let mut tunel = Tunel::novo(tipo, aberto);
        tunel.porta = porta;
        if tipo != TipoTunel::Dinamico {
            let Some((host, p)) = destino.and_then(|d| d.rsplit_once(':')) else {
                continue;
            };
            let Ok(p) = p.trim().parse::<u16>() else {
                continue;
            };
            tunel.destino_host = host.trim_matches(['[', ']']).to_owned();
            tunel.destino_porta = p;
        }
        lista.push(tunel);
    }
    lista
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::importar::Entrada;

    const AMOSTRA: &str = r#"Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Default%20Settings]
"HostName"=""

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\ACME%20web]
"HostName"="srv.acme.com"
"PortNumber"=dword:000008ae
"Protocol"="ssh"
"UserName"="ana"
"PublicKeyFile"="C:\\chaves\\acme.ppk"
"X11Forward"=dword:00000001
"PortForwardings"="L8080=localhost:80,R2222=10.0.0.2:22,D1080,L127.0.0.1:3306=db:3306"
"LocalPortAcceptAll"=dword:00000000
"ProxyMethod"=dword:00000002
"ProxyHost"="proxy.acme.com"
"ProxyPort"=dword:00000438
"ProxyUsername"="pedro"
"ProxyPassword"="p4ss"
"PingIntervalSecs"=dword:0000001e
"TerminalType"="xterm-256color"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Roteador]
"HostName"="admin@10.0.0.1"
"Protocol"="telnet"

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Porta%20serial]
"HostName"="COM3"
"Protocol"="serial"

[HKEY_CURRENT_USER\Software\9bis.com\KiTTY\Sessions\Clientes%2FBeta]
"HostName"="beta.exemplo.com"
"Protocol"="ssh"
"X11Forward"=dword:00000000
"#;

    #[test]
    fn le_reg_com_tuneis_proxy_e_avisos() {
        let i = Putty.analisar(AMOSTRA, None).unwrap();
        assert_eq!(i.total(), 3);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.name, "ACME web");
        assert_eq!(c[0].conexao.host, "srv.acme.com");
        assert_eq!(c[0].conexao.port, 2222);
        assert_eq!(c[0].conexao.user, "ana");
        assert_eq!(
            c[0].conexao.key_file,
            Some(PathBuf::from("C:\\chaves\\acme.ppk"))
        );
        assert_eq!(c[0].conexao.x11, ModoX11::Confiavel);
        assert_eq!(c[0].conexao.keepalive_secs, 30);
        assert_eq!(c[0].conexao.term, "xterm-256color");
        let t = &c[0].conexao.tuneis;
        assert_eq!(t.len(), 4);
        assert_eq!(
            (
                t[0].tipo,
                t[0].porta,
                t[0].destino_host.as_str(),
                t[0].destino_porta
            ),
            (TipoTunel::Local, 8080, "localhost", 80)
        );
        assert_eq!(
            (
                t[1].tipo,
                t[1].porta,
                t[1].destino_host.as_str(),
                t[1].destino_porta
            ),
            (TipoTunel::Remoto, 2222, "10.0.0.2", 22)
        );
        assert_eq!((t[2].tipo, t[2].porta), (TipoTunel::Dinamico, 1080));
        assert_eq!(t[3].porta, 3306);
        assert!(!t[3].outras_maquinas);
        let perfil = c[0].perfil.as_ref().unwrap();
        assert_eq!(perfil.perfil.name, "Proxy importado (proxy.acme.com:1080)");
        assert_eq!(perfil.usuario, "pedro");
        assert_eq!(perfil.senha.as_deref(), Some("p4ss"));
        assert_eq!(c[1].conexao.kind, Kind::Telnet);
        assert_eq!(c[1].conexao.host, "10.0.0.1");
        assert_eq!(c[1].conexao.user, "admin");
        assert_eq!(c[1].conexao.port, 23);
        assert_eq!(c[2].conexao.name, "Beta");
        assert_eq!(c[2].pasta, ["Clientes"]);
        assert_eq!(c[2].conexao.x11, ModoX11::Desligado);
        assert!(
            i.avisos
                .iter()
                .any(|a| a.contains("1 sessão com protocolo SERIAL"))
        );
        assert!(i.avisos.iter().any(|a| a.contains(".ppk")));
        assert!(
            i.entradas
                .iter()
                .any(|e| matches!(e, Entrada::Pasta(p) if p.caminho == ["Clientes"]))
        );
    }

    #[test]
    fn reg_de_outro_programa_vira_erro() {
        let texto = "Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Software\\Martin Prikryl\\WinSCP 2\\Sessions\\x]\n\"HostName\"=\"a\"\n";
        assert!(Putty.analisar(texto, None).is_err());
    }

    #[test]
    fn le_a_pasta_nos_dois_jeitos() {
        let arquivos = vec![
            (
                "Servidor%20Unix".to_owned(),
                "HostName=unix.exemplo.com\nPortNumber=2200\nProtocol=ssh\nUserName=ana\nX11Forward=1\n"
                    .to_owned(),
            ),
            (
                "Clientes/Gama".to_owned(),
                "HostName\\gama.exemplo.com\\\nPortNumber\\22\\\nProtocol\\ssh\\\n".to_owned(),
            ),
            ("Default%20Settings".to_owned(), "HostName=\n".to_owned()),
        ];
        let i = analisar_pasta(&arquivos).unwrap();
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].conexao.name, "Servidor Unix");
        assert_eq!(c[0].conexao.port, 2200);
        assert_eq!(c[0].conexao.x11, ModoX11::Confiavel);
        assert_eq!(c[1].conexao.host, "gama.exemplo.com");
        assert_eq!(c[1].pasta, ["Clientes"]);
        assert!(analisar_pasta(&[]).is_err());
    }
}
