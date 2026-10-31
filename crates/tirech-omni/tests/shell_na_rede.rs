//! O shell local pela rede do perfil, com o `RealOpener` e um `sh` de verdade: o aviso de por
//! onde o shell sai, as variáveis de proxy no ambiente e o túnel local que leva ao destino pelo
//! proxy do perfil.

#![cfg(unix)]

mod comum;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tirech_omni::abrir::RealOpener;
use tirech_omni::modelo::{Connection, Kind, TipoTunel, Tree, Tunel};
use tirech_omni::redes::{PerfilRede, ProxyRede, Rede, TipoProxy};
use tirech_omni::tela::tela_com_ssh;
use tomni_ui::net::HttpProxyServer;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::Pasta;

fn esperar_tela(app: &mut TestApp, chave: &str, o_que: &str) -> String {
    let limite = Instant::now() + Duration::from_secs(15);
    loop {
        let texto = app.session_text(chave);
        if texto.contains(o_que) {
            return texto;
        }
        assert!(
            Instant::now() < limite,
            "não apareceu {o_que:?} em 15 s:\n{texto}"
        );
        app.wait(Duration::from_millis(20));
    }
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

#[test]
fn o_shell_local_com_perfil_tem_o_proxy_no_ambiente_e_o_tunel_pelo_perfil() {
    let destino = eco();
    // O proxy do perfil (no lugar da VPN): anota o destino pedido e leva sempre ao eco.
    let pedidos = Arc::new(Mutex::new(Vec::new()));
    let anotados = Arc::clone(&pedidos);
    let proxy_do_perfil = HttpProxyServer::start(move |host: &str, porta| {
        anotados.lock().unwrap().push(format!("{host}:{porta}"));
        TcpStream::connect(("127.0.0.1", destino))
    })
    .unwrap();

    let pasta = Pasta::nova("shell-na-rede");
    let mut tree = Tree::default();
    let mut perfil = PerfilRede::new("Cliente A");
    perfil.proxy = Some(ProxyRede {
        tipo: TipoProxy::Socks5,
        host: "127.0.0.1".into(),
        port: proxy_do_perfil.addr().port(),
        credencial: None,
    });
    let perfil = tree.add_perfil(perfil);
    let mut c = Connection::new("Shell", Kind::Local);
    c.local_programa = "sh".into();
    c.rede = Rede::Perfil(perfil);
    let mut rdp = Tunel::novo(TipoTunel::Local, false);
    rdp.porta = 13389;
    rdp.proxima_livre = true;
    rdp.destino_host = "10.0.0.5".into();
    rdp.destino_porta = 3389;
    c.tuneis.push(rdp);
    let id = tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.pagina_inicial_padrao("terminais");
    pasta.agente_padrao(false);
    let (caminho, ssh) = (pasta.0.clone(), pasta.ssh());
    let mut app = TestApp::new(move || {
        tela_com_ssh(caminho, ssh, |fila| Rc::new(RealOpener::new(fila, None)))
    })
    .size(1280, 720)
    .theme(Theme::Light);

    app.double_click(&format!("arvore-{id}"));
    let chave = format!("{id}-1");
    let tela = esperar_tela(&mut app, &chave, "→ 10.0.0.5:3389");
    assert!(tela.contains("Cliente A"), "{tela}");
    let porta: u16 = tela
        .lines()
        .find_map(|l| {
            let resto = l.split("Túnel 127.0.0.1:").nth(1)?;
            resto.split_whitespace().next()?.parse().ok()
        })
        .unwrap_or_else(|| panic!("sem a porta do túnel:\n{tela}"));

    Session::find(&chave)
        .expect("a sessão da aba")
        .send(b"echo \"proxy=$ALL_PROXY https=$https_proxy\"\n");
    let tela = esperar_tela(&mut app, &chave, "proxy=socks5h://127.0.0.1:");
    assert!(tela.contains("https=http://127.0.0.1:"), "{tela}");

    let mut rdp = TcpStream::connect(("127.0.0.1", porta)).unwrap();
    rdp.write_all(b"oi").unwrap();
    rdp.shutdown(std::net::Shutdown::Write).unwrap();
    let mut volta = Vec::new();
    rdp.read_to_end(&mut volta).unwrap();
    assert_eq!(volta, b"eco:oi");
    assert_eq!(*pedidos.lock().unwrap(), ["10.0.0.5:3389"]);
}
