//! Os painéis com alfinete (M54): a árvore e as propriedades da página Terminais, o menu Exibir,
//! o atalho e o que o `estado.json` guarda.

mod comum;

use tirech_omni::estado;
use tirech_omni::paineis::{LadoSalvo, ModoSalvo, PaineisSalvos};
use tomni_ui::prelude::Shortcut;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui::{Modifiers, WindowHandle};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar(chaves: &[&str]) {
    for chave in chaves {
        if let Some(s) = Session::find(chave) {
            s.dispose();
        }
    }
}

/// Os painéis da página `pagina` como o `estado.json` da pasta os guarda agora.
fn salvos(pasta: &Pasta, pagina: &str) -> Option<PaineisSalvos> {
    let texto = std::fs::read_to_string(pasta.caminho().join("estado.json")).ok()?;
    let valor = serde_json::from_str(&texto).ok()?;
    estado::do_valor(&valor)?.paineis.get(pagina).cloned()
}

fn modo_salvo(pasta: &Pasta, pagina: &str, chave: &str) -> Option<ModoSalvo> {
    salvos(pasta, pagina)?.paineis.get(chave).map(|p| p.modo)
}

fn nomes_das_abas(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()))
        .collect()
}

#[test]
fn a_pagina_terminais_abre_com_os_dois_paineis_fixados() {
    let pasta = Pasta::nova("paineis-padrao");
    pasta.semear(&exemplo());
    let m = montar(&pasta, Theme::Light);
    assert!(m.app.visible("terminais-paineis-conexoes"));
    assert!(m.app.visible("terminais-paineis-propriedades"));
    assert!(m.app.visible("arvore"));
    // Os dois ocupam espaço à esquerda do conteúdo, um sobre o outro.
    let conteudo = m.app.rect("terminais-paineis-conteudo");
    let conexoes = m.app.rect("terminais-paineis-conexoes");
    let propriedades = m.app.rect("terminais-paineis-propriedades");
    assert!(conexoes.x1 <= conteudo.x0);
    assert!(propriedades.x1 <= conteudo.x0);
    assert!(conexoes.y1 <= propriedades.y0 + 1.0);
    // O `DockHost` completa o layout vazio com o padrão dos painéis e o app o guarda assim.
    assert_eq!(
        modo_salvo(&pasta, "terminais", "conexoes"),
        Some(ModoSalvo::Fixado)
    );
    assert_eq!(
        modo_salvo(&pasta, "terminais", "propriedades"),
        Some(ModoSalvo::Fixado)
    );
}

#[test]
fn soltar_o_alfinete_das_conexoes_e_reabrir_o_app_mantem() {
    let pasta = Pasta::nova("paineis-alfinete");
    pasta.semear(&exemplo());
    {
        let mut m = montar(&pasta, Theme::Light);
        m.app.click("terminais-paineis-conexoes-alfinete");
        assert!(m.app.visible("terminais-paineis-conexoes-faixa"));
        assert!(!m.app.visible("terminais-paineis-conexoes"));
        assert_eq!(
            modo_salvo(&pasta, "terminais", "conexoes"),
            Some(ModoSalvo::Auto)
        );
    }
    let m = montar(&pasta, Theme::Light);
    assert!(m.app.visible("terminais-paineis-conexoes-faixa"));
    assert!(!m.app.visible("terminais-paineis-conexoes"));
    assert!(m.app.visible("terminais-paineis-propriedades"));
}

#[test]
fn um_estado_antigo_das_notas_usa_a_lateral_gravada() {
    let pasta = Pasta::nova("paineis-lateral-antiga");
    pasta.semear(&exemplo());
    std::fs::write(
        pasta.caminho().join("estado.json"),
        r#"{"janelas":[],"notas":{"lateral":420}}"#,
    )
    .unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-notas");
    let largura = m.app.rect("notas-paineis-lista").width();
    assert!(
        (largura - 420.0).abs() <= 2.0,
        "a lateral gravada devia valer: {largura}"
    );
    // Depois de mexer, os painéis passam a ser guardados com o lado esquerdo.
    m.app.click("notas-paineis-lista-alfinete");
    let salvos = salvos(&pasta, "notas").expect("os painéis das notas foram guardados");
    assert_eq!(salvos.paineis["lista"].lado, LadoSalvo::Esquerda);
    assert_eq!(salvos.paineis["lista"].modo, ModoSalvo::Auto);
}

#[test]
fn o_item_do_menu_exibir_fecha_e_reabre_as_conexoes() {
    let pasta = Pasta::nova("paineis-menu");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-exibir").click("painel-conexoes");
    assert_eq!(
        modo_salvo(&pasta, "terminais", "conexoes"),
        Some(ModoSalvo::Fechado)
    );
    // As propriedades sobem para ocupar a área toda.
    assert!(m.app.rect("terminais-paineis-propriedades").height() > 400.0);

    m.app.click("menus-exibir").click("painel-conexoes");
    assert!(m.app.visible("terminais-paineis-conexoes"));
    assert_eq!(
        modo_salvo(&pasta, "terminais", "conexoes"),
        Some(ModoSalvo::Fixado)
    );
}

#[test]
fn o_atalho_das_conexoes_alterna() {
    let pasta = Pasta::nova("paineis-atalho");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    let atalho = Shortcut::new('B', Modifiers::CTRL | Modifiers::ALT);
    m.app.shortcut(atalho);
    assert_eq!(
        modo_salvo(&pasta, "terminais", "conexoes"),
        Some(ModoSalvo::Fechado)
    );
    m.app.shortcut(atalho);
    assert!(m.app.visible("terminais-paineis-conexoes"));
}

#[test]
fn a_arvore_flutuando_abre_a_conexao_na_janela_principal() {
    let pasta = Pasta::nova("paineis-flutuando");
    pasta.semear(&exemplo());
    let Montagem {
        mut app, opener, ..
    } = montar(&pasta, Theme::Light);
    app.click("terminais-paineis-conexoes-flutuar");
    let janelas = app.windows();
    assert_eq!(janelas.len(), 2, "a árvore ganhou uma janela própria");
    assert_eq!(
        modo_salvo(&pasta, "terminais", "conexoes"),
        Some(ModoSalvo::Flutuando)
    );

    app.switch_to(janelas[1]);
    app.expand("arvore", "1");
    app.double_click("arvore-2");
    assert_eq!(*opener.abertas.borrow(), [2]);

    app.switch_to(WindowHandle::main());
    assert_eq!(nomes_das_abas(&app), ["web-01"]);
    encerrar(&["2-1"]);
}
