//! "Controlar com" (M63, fatia 3): o submenu do clique direito do terminal, da aba e do menu
//! Ferramentas; a aba do assistente à direita da aba alvo; a linha de comando de cada assistente;
//! o segundo clique; o aviso de ligar o controle; e o assistente que não está instalado. Nenhum
//! teste roda `claude`, `codex` nem `opencode`: o abridor é o `FakeOpener`, que só anota o que
//! a tela pediu em `locais`.

#![cfg(unix)]

mod comum;

use std::time::{Duration, Instant};

use serde_json::json;
use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::configuracoes::Configuracoes;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::Modifiers;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

fn gravada(pasta: &Pasta) -> Configuracoes {
    Configuracoes::carregar(&Armazenamento::new(pasta.caminho())).0
}

/// Um shell local salvo (id 1) aberto na aba `1-1`, com o controle `ligado` ou não.
fn cenario(nome: &str, ligado: bool) -> (Pasta, Montagem) {
    cenario_com(nome, ligado, |_| {})
}

/// O [`cenario`], com `preparar` mexendo na pasta antes de o app subir.
fn cenario_com(nome: &str, ligado: bool, preparar: impl FnOnce(&Pasta)) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("salva", Kind::Local));
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(ligado));
    pasta.configurar("ia", "mcp_avisado", json!(ligado));
    preparar(&pasta);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    m.opener
        .handle_de(Session::find("1-1").expect("a aba 1-1"))
        .unwrap()
        .feed("$ ");
    (pasta, m)
}

/// Os shells locais que o "Controlar com" abriu: o abridor também anota o shell salvo que o
/// cenário abre primeiro, que não conta.
fn lancados(m: &Montagem) -> Vec<Connection> {
    m.opener.locais.borrow().iter().skip(1).cloned().collect()
}

/// As chaves das abas, painel a painel.
fn abas(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()).collect())
        .collect()
}

/// Pelo clique direito no terminal da aba `chave`: "Controlar com ▸ {ferramenta}".
fn controlar_pelo_terminal(m: &mut Montagem, chave: &str, ferramenta: &str) {
    m.app
        .right_click_with(&format!("paineis-terminal-{chave}"), Modifiers::CTRL);
    m.app.click("terminal-controlar");
    m.app.click(&format!("terminal-controlar-{ferramenta}"));
}

#[test]
fn o_claude_abre_numa_aba_a_direita_com_a_ponte_presa_a_aba() {
    let (pasta, mut m) = cenario("controlar-claude", true);
    controlar_pelo_terminal(&mut m, "1-1", "claude");

    // A aba nova fica no painel à direita da alvo.
    let abas = abas(&m.app);
    assert_eq!(abas.len(), 2, "{abas:?}");
    assert_eq!(abas[0], ["1-1"]);
    assert_eq!(abas[1].len(), 1);

    let locais = lancados(&m);
    let [conexao] = locais.as_slice() else {
        panic!("{} shells lançados", locais.len());
    };
    assert_eq!(conexao.name, "Claude Code → salva");
    assert_eq!(conexao.local_programa, "/bin/sh");
    let args = &conexao.local_argumentos;
    assert!(args.contains("-l"), "{args}");
    assert!(args.contains("exec"), "{args}");
    assert!(args.contains("claude"), "{args}");
    assert!(args.contains("--mcp-config"), "{args}");
    assert!(args.contains("Claude Code · salva"), "{args}");
    // Começa na pasta de trabalho dos assistentes, criada na hora, e não na pasta pessoal.
    let trabalho = pasta.0.join(".cache/tirech-omni/assistentes");
    assert_eq!(conexao.local_pasta, trabalho.display().to_string());
    assert!(trabalho.is_dir());

    // O Claude Code lê a ponte de um arquivo, que aponta para a aba alvo, e o hook dos avisos de
    // outro (`--settings`), que chama a ponte com o mesmo lançamento.
    let mut arquivos: Vec<_> = std::fs::read_dir(pasta.0.join("mcp"))
        .expect("a pasta mcp")
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("lancar-"))
        })
        .collect();
    arquivos.sort();
    // Em ordem, o `lancar-<n>-avisos.json` vem antes do `lancar-<n>.json`.
    let [avisos, ponte] = arquivos.as_slice() else {
        panic!("{} arquivos de lançamento", arquivos.len());
    };
    assert!(
        avisos.to_string_lossy().ends_with("-avisos.json"),
        "{avisos:?}"
    );
    assert!(args.contains("--settings"), "{args}");
    assert!(args.contains(&*avisos.to_string_lossy()), "{args}");
    let json = std::fs::read_to_string(ponte).expect("o arquivo de lançamento");
    assert!(json.contains("--aba"), "{json}");
    assert!(json.contains("1-1"), "{json}");
    // O endereço do canal vai escrito, porque o Codex roda a ponte sem o `XDG_RUNTIME_DIR`.
    assert!(json.contains("--canal"), "{json}");
    let lancamento = json
        .split("\"--lancamento\",")
        .nth(1)
        .and_then(|resto| resto.split('"').nth(1))
        .unwrap_or_else(|| panic!("a ponte sem --lancamento: {json}"));
    let hook = std::fs::read_to_string(avisos).expect("o arquivo dos avisos");
    assert!(hook.contains("UserPromptSubmit"), "{hook}");
    assert!(hook.contains(&format!("--avisos '{lancamento}'")), "{hook}");
    // A configuração da pessoa não foi tocada.
    assert!(gravada(&pasta).ia.mcp_ligado);
}

#[test]
fn o_codex_leva_a_ponte_em_dois_c_e_o_opencode_no_ambiente() {
    let (_pasta, mut m) = cenario("controlar-codex-opencode", true);
    controlar_pelo_terminal(&mut m, "1-1", "codex");
    controlar_pelo_terminal(&mut m, "1-1", "opencode");

    let locais = lancados(&m);
    assert_eq!(locais.len(), 2);
    let (codex, opencode) = (&locais[0], &locais[1]);
    assert_eq!(codex.name, "Codex → salva");
    assert!(
        codex.local_argumentos.contains("'-c'"),
        "{}",
        codex.local_argumentos
    );
    assert!(
        codex
            .local_argumentos
            .contains("mcp_servers.tirech-omni.command"),
        "{}",
        codex.local_argumentos
    );
    assert!(codex.local_ambiente.is_empty());
    assert!(
        codex.local_argumentos.contains("--canal"),
        "{}",
        codex.local_argumentos
    );

    assert_eq!(opencode.name, "OpenCode → salva");
    assert!(
        opencode.local_argumentos.contains("opencode"),
        "{}",
        opencode.local_argumentos
    );
    let [(nome, valor)] = opencode.local_ambiente.as_slice() else {
        panic!("{:?}", opencode.local_ambiente);
    };
    assert_eq!(nome, "OPENCODE_CONFIG_CONTENT");
    assert!(valor.contains("--aba"), "{valor}");
    assert!(valor.contains("1-1"), "{valor}");
}

#[test]
fn o_caminho_e_os_argumentos_das_configuracoes_valem() {
    let pasta = Pasta::nova("controlar-configurado");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("salva", Kind::Local));
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    // O caminho tem que existir; o argumento com espaço fica numa palavra só.
    let programa = pasta.0.join("meu-claude");
    std::fs::write(&programa, "").unwrap();
    pasta.configurar(
        "ia",
        "local_claude_caminho",
        json!(programa.display().to_string()),
    );
    pasta.configurar("ia", "local_claude_argumentos", json!("--model 'opus 4'"));
    pasta.configurar("ia", "local_pasta", json!("~/obra/ia"));
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    controlar_pelo_terminal(&mut m, "1-1", "claude");

    let locais = lancados(&m);
    let args = &locais[0].local_argumentos;
    assert!(args.contains(&programa.display().to_string()), "{args}");
    assert!(args.contains("--model"), "{args}");
    assert!(args.contains("opus 4"), "{args}");
    // A pasta de trabalho escolhida, com o `~` da pasta pessoal.
    let trabalho = pasta.0.join("obra/ia");
    assert_eq!(locais[0].local_pasta, trabalho.display().to_string());
    assert!(trabalho.is_dir());
}

#[test]
fn clicar_de_novo_so_traz_a_aba_do_assistente() {
    let (_pasta, mut m) = cenario("controlar-segundo-clique", true);
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    assert_eq!(lancados(&m).len(), 1);
    assert_eq!(abas(&m.app).iter().map(Vec::len).sum::<usize>(), 2);
}

#[test]
fn fechar_a_aba_controlada_fecha_o_assistente() {
    let (_pasta, mut m) = cenario("controlar-fechar-alvo", true);
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    let assistente = abas(&m.app)[1][0].clone();
    let sessao = Session::find(&assistente).expect("a aba do assistente");
    m.app.close_tab("paineis-painel-0", "1-1");
    assert!(
        abas(&m.app).iter().all(|p| !p.contains(&assistente)),
        "{:?}",
        abas(&m.app)
    );
    assert!(sessao.closed() || Session::find(&assistente).is_none());
}

#[test]
fn fechar_o_assistente_deixa_a_aba_controlada() {
    let (_pasta, mut m) = cenario("controlar-fechar-assistente", true);
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    let assistente = abas(&m.app)[1][0].clone();
    m.app.close_tab("paineis-painel-1", &assistente);
    assert!(abas(&m.app).iter().any(|p| p.contains(&"1-1".to_owned())));
}

#[test]
fn a_aba_do_assistente_nao_oferece_o_controle() {
    let (_pasta, mut m) = cenario("controlar-sem-submenu", true);
    // Os menus de cada aba repetem os mesmos ids; o do menu Ferramentas é um só e segue a aba
    // ativa.
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert!(
        aparece(&m.app, "mcp-controlar"),
        "a aba alvo pode ser controlada"
    );
    m.app.key(tomni_ui::Key::Escape);
    m.app.key(tomni_ui::Key::Escape);

    controlar_pelo_terminal(&mut m, "1-1", "claude");
    let nova = abas(&m.app)[1][0].clone();
    m.app.click(&format!("paineis-terminal-{nova}"));
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert!(
        aparece(&m.app, "mcp-ligar"),
        "o submenu Assistentes de IA abre"
    );
    assert!(
        !aparece(&m.app, "mcp-controlar"),
        "a aba do assistente não é controlada por outro"
    );
}

#[test]
fn o_menu_da_aba_e_o_das_ferramentas_tem_o_submenu() {
    let (_pasta, mut m) = cenario("controlar-menus", true);
    m.app.context_menu("paineis-painel-0-1-1");
    m.app.click("aba-controlar").click("aba-controlar-codex");
    assert_eq!(lancados(&m).len(), 1);

    // Ferramentas > Assistentes de IA > Controlar a aba atual com: a aba ativa é a do Codex
    // (a nova), que não pode ser controlada; a alvo volta ao foco e o item volta.
    m.app.click("paineis-terminal-1-1");
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-controlar").click("mcp-controlar-opencode");
    let locais = lancados(&m);
    assert_eq!(locais.len(), 2);
    assert_eq!(locais[1].name, "OpenCode → salva");
}

#[test]
fn com_o_controle_desligado_avisa_e_depois_do_aceite_abre() {
    let (pasta, mut m) = cenario("controlar-aviso", false);
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    assert!(m.app.is_open("mcp-aviso"), "a primeira vez pergunta");
    assert!(lancados(&m).is_empty(), "ainda não abriu");
    assert!(!gravada(&pasta).ia.mcp_ligado);

    m.app.click("mcp-aviso-primary");
    assert!(gravada(&pasta).ia.mcp_ligado);
    assert_eq!(lancados(&m).len(), 1, "o aceite retoma o clique");
}

#[test]
fn cancelar_o_aviso_descarta_o_pedido() {
    let (pasta, mut m) = cenario("controlar-cancelar", false);
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    assert!(m.app.is_open("mcp-aviso"));
    m.app.click("mcp-aviso-close");
    assert!(lancados(&m).is_empty());

    // Ligar depois, pelo menu, não dispara o pedido que a pessoa cancelou.
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-ligar");
    m.app.click("mcp-aviso-primary");
    assert!(gravada(&pasta).ia.mcp_ligado);
    assert!(lancados(&m).is_empty());
}

#[test]
fn ja_avisado_liga_direto_e_oferece_desligar() {
    let pasta = Pasta::nova("controlar-liga-direto");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("salva", Kind::Local));
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(false));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    assert!(!m.app.is_open("mcp-aviso"));
    assert!(gravada(&pasta).ia.mcp_ligado);
    assert_eq!(lancados(&m).len(), 1);
    assert!(m.app.visible("controlar-aviso-ligado"));

    m.app.click("controlar-aviso-ligado-acao");
    assert!(!gravada(&pasta).ia.mcp_ligado);
}

#[test]
fn programa_nao_encontrado_fecha_a_aba_e_avisa() {
    let (_pasta, mut m) = cenario("controlar-127", true);
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    let nova = abas(&m.app)[1][0].clone();
    let sessao = Session::find(&nova).expect("a aba do assistente");
    m.opener
        .derrubar_sessao(sessao, "o processo saiu com o código 127");
    esperar(
        &mut m.app,
        |app| aparece(app, "controlar-aviso-nao-achado"),
        "o aviso não apareceu",
    );
    assert_eq!(abas(&m.app).iter().map(Vec::len).sum::<usize>(), 1);
    assert!(
        m.app
            .text("controlar-aviso-nao-achado")
            .contains("Claude Code")
    );
}

#[test]
fn outra_saida_nao_e_programa_nao_encontrado() {
    let (_pasta, mut m) = cenario("controlar-saida-normal", true);
    controlar_pelo_terminal(&mut m, "1-1", "codex");
    let nova = abas(&m.app)[1][0].clone();
    let sessao = Session::find(&nova).expect("a aba do assistente");
    m.opener
        .derrubar_sessao(sessao, "o processo saiu com o código 0");
    m.app.wait(Duration::from_millis(200));
    assert!(!aparece(&m.app, "controlar-aviso-nao-achado"));
    assert_eq!(abas(&m.app).iter().map(Vec::len).sum::<usize>(), 2);
}

// --- O diário das conversas ---

/// Grava no diário uma conversa do OpenCode com a aba da conexão 1. O OpenCode serve aqui porque
/// o teste não roda o `opencode session list`: sem a lista, a conversa vale.
fn semear_diario(pasta: &Pasta, id: &str, titulo: &str) {
    let diario = json!({
        "abas": {
            "1": [{ "ferramenta": "opencode", "id": id, "quando": 1_750_000_000, "titulo": titulo }]
        }
    });
    std::fs::write(
        pasta.caminho().join("conversas.json"),
        serde_json::to_string(&diario).unwrap(),
    )
    .unwrap();
}

/// A palavra depois de `opcao` nos argumentos guardados da conexão (que vêm com as aspas do
/// shell, duas vezes).
fn depois_de<'a>(args: &'a str, opcao: &str) -> Option<&'a str> {
    let resto = args.split(&format!("{opcao}'")).nth(1)?;
    let resto = resto.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    resto
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .next()
}

fn diario(pasta: &Pasta) -> serde_json::Value {
    std::fs::read_to_string(pasta.caminho().join("conversas.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

#[test]
fn sem_conversa_anterior_o_claude_abre_direto_e_o_id_vai_para_o_diario() {
    let (pasta, mut m) = cenario("conversas-sem-diario", true);
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    assert!(!aparece(&m.app, "conversas-dialogo"));
    let locais = lancados(&m);
    let args = &locais[0].local_argumentos;
    let id = depois_de(args, "--session-id").unwrap_or_else(|| panic!("sem --session-id: {args}"));
    assert_eq!(id.len(), 36, "{id}");
    let diario = diario(&pasta);
    let anotada = &diario["abas"]["1"][0];
    assert_eq!(anotada["ferramenta"], "claude", "{diario}");
    assert_eq!(anotada["id"], id, "{diario}");
}

#[test]
fn a_aba_que_ja_conversou_pergunta_e_continuar_leva_a_conversa() {
    let (pasta, mut m) = cenario("conversas-continuar", true);
    semear_diario(&pasta, "ses_velha", "ver o disco");
    controlar_pelo_terminal(&mut m, "1-1", "opencode");
    assert!(m.app.is_open("conversas-dialogo"));
    assert!(lancados(&m).is_empty(), "nada abre antes da resposta");
    let texto = m.app.text("conversas-texto");
    assert!(
        texto.contains("salva") && texto.contains("OpenCode"),
        "{texto}"
    );
    // A conversa mais recente já vem marcada.
    m.app.click("conversas-dialogo-primary");
    assert!(!m.app.is_open("conversas-dialogo"));
    let locais = lancados(&m);
    let [opencode] = locais.as_slice() else {
        panic!("{} shells lançados", locais.len());
    };
    assert!(
        depois_de(&opencode.local_argumentos, "--session") == Some("ses_velha"),
        "{}",
        opencode.local_argumentos
    );
    // A escolha não muda as Configurações sem o "Lembrar".
    assert_eq!(
        format!("{:?}", gravada(&pasta).ia.conversas_ao_controlar),
        "Perguntar"
    );
}

#[test]
fn nova_conversa_com_lembrar_grava_a_escolha_e_nao_pergunta_mais() {
    let (pasta, mut m) = cenario("conversas-nova", true);
    semear_diario(&pasta, "ses_velha", "ver o disco");
    controlar_pelo_terminal(&mut m, "1-1", "opencode");
    m.app.select("conversas-opcoes", "Nova conversa");
    m.app.click("conversas-lembrar");
    m.app.click("conversas-dialogo-primary");
    let locais = lancados(&m);
    assert!(
        !locais[0].local_argumentos.contains("--session"),
        "{}",
        locais[0].local_argumentos
    );
    esperar(
        &mut m.app,
        |_| format!("{:?}", gravada(&pasta).ia.conversas_ao_controlar) == "Nova",
        "a escolha não foi gravada",
    );

    // Fechado o assistente, o próximo "Controlar com" abre direto, numa conversa nova.
    let assistente = abas(&m.app)[1][0].clone();
    m.app.close_tab("paineis-painel-1", &assistente);
    controlar_pelo_terminal(&mut m, "1-1", "opencode");
    assert!(!aparece(&m.app, "conversas-dialogo"));
    assert_eq!(lancados(&m).len(), 2);
}

#[test]
fn cancelar_a_pergunta_nao_abre_nada() {
    let (pasta, mut m) = cenario("conversas-cancelar", true);
    semear_diario(&pasta, "ses_velha", "");
    controlar_pelo_terminal(&mut m, "1-1", "opencode");
    assert!(m.app.is_open("conversas-dialogo"));
    m.app.click("conversas-dialogo-close");
    assert!(!m.app.is_open("conversas-dialogo"));
    assert!(lancados(&m).is_empty());
    assert_eq!(abas(&m.app), [["1-1"]]);
}

#[test]
fn no_modo_continuar_abre_a_mais_recente_sem_perguntar() {
    let (pasta, mut m) = cenario_com("conversas-modo-continuar", true, |p| {
        p.configurar("ia", "conversas_ao_controlar", json!("continuar"));
    });
    semear_diario(&pasta, "ses_velha", "ver o disco");
    controlar_pelo_terminal(&mut m, "1-1", "opencode");
    assert!(!aparece(&m.app, "conversas-dialogo"));
    let locais = lancados(&m);
    assert!(
        depois_de(&locais[0].local_argumentos, "--session") == Some("ses_velha"),
        "{}",
        locais[0].local_argumentos
    );
}

#[test]
fn com_o_diario_desligado_nao_pergunta_nem_anota() {
    let (pasta, mut m) = cenario_com("conversas-desligado", true, |p| {
        p.configurar("ia", "conversas_lembrar", json!(false));
    });
    semear_diario(&pasta, "ses_velha", "ver o disco");
    controlar_pelo_terminal(&mut m, "1-1", "opencode");
    controlar_pelo_terminal(&mut m, "1-1", "claude");
    assert!(!aparece(&m.app, "conversas-dialogo"));
    let locais = lancados(&m);
    assert_eq!(locais.len(), 2);
    assert!(!locais[0].local_argumentos.contains("--session"));
    assert!(!locais[1].local_argumentos.contains("--session-id"));
    // O diário ficou como estava.
    assert_eq!(
        diario(&pasta)["abas"]["1"].as_array().map(Vec::len),
        Some(1)
    );
}

/// Um `estado.json` com a aba da conexão 1 controlada pelo OpenCode na conversa `ses_1`.
fn estado_com_assistente(pasta: &Pasta) {
    let estado = json!({
        "versao": 1,
        "janelas": [{
            "grade": [1, 1],
            "paineis": [[{
                "id": 1,
                "nome": "salva",
                "assistentes": [{ "ferramenta": "opencode", "conversa": "ses_1" }]
            }]]
        }]
    });
    std::fs::write(
        pasta.caminho().join("estado.json"),
        serde_json::to_string(&estado).unwrap(),
    )
    .unwrap();
}

#[test]
fn ao_reabrir_o_app_o_assistente_volta_continuando_a_conversa() {
    let pasta = Pasta::nova("conversas-restaurar");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("salva", Kind::Local));
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    estado_com_assistente(&pasta);
    let mut m = montar(&pasta, Theme::Light);
    esperar(
        &mut m.app,
        |app| abas(app).len() == 2,
        "o assistente não voltou",
    );
    let locais = lancados(&m);
    let [opencode] = locais.as_slice() else {
        panic!("{} shells lançados", locais.len());
    };
    assert_eq!(opencode.name, "OpenCode → salva");
    assert!(
        depois_de(&opencode.local_argumentos, "--session") == Some("ses_1"),
        "{}",
        opencode.local_argumentos
    );
    assert!(!aparece(&m.app, "conversas-dialogo"));
}

#[test]
fn sem_reabrir_os_assistentes_so_a_aba_volta() {
    let pasta = Pasta::nova("conversas-restaurar-desligado");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("salva", Kind::Local));
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    pasta.configurar("ia", "conversas_reabrir", json!(false));
    estado_com_assistente(&pasta);
    let mut m = montar(&pasta, Theme::Light);
    m.app.wait(Duration::from_millis(200));
    assert_eq!(abas(&m.app).len(), 1);
    assert!(lancados(&m).is_empty());
}
