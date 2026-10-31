//! O modo da credencial e a chave privada herdados pela pasta (o agente SSH ou uma chave para
//! tudo o que está dentro, por exemplo). A parte pura está em [`crate::credencial`]; aqui ficam
//! as linhas "Credencial", "Chave do cofre" e "Chave privada" das propriedades da pasta e a
//! opção "Herdar" nas linhas da conexão.

use tomni_ui::prelude::*;

use super::{
    CofreCompartilhado, Tela, caminho_do_texto, opcoes_de_chave_do_cofre, poe,
    rotulo_da_chave_do_cofre, texto_do_caminho, uid_da_chave_do_cofre,
};
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

/// As opções da linha "Chave do cofre" da conexão ou da pasta escolhida: a opção sem chave
/// ("Herdar (pasta)" quando uma pasta de cima tem chave, "Nenhuma" quando não) e as chaves.
pub(super) fn opcoes_de_chave_da_selecao(
    cofre: &CofreCompartilhado,
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let mut opcoes = opcoes_de_chave_do_cofre(cofre);
    opcoes[0] = sem_chave_da_selecao(tree, selecionado);
    opcoes
}

/// A opção sem chave da linha "Chave do cofre" do item escolhido.
pub(super) fn sem_chave_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> String {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| credencial::rotulo_sem_chave(t, id.unwrap_or(0)))
}

/// A opção "Herdar" da linha "Credencial" da conexão escolhida, com o que vale pela pasta.
pub(super) fn herdar_da_selecao(tree: Signal<Tree>, selecionado: Signal<Option<String>>) -> String {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| credencial::rotulo_de_herdar(t, id.unwrap_or(0)))
}

impl Tela {
    /// Mostra nas linhas "Credencial", "Chave do cofre" e "Chave privada" da pasta as escolhas
    /// dela.
    pub(super) fn sincronizar_credencial_do_grupo(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let linhas = self.tree.with_untracked(|t| match t.find(id)? {
            Node::Group(g) => Some((
                credencial::rotulo(t, id, g.credencial_modo),
                match g.chave_do_cofre.as_deref() {
                    Some(uid) => rotulo_da_chave_do_cofre(&self.cofre, Some(uid)),
                    None => credencial::rotulo_sem_chave(t, id),
                },
                texto_do_caminho(g.key_file.as_ref()),
            )),
            Node::Connection(_) => None,
        });
        if let Some((modo, chave_cofre, chave)) = linhas {
            poe(self.campos.credencial_da_pasta, modo);
            poe(self.campos.chave_cofre_da_pasta, chave_cofre);
            poe(self.campos.chave_da_pasta, chave);
        }
    }

    /// Na conexão SSH ou SFTP que herda e não tem item do cofre, a linha "Credencial" mostra a
    /// opção "Herdar" com o modo que vale pela pasta (o rótulo do modo não diz de onde vem).
    /// Na conexão sem chave do cofre, a linha "Chave do cofre" mostra "Herdar (pasta)" quando
    /// uma pasta de cima tem chave.
    pub(super) fn sincronizar_credencial_herdada(&self, c: &Connection) {
        if c.chave_do_cofre.is_none() {
            let texto = self
                .tree
                .with_untracked(|t| credencial::rotulo_sem_chave(t, c.id));
            poe(self.campos.chave_cofre, texto);
        }
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

    /// Passa as linhas "Credencial", "Chave do cofre" e "Chave privada" para a pasta escolhida
    /// e grava. Lê os signals com rastreamento: serve a um efeito.
    pub(super) fn guardar_credencial_do_grupo(&self) {
        let texto = self.campos.credencial_da_pasta.get();
        let chave_cofre = self.campos.chave_cofre_da_pasta.get();
        let chave = caminho_do_texto(&self.campos.chave_da_pasta.get());
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let nova = ModoCredencial::do_rotulo(&texto);
            // Com o cofre bloqueado a lista não tem as chaves: a que a pasta já usa fica.
            let uid = (!self.cofre.agora(|c| c.bloqueado()))
                .then(|| uid_da_chave_do_cofre(&self.cofre, &chave_cofre));
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id) {
                    let uid = uid.unwrap_or_else(|| g.chave_do_cofre.clone());
                    if (g.credencial_modo, &g.chave_do_cofre, &g.key_file) != (nova, &uid, &chave) {
                        g.credencial_modo = nova;
                        g.chave_do_cofre = uid;
                        g.key_file = chave;
                        mudou = true;
                    }
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }
}
