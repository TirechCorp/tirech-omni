//! O que o Ctrl+clique faz com cada link do terminal (M49). Puro: dado o endereço, o tipo da
//! sessão e o nome desta máquina, diz o que abrir. Quem abre é o [`Opener`](crate::abrir::Opener)
//! (o navegador e a pasta) ou a tela (o painel de arquivos da conexão).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::modelo::Kind;
use crate::t;

/// Para onde um link leva.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DestinoDoLink {
    /// O endereço (`http`, `https` ou `mailto`), no programa que o sistema associa a ele.
    Navegador(String),
    /// Um caminho deste computador. Quem abre mostra a pasta (ver [`pasta_do_link`]), nunca o
    /// arquivo.
    PastaLocal(PathBuf),
    /// Um caminho do servidor da conexão, no painel de arquivos dela.
    PainelRemoto(String),
    /// Uma porta do servidor da conexão (`http://localhost:3000/` impresso lá): a tela
    /// encaminha a porta e abre o endereço daqui (M58).
    PortaDoServidor {
        host: String,
        porta: u16,
        caminho: String,
        https: bool,
    },
    /// Não abre daqui: o menu só oferece copiar.
    SoCopiar,
}

/// O destino de `uri`, clicada numa sessão do tipo `sessao` (o tipo da conexão que a abriu; o
/// shell sem conexão é [`Kind::Local`]) neste computador, de nome `maquina`.
///
/// Só `http`, `https`, `mailto` e `file` abrem. O `file` de um shell local abre a pasta, porque
/// abrir o arquivo pelo sistema pode rodar um programa com um clique. Numa sessão SSH o `file`
/// de qualquer host abre o painel de arquivos da conexão: o nome que o servidor imprime quase
/// nunca é o endereço da conexão, e um caminho que não existe lá mostra o erro de sempre.
/// Qualquer outro caso (esquema desconhecido, `javascript:`, `file` de outro host num shell
/// local, `file` no Telnet) só copia.
///
/// Numa sessão SSH, com `links_pelo_tunel`, o `http`/`https` de `localhost` (e dos outros
/// endereços de loopback ou `0.0.0.0`) é uma porta do servidor: vira
/// [`DestinoDoLink::PortaDoServidor`], porque o `localhost` do terminal é o do servidor.
#[must_use]
pub fn destino(uri: &str, sessao: Kind, maquina: &str, links_pelo_tunel: bool) -> DestinoDoLink {
    let uri = uri.trim();
    if uri.chars().any(char::is_control) {
        return DestinoDoLink::SoCopiar;
    }
    let Some((esquema, resto)) = uri.split_once(':') else {
        return DestinoDoLink::SoCopiar;
    };
    match esquema.to_ascii_lowercase().as_str() {
        "http" | "https" if resto.strip_prefix("//").is_some_and(|r| !r.is_empty()) => {
            let https = esquema.eq_ignore_ascii_case("https");
            match porta_do_servidor(resto, https) {
                Some(destino) if links_pelo_tunel && sessao == Kind::Ssh => destino,
                _ => DestinoDoLink::Navegador(uri.to_owned()),
            }
        }
        "mailto" if !resto.is_empty() => DestinoDoLink::Navegador(uri.to_owned()),
        "file" => destino_de_arquivo(resto, sessao, maquina),
        _ => DestinoDoLink::SoCopiar,
    }
}

/// O `//host[:porta]/caminho` (`resto` é o que vem depois de `http:`) quando o host é o próprio
/// servidor: `localhost`, `127.x.y.z`, `0.0.0.0`, `::1` ou `[::1]`. `0.0.0.0` e `127.x` viram
/// `127.0.0.1`; sem porta, vale a do esquema.
fn porta_do_servidor(resto: &str, https: bool) -> Option<DestinoDoLink> {
    let sem_barras = resto.strip_prefix("//")?;
    let fim = sem_barras.find(['/', '?', '#']).unwrap_or(sem_barras.len());
    let (autoridade, caminho) = sem_barras.split_at(fim);
    // Com usuário e senha na frente (`a@host`), o link é outra coisa: fica no navegador.
    if autoridade.contains('@') {
        return None;
    }
    let (host, porta) = if let Some(dentro) = autoridade.strip_prefix('[') {
        let (host, depois) = dentro.split_once(']')?;
        (host, depois.strip_prefix(':'))
    } else {
        match autoridade.rsplit_once(':') {
            Some((host, porta)) if !host.contains(':') => (host, Some(porta)),
            _ => (autoridade, None),
        }
    };
    let host = match host.to_ascii_lowercase().as_str() {
        "localhost" => "localhost".to_owned(),
        "0.0.0.0" => "127.0.0.1".to_owned(),
        "::1" => "::1".to_owned(),
        h if h
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback()) =>
        {
            "127.0.0.1".to_owned()
        }
        _ => return None,
    };
    let porta = match porta {
        None | Some("") => {
            if https {
                443
            } else {
                80
            }
        }
        Some(p) => p.parse::<u16>().ok().filter(|&p| p > 0)?,
    };
    let caminho = if caminho.starts_with('/') {
        caminho.to_owned()
    } else {
        format!("/{caminho}")
    };
    Some(DestinoDoLink::PortaDoServidor {
        host,
        porta,
        caminho,
        https,
    })
}

/// O `file://<host>/<caminho>` (`resto` é o que vem depois de `file:`).
fn destino_de_arquivo(resto: &str, sessao: Kind, maquina: &str) -> DestinoDoLink {
    let Some(sem_barras) = resto.strip_prefix("//") else {
        return DestinoDoLink::SoCopiar;
    };
    let (host, caminho) = match sem_barras.find('/') {
        Some(n) => sem_barras.split_at(n),
        None => return DestinoDoLink::SoCopiar,
    };
    // A consulta e o fragmento não fazem parte do caminho (um `#` do nome vem como `%23`).
    let caminho = caminho.split(['?', '#']).next().unwrap_or_default();
    let caminho = decodificar(caminho);
    match sessao {
        Kind::Local if e_esta_maquina(host, maquina) => {
            DestinoDoLink::PastaLocal(caminho_local(&caminho))
        }
        Kind::Ssh | Kind::Sftp => DestinoDoLink::PainelRemoto(caminho),
        _ => DestinoDoLink::SoCopiar,
    }
}

/// Se o host do `file://` é este computador: vazio, `localhost` ou o nome da máquina (só a
/// parte antes do primeiro ponto, sem diferenciar maiúsculas).
fn e_esta_maquina(host: &str, maquina: &str) -> bool {
    let primeiro = |nome: &str| nome.split('.').next().unwrap_or_default().to_lowercase();
    host.is_empty()
        || host.eq_ignore_ascii_case("localhost")
        || (!maquina.is_empty() && primeiro(host) == primeiro(maquina))
}

/// O caminho do `file:` como o sistema o escreve: no Windows, `/C:/pasta` vira `C:/pasta`.
fn caminho_local(caminho: &str) -> PathBuf {
    let sem_barra = caminho.strip_prefix('/').unwrap_or(caminho);
    let unidade = sem_barra.as_bytes();
    if cfg!(windows) && unidade.len() >= 2 && unidade[0].is_ascii_alphabetic() && unidade[1] == b':'
    {
        PathBuf::from(sem_barra)
    } else {
        PathBuf::from(caminho)
    }
}

/// Decodifica os `%XX` como UTF-8. Um `%` torto, ou bytes que não formam UTF-8, deixam o texto
/// como veio.
fn decodificar(texto: &str) -> String {
    let bytes = texto.as_bytes();
    let mut saida = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let par = bytes
            .get(i + 1..i + 3)
            .filter(|_| bytes[i] == b'%')
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match par {
            Some(byte) => {
                saida.push(byte);
                i += 3;
            }
            None => {
                saida.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8(saida).unwrap_or_else(|_| texto.to_owned())
}

/// O rótulo do item de abrir do menu e da dica, ou `None` para o que só copia.
#[must_use]
pub fn rotulo(destino: &DestinoDoLink) -> Option<String> {
    Some(match destino {
        DestinoDoLink::Navegador(uri) if uri.to_ascii_lowercase().starts_with("mailto:") => {
            t!("links-escrever-email")
        }
        DestinoDoLink::Navegador(_) => t!("links-abrir-no-navegador"),
        DestinoDoLink::PastaLocal(_) => t!("links-abrir-a-pasta"),
        DestinoDoLink::PainelRemoto(_) => t!("links-abrir-no-painel"),
        DestinoDoLink::PortaDoServidor { .. } => t!("links-abrir-pelo-tunel"),
        DestinoDoLink::SoCopiar => return None,
    })
}

/// A pasta que o sistema abre para um `PastaLocal`: o próprio caminho, se for uma pasta; senão a
/// pasta que contém o arquivo.
#[must_use]
pub fn pasta_do_link(caminho: &Path) -> PathBuf {
    if caminho.is_dir() {
        return caminho.to_owned();
    }
    caminho
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(caminho)
        .to_owned()
}

/// O nome deste computador, lido uma vez (vazio se o sistema não disser).
#[must_use]
pub fn nome_desta_maquina() -> &'static str {
    static NOME: OnceLock<String> = OnceLock::new();
    NOME.get_or_init(|| gethostname::gethostname().to_string_lossy().into_owned())
}

#[cfg(test)]
mod testes {
    use super::*;

    const MAQUINA: &str = "Ana-PC.casa.lan";

    fn d(uri: &str, sessao: Kind) -> DestinoDoLink {
        destino(uri, sessao, MAQUINA, true)
    }

    fn rotulo_de(uri: &str, sessao: Kind) -> Option<String> {
        rotulo(&d(uri, sessao))
    }

    #[test]
    fn http_e_https_abrem_no_navegador_em_qualquer_sessao() {
        for sessao in [Kind::Local, Kind::Ssh, Kind::Telnet] {
            assert_eq!(
                d("https://a.b/c?x=1;y=2#f", sessao),
                DestinoDoLink::Navegador("https://a.b/c?x=1;y=2#f".into())
            );
            assert_eq!(
                d("http://a.b", sessao),
                DestinoDoLink::Navegador("http://a.b".into())
            );
        }
        assert_eq!(
            rotulo_de("https://a.b", Kind::Ssh).as_deref(),
            Some("Abrir no navegador")
        );
        assert_eq!(d("https://", Kind::Local), DestinoDoLink::SoCopiar);
    }

    #[test]
    fn mailto_escreve_o_email() {
        assert_eq!(
            d("mailto:ana@a.b", Kind::Local),
            DestinoDoLink::Navegador("mailto:ana@a.b".into())
        );
        assert_eq!(
            rotulo_de("mailto:ana@a.b", Kind::Ssh).as_deref(),
            Some("Escrever e-mail")
        );
        assert_eq!(d("mailto:", Kind::Local), DestinoDoLink::SoCopiar);
    }

    #[test]
    fn o_esquema_nao_diferencia_maiusculas() {
        assert_eq!(
            d("HTTPS://A.B/C", Kind::Local),
            DestinoDoLink::Navegador("HTTPS://A.B/C".into())
        );
        assert_eq!(
            d("MailTo:ana@a.b", Kind::Local),
            DestinoDoLink::Navegador("MailTo:ana@a.b".into())
        );
        assert_eq!(
            d("FILE:///tmp/x", Kind::Local),
            DestinoDoLink::PastaLocal(PathBuf::from("/tmp/x"))
        );
    }

    #[test]
    fn o_file_local_leva_o_caminho_decodificado() {
        let esperado =
            DestinoDoLink::PastaLocal(PathBuf::from("/home/ana/meus docs/relatório.txt"));
        for host in [
            "",
            "localhost",
            "LOCALHOST",
            "Ana-PC",
            "ana-pc.casa.lan",
            "ANA-PC.outro",
        ] {
            assert_eq!(
                d(
                    &format!("file://{host}/home/ana/meus%20docs/relat%C3%B3rio.txt"),
                    Kind::Local
                ),
                esperado,
                "host {host:?}"
            );
        }
        assert_eq!(
            rotulo(&esperado).as_deref(),
            Some("Abrir a pasta"),
            "o rótulo diz pasta, porque é ela que abre"
        );
    }

    #[test]
    fn o_file_de_outro_host_num_shell_local_so_copia() {
        assert_eq!(
            d("file://servidor/tmp/x", Kind::Local),
            DestinoDoLink::SoCopiar
        );
        assert_eq!(rotulo_de("file://servidor/tmp/x", Kind::Local), None);
    }

    #[test]
    fn o_file_numa_sessao_ssh_abre_o_painel_de_qualquer_host() {
        for uri in [
            "file:///var/log/a%20b.log",
            "file://web-01/var/log/a%20b.log",
        ] {
            assert_eq!(
                d(uri, Kind::Ssh),
                DestinoDoLink::PainelRemoto("/var/log/a b.log".into()),
                "{uri}"
            );
        }
        assert_eq!(
            rotulo_de("file://web-01/var/log", Kind::Ssh).as_deref(),
            Some("Abrir no painel de arquivos")
        );
    }

    #[test]
    fn o_file_no_telnet_e_sem_conexao_de_arquivos_so_copia() {
        for sessao in [Kind::Telnet, Kind::Ftp, Kind::S3, Kind::Rclone] {
            assert_eq!(
                d("file:///tmp/x", sessao),
                DestinoDoLink::SoCopiar,
                "{sessao:?}"
            );
        }
    }

    #[test]
    fn o_resto_so_copia() {
        for uri in [
            "javascript:alert(1)",
            "data:text/html,oi",
            "ssh://a.b",
            "ftp://a.b/x",
            "vnc://a.b",
            "sem-esquema",
            "file:/tmp/x",
            "file://so-host",
            "",
        ] {
            for sessao in [Kind::Local, Kind::Ssh] {
                assert_eq!(d(uri, sessao), DestinoDoLink::SoCopiar, "{uri:?}");
            }
        }
        assert_eq!(rotulo(&DestinoDoLink::SoCopiar), None);
    }

    #[test]
    fn caracteres_de_controle_no_endereco_so_copiam() {
        assert_eq!(
            d("https://a.b/\u{1b}[31m", Kind::Local),
            DestinoDoLink::SoCopiar
        );
        assert_eq!(d("file:///tmp/a\nb", Kind::Local), DestinoDoLink::SoCopiar);
    }

    #[test]
    fn o_percent_torto_e_o_que_nao_e_utf8_ficam_como_vieram() {
        assert_eq!(
            d("file:///tmp/100%/a%zzb%2", Kind::Ssh),
            DestinoDoLink::PainelRemoto("/tmp/100%/a%zzb%2".into())
        );
        assert_eq!(
            d("file:///tmp/%FF%FE", Kind::Ssh),
            DestinoDoLink::PainelRemoto("/tmp/%FF%FE".into())
        );
        // O `%23` é um `#` do nome; o `#` solto começa o fragmento.
        assert_eq!(
            d("file:///tmp/a%23b#linha", Kind::Ssh),
            DestinoDoLink::PainelRemoto("/tmp/a#b".into())
        );
    }

    #[test]
    fn a_pasta_do_link_e_a_pasta_ou_a_que_contem_o_arquivo() {
        let pasta = std::env::temp_dir().join(format!("tirech-link-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let arquivo = pasta.join("a.txt");
        std::fs::write(&arquivo, "x").unwrap();
        assert_eq!(pasta_do_link(&pasta), pasta);
        assert_eq!(pasta_do_link(&arquivo), pasta);
        assert_eq!(pasta_do_link(&pasta.join("não existe.txt")), pasta);
        assert_eq!(pasta_do_link(Path::new("/")), Path::new("/"));
        std::fs::remove_dir_all(&pasta).unwrap();
    }

    #[test]
    fn o_localhost_de_uma_sessao_ssh_e_uma_porta_do_servidor() {
        let porta =
            |host: &str, porta: u16, caminho: &str, https: bool| DestinoDoLink::PortaDoServidor {
                host: host.into(),
                porta,
                caminho: caminho.into(),
                https,
            };
        assert_eq!(
            d("http://localhost:3000/", Kind::Ssh),
            porta("localhost", 3000, "/", false)
        );
        assert_eq!(
            d("http://localhost:3000", Kind::Ssh),
            porta("localhost", 3000, "/", false)
        );
        assert_eq!(
            d("http://0.0.0.0:8080/a/b?x=1#f", Kind::Ssh),
            porta("127.0.0.1", 8080, "/a/b?x=1#f", false)
        );
        assert_eq!(
            d("http://127.0.0.2:81/", Kind::Ssh),
            porta("127.0.0.1", 81, "/", false)
        );
        assert_eq!(
            d("http://[::1]:9000/x", Kind::Ssh),
            porta("::1", 9000, "/x", false)
        );
        assert_eq!(d("http://[::1]/", Kind::Ssh), porta("::1", 80, "/", false));
        assert_eq!(
            d("HTTPS://LocalHost/", Kind::Ssh),
            porta("localhost", 443, "/", true)
        );
        assert_eq!(
            d("http://localhost?q=1", Kind::Ssh),
            porta("localhost", 80, "/?q=1", false)
        );
        assert_eq!(
            rotulo_de("http://localhost:3000/", Kind::Ssh).as_deref(),
            Some("Abrir pelo túnel")
        );
    }

    #[test]
    fn so_o_ssh_com_a_opcao_ligada_usa_o_tunel() {
        let uri = "http://localhost:3000/";
        for sessao in [Kind::Local, Kind::Telnet] {
            assert_eq!(d(uri, sessao), DestinoDoLink::Navegador(uri.into()));
        }
        assert_eq!(
            destino(uri, Kind::Ssh, MAQUINA, false),
            DestinoDoLink::Navegador(uri.into())
        );
    }

    #[test]
    fn outros_hosts_e_portas_invalidas_ficam_no_navegador() {
        for uri in [
            "http://exemplo.com:3000/",
            "http://localhost.exemplo.com/",
            "http://localhost:0/",
            "http://localhost:99999/",
            "http://localhost:abc/",
            "http://ana@localhost:3000/",
            "http://10.0.0.5:3000/",
        ] {
            assert_eq!(
                d(uri, Kind::Ssh),
                DestinoDoLink::Navegador(uri.into()),
                "{uri:?}"
            );
        }
    }
}
