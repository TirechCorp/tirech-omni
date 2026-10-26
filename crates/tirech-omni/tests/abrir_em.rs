//! "Abrir em" (M36): o submenu que escolhe o painel de uma conexão ou de uma pasta, os números
//! dos painéis enquanto a pessoa escolhe e o "Identificar os painéis" de Exibir.

mod comum;

use std::time::Duration;

use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::prelude::Shortcut;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui::{Key, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar, montar_pura, so_com_senha};

/// Quanto o ponteiro espera sobre um item para o submenu abrir.
const ATRASO_DO_SUBMENU: Duration = Duration::from_millis(450);

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar() {
    for id in 0..=9 {
        for n in 1..=8 {
            if let Some(s) = Session::find(&format!("{id}-{n}")) {
                s.dispose();
            }
        }
    }
}

/// Os títulos das abas de cada painel da janela atual.
fn abas(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()).collect())
        .collect()
}

/// Quantos painéis a janela atual tem.
fn paineis(app: &TestApp) -> usize {
    app.panes("paineis").panes.len()
}

/// A tela com `exemplo()` e o grupo Produção aberto (web-01 é o id 2).
fn tela(nome: &str) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    (pasta, m)
}

/// Abre o menu da conexão `id` e o submenu "Abrir em" com o mouse.
fn abrir_submenu(app: &mut TestApp, id: u64) {
    app.context_menu(&format!("arvore-{id}"));
    app.hover(&format!("menu-{id}-abrir-em"));
    app.wait(ATRASO_DO_SUBMENU);
}

/// Uma pasta (id 1) com três SSH (2, 3 e 4), um S3 (5) e uma subpasta (6) com um Telnet (7).
fn pasta_mista() -> Tree {
    let mut tree = Tree::default();
    let pasta = tree.add_group(None, "Lote");
    for nome in ["a", "b", "c"] {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.host = format!("{nome}.exemplo");
        c.user = "deploy".into();
        c.key_file = Some(format!("/chaves/{nome}").into());
        tree.add_connection(Some(pasta), c);
    }
    let mut s3 = Connection::new("bucket", Kind::S3);
    s3.host = "s3.exemplo".into();
    tree.add_connection(Some(pasta), s3);
    let sub = tree.add_group(Some(pasta), "Dentro");
    let mut telnet = Connection::new("d", Kind::Telnet);
    telnet.host = "d.exemplo".into();
    tree.add_connection(Some(sub), telnet);
    tree
}

#[test]
fn abrir_no_painel_2_da_grade_2x2() {
    let (_pasta, mut m) = tela("abrir-em-painel-2");
    m.app.click("menus-exibir").click("layout-2x2");
    assert_eq!(paineis(&m.app), 4);

    abrir_submenu(&mut m.app, 2);
    for k in 1..=4 {
        assert!(
            m.app.visible(&format!("menu-2-abrir-em-painel-{}", k - 1)),
            "sem o Painel {k}"
        );
    }
    assert!(!m.app.visible("menu-2-abrir-em-painel-4"));
    m.app.click("menu-2-abrir-em-painel-1");

    assert_eq!(
        abas(&m.app),
        [vec![], vec!["web-01".to_owned()], vec![], vec![]]
    );
    let painel = &m.app.panes("paineis").panes[1];
    assert_eq!(painel.selected.as_ref(), Some(&painel.tabs[0].0));
    assert_eq!(m.app.panes("paineis").active, 1);
    assert!(m.app.visible("paineis"), "foi para a página Terminais");
    assert_eq!(m.opener.abertas.borrow().len(), 1);
    encerrar();
}

#[test]
fn o_submenu_mostra_so_os_paineis_que_existem() {
    let (_pasta, mut m) = tela("abrir-em-quantos");
    abrir_submenu(&mut m.app, 2);
    assert!(m.app.visible("menu-2-abrir-em-painel-0"));
    assert!(!m.app.visible("menu-2-abrir-em-painel-1"));
    m.app.key(Key::Escape).key(Key::Escape);

    m.app.click("menus-exibir").click("layout-2x2");
    abrir_submenu(&mut m.app, 2);
    for k in 0..4 {
        assert!(m.app.visible(&format!("menu-2-abrir-em-painel-{k}")));
    }
    assert!(!m.app.visible("menu-2-abrir-em-painel-4"));
    assert_eq!(
        m.app.label("menu-2-abrir-em-painel-2"),
        "Painel 3",
        "o rótulo é o número do painel"
    );
    encerrar();
}

#[test]
fn passar_o_mouse_mostra_os_numeros_e_destaca_o_painel() {
    let (_pasta, mut m) = tela("abrir-em-realce");
    m.app.click("menus-exibir").click("layout-2x2");
    assert!(!m.app.panes("paineis").identifying);

    abrir_submenu(&mut m.app, 2);
    let info = m.app.panes("paineis");
    assert!(info.identifying, "o item \"Abrir em\" já mostra os números");
    assert_eq!(info.highlighted, None);

    m.app.hover("menu-2-abrir-em-painel-2");
    let info = m.app.panes("paineis");
    assert!(info.identifying);
    assert_eq!(info.highlighted, Some(2));

    m.app.hover("menu-2-abrir-em-painel-0");
    assert_eq!(m.app.panes("paineis").highlighted, Some(0));

    m.app.key(Key::Escape);
    m.app.key(Key::Escape);
    let info = m.app.panes("paineis");
    assert!(!info.identifying, "fechar o menu apaga os números");
    assert_eq!(info.highlighted, None);
    assert_eq!(
        abas(&m.app),
        [const { Vec::<String>::new() }; 4],
        "nada foi aberto"
    );
    encerrar();
}

#[test]
fn pelo_teclado_com_as_setas_e_com_o_numero() {
    let (_pasta, mut m) = tela("abrir-em-teclado");
    m.app.click("menus-exibir").click("layout-2x2");

    // Setas: o foco começa em "Abrir"; ↓ vai a "Abrir em", → entra no submenu e ↓ escolhe.
    m.app.context_menu("arvore-2");
    m.app.key(Key::Down);
    assert!(m.app.panes("paineis").identifying);
    m.app.key(Key::Right);
    assert_eq!(m.app.panes("paineis").highlighted, Some(0));
    m.app.key(Key::Down);
    assert!(m.app.panes("paineis").identifying);
    assert_eq!(m.app.panes("paineis").highlighted, Some(1));
    m.app.key(Key::Enter);
    assert_eq!(
        abas(&m.app),
        [vec![], vec!["web-01".to_owned()], vec![], vec![]]
    );

    // Tecla de acesso: o 3 escolhe o Painel 3.
    m.app.context_menu("arvore-3");
    m.app.hover("menu-3-abrir-em");
    m.app.wait(ATRASO_DO_SUBMENU);
    m.app.key(Key::Right);
    m.app.key(Key::Char('3'));
    assert_eq!(
        abas(&m.app),
        [
            vec![],
            vec!["web-01".to_owned()],
            vec!["db-01".to_owned()],
            vec![]
        ]
    );
    encerrar();
}

#[test]
fn novo_painel_a_direita_divide_e_abre_no_novo() {
    let (_pasta, mut m) = tela("abrir-em-direita");
    assert_eq!(paineis(&m.app), 1);
    abrir_submenu(&mut m.app, 2);
    m.app.hover("menu-2-abrir-em-direita");
    assert!(m.app.panes("paineis").identifying);
    m.app.click("menu-2-abrir-em-direita");
    assert_eq!(paineis(&m.app), 2);
    assert_eq!(abas(&m.app), [vec![], vec!["web-01".to_owned()]]);
    assert!(!m.app.panes("paineis").identifying);
    encerrar();
}

#[test]
fn nova_janela_abre_a_conexao_numa_janela_nova() {
    let (_pasta, mut m) = tela("abrir-em-nova-janela");
    assert_eq!(m.app.windows().len(), 1);
    abrir_submenu(&mut m.app, 2);
    m.app.click("menu-2-abrir-em-nova");
    m.app.frame();
    let janelas = m.app.windows();
    assert_eq!(janelas.len(), 2);
    assert_eq!(abas(&m.app), [Vec::<String>::new()], "a principal ficou");
    m.app.switch_to(janelas[1]);
    assert_eq!(abas(&m.app), [["web-01"]]);
    encerrar();
}

#[test]
fn outra_janela_so_aparece_com_mais_de_uma_e_abre_na_escolhida() {
    let (_pasta, mut m) = tela("abrir-em-outra");
    abrir_submenu(&mut m.app, 2);
    assert!(!m.app.visible("menu-2-abrir-em-outra"), "só há uma janela");
    m.app.key(Key::Escape).key(Key::Escape);

    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(principal);
    m.app.set_active_window(principal);
    abrir_submenu(&mut m.app, 2);
    // A janela de destino é a principal: só a outra aparece, como "Janela 2".
    m.app.hover("menu-2-abrir-em-outra");
    m.app.wait(ATRASO_DO_SUBMENU);
    assert!(!m.app.visible("menu-2-abrir-em-janela-0"));
    assert!(m.app.visible("menu-2-abrir-em-janela-1"));
    m.app.hover("menu-2-abrir-em-janela-1");
    m.app.click("menu-2-abrir-em-janela-1");
    m.app.frame();
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);
    m.app.switch_to(outra);
    assert_eq!(abas(&m.app), [["web-01"]]);
    encerrar();
}

#[test]
fn com_a_janela_2_ativa_o_submenu_mostra_os_paineis_dela() {
    let (_pasta, mut m) = tela("abrir-em-ativa");
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(outra);
    m.app.click("menus-exibir").click("layout-2x2");
    assert_eq!(paineis(&m.app), 4);
    m.app.set_active_window(outra);

    m.app.switch_to(principal);
    abrir_submenu(&mut m.app, 2);
    for k in 0..4 {
        assert!(
            m.app.visible(&format!("menu-2-abrir-em-painel-{k}")),
            "sem o painel {k} da janela ativa"
        );
    }
    m.app.hover("menu-2-abrir-em-painel-3");
    m.app.click("menu-2-abrir-em-painel-3");
    m.app.frame();
    m.app.switch_to(outra);
    assert_eq!(
        abas(&m.app),
        [vec![], vec![], vec![], vec!["web-01".to_owned()]]
    );
    encerrar();
}

#[test]
fn abrir_numa_pasta_abre_as_conexoes_de_terminal() {
    let pasta = Pasta::nova("abrir-em-pasta");
    pasta.semear(&pasta_mista());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");

    m.app.context_menu("arvore-1");
    m.app.click("menu-1-abrir");
    assert_eq!(abas(&m.app), [["a", "b", "c", "d"]]);
    assert_eq!(m.opener.abertas.borrow().len(), 4, "o S3 ficou de fora");
    encerrar();
}

#[test]
fn abrir_todas_em_poe_a_pasta_inteira_no_painel_escolhido() {
    let pasta = Pasta::nova("abrir-em-pasta-todas");
    pasta.semear(&pasta_mista());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.click("menus-exibir").click("layout-2x2");

    m.app.context_menu("arvore-1");
    assert_eq!(m.app.label("menu-1-todas-abrir-em"), "Abrir todas em");
    m.app.hover("menu-1-todas-abrir-em");
    m.app.wait(ATRASO_DO_SUBMENU);
    m.app.hover("menu-1-todas-abrir-em-painel-1");
    assert_eq!(m.app.panes("paineis").highlighted, Some(1));
    m.app.click("menu-1-todas-abrir-em-painel-1");
    assert_eq!(
        abas(&m.app),
        [
            vec![],
            vec!["a".to_owned(), "b".into(), "c".into(), "d".into()],
            vec![],
            vec![]
        ]
    );
    encerrar();
}

#[test]
fn conexao_so_de_arquivos_nao_tem_abrir_em() {
    let pasta = Pasta::nova("abrir-em-s3");
    pasta.semear(&pasta_mista());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.context_menu("arvore-5");
    assert!(m.app.visible("menu-5-renomear"), "o menu abre");
    assert!(!m.app.visible("menu-5-arquivos"), "o S3 não usa o SSH");
    assert!(!m.app.visible("menu-5-abrir-em"));
    encerrar();
}

#[test]
fn o_menu_de_uma_conexao_sem_ssh_tambem_abre() {
    let (_pasta, mut m) = tela("abrir-em-telnet");
    m.app.context_menu("arvore-4");
    assert!(m.app.visible("menu-4-abrir"));
    assert!(m.app.visible("menu-4-abrir-em"));
    assert!(m.app.visible("menu-4-renomear"));
    assert!(!m.app.visible("menu-4-arquivos"), "o Telnet não tem SFTP");
    assert!(!m.app.visible("menu-4-copiar-chave"));
    encerrar();
}

#[test]
fn o_cartao_do_inicio_tambem_abre_em_um_painel() {
    let pasta = Pasta::nova("abrir-em-inicio");
    pasta.semear(&exemplo());
    {
        // Uma execução anterior abre web-01 para ele virar recente.
        let mut m = montar(&pasta, Theme::Light);
        m.app.expand("arvore", "1");
        m.app.double_click("arvore-2");
        assert_eq!(abas(&m.app), [["web-01"]]);
    }
    encerrar();
    std::fs::remove_file(pasta.armazenamento().arquivo_estado()).ok();
    std::fs::write(
        pasta.armazenamento().arquivo_configuracoes(),
        r#"{"pagina_inicial": "inicio"}"#,
    )
    .expect("gravar as configurações");
    let mut app = montar_pura(&pasta, Theme::Light).app;
    app.click("navegacao-terminais");
    app.click("menus-exibir").click("layout-2x2");
    app.click("navegacao-inicio");

    app.right_click("inicio-recente-0");
    app.hover("inicio-recente-0-menu-abrir-em");
    app.wait(ATRASO_DO_SUBMENU);
    app.hover("inicio-recente-0-menu-abrir-em-painel-1");
    assert_eq!(app.panes("paineis").highlighted, Some(1));
    app.click("inicio-recente-0-menu-abrir-em-painel-1");
    assert_eq!(
        abas(&app),
        [vec![], vec!["web-01".to_owned()], vec![], vec![]]
    );
    assert!(app.visible("paineis"), "foi para os Terminais");
    encerrar();
}

#[test]
fn a_conexao_que_espera_a_senha_entra_no_painel_escolhido() {
    // A senha pendente guarda o painel escolhido até o diálogo fechar.
    let pasta = Pasta::nova("abrir-em-sumiu");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-exibir").click("layout-2x2");
    abrir_submenu(&mut m.app, 1);
    m.app.click("menu-1-abrir-em-painel-3");
    assert!(m.app.is_open("senha-dialogo"));

    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-primary");
    assert_eq!(paineis(&m.app), 4);
    assert_eq!(abas(&m.app)[3], ["sem-chave"]);
    encerrar();
}

/// Espera `tempo` de verdade (o alarme do "Identificar" é uma thread) e roda um quadro.
fn esperar(app: &mut TestApp, tempo: Duration) {
    std::thread::sleep(tempo);
    app.frame();
}

#[test]
fn identificar_os_paineis_pelo_menu_e_pelo_atalho() {
    let (_pasta, mut m) = tela("identificar");
    m.app.click("menus-exibir").click("layout-2x2");
    assert!(!m.app.panes("paineis").identifying);

    m.app.click("menus-exibir").click("identificar");
    assert!(m.app.panes("paineis").identifying);
    assert_eq!(m.app.panes("paineis").highlighted, None);

    // O atalho liga de novo: o segundo pedido estende o prazo e o alarme do primeiro não
    // desliga nada.
    esperar(&mut m.app, Duration::from_millis(1200));
    assert!(m.app.panes("paineis").identifying);
    m.app
        .shortcut(Shortcut::new('I', Modifiers::CTRL | Modifiers::ALT));
    esperar(&mut m.app, Duration::from_millis(1200));
    assert!(
        m.app.panes("paineis").identifying,
        "o primeiro alarme (2 s) já passou, mas o pedido novo vale até 3,2 s"
    );
    esperar(&mut m.app, Duration::from_millis(1200));
    assert!(
        !m.app.panes("paineis").identifying,
        "passaram 2 s do último"
    );
    encerrar();
}

#[test]
fn o_atalho_escolhido_vale_e_o_de_fabrica_deixa_de_valer() {
    let pasta = Pasta::nova("identificar-atalho");
    pasta.semear(&exemplo());
    std::fs::create_dir_all(pasta.caminho()).expect("criar a pasta do teste");
    std::fs::write(
        pasta.armazenamento().arquivo_configuracoes(),
        r#"{"pagina_inicial": "terminais", "atalhos": {"identificar_paineis": ["Ctrl+Alt+J"]}}"#,
    )
    .expect("gravar as configurações");
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("menus-exibir").click("layout-2x2");

    // O atalho de fábrica deixou de valer.
    m.app
        .shortcut(Shortcut::new('I', Modifiers::CTRL | Modifiers::ALT));
    assert!(!m.app.panes("paineis").identifying);
    // O escolhido vale.
    m.app
        .shortcut(Shortcut::new('J', Modifiers::CTRL | Modifiers::ALT));
    assert!(m.app.panes("paineis").identifying);
    encerrar();
}
