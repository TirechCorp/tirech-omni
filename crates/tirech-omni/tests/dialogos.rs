//! Os diálogos do `tirech-omni`: a senha, a chave do servidor e o seletor da importação.

mod comum;

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tirech_omni::abrir::{Senhas, ler_fila_agora, perguntar};
use tomni_ui::terminal::KnownHost;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, exemplo, montar, so_com_senha};

const FINGERPRINT: &str = "SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

fn abas(app: &TestApp) -> usize {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.len())
        .sum()
}

/// Espera a resposta de uma pergunta feita de outra thread, rodando frames enquanto isso.
fn esperar(app: &mut TestApp, resposta: &mpsc::Receiver<bool>) -> bool {
    let limite = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(r) = resposta.try_recv() {
            return r;
        }
        assert!(Instant::now() < limite, "a pergunta não foi respondida");
        app.wait(Duration::from_millis(20));
    }
}

/// Faz a pergunta da chave de outra thread, como o `russh` faz, e devolve onde a resposta sai.
fn perguntar_de_outra_thread(m: &comum::Montagem, conhecida: KnownHost) -> mpsc::Receiver<bool> {
    let (envia, recebe) = mpsc::channel();
    let fila = m.fila;
    thread::spawn(move || {
        let r = perguntar(
            fila,
            "web-01",
            "web-01.exemplo:22",
            "ssh-ed25519",
            FINGERPRINT,
            conhecida,
        );
        let _ = envia.send(r);
    });
    recebe
}

fn esperar_dialogo(app: &mut TestApp, id: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !app.is_open(id) {
        assert!(Instant::now() < limite, "o diálogo {id} não abriu");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn ssh_sem_chave_pede_a_senha_e_sem_salvar_ela_so_vale_para_a_abertura() {
    let pasta = Pasta::nova("senha");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert_eq!(abas(&m.app), 0, "ainda não abriu");
    assert!(
        m.app
            .text("senha-destino")
            .contains("ana@sem-chave.exemplo")
    );

    // A caixa "Salvar a senha" vem marcada; desmarcada, a senha só vale para esta abertura.
    m.app.click("senha-salvar");
    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-primary");
    assert!(!m.app.is_open("senha-dialogo"));
    assert_eq!(abas(&m.app), 1);
    assert_eq!(
        *m.opener.senhas.borrow(),
        [Senhas {
            conexao: Some("segredo".to_owned()),
            salto: None,
            chave: None,
            token: None,
            chave_cofre: None,
            rede: None,
        }]
    );

    // A senha não vai para nenhum arquivo da pasta.
    for entrada in std::fs::read_dir(pasta.caminho()).unwrap() {
        let texto = std::fs::read_to_string(entrada.unwrap().path()).unwrap_or_default();
        assert!(!texto.contains("segredo"), "a senha foi gravada");
    }

    // Da próxima vez, pede de novo (e o campo vem vazio).
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert_eq!(m.app.text("senha-campo"), "");
    m.app.assert_lint();
}

#[test]
fn cancelar_a_senha_nao_abre_aba() {
    let pasta = Pasta::nova("senha-cancelar");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-close");
    assert!(!m.app.is_open("senha-dialogo"));
    assert_eq!(abas(&m.app), 0);
    assert!(m.opener.abertas.borrow().is_empty());

    m.app.double_click("arvore-1");
    m.app.key(tomni_ui::Key::Escape);
    assert_eq!(abas(&m.app), 0, "Esc também cancela");
}

#[test]
fn senha_vazia_tenta_so_a_chave_do_agente() {
    let pasta = Pasta::nova("senha-vazia");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app
        .double_click("arvore-1")
        .click("senha-dialogo-primary");
    assert_eq!(*m.opener.senhas.borrow(), [Senhas::default()]);
    assert_eq!(abas(&m.app), 1);
}

#[test]
fn chave_desconhecida_pergunta_e_confiar_responde_sim() {
    let pasta = Pasta::nova("chave-nova");
    let mut m = montar(&pasta, Theme::Light);
    let resposta = perguntar_de_outra_thread(&m, KnownHost::Unknown);
    esperar_dialogo(&mut m.app, "chave-dialogo");
    assert_eq!(m.app.text("chave-servidor"), "Servidor: web-01.exemplo:22");
    assert!(m.app.text("chave-algoritmo").contains("ssh-ed25519"));
    assert!(m.app.text("chave-fingerprint").contains(FINGERPRINT));
    m.app.assert_lint();
    m.app.click("chave-dialogo-primary");
    assert!(esperar(&mut m.app, &resposta));
    assert!(!m.app.is_open("chave-dialogo"));
}

#[test]
fn chave_desconhecida_cancelada_responde_nao() {
    let pasta = Pasta::nova("chave-cancelar");
    let mut m = montar(&pasta, Theme::Light);
    let resposta = perguntar_de_outra_thread(&m, KnownHost::Unknown);
    esperar_dialogo(&mut m.app, "chave-dialogo");
    m.app.click("chave-dialogo-close");
    assert!(!esperar(&mut m.app, &resposta));
}

#[test]
fn chave_mudada_avisa_e_so_deixa_fechar() {
    let pasta = Pasta::nova("chave-mudou");
    let mut m = montar(&pasta, Theme::Dark);
    let resposta = perguntar_de_outra_thread(&m, KnownHost::Changed { line: 3 });
    esperar_dialogo(&mut m.app, "chave-mudou-dialogo");
    assert!(m.app.visible("chave-mudou-aviso"));
    assert!(!m.app.is_open("chave-dialogo"));
    assert!(
        !m.app.visible("chave-dialogo-primary"),
        "sem o botão de confiar"
    );
    m.app.assert_lint();
    m.app.click("chave-mudou-dialogo-close");
    assert!(!esperar(&mut m.app, &resposta));
}

#[test]
fn chave_conhecida_passa_sem_dialogo() {
    let pasta = Pasta::nova("chave-conhecida");
    let m = montar(&pasta, Theme::Light);
    assert!(perguntar(
        m.fila,
        "web-01",
        "web-01.exemplo:22",
        "ssh-ed25519",
        FINGERPRINT,
        KnownHost::Known
    ));
    assert!(ler_fila_agora(m.fila, <[_]>::is_empty));
}

#[test]
fn os_pedidos_de_chave_entram_na_fila_um_de_cada_vez() {
    let pasta = Pasta::nova("chave-fila");
    let mut m = montar(&pasta, Theme::Light);
    let a = perguntar_de_outra_thread(&m, KnownHost::Unknown);
    let b = perguntar_de_outra_thread(&m, KnownHost::Unknown);
    esperar_dialogo(&mut m.app, "chave-dialogo");
    m.app.click("chave-dialogo-primary");
    esperar_dialogo(&mut m.app, "chave-dialogo");
    m.app.click("chave-dialogo-close");
    let (ra, rb) = (esperar(&mut m.app, &a), esperar(&mut m.app, &b));
    assert!(ra ^ rb, "um confiou e o outro recusou: {ra} {rb}");
}

#[test]
fn importar_pede_o_config_em_ssh_e_agrupa_o_que_leu() {
    let pasta = Pasta::nova("importar");
    let mut m = montar(&pasta, Theme::Light);
    let config = pasta.caminho().join("config-exemplo");
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    std::fs::write(
        &config,
        "Host web\n  HostName web.exemplo\n  User deploy\n  Port 2222\nHost *\n  User ninguem\n",
    )
    .unwrap();
    m.app.click("menus-arquivo");
    m.app.hover("importar-menu");
    m.app.wait(std::time::Duration::from_millis(450));
    m.app.click("importar");
    let pedidos = m.app.file_dialogs();
    assert_eq!(pedidos.len(), 1);
    assert_eq!(pedidos[0].file_name.as_deref(), Some("config"));
    if let Some(home) = tirech_omni::armazenamento::pasta_pessoal() {
        assert_eq!(pedidos[0].directory, Some(home.join(".ssh")));
    }
    m.app.file_dialog_answer(Some(config));
    assert!(m.app.visible("aviso-sucesso") || m.app.visible("aviso-atencao"));
    let tree = pasta.conexoes();
    assert_eq!(tree.connection_count(), 1);
    let rotulos: Vec<String> = m
        .app
        .tree_items("arvore")
        .into_iter()
        .map(|r| r.label)
        .collect();
    assert_eq!(rotulos[0], "Importadas do ~/.ssh/config");
    assert_eq!(rotulos[1], "web (deploy@web.exemplo)");
    let c = tree.connections()[0];
    assert_eq!(c.port, 2222);
}

#[test]
fn cancelar_o_seletor_nao_muda_nada() {
    let pasta = Pasta::nova("importar-cancelar");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-arquivo");
    m.app.hover("importar-menu");
    m.app.wait(std::time::Duration::from_millis(450));
    m.app.click("importar");
    m.app.file_dialog_cancel();
    assert_eq!(pasta.conexoes(), exemplo());
}
