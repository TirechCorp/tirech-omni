//! As ferramentas do MCP das portas do servidor (M58, fatia 8) contra um servidor SSH local de
//! verdade: `listar_portas`, `encaminhar_porta` e `parar_porta`, com o `ss` programado e um
//! "site" de verdade na porta que ele lista.

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;
#[path = "comum/servidor.rs"]
mod servidor;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
use tirech_omni::mcp::permissao::PermissaoIa;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Pasta, montar_com};
use servidor::Servidor;

/// Abre a sessão SSH de verdade no servidor de teste, sem perguntar nada.
struct AbreSsh(Rc<Servidor>);

impl Opener for AbreSsh {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        Session::new(self.0.builder().connect())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }
}

/// Um "site" local: responde a qualquer conexão com uma página de título `Teste`.
fn site() -> u16 {
    let escuta = TcpListener::bind("127.0.0.1:0").expect("porta do site");
    let porta = escuta.local_addr().expect("endereço do site").port();
    thread::spawn(move || {
        for mut conexao in escuta.incoming().flatten() {
            thread::spawn(move || {
                let mut lido = [0u8; 1024];
                let _ = conexao.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = conexao.read(&mut lido);
                let corpo = "<html><head><title>Teste</title></head><body>oi</body></html>";
                let _ = write!(
                    conexao,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
                    corpo.len()
                );
            });
        }
    });
    porta
}

struct Cenario {
    _pasta: Pasta,
    _servidor: Rc<Servidor>,
    app: TestApp,
    ia: Assistente,
    /// A porta do site que o `ss` programado lista.
    porta: u16,
}

impl Cenario {
    fn novo(nome: &str, permissao: PermissaoIa) -> Self {
        let pasta = Pasta::nova(nome);
        let servidor = Rc::new(Servidor::novo(&pasta.caminho().join("servidor")));
        let porta = site();
        servidor.programar(
            "ss -ltnpeH",
            &format!(
                "LISTEN 0 511 127.0.0.1:{porta} 0.0.0.0:* users:((\"nginx\",pid=7,fd=6)) uid:33 ino:99\n\
                 LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=1,fd=3)) uid:0 ino:11\n"
            ),
            0,
        );
        let mut tree = Tree::default();
        let mut c = Connection::new("local", Kind::Ssh);
        c.host = "127.0.0.1".into();
        c.port = servidor.porta;
        c.user = "ana".into();
        c.key_file = Some("/chaves/ana".into());
        c.mcp = permissao;
        tree.add_connection(None, c);
        pasta.semear(&tree);
        pasta.configurar("ia", "mcp_ligado", json!(true));
        pasta.configurar("ia", "verificar_ao_abrir", json!(false));
        let endereco = pasta.0.join("mcp.sock");
        let (mut app, _fila) = montar_com(&pasta, Theme::Light, AbreSsh(Rc::clone(&servidor)));
        let ia = Assistente::ligar(&mut app, &endereco, "Teste");
        Self {
            _pasta: pasta,
            _servidor: servidor,
            app,
            ia,
            porta,
        }
    }

    fn chamar(&mut self, ferramenta: &str, args: Value) -> assistente::Resposta {
        self.ia.chamar(&mut self.app, ferramenta, args)
    }

    /// Espera de verdade: o servidor e as threads andam em tempo real.
    fn esperar(&mut self, o_que: &str, mut pronto: impl FnMut() -> bool) {
        let limite = Instant::now() + Duration::from_secs(15);
        while !pronto() {
            assert!(Instant::now() < limite, "esperando {o_que}");
            self.app.wait(Duration::from_millis(20));
        }
    }
}

impl Drop for Cenario {
    fn drop(&mut self) {
        if let Some(s) = Session::find("1-1") {
            s.dispose();
        }
    }
}

/// O JSON que a ferramenta respondeu (sem o envelope de texto do servidor, quando há).
fn json_de(texto: &str) -> Value {
    let inicio = texto.find(['[', '{']).expect("a resposta tem JSON");
    let fim = texto.rfind([']', '}']).expect("a resposta tem JSON");
    serde_json::from_str(&texto[inicio..=fim]).expect("JSON válido")
}

/// Pede a página `/` em `127.0.0.1:porta` e devolve a resposta.
fn buscar(porta: u16) -> String {
    let mut conexao = TcpStream::connect(("127.0.0.1", porta)).expect("a porta daqui escuta");
    conexao
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        conexao,
        "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut texto = String::new();
    let _ = conexao.read_to_string(&mut texto);
    texto
}

#[test]
fn listar_portas_abre_a_aba_e_mostra_o_que_escuta() {
    let mut c = Cenario::novo("mcp-portas-listar", PermissaoIa::Livre);
    let r = c.chamar("listar_portas", json!({ "conexao": 1 }));
    assert!(!r.e_erro(), "{}", r.texto());
    let lista = json_de(&r.texto());
    let itens = lista.as_array().expect("uma lista");
    // A 22 é do sistema e fica de fora, como no painel.
    assert_eq!(itens.len(), 1, "{lista}");
    let item = &itens[0];
    assert_eq!(item["porta"], c.porta);
    assert_eq!(item["alcance"], "servidor");
    assert_eq!(item["processo"], "nginx");
    assert!(item.get("encaminhada").is_none());
}

#[test]
fn encaminhar_porta_devolve_o_endereco_e_reaproveita_o_tunel() {
    let mut c = Cenario::novo("mcp-portas-encaminhar", PermissaoIa::Livre);
    let r = c.chamar(
        "encaminhar_porta",
        json!({ "conexao": 1, "porta": c.porta }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    let resposta = json_de(&r.texto());
    let local = u16::try_from(resposta["porta_local"].as_u64().unwrap()).unwrap();
    assert_eq!(resposta["endereco"], format!("127.0.0.1:{local}"));
    assert!(buscar(local).contains("Teste"));

    // De novo: o mesmo túnel.
    let outra = c.chamar(
        "encaminhar_porta",
        json!({ "conexao": 1, "porta": c.porta }),
    );
    assert_eq!(json_de(&outra.texto())["porta_local"], local);

    // E o `listar_portas` já mostra por onde.
    let lista = c.chamar("listar_portas", json!({ "conexao": 1 }));
    let lista = json_de(&lista.texto());
    assert_eq!(lista[0]["encaminhada"]["porta_local"], local);
}

#[test]
fn encaminhar_com_nome_de_site_passa_pelo_rele_e_devolve_a_url() {
    let mut c = Cenario::novo("mcp-portas-site", PermissaoIa::Livre);
    let r = c.chamar(
        "encaminhar_porta",
        json!({ "conexao": 1, "porta": c.porta, "host_http": "loja.exemplo.com.br" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    let resposta = json_de(&r.texto());
    let local = u16::try_from(resposta["porta_local"].as_u64().unwrap()).unwrap();
    let url = resposta["url"].as_str().expect("a URL do site");
    assert!(
        url.starts_with("http://") && url.contains(".localhost:"),
        "{url}"
    );
    assert!(buscar(local).contains("Teste"));
}

#[test]
fn parar_porta_fecha_o_tunel() {
    let mut c = Cenario::novo("mcp-portas-parar", PermissaoIa::Livre);
    let r = c.chamar(
        "encaminhar_porta",
        json!({ "conexao": 1, "porta": c.porta }),
    );
    let local = u16::try_from(json_de(&r.texto())["porta_local"].as_u64().unwrap()).unwrap();

    let parou = c.chamar("parar_porta", json!({ "conexao": 1, "porta_local": local }));
    assert!(!parou.e_erro(), "{}", parou.texto());
    assert_eq!(parou.texto(), "parado");
    c.esperar("a porta fechar", || {
        TcpStream::connect(("127.0.0.1", local)).is_err()
    });

    let de_novo = c.chamar("parar_porta", json!({ "conexao": 1, "porta_local": local }));
    assert!(de_novo.e_erro());
}

#[test]
fn porta_invalida_e_recusada() {
    let mut c = Cenario::novo("mcp-portas-invalida", PermissaoIa::Livre);
    for porta in [json!(0), json!(70000), json!("abc"), Value::Null] {
        let r = c.chamar("encaminhar_porta", json!({ "conexao": 1, "porta": porta }));
        assert!(r.e_erro(), "{porta}: {}", r.texto());
    }
}

#[test]
fn so_ler_deixa_listar_e_recusa_encaminhar() {
    let mut c = Cenario::novo("mcp-portas-so-ler", PermissaoIa::SoLer);
    let lista = c.chamar("listar_portas", json!({ "conexao": 1 }));
    assert!(!lista.e_erro(), "{}", lista.texto());
    let r = c.chamar(
        "encaminhar_porta",
        json!({ "conexao": 1, "porta": c.porta }),
    );
    assert!(r.e_erro(), "{}", r.texto());
    let r = c.chamar("parar_porta", json!({ "conexao": 1, "porta_local": 9 }));
    assert!(r.e_erro(), "{}", r.texto());
}

#[test]
fn https_e_aceito_no_esquema() {
    let mut c = Cenario::novo("mcp-portas-https", PermissaoIa::Livre);
    // A porta do site fala HTTP puro: o relé abre mesmo assim, e o argumento não é recusado.
    let r = c.chamar(
        "encaminhar_porta",
        json!({ "conexao": 1, "porta": c.porta, "https": true }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(json_de(&r.texto())["url"].is_string(), "{}", r.texto());
}
