//! "Abrir em" (M36) na tela: o submenu que escolhe o painel de uma conexão (ou de uma pasta
//! inteira), a abertura no lugar escolhido e o estado que faz os painéis mostrarem o número
//! enquanto a pessoa escolhe. Os ids e a leitura do item em realce ficam em [`crate::abrir_em`].

use std::rc::Rc;
use std::time::Duration;

use tomni_ui::prelude::*;
use tomni_ui::terminal::PaneDirection;

use super::{Lugar, Tela};
use crate::abrir_em::{AlvoAbrir, Destino, Onde, PAINEIS, identificacao};
use crate::janelas::{self, Janela};
use crate::t;

/// Quanto tempo "Identificar os painéis" deixa os números na tela.
pub(super) const TEMPO_DE_IDENTIFICAR: Duration = Duration::from_secs(2);

impl Tela {
    /// A janela de destino do "Abrir em", lida com rastreamento: a última ativa ou, se ela já
    /// fechou, a principal.
    fn janela_de_destino(&self) -> Janela {
        let ativa = self.ativa.get();
        self.janelas
            .todas()
            .into_iter()
            .find(|j| j.handle == ativa)
            .unwrap_or_else(|| self.janelas.principal())
    }

    /// Quantos painéis a janela de destino tem (com rastreamento: o submenu acompanha a janela
    /// ativa e a grade sem ser montado de novo).
    fn paineis_do_destino(&self) -> usize {
        let janela = self.janela_de_destino();
        janela.layout.try_with(|l| l.pane_count()).unwrap_or(0)
    }

    /// Se `janela` mostra os números dos painéis agora e qual painel destacar: o item do menu em
    /// realce (o "Abrir em" aberto) ou o "Identificar os painéis" de Exibir. Lê signals: é o
    /// corpo de um `memo`.
    ///
    /// Devolve `None` quando a janela não identifica, e `Some(destaque)` quando identifica.
    pub(super) fn identificacao_da(&self, janela: Janela) -> Option<Option<usize>> {
        let realce = self.realce_menu.get();
        if let Some(i) = identificacao(realce.as_deref()) {
            let alvo = match i.onde {
                Onde::Destino => Some(self.janela_de_destino()),
                Onde::Janela(j) => self.janelas.todas().get(j).copied(),
            };
            if alvo.is_some_and(|a| a.handle == janela.handle) {
                return Some(i.painel);
            }
        }
        if self.identificando.get().is_some() && self.janela_de_destino().handle == janela.handle {
            return Some(None);
        }
        None
    }

    /// "Identificar os painéis": mostra os números na janela de destino por 2 segundos. Apertar
    /// de novo estende o tempo: cada pedido tem um número e só o alarme do último desliga.
    pub(super) fn identificar_paineis(&self) {
        let sinal = self.identificando;
        let pedido = sinal.get_untracked().map_or(1, |n| n.wrapping_add(1));
        sinal.set(Some(pedido));
        self.alarmes.depois(TEMPO_DE_IDENTIFICAR, move || {
            if sinal.try_get_untracked().flatten() == Some(pedido) {
                sinal.set(None);
            }
        });
    }

    /// As conexões que `alvo` abre num terminal, na ordem da árvore. Uma conexão só de arquivos
    /// (FTP, S3, rclone) não tem terminal: fica de fora.
    pub(super) fn conexoes_de_terminal(&self, alvo: AlvoAbrir) -> Vec<u64> {
        self.tree.with_untracked(|t| match alvo {
            AlvoAbrir::Conexao(id) => t
                .connection(id)
                .filter(|c| !c.kind.so_arquivos())
                .map(|c| c.id)
                .into_iter()
                .collect(),
            AlvoAbrir::Pasta(id) => t
                .connections_in(id)
                .into_iter()
                .filter(|c| !c.kind.so_arquivos())
                .map(|c| c.id)
                .collect(),
        })
    }

    /// Como [`Tela::conexoes_de_terminal`], com rastreamento: o item do menu some quando a
    /// árvore deixa de ter o que abrir.
    pub(super) fn tem_terminal(&self, alvo: AlvoAbrir) -> bool {
        self.tree.with(|t| match alvo {
            AlvoAbrir::Conexao(id) => t.connection(id).is_some_and(|c| !c.kind.so_arquivos()),
            AlvoAbrir::Pasta(id) => t.connections_in(id).iter().any(|c| !c.kind.so_arquivos()),
        })
    }

    /// "Abrir" numa pasta: abre todas as conexões de terminal dela, em qualquer profundidade e
    /// na ordem da árvore, cada uma onde uma conexão aberta pelo usuário entra (elas vão
    /// ocupando os painéis livres). Sem confirmação, por maior que seja a pasta.
    pub(super) fn abrir_pasta(self: &Rc<Self>, id: u64) {
        let conexoes = self.conexoes_de_terminal(AlvoAbrir::Pasta(id));
        for conexao in &conexoes {
            self.abrir(*conexao, self.lugar_novo());
        }
        if !conexoes.is_empty() {
            self.mostrar_terminais();
        }
    }

    /// Abre o que `alvo` diz em `destino` e vai para os Terminais. Todas as conexões de uma pasta
    /// entram no mesmo lugar, como abas dele.
    pub(super) fn abrir_em(self: &Rc<Self>, alvo: AlvoAbrir, destino: Destino) {
        let conexoes = self.conexoes_de_terminal(alvo);
        if conexoes.is_empty() {
            return;
        }
        let lugar = match destino {
            Destino::Painel(k) => Lugar::painel(self.janela_ativa(), k),
            Destino::Direita => {
                let janela = self.janela_ativa();
                let novo = janela.layout.try_update(|l| {
                    let ativo = l.active();
                    l.split(ativo, PaneDirection::Right)
                });
                match novo {
                    Some(novo) => Lugar::painel(janela, novo),
                    None => Lugar::default(),
                }
            }
            Destino::NovaJanela => match self.abrir_janela((1, 1), None) {
                Some(janela) => Lugar::painel(janela, 0),
                None => Lugar::default(),
            },
            Destino::Janela(j) => match self.janelas.todas_agora().get(j) {
                Some(janela) => Lugar {
                    janela: Some(janela.handle),
                    painel: None,
                },
                None => return,
            },
        };
        for id in conexoes {
            self.abrir(id, lugar);
        }
        if !matches!(destino, Destino::Janela(_)) {
            self.mostrar_terminais();
        }
    }
}

/// O submenu "Abrir em" (ou "Abrir todas em", para uma pasta). `base` é o id do menu de quem o
/// leva; os itens têm os ids `{base}-abrir-em-painel-{k}`, `-direita`, `-nova` e
/// `-janela-{j}` (veja [`crate::abrir_em`]).
fn submenu(tela: &Rc<Tela>, base: &str, alvo: AlvoAbrir) -> MenuFlyout {
    let id = |sufixo: &str| format!("{base}-abrir-em{sufixo}");
    let mut menu = MenuFlyout::new().id(id("-menu"));
    // Os itens de painel ficam todos reservados: o menu não muda de forma, só mostra os dos
    // painéis que a janela de destino tem na hora.
    for k in 0..PAINEIS {
        let t = Rc::clone(tela);
        let visivel = Rc::clone(tela);
        menu = menu
            .item(
                id(&format!("-painel-{k}")),
                t!("casca-painel-n", n = k + 1),
                move || t.abrir_em(alvo, Destino::Painel(k)),
            )
            .item_visible(move || visivel.paineis_do_destino() > k);
        if let Some(tecla) = char::from_digit(k as u32 + 1, 10).filter(|_| k < 9) {
            menu = menu.access_key(tecla);
        }
    }
    let t = Rc::clone(tela);
    menu = menu.separator().item(
        id("-direita"),
        t!("casca-novo-painel-a-direita"),
        move || t.abrir_em(alvo, Destino::Direita),
    );
    let t = Rc::clone(tela);
    menu = menu.item(id("-nova"), t!("casca-nova-janela"), move || {
        t.abrir_em(alvo, Destino::NovaJanela)
    });
    let mut outras = MenuFlyout::new().id(id("-outra-menu"));
    for j in 0..janelas::NO_MENU {
        let t = Rc::clone(tela);
        let visivel = Rc::clone(tela);
        outras = outras
            .item(
                id(&format!("-janela-{j}")),
                t!("casca-janela-n", n = j + 1),
                move || t.abrir_em(alvo, Destino::Janela(j)),
            )
            .item_visible(move || {
                let destino = visivel.janela_de_destino().handle;
                visivel
                    .janelas
                    .todas()
                    .get(j)
                    .is_some_and(|outra| outra.handle != destino)
            });
    }
    let varias = Rc::clone(tela);
    menu.submenu(id("-outra"), t!("casca-outra-janela"), outras)
        .item_visible(move || varias.janelas.quantas() > 1)
}

/// Acrescenta a `menu` o item que abre o submenu "Abrir em" (ou "Abrir todas em") e liga o
/// realce do menu à identificação dos painéis. O item some quando `alvo` não tem o que abrir
/// num terminal. `base` é o prefixo dos ids do submenu.
pub(super) fn com_abrir_em(
    tela: &Rc<Tela>,
    menu: MenuFlyout,
    base: &str,
    alvo: AlvoAbrir,
) -> MenuFlyout {
    let rotulo = match alvo {
        AlvoAbrir::Conexao(_) => t!("casca-abrir-em"),
        AlvoAbrir::Pasta(_) => t!("casca-abrir-todas-em"),
    };
    let visivel = Rc::clone(tela);
    menu.highlighted(tela.realce_menu)
        .submenu(
            format!("{base}-abrir-em"),
            rotulo,
            submenu(tela, base, alvo),
        )
        .icon(Symbol::Grid)
        .item_visible(move || visivel.tem_terminal(alvo))
}
