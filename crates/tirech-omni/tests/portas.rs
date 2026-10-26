//! As portas do servidor (M58) contra um servidor SSH local de verdade: a descoberta por `ss`
//! programado, o encaminhamento para uma porta livre, o reuso do túnel, fixar na conexão e o
//! diálogo "Encaminhar destino…" com o destino fechado.

#![cfg(unix)]

mod comum;
#[path = "comum/portas.rs"]
#[allow(dead_code)]
mod fixtura;
#[path = "comum/servidor.rs"]
mod servidor;

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
use tirech_omni::links::DestinoDoLink;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::kurbo::Point;
use tomni_ui::terminal::ssh::ForwardState;
use tomni_ui::terminal::{Session, TerminalVisual};
use tomni_ui::theme::Theme;
use tomni_ui::{Look, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Pasta, montar_com};
use fixtura::{aparece, escrever};
use servidor::Servidor;

/// Espera de verdade (o servidor e as threads do painel andam em tempo real, não no relógio do
/// teste): dorme um pouco e desenha um quadro, até `pronto` ou 20 s.
fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(20);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        thread::sleep(Duration::from_millis(30));
        app.frame();
    }
}

/// Abre a sessão SSH de verdade no servidor de teste e anota os links que o app abriria.
struct AbreReal {
    servidor: Rc<Servidor>,
    links: Rc<RefCell<Vec<DestinoDoLink>>>,
}

impl Opener for AbreReal {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        Session::new(self.servidor.builder().connect())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }

    fn abrir_link(&self, destino: &DestinoDoLink) -> std::io::Result<()> {
        self.links.borrow_mut().push(destino.clone());
        Ok(())
    }
}

/// Um "site" local: responde a qualquer conexão com uma página de título `Teste`.
fn site() -> u16 {
    let escuta = TcpListener::bind("127.0.0.1:0").expect("porta do site");
    let porta = escuta.local_addr().expect("endereço do site").port();
    thread::spawn(move || {
        for conexao in escuta.incoming().flatten() {
            thread::spawn(move || responder(conexao));
        }
    });
    porta
}

fn responder(mut conexao: TcpStream) {
    let mut lido = [0u8; 1024];
    let _ = conexao.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = conexao.read(&mut lido);
    let corpo = "<html><head><title>Teste</title></head><body>oi</body></html>";
    let _ = write!(
        conexao,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
}

struct Montagem {
    pasta: Pasta,
    app: TestApp,
    servidor: Rc<Servidor>,
    links: Rc<RefCell<Vec<DestinoDoLink>>>,
    /// A porta do site que o `ss` programado lista.
    porta: u16,
}

/// A aba SSH aberta contra o servidor de teste, com o painel de portas dela.
fn abrir(nome: &str) -> Montagem {
    abrir_com(nome, false)
}

/// Como [`abrir`], com o aviso de porta nova (`avisar_novas`) ligado ou não.
fn abrir_com(nome: &str, avisar_novas: bool) -> Montagem {
    montar(nome, avisar_novas, Vec::new())
}

/// Como [`abrir`], com `tuneis` já fixados na conexão.
fn montar(nome: &str, avisar_novas: bool, tuneis: Vec<tirech_omni::modelo::Tunel>) -> Montagem {
    let pasta = Pasta::nova(nome);
    if avisar_novas {
        pasta.configurar("portas", "avisar_novas", true.into());
        pasta.configurar("portas", "intervalo_secs", 1.into());
    }
    let servidor = Rc::new(Servidor::novo(&pasta.caminho().join("servidor")));
    let porta = site();
    servidor.programar(
        "ss -ltnpeH",
        &format!(
            "LISTEN 0 511 127.0.0.1:{porta} 0.0.0.0:* users:((\"nginx\",pid=7,fd=6)) uid:33 ino:99\n"
        ),
        0,
    );
    let mut tree = Tree::default();
    let mut c = Connection::new("local", Kind::Ssh);
    c.host = "127.0.0.1".into();
    c.port = servidor.porta;
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    c.tuneis = tuneis;
    tree.add_connection(None, c);
    pasta.semear(&tree);
    let links: Rc<RefCell<Vec<DestinoDoLink>>> = Rc::default();
    let (mut app, _fila) = montar_com(
        &pasta,
        Theme::Light,
        AbreReal {
            servidor: Rc::clone(&servidor),
            links: Rc::clone(&links),
        },
    );
    app.double_click("arvore-1");
    let chave = app.panes("paineis").panes[0].tabs[0].0.clone();
    app.context_menu(&format!("paineis-painel-0-{chave}"));
    app.click("portas-da-sessao");
    Montagem {
        pasta,
        app,
        servidor,
        links,
        porta,
    }
}

impl Montagem {
    fn linha(&self) -> String {
        format!("portas-srv-{}", self.porta)
    }

    fn sessao(&self) -> Session {
        Session::find("1-1").expect("a sessão da aba")
    }

    /// As portas locais dos túneis `-L` da sessão que escutam.
    fn tuneis_escutando(&self) -> Vec<String> {
        self.sessao()
            .ssh_forwards()
            .expect("a sessão é SSH")
            .list()
            .into_iter()
            .filter(|s| s.state == ForwardState::Listening)
            .filter_map(|s| s.listening)
            .collect()
    }

    fn esperar_link(&mut self, quantos: usize) {
        let links = Rc::clone(&self.links);
        esperar(&mut self.app, "o link abrir", move |_| {
            links.borrow().len() >= quantos
        });
    }

    fn ultimo_link(&self) -> String {
        match self.links.borrow().last() {
            Some(DestinoDoLink::Navegador(url)) => url.clone(),
            outro => panic!("esperava um link do navegador, veio {outro:?}"),
        }
    }
}

impl Drop for Montagem {
    fn drop(&mut self) {
        if let Some(s) = Session::find("1-1") {
            s.dispose();
        }
        let _ = &self.pasta;
    }
}

/// A porta da URL `http://localhost:{porta}/…`.
fn porta_da_url(url: &str) -> u16 {
    url.strip_prefix("http://localhost:")
        .and_then(|resto| resto.split('/').next())
        .and_then(|porta| porta.parse().ok())
        .unwrap_or_else(|| panic!("URL inesperada: {url}"))
}

/// Pede `/` pela porta `porta` deste computador e devolve a resposta.
fn pedir(porta: u16) -> String {
    let mut conexao = TcpStream::connect(("127.0.0.1", porta)).expect("conectar no túnel");
    conexao
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("limite de leitura");
    conexao
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("pedir a página");
    let mut resposta = String::new();
    let _ = conexao.read_to_string(&mut resposta);
    resposta
}

#[test]
fn a_descoberta_por_ss_e_a_sonda_dao_nome_ao_site() {
    let mut m = abrir("portas-descoberta");
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    // A sonda sai pelo canal SSH e acha o título da página.
    esperar(&mut m.app, "a sonda", |a| {
        a.label(&format!("{linha}-principal")) == "Abrir"
    });
    assert!(
        m.servidor
            .tuneis()
            .contains(&format!("127.0.0.1:{}", m.porta))
    );
}

#[test]
fn abrir_encaminha_para_outra_porta_quando_a_mesma_esta_ocupada() {
    let mut m = abrir("portas-abrir");
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    esperar(&mut m.app, "a sonda", |a| {
        a.label(&format!("{linha}-principal")) == "Abrir"
    });
    m.app.click(&format!("{linha}-principal"));
    m.esperar_link(1);

    let local = porta_da_url(&m.ultimo_link());
    // O site já ocupa a mesma porta no computador: o app escolheu outra.
    assert_ne!(local, m.porta);
    assert!(pedir(local).contains("<title>Teste</title>"));
    esperar(&mut m.app, "o botão mostrar a porta daqui", |a| {
        a.label(&format!("{linha}-principal")) == format!("localhost:{local}")
    });
}

#[test]
fn abrir_de_novo_reaproveita_o_tunel() {
    let mut m = abrir("portas-reuso");
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    esperar(&mut m.app, "a sonda", |a| {
        a.label(&format!("{linha}-principal")) == "Abrir"
    });
    m.app.click(&format!("{linha}-principal"));
    m.esperar_link(1);
    let primeira = m.ultimo_link();
    m.app.click(&format!("{linha}-principal"));
    m.esperar_link(2);

    assert_eq!(m.ultimo_link(), primeira);
    assert_eq!(m.tuneis_escutando().len(), 1);
}

#[test]
fn parar_encerra_o_tunel_e_o_botao_volta_a_abrir() {
    let mut m = abrir("portas-parar");
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    esperar(&mut m.app, "a sonda", |a| {
        a.label(&format!("{linha}-principal")) == "Abrir"
    });
    m.app.click(&format!("{linha}-principal"));
    m.esperar_link(1);
    esperar(&mut m.app, "a seção Encaminhadas", |a| {
        aparece(a, "portas-encaminhadas")
    });

    m.app.click(&format!("{linha}-mais"));
    m.app.click(&format!("{linha}-parar"));
    esperar(&mut m.app, "o botão voltar a Abrir", |a| {
        a.label(&format!("{linha}-principal")) == "Abrir"
    });
    assert!(m.tuneis_escutando().is_empty());
    assert!(!aparece(&m.app, "portas-encaminhadas"));
}

#[test]
fn fixar_grava_o_tunel_na_conexao_e_desafixar_tira() {
    let mut m = abrir("portas-fixar");
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    esperar(&mut m.app, "a sonda", |a| {
        a.label(&format!("{linha}-principal")) == "Abrir"
    });

    m.app.click(&format!("{linha}-mais"));
    m.app.click(&format!("{linha}-fixar"));
    let pasta = m.pasta.0.clone();
    let porta = m.porta;
    // O arquivo é regravado de vez em quando: lê até achar, sem reler depois.
    let achado: RefCell<Option<tirech_omni::modelo::Tunel>> = RefCell::default();
    esperar(&mut m.app, "o túnel gravado", |_| {
        let t = gravados(&pasta)
            .into_iter()
            .find(|t| t.destino_porta == porta);
        let achou = t.is_some();
        if achou {
            *achado.borrow_mut() = t;
        }
        achou
    });
    let t = achado.into_inner().expect("o túnel fixado");
    assert_eq!(t.destino_host, "127.0.0.1");
    assert!(t.proxima_livre);
    assert!(t.ligado);
    assert_ne!(t.porta, porta);
    // Fixar de novo não duplica.
    m.app.click(&format!("{linha}-mais"));
    m.app.click(&format!("{linha}-desafixar"));
    esperar(&mut m.app, "o túnel sair", |_| {
        !gravados(&pasta).iter().any(|t| t.destino_porta == porta)
    });
}

fn gravados(pasta: &std::path::Path) -> Vec<tirech_omni::modelo::Tunel> {
    let pasta = Pasta(pasta.to_path_buf());
    let tree = pasta.conexoes();
    tree.connections()
        .into_iter()
        .flat_map(|c| c.tuneis.clone())
        .collect()
}

#[test]
fn o_destino_fechado_mostra_o_erro_e_encaminhar_mesmo_assim_funciona() {
    let mut m = abrir("portas-destino-fechado");
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    esperar(&mut m.app, "a sonda", |a| {
        a.label(&format!("{linha}-principal")) == "Abrir"
    });
    m.servidor.negar_direct(true);

    m.app.click("portas-destino");
    assert!(aparece(&m.app, "portas-dialogo"));
    // Sem host e porta, "Encaminhar" não vale.
    assert!(!m.app.enabled("portas-dialogo-encaminhar"));
    escrever(&mut m.app, "portas-dialogo-host", "127.0.0.1");
    m.app.set_value("portas-dialogo-porta", 5432.0);
    assert!(m.app.enabled("portas-dialogo-encaminhar"));
    m.app.click("portas-dialogo-encaminhar");
    esperar(&mut m.app, "o erro do destino", |a| {
        aparece(a, "portas-dialogo-erro") && a.visible("portas-dialogo-erro")
    });
    assert!(aparece(&m.app, "portas-dialogo-mesmo"));

    m.app.click("portas-dialogo-mesmo");
    assert!(!aparece(&m.app, "portas-dialogo"));
    let sessao = m.sessao();
    esperar(&mut m.app, "o túnel escutar", |_| {
        sessao
            .ssh_forwards()
            .is_some_and(|f| f.list().iter().any(|s| s.state == ForwardState::Listening))
    });
}

#[test]
fn o_destino_que_responde_encaminha_sem_pedir_mais() {
    let mut m = abrir("portas-destino-bom");
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));

    m.app.click("portas-destino");
    escrever(&mut m.app, "portas-dialogo-host", "127.0.0.1");
    m.app.set_value("portas-dialogo-porta", f64::from(m.porta));
    m.app.click("portas-dialogo-encaminhar");
    esperar(&mut m.app, "o diálogo fechar", |a| {
        !aparece(a, "portas-dialogo")
    });
    esperar(&mut m.app, "a seção Encaminhadas", |a| {
        aparece(a, "portas-encaminhadas")
    });
    assert_eq!(m.tuneis_escutando().len(), 1);
}

/// O centro da célula (`col`, `linha`) do terminal `id`.
fn celula(app: &TestApp, id: &str, col: usize, linha: usize) -> Point {
    match app.look(id) {
        Look::Custom(custom) => custom
            .downcast::<TerminalVisual>()
            .expect("o visual do terminal")
            .cell_rect(app.rect(id), linha, col, 1)
            .center(),
        outro => panic!("não é o terminal: {outro:?}"),
    }
}

fn ctrl_clique(app: &mut TestApp, id: &str, col: usize, linha: usize) {
    let ponto = celula(app, id, col, linha);
    app.ui().set_modifiers(Modifiers::CTRL);
    app.click_at(ponto, 1);
    app.ui().set_modifiers(Modifiers::NONE);
}

#[test]
fn o_localhost_impresso_no_terminal_abre_pelo_tunel() {
    let mut m = abrir("portas-link");
    let terminal = "paineis-terminal-1-1";
    m.app.feed(
        terminal,
        format!("\x1b[2J\x1b[HServidor em http://localhost:{}/\r\n", m.porta),
    );
    // "Servidor em " tem 12 colunas: a URL começa na 12.
    ctrl_clique(&mut m.app, terminal, 14, 0);
    m.esperar_link(1);

    let local = porta_da_url(&m.ultimo_link());
    assert_ne!(local, m.porta);
    assert!(pedir(local).contains("<title>Teste</title>"));
    assert_eq!(m.tuneis_escutando().len(), 1);
}

#[test]
fn a_porta_nova_avisa_uma_vez() {
    let mut m = abrir_com("portas-nova", true);
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    // A primeira leitura não conta como nova.
    assert!(!aparece(&m.app, "portas-nova"));

    m.servidor.programar(
        "ss -ltnpeH",
        &format!(
            "LISTEN 0 511 127.0.0.1:{} 0.0.0.0:* users:((\"nginx\",pid=7,fd=6)) uid:33 ino:99\n\
             LISTEN 0 511 127.0.0.1:3999 0.0.0.0:* users:((\"node\",pid=8,fd=7)) uid:1000 ino:100\n",
            m.porta
        ),
        0,
    );
    esperar(&mut m.app, "o aviso", |a| aparece(a, "portas-nova"));
    assert!(m.app.text("portas-nova").contains("3999"));

    // "Ver portas" fecha o aviso; as próximas leituras são iguais e não avisam de novo.
    m.app.click("portas-nova-acao");
    assert!(!aparece(&m.app, "portas-nova"));
    let limite = Instant::now() + Duration::from_secs(3);
    while Instant::now() < limite {
        thread::sleep(Duration::from_millis(30));
        m.app.frame();
        assert!(!aparece(&m.app, "portas-nova"));
    }
}

/// Um site que devolve o que recebeu: o corpo é a cabeça do pedido, e a resposta traz um
/// `Location` e um `Set-Cookie` com o nome verdadeiro do site. Com `tls`, fala TLS.
fn site_que_ecoa(tls: Option<Arc<rustls::ServerConfig>>) -> u16 {
    let escuta = TcpListener::bind("127.0.0.1:0").expect("porta do site");
    let porta = escuta.local_addr().expect("endereço do site").port();
    thread::spawn(move || {
        for conexao in escuta.incoming().flatten() {
            let tls = tls.clone();
            thread::spawn(move || {
                let _ = conexao.set_read_timeout(Some(Duration::from_secs(5)));
                match tls {
                    None => ecoar(conexao),
                    Some(config) => {
                        if let Ok(sessao) = rustls::ServerConnection::new(config) {
                            ecoar(rustls::StreamOwned::new(sessao, conexao));
                        }
                    }
                }
            });
        }
    });
    porta
}

fn ecoar(mut fluxo: impl Read + Write) {
    let mut cabeca = Vec::new();
    let mut pedaco = [0u8; 512];
    while !cabeca.windows(4).any(|j| j == b"\r\n\r\n") {
        match fluxo.read(&mut pedaco) {
            Ok(0) | Err(_) => return,
            Ok(n) => cabeca.extend_from_slice(&pedaco[..n]),
        }
    }
    let corpo = String::from_utf8_lossy(&cabeca).into_owned();
    let _ = write!(
        fluxo,
        "HTTP/1.1 302 Found\r\nLocation: http://loja.exemplo.com.br/painel\r\n\
         Set-Cookie: sessao=1; Domain=loja.exemplo.com.br; Path=/\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
    let _ = fluxo.flush();
}

/// Pede `/` pela porta `porta` com o `Host` que o navegador mandaria.
fn pedir_como(porta: u16, host: &str) -> String {
    let mut conexao = TcpStream::connect(("127.0.0.1", porta)).expect("conectar no relé");
    conexao
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("limite de leitura");
    write!(
        conexao,
        "GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .expect("pedir a página");
    let mut resposta = String::new();
    let _ = conexao.read_to_string(&mut resposta);
    resposta
}

/// O servidor TLS de teste: um certificado autoassinado para `loja.exemplo`, que o nome do
/// destino (`127.0.0.1`) não confere.
fn config_tls() -> Arc<rustls::ServerConfig> {
    let emitido = rcgen::generate_simple_self_signed(vec!["loja.exemplo".to_owned()])
        .expect("certificado de teste");
    let chave = rustls::pki_types::PrivatePkcs8KeyDer::from(emitido.signing_key.serialize_der());
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("versões do TLS")
    .with_no_client_auth()
    .with_single_cert(vec![emitido.cert.der().clone()], chave.into())
    .expect("certificado do servidor");
    Arc::new(config)
}

/// Abre o diálogo "Encaminhar destino…" para `127.0.0.1:{porta}` com as opções da web.
fn encaminhar_site(m: &mut Montagem, porta: u16, site: &str, https: bool, fixar: bool) {
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    m.app.click("portas-destino");
    escrever(&mut m.app, "portas-dialogo-host", "127.0.0.1");
    m.app.set_value("portas-dialogo-porta", f64::from(porta));
    m.app.click("portas-dialogo-opcoes-web");
    if !site.is_empty() {
        escrever(&mut m.app, "portas-dialogo-site", site);
    }
    if https {
        m.app.click("portas-dialogo-https");
    }
    if fixar {
        m.app.click("portas-dialogo-fixar");
    }
    m.app.click("portas-dialogo-abrir");
    m.app.click("portas-dialogo-encaminhar");
    m.esperar_link(1);
}

#[test]
fn o_nome_do_site_vira_host_e_reescreve_location_e_cookie() {
    let mut m = abrir("portas-site-por-nome");
    let eco = site_que_ecoa(None);
    encaminhar_site(&mut m, eco, "loja.exemplo.com.br", false, false);

    let url = m.ultimo_link();
    let local: u16 = url
        .strip_prefix("http://loja-exemplo-com-br.localhost:")
        .and_then(|resto| resto.split('/').next())
        .and_then(|porta| porta.parse().ok())
        .unwrap_or_else(|| panic!("URL inesperada: {url}"));
    let apelido = format!("loja-exemplo-com-br.localhost:{local}");
    let resposta = pedir_como(local, &apelido);
    // O servidor recebeu o nome verdadeiro do site no Host.
    assert!(
        resposta.contains("Host: loja.exemplo.com.br\r\n"),
        "{resposta}"
    );
    // O endereço do redirecionamento volta para o nome daqui, e o cookie perde o Domain.
    assert!(
        resposta.contains(&format!("Location: http://{apelido}/painel\r\n")),
        "{resposta}"
    );
    assert!(
        resposta.contains("Set-Cookie: sessao=1; Path=/\r\n"),
        "{resposta}"
    );
    esperar(&mut m.app, "a linha do relé", |a| {
        aparece(a, &format!("portas-enc-r{local}"))
    });
}

#[test]
fn o_servidor_https_com_certificado_que_nao_confere_abre_e_avisa() {
    let mut m = abrir("portas-https");
    let tls = site_que_ecoa(Some(config_tls()));
    encaminhar_site(&mut m, tls, "", true, false);

    let url = m.ultimo_link();
    // O navegador fala HTTP: quem fala TLS com o servidor é o relé.
    assert!(url.starts_with("http://localhost:"), "{url}");
    let local = porta_da_url(&url);
    let resposta = pedir_como(local, &format!("localhost:{local}"));
    assert!(resposta.starts_with("HTTP/1.1 302"), "{resposta}");
    let estado = format!("portas-enc-r{local}-estado");
    // O relógio do painel (1 s) é o do teste: deixa-o andar para a linha renovar.
    m.app.wait(Duration::from_secs(2));
    esperar(&mut m.app, "o aviso do certificado", |a| {
        aparece(a, &estado)
            && matches!(a.look(&estado), Look::Text(t) if t.text.contains("certificado não confere"))
    });
}

#[test]
fn fixar_o_site_grava_o_nome_e_nao_cria_tunel_do_ssh() {
    let mut m = abrir("portas-site-fixado");
    let eco = site_que_ecoa(None);
    encaminhar_site(&mut m, eco, "loja.exemplo.com.br", false, true);

    let pasta = m.pasta.0.clone();
    let achado: RefCell<Option<tirech_omni::modelo::Tunel>> = RefCell::default();
    esperar(&mut m.app, "o túnel gravado", |_| {
        let t = gravados(&pasta)
            .into_iter()
            .find(|t| t.destino_porta == eco);
        let achou = t.is_some();
        if achou {
            *achado.borrow_mut() = t;
        }
        achou
    });
    let t = achado.into_inner().expect("o túnel fixado");
    assert_eq!(t.host_http, "loja.exemplo.com.br");
    assert!(t.usa_rele());
    // O túnel do nome de site é do relé do app: o SSH não recebe um `-L`.
    assert!(m.tuneis_escutando().is_empty());
}

#[test]
fn o_tunel_fixado_com_nome_de_site_sobe_com_a_sessao() {
    let eco = site_que_ecoa(None);
    let mut tunel = tirech_omni::modelo::Tunel::novo(tirech_omni::modelo::TipoTunel::Local, false);
    tunel.porta = TcpListener::bind("127.0.0.1:0")
        .expect("porta livre")
        .local_addr()
        .expect("endereço")
        .port();
    tunel.destino_host = "127.0.0.1".into();
    tunel.destino_porta = eco;
    tunel.proxima_livre = true;
    tunel.ligado = true;
    tunel.host_http = "loja.exemplo.com.br".into();
    let mut m = montar("portas-site-sobe", false, vec![tunel]);
    let linha = m.linha();
    esperar(&mut m.app, "a porta na lista", |a| aparece(a, &linha));
    // O relé subiu sozinho: a seção Encaminhadas mostra o nome do site.
    esperar(&mut m.app, "a seção Encaminhadas", |a| {
        aparece(a, "portas-encaminhadas")
    });
    assert!(m.tuneis_escutando().is_empty());
}
