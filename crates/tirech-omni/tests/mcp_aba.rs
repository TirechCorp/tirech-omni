//! O assistente preso a uma aba (M63, fatia 2): a ponte com `--aba` faz da sessão a dona da aba,
//! dá a ela o Livre só ali (a lista de bloqueio continua valendo) e a usa quando a ferramenta
//! não diz a aba. O assistente é um cliente do canal; a aba é a de um shell local avulso, que
//! sem o clique recusa tudo (o id 0 não está na árvore).

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::Modifiers;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Montagem, Pasta, montar};

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// O app com o MCP ligado, uma conexão local salva (id 1, a permissão padrão: Desligada), o
/// shell local avulso aberto na aba `0-1` e a conexão salva na `1-2`.
fn cenario(nome: &str) -> (Pasta, Montagem, std::path::PathBuf) {
    let pasta = Pasta::nova(nome);
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("salva", Kind::Local));
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    // A reconexão no modo padrão (Oferecer), com a espera mais curta, para os testes da queda.
    pasta.configurar("conexoes", "reconectar", json!({ "espera_secs": 1 }));
    let endereco = pasta.0.join("mcp.sock");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-ferramentas").click("abrir-shell");
    m.app.double_click("arvore-1");
    for chave in ["0-1", "1-2"] {
        let sessao = Session::find(chave).unwrap_or_else(|| panic!("a aba {chave} não abriu"));
        m.opener.handle_de(sessao).unwrap().feed("$ ");
    }
    (pasta, m, endereco)
}

/// O primeiro JSON de um texto de resposta (o resto são as marcas e o texto do servidor).
fn primeiro_json(texto: &str) -> Value {
    let inicio = texto
        .find(['{', '['])
        .unwrap_or_else(|| panic!("sem JSON em: {texto}"));
    serde_json::Deserializer::from_str(&texto[inicio..])
        .into_iter::<Value>()
        .next()
        .and_then(Result::ok)
        .unwrap_or_else(|| panic!("JSON inválido em: {texto}"))
}

#[test]
fn a_sessao_presa_age_no_shell_avulso_sem_dizer_a_aba() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-avulso");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let tela = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!tela.e_erro(), "{}", tela.texto());
    let digitou = ia.chamar(&mut m.app, "digitar", json!({ "texto": "ls\n" }));
    assert!(!digitou.e_erro(), "{}", digitou.texto());
    assert!(
        !m.app.visible("mcp-aprovar"),
        "o clique deu o Livre nesta aba"
    );
}

#[test]
fn sem_o_clique_o_shell_avulso_continua_recusado() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-sem-clique");
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Claude Code");
    let r = ia.chamar(&mut m.app, "ler_tela", json!({ "aba": "0-1" }));
    assert!(r.e_erro());
    assert!(r.texto().contains("não está liberada"), "{}", r.texto());
}

#[test]
fn a_liberacao_vale_so_na_aba_alvo() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-so-nela");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    // A outra aba é de uma conexão Desligada: segue recusada, com o argumento e sem ele.
    let r = ia.chamar(&mut m.app, "ler_tela", json!({ "aba": "1-2" }));
    assert!(r.e_erro());
    assert!(r.texto().contains("não está liberada"), "{}", r.texto());
    let r = ia.chamar(&mut m.app, "digitar", json!({ "aba": "1-2", "texto": "x" }));
    assert!(r.e_erro());
    // A permissão salva da conexão não mudou, e a lista só mostra a aba alvo.
    let abas = ia.chamar(&mut m.app, "listar_abas", json!({}));
    let lista = primeiro_json(&abas.texto());
    let lista = lista.as_array().unwrap();
    assert_eq!(lista.len(), 1, "{lista:?}");
    assert_eq!(lista[0]["aba"], "0-1");
    assert_eq!(lista[0]["alvo"], true);
}

#[test]
fn a_lista_de_bloqueio_continua_perguntando() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-bloqueio");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let id = ia.pedir("digitar", json!({ "texto": "rm -rf /tmp/x\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert!(m.app.visible("mcp-aprovar-perigo"));
    m.app.click("mcp-aprovar-primary");
    let r = ia.resposta(&mut m.app, id);
    assert!(r.e_erro(), "recusado pela pessoa");
}

#[test]
fn o_clique_vence_o_dono_anterior() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dono");
    let mut primeira = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let r = primeira.chamar(&mut m.app, "digitar", json!({ "texto": "a" }));
    assert!(!r.e_erro(), "{}", r.texto());
    // Outro assistente clicado na mesma aba toma o dono, sem perguntar.
    let mut segunda = Assistente::ligar_na_aba(&mut m.app, &endereco, "Codex", "0-1");
    let r = segunda.chamar(&mut m.app, "digitar", json!({ "texto": "b" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!m.app.visible("mcp-aprovar"));
    // E agora é a primeira que muda uma aba de outra sessão: pergunta.
    let id = primeira.pedir("digitar", json!({ "texto": "c" }));
    esperar(&mut m.app, |a| a.visible("mcp-aprovar"), "devia perguntar");
    m.app.click("mcp-aprovar-primary");
    let _ = primeira.resposta(&mut m.app, id);
}

#[test]
fn ao_sair_a_liberacao_e_o_dono_somem() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-fim");
    let ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    ia.desligar();
    // Uma sessão sem `--aba` não herda nada: a aba volta a ser recusada e fica sem dono.
    let mut outra = Assistente::ligar(&mut m.app, &endereco, "Outra");
    let r = outra.chamar(&mut m.app, "ler_tela", json!({ "aba": "0-1" }));
    assert!(r.e_erro());
    assert!(r.texto().contains("não está liberada"), "{}", r.texto());
}

#[test]
fn fechar_a_aba_alvo_deixa_o_erro_de_aba_inexistente() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-fechada");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let fechou = ia.chamar(&mut m.app, "fechar_aba", json!({}));
    assert!(!fechou.e_erro(), "{}", fechou.texto());
    let r = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(r.e_erro());
    assert!(r.texto().contains("0-1"), "{}", r.texto());
    assert!(r.texto().contains("não existe"), "{}", r.texto());
}

/// O estado da aba `0-1` em `listar_abas`.
fn estado_na_lista(ia: &mut Assistente, app: &mut TestApp) -> Value {
    let abas = ia.chamar(app, "listar_abas", json!({}));
    let lista = primeiro_json(&abas.texto());
    lista
        .as_array()
        .and_then(|l| l.iter().find(|a| a["aba"] == "0-1"))
        .cloned()
        .unwrap_or_else(|| panic!("a aba 0-1 não está na lista: {lista}"))
}

#[test]
fn a_aba_do_assistente_que_cai_reconecta_sozinha_mesmo_no_modo_oferecer() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-reconecta");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let tela = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!tela.e_erro(), "{}", tela.texto());
    let sessao = Session::find("0-1").unwrap();
    m.opener.derrubar_sessao(sessao, "conexão perdida");
    m.app.wait(Duration::from_millis(50));
    // Enquanto espera, a lista e as ferramentas dizem que está reconectando, com a tentativa.
    let aba = estado_na_lista(&mut ia, &mut m.app);
    assert_eq!(aba["estado"], "reconectando", "{aba}");
    assert_eq!(aba["tentativa"], 1, "{aba}");
    assert_eq!(aba["tentativas"], 3, "{aba}");
    let r = ia.chamar(&mut m.app, "digitar", json!({ "texto": "ls\n" }));
    assert!(r.e_erro());
    assert!(r.texto().contains("tentativa 1 de 3"), "{}", r.texto());
    // Passada a espera, a mesma aba reabre, sem o aviso de "Reconectar" que a pessoa veria.
    let handle = m.opener.handle_de(sessao).unwrap();
    esperar(
        &mut m.app,
        |_| handle.reopened() == 1,
        "a aba do assistente não reconectou",
    );
    assert!(!sessao.closed());
    let aba = estado_na_lista(&mut ia, &mut m.app);
    assert_ne!(aba["estado"], "reconectando", "{aba}");
}

#[test]
fn a_aba_sem_assistente_continua_so_oferecendo() {
    let (_pasta, mut m, _endereco) = cenario("mcp-aba-so-oferece");
    let sessao = Session::find("1-2").unwrap();
    m.opener.derrubar_sessao(sessao, "conexão perdida");
    m.app.wait(Duration::from_millis(1500));
    assert_eq!(m.opener.handle_de(sessao).unwrap().reopened(), 0);
    assert!(sessao.closed());
}

#[test]
fn reconectar_aba_reabre_a_caida_e_recusa_a_de_pe() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-reconectar");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    // A primeira chamada já registra o assistente na aba: a queda marca a reconexão sozinha.
    let de_pe = ia.chamar(&mut m.app, "reconectar_aba", json!({ "aba": "0-1" }));
    assert!(!de_pe.e_erro(), "{}", de_pe.texto());
    assert!(
        de_pe.texto().contains("já está conectada"),
        "{}",
        de_pe.texto()
    );
    let sessao = Session::find("0-1").unwrap();
    let handle = m.opener.handle_de(sessao).unwrap();
    m.opener.derrubar_sessao(sessao, "conexão perdida");
    m.app.wait(Duration::from_millis(50));
    assert_eq!(
        estado_na_lista(&mut ia, &mut m.app)["estado"],
        "reconectando"
    );
    // Antes da espera da reconexão sozinha: a ferramenta reabre na hora, e a marcada não repete.
    let id = ia.pedir("reconectar_aba", json!({ "aba": "0-1" }));
    esperar(&mut m.app, |_| handle.reopened() == 1, "não reabriu");
    handle.feed("$ ");
    let r = ia.resposta(&mut m.app, id);
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(
        r.texto().contains("\"estado\":\"conectada\""),
        "{}",
        r.texto()
    );
    m.app.wait(Duration::from_millis(1500));
    assert_eq!(handle.reopened(), 1, "a reconexão marcada rodou de novo");
    assert!(!sessao.closed());
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// Por Ferramentas > Assistentes de IA > "Dar a aba atual também a ▸", com a aba `chave` na
/// frente, a sessão do lugar `lugar` (a 1ª é 1). Os menus das abas e dos terminais repetem os ids
/// de uma aba para outra, então só este serve com duas abas abertas.
fn dar_pelo_menu(m: &mut Montagem, chave: &str, lugar: usize) {
    abrir_dar_das_ferramentas(m, chave);
    m.app.click(&format!("mcp-controlar-dar-{lugar}"));
}

/// Abre o submenu "Dar a aba atual também a" das Ferramentas, com a aba `chave` na frente.
fn abrir_dar_das_ferramentas(m: &mut Montagem, chave: &str) {
    m.app.click(&format!("paineis-painel-0-{chave}"));
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-controlar-dar");
}

/// O que a aba `chave` recebeu do assistente desde a última leitura.
fn recebido(m: &Montagem, chave: &str) -> String {
    let sessao = Session::find(chave).unwrap();
    let bytes = m.opener.handle_de(sessao).unwrap().take_written();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn dar_a_aba_pelo_menu_libera_so_nela_sem_mudar_a_padrao() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dar");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    // A aba da conexão Desligada segue recusada até o clique.
    let r = ia.chamar(&mut m.app, "digitar", json!({ "aba": "1-2", "texto": "a" }));
    assert!(r.e_erro());
    assert!(r.texto().contains("não está liberada"), "{}", r.texto());

    dar_pelo_menu(&mut m, "1-2", 1);
    let _ = recebido(&m, "1-2");
    let r = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-2", "texto": "ls" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    // A sessão fica sabendo da aba dada na resposta seguinte, uma vez só.
    assert!(
        r.texto().contains("deu a você também a aba 1-2"),
        "{}",
        r.texto()
    );
    assert!(
        !m.app.visible("mcp-aprovar"),
        "o clique deu o Livre nesta aba"
    );
    assert!(recebido(&m, "1-2").contains("ls"));

    // Sem dizer a aba, a ferramenta continua na padrão, a do "Controlar com".
    let _ = recebido(&m, "0-1");
    let r = ia.chamar(&mut m.app, "digitar", json!({ "texto": "pwd" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!r.texto().contains("deu a você"), "{}", r.texto());
    assert!(recebido(&m, "0-1").contains("pwd"));
    assert!(!recebido(&m, "1-2").contains("pwd"));
}

#[test]
fn o_hook_avisa_da_aba_dada_antes_da_proxima_ferramenta() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dar-hook");
    let mut ia = Assistente::ligar_lancado(&mut m.app, &endereco, "Claude Code", "0-1", "7-1");
    let _ = ia.chamar(&mut m.app, "ler_tela", json!({}));
    dar_pelo_menu(&mut m, "1-2", 1);
    let texto = assistente::avisos(&mut m.app, &endereco, "7-1").expect("o recado");
    assert!(texto.contains("deu a você também a aba 1-2"), "{texto}");
    // O recado sai da fila: nem o hook nem a próxima ferramenta repetem.
    assert_eq!(assistente::avisos(&mut m.app, &endereco, "7-1"), None);
    let r = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!r.texto().contains("deu a você"), "{}", r.texto());
}

#[test]
fn a_lista_de_bloqueio_continua_perguntando_na_aba_dada() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dar-bloqueio");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let _ = ia.chamar(&mut m.app, "ler_tela", json!({}));
    dar_pelo_menu(&mut m, "1-2", 1);
    let id = ia.pedir(
        "digitar",
        json!({ "aba": "1-2", "texto": "rm -rf /tmp/x\n" }),
    );
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert!(m.app.visible("mcp-aprovar-perigo"));
    m.app.click("mcp-aprovar-primary");
    let r = ia.resposta(&mut m.app, id);
    assert!(r.e_erro(), "recusado pela pessoa");
}

#[test]
fn o_listar_abas_mostra_a_alvo_e_a_dada() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dar-lista");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let _ = ia.chamar(&mut m.app, "ler_tela", json!({}));
    dar_pelo_menu(&mut m, "1-2", 1);
    let abas = ia.chamar(&mut m.app, "listar_abas", json!({}));
    let lista = primeiro_json(&abas.texto());
    let lista = lista.as_array().unwrap();
    assert_eq!(lista.len(), 2, "{lista:?}");
    let alvo = lista.iter().find(|a| a["aba"] == "0-1").unwrap();
    assert_eq!(alvo["alvo"], true);
    assert!(alvo.get("dada").is_none(), "{alvo}");
    let dada = lista.iter().find(|a| a["aba"] == "1-2").unwrap();
    assert_eq!(dada["alvo"], false);
    assert_eq!(dada["dada"], true);
    assert_eq!(dada["minha"], true, "a sessão virou a dona da aba");
}

#[test]
fn dar_a_aba_tira_o_dono_anterior_sem_perguntar() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dar-dono");
    let mut primeira = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let mut segunda = Assistente::ligar_na_aba(&mut m.app, &endereco, "Codex", "0-1");
    let _ = primeira.chamar(&mut m.app, "ler_tela", json!({}));
    let _ = segunda.chamar(&mut m.app, "ler_tela", json!({}));
    // O Codex é o 2º da lista: ele leva a 1-2 e depois a primeira também a leva, sem perguntar.
    dar_pelo_menu(&mut m, "1-2", 2);
    let r = segunda.chamar(&mut m.app, "digitar", json!({ "aba": "1-2", "texto": "a" }));
    assert!(!r.e_erro(), "{}", r.texto());
    dar_pelo_menu(&mut m, "1-2", 1);
    let r = primeira.chamar(&mut m.app, "digitar", json!({ "aba": "1-2", "texto": "b" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!m.app.visible("mcp-aprovar"));
    // A segunda continua com o Livre, mas agora muda uma aba de outra sessão: pergunta.
    let id = segunda.pedir("digitar", json!({ "aba": "1-2", "texto": "c" }));
    esperar(&mut m.app, |a| a.visible("mcp-aprovar"), "devia perguntar");
    m.app.click("mcp-aprovar-primary");
    let _ = segunda.resposta(&mut m.app, id);
}

#[test]
fn o_submenu_so_aparece_com_sessao_e_sem_a_aba_ainda() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dar-menu");
    // Sem sessão ligada, o submenu some, e o de "Controlar com" fica.
    m.app.click("paineis-painel-0-1-2");
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert!(aparece(&m.app, "mcp-controlar"));
    assert!(!aparece(&m.app, "mcp-controlar-dar"));
    m.app.key(tomni_ui::Key::Escape);
    m.app.key(tomni_ui::Key::Escape);

    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let _ = ia.chamar(&mut m.app, "ler_tela", json!({}));
    // Na aba alvo da única sessão não há a quem dar; na outra há, e só a ela.
    m.app.click("paineis-painel-0-0-1");
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert!(aparece(&m.app, "mcp-controlar"));
    assert!(!aparece(&m.app, "mcp-controlar-dar"));
    m.app.key(tomni_ui::Key::Escape);
    m.app.key(tomni_ui::Key::Escape);
    abrir_dar_das_ferramentas(&mut m, "1-2");
    assert!(aparece(&m.app, "mcp-controlar-dar-1"));
    assert!(!aparece(&m.app, "mcp-controlar-dar-2"));
    m.app.click("mcp-controlar-dar-1");

    // Depois de dada, a aba já é dela: o submenu some.
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    assert!(!aparece(&m.app, "mcp-controlar-dar"));
    m.app.key(tomni_ui::Key::Escape);
    m.app.key(tomni_ui::Key::Escape);

    // Com uma 2ª sessão, presa à 1-2, a aba 0-1 pode ser dada a ela e só a ela; e os menus da
    // aba e do terminal (da primeira aba, onde os ids são únicos) mostram o mesmo.
    let mut outra = Assistente::ligar_na_aba(&mut m.app, &endereco, "Codex", "1-2");
    let _ = outra.chamar(&mut m.app, "ler_tela", json!({}));
    m.app.click("paineis-painel-0-0-1");
    m.app.context_menu("paineis-painel-0-0-1");
    assert!(aparece(&m.app, "aba-controlar-dar"));
    m.app.click("aba-controlar-dar");
    assert!(!aparece(&m.app, "aba-controlar-dar-1"));
    assert!(aparece(&m.app, "aba-controlar-dar-2"));
    m.app.key(tomni_ui::Key::Escape);
    m.app.key(tomni_ui::Key::Escape);
    m.app
        .right_click_with("paineis-terminal-0-1", Modifiers::CTRL);
    assert!(aparece(&m.app, "terminal-controlar-dar"));
    m.app.click("terminal-controlar-dar");
    assert!(!aparece(&m.app, "terminal-controlar-dar-1"));
    m.app.click("terminal-controlar-dar-2");

    // Dada a 0-1 ao Codex, ele passa a mexer nela sem perguntar.
    let r = outra.chamar(&mut m.app, "digitar", json!({ "aba": "0-1", "texto": "x" }));
    assert!(!r.e_erro(), "{}", r.texto());
}

#[test]
fn ao_sair_a_aba_dada_deixa_de_estar_liberada() {
    let (_pasta, mut m, endereco) = cenario("mcp-aba-dar-fim");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    let _ = ia.chamar(&mut m.app, "ler_tela", json!({}));
    dar_pelo_menu(&mut m, "1-2", 1);
    ia.desligar();
    // Outra sessão, ligada depois, não herda nada: as duas abas voltam a ser recusadas.
    let mut outra = Assistente::ligar(&mut m.app, &endereco, "Outra");
    for aba in ["0-1", "1-2"] {
        let r = outra.chamar(&mut m.app, "ler_tela", json!({ "aba": aba }));
        assert!(r.e_erro(), "{aba}");
        assert!(r.texto().contains("não está liberada"), "{}", r.texto());
    }
}

#[test]
fn o_arquivo_solto_na_aba_dada_chega_mesmo_sem_pedido_a_ela() {
    let (pasta, mut m, endereco) = cenario("mcp-aba-dar-soltar");
    let mut ia = Assistente::ligar_na_aba(&mut m.app, &endereco, "Claude Code", "0-1");
    // A primeira chamada faz o app conhecer a sessão, para ela aparecer no menu.
    assert!(!ia.chamar(&mut m.app, "ler_tela", json!({})).e_erro());
    dar_pelo_menu(&mut m, "1-2", 1);
    let arquivo = pasta.0.join("instrucoes.md");
    std::fs::write(&arquivo, "# leia").unwrap();
    let _ = recebido(&m, "1-2");
    m.app
        .drop_files("paineis-terminal-1-2", std::slice::from_ref(&arquivo));
    esperar(
        &mut m.app,
        |a| aparece(a, "colar-entregue"),
        "o aviso da entrega",
    );
    assert!(
        !recebido(&m, "1-2").contains("instrucoes.md"),
        "o caminho foi ao shell"
    );

    // O pedido seguinte, mesmo na aba padrão, leva o aviso.
    let r = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!r.e_erro(), "{}", r.texto());
    let caminho = arquivo.display().to_string();
    assert!(r.texto().contains(&caminho), "{}", r.texto());
}
