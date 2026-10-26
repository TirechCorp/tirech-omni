//! O `getopt` curto dos programas: `-t ed25519`, `-ted25519`, `-pq` e `--` valem. Sem crate.

/// As opções lidas e o que sobrou (os argumentos que não são opção), na ordem.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Parsed {
    options: Vec<(char, Option<String>)>,
    /// Os argumentos que não são opção.
    pub rest: Vec<String>,
}

impl Parsed {
    /// Se a opção apareceu.
    pub fn has(&self, option: char) -> bool {
        self.options.iter().any(|(o, _)| *o == option)
    }

    /// O valor da última vez que a opção apareceu.
    pub fn value(&self, option: char) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|(o, _)| *o == option)
            .and_then(|(_, value)| value.as_deref())
    }

    /// Todos os valores da opção, na ordem.
    pub fn values(&self, option: char) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(o, _)| *o == option)
            .filter_map(|(_, value)| value.as_deref())
            .collect()
    }
}

/// Lê `args` segundo `spec`: cada letra é uma opção, e uma letra seguida de `:` pede um valor
/// (`"t:b:pq"`). As opções podem vir antes ou depois dos argumentos. `--` encerra as opções.
pub fn getopt(args: &[String], spec: &str) -> Result<Parsed, String> {
    let mut known = Vec::new();
    let mut chars = spec.chars().peekable();
    while let Some(letter) = chars.next() {
        let takes_value = chars.next_if_eq(&':').is_some();
        known.push((letter, takes_value));
    }

    let mut parsed = Parsed::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            parsed.rest.extend(iter.by_ref().cloned());
            break;
        }
        let Some(letters) = arg.strip_prefix('-').filter(|l| !l.is_empty()) else {
            parsed.rest.push(arg.clone());
            continue;
        };
        let mut letters = letters.chars();
        while let Some(letter) = letters.next() {
            let Some(&(_, takes_value)) = known.iter().find(|(l, _)| *l == letter) else {
                return Err(format!("opção desconhecida: -{letter}"));
            };
            if !takes_value {
                parsed.options.push((letter, None));
                continue;
            }
            // O valor é o resto do argumento (`-ted25519`) ou o argumento seguinte.
            let attached: String = letters.collect();
            let value = if attached.is_empty() {
                iter.next()
                    .cloned()
                    .ok_or_else(|| format!("a opção -{letter} precisa de um valor"))?
            } else {
                attached
            };
            parsed.options.push((letter, Some(value)));
            break;
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn valor_separado_ou_colado() {
        for linha in ["-t ed25519 -a 64", "-ted25519 -a64", "-t ed25519 -a64"] {
            let lidas = getopt(&args(linha), "t:a:").unwrap();
            assert_eq!(lidas.value('t'), Some("ed25519"), "{linha}");
            assert_eq!(lidas.value('a'), Some("64"), "{linha}");
        }
    }

    #[test]
    fn opcoes_agrupadas_e_repetidas() {
        let lidas = getopt(&args("-pq -o A=1 -oB=2 arquivo"), "pqo:").unwrap();
        assert!(lidas.has('p') && lidas.has('q'));
        assert_eq!(lidas.values('o'), ["A=1", "B=2"]);
        assert_eq!(lidas.value('o'), Some("B=2"));
        assert_eq!(lidas.rest, ["arquivo"]);
    }

    #[test]
    fn o_ultimo_valor_vale_e_o_agrupamento_termina_no_valor() {
        let lidas = getopt(&args("-qfchave -f outra"), "qf:").unwrap();
        assert!(lidas.has('q'));
        assert_eq!(lidas.value('f'), Some("outra"));
    }

    #[test]
    fn duplo_traco_encerra_as_opcoes() {
        let lidas = getopt(&args("-l -- -x arquivo"), "l").unwrap();
        assert!(lidas.has('l'));
        assert_eq!(lidas.rest, ["-x", "arquivo"]);
    }

    #[test]
    fn um_traco_sozinho_e_argumento() {
        let lidas = getopt(&args("-"), "l").unwrap();
        assert_eq!(lidas.rest, ["-"]);
    }

    #[test]
    fn erros_em_portugues() {
        assert_eq!(
            getopt(&args("-z"), "l").unwrap_err(),
            "opção desconhecida: -z"
        );
        assert_eq!(
            getopt(&args("-t"), "t:").unwrap_err(),
            "a opção -t precisa de um valor"
        );
    }
}
