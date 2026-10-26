//! O botão de tema da barra de título: o menu com os três temas, ligado a `geral.tema`.

mod comum;

use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::configuracoes::{Configuracoes, Tema};
use tomni_ui::theme::Theme;

use comum::{Montagem, Pasta, montar_pura};

/// A tela pura (sem conexões) no tema `tema` do sistema.
fn abrir(nome: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(&format!("tema-{nome}"));
    let m = montar_pura(&pasta, tema);
    (pasta, m)
}

/// O `geral.tema` gravado no `configuracoes.json`.
fn gravado(pasta: &Pasta) -> Tema {
    Configuracoes::carregar(&Armazenamento::new(pasta.caminho()))
        .0
        .geral
        .tema
}

#[test]
fn o_botao_de_tema_troca_o_tema() {
    let (pasta, mut m) = abrir("troca", Theme::Light);
    assert_eq!(gravado(&pasta), Tema::Sistema);

    m.app.click("barra-de-titulo-tema");
    m.app.click("tema-escuro");
    assert_eq!(gravado(&pasta), Tema::Escuro);

    m.app.click("barra-de-titulo-tema");
    m.app.click("tema-claro");
    assert_eq!(gravado(&pasta), Tema::Claro);

    m.app.click("barra-de-titulo-tema");
    m.app.click("tema-sistema");
    assert_eq!(gravado(&pasta), Tema::Sistema);
}

#[test]
fn o_menu_mostra_o_tema_atual() {
    let pasta = Pasta::nova("tema-atual");
    pasta.configurar("geral", "tema", serde_json::json!("escuro"));
    let mut m = montar_pura(&pasta, Theme::Light);

    m.app.click("barra-de-titulo-tema");
    assert!(m.app.checked("tema-escuro"));
    assert!(!m.app.checked("tema-claro"));
    assert!(!m.app.checked("tema-sistema"));
}

#[test]
fn trocar_na_pagina_de_configuracoes_atualiza_o_menu() {
    let (_pasta, mut m) = abrir("pagina", Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-geral");
    let id = tirech_omni::configuracoes::catalogo::id_do_valor("geral.tema");
    m.app.select(&id, &Tema::Escuro.nome());

    m.app.click("barra-de-titulo-tema");
    assert!(m.app.checked("tema-escuro"));
    assert!(!m.app.checked("tema-sistema"));
}

#[test]
fn o_botao_e_o_menu_passam_no_lint_nos_dois_temas() {
    for tema in [Theme::Light, Theme::Dark] {
        let (_pasta, mut m) = abrir("lint", tema);
        m.app.click("barra-de-titulo-tema");
        m.app.assert_lint();
    }
}
