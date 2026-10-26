//! "Instalar no computador" (M65): o diálogo, as marcas, o modo desinstalar e o "Aplicar" de
//! verdade numa pasta temporária (Linux). O que depende do Windows (`.lnk`, registro) é testado
//! em `instalacao.rs` (módulo) e na máquina virtual.

mod comum;

use std::path::Path;
use std::time::{Duration, Instant};

use tirech_omni::instalacao::{EstadoDoItem as Estado, ItemDeInstalacao as Item, Motivo};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, ambiente_windows_falso, montar_com_ambiente};

fn nada_instalado() -> Vec<(Item, Estado)> {
    vec![
        (Item::Copia, Estado::Ausente),
        (Item::AreaDeTrabalho, Estado::Ausente),
        (Item::Menu, Estado::Ausente),
        (Item::Barra, Estado::Ausente),
        (Item::Inicio, Estado::Ausente),
        (
            Item::Aplicativos,
            Estado::Indisponivel(Motivo::PrecisaDaCopia),
        ),
    ]
}

fn tudo_instalado() -> Vec<(Item, Estado)> {
    Item::TODOS
        .into_iter()
        .map(|i| (i, Estado::Presente))
        .collect()
}

fn windows_falso(nome: &str, estados: Vec<(Item, Estado)>) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    let (m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| {
        ambiente_windows_falso(a, estados);
    });
    (pasta, m)
}

/// Se o widget `id` existe e aparece.
fn no_ar(app: &mut TestApp, id: &str) -> bool {
    app.ui().find(id).is_some() && app.visible(id)
}

fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(15);
    while !condicao(&mut *app) && Instant::now() < limite {
        app.wait(Duration::from_millis(100));
    }
    assert!(condicao(&mut *app), "a condição não veio a tempo");
}

#[test]
fn o_item_do_menu_arquivo_abre_o_dialogo() {
    let (_pasta, mut m) = windows_falso("instalar-menu", nada_instalado());
    assert!(!m.app.is_open("instalar-dialogo"));
    m.app.click("menus-arquivo").click("instalar-no-computador");
    assert!(m.app.is_open("instalar-dialogo"));
}

#[test]
fn sem_nada_instalado_marca_a_copia_a_barra_e_os_aplicativos_e_nao_oferece_desinstalar() {
    let (_pasta, mut m) = windows_falso("instalar-marcas", nada_instalado());
    m.app.click("menus-arquivo").click("instalar-no-computador");
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.checked("instalar-copia"));
    assert!(m.app.checked("instalar-barra"));
    assert!(m.app.checked("instalar-aplicativos"));
    assert!(!m.app.checked("instalar-area"));
    assert!(!m.app.checked("instalar-inicio"));
    // A barra do Windows precisa do atalho no menu: ele vem marcado e travado.
    assert!(m.app.checked("instalar-menu"));
    assert!(!m.app.enabled("instalar-menu"));
    // Não há o que desinstalar.
    assert!(!m.app.visible("instalar-dialogo-secondary"));
}

#[test]
fn marcar_a_barra_marca_o_menu_e_trava_a_caixa_dele() {
    let (_pasta, mut m) = windows_falso("instalar-barra", nada_instalado());
    m.app.click("menus-arquivo").click("instalar-no-computador");
    m.app.wait(Duration::from_millis(300));
    // Desmarca a barra: o menu destrava e pode ser desmarcado.
    m.app.click("instalar-barra");
    assert!(!m.app.checked("instalar-barra"));
    assert!(m.app.enabled("instalar-menu"));
    m.app.click("instalar-menu");
    assert!(!m.app.checked("instalar-menu"));
    // Marca a barra de novo: o menu volta marcado e travado.
    m.app.click("instalar-barra");
    assert!(m.app.checked("instalar-menu"));
    assert!(!m.app.enabled("instalar-menu"));
}

#[test]
fn os_aplicativos_instalados_dependem_da_marca_da_copia() {
    let (_pasta, mut m) = windows_falso("instalar-aplicativos", nada_instalado());
    m.app.click("menus-arquivo").click("instalar-no-computador");
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.enabled("instalar-aplicativos"));
    m.app.click("instalar-copia");
    assert!(!m.app.enabled("instalar-aplicativos"));
}

#[test]
fn com_algo_instalado_o_desinstalar_vira_o_modo_de_remocao() {
    let (_pasta, mut m) = windows_falso("instalar-modo", tudo_instalado());
    m.app.click("menus-arquivo").click("instalar-no-computador");
    m.app.wait(Duration::from_millis(300));
    // Tudo já feito: tudo marcado, e o "Desinstalar…" aparece.
    assert!(m.app.checked("instalar-copia"));
    assert!(m.app.checked("instalar-area"));
    assert!(m.app.visible("instalar-dialogo-secondary"));
    assert!(!m.app.visible("instalar-dados-bloco"));
    m.app.click("instalar-dialogo-secondary");
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.is_open("instalar-dialogo"));
    // No modo desinstalar tudo vem desmarcado e aparece a caixa de apagar os dados.
    assert!(!m.app.checked("instalar-copia"));
    assert!(!m.app.checked("instalar-area"));
    assert!(!m.app.checked("instalar-barra"));
    assert!(m.app.visible("instalar-dados-bloco"));
    assert!(!m.app.checked("instalar-dados"));
    assert!(!m.app.visible("instalar-dialogo-secondary"));
}

#[test]
fn a_confirmacao_so_aparece_com_a_caixa_de_apagar_os_dados_marcada() {
    let (_pasta, mut m) = windows_falso("instalar-confirmar", tudo_instalado());
    m.app.click("menus-arquivo").click("instalar-no-computador");
    m.app.wait(Duration::from_millis(300));
    m.app.click("instalar-dialogo-secondary");
    m.app.wait(Duration::from_millis(300));
    m.app.click("instalar-dados");
    assert!(m.app.checked("instalar-dados"));
    m.app.click("instalar-dialogo-primary");
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.is_open("instalar-confirmar"));
    assert!(m.app.text("instalar-confirmar-texto").contains("tirech"));
    // Cancelar a confirmação não apaga nada (nem chega a aplicar).
    m.app.click("instalar-confirmar-close");
    assert!(!m.app.is_open("instalar-confirmar"));
}

/// Um Linux de mentira: tudo dentro da pasta temporária e o executável é um arquivo pequeno.
fn linux_na_pasta(pasta: &Pasta) -> impl FnOnce(&mut tirech_omni::ferramentas::Ambiente) {
    let raiz = pasta.0.join("casa");
    let exe = pasta.0.join("origem").join("tirech-omni");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::write(&exe, b"#!/bin/sh\nexit 0\n").unwrap();
    move |a| {
        a.windows = false;
        a.macos = false;
        a.pessoal = Some(raiz.clone());
        a.area_de_trabalho = Some(raiz.join("Desktop"));
        a.xdg_config = Some(raiz.join(".config"));
        a.xdg_data = Some(raiz.join(".local/share"));
        a.exe = Some(exe);
        a.desktop_atual = String::new();
    }
}

fn existe(caminho: &Path) -> bool {
    caminho.exists()
}

#[test]
fn aplicar_copia_o_app_poe_os_atalhos_e_mostra_o_resultado() {
    let pasta = Pasta::nova("instalar-aplicar");
    let ajuste = linux_na_pasta(&pasta);
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, ajuste);
    let casa = pasta.0.join("casa");
    std::fs::create_dir_all(casa.join("Desktop")).unwrap();
    m.app.click("menus-arquivo").click("instalar-no-computador");
    esperar(&mut m.app, |a| a.checked("instalar-copia"));
    m.app.click("instalar-area");
    m.app.click("instalar-dialogo-primary");
    esperar(&mut m.app, |a| no_ar(a, "instalar-aviso"));
    assert!(existe(&casa.join(".local/bin/tirech-omni")));
    assert!(existe(&casa.join("Desktop/tirech-omni.desktop")));
    // Agora há o que desinstalar.
    m.app.click("menus-arquivo").click("instalar-no-computador");
    m.app.wait(Duration::from_millis(300));
    assert!(m.app.visible("instalar-dialogo-secondary"));
}

#[test]
fn desinstalar_tira_a_copia_e_os_atalhos_e_nao_fecha_sem_apagar_os_dados() {
    let pasta = Pasta::nova("instalar-desinstalar");
    let ajuste = linux_na_pasta(&pasta);
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, ajuste);
    let casa = pasta.0.join("casa");
    std::fs::create_dir_all(casa.join("Desktop")).unwrap();
    m.app.click("menus-arquivo").click("instalar-no-computador");
    esperar(&mut m.app, |a| a.checked("instalar-copia"));
    m.app.click("instalar-area");
    m.app.click("instalar-dialogo-primary");
    esperar(&mut m.app, |a| no_ar(a, "instalar-aviso"));
    assert!(existe(&casa.join(".local/bin/tirech-omni")));

    m.app.click("menus-arquivo").click("instalar-no-computador");
    m.app.wait(Duration::from_millis(300));
    m.app.click("instalar-dialogo-secondary");
    m.app.wait(Duration::from_millis(300));
    // No modo desinstalar tudo vem desmarcado: "Aplicar" tira tudo.
    m.app.click("instalar-dialogo-primary");
    let copia = casa.join(".local/bin/tirech-omni");
    esperar(&mut m.app, |_| !copia.exists());
    assert!(!existe(&casa.join(".local/bin/tirech-omni")));
    assert!(!existe(&casa.join("Desktop/tirech-omni.desktop")));
    // Os dados ficaram: a caixa não estava marcada.
    assert!(existe(&pasta.0));
}

#[test]
fn aberto_com_desinstalar_o_dialogo_ja_vem_no_modo_de_remocao() {
    let pasta = Pasta::nova("instalar-flag");
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| {
        ambiente_windows_falso(a, tudo_instalado());
        a.desinstalar_ao_abrir = true;
    });
    esperar(&mut m.app, |a| no_ar(a, "instalar-dados-bloco"));
    assert!(m.app.is_open("instalar-dialogo"));
    assert!(!m.app.checked("instalar-copia"));
}

#[test]
fn ao_abrir_reaponta_o_atalho_que_ficou_com_um_executavel_que_sumiu() {
    use tirech_omni::ferramentas::Ambiente;
    use tirech_omni::instalacao::Contexto;

    let pasta = Pasta::nova("instalar-repontar");
    let casa = pasta.0.join("casa");
    std::fs::create_dir_all(casa.join("Desktop")).unwrap();
    let velho = pasta.0.join("velho/tirech-omni");
    let novo = pasta.0.join("novo/tirech-omni");
    for exe in [&velho, &novo] {
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(exe, b"#!/bin/sh\nexit 0\n").unwrap();
    }
    let arrumar = {
        let casa = casa.clone();
        move |a: &mut Ambiente, exe: std::path::PathBuf| {
            a.pessoal = Some(casa.clone());
            a.area_de_trabalho = Some(casa.join("Desktop"));
            a.xdg_config = Some(casa.join(".config"));
            a.xdg_data = Some(casa.join(".local/share"));
            a.exe = Some(exe);
        }
    };
    // O atalho nasce apontando para o executável velho, que depois some.
    let mut antes = Ambiente::de_teste(&pasta.0, pasta.ssh());
    arrumar(&mut antes, velho.clone());
    let ctx = Contexto::de(&antes);
    ctx.aplicar(Item::AreaDeTrabalho, true).unwrap();
    assert_eq!(ctx.apontamento(Item::AreaDeTrabalho), Some(velho.clone()));
    std::fs::remove_file(&velho).unwrap();

    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| {
        arrumar(a, novo.clone());
        a.repontar_ao_abrir = true;
    });
    esperar(&mut m.app, |_| {
        let mut conferir = Ambiente::de_teste(&pasta.0, pasta.ssh());
        arrumar(&mut conferir, novo.clone());
        Contexto::de(&conferir).apontamento(Item::AreaDeTrabalho) == Some(novo.clone())
    });
}

#[test]
fn ao_abrir_nao_recria_o_atalho_que_a_pessoa_apagou() {
    let pasta = Pasta::nova("instalar-nao-recria");
    let ajuste = linux_na_pasta(&pasta);
    let casa = pasta.0.join("casa");
    std::fs::create_dir_all(casa.join("Desktop")).unwrap();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| {
        ajuste(a);
        a.repontar_ao_abrir = true;
    });
    m.app.wait(Duration::from_millis(800));
    assert!(!casa.join("Desktop/tirech-omni.desktop").exists());
}
