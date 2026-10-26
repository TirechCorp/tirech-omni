//! As ferramentas para vários servidores (M42): abas caídas, fechar todas, a dica da aba e o
//! painel vazio.

mod comum;

use tomni_ui::theme::Theme;
use tomni_ui::{Look, Symbol};
use tomni_ui_test::TestApp;

use tirech_omni::marca::NOME_DO_APP;

use comum::{Montagem, Pasta, exemplo, montar};

/// A tela com a árvore de exemplo e o grupo "Produção" aberto (ids 2 e 3: `web-01` e `db-01`;
/// 4: o Telnet).
fn tela(nome: &str) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    (pasta, m)
}

/// Abre `web-01`, `db-01` e o Telnet, nesta ordem, no painel ativo.
fn abrir_tres(m: &mut Montagem) {
    m.app
        .double_click("arvore-2")
        .double_click("arvore-3")
        .double_click("arvore-4");
    m.app.frame();
}

/// As chaves das abas do painel `painel`.
fn chaves(app: &TestApp, painel: usize) -> Vec<String> {
    app.panes("paineis").panes[painel]
        .tabs
        .iter()
        .map(|(chave, _)| chave.clone())
        .collect()
}

/// O ícone da aba `chave` do painel 0.
fn icone(app: &TestApp, chave: &str) -> Option<Symbol> {
    match app.look(&format!("paineis-painel-0-{chave}")) {
        Look::TabHeader { visual, .. } => visual.icon,
        outro => panic!("{outro:?}"),
    }
}

/// Se a aba `chave` mostra o ícone de aba caída.
fn caida(app: &TestApp, chave: &str) -> bool {
    icone(app, chave) == Some(Symbol::Warning)
}

/// O texto da dica da aba `chave` do painel 0.
fn dica(app: &mut TestApp, chave: &str) -> String {
    app.hover(&format!("paineis-painel-0-{chave}"));
    let id = format!("paineis-painel-0-{chave}-tooltip");
    let arvore = app.tree();
    let no = arvore.find(&id).unwrap_or_else(|| panic!("sem {id:?}"));
    no.children
        .first()
        .and_then(|texto| texto.text.clone())
        .unwrap_or_default()
}

fn encerrar(m: &Montagem) {
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

// ---------------------------------------------------------------------------------------
// Abas caídas
// ---------------------------------------------------------------------------------------

#[test]
fn reconectar_as_abas_caidas_reabre_so_as_caidas() {
    let (_pasta, mut m) = tela("ferramentas-reconectar");
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);
    assert_eq!(chaves.len(), 3);

    m.opener.derrubar(2, "tempo esgotado");
    m.opener.derrubar(3, "conexão recusada");
    m.app.frame();
    assert!(caida(&m.app, &chaves[0]));
    assert!(caida(&m.app, &chaves[1]));
    assert!(!caida(&m.app, &chaves[2]));

    m.app.click("menus-ferramentas").click("reconectar-caidas");
    m.app.frame();
    assert_eq!(m.app.text("aviso-sucesso"), "2 abas reconectadas.");
    for chave in &chaves {
        assert!(!caida(&m.app, chave), "{chave} ainda caída");
    }
    let sessoes = m.opener.sessoes.borrow().clone();
    let reaberturas: Vec<usize> = sessoes
        .iter()
        .map(|(s, _)| m.opener.handle_de(*s).unwrap().reopened())
        .collect();
    assert_eq!(reaberturas, [1, 1, 0]);
    encerrar(&m);
}

#[test]
fn fechar_as_abas_caidas_deixa_as_vivas() {
    let (_pasta, mut m) = tela("ferramentas-fechar-caidas");
    abrir_tres(&mut m);
    let antes = chaves(&m.app, 0);

    m.opener.derrubar(2, "tempo esgotado");
    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();
    m.app.click("menus-ferramentas").click("fechar-caidas");
    m.app.frame();

    assert_eq!(m.app.text("aviso-sucesso"), "2 abas caídas fechadas.");
    assert_eq!(chaves(&m.app, 0), [antes[2].clone()]);
    encerrar(&m);
}

#[test]
fn sem_aba_caida_o_aviso_diz_que_nao_ha() {
    let (_pasta, mut m) = tela("ferramentas-sem-caidas");
    abrir_tres(&mut m);

    m.app.click("menus-ferramentas").click("reconectar-caidas");
    assert_eq!(m.app.text("aviso-info"), "Nenhuma aba caída.");
    m.app.click("menus-ferramentas").click("fechar-caidas");
    assert_eq!(m.app.text("aviso-info"), "Nenhuma aba caída.");
    assert_eq!(chaves(&m.app, 0).len(), 3);
    encerrar(&m);
}

#[test]
fn o_atalho_reconecta_as_caidas() {
    let (_pasta, mut m) = tela("ferramentas-atalho-reconectar");
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);
    m.opener.derrubar(2, "tempo esgotado");
    m.app.frame();
    assert!(caida(&m.app, &chaves[0]));

    m.app.shortcut(tomni_ui::prelude::Shortcut::ctrl_shift('r'));
    m.app.frame();
    assert!(!caida(&m.app, &chaves[0]));
    assert_eq!(m.app.text("aviso-sucesso"), "1 aba reconectada.");
    encerrar(&m);
}

// ---------------------------------------------------------------------------------------
// A dica da aba
// ---------------------------------------------------------------------------------------

#[test]
fn a_dica_da_aba_diz_o_destino_e_acompanha_a_queda() {
    let (_pasta, mut m) = tela("ferramentas-dica");
    m.app.double_click("arvore-2");
    m.app.frame();
    let chave = chaves(&m.app, 0).remove(0);

    let texto = dica(&mut m.app, &chave);
    assert!(texto.contains("SSH · deploy@web-01.exemplo:22"), "{texto}");
    assert!(texto.ends_with("Conectada"), "{texto}");

    m.opener.derrubar(2, "tempo esgotado");
    m.app.frame();
    let texto = dica(&mut m.app, &chave);
    assert!(texto.ends_with("Caiu: tempo esgotado"), "{texto}");

    m.app.click("menus-ferramentas").click("reconectar-caidas");
    m.app.frame();
    let texto = dica(&mut m.app, &chave);
    assert!(texto.ends_with("Conectada"), "{texto}");
    encerrar(&m);
}

#[test]
fn a_dica_mostra_o_salto() {
    let (_pasta, mut m) = tela("ferramentas-dica-salto");
    m.app.double_click("arvore-3");
    m.app.frame();
    let chave = chaves(&m.app, 0).remove(0);
    let texto = dica(&mut m.app, &chave);
    assert!(texto.contains("Salto: ana@bastiao.exemplo:2200"), "{texto}");
    assert!(!texto.contains("Conexão:"), "{texto}");
    encerrar(&m);
}

// ---------------------------------------------------------------------------------------
// Fechar todas
// ---------------------------------------------------------------------------------------

#[test]
fn fechar_todas_pergunta_e_cancelar_nao_fecha() {
    let (_pasta, mut m) = tela("ferramentas-fechar-todas");
    abrir_tres(&mut m);

    m.app.click("menus-ferramentas").click("fechar-todas");
    assert!(m.app.is_open("fechar-todas-dialogo"));
    assert_eq!(
        m.app.text("fechar-todas-texto"),
        "3 sessões ainda estão abertas."
    );
    m.app.click("fechar-todas-dialogo-close");
    assert!(!m.app.is_open("fechar-todas-dialogo"));
    assert_eq!(chaves(&m.app, 0).len(), 3);

    m.app.click("menus-ferramentas").click("fechar-todas");
    m.app.click("fechar-todas-dialogo-primary");
    m.app.frame();
    assert!(!m.app.is_open("fechar-todas-dialogo"));
    assert!(m.app.visible("boas-vindas"));
    assert_eq!(m.app.panes("paineis").panes.len(), 1);
    encerrar(&m);
}

#[test]
fn nao_perguntar_de_novo_desliga_a_configuracao() {
    let (pasta, mut m) = tela("ferramentas-fechar-todas-nao-perguntar");
    abrir_tres(&mut m);

    m.app.click("menus-ferramentas").click("fechar-todas");
    m.app.click("fechar-todas-nao-perguntar");
    m.app.click("fechar-todas-dialogo-primary");
    m.app.frame();
    assert!(m.app.visible("boas-vindas"));
    let gravado = std::fs::read_to_string(pasta.caminho().join("configuracoes.json")).unwrap();
    assert!(
        gravado.contains("\"confirmar_fechar_todas\": false"),
        "{gravado}"
    );

    // Da próxima vez fecha sem perguntar.
    abrir_tres(&mut m);
    m.app.click("menus-ferramentas").click("fechar-todas");
    assert!(!m.app.is_open("fechar-todas-dialogo"));
    assert!(m.app.visible("boas-vindas"));
    encerrar(&m);
}

#[test]
fn so_com_caidas_fecha_sem_perguntar() {
    let (_pasta, mut m) = tela("ferramentas-fechar-todas-caidas");
    m.app.double_click("arvore-2");
    m.app.frame();
    m.opener.derrubar(2, "tempo esgotado");
    m.app.frame();

    m.app.click("menus-ferramentas").click("fechar-todas");
    assert!(!m.app.is_open("fechar-todas-dialogo"));
    assert!(m.app.visible("boas-vindas"));
    encerrar(&m);
}

#[test]
fn o_menu_da_aba_age_so_na_janela_da_aba() {
    let (_pasta, mut m) = tela("ferramentas-menu-da-aba");
    m.app.double_click("arvore-2");
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.set_active_window(outra);
    m.app.double_click("arvore-3");
    m.app.frame();

    m.opener.derrubar(2, "tempo esgotado");
    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();

    // O menu da aba da primeira janela: só a dela é reaberta.
    m.app.switch_to(principal);
    let chave = chaves(&m.app, 0).remove(0);
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("reconectar-caidas-aba");
    m.app.frame();
    assert!(!caida(&m.app, &chave));
    m.app.switch_to(outra);
    let chave_outra = chaves(&m.app, 0).remove(0);
    assert!(caida(&m.app, &chave_outra), "a outra janela foi reaberta");

    // Fechar todas na primeira janela deixa a segunda como está.
    m.app.switch_to(principal);
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("fechar-todas-aba");
    m.app.click("fechar-todas-dialogo-primary");
    m.app.frame();
    assert!(m.app.visible("boas-vindas"));
    m.app.switch_to(outra);
    assert_eq!(chaves(&m.app, 0), [chave_outra]);
    encerrar(&m);
}

// ---------------------------------------------------------------------------------------
// Painel vazio
// ---------------------------------------------------------------------------------------

/// A quantidade de painéis de `paineis`.
fn paineis(app: &TestApp) -> usize {
    app.panes("paineis").panes.len()
}

#[test]
fn o_botao_fecha_o_painel_vazio_sem_fechar_a_sessao_do_outro() {
    let (_pasta, mut m) = tela("ferramentas-painel-vazio");
    m.app.click("menus-exibir").click("layout-2x2");
    assert_eq!(paineis(&m.app), 4);
    // Um painel com aba e os outros vazios: o botão de fechar aparece nos vazios.
    m.app.double_click("arvore-2");
    m.app.frame();
    let com_aba = m
        .app
        .panes("paineis")
        .panes
        .iter()
        .position(|p| !p.tabs.is_empty())
        .unwrap();
    let vazio = (0..4).find(|p| *p != com_aba).unwrap();
    assert!(m.app.visible(&format!("paineis-painel-{vazio}-fechar")));
    assert!(!m.app.visible(&format!("paineis-painel-{com_aba}-fechar")));

    m.app.click(&format!("paineis-painel-{vazio}-fechar"));
    m.app.frame();
    assert_eq!(paineis(&m.app), 3);
    let abas: usize = m
        .app
        .panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.len())
        .sum();
    assert_eq!(abas, 1, "a sessão do outro painel foi fechada");
    encerrar(&m);
}

#[test]
fn com_a_opcao_ligada_o_painel_some_ao_perder_a_ultima_aba() {
    let (pasta, _) = tela("ferramentas-painel-some");
    pasta.configurar("terminal", "fechar_painel_vazio", true.into());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.click("menus-exibir").click("layout-2x2");
    m.app.double_click("arvore-2");
    m.app.frame();
    let com_aba = m
        .app
        .panes("paineis")
        .panes
        .iter()
        .position(|p| !p.tabs.is_empty())
        .unwrap();
    let chave = chaves(&m.app, com_aba).remove(0);
    m.app
        .close_tab(&format!("paineis-painel-{com_aba}"), &chave);
    m.app.frame();
    assert_eq!(paineis(&m.app), 3, "o painel esvaziado não sumiu");
    encerrar(&m);
}

#[test]
fn fechar_todas_volta_a_grade_a_um_painel() {
    let (_pasta, mut m) = tela("ferramentas-grade-volta");
    m.app.click("menus-exibir").click("layout-2x2");
    m.app.double_click("arvore-2");
    m.app.frame();
    m.app.click("menus-ferramentas").click("fechar-todas");
    m.app.click("fechar-todas-dialogo-primary");
    m.app.frame();
    assert_eq!(paineis(&m.app), 1);
    encerrar(&m);
}

// ---------------------------------------------------------------------------------------
// Envio para vários
// ---------------------------------------------------------------------------------------

/// O que a conexão `id` recebeu desde a última chamada (o que o programa dela leria).
fn recebido(m: &Montagem, id: u64) -> String {
    let sessao = m
        .opener
        .sessoes
        .borrow()
        .iter()
        .find(|(_, conexao)| *conexao == id)
        .map(|(s, _)| *s)
        .unwrap_or_else(|| panic!("sem sessão da conexão {id}"));
    let bytes = m.opener.handle_de(sessao).unwrap().take_written();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Abre a barra (Ctrl+Shift+I) e esvazia o que as sessões já tinham recebido.
fn abrir_barra(m: &mut Montagem) {
    m.app.shortcut(tomni_ui::prelude::Shortcut::ctrl_shift('i'));
    m.app.frame();
    for id in [2, 3, 4] {
        let _ = recebido(m, id);
    }
}

/// Escolhe, no diálogo da barra, só as abas `chaves` e fecha o diálogo.
fn escolher_abas(m: &mut Montagem, chaves: &[&str]) {
    m.app.click("multi-abas");
    m.app.grid_select("multi-abas-grade", chaves);
    m.app.click("multi-abas-dialogo-close");
    m.app.frame();
}

#[test]
fn a_barra_abre_com_todas_as_abas_e_envia_para_as_escolhidas() {
    let (_pasta, mut m) = tela("envio-barra");
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);
    assert!(!m.app.visible("multi-entrada"));

    abrir_barra(&mut m);
    assert!(m.app.visible("multi-entrada"));
    assert_eq!(m.app.label("multi-abas"), "3 abas…");
    // Abrir a barra não muda o modo do teclado.
    assert_eq!(m.app.panes("paineis").broadcast, "none");

    escolher_abas(&mut m, &[&chaves[0], &chaves[2]]);
    assert_eq!(m.app.label("multi-abas"), "2 abas…");

    m.app.type_into("multi-texto", "uptime");
    m.app.key(tomni_ui::prelude::Key::Enter);
    m.app.frame();
    assert_eq!(recebido(&m, 2), "uptime\r");
    assert_eq!(recebido(&m, 3), "");
    assert_eq!(recebido(&m, 4), "uptime\r");
    // O campo se limpa e o texto vai para o histórico.
    assert_eq!(m.app.edit("multi-texto").text, "");
    encerrar(&m);
}

#[test]
fn sem_o_enter_marcado_o_texto_vai_sem_o_retorno() {
    let (_pasta, mut m) = tela("envio-sem-enter");
    abrir_tres(&mut m);
    abrir_barra(&mut m);
    m.app.click("multi-enter");
    m.app.type_into("multi-texto", "ls");
    m.app.click("multi-enviar");
    m.app.frame();
    for id in [2, 3, 4] {
        assert_eq!(recebido(&m, id), "ls");
    }
    encerrar(&m);
}

#[test]
fn o_menu_teclas_manda_o_byte_da_tecla() {
    let (_pasta, mut m) = tela("envio-teclas");
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);
    abrir_barra(&mut m);
    escolher_abas(&mut m, &[&chaves[1]]);

    m.app.click("multi-teclas").click("multi-tecla-ctrl-c");
    assert_eq!(recebido(&m, 3), "\x03");
    m.app.click("multi-teclas").click("multi-tecla-tab");
    assert_eq!(recebido(&m, 3), "\t");
    assert_eq!(recebido(&m, 2), "");
    encerrar(&m);
}

#[test]
fn a_aba_caida_no_grupo_nao_recebe_e_o_grupo_vazio_avisa() {
    let (_pasta, mut m) = tela("envio-caida-e-vazio");
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);
    abrir_barra(&mut m);

    m.opener.derrubar(3, "tempo esgotado");
    m.app.frame();
    m.app.type_into("multi-texto", "uptime");
    m.app.key(tomni_ui::prelude::Key::Enter);
    m.app.frame();
    assert_eq!(recebido(&m, 2), "uptime\r");
    assert_eq!(recebido(&m, 3), "");
    assert_eq!(recebido(&m, 4), "uptime\r");

    // Só a aba caída escolhida: ninguém recebe, e a barra diz por quê.
    escolher_abas(&mut m, &[&chaves[1]]);
    m.app.type_into("multi-texto", "x");
    m.app.click("multi-enviar");
    m.app.frame();
    assert_eq!(
        m.app.text("aviso-atencao"),
        "Nenhuma das abas escolhidas está conectada."
    );
    assert_eq!(m.app.edit("multi-texto").text, "x", "o texto fica");

    // Grupo vazio.
    m.app.click("multi-abas").click("multi-abas-nenhuma");
    m.app.click("multi-abas-dialogo-close");
    m.app.click("multi-enviar");
    m.app.frame();
    assert_eq!(
        m.app.text("aviso-atencao"),
        "Escolha as abas que recebem o envio."
    );
    for id in [2, 3, 4] {
        assert_eq!(recebido(&m, id), "");
    }
    encerrar(&m);
}

#[test]
fn o_historico_sugere_o_que_ja_foi_enviado() {
    let (_pasta, mut m) = tela("envio-historico");
    abrir_tres(&mut m);
    abrir_barra(&mut m);
    m.app.type_into("multi-texto", "uptime -p");
    m.app.key(tomni_ui::prelude::Key::Enter);
    m.app.frame();
    m.app.type_into("multi-texto", "upt");
    m.app.frame();
    assert!(m.app.is_open("multi-texto"), "a lista de sugestões abre");
    encerrar(&m);
}

#[test]
fn o_modo_escolhidas_segue_o_menu_da_aba_e_o_dialogo() {
    let (_pasta, mut m) = tela("envio-escolhidas");
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);

    // Marcar a aba no menu dela não liga o modo "Escolhidas".
    for chave in &chaves[..2] {
        m.app.context_menu(&format!("paineis-painel-0-{chave}"));
        m.app.click(&format!("receber-envio-{chave}"));
        m.app.frame();
    }
    assert_eq!(m.app.panes("paineis").broadcast, "none");

    m.app.click("envio-escolhidas");
    m.app.frame();
    assert_eq!(
        m.app.panes("paineis").broadcast,
        format!("group:{},{}", chaves[0], chaves[1])
    );
    assert!(!m.app.checked("envio-nenhum"));

    // Digitar num terminal do grupo chega às duas abas, e à terceira não.
    for id in [2, 3, 4] {
        let _ = recebido(&m, id);
    }
    m.app.click(&format!("paineis-painel-0-{}", chaves[0]));
    m.app.click(&format!("paineis-terminal-{}", chaves[0]));
    m.app.type_text("a");
    assert_eq!(recebido(&m, 2), "a");
    assert_eq!(recebido(&m, 3), "a");
    assert_eq!(recebido(&m, 4), "");

    // Tirar uma pelo diálogo da barra desmarca o item do menu dela.
    m.app.shortcut(tomni_ui::prelude::Shortcut::ctrl_shift('i'));
    escolher_abas(&mut m, &[&chaves[0]]);
    assert_eq!(
        m.app.panes("paineis").broadcast,
        format!("group:{}", chaves[0])
    );
    m.app
        .context_menu(&format!("paineis-painel-0-{}", chaves[1]));
    assert!(!m.app.checked(&format!("receber-envio-{}", chaves[1])));
    encerrar(&m);
}

#[test]
fn fechar_a_aba_tira_a_chave_do_grupo() {
    let (_pasta, mut m) = tela("envio-fechar-aba");
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);
    abrir_barra(&mut m);
    assert_eq!(m.app.label("multi-abas"), "3 abas…");

    m.app.close_tab("paineis-painel-0", &chaves[1]);
    m.app.frame();
    assert_eq!(m.app.label("multi-abas"), "2 abas…");
    encerrar(&m);
}

#[test]
fn o_atalho_do_envio_vale_na_janela_secundaria_e_o_menu_abre_a_da_janela() {
    let (_pasta, mut m) = tela("envio-janela-secundaria");
    m.app.double_click("arvore-2");
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.set_active_window(outra);
    m.app.double_click("arvore-3");
    m.app.frame();

    m.app.switch_to(outra);
    m.app.shortcut(tomni_ui::prelude::Shortcut::ctrl_shift('i'));
    m.app.frame();
    assert!(m.app.visible("multi-entrada"));
    assert_eq!(m.app.label("multi-abas"), "1 aba…");

    // A barra da principal é outra: continua fechada.
    m.app.switch_to(principal);
    assert!(!m.app.visible("multi-entrada"));
    encerrar(&m);
}

// ---------------------------------------------------------------------------------------
// Modo cluster
// ---------------------------------------------------------------------------------------

#[test]
fn o_cluster_de_um_grupo_abre_numa_janela_nova_com_o_envio_em_todos() {
    let (_pasta, mut m) = tela("cluster-grupo");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-cluster");
    m.app.frame();
    assert!(m.app.is_open("cluster-dialogo"));
    // As conexões de terminal do grupo já vêm escolhidas.
    assert_eq!(m.app.text("cluster-contagem"), "2 conexões");
    assert_eq!(m.app.grid("cluster-grade").selected, ["2", "3"]);
    assert!(m.app.enabled("cluster-abrir"));

    m.app.click("cluster-abrir");
    m.app.frame();
    assert!(!m.app.is_open("cluster-dialogo"));
    let [principal, nova] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    assert_eq!(nova.title(), format!("{NOME_DO_APP} — Cluster (2)"));
    m.app.switch_to(nova);
    let paineis = m.app.panes("paineis");
    // Duas conexões ficam numa grade 2 × 1, uma por painel.
    assert_eq!(paineis.panes.len(), 2);
    assert_eq!(
        paineis
            .panes
            .iter()
            .map(|p| p.tabs.len())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    assert_eq!(paineis.broadcast, "all");
    assert!(m.app.checked("envio-todos"));

    // Digitar num terminal chega ao outro, e a janela principal não é tocada.
    let chave = paineis.panes[0].tabs[0].0.clone();
    m.app.click(&format!("paineis-terminal-{chave}"));
    m.app.type_text("x");
    for id in [2, 3] {
        assert_eq!(recebido(&m, id), "x", "conexão {id}");
    }
    m.app.switch_to(principal);
    assert!(chaves(&m.app, 0).is_empty());
    encerrar(&m);
}

#[test]
fn o_cluster_de_uma_conexao_so_traz_ela_e_o_menu_ferramentas_usa_a_selecao() {
    let (_pasta, mut m) = tela("cluster-conexao");
    m.app.context_menu("arvore-3");
    m.app.click("menu-3-cluster");
    m.app.frame();
    assert_eq!(m.app.text("cluster-contagem"), "1 conexão");
    assert_eq!(m.app.grid("cluster-grade").selected, ["3"]);
    m.app.click("cluster-dialogo-close");
    m.app.frame();
    assert!(!m.app.is_open("cluster-dialogo"));

    // Sem seleção na árvore, o menu Ferramentas abre o diálogo sem nada escolhido.
    m.app.click("arvore-5");
    m.app
        .click("menus-ferramentas")
        .click("cluster-modo")
        .click("cluster-menu");
    m.app.frame();
    assert_eq!(m.app.grid("cluster-grade").selected, ["5"]);
    encerrar(&m);
}

#[test]
fn sem_conexao_escolhida_o_abrir_fica_desabilitado_e_o_filtro_estreita_a_grade() {
    let (_pasta, mut m) = tela("cluster-filtro");
    m.app
        .click("menus-ferramentas")
        .click("cluster-modo")
        .click("cluster-menu");
    m.app.frame();
    assert!(m.app.is_open("cluster-dialogo"));
    assert_eq!(m.app.text("cluster-contagem"), "0 conexões");
    assert!(!m.app.enabled("cluster-abrir"));
    // O diálogo mostra só as conexões de terminal.
    let todas = m.app.grid("cluster-grade").keys().len();
    assert!(todas >= 4, "{todas} linhas");

    m.app.type_into("cluster-filtro", "db-01");
    m.app.frame();
    assert_eq!(m.app.grid("cluster-grade").keys(), ["3"]);
    m.app.grid_select("cluster-grade", &["3"]);
    assert!(m.app.enabled("cluster-abrir"));
    assert_eq!(m.app.text("cluster-contagem"), "1 conexão");
    encerrar(&m);
}

#[test]
fn um_cluster_de_uma_conexao_usa_a_grade_1_e_nao_precisa_de_aviso() {
    let (_pasta, mut m) = tela("cluster-uma");
    m.app.context_menu("arvore-2");
    m.app.click("menu-2-cluster");
    m.app.click("cluster-abrir");
    m.app.frame();
    assert_eq!(m.app.windows().len(), 2);
    let nova = m.app.windows()[1];
    assert_eq!(nova.title(), format!("{NOME_DO_APP} — Cluster (1)"));
    m.app.switch_to(nova);
    assert_eq!(m.app.panes("paineis").panes.len(), 1);
    assert_eq!(m.app.panes("paineis").broadcast, "all");
    encerrar(&m);
}
