//! Os processos do servidor de uma conexão SSH: o [`SCRIPT`](super::proc_linux::SCRIPT) roda por
//! um canal `exec` a cada leitura, e encerrar é um `kill` pelo mesmo caminho. Só servidores
//! Linux (com o `/proc`).

use std::time::Duration;

use super::{FonteDeProcessos, Foto, proc_linux};
use crate::portas::descobrir::Canais;
use crate::t;

/// Quanto uma leitura ou um `kill` pode levar.
const TEMPO: Duration = Duration::from_secs(15);

/// Os processos do outro lado de `canais`.
pub struct FonteRemota {
    canais: Box<dyn Canais>,
}

impl FonteRemota {
    pub fn nova(canais: Box<dyn Canais>) -> Self {
        Self { canais }
    }
}

impl FonteDeProcessos for FonteRemota {
    fn ler(&mut self) -> Result<Foto, String> {
        let saida = self
            .canais
            .exec("sh -s", proc_linux::SCRIPT.as_bytes(), TEMPO)
            .map_err(|e| e.to_string())?;
        proc_linux::da_saida_do_script(&String::from_utf8_lossy(&saida.stdout))
    }

    fn encerrar(&mut self, pid: u32, forcar: bool) -> Result<(), String> {
        let sinal = if forcar { "KILL" } else { "TERM" };
        let saida = self
            .canais
            .exec(&format!("kill -s {sinal} {pid}"), &[], TEMPO)
            .map_err(|e| e.to_string())?;
        if saida.status == Some(0) {
            Ok(())
        } else {
            let erro = String::from_utf8_lossy(&saida.stderr).trim().to_owned();
            Err(if erro.is_empty() {
                t!("processos-nao-encerrou", pid = pid)
            } else {
                erro
            })
        }
    }
}
