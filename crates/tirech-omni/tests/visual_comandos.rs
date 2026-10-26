//! O editor dos comandos ao conectar (M50) nos dois temas: três passos, a pasta em "Herdar" com
//! a lista desabilitada e a lista de sugestões aberta. Snapshots e lint.

mod comum;

use tirech_omni::modelo::{
    ComandosAoConectar, Connection, Espera, Kind, ModoComandos, Node, OrigemDaSenha, Passo, Tree,
};
use tomni_ui::theme::Theme;

use comum::{Pasta, escolher_propriedade, montar};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

fn ssh(nome: &str, comandos: ComandosAoConectar) -> Connection {
    let mut c = Connection::new(nome, Kind::Ssh);
    c.host = format!("{nome}.exemplo");
    c.user = "deploy".into();
    c.comandos = comandos;
    c
}

fn tres_passos() -> ComandosAoConectar {
    let mut espera = Passo::comando("sudo su -", 15);
    espera.espera = Espera::Texto {
        texto: "password|senha".into(),
    };
    ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos: vec![
            espera,
            Passo::senha(OrigemDaSenha::Conexao, 15),
            Passo::comando("cd /srv/app", 15),
        ],
        reconectar: true,
    }
}

#[test]
fn o_editor_com_tres_passos_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("cmd-visual-{nome}"));
        let mut tree = Tree::default();
        tree.add_connection(None, ssh("web", tres_passos()));
        pasta.semear(&tree);
        let mut m = montar(&pasta, tema);
        m.app.click("arvore-1");
        escolher_propriedade(&mut m.app, "Comandos");
        m.app.click("props-Comandos-botao");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("comandos-editor-{nome}"));
    }
}

#[test]
fn a_conexao_que_herda_da_pasta_tem_a_lista_desabilitada() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("cmd-visual-herda-{nome}"));
        let mut tree = Tree::default();
        let grupo = tree.add_group(None, "Cliente A");
        if let Node::Group(g) = &mut tree.nodes[0] {
            g.comandos = tres_passos();
        }
        let filha = tree.add_connection(Some(grupo), ssh("web", ComandosAoConectar::default()));
        pasta.semear(&tree);
        let mut m = montar(&pasta, tema);
        m.app.expand("arvore", &grupo.to_string());
        m.app.click(&format!("arvore-{filha}"));
        escolher_propriedade(&mut m.app, "Comandos");
        m.app.click("props-Comandos-botao");
        m.app.click("comandos-copiar-da-pasta");
        m.app
            .select("comandos-modo", "Herdar (Cliente A: 3 passos)");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("comandos-herda-{nome}"));
    }
}

#[test]
fn a_lista_de_sugestoes_aberta_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("cmd-visual-sugestoes-{nome}"));
        let mut tree = Tree::default();
        tree.add_connection(None, ssh("web", tres_passos()));
        tree.add_connection(None, ssh("db", ComandosAoConectar::default()));
        pasta.semear(&tree);
        let mut m = montar(&pasta, tema);
        m.app.click("arvore-2");
        escolher_propriedade(&mut m.app, "Comandos");
        m.app.click("props-Comandos-botao");
        m.app.click("comandos-adicionar");
        m.app.click("comandos-passo-1-texto").type_text("c");
        m.app.assert_snapshot(&format!("comandos-sugestoes-{nome}"));
    }
}
