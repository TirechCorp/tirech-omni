//! A linha "Abrir abas novas" das propriedades da conexão e da pasta: se um assistente pode
//! abrir uma aba nova pelo MCP (Permitir, Perguntar ou Bloquear), herdando da pasta e, no fim,
//! de Configurações › MCP. Fica logo abaixo da linha "Assistentes (MCP)"
//! ([`super::mcp_propriedades`]); a regra está em [`crate::mcp::permissao`].

use tomni_ui::prelude::*;

use super::{Tela, poe};
use crate::configuracoes::{AbrirAbaIa, Configuracoes};
use crate::mcp::permissao::abrir_aba_herdada;
use crate::modelo::{Node, Tree};
use crate::t;

/// O texto da opção "Herdar" do nó `no`, com a escolha que vem de cima: "Herdar (Perguntar)".
fn rotulo_de_herdar(tree: &Tree, no: u64, padrao: AbrirAbaIa) -> String {
    t!(
        "mcp-abrir-herdar-de",
        escolha = abrir_aba_herdada(tree, no, padrao).nome()
    )
}

/// O texto da linha para a escolha `atual` do nó `no`.
fn rotulo(tree: &Tree, no: u64, atual: Option<AbrirAbaIa>, padrao: AbrirAbaIa) -> String {
    match atual {
        Some(escolha) => escolha.nome(),
        None => rotulo_de_herdar(tree, no, padrao),
    }
}

/// A escolha que o texto da linha faz; um texto que não é de nenhuma opção é "Herdar".
fn do_rotulo(texto: &str) -> Option<AbrirAbaIa> {
    AbrirAbaIa::do_nome(texto)
}

/// As opções da linha do nó escolhido na árvore: "Herdar (…)" e as três escolhas.
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
    config: Signal<Configuracoes>,
) -> Vec<String> {
    let id = selecionado
        .get()
        .and_then(|k| k.parse::<u64>().ok())
        .unwrap_or(0);
    let padrao = config.with(|c| c.ia.mcp_abrir);
    let herdar = tree.with(|t| rotulo_de_herdar(t, id, padrao));
    std::iter::once(herdar).chain(AbrirAbaIa::nomes()).collect()
}

impl Tela {
    /// A escolha que a linha diz agora (`None`: herdar).
    pub(super) fn mcp_abrir_da_linha(&self) -> Option<AbrirAbaIa> {
        do_rotulo(&self.campos.mcp_abrir.get())
    }

    /// Mostra na linha "Abrir abas novas" a escolha do nó escolhido (conexão ou pasta).
    pub(super) fn sincronizar_mcp_abrir(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let padrao = self.config.with_untracked(|c| c.ia.mcp_abrir);
        let texto = self.tree.with_untracked(|t| {
            let atual = match t.find(id)? {
                Node::Connection(c) => c.mcp_abrir,
                Node::Group(g) => g.mcp_abrir,
            };
            Some(rotulo(t, id, atual, padrao))
        });
        if let Some(texto) = texto {
            poe(self.campos.mcp_abrir, texto);
        }
    }

    /// Passa a linha "Abrir abas novas" para a pasta escolhida e grava. Lê o signal com
    /// rastreamento: serve a um efeito.
    pub(super) fn guardar_mcp_abrir_do_grupo(&self) {
        let nova = self.mcp_abrir_da_linha();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id)
                    && g.mcp_abrir != nova
                {
                    g.mcp_abrir = nova;
                    mudou = true;
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::{Connection, Kind};

    #[test]
    fn o_rotulo_volta_para_a_mesma_escolha() {
        let mut tree = Tree::default();
        let pasta = tree.add_group(None, "Clientes");
        let c = tree.add_connection(Some(pasta), Connection::new("web", Kind::Ssh));
        if let Some(Node::Group(g)) = tree.find_mut(pasta) {
            g.mcp_abrir = Some(AbrirAbaIa::Bloquear);
        }
        let herdar = rotulo(&tree, c, None, AbrirAbaIa::Permitir);
        assert_eq!(
            herdar,
            t!("mcp-abrir-herdar-de", escolha = AbrirAbaIa::Bloquear.nome())
        );
        assert_eq!(do_rotulo(&herdar), None);
        for escolha in AbrirAbaIa::TODAS.iter().copied() {
            let texto = rotulo(&tree, c, Some(escolha), AbrirAbaIa::Permitir);
            assert_eq!(do_rotulo(&texto), Some(escolha));
        }
        // A pasta sem escolha acima herda das Configurações.
        assert_eq!(
            rotulo(&tree, pasta, None, AbrirAbaIa::Perguntar),
            t!(
                "mcp-abrir-herdar-de",
                escolha = AbrirAbaIa::Perguntar.nome()
            )
        );
    }
}
