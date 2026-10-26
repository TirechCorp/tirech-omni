//! A página Início, os recentes, a busca global e as configurações do M26.

mod comum;

use tirech_omni::cofre::Tipo;
use tirech_omni::recentes::fixar_relogio;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, exemplo, item_do_tipo, montar, montar_pura};

/// O relógio dos testes: 14/11/2023, 22:13:20 UTC.
const T0: u64 = 1_700_000_000;

/// Fecha as sessões que uma "execução" anterior deixou no registro desta thread.
fn encerrar() {
    for id in 0..=5 {
        for n in 1..=8 {
            if let Some(s) = Session::find(&format!("{id}-{n}")) {
                s.dispose();
            }
        }
    }
}

fn escrever_configuracoes(pasta: &Pasta, json: &str) {
    std::fs::create_dir_all(pasta.caminho()).expect("criar a pasta do teste");
    std::fs::write(pasta.armazenamento().arquivo_configuracoes(), json)
        .expect("gravar as configurações do teste");
}

fn abas(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()))
        .collect()
}

/// Se o widget existe: `Show` e `For` não deixam na árvore o que não aparece.
fn existe(app: &mut TestApp, id: &str) -> bool {
    app.ui().find(id).is_some()
}

fn recentes_gravados(pasta: &Pasta) -> String {
    std::fs::read_to_string(pasta.armazenamento().arquivo_recentes()).unwrap_or_default()
}

/// Abre `web-01` (id 2) pela árvore, num app na página Terminais, e o fecha ao sair do escopo.
fn abrir_web_01_pela_arvore(pasta: &Pasta) {
    let mut m = montar(pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    assert_eq!(abas(&m.app), ["web-01"]);
}

/// Um app novo na pasta, que abre em Início, depois de uma execução que abriu `web-01`.
fn reiniciar_no_inicio(pasta: &Pasta) -> TestApp {
    encerrar();
    std::fs::remove_file(pasta.armazenamento().arquivo_estado()).ok();
    escrever_configuracoes(pasta, r#"{"pagina_inicial": "inicio"}"#);
    montar_pura(pasta, Theme::Light).app
}

#[test]
fn abrir_pela_arvore_registra_o_recente_e_o_inicio_mostra() {
    fixar_relogio(Some(T0));
    let pasta = Pasta::nova("recente-arvore");
    pasta.semear(&exemplo());
    abrir_web_01_pela_arvore(&pasta);
    assert!(recentes_gravados(&pasta).contains("web-01"));

    fixar_relogio(Some(T0 + 300));
    let mut app = reiniciar_no_inicio(&pasta);
    assert_eq!(app.text("navegacao"), "inicio");
    assert!(app.visible("inicio-recente-0"));
    assert_eq!(app.label("inicio-recente-0"), "web-01");
    assert!(app.visible("inicio-recente-0"));
    assert!(!existe(&mut app, "inicio-sem-recentes"));
    assert!(!existe(&mut app, "inicio-fixada-0"));
    assert_eq!(
        app.text("inicio-recente-0-detalhe"),
        "deploy@web-01.exemplo:22 · há 5 min"
    );
    assert!(app.text("inicio-versao").starts_with("Versão "));
    app.assert_lint();
    encerrar();
    fixar_relogio(None);
}

#[test]
fn clicar_no_cartao_abre_a_aba_e_vai_para_os_terminais() {
    fixar_relogio(Some(T0));
    let pasta = Pasta::nova("recente-clicar");
    pasta.semear(&exemplo());
    abrir_web_01_pela_arvore(&pasta);

    let mut app = reiniciar_no_inicio(&pasta);
    assert!(abas(&app).is_empty());
    app.click("inicio-recente-0");
    assert_eq!(app.text("navegacao"), "terminais");
    assert_eq!(abas(&app), ["web-01"]);
    encerrar();
    fixar_relogio(None);
}

#[test]
fn restaurar_o_estado_nao_conta_como_abertura() {
    fixar_relogio(Some(T0));
    let pasta = Pasta::nova("recente-restaurar");
    pasta.semear(&exemplo());
    abrir_web_01_pela_arvore(&pasta);
    fixar_relogio(Some(T0 + 600));
    encerrar();
    {
        let m = montar(&pasta, Theme::Light);
        assert_eq!(abas(&m.app), ["web-01"], "a aba voltou");
    }
    encerrar();
    assert!(
        recentes_gravados(&pasta).contains(&format!("\"ultimo\": {T0}"))
            || recentes_gravados(&pasta).contains(&format!("\"ultimo\":{T0}")),
        "o uso continua o de antes: {}",
        recentes_gravados(&pasta)
    );
    fixar_relogio(None);
}

#[test]
fn fixar_desafixar_e_remover_pelo_menu_do_cartao() {
    fixar_relogio(Some(T0));
    let pasta = Pasta::nova("recente-menu");
    pasta.semear(&exemplo());
    abrir_web_01_pela_arvore(&pasta);

    let mut app = reiniciar_no_inicio(&pasta);
    app.right_click("inicio-recente-0");
    app.click("inicio-recente-0-menu-fixar");
    assert!(app.visible("inicio-fixada-0"));
    assert!(!existe(&mut app, "inicio-recente-0"));
    assert!(recentes_gravados(&pasta).contains("\"fixado\": true"));
    drop(app);

    // A fixação sobrevive ao reinício.
    let mut app = reiniciar_no_inicio(&pasta);
    assert!(app.visible("inicio-fixada-0"));
    app.right_click("inicio-fixada-0");
    assert_eq!(app.label("inicio-fixada-0-menu-fixar"), "Desafixar");
    app.click("inicio-fixada-0-menu-fixar");
    assert!(app.visible("inicio-recente-0"));
    assert!(!existe(&mut app, "inicio-fixada-0"));

    app.right_click("inicio-recente-0");
    app.click("inicio-recente-0-menu-remover");
    assert!(!existe(&mut app, "inicio-recente-0"));
    assert!(app.visible("inicio-sem-recentes"));
    assert!(!recentes_gravados(&pasta).contains("web-01"));
    encerrar();
    fixar_relogio(None);
}

#[test]
fn mostrar_na_arvore_abre_os_grupos_e_escolhe_a_conexao() {
    fixar_relogio(Some(T0));
    let pasta = Pasta::nova("recente-arvore-mostrar");
    pasta.semear(&exemplo());
    abrir_web_01_pela_arvore(&pasta);

    let mut app = reiniciar_no_inicio(&pasta);
    app.right_click("inicio-recente-0");
    app.click("inicio-recente-0-menu-arvore");
    assert_eq!(app.text("navegacao"), "terminais");
    let linhas = app.tree_items("arvore");
    let web = linhas
        .iter()
        .find(|l| l.label.starts_with("web-01"))
        .expect("o grupo foi aberto");
    assert!(web.selected);
    encerrar();
    fixar_relogio(None);
}

#[test]
fn apagar_a_conexao_tira_o_recente_da_tela_e_do_arquivo() {
    fixar_relogio(Some(T0));
    let pasta = Pasta::nova("recente-apagar");
    pasta.semear(&exemplo());
    {
        let mut m = montar(&pasta, Theme::Light);
        m.app.expand("arvore", "1");
        m.app.double_click("arvore-2");
        m.app.click("arvore-2").click("apagar");
        m.app.click("apagar-dialogo-primary");
        assert!(!recentes_gravados(&pasta).contains("web-01"));
    }
    let mut app = reiniciar_no_inicio(&pasta);
    assert!(app.visible("inicio-sem-recentes"));
    assert!(!existe(&mut app, "inicio-recente-0"));
    encerrar();
    fixar_relogio(None);
}

#[test]
fn sem_recentes_o_inicio_convida_a_criar_uma_conexao() {
    let pasta = Pasta::nova("inicio-vazio");
    escrever_configuracoes(&pasta, r#"{"pagina_inicial": "inicio"}"#);
    let mut m = montar_pura(&pasta, Theme::Light);
    assert_eq!(m.app.text("navegacao"), "inicio");
    assert_eq!(
        m.app.text("inicio-sem-recentes"),
        "Nenhuma conexão aberta ainda."
    );
    m.app.click("inicio-recentes-novo");
    assert_eq!(m.app.text("navegacao"), "terminais");
    assert_eq!(pasta.conexoes().connection_count(), 1);

    m.app.click("navegacao-inicio");
    m.app.click("inicio-novo");
    assert_eq!(pasta.conexoes().connection_count(), 2);
    m.app.click("navegacao-inicio");
    m.app.click("inicio-shell");
    assert_eq!(m.app.text("navegacao"), "terminais");
    assert_eq!(abas(&m.app).len(), 1);
    encerrar();
}

#[test]
fn os_cartoes_de_ferramentas_levam_as_paginas() {
    let pasta = Pasta::nova("inicio-ferramentas");
    escrever_configuracoes(&pasta, r#"{"pagina_inicial": "inicio"}"#);
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-cofre");
    assert_eq!(m.app.text("navegacao"), "cofre");
    m.app.click("navegacao-inicio");
    m.app.click("inicio-ferramenta-configuracoes");
    assert_eq!(m.app.text("navegacao"), "configuracoes");
    assert!(m.app.visible("configuracoes-pagina"));
}

#[test]
fn a_pagina_ao_iniciar_segue_o_estado_e_as_configuracoes() {
    let pasta = Pasta::nova("pagina-inicial");
    pasta.semear(&exemplo());

    // Sem estado, a automática abre o Início.
    let m = montar_pura(&pasta, Theme::Light);
    assert_eq!(m.app.text("navegacao"), "inicio");
    drop(m);

    // Com abas no estado, abre os Terminais.
    escrever_configuracoes(&pasta, r#"{"pagina_inicial": "terminais"}"#);
    {
        let mut m = montar_pura(&pasta, Theme::Light);
        m.app.expand("arvore", "1");
        m.app.double_click("arvore-2");
    }
    encerrar();
    std::fs::remove_file(pasta.armazenamento().arquivo_configuracoes()).unwrap();
    let m = montar_pura(&pasta, Theme::Light);
    assert_eq!(m.app.text("navegacao"), "terminais");
    drop(m);
    encerrar();

    // "inicio" vence o estado.
    escrever_configuracoes(&pasta, r#"{"pagina_inicial": "inicio"}"#);
    let m = montar_pura(&pasta, Theme::Light);
    assert_eq!(m.app.text("navegacao"), "inicio");
    drop(m);
    encerrar();
}

#[test]
fn a_busca_acha_a_conexao_pelo_host_e_abre_a_aba() {
    let pasta = Pasta::nova("busca-conexao");
    pasta.semear(&exemplo());
    let mut m = montar_pura(&pasta, Theme::Light);
    assert_eq!(m.app.text("navegacao"), "inicio");
    m.app.click("busca").type_text("10.0.0.20");
    assert!(m.app.is_open("busca"));
    m.app.key(tomni_ui::Key::Down).key(tomni_ui::Key::Enter);
    assert_eq!(m.app.text("navegacao"), "terminais");
    assert_eq!(abas(&m.app), ["db-01"]);
    assert_eq!(m.app.text("busca"), "", "a busca é limpa");
    drop(m);
    encerrar();
}

#[test]
fn enviar_um_texto_sem_escolher_usa_a_primeira_sugestao() {
    let pasta = Pasta::nova("busca-enter");
    pasta.semear(&exemplo());
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("busca").type_text("roteador");
    m.app.key(tomni_ui::Key::Enter);
    assert_eq!(abas(&m.app), ["Roteador"]);
    drop(m);
    encerrar();
}

#[test]
fn a_busca_acha_itens_do_cofre_e_o_seleciona() {
    let pasta = Pasta::nova("busca-cofre");
    pasta.semear(&exemplo());
    let ids = pasta.semear_cofre(vec![
        item_do_tipo(Tipo::Ssh, "Servidor de backup"),
        item_do_tipo(Tipo::Ssh, "Outro"),
    ]);
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("busca").type_text("backup");
    m.app.key(tomni_ui::Key::Enter);
    assert_eq!(m.app.text("navegacao"), "cofre");
    assert!(m.app.visible(&format!("cofre-lista-item:{}", ids[0])));
}

#[test]
fn o_atalho_ctrl_e_leva_o_foco_para_a_busca() {
    let pasta = Pasta::nova("busca-atalho");
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.shortcut(tomni_ui::prelude::Shortcut::ctrl('E'));
    assert_eq!(m.app.focused().as_deref(), Some("busca"));
}

#[test]
fn limpar_recentes_mantem_os_fixados() {
    fixar_relogio(Some(T0));
    let pasta = Pasta::nova("config-limpar");
    pasta.semear(&exemplo());
    {
        let mut m = montar(&pasta, Theme::Light);
        m.app.expand("arvore", "1");
        m.app.double_click("arvore-2").double_click("arvore-3");
    }
    let mut app = reiniciar_no_inicio(&pasta);
    app.right_click("inicio-recente-0");
    app.click("inicio-recente-0-menu-fixar");
    assert!(app.visible("inicio-fixada-0"));
    assert!(app.visible("inicio-recente-0"));
    assert_eq!(app.label("inicio-fixada-0"), "db-01");
    assert_eq!(app.label("inicio-recente-0"), "web-01");

    app.click("inicio-ferramenta-configuracoes");
    app.click("config-secao-geral");
    app.click("config-acao-limpar_recentes");
    app.click("navegacao-inicio");
    assert!(app.visible("inicio-fixada-0"));
    assert!(!existe(&mut app, "inicio-recente-0"));
    encerrar();
    fixar_relogio(None);
}

#[test]
fn a_pagina_de_configuracoes_mostra_e_grava_o_arquivo() {
    let pasta = Pasta::nova("config-gravar");
    escrever_configuracoes(
        &pasta,
        r#"{"pagina_inicial": "inicio", "desconhecida": "claro"}"#,
    );
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-geral");
    assert_eq!(m.app.text("config-geral-pagina_inicial-valor"), "Início");
    assert_eq!(m.app.value("config-geral-recentes_max-valor"), 30.0);

    m.app
        .select("config-geral-pagina_inicial-valor", "Terminais");
    m.app.select("config-geral-pagina_inicial-valor", "Início");
    m.app.set_value("config-geral-recentes_max-valor", 10.0);

    let texto = std::fs::read_to_string(pasta.armazenamento().arquivo_configuracoes()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&texto).unwrap();
    assert_eq!(json["geral"]["recentes_max"], 10, "{texto}");
    assert_eq!(json["geral"]["pagina_inicial"], "inicio");
    assert_eq!(
        json["desconhecida"], "claro",
        "o que o app não conhece sobrevive"
    );
}
