//! Fechar a aba sem uso, por conexão e por pasta: a linha "Fechar sem uso após" mostra de
//! onde vem o tempo e grava a escolha na conexão e na pasta. Os minutos de espera não cabem num
//! teste: a conta do relógio (o uso recomeça a contagem) está nos testes de `src/ocioso.rs`.

mod comum;

use tirech_omni::modelo::{Connection, Kind, Node, Ocioso, Tree};
use tomni_ui::theme::Theme;

use comum::{Pasta, escolher_propriedade, montar, valor};

const LINHA: &str = "Fechar sem uso após";

#[test]
fn a_linha_mostra_a_heranca_e_grava_na_conexao_e_na_pasta() {
    let pasta = Pasta::nova("ocioso-linha");
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Clientes");
    if let Some(Node::Group(g)) = tree.find_mut(grupo) {
        g.ocioso = Ocioso::Minutos(30);
    }
    tree.add_connection(Some(grupo), Connection::new("Shell", Kind::Local));
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, LINHA), "Herdar (Clientes)");
    escolher_propriedade(&mut m.app, LINHA);
    m.app.select(&format!("props-{LINHA}-valor"), "10 min");
    m.app.frame();
    let conexao = || pasta.conexoes().connection(2).expect("a conexão").ocioso;
    assert_eq!(conexao(), Ocioso::Minutos(10));
    m.app.select(&format!("props-{LINHA}-valor"), "Desligado");
    m.app.frame();
    assert_eq!(conexao(), Ocioso::Desligado);

    // A pasta passa a herdar das Configurações.
    m.app.click("arvore-1");
    let na_grade_da_pasta = m
        .app
        .properties("props-grupo")
        .into_iter()
        .find(|p| p.name == LINHA)
        .map(|p| p.value);
    assert_eq!(na_grade_da_pasta.as_deref(), Some("30 min"));
    m.app.click("props-grupo-Rede");
    comum::escolher_propriedade_em(&mut m.app, "props-grupo", LINHA);
    m.app.select(
        &format!("props-grupo-{LINHA}-valor"),
        "Herdar (Configurações)",
    );
    m.app.frame();
    let grupo = pasta.conexoes().find(1).cloned();
    assert!(
        matches!(grupo, Some(Node::Group(ref g)) if g.ocioso == Ocioso::Herdar),
        "{grupo:?}"
    );
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, LINHA), "Desligado");
    m.app.assert_lint();
}
