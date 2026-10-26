//! O editor dos comandos ao conectar (M50) na tela: a linha "Comandos" das propriedades, o
//! diálogo, os atalhos de root, os ids que seguem a posição e o autocompletar.

mod comum;

use tirech_omni::modelo::{
    AcaoDoPasso, ComandosAoConectar, Connection, Espera, Kind, ModoComandos, Node, OrigemDaSenha,
    Passo, Tree,
};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, montar, valor};

fn proprios(passos: Vec<Passo>) -> ComandosAoConectar {
    ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos,
        reconectar: true,
    }
}

fn ssh(nome: &str) -> Connection {
    let mut c = Connection::new(nome, Kind::Ssh);
    c.host = format!("{nome}.exemplo");
    c.user = "deploy".into();
    c.key_file = Some(format!("/chaves/{nome}").into());
    c
}

fn tela(nome: &str, tree: &Tree) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(tree);
    let m = montar(&pasta, Theme::Light);
    (pasta, m)
}

fn gravados(pasta: &Pasta, id: u64) -> ComandosAoConectar {
    pasta
        .conexoes()
        .connection(id)
        .map(|c| c.comandos.clone())
        .unwrap_or_default()
}

fn texto_do_passo(p: &Passo) -> String {
    match &p.acao {
        AcaoDoPasso::Comando { texto } => texto.clone(),
        AcaoDoPasso::Senha { .. } => "<senha>".into(),
    }
}

fn textos(c: &ComandosAoConectar) -> Vec<String> {
    c.passos.iter().map(texto_do_passo).collect()
}

fn tem_id(no: &tomni_ui::Node, id: &str) -> bool {
    no.id.as_deref() == Some(id) || no.children.iter().any(|f| tem_id(f, id))
}

/// O widget existe na árvore (ao contrário de `visible`, que dá pânico se o id não existe).
fn existe(app: &TestApp, id: &str) -> bool {
    tem_id(&app.tree(), id)
}

fn abrir_editor(app: &mut TestApp) {
    escolher_propriedade(app, "Comandos");
    app.click("props-Comandos-botao");
    assert!(app.visible("comandos-dialogo"));
}

#[test]
fn a_linha_comandos_mostra_o_resumo_e_o_botao_abre_o_editor() {
    let mut tree = Tree::default();
    let mut c = ssh("web");
    c.comandos = proprios(vec![
        Passo::comando("sudo su -", 15),
        Passo::senha(OrigemDaSenha::Conexao, 15),
        Passo::comando("cd /srv", 15),
        Passo::comando("ls", 15),
    ]);
    tree.add_connection(None, c);
    tree.add_connection(None, ssh("db"));
    let (_pasta, mut m) = tela("cmd-ed-resumo", &tree);
    m.app.click("arvore-1");
    assert_eq!(valor(&m.app, "Comandos"), "sudo su -, senha, cd /srv, …");
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, "Comandos"), "Herdar (nenhum)");
    abrir_editor(&mut m.app);
    assert!(m.app.visible("comandos-modo"));
}

#[test]
fn so_conexoes_com_terminal_podem_ter_comandos() {
    let mut tree = Tree::default();
    tree.add_connection(None, ssh("web"));
    tree.add_connection(None, Connection::new("arquivos", Kind::Sftp));
    let (_pasta, mut m) = tela("cmd-ed-sftp", &tree);
    m.app.click("arvore-1");
    let ssh = m.app.properties("props");
    let linha = ssh.iter().find(|p| p.name == "Comandos").unwrap();
    assert!(!linha.hidden && linha.enabled);
    m.app.click("arvore-2");
    let sftp = m.app.properties("props");
    assert!(sftp.iter().find(|p| p.name == "Comandos").unwrap().hidden);
}

#[test]
fn editor_adiciona_move_e_salva() {
    let mut tree = Tree::default();
    tree.add_connection(None, ssh("web"));
    let (pasta, mut m) = tela("cmd-ed-salvar", &tree);
    m.app.click("arvore-1");
    abrir_editor(&mut m.app);
    m.app.click("comandos-adicionar");
    m.app.click("comandos-passo-1-texto").type_text("cd /srv");
    m.app.click("comandos-adicionar");
    m.app.click("comandos-passo-2-texto").type_text("ls");
    m.app.click("comandos-adicionar");
    // O terceiro fica vazio e é descartado ao salvar.
    m.app.click("comandos-passo-2-subir");
    assert!(m.app.visible("comandos-passo-3-texto"));
    assert!(!m.app.enabled("comandos-passo-1-subir"));
    assert!(!m.app.enabled("comandos-passo-3-descer"));
    m.app.click("comandos-dialogo-primary");
    assert!(!m.app.visible("comandos-dialogo"));
    let salvos = gravados(&pasta, 1);
    assert_eq!(salvos.modo, ModoComandos::Proprios);
    assert_eq!(textos(&salvos), ["ls", "cd /srv"]);
    assert_eq!(valor(&m.app, "Comandos"), "ls, cd /srv");
}

#[test]
fn cancelar_nao_grava_nada() {
    let mut tree = Tree::default();
    tree.add_connection(None, ssh("web"));
    let (pasta, mut m) = tela("cmd-ed-cancelar", &tree);
    m.app.click("arvore-1");
    abrir_editor(&mut m.app);
    m.app.click("comandos-adicionar");
    m.app.click("comandos-passo-1-texto").type_text("ls");
    m.app.click("comandos-dialogo-close");
    assert_eq!(gravados(&pasta, 1), ComandosAoConectar::default());
    assert_eq!(valor(&m.app, "Comandos"), "Herdar (nenhum)");
}

#[test]
fn atalho_virar_root_poe_dois_passos_no_comeco() {
    let mut tree = Tree::default();
    let mut c = ssh("web");
    c.comandos = proprios(vec![Passo::comando("cd /srv", 15)]);
    tree.add_connection(None, c);
    let (pasta, mut m) = tela("cmd-ed-root", &tree);
    m.app.click("arvore-1");
    abrir_editor(&mut m.app);
    m.app.click("comandos-root-su");
    m.app.click("comandos-dialogo-primary");
    let salvos = gravados(&pasta, 1);
    assert_eq!(textos(&salvos), ["sudo su -", "<senha>", "cd /srv"]);
    assert_eq!(
        salvos.passos[1].acao,
        AcaoDoPasso::Senha {
            origem: OrigemDaSenha::Conexao,
            texto: "password|senha".into()
        }
    );
    assert_eq!(salvos.passos[0].espera, Espera::TelaParar);
}

#[test]
fn passo_de_senha_e_espera_de_texto_vao_para_o_modelo() {
    let mut tree = Tree::default();
    tree.add_connection(None, ssh("web"));
    let (pasta, mut m) = tela("cmd-ed-espera", &tree);
    m.app.click("arvore-1");
    abrir_editor(&mut m.app);
    m.app.click("comandos-adicionar");
    m.app.click("comandos-passo-1-texto").type_text("su -");
    m.app
        .select("comandos-passo-1-espera", "Quando aparecer o texto…");
    m.app.click("comandos-passo-1-esperado").type_text("Senha:");
    // O Enter do `set_value` aciona o botão padrão do diálogo: aqui o valor vale ao perder o foco.
    m.app
        .click("comandos-passo-1-max")
        .shortcut('a')
        .type_text("30");
    m.app.click("comandos-adicionar-senha");
    assert!(m.app.visible("comandos-passo-2-senha"));
    m.app.click("comandos-dialogo-primary");
    let salvos = gravados(&pasta, 1);
    assert_eq!(salvos.passos.len(), 2);
    assert_eq!(
        salvos.passos[0].espera,
        Espera::Texto {
            texto: "Senha:".into()
        }
    );
    assert_eq!(salvos.passos[0].max_secs, 30);
    assert!(matches!(
        salvos.passos[1].acao,
        AcaoDoPasso::Senha {
            origem: OrigemDaSenha::Conexao,
            ..
        }
    ));
}

#[test]
fn remover_um_passo_reordena_os_ids() {
    let mut tree = Tree::default();
    let mut c = ssh("web");
    c.comandos = proprios(vec![
        Passo::comando("um", 15),
        Passo::comando("dois", 15),
        Passo::comando("tres", 15),
    ]);
    tree.add_connection(None, c);
    let (pasta, mut m) = tela("cmd-ed-remover", &tree);
    m.app.click("arvore-1");
    abrir_editor(&mut m.app);
    m.app.click("comandos-passo-1-remover");
    assert!(!existe(&m.app, "comandos-passo-3-texto"));
    assert!(existe(&m.app, "comandos-passo-2-texto"));
    m.app.click("comandos-dialogo-primary");
    assert_eq!(textos(&gravados(&pasta, 1)), ["dois", "tres"]);
}

#[test]
fn pasta_herdada_aparece_no_resumo_e_a_lista_fica_desabilitada() {
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Cliente A");
    if let Node::Group(g) = &mut tree.nodes[0] {
        g.comandos = proprios(vec![
            Passo::comando("sudo su -", 15),
            Passo::senha(OrigemDaSenha::Conexao, 15),
        ]);
    }
    let filha = tree.add_connection(Some(grupo), ssh("web"));
    let (pasta, mut m) = tela("cmd-ed-pasta", &tree);
    m.app.expand("arvore", &grupo.to_string());
    m.app.click(&format!("arvore-{filha}"));
    assert_eq!(
        valor(&m.app, "Comandos"),
        "Herdar (Cliente A: sudo su -, senha)"
    );
    abrir_editor(&mut m.app);
    assert!(m.app.visible("comandos-copiar-da-pasta"));
    m.app.click("comandos-copiar-da-pasta");
    assert!(existe(&m.app, "comandos-passo-2-senha"));
    m.app.click("comandos-dialogo-primary");
    let salvos = gravados(&pasta, filha);
    assert_eq!(salvos.modo, ModoComandos::Proprios);
    assert_eq!(textos(&salvos), ["sudo su -", "<senha>"]);
}

#[test]
fn a_pasta_tem_a_linha_e_grava_no_grupo() {
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Cliente A");
    tree.add_connection(Some(grupo), ssh("web"));
    let (pasta, mut m) = tela("cmd-ed-grupo", &tree);
    m.app.click(&format!("arvore-{grupo}"));
    m.app.click("props-grupo-Rede");
    comum::escolher_propriedade_em(&mut m.app, "props-grupo", "Comandos");
    m.app.click("props-grupo-Comandos-botao");
    assert!(m.app.visible("comandos-dialogo"));
    m.app.click("comandos-adicionar");
    m.app.click("comandos-passo-1-texto").type_text("cd /srv");
    m.app.click("comandos-dialogo-primary");
    let Some(Node::Group(g)) = pasta.conexoes().find(grupo).cloned() else {
        panic!("sem a pasta");
    };
    assert_eq!(g.comandos.modo, ModoComandos::Proprios);
    assert_eq!(textos(&g.comandos), ["cd /srv"]);
}

#[test]
fn autocompletar_mostra_comandos_de_outras_conexoes() {
    let mut tree = Tree::default();
    let mut a = ssh("web");
    a.comandos = proprios(vec![Passo::comando("docker compose up -d", 15)]);
    tree.add_connection(None, a);
    tree.add_connection(None, ssh("db"));
    let (_pasta, mut m) = tela("cmd-ed-sugestoes", &tree);
    m.app.click("arvore-2");
    abrir_editor(&mut m.app);
    m.app.click("comandos-adicionar");
    m.app.click("comandos-passo-1-texto").type_text("dock");
    assert!(m.app.is_open("comandos-passo-1-texto"));
    assert_eq!(
        m.app.label("comandos-passo-1-texto-docker-compose-up--d"),
        "docker compose up -d"
    );
}

/// Se a lista de origens da senha do passo `n` tem a opção "Senha da conexão".
fn oferece_senha_da_conexao(app: &mut TestApp, n: usize) -> bool {
    let id = format!("comandos-passo-{n}-senha");
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.select(&id, "Senha da conexão");
    }));
    r.is_ok()
}

fn com_passo_de_senha(mut c: Connection) -> Connection {
    c.comandos = proprios(vec![Passo::senha(OrigemDaSenha::Cofre { id: 99 }, 15)]);
    c
}

#[test]
fn a_senha_da_conexao_so_e_oferecida_onde_o_tipo_tem_credencial() {
    for (tipo, esperado) in [
        (Kind::Ssh, true),
        (Kind::Telnet, true),
        (Kind::Local, false),
    ] {
        let mut tree = Tree::default();
        let mut c = Connection::new("alvo", tipo);
        if tipo != Kind::Local {
            c.host = "alvo.exemplo".into();
        }
        tree.add_connection(None, com_passo_de_senha(c));
        let (_pasta, mut m) = tela(&format!("cmd-ed-origem-{}", tipo.nome()), &tree);
        m.app.click("arvore-1");
        abrir_editor(&mut m.app);
        assert_eq!(
            oferece_senha_da_conexao(&mut m.app, 1),
            esperado,
            "tipo {}",
            tipo.nome()
        );
    }
}

#[test]
fn a_pasta_sempre_oferece_a_senha_da_conexao() {
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Cliente A");
    if let Node::Group(g) = &mut tree.nodes[0] {
        g.comandos = proprios(vec![Passo::senha(OrigemDaSenha::Cofre { id: 99 }, 15)]);
    }
    tree.add_connection(Some(grupo), ssh("web"));
    let (_pasta, mut m) = tela("cmd-ed-origem-pasta", &tree);
    m.app.click(&format!("arvore-{grupo}"));
    m.app.click("props-grupo-Rede");
    comum::escolher_propriedade_em(&mut m.app, "props-grupo", "Comandos");
    m.app.click("props-grupo-Comandos-botao");
    assert!(m.app.visible("comandos-dialogo"));
    assert!(oferece_senha_da_conexao(&mut m.app, 1));
}

#[test]
fn passo_gravado_com_a_senha_da_conexao_no_local_continua_como_esta() {
    let mut tree = Tree::default();
    let mut c = Connection::new("sh", Kind::Local);
    c.comandos = proprios(vec![Passo::senha(OrigemDaSenha::Conexao, 15)]);
    tree.add_connection(None, c);
    let (pasta, mut m) = tela("cmd-ed-origem-local-gravado", &tree);
    m.app.click("arvore-1");
    abrir_editor(&mut m.app);
    assert_eq!(m.app.text("comandos-passo-1-senha"), "Senha da conexão");
    m.app.click("comandos-dialogo-primary");
    assert!(matches!(
        gravados(&pasta, 1).passos[0].acao,
        AcaoDoPasso::Senha {
            origem: OrigemDaSenha::Conexao,
            ..
        }
    ));
}
