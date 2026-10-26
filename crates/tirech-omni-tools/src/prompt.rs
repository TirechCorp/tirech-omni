//! Perguntas no terminal. A passphrase e a senha são lidas sem eco: sem crate novo, o eco só
//! se desliga com `unsafe` (`termios`), então se chama o `stty` (Unix). No Windows a digitação
//! aparece na tela.

use std::io::{self, BufRead, IsTerminal, Write};
use std::process::{Command, Stdio};

/// Escreve `question` no stderr (o stdout fica livre para a saída do programa) e lê uma linha.
pub fn line(question: &str) -> io::Result<String> {
    eprint!("{question}");
    io::stderr().flush()?;
    read_line()
}

/// Como [`line()`], sem mostrar o que se digita (quando o `stdin` é um terminal).
pub fn secret(question: &str) -> io::Result<String> {
    eprint!("{question}");
    io::stderr().flush()?;
    let hidden = io::stdin().is_terminal() && stty("-echo");
    let text = read_line();
    if hidden {
        stty("echo");
        eprintln!();
    }
    text
}

/// Pergunta sim ou não (`s`, `sim`, `y`, `yes`).
pub fn confirm(question: &str) -> io::Result<bool> {
    let answer = line(question)?;
    Ok(matches!(
        answer.trim().to_lowercase().as_str(),
        "s" | "sim" | "y" | "yes"
    ))
}

fn read_line() -> io::Result<String> {
    let mut text = String::new();
    if io::stdin().lock().read_line(&mut text)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "a entrada acabou antes da resposta",
        ));
    }
    Ok(text.trim_end_matches(['\n', '\r']).to_owned())
}

/// Liga ou desliga o eco do terminal. Devolve se deu certo (no Windows não há `stty`).
fn stty(argument: &str) -> bool {
    Command::new("stty")
        .arg(argument)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
