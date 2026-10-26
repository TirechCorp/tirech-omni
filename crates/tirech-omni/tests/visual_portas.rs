//! A aparência do painel de portas do servidor (M58) nos dois temas: snapshots e lint.

mod comum;
#[path = "comum/portas.rs"]
#[allow(dead_code)]
mod fixtura;

use std::time::Duration;

use tirech_omni::portas::{Descoberta, Fonte};
use tomni_ui::theme::Theme;

use fixtura::{abrir, abrir_configurando, aparece, descoberta, escrever, esperar, porta};

fn nome_do_tema(tema: Theme) -> &'static str {
    match tema {
        Theme::Dark => "escuro",
        _ => "claro",
    }
}

fn conferir(nome: &str, tema: Theme, descoberta: Descoberta, pronto: &str) {
    let (_pasta, mut m, _fonte) = abrir(&format!("visual-{nome}"), vec![Ok(descoberta)], tema);
    esperar(&mut m.app, "a leitura", |a| aparece(a, pronto));
    m.app.wait(Duration::from_millis(400));
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("{nome}-{}", nome_do_tema(tema)));
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn portas_com_lista_claro() {
    conferir(
        "portas-lista",
        Theme::Light,
        descoberta(),
        "portas-srv-3306",
    );
}

#[test]
fn portas_com_lista_escuro() {
    conferir("portas-lista", Theme::Dark, descoberta(), "portas-srv-3306");
}

#[test]
fn portas_vazio_claro() {
    let vazia = Descoberta::vazia(Fonte::Proc, Vec::new());
    conferir("portas-vazio", Theme::Light, vazia, "portas-lista-vazia");
}

#[test]
fn portas_vazio_escuro() {
    let vazia = Descoberta::vazia(Fonte::Proc, Vec::new());
    conferir("portas-vazio", Theme::Dark, vazia, "portas-lista-vazia");
}

/// Abre o menu "…" da porta web 8081 (com um app do catálogo) e confere.
fn menu_da_porta(tema: Theme) {
    let (_pasta, mut m, _fonte) = abrir("visual-portas-menu", vec![Ok(descoberta())], tema);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-8081"));
    m.app.click("portas-srv-8081-mais");
    assert!(aparece(&m.app, "portas-srv-8081-encaminhar"));
    m.app.wait(Duration::from_millis(400));
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("portas-menu-{}", nome_do_tema(tema)));
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn portas_menu_claro() {
    menu_da_porta(Theme::Light);
}

#[test]
fn portas_menu_escuro() {
    menu_da_porta(Theme::Dark);
}

/// O diálogo "Encaminhar destino…" depois de um destino que não responde.
fn dialogo_com_erro(tema: Theme) {
    let (_pasta, mut m, _fonte) = abrir("visual-portas-dialogo", vec![Ok(descoberta())], tema);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    m.opener
        .destinos_fechados
        .lock()
        .expect("lista")
        .push(("db.interno".to_owned(), 5432));
    m.app.click("portas-destino");
    escrever(&mut m.app, "portas-dialogo-host", "db.interno");
    m.app.set_value("portas-dialogo-porta", 5432.0);
    m.app.click("portas-dialogo-encaminhar");
    esperar(&mut m.app, "o erro", |a| aparece(a, "portas-dialogo-erro"));
    m.app.wait(Duration::from_millis(400));
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("portas-dialogo-erro-{}", nome_do_tema(tema)));
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn portas_dialogo_com_erro_claro() {
    dialogo_com_erro(Theme::Light);
}

#[test]
fn portas_dialogo_com_erro_escuro() {
    dialogo_com_erro(Theme::Dark);
}

/// O aviso "Porta nova em web: 3000 (node)" em cima dos terminais.
fn aviso_de_porta_nova(tema: Theme) {
    let mut lista = descoberta();
    let mut node = porta(3000, tirech_omni::portas::Servico::Desconhecido);
    node.processo = Some(tirech_omni::portas::Processo {
        pid: Some(77),
        nome: "node".into(),
    });
    lista.portas.push(node);
    let (_pasta, mut m, _fonte) = abrir_configurando(
        "visual-portas-nova",
        vec![Ok(descoberta()), Ok(lista)],
        tema,
        |pasta| {
            pasta.configurar("portas", "avisar_novas", true.into());
            pasta.configurar("portas", "intervalo_secs", 1.into());
        },
    );
    esperar(&mut m.app, "o aviso", |a| aparece(a, "portas-nova"));
    // O mouse ficou em cima do "Atualizar": tira dele para a dica não sair no quadro.
    m.app.hover("portas-titulo");
    m.app.wait(Duration::from_millis(400));
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("portas-nova-{}", nome_do_tema(tema)));
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn portas_aviso_de_porta_nova_claro() {
    aviso_de_porta_nova(Theme::Light);
}

#[test]
fn portas_aviso_de_porta_nova_escuro() {
    aviso_de_porta_nova(Theme::Dark);
}
