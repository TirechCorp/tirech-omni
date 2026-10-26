//! A permissão de assistentes (M51) nas propriedades: a linha "Assistentes (MCP)" da conexão e
//! da pasta, no mesmo molde da linha "Rede" ([`super::redes`]). Os rótulos das cinco opções e
//! a leitura do texto de volta ficam aqui; a regra de quem vale está em [`crate::mcp::permissao`].

use std::rc::Rc;

use tomni_ui::prelude::*;

use super::{Tela, poe};
use crate::mcp::permissao::{PermissaoIa, permissao_herdada};
use crate::modelo::{Node, Tree};
use crate::t;

/// Os quatro valores que a pessoa escolhe (sem o `Herdar`), na ordem do menu.
pub(crate) const ESCOLHAS: [PermissaoIa; 4] = [
    PermissaoIa::Desligada,
    PermissaoIa::SoLer,
    PermissaoIa::Aprovar,
    PermissaoIa::Livre,
];

/// O nome curto da permissão, nos menus, nas dicas e nas linhas das propriedades.
pub(crate) fn nome_da_permissao(permissao: PermissaoIa) -> String {
    match permissao {
        PermissaoIa::Herdar => t!("mcp-perm-herdar"),
        PermissaoIa::Desligada => t!("mcp-perm-desligada"),
        PermissaoIa::SoLer => t!("mcp-perm-so-ler"),
        PermissaoIa::Aprovar => t!("mcp-perm-aprovar"),
        PermissaoIa::Livre => t!("mcp-perm-livre"),
    }
}

/// O texto da opção `Herdar` do nó `no`: "Herdar (Só ler)".
pub(crate) fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    t!(
        "mcp-perm-herdar-de",
        permissao = nome_da_permissao(permissao_herdada(tree, no))
    )
}

/// O texto da linha para a permissão `atual` do nó `no`.
pub(crate) fn rotulo_da_permissao(tree: &Tree, no: u64, atual: PermissaoIa) -> String {
    match atual {
        PermissaoIa::Herdar => rotulo_de_herdar(tree, no),
        outra => nome_da_permissao(outra),
    }
}

/// As opções da linha para o nó `no` (`Herdar` e as quatro escolhas).
pub(crate) fn opcoes(tree: &Tree, no: u64) -> Vec<String> {
    std::iter::once(rotulo_de_herdar(tree, no))
        .chain(ESCOLHAS.map(nome_da_permissao))
        .collect()
}

/// As opções da linha do nó escolhido na árvore (só as quatro escolhas e um `Herdar` sem pasta
/// se não há nó).
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => opcoes(t, id),
        _ => opcoes(t, 0),
    })
}

/// A permissão que o texto da linha diz para o nó `no`, que tinha `atual`. O texto que é o
/// rótulo de `atual` não muda nada.
pub(crate) fn permissao_do_texto(
    tree: &Tree,
    no: u64,
    atual: PermissaoIa,
    texto: &str,
) -> PermissaoIa {
    if texto == rotulo_da_permissao(tree, no, atual) {
        return atual;
    }
    if texto == rotulo_de_herdar(tree, no) {
        return PermissaoIa::Herdar;
    }
    ESCOLHAS
        .into_iter()
        .find(|&p| nome_da_permissao(p) == texto)
        .unwrap_or(atual)
}

impl Tela {
    /// A permissão que o texto da linha diz para o nó `id`, que tinha `atual`.
    pub(super) fn mcp_do_texto(&self, id: u64, atual: PermissaoIa, texto: &str) -> PermissaoIa {
        self.tree
            .with_untracked(|t| permissao_do_texto(t, id, atual, texto))
    }

    /// Mostra na linha "Assistentes (MCP)" a permissão do nó escolhido (conexão ou pasta).
    pub(super) fn sincronizar_mcp(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| {
            let atual = match t.find(id)? {
                Node::Connection(c) => c.mcp,
                Node::Group(g) => g.mcp,
            };
            Some(rotulo_da_permissao(t, id, atual))
        });
        if let Some(texto) = texto {
            poe(self.campos.mcp, texto);
        }
    }

    /// Passa a linha "Assistentes (MCP)" para a pasta escolhida, grava e refaz os ícones da
    /// árvore. Lê o signal com rastreamento: serve a um efeito.
    pub(super) fn guardar_mcp_do_grupo(&self) {
        let texto = self.campos.mcp.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let atual = self.tree.with_untracked(|t| match t.find(id) {
                Some(Node::Group(g)) => Some(g.mcp),
                _ => None,
            });
            let Some(atual) = atual else {
                return;
            };
            let nova = self.mcp_do_texto(id, atual, &texto);
            if nova == atual {
                return;
            }
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id) {
                    g.mcp = nova;
                }
            });
            self.atualizar();
        });
    }

    /// Muda a permissão do nó `id` (conexão ou pasta) pelo menu da árvore. Se o nó é o que as
    /// propriedades mostram, a linha acompanha.
    pub(super) fn mudar_permissao_mcp(&self, id: u64, nova: PermissaoIa) {
        let mut mudou = false;
        self.tree.update(|t| match t.find_mut(id) {
            Some(Node::Connection(c)) if c.mcp != nova => {
                c.mcp = nova;
                mudou = true;
            }
            Some(Node::Group(g)) if g.mcp != nova => {
                g.mcp = nova;
                mudou = true;
            }
            _ => {}
        });
        if mudou {
            self.atualizar();
            self.sincronizar_mcp();
        }
    }
}

/// O submenu "Assistentes (MCP)" do menu de contexto da árvore: "Herdar (…)" e as
/// quatro escolhas, com o visto na que vale para o nó `id`. Os ids são
/// `menu-{chave}-mcp-{herdar|desligada|so-ler|aprovar|livre}`.
pub(super) fn menu_da_arvore(tela: &Rc<Tela>, chave: &str, id: u64) -> MenuFlyout {
    let (atual, herdar) = tela.tree.with_untracked(|t| {
        let atual = match t.find(id) {
            Some(Node::Connection(c)) => c.mcp,
            Some(Node::Group(g)) => g.mcp,
            None => PermissaoIa::Herdar,
        };
        (atual, rotulo_de_herdar(t, id))
    });
    let mut menu = MenuFlyout::new().id(format!("menu-{chave}-mcp-menu"));
    let linhas = [
        (PermissaoIa::Herdar, "herdar", herdar),
        (PermissaoIa::Desligada, "desligada", String::new()),
        (PermissaoIa::SoLer, "so-ler", String::new()),
        (PermissaoIa::Aprovar, "aprovar", String::new()),
        (PermissaoIa::Livre, "livre", String::new()),
    ];
    for (permissao, sufixo, rotulo) in linhas {
        let rotulo = if rotulo.is_empty() {
            nome_da_permissao(permissao)
        } else {
            rotulo
        };
        let t = Rc::clone(tela);
        menu = menu.item(format!("menu-{chave}-mcp-{sufixo}"), rotulo, move || {
            t.mudar_permissao_mcp(id, permissao)
        });
        if permissao == atual {
            menu = menu.icon(Symbol::Checkmark);
        }
    }
    menu
}
