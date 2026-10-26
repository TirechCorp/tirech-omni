//! O estado das conexões na árvore (M62): o selo no canto do ícone, a dica com o motivo da queda,
//! o estado mais grave na pasta recolhida e os itens "Reconectar" e "Ir para a aba" do menu de
//! contexto.

mod comum;

use std::time::Duration;

use tomni_ui::TOOLTIP_DELAY;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

/// O selo da linha `id` da árvore: "success", "caution", "critical", "neutral" ou nenhum.
fn selo(app: &TestApp, id: &str) -> Option<&'static str> {
    app.tree().find(id).and_then(|n| n.status)
}

/// O texto da dica da linha, depois do tempo de espera do ponteiro parado.
fn dica(app: &mut TestApp, id: &str) -> Option<String> {
    app.hover(id);
    app.wait(TOOLTIP_DELAY + Duration::from_millis(10));
    let dica = format!("{id}-tooltip");
    let arvore = app.tree();
    let no = arvore.find(&dica)?;
    no.children
        .first()
        .and_then(|texto| texto.text.clone())
        .filter(|texto| !texto.is_empty())
}

/// A tela com a pasta "Produção" aberta.
fn tela(nome: &str) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    (pasta, m)
}

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar(m: &Montagem) {
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn sem_sessao_nao_ha_selo() {
    let (_pasta, mut m) = tela("estado-sem-sessao");
    for id in ["arvore-2", "arvore-3", "arvore-4"] {
        assert_eq!(selo(&m.app, id), None, "{id}");
    }
    assert_eq!(dica(&mut m.app, "arvore-2"), None);
}

#[test]
fn a_conexao_aberta_fica_verde_e_a_derrubada_vermelha_com_o_motivo() {
    let (_pasta, mut m) = tela("estado-aberta-e-caida");
    m.app.double_click("arvore-2");
    m.app.double_click("arvore-3");
    m.app.frame();
    assert_eq!(selo(&m.app, "arvore-2"), Some("success"));
    assert_eq!(selo(&m.app, "arvore-3"), Some("success"));
    assert_eq!(selo(&m.app, "arvore-4"), None);
    assert_eq!(dica(&mut m.app, "arvore-2").as_deref(), Some("Conectada"));

    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();
    assert_eq!(selo(&m.app, "arvore-2"), Some("success"));
    assert_eq!(selo(&m.app, "arvore-3"), Some("critical"));
    assert_eq!(
        dica(&mut m.app, "arvore-3").as_deref(),
        Some("Caiu: tempo esgotado")
    );
    encerrar(&m);
}

#[test]
fn o_shell_que_encerrou_fica_cinza_sem_motivo() {
    let (_pasta, mut m) = tela("estado-encerrada");
    m.app.double_click("arvore-2");
    m.app.frame();
    let sessao = m.opener.sessoes.borrow()[0].0;
    m.opener.handle_de(sessao).unwrap().close(None);
    m.app.frame();
    assert_eq!(selo(&m.app, "arvore-2"), Some("neutral"));
    assert_eq!(dica(&mut m.app, "arvore-2").as_deref(), Some("Encerrada"));
    encerrar(&m);
}

#[test]
fn duas_sessoes_mostram_o_pior_estado_e_a_contagem() {
    let (_pasta, mut m) = tela("estado-duas-sessoes");
    m.app.double_click("arvore-2");
    m.app.double_click("arvore-2");
    m.app.frame();
    let sessoes = m.opener.sessoes.borrow().clone();
    assert_eq!(sessoes.len(), 2, "uma aba nova por abertura");
    assert_eq!(selo(&m.app, "arvore-2"), Some("success"));
    assert_eq!(
        dica(&mut m.app, "arvore-2").as_deref(),
        Some("2 conectadas")
    );

    m.opener.derrubar_sessao(sessoes[0].0, "conexão recusada");
    m.app.frame();
    assert_eq!(selo(&m.app, "arvore-2"), Some("critical"));
    assert_eq!(
        dica(&mut m.app, "arvore-2").as_deref(),
        Some("1 conectada, 1 caiu: conexão recusada")
    );
    encerrar(&m);
}

#[test]
fn a_pasta_recolhida_mostra_o_estado_mais_grave_de_dentro() {
    let (_pasta, mut m) = tela("estado-pasta");
    m.app.double_click("arvore-2");
    m.app.double_click("arvore-3");
    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();

    // Aberta: cada filho mostra o seu e a pasta fica sem selo.
    assert_eq!(selo(&m.app, "arvore-1"), None);
    assert_eq!(selo(&m.app, "arvore-3"), Some("critical"));

    m.app.collapse("arvore", "1");
    m.app.frame();
    assert_eq!(selo(&m.app, "arvore-1"), Some("critical"));
    assert_eq!(
        dica(&mut m.app, "arvore-1").as_deref(),
        Some("1 conectada, 1 caiu: tempo esgotado")
    );
    encerrar(&m);
}

#[test]
fn reconectar_reabre_so_as_sessoes_caidas() {
    let (_pasta, mut m) = tela("estado-reconectar");
    m.app.double_click("arvore-2");
    m.app.double_click("arvore-3");
    m.app.frame();
    let sessoes = m.opener.sessoes.borrow().clone();
    let (web, db) = (sessoes[0].0, sessoes[1].0);

    // Sem queda, o item não aparece.
    m.app.context_menu("arvore-3");
    assert!(!m.app.visible("menu-3-reconectar"));
    m.app.key(tomni_ui::Key::Escape);

    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();
    m.app.context_menu("arvore-2");
    assert!(!m.app.visible("menu-2-reconectar"), "web-01 não caiu");
    m.app.key(tomni_ui::Key::Escape);

    m.app.context_menu("arvore-3");
    assert!(m.app.visible("menu-3-reconectar"));
    m.app.click("menu-3-reconectar");
    m.app.frame();
    assert_eq!(m.opener.handle_de(db).unwrap().reopened(), 1);
    assert_eq!(m.opener.handle_de(web).unwrap().reopened(), 0);
    encerrar(&m);
}

#[test]
fn reconectar_na_pasta_reabre_as_caidas_de_dentro() {
    let (_pasta, mut m) = tela("estado-reconectar-pasta");
    m.app.double_click("arvore-2");
    m.app.double_click("arvore-3");
    m.app.frame();
    let sessoes = m.opener.sessoes.borrow().clone();
    let (web, db) = (sessoes[0].0, sessoes[1].0);
    m.opener.derrubar(2, "tempo esgotado");
    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();

    m.app.context_menu("arvore-1");
    assert!(m.app.visible("menu-1-reconectar"));
    m.app.click("menu-1-reconectar");
    m.app.frame();
    assert_eq!(m.opener.handle_de(web).unwrap().reopened(), 1);
    assert_eq!(m.opener.handle_de(db).unwrap().reopened(), 1);
    encerrar(&m);
}

#[test]
fn ir_para_a_aba_so_aparece_com_sessao_e_seleciona_a_aba_certa() {
    let (_pasta, mut m) = tela("estado-ir-para-a-aba");
    m.app.context_menu("arvore-2");
    assert!(!m.app.visible("menu-2-ir-para-aba"), "sem sessão");
    m.app.key(tomni_ui::Key::Escape);

    m.app.double_click("arvore-2");
    m.app.double_click("arvore-3");
    m.app.frame();
    let sessoes = m.opener.sessoes.borrow().clone();
    let (web, db) = (sessoes[0].0, sessoes[1].0);
    // A última aba aberta é a ativa; volta para a primeira pelo menu da conexão dela.
    m.app.context_menu("arvore-2");
    assert!(m.app.visible("menu-2-ir-para-aba"));
    m.app.click("menu-2-ir-para-aba");
    m.app.frame();
    assert_eq!(
        m.app.panes("paineis").panes[0].selected,
        Some(web.key_str())
    );

    m.app.context_menu("arvore-3");
    m.app.click("menu-3-ir-para-aba");
    m.app.frame();
    assert_eq!(m.app.panes("paineis").panes[0].selected, Some(db.key_str()));

    m.app.context_menu("arvore-1");
    assert!(
        !m.app.visible("menu-1-ir-para-aba"),
        "a pasta não tem uma aba só"
    );
    encerrar(&m);
}
