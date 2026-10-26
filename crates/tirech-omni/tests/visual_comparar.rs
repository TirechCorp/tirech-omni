//! Comparar pastas, navegação sincronizada e pastas padrão nos dois temas (M41): a grade de
//! propriedades com a categoria "Arquivos" e a página Arquivos comparando. Snapshots e lint.

mod comum;

use std::time::{Duration, Instant, UNIX_EPOCH};

use tirech_omni::modelo::{Ativar, Connection, Kind, Tree};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, montar};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

#[test]
fn as_propriedades_com_a_categoria_arquivos_nos_dois_temas() {
    for (nome, tema) in TEMAS {
        let pasta = Pasta::nova(&format!("comparar-props-{nome}"));
        let mut sftp = Connection::new("servidor", Kind::Sftp);
        sftp.host = "arquivos.exemplo.com.br".into();
        sftp.user = "ana".into();
        sftp.pasta_local = "/home/ana/projetos".into();
        sftp.pasta_remota = "/var/www/html".into();
        sftp.comparar = Ativar::Sim;
        let mut tree = Tree::default();
        tree.add_connection(None, sftp);
        pasta.semear(&tree);
        let mut m = montar(&pasta, tema);
        m.app.click("arvore-1");
        escolher_propriedade(&mut m.app, "Sincronizar pastas");
        m.app.assert_lint();
        m.app
            .assert_snapshot(&format!("propriedades-arquivos-{nome}"));
    }
}

/// A conexão 1 aberta, com um conjunto de arquivos que mostra as três cores. As datas são fixas:
/// o snapshot não pode mudar com o relógio.
fn tela_comparando(nome: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    let mut sftp = Connection::new("servidor", Kind::Sftp);
    sftp.host = "arquivos.exemplo.com.br".into();
    sftp.user = "ana".into();
    sftp.key_file = Some("/chaves/ana".into());
    let mut tree = Tree::default();
    tree.add_connection(None, sftp);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    let (disco, servidor) = (m.opener.disco.clone(), m.opener.servidor(1));
    let base = UNIX_EPOCH + Duration::from_secs(1_760_000_000);
    for (nome, local, remoto, atraso) in [
        ("index.html", "<html>", "<html>", 0),
        ("estilo.css", "body {}", "body { margin: 0 }", 0),
        ("script.js", "let a;", "let a;", 7200),
    ] {
        let caminho = format!("/home/ana/{nome}");
        disco.put(&caminho, local);
        servidor.put(&caminho, remoto);
        disco.touch(&caminho, base + Duration::from_secs(atraso));
        servidor.touch(&caminho, base);
    }
    disco.put("/home/ana/rascunho.txt", "só aqui");
    disco.touch("/home/ana/rascunho.txt", base);
    servidor.put("/home/ana/backup.tar", "só lá");
    servidor.touch("/home/ana/backup.tar", base);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "os dois painéis", |a| {
        a.grid("a1-local").keys().len() > 3 && a.grid("a1-remoto").keys().len() > 3
    });
    m.app.wait(Duration::from_millis(100));
    (pasta, m)
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// A página comparando pelo tamanho: amarelo para o que só existe de um lado, vermelho para o
/// tamanho diferente.
fn comparando_pelo_tamanho(nome: &str, tema: Theme) {
    let (_pasta, mut m) = tela_comparando(&format!("comparar-snap-{nome}"), tema);
    m.app.click("a1-comparar");
    m.app.wait(Duration::from_millis(100));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("arquivos-comparar-{nome}"));
}

/// O menu das opções aberto, com o critério da data (o verde do mais novo aparece atrás).
fn menu_das_opcoes(nome: &str, tema: Theme) {
    let (_pasta, mut m) = tela_comparando(&format!("comparar-opcoes-snap-{nome}"), tema);
    m.app.click("a1-comparar");
    m.app.click("a1-comparar-opcoes");
    m.app.click("a1-comparar-data");
    m.app.click("a1-comparar-opcoes");
    m.app.wait(Duration::from_millis(100));
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("arquivos-comparar-opcoes-{nome}"));
}

#[test]
fn a_pagina_arquivos_comparando_pelo_tamanho_no_tema_claro() {
    comparando_pelo_tamanho("claro", Theme::Light);
}

#[test]
fn a_pagina_arquivos_comparando_pelo_tamanho_no_tema_escuro() {
    comparando_pelo_tamanho("escuro", Theme::Dark);
}

#[test]
fn o_menu_das_opcoes_da_comparacao_no_tema_claro() {
    menu_das_opcoes("claro", Theme::Light);
}

#[test]
fn o_menu_das_opcoes_da_comparacao_no_tema_escuro() {
    menu_das_opcoes("escuro", Theme::Dark);
}
