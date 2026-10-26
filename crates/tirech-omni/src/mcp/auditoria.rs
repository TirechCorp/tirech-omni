//! O registro de auditoria do MCP (M51, seção 6.7): uma linha JSON por pedido de ferramenta, num
//! arquivo por dia em `<pasta de dados>/mcp/AAAA-MM-DD.jsonl`. Sem rotação nem apagamento: o
//! arquivo é da pessoa.
//!
//! Os argumentos passam pela redação antes de ir para o disco, e o conteúdo que o assistente
//! manda gravar num arquivo (`enviar_arquivo`, `escrever`) não entra: só o tamanho e o SHA-256.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::redacao::Redator;

/// O que se decidiu sobre o pedido.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisaoRegistrada {
    /// A permissão da conexão deixou rodar sem perguntar.
    Permitida,
    /// A pessoa aprovou no diálogo.
    Aprovada,
    /// A pessoa tinha aprovado as próximas daquela aba.
    AprovadaSessao,
    /// A pessoa recusou no diálogo (ou parou a IA).
    Recusada,
    /// A lista de bloqueio pegou e a pessoa recusou.
    BloqueadaRecusada,
    /// A permissão da conexão não deixa (desligada, só leitura) ou o pedido era inválido.
    Negada,
}

/// Uma linha do registro.
#[derive(Clone, Debug, Serialize)]
pub struct Registro {
    /// O pid da ponte que mandou o pedido.
    pub ponte: u32,
    /// O nome da sessão do assistente.
    pub sessao: String,
    /// A ferramenta.
    pub ferramenta: String,
    /// O id da conexão alvo, quando houve.
    pub conexao: Option<u64>,
    /// A chave da aba alvo, quando houve.
    pub aba: Option<String>,
    /// Os argumentos, já redigidos (veja [`args_para_gravar`]).
    pub args: Value,
    /// A decisão.
    pub decisao: DecisaoRegistrada,
    /// O porquê, quando a decisão foi uma recusa.
    pub motivo: Option<String>,
    /// `"ok"` ou `"erro"`.
    pub resultado: String,
    /// O texto do erro, quando houve.
    pub erro: Option<String>,
    /// Quanto o pedido levou, da chegada à resposta.
    pub duracao_ms: u64,
}

/// A linha JSON de `registro`, com `quando` (RFC 3339) na frente.
pub fn linha(quando: &str, registro: &Registro) -> String {
    let mut objeto = Map::new();
    objeto.insert("quando".to_owned(), json!(quando));
    if let Ok(Value::Object(resto)) = serde_json::to_value(registro) {
        objeto.extend(resto);
    }
    Value::Object(objeto).to_string()
}

/// Os argumentos como vão para o registro: cada texto passa pelo `redator` e o conteúdo que o
/// assistente manda gravar vira `{bytes, sha256}`.
pub fn args_para_gravar(args: &Value, redator: &Redator) -> Value {
    let Value::Object(mapa) = args else {
        return redigir(args, redator);
    };
    let mut saida = Map::new();
    for (chave, valor) in mapa {
        let novo = match (chave.as_str(), valor) {
            ("conteudo" | "novo" | "antigo", Value::String(texto)) => resumo_do_conteudo(texto),
            _ => redigir(valor, redator),
        };
        saida.insert(chave.clone(), novo);
    }
    Value::Object(saida)
}

fn resumo_do_conteudo(texto: &str) -> Value {
    let sha: String = Sha256::digest(texto.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    json!({ "bytes": texto.len(), "sha256": sha })
}

fn redigir(valor: &Value, redator: &Redator) -> Value {
    match valor {
        Value::String(texto) => Value::String(redator.aplicar(texto)),
        Value::Array(itens) => Value::Array(itens.iter().map(|v| redigir(v, redator)).collect()),
        Value::Object(mapa) => Value::Object(
            mapa.iter()
                .map(|(k, v)| (k.clone(), redigir(v, redator)))
                .collect(),
        ),
        outro => outro.clone(),
    }
}

/// A pasta do registro dentro da pasta de dados.
pub fn pasta_do_registro(pasta_de_dados: &Path) -> PathBuf {
    pasta_de_dados.join("mcp")
}

/// O arquivo do dia `data` (`AAAA-MM-DD`).
pub fn arquivo_do_dia(pasta_de_dados: &Path, data: &str) -> PathBuf {
    pasta_do_registro(pasta_de_dados).join(format!("{data}.jsonl"))
}

/// Acrescenta `linha` ao arquivo do dia, criando a pasta e o arquivo (só do usuário, no Unix).
pub fn gravar(pasta_de_dados: &Path, data: &str, linha: &str) -> std::io::Result<()> {
    fs::create_dir_all(pasta_do_registro(pasta_de_dados))?;
    let mut opcoes = OpenOptions::new();
    opcoes.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opcoes.mode(0o600);
    }
    let mut arquivo = opcoes.open(arquivo_do_dia(pasta_de_dados, data))?;
    arquivo.write_all(linha.as_bytes())?;
    arquivo.write_all(b"\n")
}

#[cfg(test)]
mod testes {
    use super::*;

    fn registro(args: Value) -> Registro {
        Registro {
            ponte: 42,
            sessao: "IA 1".into(),
            ferramenta: "executar".into(),
            conexao: Some(7),
            aba: Some("7-1".into()),
            args,
            decisao: DecisaoRegistrada::AprovadaSessao,
            motivo: None,
            resultado: "ok".into(),
            erro: None,
            duracao_ms: 12,
        }
    }

    #[test]
    fn a_linha_tem_o_quando_e_os_campos_do_plano() {
        let texto = linha(
            "2026-10-21T10:00:00-03:00",
            &registro(json!({ "comando": "ls" })),
        );
        let v: Value = serde_json::from_str(&texto).unwrap();
        assert_eq!(v["quando"], "2026-10-21T10:00:00-03:00");
        assert_eq!(v["decisao"], "aprovada_sessao");
        assert_eq!(v["conexao"], 7);
        assert_eq!(v["aba"], "7-1");
        assert_eq!(v["args"]["comando"], "ls");
        assert!(!texto.contains('\n'));
    }

    #[test]
    fn os_args_passam_pela_redacao() {
        let redator = Redator::novo(vec!["hunter2-segredo".to_owned().into()], &[]);
        let args = json!({ "texto": "senha hunter2-segredo", "teclas": ["Enter"] });
        let gravado = args_para_gravar(&args, &redator);
        assert!(!gravado.to_string().contains("hunter2-segredo"));
        assert_eq!(gravado["teclas"][0], "Enter");
    }

    #[test]
    fn o_conteudo_gravado_vira_tamanho_e_sha256() {
        let args = json!({ "caminho": "/tmp/a", "conteudo": "olá" });
        let gravado = args_para_gravar(&args, &Redator::desligado());
        assert_eq!(gravado["caminho"], "/tmp/a");
        assert_eq!(gravado["conteudo"]["bytes"], 4);
        assert_eq!(
            gravado["conteudo"]["sha256"],
            "9b186e077c7c6d044f5789d76e6d8070a5b0aaa902ebc608bc34170722dba903"
        );
    }

    #[test]
    fn grava_uma_linha_por_pedido_no_arquivo_do_dia() {
        let pasta = std::env::temp_dir().join(format!("tirech-auditoria-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        gravar(&pasta, "2026-10-21", "{\"a\":1}").unwrap();
        gravar(&pasta, "2026-10-21", "{\"b\":2}").unwrap();
        let lido = fs::read_to_string(arquivo_do_dia(&pasta, "2026-10-21")).unwrap();
        assert_eq!(lido, "{\"a\":1}\n{\"b\":2}\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let modo = fs::metadata(arquivo_do_dia(&pasta, "2026-10-21"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(modo & 0o777, 0o600);
        }
        let _ = fs::remove_dir_all(&pasta);
    }
}
