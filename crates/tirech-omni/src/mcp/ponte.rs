//! O subcomando `tirech-omni mcp` (M51): o programa que o Claude Code e o Codex rodam como
//! servidor MCP. Ele não faz nada sozinho: responde `initialize`, `tools/list` (a lista é a
//! mesma do app) e os [`prompts`], e repassa cada `tools/call`, por uma linha do [`canal`], para
//! o app aberto.
//!
//! Não abre o app: abrir uma janela a partir de um processo que o assistente rodou surpreende
//! a pessoa. App fechado ou controle desligado voltam como resultado de ferramenta com erro
//! (não como erro JSON-RPC), para o assistente ler e contar à pessoa.
//!
//! A ponte liga no app já no `initialize` (o app mostra a sessão nos menus, como o "Dar esta
//! aba também a", antes da primeira ferramenta) e mantém uma ligação; app fechado ali não é erro.
//! Se ela cair (o app fechou), a chamada seguinte tenta de novo uma vez. Vários `tools/call` podem estar em andamento: o
//! laço manda cada um ao app sem esperar e uma thread escreve as respostas quando chegam.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tomni_ui::mcp::{McpError, McpTools, error_result, handle};

use super::canal::{self, Abertura, Fluxo, PedidoDeAvisos, RespostaDaAbertura, RespostaDosAvisos};
use super::{ferramentas, prompts};
use crate::t;

/// O que a linha de comando pediu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opcoes {
    /// O espaço de trabalho a que a ponte fica presa (`--espaco`).
    pub espaco: Option<String>,
    /// O nome da sessão (`--nome`).
    pub nome: Option<String>,
    /// A chave da aba que o assistente controla (`--aba`, M63).
    pub aba: Option<String>,
    /// O lançamento do "Controlar com" (`--lancamento`), que o hook dos avisos usa para achar a
    /// sessão.
    pub lancamento: Option<String>,
    /// O assistente controla um cluster (`--cluster`): o app dá a ele as abas das máquinas
    /// escolhidas, conforme elas abrem, e não há uma aba padrão.
    pub cluster: bool,
    /// O endereço do canal do app (`--canal`), que vence o calculado: o "Controlar com" passa o
    /// seu, porque o Codex roda a ponte sem o `XDG_RUNTIME_DIR` e sem o `TIRECH_OMNI_CONFIG`.
    pub canal: Option<PathBuf>,
    /// O texto que abre as `instructions` do `initialize` (`--instrucoes`): o "Controlar com"
    /// passa ao Codex e ao OpenCode o mesmo que o Claude Code recebe pelo
    /// `--append-system-prompt`, que eles não têm.
    pub instrucoes: Option<String>,
}

/// O que fazer com os argumentos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pedido {
    /// Servir o MCP.
    Servir(Opcoes),
    /// O hook do Claude Code (`--avisos <lançamento>`): escrever os avisos pendentes da sessão
    /// desse lançamento e sair.
    Avisos(String),
    /// O `notify` do Codex (`--conversa <arquivo> [-- <notify da pessoa>…] <json>`): gravar no
    /// arquivo o id da conversa que veio no JSON, chamar o `notify` que a pessoa tinha e sair.
    Conversa {
        arquivo: PathBuf,
        original: Vec<String>,
        json: String,
    },
    /// Mostrar o uso.
    Ajuda,
}

/// Lê os argumentos depois de `mcp`. O erro é a mensagem em português.
pub fn analisar(args: impl IntoIterator<Item = String>) -> Result<Pedido, String> {
    let mut opcoes = Opcoes::default();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Pedido::Ajuda),
            "--cluster" => opcoes.cluster = true,
            "--conversa" => {
                let arquivo = args
                    .next()
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| t!("mcp-ponte-falta-valor", argumento = arg.clone()))?;
                let mut resto: Vec<String> = args.collect();
                let json = resto.pop().unwrap_or_default();
                let original = match resto.first().map(String::as_str) {
                    Some("--") => resto.split_off(1),
                    _ => Vec::new(),
                };
                return Ok(Pedido::Conversa {
                    arquivo: PathBuf::from(arquivo),
                    original,
                    json,
                });
            }
            "--espaco" | "--nome" | "--aba" | "--lancamento" | "--avisos" | "--canal"
            | "--instrucoes" => {
                let valor = args
                    .next()
                    .filter(|v| !v.trim().is_empty())
                    .ok_or_else(|| t!("mcp-ponte-falta-valor", argumento = arg.clone()))?;
                match arg.as_str() {
                    "--espaco" => opcoes.espaco = Some(valor),
                    "--aba" => opcoes.aba = Some(valor),
                    "--lancamento" => opcoes.lancamento = Some(valor),
                    "--avisos" => return Ok(Pedido::Avisos(valor)),
                    "--canal" => opcoes.canal = Some(PathBuf::from(valor)),
                    "--instrucoes" => opcoes.instrucoes = Some(valor),
                    _ => opcoes.nome = Some(valor),
                }
            }
            _ => return Err(t!("mcp-ponte-argumento", argumento = arg)),
        }
    }
    Ok(Pedido::Servir(opcoes))
}

/// O ponto de entrada do `main`: devolve o código de saída do processo.
pub fn rodar(args: impl IntoIterator<Item = String>) -> i32 {
    rodar_com(
        args,
        &super::endereco_padrao(),
        io::stdin().lock(),
        io::stdout(),
        &mut io::stderr(),
    )
}

/// [`rodar`] com o endereço, a entrada, a saída e a saída de erro nos parâmetros (os testes dão
/// os seus). Termina quando a entrada fecha.
pub fn rodar_com(
    args: impl IntoIterator<Item = String>,
    endereco: &Path,
    entrada: impl BufRead,
    saida: impl Write + Send + 'static,
    erro: &mut dyn Write,
) -> i32 {
    let opcoes = match analisar(args) {
        Ok(Pedido::Servir(opcoes)) => opcoes,
        Ok(Pedido::Ajuda) => {
            let _ = writeln!(erro, "{}", t!("mcp-ponte-uso"));
            return 0;
        }
        Ok(Pedido::Conversa {
            arquivo,
            original,
            json,
        }) => {
            crate::conversas::gravar_aviso_do_codex(&arquivo, &json);
            if let Some((programa, args)) = original.split_first() {
                // O Codex não espera o `notify`; o da pessoa roda solto, como rodaria sem o app.
                let _ = std::process::Command::new(programa)
                    .args(args)
                    .arg(&json)
                    .stdin(std::process::Stdio::null())
                    .spawn();
            }
            return 0;
        }
        Ok(Pedido::Avisos(lancamento)) => {
            let mut saida = saida;
            if let Some(texto) = buscar_avisos(endereco, &lancamento) {
                let _ = writeln!(saida, "{}", saida_do_hook(&texto)).and_then(|()| saida.flush());
            }
            return 0;
        }
        Err(mensagem) => {
            let _ = writeln!(erro, "{mensagem}");
            return 2;
        }
    };
    let endereco = opcoes.canal.clone().unwrap_or_else(|| endereco.to_owned());
    let mut ponte = Ponte::nova(endereco, opcoes, Box::new(saida));
    for linha in entrada.lines() {
        let Ok(linha) = linha else { break };
        if linha.trim().is_empty() {
            continue;
        }
        let resposta = match serde_json::from_str::<Value>(&linha) {
            Ok(mensagem) => ponte.atender(&mensagem),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": Value::Null,
                "error": { "code": -32700, "message": format!("JSON inválido: {e}") },
            })),
        };
        if let Some(resposta) = resposta {
            ponte.compartilhado.escrever(&resposta);
        }
    }
    ponte.encerrar();
    0
}

/// A espera máxima pela resposta do app ao pedido de avisos.
const ESPERA_DOS_AVISOS: Duration = Duration::from_secs(8);

/// Pede ao app os avisos pendentes da sessão do `lancamento`. App fechado, de outra versão ou
/// sem resposta a tempo: `None`, em silêncio (o hook não atrapalha a mensagem da pessoa).
pub fn buscar_avisos(endereco: &Path, lancamento: &str) -> Option<String> {
    let mut fluxo = canal::conectar(endereco).ok()?;
    let _ = fluxo.set_read_timeout(Some(ESPERA_DOS_AVISOS));
    let pedido = PedidoDeAvisos {
        avisos: lancamento.to_owned(),
        versao: canal::VERSAO,
    };
    writeln!(fluxo, "{}", json!(pedido))
        .and_then(|()| fluxo.flush())
        .ok()?;
    let mut linha = String::new();
    BufReader::new(fluxo).read_line(&mut linha).ok()?;
    serde_json::from_str::<RespostaDosAvisos>(&linha)
        .ok()?
        .texto
        .filter(|t| !t.trim().is_empty())
}

/// O que o hook `UserPromptSubmit` do Claude Code escreve: o `texto` como contexto somado à
/// mensagem da pessoa.
pub fn saida_do_hook(texto: &str) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": texto,
        }
    })
}

/// O que a ponte e a thread que lê o app dividem.
struct Compartilhado {
    /// A saída padrão da ponte (o que o assistente lê), com a trava das linhas inteiras.
    saida: Mutex<Box<dyn Write + Send>>,
    /// Os pedidos que o app ainda não respondeu: o `id` em texto, o `id` e a ligação.
    pendentes: Mutex<HashMap<String, (Value, u64)>>,
}

impl Compartilhado {
    fn escrever(&self, mensagem: &Value) {
        let mut saida = self.saida.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = writeln!(saida, "{mensagem}").and_then(|()| saida.flush());
    }

    fn responder(&self, id: &Value, resultado: &Value) {
        self.escrever(&json!({ "jsonrpc": "2.0", "id": id, "result": resultado }));
    }
}

/// A ligação com o app: o lado de escrita e o número dela (cada ligação nova tem o seu).
struct Ligacao {
    escritor: Fluxo,
    numero: u64,
}

/// A ponte inteira: o que o `McpTools` precisa e o estado da ligação com o app.
struct Ponte {
    endereco: PathBuf,
    opcoes: Opcoes,
    compartilhado: Arc<Compartilhado>,
    ligacao: Option<Ligacao>,
    ligacoes_feitas: u64,
}

impl Ponte {
    fn nova(endereco: PathBuf, opcoes: Opcoes, saida: Box<dyn Write + Send>) -> Self {
        Self {
            endereco,
            opcoes,
            compartilhado: Arc::new(Compartilhado {
                saida: Mutex::new(saida),
                pendentes: Mutex::new(HashMap::new()),
            }),
            ligacao: None,
            ligacoes_feitas: 0,
        }
    }

    /// Atende uma mensagem do assistente. `tools/call` segue ao app sem esperar (a resposta sai
    /// pela thread leitora) e não tem resposta aqui; o resto passa pelo `handle` da lib.
    fn atender(&mut self, mensagem: &Value) -> Option<Value> {
        if mensagem["method"] == "tools/call"
            && let Some(id) = mensagem.get("id")
        {
            return self.chamar(
                id,
                mensagem["params"]["name"].as_str().unwrap_or_default(),
                &mensagem["params"]["arguments"],
            );
        }
        if let Some(resposta) = self.atender_prompt(mensagem) {
            return Some(resposta);
        }
        let inicio = mensagem["method"] == "initialize";
        if inicio {
            // Liga já, para a sessão aparecer no app; sem o app, a primeira chamada tenta de novo.
            let _ = self.garantir();
        }
        let mut atendente = Atendente { ponte: self };
        let mut resposta = handle(&mut atendente, mensagem);
        // A lib só anuncia ferramentas; os prompts são da ponte.
        if inicio
            && let Some(capacidades) = resposta
                .as_mut()
                .and_then(|r| r.pointer_mut("/result/capabilities"))
                .and_then(Value::as_object_mut)
        {
            capacidades.insert("prompts".into(), json!({}));
        }
        resposta
    }

    /// `prompts/list` e `prompts/get`, que a ponte responde sem o app ([`prompts`]). `None` para
    /// as outras mensagens e para as notificações.
    fn atender_prompt(&self, mensagem: &Value) -> Option<Value> {
        let id = mensagem.get("id")?;
        let params = &mensagem["params"];
        let resultado = match mensagem["method"].as_str()? {
            "prompts/list" => Ok(json!({ "prompts": prompts::lista() })),
            "prompts/get" => prompts::obter(
                params["name"].as_str().unwrap_or_default(),
                &params["arguments"],
                self.opcoes.aba.as_deref(),
            ),
            _ => return None,
        };
        Some(match resultado {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(e) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": e.code, "message": e.message },
            }),
        })
    }

    /// Manda um `tools/call` ao app. `Some` é a resposta imediata (erro); `None`, que a thread
    /// leitora responde quando o app responder.
    fn chamar(&mut self, id: &Value, nome: &str, args: &Value) -> Option<Value> {
        let erro_jsonrpc = |code: i64, message: String| {
            Some(
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
            )
        };
        if !ferramentas::nomes(self.opcoes.espaco.as_deref())
            .iter()
            .any(|n| n == nome)
        {
            return erro_jsonrpc(-32602, t!("mcp-ponte-ferramenta-desconhecida", nome = nome));
        }
        let args = if args.is_null() {
            json!({})
        } else {
            args.clone()
        };
        if !args.is_object() {
            return erro_jsonrpc(-32602, t!("mcp-ponte-argumentos-tortos"));
        }
        let linha = json!({ "id": id, "ferramenta": nome, "args": args });
        let chave = id.to_string();
        // Uma nova tentativa só: a ligação pode ter caído desde a chamada passada.
        for tentativa in 0..2 {
            let numero = match self.garantir() {
                Ok(numero) => numero,
                Err(texto) => return Some(resultado_com_erro(id, &texto)),
            };
            self.compartilhado
                .pendentes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(chave.clone(), (id.clone(), numero));
            if self.escrever_no_app(&linha) {
                return None;
            }
            self.compartilhado
                .pendentes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&chave);
            self.ligacao = None;
            if tentativa == 1 {
                break;
            }
        }
        Some(resultado_com_erro(id, &t!("mcp-ponte-sem-app")))
    }

    /// Escreve uma linha no app; `false` se a ligação não está ou falhou.
    fn escrever_no_app(&mut self, linha: &Value) -> bool {
        match &mut self.ligacao {
            Some(l) => writeln!(l.escritor, "{linha}")
                .and_then(|()| l.escritor.flush())
                .is_ok(),
            None => false,
        }
    }

    /// Garante a ligação com o app (liga e abre a sessão se preciso) e devolve o número dela.
    /// O erro é o texto para o assistente.
    fn garantir(&mut self) -> Result<u64, String> {
        if let Some(l) = &self.ligacao {
            return Ok(l.numero);
        }
        let mut fluxo = canal::conectar(&self.endereco).map_err(|_| t!("mcp-ponte-sem-app"))?;
        let abertura = Abertura {
            ponte: canal::VERSAO,
            pid: std::process::id(),
            espaco: self.opcoes.espaco.clone(),
            nome: self.opcoes.nome.clone(),
            aba: self.opcoes.aba.clone(),
            lancamento: self.opcoes.lancamento.clone(),
        };
        let leitor = fluxo.try_clone().map_err(|_| t!("mcp-ponte-sem-app"))?;
        let mut leitor = BufReader::new(leitor);
        writeln!(fluxo, "{}", json!(abertura))
            .and_then(|()| fluxo.flush())
            .map_err(|_| t!("mcp-ponte-sem-app"))?;
        let mut linha = String::new();
        if leitor.read_line(&mut linha).unwrap_or(0) == 0 {
            return Err(t!("mcp-ponte-sem-app"));
        }
        let resposta: RespostaDaAbertura =
            serde_json::from_str(&linha).map_err(|_| t!("mcp-ponte-outra-versao"))?;
        if !resposta.ok {
            return Err(resposta.erro.unwrap_or_else(|| t!("mcp-ponte-desligado")));
        }
        self.ligacoes_feitas += 1;
        let numero = self.ligacoes_feitas;
        let compartilhado = Arc::clone(&self.compartilhado);
        thread::spawn(move || ler_do_app(leitor, &compartilhado, numero));
        self.ligacao = Some(Ligacao {
            escritor: fluxo,
            numero,
        });
        Ok(numero)
    }

    /// A entrada fechou: cancela o que está no app e solta a ligação.
    fn encerrar(&mut self) {
        let ids: Vec<Value> = self
            .compartilhado
            .pendentes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.escrever_no_app(&json!({ "cancelar": id }));
        }
        if let Some(l) = self.ligacao.take() {
            let _ = l.escritor.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// A resposta de `tools/call` que diz que falhou, com o texto para o assistente ler.
fn resultado_com_erro(id: &Value, texto: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": error_result(texto) })
}

/// A thread que lê o app: entrega cada resposta ao assistente. Quando a ligação acaba, os
/// pedidos que ficaram sem resposta voltam como erro.
fn ler_do_app(leitor: BufReader<Fluxo>, compartilhado: &Compartilhado, numero: u64) {
    for linha in leitor.lines() {
        let Ok(linha) = linha else { break };
        let Ok(mensagem) = serde_json::from_str::<Value>(&linha) else {
            continue;
        };
        let Some(id) = mensagem.get("id") else {
            continue;
        };
        let tirado = compartilhado
            .pendentes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id.to_string());
        // Sem pedido pendente: o assistente cancelou, e o MCP manda ignorar a resposta.
        let Some((id, _)) = tirado else { continue };
        if let Some(resultado) = mensagem.get("resultado") {
            compartilhado.responder(&id, resultado);
        } else if let Some(erro) = mensagem.get("erro") {
            compartilhado.escrever(&json!({ "jsonrpc": "2.0", "id": id, "error": erro }));
        }
    }
    let orfaos: Vec<Value> = {
        let mut pendentes = compartilhado
            .pendentes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let chaves: Vec<String> = pendentes
            .iter()
            .filter(|(_, (_, n))| *n == numero)
            .map(|(k, _)| k.clone())
            .collect();
        chaves
            .iter()
            .filter_map(|k| pendentes.remove(k).map(|(id, _)| id))
            .collect()
    };
    for id in orfaos {
        compartilhado.escrever(&resultado_com_erro(&id, &t!("mcp-ponte-caiu")));
    }
}

/// O `McpTools` da ponte, para o `handle` da lib responder `initialize`, `tools/list`, `ping`
/// e levar o cancelamento ao app. O `tools/call` não passa por aqui: [`Ponte::atender`] o
/// intercepta para não esperar a resposta.
struct Atendente<'a> {
    ponte: &'a mut Ponte,
}

impl McpTools for Atendente<'_> {
    fn server_info(&self) -> (String, String) {
        (
            super::clientes::NOME.to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        )
    }

    fn instructions(&self) -> String {
        let mut partes: Vec<String> = self.ponte.opcoes.instrucoes.iter().cloned().collect();
        if let Some(aba) = &self.ponte.opcoes.aba {
            partes.push(t!("mcp-instrucoes-aba", aba = aba.clone()));
        }
        if self.ponte.opcoes.cluster {
            partes.push(t!("mcp-instrucoes-cluster"));
        }
        if let Some(espaco) = &self.ponte.opcoes.espaco {
            partes.push(t!("mcp-instrucoes-espaco", espaco = espaco.clone()));
        }
        partes.push(t!("mcp-instrucoes"));
        partes.join("\n\n")
    }

    fn tools(&self) -> Value {
        Value::Array(ferramentas::lista(
            self.ponte.opcoes.espaco.as_deref(),
            self.ponte.opcoes.aba.is_some(),
        ))
    }

    fn call(&mut self, _: &str, _: &Value, _: &Value) -> Result<Value, McpError> {
        Err(McpError::new(-32603, "tools/call não passa por aqui"))
    }

    fn cancelled(&mut self, id: &Value) {
        let pendente = self
            .ponte
            .compartilhado
            .pendentes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id.to_string())
            .is_some();
        if pendente {
            self.ponte.escrever_no_app(&json!({ "cancelar": id }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(lista: &[&str]) -> Vec<String> {
        lista.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn analisar_le_a_aba() {
        assert_eq!(
            analisar(args(&["--nome", "Claude Code · api", "--aba", "0-3"])),
            Ok(Pedido::Servir(Opcoes {
                espaco: None,
                nome: Some("Claude Code · api".into()),
                aba: Some("0-3".into()),
                lancamento: None,
                cluster: false,
                canal: None,
                instrucoes: None,
            }))
        );
        assert!(analisar(args(&["--aba", "  "])).is_err());
        assert_eq!(
            analisar(args(&["--aba", "0-3", "--lancamento", "42-1"])),
            Ok(Pedido::Servir(Opcoes {
                aba: Some("0-3".into()),
                lancamento: Some("42-1".into()),
                ..Opcoes::default()
            }))
        );
        assert_eq!(
            analisar(args(&[
                "--aba",
                "0-3",
                "--canal",
                "/run/user/1000/tirech-omni/mcp.sock"
            ])),
            Ok(Pedido::Servir(Opcoes {
                aba: Some("0-3".into()),
                canal: Some("/run/user/1000/tirech-omni/mcp.sock".into()),
                ..Opcoes::default()
            }))
        );
        assert_eq!(
            analisar(args(&[
                "--aba",
                "0-3",
                "--instrucoes",
                "Trabalhe na aba \"api\"."
            ])),
            Ok(Pedido::Servir(Opcoes {
                aba: Some("0-3".into()),
                instrucoes: Some("Trabalhe na aba \"api\".".into()),
                ..Opcoes::default()
            }))
        );
        assert!(analisar(args(&["--instrucoes", " "])).is_err());
        assert_eq!(
            analisar(args(&["--avisos", "42-1"])),
            Ok(Pedido::Avisos("42-1".into()))
        );
        assert_eq!(analisar(args(&["-h"])), Ok(Pedido::Ajuda));
    }

    #[test]
    fn analisar_le_o_notify_do_codex() {
        assert_eq!(
            analisar(args(&["--conversa", "/c/conversa-1.json", "{}"])),
            Ok(Pedido::Conversa {
                arquivo: "/c/conversa-1.json".into(),
                original: vec![],
                json: "{}".into(),
            })
        );
        assert_eq!(
            analisar(args(&[
                "--conversa",
                "/c/x.json",
                "--",
                "notificar",
                "-q",
                "{}"
            ])),
            Ok(Pedido::Conversa {
                arquivo: "/c/x.json".into(),
                original: vec!["notificar".into(), "-q".into()],
                json: "{}".into(),
            })
        );
        assert!(analisar(args(&["--conversa"])).is_err());
    }

    #[test]
    fn o_notify_do_codex_grava_a_conversa() {
        let pasta = crate::armazenamento::testes::PastaTemporaria::nova("ponte-conversa");
        let arquivo = pasta.0.join("conversa-1.json");
        let json = r#"{"type":"agent-turn-complete","thread-id":"019a","input-messages":["oi"]}"#;
        let mut erro = Vec::new();
        let codigo = rodar_com(
            args(&["--conversa", &arquivo.to_string_lossy(), json]),
            &pasta.0.join("nada.sock"),
            io::empty(),
            Vec::new(),
            &mut erro,
        );
        assert_eq!(codigo, 0);
        assert_eq!(
            crate::conversas::ler_aviso_do_codex(&arquivo).map(|a| a.id),
            Some("019a".into())
        );
    }

    #[test]
    fn analisar_le_o_cluster_sem_valor() {
        assert_eq!(
            analisar(args(&["--cluster", "--lancamento", "42-1"])),
            Ok(Pedido::Servir(Opcoes {
                lancamento: Some("42-1".into()),
                cluster: true,
                ..Opcoes::default()
            }))
        );
    }
}
