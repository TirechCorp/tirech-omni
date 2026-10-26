//! As páginas Chaves SSH e Agente nos dois temas: snapshots com chaves fixas (os arquivos de
//! `tests/dados/`) e o lint com cada diálogo aberto. Também a confirmação de uso da chave.

mod comum;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tirech_omni::agente::ChaveSalva;
use tirech_omni::configuracoes::Configuracoes;
use tomni_ui::Key;
use tomni_ui::kurbo::Point;
use tomni_ui::terminal::ssh::CopyIdOutcome;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar, montar_com_ambiente};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

fn dados(nome: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/dados")
        .join(nome)
}

/// Copia as chaves de `tests/dados/` para `ssh/` da pasta.
fn semear_chaves(pasta: &Pasta) {
    std::fs::create_dir_all(pasta.ssh()).unwrap();
    for nome in [
        "id_ed25519",
        "id_ed25519.pub",
        "id_trabalho",
        "id_trabalho.pub",
    ] {
        std::fs::copy(dados(nome), pasta.ssh().join(nome)).unwrap();
    }
}

/// Grava o `configuracoes.json` da pasta com o agente ligado ou não e a lista de chaves.
fn agente_json(pasta: &Pasta, ligado: bool, chaves: Vec<ChaveSalva>) {
    let mut config = Configuracoes::default();
    config.agente.ligado = ligado;
    config.agente.chaves = chaves;
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    config.salvar(&pasta.armazenamento()).unwrap();
}

/// Uma pasta de caminho fixo (o do socket aparece na página Agente e entra no snapshot).
fn pasta_fixa(nome: &str) -> Pasta {
    let pasta = Pasta(PathBuf::from(format!("/tmp/tirech-omni-visual-{nome}")));
    let _ = std::fs::remove_dir_all(&pasta.0);
    pasta
}

/// Troca uma propriedade de texto do formulário de nova chave.
fn editar(app: &mut TestApp, nome: &str, texto: &str) {
    let grade = "gerar-chave-props";
    let r = app.rect(&format!("{grade}-Tipo"));
    app.click_at(Point::new(r.x0 + 24.0, (r.y0 + r.y1) / 2.0), 1);
    for _ in 0..20 {
        if app
            .properties(grade)
            .iter()
            .any(|p| p.selected && p.name == nome)
        {
            break;
        }
        app.key(Key::Down);
    }
    app.key(Key::Enter);
    let atual = app.text(&format!("{grade}-{nome}-valor"));
    app.key(Key::End);
    for _ in 0..atual.chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text(texto).key(Key::Enter);
}

fn esperar(app: &mut TestApp, id: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !app.is_open(id) {
        assert!(Instant::now() < limite, "o diálogo {id} não abriu");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn snapshot_da_pagina_chaves() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("chaves-{nome}"));
        semear_chaves(&pasta);
        let mut m = montar(&pasta, tema);
        m.app.click("navegacao-chaves");
        // O comentário padrão é usuário@máquina: troca por um fixo.
        editar(&mut m.app, "Comentário", "ana@teste");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("chaves-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

/// Um item de chave SSH do cofre com a `id_ed25519` fixa, para o fingerprint não mudar.
fn item_da_chave_fixa(titulo: &str) -> tirech_omni::cofre::Item {
    use tirech_omni::cofre::{ChaveSsh, Item, Tipo};
    use tomni_ui::terminal::ssh::keys::KeyFile;
    let texto = std::fs::read_to_string(dados("id_ed25519")).unwrap();
    let aberta = KeyFile::from_text(&texto, None).expect("abrir a chave fixa");
    let mut chave = ChaveSsh::de_chave(aberta.private_key()).unwrap();
    chave.no_agente = true;
    Item {
        tipo: Tipo::ChaveSsh,
        titulo: titulo.to_owned(),
        chave: Some(chave),
        ..Item::default()
    }
}

#[test]
fn snapshot_da_pagina_agente() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("agente-{nome}"));
        semear_chaves(&pasta);
        std::fs::copy(dados("id_agente"), pasta.ssh().join("id_agente")).unwrap();
        agente_json(
            &pasta,
            true,
            vec![ChaveSalva::new(pasta.ssh().join("id_agente"))],
        );
        pasta.semear_cofre(vec![item_da_chave_fixa("github")]);
        let mut m = montar(&pasta, tema);
        m.app.click("navegacao-agente");
        m.app.resize(1280, 1000);
        m.app.wait(Duration::from_millis(100));
        assert_eq!(m.app.text("agente-chave-JKfoY9ou-origem"), "Cofre · github");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("agente-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn snapshot_da_pagina_agente_no_windows() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("agente-windows-{nome}"));
        semear_chaves(&pasta);
        agente_json(&pasta, true, Vec::new());
        let (mut m, ferr) = montar_com_ambiente(&pasta, tema, |a| {
            a.windows = true;
            a.socket = tirech_omni::agente::endereco_em(true, false, None, None);
            // A página mostra o pipe do Windows; o agente de verdade liga num socket da pasta.
            a.socket_real = Some(pasta.caminho().join("agent.sock"));
        });
        m.app.click("navegacao-agente");
        m.app.resize(1280, 1000);
        ferr.agente.marcar_servico_ocupa(true);
        ferr.agente.marcar_pageant_ocupado(true);
        m.app.wait(Duration::from_millis(100));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("agente-windows-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn lint_dos_dialogos_do_agente() {
    for (_, tema) in TEMAS {
        let pasta = Pasta::nova("lint-agente");
        semear_chaves(&pasta);
        agente_json(&pasta, true, Vec::new());
        let mut m = montar(&pasta, tema);
        m.app.click("navegacao-agente");

        m.app.click("agente-adicionar");
        assert!(m.app.is_open("agente-adicionar-dialogo"));
        m.app.assert_lint();
        m.app.click("agente-adicionar-dialogo-close");

        m.app.click("agente-travar");
        assert!(m.app.is_open("agente-travar-dialogo"));
        m.app.assert_lint();
        m.app.click("agente-travar-dialogo-close");
    }
}

#[test]
fn lint_do_dialogo_de_passphrase_ao_iniciar() {
    for (_, tema) in TEMAS {
        let pasta = Pasta::nova("lint-passphrase");
        semear_chaves(&pasta);
        agente_json(
            &pasta,
            true,
            vec![ChaveSalva::new(pasta.ssh().join("id_trabalho"))],
        );
        let mut m = montar(&pasta, tema);
        esperar(&mut m.app, "agente-passphrase");
        assert!(
            m.app
                .text("agente-passphrase-arquivo")
                .contains("id_trabalho")
        );
        m.app.assert_lint();
        m.app.type_into("agente-passphrase-campo", "errada");
        m.app.click("agente-passphrase-primary");
        assert!(
            m.app.is_open("agente-passphrase"),
            "reabre com a passphrase errada"
        );
        assert!(m.app.visible("agente-passphrase-erro"));
        m.app.type_into("agente-passphrase-campo", "teste");
        m.app.click("agente-passphrase-primary");
        m.app.wait(Duration::from_millis(100));
        assert!(!m.app.is_open("agente-passphrase"));
    }
}

#[test]
fn lint_dos_dialogos_de_copiar_e_trocar() {
    for (_, tema) in TEMAS {
        let pasta = Pasta::nova("lint-copiar");
        semear_chaves(&pasta);
        pasta.semear(&exemplo());
        let mut m = montar(&pasta, tema);
        *m.opener.resposta_da_instalacao.borrow_mut() = Ok(CopyIdOutcome::AlreadyThere);
        m.app.click("menus-ferramentas").click("copiar-chave-menu");
        assert!(m.app.is_open("copiar-chave-dialogo"));
        m.app.assert_lint();
        m.app.click("copiar-chave-dialogo-close");

        m.app.click("navegacao-chaves");
        m.app.click("chave-id_trabalho-mais");
        m.app.click("chave-id_trabalho-trocar");
        assert!(m.app.is_open("chaves-trocar-dialogo"));
        m.app.assert_lint();
    }
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

fn montar_com_confirmacao(nome: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    semear_chaves(&pasta);
    agente_json(
        &pasta,
        true,
        vec![ChaveSalva {
            confirmar: true,
            ..ChaveSalva::new(pasta.ssh().join("id_ed25519"))
        }],
    );
    let m = montar(&pasta, tema);
    (pasta, m)
}

#[test]
fn negar_a_confirmacao_faz_a_assinatura_falhar() {
    for (_, tema) in TEMAS {
        let (pasta, mut m) = montar_com_confirmacao("uso-negar", tema);
        let Some(filho) = pedir_assinatura(&pasta) else {
            eprintln!("sem ssh-keygen: teste pulado");
            return;
        };
        esperar(&mut m.app, "agente-uso-dialogo");
        assert_eq!(m.app.text("agente-uso-chave"), "ana@teste");
        assert!(
            m.app.text("agente-uso-programa").contains("ssh-keygen"),
            "quem pede: {}",
            m.app.text("agente-uso-programa")
        );
        assert!(m.app.text("agente-uso-fingerprint").starts_with("SHA256:"));
        m.app.assert_lint();
        m.app.click("agente-uso-dialogo-secondary");
        let saida = filho.wait_with_output().unwrap();
        assert!(!saida.status.success(), "a assinatura devia ter falhado");
        assert!(!m.app.is_open("agente-uso-dialogo"));
    }
}

#[test]
fn permitir_a_confirmacao_assina() {
    let (pasta, mut m) = montar_com_confirmacao("uso-permitir", Theme::Light);
    let Some(filho) = pedir_assinatura(&pasta) else {
        eprintln!("sem ssh-keygen: teste pulado");
        return;
    };
    esperar(&mut m.app, "agente-uso-dialogo");
    m.app.click("agente-uso-dialogo-primary");
    let saida = filho.wait_with_output().unwrap();
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

#[test]
fn lembrar_evita_a_segunda_pergunta_do_mesmo_programa() {
    let (pasta, mut m) = montar_com_confirmacao("uso-lembrar", Theme::Light);
    let Some(primeiro) = pedir_assinatura(&pasta) else {
        eprintln!("sem ssh-keygen: teste pulado");
        return;
    };
    esperar(&mut m.app, "agente-uso-dialogo");
    assert!(!m.app.checked("agente-uso-lembrar"));
    m.app.click("agente-uso-lembrar");
    m.app.click("agente-uso-dialogo-primary");
    assert!(primeiro.wait_with_output().unwrap().status.success());

    // A segunda vez, do mesmo programa e com a mesma chave, assina sem abrir o diálogo.
    let segundo = pedir_assinatura(&pasta).expect("o ssh-keygen existe");
    let saida = segundo.wait_with_output().unwrap();
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    m.app.wait(Duration::from_millis(50));
    assert!(!m.app.is_open("agente-uso-dialogo"));
}

/// Importa a `id_ed25519` fixa pelo botão e cola a pública do `id_trabalho`: duas chaves no cofre.
fn duas_chaves_no_cofre(m: &mut Montagem, pasta: &Pasta) {
    m.app
        .file_dialog_answer(Some(pasta.ssh().join("id_ed25519")));
    m.app.click("chaves-importar");
    m.app.click("chaves-importar-dialogo-primary");
    m.app.click("chaves-colar");
    let publica = std::fs::read_to_string(dados("id_trabalho.pub")).unwrap();
    m.app.type_into("chaves-importar-texto", publica.trim());
    m.app.click("chaves-importar-dialogo-primary");
}

#[test]
fn snapshot_da_pagina_chaves_com_o_cofre() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("chaves-cofre-{nome}"));
        semear_chaves(&pasta);
        let mut m = montar(&pasta, tema);
        m.app.click("navegacao-chaves");
        duas_chaves_no_cofre(&mut m, &pasta);
        assert!(!m.app.is_open("chaves-importar-dialogo"));
        editar(&mut m.app, "Comentário", "ana@teste");
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("chaves-cofre-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn snapshot_dos_dialogos_de_importar_e_exportar() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("chaves-dialogos-{nome}"));
        semear_chaves(&pasta);
        let mut m = montar(&pasta, tema);
        m.app.click("navegacao-chaves");
        editar(&mut m.app, "Comentário", "ana@teste");

        // Importar uma chave com passphrase: o campo da passphrase aparece.
        m.app
            .file_dialog_answer(Some(pasta.ssh().join("id_trabalho")));
        m.app.click("chaves-importar");
        assert!(m.app.visible("chaves-importar-passphrase"));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("chaves-importar-{nome}"));
        m.app.click("chaves-importar-dialogo-close");

        // Exportar uma chave Ed25519 como PEM tradicional: o aviso e o Salvar desligado.
        m.app
            .file_dialog_answer(Some(pasta.ssh().join("id_ed25519")));
        m.app.click("chaves-converter");
        m.app.select(
            "chaves-converter-formato",
            "PEM tradicional (AWS, Paramiko)",
        );
        assert!(m.app.visible("chaves-converter-aviso"));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("chaves-converter-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}
