//! "Mostrar a conexão da aba na árvore", no menu Exibir: ligado, escolher uma aba escolhe a
//! conexão dela na árvore, com os grupos abertos e a linha à vista, como o "sincronizar a
//! árvore" do WinSSHTerm. Desligado por padrão; a escolha fica no `configuracoes.json`.

mod comum;

use tirech_omni::configuracoes::Configuracoes;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

/// A chave escolhida na árvore.
fn escolhida(app: &TestApp) -> Option<String> {
    app.tree_items("arvore")
        .into_iter()
        .find(|l| l.selected)
        .map(|l| l.key)
}

fn ligada(pasta: &Pasta) -> bool {
    Configuracoes::carregar(&pasta.armazenamento())
        .0
        .conexoes
        .mostrar_aba_na_arvore
}

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar(m: &Montagem) {
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

/// Abre o web-01 (id 2, dentro de "Produção"; aba `2-1`) e o shell local (id 5; aba `5-2`), fecha
/// a pasta e deixa o shell escolhido na árvore.
fn abrir_duas(m: &mut Montagem) {
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    m.app.double_click("arvore-5");
    m.app.collapse("arvore", "1");
    m.app.click("arvore-5");
    m.app.frame();
    assert_eq!(escolhida(&m.app).as_deref(), Some("5"));
}

#[test]
fn desligada_por_padrao_a_aba_nao_mexe_na_arvore() {
    let pasta = Pasta::nova("aba-na-arvore-desligada");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-exibir");
    assert!(!m.app.checked("mostrar-aba-na-arvore"));
    m.app.key(tomni_ui::Key::Escape);
    abrir_duas(&mut m);
    m.app.click("paineis-painel-0-2-1");
    assert_eq!(escolhida(&m.app).as_deref(), Some("5"));
    assert!(!m.app.tree_items("arvore").iter().any(|l| l.key == "2"));
    encerrar(&m);
}

#[test]
fn ligada_pelo_menu_grava_e_a_aba_escolhe_a_conexao_abrindo_a_pasta() {
    let pasta = Pasta::nova("aba-na-arvore-ligada");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m);
    m.app.click("menus-exibir").click("mostrar-aba-na-arvore");
    assert!(ligada(&pasta), "a escolha vai para o configuracoes.json");

    m.app.click("paineis-painel-0-2-1");
    assert_eq!(escolhida(&m.app).as_deref(), Some("2"));
    assert!(m.app.visible("arvore-2"), "a pasta Produção abriu");
    assert_ne!(
        m.app.focused().as_deref(),
        Some("arvore-2"),
        "o foco não vai para a árvore"
    );

    // Escolher outra linha na árvore não volta atrás; a próxima aba escolhida, sim.
    m.app.click("arvore-4");
    m.app.frame();
    assert_eq!(escolhida(&m.app).as_deref(), Some("4"));
    m.app.click("paineis-painel-0-5-2");
    assert_eq!(escolhida(&m.app).as_deref(), Some("5"));

    // O clique na aba que já é a ativa também volta a mostrar a conexão dela.
    m.app.click("arvore-4");
    m.app.frame();
    m.app.click("paineis-painel-0-5-2");
    assert_eq!(
        escolhida(&m.app).as_deref(),
        Some("5"),
        "a aba ativa clicada de novo"
    );

    // Desligar pelo menu grava de novo.
    m.app.click("menus-exibir").click("mostrar-aba-na-arvore");
    assert!(!ligada(&pasta));
    m.app.click("paineis-painel-0-2-1");
    assert_eq!(escolhida(&m.app).as_deref(), Some("5"));
    encerrar(&m);
}

#[test]
fn a_opcao_gravada_volta_ligada_e_rola_a_arvore_ate_a_linha() {
    let pasta = Pasta::nova("aba-na-arvore-rola");
    let mut tree = Tree::default();
    for i in 1..=80 {
        tree.add_connection(None, Connection::new(format!("shell {i:02}"), Kind::Local));
    }
    pasta.semear(&tree);
    pasta.configurar(
        "conexoes",
        "mostrar_aba_na_arvore",
        serde_json::Value::Bool(true),
    );
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-exibir");
    assert!(m.app.checked("mostrar-aba-na-arvore"));
    m.app.key(tomni_ui::Key::Escape);

    m.app.double_click("arvore-1");
    m.app.scroll("arvore-rolagem", 100_000.0);
    m.app.double_click("arvore-80");
    m.app.scroll("arvore-rolagem", -100_000.0);
    // A aba do shell 01 escolhe a linha 1, que já está à vista no topo.
    m.app.click("paineis-painel-0-1-1");
    m.app.frame();
    assert_eq!(escolhida(&m.app).as_deref(), Some("1"));
    // Com 80 linhas de 24 px, a 80 fica bem abaixo do fim da árvore (nem é montada).
    assert!(
        m.app.in_view("arvore-1"),
        "o teste começa com a árvore no topo"
    );

    m.app.click("paineis-painel-0-80-2");
    m.app.frame();
    assert_eq!(escolhida(&m.app).as_deref(), Some("80"));
    let arvore = m.app.rect("arvore");
    let linha = m.app.rect("arvore-80");
    assert!(
        linha.y0 >= arvore.y0 && linha.y1 <= arvore.y1,
        "a linha {linha:?} aparece na árvore {arvore:?}"
    );

    // Rolar a árvore para longe e clicar na aba que já é a ativa a traz de volta.
    m.app.scroll("arvore-rolagem", -100_000.0);
    m.app.frame();
    assert!(m.app.rect("arvore-80").y0 > arvore.y1, "rolou para longe");
    m.app.click("paineis-painel-0-80-2");
    m.app.frame();
    let linha = m.app.rect("arvore-80");
    assert!(
        linha.y0 >= arvore.y0 && linha.y1 <= arvore.y1,
        "a aba ativa clicada de novo traz a linha {linha:?} para a árvore {arvore:?}"
    );
    encerrar(&m);
}
