//! `tirech-omni-agent`: o agente de chaves, no mesmo protocolo do `ssh-agent`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use tomni_ui_terminal::ssh::agent::{Agent, AgentConnection};

use crate::getopt::getopt;
use crate::{tempo, usage_error};

const PROGRAM: &str = "tirech-omni-agent";

/// Quanto o processo que fica em segundo plano tem para subir o socket.
const START_TIMEOUT: Duration = Duration::from_secs(5);

const HELP: &str = "\
tirech-omni-agent: o agente de chaves do Tomni UI (fala o protocolo do ssh-agent)

Uso:
  tirech-omni-agent [-s | -c] [-a socket] [-t vida]      sobe o agente em segundo plano
  tirech-omni-agent -D [-s | -c] [-a socket] [-t vida]   fica em primeiro plano
  tirech-omni-agent -d [-s | -c] [-a socket] [-t vida]   primeiro plano, com depuração no stderr
  tirech-omni-agent -k                                   encerra o agente de SSH_AGENT_PID

Imprime SSH_AUTH_SOCK e SSH_AGENT_PID na sintaxe do sh (-s, o padrão) ou do csh (-c); use
`eval $(tirech-omni-agent)`. -t é a validade das chaves adicionadas sem uma própria (30, 10m,
1h, 1d). Sem -a, o socket fica em $XDG_RUNTIME_DIR/tirech-omni/ (no Windows, um pipe nomeado).
";

/// A sintaxe das linhas de ambiente.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shell {
    Sh,
    Csh,
}

/// Roda o programa com `args` (sem o nome dele) e devolve o código de saída.
pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let parsed = match getopt(args, "a:t:Ddsck") {
        Ok(parsed) => parsed,
        Err(message) => return usage_error(PROGRAM, &message),
    };
    if let Some(extra) = parsed.rest.first() {
        return usage_error(PROGRAM, &format!("argumento inesperado: {extra}"));
    }
    let shell = if parsed.has('c') && !parsed.has('s') {
        Shell::Csh
    } else if parsed.has('s') || parsed.has('c') {
        Shell::Sh
    } else if std::env::var("SHELL").is_ok_and(|s| s.ends_with("csh")) {
        Shell::Csh
    } else {
        Shell::Sh
    };
    let lifetime = match parsed.value('t').map(tempo::parse).transpose() {
        Ok(lifetime) => lifetime,
        Err(message) => return usage_error(PROGRAM, &message),
    };
    let socket = parsed.value('a').map(PathBuf::from);

    let result = if parsed.has('k') {
        kill(shell)
    } else if parsed.has('D') || parsed.has('d') {
        foreground(socket, lifetime, shell, parsed.has('d'))
    } else {
        background(args)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{PROGRAM}: {message}");
            ExitCode::FAILURE
        }
    }
}

/// As linhas que o `eval` do shell lê.
fn environment(shell: Shell, socket: &Path, pid: u32) -> String {
    let socket = quote(&socket.display().to_string());
    match shell {
        Shell::Sh => format!(
            "SSH_AUTH_SOCK={socket}; export SSH_AUTH_SOCK;\nSSH_AGENT_PID={pid}; export SSH_AGENT_PID;\n"
        ),
        Shell::Csh => format!("setenv SSH_AUTH_SOCK {socket};\nsetenv SSH_AGENT_PID {pid};\n"),
    }
}

/// Põe entre aspas simples o que o shell leria errado (espaços, `\` do pipe do Windows…).
fn quote(text: &str) -> String {
    let plain = text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-:+@%,=".contains(c));
    if plain {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

/// `-D` e `-d`: o agente neste processo, para sempre.
fn foreground(
    socket: Option<PathBuf>,
    lifetime: Option<Duration>,
    shell: Shell,
    debug: bool,
) -> Result<(), String> {
    let agent = Agent::start(socket).map_err(|e| e.to_string())?;
    agent.default_lifetime(lifetime);
    if debug {
        eprintln!("{PROGRAM}: escutando em {}", agent.path().display());
        let watched = agent.clone();
        agent.on_change(move || {
            eprintln!(
                "{PROGRAM}: {} chave(s){}",
                watched.list().len(),
                if watched.is_locked() { ", travado" } else { "" }
            );
        });
    }
    let mut out = std::io::stdout().lock();
    // O processo que nos lançou lê estas linhas; se ele já saiu, o agente segue assim mesmo.
    let _ = out.write_all(environment(shell, agent.path(), std::process::id()).as_bytes());
    let _ = out.flush();
    drop(out);
    loop {
        std::thread::park();
    }
}

/// Sem `-D`: um `fork` seria `unsafe`, então o programa lança a si mesmo com `-D`, espera o
/// agente dizer onde está e imprime o que o processo lançado imprimiu.
fn background(args: &[String]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = Command::new(exe);
    command.args(args).arg("-D");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    detach(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("sem saída do agente")?;

    // Duas linhas (sh e csh têm duas): a leitura roda numa thread para o limite de tempo valer.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut lines = BufReader::new(stdout).lines();
        let first = lines.next().and_then(Result::ok);
        let second = lines.next().and_then(Result::ok);
        let _ = sender.send((first, second));
    });
    match receiver.recv_timeout(START_TIMEOUT) {
        Ok((Some(first), Some(second))) => {
            println!("{first}\n{second}");
            Ok(())
        }
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            Err("o agente não subiu".to_owned())
        }
    }
}

/// Solta o agente do terminal de quem o lançou.
fn detach(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
}

/// `-k`: encerra o agente de `SSH_AGENT_PID`.
fn kill(shell: Shell) -> Result<(), String> {
    let pid = std::env::var("SSH_AGENT_PID")
        .ok()
        .filter(|pid| !pid.is_empty())
        .ok_or("a variável SSH_AGENT_PID não está definida")?;
    if pid.parse::<u32>().is_err() {
        return Err(format!("SSH_AGENT_PID inválido: {pid}"));
    }
    let status = if cfg!(windows) {
        Command::new("taskkill").args(["/PID", &pid, "/F"]).status()
    } else {
        Command::new("kill").arg(&pid).status()
    }
    .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("não deu para encerrar o processo {pid}"));
    }
    // Um processo morto por sinal não apaga o socket: quem ficou sem resposta é limpo aqui.
    if let Some(socket) = std::env::var_os("SSH_AUTH_SOCK").map(PathBuf::from) {
        for _ in 0..20 {
            if AgentConnection::connect(&socket).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if AgentConnection::connect(&socket).is_err() {
            let _ = std::fs::remove_file(&socket);
        }
    }
    match shell {
        Shell::Sh => println!(
            "unset SSH_AUTH_SOCK;\nunset SSH_AGENT_PID;\necho O agente de pid {pid} foi encerrado;"
        ),
        Shell::Csh => println!(
            "unsetenv SSH_AUTH_SOCK;\nunsetenv SSH_AGENT_PID;\necho O agente de pid {pid} foi encerrado;"
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_linhas_do_ambiente() {
        let socket = Path::new("/run/user/1000/tirech-omni/agent.1");
        assert_eq!(
            environment(Shell::Sh, socket, 42),
            "SSH_AUTH_SOCK=/run/user/1000/tirech-omni/agent.1; export SSH_AUTH_SOCK;\n\
SSH_AGENT_PID=42; export SSH_AGENT_PID;\n"
        );
        assert_eq!(
            environment(Shell::Csh, socket, 42),
            "setenv SSH_AUTH_SOCK /run/user/1000/tirech-omni/agent.1;\nsetenv SSH_AGENT_PID 42;\n"
        );
    }

    #[test]
    fn caminhos_com_espaco_ou_barra_invertida_levam_aspas() {
        assert_eq!(quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(quote(r"\\.\pipe\fluent"), r"'\\.\pipe\fluent'");
        assert_eq!(quote("/tmp/o'k"), "'/tmp/o'\\''k'");
    }
}
