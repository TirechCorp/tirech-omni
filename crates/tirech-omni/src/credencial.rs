//! A herança do modo da credencial pela pasta: uma pasta pode dizer que as conexões SSH e SFTP
//! de dentro entram pelo agente SSH (ou perguntam a senha, ou tentam o agente e depois a senha),
//! e cada conexão que deixou "Herdar" segue a pasta mais próxima que decidiu. A chave privada
//! também: a conexão sem chave própria (nem do cofre, nem em arquivo) usa a da pasta mais
//! próxima que tem uma. A parte pura; as linhas das propriedades moram em `tela/credencial.rs`.

use crate::modelo::{Group, ModoCredencial, Node, Tree};
use crate::t;

/// O que uma escolha decide: `None` segue subindo.
fn decide(modo: ModoCredencial) -> Option<ModoCredencial> {
    (!modo.is_herdar()).then_some(modo)
}

/// A pasta que decide o modo do item `no`, subindo a partir da de cima, e o modo que ela
/// escolhe. `None` se nenhuma pasta decide.
pub fn herdado(tree: &Tree, no: u64) -> Option<(&str, ModoCredencial)> {
    for grupo in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(grupo)
            && let Some(modo) = decide(g.credencial_modo)
        {
            return Some((&g.name, modo));
        }
    }
    None
}

/// A pasta mais próxima do item `no`, subindo a partir da de cima, que tem uma chave privada
/// (do cofre ou em arquivo). `None` se nenhuma tem.
pub fn chave_herdada(tree: &Tree, no: u64) -> Option<&Group> {
    tree.ancestors(no)
        .into_iter()
        .rev()
        .find_map(|grupo| match tree.find(grupo) {
            Some(Node::Group(g)) if g.key_file.is_some() || g.chave_do_cofre.is_some() => Some(g),
            _ => None,
        })
}

/// O texto da opção sem chave da linha "Chave do cofre" do item `no`: "Herdar (pasta)" quando
/// uma pasta de cima tem chave, "Nenhuma" quando não.
pub fn rotulo_sem_chave(tree: &Tree, no: u64) -> String {
    match chave_herdada(tree, no) {
        Some(g) => t!("comum-credencial-herdar-de", nome = g.name.as_str()),
        None => t!("comum-nenhuma"),
    }
}

/// O modo que vale na conexão `conexao`: o dela ou, com "Herdar", o da pasta; sem nenhuma pasta
/// que decida, `Perguntar`. O item do cofre ainda vence (ver
/// [`crate::modelo::Connection::modo_da_credencial`]).
pub fn modo_efetivo(tree: &Tree, conexao: u64) -> ModoCredencial {
    tree.connection(conexao)
        .and_then(|c| decide(c.credencial_modo))
        .or_else(|| herdado(tree, conexao).map(|(_, modo)| modo))
        .unwrap_or(ModoCredencial::Perguntar)
}

/// O texto da opção "Herdar" do item `no`: diz de que pasta vem, como a linha "Rede" (o modo
/// não cabe na coluna; a linha da pasta mostra o dela).
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    match herdado(tree, no) {
        Some((nome, _)) => t!("comum-credencial-herdar-de", nome = nome),
        None => t!("comum-credencial-herdar"),
    }
}

/// O texto da linha da pasta para `modo` no item `no`.
pub fn rotulo(tree: &Tree, no: u64, modo: ModoCredencial) -> String {
    match decide(modo) {
        None => rotulo_de_herdar(tree, no),
        Some(modo) => modo.nome(),
    }
}

/// As opções da linha da pasta `no`: "Herdar" e os modos.
pub fn opcoes(tree: &Tree, no: u64) -> Vec<String> {
    let mut opcoes = vec![rotulo_de_herdar(tree, no)];
    opcoes.extend(ModoCredencial::TODOS.map(ModoCredencial::nome));
    opcoes
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::{Connection, Kind};

    fn pasta_com(tree: &mut Tree, pai: Option<u64>, nome: &str, modo: ModoCredencial) -> u64 {
        let id = tree.add_group(pai, nome);
        if let Some(Node::Group(g)) = tree.find_mut(id) {
            g.credencial_modo = modo;
        }
        id
    }

    #[test]
    fn a_conexao_que_herda_segue_a_pasta_mais_proxima_que_decide() {
        let mut tree = Tree::default();
        let cliente = pasta_com(&mut tree, None, "Cliente", ModoCredencial::Agente);
        let meio = pasta_com(&mut tree, Some(cliente), "Prod", ModoCredencial::Herdar);
        let id = tree.add_connection(Some(meio), Connection::new("web", Kind::Ssh));
        assert_eq!(modo_efetivo(&tree, id), ModoCredencial::Agente);
        assert_eq!(
            herdado(&tree, id),
            Some(("Cliente", ModoCredencial::Agente))
        );
        let aberta = tree.conexao_para_abrir(id).unwrap();
        assert_eq!(aberta.modo_da_credencial(), ModoCredencial::Agente);

        let perto = pasta_com(&mut tree, Some(meio), "DB", ModoCredencial::AgenteOuSenha);
        let db = tree.add_connection(Some(perto), Connection::new("db", Kind::Sftp));
        assert_eq!(modo_efetivo(&tree, db), ModoCredencial::AgenteOuSenha);
    }

    #[test]
    fn a_escolha_da_conexao_vence_a_pasta_e_o_item_do_cofre_vence_tudo() {
        let mut tree = Tree::default();
        let cliente = pasta_com(&mut tree, None, "Cliente", ModoCredencial::Agente);
        let mut c = Connection::new("web", Kind::Ssh);
        c.credencial_modo = ModoCredencial::Perguntar;
        let id = tree.add_connection(Some(cliente), c);
        assert_eq!(modo_efetivo(&tree, id), ModoCredencial::Perguntar);

        let mut com_item = Connection::new("cofre", Kind::Ssh);
        com_item.credencial = Some(7);
        let id = tree.add_connection(Some(cliente), com_item);
        let aberta = tree.conexao_para_abrir(id).unwrap();
        assert_eq!(aberta.modo_da_credencial(), ModoCredencial::Perguntar);
    }

    #[test]
    fn sem_pasta_que_decida_pergunta_e_so_o_ssh_usa_o_modo() {
        let mut tree = Tree::default();
        let id = tree.add_connection(None, Connection::new("web", Kind::Ssh));
        assert_eq!(modo_efetivo(&tree, id), ModoCredencial::Perguntar);
        assert_eq!(herdado(&tree, id), None);

        let cliente = pasta_com(&mut tree, None, "Cliente", ModoCredencial::Agente);
        let ftp = tree.add_connection(Some(cliente), Connection::new("ftp", Kind::Ftp));
        let aberta = tree.conexao_para_abrir(ftp).unwrap();
        assert_eq!(aberta.modo_da_credencial(), ModoCredencial::Perguntar);
    }

    #[test]
    fn a_conexao_sem_chave_usa_a_da_pasta_mais_proxima_que_tem_uma() {
        let mut tree = Tree::default();
        let cliente = pasta_com(&mut tree, None, "Cliente", ModoCredencial::Herdar);
        let prod = pasta_com(&mut tree, Some(cliente), "Prod", ModoCredencial::Herdar);
        if let Some(Node::Group(g)) = tree.find_mut(cliente) {
            g.key_file = Some("/chaves/cliente".into());
        }
        let web = tree.add_connection(Some(prod), Connection::new("web", Kind::Ssh));
        let aberta = tree.conexao_para_abrir(web).unwrap();
        assert_eq!(aberta.key_file.as_deref(), Some("/chaves/cliente".as_ref()));
        assert_eq!(aberta.chave_do_cofre, None);
        assert!(rotulo_sem_chave(&tree, web).contains("Cliente"));
        // Na conexão gravada, nada muda: a herança só vale ao abrir.
        assert_eq!(tree.connection(web).unwrap().key_file, None);

        // A pasta mais perto ganha, com a chave do cofre dela.
        if let Some(Node::Group(g)) = tree.find_mut(prod) {
            g.chave_do_cofre = Some("uid-prod".into());
        }
        let aberta = tree.conexao_para_abrir(web).unwrap();
        assert_eq!(aberta.chave_do_cofre.as_deref(), Some("uid-prod"));
        assert_eq!(aberta.key_file, None);

        // A chave da própria conexão vence a da pasta.
        let mut propria = Connection::new("db", Kind::Ssh);
        propria.key_file = Some("/chaves/db".into());
        let db = tree.add_connection(Some(prod), propria);
        let aberta = tree.conexao_para_abrir(db).unwrap();
        assert_eq!(aberta.key_file.as_deref(), Some("/chaves/db".as_ref()));
        assert_eq!(aberta.chave_do_cofre, None);

        let solta = tree.add_connection(None, Connection::new("solta", Kind::Ssh));
        assert!(chave_herdada(&tree, solta).is_none());
        assert_eq!(rotulo_sem_chave(&tree, solta), t!("comum-nenhuma"));
    }

    #[test]
    fn o_rotulo_de_herdar_diz_a_pasta() {
        let mut tree = Tree::default();
        let cliente = pasta_com(&mut tree, None, "Cliente", ModoCredencial::Agente);
        let id = tree.add_connection(Some(cliente), Connection::new("web", Kind::Ssh));
        let herdar = rotulo_de_herdar(&tree, id);
        assert!(herdar.contains("Cliente"), "{herdar}");
        assert_eq!(opcoes(&tree, id).len(), 4);
        assert_eq!(
            rotulo(&tree, cliente, ModoCredencial::Agente),
            ModoCredencial::Agente.nome()
        );
    }
}
