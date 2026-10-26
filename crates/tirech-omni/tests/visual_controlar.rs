//! A aparência do "Controlar com" (M63) nos dois temas: o clique direito no terminal com o
//! submenu aberto (Claude Code, Codex e OpenCode). Snapshot e lint.

#![cfg(unix)]

mod comum;

use std::time::Duration;

use serde_json::json;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::Modifiers;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;

use comum::{Pasta, montar};

fn submenu_aberto(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("visual-controlar-{nome}"));
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("salva", Kind::Local));
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    let mut m = montar(&pasta, tema);
    m.app.double_click("arvore-1");
    m.opener
        .handle_de(Session::find("1-1").expect("a aba 1-1"))
        .unwrap()
        .feed("$ ");
    m.app
        .right_click_with("paineis-terminal-1-1", Modifiers::CTRL);
    m.app.click("terminal-controlar");
    // Deixa as animações do menu e do submenu terminarem.
    m.app.wait(Duration::from_millis(500));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("controlar-submenu-{nome}"));
}

#[test]
fn submenu_do_terminal_no_tema_claro() {
    submenu_aberto("claro", Theme::Light);
}

#[test]
fn submenu_do_terminal_no_tema_escuro() {
    submenu_aberto("escuro", Theme::Dark);
}
