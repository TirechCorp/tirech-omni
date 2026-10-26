//! O editor de túneis, o formulário, o diálogo de executar e o aviso de um programa nos dois
//! temas (M37). Snapshots e lint.

mod comum;

use tirech_omni::modelo::{Connection, Kind, TipoTunel, Tree, Tunel};
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::RemoteAppStatus;
use tomni_ui::theme::Theme;

use comum::{Montagem, Pasta, montar};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

fn tunel(tipo: TipoTunel, porta: u16, host: &str, destino: u16) -> Tunel {
    let mut t = Tunel::novo(tipo, false);
    t.porta = porta;
    host.clone_into(&mut t.destino_host);
    t.destino_porta = destino;
    t
}

/// Uma conexão SSH `servidor1` com três túneis: um local, um SOCKS5 e o socket do libvirt.
fn com_tuneis() -> Tree {
    let mut tree = Tree::default();
    let mut c = Connection::new("servidor1", Kind::Ssh);
    c.host = "servidor1.exemplo".into();
    c.user = "deploy".into();
    c.key_file = Some("/chaves/web".into());
    c.tuneis
        .push(tunel(TipoTunel::Local, 8080, "db.interno", 5432));
    c.tuneis.push(tunel(TipoTunel::Dinamico, 1080, "", 0));
    let mut libvirt = tunel(TipoTunel::SocketUnix, 0, "", 0);
    libvirt.caminho_remoto = "/var/run/libvirt/libvirt-sock".into();
    c.tuneis.push(libvirt);
    tree.add_connection(None, c);
    tree
}

fn tela(nome: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(&format!("{nome}-{tema:?}"));
    pasta.semear(&com_tuneis());
    let m = montar(&pasta, tema);
    (pasta, m)
}

fn encerrar() {
    for n in 1..=3 {
        if let Some(s) = Session::find(&format!("1-{n}")) {
            s.dispose();
        }
    }
}

#[test]
fn o_editor_de_tuneis_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("tuneis-visual", tema);
        m.app.click("arvore-1");
        m.app.context_menu("arvore-1");
        m.app.click("menu-1-tuneis");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("tuneis-lista-{nome}"));
    }
}

#[test]
fn o_formulario_de_tunel_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("tuneis-form-visual", tema);
        m.app.click("arvore-1");
        m.app.context_menu("arvore-1");
        m.app.click("menu-1-tuneis");
        m.app.click("tuneis-adicionar");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("tuneis-formulario-{nome}"));
    }
}

#[test]
fn o_dialogo_de_executar_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("executar-visual", tema);
        m.app.context_menu("arvore-1");
        m.app.click("menu-1-executar");
        m.app.click("executar-comando").type_text("virt-viewer");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("tuneis-executar-{nome}"));
    }
}

#[test]
fn os_avisos_dos_programas_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("programas-visual", tema);
        m.app.context_menu("arvore-1");
        m.app.click("menu-1-virt-manager");
        m.app.context_menu("arvore-1");
        m.app.click("menu-1-executar");
        m.app.click("executar-comando").type_text("gitk");
        m.app.click("executar-dialogo-primary");
        m.opener.lancados.borrow()[1]
            .lancamento
            .definir_status(RemoteAppStatus::Exited(Some(127)));
        m.opener.lancados.borrow()[1]
            .lancamento
            .definir_stderr("bash: gitk: command not found\n");
        std::thread::sleep(std::time::Duration::from_millis(1300));
        m.app.frame();
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("programas-{nome}"));
    }
    encerrar();
}
