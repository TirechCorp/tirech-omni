//! O mouse no terminal por conexão e por pasta: a linha "Mouse no terminal" escolhe entre copiar
//! e colar e o menu, e a aba aberta segue a escolha da conexão, a da pasta ou, sem
//! nenhuma, as Configurações. O clique direito sem Ctrl mostra qual vale: abre o menu ou não.

mod comum;

use std::time::{Duration, Instant};

use serde_json::json;

use tirech_omni::modelo::{Connection, Kind, MouseDoTerminal, Node, Tree};
use tomni_ui::Key;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, montar, valor};

const LINHA: &str = "Mouse no terminal";
const TERMINAL: &str = "paineis-terminal-2-1";

/// A pasta `Clientes` (id 1) com o mouse `na_pasta` e, dentro dela, a conexão SSH `web` (id 2)
/// aberta numa aba.
fn cenario(pasta: &Pasta, na_pasta: MouseDoTerminal) -> Montagem {
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Clientes");
    if let Some(Node::Group(g)) = tree.find_mut(grupo) {
        g.mouse = na_pasta;
    }
    let mut c = Connection::new("web", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(Some(grupo), c);
    pasta.semear(&tree);
    let mut m = montar(pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    esperar(&mut m.app, "a aba abrir", |_| {
        Session::find("2-1").is_some()
    });
    m
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// Se o clique direito, sem Ctrl, abre o menu do terminal (e o fecha de novo).
fn clique_direito_abre_o_menu(app: &mut TestApp) -> bool {
    app.right_click(TERMINAL);
    app.frame();
    let abriu = aparece(app, "terminal-controlar");
    if abriu {
        app.key(Key::Escape);
        app.frame();
    }
    abriu
}

fn mouse_da_conexao(pasta: &Pasta) -> MouseDoTerminal {
    pasta.conexoes().connection(2).expect("a conexão").mouse
}

#[test]
fn a_pasta_escolhe_o_menu_e_a_conexao_volta_a_copiar_e_colar() {
    let pasta = Pasta::nova("mouse-pasta-menu");
    // Nas Configurações, o padrão: copiar e colar.
    let mut m = cenario(&pasta, MouseDoTerminal::Menu);
    assert!(
        clique_direito_abre_o_menu(&mut m.app),
        "a pasta escolheu o menu"
    );

    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, LINHA), "Herdar (Clientes)");
    escolher_propriedade(&mut m.app, LINHA);
    m.app
        .select(&format!("props-{LINHA}-valor"), "Copiar e colar");
    m.app.frame();
    assert_eq!(mouse_da_conexao(&pasta), MouseDoTerminal::CopiarColar);
    assert!(
        !clique_direito_abre_o_menu(&mut m.app),
        "a conexão escolheu copiar e colar, e a aba aberta muda junto"
    );
    // O Ctrl+clique direito continua abrindo o menu.
    m.app.right_click_with(TERMINAL, tomni_ui::Modifiers::CTRL);
    m.app.frame();
    assert!(aparece(&m.app, "terminal-controlar"));
}

#[test]
fn sem_escolha_valem_as_configuracoes_e_a_pasta_vence_elas() {
    let pasta = Pasta::nova("mouse-configuracoes");
    pasta.configurar("terminal", "botao_direito", json!("menu"));
    let mut m = cenario(&pasta, MouseDoTerminal::Herdar);
    assert!(
        clique_direito_abre_o_menu(&mut m.app),
        "sem escolha na pasta, vale o menu das Configurações"
    );
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, LINHA), "Herdar (Configurações)");

    // A pasta passa a copiar e colar: a conexão, que herda, segue.
    m.app.click("arvore-1");
    m.app.click("props-grupo-Rede");
    comum::escolher_propriedade_em(&mut m.app, "props-grupo", LINHA);
    m.app
        .select(&format!("props-grupo-{LINHA}-valor"), "Copiar e colar");
    m.app.frame();
    let grupo = pasta.conexoes().find(1).cloned();
    assert!(
        matches!(grupo, Some(Node::Group(ref g)) if g.mouse == MouseDoTerminal::CopiarColar),
        "{grupo:?}"
    );
    assert_eq!(mouse_da_conexao(&pasta), MouseDoTerminal::Herdar);
    assert!(!clique_direito_abre_o_menu(&mut m.app));
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, LINHA), "Herdar (Clientes)");
    m.app.assert_lint();
}
