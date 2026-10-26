//! O painel de portas do servidor (M58): a lista, o filtro, "Mostrar todas", o erro e o
//! "Atualizar", com uma fonte falsa no lugar do servidor.

mod comum;
#[path = "comum/portas.rs"]
mod fixtura;

use std::io;

use tirech_omni::portas::{Descoberta, Fonte};
use tomni_ui::theme::Theme;

use fixtura::{abrir, abrir_configurando, aparece, descoberta, escrever, esperar, porta};

#[test]
fn a_lista_mostra_o_nome_a_porta_e_o_dono() {
    let (_pasta, mut m, _fonte) = abrir("portas-lista", vec![Ok(descoberta())], Theme::Light);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));

    assert_eq!(m.app.text("portas-titulo"), "Portas de web");
    assert!(aparece(&m.app, "portas-srv-8080"));
    assert!(aparece(&m.app, "portas-srv-8081"));
    assert!(aparece(&m.app, "portas-no-servidor"));
    assert!(aparece(&m.app, "portas-ctr-loja_db"));
    // As do sistema ficam escondidas.
    assert!(!aparece(&m.app, "portas-srv-22"));
    assert!(!aparece(&m.app, "portas-srv-53"));
    assert!(m.app.text("portas-status").contains("ss"));
    assert!(!aparece(&m.app, "portas-erro"));
    assert!(!aparece(&m.app, "portas-carregando"));
    assert!(!aparece(&m.app, "portas-lista-vazia"));
}

#[test]
fn mostrar_todas_traz_as_portas_do_sistema() {
    let (_pasta, mut m, _fonte) = abrir("portas-todas", vec![Ok(descoberta())], Theme::Light);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    m.app.click("portas-todas");
    assert!(aparece(&m.app, "portas-srv-22"));
    assert!(aparece(&m.app, "portas-srv-53"));
    m.app.click("portas-todas");
    assert!(!aparece(&m.app, "portas-srv-22"));
}

#[test]
fn o_filtro_casa_porta_nome_e_processo_sem_acento() {
    let (_pasta, mut m, _fonte) = abrir("portas-filtro", vec![Ok(descoberta())], Theme::Light);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));

    escrever(&mut m.app, "portas-filtro", "PHPMYADMIN");
    assert!(aparece(&m.app, "portas-srv-8081"));
    assert!(!aparece(&m.app, "portas-srv-3306"));
    assert!(!aparece(&m.app, "portas-srv-8080"));

    escrever(&mut m.app, "portas-filtro", "mariadbd");
    assert!(aparece(&m.app, "portas-srv-3306"));
    assert!(!aparece(&m.app, "portas-srv-8081"));

    escrever(&mut m.app, "portas-filtro", "painel da loja");
    assert!(aparece(&m.app, "portas-srv-8080"));
    assert!(!aparece(&m.app, "portas-srv-3306"));
}

#[test]
fn o_erro_aparece_e_o_atualizar_traz_a_lista() {
    let (_pasta, mut m, fonte) = abrir(
        "portas-erro",
        vec![Err(io::Error::other("a sessão caiu")), Ok(descoberta())],
        Theme::Light,
    );
    esperar(&mut m.app, "o erro", |a| aparece(a, "portas-erro"));
    assert!(!aparece(&m.app, "portas-srv-3306"));
    assert!(!aparece(&m.app, "portas-carregando"));

    m.app.click("portas-atualizar");
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    assert!(!aparece(&m.app, "portas-erro"));
    assert_eq!(*fonte.leituras.lock().expect("contador"), 2);
}

#[test]
fn a_lista_nao_some_quando_a_leitura_seguinte_falha() {
    let (_pasta, mut m, _fonte) = abrir(
        "portas-erro-depois",
        vec![Ok(descoberta()), Err(io::Error::other("sem rede"))],
        Theme::Light,
    );
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    m.app.click("portas-atualizar");
    esperar(&mut m.app, "o erro", |a| aparece(a, "portas-erro"));
    assert!(aparece(&m.app, "portas-srv-3306"));
}

#[test]
fn sem_porta_nenhuma_o_painel_diz_que_esta_vazio() {
    let vazia = Descoberta::vazia(Fonte::Proc, Vec::new());
    let (_pasta, mut m, _fonte) = abrir("portas-vazio", vec![Ok(vazia)], Theme::Light);
    esperar(&mut m.app, "o aviso", |a| aparece(a, "portas-lista-vazia"));
    assert!(!aparece(&m.app, "portas-no-servidor"));
}

/// Abre o menu "…" da linha `id` e espera os itens.
fn abrir_menu(app: &mut tomni_ui_test::TestApp, id: &str) {
    app.click(&format!("{id}-mais"));
    assert!(aparece(app, &format!("{id}-encaminhar")));
}

#[test]
fn o_menu_de_uma_porta_web_oferece_abrir_os_apps_e_copiar_a_url() {
    let (_pasta, mut m, _fonte) = abrir("portas-menu-web", vec![Ok(descoberta())], Theme::Light);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-8081"));
    assert_eq!(m.app.label("portas-srv-8081-principal"), "Abrir");

    abrir_menu(&mut m.app, "portas-srv-8081");
    assert_eq!(m.app.label("portas-srv-8081-abrir"), "Abrir no navegador");
    assert_eq!(m.app.label("portas-srv-8081-app-0"), "Abrir phpMyAdmin");
    assert!(aparece(&m.app, "portas-srv-8081-copiar-url"));
    assert!(aparece(&m.app, "portas-srv-8081-fixar"));
    // Nada está encaminhado nem fixado ainda.
    assert!(!aparece(&m.app, "portas-srv-8081-parar"));
    assert!(!aparece(&m.app, "portas-srv-8081-desafixar"));
    assert!(!aparece(&m.app, "portas-srv-8081-copiar-esquema"));
}

#[test]
fn o_menu_de_um_banco_copia_com_o_esquema_e_nao_abre_no_navegador() {
    let (_pasta, mut m, _fonte) = abrir("portas-menu-banco", vec![Ok(descoberta())], Theme::Light);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    assert_eq!(m.app.label("portas-srv-3306-principal"), "Encaminhar");

    abrir_menu(&mut m.app, "portas-srv-3306");
    assert_eq!(
        m.app.label("portas-srv-3306-copiar-esquema"),
        "Copiar como mysql://"
    );
    assert!(!aparece(&m.app, "portas-srv-3306-abrir"));
    assert!(!aparece(&m.app, "portas-srv-3306-copiar-url"));
}

#[test]
fn encaminhar_para_a_porta_abre_o_dialogo_ja_preenchido() {
    let (_pasta, mut m, _fonte) = abrir(
        "portas-dialogo-previo",
        vec![Ok(descoberta())],
        Theme::Light,
    );
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    abrir_menu(&mut m.app, "portas-srv-3306");
    m.app.click("portas-srv-3306-encaminhar-para");

    assert!(aparece(&m.app, "portas-dialogo"));
    assert_eq!(m.app.text("portas-dialogo-host"), "127.0.0.1");
    assert_eq!(m.app.value("portas-dialogo-porta"), 3306.0);
    assert!(m.app.enabled("portas-dialogo-encaminhar"));
    assert!(!aparece(&m.app, "portas-dialogo-erro"));
    assert!(!aparece(&m.app, "portas-dialogo-mesmo"));
}

#[test]
fn o_dialogo_so_deixa_encaminhar_com_host_e_porta_validos() {
    let (_pasta, mut m, _fonte) = abrir(
        "portas-dialogo-valido",
        vec![Ok(descoberta())],
        Theme::Light,
    );
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    m.app.click("portas-destino");
    assert!(!m.app.enabled("portas-dialogo-encaminhar"));

    escrever(&mut m.app, "portas-dialogo-host", "db.interno");
    assert!(!m.app.enabled("portas-dialogo-encaminhar"));
    m.app.set_value("portas-dialogo-porta", 5432.0);
    assert!(m.app.enabled("portas-dialogo-encaminhar"));
    m.app.set_value("portas-dialogo-porta", 0.0);
    assert!(!m.app.enabled("portas-dialogo-encaminhar"));

    m.app.click("portas-dialogo-close");
    assert!(!aparece(&m.app, "portas-dialogo"));
}

#[test]
fn o_destino_fechado_no_servidor_mostra_o_erro_e_oferece_encaminhar_mesmo_assim() {
    let (_pasta, mut m, _fonte) = abrir(
        "portas-dialogo-fechado",
        vec![Ok(descoberta())],
        Theme::Light,
    );
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
    assert!(aparece(&m.app, "portas-dialogo-mesmo"));
    assert!(m.app.enabled("portas-dialogo-encaminhar"));
}

#[test]
fn as_portas_dos_conteineres_tem_botao_e_menu() {
    let (_pasta, mut m, _fonte) = abrir("portas-conteineres", vec![Ok(descoberta())], Theme::Light);
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-ctr-loja_db"));
    m.app.click("portas-ctr-loja_db");
    assert!(aparece(&m.app, "portas-ctr-loja_db-5432-principal"));
    assert!(aparece(&m.app, "portas-ctr-loja_db-80-principal"));
    // A publicada sai pelo loopback do servidor e é web; a interna vai pelo IP do contêiner.
    assert_eq!(m.app.label("portas-ctr-loja_db-80-principal"), "Abrir");
    assert_eq!(
        m.app.label("portas-ctr-loja_db-5432-principal"),
        "Encaminhar"
    );

    m.app.click("portas-ctr-loja_db-5432-mais");
    // O menu abre como sobreposição fora da área da lista, onde `visible` já não o enxerga:
    // confere pelo retângulo, que só é desenhado com o menu aberto.
    assert!(m.app.rect("portas-ctr-loja_db-5432-menu").height() > 0.0);
    assert_eq!(
        m.app.label("portas-ctr-loja_db-5432-copiar-esquema"),
        "Copiar como postgresql://"
    );
}

/// A descoberta de exemplo com a porta 3000 (um `node`) a mais.
fn com_porta_nova() -> Descoberta {
    let mut lista = descoberta();
    let mut node = porta(3000, tirech_omni::portas::Servico::Desconhecido);
    node.processo = Some(tirech_omni::portas::Processo {
        pid: Some(77),
        nome: "node".into(),
    });
    lista.portas.push(node);
    lista
}

#[test]
fn uma_porta_nova_avisa_so_depois_da_primeira_leitura() {
    let (_pasta, mut m, _fonte) = abrir_configurando(
        "portas-nova-tela",
        vec![Ok(descoberta()), Ok(com_porta_nova())],
        Theme::Light,
        |pasta| {
            pasta.configurar("portas", "avisar_novas", true.into());
            pasta.configurar("portas", "intervalo_secs", 1.into());
        },
    );
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    assert!(!aparece(&m.app, "portas-nova"));

    esperar(&mut m.app, "o aviso", |a| aparece(a, "portas-nova"));
    assert_eq!(m.app.text("portas-nova"), "Porta nova em web: 3000 (node)");
}

#[test]
fn sem_a_opcao_ligada_nao_ha_aviso() {
    let (_pasta, mut m, fonte) = abrir(
        "portas-nova-desligada",
        vec![Ok(descoberta()), Ok(com_porta_nova())],
        Theme::Light,
    );
    esperar(&mut m.app, "a lista", |a| aparece(a, "portas-srv-3306"));
    m.app.click("portas-atualizar");
    esperar(&mut m.app, "a segunda leitura", |a| {
        aparece(a, "portas-srv-3000")
    });
    assert_eq!(*fonte.leituras.lock().expect("contador"), 2);
    assert!(!aparece(&m.app, "portas-nova"));
}
