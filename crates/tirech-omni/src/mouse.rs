//! O mouse no terminal de cada conexão: copiar ao selecionar e colar com o botão direito, ou
//! abrir o menu. A conexão escolhe, ou herda da pasta mais perto que escolheu; sem
//! nenhuma, valem as duas linhas de Configurações › Terminal (Copiar ao selecionar e Botão
//! direito). A parte pura: a herança e os textos da linha. O que liga à tela mora em
//! `tela/mouse.rs`.

use crate::configuracoes::BotaoDireito;
use crate::modelo::{MouseDoTerminal, Node, Tree};
use crate::t;

/// O que uma escolha decide: `None` segue subindo.
fn decide(mouse: MouseDoTerminal) -> Option<MouseDoTerminal> {
    (!mouse.is_herdar()).then_some(mouse)
}

/// A pasta que decide o mouse do item `no`, subindo a partir da de cima, e a escolha dela.
/// `None` se nenhuma pasta decide.
pub fn herdada(tree: &Tree, no: u64) -> Option<(&str, MouseDoTerminal)> {
    for grupo in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(grupo)
            && let Some(mouse) = decide(g.mouse)
        {
            return Some((&g.name, mouse));
        }
    }
    None
}

/// O mouse do terminal da conexão `conexao`: o dela ou o da pasta. `None` segue as
/// Configurações (também para uma conexão que não existe).
pub fn da_conexao(tree: &Tree, conexao: u64) -> Option<MouseDoTerminal> {
    let c = tree.connection(conexao)?;
    decide(c.mouse).or_else(|| herdada(tree, conexao).map(|(_, mouse)| mouse))
}

/// Se selecionar copia, com a escolha `mouse` (`None`: o `padrao` das Configurações).
pub fn copiar_ao_selecionar(mouse: Option<MouseDoTerminal>, padrao: bool) -> bool {
    match mouse {
        Some(MouseDoTerminal::CopiarColar) => true,
        Some(MouseDoTerminal::Menu) => false,
        Some(MouseDoTerminal::Herdar) | None => padrao,
    }
}

/// O que o botão direito faz, com a escolha `mouse` (`None`: o `padrao` das Configurações).
pub fn botao_direito(mouse: Option<MouseDoTerminal>, padrao: BotaoDireito) -> BotaoDireito {
    match mouse {
        Some(MouseDoTerminal::CopiarColar) => BotaoDireito::CopiarColar,
        Some(MouseDoTerminal::Menu) => BotaoDireito::Menu,
        Some(MouseDoTerminal::Herdar) | None => padrao,
    }
}

/// A opção "Copiar e colar" da linha.
pub fn copiar_colar() -> String {
    t!("mouse-copiar-colar")
}

/// A opção "Menu" da linha.
pub fn menu() -> String {
    t!("mouse-menu")
}

/// O texto de uma escolha que vale.
fn rotulo_da_escolha(mouse: MouseDoTerminal) -> String {
    match mouse {
        MouseDoTerminal::CopiarColar => copiar_colar(),
        MouseDoTerminal::Menu => menu(),
        MouseDoTerminal::Herdar => t!("mouse-herdar-configuracoes"),
    }
}

/// O texto da opção "Herdar" do item `no`: diz de onde vem (a escolha junto não cabe na coluna,
/// como na linha Credencial).
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    match herdada(tree, no) {
        Some((nome, _)) => t!("mouse-herdar-de", nome = nome),
        None => t!("mouse-herdar-configuracoes"),
    }
}

/// O texto da linha para `mouse` no item `no`.
pub fn rotulo(tree: &Tree, no: u64, mouse: MouseDoTerminal) -> String {
    match decide(mouse) {
        None => rotulo_de_herdar(tree, no),
        Some(mouse) => rotulo_da_escolha(mouse),
    }
}

/// As opções da linha do item `no`.
pub fn opcoes(tree: &Tree, no: u64) -> Vec<String> {
    vec![rotulo_de_herdar(tree, no), copiar_colar(), menu()]
}

/// A escolha que o texto da linha faz; um texto que não é de nenhuma opção é "Herdar".
pub fn do_rotulo(texto: &str) -> MouseDoTerminal {
    if texto == copiar_colar() {
        MouseDoTerminal::CopiarColar
    } else if texto == menu() {
        MouseDoTerminal::Menu
    } else {
        MouseDoTerminal::Herdar
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::{Connection, Kind};

    fn arvore() -> (Tree, u64, u64, u64) {
        let mut tree = Tree::default();
        let pasta = tree.add_group(None, "Clientes");
        let dentro = tree.add_group(Some(pasta), "ACME");
        let id = tree.add_connection(Some(dentro), Connection::new("web", Kind::Ssh));
        (tree, pasta, dentro, id)
    }

    fn mudar(tree: &mut Tree, id: u64, mouse: MouseDoTerminal) {
        match tree.find_mut(id) {
            Some(Node::Group(g)) => g.mouse = mouse,
            Some(Node::Connection(c)) => c.mouse = mouse,
            None => panic!("o item {id} não existe"),
        }
    }

    #[test]
    fn herda_da_pasta_mais_perto_que_decide() {
        let (mut tree, pasta, dentro, id) = arvore();
        assert_eq!(
            da_conexao(&tree, id),
            None,
            "sem escolha, valem as Configurações"
        );
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Configurações)");

        mudar(&mut tree, pasta, MouseDoTerminal::CopiarColar);
        assert_eq!(da_conexao(&tree, id), Some(MouseDoTerminal::CopiarColar));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Clientes)");

        mudar(&mut tree, dentro, MouseDoTerminal::Menu);
        assert_eq!(da_conexao(&tree, id), Some(MouseDoTerminal::Menu));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (ACME)");
        // A pasta mostra de onde ela herda, sem contar a si mesma.
        assert_eq!(
            rotulo(&tree, dentro, MouseDoTerminal::Herdar),
            "Herdar (Clientes)"
        );

        mudar(&mut tree, id, MouseDoTerminal::CopiarColar);
        assert_eq!(da_conexao(&tree, id), Some(MouseDoTerminal::CopiarColar));
        assert_eq!(da_conexao(&tree, 999), None);
    }

    #[test]
    fn a_escolha_vence_as_configuracoes_e_herdar_segue_elas() {
        for padrao in [false, true] {
            assert_eq!(copiar_ao_selecionar(None, padrao), padrao);
            assert!(copiar_ao_selecionar(
                Some(MouseDoTerminal::CopiarColar),
                padrao
            ));
            assert!(!copiar_ao_selecionar(Some(MouseDoTerminal::Menu), padrao));
        }
        for &padrao in BotaoDireito::TODAS {
            assert_eq!(botao_direito(None, padrao), padrao);
            assert_eq!(
                botao_direito(Some(MouseDoTerminal::CopiarColar), padrao),
                BotaoDireito::CopiarColar
            );
            assert_eq!(
                botao_direito(Some(MouseDoTerminal::Menu), padrao),
                BotaoDireito::Menu
            );
        }
    }

    #[test]
    fn os_rotulos_voltam_a_escolha() {
        let (tree, _, _, id) = arvore();
        let opcoes = opcoes(&tree, id);
        assert_eq!(do_rotulo(&opcoes[0]), MouseDoTerminal::Herdar);
        assert_eq!(do_rotulo(&opcoes[1]), MouseDoTerminal::CopiarColar);
        assert_eq!(do_rotulo(&opcoes[2]), MouseDoTerminal::Menu);
        assert_eq!(do_rotulo("qualquer"), MouseDoTerminal::Herdar);
        for escolha in [MouseDoTerminal::CopiarColar, MouseDoTerminal::Menu] {
            assert_eq!(do_rotulo(&rotulo(&tree, id, escolha)), escolha);
        }
    }
}
