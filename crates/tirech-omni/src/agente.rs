//! O agente de chaves embutido do app (M29): a lista de chaves (hoje em `agente` das
//! configurações), o [`Agent`] da lib, a confirmação de uso e a passphrase das chaves guardada no cofre.
//!
//! As threads do agente (uma assinatura que pede confirmação, um `ssh-add` externo) falam com a
//! tela por dois signals, [`AgenteApp::versao`] e [`AgenteApp::pedidos`]. Como a fila das chaves
//! do servidor em [`crate::abrir`], todo acesso a eles passa por um `Mutex`, porque o `Signal`
//! não aguenta uma thread lendo enquanto outra escreve.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::agent::{Agent, AgentKey, Constraints, OPENSSH_PIPE, Peer};
use tomni_ui::terminal::ssh::keys::{self, KeyError, KeyFile};

use crate::armazenamento::com_til;
use crate::cofre::{CAMPO_ARQUIVO, Campo, CofreCompartilhado, Item, Tipo};
use crate::configuracoes::AgenteCfg;
use crate::t;

/// Quanto a tela tem para responder a um pedido de uso (o agente da lib também nega aos 60 s).
const ESPERA_DO_USO: Duration = Duration::from_secs(60);

/// Por quanto tempo "Não perguntar de novo para este programa" vale.
pub const LEMBRAR_POR: Duration = Duration::from_secs(15 * 60);

/// O endereço padrão do agente do app, o que o `ssh` do sistema acha sem ajuda: o `agent.sock` em
/// `$XDG_RUNTIME_DIR/tirech-omni` no Linux (a pasta do usuário que o sistema limpa ao sair), o
/// `~/.ssh/tirech-omni-agent.sock` quando não há essa pasta e no macOS, e o pipe
/// `\\.\pipe\openssh-ssh-agent` no Windows (o que o `ssh.exe` usa sem `SSH_AUTH_SOCK`).
pub fn endereco_padrao() -> PathBuf {
    endereco_em(
        cfg!(windows),
        cfg!(target_os = "macos"),
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .as_deref(),
        crate::armazenamento::pasta_pessoal().as_deref(),
    )
}

/// [`endereco_padrao`] com o ambiente nos parâmetros (os testes dão o seu).
pub fn endereco_em(
    windows: bool,
    macos: bool,
    xdg_runtime_dir: Option<&Path>,
    pessoal: Option<&Path>,
) -> PathBuf {
    if windows {
        return PathBuf::from(OPENSSH_PIPE);
    }
    match xdg_runtime_dir {
        Some(pasta) if !macos && pasta.is_absolute() => {
            pasta.join("tirech-omni").join("agent.sock")
        }
        _ => pessoal
            .map(|p| p.join(".ssh"))
            .unwrap_or_else(|| PathBuf::from(".ssh"))
            .join("tirech-omni-agent.sock"),
    }
}

/// O endereço do agente com a pasta de dados isolada (a variável ou o modo portable, M66): no
/// Windows, o pipe `\\.\pipe\tirech-omni-agent-<resumo da pasta>`, que não disputa o pipe do
/// OpenSSH com uma cópia instalada; nos outros sistemas, o `agent.sock` na pasta.
pub fn endereco_isolado(pasta: &Path, windows: bool) -> PathBuf {
    if windows {
        PathBuf::from(format!(
            r"\\.\pipe\tirech-omni-agent-{}",
            resumo_do_caminho(pasta)
        ))
    } else {
        pasta.join("agent.sock")
    }
}

/// Os 16 primeiros caracteres hexadecimais do SHA-256 de `caminho`: o que separa os endereços de
/// duas pastas.
pub fn resumo_do_caminho(caminho: &Path) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    let resumo = Sha256::digest(caminho.as_os_str().as_encoded_bytes());
    resumo.iter().take(8).fold(String::new(), |mut h, b| {
        let _ = write!(h, "{b:02x}");
        h
    })
}

/// O tamanho máximo do caminho de um socket Unix neste sistema, sem o zero do fim: o campo
/// `sun_path` tem 108 bytes no Linux e 104 no macOS.
pub const fn limite_do_socket(macos: bool) -> usize {
    if macos { 103 } else { 107 }
}

/// `desejado`, se cabe num socket Unix; senão, um endereço curto e estável calculado dele:
/// `$XDG_RUNTIME_DIR/tirech-{h}.sock` (pasta que já é só do usuário) ou, sem ela e no macOS,
/// `{temporaria}/tirech-{usuario}/{h}.sock`, com a pasta em 0700. `h` são os 16 primeiros
/// caracteres hexadecimais do SHA-256 do pedido. O prefixo `tirech-` não leva o nome do app, para
/// o endereço não mudar com ele. Se a pasta da temporária não é do usuário ou tem outra
/// permissão, devolve o motivo em vez de usá-la.
pub fn endereco_que_cabe(
    desejado: &Path,
    macos: bool,
    xdg_runtime_dir: Option<&Path>,
    temporaria: &Path,
    usuario: &str,
) -> Result<PathBuf, String> {
    if desejado.as_os_str().len() <= limite_do_socket(macos) {
        return Ok(desejado.to_owned());
    }
    let h = resumo_do_caminho(desejado);
    match xdg_runtime_dir {
        Some(pasta) if !macos && pasta.is_absolute() => Ok(pasta.join(format!("tirech-{h}.sock"))),
        _ => {
            let pasta = temporaria.join(format!("tirech-{usuario}"));
            pasta_so_do_usuario(&pasta)?;
            Ok(pasta.join(format!("{h}.sock")))
        }
    }
}

/// [`endereco_que_cabe`] com o ambiente de verdade; se a pasta curta não puder ser usada, fica o
/// pedido (e o erro do bind diz o que houve).
pub fn endereco_que_cabe_aqui(desejado: PathBuf) -> PathBuf {
    let usuario = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    endereco_que_cabe(
        &desejado,
        cfg!(target_os = "macos"),
        std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .as_deref(),
        &std::env::temp_dir(),
        &usuario,
    )
    .unwrap_or(desejado)
}

/// Cria `pasta` com permissão 0700 ou confere que a que existe é uma pasta só do usuário.
/// (Uma pasta 0700 de outra pessoa não deixa o app entrar, e o bind acusa; as demais
/// permissões são recusadas aqui.)
#[cfg(unix)]
fn pasta_so_do_usuario(pasta: &Path) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let erro = |e: io::Error| {
        t!(
            "agente-pasta-curta-erro",
            pasta = pasta.display().to_string(),
            erro = e.to_string()
        )
    };
    if let Err(e) = std::fs::DirBuilder::new().mode(0o700).create(pasta)
        && e.kind() != io::ErrorKind::AlreadyExists
    {
        return Err(erro(e));
    }
    let meta = std::fs::symlink_metadata(pasta).map_err(erro)?;
    if !meta.is_dir() || meta.permissions().mode() & 0o777 != 0o700 {
        return Err(t!(
            "agente-pasta-curta-insegura",
            pasta = pasta.display().to_string()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn pasta_so_do_usuario(_: &Path) -> Result<(), String> {
    Ok(())
}

/// O pipe do app no Windows, para quando o serviço `ssh-agent` ocupa o do OpenSSH.
pub fn pipe_proprio() -> PathBuf {
    let usuario = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_default();
    PathBuf::from(format!(r"\\.\pipe\tirech-omni-agent-{usuario}"))
}

/// De onde veio uma chave que está no agente.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrigemDaChave {
    /// Do cofre: o `uid` do item.
    Cofre(String),
    /// De um arquivo da pasta `~/.ssh` (ou outra), carregado pelo app.
    Arquivo(PathBuf),
    /// De outro programa (um `ssh-add`).
    Externa,
}

/// O que se sabe da chave do cofre que o app pôs no agente.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DoCofre {
    fingerprint: String,
    confirmar: bool,
    vida_secs: u64,
    certificado: String,
    titulo: String,
}

/// Uma chave que o app carrega no agente ao iniciar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChaveSalva {
    /// O arquivo da chave privada (gravado com `~/` quando está na pasta pessoal).
    #[serde(with = "crate::armazenamento::caminho_com_til")]
    pub arquivo: PathBuf,
    /// Se cada uso pede confirmação.
    pub confirmar: bool,
    /// Depois de quantos segundos o agente esquece a chave (0: nunca).
    pub vida_secs: u64,
}

impl Default for ChaveSalva {
    fn default() -> Self {
        Self {
            arquivo: PathBuf::new(),
            confirmar: false,
            vida_secs: 0,
        }
    }
}

impl ChaveSalva {
    /// Uma chave sem restrições.
    pub fn new(arquivo: impl Into<PathBuf>) -> Self {
        Self {
            arquivo: arquivo.into(),
            ..Self::default()
        }
    }

    fn restricoes(&self) -> Constraints {
        Constraints {
            lifetime: (self.vida_secs > 0).then(|| Duration::from_secs(self.vida_secs)),
            confirm: self.confirmar,
        }
    }
}

/// Um pedido de uso de uma chave que exige confirmação.
#[derive(Debug)]
pub struct PedidoDeUso {
    /// O comentário da chave.
    pub comentario: String,
    /// A impressão digital SHA-256.
    pub fingerprint: String,
    /// Quem pede, numa linha: "codex ← git ← ssh (pid 4182)".
    pub programa: String,
    /// O nome do programa de que a linha parte ("codex").
    pub programa_nome: String,
    /// O servidor a que a conexão vai ("github.com"), quando o `known_hosts` o reconhece.
    pub destino: Option<String>,
    /// A chave de "Não perguntar de novo": o programa e a chave.
    pub lembrete: String,
    /// Por onde a tela responde: `true` permite, `false` nega.
    pub resposta: Sender<bool>,
}

/// A fila de pedidos de uso.
pub type FilaDeUso = Signal<Vec<Arc<PedidoDeUso>>>;

/// Serializa o acesso aos signals do agente entre a thread dele e a da tela.
static ACESSO: Mutex<()> = Mutex::new(());

fn travar() -> MutexGuard<'static, ()> {
    ACESSO.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Sobe o contador `sinal` e acorda a tela, de qualquer thread. É assim que uma thread de
/// trabalho (o agente, a geração de uma chave, o `ssh-copy-id`) avisa que acabou.
pub fn acordar(sinal: Signal<u64>) {
    {
        let _trava = travar();
        let _ = sinal.try_update(|v| *v += 1);
    }
    tomni_ui::wake();
}

/// Lê um contador de qualquer thread, rastreando-o (para views e efeitos).
pub fn ler_contador(sinal: Signal<u64>) -> u64 {
    let _trava = travar();
    sinal.get()
}

/// Como [`ler_contador`], sem rastrear.
pub fn ler_contador_agora(sinal: Signal<u64>) -> u64 {
    let _trava = travar();
    sinal.get_untracked()
}

/// Quem pergunta à tela se uma chave pode ser usada. Roda na thread do agente (por isso é
/// `Clone` e não guarda nada que não cruze threads); o [`AgenteApp`] entrega um ao agente da lib.
#[derive(Clone)]
pub struct Pedinte {
    fila: FilaDeUso,
    lembrados: Arc<Mutex<HashMap<String, Instant>>>,
    known_hosts: Arc<Mutex<Vec<PathBuf>>>,
    espera: Duration,
}

impl Pedinte {
    /// Pergunta (ou, se a pessoa pediu para não perguntar de novo, responde sozinho). Bloqueia
    /// até a resposta, o tempo esgotar (nega) ou a conexão desistir.
    pub fn perguntar(&self, chave: &AgentKey, quem: &Peer) -> bool {
        let fingerprint = keys::fingerprint(&chave.public);
        let (programa, programa_nome) = descrever_pedinte(&quem.chain, quem.pid);
        let lembrete = format!("{programa_nome}\u{1}{fingerprint}");
        if lembrado_ate(&self.lembrados, &lembrete, Instant::now()) {
            return true;
        }
        let destino = quem.host_key.as_ref().and_then(|host| {
            let arquivos = self
                .known_hosts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            destino_no_known_hosts(host, &arquivos)
        });
        let (resposta, espera) = mpsc::channel();
        let pedido = Arc::new(PedidoDeUso {
            comentario: chave.comment.clone(),
            fingerprint,
            programa,
            programa_nome,
            destino,
            lembrete,
            resposta,
        });
        {
            let _trava = travar();
            let _ = self.fila.try_update(|f| f.push(pedido));
        }
        tomni_ui::wake();
        espera.recv_timeout(self.espera).unwrap_or(false)
    }
}

/// Se `lembrete` foi lembrado e ainda não passaram [`LEMBRAR_POR`] em `agora`. Esquece o vencido.
fn lembrado_ate(
    lembrados: &Mutex<HashMap<String, Instant>>,
    lembrete: &str,
    agora: Instant,
) -> bool {
    let mut lembrados = lembrados.lock().unwrap_or_else(PoisonError::into_inner);
    match lembrados.get(lembrete) {
        Some(ate) if *ate > agora => true,
        Some(_) => {
            lembrados.remove(lembrete);
            false
        }
        None => false,
    }
}

/// O agente do app: o [`Agent`] (se roda), o que mudou e os pedidos de confirmação. Quem o
/// divide usa um `Rc`: ligar e desligar o agente não troca o objeto.
pub struct AgenteApp {
    agente: RefCell<Option<Agent>>,
    erro: RefCell<Option<String>>,
    versao: Signal<u64>,
    pedidos: FilaDeUso,
    /// De onde veio cada chave do agente (pelo fingerprint).
    origens: RefCell<HashMap<String, OrigemDaChave>>,
    /// As chaves do cofre que já foram postas no agente (por `uid`), para só mexer no que mudou
    /// e para "Remover" na página não as trazer de volta.
    do_cofre: RefCell<HashMap<String, DoCofre>>,
    lembrados: Arc<Mutex<HashMap<String, Instant>>>,
    known_hosts: Arc<Mutex<Vec<PathBuf>>>,
    /// Outro Tirech Omni já atende o endereço: as chaves dele valem.
    outro_app: Cell<bool>,
    /// No Windows, o serviço `ssh-agent` ocupa o pipe do OpenSSH e o app atende o próprio.
    servico_ocupa: Cell<bool>,
    pageant_ocupado: Cell<bool>,
    #[cfg(windows)]
    pageant: RefCell<Option<tomni_ui::terminal::ssh::agent::PageantGuard>>,
}

impl AgenteApp {
    /// Um agente parado. [`AgenteApp::ligar`] o sobe.
    pub fn new() -> Self {
        Self {
            agente: RefCell::new(None),
            erro: RefCell::new(None),
            versao: signal(0),
            pedidos: signal(Vec::new()),
            origens: RefCell::new(HashMap::new()),
            do_cofre: RefCell::new(HashMap::new()),
            lembrados: Arc::default(),
            known_hosts: Arc::default(),
            outro_app: Cell::new(false),
            servico_ocupa: Cell::new(false),
            pageant_ocupado: Cell::new(false),
            #[cfg(windows)]
            pageant: RefCell::new(None),
        }
    }

    /// Quem pergunta pelos pedidos de uso: o que o agente chama, e os testes também.
    pub fn pedinte(&self) -> Pedinte {
        Pedinte {
            fila: self.pedidos,
            lembrados: Arc::clone(&self.lembrados),
            known_hosts: Arc::clone(&self.known_hosts),
            espera: ESPERA_DO_USO,
        }
    }

    /// Os arquivos `known_hosts` onde o destino de um pedido de uso é procurado.
    pub fn definir_known_hosts(&self, arquivos: Vec<PathBuf>) {
        *self
            .known_hosts
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = arquivos;
    }

    /// Sobe o agente em `socket`. Se não subir, guarda o motivo ([`AgenteApp::erro`]) e o app
    /// usa o agente do sistema. Se outro Tirech Omni já atende o endereço, não liga nada e
    /// [`AgenteApp::outro_app`] diz isso: as chaves dele valem para o sistema.
    pub fn ligar(&self, socket: &Path, config: &AgenteCfg) -> Result<(), String> {
        self.desligar();
        self.outro_app.set(false);
        self.servico_ocupa.set(false);
        // Fora do Windows o endereço é um socket Unix: um caminho relativo criaria o arquivo
        // no diretório de trabalho, e um nome de pipe do Windows viraria um arquivo com ele.
        let texto = socket.to_string_lossy();
        if cfg!(not(windows)) && (!socket.is_absolute() || texto.starts_with(r"\\.\pipe\")) {
            let msg = t!("agente-endereco-invalido", endereco = texto.into_owned());
            *self.erro.borrow_mut() = Some(msg.clone());
            return Err(msg);
        }
        let ligado = match Agent::start(Some(socket.to_owned())) {
            Err(erro)
                if erro.kind() == io::ErrorKind::AddrInUse
                    && cfg!(windows)
                    && socket == Path::new(OPENSSH_PIPE) =>
            {
                self.servico_ocupa.set(true);
                Agent::start(Some(pipe_proprio()))
            }
            outro => outro,
        };
        match ligado {
            Ok(agente) => {
                agente.default_lifetime(
                    (config.vida_padrao_secs > 0)
                        .then(|| Duration::from_secs(config.vida_padrao_secs)),
                );
                let versao = self.versao;
                agente.on_change(move || acordar(versao));
                let pedinte = self.pedinte();
                agente.on_confirm(move |pedido| pedinte.perguntar(&pedido.key, &pedido.peer));
                *self.agente.borrow_mut() = Some(agente);
                *self.erro.borrow_mut() = None;
                acordar(self.versao);
                Ok(())
            }
            Err(erro) if erro.kind() == io::ErrorKind::AddrInUse => {
                self.servico_ocupa.set(false);
                self.outro_app.set(true);
                *self.erro.borrow_mut() = None;
                acordar(self.versao);
                Ok(())
            }
            Err(erro) => {
                let mensagem = t!("agente-embutido-nao-iniciou", erro = erro.to_string());
                *self.erro.borrow_mut() = Some(mensagem.clone());
                Err(mensagem)
            }
        }
    }

    /// Se outro Tirech Omni aberto já atende o endereço do agente.
    pub fn outro_app(&self) -> bool {
        self.outro_app.get()
    }

    /// Se o serviço `ssh-agent` do Windows ocupa o pipe do OpenSSH (o app atende o dele).
    pub fn servico_ocupa(&self) -> bool {
        self.servico_ocupa.get()
    }

    /// Marca o serviço `ssh-agent` como ocupando o pipe (ou livre) sem tentar ligar nada: os
    /// testes mostram assim o aviso do serviço fora do Windows.
    pub fn marcar_servico_ocupa(&self, ocupa: bool) {
        self.servico_ocupa.set(ocupa);
        acordar(self.versao);
    }

    /// Liga ou desliga o Pageant (só no Windows; nos outros sistemas não faz nada). Devolve
    /// `false` se outro Pageant já está aberto.
    pub fn definir_pageant(&self, ligado: bool) -> bool {
        #[cfg(windows)]
        {
            use tomni_ui::terminal::ssh::agent::PageantError;
            self.pageant.borrow_mut().take();
            let resultado = if !ligado || !self.rodando() {
                self.pageant_ocupado.set(false);
                true
            } else {
                let guarda = self.agente.borrow().as_ref().map(Agent::start_pageant);
                match guarda {
                    Some(Ok(guarda)) => {
                        *self.pageant.borrow_mut() = Some(guarda);
                        self.pageant_ocupado.set(false);
                        true
                    }
                    Some(Err(PageantError::OutroPageant)) => {
                        self.pageant_ocupado.set(true);
                        false
                    }
                    _ => {
                        self.pageant_ocupado.set(false);
                        true
                    }
                }
            };
            acordar(self.versao);
            resultado
        }
        #[cfg(not(windows))]
        {
            let _ = ligado;
            true
        }
    }

    /// Se o Pageant está ligado e atende o PuTTY, o WinSCP e o FileZilla.
    pub fn pageant_ligado(&self) -> bool {
        #[cfg(windows)]
        {
            self.pageant.borrow().is_some()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    /// Se outro Pageant (o do PuTTY) já estava aberto quando o app tentou ligar o dele.
    pub fn pageant_ocupado(&self) -> bool {
        self.pageant_ocupado.get()
    }

    /// Marca o Pageant como ocupado (ou livre) sem tentar ligar nada: os testes mostram assim o
    /// aviso do Pageant fora do Windows.
    pub fn marcar_pageant_ocupado(&self, ocupado: bool) {
        self.pageant_ocupado.set(ocupado);
        acordar(self.versao);
    }

    /// Para o agente: fecha o socket e esquece as chaves.
    pub fn desligar(&self) {
        #[cfg(windows)]
        self.pageant.borrow_mut().take();
        self.origens.borrow_mut().clear();
        self.do_cofre.borrow_mut().clear();
        self.outro_app.set(false);
        self.servico_ocupa.set(false);
        if let Some(agente) = self.agente.borrow_mut().take() {
            agente.stop();
            acordar(self.versao);
        }
    }

    /// Se o agente embutido está rodando.
    pub fn rodando(&self) -> bool {
        self.agente.borrow().is_some()
    }

    /// Por que o agente não subiu.
    pub fn erro(&self) -> Option<String> {
        self.erro.borrow().clone()
    }

    /// O caminho do socket (o que vai no `SSH_AUTH_SOCK`), se o agente roda.
    pub fn caminho(&self) -> Option<PathBuf> {
        self.agente.borrow().as_ref().map(|a| a.path().to_owned())
    }

    /// O contador que sobe a cada mudança no agente. Dentro de uma view, passa a depender dele.
    pub fn versao(&self) -> u64 {
        ler_contador(self.versao)
    }

    /// Como [`AgenteApp::versao`], sem rastrear (eventos e testes).
    pub fn versao_agora(&self) -> u64 {
        ler_contador_agora(self.versao)
    }

    /// Os pedidos de uso à espera da pessoa. Dentro de uma view, passa a depender da fila.
    pub fn pedidos(&self) -> Vec<Arc<PedidoDeUso>> {
        let _trava = travar();
        self.pedidos.with(|p| p.clone())
    }

    /// Como [`AgenteApp::pedidos`], sem rastrear.
    pub fn pedidos_agora(&self) -> Vec<Arc<PedidoDeUso>> {
        let _trava = travar();
        self.pedidos.with_untracked(|p| p.clone())
    }

    /// Responde a um pedido de uso e o tira da fila. Com `permitir` e `lembrar`, o mesmo programa
    /// usa a mesma chave sem perguntar por [`LEMBRAR_POR`].
    pub fn responder(&self, pedido: &Arc<PedidoDeUso>, permitir: bool, lembrar: bool) {
        if permitir && lembrar {
            self.lembrados
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(pedido.lembrete.clone(), Instant::now() + LEMBRAR_POR);
        }
        // A conexão que esperava pode ter desistido: o erro não importa.
        let _ = pedido.resposta.send(permitir);
        let _trava = travar();
        let _ = self
            .pedidos
            .try_update(|p| p.retain(|outro| !Arc::ptr_eq(outro, pedido)));
    }

    /// As chaves no agente (vazio se ele não roda ou está travado).
    pub fn chaves(&self) -> Vec<AgentKey> {
        self.agente
            .borrow()
            .as_ref()
            .map(Agent::list)
            .unwrap_or_default()
    }

    /// Se o agente está travado.
    pub fn travado(&self) -> bool {
        self.agente.borrow().as_ref().is_some_and(Agent::is_locked)
    }

    /// Se a chave (a impressão digital `SHA256:…`) está no agente.
    pub fn tem(&self, fingerprint: &str) -> bool {
        self.chaves()
            .iter()
            .any(|k| keys::fingerprint(&k.public) == fingerprint)
    }

    /// De onde veio a chave: do cofre, de um arquivo ou de outro programa.
    pub fn origem_de(&self, chave: &AgentKey) -> OrigemDaChave {
        self.origens
            .borrow()
            .get(&keys::fingerprint(&chave.public))
            .cloned()
            .unwrap_or(OrigemDaChave::Externa)
    }

    /// Anota de onde veio a chave `fingerprint`.
    pub fn registrar_origem(&self, fingerprint: &str, origem: OrigemDaChave) {
        self.origens
            .borrow_mut()
            .insert(fingerprint.to_owned(), origem);
    }

    /// O arquivo de onde o app carregou a chave, se foi isso.
    pub fn arquivo_de(&self, chave: &AgentKey) -> Option<PathBuf> {
        match self.origem_de(chave) {
            OrigemDaChave::Arquivo(arquivo) => Some(arquivo),
            _ => None,
        }
    }

    /// Esquece que a chave veio de um arquivo e devolve o arquivo.
    pub fn esquecer_arquivo_de(&self, fingerprint: &str) -> Option<PathBuf> {
        let mut origens = self.origens.borrow_mut();
        match origens.get(fingerprint) {
            Some(OrigemDaChave::Arquivo(_)) => match origens.remove(fingerprint) {
                Some(OrigemDaChave::Arquivo(arquivo)) => Some(arquivo),
                _ => None,
            },
            _ => None,
        }
    }

    /// Põe no agente as chaves do cofre marcadas "No agente" e tira as que deixaram de estar. É
    /// idempotente: só mexe no que mudou desde a última vez, e uma chave que a pessoa tirou na
    /// página Agente só volta quando o item muda. Com o cofre bloqueado e `tirar_ao_bloquear`,
    /// tira as do cofre; sem a opção, deixa como está.
    pub fn carregar_do_cofre(&self, cofre: &CofreCompartilhado, tirar_ao_bloquear: bool) {
        if !self.rodando() {
            return;
        }
        if cofre.agora(|c| c.bloqueado()) {
            if tirar_ao_bloquear {
                self.tirar_as_do_cofre();
            }
            return;
        }
        let desejadas: Vec<(String, DoCofre)> = cofre.agora(|c| {
            c.chaves_ssh()
                .into_iter()
                .filter_map(|item| {
                    let chave = item.chave.as_ref()?;
                    if !chave.no_agente || chave.e_token() || chave.privada.trim().is_empty() {
                        return None;
                    }
                    let uid = if item.uid.is_empty() {
                        format!("#{}", item.id)
                    } else {
                        item.uid.clone()
                    };
                    Some((
                        uid,
                        DoCofre {
                            fingerprint: chave.fingerprint.clone(),
                            confirmar: chave.confirmar,
                            vida_secs: chave.vida_secs,
                            certificado: chave.certificado.trim().to_owned(),
                            titulo: item.titulo.clone(),
                        },
                    ))
                })
                .collect()
        });
        // Quem saiu: tira do agente (se ainda é a do cofre) e do mapa.
        let antigas: Vec<(String, DoCofre)> = self
            .do_cofre
            .borrow()
            .iter()
            .map(|(uid, d)| (uid.clone(), d.clone()))
            .collect();
        for (uid, antiga) in antigas {
            let mudou = desejadas
                .iter()
                .find(|(u, _)| *u == uid)
                .is_none_or(|(_, nova)| *nova != antiga);
            if mudou {
                self.tirar_a_do_cofre(&uid, &antiga.fingerprint);
                self.do_cofre.borrow_mut().remove(&uid);
            }
        }
        for (uid, meta) in desejadas {
            if self.do_cofre.borrow().contains_key(&uid) {
                continue;
            }
            // Já está no agente por outro caminho (um arquivo, um `ssh-add`): vale o que a
            // pessoa fez. Tenta de novo na próxima vez.
            if self.tem(&meta.fingerprint) {
                continue;
            }
            self.do_cofre.borrow_mut().insert(uid.clone(), meta.clone());
            let Some(privada) = cofre.agora(|c| c.chave_privada_por_uid(&uid)) else {
                continue;
            };
            let fingerprint = keys::fingerprint(privada.public_key());
            let restricoes = Constraints {
                lifetime: (meta.vida_secs > 0).then(|| Duration::from_secs(meta.vida_secs)),
                confirm: meta.confirmar,
            };
            let guardada = self
                .agente
                .borrow()
                .as_ref()
                .is_some_and(|a| a.add(privada, &meta.titulo, restricoes).is_ok());
            if !guardada {
                continue;
            }
            self.registrar_origem(&fingerprint, OrigemDaChave::Cofre(uid));
            if !meta.certificado.is_empty()
                && let Ok(certificado) = ssh_key::Certificate::from_openssh(&meta.certificado)
                && let Some(agente) = self.agente.borrow().as_ref()
            {
                agente.add_certificate(certificado, &meta.titulo);
            }
        }
    }

    /// Tira do agente todas as chaves que vieram do cofre.
    fn tirar_as_do_cofre(&self) {
        let todas: Vec<(String, DoCofre)> = self.do_cofre.borrow_mut().drain().collect();
        for (uid, d) in todas {
            self.tirar_a_do_cofre(&uid, &d.fingerprint);
        }
    }

    /// Tira a chave do agente se ela ainda é a que o cofre pôs (`uid`).
    fn tirar_a_do_cofre(&self, uid: &str, fingerprint: &str) {
        let Some(chave) = self.chaves().into_iter().find(|k| {
            keys::fingerprint(&k.public) == fingerprint
                && self.origens.borrow().get(fingerprint)
                    == Some(&OrigemDaChave::Cofre(uid.to_owned()))
        }) else {
            return;
        };
        self.remover(&chave);
    }

    /// Abre o arquivo da chave e a põe no agente, com as restrições de `chave`. Devolve a
    /// impressão digital.
    pub fn carregar(
        &self,
        arquivo: &Path,
        passphrase: Option<&str>,
        chave: &ChaveSalva,
    ) -> Result<String, KeyError> {
        let aberta = KeyFile::open(arquivo, passphrase)?;
        let privada = aberta.private_key().clone();
        let impressao = keys::fingerprint(privada.public_key());
        let comentario = match aberta.info().comment.as_str() {
            "" => arquivo
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            c => c.to_owned(),
        };
        self.guardar(privada, &comentario, chave)?;
        Ok(impressao)
    }

    /// Põe uma chave já aberta no agente.
    pub fn guardar(
        &self,
        privada: ssh_key::PrivateKey,
        comentario: &str,
        chave: &ChaveSalva,
    ) -> Result<(), KeyError> {
        let agente = self.agente.borrow();
        let agente = agente
            .as_ref()
            .ok_or_else(|| KeyError::Invalid(t!("agente-embutido-parado")))?;
        agente
            .add(privada, comentario, chave.restricoes())
            .map_err(|erro| KeyError::Invalid(erro.to_string()))
    }

    /// Tira a chave. Devolve se estava.
    pub fn remover(&self, chave: &AgentKey) -> bool {
        self.origens
            .borrow_mut()
            .remove(&keys::fingerprint(&chave.public));
        self.agente
            .borrow()
            .as_ref()
            .is_some_and(|a| a.remove(&chave.public))
    }

    /// Esquece todas as chaves.
    pub fn remover_todas(&self) {
        self.origens.borrow_mut().clear();
        if let Some(a) = self.agente.borrow().as_ref() {
            a.remove_all();
        }
    }

    /// Trava o agente com a passphrase.
    pub fn travar(&self, passphrase: &str) -> bool {
        self.agente
            .borrow()
            .as_ref()
            .is_some_and(|a| a.lock(passphrase))
    }

    /// Destrava o agente. Devolve `false` se a passphrase não confere.
    pub fn destravar(&self, passphrase: &str) -> bool {
        self.agente
            .borrow()
            .as_ref()
            .is_some_and(|a| a.unlock(passphrase))
    }

    /// Muda a validade padrão das chaves novas.
    pub fn vida_padrao(&self, segundos: u64) {
        if let Some(a) = self.agente.borrow().as_ref() {
            a.default_lifetime((segundos > 0).then(|| Duration::from_secs(segundos)));
        }
    }
}

impl Default for AgenteApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Os nomes de programa que só repassam o pedido: a linha de quem pede os pula.
const INTERMEDIARIOS: [&str; 4] = ["ssh", "git", "sh", "bash"];

/// Descreve quem pede a chave: a cadeia (do `ssh` para cima, cortada no primeiro programa que
/// não é só um intermediário) de trás para a frente, com o pid de quem falou com o agente, e o
/// nome do programa de que a linha parte. Sem cadeia, um texto genérico.
fn descrever_pedinte(cadeia: &[String], pid: Option<u32>) -> (String, String) {
    let corte = cadeia
        .iter()
        .position(|n| !INTERMEDIARIOS.contains(&n.as_str()))
        .map_or(cadeia.len(), |i| i + 1);
    let cadeia = &cadeia[..corte];
    let Some(ultimo) = cadeia.last() else {
        let texto = if cfg!(windows) {
            t!("agente-um-programa-pageant")
        } else {
            t!("agente-um-programa")
        };
        return (texto, "programa".to_owned());
    };
    let mut linha = cadeia
        .iter()
        .rev()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" ← ");
    if let Some(pid) = pid {
        linha.push(' ');
        linha.push_str(&t!("agente-pid", pid = i64::from(pid)));
    }
    (linha, ultimo.clone())
}

/// O nome com que o servidor de `host` aparece em algum dos `known_hosts` (só nomes em claro:
/// o que está com hash, com curinga ou negado não diz nada).
pub fn destino_no_known_hosts(host: &ssh_key::PublicKey, arquivos: &[PathBuf]) -> Option<String> {
    for arquivo in arquivos {
        let Ok(texto) = std::fs::read_to_string(arquivo) else {
            continue;
        };
        for linha in texto.lines() {
            let linha = linha.trim();
            if linha.is_empty() || linha.starts_with('#') || linha.starts_with('@') {
                continue;
            }
            let mut partes = linha.split_whitespace();
            let (Some(nomes), Some(tipo), Some(chave)) =
                (partes.next(), partes.next(), partes.next())
            else {
                continue;
            };
            let Ok(da_linha) = ssh_key::PublicKey::from_openssh(&format!("{tipo} {chave}")) else {
                continue;
            };
            if da_linha.key_data() != host.key_data() {
                continue;
            }
            let nome = nomes.split(',').find_map(|nome| {
                if nome.starts_with('|') || nome.contains(['*', '?', '!']) {
                    return None;
                }
                let nome = match nome.strip_prefix('[').and_then(|n| n.split_once("]:")) {
                    Some((host, _porta)) => host,
                    None => nome,
                };
                (!nome.is_empty()).then(|| nome.to_owned())
            });
            if nome.is_some() {
                return nome;
            }
        }
    }
    None
}

/// O fingerprint (`SHA256:…`) da chave em `arquivo`, lido da parte pública do arquivo ou do
/// `.pub` ao lado. `None` num PEM cifrado sem `.pub`, que não traz a pública.
pub fn fingerprint_do_arquivo(arquivo: &Path) -> Option<String> {
    KeyFile::inspect(arquivo).ok()?.fingerprint()
}

/// Como o item da passphrase acha a chave: pelo fingerprint, que vale em qualquer máquina, ou,
/// sem ele, pelo nome do arquivo.
fn url_do_item(arquivo: &Path) -> String {
    fingerprint_do_arquivo(arquivo).unwrap_or_else(|| nome_do_arquivo(arquivo))
}

fn nome_do_arquivo(arquivo: &Path) -> String {
    arquivo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// O item do cofre com a passphrase desta chave, se há. Acha pelo fingerprint e, para o PEM
/// cifrado, pelo nome do arquivo; um item do M43 para trás, com o caminho inteiro, ainda vale.
pub fn item_da_passphrase(cofre: &CofreCompartilhado, arquivo: &Path) -> Option<u64> {
    let fingerprint = fingerprint_do_arquivo(arquivo);
    let nome = nome_do_arquivo(arquivo);
    let caminho = arquivo.display().to_string();
    cofre.agora(|c| {
        c.itens()
            .iter()
            .find(|i| {
                i.tipo == Tipo::Chave
                    && !i.url.is_empty()
                    && (fingerprint.as_deref() == Some(i.url.as_str())
                        || i.url == caminho
                        || (fingerprint.is_none() && i.url == nome))
            })
            .map(|i| i.id)
    })
}

/// A passphrase guardada no cofre para a chave. `None` se não há (ou o cofre está bloqueado).
pub fn passphrase_do_cofre(cofre: &CofreCompartilhado, arquivo: &Path) -> Option<String> {
    if cofre.agora(|c| c.bloqueado()) {
        return None;
    }
    let id = item_da_passphrase(cofre, arquivo)?;
    cofre
        .agora(|c| c.item(id).map(|i| i.senha.clone()))
        .filter(|s| !s.is_empty())
}

/// Guarda (ou troca) a passphrase da chave no cofre, num item "Passphrase de chave" cujo `url`
/// é o fingerprint da chave (ou o nome do arquivo, quando ela não traz a pública). O campo
/// `arquivo` só informa de onde a chave veio.
pub fn guardar_passphrase(
    cofre: &CofreCompartilhado,
    arquivo: &Path,
    passphrase: &str,
) -> io::Result<u64> {
    let existente =
        item_da_passphrase(cofre, arquivo).and_then(|id| cofre.agora(|c| c.item(id).cloned()));
    let mut item = existente.unwrap_or_else(|| Item {
        tipo: Tipo::Chave,
        titulo: nome_do_arquivo(arquivo),
        ..Item::default()
    });
    item.url = url_do_item(arquivo);
    item.senha = passphrase.to_owned();
    item.campos.retain(|c| c.nome != CAMPO_ARQUIVO);
    item.campos.push(Campo {
        nome: CAMPO_ARQUIVO.to_owned(),
        valor: com_til(arquivo),
        oculto: false,
    });
    cofre.mudar(|c| c.salvar(item))
}

#[cfg(test)]
mod tests {
    /// Um caminho com `n` bytes dentro de `/tmp`.
    fn caminho_de(n: usize) -> PathBuf {
        let base = "/tmp/";
        PathBuf::from(format!("{base}{}", "a".repeat(n - base.len())))
    }

    #[test]
    fn um_endereco_curto_fica_igual() {
        let curto = Path::new("/run/user/1000/tirech-omni/agent.sock");
        let r = endereco_que_cabe(curto, false, None, Path::new("/tmp"), "ana");
        assert_eq!(r.unwrap(), curto);
        // O limite exato (107 bytes) ainda cabe.
        let no_limite = caminho_de(107);
        let r = endereco_que_cabe(&no_limite, false, None, Path::new("/tmp"), "ana");
        assert_eq!(r.unwrap(), no_limite);
    }

    #[test]
    fn um_endereco_longo_vira_o_curto_no_xdg() {
        let xdg = Path::new("/run/user/1000");
        let r = endereco_que_cabe(&caminho_de(200), false, Some(xdg), Path::new("/tmp"), "ana")
            .unwrap();
        let nome = r.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(r.parent().unwrap(), xdg);
        assert!(
            nome.starts_with("tirech-") && nome.ends_with(".sock"),
            "{nome}"
        );
        assert_eq!(nome.len(), "tirech-".len() + 16 + ".sock".len());
        assert!(r.as_os_str().len() <= limite_do_socket(false));
    }

    #[test]
    fn o_curto_e_sempre_o_mesmo_para_o_mesmo_pedido() {
        let xdg = Some(Path::new("/run/user/1000"));
        let a = endereco_que_cabe(&caminho_de(200), false, xdg, Path::new("/tmp"), "ana");
        let b = endereco_que_cabe(&caminho_de(200), false, xdg, Path::new("/tmp"), "ana");
        assert_eq!(a, b);
        // O valor não muda entre versões: fixa o resumo de um pedido conhecido.
        let conhecido =
            endereco_que_cabe(&caminho_de(150), false, xdg, Path::new("/tmp"), "ana").unwrap();
        assert_eq!(
            conhecido,
            Path::new("/run/user/1000/tirech-4c9dd1f44b8558fa.sock")
        );
    }

    #[test]
    fn pedidos_diferentes_dao_curtos_diferentes() {
        let xdg = Some(Path::new("/run/user/1000"));
        let a = endereco_que_cabe(&caminho_de(200), false, xdg, Path::new("/tmp"), "ana");
        let b = endereco_que_cabe(&caminho_de(201), false, xdg, Path::new("/tmp"), "ana");
        assert_ne!(a.unwrap(), b.unwrap());
    }

    #[test]
    fn no_macos_o_limite_e_menor() {
        let caminho = caminho_de(105);
        let temporaria =
            std::env::temp_dir().join(format!("tirech-teste-mac-{}", std::process::id()));
        std::fs::create_dir_all(&temporaria).unwrap();
        let linux = endereco_que_cabe(&caminho, false, None, &temporaria, "ana").unwrap();
        assert_eq!(linux, caminho);
        let mac = endereco_que_cabe(
            &caminho,
            true,
            Some(Path::new("/run/user/1")),
            &temporaria,
            "ana",
        )
        .unwrap();
        assert_ne!(mac, caminho);
        // No macOS a pasta do XDG não vale: o curto vai para a temporária.
        assert!(mac.starts_with(&temporaria), "{mac:?}");
        let _ = std::fs::remove_dir_all(&temporaria);
    }

    #[cfg(unix)]
    #[test]
    fn sem_xdg_vai_para_a_temporaria_com_a_pasta_em_0700() {
        use std::os::unix::fs::PermissionsExt;
        let temporaria =
            std::env::temp_dir().join(format!("tirech-teste-tmp-{}", std::process::id()));
        std::fs::create_dir_all(&temporaria).unwrap();
        let r = endereco_que_cabe(&caminho_de(200), false, None, &temporaria, "ana").unwrap();
        let pasta = temporaria.join("tirech-ana");
        assert_eq!(r.parent().unwrap(), pasta);
        let modo = std::fs::metadata(&pasta).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo, 0o700);
        // Pedir de novo reaproveita a pasta e dá o mesmo endereço.
        assert_eq!(
            endereco_que_cabe(&caminho_de(200), false, None, &temporaria, "ana").unwrap(),
            r
        );
        // Uma pasta que existe com outra permissão é recusada.
        std::fs::set_permissions(&pasta, std::fs::Permissions::from_mode(0o755)).unwrap();
        let erro =
            endereco_que_cabe(&caminho_de(200), false, None, &temporaria, "ana").unwrap_err();
        assert!(erro.contains("não é só do seu usuário"), "{erro}");
        let _ = std::fs::remove_dir_all(&temporaria);
    }

    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::cofre::Cofre;

    #[test]
    fn a_vida_zero_e_sem_validade() {
        assert_eq!(ChaveSalva::new("/a").restricoes().lifetime, None);
        let c = ChaveSalva {
            vida_secs: 90,
            ..ChaveSalva::new("/a")
        };
        assert_eq!(c.restricoes().lifetime, Some(Duration::from_secs(90)));
    }

    #[test]
    fn a_passphrase_vai_para_o_cofre_e_volta() {
        let pasta = PastaTemporaria::nova("agente");
        let (cofre, _) = Cofre::abrir(&pasta.0);
        let cofre = CofreCompartilhado::new(cofre);
        let arquivo = Path::new("/casa/.ssh/id_ed25519");
        assert_eq!(passphrase_do_cofre(&cofre, arquivo), None);
        let id = guardar_passphrase(&cofre, arquivo, "uma frase").unwrap();
        assert_eq!(
            passphrase_do_cofre(&cofre, arquivo).as_deref(),
            Some("uma frase")
        );
        // Trocar a passphrase muda o mesmo item.
        assert_eq!(guardar_passphrase(&cofre, arquivo, "outra").unwrap(), id);
        assert_eq!(
            passphrase_do_cofre(&cofre, arquivo).as_deref(),
            Some("outra")
        );
        assert_eq!(cofre.agora(|c| c.itens().len()), 1);
        assert_eq!(
            cofre.agora(|c| c.item(id).map(|i| i.tipo)),
            Some(Tipo::Chave)
        );
    }

    #[test]
    fn o_endereco_padrao_de_cada_sistema() {
        let xdg = Path::new("/run/user/1000");
        let casa = Path::new("/home/ana");
        assert_eq!(
            endereco_em(false, false, Some(xdg), Some(casa)),
            Path::new("/run/user/1000/tirech-omni/agent.sock")
        );
        // Sem a pasta do usuário, ou com uma relativa, vai para o ~/.ssh.
        for xdg in [None, Some(Path::new("relativa"))] {
            assert_eq!(
                endereco_em(false, false, xdg, Some(casa)),
                Path::new("/home/ana/.ssh/tirech-omni-agent.sock")
            );
        }
        // No macOS a pasta do usuário não é a do XDG: o ~/.ssh.
        assert_eq!(
            endereco_em(false, true, Some(xdg), Some(casa)),
            Path::new("/home/ana/.ssh/tirech-omni-agent.sock")
        );
        // No Windows é o pipe do OpenSSH.
        assert_eq!(
            endereco_em(true, false, Some(xdg), Some(casa)),
            Path::new(OPENSSH_PIPE)
        );
    }

    #[test]
    fn a_pasta_isolada_tem_um_agente_so_dela() {
        let pendrive = Path::new(r"E:\Tirech Omni\tirech-omnidata");
        let pipe = endereco_isolado(pendrive, true)
            .to_string_lossy()
            .into_owned();
        assert!(pipe.starts_with(r"\\.\pipe\tirech-omni-agent-"), "{pipe}");
        assert_eq!(pipe.len(), r"\\.\pipe\tirech-omni-agent-".len() + 16);
        assert_ne!(
            endereco_isolado(Path::new(r"D:\tirech-omnidata"), true),
            endereco_isolado(pendrive, true)
        );
        assert_eq!(
            endereco_isolado(Path::new("/media/ana/tirech-omnidata"), false),
            Path::new("/media/ana/tirech-omnidata/agent.sock")
        );
    }

    #[test]
    fn quem_pede_aparece_do_ssh_para_cima_sem_os_intermediarios() {
        let cadeia = |nomes: &[&str]| nomes.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let (linha, nome) = descrever_pedinte(&cadeia(&["ssh", "git", "codex"]), Some(4182));
        assert_eq!(linha, "codex ← git ← ssh (pid 4182)");
        assert_eq!(nome, "codex");
        // A cadeia corta no primeiro programa que não só repassa.
        let (linha, nome) = descrever_pedinte(&cadeia(&["ssh", "bash", "make", "tmux"]), None);
        assert_eq!(linha, "make ← bash ← ssh");
        assert_eq!(nome, "make");
        // Sem cadeia: um texto genérico.
        let (linha, nome) = descrever_pedinte(&[], Some(7));
        assert!(linha.starts_with("Um programa"));
        assert_eq!(nome, "programa");
    }

    #[test]
    fn o_destino_vem_do_known_hosts_so_com_nomes_em_claro() {
        let pasta = PastaTemporaria::nova("agente-known-hosts");
        std::fs::create_dir_all(&pasta.0).unwrap();
        let publica = include_str!("../tests/dados/id_ed25519.pub");
        let mut partes = publica.split_whitespace();
        let corpo = format!("{} {}", partes.next().unwrap(), partes.next().unwrap());
        let chave = ssh_key::PublicKey::from_openssh(&corpo).unwrap();
        let arquivo = pasta.0.join("known_hosts");
        std::fs::write(
            &arquivo,
            format!(
                "# um comentário\n|1|aGFzaA==|aGFzaA== {corpo}\n*.exemplo.com {corpo}\n\
                 [git.exemplo.com]:2222,10.0.0.9 {corpo}\n"
            ),
        )
        .unwrap();
        assert_eq!(
            destino_no_known_hosts(&chave, &[arquivo]).as_deref(),
            Some("git.exemplo.com")
        );
        // Sem o arquivo, ou com outra chave, não diz nada.
        assert_eq!(
            destino_no_known_hosts(&chave, &[pasta.0.join("nao-existe")]),
            None
        );
    }

    #[test]
    fn o_lembrete_vale_ate_o_prazo_e_depois_se_esquece() {
        let lembrados = Mutex::new(HashMap::new());
        let agora = Instant::now();
        lembrados
            .lock()
            .unwrap()
            .insert("codex\u{1}SHA256:x".to_owned(), agora + LEMBRAR_POR);
        assert!(lembrado_ate(&lembrados, "codex\u{1}SHA256:x", agora));
        assert!(!lembrado_ate(&lembrados, "outro\u{1}SHA256:x", agora));
        // Passado o prazo, pergunta de novo e o lembrete some.
        let depois = agora + LEMBRAR_POR + Duration::from_secs(1);
        assert!(!lembrado_ate(&lembrados, "codex\u{1}SHA256:x", depois));
        assert!(lembrados.lock().unwrap().is_empty());
    }
}
