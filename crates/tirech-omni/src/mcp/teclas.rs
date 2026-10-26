//! Os nomes de tecla da ferramenta `teclas` (M51, seção 7.1) e os bytes que cada um vira.
//!
//! O nome é o do atalho do app (`Ctrl+C`, `Alt+X`, `F2`, `PageDown`…), lido pelo mesmo
//! `Shortcut::from_str` da lib; os bytes saem do `encode_key` do terminal, o mesmo caminho do
//! teclado, com os modos atuais da tela (setas no modo aplicação, por exemplo).

use tomni_ui::event::Modifiers;
use tomni_ui::prelude::*;
use tomni_ui::terminal::emulator::Modes;
use tomni_ui::terminal::keys::{encode_key, encode_text};

use crate::t;

/// Os bytes do nome `nome`, ou o erro em português.
pub fn bytes_da_tecla(nome: &str, modes: &Modes) -> Result<Vec<u8>, String> {
    let atalho: Shortcut = nome
        .trim()
        .parse()
        .map_err(|_| t!("mcp-tecla-desconhecida", nome = nome))?;
    let Shortcut { key, modifiers } = atalho;
    let bytes = match key {
        // Uma letra sem Ctrl é texto: o `encode_key` só cuida do que o evento de texto não traz.
        Key::Char(c) if !modifiers.ctrl => {
            let c = if modifiers.shift {
                c.to_ascii_uppercase()
            } else {
                c
            };
            let sem_shift = Modifiers {
                shift: false,
                ..modifiers
            };
            Some(encode_text(&c.to_string(), sem_shift))
        }
        Key::Space if !modifiers.ctrl => Some(encode_text(" ", modifiers)),
        outra => encode_key(outra, modifiers, modes),
    };
    bytes
        .filter(|b| !b.is_empty())
        .ok_or_else(|| t!("mcp-tecla-sem-codigo", nome = nome))
}

/// Os bytes de todas as `teclas`, na ordem; falha na primeira que não entende, sem mandar nada.
pub fn bytes_das_teclas(teclas: &[String], modes: &Modes) -> Result<Vec<u8>, String> {
    let mut tudo = Vec::new();
    for tecla in teclas {
        tudo.extend(bytes_da_tecla(tecla, modes)?);
    }
    Ok(tudo)
}

/// Se `nome` é a tecla Enter (com ou sem modificadores, só Enter): o app confere a linha do
/// cursor contra a lista de bloqueio antes de mandá-la.
pub fn eh_enter(nome: &str) -> bool {
    matches!(
        nome.trim().parse::<Shortcut>(),
        Ok(Shortcut {
            key: Key::Enter,
            ..
        })
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    fn bytes(nome: &str) -> Vec<u8> {
        bytes_da_tecla(nome, &Modes::default()).unwrap()
    }

    #[test]
    fn as_teclas_comuns() {
        assert_eq!(bytes("Enter"), b"\r");
        assert_eq!(bytes("Tab"), b"\t");
        assert_eq!(bytes("Esc"), b"\x1b");
        assert_eq!(bytes("Backspace"), b"\x7f");
        assert_eq!(bytes("Ctrl+C"), [3]);
        assert_eq!(bytes("ctrl+d"), [4]);
        assert_eq!(bytes("Up"), b"\x1b[A");
        assert_eq!(bytes("F2"), b"\x1bOQ");
        assert_eq!(bytes("PageDown"), b"\x1b[6~");
        assert_eq!(bytes("Alt+X"), b"\x1bx");
    }

    #[test]
    fn letra_sozinha_e_texto() {
        assert_eq!(bytes("q"), b"q");
        assert_eq!(bytes("Space"), b" ");
    }

    #[test]
    fn as_setas_seguem_o_modo_da_aplicacao() {
        let modes = Modes {
            app_cursor: true,
            ..Modes::default()
        };
        assert_eq!(bytes_da_tecla("Up", &modes).unwrap(), b"\x1bOA");
    }

    #[test]
    fn nome_desconhecido_e_erro_e_nao_manda_nada() {
        assert!(bytes_da_tecla("Ctrl+Xyz", &Modes::default()).is_err());
        let r = bytes_das_teclas(
            &["Enter".to_owned(), "Inventada".to_owned()],
            &Modes::default(),
        );
        assert!(r.is_err());
        assert_eq!(
            bytes_das_teclas(&["Esc".to_owned(), ":".to_owned()], &Modes::default()).unwrap(),
            b"\x1b:"
        );
    }

    #[test]
    fn so_o_enter_conta_como_enter() {
        assert!(eh_enter("Enter"));
        assert!(eh_enter("return"));
        assert!(!eh_enter("Esc"));
        assert!(!eh_enter("Inventada"));
    }
}
