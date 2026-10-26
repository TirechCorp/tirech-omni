//! O tempo no formato do OpenSSH (`-t` do `ssh-add` e do `ssh-agent`).

use std::time::Duration;

/// Lê `30` (segundos), `10m`, `1h`, `2d`, `1w` e somas como `1h30m`. Zero ou vazio é um erro.
pub fn parse(text: &str) -> Result<Duration, String> {
    let invalid = || format!("tempo inválido: {text}");
    let mut total: u64 = 0;
    let mut digits = String::new();
    for character in text.trim().chars() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        let unit: u64 = match character.to_ascii_lowercase() {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            'd' => 86_400,
            'w' => 604_800,
            _ => return Err(invalid()),
        };
        let amount: u64 = digits.parse().map_err(|_| invalid())?;
        total = amount
            .checked_mul(unit)
            .and_then(|seconds| total.checked_add(seconds))
            .ok_or_else(invalid)?;
        digits.clear();
    }
    if !digits.is_empty() {
        let seconds: u64 = digits.parse().map_err(|_| invalid())?;
        total = total.checked_add(seconds).ok_or_else(invalid)?;
    }
    if total == 0 {
        return Err(invalid());
    }
    Ok(Duration::from_secs(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unidades_e_somas() {
        let segundos = |texto| parse(texto).unwrap().as_secs();
        assert_eq!(segundos("90"), 90);
        assert_eq!(segundos("10m"), 600);
        assert_eq!(segundos("1h30m"), 5400);
        assert_eq!(segundos("2d"), 172_800);
        assert_eq!(segundos("1W"), 604_800);
        assert_eq!(segundos("1m30"), 90);
    }

    #[test]
    fn invalidos() {
        for texto in ["", "0", "abc", "10x", "m", "-5", "99999999999999999999"] {
            assert!(parse(texto).is_err(), "{texto:?}");
        }
    }
}
