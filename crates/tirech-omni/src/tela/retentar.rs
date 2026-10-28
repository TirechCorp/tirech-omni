//! A rodada nova de reconexão de cada conexão (Tentar de novo após), na tela. A parte pura
//! (herança e textos) está em [`crate::retentar`]; aqui fica a linha das propriedades da conexão e
//! da pasta, no mesmo molde da linha "Mouse no terminal", e o tempo que vale para uma aba. A
//! rodada em si mora em `ganchos.rs`.

use std::time::Duration;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{Tela, poe};
use crate::modelo::{Node, Retentar, Tree};
use crate::retentar;

/// A escolha gravada no nó `id` (conexão ou pasta); `Herdar` se ele não existe.
fn do_no(tree: &Tree, id: u64) -> Retentar {
    match tree.find(id) {
        Some(Node::Connection(c)) => c.retentar,
        Some(Node::Group(g)) => g.retentar,
        None => Retentar::Herdar,
    }
}

/// As opções da linha do nó escolhido na árvore (com um `Herdar` sem pasta se não há nó).
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => retentar::opcoes(t, id, do_no(t, id)),
        _ => retentar::opcoes(t, 0, Retentar::Herdar),
    })
}

impl Tela {
    /// Mostra na linha "Tentar de novo após" a escolha do nó escolhido (conexão ou pasta).
    pub(super) fn sincronizar_retentar(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| {
            t.find(id)?;
            Some(retentar::rotulo(t, id, do_no(t, id)))
        });
        if let Some(texto) = texto {
            poe(self.campos.retentar, texto);
        }
    }

    /// Passa a linha "Tentar de novo após" para a pasta escolhida e grava. Lê o signal com
    /// rastreamento: serve a um efeito.
    pub(super) fn guardar_retentar_do_grupo(&self) {
        let texto = self.campos.retentar.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id) {
                    let nova = retentar::do_rotulo(&texto, g.retentar);
                    if g.retentar != nova {
                        g.retentar = nova;
                        mudou = true;
                    }
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }

    /// Quanto a aba `sessao` espera, depois das tentativas, para começar outra rodada: a escolha
    /// da conexão (a dela ou a da pasta) ou, sem nenhuma, os `padrao_mins` das Configurações.
    /// `None` não tenta de novo.
    pub(super) fn pausa_da_reconexao(&self, sessao: Session, padrao_mins: u32) -> Option<Duration> {
        let escolha = self
            .conexao_da_sessao(sessao)
            .and_then(|id| self.tree.with_untracked(|t| retentar::da_conexao(t, id)));
        retentar::pausa(escolha, padrao_mins)
    }
}
