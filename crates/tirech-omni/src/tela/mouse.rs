//! O mouse no terminal de cada conexão (copiar e colar, ou o menu), na tela. A
//! parte pura (herança e textos) está em [`crate::mouse`]; aqui fica a linha "Mouse no terminal"
//! das propriedades da conexão e da pasta, no mesmo molde da linha "Sessão persistente", e o que
//! cada terminal pergunta para saber o que o mouse faz.

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{Tela, poe};
use crate::configuracoes::BotaoDireito;
use crate::modelo::{MouseDoTerminal, Node, Tree};
use crate::mouse;

/// As opções da linha do nó escolhido na árvore (as três, com um `Herdar` sem pasta se não há
/// nó).
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => mouse::opcoes(t, id),
        _ => mouse::opcoes(t, 0),
    })
}

impl Tela {
    /// Mostra na linha "Mouse no terminal" a escolha do nó escolhido (conexão ou pasta).
    pub(super) fn sincronizar_mouse(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| {
            let atual = match t.find(id)? {
                Node::Connection(c) => c.mouse,
                Node::Group(g) => g.mouse,
            };
            Some(mouse::rotulo(t, id, atual))
        });
        if let Some(texto) = texto {
            poe(self.campos.mouse, texto);
        }
    }

    /// Passa a linha "Mouse no terminal" para a pasta escolhida e grava. Lê o signal com
    /// rastreamento: serve a um efeito.
    pub(super) fn guardar_mouse_do_grupo(&self) {
        let texto = self.campos.mouse.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let nova = mouse::do_rotulo(&texto);
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id)
                    && g.mouse != nova
                {
                    g.mouse = nova;
                    mudou = true;
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }

    /// A escolha da conexão da aba `sessao` (a dela ou a da pasta). `None` segue as
    /// Configurações, também numa aba que não é de uma conexão. Rastreia a árvore: o terminal
    /// muda junto quando a conexão ou a pasta mudam.
    fn mouse_da_sessao(&self, sessao: Session) -> Option<MouseDoTerminal> {
        let id = self.conexao_da_sessao(sessao)?;
        self.tree.with(|t| mouse::da_conexao(t, id))
    }

    /// Se selecionar copia no terminal da aba `sessao`.
    pub(super) fn copiar_ao_selecionar(&self, sessao: Session) -> bool {
        let padrao = self.config.with(|c| c.terminal.copiar_ao_selecionar);
        mouse::copiar_ao_selecionar(self.mouse_da_sessao(sessao), padrao)
    }

    /// O que o botão direito faz no terminal da aba `sessao`.
    pub(super) fn botao_direito(&self, sessao: Session) -> BotaoDireito {
        let padrao = self.config.with(|c| c.terminal.botao_direito);
        mouse::botao_direito(self.mouse_da_sessao(sessao), padrao)
    }
}
