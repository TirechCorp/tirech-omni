//! Os padrões de arquivo do `glob` e do `grep` (M51): `*`, `**`, `?`, `[abc]`, `[!a-z]` e
//! `{a,b}`. Sem acesso ao disco: casa um caminho de texto com um padrão de texto.
//!
//! - `*` e `?` não atravessam o `/`; `*` casa nomes que começam com `.`.
//! - `**` sozinho num trecho (`src/**/x.rs`) casa zero ou mais pastas.
//! - `[abc]`, `[a-z]` e `[!a-z]` (ou `[^a-z]`); `\*` é um `*` de verdade.
//! - `{a,b}` vira alternativas, e pode aninhar (`{a,b{1,2}}`).
//! - Maiúsculas e minúsculas contam.

/// O máximo de alternativas que um padrão com `{ }` pode gerar.
const LIMITE: usize = 1024;

/// Se `caminho` (com `/`, relativo, sem `./`) casa com `padrao`.
///
/// ```
/// use tirech_omni::mcp::glob::casa;
///
/// assert!(casa("src/**/*.rs", "src/a/b/c.rs"));
/// assert!(casa("*.{rs,toml}", "Cargo.toml"));
/// assert!(!casa("*.rs", "src/a.rs"));
/// ```
pub fn casa(padrao: &str, caminho: &str) -> bool {
    let partes: Vec<&str> = caminho.split('/').collect();
    chaves(padrao).iter().any(|p| casa_trechos(p, &partes))
}

/// As alternativas de um padrão com `{ }` (ele mesmo, se não tem).
pub fn chaves(padrao: &str) -> Vec<String> {
    let mut saida = vec![padrao.to_string()];
    let mut i = 0;
    while i < saida.len() && saida.len() < LIMITE {
        let atual: Vec<char> = saida[i].chars().collect();
        match abre_e_fecha(&atual) {
            Some((a, f)) => {
                let antes: String = atual[..a].iter().collect();
                let depois: String = atual[f + 1..].iter().collect();
                let miolo = &atual[a + 1..f];
                let mut novas = Vec::new();
                let (mut prof, mut ini, mut k) = (0usize, 0usize, 0usize);
                while k <= miolo.len() {
                    let fim = k == miolo.len();
                    if !fim {
                        match miolo[k] {
                            '\\' => k += 1,
                            '{' => prof += 1,
                            '}' => prof = prof.saturating_sub(1),
                            _ => {}
                        }
                    }
                    if fim || (miolo[k] == ',' && prof == 0) {
                        let alt: String = miolo[ini..k.min(miolo.len())].iter().collect();
                        novas.push(format!("{antes}{alt}{depois}"));
                        ini = k + 1;
                    }
                    k += 1;
                }
                saida.splice(i..=i, novas);
            }
            None => i += 1,
        }
    }
    saida.truncate(LIMITE);
    saida
}

/// O primeiro par `{ }` casado e sem escape (e com uma vírgula dentro, no nível dele).
fn abre_e_fecha(cs: &[char]) -> Option<(usize, usize)> {
    let mut k = 0;
    while k < cs.len() {
        match cs[k] {
            '\\' => k += 1,
            '{' => {
                let (mut prof, mut j, mut virgula) = (0usize, k, false);
                while j < cs.len() {
                    match cs[j] {
                        '\\' => j += 1,
                        '{' => prof += 1,
                        '}' => {
                            prof -= 1;
                            if prof == 0 {
                                if virgula {
                                    return Some((k, j));
                                }
                                break;
                            }
                        }
                        ',' if prof == 1 => virgula = true,
                        _ => {}
                    }
                    j += 1;
                }
            }
            _ => {}
        }
        k += 1;
    }
    None
}

fn casa_trechos(padrao: &str, caminho: &[&str]) -> bool {
    let trechos: Vec<&str> = padrao.split('/').collect();
    trechos_casam(&trechos, caminho)
}

fn trechos_casam(padrao: &[&str], caminho: &[&str]) -> bool {
    match padrao.split_first() {
        None => caminho.is_empty(),
        Some((&"**", resto)) => {
            // Zero ou mais pastas. No fim do padrão, casa o que há dentro (pelo menos um nome).
            if resto.is_empty() {
                return !caminho.is_empty();
            }
            (0..=caminho.len()).any(|k| trechos_casam(resto, &caminho[k..]))
        }
        Some((p, resto)) => match caminho.split_first() {
            Some((c, mais)) => {
                let p: Vec<char> = p.chars().collect();
                let c: Vec<char> = c.chars().collect();
                nome_casa(&p, &c) && trechos_casam(resto, mais)
            }
            None => false,
        },
    }
}

/// Casa um nome (sem `/`) com um trecho do padrão.
fn nome_casa(p: &[char], c: &[char]) -> bool {
    match p.split_first() {
        None => c.is_empty(),
        Some(('*', resto)) => {
            // Vários `*` seguidos valem um (dentro de um trecho, `**` é um `*`).
            let mut resto = resto;
            while let Some(('*', r)) = resto.split_first() {
                resto = r;
            }
            (0..=c.len()).any(|k| nome_casa(resto, &c[k..]))
        }
        Some(('?', resto)) => !c.is_empty() && nome_casa(resto, &c[1..]),
        Some(('[', resto)) => match classe(resto) {
            Some((conjunto, negado, depois)) => match c.split_first() {
                Some((x, mais)) => {
                    (conjunto_tem(&conjunto, *x) != negado) && nome_casa(depois, mais)
                }
                None => false,
            },
            // Sem o `]`, o `[` é uma letra como outra qualquer.
            None => c.first() == Some(&'[') && nome_casa(resto, &c[1..]),
        },
        Some(('\\', resto)) if !resto.is_empty() => {
            c.first() == Some(&resto[0]) && nome_casa(&resto[1..], &c[1..])
        }
        Some((x, resto)) => c.first() == Some(x) && nome_casa(resto, &c[1..]),
    }
}

/// Lê o miolo de um `[ … ]` (já sem o `[`): os itens, se é negado e o que vem depois do `]`.
#[allow(clippy::type_complexity)]
fn classe(p: &[char]) -> Option<(Vec<(char, char)>, bool, &[char])> {
    let mut k = 0;
    let negado = matches!(p.first(), Some('!' | '^'));
    if negado {
        k = 1;
    }
    let mut itens = Vec::new();
    let inicio = k;
    while k < p.len() {
        // O primeiro `]` é uma letra.
        if p[k] == ']' && k > inicio {
            return Some((itens, negado, &p[k + 1..]));
        }
        let mut a = p[k];
        if a == '\\' && k + 1 < p.len() {
            k += 1;
            a = p[k];
        }
        if p.get(k + 1) == Some(&'-') && p.get(k + 2).is_some_and(|c| *c != ']') {
            itens.push((a, p[k + 2]));
            k += 3;
        } else {
            itens.push((a, a));
            k += 1;
        }
    }
    None
}

fn conjunto_tem(itens: &[(char, char)], x: char) -> bool {
    itens.iter().any(|(a, b)| *a <= x && x <= *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tabela_de_casos() {
        let casos = [
            // (padrão, caminho, casa?)
            ("*.rs", "a.rs", true),
            ("*.rs", "src/a.rs", false),
            ("*.rs", ".oculto.rs", true),
            ("*", ".git", true),
            ("*", "a/b", false),
            ("a?c", "abc", true),
            ("a?c", "a/c", false),
            ("a?c", "ac", false),
            ("**/*.rs", "a.rs", true),
            ("**/*.rs", "a/b/c.rs", true),
            ("src/**", "src/a/b.rs", true),
            ("src/**", "src", false),
            ("src/**/x.rs", "src/x.rs", true),
            ("src/**/x.rs", "src/a/b/x.rs", true),
            ("src/**/x.rs", "outro/x.rs", false),
            ("**", "qualquer/coisa", true),
            ("a**b", "axxb", true),
            ("a**b", "a/b", false),
            ("[abc].txt", "b.txt", true),
            ("[abc].txt", "d.txt", false),
            ("[a-c]1", "b1", true),
            ("[!a-c]1", "d1", true),
            ("[!a-c]1", "b1", false),
            ("[^a-c]1", "d1", true),
            ("[]]x", "]x", true),
            ("[a", "[a", true),
            ("{a,b}.txt", "b.txt", true),
            ("{a,b}.txt", "c.txt", false),
            ("*.{rs,toml}", "Cargo.toml", true),
            ("{src,tests}/**/*.rs", "tests/a/b.rs", true),
            ("{a,b{1,2}}x", "b2x", true),
            ("{a,b{1,2}}x", "b3x", false),
            ("{a}", "{a}", true),
            ("a{,b}", "a", true),
            ("a{,b}", "ab", true),
            ("\\*.txt", "*.txt", true),
            ("\\*.txt", "a.txt", false),
            ("Leia.md", "leia.md", false),
            ("", "", true),
            ("", "a", false),
            ("a/b", "a/b", true),
            ("a/b", "a/b/c", false),
        ];
        for (padrao, caminho, esperado) in casos {
            assert_eq!(casa(padrao, caminho), esperado, "{padrao} × {caminho}");
        }
    }

    #[test]
    fn chaves_demais_nao_explodem() {
        let padrao = "{a,b}".repeat(30);
        assert!(chaves(&padrao).len() <= LIMITE);
        assert!(!casa(&padrao, "zzz"));
    }

    #[test]
    fn expande_as_chaves() {
        assert_eq!(chaves("{a,b}c"), ["ac", "bc"]);
        assert_eq!(chaves("x"), ["x"]);
        assert_eq!(chaves("{a,b{1,2}}"), ["a", "b1", "b2"]);
    }
}
