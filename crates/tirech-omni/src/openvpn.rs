//! O OpenVPN embutido (M56): o motor da lib `tomni-ui-openvpn` roda dentro do app, só para o
//! tráfego dele, sem placa virtual nem privilégio. Este módulo importa o `.ovpn`, separa os
//! segredos (que vão para o cofre) do texto do perfil (que não tem nenhum), monta o `Config` na
//! hora de ligar e guarda os motores ligados, um por perfil de rede.
//!
//! É o par de `nebula.rs`: a rede entrega ao resto do app um proxy SOCKS5 de `127.0.0.1` (a
//! ponte do motor), que a tela põe no `perfil.proxy` da `RedeResolvida`. A ponte nunca aparece
//! para quem usa o app.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use tomni_ui::prelude::Signal;
use tomni_ui_openvpn::config::{AuthUserPass, Config, Proto, Remote};
use tomni_ui_openvpn::pki::{
    KeyFormat, Pki, PrivateKey, Severity, Wrap, parse_certificates, parse_tls_crypt_v2_client,
};
use tomni_ui_openvpn::static_key::StaticKey;
use tomni_ui_openvpn::{OpenVpn, OpenVpnBuilder, Prompt, PromptKind, State};
use zeroize::Zeroizing;

use crate::cofre::IdentidadeOpenVpn;
use crate::redes::{MotorDeRede, Orfao, Orfaos, PerfilRede, TravaDoMotor, TravasDeRede};
use crate::t;

/// Os modos de proteção do canal de controle que a identidade guarda em `modo_tls`.
const MODO_TLS_AUTH: &str = "tls-auth";
const MODO_TLS_CRYPT: &str = "tls-crypt";
const MODO_TLS_CRYPT_V2: &str = "tls-crypt-v2";

/// O que sobra de uma importação: o `.ovpn` sem os segredos, a identidade que vai para o cofre, o
/// usuário e a senha que o arquivo trazia e o que a lib avisou.
#[derive(Clone, Debug)]
pub struct Importacao {
    /// O `.ovpn` sem a CA, o certificado, a chave, as chaves TLS e o conteúdo do
    /// `auth-user-pass`. Nada aqui é segredo.
    pub ovpn: String,
    /// A CA, o certificado, a chave e a chave TLS, com o resumo para mostrar.
    pub identidade: IdentidadeOpenVpn,
    /// O usuário do `auth-user-pass arquivo` (vazio se o arquivo não trazia).
    pub usuario: String,
    /// A senha do `auth-user-pass arquivo` (vazia se não trazia).
    pub senha: String,
    /// Se o `.ovpn` tem `auth-user-pass`: sem usuário, a tela pergunta antes de salvar.
    pub pede_credenciais: bool,
    /// Os avisos da conferência (certificado perto de vencer, o servidor não é conferido…).
    pub avisos: Vec<String>,
    /// As opções que o app leu e ignorou (scripts, plugins, `crl-verify`…), uma por linha.
    pub ignoradas: Vec<String>,
}

/// O `.ovpn` lido e conferido: o `Config` com o PKI, os avisos da lib e os da conferência.
struct Lido {
    config: Config,
    ignoradas: Vec<String>,
    avisos: Vec<String>,
}

/// Lê o `.ovpn` e confere. Um erro da conferência (CA que não assinou, certificado vencido, sem
/// `remote`…) recusa; os avisos acompanham.
fn ler(texto: &str, pasta: Option<&Path>, agora: SystemTime) -> Result<Lido, String> {
    let (config, ignoradas) = Config::from_ovpn(texto, pasta).map_err(|e| e.to_string())?;
    let mut avisos = Vec::new();
    let mut erros = Vec::new();
    for problema in config.check(agora) {
        match problema.severity {
            Severity::Error => erros.push(problema.message),
            Severity::Warning => avisos.push(problema.message),
        }
    }
    if !erros.is_empty() {
        return Err(erros.join("\n"));
    }
    Ok(Lido {
        config,
        ignoradas: ignoradas.iter().map(ToString::to_string).collect(),
        avisos,
    })
}

/// Importa o `.ovpn` em `texto`. A `pasta` resolve os arquivos citados (`ca ca.crt`…), e só vale
/// o que está dentro dela; o arquivo original não é mexido.
///
/// # Errors
///
/// A mensagem, em português, do que impede a importação.
pub fn importar(
    texto: &str,
    pasta: Option<&Path>,
    agora: SystemTime,
) -> Result<Importacao, String> {
    let lido = ler(texto, pasta, agora)?;
    let config = &lido.config;
    let (usuario, senha, pede_credenciais) = match &config.auth_user_pass {
        AuthUserPass::None => (String::new(), String::new(), false),
        AuthUserPass::Ask => (String::new(), String::new(), true),
        AuthUserPass::Credentials { user, password } => (
            user.clone(),
            password.as_ref().map(|p| p.to_string()).unwrap_or_default(),
            true,
        ),
    };
    let identidade = identidade_de_config(config)?;
    let ovpn = Config::without_secrets(texto);
    // O que sobrou precisa ler sozinho e sem nenhum segredo.
    conferir_ovpn(&ovpn)?;
    Ok(Importacao {
        ovpn,
        identidade,
        usuario,
        senha,
        pede_credenciais,
        avisos: lido.avisos,
        ignoradas: lido.ignoradas,
    })
}

/// Confere o texto de um `.ovpn` que vai para o perfil (importado ou editado à mão): precisa ler
/// e não pode trazer segredo nenhum. Devolve as opções ignoradas, uma por linha.
///
/// # Errors
///
/// A mensagem, em português, de por que o texto não serve.
pub fn conferir_ovpn(texto: &str) -> Result<Vec<String>, String> {
    let (config, ignoradas) = Config::from_ovpn(texto, None).map_err(|e| e.to_string())?;
    let pki = &config.pki;
    let com_segredo = !pki.ca.is_empty()
        || pki.cert.is_some()
        || pki.key.is_some()
        || pki.static_key.is_some()
        || pki.wrap != Wrap::None
        || matches!(config.auth_user_pass, AuthUserPass::Credentials { .. });
    if com_segredo {
        return Err(t!("openvpn-config-com-segredo"));
    }
    Ok(ignoradas.iter().map(ToString::to_string).collect())
}

/// O PEM de `der` com o `rotulo` (`CERTIFICATE`, `PRIVATE KEY`…), em linhas de 64 colunas.
fn pem(rotulo: &str, der: &[u8]) -> Zeroizing<String> {
    let base64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut saida = Zeroizing::new(format!("-----BEGIN {rotulo}-----\n"));
    for linha in base64.as_bytes().chunks(64) {
        saida.push_str(&String::from_utf8_lossy(linha));
        saida.push('\n');
    }
    saida.push_str(&format!("-----END {rotulo}-----\n"));
    saida
}

fn pem_dos_certificados(certificados: &[impl AsRef<[u8]>]) -> String {
    certificados
        .iter()
        .map(|c| pem("CERTIFICATE", c.as_ref()).to_string())
        .collect()
}

fn pem_da_chave(chave: &PrivateKey) -> Zeroizing<String> {
    let rotulo = match chave.format() {
        KeyFormat::Pkcs1 => "RSA PRIVATE KEY",
        KeyFormat::Pkcs8 => "PRIVATE KEY",
        KeyFormat::Sec1 => "EC PRIVATE KEY",
    };
    pem(rotulo, chave.as_rustls().secret_der())
}

/// O arquivo de chave de cliente do `tls-crypt-v2`: `Kc` e `WKc` em base64 entre as marcas.
fn pem_tls_crypt_v2(kc: &[u8], wkc: &[u8]) -> Zeroizing<String> {
    let mut bytes = Zeroizing::new(Vec::with_capacity(kc.len() + wkc.len()));
    bytes.extend_from_slice(kc);
    bytes.extend_from_slice(wkc);
    pem("OpenVPN tls-crypt-v2 client key", &bytes)
}

/// O primeiro `remote` como `host:porta/proto`, para mostrar.
fn servidor_de(remotes: &[Remote]) -> String {
    remotes.first().map_or_else(String::new, |r| {
        let proto = match r.proto {
            Proto::Udp => "udp",
            Proto::Tcp => "tcp",
        };
        format!("{}:{}/{proto}", r.host, r.port)
    })
}

/// A identidade com o PEM do `Pki` do `Config` e o resumo do certificado (nome, validade,
/// impressão). O `Config` guarda os segredos já em DER, então o PEM é refeito aqui.
fn identidade_de_config(config: &Config) -> Result<IdentidadeOpenVpn, String> {
    let pki = &config.pki;
    let (modo_tls, chave_tls) = match &pki.wrap {
        Wrap::None => (String::new(), String::new()),
        Wrap::TlsAuth | Wrap::TlsCrypt => {
            let chave = pki
                .static_key
                .as_ref()
                .ok_or_else(|| t!("openvpn-sem-chave-tls"))?;
            let modo = if pki.wrap == Wrap::TlsAuth {
                MODO_TLS_AUTH
            } else {
                MODO_TLS_CRYPT
            };
            (modo.to_owned(), chave.to_pem().to_string())
        }
        Wrap::TlsCryptV2 { kc, wkc } => (
            MODO_TLS_CRYPT_V2.to_owned(),
            pem_tls_crypt_v2(kc.as_slice(), wkc).to_string(),
        ),
    };
    let mut identidade = IdentidadeOpenVpn {
        ca: pem_dos_certificados(&pki.ca),
        certificado: pki
            .cert
            .as_deref()
            .map(pem_dos_certificados)
            .unwrap_or_default(),
        chave: pki
            .key
            .as_ref()
            .map(|c| pem_da_chave(c).to_string())
            .unwrap_or_default(),
        chave_tls,
        modo_tls,
        key_direction: pki.key_direction,
        servidor: servidor_de(&config.remotes),
        ..IdentidadeOpenVpn::default()
    };
    resumir(&mut identidade, pki);
    Ok(identidade)
}

/// Preenche o nome, a validade e a impressão do certificado do cliente.
fn resumir(identidade: &mut IdentidadeOpenVpn, pki: &Pki) {
    let Some(resumo) = pki.summary() else {
        identidade.cn.clear();
        identidade.vence_em = 0;
        identidade.impressao.clear();
        return;
    };
    identidade.cn = resumo.client_common_name.unwrap_or_default();
    identidade.vence_em = resumo
        .not_after
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    identidade.impressao = resumo.fingerprint_sha256;
}

/// O `Pki` de uma identidade do cofre.
///
/// # Errors
///
/// A mensagem, em português, de por que o PEM guardado não lê.
pub fn pki_de(identidade: &IdentidadeOpenVpn) -> Result<Pki, String> {
    let mut pki = Pki {
        ca: parse_certificates(&identidade.ca).map_err(|e| e.to_string())?,
        key_direction: identidade.key_direction,
        ..Pki::default()
    };
    if !identidade.certificado.trim().is_empty() {
        pki.cert = Some(parse_certificates(&identidade.certificado).map_err(|e| e.to_string())?);
    }
    if !identidade.chave.trim().is_empty() {
        pki.key = Some(PrivateKey::from_pem(&identidade.chave).map_err(|e| e.to_string())?);
    }
    match identidade.modo_tls.as_str() {
        "" => {}
        MODO_TLS_AUTH | MODO_TLS_CRYPT => {
            pki.static_key =
                Some(StaticKey::from_pem(&identidade.chave_tls).map_err(|e| e.to_string())?);
            pki.wrap = if identidade.modo_tls == MODO_TLS_AUTH {
                Wrap::TlsAuth
            } else {
                Wrap::TlsCrypt
            };
        }
        MODO_TLS_CRYPT_V2 => {
            pki.wrap =
                parse_tls_crypt_v2_client(&identidade.chave_tls).map_err(|e| e.to_string())?;
        }
        outro => return Err(t!("openvpn-modo-tls-desconhecido", modo = outro)),
    }
    Ok(pki)
}

/// Refaz o resumo de uma identidade vinda de fora (a volta de uma exportação ou de um Bitwarden):
/// confere que os PEM leem e preenche nome, validade e impressão. Não confere a validade: o que
/// venceu ainda se guarda.
///
/// # Errors
///
/// A mensagem, em português, de por que os PEM não formam uma identidade.
pub fn completar(identidade: &mut IdentidadeOpenVpn) -> Result<(), String> {
    if identidade.certificado.contains("PRIVATE KEY") {
        return Err(t!("openvpn-arquivo-e-chave"));
    }
    let pki = pki_de(identidade)?;
    resumir(identidade, &pki);
    Ok(())
}

/// Troca o certificado (e, se vier, a chave) de uma identidade, mantendo a CA e a chave TLS: o
/// certificado novo precisa ser da chave, ter sido assinado pela CA e não estar vencido. É a
/// renovação comum. O resumo (nome, validade, impressão) é refeito.
///
/// # Errors
///
/// A mensagem, em português, de por que o certificado não serve.
pub fn substituir_certificado(
    atual: &IdentidadeOpenVpn,
    certificado: &str,
    chave: Option<&str>,
    agora: SystemTime,
) -> Result<IdentidadeOpenVpn, String> {
    if certificado.contains("PRIVATE KEY") {
        return Err(t!("openvpn-arquivo-e-chave"));
    }
    let mut nova = atual.clone();
    nova.certificado = certificado.trim().to_owned() + "\n";
    if let Some(chave) = chave {
        nova.chave = chave.trim().to_owned() + "\n";
    }
    let pki = pki_de(&nova)?;
    let erros: Vec<String> = pki
        .check(agora)
        .into_iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message)
        .collect();
    if !erros.is_empty() {
        return Err(erros.join("\n"));
    }
    resumir(&mut nova, &pki);
    Ok(nova)
}

/// Como está a validade do certificado do cliente.
pub use crate::nebula::{Validade, validade};

/// O `Config` pronto para ligar: o `.ovpn` do perfil (sem segredos), o PKI da identidade e as
/// credenciais do `auth-user-pass`; `ignorar_rotas` liga o `route-nopull`. Sem usuário, um `.ovpn` que pede senha é recusado com o que
/// fazer; com o usuário e sem a senha, o motor pergunta a senha ao ligar. O usuário do item vai
/// mesmo quando o `.ovpn` não tem `auth-user-pass`.
///
/// # Errors
///
/// A mensagem, em português, de por que o perfil não pode ligar.
pub fn config_de(
    ovpn: &str,
    identidade: &IdentidadeOpenVpn,
    usuario: &str,
    senha: &str,
    ignorar_rotas: bool,
    agora: SystemTime,
) -> Result<Config, String> {
    let (mut config, _) = Config::from_ovpn(ovpn, None).map_err(|e| e.to_string())?;
    config.set_pki(pki_de(identidade)?);
    config.route_nopull |= ignorar_rotas;
    if config.auth_user_pass != AuthUserPass::None && usuario.trim().is_empty() {
        return Err(t!("openvpn-sem-usuario"));
    }
    // Com o usuário no item, as credenciais vão mesmo que o `.ovpn` não tenha `auth-user-pass`
    // (a linha comentada, por exemplo): o servidor que não pede senha as ignora.
    if !usuario.trim().is_empty() {
        config.auth_user_pass = AuthUserPass::Credentials {
            user: usuario.to_owned(),
            password: (!senha.is_empty()).then(|| Zeroizing::new(senha.to_owned())),
        };
    }
    let erros: Vec<String> = config
        .check(agora)
        .into_iter()
        .filter(|p| p.severity == Severity::Error)
        .map(|p| p.message)
        .collect();
    if !erros.is_empty() {
        return Err(erros.join("; "));
    }
    Ok(config)
}

/// Os servidores (`remote`) do `.ovpn`, como `host:porta/proto`, para a página Redes.
pub fn servidores(ovpn: &str) -> Vec<String> {
    Config::from_ovpn(ovpn, None)
        .map(|(c, _)| {
            c.remotes
                .iter()
                .map(|r| servidor_de(std::slice::from_ref(r)))
                .collect()
        })
        .unwrap_or_default()
}

/// As rotas que o `.ovpn` fixa (`route`, `route-nopull`) e o que o servidor empurra não entra: só
/// o que o arquivo diz. Cada rota vai numa linha.
pub fn rotas(ovpn: &str) -> Vec<String> {
    ovpn.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("route ") || *l == "route-nopull")
        .map(str::to_owned)
        .collect()
}

/// A linha de estado da página Redes: `None` é a rede desligada.
pub fn descrever_estado(estado: Option<&State>) -> String {
    match estado {
        None | Some(State::Stopped) => t!("openvpn-estado-desligada"),
        Some(State::Connecting { remote }) => {
            t!("openvpn-estado-conectando", servidor = remote.as_str())
        }
        Some(State::Asking(pergunta)) => {
            t!("openvpn-estado-perguntando", texto = pergunta.text.as_str())
        }
        Some(State::Waiting { message, .. }) => {
            t!("openvpn-estado-aguardando", mensagem = message.as_str())
        }
        Some(State::Connected {
            address,
            remote,
            cipher,
            ..
        }) => t!(
            "openvpn-estado-conectada",
            ip = address.to_string(),
            servidor = remote.as_str(),
            cifra = cipher.as_str()
        ),
        Some(State::Retrying(motivo)) => t!("openvpn-estado-tentando", motivo = motivo.as_str()),
        Some(State::Failed(motivo)) => t!("openvpn-estado-falhou", motivo = motivo.as_str()),
    }
}

/// Os avisos de um estado conectado (opções do servidor que o app ignorou…), para a página.
pub fn avisos_do_estado(estado: Option<&State>) -> Vec<String> {
    match estado {
        Some(State::Connected { warnings, .. }) => warnings.clone(),
        _ => Vec::new(),
    }
}

fn sem_veneno<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O motor ligado de um perfil, como a `RedeResolvida` o leva até o `Opener`.
#[derive(Clone)]
pub struct MotorOpenVpn {
    vpn: Arc<OpenVpn>,
}

impl PartialEq for MotorOpenVpn {
    fn eq(&self, outro: &Self) -> bool {
        Arc::ptr_eq(&self.vpn, &outro.vpn)
    }
}

impl Eq for MotorOpenVpn {}

// A ponte (porta e credencial) não aparece em `{:?}`.
impl std::fmt::Debug for MotorOpenVpn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MotorOpenVpn")
            .field("estado", &self.vpn.state())
            .finish_non_exhaustive()
    }
}

impl MotorOpenVpn {
    /// O proxy SOCKS5 da ponte do motor.
    pub fn proxy(&self) -> tomni_ui::terminal::Proxy {
        self.vpn.proxy()
    }

    /// O estado agora.
    pub fn estado(&self) -> State {
        self.vpn.state()
    }

    /// Espera o motor conectar (até `espera_secs`; o tempo que a pessoa leva para responder às
    /// perguntas não conta). O `aviso` recebe as linhas que o terminal mostra. Bloqueia a thread
    /// de quem chama, que não pode ser a da tela.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que a rede não conectou.
    pub fn esperar(&self, perfil: &PerfilRede, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        if matches!(self.vpn.state(), State::Connected { .. }) {
            return Ok(());
        }
        aviso(&t!("openvpn-conectando", nome = perfil.name.as_str()));
        let limite = Duration::from_secs(u64::from(perfil.espera_secs.max(1)));
        match self.vpn.wait_connected(limite) {
            Ok(ip) => {
                aviso(&t!(
                    "openvpn-pronta",
                    nome = perfil.name.as_str(),
                    ip = ip.to_string()
                ));
                Ok(())
            }
            Err(motivo) => Err(t!(
                "openvpn-nao-conectou",
                nome = perfil.name.as_str(),
                motivo = motivo
            )),
        }
    }
}

/// Um motor ligado e com o que o fez subir (para religar se o perfil ou a identidade mudou).
struct Ligado {
    vpn: Arc<OpenVpn>,
    assinatura: u64,
    /// Falso depois do [`MotoresVpn::parar`]: os últimos estados do motor velho não valem mais.
    vivo: Arc<AtomicBool>,
    /// A trava do perfil entre as cópias do app ([`TravasDeRede`]): solta quando o motor para.
    trava: Arc<TravaDoMotor>,
}

impl MotorDeRede for OpenVpn {
    fn fechar(&self) {
        self.close();
    }

    fn acabou(&self) -> bool {
        matches!(self.state(), State::Stopped | State::Failed(_))
    }
}

/// Os motores OpenVPN ligados, um por perfil. Cada motor tem a thread própria da lib. Ligam sob
/// demanda e param quando o último uso do perfil fecha (veja `Usos`) ou quando o app fecha.
#[derive(Default)]
pub struct MotoresVpn {
    ligados: Mutex<HashMap<u64, Ligado>>,
    /// O último estado de cada perfil, para a página Redes e para as perguntas.
    estados: Arc<Mutex<HashMap<u64, State>>>,
    /// O contador que acorda a tela quando um estado muda.
    acordar: Arc<Mutex<Option<Signal<u64>>>>,
    /// Uma cópia do app por vez liga cada perfil.
    travas: TravasDeRede,
    /// Os motores parados que conexões ainda seguram: um motor por perfil, nunca dois.
    orfaos: Orfaos<OpenVpn>,
}

impl MotoresVpn {
    /// Avisa a tela (por este contador) cada vez que o estado de um motor muda.
    pub fn ao_mudar(&self, sinal: Signal<u64>) {
        *sem_veneno(&self.acordar) = Some(sinal);
    }

    /// A pasta de configuração, onde fica a trava de cada perfil entre as cópias do app.
    pub fn na_pasta(&self, pasta: &Path) {
        self.travas.na_pasta(pasta);
    }

    /// Liga o motor do perfil, se ainda não está ligado com esta `assinatura`, e devolve o acesso
    /// à ponte. Não espera a conexão (isso é o [`MotorOpenVpn::esperar`]).
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que o motor não subiu. O estado do perfil fica
    /// `Failed`.
    pub fn garantir(
        &self,
        perfil: &PerfilRede,
        config: Result<Config, String>,
        assinatura: u64,
    ) -> Result<MotorOpenVpn, String> {
        {
            let ligados = sem_veneno(&self.ligados);
            if let Some(l) = ligados.get(&perfil.id)
                && l.assinatura == assinatura
            {
                return Ok(MotorOpenVpn {
                    vpn: Arc::clone(&l.vpn),
                });
            }
        }
        // O perfil ou a identidade mudou desde que subiu: o motor velho para.
        self.parar(perfil.id);
        let falhou = |motivo: String| {
            let motivo = t!(
                "openvpn-nao-subiu",
                nome = perfil.name.as_str(),
                motivo = motivo
            );
            self.guardar_estado(perfil.id, State::Failed(motivo.clone()));
            motivo
        };
        if let Some(orfao) = self.orfaos.retomar(perfil.id, assinatura) {
            // O motor parado ainda roda (uma conexão o segura) e com a mesma identidade: volta
            // a ser o do perfil. Ligar outro derrubaria este no servidor.
            let vpn = Arc::clone(&orfao.motor);
            sem_veneno(&self.ligados).insert(
                perfil.id,
                Ligado {
                    vpn: orfao.motor,
                    assinatura,
                    vivo: orfao.vivo,
                    trava: orfao.trava,
                },
            );
            self.guardar_estado(perfil.id, vpn.state());
            return Ok(MotorOpenVpn { vpn });
        }
        let config = config.map_err(&falhou)?;
        let trava = TravaDoMotor::new(self.travas.travar(perfil).map_err(&falhou)?);
        let vivo = Arc::new(AtomicBool::new(true));
        let (estados, acordar, id, vivo_do_motor, trava_do_motor) = (
            Arc::clone(&self.estados),
            Arc::clone(&self.acordar),
            perfil.id,
            Arc::clone(&vivo),
            Arc::clone(&trava),
        );
        let ligado = OpenVpnBuilder::new(config)
            .name(&perfil.name)
            .on_state(move |estado| {
                if matches!(estado, State::Stopped | State::Failed(_)) {
                    trava_do_motor.soltar();
                }
                if vivo_do_motor.load(Ordering::Relaxed) {
                    sem_veneno(&estados).insert(id, estado.clone());
                    acordar_a_tela(&acordar);
                }
            })
            .start()
            .map_err(|e| falhou(e.to_string()))?;
        let ligado = Arc::new(ligado);
        sem_veneno(&self.ligados).insert(
            perfil.id,
            Ligado {
                vpn: Arc::clone(&ligado),
                assinatura,
                vivo,
                trava,
            },
        );
        self.guardar_estado(perfil.id, ligado.state());
        Ok(MotorOpenVpn { vpn: ligado })
    }

    fn guardar_estado(&self, perfil: u64, estado: State) {
        sem_veneno(&self.estados).insert(perfil, estado);
        acordar_a_tela(&self.acordar);
    }

    /// Para o motor do perfil (se está ligado), sem esperar: o aviso de saída ao servidor sai em
    /// segundo plano. Se uma conexão ainda segura o motor, ele segue até ela largar, e fica nos
    /// [`Orfaos`] para o próximo [`MotoresVpn::garantir`] reaproveitar ou fechar.
    pub fn parar(&self, perfil: u64) {
        let Some(ligado) = sem_veneno(&self.ligados).remove(&perfil) else {
            return;
        };
        ligado.vivo.store(false, Ordering::Relaxed);
        sem_veneno(&self.estados).remove(&perfil);
        acordar_a_tela(&self.acordar);
        match Arc::try_unwrap(ligado.vpn) {
            Ok(vpn) => {
                ligado.trava.soltar();
                std::thread::spawn(move || vpn.stop(Duration::from_secs(2)));
            }
            Err(vpn) => self.orfaos.guardar(
                perfil,
                Orfao {
                    motor: vpn,
                    assinatura: ligado.assinatura,
                    vivo: ligado.vivo,
                    trava: ligado.trava,
                },
            ),
        }
    }

    /// Para todos, esperando no máximo 200 ms: é o tempo de o aviso de saída chegar ao servidor
    /// antes de o app fechar.
    pub fn parar_todos(&self) {
        self.orfaos.fechar_todos();
        let todos: Vec<Ligado> = sem_veneno(&self.ligados).drain().map(|(_, l)| l).collect();
        for l in &todos {
            l.vivo.store(false, Ordering::Relaxed);
        }
        if todos.is_empty() {
            return;
        }
        let (fim, espera) = mpsc::channel();
        let mut esperados = 0;
        for l in todos {
            // Se alguém ainda segura o motor (uma conexão em andamento), ele para do mesmo jeito,
            // só sem esperar o aviso de saída.
            match Arc::try_unwrap(l.vpn) {
                Err(vpn) => vpn.close(),
                Ok(vpn) => {
                    let fim = fim.clone();
                    esperados += 1;
                    std::thread::spawn(move || {
                        vpn.stop(Duration::from_millis(200));
                        let _ = fim.send(());
                    });
                }
            }
        }
        let limite = std::time::Instant::now() + Duration::from_millis(200);
        for _ in 0..esperados {
            let resta = limite.saturating_duration_since(std::time::Instant::now());
            if espera.recv_timeout(resta).is_err() {
                break;
            }
        }
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

    /// As perguntas que os motores fazem agora (código de uso único, desafio, senha), com o id do
    /// perfil, na ordem dos perfis.
    pub fn perguntas(&self) -> Vec<(u64, Prompt)> {
        let mut todas: Vec<(u64, Prompt)> = sem_veneno(&self.estados)
            .iter()
            .filter_map(|(id, estado)| match estado {
                State::Asking(p) => Some((*id, p.clone())),
                _ => None,
            })
            .collect();
        todas.sort_by_key(|(id, _)| *id);
        todas
    }

    /// Responde à pergunta `pergunta` do perfil. `None` cancela: a VPN para com falha.
    pub fn responder(&self, perfil: u64, pergunta: u64, resposta: Option<String>) {
        if let Some(l) = sem_veneno(&self.ligados).get(&perfil) {
            l.vpn.answer(pergunta, resposta.map(Zeroizing::new));
        }
    }
}

fn acordar_a_tela(acordar: &Mutex<Option<Signal<u64>>>) {
    if let Some(sinal) = *sem_veneno(acordar) {
        crate::agente::acordar(sinal);
    }
}

/// O que identifica "este motor": o `.ovpn`, a identidade e as credenciais. Mudou, o motor
/// religa.
pub fn assinatura(
    ovpn: &str,
    identidade: &IdentidadeOpenVpn,
    usuario: &str,
    senha: &str,
    ignorar_rotas: bool,
) -> u64 {
    let mut h = DefaultHasher::new();
    ovpn.hash(&mut h);
    identidade.ca.hash(&mut h);
    identidade.certificado.hash(&mut h);
    identidade.chave.hash(&mut h);
    identidade.chave_tls.hash(&mut h);
    identidade.modo_tls.hash(&mut h);
    identidade.key_direction.hash(&mut h);
    usuario.hash(&mut h);
    senha.hash(&mut h);
    ignorar_rotas.hash(&mut h);
    h.finish()
}

/// O texto que a tela mostra para o tipo de uma pergunta.
pub fn rotulo_da_pergunta(tipo: PromptKind) -> String {
    match tipo {
        PromptKind::StaticChallenge => t!("openvpn-pergunta-desafio-estatico"),
        PromptKind::Challenge => t!("openvpn-pergunta-desafio"),
        PromptKind::Password => t!("openvpn-pergunta-senha"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUDO_EMBUTIDO: &str = include_str!("../tests/dados/openvpn/tudo-embutido.ovpn");
    const SCRIPTS: &str = include_str!("../tests/dados/openvpn/scripts.ovpn");

    /// 2030-03-17: dentro da validade dos certificados de teste (2026 a 2036).
    fn agora() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_900_000_000)
    }

    fn pasta_dos_dados() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dados/openvpn")
    }

    #[test]
    fn importar_separa_o_ovpn_dos_segredos() {
        let i = importar(TUDO_EMBUTIDO, None, agora()).unwrap();
        assert!(i.identidade.ca.contains("BEGIN CERTIFICATE"));
        assert!(i.identidade.certificado.contains("BEGIN CERTIFICATE"));
        assert!(i.identidade.chave.contains("PRIVATE KEY"));
        assert_eq!(i.identidade.modo_tls, "tls-auth");
        assert!(i.identidade.chave_tls.contains("OpenVPN Static key"));
        assert_eq!(i.identidade.key_direction, Some(1));
        assert_eq!(i.identidade.servidor, "vpn.exemplo.test:1194/udp");
        assert!(!i.identidade.cn.is_empty());
        assert!(i.identidade.vence_em > 0);
        assert_eq!(i.identidade.impressao.len(), 64);
        // O texto do perfil não tem nenhum segredo e continua lendo.
        assert!(!i.ovpn.contains("BEGIN"));
        assert!(i.ovpn.contains("remote vpn.exemplo.test 1194"));
        conferir_ovpn(&i.ovpn).unwrap();
        assert!(!i.pede_credenciais);
    }

    #[test]
    fn importar_de_arquivos_junto_do_ovpn() {
        let texto = std::fs::read_to_string(pasta_dos_dados().join("com-arquivos.ovpn")).unwrap();
        let i = importar(&texto, Some(&pasta_dos_dados()), agora()).unwrap();
        assert_eq!(i.identidade.modo_tls, "tls-crypt");
        assert!(!i.ovpn.contains("tc.key"));
        assert!(!i.ovpn.contains("cliente-rsa"));
    }

    #[test]
    fn arquivo_fora_da_pasta_e_recusado() {
        let texto = "client\nremote a.test\nca /etc/passwd\n";
        let erro = importar(texto, Some(&pasta_dos_dados()), agora()).unwrap_err();
        assert!(erro.contains("fora da pasta"), "{erro}");
    }

    #[test]
    fn o_pem_refeito_le_de_volta_com_o_mesmo_resumo() {
        let i = importar(TUDO_EMBUTIDO, None, agora()).unwrap();
        let pki = pki_de(&i.identidade).unwrap();
        let (original, _) = Config::from_ovpn(TUDO_EMBUTIDO, None).unwrap();
        assert_eq!(pki.ca, original.pki.ca);
        assert_eq!(pki.cert, original.pki.cert);
        assert_eq!(pki.key, original.pki.key);
        assert_eq!(pki.static_key, original.pki.static_key);
        assert_eq!(pki.wrap, original.pki.wrap);
        assert_eq!(pki.key_direction, original.pki.key_direction);
    }

    #[test]
    fn tls_crypt_v2_vai_e_volta() {
        let texto = std::fs::read_to_string(pasta_dos_dados().join("tc2-cliente.key")).unwrap();
        let wrap = parse_tls_crypt_v2_client(&texto).unwrap();
        let Wrap::TlsCryptV2 { kc, wkc } = &wrap else {
            panic!("não é tls-crypt-v2");
        };
        let refeito = pem_tls_crypt_v2(kc.as_slice(), wkc);
        assert_eq!(parse_tls_crypt_v2_client(&refeito).unwrap(), wrap);
    }

    #[test]
    fn a_identidade_refaz_o_config_completo() {
        let i = importar(TUDO_EMBUTIDO, None, agora()).unwrap();
        let config = config_de(&i.ovpn, &i.identidade, "", "", false, agora()).unwrap();
        let (original, _) = Config::from_ovpn(TUDO_EMBUTIDO, None).unwrap();
        assert_eq!(config.pki, original.pki);
        assert_eq!(config.remotes, original.remotes);
        assert_eq!(config.cipher, original.cipher);
    }

    #[test]
    fn auth_user_pass_sem_usuario_diz_o_que_fazer() {
        let texto = format!("{TUDO_EMBUTIDO}\nauth-user-pass\n");
        let i = importar(&texto, None, agora()).unwrap();
        assert!(i.pede_credenciais);
        let erro = config_de(&i.ovpn, &i.identidade, "", "", false, agora()).unwrap_err();
        assert!(!erro.is_empty());
        let config = config_de(&i.ovpn, &i.identidade, "ana", "segredo", false, agora()).unwrap();
        match config.auth_user_pass {
            AuthUserPass::Credentials { user, password } => {
                assert_eq!(user, "ana");
                assert_eq!(password.as_deref().map(String::as_str), Some("segredo"));
            }
            outro => panic!("{outro:?}"),
        }
        // Só o usuário: o motor pergunta a senha.
        let config = config_de(&i.ovpn, &i.identidade, "ana", "", false, agora()).unwrap();
        assert!(matches!(
            config.auth_user_pass,
            AuthUserPass::Credentials { password: None, .. }
        ));
    }

    #[test]
    fn o_usuario_do_item_vai_mesmo_sem_auth_user_pass_no_ovpn() {
        let texto = format!("{TUDO_EMBUTIDO}\n# auth-user-pass auth.txt\n");
        let i = importar(&texto, None, agora()).unwrap();
        assert!(!i.pede_credenciais);
        let config = config_de(&i.ovpn, &i.identidade, "", "", false, agora()).unwrap();
        assert_eq!(config.auth_user_pass, AuthUserPass::None);
        let config = config_de(&i.ovpn, &i.identidade, "ana", "segredo", false, agora()).unwrap();
        match config.auth_user_pass {
            AuthUserPass::Credentials { user, password } => {
                assert_eq!(user, "ana");
                assert_eq!(password.as_deref().map(String::as_str), Some("segredo"));
            }
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn o_usuario_e_a_senha_do_arquivo_saem_do_ovpn() {
        let texto = format!("{TUDO_EMBUTIDO}\n<auth-user-pass>\nana\nsegredo\n</auth-user-pass>\n");
        let i = importar(&texto, None, agora()).unwrap();
        assert_eq!((i.usuario.as_str(), i.senha.as_str()), ("ana", "segredo"));
        assert!(!i.ovpn.contains("segredo"));
        assert!(!i.ovpn.contains("ana"));
    }

    #[test]
    fn opcoes_de_script_viram_aviso_e_nao_rodam() {
        let i = importar(SCRIPTS, None, agora()).unwrap();
        assert!(
            !i.ignoradas.is_empty(),
            "scripts e plugin deviam ser avisados"
        );
    }

    #[test]
    fn config_com_segredo_no_texto_e_recusada() {
        let erro = conferir_ovpn(TUDO_EMBUTIDO).unwrap_err();
        assert_eq!(erro, t!("openvpn-config-com-segredo"));
    }

    #[test]
    fn certificado_vencido_recusa_a_importacao() {
        let dados = pasta_dos_dados();
        let texto = std::fs::read_to_string(dados.join("com-arquivos.ovpn"))
            .unwrap()
            .replace("cliente-rsa", "cliente-vencido");
        let erro = importar(&texto, Some(&dados), agora()).unwrap_err();
        assert!(erro.to_lowercase().contains("venc"), "{erro}");
    }

    #[test]
    fn completar_refaz_o_resumo_e_recusa_chave_no_lugar_do_certificado() {
        let mut i = importar(TUDO_EMBUTIDO, None, agora()).unwrap().identidade;
        let original = i.clone();
        i.cn.clear();
        i.impressao.clear();
        i.vence_em = 0;
        completar(&mut i).unwrap();
        assert_eq!(i, original);
        i.certificado = original.chave.clone();
        assert!(completar(&mut i).is_err());
    }

    #[test]
    fn substituir_o_certificado_confere_a_chave_e_refaz_o_resumo() {
        let dados = pasta_dos_dados();
        let atual = importar(TUDO_EMBUTIDO, None, agora()).unwrap().identidade;
        let cert = std::fs::read_to_string(dados.join("cliente-rsa.crt")).unwrap();
        let chave = std::fs::read_to_string(dados.join("cliente-rsa.key")).unwrap();
        // O mesmo certificado e a mesma chave: nada muda.
        let nova = substituir_certificado(&atual, &cert, Some(&chave), agora()).unwrap();
        assert_eq!(nova.impressao, atual.impressao);
        assert_eq!(nova.ca, atual.ca);
        assert_eq!(nova.chave_tls, atual.chave_tls);
        // Um certificado que não é da chave guardada é recusado.
        let vencido = std::fs::read_to_string(dados.join("cliente-vencido.crt")).unwrap();
        assert!(substituir_certificado(&atual, &vencido, None, agora()).is_err());
        // Uma chave no lugar do certificado também.
        assert!(substituir_certificado(&atual, &chave, None, agora()).is_err());
    }

    #[test]
    fn a_assinatura_muda_com_qualquer_parte() {
        let i = importar(TUDO_EMBUTIDO, None, agora()).unwrap();
        let base = assinatura(&i.ovpn, &i.identidade, "a", "b", false);
        assert_eq!(base, assinatura(&i.ovpn, &i.identidade, "a", "b", false));
        assert_ne!(base, assinatura(&i.ovpn, &i.identidade, "a", "c", false));
        assert_ne!(
            base,
            assinatura(&format!("{}\n", i.ovpn), &i.identidade, "a", "b", false)
        );
        let mut outra = i.identidade.clone();
        outra.chave.push('\n');
        assert_ne!(base, assinatura(&i.ovpn, &outra, "a", "b", false));
        assert_ne!(base, assinatura(&i.ovpn, &i.identidade, "a", "b", true));
    }

    #[test]
    fn o_estado_vira_texto_da_pagina() {
        assert_eq!(descrever_estado(None), t!("openvpn-estado-desligada"));
        assert_eq!(
            descrever_estado(Some(&State::Failed("senha recusada".into()))),
            t!("openvpn-estado-falhou", motivo = "senha recusada")
        );
        let conectada = State::Connected {
            address: "10.8.0.6".parse().unwrap(),
            remote: "vpn.exemplo.test:1194/udp".into(),
            cipher: "AES-256-GCM".into(),
            warnings: vec!["opção ignorada".into()],
        };
        assert!(descrever_estado(Some(&conectada)).contains("10.8.0.6"));
        assert_eq!(avisos_do_estado(Some(&conectada)), ["opção ignorada"]);
    }

    #[test]
    fn servidores_e_rotas_do_ovpn() {
        let ovpn = "client\nremote a.test 443 tcp\nremote b.test\nroute 10.1.0.0 255.255.0.0\nroute-nopull\n";
        assert_eq!(servidores(ovpn), ["a.test:443/tcp", "b.test:1194/udp"]);
        assert_eq!(rotas(ovpn), ["route 10.1.0.0 255.255.0.0", "route-nopull"]);
    }

    #[test]
    fn o_debug_do_motor_e_da_identidade_nao_mostram_segredo() {
        let i = importar(TUDO_EMBUTIDO, None, agora()).unwrap().identidade;
        let texto = format!("{i:?}");
        assert!(!texto.contains("PRIVATE KEY"));
        assert!(!texto.contains(i.chave.lines().nth(1).unwrap()));
    }
}
