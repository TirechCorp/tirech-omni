//! Os links com perfil de rede: cada perfil ganha um proxy HTTP no `127.0.0.1`
//! ([`tomni_ui::net::HttpProxyServer`]) que leva cada pedido pela rede do perfil (o proxy dele
//! ou a VPN embutida), e um navegador só dele, com a pasta de perfil própria
//! (`<dados>/navegador/<perfil>/`), apontado para esse proxy. As páginas de um cliente não se
//! misturam com as de outro nem com o navegador do dia a dia.
//!
//! A porta de cada perfil fica guardada em `navegador/portas.json` e volta a mesma: o Chromium
//! com a mesma pasta de perfil entrega o endereço à janela que já está aberta, que segue com o
//! `--proxy-server` da primeira vez.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::net::{CONNECT_TIMEOUT, HttpProxyServer};

use crate::redes::RedeResolvida;
use crate::t;

/// De que família é o navegador: cada uma recebe o proxy e a pasta de perfil de um jeito.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Familia {
    /// Chrome, Edge, Brave, Chromium, Vivaldi: `--user-data-dir` e `--proxy-server`.
    Chromium,
    /// Firefox e derivados: `--profile` e o proxy no `user.js` da pasta.
    Firefox,
}

/// O navegador dos links com perfil de rede.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Navegador {
    /// O executável.
    pub programa: PathBuf,
    /// A família, pelo nome do executável.
    pub familia: Familia,
}

impl Navegador {
    /// O navegador de `programa`, com a família pelo nome do arquivo.
    pub fn de(programa: PathBuf) -> Self {
        let familia = familia_de(&programa);
        Self { programa, familia }
    }
}

/// A família pelo nome do executável: os derivados do Firefox pelo nome; o resto é Chromium.
pub fn familia_de(programa: &Path) -> Familia {
    let nome = programa
        .file_stem()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if ["firefox", "librewolf", "waterfox", "floorp", "zen"]
        .iter()
        .any(|f| nome.contains(f))
    {
        Familia::Firefox
    } else {
        Familia::Chromium
    }
}

/// Os nomes procurados no `PATH`, na ordem (Linux e outros Unix).
const NO_PATH: [&str; 10] = [
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "brave-browser",
    "brave",
    "microsoft-edge",
    "microsoft-edge-stable",
    "vivaldi",
    "firefox",
];

/// Os executáveis do macOS, na ordem.
const NO_MAC: [&str; 5] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Firefox.app/Contents/MacOS/firefox",
];

/// Os executáveis do Windows dentro do `ProgramFiles`, do `ProgramFiles(x86)` e do
/// `LOCALAPPDATA`, na ordem.
const NO_WINDOWS: [&str; 4] = [
    r"Google\Chrome\Application\chrome.exe",
    r"Microsoft\Edge\Application\msedge.exe",
    r"BraveSoftware\Brave-Browser\Application\brave.exe",
    r"Mozilla Firefox\firefox.exe",
];

/// O navegador a usar: o `configurado` (caminho ou nome no `PATH`) ou, vazio, o primeiro Chrome,
/// Edge, Brave, Chromium ou Firefox que achar. A mensagem diz o que faltou.
pub fn achar(configurado: &str) -> Result<Navegador, String> {
    let configurado = configurado.trim();
    if !configurado.is_empty() {
        let caminho = Path::new(configurado);
        if caminho.is_file() {
            return Ok(Navegador::de(caminho.to_owned()));
        }
        return no_path(configurado)
            .map(Navegador::de)
            .ok_or_else(|| t!("navegador-nao-achou", programa = configurado));
    }
    candidatos()
        .into_iter()
        .find(|c| c.is_file())
        .map(Navegador::de)
        .ok_or_else(|| t!("navegador-nenhum"))
}

/// Onde procurar, na ordem, neste sistema.
fn candidatos() -> Vec<PathBuf> {
    if cfg!(windows) {
        ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .iter()
            .filter_map(std::env::var_os)
            .flat_map(|base| NO_WINDOWS.iter().map(move |rel| Path::new(&base).join(rel)))
            .collect()
    } else if cfg!(target_os = "macos") {
        NO_MAC.iter().map(PathBuf::from).collect()
    } else {
        NO_PATH.iter().filter_map(|nome| no_path(nome)).collect()
    }
}

/// `programa` numa das pastas do `PATH` (no Windows, também com `.exe`).
pub fn no_path(programa: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|pasta| {
        let exato = pasta.join(programa);
        if exato.is_file() {
            return Some(exato);
        }
        let exe = pasta.join(format!("{programa}.exe"));
        (cfg!(windows) && exe.is_file()).then_some(exe)
    })
}

/// O endereço a abrir de uma conexão Link: sem esquema, ganha `http://`; só `http` e `https`
/// abrem. `None` se não abre (vazio, com espaço ou de outro esquema).
pub fn endereco_do_link(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() || url.chars().any(char::is_whitespace) {
        return None;
    }
    match url.split_once("://") {
        Some((esquema, resto)) => (["http", "https"]
            .contains(&esquema.to_ascii_lowercase().as_str())
            && !resto.is_empty())
        .then(|| url.to_owned()),
        None => {
            // `mailto:`, `javascript:` e afins: o que vem depois dos dois-pontos não é porta.
            let host = url.split('/').next().unwrap_or_default();
            let porta_ok = host
                .rsplit_once(':')
                .is_none_or(|(_, p)| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
            porta_ok.then(|| format!("http://{url}"))
        }
    }
}

/// A pasta de perfil do navegador do perfil de rede `perfil`.
pub fn pasta_do_perfil(pasta_de_dados: &Path, perfil: u64) -> PathBuf {
    pasta_de_dados.join("navegador").join(perfil.to_string())
}

/// O `user.js` do Firefox: o proxy HTTP para tudo (inclusive o HTTPS) e sem a pergunta do
/// navegador padrão.
pub fn user_js(porta: u16) -> String {
    format!(
        "// Gravado pelo Tirech Omni a cada abertura: o proxy é o do perfil de rede.\n\
         user_pref(\"network.proxy.type\", 1);\n\
         user_pref(\"network.proxy.http\", \"127.0.0.1\");\n\
         user_pref(\"network.proxy.http_port\", {porta});\n\
         user_pref(\"network.proxy.ssl\", \"127.0.0.1\");\n\
         user_pref(\"network.proxy.ssl_port\", {porta});\n\
         user_pref(\"network.proxy.share_proxy_settings\", true);\n\
         user_pref(\"network.proxy.no_proxies_on\", \"\");\n\
         user_pref(\"network.trr.mode\", 5);\n\
         user_pref(\"browser.shell.checkDefaultBrowser\", false);\n\
         user_pref(\"browser.aboutwelcome.enabled\", false);\n"
    )
}

/// Os argumentos que abrem `url` no navegador do perfil, com a pasta de perfil `pasta` e o proxy
/// na `porta`. Puro: o `user.js` do Firefox é gravado por quem lança ([`user_js`]).
pub fn argumentos(nav: &Navegador, pasta: &Path, porta: u16, url: &str) -> Vec<OsString> {
    match nav.familia {
        Familia::Chromium => {
            let mut dir = OsString::from("--user-data-dir=");
            dir.push(pasta);
            vec![
                dir,
                format!("--proxy-server=http://127.0.0.1:{porta}").into(),
                "--no-first-run".into(),
                "--no-default-browser-check".into(),
                url.into(),
            ]
        }
        // Sem `--no-remote`: a segunda abertura com a mesma pasta vai à janela que já está
        // aberta.
        Familia::Firefox => vec!["--profile".into(), pasta.into(), url.into()],
    }
}

/// Um proxy ligado: o servidor e a rede que ele usa, trocada a cada abertura (o motor da VPN
/// pode ter subido de novo).
struct Ligado {
    rede: Arc<Mutex<RedeResolvida>>,
    servidor: HttpProxyServer,
}

/// Os proxies dos perfis de rede, um por perfil, ligados na primeira abertura de um link do
/// perfil e desligados com o app.
#[derive(Default)]
pub struct Proxies {
    ligados: Mutex<HashMap<u64, Ligado>>,
}

impl Proxies {
    /// A porta do proxy do perfil `perfil`, que sai por `rede`. Na primeira vez liga o servidor,
    /// na porta guardada em `<dados>/navegador/portas.json` se ela estiver livre; nas outras,
    /// só troca a rede.
    pub fn porta(
        &self,
        pasta_de_dados: &Path,
        perfil: u64,
        rede: RedeResolvida,
    ) -> io::Result<u16> {
        let mut ligados = self.ligados.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(ligado) = ligados.get(&perfil) {
            *ligado.rede.lock().unwrap_or_else(PoisonError::into_inner) = rede;
            return Ok(ligado.servidor.addr().port());
        }
        let rede = Arc::new(Mutex::new(rede));
        let discar = {
            let rede = Arc::clone(&rede);
            move |host: &str, porta: u16| {
                let rede = rede.lock().unwrap_or_else(PoisonError::into_inner).clone();
                rede.preparar(&mut |_| {}).map_err(io::Error::other)?;
                tomni_ui::net::connect(host, porta, rede.proxy().as_ref(), CONNECT_TIMEOUT)
            }
        };
        let arquivo = pasta_de_dados.join("navegador").join("portas.json");
        let mut portas = ler_portas(&arquivo);
        let guardada = portas.get(&perfil.to_string()).copied();
        let servidor = match guardada {
            Some(p) => HttpProxyServer::bind(("127.0.0.1", p), discar.clone())
                .or_else(|_| HttpProxyServer::start(discar))?,
            None => HttpProxyServer::start(discar)?,
        };
        let porta = servidor.addr().port();
        if guardada != Some(porta) && !crate::armazenamento::so_leitura(pasta_de_dados) {
            portas.insert(perfil.to_string(), porta);
            // Sem a porta guardada, a próxima vez escolhe outra: o link ainda abre.
            let _ = gravar_portas(&arquivo, &portas);
        }
        ligados.insert(perfil, Ligado { rede, servidor });
        Ok(porta)
    }

    /// Quantos proxies estão ligados.
    pub fn quantos(&self) -> usize {
        self.ligados
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

fn ler_portas(arquivo: &Path) -> BTreeMap<String, u16> {
    std::fs::read(arquivo)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn gravar_portas(arquivo: &Path, portas: &BTreeMap<String, u16>) -> io::Result<()> {
    if let Some(pasta) = arquivo.parent() {
        std::fs::create_dir_all(pasta)?;
    }
    let texto = serde_json::to_vec_pretty(portas).map_err(io::Error::other)?;
    crate::armazenamento::gravar_atomico(arquivo, &texto, false)
}

/// Abre `url` no navegador do perfil `perfil`, que sai por `rede`: liga o proxy do perfil,
/// prepara a pasta de perfil e lança o `navegador` configurado (vazio: o primeiro que achar).
/// Volta na hora; a VPN sobe no primeiro pedido do navegador.
pub fn abrir(
    proxies: &Proxies,
    pasta_de_dados: &Path,
    navegador: &str,
    perfil: u64,
    rede: RedeResolvida,
    url: &str,
) -> Result<(), String> {
    if rede.perfil.proxy.is_none() {
        return Err(t!(
            "navegador-perfil-sem-proxy",
            perfil = rede.perfil.name.as_str()
        ));
    }
    let nav = achar(navegador)?;
    let porta = proxies
        .porta(pasta_de_dados, perfil, rede)
        .map_err(|e| t!("navegador-proxy-falhou", erro = e.to_string()))?;
    let pasta = pasta_do_perfil(pasta_de_dados, perfil);
    let preparar = || -> io::Result<()> {
        std::fs::create_dir_all(&pasta)?;
        if nav.familia == Familia::Firefox {
            std::fs::write(pasta.join("user.js"), user_js(porta))?;
        }
        Ok(())
    };
    preparar().map_err(|e| t!("navegador-pasta-falhou", erro = e.to_string()))?;
    let mut comando = std::process::Command::new(&nav.programa);
    crate::sistema::sem_janela(&mut comando);
    let mut filho = comando
        .args(argumentos(&nav, &pasta, porta, url))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| {
            t!(
                "navegador-nao-abriu",
                programa = nav.programa.display().to_string(),
                erro = e.to_string()
            )
        })?;
    // O navegador que entrega o endereço à janela aberta sai logo: alguém precisa recolhê-lo.
    std::thread::spawn(move || {
        let _ = filho.wait();
    });
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::redes::{PerfilRede, ProxyRede, TipoProxy};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn o_endereco_do_link_ganha_http_e_recusa_outros_esquemas() {
        assert_eq!(
            endereco_do_link(" intranet.cliente/painel "),
            Some("http://intranet.cliente/painel".to_owned())
        );
        assert_eq!(
            endereco_do_link("HTTPS://10.0.0.5:8443/"),
            Some("HTTPS://10.0.0.5:8443/".to_owned())
        );
        assert_eq!(endereco_do_link("file:///etc/passwd"), None);
        assert_eq!(
            endereco_do_link("10.0.0.5:8080/x"),
            Some("http://10.0.0.5:8080/x".to_owned())
        );
        assert_eq!(endereco_do_link("javascript:alert(1)"), None);
        assert_eq!(endereco_do_link("mailto:ana@cliente"), None);
        assert_eq!(endereco_do_link(""), None);
        assert_eq!(endereco_do_link("https://"), None);
        assert_eq!(endereco_do_link("a b"), None);
    }

    fn pasta_temporaria(nome: &str) -> PathBuf {
        let pasta =
            std::env::temp_dir().join(format!("tirech-navegador-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        pasta
    }

    #[test]
    fn a_familia_vem_do_nome_do_executavel() {
        for (programa, familia) in [
            ("/usr/bin/google-chrome", Familia::Chromium),
            ("/usr/bin/chromium", Familia::Chromium),
            (
                r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
                Familia::Chromium,
            ),
            ("/usr/bin/brave-browser", Familia::Chromium),
            ("/usr/bin/firefox", Familia::Firefox),
            ("/usr/bin/librewolf", Familia::Firefox),
            (
                r"C:\Program Files\Mozilla Firefox\firefox.exe",
                Familia::Firefox,
            ),
            (
                "/Applications/Firefox.app/Contents/MacOS/firefox",
                Familia::Firefox,
            ),
        ] {
            assert_eq!(familia_de(Path::new(programa)), familia, "{programa}");
        }
    }

    #[test]
    fn o_chromium_recebe_a_pasta_e_o_proxy_nos_argumentos() {
        let nav = Navegador::de(PathBuf::from("/usr/bin/chromium"));
        let args = argumentos(
            &nav,
            Path::new("/dados/navegador/7"),
            40123,
            "https://intra/",
        );
        assert_eq!(
            args,
            [
                "--user-data-dir=/dados/navegador/7",
                "--proxy-server=http://127.0.0.1:40123",
                "--no-first-run",
                "--no-default-browser-check",
                "https://intra/",
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn o_firefox_recebe_a_pasta_e_o_proxy_vai_no_user_js() {
        let nav = Navegador::de(PathBuf::from("/usr/bin/firefox"));
        let args = argumentos(
            &nav,
            Path::new("/dados/navegador/7"),
            40123,
            "https://intra/",
        );
        assert_eq!(
            args,
            ["--profile", "/dados/navegador/7", "https://intra/"].map(OsString::from)
        );
        let js = user_js(40123);
        assert!(js.contains("\"network.proxy.type\", 1"));
        assert!(js.contains("\"network.proxy.http_port\", 40123"));
        assert!(js.contains("\"network.proxy.ssl_port\", 40123"));
        assert!(js.contains("\"network.proxy.no_proxies_on\", \"\""));
    }

    #[test]
    fn o_navegador_configurado_que_nao_existe_diz_qual_faltou() {
        let erro = achar("navegador-que-nao-existe-xyz").unwrap_err();
        assert!(erro.contains("navegador-que-nao-existe-xyz"), "{erro}");
    }

    #[test]
    fn o_navegador_configurado_por_caminho_vale_mesmo_fora_do_path() {
        let pasta = pasta_temporaria("caminho");
        let programa = pasta.join("firefox");
        std::fs::write(&programa, "").unwrap();
        let nav = achar(programa.to_str().unwrap()).unwrap();
        assert_eq!(nav.programa, programa);
        assert_eq!(nav.familia, Familia::Firefox);
    }

    /// Um servidor que responde "ola" a quem conectar.
    fn destino() -> u16 {
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for conexao in ouvinte.incoming().flatten() {
                let mut c = conexao;
                let _ = c.write_all(b"ola");
            }
        });
        porta
    }

    /// Um proxy HTTP de mentira (só o `CONNECT`) na frente do destino: é a "rede do perfil".
    fn proxy_do_perfil() -> (u16, Arc<Mutex<Vec<String>>>) {
        let ouvinte = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = ouvinte.local_addr().unwrap().port();
        let pedidos = Arc::new(Mutex::new(Vec::new()));
        let anotados = Arc::clone(&pedidos);
        std::thread::spawn(move || {
            for conexao in ouvinte.incoming().flatten() {
                let anotados = Arc::clone(&anotados);
                std::thread::spawn(move || {
                    let mut c = conexao;
                    let mut cabecalho = Vec::new();
                    let mut byte = [0u8; 1];
                    while !cabecalho.ends_with(b"\r\n\r\n") {
                        if c.read(&mut byte).unwrap_or(0) == 0 {
                            return;
                        }
                        cabecalho.push(byte[0]);
                    }
                    let linha = String::from_utf8_lossy(&cabecalho);
                    let alvo = linha.split_whitespace().nth(1).unwrap_or("").to_owned();
                    anotados.lock().unwrap().push(alvo.clone());
                    let mut fim = TcpStream::connect(&alvo).unwrap();
                    c.write_all(b"HTTP/1.1 200 OK\r\n\r\n").unwrap();
                    let mut resposta = [0u8; 3];
                    fim.read_exact(&mut resposta).unwrap();
                    c.write_all(&resposta).unwrap();
                });
            }
        });
        (porta, pedidos)
    }

    fn rede_por(porta: u16) -> RedeResolvida {
        let mut perfil = PerfilRede::new("Cliente A");
        perfil.id = 3;
        perfil.proxy = Some(ProxyRede {
            tipo: TipoProxy::Http,
            port: porta,
            ..ProxyRede::novo()
        });
        RedeResolvida::sem_segredos(perfil)
    }

    /// Pede `CONNECT` ao proxy local e devolve o que veio do outro lado.
    fn pedir(porta_local: u16, alvo: u16) -> String {
        let mut c = TcpStream::connect(("127.0.0.1", porta_local)).unwrap();
        write!(
            c,
            "CONNECT 127.0.0.1:{alvo} HTTP/1.1\r\nHost: 127.0.0.1:{alvo}\r\n\r\n"
        )
        .unwrap();
        let mut tudo = Vec::new();
        let _ = c.read_to_end(&mut tudo);
        String::from_utf8_lossy(&tudo).into_owned()
    }

    #[test]
    fn o_proxy_do_perfil_sai_pela_rede_dele_e_guarda_a_porta() {
        let dados = pasta_temporaria("proxy");
        let alvo = destino();
        let (porta_do_perfil, pedidos) = proxy_do_perfil();
        let proxies = Proxies::default();
        let porta = proxies
            .porta(dados.as_path(), 3, rede_por(porta_do_perfil))
            .unwrap();
        let resposta = pedir(porta, alvo);
        assert!(resposta.ends_with("ola"), "{resposta:?}");
        assert_eq!(*pedidos.lock().unwrap(), [format!("127.0.0.1:{alvo}")]);

        // A segunda abertura troca a rede e fica na mesma porta.
        assert_eq!(
            proxies
                .porta(dados.as_path(), 3, rede_por(porta_do_perfil))
                .unwrap(),
            porta
        );
        assert_eq!(proxies.quantos(), 1);
        let guardadas = ler_portas(&dados.as_path().join("navegador/portas.json"));
        assert_eq!(guardadas.get("3"), Some(&porta));

        // Com o app de novo (outro registro), a porta guardada volta.
        drop(proxies);
        let outra = Proxies::default();
        assert_eq!(
            outra
                .porta(dados.as_path(), 3, rede_por(porta_do_perfil))
                .unwrap(),
            porta
        );
    }

    #[test]
    fn o_perfil_so_com_salto_nao_abre_link() {
        let dados = pasta_temporaria("salto");
        let mut perfil = PerfilRede::new("Só salto");
        perfil.id = 4;
        let rede = RedeResolvida::sem_segredos(perfil);
        let erro = abrir(
            &Proxies::default(),
            dados.as_path(),
            "",
            4,
            rede,
            "https://intra/",
        )
        .unwrap_err();
        assert!(erro.contains("Só salto"), "{erro}");
    }
}
