//! A página "Chaves SSH": gerar uma chave (no cofre, que é o padrão, ou em arquivo), a
//! passphrase no cofre, o erro de passphrases diferentes e a lista de `~/.ssh`.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::cofre::Tipo;
use tomni_ui::Key;
use tomni_ui::kurbo::Point;
use tomni_ui::terminal::ssh::keys::{KeyFile, KeyGen, KeyType};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, montar, so_com_senha};

const GRADE: &str = "gerar-chave-props";

/// Espera a thread da geração terminar (o anel de andamento some).
fn esperar_geracao(app: &mut TestApp) {
    let limite = Instant::now() + Duration::from_secs(30);
    loop {
        app.wait(Duration::from_millis(20));
        if !app.visible("gerar-chave-andamento") {
            // Um quadro a mais: o resultado nasce numa view que o `Show` reconstrói.
            app.wait(Duration::from_millis(50));
            return;
        }
        assert!(Instant::now() < limite, "a chave não terminou de gerar");
    }
}

/// Escolhe a linha da grade do formulário: foca a grade clicando na primeira linha e desce
/// com ↓ até a pedida.
fn escolher(app: &mut TestApp, nome: &str) {
    if app.focused().as_deref() == Some(GRADE) {
        app.key(Key::Home);
    } else {
        let r = app.rect(&format!("{GRADE}-Tipo"));
        app.click_at(Point::new(r.x0 + 24.0, (r.y0 + r.y1) / 2.0), 1);
    }
    for _ in 0..20 {
        let escolhida = app
            .properties(GRADE)
            .into_iter()
            .find(|p| p.selected)
            .map(|p| p.name);
        if escolhida.as_deref() == Some(nome) {
            return;
        }
        app.key(Key::Down);
    }
    panic!("a propriedade {nome} não foi escolhida");
}

/// Troca o valor de uma propriedade de texto: Enter abre o editor, Enter confirma.
fn editar(app: &mut TestApp, nome: &str, texto: &str) {
    escolher(app, nome);
    app.key(Key::Enter);
    app.type_text(texto).key(Key::Enter);
}

/// O destino "Arquivo em ~/.ssh" da lista da grade (o padrão da página é o cofre).
const DESTINO_ARQUIVO: &str = "Arquivo em ~/.ssh (OpenSSH)";

/// O valor da linha `nome` da grade do formulário.
fn valor_da_grade(app: &TestApp, nome: &str) -> String {
    app.properties(GRADE)
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a propriedade {nome}"))
        .value
}

/// Se a linha `nome` da grade do formulário está habilitada.
fn habilitada(app: &TestApp, nome: &str) -> bool {
    app.properties(GRADE)
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a propriedade {nome}"))
        .enabled
}

/// Troca o destino do formulário para o arquivo em `~/.ssh`.
fn destino_arquivo(app: &mut TestApp) {
    escolher(app, "Destino");
    app.select(&format!("{GRADE}-Destino-valor"), DESTINO_ARQUIVO);
    assert_eq!(valor_da_grade(app, "Destino"), DESTINO_ARQUIVO);
}

/// Desmarca "Carregar no agente" (o agente dos testes vem desligado).
fn sem_agente(app: &mut TestApp) {
    escolher(app, "Carregar no agente");
    app.key(Key::Space);
}

#[test]
fn gera_ed25519_sem_passphrase() {
    let pasta = Pasta::nova("chaves-gera");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    assert!(m.app.visible("chaves-pagina"));
    destino_arquivo(&mut m.app);
    sem_agente(&mut m.app);
    m.app.click("gerar-chave-gerar");
    esperar_geracao(&mut m.app);

    assert!(pasta.ssh().join("id_ed25519").is_file());
    assert!(pasta.ssh().join("id_ed25519.pub").is_file());
    assert!(m.app.text("gerar-chave-resultado").contains("id_ed25519"));
    assert!(m.app.text("gerar-chave-fingerprint").contains("SHA256:"));
    assert!(
        m.app
            .text("gerar-chave-publica")
            .starts_with("ssh-ed25519 ")
    );
    assert!(!m.app.visible("gerar-chave-erro"));
    m.app.assert_lint();
}

#[test]
fn passphrases_diferentes_dao_erro_e_nao_gravam() {
    let pasta = Pasta::nova("chaves-diferentes");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    destino_arquivo(&mut m.app);
    editar(&mut m.app, "Passphrase", "um");
    editar(&mut m.app, "Repetir", "dois");
    m.app.click("gerar-chave-gerar");
    assert!(m.app.visible("gerar-chave-erro"));
    assert!(m.app.text("gerar-chave-erro").contains("não conferem"));
    assert!(!pasta.ssh().join("id_ed25519").exists());
}

#[test]
fn passphrase_vai_para_o_cofre_e_nao_para_os_json() {
    let pasta = Pasta::nova("chaves-cofre");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    destino_arquivo(&mut m.app);
    sem_agente(&mut m.app);
    editar(&mut m.app, "Passphrase", "frase-secreta-7");
    editar(&mut m.app, "Repetir", "frase-secreta-7");
    m.app.click("gerar-chave-gerar");
    esperar_geracao(&mut m.app);
    if m.app.visible("gerar-chave-erro") {
        panic!("erro: {}", m.app.text("gerar-chave-erro"));
    }

    assert!(m.app.text("gerar-chave-resultado").contains("id_ed25519"));
    let cofre = pasta.cofre();
    let itens: Vec<_> = cofre
        .itens()
        .iter()
        .filter(|i| i.tipo == Tipo::Chave)
        .collect();
    assert_eq!(itens.len(), 1, "um item Chave no cofre");
    assert_eq!(itens[0].senha, "frase-secreta-7");
    assert!(!pasta.texto_conexoes().contains("frase-secreta-7"));
    assert!(!pasta.texto_cofre().contains("frase-secreta-7"));
    let privada = std::fs::read_to_string(pasta.ssh().join("id_ed25519")).unwrap();
    assert!(!privada.contains("frase-secreta-7"));
}

/// Grava `ssh/id_ed25519` com a passphrase dada (ou sem) e devolve o caminho.
fn chave_de_teste(pasta: &Pasta, passphrase: Option<&str>) -> std::path::PathBuf {
    let mut gerador = KeyGen::new(KeyType::Ed25519).comment("ana@teste");
    if let Some(p) = passphrase {
        gerador = gerador.passphrase(p);
    }
    let chave = gerador.generate().expect("gerar a chave do teste");
    let caminho = pasta.ssh().join("id_ed25519");
    std::fs::create_dir_all(pasta.ssh()).unwrap();
    chave
        .write(&caminho, false)
        .expect("gravar a chave do teste");
    caminho
}

fn abrir_troca(app: &mut TestApp) {
    app.click("navegacao-chaves");
    app.click("chave-id_ed25519-mais");
    app.click("chave-id_ed25519-trocar");
    assert!(app.is_open("chaves-trocar-dialogo"));
}

#[test]
fn a_lista_mostra_a_chave_da_pasta() {
    let pasta = Pasta::nova("chaves-lista");
    chave_de_teste(&pasta, None);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    assert!(m.app.visible("chave-id_ed25519"));
    assert!(
        m.app
            .text("chave-id_ed25519-fingerprint")
            .starts_with("SHA256:")
    );
    m.app.assert_lint();
}

#[test]
fn trocar_a_passphrase_cifra_e_guarda_no_cofre() {
    let pasta = Pasta::nova("chaves-trocar");
    let caminho = chave_de_teste(&pasta, Some("velha"));
    let mut m = montar(&pasta, Theme::Light);
    abrir_troca(&mut m.app);
    m.app.assert_lint();
    m.app.type_into("chaves-trocar-atual", "velha");
    m.app.type_into("chaves-trocar-nova", "nova-frase");
    m.app.type_into("chaves-trocar-repetir", "nova-frase");
    m.app.click("chaves-trocar-dialogo-primary");

    assert!(!m.app.is_open("chaves-trocar-dialogo"));
    assert!(m.app.visible("aviso-sucesso"));
    assert!(KeyFile::open(&caminho, Some("velha")).is_err());
    assert!(KeyFile::open(&caminho, Some("nova-frase")).is_ok());
    let guardadas: Vec<_> = pasta
        .cofre()
        .itens()
        .iter()
        .filter(|i| i.tipo == Tipo::Chave)
        .map(|i| i.senha.clone())
        .collect();
    assert_eq!(guardadas, ["nova-frase"]);
}

#[test]
fn trocar_com_a_atual_errada_reabre_com_erro() {
    let pasta = Pasta::nova("chaves-trocar-errada");
    let caminho = chave_de_teste(&pasta, Some("velha"));
    let mut m = montar(&pasta, Theme::Light);
    abrir_troca(&mut m.app);
    m.app.type_into("chaves-trocar-atual", "errada");
    m.app.type_into("chaves-trocar-nova", "x");
    m.app.type_into("chaves-trocar-repetir", "x");
    m.app.click("chaves-trocar-dialogo-primary");

    assert!(m.app.is_open("chaves-trocar-dialogo"), "reabre no erro");
    assert!(m.app.visible("chaves-trocar-erro"));
    assert!(
        KeyFile::open(&caminho, Some("velha")).is_ok(),
        "a chave não mudou"
    );
}

#[test]
fn trocar_com_novas_diferentes_reabre_com_erro() {
    let pasta = Pasta::nova("chaves-trocar-diferentes");
    chave_de_teste(&pasta, None);
    let mut m = montar(&pasta, Theme::Light);
    abrir_troca(&mut m.app);
    assert!(
        !m.app.visible("chaves-trocar-atual"),
        "chave sem passphrase"
    );
    m.app.type_into("chaves-trocar-nova", "a");
    m.app.type_into("chaves-trocar-repetir", "b");
    m.app.click("chaves-trocar-dialogo-primary");
    assert!(m.app.is_open("chaves-trocar-dialogo"));
    assert!(m.app.text("chaves-trocar-erro").contains("conferem"));
}

// ---------------------------------------------------------------------------------------
// O destino da chave nova: o cofre (padrão) ou um arquivo
// ---------------------------------------------------------------------------------------

/// Quantos arquivos há na pasta `.ssh` de teste (0 se ela nem existe).
fn arquivos_em_ssh(pasta: &Pasta) -> usize {
    std::fs::read_dir(pasta.ssh()).map_or(0, |d| d.count())
}

#[test]
fn o_destino_padrao_e_o_cofre_e_desabilita_o_que_e_do_arquivo() {
    let pasta = Pasta::nova("chaves-padrao");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    assert_eq!(valor_da_grade(&m.app, "Destino"), "Cofre (recomendado)");
    assert_eq!(valor_da_grade(&m.app, "Nome no cofre"), "id_ed25519");
    for nome in ["Destino", "Tipo", "Comentário", "Nome no cofre"] {
        assert!(habilitada(&m.app, nome), "{nome} habilitada no cofre");
    }
    for nome in [
        "Arquivo",
        "Proteção",
        "Passphrase",
        "Repetir",
        "Salvar a passphrase no cofre",
    ] {
        assert!(!habilitada(&m.app, nome), "{nome} desabilitada no cofre");
    }
    m.app.assert_lint();

    destino_arquivo(&mut m.app);
    assert!(!habilitada(&m.app, "Nome no cofre"));
    for nome in ["Arquivo", "Proteção", "Passphrase", "Repetir"] {
        assert!(habilitada(&m.app, nome), "{nome} habilitada no arquivo");
    }
    m.app.assert_lint();
}

#[test]
fn o_nome_no_cofre_acompanha_o_tipo_ate_ser_editado() {
    let pasta = Pasta::nova("chaves-nome-segue-tipo");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    escolher(&mut m.app, "Tipo");
    m.app.select(&format!("{GRADE}-Tipo-valor"), "ECDSA P-256");
    assert_eq!(valor_da_grade(&m.app, "Nome no cofre"), "id_ecdsa");
    editar(&mut m.app, "Nome no cofre", "minha-chave");
    escolher(&mut m.app, "Tipo");
    m.app.select(&format!("{GRADE}-Tipo-valor"), "Ed25519");
    assert_eq!(valor_da_grade(&m.app, "Nome no cofre"), "minha-chave");
}

#[test]
fn gerar_no_cofre_guarda_o_item_sem_criar_arquivo_e_poe_no_agente() {
    let pasta = Pasta::nova("chaves-gera-no-cofre");
    pasta.agente_padrao(true);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    editar(&mut m.app, "Nome no cofre", "servidor-novo");
    m.app.click("gerar-chave-gerar");
    esperar_geracao(&mut m.app);
    assert!(
        !m.app.visible("gerar-chave-erro"),
        "erro: {}",
        m.app.text("gerar-chave-erro")
    );

    // O item do cofre, com a privada sem passphrase; nenhum arquivo em ~/.ssh.
    let cofre = pasta.cofre();
    let itens = cofre.chaves_ssh();
    assert_eq!(itens.len(), 1, "um item Chave SSH");
    let item = itens[0];
    assert_eq!(item.titulo, "servidor-novo");
    let chave = item.chave.as_ref().expect("a chave do item");
    assert!(chave.no_agente);
    assert!(chave.publica.starts_with("ssh-ed25519 "));
    assert!(
        KeyFile::from_text(&chave.privada, None).is_ok(),
        "a privada está sem passphrase"
    );
    assert!(item.senha.is_empty(), "sem passphrase guardada");
    assert_eq!(arquivos_em_ssh(&pasta), 0, "nada em ~/.ssh");
    assert!(!pasta.ssh().join("id_ed25519").exists());

    // O resultado mostra a impressão digital, o randomart e a pública do item.
    assert!(
        m.app
            .text("gerar-chave-resultado")
            .contains("servidor-novo")
    );
    let impressao = m.app.text("gerar-chave-fingerprint");
    assert!(impressao.contains(&chave.fingerprint), "{impressao}");
    assert!(m.app.visible("gerar-chave-randomart"));
    assert!(
        m.app
            .text("gerar-chave-publica")
            .starts_with("ssh-ed25519 ")
    );
    // A lista "Chaves no cofre" já mostra a chave nova.
    assert!(m.app.visible(&format!("chave-cofre-{}", item.id)));

    // O agente recebeu a chave, com a origem do cofre.
    let sufixo: String = chave
        .fingerprint
        .trim_start_matches("SHA256:")
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    m.app.click("navegacao-agente");
    m.app.wait(Duration::from_millis(100));
    let linha = format!("agente-chave-{sufixo}");
    assert!(m.app.visible(&linha), "a chave está no agente");
    assert_eq!(
        m.app.text(&format!("{linha}-origem")),
        "Cofre · servidor-novo"
    );
    m.app.assert_lint();
}

#[test]
fn gerar_no_cofre_sem_carregar_no_agente_deixa_a_chave_de_fora() {
    let pasta = Pasta::nova("chaves-gera-no-cofre-sem-agente");
    pasta.agente_padrao(true);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    sem_agente(&mut m.app);
    m.app.click("gerar-chave-gerar");
    esperar_geracao(&mut m.app);
    assert!(!m.app.visible("gerar-chave-erro"));

    let cofre = pasta.cofre();
    let chave = cofre.chaves_ssh()[0]
        .chave
        .clone()
        .expect("a chave do item");
    assert!(!chave.no_agente, "o item não manda a chave para o agente");
    m.app.click("navegacao-agente");
    m.app.wait(Duration::from_millis(100));
    assert!(m.app.visible("agente-vazio"), "o agente ficou vazio");
}

#[test]
fn o_cofre_bloqueado_recusa_antes_de_gerar() {
    let pasta = Pasta::nova("chaves-gera-cofre-bloqueado");
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("cofre-desbloquear-close");
    m.app.click("navegacao-chaves");
    m.app.click("gerar-chave-gerar");

    assert!(m.app.visible("gerar-chave-erro"));
    assert!(m.app.text("gerar-chave-erro").contains("bloqueado"));
    assert!(
        !m.app.visible("gerar-chave-andamento"),
        "nem começou a gerar"
    );
    assert_eq!(arquivos_em_ssh(&pasta), 0);

    // Com o destino Arquivo, o cofre bloqueado não atrapalha.
    destino_arquivo(&mut m.app);
    sem_agente(&mut m.app);
    m.app.click("gerar-chave-gerar");
    esperar_geracao(&mut m.app);
    assert!(pasta.ssh().join("id_ed25519").is_file());
}

#[test]
fn a_chave_gerada_no_cofre_serve_a_conexao_escolhida() {
    let pasta = Pasta::nova("chaves-gera-usar-cofre");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-terminais");
    m.app.click("arvore-1");
    m.app.click("navegacao-chaves");
    m.app.click("gerar-chave-gerar");
    esperar_geracao(&mut m.app);
    assert!(m.app.visible("gerar-chave-usar"));
    m.app.click("gerar-chave-usar");

    let uid = pasta.cofre().chaves_ssh()[0].uid.clone();
    let conexao = pasta.conexoes().connection(1).unwrap().clone();
    assert_eq!(conexao.chave_do_cofre.as_deref(), Some(uid.as_str()));
    assert_eq!(conexao.key_file, None, "a conexão não aponta para arquivo");
}
