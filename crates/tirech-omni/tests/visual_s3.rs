//! O S3 nos dois temas (M32): a grade de propriedades de uma conexão S3 e a página Arquivos
//! com o bucket aberto. Snapshots e lint.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::cofre::{Item, Tipo};
use tirech_omni::modelo::{Connection, Kind, S3Estilo, S3Provedor, Tree};
use tomni_ui::theme::Theme;

use comum::{Pasta, escolher_propriedade, montar};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

/// Uma conexão S3 (id 1, `nuvem`) no bucket `fotos`.
fn arvore() -> Tree {
    let mut s3 = Connection::new("nuvem", Kind::S3);
    s3.user = "AKIAEXEMPLO0123456".into();
    s3.s3.provider = S3Provedor::Minio;
    s3.s3.endpoint = "http://127.0.0.1:1".into();
    s3.s3.url_style = S3Estilo::Caminho;
    s3.s3.bucket = "fotos".into();
    s3.credencial = Some(1);
    let mut tree = Tree::default();
    tree.add_connection(None, s3);
    tree
}

/// O item do cofre com as chaves da conexão.
fn item() -> Item {
    Item {
        tipo: Tipo::S3,
        titulo: "nuvem".into(),
        usuario: "AKIAEXEMPLO0123456".into(),
        senha: "segredo".into(),
        ..Item::default()
    }
}

#[test]
fn as_propriedades_do_s3_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("s3-props-{nome}"));
        pasta.semear(&arvore());
        let mut m = montar(&pasta, tema);
        m.app.click("arvore-1");
        escolher_propriedade(&mut m.app, "Bucket");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("s3-propriedades-{nome}"));
    }
}

#[test]
fn a_pagina_arquivos_do_s3_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("s3-arquivos-{nome}"));
        pasta.semear_cofre(vec![item()]);
        pasta.semear(&arvore());
        let mut m = montar(&pasta, tema);
        m.opener
            .disco
            .dir("/home/ana/Projetos")
            .put("/home/ana/relatorio.pdf", vec![0_u8; 182_000]);
        m.opener
            .servidor(1)
            .dir("/home/ana/2025")
            .dir("/home/ana/backups")
            .put("/home/ana/capa.png", vec![0_u8; 1_800_000])
            .put("/home/ana/leia-me.txt", "olá");
        m.app.double_click("arvore-1");
        let limite = Instant::now() + Duration::from_secs(10);
        while !m.app.visible("a1-remoto") || !m.app.grid("a1-remoto").keys().contains(&"capa.png") {
            assert!(Instant::now() < limite, "esperando a listagem do bucket");
            m.app.wait(Duration::from_millis(20));
        }
        m.app.wait(Duration::from_millis(100));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("s3-arquivos-{nome}"));
    }
}
