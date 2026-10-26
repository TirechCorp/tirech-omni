//! Soltar arquivos numa aba que um assistente usa: o app envia ao servidor como sempre, mas não
//! escreve o caminho no shell (atrapalharia o próximo `executar` do assistente). O aviso vai na
//! próxima resposta do MCP sobre a aba e no `listar_abas`. O servidor é um `MemFs` e o disco
//! também; o relógio do app é fixo.

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use jiff::civil::DateTime;
use serde_json::{Value, json};
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Montagem, Pasta, montar_com_ambiente};

/// O carimbo do relógio fixo, o que os nomes remotos levam na frente.
const CARIMBO: &str = "20261019-143012";

/// A pasta padrão do que é colado.
const PASTA: &str = "/tmp/tirech-colar";

/// O `--lancamento` da ponte do cenário, o que o hook dos avisos usa.
const LANCAMENTO: &str = "4242-1";

/// 19/10/2026, 14:30:12.
fn agora_fixo() -> DateTime {
    jiff::civil::date(2026, 10, 19).at(14, 30, 12, 0)
}

/// O app com o MCP ligado e uma conexão SSH (id 1, `web`) aberta, com o assistente preso à aba
/// dela. Devolve também a chave da aba.
fn cenario(nome: &str) -> (Pasta, Montagem, Assistente, String) {
    let pasta = Pasta::nova(nome);
    let mut tree = Tree::default();
    let mut c = Connection::new("web", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    let endereco = pasta.0.join("mcp.sock");
    let (mut m, _f) = montar_com_ambiente(&pasta, Theme::Light, |a| a.agora = agora_fixo);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a aba abrir", |_| {
        Session::find("1-1").is_some()
    });
    let chave = "1-1".to_owned();
    let sessao = Session::find(&chave).unwrap();
    m.opener.handle_de(sessao).unwrap().feed("$ ");
    let mut ia =
        Assistente::ligar_lancado(&mut m.app, &endereco, "Claude Code", &chave, LANCAMENTO);
    // A primeira chamada faz o app conhecer a sessão (o alarme que sincroniza não anda com o
    // relógio fixo).
    let tela = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!tela.e_erro(), "{}", tela.texto());
    (pasta, m, ia, chave)
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

/// O que o app escreveu na conexão da aba desde a última leitura.
fn escrito(m: &Montagem, chave: &str) -> String {
    let sessao = Session::find(chave).expect("a sessão da aba");
    let handle = m.opener.handle_de(sessao).expect("a ponta da sessão");
    String::from_utf8_lossy(&handle.take_written()).into_owned()
}

fn arquivo_local(m: &Montagem, caminho: &str, dados: &[u8]) -> PathBuf {
    m.opener.disco.put(caminho, dados);
    PathBuf::from(caminho)
}

/// Todo o texto de uma resposta (a nota vem somada ao primeiro).
fn todo_o_texto(resposta: &assistente::Resposta) -> String {
    resposta.resultado()["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Solta `um.txt` na aba e espera o envio terminar (o aviso aparece).
fn soltar(m: &mut Montagem, chave: &str) -> String {
    let um = arquivo_local(m, "/home/ana/um.txt", b"um");
    escrito(m, chave);
    m.app
        .drop_files(&format!("paineis-terminal-{chave}"), &[um]);
    esperar(&mut m.app, "o aviso da entrega", |a| {
        aparece(a, "colar-entregue")
    });
    format!("{PASTA}/{CARIMBO}-um.txt")
}

#[test]
fn solto_na_aba_do_assistente_vai_ao_servidor_e_nao_ao_shell() {
    let (_pasta, mut m, mut ia, chave) = cenario("mcp-soltar-envia");
    let remoto = soltar(&mut m, &chave);
    assert_eq!(m.opener.servidor(1).contents(&remoto), Some(b"um".to_vec()));
    let digitado = escrito(&m, &chave);
    assert!(
        !digitado.contains(PASTA),
        "o caminho foi ao shell: {digitado:?}"
    );

    // A próxima resposta sobre a aba leva o aviso, uma vez só.
    let tela = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!tela.e_erro(), "{}", tela.texto());
    let texto = todo_o_texto(&tela);
    assert!(texto.contains(&remoto), "{texto}");
    assert!(texto.contains("já foram enviados"), "{texto}");
    let de_novo = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!todo_o_texto(&de_novo).contains(&remoto));
    m.app.assert_lint();
}

#[test]
fn o_hook_entrega_os_soltos_junto_da_mensagem_da_pessoa() {
    let (pasta, mut m, mut ia, chave) = cenario("mcp-soltar-hook");
    let endereco = pasta.0.join("mcp.sock");
    assert_eq!(assistente::avisos(&mut m.app, &endereco, LANCAMENTO), None);
    let remoto = soltar(&mut m, &chave);
    let texto = assistente::avisos(&mut m.app, &endereco, LANCAMENTO).expect("os avisos");
    assert!(texto.contains(&remoto), "{texto}");
    assert!(texto.contains("já foram enviados"), "{texto}");
    // Saiu da fila: nem o hook nem a próxima ferramenta repetem.
    assert_eq!(assistente::avisos(&mut m.app, &endereco, LANCAMENTO), None);
    let tela = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!todo_o_texto(&tela).contains(&remoto));
    // Outro lançamento não leva os avisos desta sessão.
    let _ = soltar(&mut m, &chave);
    assert_eq!(assistente::avisos(&mut m.app, &endereco, "1-1"), None);
    assert!(todo_o_texto(&ia.chamar(&mut m.app, "ler_tela", json!({}))).contains(PASTA));
}

#[test]
fn listar_abas_mostra_os_arquivos_soltos() {
    let (_pasta, mut m, mut ia, chave) = cenario("mcp-soltar-lista");
    let remoto = soltar(&mut m, &chave);
    let abas = ia.chamar(&mut m.app, "listar_abas", json!({}));
    let texto = abas.texto();
    let inicio = texto
        .find('[')
        .unwrap_or_else(|| panic!("sem lista: {texto}"));
    let lista: Value = serde_json::Deserializer::from_str(&texto[inicio..])
        .into_iter::<Value>()
        .next()
        .and_then(Result::ok)
        .unwrap_or_else(|| panic!("JSON inválido: {texto}"));
    let aba = lista
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["aba"] == chave.as_str())
        .unwrap_or_else(|| panic!("a aba não está na lista: {lista}"));
    assert_eq!(
        aba["arquivos_soltos"],
        json!([{ "caminho": remoto, "enviado": true }])
    );
    // O listar_abas já deu o aviso: a próxima resposta não repete.
    let tela = ia.chamar(&mut m.app, "ler_tela", json!({}));
    assert!(!todo_o_texto(&tela).contains(&remoto));
}
