//! O que cada ferramenta de operação faz, nas partes que não dependem da interface (M51).
//! Aqui está o `listar_conexoes`; as demais voltam "ainda não implementada" até chegarem.

use serde_json::{Value, json};
use tomni_ui::mcp::{error_result, text_result};

use super::ferramentas;
use super::permissao::{PermissaoIa, permissao_efetiva};
use crate::busca::normalizar;
use crate::campos::Campo;
use crate::modelo::{Connection, Kind, Node, Tree};
use crate::t;

/// O que fazer com um pedido de ferramenta que a interface recebeu.
#[derive(Clone, Debug, PartialEq)]
pub enum Resposta {
    /// O `result` do MCP (texto ou erro de ferramenta).
    Resultado(Value),
    /// Um erro JSON-RPC: o código e a mensagem.
    Recusa(i64, String),
}

/// O que a ferramenta `nome` devolve, sabendo só da árvore de conexões e de quantas abas cada
/// conexão tem. As que precisam da interface (abas, terminal) são tratadas por quem chama.
pub fn atender(
    tree: &Tree,
    abas_da_conexao: &dyn Fn(u64) -> usize,
    nome: &str,
    args: &Value,
) -> Resposta {
    match nome {
        "listar_conexoes" => Resposta::Resultado(listar_conexoes(tree, abas_da_conexao, args)),
        outra if ferramentas::nomes(None).iter().any(|n| n == outra) => {
            Resposta::Resultado(error_result(&t!("mcp-ferramenta-pendente", nome = outra)))
        }
        outra => Resposta::Recusa(
            -32602,
            t!("mcp-ponte-ferramenta-desconhecida", nome = outra),
        ),
    }
}

/// O texto da permissão como o assistente a lê.
pub fn nome_da_permissao(permissao: PermissaoIa) -> &'static str {
    match permissao {
        PermissaoIa::Herdar | PermissaoIa::Desligada => "desligada",
        PermissaoIa::SoLer => "so_ler",
        PermissaoIa::Aprovar => "aprovar",
        PermissaoIa::Livre => "livre",
    }
}

/// O caminho de uma conexão na árvore: as pastas e o nome, separados por `/`.
pub fn caminho_na_arvore(tree: &Tree, conexao: &Connection) -> String {
    let mut partes: Vec<String> = tree
        .ancestors(conexao.id)
        .into_iter()
        .filter_map(|id| match tree.find(id) {
            Some(Node::Group(g)) => Some(g.name.clone()),
            _ => None,
        })
        .collect();
    partes.push(conexao.name.clone());
    partes.join("/")
}

/// `listar_conexoes`: as conexões liberadas para assistentes (as `Desligada` não aparecem), com
/// o filtro opcional. O resultado é um texto com a lista em JSON.
fn listar_conexoes(tree: &Tree, abas_da_conexao: &dyn Fn(u64) -> usize, args: &Value) -> Value {
    let filtro = args["filtro"].as_str().map(normalizar).unwrap_or_default();
    let mut linhas = Vec::new();
    for conexao in tree.connections() {
        let permissao = permissao_efetiva(tree, conexao.id);
        if permissao == PermissaoIa::Desligada {
            continue;
        }
        let caminho = caminho_na_arvore(tree, conexao);
        // Só entram os campos que valem para o tipo (`campos::Campo`); o usuário do S3 é o
        // Access Key ID e nunca sai.
        let host = Campo::Host.vale(conexao.kind).then_some(&conexao.host);
        let porta = Campo::Porta.vale(conexao.kind).then_some(conexao.port);
        let usuario = (Campo::Usuario.vale(conexao.kind) && conexao.kind != Kind::S3)
            .then_some(&conexao.user);
        let procurado = normalizar(&format!(
            "{caminho} {} {} {}",
            conexao.name,
            host.map_or("", |h| h.as_str()),
            usuario.map_or("", |u| u.as_str())
        ));
        if !filtro.is_empty() && !procurado.contains(&filtro) {
            continue;
        }
        let tipo = serde_json::to_value(conexao.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_lowercase))
            .unwrap_or_default();
        let mut linha = json!({
            "id": conexao.id,
            "caminho": caminho,
            "nome": conexao.name,
            "tipo": tipo,
            "permissao": nome_da_permissao(permissao),
            "abas": abas_da_conexao(conexao.id),
        });
        if let Some(campos) = linha.as_object_mut() {
            if let Some(host) = host {
                campos.insert("host".to_owned(), json!(host));
            }
            if let Some(porta) = porta {
                campos.insert("porta".to_owned(), json!(porta));
            }
            if let Some(usuario) = usuario {
                campos.insert("usuario".to_owned(), json!(usuario));
            }
        }
        linhas.push(linha);
    }
    text_result(&serde_json::to_string_pretty(&linhas).unwrap_or_else(|_| "[]".to_owned()))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn arvore() -> Tree {
        let mut tree = Tree::default();
        let pasta = tree.add_group(None, "Clientes");
        let mut a = Connection::new("api-prod", Kind::Ssh);
        a.host = "api.exemplo".into();
        a.user = "ana".into();
        a.mcp = PermissaoIa::Livre;
        tree.add_connection(Some(pasta), a);
        let mut b = Connection::new("banco", Kind::Ssh);
        b.host = "db.exemplo".into();
        b.mcp = PermissaoIa::SoLer;
        tree.add_connection(Some(pasta), b);
        // Sem permissão (herda a raiz, que é desligada): não aparece.
        tree.add_connection(None, Connection::new("segredo", Kind::Ssh));
        tree
    }

    fn lista(tree: &Tree, args: &Value) -> Vec<Value> {
        let r = listar_conexoes(tree, &|_| 2, args);
        assert_eq!(r["isError"], false);
        serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap()
    }

    #[test]
    fn so_aparecem_as_conexoes_liberadas() {
        let itens = lista(&arvore(), &json!({}));
        let nomes: Vec<_> = itens.iter().map(|i| i["nome"].as_str().unwrap()).collect();
        assert_eq!(nomes, ["api-prod", "banco"]);
        assert_eq!(itens[0]["caminho"], "Clientes/api-prod");
        assert_eq!(itens[0]["permissao"], "livre");
        assert_eq!(itens[1]["permissao"], "so_ler");
        assert_eq!(itens[0]["abas"], 2);
        assert_eq!(itens[0]["tipo"], "ssh");
    }

    #[test]
    fn o_filtro_nao_diferencia_maiusculas_nem_acentos() {
        let itens = lista(&arvore(), &json!({ "filtro": "CLIENTES/API" }));
        assert_eq!(itens.len(), 1);
        let itens = lista(&arvore(), &json!({ "filtro": "db.exe" }));
        assert_eq!(itens[0]["nome"], "banco");
        assert!(lista(&arvore(), &json!({ "filtro": "nada" })).is_empty());
    }

    #[test]
    fn a_lista_so_traz_host_porta_e_usuario_dos_tipos_que_os_usam() {
        let mut tree = Tree::default();
        let mut ssh = Connection::new("web", Kind::Ssh);
        ssh.host = "web.exemplo".into();
        ssh.user = "ana".into();
        ssh.mcp = PermissaoIa::Livre;
        tree.add_connection(None, ssh);
        let mut local = Connection::new("shell", Kind::Local);
        local.host = "antigo.exemplo".into();
        local.user = "ana".into();
        local.mcp = PermissaoIa::Livre;
        tree.add_connection(None, local);
        let mut s3 = Connection::new("nuvem", Kind::S3);
        s3.user = "AKIAEXEMPLO".into();
        s3.mcp = PermissaoIa::Livre;
        tree.add_connection(None, s3);

        let itens = lista(&tree, &json!({}));
        assert_eq!(itens.len(), 3);
        assert_eq!(itens[0]["host"], "web.exemplo");
        assert_eq!(itens[0]["porta"], 22);
        assert_eq!(itens[0]["usuario"], "ana");
        for chave in ["host", "porta", "usuario"] {
            assert!(
                itens[1].get(chave).is_none(),
                "o shell local não tem {chave}"
            );
        }
        for chave in ["host", "porta", "usuario"] {
            assert!(itens[2].get(chave).is_none(), "o S3 não lista {chave}");
        }
        // O filtro procura nos mesmos campos que saem.
        assert!(lista(&tree, &json!({ "filtro": "akia" })).is_empty());
        assert!(lista(&tree, &json!({ "filtro": "antigo" })).is_empty());
        assert_eq!(lista(&tree, &json!({ "filtro": "ana" })).len(), 1);
    }

    #[test]
    fn ferramenta_pendente_e_desconhecida() {
        let tree = arvore();
        match atender(&tree, &|_| 0, "ler", &json!({})) {
            Resposta::Resultado(r) => assert_eq!(r["isError"], true),
            outra => panic!("{outra:?}"),
        }
        assert!(matches!(
            atender(&tree, &|_| 0, "inventada", &json!({})),
            Resposta::Recusa(-32602, _)
        ));
    }
}
