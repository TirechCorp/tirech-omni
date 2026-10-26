//! "Programas do sistema" na página Agente: o bloco do `~/.ssh/config`, os shells, a sessão
//! gráfica, o Windows e o Pageant (forçados no Linux pelo ambiente), e o agente carregando o
//! cofre. O `ssh` e o `ssh-add` de verdade conferem o que o app escreve e o que ele atende.

mod comum;

use std::cell::RefCell;
use std::io;
#[cfg(unix)]
use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use tirech_omni::ferramentas::Ambiente;
#[cfg(unix)]
use tirech_omni::sistema::{self, Lugar};
use tirech_omni::sistema::{INICIO_CONFIG, INICIO_SHELL};
use tomni_ui::prelude::Update;
use tomni_ui::theme::Theme;

use comum::{Pasta, item_chave_ssh, montar_com_ambiente};

type Comandos = Rc<RefCell<Vec<String>>>;

/// Um ambiente cujo executor anota o que rodaria e responde `resposta`.
fn anotando(
    comandos: &Comandos,
    resposta: fn() -> io::Result<String>,
) -> impl FnOnce(&mut Ambiente) {
    let comandos = Rc::clone(comandos);
    move |a| {
        a.executor = Rc::new(move |programa, argumentos| {
            comandos
                .borrow_mut()
                .push(format!("{programa} {}", argumentos.join(" ")));
            resposta()
        });
    }
}

fn ok() -> io::Result<String> {
    Ok(String::new())
}

fn sem_o_programa() -> io::Result<String> {
    Err(io::ErrorKind::NotFound.into())
}

/// Uma pasta com o agente ligado, o `~/.ssh` criado e um `config` que já tinha coisa da pessoa.
fn pasta_com_config(nome: &str) -> Pasta {
    let pasta = Pasta::nova(nome);
    pasta.agente_padrao(true);
    std::fs::create_dir_all(pasta.ssh()).unwrap();
    std::fs::write(
        pasta.ssh().join("config"),
        "Host exemplo\n    HostName exemplo.test\n",
    )
    .unwrap();
    pasta
}

fn config_do(pasta: &Pasta) -> String {
    std::fs::read_to_string(pasta.ssh().join("config")).unwrap_or_default()
}

#[cfg(unix)]
fn ha_ssh() -> bool {
    Command::new("ssh").arg("-V").output().is_ok()
}

// --- O bloco do ssh_config, com o ssh de verdade ---

#[cfg(unix)]
#[test]
fn o_bloco_do_ssh_config_segue_o_socket_e_nao_atrapalha_o_resto() {
    use std::os::unix::net::UnixListener;

    if !ha_ssh() {
        eprintln!("sem ssh no PATH: teste pulado");
        return;
    }
    let pasta = Pasta::nova("sistema-ssh-g");
    std::fs::create_dir_all(&pasta.0).unwrap();
    let socket = pasta.caminho().join("agent.sock");
    let config = pasta.caminho().join("config");
    std::fs::write(
        &config,
        "User global\n\nHost exemplo\n    HostName exemplo.test\n",
    )
    .unwrap();
    let bloco = sistema::bloco_ssh_config(&socket);
    sistema::aplicar_bloco(&config, Some(&bloco), Lugar::Topo, true, None).unwrap();

    let g = |config: &Path| -> String {
        let saida = Command::new("ssh")
            .arg("-F")
            .arg(config)
            .args(["-G", "exemplo"])
            .output()
            .expect("rodar o ssh");
        String::from_utf8_lossy(&saida.stdout).into_owned()
    };

    // Sem o socket, o bloco não vale e o que vem depois dele continua valendo.
    let sem = g(&config);
    assert!(!sem.contains(&socket.display().to_string()), "{sem}");
    assert!(sem.lines().any(|l| l == "user global"), "{sem}");
    assert!(sem.lines().any(|l| l == "hostname exemplo.test"), "{sem}");

    // Com o socket atendendo, o `ssh` usa o agente do app, e o resto também continua valendo.
    let _atende = UnixListener::bind(&socket).unwrap();
    let com = g(&config);
    assert!(
        com.lines()
            .any(|l| l == format!("identityagent {}", socket.display())),
        "{com}"
    );
    assert!(com.lines().any(|l| l == "user global"), "{com}");
    assert!(com.lines().any(|l| l == "hostname exemplo.test"), "{com}");
}

// --- A página ---

#[test]
fn ligar_o_ssh_do_sistema_escreve_o_bloco_e_desligar_tira() {
    let pasta = pasta_com_config("sistema-ligar");
    let original = config_do(&pasta);
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    m.app.click("navegacao-agente");
    assert_eq!(
        m.app.text("agente-sistema-estado"),
        "O ssh do sistema ainda não usa as chaves do app"
    );
    assert!(m.app.visible("agente-sistema-aviso"));
    m.app.assert_lint();

    m.app.click("agente-sistema-ssh-config");
    m.app.wait(Duration::from_millis(50));
    let texto = config_do(&pasta);
    assert!(texto.contains(INICIO_CONFIG), "{texto}");
    assert!(texto.contains("agent.sock"), "{texto}");
    assert!(
        texto.ends_with(&original),
        "o que a pessoa tinha fica: {texto}"
    );
    assert_eq!(
        m.app.text("agente-sistema-estado"),
        "O ssh do sistema usa as chaves do app"
    );
    assert!(!m.app.visible("agente-sistema-aviso"));
    let backup = pasta.ssh().join("config.tirech-bak");
    assert_eq!(std::fs::read_to_string(backup).unwrap(), original);

    m.app.click("agente-sistema-ssh-config");
    m.app.wait(Duration::from_millis(50));
    assert_eq!(config_do(&pasta), original, "desligar devolve o arquivo");
    assert_eq!(
        m.app.text("agente-sistema-estado"),
        "O ssh do sistema ainda não usa as chaves do app"
    );
}

#[test]
fn usar_no_ssh_do_sistema_liga_com_um_clique() {
    let pasta = pasta_com_config("sistema-um-clique");
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    m.app.click("navegacao-agente");
    m.app.click("agente-sistema-usar");
    m.app.wait(Duration::from_millis(50));
    assert!(config_do(&pasta).contains(INICIO_CONFIG));
    assert!(m.app.checked("agente-sistema-ssh-config"));
    assert!(!m.app.visible("agente-sistema-aviso"));
}

#[test]
fn ver_o_que_e_escrito_mostra_o_bloco_e_nao_grava_nada() {
    let pasta = pasta_com_config("sistema-ver-bloco");
    let original = config_do(&pasta);
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    m.app.click("navegacao-agente");
    m.app.click("agente-sistema-ver-bloco");
    assert!(m.app.is_open("agente-sistema-bloco-dialogo"));
    let texto = m.app.text("agente-sistema-bloco");
    assert!(texto.contains("Match exec"), "{texto}");
    assert!(texto.contains("IdentityAgent"), "{texto}");
    assert!(texto.contains("agent.sock"), "{texto}");
    m.app.assert_lint();
    assert_eq!(config_do(&pasta), original);
}

#[test]
fn os_shells_recebem_o_bloco_so_nos_arquivos_que_existem() {
    let pasta = pasta_com_config("sistema-shells");
    let bashrc = pasta.caminho().join(".bashrc");
    let zshrc = pasta.caminho().join(".zshrc");
    let fish = pasta.caminho().join(".config/fish/conf.d/tirech-omni.fish");
    std::fs::write(&bashrc, "alias ll='ls -l'\n").unwrap();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    m.app.click("navegacao-agente");
    m.app.click("agente-sistema-shells");
    m.app.wait(Duration::from_millis(50));

    let texto = std::fs::read_to_string(&bashrc).unwrap();
    assert!(texto.contains(INICIO_SHELL), "{texto}");
    assert!(
        texto.starts_with("alias ll='ls -l'\n"),
        "o bloco vai no fim"
    );
    assert!(!zshrc.exists(), "o .zshrc que não existe não é criado");
    assert!(
        std::fs::read_to_string(&fish)
            .unwrap()
            .contains("SSH_AUTH_SOCK"),
        "o do fish é só do app e é criado"
    );

    m.app.click("agente-sistema-shells");
    m.app.wait(Duration::from_millis(50));
    assert_eq!(
        std::fs::read_to_string(&bashrc).unwrap(),
        "alias ll='ls -l'\n"
    );
    assert!(!fish.exists(), "sem o bloco o arquivo do fish some");
}

#[test]
fn a_sessao_grafica_recebe_o_socket_pelo_systemctl() {
    let pasta = pasta_com_config("sistema-sessao");
    let comandos = Comandos::default();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, anotando(&comandos, ok));
    m.app.click("navegacao-agente");
    m.app.click("agente-sistema-sessao");
    m.app.wait(Duration::from_millis(50));
    let dados = comandos.borrow().clone();
    assert!(
        dados.iter().any(
            |c| c.starts_with("systemctl --user set-environment SSH_AUTH_SOCK=")
                && c.ends_with("agent.sock")
        ),
        "{dados:?}"
    );
    m.app.click("agente-sistema-sessao");
    m.app.wait(Duration::from_millis(50));
    let dados = comandos.borrow().clone();
    assert!(
        dados
            .iter()
            .any(|c| c == "systemctl --user unset-environment SSH_AUTH_SOCK"),
        "{dados:?}"
    );
}

#[test]
fn sem_o_systemctl_a_opcao_se_desliga_com_um_aviso() {
    let pasta = pasta_com_config("sistema-sem-systemctl");
    let comandos = Comandos::default();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, anotando(&comandos, sem_o_programa));
    m.app.click("navegacao-agente");
    m.app.click("agente-sistema-sessao");
    m.app.wait(Duration::from_millis(50));
    assert!(!m.app.checked("agente-sistema-sessao"));
    assert!(m.app.visible("aviso-atencao"), "o aviso explica");
}

// --- O Windows e o Pageant, forçados no Linux ---

#[test]
fn a_pagina_no_windows_mostra_o_pipe_o_git_e_o_pageant() {
    let pasta = Pasta::nova("sistema-windows");
    let (mut m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |a| {
        a.windows = true;
        a.socket = tirech_omni::agente::endereco_em(true, false, None, None);
    });
    m.app.click("navegacao-agente");
    let arvore = format!("{:?}", m.app.tree());
    for sumiu in [
        "agente-sistema-ssh-config",
        "agente-sistema-shells",
        "agente-sistema-ver-bloco",
    ] {
        assert!(
            !arvore.contains(sumiu),
            "{sumiu} só existe no Linux e no macOS"
        );
    }
    for ha in [
        "agente-sistema-pipe",
        "agente-sistema-git",
        "agente-pageant",
    ] {
        assert!(m.app.visible(ha), "{ha}");
    }
    assert!(
        m.app
            .text("agente-sistema-pipe")
            .contains("openssh-ssh-agent")
    );
    assert_eq!(
        m.app.text("agente-pageant-estado"),
        "Desligado",
        "fora do Windows não há Pageant de verdade"
    );
    assert!(!m.app.visible("agente-pageant-ocupado"));
    m.app.assert_lint();

    ferr.agente.marcar_pageant_ocupado(true);
    m.app.wait(Duration::from_millis(50));
    assert!(m.app.visible("agente-pageant-ocupado"));
    assert!(m.app.visible("agente-pageant-tentar"));
    m.app.assert_lint();
}

#[test]
fn o_servico_do_windows_ocupando_o_endereco_mostra_o_comando() {
    let pasta = Pasta::nova("sistema-servico");
    let (mut m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |a| a.windows = true);
    m.app.click("navegacao-agente");
    assert!(!m.app.visible("agente-sistema-servico"));
    ferr.agente.marcar_servico_ocupa(true);
    m.app.wait(Duration::from_millis(50));
    assert!(m.app.visible("agente-sistema-servico"));
    assert!(
        m.app
            .text("agente-sistema-servico")
            .contains("serviço ssh-agent")
    );
    assert!(m.app.visible("agente-sistema-copiar-comando"));
    assert!(m.app.visible("agente-sistema-tentar"));
    m.app.assert_lint();
}

// --- Outro Tirech Omni atendendo o endereço ---

#[cfg(unix)]
#[test]
fn outro_tirech_aberto_atende_o_agente_sem_erro() {
    use tomni_ui::terminal::ssh::agent::Agent;

    let pasta = Pasta::nova("sistema-outro-app");
    pasta.agente_padrao(true);
    std::fs::create_dir_all(&pasta.0).unwrap();
    let _outro = Agent::start(Some(pasta.caminho().join("agent.sock"))).unwrap();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    m.app.click("navegacao-agente");
    assert!(m.app.visible("agente-sistema-outro"));
    assert!(!m.app.visible("agente-erro"), "não é erro");
    assert!(m.app.checked("agente-ligado"));
}

// --- O agente carrega o cofre ---

/// A saída do `ssh-add -l` contra o agente da pasta.
fn ssh_add(pasta: &Pasta) -> Option<String> {
    let saida = Command::new("ssh-add")
        .arg("-l")
        .env("SSH_AUTH_SOCK", pasta.caminho().join("agent.sock"))
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&saida.stdout).into_owned())
}

#[test]
fn o_agente_so_leva_as_chaves_do_cofre_marcadas_no_agente() {
    let pasta = Pasta::nova("sistema-cofre-agente");
    pasta.agente_padrao(true);
    let (item_a, fp_a) = item_chave_ssh("github", true);
    let (item_b, fp_b) = item_chave_ssh("so-no-cofre", false);
    pasta.semear_cofre(vec![item_a, item_b]);
    let (mut m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    m.app.click("navegacao-agente");
    m.app.wait(Duration::from_millis(100));

    assert!(ferr.agente.tem(&fp_a));
    assert!(!ferr.agente.tem(&fp_b));
    if let Some(lista) = ssh_add(&pasta) {
        assert!(lista.contains(&fp_a), "{lista}");
        assert!(!lista.contains(&fp_b), "{lista}");
        assert_eq!(lista.lines().count(), 1, "{lista}");
    }
    // A origem aparece na lista.
    let sufixo: String = fp_a
        .trim_start_matches("SHA256:")
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(8)
        .collect();
    assert_eq!(
        m.app.text(&format!("agente-chave-{sufixo}-origem")),
        "Cofre · github"
    );
    m.app.assert_lint();
}

#[test]
fn bloquear_o_cofre_tira_as_chaves_e_destravar_devolve() {
    let pasta = Pasta::nova("sistema-cofre-bloqueio");
    pasta.agente_padrao(true);
    let (item, fp) = item_chave_ssh("github", true);
    pasta.semear_cofre(vec![item]);
    pasta.cofre_com_senha_mestra("abc");
    let (mut m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |_| {});

    // Bloqueado ao abrir: nada no agente.
    assert!(m.app.is_open("cofre-desbloquear"));
    assert!(!ferr.agente.tem(&fp));
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    m.app.wait(Duration::from_millis(100));
    assert!(ferr.agente.tem(&fp), "destravar põe a chave no agente");

    m.app.click("navegacao-cofre");
    m.app.click("cofre-bloquear");
    m.app.wait(Duration::from_millis(100));
    assert!(!ferr.agente.tem(&fp), "bloquear tira a chave do agente");

    m.app.click("cofre-desbloquear-botao");
    m.app.type_into("cofre-pagina-desbloquear-campo", "abc");
    m.app.click("cofre-pagina-desbloquear-primary");
    m.app.wait(Duration::from_millis(100));
    assert!(ferr.agente.tem(&fp), "destravar de novo a devolve");
}

#[test]
fn com_tirar_ao_bloquear_desligado_a_chave_fica_no_agente() {
    let pasta = Pasta::nova("sistema-cofre-fica");
    pasta.agente_padrao(true);
    let (item, fp) = item_chave_ssh("github", true);
    pasta.semear_cofre(vec![item]);
    pasta.cofre_com_senha_mestra("abc");
    let (mut m, ferr) = montar_com_ambiente(&pasta, Theme::Light, |_| {});
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    m.app.wait(Duration::from_millis(100));
    assert!(ferr.agente.tem(&fp));

    ferr.config.update(|c| c.agente.tirar_ao_bloquear = false);
    m.app.click("navegacao-cofre");
    m.app.click("cofre-bloquear");
    m.app.wait(Duration::from_millis(100));
    assert!(ferr.agente.tem(&fp), "a chave fica no agente");
}

#[test]
fn no_portable_a_pagina_so_mostra_o_endereco_e_nao_escreve_nada() {
    let pasta = pasta_com_config("sistema-portatil");
    let original = config_do(&pasta);
    let caminho = pasta.caminho().to_owned();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| a.portatil = Some(caminho));
    m.app.click("navegacao-agente");
    let texto = m.app.text("agente-sistema-portatil");
    assert!(texto.contains("agent.sock"), "{texto}");
    for id in [
        "agente-sistema-estado",
        "agente-sistema-aviso",
        "agente-sistema-ssh-config",
        "agente-sistema-shells",
        "agente-sistema-sessao",
        "agente-sistema-ver-bloco",
    ] {
        assert!(!m.app.visible(id), "{id} aparece");
    }
    m.app.assert_lint();
    m.app.wait(Duration::from_millis(50));
    assert_eq!(config_do(&pasta), original);
}
