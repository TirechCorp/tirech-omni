//! O menu do botão de tema da barra de título (M41) nos dois temas: snapshot e lint. Um teste
//! por tema, para que um `.snap.new` de um não esconda o do outro.

mod comum;

use tomni_ui::theme::Theme;

use comum::{Pasta, montar_pura};

/// A tela com o menu de tema aberto, no tema do sistema (o padrão).
fn menu_aberto(nome: &str, tema: Theme) -> comum::Montagem {
    let pasta = Pasta::nova(&format!("tema-menu-{nome}"));
    pasta.configurar("geral", "tema", serde_json::json!("sistema"));
    let mut m = montar_pura(&pasta, tema);
    m.app.click("barra-de-titulo-tema");
    m.app.assert_lint();
    m
}

#[test]
fn o_menu_de_tema_no_claro() {
    let m = menu_aberto("claro", Theme::Light);
    m.app.assert_snapshot("tema-menu-claro");
}

#[test]
fn o_menu_de_tema_no_escuro() {
    let m = menu_aberto("escuro", Theme::Dark);
    m.app.assert_snapshot("tema-menu-escuro");
}
