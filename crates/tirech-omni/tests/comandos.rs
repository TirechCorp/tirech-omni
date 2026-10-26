//! Os comandos ao conectar (M50) na tela, com sessões `Loopback` (eco local) no lugar das
//! conexões de verdade.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::modelo::{
    ComandosAoConectar, Connection, Espera, Kind, ModoComandos, Node, OrigemDaSenha, Passo, Tree,
};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

fn comandos(passos: Vec<Passo>) -> ComandosAoConectar {
    ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos,
        reconectar: true,
    }
}

fn passo(texto: &str, espera: Espera) -> Passo {
    let mut p = Passo::comando(texto, 5);
    p.espera = espera;
    p
}

/// Uma conexão local (id 1) com os `comandos`.
fn pasta_com(nome: &str, comandos: ComandosAoConectar) -> Pasta {
    let pasta = Pasta::nova(nome);
    let mut tree = Tree::default();
    let mut c = Connection::new("Shell", Kind::Local);
    c.comandos = comandos;
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.configurar("conexoes", "comandos_quieto_ms", serde_json::json!(50));
    pasta
}

fn tela(pasta: &Pasta) -> Montagem {
    montar(pasta, Theme::Light)
}

/// A chave da aba escolhida no painel 0.
fn aba(m: &Montagem) -> String {
    m.app.panes("paineis").panes[0].tabs[0].0.clone()
}

fn menu_da_aba(m: &mut Montagem) {
    let chave = aba(m);
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
}

fn escrito(m: &Montagem) -> String {
    let chave = aba(m);
    let sessao = tomni_ui::terminal::Session::find(&chave).expect("a sessão da aba");
    let handle = m.opener.handle_de(sessao).expect("a ponta da sessão");
    String::from_utf8_lossy(&handle.take_written()).into_owned()
}

#[test]
fn roda_ao_abrir_com_o_loopback() {
    let pasta = pasta_com(
        "cmd-abrir",
        comandos(vec![
            passo("cd /srv", Espera::Logo),
            passo("ls", Espera::Logo),
        ]),
    );
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "os dois comandos na tela", |a| {
        let t = a.session_text(&a.panes("paineis").panes[0].tabs[0].0);
        t.contains("cd /srv") && t.contains("ls")
    });
    assert!(!m.app.session_text(&aba(&m)).contains("senha"));
}

#[test]
fn sem_comandos_nada_e_enviado() {
    let pasta = pasta_com("cmd-vazio", ComandosAoConectar::default());
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    m.app.wait(Duration::from_millis(200));
    assert_eq!(escrito(&m), "");
}

#[test]
fn a_dica_da_aba_mostra_o_passo_enquanto_espera() {
    let pasta = pasta_com(
        "cmd-dica",
        comandos(vec![
            passo("um", Espera::Logo),
            passo(
                "dois",
                Espera::Texto {
                    texto: "nunca".into(),
                },
            ),
            passo("tres", Espera::Logo),
        ]),
    );
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o primeiro comando", |a| {
        a.session_text(&a.panes("paineis").panes[0].tabs[0].0)
            .contains("um")
    });
    let chave = aba(&m);
    let sessao = tomni_ui::terminal::Session::find(&chave).unwrap();
    assert_eq!(sessao.title(), "Shell");
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    assert!(m.app.visible("comandos-cancelar"));
    assert!(m.app.visible("comandos-rodar-de-novo"));
}

#[test]
fn menu_da_aba_cancela_e_roda_de_novo() {
    let pasta = pasta_com(
        "cmd-menu",
        comandos(vec![
            passo("um", Espera::Logo),
            passo(
                "dois",
                Espera::Texto {
                    texto: "nunca".into(),
                },
            ),
            passo("tres", Espera::Logo),
        ]),
    );
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o primeiro comando", |a| {
        a.session_text(&a.panes("paineis").panes[0].tabs[0].0)
            .contains("um")
    });
    menu_da_aba(&mut m);
    m.app.click("comandos-cancelar");
    m.app.wait(Duration::from_millis(100));
    assert!(!m.app.session_text(&aba(&m)).contains("dois"));
    menu_da_aba(&mut m);
    assert!(!m.app.visible("comandos-cancelar"));
    m.app.click("comandos-rodar-de-novo");
    esperar(&mut m.app, "o segundo comando", |a| {
        a.session_text(&a.panes("paineis").panes[0].tabs[0].0)
            .matches("um")
            .count()
            >= 2
    });
}

#[test]
fn sem_passos_o_menu_nao_oferece_rodar_de_novo() {
    let pasta = pasta_com("cmd-menu-vazio", ComandosAoConectar::default());
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    menu_da_aba(&mut m);
    assert!(!m.app.visible("comandos-rodar-de-novo"));
    assert!(!m.app.visible("comandos-cancelar"));
}

#[test]
fn passo_que_nao_acha_o_texto_avisa_e_para() {
    let mut p = passo(
        "nunca-roda",
        Espera::Texto {
            texto: "xyz".into(),
        },
    );
    p.max_secs = 1;
    let mut primeiro = passo("primeiro", Espera::Logo);
    primeiro.max_secs = 1;
    let mut c = comandos(vec![primeiro, p, passo("depois", Espera::Logo)]);
    c.reconectar = false;
    let pasta = pasta_com("cmd-tempo", c);
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o aviso de tempo esgotado", |a| {
        a.visible("aviso-atencao")
    });
    let texto = m.app.text("aviso-atencao");
    assert!(texto.contains("passo 2"), "{texto}");
    assert!(texto.contains("xyz"), "{texto}");
    assert!(!m.app.session_text(&aba(&m)).contains("depois"));
}

/// Abre a conexão, deixa o comando rodar, derruba a sessão e a reabre; devolve quantas vezes
/// "marca" foi mandado depois da reabertura.
fn marcas_depois_de_reconectar(nome: &str, reconectar: bool) -> usize {
    let mut c = comandos(vec![passo("marca", Espera::Logo)]);
    c.reconectar = reconectar;
    let pasta = pasta_com(nome, c);
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o comando da abertura", |a| {
        a.session_text(&a.panes("paineis").panes[0].tabs[0].0)
            .contains("marca")
    });
    let chave = aba(&m);
    let sessao = tomni_ui::terminal::Session::find(&chave).unwrap();
    let _ = escrito(&m);
    m.opener.derrubar_sessao(sessao, "caiu");
    m.app.wait(Duration::from_millis(50));
    assert!(sessao.restart());
    m.app.wait(Duration::from_millis(300));
    // O que o app mandou à sessão (o eco do Loopback não volta à tela depois do restart).
    escrito(&m).matches("marca\r").count()
}

#[test]
fn reconectar_roda_de_novo() {
    assert_eq!(marcas_depois_de_reconectar("cmd-reconectar-sim", true), 1);
}

#[test]
fn reconectar_desligado_nao_roda_de_novo() {
    assert_eq!(marcas_depois_de_reconectar("cmd-reconectar-nao", false), 0);
}

#[test]
fn a_pasta_passa_os_passos_as_conexoes_e_nenhum_os_desliga() {
    let pasta = Pasta::nova("cmd-pasta");
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Pasta");
    if let Node::Group(g) = &mut tree.nodes[0] {
        g.comandos = comandos(vec![passo("da-pasta", Espera::Logo)]);
    }
    let herda = tree.add_connection(Some(grupo), Connection::new("Herda", Kind::Local));
    let mut nenhum = Connection::new("Nenhum", Kind::Local);
    nenhum.comandos = ComandosAoConectar {
        modo: ModoComandos::Nenhum,
        ..ComandosAoConectar::default()
    };
    let nenhum = tree.add_connection(Some(grupo), nenhum);
    pasta.semear(&tree);
    pasta.configurar("conexoes", "comandos_quieto_ms", serde_json::json!(50));
    let mut m = tela(&pasta);
    m.app.expand("arvore", &grupo.to_string());
    m.app.double_click(&format!("arvore-{herda}"));
    esperar(&mut m.app, "o comando da pasta", |a| {
        a.session_text(&a.panes("paineis").panes[0].tabs[0].0)
            .contains("da-pasta")
    });
    m.app.double_click(&format!("arvore-{nenhum}"));
    m.app.wait(Duration::from_millis(300));
    let abas = m.app.panes("paineis").panes[0].tabs.clone();
    let segunda = &abas[1].0;
    assert!(!m.app.session_text(segunda).contains("da-pasta"));
}

#[test]
fn passo_de_senha_sem_credencial_avisa_e_para() {
    let mut c = comandos(vec![Passo::senha(OrigemDaSenha::Conexao, 5)]);
    c.reconectar = false;
    let pasta = pasta_com("cmd-sem-senha", c);
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    // O servidor pede a senha; a credencial não existe.
    let sessao = tomni_ui::terminal::Session::find(&aba(&m)).unwrap();
    m.opener
        .handle_de(sessao)
        .expect("a ponta da sessão")
        .feed("[sudo] password for ana: ");
    esperar(&mut m.app, "o aviso de senha", |a| {
        a.visible("aviso-atencao")
    });
    let texto = m.app.text("aviso-atencao");
    assert!(texto.contains("não há senha"), "{texto}");
}

#[test]
fn duplicar_a_aba_roda_os_comandos_na_copia() {
    let pasta = pasta_com("cmd-duplicar", comandos(vec![passo("marca", Espera::Logo)]));
    let mut m = tela(&pasta);
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o comando da abertura", |a| {
        a.session_text(&a.panes("paineis").panes[0].tabs[0].0)
            .contains("marca")
    });
    m.app.click("menus-ferramentas").click("duplicar");
    esperar(&mut m.app, "a cópia", |a| {
        a.panes("paineis").panes[0].tabs.len() == 2
    });
    let copia = m.app.panes("paineis").panes[0].tabs[1].0.clone();
    assert!(
        copia.starts_with("1-"),
        "a cópia herda o id da conexão: {copia}"
    );
    esperar(&mut m.app, "o comando na cópia", |a| {
        a.session_text(&copia).contains("marca")
    });
}
