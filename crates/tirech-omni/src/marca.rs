//! A marca do app (M49): o nome que a pessoa lê e o selo da barra de título.
//!
//! O nome exibido é "Tirech Omni" (até o M54 era "Tirech SSH"; a troca é do M55). O identificador
//! `tirech-omni` está no binário, no `app_id`, nos nomes de arquivo e de pasta. Os identificadores
//! de formato gravados em arquivos de quem usa (o cofre, os logs cifrados, o JSON do cofre) ficam
//! com o nome antigo: mudar esses quebraria dados salvos.

use tomni_ui::{Color, TitleBadge};

/// O nome do app para quem lê: o título da janela, o `.desktop`, o valor do registro.
pub const NOME_DO_APP: &str = "Tirech Omni";

/// O identificador do app para o sistema: o `app_id` da janela, o `StartupWMClass` e o `Icon=`
/// do `.desktop` e o nome do arquivo do ícone.
pub const ID_DO_APP: &str = "tirech-omni";

/// O fundo do selo, sólido. Branco sobre ele dá 11,64 : 1 e o ciano, 5,80 : 1 (WCAG 2).
pub const AZUL_DO_SELO: Color = Color::hex(0x063A66);
/// O texto "Tirech" do selo.
pub const BRANCO_DO_SELO: Color = Color::WHITE;
/// O texto "Omni" do selo.
pub const CIANO_DO_SELO: Color = Color::hex(0x4CC2FF);

/// O selo da barra de título: "Tirech" comum e "Omni" em negrito, no azul da marca.
pub fn selo() -> TitleBadge {
    TitleBadge::new(AZUL_DO_SELO)
        .part("Tirech ", BRANCO_DO_SELO)
        .bold_part("Omni", CIANO_DO_SELO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuracoes::Idioma;
    use crate::i18n;
    use fluent_bundle::FluentArgs;

    #[test]
    fn o_nome_do_app_vem_do_termo() {
        let mut args = FluentArgs::new();
        args.set("n", 2);
        for idioma in [Idioma::PtBr, Idioma::EnUs, Idioma::EsEs] {
            let titulo = i18n::texto_em(idioma, "casca-titulo-da-janela", Some(&args));
            assert!(titulo.starts_with(NOME_DO_APP), "{idioma:?}: {titulo}");
        }
    }

    #[test]
    fn o_id_do_app_e_o_nome_em_minusculas_e_com_hifen() {
        assert_eq!(ID_DO_APP, NOME_DO_APP.to_lowercase().replace(' ', "-"));
    }
}
