//! O FTP nos dois temas (M31): a grade de propriedades de uma conexão FTP e o diálogo
//! "Certificado não confiável". Snapshots e lint.

mod comum;

use std::thread;
use std::time::{Duration, SystemTime};

use tirech_omni::abrir::conferir_certificado;
use tirech_omni::modelo::{Connection, FtpModo, FtpSeguranca, Kind, Tree};
use tomni_ui::Key;
use tomni_ui::files::ftp::CertificateInfo;
use tomni_ui::theme::Theme;

use comum::{Pasta, escolher_propriedade, montar};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

#[test]
fn as_propriedades_do_ftp_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("ftp-props-{nome}"));
        let mut ftp = Connection::new("arquivos", Kind::Ftp);
        ftp.host = "ftp.exemplo.com.br".into();
        ftp.user = "ana".into();
        ftp.ftp_security = FtpSeguranca::ExplicitaSeDisponivel;
        ftp.ftp_mode = FtpModo::Passivo;
        let mut tree = Tree::default();
        tree.add_connection(None, ftp);
        pasta.semear(&tree);
        let mut m = montar(&pasta, tema);
        m.app.click("arvore-1");
        // Abre "FTP avançado": desce da última linha aberta até o cabeçalho.
        escolher_propriedade(&mut m.app, "Modo");
        m.app.key(Key::Down).key(Key::Right);
        escolher_propriedade(&mut m.app, "Versão do TLS");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("ftp-propriedades-{nome}"));
    }
}

#[test]
fn o_dialogo_do_certificado_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("ftp-certificado-{nome}"));
        let mut m = montar(&pasta, tema);
        let fila = m.fila;
        let arquivo = pasta.caminho().join("ssh").join("certificados.json");
        let info = CertificateInfo {
            host: "ftp.exemplo.com.br".into(),
            sha256: "3A:9F:0C:D4:71:BE:22:58:A6:19:C0:7D:E3:4B:90:15:F8:62:AD:3C:0E:87:B1:54:2F:D9:76:CA:13:E8:5B:A0"
                .into(),
            subject: "CN=ftp.exemplo.com.br, O=Exemplo Ltda".into(),
            issuer: "CN=ftp.exemplo.com.br, O=Exemplo Ltda".into(),
            not_before: SystemTime::UNIX_EPOCH + Duration::from_secs(1_735_689_600),
            not_after: SystemTime::UNIX_EPOCH + Duration::from_secs(1_767_225_600),
            error: "o certificado é autoassinado".into(),
        };
        thread::spawn(move || {
            conferir_certificado(fila, "arquivos", 21, Some(&arquivo), &info);
        });
        while !m.app.is_open("certificado-dialogo") {
            m.app.wait(Duration::from_millis(20));
        }
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("ftp-certificado-{nome}"));
    }
}
