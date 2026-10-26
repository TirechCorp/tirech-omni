//! O caminho completo, sem simulações: um shell de verdade (`sh`) numa aba do app (aba, teclado,
//! processo e a saída de volta no terminal) e a página Arquivos contra um servidor SSH local
//! com SFTP (enviar um arquivo pela tela e achá-lo no disco).

#![cfg(unix)]

mod comum;
#[path = "comum/servidor.rs"]
mod servidor;

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::Key;
use tomni_ui::files::sftp::Sftp;
use tomni_ui::files::{Fs, Opener as ArquivosOpener};
use tomni_ui::terminal::{Process, Session};
use tomni_ui::theme::Theme;

use comum::{Pasta, exemplo, montar_com};
use servidor::Servidor;

/// Abre o `sh` com um prompt curto no lugar do shell do usuário (que carregaria o `.rc` dele).
struct AbreSh;

impl Opener for AbreSh {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        Session::new(Process::builder("sh").env("PS1", "$ ").spawn())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }
}

#[test]
fn o_shell_local_responde_ao_que_se_digita() {
    let pasta = Pasta::nova("sh");
    pasta.semear(&exemplo());
    let (mut app, _fila) = montar_com(&pasta, Theme::Light, AbreSh);
    app.double_click("arvore-5");
    let terminal = "paineis-terminal-5-1";
    app.click(terminal)
        .type_into(terminal, "echo oi-$((20+22))")
        .key(Key::Enter);

    let limite = Instant::now() + Duration::from_secs(10);
    // O comando digitado aparece como "echo oi-$((20+22))"; a saída, "oi-42" sozinho na linha.
    while !app.session_text("5-1").lines().any(|l| l.trim() == "oi-42") {
        assert!(
            Instant::now() < limite,
            "o sh não respondeu em 10 s:\n{}",
            app.session_text("5-1")
        );
        app.wait(Duration::from_millis(20));
    }
    if let Some(s) = Session::find("5-1") {
        s.dispose();
    }
}

/// Abre os arquivos no servidor SSH local de teste, sem pedir senha.
struct AbreSftp(Rc<Servidor>);

impl Opener for AbreSftp {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        Session::new(Process::builder("sh").env("PS1", "$ ").spawn())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }

    fn arquivos(&self, _conexao: &Connection, _senhas: Senhas) -> Result<ArquivosOpener, String> {
        let builder = self.0.builder();
        Ok(Arc::new(move || {
            Ok(Box::new(Sftp::connect(&builder)?) as Box<dyn Fs>)
        }))
    }
}

#[test]
fn enviar_um_arquivo_pela_tela_chega_ao_disco_do_servidor() {
    let pasta = Pasta::nova("sftp-real");
    let servidor = Rc::new(Servidor::novo(&pasta.caminho().join("servidor")));
    servidor.escrever("/home/docs/.keep", b"");

    let mut tree = Tree::default();
    let mut c = Connection::new("local", Kind::Sftp);
    c.host = "127.0.0.1".into();
    c.port = servidor.porta;
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, c);
    pasta.semear(&tree);

    let (mut app, _fila) = montar_com(&pasta, Theme::Light, AbreSftp(Rc::clone(&servidor)));
    app.double_click("arvore-1");
    let limite = Instant::now() + Duration::from_secs(20);
    let esperar = |app: &mut tomni_ui_test::TestApp,
                   o_que: &str,
                   pronto: &dyn Fn(&tomni_ui_test::TestApp) -> bool| {
        while !pronto(app) {
            assert!(Instant::now() < limite, "esperando {o_que}");
            app.wait(Duration::from_millis(20));
        }
    };
    esperar(&mut app, "a pasta do servidor", &|a| {
        a.grid("a1-remoto").keys().contains(&"docs")
    });

    let arquivo = pasta.caminho().join("enviar.txt");
    std::fs::write(&arquivo, "pela tela").unwrap();
    app.wait(Duration::from_millis(50));
    app.drop_files("a1-remoto-docs", &[arquivo]);
    let destino = servidor.real("/home/docs/enviar.txt");
    esperar(&mut app, "o arquivo no disco do servidor", &|_| {
        std::fs::read(&destino).is_ok_and(|d| d == b"pela tela")
    });
    esperar(&mut app, "a fila concluir", &|a| {
        let fila = a.grid("fila");
        fila.rows.len() == 1 && fila.rows[0].cells[6] == "Concluída"
    });
    assert_eq!(app.grid("fila").rows[0].cells[1], "Envio");
}
