//! Os links do terminal no app (M49, 14): o Ctrl+clique numa URL pede o navegador ao `Opener`, o
//! `file://` de uma aba SSH abre o painel de arquivos na pasta, o de um shell local abre a pasta
//! daqui, e com a configuração desligada nada é pedido. Os links da página Sobre também passam
//! pelo `Opener`.

mod comum;

use std::time::{Duration, Instant};

use tomni_ui::kurbo::Point;
use tomni_ui::terminal::TerminalVisual;
use tomni_ui::theme::Theme;
use tomni_ui::{Look, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Pasta, exemplo, montar};
use tirech_omni::links::DestinoDoLink;

const URL: &str = "https://example.com/pagina?a=1";

/// Uma linha com a URL no texto e uma com um link do programa (`OSC 8`) para `destino`.
fn linhas(destino: &str) -> String {
    format!("\x1b[2J\x1b[H{URL}\r\n\x1b]8;;{destino}\x1b\\arquivo\x1b]8;;\x1b\\\r\n")
}

fn visual(app: &TestApp, id: &str) -> TerminalVisual {
    match app.look(id) {
        Look::Custom(custom) => custom
            .downcast::<TerminalVisual>()
            .expect("o visual do terminal")
            .clone(),
        outro => panic!("não é o terminal: {outro:?}"),
    }
}

/// O centro da célula (`col`, `linha`) do terminal `id`.
fn celula(app: &TestApp, id: &str, col: usize, linha: usize) -> Point {
    visual(app, id)
        .cell_rect(app.rect(id), linha, col, 1)
        .center()
}

fn ctrl_clique(app: &mut TestApp, id: &str, col: usize, linha: usize) {
    let ponto = celula(app, id, col, linha);
    app.ui().set_modifiers(Modifiers::CTRL);
    app.click_at(ponto, 1);
    app.ui().set_modifiers(Modifiers::NONE);
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn ctrl_clique_numa_url_pede_o_navegador() {
    let pasta = Pasta::nova("links-url");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let terminal = "paineis-terminal-2-1";
    m.app.feed(terminal, linhas("file:///var/log/a.log"));

    ctrl_clique(&mut m.app, terminal, 12, 0);
    assert_eq!(
        *m.opener.links.borrow(),
        [DestinoDoLink::Navegador(URL.into())]
    );

    // Sem o Ctrl é um clique comum: nada é pedido.
    m.opener.links.borrow_mut().clear();
    let ponto = celula(&m.app, terminal, 12, 0);
    m.app.click_at(ponto, 1);
    assert!(m.opener.links.borrow().is_empty());
}

#[test]
fn o_file_de_uma_aba_ssh_abre_o_painel_de_arquivos_na_pasta() {
    let pasta = Pasta::nova("links-file-ssh");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.opener.servidor(2).put("/var/log/a.log", "x");
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let terminal = "paineis-terminal-2-1";
    m.app.feed(terminal, linhas("file://web-01/var/log"));

    ctrl_clique(&mut m.app, terminal, 2, 1);
    esperar(&mut m.app, "a listagem da pasta do link", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"a.log")
    });
    assert_eq!(m.app.text("navegacao"), "arquivos");
    assert!(
        m.opener.links.borrow().is_empty(),
        "o painel é da tela, não do abridor"
    );
}

#[test]
fn o_file_de_um_shell_local_pede_a_pasta_daqui() {
    let pasta = Pasta::nova("links-file-local");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-5");
    let terminal = "paineis-terminal-5-1";
    m.app
        .feed(terminal, linhas("file:///tmp/meus%20docs/a.txt"));

    ctrl_clique(&mut m.app, terminal, 2, 1);
    assert_eq!(
        *m.opener.links.borrow(),
        [DestinoDoLink::PastaLocal("/tmp/meus docs/a.txt".into())]
    );

    // O `file://` de outro computador só copia: o Ctrl+clique não pede nada.
    m.opener.links.borrow_mut().clear();
    m.app
        .feed(terminal, linhas("file://outra-maquina/tmp/a.txt"));
    ctrl_clique(&mut m.app, terminal, 2, 1);
    assert!(m.opener.links.borrow().is_empty());
}

#[test]
fn com_a_configuracao_desligada_o_ctrl_clique_nao_pede_nada() {
    let pasta = Pasta::nova("links-desligado");
    pasta.semear(&exemplo());
    pasta.configurar("terminal", "links_com_ctrl", false.into());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let terminal = "paineis-terminal-2-1";
    m.app.feed(terminal, linhas("file:///var/log/a.log"));
    ctrl_clique(&mut m.app, terminal, 12, 0);
    ctrl_clique(&mut m.app, terminal, 2, 1);
    assert!(m.opener.links.borrow().is_empty());
    assert_eq!(m.app.text("navegacao"), "terminais");
}

#[test]
fn o_erro_do_abridor_vira_um_aviso() {
    let pasta = Pasta::nova("links-erro");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.opener.link_falha.set(true);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let terminal = "paineis-terminal-2-1";
    m.app.feed(terminal, linhas("file:///var/log/a.log"));
    ctrl_clique(&mut m.app, terminal, 12, 0);
    assert_eq!(
        m.app.text("aviso-atencao"),
        "Não foi possível abrir o link: sem navegador"
    );
}

#[test]
fn a_pagina_sobre_abre_os_repositorios_no_navegador() {
    let pasta = Pasta::nova("links-sobre");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-sobre");
    m.app.click("sobre-repositorio-app");
    m.app.click("sobre-repositorio-lib");
    assert_eq!(
        *m.opener.links.borrow(),
        [
            DestinoDoLink::Navegador("https://github.com/TirechCorp/tirech-omni".into()),
            DestinoDoLink::Navegador("https://github.com/TirechCorp/tomni-ui".into()),
        ]
    );
}
