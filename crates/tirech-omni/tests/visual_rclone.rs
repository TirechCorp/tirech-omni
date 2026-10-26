//! O rclone nos dois temas (M33): a subpágina com dois remotos e o formulário de um provedor.
//! Snapshots e lint.

mod comum;
#[path = "comum/rcd_falso.rs"]
mod rcd_falso;

use std::time::{Duration, Instant};

use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, montar};
use rcd_falso::RcdFalso;

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn a_subpagina_dos_remotos_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("rclone-visual-lista-{nome}"));
        let falso = RcdFalso::iniciar();
        falso.configurar("drive", "drive");
        falso.configurar("fotos", "s3");
        let mut m = montar(&pasta, tema);
        *m.opener.rcd.lock().unwrap() = Some(falso.rcd());
        m.app.click("navegacao-arquivos");
        m.app.click("arquivos-remotos-rclone");
        esperar(&mut m.app, "a lista de remotos", |a| {
            !a.grid("rclone-remotos").keys().is_empty()
        });
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("rclone-remotos-{nome}"));
    }
}

#[test]
fn o_formulario_de_um_provedor_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("rclone-visual-form-{nome}"));
        let falso = RcdFalso::iniciar();
        falso.configurar("drive", "drive");
        let mut m = montar(&pasta, tema);
        *m.opener.rcd.lock().unwrap() = Some(falso.rcd());
        m.app.click("navegacao-arquivos");
        m.app.click("arquivos-remotos-rclone");
        esperar(&mut m.app, "a lista de remotos", |a| {
            !a.grid("rclone-remotos").keys().is_empty()
        });
        m.app.click("rclone-novo");
        esperar(&mut m.app, "os tipos do rclone", |a| {
            a.visible("rclone-form") && !a.visible("rclone-form-carregando")
        });
        // Escolhe o tipo como uma pessoa: clica na linha "Nome" e desce até "Tipo".
        let r = m.app.rect("rclone-form-grade-Nome");
        m.app.click_at(
            tomni_ui::kurbo::Point::new(r.x0 + 24.0, (r.y0 + r.y1) / 2.0),
            1,
        );
        m.app.key(tomni_ui::Key::Down);
        m.app
            .select("rclone-form-grade-Tipo-valor", "Provedor falso");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("rclone-formulario-{nome}"));
    }
}
