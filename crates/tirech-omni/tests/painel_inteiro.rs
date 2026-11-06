//! Fechar e mover um painel inteiro: a pergunta do "Fechar o painel…" (fechar as conexões ou levar
//! as abas a um painel que já existe, nesta janela ou em outra), o "Mover o painel para a janela",
//! o atalho e o menu Janela.

mod comum;

use tomni_ui::WindowHandle;
use tomni_ui::prelude::Shortcut;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

const DIALOGO: &str = "fechar-painel-dialogo";

/// As chaves das abas de cada painel da janela atual.
fn abas(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()).collect())
        .collect()
}

/// O título da aba `chave` da janela atual.
fn titulo(app: &TestApp, chave: &str) -> String {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter())
        .find(|(c, _)| c == chave)
        .map(|(_, titulo)| titulo.clone())
        .expect("a aba está na janela")
}

/// As opções do ComboBox `id`.
fn opcoes(app: &mut TestApp, id: &str) -> Vec<String> {
    let no = app.ui().find(id).expect("o ComboBox está na tela");
    app.ui()
        .part(no, "(nenhuma opção tem este nome)")
        .expect_err("a opção não existe")
}

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar(chaves: &[&String]) {
    for chave in chaves {
        if let Some(s) = Session::find(chave) {
            s.dispose();
        }
    }
}

/// Manda a aba `chave`, do painel `n`, para um painel novo à direita.
fn para_a_direita(app: &mut TestApp, n: usize, chave: &str) {
    app.context_menu(&format!("paineis-painel-{n}-{chave}"));
    app.click(&format!("paineis-painel-{n}-{chave}-menu-direita"));
    app.frame();
}

/// Abre o menu da aba `chave` do painel `n` e escolhe `item` (um submenu e o item, se há dois).
fn no_menu_da_aba(app: &mut TestApp, n: usize, chave: &str, itens: &[&str]) {
    app.context_menu(&format!("paineis-painel-{n}-{chave}"));
    for item in itens {
        app.click(item);
    }
    app.frame();
}

/// A tela com três abas: `[a, b] | [c]`.
fn dois_paineis(pasta: &Pasta) -> (Montagem, String, String, String) {
    pasta.semear(&exemplo());
    let mut m = montar(pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app
        .double_click("arvore-2")
        .double_click("arvore-3")
        .double_click("arvore-4");
    let chaves = abas(&m.app).remove(0);
    let [a, b, c] = &chaves[..] else {
        panic!("esperava três abas: {chaves:?}");
    };
    para_a_direita(&mut m.app, 0, c);
    assert_eq!(abas(&m.app), [vec![a.clone(), b.clone()], vec![c.clone()]]);
    (m, a.clone(), b.clone(), c.clone())
}

/// Uma segunda janela, vazia, com a principal em foco.
fn segunda_janela(m: &mut Montagem) -> (WindowHandle, WindowHandle) {
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(principal);
    (principal, outra)
}

#[test]
fn fechar_as_abas_fecha_o_painel_e_as_conexoes() {
    let pasta = Pasta::nova("painel-fechar-abas");
    let (mut m, a, b, c) = dois_paineis(&pasta);
    no_menu_da_aba(&mut m.app, 0, &a, &["painel-fechar"]);
    assert!(m.app.is_open(DIALOGO));
    assert_eq!(
        m.app.text("fechar-painel-texto"),
        "Este painel tem 2 abas. Mova-as para outro painel ou feche as conexões junto."
    );
    m.app.click(&format!("{DIALOGO}-secondary"));
    m.app.frame();
    assert!(!m.app.is_open(DIALOGO));
    assert_eq!(abas(&m.app), [[c.clone()]]);
    assert!(Session::find(&a).is_none());
    assert!(Session::find(&b).is_none());
    assert_eq!(
        m.app.focused(),
        Some(format!("paineis-terminal-{c}")),
        "o foco vai para o painel que ficou"
    );
    encerrar(&[&c]);
}

#[test]
fn mover_as_abas_para_um_painel_de_outra_janela_nao_reconecta() {
    let pasta = Pasta::nova("painel-mover-abas");
    let (mut m, a, b, c) = dois_paineis(&pasta);
    let (principal, outra) = segunda_janela(&mut m);
    Session::find(&a).unwrap().feed(b"uptime\r\n");
    no_menu_da_aba(&mut m.app, 0, &a, &["painel-fechar"]);
    m.app.select("fechar-painel-destino", "Janela 2 › Painel 1");
    m.app.click(&format!("{DIALOGO}-primary"));
    m.app.frame();
    assert_eq!(abas(&m.app), [[c.clone()]], "o painel de origem fechou");
    m.app.switch_to(outra);
    assert_eq!(abas(&m.app), [[a.clone(), b.clone()]]);
    assert!(m.app.session_text(&a).contains("uptime"));
    assert_eq!(m.opener.abertas.borrow().len(), 3);
    assert!(principal.is_open());
    encerrar(&[&a, &b, &c]);
}

#[test]
fn a_lista_comeca_nos_paineis_desta_janela_e_nao_tem_o_proprio() {
    let pasta = Pasta::nova("painel-lista");
    let (mut m, a, b, c) = dois_paineis(&pasta);
    segunda_janela(&mut m);
    no_menu_da_aba(&mut m.app, 0, &a, &["painel-fechar"]);
    let opcoes = opcoes(&mut m.app, "fechar-painel-destino");
    let nome = titulo(&m.app, &c);
    assert_eq!(
        opcoes,
        [
            format!("Esta janela › Painel 2 — {nome}"),
            "Janela 2 › Painel 1".to_owned()
        ]
    );
    m.app.key(tomni_ui::Key::Escape);
    encerrar(&[&a, &b, &c]);
}

/// O painel de origem vem antes do de destino, e com a opção ligada ele sumiria na última aba:
/// as abas ainda chegam ao painel escolhido.
#[test]
fn mover_para_um_painel_desta_janela_acerta_o_destino() {
    let pasta = Pasta::nova("painel-mover-aqui");
    pasta.configurar("terminal", "fechar_painel_vazio", true.into());
    let (mut m, a, b, c) = dois_paineis(&pasta);
    // [a] | [b] | [c]
    para_a_direita(&mut m.app, 0, &b);
    assert_eq!(abas(&m.app).len(), 3);
    let (primeiro, segundo) = (abas(&m.app)[1].clone(), abas(&m.app)[2].clone());
    assert_eq!(
        (primeiro[0].as_str(), segundo[0].as_str()),
        (b.as_str(), c.as_str())
    );
    no_menu_da_aba(&mut m.app, 0, &a, &["painel-fechar"]);
    let nome = titulo(&m.app, &c);
    m.app.select(
        "fechar-painel-destino",
        &format!("Esta janela › Painel 3 — {nome}"),
    );
    m.app.click(&format!("{DIALOGO}-primary"));
    m.app.frame();
    assert_eq!(abas(&m.app), [vec![b.clone()], vec![c.clone(), a.clone()]]);
    assert!(Session::find(&a).is_some());
    encerrar(&[&a, &b, &c]);
}

#[test]
fn sem_outro_painel_so_resta_fechar_e_a_secundaria_fecha_junto() {
    let pasta = Pasta::nova("painel-sem-destino");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let a = abas(&m.app).remove(0).remove(0);
    no_menu_da_aba(&mut m.app, 0, &a, &["painel-fechar"]);
    assert_eq!(
        m.app.text("fechar-painel-texto"),
        "Este painel tem 1 aba, e não há outro painel para recebê-la. Fechar o painel fecha a conexão."
    );
    assert!(!m.app.visible(&format!("{DIALOGO}-secondary")));
    assert!(m.app.ui().find("fechar-painel-destino").is_none());
    m.app.click(&format!("{DIALOGO}-primary"));
    m.app.frame();
    assert!(Session::find(&a).is_none());
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);

    // Na secundária com um painel só, fechar as abas leva a janela junto.
    m.app.double_click("arvore-3");
    let b = abas(&m.app).remove(0).remove(0);
    no_menu_da_aba(&mut m.app, 0, &b, &["mover-janela", "mover-janela-nova"]);
    let secundaria = m.app.windows()[1];
    m.app.switch_to(secundaria);
    m.app.set_active_window(secundaria);
    no_menu_da_aba(&mut m.app, 0, &b, &["painel-fechar"]);
    assert!(
        m.app.is_open(DIALOGO),
        "a pergunta abre na janela do painel"
    );
    m.app.click(&format!("{DIALOGO}-secondary"));
    m.app.frame();
    assert!(!secundaria.is_open());
    assert!(Session::find(&b).is_none());
}

/// Sem destino, o atalho não tem o que perguntar: fecha direto. O menu, que a pessoa escolheu com
/// o painel à vista, continua mostrando a caixa.
#[test]
fn sem_outro_painel_o_atalho_fecha_direto() {
    let pasta = Pasta::nova("painel-atalho-direto");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2").double_click("arvore-3");
    let chaves = abas(&m.app).remove(0);
    m.app.click(&format!("paineis-terminal-{}", chaves[1]));
    m.app.shortcut(Shortcut::ctrl_shift('w'));
    m.app.frame();
    assert!(!m.app.is_open(DIALOGO));
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);
    for chave in &chaves {
        assert!(Session::find(chave).is_none());
    }
}

#[test]
fn mover_o_painel_para_outra_janela_abre_um_painel_so_dele() {
    let pasta = Pasta::nova("painel-mover-janela");
    let (mut m, a, b, c) = dois_paineis(&pasta);
    let (_, outra) = segunda_janela(&mut m);
    // A outra janela já tem uma aba: [c]. Sobra [a, b] na principal.
    no_menu_da_aba(&mut m.app, 1, &c, &["mover-janela", "mover-janela-2"]);
    m.app.switch_to(m.app.windows()[0]);
    assert_eq!(abas(&m.app).len(), 2, "sem a opção, o painel fica vazio");
    // A primeira aba é a escolhida: continua sendo depois de mudar de janela.
    m.app.click(&format!("paineis-painel-0-{a}"));
    Session::find(&a).unwrap().feed(b"uptime\r\n");
    no_menu_da_aba(
        &mut m.app,
        0,
        &a,
        &["painel-mover-janela", "painel-mover-janela-2"],
    );
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);
    m.app.switch_to(outra);
    assert_eq!(abas(&m.app), [vec![c.clone()], vec![a.clone(), b.clone()]]);
    let info = m.app.panes("paineis");
    assert_eq!(info.panes[1].selected.as_deref(), Some(a.as_str()));
    assert_eq!(info.active, 1);
    assert!(m.app.session_text(&a).contains("uptime"));
    assert_eq!(m.opener.abertas.borrow().len(), 3);
    assert_eq!(m.app.focused(), Some(format!("paineis-terminal-{a}")));
    encerrar(&[&a, &b, &c]);
}

#[test]
fn mover_o_painel_para_uma_janela_nova_e_de_volta() {
    let pasta = Pasta::nova("painel-mover-nova");
    let (mut m, a, b, c) = dois_paineis(&pasta);
    no_menu_da_aba(
        &mut m.app,
        0,
        &a,
        &["painel-mover-janela", "painel-mover-janela-nova"],
    );
    let [principal, nova] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    assert_eq!(abas(&m.app), [[c.clone()]]);
    m.app.switch_to(nova);
    assert_eq!(abas(&m.app), [[a.clone(), b.clone()]]);

    // De volta: a secundária esvazia e fecha, e a principal ganha um painel ao lado do que tem.
    no_menu_da_aba(
        &mut m.app,
        0,
        &a,
        &["painel-mover-janela", "painel-mover-janela-1"],
    );
    assert!(!nova.is_open());
    m.app.switch_to(principal);
    assert_eq!(abas(&m.app), [vec![c.clone()], vec![a.clone(), b.clone()]]);
    assert_eq!(m.opener.abertas.borrow().len(), 3);
    encerrar(&[&a, &b, &c]);
}

#[test]
fn o_atalho_e_o_menu_janela_perguntam_e_o_painel_vazio_fecha_direto() {
    let pasta = Pasta::nova("painel-atalho");
    let (mut m, a, b, c) = dois_paineis(&pasta);
    m.app.click(&format!("paineis-terminal-{c}"));
    m.app.shortcut(Shortcut::ctrl_shift('w'));
    m.app.frame();
    assert!(m.app.is_open(DIALOGO), "o atalho pergunta");
    m.app.click(&format!("{DIALOGO}-close"));
    m.app.frame();
    assert_eq!(abas(&m.app), [vec![a.clone(), b.clone()], vec![c.clone()]]);
    assert!(Session::find(&c).is_some(), "cancelar não fecha nada");

    m.app.click("menus-janela").click("janela-painel-fechar");
    m.app.frame();
    assert!(m.app.is_open(DIALOGO), "o menu Janela age no painel ativo");
    m.app.assert_lint();
    m.app.click(&format!("{DIALOGO}-primary"));
    m.app.frame();
    // A lista começa no painel 1 desta janela: as abas de c foram para lá.
    assert_eq!(abas(&m.app), [[a.clone(), b.clone(), c.clone()]]);

    encerrar(&[&a, &b, &c]);
}

#[test]
fn o_painel_vazio_fecha_direto_e_o_ultimo_nao_oferece_fechar() {
    let pasta = Pasta::nova("painel-vazio");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela");
    assert!(!m.app.visible("janela-painel-fechar"));
    assert!(!m.app.visible("janela-painel-mover"));
    m.app.key(tomni_ui::Key::Escape);

    m.app.click("menus-exibir").click("layout-2x2");
    m.app.frame();
    assert_eq!(abas(&m.app).len(), 4);
    m.app.click("menus-janela").click("janela-painel-fechar");
    m.app.frame();
    assert!(!m.app.is_open(DIALOGO));
    assert_eq!(abas(&m.app).len(), 3);
}

#[test]
fn a_pergunta_passa_no_lint_no_tema_escuro() {
    let pasta = Pasta::nova("painel-lint");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Dark);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2").double_click("arvore-3");
    let chaves = abas(&m.app).remove(0);
    para_a_direita(&mut m.app, 0, &chaves[1]);
    no_menu_da_aba(&mut m.app, 0, &chaves[0], &["painel-fechar"]);
    assert!(m.app.is_open(DIALOGO));
    m.app.assert_lint();
    m.app.key(tomni_ui::Key::Escape);
    encerrar(&[&chaves[0], &chaves[1]]);
}
