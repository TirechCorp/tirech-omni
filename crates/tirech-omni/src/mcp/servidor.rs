//! O servidor MCP dentro do app (M51): escuta no endereço de [`super::endereco_padrao`], uma
//! thread por ponte ligada, e põe cada pedido numa fila que a interface atende. A thread do
//! servidor nunca toca em signal nem em sessão: só em `Live`, em canais e em flags.
//!
//! No Unix o endereço é um socket numa pasta `0700`, e o socket é `0600`: só o próprio usuário
//! chega nele. O pid que a ponte declara é informativo; a proteção é o socket. No Windows é um
//! pipe com dono, que só o próprio usuário abre, e o pid declarado é conferido com o que o
//! sistema lê do pipe.
//!
//! O servidor existe enquanto o controle por assistentes está ligado. Soltá-lo (`Drop`) fecha o
//! socket, derruba as ligações e marca como cancelado tudo o que estava em andamento.

use std::collections::HashMap;
#[cfg(unix)]
use std::io;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};
use tomni_ui::files::Live;

use super::canal::{self, Abertura, Fluxo, PedidoDeAvisos, RespostaDaAbertura, RespostaDosAvisos};
use crate::t;

/// Um pedido de ferramenta, a caminho da interface.
#[derive(Clone, Debug)]
pub struct Pedido {
    /// A ligação (a ponte) que pediu.
    pub ligacao: u64,
    /// O nome da sessão dessa ligação.
    pub sessao: String,
    /// O espaço de trabalho a que a ponte está presa (`--espaco`).
    pub espaco: Option<String>,
    /// O `id` do JSON-RPC.
    pub id: Value,
    /// O nome da ferramenta.
    pub ferramenta: String,
    /// Os argumentos.
    pub args: Value,
    /// O pid que a ponte declarou (informativo).
    pub pid: u32,
    /// Vira `true` quando o assistente cancela, a ligação cai ou o servidor para: quem roda
    /// confere entre os passos.
    pub cancelado: Arc<AtomicBool>,
    responder: Sender<Value>,
}

impl Pedido {
    /// Responde com o `result` do MCP (`text_result` ou `error_result`). Um pedido cancelado
    /// não responde: o MCP diz que o cliente ignora essa resposta.
    pub fn responder(&self, resultado: Value) {
        self.enviar(json!({ "id": self.id, "resultado": resultado }));
    }

    /// Responde com um erro JSON-RPC (argumentos que não seguem o esquema).
    pub fn recusar(&self, codigo: i64, mensagem: &str) {
        self.enviar(json!({ "id": self.id, "erro": { "code": codigo, "message": mensagem } }));
    }

    /// Se o assistente cancelou (ou a ligação acabou).
    pub fn foi_cancelado(&self) -> bool {
        self.cancelado.load(Ordering::SeqCst)
    }

    fn enviar(&self, linha: Value) {
        if !self.foi_cancelado() {
            let _ = self.responder.send(linha);
        }
    }
}

/// O nome do pedido interno dos avisos (o hook do Claude Code): não é ferramenta, e uma ponte
/// que o pede ouve "ferramenta desconhecida". A interface responde só o texto (ou `null`).
pub const AVISOS: &str = "#avisos";

/// A espera máxima pela interface no pedido de avisos: o hook segura a mensagem da pessoa.
const ESPERA_DOS_AVISOS: Duration = Duration::from_secs(5);

/// A fila dos pedidos para a interface.
pub type Fila = Live<Vec<Pedido>>;

/// Tira os pedidos da fila, sem rastrear. Com a fila vazia não mexe na versão: quem a rastreia
/// (o efeito da tela) não acorda de novo por causa da própria leitura.
pub fn tirar(fila: &Fila) -> Vec<Pedido> {
    if fila.with_untracked(Vec::is_empty) {
        return Vec::new();
    }
    let mut tirados = Vec::new();
    fila.update(|f| tirados = std::mem::take(f));
    tirados
}

/// O que se sabe de uma ponte ligada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessaoLigada {
    /// O número da ligação.
    pub ligacao: u64,
    /// O nome da sessão (seção 6.8 do plano).
    pub nome: String,
    /// O pid que a ponte declarou.
    pub pid: u32,
    /// O espaço de trabalho a que a ponte está presa.
    pub espaco: Option<String>,
    /// A chave da aba a que a ponte está presa (`--aba`, M63).
    pub aba: Option<String>,
    /// O lançamento do "Controlar com" que abriu o assistente (`--lancamento`).
    pub lancamento: Option<String>,
}

/// Por que o servidor não subiu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErroDoServidor {
    /// Outro Tirech Omni aberto já atende os assistentes.
    OutroApp,
    /// Qualquer outro motivo, em português.
    Falha(String),
}

impl ErroDoServidor {
    /// O texto para a pessoa.
    pub fn texto(&self) -> String {
        match self {
            Self::OutroApp => t!("mcp-servidor-outro-app"),
            Self::Falha(motivo) => t!("mcp-servidor-nao-subiu", erro = motivo.clone()),
        }
    }
}

/// Uma ponte ligada, do lado do servidor.
struct Entrada {
    sessao: SessaoLigada,
    /// Para derrubar a ligação de fora.
    fluxo: Fluxo,
    /// Os flags dos pedidos que a ponte fez e ainda não acabaram, pelo `id` em texto.
    cancelamentos: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

struct Estado {
    endereco: PathBuf,
    parar: AtomicBool,
    proxima: AtomicU64,
    ligacoes: Mutex<HashMap<u64, Entrada>>,
}

impl Estado {
    fn ligacoes(&self) -> MutexGuard<'_, HashMap<u64, Entrada>> {
        self.ligacoes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// O servidor, no ar enquanto este valor existe.
pub struct ServidorMcp {
    estado: Arc<Estado>,
}

impl ServidorMcp {
    /// Sobe o servidor em `endereco`; os pedidos vão para `fila`. Prepara a pasta (0700), remove
    /// um socket velho sem dono e deixa o socket em 0600.
    pub fn iniciar(endereco: &Path, fila: Fila) -> Result<Self, ErroDoServidor> {
        let escuta = escutar(endereco)?;
        let estado = Arc::new(Estado {
            endereco: endereco.to_owned(),
            parar: AtomicBool::new(false),
            proxima: AtomicU64::new(1),
            ligacoes: Mutex::new(HashMap::new()),
        });
        let aceitar = Arc::clone(&estado);
        thread::spawn(move || aceitar_ligacoes(escuta, &aceitar, &fila));
        Ok(Self { estado })
    }

    /// As pontes ligadas agora, da mais antiga para a mais nova.
    pub fn sessoes(&self) -> Vec<SessaoLigada> {
        let mut todas: Vec<_> = self
            .estado
            .ligacoes()
            .values()
            .map(|e| e.sessao.clone())
            .collect();
        todas.sort_by_key(|s| s.ligacao);
        todas
    }

    /// Derruba a ligação `ligacao` e cancela o que ela tinha em andamento.
    pub fn derrubar(&self, ligacao: u64) {
        if let Some(entrada) = self.estado.ligacoes().remove(&ligacao) {
            soltar(&entrada);
        }
    }

    /// Derruba todas as ligações ("Parar a IA"), com o servidor seguindo no ar.
    pub fn derrubar_todas(&self) {
        let todas: Vec<Entrada> = self.estado.ligacoes().drain().map(|(_, e)| e).collect();
        for entrada in &todas {
            soltar(entrada);
        }
    }
}

impl Drop for ServidorMcp {
    fn drop(&mut self) {
        self.estado.parar.store(true, Ordering::SeqCst);
        self.derrubar_todas();
        // A thread que aceita está parada em `accept`: uma ligação a acorda.
        let _ = canal::conectar(&self.estado.endereco);
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.estado.endereco);
    }
}

/// Fecha a ligação e cancela o que ela fez.
fn soltar(entrada: &Entrada) {
    for flag in entrada
        .cancelamentos
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
    {
        flag.store(true, Ordering::SeqCst);
    }
    let _ = entrada.fluxo.shutdown(std::net::Shutdown::Both);
}

#[cfg(unix)]
type Escuta = std::os::unix::net::UnixListener;

#[cfg(windows)]
type Escuta = super::pipe::Escuta;

/// Prepara a pasta e abre o socket.
#[cfg(unix)]
fn escutar(endereco: &Path) -> Result<Escuta, ErroDoServidor> {
    use std::os::unix::fs::{DirBuilderExt, FileTypeExt, PermissionsExt};
    let falha = |e: io::Error| ErroDoServidor::Falha(e.to_string());
    if !endereco.is_absolute() {
        return Err(ErroDoServidor::Falha(t!(
            "agente-endereco-invalido",
            endereco = endereco.display().to_string()
        )));
    }
    if let Some(pasta) = endereco.parent() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(pasta)
            .map_err(falha)?;
        // A pasta que já existia pode ter outra permissão (a de configuração, por exemplo).
        if std::fs::metadata(pasta)
            .map_err(falha)?
            .permissions()
            .mode()
            & 0o777
            != 0o700
        {
            std::fs::set_permissions(pasta, std::fs::Permissions::from_mode(0o700))
                .map_err(falha)?;
        }
    }
    // Um socket que ficou de um app que fechou mal: só some se ninguém responde nele.
    if let Ok(meta) = std::fs::symlink_metadata(endereco) {
        if !meta.file_type().is_socket() {
            return Err(ErroDoServidor::Falha(t!(
                "mcp-servidor-nao-e-socket",
                endereco = endereco.display().to_string()
            )));
        }
        match canal::conectar(endereco) {
            Ok(_) => return Err(ErroDoServidor::OutroApp),
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                std::fs::remove_file(endereco).map_err(falha)?;
            }
            Err(e) => return Err(falha(e)),
        }
    }
    let escuta = std::os::unix::net::UnixListener::bind(endereco).map_err(|e| {
        if e.kind() == io::ErrorKind::AddrInUse {
            ErroDoServidor::OutroApp
        } else {
            falha(e)
        }
    })?;
    std::fs::set_permissions(endereco, std::fs::Permissions::from_mode(0o600)).map_err(falha)?;
    Ok(escuta)
}

/// Abre o pipe com dono.
#[cfg(windows)]
fn escutar(endereco: &Path) -> Result<Escuta, ErroDoServidor> {
    Escuta::abrir(endereco)
}

#[cfg(unix)]
fn aceitar_ligacoes(escuta: Escuta, estado: &Arc<Estado>, fila: &Fila) {
    for fluxo in escuta.incoming() {
        if estado.parar.load(Ordering::SeqCst) {
            break;
        }
        let Ok(fluxo) = fluxo else { continue };
        let (estado, fila) = (Arc::clone(estado), fila.clone());
        thread::spawn(move || atender_ligacao(fluxo, None, &estado, &fila));
    }
}

#[cfg(windows)]
fn aceitar_ligacoes(escuta: Escuta, estado: &Arc<Estado>, fila: &Fila) {
    let (para_atender, fila) = (Arc::clone(estado), fila.clone());
    escuta.aceitar(
        || estado.parar.load(Ordering::SeqCst),
        move |fluxo, pid| atender_ligacao(fluxo, pid, &para_atender, &fila),
    );
}

/// A espera máxima pela abertura: uma ponte que liga e não diz nada não prende a thread.
const ESPERA_DA_ABERTURA: Duration = Duration::from_secs(10);

/// O nome de uma sessão nova. `pedido` é o `--nome`; sem ele vale o nome do espaço; sem os dois,
/// "IA {n}" com o menor `n` livre. Um nome em uso ganha " (2)", " (3)"…
pub fn nome_da_sessao(usados: &[String], pedido: Option<&str>, espaco: Option<&str>) -> String {
    let livre = |nome: &str| !usados.iter().any(|u| u == nome);
    let base = pedido.or(espaco).map(str::trim).filter(|n| !n.is_empty());
    match base {
        Some(base) if livre(base) => base.to_owned(),
        Some(base) => (2..)
            .map(|n| format!("{base} ({n})"))
            .find(|nome| livre(nome))
            .unwrap_or_else(|| base.to_owned()),
        None => (1..)
            .map(|n| format!("IA {n}"))
            .find(|nome| livre(nome))
            .unwrap_or_default(),
    }
}

/// Uma linha do canal para o JSON, ou `None` se não é JSON.
fn linha_json(linha: &str) -> Option<Value> {
    serde_json::from_str(linha).ok()
}

/// Atende uma ponte até ela sair. `pid_do_sistema` é o pid que o sistema leu do outro lado do
/// canal (só no Windows): se a ponte declara outro, a abertura é recusada.
fn atender_ligacao(fluxo: Fluxo, pid_do_sistema: Option<u32>, estado: &Arc<Estado>, fila: &Fila) {
    let Ok(leitura) = fluxo.try_clone() else {
        return;
    };
    let Ok(mut escrita) = fluxo.try_clone() else {
        return;
    };
    let Ok(para_derrubar) = fluxo.try_clone() else {
        return;
    };
    let _ = fluxo.set_read_timeout(Some(ESPERA_DA_ABERTURA));
    let mut leitor = BufReader::new(leitura);
    let mut linha = String::new();
    if leitor.read_line(&mut linha).unwrap_or(0) == 0 {
        return;
    }
    let _ = fluxo.set_read_timeout(None);
    let responder_abertura = |escrita: &mut Fluxo, resposta: &RespostaDaAbertura| {
        let _ = writeln!(escrita, "{}", json!(resposta)).and_then(|()| escrita.flush());
    };
    if let Ok(pedido) = serde_json::from_str::<PedidoDeAvisos>(&linha) {
        atender_avisos(&pedido, &mut escrita, estado, fila);
        return;
    }
    let abertura = match serde_json::from_str::<Abertura>(&linha) {
        Ok(a) if a.ponte == canal::VERSAO => a,
        _ => {
            responder_abertura(
                &mut escrita,
                &RespostaDaAbertura::recusada(t!("mcp-ponte-outra-versao")),
            );
            return;
        }
    };
    if pid_do_sistema.is_some_and(|pid| pid != abertura.pid) {
        responder_abertura(
            &mut escrita,
            &RespostaDaAbertura::recusada(t!("mcp-ponte-pid-diferente")),
        );
        return;
    }
    let cancelamentos: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>> = Arc::default();
    let numero = estado.proxima.fetch_add(1, Ordering::SeqCst);
    let sessao = {
        let mut ligacoes = estado.ligacoes();
        let usados: Vec<String> = ligacoes.values().map(|e| e.sessao.nome.clone()).collect();
        let nome = nome_da_sessao(
            &usados,
            abertura.nome.as_deref(),
            abertura.espaco.as_deref(),
        );
        let sessao = SessaoLigada {
            ligacao: numero,
            nome,
            pid: abertura.pid,
            espaco: abertura.espaco.clone(),
            aba: abertura.aba.clone(),
            lancamento: abertura.lancamento.clone(),
        };
        ligacoes.insert(
            numero,
            Entrada {
                sessao: sessao.clone(),
                fluxo: para_derrubar,
                cancelamentos: Arc::clone(&cancelamentos),
            },
        );
        sessao
    };
    responder_abertura(
        &mut escrita,
        &RespostaDaAbertura::aceita(sessao.nome.clone()),
    );
    // Quem escreve as respostas na ligação: os pedidos mandam por aqui, de qualquer thread.
    let (envia, recebe) = mpsc::channel::<Value>();
    thread::spawn(move || {
        for resposta in recebe {
            if writeln!(escrita, "{resposta}")
                .and_then(|()| escrita.flush())
                .is_err()
            {
                break;
            }
        }
    });
    for linha in leitor.lines() {
        let Ok(linha) = linha else { break };
        let Some(mensagem) = linha_json(&linha) else {
            continue;
        };
        if let Some(id) = mensagem.get("cancelar") {
            if let Some(flag) = cancelamentos
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&id.to_string())
            {
                flag.store(true, Ordering::SeqCst);
            }
            continue;
        }
        let (Some(id), Some(ferramenta)) = (mensagem.get("id"), mensagem["ferramenta"].as_str())
        else {
            continue;
        };
        if ferramenta == AVISOS {
            let mensagem = t!("mcp-ponte-ferramenta-desconhecida", nome = ferramenta);
            let _ =
                envia.send(json!({ "id": id, "erro": { "code": -32602, "message": mensagem } }));
            continue;
        }
        let cancelado = Arc::new(AtomicBool::new(false));
        cancelamentos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_string(), Arc::clone(&cancelado));
        let pedido = Pedido {
            ligacao: numero,
            sessao: sessao.nome.clone(),
            espaco: sessao.espaco.clone(),
            id: id.clone(),
            ferramenta: ferramenta.to_owned(),
            args: mensagem.get("args").cloned().unwrap_or_else(|| json!({})),
            pid: sessao.pid,
            cancelado,
            responder: envia.clone(),
        };
        fila.update(|f| f.push(pedido));
    }
    // A ponte saiu: o que ela deixou pendente não tem mais para quem responder. Se `derrubar` já
    // a tirou do mapa, os flags já foram cancelados.
    if let Some(entrada) = estado.ligacoes().remove(&numero) {
        soltar(&entrada);
    }
}

/// O pedido de avisos do hook: acha a sessão do lançamento, pede o texto à interface e responde
/// numa linha. Sem a sessão (a ponte ainda não ligou) ou sem resposta a tempo, o texto é vazio.
fn atender_avisos(pedido: &PedidoDeAvisos, escrita: &mut Fluxo, estado: &Estado, fila: &Fila) {
    let sessao = if pedido.versao == canal::VERSAO {
        estado
            .ligacoes()
            .values()
            .map(|e| &e.sessao)
            .find(|s| s.lancamento.as_deref() == Some(pedido.avisos.as_str()))
            .cloned()
    } else {
        None
    };
    let texto = sessao.and_then(|sessao| {
        let (envia, recebe) = mpsc::channel();
        let cancelado = Arc::new(AtomicBool::new(false));
        let pedido = Pedido {
            ligacao: sessao.ligacao,
            sessao: sessao.nome,
            espaco: sessao.espaco,
            id: Value::Null,
            ferramenta: AVISOS.to_owned(),
            args: json!({}),
            pid: sessao.pid,
            cancelado: Arc::clone(&cancelado),
            responder: envia,
        };
        fila.update(|f| f.push(pedido));
        let resposta = recebe.recv_timeout(ESPERA_DOS_AVISOS);
        cancelado.store(true, Ordering::SeqCst);
        resposta.ok()?["resultado"].as_str().map(str::to_owned)
    });
    let resposta = RespostaDosAvisos { texto };
    let _ = writeln!(escrita, "{}", json!(resposta)).and_then(|()| escrita.flush());
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_nome_da_sessao_usa_o_menor_numero_livre() {
        let usados = |nomes: &[&str]| nomes.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        assert_eq!(nome_da_sessao(&[], None, None), "IA 1");
        assert_eq!(nome_da_sessao(&usados(&["IA 1"]), None, None), "IA 2");
        // O nome de uma ponte que saiu volta a ficar livre.
        assert_eq!(
            nome_da_sessao(&usados(&["IA 2", "IA 3"]), None, None),
            "IA 1"
        );
    }

    #[test]
    fn o_nome_pedido_ou_o_do_espaco_vale_e_o_repetido_ganha_numero() {
        let usados = vec!["Deploy".to_owned()];
        assert_eq!(nome_da_sessao(&[], Some("Deploy"), None), "Deploy");
        assert_eq!(nome_da_sessao(&usados, Some("Deploy"), None), "Deploy (2)");
        assert_eq!(nome_da_sessao(&[], None, Some("api")), "api");
        // O `--nome` ganha do espaço.
        assert_eq!(nome_da_sessao(&[], Some("Logs"), Some("api")), "Logs");
        // Um nome em branco é como se não viesse.
        assert_eq!(nome_da_sessao(&[], Some("  "), None), "IA 1");
        let dois = vec!["Deploy".to_owned(), "Deploy (2)".to_owned()];
        assert_eq!(nome_da_sessao(&dois, Some("Deploy"), None), "Deploy (3)");
    }
}
