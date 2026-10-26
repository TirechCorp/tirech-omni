//! O `.gitignore` do `glob` e do `grep` (M51): as mesmas regras do Git para o que um arquivo
//! `.gitignore` esconde, sem acesso ao disco.
//!
//! - Linhas vazias e as que começam com `#` não valem; `\#` e `\!` são literais.
//! - `!` no começo desfaz uma regra anterior.
//! - `/` no fim: só pasta. `/` no começo ou no meio: o padrão vale a partir da pasta do
//!   `.gitignore`. Sem `/`, vale em qualquer nível abaixo dela.
//! - Vale a última regra que casa. Um arquivo dentro de uma pasta ignorada está ignorado e não
//!   dá para reincluí-lo.
//! - O `.gitignore` de uma subpasta só vale dentro dela e vence os de cima.
//! - `.git/info/exclude` e o `.gitignore` global do usuário não entram.

use super::glob;

#[derive(Debug, Clone)]
struct Regra {
    padrao: String,
    nega: bool,
    so_pasta: bool,
}

/// As regras de um arquivo `.gitignore`.
#[derive(Debug, Clone, Default)]
pub struct Gitignore {
    regras: Vec<Regra>,
}

impl Gitignore {
    pub fn novo(texto: &str) -> Self {
        let mut regras = Vec::new();
        for linha in texto.lines() {
            let mut linha = linha.trim_end_matches(['\r', ' ']);
            if linha.is_empty() || linha.starts_with('#') {
                continue;
            }
            let nega = linha.starts_with('!');
            if nega {
                linha = &linha[1..];
            }
            let so_pasta = linha.ends_with('/');
            let linha = linha.trim_end_matches('/');
            if linha.is_empty() {
                continue;
            }
            let padrao = match linha.strip_prefix('/') {
                Some(preso) => preso.to_string(),
                None if linha.contains('/') => linha.to_string(),
                None => format!("**/{linha}"),
            };
            regras.push(Regra {
                padrao,
                nega,
                so_pasta,
            });
        }
        Self { regras }
    }

    /// O que as regras dizem de `caminho` (relativo à pasta do arquivo): `Some(true)` ignorado,
    /// `Some(false)` reincluído por um `!`, `None` se nenhuma regra casa. Só olha o caminho
    /// em si, não as pastas de cima.
    fn veredito(&self, caminho: &str, pasta: bool) -> Option<bool> {
        self.regras
            .iter()
            .rev()
            .find(|r| (!r.so_pasta || pasta) && glob::casa(&r.padrao, caminho))
            .map(|r| !r.nega)
    }

    /// Se `caminho` está ignorado só por este arquivo (inclusive por uma pasta de cima).
    pub fn ignora(&self, caminho: &str, pasta: bool) -> bool {
        Gitignores::default()
            .com("", self.clone())
            .ignora(caminho, pasta)
    }
}

/// Os `.gitignore` de uma árvore: o de cada pasta, pelo caminho dela relativo à raiz do espaço
/// (`""` para a raiz).
#[derive(Debug, Clone, Default)]
pub struct Gitignores {
    arquivos: Vec<(String, Gitignore)>,
}

impl Gitignores {
    /// Acrescenta o `.gitignore` da pasta `base` (relativa à raiz, sem `/` no fim).
    pub fn com(mut self, base: &str, arquivo: Gitignore) -> Self {
        self.arquivos
            .push((base.trim_matches('/').to_string(), arquivo));
        // Do mais raso ao mais fundo: o de dentro vence.
        self.arquivos
            .sort_by_key(|(b, _)| b.split('/').filter(|p| !p.is_empty()).count());
        self
    }

    /// Se `caminho` (relativo à raiz) está ignorado. `pasta` diz se ele é uma pasta.
    pub fn ignora(&self, caminho: &str, pasta: bool) -> bool {
        let partes: Vec<&str> = caminho.split('/').filter(|p| !p.is_empty()).collect();
        for n in 1..=partes.len() {
            let prefixo = partes[..n].join("/");
            let eh_pasta = n < partes.len() || pasta;
            if self.veredito(&prefixo, eh_pasta) {
                return true;
            }
        }
        false
    }

    /// O veredito para um caminho só: vence o `.gitignore` mais fundo que tem uma regra que casa.
    fn veredito(&self, caminho: &str, pasta: bool) -> bool {
        let mut ignorado = false;
        for (base, arquivo) in &self.arquivos {
            let relativo = if base.is_empty() {
                Some(caminho)
            } else {
                caminho
                    .strip_prefix(base.as_str())
                    .and_then(|r| r.strip_prefix('/'))
            };
            if let Some(v) = relativo.and_then(|r| arquivo.veredito(r, pasta)) {
                ignorado = v;
            }
        }
        ignorado
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ig(texto: &str) -> Gitignore {
        Gitignore::novo(texto)
    }

    #[test]
    fn pasta_so_casa_pasta() {
        let g = ig("target/\n");
        assert!(g.ignora("target", true));
        assert!(!g.ignora("target", false));
        // O que há dentro de uma pasta ignorada está ignorado.
        assert!(g.ignora("target/debug/app", false));
        assert!(g.ignora("sub/target/x", false));
    }

    #[test]
    fn barra_no_comeco_prende_a_pasta() {
        let g = ig("/raiz.txt\ndocs/saida\n");
        assert!(g.ignora("raiz.txt", false));
        assert!(!g.ignora("sub/raiz.txt", false));
        assert!(g.ignora("docs/saida", false));
        assert!(!g.ignora("sub/docs/saida", false));
    }

    #[test]
    fn sem_barra_vale_em_qualquer_nivel() {
        let g = ig("*.log\n");
        assert!(g.ignora("a.log", false));
        assert!(g.ignora("a/b/c.log", false));
        assert!(!g.ignora("a/b/c.txt", false));
    }

    #[test]
    fn exclamacao_desfaz_a_regra() {
        let g = ig("*.log\n!importante.log\n");
        assert!(g.ignora("a.log", false));
        assert!(!g.ignora("importante.log", false));
        assert!(!g.ignora("sub/importante.log", false));
        // Se a ordem muda, vale a última.
        let g = ig("!importante.log\n*.log\n");
        assert!(g.ignora("importante.log", false));
    }

    #[test]
    fn nao_da_para_reincluir_dentro_de_pasta_ignorada() {
        let g = ig("build/\n!build/lib.so\n");
        assert!(g.ignora("build/lib.so", false));
    }

    #[test]
    fn comentario_e_linha_vazia_somem() {
        let g = ig("# comentario\n\n   \n*.tmp\n\\#literal\n");
        assert!(g.ignora("a.tmp", false));
        assert!(g.ignora("#literal", false));
        assert!(!g.ignora("comentario", false));
        assert!(!g.ignora("", false));
    }

    #[test]
    fn terminadores_do_windows_e_espacos_no_fim() {
        let g = ig("*.bak  \r\nx.txt\r\n");
        assert!(g.ignora("a.bak", false));
        assert!(g.ignora("x.txt", false));
    }

    #[test]
    fn gitignore_de_subpasta_so_vale_nela() {
        let todos = Gitignores::default()
            .com("", ig("*.log\n"))
            .com("app", ig("segredo.txt\n!debug.log\n"));
        assert!(!todos.ignora("segredo.txt", false));
        assert!(todos.ignora("app/segredo.txt", false));
        assert!(todos.ignora("app/fundo/segredo.txt", false));
        assert!(!todos.ignora("outra/segredo.txt", false));
        assert!(!todos.ignora("app2/segredo.txt", false));
        // O de dentro vence o de cima.
        assert!(todos.ignora("outra/debug.log", false));
        assert!(!todos.ignora("app/debug.log", false));
    }
}
