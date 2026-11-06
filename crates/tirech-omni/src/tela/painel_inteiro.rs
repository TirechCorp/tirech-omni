//! Fechar e mover um painel inteiro, com as abas dele. "Fechar o painel…" (no menu da aba, no menu
//! Janela e no atalho) pergunta o que fazer com as conexões: fechá-las junto ou levá-las para um
//! painel que já existe, nesta janela ou em outra. "Mover o painel para a janela" leva o painel
//! como ele está: as abas continuam juntas, num painel próprio da janela de destino.
//!
//! As sessões seguem vivas ao mudar de janela, com o histórico e a conexão. Um painel vazio fecha
//! sem perguntar, e a janela secundária que fica sem abas fecha junto.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::{PaneDirection, Session, focus_active_pane};

use super::Tela;
use crate::janelas::{self, Janela};
use crate::t;

/// Um painel que pode receber as abas: o nome na lista e onde ele está.
struct Destino {
    rotulo: String,
    janela: WindowHandle,
    painel: usize,
}

/// O estado da pergunta "Fechar o painel?".
pub(super) struct Estado {
    aberto: Signal<bool>,
    /// A janela e uma aba do painel: o índice muda se outro painel fechar com a pergunta aberta.
    alvo: Cell<Option<(Janela, Session)>>,
    numero: Signal<usize>,
    abas: Signal<usize>,
    destinos: RefCell<Vec<Destino>>,
    /// Se há para onde mover (`destinos` não é reativo).
    tem_destino: Signal<bool>,
    /// O rótulo do painel escolhido.
    destino: Signal<String>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            alvo: Cell::new(None),
            numero: signal(0),
            abas: signal(0),
            destinos: RefCell::default(),
            tem_destino: signal(false),
            destino: signal(String::new()),
        }
    }
}

impl Tela {
    /// "Fechar o painel": pergunta o que fazer com as abas. Um painel vazio fecha direto.
    pub(super) fn pedir_fechar_painel(&self, janela: Janela, painel: usize) {
        let (ancora, abas) = janela.layout.with_untracked(|l| {
            let p = l.pane(painel);
            (p.selected.or_else(|| p.tabs.first().copied()), p.tabs.len())
        });
        let Some(ancora) = ancora else {
            janela.layout.update(|l| {
                l.close_pane(painel);
            });
            focus_active_pane(janela.layout);
            return;
        };
        let destinos = self.destinos_do_painel(janela, painel);
        let estado = &self.painel_inteiro;
        estado.alvo.set(Some((janela, ancora)));
        estado.numero.set(painel + 1);
        estado.abas.set(abas);
        estado.tem_destino.set(!destinos.is_empty());
        estado.destino.set(
            destinos
                .first()
                .map(|d| d.rotulo.clone())
                .unwrap_or_default(),
        );
        *estado.destinos.borrow_mut() = destinos;
        estado.aberto.set(true);
    }

    /// O atalho de fechar o painel: pergunta só se há para onde levar as abas. Sem outro painel em
    /// nenhuma janela não há escolha a oferecer, e ele fecha direto.
    pub(super) fn atalho_fechar_painel(&self, janela: Janela, painel: usize) {
        let ancora = janela.layout.with_untracked(|l| {
            let p = l.pane(painel);
            p.selected.or_else(|| p.tabs.first().copied())
        });
        match ancora {
            Some(ancora) if self.destinos_do_painel(janela, painel).is_empty() => {
                self.painel_inteiro.alvo.set(Some((janela, ancora)));
                self.fechar_abas_do_painel();
            }
            _ => self.pedir_fechar_painel(janela, painel),
        }
    }

    /// Os painéis que podem receber as abas do painel `painel` de `janela`: primeiro os da própria
    /// janela, depois os das outras, na ordem delas.
    fn destinos_do_painel(&self, janela: Janela, painel: usize) -> Vec<Destino> {
        let todas = self.janelas.todas_agora();
        let (daqui, outras): (Vec<_>, Vec<_>) = todas
            .iter()
            .copied()
            .enumerate()
            .partition(|(_, j)| j.handle == janela.handle);
        let mut destinos = Vec::new();
        for (n, j) in daqui.into_iter().chain(outras) {
            let aqui = j.handle == janela.handle;
            let nome_da_janela = if aqui {
                t!("casca-esta-janela")
            } else {
                janelas::no_menu(n)
            };
            let paineis: Vec<(usize, Option<Session>)> = j
                .layout
                .try_with_untracked(|l| {
                    (0..l.pane_count())
                        .filter(|&i| !aqui || i != painel)
                        .map(|i| (i, l.pane(i).selected))
                        .collect()
                })
                .unwrap_or_default();
            for (i, escolhida) in paineis {
                let nome_do_painel = match escolhida {
                    Some(s) => t!(
                        "casca-painel-n-conexao",
                        n = i + 1,
                        conexao = untrack(|| s.title())
                    ),
                    None => t!("casca-painel-n", n = i + 1),
                };
                destinos.push(Destino {
                    rotulo: t!(
                        "casca-janela-e-painel",
                        janela = nome_da_janela.clone(),
                        painel = nome_do_painel
                    ),
                    janela: j.handle,
                    painel: i,
                });
            }
        }
        destinos
    }

    /// A resposta "Mover as abas": leva as abas ao painel escolhido e fecha o de origem.
    fn mover_abas_do_painel(&self) {
        let estado = &self.painel_inteiro;
        let Some((janela, ancora)) = estado.alvo.take() else {
            return;
        };
        let rotulo = estado.destino.get_untracked();
        let escolhido = estado
            .destinos
            .borrow()
            .iter()
            .find(|d| d.rotulo == rotulo)
            .map(|d| (d.janela, d.painel));
        // A janela de destino pode ter fechado com a pergunta aberta: nada sai do lugar.
        if let Some((handle, painel)) = escolhido
            && let Some(destino) = self.janelas.achar(handle)
        {
            self.mover_painel(janela, ancora, destino, Some(painel));
        }
    }

    /// A resposta "Fechar as abas": fecha o painel e as conexões dele, como o ✕ de cada aba.
    fn fechar_abas_do_painel(&self) {
        let Some((janela, ancora)) = self.painel_inteiro.alvo.take() else {
            return;
        };
        let Some(painel) = janela.layout.with_untracked(|l| l.pane_of(ancora)) else {
            return;
        };
        let mut abas = Vec::new();
        janela.layout.update(|l| abas = l.close_pane(painel));
        for sessao in &abas {
            self.esquecer_aba(janela, *sessao);
        }
        self.fechar_abas_tmux(abas.clone());
        for sessao in abas {
            self.fechar_assistentes_de(sessao);
        }
        focus_active_pane(janela.layout);
        self.fechar_se_vazia(janela);
    }

    /// Leva as abas do painel de `ancora` para o painel `para` de `destino` (sem ele, para um painel
    /// só delas) e fecha o painel de origem. A aba escolhida continua escolhida.
    fn mover_painel(&self, origem: Janela, ancora: Session, destino: Janela, para: Option<usize>) {
        let Some(painel) = origem.layout.with_untracked(|l| l.pane_of(ancora)) else {
            return;
        };
        let escolhida = origem.layout.with_untracked(|l| l.pane(painel).selected);
        // Primeiro o painel sai: com a opção de fechar o painel vazio, ele sumiria na última aba,
        // e o índice do destino mudaria no meio do caminho.
        let mut abas = Vec::new();
        origem.layout.update(|l| abas = l.close_pane(painel));
        let alvo = match para {
            Some(p) if destino.handle == origem.handle && p > painel => p - 1,
            Some(p) => p,
            None => painel_para_receber(destino),
        };
        for sessao in abas {
            self.mover_sessao_ao_painel(sessao, destino, alvo);
        }
        if let Some(sessao) = escolhida {
            destino.layout.update(|l| l.select(sessao));
        }
        focus_active_pane(destino.layout);
        destino.handle.focus();
        focus_active_pane(origem.layout);
        self.fechar_se_vazia(origem);
    }
}

/// O painel de `janela` que recebe um painel inteiro: o primeiro vazio ou, sem nenhum, um novo ao
/// lado do ativo, para o lado em que ele é maior.
fn painel_para_receber(janela: Janela) -> usize {
    let vazio = janela
        .layout
        .with_untracked(|l| (0..l.pane_count()).find(|&i| l.pane(i).tabs.is_empty()));
    if let Some(vazio) = vazio {
        return vazio;
    }
    let mut novo = 0;
    janela.layout.update(|l| {
        let ativo = l.active();
        let [_, _, largura, altura] = l.rects()[ativo];
        let lado = if largura >= altura {
            PaneDirection::Right
        } else {
            PaneDirection::Down
        };
        novo = l.split(ativo, lado);
    });
    novo
}

/// O submenu "Mover o painel para a janela": as outras janelas e uma nova. Os ids são
/// `{prefixo}-menu`, `{prefixo}-{n}` e `{prefixo}-nova`. Como em "Mover para a janela", os itens
/// são reservados e só aparecem os das janelas que existem. `ancora` diz, na hora do clique, uma
/// aba do painel.
pub(super) fn submenu_mover(
    tela: &Rc<Tela>,
    janela: Janela,
    prefixo: &str,
    ancora: impl Fn() -> Option<Session> + Clone + 'static,
) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id(format!("{prefixo}-menu"));
    for n in 0..janelas::NO_MENU {
        let t = Rc::clone(tela);
        let lista = tela.janelas.clone();
        let ancora = ancora.clone();
        menu = menu
            .item(
                format!("{prefixo}-{}", n + 1),
                janelas::no_menu(n),
                move || {
                    if let (Some(sessao), Some(outra)) =
                        (ancora(), t.janelas.todas_agora().get(n).copied())
                    {
                        t.mover_painel(janela, sessao, outra, None);
                    }
                },
            )
            .item_visible(move || {
                lista
                    .todas()
                    .get(n)
                    .is_some_and(|outra| outra.handle != janela.handle)
            });
    }
    let t = Rc::clone(tela);
    menu.item(
        format!("{prefixo}-nova"),
        t!("casca-nova-janela"),
        move || {
            if let Some(sessao) = ancora()
                && let Some(nova) = t.abrir_janela((1, 1), None)
            {
                t.mover_painel(janela, sessao, nova, Some(0));
            }
        },
    )
}

/// A pergunta "Fechar o painel?". Com um painel para receber as abas, mover é a resposta do Enter;
/// sem nenhum, só resta fechar.
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl View + use<> {
    let estado = &tela.painel_inteiro;
    let (aberto, numero, abas, tem_destino, destino) = (
        estado.aberto,
        estado.numero,
        estado.abas,
        estado.tem_destino,
        estado.destino,
    );
    let texto = move || {
        let n = abas.get() as i64;
        if tem_destino.get() {
            t!("casca-painel-abas-mover-ou-fechar", n = n)
        } else {
            t!("casca-painel-abas-so-fechar", n = n)
        }
    };
    let lista = Rc::clone(tela);
    let principal = Rc::clone(tela);
    let secundaria = Rc::clone(tela);
    ContentDialog::new(
        move || t!("casca-fechar-o-painel-n", n = numero.get()),
        aberto,
    )
    .id("fechar-painel-dialogo")
    .content(
        Column::new()
            .gap(8)
            .child(Text::new(texto).id("fechar-painel-texto").wrap())
            // A lista muda entre uma abertura e outra: o seletor nasce com o diálogo.
            .child(Show::when(
                move || aberto.get() && tem_destino.get(),
                move || {
                    let nomes: Vec<String> = lista
                        .painel_inteiro
                        .destinos
                        .borrow()
                        .iter()
                        .map(|d| d.rotulo.clone())
                        .collect();
                    Column::new()
                        .gap(8)
                        .child(Text::new(t!("casca-mover-as-abas-para")).caption())
                        .child(ComboBox::bind(destino, nomes).id("fechar-painel-destino"))
                },
            )),
    )
    .primary(
        move || {
            if tem_destino.get() {
                t!("casca-mover-as-abas")
            } else {
                t!("casca-fechar-as-abas")
            }
        },
        move || {
            if tem_destino.get_untracked() {
                principal.mover_abas_do_painel();
            } else {
                principal.fechar_abas_do_painel();
            }
        },
    )
    .secondary(t!("casca-fechar-as-abas"), move || {
        secundaria.fechar_abas_do_painel()
    })
    .secondary_visible(move || tem_destino.get())
    .close(t!("comum-cancelar"))
}
