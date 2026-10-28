//! O app como servidor MCP (M51), do lado da ponte e do canal: a ponte sem o app aberto, o canal
//! entre a ponte e o servidor, e a volta inteira pelo socket com o app de verdade (fatia 3: o
//! `initialize`, o `tools/list` e o `listar_conexoes`).

mod comum;

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Value, json};
use tirech_omni::mcp::permissao::PermissaoIa;
use tirech_omni::mcp::ponte;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::theme::Theme;

use comum::{Montagem, Pasta, montar};

/// Uma saída que o teste lê de volta.
#[derive(Clone, Default)]
struct Saida(Arc<Mutex<Vec<u8>>>);

impl Saida {
    fn linhas(&self) -> Vec<Value> {
        let bytes = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        // A ponte escreve a linha em partes: só vale o que já chegou até o último `\n`.
        let completo = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        String::from_utf8(bytes[..completo].to_vec())
            .expect("a saída da ponte é UTF-8")
            .lines()
            .map(|l| serde_json::from_str(l).expect("cada linha é um JSON"))
            .collect()
    }
}

impl Write for Saida {
    fn write(&mut self, dados: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(dados);
        Ok(dados.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn pedido(id: u32, metodo: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": metodo, "params": params }).to_string()
}

fn inicio() -> String {
    pedido(1, "initialize", json!({ "protocolVersion": "2025-06-18" }))
}

fn chamada(id: u32, ferramenta: &str, args: Value) -> String {
    pedido(
        id,
        "tools/call",
        json!({ "name": ferramenta, "arguments": args }),
    )
}

/// Roda a ponte com `entrada` (uma linha por mensagem) e devolve o código, a saída e o erro.
fn rodar(args: &[&str], endereco: &Path, entrada: &str) -> (i32, Vec<Value>, String) {
    let saida = Saida::default();
    let mut erro = Vec::new();
    let codigo = ponte::rodar_com(
        args.iter().map(|a| (*a).to_owned()),
        endereco,
        entrada.as_bytes(),
        saida.clone(),
        &mut erro,
    );
    (codigo, saida.linhas(), String::from_utf8(erro).unwrap())
}

fn texto_do_resultado(resposta: &Value) -> String {
    resposta["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn a_ponte_responde_sozinha_sem_o_app_aberto() {
    let pasta = Pasta::nova("ponte-sem-app");
    let endereco = pasta.0.join("mcp.sock");
    let entrada = format!(
        "{}\n{}\n{}\n",
        inicio(),
        pedido(2, "tools/list", Value::Null),
        pedido(3, "ping", Value::Null)
    );
    let (codigo, saida, _) = rodar(&[], &endereco, &entrada);
    assert_eq!(codigo, 0);
    assert_eq!(saida.len(), 3);
    assert_eq!(saida[0]["result"]["serverInfo"]["name"], "tirech-omni");
    assert!(
        saida[0]["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("saida_do_servidor")
    );
    let ferramentas = saida[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(ferramentas.len(), 29);
    assert_eq!(saida[2]["result"], json!({}));
}

#[test]
fn chamar_sem_o_app_devolve_erro_de_ferramenta_em_portugues() {
    let pasta = Pasta::nova("ponte-chamada-sem-app");
    let endereco = pasta.0.join("mcp.sock");
    let (_, saida, _) = rodar(
        &[],
        &endereco,
        &format!("{}\n", chamada(7, "listar_conexoes", json!({}))),
    );
    assert_eq!(saida.len(), 1);
    assert_eq!(saida[0]["id"], 7);
    assert_eq!(saida[0]["result"]["isError"], true);
    assert!(texto_do_resultado(&saida[0]).contains("Tirech Omni não está aberto"));
}

#[test]
fn ferramenta_desconhecida_e_argumentos_tortos_sao_erros_jsonrpc() {
    let pasta = Pasta::nova("ponte-invalidos");
    let endereco = pasta.0.join("mcp.sock");
    let entrada = format!(
        "{}\n{}\n",
        chamada(1, "apagar_tudo", json!({})),
        chamada(2, "listar_conexoes", json!([1]))
    );
    let (_, saida, _) = rodar(&[], &endereco, &entrada);
    assert_eq!(saida[0]["error"]["code"], -32602);
    assert_eq!(saida[1]["error"]["code"], -32602);
}

#[test]
fn com_espaco_a_lista_perde_listar_espacos() {
    let pasta = Pasta::nova("ponte-espaco");
    let endereco = pasta.0.join("mcp.sock");
    let entrada = format!("{}\n{}\n", inicio(), pedido(2, "tools/list", Value::Null));
    let (_, saida, _) = rodar(
        &["--espaco", "Site", "--nome", "Revisão"],
        &endereco,
        &entrada,
    );
    let nomes: Vec<&str> = saida[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(nomes.len(), 28);
    assert!(!nomes.contains(&"listar_espacos"));
    assert!(
        saida[0]["result"]["instructions"]
            .as_str()
            .unwrap()
            .contains("Site")
    );
}

#[test]
fn com_instrucoes_o_initialize_comeca_por_elas() {
    // O "Controlar com" do Codex e do OpenCode: eles não têm o `--append-system-prompt` do
    // Claude Code, e as `instructions` do `initialize` levam o mesmo texto.
    let pasta = Pasta::nova("ponte-instrucoes");
    let endereco = pasta.0.join("mcp.sock");
    let (_, saida, _) = rodar(
        &[
            "--aba",
            "0-3",
            "--instrucoes",
            "Você foi aberto para trabalhar na aba \"api\".",
        ],
        &endereco,
        &format!("{}\n", inicio()),
    );
    let instrucoes = saida[0]["result"]["instructions"].as_str().unwrap();
    assert!(
        instrucoes.starts_with("Você foi aberto para trabalhar na aba \"api\".\n\n"),
        "{instrucoes}"
    );
    // O resto continua: a aba padrão e o aviso sobre a saída do servidor.
    assert!(instrucoes.contains("0-3"), "{instrucoes}");
    assert!(instrucoes.contains("saida_do_servidor"), "{instrucoes}");
    // Sem `--instrucoes`, nada muda.
    let (_, saida, _) = rodar(&["--aba", "0-3"], &endereco, &format!("{}\n", inicio()));
    let sem = saida[0]["result"]["instructions"].as_str().unwrap();
    assert!(!sem.contains("\"api\""), "{sem}");
}

#[test]
fn com_aba_a_ponte_conta_a_aba_ao_assistente() {
    let pasta = Pasta::nova("ponte-aba");
    let endereco = pasta.0.join("mcp.sock");
    let entrada = format!("{}\n{}\n", inicio(), pedido(2, "tools/list", Value::Null));
    let (_, saida, _) = rodar(
        &["--nome", "Claude Code · api", "--aba", "0-3"],
        &endereco,
        &entrada,
    );
    let instrucoes = saida[0]["result"]["instructions"].as_str().unwrap();
    assert!(instrucoes.contains("0-3"), "{instrucoes}");
    assert!(instrucoes.contains("saida_do_servidor"), "{instrucoes}");
    let ferramentas = saida[1]["result"]["tools"].as_array().unwrap();
    assert_eq!(ferramentas.len(), 29);
    let digitar = ferramentas.iter().find(|f| f["name"] == "digitar").unwrap();
    let descricao = digitar["inputSchema"]["properties"]["aba"]["description"]
        .as_str()
        .unwrap();
    assert!(descricao.contains("Opcional"), "{descricao}");
    // Sem `--aba`, a descrição continua a de sempre.
    let (_, saida, _) = rodar(&[], &endereco, &entrada);
    let digitar = saida[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "digitar")
        .unwrap()
        .clone();
    assert!(
        !digitar["inputSchema"]["properties"]["aba"]["description"]
            .as_str()
            .unwrap()
            .contains("Opcional")
    );
}

/// Os prompts, que o Claude Code mostra como `/mcp__<servidor>__abas`: a ponte responde sem o
/// app, e junta de volta as palavras que o cliente repartiu entre os argumentos.
#[test]
fn os_prompts_viram_comandos_com_barra_sem_o_app() {
    let pasta = Pasta::nova("ponte-prompts");
    let endereco = pasta.0.join("mcp.sock");
    let obter = |id: u32, nome: &str, args: Value| {
        pedido(
            id,
            "prompts/get",
            json!({ "name": nome, "arguments": args }),
        )
    };
    let entrada = [
        inicio(),
        pedido(2, "prompts/list", Value::Null),
        obter(
            3,
            "rodar-em-todas",
            json!({ "comando": "df", "+1": "-h", "+2": "/var" }),
        ),
        obter(4, "abas", json!({})),
        obter(5, "conectar", json!({})),
        obter(6, "apagar-tudo", json!({})),
    ]
    .join("\n");
    let (codigo, saida, _) = rodar(&["--aba", "0-3"], &endereco, &entrada);
    assert_eq!(codigo, 0);
    assert_eq!(saida.len(), 6);
    assert_eq!(saida[0]["result"]["capabilities"]["prompts"], json!({}));
    assert!(saida[0]["result"]["capabilities"]["tools"].is_object());
    let nomes: Vec<&str> = saida[1]["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(nomes, ["abas", "conectar", "rodar-em-todas"]);
    let texto = |r: &Value| {
        r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert!(
        texto(&saida[2]).contains("df -h /var"),
        "{}",
        texto(&saida[2])
    );
    assert!(texto(&saida[2]).contains("executar"));
    assert!(texto(&saida[3]).contains("listar_abas"));
    assert!(texto(&saida[3]).contains("0-3"), "{}", texto(&saida[3]));
    assert_eq!(saida[4]["error"]["code"], -32602);
    assert!(
        saida[4]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("nomes")
    );
    assert_eq!(saida[5]["error"]["code"], -32602);
}

#[test]
fn aba_sem_valor_e_erro() {
    let pasta = Pasta::nova("ponte-aba-sem-valor");
    let endereco = pasta.0.join("mcp.sock");
    let (codigo, _, erro) = rodar(&["--aba"], &endereco, "");
    assert_eq!(codigo, 2);
    assert!(erro.contains("--aba"));
}

#[test]
fn ajuda_e_argumento_desconhecido() {
    let pasta = Pasta::nova("ponte-argumentos");
    let endereco = pasta.0.join("mcp.sock");
    let (codigo, saida, erro) = rodar(&["--help"], &endereco, "");
    assert_eq!((codigo, saida.len()), (0, 0));
    assert!(erro.contains("Uso: tirech-omni mcp"));
    let (codigo, _, erro) = rodar(&["--mudar"], &endereco, "");
    assert_eq!(codigo, 2);
    assert!(erro.contains("--mudar"));
    let (codigo, _, erro) = rodar(&["--nome"], &endereco, "");
    assert_eq!(codigo, 2);
    assert!(erro.contains("--nome"));
}

#[cfg(unix)]
mod unix {
    use std::collections::HashMap;
    use std::io::BufReader;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use tirech_omni::mcp::canal::{Abertura, RespostaDaAbertura, VERSAO, conectar};
    use tirech_omni::mcp::servidor::{AVISOS, Fila, Pedido, ServidorMcp, tirar};
    use tomni_ui::files::Live;
    use tomni_ui_test::TestApp;

    use super::*;

    fn abrir(
        endereco: &Path,
        espaco: Option<&str>,
        nome: Option<&str>,
    ) -> (UnixStream, RespostaDaAbertura) {
        let mut fluxo = conectar(endereco).expect("conectar ao servidor");
        let abertura = Abertura {
            ponte: VERSAO,
            pid: std::process::id(),
            espaco: espaco.map(str::to_owned),
            nome: nome.map(str::to_owned),
            aba: None,
            lancamento: None,
        };
        writeln!(fluxo, "{}", json!(abertura)).unwrap();
        let mut linha = String::new();
        BufReader::new(fluxo.try_clone().unwrap())
            .read_line(&mut linha)
            .unwrap();
        (fluxo, serde_json::from_str(&linha).unwrap())
    }

    fn esperar_pedidos(fila: &Fila, quantos: usize) -> Vec<Pedido> {
        let limite = Instant::now() + Duration::from_secs(10);
        let mut todos = Vec::new();
        while todos.len() < quantos {
            assert!(Instant::now() < limite, "os pedidos não chegaram");
            todos.extend(tirar(fila));
            std::thread::sleep(Duration::from_millis(10));
        }
        todos
    }

    fn subir(pasta: &Pasta) -> (PathBuf, Fila, ServidorMcp) {
        let endereco = pasta.0.join("mcp.sock");
        let fila: Fila = Live::detached(Vec::new());
        let servidor = ServidorMcp::iniciar(&endereco, fila.clone()).expect("subir o servidor");
        (endereco, fila, servidor)
    }

    #[test]
    fn abertura_e_pedido_ida_e_volta() {
        let pasta = Pasta::nova("canal-ida-e-volta");
        let (endereco, fila, servidor) = subir(&pasta);
        let (mut fluxo, resposta) = abrir(&endereco, None, Some("Revisão"));
        assert!(resposta.ok);
        assert_eq!(resposta.sessao.as_deref(), Some("Revisão"));
        assert_eq!(servidor.sessoes().len(), 1);
        writeln!(
            fluxo,
            "{}",
            json!({ "id": 5, "ferramenta": "listar_conexoes", "args": {} })
        )
        .unwrap();
        let pedidos = esperar_pedidos(&fila, 1);
        assert_eq!(pedidos[0].ferramenta, "listar_conexoes");
        assert_eq!(pedidos[0].sessao, "Revisão");
        pedidos[0].responder(json!({ "ok": true }));
        let mut linha = String::new();
        BufReader::new(fluxo).read_line(&mut linha).unwrap();
        let volta: Value = serde_json::from_str(&linha).unwrap();
        assert_eq!(volta, json!({ "id": 5, "resultado": { "ok": true } }));
    }

    #[test]
    fn a_ponte_liga_no_app_ja_no_initialize() {
        // Sem esperar a primeira ferramenta: a sessão aparece no app (o "Dar esta aba também
        // a" precisa dela) assim que o assistente abre.
        let pasta = Pasta::nova("ponte-liga-no-inicio");
        let (endereco, _fila, servidor) = subir(&pasta);
        let (mut escreve, le) = UnixStream::pair().unwrap();
        let ponte = std::thread::spawn({
            let endereco = endereco.clone();
            move || {
                let args = ["--nome", "Claude Code · api", "--aba", "0-3"];
                ponte::rodar_com(
                    args.iter().map(|a| (*a).to_owned()),
                    &endereco,
                    BufReader::new(le),
                    Saida::default(),
                    &mut Vec::new(),
                )
            }
        });
        writeln!(escreve, "{}", inicio()).unwrap();
        let limite = Instant::now() + Duration::from_secs(10);
        while servidor.sessoes().is_empty() {
            assert!(Instant::now() < limite, "a ponte não ligou no initialize");
            std::thread::sleep(Duration::from_millis(10));
        }
        let sessoes = servidor.sessoes();
        assert_eq!(sessoes[0].nome, "Claude Code · api");
        assert_eq!(sessoes[0].aba.as_deref(), Some("0-3"));
        drop(escreve);
        assert_eq!(ponte.join().unwrap(), 0);
    }

    /// O hook do Claude Code (`mcp --avisos <lancamento>`) numa thread: o código de saída e as
    /// linhas que ele escreveu.
    fn rodar_hook(endereco: &Path, lancamento: &str) -> std::thread::JoinHandle<(i32, Vec<Value>)> {
        let (endereco, lancamento) = (endereco.to_owned(), lancamento.to_owned());
        std::thread::spawn(move || {
            let saida = Saida::default();
            let codigo = ponte::rodar_com(
                ["--avisos".to_owned(), lancamento],
                &endereco,
                std::io::empty(),
                saida.clone(),
                &mut Vec::new(),
            );
            (codigo, saida.linhas())
        })
    }

    #[test]
    fn o_hook_dos_avisos_pede_a_interface_os_da_sessao_do_lancamento() {
        let pasta = Pasta::nova("hook-avisos");
        let (endereco, fila, servidor) = subir(&pasta);
        let mut fluxo = conectar(&endereco).expect("conectar ao servidor");
        let abertura = Abertura {
            ponte: VERSAO,
            pid: std::process::id(),
            espaco: None,
            nome: Some("Claude Code · api".to_owned()),
            aba: Some("0-3".to_owned()),
            lancamento: Some("42-1".to_owned()),
        };
        writeln!(fluxo, "{}", json!(abertura)).unwrap();
        let mut leitor = BufReader::new(fluxo.try_clone().unwrap());
        let mut linha = String::new();
        leitor.read_line(&mut linha).unwrap();
        assert_eq!(servidor.sessoes()[0].lancamento.as_deref(), Some("42-1"));

        // Com aviso: a interface recebe o pedido interno da sessão e o hook escreve o JSON que
        // o Claude Code soma à mensagem da pessoa.
        let hook = rodar_hook(&endereco, "42-1");
        let pedidos = esperar_pedidos(&fila, 1);
        assert_eq!(pedidos[0].ferramenta, AVISOS);
        assert_eq!(pedidos[0].sessao, "Claude Code · api");
        pedidos[0].responder(json!("A pessoa soltou na aba 0-3 o arquivo /tmp/a.txt."));
        let (codigo, linhas) = hook.join().unwrap();
        assert_eq!(codigo, 0);
        assert_eq!(
            linhas,
            [json!({ "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": "A pessoa soltou na aba 0-3 o arquivo /tmp/a.txt.",
            } })]
        );

        // Sem aviso: nada na saída.
        let hook = rodar_hook(&endereco, "42-1");
        esperar_pedidos(&fila, 1)[0].responder(Value::Null);
        assert_eq!(hook.join().unwrap(), (0, Vec::new()));

        // Outro lançamento: o app responde sem perguntar à interface.
        assert_eq!(
            rodar_hook(&endereco, "99-9").join().unwrap(),
            (0, Vec::new())
        );
        assert!(tirar(&fila).is_empty());
        assert_eq!(servidor.sessoes().len(), 1, "o hook não vira sessão");

        // A ponte não pede os avisos como ferramenta.
        writeln!(
            fluxo,
            "{}",
            json!({ "id": 7, "ferramenta": AVISOS, "args": {} })
        )
        .unwrap();
        let mut linha = String::new();
        leitor.read_line(&mut linha).unwrap();
        let resposta: Value = serde_json::from_str(&linha).unwrap();
        assert_eq!(resposta["id"], 7);
        assert_eq!(resposta["erro"]["code"], -32602);
        assert!(tirar(&fila).is_empty());

        // App fechado: silêncio, e a mensagem da pessoa segue.
        drop(servidor);
        assert_eq!(
            rodar_hook(&endereco, "42-1").join().unwrap(),
            (0, Vec::new())
        );
    }

    #[test]
    fn a_abertura_leva_a_aba_ate_a_sessao() {
        let pasta = Pasta::nova("canal-aba");
        let (endereco, _fila, servidor) = subir(&pasta);
        let mut fluxo = conectar(&endereco).expect("conectar ao servidor");
        let abertura = Abertura {
            ponte: VERSAO,
            pid: std::process::id(),
            espaco: None,
            nome: Some("Codex · api".to_owned()),
            aba: Some("0-3".to_owned()),
            lancamento: None,
        };
        writeln!(fluxo, "{}", json!(abertura)).unwrap();
        let mut linha = String::new();
        BufReader::new(fluxo.try_clone().unwrap())
            .read_line(&mut linha)
            .unwrap();
        let resposta: RespostaDaAbertura = serde_json::from_str(&linha).unwrap();
        assert!(resposta.ok);
        let sessoes = servidor.sessoes();
        assert_eq!(sessoes.len(), 1);
        assert_eq!(sessoes[0].aba.as_deref(), Some("0-3"));
        // Uma ponte sem `--aba` continua sem aba.
        let (_fluxo, _) = abrir(&endereco, None, Some("Outra"));
        assert!(servidor.sessoes().iter().any(|s| s.aba.is_none()));
    }

    #[test]
    fn dois_pedidos_respondidos_fora_de_ordem() {
        let pasta = Pasta::nova("canal-fora-de-ordem");
        let (endereco, fila, _servidor) = subir(&pasta);
        let (mut fluxo, _) = abrir(&endereco, None, None);
        for id in [1, 2] {
            writeln!(
                fluxo,
                "{}",
                json!({ "id": id, "ferramenta": "ler_tela", "args": {} })
            )
            .unwrap();
        }
        let pedidos = esperar_pedidos(&fila, 2);
        let por_id: HashMap<u64, &Pedido> = pedidos
            .iter()
            .map(|p| (p.id.as_u64().unwrap(), p))
            .collect();
        por_id[&2].responder(json!("dois"));
        por_id[&1].responder(json!("um"));
        let mut leitor = BufReader::new(fluxo);
        let mut ids = Vec::new();
        for _ in 0..2 {
            let mut linha = String::new();
            leitor.read_line(&mut linha).unwrap();
            let v: Value = serde_json::from_str(&linha).unwrap();
            ids.push((
                v["id"].as_u64().unwrap(),
                v["resultado"].as_str().unwrap().to_owned(),
            ));
        }
        assert_eq!(ids, vec![(2, "dois".to_owned()), (1, "um".to_owned())]);
    }

    #[test]
    fn cancelar_chega_ao_servidor() {
        let pasta = Pasta::nova("canal-cancelar");
        let (endereco, fila, _servidor) = subir(&pasta);
        let (mut fluxo, _) = abrir(&endereco, None, None);
        writeln!(
            fluxo,
            "{}",
            json!({ "id": 9, "ferramenta": "bash", "args": {} })
        )
        .unwrap();
        let pedidos = esperar_pedidos(&fila, 1);
        assert!(!pedidos[0].foi_cancelado());
        writeln!(fluxo, "{}", json!({ "cancelar": 9 })).unwrap();
        let limite = Instant::now() + Duration::from_secs(10);
        while !pedidos[0].foi_cancelado() {
            assert!(Instant::now() < limite, "o cancelamento não chegou");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn fechar_a_ponte_cancela_o_que_ficou_pendente() {
        let pasta = Pasta::nova("canal-fechar");
        let (endereco, fila, servidor) = subir(&pasta);
        let (mut fluxo, _) = abrir(&endereco, None, None);
        writeln!(
            fluxo,
            "{}",
            json!({ "id": 1, "ferramenta": "bash", "args": {} })
        )
        .unwrap();
        let pedidos = esperar_pedidos(&fila, 1);
        drop(fluxo);
        let limite = Instant::now() + Duration::from_secs(10);
        while !pedidos[0].foi_cancelado() || !servidor.sessoes().is_empty() {
            assert!(Instant::now() < limite, "a ligação não foi solta");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn nomes_repetidos_ganham_numero() {
        let pasta = Pasta::nova("canal-nomes");
        let (endereco, _fila, servidor) = subir(&pasta);
        let (_a, ra) = abrir(&endereco, None, None);
        let (_b, rb) = abrir(&endereco, None, None);
        let (_c, rc) = abrir(&endereco, Some("Site"), None);
        let (_d, rd) = abrir(&endereco, Some("Site"), None);
        assert_eq!(ra.sessao.as_deref(), Some("IA 1"));
        assert_eq!(rb.sessao.as_deref(), Some("IA 2"));
        assert_eq!(rc.sessao.as_deref(), Some("Site"));
        assert_eq!(rd.sessao.as_deref(), Some("Site (2)"));
        assert_eq!(servidor.sessoes().len(), 4);
    }

    #[test]
    fn versao_do_canal_diferente_e_recusada() {
        let pasta = Pasta::nova("canal-versao");
        let (endereco, _fila, _servidor) = subir(&pasta);
        let mut fluxo = conectar(&endereco).unwrap();
        writeln!(
            fluxo,
            "{}",
            json!({ "ponte": 99, "pid": 1, "espaco": null, "nome": null })
        )
        .unwrap();
        let mut linha = String::new();
        BufReader::new(fluxo).read_line(&mut linha).unwrap();
        let resposta: RespostaDaAbertura = serde_json::from_str(&linha).unwrap();
        assert!(!resposta.ok);
        assert!(resposta.erro.unwrap().contains("outra versão"));
    }

    #[test]
    fn socket_velho_sem_dono_e_removido() {
        let pasta = Pasta::nova("canal-socket-velho");
        std::fs::create_dir_all(&pasta.0).unwrap();
        let endereco = pasta.0.join("mcp.sock");
        // Um socket que ficou no disco de um app que caiu: ninguém escuta nele.
        drop(std::os::unix::net::UnixListener::bind(&endereco).unwrap());
        assert!(endereco.exists());
        let fila: Fila = Live::detached(Vec::new());
        let servidor = ServidorMcp::iniciar(&endereco, fila).expect("tomar o lugar do velho");
        let (_, resposta) = abrir(&endereco, None, None);
        assert!(resposta.ok);
        drop(servidor);
        assert!(!endereco.exists(), "o socket some quando o servidor para");
    }

    #[test]
    fn socket_com_dono_nao_e_removido() {
        let pasta = Pasta::nova("canal-socket-com-dono");
        let (endereco, fila, _primeiro) = subir(&pasta);
        let erro = ServidorMcp::iniciar(&endereco, fila)
            .err()
            .expect("o segundo não sobe");
        assert!(erro.texto().contains("Outro Tirech Omni"));
        assert!(
            abrir(&endereco, None, None).1.ok,
            "o primeiro segue atendendo"
        );
    }

    #[test]
    fn arquivo_comum_no_endereco_nao_e_apagado() {
        let pasta = Pasta::nova("canal-arquivo");
        std::fs::create_dir_all(&pasta.0).unwrap();
        let endereco = pasta.0.join("mcp.sock");
        std::fs::write(&endereco, "meu texto").unwrap();
        let fila: Fila = Live::detached(Vec::new());
        assert!(ServidorMcp::iniciar(&endereco, fila).is_err());
        assert_eq!(std::fs::read_to_string(&endereco).unwrap(), "meu texto");
    }

    #[test]
    fn permissoes_0700_na_pasta_e_0600_no_socket() {
        let pasta = Pasta::nova("canal-permissoes");
        let endereco = pasta.0.join("sub").join("mcp.sock");
        let fila: Fila = Live::detached(Vec::new());
        let _servidor = ServidorMcp::iniciar(&endereco, fila).expect("subir");
        let modo = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo(endereco.parent().unwrap()), 0o700);
        assert_eq!(modo(&endereco), 0o600);
    }

    /// A tela com o MCP ligado e duas conexões: uma liberada para ver, outra desligada.
    fn tela_com_mcp(teste: &str) -> (Pasta, Montagem, PathBuf) {
        let pasta = Pasta::nova(teste);
        let mut tree = Tree::default();
        let mut liberada = Connection::new("web-01", Kind::Ssh);
        liberada.host = "web.exemplo".into();
        liberada.mcp = PermissaoIa::SoLer;
        tree.add_connection(None, liberada);
        let mut fechada = Connection::new("banco", Kind::Ssh);
        fechada.mcp = PermissaoIa::Desligada;
        tree.add_connection(None, fechada);
        pasta.semear(&tree);
        pasta.configurar("ia", "mcp_ligado", json!(true));
        let endereco = pasta.0.join("mcp.sock");
        let montagem = montar(&pasta, Theme::Light);
        (pasta, montagem, endereco)
    }

    fn esperar_socket(app: &mut TestApp, endereco: &Path) {
        let limite = Instant::now() + Duration::from_secs(10);
        while !endereco.exists() {
            assert!(Instant::now() < limite, "o app não abriu o socket");
            app.wait(Duration::from_millis(20));
        }
    }

    #[test]
    fn ponta_a_ponta_pelo_socket() {
        let (_pasta, mut m, endereco) = tela_com_mcp("ponta-a-ponta");
        esperar_socket(&mut m.app, &endereco);
        // A ponte roda numa thread, com a entrada num cano que o teste alimenta.
        let (leitura, mut escrita) = std::io::pipe().unwrap();
        let saida = Saida::default();
        let para_ponte = saida.clone();
        let caminho = endereco.clone();
        let ponte = std::thread::spawn(move || {
            let mut erro = Vec::new();
            ponte::rodar_com(
                ["--nome".to_owned(), "Teste".to_owned()],
                &caminho,
                BufReader::new(leitura),
                para_ponte,
                &mut erro,
            )
        });
        for linha in [
            inicio(),
            pedido(2, "tools/list", Value::Null),
            chamada(3, "listar_conexoes", json!({})),
            chamada(4, "ler", json!({ "caminho": "a.txt" })),
        ] {
            writeln!(escrita, "{linha}").unwrap();
        }
        let limite = Instant::now() + Duration::from_secs(20);
        while saida.linhas().len() < 4 {
            assert!(
                Instant::now() < limite,
                "faltam respostas: {:?}",
                saida.linhas()
            );
            m.app.wait(Duration::from_millis(20));
        }
        drop(escrita);
        assert_eq!(ponte.join().unwrap(), 0);
        let linhas = saida.linhas();
        let por_id = |id: u64| {
            linhas
                .iter()
                .find(|l| l["id"] == id)
                .expect("resposta com o id")
        };
        assert_eq!(por_id(1)["result"]["serverInfo"]["name"], "tirech-omni");
        assert_eq!(por_id(2)["result"]["tools"].as_array().unwrap().len(), 29);
        let conexoes: Value = serde_json::from_str(&texto_do_resultado(por_id(3))).unwrap();
        let nomes: Vec<&str> = conexoes
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["nome"].as_str().unwrap())
            .collect();
        assert_eq!(nomes, ["web-01"], "só a conexão liberada aparece");
        assert_eq!(conexoes[0]["permissao"], "so_ler");
        assert_eq!(por_id(4)["result"]["isError"], true);
        // Sem `--espaco`, as ferramentas do espaço pedem o argumento `espaco`.
        assert!(texto_do_resultado(por_id(4)).contains("espaco"));
    }
}
