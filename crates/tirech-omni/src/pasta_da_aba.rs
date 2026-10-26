//! A pasta em que uma aba está agora (M64): de onde "Duplicar aba" tira o lugar da cópia.
//!
//! A ordem das fontes (o plano, seção 3.1):
//!
//! | Tipo | 1ª | 2ª |
//! |---|---|---|
//! | Shell local, no Linux | o `/proc` do processo em primeiro plano | o OSC 7 |
//! | Shell local, no macOS e no Windows | o OSC 7 | |
//! | SSH | a sonda no servidor ([`SshChannelOpener::shell_dir`]) | o OSC 7 |
//! | SSH no tmux | o `tmux display-message` da sessão da aba | o OSC 7 |
//! | SSH no screen | o `/proc` do shell da sessão, no servidor (só com uma janela) | o OSC 7 |
//! | Telnet, WSL | o OSC 7 | |
//!
//! O bash do WSL não manda o OSC 7 sozinho: o `wsl.exe` do terminal local recebe o
//! [`PROMPT_COMMAND_DO_OSC7`] pelo `WSLENV` ([`ambiente_do_osc7_no_wsl`]).
//!
//! A sonda e o `/proc` vêm antes do OSC 7 porque olham o processo de verdade: o OSC 7 pode ter
//! vindo de um `ssh` aninhado para outra máquina, e o caminho dele não existe aqui. Uma pasta que
//! não começa com `/` é descartada (no shell local do Windows, `C:\...` também vale).
//!
//! Numa aba que roda no tmux ou no screen, a sonda do shell acharia o cliente do programa, e
//! não o shell da sessão: quem diz a pasta é o comando de [`crate::tmux::comando_pasta`].

use std::io;
use std::time::Duration;

use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::SshChannelOpener;

use crate::abrir::{dividir_argumentos, juntar_argumentos};
use crate::modelo::SessaoDaAba;

/// Quanto a sonda do servidor pode demorar.
pub const LIMITE_DA_SONDA: Duration = Duration::from_secs(2);

/// O que se sabe da pasta de uma aba na hora de duplicar.
#[derive(Clone)]
pub enum PastaDaAba {
    /// Já se sabe (ou não há como saber: `None`).
    Pronta(Option<String>),
    /// A aba é SSH: a pasta vem de uma sonda no servidor, que bloqueia e roda numa thread. Se a
    /// sonda não achar nada, vale o OSC 7 que a aba tiver naquela hora. Com a sessão do tmux
    /// ou do screen, a sonda pergunta a ela.
    Sondar(SshChannelOpener, Option<SessaoDaAba>),
}

impl PastaDaAba {
    /// A mesma pasta, numa aba que roda na `sessao` do tmux ou do screen (`None`: fora delas).
    pub fn no_tmux(self, sessao: Option<SessaoDaAba>) -> Self {
        match self {
            Self::Sondar(canais, _) => Self::Sondar(canais, sessao),
            pronta => pronta,
        }
    }

    /// A pasta, rodando a sonda se precisa: bloqueia até [`LIMITE_DA_SONDA`], por isso roda
    /// numa thread. Qualquer erro da sonda é "não achei".
    pub fn sondada(&self) -> Option<String> {
        match self {
            Self::Pronta(pasta) => pasta.clone(),
            Self::Sondar(canais, None) => canais
                .shell_dir(LIMITE_DA_SONDA)
                .ok()
                .flatten()
                .as_deref()
                .and_then(pasta_valida),
            Self::Sondar(canais, Some(sessao)) => {
                let saida = canais
                    .exec(
                        &crate::tmux::comando_pasta(sessao),
                        &mut io::empty(),
                        &mut |_| {},
                        LIMITE_DA_SONDA,
                    )
                    .ok()
                    .filter(|s| s.status == Some(0))?;
                pasta_valida(&String::from_utf8_lossy(&saida.stdout))
            }
        }
    }
}

impl std::fmt::Debug for PastaDaAba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pronta(pasta) => f.debug_tuple("Pronta").field(pasta).finish(),
            Self::Sondar(_, tmux) => f.debug_tuple("Sondar").field(&"..").field(tmux).finish(),
        }
    }
}

/// A pasta em que `sessao` está agora, pela ordem da tabela do módulo.
pub fn pasta_atual(sessao: Session) -> PastaDaAba {
    if let Some(pid) = sessao.process_id() {
        let do_processo = pasta_do_processo(pid);
        return PastaDaAba::Pronta(do_processo.or_else(|| pasta_do_osc7(sessao)));
    }
    if let Some(canais) = sessao.ssh_channels() {
        return PastaDaAba::Sondar(canais, None);
    }
    PastaDaAba::Pronta(pasta_do_osc7(sessao))
}

/// A pasta que o programa informou pelo OSC 7, se for um caminho que o `cd` entende.
pub fn pasta_do_osc7(sessao: Session) -> Option<String> {
    sessao.cwd().as_deref().and_then(pasta_valida)
}

/// `pasta` sem os espaços das pontas, se for um caminho absoluto: começa com `/` (no Windows,
/// também `C:\...` e `/C:/...`, que o OSC 7 do Windows manda). O resto é descartado.
pub fn pasta_valida(pasta: &str) -> Option<String> {
    let pasta = pasta.trim();
    if pasta.is_empty() || pasta.contains(['\n', '\r', '\0']) {
        return None;
    }
    #[cfg(windows)]
    {
        let sem_barra = pasta.strip_prefix('/').unwrap_or(pasta);
        let b = sem_barra.as_bytes();
        if b.len() >= 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && matches!(b[2], b'\\' | b'/')
        {
            return Some(sem_barra.to_owned());
        }
    }
    pasta.starts_with('/').then(|| pasta.to_owned())
}

/// A pasta do processo em primeiro plano do terminal do filho `pid`, lida do `/proc`: o `tpgid`
/// de `/proc/<pid>/stat` e o `cwd` dele, ou o `cwd` do próprio filho se o primeiro não puder ser
/// lido. Só no Linux; nos outros sistemas é `None`, e vale o OSC 7.
pub fn pasta_do_processo(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let primeiro_plano = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|stat| grupo_em_primeiro_plano(&stat));
        let ler = |dono: u32| {
            let caminho = std::fs::read_link(format!("/proc/{dono}/cwd")).ok()?;
            // Uma pasta apagada aparece como "… (deleted)": não existe, então não serve.
            caminho
                .is_dir()
                .then(|| caminho.to_str().and_then(pasta_valida))
                .flatten()
        };
        primeiro_plano.and_then(ler).or_else(|| ler(pid))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

/// O `tpgid` (o grupo de processos em primeiro plano do terminal) de uma linha do
/// `/proc/<pid>/stat`. O nome do programa, entre parênteses, pode ter espaços e parênteses;
/// por isso a leitura começa depois do **último** `) `. Dali: estado, ppid, pgrp, sessão,
/// `tty_nr` e `tpgid`. `-1` (sem terminal) e `0` são `None`.
pub fn grupo_em_primeiro_plano(stat: &str) -> Option<u32> {
    let depois = &stat[stat.rfind(") ")? + 2..];
    let tpgid: i64 = depois.split_whitespace().nth(5)?.parse().ok()?;
    u32::try_from(tpgid).ok().filter(|&g| g > 0)
}

/// Os argumentos de um `wsl.exe` com a pasta `pasta` no `--cd`: o `--cd` que já havia (em duas
/// palavras ou como `--cd=...`) sai, e `--cd <pasta>` entra no fim. O resultado volta a ser uma
/// linha que [`dividir_argumentos`] lê igual.
pub fn argumentos_do_wsl_na_pasta(argumentos: &str, pasta: &str) -> String {
    let mut novos = Vec::new();
    let mut antigos = dividir_argumentos(argumentos).into_iter();
    while let Some(argumento) = antigos.next() {
        if argumento == "--cd" {
            antigos.next();
        } else if !argumento.starts_with("--cd=") {
            novos.push(argumento);
        }
    }
    novos.push("--cd".to_owned());
    novos.push(pasta.to_owned());
    juntar_argumentos(&novos)
}

/// O `PROMPT_COMMAND` que faz o bash do WSL mandar a pasta pelo OSC 7 a cada prompt. O bash das
/// distribuições não manda, e sem ele "Duplicar aba" abriria a cópia em `~`. O `%` da pasta vira
/// `%25`, porque o OSC 7 é lido com os `%XX` decodificados.
pub const PROMPT_COMMAND_DO_OSC7: &str =
    r#"printf "\033]7;file://%s%s\033\\" "$HOSTNAME" "${PWD//\%/%25}""#;

/// As variáveis do processo do `wsl.exe` que levam o [`PROMPT_COMMAND_DO_OSC7`] para dentro da
/// distribuição: o próprio `PROMPT_COMMAND` e o `WSLENV` (o `wslenv` do Windows, se houver, com
/// `PROMPT_COMMAND/u` no fim). Nenhum arquivo da distribuição é tocado; um `.bashrc` que define o
/// seu `PROMPT_COMMAND`, o zsh e o fish não mandam a pasta.
pub fn ambiente_do_osc7_no_wsl(wslenv: Option<&str>) -> [(&'static str, String); 2] {
    let mut nomes: Vec<&str> = wslenv
        .unwrap_or("")
        .split(':')
        .filter(|nome| !nome.is_empty() && nome.split('/').next() != Some("PROMPT_COMMAND"))
        .collect();
    nomes.push("PROMPT_COMMAND/u");
    [
        ("PROMPT_COMMAND", PROMPT_COMMAND_DO_OSC7.to_owned()),
        ("WSLENV", nomes.join(":")),
    ]
}

/// Se o programa do terminal local é o `wsl.exe` (com ou sem `.exe`, em qualquer caixa).
pub fn eh_wsl(programa: &str) -> bool {
    let nome = programa.trim().rsplit(['/', '\\']).next().unwrap_or("");
    let nome = nome.to_lowercase();
    nome == "wsl" || nome == "wsl.exe"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_pasta_absoluta_vale() {
        assert_eq!(pasta_valida("/srv/app"), Some("/srv/app".to_owned()));
        assert_eq!(pasta_valida("  /srv/app  "), Some("/srv/app".to_owned()));
        assert_eq!(pasta_valida("srv/app"), None);
        assert_eq!(pasta_valida("~/app"), None);
        assert_eq!(pasta_valida(""), None);
        assert_eq!(pasta_valida("   "), None);
        assert_eq!(pasta_valida("/srv\napp"), None);
        assert_eq!(pasta_valida("/srv\rapp"), None);
    }

    #[test]
    fn o_tpgid_vem_depois_do_ultimo_parenteses() {
        let stat = "1234 (bash) S 1000 1234 1234 34816 5678 4194304 1 0";
        assert_eq!(grupo_em_primeiro_plano(stat), Some(5678));
        // O nome do programa pode ter espaços e parênteses.
        let estranho = "1234 (a) b (c)) S 1000 1234 1234 34816 9001 4194304 1";
        assert_eq!(grupo_em_primeiro_plano(estranho), Some(9001));
        // Sem terminal: -1.
        let sem = "1234 (sh) S 1000 1234 1234 0 -1 4194304 1";
        assert_eq!(grupo_em_primeiro_plano(sem), None);
        assert_eq!(grupo_em_primeiro_plano("lixo"), None);
        assert_eq!(grupo_em_primeiro_plano("1 (sh) S 1"), None);
    }

    #[test]
    fn o_wsl_recebe_a_pasta_no_cd() {
        assert_eq!(
            dividir_argumentos(&argumentos_do_wsl_na_pasta(
                "-d Ubuntu --cd ~",
                "/home/ana/meu app"
            )),
            ["-d", "Ubuntu", "--cd", "/home/ana/meu app"]
        );
        assert_eq!(
            dividir_argumentos(&argumentos_do_wsl_na_pasta("--cd=/tmp -d Debian", "/var")),
            ["-d", "Debian", "--cd", "/var"]
        );
        assert_eq!(
            dividir_argumentos(&argumentos_do_wsl_na_pasta("", "/o'brien")),
            ["--cd", "/o'brien"]
        );
    }

    #[test]
    fn o_wsl_recebe_o_prompt_command_pelo_wslenv() {
        let [prompt, wslenv] = ambiente_do_osc7_no_wsl(None);
        assert_eq!(
            prompt,
            ("PROMPT_COMMAND", PROMPT_COMMAND_DO_OSC7.to_owned())
        );
        assert_eq!(wslenv, ("WSLENV", "PROMPT_COMMAND/u".to_owned()));
        // O WSLENV de quem usa fica, e um PROMPT_COMMAND que já estava nele sai.
        let [_, wslenv] = ambiente_do_osc7_no_wsl(Some("USERPROFILE/p:PROMPT_COMMAND/w::GOPATH/l"));
        assert_eq!(wslenv.1, "USERPROFILE/p:GOPATH/l:PROMPT_COMMAND/u");
        let [_, wslenv] = ambiente_do_osc7_no_wsl(Some("PROMPT_COMMAND"));
        assert_eq!(wslenv.1, "PROMPT_COMMAND/u");
    }

    #[cfg(unix)]
    #[test]
    fn o_prompt_command_manda_a_pasta_pelo_osc7() {
        let pasta = std::env::temp_dir().join(format!("tirech-osc7 50%41-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let saida = std::process::Command::new("bash")
            .args(["-c", r#"eval "$PROMPT_COMMAND""#])
            .env("PROMPT_COMMAND", PROMPT_COMMAND_DO_OSC7)
            .env("HOSTNAME", "maquina")
            .current_dir(&pasta)
            .output()
            .unwrap();
        let _ = std::fs::remove_dir(&pasta);
        let esperado = format!(
            "\x1b]7;file://maquina{}\x1b\\",
            pasta.to_str().unwrap().replace('%', "%25")
        );
        assert_eq!(String::from_utf8_lossy(&saida.stdout), esperado);
    }

    #[test]
    fn reconhece_o_wsl() {
        assert!(eh_wsl("wsl.exe"));
        assert!(eh_wsl("WSL"));
        assert!(eh_wsl(r"C:\Windows\System32\wsl.exe"));
        assert!(!eh_wsl("bash"));
        assert!(!eh_wsl("/usr/bin/wslview"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn le_a_pasta_do_processo_em_primeiro_plano() {
        use tomni_ui::terminal::{Process, Transport};
        let pasta = std::env::temp_dir().join(format!("tirech-pasta-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let pasta = pasta.canonicalize().unwrap();
        let processo = Process::builder("sh")
            .args(["-c", "cd \"$0\" && exec sleep 30"])
            .arg(pasta.to_str().unwrap())
            .spawn();
        let pid = Transport::process_id(&processo).expect("tem pid");
        let esperado = Some(pasta.to_str().unwrap().to_owned());
        let inicio = std::time::Instant::now();
        let mut lido = None;
        while inicio.elapsed() < Duration::from_secs(5) {
            lido = pasta_do_processo(pid);
            if lido == esperado {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(lido, esperado);
        drop(processo);
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
