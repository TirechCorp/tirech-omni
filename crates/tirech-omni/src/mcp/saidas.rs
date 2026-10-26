//! As saídas grandes (M51, seção 7.2): uma resposta que passa de `ia.caracteres_por_resposta`
//! volta com o começo e o fim, o meio vira uma linha dizendo quanto faltou, e a saída inteira
//! (já redigida) fica guardada aqui para o `ler_saida` buscar o resto.

use std::collections::HashMap;
use std::sync::Arc;

use crate::t;

/// Uma saída guardada: de qual conexão veio (o `ler_saida` confere a permissão dela) e o texto.
struct Guardada {
    conexao: u64,
    texto: Arc<str>,
}

/// As saídas guardadas de um servidor. Esvazia quando a ponte que pediu se desliga ou na parada.
#[derive(Default)]
pub struct Saidas {
    proximo: u64,
    guardadas: HashMap<String, Guardada>,
}

/// O trecho que `ler_saida` devolve.
pub struct Trecho {
    /// A conexão da saída original.
    pub conexao: u64,
    /// O texto do trecho, com a marca de "continua" quando há mais.
    pub texto: String,
}

/// Os `n` primeiros caracteres de `texto`.
fn comeco(texto: &str, n: usize) -> &str {
    match texto.char_indices().nth(n) {
        Some((i, _)) => &texto[..i],
        None => texto,
    }
}

/// Os `n` últimos caracteres de `texto`.
fn fim(texto: &str, n: usize) -> &str {
    let total = texto.chars().count();
    if n >= total {
        return texto;
    }
    match texto.char_indices().nth(total - n) {
        Some((i, _)) => &texto[i..],
        None => "",
    }
}

impl Saidas {
    /// Devolve `texto` se couber em `limite` caracteres; senão guarda o texto inteiro e devolve o
    /// começo, a linha do meio e o fim. `limite == 0` não limita.
    pub fn limitar(&mut self, conexao: u64, texto: String, limite: usize) -> String {
        let total = texto.chars().count();
        if limite == 0 || total <= limite {
            return texto;
        }
        self.proximo += 1;
        let id = format!("s{}", self.proximo);
        let metade = limite / 2;
        let resto = limite - metade;
        let omitidos = total - metade - resto;
        let cortado = format!(
            "{}\n{}\n{}",
            comeco(&texto, metade),
            t!(
                "mcp-saida-omitida",
                n = omitidos.to_string(),
                id = id.as_str(),
                inicio = metade.to_string()
            ),
            fim(&texto, resto)
        );
        self.guardadas.insert(
            id,
            Guardada {
                conexao,
                texto: Arc::from(texto),
            },
        );
        cortado
    }

    /// A conexão de onde veio a saída `id`, para conferir a permissão antes de ler.
    pub fn conexao_de(&self, id: &str) -> Option<u64> {
        self.guardadas.get(id).map(|g| g.conexao)
    }

    /// O trecho de `tamanho` caracteres (ou `padrao`, sem `tamanho`) a partir do caractere
    /// `inicio`. Erro em português quando o id não existe ou o início passa do fim.
    pub fn ler(
        &self,
        id: &str,
        inicio: usize,
        tamanho: Option<usize>,
        padrao: usize,
    ) -> Result<Trecho, String> {
        let guardada = self
            .guardadas
            .get(id)
            .ok_or_else(|| t!("mcp-saida-inexistente", id = id))?;
        let total = guardada.texto.chars().count();
        if inicio >= total {
            return Err(t!("mcp-saida-inicio-fora", total = total.to_string()));
        }
        let quanto = match tamanho.unwrap_or(padrao) {
            0 => total,
            n => n,
        };
        let a_partir = match guardada.texto.char_indices().nth(inicio) {
            Some((i, _)) => &guardada.texto[i..],
            None => "",
        };
        let pedaco = comeco(a_partir, quanto);
        let lidos = pedaco.chars().count();
        let mut texto = pedaco.to_owned();
        if inicio + lidos < total {
            texto.push('\n');
            texto.push_str(&t!(
                "mcp-saida-continua",
                id = id,
                inicio = (inicio + lidos).to_string(),
                resta = (total - inicio - lidos).to_string()
            ));
        }
        Ok(Trecho {
            conexao: guardada.conexao,
            texto,
        })
    }

    /// Esquece tudo.
    pub fn limpar(&mut self) {
        self.guardadas.clear();
    }

    /// Quantas saídas há guardadas.
    pub fn quantas(&self) -> usize {
        self.guardadas.len()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_que_cabe_volta_igual_e_nao_guarda() {
        let mut s = Saidas::default();
        assert_eq!(s.limitar(1, "curto".into(), 100), "curto");
        assert_eq!(s.limitar(1, "x".repeat(100), 100).len(), 100);
        assert_eq!(s.quantas(), 0);
    }

    #[test]
    fn limite_zero_nao_corta() {
        let mut s = Saidas::default();
        assert_eq!(s.limitar(1, "x".repeat(10_000), 0).len(), 10_000);
    }

    #[test]
    fn corta_no_meio_com_o_comeco_e_o_fim() {
        let mut s = Saidas::default();
        let texto = format!("{}{}{}", "a".repeat(50), "m".repeat(100), "z".repeat(50));
        let cortado = s.limitar(3, texto, 100);
        assert!(cortado.starts_with(&"a".repeat(50)));
        assert!(cortado.ends_with(&"z".repeat(50)));
        assert!(!cortado.contains("mmm"));
        assert!(cortado.contains("s1"));
        assert!(cortado.contains("100"));
        assert_eq!(s.conexao_de("s1"), Some(3));
    }

    #[test]
    fn ler_saida_devolve_o_resto_em_pedacos_e_diz_onde_continua() {
        let mut s = Saidas::default();
        let texto: String = (0..300)
            .map(|i| char::from(b'a' + (i % 26) as u8))
            .collect();
        s.limitar(1, texto.clone(), 100);
        let t1 = s.ler("s1", 50, Some(100), 100).unwrap();
        assert!(t1.texto.starts_with(&texto[50..150]));
        assert!(t1.texto.contains("inicio=150"));
        let ultimo = s.ler("s1", 250, None, 1000).unwrap();
        assert_eq!(ultimo.texto, texto[250..]);
    }

    #[test]
    fn conta_caracteres_e_nao_bytes() {
        let mut s = Saidas::default();
        let texto = "ção".repeat(100);
        let cortado = s.limitar(1, texto.clone(), 100);
        assert!(cortado.contains("ção"));
        let t = s.ler("s1", 290, Some(100), 100).unwrap();
        assert_eq!(
            t.texto,
            "ção".repeat(100).chars().skip(290).collect::<String>()
        );
    }

    #[test]
    fn erros_de_id_e_de_inicio() {
        let mut s = Saidas::default();
        assert!(s.ler("s9", 0, None, 10).is_err());
        s.limitar(1, "x".repeat(300), 100);
        assert!(s.ler("s1", 300, None, 10).is_err());
        s.limpar();
        assert!(s.ler("s1", 0, None, 10).is_err());
    }
}
