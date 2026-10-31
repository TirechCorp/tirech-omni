//! Testes da tela do `tirech-omni`, com sessões `Loopback` no lugar das conexões de verdade.

mod comum;

use tirech_omni::abrir::Senhas;
use tirech_omni::modelo::{Ativar, Kind, Node};
use tomni_ui::Key;
use tomni_ui::prelude::Shortcut;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, editar_propriedade, escolher_propriedade, exemplo, montar, valor};

/// As linhas das conexões; o grupo fixo dos espaços de trabalho (o último) fica de fora.
fn rotulos(app: &TestApp) -> Vec<(usize, String)> {
    app.tree_items("arvore")
        .into_iter()
        .take_while(|r| r.key != "espaco:raiz")
        .map(|r| (r.depth, r.label))
        .collect()
}

fn abas(app: &TestApp) -> Vec<Vec<(String, String)>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.clone())
        .collect()
}

#[test]
fn tela_inicial_sem_conexoes_mostra_a_dica_e_os_menus() {
    let pasta = Pasta::nova("inicial");
    let mut m = montar(&pasta, Theme::Light);
    assert!(m.app.visible("dica-vazia"));
    assert_eq!(
        m.app.text("dica-vazia"),
        "Nenhuma conexão. Use Arquivo > Nova conexão"
    );
    assert!(m.app.visible("boas-vindas"));
    // Só o grupo fixo dos espaços de trabalho.
    let linhas = m.app.tree_items("arvore");
    assert_eq!(linhas.len(), 1);
    assert_eq!(linhas[0].label, "Espaços de trabalho");
    for menu in ["arquivo", "exibir", "ferramentas", "ajuda"] {
        assert!(m.app.visible(&format!("menus-{menu}")), "sem o menu {menu}");
    }
    m.app.assert_lint();
}

#[test]
fn a_arvore_mostra_grupos_e_conexoes_com_usuario_e_host() {
    let pasta = Pasta::nova("arvore");
    pasta.semear(&exemplo());
    let m = montar(&pasta, Theme::Light);
    assert!(!m.app.visible("dica-vazia"));
    assert_eq!(
        rotulos(&m.app),
        [
            (0, "Produção".to_owned()),
            (0, "Roteador (192.168.0.1)".to_owned()),
            (0, "Shell".to_owned()),
        ],
        "o grupo começa fechado"
    );
    let mut m = m;
    m.app.expand("arvore", "1");
    let r = rotulos(&m.app);
    assert_eq!(r[1], (1, "web-01 (deploy@web-01.exemplo)".to_owned()));
    assert_eq!(r[2], (1, "db-01 (dba@10.0.0.20)".to_owned()));
}

#[test]
fn nova_conexao_e_novo_grupo_entram_na_arvore_e_no_arquivo() {
    let pasta = Pasta::nova("criar");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("nova-conexao");
    assert!(!m.app.visible("dica-vazia"));
    assert_eq!(rotulos(&m.app), [(0, "Nova conexão 1".to_owned())]);
    assert_eq!(valor(&m.app, "Nome"), "Nova conexão 1");
    assert_eq!(pasta.conexoes().connection_count(), 1);

    // Ctrl+Shift+G cria o grupo (e já o põe em renomeação).
    m.app.shortcut(Shortcut::ctrl_shift('G'));
    let nodes = pasta.conexoes().nodes;
    assert!(matches!(&nodes[1], Node::Group(g) if g.name == "Novo grupo"));

    // Com o grupo escolhido, a conexão nova entra nele.
    m.app.key(Key::Escape);
    m.app.shortcut(Shortcut::ctrl_shift('N'));
    let tree = pasta.conexoes();
    let Node::Group(g) = &tree.nodes[1] else {
        panic!("o segundo item deveria ser o grupo");
    };
    assert_eq!(g.children.len(), 1);
    assert_eq!(g.children[0].name(), "Nova conexão 2");
}

#[test]
fn subir_descer_e_apagar_com_confirmacao() {
    let pasta = Pasta::nova("ordem");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-5").click("subir");
    let nomes: Vec<String> = pasta
        .conexoes()
        .nodes
        .iter()
        .map(|n| n.name().to_owned())
        .collect();
    assert_eq!(nomes, ["Produção", "Shell", "Roteador"]);
    m.app.click("descer");
    let nomes: Vec<String> = pasta
        .conexoes()
        .nodes
        .iter()
        .map(|n| n.name().to_owned())
        .collect();
    assert_eq!(nomes, ["Produção", "Roteador", "Shell"]);

    m.app.click("apagar");
    assert!(m.app.is_open("apagar-dialogo"));
    m.app.click("apagar-dialogo-close");
    assert_eq!(pasta.conexoes().connection_count(), 4, "cancelar não apaga");
    m.app.click("apagar").click("apagar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 3);
    assert!(pasta.conexoes().nodes.iter().all(|n| n.name() != "Shell"));
    m.app.assert_lint();
}

#[test]
fn o_filtro_acha_pelo_host_e_mostra_os_ancestrais() {
    let pasta = Pasta::nova("filtro");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("filtrar");
    m.app.type_into("filtro", "10.0.0.20");
    let r = rotulos(&m.app);
    assert_eq!(r.len(), 2, "{r:?}");
    assert_eq!(r[0].1, "Produção");
    assert!(r[1].1.starts_with("db-01"));
}

#[test]
fn a_lupa_abre_o_filtro_com_o_cursor_e_o_esc_fecha_e_limpa() {
    let pasta = Pasta::nova("lupa");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.visible("filtro"), "o campo começa escondido");
    let inteira = rotulos(&m.app);

    m.app.click("filtrar");
    m.app.wait(std::time::Duration::from_millis(100));
    assert!(m.app.visible("filtro"));
    assert_eq!(m.app.focused().as_deref(), Some("filtro"));

    // Pelo usuário.
    m.app.type_text("deploy");
    let r = rotulos(&m.app);
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(r[1].1.starts_with("web-01"), "{r:?}");

    // Pelo servidor de salto, que não está no rótulo.
    m.app.key_with(Key::Char('a'), tomni_ui::Modifiers::COMMAND);
    m.app.type_text("bastiao");
    let r = rotulos(&m.app);
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(r[1].1.starts_with("db-01"), "{r:?}");

    // O Esc fecha o campo, limpa o filtro e devolve a árvore inteira.
    m.app.key(Key::Escape);
    assert!(!m.app.visible("filtro"));
    assert_eq!(m.app.text("filtro"), "");
    assert_eq!(rotulos(&m.app), inteira);

    // Desligar a lupa também limpa.
    m.app.click("filtrar").type_into("filtro", "roteador");
    assert_eq!(rotulos(&m.app).len(), 1);
    m.app.click("filtrar");
    assert!(!m.app.visible("filtro"));
    assert_eq!(rotulos(&m.app), inteira);
    m.app.assert_lint();
}

#[test]
fn a_descricao_grava_entra_na_busca_e_aparece_na_dica() {
    let pasta = Pasta::nova("descricao");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").click("arvore-2");
    assert_eq!(valor(&m.app, "Descrição"), "");
    editar_propriedade(&mut m.app, "Descrição", "servidor do abacate");
    assert_eq!(
        pasta.conexoes().connection(2).unwrap().descricao,
        "servidor do abacate"
    );

    // A dica da linha mostra a descrição.
    m.app.hover("arvore-2");
    m.app
        .wait(tomni_ui::TOOLTIP_DELAY + std::time::Duration::from_millis(10));
    let dica = m
        .app
        .tree()
        .find("arvore-2-tooltip")
        .and_then(|n| n.children.first().and_then(|t| t.text.clone()));
    assert_eq!(dica.as_deref(), Some("servidor do abacate"));

    // O filtro acha a conexão pelo texto da descrição, que não está no rótulo.
    m.app.click("filtrar");
    m.app.type_into("filtro", "abacate");
    let r = rotulos(&m.app);
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(r[1].1.starts_with("web-01"), "{r:?}");
}

#[test]
fn editar_as_propriedades_grava_no_arquivo_na_hora() {
    let pasta = Pasta::nova("propriedades");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").click("arvore-2");
    assert!(m.app.visible("props"));
    assert_eq!(valor(&m.app, "Host"), "web-01.exemplo");
    assert_eq!(valor(&m.app, "Usuário"), "deploy");
    assert_eq!(valor(&m.app, "Host do jump"), "");

    editar_propriedade(&mut m.app, "Host", "10.1.2.3");
    editar_propriedade(&mut m.app, "Porta", "2222");
    editar_propriedade(&mut m.app, "Host do jump", "salto.exemplo");
    editar_propriedade(&mut m.app, "Nome", "web-novo");

    let c = pasta.conexoes().connection(2).cloned().unwrap();
    assert_eq!(c.host, "10.1.2.3");
    assert_eq!(c.port, 2222);
    assert_eq!(c.name, "web-novo");
    assert_eq!(
        c.jump.as_ref().map(|j| j.host.as_str()),
        Some("salto.exemplo")
    );
    // O rótulo da árvore acompanha.
    assert!(
        rotulos(&m.app)
            .iter()
            .any(|(_, r)| r == "web-novo (deploy@10.1.2.3)")
    );

    // Trocar a escolha mostra os valores da outra, sem sujar nenhuma das duas.
    m.app.click("arvore-3");
    assert_eq!(valor(&m.app, "Host"), "10.0.0.20");
    assert_eq!(valor(&m.app, "Host do jump"), "bastiao.exemplo");
    assert_eq!(valor(&m.app, "Porta do jump"), "2200");
    let db = pasta.conexoes().connection(3).cloned().unwrap();
    assert_eq!(db, exemplo().connection(3).cloned().unwrap());

    // Um grupo só tem as linhas Rede, Assistentes (MCP), Abrir abas novas, Sessão persistente,
    // Mouse no terminal, Tentar de novo após, Fechar sem uso após, Credencial, Chave do cofre,
    // Chave privada e Comandos.
    m.app.click("arvore-1");
    assert!(!m.app.visible("props"));
    assert!(m.app.visible("props-grupo"));
    assert!(!m.app.visible("sem-propriedades"));
    let linhas = m.app.properties("props-grupo");
    let nomes: Vec<_> = linhas.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(
        nomes,
        [
            "Rede",
            "Assistentes (MCP)",
            "Abrir abas novas",
            "Sessão persistente",
            "Mouse no terminal",
            "Tentar de novo após",
            "Fechar sem uso após",
            "Credencial",
            "Chave do cofre",
            "Chave privada",
            "Comandos"
        ]
    );
}

#[test]
fn o_tipo_muda_a_porta_padrao_e_esconde_o_que_nao_vale() {
    let pasta = Pasta::nova("tipo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-5");
    let props = m.app.properties("props");
    let linha = |nome: &str| props.iter().find(|p| p.name == nome).unwrap();
    let visivel = |nome: &str| !linha(nome).hidden;
    assert!(!visivel("Host") && !visivel("Porta") && !visivel("Usuário"));
    assert!(!visivel("Chave privada") && !visivel("Host do jump"));
    assert!(visivel("Nome") && visivel("Log") && linha("Log").enabled);

    // SSH com a porta padrão vira Telnet com a porta 23.
    m.app.expand("arvore", "1").click("arvore-3");
    escolher_propriedade(&mut m.app, "Tipo");
    m.app.select("props-Tipo-valor", "Telnet");
    let c = pasta.conexoes().connection(3).cloned().unwrap();
    assert_eq!(c.kind, Kind::Telnet);
    assert_eq!(c.port, 23);
    assert_eq!(valor(&m.app, "Porta"), "23");
}

#[test]
fn clique_duplo_abre_uma_aba_e_outro_abre_a_segunda() {
    let pasta = Pasta::nova("abas");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    assert!(m.app.visible("boas-vindas"));
    m.app.expand("arvore", "1").double_click("arvore-2");
    assert!(!m.app.visible("boas-vindas"));
    assert_eq!(abas(&m.app), [[("2-1".to_owned(), "web-01".to_owned())]]);

    m.app.double_click("arvore-2");
    m.app.click("arvore-3").key(Key::Enter);
    let titulos: Vec<String> = abas(&m.app)[0].iter().map(|(_, t)| t.clone()).collect();
    assert_eq!(titulos, ["web-01", "web-01", "db-01"]);
    assert_eq!(
        m.app.panes("paineis").panes[0].selected.as_deref(),
        Some("3-3")
    );
    assert_eq!(*m.opener.abertas.borrow(), [2, 2, 3]);
    assert_eq!(
        *m.opener.senhas.borrow(),
        [Senhas::default(), Senhas::default(), Senhas::default()]
    );

    // O que se digita vai para a sessão (o Loopback devolve o eco).
    m.app.type_into("paineis", "uptime\n");
    assert!(m.app.session_text("3-3").contains("uptime"));
    m.app.assert_lint();
}

#[test]
fn clique_duplo_num_grupo_nao_abre_sessao() {
    let pasta = Pasta::nova("grupo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(m.opener.abertas.borrow().is_empty());
    assert!(m.app.visible("boas-vindas"));
}

#[test]
fn abrir_shell_local_pelo_atalho_e_pelo_menu() {
    let pasta = Pasta::nova("shell");
    let mut m = montar(&pasta, Theme::Light);
    m.app.shortcut(Shortcut::ctrl_shift('T'));
    m.app.click("menus-ferramentas").click("abrir-shell");
    assert_eq!(abas(&m.app)[0].len(), 2);
    assert_eq!(abas(&m.app)[0][0].1, "Shell local");
}

#[test]
fn layouts_1_2x2_e_3x3_pelo_menu_e_pelo_atalho() {
    let pasta = Pasta::nova("layouts");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-4");
    assert_eq!(m.app.panes("paineis").panes.len(), 1);
    m.app.click("menus-exibir").click("layout-2x2");
    assert_eq!(m.app.panes("paineis").panes.len(), 4);
    m.app.click("menus-exibir").click("layout-3x3");
    assert_eq!(m.app.panes("paineis").panes.len(), 9);
    m.app.click("menus-exibir").click("layout-1");
    assert_eq!(m.app.panes("paineis").panes.len(), 1);
    assert_eq!(
        m.app.panes("paineis").panes[0].tabs.len(),
        1,
        "as abas não se perdem"
    );
    m.app.shortcut(tomni_ui::prelude::Shortcut::new(
        '2',
        tomni_ui::Modifiers::CTRL | tomni_ui::Modifiers::ALT,
    ));
    assert_eq!(m.app.panes("paineis").panes.len(), 4);
}

#[test]
fn os_botoes_de_envio_escolhem_um_destino_de_cada_vez() {
    let pasta = Pasta::nova("envio");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-exibir").click("layout-2x2");
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2").double_click("arvore-3");
    assert!(m.app.checked("envio-nenhum"));

    m.app.click("envio-todos");
    assert!(m.app.checked("envio-todos") && !m.app.checked("envio-nenhum"));
    m.app.click("envio-visiveis");
    assert!(m.app.checked("envio-visiveis") && !m.app.checked("envio-todos"));
    m.app.click("envio-visiveis");
    assert!(
        m.app.checked("envio-visiveis"),
        "clicar no escolhido mantém"
    );

    // Com "Todos", o que se digita numa sessão chega na outra.
    m.app.click("envio-todos");
    let terminal = format!("paineis-terminal-{}", "2-1");
    m.app.type_into(&terminal, "ls\n");
    assert!(m.app.session_text("2-1").contains("ls"));
    assert!(m.app.session_text("3-2").contains("ls"));

    m.app.click("envio-nenhum");
    m.app.type_into(&terminal, "pwd\n");
    assert!(m.app.session_text("2-1").contains("pwd"));
    assert!(!m.app.session_text("3-2").contains("pwd"));
}

#[test]
fn a_area_dos_terminais_ocupa_85_por_cento_da_altura() {
    for tema in [Theme::Light, Theme::Dark] {
        let pasta = Pasta::nova("altura");
        pasta.semear(&exemplo());
        let mut m = montar(&pasta, tema);
        m.app.double_click("arvore-4");
        let terminal = m.app.rect("paineis-terminal-4-1");
        let barra = m.app.rect("barra-de-titulo");
        assert!(
            terminal.height() >= 0.85 * 720.0,
            "o terminal tem {} px de altura, menos de 85% de 720",
            terminal.height()
        );
        // M26: a barra de título leva a busca e os menus descem para a página Terminais.
        assert!(terminal.y0 <= 112.0, "o terminal começa em {}", terminal.y0);
        assert!(
            m.app.rect("menus").y1 <= terminal.y0,
            "o menu fica acima do terminal"
        );
        assert!(
            m.app.rect("menus").y0 >= barra.y1,
            "o menu fica logo abaixo da barra de título"
        );
        m.app.assert_lint();
        if let Some(s) = tomni_ui::terminal::Session::find("4-1") {
            s.dispose();
        }
    }
}

#[test]
fn propriedades_da_conexao_gravam_as_pastas() {
    let pasta = Pasta::nova("propriedades-pastas");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").click("arvore-2");
    assert_eq!(valor(&m.app, "Pasta local padrão"), "");
    assert_eq!(valor(&m.app, "Comparar pastas"), "Padrão");

    editar_propriedade(&mut m.app, "Pasta remota padrão", "/srv/www");

    // O "…" da pasta local pede uma pasta ao sistema e grava a resposta.
    escolher_propriedade(&mut m.app, "Pasta local padrão");
    m.app.click("props-Pasta local padrão-procurar");
    let pedidos = m.app.file_dialogs();
    assert_eq!(pedidos.len(), 1);
    assert_eq!(pedidos[0].kind, tomni_ui::dialogs::FileDialogKind::Folder);
    m.app.file_dialog_answer(Some("/home/deploy/www"));

    let c = pasta.conexoes().connection(2).cloned().unwrap();
    assert_eq!(c.pasta_local, "/home/deploy/www");
    assert_eq!(c.pasta_remota, "/srv/www");
    assert_eq!(valor(&m.app, "Pasta local padrão"), "/home/deploy/www");
}

/// Troca uma das linhas Padrão/Ligado/Desligado numa tela nova (depois de um seletor o foco
/// fica nele) e devolve a conexão 2 gravada.
fn trocar_ativar(nome: &str, valor: &str) -> tirech_omni::modelo::Connection {
    let pasta = Pasta::nova(&format!("propriedades-{nome}").replace(' ', "-"));
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").click("arvore-2");
    escolher_propriedade(&mut m.app, nome);
    m.app.select(&format!("props-{nome}-valor"), valor);
    pasta.conexoes().connection(2).cloned().unwrap()
}

#[test]
fn comparar_e_sincronizar_da_conexao_gravam_ligado_e_desligado() {
    let c = trocar_ativar("Comparar pastas", "Ligado");
    assert_eq!((c.comparar, c.sincronizar), (Ativar::Sim, Ativar::Padrao));
    let c = trocar_ativar("Sincronizar pastas", "Desligado");
    assert_eq!((c.comparar, c.sincronizar), (Ativar::Padrao, Ativar::Nao));
}

#[test]
fn as_linhas_de_arquivos_ficam_escondidas_sem_a_pagina_arquivos() {
    let pasta = Pasta::nova("propriedades-pastas-sem");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    // O Roteador (Telnet) não abre a página Arquivos; o web-01 (SSH) abre.
    m.app.click("arvore-4");
    let props = m.app.properties("props");
    for nome in [
        "Pasta local padrão",
        "Pasta remota padrão",
        "Comparar pastas",
        "Sincronizar pastas",
    ] {
        let linha = props.iter().find(|p| p.name == nome).unwrap();
        assert!(linha.hidden, "{nome} deveria estar escondida");
    }
    m.app.expand("arvore", "1").click("arvore-2");
    let props = m.app.properties("props");
    assert!(
        props
            .iter()
            .find(|p| p.name == "Pasta local padrão")
            .unwrap()
            .enabled
    );
}
