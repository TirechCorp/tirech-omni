//! O canal entre as cópias do app abertas na mesma pasta de dados. A primeira a abrir trava a
//! pasta e é a principal; as outras abrem só de leitura ([`crate::armazenamento::so_leitura`]) e
//! mandam para ela o que a pessoa muda nelas: as conexões, as configurações, os provedores de IA
//! e o cofre. O resto (o estado das janelas, os recentes, os espaços, as conversas) fica só na
//! memória da secundária, como antes.
//!
//! A VPN embutida (Nebula, OpenVPN, WireGuard, Tailscale, ZeroTier) também passa por aqui: só a
//! principal liga o motor de um perfil, e a secundária sai pela ponte dele ([`redes`]).
//!
//! - **A secundária junta:** ao gravar, lê o arquivo de agora, junta a mudança dela com o que a
//!   principal gravou desde a última leitura ([`mesclar`]: três vias, campo a campo e item a
//!   item; onde as duas mudaram a mesma coisa, fica a da principal) e manda o resultado. O cofre
//!   já se junta pelo [`crate::cofre::Cofre`] e vai cifrado: nenhum segredo passa aberto.
//! - **A principal só grava se o arquivo ainda é o que a secundária leu** (o SHA-256 dele), na
//!   thread da tela, e relê o documento na memória. Se mudou no meio, a secundária lê de novo,
//!   junta e manda outra vez.
//! - **O canal:** uma linha JSON de pedido ([`Pedido`]) e uma de resposta ([`Resposta`]) por
//!   ligação, num socket Unix só do usuário (o do [`crate::mcp::servidor`]: pasta `0700` e
//!   socket `0600`), ou num pipe com dono no Windows. Está sempre no ar na principal, com a IA
//!   ligada ou não.
//!
//! A secundária manda numa thread só dela, um pedido por vez, e não espera na thread da tela:
//! duas gravações seguidas do mesmo arquivo viram uma só. O que voltou da principal (a gravação,
//! os conflitos, uma falha) chega à tela por [`Eventos`].

pub mod mesclar;
pub mod redes;

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tomni_ui::files::Live;

use crate::mcp::canal::{self, Fluxo};
use crate::mcp::servidor::{ErroDoServidor, Escuta, escutar};
use crate::t;

pub use mesclar::{Mescla, mesclar};
pub use redes::{ChamadoDeRede, Emprestimo, FilaDeRedes};

/// A versão do canal. A principal recusa um pedido de outra.
pub const VERSAO: u64 = 1;

/// Quantas vezes a secundária junta e manda de novo quando a principal diz que o arquivo mudou.
const TENTATIVAS: usize = 5;

/// A espera pelo pedido, depois que a secundária liga.
const ESPERA_DO_PEDIDO: Duration = Duration::from_secs(10);

/// A espera pela tela da principal (ela atende entre um quadro e outro).
const ESPERA_DA_PRINCIPAL: Duration = Duration::from_secs(30);

/// A espera da secundária pela resposta: um pouco mais que a da principal pela tela dela.
const ESPERA_DA_RESPOSTA: Duration = Duration::from_secs(40);

/// Os documentos que a secundária manda para a principal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Documento {
    /// O `connections.json`.
    Conexoes,
    /// O `configuracoes.json`.
    Configuracoes,
    /// O `ia.json`.
    Ia,
    /// O `vault.json` (na pasta do cofre, que pode ser outra).
    Cofre,
}

impl Documento {
    /// Todos, para achar pelo nome.
    const TODOS: [Self; 4] = [Self::Conexoes, Self::Configuracoes, Self::Ia, Self::Cofre];

    /// O nome do arquivo.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Conexoes => crate::armazenamento::ARQUIVO_CONEXOES,
            Self::Configuracoes => "configuracoes.json",
            Self::Ia => "ia.json",
            Self::Cofre => "vault.json",
        }
    }

    /// O documento do arquivo de nome `nome`.
    pub fn do_nome(nome: &str) -> Option<Self> {
        Self::TODOS.into_iter().find(|d| d.nome() == nome)
    }

    /// Se a pasta dele é a do cofre (e não a de dados).
    pub fn na_pasta_do_cofre(self) -> bool {
        self == Self::Cofre
    }

    /// O JSON lido do arquivo como esta versão do app o grava: com os padrões dos campos que
    /// faltam e com o que ela não conhece ([`crate::armazenamento::valor_com_enxerto`]). A base,
    /// o disco e a memória se comparam nesta forma: um campo que só faltava no arquivo não vira
    /// mudança nem conflito.
    fn normalizar(self, lido: Value) -> Value {
        fn como<T: Serialize + serde::de::DeserializeOwned>(
            lido: Value,
            valor: Option<T>,
        ) -> Value {
            valor
                .and_then(|v| crate::armazenamento::valor_com_enxerto(&v, Some(&lido)).ok())
                .unwrap_or(lido)
        }
        match self {
            _ if lido.is_null() => lido,
            Self::Conexoes => {
                let arvore = serde_json::from_value::<crate::modelo::Tree>(lido.clone()).ok();
                como(lido, arvore)
            }
            Self::Configuracoes => {
                let config = lido
                    .as_object()
                    .map(|m| crate::configuracoes::mesclar(m.clone()).0);
                como(lido, config)
            }
            Self::Ia => {
                let ia =
                    serde_json::from_value::<crate::assistente::perfis::ArquivoIa>(lido.clone())
                        .ok();
                como(lido, ia)
            }
            Self::Cofre => lido,
        }
    }

    /// Dá outro id a um item repetido depois da junção (dois itens novos, um de cada lado, com o
    /// mesmo id). O item que já está no `disco` fica com o dele: a principal não vê os seus
    /// mudarem.
    fn renumerar(self, valor: &mut Value, disco: &Value) {
        match self {
            Self::Conexoes => renumerar_nos(valor, disco),
            Self::Ia => renumerar_perfis(valor),
            Self::Configuracoes | Self::Cofre => {}
        }
    }
}

/// O pedido de gravação, da secundária para a principal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pedido {
    /// A versão do canal ([`VERSAO`]).
    pub instancias: u64,
    /// O nome do arquivo ([`Documento::nome`]).
    pub documento: String,
    /// A pasta do arquivo, como a secundária a vê.
    pub pasta: PathBuf,
    /// O [`resumo`] do arquivo que a secundária leu e juntou (vazio: não havia arquivo).
    pub esperado: String,
    /// O conteúdo novo, inteiro.
    pub conteudo: String,
}

/// A resposta da principal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "resultado", rename_all = "snake_case")]
pub enum Resposta {
    /// Gravou e releu.
    Gravado,
    /// O arquivo não é mais o esperado: a secundária lê de novo e junta.
    Mudou,
    /// Não gravou, pelo motivo dado.
    Recusado {
        /// O motivo, para a pessoa.
        erro: String,
    },
}

/// O SHA-256 em hexadecimal de um arquivo lido; vazio se o arquivo não existe.
pub fn resumo(bytes: Option<&[u8]>) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    bytes.map_or_else(String::new, |bytes| {
        Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut h, b| {
                let _ = write!(h, "{b:02x}");
                h
            })
    })
}

/// O endereço do canal da pasta de dados `pasta`, pelas regras de [`endereco_em`].
pub fn endereco_da_pasta(pasta: &Path) -> PathBuf {
    let usuario = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let xdg = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(crate::mcp::pasta_de_execucao_do_usuario);
    let isolada = crate::armazenamento::variavel_da_pasta().is_some()
        || crate::armazenamento::modo_portatil()
        || pasta != crate::armazenamento::pasta();
    let desejado = endereco_em(
        cfg!(windows),
        cfg!(target_os = "macos"),
        xdg.as_deref(),
        pasta,
        &usuario,
        isolada,
    );
    if cfg!(windows) {
        desejado
    } else {
        crate::agente::endereco_que_cabe_aqui(desejado)
    }
}

/// O endereço do canal da pasta de dados `pasta`. No Windows, um pipe com o usuário e um resumo
/// da pasta no nome. No Unix, `instancias-{resumo}.sock` na pasta do `XDG_RUNTIME_DIR` (fora do
/// macOS), ou `instancias.sock` na própria pasta de dados se ela é `isolada` (trocada por
/// variável, a do modo portable, ou dada por um teste) ou se não há aquela pasta. Cada pasta de
/// dados tem a sua principal.
pub fn endereco_em(
    windows: bool,
    macos: bool,
    xdg_runtime_dir: Option<&Path>,
    pasta: &Path,
    usuario: &str,
    isolada: bool,
) -> PathBuf {
    let resumo = crate::agente::resumo_do_caminho(pasta);
    if windows {
        return PathBuf::from(format!(
            r"\\.\pipe\tirech-omni-instancias-{usuario}-{resumo}"
        ));
    }
    match xdg_runtime_dir {
        Some(xdg) if !isolada && !macos && xdg.is_absolute() => xdg
            .join("tirech-omni")
            .join(format!("instancias-{resumo}.sock")),
        _ => pasta.join("instancias.sock"),
    }
}

// ---------------------------------------------------------------------------------------------
// A principal.

/// Um pedido de uma secundária, a caminho da tela da principal.
#[derive(Debug)]
pub struct PedidoDaSecundaria {
    /// O que ela pediu.
    pub pedido: Pedido,
    responder: Sender<Resposta>,
}

impl PedidoDaSecundaria {
    /// Responde à secundária (que espera na thread da ligação).
    pub fn responder(self, resposta: Resposta) {
        let _ = self.responder.send(resposta);
    }
}

/// A fila dos pedidos para a tela da principal.
pub type Fila = Live<Vec<PedidoDaSecundaria>>;

/// Tira o que há na fila (os pedidos, na principal; os eventos, na secundária), sem rastrear
/// (como o [`crate::mcp::servidor::tirar`]).
pub fn tirar<T>(fila: &Live<Vec<T>>) -> Vec<T> {
    if fila.with_untracked(Vec::is_empty) {
        return Vec::new();
    }
    let mut tirados = Vec::new();
    fila.update(|f| tirados = std::mem::take(f));
    tirados
}

/// O canal da principal, no ar enquanto este valor existe. Outra tela deste processo na mesma
/// pasta (uma janela nova depois de a primeira fechar sem soltar a tela, ou os testes) toma o canal
/// para si: os pedidos passam a ir para a fila dela.
pub struct ServidorDasInstancias {
    endereco: PathBuf,
    no_ar: Arc<NoAr>,
    vez: u64,
}

/// Um canal deste processo: a fila da tela que o tem agora e quem é ela.
struct NoAr {
    fila: Mutex<Fila>,
    redes: Mutex<FilaDeRedes>,
    dono: AtomicU64,
    parar: AtomicBool,
}

/// Os canais que este processo atende, pelo endereço.
fn canais_no_ar() -> MutexGuard<'static, HashMap<PathBuf, Arc<NoAr>>> {
    static CANAIS: OnceLock<Mutex<HashMap<PathBuf, Arc<NoAr>>>> = OnceLock::new();
    CANAIS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

impl ServidorDasInstancias {
    /// Sobe o canal em `endereco`; os pedidos de gravação vão para `fila` e os de rede
    /// ([`redes`]), para `redes`. O erro vem em texto para a pessoa.
    pub fn iniciar(endereco: &Path, fila: Fila, redes: FilaDeRedes) -> Result<Self, String> {
        static VEZ: AtomicU64 = AtomicU64::new(0);
        let vez = VEZ.fetch_add(1, Ordering::SeqCst);
        let mut canais = canais_no_ar();
        if let Some(no_ar) = canais.get(endereco) {
            *no_ar.fila.lock().unwrap_or_else(PoisonError::into_inner) = fila;
            *no_ar.redes.lock().unwrap_or_else(PoisonError::into_inner) = redes;
            no_ar.dono.store(vez, Ordering::SeqCst);
            return Ok(Self {
                endereco: endereco.to_owned(),
                no_ar: Arc::clone(no_ar),
                vez,
            });
        }
        let escuta = escutar(endereco).map_err(|erro| match erro {
            ErroDoServidor::OutroApp => t!("instancias-outra-principal"),
            ErroDoServidor::Falha(motivo) => motivo,
        })?;
        let no_ar = Arc::new(NoAr {
            fila: Mutex::new(fila),
            redes: Mutex::new(redes),
            dono: AtomicU64::new(vez),
            parar: AtomicBool::new(false),
        });
        canais.insert(endereco.to_owned(), Arc::clone(&no_ar));
        let atende = Arc::clone(&no_ar);
        thread::spawn(move || aceitar(escuta, &atende));
        Ok(Self {
            endereco: endereco.to_owned(),
            no_ar,
            vez,
        })
    }
}

impl Drop for ServidorDasInstancias {
    fn drop(&mut self) {
        let mut canais = canais_no_ar();
        // Outra tela tomou o canal: ele fica no ar, com ela.
        if self.no_ar.dono.load(Ordering::SeqCst) != self.vez {
            return;
        }
        canais.remove(&self.endereco);
        self.no_ar.parar.store(true, Ordering::SeqCst);
        // A thread que aceita está parada em `accept`: uma ligação a acorda.
        let _ = canal::conectar(&self.endereco);
        #[cfg(unix)]
        let _ = fs::remove_file(&self.endereco);
    }
}

#[cfg(unix)]
fn aceitar(escuta: Escuta, no_ar: &Arc<NoAr>) {
    for fluxo in escuta.incoming() {
        if no_ar.parar.load(Ordering::SeqCst) {
            break;
        }
        let Ok(fluxo) = fluxo else { continue };
        let no_ar = Arc::clone(no_ar);
        thread::spawn(move || atender(fluxo, &no_ar));
    }
}

#[cfg(windows)]
fn aceitar(escuta: Escuta, no_ar: &Arc<NoAr>) {
    let atende = Arc::clone(no_ar);
    escuta.aceitar(
        || no_ar.parar.load(Ordering::SeqCst),
        move |fluxo, _| atender(fluxo, &atende),
    );
}

/// Atende uma ligação: lê o pedido, espera a tela e responde. Um pedido de rede segue em
/// [`redes::atender`].
fn atender(fluxo: Fluxo, no_ar: &NoAr) {
    let _ = fluxo.set_read_timeout(Some(ESPERA_DO_PEDIDO));
    let Ok(leitura) = fluxo.try_clone() else {
        return;
    };
    let mut escrita = fluxo;
    let mut linha = String::new();
    let mut leitura = BufReader::new(leitura);
    if leitura.read_line(&mut linha).unwrap_or(0) == 0 {
        return;
    }
    if let Ok(pedido) = serde_json::from_str::<redes::PedidoDeRede>(&linha)
        && pedido.instancias == VERSAO
    {
        let fila = no_ar
            .redes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        redes::atender(&pedido, leitura, escrita, &fila);
        return;
    }
    let resposta = match serde_json::from_str::<Pedido>(&linha) {
        Ok(pedido) if pedido.instancias == VERSAO => {
            let (responder, resposta) = mpsc::channel();
            let fila = no_ar
                .fila
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            fila.update(|f| f.push(PedidoDaSecundaria { pedido, responder }));
            resposta
                .recv_timeout(ESPERA_DA_PRINCIPAL)
                .unwrap_or_else(|_| Resposta::Recusado {
                    erro: t!("instancias-sem-resposta"),
                })
        }
        _ => Resposta::Recusado {
            erro: t!("instancias-outra-versao"),
        },
    };
    let _ = writeln!(escrita, "{}", json!(resposta)).and_then(|()| escrita.flush());
}

/// Se `conteudo` serve como o `documento`: um JSON que esta versão do app lê. O cofre a tela
/// confere pelo [`crate::cofre::Cofre::aceita_de_outra_copia`].
pub fn validar(documento: Documento, conteudo: &[u8]) -> Result<(), String> {
    let valor = serde_json::from_slice::<Value>(conteudo).map_err(|e| e.to_string())?;
    match documento {
        Documento::Conexoes => {
            if crate::armazenamento::versao_do_json(&valor) > crate::modelo::VERSAO {
                return Err(t!("armazenamento-conexoes-versao-mais-nova"));
            }
            serde_json::from_value::<crate::modelo::Tree>(valor)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        Documento::Configuracoes => valor
            .is_object()
            .then_some(())
            .ok_or_else(|| t!("instancias-nao-e-objeto")),
        Documento::Ia => serde_json::from_value::<crate::assistente::perfis::ArquivoIa>(valor)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Documento::Cofre => Ok(()),
    }
}

/// Grava `conteudo` em `arquivo` se ele ainda é o de [`resumo`] `esperado` (atômico; com
/// `privado`, só do dono no Unix). É o que a principal faz com um pedido, na thread da tela.
pub fn gravar_se_ainda_for(
    arquivo: &Path,
    esperado: &str,
    conteudo: &[u8],
    privado: bool,
) -> Resposta {
    let agora = match fs::read(arquivo) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => {
            return Resposta::Recusado {
                erro: e.to_string(),
            };
        }
    };
    if resumo(agora.as_deref()) != esperado {
        return Resposta::Mudou;
    }
    let gravado = arquivo
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| crate::armazenamento::gravar_atomico(arquivo, conteudo, privado));
    match gravado {
        Ok(()) => Resposta::Gravado,
        Err(e) => Resposta::Recusado {
            erro: e.to_string(),
        },
    }
}

/// Se `a` e `b` são a mesma pasta (como vieram, ou com os links resolvidos).
pub fn mesma_pasta(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

// ---------------------------------------------------------------------------------------------
// A secundária.

/// O que voltou da principal, para a tela da secundária.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Evento {
    /// A principal gravou o documento. Com `adotar`, o que ficou no disco não é o que a
    /// secundária tem na memória (a principal tinha mudado outra coisa, ou ganhou um conflito):
    /// a tela relê.
    Gravado {
        /// O documento.
        documento: Documento,
        /// Se a tela relê.
        adotar: bool,
    },
    /// As duas mudaram a mesma coisa, e ficou a da principal: os caminhos, para o aviso.
    Conflitos(Vec<String>),
    /// Não foi: o motivo, para o aviso. A mudança fica só na memória da secundária.
    Falhou(String),
    /// A principal não atende (fechou): o motivo, avisado uma vez até ela voltar a atender.
    SemPrincipal(String),
}

/// A fila dos eventos para a tela da secundária.
pub type Eventos = Live<Vec<Evento>>;

/// Uma pasta de dados cuja principal é outra cópia do app.
struct Seguida {
    dados: PathBuf,
    /// A de dados e a do cofre.
    pastas: Vec<PathBuf>,
    endereco: PathBuf,
    eventos: Option<Eventos>,
    /// Se a falta da principal já foi avisada (uma vez, até ela voltar a responder).
    avisou_sem_principal: bool,
}

static SEGUIDAS: Mutex<Vec<Seguida>> = Mutex::new(Vec::new());

fn seguidas() -> MutexGuard<'static, Vec<Seguida>> {
    SEGUIDAS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Esta cópia do app é secundária na pasta de dados `dados`: o que ela grava ali vai para a
/// principal. Chame antes de ler os arquivos, para a leitura anotar a base da junção.
pub fn seguir_a_principal(dados: &Path) {
    let mut lista = seguidas();
    if lista.iter().any(|s| s.dados == dados) {
        return;
    }
    lista.push(Seguida {
        dados: dados.to_owned(),
        pastas: vec![dados.to_owned()],
        endereco: endereco_da_pasta(dados),
        eventos: None,
        avisou_sem_principal: false,
    });
}

/// A pasta do cofre (`pasta`) da cópia secundária de `dados` também vai para a principal.
pub fn seguir_tambem(dados: &Path, pasta: &Path) {
    if let Some(s) = seguidas().iter_mut().find(|s| s.dados == dados)
        && !s.pastas.iter().any(|p| p == pasta)
    {
        s.pastas.push(pasta.to_owned());
    }
}

/// Os eventos da secundária de `dados` vão para `eventos`.
pub fn ouvir(dados: &Path, eventos: Eventos) {
    if let Some(s) = seguidas().iter_mut().find(|s| s.dados == dados) {
        s.eventos = Some(eventos);
    }
}

/// O destino de um arquivo de uma secundária: o documento, a pasta de dados e o endereço.
fn destino(arquivo: &Path) -> Option<(Documento, PathBuf, PathBuf)> {
    let documento = Documento::do_nome(arquivo.file_name()?.to_str()?)?;
    let pasta = arquivo.parent()?;
    let lista = seguidas();
    let seguida = lista.iter().find(|s| {
        if documento.na_pasta_do_cofre() {
            s.pastas.iter().any(|p| mesma_pasta(p, pasta))
        } else {
            mesma_pasta(&s.dados, pasta)
        }
    })?;
    Some((documento, seguida.dados.clone(), seguida.endereco.clone()))
}

/// Se o que se grava em `arquivo` vai para a principal (esta cópia é secundária ali, e o arquivo
/// é um dos [`Documento`]).
pub fn segue_a_principal(arquivo: &Path) -> bool {
    destino(arquivo).is_some()
}

fn avisar(dados: &Path, evento: Evento) {
    let mut lista = seguidas();
    let Some(seguida) = lista.iter_mut().find(|s| s.dados == dados) else {
        return;
    };
    match &evento {
        Evento::SemPrincipal(_) if seguida.avisou_sem_principal => return,
        Evento::SemPrincipal(_) => seguida.avisou_sem_principal = true,
        Evento::Gravado { .. } => seguida.avisou_sem_principal = false,
        _ => {}
    }
    if let Some(eventos) = &seguida.eventos {
        eventos.update(|e| e.push(evento));
    }
}

/// O que há para mandar de um arquivo.
#[derive(Clone, Debug)]
enum Conteudo {
    /// O JSON da memória da secundária, para juntar com o disco.
    Json(Value),
    /// Pronto (o cofre, já junto e cifrado): vai se o disco ainda for o `esperado`.
    Pronto { esperado: String, bytes: Vec<u8> },
}

#[derive(Default)]
struct Trabalhos {
    /// Os arquivos com algo a mandar, na ordem.
    ordem: VecDeque<PathBuf>,
    /// O último conteúdo de cada um: duas gravações seguidas viram uma.
    conteudos: HashMap<PathBuf, Conteudo>,
    /// O que está indo agora.
    indo: HashSet<PathBuf>,
    /// A base da junção de cada arquivo: o que a memória da secundária tem de igual ao disco
    /// (o que ela leu, ou o que mandou por último).
    bases: HashMap<PathBuf, Value>,
}

static TRABALHOS: OnceLock<(Mutex<Trabalhos>, Condvar)> = OnceLock::new();

fn trabalhos() -> &'static (Mutex<Trabalhos>, Condvar) {
    TRABALHOS.get_or_init(|| {
        thread::Builder::new()
            .name("instancias".into())
            .spawn(mandar_sempre)
            .ok();
        (Mutex::new(Trabalhos::default()), Condvar::new())
    })
}

fn travar_trabalhos() -> MutexGuard<'static, Trabalhos> {
    trabalhos().0.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Anota o que a secundária leu de `arquivo` (`None`: não existe), a base da próxima junção.
/// Fora de uma secundária, ou com uma mudança dela a caminho (a junção dessa mudança é com o que
/// ela tinha, e o envio anota a base ao terminar), não faz nada.
pub fn anotar_base(arquivo: &Path, bytes: Option<&[u8]>) {
    let Some(documento) = destino(arquivo).map(|(d, ..)| d) else {
        return;
    };
    let valor = match bytes {
        None => Value::Null,
        Some(bytes) => match serde_json::from_slice(bytes) {
            Ok(valor) => documento.normalizar(valor),
            Err(_) => return,
        },
    };
    let mut lista = travar_trabalhos();
    if !lista.conteudos.contains_key(arquivo) && !lista.indo.contains(arquivo) {
        lista.bases.insert(arquivo.to_owned(), valor);
    }
}

/// Manda para a principal o JSON `valor` de `arquivo`, juntado com o disco. Fora de uma
/// secundária, não faz nada.
pub fn enviar_json(arquivo: &Path, valor: Value) {
    enfileirar(arquivo, Conteudo::Json(valor));
}

/// Manda `bytes` para a principal gravar em `arquivo`, se o disco ainda for `lido`. Fora de uma
/// secundária, não faz nada.
pub fn enviar_pronto(arquivo: &Path, lido: &[u8], bytes: Vec<u8>) {
    let esperado = resumo((!lido.is_empty()).then_some(lido));
    enfileirar(arquivo, Conteudo::Pronto { esperado, bytes });
}

fn enfileirar(arquivo: &Path, conteudo: Conteudo) {
    if !segue_a_principal(arquivo) {
        return;
    }
    let mut lista = travar_trabalhos();
    if lista
        .conteudos
        .insert(arquivo.to_owned(), conteudo)
        .is_none()
    {
        lista.ordem.push_back(arquivo.to_owned());
    }
    drop(lista);
    trabalhos().1.notify_all();
}

/// Se há algo de `arquivo` a caminho da principal: a vigia não relê o arquivo antes, para não
/// trocar a memória por um disco que ainda não tem a mudança.
pub fn pendente(arquivo: &Path) -> bool {
    let lista = travar_trabalhos();
    lista.conteudos.contains_key(arquivo) || lista.indo.contains(arquivo)
}

/// Espera, até `limite`, não haver nada a caminho da principal. Para os testes.
pub fn esperar_pendentes(limite: Duration) -> bool {
    let (mutex, condvar) = trabalhos();
    let lista = mutex.lock().unwrap_or_else(PoisonError::into_inner);
    let (lista, _) = condvar
        .wait_timeout_while(lista, limite, |l| {
            !l.conteudos.is_empty() || !l.indo.is_empty()
        })
        .unwrap_or_else(PoisonError::into_inner);
    lista.conteudos.is_empty() && lista.indo.is_empty()
}

/// A thread que manda: um arquivo por vez, na ordem.
fn mandar_sempre() {
    let (mutex, condvar) = trabalhos();
    loop {
        let (arquivo, conteudo) = {
            let mut lista = condvar
                .wait_while(mutex.lock().unwrap_or_else(PoisonError::into_inner), |l| {
                    l.ordem.is_empty()
                })
                .unwrap_or_else(PoisonError::into_inner);
            let Some(arquivo) = lista.ordem.pop_front() else {
                continue;
            };
            let Some(conteudo) = lista.conteudos.remove(&arquivo) else {
                continue;
            };
            lista.indo.insert(arquivo.clone());
            (arquivo, conteudo)
        };
        if let Some((documento, dados, endereco)) = destino(&arquivo) {
            let eventos = match conteudo {
                Conteudo::Json(valor) => mandar_json(&arquivo, documento, &endereco, &valor),
                Conteudo::Pronto { esperado, bytes } => {
                    mandar_pronto(&arquivo, documento, &endereco, esperado, bytes)
                }
            };
            for evento in eventos {
                avisar(&dados, evento);
            }
        }
        mutex
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .indo
            .remove(&arquivo);
        condvar.notify_all();
    }
}

/// Liga na principal, manda `pedido` e lê a resposta.
pub fn mandar(endereco: &Path, pedido: &Pedido) -> io::Result<Resposta> {
    let mut fluxo = canal::conectar(endereco)?;
    fluxo.set_read_timeout(Some(ESPERA_DA_RESPOSTA))?;
    let linha = serde_json::to_string(pedido).map_err(io::Error::other)?;
    writeln!(fluxo, "{linha}")?;
    fluxo.flush()?;
    let mut resposta = String::new();
    BufReader::new(fluxo).read_line(&mut resposta)?;
    serde_json::from_str(&resposta).map_err(io::Error::other)
}

fn sem_principal(erro: &io::Error) -> Evento {
    Evento::SemPrincipal(t!("instancias-sem-principal", erro = erro.to_string()))
}

fn pedido(arquivo: &Path, documento: Documento, esperado: String, conteudo: String) -> Pedido {
    Pedido {
        instancias: VERSAO,
        documento: documento.nome().to_owned(),
        pasta: arquivo.parent().map(Path::to_owned).unwrap_or_default(),
        esperado,
        conteudo,
    }
}

/// Junta `nosso` com o disco e manda, de novo enquanto a principal disser que o arquivo mudou.
fn mandar_json(
    arquivo: &Path,
    documento: Documento,
    endereco: &Path,
    nosso: &Value,
) -> Vec<Evento> {
    for _ in 0..TENTATIVAS {
        let lido = match fs::read(arquivo) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return vec![Evento::Falhou(e.to_string())],
        };
        let mut disco = match &lido {
            None => Value::Null,
            Some(bytes) => match serde_json::from_slice::<Value>(bytes) {
                Ok(valor) => documento.normalizar(valor),
                Err(e) => return vec![Evento::Falhou(e.to_string())],
            },
        };
        let mut base = travar_trabalhos()
            .bases
            .get(arquivo)
            .cloned()
            .unwrap_or(Value::Null);
        let Mescla {
            mut valor,
            conflitos,
        } = if base == disco {
            Mescla {
                valor: nosso.clone(),
                conflitos: Vec::new(),
            }
        } else {
            // O próximo id não é um campo da pessoa: fica o maior dos três, e não dá conflito.
            let mut nosso = nosso.clone();
            let proximo = [&mut base, &mut disco, &mut nosso]
                .into_iter()
                .filter_map(|v| v.as_object_mut()?.remove("next_id")?.as_u64())
                .max();
            let mut mescla = mesclar(&base, &disco, &nosso);
            if let (Some(proximo), Some(o)) = (proximo, mescla.valor.as_object_mut()) {
                o.insert("next_id".into(), json!(proximo));
            }
            if let (Some(proximo), Some(o)) = (proximo, disco.as_object_mut()) {
                o.insert("next_id".into(), json!(proximo));
            }
            mescla
        };
        documento.renumerar(&mut valor, &disco);
        let mut eventos = Vec::new();
        if !conflitos.is_empty() {
            eventos.push(Evento::Conflitos(conflitos));
        }
        let gravado = Evento::Gravado {
            documento,
            adotar: valor != *nosso,
        };
        if valor == disco {
            travar_trabalhos()
                .bases
                .insert(arquivo.to_owned(), nosso.clone());
            if valor != *nosso {
                eventos.push(gravado);
            }
            return eventos;
        }
        let texto = match serde_json::to_string_pretty(&valor) {
            Ok(texto) => texto,
            Err(e) => return vec![Evento::Falhou(e.to_string())],
        };
        let pedido = pedido(arquivo, documento, resumo(lido.as_deref()), texto);
        match mandar(endereco, &pedido) {
            Ok(Resposta::Gravado) => {
                travar_trabalhos()
                    .bases
                    .insert(arquivo.to_owned(), nosso.clone());
                eventos.push(gravado);
                return eventos;
            }
            Ok(Resposta::Mudou) => {}
            Ok(Resposta::Recusado { erro }) => return vec![Evento::Falhou(erro)],
            Err(e) => return vec![sem_principal(&e)],
        }
    }
    vec![Evento::Falhou(t!("instancias-mudou-sempre"))]
}

/// Manda o arquivo pronto uma vez. Se o disco mudou, quem o fez (o cofre) junta de novo na
/// conferência periódica e manda outra vez.
fn mandar_pronto(
    arquivo: &Path,
    documento: Documento,
    endereco: &Path,
    esperado: String,
    bytes: Vec<u8>,
) -> Vec<Evento> {
    let Ok(texto) = String::from_utf8(bytes) else {
        return Vec::new();
    };
    match mandar(endereco, &pedido(arquivo, documento, esperado, texto)) {
        Ok(Resposta::Gravado) => vec![Evento::Gravado {
            documento,
            adotar: false,
        }],
        Ok(Resposta::Mudou) => Vec::new(),
        Ok(Resposta::Recusado { erro }) => vec![Evento::Falhou(erro)],
        Err(e) => vec![sem_principal(&e)],
    }
}

/// Os nós da árvore (`nodes`, e os `children` dos grupos) com o id de cada um só uma vez. Fica
/// com o id o nó que já está no `disco` (o mesmo tipo e id), e senão o primeiro; os outros ganham
/// o próximo livre, e o `next_id` passa do maior.
fn renumerar_nos(arvore: &mut Value, disco: &Value) {
    fn dentro(no: &mut Value) -> Option<&mut serde_json::Map<String, Value>> {
        no.as_object_mut()?.values_mut().next()?.as_object_mut()
    }
    /// A identidade de cada nó (`Tipo:id`), na ordem da visita.
    fn identidades(nos: &[Value], lista: &mut Vec<Option<(String, u64)>>) {
        for no in nos {
            let par = no.as_object().and_then(|o| o.iter().next());
            let id = par.and_then(|(tipo, o)| Some((tipo.clone(), o.get("id")?.as_u64()?)));
            lista.push(id);
            if let Some(filhos) = par.and_then(|(_, o)| o.get("children")?.as_array()) {
                identidades(filhos, lista);
            }
        }
    }
    fn maior(nos: &[Value]) -> u64 {
        nos.iter()
            .filter_map(|no| no.as_object()?.values().next()?.as_object())
            .map(|o| {
                let id = o.get("id").and_then(Value::as_u64).unwrap_or(0);
                let filhos = o
                    .get("children")
                    .and_then(Value::as_array)
                    .map_or(0, |f| maior(f));
                id.max(filhos)
            })
            .max()
            .unwrap_or(0)
    }
    fn visitar(
        nos: &mut [Value],
        fica: &mut impl Iterator<Item = bool>,
        vistos: &mut HashSet<u64>,
        proximo: &mut u64,
    ) {
        for no in nos {
            let fica_com_o_dele = fica.next().unwrap_or(false);
            let Some(o) = dentro(no) else { continue };
            if let Some(id) = o.get("id").and_then(Value::as_u64)
                && !fica_com_o_dele
                && !vistos.insert(id)
            {
                o.insert("id".into(), json!(*proximo));
                vistos.insert(*proximo);
                *proximo += 1;
            }
            if let Some(Value::Array(filhos)) = o.get_mut("children") {
                visitar(filhos, fica, vistos, proximo);
            }
        }
    }
    let nos_de = |v: &Value| v.get("nodes").and_then(Value::as_array).cloned();
    let mut do_disco = Vec::new();
    identidades(&nos_de(disco).unwrap_or_default(), &mut do_disco);
    let do_disco: HashSet<(String, u64)> = do_disco.into_iter().flatten().collect();
    let mut nossas = Vec::new();
    identidades(&nos_de(arvore).unwrap_or_default(), &mut nossas);
    // Os que ficam com o id: o primeiro de cada identidade que o disco tem.
    let mut reservados = HashSet::new();
    let fica: Vec<bool> = nossas
        .into_iter()
        .map(|id| id.is_some_and(|id| do_disco.contains(&id) && reservados.insert(id.1)))
        .collect();
    let Some(raiz) = arvore.as_object_mut() else {
        return;
    };
    let maior = raiz
        .get("nodes")
        .and_then(Value::as_array)
        .map_or(0, |n| maior(n));
    let mut proximo = raiz
        .get("next_id")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .max(maior + 1);
    if let Some(Value::Array(nos)) = raiz.get_mut("nodes") {
        visitar(nos, &mut fica.into_iter(), &mut reservados, &mut proximo);
    }
    raiz.insert("next_id".into(), json!(proximo));
}

/// Os provedores de IA (`perfis`) com o id de cada um só uma vez.
fn renumerar_perfis(arquivo: &mut Value) {
    let Some(Value::Array(perfis)) = arquivo.get_mut("perfis") else {
        return;
    };
    let mut proximo = perfis
        .iter()
        .filter_map(|p| p.get("id").and_then(Value::as_u64))
        .max()
        .unwrap_or(0)
        + 1;
    let mut vistos = HashSet::new();
    for perfil in perfis {
        if let Some(id) = perfil.get("id").and_then(Value::as_u64)
            && !vistos.insert(id)
            && let Some(o) = perfil.as_object_mut()
        {
            o.insert("id".into(), json!(proximo));
            vistos.insert(proximo);
            proximo += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn outra_tela_do_processo_toma_o_canal_da_mesma_pasta() {
        let pasta = std::env::temp_dir().join(format!("tirech-instancias-{}", std::process::id()));
        let endereco = pasta.join("instancias.sock");
        let primeira: Fila = Live::detached(Vec::new());
        let segunda: Fila = Live::detached(Vec::new());
        let servidor =
            ServidorDasInstancias::iniciar(&endereco, primeira.clone(), Live::detached(Vec::new()))
                .unwrap();
        let outro =
            ServidorDasInstancias::iniciar(&endereco, segunda.clone(), Live::detached(Vec::new()))
                .unwrap();
        // A primeira tela se vai depois: o canal fica com a segunda.
        drop(servidor);
        let pedido = Pedido {
            instancias: VERSAO,
            documento: "connections.json".into(),
            pasta: pasta.clone(),
            esperado: resumo(None),
            conteudo: "{}".into(),
        };
        let ida = {
            let endereco = endereco.clone();
            thread::spawn(move || mandar(&endereco, &pedido))
        };
        let limite = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(p) = tirar(&segunda).pop() {
                p.responder(Resposta::Gravado);
                break;
            }
            assert!(std::time::Instant::now() < limite, "o pedido não chegou");
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(ida.join().unwrap().unwrap(), Resposta::Gravado);
        assert!(tirar(&primeira).is_empty());
        drop(outro);
        assert!(!endereco.exists());
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn o_endereco_e_um_por_pasta_de_dados() {
        let xdg = Path::new("/run/user/1000");
        let a = endereco_em(
            false,
            false,
            Some(xdg),
            Path::new("/home/ana/.config/tirech-omni"),
            "ana",
            false,
        );
        let b = endereco_em(false, false, Some(xdg), Path::new("/outra"), "ana", false);
        assert!(a.starts_with("/run/user/1000/tirech-omni"));
        assert_ne!(a, b);
        assert_eq!(
            endereco_em(false, false, Some(xdg), Path::new("/portatil"), "ana", true),
            Path::new("/portatil/instancias.sock")
        );
        assert_eq!(
            endereco_em(false, true, Some(xdg), Path::new("/mac"), "ana", false),
            Path::new("/mac/instancias.sock")
        );
        let pipe = endereco_em(true, false, None, Path::new("C:/dados"), "ana", false);
        assert!(
            pipe.to_string_lossy()
                .starts_with(r"\\.\pipe\tirech-omni-instancias-ana-")
        );
    }

    #[test]
    fn o_resumo_de_um_arquivo_que_nao_existe_e_vazio() {
        assert_eq!(resumo(None), "");
        assert_eq!(resumo(Some(b"")).len(), 64);
        assert_ne!(resumo(Some(b"a")), resumo(Some(b"b")));
    }

    #[test]
    fn ids_repetidos_na_arvore_ganham_outro() {
        let mut arvore = json!({"version": 1, "next_id": 3, "nodes": [
            {"Group": {"id": 1, "name": "g", "children": [
                {"Connection": {"id": 2, "name": "de lá"}},
                {"Connection": {"id": 2, "name": "daqui"}},
            ]}},
            {"Connection": {"id": 1, "name": "outra"}},
        ]});
        renumerar_nos(&mut arvore, &Value::Null);
        assert_eq!(
            arvore["nodes"][0]["Group"]["children"][0]["Connection"]["id"],
            2
        );
        assert_eq!(
            arvore["nodes"][0]["Group"]["children"][1]["Connection"]["id"],
            3
        );
        assert_eq!(arvore["nodes"][1]["Connection"]["id"], 4);
        assert_eq!(arvore["next_id"], 5);
    }

    #[test]
    fn o_no_que_ja_esta_no_disco_fica_com_o_id() {
        let disco = json!({"next_id": 7, "nodes": [{"Group": {"id": 6, "name": "de lá"}}]});
        let mut arvore = json!({"next_id": 7, "nodes": [
            {"Connection": {"id": 6, "name": "daqui"}},
            {"Group": {"id": 6, "name": "de lá"}},
        ]});
        renumerar_nos(&mut arvore, &disco);
        assert_eq!(arvore["nodes"][0]["Connection"]["id"], 7);
        assert_eq!(arvore["nodes"][1]["Group"]["id"], 6);
        assert_eq!(arvore["next_id"], 8);
    }

    #[test]
    fn ids_repetidos_nos_provedores_ganham_outro() {
        let mut arquivo = json!({"perfis": [{"id": 1}, {"id": 2}, {"id": 2}]});
        renumerar_perfis(&mut arquivo);
        assert_eq!(arquivo["perfis"][2]["id"], 3);
    }

    #[test]
    fn a_principal_so_grava_se_o_arquivo_ainda_e_o_lido() {
        let pasta = crate::armazenamento::testes::PastaTemporaria::nova("instancias-cas");
        fs::create_dir_all(&pasta.0).unwrap();
        let arquivo = pasta.0.join("ia.json");
        assert_eq!(
            gravar_se_ainda_for(&arquivo, "", b"{}", false),
            Resposta::Gravado
        );
        assert_eq!(
            gravar_se_ainda_for(&arquivo, "", b"{\"a\":1}", false),
            Resposta::Mudou,
            "o arquivo já existe"
        );
        let lido = fs::read(&arquivo).unwrap();
        assert_eq!(
            gravar_se_ainda_for(&arquivo, &resumo(Some(&lido)), b"{\"a\":1}", false),
            Resposta::Gravado
        );
        assert_eq!(fs::read(&arquivo).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn so_os_documentos_validos_passam() {
        assert!(validar(Documento::Configuracoes, b"{}").is_ok());
        assert!(validar(Documento::Configuracoes, b"[]").is_err());
        assert!(validar(Documento::Conexoes, br#"{"version": 1, "nodes": []}"#).is_ok());
        assert!(validar(Documento::Conexoes, br#"{"version": 999, "nodes": []}"#).is_err());
        assert!(validar(Documento::Ia, b"nada").is_err());
    }

    #[test]
    fn o_pedido_e_a_resposta_sao_uma_linha_json() {
        let resposta = Resposta::Recusado { erro: "x".into() };
        let texto = serde_json::to_string(&resposta).unwrap();
        assert_eq!(texto, r#"{"resultado":"recusado","erro":"x"}"#);
        assert_eq!(
            serde_json::to_string(&Resposta::Gravado).unwrap(),
            r#"{"resultado":"gravado"}"#
        );
    }
}
