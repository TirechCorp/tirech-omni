//! O shell local pela rede do perfil: as variáveis de proxy que levam os programas do shell pelo
//! proxy local do perfil (o mesmo dos links) e os túneis locais e dinâmicos, que escutam aqui e
//! saem pela rede do perfil (ou direto, sem perfil). As VPNs embutidas rodam sem placa virtual,
//! então só passa por elas o programa que fala com um proxy ou com uma porta daqui.

use std::io;
use std::net::{Ipv4Addr, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::net::{CONNECT_TIMEOUT, HttpProxyServer, PortForward};

use crate::modelo::{TipoTunel, Tunel};
use crate::navegador::Proxies;
use crate::portas::local::{escolher_porta_local, porta_livre};
use crate::redes::RedeResolvida;
use crate::t;

/// O que o shell local leva da rede: as variáveis do ambiente, as linhas que o terminal mostra
/// antes do shell e as portas, que vivem enquanto a aba existir.
#[derive(Default)]
pub struct ShellNaRede {
    /// As variáveis de proxy, vazias sem perfil.
    pub ambiente: Vec<(&'static str, String)>,
    /// O que o terminal mostra: por onde o shell sai, os túneis abertos e os que falharam.
    pub avisos: Vec<String>,
    /// Os túneis abertos; param quando isto cai.
    pub portas: Vec<Porta>,
}

/// Um túnel do shell local aberto.
pub enum Porta {
    /// O túnel local: uma porta daqui que leva sempre ao mesmo destino (o host e a porta).
    Local(PortForward, String, u16),
    /// O túnel dinâmico: um proxy SOCKS5 e HTTP aqui.
    Dinamico(HttpProxyServer),
}

impl Porta {
    /// Onde a porta escuta.
    pub fn endereco(&self) -> std::net::SocketAddr {
        match self {
            Self::Local(p, ..) => p.addr(),
            Self::Dinamico(p) => p.addr(),
        }
    }

    /// Como o autocompletar enxerga a porta.
    pub fn vista(&self) -> crate::autocompletar::TunelVisto {
        crate::autocompletar::TunelVisto {
            escuta: self.endereco(),
            destino: match self {
                Self::Local(_, host, porta) => Some((host.clone(), *porta)),
                Self::Dinamico(_) => None,
            },
            aba: None,
        }
    }
}

/// As variáveis que levam os programas pelo proxy local da `porta`: o `ALL_PROXY` em SOCKS5 com
/// os nomes resolvidos do outro lado (`socks5h`), o `HTTP_PROXY` e o `HTTPS_PROXY` em HTTP, nas
/// duas grafias, e o `NO_PROXY` com o próprio computador.
pub fn ambiente_do_proxy(porta: u16) -> Vec<(&'static str, String)> {
    let socks = format!("socks5h://127.0.0.1:{porta}");
    let http = format!("http://127.0.0.1:{porta}");
    let fora = "localhost,127.0.0.1,::1".to_owned();
    vec![
        ("ALL_PROXY", socks.clone()),
        ("all_proxy", socks),
        ("HTTP_PROXY", http.clone()),
        ("http_proxy", http.clone()),
        ("HTTPS_PROXY", http.clone()),
        ("https_proxy", http),
        ("NO_PROXY", fora.clone()),
        ("no_proxy", fora),
    ]
}

/// Prepara o shell local que sai por `rede` (`None`: direto): liga o proxy local do perfil em
/// `proxies` (o mesmo dos links, com a porta guardada em `pasta_de_dados`) e abre os `tuneis`
/// ligados. Um perfil só com salto SSH não tem proxy: o shell sai direto e os túneis não sobem,
/// para não chegar ao endereço errado.
pub fn preparar(
    proxies: &Proxies,
    pasta_de_dados: &Path,
    tuneis: &[Tunel],
    rede: Option<RedeResolvida>,
) -> ShellNaRede {
    let mut shell = ShellNaRede::default();
    let ligados: Vec<&Tunel> = tuneis.iter().filter(|t| t.ligado).collect();
    let discagem = match rede {
        None => Some(Discagem::Direta),
        Some(rede) if rede.perfil.proxy.is_none() => {
            shell.avisos.push(t!(
                "shell-rede-sem-proxy",
                perfil = rede.perfil.name.as_str()
            ));
            None
        }
        Some(rede) => {
            let perfil = rede.perfil.name.clone();
            if let Some(erro) = rede.erro_do_motor() {
                shell.avisos.push(t!(
                    "shell-rede-motor-falhou",
                    perfil = perfil.as_str(),
                    erro = erro
                ));
            }
            match proxies.porta(pasta_de_dados, rede.perfil.id, rede.clone()) {
                Ok(porta) => {
                    shell.ambiente = ambiente_do_proxy(porta);
                    shell.avisos.push(t!(
                        "shell-rede-saindo",
                        perfil = perfil.as_str(),
                        endereco = format!("127.0.0.1:{porta}")
                    ));
                }
                Err(erro) => shell.avisos.push(t!(
                    "shell-rede-proxy-falhou",
                    perfil = perfil.as_str(),
                    erro = erro.to_string()
                )),
            }
            Some(Discagem::Perfil(Arc::new(Mutex::new(rede))))
        }
    };
    let Some(discagem) = discagem else {
        return shell;
    };
    for tunel in ligados {
        match abrir_tunel(tunel, &discagem) {
            Ok(porta) => {
                shell.avisos.push(t!(
                    "shell-rede-tunel-aberto",
                    endereco = porta.endereco().to_string(),
                    destino = tunel.destino()
                ));
                shell.portas.push(porta);
            }
            Err(erro) => shell.avisos.push(t!(
                "shell-rede-tunel-falhou",
                tunel = tunel.titulo(),
                erro = erro
            )),
        }
    }
    shell
}

/// Por onde os túneis do shell discam.
#[derive(Clone)]
enum Discagem {
    Direta,
    /// Pelo proxy do perfil, depois de esperar a VPN embutida subir.
    Perfil(Arc<Mutex<RedeResolvida>>),
}

impl Discagem {
    fn discar(&self, host: &str, porta: u16) -> io::Result<TcpStream> {
        match self {
            Self::Direta => tomni_ui::net::connect(host, porta, None, CONNECT_TIMEOUT),
            Self::Perfil(rede) => {
                let rede = rede.lock().unwrap_or_else(PoisonError::into_inner).clone();
                rede.preparar(&mut |_| {}).map_err(io::Error::other)?;
                tomni_ui::net::connect(host, porta, rede.proxy().as_ref(), CONNECT_TIMEOUT)
            }
        }
    }
}

/// Abre um túnel do shell: o local e o dinâmico; os outros tipos precisam de um servidor SSH.
fn abrir_tunel(tunel: &Tunel, discagem: &Discagem) -> Result<Porta, String> {
    if !matches!(tunel.tipo, TipoTunel::Local | TipoTunel::Dinamico) {
        return Err(t!("shell-rede-tunel-tipo"));
    }
    if let Some(problema) = tunel.problema() {
        return Err(problema);
    }
    let porta = if tunel.proxima_livre {
        escolher_porta_local(tunel.porta, porta_livre).unwrap_or(tunel.porta)
    } else {
        tunel.porta
    };
    let ip = if tunel.outras_maquinas {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::LOCALHOST
    };
    let discagem = discagem.clone();
    let discar = move |host: &str, porta: u16| discagem.discar(host, porta);
    let aberta = match tunel.tipo {
        TipoTunel::Dinamico => HttpProxyServer::bind((ip, porta), discar).map(Porta::Dinamico),
        _ => {
            let host = tunel.destino_host.trim();
            PortForward::bind((ip, porta), host, tunel.destino_porta, discar)
                .map(|p| Porta::Local(p, host.to_owned(), tunel.destino_porta))
        }
    };
    aberta.map_err(|e| tomni_ui::net::error_message(&e))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;
    use crate::redes::{PerfilRede, ProxyRede, TipoProxy};

    fn tunel(tipo: TipoTunel, porta: u16, destino: u16) -> Tunel {
        let mut t = Tunel::novo(tipo, false);
        t.porta = porta;
        t.destino_host = "127.0.0.1".into();
        t.destino_porta = destino;
        t.ligado = true;
        t
    }

    /// Um servidor que responde "eco:" e o que recebeu, uma vez por conexão.
    fn eco() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut c in l.incoming().flatten() {
                let mut buf = [0; 64];
                let n = c.read(&mut buf).unwrap_or(0);
                let _ = c.write_all(b"eco:");
                let _ = c.write_all(&buf[..n]);
            }
        });
        porta
    }

    fn pasta_temporaria(nome: &str) -> std::path::PathBuf {
        let pasta =
            std::env::temp_dir().join(format!("tirech-shell-rede-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        pasta
    }

    fn falar(porta: u16, texto: &[u8]) -> Vec<u8> {
        let mut c = TcpStream::connect(("127.0.0.1", porta)).unwrap();
        c.write_all(texto).unwrap();
        c.shutdown(std::net::Shutdown::Write).unwrap();
        let mut volta = Vec::new();
        c.read_to_end(&mut volta).unwrap();
        volta
    }

    #[test]
    fn o_ambiente_leva_o_socks_com_nomes_do_outro_lado_e_o_http_nas_duas_grafias() {
        let ambiente = ambiente_do_proxy(13080);
        let valor = |nome: &str| {
            ambiente
                .iter()
                .find(|(n, _)| *n == nome)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(valor("ALL_PROXY"), Some("socks5h://127.0.0.1:13080"));
        assert_eq!(valor("all_proxy"), Some("socks5h://127.0.0.1:13080"));
        assert_eq!(valor("https_proxy"), Some("http://127.0.0.1:13080"));
        assert_eq!(valor("HTTP_PROXY"), Some("http://127.0.0.1:13080"));
        assert_eq!(valor("no_proxy"), Some("localhost,127.0.0.1,::1"));
    }

    #[test]
    fn sem_perfil_o_tunel_local_sai_direto_e_nao_ha_proxy_no_ambiente() {
        let destino = eco();
        let pasta = pasta_temporaria("direto");
        let shell = preparar(
            &Proxies::default(),
            &pasta,
            &[tunel(TipoTunel::Local, 0, destino)],
            None,
        );
        assert!(shell.ambiente.is_empty());
        // Porta 0 é um problema do túnel: não sobe e diz por quê.
        assert!(shell.portas.is_empty());
        assert_eq!(shell.avisos.len(), 1);

        let mut t = tunel(TipoTunel::Local, 1, destino);
        t.proxima_livre = true;
        let shell = preparar(&Proxies::default(), &pasta, &[t], None);
        let porta = shell.portas[0].endereco().port();
        assert!(shell.portas[0].endereco().ip().is_loopback());
        assert_eq!(falar(porta, b"oi"), b"eco:oi");
    }

    #[test]
    fn o_perfil_leva_o_tunel_e_o_ambiente_pelo_proxy_dele() {
        let destino = eco();
        // O "proxy do perfil" é um proxy local de teste que anota cada destino pedido.
        let pedidos = Arc::new(Mutex::new(Vec::new()));
        let anotados = Arc::clone(&pedidos);
        let proxy_do_perfil = HttpProxyServer::start(move |host: &str, porta| {
            anotados.lock().unwrap().push(format!("{host}:{porta}"));
            TcpStream::connect(("127.0.0.1", destino))
        })
        .unwrap();
        let mut perfil = PerfilRede::new("Cliente A");
        perfil.proxy = Some(ProxyRede {
            tipo: TipoProxy::Socks5,
            host: "127.0.0.1".into(),
            port: proxy_do_perfil.addr().port(),
            credencial: None,
        });
        let pasta = pasta_temporaria("perfil");
        let mut t = tunel(TipoTunel::Local, 1, 3389);
        t.destino_host = "10.0.0.5".into();
        t.proxima_livre = true;
        let shell = preparar(
            &Proxies::default(),
            &pasta,
            &[t, tunel(TipoTunel::Remoto, 2222, 22)],
            Some(RedeResolvida::sem_segredos(perfil)),
        );
        assert_eq!(shell.ambiente.len(), 8);
        assert_eq!(shell.portas.len(), 1, "{:?}", shell.avisos);
        // Saindo, o túnel aberto e o remoto recusado.
        assert_eq!(shell.avisos.len(), 3, "{:?}", shell.avisos);
        let porta = shell.portas[0].endereco().port();
        assert_eq!(falar(porta, b"rdp"), b"eco:rdp");
        assert_eq!(*pedidos.lock().unwrap(), ["10.0.0.5:3389"]);
    }

    #[test]
    fn o_perfil_so_com_salto_nao_abre_os_tuneis() {
        let pasta = pasta_temporaria("salto");
        let mut t = tunel(TipoTunel::Dinamico, 1, 0);
        t.proxima_livre = true;
        let shell = preparar(
            &Proxies::default(),
            &pasta,
            &[t],
            Some(RedeResolvida::sem_segredos(PerfilRede::new("Só salto"))),
        );
        assert!(shell.ambiente.is_empty());
        assert!(shell.portas.is_empty());
        assert_eq!(shell.avisos.len(), 1);
    }
}
