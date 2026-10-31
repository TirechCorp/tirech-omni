//! O cluster com IA: o submenu "Modo cluster" do menu Ferramentas (o normal e o com IA), o item
//! do clique direito da árvore, o diálogo com o assistente, a janela com as máquinas e o
//! assistente num painel ao lado (a ponte com `--cluster` e sem `--aba`), as abas das máquinas
//! que chegam ao assistente, e o espaço de trabalho que guarda e relança o cluster. Nenhum teste
//! roda `claude`, `codex` nem `opencode`: o abridor é o `FakeOpener`, que só anota o que a tela
//! pediu em `locais`.

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;

use std::path::PathBuf;

use serde_json::json;
use tirech_omni::espacos::{ClusterIaSalvo, Espacos};
use tirech_omni::marca::NOME_DO_APP;
use tirech_omni::modelo::Connection;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Montagem, Pasta, exemplo, montar};

/// O app com a árvore de exemplo, a pasta "Produção" aberta e o controle por assistentes
/// `ligado` ou não (desligado, também sem o aviso dado).
fn cenario(nome: &str, ligado: bool) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    pasta.configurar("ia", "mcp_ligado", json!(ligado));
    pasta.configurar("ia", "mcp_avisado", json!(ligado));
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

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// Os shells locais que a tela pediu: aqui, só os dos assistentes.
fn lancados(m: &Montagem) -> Vec<Connection> {
    m.opener.locais.borrow().clone()
}

/// Os títulos das abas da janela atual, painel a painel.
fn titulos(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()).collect())
        .collect()
}

/// As chaves das abas da janela atual, painel a painel.
fn chaves(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()).collect())
        .collect()
}

/// Os arquivos de lançamento da ponte (sem os dos avisos do Claude Code), em ordem.
fn arquivos_da_ponte(pasta: &Pasta) -> Vec<PathBuf> {
    let mut arquivos: Vec<_> = std::fs::read_dir(pasta.0.join("mcp"))
        .expect("a pasta mcp")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with("lancar-") && !n.ends_with("-avisos.json")
            })
        })
        .collect();
    arquivos.sort();
    arquivos
}

/// O `--lancamento` do arquivo da ponte.
fn lancamento_de(json: &str) -> String {
    json.split("\"--lancamento\",")
        .nth(1)
        .and_then(|resto| resto.split('"').nth(1))
        .unwrap_or_else(|| panic!("a ponte sem --lancamento: {json}"))
        .to_owned()
}

/// Pelo clique direito no grupo "Produção" (web-01 e db-01): "Abrir em cluster com IA…" e
/// "Abrir", com o assistente que o diálogo sugere.
fn abrir_cluster_do_grupo(m: &mut Montagem) {
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-cluster-ia");
    m.app.frame();
    assert!(m.app.is_open("cluster-dialogo"));
    m.app.click("cluster-abrir");
    m.app.frame();
}

#[test]
fn o_menu_ferramentas_tem_o_submenu_com_o_cluster_normal_e_o_com_ia() {
    let (_pasta, mut m) = cenario("cluster-ia-submenu", true);
    m.app.click("menus-ferramentas").click("cluster-modo");
    assert!(m.app.visible("cluster-menu"));
    assert!(m.app.visible("cluster-ia-menu"));

    // O com IA: o mesmo diálogo, com a escolha do assistente (o primeiro mostrado, de início).
    m.app.click("cluster-ia-menu");
    m.app.frame();
    assert!(m.app.is_open("cluster-dialogo"));
    assert!(m.app.visible("cluster-assistente"));
    assert_eq!(m.app.text("cluster-assistente"), "Claude Code");
    assert!(!m.app.enabled("cluster-abrir"), "nenhuma máquina escolhida");
    m.app.grid_select("cluster-grade", &["2"]);
    assert!(m.app.enabled("cluster-abrir"));
    m.app.click("cluster-dialogo-close");
    m.app.frame();

    // O normal não pergunta o assistente.
    m.app
        .click("menus-ferramentas")
        .click("cluster-modo")
        .click("cluster-menu");
    m.app.frame();
    assert!(m.app.is_open("cluster-dialogo"));
    assert!(!aparece(&m.app, "cluster-assistente"));
    encerrar(&m);
}

#[test]
fn o_cluster_com_ia_abre_as_maquinas_e_um_assistente_ao_lado_sem_envio_para_todas() {
    let (pasta, mut m) = cenario("cluster-ia-janela", true);
    abrir_cluster_do_grupo(&mut m);
    assert!(!m.app.is_open("cluster-dialogo"));
    let [principal, nova] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    assert_eq!(
        nova.title(),
        format!("{NOME_DO_APP} — Cluster com IA (2, Claude Code)")
    );
    m.app.switch_to(nova);
    // Duas máquinas e o assistente: a grade 2 × 2, uma aba por painel, o assistente no terceiro.
    let titulos = titulos(&m.app);
    assert_eq!(titulos.len(), 4, "{titulos:?}");
    assert_eq!(titulos[0], ["web-01"]);
    assert_eq!(titulos[1], ["db-01"]);
    assert_eq!(titulos[2].len(), 1, "{titulos:?}");
    assert!(titulos[3].is_empty(), "{titulos:?}");
    // O que se digita vai só para a aba ativa: quem fala com todas é o assistente.
    assert_ne!(m.app.panes("paineis").broadcast, "all");

    // Um só assistente, com a lista das máquinas nas instruções.
    let locais = lancados(&m);
    let [conexao] = locais.as_slice() else {
        panic!("{} shells lançados", locais.len());
    };
    assert_eq!(conexao.name, "Claude Code → cluster de 2 máquinas");
    let args = &conexao.local_argumentos;
    assert!(args.contains("web-01, db-01"), "{args}");

    // A ponte vai sem aba alvo, com o `--cluster` e o lançamento (as abas chegam por ele).
    let arquivos = arquivos_da_ponte(&pasta);
    let [ponte] = arquivos.as_slice() else {
        panic!("{} arquivos de lançamento", arquivos.len());
    };
    let json = std::fs::read_to_string(ponte).expect("o arquivo de lançamento");
    assert!(json.contains("--cluster"), "{json}");
    assert!(!json.contains("--aba"), "{json}");
    let _ = lancamento_de(&json);

    // A janela principal não é tocada.
    m.app.switch_to(principal);
    assert!(chaves(&m.app).iter().all(Vec::is_empty));
    encerrar(&m);
}

#[test]
fn o_assistente_escolhido_no_dialogo_e_o_que_abre() {
    let (_pasta, mut m) = cenario("cluster-ia-codex", true);
    m.app.context_menu("arvore-3");
    m.app.click("menu-3-cluster-ia");
    m.app.frame();
    m.app.select("cluster-assistente", "Codex");
    m.app.click("cluster-abrir");
    m.app.frame();
    let nova = m.app.windows()[1];
    assert_eq!(
        nova.title(),
        format!("{NOME_DO_APP} — Cluster com IA (1, Codex)")
    );
    m.app.switch_to(nova);
    // Uma máquina e o assistente: a grade 2 × 1.
    assert_eq!(titulos(&m.app).len(), 2);
    let locais = lancados(&m);
    assert_eq!(locais.len(), 1);
    assert_eq!(locais[0].name, "Codex → cluster de 1 máquina");
    // O Codex leva a ponte nos argumentos.
    assert!(
        locais[0].local_argumentos.contains("--cluster"),
        "{}",
        locais[0].local_argumentos
    );
    encerrar(&m);
}

#[test]
fn a_pasta_abre_todas_controladas_pela_ia_sem_o_dialogo() {
    let (_pasta, mut m) = cenario("cluster-ia-pasta", true);
    // Na pasta, o "Abrir" diz que abre todas; numa conexão, continua "Abrir" e sem o submenu.
    m.app.context_menu("arvore-1");
    assert_eq!(m.app.label("menu-1-abrir"), "Abrir todas");
    assert!(m.app.visible("menu-1-todas-ia"));
    m.app.key(tomni_ui::Key::Escape);
    m.app.context_menu("arvore-2");
    assert_eq!(m.app.label("menu-2-abrir"), "Abrir");
    assert!(!aparece(&m.app, "menu-2-todas-ia"));
    m.app.key(tomni_ui::Key::Escape);

    m.app.context_menu("arvore-1");
    m.app
        .click("menu-1-todas-ia")
        .click("menu-1-todas-ia-codex");
    m.app.frame();
    assert!(!m.app.is_open("cluster-dialogo"), "não passa pelo diálogo");
    let [_, nova] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    assert_eq!(
        nova.title(),
        format!("{NOME_DO_APP} — Cluster com IA (2, Codex)")
    );
    m.app.switch_to(nova);
    let titulos = titulos(&m.app);
    assert_eq!(titulos[0], ["web-01"]);
    assert_eq!(titulos[1], ["db-01"]);
    let locais = lancados(&m);
    assert_eq!(locais.len(), 1);
    assert_eq!(locais[0].name, "Codex → cluster de 2 máquinas");
    encerrar(&m);
}

#[test]
fn a_pasta_so_mostra_os_assistentes_ligados_nas_configuracoes() {
    let pasta = Pasta::nova("cluster-ia-pasta-mostrados");
    pasta.semear(&exemplo());
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    pasta.configurar("ia", "mostrar_codex", json!(false));
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-todas-ia");
    assert!(m.app.visible("menu-1-todas-ia-claude"));
    assert!(!aparece(&m.app, "menu-1-todas-ia-codex"));
    assert!(m.app.visible("menu-1-todas-ia-opencode"));
    encerrar(&m);
}

#[test]
fn com_o_controle_desligado_avisa_e_so_abre_depois_do_aceite() {
    let (_pasta, mut m) = cenario("cluster-ia-aviso", false);
    abrir_cluster_do_grupo(&mut m);
    assert!(m.app.is_open("mcp-aviso"), "a primeira vez pergunta");
    assert_eq!(m.app.windows().len(), 1, "ainda não abriu");
    assert!(lancados(&m).is_empty());

    m.app.click("mcp-aviso-primary");
    m.app.frame();
    assert_eq!(m.app.windows().len(), 2, "o aceite retoma o cluster");
    assert_eq!(lancados(&m).len(), 1);
    encerrar(&m);
}

#[test]
fn as_abas_das_maquinas_chegam_ao_assistente_do_cluster() {
    let (pasta, mut m) = cenario("cluster-ia-abas", true);
    abrir_cluster_do_grupo(&mut m);
    let nova = m.app.windows()[1];
    m.app.switch_to(nova);
    let chaves = chaves(&m.app);
    let (web, db) = (chaves[0][0].clone(), chaves[1][0].clone());
    let json = std::fs::read_to_string(&arquivos_da_ponte(&pasta)[0]).unwrap();
    let lancamento = lancamento_de(&json);

    // A ponte liga como o assistente lançado ligaria: o app reconhece o lançamento e dá a ela
    // as abas das máquinas, cada uma com o recado para o hook.
    let endereco = pasta.0.join("mcp.sock");
    let mut ia = Assistente::ligar_no_cluster(&mut m.app, &endereco, "Claude Code", &lancamento);
    let texto = assistente::avisos(&mut m.app, &endereco, &lancamento).expect("o recado");
    for chave in [&web, &db] {
        assert!(
            texto.contains(&format!("A aba {chave}")) && texto.contains("do seu cluster"),
            "{texto}"
        );
    }
    // As máquinas (Desligadas na árvore) ficam liberadas para o assistente, sem perguntar.
    for chave in [&web, &db] {
        let r = ia.chamar(&mut m.app, "ler_tela", json!({ "aba": chave }));
        assert!(!r.e_erro(), "{chave}: {}", r.texto());
    }
    let r = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": web, "texto": "uptime" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!m.app.visible("mcp-aprovar"));
    // A aba do próprio assistente não é uma máquina do cluster.
    let assistente = &titulos(&m.app)[2];
    assert_eq!(assistente.len(), 1);
    assert!(!texto.contains(&assistente[0]), "{texto}");
    encerrar(&m);
}

#[test]
fn salvar_a_janela_como_espaco_guarda_o_cluster_sem_a_aba_do_assistente_e_reabrir_relanca() {
    let (pasta, mut m) = cenario("cluster-ia-espaco", true);
    abrir_cluster_do_grupo(&mut m);
    let [_, nova] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.set_active_window(nova);
    m.app.switch_to(nova);
    m.app.click("menus-janela").click("salvar-janela-espaco");
    // O diálogo do nome abre na janela de onde veio.
    assert!(m.app.is_open("salvar-espaco-dialogo"));
    m.app.click("espaco-nome");
    m.app.shortcut(tomni_ui::prelude::Shortcut::new(
        'A',
        tomni_ui::Modifiers::CTRL,
    ));
    m.app.key(tomni_ui::Key::Delete);
    m.app.type_text("cluster");
    m.app.click("salvar-espaco-dialogo-primary");
    m.app.frame();

    let salvo = Espacos::new(pasta.caminho()).ler("cluster").unwrap();
    let [janela] = salvo.janelas.as_slice() else {
        panic!("{} janelas salvas", salvo.janelas.len());
    };
    assert_eq!(janela.grade, (2, 2));
    // A conversa do Claude Code vai junto (o id que o app escolheu), para continuar.
    let Some(ClusterIaSalvo {
        ferramenta,
        painel: 2,
        conversa: Some(conversa),
    }) = &janela.cluster_ia
    else {
        panic!("{:?}", janela.cluster_ia);
    };
    assert_eq!(ferramenta, "claude");
    assert_eq!(conversa.len(), 36, "{conversa}");
    let nomes: Vec<Vec<&str>> = janela
        .paineis
        .iter()
        .map(|p| p.iter().map(|i| i.nome.as_str()).collect())
        .collect();
    assert_eq!(nomes[0], ["web-01"]);
    assert_eq!(nomes[1], ["db-01"]);
    assert!(
        nomes.iter().skip(2).all(Vec::is_empty),
        "a aba do assistente não é salva: {nomes:?}"
    );

    // Reabrir o espaço abre outra janela do cluster e lança outro assistente, no painel dele.
    m.app.expand("arvore", "espaco:raiz");
    m.app.context_menu("arvore-espaco:item:cluster");
    m.app.click("menu-espaco:item:cluster-abrir");
    m.app.frame();
    let janelas = m.app.windows();
    assert_eq!(janelas.len(), 3);
    assert_eq!(
        janelas[2].title(),
        format!("{NOME_DO_APP} — Cluster com IA (2, Claude Code)")
    );
    assert_eq!(lancados(&m).len(), 2, "o assistente voltou");
    m.app.switch_to(janelas[2]);
    let titulos = titulos(&m.app);
    assert_eq!(titulos[0], ["web-01"]);
    assert_eq!(titulos[1], ["db-01"]);
    assert_eq!(
        titulos[2].len(),
        1,
        "o assistente no painel salvo: {titulos:?}"
    );
    let arquivos = arquivos_da_ponte(&pasta);
    assert_eq!(arquivos.len(), 2);
    let json = std::fs::read_to_string(&arquivos[1]).unwrap();
    assert!(json.contains("--cluster"), "{json}");
    encerrar(&m);
}

/// Abre o cluster com IA, fecha o app e devolve o `estado.json` gravado.
fn estado_depois_do_cluster(nome: &str, reabrir: bool) -> (String, tirech_omni::espacos::Espaco) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    pasta.configurar("ia", "conversas_reabrir", json!(reabrir));
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    abrir_cluster_do_grupo(&mut m);
    assert_eq!(lancados(&m).len(), 1);
    let sessoes: Vec<Session> = m.opener.sessoes.borrow().iter().map(|(s, _)| *s).collect();
    drop(m);
    for sessao in sessoes {
        sessao.dispose();
    }

    let texto = std::fs::read_to_string(pasta.caminho().join("estado.json")).expect("o estado");
    let valor = serde_json::from_str(&texto).unwrap();
    let estado = tirech_omni::estado::do_valor(&valor).unwrap();
    (texto, estado)
}

#[test]
fn o_estado_da_sessao_guarda_o_cluster_sem_a_aba_do_assistente() {
    let (texto, estado) = estado_depois_do_cluster("cluster-ia-estado", true);
    let cluster = estado
        .janelas
        .iter()
        .find(|j| j.paineis.iter().flatten().any(|i| i.id == Some(2)))
        .unwrap_or_else(|| panic!("a janela do cluster não foi guardada: {texto}"));
    let abas: Vec<Option<u64>> = cluster.paineis.iter().flatten().map(|i| i.id).collect();
    assert_eq!(abas, [Some(2), Some(3)], "só as máquinas: {texto}");
    let ia = cluster
        .cluster_ia
        .as_ref()
        .unwrap_or_else(|| panic!("{texto}"));
    assert_eq!(ia.ferramenta, "claude");
    assert!(ia.conversa.is_some(), "{texto}");
}

#[test]
fn sem_reabrir_os_assistentes_o_estado_nao_guarda_o_cluster() {
    let (texto, estado) = estado_depois_do_cluster("cluster-ia-estado-sem-reabrir", false);
    assert!(!texto.contains("cluster_ia"), "{texto}");
    let cluster = estado
        .janelas
        .iter()
        .find(|j| j.paineis.iter().flatten().any(|i| i.id == Some(2)))
        .unwrap_or_else(|| panic!("a janela do cluster não foi guardada: {texto}"));
    let abas: Vec<Option<u64>> = cluster.paineis.iter().flatten().map(|i| i.id).collect();
    assert_eq!(abas, [Some(2), Some(3)], "só as máquinas: {texto}");
}
