//! Os túneis ativos de uma aba (M37) contra um servidor SSH local de verdade: a lista, o
//! "Parar" e o "Ligar".

#![cfg(unix)]

mod comum;
#[path = "comum/servidor.rs"]
mod servidor;

use std::net::TcpListener;
use std::rc::Rc;
use std::time::{Duration, Instant};

use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
use tirech_omni::modelo::{Connection, Kind, TipoTunel, Tree, Tunel};
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::Forward;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, montar_com};
use servidor::Servidor;

/// Abre a sessão SSH de verdade, com um túnel local `-L porta:127.0.0.1:9`.
struct AbreComTunel {
    servidor: Rc<Servidor>,
    porta: u16,
}

impl Opener for AbreComTunel {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        let builder = self
            .servidor
            .builder()
            .forward(Forward::local(self.porta, "127.0.0.1", 9));
        Session::new(builder.connect())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }
}

/// Uma porta que ninguém usa agora.
fn porta_livre() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("achar uma porta livre")
        .local_addr()
        .expect("endereço da porta")
        .port()
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: &dyn Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(20);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        std::thread::sleep(Duration::from_millis(30));
        app.frame();
    }
}

fn estado_do_primeiro_tunel(app: &TestApp) -> String {
    app.grid("ativos-lista")
        .rows
        .iter()
        .find(|r| r.key.starts_with('t'))
        .map(|r| r.cells[3].clone())
        .unwrap_or_default()
}

#[test]
fn a_lista_dos_ativos_mostra_o_tunel_e_para_e_liga_de_novo() {
    let pasta = Pasta::nova("tuneis-ativos");
    let servidor = Rc::new(Servidor::novo(&pasta.caminho().join("servidor")));
    let mut tree = Tree::default();
    let mut c = Connection::new("local", Kind::Ssh);
    c.host = "127.0.0.1".into();
    c.port = servidor.porta;
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, c);
    pasta.semear(&tree);
    let porta = porta_livre();

    let (mut app, _fila) = montar_com(
        &pasta,
        Theme::Light,
        AbreComTunel {
            servidor: Rc::clone(&servidor),
            porta,
        },
    );
    app.double_click("arvore-1");
    let chave = app.panes("paineis").panes[0].tabs[0].0.clone();
    app.context_menu(&format!("paineis-painel-0-{chave}"));
    app.click("tuneis-ativos-da-sessao");
    assert!(app.visible("ativos-dialogo"));

    esperar(&mut app, "o túnel escutar", &|a| {
        estado_do_primeiro_tunel(a) == "Escutando"
    });
    let linha = app
        .grid("ativos-lista")
        .rows
        .into_iter()
        .find(|r| r.key.starts_with('t'))
        .expect("a linha do túnel");
    assert_eq!(linha.cells[0], "Local");
    assert!(
        linha.cells[1].contains(&porta.to_string()),
        "{:?}",
        linha.cells
    );
    assert!(linha.cells[2].contains("127.0.0.1:9"), "{:?}", linha.cells);

    app.grid_select("ativos-lista", &[linha.key.as_str()]);
    assert!(app.enabled("ativos-parar"));
    assert!(!app.enabled("ativos-ligar"));
    app.click("ativos-parar");
    esperar(&mut app, "o túnel parar", &|a| {
        estado_do_primeiro_tunel(a) == "Parado"
    });
    assert!(app.enabled("ativos-ligar"));
    app.click("ativos-ligar");
    esperar(&mut app, "o túnel escutar de novo", &|a| {
        estado_do_primeiro_tunel(a) == "Escutando"
    });

    if let Some(s) = Session::find("1-1") {
        s.dispose();
    }
}

#[test]
fn a_porta_ocupada_avisa_que_o_tunel_nao_subiu() {
    let pasta = Pasta::nova("tuneis-ocupada");
    let servidor = Rc::new(Servidor::novo(&pasta.caminho().join("servidor")));
    let mut tree = Tree::default();
    let mut c = Connection::new("local", Kind::Ssh);
    c.host = "127.0.0.1".into();
    c.port = servidor.porta;
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    c.tuneis.push({
        let mut t = Tunel::novo(TipoTunel::Local, false);
        t.porta = 1;
        t.destino_host = "127.0.0.1".into();
        t.destino_porta = 9;
        t
    });
    tree.add_connection(None, c);
    pasta.semear(&tree);

    // Quem ocupa a porta é este teste; a sessão pede a mesma.
    let ocupante = TcpListener::bind("127.0.0.1:0").expect("ocupar uma porta");
    let porta = ocupante.local_addr().expect("endereço").port();
    let (mut app, _fila) = montar_com(
        &pasta,
        Theme::Light,
        AbreComTunel {
            servidor: Rc::clone(&servidor),
            porta,
        },
    );
    app.double_click("arvore-1");
    esperar(&mut app, "o aviso do túnel", &|a| {
        a.visible("aviso-atencao")
    });
    let texto = app.text("aviso-atencao");
    assert!(
        texto.contains(&format!("O túnel local {porta} não subiu")),
        "{texto}"
    );
    assert!(texto.contains("Túneis ativos"), "{texto}");
    drop(ocupante);
    if let Some(s) = Session::find("1-1") {
        s.dispose();
    }
}
