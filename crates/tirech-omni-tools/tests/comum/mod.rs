//! Auxiliares dos testes dos programas: uma pasta pessoal temporária e a execução dos binários.
//! Cada arquivo de teste usa uma parte deste módulo.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Uma pasta pessoal só do teste (`HOME` e `USERPROFILE` apontam para ela).
pub struct Casa {
    pub pasta: PathBuf,
}

impl Casa {
    pub fn nova(nome: &str) -> Self {
        let pasta =
            std::env::temp_dir().join(format!("tirech-omni-tools-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(pasta.join(".ssh")).unwrap();
        Self { pasta }
    }

    pub fn ssh(&self) -> PathBuf {
        self.pasta.join(".ssh")
    }

    /// Prepara o comando: `HOME` temporário, sem agente herdado e sem terminal.
    pub fn comando(&self, programa: &str) -> Command {
        let mut comando = Command::new(programa);
        comando
            .env("HOME", &self.pasta)
            .env("USERPROFILE", &self.pasta)
            .env_remove("SSH_AUTH_SOCK")
            .env_remove("SSH_AGENT_PID")
            .current_dir(&self.pasta)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        comando
    }
}

impl Drop for Casa {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.pasta);
    }
}

pub fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn rodar(mut comando: Command) -> Output {
    comando.output().unwrap()
}

/// Gera uma chave ed25519 sem passphrase em `arquivo` com o `tirech-omni-keygen`.
pub fn gerar(casa: &Casa, arquivo: &Path, comentario: &str) {
    let mut comando = casa.comando(env!("CARGO_BIN_EXE_tirech-omni-keygen"));
    comando
        .args(["-t", "ed25519", "-N", "", "-q", "-C", comentario, "-f"])
        .arg(arquivo);
    let saida = rodar(comando);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
}
