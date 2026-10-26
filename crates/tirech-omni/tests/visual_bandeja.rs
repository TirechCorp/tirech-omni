//! A bandeja do M44 nos dois temas: o diálogo ao fechar (com uma e com duas janelas), o aviso de
//! um sistema sem bandeja e Configurações › Geral sem bandeja. Snapshots e lint.

mod comum;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tomni_ui::WindowHandle;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, exemplo, montar, montar_pura};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

fn pasta_fixa(teste: &str) -> Pasta {
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-bandeja-{teste}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    pasta
}

fn esperar(app: &mut TestApp, o_que: &str, mut pronto: impl FnMut(&mut TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn o_dialogo_ao_fechar() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("dialogo-{nome}"));
        let mut m = montar_pura(&pasta, tema);
        assert!(!m.app.close_window(WindowHandle::main()));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("bandeja-dialogo-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn o_dialogo_ao_fechar_com_duas_janelas() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("duas-{nome}"));
        pasta.semear(&exemplo());
        let mut m = montar(&pasta, tema);
        m.app.click("menus-janela").click("nova-janela");
        m.app.frame();
        let principal = m.app.windows()[0];
        m.app.switch_to(principal);
        assert!(!m.app.close_window(principal));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("bandeja-duas-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn o_aviso_de_sistema_sem_bandeja() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("aviso-{nome}"));
        let mut m = montar_pura(&pasta, tema);
        m.app.set_tray_available(false);
        esperar(&mut m.app, "o aviso aparecer", |a| {
            a.visible("sem-bandeja-aviso")
        });
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("bandeja-aviso-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn configuracoes_sem_bandeja() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("config-{nome}"));
        let mut m = montar_pura(&pasta, tema);
        m.app.set_tray_available(false);
        m.app.resize(1000, 760);
        m.app.click("inicio-ferramenta-configuracoes");
        m.app
            .assert_snapshot(&format!("bandeja-configuracoes-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}
