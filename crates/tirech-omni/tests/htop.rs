//! O `htop-omni`: as abas do Desempenho numa janela sem o resto do Omni, com a fonte de teste
//! (três processos, quatro núcleos e três interfaces). Abre na aba Processos, só com este
//! computador (sem o seletor de máquina) e sem a aba "Este app"; a aba Sensores só aparece com
//! sensores; encerrar chega à fonte; a saúde dos discos cabe na aba Disco; e passa no lint nos
//! dois temas.

use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tirech_omni::configuracoes::Idioma;
use tirech_omni::i18n;
use tirech_omni::processos::{FonteDeTeste, foto_de_exemplo};
use tirech_omni::saude_discos::{Diagnostico, ler_saida};
use tirech_omni::sensores::{Sensores, exemplo};
use tirech_omni::tela::htop;
use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

const TABELA: &str = "desempenho-processos-tabela";

/// A janela com a fonte de teste e os sensores `sensores`, já com a primeira leitura na tabela.
fn abrir(fonte: &FonteDeTeste, sensores: Sensores, tema: Theme) -> TestApp {
    i18n::fixar(Idioma::PtBr);
    let f = fonte.clone();
    let mut app = TestApp::new(move || {
        htop::tela_com(
            Rc::new(move |_| Box::new(f.clone())),
            Duration::from_millis(50),
            Arc::new(move || sensores.clone()),
            Arc::new(|_| Diagnostico::OutroSistema),
            true,
            false,
        )
    })
    .size(1100, 720)
    .theme(tema);
    esperar(&mut app, "a primeira leitura", |a| {
        a.grid(TABELA).rows.len() == 3
    });
    app
}

/// A fonte de exemplo com `sensores` em cada leitura (a lista da aba Sensores vem da leitura).
fn com_sensores(sensores: &Sensores) -> FonteDeTeste {
    let sensores = sensores.clone();
    FonteDeTeste::nova(move |n| {
        let mut foto = foto_de_exemplo(n);
        foto.sensores = sensores.clone();
        Ok(foto)
    })
}

/// Espera de verdade (o leitor anda numa thread): dorme um pouco e desenha um quadro, até
/// `pronto` ou 10 s.
fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        thread::sleep(Duration::from_millis(20));
        app.frame();
    }
}

#[test]
fn abre_na_aba_processos_so_com_este_computador() {
    let fonte = FonteDeTeste::exemplo();
    let mut app = abrir(&fonte, Sensores::default(), Theme::Light);
    assert_eq!(
        app.tab_titles("desempenho-secoes"),
        ["Processos", "Disco", "Rede"],
        "sem a aba do Omni e, sem sensores, sem a Sensores"
    );
    assert_eq!(app.ui().title().as_deref(), Some(htop::NOME));
    // Só há este computador: o seletor de máquina não aparece.
    assert!(app.ui().find("desempenho-processos-maquina").is_none());
    assert!(app.visible("desempenho-processos-filtro"));
}

#[test]
fn com_sensores_a_aba_sensores_aparece() {
    let sensores = exemplo(false);
    let mut app = abrir(&com_sensores(&sensores), sensores, Theme::Dark);
    assert_eq!(
        app.tab_titles("desempenho-secoes"),
        ["Processos", "Disco", "Rede", "Sensores"]
    );
    app.click("desempenho-secoes-sensores");
    esperar(&mut app, "a lista", |a| {
        !a.grid("desempenho-sensores-lista").rows.is_empty()
    });
    assert!(app.visible("desempenho-sensores-lista"));
}

#[test]
fn encerrar_chega_a_fonte() {
    let fonte = FonteDeTeste::exemplo();
    let mut app = abrir(&fonte, Sensores::default(), Theme::Light);
    app.click(&format!("{TABELA}-900"));
    app.click("desempenho-processos-encerrar");
    esperar(&mut app, "o pedido", |_| fonte.pedidos().len() == 1);
    app.click(&format!("{TABELA}-400"));
    app.key(Key::Delete);
    esperar(&mut app, "o do Delete", |_| fonte.pedidos().len() == 2);
    assert_eq!(fonte.pedidos(), [(900, false), (400, false)]);
}

#[test]
fn passa_no_lint_nos_dois_temas() {
    for tema in [Theme::Light, Theme::Dark] {
        let sensores = exemplo(false);
        let mut app = abrir(&com_sensores(&sensores), sensores, tema);
        for aba in ["processos", "disco", "rede", "sensores"] {
            app.click(&format!("desempenho-secoes-{aba}"));
            app.frame();
            app.assert_lint();
        }
    }
}

/// A janela com `diagnostico` como a leitura dos discos deste computador, já na aba Disco.
fn na_aba_disco(diagnostico: Diagnostico, tema: Theme) -> TestApp {
    na_aba_disco_com(Arc::new(move |_| diagnostico.clone()), false, tema)
}

/// A janela já na aba Disco, com a leitura dos discos e a prova do `sudo` dadas.
fn na_aba_disco_com(
    ler: Arc<dyn Fn(bool) -> Diagnostico + Send + Sync>,
    sudo_provado: bool,
    tema: Theme,
) -> TestApp {
    i18n::fixar(Idioma::PtBr);
    let fonte = FonteDeTeste::exemplo();
    let mut app = TestApp::new(move || {
        htop::tela_com(
            Rc::new(move |_| Box::new(fonte.clone())),
            Duration::from_millis(50),
            Arc::new(Sensores::default),
            Arc::clone(&ler),
            true,
            sudo_provado,
        )
    })
    .size(1100, 720)
    .theme(tema);
    esperar(&mut app, "a primeira leitura", |a| {
        a.grid(TABELA).rows.len() == 3
    });
    app.click("desempenho-secoes-disco");
    app
}

#[test]
fn o_sudo_so_vem_do_botao_ou_de_quando_ja_funcionou() {
    for (provado, esperado) in [(false, [false, true]), (true, [true, true])] {
        let pedidos: Arc<Mutex<Vec<bool>>> = Arc::default();
        let anota = Arc::clone(&pedidos);
        let mut app = na_aba_disco_com(
            Arc::new(move |com_sudo| {
                anota.lock().unwrap().push(com_sudo);
                Diagnostico::SemPermissao
            }),
            provado,
            Theme::Light,
        );
        esperar(&mut app, "a primeira leitura", |_| {
            pedidos.lock().unwrap().len() == 1
        });
        esperar(&mut app, "o botão", |a| {
            a.tree().find("desempenho-disco-saude-verificar").is_some()
        });
        app.click("desempenho-disco-saude-verificar");
        esperar(&mut app, "a segunda leitura", |_| {
            pedidos.lock().unwrap().len() == 2
        });
        assert_eq!(*pedidos.lock().unwrap(), esperado);
    }
}

#[test]
fn a_saude_dos_discos_aparece_na_aba_disco_e_cabe_na_janela() {
    let saida = r#"@tirech-discos discos
@disco /dev/nvme0 nvme
{"model_name":"SSD Rápido","smart_status":{"passed":true},"power_on_time":{"hours":26280},
 "power_cycle_count":412,"nvme_smart_health_information_log":{"critical_warning":0,
 "available_spare":100,"available_spare_threshold":10,"percentage_used":7,
 "data_units_written":2000000,"media_errors":0}}
@fim
"#;
    for tema in [Theme::Light, Theme::Dark] {
        let mut app = na_aba_disco(ler_saida(saida), tema);
        esperar(&mut app, "o resumo", |a| {
            a.tree().find("desempenho-disco-saude-detalhes").is_some()
        });
        app.assert_lint();
        app.click("desempenho-disco-saude-detalhes");
        esperar(&mut app, "a tabela", |a| {
            a.grid("desempenho-disco-saude").rows.len() == 1
        });
        app.assert_lint();
    }
}
