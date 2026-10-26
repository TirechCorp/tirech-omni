//! As sessões de assistente ligadas ao mesmo tempo (M51, seção 6.8): o que o app guarda de cada
//! uma e a paleta de cores que as diferencia na marca da aba, no diálogo e nos menus.

use tomni_ui::prelude::*;

use super::servidor::SessaoLigada;

/// Uma sessão de assistente ligada, como a tela a conhece.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessaoIa {
    /// O número da ligação no servidor.
    pub ligacao: u64,
    /// O pid que a ponte declarou.
    pub pid: u32,
    /// O nome (`--nome`, "IA {n}" ou o do espaço).
    pub nome: String,
    /// O lugar na paleta (`PALETA[cor % 8]`).
    pub cor: usize,
    /// O espaço de trabalho a que a ponte está presa.
    pub espaco: Option<String>,
    /// A aba a que a ponte está presa (`--aba`, M63).
    pub aba: Option<String>,
    /// Quando o app viu a ponte ligar (o "desde quando" do menu).
    pub desde: std::time::Instant,
}

impl SessaoIa {
    /// A sessão que o servidor anunciou, com a cor `cor`.
    pub fn nova(ligada: &SessaoLigada, cor: usize) -> Self {
        Self {
            ligacao: ligada.ligacao,
            pid: ligada.pid,
            nome: ligada.nome.clone(),
            cor,
            espaco: ligada.espaco.clone(),
            aba: ligada.aba.clone(),
            desde: std::time::Instant::now(),
        }
    }
}

/// As oito cores de sessão, `(claro, escuro)`: os tokens `ia_sessao_1` a `ia_sessao_8` do app.
/// Todas passam de 4:1 contra o fundo da aba selecionada e o da barra de abas nos dois temas
/// (o teste `a_paleta_tem_contraste_nos_dois_temas` confere 3:1 com os valores do tema).
pub const PALETA: [(u32, u32); 8] = [
    (0x0F6CBD, 0x4CA3F0),
    (0xC4314B, 0xF1707B),
    (0x107C10, 0x6CCB5F),
    (0x8764B8, 0xB59BE0),
    (0xCA5010, 0xFC9F5B),
    (0x038387, 0x4FD1D6),
    (0xB146C2, 0xE08AEB),
    (0x8E562E, 0xD39B6B),
];

/// A cor do lugar `cor` da paleta; da 9ª sessão em diante a paleta recomeça.
pub fn cor_da_paleta(cor: usize) -> ThemeColor {
    let (claro, escuro) = PALETA[cor % PALETA.len()];
    ThemeColor::themed(Color::hex(claro), Color::hex(escuro))
}

/// O lugar da paleta para uma sessão nova: o primeiro que nenhuma sessão em uso ocupa; com a
/// paleta toda em uso, o menos usado (o mais baixo, no empate).
pub fn cor_livre(em_uso: &[usize]) -> usize {
    let usos = |lugar: usize| {
        em_uso
            .iter()
            .filter(|&&c| c % PALETA.len() == lugar)
            .count()
    };
    (0..PALETA.len())
        .min_by_key(|&lugar| (usos(lugar), lugar))
        .unwrap_or(0)
}

#[cfg(test)]
mod testes {
    use tomni_ui::theme::{Theme, Tokens};

    use super::*;

    fn luminancia(cor: u32) -> f64 {
        let canal = |deslocamento: u32| {
            let v = f64::from((cor >> deslocamento) & 0xFF) / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * canal(16) + 0.7152 * canal(8) + 0.0722 * canal(0)
    }

    fn contraste(a: u32, b: u32) -> f64 {
        let (a, b) = (luminancia(a), luminancia(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
    }

    #[test]
    fn a_paleta_tem_contraste_nos_dois_temas() {
        for (tema, escuro) in [(Theme::Light, false), (Theme::Dark, true)] {
            let tokens = Tokens::new(tema);
            // A aba selecionada e a barra de abas (a base do Mica).
            let (selecionada, barra) = (
                tokens.solid_background_tertiary.to_rgba8(),
                tokens.solid_background_base.to_rgba8(),
            );
            let fundos = [
                rgb(selecionada.r, selecionada.g, selecionada.b),
                rgb(barra.r, barra.g, barra.b),
            ];
            for (lugar, (claro, escuro_cor)) in PALETA.iter().enumerate() {
                let cor = if escuro { *escuro_cor } else { *claro };
                for fundo in fundos {
                    let razao = contraste(cor, fundo);
                    assert!(
                        razao >= 3.0,
                        "ia_sessao_{} tem {razao:.2}:1 contra #{fundo:06X} no tema {tema:?}",
                        lugar + 1
                    );
                }
            }
        }
    }

    #[test]
    fn as_cores_sao_distintas_e_a_paleta_recomeca() {
        let mut claras: Vec<u32> = PALETA.iter().map(|p| p.0).collect();
        claras.sort_unstable();
        claras.dedup();
        assert_eq!(claras.len(), PALETA.len());
        assert_eq!(cor_da_paleta(0), cor_da_paleta(8));
        assert_ne!(cor_da_paleta(0), cor_da_paleta(1));
    }

    #[test]
    fn a_cor_livre_e_a_primeira_que_ninguem_usa() {
        assert_eq!(cor_livre(&[]), 0);
        assert_eq!(cor_livre(&[0]), 1);
        assert_eq!(cor_livre(&[0, 1, 3]), 2);
        // A cor de uma sessão que saiu volta a ficar livre.
        assert_eq!(cor_livre(&[1, 2]), 0);
    }

    #[test]
    fn com_a_paleta_toda_em_uso_a_cor_menos_usada_repete() {
        let todas: Vec<usize> = (0..8).collect();
        assert_eq!(cor_livre(&todas), 0);
        let mut nove = todas.clone();
        nove.push(0);
        assert_eq!(cor_livre(&nove), 1);
    }
}
