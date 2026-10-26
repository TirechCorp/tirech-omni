//! O WSL no menu Ferramentas (M42, fase E): o submenu com as distribuições, abrir uma, atualizar
//! a lista e salvar como conexões; e o programa por conexão local nas propriedades.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::abrir::FakeOpener;
use tirech_omni::modelo::{Kind, Node};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, exemplo, montar, montar_com, valor};

/// Quanto o ponteiro espera sobre um item para o submenu abrir.
const ATRASO_DO_SUBMENU: Duration = Duration::from_millis(450);

/// A tela com `distros` já na lista do WSL.
fn tela(nome: &str, distros: &[&str]) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    let opener = FakeOpener::new();
    *opener.distros_wsl.lock().unwrap() = distros.iter().map(|d| (*d).to_owned()).collect();
    let (app, fila) = montar_com(&pasta, Theme::Light, opener.clone());
    let mut m = Montagem { app, opener, fila };
    m.app.frame();
    esperar(&mut m.app, "a lista do WSL", |_| true);
    (pasta, m)
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
    // A thread da lista acorda a tela; um quadro a mais para o menu refletir.
    app.wait(Duration::from_millis(100));
}

fn abrir_submenu(app: &mut TestApp) {
    app.click("menus-ferramentas");
    app.hover("wsl");
    app.wait(ATRASO_DO_SUBMENU);
}

#[test]
fn sem_distribuicoes_o_submenu_nao_aparece() {
    let (_pasta, mut m) = tela("wsl-vazio", &[]);
    m.app.click("menus-ferramentas");
    assert!(
        m.app
            .tree()
            .find("wsl")
            .is_none_or(|_| !m.app.visible("wsl"))
    );
}

#[test]
fn o_submenu_lista_as_distribuicoes() {
    let (_pasta, mut m) = tela("wsl-lista", &["Ubuntu", "Debian"]);
    abrir_submenu(&mut m.app);
    assert_eq!(m.app.text("wsl-1"), "Ubuntu");
    assert_eq!(m.app.text("wsl-2"), "Debian");
    assert!(!m.app.visible("wsl-3"));
}

#[test]
fn escolher_uma_distribuicao_abre_um_terminal_local() {
    let (pasta, mut m) = tela("wsl-abrir", &["Ubuntu", "Debian"]);
    abrir_submenu(&mut m.app);
    m.app.click("wsl-2");
    m.app.frame();
    let abas = m.app.panes("paineis").panes[0].tabs.len();
    assert_eq!(abas, 1);
    let sessoes = m.opener.sessoes.borrow();
    assert_eq!(sessoes.len(), 1);
    assert!(sessoes[0].0.text().contains("Conectado a Debian"));
    drop(sessoes);
    // Como o shell local, não entra nos recentes.
    let recentes =
        std::fs::read_to_string(pasta.armazenamento().arquivo_recentes()).unwrap_or_default();
    assert!(!recentes.contains("Debian"), "{recentes}");
}

#[test]
fn atualizar_le_a_lista_de_novo() {
    let (_pasta, mut m) = tela("wsl-atualizar", &["Ubuntu"]);
    *m.opener.distros_wsl.lock().unwrap() = vec!["Ubuntu".into(), "Alpine".into()];
    abrir_submenu(&mut m.app);
    m.app.click("wsl-atualizar");
    esperar(&mut m.app, "a lista nova", |_| true);
    abrir_submenu(&mut m.app);
    assert_eq!(m.app.text("wsl-2"), "Alpine");
}

#[test]
fn salvar_cria_o_grupo_wsl_sem_repetir() {
    let (pasta, mut m) = tela("wsl-salvar", &["Ubuntu", "Debian"]);
    abrir_submenu(&mut m.app);
    m.app.click("wsl-salvar");
    m.app.frame();
    let arvore = pasta.conexoes();
    let grupo = arvore
        .nodes
        .iter()
        .find_map(|n| match n {
            Node::Group(g) if g.name == "WSL" => Some(g),
            _ => None,
        })
        .expect("o grupo WSL");
    let nomes: Vec<&str> = grupo
        .children
        .iter()
        .filter_map(|n| match n {
            Node::Connection(c) => Some(c.name.as_str()),
            Node::Group(_) => None,
        })
        .collect();
    assert_eq!(nomes, ["Ubuntu", "Debian"]);
    for n in &grupo.children {
        let Node::Connection(c) = n else { continue };
        assert_eq!(c.kind, Kind::Local);
        assert_eq!(c.local_programa, "wsl.exe");
        assert_eq!(c.local_argumentos, format!("-d {} --cd ~", c.name));
    }

    // De novo, com uma distribuição a mais: só ela entra.
    *m.opener.distros_wsl.lock().unwrap() = vec!["Ubuntu".into(), "Debian".into(), "Kali".into()];
    abrir_submenu(&mut m.app);
    m.app.click("wsl-atualizar");
    esperar(&mut m.app, "a lista nova", |_| true);
    abrir_submenu(&mut m.app);
    m.app.click("wsl-salvar");
    m.app.frame();
    let arvore = pasta.conexoes();
    let grupos = arvore
        .nodes
        .iter()
        .filter(|n| matches!(n, Node::Group(g) if g.name == "WSL"))
        .count();
    assert_eq!(grupos, 1);
    let total = arvore
        .connections()
        .into_iter()
        .filter(|c| c.local_programa == "wsl.exe")
        .count();
    assert_eq!(total, 3);
}

#[test]
fn o_programa_da_conexao_local_aparece_e_grava_nas_propriedades() {
    let pasta = Pasta::nova("wsl-propriedades");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    // O Shell (id 5) é do tipo Local.
    m.app.click("arvore-5");
    escolher_propriedade(&mut m.app, "Programa");
    comum::editar_propriedade(&mut m.app, "Programa", "pwsh");
    comum::editar_propriedade(&mut m.app, "Argumentos", "-NoLogo");
    comum::editar_propriedade(&mut m.app, "Pasta inicial", "~/projetos");
    assert_eq!(valor(&m.app, "Programa"), "pwsh");
    let c = pasta.conexoes();
    let local = c.connection(5).expect("o shell local");
    assert_eq!(local.local_programa, "pwsh");
    assert_eq!(local.local_argumentos, "-NoLogo");
    assert_eq!(local.local_pasta, "~/projetos");
}
