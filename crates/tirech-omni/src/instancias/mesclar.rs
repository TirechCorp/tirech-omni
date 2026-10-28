//! A junção de três vias de um JSON da configuração: o que a cópia secundária mudou (`nosso`,
//! desde a `base` que ela leu) vai para cima do que está no disco agora (`deles`, o que a
//! principal gravou). Desce nos objetos campo a campo e nas listas item a item, pelo `id` (ou
//! `Variante:id`, como os `Node` da árvore). Onde os dois mudaram a mesma coisa de jeitos
//! diferentes, fica a de `deles` (a principal ganha) e o caminho entra nos conflitos.

use std::collections::HashMap;

use serde_json::{Map, Value};

use crate::armazenamento::identidade;

/// O resultado de [`mesclar`].
#[derive(Clone, Debug, PartialEq)]
pub struct Mescla {
    /// O JSON junto.
    pub valor: Value,
    /// Onde os dois lados mudaram a mesma coisa (ficou a de `deles`), como caminhos legíveis:
    /// `geral › tema`, `Produção › web-01 › host`.
    pub conflitos: Vec<String>,
}

/// Junta `nosso` e `deles`, os dois feitos a partir de `base`. Um item novo dos dois lados com o
/// mesmo `id` e conteúdo diferente fica duas vezes, o de `deles` primeiro: quem chama dá outro id
/// ao segundo.
pub fn mesclar(base: &Value, deles: &Value, nosso: &Value) -> Mescla {
    let mut conflitos = Vec::new();
    let valor = juntar(
        Some(base),
        Some(deles),
        Some(nosso),
        &mut Vec::new(),
        &mut conflitos,
    )
    .unwrap_or(Value::Null);
    Mescla { valor, conflitos }
}

/// A junção de um valor; `None` é ausente (o campo ou o item não existe daquele lado).
fn juntar(
    base: Option<&Value>,
    deles: Option<&Value>,
    nosso: Option<&Value>,
    caminho: &mut Vec<String>,
    conflitos: &mut Vec<String>,
) -> Option<Value> {
    if nosso == base {
        return deles.cloned();
    }
    if deles == base || deles == nosso {
        return nosso.cloned();
    }
    match (deles, nosso) {
        (Some(Value::Object(d)), Some(Value::Object(n))) => {
            let vazio = Map::new();
            let b = base.and_then(Value::as_object).unwrap_or(&vazio);
            Some(Value::Object(juntar_objetos(b, d, n, caminho, conflitos)))
        }
        (Some(Value::Array(d)), Some(Value::Array(n)))
            if com_identidade(d)
                && com_identidade(n)
                && base
                    .and_then(Value::as_array)
                    .is_none_or(|b| com_identidade(b)) =>
        {
            let b = base
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            Some(Value::Array(juntar_listas(b, d, n, caminho, conflitos)))
        }
        _ => {
            let partes: Vec<&str> = caminho
                .iter()
                .map(String::as_str)
                .filter(|p| !p.is_empty())
                .collect();
            conflitos.push(partes.join(" › "));
            deles.cloned()
        }
    }
}

fn juntar_objetos(
    base: &Map<String, Value>,
    deles: &Map<String, Value>,
    nosso: &Map<String, Value>,
    caminho: &mut Vec<String>,
    conflitos: &mut Vec<String>,
) -> Map<String, Value> {
    // A ordem dos campos é a de `deles`, com os novos de `nosso` no fim.
    let chaves = deles
        .keys()
        .chain(nosso.keys().filter(|c| !deles.contains_key(*c)))
        .chain(
            base.keys()
                .filter(|c| !deles.contains_key(*c) && !nosso.contains_key(*c)),
        );
    // O envelope de um enum marcado por fora (`{"Group": {..}}`) e o nome de uma lista de itens
    // com id não entram no caminho: os nomes dos itens bastam.
    let envelope = deles.len() == 1
        && nosso.len() == 1
        && deles.keys().eq(nosso.keys())
        && deles
            .values()
            .all(|v| v.as_object().is_some_and(|o| o.contains_key("id")));
    let lista = |valor: Option<&Value>| {
        valor
            .and_then(Value::as_array)
            .is_some_and(|l| com_identidade(l))
    };
    let mut junto = Map::new();
    for chave in chaves {
        let oculta = envelope || (lista(deles.get(chave)) && lista(nosso.get(chave)));
        caminho.push(if oculta { String::new() } else { chave.clone() });
        if let Some(valor) = juntar(
            base.get(chave),
            deles.get(chave),
            nosso.get(chave),
            caminho,
            conflitos,
        ) {
            junto.insert(chave.clone(), valor);
        }
        caminho.pop();
    }
    junto
}

/// Se todos os itens da lista têm identidade (uma lista vazia tem).
fn com_identidade(lista: &[Value]) -> bool {
    lista.iter().all(|item| identidade(item).is_some())
}

/// O nome legível de um item com identidade: o `name` (ou `nome`) dele, ou do valor de dentro
/// num enum marcado por fora; sem nome, a identidade.
fn nome_do_item(item: &Value, id: &str) -> String {
    let nome = |objeto: &Map<String, Value>| {
        objeto
            .get("name")
            .or_else(|| objeto.get("nome"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    item.as_object()
        .and_then(|o| {
            nome(o).or_else(|| {
                let mut campos = o.values();
                match (campos.next(), campos.next()) {
                    (Some(Value::Object(dentro)), None) => nome(dentro),
                    _ => None,
                }
            })
        })
        .unwrap_or_else(|| id.to_owned())
}

fn juntar_listas(
    base: &[Value],
    deles: &[Value],
    nosso: &[Value],
    caminho: &mut Vec<String>,
    conflitos: &mut Vec<String>,
) -> Vec<Value> {
    let ids = |lista: &[Value]| -> Vec<String> {
        lista
            .iter()
            .filter_map(identidade)
            .fold(Vec::new(), |mut ids, id| {
                if !ids.contains(&id) {
                    ids.push(id);
                }
                ids
            })
    };
    let mapa = |lista: &[Value]| -> HashMap<String, Value> {
        let mut mapa = HashMap::new();
        for item in lista {
            if let Some(id) = identidade(item) {
                mapa.entry(id).or_insert_with(|| item.clone());
            }
        }
        mapa
    };
    let (ordem_base, ordem_deles, ordem_nosso) = (ids(base), ids(deles), ids(nosso));
    let (b, d, n) = (mapa(base), mapa(deles), mapa(nosso));

    // A ordem segue o lado que reordenou: se `deles` manteve a da base, vale a de `nosso`. Os
    // itens que só o outro lado tem entram depois do vizinho de antes deles lá (sem ele, antes
    // do de depois; sem nenhum, no fim).
    let (mut ordem, outro) = if ordem_deles == ordem_base {
        (ordem_nosso, ordem_deles)
    } else {
        (ordem_deles, ordem_nosso)
    };
    for (i, id) in outro.iter().enumerate() {
        if ordem.contains(id) {
            continue;
        }
        let depois_de = outro[..i]
            .iter()
            .rev()
            .find_map(|vizinho| ordem.iter().position(|o| o == vizinho));
        let antes_de = || {
            outro[i + 1..]
                .iter()
                .find_map(|vizinho| ordem.iter().position(|o| o == vizinho))
        };
        let lugar = depois_de.map_or_else(|| antes_de().unwrap_or(ordem.len()), |p| p + 1);
        ordem.insert(lugar, id.clone());
    }

    let mut junto = Vec::new();
    for id in &ordem {
        let (base, deles, nosso) = (b.get(id), d.get(id), n.get(id));
        // Novo dos dois lados, com o mesmo id e conteúdo diferente: os dois ficam.
        if let (None, Some(deles), Some(nosso)) = (base, deles, nosso)
            && deles != nosso
        {
            junto.push(deles.clone());
            junto.push(nosso.clone());
            continue;
        }
        let exemplo = deles.or(nosso).or(base).unwrap_or(&Value::Null);
        caminho.push(nome_do_item(exemplo, id));
        if let Some(valor) = juntar(base, deles, nosso, caminho, conflitos) {
            junto.push(valor);
        }
        caminho.pop();
    }
    junto
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn so_um_lado_mudou_vale_o_que_mudou() {
        let base = json!({"a": 1, "b": 2});
        let mudado = json!({"a": 1, "b": 3});
        assert_eq!(mesclar(&base, &base, &mudado).valor, mudado);
        assert_eq!(mesclar(&base, &mudado, &base).valor, mudado);
        assert!(mesclar(&base, &mudado, &base).conflitos.is_empty());
    }

    #[test]
    fn campos_diferentes_dos_dois_lados_ficam_os_dois() {
        let base = json!({"geral": {"tema": "claro", "fonte": 12}, "outro": true});
        let deles = json!({"geral": {"tema": "escuro", "fonte": 12}, "outro": true});
        let nosso = json!({"geral": {"tema": "claro", "fonte": 14}, "outro": true, "novo": 1});
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(
            mescla.valor,
            json!({"geral": {"tema": "escuro", "fonte": 14}, "outro": true, "novo": 1})
        );
        assert!(mescla.conflitos.is_empty());
    }

    #[test]
    fn o_mesmo_campo_dos_dois_lados_fica_o_deles_e_vira_conflito() {
        let base = json!({"geral": {"tema": "claro"}});
        let deles = json!({"geral": {"tema": "escuro"}});
        let nosso = json!({"geral": {"tema": "sistema"}});
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(mescla.valor, deles);
        assert_eq!(mescla.conflitos, ["geral › tema"]);
    }

    #[test]
    fn a_mesma_mudanca_dos_dois_lados_nao_e_conflito() {
        let base = json!({"a": 1});
        let os_dois = json!({"a": 2});
        let mescla = mesclar(&base, &os_dois, &os_dois);
        assert_eq!(mescla.valor, os_dois);
        assert!(mescla.conflitos.is_empty());
    }

    fn conexao(id: u64, nome: &str, host: &str) -> Value {
        json!({"Connection": {"id": id, "name": nome, "host": host}})
    }

    fn grupo(id: u64, nome: &str, filhos: Vec<Value>) -> Value {
        json!({"Group": {"id": id, "name": nome, "children": filhos}})
    }

    #[test]
    fn itens_novos_dos_dois_lados_entram_os_dois_no_lugar() {
        let base = json!([conexao(1, "a", "h1"), conexao(2, "b", "h2")]);
        let deles = json!([
            conexao(1, "a", "h1"),
            conexao(3, "c", "h3"),
            conexao(2, "b", "h2")
        ]);
        let nosso = json!([
            conexao(1, "a", "h1"),
            conexao(2, "b", "h2"),
            conexao(4, "d", "h4")
        ]);
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(
            mescla.valor,
            json!([
                conexao(1, "a", "h1"),
                conexao(3, "c", "h3"),
                conexao(2, "b", "h2"),
                conexao(4, "d", "h4")
            ])
        );
        assert!(mescla.conflitos.is_empty());
    }

    #[test]
    fn o_mesmo_id_novo_dos_dois_lados_fica_duas_vezes() {
        let base = json!([conexao(1, "a", "h1")]);
        let deles = json!([conexao(1, "a", "h1"), conexao(2, "de lá", "h2")]);
        let nosso = json!([conexao(1, "a", "h1"), conexao(2, "daqui", "h3")]);
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(
            mescla.valor,
            json!([
                conexao(1, "a", "h1"),
                conexao(2, "de lá", "h2"),
                conexao(2, "daqui", "h3")
            ])
        );
        assert!(mescla.conflitos.is_empty());
    }

    #[test]
    fn mudar_um_item_que_o_outro_apagou_fica_o_da_principal() {
        let base = json!([conexao(1, "a", "h1"), conexao(2, "b", "h2")]);
        // A principal apagou o 2, e a secundária mudou o host dele: fica apagado.
        let deles = json!([conexao(1, "a", "h1")]);
        let nosso = json!([conexao(1, "a", "h1"), conexao(2, "b", "outro")]);
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(mescla.valor, deles);
        assert_eq!(mescla.conflitos, ["b"]);
        // Ao contrário, a mudança da principal fica.
        let mescla = mesclar(&base, &nosso, &deles);
        assert_eq!(mescla.valor, nosso);
    }

    #[test]
    fn mover_uma_conexao_de_pasta_com_a_principal_mudando_outra_coisa() {
        let base = json!({"nodes": [
            grupo(1, "Produção", vec![conexao(2, "web", "h2")]),
            grupo(3, "Teste", vec![]),
        ]});
        let deles = json!({"nodes": [
            grupo(1, "Produção", vec![conexao(2, "web", "h2")]),
            grupo(3, "Teste", vec![conexao(4, "nova", "h4")]),
        ]});
        let nosso = json!({"nodes": [
            grupo(1, "Produção", vec![]),
            grupo(3, "Teste", vec![conexao(2, "web", "h2")]),
        ]});
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(
            mescla.valor,
            json!({"nodes": [
                grupo(1, "Produção", vec![]),
                grupo(3, "Teste", vec![conexao(4, "nova", "h4"), conexao(2, "web", "h2")]),
            ]})
        );
        assert!(mescla.conflitos.is_empty());
    }

    #[test]
    fn o_conflito_dentro_da_arvore_diz_o_caminho_pelos_nomes() {
        let base = json!({"nodes": [grupo(1, "Produção", vec![conexao(2, "web", "h2")])]});
        let deles = json!({"nodes": [grupo(1, "Produção", vec![conexao(2, "web", "lá")])]});
        let nosso = json!({"nodes": [grupo(1, "Produção", vec![conexao(2, "web", "cá")])]});
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(mescla.valor, deles);
        assert_eq!(mescla.conflitos, ["Produção › web › host"]);
    }

    #[test]
    fn reordenar_de_um_lado_vale() {
        let base = json!([
            conexao(1, "a", "h"),
            conexao(2, "b", "h"),
            conexao(3, "c", "h")
        ]);
        let nosso = json!([
            conexao(3, "c", "h"),
            conexao(1, "a", "h"),
            conexao(2, "b", "h")
        ]);
        let deles = json!([
            conexao(1, "a", "h"),
            conexao(2, "b", "mudou"),
            conexao(3, "c", "h")
        ]);
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(
            mescla.valor,
            json!([
                conexao(3, "c", "h"),
                conexao(1, "a", "h"),
                conexao(2, "b", "mudou")
            ])
        );
    }

    #[test]
    fn listas_sem_id_sao_um_valor_so() {
        let base = json!({"tags": ["a"]});
        let deles = json!({"tags": ["a", "b"]});
        let nosso = json!({"tags": ["a", "c"]});
        let mescla = mesclar(&base, &deles, &nosso);
        assert_eq!(mescla.valor, deles);
        assert_eq!(mescla.conflitos, ["tags"]);
    }

    #[test]
    fn sem_base_os_dois_objetos_se_juntam() {
        let deles = json!({"a": 1, "b": 2});
        let nosso = json!({"a": 1, "c": 3});
        let mescla = mesclar(&Value::Null, &deles, &nosso);
        assert_eq!(mescla.valor, json!({"a": 1, "b": 2, "c": 3}));
        assert!(mescla.conflitos.is_empty());
    }
}
