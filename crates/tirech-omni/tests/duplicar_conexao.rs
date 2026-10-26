//! "Duplicar" no menu da conexão na árvore: a cópia logo abaixo, com o nome "(cópia)", e a senha
//! do cofre separada quando só a original a usava.

mod comum;

use tirech_omni::modelo::{Node, Tree};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, item_ssh, montar, so_com_senha};

fn nomes(tree: &Tree) -> Vec<String> {
    tree.nodes.iter().map(|n| n.name().to_owned()).collect()
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

#[test]
fn duplicar_poe_a_copia_abaixo_e_separa_a_senha_so_dela() {
    let pasta = Pasta::nova("duplicar-separa");
    let ids = pasta.semear_cofre(vec![item_ssh("ana@sem-chave", "ana", "s3nh4")]);
    let mut tree = so_com_senha();
    tree.connection_mut(1).unwrap().credencial = Some(ids[0]);
    let mut outra = tree.connection(1).unwrap().clone();
    outra.name = "depois".into();
    tree.add_connection(None, outra);
    // A outra conexão usava o mesmo item: tira, para o item ser só da primeira.
    tree.connection_mut(2).unwrap().credencial = None;
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);

    m.app.context_menu("arvore-1");
    m.app.click("menu-1-duplicar");

    let gravada = pasta.conexoes();
    assert_eq!(
        nomes(&gravada),
        ["sem-chave", "sem-chave (cópia)", "depois"]
    );
    let Node::Connection(copia) = &gravada.nodes[1] else {
        panic!("a cópia não é uma conexão")
    };
    assert_ne!(copia.id, 1);
    assert_eq!(
        (copia.host.as_str(), copia.user.as_str()),
        ("sem-chave.exemplo", "ana")
    );
    let item_da_copia = copia.credencial.expect("a cópia tem credencial");
    assert_ne!(
        item_da_copia, ids[0],
        "a senha só da original ganhou um item próprio"
    );
    assert_eq!(gravada.connection(1).unwrap().credencial, Some(ids[0]));
    let cofre = pasta.cofre();
    let item = cofre.item(item_da_copia).expect("o item da cópia");
    assert_eq!(item.senha, "s3nh4");
    assert_eq!(item.titulo, "ana@sem-chave (cópia)");

    // A cópia fica escolhida.
    let escolhida: Vec<String> = m
        .app
        .tree_items("arvore")
        .into_iter()
        .filter(|l| l.selected)
        .map(|l| l.key)
        .collect();
    assert_eq!(escolhida, [copia.id.to_string()]);
}

#[test]
fn credencial_de_varias_conexoes_continua_compartilhada() {
    let pasta = Pasta::nova("duplicar-compartilhada");
    let ids = pasta.semear_cofre(vec![item_ssh("ana@servidores", "ana", "s3nh4")]);
    let mut tree = so_com_senha();
    tree.connection_mut(1).unwrap().credencial = Some(ids[0]);
    let outra = tree.connection(1).unwrap().clone();
    tree.add_connection(None, outra);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);

    m.app.context_menu("arvore-1");
    m.app.click("menu-1-duplicar");

    let gravada = pasta.conexoes();
    let Node::Connection(copia) = &gravada.nodes[1] else {
        panic!("a cópia não é uma conexão")
    };
    assert_eq!(copia.credencial, Some(ids[0]));
    assert_eq!(pasta.cofre().itens().len(), 1);
}

#[test]
fn pasta_nao_tem_duplicar() {
    let pasta = Pasta::nova("duplicar-pasta");
    let mut tree = Tree::default();
    tree.add_group(None, "Clientes");
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);

    m.app.context_menu("arvore-1");
    assert!(aparece(&m.app, "menu-1-renomear"));
    assert!(!aparece(&m.app, "menu-1-duplicar"));
}
