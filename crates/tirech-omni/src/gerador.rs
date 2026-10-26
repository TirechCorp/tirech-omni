//! O gerador de senhas do cofre: sorteio com `getrandom`, sem viés de módulo.

const MAIUSCULAS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const MINUSCULAS: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITOS: &str = "0123456789";
const SIMBOLOS: &str = "!@#$%&*()-_=+[]{};:,.?/";
/// Os caracteres que se confundem na leitura.
const AMBIGUOS: &str = "0Oo1lI|";

/// O que a senha gerada pode ter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gerador {
    /// O tamanho, de 8 a 128 (fora disso é ajustado).
    pub tamanho: usize,
    /// Letras maiúsculas.
    pub maiusculas: bool,
    /// Letras minúsculas.
    pub minusculas: bool,
    /// Dígitos.
    pub digitos: bool,
    /// Símbolos: `!@#$%&*()-_=+[]{};:,.?/`.
    pub simbolos: bool,
    /// Tira `0 O o 1 l I |`.
    pub sem_ambiguos: bool,
}

impl Default for Gerador {
    fn default() -> Self {
        Self {
            tamanho: 20,
            maiusculas: true,
            minusculas: true,
            digitos: true,
            simbolos: true,
            sem_ambiguos: false,
        }
    }
}

impl Gerador {
    /// O menor tamanho aceito.
    pub const MIN: usize = 8;
    /// O maior tamanho aceito.
    pub const MAX: usize = 128;

    /// As classes ligadas, já sem os ambíguos (se for o caso). Nenhuma ligada vale minúsculas.
    fn classes(&self) -> Vec<Vec<char>> {
        let filtra = |texto: &str| -> Vec<char> {
            texto
                .chars()
                .filter(|c| !(self.sem_ambiguos && AMBIGUOS.contains(*c)))
                .collect()
        };
        let mut classes: Vec<Vec<char>> = [
            (self.maiusculas, MAIUSCULAS),
            (self.minusculas, MINUSCULAS),
            (self.digitos, DIGITOS),
            (self.simbolos, SIMBOLOS),
        ]
        .into_iter()
        .filter(|(ligada, _)| *ligada)
        .map(|(_, texto)| filtra(texto))
        .collect();
        if classes.is_empty() {
            classes.push(filtra(MINUSCULAS));
        }
        classes
    }
}

/// Um número em `0..limite`, sorteado sem viés: descarta o que passa do maior múltiplo de
/// `limite`.
fn sorteia(limite: usize) -> usize {
    debug_assert!(limite > 0);
    let limite = limite as u32;
    let teto = u32::MAX - (u32::MAX % limite);
    loop {
        let mut bytes = [0u8; 4];
        // Sem fonte de aleatório do sistema não há senha segura para dar.
        getrandom::fill(&mut bytes).expect("o sistema não deu números aleatórios");
        let n = u32::from_le_bytes(bytes);
        if n < teto {
            return (n % limite) as usize;
        }
    }
}

/// Gera uma senha. Tem ao menos um caractere de cada classe ligada.
pub fn gerar_senha(g: &Gerador) -> String {
    let tamanho = g.tamanho.clamp(Gerador::MIN, Gerador::MAX);
    let classes = g.classes();
    let todos: Vec<char> = classes.iter().flatten().copied().collect();
    let mut senha: Vec<char> = (0..tamanho).map(|_| todos[sorteia(todos.len())]).collect();

    // Um caractere de cada classe, em posições sorteadas e diferentes.
    let mut livres: Vec<usize> = (0..tamanho).collect();
    for classe in &classes {
        let posicao = livres.swap_remove(sorteia(livres.len()));
        senha[posicao] = classe[sorteia(classe.len())];
    }
    senha.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn so_ligada(g: &Gerador) -> [bool; 4] {
        [g.maiusculas, g.minusculas, g.digitos, g.simbolos]
    }

    #[test]
    fn padrao_tem_20_caracteres_de_todas_as_classes() {
        let senha = gerar_senha(&Gerador::default());
        assert_eq!(senha.chars().count(), 20);
    }

    #[test]
    fn toda_combinacao_de_classes_so_usa_as_ligadas_e_todas_aparecem() {
        for mascara in 1u8..16 {
            let g = Gerador {
                tamanho: 12,
                maiusculas: mascara & 1 != 0,
                minusculas: mascara & 2 != 0,
                digitos: mascara & 4 != 0,
                simbolos: mascara & 8 != 0,
                sem_ambiguos: false,
            };
            let textos = [MAIUSCULAS, MINUSCULAS, DIGITOS, SIMBOLOS];
            for _ in 0..200 {
                let senha = gerar_senha(&g);
                assert_eq!(senha.chars().count(), 12);
                for (ligada, texto) in so_ligada(&g).into_iter().zip(textos) {
                    let tem = senha.chars().any(|c| texto.contains(c));
                    assert_eq!(tem, ligada, "{senha} com a máscara {mascara}");
                }
            }
        }
    }

    #[test]
    fn sem_ambiguos_nunca_da_os_confusos() {
        let g = Gerador {
            sem_ambiguos: true,
            ..Gerador::default()
        };
        for _ in 0..200 {
            let senha = gerar_senha(&g);
            assert!(!senha.chars().any(|c| AMBIGUOS.contains(c)), "{senha}");
        }
    }

    #[test]
    fn sem_classe_ligada_usa_minusculas() {
        let g = Gerador {
            maiusculas: false,
            minusculas: false,
            digitos: false,
            simbolos: false,
            ..Gerador::default()
        };
        assert!(gerar_senha(&g).chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn tamanho_fora_da_faixa_e_ajustado() {
        let curto = Gerador {
            tamanho: 1,
            ..Gerador::default()
        };
        let longo = Gerador {
            tamanho: 10_000,
            ..Gerador::default()
        };
        assert_eq!(gerar_senha(&curto).chars().count(), 8);
        assert_eq!(gerar_senha(&longo).chars().count(), 128);
    }

    #[test]
    fn duas_senhas_seguidas_diferem() {
        let g = Gerador::default();
        assert_ne!(gerar_senha(&g), gerar_senha(&g));
    }
}
