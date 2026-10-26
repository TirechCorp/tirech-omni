//! A bandeja do sistema (M44): o ícone, o menu e a dica, esconder e mostrar as janelas, o
//! diálogo ao fechar a principal, o app sem bandeja e a abertura com `--bandeja`. A bandeja é a
//! do `TestApp` (a plataforma de verdade não entra nos testes).

mod comum;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tirech_omni::agente::ChaveSalva;
use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::configuracoes::{AoFechar, Configuracoes, PaginaInicial};
use tomni_ui::WindowHandle;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar, montar_com_ambiente};

fn dados(nome: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/dados")
        .join(nome)
}

/// Grava o `configuracoes.json` da pasta depois de `ajustar` mexer nas configurações padrão (na
/// página dos terminais, como a montagem de teste faz quando a pasta não tem o arquivo).
fn configurar(pasta: &Pasta, ajustar: impl FnOnce(&mut Configuracoes)) {
    let mut config = Configuracoes::default();
    config.geral.pagina_inicial = PaginaInicial::Terminais;
    ajustar(&mut config);
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    config.salvar(&pasta.armazenamento()).unwrap();
}

fn gravada(pasta: &Pasta) -> Configuracoes {
    Configuracoes::carregar(&Armazenamento::new(pasta.caminho())).0
}

/// Uma pasta com `geral.ao_fechar` = `como`.
fn pasta_com(nome: &str, como: AoFechar) -> Pasta {
    let pasta = Pasta::nova(&format!("bandeja-{nome}"));
    configurar(&pasta, |c| c.geral.ao_fechar = como);
    pasta
}

/// Passa quadros até `pronto` ser verdadeiro (as esperas do app voltam por threads).
fn esperar(app: &mut TestApp, o_que: &str, mut pronto: impl FnMut(&mut TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

fn principal_escondida(app: &TestApp) -> bool {
    app.window_hidden(WindowHandle::main())
}

fn itens(app: &TestApp) -> Vec<(String, String)> {
    app.tray()
        .expect("o app mostra a bandeja")
        .items
        .into_iter()
        .map(|i| (i.id, i.label))
        .collect()
}

fn item(app: &TestApp, id: &str) -> tomni_ui::tray::TrayItemState {
    app.tray()
        .expect("o app mostra a bandeja")
        .items
        .into_iter()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("a bandeja não tem {id}"))
}

fn dica(app: &TestApp) -> String {
    app.tray().expect("o app mostra a bandeja").tooltip
}

// --- O ícone, o menu e a dica ---

#[test]
fn o_menu_da_bandeja_tem_os_itens_na_ordem_e_a_dica() {
    let pasta = Pasta::nova("bandeja-menu");
    let m = montar(&pasta, Theme::Light);
    assert_eq!(
        itens(&m.app),
        [
            ("bandeja-abrir".to_owned(), "Abrir o Tirech Omni".to_owned()),
            ("bandeja-travar".to_owned(), "Travar o cofre".to_owned()),
            (String::new(), String::new()),
            ("bandeja-agente".to_owned(), "Agente: desligado".to_owned()),
            (String::new(), String::new()),
            ("bandeja-sair".to_owned(), "Sair".to_owned()),
        ]
    );
    assert_eq!(dica(&m.app), "Tirech Omni · agente desligado");
    assert!(!item(&m.app, "bandeja-agente").checked);
    assert!(item(&m.app, "bandeja-agente").toggle);
}

#[test]
fn o_agente_ligado_mostra_o_numero_de_chaves_no_menu_e_na_dica() {
    let pasta = Pasta::nova("bandeja-chaves");
    std::fs::create_dir_all(pasta.ssh()).unwrap();
    std::fs::copy(dados("id_agente"), pasta.ssh().join("id_agente")).unwrap();
    configurar(&pasta, |c| {
        c.agente.ligado = true;
        c.agente.chaves = vec![ChaveSalva::new(pasta.ssh().join("id_agente"))];
    });
    let m = montar(&pasta, Theme::Light);
    assert_eq!(dica(&m.app), "Tirech Omni · agente com 1 chave");
    let agente = item(&m.app, "bandeja-agente");
    assert_eq!(agente.label, "Agente: ligado · 1 chave");
    assert!(agente.checked);
}

#[test]
fn o_item_do_agente_liga_e_desliga_e_grava_a_escolha() {
    let pasta = Pasta::nova("bandeja-liga");
    let mut m = montar(&pasta, Theme::Light);
    m.app.tray_click("bandeja-agente");
    assert_eq!(
        item(&m.app, "bandeja-agente").label,
        "Agente: ligado · 0 chaves"
    );
    assert!(item(&m.app, "bandeja-agente").checked);
    assert_eq!(dica(&m.app), "Tirech Omni · agente com 0 chaves");
    assert!(gravada(&pasta).agente.ligado);

    m.app.tray_click("bandeja-agente");
    assert_eq!(item(&m.app, "bandeja-agente").label, "Agente: desligado");
    assert!(!item(&m.app, "bandeja-agente").checked);
    assert!(!gravada(&pasta).agente.ligado);
}

#[test]
fn travar_o_cofre_pela_bandeja() {
    let pasta = Pasta::nova("bandeja-travar");
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    // Com a senha mestra, o cofre abre trancado.
    assert!(!item(&m.app, "bandeja-travar").enabled);
    assert_eq!(dica(&m.app), "Tirech Omni · cofre bloqueado");

    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    assert!(!m.app.visible("cofre-bloqueado"));
    assert!(item(&m.app, "bandeja-travar").enabled);
    assert_eq!(dica(&m.app), "Tirech Omni · agente desligado");

    m.app.tray_click("bandeja-travar");
    m.app.click("navegacao-cofre");
    assert!(m.app.visible("cofre-bloqueado"));
    assert!(!item(&m.app, "bandeja-travar").enabled);
    assert_eq!(dica(&m.app), "Tirech Omni · cofre bloqueado");
}

#[test]
fn sem_senha_mestra_nao_ha_o_que_travar() {
    let pasta = Pasta::nova("bandeja-sem-mestra");
    let m = montar(&pasta, Theme::Light);
    assert!(!item(&m.app, "bandeja-travar").enabled);
}

// --- Esconder e mostrar ---

#[test]
fn abrir_e_o_clique_no_icone_mostram_o_app_escondido() {
    let pasta = pasta_com("mostrar", AoFechar::Bandeja);
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.close_window(WindowHandle::main()));
    assert!(principal_escondida(&m.app));
    assert!(!m.app.exited());

    m.app.tray_click("bandeja-abrir");
    assert!(!principal_escondida(&m.app));

    assert!(!m.app.close_window(WindowHandle::main()));
    assert!(principal_escondida(&m.app));
    m.app.tray_activate();
    assert!(!principal_escondida(&m.app));
}

#[test]
fn esconder_esconde_todas_as_janelas_e_mostrar_traz_todas() {
    let pasta = pasta_com("duas-escondidas", AoFechar::Bandeja);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(principal);
    assert!(!m.app.close_window(principal));
    assert!(m.app.window_hidden(principal) && m.app.window_hidden(outra));
    assert!(!m.app.is_open("fechar-tudo-dialogo"));
    assert!(!m.app.is_open("fechar-ou-bandeja-dialogo"));

    m.app.tray_click("bandeja-abrir");
    assert!(!m.app.window_hidden(principal) && !m.app.window_hidden(outra));
}

#[test]
fn fechar_com_o_app_escondido_encerra() {
    // O sistema pediu para fechar (desligar o computador) e ninguém está olhando.
    let pasta = pasta_com("fechar-escondido", AoFechar::Bandeja);
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.close_window(WindowHandle::main()));
    assert!(principal_escondida(&m.app));
    assert!(m.app.close_window(WindowHandle::main()));
    assert!(m.app.exited());
}

#[test]
fn ao_fechar_fechar_encerra_como_antes() {
    let pasta = pasta_com("fechar-fecha", AoFechar::Fechar);
    let mut m = montar(&pasta, Theme::Light);
    assert!(m.app.close_window(WindowHandle::main()));
    assert!(m.app.exited());
}

/// Pede ao agente da pasta, com o `ssh-keygen`, que assine algo com a chave `id_ed25519`.
fn pedir_assinatura(pasta: &Pasta) -> Option<std::process::Child> {
    Command::new("ssh-keygen").arg("-h").output().ok()?;
    let mut filho = Command::new("ssh-keygen")
        .args(["-Y", "sign", "-n", "teste", "-U", "-f"])
        .arg(pasta.ssh().join("id_ed25519.pub"))
        .env("SSH_AUTH_SOCK", pasta.caminho().join("agent.sock"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    use std::io::Write;
    let mut entrada = filho.stdin.take()?;
    entrada.write_all(b"conteudo para assinar\n").ok()?;
    drop(entrada);
    Some(filho)
}

#[test]
fn o_pedido_de_uso_com_o_app_escondido_mostra_so_a_principal_e_esconde_depois() {
    let pasta = Pasta::nova("bandeja-uso");
    std::fs::create_dir_all(pasta.ssh()).unwrap();
    for nome in ["id_ed25519", "id_ed25519.pub"] {
        std::fs::copy(dados(nome), pasta.ssh().join(nome)).unwrap();
    }
    configurar(&pasta, |c| {
        c.geral.ao_fechar = AoFechar::Bandeja;
        c.agente.ligado = true;
        c.agente.chaves = vec![ChaveSalva {
            confirmar: true,
            ..ChaveSalva::new(pasta.ssh().join("id_ed25519"))
        }];
    });
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(principal);
    assert!(!m.app.close_window(principal));
    assert!(m.app.window_hidden(principal) && m.app.window_hidden(outra));

    let Some(filho) = pedir_assinatura(&pasta) else {
        eprintln!("sem ssh-keygen: teste pulado");
        return;
    };
    esperar(&mut m.app, "o diálogo de uso abrir", |a| {
        a.is_open("agente-uso-dialogo")
    });
    assert!(!m.app.window_hidden(principal), "a principal voltou");
    assert!(
        m.app.window_hidden(outra),
        "a outra janela continua escondida"
    );

    m.app.click("agente-uso-dialogo-primary");
    assert!(filho.wait_with_output().unwrap().status.success());
    esperar(&mut m.app, "a principal esconder de novo", |a| {
        a.window_hidden(principal)
    });
}

// --- Sair ---

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar(chaves: &[&str]) {
    for chave in chaves {
        if let Some(s) = Session::find(chave) {
            s.dispose();
        }
    }
}

#[test]
fn sair_pela_bandeja_encerra_sem_perguntar_com_sessoes_e_duas_janelas() {
    let pasta = Pasta::nova("bandeja-sair");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").double_click("arvore-2");
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    assert_eq!(m.app.windows().len(), 2);

    m.app.tray_click("bandeja-sair");
    assert!(m.app.exited());
    assert!(!m.app.is_open("fechar-tudo-dialogo"));
    assert!(!m.app.is_open("fechar-ou-bandeja-dialogo"));
    encerrar(&["2-1"]);
}

#[test]
fn arquivo_sair_encerra_sem_perguntar() {
    let pasta = Pasta::nova("bandeja-arquivo-sair");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    m.app.switch_to(WindowHandle::main());
    m.app.click("menus-arquivo").click("sair");
    assert!(m.app.exited());
    assert!(!m.app.is_open("fechar-tudo-dialogo"));
    assert!(!m.app.is_open("fechar-ou-bandeja-dialogo"));
}

// --- O diálogo ao fechar ---

fn diálogo_aberto(m: &Montagem) -> bool {
    m.app.is_open("fechar-ou-bandeja-dialogo")
}

#[test]
fn fechar_a_principal_pergunta_e_o_texto_segue_o_sistema() {
    let pasta = Pasta::nova("bandeja-pergunta");
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.close_window(WindowHandle::main()));
    assert!(diálogo_aberto(&m));
    assert_eq!(
        m.app.text("fechar-ou-bandeja-texto"),
        "O agente de chaves para se o app fechar. Você pode deixá-lo na bandeja e continuar \
         usando as chaves."
    );
    assert!(!m.app.visible("fechar-ou-bandeja-janelas"));
    assert!(!m.app.checked("fechar-ou-bandeja-nao-perguntar"));
    assert!(!m.app.is_open("fechar-tudo-dialogo"));
    m.app.assert_lint();
}

#[test]
fn no_windows_com_o_pageant_o_texto_fala_do_pageant() {
    let pasta = Pasta::nova("bandeja-pageant");
    configurar(&pasta, |c| c.agente.pageant = true);
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| a.windows = true);
    assert!(!m.app.close_window(WindowHandle::main()));
    assert!(
        m.app
            .text("fechar-ou-bandeja-texto")
            .starts_with("O agente de chaves e o Pageant param se o app fechar."),
        "{}",
        m.app.text("fechar-ou-bandeja-texto")
    );
}

#[test]
fn deixar_na_bandeja_esconde_e_nao_grava_sem_a_caixa() {
    let pasta = Pasta::nova("bandeja-deixar");
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.close_window(WindowHandle::main()));
    m.app.click("fechar-ou-bandeja-dialogo-primary");
    assert!(principal_escondida(&m.app));
    assert!(!m.app.exited());
    assert!(!diálogo_aberto(&m));
    assert_eq!(gravada(&pasta).geral.ao_fechar, AoFechar::Perguntar);
}

#[test]
fn fechar_pelo_dialogo_encerra_o_app() {
    let pasta = Pasta::nova("bandeja-fechar");
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.close_window(WindowHandle::main()));
    m.app.click("fechar-ou-bandeja-dialogo-secondary");
    assert!(m.app.exited());
    assert_eq!(gravada(&pasta).geral.ao_fechar, AoFechar::Perguntar);
}

#[test]
fn cancelar_nao_fecha_nem_grava_mesmo_com_a_caixa_marcada() {
    let pasta = Pasta::nova("bandeja-cancelar");
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.close_window(WindowHandle::main()));
    m.app.click("fechar-ou-bandeja-nao-perguntar");
    assert!(m.app.checked("fechar-ou-bandeja-nao-perguntar"));
    m.app.click("fechar-ou-bandeja-dialogo-close");
    assert!(!diálogo_aberto(&m));
    assert!(!principal_escondida(&m.app) && !m.app.exited());
    assert_eq!(gravada(&pasta).geral.ao_fechar, AoFechar::Perguntar);
}

#[test]
fn nao_perguntar_com_deixar_na_bandeja_vira_a_configuracao() {
    let pasta = Pasta::nova("bandeja-nao-pergunta-bandeja");
    let mut m = montar(&pasta, Theme::Light);
    assert!(!m.app.close_window(WindowHandle::main()));
    m.app.click("fechar-ou-bandeja-nao-perguntar");
    m.app.click("fechar-ou-bandeja-dialogo-primary");
    assert_eq!(gravada(&pasta).geral.ao_fechar, AoFechar::Bandeja);

    // Da próxima vez esconde direto.
    m.app.tray_click("bandeja-abrir");
    assert!(!m.app.close_window(WindowHandle::main()));
    assert!(!diálogo_aberto(&m));
    assert!(principal_escondida(&m.app));
}

#[test]
fn nao_perguntar_com_fechar_vira_a_configuracao() {
    let pasta = Pasta::nova("bandeja-nao-pergunta-fechar");
    {
        let mut m = montar(&pasta, Theme::Light);
        assert!(!m.app.close_window(WindowHandle::main()));
        m.app.click("fechar-ou-bandeja-nao-perguntar");
        m.app.click("fechar-ou-bandeja-dialogo-secondary");
        assert!(m.app.exited());
    }
    assert_eq!(gravada(&pasta).geral.ao_fechar, AoFechar::Fechar);
    let mut m = montar(&pasta, Theme::Light);
    assert!(m.app.close_window(WindowHandle::main()));
    assert!(m.app.exited());
}

#[test]
fn com_duas_janelas_o_dialogo_toma_o_lugar_do_fechar_tudo() {
    let pasta = Pasta::nova("bandeja-duas");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(principal);
    assert!(!m.app.close_window(principal));
    assert!(diálogo_aberto(&m));
    assert!(!m.app.is_open("fechar-tudo-dialogo"));
    assert_eq!(
        m.app.text("fechar-ou-bandeja-janelas"),
        "Há 2 janelas abertas. Fechar fecha todas elas."
    );
    m.app.assert_lint();

    // Cancelar deixa tudo como está.
    m.app.click("fechar-ou-bandeja-dialogo-close");
    assert!(outra.is_open() && !m.app.exited());

    // Fechar fecha as duas, sem outra pergunta.
    assert!(!m.app.close_window(principal));
    m.app.click("fechar-ou-bandeja-dialogo-secondary");
    assert!(m.app.exited());
    assert!(!m.app.is_open("fechar-tudo-dialogo"));
}

#[test]
fn com_duas_janelas_deixar_na_bandeja_esconde_as_duas() {
    let pasta = Pasta::nova("bandeja-duas-esconder");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(principal);
    assert!(!m.app.close_window(principal));
    m.app.click("fechar-ou-bandeja-dialogo-primary");
    assert!(m.app.window_hidden(principal) && m.app.window_hidden(outra));
    assert!(!m.app.exited());
}

#[test]
fn fechar_uma_secundaria_so_fecha_ela() {
    let pasta = Pasta::nova("bandeja-secundaria");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let [principal, outra] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(outra);
    assert!(m.app.close_window(outra));
    assert!(!outra.is_open() && principal.is_open());
    assert!(!m.app.is_open("fechar-ou-bandeja-dialogo"));
    assert!(!m.app.exited());
}

// --- Sem bandeja ---

#[test]
fn sem_bandeja_fechar_encerra_sem_dialogo_e_o_aviso_vem_uma_vez() {
    let pasta = Pasta::nova("bandeja-sem");
    {
        let mut m = montar(&pasta, Theme::Light);
        m.app.set_tray_available(false);
        assert!(m.app.tray().is_none());
        esperar(&mut m.app, "o aviso aparecer", |a| {
            a.visible("sem-bandeja-aviso")
        });
        assert!(
            m.app
                .text("sem-bandeja-aviso")
                .contains("extensão AppIndicator"),
        );
        m.app.assert_lint();
        m.app.click("sem-bandeja-entendi");
        assert!(!m.app.visible("sem-bandeja-aviso"));
        assert!(gravada(&pasta).geral.aviso_sem_bandeja);

        assert!(m.app.close_window(WindowHandle::main()));
        assert!(m.app.exited());
        assert!(!m.app.is_open("fechar-ou-bandeja-dialogo"));
    }
    // Num app novo, com a mesma pasta, o aviso não volta.
    let mut m = montar(&pasta, Theme::Light);
    m.app.set_tray_available(false);
    for _ in 0..20 {
        m.app.wait(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));
    m.app.wait(Duration::from_millis(20));
    assert!(!m.app.visible("sem-bandeja-aviso"));
}

#[test]
fn com_bandeja_nao_ha_aviso() {
    let pasta = Pasta::nova("bandeja-com");
    let mut m = montar(&pasta, Theme::Light);
    std::thread::sleep(Duration::from_millis(250));
    m.app.wait(Duration::from_millis(20));
    assert!(!m.app.visible("sem-bandeja-aviso"));
}

// --- --bandeja ---

#[test]
fn aberto_com_bandeja_o_app_comeca_escondido() {
    let pasta = Pasta::nova("bandeja-argumento");
    pasta.configurar("geral", "iniciar_na_bandeja", true.into());
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| a.bandeja_ao_abrir = true);
    assert!(principal_escondida(&m.app));
    std::thread::sleep(Duration::from_millis(250));
    m.app.wait(Duration::from_millis(20));
    assert!(
        principal_escondida(&m.app),
        "com bandeja, continua escondido"
    );
    m.app.tray_activate();
    assert!(!principal_escondida(&m.app));
}

#[test]
fn aberto_pelo_sistema_sem_ficar_na_bandeja_a_janela_abre() {
    let pasta = Pasta::nova("bandeja-argumento-janela");
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| a.bandeja_ao_abrir = true);
    assert!(!principal_escondida(&m.app), "o padrão é abrir a janela");
    std::thread::sleep(Duration::from_millis(250));
    m.app.wait(Duration::from_millis(20));
    assert!(!principal_escondida(&m.app));
}

#[test]
fn aberto_com_bandeja_mas_sem_bandeja_no_sistema_a_janela_volta() {
    let pasta = Pasta::nova("bandeja-argumento-sem");
    pasta.configurar("geral", "iniciar_na_bandeja", true.into());
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| a.bandeja_ao_abrir = true);
    assert!(principal_escondida(&m.app));
    m.app.set_tray_available(false);
    esperar(&mut m.app, "a janela voltar", |a| !principal_escondida(a));
}
