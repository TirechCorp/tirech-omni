//! Os prompts do MCP, que o Claude Code mostra como comandos com barra
//! (`/mcp__<servidor>__abas`). A ponte responde `prompts/list` e `prompts/get` sozinha, sem o
//! app: cada prompt só devolve um pedido em texto, que o assistente atende com as ferramentas.
//!
//! O Claude Code divide o que se escreve depois do comando por espaços e dá uma palavra a cada
//! argumento, na ordem; as palavras que sobram se perdem. Por isso o argumento de cada prompt vem
//! seguido de [`EXTRAS`] argumentos opcionais (`+1`, `+2`…), e a ponte junta tudo de volta: assim
//! `/rodar-em-todas df -h /var` chega inteiro.

use serde_json::{Value, json};
use tomni_ui::mcp::McpError;

use crate::t;

/// Quantas palavras a mais cabem depois da primeira do argumento.
pub const EXTRAS: usize = 15;

/// A lista do `prompts/list`.
pub fn lista() -> Value {
    json!([
        prompt("abas", t!("mcp-prompt-abas-descricao"), None),
        prompt(
            "conectar",
            t!("mcp-prompt-conectar-descricao"),
            Some(("nomes", t!("mcp-prompt-conectar-nomes"))),
        ),
        prompt(
            "rodar-em-todas",
            t!("mcp-prompt-rodar-descricao"),
            Some(("comando", t!("mcp-prompt-rodar-comando"))),
        ),
    ])
}

fn prompt(nome: &str, descricao: String, argumento: Option<(&str, String)>) -> Value {
    let mut argumentos = Vec::new();
    if let Some((arg, texto)) = argumento {
        argumentos.push(json!({ "name": arg, "description": texto, "required": true }));
        for n in 1..=EXTRAS {
            argumentos.push(json!({
                "name": format!("+{n}"),
                "description": t!("mcp-prompt-continuacao"),
                "required": false,
            }));
        }
    }
    json!({ "name": nome, "description": descricao, "arguments": argumentos })
}

/// O argumento `nome` com as palavras dos extras, na ordem, separadas por um espaço.
fn juntar(args: &Value, nome: &str) -> String {
    std::iter::once(nome.to_owned())
        .chain((1..=EXTRAS).map(|n| format!("+{n}")))
        .filter_map(|chave| args.get(&chave).and_then(Value::as_str).map(str::trim))
        .filter(|palavra| !palavra.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// O `result` do `prompts/get`. `aba` é a do `--aba` da ponte, que o prompt das abas cita.
pub fn obter(nome: &str, args: &Value, aba: Option<&str>) -> Result<Value, McpError> {
    let exigir = |arg: &str| {
        let texto = juntar(args, arg);
        if texto.is_empty() {
            Err(McpError::new(
                -32602,
                t!("mcp-prompt-falta-argumento", argumento = arg),
            ))
        } else {
            Ok(texto)
        }
    };
    let (descricao, texto) = match nome {
        "abas" => {
            let mut texto = t!("mcp-prompt-abas");
            if let Some(aba) = aba {
                texto = format!("{texto} {}", t!("mcp-prompt-abas-padrao", aba = aba));
            }
            (t!("mcp-prompt-abas-descricao"), texto)
        }
        "conectar" => (
            t!("mcp-prompt-conectar-descricao"),
            t!("mcp-prompt-conectar", nomes = exigir("nomes")?),
        ),
        "rodar-em-todas" => (
            t!("mcp-prompt-rodar-descricao"),
            t!("mcp-prompt-rodar", comando = exigir("comando")?),
        ),
        _ => {
            return Err(McpError::new(
                -32602,
                t!("mcp-prompt-desconhecido", nome = nome),
            ));
        }
    };
    Ok(json!({
        "description": descricao,
        "messages": [{ "role": "user", "content": { "type": "text", "text": texto } }],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texto(resultado: &Value) -> &str {
        resultado["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
    }

    #[test]
    fn a_lista_tem_os_tres_com_os_extras() {
        let lista = lista();
        let nomes: Vec<&str> = lista
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(nomes, ["abas", "conectar", "rodar-em-todas"]);
        assert_eq!(lista[0]["arguments"], json!([]));
        let args = lista[2]["arguments"].as_array().unwrap();
        assert_eq!(args.len(), 1 + EXTRAS);
        assert_eq!(args[0]["name"], "comando");
        assert_eq!(args[0]["required"], true);
        assert_eq!(args[1]["name"], "+1");
        assert_eq!(args[1]["required"], false);
    }

    #[test]
    fn junta_as_palavras_que_o_cliente_separou() {
        let args = json!({ "comando": "df", "+1": "-h", "+2": "/var" });
        let r = obter("rodar-em-todas", &args, None).unwrap();
        assert!(texto(&r).contains("df -h /var"), "{}", texto(&r));
        assert_eq!(r["messages"][0]["role"], "user");
    }

    #[test]
    fn sem_o_argumento_ou_com_nome_errado_da_erro() {
        let erro = obter("conectar", &json!({}), None).unwrap_err();
        assert_eq!(erro.code, -32602);
        assert!(erro.message.contains("nomes"), "{}", erro.message);
        let erro = obter("conectar", &json!({ "nomes": "  " }), None).unwrap_err();
        assert_eq!(erro.code, -32602);
        assert_eq!(obter("outro", &json!({}), None).unwrap_err().code, -32602);
    }

    #[test]
    fn o_prompt_das_abas_cita_a_aba_padrao() {
        let sem = obter("abas", &json!({}), None).unwrap();
        let com = obter("abas", &json!({}), Some("ab12")).unwrap();
        assert!(texto(&com).starts_with(texto(&sem)));
        assert!(texto(&com).contains("ab12"));
        assert!(!texto(&sem).contains("ab12"));
    }
}
