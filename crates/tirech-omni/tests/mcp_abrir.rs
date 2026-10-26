//! O controle por assistentes (M51, fatia 6): a conexão aberta pela IA, com e sem senha salva, e a
//! pasta com permissão `Livre`. O assistente é um cliente do canal, que no Windows é um pipe: o
//! que depende do endereço do socket fica só no Unix.

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;

use std::time::{Duration, Instant};

use serde_json::json;
use tirech_omni::abrir::Senhas;
use tirech_omni::mcp::permissao::PermissaoIa;
use tirech_omni::modelo::{Connection, Kind, Node, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Montagem, Pasta, item_ssh, montar, so_com_senha};

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

// ---------------------------------------------------------------------------------------
// A IA abre conexões.
// ---------------------------------------------------------------------------------------

/// Uma tela com o MCP ligado e a árvore dada.
fn tela(nome: &str, tree: &Tree) -> (Pasta, Montagem, std::path::PathBuf) {
    let pasta = Pasta::nova(nome);
    pasta.semear(tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    let endereco = pasta.0.join("mcp.sock");
    let m = montar(&pasta, Theme::Light);
    (pasta, m, endereco)
}

/// Uma SSH sem chave (id 1) com permissão `Livre`.
fn so_com_senha_livre() -> Tree {
    let mut tree = so_com_senha();
    tree.connection_mut(1).unwrap().mcp = PermissaoIa::Livre;
    tree
}

#[test]
fn abrir_conexao_com_senha_salva_nao_pergunta() {
    let pasta = Pasta::nova("mcp-tela-senha-salva");
    let ids = pasta.semear_cofre(vec![item_ssh("ana@sem-chave", "ana", "s3nh4")]);
    let mut tree = so_com_senha_livre();
    tree.connection_mut(1).unwrap().credencial = Some(ids[0]);
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    let endereco = pasta.0.join("mcp.sock");
    let mut m = montar(&pasta, Theme::Light);

    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let pedido = ia.pedir("abrir_conexao", json!({ "conexao": 1 }));
    esperar(
        &mut m.app,
        |_| Session::find("1-1").is_some(),
        "a aba não abriu",
    );
    assert!(!m.app.is_open("senha-dialogo"), "a senha está no cofre");
    let sessao = Session::find("1-1").unwrap();
    m.opener.handle_de(sessao).unwrap().feed("$ ");
    let r = ia.resposta(&mut m.app, pedido);
    assert!(!r.e_erro(), "{}", r.texto());
    assert_eq!(
        m.opener.senhas.borrow()[0],
        Senhas {
            conexao: Some("s3nh4".to_owned()),
            salto: None,
            chave: None,
            token: None,
            chave_cofre: None,
            rede: None,
        }
    );
}

#[test]
fn abrir_conexao_sem_senha_pergunta_a_pessoa() {
    let (_pasta, mut m, endereco) = tela("mcp-tela-sem-senha", &so_com_senha_livre());
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let pedido = ia.pedir("abrir_conexao", json!({ "conexao": 1 }));
    esperar(
        &mut m.app,
        |a| a.is_open("senha-dialogo"),
        "o diálogo de senha não abriu",
    );
    assert!(Session::find("1-1").is_none(), "nada abre antes da senha");

    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-primary");
    esperar(
        &mut m.app,
        |_| Session::find("1-1").is_some(),
        "a aba não abriu depois da senha",
    );
    let sessao = Session::find("1-1").unwrap();
    m.opener.handle_de(sessao).unwrap().feed("$ ");
    let r = ia.resposta(&mut m.app, pedido);
    assert!(!r.e_erro(), "{}", r.texto());
    assert_eq!(
        m.opener.senhas.borrow()[0].conexao.as_deref(),
        Some("segredo")
    );
}

#[test]
fn pasta_livre_libera_as_de_dentro() {
    let mut tree = Tree::default();
    let pasta_id = tree.add_group(None, "Livres");
    if let Some(Node::Group(g)) = tree.find_mut(pasta_id) {
        g.mcp = PermissaoIa::Livre;
    }
    tree.add_connection(Some(pasta_id), Connection::new("dentro", Kind::Local));
    let (_pasta, mut m, endereco) = tela("mcp-tela-pasta-livre", &tree);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");

    let aberta = ia.chamar(&mut m.app, "abrir_conexao", json!({ "conexao": 2 }));
    assert!(!aberta.e_erro(), "{}", aberta.texto());
    let sessao = Session::find("2-1").expect("a aba da conexão de dentro");
    m.opener.handle_de(sessao).unwrap().feed("$ ");
    let digitou = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "2-1", "texto": "ls\n" }),
    );
    assert!(!digitou.e_erro(), "{}", digitou.texto());
    assert!(!m.app.visible("mcp-aprovar"), "Livre não pergunta");
}
