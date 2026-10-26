//! O Tailscale embutido (M57): o nó da lib `tomni-ui-tailscale` roda dentro do app, só para o
//! tráfego dele, sem `tailscaled`, sem placa virtual e sem privilégio. Funciona com o Tailscale e
//! com o Headscale. Este módulo converte o estado do nó de e para o item do cofre, liga os nós
//! (um por perfil de rede) e conta o que a página Redes mostra.
//!
//! É o par de `nebula.rs`: a rede entrega ao resto do app um proxy SOCKS5 de `127.0.0.1` (a
//! ponte do motor), que a tela põe no `perfil.proxy` da `RedeResolvida`. A ponte nunca aparece
//! para quem usa o app.
//!
//! O motor muda o estado do nó (chaves novas, login, endereços) de uma thread do `tokio`, e o
//! cofre só vive na thread da tela. Por isso o estado novo espera em [`MotoresTailscale::tomar_pendente`]
//! até a tela gravá-lo no item do cofre; com o cofre trancado ele fica em memória.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinSet;
use tomni_ui::prelude::Signal;
use tomni_ui_tailscale::key::{MachineKey, NodeKey};
use tomni_ui_tailscale::{NodeState, Peer, State, Tailscale, TailscaleBuilder};
use zeroize::Zeroizing;

use crate::cofre::IdentidadeTailscale;
use crate::redes::{PerfilRede, TravasDeRede};
use crate::t;

/// O servidor de controle do Tailscale oficial, o que vale quando o perfil não escolhe outro.
pub const SERVIDOR_OFICIAL: &str = "https://controlplane.tailscale.com";

/// Quanto o [`MotorTailscale::esperar`] olha o estado de uma vez só: o tempo máximo para mostrar o
/// aviso do navegador sem esperar o limite inteiro.
const FATIA_DA_ESPERA: Duration = Duration::from_millis(500);

/// O endereço do servidor de controle do perfil: vazio é o oficial; senão precisa ser uma URL
/// `http://` ou `https://`. Devolve a URL sem a barra do fim.
///
/// # Errors
///
/// A mensagem, em português, de por que o endereço não serve.
pub fn url_do_servidor(servidor: &str) -> Result<String, String> {
    let servidor = servidor.trim();
    if servidor.is_empty() {
        return Ok(SERVIDOR_OFICIAL.to_owned());
    }
    let minusculo = servidor.to_ascii_lowercase();
    if !(minusculo.starts_with("https://") || minusculo.starts_with("http://"))
        || servidor.contains(char::is_whitespace)
    {
        return Err(t!("tailscale-servidor-invalido", servidor = servidor));
    }
    Ok(servidor.trim_end_matches('/').to_owned())
}

/// Se o servidor do perfil é o Tailscale oficial (vazio ou o endereço do próprio).
pub fn e_oficial(servidor: &str) -> bool {
    url_do_servidor(servidor).is_ok_and(|u| u == SERVIDOR_OFICIAL)
}

/// O estado do nó que o item do cofre guarda. Chaves vazias viram `None` (o motor gera as
/// primeiras); uma chave estragada é um erro, para o motor não virar outra máquina em silêncio.
///
/// # Errors
///
/// A mensagem, em português, de qual chave do item não serve.
pub fn no_de(identidade: &IdentidadeTailscale) -> Result<NodeState, String> {
    let chave_maquina = match identidade.chave_maquina.trim() {
        "" => None,
        texto => Some(
            MachineKey::from_text(texto)
                .map_err(|e| t!("tailscale-chave-invalida", erro = e.to_string()))?,
        ),
    };
    let chave_no = match identidade.chave_no.trim() {
        "" => None,
        texto => Some(
            NodeKey::from_text(texto)
                .map_err(|e| t!("tailscale-chave-invalida", erro = e.to_string()))?,
        ),
    };
    Ok(NodeState {
        machine_key: chave_maquina,
        node_key: chave_no,
        node_key_expiry: (identidade.vence_em > 0)
            .then(|| UNIX_EPOCH + Duration::from_secs(identidade.vence_em)),
        login: identidade.login.clone(),
        tailnet: identidade.tailnet.clone(),
        node_name: identidade.nome.clone(),
        addresses: identidade
            .enderecos
            .iter()
            .filter_map(|e| e.parse::<IpAddr>().ok())
            .collect(),
    })
}

/// Confere o estado de uma identidade que veio de fora (Bitwarden, JSON): as chaves precisam ler.
///
/// # Errors
///
/// A mensagem, em português, de qual chave não serve.
pub fn completar(identidade: &IdentidadeTailscale) -> Result<(), String> {
    no_de(identidade).map(|_| ())
}

/// Grava em `identidade` o estado novo do nó. A chave de autenticação e o servidor ficam como
/// estavam: o nó não os conhece.
pub fn gravar_no(identidade: &mut IdentidadeTailscale, no: &NodeState) {
    if let Some(chave) = &no.machine_key {
        identidade.chave_maquina = chave.to_text();
    }
    match &no.node_key {
        Some(chave) => identidade.chave_no = chave.to_text(),
        None => identidade.chave_no.clear(),
    }
    identidade.vence_em = no
        .node_key_expiry
        .and_then(|v| v.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    identidade.login = no.login.clone();
    identidade.tailnet = no.tailnet.clone();
    identidade.nome = no.node_name.clone();
    identidade.enderecos = no.addresses.iter().map(ToString::to_string).collect();
}

/// Esquece a máquina: apaga as chaves e o que o controle sabia dela, para o próximo uso entrar
/// como uma máquina nova. A chave de autenticação (que a pessoa guardou) e o servidor ficam.
pub fn esquecer_maquina(identidade: &mut IdentidadeTailscale) {
    identidade.chave_maquina.clear();
    identidade.chave_no.clear();
    identidade.vence_em = 0;
    identidade.login.clear();
    identidade.tailnet.clear();
    identidade.nome.clear();
    identidade.enderecos.clear();
}

/// Se o item já tem a máquina registrada (a chave do nó existe).
pub fn tem_maquina(identidade: &IdentidadeTailscale) -> bool {
    !identidade.chave_no.trim().is_empty()
}

/// A linha de estado da página Redes: `None` é a rede desligada.
pub fn descrever_estado(estado: Option<&State>) -> String {
    match estado {
        None | Some(State::Stopped) => t!("tailscale-estado-desligada"),
        Some(State::Connecting) => t!("tailscale-estado-conectando"),
        Some(State::NeedsLogin { .. }) => t!("tailscale-estado-precisa-entrar"),
        Some(State::NeedsApproval) => t!("tailscale-estado-esperando-aprovacao"),
        Some(State::Connected {
            address,
            tailnet,
            peers_online,
            ..
        }) => t!(
            "tailscale-estado-conectada",
            ip = address.to_string(),
            tailnet = tailnet.as_str(),
            online = *peers_online
        ),
        Some(State::Expired) => t!("tailscale-estado-venceu"),
        Some(State::Retrying(motivo)) => {
            t!("tailscale-estado-tentando", motivo = motivo.as_str())
        }
        Some(State::Failed(motivo)) => t!("tailscale-estado-falhou", motivo = motivo.as_str()),
    }
}

/// A URL de login do estado, se o nó está esperando a pessoa entrar.
pub fn url_de_login(estado: Option<&State>) -> Option<String> {
    match estado {
        Some(State::NeedsLogin { url }) => Some(url.clone()),
        _ => None,
    }
}

/// O primeiro endereço IPv4 da máquina.
fn ipv4_da(maquina: &Peer) -> Option<IpAddr> {
    maquina.addresses.iter().copied().find(IpAddr::is_ipv4)
}

/// O endereço para uma conexão nova a esta máquina: o IPv4 da tailnet, que não depende do DNS.
/// `None`: a máquina não tem endereço.
pub fn host_da_maquina(maquina: &Peer) -> Option<String> {
    ipv4_da(maquina)
        .or_else(|| maquina.addresses.first().copied())
        .map(|ip| ip.to_string())
}

/// O nome para mostrar da máquina: o nome curto do MagicDNS, ou o nome que o controle deu.
pub fn nome_da_maquina(maquina: &Peer) -> String {
    let dns = maquina.dns_name.trim().trim_end_matches('.');
    let curto = dns.split('.').next().unwrap_or("");
    if !curto.is_empty() {
        curto.to_owned()
    } else {
        maquina.name.trim().trim_end_matches('.').to_owned()
    }
}

/// Uma linha da lista de máquinas da página Redes: o nome, o endereço, o sistema e a situação.
pub fn descrever_maquina(maquina: &Peer) -> String {
    let ip = host_da_maquina(maquina).unwrap_or_else(|| t!("tailscale-maquina-sem-endereco"));
    let situacao = match maquina.online {
        Some(true) if maquina.path.is_empty() => t!("tailscale-maquina-online"),
        Some(true) => t!(
            "tailscale-maquina-online-por",
            caminho = maquina.path.as_str()
        ),
        Some(false) => t!("tailscale-maquina-offline"),
        None => t!("tailscale-maquina-desconhecida"),
    };
    let nome = nome_da_maquina(maquina);
    if maquina.os.is_empty() {
        t!(
            "tailscale-maquina",
            nome = nome,
            ip = ip,
            situacao = situacao
        )
    } else {
        t!(
            "tailscale-maquina-com-sistema",
            nome = nome,
            ip = ip,
            sistema = maquina.os.as_str(),
            situacao = situacao
        )
    }
}

fn sem_veneno<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O que o perfil e o item do cofre dizem para ligar o nó.
#[derive(Clone)]
pub struct Parametros {
    /// A URL do servidor de controle (já passada por [`url_do_servidor`]).
    pub servidor: String,
    /// O nome da máquina na tailnet; vazio usa o do motor.
    pub nome_maquina: String,
    /// O estado do nó guardado no cofre.
    pub no: NodeState,
    /// A chave de autenticação do item, se a pessoa guardou uma.
    pub chave_autenticacao: Option<Zeroizing<String>>,
    /// Usar as sub-redes que outras máquinas anunciam.
    pub usar_sub_redes: bool,
    /// A máquina é efêmera.
    pub efemero: bool,
    /// CAs extras em PEM para o TLS do servidor.
    pub ca: String,
}

// O estado do nó e a chave de autenticação não aparecem em `{:?}`.
impl std::fmt::Debug for Parametros {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parametros")
            .field("servidor", &self.servidor)
            .field("nome_maquina", &self.nome_maquina)
            .field("usar_sub_redes", &self.usar_sub_redes)
            .field("efemero", &self.efemero)
            .finish_non_exhaustive()
    }
}

/// O que identifica "este motor": o servidor, o nome, as opções e a chave de autenticação. Mudou,
/// o motor religa. O estado do nó **não** entra: o motor o muda sozinho (chaves novas, endereços)
/// e religar por isso seria um laço.
pub fn assinatura(parametros: &Parametros) -> u64 {
    let mut h = DefaultHasher::new();
    parametros.servidor.hash(&mut h);
    parametros.nome_maquina.hash(&mut h);
    parametros.usar_sub_redes.hash(&mut h);
    parametros.efemero.hash(&mut h);
    parametros.ca.hash(&mut h);
    parametros
        .chave_autenticacao
        .as_ref()
        .map(|c| c.as_str())
        .hash(&mut h);
    h.finish()
}

/// O motor ligado de um perfil, como a `RedeResolvida` o leva até o `Opener`.
#[derive(Clone)]
pub struct MotorTailscale {
    ts: Arc<Tailscale>,
    tempo: Handle,
}

impl PartialEq for MotorTailscale {
    fn eq(&self, outro: &Self) -> bool {
        Arc::ptr_eq(&self.ts, &outro.ts)
    }
}

impl Eq for MotorTailscale {}

// A ponte (porta e credencial) não aparece em `{:?}`.
impl std::fmt::Debug for MotorTailscale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotorTailscale")
            .field("estado", &self.ts.state())
            .finish_non_exhaustive()
    }
}

impl MotorTailscale {
    /// O proxy SOCKS5 da ponte do motor.
    pub fn proxy(&self) -> tomni_ui::terminal::Proxy {
        self.ts.proxy()
    }

    /// O estado agora.
    pub fn estado(&self) -> State {
        self.ts.state()
    }

    /// Espera o nó conectar (até `espera_secs`). O `aviso` recebe as linhas que o terminal mostra,
    /// inclusive a de que a pessoa precisa entrar pelo navegador (que o app já abriu) ou esperar
    /// a aprovação do administrador. Bloqueia a thread de quem chama, que não pode ser a da tela.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que a rede não conectou.
    pub fn esperar(&self, perfil: &PerfilRede, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        if matches!(self.ts.state(), State::Connected { .. }) {
            return Ok(());
        }
        aviso(&t!("tailscale-conectando", nome = perfil.name.as_str()));
        let limite = Duration::from_secs(u64::from(perfil.espera_secs.max(1)));
        let fim = Instant::now() + limite;
        let (mut avisou_login, mut avisou_aprovacao) = (false, false);
        loop {
            let resta = fim.saturating_duration_since(Instant::now());
            match self
                .tempo
                .block_on(self.ts.wait_connected(resta.min(FATIA_DA_ESPERA)))
            {
                Ok(ip) => {
                    aviso(&t!(
                        "tailscale-pronta",
                        nome = perfil.name.as_str(),
                        ip = ip.to_string()
                    ));
                    return Ok(());
                }
                Err(erro) if erro.kind() == std::io::ErrorKind::TimedOut => {}
                Err(erro) => {
                    return Err(t!(
                        "tailscale-nao-conectou",
                        nome = perfil.name.as_str(),
                        motivo = erro.to_string()
                    ));
                }
            }
            match self.ts.state() {
                State::NeedsLogin { .. } if !avisou_login => {
                    avisou_login = true;
                    aviso(&t!(
                        "tailscale-entre-pelo-navegador",
                        nome = perfil.name.as_str()
                    ));
                }
                State::NeedsApproval if !avisou_aprovacao => {
                    avisou_aprovacao = true;
                    aviso(&t!(
                        "tailscale-aguardando-aprovacao",
                        nome = perfil.name.as_str()
                    ));
                }
                _ => {}
            }
            if Instant::now() >= fim {
                let motivo = match self.ts.state() {
                    State::NeedsLogin { .. } => t!("tailscale-motivo-login"),
                    State::NeedsApproval => t!("tailscale-motivo-aprovacao"),
                    State::Retrying(motivo) => motivo,
                    outro => descrever_estado(Some(&outro)),
                };
                return Err(t!(
                    "tailscale-nao-conectou",
                    nome = perfil.name.as_str(),
                    motivo = motivo
                ));
            }
        }
    }
}

/// Um motor ligado e com o que o fez subir (para religar se o perfil mudou).
struct Ligado {
    ts: Arc<Tailscale>,
    assinatura: u64,
    /// Falso depois do [`MotoresTailscale::parar`]: os últimos estados do motor velho não valem
    /// mais.
    vivo: Arc<AtomicBool>,
    /// A trava do perfil entre as cópias do app ([`TravasDeRede`]): solta quando o motor para.
    _trava: Option<File>,
}

/// O que abre a URL de login no navegador. Os testes trocam por um que só anota.
type Abridor = Arc<dyn Fn(&str) + Send + Sync>;

fn abridor_do_sistema() -> Abridor {
    Arc::new(|url| {
        let _ = crate::ferramentas::abrir_endereco(url);
    })
}

/// Os nós Tailscale ligados, um por perfil, sobre um runtime próprio do `tokio` (o app não tem
/// outro). Ligam sob demanda e param quando o último uso do perfil fecha (veja `Usos`) ou quando
/// o app fecha.
pub struct MotoresTailscale {
    tempo: Mutex<Option<Arc<Runtime>>>,
    ligados: Mutex<HashMap<u64, Ligado>>,
    /// O último estado de cada perfil, para a página Redes.
    estados: Arc<Mutex<HashMap<u64, State>>>,
    /// O estado novo do nó de cada perfil, esperando a tela gravar no cofre.
    pendentes: Arc<Mutex<HashMap<u64, NodeState>>>,
    /// A última URL de login que o app abriu no navegador, por perfil: a mesma não abre de novo.
    logins: Arc<Mutex<HashMap<u64, String>>>,
    /// O que abre a URL de login.
    abridor: Arc<Mutex<Abridor>>,
    /// O contador que acorda a tela quando um estado muda.
    acordar: Arc<Mutex<Option<Signal<u64>>>>,
    /// Uma cópia do app por vez liga cada perfil.
    travas: TravasDeRede,
}

impl Default for MotoresTailscale {
    fn default() -> Self {
        Self {
            tempo: Mutex::default(),
            ligados: Mutex::default(),
            estados: Arc::default(),
            pendentes: Arc::default(),
            logins: Arc::default(),
            abridor: Arc::new(Mutex::new(abridor_do_sistema())),
            acordar: Arc::default(),
            travas: TravasDeRede::default(),
        }
    }
}

impl MotoresTailscale {
    /// Avisa a tela (por este contador) cada vez que o estado de um motor muda.
    pub fn ao_mudar(&self, sinal: Signal<u64>) {
        *sem_veneno(&self.acordar) = Some(sinal);
    }

    /// A pasta de configuração, onde fica a trava de cada perfil entre as cópias do app.
    pub fn na_pasta(&self, pasta: &Path) {
        self.travas.na_pasta(pasta);
    }

    /// Troca o que abre a URL de login (os testes não abrem navegador).
    pub fn definir_abridor(&self, abridor: impl Fn(&str) + Send + Sync + 'static) {
        *sem_veneno(&self.abridor) = Arc::new(abridor);
    }

    /// Abre a URL de login do perfil no navegador (o "Abrir de novo").
    pub fn abrir_login(&self, url: &str) {
        let abridor = Arc::clone(&sem_veneno(&self.abridor));
        abridor(url);
    }

    fn runtime(&self) -> Result<Arc<Runtime>, String> {
        let mut tempo = sem_veneno(&self.tempo);
        if let Some(r) = tempo.as_ref() {
            return Ok(Arc::clone(r));
        }
        let novo = Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("tailscale")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let novo = Arc::new(novo);
        *tempo = Some(Arc::clone(&novo));
        Ok(novo)
    }

    /// Liga o nó do perfil, se ainda não está ligado com esta assinatura, e devolve o acesso à
    /// ponte. Não espera a conexão (isso é o [`MotorTailscale::esperar`]).
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que o nó não subiu. O estado do perfil fica `Failed`.
    pub fn garantir(
        &self,
        perfil: &PerfilRede,
        parametros: Result<Parametros, String>,
    ) -> Result<MotorTailscale, String> {
        let tempo = self.runtime()?;
        // Sem parâmetros (o estado do cofre não serve), a assinatura nem existe: o motor velho,
        // se há, para antes de falhar.
        let assinatura = parametros.as_ref().ok().map(assinatura);
        if let Some(assinatura) = assinatura {
            let ligados = sem_veneno(&self.ligados);
            if let Some(l) = ligados.get(&perfil.id)
                && l.assinatura == assinatura
            {
                return Ok(MotorTailscale {
                    ts: Arc::clone(&l.ts),
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
                "tailscale-nao-subiu",
                nome = perfil.name.as_str(),
                motivo = motivo
            )
        };
        let parametros = parametros.map_err(&falhou)?;
        let assinatura = assinatura.unwrap_or_default();
        let trava = self.travas.travar(perfil).map_err(&falhou)?;
        let vivo = Arc::new(AtomicBool::new(true));
        let (estados, pendentes, logins, abridor, acordar, id, vivo_do_motor) = (
            Arc::clone(&self.estados),
            Arc::clone(&self.pendentes),
            Arc::clone(&self.logins),
            Arc::clone(&self.abridor),
            Arc::clone(&self.acordar),
            perfil.id,
            Arc::clone(&vivo),
        );
        let (pendentes_do_no, acordar_do_no, vivo_do_no) = (
            Arc::clone(&pendentes),
            Arc::clone(&acordar),
            Arc::clone(&vivo),
        );
        let mut construtor = TailscaleBuilder::new(parametros.servidor.clone(), parametros.no)
            .ephemeral(parametros.efemero)
            .accept_routes(parametros.usar_sub_redes)
            .on_state(move |estado| {
                if !vivo_do_motor.load(Ordering::Relaxed) {
                    return;
                }
                if let State::NeedsLogin { url } = estado {
                    // A mesma URL abre uma vez só; "Abrir de novo" é por pedido.
                    let nova = sem_veneno(&logins).insert(id, url.clone()).as_ref() != Some(url);
                    if nova {
                        let abridor = Arc::clone(&sem_veneno(&abridor));
                        abridor(url);
                    }
                }
                sem_veneno(&estados).insert(id, estado.clone());
                acordar_a_tela(&acordar);
            })
            .on_node_state(move |no| {
                if vivo_do_no.load(Ordering::Relaxed) {
                    sem_veneno(&pendentes_do_no).insert(id, no.clone());
                    acordar_a_tela(&acordar_do_no);
                }
            });
        if !parametros.nome_maquina.trim().is_empty() {
            construtor = construtor.hostname(parametros.nome_maquina.trim());
        }
        if let Some(chave) = parametros.chave_autenticacao {
            construtor = construtor.auth_key(chave);
        }
        if !parametros.ca.trim().is_empty() {
            construtor = construtor.extra_roots(parametros.ca.clone());
        }
        let ligado = tempo
            .block_on(construtor.start())
            .map_err(|e| falhou(e.to_string()))?;
        let ligado = Arc::new(ligado);
        sem_veneno(&self.ligados).insert(
            perfil.id,
            Ligado {
                ts: Arc::clone(&ligado),
                assinatura,
                vivo,
                _trava: trava,
            },
        );
        self.guardar_estado(perfil.id, ligado.state());
        Ok(MotorTailscale {
            ts: ligado,
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
        sem_veneno(&self.logins).remove(&perfil);
        acordar_a_tela(&self.acordar);
        if let Ok(tempo) = self.runtime() {
            tempo.spawn(parar_um(ligado.ts));
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
                tarefas.spawn(parar_um(l.ts));
            }
            let _ = tokio::time::timeout(Duration::from_millis(200), async {
                while tarefas.join_next().await.is_some() {}
            })
            .await;
        });
        sem_veneno(&self.estados).clear();
        sem_veneno(&self.logins).clear();
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

    /// As máquinas da tailnet agora, com o caminho de cada uma. Vazio se o perfil está desligado.
    pub fn maquinas(&self, perfil: u64) -> Vec<Peer> {
        let ts = {
            let ligados = sem_veneno(&self.ligados);
            let Some(l) = ligados.get(&perfil) else {
                return Vec::new();
            };
            Arc::clone(&l.ts)
        };
        match self.runtime() {
            Ok(tempo) => tempo.block_on(ts.peers()),
            Err(_) => Vec::new(),
        }
    }

    /// Os perfis que têm estado de nó novo esperando a gravação no cofre, sem tirá-los.
    pub fn perfis_pendentes(&self) -> HashSet<u64> {
        sem_veneno(&self.pendentes).keys().copied().collect()
    }

    /// Tira o estado novo do nó do perfil, para a tela gravá-lo no item do cofre. Se a gravação
    /// falhar, [`MotoresTailscale::devolver`] o põe de volta.
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
async fn parar_um(ts: Arc<Tailscale>) {
    match Arc::try_unwrap(ts) {
        Ok(ts) => ts.stop(Duration::from_millis(200)).await,
        Err(restante) => drop(restante),
    }
}

fn acordar_a_tela(acordar: &Mutex<Option<Signal<u64>>>) {
    if let Some(sinal) = *sem_veneno(acordar) {
        crate::agente::acordar(sinal);
    }
}

/// A hora de `vence_em`, para mostrar ("a chave vence em…"): `None` se não há validade.
pub fn vence_em_como_hora(vence_em: u64) -> Option<SystemTime> {
    (vence_em > 0).then(|| UNIX_EPOCH + Duration::from_secs(vence_em))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identidade() -> IdentidadeTailscale {
        IdentidadeTailscale {
            servidor: "https://hs.exemplo.test".into(),
            chave_maquina: MachineKey::generate().to_text(),
            chave_no: NodeKey::generate().to_text(),
            vence_em: 1_900_000_000,
            chave_autenticacao: "tskey-auth-segredo".into(),
            login: "ana@exemplo.test".into(),
            tailnet: "exemplo.ts.net".into(),
            nome: "meu-app".into(),
            enderecos: vec!["100.64.0.7".into(), "fd7a:115c:a1e0::7".into()],
        }
    }

    fn parametros() -> Parametros {
        Parametros {
            servidor: SERVIDOR_OFICIAL.into(),
            nome_maquina: "meu-app".into(),
            no: NodeState::default(),
            chave_autenticacao: None,
            usar_sub_redes: true,
            efemero: false,
            ca: String::new(),
        }
    }

    #[test]
    fn o_servidor_vazio_e_o_oficial_e_a_url_perde_a_barra_final() {
        assert_eq!(url_do_servidor("").unwrap(), SERVIDOR_OFICIAL);
        assert_eq!(url_do_servidor("  ").unwrap(), SERVIDOR_OFICIAL);
        assert_eq!(
            url_do_servidor(" https://hs.exemplo.test/ ").unwrap(),
            "https://hs.exemplo.test"
        );
        assert_eq!(
            url_do_servidor("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert!(e_oficial(""));
        assert!(e_oficial(SERVIDOR_OFICIAL));
        assert!(!e_oficial("https://hs.exemplo.test"));
    }

    #[test]
    fn servidor_que_nao_e_url_e_recusado() {
        for ruim in ["hs.exemplo.test", "ftp://hs.exemplo.test", "https://a b"] {
            let erro = url_do_servidor(ruim).unwrap_err();
            assert_eq!(erro, t!("tailscale-servidor-invalido", servidor = ruim));
        }
    }

    #[test]
    fn o_estado_do_no_vai_e_volta_pelo_item() {
        let item = identidade();
        let no = no_de(&item).unwrap();
        assert!(no.machine_key.is_some() && no.node_key.is_some());
        assert_eq!(no.addresses.len(), 2);
        assert_eq!(
            no.node_key_expiry,
            Some(UNIX_EPOCH + Duration::from_secs(1_900_000_000))
        );
        let mut volta = IdentidadeTailscale {
            servidor: item.servidor.clone(),
            chave_autenticacao: item.chave_autenticacao.clone(),
            ..IdentidadeTailscale::default()
        };
        gravar_no(&mut volta, &no);
        assert_eq!(volta, item);
    }

    #[test]
    fn item_sem_chaves_comeca_um_no_novo_e_chave_estragada_e_erro() {
        let vazio = no_de(&IdentidadeTailscale::default()).unwrap();
        assert!(vazio.machine_key.is_none() && vazio.node_key.is_none());
        let mut ruim = identidade();
        ruim.chave_no = "lixo".into();
        let erro = no_de(&ruim).unwrap_err();
        assert!(
            erro.starts_with(&t!("tailscale-chave-invalida", erro = "")),
            "{erro}"
        );
    }

    #[test]
    fn esquecer_a_maquina_guarda_so_o_servidor_e_a_chave_de_autenticacao() {
        let mut item = identidade();
        assert!(tem_maquina(&item));
        esquecer_maquina(&mut item);
        assert!(!tem_maquina(&item));
        assert_eq!(item.servidor, "https://hs.exemplo.test");
        assert_eq!(item.chave_autenticacao, "tskey-auth-segredo");
        assert!(item.chave_maquina.is_empty() && item.login.is_empty());
        assert!(item.enderecos.is_empty() && item.vence_em == 0);
    }

    #[test]
    fn a_assinatura_ignora_o_estado_do_no_e_muda_com_o_resto() {
        let base = assinatura(&parametros());
        let mut com_chaves = parametros();
        com_chaves.no = no_de(&identidade()).unwrap();
        assert_eq!(base, assinatura(&com_chaves), "o estado do nó não religa");
        let mut outro = parametros();
        outro.servidor = "https://hs.exemplo.test".into();
        assert_ne!(base, assinatura(&outro));
        let mut outro = parametros();
        outro.chave_autenticacao = Some(Zeroizing::new("tskey-auth-x".into()));
        assert_ne!(base, assinatura(&outro));
        let mut outro = parametros();
        outro.usar_sub_redes = false;
        assert_ne!(base, assinatura(&outro));
        let mut outro = parametros();
        outro.efemero = true;
        assert_ne!(base, assinatura(&outro));
        let mut outro = parametros();
        outro.nome_maquina = "outro".into();
        assert_ne!(base, assinatura(&outro));
    }

    #[test]
    fn o_estado_vira_texto_da_pagina() {
        assert_eq!(descrever_estado(None), t!("tailscale-estado-desligada"));
        assert_eq!(
            descrever_estado(Some(&State::NeedsLogin {
                url: "https://x".into()
            })),
            t!("tailscale-estado-precisa-entrar")
        );
        assert_eq!(
            descrever_estado(Some(&State::NeedsApproval)),
            t!("tailscale-estado-esperando-aprovacao")
        );
        assert_eq!(
            descrever_estado(Some(&State::Failed("sem rede".into()))),
            t!("tailscale-estado-falhou", motivo = "sem rede")
        );
        let conectada = State::Connected {
            address: "100.64.0.7".parse().unwrap(),
            tailnet: "exemplo.ts.net".into(),
            peers_online: 3,
            derp_home: String::new(),
        };
        let texto = descrever_estado(Some(&conectada));
        assert!(
            texto.contains("100.64.0.7") && texto.contains("exemplo.ts.net"),
            "{texto}"
        );
        assert_eq!(
            url_de_login(Some(&State::NeedsLogin {
                url: "https://x".into()
            }))
            .as_deref(),
            Some("https://x")
        );
        assert_eq!(url_de_login(Some(&State::NeedsApproval)), None);
    }

    fn maquina() -> Peer {
        Peer {
            name: "servidor-1.exemplo.ts.net.".into(),
            dns_name: "servidor-1.exemplo.ts.net.".into(),
            addresses: vec![
                "fd7a:115c:a1e0::9".parse().unwrap(),
                "100.64.0.9".parse().unwrap(),
            ],
            os: "linux".into(),
            online: Some(true),
            last_seen: None,
            user: "ana@exemplo.test".into(),
            tags: Vec::new(),
            path: "direto".into(),
        }
    }

    #[test]
    fn a_maquina_usa_o_ipv4_e_o_nome_curto() {
        let m = maquina();
        assert_eq!(host_da_maquina(&m).as_deref(), Some("100.64.0.9"));
        assert_eq!(nome_da_maquina(&m), "servidor-1");
        let texto = descrever_maquina(&m);
        assert!(
            texto.contains("servidor-1") && texto.contains("100.64.0.9"),
            "{texto}"
        );
        let mut sem_dns = maquina();
        sem_dns.dns_name.clear();
        sem_dns.name = "servidor-1".into();
        assert_eq!(nome_da_maquina(&sem_dns), "servidor-1");
        sem_dns.addresses.clear();
        assert_eq!(host_da_maquina(&sem_dns), None);
    }

    #[test]
    fn o_debug_da_identidade_e_dos_parametros_nao_mostra_segredo() {
        let item = identidade();
        let texto = format!("{item:?}");
        assert!(!texto.contains(&item.chave_no));
        assert!(!texto.contains(&item.chave_maquina));
        assert!(!texto.contains("tskey-auth-segredo"));
        let mut p = parametros();
        p.chave_autenticacao = Some(Zeroizing::new("tskey-auth-segredo".into()));
        assert!(!format!("{p:?}").contains("tskey-auth-segredo"));
    }
}
