//! A sessão persistente (tmux ou screen) nas abas SSH, com sessões `Loopback` no lugar das
//! conexões: o `FakeOpener` guarda o comando que cada aba rodaria no lugar do shell, e é ele que
//! os testes conferem (a aba no tmux, a reconexão, a cópia, a restauração e o fecho da aba). A
//! lista "Sessões no servidor…" lê o que o servidor falso responde ao `sh -s`.

mod comum;

use std::time::{Duration, Instant};

use serde_json::json;

use tirech_omni::modelo::{
    ComandosAoConectar, Connection, Espera, Kind, ModoComandos, Node, Passo, SessaoPersistente,
    Tree,
};
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::ExecOutput;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

fn abas(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()))
        .collect()
}

fn sessao(chave: &str) -> Session {
    Session::find(chave).expect("a sessão da aba")
}

/// Uma conexão SSH `web` (id 1, dentro da pasta "Servidores") com o comando ao conectar
/// `marca`. `na_conexao` é a escolha dela; `na_pasta`, a da pasta.
fn pasta_com(nome: &str, na_conexao: SessaoPersistente, na_pasta: SessaoPersistente) -> Pasta {
    let pasta = Pasta::nova(nome);
    let mut tree = Tree::default();
    let grupo = tree.add_group(None, "Servidores");
    if let Some(Node::Group(g)) = tree.find_mut(grupo) {
        g.persistente = na_pasta;
    }
    let mut c = Connection::new("web", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    c.persistente = na_conexao;
    let mut p = Passo::comando("marca", 5);
    p.espera = Espera::Logo;
    c.comandos = ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos: vec![p],
        reconectar: true,
    };
    let id = tree.add_connection(Some(grupo), c);
    assert_eq!(id, 2, "a conexão é o segundo item da árvore");
    pasta.semear(&tree);
    pasta.configurar("conexoes", "comandos_quieto_ms", json!(50));
    pasta.configurar("ia", "verificar_ao_abrir", json!(false));
    pasta
}

fn com_tmux(nome: &str) -> Pasta {
    pasta_com(nome, SessaoPersistente::Tmux, SessaoPersistente::Herdar)
}

/// Abre a conexão e devolve a chave da aba, depois dos comandos ao conectar.
fn abrir(m: &mut Montagem) -> String {
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    esperar(&mut m.app, "a aba", |a| abas(a).len() == 1);
    let chave = abas(&m.app)[0].clone();
    sessao(&chave).send(b"ana@web:~$ ");
    esperar(&mut m.app, "o comando ao conectar", |a| {
        a.session_text(&abas(a)[0]).contains("marca")
    });
    chave
}

/// O comando que a aba `chave` roda no lugar do shell.
fn comando_da(m: &Montagem, chave: &str) -> Option<String> {
    m.opener.handle_de(sessao(chave)).and_then(|h| h.command())
}

fn escrito(m: &Montagem, chave: &str) -> String {
    let handle = m
        .opener
        .handle_de(sessao(chave))
        .expect("a ponta da sessão");
    String::from_utf8_lossy(&handle.take_written()).into_owned()
}

fn encerrados(m: &Montagem) -> Vec<String> {
    m.opener
        .comandos_rodados()
        .into_iter()
        .filter(|c| c.starts_with("tmux kill-session") || c.contains(" -X quit"))
        .collect()
}

fn fechar(m: &mut Montagem, chave: &str) {
    m.app.close_tab("paineis-painel-0", chave);
    m.app.wait(Duration::from_millis(50));
}

#[test]
fn a_aba_roda_no_tmux_e_a_reconexao_volta_a_mesma_sessao() {
    let pasta = com_tmux("tmux-reconectar");
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    let comando = comando_da(&m, &chave).expect("a aba roda um comando");
    assert!(
        comando.contains("new-session -A -s tirech-2-1"),
        "{comando}"
    );

    let s = sessao(&chave);
    let _ = escrito(&m, &chave);
    m.opener.derrubar_sessao(s, "caiu");
    m.app.wait(Duration::from_millis(50));
    assert!(s.restart());
    m.app.wait(Duration::from_millis(300));
    assert_eq!(comando_da(&m, &chave).as_deref(), Some(comando.as_str()));
    assert!(
        !escrito(&m, &chave).contains("marca"),
        "os comandos ao conectar rodaram de novo dentro da mesma sessão do tmux"
    );
    s.dispose();
}

#[test]
fn sem_a_opcao_a_aba_abre_o_shell_comum() {
    let pasta = pasta_com(
        "tmux-desligado",
        SessaoPersistente::Herdar,
        SessaoPersistente::Herdar,
    );
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    assert_eq!(comando_da(&m, &chave), None);
    fechar(&mut m, &chave);
    assert!(!aparece(&m.app, "tmux-fechar-dialogo"));
    assert!(encerrados(&m).is_empty());
}

#[test]
fn a_conexao_herda_da_pasta_e_pode_recusar() {
    let pasta = pasta_com(
        "tmux-herda",
        SessaoPersistente::Herdar,
        SessaoPersistente::Tmux,
    );
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    assert!(comando_da(&m, &chave).is_some_and(|c| c.contains("tirech-2-1")));
    sessao(&chave).dispose();

    let pasta = pasta_com(
        "tmux-recusa",
        SessaoPersistente::Nenhuma,
        SessaoPersistente::Tmux,
    );
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    assert_eq!(comando_da(&m, &chave), None);
    sessao(&chave).dispose();
}

#[test]
fn duplicar_abre_outra_sessao_do_tmux() {
    let pasta = com_tmux("tmux-duplicar");
    let mut m = montar(&pasta, Theme::Light);
    let origem = abrir(&mut m);
    m.app.context_menu(&format!("paineis-painel-0-{origem}"));
    m.app.click("aba-duplicar");
    esperar(&mut m.app, "a cópia", |a| abas(a).len() == 2);
    let copia = abas(&m.app).into_iter().find(|c| *c != origem).unwrap();
    let comando = comando_da(&m, &copia).expect("a cópia roda no tmux");
    assert!(
        comando.contains("new-session -A -s tirech-2-2"),
        "{comando}"
    );
    for chave in abas(&m.app) {
        sessao(&chave).dispose();
    }
}

#[test]
fn o_estado_guarda_a_sessao_e_a_restauracao_volta_a_ela() {
    let pasta = com_tmux("tmux-estado");
    {
        let mut m = montar(&pasta, Theme::Light);
        let chave = abrir(&mut m);
        // A segunda aba é outra sessão: a restauração volta cada aba à sua.
        m.app.double_click("arvore-2");
        esperar(&mut m.app, "a segunda aba", |a| abas(a).len() == 2);
        let segunda = abas(&m.app).into_iter().find(|c| *c != chave).unwrap();
        assert!(comando_da(&m, &segunda).is_some_and(|c| c.contains("tirech-2-2")));
        // Fecha a primeira e mantém: na próxima vez, a aba nova reusa o nome livre.
        fechar(&mut m, &chave);
        m.app.click("tmux-fechar-dialogo-secondary");
        assert!(encerrados(&m).is_empty());
    }
    for chave in ["2-1", "2-2"] {
        if let Some(s) = Session::find(chave) {
            s.dispose();
        }
    }
    let estado = std::fs::read_to_string(pasta.caminho().join("estado.json")).unwrap();
    assert!(estado.contains("\"tmux\": \"tirech-2-2\""), "{estado}");
    assert!(!estado.contains("tirech-2-1"), "{estado}");

    let mut m = montar(&pasta, Theme::Light);
    esperar(&mut m.app, "a aba restaurada", |a| abas(a).len() == 1);
    let chave = abas(&m.app)[0].clone();
    let comando = comando_da(&m, &chave).expect("a aba restaurada roda no tmux");
    assert!(
        comando.contains("new-session -A -s tirech-2-2"),
        "{comando}"
    );
    sessao(&chave).send(b"ana@web:~$ ");
    m.app.wait(Duration::from_millis(300));
    assert!(
        !escrito(&m, &chave).contains("marca"),
        "os comandos ao conectar rodaram de novo na sessão restaurada"
    );
    // A aba nova da mesma conexão não pega o nome da restaurada.
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    esperar(&mut m.app, "a aba nova", |a| abas(a).len() == 2);
    let nova = abas(&m.app).into_iter().find(|c| *c != chave).unwrap();
    assert!(comando_da(&m, &nova).is_some_and(|c| c.contains("tirech-2-1")));
    for chave in abas(&m.app) {
        sessao(&chave).dispose();
    }
}

#[test]
fn fechar_a_aba_pergunta_e_encerrar_mata_a_sessao_no_servidor() {
    let pasta = com_tmux("tmux-encerrar");
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    let s = sessao(&chave);
    fechar(&mut m, &chave);
    assert!(aparece(&m.app, "tmux-fechar-dialogo"), "não perguntou");
    let texto = m.app.text("tmux-fechar-texto");
    assert!(texto.contains("tirech-2-1"), "{texto}");
    assert!(texto.contains("web"), "{texto}");
    m.app.click("tmux-fechar-dialogo-primary");
    assert!(!aparece(&m.app, "tmux-fechar-dialogo"));
    assert_eq!(encerrados(&m), ["tmux kill-session -t =tirech-2-1"]);
    assert!(untracked_closed(s), "a aba não foi descartada");
}

fn untracked_closed(s: Session) -> bool {
    tomni_ui::prelude::untrack(|| s.closed())
}

#[test]
fn manter_e_o_esc_deixam_a_sessao_no_servidor() {
    let pasta = com_tmux("tmux-manter");
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    fechar(&mut m, &chave);
    m.app.click("tmux-fechar-dialogo-secondary");
    assert!(encerrados(&m).is_empty());

    let chave = abrir(&mut m);
    fechar(&mut m, &chave);
    assert!(aparece(&m.app, "tmux-fechar-dialogo"));
    m.app.key(tomni_ui::Key::Escape);
    m.app.wait(Duration::from_millis(50));
    assert!(!aparece(&m.app, "tmux-fechar-dialogo"));
    assert!(encerrados(&m).is_empty(), "o Esc encerrou a sessão");
}

#[test]
fn lembrar_a_escolha_grava_a_configuracao() {
    let pasta = com_tmux("tmux-lembrar");
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    fechar(&mut m, &chave);
    m.app.click("tmux-fechar-lembrar");
    m.app.click("tmux-fechar-dialogo-primary");
    assert_eq!(encerrados(&m).len(), 1);
    let config = std::fs::read_to_string(pasta.caminho().join("configuracoes.json")).unwrap();
    assert!(
        config.contains("\"tmux_ao_fechar\": \"encerrar\""),
        "{config}"
    );

    // Daqui em diante, fecha sem perguntar.
    let chave = abrir(&mut m);
    fechar(&mut m, &chave);
    assert!(!aparece(&m.app, "tmux-fechar-dialogo"));
    assert_eq!(encerrados(&m).len(), 2);
}

#[test]
fn com_manter_na_configuracao_fecha_sem_perguntar_nem_encerrar() {
    let pasta = com_tmux("tmux-config-manter");
    pasta.configurar("conexoes", "tmux_ao_fechar", json!("manter"));
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    let s = sessao(&chave);
    fechar(&mut m, &chave);
    assert!(!aparece(&m.app, "tmux-fechar-dialogo"));
    assert!(encerrados(&m).is_empty());
    assert!(untracked_closed(s));
}

#[test]
fn fechar_todas_pergunta_uma_vez_so() {
    let pasta = com_tmux("tmux-fechar-todas");
    pasta.configurar("conexoes", "confirmar_fechar_todas", json!(false));
    let mut m = montar(&pasta, Theme::Light);
    abrir(&mut m);
    m.app.double_click("arvore-2");
    esperar(&mut m.app, "a segunda aba", |a| abas(a).len() == 2);
    m.app.click("menus-ferramentas").click("fechar-todas");
    assert!(aparece(&m.app, "tmux-fechar-dialogo"));
    let texto = m.app.text("tmux-fechar-texto");
    assert!(
        texto.contains("tirech-2-1") && texto.contains("tirech-2-2"),
        "{texto}"
    );
    m.app.click("tmux-fechar-dialogo-primary");
    assert!(!aparece(&m.app, "tmux-fechar-dialogo"), "perguntou de novo");
    let mut mortos = encerrados(&m);
    mortos.sort();
    assert_eq!(
        mortos,
        [
            "tmux kill-session -t =tirech-2-1",
            "tmux kill-session -t =tirech-2-2"
        ]
    );
}

#[test]
fn a_aba_no_screen_liga_na_sessao_e_encerrar_sai_dela() {
    let pasta = pasta_com(
        "screen-aba",
        SessaoPersistente::Screen,
        SessaoPersistente::Herdar,
    );
    let mut m = montar(&pasta, Theme::Light);
    let chave = abrir(&mut m);
    let comando = comando_da(&m, &chave).expect("a aba roda no screen");
    assert!(comando.contains("screen -x"), "{comando}");
    assert!(
        comando.contains("screen -h 10000 -S tirech-2-1"),
        "{comando}"
    );
    assert!(!comando.contains("tmux"), "{comando}");

    fechar(&mut m, &chave);
    assert!(aparece(&m.app, "tmux-fechar-dialogo"), "não perguntou");
    let texto = m.app.text("tmux-fechar-texto");
    assert!(texto.contains("screen"), "{texto}");
    m.app.click("tmux-fechar-dialogo-primary");
    let mortos = encerrados(&m);
    assert_eq!(mortos.len(), 1, "{mortos:?}");
    assert!(mortos[0].contains("tirech-2-1"), "{mortos:?}");
}

/// O que o servidor falso da conexão 2 responde ao script da lista.
const NO_SERVIDOR: &str = "\
tirech-tem:tmux
tirech-tem:screen
tirech-sessao:tmux:1:obras
tirech-sessao:screen:0:4242.tirech-2-7
tirech-sessao:tmux:0:x y
";

fn responder(m: &Montagem, resposta: Result<&str, &str>) {
    m.opener.respostas_exec.lock().unwrap().insert(
        2,
        resposta
            .map(|saida| ExecOutput {
                status: Some(0),
                stdout: saida.as_bytes().to_vec(),
                stderr: Vec::new(),
            })
            .map_err(str::to_owned),
    );
}

/// As entradas dos `sh -s` que rodaram na conexão 2.
fn scripts(m: &Montagem) -> Vec<String> {
    m.opener
        .execucoes
        .lock()
        .unwrap()
        .iter()
        .filter(|(id, comando, _)| *id == 2 && comando == "sh -s")
        .map(|(_, _, entrada)| String::from_utf8_lossy(entrada).into_owned())
        .collect()
}

/// Abre a lista pelo menu da conexão na árvore e espera a linha `linha`.
fn lista_pela_arvore(m: &mut Montagem, linha: &str) {
    m.app.expand("arvore", "1");
    m.app.context_menu("arvore-2");
    m.app.click("menu-2-sessoes");
    esperar(&mut m.app, "a lista", |a| aparece(a, linha));
}

#[test]
fn a_lista_mostra_as_sessoes_do_servidor_e_abrir_liga_uma_aba_nela() {
    let pasta = pasta_com(
        "sessoes-lista",
        SessaoPersistente::Herdar,
        SessaoPersistente::Herdar,
    );
    let mut m = montar(&pasta, Theme::Light);
    responder(&m, Ok(NO_SERVIDOR));
    lista_pela_arvore(&mut m, "sessoes-tmux-obras");
    assert!(
        scripts(&m)[0].contains("tirech-sessao"),
        "{:?}",
        scripts(&m)
    );
    assert!(aparece(&m.app, "sessoes-screen-4242-tirech-2-7"));
    // Um nome que o app não sabe abrir fica na lista, mas sem o "Abrir".
    assert!(!m.app.enabled("sessoes-tmux-x-y-abrir"));
    assert!(m.app.enabled("sessoes-tmux-x-y-encerrar"));

    // Mesmo com a conexão sem sessão persistente, "Abrir" liga a aba nova à sessão escolhida.
    m.app.click("sessoes-tmux-obras-abrir");
    assert!(!aparece(&m.app, "sessoes-dialogo"));
    esperar(&mut m.app, "a aba", |a| abas(a).len() == 1);
    let chave = abas(&m.app)[0].clone();
    let comando = comando_da(&m, &chave).expect("a aba roda no tmux");
    assert!(comando.contains("new-session -A -s obras"), "{comando}");

    // Pelo menu da aba, a sessão aparece aberta aqui, e "Abrir" vira "Ir para a aba".
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("aba-sessoes");
    esperar(&mut m.app, "a lista de novo", |a| {
        aparece(a, "sessoes-tmux-obras") && a.label("sessoes-tmux-obras-abrir") == "Ir para a aba"
    });
    m.app.click("sessoes-tmux-obras-abrir");
    assert!(!aparece(&m.app, "sessoes-dialogo"));
    assert_eq!(abas(&m.app).len(), 1, "abriu outra aba na mesma sessão");

    // A do screen abre pelo nome, com o screen.
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("aba-sessoes");
    esperar(&mut m.app, "a lista mais uma vez", |a| {
        aparece(a, "sessoes-screen-4242-tirech-2-7")
    });
    m.app.click("sessoes-screen-4242-tirech-2-7-abrir");
    esperar(&mut m.app, "a aba do screen", |a| abas(a).len() == 2);
    let outra = abas(&m.app).into_iter().find(|c| *c != chave).unwrap();
    let comando = comando_da(&m, &outra).expect("a aba roda no screen");
    assert!(
        comando.contains("screen -h 10000 -S tirech-2-7"),
        "{comando}"
    );
    for chave in abas(&m.app) {
        sessao(&chave).dispose();
    }
}

#[test]
fn encerrar_da_lista_roda_no_servidor_e_a_aba_fecha_sem_perguntar() {
    let pasta = pasta_com(
        "sessoes-encerrar",
        SessaoPersistente::Herdar,
        SessaoPersistente::Herdar,
    );
    let mut m = montar(&pasta, Theme::Light);
    responder(&m, Ok(NO_SERVIDOR));
    lista_pela_arvore(&mut m, "sessoes-tmux-obras");
    m.app.click("sessoes-tmux-obras-abrir");
    esperar(&mut m.app, "a aba", |a| abas(a).len() == 1);
    let chave = abas(&m.app)[0].clone();

    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("aba-sessoes");
    esperar(&mut m.app, "a lista", |a| aparece(a, "sessoes-tmux-obras"));
    m.app.click("sessoes-tmux-obras-encerrar");
    let limite = Instant::now() + Duration::from_secs(10);
    while scripts(&m).len() < 4 {
        assert!(
            Instant::now() < limite,
            "não rodou o encerrar e a lista nova"
        );
        m.app.wait(Duration::from_millis(20));
    }
    let rodados = scripts(&m);
    assert!(
        rodados[2].contains("tmux kill-session -t") && rodados[2].contains("obras"),
        "{rodados:?}"
    );
    assert!(rodados[3].contains("tirech-sessao"), "{rodados:?}");
    assert!(aparece(&m.app, "sessoes-dialogo"), "a lista fechou");

    // A aba deixou de ser da sessão: fechar não pergunta nem encerra de novo.
    m.app.click("sessoes-dialogo-close");
    fechar(&mut m, &chave);
    assert!(!aparece(&m.app, "tmux-fechar-dialogo"));
    assert!(encerrados(&m).is_empty());
}

#[test]
fn a_lista_mostra_o_erro_do_servidor() {
    let pasta = pasta_com(
        "sessoes-erro",
        SessaoPersistente::Herdar,
        SessaoPersistente::Herdar,
    );
    let mut m = montar(&pasta, Theme::Light);
    responder(&m, Err("a conexão caiu"));
    lista_pela_arvore(&mut m, "sessoes-erro");
    let texto = m.app.text("sessoes-erro");
    assert!(texto.contains("a conexão caiu"), "{texto}");
}
