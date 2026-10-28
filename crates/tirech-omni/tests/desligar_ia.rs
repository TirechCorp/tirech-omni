//! Os interruptores "Assistentes de IA" e "Controle por assistentes (MCP)" da seção Geral: com a
//! área desligada, os menus, as linhas das propriedades e os itens da árvore dela somem, e o
//! servidor MCP não sobe mesmo com `ia.mcp_ligado`. O que estava gravado fica. A página
//! Configurações tem os testes de efeito (`tests/configuracoes.rs`) e o botão e o atalho da IA
//! ficam em `tests/ia.rs`.

mod comum;

use serde_json::json;
use tirech_omni::modelo::Tree;
use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

/// O app com a árvore de exemplo (1 Produção, 2 web-01 e 3 db-01 dentro, SSH), o controle por
/// assistentes ligado na seção dele e as áreas de Geral como em `geral`.
fn cenario(nome: &str, geral: &[(&str, bool)]) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    pasta.pagina_inicial_padrao("terminais");
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    for (campo, valor) in geral {
        pasta.configurar("geral", campo, json!(valor));
    }
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    (pasta, m)
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// Os nomes das linhas à mostra na grade `grade`.
fn linhas(app: &TestApp, grade: &str) -> Vec<String> {
    app.properties(grade)
        .into_iter()
        .filter(|p| !p.hidden)
        .map(|p| p.name)
        .collect()
}

/// Os itens de IA (`ia`) e de MCP (`mcp`) que aparecem nos menus da árvore, nas propriedades e
/// nos menus do alto, nessa ordem.
fn o_que_aparece(m: &mut Montagem) -> (Vec<&'static str>, Vec<&'static str>) {
    let app = &mut m.app;
    let (mut ia, mut mcp) = (Vec::new(), Vec::new());

    app.context_menu("arvore-2");
    assert!(aparece(app, "menu-2-renomear"), "o menu da conexão abre");
    if aparece(app, "menu-2-ia") {
        ia.push("menu da conexão");
    }
    for (id, nome) in [
        ("menu-2-cluster-ia", "cluster com IA da conexão"),
        ("menu-2-mcp", "permissão da conexão"),
    ] {
        if aparece(app, id) {
            mcp.push(nome);
        }
    }
    app.key(Key::Escape);
    app.context_menu("arvore-1");
    assert!(aparece(app, "menu-1-renomear"), "o menu da pasta abre");
    for (id, nome) in [
        ("menu-1-todas-ia", "todas com IA"),
        ("menu-1-mcp", "permissão da pasta"),
    ] {
        if aparece(app, id) {
            mcp.push(nome);
        }
    }
    app.key(Key::Escape);

    app.click("arvore-2");
    let da_conexao = linhas(app, "props");
    if da_conexao.iter().any(|l| l == "Provedor de IA") {
        ia.push("provedor da conexão");
    }
    if da_conexao.iter().any(|l| l == "Instalar por") {
        ia.push("assistentes no servidor");
    }
    if da_conexao.iter().any(|l| l == "Assistentes (MCP)") {
        mcp.push("linha da conexão");
    }
    app.click("arvore-1");
    if linhas(app, "props-grupo")
        .iter()
        .any(|l| l == "Assistentes (MCP)")
    {
        mcp.push("linha da pasta");
    }

    app.click("menus-ferramentas");
    assert!(aparece(app, "scripts-menu"), "o Ferramentas abre");
    if aparece(app, "mcp-assistentes") {
        mcp.push("menu dos assistentes");
    }
    app.click("cluster-modo");
    assert!(aparece(app, "cluster-menu"));
    if aparece(app, "cluster-ia-menu") {
        mcp.push("cluster com IA do menu");
    }
    app.key(Key::Escape).key(Key::Escape);
    app.click("menus-exibir");
    assert!(aparece(app, "painel-propriedades"), "o Exibir abre");
    if aparece(app, "painel-chat") {
        ia.push("painel do chat");
    }
    app.key(Key::Escape);
    (ia, mcp)
}

const IA: [&str; 4] = [
    "menu da conexão",
    "provedor da conexão",
    "assistentes no servidor",
    "painel do chat",
];

const MCP: [&str; 7] = [
    "cluster com IA da conexão",
    "permissão da conexão",
    "todas com IA",
    "permissão da pasta",
    "linha da conexão",
    "linha da pasta",
    "menu dos assistentes",
];

#[test]
fn com_tudo_ligado_a_ia_e_o_mcp_aparecem() {
    let (_pasta, mut m) = cenario("desligar-ia-tudo-ligado", &[]);
    let (ia, mut mcp) = o_que_aparece(&mut m);
    assert_eq!(ia, IA);
    assert_eq!(mcp.pop(), Some("cluster com IA do menu"));
    assert_eq!(mcp, MCP);
}

#[test]
fn os_assistentes_de_ia_desligados_somem_e_o_mcp_fica() {
    let (_pasta, mut m) = cenario("desligar-ia-so-ia", &[("assistente_ia", false)]);
    let (ia, mut mcp) = o_que_aparece(&mut m);
    assert!(ia.is_empty(), "sobrou: {ia:?}");
    assert_eq!(mcp.pop(), Some("cluster com IA do menu"));
    assert_eq!(mcp, MCP);
}

#[test]
fn o_mcp_desligado_some_e_a_ia_fica() {
    let (_pasta, mut m) = cenario("desligar-ia-so-mcp", &[("mcp", false)]);
    let (ia, mcp) = o_que_aparece(&mut m);
    assert_eq!(ia, IA);
    assert!(mcp.is_empty(), "sobrou: {mcp:?}");
}

#[test]
fn desligar_o_mcp_mantem_as_permissoes_gravadas() {
    let mut tree: Tree = exemplo();
    if let Some(c) = tree.connection_mut(2) {
        c.mcp = tirech_omni::mcp::permissao::PermissaoIa::Livre;
    }
    let pasta = Pasta::nova("desligar-ia-mantem");
    pasta.semear(&tree);
    pasta.pagina_inicial_padrao("terminais");
    pasta.configurar("geral", "mcp", json!(false));
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.click("arvore-2");
    m.app.frame();
    assert_eq!(
        pasta.conexoes().connection(2).map(|c| c.mcp),
        Some(tirech_omni::mcp::permissao::PermissaoIa::Livre)
    );
}

// No Windows o canal é um pipe, e o teste olha o arquivo do socket.
#[cfg(unix)]
#[test]
fn o_servidor_so_sobe_com_o_mcp_ligado_em_geral() {
    use std::time::{Duration, Instant};

    let (pasta, mut m) = cenario("desligar-ia-servidor", &[("mcp", false)]);
    let socket = pasta.0.join("mcp.sock");
    for _ in 0..10 {
        m.app.wait(Duration::from_millis(20));
    }
    assert!(!socket.exists(), "o servidor subiu com o MCP desligado");

    // Ligar em Geral sobe o servidor, sem mexer no `ia.mcp_ligado`.
    m.app.click("navegacao-configuracoes");
    if m.app.label("config-expandir-tudo") == "Expandir tudo" {
        m.app.click("config-expandir-tudo");
    }
    let id = tirech_omni::configuracoes::catalogo::id_do_valor("geral.mcp");
    m.app.click(&id);
    let limite = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(Instant::now() < limite, "o servidor não subiu");
        m.app.wait(Duration::from_millis(20));
    }
}
