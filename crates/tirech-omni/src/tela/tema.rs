//! O botão de tema da barra de título: um menu com os três temas (o do sistema, claro e
//! escuro), ligado a `geral.tema` das Configurações. Trocar por aqui ou pela página de
//! Configurações dá no mesmo, e cada lado acompanha o outro.

use tomni_ui::prelude::*;

use crate::configuracoes::{Configuracoes, Tema};
use crate::t;
use crate::tela_configuracoes::ligar;

/// O botão "Tema" e o menu dele. Chame dentro da view: os efeitos que ligam o menu à
/// configuração morrem com ela.
pub(super) fn botao_de_tema(config: Signal<Configuracoes>) -> Button {
    let tema = ligar(config, |c| c.geral.tema, |c, v| c.geral.tema = v);
    let menu = MenuFlyout::new()
        .id("tema-menu")
        .radio(
            "tema-sistema",
            t!("opcao-tema-sistema"),
            tema,
            Tema::Sistema,
        )
        .radio("tema-claro", t!("opcao-tema-claro"), tema, Tema::Claro)
        .radio("tema-escuro", t!("opcao-tema-escuro"), tema, Tema::Escuro);
    Button::icon_only(Symbol::DarkTheme)
        .id("barra-de-titulo-tema")
        .label(t!("tema-tema"))
        .tooltip(t!("tema-tema"))
        .flyout(menu)
}
