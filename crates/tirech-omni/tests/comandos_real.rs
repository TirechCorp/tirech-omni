//! Os comandos ao conectar (M50) contra um processo de verdade: um `sh` que imita um shell com
//! um `sudo` que pede a senha (sem eco), numa aba do app. Confere o motor com a saída real do
//! pseudoterminal: a tela parar, o pedido de senha e o passo que pula.

#![cfg(unix)]

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
use tirech_omni::modelo::{
    ComandosAoConectar, Connection, Espera, Kind, ModoComandos, OrigemDaSenha, Passo, Tree,
};
use tomni_ui::terminal::{Process, Session};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, item_ssh, montar_com};

/// O "shell": um prompt `$ ` que executa o que chega. O `sudo` pede a senha sem eco e responde
/// `sudo-ok:{senha}`; o `sudo-livre` não pede nada (como um sudo sem senha).
const SHELL: &str = r#"
sudo() { printf '[sudo] password for ana: '; stty -echo; read -r s; stty echo; printf '\n'; echo "sudo-ok:$s"; }
sudo_livre() { echo "sudo-livre-ok"; }
while printf '$ '; read -r linha; do eval "$linha"; done
"#;

struct AbreScript;

impl Opener for AbreScript {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        Session::new(Process::builder("sh").arg("-c").arg(SHELL).spawn())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }
}

fn esperar_tela(app: &mut TestApp, chave: &str, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(15);
    while !app.session_text(chave).contains(o_que) {
        assert!(
            Instant::now() < limite,
            "não apareceu {o_que:?} em 15 s:\n{}",
            app.session_text(chave)
        );
        app.wait(Duration::from_millis(20));
    }
}

fn montar_com_passos(nome: &str, passos: impl FnOnce(u64) -> Vec<Passo>) -> (TestApp, Pasta) {
    let pasta = Pasta::nova(nome);
    let ids = pasta.semear_cofre(vec![item_ssh("Root", "root", "segredo123")]);
    let mut tree = Tree::default();
    let mut c = Connection::new("Servidor", Kind::Local);
    c.comandos = ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos: passos(ids[0]),
        reconectar: true,
    };
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.configurar("conexoes", "comandos_quieto_ms", serde_json::json!(150));
    let (mut app, _fila) = montar_com(&pasta, Theme::Light, AbreScript);
    app.double_click("arvore-1");
    (app, pasta)
}

#[test]
fn echo_ao_conectar_aparece_na_tela() {
    let (mut app, _pasta) = montar_com_passos("real-echo", |_| {
        vec![Passo::comando("echo ola-$((1+2))", 10)]
    });
    esperar_tela(&mut app, "1-1", "ola-3");
}

#[test]
fn sudo_com_a_senha_do_cofre() {
    let (mut app, _pasta) = montar_com_passos("real-sudo", |cofre| {
        vec![
            Passo::comando("echo antes", 10),
            Passo::comando("sudo", 10),
            Passo::senha(OrigemDaSenha::Cofre { id: cofre }, 10),
            Passo::comando("echo depois", 10),
        ]
    });
    esperar_tela(&mut app, "1-1", "sudo-ok:segredo123");
    // O comando seguinte só sai depois da senha.
    esperar_tela(&mut app, "1-1", "depois");
    let tela = app.session_text("1-1");
    assert_eq!(
        tela.matches("segredo123").count(),
        1,
        "a senha não aparece digitada:\n{tela}"
    );
}

#[test]
fn sudo_sem_pedido_de_senha_pula_o_passo() {
    let (mut app, _pasta) = montar_com_passos("real-pular", |cofre| {
        vec![
            Passo::comando("sudo_livre", 10),
            Passo::senha(OrigemDaSenha::Cofre { id: cofre }, 10),
            Passo::comando("echo seguiu", 10),
        ]
    });
    esperar_tela(&mut app, "1-1", "sudo-livre-ok");
    esperar_tela(&mut app, "1-1", "seguiu");
    assert!(!app.session_text("1-1").contains("segredo123"));
    assert!(!app.visible("aviso-atencao"));
}

#[test]
fn texto_esperado_segura_o_passo_ate_aparecer() {
    let (mut app, _pasta) = montar_com_passos("real-texto", |_| {
        let mut espera = Passo::comando("echo pronto-mesmo", 10);
        espera.espera = Espera::Texto {
            texto: "nao-vai-aparecer|$ ".into(),
        };
        vec![espera, Passo::comando("echo fim", 10)]
    });
    esperar_tela(&mut app, "1-1", "fim");
}
