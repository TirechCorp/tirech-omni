//! Como o assistente aponta uma conexão (M51, seção 7): pelo id, pelo caminho na árvore
//! (`"Clientes/ACME/api-prod"`) ou pelo nome sozinho, quando é único. Só as conexões liberadas
//! (a permissão efetiva não é `Desligada`) valem; as outras respondem "não está liberada".

use serde_json::Value;

use super::operacao::caminho_na_arvore;
use super::permissao::{PermissaoIa, permissao_efetiva};
use crate::busca::normalizar;
use crate::modelo::{Connection, Tree};
use crate::t;

/// A conexão que `valor` aponta (um número, ou um texto com o id, o caminho ou o nome).
pub fn achar_conexao<'a>(tree: &'a Tree, valor: &Value) -> Result<&'a Connection, String> {
    let por_id = match valor {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => return Err(t!("mcp-conexao-invalida")),
    };
    if let Some(id) = por_id {
        let conexao = tree
            .connection(id)
            .ok_or_else(|| t!("mcp-conexao-nao-existe", id = id.to_string()))?;
        return liberada(tree, conexao);
    }
    let Value::String(texto) = valor else {
        return Err(t!("mcp-conexao-invalida"));
    };
    let procurado = normalizar(texto.trim().trim_matches('/'));
    if procurado.is_empty() {
        return Err(t!("mcp-conexao-invalida"));
    }
    let todas = tree.connections();
    let mut casam: Vec<&Connection> = todas
        .iter()
        .copied()
        .filter(|c| normalizar(&caminho_na_arvore(tree, c)) == procurado)
        .collect();
    if casam.is_empty() {
        casam = todas
            .iter()
            .copied()
            .filter(|c| normalizar(&c.name) == procurado)
            .collect();
    }
    match casam.as_slice() {
        [] => Err(t!("mcp-conexao-nao-achei", nome = texto.as_str())),
        [uma] => liberada(tree, uma),
        varias => {
            let liberadas: Vec<String> = varias
                .iter()
                .filter(|c| permissao_efetiva(tree, c.id) != PermissaoIa::Desligada)
                .map(|c| caminho_na_arvore(tree, c))
                .collect();
            match liberadas.len() {
                0 => Err(t!("mcp-conexao-nao-liberada")),
                1 => achar_conexao(tree, &Value::String(liberadas[0].clone())),
                _ => Err(t!(
                    "mcp-conexao-ambigua",
                    nome = texto.as_str(),
                    caminhos = liberadas.join(", ")
                )),
            }
        }
    }
}

fn liberada<'a>(tree: &'a Tree, conexao: &'a Connection) -> Result<&'a Connection, String> {
    if permissao_efetiva(tree, conexao.id) == PermissaoIa::Desligada {
        Err(t!("mcp-conexao-nao-liberada"))
    } else {
        Ok(conexao)
    }
}

#[cfg(test)]
mod testes {
    use serde_json::json;

    use super::*;
    use crate::modelo::Kind;

    fn arvore() -> (Tree, u64, u64, u64) {
        let mut tree = Tree::default();
        let acme = tree.add_group(None, "ACME");
        let beta = tree.add_group(None, "Beta");
        let mut a = Connection::new("api", Kind::Ssh);
        a.mcp = PermissaoIa::Livre;
        let a = tree.add_connection(Some(acme), a);
        let mut b = Connection::new("api", Kind::Ssh);
        b.mcp = PermissaoIa::SoLer;
        let b = tree.add_connection(Some(beta), b);
        let c = tree.add_connection(None, Connection::new("fechada", Kind::Ssh));
        (tree, a, b, c)
    }

    #[test]
    fn pelo_id_pelo_caminho_e_pelo_nome_unico() {
        let (tree, a, b, _) = arvore();
        assert_eq!(achar_conexao(&tree, &json!(a)).unwrap().id, a);
        assert_eq!(achar_conexao(&tree, &json!(b.to_string())).unwrap().id, b);
        assert_eq!(achar_conexao(&tree, &json!("beta/API")).unwrap().id, b);
    }

    #[test]
    fn nome_repetido_lista_os_caminhos() {
        let (tree, ..) = arvore();
        let erro = achar_conexao(&tree, &json!("api")).unwrap_err();
        assert!(
            erro.contains("ACME/api") && erro.contains("Beta/api"),
            "{erro}"
        );
    }

    #[test]
    fn conexao_nao_liberada_e_inexistente() {
        let (tree, _, _, c) = arvore();
        assert!(achar_conexao(&tree, &json!(c)).is_err());
        assert!(achar_conexao(&tree, &json!("fechada")).is_err());
        assert!(achar_conexao(&tree, &json!(9999)).is_err());
        assert!(achar_conexao(&tree, &json!("nada")).is_err());
        assert!(achar_conexao(&tree, &json!(true)).is_err());
    }
}
