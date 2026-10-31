//! O autocompletar do terminal (Ctrl+Espaço): o Flyout com os comandos que cabem na aba, o
//! filtro e o Enter que põe o comando na linha sem executar. Um teste usa o `Loopback` (os
//! comandos ao conectar e os escolhidos); o outro, o `RealOpener` com um `sh` e o túnel local do
//! shell pela rede do perfil (o comando do cliente de área de trabalho remota na porta de verdade).

mod comum;

use std::net::TcpStream;
use std::rc::Rc;
use std::time::{Duration, Instant};

use tirech_omni::modelo::{
    ComandosAoConectar, Connection, Espera, Kind, ModoComandos, Passo, TipoTunel, Tree, Tunel,
};
use tomni_ui::Shortcut;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

fn atalho() -> Shortcut {
    "Ctrl+Space".parse().expect("o atalho do catálogo")
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&mut TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(15);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// As linhas da lista: (comando, origem).
fn linhas(app: &TestApp) -> Vec<(String, String)> {
    app.grid("autocompletar-lista")
        .rows
        .into_iter()
        .map(|r| (r.cells[0].clone(), r.cells[1].clone()))
        .collect()
}

fn aba(m: &Montagem) -> String {
    m.app.panes("paineis").panes[0].tabs[0].0.clone()
}

fn escrito(m: &Montagem) -> String {
    let sessao = tomni_ui::terminal::Session::find(&aba(m)).expect("a sessão da aba");
    let handle = m.opener.handle_de(sessao).expect("a ponta da sessão");
    String::from_utf8_lossy(&handle.take_written()).into_owned()
}

#[test]
fn os_comandos_ao_conectar_e_os_escolhidos_entram_e_o_enter_insere_sem_executar() {
    let pasta = Pasta::nova("autocompletar-comandos");
    let mut tree = Tree::default();
    let mut c = Connection::new("Shell", Kind::Local);
    let mut passo = Passo::comando("cd /srv/aplicacao-do-teste", 5);
    passo.espera = Espera::Logo;
    c.comandos = ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos: vec![passo],
        reconectar: true,
    };
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.configurar("conexoes", "comandos_quieto_ms", serde_json::json!(50));
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o comando ao conectar", |a| {
        let chave = a.panes("paineis").panes[0].tabs[0].0.clone();
        a.session_text(&chave).contains("aplicacao-do-teste")
    });
    escrito(&m);

    let terminal = format!("paineis-terminal-{}", aba(&m));
    m.app.click(&terminal);
    m.app.shortcut(atalho());
    assert!(m.app.visible("autocompletar"), "o Flyout não abriu");
    m.app.type_into("autocompletar-filtro", "SRV aplicacao-do");
    assert_eq!(
        linhas(&m.app)[0],
        (
            "cd /srv/aplicacao-do-teste".to_owned(),
            "Comandos ao conectar".to_owned()
        )
    );
    m.app.key(tomni_ui::Key::Enter);
    assert!(!m.app.visible("autocompletar"), "o Enter não fechou");
    assert_eq!(
        escrito(&m),
        "cd /srv/aplicacao-do-teste",
        "sem Enter no fim"
    );

    // Na próxima vez, ele vem como usado aqui, na frente das outras fontes.
    m.app.shortcut(atalho());
    m.app
        .type_into("autocompletar-filtro", "aplicacao-do-teste");
    assert_eq!(linhas(&m.app)[0].1, "Usado nesta conexão");
    m.app
        .type_into("autocompletar-filtro", "nada-casa-com-isto-zz");
    assert!(linhas(&m.app).is_empty());
    m.app.key(tomni_ui::Key::Escape);
    assert!(!m.app.visible("autocompletar"));
    assert_eq!(escrito(&m), "", "o Esc não insere nada");
}

#[cfg(unix)]
#[test]
fn o_tunel_do_shell_local_sugere_o_rdp_na_porta_de_verdade() {
    use tirech_omni::abrir::RealOpener;
    use tirech_omni::redes::{PerfilRede, ProxyRede, Rede, TipoProxy};
    use tirech_omni::tela::tela_com_ssh;
    use tomni_ui::net::HttpProxyServer;

    let proxy_do_perfil = HttpProxyServer::start(|_: &str, _| {
        TcpStream::connect(("127.0.0.1", 9)).map_err(std::io::Error::other)
    })
    .unwrap();
    let pasta = Pasta::nova("autocompletar-tunel");
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
    esperar(&mut app, "o aviso do túnel", |a| {
        a.session_text(&chave).contains("→ 10.0.0.5:3389")
    });
    let tela = app.session_text(&chave);
    let porta: u16 = tela
        .lines()
        .find_map(|l| {
            let resto = l.split("Túnel 127.0.0.1:").nth(1)?;
            resto.split_whitespace().next()?.parse().ok()
        })
        .unwrap_or_else(|| panic!("sem a porta do túnel:\n{tela}"));

    app.click(&format!("paineis-terminal-{chave}"));
    app.shortcut(atalho());
    assert!(app.visible("autocompletar"), "o Flyout não abriu");
    let (comando, origem) = linhas(&app)[0].clone();
    assert!(
        comando.contains(&format!("/v:127.0.0.1:{porta} ")) && comando.contains("freerdp"),
        "{comando}"
    );
    assert!(origem.contains("10.0.0.5:3389"), "{origem}");

    // ↓ leva à lista, e o Enter dela insere.
    app.key(tomni_ui::Key::Down);
    app.key(tomni_ui::Key::Enter);
    assert!(!app.visible("autocompletar"));
    esperar(&mut app, "o comando na linha", |a| {
        a.session_text(&chave)
            .contains(&format!("/v:127.0.0.1:{porta}"))
    });
}
