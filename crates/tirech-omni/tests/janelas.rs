//! Várias janelas no mesmo processo (M28): abrir, listar, dar o foco, abrir conexões na janela
//! ativa, mover sessões entre janelas, fechar e lembrar as janelas entre uma execução e outra.

mod comum;

use tomni_ui::Modifiers;
use tomni_ui::WindowHandle;
use tomni_ui::prelude::{Key, Shortcut};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use tirech_omni::configuracoes::{AoFechar, Configuracoes, PaginaInicial};
use tirech_omni::marca::NOME_DO_APP;

use comum::{Montagem, Pasta, exemplo, montar};

/// Os títulos das abas de cada painel da janela atual.
fn abas(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()).collect())
        .collect()
}

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar(chaves: &[&str]) {
    for chave in chaves {
        if let Some(s) = Session::find(chave) {
            s.dispose();
        }
    }
}

/// O painel `id` da página Terminais está à mostra na janela atual? Um painel fechado sai da
/// árvore de widgets.
fn painel_aberto(app: &TestApp, id: &str) -> bool {
    app.tree().find(id).is_some() && app.visible(id)
}

/// A tela com uma segunda janela aberta e a principal em foco.
fn com_duas_janelas(pasta: &Pasta) -> (Montagem, WindowHandle, WindowHandle) {
    let mut m = montar(pasta, Theme::Light);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    (m, principal, outra)
}

#[test]
fn nova_janela_pelo_menu_e_pelo_atalho() {
    let pasta = Pasta::nova("janelas-nova");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    assert_eq!(m.app.windows().len(), 1);

    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    assert_eq!(m.app.windows().len(), 2);
    let segunda = m.app.windows()[1];
    assert_eq!(segunda.title(), format!("{NOME_DO_APP} — Janela 2"));

    m.app
        .shortcut(Shortcut::new('N', Modifiers::CTRL | Modifiers::ALT));
    m.app.frame();
    assert_eq!(m.app.windows().len(), 3);
    assert_eq!(
        m.app.windows()[2].title(),
        format!("{NOME_DO_APP} — Janela 3")
    );
}

#[test]
fn com_varias_janelas_a_principal_vem_marcada() {
    let pasta = Pasta::nova("janelas-principal");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.frame();
    let principal = m.app.windows()[0];
    assert_eq!(principal.title(), NOME_DO_APP);

    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    assert_eq!(
        principal.title(),
        format!("{NOME_DO_APP} — Janela principal")
    );
    // O subtítulo "Janela principal" ganha espaço na barra (o lint pega texto com 0×0).
    m.app.switch_to(principal);
    m.app.assert_lint();
    m.app.click("menus-janela");
    assert_eq!(m.app.label("janela-foco-1"), "Janela 1 (principal)");
    assert_eq!(m.app.label("janela-foco-2"), "Janela 2");
}

#[test]
fn a_janela_secundaria_so_tem_titulo_menus_e_paineis() {
    let pasta = Pasta::nova("janelas-casca");
    pasta.semear(&exemplo());
    let (mut m, _principal, outra) = com_duas_janelas(&pasta);
    m.app.switch_to(outra);
    for menu in ["janela", "exibir"] {
        assert!(m.app.visible(&format!("menus-{menu}")), "sem o menu {menu}");
    }
    assert!(m.app.visible("boas-vindas"));
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);
    m.app.assert_lint();
}

#[test]
fn o_menu_janela_lista_as_janelas_e_da_o_foco() {
    let pasta = Pasta::nova("janelas-menu");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);

    m.app.click("menus-janela");
    assert!(m.app.visible("janela-foco-1"));
    assert!(m.app.visible("janela-foco-2"));
    assert!(!m.app.visible("janela-foco-3"));
    m.app.click("janela-foco-2");
    m.app.set_active_window(outra);
    assert_eq!(outra.title(), format!("{NOME_DO_APP} — Janela 2"));
    assert!(principal.is_open() && outra.is_open());
}

#[test]
fn o_clique_duplo_abre_na_ultima_janela_ativa() {
    let pasta = Pasta::nova("janelas-ativa");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);

    m.app.set_active_window(outra);
    m.app.double_click("arvore-4");
    m.app.frame();
    assert_eq!(
        abas(&m.app),
        [Vec::<String>::new()],
        "a principal não recebeu a aba"
    );
    m.app.switch_to(outra);
    assert_eq!(abas(&m.app), [["Roteador"]]);

    // Com a principal ativa de novo, a aba vai para ela.
    m.app.switch_to(principal);
    m.app.set_active_window(principal);
    m.app.expand("arvore", "1").double_click("arvore-2");
    assert_eq!(abas(&m.app), [["web-01"]]);
    encerrar(&["2-1", "4-1", "4-2"]);
}

#[test]
fn mover_pelo_menu_da_aba_leva_a_sessao_viva() {
    let pasta = Pasta::nova("janelas-mover");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2").double_click("arvore-3");
    let chaves: Vec<String> = m.app.panes("paineis").panes[0]
        .tabs
        .iter()
        .map(|(chave, _)| chave.clone())
        .collect();
    assert_eq!(chaves.len(), 2);
    Session::find(&chaves[0]).unwrap().feed(b"uptime\r\n");

    m.app
        .context_menu(&format!("paineis-painel-0-{}", chaves[0]));
    m.app.click("mover-janela").click("mover-janela-nova");
    m.app.frame();
    assert_eq!(m.app.windows().len(), 2);
    assert_eq!(abas(&m.app), [["db-01"]]);
    m.app.switch_to(m.app.windows()[1]);
    assert_eq!(abas(&m.app), [["web-01"]]);
    // A mesma sessão: o histórico continua lá, ninguém reconectou.
    assert!(m.app.session_text(&chaves[0]).contains("uptime"));
    assert_eq!(m.opener.abertas.borrow().len(), 2);

    // Da secundária para a principal: sem outras abas, a secundária fecha.
    let secundaria = m.app.current_window();
    m.app
        .context_menu(&format!("paineis-painel-0-{}", chaves[0]));
    m.app.click("mover-janela").click("mover-janela-1");
    m.app.frame();
    assert!(!secundaria.is_open());
    assert_eq!(m.app.current_window(), WindowHandle::main());
    assert_eq!(abas(&m.app), [["db-01", "web-01"]]);
    encerrar(&chaves.iter().map(String::as_str).collect::<Vec<_>>());
}

#[test]
fn arrastar_a_aba_para_outra_janela_nao_reconecta() {
    let pasta = Pasta::nova("janelas-arrastar");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2").double_click("arvore-3");
    let chave = m.app.panes("paineis").panes[0].tabs[1].0.clone();
    Session::find(&chave).unwrap().feed(b"hostname\r\n");

    m.app
        .drag_tab_to_window(&format!("paineis-painel-0-{chave}"), outra, "paineis");
    m.app.frame();
    m.app.switch_to(outra);
    assert_eq!(abas(&m.app), [["db-01"]]);
    assert!(m.app.session_text(&chave).contains("hostname"));
    m.app.switch_to(principal);
    assert_eq!(abas(&m.app), [["web-01"]]);
    assert_eq!(m.opener.abertas.borrow().len(), 2);
    encerrar(&["2-1", "3-2"]);
}

#[test]
fn mover_entre_quadrantes_da_mesma_janela() {
    let pasta = Pasta::nova("janelas-quadrantes");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-exibir").click("layout-2x2");
    m.app.expand("arvore", "1").double_click("arvore-2");
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    Session::find(&chave).unwrap().feed(b"pwd\r\n");
    m.app.drag(
        format!("paineis-painel-0-{chave}").as_str(),
        "paineis-painel-1-quadro",
    );
    m.app.frame();
    assert_eq!(
        abas(&m.app),
        [vec![], vec!["web-01".to_owned()], vec![], vec![]]
    );
    assert!(m.app.session_text(&chave).contains("pwd"));
    assert_eq!(m.opener.abertas.borrow().len(), 1);
    encerrar(&[&chave]);
}

#[test]
fn soltar_a_aba_fora_abre_uma_janela_nova() {
    let pasta = Pasta::nova("janelas-tear-off");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2").double_click("arvore-3");
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    m.app.drag_tab_outside(&format!("paineis-painel-0-{chave}"));
    m.app.frame();
    assert_eq!(m.app.windows().len(), 2);
    assert_eq!(abas(&m.app), [["db-01"]]);
    m.app.switch_to(m.app.windows()[1]);
    assert_eq!(abas(&m.app), [["web-01"]]);
    encerrar(&["2-1", "3-2"]);
}

#[test]
fn fechar_a_secundaria_sem_sessoes_fecha_direto() {
    let pasta = Pasta::nova("janelas-fechar-vazia");
    pasta.semear(&exemplo());
    let (mut m, _principal, outra) = com_duas_janelas(&pasta);
    assert!(m.app.close_window(outra));
    assert!(!outra.is_open());
    assert_eq!(m.app.windows().len(), 1);
    assert!(!m.app.exited());
}

#[test]
fn fechar_a_secundaria_com_sessoes_pergunta_o_que_fazer() {
    let pasta = Pasta::nova("janelas-fechar-sessoes");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);
    m.app.set_active_window(outra);
    m.app.double_click("arvore-4");
    m.app.switch_to(outra);
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();

    // Cancelar: a janela e a sessão ficam.
    assert!(!m.app.close_window(outra));
    assert!(m.app.is_open("fechar-janela-dialogo"));
    m.app.click("fechar-janela-dialogo-close");
    assert!(outra.is_open());
    assert!(Session::find(&chave).is_some());

    // Mover para a principal: a sessão sobrevive lá.
    assert!(!m.app.close_window(outra));
    m.app.click("fechar-janela-dialogo-primary");
    m.app.frame();
    assert!(!outra.is_open());
    assert_eq!(m.app.current_window(), principal);
    assert_eq!(abas(&m.app), [["Roteador"]]);
    assert!(Session::find(&chave).is_some());
    m.app.click("menus-janela");
    assert!(
        !m.app.visible("janela-foco-2"),
        "a janela fechada saiu da lista"
    );
    m.app.key(Key::Escape);

    // Fechar as sessões: uma nova janela com a mesma sessão, fechada com a sessão junto.
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let terceira = m.app.windows()[1];
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("mover-janela").click("mover-janela-2");
    m.app.frame();
    m.app.switch_to(terceira);
    assert!(!m.app.close_window(terceira));
    m.app.click("fechar-janela-dialogo-secondary");
    m.app.frame();
    assert!(!terceira.is_open());
    assert!(Session::find(&chave).is_none(), "a sessão foi encerrada");
}

#[test]
fn fechar_a_principal_com_outras_abertas_pergunta() {
    let pasta = Pasta::nova("janelas-fechar-tudo");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);
    // Sem bandeja, fechar a principal é o caminho de sempre (com bandeja, o diálogo é outro).
    m.app.set_tray_available(false);
    m.app.switch_to(principal);

    assert!(!m.app.close_window(principal));
    assert!(m.app.is_open("fechar-tudo-dialogo"));
    m.app.click("fechar-tudo-dialogo-close");
    assert!(!m.app.is_open("fechar-tudo-dialogo"));
    assert!(outra.is_open() && !m.app.exited());

    assert!(!m.app.close_window(principal));
    m.app.click("fechar-tudo-dialogo-primary");
    assert!(m.app.exited());
}

#[test]
fn fechar_a_principal_sozinha_nao_pergunta() {
    let pasta = Pasta::nova("janelas-fechar-sozinha");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.set_tray_available(false);
    assert!(m.app.close_window(WindowHandle::main()));
    assert!(m.app.exited());
}

#[test]
fn as_duas_janelas_voltam_na_proxima_execucao() {
    let pasta = Pasta::nova("janelas-estado");
    pasta.semear(&exemplo());
    {
        let (mut m, _principal, outra) = com_duas_janelas(&pasta);
        m.app.set_tray_available(false);
        m.app.expand("arvore", "1").double_click("arvore-2");
        m.app.set_active_window(outra);
        m.app.double_click("arvore-4");
        m.app.frame();
        assert!(!m.app.close_window(WindowHandle::main()));
        m.app.click("fechar-tudo-dialogo-primary");
        assert!(m.app.exited());
    }
    encerrar(&["2-1", "4-2"]);
    let estado = std::fs::read_to_string(pasta.caminho().join("estado.json")).unwrap();
    assert!(estado.contains("janelas"), "estado.json: {estado}");

    let mut m = montar(&pasta, Theme::Light);
    assert_eq!(m.app.windows().len(), 2, "as duas janelas voltaram");
    assert_eq!(abas(&m.app), [["web-01"]]);
    let [_, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(outra);
    assert_eq!(abas(&m.app), [["Roteador"]]);
    encerrar(&["2-1", "4-2", "2-2", "4-1"]);
}

#[test]
fn o_estado_do_m24_ainda_abre() {
    let pasta = Pasta::nova("janelas-estado-m24");
    pasta.semear(&exemplo());
    std::fs::write(
        pasta.caminho().join("estado.json"),
        r#"{"grade":[2,1],"paineis":[[2],[4]]}"#,
    )
    .unwrap();
    let m = montar(&pasta, Theme::Light);
    assert_eq!(m.app.windows().len(), 1);
    assert_eq!(abas(&m.app), [["web-01"], ["Roteador"]]);
    encerrar(&["2-1", "4-2"]);
}

#[test]
fn nao_confunde_a_tecla_n_sem_modificadores() {
    let pasta = Pasta::nova("janelas-n");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.key_with(Key::Char('n'), Modifiers::NONE);
    m.app.frame();
    assert_eq!(m.app.windows().len(), 1);
}

// --- M62: o mesmo menu em todas as janelas ---

/// Os ids dos itens de cada menu da barra, na ordem em que aparecem. Os painéis da lista de
/// notas e da fila só aparecem no Exibir da principal quando a página deles está aberta, e
/// ficam de fora daqui (o teste dos painéis desabilitados os cobre).
const MENUS: [(&str, &[&str]); 5] = [
    (
        "arquivo",
        &[
            "nova-conexao-menu",
            "novo-grupo-menu",
            "importar-menu",
            "exportar-menu",
            "remotos-rclone-menu",
            "sair",
        ],
    ),
    (
        "exibir",
        &[
            "layout-1",
            "layout-2x2",
            "layout-3x3",
            "painel-conexoes",
            "painel-propriedades",
            "painel-chat",
        ],
    ),
    (
        "janela",
        &[
            "nova-janela",
            "salvar-janela-espaco",
            "salvar-todas-espaco",
            "janela-foco-1",
        ],
    ),
    (
        "ferramentas",
        &[
            "abrir-shell",
            "abrir-sessao",
            "reconectar",
            "duplicar",
            "multi-entrada-menu",
            "cluster-modo",
            "scripts-menu",
            "copiar-chave-menu",
            "abrir-cofre",
            "abrir-notas",
            "reconectar-caidas",
            "fechar-caidas",
            "fechar-todas",
            "gravar-log",
            "parar-log",
            "mcp-assistentes",
        ],
    ),
    ("ajuda", &["atalhos", "sobre"]),
];

/// Abre o menu `menu` da janela em foco do teste e devolve os ids de `MENUS` que aparecem.
fn itens_do_menu(app: &mut TestApp, menu: &str, candidatos: &[&'static str]) -> Vec<&'static str> {
    app.click(&format!("menus-{menu}"));
    let visiveis = candidatos
        .iter()
        .copied()
        .filter(|id| app.visible(id))
        .collect();
    app.key(Key::Escape);
    visiveis
}

#[test]
fn a_janela_secundaria_tem_os_cinco_menus_da_principal() {
    let pasta = Pasta::nova("janelas-menus-iguais");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);

    for (menu, ids) in MENUS {
        m.app.switch_to(principal);
        let na_principal = itens_do_menu(&mut m.app, menu, ids);
        assert_eq!(
            na_principal, ids,
            "a principal não mostra os itens do menu {menu}"
        );
        m.app.switch_to(outra);
        let na_outra = itens_do_menu(&mut m.app, menu, ids);
        assert_eq!(
            na_outra, na_principal,
            "o menu {menu} difere entre as janelas"
        );
    }
    m.app.assert_lint();
}

#[test]
fn o_exibir_da_secundaria_controla_as_conexoes_e_as_propriedades_dela() {
    let pasta = Pasta::nova("janelas-exibir-paineis");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);

    m.app.switch_to(principal);
    m.app.click("menus-exibir");
    assert!(m.app.enabled("painel-conexoes"));
    assert!(m.app.enabled("painel-fila"));
    m.app.key(Key::Escape);

    m.app.switch_to(outra);
    m.app.click("menus-exibir");
    for id in ["painel-conexoes", "painel-propriedades"] {
        assert!(m.app.enabled(id), "{id} devia valer na secundária");
    }
    for id in ["painel-chat", "painel-notas-lista", "painel-fila"] {
        assert!(m.app.visible(id), "{id} sumiu");
        assert!(!m.app.enabled(id), "{id} devia estar desabilitado");
    }
    assert!(
        m.app.enabled("layout-2x2"),
        "o layout vale em qualquer janela"
    );

    // Esconder as Conexões da secundária não mexe nas da principal.
    m.app.click("painel-conexoes");
    assert!(!painel_aberto(&m.app, "terminais-paineis-conexoes"));
    assert!(painel_aberto(&m.app, "terminais-paineis-propriedades"));
    m.app.switch_to(principal);
    assert!(painel_aberto(&m.app, "terminais-paineis-conexoes"));
    m.app.switch_to(outra);
    m.app.click("menus-exibir").click("painel-conexoes");
    assert!(painel_aberto(&m.app, "terminais-paineis-conexoes"));
}

#[test]
fn a_secundaria_tem_a_arvore_e_as_propriedades_com_a_mesma_selecao() {
    let pasta = Pasta::nova("janelas-arvore");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);
    m.app.set_active_window(outra);
    m.app.switch_to(outra);
    assert!(m.app.visible("terminais-paineis-conexoes"));
    assert!(m.app.visible("terminais-paineis-propriedades"));
    assert!(m.app.visible("arvore"));
    m.app.click("arvore-5");
    let escolhida = |app: &TestApp| {
        app.tree_items("arvore")
            .into_iter()
            .find(|l| l.selected)
            .map(|l| l.key)
    };
    assert_eq!(escolhida(&m.app).as_deref(), Some("5"));

    // A escolha, as pastas abertas e o filtro são os mesmos nas duas janelas.
    m.app.expand("arvore", "1");
    m.app.switch_to(principal);
    assert_eq!(escolhida(&m.app).as_deref(), Some("5"));
    assert!(
        m.app.tree_items("arvore").iter().any(|l| l.key == "2"),
        "a pasta aberta na secundária aparece aberta na principal"
    );

    // O clique duplo na árvore da secundária abre a conexão nela.
    m.app.switch_to(outra);
    m.app.double_click("arvore-4");
    m.app.frame();
    assert_eq!(abas(&m.app), [["Roteador"]]);
    m.app.assert_lint();
    encerrar(&["4-1"]);
}

#[test]
fn apagar_na_secundaria_pergunta_nela() {
    let pasta = Pasta::nova("janelas-apagar");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);
    m.app.set_active_window(outra);
    m.app.switch_to(outra);
    m.app.click("arvore-5").click("apagar");
    assert!(
        m.app.is_open("apagar-dialogo"),
        "o diálogo abre na secundária"
    );
    m.app.switch_to(principal);
    assert!(
        !m.app.is_open("apagar-dialogo"),
        "a principal não mostra o diálogo da secundária"
    );
    m.app.switch_to(outra);
    m.app.click("apagar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 3);
    assert!(pasta.conexoes().nodes.iter().all(|n| n.name() != "Shell"));

    // Na principal, de novo ativa, o diálogo volta a abrir lá.
    m.app.set_active_window(principal);
    m.app.switch_to(principal);
    m.app.click("arvore-4").click("apagar");
    assert!(m.app.is_open("apagar-dialogo"));
    m.app.switch_to(outra);
    assert!(!m.app.is_open("apagar-dialogo"));
}

#[test]
fn a_lupa_da_secundaria_poe_o_cursor_no_filtro_dela() {
    let pasta = Pasta::nova("janelas-lupa");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);
    m.app.set_active_window(outra);
    m.app.switch_to(outra);
    m.app.click("filtrar");
    m.app.wait(std::time::Duration::from_millis(100));
    assert!(m.app.visible("filtro"));
    assert_eq!(m.app.focused().as_deref(), Some("filtro"));
    m.app.switch_to(principal);
    assert_ne!(m.app.focused().as_deref(), Some("filtro"));
}

#[test]
fn os_paineis_da_secundaria_voltam_na_proxima_execucao() {
    let pasta = Pasta::nova("janelas-paineis-estado");
    pasta.semear(&exemplo());
    {
        let (mut m, principal, outra) = com_duas_janelas(&pasta);
        m.app.set_tray_available(false);
        m.app.set_active_window(outra);
        m.app.switch_to(outra);
        m.app.click("menus-exibir").click("painel-propriedades");
        assert!(!painel_aberto(&m.app, "terminais-paineis-propriedades"));
        m.app.double_click("arvore-4");
        m.app.frame();
        m.app.switch_to(principal);
        assert!(!m.app.close_window(WindowHandle::main()));
        m.app.click("fechar-tudo-dialogo-primary");
        assert!(m.app.exited());
    }
    encerrar(&["4-1"]);

    let mut m = montar(&pasta, Theme::Light);
    assert!(
        m.app.visible("terminais-paineis-propriedades"),
        "a principal guarda os dela"
    );
    let [_, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(outra);
    assert!(painel_aberto(&m.app, "terminais-paineis-conexoes"));
    assert!(!painel_aberto(&m.app, "terminais-paineis-propriedades"));
    encerrar(&["4-1", "4-2"]);
}

#[test]
fn o_cofre_na_secundaria_traz_a_principal_com_a_pagina() {
    let pasta = Pasta::nova("janelas-cofre");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);

    m.app.switch_to(outra);
    m.app.click("menus-ferramentas").click("abrir-cofre");
    m.app.switch_to(principal);
    assert_eq!(m.app.text("navegacao"), "cofre");
    assert!(m.app.visible("cofre-pagina"));

    // Ajuda › Sobre também vai para a principal.
    m.app.switch_to(outra);
    m.app.click("menus-ajuda").click("sobre");
    m.app.switch_to(principal);
    assert_eq!(m.app.text("navegacao"), "sobre");
}

#[test]
fn o_cofre_na_secundaria_tira_a_principal_da_bandeja() {
    let pasta = Pasta::nova("janelas-cofre-bandeja");
    pasta.semear(&exemplo());
    let mut config = Configuracoes::default();
    config.geral.pagina_inicial = PaginaInicial::Terminais;
    config.geral.ao_fechar = AoFechar::Bandeja;
    config.salvar(&pasta.armazenamento()).unwrap();
    let (mut m, principal, outra) = com_duas_janelas(&pasta);

    m.app.switch_to(principal);
    assert!(!m.app.close_window(principal));
    assert!(m.app.window_hidden(principal) && m.app.window_hidden(outra));

    // O menu da secundária age mesmo com tudo escondido: a principal volta com a página.
    m.app.switch_to(outra);
    m.app.click("menus-ferramentas").click("abrir-cofre");
    m.app.switch_to(principal);
    assert!(!m.app.window_hidden(principal));
    assert_eq!(m.app.text("navegacao"), "cofre");
}

#[test]
fn abrir_shell_na_secundaria_abre_a_aba_nela() {
    let pasta = Pasta::nova("janelas-shell");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);

    // A última janela ativa é a principal, mas o menu é o da secundária.
    m.app.set_active_window(principal);
    m.app.switch_to(outra);
    m.app.click("menus-ferramentas").click("abrir-shell");
    assert_eq!(abas(&m.app).concat().len(), 1, "a aba entrou na secundária");
    m.app.switch_to(principal);
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);
    for janela in [outra, principal] {
        m.app.switch_to(janela);
        for chave in m.app.panes("paineis").panes[0]
            .tabs
            .iter()
            .map(|(c, _)| c.clone())
            .collect::<Vec<_>>()
        {
            encerrar(&[&chave]);
        }
    }
}

#[test]
fn reconectar_e_parar_o_log_na_secundaria_agem_na_sessao_dela() {
    let pasta = Pasta::nova("janelas-sessao");
    pasta.semear(&exemplo());
    let (mut m, principal, outra) = com_duas_janelas(&pasta);
    m.app.expand("arvore", "1");

    m.app.set_active_window(principal);
    m.app.double_click("arvore-2");
    m.app.set_active_window(outra);
    m.app.double_click("arvore-3");
    m.app.frame();
    let sessoes = m.opener.sessoes.borrow().clone();
    assert_eq!(sessoes.len(), 2);
    let (da_principal, da_outra) = (sessoes[0].0, sessoes[1].0);

    // Reconectar: as duas caem, o menu da secundária só reabre a dela.
    m.opener.derrubar(2, "tempo esgotado");
    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();
    m.app.set_active_window(principal);
    m.app.switch_to(outra);
    m.app.click("menus-ferramentas").click("reconectar");
    m.app.frame();
    assert_eq!(m.opener.handle_de(da_outra).unwrap().reopened(), 1);
    assert_eq!(m.opener.handle_de(da_principal).unwrap().reopened(), 0);

    // Parar o log: as duas gravam, o menu da secundária só para a dela.
    let log_principal = pasta.caminho().join("principal.log");
    let log_outra = pasta.caminho().join("outra.log");
    da_principal
        .log_to(&log_principal, tomni_ui::terminal::LogMode::Text)
        .unwrap();
    da_outra
        .log_to(&log_outra, tomni_ui::terminal::LogMode::Text)
        .unwrap();
    m.app.click("menus-ferramentas").click("parar-log");
    assert!(!da_outra.is_logging());
    assert!(da_principal.is_logging());
    da_principal.stop_log();
    encerrar_todas(&m);
}

/// Descarta as sessões que o `Opener` falso abriu.
fn encerrar_todas(m: &Montagem) {
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn o_atalho_de_item_na_secundaria_abre_uma_janela_so() {
    let pasta = Pasta::nova("janelas-atalho-secundaria");
    pasta.semear(&exemplo());
    let (mut m, _principal, outra) = com_duas_janelas(&pasta);

    m.app.switch_to(outra);
    m.app
        .shortcut(Shortcut::new('N', Modifiers::CTRL | Modifiers::ALT));
    m.app.frame();
    assert_eq!(m.app.windows().len(), 3);
}
