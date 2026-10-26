//! A aparência do diálogo "Instalar no computador" (M65) nos dois temas, num Windows de mentira
//! (os caminhos não variam com a máquina): nada instalado, tudo instalado e o resultado numa
//! `InfoBar`. Snapshot e lint.

mod comum;

use std::time::Duration;

use tirech_omni::instalacao::{EstadoDoItem as Estado, ItemDeInstalacao as Item, Motivo};
use tomni_ui::theme::Theme;

use comum::{Pasta, ambiente_windows_falso, montar_com_ambiente};

fn nada_instalado() -> Vec<(Item, Estado)> {
    vec![
        (Item::Copia, Estado::Ausente),
        (Item::AreaDeTrabalho, Estado::Ausente),
        (Item::Menu, Estado::Ausente),
        (Item::Barra, Estado::Ausente),
        (Item::Inicio, Estado::Ausente),
        (
            Item::Aplicativos,
            Estado::Indisponivel(Motivo::PrecisaDaCopia),
        ),
    ]
}

fn tudo_instalado() -> Vec<(Item, Estado)> {
    Item::TODOS
        .into_iter()
        .map(|i| (i, Estado::Presente))
        .collect()
}

fn dialogo(nome: &str, tema: Theme, estados: Vec<(Item, Estado)>) {
    let pasta = Pasta::nova(&format!("visual-instalacao-{nome}"));
    let (mut m, _) = montar_com_ambiente(&pasta, tema, |a| ambiente_windows_falso(a, estados));
    m.app.click("menus-arquivo").click("instalar-no-computador");
    // Deixa a leitura do estado e a animação do diálogo terminarem.
    m.app.wait(Duration::from_millis(600));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("instalacao-{nome}"));
}

#[test]
fn nada_instalado_no_tema_claro() {
    dialogo("nada-claro", Theme::Light, nada_instalado());
}

#[test]
fn nada_instalado_no_tema_escuro() {
    dialogo("nada-escuro", Theme::Dark, nada_instalado());
}

#[test]
fn tudo_instalado_no_tema_claro() {
    dialogo("tudo-claro", Theme::Light, tudo_instalado());
}

#[test]
fn tudo_instalado_no_tema_escuro() {
    dialogo("tudo-escuro", Theme::Dark, tudo_instalado());
}
