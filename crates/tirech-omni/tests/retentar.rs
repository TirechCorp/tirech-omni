//! A rodada nova de reconexão por conexão e por pasta: acabadas as tentativas de reconectar
//! sozinho, a aba espera o tempo da linha "Tentar de novo após" (da conexão, da pasta ou das
//! Configurações) e começa outra rodada; desligado, para. A espera de minutos não cabe num
//! teste: aqui se confere o aviso na aba e que nada reabre antes da hora (o alarme é o mesmo das
//! tentativas, que `tests/mcp_aba.rs` vê reabrir).

mod comum;

use std::time::{Duration, Instant};

use serde_json::json;

use tirech_omni::modelo::{Connection, Kind, Node, Retentar, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, montar, valor};

const LINHA: &str = "Tentar de novo após";

/// A pasta `Clientes` (id 1) com `na_pasta` e, dentro dela, a conexão SSH `web` (id 2) com
/// `na_conexao`, aberta numa aba. Nas Configurações, reconectar sozinho uma vez, com 1 s de
/// espera e `retentar_mins` de pausa.
fn cenario(
    pasta: &Pasta,
    na_pasta: Retentar,
    na_conexao: Retentar,
    retentar_mins: u32,
) -> (Montagem, Session) {
    pasta.configurar(
        "conexoes",
        "reconectar",
        json!({
            "modo": "sozinho",
            "tentativas": 1,
            "espera_secs": 1,
            "retentar_mins": retentar_mins
        }),
    );
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Clientes");
    if let Some(Node::Group(g)) = tree.find_mut(grupo) {
        g.retentar = na_pasta;
    }
    let mut c = Connection::new("web", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    c.retentar = na_conexao;
    tree.add_connection(Some(grupo), c);
    pasta.semear(&tree);
    let mut m = montar(pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    esperar(&mut m.app, "a aba abrir", |_| {
        Session::find("2-1").is_some()
    });
    let sessao = Session::find("2-1").expect("a aba");
    (m, sessao)
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// O texto da aba numa linha só (o aviso pode quebrar na largura do terminal).
fn texto(sessao: Session) -> String {
    sessao.text().replace('\n', "")
}

/// Derruba a aba, espera a única tentativa reabrir e derruba de novo: as tentativas acabam.
fn esgotar_as_tentativas(m: &mut Montagem, sessao: Session) {
    let handle = m.opener.handle_de(sessao).expect("a conexão da aba");
    m.opener.derrubar_sessao(sessao, "conexão perdida");
    esperar(&mut m.app, "a tentativa reabrir", |_| {
        handle.reopened() == 1
    });
    m.app.wait(Duration::from_millis(100));
    m.opener.derrubar_sessao(sessao, "conexão perdida");
    m.app.wait(Duration::from_millis(100));
}

#[test]
fn acabadas_as_tentativas_avisa_a_rodada_nova_e_nao_reabre_antes_da_hora() {
    let pasta = Pasta::nova("retentar-configuracoes");
    let (mut m, sessao) = cenario(&pasta, Retentar::Herdar, Retentar::Herdar, 5);
    esgotar_as_tentativas(&mut m, sessao);
    esperar(&mut m.app, "o aviso da rodada nova", |_| {
        texto(sessao).contains("Tento de novo em 5 min")
    });
    m.app.wait(Duration::from_millis(1500));
    assert_eq!(m.opener.handle_de(sessao).unwrap().reopened(), 1);
    assert!(sessao.closed());
}

#[test]
fn a_pasta_escolhe_o_tempo_e_a_conexao_que_herda_segue() {
    let pasta = Pasta::nova("retentar-pasta");
    let (mut m, sessao) = cenario(&pasta, Retentar::Minutos(30), Retentar::Herdar, 5);
    esgotar_as_tentativas(&mut m, sessao);
    esperar(&mut m.app, "o aviso com o tempo da pasta", |_| {
        texto(sessao).contains("Tento de novo em 30 min")
    });
}

#[test]
fn desligado_na_conexao_para_depois_das_tentativas() {
    let pasta = Pasta::nova("retentar-desligado");
    let (mut m, sessao) = cenario(&pasta, Retentar::Minutos(30), Retentar::Desligado, 5);
    esgotar_as_tentativas(&mut m, sessao);
    esperar(&mut m.app, "o aviso de que não reconectou", |_| {
        texto(sessao).contains("Não consegui reconectar: conexão perdida")
    });
    assert!(
        !texto(sessao).contains("Tento de novo"),
        "{}",
        texto(sessao)
    );
}

#[test]
fn zero_nas_configuracoes_desliga_a_rodada_nova() {
    let pasta = Pasta::nova("retentar-zero");
    let (mut m, sessao) = cenario(&pasta, Retentar::Herdar, Retentar::Herdar, 0);
    esgotar_as_tentativas(&mut m, sessao);
    esperar(&mut m.app, "o aviso de que não reconectou", |_| {
        texto(sessao).contains("Não consegui reconectar")
    });
    assert!(
        !texto(sessao).contains("Tento de novo"),
        "{}",
        texto(sessao)
    );
}

#[test]
fn a_linha_mostra_a_heranca_e_grava_na_conexao_e_na_pasta() {
    let pasta = Pasta::nova("retentar-linha");
    let (mut m, _) = cenario(&pasta, Retentar::Minutos(30), Retentar::Herdar, 5);
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, LINHA), "Herdar (Clientes)");
    escolher_propriedade(&mut m.app, LINHA);
    m.app.select(&format!("props-{LINHA}-valor"), "Desligado");
    m.app.frame();
    let conexao = || pasta.conexoes().connection(2).expect("a conexão").retentar;
    assert_eq!(conexao(), Retentar::Desligado);
    m.app.select(&format!("props-{LINHA}-valor"), "1 h");
    m.app.frame();
    assert_eq!(conexao(), Retentar::Minutos(60));

    // A pasta passa a herdar das Configurações.
    m.app.click("arvore-1");
    let na_grade_da_pasta = m
        .app
        .properties("props-grupo")
        .into_iter()
        .find(|p| p.name == LINHA)
        .map(|p| p.value);
    assert_eq!(na_grade_da_pasta.as_deref(), Some("30 min"));
    m.app.click("props-grupo-Rede");
    comum::escolher_propriedade_em(&mut m.app, "props-grupo", LINHA);
    m.app.select(
        &format!("props-grupo-{LINHA}-valor"),
        "Herdar (Configurações)",
    );
    m.app.frame();
    let grupo = pasta.conexoes().find(1).cloned();
    assert!(
        matches!(grupo, Some(Node::Group(ref g)) if g.retentar == Retentar::Herdar),
        "{grupo:?}"
    );
    m.app.click("arvore-2");
    assert_eq!(valor(&m.app, LINHA), "1 h");
    m.app.assert_lint();
}
