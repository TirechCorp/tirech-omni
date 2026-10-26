//! O cofre entre máquinas (M44) nos dois temas: o diálogo "Levar o cofre para outra máquina" e o
//! "Abrir cofre de outra máquina" com o resumo do que entra. Snapshots e lint.

mod comum;

use std::path::PathBuf;

use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, item_ssh, montar_pura};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

fn pasta_fixa(teste: &str, itens: &[&str]) -> Pasta {
    // As datas da lista e do resumo entram na imagem: o relógio precisa ser o mesmo sempre.
    tirech_omni::recentes::fixar_relogio(Some(1_760_000_000));
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-cofre-portatil-{teste}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    std::fs::create_dir_all(&pasta.0).unwrap();
    pasta.semear_cofre(
        itens
            .iter()
            .map(|t| item_ssh(t, "root", "segredo"))
            .collect(),
    );
    pasta
}

fn na_pagina_do_cofre(pasta: &Pasta, tema: Theme) -> TestApp {
    let mut m = montar_pura(pasta, tema);
    m.app.click("navegacao-cofre");
    m.app
}

#[test]
fn levar_o_cofre_para_outra_maquina() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("levar-{nome}"), &["srv-a", "srv-b"]);
        let mut app = na_pagina_do_cofre(&pasta, tema);
        app.click("cofre-mais").click("cofre-levar");
        app.assert_lint();
        app.assert_snapshot(&format!("cofre-levar-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn abrir_o_cofre_de_outra_maquina() {
    for (nome, tema) in TEMAS {
        let de_la = pasta_fixa(&format!("abrir-de-la-{nome}"), &["srv-a", "srv-b", "srv-c"]);
        let daqui = pasta_fixa(&format!("abrir-daqui-{nome}"), &["srv-a"]);
        let arquivo = de_la.caminho().join("cofre.tirech-vault");
        {
            let mut app = na_pagina_do_cofre(&de_la, tema);
            app.click("cofre-mais").click("cofre-levar");
            app.file_dialog_answer(Some(arquivo.clone()));
            app.click("cofre-levar-dialogo-primary");
        }
        let mut app = na_pagina_do_cofre(&daqui, tema);
        app.click("cofre-mais").click("cofre-abrir-outro");
        app.file_dialog_answer(Some(arquivo.clone()));
        assert!(app.is_open("cofre-abrir-dialogo"));
        app.assert_lint();
        app.assert_snapshot(&format!("cofre-abrir-{nome}"));
        let _ = std::fs::remove_dir_all(de_la.caminho());
        let _ = std::fs::remove_dir_all(daqui.caminho());
    }
}
