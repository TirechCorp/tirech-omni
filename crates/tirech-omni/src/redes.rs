//! As redes de clientes (M34): perfis que dizem por onde uma conexão sai (um proxy SOCKS5 ou
//! HTTP em `127.0.0.1`, um salto e os comandos que sobem e descem a VPN). É dado puro, como o
//! `modelo.rs`. Mais abaixo ficam o [`preparar`] (sobe a rede quando a porta do proxy não
//! responde) e os [`Usos`] (quem ainda usa o perfil, para [`descer`] no último).

use std::collections::{BTreeMap, HashMap};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use tomni_ui::terminal::Proxy;

use crate::modelo::{Jump, Node, Tree};
use crate::nebula::MotorNebula;
use crate::openvpn::MotorOpenVpn;
use crate::t;
use crate::tailscale::MotorTailscale;
use crate::wireguard::MotorWireGuard;
use crate::zerotier::MotorZeroTier;

/// Os nomes das 8 cores da paleta dos perfis (o campo `cor` é o índice).
pub const CORES: [&str; 8] = [
    "Azul", "Verde", "Laranja", "Roxo", "Vermelho", "Turquesa", "Rosa", "Cinza",
];

/// O protocolo do proxy do perfil.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoProxy {
    /// SOCKS5 (o nome do servidor vai para o proxy, que resolve).
    #[default]
    Socks5,
    /// HTTP `CONNECT`.
    Http,
}

impl TipoProxy {
    /// O nome na tela.
    pub fn nome(self) -> &'static str {
        match self {
            TipoProxy::Socks5 => "SOCKS5",
            TipoProxy::Http => "HTTP",
        }
    }
}

/// O proxy por onde o perfil sai.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyRede {
    /// SOCKS5 ou HTTP.
    pub tipo: TipoProxy,
    /// O servidor do proxy (quase sempre `127.0.0.1`).
    pub host: String,
    /// A porta do proxy.
    pub port: u16,
    /// O item do cofre com o usuário e a senha do proxy. Um id que não existe mais vale `None`.
    #[serde(default)]
    pub credencial: Option<u64>,
}

impl ProxyRede {
    /// Um proxy SOCKS5 em `127.0.0.1:1081`, o que as receitas de `redes/` sobem.
    pub fn novo() -> Self {
        Self {
            tipo: TipoProxy::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 1081,
            credencial: None,
        }
    }

    /// `host:porta`, para as mensagens.
    pub fn endereco(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// A rede Nebula embutida de um perfil (M48): o motor roda dentro do app, só para o tráfego dele.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NebulaRede {
    /// O `config.yml` importado, **sem** `pki.ca`, `pki.cert` e `pki.key` (eles vão para o cofre).
    /// Nada aqui é segredo.
    #[serde(default)]
    pub config: String,
    /// O item `Tipo::Nebula` do cofre com a CA, o certificado e a chave. Um id que não existe
    /// mais vale `None`.
    #[serde(default)]
    pub identidade: Option<u64>,
    /// De onde veio o config (o nome do arquivo importado), só para mostrar.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origem: String,
}

/// A rede OpenVPN embutida de um perfil (M56): o motor roda dentro do app, só para o tráfego dele.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenVpnRede {
    /// O `.ovpn` importado, **sem** a CA, o certificado, a chave, as chaves TLS nem o conteúdo do
    /// `auth-user-pass` (eles vão para o cofre). Nada aqui é segredo.
    #[serde(default)]
    pub config: String,
    /// O item `Tipo::OpenVpn` do cofre com os segredos do perfil. Um id que não existe mais vale
    /// `None`.
    #[serde(default)]
    pub identidade: Option<u64>,
    /// De onde veio o `.ovpn` (o nome do arquivo importado), só para mostrar.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origem: String,
    /// Não usar as rotas que o servidor empurra (o `route-nopull`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ignorar_rotas: bool,
}

/// A rede WireGuard embutida de um perfil (M57): o motor roda dentro do app, só para o tráfego dele.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WireGuardRede {
    /// O `.conf` importado, **sem** a `PrivateKey` e as `PresharedKey` (elas vão para o cofre).
    /// Nada aqui é segredo.
    #[serde(default)]
    pub config: String,
    /// O item `Tipo::WireGuard` do cofre com a chave privada e as chaves pré-compartilhadas. Um
    /// id que não existe mais vale `None`.
    #[serde(default)]
    pub identidade: Option<u64>,
    /// De onde veio o `.conf` (o nome do arquivo importado), só para mostrar.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origem: String,
}

/// A rede Tailscale embutida de um perfil (M57), do Tailscale ou de um Headscale: o motor roda
/// dentro do app, só para o tráfego dele.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TailscaleRede {
    /// A URL do servidor de controle; vazia é o Tailscale oficial.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub servidor: String,
    /// O nome da máquina na tailnet; vazio usa o nome do computador.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nome_maquina: String,
    /// O item `Tipo::Tailscale` do cofre com o estado da máquina (chaves, login, chave de
    /// autenticação). Um id que não existe mais vale `None`.
    #[serde(default)]
    pub identidade: Option<u64>,
    /// Usar as sub-redes que outras máquinas anunciam.
    #[serde(default = "verdadeiro")]
    pub usar_sub_redes: bool,
    /// A máquina é efêmera: o controle a esquece quando ela some.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub efemero: bool,
    /// CAs extras em PEM para o TLS do servidor (um Headscale com CA própria). Não é segredo.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ca: String,
}

fn verdadeiro() -> bool {
    true
}

impl Default for TailscaleRede {
    fn default() -> Self {
        Self {
            servidor: String::new(),
            nome_maquina: String::new(),
            identidade: None,
            usar_sub_redes: true,
            efemero: false,
            ca: String::new(),
        }
    }
}

/// A rede ZeroTier embutida de um perfil (M61): o nó roda dentro do app, só para o tráfego dele.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZeroTierRede {
    /// O id da rede, 16 hexadecimais minúsculos.
    #[serde(default)]
    pub rede: String,
    /// O item `Tipo::ZeroTier` do cofre (a identidade e o estado do nó). Um id que não existe
    /// mais vale `None`.
    #[serde(default)]
    pub identidade: Option<u64>,
    /// Aceitar IPs e rotas em faixas públicas (o `allowGlobal` do ZeroTier).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub permitir_globais: bool,
    /// Aceitar a rota padrão da rede (o `allowDefault`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub permitir_padrao: bool,
    /// Usar o DNS que a rede anuncia (o `allowDNS`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub permitir_dns: bool,
    /// Um IP fixo para as redes que não distribuem endereço (`10.0.0.5/24`); vazio vale o do
    /// controlador.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ip_fixo: String,
    /// As moons para orbitar: o id (16 hexadecimais) e a semente (10 hexadecimais).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moons: Vec<(String, String)>,
}

/// Uma rede de cliente.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfilRede {
    /// Estável e nunca reaproveitado; vem do `next_id` da [`Tree`].
    #[serde(default)]
    pub id: u64,
    /// O nome na tela ("Cliente A").
    pub name: String,
    /// O índice na paleta [`CORES`].
    #[serde(default)]
    pub cor: u8,
    /// O proxy; sem ele, o perfil só vale pelo salto.
    #[serde(default)]
    pub proxy: Option<ProxyRede>,
    /// Um salto que vem antes do da conexão (só no SSH e no SFTP próprio).
    #[serde(default)]
    pub salto: Option<Jump>,
    /// O comando que sobe a rede quando a porta do proxy não responde.
    #[serde(default)]
    pub subir: Option<String>,
    /// O comando que desce a rede quando o último uso fecha.
    #[serde(default)]
    pub descer: Option<String>,
    /// Quantos segundos esperar a porta do proxy depois de subir (ou a rede Nebula conectar).
    #[serde(default = "espera_padrao")]
    pub espera_secs: u32,
    /// A rede Nebula embutida. Com ela, `proxy`, `subir` e `descer` não valem: o motor é interno
    /// e entrega o próprio proxy ([`RedeResolvida`]). O `salto` continua valendo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nebula: Option<NebulaRede>,
    /// A rede OpenVPN embutida (M56), com as mesmas regras da Nebula: o motor é interno e entrega
    /// o próprio proxy. Um perfil tem uma rede embutida só, a Nebula ou a OpenVPN.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openvpn: Option<OpenVpnRede>,
    /// A rede WireGuard embutida (M57), com as mesmas regras da Nebula.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wireguard: Option<WireGuardRede>,
    /// A rede Tailscale embutida (M57), com as mesmas regras da Nebula.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<TailscaleRede>,
    /// A rede ZeroTier embutida (M61), com as mesmas regras da Nebula.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zerotier: Option<ZeroTierRede>,
}

fn espera_padrao() -> u32 {
    30
}

impl PerfilRede {
    /// Um perfil novo, sem proxy. O `id` é preenchido pela [`Tree`].
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: 0,
            name: name.into(),
            cor: 0,
            proxy: None,
            salto: None,
            subir: None,
            descer: None,
            espera_secs: espera_padrao(),
            nebula: None,
            openvpn: None,
            wireguard: None,
            tailscale: None,
            zerotier: None,
        }
    }

    /// Se o perfil é uma rede Nebula embutida.
    pub fn e_nebula(&self) -> bool {
        self.nebula.is_some()
    }

    /// Se o perfil é uma rede OpenVPN embutida.
    pub fn e_openvpn(&self) -> bool {
        self.openvpn.is_some()
    }

    /// Se o perfil é uma rede WireGuard embutida.
    pub fn e_wireguard(&self) -> bool {
        self.wireguard.is_some()
    }

    /// Se o perfil é uma rede Tailscale embutida.
    pub fn e_tailscale(&self) -> bool {
        self.tailscale.is_some()
    }

    /// Se o perfil é uma rede ZeroTier embutida.
    pub fn e_zerotier(&self) -> bool {
        self.zerotier.is_some()
    }

    /// Se o perfil tem um motor embutido (Nebula, OpenVPN, WireGuard, Tailscale ou ZeroTier): o
    /// `proxy`, o `subir` e o `descer` não valem, o motor entrega o próprio proxy.
    pub fn e_embutida(&self) -> bool {
        self.nebula.is_some()
            || self.openvpn.is_some()
            || self.wireguard.is_some()
            || self.tailscale.is_some()
            || self.zerotier.is_some()
    }

    /// O nome da cor.
    pub fn nome_da_cor(&self) -> &'static str {
        CORES[usize::from(self.cor) % CORES.len()]
    }
}

/// A rede de um grupo ou de uma conexão.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rede {
    /// Vale a do grupo de cima (e, sem nenhuma, a direta).
    #[default]
    Herdar,
    /// Sai direto, mesmo dentro de um grupo com perfil.
    Direta,
    /// Sai pelo perfil de id dado.
    Perfil(u64),
}

/// O que uma escolha de rede decide: `None` segue subindo, `Some(None)` sai direto e
/// `Some(Some(perfil))` sai pelo perfil. Um perfil que não existe mais vale como direta.
fn decide(tree: &Tree, rede: Rede) -> Option<Option<&PerfilRede>> {
    match rede {
        Rede::Herdar => None,
        Rede::Direta => Some(None),
        Rede::Perfil(id) => Some(tree.perfil(id)),
    }
}

/// A rede que o item `no` (grupo ou conexão) herdaria: sobe a partir do grupo de cima e vale o
/// primeiro [`Rede::Direta`] (`None`) ou [`Rede::Perfil`].
pub fn rede_herdada(tree: &Tree, no: u64) -> Option<&PerfilRede> {
    for grupo in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(grupo)
            && let Some(r) = decide(tree, g.rede)
        {
            return r;
        }
    }
    None
}

/// A rede por onde a conexão `conexao` sai: a dela, se não for [`Rede::Herdar`]; senão sobe
/// pelos grupos ([`rede_herdada`]).
pub fn rede_efetiva(tree: &Tree, conexao: u64) -> Option<&PerfilRede> {
    if let Some(c) = tree.connection(conexao)
        && let Some(r) = decide(tree, c.rede)
    {
        return r;
    }
    rede_herdada(tree, conexao)
}

/// A opção "Direta" da linha "Rede".
pub fn direta() -> String {
    t!("redes-direta")
}
/// O começo da opção "Herdar (Cliente A)" da linha "Rede".
pub fn herdar() -> String {
    t!("redes-herdar")
}

/// O texto da opção "Herdar" do item `no`: mostra a rede que ele herdaria agora.
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    let nome = rede_herdada(tree, no).map_or_else(direta, |p| p.name.clone());
    t!("redes-herdar-de", nome = nome)
}

/// O texto da linha "Rede" para `rede` no item `no`.
pub fn rotulo_da_rede(tree: &Tree, no: u64, rede: Rede) -> String {
    match rede {
        Rede::Herdar => rotulo_de_herdar(tree, no),
        Rede::Direta => direta(),
        Rede::Perfil(id) => tree.perfil(id).map_or_else(direta, |p| p.name.clone()),
    }
}

/// As opções da linha "Rede" do item `no`: herdar, direta e cada perfil.
pub fn opcoes_de_rede(tree: &Tree, no: u64) -> Vec<String> {
    let mut opcoes = vec![rotulo_de_herdar(tree, no), direta()];
    opcoes.extend(tree.redes.iter().map(|p| p.name.clone()));
    opcoes
}

/// A rede que o texto da linha "Rede" escolhe (`None`: o texto não é de nenhuma opção).
pub fn rede_do_rotulo(tree: &Tree, texto: &str) -> Option<Rede> {
    if texto == direta() {
        return Some(Rede::Direta);
    }
    if e_herdar(texto) {
        return Some(Rede::Herdar);
    }
    tree.redes
        .iter()
        .find(|p| p.name == texto)
        .map(|p| Rede::Perfil(p.id))
}

/// Se o texto é a opção "Herdar" ou "Herdar (…)".
fn e_herdar(texto: &str) -> bool {
    texto == herdar() || texto.starts_with(&format!("{} (", herdar()))
}

/// Se o texto é de uma das opções fixas da linha "Rede" ("Direta" e "Herdar (…)").
fn e_opcao_fixa(texto: &str) -> bool {
    texto == direta() || e_herdar(texto)
}

/// `base`, ou `base (2)`, `base (3)`… se outro perfil (que não `ignorar`) já tem o nome. Um nome
/// que imita uma opção fixa da linha "Rede" ganha "Rede " na frente.
pub fn nome_livre(tree: &Tree, base: &str, ignorar: Option<u64>) -> String {
    let base = base.trim();
    let base = if base.is_empty() {
        t!("comum-rede")
    } else if e_opcao_fixa(base) {
        t!("redes-prefixo-opcao-fixa", base = base)
    } else {
        base.to_owned()
    };
    let ocupado = |nome: &str| {
        tree.redes
            .iter()
            .any(|p| Some(p.id) != ignorar && p.name == nome)
    };
    if !ocupado(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base} ({n})"))
        .find(|nome| !ocupado(nome))
        .unwrap_or(base)
}

/// O motor WireGuard, Tailscale ou ZeroTier ligado de um perfil (M57, M61). Os três são assíncronos
/// e rodam no runtime próprio de cada módulo; a `RedeResolvida` só precisa do proxy, do estado e da
/// espera.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MotorRede {
    /// Um túnel WireGuard.
    WireGuard(MotorWireGuard),
    /// Um nó da tailnet.
    Tailscale(MotorTailscale),
    /// Um membro de uma rede ZeroTier.
    ZeroTier(MotorZeroTier),
}

impl MotorRede {
    /// O proxy SOCKS5 da ponte do motor.
    pub fn proxy(&self) -> Proxy {
        match self {
            MotorRede::WireGuard(m) => m.proxy(),
            MotorRede::Tailscale(m) => m.proxy(),
            MotorRede::ZeroTier(m) => m.proxy(),
        }
    }

    /// Espera o motor conectar (até `espera_secs`). Bloqueia a thread de quem chama, que não pode
    /// ser a da tela.
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que a rede não conectou.
    pub fn esperar(&self, perfil: &PerfilRede, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        match self {
            MotorRede::WireGuard(m) => m.esperar(perfil, aviso),
            MotorRede::Tailscale(m) => m.esperar(perfil, aviso),
            MotorRede::ZeroTier(m) => m.esperar(perfil, aviso),
        }
    }
}

/// A rede de uma abertura, já com o que o cofre guarda: o `Opener` não enxerga a [`Tree`] nem o
/// cofre, então a tela resolve tudo e entrega isto dentro das `Senhas`.
#[derive(Clone, PartialEq, Eq)]
pub struct RedeResolvida {
    /// O perfil, com o usuário do salto já preenchido pela credencial dele.
    pub perfil: PerfilRede,
    /// O usuário do proxy, do item do cofre.
    pub proxy_usuario: Option<String>,
    /// A senha do proxy.
    pub proxy_senha: Option<String>,
    /// A senha do salto do perfil.
    pub salto_senha: Option<String>,
    /// O `known_hosts` do perfil (veja [`known_hosts_do_perfil`]).
    pub known_hosts: Option<PathBuf>,
    /// O motor Nebula do perfil, ligado pela tela (M48). Com ele, o `perfil.proxy` é a ponte do
    /// motor e `proxy_usuario`/`proxy_senha` são a credencial dela.
    pub nebula: Option<MotorNebula>,
    /// Por que o motor Nebula não subiu. Com isto, o [`RedeResolvida::preparar`] devolve o erro e
    /// nada conecta direto (o `perfil.proxy` aponta para uma porta que não existe).
    pub nebula_erro: Option<String>,
    /// O motor OpenVPN do perfil, ligado pela tela (M56), com as mesmas regras do `nebula`.
    pub openvpn: Option<MotorOpenVpn>,
    /// Por que o motor OpenVPN não subiu, com as mesmas regras do `nebula_erro`.
    pub openvpn_erro: Option<String>,
    /// O motor WireGuard, Tailscale ou ZeroTier do perfil, ligado pela tela (M57, M61), com as mesmas regras do
    /// `nebula`.
    pub motor: Option<MotorRede>,
    /// Por que o motor WireGuard, Tailscale ou ZeroTier não subiu, com as mesmas regras do `nebula_erro`.
    pub motor_erro: Option<String>,
}

// O `Debug` não mostra as senhas.
impl std::fmt::Debug for RedeResolvida {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedeResolvida")
            .field("perfil", &self.perfil.name)
            .field("proxy_usuario", &self.proxy_usuario)
            .field("proxy_senha", &self.proxy_senha.as_ref().map(|_| "***"))
            .field("salto_senha", &self.salto_senha.as_ref().map(|_| "***"))
            .field("known_hosts", &self.known_hosts)
            .field("nebula", &self.nebula)
            .field("nebula_erro", &self.nebula_erro)
            .field("openvpn", &self.openvpn)
            .field("openvpn_erro", &self.openvpn_erro)
            .field("motor", &self.motor)
            .field("motor_erro", &self.motor_erro)
            .finish()
    }
}

impl RedeResolvida {
    /// O proxy do perfil, com o usuário e a senha. `None` se o perfil não tem proxy.
    pub fn proxy(&self) -> Option<Proxy> {
        let p = self.perfil.proxy.as_ref()?;
        let (user, password) = (self.proxy_usuario.clone(), self.proxy_senha.clone());
        Some(match p.tipo {
            TipoProxy::Socks5 => Proxy::Socks5 {
                host: p.host.clone(),
                port: p.port,
                user,
                password,
            },
            TipoProxy::Http => Proxy::Http {
                host: p.host.clone(),
                port: p.port,
                user,
                password,
            },
        })
    }

    /// O proxy como URL para o `rclone` (`socks5h://usuario:senha@127.0.0.1:1081`): o `h` faz o
    /// proxy resolver os nomes, como no SSH.
    pub fn url_do_proxy(&self) -> Option<String> {
        let p = self.perfil.proxy.as_ref()?;
        let esquema = match p.tipo {
            TipoProxy::Socks5 => "socks5h",
            TipoProxy::Http => "http",
        };
        let acesso = match (&self.proxy_usuario, &self.proxy_senha) {
            (Some(u), Some(s)) => format!("{}:{}@", codificar(u), codificar(s)),
            (Some(u), None) => format!("{}@", codificar(u)),
            _ => String::new(),
        };
        Some(format!("{esquema}://{acesso}{}", p.endereco()))
    }

    /// Deixa a rede pronta para a conexão sair por ela: no perfil Nebula, OpenVPN, WireGuard ou
    /// Tailscale, espera o motor conectar; no de proxy, é o [`preparar`]. Uma rede Nebula que não subiu devolve o erro (a
    /// conexão nunca sai direto).
    ///
    /// # Errors
    ///
    /// A mensagem, em português, de por que a rede não ficou pronta.
    pub fn preparar(&self, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        if let Some(erro) = self
            .nebula_erro
            .as_ref()
            .or(self.openvpn_erro.as_ref())
            .or(self.motor_erro.as_ref())
        {
            return Err(erro.clone());
        }
        if let Some(motor) = &self.nebula {
            return motor.esperar(&self.perfil, aviso);
        }
        if let Some(motor) = &self.openvpn {
            return motor.esperar(&self.perfil, aviso);
        }
        if let Some(motor) = &self.motor {
            return motor.esperar(&self.perfil, aviso);
        }
        preparar(&self.perfil, aviso)
    }

    /// O gancho do `before_connect`: [`RedeResolvida::preparar`] a rede.
    // O tipo é o do `before_connect` da lib; um alias não cabe num `impl Trait` de retorno.
    #[allow(clippy::type_complexity)]
    pub fn gancho(&self) -> impl FnMut(&mut dyn FnMut(&str)) -> Result<(), String> + Send + use<> {
        let rede = self.clone();
        move |aviso| rede.preparar(aviso)
    }
}

/// Codifica o texto para o usuário e a senha de uma URL (tudo fora de letras, números e `-_.~`
/// vira `%XX`).
fn codificar(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for byte in texto.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            saida.push(char::from(byte));
        } else {
            saida.push_str(&format!("%{byte:02X}"));
        }
    }
    saida
}

/// O `known_hosts` do perfil, em `pasta/redes/<id>/known_hosts`. Os saltos do perfil usam o
/// mesmo arquivo. A conexão direta mantém o `known_hosts` de sempre.
pub fn known_hosts_do_perfil(pasta: &Path, perfil: u64) -> PathBuf {
    pasta
        .join("redes")
        .join(perfil.to_string())
        .join("known_hosts")
}

/// A trava que deixa um perfil de rede embutida (OpenVPN, WireGuard, Tailscale, Nebula ou
/// ZeroTier) ligado em uma só cópia do app por vez: duas cópias com a mesma identidade derrubam
/// uma à outra no servidor. É o `redes/<id>/trava.lock` da pasta de configuração, pego quando o
/// motor liga e solto quando ele para (ou quando o app fecha ou cai).
#[derive(Default)]
pub struct TravasDeRede {
    pasta: Mutex<Option<PathBuf>>,
}

impl TravasDeRede {
    /// A pasta de configuração da tela. Sem ela (nos testes dos motores), não há trava.
    pub fn na_pasta(&self, pasta: &Path) {
        *self
            .pasta
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(pasta.to_path_buf());
    }

    /// Pega a trava do perfil. Um erro de disco não impede a rede de subir (`Ok(None)`).
    ///
    /// # Errors
    ///
    /// A mensagem de que outra cópia do app já está com o perfil ligado.
    pub fn travar(&self, perfil: &PerfilRede) -> Result<Option<std::fs::File>, String> {
        let pasta = self
            .pasta
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(pasta) = pasta else {
            return Ok(None);
        };
        match crate::armazenamento::travar(&pasta.join("redes").join(perfil.id.to_string())) {
            Ok(crate::armazenamento::Trava::Travada(arquivo)) => Ok(Some(arquivo)),
            Ok(crate::armazenamento::Trava::Ocupada) => Err(t!("redes-ligada-em-outra-copia")),
            Err(_) => Ok(None),
        }
    }
}

impl Tree {
    /// O perfil de id `id`.
    pub fn perfil(&self, id: u64) -> Option<&PerfilRede> {
        self.redes.iter().find(|p| p.id == id)
    }

    /// O perfil de id `id`, para mudar.
    pub fn perfil_mut(&mut self, id: u64) -> Option<&mut PerfilRede> {
        self.redes.iter_mut().find(|p| p.id == id)
    }

    /// Acrescenta o perfil e devolve o id que ele recebeu.
    pub fn add_perfil(&mut self, mut perfil: PerfilRede) -> u64 {
        let id = self.novo_id();
        perfil.id = id;
        self.redes.push(perfil);
        id
    }

    /// Copia o perfil logo depois dele, com " (cópia)" no nome. Devolve o id da cópia.
    pub fn duplicar_perfil(&mut self, id: u64) -> Option<u64> {
        let pos = self.redes.iter().position(|p| p.id == id)?;
        let mut copia = self.redes[pos].clone();
        copia.name = t!("comum-titulo-copia", titulo = copia.name.as_str());
        let novo = self.novo_id();
        copia.id = novo;
        self.redes.insert(pos + 1, copia);
        Some(novo)
    }

    /// Apaga o perfil e troca por [`Rede::Herdar`] o que apontava para ele, na mesma gravação.
    /// Devolve `false` se o id não existe.
    pub fn apagar_perfil(&mut self, id: u64) -> bool {
        let antes = self.redes.len();
        self.redes.retain(|p| p.id != id);
        if self.redes.len() == antes {
            return false;
        }
        soltar(&mut self.nodes, id);
        true
    }
}

fn soltar(nodes: &mut [Node], perfil: u64) {
    for node in nodes {
        match node {
            Node::Connection(c) => {
                if c.rede == Rede::Perfil(perfil) {
                    c.rede = Rede::Herdar;
                }
            }
            Node::Group(g) => {
                if g.rede == Rede::Perfil(perfil) {
                    g.rede = Rede::Herdar;
                }
                soltar(&mut g.children, perfil);
            }
        }
    }
}

/// Quanto o [`preparar`] espera o TCP da porta do proxy responder a cada tentativa.
const TEMPO_DA_PORTA: Duration = Duration::from_millis(500);
/// O intervalo entre uma tentativa e outra, depois de subir a rede.
const INTERVALO: Duration = Duration::from_millis(250);

fn sem_veneno<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Uma trava por perfil: a segunda chamada do [`preparar`] espera a primeira e vê a porta já
/// aberta, em vez de rodar o comando de novo.
fn trava_do(perfil: u64) -> Arc<Mutex<()>> {
    static TRAVAS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<()>>>>> = OnceLock::new();
    let mapa = TRAVAS.get_or_init(Default::default);
    Arc::clone(sem_veneno(mapa).entry(perfil).or_default())
}

/// A porta aceita uma conexão TCP?
fn porta_aberta(proxy: &ProxyRede) -> bool {
    let Ok(enderecos) = (proxy.host.as_str(), proxy.port).to_socket_addrs() else {
        return false;
    };
    enderecos
        .into_iter()
        .any(|a| TcpStream::connect_timeout(&a, TEMPO_DA_PORTA).is_ok())
}

/// O comando do perfil no interpretador do sistema (`sh -c` ou `cmd /C`).
fn comando(texto: &str) -> Command {
    #[cfg(windows)]
    {
        let mut c = Command::new("cmd");
        c.args(["/C", texto]);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.args(["-c", texto]);
        c
    }
}

/// Roda o comando até o fim e devolve o que ele escreveu no stderr. O stderr vai para um
/// arquivo (e não para um pipe) para um processo que ele deixe rodando ao fundo não segurar a
/// leitura.
fn rodar(texto: &str) -> Result<(Option<i32>, String), String> {
    static N: AtomicU64 = AtomicU64::new(0);
    let arquivo = std::env::temp_dir().join(format!(
        "tirech-omni-rede-{}-{}.err",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let erro = std::fs::File::create(&arquivo).map_err(|e| e.to_string())?;
    let mut comando = comando(texto);
    let estado = crate::sistema::sem_janela(&mut comando)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(erro)
        .status();
    let saida = std::fs::read_to_string(&arquivo).unwrap_or_default();
    let _ = std::fs::remove_file(&arquivo);
    let estado = estado.map_err(|e| t!("redes-nao-consegui-rodar", erro = e.to_string()))?;
    Ok((estado.code(), saida))
}

/// A última linha que não está vazia.
fn ultima_linha(texto: &str) -> &str {
    texto
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// Deixa a rede do perfil pronta para a conexão sair por ela.
///
/// Sem proxy não há o que esperar. Se a porta do proxy já aceita TCP, não roda nada. Senão roda
/// o comando `subir` (quando há) e tenta a porta a cada 250 ms por até `espera_secs`. O `aviso`
/// recebe as linhas que o terminal mostra ("Subindo a rede…", "Rede … pronta."). Duas chamadas
/// do mesmo perfil não rodam o comando duas vezes: a segunda espera a primeira.
pub fn preparar(perfil: &PerfilRede, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
    let Some(proxy) = &perfil.proxy else {
        return Ok(());
    };
    let trava = trava_do(perfil.id);
    let _preso = sem_veneno(&trava);
    if porta_aberta(proxy) {
        return Ok(());
    }
    if let Some(subir) = perfil.subir.as_deref().filter(|c| !c.trim().is_empty()) {
        aviso(&t!("redes-subindo", nome = perfil.name.as_str()));
        let (codigo, saida) = rodar(subir)?;
        if codigo != Some(0) {
            let linha = ultima_linha(&saida);
            return Err(if linha.is_empty() {
                match codigo {
                    Some(c) => t!(
                        "redes-nao-subiu-codigo",
                        nome = perfil.name.as_str(),
                        c = i64::from(c)
                    ),
                    None => t!("redes-nao-subiu", nome = perfil.name.as_str()),
                }
            } else {
                t!(
                    "redes-nao-subiu-motivo",
                    nome = perfil.name.as_str(),
                    linha = linha
                )
            });
        }
    }
    let limite = Instant::now() + Duration::from_secs(u64::from(perfil.espera_secs));
    loop {
        if porta_aberta(proxy) {
            aviso(&t!("redes-pronta", nome = perfil.name.as_str()));
            return Ok(());
        }
        if Instant::now() >= limite {
            return Err(t!(
                "redes-nao-respondeu",
                nome = perfil.name.as_str(),
                espera = i64::from(perfil.espera_secs),
                endereco = proxy.endereco().to_string()
            ));
        }
        std::thread::sleep(INTERVALO);
    }
}

/// Roda o `descer` do perfil numa thread, sem diálogo e sem esperar. O erro é ignorado: a rede
/// que não desce fica no ar, e o próximo `preparar` a encontra pronta.
pub fn descer(perfil: &PerfilRede) {
    if let Some(texto) = comando_de_descer(perfil) {
        std::thread::spawn(move || {
            let _ = rodar(&texto);
        });
    }
}

/// Como [`descer`], para o app que está fechando: só dispara o comando (`spawn`), sem thread
/// nem espera, que o processo não sobreviveria para terminar.
pub fn descer_ao_fechar(perfil: &PerfilRede) {
    if let Some(texto) = comando_de_descer(perfil) {
        let _ = comando(&texto)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

fn comando_de_descer(perfil: &PerfilRede) -> Option<String> {
    // O perfil Nebula, OpenVPN, WireGuard, Tailscale ou ZeroTier não roda comando: o motor é interno e para pelos `Motores`.
    if perfil.e_embutida() {
        return None;
    }
    perfil
        .descer
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
}

/// Quais perfis têm sessão ou painel de arquivos aberto. A tela diz o que está aberto agora
/// ([`Usos::atualizar`]) e recebe de volta os perfis que ficaram sem uso, para o [`descer`].
#[derive(Default)]
pub struct Usos {
    /// Os perfis em uso, como estavam quando o primeiro uso entrou: editar o comando depois não
    /// muda o que roda na saída.
    ativos: Mutex<BTreeMap<u64, PerfilRede>>,
}

impl Usos {
    /// Troca o conjunto de perfis em uso por `agora` (os que têm proxy ou rede embutida; um perfil repetido conta
    /// uma vez). Devolve os que estavam em uso e deixaram de estar.
    pub fn atualizar(&self, agora: &[PerfilRede]) -> Vec<PerfilRede> {
        let mut ativos = sem_veneno(&self.ativos);
        let mut novos = BTreeMap::new();
        for perfil in agora.iter().filter(|p| p.proxy.is_some() || p.e_embutida()) {
            let guardado = ativos.get(&perfil.id).unwrap_or(perfil).clone();
            novos.entry(perfil.id).or_insert(guardado);
        }
        let saiu = ativos
            .iter()
            .filter(|(id, _)| !novos.contains_key(id))
            .map(|(_, p)| p.clone())
            .collect();
        *ativos = novos;
        saiu
    }

    /// Os perfis em uso agora (o app fechando roda o `descer` deles).
    pub fn em_uso(&self) -> Vec<PerfilRede> {
        sem_veneno(&self.ativos).values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::{Connection, Kind};

    /// Raiz → grupo "Clientes" → grupo "A" → conexão; mais uma conexão solta na raiz.
    struct Cena {
        tree: Tree,
        cliente_a: u64,
        clientes: u64,
        grupo_a: u64,
        conexao: u64,
        solta: u64,
    }

    fn cena() -> Cena {
        let mut tree = Tree::default();
        let cliente_a = tree.add_perfil(PerfilRede::new("Cliente A"));
        let clientes = tree.add_group(None, "Clientes");
        let grupo_a = tree.add_group(Some(clientes), "A");
        let conexao = tree.add_connection(Some(grupo_a), Connection::new("web", Kind::Ssh));
        let solta = tree.add_connection(None, Connection::new("solta", Kind::Ssh));
        Cena {
            tree,
            cliente_a,
            clientes,
            grupo_a,
            conexao,
            solta,
        }
    }

    fn grupo_mut(tree: &mut Tree, id: u64) -> &mut crate::modelo::Group {
        match tree.find_mut(id) {
            Some(Node::Group(g)) => g,
            _ => panic!("sem o grupo {id}"),
        }
    }

    fn nome(tree: &Tree, id: u64) -> Option<&str> {
        rede_efetiva(tree, id).map(|p| p.name.as_str())
    }

    #[test]
    fn sem_nenhum_perfil_sai_direto() {
        let c = cena();
        assert_eq!(nome(&c.tree, c.conexao), None);
        assert_eq!(nome(&c.tree, c.solta), None);
    }

    #[test]
    fn a_conexao_herda_do_grupo() {
        let mut c = cena();
        grupo_mut(&mut c.tree, c.clientes).rede = Rede::Perfil(c.cliente_a);
        assert_eq!(nome(&c.tree, c.conexao), Some("Cliente A"));
        // Fora do grupo, nada muda.
        assert_eq!(nome(&c.tree, c.solta), None);
    }

    #[test]
    fn o_grupo_de_dentro_vale_mais_que_o_de_fora() {
        let mut c = cena();
        let b = c.tree.add_perfil(PerfilRede::new("Cliente B"));
        grupo_mut(&mut c.tree, c.clientes).rede = Rede::Perfil(c.cliente_a);
        grupo_mut(&mut c.tree, c.grupo_a).rede = Rede::Perfil(b);
        assert_eq!(nome(&c.tree, c.conexao), Some("Cliente B"));
    }

    #[test]
    fn direta_corta_a_heranca() {
        let mut c = cena();
        grupo_mut(&mut c.tree, c.clientes).rede = Rede::Perfil(c.cliente_a);
        grupo_mut(&mut c.tree, c.grupo_a).rede = Rede::Direta;
        assert_eq!(nome(&c.tree, c.conexao), None);
    }

    #[test]
    fn a_conexao_vale_mais_que_o_grupo() {
        let mut c = cena();
        grupo_mut(&mut c.tree, c.clientes).rede = Rede::Perfil(c.cliente_a);
        c.tree.connection_mut(c.conexao).unwrap().rede = Rede::Direta;
        assert_eq!(nome(&c.tree, c.conexao), None);
        c.tree.connection_mut(c.solta).unwrap().rede = Rede::Perfil(c.cliente_a);
        assert_eq!(nome(&c.tree, c.solta), Some("Cliente A"));
    }

    #[test]
    fn perfil_que_nao_existe_vale_direta() {
        let mut c = cena();
        grupo_mut(&mut c.tree, c.clientes).rede = Rede::Perfil(c.cliente_a);
        // O perfil da conexão sumiu: não cai no do grupo, é direta.
        c.tree.connection_mut(c.conexao).unwrap().rede = Rede::Perfil(999);
        assert_eq!(nome(&c.tree, c.conexao), None);
    }

    #[test]
    fn apagar_o_perfil_troca_as_referencias_por_herdar() {
        let mut c = cena();
        grupo_mut(&mut c.tree, c.grupo_a).rede = Rede::Perfil(c.cliente_a);
        c.tree.connection_mut(c.solta).unwrap().rede = Rede::Perfil(c.cliente_a);
        let b = c.tree.add_perfil(PerfilRede::new("Cliente B"));
        c.tree.connection_mut(c.conexao).unwrap().rede = Rede::Perfil(b);

        assert!(c.tree.apagar_perfil(c.cliente_a));
        assert!(!c.tree.apagar_perfil(c.cliente_a));
        assert_eq!(grupo_mut(&mut c.tree, c.grupo_a).rede, Rede::Herdar);
        assert_eq!(c.tree.connection(c.solta).unwrap().rede, Rede::Herdar);
        // O que apontava para outro perfil fica como estava.
        assert_eq!(c.tree.connection(c.conexao).unwrap().rede, Rede::Perfil(b));
    }

    #[test]
    fn duplicar_copia_o_perfil_com_id_novo() {
        let mut c = cena();
        c.tree.perfil_mut(c.cliente_a).unwrap().subir = Some("docker start x".to_owned());
        let copia = c.tree.duplicar_perfil(c.cliente_a).unwrap();
        assert_ne!(copia, c.cliente_a);
        let p = c.tree.perfil(copia).unwrap();
        assert_eq!(p.name, "Cliente A (cópia)");
        assert_eq!(p.subir.as_deref(), Some("docker start x"));
        // A cópia fica logo depois do original.
        assert_eq!(c.tree.redes[1].id, copia);
        assert_eq!(c.tree.duplicar_perfil(12345), None);
    }

    #[test]
    fn o_id_do_perfil_vem_do_next_id_da_arvore() {
        let mut tree = Tree::default();
        let a = tree.add_perfil(PerfilRede::new("A"));
        let conexao = tree.add_connection(None, Connection::new("c", Kind::Ssh));
        let b = tree.add_perfil(PerfilRede::new("B"));
        assert_eq!([a, conexao, b], [1, 2, 3]);
    }

    #[test]
    fn json_antigo_sem_redes_abre_com_os_padroes() {
        let json = r#"{"version":1,"next_id":3,"nodes":[
            {"Group":{"id":1,"name":"G","children":[
                {"Connection":{"id":2,"name":"web","host":"h"}}]}}]}"#;
        let tree: Tree = serde_json::from_str(json).unwrap();
        assert!(tree.redes.is_empty());
        assert_eq!(tree.connection(2).unwrap().rede, Rede::Herdar);
        assert_eq!(nome(&tree, 2), None);
    }

    #[test]
    fn ida_e_volta_pelo_json() {
        let mut c = cena();
        {
            let p = c.tree.perfil_mut(c.cliente_a).unwrap();
            p.cor = 3;
            p.proxy = Some(ProxyRede {
                credencial: Some(7),
                ..ProxyRede::novo()
            });
            p.salto = Some(Jump {
                host: "bastion".to_owned(),
                port: 22,
                user: "ana".to_owned(),
                credencial: None,
            });
            p.subir = Some("docker start rede-cliente-a".to_owned());
            p.descer = Some("docker stop rede-cliente-a".to_owned());
            p.espera_secs = 12;
        }
        grupo_mut(&mut c.tree, c.clientes).rede = Rede::Perfil(c.cliente_a);
        c.tree.connection_mut(c.solta).unwrap().rede = Rede::Direta;

        let json = serde_json::to_string_pretty(&c.tree).unwrap();
        let volta: Tree = serde_json::from_str(&json).unwrap();
        assert_eq!(volta, c.tree);
        assert_eq!(volta.version, crate::modelo::VERSAO);
        assert_eq!(volta.perfil(c.cliente_a).unwrap().nome_da_cor(), "Roxo");
    }

    #[test]
    fn o_known_hosts_fica_na_pasta_do_perfil() {
        let pasta = Path::new("/dados/tirech-omni");
        assert_eq!(
            known_hosts_do_perfil(pasta, 4),
            Path::new("/dados/tirech-omni/redes/4/known_hosts")
        );
    }
}

#[cfg(all(test, unix))]
mod tests_preparar {
    use std::net::TcpListener;
    use std::sync::atomic::AtomicU64;

    use super::*;

    /// Ids distintos por teste: a trava e o marcador são por perfil.
    fn id_novo() -> u64 {
        static N: AtomicU64 = AtomicU64::new(1000);
        N.fetch_add(1, Ordering::Relaxed)
    }

    fn pasta() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "tirech-omni-redes-teste-{}-{}",
            std::process::id(),
            id_novo()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Uma porta livre agora. Cada porta sai uma vez só no processo: o sistema devolve a mesma
    /// porta recém-liberada a quem pedir logo em seguida, e os testes rodam em paralelo.
    fn porta_livre() -> u16 {
        static DADAS: Mutex<Vec<u16>> = Mutex::new(Vec::new());
        loop {
            let porta = TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            let mut dadas = DADAS.lock().unwrap();
            if !dadas.contains(&porta) {
                dadas.push(porta);
                return porta;
            }
        }
    }

    fn perfil(porta: u16, subir: Option<String>) -> PerfilRede {
        PerfilRede {
            id: id_novo(),
            proxy: Some(ProxyRede {
                port: porta,
                ..ProxyRede::novo()
            }),
            subir,
            espera_secs: 5,
            ..PerfilRede::new("Cliente A")
        }
    }

    fn marcador(pasta: &Path) -> (PathBuf, String) {
        let arquivo = pasta.join("subiu");
        let comando = format!("touch '{}'", arquivo.display());
        (arquivo, comando)
    }

    #[test]
    fn sem_proxy_nao_faz_nada() {
        let dir = pasta();
        let (arquivo, cmd) = marcador(&dir);
        let mut p = perfil(1, Some(cmd));
        p.proxy = None;
        preparar(&p, &mut |_| panic!("sem aviso")).unwrap();
        assert!(!arquivo.exists());
    }

    #[test]
    fn porta_aberta_nao_roda_o_comando() {
        let dir = pasta();
        let (arquivo, cmd) = marcador(&dir);
        let escuta = TcpListener::bind("127.0.0.1:0").unwrap();
        let p = perfil(escuta.local_addr().unwrap().port(), Some(cmd));
        preparar(&p, &mut |_| panic!("sem aviso")).unwrap();
        assert!(!arquivo.exists());
    }

    #[test]
    fn porta_fechada_roda_o_comando_e_espera() {
        let dir = pasta();
        let (arquivo, cmd) = marcador(&dir);
        let porta = porta_livre();
        let p = perfil(porta, Some(cmd));
        // A "rede" sobe depois do comando: a porta só abre quando o marcador aparece.
        let marca = arquivo.clone();
        let rede = std::thread::spawn(move || {
            while !marca.exists() {
                std::thread::sleep(Duration::from_millis(20));
            }
            std::thread::sleep(Duration::from_millis(400));
            let escuta = TcpListener::bind(("127.0.0.1", porta)).unwrap();
            std::thread::sleep(Duration::from_secs(3));
            drop(escuta);
        });
        let mut avisos = Vec::new();
        preparar(&p, &mut |a| avisos.push(a.to_owned())).unwrap();
        assert!(arquivo.exists());
        assert_eq!(
            avisos,
            ["Subindo a rede Cliente A…", "Rede Cliente A pronta."]
        );
        rede.join().unwrap();
    }

    #[test]
    fn codigo_diferente_de_zero_vira_erro_com_a_ultima_linha_do_stderr() {
        let p = perfil(
            porta_livre(),
            Some("echo primeira >&2; echo 'docker: sem permissão' >&2; exit 1".to_owned()),
        );
        let erro = preparar(&p, &mut |_| {}).unwrap_err();
        assert_eq!(erro, "a rede Cliente A não subiu: docker: sem permissão");
    }

    #[test]
    fn codigo_sem_stderr_diz_o_codigo() {
        let p = perfil(porta_livre(), Some("exit 3".to_owned()));
        let erro = preparar(&p, &mut |_| {}).unwrap_err();
        assert_eq!(erro, "a rede Cliente A não subiu (código 3)");
    }

    #[test]
    fn tempo_esgotado_diz_o_endereco() {
        let porta = porta_livre();
        let mut p = perfil(porta, Some("true".to_owned()));
        p.espera_secs = 1;
        let inicio = Instant::now();
        let erro = preparar(&p, &mut |_| {}).unwrap_err();
        assert_eq!(
            erro,
            format!("a rede Cliente A não respondeu em 1 s (127.0.0.1:{porta})")
        );
        assert!(inicio.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn duas_chamadas_ao_mesmo_tempo_rodam_o_comando_uma_vez() {
        let dir = pasta();
        let contagem = dir.join("contagem");
        let porta = porta_livre();
        // Cada execução do comando anota uma linha; a porta abre logo depois da primeira.
        let cmd = format!("echo x >> '{}'; sleep 0.3", contagem.display());
        let p = perfil(porta, Some(cmd));
        let marca = contagem.clone();
        let rede = std::thread::spawn(move || {
            while !marca.exists() {
                std::thread::sleep(Duration::from_millis(10));
            }
            let escuta = TcpListener::bind(("127.0.0.1", porta)).unwrap();
            std::thread::sleep(Duration::from_secs(3));
            drop(escuta);
        });
        let a = p.clone();
        let b = p.clone();
        let ta = std::thread::spawn(move || preparar(&a, &mut |_| {}));
        let tb = std::thread::spawn(move || preparar(&b, &mut |_| {}));
        ta.join().unwrap().unwrap();
        tb.join().unwrap().unwrap();
        rede.join().unwrap();
        let linhas = std::fs::read_to_string(&contagem).unwrap();
        assert_eq!(linhas.lines().count(), 1);
    }

    #[test]
    fn o_perfil_so_sai_quando_nenhum_uso_resta() {
        let usos = Usos::default();
        let p = perfil(1, None);
        assert!(usos.atualizar(&[p.clone(), p.clone()]).is_empty());
        assert_eq!(usos.em_uso().len(), 1);
        // Uma das duas abas fechou: o perfil continua em uso.
        assert!(usos.atualizar(std::slice::from_ref(&p)).is_empty());
        let saiu = usos.atualizar(&[]);
        assert_eq!(saiu.iter().map(|x| x.id).collect::<Vec<_>>(), [p.id]);
        assert!(usos.em_uso().is_empty());
        // Sem uso de novo: nada a devolver.
        assert!(usos.atualizar(&[]).is_empty());
    }

    #[test]
    fn perfil_sem_proxy_nao_conta_como_uso() {
        let usos = Usos::default();
        let mut p = perfil(1, None);
        p.proxy = None;
        assert!(usos.atualizar(&[p]).is_empty());
        assert!(usos.em_uso().is_empty());
    }

    #[test]
    fn perfil_nebula_conta_como_uso_mesmo_sem_proxy() {
        let usos = Usos::default();
        let mut p = perfil(1, None);
        p.proxy = None;
        p.nebula = Some(NebulaRede::default());
        assert!(usos.atualizar(std::slice::from_ref(&p)).is_empty());
        assert_eq!(usos.em_uso().len(), 1);
        let saiu = usos.atualizar(&[]);
        assert_eq!(saiu.iter().map(|x| x.id).collect::<Vec<_>>(), [p.id]);
    }

    #[test]
    fn o_perfil_antigo_sem_nebula_continua_lendo_e_nao_grava_o_campo() {
        let antigo = r#"{"id":3,"name":"Cliente A","cor":0}"#;
        let perfil: PerfilRede = serde_json::from_str(antigo).unwrap();
        assert!(perfil.nebula.is_none() && !perfil.e_nebula());
        assert!(!serde_json::to_string(&perfil).unwrap().contains("nebula"));
    }

    #[test]
    fn o_perfil_nebula_vai_e_volta_sem_segredo() {
        let mut p = PerfilRede::new("Cliente A");
        p.nebula = Some(NebulaRede {
            config: "cipher: aes\n".into(),
            identidade: Some(9),
            origem: "config.yml".into(),
        });
        let texto = serde_json::to_string(&p).unwrap();
        assert!(!texto.contains("BEGIN"));
        let volta: PerfilRede = serde_json::from_str(&texto).unwrap();
        assert_eq!(volta, p);
    }

    #[test]
    fn sem_o_motor_a_rede_nebula_nao_conecta_direto() {
        let mut p = perfil(1, None);
        p.nebula = Some(NebulaRede::default());
        let rede = RedeResolvida {
            perfil: p,
            proxy_usuario: None,
            proxy_senha: None,
            salto_senha: None,
            known_hosts: None,
            nebula: None,
            nebula_erro: Some("o cofre está trancado".into()),
            openvpn: None,
            openvpn_erro: None,
            motor: None,
            motor_erro: None,
        };
        let mut avisos = Vec::new();
        let erro = rede
            .preparar(&mut |a| avisos.push(a.to_owned()))
            .unwrap_err();
        assert_eq!(erro, "o cofre está trancado");
        assert!(avisos.is_empty());
        let texto = format!("{rede:?}");
        assert!(texto.contains("o cofre está trancado"));
    }

    #[test]
    fn o_descer_usa_o_perfil_de_quando_o_uso_comecou() {
        let usos = Usos::default();
        let mut p = perfil(1, None);
        p.descer = Some("docker stop antigo".to_owned());
        usos.atualizar(std::slice::from_ref(&p));
        p.descer = Some("docker stop novo".to_owned());
        usos.atualizar(std::slice::from_ref(&p));
        let saiu = usos.atualizar(&[]);
        assert_eq!(saiu[0].descer.as_deref(), Some("docker stop antigo"));
    }

    #[test]
    fn descer_roda_o_comando_numa_thread() {
        let dir = pasta();
        let (arquivo, cmd) = marcador(&dir);
        let mut p = perfil(1, None);
        p.descer = Some(cmd);
        descer(&p);
        let limite = Instant::now() + Duration::from_secs(5);
        while !arquivo.exists() && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(arquivo.exists());
    }
}

#[cfg(test)]
mod tests_rotulos {
    use super::*;
    use crate::modelo::{Connection, Kind};

    fn arvore() -> (Tree, u64, u64, u64) {
        let mut tree = Tree::default();
        let a = tree.add_perfil(PerfilRede::new("Cliente A"));
        let grupo = tree.add_group(None, "Clientes");
        let conexao = tree.add_connection(Some(grupo), Connection::new("web", Kind::Ssh));
        if let Some(Node::Group(g)) = tree.find_mut(grupo) {
            g.rede = Rede::Perfil(a);
        }
        (tree, a, grupo, conexao)
    }

    #[test]
    fn herdar_mostra_a_rede_do_grupo_de_cima() {
        let (mut tree, a, grupo, conexao) = arvore();
        assert_eq!(rotulo_de_herdar(&tree, conexao), "Herdar (Cliente A)");
        // O grupo, na raiz, herda a direta.
        assert_eq!(rotulo_de_herdar(&tree, grupo), "Herdar (Direta)");
        assert_eq!(
            rotulo_da_rede(&tree, grupo, Rede::Perfil(a)),
            "Cliente A".to_owned()
        );
        // Perfil apagado: a linha mostra "Direta", como a conexão sai.
        tree.apagar_perfil(a);
        assert_eq!(rotulo_de_herdar(&tree, conexao), "Herdar (Direta)");
        assert_eq!(rotulo_da_rede(&tree, conexao, Rede::Perfil(a)), "Direta");
    }

    #[test]
    fn as_opcoes_vao_de_herdar_a_cada_perfil() {
        let (mut tree, _, _, conexao) = arvore();
        tree.add_perfil(PerfilRede::new("Cliente B"));
        assert_eq!(
            opcoes_de_rede(&tree, conexao),
            ["Herdar (Cliente A)", "Direta", "Cliente A", "Cliente B"]
        );
    }

    #[test]
    fn o_texto_volta_a_ser_a_rede() {
        let (tree, a, _, conexao) = arvore();
        let texto = |t: &str| rede_do_rotulo(&tree, t);
        assert_eq!(texto("Direta"), Some(Rede::Direta));
        assert_eq!(texto(&rotulo_de_herdar(&tree, conexao)), Some(Rede::Herdar));
        // O texto de "Herdar" muda com a árvore, e continua valendo.
        assert_eq!(texto("Herdar (Outra)"), Some(Rede::Herdar));
        assert_eq!(texto("Cliente A"), Some(Rede::Perfil(a)));
        assert_eq!(texto("Cliente Z"), None);
    }

    #[test]
    fn o_nome_do_perfil_nao_repete_nem_imita_uma_opcao() {
        let (tree, a, _, _) = arvore();
        assert_eq!(nome_livre(&tree, "Cliente B", None), "Cliente B");
        assert_eq!(nome_livre(&tree, "Cliente A", None), "Cliente A (2)");
        // Renomear para o próprio nome não conta como repetido.
        assert_eq!(nome_livre(&tree, "Cliente A", Some(a)), "Cliente A");
        assert_eq!(nome_livre(&tree, "Direta", None), "Rede Direta");
        assert_eq!(nome_livre(&tree, "Herdar (x)", None), "Rede Herdar (x)");
        assert_eq!(nome_livre(&tree, "Herdar tudo", None), "Herdar tudo");
        assert_eq!(nome_livre(&tree, "  ", None), "Rede");
    }
}
