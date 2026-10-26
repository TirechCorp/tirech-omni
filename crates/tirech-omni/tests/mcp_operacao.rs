//! O app como servidor MCP (M51, fatias 4 e 5): as ferramentas de operação com o app de verdade e
//! a segurança que aparece na tela (o diálogo de aprovação, a fila, a marca na aba, o "Parar a
//! IA", a redação, o corte das saídas grandes e a auditoria), e as várias sessões de assistente
//! ao mesmo tempo (nomes, posse das abas, painéis, parar uma ou todas). O assistente é um
//! cliente do canal, que no Windows é um pipe: o arquivo todo fica só no Unix.

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tirech_omni::mcp::permissao::PermissaoIa;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::Key;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Montagem, Pasta, item_ssh, montar};

/// Uma árvore de conexões locais, `con-1`, `con-2`…, cada uma com a permissão dada.
fn arvore(permissoes: &[PermissaoIa]) -> Tree {
    let mut tree = Tree::default();
    for (i, p) in permissoes.iter().enumerate() {
        let mut c = Connection::new(format!("con-{}", i + 1), Kind::Local);
        c.mcp = *p;
        tree.add_connection(None, c);
    }
    tree
}

/// A tela com o MCP ligado e as conexões de `permissoes`; o socket fica na pasta do teste.
fn tela(nome: &str, permissoes: &[PermissaoIa]) -> (Pasta, Montagem, PathBuf) {
    tela_com(nome, permissoes, |_| {})
}

/// Como [`tela`], com `preparar` mexendo na pasta antes de a tela abrir.
fn tela_com(
    nome: &str,
    permissoes: &[PermissaoIa],
    preparar: impl FnOnce(&Pasta),
) -> (Pasta, Montagem, PathBuf) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore(permissoes));
    pasta.configurar("ia", "mcp_ligado", json!(true));
    preparar(&pasta);
    let endereco = pasta.0.join("mcp.sock");
    let m = montar(&pasta, Theme::Light);
    (pasta, m, endereco)
}

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Abre a conexão `id` pela árvore e devolve a sessão da primeira aba.
fn abrir_aba(m: &mut Montagem, id: u64) -> Session {
    m.app.double_click(&format!("arvore-{id}"));
    let chave = format!("{id}-1");
    esperar(
        &mut m.app,
        |_| Session::find(&chave).is_some(),
        "a aba não abriu",
    );
    Session::find(&chave).unwrap()
}

/// O que a pessoa digitou na sessão desde a última leitura.
fn digitado(m: &Montagem, sessao: Session) -> String {
    let h = m.opener.handle_de(sessao).expect("a ponta da sessão");
    String::from_utf8_lossy(&h.take_written()).into_owned()
}

fn diz(m: &Montagem, sessao: Session, texto: &str) {
    m.opener
        .handle_de(sessao)
        .expect("a ponta da sessão")
        .feed(texto);
}

#[test]
fn desligado_nao_sobe_o_servidor() {
    let pasta = Pasta::nova("mcp-op-desligado");
    pasta.semear(&arvore(&[PermissaoIa::Livre]));
    let endereco = pasta.0.join("mcp.sock");
    let mut m = montar(&pasta, Theme::Light);
    for _ in 0..15 {
        m.app.wait(Duration::from_millis(20));
    }
    assert!(!endereco.exists(), "o MCP desligado não escuta");
}

#[test]
fn conexao_desligada_nao_aparece() {
    let (_pasta, mut m, endereco) = tela(
        "mcp-op-desligada",
        &[PermissaoIa::Desligada, PermissaoIa::SoLer],
    );
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let lista = ia.chamar(&mut m.app, "listar_conexoes", json!({}));
    let conexoes: Value = serde_json::from_str(&lista.texto()).unwrap();
    let nomes: Vec<&str> = conexoes
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["nome"].as_str().unwrap())
        .collect();
    assert_eq!(nomes, ["con-2"]);
    let abrir = ia.chamar(&mut m.app, "abrir_conexao", json!({ "conexao": 1 }));
    assert!(abrir.e_erro(), "{}", abrir.texto());
    assert!(Session::find("1-1").is_none(), "nada abriu");
}

#[test]
fn so_ler_recusa_digitar() {
    let (_pasta, mut m, endereco) = tela("mcp-op-so-ler-digitar", &[PermissaoIa::SoLer]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let r = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-1", "texto": "ls\n" }),
    );
    assert!(r.e_erro());
    assert!(!m.app.visible("mcp-aprovar"), "não pergunta o que não pode");
    assert_eq!(digitado(&m, sessao), "");
}

#[test]
fn so_ler_le_a_tela() {
    let (_pasta, mut m, endereco) = tela("mcp-op-so-ler-ler", &[PermissaoIa::SoLer]);
    let sessao = abrir_aba(&mut m, 1);
    diz(&m, sessao, "olá, mundo\r\n");
    m.app.wait(Duration::from_millis(100));
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let r = ia.chamar(&mut m.app, "ler_tela", json!({ "aba": "1-1" }));
    assert!(!r.e_erro(), "{}", r.texto());
    let texto = r.texto();
    assert!(texto.contains("<saida_do_servidor"), "{texto}");
    assert!(texto.contains("olá, mundo"), "{texto}");
    assert!(texto.contains("</saida_do_servidor>"), "{texto}");
}

#[test]
fn aprovar_abre_o_dialogo_e_permitir_digita() {
    let (_pasta, mut m, endereco) = tela("mcp-op-aprovar", &[PermissaoIa::Aprovar]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Revisão");
    let id = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "uptime\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert!(m.app.text("mcp-aprovar-detalhe").contains("uptime"));
    assert!(m.app.text("mcp-aprovar-contexto").contains("Revisão"));
    assert_eq!(digitado(&m, sessao), "", "nada digitado antes de permitir");
    m.app.click("mcp-aprovar-secondary");
    let r = ia.resposta(&mut m.app, id);
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(digitado(&m, sessao).contains("uptime"));
}

#[test]
fn recusar_com_motivo_volta_para_a_ia() {
    let (_pasta, mut m, endereco) = tela("mcp-op-recusar", &[PermissaoIa::Aprovar]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let id = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "reboot\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    m.app
        .type_into("mcp-aprovar-motivo", "agora não, é produção");
    m.app.click("mcp-aprovar-primary");
    let r = ia.resposta(&mut m.app, id);
    assert!(r.e_erro());
    assert!(r.texto().contains("agora não, é produção"), "{}", r.texto());
    assert_eq!(digitado(&m, sessao), "");
}

#[test]
fn foco_comeca_no_recusar_e_esc_recusa() {
    let (_pasta, mut m, endereco) = tela("mcp-op-esc", &[PermissaoIa::Aprovar]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let id = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "ls\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert_eq!(m.app.focused().as_deref(), Some("mcp-aprovar-primary"));
    m.app.key(Key::Escape);
    let r = ia.resposta(&mut m.app, id);
    assert!(r.e_erro());
    assert_eq!(digitado(&m, sessao), "");
    assert!(!m.app.visible("mcp-aprovar"));
}

#[test]
fn aprovar_sessao_vale_para_a_proxima_da_mesma_aba_e_nao_para_outra() {
    let (_pasta, mut m, endereco) = tela("mcp-op-sessao", &[PermissaoIa::Aprovar]);
    let primeira = abrir_aba(&mut m, 1);
    // Uma segunda aba da mesma conexão.
    m.app.double_click("arvore-1");
    esperar(
        &mut m.app,
        |_| Session::find("1-2").is_some(),
        "a segunda aba não abriu",
    );
    let segunda = Session::find("1-2").unwrap();
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");

    let id = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "um\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    m.app.click("mcp-aprovar-sessao");
    m.app.click("mcp-aprovar-secondary");
    assert!(!ia.resposta(&mut m.app, id).e_erro());

    // Na mesma aba, não pergunta de novo.
    let r = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-1", "texto": "dois\n" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(digitado(&m, primeira).contains("dois"));

    // Em outra aba, pergunta.
    let id = ia.pedir("digitar", json!({ "aba": "1-2", "texto": "tres\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert_eq!(digitado(&m, segunda), "");
    m.app.click("mcp-aprovar-secondary");
    assert!(!ia.resposta(&mut m.app, id).e_erro());
}

#[test]
fn comando_perigoso_pergunta_mesmo_no_livre_e_nao_oferece_a_sessao() {
    let (_pasta, mut m, endereco) = tela("mcp-op-bloqueado", &[PermissaoIa::Livre]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");

    // O comum passa direto.
    let r = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-1", "texto": "ls\n" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!m.app.visible("mcp-aprovar"));
    assert!(digitado(&m, sessao).contains("ls"));

    // O perigoso pergunta, e a escolha de lembrar não aparece.
    let id = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "rm -rf /\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert!(m.app.visible("mcp-aprovar-perigo"));
    assert!(!m.app.visible("mcp-aprovar-sessao"));
    m.app.click("mcp-aprovar-primary");
    assert!(ia.resposta(&mut m.app, id).e_erro());
    assert_eq!(digitado(&m, sessao), "");
}

#[test]
fn dois_pedidos_aparecem_um_de_cada_vez() {
    let (_pasta, mut m, endereco) = tela("mcp-op-fila", &[PermissaoIa::Aprovar]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let a = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "primeiro\n" }));
    let b = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "segundo\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    let primeiro = m.app.text("mcp-aprovar-detalhe");
    m.app.click("mcp-aprovar-secondary");
    assert!(!ia.resposta(&mut m.app, a).e_erro());
    esperar(
        &mut m.app,
        |app| app.visible("mcp-aprovar") && app.text("mcp-aprovar-detalhe") != primeiro,
        "o segundo não apareceu",
    );
    assert!(m.app.text("mcp-aprovar-detalhe").contains("segundo"));
    m.app.click("mcp-aprovar-secondary");
    assert!(!ia.resposta(&mut m.app, b).e_erro());
    let tudo = digitado(&m, sessao);
    assert!(
        tudo.find("primeiro").unwrap() < tudo.find("segundo").unwrap(),
        "{tudo}"
    );
}

#[test]
fn parar_a_ia_cancela_o_que_esta_esperando() {
    let (_pasta, mut m, endereco) = tela("mcp-op-parar", &[PermissaoIa::Livre]);
    abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let id = ia.pedir(
        "esperar",
        json!({ "aba": "1-1", "texto": "nunca aparece", "prazo_secs": 120 }),
    );
    esperar(
        &mut m.app,
        |app| app.visible("mcp-ativa"),
        "a faixa não apareceu",
    );
    m.app.click("mcp-ativa-acao");
    let r = ia.resposta(&mut m.app, id);
    assert!(r.e_erro());
    assert!(r.texto().contains("pessoa"), "{}", r.texto());
    esperar(
        &mut m.app,
        |app| !app.visible("mcp-ativa"),
        "a faixa ficou na tela",
    );
}

#[test]
fn parar_a_ia_recusa_o_que_esta_na_fila() {
    let (_pasta, mut m, endereco) = tela("mcp-op-parar-fila", &[PermissaoIa::Aprovar]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let a = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "primeiro\n" }));
    let b = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "segundo\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    // O diálogo cobre a janela: quem para a IA por baixo dele é o menu da aba ou a bandeja.
    // Aqui o Esc recusa o primeiro e a pessoa para a IA assim que o segundo aparece.
    m.app.key(Key::Escape);
    assert!(ia.resposta(&mut m.app, a).e_erro());
    esperar(
        &mut m.app,
        |app| app.visible("mcp-aprovar"),
        "o segundo não abriu",
    );
    assert!(m.app.text("mcp-aprovar-detalhe").contains("segundo"));
    m.app.click("mcp-aprovar-primary");
    assert!(ia.resposta(&mut m.app, b).e_erro());
    assert_eq!(digitado(&m, sessao), "");
}

#[test]
fn pedido_cancelado_pela_ia_some_do_dialogo() {
    let (_pasta, mut m, endereco) = tela("mcp-op-cancelado", &[PermissaoIa::Aprovar]);
    abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let id = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "x\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    ia.cancelar(id);
    esperar(
        &mut m.app,
        |a| !a.visible("mcp-aprovar"),
        "o diálogo ficou aberto",
    );
}

#[test]
fn marca_na_aba_some_depois_do_tempo_e_a_faixa_conta_as_abas() {
    let (_pasta, mut m, endereco) = tela_com("mcp-op-marca", &[PermissaoIa::Livre], |p| {
        p.configurar("ia", "marca_secs", json!(1));
    });
    abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    assert!(!m.app.visible("mcp-ativa"));
    let r = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-1", "texto": "ls\n" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    esperar(
        &mut m.app,
        |a| a.visible("mcp-ativa"),
        "a faixa não apareceu",
    );
    assert!(m.app.text("mcp-ativa").contains("uma aba"));
    let limite = Instant::now() + Duration::from_secs(5);
    while m.app.visible("mcp-ativa") {
        assert!(Instant::now() < limite, "a marca não saiu");
        m.app.wait(Duration::from_millis(50));
    }
}

#[test]
fn abrir_conexao_abre_a_aba_e_listar_abas_mostra() {
    let (_pasta, mut m, endereco) = tela("mcp-op-abrir", &[PermissaoIa::Livre]);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let r = ia.chamar(&mut m.app, "abrir_conexao", json!({ "conexao": 1 }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(Session::find("1-1").is_some(), "a aba abriu");
    let abas = ia.chamar(&mut m.app, "listar_abas", json!({}));
    assert!(!abas.e_erro(), "{}", abas.texto());
    assert!(abas.texto().contains("1-1"), "{}", abas.texto());
    let fechar = ia.chamar(&mut m.app, "fechar_aba", json!({ "aba": "1-1" }));
    assert!(!fechar.e_erro(), "{}", fechar.texto());
    esperar(
        &mut m.app,
        |_| Session::find("1-1").is_none_or(|s| s.closed()),
        "a aba não fechou",
    );
}

#[test]
fn executar_na_aba_devolve_o_que_apareceu() {
    let (_pasta, mut m, endereco) = tela("mcp-op-executar", &[PermissaoIa::Livre]);
    let sessao = abrir_aba(&mut m, 1);
    diz(&m, sessao, "usuario@host:~$ ");
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let id = ia.pedir(
        "executar",
        json!({ "aba": "1-1", "comando": "echo oi", "prazo_secs": 10 }),
    );
    let ponta = m.opener.handle_de(sessao).expect("a ponta da sessão");
    let mut escrito = String::new();
    esperar(
        &mut m.app,
        |_| {
            escrito.push_str(&String::from_utf8_lossy(&ponta.take_written()));
            escrito.contains("echo oi")
        },
        "o comando não foi digitado",
    );
    diz(&m, sessao, "echo oi\r\noi\r\nusuario@host:~$ ");
    let r = ia.resposta(&mut m.app, id);
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("oi"), "{}", r.texto());
}

#[test]
fn redacao_tira_a_senha_do_cofre_da_tela() {
    let (_pasta, mut m, endereco) = tela_com("mcp-op-redacao", &[PermissaoIa::SoLer], |p| {
        p.semear_cofre(vec![item_ssh("servidor", "root", "s3gr3d0-xyz-123")]);
    });
    let sessao = abrir_aba(&mut m, 1);
    diz(&m, sessao, "a senha é s3gr3d0-xyz-123 mesmo\r\n");
    m.app.wait(Duration::from_millis(100));
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let r = ia.chamar(&mut m.app, "ler_tela", json!({ "aba": "1-1" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!r.texto().contains("s3gr3d0-xyz-123"), "{}", r.texto());
    assert!(r.texto().contains("a senha é"), "{}", r.texto());
}

#[test]
fn saida_grande_corta_e_ler_saida_devolve_o_resto() {
    let (_pasta, mut m, endereco) = tela_com("mcp-op-corte", &[PermissaoIa::SoLer], |p| {
        p.configurar("ia", "caracteres_por_resposta", json!(200));
    });
    let sessao = abrir_aba(&mut m, 1);
    let mut grande = String::new();
    for i in 0..40 {
        grande.push_str(&format!("linha numero {i:03}\r\n"));
    }
    diz(&m, sessao, &grande);
    m.app.wait(Duration::from_millis(100));
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let r = ia.chamar(&mut m.app, "ler_tela", json!({ "aba": "1-1" }));
    assert!(!r.e_erro(), "{}", r.texto());
    let texto = r.texto();
    assert!(texto.contains("ler_saida"), "{texto}");
    assert!(!texto.contains("linha numero 020"), "o meio sai: {texto}");
    let resto = ia.chamar(
        &mut m.app,
        "ler_saida",
        json!({ "saida": "s1", "inicio": 0, "tamanho": 100_000 }),
    );
    assert!(!resto.e_erro(), "{}", resto.texto());
    assert!(
        resto.texto().contains("linha numero 020"),
        "{}",
        resto.texto()
    );
}

#[test]
fn auditoria_grava_uma_linha_por_pedido() {
    let (pasta, mut m, endereco) = tela("mcp-op-auditoria", &[PermissaoIa::Aprovar]);
    abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let r = ia.chamar(&mut m.app, "listar_abas", json!({}));
    assert!(!r.e_erro(), "{}", r.texto());
    let id = ia.pedir("digitar", json!({ "aba": "1-1", "texto": "ls\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    m.app.type_into("mcp-aprovar-motivo", "não");
    m.app.click("mcp-aprovar-primary");
    assert!(ia.resposta(&mut m.app, id).e_erro());

    let pasta_do_registro = pasta.0.join("mcp");
    let mut linhas = Vec::new();
    esperar(
        &mut m.app,
        |_| {
            linhas = std::fs::read_dir(&pasta_do_registro)
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .flat_map(|t| t.lines().map(str::to_owned).collect::<Vec<_>>())
                .collect();
            linhas.len() >= 2
        },
        "a auditoria não gravou",
    );
    let registros: Vec<Value> = linhas
        .iter()
        .map(|l| serde_json::from_str(l).expect("cada linha é um JSON"))
        .collect();
    let digitar = registros
        .iter()
        .find(|r| r["ferramenta"] == "digitar")
        .expect("o registro do digitar");
    assert_eq!(digitar["resultado"], "erro");
    assert_eq!(digitar["motivo"], "não");
    assert!(registros.iter().any(|r| r["ferramenta"] == "listar_abas"));
}

// ---------------------------------------------------------------------------------------------
// Várias sessões de assistente ao mesmo tempo (M51, seção 6.8)
// ---------------------------------------------------------------------------------------------

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

/// A sessão abre a conexão `id` e devolve a chave da aba e a sessão do terminal, com o prompt na
/// tela. A chave é `{id}-{n}`, com `n` contado entre todas as abas abertas.
fn abrir_pela_ia(
    m: &mut Montagem,
    ia: &mut Assistente,
    id: u64,
    extra: Value,
) -> (String, Session) {
    let mut args = json!({ "conexao": id });
    if let (Some(args), Some(extra)) = (args.as_object_mut(), extra.as_object()) {
        args.extend(extra.clone());
    }
    let pedido = ia.pedir("abrir_conexao", args);
    // A resposta só vem depois de a tela receber algo e ficar quieta: a aba falsa não diz nada
    // sozinha, então o teste faz o servidor responder assim que a aba aparece.
    let mut achada = None;
    esperar(
        &mut m.app,
        |_| {
            achada = (1..=16)
                .map(|n| format!("{id}-{n}"))
                .find_map(|chave| Session::find(&chave).map(|s| (chave, s)));
            achada.is_some()
        },
        "a aba não abriu",
    );
    let (chave, sessao) = achada.unwrap();
    diz(m, sessao, "$ ");
    let r = ia.resposta(&mut m.app, pedido);
    assert!(!r.e_erro(), "{}", r.texto());
    assert_eq!(primeiro_json(&r.texto())["aba"], chave.as_str());
    (chave, sessao)
}

/// As abas que `listar_abas` mostra para a sessão.
fn abas_da(m: &mut Montagem, ia: &mut Assistente) -> Vec<Value> {
    let r = ia.chamar(&mut m.app, "listar_abas", json!({}));
    assert!(!r.e_erro(), "{}", r.texto());
    primeiro_json(&r.texto()).as_array().unwrap().clone()
}

/// A aba `aba` na lista de `listar_abas` da sessão.
fn aba_de(m: &mut Montagem, ia: &mut Assistente, aba: &str) -> Value {
    abas_da(m, ia)
        .into_iter()
        .find(|a| a["aba"] == aba)
        .unwrap_or_else(|| panic!("a aba {aba} não está na lista"))
}

/// O texto aparece em algum lugar da tela agora (o título do diálogo não tem id).
fn na_tela(m: &Montagem, texto: &str) -> bool {
    format!("{:?}", m.app.tree()).contains(texto)
}

/// Organiza a janela como uma grade pela sessão `ia`.
fn organizar(m: &mut Montagem, ia: &mut Assistente, colunas: u64, linhas: u64) {
    let r = ia.chamar(
        &mut m.app,
        "organizar_paineis",
        json!({ "colunas": colunas, "linhas": linhas }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
}

#[test]
fn nomes_ia_1_e_ia_2_e_o_nome_da_ponte() {
    let (_pasta, mut m, endereco) = tela("mcp-op-nomes", &[PermissaoIa::Livre]);
    let primeira = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let segunda = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let chamada = Assistente::ligar_com(&mut m.app, &endereco, Some("Revisão"), None);
    assert_eq!(primeira.nome, "IA 1");
    assert_eq!(segunda.nome, "IA 2");
    assert_eq!(chamada.nome, "Revisão");
}

#[test]
fn nome_repetido_ganha_sufixo() {
    let (_pasta, mut m, endereco) = tela("mcp-op-nome-repetido", &[PermissaoIa::Livre]);
    let a = Assistente::ligar_com(&mut m.app, &endereco, Some("Claude"), None);
    let b = Assistente::ligar_com(&mut m.app, &endereco, Some("Claude"), None);
    let c = Assistente::ligar_com(&mut m.app, &endereco, Some("Claude"), None);
    // Sem `--nome`, o nome do espaço de trabalho.
    let d = Assistente::ligar_com(&mut m.app, &endereco, None, Some("projeto"));
    let e = Assistente::ligar_com(&mut m.app, &endereco, None, Some("projeto"));
    assert_eq!(
        [a.nome, b.nome, c.nome, d.nome, e.nome],
        [
            "Claude",
            "Claude (2)",
            "Claude (3)",
            "projeto",
            "projeto (2)"
        ]
    );
}

#[test]
fn nome_volta_a_ficar_livre_quando_a_ponte_desliga() {
    let (_pasta, mut m, endereco) = tela("mcp-op-nome-livre", &[PermissaoIa::Livre]);
    let primeira = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let segunda = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    assert_eq!(
        (primeira.nome.as_str(), segunda.nome.as_str()),
        ("IA 1", "IA 2")
    );
    primeira.desligar();
    // O app percebe a ligação que caiu na rodada seguinte da sincronização.
    let mut nova = None;
    esperar(
        &mut m.app,
        |app| {
            let ia = Assistente::ligar_com(app, &endereco, None, None);
            if ia.nome == "IA 1" {
                nova = Some(ia);
                true
            } else {
                ia.desligar();
                false
            }
        },
        "o nome IA 1 não voltou a ficar livre",
    );
    assert!(nova.is_some());
}

#[test]
fn abrir_conexao_no_painel_3_da_grade_2x2() {
    let (_pasta, mut m, endereco) = tela("mcp-op-painel-3", &[PermissaoIa::Livre]);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    organizar(&mut m, &mut ia, 2, 2);
    assert_eq!(m.app.panes("paineis").panes.len(), 4);
    abrir_pela_ia(&mut m, &mut ia, 1, json!({ "painel": 3 }));
    let paineis = m.app.panes("paineis").panes;
    assert_eq!(paineis[2].tabs.len(), 1, "a aba está no painel 3");
    assert!(paineis[0].tabs.is_empty() && paineis[1].tabs.is_empty() && paineis[3].tabs.is_empty());
}

#[test]
fn painel_que_nao_existe_da_erro() {
    let (_pasta, mut m, endereco) = tela("mcp-op-painel-fora", &[PermissaoIa::Livre]);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let r = ia.chamar(
        &mut m.app,
        "abrir_conexao",
        json!({ "conexao": 1, "painel": 3 }),
    );
    assert!(r.e_erro(), "{}", r.texto());
    assert!(
        r.texto().contains("A janela 1 tem 1 painel"),
        "{}",
        r.texto()
    );
    assert!(r.texto().contains("listar_paineis"), "{}", r.texto());
    assert!(Session::find("1-1").is_none(), "nada abriu");
}

#[test]
fn organizar_paineis_monta_a_grade_e_pergunta_em_aprovar() {
    let (_pasta, mut m, endereco) = tela("mcp-op-organizar", &[PermissaoIa::Aprovar]);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Teste");
    let id = ia.pedir("organizar_paineis", json!({ "colunas": 2, "linhas": 2 }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert!(na_tela(&m, "IA 1") || na_tela(&m, "Teste"));
    assert!(na_tela(&m, "quer organizar a janela 1 em 2 × 2"));
    assert_eq!(m.app.panes("paineis").panes.len(), 1, "nada mudou antes");
    m.app.click("mcp-aprovar-secondary");
    let r = ia.resposta(&mut m.app, id);
    assert!(!r.e_erro(), "{}", r.texto());
    assert_eq!(m.app.panes("paineis").panes.len(), 4);
}

#[test]
fn listar_paineis_mostra_donos() {
    let (_pasta, mut m, endereco) = tela(
        "mcp-op-listar-paineis",
        &[PermissaoIa::Livre, PermissaoIa::Livre],
    );
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    organizar(&mut m, &mut ia1, 2, 1);
    let (aba1, _) = abrir_pela_ia(&mut m, &mut ia1, 1, json!({ "painel": 1 }));
    let (aba2, _) = abrir_pela_ia(&mut m, &mut ia2, 2, json!({ "painel": 2 }));
    let r = ia1.chamar(&mut m.app, "listar_paineis", json!({}));
    assert!(!r.e_erro(), "{}", r.texto());
    let lista = primeiro_json(&r.texto());
    assert_eq!(lista["janela"], 1);
    assert_eq!(
        (lista["colunas"].as_u64(), lista["linhas"].as_u64()),
        (Some(2), Some(1))
    );
    let paineis = lista["paineis"].as_array().unwrap();
    assert_eq!(paineis.len(), 2);
    assert_eq!(paineis[0]["abas"], json!([aba1]));
    assert_eq!(paineis[0]["donos"], json!(["IA 1"]));
    assert_eq!(paineis[1]["abas"], json!([aba2]));
    assert_eq!(paineis[1]["donos"], json!(["IA 2"]));
}

#[test]
fn aba_aberta_pela_ia_e_dela() {
    let (_pasta, mut m, endereco) = tela("mcp-op-dona", &[PermissaoIa::Livre]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    abrir_pela_ia(&mut m, &mut ia1, 1, json!({}));
    let minha = aba_de(&mut m, &mut ia1, "1-1");
    assert_eq!(minha["dono"], "IA 1");
    assert_eq!(minha["minha"], true);
    let vista_pela_outra = aba_de(&mut m, &mut ia2, "1-1");
    assert_eq!(vista_pela_outra["dono"], "IA 1");
    assert_eq!(vista_pela_outra["minha"], false);
    assert_eq!(vista_pela_outra["painel"], 1);
}

#[test]
fn aba_da_pessoa_vira_da_primeira_que_muda() {
    let (_pasta, mut m, endereco) = tela("mcp-op-pessoa", &[PermissaoIa::Livre]);
    let sessao = abrir_aba(&mut m, 1);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    assert!(aba_de(&mut m, &mut ia1, "1-1")["dono"].is_null());
    // Só olhar não toma a aba.
    let r = ia1.chamar(&mut m.app, "ler_tela", json!({ "aba": "1-1" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(aba_de(&mut m, &mut ia2, "1-1")["dono"].is_null());
    // A primeira que muda algo fica com ela.
    let r = ia1.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-1", "texto": "ls\n" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(digitado(&m, sessao).contains("ls"));
    assert_eq!(aba_de(&mut m, &mut ia2, "1-1")["dono"], "IA 1");
}

#[test]
fn outra_sessao_le_sem_perguntar() {
    let (_pasta, mut m, endereco) = tela("mcp-op-le-de-outra", &[PermissaoIa::Aprovar]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let sessao = abrir_aba(&mut m, 1);
    // A pessoa libera o digitar da IA 1 uma vez; a aba passa a ser dela.
    let id = ia1.pedir("digitar", json!({ "aba": "1-1", "texto": "ls\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    m.app.click("mcp-aprovar-secondary");
    assert!(!ia1.resposta(&mut m.app, id).e_erro());
    diz(&m, sessao, "olá da aba\r\n");
    m.app.wait(Duration::from_millis(100));
    let r = ia2.chamar(&mut m.app, "ler_tela", json!({ "aba": "1-1" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("olá da aba"), "{}", r.texto());
    assert!(!m.app.visible("mcp-aprovar"), "ler não pergunta");
}

#[test]
fn outra_sessao_digitar_pergunta_mesmo_em_livre() {
    let (_pasta, mut m, endereco) = tela("mcp-op-posse-livre", &[PermissaoIa::Livre]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let (_, sessao) = abrir_pela_ia(&mut m, &mut ia1, 1, json!({}));
    let id = ia2.pedir("digitar", json!({ "aba": "1-1", "texto": "uptime\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo da posse não abriu",
    );
    assert!(na_tela(&m, "IA 2 quer digitar numa aba de IA 1"));
    assert!(
        m.app.visible("mcp-aprovar-close"),
        "o botão de passar a aba"
    );
    assert_eq!(digitado(&m, sessao), "", "nada digitado antes de permitir");
    m.app.click("mcp-aprovar-secondary");
    let r = ia2.resposta(&mut m.app, id);
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(digitado(&m, sessao).contains("uptime"));
    // "Permitir uma vez" não muda o dono.
    assert_eq!(aba_de(&mut m, &mut ia2, "1-1")["dono"], "IA 1");
}

#[test]
fn passar_a_aba_troca_o_dono() {
    let (_pasta, mut m, endereco) = tela("mcp-op-passar", &[PermissaoIa::Livre]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let (_, sessao) = abrir_pela_ia(&mut m, &mut ia1, 1, json!({}));
    let id = ia2.pedir("digitar", json!({ "aba": "1-1", "texto": "pwd\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert!(na_tela(&m, "Passar a aba para IA 2"));
    m.app.click("mcp-aprovar-close");
    let r = ia2.resposta(&mut m.app, id);
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(digitado(&m, sessao).contains("pwd"));
    assert_eq!(aba_de(&mut m, &mut ia1, "1-1")["dono"], "IA 2");
    assert_eq!(aba_de(&mut m, &mut ia2, "1-1")["minha"], true);
    // Agora é a IA 1 que precisa de licença.
    let id = ia1.pedir("digitar", json!({ "aba": "1-1", "texto": "ls\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert!(na_tela(&m, "IA 1 quer digitar numa aba de IA 2"));
    m.app.click("mcp-aprovar-primary");
    assert!(ia1.resposta(&mut m.app, id).e_erro());
}

#[test]
fn recusa_explica_que_a_aba_e_de_outra_sessao() {
    let (_pasta, mut m, endereco) = tela("mcp-op-recusa-posse", &[PermissaoIa::Livre]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let (_, sessao) = abrir_pela_ia(&mut m, &mut ia1, 1, json!({}));
    let id = ia2.pedir("digitar", json!({ "aba": "1-1", "texto": "reboot\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    assert_eq!(m.app.focused().as_deref(), Some("mcp-aprovar-primary"));
    m.app.click("mcp-aprovar-primary");
    let r = ia2.resposta(&mut m.app, id);
    assert!(r.e_erro());
    assert!(
        r.texto()
            .contains("Esta aba é de outra sessão de assistente (IA 1)"),
        "{}",
        r.texto()
    );
    assert!(r.texto().contains("abrir_conexao"), "{}", r.texto());
    assert_eq!(digitado(&m, sessao), "");
    assert_eq!(aba_de(&mut m, &mut ia2, "1-1")["dono"], "IA 1");
}

/// Duas sessões esperando algo em abas suas: a faixa conta as duas.
fn duas_esperando(nome: &str) -> (Pasta, Montagem, Assistente, Assistente, u64, u64) {
    let (pasta, mut m, endereco) = tela(nome, &[PermissaoIa::Livre, PermissaoIa::Livre]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let (aba1, _) = abrir_pela_ia(&mut m, &mut ia1, 1, json!({}));
    let (aba2, _) = abrir_pela_ia(&mut m, &mut ia2, 2, json!({}));
    let a = ia1.pedir(
        "esperar",
        json!({ "aba": aba1, "texto": "nunca aparece", "prazo_secs": 120 }),
    );
    let b = ia2.pedir(
        "esperar",
        json!({ "aba": aba2, "texto": "nunca aparece", "prazo_secs": 120 }),
    );
    esperar(
        &mut m.app,
        |app| app.visible("mcp-ativas"),
        "a faixa das várias sessões não apareceu",
    );
    (pasta, m, ia1, ia2, a, b)
}

#[test]
fn parar_uma_sessao_nao_para_as_outras() {
    let (_pasta, mut m, mut ia1, mut ia2, a, b) = duas_esperando("mcp-op-parar-uma");
    assert!(
        m.app.text("mcp-ativas").contains("2 assistentes"),
        "{}",
        m.app.text("mcp-ativas")
    );
    m.app.click("mcp-parar-varias");
    m.app.click("mcp-parar-sessao-1");
    let r = ia1.resposta(&mut m.app, a);
    assert!(r.e_erro());
    assert!(r.texto().contains("pessoa"), "{}", r.texto());
    // A outra continua esperando, e a ponte da primeira segue ligada.
    m.app.wait(Duration::from_millis(200));
    assert!(
        ia2.guardadas.iter().all(|v| v["id"] != b),
        "a IA 2 não foi parada"
    );
    let r = ia1.chamar(&mut m.app, "listar_abas", json!({}));
    assert!(!r.e_erro(), "{}", r.texto());
    ia2.cancelar(b);
}

#[test]
fn parar_todas() {
    let (_pasta, mut m, mut ia1, mut ia2, a, b) = duas_esperando("mcp-op-parar-todas");
    m.app.click("mcp-parar-varias");
    m.app.click("mcp-parar-todas");
    assert!(ia1.resposta(&mut m.app, a).e_erro());
    assert!(ia2.resposta(&mut m.app, b).e_erro());
    esperar(
        &mut m.app,
        |app| !app.visible("mcp-ativas") && !app.visible("mcp-ativa"),
        "a faixa ficou na tela",
    );
}

#[test]
fn ponte_que_cai_solta_as_abas() {
    let (_pasta, mut m, endereco) = tela("mcp-op-ponte-cai", &[PermissaoIa::Livre]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let mut ia2 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    let (_, sessao) = abrir_pela_ia(&mut m, &mut ia1, 1, json!({}));
    assert_eq!(aba_de(&mut m, &mut ia2, "1-1")["dono"], "IA 1");
    ia1.desligar();
    esperar(
        &mut m.app,
        |app| {
            let r = ia2.chamar(app, "listar_abas", json!({}));
            let abas = primeiro_json(&r.texto());
            abas.as_array().is_some_and(|abas| {
                abas.iter()
                    .any(|a| a["aba"] == "1-1" && a["dono"].is_null())
            })
        },
        "a aba não ficou sem dono",
    );
    // Sem dono, a IA 2 digita sem diálogo de posse.
    let r = ia2.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-1", "texto": "ls\n" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!m.app.visible("mcp-aprovar"));
    assert!(digitado(&m, sessao).contains("ls"));
}

#[test]
fn tirar_da_ia_pelo_menu_da_aba() {
    let (_pasta, mut m, endereco) = tela("mcp-op-tirar-da-ia", &[PermissaoIa::Livre]);
    let mut ia1 = Assistente::ligar_com(&mut m.app, &endereco, None, None);
    abrir_pela_ia(&mut m, &mut ia1, 1, json!({}));
    assert_eq!(aba_de(&mut m, &mut ia1, "1-1")["dono"], "IA 1");
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    assert!(m.app.visible("mcp-tirar-da-ia"));
    assert_eq!(m.app.text("mcp-tirar-da-ia"), "Tirar da IA 1");
    m.app.click("mcp-tirar-da-ia");
    m.app.frame();
    assert!(aba_de(&mut m, &mut ia1, "1-1")["dono"].is_null());
    // Sem dono, o menu não oferece mais.
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    assert!(!m.app.visible("mcp-tirar-da-ia"));
}

#[test]
fn quatro_sessoes_em_quatro_quadrantes() {
    let (_pasta, mut m, endereco) = tela("mcp-op-quadrantes", &[PermissaoIa::Livre; 4]);
    let mut ias: Vec<Assistente> = (0..4)
        .map(|_| Assistente::ligar_com(&mut m.app, &endereco, None, None))
        .collect();
    organizar(&mut m, &mut ias[0], 2, 2);
    let mut abas = Vec::new();
    let mut sessoes = Vec::new();
    for (i, ia) in ias.iter_mut().enumerate() {
        let id = i as u64 + 1;
        let (aba, sessao) = abrir_pela_ia(&mut m, ia, id, json!({ "painel": id }));
        abas.push(aba);
        sessoes.push(sessao);
    }
    for (i, painel) in m.app.panes("paineis").panes.iter().enumerate() {
        assert_eq!(
            painel.tabs.len(),
            1,
            "o painel {} tem a aba da sessão",
            i + 1
        );
    }
    // Cada uma digita na sua aba, a tela responde, e cada uma lê só a sua.
    for (i, ia) in ias.iter_mut().enumerate() {
        let r = ia.chamar(
            &mut m.app,
            "digitar",
            json!({ "aba": abas[i], "texto": format!("eco-{}\n", i + 1) }),
        );
        assert!(!r.e_erro(), "{}", r.texto());
        assert!(digitado(&m, sessoes[i]).contains(&format!("eco-{}", i + 1)));
        diz(
            &m,
            sessoes[i],
            &format!("eco-{}\r\nresposta-da-{}\r\n", i + 1, i + 1),
        );
    }
    m.app.wait(Duration::from_millis(150));
    for (i, ia) in ias.iter_mut().enumerate() {
        let r = ia.chamar(&mut m.app, "ler_tela", json!({ "aba": abas[i] }));
        assert!(!r.e_erro(), "{}", r.texto());
        assert!(
            r.texto().contains(&format!("resposta-da-{}", i + 1)),
            "{}",
            r.texto()
        );
        for outra in (1..=4).filter(|&n| n != i + 1) {
            assert!(
                !r.texto().contains(&format!("resposta-da-{outra}")),
                "{}",
                r.texto()
            );
        }
    }
    // Cada aba é de quem a abriu.
    for (i, ia) in ias.iter_mut().enumerate() {
        let aba = aba_de(&mut m, ia, &abas[i]);
        assert_eq!(aba["dono"], format!("IA {}", i + 1));
        assert_eq!(aba["painel"], i + 1);
    }
}
