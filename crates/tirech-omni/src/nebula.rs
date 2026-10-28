//! A rede Nebula embutida (M48): o app liga o motor da lib (`tomni-ui-nebula`) só para o tráfego
//! dele, sem placa virtual e sem administrador. Este módulo tem a importação do `config.yml` (a
//! identidade vai para o cofre, o resto fica no perfil) e os [`Motores`], um por perfil, que a
//! tela liga na hora de abrir uma conexão. Daí para baixo nada muda: o motor entrega um proxy
//! SOCKS5 em `127.0.0.1`, e os clientes do app já falam com proxy (M34).

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinSet;
use tomni_ui::prelude::Signal;
use tomni_ui_nebula::{Config, Credentials, Nebula, NebulaBuilder, Severity, State, Warning};

use crate::cofre::IdentidadeNebula;
use crate::redes::{MotorDeRede, Orfao, Orfaos, PerfilRede, TravaDoMotor, TravasDeRede};
use crate::t;

/// O que sobra de uma importação: o YAML sem os segredos, a identidade que vai para o cofre e o
/// que a lib avisou (chaves ignoradas, certificado perto de vencer…).
#[derive(Clone, Debug)]
pub struct Importacao {
    /// O `config.yml` sem `pki.ca`, `pki.cert` e `pki.key`. Nada aqui é segredo.
    pub yaml: String,
    /// A CA, o certificado e a chave, com o resumo para mostrar.
    pub identidade: IdentidadeNebula,
    /// Os avisos, um por linha, prontos para a tela.
    pub avisos: Vec<String>,
}

/// O texto de um aviso da lib (`chave: mensagem`).
fn texto_do_aviso(aviso: &Warning) -> String {
    format!("{}: {}", aviso.key, aviso.message)
}

/// Importa o `config.yml` em `texto`. A `pasta` resolve os caminhos do `pki` (o arquivo original
/// não é mexido). Um erro de `Config::check` (certificado vencido, CA que não assinou, nenhum
/// lighthouse…) recusa a importação; os avisos acompanham o resultado.
///
/// # Errors
///
/// A mensagem, em português, do que impede a importação.
pub fn importar(texto: &str, pasta: Option<&Path>, agora: i64) -> Result<Importacao, String> {
    let (config, avisos) = Config::from_yaml(texto, pasta).map_err(|e| e.to_string())?;
    let mut linhas: Vec<String> = avisos.iter().map(texto_do_aviso).collect();
    let mut erros = Vec::new();
    for problema in config.check(agora) {
        match problema.severity {
            Severity::Error => erros.push(problema.message),
            Severity::Warning => linhas.push(problema.message),
        }
    }
    if !erros.is_empty() {
        return Err(erros.join("\n"));
    }
    let credenciais = config.credentials().map_err(|e| e.to_string())?;
    let (ca, certificado, chave) = match (&config.pki.ca, &config.pki.cert, &config.pki.key) {
        (Some(ca), Some(cert), Some(chave)) => (ca.clone(), cert.clone(), chave.to_string()),
        _ => return Err(t!("nebula-pki-incompleto")),
    };
    let identidade = identidade_de(ca, certificado, chave, &credenciais);
    let yaml = sem_pki(texto)?;
    Ok(Importacao {
        yaml,
        identidade,
        avisos: linhas,
    })
}

/// Refaz a identidade a partir dos três PEM (a volta de uma exportação), com o resumo do
/// certificado. Não confere a CA nem a validade: o que vence ou foi trocado ainda se guarda.
///
/// # Errors
///
/// A mensagem, em português, de por que os PEM não formam uma identidade.
pub fn identidade_de_pem(
    ca: &str,
    certificado: &str,
    chave: &str,
) -> Result<IdentidadeNebula, String> {
    if certificado.contains("PRIVATE KEY") {
        return Err(t!("nebula-arquivo-e-chave"));
    }
    let (mut config, _) = Config::from_yaml("cipher: aes\n", None).map_err(|e| e.to_string())?;
    config.set_pki(ca, certificado, chave);
    let credenciais = config.credentials().map_err(|e| e.to_string())?;
    Ok(identidade_de(
        ca.to_owned(),
        certificado.to_owned(),
        chave.to_owned(),
        &credenciais,
    ))
}

/// A identidade com o resumo do certificado (nome, endereço, grupos, validade e impressão).
fn identidade_de(
    ca: String,
    certificado: String,
    chave: String,
    credenciais: &Credentials,
) -> IdentidadeNebula {
    let principal = credenciais.default_cert();
    let vence_em = [credenciais.v1.as_ref(), credenciais.v2.as_ref()]
        .into_iter()
        .flatten()
        .map(|c| c.not_after())
        .min()
        .unwrap_or(0)
        .max(0) as u64;
    let impressao = credenciais.v2.as_ref().unwrap_or(principal).fingerprint();
    IdentidadeNebula {
        ca,
        certificado,
        chave,
        nome: principal.name().to_owned(),
        endereco: principal
            .networks()
            .first()
            .map(ToString::to_string)
            .unwrap_or_default(),
        grupos: principal.groups().to_vec(),
        vence_em,
        impressao,
    }
}

/// Troca o certificado (e, se vier, a CA) de uma identidade, mantendo a chave privada: o
/// certificado novo precisa ser da mesma chave, ter sido assinado pela CA e não estar vencido.
/// O resumo (nome, endereço, grupos, validade, impressão) é refeito.
///
/// # Errors
///
/// A mensagem, em português, de por que o certificado não serve.
pub fn substituir_certificado(
    atual: &IdentidadeNebula,
    certificado: &str,
    ca: Option<&str>,
    agora: i64,
) -> Result<IdentidadeNebula, String> {
    if certificado.contains("PRIVATE KEY") {
        return Err(t!("nebula-arquivo-e-chave"));
    }
    let ca = ca.unwrap_or(&atual.ca).to_owned();
    let (mut config, _) = Config::from_yaml("cipher: aes\n", None).map_err(|e| e.to_string())?;
    config.set_pki(&ca, certificado, &atual.chave);
    let credenciais = config.credentials().map_err(|e| e.to_string())?;
    for c in credenciais.v1.iter().chain(credenciais.v2.iter()) {
        credenciais
            .pool
            .verify(c, agora)
            .map_err(|e| e.to_string())?;
    }
    Ok(identidade_de(
        ca,
        certificado.to_owned(),
        atual.chave.clone(),
        &credenciais,
    ))
}

/// Como está a validade do certificado: sem data, em dia, perto de vencer (30 dias) ou vencido.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validade {
    /// O certificado não diz até quando vale.
    Sem,
    /// Vence daqui a mais de 30 dias.
    EmDia,
    /// Vence em menos de 30 dias.
    PertoDeVencer,
    /// Já venceu.
    Vencido,
}

/// A [`Validade`] de um certificado que vence em `vence_em` (segundos desde 1970), às `agora`.
pub fn validade(vence_em: u64, agora: u64) -> Validade {
    const TRINTA_DIAS: u64 = 30 * 24 * 3600;
    match vence_em {
        0 => Validade::Sem,
        v if v <= agora => Validade::Vencido,
        v if v - agora < TRINTA_DIAS => Validade::PertoDeVencer,
        _ => Validade::EmDia,
    }
}

/// A linha de estado da página Redes: `None` é a rede desligada.
pub fn descrever_estado(estado: Option<&State>) -> String {
    match estado {
        None | Some(State::Stopped) => t!("nebula-estado-desligada"),
        Some(State::Connecting) => t!("nebula-estado-conectando"),
        Some(State::Connected {
            address,
            lighthouses,
            tunnels,
        }) => t!(
            "nebula-estado-conectada",
            ip = address.to_string(),
            lighthouses = *lighthouses as i64,
            tuneis = *tunnels as i64
        ),
        Some(State::Retrying(motivo)) => t!("nebula-estado-tentando", motivo = motivo.as_str()),
        Some(State::Failed(motivo)) => t!("nebula-estado-falhou", motivo = motivo.as_str()),
    }
}

/// O `config.yml` sem as chaves `ca`, `cert` e `key` do bloco `pki`: só essas três linhas (e o
/// texto que continua nelas) saem, e os comentários e o resto ficam como estavam.
///
/// # Errors
///
/// Se o `pki` está em formato de linha (`pki: {…}`) ou se, depois de tirar, ainda sobra uma chave
/// privada no texto (o segredo nunca fica no perfil).
pub fn sem_pki(texto: &str) -> Result<String, String> {
    let mut saida: Vec<&str> = Vec::new();
    let mut dentro = false;
    let mut recuo_filho = None;
    // O recuo da chave que está sendo pulada e as linhas em branco que vieram depois dela (só
    // voltam se o que segue não for continuação do valor).
    let mut pulando: Option<usize> = None;
    let mut pendentes: Vec<&str> = Vec::new();
    for linha in texto.lines() {
        let util = linha.trim();
        let recuo = linha.len() - linha.trim_start().len();
        if let Some(r) = pulando {
            if util.is_empty() {
                pendentes.push(linha);
                continue;
            }
            if recuo > r {
                pendentes.clear();
                continue;
            }
            pulando = None;
            saida.append(&mut pendentes);
        }
        let comentario = util.is_empty() || util.starts_with('#');
        if !comentario && recuo == 0 {
            dentro = util
                .strip_prefix("pki:")
                .is_some_and(|resto| resto.trim().is_empty() || resto.trim().starts_with('#'));
            if util.starts_with("pki:") && !dentro {
                return Err(t!("nebula-pki-em-fluxo"));
            }
            recuo_filho = None;
        } else if dentro && !comentario {
            let filho = *recuo_filho.get_or_insert(recuo);
            let chave = util.split(':').next().unwrap_or("").trim();
            if recuo == filho && matches!(chave, "ca" | "cert" | "key") {
                pulando = Some(recuo);
                continue;
            }
        }
        saida.push(linha);
    }
    saida.append(&mut pendentes);
    let mut yaml = saida.join("\n");
    if texto.ends_with('\n') {
        yaml.push('\n');
    }
    if yaml.contains("-----BEGIN") {
        return Err(t!("nebula-restou-segredo"));
    }
    Ok(yaml)
}

/// O config pronto para ligar: o YAML do perfil com a identidade do cofre no lugar do `pki`.
///
/// # Errors
///
/// A mensagem, em português, do config ou da identidade que não valem.
pub fn config_de(yaml: &str, identidade: &IdentidadeNebula) -> Result<Config, String> {
    let (mut config, _) = Config::from_yaml(yaml, None).map_err(|e| e.to_string())?;
    config.set_pki(&identidade.ca, &identidade.certificado, &identidade.chave);
    Ok(config)
}

/// Os lighthouses do config, para a página Redes mostrar: `10.42.0.1 (127.0.0.1:14242, +1)`, com o
/// primeiro endereço e quantos mais há (o resto está no config). Vazio se o YAML não vale.
pub fn lighthouses(yaml: &str) -> Vec<String> {
    let Ok((config, _)) = Config::from_yaml(yaml, None) else {
        return Vec::new();
    };
    config
        .lighthouse
        .hosts
        .iter()
        .map(|host| match config.static_host_map.get(host) {
            Some(enderecos) if enderecos.len() > 1 => {
                format!("{host} ({}, +{})", enderecos[0], enderecos.len() - 1)
            }
            Some(enderecos) if !enderecos.is_empty() => format!("{host} ({})", enderecos[0]),
            _ => host.to_string(),
        })
        .collect()
}

/// Confere o YAML de um config editado à mão (sem o `pki`, que vem do cofre): precisa ler, e o
/// que a lib avisa volta como lista. O `check` completo só roda com a identidade, ao ligar.
///
/// # Errors
///
/// A mensagem, em português, do que não vale.
pub fn conferir_yaml(yaml: &str) -> Result<Vec<String>, String> {
    if yaml.contains("-----BEGIN") {
        return Err(t!("nebula-restou-segredo"));
    }
    let (_, avisos) = Config::from_yaml(yaml, None).map_err(|e| e.to_string())?;
    Ok(avisos.iter().map(texto_do_aviso).collect())
}

fn sem_veneno<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O motor ligado de um perfil, como a `RedeResolvida` o leva até o `Opener`.
#[derive(Clone)]
pub struct MotorNebula {
    nebula: Arc<Nebula>,
    tempo: Handle,
}

impl PartialEq for MotorNebula {
    fn eq(&self, outro: &Self) -> bool {
        Arc::ptr_eq(&self.nebula, &outro.nebula)
    }
}

impl Eq for MotorNebula {}

// A ponte (porta e credencial) não aparece em `{:?}`.
impl std::fmt::Debug for MotorNebula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotorNebula")
            .field("estado", &self.nebula.state())
            .finish_non_exhaustive()
    }
}

impl MotorNebula {
    /// O proxy SOCKS5 da ponte do motor.
    pub fn proxy(&self) -> tomni_ui::terminal::Proxy {
        self.nebula.proxy()
    }

    /// O estado agora.
    pub fn estado(&self) -> State {
        self.nebula.state()
    }

    /// Espera o motor conectar (até `espera_secs`). O `aviso` recebe as linhas que o terminal
    /// mostra. Bloqueia a thread de quem chama, que não pode ser a da tela.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que a rede não conectou.
    pub fn esperar(&self, perfil: &PerfilRede, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        if !matches!(self.nebula.state(), State::Connected { .. }) {
            aviso(&t!("nebula-conectando", nome = perfil.name.as_str()));
        } else {
            return Ok(());
        }
        let limite = Duration::from_secs(u64::from(perfil.espera_secs.max(1)));
        match self.tempo.block_on(self.nebula.wait_connected(limite)) {
            Ok(()) => {
                let ip = self
                    .nebula
                    .state()
                    .address()
                    .map(|a| a.to_string())
                    .unwrap_or_default();
                aviso(&t!("nebula-pronta", nome = perfil.name.as_str(), ip = ip));
                Ok(())
            }
            Err(erro) => Err(t!(
                "nebula-nao-conectou",
                nome = perfil.name.as_str(),
                motivo = erro.to_string()
            )),
        }
    }
}

/// Um motor ligado e com o que o fez subir (para religar se o config ou a identidade mudou).
struct Ligado {
    nebula: Arc<Nebula>,
    assinatura: u64,
    /// Falso depois do [`Motores::parar`]: os últimos estados do motor velho não valem mais.
    vivo: Arc<AtomicBool>,
    /// A trava do perfil entre as cópias do app ([`TravasDeRede`]): solta quando o motor para.
    trava: Arc<TravaDoMotor>,
}

impl MotorDeRede for Nebula {
    fn fechar(&self) {
        self.close();
    }

    fn acabou(&self) -> bool {
        matches!(self.state(), State::Stopped | State::Failed(_))
    }
}

/// Os motores Nebula ligados, um por perfil, sobre um runtime próprio do `tokio` (o app não tem
/// outro). Ligam sob demanda e param quando o último uso do perfil fecha (veja `Usos`) ou quando
/// o app fecha.
#[derive(Default)]
pub struct Motores {
    tempo: Mutex<Option<Arc<Runtime>>>,
    ligados: Mutex<HashMap<u64, Ligado>>,
    /// O último estado de cada perfil, para a página Redes.
    estados: Arc<Mutex<HashMap<u64, State>>>,
    /// O contador que acorda a tela quando um estado muda.
    acordar: Arc<Mutex<Option<Signal<u64>>>>,
    /// Uma cópia do app por vez liga cada perfil.
    travas: TravasDeRede,
    /// Os motores parados que conexões ainda seguram: um motor por perfil, nunca dois.
    orfaos: Orfaos<Nebula>,
}

impl Motores {
    /// Avisa a `tela` (por este contador) cada vez que o estado de um motor muda.
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
            .thread_name("nebula")
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let novo = Arc::new(novo);
        *tempo = Some(Arc::clone(&novo));
        Ok(novo)
    }

    /// Liga o motor do perfil, se ainda não está ligado com este config e esta identidade, e
    /// devolve o acesso à ponte. Não espera a conexão (isso é o [`MotorNebula::esperar`]).
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que o motor não subiu. O estado do perfil fica
    /// `Failed`.
    pub fn garantir(
        &self,
        perfil: &PerfilRede,
        identidade: &IdentidadeNebula,
        agora: i64,
    ) -> Result<MotorNebula, String> {
        let Some(nebula) = perfil.nebula.as_ref() else {
            return Err(t!("nebula-sem-config", nome = perfil.name.as_str()));
        };
        let tempo = self.runtime()?;
        let assinatura = assinatura(&nebula.config, identidade);
        {
            let ligados = sem_veneno(&self.ligados);
            if let Some(l) = ligados.get(&perfil.id)
                && l.assinatura == assinatura
            {
                return Ok(MotorNebula {
                    nebula: Arc::clone(&l.nebula),
                    tempo: tempo.handle().clone(),
                });
            }
        }
        // O config ou a identidade mudou desde que subiu: o motor velho para.
        self.parar(perfil.id);
        let falhou = |motivo: String| {
            let motivo = t!(
                "nebula-nao-subiu",
                nome = perfil.name.as_str(),
                motivo = motivo
            );
            self.guardar_estado(perfil.id, State::Failed(motivo.clone()));
            motivo
        };
        if nebula.config.trim().is_empty() {
            return Err(falhou(t!("nebula-config-vazio")));
        }
        let config = config_de(&nebula.config, identidade).map_err(&falhou)?;
        let erros: Vec<String> = config
            .check(agora)
            .into_iter()
            .filter(|p| p.severity == Severity::Error)
            .map(|p| p.message)
            .collect();
        if !erros.is_empty() {
            return Err(falhou(erros.join("; ")));
        }
        if let Some(orfao) = self.orfaos.retomar(perfil.id, assinatura) {
            // O motor parado ainda roda (uma conexão o segura) e com a mesma identidade: volta
            // a ser o do perfil. Ligar outro derrubaria este no servidor.
            let nebula = Arc::clone(&orfao.motor);
            sem_veneno(&self.ligados).insert(
                perfil.id,
                Ligado {
                    nebula: orfao.motor,
                    assinatura,
                    vivo: orfao.vivo,
                    trava: orfao.trava,
                },
            );
            self.guardar_estado(perfil.id, nebula.state());
            return Ok(MotorNebula {
                nebula,
                tempo: tempo.handle().clone(),
            });
        }
        let trava = TravaDoMotor::new(self.travas.travar(perfil).map_err(&falhou)?);
        let trava_do_motor = Arc::clone(&trava);
        let vivo = Arc::new(AtomicBool::new(true));
        let (estados, acordar, id, vivo_do_motor) = (
            Arc::clone(&self.estados),
            Arc::clone(&self.acordar),
            perfil.id,
            Arc::clone(&vivo),
        );
        let ligado = tempo
            .block_on(
                NebulaBuilder::new(config)
                    .on_state(move |estado| {
                        if matches!(estado, State::Stopped | State::Failed(_)) {
                            trava_do_motor.soltar();
                        }
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
                nebula: Arc::clone(&ligado),
                assinatura,
                vivo,
                trava,
            },
        );
        self.guardar_estado(perfil.id, ligado.state());
        Ok(MotorNebula {
            nebula: ligado,
            tempo: tempo.handle().clone(),
        })
    }

    fn guardar_estado(&self, perfil: u64, estado: State) {
        sem_veneno(&self.estados).insert(perfil, estado);
        acordar_a_tela(&self.acordar);
    }

    /// Para o motor do perfil (se está ligado), sem esperar: os `CloseTunnel` saem em segundo
    /// plano.
    pub fn parar(&self, perfil: u64) {
        let Some(ligado) = sem_veneno(&self.ligados).remove(&perfil) else {
            return;
        };
        ligado.vivo.store(false, Ordering::Relaxed);
        sem_veneno(&self.estados).remove(&perfil);
        acordar_a_tela(&self.acordar);
        match Arc::try_unwrap(ligado.nebula) {
            Ok(nebula) => {
                ligado.trava.soltar();
                if let Ok(tempo) = self.runtime() {
                    tempo.spawn(async move { nebula.stop().await });
                }
            }
            // Uma conexão ainda segura o motor: ele segue até ela largar, e o próximo `garantir`
            // o reaproveita ou fecha.
            Err(nebula) => self.orfaos.guardar(
                perfil,
                Orfao {
                    motor: nebula,
                    assinatura: ligado.assinatura,
                    vivo: ligado.vivo,
                    trava: ligado.trava,
                },
            ),
        }
    }

    /// Para todos, esperando no máximo 200 ms: é o tempo de os `CloseTunnel` saírem antes de o
    /// app fechar.
    pub fn parar_todos(&self) {
        self.orfaos.fechar_todos();
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
                tarefas.spawn(parar_um(l.nebula));
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

    /// Quantos túneis o motor do perfil tem abertos agora (os lighthouses e os pares), com os
    /// endereços e nomes. Vazio se está desligado.
    pub fn tuneis(&self, perfil: u64) -> Vec<tomni_ui_nebula::TunnelInfo> {
        let (nebula, tempo) = {
            let ligados = sem_veneno(&self.ligados);
            let Some(l) = ligados.get(&perfil) else {
                return Vec::new();
            };
            (Arc::clone(&l.nebula), self.runtime())
        };
        match tempo {
            Ok(tempo) => tempo.block_on(nebula.tunnels()),
            Err(_) => Vec::new(),
        }
    }
}

/// Para um motor: com a única referência, o `stop` avisa os pares; se ainda há quem
/// segure o `Arc` (uma conexão em andamento), o `close` para do mesmo jeito: o app vai fechar.
async fn parar_um(nebula: Arc<Nebula>) {
    match Arc::try_unwrap(nebula) {
        Ok(nebula) => nebula.stop().await,
        Err(restante) => restante.close(),
    }
}

fn acordar_a_tela(acordar: &Mutex<Option<Signal<u64>>>) {
    if let Some(sinal) = *sem_veneno(acordar) {
        crate::agente::acordar(sinal);
    }
}

/// O que identifica "este motor": o config e a identidade. Mudou, o motor religa.
fn assinatura(yaml: &str, identidade: &IdentidadeNebula) -> u64 {
    let mut h = DefaultHasher::new();
    yaml.hash(&mut h);
    identidade.ca.hash(&mut h);
    identidade.certificado.hash(&mut h);
    identidade.chave.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGORA: i64 = 1_800_000_000;

    #[test]
    fn tira_so_as_tres_chaves_do_pki_e_mantem_o_resto() {
        let texto = "\
# Config do nó
pki:
  # a CA da rede
  ca: ca.crt
  cert: |
    -----BEGIN NEBULA CERTIFICATE V2-----
    AAAA
    -----END NEBULA CERTIFICATE V2-----

  key: |
    -----BEGIN NEBULA X25519 PRIVATE KEY-----
    BBBB
    -----END NEBULA X25519 PRIVATE KEY-----
  disconnect_invalid: true

lighthouse:
  hosts:
    - \"10.42.0.1\"
";
        let saida = sem_pki(texto).expect("separa");
        assert!(!saida.contains("-----BEGIN"), "{saida}");
        assert!(!saida.contains("ca.crt") && !saida.contains("AAAA") && !saida.contains("BBBB"));
        assert!(saida.contains("# Config do nó"));
        assert!(saida.contains("# a CA da rede"));
        assert!(saida.contains("disconnect_invalid: true"));
        assert!(saida.contains("lighthouse:\n  hosts:\n    - \"10.42.0.1\""));
        assert!(saida.ends_with('\n'));
    }

    #[test]
    fn o_pki_em_formato_de_linha_e_recusado() {
        let erro = sem_pki("pki: {ca: ca.crt, cert: no.crt, key: no.key}\n").unwrap_err();
        assert_eq!(erro, t!("nebula-pki-em-fluxo"));
    }

    #[test]
    fn so_chaves_do_pki_saem_e_nao_as_de_outros_blocos() {
        let texto = "pki:\n  ca: a\nlighthouse:\n  key: fica\n  ca: fica-tambem\n";
        let saida = sem_pki(texto).unwrap();
        assert_eq!(saida, "pki:\nlighthouse:\n  key: fica\n  ca: fica-tambem\n");
    }

    #[test]
    fn config_de_teste_de_chaves_embutidas_nao_deixa_segredo() {
        let texto = "\
pki:
  ca: |
    -----BEGIN NEBULA CERTIFICATE-----
    x
    -----END NEBULA CERTIFICATE-----
  cert: c
  key: k
";
        assert!(!sem_pki(texto).unwrap().contains("-----BEGIN"));
    }

    #[test]
    fn config_sem_pki_nem_lighthouse_e_recusada_na_importacao() {
        let erro = importar("lighthouse:\n  hosts: []\n", None, AGORA).unwrap_err();
        assert!(!erro.is_empty());
    }

    #[test]
    fn conferir_yaml_recusa_chave_privada_e_yaml_quebrado() {
        assert!(conferir_yaml("pki:\n  key: |\n    -----BEGIN X-----\n").is_err());
        assert!(conferir_yaml("a: [\n").is_err());
        assert!(conferir_yaml("cipher: aes\n").is_ok());
    }

    const CA: &str = include_str!("../tests/dados/nebula/ca-25519.crt");
    const CERT: &str = include_str!("../tests/dados/nebula/no-v2.crt");
    const CHAVE: &str = include_str!("../tests/dados/nebula/no-v2.key");
    const CERT_OUTRO: &str = include_str!("../tests/dados/nebula/lighthouse.crt");
    const CONFIG: &str = include_str!("../tests/dados/nebula/config-no.yml");

    fn identidade() -> IdentidadeNebula {
        identidade_de_pem(CA, CERT, CHAVE).expect("a fixture vale")
    }

    #[test]
    fn a_identidade_traz_o_resumo_do_certificado() {
        let i = identidade();
        assert!(!i.nome.is_empty() && !i.impressao.is_empty());
        assert!(i.endereco.contains('/'), "{}", i.endereco);
        assert!(i.vence_em > AGORA as u64);
        assert_eq!((i.ca.as_str(), i.chave.as_str()), (CA, CHAVE));
    }

    #[test]
    fn identidade_de_pem_recusa_chave_no_lugar_do_certificado() {
        assert_eq!(
            identidade_de_pem(CA, CHAVE, CHAVE).unwrap_err(),
            t!("nebula-arquivo-e-chave")
        );
        assert!(identidade_de_pem(CA, "lixo", CHAVE).is_err());
    }

    #[test]
    fn importar_separa_o_config_da_identidade() {
        let pasta = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dados/nebula");
        let importado = importar(CONFIG, Some(&pasta), AGORA).expect("importa");
        assert!(!importado.yaml.contains("-----BEGIN"));
        assert!(!importado.yaml.contains("no-v2.key"));
        assert!(importado.yaml.contains("static_host_map"));
        assert_eq!(importado.identidade.certificado, CERT);
        assert_eq!(importado.identidade.chave, CHAVE);
        assert!(conferir_yaml(&importado.yaml).is_ok());
        assert_eq!(
            lighthouses(&importado.yaml),
            ["10.42.0.1 (127.0.0.1:14242, +1)"]
        );
    }

    #[test]
    fn importar_sem_os_arquivos_do_pki_diz_o_que_falta() {
        let erro = importar(CONFIG, None, AGORA).unwrap_err();
        assert!(!erro.is_empty());
    }

    #[test]
    fn substituir_o_certificado_mantem_a_chave_e_refaz_o_resumo() {
        let atual = identidade();
        let nova = substituir_certificado(&atual, CERT, None, AGORA).expect("o mesmo serve");
        assert_eq!(
            (nova.chave.as_str(), nova.impressao.as_str()),
            (CHAVE, atual.impressao.as_str())
        );
        // Um certificado de outra chave não casa com a chave guardada.
        assert!(substituir_certificado(&atual, CERT_OUTRO, None, AGORA).is_err());
        assert_eq!(
            substituir_certificado(&atual, CHAVE, None, AGORA).unwrap_err(),
            t!("nebula-arquivo-e-chave")
        );
    }

    #[test]
    fn a_validade_muda_de_cor_a_30_dias_e_ao_vencer() {
        const DIA: u64 = 24 * 3600;
        let agora = 1_000_000_000;
        assert_eq!(validade(0, agora), Validade::Sem);
        assert_eq!(validade(agora + 31 * DIA, agora), Validade::EmDia);
        assert_eq!(validade(agora + 29 * DIA, agora), Validade::PertoDeVencer);
        assert_eq!(validade(agora, agora), Validade::Vencido);
        assert_eq!(validade(agora - 1, agora), Validade::Vencido);
    }

    #[test]
    fn o_estado_vira_texto_da_pagina() {
        assert_eq!(descrever_estado(None), t!("nebula-estado-desligada"));
        assert_eq!(
            descrever_estado(Some(&State::Stopped)),
            t!("nebula-estado-desligada")
        );
        assert_eq!(
            descrever_estado(Some(&State::Connecting)),
            t!("nebula-estado-conectando")
        );
        assert_eq!(
            descrever_estado(Some(&State::Failed("sem rota".into()))),
            t!("nebula-estado-falhou", motivo = "sem rota")
        );
        assert_eq!(
            descrever_estado(Some(&State::Retrying("de novo".into()))),
            t!("nebula-estado-tentando", motivo = "de novo")
        );
    }

    #[test]
    fn o_debug_da_identidade_nao_mostra_a_chave() {
        let texto = format!("{:?}", identidade());
        assert!(texto.contains("***"));
        assert!(!texto.contains("BEGIN") && !texto.contains(CHAVE.lines().nth(1).unwrap()));
    }
}
