//! O que as configurações do terminal viram na lib: os esquemas de cores que a página oferece
//! (o Campbell, o do tema e os outros, com os valores dos esquemas de fábrica do Windows Terminal,
//! licença MIT), o cursor, o clique direito e a campainha.
//!
//! Adaptação: nos esquemas claros a seleção é o texto a 25%, em vez de uma seleção
//! branca, que não aparece sobre o fundo branco (o mesmo que a lib faz no
//! `ONE_HALF_LIGHT`).

use tomni_ui::Color;
use tomni_ui::terminal::{BellStyle, CursorShape, ONE_HALF_LIGHT, Palette, RightClick, Scheme};

use crate::configuracoes::{BotaoDireito, Campainha, Cursor, Esquema};

const BRANCO_50: Color = Color::rgba(0xFF, 0xFF, 0xFF, 0x80);

const fn escuro(fundo: u32, texto: u32, ansi: [u32; 16]) -> Palette {
    palette(fundo, texto, 0xFFFFFF, BRANCO_50, ansi)
}

const fn palette(fundo: u32, texto: u32, cursor: u32, selecao: Color, ansi: [u32; 16]) -> Palette {
    let mut cores = [Color::hex(0); 16];
    let mut i = 0;
    while i < 16 {
        cores[i] = Color::hex(ansi[i]);
        i += 1;
    }
    Palette {
        background: Color::hex(fundo),
        foreground: Color::hex(texto),
        cursor: Color::hex(cursor),
        selection: selecao,
        ansi: cores,
    }
}

/// O "Campbell PowerShell": as cores do Campbell sobre o azul do PowerShell.
const CAMPBELL_POWERSHELL: Palette = escuro(
    0x012456,
    0xCCCCCC,
    [
        0x0C0C0C, 0xC50F1F, 0x13A10E, 0xC19C00, 0x0037DA, 0x881798, 0x3A96DD, 0xCCCCCC, 0x767676,
        0xE74856, 0x16C60C, 0xF9F1A5, 0x3B78FF, 0xB4009E, 0x61D6D6, 0xF2F2F2,
    ],
);

/// O "Vintage": as 16 cores do console antigo do Windows.
const VINTAGE: Palette = escuro(
    0x000000,
    0xC0C0C0,
    [
        0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080, 0xC0C0C0, 0x808080,
        0xFF0000, 0x00FF00, 0xFFFF00, 0x0000FF, 0xFF00FF, 0x00FFFF, 0xFFFFFF,
    ],
);

/// O "One Half Dark".
const ONE_HALF_DARK: Palette = escuro(
    0x282C34,
    0xDCDFE4,
    [
        0x282C34, 0xE06C75, 0x98C379, 0xE5C07B, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xDCDFE4, 0x5A6374,
        0xE06C75, 0x98C379, 0xE5C07B, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xDCDFE4,
    ],
);

/// O "Tango Dark".
const TANGO_DARK: Palette = escuro(
    0x000000,
    0xD3D7CF,
    [
        0x000000, 0xCC0000, 0x4E9A06, 0xC4A000, 0x3465A4, 0x75507B, 0x06989A, 0xD3D7CF, 0x555753,
        0xEF2929, 0x8AE234, 0xFCE94F, 0x729FCF, 0xAD7FA8, 0x34E2E2, 0xEEEEEC,
    ],
);

/// O "Tango Light".
const TANGO_LIGHT: Palette = palette(
    0xFFFFFF,
    0x555753,
    0x000000,
    Color::rgba(0x55, 0x57, 0x53, 0x40),
    [
        0x000000, 0xCC0000, 0x4E9A06, 0xC4A000, 0x3465A4, 0x75507B, 0x06989A, 0xD3D7CF, 0x555753,
        0xEF2929, 0x8AE234, 0xFCE94F, 0x729FCF, 0xAD7FA8, 0x34E2E2, 0xEEEEEC,
    ],
);

/// O esquema da lib que a escolha pede.
pub fn esquema(escolha: Esquema) -> Scheme {
    match escolha {
        Esquema::Campbell => Scheme::Campbell,
        Esquema::Tema => Scheme::Theme,
        Esquema::CampbellPowershell => Scheme::Custom(CAMPBELL_POWERSHELL),
        Esquema::Vintage => Scheme::Custom(VINTAGE),
        Esquema::OneHalfDark => Scheme::Custom(ONE_HALF_DARK),
        Esquema::OneHalfLight => Scheme::Custom(ONE_HALF_LIGHT),
        Esquema::TangoDark => Scheme::Custom(TANGO_DARK),
        Esquema::TangoLight => Scheme::Custom(TANGO_LIGHT),
    }
}

/// O formato do cursor que a escolha pede.
pub fn cursor(escolha: Cursor) -> CursorShape {
    match escolha {
        Cursor::Barra => CursorShape::Bar,
        Cursor::Bloco => CursorShape::Block,
        Cursor::Sublinhado => CursorShape::Underline,
    }
}

/// O que o clique direito faz.
pub fn botao_direito(escolha: BotaoDireito) -> RightClick {
    match escolha {
        BotaoDireito::Menu => RightClick::Menu,
        BotaoDireito::CopiarColar => RightClick::CopyPaste,
    }
}

/// Como a campainha avisa.
pub fn campainha(escolha: Campainha) -> BellStyle {
    match escolha {
        Campainha::Piscar => BellStyle::Flash,
        Campainha::Nenhuma => BellStyle::None,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cada_esquema_vira_um_da_lib() {
        for e in Esquema::TODAS {
            let _ = esquema(*e);
        }
        assert_eq!(esquema(Esquema::Campbell), Scheme::Campbell);
        assert_eq!(esquema(Esquema::Tema), Scheme::Theme);
    }

    #[test]
    fn os_esquemas_de_fabrica_tem_as_cores_do_windows_terminal() {
        let Scheme::Custom(p) = esquema(Esquema::OneHalfDark) else {
            panic!("esperava cores próprias");
        };
        assert_eq!(p.background, Color::hex(0x282C34));
        assert_eq!(p.ansi[1], Color::hex(0xE06C75));
        let Scheme::Custom(p) = esquema(Esquema::CampbellPowershell) else {
            panic!("esperava cores próprias");
        };
        assert_eq!(p.background, Color::hex(0x012456));
    }
}
