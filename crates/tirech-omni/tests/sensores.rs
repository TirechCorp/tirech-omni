//! A aba Sensores da página Desempenho, com sensores de mentira: os grupos que existem, os que
//! somem, a lista com a cor dos limites, as ventoinhas que giraram, a máquina sem sensores e a
//! aba que some quando este computador não tem nenhum e não há sessão SSH. E a temperatura no
//! medidor da barra de título, com o item do menu dele, e o aviso de calor.

mod comum;

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::configuracoes::Configuracoes;
use tirech_omni::processos::{FonteDeTeste, Foto, PONTOS, foto_de_exemplo};
use tirech_omni::sensores::{Sensores, exemplo};
use tomni_ui::TOOLTIP_DELAY;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_com_ambiente};

const LISTA: &str = "desempenho-sensores-lista";
const TEMPERATURA: &str = "barra-de-titulo-temperatura";

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

/// A foto `n` da fonte de exemplo com `sensores`.
fn foto(n: u64, sensores: &Sensores) -> Foto {
    let mut foto = foto_de_exemplo(n);
    foto.sensores = sensores.clone();
    foto
}

/// A tela com `aqui` como os sensores deste computador e a fonte `fonte`, na página Desempenho.
fn abrir(nome: &str, tema: Theme, aqui: Sensores, fonte: FonteDeTeste) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    let (mut m, _) = montar_com_ambiente(&pasta, tema, move |a| {
        a.sensores = Arc::new(move || aqui.clone());
        a.processos = Rc::new(move |_| Box::new(fonte.clone()));
    });
    m.app.click("barra-de-titulo-cpu");
    (pasta, m)
}

/// A tela com os sensores de exemplo aqui e na fonte, na aba Sensores, depois da primeira
/// leitura.
fn na_aba(nome: &str, quente: bool) -> (Pasta, Montagem) {
    let sensores = exemplo(quente);
    let da_fonte = sensores.clone();
    let fonte = FonteDeTeste::nova(move |n| Ok(foto(n, &da_fonte)));
    let (pasta, mut m) = abrir(nome, Theme::Light, sensores, fonte);
    m.app.click("desempenho-secoes-sensores");
    esperar(&mut m.app, "a lista", |a| !a.grid(LISTA).rows.is_empty());
    (pasta, m)
}

/// As células da linha `chave` da lista.
fn celulas(app: &TestApp, chave: &str) -> Vec<String> {
    app.grid(LISTA)
        .rows
        .into_iter()
        .find(|l| l.key == chave)
        .unwrap_or_else(|| panic!("a linha {chave}"))
        .cells
}

/// O tom da linha `chave` da lista.
fn tom(app: &TestApp, chave: &str) -> Option<String> {
    app.grid(LISTA)
        .rows
        .into_iter()
        .find(|l| l.key == chave)
        .unwrap_or_else(|| panic!("a linha {chave}"))
        .tone
}

#[test]
fn sem_sensores_aqui_e_sem_servidor_a_aba_some() {
    let (_pasta, m) = abrir(
        "sensores-sem-aba",
        Theme::Light,
        Sensores::default(),
        FonteDeTeste::exemplo(),
    );
    assert!(m.app.visible("desempenho-secoes-rede"));
    assert!(!m.app.visible("desempenho-secoes-sensores"));
}

#[test]
fn so_os_grupos_que_existem_aparecem() {
    let (_pasta, m) = na_aba("sensores-grupos", false);
    let app = &m.app;
    assert_eq!(
        app.text("desempenho-sensores-processador"),
        "Processador de Teste 8 núcleos"
    );
    for grupo in ["cpu", "memoria", "disco", "placa-mae"] {
        assert!(
            app.visible(&format!("desempenho-sensores-{grupo}")),
            "{grupo}"
        );
    }
    for grupo in ["video", "outros"] {
        assert!(
            app.tree()
                .find(&format!("desempenho-sensores-{grupo}"))
                .is_none(),
            "{grupo}"
        );
    }
    assert!(!app.visible("desempenho-sensores-vazio"));
    // O item da CPU mostra o pacote, e os números embaixo, os limites dele.
    assert_eq!(app.text("desempenho-sensores-cpu-valor"), "86 °C");
    assert_eq!(app.text("desempenho-sensores-agora"), "86 °C");
    assert_eq!(app.text("desempenho-sensores-alto"), "80 °C");
    assert_eq!(app.text("desempenho-sensores-critico"), "100 °C");
    assert_eq!(app.text("desempenho-sensores-memoria-valor"), "51 °C");
}

#[test]
fn a_lista_tem_os_sensores_do_grupo_com_a_cor_do_limite() {
    let (_pasta, mut m) = na_aba("sensores-lista", false);
    assert_eq!(
        celulas(&m.app, "coretemp-0"),
        ["Pacote", "86 °C", "80 °C", "100 °C", "coretemp"]
    );
    assert_eq!(celulas(&m.app, "coretemp-1")[0], "Núcleo 0");
    // O pacote passou do alto; o núcleo, não.
    assert_eq!(tom(&m.app, "coretemp-0").as_deref(), Some("caution"));
    assert_eq!(tom(&m.app, "coretemp-1"), None);

    m.app.click("desempenho-sensores-memoria");
    assert_eq!(m.app.grid(LISTA).keys(), ["spd5118-0", "spd5118-1"]);
    assert_eq!(celulas(&m.app, "spd5118-1")[0], "Pente 2");
    assert_eq!(m.app.text("desempenho-sensores-agora"), "51 °C");
}

#[test]
fn na_placa_mae_so_as_ventoinhas_que_giraram() {
    let (_pasta, mut m) = na_aba("sensores-ventoinhas", false);
    m.app.click("desempenho-sensores-placa-mae");
    assert_eq!(
        m.app.grid(LISTA).keys(),
        ["nct6798-0", "nct6798-1", "nct6798-ventoinha-1"]
    );
    assert_eq!(
        celulas(&m.app, "nct6798-0"),
        ["Placa-mãe", "43 °C", "—", "—", "nct6798"]
    );
    assert_eq!(celulas(&m.app, "nct6798-1")[0], "Soquete da CPU");
    let ventoinha = celulas(&m.app, "nct6798-ventoinha-1");
    assert_eq!(ventoinha[0], "Ventoinha 1");
    assert!(ventoinha[1].ends_with(" RPM"), "{ventoinha:?}");
}

#[test]
fn acima_do_critico_a_linha_fica_vermelha() {
    let (_pasta, m) = na_aba("sensores-quente", true);
    assert_eq!(tom(&m.app, "coretemp-0").as_deref(), Some("critical"));
    assert_eq!(m.app.text("desempenho-sensores-cpu-valor"), "101 °C");
}

#[test]
fn uma_maquina_sem_sensores_mostra_so_a_linha() {
    let (_pasta, mut m) = abrir(
        "sensores-vazio",
        Theme::Light,
        exemplo(false),
        FonteDeTeste::exemplo(),
    );
    m.app.click("desempenho-secoes-sensores");
    esperar(&mut m.app, "a primeira leitura", |a| {
        a.text("desempenho-sensores-vazio") != "Lendo…"
    });
    assert_eq!(
        m.app.text("desempenho-sensores-vazio"),
        "Esta máquina não mostra sensores de temperatura."
    );
    assert!(m.app.visible("desempenho-sensores-vazio"));
    assert!(m.app.tree().find("desempenho-sensores-cpu").is_none());
    assert!(!m.app.visible(LISTA));
}

#[test]
fn snapshot_da_aba_sensores_escuro() {
    // A fonte entrega as fotos que enchem o gráfico e para, para a imagem sair sempre igual.
    let ultima = PONTOS as u64;
    let parou = Arc::new(AtomicBool::new(false));
    let p = Arc::clone(&parou);
    let sensores = exemplo(false);
    let da_fonte = sensores.clone();
    let fonte = FonteDeTeste::nova(move |n| {
        if n > ultima {
            // O leitor não é esperado ao ser descartado: esta thread só dorme até o fim.
            p.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_secs(60));
        }
        let mut s = da_fonte.clone();
        // O pacote sobe e desce, para a curva não ser uma reta.
        s.temperaturas[0].mili = 70_000 + i32::try_from(n.min(ultima) % 7).unwrap() * 3_000;
        Ok(foto(n.min(ultima), &s))
    });
    let (_pasta, mut m) = abrir("sensores-snapshot", Theme::Dark, sensores, fonte);
    m.app.click("desempenho-secoes-sensores");
    esperar(&mut m.app, "a última foto", |_| {
        parou.load(Ordering::SeqCst)
    });
    m.app.frame();
    m.app.assert_snapshot("desempenho-sensores-escuro");
}

/// A tela com os sensores de `aqui` (que o teste troca) e as opções `geral` gravadas antes.
fn com_sensores(
    nome: &str,
    aqui: &Arc<Mutex<Sensores>>,
    geral: &[(&str, Value)],
) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    for (campo, valor) in geral {
        pasta.configurar("geral", campo, valor.clone());
    }
    let aqui = Arc::clone(aqui);
    let da_fonte = Arc::clone(&aqui);
    let fonte = FonteDeTeste::nova(move |n| Ok(foto(n, &da_fonte.lock().unwrap())));
    let (m, _) = montar_com_ambiente(&pasta, Theme::Light, move |a| {
        a.sensores = Arc::new(move || aqui.lock().unwrap().clone());
        a.processos = Rc::new(move |_| Box::new(fonte.clone()));
    });
    (pasta, m)
}

/// Os sensores de exemplo, trocáveis.
fn trocaveis(quente: bool) -> Arc<Mutex<Sensores>> {
    Arc::new(Mutex::new(exemplo(quente)))
}

/// A dica sob o ponteiro em `id`.
fn dica(app: &mut TestApp, id: &str) -> Option<String> {
    app.hover(id);
    app.wait(TOOLTIP_DELAY + Duration::from_millis(10));
    let arvore = app.tree();
    let no = arvore.find(&format!("{id}-tooltip"))?;
    no.children.first().and_then(|texto| texto.text.clone())
}

/// Espera o número do medidor mostrar `graus`.
fn esperar_a_temperatura(app: &mut TestApp, graus: &str) {
    esperar(app, graus, |a| {
        a.tree().find(TEMPERATURA).is_some() && a.text(TEMPERATURA) == graus
    });
}

#[test]
fn o_medidor_mostra_a_temperatura_da_cpu() {
    let (_pasta, mut m) = com_sensores("sensores-medidor", &trocaveis(false), &[]);
    esperar_a_temperatura(&mut m.app, "86 °C");
    assert!(m.app.visible(TEMPERATURA));
    let dica = dica(&mut m.app, "barra-de-titulo-cpu").expect("a dica");
    assert!(dica.ends_with("\nTemperatura da CPU: 86 °C"), "{dica:?}");
    m.app.assert_lint();
}

#[test]
fn sem_sensor_o_medidor_fica_so_com_os_quadradinhos() {
    let (_pasta, mut m) = com_sensores(
        "sensores-medidor-sem",
        &Arc::new(Mutex::new(Sensores::default())),
        &[],
    );
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.visible("barra-de-titulo-cpu"));
    assert!(m.app.tree().find(TEMPERATURA).is_none());
}

#[test]
fn o_menu_do_medidor_esconde_a_temperatura() {
    let (pasta, mut m) = com_sensores("sensores-medidor-menu", &trocaveis(false), &[]);
    esperar_a_temperatura(&mut m.app, "86 °C");
    m.app.right_click("barra-de-titulo-cpu");
    assert_eq!(
        m.app.label("barra-de-titulo-cpu-temperatura"),
        "Mostrar a temperatura"
    );
    assert!(m.app.checked("barra-de-titulo-cpu-temperatura"));
    m.app.click("barra-de-titulo-cpu-temperatura");
    m.app.frame();
    assert!(m.app.tree().find(TEMPERATURA).is_none());
    let gravada = Configuracoes::carregar(&Armazenamento::new(pasta.caminho()))
        .0
        .geral
        .medidor_temperatura;
    assert!(!gravada);

    m.app.right_click("barra-de-titulo-cpu");
    assert!(!m.app.checked("barra-de-titulo-cpu-temperatura"));
    m.app.click("barra-de-titulo-cpu-temperatura");
    esperar_a_temperatura(&mut m.app, "86 °C");
}

#[test]
fn acima_do_limite_vem_um_aviso_de_calor() {
    let aqui = trocaveis(false);
    let (_pasta, mut m) = com_sensores("sensores-calor", &aqui, &[]);
    // 86 °C, abaixo do automático (o crítico de 100 °C menos 10): nada.
    esperar_a_temperatura(&mut m.app, "86 °C");
    m.app.wait(Duration::from_millis(200));
    assert!(m.app.notifications().is_empty());

    *aqui.lock().unwrap() = exemplo(true);
    esperar(&mut m.app, "o aviso", |a| !a.notifications().is_empty());
    let [aviso] = m.app.notifications().try_into().expect("um aviso");
    assert_eq!(aviso.title, "A CPU esquentou");
    assert_eq!(
        aviso.body,
        "O processador deste computador está a 101 °C (o aviso é acima de 90 °C)."
    );
    assert_eq!(aviso.tag.as_deref(), Some("calor"));
    assert_eq!(m.app.text(TEMPERATURA), "101 °C");

    // O clique abre a aba Sensores deste computador.
    m.app.notification_click("A CPU esquentou");
    assert!(m.app.visible("desempenho-secoes"));
    assert_eq!(m.app.text("desempenho-secoes"), "sensores");
    esperar(&mut m.app, "a lista", |a| !a.grid(LISTA).rows.is_empty());

    // Continua quente: o aviso não volta.
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.notifications().is_empty());
}

#[test]
fn o_limite_das_configuracoes_vale_no_lugar_do_automatico() {
    let (_pasta, mut m) = com_sensores(
        "sensores-calor-limite",
        &trocaveis(false),
        &[("limite_calor", json!(80))],
    );
    esperar(&mut m.app, "o aviso", |a| !a.notifications().is_empty());
    let [aviso] = m.app.notifications().try_into().expect("um aviso");
    assert_eq!(
        aviso.body,
        "O processador deste computador está a 86 °C (o aviso é acima de 80 °C)."
    );
}

#[test]
fn com_o_aviso_e_a_temperatura_desligados_nada_le() {
    let (_pasta, mut m) = com_sensores(
        "sensores-desligados",
        &trocaveis(true),
        &[
            ("avisar_calor", json!(false)),
            ("medidor_temperatura", json!(false)),
        ],
    );
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.notifications().is_empty());
    assert!(m.app.tree().find(TEMPERATURA).is_none());
    assert!(m.app.visible("barra-de-titulo-cpu"));
}
