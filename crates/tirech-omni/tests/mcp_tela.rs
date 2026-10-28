//! O controle por assistentes (M51, fatia 6) do lado da tela: a categoria nas propriedades, o
//! submenu da árvore, o aviso de ligar, o "Como ligar", o espaço de trabalho criado pelo painel
//! de arquivos e pelo menu. A conexão aberta pela IA fica em `mcp_abrir.rs`.

mod comum;

use std::time::{Duration, Instant};

use serde_json::json;
use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::configuracoes::Configuracoes;
use tirech_omni::mcp::permissao::PermissaoIa;
use tirech_omni::modelo::{Connection, Kind, Node, Tree};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, escolher_propriedade, montar, valor};

const LINHA: &str = "Assistentes (MCP)";

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

fn gravada(pasta: &Pasta) -> Configuracoes {
    Configuracoes::carregar(&Armazenamento::new(pasta.caminho())).0
}

/// Os tipos que a pessoa abre: 1 SSH, 2 Telnet, 3 SFTP, 4 S3, 5 shell local e a pasta 6.
fn um_de_cada() -> Tree {
    let mut tree = Tree::default();
    for (nome, tipo) in [
        ("ssh", Kind::Ssh),
        ("telnet", Kind::Telnet),
        ("sftp", Kind::Sftp),
        ("s3", Kind::S3),
        ("local", Kind::Local),
    ] {
        tree.add_connection(None, Connection::new(nome, tipo));
    }
    tree.add_group(None, "Pasta");
    tree
}

/// Uma conexão SSH com chave (abre sem pedir a senha), com a permissão `mcp`.
fn ssh_com_chave(nome: &str, mcp: PermissaoIa) -> Connection {
    let mut c = Connection::new(nome, Kind::Ssh);
    c.host = format!("{nome}.exemplo");
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    c.mcp = mcp;
    c
}

/// A permissão gravada do nó `id`.
fn permissao_de(pasta: &Pasta, id: u64) -> PermissaoIa {
    match pasta.conexoes().find(id).expect("o nó") {
        Node::Connection(c) => c.mcp,
        Node::Group(g) => g.mcp,
    }
}

#[test]
fn propriedades_tem_a_categoria_mcp_em_todos_os_tipos() {
    let pasta = Pasta::nova("mcp-tela-propriedades");
    pasta.semear(&um_de_cada());
    let mut m = montar(&pasta, Theme::Light);
    for id in 1..=5 {
        m.app.click(&format!("arvore-{id}"));
        let linha = m
            .app
            .properties("props")
            .into_iter()
            .find(|p| p.name == LINHA)
            .unwrap_or_else(|| panic!("sem a linha {LINHA} na conexão {id}"));
        assert_eq!(linha.value, "Herdar (Desligada)", "conexão {id}");
        assert!(linha.enabled, "a linha da conexão {id} fica ligada");
    }
    m.app.click("arvore-6");
    let linha = m
        .app
        .properties("props-grupo")
        .into_iter()
        .find(|p| p.name == LINHA)
        .expect("a linha na pasta");
    assert!(linha.enabled);
}

#[test]
fn mudar_a_linha_das_propriedades_grava_a_permissao() {
    let pasta = Pasta::nova("mcp-tela-propriedades-muda");
    pasta.semear(&um_de_cada());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    escolher_propriedade(&mut m.app, LINHA);
    m.app.select(&format!("props-{LINHA}-valor"), "Só ler");
    assert_eq!(permissao_de(&pasta, 1), PermissaoIa::SoLer);
    assert_eq!(valor(&m.app, LINHA), "Só ler");
}

#[test]
fn menu_da_arvore_muda_a_permissao() {
    let pasta = Pasta::nova("mcp-tela-menu-arvore");
    pasta.semear(&um_de_cada());
    pasta.configurar("ia", "mcp_ligado", json!(true));
    let mut m = montar(&pasta, Theme::Light);
    for (sufixo, esperada) in [
        ("livre", PermissaoIa::Livre),
        ("so-ler", PermissaoIa::SoLer),
        ("aprovar", PermissaoIa::Aprovar),
        ("desligada", PermissaoIa::Desligada),
        ("herdar", PermissaoIa::Herdar),
    ] {
        m.app.context_menu("arvore-1");
        m.app.click("menu-1-mcp");
        m.app.click(&format!("menu-1-mcp-{sufixo}"));
        assert_eq!(permissao_de(&pasta, 1), esperada, "{sufixo}");
    }
    // A pasta também tem o submenu.
    m.app.context_menu("arvore-6");
    m.app.click("menu-6-mcp");
    m.app.click("menu-6-mcp-livre");
    assert_eq!(permissao_de(&pasta, 6), PermissaoIa::Livre);
}

#[test]
fn menu_da_arvore_so_aparece_com_o_controle_ligado() {
    let pasta = Pasta::nova("mcp-tela-menu-arvore-desligado");
    pasta.semear(&um_de_cada());
    let mut m = montar(&pasta, Theme::Light);
    m.app.context_menu("arvore-1");
    assert!(m.app.visible("menu-1-renomear"), "o menu abre");
    assert!(!m.app.visible("menu-1-mcp"), "o MCP desligado não mostra");
}

// No Windows o canal é um pipe, e o teste espera o arquivo do socket.
#[cfg(unix)]
#[test]
fn ligar_pelo_menu_mostra_o_aviso_uma_vez() {
    let pasta = Pasta::nova("mcp-tela-ligar");
    pasta.semear(&um_de_cada());
    let mut m = montar(&pasta, Theme::Light);
    let socket = pasta.0.join("mcp.sock");

    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert_eq!(m.app.text("mcp-ligar"), "Ligar o controle por assistentes");
    m.app.click("mcp-ligar");
    assert!(m.app.is_open("mcp-aviso"), "a primeira vez pergunta");
    assert!(!gravada(&pasta).ia.mcp_ligado);
    m.app.click("mcp-aviso-primary");
    assert!(gravada(&pasta).ia.mcp_ligado);
    assert!(gravada(&pasta).ia.mcp_avisado);
    esperar(&mut m.app, |_| socket.exists(), "o servidor não subiu");

    // Desligar e ligar de novo não pergunta.
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert_eq!(
        m.app.text("mcp-ligar"),
        "Desligar o controle por assistentes"
    );
    m.app.click("mcp-ligar");
    assert!(!gravada(&pasta).ia.mcp_ligado);
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-ligar");
    assert!(!m.app.is_open("mcp-aviso"), "a segunda vez liga direto");
    assert!(gravada(&pasta).ia.mcp_ligado);
}

#[test]
fn parar_a_ia_do_menu_so_aparece_com_aba_marcada() {
    let pasta = Pasta::nova("mcp-tela-parar-menu");
    pasta.semear(&um_de_cada());
    pasta.configurar("ia", "mcp_ligado", json!(true));
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert!(m.app.visible("mcp-ligar"));
    assert!(
        !m.app.visible("mcp-parar"),
        "sem aba marcada não há o que parar"
    );
}

#[test]
fn como_ligar_mostra_o_exe_e_copia() {
    let pasta = Pasta::nova("mcp-tela-como");
    pasta.semear(&um_de_cada());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-como");
    assert!(m.app.is_open("mcp-como-dialogo"));

    // O ambiente de teste fixa o executável.
    let exe = "/opt/tirech-omni/tirech-omni";
    let comando = m.app.text("mcp-como-bloco-1");
    assert!(comando.contains("claude mcp add"), "{comando}");
    assert!(comando.contains(exe), "o caminho do executável: {comando}");
    assert!(!comando.contains("--espaco"), "sem espaço escolhido");

    m.app.click("mcp-copiar-1");
    assert_eq!(m.app.clipboard().read().as_deref(), Some(comando.as_str()));

    // A aba do Codex tem os seus.
    m.app.click("mcp-como-abas-codex");
    let codex = m.app.text("mcp-como-bloco-5");
    assert!(codex.contains("codex mcp add"), "{codex}");
    m.app.click("mcp-copiar-5");
    assert_eq!(m.app.clipboard().read().as_deref(), Some(codex.as_str()));

    // A do OpenCode, o opencode.json com o prazo das ferramentas.
    m.app.click("mcp-como-abas-opencode");
    let opencode = m.app.text("mcp-como-bloco-8");
    let lido: serde_json::Value = serde_json::from_str(&opencode).expect(&opencode);
    let servidor = &lido["mcp"]["tirech-omni"];
    assert_eq!(servidor["type"], "local");
    assert_eq!(servidor["command"], json!([exe, "mcp"]));
    assert_eq!(servidor["timeout"], 86_400_000);
    m.app.click("mcp-copiar-8");
    assert_eq!(m.app.clipboard().read().as_deref(), Some(opencode.as_str()));
}

#[test]
fn como_ligar_com_espaco_leva_o_nome_aos_comandos() {
    let pasta = Pasta::nova("mcp-tela-como-espaco");
    pasta.semear(&um_de_cada());
    pasta.configurar(
        "ia",
        "espacos",
        json!([{ "nome": "proj", "conexao": 1, "raiz": "/srv/proj" }]),
    );
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-como");
    m.app.select("mcp-como-espaco", "proj");
    let comando = m.app.text("mcp-como-bloco-1");
    assert!(comando.contains("--espaco proj"), "{comando}");
    assert!(
        m.app.visible("mcp-copiar-4"),
        "o bloco que desliga as ferramentas locais"
    );
}

#[test]
fn espaco_novo_pelo_menu_abre_o_dialogo_com_a_conexao_da_aba() {
    let pasta = Pasta::nova("mcp-tela-espaco-menu");
    let mut tree = Tree::default();
    tree.add_connection(None, ssh_com_chave("web", PermissaoIa::Livre));
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-espaco");
    assert!(m.app.is_open("mcp-espaco-dialogo"));
    assert_eq!(m.app.text("mcp-espaco-conexao"), "web");
    assert_eq!(m.app.text("mcp-espaco-raiz"), "");
}

#[test]
fn espaco_novo_pelo_painel_de_arquivos() {
    let mut tree = Tree::default();
    tree.add_connection(None, ssh_com_chave("web", PermissaoIa::Livre));
    let mut sftp = Connection::new("somente-sftp", Kind::Sftp);
    sftp.host = "arquivos.exemplo".into();
    sftp.user = "ana".into();
    sftp.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, sftp);
    let pasta = Pasta::nova("mcp-tela-espaco-painel");
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);

    m.opener.servidor(1).put("/home/ana/proj/leia.txt", "oi");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-arquivos");
    esperar(
        &mut m.app,
        |a| a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"proj"),
        "a listagem",
    );
    m.app.context_menu("a1-remoto-proj");
    assert!(m.app.visible("a1-remoto-proj-mcp-espaco"));
    m.app.click("a1-remoto-proj-mcp-espaco");
    assert!(m.app.is_open("mcp-espaco-dialogo"));
    assert_eq!(m.app.text("mcp-espaco-conexao"), "web");
    assert_eq!(m.app.text("mcp-espaco-raiz"), "/home/ana/proj");
    assert_eq!(m.app.text("mcp-espaco-nome"), "proj", "o nome vem da pasta");
    m.app.click("mcp-espaco-dialogo-primary");
    let espacos = gravada(&pasta).ia.espacos;
    assert_eq!(espacos.len(), 1);
    assert_eq!(espacos[0].nome, "proj");
    assert_eq!(espacos[0].conexao, 1);
    assert_eq!(espacos[0].raiz, "/home/ana/proj");
}

#[test]
fn o_painel_de_arquivos_so_oferece_o_espaco_em_aba_ssh() {
    let mut tree = Tree::default();
    let mut sftp = Connection::new("somente-sftp", Kind::Sftp);
    sftp.host = "arquivos.exemplo".into();
    sftp.user = "ana".into();
    sftp.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, sftp);
    let pasta = Pasta::nova("mcp-tela-espaco-painel-sftp");
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.opener.servidor(1).put("/home/ana/proj/leia.txt", "oi");
    m.app.double_click("arvore-1");
    esperar(
        &mut m.app,
        |a| a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"proj"),
        "a listagem",
    );
    m.app.context_menu("a1-remoto-proj");
    assert!(m.app.visible("a1-remoto-proj-abrir"), "o menu abre");
    assert!(
        m.app.ui().find("a1-remoto-proj-mcp-espaco").is_none(),
        "o SFTP não tem como rodar comandos"
    );
}
