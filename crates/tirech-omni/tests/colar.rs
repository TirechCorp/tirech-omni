//! Colar imagens e arquivos no terminal remoto (M45): o envio pelo SFTP, o plano B pelo `exec`, a
//! pasta pelo `tar`, os modos, a limpeza e as configurações. O servidor é um `MemFs` e o disco
//! também; o relógio do app é fixo.

mod comum;

use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use jiff::civil::DateTime;
use tirech_omni::ferramentas::Ferramentas;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::files::{Fs, MemFs};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui::{ClipboardImage, Key, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_com_ambiente};

/// O carimbo do relógio fixo, o que os nomes remotos levam na frente.
const CARIMBO: &str = "20261019-143012";

/// A pasta padrão do que é colado.
const PASTA: &str = "/tmp/tirech-colar";

const COLAR: Modifiers = Modifiers::CTRL.with(Modifiers::SHIFT);

/// 19/10/2026, 14:30:12.
fn agora_fixo() -> DateTime {
    jiff::civil::date(2026, 10, 19).at(14, 30, 12, 0)
}

/// Quatro conexões na raiz: 1 `web` e 2 `db` por SSH (com chave), 3 `Roteador` por Telnet e 4
/// `Shell` local.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    for (nome, host) in [("web", "web.exemplo"), ("db", "db.exemplo")] {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.host = host.into();
        c.user = "ana".into();
        c.key_file = Some("/chaves/ana".into());
        tree.add_connection(None, c);
    }
    let mut roteador = Connection::new("Roteador", Kind::Telnet);
    roteador.host = "192.168.0.1".into();
    tree.add_connection(None, roteador);
    tree.add_connection(None, Connection::new("Shell", Kind::Local));
    tree
}

/// A tela com o relógio fixo e a [`arvore`]. `config` põe `categoria.campo = valor` nas configurações antes de abrir.
fn tela(
    nome: &str,
    config: &[(&str, &str, serde_json::Value)],
) -> (Pasta, Montagem, Rc<Ferramentas>) {
    tela_com(nome, arvore(), config)
}

fn tela_com(
    nome: &str,
    tree: Tree,
    config: &[(&str, &str, serde_json::Value)],
) -> (Pasta, Montagem, Rc<Ferramentas>) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&tree);
    for (categoria, campo, valor) in config {
        pasta.configurar(categoria, campo, valor.clone());
    }
    let (m, ferramentas) = montar_com_ambiente(&pasta, Theme::Light, |a| a.agora = agora_fixo);
    (pasta, m, ferramentas)
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

/// Cola o que está na área de transferência no terminal da aba (Ctrl+Shift+V).
fn colar(m: &mut Montagem, chave: &str) {
    m.app.click(&format!("paineis-terminal-{chave}"));
    m.app.key_with(Key::Char('v'), COLAR);
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// O que a aba mostra.
fn tela_da(app: &TestApp, chave: &str) -> String {
    app.session_text(chave)
}

/// O que o app escreveu na conexão da aba desde a última leitura.
fn escrito(m: &Montagem, chave: &str) -> String {
    let sessao = Session::find(chave).expect("a sessão da aba");
    let handle = m.opener.handle_de(sessao).expect("a ponta da sessão");
    String::from_utf8_lossy(&handle.take_written()).into_owned()
}

/// Espera o que o app escreve na conexão da aba (somando as leituras) satisfazer `pronto`.
fn esperar_escrito(
    m: &mut Montagem,
    chave: &str,
    o_que: &str,
    pronto: impl Fn(&str) -> bool,
) -> String {
    let limite = Instant::now() + Duration::from_secs(10);
    let mut recebido = String::new();
    while !pronto(&recebido) {
        assert!(
            Instant::now() < limite,
            "não aconteceu: {o_que} (recebi {recebido:?})"
        );
        m.app.wait(Duration::from_millis(20));
        recebido.push_str(&escrito(m, chave));
    }
    recebido
}

fn arquivo_local(m: &Montagem, caminho: &str, dados: &[u8]) -> PathBuf {
    m.opener.disco.put(caminho, dados);
    PathBuf::from(caminho)
}

fn vermelha() -> ClipboardImage {
    ClipboardImage {
        width: 2,
        height: 2,
        rgba: [255, 0, 0, 255].repeat(4),
    }
}

#[test]
fn arquivo_colado_chega_ao_servidor_e_o_caminho_vai_ao_terminal() {
    let (_pasta, mut m, _f) = tela("colar-arquivo", &[]);
    let origem = arquivo_local(&m, "/home/ana/relatório final.pdf", b"0123456789");
    m.opener.servidor(1).dir("/home/ana");
    let chave = abrir(&mut m, 1);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &chave);

    let destino = format!("{PASTA}/{CARIMBO}-relatório final.pdf");
    esperar(&mut m.app, "o caminho no terminal", |a| {
        tela_da(a, &chave).contains("-relatório final.pdf' ")
            || tela_da(a, &chave).contains("-relatório final.pdf'")
    });
    let servidor = m.opener.servidor(1);
    assert_eq!(servidor.contents(&destino), Some(b"0123456789".to_vec()));
    let pasta_remota = servidor.connect().stat(PASTA).unwrap();
    assert_eq!(pasta_remota.permissions.map(|p| p & 0o777), Some(0o700));
    let tela = tela_da(&m.app, &chave);
    assert!(tela.contains(&format!("'{PASTA}/")), "{tela}");
    m.app.assert_lint();
}

/// Decodifica um PNG: largura, altura e os pixels RGBA.
fn decodificar(dados: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut leitor = png::Decoder::new(std::io::Cursor::new(dados))
        .read_info()
        .expect("ler o PNG colado");
    let mut pixels = vec![0; leitor.output_buffer_size().expect("tamanho do PNG")];
    let info = leitor.next_frame(&mut pixels).expect("decodificar o PNG");
    pixels.truncate(info.buffer_size());
    (info.width, info.height, pixels)
}

#[test]
fn imagem_colada_com_ctrl_v_chega_como_png_e_o_caminho_vai_sem_aspas() {
    let (pasta, mut m, _f) = tela("colar-imagem", &[]);
    let chave = abrir(&mut m, 1);
    m.app.clipboard().write_image(vermelha());
    m.app.click(&format!("paineis-terminal-{chave}"));
    m.app.key_with(Key::Char('v'), Modifiers::CTRL);

    let destino = format!("{PASTA}/colado-{CARIMBO}.png");
    let servidor = m.opener.servidor(1);
    esperar(&mut m.app, "o PNG no servidor", |_| {
        servidor.exists(&destino)
    });
    esperar(&mut m.app, "o caminho no terminal", |a| {
        tela_da(a, &chave).contains(&destino)
    });
    let (largura, altura, pixels) = decodificar(&servidor.contents(&destino).unwrap());
    assert_eq!((largura, altura), (2, 2));
    assert_eq!(pixels, [255, 0, 0, 255].repeat(4));
    assert!(
        !tela_da(&m.app, &chave).contains(&format!("'{destino}")),
        "o caminho de um PNG não leva aspas"
    );
    // A cópia local que serviu de origem sumiu.
    let copia = pasta
        .caminho()
        .join("colar")
        .join(format!("colado-{CARIMBO}.png"));
    let copia = copia.display().to_string();
    esperar(&mut m.app, "a cópia local sumir", |_| {
        !m.opener.disco.exists(&copia)
    });
    m.app.assert_lint();
}

#[test]
fn arrastar_arquivos_e_pasta_envia_tudo_e_cola_os_caminhos_em_ordem() {
    let (_pasta, mut m, _f) = tela("colar-arrastar", &[]);
    let um = arquivo_local(&m, "/home/ana/um.txt", b"um");
    let dois = arquivo_local(&m, "/home/ana/dois.txt", b"dois");
    arquivo_local(&m, "/home/ana/projeto/a.txt", b"aa");
    arquivo_local(&m, "/home/ana/projeto/b.txt", b"bbb");
    let chave = abrir(&mut m, 1);
    escrito(&m, &chave);
    m.app.drop_files(
        &format!("paineis-terminal-{chave}"),
        &[um, dois, "/home/ana/projeto".into()],
    );

    let esperado =
        format!("{PASTA}/{CARIMBO}-um.txt {PASTA}/{CARIMBO}-dois.txt {PASTA}/{CARIMBO}-projeto ");
    esperar_escrito(&mut m, &chave, "os três caminhos", |r| r == esperado);
    let servidor = m.opener.servidor(1);
    assert_eq!(
        servidor.contents(&format!("{PASTA}/{CARIMBO}-um.txt")),
        Some(b"um".to_vec())
    );
    assert_eq!(
        servidor.contents(&format!("{PASTA}/{CARIMBO}-dois.txt")),
        Some(b"dois".to_vec())
    );
    assert_eq!(
        servidor.contents(&format!("{PASTA}/{CARIMBO}-projeto/a.txt")),
        Some(b"aa".to_vec())
    );
    assert_eq!(
        servidor.contents(&format!("{PASTA}/{CARIMBO}-projeto/b.txt")),
        Some(b"bbb".to_vec())
    );
    m.app.assert_lint();
}

#[test]
fn com_bracketed_paste_cada_caminho_vai_entre_as_marcas() {
    let (_pasta, mut m, _f) = tela("colar-bracketed", &[]);
    let um = arquivo_local(&m, "/home/ana/um.txt", b"um");
    let dois = arquivo_local(&m, "/home/ana/dois.txt", b"dois");
    let chave = abrir(&mut m, 1);
    Session::find(&chave).unwrap().feed(b"\x1b[?2004h");
    escrito(&m, &chave);
    m.app.clipboard().write_files(vec![um, dois]);
    colar(&mut m, &chave);

    let esperado = format!(
        "\x1b[200~{PASTA}/{CARIMBO}-um.txt\x1b[201~ \x1b[200~{PASTA}/{CARIMBO}-dois.txt\x1b[201~ "
    );
    esperar_escrito(&mut m, &chave, "os dois caminhos", |r| r == esperado);
    m.app.assert_lint();
}

#[test]
fn nome_ocupado_no_servidor_vira_o_numero_dois_e_e_esse_que_e_colado() {
    let (_pasta, mut m, _f) = tela("colar-colisao", &[]);
    let origem = arquivo_local(&m, "/home/ana/laudo.pdf", b"novo");
    m.opener
        .servidor(1)
        .put(&format!("{PASTA}/{CARIMBO}-laudo.pdf"), "velho");
    let chave = abrir(&mut m, 1);
    escrito(&m, &chave);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &chave);

    let recebido = esperar_escrito(&mut m, &chave, "o caminho", |r| r.ends_with(' '));
    assert_eq!(recebido, format!("{PASTA}/{CARIMBO}-laudo-2.pdf "));
    let servidor = m.opener.servidor(1);
    assert_eq!(
        servidor.contents(&format!("{PASTA}/{CARIMBO}-laudo.pdf")),
        Some(b"velho".to_vec())
    );
    assert_eq!(
        servidor.contents(&format!("{PASTA}/{CARIMBO}-laudo-2.pdf")),
        Some(b"novo".to_vec())
    );
    m.app.assert_lint();
}

#[test]
fn a_pasta_da_conexao_vale_mais_que_a_das_configuracoes() {
    let mut tree = arvore();
    tree.connection_mut(1).unwrap().pasta_colar = "~/colados".into();
    let (_pasta, mut m, _f) = tela_com("colar-pasta-conexao", tree, &[]);
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let chave = abrir(&mut m, 1);
    escrito(&m, &chave);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &chave);

    let destino = format!("/home/ana/colados/{CARIMBO}-nota.txt");
    let servidor = m.opener.servidor(1);
    esperar(&mut m.app, "o arquivo", |_| servidor.exists(&destino));
    assert!(!servidor.exists(PASTA));
    m.app.assert_lint();
}

#[test]
fn pasta_sem_permissao_cai_para_o_cache_e_avisa() {
    let (_pasta, mut m, _f) = tela("colar-bloqueada", &[]);
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    m.opener.servidor(1).deny(PASTA);
    let chave = abrir(&mut m, 1);
    escrito(&m, &chave);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &chave);

    let destino = format!("/home/ana/.cache/tirech-colar/{CARIMBO}-nota.txt");
    let servidor = m.opener.servidor(1);
    esperar(&mut m.app, "o arquivo no cache", |_| {
        servidor.exists(&destino)
    });
    esperar(&mut m.app, "o aviso", |a| aparece(a, "colar-aviso"));
    let aviso = m.app.text("colar-aviso");
    assert!(aviso.contains(PASTA), "{aviso}");
    assert!(aviso.contains("/home/ana/.cache/tirech-colar"), "{aviso}");
    m.app.assert_lint();
}

#[test]
fn sem_sftp_o_plano_b_manda_pelo_exec() {
    let (_pasta, mut m, _f) = tela("colar-plano-b", &[]);
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"conteudo");
    m.opener.sem_sftp.set(true);
    let chave = abrir(&mut m, 1);
    escrito(&m, &chave);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &chave);

    let destino = format!("{PASTA}/{CARIMBO}-nota.txt");
    let recebido = esperar_escrito(&mut m, &chave, "o caminho", |r| r.ends_with(' '));
    assert_eq!(recebido, format!("{destino} "));
    assert_eq!(
        m.opener.servidor(1).contents(&destino),
        Some(b"conteudo".to_vec())
    );
    let execs = m.opener.execs.lock().unwrap().clone();
    assert!(
        execs.iter().any(|(id, comando, _)| *id == 1
            && comando.contains("mkdir -p --")
            && comando.contains(PASTA)),
        "{execs:?}"
    );
    assert!(
        execs
            .iter()
            .any(|(_, comando, entrada)| comando.contains("cat >") && entrada == b"conteudo"),
        "{execs:?}"
    );
    assert!(!aparece(&m.app, "colar-erro"));
    m.app.assert_lint();
}

#[test]
fn a_limpeza_apaga_so_o_que_passou_dos_dias_e_so_no_primeiro_envio_da_sessao() {
    let (_pasta, mut m, _f) = tela("colar-limpeza", &[]);
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let servidor = m.opener.servidor(1);
    servidor.put(&format!("{PASTA}/20261001-100000-velho.txt"), "v");
    servidor.put(&format!("{PASTA}/colado-20261018-090000.png"), "p");
    servidor.put(&format!("{PASTA}/notas.txt"), "n");
    let chave = abrir(&mut m, 1);
    escrito(&m, &chave);
    m.app.clipboard().write_files(vec![origem.clone()]);
    colar(&mut m, &chave);

    esperar(&mut m.app, "o arquivo novo", |_| {
        servidor.exists(&format!("{PASTA}/{CARIMBO}-nota.txt"))
    });
    esperar(&mut m.app, "a limpeza", |_| {
        !servidor.exists(&format!("{PASTA}/20261001-100000-velho.txt"))
    });
    assert!(servidor.exists(&format!("{PASTA}/colado-20261018-090000.png")));
    assert!(servidor.exists(&format!("{PASTA}/notas.txt")));

    // Um segundo envio na mesma sessão não limpa de novo: o velho posto agora fica.
    servidor.put(&format!("{PASTA}/20260901-100000-outro.txt"), "o");
    colar(&mut m, &chave);
    esperar(&mut m.app, "o segundo arquivo", |_| {
        servidor.exists(&format!("{PASTA}/{CARIMBO}-nota-2.txt"))
    });
    assert!(servidor.exists(&format!("{PASTA}/20260901-100000-outro.txt")));
    m.app.assert_lint();
}

#[test]
fn com_zero_dias_nada_e_apagado() {
    let (_pasta, mut m, _f) = tela(
        "colar-limpeza-zero",
        &[("terminal", "limpar_colados_dias", serde_json::json!(0))],
    );
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let servidor = m.opener.servidor(1);
    servidor.put(&format!("{PASTA}/20200101-100000-velho.txt"), "v");
    let chave = abrir(&mut m, 1);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &chave);
    esperar(&mut m.app, "o arquivo novo", |_| {
        servidor.exists(&format!("{PASTA}/{CARIMBO}-nota.txt"))
    });
    m.app.wait(Duration::from_millis(200));
    assert!(servidor.exists(&format!("{PASTA}/20200101-100000-velho.txt")));
    m.app.assert_lint();
}

fn configuracoes_gravadas(pasta: &Pasta) -> tirech_omni::configuracoes::Configuracoes {
    tirech_omni::configuracoes::Configuracoes::carregar(&pasta.armazenamento()).0
}

/// Cola `origem` numa sessão SSH nova e devolve a chave dela, com a escrita zerada.
fn colar_arquivo_novo(m: &mut Montagem, origem: PathBuf) -> String {
    let chave = abrir(m, 1);
    escrito(m, &chave);
    m.app.clipboard().write_files(vec![origem]);
    colar(m, &chave);
    chave
}

#[test]
fn o_modo_caminho_local_nao_manda_nada_ao_servidor() {
    let (_pasta, mut m, _f) = tela(
        "colar-modo-local",
        &[(
            "terminal",
            "colar_arquivos",
            serde_json::json!("caminho_local"),
        )],
    );
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let chave = colar_arquivo_novo(&mut m, origem);
    let recebido = esperar_escrito(&mut m, &chave, "o caminho local", |r| r.ends_with(' '));
    assert_eq!(recebido, "/home/ana/nota.txt ");
    m.app.wait(Duration::from_millis(200));
    assert!(!m.opener.servidor(1).exists(PASTA));
    m.app.assert_lint();
}

#[test]
fn a_conexao_em_caminho_local_ignora_o_enviar_das_configuracoes() {
    let mut tree = arvore();
    tree.connection_mut(1).unwrap().colar_arquivos =
        Some(tirech_omni::configuracoes::ColarArquivos::CaminhoLocal);
    let (_pasta, mut m, _f) = tela_com("colar-modo-conexao", tree, &[]);
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let chave = colar_arquivo_novo(&mut m, origem);
    let recebido = esperar_escrito(&mut m, &chave, "o caminho local", |r| r.ends_with(' '));
    assert_eq!(recebido, "/home/ana/nota.txt ");
    assert!(!m.opener.servidor(1).exists(PASTA));
    m.app.assert_lint();
}

fn perguntando(nome: &str) -> (Pasta, Montagem, String, PathBuf) {
    let (pasta, mut m, _f) = tela(
        nome,
        &[("terminal", "colar_arquivos", serde_json::json!("perguntar"))],
    );
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let chave = colar_arquivo_novo(&mut m, origem.clone());
    esperar(&mut m.app, "o diálogo", |a| {
        aparece(a, "colar-arquivos-dialogo")
    });
    (pasta, m, chave, origem)
}

#[test]
fn perguntar_e_enviar_manda_ao_servidor_e_cola_o_caminho_de_la() {
    let (pasta, mut m, chave, _) = perguntando("colar-perguntar-enviar");
    assert!(m.app.text("colar-arquivos-texto").contains("web"));
    assert!(m.app.text("colar-arquivos-itens").contains("nota.txt"));
    m.app.assert_lint();
    m.app.click("colar-arquivos-dialogo-primary");
    let recebido = esperar_escrito(&mut m, &chave, "o caminho remoto", |r| r.ends_with(' '));
    assert_eq!(recebido, format!("{PASTA}/{CARIMBO}-nota.txt "));
    assert_eq!(
        configuracoes_gravadas(&pasta).terminal.colar_arquivos,
        tirech_omni::configuracoes::ColarArquivos::Perguntar,
        "sem marcar, a configuração não muda"
    );
}

#[test]
fn perguntar_e_colar_o_caminho_local_nao_envia() {
    let (_pasta, mut m, chave, _) = perguntando("colar-perguntar-local");
    m.app.click("colar-arquivos-dialogo-secondary");
    let recebido = esperar_escrito(&mut m, &chave, "o caminho local", |r| r.ends_with(' '));
    assert_eq!(recebido, "/home/ana/nota.txt ");
    assert!(!m.opener.servidor(1).exists(PASTA));
}

#[test]
fn perguntar_e_cancelar_nao_faz_nada() {
    let (_pasta, mut m, chave, _) = perguntando("colar-perguntar-cancelar");
    m.app.click("colar-arquivos-dialogo-close");
    m.app.wait(Duration::from_millis(300));
    assert!(!aparece(&m.app, "colar-arquivos-dialogo"));
    assert_eq!(escrito(&m, &chave), "");
    assert!(!m.opener.servidor(1).exists(PASTA));
}

#[test]
fn nao_perguntar_de_novo_com_enviar_grava_enviar() {
    let (pasta, mut m, chave, _) = perguntando("colar-perguntar-lembrar");
    m.app.click("colar-arquivos-lembrar");
    m.app.click("colar-arquivos-dialogo-primary");
    esperar_escrito(&mut m, &chave, "o caminho remoto", |r| r.ends_with(' '));
    assert_eq!(
        configuracoes_gravadas(&pasta).terminal.colar_arquivos,
        tirech_omni::configuracoes::ColarArquivos::Enviar
    );
}

#[test]
fn no_shell_local_a_imagem_cola_o_caminho_da_copia_na_pasta_de_dados() {
    let (pasta, mut m, _f) = tela("colar-shell-local", &[]);
    let chave = abrir(&mut m, 4);
    escrito(&m, &chave);
    m.app.clipboard().write_image(vermelha());
    m.app.click(&format!("paineis-terminal-{chave}"));
    m.app.key_with(Key::Char('v'), Modifiers::CTRL);

    let local = pasta
        .caminho()
        .join("colar")
        .join(format!("colado-{CARIMBO}.png"));
    let recebido = esperar_escrito(&mut m, &chave, "o caminho local", |r| r.ends_with(' '));
    assert_eq!(recebido, format!("{} ", local.display()));
    assert!(
        m.opener.disco.exists(&local.display().to_string()),
        "a cópia local tem de ficar: o shell vai ler"
    );
    assert!(!aparece(&m.app, "colar-aviso"), "o shell local não avisa");
    m.app.assert_lint();
}

#[test]
fn no_telnet_cola_o_caminho_local_e_avisa_uma_vez_so() {
    let (_pasta, mut m, _f) = tela("colar-telnet", &[]);
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let chave = abrir(&mut m, 3);
    escrito(&m, &chave);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &chave);
    let recebido = esperar_escrito(&mut m, &chave, "o caminho local", |r| r.ends_with(' '));
    assert_eq!(recebido, "/home/ana/nota.txt ");
    esperar(&mut m.app, "o aviso", |a| aparece(a, "colar-aviso"));
    assert!(m.app.text("colar-aviso").contains("Roteador"));

    colar(&mut m, &chave);
    esperar_escrito(&mut m, &chave, "o segundo caminho", |r| r.ends_with(' '));
    m.app.wait(Duration::from_millis(200));
    assert!(
        !aparece(&m.app, "colar-aviso-2"),
        "o aviso vem uma vez por sessão"
    );
    m.app.assert_lint();
}

#[test]
fn com_envio_para_varios_cada_sessao_recebe_a_copia_e_o_caminho() {
    let (_pasta, mut m, _f) = tela("colar-varios", &[]);
    m.app.click("menus-exibir").click("layout-2x2");
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    let a = abrir(&mut m, 1);
    let b = abrir(&mut m, 2);
    m.app.click("envio-todos");
    escrito(&m, &a);
    escrito(&m, &b);
    m.app.clipboard().write_files(vec![origem]);
    colar(&mut m, &a);

    let destino = format!("{PASTA}/{CARIMBO}-nota.txt");
    for (chave, servidor) in [(&a, 1), (&b, 2)] {
        let recebido = esperar_escrito(&mut m, chave, "o caminho", |r| r.ends_with(' '));
        assert_eq!(recebido, format!("{destino} "));
        assert_eq!(
            m.opener.servidor(servidor).contents(&destino),
            Some(b"oi".to_vec())
        );
    }
    m.app.assert_lint();
}

#[test]
fn falha_no_envio_mostra_o_erro_e_nao_cola_nada() {
    let (_pasta, mut m, _f) = tela("colar-falha", &[]);
    let origem = arquivo_local(&m, "/home/ana/nota.txt", b"oi");
    m.opener.servidor(1).dir(PASTA);
    m.opener
        .servidor(1)
        .deny(&format!("{PASTA}/{CARIMBO}-nota.txt"));
    let chave = colar_arquivo_novo(&mut m, origem);
    esperar(&mut m.app, "o erro", |a| aparece(a, "colar-erro"));
    let erro = m.app.text("colar-erro");
    assert!(erro.contains("nota.txt"), "o motivo vem na InfoBar: {erro}");
    m.app.wait(Duration::from_millis(200));
    assert_eq!(escrito(&m, &chave), "");
    m.app.assert_lint();
}

/// Um arquivo grande o bastante para o envio ficar à vista: o servidor falso demora em cada
/// bloco.
fn envio_lento(m: &mut Montagem) -> PathBuf {
    let origem = arquivo_local(m, "/home/ana/grande.bin", &vec![7u8; 4000]);
    let servidor = m.opener.servidor(1);
    servidor.set_chunk(10);
    servidor.set_delay(Duration::from_millis(25));
    origem
}

#[test]
fn cancelar_o_envio_nao_cola_nada() {
    let (_pasta, mut m, _f) = tela("colar-cancelar", &[]);
    let origem = envio_lento(&mut m);
    let chave = colar_arquivo_novo(&mut m, origem);
    esperar(&mut m.app, "o andamento", |a| aparece(a, "colar-cancelar"));
    m.app.assert_lint();
    m.app.click("colar-cancelar");
    esperar(&mut m.app, "o andamento sumir", |a| {
        !aparece(a, "colar-progresso")
    });
    m.app.wait(Duration::from_millis(300));
    assert_eq!(escrito(&m, &chave), "");
    assert!(!aparece(&m.app, "colar-erro"), "cancelar não é erro");
}

#[test]
fn fechar_a_sessao_no_meio_do_envio_nao_cola_nem_quebra() {
    let (_pasta, mut m, _f) = tela("colar-fechar", &[]);
    let origem = envio_lento(&mut m);
    let chave = colar_arquivo_novo(&mut m, origem);
    esperar(&mut m.app, "o andamento", |a| aparece(a, "colar-cancelar"));
    m.app.key_with(Key::Char('w'), COLAR);
    esperar(&mut m.app, "a aba fechar", |a| !abas(a).contains(&chave));
    m.app.wait(Duration::from_millis(500));
    m.app.assert_lint();
}

#[test]
fn desligado_o_ctrl_v_com_imagem_vai_como_o_byte_16() {
    let (_pasta, mut m, _f) = tela(
        "colar-ctrl-v-desligado",
        &[("terminal", "ctrl_v_arquivos", serde_json::json!(false))],
    );
    let chave = abrir(&mut m, 1);
    escrito(&m, &chave);
    m.app.clipboard().write_image(vermelha());
    m.app.click(&format!("paineis-terminal-{chave}"));
    m.app.key_with(Key::Char('v'), Modifiers::CTRL);
    let recebido = esperar_escrito(&mut m, &chave, "o ^V", |r| !r.is_empty());
    assert_eq!(recebido, "\x16");
    m.app.wait(Duration::from_millis(200));
    assert!(!m.opener.servidor(1).exists(PASTA));
}

#[test]
fn as_propriedades_de_colar_valem_so_para_ssh_e_gravam_na_arvore() {
    let (pasta, mut m, _f) = tela("colar-propriedades", &[]);
    m.app.click("arvore-1");
    let props = m.app.properties("props");
    for nome in [
        "Ao colar",
        "Pasta no servidor",
        "Shell do servidor",
        "Comprimir a conexão",
    ] {
        let linha = props
            .iter()
            .find(|p| p.name == nome)
            .unwrap_or_else(|| panic!("sem {nome}"));
        assert!(
            !linha.hidden && linha.enabled,
            "{nome} deveria estar habilitada no SSH"
        );
    }
    assert_eq!(comum::valor(&m.app, "Ao colar"), "Padrão");

    comum::editar_propriedade(&mut m.app, "Pasta no servidor", "~/colados");
    assert_eq!(
        pasta.conexoes().connection(1).unwrap().pasta_colar,
        "~/colados"
    );

    m.app.click("arvore-3");
    let props = m.app.properties("props");
    for nome in [
        "Ao colar",
        "Pasta no servidor",
        "Shell do servidor",
        "Comprimir a conexão",
    ] {
        let linha = props
            .iter()
            .find(|p| p.name == nome)
            .unwrap_or_else(|| panic!("sem {nome}"));
        assert!(linha.hidden, "{nome} deveria estar escondida no Telnet");
    }
    m.app.assert_lint();
}

type Exec = (u64, String, Vec<u8>);

fn execs(m: &Montagem) -> Vec<Exec> {
    m.opener.execs.lock().unwrap().clone()
}

/// Os `exec` que extraem um pacote `tar` (a sondagem só lista).
fn extracoes(m: &Montagem) -> Vec<Exec> {
    execs(m)
        .into_iter()
        .filter(|(_, comando, _)| comando.contains("tar -x"))
        .collect()
}

/// Quantas sondagens do `tar` a conexão `id` fez.
fn sondagens(m: &Montagem, id: u64) -> usize {
    execs(m)
        .iter()
        .filter(|(i, comando, _)| *i == id && comando.starts_with("tar -t"))
        .count()
}

/// Uma pasta no disco com `n` arquivos `arq-N.<extensao>` de `bytes` bytes cada; devolve o
/// caminho relativo e o conteúdo de cada um.
fn pasta_de(
    m: &Montagem,
    raiz: &str,
    n: usize,
    extensao: &str,
    bytes: usize,
) -> Vec<(String, Vec<u8>)> {
    m.opener.disco.dir(raiz);
    (0..n)
        .map(|i| {
            let nome = format!("arq-{i}.{extensao}");
            let dados = vec![b'a' + (i % 26) as u8; bytes];
            m.opener.disco.put(&format!("{raiz}/{nome}"), &dados);
            (nome, dados)
        })
        .collect()
}

/// A pasta do teste 16: 58 arquivos, `relatório final.txt`, `a b/c.txt`, um link para arquivo
/// (`atalho.txt`) e um link para pasta (`atalho-pasta`).
fn pasta_de_projeto(m: &Montagem) -> Vec<(String, Vec<u8>)> {
    let raiz = "/home/ana/projeto";
    let mut arquivos = pasta_de(m, raiz, 58, "txt", 40);
    for (nome, dados) in [
        ("relatório final.txt", b"relatorio".to_vec()),
        ("a b/c.txt", b"c".to_vec()),
    ] {
        m.opener.disco.put(&format!("{raiz}/{nome}"), &dados);
        arquivos.push((nome.to_owned(), dados));
    }
    m.opener.disco.put("/home/ana/fora/x.txt", "x");
    m.opener
        .disco
        .link(&format!("{raiz}/atalho.txt"), "relatório final.txt");
    m.opener
        .disco
        .link(&format!("{raiz}/atalho-pasta"), "/home/ana/fora");
    arquivos.push(("atalho.txt".to_owned(), b"relatorio".to_vec()));
    arquivos
}

/// Cola `origem` na sessão `chave` e espera o caminho chegar ao terminal.
fn colar_e_esperar(m: &mut Montagem, chave: &str, origem: &str) -> String {
    escrito(m, chave);
    m.app.clipboard().write_files(vec![origem.into()]);
    colar(m, chave);
    esperar_escrito(m, chave, "o caminho colado", |r| r.ends_with(' '))
}

fn confere_arquivos(servidor: &MemFs, destino: &str, esperados: &[(String, Vec<u8>)]) {
    for (nome, dados) in esperados {
        assert_eq!(
            servidor.contents(&format!("{destino}/{nome}")).as_deref(),
            Some(dados.as_slice()),
            "{nome} não chegou inteiro"
        );
    }
}

#[test]
fn pasta_grande_vai_num_pacote_tar_so() {
    let (_pasta, mut m, _f) = tela("colar-tar", &[]);
    let esperados = pasta_de_projeto(&m);
    let chave = abrir(&mut m, 1);
    let recebido = colar_e_esperar(&mut m, &chave, "/home/ana/projeto");

    let destino = format!("{PASTA}/{CARIMBO}-projeto");
    assert_eq!(recebido, format!("{destino} "));
    let servidor = m.opener.servidor(1);
    confere_arquivos(&servidor, &destino, &esperados);
    assert!(!servidor.exists(&format!("{destino}/atalho-pasta")));
    assert!(!servidor.exists(&format!("{destino}/atalho-pasta/x.txt")));
    assert_eq!(sondagens(&m, 1), 1, "{:?}", execs(&m));
    assert!(execs(&m).iter().any(|(_, c, _)| c == "tar -tzf -"));
    let extracoes = extracoes(&m);
    assert_eq!(extracoes.len(), 1, "{extracoes:?}");
    assert!(
        extracoes[0].1.starts_with("tar -x -z -o -f - -C ") && extracoes[0].1.contains(&destino),
        "{}",
        extracoes[0].1
    );
    assert_eq!(extracoes[0].2[..2], [0x1f, 0x8b], "o fluxo vai com gzip");
    assert!(!aparece(&m.app, "colar-erro") && !aparece(&m.app, "colar-aviso"));
    m.app.assert_lint();
}

#[test]
fn a_sondagem_do_tar_e_feita_uma_vez_por_sessao() {
    let (_pasta, mut m, _f) = tela("colar-sondagem", &[]);
    pasta_de(&m, "/home/ana/p1", 60, "txt", 10);
    let a = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &a, "/home/ana/p1");
    colar_e_esperar(&mut m, &a, "/home/ana/p1");
    assert_eq!(sondagens(&m, 1), 1, "o segundo envio não sonda de novo");
    assert_eq!(extracoes(&m).len(), 2);

    let b = abrir(&mut m, 2);
    colar_e_esperar(&mut m, &b, "/home/ana/p1");
    assert_eq!(sondagens(&m, 2), 1, "outra sessão faz a sondagem dela");
    m.app.assert_lint();
}

#[test]
fn sem_tar_no_servidor_a_pasta_vai_pela_fila_sem_aviso() {
    let (_pasta, mut m, _f) = tela("colar-sem-tar", &[]);
    let esperados = pasta_de_projeto(&m);
    m.opener
        .sem_tar
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let chave = abrir(&mut m, 1);
    let recebido = colar_e_esperar(&mut m, &chave, "/home/ana/projeto");

    let destino = format!("{PASTA}/{CARIMBO}-projeto");
    assert_eq!(recebido, format!("{destino} "));
    confere_arquivos(&m.opener.servidor(1), &destino, &esperados);
    assert!(extracoes(&m).is_empty());
    assert!(!aparece(&m.app, "colar-erro") && !aparece(&m.app, "colar-aviso"));
    m.app.assert_lint();
}

#[test]
fn sem_gzip_no_servidor_o_fluxo_vai_puro() {
    let (_pasta, mut m, _f) = tela("colar-sem-gzip", &[]);
    let esperados = pasta_de_projeto(&m);
    m.opener
        .tar_sem_gzip
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let chave = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &chave, "/home/ana/projeto");

    let destino = format!("{PASTA}/{CARIMBO}-projeto");
    confere_arquivos(&m.opener.servidor(1), &destino, &esperados);
    let extracoes = extracoes(&m);
    assert_eq!(extracoes.len(), 1);
    assert!(!extracoes[0].1.contains(" -z"), "{}", extracoes[0].1);
    assert_ne!(extracoes[0].2[..2], [0x1f, 0x8b]);
    m.app.assert_lint();
}

#[test]
fn pasta_de_png_vai_sem_gzip() {
    let (_pasta, mut m, _f) = tela("colar-png", &[]);
    pasta_de(&m, "/home/ana/fotos", 60, "png", 100);
    let chave = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &chave, "/home/ana/fotos");
    let extracoes = extracoes(&m);
    assert_eq!(extracoes.len(), 1);
    assert!(!extracoes[0].1.contains(" -z"), "{}", extracoes[0].1);
    m.app.assert_lint();
}

#[test]
fn conexao_comprimida_manda_o_tar_sem_gzip() {
    let mut tree = arvore();
    tree.connection_mut(1).unwrap().comprimir = true;
    let (_pasta, mut m, _f) = tela_com("colar-comprimida", tree, &[]);
    pasta_de(&m, "/home/ana/texto", 60, "txt", 100);
    let chave = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &chave, "/home/ana/texto");
    let extracoes = extracoes(&m);
    assert_eq!(extracoes.len(), 1);
    assert!(!extracoes[0].1.contains(" -z"), "{}", extracoes[0].1);
    m.app.assert_lint();
}

#[test]
fn cancelar_o_tar_apaga_o_destino_e_nao_cola_nada() {
    let (_pasta, mut m, _f) = tela("colar-tar-cancelar", &[]);
    pasta_de(&m, "/home/ana/p1", 60, "txt", 10);
    pasta_de(&m, "/home/ana/p2", 60, "txt", 10);
    let chave = abrir(&mut m, 1);
    // O primeiro envio faz a sondagem, que o `segurar_exec` também seguraria.
    colar_e_esperar(&mut m, &chave, "/home/ana/p1");

    *m.opener.segurar_exec.0.lock().unwrap() = true;
    m.app.clipboard().write_files(vec!["/home/ana/p2".into()]);
    colar(&mut m, &chave);
    esperar(&mut m.app, "o andamento", |a| aparece(a, "colar-cancelar"));
    m.app.assert_lint();
    m.app.click("colar-cancelar");
    *m.opener.segurar_exec.0.lock().unwrap() = false;
    m.opener.segurar_exec.1.notify_all();

    let destino = format!("{PASTA}/{CARIMBO}-p2");
    esperar(&mut m.app, "o andamento sumir", |a| {
        !aparece(a, "colar-progresso")
    });
    m.app.wait(Duration::from_millis(300));
    assert!(
        !m.opener.servidor(1).exists(&destino),
        "o destino parcial ficou"
    );
    assert_eq!(escrito(&m, &chave), "");
    assert!(!aparece(&m.app, "colar-erro"), "cancelar não é erro");
}

#[test]
fn limite_zero_manda_a_pasta_sempre_pela_fila() {
    let (_pasta, mut m, _f) = tela(
        "colar-limite-zero",
        &[("terminal", "colar_tar_min_arquivos", serde_json::json!(0))],
    );
    let esperados = pasta_de_projeto(&m);
    let chave = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &chave, "/home/ana/projeto");
    confere_arquivos(
        &m.opener.servidor(1),
        &format!("{PASTA}/{CARIMBO}-projeto"),
        &esperados,
    );
    assert!(
        execs(&m).is_empty(),
        "com 0 nem a sondagem acontece: {:?}",
        execs(&m)
    );
}

#[test]
fn pasta_pequena_com_mais_arquivos_que_o_minimo_vai_pelo_tar() {
    // Com 10 arquivos ou mais e média pequena o pacote vale a pena, mesmo abaixo do mínimo (50).
    let (_pasta, mut m, _f) = tela("colar-limite-pequenos", &[]);
    pasta_de(&m, "/home/ana/pequenos", 12, "txt", 1024);
    let chave = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &chave, "/home/ana/pequenos");
    assert_eq!(extracoes(&m).len(), 1);
}

#[test]
fn pasta_com_menos_arquivos_que_o_minimo_vai_pela_fila() {
    let (_pasta, mut m, _f) = tela("colar-limite-poucos", &[]);
    let esperados = pasta_de(&m, "/home/ana/poucos", 5, "txt", 1024);
    let chave = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &chave, "/home/ana/poucos");
    confere_arquivos(
        &m.opener.servidor(1),
        &format!("{PASTA}/{CARIMBO}-poucos"),
        &esperados,
    );
    assert!(extracoes(&m).is_empty());
}

#[test]
fn sem_sftp_a_pasta_vai_num_exec_so_que_cria_e_extrai() {
    let (_pasta, mut m, _f) = tela("colar-plano-b-tar", &[]);
    let esperados = pasta_de_projeto(&m);
    m.opener.sem_sftp.set(true);
    let chave = abrir(&mut m, 1);
    colar_e_esperar(&mut m, &chave, "/home/ana/projeto");

    let destino = format!("{PASTA}/{CARIMBO}-projeto");
    confere_arquivos(&m.opener.servidor(1), &destino, &esperados);
    let extracoes = extracoes(&m);
    assert_eq!(extracoes.len(), 1, "{extracoes:?}");
    assert!(
        extracoes[0].1.starts_with("umask 077 && mkdir -p -- ")
            && extracoes[0].1.contains("&& tar -x -z -o -f - -C "),
        "{}",
        extracoes[0].1
    );
    m.app.assert_lint();
}

#[test]
fn comprimir_a_conexao_e_uma_propriedade_so_do_ssh_e_pede_compressao() {
    let (pasta, mut m, _f) = tela("colar-comprimir", &[]);
    m.app.click("arvore-1");
    let props = m.app.properties("props");
    let linha = props
        .iter()
        .find(|p| p.name == "Comprimir a conexão")
        .expect("a linha Comprimir a conexão");
    assert!(linha.enabled);
    assert_eq!(linha.category.as_deref(), Some("Sessão"));
    assert_eq!(linha.value, "false");

    comum::escolher_propriedade(&mut m.app, "Comprimir a conexão");
    m.app.key(Key::Space);
    assert_eq!(comum::valor(&m.app, "Comprimir a conexão"), "true");
    let conexao = pasta.conexoes().connection(1).cloned().unwrap();
    assert!(conexao.comprimir);
    assert!(
        pasta.texto_conexoes().contains("\"comprimir\": true")
            || pasta.texto_conexoes().contains("\"comprimir\":true")
    );
    let builder = tirech_omni::abrir::ssh_builder_com(
        &conexao,
        Default::default(),
        m.fila,
        None,
        None,
        Default::default(),
    );
    assert!(
        format!("{builder:?}").contains("compression: true"),
        "{builder:?}"
    );

    m.app.click("arvore-3");
    let props = m.app.properties("props");
    assert!(
        props
            .iter()
            .find(|p| p.name == "Comprimir a conexão")
            .unwrap()
            .hidden
    );
    m.app.assert_lint();
}
