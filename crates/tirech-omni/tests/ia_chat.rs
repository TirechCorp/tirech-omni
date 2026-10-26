//! O chat da IA ao lado do terminal (M52, fatias b e c): o painel `chat`, uma conversa por aba,
//! o "Inserir no terminal" sem Enter, o menu do clique direito e o cancelamento.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::assistente::perfis::{ArquivoIa, PerfilIa};
use tomni_ui::Modifiers;
use tomni_ui::Shortcut;
use tomni_ui::kurbo::Point;
use tomni_ui::theme::Theme;
use tomni_ui_ai::test_util::FakeAi;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

fn semear(pasta: &Pasta, falso: &FakeAi) {
    ArquivoIa {
        perfis: vec![PerfilIa {
            id: 1,
            nome: "Falso".into(),
            url: falso.url(),
            modelo: "modelo-falso".into(),
            ..PerfilIa::default()
        }],
        ..ArquivoIa::default()
    }
    .salvar(&pasta.armazenamento())
    .expect("gravar o ia.json do teste");
    pasta.configurar("ia", "mostrar_o_que_sai", serde_json::json!(false));
}

/// A tela com o shell local aberto (a sessão `4-1`) e o foco no terminal.
fn abrir(nome: &str, falso: &FakeAi) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    semear(&pasta, falso);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-4");
    m.app.click("paineis-terminal-4-1");
    (pasta, m)
}

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Se o widget `id` existe e aparece (o `visible` do `TestApp` falha quando o id não existe).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// Se o pedido saiu e a resposta já parou de gerar (sumiu o "Pensando…").
fn resposta_pronta(app: &TestApp, falso: &FakeAi) -> bool {
    !falso.pedidos().is_empty()
        && aparece(app, "ia-chat-resposta-2")
        && !aparece(app, "ia-chat-aguardando-2")
}

fn atalho() -> Shortcut {
    "Ctrl+Shift+A".parse().expect("o atalho do catálogo")
}

/// Abre o chat pelo atalho e manda `pergunta`.
fn perguntar(app: &mut TestApp, pergunta: &str) {
    app.shortcut(atalho());
    app.wait(Duration::from_millis(300));
    assert!(app.visible("ia-chat-entrada"), "o chat não abriu");
    assert_eq!(
        app.focused().as_deref(),
        Some("ia-chat-entrada"),
        "o foco não foi para a caixa"
    );
    app.type_into("ia-chat-entrada", pergunta);
    app.click("ia-chat-enviar");
}

#[test]
fn o_atalho_abre_o_chat_e_a_resposta_chega_em_markdown() {
    let falso = FakeAi::novo();
    falso.responder(&["Use o `ls`:\n\n```bash\nls -la\n```\n"]);
    let (_pasta, mut m) = abrir("ia-chat-basico", &falso);

    perguntar(&mut m.app, "como listo os arquivos?");
    m.app.wait(Duration::from_millis(100));
    assert_eq!(
        m.app.focused().as_deref(),
        Some("ia-chat-entrada"),
        "depois de enviar o foco fica na caixa"
    );
    esperar(
        &mut m.app,
        |app| resposta_pronta(app, &falso),
        "a resposta não chegou",
    );
    esperar(
        &mut m.app,
        |app| !app.enabled("ia-chat-enviar") || app.visible("ia-chat-enviar"),
        "o botão não voltou",
    );
    assert!(
        m.app.text("ia-chat-pergunta-1").contains("como listo"),
        "o balão da pessoa"
    );
    let corpo = falso.pedidos().last().expect("um pedido").corpo.clone();
    assert!(corpo.contains("como listo os arquivos?"), "{corpo}");
    // A caixa esvazia depois de enviar.
    assert_eq!(m.app.text("ia-chat-entrada"), "");
}

#[test]
fn inserir_no_terminal_vai_para_a_aba_da_conversa_sem_enter() {
    let falso = FakeAi::novo();
    falso.responder(&["```bash\necho olá-do-chat\n```\n"]);
    let (_pasta, mut m) = abrir("ia-chat-inserir", &falso);

    perguntar(&mut m.app, "mostre um echo");
    esperar(
        &mut m.app,
        |app| aparece(app, "ia-chat-resposta-2-acao-0-0"),
        "o botão Inserir no terminal não apareceu",
    );
    m.app.click("ia-chat-resposta-2-acao-0-0");
    m.app.wait(Duration::from_millis(100));
    let tela = m.app.session_text("4-1");
    assert!(tela.contains("echo olá-do-chat"), "{tela}");
    assert!(
        !tela.contains("echo olá-do-chat\n"),
        "o comando foi enviado com Enter: {tela:?}"
    );
}

#[test]
fn parar_corta_o_pedido_em_andamento() {
    let falso = FakeAi::novo();
    falso
        .responder(&["uma resposta que demora"])
        .com_pausa(Duration::from_secs(3));
    let (_pasta, mut m) = abrir("ia-chat-parar", &falso);

    perguntar(&mut m.app, "demora");
    esperar(
        &mut m.app,
        |app| app.visible("ia-chat-parar"),
        "o botão Parar não apareceu",
    );
    m.app.click("ia-chat-parar");
    m.app.wait(Duration::from_millis(200));
    assert!(!m.app.visible("ia-chat-parar"));
    assert!(m.app.visible("ia-chat-enviar"));
}

#[test]
fn a_conversa_e_por_aba() {
    let falso = FakeAi::novo();
    falso.responder(&["resposta da primeira aba"]);
    let (_pasta, mut m) = abrir("ia-chat-por-aba", &falso);

    perguntar(&mut m.app, "pergunta da aba um");
    esperar(
        &mut m.app,
        |app| resposta_pronta(app, &falso),
        "a resposta não chegou",
    );
    // Uma segunda conexão abre outra aba, com a conversa vazia.
    m.app.double_click("arvore-4");
    m.app.wait(Duration::from_millis(300));
    assert!(!aparece(&m.app, "ia-chat-pergunta-1"), "a conversa vazou");
    assert!(m.app.visible("ia-chat-vazio"));
}

#[test]
fn sem_provedor_perguntar_abre_a_escolha() {
    let falso = FakeAi::novo();
    let pasta = Pasta::nova("ia-chat-sem-provedor");
    pasta.semear(&exemplo());
    let _ = falso;
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-4");
    m.app.click("paineis-terminal-4-1");
    m.app
        .right_click_with("paineis-terminal-4-1", Modifiers::CTRL);
    m.app.wait(Duration::from_millis(100));
    m.app.click("terminal-ia-perguntar");
    assert!(m.app.visible("ia-escolher"));
}

#[test]
fn o_menu_sem_selecao_oferece_explicar_o_erro_e_com_selecao_a_selecao() {
    let falso = FakeAi::novo();
    falso.responder(&["Porque a pasta não existe."]);
    let (_pasta, mut m) = abrir("ia-chat-menu", &falso);

    m.app.type_into(
        "paineis-terminal-4-1",
        "echo pasta-que-nao-existe-$((6*7))\n",
    );
    // O Telnet de teste só repete o que se digita.
    esperar(
        &mut m.app,
        |app| app.session_text("4-1").contains("existe-$((6*7))"),
        "o comando não apareceu",
    );
    m.app
        .right_click_with("paineis-terminal-4-1", Modifiers::CTRL);
    m.app.wait(Duration::from_millis(100));
    assert!(m.app.visible("terminal-ia-explicar-erro"));
    assert!(!m.app.visible("terminal-ia-explicar-selecao"));
    assert!(m.app.visible("terminal-ia-gerar"));
    m.app.click("terminal-ia-explicar-erro");
    m.app.wait(Duration::from_millis(100));
    esperar(
        &mut m.app,
        |_| !falso.pedidos().is_empty(),
        "o pedido não saiu",
    );
    let corpo = falso.pedidos().last().expect("um pedido").corpo.clone();
    assert!(corpo.contains("pasta-que-nao-existe"), "{corpo}");
    assert!(m.app.visible("ia-chat-entrada"), "o chat abriu");

    // Com seleção, o item é outro.
    let r = m.app.rect("paineis-terminal-4-1");
    m.app.drag(
        Point::new(r.x0 + 2.0, r.y0 + 4.0),
        Point::new(r.x0 + 300.0, r.y0 + 40.0),
    );
    m.app
        .right_click_with("paineis-terminal-4-1", Modifiers::CTRL);
    m.app.wait(Duration::from_millis(100));
    assert!(m.app.visible("terminal-ia-explicar-selecao"));
    assert!(!m.app.visible("terminal-ia-explicar-erro"));
}

#[test]
fn fechar_a_aba_descarta_a_conversa() {
    let falso = FakeAi::novo();
    falso.responder(&["resposta"]);
    let (_pasta, mut m) = abrir("ia-chat-fecha", &falso);

    perguntar(&mut m.app, "oi");
    esperar(
        &mut m.app,
        |app| resposta_pronta(app, &falso),
        "a resposta não chegou",
    );
    m.app.key_with(
        tomni_ui::Key::Char('w'),
        tomni_ui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
    );
    m.app.wait(Duration::from_millis(300));
    assert!(!aparece(&m.app, "ia-chat-resposta-2"));
}

#[test]
fn a_segunda_pergunta_leva_a_primeira_no_historico() {
    let falso = FakeAi::novo();
    falso.responder(&["primeira resposta"]);
    let (_pasta, mut m) = abrir("ia-chat-historico", &falso);

    perguntar(&mut m.app, "primeira pergunta");
    esperar(
        &mut m.app,
        |app| resposta_pronta(app, &falso),
        "a resposta não chegou",
    );
    m.app.type_into("ia-chat-entrada", "segunda pergunta");
    m.app.click("ia-chat-enviar");
    esperar(
        &mut m.app,
        |_| falso.pedidos().len() == 2,
        "o segundo pedido não saiu",
    );
    let corpo = falso.pedidos().last().expect("um pedido").corpo.clone();
    assert!(corpo.contains("primeira pergunta"), "{corpo}");
    assert!(corpo.contains("primeira resposta"), "{corpo}");
    assert!(corpo.contains("segunda pergunta"), "{corpo}");
}

#[test]
fn ver_o_que_foi_enviado_mostra_o_texto_exato() {
    let falso = FakeAi::novo();
    falso.responder(&["certo"]);
    let (_pasta, mut m) = abrir("ia-chat-ver", &falso);

    perguntar(&mut m.app, "pergunta para ver");
    esperar(
        &mut m.app,
        |app| resposta_pronta(app, &falso),
        "a resposta não chegou",
    );
    m.app.click("ia-chat-ver-1");
    assert!(m.app.visible("ia-chat-ver"));
    assert!(
        m.app
            .text("ia-chat-ver-texto")
            .contains("pergunta para ver")
    );
}
