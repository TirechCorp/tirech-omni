//! Os programas de linha de comando do M29: `tirech-omni-keygen`, `tirech-omni-agent`,
//! `tirech-omni-add` e `tirech-omni-copy-id`. São um subconjunto do OpenSSH feito sobre o
//! `tomni_ui_terminal::ssh` (chaves, agente e `copy_id`), com as flags de mesmo nome e as
//! mensagens em português. Não têm tela.
//!
//! Cada programa é uma função `run` que recebe os argumentos (sem o nome do programa) e devolve
//! o código de saída: 0 é sucesso, 1 é erro e 2 é uso errado (no `tirech-omni-add`, 2 também é
//! "não conectou ao agente", como no OpenSSH).

pub mod add;
pub mod agent;
pub mod copy_id;
pub mod getopt;
pub mod keygen;
pub mod prompt;
pub mod tempo;

use std::path::PathBuf;

/// A pasta `~/.ssh`.
pub fn ssh_dir() -> PathBuf {
    std::env::home_dir().unwrap_or_default().join(".ssh")
}

/// Os arquivos de chave privada que os programas procuram em `~/.ssh` quando não se diz qual.
pub const DEFAULT_KEYS: [&str; 3] = ["id_ed25519", "id_ecdsa", "id_rsa"];

/// Mostra `~/.ssh/…` no lugar da pasta pessoal, como o OpenSSH faz nas mensagens.
pub fn display_path(path: &std::path::Path) -> String {
    if let Some(home) = std::env::home_dir()
        && !home.as_os_str().is_empty()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// O texto de erro de uso, com a linha de ajuda.
pub(crate) fn usage_error(program: &str, message: &str) -> std::process::ExitCode {
    eprintln!("{program}: {message}");
    eprintln!("Veja `{program} --help`.");
    std::process::ExitCode::from(2)
}
