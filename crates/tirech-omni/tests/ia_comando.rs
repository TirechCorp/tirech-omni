//! "Gerar comando com IA" (M52, fatia a) e o motor de envio que ela usa: o Flyout, "O que vai
//! sair", o aviso de internet, Inserir sem Enter, o aviso de perigo e o cancelamento.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::assistente::perfis::{ArquivoIa, PerfilIa};
use tomni_ui::Shortcut;
use tomni_ui::kurbo::Point;
use tomni_ui::theme::Theme;
use tomni_ui_ai::test_util::FakeAi;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

/// Um provedor que fala com `url` (o servidor falso, na própria máquina).
fn perfil(url: &str) -> PerfilIa {
    PerfilIa {
        id: 1,
        nome: "Falso".into(),
        url: url.into(),
        modelo: "modelo-falso".into(),
        ..PerfilIa::default()
    }
}

fn semear(pasta: &Pasta, perfis: Vec<PerfilIa>) {
    ArquivoIa {
        perfis,
        ..ArquivoIa::default()
    }
    .salvar(&pasta.armazenamento())
    .expect("gravar o ia.json do teste");
}

/// A tela com o shell local aberto (a sessão `4-1`) e o foco no terminal.
fn abrir(pasta: &Pasta) -> Montagem {
    pasta.semear(&exemplo());
    let mut m = montar(pasta, Theme::Light);
    m.app.double_click("arvore-4");
    // O atalho vale com o foco no terminal.
    m.app.click("paineis-terminal-4-1");
    m
}

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Abre o Flyout pelo atalho e escreve o pedido.
fn pedir(app: &mut TestApp, texto: &str) {
    app.shortcut(atalho());
    assert!(app.visible("ia-comando"), "o Flyout não abriu");
    app.type_into("ia-comando-pedido", texto);
    app.key(tomni_ui::Key::Enter);
}

#[test]
fn o_atalho_abre_o_flyout_e_o_comando_chega_pronto_para_inserir() {
    let falso = FakeAi::novo();
    falso.responder(&[
        "```bash\nfind . -type f -size +1G\n```\n",
        "Lista os arquivos com mais de 1 GB.",
    ]);
    let pasta = Pasta::nova("ia-comando");
    semear(&pasta, vec![perfil(&falso.url())]);
    pasta.configurar("ia", "mostrar_o_que_sai", serde_json::json!(false));
    let mut m = abrir(&pasta);

    pedir(&mut m.app, "achar arquivos grandes");
    esperar(
        &mut m.app,
        |app| app.visible("ia-comando-inserir"),
        "o comando não chegou",
    );
    assert_eq!(
        m.app.text("ia-comando-sugestao"),
        "find . -type f -size +1G"
    );
    assert!(m.app.text("ia-comando-explicacao").contains("1 GB"));
    assert!(!m.app.visible("ia-comando-perigo"));

    // Chegou ao modelo: o pedido, sem nada do terminal.
    let corpo = falso.pedidos().last().expect("um pedido").corpo.clone();
    assert!(corpo.contains("achar arquivos grandes"), "{corpo}");

    // Inserir põe o comando na linha, sem Enter, e fecha.
    m.app.click("ia-comando-inserir");
    assert!(!m.app.visible("ia-comando"));
    let tela = m.app.session_text("4-1");
    assert!(tela.contains("find . -type f -size +1G"), "{tela}");
    assert!(
        !tela.contains("find . -type f -size +1G\n"),
        "o comando foi enviado com Enter: {tela:?}"
    );
}

#[test]
fn um_comando_perigoso_avisa_mas_nao_bloqueia() {
    let falso = FakeAi::novo();
    falso.responder(&["```sh\nrm -rf /\n```\nApaga tudo."]);
    let pasta = Pasta::nova("ia-comando-perigo");
    semear(&pasta, vec![perfil(&falso.url())]);
    pasta.configurar("ia", "mostrar_o_que_sai", serde_json::json!(false));
    let mut m = abrir(&pasta);

    pedir(&mut m.app, "apagar tudo");
    esperar(
        &mut m.app,
        |app| app.visible("ia-comando-inserir"),
        "o comando não chegou",
    );
    assert!(m.app.visible("ia-comando-perigo"));
    assert!(m.app.enabled("ia-comando-inserir"));
}

#[test]
fn o_que_vai_sair_aparece_uma_vez_por_aba_e_mostra_o_texto_exato() {
    let falso = FakeAi::novo();
    falso.responder(&["```sh\nls\n```\nLista."]);
    let pasta = Pasta::nova("ia-comando-sai");
    semear(&pasta, vec![perfil(&falso.url())]);
    let mut m = abrir(&pasta);

    pedir(&mut m.app, "listar");
    assert!(m.app.visible("ia-o-que-sai"));
    assert!(
        falso.pedidos().is_empty(),
        "nada pode sair antes de a pessoa aceitar"
    );
    assert!(m.app.text("ia-o-que-sai-texto").contains("listar"));
    m.app.click("ia-o-que-sai-primary");
    esperar(
        &mut m.app,
        |app| app.visible("ia-comando-inserir"),
        "o comando não chegou",
    );
    assert_eq!(falso.pedidos().len(), 1);

    // Na mesma aba, a segunda vez não pergunta.
    m.app.click("ia-comando-de-novo");
    esperar(
        &mut m.app,
        |_| falso.pedidos().len() == 2,
        "o segundo pedido não saiu",
    );
    assert!(!m.app.visible("ia-o-que-sai"));
}

#[test]
fn cancelar_o_que_vai_sair_nao_manda_nada() {
    let falso = FakeAi::novo();
    falso.responder(&["```sh\nls\n```"]);
    let pasta = Pasta::nova("ia-comando-cancela");
    semear(&pasta, vec![perfil(&falso.url())]);
    let mut m = abrir(&pasta);

    pedir(&mut m.app, "listar");
    assert!(m.app.visible("ia-o-que-sai"));
    m.app.click("ia-o-que-sai-close");
    m.app.wait(Duration::from_millis(200));
    assert!(falso.pedidos().is_empty());
    assert!(!m.app.visible("ia-comando-inserir"));
}

#[test]
fn a_selecao_do_terminal_vai_junto_e_o_x_tira() {
    let falso = FakeAi::novo();
    falso.responder(&["```sh\nls\n```"]);
    let pasta = Pasta::nova("ia-comando-selecao");
    semear(&pasta, vec![perfil(&falso.url())]);
    pasta.configurar("ia", "mostrar_o_que_sai", serde_json::json!(false));
    let mut m = abrir(&pasta);

    m.app.type_into("paineis-terminal-4-1", "erro estranho\n");
    let r = m.app.rect("paineis-terminal-4-1");
    m.app.drag(
        Point::new(r.x0 + 2.0, r.y0 + 4.0),
        Point::new(r.x0 + 300.0, r.y0 + 40.0),
    );
    pedir_sem_enviar(&mut m.app);
    assert!(m.app.visible("ia-comando-com-selecao"));
    m.app.click("ia-comando-sem-selecao");
    assert!(!m.app.visible("ia-comando-com-selecao"));
}

fn atalho() -> Shortcut {
    "Ctrl+Shift+Space".parse().expect("o atalho do catálogo")
}

fn pedir_sem_enviar(app: &mut TestApp) {
    app.shortcut(atalho());
    assert!(app.visible("ia-comando"));
}

#[test]
fn sem_provedor_o_atalho_abre_a_escolha() {
    let pasta = Pasta::nova("ia-comando-sem-provedor");
    let mut m = abrir(&pasta);
    m.app.shortcut(atalho());
    assert!(m.app.visible("ia-escolher"));
    assert!(!m.app.visible("ia-comando"));
}

#[test]
fn esc_cancela_o_pedido_em_andamento() {
    let falso = FakeAi::novo();
    falso
        .responder(&["```sh\nsleep 1\n```"])
        .com_pausa(Duration::from_secs(3));
    let pasta = Pasta::nova("ia-comando-esc");
    semear(&pasta, vec![perfil(&falso.url())]);
    pasta.configurar("ia", "mostrar_o_que_sai", serde_json::json!(false));
    let mut m = abrir(&pasta);

    pedir(&mut m.app, "demorado");
    esperar(
        &mut m.app,
        |_| !falso.pedidos().is_empty(),
        "o pedido não saiu",
    );
    m.app.key(tomni_ui::Key::Escape);
    assert!(!m.app.visible("ia-comando"));
    m.app.wait(Duration::from_millis(200));
    assert!(!m.app.visible("ia-comando-inserir"));
}

#[test]
fn o_aviso_de_internet_vem_antes_de_tudo_e_so_uma_vez_por_provedor() {
    let pasta = Pasta::nova("ia-comando-internet");
    semear(&pasta, vec![perfil("https://api.exemplo.com/v1")]);
    let mut m = abrir(&pasta);

    pedir(&mut m.app, "listar");
    assert!(m.app.visible("ia-internet"));
    assert!(m.app.text("ia-internet-texto").contains("api.exemplo.com"));
    // Cancelar fecha e não deixa "O que vai sair" aparecer.
    m.app.click("ia-internet-close");
    m.app.wait(Duration::from_millis(100));
    assert!(!m.app.visible("ia-internet"));
    assert!(!m.app.visible("ia-o-que-sai"));

    // Continuar grava a aceitação no provedor e segue para "O que vai sair".
    pedir(&mut m.app, "listar");
    assert!(m.app.visible("ia-internet"));
    m.app.click("ia-internet-primary");
    assert!(m.app.visible("ia-o-que-sai"));
    let arquivo = ArquivoIa::carregar(&pasta.armazenamento()).0;
    assert!(arquivo.perfis[0].aviso_saida_aceito);
}
