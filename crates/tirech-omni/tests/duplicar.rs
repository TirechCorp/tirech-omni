//! Duplicar a aba na pasta em que ela está (M64): o menu da sessão, o menu da aba e o
//! Alt+Shift+D abrem a cópia no mesmo lugar, com a rede, os comandos ao conectar e o zoom da
//! fonte da origem. Aqui as sessões são `Loopback` (eco local), que não têm processo nem canal
//! SSH: a pasta vem do OSC 7, e o app digita o ` cd` depois dos comandos ao conectar. O shell
//! local de verdade (o `/proc` do processo) é conferido no fim, só no Linux.

mod comum;
#[cfg(unix)]
#[path = "comum/servidor.rs"]
mod servidor;

use std::time::{Duration, Instant};

use tirech_omni::modelo::{
    ComandosAoConectar, Connection, Espera, Kind, ModoComandos, Passo, Tree,
};
use tomni_ui::terminal::{Session, TerminalVisual};
use tomni_ui::theme::Theme;
use tomni_ui::{Look, Modifiers, Shortcut};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

fn comandos(textos: &[&str]) -> ComandosAoConectar {
    ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos: textos
            .iter()
            .map(|texto| {
                let mut p = Passo::comando(*texto, 5);
                p.espera = Espera::Logo;
                p
            })
            .collect(),
        reconectar: true,
    }
}

/// Uma pasta com a conexão local 1, com os `comandos` ao conectar.
fn pasta_com(nome: &str, comandos: ComandosAoConectar) -> Pasta {
    let pasta = Pasta::nova(nome);
    let mut tree = Tree::default();
    let mut c = Connection::new("Shell", Kind::Local);
    c.comandos = comandos;
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.configurar("conexoes", "comandos_quieto_ms", serde_json::json!(50));
    pasta
}

/// Todas as abas dos painéis, na ordem.
fn abas(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()))
        .collect()
}

/// A sessão da aba `chave`.
fn sessao(chave: &str) -> Session {
    Session::find(chave).expect("a sessão da aba")
}

/// Abre a conexão 1 e espera os comandos ao conectar de `marca` (se há) aparecerem. Devolve a
/// chave da aba.
fn abrir(m: &mut Montagem, marca: Option<&str>) -> String {
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a aba", |a| abas(a).len() == 1);
    let chave = abas(&m.app)[0].clone();
    if let Some(marca) = marca {
        esperar(&mut m.app, "o comando da abertura", |a| {
            a.session_text(&chave).contains(marca)
        });
    }
    chave
}

/// O programa em primeiro plano da origem avisou que está em `pasta` (o OSC 7).
fn avisar_pasta(m: &mut Montagem, chave: &str, pasta: &str) {
    let s = sessao(chave);
    s.feed(format!("\x1b]7;file://host{pasta}\x07").as_bytes());
    esperar(&mut m.app, "o OSC 7", |_| s.cwd().is_some());
}

#[derive(Clone, Copy, Debug)]
enum Via {
    MenuDaSessao,
    MenuDaAba,
    Atalho,
}

/// Duplica a aba `chave` por `via` e devolve a chave da cópia.
fn duplicar(m: &mut Montagem, chave: &str, via: Via) -> String {
    match via {
        Via::MenuDaSessao => {
            m.app.click("menus-ferramentas").click("duplicar");
        }
        Via::MenuDaAba => {
            m.app.context_menu(&format!("paineis-painel-0-{chave}"));
            m.app.click("aba-duplicar");
        }
        Via::Atalho => {
            m.app
                .shortcut(Shortcut::new('d', Modifiers::ALT | Modifiers::SHIFT));
        }
    }
    esperar(&mut m.app, "a cópia", |a| abas(a).len() == 2);
    let copia = abas(&m.app)
        .into_iter()
        .find(|c| c != chave)
        .expect("a aba nova");
    // O servidor mostra o prompt da cópia: é a deixa do motor dos comandos, que espera alguma
    // saída da conexão. O `Loopback` devolve o que recebe, e é isso que conta como saída.
    sessao(&copia).send(b"ana@srv:~$ ");
    copia
}

/// O texto da sessão `chave`, depois de esperar `trecho` aparecer nele.
fn esperar_texto(m: &mut Montagem, chave: &str, trecho: &str) -> String {
    let limite = Instant::now() + Duration::from_secs(10);
    while !m.app.session_text(chave).contains(trecho) {
        assert!(
            Instant::now() < limite,
            "não apareceu {trecho:?} em {:?}",
            m.app.session_text(chave)
        );
        m.app.wait(Duration::from_millis(20));
    }
    m.app.session_text(chave)
}

/// Dá tempo de o motor dos comandos rodar tudo o que tinha para rodar.
fn sossegar(m: &mut Montagem) {
    m.app.wait(Duration::from_millis(600));
}

fn pontos(app: &TestApp, chave: &str) -> f32 {
    match app.look(&format!("paineis-terminal-{chave}")) {
        Look::Custom(custom) => {
            custom
                .downcast::<TerminalVisual>()
                .expect("o visual do terminal")
                .metrics()
                .font_size
        }
        outro => panic!("não é o terminal: {outro:?}"),
    }
}

fn confere_a_copia_na_pasta(via: Via, nome: &str) {
    let pasta = pasta_com(nome, comandos(&["marca"]));
    let mut m = montar(&pasta, Theme::Light);
    let origem = abrir(&mut m, Some("marca"));
    avisar_pasta(&mut m, &origem, "/srv/app");

    // O zoom da origem, para a cópia herdar.
    let base = pontos(&m.app, &origem);
    m.app.click(&format!("paineis-terminal-{origem}"));
    m.app.shortcut(Shortcut::ctrl('='));
    m.app.shortcut(Shortcut::ctrl('='));
    let ampliada = pontos(&m.app, &origem);
    assert!(ampliada > base, "o zoom da origem mudou");

    let copia = duplicar(&mut m, &origem, via);
    assert!(
        copia.starts_with("1-"),
        "{via:?}: a cópia herda o id da conexão: {copia}"
    );
    let texto = esperar_texto(&mut m, &copia, "cd /srv/app");
    let marca = texto.find("marca").expect("o comando da conexão rodou");
    let cd = texto.find("cd /srv/app").unwrap();
    assert!(
        marca < cd,
        "{via:?}: o cd vem depois dos comandos: {texto:?}"
    );
    assert_eq!(pontos(&m.app, &copia), ampliada, "{via:?}: herdou o zoom");
}

#[test]
fn o_menu_da_sessao_duplica_na_pasta() {
    confere_a_copia_na_pasta(Via::MenuDaSessao, "dup-menu-sessao");
}

#[test]
fn o_menu_da_aba_duplica_na_pasta() {
    confere_a_copia_na_pasta(Via::MenuDaAba, "dup-menu-aba");
}

#[test]
fn o_atalho_alt_shift_d_duplica_na_pasta() {
    confere_a_copia_na_pasta(Via::Atalho, "dup-atalho");
}

#[test]
fn sem_comandos_ao_conectar_a_copia_so_digita_o_cd() {
    let pasta = pasta_com("dup-so-cd", comandos(&[]));
    let mut m = montar(&pasta, Theme::Light);
    let origem = abrir(&mut m, None);
    avisar_pasta(&mut m, &origem, "/var/log");
    let copia = duplicar(&mut m, &origem, Via::MenuDaAba);
    assert!(copia.starts_with("1-"), "{copia}");
    esperar_texto(&mut m, &copia, "cd /var/log");
}

#[test]
fn sem_pasta_conhecida_nao_digita_cd() {
    let pasta = pasta_com("dup-sem-pasta", comandos(&["marca"]));
    let mut m = montar(&pasta, Theme::Light);
    let origem = abrir(&mut m, Some("marca"));
    let copia = duplicar(&mut m, &origem, Via::MenuDaAba);
    esperar_texto(&mut m, &copia, "marca");
    sossegar(&mut m);
    let texto = m.app.session_text(&copia);
    assert!(!texto.contains("cd "), "{texto:?}");
}

#[test]
fn a_pasta_vai_entre_aspas_simples() {
    let pasta = pasta_com("dup-aspas", comandos(&[]));
    let mut m = montar(&pasta, Theme::Light);
    let origem = abrir(&mut m, None);
    avisar_pasta(&mut m, &origem, "/srv/o%27brien%20app");
    let copia = duplicar(&mut m, &origem, Via::MenuDaAba);
    esperar_texto(&mut m, &copia, r"cd '/srv/o'\''brien app'");
}

#[test]
fn com_a_opcao_desligada_a_copia_sai_como_antes() {
    let pasta = pasta_com("dup-desligado", comandos(&["marca"]));
    pasta.configurar("terminal", "duplicar_na_pasta", serde_json::json!(false));
    let mut m = montar(&pasta, Theme::Light);
    let origem = abrir(&mut m, Some("marca"));
    avisar_pasta(&mut m, &origem, "/srv/app");
    let copia = duplicar(&mut m, &origem, Via::MenuDaAba);
    assert!(copia.starts_with("1-"), "{copia}");
    esperar_texto(&mut m, &copia, "marca");
    sossegar(&mut m);
    let texto = m.app.session_text(&copia);
    assert!(!texto.contains("cd "), "{texto:?}");
}

#[test]
fn o_shell_local_sem_conexao_salva_tambem_recebe_o_cd() {
    let pasta = Pasta::nova("dup-sem-conexao");
    pasta.semear(&Tree::default());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-ferramentas");
    m.app.click("abrir-shell");
    esperar(&mut m.app, "a aba", |a| abas(a).len() == 1);
    let origem = abas(&m.app)[0].clone();
    avisar_pasta(&mut m, &origem, "/srv/app");
    let copia = duplicar(&mut m, &origem, Via::MenuDaAba);
    esperar_texto(&mut m, &copia, "cd /srv/app");
}

#[test]
fn o_wsl_recebe_a_pasta_no_cd_do_wsl() {
    let pasta = Pasta::nova("dup-wsl");
    let mut tree = Tree::default();
    let mut c = Connection::new("Ubuntu", Kind::Local);
    c.local_programa = "wsl.exe".into();
    c.local_argumentos = "-d Ubuntu --cd ~".into();
    tree.add_connection(None, c);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    let origem = abrir(&mut m, None);
    avisar_pasta(&mut m, &origem, "/home/ana/meu app");
    let copia = duplicar(&mut m, &origem, Via::MenuDaAba);
    let locais = m.opener.locais.borrow().clone();
    assert_eq!(locais.len(), 2, "{locais:?}");
    let args = tirech_omni::abrir::dividir_argumentos(&locais[1].local_argumentos);
    assert_eq!(args, ["-d", "Ubuntu", "--cd", "/home/ana/meu app"]);
    // O `--cd` basta: o app não digita nada na cópia.
    sossegar(&mut m);
    assert!(!m.app.session_text(&copia).contains("cd "));
}

/// O shell local de verdade, no Linux: a pasta vem do `/proc` do processo, sem o shell avisar.
#[cfg(target_os = "linux")]
#[test]
fn o_shell_local_duplica_na_pasta_do_processo() {
    use std::rc::Rc;
    use tirech_omni::abrir::RealOpener;
    use tirech_omni::tela::tela_com_ssh;

    let pasta = Pasta::nova("dup-shell-real");
    let base = std::env::temp_dir().join(format!("tirech-dup-{}", std::process::id()));
    let a = base.join("a");
    let b = base.join("b");
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    let a = a.canonicalize().unwrap();
    let b = b.canonicalize().unwrap();

    let mut tree = Tree::default();
    let mut c = Connection::new("Shell", Kind::Local);
    c.local_programa = "sh".into();
    c.local_pasta = a.display().to_string();
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.pagina_inicial_padrao("terminais");
    pasta.agente_padrao(false);
    let (caminho, ssh) = (pasta.0.clone(), pasta.ssh());
    let mut app = TestApp::new(move || {
        tela_com_ssh(caminho, ssh, |fila| Rc::new(RealOpener::new(fila, None)))
    })
    .size(1280, 720)
    .theme(Theme::Light);

    app.double_click("arvore-1");
    esperar(&mut app, "a aba", |a| abas(a).len() == 1);
    let origem = abas(&app)[0].clone();
    // A origem vai para a pasta `b` sem avisar ninguém: só o /proc sabe.
    let sessao_origem = sessao(&origem);
    sessao_origem.send(format!("cd '{}'\n", b.display()).as_bytes());
    let pid = sessao_origem.process_id().expect("o processo local");
    let esperado = Some(b.display().to_string());
    esperar(&mut app, "o shell entrar em b", |_| {
        tirech_omni::pasta_da_aba::pasta_do_processo(pid) == esperado
    });

    app.context_menu(&format!("paineis-painel-0-{origem}"));
    app.click("aba-duplicar");
    esperar(&mut app, "a cópia", |a| abas(a).len() == 2);
    let copia = abas(&app)[1].clone();
    sessao(&copia).send(b"pwd -P\n");
    let alvo = b.display().to_string();
    esperar(&mut app, "a cópia em b", |a| {
        a.session_text(&copia)
            .lines()
            .any(|l| l.trim_end() == alvo || l.trim_end().ends_with(&format!(" {alvo}")))
    });

    for chave in abas(&app) {
        sessao(&chave).dispose();
    }
    let _ = std::fs::remove_dir_all(&base);
}

/// Abre o SSH no servidor local de teste, sem pedir senha.
#[cfg(unix)]
struct AbreSsh(std::rc::Rc<servidor::Servidor>);

#[cfg(unix)]
impl tirech_omni::abrir::Opener for AbreSsh {
    fn open(&self, conexao: &Connection, _senhas: tirech_omni::abrir::Senhas, n: u32) -> Session {
        Session::new(self.0.builder().connect())
            .key(tirech_omni::abrir::chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }
}

/// O caminho do SSH de verdade: a cópia só nasce quando a sonda volta (numa thread, pelo canal
/// `exec` da mesma conexão), nasce numa conexão nova e recebe o ` cd` depois do prompt. O servidor
/// de teste roda a sonda num `sh` local, sem `sshd` entre os antepassados: ela não acha pasta, e a
/// reserva é o OSC 7.
#[cfg(unix)]
#[test]
fn o_ssh_sonda_o_servidor_e_cai_no_osc_7() {
    use std::rc::Rc;
    let pasta = Pasta::nova("dup-ssh");
    let servidor = Rc::new(servidor::Servidor::novo(&pasta.caminho().join("servidor")));
    let mut tree = Tree::default();
    let mut c = Connection::new("local", Kind::Ssh);
    c.host = "127.0.0.1".into();
    c.port = servidor.porta;
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, c);
    pasta.semear(&tree);
    pasta.configurar("ia", "verificar_ao_abrir", serde_json::json!(false));
    let (mut app, _fila) = comum::montar_com(&pasta, Theme::Light, AbreSsh(Rc::clone(&servidor)));

    app.double_click("arvore-1");
    esperar(&mut app, "a aba", |a| abas(a).len() == 1);
    let origem = abas(&app)[0].clone();
    esperar(&mut app, "o prompt da origem", |a| {
        a.session_text(&origem).contains("ana@servidor:~$")
    });
    let s = sessao(&origem);
    s.feed(b"\x1b]7;file://servidor/srv/app\x07");
    esperar(&mut app, "o OSC 7", |_| s.cwd().is_some());

    app.context_menu(&format!("paineis-painel-0-{origem}"));
    app.click("aba-duplicar");
    esperar(&mut app, "a cópia", |a| abas(a).len() == 2);
    let copia = abas(&app)[1].clone();
    assert!(copia.starts_with("1-"), "{copia}");
    esperar(&mut app, "o cd na cópia", |a| {
        a.session_text(&copia).contains("cd /")
    });
    let comandos = servidor.comandos();
    assert!(
        comandos.iter().any(|c| c.contains("/proc/$1/stat")),
        "a sonda não rodou: {comandos:?}"
    );
    for chave in abas(&app) {
        sessao(&chave).dispose();
    }
}
