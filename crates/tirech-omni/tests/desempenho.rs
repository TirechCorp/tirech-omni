//! A página Desempenho e o medidor de CPU da barra de título, com uma fonte de leituras fixa: o
//! relógio e o uso de cada aba são os que o teste põe, e cada batida pede uma amostra.

mod comum;

use std::rc::Rc;
use std::time::{Duration, Instant};

use tirech_omni::desempenho::{FonteFixa, LeituraDaAba, Medicao, UsoDoProcesso};
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::TOOLTIP_DELAY;
use tomni_ui::files::Live;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_com_ambiente};

/// Duas conexões SSH na raiz, com chave: 1 `web` e 2 `db`.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    for (nome, host) in [("web", "web.exemplo"), ("db", "db.exemplo")] {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.host = host.into();
        c.user = "ana".into();
        c.key_file = Some("/chaves/ana".into());
        tree.add_connection(None, c);
    }
    tree
}

/// A tela, a fonte das leituras e a batida que pede uma amostra.
struct Cena {
    _pasta: Pasta,
    m: Montagem,
    fonte: Rc<FonteFixa>,
    batida: Live<u64>,
}

impl Cena {
    fn nova(nome: &str, tema: Theme, config: &[(&str, &str, serde_json::Value)]) -> Self {
        let pasta = Pasta::nova(nome);
        pasta.semear(&arvore());
        pasta.pagina_inicial_padrao("terminais");
        for (categoria, campo, valor) in config {
            pasta.configurar(categoria, campo, valor.clone());
        }
        let fonte = Rc::new(FonteFixa::nova());
        let batida = Live::detached(0_u64);
        let (f, b) = (Rc::clone(&fonte), batida.clone());
        let (mut m, _) = montar_com_ambiente(&pasta, tema, move |a| {
            a.medicao = Medicao::de_teste(f, b);
        });
        m.app.wait(Duration::from_millis(10));
        Self {
            _pasta: pasta,
            m,
            fonte,
            batida,
        }
    }

    /// Pede uma amostra.
    fn bater(&mut self) {
        self.batida.update(|n| *n += 1);
        self.m.app.wait(Duration::from_millis(10));
    }

    /// Abre a conexão `id` pela árvore e devolve a chave da aba nova.
    fn abrir(&mut self, id: u64) -> String {
        let antes = abas(&self.m.app);
        self.m.app.double_click(&format!("arvore-{id}"));
        let limite = Instant::now() + Duration::from_secs(10);
        while abas(&self.m.app).len() == antes.len() {
            assert!(Instant::now() < limite, "a aba nova não abriu");
            self.m.app.wait(Duration::from_millis(20));
        }
        abas(&self.m.app)
            .into_iter()
            .find(|c| !antes.contains(c))
            .expect("a aba nova")
    }

    /// Duas abas abertas e um segundo de uso: o app gastou 1,6 s de CPU nos 4 núcleos (40%),
    /// `web` 0,4 s na interface e 0,4 s nas threads da conexão (20%) com 512 KB lidos, e `db`
    /// 0,12 s (3%). O resto (17%) é da interface. Devolve as chaves de `web` e `db`.
    fn um_segundo_de_uso(&mut self) -> (String, String) {
        let web = self.abrir(1);
        let db = self.abrir(2);
        // A leitura de partida: o relógio ainda não andou.
        self.fonte.processo(UsoDoProcesso {
            cpu: Duration::ZERO,
            memoria: 300 * 1024 * 1024,
        });
        self.bater();
        self.fonte.andar(Duration::from_secs(1));
        self.fonte.processo(UsoDoProcesso {
            cpu: Duration::from_millis(1600),
            memoria: 300 * 1024 * 1024,
        });
        self.fonte.aba(
            &web,
            LeituraDaAba {
                ocupado: Duration::from_millis(400),
                bytes: 512 * 1024,
                threads: vec![(7, Duration::from_millis(400))],
                programa: None,
            },
        );
        self.fonte.aba(
            &db,
            LeituraDaAba {
                ocupado: Duration::from_millis(120),
                bytes: 0,
                threads: Vec::new(),
                programa: None,
            },
        );
        self.bater();
        (web, db)
    }
}

/// Todas as chaves de aba, na ordem dos painéis.
fn abas(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()))
        .collect()
}

/// A aba à mostra no painel ativo.
fn aba_escolhida(app: &TestApp) -> Option<String> {
    let paineis = app.panes("paineis");
    paineis.panes[paineis.active].selected.clone()
}

/// O texto da dica de `id`, depois do tempo de espera do ponteiro parado.
fn dica(app: &mut TestApp, id: &str) -> Option<String> {
    app.hover(id);
    app.wait(TOOLTIP_DELAY + Duration::from_millis(10));
    let arvore = app.tree();
    let no = arvore.find(&format!("{id}-tooltip"))?;
    no.children.first().and_then(|texto| texto.text.clone())
}

#[test]
fn o_medidor_mostra_a_cpu_do_app_e_a_dica() {
    let mut c = Cena::nova("desempenho-medidor", Theme::Light, &[]);
    assert!(c.m.app.visible("barra-de-titulo-cpu"));
    assert_eq!(c.m.app.value("barra-de-titulo-cpu"), 0.0);
    c.um_segundo_de_uso();
    assert!((c.m.app.value("barra-de-titulo-cpu") - 0.4).abs() < 1e-9);
    assert_eq!(
        dica(&mut c.m.app, "barra-de-titulo-cpu").as_deref(),
        Some("CPU do Tirech Omni: 40%")
    );
}

#[test]
fn desligado_nas_configuracoes_o_medidor_some() {
    let mut c = Cena::nova(
        "desempenho-desligado",
        Theme::Light,
        &[("geral", "medidor_cpu", serde_json::Value::Bool(false))],
    );
    assert!(!c.m.app.visible("barra-de-titulo-cpu"));
    // Sem o medidor e com a página fechada, nada é medido.
    c.um_segundo_de_uso();
    c.m.app.click("menus-exibir").click("desempenho");
    assert!(c.m.app.visible("desempenho-pagina"));
    assert_eq!(c.m.app.text("desempenho-utilizacao"), "0%");
    // Com a página aberta, mede: mais um segundo como o primeiro.
    c.fonte.andar(Duration::from_secs(1));
    c.fonte.processo(UsoDoProcesso {
        cpu: Duration::from_millis(3200),
        memoria: 300 * 1024 * 1024,
    });
    c.bater();
    assert_eq!(c.m.app.text("desempenho-utilizacao"), "40%");
}

#[test]
fn o_clique_no_medidor_e_o_menu_exibir_abrem_a_pagina() {
    let mut c = Cena::nova("desempenho-abrir", Theme::Light, &[]);
    assert!(!c.m.app.visible("desempenho-pagina"));
    c.m.app.click("barra-de-titulo-cpu");
    assert!(c.m.app.visible("desempenho-pagina"));
    c.m.app.click("navegacao-terminais");
    assert!(!c.m.app.visible("desempenho-pagina"));
    c.m.app.click("menus-exibir").click("desempenho");
    assert!(c.m.app.visible("desempenho-pagina"));
    c.m.app.assert_lint();
}

#[test]
fn a_tabela_pesa_cada_aba_e_a_interface() {
    let mut c = Cena::nova("desempenho-tabela", Theme::Light, &[]);
    let (web, db) = c.um_segundo_de_uso();
    c.m.app.click("barra-de-titulo-cpu");
    let tabela = c.m.app.grid("desempenho-terminais");
    // Pela CPU, do maior ao menor.
    assert_eq!(tabela.keys(), [web.as_str(), "interface", db.as_str()]);
    let celulas = |chave: &str| {
        let linha = tabela
            .rows
            .iter()
            .find(|l| l.key == chave)
            .expect("a linha");
        linha.cells[1..5].to_vec()
    };
    assert_eq!(celulas(&web), ["web", "20%", "512,0 KB/s", "—"]);
    assert_eq!(celulas(&db), ["db", "3,0%", "0 B/s", "—"]);
    assert_eq!(celulas("interface"), ["O próprio app", "17%", "—", "—"]);
    assert_eq!(c.m.app.text("desempenho-utilizacao"), "40%");
    assert_eq!(c.m.app.text("desempenho-memoria-numero"), "300 MB");
    assert_eq!(c.m.app.text("desempenho-abas"), "2");
    assert_eq!(c.m.app.text("desempenho-aberto"), "0:01");
    assert!((c.m.app.value("desempenho-grafico-cpu") - 40.0).abs() < 1e-9);
}

#[test]
fn o_clique_duplo_na_linha_vai_para_a_aba() {
    let mut c = Cena::nova("desempenho-ir", Theme::Light, &[]);
    let (web, db) = c.um_segundo_de_uso();
    assert_eq!(aba_escolhida(&c.m.app).as_deref(), Some(db.as_str()));
    c.m.app.click("menus-exibir").click("desempenho");
    c.m.app.double_click(&format!("desempenho-terminais-{web}"));
    assert!(!c.m.app.visible("desempenho-pagina"));
    assert_eq!(aba_escolhida(&c.m.app).as_deref(), Some(web.as_str()));
}

#[test]
fn o_item_da_esquerda_troca_o_grafico() {
    let mut c = Cena::nova("desempenho-grafico", Theme::Light, &[]);
    c.um_segundo_de_uso();
    c.m.app.click("barra-de-titulo-cpu");
    assert!(c.m.app.visible("desempenho-grafico-cpu"));
    assert!(!c.m.app.visible("desempenho-grafico-rede"));
    c.m.app.click("desempenho-rede");
    assert!(c.m.app.visible("desempenho-grafico-rede"));
    assert!(!c.m.app.visible("desempenho-grafico-cpu"));
    assert!((c.m.app.value("desempenho-grafico-rede") - 512.0).abs() < 1e-9);
}

/// A página depois de alguns segundos de uso variado, para os gráficos terem linha.
fn pagina_nos_temas(tema: Theme, nome: &str) {
    let mut c = Cena::nova(&format!("desempenho-{nome}"), tema, &[]);
    let (web, db) = c.um_segundo_de_uso();
    // Por segundo: o app, `web` (interface e conexão), `db`, os bytes de `web` e a memória, em MB.
    let segundos: [(u64, u64, u64, u64, u64); 7] = [
        (900, 300, 100, 128, 302),
        (2200, 800, 300, 900, 305),
        (3100, 1200, 500, 2048, 310),
        (1800, 600, 400, 700, 311),
        (1200, 300, 200, 300, 309),
        (2600, 900, 600, 1500, 312),
        (1600, 800, 120, 512, 312),
    ];
    let (mut app, mut w, mut d, mut bytes) = (1600, 800, 120, 512 * 1024);
    for (no_app, na_web, no_db, kb, mb) in segundos {
        app += no_app;
        w += na_web;
        d += no_db;
        bytes += kb * 1024;
        c.fonte.andar(Duration::from_secs(1));
        c.fonte.processo(UsoDoProcesso {
            cpu: Duration::from_millis(app),
            memoria: mb * 1024 * 1024,
        });
        c.fonte.aba(
            &web,
            LeituraDaAba {
                ocupado: Duration::from_millis(w / 2),
                bytes,
                threads: vec![(7, Duration::from_millis(w - w / 2))],
                programa: None,
            },
        );
        c.fonte.aba(
            &db,
            LeituraDaAba {
                ocupado: Duration::from_millis(d),
                bytes: 0,
                threads: Vec::new(),
                programa: None,
            },
        );
        c.bater();
    }
    c.m.app.click("barra-de-titulo-cpu");
    c.m.app.assert_lint();
    c.m.app.assert_snapshot(&format!("desempenho-{nome}"));
}

#[test]
fn snapshot_da_pagina_claro() {
    pagina_nos_temas(Theme::Light, "claro");
}

#[test]
fn snapshot_da_pagina_escuro() {
    pagina_nos_temas(Theme::Dark, "escuro");
}
