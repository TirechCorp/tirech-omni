//! O zoom da fonte do terminal, por aba: Ctrl+`=`, Ctrl+`-`, Ctrl+`0` e Ctrl+roda mudam só a aba
//! do painel ativo, com o tamanho das configurações por baixo e os limites de 6 a 72 pt.
//!
//! O tamanho da fonte de cada terminal sai do `TerminalVisual::metrics`.

mod comum;

use tomni_ui::kurbo::Vec2;
use tomni_ui::terminal::{FONT_ZOOM_WHEEL, TerminalVisual};
use tomni_ui::theme::Theme;
use tomni_ui::{Look, Modifiers, Shortcut};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};
use tirech_omni::configuracoes::catalogo::{self, Tipo};

/// O tamanho da fonte do terminal `id`, em pontos.
fn pontos(app: &TestApp, id: &str) -> f32 {
    match app.look(id) {
        Look::Custom(custom) => {
            custom
                .downcast::<TerminalVisual>()
                .expect("o visual do terminal")
                .metrics()
                .font_size
        }
        outro => panic!("não é o terminal: {outro:?}"),
    }
}

/// Os ids dos terminais dos painéis, na ordem dos painéis.
fn terminais(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .filter_map(|p| {
            p.tabs
                .first()
                .map(|(chave, _)| format!("paineis-terminal-{chave}"))
        })
        .collect()
}

/// Uma aba aberta e dividida à direita: dois painéis, o segundo duplicado do primeiro.
fn dois_paineis(nome: &str, base: Option<u32>) -> (Pasta, Montagem, String, String) {
    let pasta = Pasta::nova(nome);
    if let Some(base) = base {
        pasta.configurar("terminal", "tamanho_fonte", base.into());
    }
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    m.app
        .shortcut(Shortcut::new('=', Modifiers::ALT | Modifiers::SHIFT));
    let ids = terminais(&m.app);
    assert_eq!(ids.len(), 2, "o painel foi dividido: {ids:?}");
    (pasta, m, ids[0].clone(), ids[1].clone())
}

fn ctrl(app: &mut TestApp, tecla: char) {
    app.shortcut(Shortcut::ctrl(tecla));
}

#[test]
fn os_atalhos_mudam_so_a_aba_do_painel_ativo() {
    let (_pasta, mut m, a, b) = dois_paineis("zoom-atalhos", None);
    let base = pontos(&m.app, &a);
    assert_eq!(pontos(&m.app, &b), base);

    m.app.click(&a);
    ctrl(&mut m.app, '=');
    ctrl(&mut m.app, '=');
    assert!(pontos(&m.app, &a) > base, "Ctrl+= aumenta a aba ativa");
    assert_eq!(pontos(&m.app, &b), base, "a outra aba não muda");

    // Ctrl++ (Shift junto, como sai no teclado) e o `+` do teclado numérico também aumentam.
    let dois_pontos = pontos(&m.app, &a);
    m.app
        .shortcut(Shortcut::new('+', Modifiers::CTRL | Modifiers::SHIFT));
    m.app.shortcut(Shortcut::new('+', Modifiers::CTRL));
    assert!(pontos(&m.app, &a) > dois_pontos);
    assert_eq!(pontos(&m.app, &b), base);

    // Ctrl+- diminui só a aba ativa, agora a outra.
    m.app.click(&b);
    ctrl(&mut m.app, '-');
    assert!(pontos(&m.app, &b) < base);
    let a_zoom = pontos(&m.app, &a);
    assert!(a_zoom > base, "a primeira segue ampliada");

    // Ctrl+0 volta a ativa ao tamanho das configurações, sem tocar na outra.
    ctrl(&mut m.app, '0');
    assert_eq!(pontos(&m.app, &b), base);
    assert_eq!(pontos(&m.app, &a), a_zoom);
    m.app.click(&a);
    ctrl(&mut m.app, '0');
    assert_eq!(pontos(&m.app, &a), base);
}

#[test]
fn o_zoom_para_em_6_e_em_72_pontos() {
    // 8 pt: dois passos para baixo chegam ao 6 e o resto não anda mais.
    let (_pasta, mut m, a, _) = dois_paineis("zoom-limite-baixo", Some(8));
    m.app.click(&a);
    let oito = pontos(&m.app, &a);
    ctrl(&mut m.app, '-');
    let sete = pontos(&m.app, &a);
    ctrl(&mut m.app, '-');
    let seis = pontos(&m.app, &a);
    assert!(oito > sete && sete > seis);
    for _ in 0..5 {
        ctrl(&mut m.app, '-');
    }
    assert_eq!(pontos(&m.app, &a), seis, "6 pt é o menor");
    // Um passo para cima sai do mínimo na hora (o excesso não se acumulou).
    ctrl(&mut m.app, '=');
    assert_eq!(pontos(&m.app, &a), sete);

    // 48 pt: 24 passos chegam ao 72 e o resto não anda.
    let (_pasta, mut m, a, _) = dois_paineis("zoom-limite-alto", Some(48));
    m.app.click(&a);
    for _ in 0..23 {
        ctrl(&mut m.app, '=');
    }
    let setenta_e_um = pontos(&m.app, &a);
    ctrl(&mut m.app, '=');
    let setenta_e_dois = pontos(&m.app, &a);
    assert!(setenta_e_dois > setenta_e_um);
    for _ in 0..5 {
        ctrl(&mut m.app, '=');
    }
    assert_eq!(pontos(&m.app, &a), setenta_e_dois, "72 pt é o maior");
    ctrl(&mut m.app, '-');
    assert_eq!(pontos(&m.app, &a), setenta_e_um);
}

#[test]
fn o_ctrl_roda_no_terminal_faz_o_zoom_da_aba() {
    let (_pasta, mut m, a, b) = dois_paineis("zoom-roda", None);
    let base = pontos(&m.app, &a);
    let ponto = m.app.rect(&a).center();
    m.app.ui().set_modifiers(Modifiers::CTRL);
    m.app.wheel(ponto, Vec2::new(0.0, -2.0 * FONT_ZOOM_WHEEL));
    m.app.ui().set_modifiers(Modifiers::NONE);
    assert!(pontos(&m.app, &a) > base, "a roda para cima aumenta");
    assert_eq!(pontos(&m.app, &b), base);

    m.app.ui().set_modifiers(Modifiers::CTRL);
    m.app.wheel(ponto, Vec2::new(0.0, 2.0 * FONT_ZOOM_WHEEL));
    m.app.ui().set_modifiers(Modifiers::NONE);
    assert_eq!(pontos(&m.app, &a), base, "a roda para baixo desfaz");
}

#[test]
fn a_copia_da_aba_nasce_com_o_zoom_dela() {
    let pasta = Pasta::nova("zoom-copia");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let a = terminais(&m.app).remove(0);
    let base = pontos(&m.app, &a);
    ctrl(&mut m.app, '=');
    ctrl(&mut m.app, '=');
    let ampliada = pontos(&m.app, &a);
    assert!(ampliada > base);

    m.app
        .shortcut(Shortcut::new('=', Modifiers::ALT | Modifiers::SHIFT));
    let ids = terminais(&m.app);
    assert_eq!(ids.len(), 2);
    assert_eq!(pontos(&m.app, &ids[1]), ampliada, "a cópia herda o zoom");
    assert_eq!(pontos(&m.app, &ids[0]), ampliada);
}

#[test]
fn mudar_o_tamanho_nas_configuracoes_leva_o_zoom_junto() {
    let (_pasta, mut m, a, b) = dois_paineis("zoom-configuracao", None);
    m.app.click(&a);
    ctrl(&mut m.app, '=');
    ctrl(&mut m.app, '=');
    let (a_16, b_16) = (pontos(&m.app, &a), pontos(&m.app, &b));
    assert!(a_16 > b_16);

    // Nas configurações a base passa de 16 para 24: as duas abas crescem e a diferença de 2 pt
    // continua.
    m.app.shortcut(Shortcut::ctrl(','));
    let fonte = comum_id_do_valor();
    alcancar(&mut m.app, &fonte);
    m.app.set_value(&fonte, 24.0);
    m.app.click("navegacao-terminais");
    let (a_24, b_24) = (pontos(&m.app, &a), pontos(&m.app, &b));
    assert!(b_24 > b_16, "a aba sem zoom segue a configuração");
    assert!(a_24 > a_16, "a aba com zoom também");
    assert!(a_24 > b_24, "e mantém a diferença");

    // Ctrl+0 na aba ampliada a iguala à outra, agora de 24 pt.
    m.app.click(&a);
    ctrl(&mut m.app, '0');
    assert_eq!(pontos(&m.app, &a), b_24);
}

fn comum_id_do_valor() -> String {
    catalogo::id_do_valor("terminal.tamanho_fonte")
}

/// Abre as seções e os grupos de Configurações até o controle `id` aparecer.
fn alcancar(app: &mut TestApp, id: &str) {
    let aparece = |app: &mut TestApp| app.ui().find(id).is_some() && app.visible(id);
    if !aparece(app) && app.label("config-expandir-tudo") == "Expandir tudo" {
        app.click("config-expandir-tudo");
    }
    for grupo in catalogo::CATALOGO.iter().filter(|d| d.tipo == Tipo::Grupo) {
        if aparece(app) {
            return;
        }
        let cabecalho = format!("{}-cabecalho", grupo.id());
        if app.ui().find(&cabecalho).is_some() {
            app.click(&cabecalho);
        }
    }
    assert!(aparece(app), "{id:?} não aparece na página");
}
