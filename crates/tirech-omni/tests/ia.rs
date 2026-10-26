//! Assistentes de IA no servidor remoto (M46): a sondagem por `exec`, os rótulos dos menus, abrir
//! em aba nova, na aba ou em outra pasta, instalar com o diálogo e o aviso do fim. Nenhum teste
//! roda `claude`, `codex` nem `opencode` de verdade: o servidor é o `FakeOpener`, que responde à
//! sondagem pelo que o teste põe em `respostas_exec`.

mod comum;

use std::net::TcpListener;
use std::time::{Duration, Instant};

use tirech_omni::abrir::FakeOpener;
use tirech_omni::ia::COMANDO_SONDAGEM_WINDOWS;
use tirech_omni::modelo::{Connection, Kind, ShellRemoto, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::ExecOutput;
use tomni_ui::theme::Theme;
use tomni_ui::{Key, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_com_ambiente};

/// Ctrl+Alt+A.
const ATALHO: Modifiers = Modifiers::CTRL.with(Modifiers::ALT);

/// Três conexões SSH na raiz: 1 `web` (sem pasta), 2 `app` (pasta remota `/var/www`) e 3 `lab`
/// (pasta remota `/var/www` e pasta lembrada `/opt/lab`); 4 é o `Shell` local e 5 é `win`, um
/// servidor Windows com o shell remoto em PowerShell.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    for (nome, host) in [
        ("web", "web.exemplo"),
        ("app", "app.exemplo"),
        ("lab", "lab.exemplo"),
    ] {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.host = host.into();
        c.user = "ana".into();
        c.key_file = Some("/chaves/ana".into());
        match nome {
            "app" => c.pasta_remota = "/var/www".into(),
            "lab" => {
                c.pasta_remota = "/var/www".into();
                c.ia.pasta = "/opt/lab".into();
            }
            _ => {}
        }
        tree.add_connection(None, c);
    }
    tree.add_connection(None, Connection::new("Shell", Kind::Local));
    let mut win = Connection::new("win", Kind::Ssh);
    win.host = "win.exemplo".into();
    win.user = "ana".into();
    win.key_file = Some("/chaves/ana".into());
    win.shell_remoto = ShellRemoto::PowerShell;
    tree.add_connection(None, win);
    tree
}

fn tela(nome: &str, config: &[(&str, &str, serde_json::Value)]) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore());
    for (categoria, campo, valor) in config {
        pasta.configurar(categoria, campo, valor.clone());
    }
    let (m, _) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    (pasta, m)
}

/// O que o script de sondagem imprime: um Linux com curl, tar e bash, mais as `linhas`.
fn sondagem(linhas: &[&str]) -> String {
    let mut texto = String::from(
        "tirech-ia-inicio\nuname=Linux x86_64\nhome=/home/ana\nshell=/bin/bash\n\
         path=/usr/bin:/bin\ntem_curl=1\ntem_tar=1\ntem_bash=1\n",
    );
    for linha in linhas {
        texto.push_str(linha);
        texto.push('\n');
    }
    texto.push_str("tirech-ia-fim\n");
    texto
}

/// O Claude Code instalado (2.1.0) e o Codex, não.
fn so_com_claude() -> String {
    sondagem(&[
        "claude=/home/ana/.local/bin/claude",
        "claude_versao=2.1.0 (Claude Code)",
        "claude_logado=1",
    ])
}

/// Faz o servidor falso do `id` responder `saida` à sondagem.
fn responder(m: &Montagem, id: u64, saida: &str) {
    m.opener.respostas_exec.lock().unwrap().insert(
        id,
        Ok(ExecOutput {
            status: Some(0),
            stdout: saida.as_bytes().to_vec(),
            stderr: Vec::new(),
        }),
    );
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
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

/// Abre a conexão `id` pela árvore e devolve a chave da aba nova.
fn abrir(m: &mut Montagem, id: u64) -> String {
    let antes = abas(&m.app);
    m.app.double_click(&format!("arvore-{id}"));
    esperar(&mut m.app, "a aba nova", |a| abas(a).len() > antes.len());
    abas(&m.app)
        .into_iter()
        .find(|c| !antes.contains(c))
        .expect("a aba nova")
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// Quantas vezes a sondagem (`sh -s`) rodou na conexão `id`.
fn sondagens(m: &Montagem, id: u64) -> usize {
    sondagens_de(&m.opener, id)
}

fn sondagens_de(opener: &FakeOpener, id: u64) -> usize {
    execucoes_de(opener, id, "sh -s")
}

/// Quantas vezes o `comando` rodou na conexão `id`.
fn execucoes_de(opener: &FakeOpener, id: u64, comando: &str) -> usize {
    opener
        .execucoes
        .lock()
        .unwrap()
        .iter()
        .filter(|(conexao, rodado, _)| *conexao == id && rodado == comando)
        .count()
}

/// Faz o servidor falso do `id` responder `saida` à sondagem por PowerShell.
fn responder_windows(m: &Montagem, id: u64, saida: &str) {
    m.opener.respostas_exec_windows.lock().unwrap().insert(
        id,
        Ok(ExecOutput {
            status: Some(0),
            stdout: saida.as_bytes().to_vec(),
            stderr: Vec::new(),
        }),
    );
}

/// A sondagem de um Windows com o Claude Code instalado.
fn windows_com_claude() -> String {
    "tirech-ia-inicio\r\nwindows=1\r\nuname=Windows AMD64\r\nhome=C:\\Users\\ana\r\n\
     shell=powershell\r\ntem_npm=C:\\nodejs\\npm.cmd\r\nnode=v22.11.0\r\n\
     claude=C:\\Users\\ana\\.local\\bin\\claude.exe\r\nclaude_versao=2.1.0 (Claude Code)\r\n\
     tirech-ia-fim\r\n"
        .to_owned()
}

/// Espera a primeira sondagem da conexão `id` terminar (o rótulo deixa de ser "verificando").
fn esperar_sondagem(m: &mut Montagem, id: u64, quantas: usize) {
    let opener = m.opener.clone();
    esperar(&mut m.app, "a sondagem", |_| {
        sondagens_de(&opener, id) >= quantas
    });
    // Dá tempo da thread entregar e da tela guardar o resultado.
    m.app.wait(Duration::from_millis(100));
}

/// Abre o submenu "Assistentes de IA" do menu Ferramentas.
fn abrir_menu(m: &mut Montagem) {
    m.app.click("menus-ferramentas").click("ia");
}

/// Espera o app ter feito `quantos` pedidos de comando.
fn esperar_pedidos(m: &mut Montagem, o_que: &str, quantos: usize) {
    let opener = m.opener.clone();
    esperar(&mut m.app, o_que, |_| {
        opener.comandos.borrow().len() == quantos
    });
}

/// Quantos pedidos de comando (abas de assistente) o app fez.
fn pedidos(m: &Montagem) -> usize {
    m.opener.comandos.borrow().len()
}

/// O último pedido de comando: `(id da conexão, título, comando, túneis)`.
fn ultimo(m: &Montagem) -> (u64, String, String, usize) {
    let comandos = m.opener.comandos.borrow();
    let (id, pedido) = comandos.last().expect("nenhum pedido de comando");
    (
        id.to_owned(),
        pedido.titulo.clone(),
        pedido.comando.clone(),
        pedido.tuneis.len(),
    )
}

#[test]
fn sem_aba_ssh_o_atalho_avisa_e_o_botao_some() {
    let (_pasta, mut m) = tela("ia-sem-aba", &[]);
    assert!(!aparece(&m.app, "ia-botao"));
    m.app.key_with(Key::Char('a'), ATALHO);
    assert!(
        m.app.text("aviso-info").contains("Abra uma aba SSH"),
        "o aviso: {}",
        m.app.text("aviso-info")
    );
    assert_eq!(pedidos(&m), 0);
}

#[test]
fn a_sondagem_roda_uma_vez_e_os_rotulos_seguem_o_que_o_servidor_tem() {
    let (_pasta, mut m) = tela("ia-rotulos", &[]);
    responder(&m, 1, &so_com_claude());
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    assert!(aparece(&m.app, "ia-botao"), "o botão aparece numa aba SSH");

    abrir_menu(&mut m);
    assert_eq!(m.app.label("ia-claude"), "Abrir Claude Code");
    assert_eq!(m.app.label("ia-codex"), "Instalar Codex…");
    assert_eq!(m.app.label("ia-opencode"), "Instalar OpenCode…");
    // O que não está instalado não tem como entrar.
    m.app.click("ia-entrar");
    assert!(aparece(&m.app, "ia-entrar-claude"));
    assert!(!aparece(&m.app, "ia-entrar-codex"));

    // A sondagem leva o script pela entrada e fica em cache: reabrir o menu não repete.
    let entrada = {
        let execucoes = m.opener.execucoes.lock().unwrap();
        let (_, _, entrada) = execucoes
            .iter()
            .find(|(_, comando, _)| comando == "sh -s")
            .expect("a sondagem");
        String::from_utf8_lossy(entrada).into_owned()
    };
    assert!(
        entrada.contains("tirech-ia-fim"),
        "o script segue pela entrada"
    );
    m.app.key(Key::Escape).key(Key::Escape).key(Key::Escape);
    abrir_menu(&mut m);
    assert_eq!(m.app.label("ia-claude"), "Abrir Claude Code");
    assert_eq!(sondagens(&m, 1), 1, "a segunda abertura usa o cache");
}

#[test]
fn servidor_que_nao_responde_direito_vira_nao_deu_para_verificar_e_verificar_tenta_de_novo() {
    let (_pasta, mut m) = tela("ia-falhou", &[]);
    m.opener
        .respostas_exec
        .lock()
        .unwrap()
        .insert(1, Err("a conexão caiu".to_owned()));
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    abrir_menu(&mut m);
    assert_eq!(
        m.app.label("ia-claude"),
        "Claude Code (não deu para verificar)"
    );

    responder(&m, 1, &so_com_claude());
    m.app.click("ia-verificar");
    esperar_sondagem(&mut m, 1, 2);
    abrir_menu(&mut m);
    assert_eq!(m.app.label("ia-claude"), "Abrir Claude Code");
}

#[test]
fn abrir_o_claude_faz_uma_aba_nova_com_o_comando_na_pasta_do_terminal() {
    let (_pasta, mut m) = tela("ia-abrir", &[]);
    responder(&m, 1, &so_com_claude());
    let chave = abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    // O shell avisa a pasta pelo OSC 7.
    Session::find(&chave)
        .unwrap()
        .feed(b"\x1b]7;file://web.exemplo/srv/app\x07");

    let antes = abas(&m.app).len();
    abrir_menu(&mut m);
    m.app.click("ia-claude");
    esperar_pedidos(&mut m, "o pedido", 1);
    let (id, titulo, comando, tuneis) = ultimo(&m);
    assert_eq!(id, 1);
    assert_eq!(titulo, "Claude Code");
    assert!(comando.contains("claude"), "o comando: {comando}");
    assert!(comando.contains("/srv/app"), "a pasta do OSC 7: {comando}");
    assert_eq!(tuneis, 0);
    assert_eq!(abas(&m.app).len(), antes + 1, "uma aba nova");
}

#[test]
fn a_pasta_vem_da_lembrada_da_conexao_e_depois_da_remota_padrao() {
    let (_pasta, mut m) = tela("ia-pastas", &[]);
    for id in [1, 2, 3] {
        responder(&m, id, &so_com_claude());
    }
    // A conexão 1 não tem pasta nenhuma: o comando não faz `cd` para lugar nenhum.
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    m.app.key_with(Key::Char('a'), ATALHO);
    esperar_pedidos(&mut m, "o pedido 1", 1);
    let (_, _, comando, _) = ultimo(&m);
    assert!(
        !comando.contains("/var/www") && !comando.contains("/opt/lab"),
        "{comando}"
    );

    abrir(&mut m, 2);
    esperar_sondagem(&mut m, 2, 1);
    m.app.key_with(Key::Char('a'), ATALHO);
    esperar_pedidos(&mut m, "o pedido 2", 2);
    let (id, _, comando, _) = ultimo(&m);
    assert_eq!(id, 2);
    assert!(
        comando.contains("/var/www"),
        "a pasta remota padrão: {comando}"
    );

    abrir(&mut m, 3);
    esperar_sondagem(&mut m, 3, 1);
    m.app.key_with(Key::Char('a'), ATALHO);
    esperar_pedidos(&mut m, "o pedido 3", 3);
    let (id, _, comando, _) = ultimo(&m);
    assert_eq!(id, 3);
    assert!(
        comando.contains("/opt/lab"),
        "a lembrada vence a remota: {comando}"
    );
    assert!(!comando.contains("/var/www"), "{comando}");
}

#[test]
fn o_atalho_abre_o_assistente_preferido() {
    let (_pasta, mut m) = tela("ia-preferido", &[("ia", "preferido", "codex".into())]);
    responder(
        &m,
        1,
        &sondagem(&["codex=/home/ana/.local/bin/codex", "codex_logado=1"]),
    );
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    m.app.key_with(Key::Char('a'), ATALHO);
    esperar_pedidos(&mut m, "o pedido", 1);
    let (_, titulo, comando, tuneis) = ultimo(&m);
    assert_eq!(titulo, "Codex");
    assert!(comando.contains("codex"), "{comando}");
    assert_eq!(tuneis, 0, "logado: sem o túnel do login");
}

#[test]
fn o_codex_sem_login_leva_o_tunel_1455_e_avisa_quando_a_porta_esta_em_uso() {
    // Se algum outro processo já usa a 1455, não dá para testar nenhum dos dois caminhos.
    let Ok(ocupada) = TcpListener::bind(("127.0.0.1", 1455)) else {
        return;
    };
    let (_pasta, mut m) = tela("ia-tunel", &[("ia", "preferido", "codex".into())]);
    responder(&m, 1, &sondagem(&["codex=/home/ana/.local/bin/codex"]));
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);

    m.app.key_with(Key::Char('a'), ATALHO);
    esperar_pedidos(&mut m, "o pedido sem túnel", 1);
    assert_eq!(ultimo(&m).3, 0, "porta em uso: sem túnel");
    let aviso = m.app.text("aviso-atencao");
    assert!(aviso.contains("1455"), "o aviso: {aviso}");
    assert!(
        aviso.contains("Entrar"),
        "aponta o caminho com código: {aviso}"
    );

    drop(ocupada);
    m.app.key_with(Key::Char('a'), ATALHO);
    esperar_pedidos(&mut m, "o pedido com túnel", 2);
    assert_eq!(ultimo(&m).3, 1, "porta livre: o túnel 1455");
}

#[test]
fn nesta_aba_o_comando_e_digitado_no_terminal_sem_aba_nova() {
    let (_pasta, mut m) = tela("ia-nesta-aba", &[("ia", "abrir_em", "nesta_aba".into())]);
    responder(&m, 1, &so_com_claude());
    let chave = abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    let sessao = Session::find(&chave).unwrap();
    let handle = m.opener.handle_de(sessao).expect("a ponta da sessão");
    let _ = handle.take_written();

    let antes = abas(&m.app).len();
    m.app.key_with(Key::Char('a'), ATALHO);
    let limite = Instant::now() + Duration::from_secs(10);
    let mut escrito = String::new();
    while !escrito.contains("claude") {
        assert!(Instant::now() < limite, "não digitou (recebi {escrito:?})");
        m.app.wait(Duration::from_millis(20));
        escrito.push_str(&String::from_utf8_lossy(&handle.take_written()));
    }
    assert!(
        escrito.ends_with('\r'),
        "o comando termina em Enter: {escrito:?}"
    );
    assert_eq!(pedidos(&m), 0, "nenhum pedido de aba");
    assert_eq!(abas(&m.app).len(), antes);
}

#[test]
fn ferramenta_desligada_e_o_botao_desligado_somem() {
    let (_pasta, mut m) = tela(
        "ia-desligados",
        &[
            ("ia", "mostrar_codex", false.into()),
            ("ia", "mostrar_botao", false.into()),
        ],
    );
    responder(&m, 1, &so_com_claude());
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    assert!(!aparece(&m.app, "ia-botao"));
    abrir_menu(&mut m);
    assert!(aparece(&m.app, "ia-claude"));
    assert!(!aparece(&m.app, "ia-codex"));
    assert!(aparece(&m.app, "ia-opencode"));
}

#[test]
fn instalar_mostra_o_comando_roda_numa_aba_e_abre_sozinho_no_fim() {
    let (_pasta, mut m) = tela("ia-instalar", &[]);
    // Servidor sem nenhum assistente.
    responder(&m, 1, &sondagem(&[]));
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    abrir_menu(&mut m);
    assert_eq!(m.app.label("ia-claude"), "Instalar Claude Code…");
    m.app.click("ia-claude");
    assert!(m.app.is_open("ia-instalar"));
    let comando = m.app.text("ia-instalar-comando");
    assert!(
        comando.contains("curl -fsSL https://claude.ai/install.sh"),
        "o instalador oficial é o padrão: {comando}"
    );
    assert_eq!(pedidos(&m), 0, "só roda depois de confirmar");

    m.app.click("ia-instalar-primary");
    esperar_pedidos(&mut m, "a aba de instalação", 1);
    let (id, titulo, comando, _) = ultimo(&m);
    assert_eq!(id, 1);
    assert_eq!(titulo, "Instalar Claude Code");
    assert!(comando.contains("claude.ai/install.sh"), "{comando}");
    assert!(!m.app.is_open("ia-instalar"));

    // O servidor passa a ter o Claude; a aba de instalação termina.
    responder(&m, 1, &so_com_claude());
    let sessao = m.opener.sessoes.borrow().last().map(|(s, _)| *s).unwrap();
    m.opener.handle_de(sessao).unwrap().close(None);
    // Com "Abrir quando terminar" (ligado por padrão), o assistente abre sozinho.
    esperar_pedidos(&mut m, "o assistente abrir", 2);
    let (_, titulo, comando, _) = ultimo(&m);
    assert_eq!(titulo, "Claude Code");
    assert!(comando.contains("claude"), "{comando}");
}

#[test]
fn sem_abrir_quando_terminar_o_aviso_oferece_abrir_agora() {
    let (_pasta, mut m) = tela("ia-aviso", &[]);
    responder(&m, 1, &sondagem(&[]));
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    abrir_menu(&mut m);
    m.app.click("ia-claude");
    m.app.click("ia-instalar-abrir-depois");
    m.app.click("ia-instalar-primary");
    esperar_pedidos(&mut m, "a aba de instalação", 1);

    responder(&m, 1, &so_com_claude());
    let sessao = m.opener.sessoes.borrow().last().map(|(s, _)| *s).unwrap();
    m.opener.handle_de(sessao).unwrap().close(None);
    esperar(&mut m.app, "o aviso", |a| aparece(a, "ia-aviso"));
    let aviso = m.app.text("ia-aviso");
    assert!(
        aviso.contains("Claude Code instalado em web"),
        "o aviso: {aviso}"
    );
    assert!(aviso.contains("2.1.0"), "com a versão: {aviso}");
    assert_eq!(pedidos(&m), 1, "não abriu sozinho");
}

#[test]
fn instalacao_que_nao_deu_certo_mostra_o_erro_com_o_nome_da_aba() {
    let (_pasta, mut m) = tela("ia-erro", &[]);
    responder(&m, 1, &sondagem(&[]));
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    abrir_menu(&mut m);
    m.app.click("ia-claude");
    m.app.click("ia-instalar-primary");
    esperar_pedidos(&mut m, "a aba de instalação", 1);

    // O servidor segue sem o Claude quando a aba termina.
    let sessao = m.opener.sessoes.borrow().last().map(|(s, _)| *s).unwrap();
    m.opener.handle_de(sessao).unwrap().close(None);
    esperar(&mut m.app, "o aviso de erro", |a| {
        aparece(a, "ia-aviso-erro")
    });
    let erro = m.app.text("ia-aviso-erro");
    assert!(erro.contains("não deu certo"), "o erro: {erro}");
    assert!(
        erro.contains("Instalar Claude Code"),
        "com o nome da aba: {erro}"
    );
}

#[test]
fn outra_pasta_pergunta_e_lembra_para_a_conexao() {
    let (_pasta, mut m) = tela("ia-outra-pasta", &[]);
    responder(&m, 1, &so_com_claude());
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    abrir_menu(&mut m);
    m.app.click("ia-outra-pasta");
    assert!(m.app.is_open("ia-pasta"));
    m.app.type_into("ia-pasta-caminho", "/srv/app");
    m.app.click("ia-pasta-lembrar");
    m.app.click("ia-pasta-primary");
    esperar_pedidos(&mut m, "o pedido", 1);
    assert!(ultimo(&m).2.contains("/srv/app"), "{}", ultimo(&m).2);

    // Lembrada: o atalho seguinte já abre lá.
    m.app.key_with(Key::Char('a'), ATALHO);
    esperar_pedidos(&mut m, "o segundo pedido", 2);
    assert!(ultimo(&m).2.contains("/srv/app"), "{}", ultimo(&m).2);
}

#[test]
fn entrar_no_codex_abre_a_aba_de_login_com_codigo() {
    let (_pasta, mut m) = tela("ia-entrar", &[]);
    responder(&m, 1, &sondagem(&["codex=/home/ana/.local/bin/codex"]));
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    abrir_menu(&mut m);
    m.app.click("ia-entrar").click("ia-entrar-codex");
    esperar_pedidos(&mut m, "o pedido", 1);
    let (_, titulo, comando, tuneis) = ultimo(&m);
    assert_eq!(titulo, "Entrar no Codex");
    assert!(comando.contains("login --device-auth"), "{comando}");
    assert_eq!(tuneis, 0);
}

#[test]
fn servidor_windows_cai_no_powershell_quando_o_sh_nao_existe() {
    let (_pasta, mut m) = tela("ia-windows-queda", &[]);
    // O `cmd.exe` do servidor não conhece o `sh`.
    m.opener.respostas_exec.lock().unwrap().insert(
        1,
        Ok(ExecOutput {
            status: Some(1),
            stdout: Vec::new(),
            stderr: b"'sh' is not recognized as an internal or external command".to_vec(),
        }),
    );
    responder_windows(&m, 1, &windows_com_claude());
    abrir(&mut m, 1);
    esperar_sondagem(&mut m, 1, 1);
    esperar(&mut m.app, "a sondagem do PowerShell", |_| {
        execucoes_de(&m.opener, 1, COMANDO_SONDAGEM_WINDOWS) == 1
    });
    m.app.wait(Duration::from_millis(100));

    abrir_menu(&mut m);
    assert_eq!(m.app.label("ia-claude"), "Abrir Claude Code");
    m.app.click("ia-claude");
    esperar_pedidos(&mut m, "o pedido", 1);
    let (id, _, comando, _) = ultimo(&m);
    assert_eq!(id, 1);
    assert!(comando.starts_with("powershell -NoLogo"), "{comando}");
    assert!(comando.contains("claude.exe"), "{comando}");
    let execucoes = m.opener.execucoes.lock().unwrap();
    let (_, _, entrada) = execucoes
        .iter()
        .find(|(_, comando, _)| comando == COMANDO_SONDAGEM_WINDOWS)
        .expect("a sondagem do PowerShell");
    assert!(String::from_utf8_lossy(entrada).contains("windows=1"));
}

#[test]
fn shell_remoto_powershell_vai_direto_sem_tentar_o_sh() {
    let (_pasta, mut m) = tela("ia-windows-direto", &[]);
    responder_windows(&m, 5, &windows_com_claude());
    abrir(&mut m, 5);
    esperar(&mut m.app, "a sondagem do PowerShell", |_| {
        execucoes_de(&m.opener, 5, COMANDO_SONDAGEM_WINDOWS) == 1
    });
    m.app.wait(Duration::from_millis(100));
    assert_eq!(sondagens(&m, 5), 0, "o sh -s não roda");

    abrir_menu(&mut m);
    assert_eq!(m.app.label("ia-claude"), "Abrir Claude Code");
    assert_eq!(m.app.label("ia-codex"), "Instalar Codex…");
}
