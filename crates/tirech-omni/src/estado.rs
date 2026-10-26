//! O que o app lembra das janelas entre uma execução e outra. O `estado.json` tem o mesmo
//! formato de um espaço de trabalho ([`Espaco`], origem [`Origem::Todas`]): todas as janelas,
//! cada uma com a grade, as conexões de cada painel e, se o sistema informa, a posição e o
//! tamanho. Só referências às conexões vão para o arquivo; o conteúdo dos terminais e as senhas
//! não.
//!
//! O `estado.json` do M24 ao M27 guardava uma janela só, `{"grade": [2, 1], "paineis": [[2, 3],
//! [0]]}`, com os ids das conexões. Ele ainda é lido, como uma janela única.

use serde_json::Value;

use crate::armazenamento::Armazenamento;
use crate::espacos::{Espaco, Item, JanelaSalva, Origem};

/// O estado guardado, no formato atual ou no do M24. `None` sem arquivo ou com um arquivo que
/// não serve (o estado é só uma conveniência: as abas não voltam e fica por aí).
pub fn ler(armazenamento: &Armazenamento) -> Option<Espaco> {
    let valor = armazenamento.ler::<Value>(&armazenamento.arquivo_estado())?;
    do_valor(&valor)
}

/// Grava o estado. Falhar não avisa ninguém: o estado é só uma conveniência.
pub fn gravar(armazenamento: &Armazenamento, estado: &Espaco) {
    let _ = armazenamento.gravar(&armazenamento.arquivo_estado(), estado);
}

/// Há alguma aba para reabrir no estado guardado?
pub fn tem_abas(armazenamento: &Armazenamento) -> bool {
    ler(armazenamento).is_some_and(|e| e.tem_abas())
}

/// O estado de um JSON: o formato atual (tem `janelas`) ou o do M24 (tem `grade` e `paineis`).
pub fn do_valor(valor: &Value) -> Option<Espaco> {
    if valor.get("janelas").is_some() {
        return serde_json::from_value(valor.clone()).ok();
    }
    let grade = valor.get("grade")?.as_array()?;
    let colunas = usize::try_from(grade.first()?.as_u64()?).ok()?;
    let linhas = usize::try_from(grade.get(1)?.as_u64()?).ok()?;
    let paineis = valor
        .get("paineis")?
        .as_array()?
        .iter()
        .map(|painel| {
            painel
                .as_array()
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_u64)
                        .map(|id| Item {
                            id: (id != 0).then_some(id),
                            ..Item::default()
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    Some(Espaco::new(
        Origem::Todas,
        vec![JanelaSalva {
            grade: (colunas, linhas),
            paineis,
            posicao: None,
            tamanho: None,
            cluster_ia: None,
        }],
    ))
}

#[cfg(test)]
mod testes {
    use serde_json::json;

    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    #[test]
    fn o_formato_do_m24_vira_uma_janela_so() {
        let estado = do_valor(&json!({"grade": [2, 1], "paineis": [[2, 3], [0]]})).unwrap();
        assert_eq!(estado.origem, Origem::Todas);
        assert_eq!(estado.janelas.len(), 1);
        let janela = &estado.janelas[0];
        assert_eq!(janela.grade, (2, 1));
        let ids: Vec<Vec<Option<u64>>> = janela
            .paineis
            .iter()
            .map(|p| p.iter().map(|i| i.id).collect())
            .collect();
        assert_eq!(ids, [vec![Some(2), Some(3)], vec![None]]);
        assert!(estado.tem_abas());
    }

    #[test]
    fn o_formato_atual_ida_e_volta() {
        let estado = Espaco::new(
            Origem::Todas,
            vec![
                JanelaSalva {
                    grade: (1, 1),
                    paineis: vec![vec![Item {
                        id: Some(4),
                        nome: "Roteador".into(),
                        host: "192.168.0.1".into(),
                        user: String::new(),
                        tmux: None,
                        screen: None,
                        assistentes: Vec::new(),
                    }]],
                    posicao: None,
                    tamanho: None,
                    cluster_ia: None,
                },
                JanelaSalva {
                    grade: (2, 2),
                    paineis: vec![vec![], vec![Item::shell_local()], vec![], vec![]],
                    posicao: Some((40, 60)),
                    tamanho: Some((900, 600)),
                    cluster_ia: None,
                },
            ],
        );
        let valor = serde_json::to_value(&estado).unwrap();
        assert_eq!(do_valor(&valor), Some(estado));
    }

    #[test]
    fn json_que_nao_e_estado_nao_serve() {
        assert_eq!(do_valor(&json!([1, 2])), None);
        assert_eq!(do_valor(&json!({"grade": "x"})), None);
        assert_eq!(do_valor(&json!({})), None);
    }

    #[test]
    fn grava_e_le_pelo_armazenamento() {
        let pasta = PastaTemporaria::nova("estado");
        let armazenamento = Armazenamento::new(&pasta.0);
        assert_eq!(ler(&armazenamento), None);
        assert!(!tem_abas(&armazenamento));
        let estado = Espaco::new(
            Origem::Todas,
            vec![JanelaSalva {
                grade: (1, 1),
                paineis: vec![vec![Item {
                    id: Some(2),
                    nome: "web-01".into(),
                    host: "h".into(),
                    user: "u".into(),
                    tmux: None,
                    screen: None,
                    assistentes: Vec::new(),
                }]],
                posicao: None,
                tamanho: None,
                cluster_ia: None,
            }],
        );
        gravar(&armazenamento, &estado);
        assert_eq!(ler(&armazenamento), Some(estado));
        assert!(tem_abas(&armazenamento));
    }
}
