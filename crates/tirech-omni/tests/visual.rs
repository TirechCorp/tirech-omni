//! A aparência do app nos dois temas: snapshots e o lint com os diálogos abertos.

mod comum;

use std::thread;

use tirech_omni::abrir::perguntar;
use tirech_omni::cofre::{Cofre, CofreCompartilhado, Item, Tipo};
use tirech_omni::espacos::{Espaco, Espacos, Item as ItemSalvo, JanelaSalva, Origem};
use tirech_omni::tela_cofre::{dialogo_desbloquear, pagina_cofre};
use tomni_ui::prelude::*;
use tomni_ui::terminal::{KnownHost, Session};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use tirech_omni::recentes::fixar_relogio;

use comum::{Montagem, Pasta, exemplo, montar, montar_pura, so_com_senha};

fn item(titulo: &str, usuario: &str, senha: &str, tipo: Tipo) -> Item {
    Item {
        tipo,
        titulo: titulo.into(),
        usuario: usuario.into(),
        senha: senha.into(),
        url: "https://painel.acme.exemplo".into(),
        ..Item::default()
    }
}

/// O id do item de `titulo` no cofre gravado.
fn id_do_item(pasta: &Pasta, titulo: &str) -> u64 {
    pasta
        .cofre()
        .itens()
        .iter()
        .find(|i| i.titulo == titulo)
        .unwrap_or_else(|| panic!("o item {titulo} não existe"))
        .id
}

/// A página do cofre sozinha, aberta em Clientes › ACME: duas subpastas e três itens (mais um
/// em Pessoal, fora de vista).
fn pagina_com_itens(pasta: &Pasta, tema: Theme) -> TestApp {
    // As datas da coluna "Alterado" entram na imagem: o relógio precisa ser o mesmo sempre.
    tirech_omni::recentes::fixar_relogio(Some(1_760_000_000));
    pasta.semear_cofre_em(vec![
        (
            "Clientes/ACME",
            item("Painel da ACME", "ana", "s3nh4-longa", Tipo::Senha),
        ),
        ("Clientes/ACME", item("web-01", "root", "outra", Tipo::Ssh)),
        ("Clientes/ACME", item("Contrato", "", "", Tipo::Nota)),
        (
            "Clientes/ACME/Produção",
            item("db-01", "dba", "pw", Tipo::Ssh),
        ),
        (
            "Clientes/ACME/Homologação",
            item("db-hml", "dba", "pw2", Tipo::Ssh),
        ),
        ("Pessoal", item("Banco", "bia", "1234", Tipo::Senha)),
    ]);
    let (cofre, _) = Cofre::abrir(pasta.caminho());
    let cofre = CofreCompartilhado::new(cofre);
    let mut app = TestApp::new(move || {
        Form::new("Cofre")
            .density(Density::Compact)
            .child(pagina_cofre(cofre.clone()))
    })
    .size(1280, 720)
    .theme(tema);
    for caminho in ["Clientes", "Clientes/ACME"] {
        let id = pasta.id_da_pasta(caminho);
        app.double_click(&format!("cofre-lista-pasta:{id}"));
    }
    app
}

fn encerrar_sessoes() {
    for id in 1..=5 {
        for n in 1..=8 {
            if let Some(s) = Session::find(&format!("{id}-{n}")) {
                s.dispose();
            }
        }
    }
}

/// O Início com três recentes (um fixado), como fica depois de um dia de uso.
fn inicio_com_recentes(pasta: &Pasta, tema: Theme) -> Montagem {
    const T0: u64 = 1_700_000_000;
    pasta.semear(&exemplo());
    {
        let mut m = montar(pasta, tema);
        m.app.expand("arvore", "1");
        fixar_relogio(Some(T0));
        m.app.double_click("arvore-2");
        fixar_relogio(Some(T0 + 120));
        m.app.double_click("arvore-3");
        fixar_relogio(Some(T0 + 7200));
        m.app.double_click("arvore-4");
    }
    encerrar_sessoes();
    std::fs::remove_file(pasta.armazenamento().arquivo_estado()).ok();
    std::fs::write(
        pasta.armazenamento().arquivo_configuracoes(),
        r#"{"pagina_inicial": "inicio"}"#,
    )
    .unwrap();
    fixar_relogio(Some(T0 + 7200 + 300));
    let mut m = montar_pura(pasta, tema);
    m.app.right_click("inicio-recente-2");
    m.app.click("inicio-recente-2-menu-fixar");
    m
}

fn com_duas_abas(pasta: &Pasta, tema: Theme) -> Montagem {
    pasta.semear(&exemplo());
    let mut m = montar(pasta, tema);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2").double_click("arvore-3");
    m.app.click("arvore-2");
    m
}

#[test]
fn a_tela_com_conexoes_e_duas_abas_nos_dois_temas() {
    for (tema, nome) in [(Theme::Light, "tela-claro"), (Theme::Dark, "tela-escuro")] {
        let pasta = Pasta::nova(nome);
        let mut m = com_duas_abas(&pasta, tema);
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
        drop(m);
        encerrar_sessoes();
    }
}

#[test]
fn as_conexoes_abertas_por_cima_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "terminais_conexoes_por_cima-claro"),
        (Theme::Dark, "terminais_conexoes_por_cima-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        let mut m = com_duas_abas(&pasta, tema);
        m.app.click("terminais-paineis-conexoes-alfinete");
        m.app.click("terminais-paineis-conexoes-faixa");
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
        drop(m);
        encerrar_sessoes();
    }
}

#[test]
fn o_dialogo_da_chave_do_servidor_nos_dois_temas() {
    for (tema, nome) in [(Theme::Light, "chave-claro"), (Theme::Dark, "chave-escuro")] {
        let pasta = Pasta::nova(nome);
        let mut m = montar(&pasta, tema);
        let fila = m.fila;
        thread::spawn(move || {
            perguntar(
                fila,
                "web-01",
                "web-01.exemplo:22",
                "ssh-ed25519",
                "SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
                KnownHost::Unknown,
            )
        });
        while !m.app.is_open("chave-dialogo") {
            m.app.wait(std::time::Duration::from_millis(20));
        }
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
    }
}

#[test]
fn o_dialogo_da_senha_passa_no_lint_nos_dois_temas() {
    for tema in [Theme::Light, Theme::Dark] {
        let pasta = Pasta::nova("lint-senha");
        pasta.semear(&so_com_senha());
        let mut m = montar(&pasta, tema);
        m.app.double_click("arvore-1");
        assert!(m.app.is_open("senha-dialogo"));
        m.app.assert_lint();
    }
}

#[test]
fn os_menus_abertos_passam_no_lint_nos_dois_temas() {
    for tema in [Theme::Light, Theme::Dark] {
        let pasta = Pasta::nova("lint-menus");
        pasta.semear(&exemplo());
        let mut m = montar(&pasta, tema);
        for menu in ["arquivo", "exibir", "ferramentas", "ajuda"] {
            m.app.click(&format!("menus-{menu}"));
            m.app.assert_lint();
            m.app.key(tomni_ui::Key::Escape);
        }
    }
}

#[test]
fn a_pagina_do_cofre_nos_dois_temas() {
    for (tema, nome) in [(Theme::Light, "cofre-claro"), (Theme::Dark, "cofre-escuro")] {
        let pasta = Pasta::nova(nome);
        let mut app = pagina_com_itens(&pasta, tema);
        app.click(&format!(
            "cofre-lista-item:{}",
            id_do_item(&pasta, "Painel da ACME")
        ));
        app.assert_lint();
        app.assert_snapshot(nome);
    }
}

#[test]
fn o_dialogo_de_mover_do_cofre_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "cofre-mover-claro"),
        (Theme::Dark, "cofre-mover-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        let mut app = pagina_com_itens(&pasta, tema);
        app.click(&format!(
            "cofre-lista-item:{}",
            id_do_item(&pasta, "Painel da ACME")
        ));
        app.click("cofre-mover");
        assert!(app.is_open("cofre-mover-dialogo"));
        app.assert_lint();
        app.assert_snapshot(nome);
    }
}

#[test]
fn o_dialogo_de_apagar_pasta_do_cofre_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "cofre-apagar-pasta-claro"),
        (Theme::Dark, "cofre-apagar-pasta-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        let mut app = pagina_com_itens(&pasta, tema);
        app.click(&format!(
            "cofre-pastas-pasta:{}",
            pasta.id_da_pasta("Clientes")
        ));
        app.click(&format!(
            "cofre-lista-pasta:{}",
            pasta.id_da_pasta("Clientes/ACME")
        ));
        app.click("cofre-apagar");
        assert!(app.is_open("cofre-apagar-dialogo"));
        assert_eq!(
            app.text("cofre-apagar-texto"),
            "A pasta, 2 subpastas e 5 itens saem do cofre."
        );
        app.assert_lint();
        app.assert_snapshot(nome);
    }
}

#[test]
fn os_dialogos_da_pagina_do_cofre_passam_no_lint_nos_dois_temas() {
    for tema in [Theme::Light, Theme::Dark] {
        let pasta = Pasta::nova("lint-cofre");
        let mut app = pagina_com_itens(&pasta, tema);
        app.click(&format!(
            "cofre-lista-item:{}",
            id_do_item(&pasta, "Painel da ACME")
        ));
        for (abre, dialogo, fecha) in [
            (
                "cofre-apagar",
                "cofre-apagar-dialogo",
                "cofre-apagar-dialogo-close",
            ),
            (
                "cofre-mover",
                "cofre-mover-dialogo",
                "cofre-mover-dialogo-close",
            ),
            ("cofre-gerar", "cofre-gerador", "cofre-gerador-close"),
            (
                "cofre-mestra",
                "cofre-senha-mestra",
                "cofre-senha-mestra-close",
            ),
            ("cofre-campo-novo", "cofre-campo", "cofre-campo-close"),
        ] {
            app.click(abre);
            assert!(app.is_open(dialogo));
            app.assert_lint();
            app.click(fecha);
        }
        app.click("cofre-exportar");
        app.assert_lint();
    }
}

#[test]
fn o_cofre_bloqueado_passa_no_lint_nos_dois_temas() {
    for tema in [Theme::Light, Theme::Dark] {
        let pasta = Pasta::nova("lint-bloqueado");
        pasta.semear_cofre(vec![item("Banco", "bia", "1234", Tipo::Senha)]);
        pasta.cofre_com_senha_mestra("abc");
        let (cofre, _) = Cofre::abrir(pasta.caminho());
        let cofre = CofreCompartilhado::new(cofre);
        let mut app = TestApp::new(move || {
            Form::new("Cofre")
                .density(Density::Compact)
                .child(pagina_cofre(cofre.clone()))
        })
        .size(1280, 720)
        .theme(tema);
        assert!(app.visible("cofre-bloqueado"));
        app.assert_lint();
        app.click("cofre-desbloquear-botao");
        assert!(app.is_open("cofre-pagina-desbloquear"));
        app.assert_lint();
    }
}

#[test]
fn o_dialogo_de_desbloqueio_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "desbloquear-claro"),
        (Theme::Dark, "desbloquear-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        pasta.semear_cofre(vec![item("Banco", "bia", "1234", Tipo::Senha)]);
        pasta.cofre_com_senha_mestra("abc");
        let (cofre, _) = Cofre::abrir(pasta.caminho());
        let cofre = CofreCompartilhado::new(cofre);
        let aberto = signal(true);
        let mut app = TestApp::new(move || {
            Form::new("Cofre")
                .density(Density::Compact)
                .child(dialogo_desbloquear(cofre.clone(), aberto))
        })
        .size(640, 360)
        .theme(tema);
        assert!(app.is_open("cofre-desbloquear"));
        app.assert_lint();
        app.assert_snapshot(nome);
    }
}

#[test]
fn o_dialogo_da_senha_com_salvar_a_senha_nos_dois_temas() {
    for (tema, nome) in [(Theme::Light, "senha-claro"), (Theme::Dark, "senha-escuro")] {
        let pasta = Pasta::nova(nome);
        pasta.semear(&so_com_senha());
        let mut m = montar(&pasta, tema);
        m.app.double_click("arvore-1");
        assert!(m.app.is_open("senha-dialogo"));
        assert!(m.app.checked("senha-salvar"));
        m.app.assert_snapshot(nome);
    }
}

#[test]
fn o_inicio_com_recentes_fixadas_e_ferramentas_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "inicio-claro"),
        (Theme::Dark, "inicio-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        let mut m = inicio_com_recentes(&pasta, tema);
        assert!(m.app.visible("inicio-fixada-0"));
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
        drop(m);
        encerrar_sessoes();
        fixar_relogio(None);
    }
}

#[test]
fn o_inicio_vazio_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "inicio-vazio-claro"),
        (Theme::Dark, "inicio-vazio-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        let mut m = montar_pura(&pasta, tema);
        assert_eq!(m.app.text("navegacao"), "inicio");
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
    }
}

/// Um item de espaço que aponta para uma conexão do `exemplo()`.
fn aba_salva(id: u64, nome: &str, host: &str, user: &str) -> ItemSalvo {
    ItemSalvo {
        id: Some(id),
        nome: nome.into(),
        host: host.into(),
        user: user.into(),
        tmux: None,
        screen: None,
        assistentes: Vec::new(),
    }
}

/// Grava, na pasta do app, três espaços: dois na raiz e um na subpasta "Produção".
fn semear_espacos(pasta: &Pasta) {
    let espacos = Espacos::new(pasta.caminho());
    let um_painel = |abas: Vec<ItemSalvo>| {
        Espaco::new(
            Origem::Janela,
            vec![JanelaSalva {
                grade: (1, 1),
                paineis: vec![abas],
                posicao: None,
                tamanho: None,
                cluster_ia: None,
                laterais: None,
            }],
        )
    };
    let lado_a_lado = Espaco::new(
        Origem::Janela,
        vec![JanelaSalva {
            grade: (2, 1),
            paineis: vec![
                vec![aba_salva(2, "web-01", "10.0.0.2", "root")],
                vec![aba_salva(3, "db-01", "10.0.0.3", "root")],
            ],
            posicao: None,
            tamanho: None,
            cluster_ia: None,
            laterais: None,
        }],
    );
    espacos.criar_pasta("", "Produção").unwrap();
    espacos
        .gravar(
            "",
            "diário",
            &um_painel(vec![aba_salva(2, "web-01", "10.0.0.2", "root")]),
            false,
        )
        .unwrap();
    espacos
        .gravar("", "lado a lado", &lado_a_lado, false)
        .unwrap();
    espacos
        .gravar(
            "Produção",
            "plantão",
            &um_painel(vec![aba_salva(3, "db-01", "10.0.0.3", "root")]),
            false,
        )
        .unwrap();
}

#[test]
fn a_arvore_com_conexoes_e_espacos_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "espacos-arvore-claro"),
        (Theme::Dark, "espacos-arvore-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        pasta.semear(&exemplo());
        semear_espacos(&pasta);
        let mut m = montar(&pasta, tema);
        m.app.expand("arvore", "1");
        m.app.expand("arvore", "espaco:raiz");
        m.app.expand("arvore", "espaco:pasta:Produção");
        m.app.click("arvore-espaco:item:diário");
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
    }
}

/// O selo do estado no canto do ícone (M62): conectada, caiu e encerrada, e a conexão sem sessão
/// sem selo. O "conectando" não tem como aparecer aqui (o transporte falso conecta na hora); ele
/// fica nos snapshots da lib.
#[test]
fn a_arvore_com_o_estado_das_conexoes_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "estado-conexoes-claro"),
        (Theme::Dark, "estado-conexoes-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        pasta.semear(&exemplo());
        let mut m = montar(&pasta, tema);
        m.app.expand("arvore", "1");
        m.app.double_click("arvore-2");
        m.app.double_click("arvore-3");
        m.app.double_click("arvore-4");
        m.app.frame();
        m.opener.derrubar(3, "tempo esgotado");
        let roteador = m.opener.sessoes.borrow()[2].0;
        m.opener.handle_de(roteador).unwrap().close(None);
        m.app.frame();
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
        drop(m);
        encerrar_sessoes();
    }
}

#[test]
fn a_janela_secundaria_com_dois_paineis_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "janela-secundaria-claro"),
        (Theme::Dark, "janela-secundaria-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        pasta.semear(&exemplo());
        semear_espacos(&pasta);
        let mut m = montar(&pasta, tema);
        m.app.expand("arvore", "espaco:raiz");
        m.app.context_menu("arvore-espaco:item:lado a lado");
        m.app.click("menu-espaco:item:lado a lado-nova-janela");
        m.app.frame();
        assert_eq!(m.app.windows().len(), 2);
        m.app.switch_to(m.app.windows()[1]);
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
        drop(m);
        encerrar_sessoes();
    }
}

#[test]
fn o_dialogo_de_salvar_o_espaco_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "espacos-salvar-claro"),
        (Theme::Dark, "espacos-salvar-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        pasta.semear(&exemplo());
        semear_espacos(&pasta);
        let mut m = montar(&pasta, tema);
        m.app.expand("arvore", "1");
        m.app.double_click("arvore-2").double_click("arvore-3");
        m.app.click("menus-janela").click("salvar-janela-espaco");
        assert!(m.app.is_open("salvar-espaco-dialogo"));
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
        drop(m);
        encerrar_sessoes();
    }
}

#[test]
fn o_abrir_em_com_o_painel_3_em_realce_nos_dois_temas() {
    for (tema, nome) in [
        (Theme::Light, "abrir-em-claro"),
        (Theme::Dark, "abrir-em-escuro"),
    ] {
        let pasta = Pasta::nova(nome);
        pasta.semear(&exemplo());
        let mut m = montar(&pasta, tema);
        m.app.resize(1000, 760);
        m.app.expand("arvore", "1");
        m.app.click("menus-exibir").click("layout-2x2");
        m.app.double_click("arvore-2").double_click("arvore-3");
        m.app.context_menu("arvore-2");
        m.app.hover("menu-2-abrir-em");
        m.app.wait(std::time::Duration::from_millis(450));
        m.app.hover("menu-2-abrir-em-painel-2");
        m.app.assert_lint();
        m.app.assert_snapshot(nome);
        drop(m);
        encerrar_sessoes();
    }
}

fn propriedades_do_shell_local(nome: &str, tema: Theme) {
    use tirech_omni::modelo::{Connection, Kind, Tree};
    let pasta = Pasta::nova(&format!("props-local-{nome}"));
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("Shell", Kind::Local));
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("arvore-1");
    comum::escolher_propriedade(&mut m.app, "Pasta inicial");
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("props-shell-local-{nome}"));
    drop(m);
    encerrar_sessoes();
}

#[test]
fn as_propriedades_do_shell_local_no_tema_claro() {
    propriedades_do_shell_local("claro", Theme::Light);
}

#[test]
fn as_propriedades_do_shell_local_no_tema_escuro() {
    propriedades_do_shell_local("escuro", Theme::Dark);
}
