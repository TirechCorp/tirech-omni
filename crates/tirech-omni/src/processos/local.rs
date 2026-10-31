//! Os processos deste computador: o `/proc` no Linux, o `CreateToolhelp32Snapshot` e companhia
//! no Windows (pelo `tomni-ui-win32`) e o `libproc` no macOS (pelo `tomni-ui-macos`). Nos outros
//! sistemas, um erro que a tela mostra.
//!
//! Encerrar usa o `kill` no Linux e no macOS e o `taskkill` no Windows.

#[cfg(any(target_os = "linux", windows, target_os = "macos"))]
use std::collections::HashMap;
use std::process::Command;
#[cfg(any(windows, target_os = "macos"))]
use std::time::Instant;

#[cfg(target_os = "macos")]
use super::Estado;
#[cfg(any(windows, target_os = "macos"))]
use super::Processo;
use super::{FonteDeProcessos, Foto};
use crate::t;

/// Os processos deste computador.
pub struct FonteLocal {
    /// Os nomes dos usuários pelo uid (Linux e macOS), lidos uma vez.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    usuarios: HashMap<u32, String>,
    /// O caminho do executável (Windows e macOS) e o dono (Windows), pelo pid e pelo início:
    /// não mudam enquanto o processo vive.
    #[cfg(any(windows, target_os = "macos"))]
    detalhes: HashMap<(u32, u64), (String, String)>,
    #[cfg(any(windows, target_os = "macos"))]
    inicio: Instant,
}

impl Default for FonteLocal {
    fn default() -> Self {
        Self::nova()
    }
}

impl FonteLocal {
    pub fn nova() -> Self {
        Self {
            #[cfg(target_os = "linux")]
            usuarios: super::proc_linux::usuarios(
                &std::fs::read_to_string("/etc/passwd").unwrap_or_default(),
            ),
            #[cfg(target_os = "macos")]
            usuarios: HashMap::new(),
            #[cfg(any(windows, target_os = "macos"))]
            detalhes: HashMap::new(),
            #[cfg(any(windows, target_os = "macos"))]
            inicio: Instant::now(),
        }
    }

    #[cfg(target_os = "linux")]
    fn foto(&mut self) -> Result<Foto, String> {
        super::proc_linux::deste_computador(&self.usuarios)
    }

    #[cfg(windows)]
    fn foto(&mut self) -> Result<Foto, String> {
        let tabela = tomni_ui_win32::process_table().map_err(|e| e.to_string())?;
        let mut vistos = Vec::with_capacity(tabela.len());
        let processos = tabela
            .into_iter()
            .map(|p| {
                // O "processo ocioso" (pid 0) e os protegidos não abrem: ficam só com o nome.
                let uso = tomni_ui_win32::process_usage(p.pid).ok();
                let inicio = uso.as_ref().map_or(0, |u| u.start_time);
                vistos.push((p.pid, inicio));
                let (comando, usuario) = self
                    .detalhes
                    .entry((p.pid, inicio))
                    .or_insert_with(|| {
                        (
                            tomni_ui_win32::process_path(p.pid).unwrap_or_default(),
                            tomni_ui_win32::process_user(p.pid).unwrap_or_default(),
                        )
                    })
                    .clone();
                Processo {
                    pid: p.pid,
                    pai: p.parent_pid,
                    inicio,
                    nome: p.name,
                    comando,
                    usuario,
                    estado: None,
                    threads: Some(p.threads),
                    cpu: uso.as_ref().map(|u| u.cpu),
                    memoria: uso.as_ref().map(|u| u.memory),
                    disco: uso.as_ref().map(|u| (u.read_bytes, u.written_bytes)),
                }
            })
            .collect();
        self.detalhes.retain(|chave, _| vistos.contains(chave));
        Ok(Foto {
            relogio: self.inicio.elapsed(),
            nucleos: std::thread::available_parallelism().map_or(1, usize::from),
            memoria_total: None,
            memoria_disponivel: None,
            processos,
        })
    }

    #[cfg(target_os = "macos")]
    fn foto(&mut self) -> Result<Foto, String> {
        use tomni_ui_macos::ProcessState;
        let pids = tomni_ui_macos::all_pids().map_err(|e| e.to_string())?;
        let mut vistos = Vec::with_capacity(pids.len());
        let mut processos = Vec::with_capacity(pids.len());
        for pid in pids {
            // O processo pode acabar entre a lista e a leitura.
            let Ok(info) = tomni_ui_macos::process_info(pid) else {
                continue;
            };
            vistos.push((pid, info.start_time));
            let usuario = self
                .usuarios
                .entry(info.uid)
                .or_insert_with(|| {
                    tomni_ui_macos::user_name(info.uid).unwrap_or_else(|| info.uid.to_string())
                })
                .clone();
            let comando = self
                .detalhes
                .entry((pid, info.start_time))
                .or_insert_with(|| {
                    (
                        tomni_ui_macos::process_path(pid).unwrap_or_default(),
                        String::new(),
                    )
                })
                .0
                .clone();
            processos.push(Processo {
                pid,
                pai: info.parent_pid,
                inicio: info.start_time,
                nome: info.name,
                comando,
                usuario,
                estado: Some(match info.state {
                    ProcessState::Idle => Estado::Ocioso,
                    ProcessState::Running => Estado::Executando,
                    ProcessState::Sleeping => Estado::Dormindo,
                    ProcessState::Stopped => Estado::Parado,
                    ProcessState::Zombie => Estado::Zumbi,
                    _ => Estado::Desconhecido,
                }),
                threads: info.threads,
                cpu: info.usage.as_ref().map(|u| u.cpu),
                memoria: info.usage.as_ref().map(|u| u.memory),
                disco: info.disk.map(|d| (d.read_bytes, d.written_bytes)),
            });
        }
        self.detalhes.retain(|chave, _| vistos.contains(chave));
        Ok(Foto {
            relogio: self.inicio.elapsed(),
            nucleos: std::thread::available_parallelism().map_or(1, usize::from),
            memoria_total: None,
            memoria_disponivel: None,
            processos,
        })
    }

    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    fn foto(&mut self) -> Result<Foto, String> {
        Err(t!("processos-sem-suporte"))
    }
}

impl FonteDeProcessos for FonteLocal {
    fn ler(&mut self) -> Result<Foto, String> {
        self.foto()
    }

    fn encerrar(&mut self, pid: u32, forcar: bool) -> Result<(), String> {
        let mut comando = if cfg!(windows) {
            let mut c = Command::new("taskkill");
            c.args(["/PID", &pid.to_string()]);
            if forcar {
                c.arg("/F");
            }
            c
        } else {
            let mut c = Command::new("kill");
            c.args(["-s", if forcar { "KILL" } else { "TERM" }, &pid.to_string()]);
            c
        };
        crate::sistema::sem_janela(&mut comando);
        let saida = comando.output().map_err(|e| e.to_string())?;
        if saida.status.success() {
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

#[cfg(test)]
mod testes {
    use super::*;

    #[cfg(any(target_os = "linux", windows, target_os = "macos"))]
    #[test]
    fn a_foto_tem_o_proprio_processo() {
        let foto = FonteLocal::nova().ler().expect("a foto");
        let eu = foto
            .processos
            .iter()
            .find(|p| p.pid == std::process::id())
            .expect("o próprio processo");
        assert!(eu.cpu.is_some());
        assert!(eu.memoria.unwrap_or(0) > 0);
        assert!(!eu.nome.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn encerra_um_processo_filho() {
        let mut filho = Command::new("sleep").arg("30").spawn().expect("o sleep");
        FonteLocal::nova()
            .encerrar(filho.id(), false)
            .expect("encerrar");
        let status = filho.wait().expect("o fim");
        assert!(!status.success());
        // Um pid que não existe mais dá erro.
        assert!(FonteLocal::nova().encerrar(filho.id(), true).is_err());
    }
}
