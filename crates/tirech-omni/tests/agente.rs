//! A página "Agente": a chave gerada entra, aparece na lista, o `ssh-add` de fora a enxerga,
//! "Travar" esvazia e "Remover" tira.

mod comum;

use std::process::Command;
use std::time::{Duration, Instant};

use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar, montar_com_ambiente};

/// Pasta com o agente embutido ligado.
fn pasta_com_agente(nome: &str) -> Pasta {
    let pasta = Pasta::nova(nome);
    pasta.agente_padrao(true);
    pasta
}

fn esperar_geracao(app: &mut TestApp) {
    let limite = Instant::now() + Duration::from_secs(30);
    loop {
        app.wait(Duration::from_millis(20));
        if !app.visible("gerar-chave-andamento") {
            app.wait(Duration::from_millis(50));
            return;
        }
        assert!(Instant::now() < limite, "a chave não terminou de gerar");
    }
}

/// Gera a Ed25519 padrão na página Chaves (que a carrega no agente) e devolve o sufixo dos ids
/// da linha dela na página Agente.
fn gerar_e_ir_ao_agente(m: &mut Montagem) -> String {
    m.app.click("navegacao-chaves");
    m.app.click("gerar-chave-gerar");
    esperar_geracao(&mut m.app);
    assert!(
        !m.app.visible("gerar-chave-erro"),
        "erro: {}",
        m.app.text("gerar-chave-erro")
    );
    let impressao = m.app.text("gerar-chave-fingerprint");
    let sufixo: String = impressao
        .rsplit("SHA256:")
        .next()
        .unwrap()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    m.app.click("navegacao-agente");
    m.app.wait(Duration::from_millis(100));
    sufixo
}

#[test]
fn a_chave_gerada_aparece_na_lista_do_agente() {
    let pasta = pasta_com_agente("agente-lista");
    let mut m = montar(&pasta, Theme::Light);
    let sufixo = gerar_e_ir_ao_agente(&mut m);

    let linha = format!("agente-chave-{sufixo}");
    assert!(m.app.visible(&linha));
    assert!(
        m.app
            .text(&format!("{linha}-detalhe"))
            .contains("não expira")
    );
    assert!(m.app.text("agente-socket").contains("agent.sock"));
    assert!(pasta.caminho().join("agent.sock").exists());
    m.app.assert_lint();
}

#[test]
fn ssh_add_de_fora_enxerga_a_chave_do_agente_embutido() {
    if Command::new("ssh-add").arg("-h").output().is_err() {
        eprintln!("sem ssh-add no PATH: teste pulado");
        return;
    }
    let pasta = pasta_com_agente("agente-ssh-add");
    let mut m = montar(&pasta, Theme::Light);
    gerar_e_ir_ao_agente(&mut m);

    let socket = pasta.caminho().join("agent.sock");
    let saida = Command::new("ssh-add")
        .arg("-l")
        .env("SSH_AUTH_SOCK", &socket)
        .output()
        .expect("rodar o ssh-add");
    let texto = String::from_utf8_lossy(&saida.stdout).into_owned();
    assert!(saida.status.success(), "ssh-add falhou: {texto}");
    assert!(texto.contains("ED25519"), "lista: {texto}");
}

#[test]
fn travar_esvazia_e_destravar_devolve() {
    let pasta = pasta_com_agente("agente-travar");
    let mut m = montar(&pasta, Theme::Light);
    let sufixo = gerar_e_ir_ao_agente(&mut m);
    let linha = format!("agente-chave-{sufixo}");
    assert!(m.app.visible(&linha));

    m.app.click("agente-travar");
    assert!(m.app.is_open("agente-travar-dialogo"));
    m.app.type_into("agente-travar-passphrase", "trava-1");
    m.app.click("agente-travar-dialogo-primary");
    m.app.wait(Duration::from_millis(100));
    assert!(m.app.label("agente-travar").contains("Destravar"));
    assert!(m.app.visible("agente-vazio"));

    m.app.click("agente-travar");
    m.app.type_into("agente-travar-passphrase", "errada");
    m.app.click("agente-travar-dialogo-primary");
    assert!(m.app.is_open("agente-travar-dialogo"), "reabre no erro");
    assert!(m.app.text("agente-travar-erro").contains("não confere"));
    m.app.key(Key::Escape);

    m.app.click("agente-travar");
    m.app.type_into("agente-travar-passphrase", "trava-1");
    m.app.click("agente-travar-dialogo-primary");
    m.app.wait(Duration::from_millis(100));
    assert!(m.app.label("agente-travar").contains("Travar"));
    assert!(m.app.visible(&linha));
}

#[test]
fn remover_tira_a_chave_da_lista() {
    let pasta = pasta_com_agente("agente-remover");
    let mut m = montar(&pasta, Theme::Light);
    let sufixo = gerar_e_ir_ao_agente(&mut m);
    m.app.click(&format!("agente-chave-{sufixo}-remover"));
    m.app.wait(Duration::from_millis(100));
    assert!(m.app.visible("agente-vazio"));
    let json = std::fs::read_to_string(pasta.armazenamento().arquivo_configuracoes()).unwrap();
    assert!(!json.contains("id_ed25519"), "configuracoes.json: {json}");
}

#[test]
fn desligado_mostra_o_aviso() {
    let pasta = Pasta::nova("agente-desligado");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-agente");
    assert!(!m.app.checked("agente-ligado"));
    assert!(m.app.text("agente-vazio").contains("desligado"));
    assert!(!m.app.enabled("agente-travar"));
    m.app.assert_lint();
}

/// O `agente.socket` das configurações, com 200 bytes, numa pasta temporária.
fn pedido_comprido(pasta: &Pasta) -> std::path::PathBuf {
    let pedido = pasta.caminho().join("p".repeat(200)).join("agent.sock");
    pasta.configurar(
        "agente",
        "socket",
        serde_json::json!(pedido.display().to_string()),
    );
    pedido
}

#[test]
fn um_socket_comprido_nas_configuracoes_liga_no_curto() {
    let pasta = pasta_com_agente("agente-socket-longo");
    let pedido = pedido_comprido(&pasta);
    let (mut m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    let curto = ferr.socket();
    assert_ne!(curto, pedido);
    assert!(curto.as_os_str().len() <= 107, "{curto:?}");
    assert!(ferr.socket_encurtado());
    assert!(ferr.agente.rodando(), "{:?}", ferr.agente.erro());
    assert!(curto.exists(), "o socket curto não foi criado: {curto:?}");
    assert!(!pedido.exists());
    m.app.click("navegacao-agente");
    assert!(m.app.visible("agente-endereco-encurtado"));
    assert!(
        m.app
            .text("agente-socket")
            .contains(&curto.display().to_string())
    );
    // Abrir de novo dá o mesmo endereço.
    let (_m2, ferr2) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    assert_eq!(ferr2.socket(), curto);
    let _ = std::fs::remove_file(&curto);
}

#[test]
fn o_bloco_do_ssh_config_usa_o_endereco_curto() {
    let pasta = pasta_com_agente("agente-bloco-curto");
    let pedido = pedido_comprido(&pasta);
    let (_m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    let bloco = tirech_omni::sistema::bloco_ssh_config(&ferr.socket_do_sistema());
    assert!(
        bloco.contains(&ferr.socket().display().to_string()),
        "{bloco}"
    );
    assert!(!bloco.contains(&pedido.display().to_string()), "{bloco}");
    let _ = std::fs::remove_file(ferr.socket());
}

#[test]
fn um_socket_curto_nao_mostra_a_dica() {
    let pasta = pasta_com_agente("agente-socket-curto");
    let (mut m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    assert!(!ferr.socket_encurtado());
    m.app.click("navegacao-agente");
    assert!(!m.app.visible("agente-endereco-encurtado"));
}

#[test]
fn ligar_com_endereco_relativo_falha_e_nao_cria_nada() {
    let pasta = pasta_com_agente("agente-relativo");
    let (_m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    let antes = arquivos_do_crate();
    let erro = ferr
        .agente
        .ligar(
            std::path::Path::new("socket-relativo-do-teste"),
            &ferr.cfg(),
        )
        .unwrap_err();
    assert!(erro.contains("caminho completo"), "{erro}");
    let erro = ferr
        .agente
        .ligar(
            std::path::Path::new(r"\\.\pipe\openssh-ssh-agent"),
            &ferr.cfg(),
        )
        .unwrap_err();
    assert!(erro.contains("caminho completo"), "{erro}");
    assert_eq!(arquivos_do_crate(), antes);
}

fn arquivos_do_crate() -> Vec<String> {
    let mut nomes: Vec<String> = std::fs::read_dir(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    nomes.sort();
    nomes
}

/// A rede de segurança: nenhum teste deixa um socket com nome de pipe do Windows no crate.
#[test]
fn nenhum_teste_deixa_socket_no_crate() {
    let sobras: Vec<_> = arquivos_do_crate()
        .into_iter()
        .filter(|n| n.starts_with(r"\\.\pipe\") || n.ends_with(".sock"))
        .collect();
    assert!(sobras.is_empty(), "sobrou no crate: {sobras:?}");
}
