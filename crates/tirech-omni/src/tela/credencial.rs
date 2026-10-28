//! O modo da credencial herdado pela pasta (o agente SSH para tudo o que está dentro, por
//! exemplo). A parte pura está em [`crate::credencial`]; aqui ficam a linha "Credencial" das
//! propriedades da pasta e a opção "Herdar" na linha da conexão.

use tomni_ui::prelude::*;

use super::{Tela, poe};
use crate::credencial;
use crate::modelo::{Connection, ModoCredencial, Node, Tree};

/// As opções da linha "Credencial" da pasta escolhida na árvore.
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => credencial::opcoes(t, id),
        _ => credencial::opcoes(t, 0),
    })
}

/// A opção "Herdar" da linha "Credencial" da conexão escolhida, com o que vale pela pasta.
pub(super) fn herdar_da_selecao(tree: Signal<Tree>, selecionado: Signal<Option<String>>) -> String {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| credencial::rotulo_de_herdar(t, id.unwrap_or(0)))
}

impl Tela {
    /// Mostra na linha "Credencial" da pasta o modo dela.
    pub(super) fn sincronizar_credencial_do_grupo(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| match t.find(id)? {
            Node::Group(g) => Some(credencial::rotulo(t, id, g.credencial_modo)),
            Node::Connection(_) => None,
        });
        if let Some(texto) = texto {
            poe(self.campos.credencial_da_pasta, texto);
        }
    }

    /// Na conexão SSH ou SFTP que herda e não tem item do cofre, a linha "Credencial" mostra a
    /// opção "Herdar" com o modo que vale pela pasta (o rótulo do modo não diz de onde vem).
    pub(super) fn sincronizar_credencial_herdada(&self, c: &Connection) {
        let sem_item = c
            .credencial
            .is_none_or(|id| self.cofre.agora(|x| x.item(id).is_none()));
        if c.kind.usa_ssh() && c.credencial_modo.is_herdar() && sem_item {
            let texto = self
                .tree
                .with_untracked(|t| credencial::rotulo_de_herdar(t, c.id));
            poe(self.campos.credencial, texto);
        }
    }

    /// Passa a linha "Credencial" para a pasta escolhida e grava. Lê o signal com rastreamento:
    /// serve a um efeito.
    pub(super) fn guardar_credencial_do_grupo(&self) {
        let texto = self.campos.credencial_da_pasta.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let nova = ModoCredencial::do_rotulo(&texto);
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id)
                    && g.credencial_modo != nova
                {
                    g.credencial_modo = nova;
                    mudou = true;
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }
}
