//! Baixar o arquivo marcado no terminal, como no WinSSHTerm: com texto marcado numa aba SSH, o
//! Ctrl+clique direito pergunta onde gravar e copia do servidor (um `MemFs`) para o disco (outro
//! `MemFs`), com a barra de progresso e o aviso no fim. Sem nada marcado, ou numa aba sem
//! arquivos, o Ctrl+clique direito abre o menu, como sempre.

mod comum;

use std::time::{Duration, Instant};

use serde_json::json;

use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::kurbo::Point;
use tomni_ui::prelude::untrack;
use tomni_ui::terminal::{Session, TerminalVisual};
use tomni_ui::theme::Theme;
use tomni_ui::{Look, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

const TERMINAL: &str = "paineis-terminal-1-1";

/// O app com uma conexão SSH (id 1, `web`) aberta e a pasta `/baixados` no disco.
fn cenario(nome: &str) -> (Pasta, Montagem) {
    cenario_em(Pasta::nova(nome))
}

fn cenario_em(pasta: Pasta) -> (Pasta, Montagem) {
    let mut tree = Tree::default();
    let mut c = Connection::new("web", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, c);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a aba abrir", |_| {
        Session::find("1-1").is_some()
    });
    m.opener.disco.dir("/baixados");
    (pasta, m)
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

fn sessao() -> Session {
    Session::find("1-1").expect("a sessão da aba")
}

/// Escreve `texto` na primeira linha do terminal e marca a linha com o triplo clique.
fn marcar(m: &mut Montagem, texto: &str) {
    m.opener
        .handle_de(sessao())
        .unwrap()
        .feed(format!("\x1b[2J\x1b[H{texto}"));
    esperar(&mut m.app, "o texto chegar", |_| {
        sessao().text().contains(texto)
    });
    let ponto = match m.app.look(TERMINAL) {
        Look::Custom(custom) => custom
            .downcast::<TerminalVisual>()
            .expect("o visual do terminal")
            .cell_rect(m.app.rect(TERMINAL), 0, 0, 1)
            .center(),
        outro => panic!("não é o terminal: {outro:?}"),
    };
    m.app.click_at(Point::new(ponto.x, ponto.y), 3);
    assert!(
        untrack(|| sessao().has_selection()),
        "a linha não ficou marcada"
    );
}

/// Ctrl+clique direito, responde o diálogo de salvar com `destino` e espera o aviso `aviso`.
fn baixar_para(m: &mut Montagem, destino: &str, aviso: &str) {
    let antes = m.app.file_dialogs().len();
    m.app.right_click_with(TERMINAL, Modifiers::CTRL);
    assert!(
        !aparece(&m.app, "terminal-controlar"),
        "o Ctrl+clique direito abriu o menu"
    );
    assert_eq!(m.app.file_dialogs().len(), antes + 1, "o diálogo não abriu");
    m.app.file_dialog_answer(Some(destino));
    esperar(&mut m.app, aviso, |a| aparece(a, aviso));
}

#[test]
fn ctrl_clique_direito_baixa_o_arquivo_marcado_da_home() {
    let (_pasta, mut m) = cenario("baixar-home");
    m.opener
        .servidor(1)
        .put("/home/ana/relatorio.txt", b"conteudo do relatorio");
    marcar(&mut m, "relatorio.txt");
    baixar_para(&mut m, "/baixados/relatorio.txt", "baixar-feito");

    let pedido = m.app.file_dialogs().pop().unwrap();
    assert_eq!(pedido.file_name.as_deref(), Some("relatorio.txt"));
    assert_eq!(
        m.opener.disco.contents("/baixados/relatorio.txt"),
        Some(b"conteudo do relatorio".to_vec())
    );
    let texto = m.app.text("baixar-feito");
    assert!(texto.contains("relatorio.txt"), "{texto}");
    assert!(!aparece(&m.app, "baixar-progresso-1"), "a barra ficou");
    // O aviso tem o "Abrir a pasta", que abre a pasta onde o arquivo foi gravado.
    let arvore = m.app.tree();
    let botao = arvore.find("baixar-feito-acao").expect("o Abrir a pasta");
    assert_eq!(botao.label.as_deref(), Some("Abrir a pasta"));
    m.app.assert_lint();
}

#[test]
fn caminho_absoluto_e_relativo_a_pasta_do_osc7() {
    let (_pasta, mut m) = cenario("baixar-osc7");
    let servidor = m.opener.servidor(1);
    servidor.put("/etc/hosts", b"127.0.0.1 localhost");
    servidor.put("/var/log/app.log", b"linha 1");
    marcar(&mut m, "'/etc/hosts'");
    baixar_para(&mut m, "/baixados/hosts", "baixar-feito");
    assert_eq!(
        m.opener.disco.contents("/baixados/hosts"),
        Some(b"127.0.0.1 localhost".to_vec())
    );

    // O shell avisou pelo OSC 7 que está em /var/log: o relativo vai para lá.
    m.opener
        .handle_de(sessao())
        .unwrap()
        .feed("\x1b]7;file://web/var/log\x07");
    marcar(&mut m, "app.log");
    // O segundo aviso igual ganha o número no id.
    baixar_para(&mut m, "/baixados/app.log", "baixar-feito-2");
    assert_eq!(
        m.opener.disco.contents("/baixados/app.log"),
        Some(b"linha 1".to_vec())
    );
}

#[test]
fn uma_pasta_ou_um_arquivo_que_nao_existe_avisam() {
    let (_pasta, mut m) = cenario("baixar-erros");
    m.opener.servidor(1).dir("/home/ana/fotos");
    marcar(&mut m, "fotos");
    baixar_para(&mut m, "/baixados/fotos", "baixar-erro");
    assert!(m.app.text("baixar-erro").contains("é uma pasta"));
    assert!(
        m.app.tree().find("baixar-erro-acao").is_none(),
        "o erro não tem pasta para abrir"
    );
    assert!(!m.opener.disco.exists("/baixados/fotos"));

    marcar(&mut m, "sumiu.txt");
    baixar_para(&mut m, "/baixados/sumiu.txt", "baixar-erro-2");
    let texto = m.app.text("baixar-erro-2");
    assert!(texto.contains("sumiu.txt"), "{texto}");
    assert!(!m.opener.disco.exists("/baixados/sumiu.txt"));
}

#[test]
fn sem_nada_marcado_o_ctrl_clique_direito_abre_o_menu() {
    let (_pasta, mut m) = cenario("baixar-sem-marca");
    m.app.right_click_with(TERMINAL, Modifiers::CTRL);
    assert!(aparece(&m.app, "terminal-controlar"), "o menu não abriu");
    assert!(!aparece(&m.app, "terminal-baixar"));
    assert!(m.app.file_dialogs().is_empty());
}

#[test]
fn o_menu_do_terminal_tambem_baixa_o_marcado() {
    let pasta = Pasta::nova("baixar-menu");
    // Com o botão direito no modo "menu", o clique direito sem Ctrl abre o menu com a marca.
    pasta.configurar("terminal", "botao_direito", json!("menu"));
    let (_pasta, mut m) = cenario_em(pasta);
    m.opener.servidor(1).put("/home/ana/a.txt", b"a");
    marcar(&mut m, "a.txt");
    m.app.context_menu(TERMINAL);
    assert!(aparece(&m.app, "terminal-baixar"), "o item não apareceu");
    m.app.click("terminal-baixar");
    m.app.file_dialog_answer(Some("/baixados/a.txt"));
    esperar(&mut m.app, "o aviso", |a| aparece(a, "baixar-feito"));
    assert_eq!(
        m.opener.disco.contents("/baixados/a.txt"),
        Some(b"a".to_vec())
    );
}

#[test]
fn cancelar_para_e_apaga_o_que_comecou() {
    let (_pasta, mut m) = cenario("baixar-cancelar");
    let servidor = m.opener.servidor(1);
    servidor.put("/home/ana/grande.bin", vec![7u8; 4 * 1024 * 1024]);
    servidor.set_chunk(1024);
    servidor.set_delay(Duration::from_millis(5));
    marcar(&mut m, "grande.bin");
    m.app.right_click_with(TERMINAL, Modifiers::CTRL);
    m.app.file_dialog_answer(Some("/baixados/grande.bin"));
    esperar(&mut m.app, "a barra", |a| aparece(a, "baixar-progresso-1"));
    m.app.click("baixar-cancelar-1");
    esperar(&mut m.app, "a barra sumir", |a| {
        !aparece(a, "baixar-progresso-1")
    });
    assert!(!aparece(&m.app, "baixar-feito"));
    assert!(!aparece(&m.app, "baixar-erro"));
    esperar(&mut m.app, "o parcial sumir", |_| {
        !m.opener.disco.exists("/baixados/grande.bin")
    });
}
