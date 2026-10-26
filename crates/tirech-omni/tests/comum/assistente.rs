//! O assistente dos testes do MCP: uma ligação no canal do app, com as respostas lidas numa
//! thread, e a leitura das respostas.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::Shutdown;
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tirech_omni::mcp::canal::{Abertura, Fluxo, VERSAO, conectar};
use tomni_ui_test::TestApp;

/// Quem faz as vezes do assistente: uma ligação no canal, com as respostas lidas numa thread.
pub struct Assistente {
    pub escrita: Fluxo,
    pub chegadas: Receiver<Value>,
    pub guardadas: Vec<Value>,
    pub proximo: u64,
    /// O nome que o app deu à sessão no aceite.
    pub nome: String,
}

impl Assistente {
    /// Liga no app e espera o aceite, deixando a tela rodar.
    pub fn ligar(app: &mut TestApp, endereco: &Path, nome: &str) -> Self {
        Self::ligar_com(app, endereco, Some(nome), None)
    }

    /// Como [`Self::ligar`], com o `--nome` e o espaço de trabalho que a ponte mandaria.
    pub fn ligar_com(
        app: &mut TestApp,
        endereco: &Path,
        nome: Option<&str>,
        espaco: Option<&str>,
    ) -> Self {
        Self::ligar_tudo(app, endereco, nome, espaco, None, None)
    }

    /// Como [`Self::ligar`], presa à aba `aba` (a ponte com `--aba`, M63).
    pub fn ligar_na_aba(app: &mut TestApp, endereco: &Path, nome: &str, aba: &str) -> Self {
        Self::ligar_tudo(app, endereco, Some(nome), None, Some(aba), None)
    }

    /// Como [`Self::ligar_na_aba`], com o `--lancamento` que o "Controlar com" do Claude Code
    /// passa à ponte (o hook dos avisos acha a sessão por ele).
    pub fn ligar_lancado(
        app: &mut TestApp,
        endereco: &Path,
        nome: &str,
        aba: &str,
        lancamento: &str,
    ) -> Self {
        Self::ligar_tudo(app, endereco, Some(nome), None, Some(aba), Some(lancamento))
    }

    /// A ponte do assistente de um cluster com IA: sem `--aba`, com o `--lancamento` que o app
    /// pôs no arquivo de lançamento (as abas das máquinas chegam por ele).
    pub fn ligar_no_cluster(
        app: &mut TestApp,
        endereco: &Path,
        nome: &str,
        lancamento: &str,
    ) -> Self {
        Self::ligar_tudo(app, endereco, Some(nome), None, None, Some(lancamento))
    }

    fn ligar_tudo(
        app: &mut TestApp,
        endereco: &Path,
        nome: Option<&str>,
        espaco: Option<&str>,
        aba: Option<&str>,
        lancamento: Option<&str>,
    ) -> Self {
        let limite = Instant::now() + Duration::from_secs(10);
        while !endereco.exists() {
            assert!(Instant::now() < limite, "o app não abriu o socket");
            app.wait(Duration::from_millis(20));
        }
        let mut escrita = conectar(endereco).expect("ligar no socket do app");
        let abertura = Abertura {
            ponte: VERSAO,
            pid: std::process::id(),
            espaco: espaco.map(str::to_owned),
            nome: nome.map(str::to_owned),
            aba: aba.map(str::to_owned),
            lancamento: lancamento.map(str::to_owned),
        };
        writeln!(escrita, "{}", serde_json::to_string(&abertura).unwrap()).unwrap();
        let leitura = escrita.try_clone().expect("clonar o fluxo");
        let (envia, chegadas) = channel();
        std::thread::spawn(move || {
            for linha in BufReader::new(leitura).lines() {
                let Ok(linha) = linha else { return };
                let Ok(valor) = serde_json::from_str::<Value>(&linha) else {
                    continue;
                };
                if envia.send(valor).is_err() {
                    return;
                }
            }
        });
        let mut eu = Self {
            escrita,
            chegadas,
            guardadas: Vec::new(),
            proximo: 0,
            nome: String::new(),
        };
        let aceite = eu.proxima(app, |v| v.get("ok").is_some());
        assert_eq!(aceite["ok"], true, "o app recusou a ponte: {aceite}");
        eu.nome = aceite["sessao"].as_str().unwrap_or_default().to_owned();
        eu
    }

    /// Fecha a ligação, como a ponte que cai.
    pub fn desligar(self) {
        let _ = self.escrita.shutdown(Shutdown::Both);
    }

    /// Manda o pedido e devolve o id, sem esperar a resposta.
    pub fn pedir(&mut self, ferramenta: &str, args: Value) -> u64 {
        self.proximo += 1;
        let id = self.proximo;
        writeln!(
            self.escrita,
            "{}",
            json!({ "id": id, "ferramenta": ferramenta, "args": args })
        )
        .unwrap();
        id
    }

    pub fn cancelar(&mut self, id: u64) {
        writeln!(self.escrita, "{}", json!({ "cancelar": id })).unwrap();
    }

    /// A próxima mensagem que `serve`, deixando a tela rodar enquanto espera.
    pub fn proxima(&mut self, app: &mut TestApp, serve: impl Fn(&Value) -> bool) -> Value {
        if let Some(i) = self.guardadas.iter().position(&serve) {
            return self.guardadas.remove(i);
        }
        let limite = Instant::now() + Duration::from_secs(20);
        loop {
            match self.chegadas.recv_timeout(Duration::from_millis(5)) {
                Ok(v) if serve(&v) => return v,
                Ok(v) => self.guardadas.push(v),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => panic!("o canal fechou"),
            }
            assert!(Instant::now() < limite, "a resposta não chegou");
            app.wait(Duration::from_millis(20));
        }
    }

    /// A resposta ao pedido `id`: o `resultado` (ou o erro do protocolo).
    pub fn resposta(&mut self, app: &mut TestApp, id: u64) -> Resposta {
        let v = self.proxima(app, |v| v["id"] == id);
        Resposta(v)
    }

    /// Pede e espera a resposta (para o que não precisa da pessoa).
    pub fn chamar(&mut self, app: &mut TestApp, ferramenta: &str, args: Value) -> Resposta {
        let id = self.pedir(ferramenta, args);
        self.resposta(app, id)
    }
}

pub struct Resposta(pub Value);

impl Resposta {
    pub fn erro_de_protocolo(&self) -> bool {
        self.0.get("erro").is_some()
    }

    pub fn resultado(&self) -> &Value {
        &self.0["resultado"]
    }

    pub fn e_erro(&self) -> bool {
        self.erro_de_protocolo() || self.resultado()["isError"].as_bool().unwrap_or(false)
    }

    pub fn texto(&self) -> String {
        if let Some(e) = self.0.get("erro") {
            return e["message"].as_str().unwrap_or_default().to_owned();
        }
        self.resultado()["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

/// O que o hook do Claude Code (`tirech-omni mcp --avisos <lancamento>`) recebe do app, com a
/// tela rodando enquanto ele espera.
pub fn avisos(app: &mut TestApp, endereco: &Path, lancamento: &str) -> Option<String> {
    let (endereco, lancamento) = (endereco.to_owned(), lancamento.to_owned());
    let hook =
        std::thread::spawn(move || tirech_omni::mcp::ponte::buscar_avisos(&endereco, &lancamento));
    let limite = Instant::now() + Duration::from_secs(10);
    while !hook.is_finished() {
        assert!(Instant::now() < limite, "o hook não acabou");
        app.wait(Duration::from_millis(20));
    }
    hook.join().expect("a thread do hook")
}
