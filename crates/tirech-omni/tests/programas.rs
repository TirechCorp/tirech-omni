//! Os programas gráficos sem aba (M37): o virt-manager remoto e local, o aplicativo remoto, os
//! avisos no alto da janela e os recentes.

mod comum;

use std::time::Duration;

use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::{RemoteAppStatus, X11Mode};
use tomni_ui::theme::Theme;

use tirech_omni::abrir::FakeOpener;

use comum::{Montagem, Pasta, montar, montar_com};

/// Um pouco mais que o intervalo com que a tela relê os programas.
const RELEITURA: Duration = Duration::from_millis(1300);

fn encerrar() {
    for id in 0..=9 {
        for n in 1..=8 {
            if let Some(s) = Session::find(&format!("{id}-{n}")) {
                s.dispose();
            }
        }
    }
}

fn tela(nome: &str) -> (Pasta, Montagem) {
    let mut tree = Tree::default();
    let mut c = Connection::new("servidor1", Kind::Ssh);
    c.host = "servidor1.exemplo".into();
    c.user = "deploy".into();
    c.key_file = Some("/chaves/web".into());
    tree.add_connection(None, c);
    let pasta = Pasta::nova(nome);
    pasta.semear(&tree);
    let m = montar(&pasta, Theme::Light);
    (pasta, m)
}

fn existe(m: &mut Montagem, id: &str) -> bool {
    m.app.ui().find(id).is_some()
}

/// Espera `tempo` de verdade (a releitura dos programas é um alarme em thread) e roda um quadro.
fn esperar(m: &mut Montagem) {
    std::thread::sleep(RELEITURA);
    m.app.frame();
}

#[test]
fn o_virt_manager_remoto_sobe_com_x11_e_ganha_um_aviso() {
    let (_pasta, mut m) = tela("programas-virt");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-virt-manager");
    let lancados = m.opener.lancados.borrow().clone();
    assert_eq!(lancados.len(), 1);
    assert_eq!(lancados[0].conexao, 1);
    assert_eq!(lancados[0].comando, "virt-manager --no-fork");
    assert_eq!(
        lancados[0].encaminhamento.x11.as_ref().map(|(m, _)| *m),
        Some(X11Mode::Trusted)
    );
    assert!(m.app.visible("programa-1"));
    assert!(
        m.app
            .text("programa-1")
            .contains("virt-manager em servidor1")
    );
    encerrar();
}

#[test]
fn o_programa_que_termina_bem_tira_o_aviso() {
    let (_pasta, mut m) = tela("programas-fim");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-virt-manager");
    m.opener.lancados.borrow()[0]
        .lancamento
        .definir_status(RemoteAppStatus::Exited(Some(0)));
    esperar(&mut m);
    assert!(!existe(&mut m, "programa-1"));
    encerrar();
}

#[test]
fn programa_que_nao_existe_diz_que_nao_esta_instalado() {
    let (_pasta, mut m) = tela("programas-127");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-virt-manager");
    {
        let lancados = m.opener.lancados.borrow();
        lancados[0]
            .lancamento
            .definir_stderr("bash: virt-manager: command not found\n");
        lancados[0]
            .lancamento
            .definir_status(RemoteAppStatus::Exited(Some(127)));
    }
    esperar(&mut m);
    assert_eq!(
        m.app.text("programa-1"),
        "O virt-manager não está instalado em servidor1."
    );
    encerrar();
}

#[test]
fn sem_display_o_aviso_manda_conferir_o_x11() {
    let (_pasta, mut m) = tela("programas-display");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-virt-manager");
    {
        let lancados = m.opener.lancados.borrow();
        lancados[0]
            .lancamento
            .definir_stderr("Error: cannot open display: :0\n");
        lancados[0]
            .lancamento
            .definir_status(RemoteAppStatus::Exited(Some(1)));
    }
    esperar(&mut m);
    let texto = m.app.text("programa-1");
    assert!(texto.contains("X11Forwarding yes"), "{texto}");
    assert!(texto.contains("xauth"), "{texto}");
    encerrar();
}

#[test]
fn sem_permissao_no_libvirt_o_aviso_cita_o_grupo() {
    let (_pasta, mut m) = tela("programas-libvirt");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-virt-manager");
    {
        let lancados = m.opener.lancados.borrow();
        lancados[0]
            .lancamento
            .definir_stderr("authentication unavailable: no polkit agent\n");
        lancados[0]
            .lancamento
            .definir_status(RemoteAppStatus::Exited(Some(1)));
    }
    esperar(&mut m);
    assert!(m.app.text("programa-1").contains("grupo `libvirt`"));
    encerrar();
}

#[test]
fn fechar_o_aviso_fecha_o_programa() {
    let (_pasta, mut m) = tela("programas-fechar");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-virt-manager");
    assert!(!m.opener.lancados.borrow()[0].lancamento.fechado());
    let area = tomni_ui::InfoBar::close_area(m.app.rect("programa-1"));
    m.app.click_at(area.center(), 1);
    assert!(m.opener.lancados.borrow()[0].lancamento.fechado());
    assert!(!existe(&mut m, "programa-1"));
    encerrar();
}

#[test]
fn o_virt_manager_local_abre_pelo_abridor() {
    let mut tree = Tree::default();
    let mut c = Connection::new("servidor1", Kind::Ssh);
    c.host = "servidor1.exemplo".into();
    c.user = "deploy".into();
    c.key_file = Some("/chaves/web".into());
    tree.add_connection(None, c);
    let pasta = Pasta::nova("programas-local");
    pasta.semear(&tree);
    // O abridor já nasce com o virt-manager instalado: o menu lê isso ao ser montado.
    let opener = FakeOpener::new();
    opener.tem_virt_manager_local.set(true);
    let (app, _fila) = montar_com(&pasta, Theme::Light, opener.clone());
    let mut m = Montagem {
        app,
        opener,
        fila: _fila,
    };
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-virt-manager-local");
    assert_eq!(m.opener.virts_locais.borrow().len(), 1);
    assert_eq!(m.opener.virts_locais.borrow()[0].conexao, 1);
    assert!(m.opener.lancados.borrow().is_empty());
    assert!(m.app.text("programa-1").contains("virt-manager local"));
    encerrar();
}

#[test]
fn executar_um_aplicativo_remoto_lanca_registra_o_recente_e_reabre_pelo_inicio() {
    let (pasta, mut m) = tela("programas-executar");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-executar");
    assert!(m.app.visible("executar-dialogo"));
    m.app.click("executar-comando").type_text("gitk --all");
    m.app.click("executar-dialogo-primary");
    {
        let lancados = m.opener.lancados.borrow();
        assert_eq!(lancados.len(), 1);
        assert_eq!(lancados[0].comando, "gitk --all");
        assert!(lancados[0].encaminhamento.x11.is_some());
    }
    assert!(m.app.text("programa-1").contains("gitk em servidor1"));
    let recentes =
        std::fs::read_to_string(pasta.armazenamento().arquivo_recentes()).unwrap_or_default();
    assert!(recentes.contains("gitk --all"), "{recentes}");

    // Pelo Início, o recente lança de novo, sem aba.
    m.app.click("navegacao-inicio");
    assert_eq!(m.app.label("inicio-recente-0"), "gitk em servidor1");
    m.app.click("inicio-recente-0");
    assert_eq!(m.opener.lancados.borrow().len(), 2);
    assert!(m.opener.comandos_em_aba.borrow().is_empty());
    encerrar();
}

#[test]
fn a_saida_numa_aba_abre_o_comando_como_sessao() {
    let (_pasta, mut m) = tela("programas-aba");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-executar");
    m.app.click("executar-comando").type_text("xclock");
    m.app.click("executar-aba");
    m.app.click("executar-dialogo-primary");
    assert_eq!(
        *m.opener.comandos_em_aba.borrow(),
        [(1, "xclock".to_owned())]
    );
    assert!(m.opener.lancados.borrow().is_empty());
    encerrar();
}
