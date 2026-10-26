//! A página Arquivos: abrir, navegar, transferir, conflitos, apagar, propriedades, recentes,
//! os arquivos de uma sessão e o estado. O servidor é um `MemFs` e o disco também.

mod comum;

use std::rc::Rc;
use std::time::{Duration, Instant};

use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::Key;
use tomni_ui::ProgressStatus;
use tomni_ui::files::{Fs, MemFs};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

/// Duas conexões: 1 `servidor` (SFTP com chave) e 2 `web` (SSH com chave).
fn arvore() -> Tree {
    let mut tree = Tree::default();
    let mut sftp = Connection::new("servidor", Kind::Sftp);
    sftp.host = "arquivos.exemplo".into();
    sftp.user = "ana".into();
    sftp.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, sftp);
    let mut web = Connection::new("web", Kind::Ssh);
    web.host = "web.exemplo".into();
    web.user = "ana".into();
    web.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, web);
    tree
}

/// Como [`arvore`], com `ajustar` aplicado à conexão 1 (as pastas padrão, por exemplo).
fn arvore_com(ajustar: impl FnOnce(&mut Connection)) -> Tree {
    let mut tree = arvore();
    ajustar(tree.connection_mut(1).expect("a conexão 1"));
    tree
}

/// Espera `pronto` ficar verdadeiro, deixando o app processar as respostas das threads.
fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// As chaves das linhas de um painel.
fn chaves(app: &TestApp, id: &str) -> Vec<String> {
    app.grid(id).keys().into_iter().map(str::to_owned).collect()
}

/// Apaga o texto do campo `id` e digita `texto`.
fn escrever(app: &mut TestApp, id: &str, texto: &str) {
    app.click(id).key(Key::End);
    for _ in 0..app.text(id).chars().count() + 4 {
        app.key(Key::Backspace);
    }
    app.type_text(texto);
}

/// Espera o servidor `fs` ter `caminho`.
fn esperar_arquivo(app: &mut TestApp, fs: &MemFs, caminho: &str) {
    esperar(app, caminho, |_| fs.exists(caminho));
}

fn montar_arquivos(nome: &str) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore());
    let m = montar(&pasta, Theme::Light);
    (pasta, m)
}

#[test]
fn abrir_uma_conexao_sftp_leva_a_pagina_arquivos() {
    let (_pasta, mut m) = montar_arquivos("arquivos-abrir");
    m.opener.servidor(1).put("/home/ana/leia.txt", "oi");
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a listagem remota", |a| {
        a.grid("a1-remoto").keys().contains(&"leia.txt")
    });
    assert!(m.app.visible("arquivos-abas"));
    assert_eq!(m.app.tab_titles("arquivos-abas"), ["servidor"]);
    assert!(
        m.opener.abertas.borrow().is_empty(),
        "não abriu um terminal"
    );
    m.app.assert_lint();
}

/// Abre os arquivos da conexão 1 e espera a listagem dos dois painéis.
fn abrir_servidor(m: &mut Montagem) {
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "os dois painéis", |a| {
        !a.grid("a1-local").keys().is_empty() && !a.grid("a1-remoto").keys().is_empty()
    });
    m.app.wait(Duration::from_millis(100));
}

#[test]
fn navega_por_clique_duplo_acima_e_caminho_digitado() {
    let (_pasta, mut m) = montar_arquivos("arquivos-navegar");
    let servidor = m.opener.servidor(1);
    servidor
        .dir("/home/ana/docs")
        .put("/home/ana/docs/nota.txt", "n");
    servidor.put("/home/ana/leia.txt", "oi");
    abrir_servidor(&mut m);
    assert_eq!(m.app.text("navegacao"), "arquivos");
    assert_eq!(chaves(&m.app, "a1-remoto"), ["..", "docs", "leia.txt"]);

    m.app.double_click("a1-remoto-docs");
    esperar(&mut m.app, "a pasta docs", |a| {
        a.grid("a1-remoto").keys().contains(&"nota.txt")
    });
    m.app.click("a1-remoto-acima");
    esperar(&mut m.app, "a pasta acima", |a| {
        a.grid("a1-remoto").keys().contains(&"leia.txt")
    });

    escrever(&mut m.app, "a1-remoto-caminho", "/home/ana/docs");
    m.app.key(Key::Enter);
    esperar(&mut m.app, "o caminho digitado", |a| {
        a.grid("a1-remoto").keys().contains(&"nota.txt")
    });
    assert!(!m.app.visible("a1-remoto-erro"));

    escrever(&mut m.app, "a1-remoto-caminho", "/nao/existe");
    m.app.key(Key::Enter);
    esperar(&mut m.app, "o erro", |a| a.visible("a1-remoto-erro"));
    assert!(
        m.app.grid("a1-remoto").keys().contains(&"nota.txt"),
        "a pasta de antes continua à mostra"
    );
    m.app.assert_lint();
}

#[test]
fn arrastar_do_disco_para_o_servidor_envia_pela_fila() {
    let (_pasta, mut m) = montar_arquivos("arquivos-enviar");
    m.opener.disco.put("/home/ana/foto.png", "pixels");
    let servidor = m.opener.servidor(1);
    abrir_servidor(&mut m);
    m.app
        .drag_rows("a1-local", &["foto.png"], "a1-remoto", None);
    esperar_arquivo(&mut m.app, &servidor, "/home/ana/foto.png");
    assert_eq!(servidor.contents("/home/ana/foto.png").unwrap(), b"pixels");
    esperar(&mut m.app, "a fila acabar", |a| {
        a.grid("fila")
            .rows
            .iter()
            .all(|l| l.cells[6] == "Concluída")
    });
    let fila = m.app.grid("fila");
    assert_eq!(fila.rows.len(), 1);
    assert_eq!(fila.rows[0].cells[0], "foto.png");
    assert_eq!(
        fila.rows[0].cells[1], "Cópia",
        "o disco do teste também é um servidor"
    );
    esperar(&mut m.app, "o painel atualizar", |a| {
        a.grid("a1-remoto").keys().contains(&"foto.png")
    });
}

#[test]
fn f5_baixa_o_arquivo_escolhido() {
    let (_pasta, mut m) = montar_arquivos("arquivos-f5");
    let servidor = m.opener.servidor(1);
    servidor.put("/home/ana/dados.csv", "1,2");
    abrir_servidor(&mut m);
    m.app.click("a1-remoto-dados.csv").key(Key::F(5));
    esperar_arquivo(&mut m.app, &m.opener.disco, "/home/ana/dados.csv");
    assert_eq!(
        m.opener.disco.contents("/home/ana/dados.csv").unwrap(),
        b"1,2"
    );
}

#[test]
fn arrastar_para_uma_pasta_do_mesmo_painel_move() {
    let (_pasta, mut m) = montar_arquivos("arquivos-mover");
    let servidor = m.opener.servidor(1);
    servidor
        .dir("/home/ana/docs")
        .put("/home/ana/leia.txt", "oi");
    abrir_servidor(&mut m);
    m.app
        .drag_rows("a1-remoto", &["leia.txt"], "a1-remoto", Some("docs"));
    esperar_arquivo(&mut m.app, &servidor, "/home/ana/docs/leia.txt");
    assert!(!servidor.exists("/home/ana/leia.txt"));
}

#[test]
fn arquivos_do_sistema_soltos_numa_pasta_do_remoto_sao_enviados() {
    let (pasta, mut m) = montar_arquivos("arquivos-soltar");
    let servidor = m.opener.servidor(1);
    servidor.dir("/home/ana/docs");
    abrir_servidor(&mut m);
    // O disco da tela é o `MemFs`: o envio lê o arquivo do `Local` de verdade.
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    let arquivo = pasta.caminho().join("solto.txt");
    std::fs::write(&arquivo, "solto").unwrap();
    m.app.drop_files("a1-remoto-docs", &[arquivo]);
    esperar_arquivo(&mut m.app, &servidor, "/home/ana/docs/solto.txt");
    assert_eq!(
        servidor.contents("/home/ana/docs/solto.txt").unwrap(),
        b"solto"
    );
}

/// Prepara um conflito: `a.txt` no disco ("novo") e no servidor ("velho"), e o arrasto.
fn conflito(nome: &str) -> (Pasta, Montagem, std::rc::Rc<MemFs>) {
    let (pasta, mut m) = montar_arquivos(nome);
    m.opener.disco.put("/home/ana/a.txt", "novo");
    let servidor = m.opener.servidor(1);
    servidor.put("/home/ana/a.txt", "velho");
    abrir_servidor(&mut m);
    m.app.drag_rows("a1-local", &["a.txt"], "a1-remoto", None);
    esperar(&mut m.app, "a pergunta", |a| a.is_open("conflito-dialogo"));
    (pasta, m, servidor)
}

#[test]
fn conflito_sobrescrever() {
    let (_pasta, mut m, servidor) = conflito("arquivos-conflito-sobrescrever");
    assert!(m.app.text("conflito-texto").contains("a.txt"));
    m.app.assert_lint();
    m.app.click("conflito-dialogo-primary");
    esperar(&mut m.app, "o envio", |_| {
        servidor.contents("/home/ana/a.txt").as_deref() == Some(b"novo".as_slice())
    });
}

#[test]
fn conflito_pular() {
    let (_pasta, mut m, servidor) = conflito("arquivos-conflito-pular");
    m.app.click("conflito-dialogo-secondary");
    esperar(&mut m.app, "a transferência pulada", |a| {
        a.grid("fila").rows.iter().all(|l| l.cells[6] == "Pulada")
    });
    assert_eq!(servidor.contents("/home/ana/a.txt").unwrap(), b"velho");
}

#[test]
fn conflito_renomear() {
    let (_pasta, mut m, servidor) = conflito("arquivos-conflito-renomear");
    m.app.click("conflito-renomear");
    esperar(&mut m.app, "o arquivo renomeado", |a| {
        a.grid("fila")
            .rows
            .iter()
            .all(|l| l.cells[6] == "Concluída")
    });
    assert_eq!(servidor.contents("/home/ana/a.txt").unwrap(), b"velho");
    let novos: Vec<String> = servidor
        .paths()
        .into_iter()
        .filter(|p| p.starts_with("/home/ana/a") && p != "/home/ana/a.txt")
        .collect();
    assert_eq!(novos.len(), 1, "{novos:?}");
    assert_eq!(servidor.contents(&novos[0]).unwrap(), b"novo");
}

#[test]
fn conflito_retomar_e_cancelar_com_esc() {
    let (_pasta, mut m, servidor) = conflito("arquivos-conflito-retomar");
    m.app.click("conflito-retomar");
    esperar(&mut m.app, "a transferência acabar", |a| {
        a.grid("fila").rows.iter().all(|l| {
            matches!(l.cells[6].as_str(), "Concluída" | "Pulada")
                || l.cells[6].starts_with("Falhou")
        })
    });
    assert!(servidor.exists("/home/ana/a.txt"));

    // Esc no diálogo cancela a transferência que perguntou.
    m.app.drag_rows("a1-local", &["a.txt"], "a1-remoto", None);
    esperar(&mut m.app, "a pergunta", |a| a.is_open("conflito-dialogo"));
    m.app.key(Key::Escape);
    esperar(&mut m.app, "o cancelamento", |a| {
        a.grid("fila")
            .rows
            .iter()
            .any(|l| l.cells[6] == "Cancelada")
    });
}

#[test]
fn fazer_o_mesmo_nos_proximos_responde_uma_vez() {
    let (_pasta, mut m) = montar_arquivos("arquivos-conflito-todos");
    for nome in ["a.txt", "b.txt"] {
        m.opener.disco.put(&format!("/home/ana/{nome}"), "novo");
    }
    let servidor = m.opener.servidor(1);
    servidor
        .put("/home/ana/a.txt", "velho")
        .put("/home/ana/b.txt", "velho");
    abrir_servidor(&mut m);
    m.app
        .drag_rows("a1-local", &["a.txt", "b.txt"], "a1-remoto", None);
    esperar(&mut m.app, "a pergunta", |a| a.is_open("conflito-dialogo"));
    m.app
        .click("conflito-todos")
        .click("conflito-dialogo-secondary");
    esperar(&mut m.app, "as duas puladas", |a| {
        let fila = a.grid("fila");
        fila.rows.len() == 2 && fila.rows.iter().all(|l| l.cells[6] == "Pulada")
    });
    assert!(!m.app.is_open("conflito-dialogo"));
    assert_eq!(servidor.contents("/home/ana/b.txt").unwrap(), b"velho");
}

#[test]
fn apagar_pede_confirmacao() {
    let (_pasta, mut m) = montar_arquivos("arquivos-apagar");
    let servidor = m.opener.servidor(1);
    servidor
        .put("/home/ana/a.txt", "a")
        .put("/home/ana/b.txt", "b");
    abrir_servidor(&mut m);
    m.app.click("a1-remoto-a.txt").key(Key::Delete);
    assert!(m.app.is_open("a1-remoto-apagar-dialogo"));
    m.app.click("a1-remoto-apagar-dialogo-close");
    assert!(servidor.exists("/home/ana/a.txt"), "cancelar não apaga");

    m.app.grid_select("a1-remoto", &["a.txt", "b.txt"]);
    m.app.key(Key::Delete);
    assert!(m.app.is_open("a1-remoto-apagar-dialogo"));
    assert!(m.app.text("a1-remoto-apagar-lista").contains("b.txt"));
    m.app.assert_lint();
    m.app.click("a1-remoto-apagar-dialogo-primary");
    esperar(&mut m.app, "os arquivos apagados", |_| {
        !servidor.exists("/home/ana/a.txt") && !servidor.exists("/home/ana/b.txt")
    });
}

#[test]
fn propriedades_trocam_as_permissoes_pelo_octal_e_pelas_caixas() {
    let (_pasta, mut m) = montar_arquivos("arquivos-propriedades");
    let servidor = m.opener.servidor(1);
    servidor.put("/home/ana/run.sh", "#!/bin/sh");
    abrir_servidor(&mut m);
    m.app.context_menu("a1-remoto-run.sh");
    m.app.click("a1-remoto-run.sh-propriedades");
    assert!(m.app.is_open("a1-remoto-prop-dialogo"));
    m.app.assert_lint();

    escrever(&mut m.app, "a1-remoto-prop-octal", "750");
    // 750 = rwxr-x---: o dono (bits 0 a 2) liga tudo, o grupo só lê e executa.
    for i in [0, 1, 2, 3, 5] {
        assert!(m.app.checked(&format!("a1-remoto-prop-bit-{i}")), "bit {i}");
    }
    for i in [4, 6, 7, 8] {
        assert!(
            !m.app.checked(&format!("a1-remoto-prop-bit-{i}")),
            "bit {i}"
        );
    }
    // Desmarca "executar" do grupo (bit 5): 740.
    m.app.click("a1-remoto-prop-bit-5");
    assert_eq!(m.app.text("a1-remoto-prop-octal"), "740");
    m.app.click("a1-remoto-prop-dialogo-primary");
    esperar(&mut m.app, "as permissões", |_| {
        servidor
            .connect()
            .stat("/home/ana/run.sh")
            .is_ok_and(|e| e.permissions.map(|p| p & 0o777) == Some(0o740))
    });
}

#[test]
fn a_pasta_remota_entra_nos_recentes_e_o_recente_volta_a_ela() {
    let (_pasta, mut m) = montar_arquivos("arquivos-recentes");
    let servidor = m.opener.servidor(1);
    servidor
        .dir("/home/ana/docs")
        .put("/home/ana/docs/nota.txt", "n");
    abrir_servidor(&mut m);
    m.app.double_click("a1-remoto-docs");
    esperar(&mut m.app, "a pasta docs", |a| {
        a.grid("a1-remoto").keys().contains(&"nota.txt")
    });

    m.app.close_tab("arquivos-abas", "a1");
    assert!(m.app.visible("arquivos-conexao-1"), "a lista de conexões");
    assert!(
        m.app.visible("arquivos-recente-0"),
        "a pasta docs nos recentes"
    );
    m.app.assert_lint();

    m.app.click("arquivos-recente-0");
    esperar(&mut m.app, "a pasta do recente", |a| {
        a.visible("a2-remoto") && a.grid("a2-remoto").keys().contains(&"nota.txt")
    });
    assert_eq!(m.app.tab_titles("arquivos-abas"), ["servidor"]);
}

/// A página sem abas, no padrão da Gallery: a faixa, o cartão da conexão e o da pasta recente.
fn pagina_vazia(nome: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore());
    let mut m = montar(&pasta, tema);
    m.opener.servidor(1).dir("/home/ana/docs");
    abrir_servidor(&mut m);
    m.app.double_click("a1-remoto-docs");
    esperar(&mut m.app, "a pasta docs", |a| {
        a.text("a1-remoto-caminho").ends_with("/docs")
    });
    m.app.close_tab("arquivos-abas", "a1");
    m.app.wait(Duration::from_millis(100));
    (pasta, m)
}

#[test]
fn sem_abas_a_pagina_mostra_a_faixa_e_os_cartoes() {
    let (_pasta, mut m) = pagina_vazia("arquivos-vazia", Theme::Light);
    assert!(m.app.visible("arquivos-protocolos"));
    assert!(m.app.visible("arquivos-remotos-rclone"));
    assert!(!m.app.visible("arquivos-sem-conexoes"));
    assert_eq!(
        m.app.text("arquivos-conexao-1-detalhe"),
        "ana@arquivos.exemplo:22 · SFTP"
    );
    assert!(
        m.app
            .text("arquivos-recente-0-detalhe")
            .ends_with("/home/ana/docs")
    );
    m.app.assert_lint();

    m.app.click("arquivos-nova-conexao");
    assert!(m.app.visible("arvore"), "foi para os Terminais");
}

#[test]
fn snapshot_da_pagina_arquivos_sem_abas_claro() {
    let (_pasta, m) = pagina_vazia("arquivos-vazia-snap-claro", Theme::Light);
    m.app.assert_snapshot("arquivos-vazia-claro");
}

#[test]
fn snapshot_da_pagina_arquivos_sem_abas_escuro() {
    let (_pasta, m) = pagina_vazia("arquivos-vazia-snap-escuro", Theme::Dark);
    m.app.assert_snapshot("arquivos-vazia-escuro");
}

#[test]
fn abrir_arquivos_pelo_menu_da_conexao_ssh() {
    let (_pasta, mut m) = montar_arquivos("arquivos-menu-da-arvore");
    m.opener.servidor(2).put("/home/ana/web.txt", "w");
    m.app.context_menu("arvore-2");
    m.app.click("menu-2-arquivos");
    esperar(&mut m.app, "a listagem", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"web.txt")
    });
    assert_eq!(m.app.text("navegacao"), "arquivos");
    assert!(m.opener.abertas.borrow().is_empty(), "sem terminal");
}

#[test]
fn abrir_arquivos_desta_sessao_usa_a_conexao_do_terminal() {
    let (_pasta, mut m) = montar_arquivos("arquivos-da-sessao");
    m.opener.servidor(2).put("/home/ana/web.txt", "w");
    m.app.double_click("arvore-2");
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("abrir-arquivos-da-sessao");
    esperar(&mut m.app, "a listagem", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"web.txt")
    });
    assert_eq!(m.app.text("navegacao"), "arquivos");
    assert_eq!(m.app.tab_titles("arquivos-abas"), ["web (sessão)"]);

    // De novo: só escolhe a aba que já existe.
    m.app.click("navegacao-terminais");
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("abrir-arquivos-da-sessao");
    assert_eq!(m.app.text("navegacao"), "arquivos");
    assert_eq!(m.app.tab_titles("arquivos-abas").len(), 1);

    // "Abrir terminal aqui" só existe na aba ligada a uma sessão e volta aos Terminais.
    m.app.context_menu("a1-remoto-web.txt");
    m.app.click("a1-remoto-web.txt-terminal");
    assert_eq!(m.app.text("navegacao"), "terminais");
}

#[test]
fn sem_credencial_pede_a_senha_e_salva_no_cofre() {
    let pasta = Pasta::nova("arquivos-senha");
    let mut tree = Tree::default();
    let mut c = Connection::new("sem-chave", Kind::Sftp);
    c.host = "sem-chave.exemplo".into();
    c.user = "ana".into();
    tree.add_connection(None, c);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.opener.servidor(1).put("/home/ana/oi.txt", "oi");
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert!(m.app.checked("senha-salvar"), "marcada por padrão");
    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-primary");
    esperar(&mut m.app, "a listagem", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"oi.txt")
    });
    assert_eq!(m.app.text("navegacao"), "arquivos");
    assert_eq!(
        m.opener.senhas_dos_arquivos.borrow()[0].conexao.as_deref(),
        Some("segredo")
    );
    let cofre = pasta.cofre();
    assert_eq!(cofre.itens().len(), 1);
    assert_eq!(cofre.itens()[0].senha, "segredo");
    assert!(!pasta.texto_cofre().contains("segredo"), "cifrada no disco");
}

#[test]
fn fechar_a_aba_com_transferencias_pergunta() {
    let (_pasta, mut m) = montar_arquivos("arquivos-fechar");
    m.opener.disco.put("/home/ana/grande.bin", "x");
    let servidor = m.opener.servidor(1);
    abrir_servidor(&mut m);
    m.app.click("fila-pausar");
    m.app
        .drag_rows("a1-local", &["grande.bin"], "a1-remoto", None);
    esperar(&mut m.app, "a transferência na fila", |a| {
        !a.grid("fila").rows.is_empty()
    });

    m.app.close_tab("arquivos-abas", "a1");
    assert!(m.app.is_open("arquivos-fechar-dialogo"));
    m.app.assert_lint();
    m.app.click("arquivos-fechar-dialogo-close");
    assert!(m.app.visible("arquivos-abas"), "Voltar mantém a aba");

    m.app.close_tab("arquivos-abas", "a1");
    m.app.click("arquivos-fechar-dialogo-primary");
    assert!(m.app.visible("arquivos-conexao-1"), "sem abas, a lista");
    m.app.wait(Duration::from_millis(200));
    assert!(!servidor.exists("/home/ana/grande.bin"));
}

#[test]
fn as_abas_e_as_pastas_voltam_na_proxima_execucao() {
    let pasta = Pasta::nova("arquivos-estado");
    pasta.semear(&arvore());
    {
        let mut m = montar(&pasta, Theme::Light);
        let servidor = m.opener.servidor(1);
        servidor
            .dir("/home/ana/docs")
            .put("/home/ana/docs/nota.txt", "n");
        abrir_servidor(&mut m);
        m.app.double_click("a1-remoto-docs");
        esperar(&mut m.app, "a pasta docs", |a| {
            a.grid("a1-remoto").keys().contains(&"nota.txt")
        });
        m.app.wait(Duration::from_millis(100));
    }
    let estado = std::fs::read_to_string(pasta.caminho().join("estado.json")).unwrap();
    assert!(estado.contains("/home/ana/docs"), "{estado}");

    let opener = tirech_omni::abrir::FakeOpener::new();
    opener
        .servidor(1)
        .dir("/home/ana/docs")
        .put("/home/ana/docs/nota.txt", "n");
    let (mut app, _fila) = comum::montar_com(&pasta, Theme::Light, opener);
    app.click("navegacao-arquivos");
    esperar(&mut app, "a pasta restaurada", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&"nota.txt")
    });
    assert_eq!(app.tab_titles("arquivos-abas"), ["servidor"]);
}

/// Duas pastas com arquivos fixos e duas cópias já concluídas, para os snapshots.
fn tela_para_snapshot(nome: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore());
    let mut m = montar(&pasta, tema);
    m.opener
        .disco
        .dir("/home/ana/Projetos")
        .put("/home/ana/relatorio.pdf", vec![0_u8; 182_000])
        .put("/home/ana/foto.png", vec![0_u8; 2_400_000])
        .put("/home/ana/notas.txt", "notas");
    m.opener
        .servidor(1)
        .dir("/home/ana/www")
        .dir("/home/ana/backups")
        .put("/home/ana/index.html", vec![0_u8; 4_300])
        .put("/home/ana/deploy.sh", "#!/bin/sh");
    abrir_servidor(&mut m);
    // Com a fila pausada as duas cópias entram na mesma leva, por rápidas que sejam: sem isso, a
    // primeira pode acabar antes de a segunda ser enfileirada e abrir uma leva nova.
    m.app.click("fila-pausar");
    m.app.drag_rows(
        "a1-local",
        &["relatorio.pdf", "notas.txt"],
        "a1-remoto",
        None,
    );
    esperar(&mut m.app, "as duas na fila", |a| {
        a.grid("fila").rows.len() == 2
    });
    m.app.click("fila-continuar");
    esperar(&mut m.app, "as duas cópias", |a| {
        let fila = a.grid("fila");
        fila.rows.len() == 2 && fila.rows.iter().all(|l| l.cells[6] == "Concluída")
    });
    // O resumo vem de uma thread própria, que publica logo depois do fim da fila.
    esperar(&mut m.app, "o resumo concluído", |a| {
        a.text("fila-resumo").starts_with("Concluído")
    });
    esperar(&mut m.app, "o painel remoto atualizar", |a| {
        a.grid("a1-remoto").keys().contains(&"notas.txt")
    });
    m.app.grid_select("a1-remoto", &["deploy.sh"]);
    m.app.wait(Duration::from_millis(100));
    (pasta, m)
}

#[test]
fn snapshot_da_pagina_arquivos_claro() {
    let (_pasta, m) = tela_para_snapshot("arquivos-snap-claro", Theme::Light);
    m.app.assert_snapshot("arquivos-claro");
}

#[test]
fn snapshot_da_pagina_arquivos_escuro() {
    let (_pasta, m) = tela_para_snapshot("arquivos-snap-escuro", Theme::Dark);
    m.app.assert_snapshot("arquivos-escuro");
}

/// Grava `texto` no arquivo com uma data bem à frente, para o vigia ver a mudança mesmo que a
/// gravação caia no mesmo instante da cópia.
fn editar_no_disco(arquivo: &std::path::Path, texto: &str, adiante: u64) {
    std::fs::write(arquivo, texto).unwrap();
    let aberto = std::fs::File::options().write(true).open(arquivo).unwrap();
    aberto
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(adiante))
        .unwrap();
}

#[test]
fn editar_no_editor_externo_baixa_vigia_e_envia_de_volta() {
    let (_pasta, mut m) = montar_arquivos("arquivos-editar");
    let servidor = m.opener.servidor(1);
    servidor.put("/home/ana/edita-nota.txt", "v1");
    abrir_servidor(&mut m);

    m.app.double_click("a1-remoto-edita-nota.txt");
    esperar(&mut m.app, "o editor abrir", |_| {
        !m.opener.externos.borrow().is_empty()
    });
    let arquivo = m.opener.externos.borrow()[0].clone();
    assert!(
        arquivo.ends_with("tirech-omni/1/home/ana/edita-nota.txt"),
        "{arquivo:?}"
    );
    assert_eq!(std::fs::read_to_string(&arquivo).unwrap(), "v1");
    assert!(!m.app.visible("edicao-enviar"), "sem mudança, sem pergunta");

    // Outro duplo clique só traz o editor de volta, sem baixar por cima.
    m.app.double_click("a1-remoto-edita-nota.txt");
    esperar(&mut m.app, "o editor abrir de novo", |_| {
        m.opener.externos.borrow().len() == 2
    });

    editar_no_disco(&arquivo, "v2", 10);
    esperar(&mut m.app, "a pergunta", |a| a.visible("edicao-enviar"));
    assert!(
        m.app
            .text("edicao-aviso")
            .contains("edita-nota.txt foi alterado")
    );
    m.app.assert_lint();
    m.app.click("edicao-enviar");
    esperar(&mut m.app, "o envio", |_| {
        servidor.contents("/home/ana/edita-nota.txt").unwrap() == b"v2"
    });
    assert!(!m.app.visible("edicao-enviar"));

    // "Sempre enviar": a mudança seguinte vai sem perguntar.
    editar_no_disco(&arquivo, "v3", 20);
    esperar(&mut m.app, "a pergunta", |a| a.visible("edicao-enviar"));
    m.app.click("edicao-sempre");
    esperar(&mut m.app, "o envio", |_| {
        servidor.contents("/home/ana/edita-nota.txt").unwrap() == b"v3"
    });
    editar_no_disco(&arquivo, "v4", 30);
    esperar(&mut m.app, "o envio direto", |_| {
        servidor.contents("/home/ana/edita-nota.txt").unwrap() == b"v4"
    });
    assert!(!m.app.visible("edicao-enviar"));

    // Fechar a aba esquece a edição: o que mudar depois não vai.
    m.app.close_tab("arquivos-abas", "a1");
    editar_no_disco(&arquivo, "v5", 40);
    m.app.wait(Duration::from_millis(1500));
    assert_eq!(
        servidor.contents("/home/ana/edita-nota.txt").unwrap(),
        b"v4"
    );
    let _ = std::fs::remove_file(&arquivo);
}

#[test]
fn duplo_clique_num_arquivo_local_abre_direto_no_editor() {
    let (_pasta, mut m) = montar_arquivos("arquivos-editar-local");
    m.opener.disco.put("/home/ana/local.txt", "l");
    abrir_servidor(&mut m);
    m.app.double_click("a1-local-local.txt");
    assert_eq!(
        m.opener.externos.borrow().as_slice(),
        [std::path::PathBuf::from("/home/ana/local.txt")]
    );
}

/// A barra do resumo da fila, com o estado que ela pinta.
fn barra_da_fila(app: &TestApp) -> ProgressStatus {
    match app.look("fila-barra") {
        tomni_ui::Look::ProgressBar(visual) => visual.status,
        look => panic!("fila-barra não é uma ProgressBar: {look:?}"),
    }
}

/// Uma pasta com três arquivos no disco local, e o servidor aberto.
fn tela_com_uma_pasta(nome: &str) -> (Pasta, Montagem, Rc<MemFs>) {
    let (pasta, mut m) = montar_arquivos(nome);
    m.opener
        .disco
        .put("/home/ana/obra/a.txt", "aaa")
        .put("/home/ana/obra/b.txt", "bbbb")
        .put("/home/ana/obra/c.txt", "ccccc");
    let servidor = m.opener.servidor(1);
    servidor.dir("/home/ana/destino");
    abrir_servidor(&mut m);
    (pasta, m, servidor)
}

#[test]
fn a_fila_mostra_o_resumo_e_conta_as_pastas() {
    let (_pasta, mut m, _servidor) = tela_com_uma_pasta("arquivos-resumo");
    m.app.click("fila-pausar");
    m.app.drag_rows("a1-local", &["obra"], "a1-remoto", None);
    esperar(&mut m.app, "a pasta na fila", |a| {
        !a.grid("fila").rows.is_empty() && a.visible("fila-barra-contando")
    });
    assert!(
        m.app.text("fila-resumo").starts_with("Contando arquivos…"),
        "{:?}",
        m.app.text("fila-resumo")
    );
    assert!(!m.app.visible("fila-barra"));
    assert_eq!(m.app.text("fila-restante"), "Pausada");

    m.app.click("fila-continuar");
    esperar(&mut m.app, "a pasta concluir", |a| {
        a.text("fila-resumo").starts_with("Concluído · 3 arquivos")
    });
    assert_eq!(m.app.value("fila-barra"), 1.0);
    assert_eq!(m.app.text("fila-restante"), "");
}

/// Dois arquivos soltos no disco local e o servidor aberto: o tamanho vem da listagem, então a
/// barra que enche aparece desde o começo (a pasta, ao contrário, começa "contando").
fn tela_com_dois_arquivos(nome: &str) -> (Pasta, Montagem, Rc<MemFs>) {
    let (pasta, mut m) = montar_arquivos(nome);
    m.opener
        .disco
        .put("/home/ana/x.bin", vec![0_u8; 3000])
        .put("/home/ana/y.bin", vec![0_u8; 4000]);
    let servidor = m.opener.servidor(1);
    abrir_servidor(&mut m);
    (pasta, m, servidor)
}

#[test]
fn a_barra_fica_amarela_com_a_fila_pausada() {
    let (_pasta, mut m, _servidor) = tela_com_dois_arquivos("arquivos-barra-pausada");
    m.app.click("fila-pausar");
    m.app
        .drag_rows("a1-local", &["x.bin", "y.bin"], "a1-remoto", None);
    esperar(&mut m.app, "as duas na fila", |a| {
        a.grid("fila").rows.len() == 2 && a.visible("fila-barra")
    });
    assert_eq!(barra_da_fila(&m.app), ProgressStatus::Paused);

    m.app.click("fila-continuar");
    esperar(&mut m.app, "as duas concluírem", |a| {
        a.text("fila-resumo").starts_with("Concluído")
    });
    assert_eq!(barra_da_fila(&m.app), ProgressStatus::Normal);
}

#[test]
fn falha_deixa_a_barra_vermelha() {
    let (_pasta, mut m, servidor) = tela_com_dois_arquivos("arquivos-barra-erro");
    servidor.deny("/home/ana/x.bin");
    m.app.drag_rows("a1-local", &["x.bin"], "a1-remoto", None);
    esperar(&mut m.app, "o resumo da falha", |a| {
        a.text("fila-resumo").starts_with("Concluído com 1 falha")
    });
    assert_eq!(barra_da_fila(&m.app), ProgressStatus::Error);
}

#[test]
fn limpar_concluidas_some_com_o_resumo() {
    let (_pasta, mut m, _servidor) = tela_com_dois_arquivos("arquivos-limpar-resumo");
    m.app
        .drag_rows("a1-local", &["x.bin", "y.bin"], "a1-remoto", None);
    esperar(&mut m.app, "o resumo concluído", |a| {
        a.text("fila-resumo").starts_with("Concluído")
    });
    assert!(m.app.visible("fila-barra"));

    m.app.click("fila-limpar");
    esperar(&mut m.app, "o resumo sumir", |a| {
        a.text("fila-resumo").is_empty()
    });
    assert!(!m.app.visible("fila-barra"));
    assert!(!m.app.visible("fila-barra-contando"));
    assert!(m.app.grid("fila").rows.is_empty());
}

// --- Pastas padrão (M41) ---

/// Monta a página com a conexão 1 já com as pastas de `ajustar`, o `/home/ana/www` no disco
/// e `/srv/www` no servidor.
fn montar_com_pastas(nome: &str, ajustar: impl FnOnce(&mut Connection)) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore_com(ajustar));
    let m = montar(&pasta, Theme::Light);
    m.opener
        .disco
        .dir("/home/ana/www")
        .put("/home/ana/www/site.css", "c");
    m.opener.disco.put("/home/ana/solto.txt", "s");
    m.opener
        .servidor(1)
        .dir("/srv/www")
        .put("/srv/www/index.html", "i");
    m.opener.servidor(1).put("/home/ana/leia.txt", "oi");
    (pasta, m)
}

#[test]
fn abre_nas_pastas_padrao_da_conexao() {
    let (_pasta, mut m) = montar_com_pastas("arquivos-padrao", |c| {
        c.pasta_remota = "/srv/www".into();
        c.pasta_local = "/home/ana/www".into();
    });
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "os dois painéis", |a| {
        a.grid("a1-remoto").keys().contains(&"index.html")
            && a.grid("a1-local").keys().contains(&"site.css")
    });
    assert_eq!(m.app.text("a1-remoto-caminho"), "/srv/www");
    assert_eq!(m.app.text("a1-local-caminho"), "/home/ana/www");
}

/// O bug do M40: `abrir_arquivos_agora` gravava `local_inicial: None` e apagava a pasta local
/// das Configurações.
#[test]
fn a_pasta_local_das_configuracoes_vale_sem_a_da_conexao() {
    let pasta = Pasta::nova("arquivos-padrao-global");
    pasta.semear(&arvore());
    pasta.configurar("arquivos", "pasta_local", "/home/ana/www".into());
    let mut m = montar(&pasta, Theme::Light);
    m.opener
        .disco
        .dir("/home/ana/www")
        .put("/home/ana/www/site.css", "c");
    m.opener.servidor(1).put("/home/ana/leia.txt", "oi");
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o painel local", |a| {
        a.grid("a1-local").keys().contains(&"site.css")
    });
    assert_eq!(m.app.text("a1-local-caminho"), "/home/ana/www");
}

#[test]
fn a_pasta_da_conexao_vence_a_das_configuracoes() {
    let pasta = Pasta::nova("arquivos-padrao-vence");
    pasta.semear(&arvore_com(|c| c.pasta_local = "/home/ana/www".into()));
    pasta.configurar("arquivos", "pasta_local", "/home/ana/outra".into());
    let mut m = montar(&pasta, Theme::Light);
    m.opener
        .disco
        .dir("/home/ana/www")
        .put("/home/ana/www/site.css", "c");
    m.opener
        .disco
        .dir("/home/ana/outra")
        .put("/home/ana/outra/x.txt", "x");
    m.opener.servidor(1).put("/home/ana/leia.txt", "oi");
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o painel local", |a| {
        a.grid("a1-local").keys().contains(&"site.css")
    });
    assert_eq!(m.app.text("a1-local-caminho"), "/home/ana/www");
}

#[test]
fn o_recente_vence_a_pasta_padrao() {
    let (_pasta, mut m) = montar_com_pastas("arquivos-padrao-recente", |c| {
        c.pasta_remota = "/srv/www".into();
    });
    m.opener
        .servidor(1)
        .dir("/home/ana/docs")
        .put("/home/ana/docs/nota.txt", "n");
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a pasta padrão", |a| {
        a.grid("a1-remoto").keys().contains(&"index.html")
    });
    escrever(&mut m.app, "a1-remoto-caminho", "/home/ana/docs");
    m.app.key(Key::Enter);
    esperar(&mut m.app, "a pasta docs", |a| {
        a.grid("a1-remoto").keys().contains(&"nota.txt")
    });
    m.app.close_tab("arquivos-abas", "a1");
    m.app.click("arquivos-recente-0");
    esperar(&mut m.app, "a pasta do recente", |a| {
        a.visible("a2-remoto") && a.grid("a2-remoto").keys().contains(&"nota.txt")
    });
    assert_eq!(m.app.text("a2-remoto-caminho"), "/home/ana/docs");
}

#[test]
fn pasta_padrao_que_nao_existe_cai_no_inicio_com_aviso() {
    let (_pasta, mut m) = montar_com_pastas("arquivos-padrao-ausente", |c| {
        c.pasta_remota = "/nao/existe".into();
    });
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "o início do servidor", |a| {
        a.grid("a1-remoto").keys().contains(&"leia.txt")
    });
    assert_eq!(m.app.text("a1-remoto-caminho"), "/home/ana");
    esperar(&mut m.app, "o aviso", |a| a.visible("a1-aviso"));
    assert_eq!(
        m.app.text("a1-aviso"),
        "A pasta padrão `/nao/existe` não existe; abri o início."
    );
}

#[test]
fn a_fila_embaixo_pode_ficar_auto_ocultar() {
    let (pasta, mut m) = montar_arquivos("arquivos-paineis-fila");
    abrir_servidor(&mut m);
    assert!(m.app.visible("arquivos-paineis-fila"));
    let conteudo = m.app.rect("arquivos-paineis-conteudo");
    assert!(m.app.rect("arquivos-paineis-fila").y0 >= conteudo.y1 - 1.0);

    m.app.click("arquivos-paineis-fila-alfinete");
    assert!(!m.app.visible("arquivos-paineis-fila"));
    assert!(m.app.visible("arquivos-paineis-fila-faixa"));
    let texto = std::fs::read_to_string(pasta.caminho().join("estado.json")).unwrap();
    let valor = serde_json::from_str(&texto).unwrap();
    let estado = tirech_omni::estado::do_valor(&valor).unwrap();
    assert_eq!(
        estado.paineis["arquivos"].paineis["fila"].modo,
        tirech_omni::paineis::ModoSalvo::Auto
    );

    // Parar o mouse na faixa abre a fila por cima, sem mexer no conteúdo.
    let antes = m.app.rect("arquivos-paineis-conteudo");
    m.app.hover("arquivos-paineis-fila-faixa");
    m.app.wait(Duration::from_millis(800));
    assert!(m.app.visible("arquivos-paineis-fila"));
    assert_eq!(m.app.rect("arquivos-paineis-conteudo"), antes);
}
