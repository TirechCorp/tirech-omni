//! Os binários `tirech-omni-keygen`, `tirech-omni-agent` e `tirech-omni-add`, rodados de verdade.

mod comum;

#[cfg(unix)]
use std::time::{Duration, Instant};

use comum::{Casa, gerar, rodar, texto};

const KEYGEN: &str = env!("CARGO_BIN_EXE_tirech-omni-keygen");
#[cfg(unix)]
const AGENT: &str = env!("CARGO_BIN_EXE_tirech-omni-agent");
const ADD: &str = env!("CARGO_BIN_EXE_tirech-omni-add");

#[test]
fn keygen_cria_o_par_de_chaves() {
    let casa = Casa::nova("keygen-par");
    let arquivo = casa.ssh().join("x");
    gerar(&casa, &arquivo, "ana@teste");
    assert!(arquivo.exists());
    let publica = std::fs::read_to_string(casa.ssh().join("x.pub")).unwrap();
    assert!(publica.starts_with("ssh-ed25519 "), "{publica}");
    assert!(publica.trim_end().ends_with("ana@teste"), "{publica}");
}

#[test]
fn keygen_l_imprime_o_tamanho_a_impressao_o_comentario_e_o_tipo() {
    let casa = Casa::nova("keygen-l");
    let arquivo = casa.ssh().join("x");
    gerar(&casa, &arquivo, "ana@teste");
    let mut comando = casa.comando(KEYGEN);
    comando.arg("-l").arg("-f").arg(&arquivo);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let linha = texto(&saida.stdout);
    assert!(
        linha.starts_with("256 SHA256:") && linha.trim_end().ends_with("ana@teste (ED25519)"),
        "{linha}"
    );
}

#[test]
fn keygen_y_imprime_a_chave_publica_igual_a_do_arquivo() {
    let casa = Casa::nova("keygen-y");
    let arquivo = casa.ssh().join("x");
    gerar(&casa, &arquivo, "ana@teste");
    let mut comando = casa.comando(KEYGEN);
    comando.arg("-y").arg("-f").arg(&arquivo);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let gravada = std::fs::read_to_string(casa.ssh().join("x.pub")).unwrap();
    assert_eq!(texto(&saida.stdout).trim(), gravada.trim());
}

#[test]
fn keygen_p_troca_a_passphrase() {
    let casa = Casa::nova("keygen-p");
    let arquivo = casa.ssh().join("x");
    let mut comando = casa.comando(KEYGEN);
    comando
        .args(["-t", "ed25519", "-q", "-N", "velha", "-f"])
        .arg(&arquivo);
    assert!(rodar(comando).status.success());

    let mut comando = casa.comando(KEYGEN);
    comando
        .args(["-p", "-P", "velha", "-N", "nova", "-q", "-f"])
        .arg(&arquivo);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));

    // A antiga não abre mais; a nova abre (a pública, em claro, não prova nada: usa-se o -p).
    let mut comando = casa.comando(KEYGEN);
    comando
        .args(["-p", "-P", "velha", "-N", "outra", "-q", "-f"])
        .arg(&arquivo);
    assert!(!rodar(comando).status.success());
    let mut comando = casa.comando(KEYGEN);
    comando
        .args(["-p", "-P", "nova", "-N", "", "-q", "-f"])
        .arg(&arquivo);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
}

#[test]
fn keygen_recusa_rsa_pequeno_e_dsa() {
    let casa = Casa::nova("keygen-fraca");
    for argumentos in [["-t", "rsa", "-b", "1024"], ["-t", "dsa", "-b", "1024"]] {
        let mut comando = casa.comando(KEYGEN);
        comando
            .args(argumentos)
            .args(["-N", "", "-q", "-f"])
            .arg(casa.ssh().join("fraca"));
        let saida = rodar(comando);
        assert!(!saida.status.success(), "{argumentos:?}");
        assert!(!casa.ssh().join("fraca").exists(), "{argumentos:?}");
    }
}

#[test]
fn keygen_com_opcao_desconhecida_sai_com_2() {
    let casa = Casa::nova("keygen-uso");
    let mut comando = casa.comando(KEYGEN);
    comando.arg("-Z");
    let saida = rodar(comando);
    assert_eq!(saida.status.code(), Some(2));
    assert!(texto(&saida.stderr).contains("tirech-omni-keygen --help"));
}

/// Espera o socket do agente aparecer.
#[cfg(unix)]
fn esperar_socket(socket: &std::path::Path) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(Instant::now() < limite, "o agente não abriu {socket:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
#[test]
fn o_agente_em_segundo_plano_atende_o_add_e_o_k_encerra() {
    let casa = Casa::nova("agente");
    let socket = casa.pasta.join("agent.sock");

    // `-s` sobe em segundo plano e imprime as duas linhas do ambiente.
    let mut comando = casa.comando(AGENT);
    comando.arg("-s").arg("-a").arg(&socket);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let ambiente = texto(&saida.stdout);
    let linhas: Vec<&str> = ambiente.lines().collect();
    assert_eq!(linhas.len(), 2, "{ambiente}");
    assert!(linhas[0].starts_with("SSH_AUTH_SOCK="), "{ambiente}");
    assert!(linhas[1].starts_with("SSH_AGENT_PID="), "{ambiente}");
    let pid = linhas[1]
        .trim_start_matches("SSH_AGENT_PID=")
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    esperar_socket(&socket);

    // Sem chaves, `-l` sai com 1.
    let mut comando = casa.comando(ADD);
    comando.arg("-l").env("SSH_AUTH_SOCK", &socket);
    assert_eq!(rodar(comando).status.code(), Some(1));

    // Adiciona uma chave e lista.
    let chave = casa.ssh().join("id_ed25519");
    gerar(&casa, &chave, "ana@agente");
    let mut comando = casa.comando(ADD);
    comando.arg(&chave).env("SSH_AUTH_SOCK", &socket);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let mut comando = casa.comando(ADD);
    comando.arg("-l").env("SSH_AUTH_SOCK", &socket);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let lista = texto(&saida.stdout);
    assert!(
        lista.starts_with("256 SHA256:") && lista.contains("ana@agente (ED25519)"),
        "{lista}"
    );

    // `-d` tira a chave e `-D` esvazia.
    let mut comando = casa.comando(ADD);
    comando.arg("-D").env("SSH_AUTH_SOCK", &socket);
    assert!(rodar(comando).status.success());
    let mut comando = casa.comando(ADD);
    comando.arg("-l").env("SSH_AUTH_SOCK", &socket);
    assert_eq!(rodar(comando).status.code(), Some(1));

    // `-k` encerra o agente do `SSH_AGENT_PID`.
    let mut comando = casa.comando(AGENT);
    comando.arg("-k").env("SSH_AGENT_PID", &pid);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let limite = Instant::now() + Duration::from_secs(10);
    loop {
        let mut comando = casa.comando(ADD);
        comando.arg("-l").env("SSH_AUTH_SOCK", &socket);
        if rodar(comando).status.code() == Some(2) {
            break;
        }
        assert!(Instant::now() < limite, "o agente não encerrou");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(unix)]
#[test]
fn o_agente_em_primeiro_plano_atende_o_add() {
    let casa = Casa::nova("agente-d");
    let socket = casa.pasta.join("agent.sock");
    let mut agente = casa
        .comando(AGENT)
        .args(["-D", "-a"])
        .arg(&socket)
        .spawn()
        .unwrap();
    esperar_socket(&socket);

    let chave = casa.ssh().join("id_ed25519");
    gerar(&casa, &chave, "ana@d");
    let mut comando = casa.comando(ADD);
    comando.arg(&chave).env("SSH_AUTH_SOCK", &socket);
    let adicionou = rodar(comando);
    let mut comando = casa.comando(ADD);
    comando.arg("-L").env("SSH_AUTH_SOCK", &socket);
    let listou = rodar(comando);

    agente.kill().unwrap();
    let _ = agente.wait();
    assert!(adicionou.status.success(), "{}", texto(&adicionou.stderr));
    let publica = std::fs::read_to_string(casa.ssh().join("id_ed25519.pub")).unwrap();
    let chave_publica: Vec<&str> = publica.split_whitespace().take(2).collect();
    assert!(
        texto(&listou.stdout).starts_with(&chave_publica.join(" ")),
        "{}",
        texto(&listou.stdout)
    );
}

#[test]
fn o_add_sem_agente_sai_com_2() {
    let casa = Casa::nova("add-sem-agente");
    let mut comando = casa.comando(ADD);
    comando.arg("-l");
    let saida = rodar(comando);
    assert_eq!(saida.status.code(), Some(2));
    assert!(texto(&saida.stderr).contains("não foi possível conectar ao agente"));
}
