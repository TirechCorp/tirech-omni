//! O estado das abas: a grade e as conexões abertas voltam na execução seguinte.

mod comum;

use tirech_omni::abrir::Senhas;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use tirech_omni::armazenamento::{self, Trava};

use comum::{Pasta, exemplo, montar, montar_com_ambiente, so_com_senha};

fn abas(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()).collect())
        .collect()
}

/// Fecha as sessões que a "execução" anterior deixou no registro desta thread.
fn encerrar(chaves: &[&str]) {
    for chave in chaves {
        if let Some(s) = Session::find(chave) {
            s.dispose();
        }
    }
}

#[test]
fn as_abas_e_a_grade_voltam_na_proxima_execucao() {
    let pasta = Pasta::nova("estado");
    pasta.semear(&exemplo());
    {
        let mut m = montar(&pasta, Theme::Light);
        m.app.click("menus-exibir").click("layout-2x2");
        m.app.expand("arvore", "1");
        m.app.double_click("arvore-2").double_click("arvore-3");
        m.app.double_click("arvore-4");
        assert_eq!(abas(&m.app).len(), 4);
    }
    encerrar(&["2-1", "3-2", "4-3"]);
    assert!(pasta.caminho().join("estado.json").exists());

    let m = montar(&pasta, Theme::Light);
    let abas = abas(&m.app);
    assert_eq!(abas.len(), 4, "a grade 2 × 2 voltou");
    let todas: Vec<&str> = abas.iter().flatten().map(String::as_str).collect();
    assert_eq!(todas.len(), 3);
    for nome in ["web-01", "db-01", "Roteador"] {
        assert!(todas.contains(&nome), "faltou a aba {nome}: {todas:?}");
    }
    assert_eq!(m.opener.abertas.borrow().len(), 3);
    assert!(
        m.opener
            .senhas
            .borrow()
            .iter()
            .all(|s| *s == Senhas::default())
    );
    encerrar(&["2-1", "3-2", "4-3"]);
}

#[test]
fn conexoes_que_sumiram_e_estado_ilegivel_sao_ignorados() {
    let pasta = Pasta::nova("estado-sumiu");
    pasta.semear(&exemplo());
    std::fs::write(
        pasta.caminho().join("estado.json"),
        r#"{"grade":[1,1],"paineis":[[2,99]]}"#,
    )
    .unwrap();
    {
        let m = montar(&pasta, Theme::Light);
        assert_eq!(abas(&m.app), [["web-01"]]);
        assert_eq!(*m.opener.abertas.borrow(), [2]);
    }
    encerrar(&["2-1"]);

    std::fs::write(pasta.caminho().join("estado.json"), "isto não é json").unwrap();
    let m = montar(&pasta, Theme::Light);
    assert!(m.app.visible("boas-vindas"));
    assert!(m.opener.abertas.borrow().is_empty());
}

#[test]
fn quem_pede_senha_volta_pelo_dialogo_no_painel_de_antes() {
    let pasta = Pasta::nova("estado-senha");
    pasta.semear(&so_com_senha());
    std::fs::write(
        pasta.caminho().join("estado.json"),
        r#"{"grade":[2,1],"paineis":[[],[1]]}"#,
    )
    .unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.frame();
    assert!(m.app.is_open("senha-dialogo"), "a senha não foi guardada");
    m.app
        .type_into("senha-campo", "x")
        .click("senha-dialogo-primary");
    assert_eq!(abas(&m.app), [vec![], vec!["sem-chave".to_owned()]]);
    encerrar(&["1-1"]);
}

#[test]
fn sem_confirmar_a_senha_o_estado_antigo_nao_e_apagado() {
    let pasta = Pasta::nova("estado-senha-cancelada");
    pasta.semear(&so_com_senha());
    let antigo = r#"{"grade":[1,1],"paineis":[[1]]}"#;
    std::fs::write(pasta.caminho().join("estado.json"), antigo).unwrap();
    let m = montar(&pasta, Theme::Light);
    drop(m);
    let texto = std::fs::read_to_string(pasta.caminho().join("estado.json")).unwrap();
    let valor = serde_json::from_str(&texto).unwrap();
    let estado = tirech_omni::estado::do_valor(&valor).unwrap();
    let ids: Vec<Option<u64>> = estado.janelas[0].paineis[0].iter().map(|i| i.id).collect();
    assert_eq!(
        ids,
        [Some(1)],
        "o estado com a conexão 1 foi apagado: {texto}"
    );
}

#[test]
fn a_segunda_copia_do_app_abre_sem_as_abas_da_primeira_e_so_grava_quando_ela_fecha() {
    let pasta = Pasta::nova("estado-segunda-copia");
    pasta.semear(&exemplo());
    let arquivo = pasta.caminho().join("estado.json");
    let da_primeira = r#"{"grade":[1,1],"paineis":[[3]]}"#;
    std::fs::write(&arquivo, da_primeira).unwrap();
    // A primeira cópia segura a trava da pasta, como o `main` faz.
    let primeira = armazenamento::travar(pasta.caminho()).unwrap();
    assert!(matches!(primeira, Trava::Travada(_)));

    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| a.dono_do_estado = false);
    assert!(
        m.opener.abertas.borrow().is_empty(),
        "a segunda cópia reabriu as abas da primeira"
    );
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    m.app.frame();
    assert_eq!(*m.opener.abertas.borrow(), [2]);
    assert_eq!(
        std::fs::read_to_string(&arquivo).unwrap(),
        da_primeira,
        "a segunda cópia gravou por cima do estado da primeira"
    );

    // A primeira fecha: na gravação seguinte, a segunda pega a trava e grava as abas dela.
    drop(primeira);
    m.app.double_click("arvore-4");
    m.app.frame();
    let texto = std::fs::read_to_string(&arquivo).unwrap();
    let valor = serde_json::from_str(&texto).unwrap();
    let estado = tirech_omni::estado::do_valor(&valor).unwrap();
    let ids: Vec<Option<u64>> = estado.janelas[0]
        .paineis
        .iter()
        .flatten()
        .map(|i| i.id)
        .collect();
    assert_eq!(ids, [Some(2), Some(4)], "{texto}");
    assert!(matches!(
        armazenamento::travar(pasta.caminho()).unwrap(),
        Trava::Ocupada
    ));
    drop(m);
    encerrar(&["2-1", "4-2"]);
}
