//! A barra de envio para vários e o diálogo de escolher as abas (M42): o que se digita na barra
//! vai para as abas escolhidas da janela, sem depender de qual terminal está com o foco.

use std::collections::BTreeSet;
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::request_focus;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::envio::{self, TECLAS};
use crate::janelas::Janela;
use crate::t;

impl Tela {
    /// Abre a barra de envio de `janela`. Com o grupo vazio, ele começa com todas as abas da
    /// janela. Já aberta, só leva o foco ao campo. Não muda o modo do teclado.
    pub(super) fn abrir_barra_de_envio(&self, janela: Janela) {
        let envio = janela.envio;
        if envio.grupo.with_untracked(BTreeSet::is_empty) {
            let todas = janela
                .layout
                .try_with_untracked(|l| l.sessions())
                .unwrap_or_default()
                .iter()
                .map(Session::key_str)
                .collect();
            envio.grupo.set(todas);
        }
        envio.barra.set(true);
        request_focus("multi-texto");
    }

    /// Manda `bytes` às abas escolhidas de `janela` que estão de pé. Sem aba escolhida, ou com
    /// todas caídas, avisa e não manda nada. Devolve se alguma aba recebeu.
    fn enviar_para_o_grupo(&self, janela: Janela, bytes: &[u8]) -> bool {
        let sessoes = janela
            .layout
            .try_with_untracked(|l| l.sessions())
            .unwrap_or_default();
        let grupo = janela.envio.grupo.get_untracked();
        let alvos = envio::alvos(&grupo, &sessoes);
        if alvos.is_empty() {
            let texto = if grupo.is_empty() {
                t!("envio-escolha-as-abas")
            } else {
                t!("envio-nenhuma-conectada")
            };
            self.avisar(InfoBarSeverity::Warning, texto);
            return false;
        }
        for sessao in alvos {
            sessao.send(bytes);
        }
        true
    }

    /// O que a barra faz ao enviar o texto: manda (com o Enter, se marcado), guarda no
    /// histórico e limpa o campo.
    fn enviar_da_barra(&self, janela: Janela, texto: &str) {
        let envio = janela.envio;
        let bytes = envio::bytes_do_envio(texto, envio.enter.get_untracked());
        if !self.enviar_para_o_grupo(janela, &bytes) {
            return;
        }
        envio.historico.update(|h| envio::lembrar(h, texto));
        envio.texto.set(String::new());
    }
}

/// O item "Receber o envio para vários" (`receber-envio-{chave}`) do menu da aba: marca a aba no grupo da janela, no
/// mesmo grupo que o diálogo da barra edita.
pub(super) fn receber_envio(janela: Janela, sessao: Session, menu: MenuFlyout) -> MenuFlyout {
    let grupo = janela.envio.grupo;
    let chave = sessao.key_str();
    let id = chave.clone();
    let recebe = signal(grupo.with_untracked(|g| g.contains(&chave)));
    // O toggle leva ao grupo e o grupo leva ao toggle; cada um só escreve quando difere.
    {
        let chave = chave.clone();
        effect(move || {
            let marcado = recebe.get();
            if grupo.with_untracked(|g| g.contains(&chave)) != marcado {
                grupo.update(|g| {
                    if marcado {
                        g.insert(chave.clone());
                    } else {
                        g.remove(&chave);
                    }
                });
            }
        });
    }
    effect(move || {
        let esta = grupo.with(|g| g.contains(&chave));
        if recebe.get_untracked() != esta {
            recebe.set(esta);
        }
    });
    // Cada aba tem o seu menu; o id leva a chave para não se repetir entre as abas.
    menu.separator().toggle(
        format!("receber-envio-{id}"),
        t!("envio-receber-o-envio-para-varios"),
        recebe,
    )
}

/// A barra de envio, embaixo dos painéis da janela.
pub(super) fn barra(tela: &Rc<Tela>, janela: Janela) -> Border {
    let envio = janela.envio;
    let rotulo = move || {
        let n = envio.grupo.with(BTreeSet::len);
        t!("envio-n-abas", n = n as i64)
    };
    let historico = envio.historico;
    let campo = {
        let t = Rc::clone(tela);
        AutoSuggestBox::bind(envio.texto, move |digitado| {
            historico.with_untracked(|h| envio::sugestoes(h, digitado))
        })
        .id("multi-texto")
        .grow(1)
        .placeholder(t!("envio-comando-para-as-abas-escolhidas"))
        .on_submit(move |texto| t.enviar_da_barra(janela, &texto))
    };
    let teclas = {
        let mut menu = MenuFlyout::new().id("multi-teclas-menu");
        for (id, nome, bytes) in TECLAS {
            let t = Rc::clone(tela);
            menu = menu.item(id, nome, move || {
                t.enviar_para_o_grupo(janela, bytes);
            });
        }
        menu
    };
    let linha = Row::new()
        .gap(8)
        .align(Align::Center)
        .child(Icon::new(Symbol::Broadcast).size(16).decorative())
        .child(Text::new(t!("comum-enviar-para")).caption().secondary())
        .child(
            Button::new(rotulo)
                .id("multi-abas")
                .icon(Symbol::Grid)
                .on_click(move || envio.dialogo.set(true)),
        )
        .child(campo)
        .child(CheckBox::bind(envio.enter, t!("envio-enter")).id("multi-enter"))
        .child(
            Button::new(t!("envio-teclas"))
                .id("multi-teclas")
                .icon(Symbol::Keyboard)
                .flyout(teclas),
        )
        .child({
            let t = Rc::clone(tela);
            Button::new(t!("envio-enviar"))
                .id("multi-enviar")
                .icon(Symbol::Send)
                .accent()
                .on_click(move || {
                    let texto = envio.texto.get_untracked();
                    t.enviar_da_barra(janela, &texto);
                })
        })
        .child(
            Button::icon_only(Symbol::Dismiss)
                .label(t!("envio-fechar-a-barra"))
                .id("multi-fechar")
                .on_click(move || envio.barra.set(false)),
        );
    Border::new()
        .id("multi-entrada")
        .visible(envio.barra)
        .background(Token::LayerFillColorDefault)
        .padding(8)
        .child(linha)
}

/// O diálogo "Abas que recebem o envio": a seleção da grade é o grupo da janela, e a mudança
/// vale na hora.
pub(super) fn dialogo(janela: Janela) -> ContentDialog {
    let envio = janela.envio;
    let layout = janela.layout;
    // Uma linha por aba da janela; muda com as abas e com a queda de cada sessão.
    effect(move || {
        let linhas = layout
            .try_with(|l| l.sessions())
            .unwrap_or_default()
            .into_iter()
            .map(|sessao| {
                let titulo = sessao.title();
                let caida = sessao.closed();
                let estado = if caida {
                    t!("envio-caida")
                } else {
                    t!("envio-conectada")
                };
                DataGridRow::new(sessao.key_str())
                    .cell(titulo.clone(), CellValue::Text(titulo))
                    .cell(estado.clone(), CellValue::Text(estado))
                    .icon(if caida {
                        Symbol::Warning
                    } else {
                        Symbol::WindowConsole
                    })
            })
            .collect();
        envio.linhas.set(linhas);
    });
    let grade = DataGrid::bind(envio.linhas, envio.grupo)
        .id("multi-abas-grade")
        .height(240)
        .columns([
            DataGridColumn::new("aba", t!("envio-aba")).width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("estado", t!("comum-estado")).width(ColumnWidth::Pixels(100.0)),
        ])
        .empty_text(t!("envio-nenhuma-aba-aberta-nesta-janela"));
    let botoes = Row::new()
        .gap(8)
        .child(
            Button::new(t!("envio-todas"))
                .id("multi-abas-todas")
                .on_click(move || {
                    let todas = envio
                        .linhas
                        .with_untracked(|l| l.iter().map(|r| r.key.clone()).collect());
                    envio.grupo.set(todas);
                }),
        )
        .child(
            Button::new(t!("envio-nenhuma"))
                .id("multi-abas-nenhuma")
                .on_click(move || envio.grupo.set(BTreeSet::new())),
        );
    ContentDialog::new(t!("envio-abas-que-recebem-o-envio"), envio.dialogo)
        .id("multi-abas-dialogo")
        .content(Column::new().gap(8).child(grade).child(botoes))
        .close(t!("comum-fechar"))
}
