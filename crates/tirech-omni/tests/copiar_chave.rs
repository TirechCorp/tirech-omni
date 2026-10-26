//! Copiar a chave pública para o servidor: o diálogo, o pedido ao servidor falso, os avisos e
//! a oferta de usar a chave na conexão.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::abrir::FakeOpener;
use tomni_ui::terminal::ssh::CopyIdOutcome;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar, so_com_senha};

/// Grava um par Ed25519 de teste em `ssh/` e devolve o texto do `.pub`.
fn par_de_teste(pasta: &Pasta) -> String {
    use tomni_ui::terminal::ssh::keys::{KeyGen, KeyType};
    let chave = KeyGen::new(KeyType::Ed25519)
        .comment("ana@teste".to_owned())
        .generate()
        .expect("gerar a chave do teste");
    std::fs::create_dir_all(pasta.ssh()).unwrap();
    chave
        .write(&pasta.ssh().join("id_ed25519"), false)
        .expect("gravar a chave do teste");
    chave.public_openssh().trim().to_owned()
}

fn esperar(app: &mut TestApp, m: &FakeOpener, quantas: usize) {
    let limite = Instant::now() + Duration::from_secs(10);
    while m.instalacoes.lock().unwrap().len() < quantas {
        assert!(
            Instant::now() < limite,
            "a instalação não chegou ao servidor"
        );
        app.wait(Duration::from_millis(20));
    }
    // Um quadro a mais para o aviso e o diálogo de usar a chave.
    app.wait(Duration::from_millis(100));
}

fn abrir_copiar(m: &mut Montagem) {
    m.app.click("menus-ferramentas").click("copiar-chave-menu");
}

#[test]
fn instala_a_chave_e_avisa() {
    let pasta = Pasta::nova("copiar-instala");
    let linha = par_de_teste(&pasta);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_copiar(&mut m);
    assert!(m.app.is_open("copiar-chave-dialogo"));
    m.app.click("copiar-chave-dialogo-primary");
    esperar(&mut m.app, &m.opener, 1);

    let feitas = m.opener.instalacoes.lock().unwrap().clone();
    assert_eq!(feitas.len(), 1);
    assert_eq!(feitas[0].1, linha);
    assert!(!feitas[0].2, "sem forçar");
    assert!(m.app.visible("aviso-sucesso"));
    assert!(
        m.app
            .text("aviso-sucesso")
            .contains("Chave instalada em .ssh/authorized_keys.")
    );
    m.app.assert_lint();
}

#[test]
fn ja_estava_no_servidor() {
    let pasta = Pasta::nova("copiar-ja-estava");
    par_de_teste(&pasta);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    *m.opener.resposta_da_instalacao.borrow_mut() = Ok(CopyIdOutcome::AlreadyThere);
    abrir_copiar(&mut m);
    m.app.click("copiar-chave-forcar");
    m.app.click("copiar-chave-dialogo-primary");
    esperar(&mut m.app, &m.opener, 1);

    assert!(m.opener.instalacoes.lock().unwrap()[0].2, "forçou");
    assert!(
        m.app
            .text("aviso-sucesso")
            .contains("A chave já estava no servidor.")
    );
}

#[test]
fn erro_do_servidor_vira_aviso() {
    let pasta = Pasta::nova("copiar-erro");
    par_de_teste(&pasta);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    *m.opener.resposta_da_instalacao.borrow_mut() = Err("acesso negado".to_owned());
    abrir_copiar(&mut m);
    m.app.click("copiar-chave-dialogo-primary");
    esperar(&mut m.app, &m.opener, 1);

    assert!(m.app.visible("aviso-erro"));
    assert!(m.app.text("aviso-erro").contains("acesso negado"));
    assert!(!m.app.is_open("copiar-chave-usar"));
}

#[test]
fn oferece_usar_a_chave_na_conexao_sem_chave() {
    let pasta = Pasta::nova("copiar-usar");
    par_de_teste(&pasta);
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    abrir_copiar(&mut m);
    m.app.click("copiar-chave-dialogo-primary");
    esperar(&mut m.app, &m.opener, 1);

    assert!(m.app.is_open("copiar-chave-usar"));
    assert!(m.app.text("copiar-chave-usar-texto").contains("sem-chave"));
    m.app.assert_lint();
    m.app.click("copiar-chave-usar-primary");
    let chave = pasta.conexoes().connection(1).unwrap().key_file.clone();
    assert_eq!(chave, Some(pasta.ssh().join("id_ed25519")));
}
