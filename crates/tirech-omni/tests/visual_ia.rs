//! A aparência dos assistentes de IA no servidor (M46) nos dois temas: o submenu, o diálogo de
//! instalar (com e sem o que falta no servidor), o de outra pasta e os avisos do fim da
//! instalação. Snapshots e lint.

mod comum;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::terminal::ssh::ExecOutput;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_com_ambiente};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

/// Uma conexão SSH com chave, na raiz.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    let mut c = Connection::new("web-01", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, c);
    tree
}

fn tela(teste: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta(PathBuf::from(format!("/tmp/tirech-omni-visual-ia-{teste}")));
    let _ = std::fs::remove_dir_all(&pasta.0);
    pasta.semear(&arvore());
    let (m, _) = montar_com_ambiente(&pasta, tema, |_| {});
    (pasta, m)
}

/// A saída da sondagem de um Linux com as `linhas` a mais.
fn sondagem(linhas: &[&str]) -> String {
    let mut texto = String::from(
        "tirech-ia-inicio\nuname=Linux x86_64\nhome=/home/ana\nshell=/bin/bash\n\
         path=/usr/bin:/bin\ntem_tar=1\ntem_bash=1\n",
    );
    for linha in linhas {
        texto.push_str(linha);
        texto.push('\n');
    }
    texto.push_str("tirech-ia-fim\n");
    texto
}

fn responder(m: &Montagem, saida: &str) {
    m.opener.respostas_exec.lock().unwrap().insert(
        1,
        Ok(ExecOutput {
            status: Some(0),
            stdout: saida.as_bytes().to_vec(),
            stderr: Vec::new(),
        }),
    );
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&mut TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Se o widget `id` existe e está visível.
fn aparece(app: &mut TestApp, id: &str) -> bool {
    app.ui().find(id).is_some() && app.visible(id)
}

/// Abre `web-01` e espera a sondagem terminar.
fn abrir_e_sondar(m: &mut Montagem) {
    m.app.double_click("arvore-1");
    let opener = m.opener.clone();
    esperar(&mut m.app, "a sondagem", |_| {
        opener
            .execucoes
            .lock()
            .unwrap()
            .iter()
            .any(|(_, comando, _)| comando == "sh -s")
    });
    m.app.wait(Duration::from_millis(150));
}

fn conferir(m: &mut Montagem, nome: &str, tema: &str) {
    // Deixa as animações (a do menu, a do diálogo, a do aviso) terminarem.
    m.app.wait(Duration::from_millis(400));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("{nome}-{tema}"));
}

/// Termina a aba de instalação aberta por último.
fn terminar_instalacao(m: &mut Montagem) {
    let sessao = m.opener.sessoes.borrow().last().map(|(s, _)| *s).unwrap();
    m.opener.handle_de(sessao).unwrap().close(None);
}

#[test]
fn o_submenu_com_o_claude_instalado() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("menu", tema);
        responder(
            &m,
            &sondagem(&[
                "tem_curl=1",
                "claude=/home/ana/.local/bin/claude",
                "claude_versao=2.1.0",
            ]),
        );
        abrir_e_sondar(&mut m);
        m.app.click("menus-ferramentas").click("ia");
        conferir(&mut m, "ia-menu", nome);
    }
}

#[test]
fn o_dialogo_de_instalar() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("instalar", tema);
        responder(&m, &sondagem(&["tem_curl=1"]));
        abrir_e_sondar(&mut m);
        m.app
            .click("menus-ferramentas")
            .click("ia")
            .click("ia-claude");
        esperar(&mut m.app, "o diálogo", |a| aparece(a, "ia-instalar"));
        conferir(&mut m, "ia-instalar", nome);
    }
}

#[test]
fn o_dialogo_de_instalar_sem_o_curl() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("sem-curl", tema);
        // Sem curl nem npm: o instalador oficial não roda.
        responder(&m, &sondagem(&[]));
        abrir_e_sondar(&mut m);
        m.app
            .click("menus-ferramentas")
            .click("ia")
            .click("ia-claude");
        esperar(&mut m.app, "o diálogo", |a| aparece(a, "ia-instalar"));
        conferir(&mut m, "ia-instalar-sem-curl", nome);
    }
}

#[test]
fn o_dialogo_de_outra_pasta() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("pasta", tema);
        responder(&m, &sondagem(&["claude=/home/ana/.local/bin/claude"]));
        abrir_e_sondar(&mut m);
        m.app
            .click("menus-ferramentas")
            .click("ia")
            .click("ia-outra-pasta");
        esperar(&mut m.app, "o diálogo", |a| aparece(a, "ia-pasta"));
        m.app.type_into("ia-pasta-caminho", "/srv/app");
        conferir(&mut m, "ia-pasta", nome);
    }
}

#[test]
fn o_aviso_de_instalado() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("aviso", tema);
        responder(&m, &sondagem(&["tem_curl=1"]));
        abrir_e_sondar(&mut m);
        m.app
            .click("menus-ferramentas")
            .click("ia")
            .click("ia-claude");
        m.app.click("ia-instalar-abrir-depois");
        m.app.click("ia-instalar-primary");
        esperar(&mut m.app, "a aba de instalação", |a| {
            !a.panes("paineis").panes.is_empty()
        });
        responder(
            &m,
            &sondagem(&["claude=/home/ana/.local/bin/claude", "claude_versao=2.1.0"]),
        );
        terminar_instalacao(&mut m);
        esperar(&mut m.app, "o aviso", |a| aparece(a, "ia-aviso"));
        conferir(&mut m, "ia-aviso", nome);
    }
}

#[test]
fn o_aviso_de_instalacao_que_falhou() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("erro", tema);
        responder(&m, &sondagem(&["tem_curl=1"]));
        abrir_e_sondar(&mut m);
        m.app
            .click("menus-ferramentas")
            .click("ia")
            .click("ia-claude");
        m.app.click("ia-instalar-primary");
        esperar(&mut m.app, "a aba de instalação", |a| {
            !a.panes("paineis").panes.is_empty()
        });
        terminar_instalacao(&mut m);
        esperar(&mut m.app, "o aviso", |a| aparece(a, "ia-aviso-erro"));
        conferir(&mut m, "ia-aviso-erro", nome);
    }
}
