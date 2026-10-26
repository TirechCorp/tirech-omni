//! Importar e exportar (M38) nos dois temas: o submenu "Importar ▸", a pré-visualização e o
//! diálogo da exportação. Snapshots e lint.

mod comum;

use std::path::PathBuf;
use std::time::Duration;

use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, montar};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

/// Um `sitemanager.xml` do FileZilla com pastas, senhas e um protocolo que o app não traz
/// (o aviso aparece na pré-visualização).
const FILEZILLA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FileZilla3 version="3.66.5" platform="*nix">
 <Servers>
  <Folder expanded="1">Clientes
   <Folder expanded="0">ACME
    <Server>
     <Host>ftp.acme.com</Host><Port>21</Port><Protocol>0</Protocol><Type>0</Type>
     <User>acme</User><Pass encoding="base64">c2VucmE=</Pass><Logontype>1</Logontype>
     <Name>ACME FTP</Name>
    </Server>
   </Folder>
   <Server>
    <Host>sftp.exemplo.com</Host><Port>2200</Port><Protocol>1</Protocol>
    <User>ana</User><Pass encoding="base64">c2VnLXNmdHA=</Pass><Logontype>1</Logontype>
    <Name>Beta SFTP</Name>
   </Server>
   <Server>
    <Host>dav.exemplo.com</Host><Port>443</Port><Protocol>5</Protocol><User>u</User><Name>DAV</Name>
   </Server>
  </Folder>
  <Server>
   <Host>s3.eu-west-1.amazonaws.com</Host><Protocol>7</Protocol><User>AKIAXYZ</User>
   <Pass encoding="base64">c2VnLXMz</Pass><Name>Bucket</Name>
  </Server>
 </Servers>
</FileZilla3>"#;

/// Uma pasta fixa em `/tmp` (o caminho entra no estado da tela) com o arquivo do FileZilla.
fn pasta_fixa(teste: &str) -> Pasta {
    tirech_omni::recentes::fixar_relogio(Some(1_760_000_000));
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-importar-{teste}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    std::fs::create_dir_all(&pasta.0).unwrap();
    std::fs::write(pasta.caminho().join("sitemanager.xml"), FILEZILLA).unwrap();
    pasta
}

fn tela(pasta: &Pasta, tema: Theme) -> TestApp {
    montar(pasta, tema).app
}

#[test]
fn o_submenu_importar() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("submenu-{nome}"));
        let mut app = tela(&pasta, tema);
        app.click("menus-arquivo");
        app.hover("importar-menu");
        app.wait(Duration::from_millis(450));
        assert!(app.visible("importar-filezilla"));
        app.assert_lint();
        app.assert_snapshot(&format!("importar-submenu-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn a_previa_da_importacao() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("previa-{nome}"));
        let mut app = tela(&pasta, tema);
        app.click("menus-arquivo");
        app.hover("importar-menu");
        app.wait(Duration::from_millis(450));
        app.click("importar-filezilla");
        app.file_dialog_answer(Some(pasta.caminho().join("sitemanager.xml")));
        assert!(app.is_open("importar-dialogo"));
        app.assert_lint();
        app.assert_snapshot(&format!("importar-previa-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn o_dialogo_da_exportacao() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("exportar-{nome}"));
        let mut app = tela(&pasta, tema);
        app.click("menus-arquivo");
        app.hover("exportar-menu");
        app.wait(Duration::from_millis(450));
        app.click("exportar-putty");
        assert!(app.is_open("exportar-dialogo"));
        app.assert_lint();
        app.assert_snapshot(&format!("exportar-dialogo-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}
