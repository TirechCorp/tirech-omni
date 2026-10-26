//! O WireGuard embutido (M57): o motor da lib `tomni-ui-wireguard` roda dentro do app, só para o
//! tráfego dele, sem placa virtual nem privilégio. Este módulo importa o `.conf`, separa os
//! segredos (que vão para o cofre) do texto do perfil (que não tem nenhum), monta o `WgConfig` na
//! hora de ligar e guarda os motores ligados, um por perfil de rede.
//!
//! É o par de `nebula.rs`: a rede entrega ao resto do app um proxy SOCKS5 de `127.0.0.1` (a
//! ponte do motor), que a tela põe no `perfil.proxy` da `RedeResolvida`. A ponte nunca aparece
//! para quem usa o app.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinSet;
use tomni_ui::prelude::Signal;
use tomni_ui_wireguard::{
    PeerInfo, PresharedKey, PrivateKey, State, WgConfig, WireGuard, WireGuardBuilder,
};

use crate::cofre::IdentidadeWireGuard;
use crate::redes::{PerfilRede, TravasDeRede};
use crate::t;

/// O que sobra de uma importação: o `.conf` sem os segredos, a identidade que vai para o cofre e o
/// que a lib avisou.
#[derive(Clone, Debug)]
pub struct Importacao {
    /// O `.conf` sem a `PrivateKey` e sem as `PresharedKey`. Nada aqui é segredo.
    pub conf: String,
    /// A chave privada, as chaves pré-compartilhadas e o resumo para mostrar.
    pub identidade: IdentidadeWireGuard,
    /// Os avisos da leitura (opções do `wg-quick` que o app ignora, como `PostUp`).
    pub avisos: Vec<String>,
}

/// Importa o `.conf` em `texto`.
///
/// # Errors
///
/// A mensagem, em português, do que impede a importação.
pub fn importar(texto: &str) -> Result<Importacao, String> {
    let (config, avisos) = WgConfig::from_conf(texto).map_err(|e| e.to_string())?;
    config.check().map_err(|e| e.to_string())?;
    let identidade = IdentidadeWireGuard {
        chave_privada: config.interface.private_key.to_base64().to_string(),
        chave_publica: config.interface.private_key.public_key().to_base64(),
        psks: config
            .peers
            .iter()
            .filter_map(|p| {
                p.preshared_key
                    .as_ref()
                    .map(|k| (p.public_key.to_base64(), k.to_base64().to_string()))
            })
            .collect(),
        enderecos: config
            .interface
            .addresses
            .iter()
            .map(ToString::to_string)
            .collect(),
    };
    let conf = config.without_secrets();
    // O que sobrou precisa ler sozinho e sem nenhum segredo.
    conferir_conf(&conf)?;
    Ok(Importacao {
        conf,
        identidade,
        avisos: avisos.iter().map(ToString::to_string).collect(),
    })
}

/// Confere as chaves de uma identidade que veio de fora (Bitwarden, JSON) e preenche a chave
/// pública a partir da privada, que é a que vale.
///
/// # Errors
///
/// A mensagem, em português, de qual chave não serve.
pub fn completar(identidade: &mut IdentidadeWireGuard) -> Result<(), String> {
    let privada = PrivateKey::from_base64(identidade.chave_privada.trim())
        .map_err(|e| t!("wireguard-chave-invalida", erro = e.to_string()))?;
    for (_, psk) in &identidade.psks {
        PresharedKey::from_base64(psk.trim())
            .map_err(|e| t!("wireguard-chave-invalida", erro = e.to_string()))?;
    }
    identidade.chave_publica = privada.public_key().to_base64();
    Ok(())
}

/// Se a linha (sem comentário) é a chave `nome` do `.conf` (sem diferenciar maiúsculas) e, se for,
/// o valor dela.
fn valor_de<'a>(linha: &'a str, nome: &str) -> Option<&'a str> {
    let sem_comentario = linha.split('#').next().unwrap_or("").trim();
    let (k, v) = sem_comentario.split_once('=')?;
    k.trim().eq_ignore_ascii_case(nome).then(|| v.trim())
}

/// Se a linha, sem comentário, é o cabeçalho `[nome]`.
fn e_secao(linha: &str, nome: &str) -> bool {
    let sem_comentario = linha.split('#').next().unwrap_or("").trim();
    sem_comentario.eq_ignore_ascii_case(&format!("[{nome}]"))
}

/// Confere o texto de um `.conf` que vai para o perfil (importado ou editado à mão): precisa ler e
/// não pode trazer segredo nenhum.
///
/// # Errors
///
/// A mensagem, em português, de por que o texto não serve.
pub fn conferir_conf(texto: &str) -> Result<(), String> {
    if texto
        .lines()
        .any(|l| valor_de(l, "PrivateKey").is_some() || valor_de(l, "PresharedKey").is_some())
    {
        return Err(t!("wireguard-config-com-segredo"));
    }
    // A leitura exige a `PrivateKey`: uma qualquer, só para conferir o resto.
    let provisoria = PrivateKey::generate().map_err(|e| e.to_string())?;
    let completo = com_chaves(texto, &provisoria.to_base64(), &[]);
    let (config, _) = WgConfig::from_conf(&completo).map_err(|e| e.to_string())?;
    config.check().map_err(|e| e.to_string())
}

/// O `.conf` com a `PrivateKey` logo depois de `[Interface]` e cada `PresharedKey` logo depois da
/// `PublicKey` do par dela.
fn com_chaves(conf: &str, chave_privada: &str, psks: &[(String, String)]) -> String {
    let mut saida = String::with_capacity(conf.len() + 128);
    for linha in conf.lines() {
        saida.push_str(linha);
        saida.push('\n');
        if e_secao(linha, "Interface") {
            saida.push_str(&format!("PrivateKey = {chave_privada}\n"));
        } else if let Some(publica) = valor_de(linha, "PublicKey")
            && let Some((_, psk)) = psks.iter().find(|(p, _)| p == publica)
        {
            saida.push_str(&format!("PresharedKey = {psk}\n"));
        }
    }
    saida
}

/// O `WgConfig` completo do perfil: o `.conf` do perfil mais as chaves da identidade.
///
/// # Errors
///
/// A mensagem, em português, de por que o texto ou a identidade não servem.
pub fn config_de(conf: &str, identidade: &IdentidadeWireGuard) -> Result<WgConfig, String> {
    // Confere a chave antes de montar o texto: a mensagem diz o que está errado.
    PrivateKey::from_base64(identidade.chave_privada.trim())
        .map_err(|e| t!("wireguard-chave-invalida", erro = e.to_string()))?;
    for (_, psk) in &identidade.psks {
        PresharedKey::from_base64(psk.trim())
            .map_err(|e| t!("wireguard-chave-invalida", erro = e.to_string()))?;
    }
    let completo = com_chaves(conf, identidade.chave_privada.trim(), &identidade.psks);
    let (config, _) = WgConfig::from_conf(&completo).map_err(|e| e.to_string())?;
    config.check().map_err(|e| e.to_string())?;
    Ok(config)
}

/// Os endereços de servidor (`Endpoint`) dos pares do `.conf`, para a página Redes.
pub fn servidores(conf: &str) -> Vec<String> {
    conf.lines()
        .filter_map(|l| valor_de(l, "Endpoint"))
        .map(str::to_owned)
        .collect()
}

/// As redes que os pares atendem (`AllowedIPs`), para a página Redes.
pub fn rotas(conf: &str) -> Vec<String> {
    conf.lines()
        .filter_map(|l| valor_de(l, "AllowedIPs"))
        .flat_map(|v| v.split(','))
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty())
        .collect()
}

/// A linha de estado da página Redes: `None` é a rede desligada.
pub fn descrever_estado(estado: Option<&State>) -> String {
    match estado {
        None | Some(State::Stopped) => t!("wireguard-estado-desligada"),
        Some(State::Connecting) => t!("wireguard-estado-conectando"),
        Some(State::Connected {
            address,
            peers_up,
            peers,
        }) => t!(
            "wireguard-estado-conectada",
            ip = address.to_string(),
            ativos = *peers_up,
            pares = *peers
        ),
        Some(State::Retrying(motivo)) => {
            t!("wireguard-estado-tentando", motivo = motivo.as_str())
        }
        Some(State::Failed(motivo)) => t!("wireguard-estado-falhou", motivo = motivo.as_str()),
    }
}

/// Uma linha da lista de pares da página Redes: a chave curta, o endereço, o handshake e o tráfego.
pub fn descrever_par(par: &PeerInfo) -> String {
    let endereco = par
        .endpoint
        .map_or_else(|| t!("wireguard-par-sem-endereco"), |e| e.to_string());
    let situacao = match par.last_handshake {
        Some(h) if par.connected => t!("wireguard-par-handshake", segundos = h.as_secs()),
        _ => t!("wireguard-par-sem-sessao"),
    };
    t!(
        "wireguard-par",
        chave = par.public_key.short(),
        endereco = endereco,
        situacao = situacao,
        enviado = par.tx_bytes,
        recebido = par.rx_bytes
    )
}

fn sem_veneno<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O motor ligado de um perfil, como a `RedeResolvida` o leva até o `Opener`.
#[derive(Clone)]
pub struct MotorWireGuard {
    wg: Arc<WireGuard>,
    tempo: Handle,
}

impl PartialEq for MotorWireGuard {
    fn eq(&self, outro: &Self) -> bool {
        Arc::ptr_eq(&self.wg, &outro.wg)
    }
}

impl Eq for MotorWireGuard {}

// A ponte (porta e credencial) não aparece em `{:?}`.
impl std::fmt::Debug for MotorWireGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotorWireGuard")
            .field("estado", &self.wg.state())
            .finish_non_exhaustive()
    }
}

impl MotorWireGuard {
    /// O proxy SOCKS5 da ponte do motor.
    pub fn proxy(&self) -> tomni_ui::terminal::Proxy {
        self.wg.proxy()
    }

    /// O estado agora.
    pub fn estado(&self) -> State {
        self.wg.state()
    }

    /// Espera o motor conectar (até `espera_secs`). O `aviso` recebe as linhas que o terminal
    /// mostra. Bloqueia a thread de quem chama, que não pode ser a da tela.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que a rede não conectou.
    pub fn esperar(&self, perfil: &PerfilRede, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        if matches!(self.wg.state(), State::Connected { .. }) {
            return Ok(());
        }
        aviso(&t!("wireguard-conectando", nome = perfil.name.as_str()));
        let limite = Duration::from_secs(u64::from(perfil.espera_secs.max(1)));
        match self.tempo.block_on(self.wg.wait_connected(limite)) {
            Ok(()) => {
                let ip = self
                    .wg
                    .state()
                    .address()
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                aviso(&t!(
                    "wireguard-pronta",
                    nome = perfil.name.as_str(),
                    ip = ip
                ));
                Ok(())
            }
            Err(erro) => Err(t!(
                "wireguard-nao-conectou",
                nome = perfil.name.as_str(),
                motivo = erro.to_string()
            )),
        }
    }
}

/// Um motor ligado e com o que o fez subir (para religar se o `.conf` ou a identidade mudou).
struct Ligado {
    wg: Arc<WireGuard>,
    assinatura: u64,
    /// Falso depois do [`MotoresWireGuard::parar`]: os últimos estados do motor velho não valem
    /// mais.
    vivo: Arc<AtomicBool>,
    /// A trava do perfil entre as cópias do app ([`TravasDeRede`]): solta quando o motor para.
    _trava: Option<File>,
}

/// Os motores WireGuard ligados, um por perfil, sobre um runtime próprio do `tokio` (o app não tem
/// outro). Ligam sob demanda e param quando o último uso do perfil fecha (veja `Usos`) ou quando
/// o app fecha.
#[derive(Default)]
pub struct MotoresWireGuard {
    tempo: Mutex<Option<Arc<Runtime>>>,
    ligados: Mutex<HashMap<u64, Ligado>>,
    /// O último estado de cada perfil, para a página Redes.
    estados: Arc<Mutex<HashMap<u64, State>>>,
    /// O contador que acorda a tela quando um estado muda.
    acordar: Arc<Mutex<Option<Signal<u64>>>>,
    /// Uma cópia do app por vez liga cada perfil.
    travas: TravasDeRede,
}

impl MotoresWireGuard {
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
            .thread_name("wireguard")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let novo = Arc::new(novo);
        *tempo = Some(Arc::clone(&novo));
        Ok(novo)
    }

    /// Liga o motor do perfil, se ainda não está ligado com esta `assinatura`, e devolve o acesso
    /// à ponte. Não espera a conexão (isso é o [`MotorWireGuard::esperar`]).
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que o motor não subiu. O estado do perfil fica `Failed`.
    pub fn garantir(
        &self,
        perfil: &PerfilRede,
        config: Result<WgConfig, String>,
        assinatura: u64,
    ) -> Result<MotorWireGuard, String> {
        let tempo = self.runtime()?;
        {
            let ligados = sem_veneno(&self.ligados);
            if let Some(l) = ligados.get(&perfil.id)
                && l.assinatura == assinatura
            {
                return Ok(MotorWireGuard {
                    wg: Arc::clone(&l.wg),
                    tempo: tempo.handle().clone(),
                });
            }
        }
        // O `.conf` ou a identidade mudou desde que subiu: o motor velho para.
        self.parar(perfil.id);
        let falhou = |motivo: String| {
            // A página mostra só o motivo (cabe na linha); o aviso leva o nome da rede.
            self.guardar_estado(perfil.id, State::Failed(motivo.clone()));
            t!(
                "wireguard-nao-subiu",
                nome = perfil.name.as_str(),
                motivo = motivo
            )
        };
        let config = config.map_err(&falhou)?;
        let trava = self.travas.travar(perfil).map_err(&falhou)?;
        let vivo = Arc::new(AtomicBool::new(true));
        let (estados, acordar, id, vivo_do_motor) = (
            Arc::clone(&self.estados),
            Arc::clone(&self.acordar),
            perfil.id,
            Arc::clone(&vivo),
        );
        let ligado = tempo
            .block_on(
                WireGuardBuilder::new(config)
                    .on_state(move |estado| {
                        if vivo_do_motor.load(Ordering::Relaxed) {
                            sem_veneno(&estados).insert(id, estado);
                            acordar_a_tela(&acordar);
                        }
                    })
                    .start(),
            )
            .map_err(|e| falhou(e.to_string()))?;
        let ligado = Arc::new(ligado);
        sem_veneno(&self.ligados).insert(
            perfil.id,
            Ligado {
                wg: Arc::clone(&ligado),
                assinatura,
                vivo,
                _trava: trava,
            },
        );
        self.guardar_estado(perfil.id, ligado.state());
        Ok(MotorWireGuard {
            wg: ligado,
            tempo: tempo.handle().clone(),
        })
    }

    fn guardar_estado(&self, perfil: u64, estado: State) {
        sem_veneno(&self.estados).insert(perfil, estado);
        acordar_a_tela(&self.acordar);
    }

    /// Para o motor do perfil (se está ligado), sem esperar.
    pub fn parar(&self, perfil: u64) {
        let Some(ligado) = sem_veneno(&self.ligados).remove(&perfil) else {
            return;
        };
        ligado.vivo.store(false, Ordering::Relaxed);
        sem_veneno(&self.estados).remove(&perfil);
        acordar_a_tela(&self.acordar);
        if let Ok(tempo) = self.runtime() {
            tempo.spawn(parar_um(ligado.wg));
        }
    }

    /// Para todos, esperando no máximo 200 ms. O WireGuard não tem "fechar": não há nada a avisar
    /// aos pares, só soltar a porta UDP e a ponte.
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
                tarefas.spawn(parar_um(l.wg));
            }
            let _ = tokio::time::timeout(Duration::from_millis(200), async {
                while tarefas.join_next().await.is_some() {}
            })
            .await;
        });
        sem_veneno(&self.estados).clear();
    }

    /// O estado do motor do perfil. `None`: desligada.
    pub fn estado(&self, perfil: u64) -> Option<State> {
        sem_veneno(&self.estados)
            .get(&perfil)
            .filter(|e| **e != State::Stopped)
            .cloned()
    }

    /// Se o perfil tem motor ligado.
    pub fn ligado(&self, perfil: u64) -> bool {
        sem_veneno(&self.ligados).contains_key(&perfil)
    }

    /// O que se sabe de cada par do perfil agora. Vazio se está desligado.
    pub fn pares(&self, perfil: u64) -> Vec<PeerInfo> {
        let wg = {
            let ligados = sem_veneno(&self.ligados);
            let Some(l) = ligados.get(&perfil) else {
                return Vec::new();
            };
            Arc::clone(&l.wg)
        };
        match self.runtime() {
            Ok(tempo) => tempo.block_on(wg.peers()),
            Err(_) => Vec::new(),
        }
    }
}

/// Para um motor: com a única referência, o `stop` fecha a ponte e a pilha; se ainda há quem
/// segure o `Arc` (uma conexão em andamento), o `Drop` do motor para quando o último largar.
async fn parar_um(wg: Arc<WireGuard>) {
    match Arc::try_unwrap(wg) {
        Ok(wg) => wg.stop(Duration::from_millis(200)).await,
        Err(restante) => drop(restante),
    }
}

fn acordar_a_tela(acordar: &Mutex<Option<Signal<u64>>>) {
    if let Some(sinal) = *sem_veneno(acordar) {
        crate::agente::acordar(sinal);
    }
}

/// O que identifica "este motor": o `.conf` e as chaves. Mudou, o motor religa.
pub fn assinatura(conf: &str, identidade: &IdentidadeWireGuard) -> u64 {
    let mut h = DefaultHasher::new();
    conf.hash(&mut h);
    identidade.chave_privada.hash(&mut h);
    identidade.psks.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = include_str!("../tests/dados/wireguard/cliente.conf");

    #[test]
    fn importar_separa_o_conf_dos_segredos() {
        let i = importar(CONF).unwrap();
        assert!(!i.identidade.chave_privada.is_empty());
        assert_eq!(i.identidade.chave_publica.len(), 44);
        assert_eq!(i.identidade.psks.len(), 1);
        assert_eq!(i.identidade.enderecos, ["10.8.0.2/24"]);
        // O texto do perfil não tem nenhum segredo e continua lendo.
        assert!(!i.conf.contains("PrivateKey"));
        assert!(!i.conf.contains("PresharedKey"));
        assert!(!i.conf.contains(&i.identidade.chave_privada));
        assert!(!i.conf.contains(&i.identidade.psks[0].1));
        assert!(i.conf.contains("Endpoint = vpn.exemplo.test:51820"));
        conferir_conf(&i.conf).unwrap();
    }

    #[test]
    fn a_identidade_refaz_o_config_completo() {
        let i = importar(CONF).unwrap();
        let config = config_de(&i.conf, &i.identidade).unwrap();
        let (original, _) = WgConfig::from_conf(CONF).unwrap();
        assert_eq!(config.interface.private_key, original.interface.private_key);
        assert_eq!(config.interface.addresses, original.interface.addresses);
        assert_eq!(config.peers.len(), original.peers.len());
        assert_eq!(
            config.peers[0]
                .preshared_key
                .as_ref()
                .map(|k| k.to_base64()),
            original.peers[0]
                .preshared_key
                .as_ref()
                .map(|k| k.to_base64())
        );
        assert_eq!(config.peers[0].allowed_ips, original.peers[0].allowed_ips);
    }

    #[test]
    fn config_com_segredo_no_texto_e_recusado() {
        let erro = conferir_conf(CONF).unwrap_err();
        assert_eq!(erro, t!("wireguard-config-com-segredo"));
    }

    #[test]
    fn config_que_nao_le_e_recusado() {
        assert!(importar("isto não é um .conf").is_err());
        assert!(importar("[Interface]\nAddress = 10.8.0.2/24\n").is_err());
    }

    #[test]
    fn chave_estragada_na_identidade_diz_o_que_esta_errado() {
        let mut i = importar(CONF).unwrap();
        i.identidade.chave_privada = "lixo".into();
        let erro = config_de(&i.conf, &i.identidade).unwrap_err();
        assert!(
            erro.starts_with(&t!("wireguard-chave-invalida", erro = "")),
            "{erro}"
        );
    }

    #[test]
    fn a_assinatura_muda_com_qualquer_parte() {
        let i = importar(CONF).unwrap();
        let base = assinatura(&i.conf, &i.identidade);
        assert_eq!(base, assinatura(&i.conf, &i.identidade));
        assert_ne!(base, assinatura(&format!("{}\n", i.conf), &i.identidade));
        let mut outra = i.identidade.clone();
        outra.chave_privada.push('x');
        assert_ne!(base, assinatura(&i.conf, &outra));
        let mut outra = i.identidade.clone();
        outra.psks.clear();
        assert_ne!(base, assinatura(&i.conf, &outra));
    }

    #[test]
    fn servidores_e_rotas_do_conf() {
        let i = importar(CONF).unwrap();
        assert_eq!(servidores(&i.conf), ["vpn.exemplo.test:51820"]);
        assert_eq!(rotas(&i.conf), ["10.8.0.0/24", "192.168.50.0/24"]);
    }

    #[test]
    fn o_estado_vira_texto_da_pagina() {
        assert_eq!(descrever_estado(None), t!("wireguard-estado-desligada"));
        assert_eq!(
            descrever_estado(Some(&State::Failed("sem rota".into()))),
            t!("wireguard-estado-falhou", motivo = "sem rota")
        );
        let conectada = State::Connected {
            address: "10.8.0.2".parse().unwrap(),
            peers_up: 1,
            peers: 2,
        };
        let texto = descrever_estado(Some(&conectada));
        assert!(texto.contains("10.8.0.2"), "{texto}");
    }

    #[test]
    fn o_debug_da_identidade_nao_mostra_segredo() {
        let i = importar(CONF).unwrap().identidade;
        let texto = format!("{i:?}");
        assert!(!texto.contains(&i.chave_privada));
        assert!(!texto.contains(&i.psks[0].1));
    }
}
