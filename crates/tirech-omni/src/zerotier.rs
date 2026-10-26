//! O ZeroTier embutido (M61): o nó da lib `tomni-ui-zerotier` roda dentro do app, só para o
//! tráfego dele, sem `zerotier-one`, sem placa virtual e sem privilégio. Este módulo converte o
//! estado do nó de e para o item do cofre, liga os nós (um por perfil de rede) e conta o que a
//! página Redes mostra.
//!
//! É o par de `tailscale.rs`: a rede entrega ao resto do app um proxy SOCKS5 de `127.0.0.1` (a
//! ponte do motor), que a tela põe no `perfil.proxy` da `RedeResolvida`. A ponte nunca aparece
//! para quem usa o app.
//!
//! O motor muda o estado do nó (identidade nova, configuração da rede, caminhos) de uma thread do
//! `tokio`, e o cofre só vive na thread da tela. Por isso o estado novo espera em
//! [`MotoresZeroTier::tomar_pendente`] até a tela gravá-lo no item do cofre; com o cofre trancado
//! ele fica em memória.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinSet;
use tomni_ui::prelude::Signal;
use tomni_ui_zerotier::peer::PeerRole;
use tomni_ui_zerotier::{
    Address, Cidr, Identity, NetworkId, NodeState, PeerInfo, State, ZeroTier, ZeroTierBuilder,
};

use crate::cofre::IdentidadeZeroTier;
use crate::redes::{PerfilRede, TravasDeRede, ZeroTierRede};
use crate::t;

/// A espera padrão de um perfil ZeroTier novo, em segundos: é o tempo de alguém autorizar o
/// membro no painel da rede.
pub const ESPERA_PADRAO_SECS: u32 = 120;

/// Quanto o [`MotorZeroTier::esperar`] olha o estado de uma vez só: o tempo máximo para mostrar o
/// aviso da autorização sem esperar o limite inteiro.
const FATIA_DA_ESPERA: Duration = Duration::from_millis(500);

/// O id da rede como o texto da tela o escreve (16 hexadecimais, em qualquer caixa).
///
/// # Errors
///
/// A mensagem, em português, de por que o id não serve.
pub fn rede_de(texto: &str) -> Result<NetworkId, String> {
    NetworkId::parse(texto.trim()).ok_or_else(|| t!("zerotier-rede-invalida"))
}

/// O id da rede em minúsculas e sem espaços, como o perfil o guarda; um texto que não é um id
/// volta como veio (a tela mostra o erro).
pub fn rede_normalizada(texto: &str) -> String {
    rede_de(texto).map_or_else(|_| texto.trim().to_owned(), |r| r.to_string())
}

/// O IP fixo do perfil (`10.0.0.5/24`); vazio vale `None`.
///
/// # Errors
///
/// A mensagem, em português, de por que o IP não serve.
pub fn ip_fixo_de(texto: &str) -> Result<Option<Cidr>, String> {
    let texto = texto.trim();
    if texto.is_empty() {
        return Ok(None);
    }
    let cidr = texto
        .parse::<Cidr>()
        .ok()
        .filter(|c| c.addr().is_ipv4())
        .ok_or_else(|| t!("zerotier-ip-fixo-invalido", ip = texto))?;
    Ok(Some(cidr))
}

/// Uma moon do perfil: o id (16 hexadecimais) e a semente (10).
///
/// # Errors
///
/// A mensagem, em português, de por que a moon não serve.
pub fn moon_de(id: &str, semente: &str) -> Result<(u64, Address), String> {
    let (id, semente) = (id.trim(), semente.trim());
    let moon = NetworkId::parse(id).map(NetworkId::as_u64);
    let endereco = (semente.len() == 10)
        .then(|| Address::parse(semente))
        .flatten();
    match (moon, endereco) {
        (Some(moon), Some(endereco)) => Ok((moon, endereco)),
        _ => Err(t!(
            "zerotier-moon-invalida",
            moon = format!("{id}/{semente}")
        )),
    }
}

/// As moons do campo de texto da tela: `id/semente` separadas por espaço, vírgula ou ponto e
/// vírgula. O que não tem a barra vai com a semente vazia, para a conferência acusar na hora de
/// ligar.
pub fn moons_de_texto(texto: &str) -> Vec<(String, String)> {
    texto
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (id, semente) = p.split_once('/').unwrap_or((p, ""));
            (
                id.trim().to_ascii_lowercase(),
                semente.trim().to_ascii_lowercase(),
            )
        })
        .collect()
}

/// O inverso de [`moons_de_texto`].
pub fn moons_para_texto(moons: &[(String, String)]) -> String {
    moons
        .iter()
        .map(|(id, semente)| format!("{id}/{semente}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// O que o `NodeState` guarda além da identidade, em JSON e base64 dentro do item do cofre.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct EstadoSalvo {
    /// O último planet aceito.
    planet: String,
    /// As moons recebidas.
    moons: Vec<String>,
    /// A última configuração assinada da rede.
    config: String,
    /// O último caminho direto bom de cada par: o endereço e o `ip:porta`.
    caminhos: Vec<(String, String)>,
}

fn codificar_estado(no: &NodeState) -> String {
    let vazio = no.planet.is_empty()
        && no.moons.is_empty()
        && no.network_config.is_empty()
        && no.memorized_paths.is_empty();
    if vazio {
        return String::new();
    }
    let salvo = EstadoSalvo {
        planet: STANDARD.encode(&no.planet),
        moons: no.moons.iter().map(|m| STANDARD.encode(m)).collect(),
        config: STANDARD.encode(&no.network_config),
        caminhos: no
            .memorized_paths
            .iter()
            .map(|(a, s)| (a.to_string(), s.to_string()))
            .collect(),
    };
    let json = serde_json::to_vec(&salvo).unwrap_or_default();
    STANDARD.encode(json)
}

fn decodificar_estado(texto: &str, no: &mut NodeState) -> Result<(), String> {
    let texto = texto.trim();
    if texto.is_empty() {
        return Ok(());
    }
    let estragado = |e: String| t!("zerotier-estado-invalido", erro = e);
    let json = STANDARD
        .decode(texto)
        .map_err(|e| estragado(e.to_string()))?;
    let salvo: EstadoSalvo = serde_json::from_slice(&json).map_err(|e| estragado(e.to_string()))?;
    let bytes = |t: &str| STANDARD.decode(t).map_err(|e| estragado(e.to_string()));
    no.planet = bytes(&salvo.planet)?;
    no.moons = salvo
        .moons
        .iter()
        .map(|m| bytes(m))
        .collect::<Result<_, _>>()?;
    no.network_config = bytes(&salvo.config)?;
    no.memorized_paths = salvo
        .caminhos
        .iter()
        .filter_map(|(a, s)| Some((Address::parse(a)?, s.parse::<SocketAddr>().ok()?)))
        .collect();
    Ok(())
}

/// A identidade do texto do `identity.secret` (com a parte secreta).
///
/// # Errors
///
/// A mensagem, em português, de por que a identidade não serve.
pub fn identidade_de(texto: &str) -> Result<Identity, String> {
    let identidade = Identity::from_secret_str(texto)
        .map_err(|e| t!("zerotier-identidade-invalida", erro = e.to_string()))?;
    if !identidade.has_secret() {
        return Err(t!("zerotier-identidade-sem-segredo"));
    }
    if !identidade.validate() {
        return Err(t!(
            "zerotier-identidade-invalida",
            erro = t!("zerotier-identidade-nao-confere")
        ));
    }
    Ok(identidade)
}

/// O item do cofre de um `identity.secret` que a pessoa já tinha (para manter o endereço que o
/// administrador autorizou).
///
/// # Errors
///
/// A mensagem, em português, de por que a identidade não serve.
pub fn importar_identidade(texto: &str) -> Result<IdentidadeZeroTier, String> {
    let identidade = identidade_de(texto)?;
    let mut item = IdentidadeZeroTier::default();
    gravar_identidade(&mut item, &identidade);
    Ok(item)
}

fn gravar_identidade(item: &mut IdentidadeZeroTier, identidade: &Identity) {
    if let Some(segredo) = identidade.to_secret_string() {
        item.identidade = segredo.to_string();
    }
    item.endereco = identidade.address().to_string();
}

/// O `identity.secret` do item, para exportar. `None`: o motor ainda não gerou a identidade.
pub fn texto_da_identidade(item: &IdentidadeZeroTier) -> Option<String> {
    Some(item.identidade.trim().to_owned()).filter(|t| !t.is_empty())
}

/// O estado do nó que o item do cofre guarda. Sem identidade, o motor gera uma no primeiro uso;
/// uma identidade ou um estado estragados são um erro, para o motor não virar outro membro em
/// silêncio.
///
/// # Errors
///
/// A mensagem, em português, de qual parte do item não serve.
pub fn no_de(item: &IdentidadeZeroTier) -> Result<NodeState, String> {
    let mut no = NodeState::default();
    if !item.identidade.trim().is_empty() {
        let identidade = Identity::from_secret_str(item.identidade.trim())
            .map_err(|e| t!("zerotier-identidade-invalida", erro = e.to_string()))?;
        if !identidade.has_secret() {
            return Err(t!("zerotier-identidade-sem-segredo"));
        }
        no.identity = Some(identidade);
    }
    decodificar_estado(&item.estado, &mut no)?;
    no.network_name.clone_from(&item.nome_rede);
    no.addresses = item
        .enderecos
        .iter()
        .filter_map(|e| e.parse::<Cidr>().ok())
        .collect();
    Ok(no)
}

/// Confere o item que veio de fora (Bitwarden, JSON): a identidade e o estado precisam ler. Põe o
/// endereço do item de acordo com a identidade.
///
/// # Errors
///
/// A mensagem, em português, de qual parte do item não serve.
pub fn completar(item: &mut IdentidadeZeroTier) -> Result<(), String> {
    let no = no_de(item)?;
    if let Some(identidade) = &no.identity {
        item.endereco = identidade.address().to_string();
    }
    Ok(())
}

/// Grava no item o estado novo do nó. O endereço e o `identity.secret` só mudam se o nó trouxe
/// uma identidade.
pub fn gravar_no(item: &mut IdentidadeZeroTier, no: &NodeState) {
    if let Some(identidade) = &no.identity {
        gravar_identidade(item, identidade);
    }
    item.estado = codificar_estado(no);
    item.nome_rede.clone_from(&no.network_name);
    item.enderecos = no.addresses.iter().map(ToString::to_string).collect();
}

/// Esquece o que o nó aprendeu (planet, moons, configuração e caminhos), para o próximo uso
/// começar do planet embutido. A identidade fica.
pub fn esquecer_estado(item: &mut IdentidadeZeroTier) {
    item.estado.clear();
    item.nome_rede.clear();
    item.enderecos.clear();
}

/// Se o item já tem estado guardado (algo a esquecer).
pub fn tem_estado(item: &IdentidadeZeroTier) -> bool {
    !item.estado.trim().is_empty()
        || !item.nome_rede.trim().is_empty()
        || !item.enderecos.is_empty()
}

/// A linha de estado da página Redes: `None` é a rede desligada.
pub fn descrever_estado(estado: Option<&State>) -> String {
    match estado {
        None | Some(State::Stopped) => t!("zerotier-estado-desligada"),
        Some(State::Connecting) => t!("zerotier-estado-conectando"),
        Some(State::WaitingConfig) => t!("zerotier-estado-pedindo-configuracao"),
        Some(State::NotAuthorized { .. }) => t!("zerotier-estado-aguardando-autorizacao"),
        Some(State::NotFound) => t!("zerotier-estado-nao-encontrada"),
        Some(State::AuthenticationRequired) => t!("zerotier-estado-exige-login"),
        Some(State::Connected { address, name, .. }) => {
            if name.trim().is_empty() {
                t!(
                    "zerotier-estado-conectada-sem-nome",
                    ip = address.to_string()
                )
            } else {
                t!(
                    "zerotier-estado-conectada",
                    ip = address.to_string(),
                    nome = name.as_str()
                )
            }
        }
        Some(State::Retrying(motivo)) => {
            t!("zerotier-estado-tentando", motivo = motivo.as_str())
        }
        Some(State::Failed(motivo)) => t!("zerotier-estado-falhou", motivo = motivo.as_str()),
    }
}

/// O endereço que o administrador precisa autorizar, se o nó está esperando isso.
pub fn endereco_a_autorizar(estado: Option<&State>) -> Option<String> {
    match estado {
        Some(State::NotAuthorized { address }) => Some(address.to_string()),
        _ => None,
    }
}

/// Os avisos da configuração da rede (regras que o nó não trata por inteiro), se conectado.
pub fn avisos(estado: Option<&State>) -> Vec<String> {
    match estado {
        Some(State::Connected { warnings, .. }) => warnings.clone(),
        _ => Vec::new(),
    }
}

/// O IP do nó na rede, se conectado.
pub fn ip_do_no(estado: Option<&State>) -> Option<String> {
    match estado {
        Some(State::Connected { address, .. }) => Some(address.to_string()),
        _ => None,
    }
}

fn papel(papel: PeerRole) -> String {
    match papel {
        PeerRole::Root => t!("zerotier-papel-raiz"),
        PeerRole::Moon => t!("zerotier-papel-moon"),
        PeerRole::Controller => t!("zerotier-papel-controlador"),
        PeerRole::Member => t!("zerotier-papel-membro"),
    }
}

/// Uma linha da lista de raiz e pares dos detalhes: o endereço, o papel, o caminho, a latência e
/// a versão que o par anunciou.
pub fn descrever_par(par: &PeerInfo) -> String {
    let caminho = if par.path.is_empty() {
        t!("zerotier-caminho-nenhum")
    } else if par.path == "pelo raiz" {
        t!("zerotier-caminho-pelo-raiz")
    } else if let Some(destino) = par
        .path
        .strip_prefix("direto (")
        .and_then(|d| d.strip_suffix(')'))
    {
        t!("zerotier-caminho-direto", destino = destino)
    } else {
        par.path.clone()
    };
    let mut partes = vec![par.address.to_string(), papel(par.role), caminho];
    if let Some(latencia) = par.latency {
        partes.push(t!("zerotier-latencia", ms = latencia.as_millis() as u64));
    }
    if !par.version.is_empty() {
        partes.push(par.version.clone());
    }
    partes.join(" · ")
}

fn sem_veneno<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O que o perfil e o item do cofre dizem para ligar o nó.
#[derive(Clone)]
pub struct Parametros {
    /// A rede.
    pub rede: NetworkId,
    /// O item do cofre com a identidade (para recusar duas redes com a mesma identidade ligadas
    /// ao mesmo tempo).
    pub item: u64,
    /// O estado do nó guardado no cofre.
    pub no: NodeState,
    /// Aceitar IPs e rotas em faixas públicas.
    pub permitir_globais: bool,
    /// Aceitar a rota padrão da rede.
    pub permitir_padrao: bool,
    /// Usar o DNS da rede.
    pub permitir_dns: bool,
    /// O IP fixo, se o perfil tem um.
    pub ip_fixo: Option<Cidr>,
    /// As moons para orbitar: o id e a semente.
    pub moons: Vec<(u64, Address)>,
}

impl Parametros {
    /// Os parâmetros de um perfil ZeroTier com o estado do item do cofre.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de qual campo do perfil ou do item não serve.
    pub fn de(
        rede: &ZeroTierRede,
        item: u64,
        identidade: &IdentidadeZeroTier,
    ) -> Result<Self, String> {
        let moons = rede
            .moons
            .iter()
            .map(|(id, semente)| moon_de(id, semente))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            rede: rede_de(&rede.rede)?,
            item,
            no: no_de(identidade)?,
            permitir_globais: rede.permitir_globais,
            permitir_padrao: rede.permitir_padrao,
            permitir_dns: rede.permitir_dns,
            ip_fixo: ip_fixo_de(&rede.ip_fixo)?,
            moons,
        })
    }
}

// O estado do nó (e a identidade dentro dele) não aparece em `{:?}`.
impl std::fmt::Debug for Parametros {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parametros")
            .field("rede", &self.rede)
            .field("item", &self.item)
            .field("permitir_globais", &self.permitir_globais)
            .field("permitir_padrao", &self.permitir_padrao)
            .field("permitir_dns", &self.permitir_dns)
            .field("ip_fixo", &self.ip_fixo)
            .field("moons", &self.moons)
            .finish_non_exhaustive()
    }
}

/// O que identifica "este motor": a rede, o item da identidade, as opções, o IP fixo e as moons.
/// Mudou, o motor religa. O estado do nó **não** entra: o motor o muda sozinho (a identidade
/// gerada, a configuração, os caminhos) e religar por isso seria um laço.
pub fn assinatura(parametros: &Parametros) -> u64 {
    let mut h = DefaultHasher::new();
    parametros.rede.as_u64().hash(&mut h);
    parametros.item.hash(&mut h);
    parametros.permitir_globais.hash(&mut h);
    parametros.permitir_padrao.hash(&mut h);
    parametros.permitir_dns.hash(&mut h);
    parametros.ip_fixo.map(|c| c.to_string()).hash(&mut h);
    for (moon, semente) in &parametros.moons {
        moon.hash(&mut h);
        semente.as_u64().hash(&mut h);
    }
    h.finish()
}

/// O motor ligado de um perfil, como a `RedeResolvida` o leva até o `Opener`.
#[derive(Clone)]
pub struct MotorZeroTier {
    zt: Arc<ZeroTier>,
    tempo: Handle,
}

impl PartialEq for MotorZeroTier {
    fn eq(&self, outro: &Self) -> bool {
        Arc::ptr_eq(&self.zt, &outro.zt)
    }
}

impl Eq for MotorZeroTier {}

// A ponte (porta e credencial) não aparece em `{:?}`.
impl std::fmt::Debug for MotorZeroTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotorZeroTier")
            .field("estado", &self.zt.state())
            .finish_non_exhaustive()
    }
}

impl MotorZeroTier {
    /// O proxy SOCKS5 da ponte do motor.
    pub fn proxy(&self) -> tomni_ui::terminal::Proxy {
        self.zt.proxy()
    }

    /// O estado agora.
    pub fn estado(&self) -> State {
        self.zt.state()
    }

    /// O endereço deste membro.
    pub fn endereco(&self) -> Address {
        self.zt.address()
    }

    /// Espera o nó conectar (até `espera_secs`). O `aviso` recebe as linhas que o terminal mostra,
    /// inclusive a de que o administrador precisa autorizar o membro. Bloqueia a thread de quem
    /// chama, que não pode ser a da tela.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que a rede não conectou.
    pub fn esperar(&self, perfil: &PerfilRede, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        if matches!(self.zt.state(), State::Connected { .. }) {
            return Ok(());
        }
        aviso(&t!("zerotier-conectando", nome = perfil.name.as_str()));
        let limite = Duration::from_secs(u64::from(perfil.espera_secs.max(1)));
        let fim = Instant::now() + limite;
        let rede = self.zt.network().to_string();
        let mut avisou_autorizacao = false;
        loop {
            let resta = fim.saturating_duration_since(Instant::now());
            match self
                .tempo
                .block_on(self.zt.wait_connected(resta.min(FATIA_DA_ESPERA)))
            {
                Ok(ip) => {
                    aviso(&t!(
                        "zerotier-pronta",
                        nome = perfil.name.as_str(),
                        ip = ip.to_string()
                    ));
                    return Ok(());
                }
                Err(erro) if erro.kind() == std::io::ErrorKind::TimedOut => {}
                Err(erro) => {
                    return Err(t!(
                        "zerotier-nao-conectou",
                        nome = perfil.name.as_str(),
                        motivo = motivo_do_erro(&self.zt.state(), &erro.to_string(), &rede)
                    ));
                }
            }
            if let State::NotAuthorized { address } = self.zt.state()
                && !avisou_autorizacao
            {
                avisou_autorizacao = true;
                aviso(&t!(
                    "zerotier-aguardando-autorizacao",
                    endereco = address.to_string(),
                    rede = rede.as_str()
                ));
            }
            if Instant::now() >= fim {
                let estado = self.zt.state();
                let motivo = match &estado {
                    State::NotAuthorized { address } => t!(
                        "zerotier-motivo-autorizacao",
                        endereco = address.to_string(),
                        rede = rede.as_str()
                    ),
                    State::Retrying(motivo) => motivo.clone(),
                    outro => descrever_estado(Some(outro)),
                };
                return Err(t!(
                    "zerotier-nao-conectou",
                    nome = perfil.name.as_str(),
                    motivo = motivo
                ));
            }
        }
    }
}

/// O motivo para mostrar de uma falha da espera: a rede que não existe e o login pelo navegador
/// têm texto próprio; o resto vai como a lib o disse.
fn motivo_do_erro(estado: &State, erro: &str, rede: &str) -> String {
    match estado {
        State::NotFound => t!("zerotier-motivo-nao-encontrada", rede = rede),
        State::AuthenticationRequired => t!("zerotier-motivo-exige-login"),
        _ => erro.to_owned(),
    }
}

/// Um motor ligado e com o que o fez subir (para religar se o perfil mudou).
struct Ligado {
    zt: Arc<ZeroTier>,
    assinatura: u64,
    /// O item do cofre com a identidade: dois perfis não ligam a mesma identidade juntos.
    item: u64,
    /// O nome do perfil, para a mensagem de identidade em uso.
    nome: String,
    /// Falso depois do [`MotoresZeroTier::parar`]: os últimos estados do motor velho não valem
    /// mais.
    vivo: Arc<AtomicBool>,
    /// A trava do perfil entre as cópias do app ([`TravasDeRede`]): solta quando o motor para.
    _trava: Option<File>,
}

/// Os nós ZeroTier ligados, um por perfil, sobre um runtime próprio do `tokio` (o app não tem
/// outro). Ligam sob demanda e param quando o último uso do perfil fecha (veja `Usos`) ou quando
/// o app fecha.
#[derive(Default)]
pub struct MotoresZeroTier {
    tempo: Mutex<Option<Arc<Runtime>>>,
    ligados: Mutex<HashMap<u64, Ligado>>,
    /// O último estado de cada perfil, para a página Redes.
    estados: Arc<Mutex<HashMap<u64, State>>>,
    /// O estado novo do nó de cada perfil, esperando a tela gravar no cofre.
    pendentes: Arc<Mutex<HashMap<u64, NodeState>>>,
    /// O contador que acorda a tela quando um estado muda.
    acordar: Arc<Mutex<Option<Signal<u64>>>>,
    /// Uma cópia do app por vez liga cada perfil.
    travas: TravasDeRede,
}

impl MotoresZeroTier {
    /// Avisa a tela (por este contador) cada vez que o estado de um motor muda.
    pub fn ao_mudar(&self, sinal: Signal<u64>) {
        *sem_veneno(&self.acordar) = Some(sinal);
    }

    /// A pasta de configuração, onde fica a trava de cada perfil entre as cópias do app.
    pub fn na_pasta(&self, pasta: &Path) {
        self.travas.na_pasta(pasta);
    }

    fn runtime(&self) -> Result<Arc<Runtime>, String> {
        let mut tempo = sem_veneno(&self.tempo);
        if let Some(r) = tempo.as_ref() {
            return Ok(Arc::clone(r));
        }
        let novo = Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("zerotier")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let novo = Arc::new(novo);
        *tempo = Some(Arc::clone(&novo));
        Ok(novo)
    }

    /// Liga o nó do perfil, se ainda não está ligado com esta assinatura, e devolve o acesso à
    /// ponte. Não espera a conexão (isso é o [`MotorZeroTier::esperar`]). A primeira vez de uma
    /// identidade nova leva um instante: o nó a gera antes de voltar.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que o nó não subiu. O estado do perfil fica `Failed`.
    pub fn garantir(
        &self,
        perfil: &PerfilRede,
        parametros: Result<Parametros, String>,
    ) -> Result<MotorZeroTier, String> {
        let tempo = self.runtime()?;
        // Sem parâmetros (o item do cofre não serve), a assinatura nem existe: o motor velho, se
        // há, para antes de falhar.
        let assinatura = parametros.as_ref().ok().map(assinatura);
        if let Some(assinatura) = assinatura {
            let ligados = sem_veneno(&self.ligados);
            if let Some(l) = ligados.get(&perfil.id)
                && l.assinatura == assinatura
            {
                return Ok(MotorZeroTier {
                    zt: Arc::clone(&l.zt),
                    tempo: tempo.handle().clone(),
                });
            }
        }
        // O perfil mudou desde que subiu: o nó velho para.
        self.parar(perfil.id);
        let falhou = |motivo: String| {
            // A página mostra só o motivo (cabe na linha); o aviso leva o nome da rede.
            self.guardar_estado(perfil.id, State::Failed(motivo.clone()));
            t!(
                "zerotier-nao-subiu",
                nome = perfil.name.as_str(),
                motivo = motivo
            )
        };
        let parametros = parametros.map_err(&falhou)?;
        // Dois membros com a mesma identidade se atrapalham: o segundo não liga.
        if let Some(outro) = sem_veneno(&self.ligados)
            .iter()
            .find(|(id, l)| **id != perfil.id && l.item == parametros.item)
            .map(|(_, l)| l.nome.clone())
        {
            return Err(falhou(t!("zerotier-identidade-em-uso", nome = outro)));
        }
        let assinatura = assinatura.unwrap_or_default();
        let trava = self.travas.travar(perfil).map_err(&falhou)?;
        let vivo = Arc::new(AtomicBool::new(true));
        let (estados, pendentes, acordar, id) = (
            Arc::clone(&self.estados),
            Arc::clone(&self.pendentes),
            Arc::clone(&self.acordar),
            perfil.id,
        );
        let (vivo_do_estado, vivo_do_no) = (Arc::clone(&vivo), Arc::clone(&vivo));
        let (pendentes_do_no, acordar_do_no) = (Arc::clone(&pendentes), Arc::clone(&acordar));
        let mut construtor = ZeroTierBuilder::new(parametros.rede, parametros.no)
            .allow_global(parametros.permitir_globais)
            .allow_default(parametros.permitir_padrao)
            .allow_dns(parametros.permitir_dns)
            .on_state(move |estado| {
                if vivo_do_estado.load(Ordering::Relaxed) {
                    sem_veneno(&estados).insert(id, estado.clone());
                    acordar_a_tela(&acordar);
                }
            })
            .on_node_state(move |no| {
                if vivo_do_no.load(Ordering::Relaxed) {
                    sem_veneno(&pendentes_do_no).insert(id, no.clone());
                    acordar_a_tela(&acordar_do_no);
                }
            });
        if let Some(ip) = parametros.ip_fixo {
            construtor = construtor.static_address(ip);
        }
        for (moon, semente) in parametros.moons {
            construtor = construtor.orbit(moon, semente);
        }
        let ligado = tempo
            .block_on(construtor.start())
            .map_err(|e| falhou(e.to_string()))?;
        let ligado = Arc::new(ligado);
        sem_veneno(&self.ligados).insert(
            perfil.id,
            Ligado {
                zt: Arc::clone(&ligado),
                assinatura,
                item: parametros.item,
                nome: perfil.name.clone(),
                vivo,
                _trava: trava,
            },
        );
        self.guardar_estado(perfil.id, ligado.state());
        Ok(MotorZeroTier {
            zt: ligado,
            tempo: tempo.handle().clone(),
        })
    }

    fn guardar_estado(&self, perfil: u64, estado: State) {
        sem_veneno(&self.estados).insert(perfil, estado);
        acordar_a_tela(&self.acordar);
    }

    /// Para o nó do perfil (se está ligado), sem esperar.
    pub fn parar(&self, perfil: u64) {
        let Some(ligado) = sem_veneno(&self.ligados).remove(&perfil) else {
            return;
        };
        ligado.vivo.store(false, Ordering::Relaxed);
        sem_veneno(&self.estados).remove(&perfil);
        acordar_a_tela(&self.acordar);
        if let Ok(tempo) = self.runtime() {
            tempo.spawn(parar_um(ligado.zt));
        }
    }

    /// Para todos, esperando no máximo 200 ms: é o tempo de o nó largar a ponte e o UDP antes de o
    /// app fechar.
    pub fn parar_todos(&self) {
        let todos: Vec<Ligado> = sem_veneno(&self.ligados).drain().map(|(_, l)| l).collect();
        for l in &todos {
            l.vivo.store(false, Ordering::Relaxed);
        }
        if todos.is_empty() {
            return;
        }
        let Ok(tempo) = self.runtime() else {
            return;
        };
        tempo.block_on(async move {
            let mut tarefas = JoinSet::new();
            for l in todos {
                tarefas.spawn(parar_um(l.zt));
            }
            let _ = tokio::time::timeout(Duration::from_millis(200), async {
                while tarefas.join_next().await.is_some() {}
            })
            .await;
        });
        sem_veneno(&self.estados).clear();
    }

    /// O estado do nó do perfil. `None`: desligada.
    pub fn estado(&self, perfil: u64) -> Option<State> {
        sem_veneno(&self.estados)
            .get(&perfil)
            .filter(|e| **e != State::Stopped)
            .cloned()
    }

    /// Se o perfil tem nó ligado.
    pub fn ligado(&self, perfil: u64) -> bool {
        sem_veneno(&self.ligados).contains_key(&perfil)
    }

    /// O endereço do membro do perfil ligado (para mostrar antes de o item ter a identidade).
    pub fn endereco(&self, perfil: u64) -> Option<Address> {
        sem_veneno(&self.ligados)
            .get(&perfil)
            .map(|l| l.zt.address())
    }

    /// Os raiz em uso e os pares com tráfego, nesta ordem, para os detalhes. Vazio se o perfil
    /// está desligado.
    pub fn pares(&self, perfil: u64) -> Vec<PeerInfo> {
        let zt = {
            let ligados = sem_veneno(&self.ligados);
            let Some(l) = ligados.get(&perfil) else {
                return Vec::new();
            };
            Arc::clone(&l.zt)
        };
        // Pouca espera: a tela pede isto a cada mudança de estado, e um nó travado não pode
        // travá-la junto.
        match self.runtime() {
            Ok(tempo) => tempo
                .block_on(async {
                    tokio::time::timeout(Duration::from_millis(300), async {
                        let mut todos = zt.roots().await;
                        todos.extend(zt.peers().await);
                        todos
                    })
                    .await
                })
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// Os perfis que têm estado de nó novo esperando a gravação no cofre, sem tirá-los.
    pub fn perfis_pendentes(&self) -> HashSet<u64> {
        sem_veneno(&self.pendentes).keys().copied().collect()
    }

    /// Tira o estado novo do nó do perfil, para a tela gravá-lo no item do cofre. Se a gravação
    /// falhar, [`MotoresZeroTier::devolver`] o põe de volta.
    pub fn tomar_pendente(&self, perfil: u64) -> Option<NodeState> {
        sem_veneno(&self.pendentes).remove(&perfil)
    }

    /// Devolve o estado que a tela não conseguiu gravar (o cofre trancou no meio). Um estado mais
    /// novo que chegou nesse meio-tempo vale mais.
    pub fn devolver(&self, perfil: u64, no: NodeState) {
        sem_veneno(&self.pendentes).entry(perfil).or_insert(no);
    }
}

/// Para um nó: com a única referência, o `stop` solta a ponte e o UDP; se ainda há quem segure o
/// `Arc` (uma conexão em andamento), o `Drop` do nó para quando o último largar.
async fn parar_um(zt: Arc<ZeroTier>) {
    match Arc::try_unwrap(zt) {
        Ok(zt) => zt.stop(Duration::from_millis(200)).await,
        Err(restante) => drop(restante),
    }
}

fn acordar_a_tela(acordar: &Mutex<Option<Signal<u64>>>) {
    if let Some(sinal) = *sem_veneno(acordar) {
        crate::agente::acordar(sinal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID1: &str = include_str!("../tests/dados/zerotier/id1.secret");

    fn item() -> IdentidadeZeroTier {
        importar_identidade(ID1).unwrap()
    }

    fn rede() -> ZeroTierRede {
        ZeroTierRede {
            rede: "8056c2e21c000001".into(),
            identidade: Some(7),
            ..ZeroTierRede::default()
        }
    }

    fn parametros() -> Parametros {
        Parametros::de(&rede(), 7, &item()).unwrap()
    }

    #[test]
    fn o_id_da_rede_tem_16_hexadecimais_em_qualquer_caixa() {
        assert_eq!(
            rede_de(" 8056C2E21C000001 ").unwrap().to_string(),
            "8056c2e21c000001"
        );
        assert_eq!(rede_normalizada(" 8056C2E21C000001"), "8056c2e21c000001");
        for ruim in [
            "",
            "8056c2e21c00000",
            "8056c2e21c0000012",
            "8056c2e21c00000g",
        ] {
            assert_eq!(rede_de(ruim).unwrap_err(), t!("zerotier-rede-invalida"));
        }
        assert_eq!(rede_normalizada("lixo"), "lixo");
    }

    #[test]
    fn o_ip_fixo_e_um_ipv4_com_prefixo_ou_vazio() {
        assert_eq!(ip_fixo_de("  ").unwrap(), None);
        assert_eq!(ip_fixo_de("10.0.0.5").unwrap().unwrap().prefix(), 32);
        let ip = ip_fixo_de("10.0.0.5/24").unwrap().unwrap();
        assert_eq!(ip.to_string(), "10.0.0.5/24");
        for ruim in ["10.0.0.5/40", "nada", "fd00::1/64"] {
            assert_eq!(
                ip_fixo_de(ruim).unwrap_err(),
                t!("zerotier-ip-fixo-invalido", ip = ruim),
                "{ruim}"
            );
        }
    }

    #[test]
    fn a_moon_tem_id_de_16_e_semente_de_10_hexadecimais() {
        let (moon, semente) = moon_de("0000001234567890", "1234567890").unwrap();
        assert_eq!(moon, 0x1234_567890);
        assert_eq!(semente.to_string(), "1234567890");
        for (id, sem) in [("0000001234567890", "123"), ("12", "1234567890"), ("", "")] {
            assert!(moon_de(id, sem).is_err(), "{id}/{sem}");
        }
    }

    #[test]
    fn as_moons_vao_e_voltam_pelo_texto() {
        let moons = moons_de_texto(" 0000001234567890/1234567890, ABCDEF0123/  ;solta ");
        assert_eq!(
            moons,
            [
                ("0000001234567890".to_owned(), "1234567890".to_owned()),
                ("abcdef0123".to_owned(), String::new()),
                ("solta".to_owned(), String::new()),
            ]
        );
        assert_eq!(moons_para_texto(&moons[..1]), "0000001234567890/1234567890");
        assert!(moons_de_texto("  ").is_empty());
    }

    #[test]
    fn importar_o_identity_secret_guarda_o_endereco_e_recusa_o_que_nao_serve() {
        let item = item();
        assert_eq!(item.endereco, ID1.trim().split(':').next().unwrap());
        assert_eq!(item.identidade, ID1.trim());
        assert!(item.estado.is_empty());
        // A parte pública sozinha não vale: o motor precisa assinar.
        let publica = Identity::from_secret_str(ID1).unwrap().to_public_string();
        assert_eq!(
            importar_identidade(&publica).unwrap_err(),
            t!("zerotier-identidade-sem-segredo")
        );
        let erro = importar_identidade("lixo").unwrap_err();
        assert!(
            erro.starts_with(&t!("zerotier-identidade-invalida", erro = "")),
            "{erro}"
        );
        // Um byte da chave pública trocado não passa na prova de trabalho.
        let mut campos: Vec<String> = ID1.trim().split(':').map(str::to_owned).collect();
        let troca = if campos[2].starts_with('0') { "1" } else { "0" };
        campos[2].replace_range(0..1, troca);
        assert!(importar_identidade(&campos.join(":")).is_err());
    }

    #[test]
    fn o_estado_do_no_vai_e_volta_pelo_item() {
        let mut item = item();
        let mut no = no_de(&item).unwrap();
        assert_eq!(
            no.identity.as_ref().unwrap().address().to_string(),
            item.endereco
        );
        no.planet = vec![1, 2, 3];
        no.moons = vec![vec![4, 5], vec![6]];
        no.network_config = vec![9; 40];
        no.memorized_paths = vec![(
            Address::new(0x0012_3456_7890),
            "192.0.2.7:9993".parse().unwrap(),
        )];
        no.network_name = "casa".into();
        no.addresses = vec!["10.147.17.5/24".parse().unwrap()];
        gravar_no(&mut item, &no);
        assert_eq!(item.nome_rede, "casa");
        assert_eq!(item.enderecos, ["10.147.17.5/24"]);
        assert!(!item.estado.is_empty());
        let volta = no_de(&item).unwrap();
        assert_eq!(volta.planet, no.planet);
        assert_eq!(volta.moons, no.moons);
        assert_eq!(volta.network_config, no.network_config);
        assert_eq!(volta.memorized_paths, no.memorized_paths);
        assert_eq!(volta.network_name, "casa");
        assert_eq!(volta.addresses, no.addresses);
        assert!(volta.identity.is_some());
    }

    #[test]
    fn item_vazio_comeca_um_no_novo_e_item_estragado_e_erro() {
        let vazio = no_de(&IdentidadeZeroTier::default()).unwrap();
        assert!(vazio.identity.is_none() && vazio.planet.is_empty());
        let mut ruim = item();
        ruim.estado = "isto não é base64!".into();
        let erro = no_de(&ruim).unwrap_err();
        assert!(
            erro.starts_with(&t!("zerotier-estado-invalido", erro = "")),
            "{erro}"
        );
        let mut ruim = item();
        ruim.identidade = "lixo".into();
        assert!(no_de(&ruim).is_err());
        // Um nó sem identidade (ainda) não apaga a do item.
        let mut guardado = item();
        gravar_no(&mut guardado, &NodeState::default());
        assert_eq!(guardado.identidade, ID1.trim());
    }

    #[test]
    fn completar_poe_o_endereco_de_acordo_com_a_identidade() {
        let mut item = item();
        item.endereco = "0000000000".into();
        completar(&mut item).unwrap();
        assert_eq!(item.endereco, ID1.trim().split(':').next().unwrap());
        assert!(completar(&mut IdentidadeZeroTier::default()).is_ok());
        item.estado = "%%%".into();
        assert!(completar(&mut item).is_err());
    }

    #[test]
    fn esquecer_o_estado_guarda_a_identidade() {
        let mut item = item();
        let mut no = no_de(&item).unwrap();
        no.network_config = vec![1; 8];
        no.network_name = "casa".into();
        gravar_no(&mut item, &no);
        assert!(tem_estado(&item));
        esquecer_estado(&mut item);
        assert!(!tem_estado(&item));
        assert_eq!(item.identidade, ID1.trim());
        assert_eq!(texto_da_identidade(&item).as_deref(), Some(ID1.trim()));
        assert_eq!(texto_da_identidade(&IdentidadeZeroTier::default()), None);
    }

    #[test]
    fn a_assinatura_ignora_o_estado_do_no_e_muda_com_o_resto() {
        let base = assinatura(&parametros());
        let mut com_estado = parametros();
        com_estado.no.network_config = vec![1; 8];
        com_estado.no.identity = None;
        assert_eq!(base, assinatura(&com_estado), "o estado do nó não religa");
        let mudancas: [fn(&mut Parametros); 7] = [
            |p| p.rede = NetworkId::new(1),
            |p| p.item = 8,
            |p| p.permitir_globais = true,
            |p| p.permitir_padrao = true,
            |p| p.permitir_dns = true,
            |p| p.ip_fixo = ip_fixo_de("10.0.0.5/24").unwrap(),
            |p| p.moons = vec![(1, Address::new(2))],
        ];
        for (n, muda) in mudancas.into_iter().enumerate() {
            let mut outro = parametros();
            muda(&mut outro);
            assert_ne!(base, assinatura(&outro), "mudança {n}");
        }
    }

    #[test]
    fn os_parametros_leem_as_opcoes_do_perfil_e_recusam_o_que_nao_serve() {
        let mut r = rede();
        r.permitir_dns = true;
        r.ip_fixo = "10.0.0.5/24".into();
        r.moons = vec![("0000001234567890".into(), "1234567890".into())];
        let p = Parametros::de(&r, 7, &item()).unwrap();
        assert!(p.permitir_dns && !p.permitir_globais);
        assert_eq!(p.ip_fixo.unwrap().to_string(), "10.0.0.5/24");
        assert_eq!(p.moons.len(), 1);
        r.rede = "xx".into();
        assert!(Parametros::de(&r, 7, &item()).is_err());
        let mut r = rede();
        r.moons = vec![("1".into(), "2".into())];
        assert!(Parametros::de(&r, 7, &item()).is_err());
    }

    #[test]
    fn o_estado_vira_texto_da_pagina() {
        assert_eq!(descrever_estado(None), t!("zerotier-estado-desligada"));
        let a_autorizar = State::NotAuthorized {
            address: Address::new(0x0012_3456_7890),
        };
        assert_eq!(
            descrever_estado(Some(&a_autorizar)),
            t!("zerotier-estado-aguardando-autorizacao")
        );
        assert_eq!(
            endereco_a_autorizar(Some(&a_autorizar)).as_deref(),
            Some("1234567890")
        );
        assert_eq!(endereco_a_autorizar(Some(&State::Connecting)), None);
        assert_eq!(
            descrever_estado(Some(&State::NotFound)),
            t!("zerotier-estado-nao-encontrada")
        );
        assert_eq!(
            descrever_estado(Some(&State::Failed("sem rede".into()))),
            t!("zerotier-estado-falhou", motivo = "sem rede")
        );
        let conectada = State::Connected {
            address: "10.147.17.5".parse().unwrap(),
            name: "casa".into(),
            warnings: vec!["regra ignorada".into()],
        };
        let texto = descrever_estado(Some(&conectada));
        assert!(
            texto.contains("10.147.17.5") && texto.contains("casa"),
            "{texto}"
        );
        assert_eq!(ip_do_no(Some(&conectada)).as_deref(), Some("10.147.17.5"));
        assert_eq!(avisos(Some(&conectada)), ["regra ignorada"]);
        assert!(avisos(None).is_empty());
    }

    #[test]
    fn o_par_vira_uma_linha_com_o_caminho_traduzido() {
        let mut par = PeerInfo {
            address: Address::new(0x0012_3456_7890),
            role: PeerRole::Root,
            path: "direto (192.0.2.7:9993)".into(),
            latency: Some(Duration::from_millis(42)),
            version: "1.14.2 (protocolo 13)".into(),
        };
        let texto = descrever_par(&par);
        for parte in ["1234567890", "192.0.2.7:9993", "42", "1.14.2"] {
            assert!(texto.contains(parte), "{texto}");
        }
        par.path = "pelo raiz".into();
        par.latency = None;
        par.version.clear();
        par.role = PeerRole::Member;
        assert_eq!(
            descrever_par(&par),
            format!(
                "1234567890 · {} · {}",
                t!("zerotier-papel-membro"),
                t!("zerotier-caminho-pelo-raiz")
            )
        );
        par.path.clear();
        assert!(descrever_par(&par).ends_with(&t!("zerotier-caminho-nenhum")));
    }

    #[test]
    fn o_debug_nao_mostra_segredo() {
        let item = item();
        let texto = format!("{item:?}");
        assert!(!texto.contains(item.identidade.rsplit(':').next().unwrap()));
        assert!(texto.contains(&item.endereco));
        let texto = format!("{:?}", parametros());
        let segredo = ID1.trim().rsplit(':').next().unwrap();
        assert!(!texto.contains(segredo), "{texto}");
        let no = format!("{:?}", no_de(&item).unwrap());
        assert!(!no.contains(segredo), "{no}");
    }
}
