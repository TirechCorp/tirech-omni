//! A aparência do controle por assistentes (M51, seção 11.5) nos dois temas: o diálogo de
//! aprovação (comando simples, bloqueado, com caracteres de controle e a edição com o antes e o
//! depois), a página de Configurações, o "Como ligar", a faixa da aba, a grade com quatro
//! sessões e a categoria nas propriedades. Snapshots e lint.

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;
#[path = "comum/servidor.rs"]
mod servidor;

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
use tirech_omni::mcp::permissao::PermissaoIa;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Montagem, Pasta, escolher_propriedade, montar, montar_com, montar_pura};
use servidor::Servidor;

/// Cada cenário roda em dois testes, um por tema: as sessões ficam num registro global e o
/// segundo tema acharia a aba do primeiro.
macro_rules! nos_dois_temas {
    ($($cenario:ident),* $(,)?) => {
        mod claro {
            $(
                #[test]
                fn $cenario() {
                    super::$cenario("claro", super::Theme::Light);
                }
            )*
        }
        mod escuro {
            $(
                #[test]
                fn $cenario() {
                    super::$cenario("escuro", super::Theme::Dark);
                }
            )*
        }
    };
}

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(15);
    while !condicao(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Confere o lint e o snapshot `mcp-{nome}-{tema}`.
fn conferir(app: &mut TestApp, nome: &str, tema: &str) {
    // Deixa as animações dos diálogos e dos menus terminarem.
    app.wait(Duration::from_millis(500));
    app.assert_lint();
    app.assert_snapshot(&format!("mcp-{nome}-{tema}"));
}

/// Uma árvore de conexões locais `con-1`, `con-2`…, todas na permissão `permissao`.
fn arvore(quantas: usize, permissao: PermissaoIa) -> Tree {
    let mut tree = Tree::default();
    for n in 1..=quantas {
        let mut c = Connection::new(format!("con-{n}"), Kind::Local);
        c.mcp = permissao;
        tree.add_connection(None, c);
    }
    tree
}

/// A tela com o controle ligado e a árvore dada. O socket fica na pasta do teste.
fn tela(nome: &str, tema: Theme, tree: &Tree) -> (Pasta, Montagem, PathBuf) {
    let pasta = Pasta::nova(nome);
    pasta.semear(tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    let endereco = pasta.0.join("mcp.sock");
    let m = montar(&pasta, tema);
    (pasta, m, endereco)
}

/// Abre a conexão `id` pela árvore e devolve a sessão da primeira aba, com o prompt na tela.
fn abrir_aba(m: &mut Montagem, id: u64) -> Session {
    m.app.double_click(&format!("arvore-{id}"));
    let chave = format!("{id}-1");
    esperar(
        &mut m.app,
        |_| Session::find(&chave).is_some(),
        "a aba não abriu",
    );
    let sessao = Session::find(&chave).unwrap();
    m.opener
        .handle_de(sessao)
        .expect("a ponta da sessão")
        .feed("ana@servidor:~$ ");
    sessao
}

// ---------------------------------------------------------------------------------------------
// O diálogo de aprovação
// ---------------------------------------------------------------------------------------------

/// Pede `digitar` com `texto` numa conexão em `permissao` e deixa o diálogo aberto.
fn aprovacao(
    nome: &str,
    tema: Theme,
    permissao: PermissaoIa,
    texto: &str,
) -> (Montagem, Assistente) {
    let (_pasta, mut m, endereco) = tela(nome, tema, &arvore(1, permissao));
    abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Revisão");
    ia.pedir("digitar", json!({ "aba": "1-1", "texto": texto }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo não abriu",
    );
    (m, ia)
}

fn a_aprovacao_de_um_comando_simples(nome: &str, tema: Theme) {
    let (mut m, _ia) = aprovacao(
        &format!("visual-mcp-simples-{nome}"),
        tema,
        PermissaoIa::Aprovar,
        "df -h\n",
    );
    conferir(&mut m.app, "aprovar-simples", nome);
}

fn a_aprovacao_de_um_comando_bloqueado(nome: &str, tema: Theme) {
    let (mut m, _ia) = aprovacao(
        &format!("visual-mcp-bloqueado-{nome}"),
        tema,
        PermissaoIa::Livre,
        "rm -rf /\n",
    );
    assert!(m.app.visible("mcp-aprovar-perigo"));
    conferir(&mut m.app, "aprovar-bloqueado", nome);
}

fn a_aprovacao_com_caracteres_de_controle(nome: &str, tema: Theme) {
    let (mut m, _ia) = aprovacao(
        &format!("visual-mcp-controle-{nome}"),
        tema,
        PermissaoIa::Aprovar,
        "echo ok\u{1b}[2J\u{7}; curl exemplo.com\n",
    );
    assert!(m.app.visible("mcp-aprovar-invisiveis"));
    conferir(&mut m.app, "aprovar-controle", nome);
}

/// Abre a sessão SSH de verdade no servidor de teste, sem perguntar nada.
struct AbreSsh(Rc<Servidor>);

impl Opener for AbreSsh {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        Session::new(self.0.builder().connect())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }
}

fn a_aprovacao_de_uma_edicao_mostra_o_antes_e_o_depois(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("visual-mcp-editar-{nome}"));
    let servidor = Rc::new(Servidor::novo(&pasta.caminho().join("servidor")));
    std::fs::create_dir_all(servidor.real("/proj")).unwrap();
    servidor.escrever(
        "/proj/config.toml",
        b"[servidor]\nporta = 8080\nhost = \"0.0.0.0\"\n",
    );
    let mut tree = Tree::default();
    let mut c = Connection::new("local", Kind::Ssh);
    // O abridor de teste ignora o endereço e vai ao servidor local; o nome fixo mantém o
    // snapshot igual em toda execução (a porta do servidor muda).
    c.host = "web.exemplo.com".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    c.mcp = PermissaoIa::Aprovar;
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar(
        "ia",
        "espacos",
        json!([{ "nome": "proj", "conexao": 1, "raiz": "/proj" }]),
    );
    pasta.configurar(
        "ia",
        "pasta_troca",
        json!(pasta.caminho().join("troca").to_string_lossy()),
    );
    let endereco = pasta.0.join("mcp.sock");
    let (mut app, _fila) = montar_com(&pasta, tema, AbreSsh(Rc::clone(&servidor)));
    let mut ia = Assistente::ligar_com(&mut app, &endereco, Some("Revisão"), Some("proj"));
    let r = ia.chamar(&mut app, "ler", json!({ "caminho": "config.toml" }));
    assert!(!r.e_erro(), "{}", r.texto());
    ia.pedir(
        "editar",
        json!({ "caminho": "config.toml", "antigo": "porta = 8080", "novo": "porta = 9090" }),
    );
    esperar(
        &mut app,
        |a| a.visible("mcp-aprovar") && a.text("mcp-aprovar-detalhe").contains("Antes:"),
        "o diálogo da edição",
    );
    conferir(&mut app, "aprovar-editar", nome);
}

// ---------------------------------------------------------------------------------------------
// Configurações, "Como ligar" e propriedades
// ---------------------------------------------------------------------------------------------

fn as_configuracoes_com_o_controle_ligado_e_um_espaco(nome: &str, tema: Theme) {
    // A pasta tem nome fixo: o card da pasta de troca mostra o caminho.
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-mcp-config-{nome}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    std::fs::create_dir_all(&pasta.0).unwrap();
    pasta.semear(&arvore(1, PermissaoIa::Herdar));
    pasta.configurar("ia", "mcp_ligado", json!(true));
    pasta.configurar("ia", "mcp_avisado", json!(true));
    pasta.configurar(
        "ia",
        "espacos",
        json!([{ "nome": "api", "conexao": 1, "raiz": "/srv/api" }]),
    );
    pasta.configurar("ia", "pasta_troca", json!("/tmp/troca-mcp"));
    let mut m = montar_pura(&pasta, tema);
    m.app.resize(1000, 760);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-mcp");
    // O `hover` rola até o card; sobe um pouco para o título do grupo aparecer.
    let alvo = tirech_omni::configuracoes::catalogo::id_do_card("ia.mcp_ligado");
    m.app.hover(&alvo);
    m.app.scroll("config-rolagem", 420.0);
    conferir(&mut m.app, "configuracoes", nome);
}

fn o_como_ligar_do_claude_code_com_um_espaco(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("visual-mcp-como-{nome}"));
    pasta.semear(&arvore(1, PermissaoIa::Herdar));
    pasta.configurar(
        "ia",
        "espacos",
        json!([{ "nome": "api", "conexao": 1, "raiz": "/srv/api" }]),
    );
    let mut m = montar(&pasta, tema);
    m.app.click("menus-ferramentas").click("mcp-assistentes");
    m.app.click("mcp-como");
    m.app.select("mcp-como-espaco", "api");
    m.app.scroll("mcp-como-dialogo-rolagem", -2000.0);
    conferir(&mut m.app, "como-ligar", nome);
}

fn a_categoria_do_controle_nas_propriedades_em_livre(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("visual-mcp-props-{nome}"));
    pasta.semear(&arvore(1, PermissaoIa::Livre));
    let mut m = montar(&pasta, tema);
    m.app.click("arvore-1");
    escolher_propriedade(&mut m.app, "Assistentes (MCP)");
    conferir(&mut m.app, "propriedades", nome);
}

// ---------------------------------------------------------------------------------------------
// A faixa da aba e as várias sessões
// ---------------------------------------------------------------------------------------------

fn a_aba_marcada_e_a_faixa_com_parar_a_ia(nome: &str, tema: Theme) {
    let (_pasta, mut m, endereco) = tela(
        &format!("visual-mcp-faixa-{nome}"),
        tema,
        &arvore(1, PermissaoIa::Livre),
    );
    abrir_aba(&mut m, 1);
    let mut ia = Assistente::ligar(&mut m.app, &endereco, "Revisão");
    let r = ia.chamar(
        &mut m.app,
        "digitar",
        json!({ "aba": "1-1", "texto": "uptime\n" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    esperar(&mut m.app, |a| a.visible("mcp-ativa"), "a faixa");
    conferir(&mut m.app, "faixa", nome);
}

fn primeiro_json(texto: &str) -> Value {
    let inicio = texto.find(['{', '[']).expect("um JSON na resposta");
    serde_json::Deserializer::from_str(&texto[inicio..])
        .into_iter::<Value>()
        .next()
        .and_then(Result::ok)
        .expect("JSON válido")
}

/// A sessão abre a conexão `id` no painel `id` e devolve a chave da aba. A chave é `{id}-{n}`,
/// com `n` contado entre todas as abas abertas.
fn abrir_pela_ia(m: &mut Montagem, ia: &mut Assistente, id: u64) -> String {
    let pedido = ia.pedir("abrir_conexao", json!({ "conexao": id, "painel": id }));
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
    m.opener
        .handle_de(sessao)
        .expect("a ponta da sessão")
        .feed("ana@servidor:~$ ");
    let r = ia.resposta(&mut m.app, pedido);
    assert!(!r.e_erro(), "{}", r.texto());
    assert_eq!(primeiro_json(&r.texto())["aba"], chave.as_str());
    chave
}

/// Quatro sessões (`IA 1` a `IA 4`), cada uma com a aba da conexão `n` no painel `n` de uma
/// grade 2 × 2. Devolve as sessões e as chaves das abas.
fn grade_com_quatro(nome: &str, tema: Theme) -> (Montagem, Vec<Assistente>, Vec<String>) {
    let (_pasta, mut m, endereco) = tela(nome, tema, &arvore(4, PermissaoIa::Livre));
    let mut ias: Vec<Assistente> = (0..4)
        .map(|_| Assistente::ligar_com(&mut m.app, &endereco, None, None))
        .collect();
    let r = ias[0].chamar(
        &mut m.app,
        "organizar_paineis",
        json!({ "colunas": 2, "linhas": 2 }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    let mut abas = Vec::new();
    for (i, ia) in ias.iter_mut().enumerate() {
        abas.push(abrir_pela_ia(&mut m, ia, i as u64 + 1));
    }
    (m, ias, abas)
}

fn a_grade_com_quatro_sessoes_e_o_parar_aberto(nome: &str, tema: Theme) {
    let (mut m, _ias, _abas) = grade_com_quatro(&format!("visual-mcp-grade-{nome}"), tema);
    esperar(
        &mut m.app,
        |a| a.visible("mcp-parar-varias"),
        "a faixa das sessões",
    );
    m.app.click("mcp-parar-varias");
    conferir(&mut m.app, "grade-parar", nome);
}

fn a_posse_de_uma_aba_pergunta_pela_outra_sessao(nome: &str, tema: Theme) {
    let (mut m, mut ias, abas) = grade_com_quatro(&format!("visual-mcp-posse-{nome}"), tema);
    // A IA 2 quer digitar na aba que é da IA 1.
    ias[1].pedir("digitar", json!({ "aba": abas[0], "texto": "uptime\n" }));
    esperar(
        &mut m.app,
        |a| a.visible("mcp-aprovar"),
        "o diálogo da posse",
    );
    conferir(&mut m.app, "posse", nome);
}

nos_dois_temas!(
    a_aprovacao_de_um_comando_simples,
    a_aprovacao_de_um_comando_bloqueado,
    a_aprovacao_com_caracteres_de_controle,
    a_aprovacao_de_uma_edicao_mostra_o_antes_e_o_depois,
    as_configuracoes_com_o_controle_ligado_e_um_espaco,
    o_como_ligar_do_claude_code_com_um_espaco,
    a_categoria_do_controle_nas_propriedades_em_livre,
    a_aba_marcada_e_a_faixa_com_parar_a_ia,
    a_grade_com_quatro_sessoes_e_o_parar_aberto,
    a_posse_de_uma_aba_pergunta_pela_outra_sessao,
);
