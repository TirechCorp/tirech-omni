//! A chave privada da pasta: as linhas "Chave do cofre" e "Chave privada" das propriedades da
//! pasta gravam nela, e a conexão sem chave própria mostra "Herdar (pasta)" na linha "Chave do
//! cofre". A conta de qual chave vale ao abrir está nos testes de `src/credencial.rs`.

mod comum;

use std::path::PathBuf;

use tirech_omni::modelo::{Connection, Kind, Node, Tree};
use tomni_ui::Key;
use tomni_ui::theme::Theme;

use comum::{Pasta, editar_propriedade_em, escolher_propriedade_em, item_chave_ssh, montar, valor};

fn grupo(pasta: &Pasta) -> tirech_omni::modelo::Group {
    match pasta.conexoes().find(1) {
        Some(Node::Group(g)) => g.clone(),
        outro => panic!("sem a pasta: {outro:?}"),
    }
}

#[test]
fn a_pasta_guarda_a_chave_e_a_conexao_mostra_que_herda() {
    let pasta = Pasta::nova("chave-da-pasta");
    let (item, _) = item_chave_ssh("ana@teste", false);
    let id = pasta.semear_cofre(vec![item])[0];
    let mut tree = Tree::default();
    let g = tree.add_group(None, "Clientes");
    tree.add_connection(Some(g), Connection::new("web", Kind::Ssh));
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);

    m.app.expand("arvore", "1");
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, "Chave do cofre"), "Nenhuma");

    m.app.click("arvore-1");
    m.app.click("props-grupo-Rede");
    escolher_propriedade_em(&mut m.app, "props-grupo", "Chave do cofre");
    m.app.key(Key::F(4));
    assert!(m.app.is_open("props-grupo-Chave do cofre-valor-lista"));
    m.app.assert_lint();
    m.app.click(&format!(
        "props-grupo-Chave do cofre-valor-ana-teste--{id}-"
    ));
    let uid = pasta.cofre().item(id).unwrap().uid.clone();
    assert_eq!(grupo(&pasta).chave_do_cofre.as_deref(), Some(uid.as_str()));

    m.app.click("props-grupo-Rede");
    editar_propriedade_em(
        &mut m.app,
        "props-grupo",
        "Chave privada",
        "/chaves/clientes",
    );
    m.app.frame();
    assert_eq!(
        grupo(&pasta).key_file,
        Some(PathBuf::from("/chaves/clientes"))
    );
    assert_eq!(
        grupo(&pasta).chave_do_cofre.as_deref(),
        Some(uid.as_str()),
        "editar o arquivo não solta a chave do cofre"
    );

    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, "Chave do cofre"), "Herdar (Clientes)");
    assert_eq!(valor(&m.app, "Chave privada"), "");
    assert!(
        pasta
            .conexoes()
            .connection(2)
            .unwrap()
            .chave_do_cofre
            .is_none(),
        "mostrar a herança não grava na conexão"
    );

    // A pasta volta a não ter chave: a conexão volta a mostrar "Nenhuma".
    m.app.click("arvore-1");
    m.app.click("props-grupo-Rede");
    escolher_propriedade_em(&mut m.app, "props-grupo", "Chave do cofre");
    m.app.select("props-grupo-Chave do cofre-valor", "Nenhuma");
    m.app.click("props-grupo-Rede");
    editar_propriedade_em(&mut m.app, "props-grupo", "Chave privada", "");
    m.app.frame();
    let g = grupo(&pasta);
    assert_eq!((g.chave_do_cofre, g.key_file), (None, None));
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, "Chave do cofre"), "Nenhuma");
    m.app.assert_lint();
}
