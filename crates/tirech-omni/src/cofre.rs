//! O cofre de senhas: itens (senhas, chaves, notas) em pastas, cifrados com AES-256-GCM num
//! `vault.json`. A chave é um arquivo `vault.key` na mesma pasta; quem quiser define uma senha
//! mestra, e então a chave sai do Argon2id da senha e o arquivo `vault.key` deixa de existir.
//!
//! O cofre anda entre máquinas (M44): cada item e cada pasta tem um `uid` que não muda, e
//! [`Cofre::juntar`] casa duas cópias por ele. O [`Cofre::gravar`] confere o arquivo antes de
//! gravar, e junta o que outra máquina mudou no meio do caminho.
//!
//! A senha em claro só vive na memória: nunca vai para o `connections.json`, o `estado.json` ou
//! uma mensagem de erro.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use aes_gcm::aead::consts::U12;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use tomni_ui::clipboard;
use tomni_ui::prelude::*;

use crate::armazenamento::{
    carimbo, conferir_gravacao, copia_do_dia, copia_do_dia_existe, gravar_atomico,
    guardar_ilegivel, so_leitura, sufixo_da_copia,
};
use crate::busca::normalizar;
use crate::configuracoes::Configuracoes;
use crate::gerador::Gerador;
use crate::relogio::Alarmes;
use crate::t;

/// A versão do formato do `vault.json`: a 7 tem o item de identidade ZeroTier (M61); a 6 tem os
/// itens de identidade WireGuard e Tailscale (M57); a 5 tem o de identidade OpenVPN (M56); a 4 tem o de identidade Nebula (M48); a 3 tem
/// `uid` nos itens e nas pastas, as lápides dos removidos e o item de chave SSH; a 2 tem pastas; a
/// 1 (itens com a pasta em texto) ainda se lê. Da 3 para a 4, da 4 para a 5, da 5 para a 6 e da 6 para a 7
/// não há migração: o campo novo é opcional.
const VERSAO: u32 = 7;

/// Quanto tempo uma lápide (o registro de que algo foi apagado) fica no cofre: o que uma
/// máquina desligada por menos tempo que isso ainda apaga ao juntar.
const VIDA_DA_LAPIDE_SECS: u64 = 90 * 24 * 3600;

/// O arquivo do cofre, na pasta de configuração.
const ARQUIVO: &str = "vault.json";
/// O arquivo da chave (sem senha mestra).
const ARQUIVO_CHAVE: &str = "vault.key";
/// Os nomes que o cofre tinha antes do M39.
const ARQUIVO_ANTIGO: &str = "cofre.json";
const ARQUIVO_CHAVE_ANTIGO: &str = "chave";

/// O texto que o `verificador` cifra, para conferir a senha mestra sem decifrar a lista.
///
/// Identificador de formato: não muda com o nome do app (está gravado nos arquivos de quem usa).
pub(crate) const VERIFICADOR: &[u8] = b"tirech-ssh";

/// O tipo de um item do cofre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tipo {
    /// Uma senha de site ou de serviço.
    #[default]
    Senha,
    /// Um servidor SSH.
    Ssh,
    /// Um servidor SFTP.
    Sftp,
    /// Um servidor FTP.
    Ftp,
    /// Um bucket S3 (ou compatível).
    S3,
    /// Um remote do rclone.
    Rclone,
    /// A passphrase de uma chave privada que fica em arquivo.
    Chave,
    /// Uma chave SSH guardada no cofre (M44).
    ChaveSsh,
    /// Uma nota solta.
    Nota,
    /// A identidade de um nó da rede Nebula: a CA, o certificado e a chave (M48).
    Nebula,
    /// A identidade de um cliente OpenVPN: CA, certificado, chave, chave TLS e senha (M56).
    OpenVpn,
    /// A identidade de um nó WireGuard: a chave privada e as chaves pré-compartilhadas (M57).
    WireGuard,
    /// O estado de uma máquina da tailnet (Tailscale ou Headscale): as chaves e a chave de
    /// autenticação (M57).
    Tailscale,
    /// A identidade e o estado de um membro de rede ZeroTier: o `identity.secret` (M61).
    ZeroTier,
}

impl Tipo {
    /// Todos os tipos, na ordem da lista de escolha.
    pub const TODOS: [Tipo; 14] = [
        Tipo::Senha,
        Tipo::Ssh,
        Tipo::Sftp,
        Tipo::Ftp,
        Tipo::S3,
        Tipo::Rclone,
        Tipo::Chave,
        Tipo::ChaveSsh,
        Tipo::Nota,
        Tipo::Nebula,
        Tipo::OpenVpn,
        Tipo::WireGuard,
        Tipo::Tailscale,
        Tipo::ZeroTier,
    ];

    /// O texto do tipo na tela.
    pub fn nome(self) -> String {
        match self {
            Tipo::Senha => t!("tipo-senha"),
            Tipo::Ssh => "SSH".to_owned(),
            Tipo::Sftp => "SFTP".to_owned(),
            Tipo::Ftp => "FTP".to_owned(),
            Tipo::S3 => "S3".to_owned(),
            Tipo::Rclone => "rclone".to_owned(),
            Tipo::Chave => t!("tipo-passphrase-de-chave"),
            Tipo::ChaveSsh => t!("tipo-chave-ssh"),
            Tipo::Nota => t!("tipo-nota"),
            Tipo::Nebula => t!("tipo-nebula"),
            Tipo::OpenVpn => t!("tipo-openvpn"),
            Tipo::WireGuard => t!("tipo-wireguard"),
            Tipo::Tailscale => t!("tipo-tailscale"),
            Tipo::ZeroTier => t!("tipo-zerotier"),
        }
    }

    /// O tipo que tem esse texto de tela.
    pub fn do_nome(nome: &str) -> Option<Tipo> {
        Tipo::TODOS.into_iter().find(|t| t.nome() == nome)
    }

    /// O id estável do tipo nos arquivos exportados (`Senha`, `Ssh`…).
    pub fn id(self) -> String {
        format!("{self:?}")
    }

    /// O tipo que tem esse id.
    pub fn do_id(id: &str) -> Option<Tipo> {
        Tipo::TODOS.into_iter().find(|t| t.id() == id)
    }
}

/// O campo extra que guarda o tipo do item nos formatos de fora (CSV e JSON do Bitwarden).
///
/// Identificador de formato: não muda com o nome do app (está gravado nos arquivos de quem usa).
pub const CAMPO_TIPO: &str = "tirech-ssh.tipo";

/// Os campos de uma nota do Bitwarden que levam a identidade Nebula (M48): a CA, o certificado e a
/// chave privada (esta oculta). Nomes fixos, para a volta não depender do idioma.
pub const CAMPO_NEBULA_CA: &str = "tirech-ssh.nebula.ca";
/// Ver [`CAMPO_NEBULA_CA`].
pub const CAMPO_NEBULA_CERTIFICADO: &str = "tirech-ssh.nebula.certificado";
/// Ver [`CAMPO_NEBULA_CA`].
pub const CAMPO_NEBULA_CHAVE: &str = "tirech-ssh.nebula.chave";

/// Os campos de uma nota do Bitwarden que levam a identidade OpenVPN (M56). A chave privada e a
/// chave TLS ficam em campos ocultos; o usuário e a senha, nos campos do login.
pub const CAMPO_OPENVPN_CA: &str = "tirech-ssh.openvpn.ca";
/// Ver [`CAMPO_OPENVPN_CA`].
pub const CAMPO_OPENVPN_CERTIFICADO: &str = "tirech-ssh.openvpn.certificado";
/// Ver [`CAMPO_OPENVPN_CA`].
pub const CAMPO_OPENVPN_CHAVE: &str = "tirech-ssh.openvpn.chave";
/// Ver [`CAMPO_OPENVPN_CA`].
pub const CAMPO_OPENVPN_CHAVE_TLS: &str = "tirech-ssh.openvpn.chave-tls";
/// Ver [`CAMPO_OPENVPN_CA`]: `tls-auth`, `tls-crypt` ou `tls-crypt-v2`.
pub const CAMPO_OPENVPN_MODO_TLS: &str = "tirech-ssh.openvpn.modo-tls";
/// Ver [`CAMPO_OPENVPN_CA`]: o `key-direction` (0 ou 1), quando há.
pub const CAMPO_OPENVPN_DIRECAO: &str = "tirech-ssh.openvpn.direcao";
/// Ver [`CAMPO_OPENVPN_CA`]: o servidor, para mostrar.
pub const CAMPO_OPENVPN_SERVIDOR: &str = "tirech-ssh.openvpn.servidor";

/// Os campos de uma nota do Bitwarden que levam a identidade WireGuard (M57). A chave privada e as
/// chaves pré-compartilhadas ficam em campos ocultos; cada chave pré-compartilhada tem o nome
/// `tirech-ssh.wireguard.psk.` seguido da chave pública do par.
pub const CAMPO_WIREGUARD_CHAVE: &str = "tirech-ssh.wireguard.chave";
/// Ver [`CAMPO_WIREGUARD_CHAVE`]: a chave pública do nó, para mostrar.
pub const CAMPO_WIREGUARD_PUBLICA: &str = "tirech-ssh.wireguard.publica";
/// Ver [`CAMPO_WIREGUARD_CHAVE`]: os endereços do nó, separados por vírgula, para mostrar.
pub const CAMPO_WIREGUARD_ENDERECOS: &str = "tirech-ssh.wireguard.enderecos";
/// Ver [`CAMPO_WIREGUARD_CHAVE`]: o começo do nome do campo de cada chave pré-compartilhada.
pub const CAMPO_WIREGUARD_PSK: &str = "tirech-ssh.wireguard.psk.";

/// Os campos de uma nota do Bitwarden que levam o estado da máquina da tailnet (M57). As chaves e
/// a chave de autenticação ficam em campos ocultos.
pub const CAMPO_TAILSCALE_SERVIDOR: &str = "tirech-ssh.tailscale.servidor";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: a chave da máquina (`privkey:…`).
pub const CAMPO_TAILSCALE_CHAVE_MAQUINA: &str = "tirech-ssh.tailscale.chave-maquina";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: a chave do nó.
pub const CAMPO_TAILSCALE_CHAVE_NO: &str = "tirech-ssh.tailscale.chave-no";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: quando a chave do nó vence (segundos desde a época UNIX).
pub const CAMPO_TAILSCALE_VENCE_EM: &str = "tirech-ssh.tailscale.vence-em";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: a chave de autenticação (`tskey-auth-…`).
pub const CAMPO_TAILSCALE_CHAVE_AUTENTICACAO: &str = "tirech-ssh.tailscale.chave-autenticacao";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: o login que o controle conhece.
pub const CAMPO_TAILSCALE_LOGIN: &str = "tirech-ssh.tailscale.login";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: o domínio da tailnet.
pub const CAMPO_TAILSCALE_TAILNET: &str = "tirech-ssh.tailscale.tailnet";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: o nome da máquina na tailnet.
pub const CAMPO_TAILSCALE_NOME: &str = "tirech-ssh.tailscale.nome";
/// Ver [`CAMPO_TAILSCALE_SERVIDOR`]: os endereços da máquina, separados por vírgula.
pub const CAMPO_TAILSCALE_ENDERECOS: &str = "tirech-ssh.tailscale.enderecos";

/// Os campos de uma nota do Bitwarden que levam a identidade e o estado de um membro ZeroTier
/// (M61). O `identity.secret` e o estado ficam em campos ocultos (o estado tem a configuração
/// assinada da rede e os caminhos dos pares).
pub const CAMPO_ZEROTIER_IDENTIDADE: &str = "tirech-ssh.zerotier.identidade";
/// Ver [`CAMPO_ZEROTIER_IDENTIDADE`]: o estado do nó (JSON em base64).
pub const CAMPO_ZEROTIER_ESTADO: &str = "tirech-ssh.zerotier.estado";
/// Ver [`CAMPO_ZEROTIER_IDENTIDADE`]: o endereço do membro (10 hexadecimais), para mostrar.
pub const CAMPO_ZEROTIER_ENDERECO: &str = "tirech-ssh.zerotier.endereco";
/// Ver [`CAMPO_ZEROTIER_IDENTIDADE`]: o nome da rede, para mostrar.
pub const CAMPO_ZEROTIER_NOME_REDE: &str = "tirech-ssh.zerotier.nome-rede";
/// Ver [`CAMPO_ZEROTIER_IDENTIDADE`]: os endereços do membro na rede, separados por vírgula.
pub const CAMPO_ZEROTIER_ENDERECOS: &str = "tirech-ssh.zerotier.enderecos";

/// O nome do campo extra que marca o item da chave dos logs cifrados (M53) e guarda a pública
/// dela, em base64. A privada é a senha do item.
///
/// Identificador de formato: não muda com o nome do app (está gravado nos arquivos de quem usa).
pub const CAMPO_LOGS: &str = "tirech-ssh.logs";

/// O nome do campo extra do token de sessão de um item `S3` (chaves temporárias da AWS).
pub const CAMPO_TOKEN: &str = "Token de sessão";

/// Um campo a mais de um item (o que não cabe nos campos fixos: a região do S3, o remote do
/// rclone…).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Campo {
    /// O nome do campo.
    pub nome: String,
    /// O valor.
    pub valor: String,
    /// Se a tela esconde o valor.
    pub oculto: bool,
}

/// Uma chave dos logs cifrados guardada no cofre (M53).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChaveDosLogs {
    /// O id da chave (8 bytes do SHA-256 da pública): o que o cabeçalho do log traz.
    pub id: [u8; 8],
    /// A privada X25519.
    pub privada: [u8; 32],
    /// A pública X25519.
    pub publica: [u8; 32],
    /// O `uid` do item, que o `.pub` cita.
    pub uid: String,
    /// Quando o item foi criado (segundos desde a época UNIX).
    pub criado: u64,
}

impl ChaveDosLogs {
    /// A chave de um item; `None` se o item não é uma chave dos logs, ou está mexido.
    fn do_item(item: &Item) -> Option<Self> {
        let campo = item.campos.iter().find(|c| c.nome == CAMPO_LOGS)?;
        let privada: [u8; 32] = STANDARD.decode(item.senha.trim()).ok()?.try_into().ok()?;
        let publica: [u8; 32] = STANDARD.decode(campo.valor.trim()).ok()?.try_into().ok()?;
        if crate::log_cifrado::publica_da(&privada) != publica {
            return None;
        }
        Some(ChaveDosLogs {
            id: crate::log_cifrado::id_da_chave(&publica),
            privada,
            publica,
            uid: item.uid.clone(),
            criado: item.criado,
        })
    }
}

/// Uma pasta do cofre. A raiz não é uma pasta: é `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pasta {
    /// O id, do mesmo contador dos itens, nunca reaproveitado.
    pub id: u64,
    /// O identificador que não muda entre máquinas: casa as cópias do cofre ao juntar.
    pub uid: String,
    /// O nome (sem `/`).
    pub nome: String,
    /// A pasta mãe; `None` é a raiz.
    pub pai: Option<u64>,
    /// Quando o nome ou a pasta mãe mudou pela última vez (segundos desde a época UNIX); a cópia
    /// mais nova vence ao juntar.
    pub alterado: u64,
}

/// Uma chave SSH guardada no cofre: a fonte única das chaves do app.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ChaveSsh {
    /// A privada em OpenSSH, **sem passphrase** (o cofre já cifra). Vazia: só a pública (a chave
    /// de outra pessoa, para o `authorized_keys`).
    pub privada: String,
    /// A linha OpenSSH da pública, com o comentário.
    pub publica: String,
    /// O certificado (`…-cert-v01@openssh.com …`), se houver.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub certificado: String,
    /// `SHA256:…`, para achar a chave sem abrir a privada.
    pub fingerprint: String,
    /// Carregar no agente ao destravar o cofre.
    pub no_agente: bool,
    /// Cada uso pede confirmação (`ssh-add -c`).
    pub confirmar: bool,
    /// O agente a esquece depois de tantos segundos (0: nunca, como `ssh-add -t`).
    pub vida_secs: u64,
}

impl Default for ChaveSsh {
    fn default() -> Self {
        Self {
            privada: String::new(),
            publica: String::new(),
            certificado: String::new(),
            fingerprint: String::new(),
            no_agente: true,
            confirmar: false,
            vida_secs: 0,
        }
    }
}

impl ChaveSsh {
    /// Uma chave do cofre a partir de uma privada já aberta: a privada em OpenSSH sem passphrase,
    /// a linha pública e o fingerprint. A política do agente fica no padrão.
    pub fn de_chave(chave: &ssh_key::PrivateKey) -> Result<Self, String> {
        use tomni_ui::terminal::ssh::keys::{self, KdfLevel, PrivateFormat};
        let privada = keys::export_private(chave, PrivateFormat::OpenSsh, None, KdfLevel::Standard)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            privada: privada.to_string(),
            publica: keys::public_openssh(chave.public_key()),
            fingerprint: keys::fingerprint(chave.public_key()),
            ..Self::default()
        })
    }

    /// Se é uma chave de token (FIDO, `sk-…`): a privada é só um "key handle", e quem assina é o
    /// token, então ela não vai para o agente.
    pub fn e_token(&self) -> bool {
        self.publica.trim_start().starts_with("sk-")
    }

    /// A chave privada aberta (a que está no cofre não tem passphrase). `None`: só a pública, ou o
    /// texto não é uma chave que o app leia.
    pub fn chave_privada(&self) -> Option<ssh_key::PrivateKey> {
        if self.privada.trim().is_empty() {
            return None;
        }
        tomni_ui::terminal::ssh::keys::KeyFile::from_text(&self.privada, None)
            .ok()
            .map(|k| k.private_key().clone())
    }
}

/// A identidade de um nó da rede Nebula (M48): o que o `pki` do `config.yml` aponta, guardado
/// cifrado no cofre em vez de em arquivos soltos.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdentidadeNebula {
    /// As CAs em PEM (o `pki.ca`).
    pub ca: String,
    /// O certificado do nó em PEM (v1, v2 ou os dois).
    pub certificado: String,
    /// A chave privada em PEM.
    pub chave: String,
    /// O nome do certificado, para mostrar sem abrir o PEM (preenchido na importação).
    pub nome: String,
    /// O endereço com o prefixo (`10.42.0.7/24`).
    pub endereco: String,
    /// Os grupos do certificado.
    pub grupos: Vec<String>,
    /// Quando o certificado vence (segundos desde a época UNIX; 0: sem validade).
    pub vence_em: u64,
    /// A impressão digital do certificado (a do v2, se houver os dois).
    pub impressao: String,
}

// A chave privada não aparece em `{:?}` nem em log.
impl std::fmt::Debug for IdentidadeNebula {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentidadeNebula")
            .field("nome", &self.nome)
            .field("endereco", &self.endereco)
            .field("impressao", &self.impressao)
            .field("chave", &"***")
            .finish_non_exhaustive()
    }
}

/// A identidade de um cliente OpenVPN (M56): o que é segredo no `.ovpn` (a CA, o certificado e a
/// chave do cliente e a chave estática do `tls-auth`/`tls-crypt`), guardado cifrado no cofre. O
/// usuário e a senha do `auth-user-pass` ficam nos campos `usuario` e `senha` do próprio item. O
/// OTP e as respostas de desafio não entram: valem uma vez.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdentidadeOpenVpn {
    /// A CA em PEM (`ca`).
    pub ca: String,
    /// O certificado do cliente em PEM, seguido dos intermediários (`extra-certs`).
    pub certificado: String,
    /// A chave privada do cliente em PEM.
    pub chave: String,
    /// A chave estática (`tls-auth`, `tls-crypt`) ou a de cliente (`tls-crypt-v2`), como veio.
    pub chave_tls: String,
    /// `tls-auth`, `tls-crypt` ou `tls-crypt-v2`; vazio sem chave.
    pub modo_tls: String,
    /// O `key-direction` do `.ovpn` (0 ou 1); `None` é bidirecional.
    pub key_direction: Option<u8>,
    /// O nome do certificado do cliente (CN), para mostrar sem abrir o PEM.
    pub cn: String,
    /// Quando o certificado vence (segundos desde a época UNIX; 0: sem validade).
    pub vence_em: u64,
    /// A impressão digital (SHA-256) do certificado do cliente.
    pub impressao: String,
    /// O primeiro `remote` do `.ovpn` (`vpn.exemplo.com:1194/udp`), para mostrar.
    pub servidor: String,
}

// A chave e a chave TLS não aparecem em `{:?}` nem em log.
impl std::fmt::Debug for IdentidadeOpenVpn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentidadeOpenVpn")
            .field("cn", &self.cn)
            .field("servidor", &self.servidor)
            .field("impressao", &self.impressao)
            .field("chave", &"***")
            .field("chave_tls", &"***")
            .finish_non_exhaustive()
    }
}

/// A identidade de um nó WireGuard (M57): o que é segredo no `.conf` (a `PrivateKey` do nó e as
/// `PresharedKey` dos pares), guardado cifrado no cofre. O resto do `.conf` fica no perfil.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdentidadeWireGuard {
    /// A chave privada do nó, em base64.
    pub chave_privada: String,
    /// A chave pública que ela gera, em base64 (o outro lado põe no `[Peer]`).
    pub chave_publica: String,
    /// As chaves pré-compartilhadas: a chave pública do par e a `PresharedKey` em base64.
    pub psks: Vec<(String, String)>,
    /// Os endereços do nó dentro da rede (`10.8.0.2/24`), para mostrar.
    pub enderecos: Vec<String>,
}

// A chave privada e as pré-compartilhadas não aparecem em `{:?}` nem em log.
impl std::fmt::Debug for IdentidadeWireGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentidadeWireGuard")
            .field("chave_publica", &self.chave_publica)
            .field("enderecos", &self.enderecos)
            .field("chave_privada", &"***")
            .field("psks", &self.psks.len())
            .finish()
    }
}

/// O estado de uma máquina da tailnet (M57), Tailscale ou Headscale: o que o motor precisa para
/// continuar sendo a mesma máquina e o que a pessoa guardou para entrar sem o navegador.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdentidadeTailscale {
    /// O servidor de controle a que este estado pertence (vazio: o oficial). Trocar o servidor do
    /// perfil descarta o estado.
    pub servidor: String,
    /// A chave da máquina (`privkey:…`); gerada na primeira vez.
    pub chave_maquina: String,
    /// A chave do nó (a do WireGuard); gerada na primeira vez.
    pub chave_no: String,
    /// Quando a chave do nó vence (segundos desde a época UNIX; 0: sem validade).
    pub vence_em: u64,
    /// A chave de autenticação (`tskey-auth-…`), se a pessoa guardou uma.
    pub chave_autenticacao: String,
    /// O login que o controle conhece (o e-mail, em geral).
    pub login: String,
    /// O domínio da tailnet (`minha-rede.ts.net`).
    pub tailnet: String,
    /// O nome desta máquina na tailnet.
    pub nome: String,
    /// Os endereços desta máquina na tailnet.
    pub enderecos: Vec<String>,
}

// As chaves e a chave de autenticação não aparecem em `{:?}` nem em log.
impl std::fmt::Debug for IdentidadeTailscale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentidadeTailscale")
            .field("servidor", &self.servidor)
            .field("login", &self.login)
            .field("tailnet", &self.tailnet)
            .field("nome", &self.nome)
            .field("enderecos", &self.enderecos)
            .field("chave_maquina", &"***")
            .field("chave_no", &"***")
            .field("chave_autenticacao", &"***")
            .finish_non_exhaustive()
    }
}

/// A identidade e o estado de um membro de rede ZeroTier (M61): o que o motor precisa para
/// continuar sendo o mesmo membro (o endereço que o administrador autorizou) e para subir mesmo
/// com o controlador fora do ar.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct IdentidadeZeroTier {
    /// O `identity.secret` (`endereço:0:pública:privada`); vazio até o motor gerar a primeira
    /// vez.
    pub identidade: String,
    /// O endereço do membro (10 hexadecimais), para mostrar e copiar.
    pub endereco: String,
    /// O estado do nó sem a identidade (planet, moons, configuração assinada, caminhos), em JSON
    /// e base64. Vazio: o nó começa do planet embutido.
    pub estado: String,
    /// O nome da rede na última vez, para mostrar sem conectar.
    pub nome_rede: String,
    /// Os endereços do membro na rede na última vez.
    pub enderecos: Vec<String>,
}

// O `identity.secret` e o estado não aparecem em `{:?}` nem em log.
impl std::fmt::Debug for IdentidadeZeroTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentidadeZeroTier")
            .field("endereco", &self.endereco)
            .field("nome_rede", &self.nome_rede)
            .field("enderecos", &self.enderecos)
            .field("identidade", &"***")
            .field("estado", &self.estado.len())
            .finish()
    }
}

/// Um item do cofre.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Item {
    /// O id, estável e nunca reaproveitado (0: ainda não foi salvo).
    pub id: u64,
    /// O identificador que não muda entre máquinas (128 bits aleatórios em hexadecimal): casa as
    /// cópias do cofre ao juntar. Vazio até o primeiro `salvar`.
    pub uid: String,
    /// O tipo.
    pub tipo: Tipo,
    /// O título que a árvore mostra.
    pub titulo: String,
    /// A pasta; `None` é a raiz.
    pub pasta: Option<u64>,
    /// O usuário.
    pub usuario: String,
    /// A senha.
    pub senha: String,
    /// O host, a URL do site, o endpoint do S3…
    pub url: String,
    /// As notas.
    pub notas: String,
    /// O que não cabe nos campos acima.
    pub campos: Vec<Campo>,
    /// Quando foi criado (segundos desde a época UNIX).
    pub criado: u64,
    /// Quando mudou pela última vez (segundos desde a época UNIX).
    pub alterado: u64,
    /// A chave, quando o tipo é [`Tipo::ChaveSsh`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chave: Option<ChaveSsh>,
    /// A identidade, quando o tipo é [`Tipo::Nebula`] (M48).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nebula: Option<IdentidadeNebula>,
    /// A identidade, quando o tipo é [`Tipo::OpenVpn`] (M56).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openvpn: Option<IdentidadeOpenVpn>,
    /// A identidade, quando o tipo é [`Tipo::WireGuard`] (M57).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wireguard: Option<IdentidadeWireGuard>,
    /// O estado da máquina, quando o tipo é [`Tipo::Tailscale`] (M57).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tailscale: Option<IdentidadeTailscale>,
    /// A identidade e o estado do membro, quando o tipo é [`Tipo::ZeroTier`] (M61).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zerotier: Option<IdentidadeZeroTier>,
    /// Os arquivos anexados (M47). Cada um é um arquivo cifrado em `vault/<id>`, lido com a chave
    /// da ficha; duas fichas podem apontar para o mesmo `id` (Duplicar, cópia de conflito).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub anexos: Vec<Anexo>,
}

impl Item {
    /// Se o item é de um tipo de rede (Nebula, OpenVPN, WireGuard, Tailscale, ZeroTier) e guarda a
    /// identidade dele. Um item criado à mão no cofre com um desses tipos só tem os campos comuns.
    pub fn tem_identidade_de_rede(&self) -> bool {
        match self.tipo {
            Tipo::Nebula => self.nebula.is_some(),
            Tipo::OpenVpn => self.openvpn.is_some(),
            Tipo::WireGuard => self.wireguard.is_some(),
            Tipo::Tailscale => self.tailscale.is_some(),
            Tipo::ZeroTier => self.zerotier.is_some(),
            _ => false,
        }
    }
}

/// A ficha de um arquivo anexado a um item: o que é preciso para achá-lo, abri-lo e conferi-lo.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Anexo {
    /// O nome do arquivo em `vault/` (128 bits aleatórios em hexadecimal).
    pub id: String,
    /// O nome que a pessoa vê.
    pub nome: String,
    /// O tamanho do conteúdo, em bytes.
    pub tamanho: u64,
    /// O tipo (MIME), pela extensão do nome.
    pub tipo: String,
    /// O SHA-256 do conteúdo, em hexadecimal.
    pub sha256: String,
    /// A chave do arquivo (32 bytes aleatórios, em base64).
    pub chave: String,
    /// Quando foi anexado (segundos desde a época UNIX).
    pub criado: u64,
}

/// Dados cifrados: o nonce e o texto cifrado (com a etiqueta do GCM), ambos em base64.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segredo {
    /// O nonce de 96 bits.
    pub nonce: String,
    /// O texto cifrado.
    pub dados: String,
}

/// Como o cofre guarda a chave.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
enum Protecao {
    /// A chave é o arquivo `vault.key` da pasta.
    Chave,
    /// A chave sai do Argon2id da senha mestra.
    SenhaMestra {
        sal: String,
        m_kib: u32,
        t: u32,
        p: u32,
        verificador: Segredo,
    },
    /// A chave vai dentro do arquivo, em base64: é o `.tirech-vault` "levado" sem senha (M44).
    /// Quem tiver o arquivo abre o cofre.
    Embutida { chave: String },
}

/// O nome do formato do arquivo que leva o cofre a outra máquina.
const FORMATO_PORTATIL: &str = "tirech-vault";

/// O `.tirech-vault`: o mesmo `Arquivo` do `vault.json` com o nome do formato na frente.
#[derive(Serialize)]
struct Portatil<'a> {
    formato: &'static str,
    #[serde(flatten)]
    arquivo: &'a Arquivo,
}

/// O `vault.json`.
#[derive(Debug, Serialize, Deserialize)]
struct Arquivo {
    versao: u32,
    protecao: Protecao,
    itens: Segredo,
}

/// O registro de que um item ou uma pasta foi apagado: a lápide faz a remoção sobreviver à
/// junção com uma cópia que ainda tem o que foi apagado.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Removido {
    /// O `uid` do que foi apagado.
    pub uid: String,
    /// Quando (segundos desde a época UNIX).
    pub quando: u64,
}

/// O que vai cifrado dentro do `itens`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Conteudo {
    /// O contador de ids, dividido entre as pastas e os itens.
    proximo_id: u64,
    pastas: Vec<Pasta>,
    itens: Vec<Item>,
    /// As lápides, podadas aos 90 dias.
    removidos: Vec<Removido>,
    /// Para cada `uid`, a [`impressao`] do item (o `alterado` e uma soma do texto) que estava no
    /// disco na última leitura ou gravação (M47). Só na memória: com ela, o juntar sabe se uma
    /// nota mudou só aqui, só lá ou dos dois lados. A soma do texto cobre duas gravações no mesmo
    /// segundo, que o `alterado` sozinho não distingue.
    #[serde(skip)]
    vistos: HashMap<String, (u64, u64)>,
}

impl Default for Conteudo {
    fn default() -> Self {
        Self {
            proximo_id: 1,
            pastas: Vec::new(),
            itens: Vec::new(),
            removidos: Vec::new(),
            vistos: HashMap::new(),
        }
    }
}

/// O que uma junção fez, para o aviso de sucesso.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Resumo {
    /// Itens e pastas que só existiam do outro lado.
    pub novos: usize,
    /// Itens e pastas que o outro lado tinha mais novos.
    pub atualizados: usize,
    /// Itens e pastas que o outro lado tinha apagado.
    pub removidos: usize,
    /// Notas alteradas nas duas máquinas: a versão que perdeu ficou como cópia (M47).
    pub conflitos: usize,
    /// Se esta cópia tem algo que o outro lado não tem (o outro lado ficou para trás).
    pub a_gravar: bool,
}

impl Resumo {
    /// Se a junção mudou alguma coisa nesta cópia.
    pub fn mudou(&self) -> bool {
        self.novos + self.atualizados + self.removidos + self.conflitos > 0
    }
}

/// O erro de um item que foi apagado enquanto se mexia nele.
fn item_sumiu() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, t!("cofre-item-sumiu"))
}

/// `nome` sem repetir o de nenhum outro anexo de `anexos` (o de `ignorar` não conta): o que
/// repete ganha " (2)", " (3)"… antes da extensão.
pub(crate) fn nome_de_anexo_unico(anexos: &[Anexo], nome: &str, ignorar: Option<usize>) -> String {
    let nome = if nome.trim().is_empty() {
        "anexo"
    } else {
        nome.trim()
    };
    let em_uso = |candidato: &str| {
        anexos
            .iter()
            .enumerate()
            .any(|(i, a)| Some(i) != ignorar && a.nome == candidato)
    };
    if !em_uso(nome) {
        return nome.to_owned();
    }
    // A extensão é o que vem depois do último ponto, se o ponto não for o primeiro caractere.
    let (base, extensao) = match nome.rfind('.') {
        Some(0) | None => (nome, ""),
        Some(i) => nome.split_at(i),
    };
    (2..)
        .map(|n| format!("{base} ({n}){extensao}"))
        .find(|c| !em_uso(c))
        .unwrap_or_else(|| nome.to_owned())
}

/// Um identificador novo de 128 bits, em hexadecimal.
pub(crate) fn uid_novo() -> String {
    // Sem o sorteio do sistema (que não falha na prática), o relógio e um contador não deixam
    // dois `uid` iguais dentro do processo.
    static SEQUENCIA: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let bytes = aleatorio::<16>().unwrap_or_else(|_| {
        let n = SEQUENCIA.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&agora().to_be_bytes());
        bytes[8..].copy_from_slice(&n.to_be_bytes());
        bytes
    });
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Conteudo {
    fn pasta(&self, id: u64) -> Option<&Pasta> {
        self.pastas.iter().find(|p| p.id == id)
    }

    fn novo_id(&mut self) -> u64 {
        let id = self.proximo_id;
        self.proximo_id += 1;
        id
    }

    /// Uma pasta nova com `uid` próprio.
    fn nova_pasta(&mut self, nome: String, pai: Option<u64>) -> u64 {
        let id = self.novo_id();
        self.pastas.push(Pasta {
            id,
            uid: uid_novo(),
            nome,
            pai,
            alterado: agora(),
        });
        id
    }

    /// A pasta de `pai` com esse nome (sem diferenciar maiúsculas nem acentos).
    fn irma(&self, pai: Option<u64>, nome: &str, exceto: Option<u64>) -> Option<&Pasta> {
        let nome = normalizar(nome);
        self.pastas
            .iter()
            .find(|p| p.pai == pai && Some(p.id) != exceto && normalizar(&p.nome) == nome)
    }

    /// Desce por `partes` a partir de `pai`, criando o que falta. Não grava.
    fn garantir_caminho(&mut self, pai: Option<u64>, partes: &[&str]) -> Option<u64> {
        let mut atual = pai;
        for parte in partes {
            let parte = parte.trim();
            if parte.is_empty() {
                continue;
            }
            atual = Some(match self.irma(atual, parte, None) {
                Some(existente) => existente.id,
                None => self.nova_pasta(parte.to_owned(), atual),
            });
        }
        atual
    }

    /// Todas as pastas de dentro de `id`, em qualquer profundidade (sem a própria).
    fn descendentes(&self, id: u64) -> Vec<u64> {
        let mut achadas = Vec::new();
        let mut fila = vec![id];
        while let Some(pai) = fila.pop() {
            for filha in self.pastas.iter().filter(|p| p.pai == Some(pai)) {
                if !achadas.contains(&filha.id) && filha.id != id {
                    achadas.push(filha.id);
                    fila.push(filha.id);
                }
            }
        }
        achadas
    }

    /// Anota a lápide de um `uid` (a mais nova fica; `uid` vazio não deixa lápide).
    fn lapide(&mut self, uid: &str, quando: u64) {
        if uid.is_empty() {
            return;
        }
        match self.removidos.iter_mut().find(|r| r.uid == uid) {
            Some(r) => r.quando = r.quando.max(quando),
            None => self.removidos.push(Removido {
                uid: uid.to_owned(),
                quando,
            }),
        }
    }

    fn lapide_de(&self, uid: &str) -> Option<u64> {
        self.removidos
            .iter()
            .find(|r| r.uid == uid)
            .map(|r| r.quando)
    }

    /// Tira as lápides com mais de 90 dias.
    fn podar(&mut self, agora: u64) {
        self.removidos
            .retain(|r| r.quando.saturating_add(VIDA_DA_LAPIDE_SECS) >= agora);
    }

    /// Dá um `uid` ao que veio sem (cofre v2, ou um arquivo mexido à mão).
    fn dar_uids(&mut self) {
        for pasta in &mut self.pastas {
            if pasta.uid.is_empty() {
                pasta.uid = uid_novo();
            }
        }
        for item in &mut self.itens {
            if item.uid.is_empty() {
                item.uid = uid_novo();
            }
        }
    }

    /// Tira uma pasta (a lápide fica por conta de quem chama): o que havia dentro dela vai para
    /// a pasta mãe.
    fn tirar_pasta(&mut self, id: u64) {
        let Some(pai) = self.pasta(id).map(|p| p.pai) else {
            return;
        };
        for filha in &mut self.pastas {
            if filha.pai == Some(id) {
                filha.pai = pai;
            }
        }
        for item in &mut self.itens {
            if item.pasta == Some(id) {
                item.pasta = pai;
            }
        }
        self.pastas.retain(|p| p.id != id);
    }

    /// Junta `outro` a este conteúdo (seção 7.4 do plano do M44).
    fn juntar(&mut self, mut outro: Conteudo) -> Resumo {
        let agora = agora();
        outro.dar_uids();
        let mut resumo = Resumo::default();
        // O que o outro lado tinha, para saber no fim se ele ficou para trás.
        let la_itens: HashMap<String, u64> = outro
            .itens
            .iter()
            .map(|i| (i.uid.clone(), i.alterado))
            .collect();
        let la_pastas: HashMap<String, u64> = outro
            .pastas
            .iter()
            .map(|p| (p.uid.clone(), p.alterado))
            .collect();

        // 1. As lápides do outro lado apagam o que aqui é mais velho que elas.
        for lapide in outro.removidos.clone() {
            if let Some(pasta) = self
                .pastas
                .iter()
                .find(|p| p.uid == lapide.uid && p.alterado < lapide.quando)
                .map(|p| p.id)
            {
                self.tirar_pasta(pasta);
                resumo.removidos += 1;
            }
            let antes = self.itens.len();
            self.itens
                .retain(|i| !(i.uid == lapide.uid && i.alterado < lapide.quando));
            resumo.removidos += antes - self.itens.len();
        }

        // 2. As pastas do outro lado, dos pais para os filhos. Da pasta de lá para a de cá
        // (`None` é a raiz: onde fica o que estava numa pasta que não vale mais).
        let mut traduzidas: HashMap<u64, Option<u64>> = HashMap::new();
        consertar_pastas(&mut outro.pastas);
        let mut faltam = std::mem::take(&mut outro.pastas);
        while !faltam.is_empty() {
            let antes = faltam.len();
            let mut adiadas = Vec::new();
            for de in faltam {
                let pai = match de.pai {
                    None => None,
                    Some(pai) => match traduzidas.get(&pai) {
                        Some(&traduzida) => traduzida,
                        None => {
                            adiadas.push(de);
                            continue;
                        }
                    },
                };
                let apagada = self.lapide_de(&de.uid).is_some_and(|q| q > de.alterado);
                let aqui = self.pastas.iter().position(|p| p.uid == de.uid);
                let destino = if apagada {
                    // Foi apagada aqui, depois da última mudança de lá: não volta.
                    pai
                } else if let Some(i) = aqui {
                    if de.alterado > self.pastas[i].alterado {
                        self.pastas[i].nome = de.nome.clone();
                        self.pastas[i].pai = pai;
                        self.pastas[i].alterado = de.alterado;
                        resumo.atualizados += 1;
                    }
                    Some(self.pastas[i].id)
                } else if let Some(irma) = self.irma(pai, &de.nome, None).map(|p| p.id) {
                    // Duas pastas criadas de cada lado com o mesmo nome viram uma só, e as
                    // duas cópias ficam com o menor `uid`, para não voltarem a divergir.
                    if let Some(p) = self.pastas.iter_mut().find(|p| p.id == irma)
                        && de.uid < p.uid
                    {
                        p.uid = de.uid.clone();
                    }
                    Some(irma)
                } else {
                    let id = self.novo_id();
                    self.pastas.push(Pasta {
                        id,
                        pai,
                        ..de.clone()
                    });
                    resumo.novos += 1;
                    Some(id)
                };
                traduzidas.insert(de.id, destino);
            }
            faltam = adiadas;
            if faltam.len() == antes {
                break;
            }
        }
        consertar_pastas(&mut self.pastas);

        // 3. Os itens.
        for mut de in std::mem::take(&mut outro.itens) {
            let pasta = de
                .pasta
                .and_then(|p| traduzidas.get(&p).copied().unwrap_or(None));
            if let Some(i) = self.itens.iter().position(|i| i.uid == de.uid) {
                if self.itens[i].tipo == Tipo::Nota
                    && de.tipo == Tipo::Nota
                    && texto_da_nota_diferente(&self.itens[i], &de)
                {
                    self.juntar_nota(i, de, pasta, &mut resumo);
                } else if de.alterado > self.itens[i].alterado {
                    de.id = self.itens[i].id;
                    de.pasta = pasta;
                    self.itens[i] = de;
                    resumo.atualizados += 1;
                }
            } else if !self.lapide_de(&de.uid).is_some_and(|q| q > de.alterado) {
                de.id = self.novo_id();
                de.pasta = pasta;
                self.itens.push(de);
                resumo.novos += 1;
            }
        }

        // 4. As lápides de lá passam para cá; o que aqui sobrou e lá não há deixa o outro lado
        // para trás.
        for lapide in &outro.removidos {
            self.lapide(&lapide.uid, lapide.quando);
        }
        self.podar(agora);
        let mais_novo = |la: &HashMap<String, u64>, uid: &str, alterado: u64| {
            la.get(uid).is_none_or(|&quando| quando < alterado)
        };
        resumo.a_gravar = self
            .itens
            .iter()
            .any(|i| mais_novo(&la_itens, &i.uid, i.alterado))
            || self
                .pastas
                .iter()
                .any(|p| mais_novo(&la_pastas, &p.uid, p.alterado))
            || self.removidos.iter().any(|r| {
                outro
                    .removidos
                    .iter()
                    .find(|o| o.uid == r.uid)
                    .is_none_or(|o| o.quando < r.quando)
            });
        resumo
    }
}

impl Conteudo {
    /// Junta a nota `de` (do outro lado) à de mesmo `uid` que está em `self.itens[i]`, sem perder
    /// o texto de nenhuma das duas (M47): só um lado mudou desde a última leitura do disco, vale
    /// o que mudou; se os dois mudaram (ou não se sabe), vale o `alterado` maior e a outra versão
    /// entra como uma nota nova, "conflito de …".
    fn juntar_nota(&mut self, i: usize, mut de: Item, pasta: Option<u64>, resumo: &mut Resumo) {
        let visto = self.vistos.get(&de.uid).copied();
        let so_aqui = visto == Some(impressao(&de));
        let so_la = visto == Some(impressao(&self.itens[i]));
        if so_aqui {
            return;
        }
        if so_la {
            de.id = self.itens[i].id;
            de.pasta = pasta;
            self.itens[i] = de;
            resumo.atualizados += 1;
            return;
        }
        // Os dois mudaram: o `alterado` maior fica, e empata a favor do daqui.
        let la_vence = de.alterado > self.itens[i].alterado;
        let mut perdedora = if la_vence {
            let id = self.itens[i].id;
            let aqui = std::mem::replace(
                &mut self.itens[i],
                Item {
                    id,
                    pasta,
                    ..de.clone()
                },
            );
            resumo.atualizados += 1;
            aqui
        } else {
            Item {
                pasta,
                ..de.clone()
            }
        };
        perdedora.titulo = t!(
            "cofre-conflito-de",
            titulo = perdedora.titulo.as_str(),
            data = crate::arquivos::formato::data(Some(
                std::time::UNIX_EPOCH + Duration::from_secs(perdedora.alterado)
            ))
        );
        perdedora.uid = uid_novo();
        perdedora.id = self.novo_id();
        self.itens.push(perdedora);
        resumo.conflitos += 1;
    }

    /// Anota o `alterado` de cada item como o que está no disco agora.
    fn marcar_vistos(&mut self) {
        self.vistos = self
            .itens
            .iter()
            .map(|i| (i.uid.clone(), impressao(i)))
            .collect();
    }
}

/// O `alterado` do item e uma soma do que a junção das notas compara (título, texto e anexos).
fn impressao(item: &Item) -> (u64, u64) {
    use std::hash::{Hash, Hasher};
    let mut soma = std::collections::hash_map::DefaultHasher::new();
    item.titulo.hash(&mut soma);
    item.notas.hash(&mut soma);
    for anexo in &item.anexos {
        anexo.id.hash(&mut soma);
        anexo.nome.hash(&mut soma);
    }
    (item.alterado, soma.finish())
}

/// Se duas versões da mesma nota têm título, texto ou anexos diferentes.
fn texto_da_nota_diferente(a: &Item, b: &Item) -> bool {
    a.titulo != b.titulo || a.notas != b.notas || a.anexos != b.anexos
}

/// O nome de uma pasta, sem os espaços das pontas; recusa vazio e `/`.
fn nome_de_pasta(nome: &str) -> io::Result<String> {
    let nome = nome.trim();
    if nome.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            t!("cofre-pasta-nome-vazio"),
        ));
    }
    if nome.contains('/') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            t!("cofre-pasta-nome-com-barra"),
        ));
    }
    Ok(nome.to_owned())
}

/// Corta um caminho em `/`, tira os espaços das pontas e descarta as partes vazias.
pub fn partes_do_caminho(texto: &str) -> Vec<&str> {
    texto
        .split('/')
        .map(str::trim)
        .filter(|parte| !parte.is_empty())
        .collect()
}

/// Conserta as referências de `pastas`: um pai que não existe, ou que forma um ciclo, vira a raiz.
fn consertar_pastas(pastas: &mut [Pasta]) {
    let ids: HashSet<u64> = pastas.iter().map(|p| p.id).collect();
    for pasta in pastas.iter_mut() {
        if pasta
            .pai
            .is_some_and(|pai| pai == pasta.id || !ids.contains(&pai))
        {
            pasta.pai = None;
        }
    }
    for i in 0..pastas.len() {
        let (id, mut atual) = (pastas[i].id, pastas[i].pai);
        for _ in 0..=pastas.len() {
            let Some(pai) = atual else { break };
            if pai == id {
                pastas[i].pai = None;
                break;
            }
            atual = pastas.iter().find(|p| p.id == pai).and_then(|p| p.pai);
        }
    }
}

/// Pastas e itens vindos de fora (um arquivo importado), com ids próprios do lote.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lote {
    /// As pastas, com `pai` entre as do lote.
    pub pastas: Vec<Pasta>,
    /// Os itens, com `pasta` entre as do lote.
    pub itens: Vec<Item>,
}

impl Lote {
    /// Cria no lote as pastas de um caminho com `/` (juntando as que já existem) e devolve a
    /// última; `None` quando o texto não tem nome nenhum.
    pub fn caminho(&mut self, texto: &str) -> Option<u64> {
        let mut atual = None;
        for parte in partes_do_caminho(texto) {
            let nome = normalizar(parte);
            let existente = self
                .pastas
                .iter()
                .find(|p| p.pai == atual && normalizar(&p.nome) == nome)
                .map(|p| p.id);
            atual = Some(existente.unwrap_or_else(|| {
                let id = self.pastas.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                self.pastas.push(Pasta {
                    id,
                    nome: parte.to_owned(),
                    pai: atual,
                    ..Pasta::default()
                });
                id
            }));
        }
        atual
    }
}

/// Os ids do lote e os que eles ganharam no cofre.
#[derive(Debug, Default)]
pub struct Mapa {
    /// Pasta do lote → pasta do cofre.
    pub pastas: HashMap<u64, u64>,
    /// Item do lote → item do cofre.
    pub itens: HashMap<u64, u64>,
}

type Chave = [u8; 32];

/// Os segundos desde 1970; o relógio das datas dos itens é o de `recentes`, que os testes fixam.
use crate::recentes::agora;

fn aleatorio<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|e| io::Error::other(t!("cofre-erro-aleatorio", erro = e.to_string())))?;
    Ok(bytes)
}

fn cifra(chave: &Chave) -> Aes256Gcm {
    Aes256Gcm::new(chave.into())
}

/// Cifra `claro` com a chave e um nonce novo.
pub fn cifrar(chave: &[u8; 32], claro: &[u8]) -> io::Result<Segredo> {
    let nonce = aleatorio::<12>()?;
    let dados = cifra(chave)
        .encrypt(&Nonce::<U12>::from(nonce), claro)
        .map_err(|_| io::Error::other(t!("cofre-erro-cifrar")))?;
    Ok(Segredo {
        nonce: STANDARD.encode(nonce),
        dados: STANDARD.encode(dados),
    })
}

/// Decifra; `None` se a chave não serve ou o texto foi mexido.
pub fn decifrar(chave: &[u8; 32], segredo: &Segredo) -> Option<Vec<u8>> {
    let nonce: [u8; 12] = STANDARD.decode(&segredo.nonce).ok()?.try_into().ok()?;
    let dados = STANDARD.decode(&segredo.dados).ok()?;
    cifra(chave)
        .decrypt(&Nonce::<U12>::from(nonce), dados.as_slice())
        .ok()
}

fn derivar(senha: &str, sal: &[u8], m_kib: u32, t: u32, p: u32) -> Option<Chave> {
    let params = Params::new(m_kib, t, p, Some(32)).ok()?;
    let mut saida = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(senha.as_bytes(), sal, &mut saida)
        .ok()?;
    Some(saida)
}

/// A chave de 32 bytes de uma proteção `embutida`.
fn chave_embutida(texto: &str) -> Option<Chave> {
    STANDARD.decode(texto).ok()?.try_into().ok()
}

/// A proteção por senha mestra com um sal novo, e a chave que ela gera.
fn proteger_com_senha(senha: &str) -> io::Result<(Protecao, Chave)> {
    let sal = aleatorio::<16>()?;
    let (m_kib, t, p) = (
        Params::DEFAULT_M_COST,
        Params::DEFAULT_T_COST,
        Params::DEFAULT_P_COST,
    );
    let chave = derivar(senha, &sal, m_kib, t, p)
        .ok_or_else(|| io::Error::other(t!("cofre-erro-derivar")))?;
    let protecao = Protecao::SenhaMestra {
        sal: STANDARD.encode(sal),
        m_kib,
        t,
        p,
        verificador: cifrar(&chave, VERIFICADOR)?,
    };
    Ok((protecao, chave))
}

/// A chave de uma proteção por senha mestra aberta com `senha`; `None` se a senha não confere.
fn chave_da_senha(protecao: &Protecao, senha: &str) -> Option<Chave> {
    let Protecao::SenhaMestra {
        sal,
        m_kib,
        t,
        p,
        verificador,
    } = protecao
    else {
        return None;
    };
    let sal = STANDARD.decode(sal).ok()?;
    let chave = derivar(senha, &sal, *m_kib, *t, *p)?;
    (decifrar(&chave, verificador).as_deref() == Some(VERIFICADOR)).then_some(chave)
}

fn bloqueado_erro() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        t!("comum-o-cofre-esta-bloqueado"),
    )
}

/// Grava `nome` na `pasta` de forma atômica ([`gravar_atomico`]), no Unix com a permissão
/// `0o600`: o cofre e a chave são só de quem os criou.
fn gravar_privado(pasta: &Path, nome: &str, bytes: &[u8]) -> io::Result<()> {
    fs::create_dir_all(pasta)?;
    gravar_atomico(&pasta.join(nome), bytes, true)
}

/// Grava `bytes` em `caminho` como o `vault.json`: atômico e, no Unix, só do dono. É o
/// `.tirech-vault` que sai do app (pode ter a chave dentro).
pub fn gravar_arquivo_privado(caminho: &Path, bytes: &[u8]) -> io::Result<()> {
    let nome = caminho
        .file_name()
        .ok_or_else(|| io::Error::other(t!("cofre-erro-sem-nome-de-arquivo")))?
        .to_string_lossy()
        .into_owned();
    let pasta = match caminho.parent() {
        Some(pasta) if !pasta.as_os_str().is_empty() => pasta,
        _ => Path::new("."),
    };
    gravar_privado(pasta, &nome, bytes)
}

/// Grava o arquivo `vault.key`.
fn gravar_chave(pasta: &Path, chave: &Chave) -> io::Result<()> {
    gravar_privado(pasta, ARQUIVO_CHAVE, chave)
}

fn ler_chave(pasta: &Path) -> Option<Chave> {
    fs::read(pasta.join(ARQUIVO_CHAVE)).ok()?.try_into().ok()
}

/// Os nomes antigos (`cofre.json` e `chave`) passam a `vault.json` e `vault.key`. Só renomeia
/// quando o novo não existe; com os dois, não mexe em nenhum. Não decifra nada. Numa pasta só de
/// leitura ([`so_leitura`]) não renomeia: quem migra é a cópia do app dona da pasta.
fn migrar_nomes(pasta: &Path) {
    if so_leitura(pasta) {
        return;
    }
    for (antigo, novo) in [
        (ARQUIVO_ANTIGO, ARQUIVO),
        (ARQUIVO_CHAVE_ANTIGO, ARQUIVO_CHAVE),
    ] {
        let (antigo, novo) = (pasta.join(antigo), pasta.join(novo));
        if antigo.exists()
            && !novo.exists()
            && let Err(e) = fs::rename(&antigo, &novo)
        {
            eprintln!(
                "Não consegui renomear {} para {}: {e}.",
                antigo.display(),
                novo.display()
            );
        }
    }
}

/// O `Conteudo` de um arquivo decifrado da `versao`. A 1 guardava a pasta de cada item em texto
/// (`Clientes/ACME`): vira pastas de verdade. A 2 não tinha `uid`: cada item e cada pasta ganha
/// um, e a passphrase de uma chave que ficava pelo caminho do arquivo passa a ficar pelo
/// fingerprint (quando o arquivo existe aqui). Só na memória; o arquivo vira v3 na gravação.
fn conteudo_de(versao: u32, claro: &[u8]) -> Option<Conteudo> {
    let mut conteudo = conteudo_antigo(versao, claro)?;
    conteudo.dar_uids();
    if versao < 3 {
        for item in &mut conteudo.itens {
            if item.tipo == Tipo::Chave
                && let Some(fingerprint) = fingerprint_do_caminho(&item.url)
            {
                item.campos.retain(|c| c.nome != CAMPO_ARQUIVO);
                item.campos.push(Campo {
                    nome: CAMPO_ARQUIVO.to_owned(),
                    valor: std::mem::replace(&mut item.url, fingerprint),
                    oculto: false,
                });
            }
        }
    }
    Some(conteudo)
}

/// O nome do campo extra do item de passphrase que diz de que arquivo a chave veio (só informa).
pub const CAMPO_ARQUIVO: &str = "arquivo";

/// O fingerprint da chave em `caminho`, se ele é o caminho de um arquivo de chave que existe e
/// diz qual é a pública.
fn fingerprint_do_caminho(caminho: &str) -> Option<String> {
    let caminho = Path::new(caminho);
    if !caminho.is_absolute() || !caminho.is_file() {
        return None;
    }
    tomni_ui::terminal::ssh::keys::KeyFile::inspect(caminho)
        .ok()?
        .fingerprint()
}

fn conteudo_antigo(versao: u32, claro: &[u8]) -> Option<Conteudo> {
    if versao >= 2 {
        return serde_json::from_slice(claro).ok();
    }
    let mut valor: serde_json::Value = serde_json::from_slice(claro).ok()?;
    let mut conteudo = Conteudo {
        proximo_id: valor
            .get("proximo_id")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1),
        ..Conteudo::default()
    };
    let mut itens = Vec::new();
    if let Some(lista) = valor.get_mut("itens").and_then(|v| v.as_array_mut()) {
        for objeto in lista {
            let texto = objeto
                .as_object_mut()
                .and_then(|o| o.remove("pasta"))
                .and_then(|p| p.as_str().map(str::to_owned))
                .unwrap_or_default();
            let item: Item = serde_json::from_value(objeto.take()).ok()?;
            itens.push((item, texto));
        }
    }
    // Os ids novos das pastas não podem esbarrar nos dos itens.
    let maior = itens.iter().map(|(i, _)| i.id).max().unwrap_or(0);
    conteudo.proximo_id = conteudo.proximo_id.max(maior + 1);
    for (mut item, texto) in itens {
        item.pasta = conteudo.garantir_caminho(None, &partes_do_caminho(&texto));
        conteudo.itens.push(item);
    }
    Some(conteudo)
}

/// O cofre de uma pasta. A cópia vai para a thread da exportação das notas (M47).
#[derive(Clone)]
pub struct Cofre {
    pasta: PathBuf,
    protecao: Protecao,
    chave: Option<Chave>,
    conteudo: Conteudo,
    /// Os bytes do `vault.json` como foram lidos ou gravados por último: se o arquivo for outro
    /// na hora de gravar, alguém mexeu nele (outra máquina, pela sincronização).
    no_disco: Vec<u8>,
    /// Por que o cofre não grava (o `vault.json` existe, mas a chave não): gravar por cima
    /// perderia o cofre.
    trancado: Option<String>,
    /// Quantas notas viraram "conflito de …" ao juntar no meio de uma gravação (M47), ainda sem
    /// aviso para quem usa. A tela o esvazia com [`Cofre::tomar_conflitos`].
    conflitos_novos: usize,
}

/// O aviso de um `vault.json` sem o `vault.key` ao lado.
fn sem_chave() -> String {
    t!("cofre-sem-vault-key")
}

impl Cofre {
    fn vazio(pasta: &Path) -> Self {
        Self {
            pasta: pasta.to_owned(),
            protecao: Protecao::Chave,
            chave: None,
            conteudo: Conteudo::default(),
            no_disco: Vec::new(),
            trancado: None,
            conflitos_novos: 0,
        }
    }

    /// Lê o `vault.json` da pasta (renomeando antes os nomes antigos, `cofre.json` e `chave`).
    /// O aviso é o do arquivo corrompido (ou ilegível).
    pub fn abrir(pasta: &Path) -> (Cofre, Option<String>) {
        migrar_nomes(pasta);
        let arquivo = pasta.join(ARQUIVO);
        let bytes = match fs::read(&arquivo) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (Self::vazio(pasta), None),
            Err(e) => {
                return (
                    Self::vazio(pasta),
                    Some(t!(
                        "cofre-nao-consegui-ler-o-cofre",
                        arquivo = arquivo.display().to_string(),
                        erro = e.to_string()
                    )),
                );
            }
        };
        // Gravado por uma versão mais nova do app: não está corrompido, só não se entende aqui.
        // Abre vazio e trancado (não grava por cima nem tira o arquivo do lugar), com o motivo.
        if versao_de(&bytes).is_some_and(|v| v > VERSAO) {
            let mut cofre = Self::vazio(pasta);
            cofre.trancado = Some(t!("cofre-versao-mais-nova"));
            cofre.no_disco = bytes;
            return (cofre, Some(t!("cofre-versao-mais-nova")));
        }
        // Só o `vault.json`, sem a chave: o cofre não está corrompido, e gravar por cima o
        // perderia. Abre vazio e trancado, com o motivo.
        if let Ok(a) = serde_json::from_slice::<Arquivo>(&bytes)
            && (1..=VERSAO).contains(&a.versao)
            && a.protecao == Protecao::Chave
            && ler_chave(pasta).is_none()
            && !pasta.join(ARQUIVO_CHAVE).exists()
        {
            let mut cofre = Self::vazio(pasta);
            cofre.trancado = Some(sem_chave());
            return (cofre, Some(sem_chave()));
        }
        match Self::ler(pasta, bytes) {
            Some(cofre) => {
                cofre.copia_do_dia(&cofre.no_disco);
                (cofre, None)
            }
            None => Self::corrompido(pasta),
        }
    }

    /// Guarda `bytes` (um `vault.json` que se leu) e o `vault.key` de agora como a cópia de
    /// hoje, em `copias/`, se ela ainda não existe ([`copia_do_dia`]).
    fn copia_do_dia(&self, bytes: &[u8]) {
        if bytes.is_empty() || copia_do_dia_existe(&self.pasta, ARQUIVO) {
            return;
        }
        if let Ok(chave) = fs::read(self.pasta.join(ARQUIVO_CHAVE)) {
            let _ = copia_do_dia(&self.pasta, ARQUIVO_CHAVE, &chave, true);
        }
        let _ = copia_do_dia(&self.pasta, ARQUIVO, bytes, true);
    }

    fn ler(pasta: &Path, bytes: Vec<u8>) -> Option<Cofre> {
        let arquivo: Arquivo = serde_json::from_slice(&bytes).ok()?;
        if !(1..=VERSAO).contains(&arquivo.versao) {
            return None;
        }
        match arquivo.protecao {
            Protecao::Chave => {
                let chave = ler_chave(pasta)?;
                let conteudo = conteudo_de(arquivo.versao, &decifrar(&chave, &arquivo.itens)?)?;
                Some(Self {
                    pasta: pasta.to_owned(),
                    protecao: Protecao::Chave,
                    chave: Some(chave),
                    conteudo: Self::ajusta_visto(conteudo),
                    no_disco: bytes,
                    trancado: None,
                    conflitos_novos: 0,
                })
            }
            // O `.tirech-vault` usado como `vault.json`: a chave vem de dentro dele.
            Protecao::Embutida { chave } => {
                let chave = chave_embutida(&chave)?;
                let conteudo = conteudo_de(arquivo.versao, &decifrar(&chave, &arquivo.itens)?)?;
                Some(Self {
                    pasta: pasta.to_owned(),
                    protecao: Protecao::Embutida {
                        chave: STANDARD.encode(chave),
                    },
                    chave: Some(chave),
                    conteudo: Self::ajusta_visto(conteudo),
                    no_disco: bytes,
                    trancado: None,
                    conflitos_novos: 0,
                })
            }
            // Bloqueado: a lista só se decifra depois da senha mestra.
            protecao @ Protecao::SenhaMestra { .. } => Some(Self {
                pasta: pasta.to_owned(),
                protecao,
                chave: None,
                conteudo: Conteudo::default(),
                no_disco: bytes,
                trancado: None,
                conflitos_novos: 0,
            }),
        }
    }

    /// Um `proximo_id` perdido ou velho não pode repetir um id que já existe (de item ou de
    /// pasta), e uma pasta mãe que não existe, ou que faz um ciclo, vira a raiz. Um item numa
    /// pasta que não existe vai para a raiz.
    fn ajusta(mut conteudo: Conteudo) -> Conteudo {
        let maior = conteudo
            .itens
            .iter()
            .map(|i| i.id)
            .chain(conteudo.pastas.iter().map(|p| p.id))
            .max()
            .unwrap_or(0);
        conteudo.proximo_id = conteudo.proximo_id.max(maior + 1);
        consertar_pastas(&mut conteudo.pastas);
        let ids: HashSet<u64> = conteudo.pastas.iter().map(|p| p.id).collect();
        for item in &mut conteudo.itens {
            if item.pasta.is_some_and(|pasta| !ids.contains(&pasta)) {
                item.pasta = None;
            }
        }
        conteudo
    }

    /// O [`Cofre::ajusta`] de um conteúdo que acabou de vir do disco: o que ele tem é o que está lá.
    fn ajusta_visto(conteudo: Conteudo) -> Conteudo {
        let mut conteudo = Self::ajusta(conteudo);
        conteudo.marcar_vistos();
        conteudo
    }

    /// Tira do caminho o `vault.json` que não se lê e o `vault.key`, com o mesmo final (`.bak`,
    /// ou `.bak-<data>` se já há um: nunca por cima de uma cópia de antes), e abre vazio. Numa
    /// pasta só de leitura não tira nada do lugar: abre vazio e trancado, com o motivo.
    fn corrompido(pasta: &Path) -> (Cofre, Option<String>) {
        if so_leitura(pasta) {
            let aviso = t!(
                "cofre-corrompido-sem-copia",
                erro = crate::armazenamento::erro_so_leitura().to_string()
            );
            let mut cofre = Self::vazio(pasta);
            cofre.trancado = Some(aviso.clone());
            return (cofre, Some(aviso));
        }
        let sufixo = sufixo_da_copia(&[pasta.join(ARQUIVO), pasta.join(ARQUIVO_CHAVE)]);
        let copia = pasta.join(format!("{ARQUIVO}{sufixo}"));
        let aviso = match fs::rename(pasta.join(ARQUIVO), &copia) {
            Ok(()) => t!(
                "cofre-corrompido-com-copia",
                caminho = copia.display().to_string()
            ),
            Err(e) => t!("cofre-corrompido-sem-copia", erro = e.to_string()),
        };
        if pasta.join(ARQUIVO_CHAVE).exists() {
            let _ = fs::rename(
                pasta.join(ARQUIVO_CHAVE),
                pasta.join(format!("{ARQUIVO_CHAVE}{sufixo}")),
            );
        }
        (Self::vazio(pasta), Some(aviso))
    }

    /// Com senha mestra e ainda sem a senha digitada.
    pub fn bloqueado(&self) -> bool {
        self.tem_senha_mestra() && self.chave.is_none()
    }

    /// Se o cofre pede uma senha mestra.
    pub fn tem_senha_mestra(&self) -> bool {
        matches!(self.protecao, Protecao::SenhaMestra { .. })
    }

    /// Desbloqueia com a senha mestra. `false` (e continua bloqueado) se a senha não confere; o
    /// arquivo não muda. Lê a proteção do arquivo, não da memória: se a senha mestra foi trocada
    /// em outra máquina, vale a nova.
    pub fn desbloquear(&mut self, senha_mestra: &str) -> bool {
        if !self.tem_senha_mestra() {
            return true;
        }
        let Some(bytes) = fs::read(self.pasta.join(ARQUIVO)).ok() else {
            return false;
        };
        let Ok(arquivo) = serde_json::from_slice::<Arquivo>(&bytes) else {
            return false;
        };
        let Some(chave) = chave_da_senha(&arquivo.protecao, senha_mestra) else {
            return false;
        };
        let Some(conteudo) =
            decifrar(&chave, &arquivo.itens).and_then(|claro| conteudo_de(arquivo.versao, &claro))
        else {
            return false;
        };
        self.conteudo = Self::ajusta_visto(conteudo);
        self.protecao = arquivo.protecao;
        self.chave = Some(chave);
        self.no_disco = bytes;
        true
    }

    /// Esquece a chave e os itens da memória (só com senha mestra; sem ela não faz nada).
    pub fn bloquear(&mut self) {
        if self.tem_senha_mestra() {
            self.chave = None;
            self.conteudo = Conteudo::default();
        }
    }

    /// Os itens (vazio enquanto bloqueado).
    pub fn itens(&self) -> &[Item] {
        &self.conteudo.itens
    }

    /// A pasta de dados onde o cofre mora (a do `vault.json`).
    pub fn pasta_de_dados(&self) -> &Path {
        &self.pasta
    }

    /// As chaves dos logs cifrados (M53) que o cofre tem: os itens com [`CAMPO_LOGS`] cuja
    /// privada (a senha) e cuja pública (o campo) são base64 de 32 bytes e combinam. Um item
    /// mexido é ignorado. Vazio enquanto bloqueado.
    pub fn chaves_dos_logs(&self) -> Vec<ChaveDosLogs> {
        self.conteudo
            .itens
            .iter()
            .filter_map(ChaveDosLogs::do_item)
            .collect()
    }

    /// Cria o item da chave dos logs (M53): sorteia o par, salva o item na raiz e devolve a chave.
    pub fn criar_chave_dos_logs(&mut self) -> io::Result<ChaveDosLogs> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let (privada, publica) = crate::log_cifrado::par_novo()?;
        let item = Item {
            tipo: Tipo::Senha,
            titulo: t!("log-chave-titulo"),
            senha: STANDARD.encode(privada),
            notas: t!("log-chave-notas"),
            campos: vec![Campo {
                nome: CAMPO_LOGS.to_owned(),
                valor: STANDARD.encode(publica),
                oculto: false,
            }],
            ..Item::default()
        };
        let id = self.salvar(item)?;
        self.item(id)
            .and_then(ChaveDosLogs::do_item)
            .ok_or_else(|| io::Error::other(t!("log-nao-cifrou")))
    }

    /// O item com esse id.
    pub fn item(&self, id: u64) -> Option<&Item> {
        self.conteudo.itens.iter().find(|i| i.id == id)
    }

    /// O item com esse `uid` (o que não muda entre máquinas).
    pub fn item_por_uid(&self, uid: &str) -> Option<&Item> {
        if uid.is_empty() {
            return None;
        }
        self.conteudo.itens.iter().find(|i| i.uid == uid)
    }

    /// Os itens do tipo "Chave SSH", em ordem alfabética do título.
    pub fn chaves_ssh(&self) -> Vec<&Item> {
        let mut chaves: Vec<&Item> = self
            .conteudo
            .itens
            .iter()
            .filter(|i| i.tipo == Tipo::ChaveSsh && i.chave.is_some())
            .collect();
        chaves.sort_by_cached_key(|i| (normalizar(&i.titulo), i.id));
        chaves
    }

    /// A chave SSH com esse fingerprint (`SHA256:…`).
    pub fn chave_ssh_por_fingerprint(&self, fingerprint: &str) -> Option<&Item> {
        if fingerprint.is_empty() {
            return None;
        }
        self.conteudo.itens.iter().find(|i| {
            i.tipo == Tipo::ChaveSsh
                && i.chave
                    .as_ref()
                    .is_some_and(|c| c.fingerprint == fingerprint)
        })
    }

    /// A privada aberta da chave com esse `uid`.
    pub fn chave_privada_por_uid(&self, uid: &str) -> Option<ssh_key::PrivateKey> {
        self.item_por_uid(uid)?.chave.as_ref()?.chave_privada()
    }

    /// Todas as pastas, na ordem em que foram criadas.
    pub fn pastas(&self) -> &[Pasta] {
        &self.conteudo.pastas
    }

    /// A pasta com esse id.
    pub fn pasta(&self, id: u64) -> Option<&Pasta> {
        self.conteudo.pasta(id)
    }

    /// As pastas de dentro de `pai` (`None` é a raiz), em ordem alfabética.
    pub fn filhas(&self, pai: Option<u64>) -> Vec<&Pasta> {
        let mut filhas: Vec<&Pasta> = self
            .conteudo
            .pastas
            .iter()
            .filter(|p| p.pai == pai)
            .collect();
        filhas.sort_by_cached_key(|p| (normalizar(&p.nome), p.id));
        filhas
    }

    /// As notas (os itens do tipo [`Tipo::Nota`]).
    pub fn notas(&self) -> impl Iterator<Item = &Item> {
        self.conteudo.itens.iter().filter(|i| i.tipo == Tipo::Nota)
    }

    /// Cria uma nota na pasta (`None` é a raiz) e grava. O título não repete o de outra nota da
    /// pasta: "Nova nota", "Nova nota (2)"… Devolve o id.
    pub fn nova_nota(&mut self, pasta: Option<u64>, titulo: &str, corpo: &str) -> io::Result<u64> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let nova_nota_titulo = t!("notas-nova-nota");
        let base = match titulo.trim() {
            "" => nova_nota_titulo.as_str(),
            t => t,
        };
        let em_uso = |candidato: &str| {
            self.notas()
                .any(|n| n.pasta == pasta && n.titulo == candidato)
        };
        let mut titulo = base.to_owned();
        let mut n = 2;
        while em_uso(&titulo) {
            titulo = format!("{base} ({n})");
            n += 1;
        }
        self.salvar(Item {
            tipo: Tipo::Nota,
            titulo,
            pasta,
            notas: corpo.to_owned(),
            ..Item::default()
        })
    }

    /// Os itens direto nessa pasta, em ordem alfabética do título.
    pub fn itens_em(&self, pasta: Option<u64>) -> Vec<&Item> {
        let mut itens: Vec<&Item> = self
            .conteudo
            .itens
            .iter()
            .filter(|i| i.pasta == pasta)
            .collect();
        itens.sort_by_cached_key(|i| (normalizar(&i.titulo), i.id));
        itens
    }

    /// As subpastas (todas as descendentes, sem a própria) e os itens de dentro de uma pasta, em
    /// qualquer profundidade.
    pub fn conteudo_de_pasta(&self, id: u64) -> (Vec<u64>, Vec<u64>) {
        let subpastas = self.conteudo.descendentes(id);
        let itens = self
            .conteudo
            .itens
            .iter()
            .filter(|i| i.pasta.is_some_and(|p| p == id || subpastas.contains(&p)))
            .map(|i| i.id)
            .collect();
        (subpastas, itens)
    }

    /// O caminho de uma pasta (`Clientes/ACME`); vazio na raiz.
    pub fn caminho(&self, pasta: Option<u64>) -> String {
        let mut nomes = Vec::new();
        let mut atual = pasta;
        // O limite protege de um ciclo que o `ajusta` não pegou.
        while let Some(p) = atual.and_then(|id| self.conteudo.pasta(id)) {
            if nomes.len() > self.conteudo.pastas.len() {
                break;
            }
            nomes.push(p.nome.as_str());
            atual = p.pai;
        }
        nomes.reverse();
        nomes.join("/")
    }

    /// Cria uma pasta em `pai`; se o nome já está em uso ali, acrescenta ` (2)`, ` (3)`…
    pub fn nova_pasta(&mut self, pai: Option<u64>, nome: &str) -> io::Result<u64> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let nome = nome_de_pasta(nome)?;
        if pai.is_some_and(|id| self.conteudo.pasta(id).is_none()) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                t!("cofre-pasta-sumiu"),
            ));
        }
        let mut final_ = nome.clone();
        let mut n = 2;
        while self.conteudo.irma(pai, &final_, None).is_some() {
            final_ = format!("{nome} ({n})");
            n += 1;
        }
        let id = self.conteudo.nova_pasta(final_, pai);
        self.gravar()?;
        Ok(id)
    }

    /// Muda o nome de uma pasta. Recusa nome vazio, com `/` ou já usado por uma irmã.
    pub fn renomear_pasta(&mut self, id: u64, nome: &str) -> io::Result<()> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let nome = nome_de_pasta(nome)?;
        let Some(pai) = self.conteudo.pasta(id).map(|p| p.pai) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                t!("cofre-pasta-sumiu"),
            ));
        };
        if self.conteudo.irma(pai, &nome, Some(id)).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                t!("cofre-pasta-ja-existe", nome = nome.as_str()),
            ));
        }
        let Some(pasta) = self.conteudo.pastas.iter_mut().find(|p| p.id == id) else {
            return Ok(());
        };
        if pasta.nome == nome {
            return Ok(());
        }
        pasta.nome = nome;
        pasta.alterado = agora();
        self.gravar()
    }

    /// Leva pastas e itens para `destino` (`None` é a raiz). Confere tudo antes de mudar; uma
    /// pasta que já está no destino fica onde está.
    pub fn mover(&mut self, pastas: &[u64], itens: &[u64], destino: Option<u64>) -> io::Result<()> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let inexistente = |o: String| io::Error::new(io::ErrorKind::NotFound, o);
        if destino.is_some_and(|id| self.conteudo.pasta(id).is_none()) {
            return Err(inexistente(t!("cofre-pasta-destino-sumiu")));
        }
        // Uma pasta cuja mãe (em qualquer nível) também vai junto acompanha a mãe.
        let mut a_mover: Vec<u64> = Vec::new();
        for &id in pastas {
            if self.conteudo.pasta(id).is_none() {
                return Err(inexistente(t!("cofre-pasta-sumiu")));
            }
            if !a_mover.contains(&id) {
                a_mover.push(id);
            }
        }
        for &id in &a_mover {
            if destino == Some(id)
                || destino.is_some_and(|d| self.conteudo.descendentes(id).contains(&d))
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    t!("cofre-pasta-dentro-dela-mesma"),
                ));
            }
        }
        let acompanham: Vec<u64> = a_mover
            .iter()
            .copied()
            .filter(|&id| {
                a_mover
                    .iter()
                    .any(|&outra| self.conteudo.descendentes(outra).contains(&id))
            })
            .collect();
        a_mover.retain(|id| !acompanham.contains(id));
        a_mover.retain(|&id| self.conteudo.pasta(id).is_some_and(|p| p.pai != destino));
        for (i, &id) in a_mover.iter().enumerate() {
            let nome = self
                .conteudo
                .pasta(id)
                .map(|p| p.nome.clone())
                .unwrap_or_default();
            let repetida = self.conteudo.pastas.iter().any(|p| {
                p.pai == destino
                    && !a_mover.contains(&p.id)
                    && normalizar(&p.nome) == normalizar(&nome)
            }) || a_mover[..i].iter().any(|&outra| {
                self.conteudo
                    .pasta(outra)
                    .is_some_and(|p| normalizar(&p.nome) == normalizar(&nome))
            });
            if repetida {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    t!("cofre-pasta-ja-existe", nome = nome.as_str()),
                ));
            }
        }
        for &id in itens {
            if self.item(id).is_none() {
                return Err(inexistente(t!("cofre-item-sumiu")));
            }
        }
        let mut mudou = false;
        let agora = agora();
        for pasta in &mut self.conteudo.pastas {
            if a_mover.contains(&pasta.id) {
                pasta.pai = destino;
                pasta.alterado = agora;
                mudou = true;
            }
        }
        for item in &mut self.conteudo.itens {
            if itens.contains(&item.id) && item.pasta != destino {
                item.pasta = destino;
                item.alterado = agora;
                mudou = true;
            }
        }
        if mudou { self.gravar() } else { Ok(()) }
    }

    /// Apaga as pastas com tudo o que têm dentro, e os itens soltos.
    pub fn apagar(&mut self, pastas: &[u64], itens: &[u64]) -> io::Result<()> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let mut pastas_fora: HashSet<u64> = HashSet::new();
        for &id in pastas {
            if self.conteudo.pasta(id).is_some() {
                pastas_fora.insert(id);
                pastas_fora.extend(self.conteudo.descendentes(id));
            }
        }
        let agora = agora();
        let conteudo = &mut self.conteudo;
        let lapides: Vec<String> = conteudo
            .pastas
            .iter()
            .filter(|p| pastas_fora.contains(&p.id))
            .map(|p| p.uid.clone())
            .chain(
                conteudo
                    .itens
                    .iter()
                    .filter(|i| {
                        itens.contains(&i.id) || i.pasta.is_some_and(|p| pastas_fora.contains(&p))
                    })
                    .map(|i| i.uid.clone()),
            )
            .collect();
        conteudo.pastas.retain(|p| !pastas_fora.contains(&p.id));
        conteudo.itens.retain(|i| {
            !itens.contains(&i.id) && !i.pasta.is_some_and(|p| pastas_fora.contains(&p))
        });
        for uid in lapides {
            conteudo.lapide(&uid, agora);
        }
        self.gravar()
    }

    /// Desce por `partes` a partir de `pai`, criando as pastas que faltam. Não grava: quem chama
    /// grava depois (o `salvar` ou o `importar`). Bloqueado, devolve `pai` sem criar nada.
    pub fn garantir_caminho(&mut self, pai: Option<u64>, partes: &[&str]) -> Option<u64> {
        if self.bloqueado() {
            return pai;
        }
        self.conteudo.garantir_caminho(pai, partes)
    }

    /// Põe o `lote` dentro de `destino`: as pastas se juntam às de mesmo nome que já existem, e
    /// os itens ganham ids novos. Grava uma vez e devolve os ids que o lote ganhou.
    pub fn importar(&mut self, lote: Lote, destino: Option<u64>) -> io::Result<Mapa> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        if destino.is_some_and(|id| self.conteudo.pasta(id).is_none()) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                t!("cofre-pasta-destino-sumiu"),
            ));
        }
        let Lote { mut pastas, itens } = lote;
        consertar_pastas(&mut pastas);
        // Da pasta do lote para a do cofre (`None` é a raiz do cofre).
        let mut traduzidas: HashMap<u64, Option<u64>> = HashMap::new();
        let mut faltam = pastas;
        while !faltam.is_empty() {
            let antes = faltam.len();
            let mut adiadas = Vec::new();
            for pasta in faltam {
                let pai = match pasta.pai {
                    None => destino,
                    Some(pai) => match traduzidas.get(&pai) {
                        Some(&traduzida) => traduzida,
                        None => {
                            adiadas.push(pasta);
                            continue;
                        }
                    },
                };
                let nova = self
                    .conteudo
                    .garantir_caminho(pai, &partes_do_caminho(&pasta.nome));
                traduzidas.insert(pasta.id, nova);
            }
            faltam = adiadas;
            if faltam.len() == antes {
                break;
            }
        }
        let agora = agora();
        let mut mapa = Mapa::default();
        for (&de, &para) in &traduzidas {
            if let Some(para) = para {
                mapa.pastas.insert(de, para);
            }
        }
        for mut item in itens {
            let de = item.id;
            item.pasta = match item.pasta {
                None => destino,
                Some(pasta) => traduzidas.get(&pasta).copied().unwrap_or(destino),
            };
            item.id = self.conteudo.novo_id();
            // O `uid` que veio de fora só vale se não é de nenhum item daqui: importar põe
            // itens novos, quem junta cópias é o `juntar`.
            if item.uid.is_empty() || self.conteudo.itens.iter().any(|i| i.uid == item.uid) {
                item.uid = uid_novo();
            }
            if item.criado == 0 {
                item.criado = agora;
            }
            item.alterado = agora;
            if de > 0 {
                mapa.itens.insert(de, item.id);
            }
            self.conteudo.itens.push(item);
        }
        self.gravar()?;
        Ok(mapa)
    }

    /// Salva um item novo (`id == 0`) ou atualiza um que existe; ajusta `criado` e `alterado`,
    /// grava e devolve o id.
    pub fn salvar(&mut self, mut item: Item) -> io::Result<u64> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let agora = agora();
        item.alterado = agora;
        if item
            .pasta
            .is_some_and(|id| self.conteudo.pasta(id).is_none())
        {
            item.pasta = None;
        }
        if item.id == 0 {
            item.id = self.conteudo.novo_id();
            item.criado = agora;
            if item.uid.is_empty() || self.conteudo.itens.iter().any(|i| i.uid == item.uid) {
                item.uid = uid_novo();
            }
            self.conteudo.itens.push(item.clone());
        } else {
            let Some(atual) = self.conteudo.itens.iter_mut().find(|i| i.id == item.id) else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    t!("cofre-item-sumiu"),
                ));
            };
            item.criado = atual.criado;
            if item.uid.is_empty() {
                item.uid = atual.uid.clone();
            }
            *atual = item.clone();
        }
        self.gravar()?;
        Ok(item.id)
    }

    /// Apaga um item (um id que não existe não faz nada).
    pub fn remover(&mut self, id: u64) -> io::Result<()> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        if let Some(uid) = self.item(id).map(|i| i.uid.clone()) {
            self.conteudo.lapide(&uid, agora());
        }
        self.conteudo.itens.retain(|i| i.id != id);
        self.gravar()
    }

    /// Os `id` dos arquivos de `vault/` que alguma ficha cita (o que a coleta nunca apaga).
    pub fn anexos_referenciados(&self) -> HashSet<String> {
        self.conteudo
            .itens
            .iter()
            .flat_map(|i| i.anexos.iter().map(|a| a.id.clone()))
            .collect()
    }

    /// Acrescenta a ficha de um anexo ao item e grava. Um nome que o item já tem ganha " (2)",
    /// " (3)"… antes da extensão; devolve o nome que ficou.
    pub fn anexar(&mut self, item: u64, mut anexo: Anexo) -> io::Result<String> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let Some(atual) = self.conteudo.itens.iter_mut().find(|i| i.id == item) else {
            return Err(item_sumiu());
        };
        anexo.nome = nome_de_anexo_unico(&atual.anexos, &anexo.nome, None);
        let nome = anexo.nome.clone();
        atual.anexos.push(anexo);
        atual.alterado = agora();
        self.gravar()?;
        Ok(nome)
    }

    /// Tira da memória as fichas com esses ids, sem gravar: para quando o salvar falhou e os
    /// arquivos que elas apontavam foram apagados (a próxima gravação leva o cofre já limpo).
    pub fn esquecer_anexos(&mut self, ids: &[String]) {
        for item in &mut self.conteudo.itens {
            item.anexos.retain(|a| !ids.contains(&a.id));
        }
    }

    /// Troca o nome de um anexo (repetido, ganha " (2)"…) e grava; devolve o nome que ficou.
    pub fn renomear_anexo(&mut self, item: u64, id: &str, nome: &str) -> io::Result<String> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let Some(atual) = self.conteudo.itens.iter_mut().find(|i| i.id == item) else {
            return Err(item_sumiu());
        };
        let Some(posicao) = atual.anexos.iter().position(|a| a.id == id) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                t!("cofre-anexo-sumiu"),
            ));
        };
        let nome = nome_de_anexo_unico(&atual.anexos, nome.trim(), Some(posicao));
        atual.anexos[posicao].nome = nome.clone();
        atual.alterado = agora();
        self.gravar()?;
        Ok(nome)
    }

    /// Tira a ficha de um anexo do item e grava. O arquivo em `vault/` fica para a coleta.
    pub fn tirar_anexo(&mut self, item: u64, id: &str) -> io::Result<()> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let Some(atual) = self.conteudo.itens.iter_mut().find(|i| i.id == item) else {
            return Err(item_sumiu());
        };
        atual.anexos.retain(|a| a.id != id);
        atual.alterado = agora();
        self.gravar()
    }

    /// `Some` define ou troca a senha mestra; `None` remove. Recifra tudo e grava.
    pub fn definir_senha_mestra(&mut self, nova: Option<&str>) -> io::Result<()> {
        conferir_gravacao(&self.pasta)?;
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        // O que outra máquina gravou entra antes de a proteção mudar; depois não há o que juntar.
        self.garantir_chave()?;
        self.juntar_do_disco()?;
        match nova {
            Some(senha) => {
                let (protecao, chave) = proteger_com_senha(senha)?;
                self.protecao = protecao;
                self.chave = Some(chave);
                self.escrever()?;
                // O arquivo `vault.key` só sai depois do `vault.json` novo estar no disco.
                match fs::remove_file(self.pasta.join(ARQUIVO_CHAVE)) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                }
            }
            None => {
                let chave = aleatorio::<32>()?;
                // O arquivo `vault.key` nasce antes do `vault.json` que depende dele.
                gravar_chave(&self.pasta, &chave)?;
                self.protecao = Protecao::Chave;
                self.chave = Some(chave);
                self.escrever()
            }
        }
    }

    /// Cifra a lista com um nonce novo e grava o `vault.json`. Antes, confere o arquivo: se
    /// outra máquina o mudou desde a última leitura, junta o que ela fez com o que há aqui.
    fn gravar(&mut self) -> io::Result<()> {
        if let Some(motivo) = &self.trancado {
            return Err(io::Error::other(motivo.clone()));
        }
        self.garantir_chave()?;
        if let Some(resumo) = self.juntar_do_disco()? {
            self.conflitos_novos += resumo.conflitos;
        }
        self.escrever()
    }

    /// Quantas notas viraram cópia em conflito numa gravação desde a última chamada. A conferência
    /// periódica avisa dos seus próprios conflitos; estes são os do salvamento, que ninguém viu.
    pub fn tomar_conflitos(&mut self) -> usize {
        std::mem::take(&mut self.conflitos_novos)
    }

    /// Sem senha mestra, a chave nasce na primeira gravação (numa pasta só de leitura, não nasce:
    /// o cofre fica só na memória).
    fn garantir_chave(&mut self) -> io::Result<()> {
        if self.chave.is_some() {
            return Ok(());
        }
        if self.tem_senha_mestra() {
            return Err(bloqueado_erro());
        }
        let chave = match ler_chave(&self.pasta) {
            Some(chave) => chave,
            None if so_leitura(&self.pasta) => return Ok(()),
            None => {
                let chave = aleatorio::<32>()?;
                gravar_chave(&self.pasta, &chave)?;
                chave
            }
        };
        self.chave = Some(chave);
        Ok(())
    }

    /// Se o `vault.json` mudou no disco desde a última leitura ou gravação (outra máquina, pela
    /// sincronização), decifra a versão nova e a junta ao que há aqui. Devolve o que a junção
    /// fez; `None` quando o arquivo não mudou. Não grava: quem chama grava (o `gravar`) ou,
    /// se [`Resumo::a_gravar`], regrava para a outra cópia não ficar para trás.
    pub fn juntar_do_disco(&mut self) -> io::Result<Option<Resumo>> {
        let bytes = match fs::read(self.pasta.join(ARQUIVO)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        if bytes == self.no_disco {
            return Ok(None);
        }
        // Um arquivo que não se lê é substituído pela gravação, como antes.
        let Ok(arquivo) = serde_json::from_slice::<Arquivo>(&bytes) else {
            return Ok(None);
        };
        if arquivo.versao > VERSAO {
            return Err(io::Error::other(t!("cofre-versao-mais-nova")));
        }
        let outra_senha = || {
            io::Error::other(if self.tem_senha_mestra() {
                t!("cofre-alterado-com-outra-senha")
            } else {
                t!("cofre-alterado-com-outra-chave")
            })
        };
        // Sem senha mestra e ainda sem a chave (a outra cópia do app criou o cofre depois de esta
        // abrir), a chave é a do `vault.key` que nasceu com ele.
        let chave = match self.chave {
            Some(chave) => chave,
            None if !self.tem_senha_mestra() && arquivo.protecao == Protecao::Chave => {
                ler_chave(&self.pasta).ok_or_else(outra_senha)?
            }
            None => return Err(bloqueado_erro()),
        };
        if arquivo.protecao != self.protecao {
            return Err(outra_senha());
        }
        let outro = decifrar(&chave, &arquivo.itens)
            .and_then(|claro| conteudo_de(arquivo.versao, &claro))
            .ok_or_else(outra_senha)?;
        self.chave = Some(chave);
        let resumo = self.conteudo.juntar(outro);
        self.no_disco = bytes;
        // Sem nada a regravar, o que há aqui é o que está no disco; com, quem chama grava e o
        // `escrever` anota.
        if !resumo.a_gravar {
            self.conteudo.marcar_vistos();
        }
        Ok(Some(resumo))
    }

    /// O que a [`Cofre::juntar_do_disco`] faz, e regrava se o outro lado ficou para trás.
    /// É a conferência periódica do app (M44).
    pub fn atualizar_do_disco(&mut self) -> io::Result<Option<Resumo>> {
        if self.bloqueado() || self.trancado.is_some() {
            return Ok(None);
        }
        let resumo = self.juntar_do_disco()?;
        if resumo.is_some_and(|r| r.a_gravar) {
            self.escrever()?;
        }
        Ok(resumo)
    }

    /// Uma cópia do conteúdo (itens, pastas e lápides), para juntar a outro cofre.
    pub fn conteudo(&self) -> Conteudo {
        self.conteudo.clone()
    }

    /// Junta `outro` (o conteúdo de uma cópia do cofre, já decifrada) ao que há aqui e grava.
    pub fn juntar(&mut self, outro: Conteudo) -> io::Result<Resumo> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        let resumo = self.conteudo.juntar(outro);
        self.gravar()?;
        Ok(resumo)
    }

    /// Se a coleta de anexos sem ficha pode rodar: o cofre está aberto, foi lido de verdade e tem
    /// itens. Um cofre que abriu vazio por erro (corrompido, sem a chave) faria todo anexo
    /// parecer sem dono.
    pub fn pode_coletar_anexos(&self) -> bool {
        !self.bloqueado() && self.trancado.is_none() && !self.conteudo.itens.is_empty()
    }

    /// A pasta onde estão o `vault.json` e o `vault.key`.
    pub fn pasta_do_cofre(&self) -> &Path {
        &self.pasta
    }

    /// Se o `vault.json` do disco já não é o que foi lido ou gravado por último (outra máquina,
    /// pela sincronização).
    pub fn mudou_no_disco(&self) -> bool {
        match fs::read(self.pasta.join(ARQUIVO)) {
            Ok(bytes) => bytes != self.no_disco,
            Err(_) => false,
        }
    }

    /// O cofre como um `.tirech-vault`, um arquivo só para levar a outra máquina. Com senha
    /// mestra, é o cofre como está. Sem ela, `senha` vazia ou ausente deixa a chave dentro do
    /// arquivo (quem o tiver abre o cofre) e uma `senha` recifra o conteúdo com ela; o cofre
    /// daqui não muda.
    pub fn levar(&self, senha: Option<&str>) -> io::Result<Vec<u8>> {
        if self.bloqueado() {
            return Err(bloqueado_erro());
        }
        if let Some(motivo) = &self.trancado {
            return Err(io::Error::other(motivo.clone()));
        }
        let (protecao, chave) = if self.tem_senha_mestra() {
            (
                self.protecao.clone(),
                self.chave.ok_or_else(bloqueado_erro)?,
            )
        } else if let Some(senha) = senha.filter(|s| !s.is_empty()) {
            proteger_com_senha(senha)?
        } else {
            // Um cofre que nunca gravou ainda não tem chave: o arquivo ganha uma só dele.
            let chave = match self.chave {
                Some(chave) => chave,
                None => aleatorio::<32>()?,
            };
            (
                Protecao::Embutida {
                    chave: STANDARD.encode(chave),
                },
                chave,
            )
        };
        let claro = serde_json::to_vec(&self.conteudo).map_err(io::Error::other)?;
        let arquivo = Arquivo {
            versao: VERSAO,
            protecao,
            itens: cifrar(&chave, &claro)?,
        };
        serde_json::to_vec_pretty(&Portatil {
            formato: FORMATO_PORTATIL,
            arquivo: &arquivo,
        })
        .map_err(io::Error::other)
    }

    /// Junta o cofre de fora (já destravado) a este e grava.
    pub fn juntar_externo(&mut self, externo: &Externo) -> io::Result<Resumo> {
        let conteudo = externo.conteudo.clone().ok_or_else(externo_trancado)?;
        self.juntar(conteudo)
    }

    /// Troca o conteúdo deste cofre pelo de fora. O `vault.json` (e o `vault.key`) de agora ficam
    /// ao lado como `.bak-<data>`; devolve o caminho da cópia do `vault.json`, se havia um. A
    /// proteção daqui continua (senha mestra ou chave); só um cofre bloqueado ou sem a chave
    /// adota a do cofre de fora, e um cofre de fora sem senha ganha uma `vault.key` nova.
    pub fn substituir_por(&mut self, externo: &Externo) -> io::Result<Option<PathBuf>> {
        conferir_gravacao(&self.pasta)?;
        let conteudo = externo.conteudo.clone().ok_or_else(externo_trancado)?;
        let copia = guardar_copia(&self.pasta, false)?;
        if self.bloqueado() || self.trancado.is_some() {
            match (&externo.arquivo.protecao, externo.chave) {
                (protecao @ Protecao::SenhaMestra { .. }, Some(chave)) => {
                    self.protecao = protecao.clone();
                    self.chave = Some(chave);
                    match fs::remove_file(self.pasta.join(ARQUIVO_CHAVE)) {
                        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                        _ => {}
                    }
                }
                _ => {
                    let chave = aleatorio::<32>()?;
                    gravar_chave(&self.pasta, &chave)?;
                    self.protecao = Protecao::Chave;
                    self.chave = Some(chave);
                }
            }
            self.trancado = None;
        } else {
            self.garantir_chave()?;
        }
        self.conteudo = Self::ajusta(conteudo);
        self.escrever()?;
        Ok(copia)
    }

    /// Leva o `vault.json` e o `vault.key` para a pasta `nova` e passa a usá-la; os de agora
    /// ficam como `.bak` (ou `.bak-<data>`, se já há um). O que já houvesse lá vira `.bak-<data>`. A pasta `vault/` dos anexos
    /// quem copia é a tela (em segundo plano, com progresso), antes; aqui a de agora só vira
    /// `vault.bak/` (ou `vault.bak-<data>/`, se já há uma).
    pub fn levar_para(&mut self, nova: &Path) -> io::Result<()> {
        if nova == self.pasta {
            return Ok(());
        }
        conferir_gravacao(&self.pasta)?;
        conferir_gravacao(nova)?;
        fs::create_dir_all(nova)?;
        guardar_copia(nova, true)?;
        for nome in [ARQUIVO, ARQUIVO_CHAVE] {
            let origem = self.pasta.join(nome);
            if origem.exists() {
                gravar_privado(nova, nome, &fs::read(&origem)?)?;
            }
        }
        let sufixo = sufixo_da_copia(&[self.pasta.join(ARQUIVO), self.pasta.join(ARQUIVO_CHAVE)]);
        for nome in [ARQUIVO, ARQUIVO_CHAVE] {
            let origem = self.pasta.join(nome);
            if origem.exists() {
                fs::rename(&origem, self.pasta.join(format!("{nome}{sufixo}")))?;
            }
        }
        let anexos = self.pasta.join(crate::anexos::PASTA);
        if anexos.is_dir() {
            let mut bak = self.pasta.join(format!("{}.bak", crate::anexos::PASTA));
            if bak.exists() {
                bak = self
                    .pasta
                    .join(format!("{}.bak-{}", crate::anexos::PASTA, carimbo()));
            }
            fs::rename(&anexos, &bak)?;
        }
        self.pasta = nova.to_owned();
        Ok(())
    }

    /// Abre a cópia do cofre em `caminho` como [`Externo::abrir`]; se ela tem a mesma senha
    /// mestra deste cofre (e ele está destravado), já vem destravada.
    pub fn abrir_copia(&self, caminho: &Path) -> Result<Externo, String> {
        let mut externo = Externo::abrir(caminho)?;
        if externo.pede_senha()
            && let Some(chave) = self.chave
            && externo.arquivo.protecao == self.protecao
        {
            externo.abrir_com(chave);
        }
        Ok(externo)
    }

    /// As cópias em conflito do `vault.json` que a sincronização deixou na pasta: Syncthing
    /// (`vault.sync-conflict-…json`), Dropbox (`vault (… conflicted copy …).json`) e OneDrive
    /// (`vault-<máquina>.json`).
    pub fn copias_em_conflito(&self) -> Vec<PathBuf> {
        let Ok(lista) = fs::read_dir(&self.pasta) else {
            return Vec::new();
        };
        let mut copias: Vec<PathBuf> = lista
            .flatten()
            .filter(|e| {
                let nome = e.file_name().to_string_lossy().into_owned();
                nome != ARQUIVO && nome.ends_with(".json") && e_copia_em_conflito(&nome)
            })
            .map(|e| e.path())
            .collect();
        copias.sort();
        copias
    }

    /// Guarda uma cópia já juntada ao lado, com o nome acrescido de `.juntado`.
    pub fn arquivar_copia(caminho: &Path) -> io::Result<PathBuf> {
        conferir_gravacao(caminho)?;
        let mut nome = caminho.as_os_str().to_owned();
        nome.push(".juntado");
        let destino = PathBuf::from(nome);
        fs::rename(caminho, &destino)?;
        Ok(destino)
    }

    /// Cifra e grava, sem juntar o que mudou no disco (quem chama já juntou). Antes, as travas
    /// contra perder o que está lá: um `vault.json` de uma versão mais nova do app não é
    /// sobrescrito, um que não se lê vai antes para um `.bak` e o de agora vira a cópia do dia.
    /// Numa pasta só de leitura ([`so_leitura`]) não grava nada: o que mudou fica na memória e se
    /// perde ao fechar, e o `no_disco` continua o do arquivo, para juntar o que a outra cópia do
    /// app gravar.
    fn escrever(&mut self) -> io::Result<()> {
        if so_leitura(&self.pasta) {
            // Numa cópia secundária, o arquivo cifrado vai para a principal gravar, se ainda for
            // o que se juntou aqui (`no_disco`). Se não for, a conferência periódica junta o
            // novo e manda de novo.
            let arquivo = self.pasta.join(ARQUIVO);
            if let Some(chave) = self.chave
                && crate::instancias::segue_a_principal(&arquivo)
            {
                self.conteudo.podar(agora());
                let bytes = self.cifrado(&chave)?;
                crate::instancias::enviar_pronto(&arquivo, &self.no_disco, bytes);
            }
            return Ok(());
        }
        let chave = self.chave.ok_or_else(bloqueado_erro)?;
        let arquivo = self.pasta.join(ARQUIVO);
        if let Ok(atual) = fs::read(&arquivo)
            && atual != self.no_disco
        {
            if versao_de(&atual).is_some_and(|v| v > VERSAO) {
                return Err(io::Error::other(t!("cofre-versao-mais-nova")));
            }
            if serde_json::from_slice::<Arquivo>(&atual).is_err() {
                guardar_ilegivel(&arquivo)?;
            }
        }
        self.copia_do_dia(&self.no_disco);
        self.conteudo.podar(agora());
        let bytes = self.cifrado(&chave)?;
        gravar_privado(&self.pasta, ARQUIVO, &bytes)?;
        self.no_disco = bytes;
        self.conteudo.marcar_vistos();
        Ok(())
    }
}

impl Cofre {
    /// O `vault.json` do conteúdo de agora, cifrado com um nonce novo.
    fn cifrado(&self, chave: &Chave) -> io::Result<Vec<u8>> {
        let claro = serde_json::to_vec(&self.conteudo).map_err(io::Error::other)?;
        let arquivo = Arquivo {
            versao: VERSAO,
            protecao: self.protecao.clone(),
            itens: cifrar(chave, &claro)?,
        };
        serde_json::to_vec_pretty(&arquivo).map_err(io::Error::other)
    }

    /// Se a principal pode gravar o `vault.json` que uma cópia secundária mandou: um cofre que
    /// esta versão lê, com a mesma proteção (a mesma senha mestra, ou a mesma chave) deste.
    pub fn aceita_de_outra_copia(&self, bytes: &[u8]) -> Result<(), String> {
        let arquivo = serde_json::from_slice::<Arquivo>(bytes).map_err(|e| e.to_string())?;
        if arquivo.versao > VERSAO {
            return Err(t!("cofre-versao-mais-nova"));
        }
        let outra_chave = self
            .chave
            .is_some_and(|chave| decifrar(&chave, &arquivo.itens).is_none());
        if arquivo.protecao != self.protecao || outra_chave {
            return Err(if self.tem_senha_mestra() {
                t!("cofre-alterado-com-outra-senha")
            } else {
                t!("cofre-alterado-com-outra-chave")
            });
        }
        Ok(())
    }
}

/// Se `nome` (de um `.json` da pasta do cofre) é uma cópia que a sincronização deixou.
fn e_copia_em_conflito(nome: &str) -> bool {
    (nome.contains(".sync-conflict-") && (nome.starts_with("vault") || nome.starts_with("cofre")))
        || (nome.starts_with("vault") && nome.contains("conflicted copy"))
        || nome.starts_with("vault-")
}

/// O campo `versao` de um `vault.json`, mesmo de um formato que este código não entende inteiro.
fn versao_de(bytes: &[u8]) -> Option<u32> {
    #[derive(Deserialize)]
    struct SoVersao {
        versao: u32,
    }
    serde_json::from_slice::<SoVersao>(bytes)
        .ok()
        .map(|a| a.versao)
}

/// O erro de usar o cofre de fora antes de destravá-lo.
fn externo_trancado() -> io::Error {
    io::Error::other(t!("cofre-de-fora-trancado"))
}

/// Guarda o `vault.json` e o `vault.key` da `pasta` como `.bak-<data>` (copiando, ou movendo
/// com `mover`). Devolve o caminho da cópia do `vault.json`, se havia um.
fn guardar_copia(pasta: &Path, mover: bool) -> io::Result<Option<PathBuf>> {
    let carimbo = carimbo();
    let mut copia = None;
    for nome in [ARQUIVO, ARQUIVO_CHAVE] {
        let origem = pasta.join(nome);
        if !origem.exists() {
            continue;
        }
        let destino = pasta.join(format!("{nome}.bak-{carimbo}"));
        if mover {
            fs::rename(&origem, &destino)?;
        } else {
            fs::copy(&origem, &destino)?;
        }
        if nome == ARQUIVO {
            copia = Some(destino);
        }
    }
    Ok(copia)
}

/// O que se vê de um cofre de fora antes de decidir o que fazer com ele.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResumoDoCofre {
    /// Quantos itens tem.
    pub itens: usize,
    /// Quantos deles são chaves SSH.
    pub chaves_ssh: usize,
    /// Quando algo mudou pela última vez (segundos desde a época UNIX); 0 sem itens.
    pub alterado: u64,
}

/// Um cofre que está fora da pasta do app: um `.tirech-vault` levado de outra máquina, o
/// `vault.json` de outra pasta ou uma cópia em conflito. Fica fechado até a senha, se pedir uma.
pub struct Externo {
    arquivo: Arquivo,
    chave: Option<Chave>,
    conteudo: Option<Conteudo>,
}

impl Externo {
    /// Lê o arquivo. Um `vault.json` sem senha mestra procura o `vault.key` na mesma pasta.
    pub fn abrir(caminho: &Path) -> Result<Externo, String> {
        let bytes = fs::read(caminho).map_err(|e| {
            t!(
                "comum-nao-consegui-ler",
                arquivo = caminho.display().to_string(),
                erro = e.to_string()
            )
        })?;
        let nao_e = || t!("cofre-externo-nao-e-cofre");
        let arquivo: Arquivo = serde_json::from_slice(&bytes).map_err(|_| nao_e())?;
        if arquivo.versao == 0 {
            return Err(nao_e());
        }
        if arquivo.versao > VERSAO {
            return Err(t!("cofre-externo-versao-mais-nova"));
        }
        let mut externo = Externo {
            arquivo,
            chave: None,
            conteudo: None,
        };
        let chave = match &externo.arquivo.protecao {
            Protecao::SenhaMestra { .. } => return Ok(externo),
            Protecao::Embutida { chave } => {
                chave_embutida(chave).ok_or_else(|| t!("cofre-externo-chave-estragada"))?
            }
            Protecao::Chave => {
                let pasta = caminho.parent().unwrap_or_else(|| Path::new("."));
                ler_chave(pasta).ok_or_else(sem_chave)?
            }
        };
        if !externo.abrir_com(chave) {
            return Err(t!("cofre-externo-chave-nao-abre"));
        }
        Ok(externo)
    }

    fn abrir_com(&mut self, chave: Chave) -> bool {
        let conteudo = decifrar(&chave, &self.arquivo.itens)
            .and_then(|claro| conteudo_de(self.arquivo.versao, &claro))
            .map(Cofre::ajusta);
        match conteudo {
            Some(conteudo) => {
                self.chave = Some(chave);
                self.conteudo = Some(conteudo);
                true
            }
            None => false,
        }
    }

    /// Se o cofre pede a senha mestra e ainda não foi destravado.
    pub fn pede_senha(&self) -> bool {
        self.conteudo.is_none()
    }

    /// Destrava com a senha mestra do cofre de fora; `false` se não confere.
    pub fn desbloquear(&mut self, senha: &str) -> bool {
        match chave_da_senha(&self.arquivo.protecao, senha) {
            Some(chave) => self.abrir_com(chave),
            None => false,
        }
    }

    /// Os `id`s dos anexos que os itens do cofre de fora citam (vazio enquanto trancado).
    pub fn anexos_referenciados(&self) -> HashSet<String> {
        self.conteudo
            .iter()
            .flat_map(|c| &c.itens)
            .flat_map(|i| i.anexos.iter().map(|a| a.id.clone()))
            .collect()
    }

    /// Os números do cofre, depois de destravado.
    pub fn resumo(&self) -> Option<ResumoDoCofre> {
        let conteudo = self.conteudo.as_ref()?;
        Some(ResumoDoCofre {
            itens: conteudo.itens.len(),
            chaves_ssh: conteudo
                .itens
                .iter()
                .filter(|i| i.tipo == Tipo::ChaveSsh)
                .count(),
            alterado: conteudo
                .itens
                .iter()
                .map(|i| i.alterado)
                .chain(conteudo.pastas.iter().map(|p| p.alterado))
                .max()
                .unwrap_or(0),
        })
    }
}

/// Uma função que roda antes de o cofre bloquear.
type Gancho = Rc<dyn Fn()>;

/// O cofre da tela: o mesmo cofre para a árvore, as linhas "Credencial" e os diálogos, e uma
/// versão que sobe a cada mudança para eles se refazerem.
#[derive(Clone)]
pub struct CofreCompartilhado {
    cofre: Rc<RefCell<Cofre>>,
    versao: Signal<u64>,
    guarda: Rc<RefCell<Option<Guarda>>>,
    /// O que roda no começo de [`CofreCompartilhado::bloquear`] (a nota aberta grava o rascunho).
    antes_de_bloquear: Rc<RefCell<Vec<Gancho>>>,
    /// Onde a coleta de anexos sem ficha anota o que viu (`None`: não coleta).
    registro_de_orfaos: Rc<RefCell<Option<PathBuf>>>,
    /// Quando a coleta rodou pela última vez.
    ultima_coleta: Rc<Cell<Option<Instant>>>,
}

/// De quanto em quanto tempo a coleta de anexos sem ficha roda, no máximo, depois de gravações.
const INTERVALO_DA_COLETA: Duration = Duration::from_secs(10 * 60);

/// O que protege o cofre enquanto o app roda: a limpeza da área de transferência e o bloqueio
/// sem uso (`cofre.limpar_area_transferencia_secs` e `cofre.bloquear_apos_min`).
struct Guarda {
    config: Signal<Configuracoes>,
    alarmes: Alarmes,
    ultimo_uso: Rc<Cell<Instant>>,
    armado: Rc<Cell<bool>>,
}

impl CofreCompartilhado {
    /// Compartilha `cofre`.
    pub fn new(cofre: Cofre) -> Self {
        Self {
            cofre: Rc::new(RefCell::new(cofre)),
            versao: signal(0),
            guarda: Rc::default(),
            antes_de_bloquear: Rc::default(),
            registro_de_orfaos: Rc::default(),
            ultima_coleta: Rc::default(),
        }
    }

    /// Liga a coleta dos arquivos de `vault/` que nenhuma ficha cita: o `registro` (fora do cofre,
    /// que a sincronização não leva) guarda quando cada um foi visto pela primeira vez. Roda já
    /// e depois de gravações do cofre, de tempos em tempos, numa thread; só com o cofre aberto.
    pub fn coletar_anexos(&self, registro: PathBuf) {
        *self.registro_de_orfaos.borrow_mut() = Some(registro);
        self.coletar_se_preciso(true);
    }

    fn coletar_se_preciso(&self, forcar: bool) {
        let Some(registro) = self.registro_de_orfaos.borrow().clone() else {
            return;
        };
        if !forcar
            && self
                .ultima_coleta
                .get()
                .is_some_and(|t| t.elapsed() < INTERVALO_DA_COLETA)
        {
            return;
        }
        let Some((vault, referenciados)) = self.agora(|c| {
            c.pode_coletar_anexos().then(|| {
                (
                    c.pasta_do_cofre().join(crate::anexos::PASTA),
                    c.anexos_referenciados(),
                )
            })
        }) else {
            return;
        };
        if !vault.is_dir() {
            return;
        }
        self.ultima_coleta.set(Some(Instant::now()));
        std::thread::spawn(move || {
            crate::anexos::coletar(
                &vault,
                &referenciados,
                &registro,
                agora(),
                crate::anexos::MARGEM_ORFAO,
            );
        });
    }

    /// Liga a limpeza da área de transferência e o bloqueio sem uso às configurações. Sem
    /// isto, o cofre não faz nenhum dos dois.
    pub fn proteger(&self, config: Signal<Configuracoes>, alarmes: Alarmes) {
        *self.guarda.borrow_mut() = Some(Guarda {
            config,
            alarmes,
            ultimo_uso: Rc::new(Cell::new(Instant::now())),
            armado: Rc::default(),
        });
        self.marcar_uso();
    }

    /// As opções do gerador de senhas de `cofre.gerador` (as de fábrica sem configurações).
    pub fn gerador_padrao(&self) -> Gerador {
        let guarda = self.guarda.borrow();
        let Some(g) = guarda.as_ref() else {
            return Gerador::default();
        };
        g.config.with_untracked(|c| {
            let cfg = &c.cofre.gerador;
            Gerador {
                tamanho: (cfg.tamanho as usize).clamp(Gerador::MIN, Gerador::MAX),
                maiusculas: cfg.maiusculas,
                minusculas: cfg.minusculas,
                digitos: cfg.digitos,
                simbolos: cfg.simbolos,
                sem_ambiguos: cfg.sem_ambiguos,
            }
        })
    }

    /// Anota que um segredo acabou de ser usado: o bloqueio sem uso conta daqui.
    pub fn marcar_uso(&self) {
        let guarda = self.guarda.borrow();
        let Some(g) = guarda.as_ref() else { return };
        g.ultimo_uso.set(Instant::now());
        let minutos = g.config.with_untracked(|c| c.cofre.bloquear_apos_min);
        if minutos == 0 || g.armado.get() || !self.agora(Cofre::tem_senha_mestra) {
            return;
        }
        g.armado.set(true);
        self.armar_bloqueio(g, Duration::from_secs(u64::from(minutos) * 60));
    }

    fn armar_bloqueio(&self, g: &Guarda, espera: Duration) {
        let (cofre, config, ultimo, armado) = (
            self.clone(),
            g.config,
            Rc::clone(&g.ultimo_uso),
            Rc::clone(&g.armado),
        );
        g.alarmes.depois(espera, move || {
            let limite = Duration::from_secs(
                u64::from(config.with_untracked(|c| c.cofre.bloquear_apos_min)) * 60,
            );
            let parado = ultimo.get().elapsed();
            if limite.is_zero() {
                armado.set(false);
            } else if parado >= limite {
                armado.set(false);
                cofre.bloquear();
            } else {
                // Houve uso no meio: espera o que falta.
                let guarda = cofre.guarda.borrow();
                if let Some(g) = guarda.as_ref() {
                    cofre.armar_bloqueio(g, limite - parado);
                }
            }
        });
    }

    /// Copia um segredo para a área de transferência e, se as configurações pedem, apaga depois
    /// de um tempo (só se a área ainda tem o que o app copiou).
    pub fn copiar_segredo(&self, texto: String) {
        self.marcar_uso();
        clipboard::write(texto.clone());
        let guarda = self.guarda.borrow();
        let Some(g) = guarda.as_ref() else { return };
        let segundos = g
            .config
            .with_untracked(|c| c.cofre.limpar_area_transferencia_secs);
        if segundos > 0 {
            g.alarmes
                .depois(Duration::from_secs(u64::from(segundos)), move || {
                    clipboard::clear_if(texto)
                });
        }
    }

    /// Registra o que roda no começo de [`CofreCompartilhado::bloquear`], antes de o cofre
    /// esquecer a chave: a página de notas grava o que está escrito e cancela os anexos em
    /// andamento.
    pub fn antes_de_bloquear(&self, f: impl Fn() + 'static) {
        self.antes_de_bloquear.borrow_mut().push(Rc::new(f));
    }

    /// Bloqueia o cofre (o manual, o da bandeja e o por tempo): primeiro roda o que pediu para
    /// ser avisado, depois apaga a chave e o conteúdo da memória.
    pub fn bloquear(&self) {
        // Uma cópia da lista: quem é avisado pode registrar outro aviso ou mudar o cofre.
        let avisos: Vec<Gancho> = self.antes_de_bloquear.borrow().clone();
        for aviso in avisos {
            aviso();
        }
        self.mudar(|c| c.bloquear());
    }

    /// Lê o cofre. Dentro de um efeito ou de uma view, o `ler` passa a depender da versão.
    pub fn ler<R>(&self, f: impl FnOnce(&Cofre) -> R) -> R {
        self.versao.get();
        f(&self.cofre.borrow())
    }

    /// Lê o cofre sem depender da versão (eventos e ações).
    pub fn agora<R>(&self, f: impl FnOnce(&Cofre) -> R) -> R {
        f(&self.cofre.borrow())
    }

    /// Muda o cofre e sobe a versão (mesmo se `f` falhar: a tela relê o que ficou).
    pub fn mudar<R>(&self, f: impl FnOnce(&mut Cofre) -> R) -> R {
        let r = f(&mut self.cofre.borrow_mut());
        self.versao.update(|v| *v += 1);
        if !self.agora(Cofre::bloqueado) {
            self.marcar_uso();
            self.coletar_se_preciso(false);
        }
        r
    }

    /// Esvazia a contagem de notas que viraram cópia em conflito no salvamento ([`Cofre::tomar_conflitos`]). Não sobe a versão: quem chama já reagiu a ela.
    pub fn tomar_conflitos(&self) -> usize {
        self.cofre.borrow_mut().tomar_conflitos()
    }

    /// A versão, para quem só quer saber que algo mudou.
    pub fn versao(&self) -> u64 {
        self.versao.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    fn item(titulo: &str) -> Item {
        Item {
            tipo: Tipo::Ssh,
            titulo: titulo.to_owned(),
            usuario: "ana-usuaria".to_owned(),
            senha: "s3gredo-bem-unico".to_owned(),
            url: "host.exemplo:22".to_owned(),
            ..Item::default()
        }
    }

    fn texto_do_cofre(pasta: &Path) -> String {
        fs::read_to_string(pasta.join("vault.json")).unwrap()
    }

    #[test]
    fn so_o_item_de_rede_com_a_identidade_a_tem() {
        let a_mao = Item {
            tipo: Tipo::OpenVpn,
            ..item("openvpn-aws")
        };
        assert!(!a_mao.tem_identidade_de_rede());
        let importado = Item {
            openvpn: Some(IdentidadeOpenVpn::default()),
            ..a_mao.clone()
        };
        assert!(importado.tem_identidade_de_rede());
        // A identidade de outro tipo não conta.
        let trocado = Item {
            tipo: Tipo::WireGuard,
            ..importado
        };
        assert!(!trocado.tem_identidade_de_rede());
        assert!(!item("ssh").tem_identidade_de_rede());
    }

    #[test]
    fn ida_e_volta_da_cifra() {
        let chave = [7u8; 32];
        let segredo = cifrar(&chave, b"ola mundo").unwrap();
        assert_eq!(
            decifrar(&chave, &segredo).as_deref(),
            Some(&b"ola mundo"[..])
        );
        assert_eq!(decifrar(&[8u8; 32], &segredo), None);
    }

    #[test]
    fn o_nonce_muda_a_cada_gravacao() {
        let pasta = PastaTemporaria::nova("nonce");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        let a = texto_do_cofre(&pasta.0);
        cofre.gravar().unwrap();
        let b = texto_do_cofre(&pasta.0);
        assert_ne!(a, b);
        let (a, b): (Arquivo, Arquivo) = (
            serde_json::from_str(&a).unwrap(),
            serde_json::from_str(&b).unwrap(),
        );
        assert_ne!(a.itens.nonce, b.itens.nonce);
        assert_ne!(a.itens.dados, b.itens.dados);
    }

    #[test]
    fn o_arquivo_nao_tem_nada_em_claro() {
        let pasta = PastaTemporaria::nova("claro");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("titulo-do-servidor")).unwrap();
        let texto = texto_do_cofre(&pasta.0);
        for claro in [
            "titulo-do-servidor",
            "ana-usuaria",
            "s3gredo-bem-unico",
            "host.exemplo",
        ] {
            assert!(!texto.contains(claro), "{claro} apareceu em claro");
        }
        let (reaberto, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert_eq!(reaberto.itens()[0].senha, "s3gredo-bem-unico");
    }

    #[cfg(unix)]
    #[test]
    fn o_arquivo_chave_nasce_com_permissao_0600() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = PastaTemporaria::nova("modo");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        let modo = fs::metadata(pasta.0.join("vault.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(modo & 0o777, 0o600);
    }

    #[test]
    fn sem_arquivo_o_cofre_nasce_vazio_e_desbloqueado() {
        let pasta = PastaTemporaria::nova("vazio");
        let (cofre, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert!(!cofre.bloqueado() && !cofre.tem_senha_mestra());
        assert!(cofre.itens().is_empty());
        assert!(!pasta.0.join("vault.json").exists());
    }

    #[test]
    fn salvar_ajusta_datas_e_atualiza_pelo_id() {
        let pasta = PastaTemporaria::nova("salvar");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let id = cofre.salvar(item("web")).unwrap();
        let criado = cofre.item(id).unwrap().criado;
        assert!(criado > 0);
        let mut novo = cofre.item(id).unwrap().clone();
        novo.titulo = "web 2".to_owned();
        novo.criado = 1;
        assert_eq!(cofre.salvar(novo).unwrap(), id);
        let salvo = cofre.item(id).unwrap();
        assert_eq!((salvo.titulo.as_str(), salvo.criado), ("web 2", criado));
        assert_eq!(cofre.itens().len(), 1);
        let inexistente = Item {
            id: 99,
            ..item("x")
        };
        assert!(cofre.salvar(inexistente).is_err());
    }

    #[test]
    fn ids_nunca_sao_reaproveitados() {
        let pasta = PastaTemporaria::nova("ids");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let a = cofre.salvar(item("a")).unwrap();
        let b = cofre.salvar(item("b")).unwrap();
        cofre.remover(b).unwrap();
        let c = cofre.salvar(item("c")).unwrap();
        assert!(c > b && b > a);
        let (mut reaberto, _) = Cofre::abrir(&pasta.0);
        reaberto.remover(c).unwrap();
        let d = reaberto.salvar(item("d")).unwrap();
        assert!(d > c);
    }

    #[test]
    fn senha_mestra_certa_desbloqueia_e_errada_nao_mexe_no_arquivo() {
        let pasta = PastaTemporaria::nova("mestra");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        cofre.definir_senha_mestra(Some("a mestra")).unwrap();
        assert!(
            !pasta.0.join("vault.key").exists(),
            "o arquivo chave deveria sumir"
        );

        let antes = texto_do_cofre(&pasta.0);
        let (mut novo, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert!(novo.bloqueado() && novo.tem_senha_mestra());
        assert!(novo.itens().is_empty());
        assert!(!novo.desbloquear("outra"));
        assert!(novo.bloqueado());
        assert_eq!(texto_do_cofre(&pasta.0), antes);
        assert!(!pasta.0.join("vault.json.bak").exists());

        assert!(novo.desbloquear("a mestra"));
        assert!(!novo.bloqueado());
        assert_eq!(novo.itens()[0].titulo, "web");
        novo.bloquear();
        assert!(novo.bloqueado() && novo.itens().is_empty());
    }

    #[test]
    fn bloqueado_recusa_escrever() {
        let pasta = PastaTemporaria::nova("recusa");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.definir_senha_mestra(Some("m")).unwrap();
        let (mut novo, _) = Cofre::abrir(&pasta.0);
        for erro in [
            novo.salvar(item("x")).unwrap_err(),
            novo.remover(1).unwrap_err(),
            novo.definir_senha_mestra(None).unwrap_err(),
        ] {
            assert_eq!(erro.kind(), io::ErrorKind::PermissionDenied);
            assert_eq!(erro.to_string(), "O cofre está bloqueado");
        }
    }

    #[test]
    fn trocar_e_remover_a_senha_mestra_recifram() {
        let pasta = PastaTemporaria::nova("trocar");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        cofre.definir_senha_mestra(Some("um")).unwrap();
        cofre.definir_senha_mestra(Some("dois")).unwrap();

        let (mut novo, _) = Cofre::abrir(&pasta.0);
        assert!(!novo.desbloquear("um"));
        assert!(novo.desbloquear("dois"));
        novo.definir_senha_mestra(None).unwrap();
        assert!(pasta.0.join("vault.key").exists());

        let (sozinho, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert!(!sozinho.bloqueado() && !sozinho.tem_senha_mestra());
        assert_eq!(sozinho.itens()[0].senha, "s3gredo-bem-unico");
    }

    #[test]
    fn segredo_adulterado_vira_aviso_e_copia() {
        let pasta = PastaTemporaria::nova("adulterado");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        let mut arquivo: Arquivo = serde_json::from_str(&texto_do_cofre(&pasta.0)).unwrap();
        let mut bytes = STANDARD.decode(&arquivo.itens.dados).unwrap();
        bytes[0] ^= 0xff;
        arquivo.itens.dados = STANDARD.encode(bytes);
        fs::write(
            pasta.0.join("vault.json"),
            serde_json::to_string(&arquivo).unwrap(),
        )
        .unwrap();

        let (vazio, aviso) = Cofre::abrir(&pasta.0);
        let aviso = aviso.expect("deveria avisar");
        assert!(aviso.starts_with("O cofre de senhas estava corrompido; guardei uma cópia em "));
        assert!(vazio.itens().is_empty());
        assert!(pasta.0.join("vault.json.bak").exists());
        assert!(pasta.0.join("vault.key.bak").exists());
        assert!(!pasta.0.join("vault.json").exists());
        // E dá para recomeçar.
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("novo")).unwrap();
        assert_eq!(Cofre::abrir(&pasta.0).0.itens().len(), 1);
    }

    #[test]
    fn json_invalido_e_versao_desconhecida_viram_copia() {
        // A versão 0 nunca existiu. Uma mais nova que esta não vira cópia: fica onde está, e o
        // cofre abre trancado (`cofre_de_versao_mais_nova_abre_trancado_sem_mexer_no_arquivo`).
        for (nome, texto) in [
            ("json", "isto não é json".to_owned()),
            (
                "versao",
                r#"{"versao":0,"protecao":{"tipo":"chave"},"itens":{"nonce":"","dados":""}}"#
                    .to_owned(),
            ),
        ] {
            let pasta = PastaTemporaria::nova(nome);
            fs::create_dir_all(&pasta.0).unwrap();
            fs::write(pasta.0.join("vault.json"), texto).unwrap();
            let (cofre, aviso) = Cofre::abrir(&pasta.0);
            assert!(aviso.is_some(), "{nome}");
            assert!(cofre.itens().is_empty());
            assert!(pasta.0.join("vault.json.bak").exists(), "{nome}");
        }
    }

    #[test]
    fn compartilhado_sobe_a_versao_ao_mudar() {
        let pasta = PastaTemporaria::nova("compartilhado");
        let compartilhado = CofreCompartilhado::new(Cofre::abrir(&pasta.0).0);
        let antes = compartilhado.versao();
        let id = compartilhado.mudar(|c| c.salvar(item("web"))).unwrap();
        assert!(compartilhado.versao() > antes);
        assert_eq!(
            compartilhado.agora(|c| c.item(id).map(|i| i.titulo.clone())),
            Some("web".into())
        );
    }

    #[test]
    fn nomes_dos_tipos() {
        let nomes: Vec<String> = Tipo::TODOS.iter().map(|t| t.nome()).collect();
        assert_eq!(
            nomes,
            [
                "Senha",
                "SSH",
                "SFTP",
                "FTP",
                "S3",
                "rclone",
                "Passphrase de chave",
                "Chave SSH",
                "Nota",
                "Identidade Nebula",
                "Identidade OpenVPN",
                "Identidade WireGuard",
                "Identidade Tailscale",
                "Identidade ZeroTier"
            ]
        );
        assert_eq!(Tipo::do_nome("rclone"), Some(Tipo::Rclone));
        assert_eq!(Tipo::do_nome("nada"), None);
    }

    // --- Pastas (M39) ---

    /// Itens de teste soltos numa pasta: `(pasta como texto do v1, título)`.
    fn conteudo_v1(itens: &[(&str, &str)]) -> Vec<u8> {
        let lista: Vec<serde_json::Value> = itens
            .iter()
            .enumerate()
            .map(|(n, (pasta, titulo))| {
                serde_json::json!({
                    "id": n + 1, "tipo": "Senha", "titulo": titulo, "pasta": pasta,
                    "usuario": "", "senha": format!("senha-{n}"), "url": "", "notas": "",
                    "campos": [], "alterado": 0,
                })
            })
            .collect();
        serde_json::json!({ "proximo_id": itens.len() + 1, "itens": lista })
            .to_string()
            .into_bytes()
    }

    const ITENS_V1: [(&str, &str); 4] = [
        ("Clientes/ACME", "Painel"),
        ("Clientes", "Contrato"),
        ("", "Solto"),
        (" Pessoal / ", "Banco"),
    ];

    /// Confere o que o `ITENS_V1` vira: Clientes › ACME e Pessoal, cada item na sua.
    fn confere_o_v1(cofre: &Cofre) {
        let nomes: Vec<String> = cofre.pastas().iter().map(|p| p.nome.clone()).collect();
        assert_eq!(nomes.len(), 3, "{nomes:?}");
        let onde = |titulo: &str| {
            let item = cofre.itens().iter().find(|i| i.titulo == titulo).unwrap();
            cofre.caminho(item.pasta)
        };
        assert_eq!(onde("Painel"), "Clientes/ACME");
        assert_eq!(onde("Contrato"), "Clientes");
        assert_eq!(onde("Solto"), "");
        assert_eq!(onde("Banco"), "Pessoal");
    }

    fn versao_do_arquivo(pasta: &Path) -> u32 {
        let arquivo: Arquivo = serde_json::from_str(&texto_do_cofre(pasta)).unwrap();
        arquivo.versao
    }

    #[test]
    fn migrar_do_v1() {
        let pasta = PastaTemporaria::nova("v1");
        fs::create_dir_all(&pasta.0).unwrap();
        let chave = [3u8; 32];
        gravar_chave(&pasta.0, &chave).unwrap();
        let arquivo = Arquivo {
            versao: 1,
            protecao: Protecao::Chave,
            itens: cifrar(&chave, &conteudo_v1(&ITENS_V1)).unwrap(),
        };
        fs::write(
            pasta.0.join("vault.json"),
            serde_json::to_string(&arquivo).unwrap(),
        )
        .unwrap();

        let (mut cofre, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert!(!pasta.0.join("vault.json.bak").exists());
        confere_o_v1(&cofre);
        assert_eq!(versao_do_arquivo(&pasta.0), 1, "só muda na gravação");

        let id = cofre.salvar(item("novo")).unwrap();
        assert!(id > 4 + 3, "os ids novos não esbarram nos das pastas: {id}");
        assert_eq!(versao_do_arquivo(&pasta.0), VERSAO);
        let (reaberto, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert_eq!(reaberto.itens().len(), 5);
        assert_eq!(reaberto.pastas().len(), 3);
    }

    #[test]
    fn migrar_do_v1_com_senha_mestra() {
        let pasta = PastaTemporaria::nova("v1-mestra");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        cofre.definir_senha_mestra(Some("m")).unwrap();
        // Troca o conteúdo cifrado por um v1, com a mesma chave.
        let chave = cofre.chave.unwrap();
        let mut arquivo: Arquivo = serde_json::from_str(&texto_do_cofre(&pasta.0)).unwrap();
        arquivo.versao = 1;
        arquivo.itens = cifrar(&chave, &conteudo_v1(&ITENS_V1)).unwrap();
        fs::write(
            pasta.0.join("vault.json"),
            serde_json::to_string(&arquivo).unwrap(),
        )
        .unwrap();

        let (mut cofre, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert!(cofre.bloqueado());
        assert!(cofre.desbloquear("m"));
        confere_o_v1(&cofre);
        assert!(!pasta.0.join("vault.json.bak").exists());
        cofre.salvar(item("novo")).unwrap();
        assert_eq!(versao_do_arquivo(&pasta.0), VERSAO);
    }

    #[test]
    fn migrar_nomes() {
        let bytes = |pasta: &Path, nome: &str| fs::read(pasta.join(nome)).unwrap();
        // Sem senha mestra: o cofre e a chave trocam de nome, com os mesmos bytes.
        let origem = PastaTemporaria::nova("nomes-origem");
        let (mut cofre, _) = Cofre::abrir(&origem.0);
        cofre.salvar(item("web")).unwrap();
        let (json, chave) = (
            bytes(&origem.0, "vault.json"),
            bytes(&origem.0, "vault.key"),
        );

        let pasta = PastaTemporaria::nova("nomes");
        fs::create_dir_all(&pasta.0).unwrap();
        fs::write(pasta.0.join("cofre.json"), &json).unwrap();
        fs::write(pasta.0.join("chave"), &chave).unwrap();
        let (aberto, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert_eq!(aberto.itens()[0].titulo, "web");
        assert!(!pasta.0.join("cofre.json").exists() && !pasta.0.join("chave").exists());
        assert_eq!(bytes(&pasta.0, "vault.json"), json);
        assert_eq!(bytes(&pasta.0, "vault.key"), chave);

        // Com senha mestra: só o cofre.
        let mestra = PastaTemporaria::nova("nomes-mestra");
        let (mut cofre, _) = Cofre::abrir(&mestra.0);
        cofre.salvar(item("web")).unwrap();
        cofre.definir_senha_mestra(Some("m")).unwrap();
        let json = bytes(&mestra.0, "vault.json");
        fs::rename(mestra.0.join("vault.json"), mestra.0.join("cofre.json")).unwrap();
        let (mut aberto, aviso) = Cofre::abrir(&mestra.0);
        assert_eq!(aviso, None);
        assert!(aberto.desbloquear("m"));
        assert_eq!(aberto.itens()[0].titulo, "web");
        assert_eq!(bytes(&mestra.0, "vault.json"), json);

        // Com o antigo e o novo, nenhum muda.
        let dupla = PastaTemporaria::nova("nomes-dupla");
        fs::create_dir_all(&dupla.0).unwrap();
        fs::write(dupla.0.join("cofre.json"), b"antigo").unwrap();
        fs::write(dupla.0.join("vault.json"), b"novo").unwrap();
        super::migrar_nomes(&dupla.0);
        assert_eq!(bytes(&dupla.0, "cofre.json"), b"antigo");
        assert_eq!(bytes(&dupla.0, "vault.json"), b"novo");

        // Sem nenhum, nada nasce.
        let vazia = PastaTemporaria::nova("nomes-vazia");
        let (cofre, aviso) = Cofre::abrir(&vazia.0);
        assert_eq!((aviso, cofre.itens().len()), (None, 0));
        assert!(!vazia.0.join("vault.json").exists() && !vazia.0.join("cofre.json").exists());
    }

    #[test]
    fn nova_pasta_repete_o_nome_com_numero() {
        let pasta = PastaTemporaria::nova("nova-pasta");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let a = cofre.nova_pasta(None, "Nova pasta").unwrap();
        let b = cofre.nova_pasta(None, "Nova pasta").unwrap();
        let filha = cofre.nova_pasta(Some(a), "Nova pasta").unwrap();
        assert_eq!(cofre.pasta(a).unwrap().nome, "Nova pasta");
        assert_eq!(cofre.pasta(b).unwrap().nome, "Nova pasta (2)");
        assert_eq!(
            cofre.pasta(filha).unwrap().nome,
            "Nova pasta",
            "dentro de outra, vale"
        );
        assert!(cofre.nova_pasta(None, "  ").is_err());
        assert!(cofre.nova_pasta(Some(999), "x").is_err());
        let (reaberto, _) = Cofre::abrir(&pasta.0);
        assert_eq!(reaberto.pastas().len(), 3);
    }

    #[test]
    fn renomear_pasta_recusa_vazio_barra_e_irma_repetida() {
        let pasta = PastaTemporaria::nova("renomear-pasta");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let a = cofre.nova_pasta(None, "A").unwrap();
        let b = cofre.nova_pasta(None, "B").unwrap();
        cofre.renomear_pasta(a, "  Alfa  ").unwrap();
        assert_eq!(cofre.pasta(a).unwrap().nome, "Alfa");
        for ruim in ["", "   ", "x/y"] {
            assert!(cofre.renomear_pasta(a, ruim).is_err(), "{ruim:?}");
        }
        assert!(
            cofre.renomear_pasta(b, "alfa").is_err(),
            "irmã com o mesmo nome"
        );
        cofre.renomear_pasta(a, "ALFA").unwrap();
        assert!(cofre.renomear_pasta(999, "x").is_err());
        assert_eq!(cofre.pasta(b).unwrap().nome, "B");
    }

    #[test]
    fn mover_recusa_dentro_de_si_e_das_filhas() {
        let pasta = PastaTemporaria::nova("mover-ciclo");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let a = cofre.nova_pasta(None, "A").unwrap();
        let b = cofre.nova_pasta(Some(a), "B").unwrap();
        let c = cofre.nova_pasta(Some(b), "C").unwrap();
        for destino in [a, b, c] {
            let erro = cofre.mover(&[a], &[], Some(destino)).unwrap_err();
            assert_eq!(
                erro.to_string(),
                "Uma pasta não pode ir para dentro dela mesma."
            );
        }
        assert_eq!(cofre.pasta(a).unwrap().pai, None);
        // Uma filha sobe para a raiz sem problema.
        cofre.mover(&[c], &[], None).unwrap();
        assert_eq!(cofre.pasta(c).unwrap().pai, None);
    }

    #[test]
    fn mover_recusa_irma_de_mesmo_nome() {
        let pasta = PastaTemporaria::nova("mover-irma");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let a = cofre.nova_pasta(None, "A").unwrap();
        let b = cofre.nova_pasta(None, "B").unwrap();
        let dentro = cofre.nova_pasta(Some(a), "B").unwrap();
        let erro = cofre.mover(&[b], &[], Some(a)).unwrap_err();
        assert_eq!(erro.to_string(), "Já existe uma pasta B aqui.");
        assert_eq!(cofre.pasta(b).unwrap().pai, None, "nada mudou");
        assert_eq!(cofre.pasta(dentro).unwrap().pai, Some(a));
    }

    #[test]
    fn mover_pastas_e_itens_grava_uma_vez() {
        let pasta = PastaTemporaria::nova("mover-varios");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let destino = cofre.nova_pasta(None, "Destino").unwrap();
        let p1 = cofre.nova_pasta(None, "P1").unwrap();
        let p2 = cofre.nova_pasta(Some(p1), "P2").unwrap();
        let i1 = cofre.salvar(item("um")).unwrap();
        let i2 = cofre.salvar(item("dois")).unwrap();

        let antes = texto_do_cofre(&pasta.0);
        cofre.mover(&[p1, p2], &[i1, i2], Some(destino)).unwrap();
        let depois = texto_do_cofre(&pasta.0);
        assert_ne!(antes, depois);
        // P2 vai junto com a mãe e continua dentro dela.
        assert_eq!(cofre.pasta(p1).unwrap().pai, Some(destino));
        assert_eq!(cofre.pasta(p2).unwrap().pai, Some(p1));
        assert_eq!(cofre.item(i1).unwrap().pasta, Some(destino));
        assert_eq!(cofre.item(i2).unwrap().pasta, Some(destino));
        let (reaberto, _) = Cofre::abrir(&pasta.0);
        assert_eq!(reaberto.item(i2).unwrap().pasta, Some(destino));

        // Mover para onde já está não grava.
        cofre.mover(&[p1], &[i1, i2], Some(destino)).unwrap();
        assert_eq!(texto_do_cofre(&pasta.0), depois);
        // Um item que não existe recusa tudo.
        assert!(cofre.mover(&[], &[i1, 999], None).is_err());
        assert_eq!(cofre.item(i1).unwrap().pasta, Some(destino));
    }

    #[test]
    fn apagar_pasta_leva_subpastas_e_itens() {
        let pasta = PastaTemporaria::nova("apagar-pasta");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let a = cofre.nova_pasta(None, "A").unwrap();
        let b = cofre.nova_pasta(Some(a), "B").unwrap();
        let outra = cofre.nova_pasta(None, "Outra").unwrap();
        let fundo = cofre
            .salvar(Item {
                pasta: Some(b),
                ..item("fundo")
            })
            .unwrap();
        let meio = cofre
            .salvar(Item {
                pasta: Some(a),
                ..item("meio")
            })
            .unwrap();
        let fica = cofre
            .salvar(Item {
                pasta: Some(outra),
                ..item("fica")
            })
            .unwrap();
        let solto = cofre.salvar(item("solto")).unwrap();

        cofre.apagar(&[a], &[solto]).unwrap();
        assert!(cofre.pasta(a).is_none() && cofre.pasta(b).is_none());
        assert!(cofre.item(fundo).is_none() && cofre.item(meio).is_none());
        assert!(cofre.item(solto).is_none());
        assert!(cofre.item(fica).is_some() && cofre.pasta(outra).is_some());
        let (reaberto, _) = Cofre::abrir(&pasta.0);
        assert_eq!((reaberto.pastas().len(), reaberto.itens().len()), (1, 1));
    }

    #[test]
    fn garantir_caminho_reaproveita_e_cria() {
        let pasta = PastaTemporaria::nova("caminho");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let acme = cofre.garantir_caminho(None, &["Clientes", "ACME"]).unwrap();
        assert_eq!(cofre.pastas().len(), 2);
        // Mesmo caminho: as mesmas pastas, mesmo com outra caixa e espaços.
        assert_eq!(
            cofre.garantir_caminho(None, &[" clientes ", "acme"]),
            Some(acme)
        );
        assert_eq!(cofre.pastas().len(), 2);
        let filha = cofre.garantir_caminho(Some(acme), &["Produção"]).unwrap();
        assert_eq!(cofre.caminho(Some(filha)), "Clientes/ACME/Produção");
        assert_eq!(cofre.garantir_caminho(None, &[]), None);
        assert_eq!(cofre.garantir_caminho(Some(acme), &["", " "]), Some(acme));
    }

    #[test]
    fn caminho_da_raiz_e_vazio() {
        let pasta = PastaTemporaria::nova("caminho-raiz");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        assert_eq!(cofre.caminho(None), "");
        let a = cofre.nova_pasta(None, "A").unwrap();
        assert_eq!(cofre.caminho(Some(a)), "A");
        assert_eq!(cofre.caminho(Some(999)), "");
    }

    #[test]
    fn importar_junta_pastas_de_mesmo_nome_e_traduz_ids() {
        let pasta = PastaTemporaria::nova("importar");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let clientes = cofre.nova_pasta(None, "Clientes").unwrap();
        let antes = cofre.pastas().len();
        let lote = Lote {
            pastas: vec![
                Pasta {
                    id: 10,
                    nome: "Clientes".into(),
                    pai: None,
                    ..Pasta::default()
                },
                Pasta {
                    id: 11,
                    nome: "Nova".into(),
                    pai: Some(10),
                    ..Pasta::default()
                },
            ],
            itens: vec![
                Item {
                    id: 5,
                    pasta: Some(11),
                    ..item("a")
                },
                Item {
                    id: 6,
                    pasta: None,
                    ..item("b")
                },
            ],
        };
        let mapa = cofre.importar(lote, None).unwrap();
        // "Clientes" se junta à que já existia; "Nova" é nova dentro dela.
        assert_eq!(mapa.pastas[&10], clientes);
        assert_eq!(cofre.pastas().len(), antes + 1);
        assert_eq!(cofre.caminho(Some(mapa.pastas[&11])), "Clientes/Nova");
        assert_eq!(
            cofre.item(mapa.itens[&5]).unwrap().pasta,
            Some(mapa.pastas[&11])
        );
        assert_eq!(cofre.item(mapa.itens[&6]).unwrap().pasta, None);

        // No destino, a raiz do lote vira filha dele.
        let lote = Lote {
            pastas: vec![Pasta {
                id: 1,
                nome: "Sub".into(),
                pai: None,
                ..Pasta::default()
            }],
            itens: vec![Item {
                id: 1,
                pasta: Some(1),
                ..item("c")
            }],
        };
        let mapa = cofre.importar(lote, Some(clientes)).unwrap();
        assert_eq!(cofre.caminho(Some(mapa.pastas[&1])), "Clientes/Sub");

        // Um lote com ciclo não trava, e um item com pasta que não existe cai no destino.
        let lote = Lote {
            pastas: vec![
                Pasta {
                    id: 1,
                    nome: "X".into(),
                    pai: Some(2),
                    ..Pasta::default()
                },
                Pasta {
                    id: 2,
                    nome: "Y".into(),
                    pai: Some(1),
                    ..Pasta::default()
                },
            ],
            itens: vec![Item {
                id: 3,
                pasta: Some(77),
                ..item("d")
            }],
        };
        let mapa = cofre.importar(lote, Some(clientes)).unwrap();
        assert_eq!(mapa.pastas.len(), 2);
        assert_eq!(cofre.item(mapa.itens[&3]).unwrap().pasta, Some(clientes));
        assert!(cofre.importar(Lote::default(), Some(999)).is_err());
        let (reaberto, _) = Cofre::abrir(&pasta.0);
        assert_eq!(reaberto.itens().len(), 4);
    }

    #[test]
    fn ids_de_pastas_e_itens_nao_se_repetem() {
        let pasta = PastaTemporaria::nova("ids-compartilhados");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let p = cofre.nova_pasta(None, "A").unwrap();
        let i = cofre.salvar(item("x")).unwrap();
        let q = cofre.nova_pasta(None, "B").unwrap();
        let mut ids = [p, i, q];
        ids.sort_unstable();
        assert!(ids[0] < ids[1] && ids[1] < ids[2], "{ids:?}");
        let (mut reaberto, _) = Cofre::abrir(&pasta.0);
        let r = reaberto.nova_pasta(None, "C").unwrap();
        assert!(r > ids[2]);
    }

    fn item_com_uid(uid: &str, titulo: &str, alterado: u64) -> Item {
        Item {
            uid: uid.to_owned(),
            titulo: titulo.to_owned(),
            alterado,
            ..item(titulo)
        }
    }

    fn conteudo_com(itens: Vec<Item>) -> Conteudo {
        let mut c = Conteudo::default();
        for mut i in itens {
            i.id = c.novo_id();
            c.itens.push(i);
        }
        c
    }

    fn titulos(c: &Conteudo) -> Vec<String> {
        let mut t: Vec<String> = c.itens.iter().map(|i| i.titulo.clone()).collect();
        t.sort_unstable();
        t
    }

    #[test]
    fn juntar_o_mais_novo_vence_e_o_empate_fica_com_o_local() {
        let mut local = conteudo_com(vec![
            item_com_uid("a", "local-velho", 10),
            item_com_uid("b", "local-novo", 30),
            item_com_uid("c", "local-empate", 20),
        ]);
        let outro = conteudo_com(vec![
            item_com_uid("a", "outro-novo", 20),
            item_com_uid("b", "outro-velho", 5),
            item_com_uid("c", "outro-empate", 20),
        ]);
        let resumo = local.juntar(outro);
        assert_eq!(
            titulos(&local),
            ["local-empate", "local-novo", "outro-novo"]
        );
        assert_eq!(
            (resumo.novos, resumo.atualizados, resumo.removidos),
            (0, 1, 0)
        );
        // O outro lado ficou para trás em dois itens: é preciso regravar.
        assert!(resumo.a_gravar);
    }

    #[test]
    fn juntar_item_novo_entra_dos_dois_lados() {
        let mut local = conteudo_com(vec![item_com_uid("a", "so-local", 10)]);
        let outro = conteudo_com(vec![item_com_uid("b", "so-outro", 10)]);
        let resumo = local.juntar(outro);
        assert_eq!(titulos(&local), ["so-local", "so-outro"]);
        assert_eq!(resumo.novos, 1);
        assert!(resumo.a_gravar, "o outro lado não tem o item do local");
        // Os ids não colidem.
        let mut ids: Vec<u64> = local.itens.iter().map(|i| i.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn juntar_sem_diferenca_nao_manda_gravar() {
        let mut local = conteudo_com(vec![item_com_uid("a", "x", 10)]);
        let outro = local.clone();
        let resumo = local.juntar(outro);
        assert!(!resumo.mudou());
        assert!(!resumo.a_gravar);
    }

    #[test]
    fn juntar_lapide_apaga_o_mais_velho_e_perde_para_o_mais_novo() {
        let mut local = conteudo_com(vec![
            item_com_uid("velho", "velho", 10),
            item_com_uid("novo", "novo", 100),
        ]);
        let mut outro = Conteudo::default();
        outro.removidos.push(Removido {
            uid: "velho".to_owned(),
            quando: 50,
        });
        outro.removidos.push(Removido {
            uid: "novo".to_owned(),
            quando: 50,
        });
        let resumo = local.juntar(outro);
        assert_eq!(titulos(&local), ["novo"]);
        assert_eq!(resumo.removidos, 1);
    }

    #[test]
    fn juntar_item_que_so_o_outro_tem_nao_volta_se_a_lapide_daqui_e_mais_nova() {
        let mut local = Conteudo::default();
        local.removidos.push(Removido {
            uid: "x".to_owned(),
            quando: 100,
        });
        let outro = conteudo_com(vec![item_com_uid("x", "apagado aqui", 50)]);
        local.juntar(outro);
        assert!(local.itens.is_empty());
    }

    #[test]
    fn juntar_pastas_do_mesmo_nome_se_fundem_no_menor_uid() {
        let mut local = Conteudo::default();
        let pasta_local = local.nova_pasta("Clientes".to_owned(), None);
        local.pastas[0].uid = "bbb".to_owned();
        let mut a = item_com_uid("a", "a", 10);
        a.pasta = Some(pasta_local);
        a.id = local.novo_id();
        local.itens.push(a);

        let mut outro = Conteudo::default();
        let pasta_outro = outro.nova_pasta("clientes".to_owned(), None);
        outro.pastas[0].uid = "aaa".to_owned();
        let mut b = item_com_uid("b", "b", 10);
        b.pasta = Some(pasta_outro);
        b.id = outro.novo_id();
        outro.itens.push(b);

        local.juntar(outro);
        assert_eq!(local.pastas.len(), 1);
        assert_eq!(local.pastas[0].uid, "aaa");
        let id = local.pastas[0].id;
        assert!(local.itens.iter().all(|i| i.pasta == Some(id)));
    }

    #[test]
    fn migracao_v2_para_v3() {
        let pasta = PastaTemporaria::nova("v2");
        fs::create_dir_all(&pasta.0).unwrap();
        // Uma chave de verdade em arquivo, para a passphrase poder ir para o fingerprint.
        let gerada = tomni_ui::terminal::ssh::keys::KeyGen::new(
            tomni_ui::terminal::ssh::keys::KeyType::Ed25519,
        )
        .passphrase("frase")
        .generate()
        .unwrap();
        let arquivo_da_chave = pasta.0.join("casa/id_ed25519");
        gerada.write(&arquivo_da_chave, false).unwrap();
        let fingerprint = gerada.fingerprint();

        let claro = serde_json::json!({
            "proximo_id": 6,
            "pastas": [{"id": 1, "nome": "Clientes", "pai": null}],
            "itens": [
                {"id": 2, "tipo": "Ssh", "titulo": "web", "pasta": 1, "senha": "s"},
                {"id": 3, "tipo": "Chave", "titulo": "id_ed25519", "senha": "frase",
                 "url": arquivo_da_chave.display().to_string()},
                {"id": 4, "tipo": "Chave", "titulo": "sumida", "senha": "x",
                 "url": "/nao/existe/mais/id_rsa"},
            ],
        })
        .to_string()
        .into_bytes();
        let chave = [5u8; 32];
        gravar_chave(&pasta.0, &chave).unwrap();
        let arquivo = Arquivo {
            versao: 2,
            protecao: Protecao::Chave,
            itens: cifrar(&chave, &claro).unwrap(),
        };
        fs::write(
            pasta.0.join("vault.json"),
            serde_json::to_string(&arquivo).unwrap(),
        )
        .unwrap();

        let (mut cofre, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert!(!pasta.0.join("vault.json.bak").exists());
        assert_eq!(versao_do_arquivo(&pasta.0), 2, "só muda na gravação");
        assert!(cofre.itens().iter().all(|i| !i.uid.is_empty()));
        assert!(cofre.pastas().iter().all(|p| !p.uid.is_empty()));
        let uid_web = cofre.item(2).unwrap().uid.clone();
        let achada = cofre.item(3).unwrap();
        assert_eq!(achada.url, fingerprint);
        assert_eq!(
            achada
                .campos
                .iter()
                .find(|c| c.nome == CAMPO_ARQUIVO)
                .map(|c| c.valor.clone()),
            Some(arquivo_da_chave.display().to_string())
        );
        assert_eq!(
            cofre.item(4).unwrap().url,
            "/nao/existe/mais/id_rsa",
            "sem o arquivo, o caminho fica como estava"
        );

        cofre.salvar(item("novo")).unwrap();
        assert_eq!(versao_do_arquivo(&pasta.0), VERSAO);
        let (reaberto, _) = Cofre::abrir(&pasta.0);
        assert_eq!(
            reaberto.item(2).unwrap().uid,
            uid_web,
            "o uid não muda mais"
        );
    }

    #[test]
    fn a_chave_ssh_vai_e_volta_pelo_arquivo() {
        let pasta = PastaTemporaria::nova("chave-ssh-serde");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let mut chave = ChaveSsh::default();
        assert!(chave.no_agente, "entra no agente por padrão");
        chave.publica = "ssh-ed25519 AAAA ana".to_owned();
        chave.confirmar = true;
        chave.vida_secs = 3600;
        let id = cofre
            .salvar(Item {
                tipo: Tipo::ChaveSsh,
                titulo: "github".to_owned(),
                chave: Some(chave.clone()),
                ..Item::default()
            })
            .unwrap();
        let (reaberto, _) = Cofre::abrir(&pasta.0);
        assert_eq!(reaberto.item(id).unwrap().chave, Some(chave));
        // Sem passar pelo disco, o `serde(default)` cobre o que faltar.
        let sem_campos: ChaveSsh = serde_json::from_str(r#"{"publica":"x"}"#).unwrap();
        assert!(sem_campos.no_agente);
        assert!(!sem_campos.confirmar);
    }

    #[test]
    fn apagar_uma_pasta_deixa_lapide_dela_e_do_que_tinha_dentro() {
        let pasta = PastaTemporaria::nova("lapides");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let p = cofre.nova_pasta(None, "Clientes").unwrap();
        let mut dentro = item("web");
        dentro.pasta = Some(p);
        let i = cofre.salvar(dentro).unwrap();
        let uids = (
            cofre.pasta(p).unwrap().uid.clone(),
            cofre.item(i).unwrap().uid.clone(),
        );
        cofre.apagar(&[p], &[i]).unwrap();
        let lapides: Vec<&str> = cofre
            .conteudo
            .removidos
            .iter()
            .map(|r| r.uid.as_str())
            .collect();
        assert!(lapides.contains(&uids.0.as_str()) && lapides.contains(&uids.1.as_str()));
    }

    /// Um cofre com dois itens numa pasta temporária.
    fn cofre_com_itens(nome: &str) -> (Cofre, PastaTemporaria) {
        let pasta = PastaTemporaria::nova(nome);
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        cofre.salvar(item("banco")).unwrap();
        (cofre, pasta)
    }

    fn salvar_arquivo(pasta: &PastaTemporaria, nome: &str, bytes: &[u8]) -> PathBuf {
        fs::create_dir_all(&pasta.0).unwrap();
        let caminho = pasta.0.join(nome);
        fs::write(&caminho, bytes).unwrap();
        caminho
    }

    fn titulos_do(cofre: &Cofre) -> Vec<String> {
        let mut titulos: Vec<String> = cofre.itens().iter().map(|i| i.titulo.clone()).collect();
        titulos.sort();
        titulos
    }

    #[test]
    fn levar_sem_senha_deixa_a_chave_dentro_e_abre_sem_pergunta() {
        let (cofre, _pasta) = cofre_com_itens("levar-embutido");
        let bytes = cofre.levar(None).unwrap();
        let texto = String::from_utf8(bytes.clone()).unwrap();
        assert!(texto.contains("\"formato\": \"tirech-vault\""));
        assert!(texto.contains("\"tipo\": \"embutida\""));
        let fora = PastaTemporaria::nova("levar-embutido-fora");
        let caminho = salvar_arquivo(&fora, "meu.tirech-vault", &bytes);
        let externo = Externo::abrir(&caminho).unwrap();
        assert!(!externo.pede_senha());
        let resumo = externo.resumo().unwrap();
        assert_eq!((resumo.itens, resumo.chaves_ssh), (2, 0));
        // Uma senha vazia é o mesmo que nenhuma.
        let vazia = cofre.levar(Some("")).unwrap();
        assert!(String::from_utf8(vazia).unwrap().contains("embutida"));
    }

    #[test]
    fn levar_de_um_cofre_que_nunca_gravou_ganha_uma_chave_so_do_arquivo() {
        let pasta = PastaTemporaria::nova("levar-vazio");
        let (cofre, _) = Cofre::abrir(&pasta.0);
        let bytes = cofre.levar(None).unwrap();
        assert!(!pasta.0.join("vault.key").exists());
        let fora = PastaTemporaria::nova("levar-vazio-fora");
        let externo = Externo::abrir(&salvar_arquivo(&fora, "a.tirech-vault", &bytes)).unwrap();
        assert_eq!(externo.resumo().unwrap().itens, 0);
    }

    #[test]
    fn levar_com_senha_propria_pede_a_senha_e_nao_muda_o_cofre() {
        let (cofre, pasta) = cofre_com_itens("levar-senha");
        let antes = fs::read(pasta.0.join("vault.json")).unwrap();
        let bytes = cofre.levar(Some("frase do arquivo")).unwrap();
        assert_eq!(fs::read(pasta.0.join("vault.json")).unwrap(), antes);
        assert!(!cofre.tem_senha_mestra());
        let fora = PastaTemporaria::nova("levar-senha-fora");
        let mut externo = Externo::abrir(&salvar_arquivo(&fora, "a.tirech-vault", &bytes)).unwrap();
        assert!(externo.pede_senha());
        assert!(externo.resumo().is_none());
        assert!(!externo.desbloquear("outra"));
        assert!(externo.desbloquear("frase do arquivo"));
        assert_eq!(externo.resumo().unwrap().itens, 2);
        // A senha não vai escrita no arquivo.
        assert!(
            !String::from_utf8(bytes)
                .unwrap()
                .contains("frase do arquivo")
        );
    }

    #[test]
    fn levar_com_senha_mestra_e_o_cofre_como_esta_e_bloqueado_nao_leva() {
        let (mut cofre, _pasta) = cofre_com_itens("levar-mestra");
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        // A senha pedida no diálogo não conta: vale a mestra.
        let bytes = cofre.levar(Some("ignorada")).unwrap();
        let fora = PastaTemporaria::nova("levar-mestra-fora");
        let caminho = salvar_arquivo(&fora, "a.tirech-vault", &bytes);
        let mut externo = Externo::abrir(&caminho).unwrap();
        assert!(!externo.desbloquear("ignorada"));
        assert!(externo.desbloquear("mestra"));
        cofre.bloquear();
        assert_eq!(
            cofre.levar(None).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn levar_e_abrir_ida_e_volta() {
        let (cofre, _pasta) = cofre_com_itens("ida-e-volta");
        let fora = PastaTemporaria::nova("ida-e-volta-fora");
        let caminho = salvar_arquivo(&fora, "a.tirech-vault", &cofre.levar(None).unwrap());
        let destino = PastaTemporaria::nova("ida-e-volta-destino");
        let (mut outro, _) = Cofre::abrir(&destino.0);
        outro.salvar(item("so-aqui")).unwrap();
        let externo = outro.abrir_copia(&caminho).unwrap();
        let resumo = outro.juntar_externo(&externo).unwrap();
        assert_eq!(resumo.novos, 2);
        assert_eq!(titulos_do(&outro), ["banco", "so-aqui", "web"]);
        // Juntar de novo não muda nada.
        assert!(!outro.juntar_externo(&externo).unwrap().mudou());
    }

    #[test]
    fn so_o_vault_json_sem_a_chave_da_o_erro_explicado_e_nao_vai_para_bak() {
        let (_cofre, pasta) = cofre_com_itens("sem-a-chave");
        let fora = PastaTemporaria::nova("sem-a-chave-fora");
        let caminho = salvar_arquivo(
            &fora,
            "vault.json",
            &fs::read(pasta.0.join("vault.json")).unwrap(),
        );
        let erro = Externo::abrir(&caminho).err().unwrap();
        assert!(erro.contains("vault.key"), "{erro}");
        // Abrir a pasta sem a chave tranca o cofre, sem mover nada.
        let (cofre, aviso) = Cofre::abrir(&fora.0);
        assert!(aviso.unwrap().contains("vault.key"));
        assert!(cofre.itens().is_empty());
        assert!(fora.0.join("vault.json").exists());
        assert!(!fora.0.join("vault.json.bak").exists());
    }

    #[test]
    fn cofre_copiado_para_outra_pasta_abre_igual() {
        let (cofre, pasta) = cofre_com_itens("copiado");
        let fora = PastaTemporaria::nova("copiado-fora");
        fs::create_dir_all(&fora.0).unwrap();
        for nome in ["vault.json", "vault.key"] {
            fs::copy(pasta.0.join(nome), fora.0.join(nome)).unwrap();
        }
        let (copia, aviso) = Cofre::abrir(&fora.0);
        assert_eq!(aviso, None);
        assert_eq!(titulos_do(&copia), titulos_do(&cofre));
    }

    #[test]
    fn cofre_com_senha_mestra_copiado_so_com_o_vault_json_abre() {
        let (mut cofre, pasta) = cofre_com_itens("copiado-mestra");
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        assert!(!pasta.0.join("vault.key").exists());
        let fora = PastaTemporaria::nova("copiado-mestra-fora");
        fs::create_dir_all(&fora.0).unwrap();
        fs::copy(pasta.0.join("vault.json"), fora.0.join("vault.json")).unwrap();
        let (mut copia, aviso) = Cofre::abrir(&fora.0);
        assert_eq!(aviso, None);
        assert!(copia.bloqueado());
        assert!(copia.desbloquear("mestra"));
        assert_eq!(titulos_do(&copia), ["banco", "web"]);
    }

    #[test]
    fn um_tirech_vault_serve_de_vault_json() {
        let (cofre, _pasta) = cofre_com_itens("como-vault-json");
        let bytes = cofre.levar(None).unwrap();
        let destino = PastaTemporaria::nova("como-vault-json-destino");
        salvar_arquivo(&destino, "vault.json", &bytes);
        let (mut aberto, aviso) = Cofre::abrir(&destino.0);
        assert_eq!(aviso, None);
        assert_eq!(titulos_do(&aberto), ["banco", "web"]);
        // E continua gravando com a chave dentro.
        aberto.salvar(item("novo")).unwrap();
        assert!(!destino.0.join("vault.key").exists());
        let (de_novo, _) = Cofre::abrir(&destino.0);
        assert_eq!(titulos_do(&de_novo), ["banco", "novo", "web"]);
    }

    #[test]
    fn arquivo_que_nao_e_cofre_e_versao_nova_dao_erro() {
        let fora = PastaTemporaria::nova("nao-e-cofre");
        let texto = salvar_arquivo(&fora, "x.tirech-vault", b"isto nao e json");
        assert!(
            Externo::abrir(&texto)
                .err()
                .unwrap()
                .contains("não é um cofre")
        );
        let nova = salvar_arquivo(
            &fora,
            "y.tirech-vault",
            br#"{"versao":99,"protecao":{"tipo":"chave"},"itens":{"nonce":"","dados":""}}"#,
        );
        assert!(Externo::abrir(&nova).err().unwrap().contains("mais nova"));
        assert!(Externo::abrir(&fora.0.join("nao-existe")).is_err());
    }

    #[test]
    fn substituir_guarda_o_cofre_de_agora_e_mantem_a_protecao() {
        let (mut cofre, pasta) = cofre_com_itens("substituir");
        let chave = fs::read(pasta.0.join("vault.key")).unwrap();
        let fora_pasta = PastaTemporaria::nova("substituir-fora");
        let (mut de_fora, _) = Cofre::abrir(&fora_pasta.0);
        de_fora.salvar(item("do-outro")).unwrap();
        let caminho = salvar_arquivo(&fora_pasta, "a.tirech-vault", &de_fora.levar(None).unwrap());
        let externo = cofre.abrir_copia(&caminho).unwrap();
        let copia = cofre.substituir_por(&externo).unwrap().unwrap();
        assert!(copia.exists());
        assert!(
            copia
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("vault.json.bak-")
        );
        // A cópia ainda abre o cofre de antes (com a chave de antes, guardada junto).
        let guardada = PastaTemporaria::nova("substituir-guardada");
        fs::create_dir_all(&guardada.0).unwrap();
        fs::copy(&copia, guardada.0.join("vault.json")).unwrap();
        let chave_guardada = fs::read_dir(&pasta.0)
            .unwrap()
            .flatten()
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("vault.key.bak-")
            })
            .unwrap()
            .path();
        fs::copy(chave_guardada, guardada.0.join("vault.key")).unwrap();
        assert_eq!(titulos_do(&Cofre::abrir(&guardada.0).0), ["banco", "web"]);
        // Aqui só ficou o de fora, com a mesma chave.
        assert_eq!(titulos_do(&cofre), ["do-outro"]);
        assert_eq!(fs::read(pasta.0.join("vault.key")).unwrap(), chave);
        assert_eq!(titulos_do(&Cofre::abrir(&pasta.0).0), ["do-outro"]);
    }

    #[test]
    fn substituir_com_o_cofre_bloqueado_adota_a_senha_do_cofre_de_fora() {
        let (mut cofre, pasta) = cofre_com_itens("substituir-bloqueado");
        cofre.definir_senha_mestra(Some("daqui")).unwrap();
        cofre.bloquear();
        let fora_pasta = PastaTemporaria::nova("substituir-bloqueado-fora");
        let (mut de_fora, _) = Cofre::abrir(&fora_pasta.0);
        de_fora.salvar(item("do-outro")).unwrap();
        de_fora.definir_senha_mestra(Some("de-la")).unwrap();
        let caminho = salvar_arquivo(&fora_pasta, "a.tirech-vault", &de_fora.levar(None).unwrap());
        let mut externo = Externo::abrir(&caminho).unwrap();
        assert!(externo.desbloquear("de-la"));
        cofre.substituir_por(&externo).unwrap();
        let (mut reaberto, _) = Cofre::abrir(&pasta.0);
        assert!(!reaberto.desbloquear("daqui"));
        assert!(reaberto.desbloquear("de-la"));
        assert_eq!(titulos_do(&reaberto), ["do-outro"]);
    }

    #[test]
    fn substituir_com_o_cofre_de_fora_sem_senha_e_o_daqui_bloqueado_ganha_chave_nova() {
        let (mut cofre, pasta) = cofre_com_itens("substituir-chave-nova");
        cofre.definir_senha_mestra(Some("daqui")).unwrap();
        cofre.bloquear();
        let fora_pasta = PastaTemporaria::nova("substituir-chave-nova-fora");
        let (mut de_fora, _) = Cofre::abrir(&fora_pasta.0);
        de_fora.salvar(item("do-outro")).unwrap();
        let caminho = salvar_arquivo(&fora_pasta, "a.tirech-vault", &de_fora.levar(None).unwrap());
        let externo = Externo::abrir(&caminho).unwrap();
        cofre.substituir_por(&externo).unwrap();
        assert!(pasta.0.join("vault.key").exists());
        let (reaberto, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert_eq!(titulos_do(&reaberto), ["do-outro"]);
    }

    #[test]
    fn levar_para_outra_pasta_move_o_cofre_e_deixa_o_bak() {
        let (mut cofre, pasta) = cofre_com_itens("mudar-local");
        let destino = PastaTemporaria::nova("mudar-local-destino");
        cofre.levar_para(&destino.0).unwrap();
        assert_eq!(cofre.pasta_do_cofre(), destino.0);
        for nome in ["vault.json", "vault.key"] {
            assert!(destino.0.join(nome).exists());
            assert!(!pasta.0.join(nome).exists());
            assert!(pasta.0.join(format!("{nome}.bak")).exists());
        }
        // E o cofre segue gravando lá.
        cofre.salvar(item("depois")).unwrap();
        assert_eq!(
            titulos_do(&Cofre::abrir(&destino.0).0),
            ["banco", "depois", "web"]
        );
        assert!(!pasta.0.join("vault.json").exists());
    }

    #[test]
    fn levar_para_uma_pasta_que_ja_tem_cofre_guarda_o_que_estava_la() {
        let (mut cofre, _pasta) = cofre_com_itens("mudar-local-ocupado");
        let (mut ocupado, destino) = cofre_com_itens("mudar-local-ocupado-destino");
        ocupado.salvar(item("velho")).unwrap();
        cofre.levar_para(&destino.0).unwrap();
        let guardados: Vec<String> = fs::read_dir(&destino.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".bak-"))
            .collect();
        assert_eq!(guardados.len(), 2, "{guardados:?}");
        assert_eq!(titulos_do(&Cofre::abrir(&destino.0).0), ["banco", "web"]);
    }

    #[test]
    fn levar_para_deixa_o_vault_antigo_como_vault_bak_sem_pisar_num_que_ja_existe() {
        let (mut cofre, pasta) = cofre_com_itens("mudar-local-vault");
        let destino = PastaTemporaria::nova("mudar-local-vault-destino");
        fs::create_dir_all(pasta.0.join("vault")).unwrap();
        fs::write(pasta.0.join("vault").join("ab01"), b"anexo").unwrap();
        fs::create_dir_all(pasta.0.join("vault.bak")).unwrap();
        fs::write(pasta.0.join("vault.bak").join("ab00"), b"de antes").unwrap();
        cofre.levar_para(&destino.0).unwrap();
        assert!(!pasta.0.join("vault").exists());
        assert!(
            pasta.0.join("vault.bak").join("ab00").exists(),
            "o .bak antigo fica"
        );
        let novos: Vec<PathBuf> = fs::read_dir(&pasta.0)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with("vault.bak-"))
            })
            .collect();
        assert_eq!(novos.len(), 1, "{novos:?}");
        assert_eq!(fs::read(novos[0].join("ab01")).unwrap(), b"anexo");
    }

    #[test]
    fn levar_para_com_senha_mestra_nao_deixa_vault_key() {
        let (mut cofre, _pasta) = cofre_com_itens("mudar-local-mestra");
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        let destino = PastaTemporaria::nova("mudar-local-mestra-destino");
        cofre.levar_para(&destino.0).unwrap();
        assert!(destino.0.join("vault.json").exists());
        assert!(!destino.0.join("vault.key").exists());
        let (mut reaberto, _) = Cofre::abrir(&destino.0);
        assert!(reaberto.desbloquear("mestra"));
    }

    #[test]
    fn mudanca_no_disco_e_vista_e_juntada() {
        let (mut aqui, pasta) = cofre_com_itens("no-disco");
        assert!(!aqui.mudou_no_disco());
        let (mut la, _) = Cofre::abrir(&pasta.0);
        la.salvar(item("da-outra-maquina")).unwrap();
        assert!(aqui.mudou_no_disco());
        let resumo = aqui.atualizar_do_disco().unwrap().unwrap();
        assert_eq!(resumo.novos, 1);
        assert!(!aqui.mudou_no_disco());
        assert_eq!(titulos_do(&aqui), ["banco", "da-outra-maquina", "web"]);
        assert_eq!(aqui.atualizar_do_disco().unwrap(), None);
    }

    #[test]
    fn copias_em_conflito_da_sincronizacao_sao_achadas_e_arquivadas() {
        let (cofre, pasta) = cofre_com_itens("conflitos");
        for nome in [
            "vault.sync-conflict-20260101-101010-ABCDEFG.json",
            "vault (conflicted copy 2026-01-01).json",
            "vault-NOTEBOOK.json",
            "vault.json.bak",
            "vault.json.bak-20260101-101010",
            "vault.sync-conflict-1.json.juntado",
            "configuracoes.json",
        ] {
            fs::write(pasta.0.join(nome), b"x").unwrap();
        }
        let achadas: Vec<String> = cofre
            .copias_em_conflito()
            .iter()
            .map(|c| c.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            achadas,
            [
                "vault (conflicted copy 2026-01-01).json",
                "vault-NOTEBOOK.json",
                "vault.sync-conflict-20260101-101010-ABCDEFG.json",
            ]
        );
        let primeira = cofre.copias_em_conflito().remove(0);
        let arquivada = Cofre::arquivar_copia(&primeira).unwrap();
        assert!(arquivada.to_string_lossy().ends_with(".json.juntado"));
        assert_eq!(cofre.copias_em_conflito().len(), 2);
    }

    #[test]
    fn copia_em_conflito_com_a_mesma_senha_mestra_abre_sozinha() {
        let (mut cofre, pasta) = cofre_com_itens("conflito-mestra");
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        let copia = pasta.0.join("vault.sync-conflict-1.json");
        fs::copy(pasta.0.join("vault.json"), &copia).unwrap();
        let externo = cofre.abrir_copia(&copia).unwrap();
        assert!(!externo.pede_senha());
        cofre.bloquear();
        assert!(cofre.abrir_copia(&copia).unwrap().pede_senha());
    }

    fn nota_com_uid(uid: &str, titulo: &str, corpo: &str, alterado: u64) -> Item {
        Item {
            tipo: Tipo::Nota,
            uid: uid.to_owned(),
            titulo: titulo.to_owned(),
            notas: corpo.to_owned(),
            alterado,
            ..Item::default()
        }
    }

    fn ficha(id: &str, nome: &str) -> Anexo {
        Anexo {
            id: id.to_owned(),
            nome: nome.to_owned(),
            tamanho: 3,
            tipo: "text/plain".to_owned(),
            sha256: "00".to_owned(),
            chave: "AA==".to_owned(),
            criado: 1,
        }
    }

    fn corpos(c: &Conteudo) -> Vec<String> {
        let mut t: Vec<String> = c.itens.iter().map(|i| i.notas.clone()).collect();
        t.sort_unstable();
        t
    }

    #[test]
    fn anexar_resolve_nome_repetido_e_muda_o_alterado() {
        let (mut cofre, _pasta) = cofre_com_itens("anexar");
        let id = cofre.salvar(item("com-anexos")).unwrap();
        let antes = cofre.item(id).unwrap().alterado;
        assert_eq!(
            cofre.anexar(id, ficha("a1", "chave.pem")).unwrap(),
            "chave.pem"
        );
        assert_eq!(
            cofre.anexar(id, ficha("a2", "chave.pem")).unwrap(),
            "chave (2).pem"
        );
        assert_eq!(
            cofre.anexar(id, ficha("a3", "chave.pem")).unwrap(),
            "chave (3).pem"
        );
        assert_eq!(cofre.anexar(id, ficha("a4", "LEIA")).unwrap(), "LEIA");
        assert_eq!(cofre.anexar(id, ficha("a5", "LEIA")).unwrap(), "LEIA (2)");
        assert_eq!(cofre.anexar(id, ficha("a6", ".env")).unwrap(), ".env");
        assert_eq!(cofre.anexar(id, ficha("a7", ".env")).unwrap(), ".env (2)");
        assert!(cofre.item(id).unwrap().alterado >= antes);
        assert_eq!(cofre.item(id).unwrap().anexos.len(), 7);
        // Sobrevive à gravação.
        let (reaberto, _) = Cofre::abrir(&_pasta.0);
        assert_eq!(reaberto.item(id).unwrap().anexos.len(), 7);
        assert!(cofre.anexar(9999, ficha("x", "x")).is_err());
    }

    #[test]
    fn renomear_e_tirar_anexo() {
        let (mut cofre, _pasta) = cofre_com_itens("anexo-renomear");
        let id = cofre.salvar(item("n")).unwrap();
        cofre.anexar(id, ficha("a1", "um.txt")).unwrap();
        cofre.anexar(id, ficha("a2", "dois.txt")).unwrap();
        assert_eq!(
            cofre.renomear_anexo(id, "a1", "dois.txt").unwrap(),
            "dois (2).txt"
        );
        // Renomear para o próprio nome não ganha número.
        assert_eq!(
            cofre.renomear_anexo(id, "a2", "dois.txt").unwrap(),
            "dois.txt"
        );
        assert!(cofre.renomear_anexo(id, "inexistente", "x").is_err());
        cofre.tirar_anexo(id, "a1").unwrap();
        let anexos = &cofre.item(id).unwrap().anexos;
        assert_eq!(anexos.len(), 1);
        assert_eq!(anexos[0].id, "a2");
    }

    #[test]
    fn anexos_referenciados_junta_fichas_do_mesmo_id() {
        let (mut cofre, _pasta) = cofre_com_itens("anexo-refs");
        let a = cofre.salvar(item("a")).unwrap();
        let b = cofre.salvar(item("b")).unwrap();
        cofre.anexar(a, ficha("mesmo", "x.txt")).unwrap();
        cofre.anexar(b, ficha("mesmo", "x.txt")).unwrap();
        cofre.anexar(b, ficha("outro", "y.txt")).unwrap();
        let refs = cofre.anexos_referenciados();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains("mesmo") && refs.contains("outro"));
    }

    #[test]
    fn serde_dos_anexos_com_e_sem() {
        let sem = serde_json::to_string(&item("x")).unwrap();
        assert!(!sem.contains("anexos"));
        let velho: Item = serde_json::from_str(&sem).unwrap();
        assert!(velho.anexos.is_empty());
        let com = Item {
            anexos: vec![ficha("a1", "n.txt")],
            ..item("x")
        };
        let volta: Item = serde_json::from_str(&serde_json::to_string(&com).unwrap()).unwrap();
        assert_eq!(volta, com);
        // Uma ficha com campo faltando ainda abre.
        let parcial: Item =
            serde_json::from_str(r#"{"titulo":"t","anexos":[{"id":"z"}]}"#).unwrap();
        assert_eq!(parcial.anexos[0].id, "z");
        assert_eq!(parcial.anexos[0].tamanho, 0);
    }

    #[test]
    fn trocar_a_senha_mestra_nao_recifra_os_anexos() {
        let pasta = PastaTemporaria::nova("anexo-senha");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let id = cofre.nova_nota(None, "n", "texto").unwrap();
        let vault = pasta.0.join("vault");
        let claro = b"conteudo do anexo".to_vec();
        let mut anexo = crate::anexos::gravar(
            &vault,
            "aa11",
            &mut std::io::Cursor::new(claro.clone()),
            claro.len() as u64,
            16,
            &|_| {},
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        anexo.nome = "a.txt".to_owned();
        cofre.anexar(id, anexo).unwrap();
        let bytes_antes = fs::read(vault.join("aa11")).unwrap();
        let mtime_antes = fs::metadata(vault.join("aa11"))
            .unwrap()
            .modified()
            .unwrap();
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        cofre.definir_senha_mestra(Some("outra")).unwrap();
        assert_eq!(fs::read(vault.join("aa11")).unwrap(), bytes_antes);
        assert_eq!(
            fs::metadata(vault.join("aa11"))
                .unwrap()
                .modified()
                .unwrap(),
            mtime_antes
        );
        let (mut reaberto, _) = Cofre::abrir(&pasta.0);
        assert!(reaberto.desbloquear("outra"));
        let ficha = reaberto.item(id).unwrap().anexos[0].clone();
        let lido = crate::anexos::ler_para_memoria(
            &vault,
            &ficha,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(lido, claro);
    }

    #[test]
    fn bloquear_avisa_antes_de_esquecer_a_chave() {
        let pasta = PastaTemporaria::nova("antes-de-bloquear");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.nova_nota(None, "n", "texto").unwrap();
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        let compartilhado = CofreCompartilhado::new(cofre);
        let vistos = Rc::new(RefCell::new(Vec::new()));
        for rotulo in ["primeiro", "segundo"] {
            let (c, vistos) = (compartilhado.clone(), Rc::clone(&vistos));
            compartilhado.antes_de_bloquear(move || {
                // Ainda dá para ler e gravar: o cofre só bloqueia depois dos avisos.
                let aberto = c.agora(|c| !c.bloqueado() && c.notas().count() == 1);
                vistos.borrow_mut().push((rotulo, aberto));
            });
        }
        compartilhado.bloquear();
        assert_eq!(*vistos.borrow(), [("primeiro", true), ("segundo", true)]);
        assert!(compartilhado.agora(Cofre::bloqueado));
        // Bloqueado de novo: os avisos rodam outra vez, sem estragar nada.
        compartilhado.bloquear();
        assert_eq!(vistos.borrow().len(), 4);
    }

    #[test]
    fn so_coleta_anexos_com_o_cofre_aberto_e_com_itens() {
        let pasta = PastaTemporaria::nova("pode-coletar");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        assert!(!cofre.pode_coletar_anexos(), "vazio");
        cofre.nova_nota(None, "n", "").unwrap();
        assert!(cofre.pode_coletar_anexos());
        cofre.definir_senha_mestra(Some("m")).unwrap();
        cofre.bloquear();
        assert!(!cofre.pode_coletar_anexos(), "bloqueado");
    }

    #[test]
    fn nova_nota_tem_titulo_unico_na_pasta() {
        let pasta = PastaTemporaria::nova("nova-nota");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let a = cofre.nova_nota(None, "", "").unwrap();
        let b = cofre.nova_nota(None, "  ", "um").unwrap();
        let c = cofre.nova_nota(None, "Nova nota", "").unwrap();
        assert_eq!(cofre.item(a).unwrap().titulo, "Nova nota");
        assert_eq!(cofre.item(b).unwrap().titulo, "Nova nota (2)");
        assert_eq!(cofre.item(c).unwrap().titulo, "Nova nota (3)");
        assert_eq!(cofre.item(b).unwrap().notas, "um");
        assert_eq!(cofre.item(a).unwrap().tipo, Tipo::Nota);
        // Outra pasta pode repetir o título; os outros tipos não contam.
        let pasta_id = cofre.nova_pasta(None, "p").unwrap();
        let d = cofre.nova_nota(Some(pasta_id), "Nova nota", "").unwrap();
        assert_eq!(cofre.item(d).unwrap().titulo, "Nova nota");
        cofre.salvar(item("Senha qualquer")).unwrap();
        assert_eq!(cofre.notas().count(), 4);
    }

    #[test]
    fn juntar_nota_so_o_local_mudou_fica_o_local() {
        let mut local = conteudo_com(vec![nota_com_uid("n", "t", "local novo", 50)]);
        let de = nota_com_uid("n", "t", "antigo", 10);
        local.vistos = [("n".to_owned(), impressao(&de))].into();
        let resumo = local.juntar(conteudo_com(vec![de]));
        assert_eq!(corpos(&local), ["local novo"]);
        assert_eq!(resumo.conflitos, 0);
        assert!(!resumo.mudou());
        assert!(resumo.a_gravar);
    }

    #[test]
    fn juntar_nota_so_o_de_fora_mudou_fica_o_de_fora_mesmo_com_alterado_menor() {
        let antigo = nota_com_uid("n", "t", "antigo", 100);
        let mut local = conteudo_com(vec![antigo.clone()]);
        local.vistos = [("n".to_owned(), impressao(&antigo))].into();
        // O relógio da outra máquina anda atrás: o alterado de fora é menor.
        let resumo = local.juntar(conteudo_com(vec![nota_com_uid("n", "t", "de fora", 60)]));
        assert_eq!(corpos(&local), ["de fora"]);
        assert_eq!((resumo.atualizados, resumo.conflitos), (1, 0));
    }

    #[test]
    fn juntar_nota_alterada_dos_dois_lados_guarda_a_que_perdeu_como_copia() {
        let base = nota_com_uid("n", "t", "base", 10);
        let mut local = conteudo_com(vec![Item {
            anexos: vec![ficha("x1", "a.txt")],
            ..nota_com_uid("n", "t", "local", 30)
        }]);
        local.itens[0].pasta = None;
        local.vistos = [("n".to_owned(), impressao(&base))].into();
        let de = Item {
            anexos: vec![ficha("x2", "b.txt")],
            ..nota_com_uid("n", "t", "de fora", 20)
        };
        let resumo = local.juntar(conteudo_com(vec![de]));
        assert_eq!(resumo.conflitos, 1);
        assert!(resumo.mudou());
        assert!(resumo.a_gravar);
        assert_eq!(local.itens.len(), 2);
        let vencedora = local.itens.iter().find(|i| i.uid == "n").unwrap();
        assert_eq!(vencedora.notas, "local");
        let copia = local.itens.iter().find(|i| i.uid != "n").unwrap();
        assert_eq!(copia.notas, "de fora");
        assert!(
            copia.titulo.starts_with("t (conflito de "),
            "{}",
            copia.titulo
        );
        assert!(
            copia.titulo.ends_with("01/01/1970 00:00)"),
            "{}",
            copia.titulo
        );
        assert_eq!(copia.anexos[0].id, "x2");
        assert_eq!(copia.tipo, Tipo::Nota);
        assert_ne!(copia.id, vencedora.id);
        assert!(!copia.uid.is_empty());
    }

    #[test]
    fn juntar_nota_alterada_dos_dois_lados_de_fora_vence_quando_e_mais_nova() {
        let base = nota_com_uid("n", "t", "base", 10);
        let mut local = conteudo_com(vec![nota_com_uid("n", "t", "local", 20)]);
        local.vistos = [("n".to_owned(), impressao(&base))].into();
        let resumo = local.juntar(conteudo_com(vec![nota_com_uid("n", "t", "de fora", 30)]));
        assert_eq!((resumo.atualizados, resumo.conflitos), (1, 1));
        assert_eq!(corpos(&local), ["de fora", "local"]);
        assert_eq!(
            local.itens.iter().find(|i| i.uid == "n").unwrap().notas,
            "de fora"
        );
    }

    #[test]
    fn juntar_nota_sem_vistos_com_textos_diferentes_vira_copia() {
        let mut local = conteudo_com(vec![nota_com_uid("n", "t", "um", 10)]);
        let resumo = local.juntar(conteudo_com(vec![nota_com_uid("n", "t", "dois", 20)]));
        assert_eq!(resumo.conflitos, 1);
        assert_eq!(corpos(&local), ["dois", "um"]);
    }

    #[test]
    fn juntar_nota_sem_vistos_com_textos_iguais_nao_gera_copia() {
        let mut local = conteudo_com(vec![nota_com_uid("n", "t", "igual", 10)]);
        let resumo = local.juntar(conteudo_com(vec![nota_com_uid("n", "t", "igual", 20)]));
        assert_eq!(resumo.conflitos, 0);
        assert_eq!(local.itens.len(), 1);
        // O M44 vale: o mais novo fica.
        assert_eq!(local.itens[0].alterado, 20);
    }

    #[test]
    fn juntar_nota_apagada_de_um_lado_e_editada_do_outro_segue_as_lapides() {
        // Apagada aqui depois da edição de lá: não volta.
        let mut local = conteudo_com(vec![]);
        local.lapide("n", 50);
        let resumo = local.juntar(conteudo_com(vec![nota_com_uid("n", "t", "editada", 20)]));
        assert!(local.itens.is_empty());
        assert_eq!(resumo.conflitos, 0);
        // Editada aqui depois de apagada lá: fica.
        let mut local = conteudo_com(vec![nota_com_uid("n", "t", "editada", 80)]);
        let mut outro = conteudo_com(vec![]);
        outro.lapide("n", 50);
        let resumo = local.juntar(outro);
        assert_eq!(local.itens.len(), 1);
        assert_eq!(resumo.conflitos, 0);
    }

    #[test]
    fn juntar_item_que_mudou_de_tipo_segue_o_m44_sem_copia() {
        let mut local = conteudo_com(vec![nota_com_uid("n", "t", "nota", 10)]);
        let outro = conteudo_com(vec![item_com_uid("n", "senha", 20)]);
        let resumo = local.juntar(outro);
        assert_eq!(resumo.conflitos, 0);
        assert_eq!(local.itens.len(), 1);
        assert_eq!(local.itens[0].titulo, "senha");
    }

    #[test]
    fn duas_maquinas_editando_a_mesma_nota_no_disco_nao_perdem_texto() {
        let pasta = PastaTemporaria::nova("nota-duas-maquinas");
        let (mut aqui, _) = Cofre::abrir(&pasta.0);
        let id = aqui.nova_nota(None, "diario", "base").unwrap();
        let (mut la, _) = Cofre::abrir(&pasta.0);
        // Lá edita e grava; aqui edita (sem ter visto) e grava, tudo no mesmo segundo.
        let mut de_la = la.item(id).unwrap().clone();
        de_la.notas = "texto de lá".to_owned();
        la.salvar(de_la).unwrap();
        let mut daqui = aqui.item(id).unwrap().clone();
        daqui.notas = "texto daqui".to_owned();
        aqui.salvar(daqui).unwrap();
        let textos: Vec<&str> = aqui.notas().map(|n| n.notas.as_str()).collect();
        assert_eq!(textos.len(), 2, "{textos:?}");
        assert!(textos.contains(&"texto de lá") && textos.contains(&"texto daqui"));
        // Salvamentos seguidos do mesmo lado não geram cópia.
        let mut de_novo = aqui.item(id).unwrap().clone();
        de_novo.notas = "texto daqui 2".to_owned();
        aqui.salvar(de_novo).unwrap();
        assert_eq!(aqui.notas().count(), 2);
        // Lá, a verificação periódica traz a cópia sem criar outra.
        let resumo = la.atualizar_do_disco().unwrap().unwrap();
        assert!(resumo.mudou());
        assert_eq!(la.notas().count(), 2);
        la.atualizar_do_disco().unwrap();
        assert_eq!(la.notas().count(), 2);
    }

    #[test]
    fn so_uma_maquina_editando_nao_gera_copia_mesmo_no_mesmo_segundo() {
        let pasta = PastaTemporaria::nova("nota-uma-maquina");
        let (mut aqui, _) = Cofre::abrir(&pasta.0);
        let id = aqui.nova_nota(None, "diario", "0").unwrap();
        let (mut la, _) = Cofre::abrir(&pasta.0);
        for n in 1..=5 {
            let mut nota = aqui.item(id).unwrap().clone();
            nota.notas = n.to_string();
            aqui.salvar(nota).unwrap();
            if n % 2 == 0 {
                la.atualizar_do_disco().unwrap();
            }
        }
        assert_eq!(aqui.notas().count(), 1);
        la.atualizar_do_disco().unwrap();
        assert_eq!(la.notas().count(), 1);
        assert_eq!(la.item(id).unwrap().notas, "5");
    }

    /// Os nomes da pasta que começam com `prefixo`, em ordem.
    fn nomes_com(pasta: &Path, prefixo: &str) -> Vec<String> {
        let mut nomes: Vec<String> = fs::read_dir(pasta)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(prefixo))
            .collect();
        nomes.sort();
        nomes
    }

    #[test]
    fn cofre_de_versao_mais_nova_abre_trancado_sem_mexer_no_arquivo() {
        let (_cofre, pasta) = cofre_com_itens("mais-nova");
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(pasta.0.join("vault.json")).unwrap()).unwrap();
        json["versao"] = (VERSAO + 1).into();
        json["campo_que_ainda_nao_existe"] = "x".into();
        fs::write(pasta.0.join("vault.json"), json.to_string()).unwrap();
        let antes = texto_do_cofre(&pasta.0);
        let chave = fs::read(pasta.0.join("vault.key")).unwrap();

        let (mut cofre, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, Some(t!("cofre-versao-mais-nova")));
        assert!(cofre.itens().is_empty());
        assert!(!cofre.pode_coletar_anexos());
        assert!(!cofre.mudou_no_disco(), "a vigia não acusa mudança à toa");
        assert!(cofre.salvar(item("novo")).is_err());
        assert_eq!(cofre.atualizar_do_disco().unwrap(), None);
        assert_eq!(texto_do_cofre(&pasta.0), antes);
        assert_eq!(fs::read(pasta.0.join("vault.key")).unwrap(), chave);
        assert_eq!(nomes_com(&pasta.0, "vault.json."), Vec::<String>::new());
        assert_eq!(nomes_com(&pasta.0, "vault.key."), Vec::<String>::new());
    }

    #[test]
    fn a_gravacao_nao_pisa_num_cofre_de_versao_mais_nova_gravado_por_outra_copia() {
        let (mut cofre, pasta) = cofre_com_itens("mais-nova-depois");
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(pasta.0.join("vault.json")).unwrap()).unwrap();
        json["versao"] = (VERSAO + 1).into();
        fs::write(pasta.0.join("vault.json"), json.to_string()).unwrap();
        let antes = texto_do_cofre(&pasta.0);
        let erro = cofre.salvar(item("novo")).unwrap_err();
        assert_eq!(erro.to_string(), t!("cofre-versao-mais-nova"));
        assert_eq!(texto_do_cofre(&pasta.0), antes);
    }

    #[test]
    fn duas_corrupcoes_guardam_duas_copias_sem_pisar_na_primeira() {
        let pasta = PastaTemporaria::nova("duas-corrupcoes");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        let chave_1 = fs::read(pasta.0.join("vault.key")).unwrap();
        fs::write(pasta.0.join("vault.json"), "{ estragado 1").unwrap();
        let (mut cofre, aviso) = Cofre::abrir(&pasta.0);
        assert!(aviso.is_some());
        cofre.salvar(item("banco")).unwrap();
        fs::write(pasta.0.join("vault.json"), "{ estragado 2").unwrap();
        let (_cofre, aviso) = Cofre::abrir(&pasta.0);
        let aviso = aviso.unwrap();

        assert_eq!(
            fs::read_to_string(pasta.0.join("vault.json.bak")).unwrap(),
            "{ estragado 1"
        );
        assert_eq!(fs::read(pasta.0.join("vault.key.bak")).unwrap(), chave_1);
        let jsons = nomes_com(&pasta.0, "vault.json.bak-");
        let chaves = nomes_com(&pasta.0, "vault.key.bak-");
        assert_eq!(jsons.len(), 1, "{jsons:?}");
        assert_eq!(chaves.len(), 1, "{chaves:?}");
        assert_eq!(
            jsons[0].strip_prefix("vault.json"),
            chaves[0].strip_prefix("vault.key"),
            "o par fica com o mesmo final"
        );
        assert!(aviso.contains(&jsons[0]), "{aviso}");
        assert_eq!(
            fs::read_to_string(pasta.0.join(&jsons[0])).unwrap(),
            "{ estragado 2"
        );
    }

    #[test]
    fn gravar_por_cima_de_um_cofre_ilegivel_guarda_antes_o_ilegivel() {
        let (mut cofre, pasta) = cofre_com_itens("ilegivel-no-disco");
        fs::write(pasta.0.join("vault.json"), "{ meio arquivo").unwrap();
        cofre.salvar(item("novo")).unwrap();
        assert_eq!(
            fs::read_to_string(pasta.0.join("vault.json.bak")).unwrap(),
            "{ meio arquivo"
        );
        let (reaberto, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert_eq!(titulos_do(&reaberto), ["banco", "novo", "web"]);
    }

    #[test]
    fn o_cofre_ganha_uma_copia_por_dia_que_abre_igual() {
        const DIA: u64 = 86_400;
        let inicio = 20_000 * DIA + 3_600;
        crate::recentes::fixar_relogio(Some(inicio));
        let (mut cofre, pasta) = cofre_com_itens("copia-do-dia");
        let copias = pasta.0.join(crate::armazenamento::PASTA_COPIAS);
        // O cofre novo não tinha o que copiar; a segunda gravação copia a primeira.
        assert_eq!(
            nomes_com(&copias, ""),
            ["20241004-vault.json", "20241004-vault.key"]
        );
        crate::recentes::fixar_relogio(Some(inicio + DIA));
        cofre.salvar(item("terca")).unwrap();
        crate::recentes::fixar_relogio(Some(inicio + DIA + 60));
        cofre.salvar(item("terca-de-novo")).unwrap();
        let nomes = nomes_com(&copias, "");
        assert_eq!(nomes.len(), 4, "{nomes:?}");
        crate::recentes::fixar_relogio(None);

        // A cópia de um dia, posta no lugar, abre com o que havia nele.
        let fora = PastaTemporaria::nova("copia-do-dia-fora");
        fs::create_dir_all(&fora.0).unwrap();
        for nome in ["vault.json", "vault.key"] {
            fs::copy(copias.join(format!("20241005-{nome}")), fora.0.join(nome)).unwrap();
        }
        let (antiga, aviso) = Cofre::abrir(&fora.0);
        assert_eq!(aviso, None);
        assert_eq!(titulos_do(&antiga), ["banco", "web"]);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(modo(&copias), 0o700);
            assert_eq!(modo(&copias.join("20241005-vault.key")), 0o600);
        }
    }

    #[test]
    fn numa_pasta_so_de_leitura_o_cofre_nao_grava_nada() {
        let pasta = PastaTemporaria::nova("cofre-so-leitura");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        let antes = texto_do_cofre(&pasta.0);
        let chave = fs::read(pasta.0.join("vault.key")).unwrap();
        crate::armazenamento::somente_leitura_em(&pasta.0);

        let (mut outra, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        assert_eq!(
            outra.itens()[0].titulo,
            "web",
            "a cópia só de leitura lê o cofre"
        );
        assert!(!pasta.0.join("copias").exists(), "nem a cópia do dia nasce");
        // Mexer no cofre fica só na memória.
        outra.salvar(item("db")).unwrap();
        outra.remover(outra.itens()[0].id).unwrap();
        assert_eq!(texto_do_cofre(&pasta.0), antes);
        assert_eq!(fs::read(pasta.0.join("vault.key")).unwrap(), chave);
        // O que a pessoa pede e mexeria nos arquivos devolve o erro.
        let erro = outra.definir_senha_mestra(Some("m")).unwrap_err();
        assert_eq!(erro.kind(), io::ErrorKind::ReadOnlyFilesystem);
        let fora = PastaTemporaria::nova("cofre-so-leitura-destino");
        let erro = outra.levar_para(&fora.0).unwrap_err();
        assert_eq!(erro.kind(), io::ErrorKind::ReadOnlyFilesystem);
        assert!(!fora.0.join("vault.json").exists());
        assert_eq!(texto_do_cofre(&pasta.0), antes);
        assert!(pasta.0.join("vault.key").exists());
    }

    #[test]
    fn a_principal_so_aceita_da_outra_copia_um_cofre_com_a_mesma_protecao() {
        let (cofre, pasta) = cofre_com_itens("outra-copia");
        let (_outro, outra_pasta) = cofre_com_itens("outra-copia-outra-chave");
        let proprio = fs::read(pasta.0.join("vault.json")).unwrap();
        assert_eq!(cofre.aceita_de_outra_copia(&proprio), Ok(()));
        // Outra chave: a principal não abriria o que gravasse.
        let de_outra_chave = fs::read(outra_pasta.0.join("vault.json")).unwrap();
        assert_eq!(
            cofre.aceita_de_outra_copia(&de_outra_chave),
            Err(t!("cofre-alterado-com-outra-chave"))
        );
        let mut json: serde_json::Value = serde_json::from_slice(&proprio).unwrap();
        json["versao"] = (VERSAO + 1).into();
        assert_eq!(
            cofre.aceita_de_outra_copia(json.to_string().as_bytes()),
            Err(t!("cofre-versao-mais-nova"))
        );
        assert!(cofre.aceita_de_outra_copia(b"{ pela metade").is_err());

        // Com a senha mestra, a outra senha também fica de fora.
        let (mut com_senha, pasta_com_senha) = cofre_com_itens("outra-copia-senha");
        com_senha.definir_senha_mestra(Some("uma")).unwrap();
        let (mut outra_senha, pasta_outra_senha) = cofre_com_itens("outra-copia-outra-senha");
        outra_senha.definir_senha_mestra(Some("outra")).unwrap();
        let proprio = fs::read(pasta_com_senha.0.join("vault.json")).unwrap();
        assert_eq!(com_senha.aceita_de_outra_copia(&proprio), Ok(()));
        let de_outra_senha = fs::read(pasta_outra_senha.0.join("vault.json")).unwrap();
        assert_eq!(
            com_senha.aceita_de_outra_copia(&de_outra_senha),
            Err(t!("cofre-alterado-com-outra-senha"))
        );
    }

    #[test]
    fn numa_pasta_so_de_leitura_sem_cofre_nao_nasce_a_chave() {
        let pasta = PastaTemporaria::nova("cofre-so-leitura-vazio");
        fs::create_dir_all(&pasta.0).unwrap();
        crate::armazenamento::somente_leitura_em(&pasta.0);
        let (mut cofre, aviso) = Cofre::abrir(&pasta.0);
        assert_eq!(aviso, None);
        cofre.salvar(item("web")).unwrap();
        assert!(!pasta.0.join("vault.key").exists());
        assert!(!pasta.0.join("vault.json").exists());
    }

    #[test]
    fn numa_pasta_so_de_leitura_o_cofre_ilegivel_fica_no_lugar_e_trancado() {
        let pasta = PastaTemporaria::nova("cofre-so-leitura-ilegivel");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item("web")).unwrap();
        fs::write(pasta.0.join("vault.json"), "{ pela metade").unwrap();
        crate::armazenamento::somente_leitura_em(&pasta.0);

        let (mut outra, aviso) = Cofre::abrir(&pasta.0);
        assert!(aviso.is_some());
        assert_eq!(texto_do_cofre(&pasta.0), "{ pela metade");
        assert!(pasta.0.join("vault.key").exists());
        assert!(!pasta.0.join("vault.json.bak").exists());
        assert!(!pasta.0.join("vault.key.bak").exists());
        assert!(
            outra.salvar(item("db")).is_err(),
            "trancado, como o de versão mais nova"
        );
        assert_eq!(texto_do_cofre(&pasta.0), "{ pela metade");
    }
}
