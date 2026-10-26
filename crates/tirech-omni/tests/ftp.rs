//! O FTP e o FTPS no app (M31): as opções na grade de propriedades, a porta que acompanha a
//! segurança, o `connections.json` antigo, a abertura na página Arquivos e a pergunta do
//! certificado. O servidor é um `MemFs`, como nos testes do SFTP.

mod comum;

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use tirech_omni::abrir::{
    Opener, Senhas, certificado_confiado, conferir_certificado, ftp_builder, ler_fila_agora,
    precisa_de_senha,
};
use tirech_omni::modelo::{Connection, FtpCodificacao, FtpModo, FtpSeguranca, FtpTls, Kind, Tree};
use tomni_ui::Key;
use tomni_ui::files::ftp::CertificateInfo;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, escolher_propriedade, montar, valor};

/// Uma conexão FTP anônima (id 1), que abre sem pedir senha.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    let mut ftp = Connection::new("arquivos", Kind::Ftp);
    ftp.host = "ftp.exemplo".into();
    tree.add_connection(None, ftp);
    tree
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

fn certificado() -> CertificateInfo {
    CertificateInfo {
        host: "ftp.exemplo".into(),
        sha256: "AB:CD:EF:01".into(),
        subject: "CN=ftp.exemplo".into(),
        issuer: "CN=ftp.exemplo".into(),
        not_before: SystemTime::UNIX_EPOCH,
        not_after: SystemTime::UNIX_EPOCH + Duration::from_secs(365 * 86_400),
        error: "o certificado é autoassinado".into(),
    }
}

#[test]
fn a_grade_mostra_as_opcoes_do_ftp_com_os_padroes() {
    let pasta = Pasta::nova("ftp-opcoes");
    pasta.semear(&arvore());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    assert_eq!(valor(&m.app, "Tipo"), "FTP");
    assert_eq!(valor(&m.app, "Porta"), "21");
    assert_eq!(valor(&m.app, "Segurança"), "FTP sobre TLS explícito");
    assert_eq!(valor(&m.app, "Modo"), "Passivo");
    assert_eq!(valor(&m.app, "Versão do TLS"), "Automática");
    assert_eq!(valor(&m.app, "Codificação"), "Automática");
    let props = m.app.properties("props");
    let ativo = |nome: &str| props.iter().find(|p| p.name == nome).unwrap().enabled;
    let escondida = |nome: &str| props.iter().find(|p| p.name == nome).unwrap().hidden;
    assert!(ativo("Segurança") && ativo("Usuário") && ativo("Credencial"));
    assert!(escondida("Chave privada"), "o FTP não usa chave");
    assert!(!ativo("Portas do modo ativo"), "só no modo ativo");
    m.app.assert_lint();
}

#[test]
fn as_opcoes_do_ftp_vao_para_o_arquivo_e_a_porta_acompanha_a_seguranca() {
    let pasta = Pasta::nova("ftp-porta");
    pasta.semear(&arvore());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");

    escolher_propriedade(&mut m.app, "Segurança");
    m.app.select("props-Segurança-valor", "FTPS implícito");
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!(c.ftp_security, FtpSeguranca::Implicita);
    assert_eq!(c.port, 990, "a porta estava no padrão");
    assert_eq!(valor(&m.app, "Porta"), "990");

    m.app
        .select("props-Segurança-valor", "FTP simples (sem TLS)");
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!((c.ftp_security, c.port), (FtpSeguranca::Nenhuma, 21));
}

/// Troca uma propriedade numa tela nova (depois de um seletor o foco fica nele) e devolve a
/// conexão gravada.
fn trocar(nome: &str, valor: &str) -> Connection {
    let pasta = Pasta::nova(&format!("ftp-{nome}").replace(' ', "-"));
    pasta.semear(&arvore());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    if nome != "Modo" {
        // "FTP avançado" começa recolhida: desce da última linha aberta até o cabeçalho e abre.
        escolher_propriedade(&mut m.app, "Modo");
        m.app.key(Key::Down).key(Key::Right);
    }
    escolher_propriedade(&mut m.app, nome);
    m.app.select(&format!("props-{nome}-valor"), valor);
    pasta.conexoes().connection(1).cloned().unwrap()
}

#[test]
fn modo_codificacao_e_versao_do_tls_vao_para_o_arquivo() {
    assert_eq!(trocar("Modo", "Ativo").ftp_mode, FtpModo::Ativo);
    assert_eq!(
        trocar("Codificação", "Latin-1").ftp_encoding,
        FtpCodificacao::Latin1
    );
    assert_eq!(trocar("Versão do TLS", "Só TLS 1.2").ftp_tls, FtpTls::Tls12);
}

#[test]
fn uma_porta_personalizada_nao_muda_com_a_seguranca() {
    let pasta = Pasta::nova("ftp-porta-propria");
    let mut tree = arvore();
    tree.connection_mut(1).unwrap().port = 2121;
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    escolher_propriedade(&mut m.app, "Segurança");
    m.app.select("props-Segurança-valor", "FTPS implícito");
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!((c.ftp_security, c.port), (FtpSeguranca::Implicita, 2121));
}

#[test]
fn trocar_o_tipo_para_ftp_usa_a_porta_21() {
    let pasta = Pasta::nova("ftp-tipo");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("novo", Kind::Ssh));
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    escolher_propriedade(&mut m.app, "Tipo");
    m.app.select("props-Tipo-valor", "FTP");
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!((c.kind, c.port), (Kind::Ftp, 21));
}

#[test]
fn o_connections_json_antigo_carrega_com_os_padroes_do_ftp() {
    let antigo = r#"{"version":1,"next_id":2,"nodes":[{"Connection":
        {"id":1,"name":"velho","kind":"Sftp","host":"h","port":22}}]}"#;
    let arvore: Tree = serde_json::from_str(antigo).unwrap();
    let c = arvore.connection(1).unwrap();
    assert_eq!(c.kind, Kind::Sftp);
    assert_eq!(c.ftp_security, FtpSeguranca::Explicita);
    assert_eq!(c.ftp_mode, FtpModo::Passivo);
    assert_eq!(c.ftp_tls, FtpTls::Auto);
    assert_eq!(c.ftp_encoding, FtpCodificacao::Auto);
    assert_eq!((c.ftp_timezone_min, c.ftp_active_ports.as_str()), (0, ""));
}

#[test]
fn abrir_o_ftp_vai_para_a_pagina_arquivos_e_vira_recente() {
    let pasta = Pasta::nova("ftp-abrir");
    pasta.semear(&arvore());
    let mut m = montar(&pasta, Theme::Light);
    m.opener.servidor(1).put("/home/ana/leia.txt", "oi");
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a listagem remota", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"leia.txt")
    });
    assert_eq!(m.app.text("navegacao"), "arquivos");
    assert_eq!(m.app.tab_titles("arquivos-abas"), ["arquivos"]);
    assert!(m.opener.abertas.borrow().is_empty(), "sem terminal");

    // O "+" da página lista a conexão FTP junto com as SFTP.
    m.app.click("arquivos-nova-aba");
    assert!(m.app.visible("arquivos-nova-aba-1"));
    m.app.key(tomni_ui::Key::Escape);

    // O Início guarda a conexão nos recentes.
    m.app.click("navegacao-inicio");
    assert!(
        m.app.visible("inicio-recente-0"),
        "o cartão do FTP no Início"
    );
}

#[test]
fn ftp_com_usuario_e_sem_senha_no_cofre_pergunta() {
    let pasta = Pasta::nova("ftp-senha");
    let mut tree = arvore();
    tree.connection_mut(1).unwrap().user = "ana".into();
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.opener.servidor(1).put("/home/ana/oi.txt", "oi");
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-primary");
    esperar(&mut m.app, "a listagem", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"oi.txt")
    });
    assert_eq!(
        m.opener.senhas_dos_arquivos.borrow()[0].conexao.as_deref(),
        Some("segredo")
    );
}

#[test]
fn so_o_usuario_nomeado_precisa_de_senha() {
    let mut c = Connection::new("a", Kind::Ftp);
    assert!(!precisa_de_senha(&c), "anônimo");
    c.user = "Anonymous".into();
    assert!(!precisa_de_senha(&c));
    c.user = "ana".into();
    assert!(precisa_de_senha(&c));
}

/// Faz a pergunta do certificado de outra thread, como o `FtpBuilder` faz.
fn perguntar_certificado(m: &comum::Montagem, arquivo: std::path::PathBuf) -> mpsc::Receiver<bool> {
    let (envia, recebe) = mpsc::channel();
    let fila = m.fila;
    thread::spawn(move || {
        let r = conferir_certificado(fila, "arquivos", 21, Some(&arquivo), &certificado());
        let _ = envia.send(r);
    });
    recebe
}

fn esperar_resposta(app: &mut TestApp, resposta: &mpsc::Receiver<bool>) -> bool {
    let limite = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(r) = resposta.try_recv() {
            return r;
        }
        assert!(Instant::now() < limite, "a pergunta não foi respondida");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn certificado_novo_pergunta_e_confiado_nao_pergunta_de_novo() {
    let pasta = Pasta::nova("ftp-certificado");
    let arquivo = pasta.caminho().join("ssh").join("certificados.json");
    let mut m = montar(&pasta, Theme::Light);

    let resposta = perguntar_certificado(&m, arquivo.clone());
    esperar(&mut m.app, "o diálogo do certificado", |a| {
        a.is_open("certificado-dialogo")
    });
    assert_eq!(
        m.app.text("certificado-servidor"),
        "Servidor: ftp.exemplo:21"
    );
    assert!(m.app.text("certificado-assunto").contains("CN=ftp.exemplo"));
    assert!(m.app.text("certificado-impressao").contains("AB:CD EF:01"));
    assert!(
        m.app
            .text("certificado-validade")
            .starts_with("Validade: de 01/01/1970")
    );
    m.app.assert_lint();
    m.app.click("certificado-dialogo-primary");
    assert!(esperar_resposta(&mut m.app, &resposta));
    assert!(certificado_confiado(
        &arquivo,
        "ftp.exemplo:21",
        "AB:CD:EF:01"
    ));

    // Da segunda vez passa direto, sem diálogo.
    assert!(conferir_certificado(
        m.fila,
        "arquivos",
        21,
        Some(&arquivo),
        &certificado()
    ));
    assert!(ler_fila_agora(m.fila, <[_]>::is_empty));

    // Outra impressão (o certificado mudou) volta a perguntar, e recusar não grava.
    let mut outro = certificado();
    outro.sha256 = "99:99".into();
    let (envia, recebe) = mpsc::channel();
    let (fila, caminho) = (m.fila, arquivo.clone());
    thread::spawn(move || {
        let _ = envia.send(conferir_certificado(
            fila,
            "arquivos",
            21,
            Some(&caminho),
            &outro,
        ));
    });
    esperar(&mut m.app, "o diálogo outra vez", |a| {
        a.is_open("certificado-dialogo")
    });
    m.app.click("certificado-dialogo-close");
    assert!(!esperar_resposta(&mut m.app, &recebe));
    assert!(!certificado_confiado(&arquivo, "ftp.exemplo:21", "99:99"));
}

#[test]
fn o_ftp_builder_aceita_a_conexao_sem_senha_do_cofre() {
    // Só confere que montar o builder não pede nada à tela (a conexão acontece depois).
    let pasta = Pasta::nova("ftp-builder");
    let m = montar(&pasta, Theme::Light);
    let mut c = Connection::new("a", Kind::Ftp);
    c.host = "127.0.0.1".into();
    c.ftp_active_ports = "50000-50100".into();
    let _ = ftp_builder(&c, Senhas::default(), m.fila, None);
    assert!(ler_fila_agora(m.fila, <[_]>::is_empty));
    let _: &dyn Opener = &m.opener;
}
