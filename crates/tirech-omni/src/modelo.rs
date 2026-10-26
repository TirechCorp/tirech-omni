//! O modelo das conexões: a árvore de grupos e conexões que o app guarda no disco. É dado puro
//! (sem widget nem signal), para os testes de lógica não precisarem de janela.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::campos::Campo;
use crate::configuracoes::{ColarArquivos, Configuracoes, S3Cfg};
use crate::mcp::permissao::PermissaoIa;
use crate::redes::{PerfilRede, Rede};
use crate::t;

/// A versão do formato do `connections.json`.
pub const VERSAO: u32 = 1;

/// Como a conexão chega ao outro lado.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    /// SSH (`Ssh`), a conexão padrão.
    #[default]
    Ssh,
    /// O shell do sistema, num pseudoterminal (`Process`). Não usa host, porta nem usuário.
    Local,
    /// Telnet.
    Telnet,
    /// Só arquivos pelo SFTP (M30): abre na página Arquivos, sem terminal. Usa os mesmos campos
    /// do SSH, menos o tipo de terminal.
    Sftp,
    /// Arquivos pelo FTP ou FTPS (M31): também abre na página Arquivos. Usa host, porta,
    /// usuário e a credencial do cofre; as opções do protocolo ficam nos campos `ftp_*`.
    Ftp,
    /// Um bucket S3 (AWS, MinIO, R2…) (M32): também abre na página Arquivos. O usuário é o
    /// Access Key ID, a senha do cofre é a chave secreta; o resto fica em [`S3Opcoes`].
    S3,
    /// Um remoto do rclone (M33): Google Drive, OneDrive, Dropbox, WebDAV… O remoto e a pasta
    /// ficam em [`RcloneOpcoes`]; as senhas dele moram no `rclone.conf`, não aqui.
    Rclone,
}

/// Como a conexão FTP é protegida.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FtpSeguranca {
    /// TLS explícito (`AUTH TLS`); recusa se o servidor não aceitar.
    #[default]
    Explicita,
    /// TLS explícito se o servidor aceitar; senão segue em claro.
    ExplicitaSeDisponivel,
    /// FTPS implícito: o TLS começa na conexão (porta 990).
    Implicita,
    /// FTP simples, sem criptografia.
    Nenhuma,
}

impl FtpSeguranca {
    /// Todas, na ordem em que a lista de escolha as mostra.
    pub const TODAS: [FtpSeguranca; 4] = [
        FtpSeguranca::Explicita,
        FtpSeguranca::ExplicitaSeDisponivel,
        FtpSeguranca::Implicita,
        FtpSeguranca::Nenhuma,
    ];

    /// O rótulo que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            FtpSeguranca::Explicita => t!("comum-ftp-seguranca-explicita"),
            FtpSeguranca::ExplicitaSeDisponivel => t!("comum-ftp-seguranca-se-disponivel"),
            FtpSeguranca::Implicita => t!("comum-ftp-seguranca-implicita"),
            FtpSeguranca::Nenhuma => t!("comum-ftp-seguranca-nenhuma"),
        }
    }

    /// A porta padrão da segurança: 990 no FTPS implícito, 21 nas outras.
    pub fn porta_padrao(self) -> u16 {
        if self == FtpSeguranca::Implicita {
            990
        } else {
            21
        }
    }
}

/// Quem abre a conexão de dados do FTP.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FtpModo {
    /// O cliente conecta ao servidor. No arquivo vai como `Pasv` (o comando do FTP): o teste
    /// `a_senha_nunca_vai_para_o_json` recusa qualquer "pass" no `connections.json`.
    #[default]
    #[serde(rename = "Pasv")]
    Passivo,
    /// O servidor conecta ao cliente.
    Ativo,
}

impl FtpModo {
    /// Todos, na ordem da lista de escolha.
    pub const TODOS: [FtpModo; 2] = [FtpModo::Passivo, FtpModo::Ativo];

    /// O rótulo que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            FtpModo::Passivo => t!("comum-ftp-modo-passivo"),
            FtpModo::Ativo => t!("comum-ftp-modo-ativo"),
        }
    }
}

/// A versão do TLS do FTPS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FtpTls {
    /// A mais nova que o servidor aceitar.
    #[default]
    Auto,
    /// Só o TLS 1.2.
    Tls12,
    /// Só o TLS 1.3.
    Tls13,
}

impl FtpTls {
    /// Todas, na ordem da lista de escolha.
    pub const TODAS: [FtpTls; 3] = [FtpTls::Auto, FtpTls::Tls12, FtpTls::Tls13];

    /// O rótulo que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            FtpTls::Auto => t!("comum-ftp-tls-auto"),
            FtpTls::Tls12 => t!("comum-ftp-tls-12"),
            FtpTls::Tls13 => t!("comum-ftp-tls-13"),
        }
    }
}

/// A codificação dos nomes de arquivo no FTP.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FtpCodificacao {
    /// UTF-8 se o servidor disser; senão Latin-1.
    #[default]
    Auto,
    /// Sempre UTF-8.
    Utf8,
    /// Sempre Latin-1.
    Latin1,
}

impl FtpCodificacao {
    /// Todas, na ordem da lista de escolha.
    pub const TODAS: [FtpCodificacao; 3] = [
        FtpCodificacao::Auto,
        FtpCodificacao::Utf8,
        FtpCodificacao::Latin1,
    ];

    /// O rótulo que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            FtpCodificacao::Auto => t!("comum-ftp-codificacao-auto"),
            FtpCodificacao::Utf8 => "UTF-8".to_owned(),
            FtpCodificacao::Latin1 => "Latin-1".to_owned(),
        }
    }
}

/// O serviço S3 da conexão: escolher um preenche o endereço, a região e o estilo da URL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum S3Provedor {
    /// A Amazon S3.
    #[default]
    Amazon,
    /// Um MinIO.
    Minio,
    /// O Cloudflare R2.
    CloudflareR2,
    /// O Wasabi.
    Wasabi,
    /// O Backblaze B2.
    BackblazeB2,
    /// O DigitalOcean Spaces.
    DigitalOcean,
    /// O Ceph RGW ou qualquer outro serviço compatível.
    Outro,
}

impl S3Provedor {
    /// Todos os provedores, na ordem da lista de escolha.
    pub const TODOS: [S3Provedor; 7] = [
        S3Provedor::Amazon,
        S3Provedor::Minio,
        S3Provedor::CloudflareR2,
        S3Provedor::Wasabi,
        S3Provedor::BackblazeB2,
        S3Provedor::DigitalOcean,
        S3Provedor::Outro,
    ];

    /// O nome que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            S3Provedor::Amazon => "Amazon S3".to_owned(),
            S3Provedor::Minio => "MinIO".to_owned(),
            S3Provedor::CloudflareR2 => "Cloudflare R2".to_owned(),
            S3Provedor::Wasabi => "Wasabi".to_owned(),
            S3Provedor::BackblazeB2 => "Backblaze B2".to_owned(),
            S3Provedor::DigitalOcean => "DigitalOcean Spaces".to_owned(),
            S3Provedor::Outro => t!("comum-s3-provedor-outro"),
        }
    }

    /// O endereço do serviço; `{região}` e `{conta}` são o que a pessoa preenche.
    pub fn modelo_do_endereco(self) -> String {
        match self {
            S3Provedor::Amazon => t!("s3-modelo-amazon"),
            S3Provedor::Minio => "http://127.0.0.1:9000".to_owned(),
            S3Provedor::CloudflareR2 => t!("s3-modelo-r2"),
            S3Provedor::Wasabi => t!("s3-modelo-wasabi"),
            S3Provedor::BackblazeB2 => t!("s3-modelo-b2"),
            S3Provedor::DigitalOcean => t!("s3-modelo-digitalocean"),
            S3Provedor::Outro => String::new(),
        }
    }

    /// O endereço do serviço na `regiao`.
    pub fn endereco(self, regiao: &str) -> String {
        self.modelo_do_endereco()
            .replace("{região}", regiao)
            .replace("{region}", regiao)
    }

    /// A região de uma conexão nova.
    pub fn regiao_padrao(self) -> &'static str {
        match self {
            S3Provedor::CloudflareR2 => "auto",
            S3Provedor::BackblazeB2 => "us-west-004",
            S3Provedor::DigitalOcean => "nyc3",
            _ => "us-east-1",
        }
    }

    /// O estilo de URL que o serviço espera.
    pub fn estilo(self) -> S3Estilo {
        match self {
            S3Provedor::Amazon
            | S3Provedor::Wasabi
            | S3Provedor::BackblazeB2
            | S3Provedor::DigitalOcean => S3Estilo::Virtual,
            S3Provedor::Minio | S3Provedor::CloudflareR2 | S3Provedor::Outro => S3Estilo::Caminho,
        }
    }
}

/// Como o bucket entra na URL do S3.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum S3Estilo {
    /// `https://servidor/bucket/chave`.
    #[default]
    Caminho,
    /// `https://bucket.servidor/chave`.
    Virtual,
}

impl S3Estilo {
    /// Todos os estilos, na ordem da lista de escolha.
    pub const TODOS: [S3Estilo; 2] = [S3Estilo::Caminho, S3Estilo::Virtual];

    /// O nome que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            S3Estilo::Caminho => t!("comum-s3-estilo-caminho"),
            S3Estilo::Virtual => t!("comum-s3-estilo-virtual"),
        }
    }
}

/// Como o certificado do servidor S3 é conferido.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum S3Certificado {
    /// Pelas autoridades do sistema.
    #[default]
    Verificar,
    /// Pelas do sistema e mais as de um arquivo PEM.
    CaPropria,
    /// Aceita qualquer certificado (um MinIO de laboratório).
    NaoVerificar,
}

impl S3Certificado {
    /// Todos os modos, na ordem da lista de escolha.
    pub const TODOS: [S3Certificado; 3] = [
        S3Certificado::Verificar,
        S3Certificado::CaPropria,
        S3Certificado::NaoVerificar,
    ];

    /// O nome que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            S3Certificado::Verificar => t!("comum-s3-certificado-verificar"),
            S3Certificado::CaPropria => t!("comum-s3-certificado-ca-propria"),
            S3Certificado::NaoVerificar => t!("comum-s3-certificado-nao-verificar"),
        }
    }
}

/// As opções de uma conexão S3.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct S3Opcoes {
    /// O serviço.
    pub provider: S3Provedor,
    /// O endereço do serviço (`https://s3.us-east-1.amazonaws.com`).
    pub endpoint: String,
    /// A região.
    pub region: String,
    /// O bucket. Vazio: a raiz lista os buckets da conta.
    pub bucket: String,
    /// Como o bucket entra na URL.
    pub url_style: S3Estilo,
    /// A classe de armazenamento dos envios (`STANDARD_IA`…); vazia: a do bucket.
    pub storage_class: String,
    /// Como o certificado é conferido.
    pub certificate: S3Certificado,
    /// O arquivo PEM da CA própria (só no modo "CA própria").
    pub ca_file: Option<PathBuf>,
}

impl Default for S3Opcoes {
    fn default() -> Self {
        Self::do_provedor(S3Provedor::Amazon)
    }
}

impl S3Opcoes {
    /// As opções de uma conexão nova no `provider`.
    pub fn do_provedor(provider: S3Provedor) -> Self {
        let region = provider.regiao_padrao().to_owned();
        Self {
            provider,
            endpoint: provider.endereco(&region),
            region,
            bucket: String::new(),
            url_style: provider.estilo(),
            storage_class: String::new(),
            certificate: S3Certificado::Verificar,
            ca_file: None,
        }
    }

    /// Aplica os padrões das configurações: a classe de armazenamento e, no provedor "outro",
    /// a região e o estilo do endereço.
    pub fn com_config(&mut self, config: &S3Cfg) {
        self.storage_class.clone_from(&config.classe);
        self.padroes_do_outro(config);
    }

    /// No provedor "outro", a região e o estilo do endereço vêm das configurações.
    pub fn padroes_do_outro(&mut self, config: &S3Cfg) {
        if self.provider == S3Provedor::Outro {
            self.region.clone_from(&config.regiao);
            self.url_style = config.estilo;
        }
    }

    /// Troca o provedor e preenche o endereço, a região e o estilo; o bucket, a classe e o
    /// certificado ficam.
    pub fn trocar_provedor(&mut self, provider: S3Provedor) {
        let novo = Self::do_provedor(provider);
        self.provider = provider;
        self.endpoint = novo.endpoint;
        self.region = novo.region;
        self.url_style = novo.url_style;
    }

    /// Ajusta `nova` (as opções como a tela as editou) ao que mudou desde `self`: um provedor
    /// novo preenche o endereço, a região e o estilo; uma região nova leva o endereço junto se
    /// ele ainda era o do provedor.
    pub fn acertar(&self, nova: &mut S3Opcoes) {
        if nova.provider != self.provider {
            nova.trocar_provedor(nova.provider);
        } else if nova.region != self.region && nova.endpoint == self.endpoint {
            let mut antes = self.clone();
            antes.trocar_regiao(&nova.region);
            nova.endpoint = antes.endpoint;
        }
    }

    /// Troca a região. O endereço acompanha se ainda era o do provedor na região de antes.
    pub fn trocar_regiao(&mut self, regiao: &str) {
        if self.endpoint == self.provider.endereco(&self.region) {
            self.endpoint = self.provider.endereco(regiao);
        }
        self.region = regiao.to_owned();
    }
}

/// As opções de uma conexão do rclone: qual remoto do `rclone.conf` e em que pasta ele abre.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RcloneOpcoes {
    /// O nome do remoto no `rclone.conf` (`gdrive`), sem os dois-pontos.
    pub remote: String,
    /// A pasta em que a conexão abre, dentro do remoto (vazia: a raiz).
    pub path: String,
}

impl RcloneOpcoes {
    /// O endereço que o rclone entende (`gdrive:` ou `gdrive:Backup`).
    pub fn spec(&self) -> String {
        format!("{}:{}", self.remote, self.path.trim_matches('/'))
    }
}

impl Kind {
    /// Todos os tipos, na ordem em que a lista de escolha os mostra.
    pub const TODOS: [Kind; 7] = [
        Kind::Ssh,
        Kind::Sftp,
        Kind::Ftp,
        Kind::S3,
        Kind::Rclone,
        Kind::Local,
        Kind::Telnet,
    ];

    /// Se a página Arquivos abre o tipo (SSH, SFTP, FTP, S3 e rclone).
    pub fn tem_arquivos(self) -> bool {
        self.usa_ssh() || matches!(self, Kind::Ftp | Kind::S3 | Kind::Rclone)
    }

    /// Se o tipo abre na página Arquivos, sem terminal (SFTP, FTP, S3 e rclone).
    pub fn so_arquivos(self) -> bool {
        matches!(self, Kind::Sftp | Kind::Ftp | Kind::S3 | Kind::Rclone)
    }

    /// O nome que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            Kind::Ssh => "SSH".to_owned(),
            Kind::Local => t!("comum-tipo-shell-local"),
            Kind::Telnet => "Telnet".to_owned(),
            Kind::Sftp => "SFTP".to_owned(),
            Kind::Ftp => "FTP".to_owned(),
            Kind::S3 => "S3".to_owned(),
            Kind::Rclone => "Rclone".to_owned(),
        }
    }

    /// Se o tipo conecta por SSH (usa servidor, usuário, senha, chave, keepalive e salto).
    pub fn usa_ssh(self) -> bool {
        matches!(self, Kind::Ssh | Kind::Sftp)
    }

    /// A porta padrão do tipo (0 no shell local, que não usa porta).
    pub fn porta_padrao(self) -> u16 {
        match self {
            Kind::Ssh | Kind::Sftp => 22,
            Kind::Local => 0,
            Kind::Telnet => 23,
            Kind::Ftp => 21,
            // O S3 usa a porta do endereço.
            Kind::S3 | Kind::Rclone => 0,
        }
    }
}

/// O servidor de salto (o `ProxyJump` do OpenSSH) por onde a conexão passa.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Jump {
    /// O servidor de salto.
    pub host: String,
    /// A porta do salto.
    pub port: u16,
    /// O usuário no salto.
    pub user: String,
    /// O item do cofre com a senha do salto. Um id que não existe mais vale como `None`.
    #[serde(default)]
    pub credencial: Option<u64>,
}

/// Se o X11 do servidor abre as janelas aqui (M37), e com que confiança.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModoX11 {
    /// O que a configuração `x11.padrao` diz.
    #[default]
    Padrao,
    /// Sem X11.
    Desligado,
    /// O `ssh -X`: um cookie limitado pela extensão SECURITY do servidor X.
    NaoConfiavel,
    /// O `ssh -Y`: o cookie de verdade.
    Confiavel,
}

impl ModoX11 {
    /// Todos, na ordem da lista de escolha.
    pub const TODOS: [ModoX11; 4] = [
        ModoX11::Padrao,
        ModoX11::Desligado,
        ModoX11::NaoConfiavel,
        ModoX11::Confiavel,
    ];

    /// O rótulo que a tela mostra (o `Padrao` ganha o modo de verdade entre parênteses na lista).
    pub fn nome(self) -> String {
        match self {
            ModoX11::Padrao => t!("comum-x11-padrao"),
            ModoX11::Desligado => t!("comum-x11-desligado"),
            ModoX11::NaoConfiavel => t!("comum-x11-nao-confiavel"),
            ModoX11::Confiavel => t!("comum-x11-confiavel"),
        }
    }
}

/// Como uma conexão SSH (ou SFTP) sem item do cofre na `credencial` se autentica: a linha
/// "Credencial" das propriedades. Com um item do cofre, o item vence e o modo não vale.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModoCredencial {
    /// Pede a senha antes de abrir (sem chave privada nem chave do cofre).
    #[default]
    Perguntar,
    /// Não pergunta: o agente SSH (e a chave, se houver). Uma recusa só mostra o erro na aba.
    Agente,
    /// Não pergunta antes: tenta o agente e, se o servidor recusar, pede a senha e reconecta.
    AgenteOuSenha,
}

impl ModoCredencial {
    /// Todos, na ordem da lista de escolha.
    pub const TODOS: [ModoCredencial; 3] = [
        ModoCredencial::Perguntar,
        ModoCredencial::Agente,
        ModoCredencial::AgenteOuSenha,
    ];

    /// O rótulo da linha "Credencial".
    pub fn nome(self) -> String {
        match self {
            ModoCredencial::Perguntar => t!("casca-perguntar-ao-conectar"),
            ModoCredencial::Agente => t!("comum-credencial-agente"),
            ModoCredencial::AgenteOuSenha => t!("comum-credencial-agente-ou-senha"),
        }
    }

    /// O modo cujo rótulo é `rotulo`; um item do cofre (ou um texto qualquer) vale como
    /// `Perguntar`.
    pub fn do_rotulo(rotulo: &str) -> Self {
        Self::TODOS
            .into_iter()
            .find(|modo| modo.nome() == rotulo)
            .unwrap_or_default()
    }

    /// Se é o padrão (`Perguntar`), que o arquivo não grava.
    pub fn e_perguntar(&self) -> bool {
        *self == ModoCredencial::Perguntar
    }
}

/// O shell que atende o outro lado da conexão SSH, para o app citar caminhos e montar comandos
/// ao colar arquivos (M45). `Automatico` descobre pelo servidor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellRemoto {
    /// Descobre ao conectar.
    #[default]
    Automatico,
    /// `sh`, `bash`, `zsh` e parentes.
    Posix,
    /// O PowerShell do Windows.
    PowerShell,
    /// O `cmd.exe` do Windows.
    Cmd,
}

impl ShellRemoto {
    /// Todos, na ordem da lista de escolha.
    pub const TODOS: [ShellRemoto; 4] = [
        ShellRemoto::Automatico,
        ShellRemoto::Posix,
        ShellRemoto::PowerShell,
        ShellRemoto::Cmd,
    ];

    /// O nome na tela, no idioma atual.
    pub fn nome(self) -> String {
        match self {
            ShellRemoto::Automatico => t!("comum-shell-remoto-automatico"),
            ShellRemoto::Posix => t!("comum-shell-remoto-posix"),
            ShellRemoto::PowerShell => t!("comum-shell-remoto-powershell"),
            ShellRemoto::Cmd => t!("comum-shell-remoto-cmd"),
        }
    }

    /// Para o `skip_serializing_if`: o padrão não vai para o arquivo.
    pub fn is_automatico(&self) -> bool {
        *self == ShellRemoto::Automatico
    }
}

/// Os assistentes de IA nesta conexão (M46). Vazio: vale o das Configurações.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IaConexao {
    /// A pasta em que os assistentes abrem; vazia, a do terminal, a pasta remota ou o home.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pasta: String,
    /// Como instalar; `None` segue as Configurações.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metodo: Option<crate::ia::Metodo>,
    /// Os argumentos de cada ferramenta, numa linha; quando existem, substituem os globais.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub argumentos: BTreeMap<crate::ia::Ferramenta, String>,
}

impl IaConexao {
    /// Para o `skip_serializing_if`: nada preenchido não vai para o arquivo.
    pub fn vazio(&self) -> bool {
        self.pasta.is_empty() && self.metodo.is_none() && self.argumentos.is_empty()
    }
}

/// O tipo de um túnel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TipoTunel {
    /// `-L`: uma porta daqui leva a um destino visto do servidor.
    #[default]
    Local,
    /// `-R`: uma porta do servidor leva a um destino visto daqui.
    Remoto,
    /// `-D`: um proxy SOCKS5 aqui.
    Dinamico,
    /// Um socket Unix do servidor (o libvirt, o Docker) numa porta ou num socket daqui.
    SocketUnix,
}

impl TipoTunel {
    /// Todos, na ordem da lista de escolha.
    pub const TODOS: [TipoTunel; 4] = [
        TipoTunel::Local,
        TipoTunel::Remoto,
        TipoTunel::Dinamico,
        TipoTunel::SocketUnix,
    ];

    /// O rótulo que a tela mostra.
    pub fn nome(self) -> String {
        match self {
            TipoTunel::Local => t!("comum-tunel-local-nome"),
            TipoTunel::Remoto => t!("comum-tunel-remoto-nome"),
            TipoTunel::Dinamico => t!("comum-tunel-dinamico-nome"),
            TipoTunel::SocketUnix => t!("comum-tunel-socket-nome"),
        }
    }

    /// O rótulo curto das colunas.
    pub fn curto(self) -> String {
        match self {
            TipoTunel::Local => "Local".to_owned(),
            TipoTunel::Remoto => t!("comum-tunel-remoto-curto"),
            TipoTunel::Dinamico => "SOCKS5".to_owned(),
            TipoTunel::SocketUnix => "Socket".to_owned(),
        }
    }
}

/// Uma opção de uma conexão que pode seguir a configuração global (M41).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ativar {
    /// Segue a configuração global.
    #[default]
    Padrao,
    /// Ligada nesta conexão, seja qual for a configuração global.
    Sim,
    /// Desligada nesta conexão, seja qual for a configuração global.
    Nao,
}

impl Ativar {
    /// Todos, na ordem da lista de escolha.
    pub const TODOS: [Ativar; 3] = [Ativar::Padrao, Ativar::Sim, Ativar::Nao];

    /// O valor de verdade: `Sim` e `Nao` mandam, e `Padrao` segue o `global`.
    pub fn resolver(self, global: bool) -> bool {
        match self {
            Ativar::Padrao => global,
            Ativar::Sim => true,
            Ativar::Nao => false,
        }
    }

    /// Para o `skip_serializing_if`: o padrão não é gravado.
    fn e_padrao(&self) -> bool {
        *self == Ativar::Padrao
    }

    /// O rótulo que a lista de escolha mostra.
    pub fn nome(self) -> String {
        match self {
            Ativar::Padrao => t!("comum-ativar-padrao"),
            Ativar::Sim => t!("comum-ativar-ligado"),
            Ativar::Nao => t!("comum-ativar-desligado"),
        }
    }
}

fn verdadeiro() -> bool {
    true
}

/// Se a aba SSH roda dentro de uma sessão do `tmux` ou do `screen` no servidor, que sobrevive à
/// queda da conexão (a sessão persistente).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessaoPersistente {
    /// Vale a da pasta de cima (e, sem nenhuma, nenhuma).
    #[default]
    Herdar,
    /// O shell comum, mesmo dentro de uma pasta com tmux.
    Nenhuma,
    /// A aba abre (ou volta a) uma sessão do `tmux` no servidor.
    Tmux,
    /// A aba abre (ou volta a) uma sessão do GNU `screen` no servidor.
    Screen,
}

impl SessaoPersistente {
    /// Para o `skip_serializing_if`: o padrão não é gravado.
    pub fn is_herdar(&self) -> bool {
        *self == SessaoPersistente::Herdar
    }
}

/// O programa que guarda a sessão persistente no servidor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Multiplexador {
    Tmux,
    Screen,
}

impl Multiplexador {
    /// O nome do programa, como se digita no servidor.
    pub fn programa(self) -> &'static str {
        match self {
            Multiplexador::Tmux => "tmux",
            Multiplexador::Screen => "screen",
        }
    }
}

/// A sessão persistente em que uma aba roda: o programa e o nome da sessão.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SessaoDaAba {
    pub programa: Multiplexador,
    pub nome: String,
}

impl SessaoDaAba {
    pub fn new(programa: Multiplexador, nome: impl Into<String>) -> Self {
        Self {
            programa,
            nome: nome.into(),
        }
    }
}

/// De onde vêm os comandos ao conectar de um nó (M50).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModoComandos {
    /// Valem os da pasta de cima (e, sem nenhuma, nada).
    #[default]
    Herdar,
    /// Valem os passos deste nó, mesmo que a lista esteja vazia.
    Proprios,
    /// Não roda nada, mesmo dentro de uma pasta com passos.
    Nenhum,
}

/// Os comandos que o app digita depois que a sessão de uma conexão (ou de tudo o que há numa
/// pasta) abre (M50).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComandosAoConectar {
    /// De onde vêm os passos.
    #[serde(default)]
    pub modo: ModoComandos,
    /// Os passos, na ordem em que rodam. Sem teto.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passos: Vec<Passo>,
    /// Rodar de novo quando a sessão reconecta.
    #[serde(default = "verdadeiro")]
    pub reconectar: bool,
}

impl Default for ComandosAoConectar {
    fn default() -> Self {
        Self {
            modo: ModoComandos::Herdar,
            passos: Vec::new(),
            reconectar: true,
        }
    }
}

impl ComandosAoConectar {
    /// Se é o que um nó novo tem: o campo some do JSON.
    pub fn e_padrao(&self) -> bool {
        *self == Self::default()
    }

    /// Os ids dos itens do cofre que os passos de senha usam como origem.
    pub fn ids_do_cofre(&self) -> impl Iterator<Item = u64> + '_ {
        self.passos.iter().filter_map(|p| match &p.acao {
            AcaoDoPasso::Senha {
                origem: OrigemDaSenha::Cofre { id },
                ..
            } => Some(*id),
            _ => None,
        })
    }

    /// Troca o id de cada origem do cofre por `f(id)`; sem resposta, a senha passa a ser a da
    /// conexão (o item não existe no cofre de destino).
    pub fn religar_senhas(&mut self, f: impl Fn(u64) -> Option<u64>) {
        for passo in &mut self.passos {
            if let AcaoDoPasso::Senha { origem, .. } = &mut passo.acao
                && let OrigemDaSenha::Cofre { id } = origem
            {
                *origem = match f(*id) {
                    Some(novo) => OrigemDaSenha::Cofre { id: novo },
                    None => OrigemDaSenha::Conexao,
                };
            }
        }
    }
}

/// Um passo dos comandos ao conectar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passo {
    /// O que o passo faz.
    pub acao: AcaoDoPasso,
    /// Quando o comando sai (o passo de senha ignora: o texto dele manda).
    #[serde(default)]
    pub espera: Espera,
    /// Segundos até desistir do passo.
    #[serde(default = "max_secs_padrao")]
    pub max_secs: u32,
}

/// O tempo máximo de um passo quando o arquivo não diz, em segundos.
pub const MAX_SECS_PADRAO: u32 = 15;

fn max_secs_padrao() -> u32 {
    MAX_SECS_PADRAO
}

impl Passo {
    /// Um passo de comando que espera a tela parar.
    pub fn comando(texto: impl Into<String>, max_secs: u32) -> Self {
        Self {
            acao: AcaoDoPasso::Comando {
                texto: texto.into(),
            },
            espera: Espera::TelaParar,
            max_secs,
        }
    }

    /// Um passo "Enviar a senha" de `origem`, que espera o pedido padrão.
    pub fn senha(origem: OrigemDaSenha, max_secs: u32) -> Self {
        Self {
            acao: AcaoDoPasso::Senha {
                origem,
                texto: TEXTO_DA_SENHA.to_owned(),
            },
            espera: Espera::TelaParar,
            max_secs,
        }
    }
}

/// O que um passo faz.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum AcaoDoPasso {
    /// Digita `texto` e o Enter.
    Comando {
        /// O comando, de uma linha.
        texto: String,
    },
    /// Digita a senha e o Enter quando `texto` aparece na linha do cursor.
    Senha {
        /// De onde vem a senha. O arquivo guarda só isto, nunca a senha.
        origem: OrigemDaSenha,
        /// O pedido que se espera: alternativas separadas por `|`.
        texto: String,
    },
}

/// De onde vem a senha de um passo "Enviar a senha".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "de", rename_all = "snake_case")]
pub enum OrigemDaSenha {
    /// A senha da própria conexão.
    Conexao,
    /// A senha de um item do cofre. Um id que não existe mais não envia nada.
    Cofre {
        /// O id do item.
        id: u64,
    },
}

/// Quando o comando de um passo sai.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "quando", rename_all = "snake_case")]
pub enum Espera {
    /// Chegou saída e depois nada por um instante.
    #[default]
    TelaParar,
    /// Apareceu um trecho (alternativas separadas por `|`, sem diferenciar maiúsculas).
    Texto {
        /// O trecho.
        texto: String,
    },
    /// Sem esperar.
    Logo,
}

/// O pedido de senha que o passo "Enviar a senha" espera, se o usuário não mudar.
pub const TEXTO_DA_SENHA: &str = "password|senha";

/// O socket que o libvirt abre no servidor, o padrão de um túnel de socket Unix.
pub const SOCKET_DO_LIBVIRT: &str = "/var/run/libvirt/libvirt-sock";

/// Um túnel de uma conexão SSH (M37). Só o SSH usa; os outros tipos ignoram.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tunel {
    /// O tipo.
    #[serde(default)]
    pub tipo: TipoTunel,
    /// A porta que escuta (aqui, ou no servidor no `Remoto`); 0 só vale no socket Unix.
    #[serde(default)]
    pub porta: u16,
    /// O destino: o servidor (`Local`, visto do servidor) ou esta máquina (`Remoto`).
    #[serde(default)]
    pub destino_host: String,
    /// A porta do destino.
    #[serde(default)]
    pub destino_porta: u16,
    /// O caminho do socket no servidor (`SocketUnix`).
    #[serde(default)]
    pub caminho_remoto: String,
    /// O socket daqui (`SocketUnix`); vazio, um caminho automático.
    #[serde(default)]
    pub socket_local: String,
    /// Escuta em todas as interfaces e não só no `127.0.0.1`.
    #[serde(default)]
    pub outras_maquinas: bool,
    /// Sobe junto com a conexão.
    #[serde(default = "verdadeiro")]
    pub ligado: bool,
    /// Um nome opcional, para a lista.
    #[serde(default)]
    pub nome: String,
    /// Se a porta estiver ocupada ao conectar, usa a próxima livre (M58).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proxima_livre: bool,
    /// O caminho que "Abrir no navegador" usa (`/phpmyadmin/`); vazio, `/` (M58).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub caminho_web: String,
    /// O nome do site (o `Host`); com ele, o túnel passa pelo relé HTTP (M58).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_http: String,
    /// O servidor fala TLS nesta porta: o relé abre o TLS e o navegador fala HTTP (M58).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub https: bool,
}

impl Tunel {
    /// Um túnel do tipo dado, ligado, sem destino.
    pub fn novo(tipo: TipoTunel, outras_maquinas: bool) -> Self {
        Self {
            tipo,
            porta: 0,
            destino_host: String::new(),
            destino_porta: 0,
            caminho_remoto: if tipo == TipoTunel::SocketUnix {
                SOCKET_DO_LIBVIRT.to_owned()
            } else {
                String::new()
            },
            socket_local: String::new(),
            outras_maquinas,
            ligado: true,
            nome: String::new(),
            proxima_livre: false,
            caminho_web: String::new(),
            host_http: String::new(),
            https: false,
        }
    }

    /// Se o túnel passa pelo relé do app (nome do site ou HTTPS) e não pelo `-L` do SSH (M58).
    pub fn usa_rele(&self) -> bool {
        self.tipo == TipoTunel::Local && (self.https || !self.host_http.trim().is_empty())
    }

    /// O destino como `host:porta` (IPv6 entre colchetes), o caminho no `SocketUnix` e `SOCKS5`
    /// no `Dinamico`.
    pub fn destino(&self) -> String {
        match self.tipo {
            TipoTunel::Dinamico => "SOCKS5".to_owned(),
            TipoTunel::SocketUnix => self.caminho_remoto.clone(),
            TipoTunel::Local | TipoTunel::Remoto => {
                if self.destino_host.contains(':') {
                    format!("[{}]:{}", self.destino_host, self.destino_porta)
                } else {
                    format!("{}:{}", self.destino_host, self.destino_porta)
                }
            }
        }
    }

    /// Onde o túnel escuta, como a lista mostra (antes de subir).
    pub fn escuta(&self) -> String {
        let endereco = if self.outras_maquinas {
            "0.0.0.0"
        } else {
            "127.0.0.1"
        };
        match self.tipo {
            TipoTunel::Local | TipoTunel::Dinamico => format!("{endereco}:{}", self.porta),
            TipoTunel::Remoto => t!("tuneis-servidor-porta", porta = i64::from(self.porta)),
            TipoTunel::SocketUnix if !self.socket_local.trim().is_empty() => {
                self.socket_local.trim().to_owned()
            }
            TipoTunel::SocketUnix if self.porta > 0 => format!("{endereco}:{}", self.porta),
            TipoTunel::SocketUnix => t!("tuneis-socket-automatico"),
        }
    }

    /// O que falta para o túnel valer, ou `None`.
    pub fn problema(&self) -> Option<String> {
        let sem_porta = self.porta == 0 && self.tipo != TipoTunel::SocketUnix;
        match self.tipo {
            _ if sem_porta => Some(t!("tuneis-escolha-a-porta")),
            TipoTunel::Local | TipoTunel::Remoto
                if self.destino_host.trim().is_empty() || self.destino_porta == 0 =>
            {
                Some(t!("tuneis-destino-precisa-de-servidor-e-porta"))
            }
            TipoTunel::SocketUnix if self.caminho_remoto.trim().is_empty() => {
                Some(t!("tuneis-diga-o-caminho-do-socket"))
            }
            _ => None,
        }
    }

    /// Uma linha para a lista: o nome, se houver, senão o que o túnel faz.
    pub fn titulo(&self) -> String {
        if self.nome.trim().is_empty() {
            format!("{} → {}", self.escuta(), self.destino())
        } else {
            self.nome.trim().to_owned()
        }
    }
}

/// Separa `host:porta` (ou `[::1]:porta`); `None` sem a porta ou com ela fora de 1 a 65535.
pub fn dividir_destino(texto: &str) -> Option<(String, u16)> {
    let texto = texto.trim();
    let (host, porta) = texto.rsplit_once(':')?;
    let host = host.trim().trim_start_matches('[').trim_end_matches(']');
    let porta: u16 = porta.trim().parse().ok().filter(|p| *p > 0)?;
    (!host.is_empty()).then(|| (host.to_owned(), porta))
}

/// "Nenhum", "1 túnel" ou "N túneis".
pub fn resumo_dos_tuneis(quantos: usize) -> String {
    t!("comum-tuneis-resumo", n = quantos as i64)
}

/// Uma conexão guardada. A senha fica no cofre (`cofre.rs`); aqui só o id do item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    /// Estável e nunca reaproveitado; a tela e o `estado.json` se referem à conexão por ele.
    #[serde(default)]
    pub id: u64,
    /// O nome na árvore e na aba.
    pub name: String,
    /// O tipo.
    #[serde(default)]
    pub kind: Kind,
    /// O servidor.
    #[serde(default)]
    pub host: String,
    /// A porta.
    #[serde(default)]
    pub port: u16,
    /// O usuário.
    #[serde(default)]
    pub user: String,
    /// A chave privada em arquivo (gravada com `~/` quando está na pasta pessoal).
    #[serde(default, with = "crate::armazenamento::caminho_opcional_com_til")]
    pub key_file: Option<PathBuf>,
    /// A chave privada guardada no cofre: o `uid` do item. É o `uid`, e não o id, porque ele
    /// sobrevive à junção de cópias do cofre. Vale no lugar de `key_file`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chave_do_cofre: Option<String>,
    /// O tipo de terminal que o servidor recebe (`TERM`).
    #[serde(default = "term_padrao")]
    pub term: String,
    /// O keepalive em segundos; 0 desliga.
    #[serde(default = "keepalive_padrao")]
    pub keepalive_secs: u32,
    /// O arquivo onde a sessão grava o que aparece no terminal.
    #[serde(default)]
    pub log_file: Option<PathBuf>,
    /// O servidor de salto.
    #[serde(default)]
    pub jump: Option<Jump>,
    /// O item do cofre com o usuário e a senha desta conexão. Um id que não existe mais vale
    /// como `None`: a conexão volta a perguntar.
    #[serde(default)]
    pub credencial: Option<u64>,
    /// SSH e SFTP sem item na `credencial`: perguntar a senha antes de abrir, só o agente SSH,
    /// ou o agente e a senha se o servidor recusar. Com um item, o item vence (ver
    /// [`Connection::modo_da_credencial`]).
    #[serde(default, skip_serializing_if = "ModoCredencial::e_perguntar")]
    pub credencial_modo: ModoCredencial,
    /// Um texto livre sobre a conexão, como a "Description" do WinSSHTerm: aparece na dica da
    /// linha da árvore e entra na busca.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub descricao: String,
    /// A nota do cofre que arquivos antigos ligavam a esta conexão (M47). Só serve para a
    /// migração: ao abrir o cofre, o texto da nota vira a `descricao` e o campo volta a `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nota: Option<u64>,
    /// FTP: como a conexão é protegida.
    #[serde(default)]
    pub ftp_security: FtpSeguranca,
    /// FTP: passivo ou ativo.
    #[serde(default)]
    pub ftp_mode: FtpModo,
    /// FTP: a versão do TLS.
    #[serde(default)]
    pub ftp_tls: FtpTls,
    /// FTP: a codificação dos nomes.
    #[serde(default)]
    pub ftp_encoding: FtpCodificacao,
    /// FTP: quantos minutos o relógio do servidor está a leste do UTC (só para o `LIST`).
    #[serde(default)]
    pub ftp_timezone_min: i32,
    /// FTP: a faixa de portas do modo ativo (`10000-10100`); vazia, qualquer porta.
    #[serde(default)]
    pub ftp_active_ports: String,
    /// S3: o serviço, o bucket e o resto das opções.
    #[serde(default)]
    pub s3: S3Opcoes,
    /// Rclone: o remoto e a pasta.
    #[serde(default)]
    pub rclone: RcloneOpcoes,
    /// A rede por onde a conexão sai (M34): herda do grupo, vai direta ou usa um perfil.
    #[serde(default)]
    pub rede: Rede,
    /// O que um assistente pode fazer nesta conexão pelo MCP (M51): herda da pasta.
    #[serde(default, skip_serializing_if = "PermissaoIa::is_herdar")]
    pub mcp: PermissaoIa,
    /// O assistente de IA da conexão (M52): o id de um provedor do `ia.json`; `None` usa o padrão.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perfil_ia: Option<u64>,
    /// SSH: o X11 (M37).
    #[serde(default)]
    pub x11: ModoX11,
    /// Os comandos que o app digita depois que a sessão abre (M50).
    #[serde(default, skip_serializing_if = "ComandosAoConectar::e_padrao")]
    pub comandos: ComandosAoConectar,
    /// SSH: a aba roda numa sessão do `tmux` ou do `screen` no servidor; herda da pasta.
    #[serde(default, skip_serializing_if = "SessaoPersistente::is_herdar")]
    pub persistente: SessaoPersistente,
    /// SSH: a sessão persistente em que a aba abre. Não vai para o arquivo: o app põe na cópia
    /// da conexão que abre a aba (`crate::tmux`).
    #[serde(skip)]
    pub sessao_persistente: Option<SessaoDaAba>,
    /// SSH: o que fazer ao colar imagem ou arquivo (M45); `None` segue a configuração
    /// `terminal.colar_arquivos`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colar_arquivos: Option<ColarArquivos>,
    /// SSH: a pasta do servidor para o que é colado (M45); vazia, a das Configurações.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pasta_colar: String,
    /// SSH: o shell do servidor (M45), para citar o caminho colado.
    #[serde(default, skip_serializing_if = "ShellRemoto::is_automatico")]
    pub shell_remoto: ShellRemoto,
    /// SSH: comprime a conexão (o `Compression yes` do OpenSSH, M45).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub comprimir: bool,
    /// SSH: os assistentes de IA no servidor (M46).
    #[serde(default, skip_serializing_if = "IaConexao::vazio")]
    pub ia: IaConexao,
    /// SSH: os túneis (M37).
    #[serde(default)]
    pub tuneis: Vec<Tunel>,
    /// SSH: sem shell, só os túneis (o `ssh -N`).
    #[serde(default)]
    pub so_tuneis: bool,
    /// SSH: fecha a conexão se um túnel não subir (o `ExitOnForwardFailure`).
    #[serde(default)]
    pub parar_se_tunel_falhar: bool,
    /// Pasta local onde a aba de arquivos abre ("" = a das Configurações).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pasta_local: String,
    /// Pasta remota onde a aba de arquivos abre ("" = o início do servidor).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pasta_remota: String,
    /// Ligar a comparação de pastas ao abrir a aba de arquivos.
    #[serde(default, skip_serializing_if = "Ativar::e_padrao")]
    pub comparar: Ativar,
    /// Ligar a navegação sincronizada ao abrir a aba de arquivos.
    #[serde(default, skip_serializing_if = "Ativar::e_padrao")]
    pub sincronizar: Ativar,
    /// Só no tipo Local: o programa do terminal desta conexão (vazio: o shell das
    /// Configurações).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub local_programa: String,
    /// Só no tipo Local: os argumentos do programa, numa linha (aspas juntam).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub local_argumentos: String,
    /// Só no tipo Local: a pasta onde o terminal começa (vazio: a das Configurações, e sem
    /// ela a pasta pessoal). Aceita `~`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub local_pasta: String,
    /// Só no tipo Local: as variáveis de ambiente a mais do programa. Não vai para o arquivo:
    /// só as conexões avulsas que o app monta na hora usam (o "Controlar com", M63).
    #[serde(skip)]
    pub local_ambiente: Vec<(String, String)>,
}

fn term_padrao() -> String {
    "xterm-256color".to_owned()
}

fn keepalive_padrao() -> u32 {
    30
}

impl Connection {
    /// Uma conexão nova do tipo dado, com os padrões. O `id` é preenchido pela [`Tree`].
    pub fn new(name: impl Into<String>, kind: Kind) -> Self {
        Self {
            id: 0,
            name: name.into(),
            kind,
            host: String::new(),
            port: kind.porta_padrao(),
            user: String::new(),
            key_file: None,
            chave_do_cofre: None,
            term: term_padrao(),
            keepalive_secs: keepalive_padrao(),
            log_file: None,
            jump: None,
            credencial: None,
            credencial_modo: ModoCredencial::Perguntar,
            descricao: String::new(),
            nota: None,
            ftp_security: FtpSeguranca::Explicita,
            ftp_mode: FtpModo::Passivo,
            ftp_tls: FtpTls::Auto,
            ftp_encoding: FtpCodificacao::Auto,
            ftp_timezone_min: 0,
            ftp_active_ports: String::new(),
            s3: S3Opcoes::default(),
            rclone: RcloneOpcoes::default(),
            rede: Rede::Herdar,
            mcp: PermissaoIa::Herdar,
            perfil_ia: None,
            x11: ModoX11::Padrao,
            comandos: ComandosAoConectar::default(),
            persistente: SessaoPersistente::Herdar,
            sessao_persistente: None,
            colar_arquivos: None,
            pasta_colar: String::new(),
            shell_remoto: ShellRemoto::Automatico,
            comprimir: false,
            ia: IaConexao::default(),
            tuneis: Vec::new(),
            so_tuneis: false,
            parar_se_tunel_falhar: false,
            pasta_local: String::new(),
            pasta_remota: String::new(),
            comparar: Ativar::Padrao,
            sincronizar: Ativar::Padrao,
            local_programa: String::new(),
            local_argumentos: String::new(),
            local_pasta: String::new(),
            local_ambiente: Vec::new(),
        }
    }

    /// Uma conexão nova do tipo dado com os padrões das configurações: o usuário, a porta SSH,
    /// o `TERM`, o keepalive, o FTP e o S3.
    pub fn com_config(name: impl Into<String>, kind: Kind, config: &Configuracoes) -> Self {
        let mut c = Self::new(name, kind);
        c.user.clone_from(&config.conexoes.usuario_padrao);
        if kind == Kind::Ssh {
            c.port = config.conexoes.porta_padrao;
        }
        c.term.clone_from(&config.conexoes.term_padrao);
        c.keepalive_secs = config.conexoes.keepalive_padrao_secs;
        c.ftp_security = config.ftp.seguranca;
        c.ftp_mode = config.ftp.modo;
        c.ftp_tls = config.ftp.tls;
        c.ftp_encoding = config.ftp.codificacao;
        c.s3.com_config(&config.s3);
        c
    }

    /// O modo da credencial que vale: o `credencial_modo` no SSH e no SFTP sem item do cofre;
    /// `Perguntar` nos outros tipos e com um item na `credencial` (o item vence).
    pub fn modo_da_credencial(&self) -> ModoCredencial {
        if self.kind.usa_ssh() && self.credencial.is_none() {
            self.credencial_modo
        } else {
            ModoCredencial::Perguntar
        }
    }

    /// A porta padrão desta conexão: a do tipo, ou a da segurança no FTP (990 no implícito).
    pub fn porta_padrao(&self) -> u16 {
        if self.kind == Kind::Ftp {
            self.ftp_security.porta_padrao()
        } else {
            self.kind.porta_padrao()
        }
    }

    /// Onde a conexão está, só com os campos que valem para o tipo (`campos::Campo`):
    /// `usuario@host` (ou só o host) no SSH, no SFTP, no FTP e no Telnet; o bucket e o serviço
    /// (`bucket@servico`) no S3, sem o Access Key ID; o remoto (`gdrive:Backup`) no rclone; vazio
    /// no shell local e quando falta o endereço. O rótulo da árvore e a busca usam este texto.
    pub fn servidor(&self) -> String {
        match self.kind {
            Kind::Rclone => match self.rclone.remote.is_empty() {
                true => String::new(),
                false => self.rclone.spec(),
            },
            Kind::S3 => {
                let servico = self.s3.endpoint.split("://").last().unwrap_or_default();
                match (self.s3.bucket.is_empty(), servico.is_empty()) {
                    (_, true) => String::new(),
                    (true, false) => servico.to_owned(),
                    (false, false) => format!("{}@{servico}", self.s3.bucket),
                }
            }
            tipo => {
                if !Campo::Host.vale(tipo) || self.host.is_empty() {
                    String::new()
                } else if !Campo::Usuario.vale(tipo) || self.user.is_empty() {
                    self.host.clone()
                } else {
                    format!("{}@{}", self.user, self.host)
                }
            }
        }
    }

    /// O rótulo na árvore: `nome (servidor)`, para o filtro achar também pelo servidor, e só o
    /// nome quando [`servidor`](Self::servidor) é vazio.
    pub fn label(&self) -> String {
        match self.servidor() {
            servidor if servidor.is_empty() => self.name.clone(),
            servidor => format!("{} ({servidor})", self.name),
        }
    }
}

/// Um grupo de conexões (e de outros grupos).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// Estável e nunca reaproveitado.
    #[serde(default)]
    pub id: u64,
    /// O nome na árvore.
    pub name: String,
    /// O conteúdo, na ordem em que a árvore mostra.
    #[serde(default)]
    pub children: Vec<Node>,
    /// A rede do grupo (M34): vale para o que está dentro e não escolheu a sua.
    #[serde(default)]
    pub rede: Rede,
    /// O que um assistente pode fazer nas conexões do grupo pelo MCP (M51): vale para o que está
    /// dentro e herda.
    #[serde(default, skip_serializing_if = "PermissaoIa::is_herdar")]
    pub mcp: PermissaoIa,
    /// Os comandos ao conectar do grupo (M50): valem para o que está dentro e herda.
    #[serde(default, skip_serializing_if = "ComandosAoConectar::e_padrao")]
    pub comandos: ComandosAoConectar,
    /// A sessão persistente (`tmux` ou `screen`) do grupo: vale para o que está dentro e herda.
    #[serde(default, skip_serializing_if = "SessaoPersistente::is_herdar")]
    pub persistente: SessaoPersistente,
}

/// Um item da árvore.
// Os nós vivem numa árvore pequena (dezenas de itens): pôr a `Connection` numa `Box` mudaria
// cada `Node::Connection(..)` do app e dos testes sem ganho que se note.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Node {
    /// Um grupo.
    Group(Group),
    /// Uma conexão.
    Connection(Connection),
}

impl Node {
    /// O id do item.
    pub fn id(&self) -> u64 {
        match self {
            Node::Group(g) => g.id,
            Node::Connection(c) => c.id,
        }
    }

    /// O nome do item.
    pub fn name(&self) -> &str {
        match self {
            Node::Group(g) => &g.name,
            Node::Connection(c) => &c.name,
        }
    }
}

/// A árvore inteira, como vai para o `connections.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tree {
    /// A versão do formato.
    #[serde(default = "versao_atual")]
    pub version: u32,
    /// O próximo id a distribuir; só cresce.
    #[serde(default)]
    pub next_id: u64,
    /// Os itens da raiz.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// Os perfis de rede (M34).
    #[serde(default)]
    pub redes: Vec<PerfilRede>,
}

fn versao_atual() -> u32 {
    VERSAO
}

impl Default for Tree {
    fn default() -> Self {
        Self {
            version: VERSAO,
            next_id: 1,
            nodes: Vec::new(),
            redes: Vec::new(),
        }
    }
}

impl Tree {
    pub(crate) fn novo_id(&mut self) -> u64 {
        let id = self.next_id.max(1);
        self.next_id = id + 1;
        id
    }

    /// Acrescenta a conexão no fim do grupo `parent` (ou da raiz, se `parent` for `None` ou não
    /// for um grupo) e devolve o id que ela recebeu.
    pub fn add_connection(&mut self, parent: Option<u64>, mut connection: Connection) -> u64 {
        let id = self.novo_id();
        connection.id = id;
        self.children_mut(parent).push(Node::Connection(connection));
        id
    }

    /// Acrescenta um grupo vazio no fim do grupo `parent` (ou da raiz) e devolve o id dele.
    pub fn add_group(&mut self, parent: Option<u64>, name: impl Into<String>) -> u64 {
        let id = self.novo_id();
        let group = Group {
            id,
            name: name.into(),
            children: Vec::new(),
            rede: Rede::Herdar,
            mcp: PermissaoIa::Herdar,
            comandos: ComandosAoConectar::default(),
            persistente: SessaoPersistente::Herdar,
        };
        self.children_mut(parent).push(Node::Group(group));
        id
    }

    /// Os filhos do grupo `parent`; a raiz se ele não existir ou não for um grupo.
    fn children_mut(&mut self, parent: Option<u64>) -> &mut Vec<Node> {
        let existe = parent.is_some_and(|id| matches!(self.find(id), Some(Node::Group(_))));
        if !existe {
            return &mut self.nodes;
        }
        match self.find_mut(parent.unwrap_or(0)) {
            Some(Node::Group(group)) => &mut group.children,
            _ => unreachable!("o grupo foi achado logo acima"),
        }
    }

    /// Apaga o item (e o que houver dentro dele). Devolve `false` se o id não existe.
    pub fn remove(&mut self, id: u64) -> bool {
        remove_in(&mut self.nodes, id)
    }

    /// Sobe o item uma posição entre os irmãos. No topo não faz nada e devolve `false`.
    pub fn move_up(&mut self, id: u64) -> bool {
        let Some(siblings) = siblings_of(&mut self.nodes, id) else {
            return false;
        };
        match siblings.iter().position(|n| n.id() == id) {
            Some(pos) if pos > 0 => {
                siblings.swap(pos, pos - 1);
                true
            }
            _ => false,
        }
    }

    /// Desce o item uma posição entre os irmãos. No fim não faz nada e devolve `false`.
    pub fn move_down(&mut self, id: u64) -> bool {
        let Some(siblings) = siblings_of(&mut self.nodes, id) else {
            return false;
        };
        match siblings.iter().position(|n| n.id() == id) {
            Some(pos) if pos + 1 < siblings.len() => {
                siblings.swap(pos, pos + 1);
                true
            }
            _ => false,
        }
    }

    /// Move o item para o grupo `parent` (`None` é a raiz). `position` é o índice na lista final
    /// do pai, depois de tirar o item (a convenção do `on_move` da `TreeView`). Devolve `false`,
    /// sem mudar nada, se o id não existe, se o destino não é um grupo ou se é o próprio item
    /// ou algo dentro dele.
    pub fn move_to(&mut self, id: u64, parent: Option<u64>, position: usize) -> bool {
        let Some(item) = self.find(id) else {
            return false;
        };
        if let Some(parent) = parent {
            let dentro = match item {
                Node::Group(group) => find_in(&group.children, parent).is_some(),
                Node::Connection(_) => false,
            };
            if parent == id || dentro || !matches!(self.find(parent), Some(Node::Group(_))) {
                return false;
            }
        }
        let Some(item) = take_in(&mut self.nodes, id) else {
            return false;
        };
        let lista = self.children_mut(parent);
        let position = position.min(lista.len());
        lista.insert(position, item);
        true
    }

    /// Copia a conexão `id`, com o nome `name`, para logo abaixo dela no mesmo grupo, e devolve o
    /// id da cópia (`None` se `id` não é uma conexão). A cópia leva tudo, até as credenciais.
    pub fn duplicate_connection(&mut self, id: u64, name: impl Into<String>) -> Option<u64> {
        let mut copia = self.connection(id)?.clone();
        copia.id = self.novo_id();
        copia.name = name.into();
        let nova = copia.id;
        let irmaos = siblings_of(&mut self.nodes, id)?;
        let pos = irmaos.iter().position(|n| n.id() == id)?;
        irmaos.insert(pos + 1, Node::Connection(copia));
        Some(nova)
    }

    /// Troca o nome do item. Devolve `false` se o id não existe.
    pub fn rename(&mut self, id: u64, name: impl Into<String>) -> bool {
        match self.find_mut(id) {
            Some(Node::Group(group)) => group.name = name.into(),
            Some(Node::Connection(connection)) => connection.name = name.into(),
            None => return false,
        }
        true
    }

    /// O item de id `id`, em qualquer profundidade.
    pub fn find(&self, id: u64) -> Option<&Node> {
        find_in(&self.nodes, id)
    }

    /// O item de id `id`, para mudar.
    pub fn find_mut(&mut self, id: u64) -> Option<&mut Node> {
        find_in_mut(&mut self.nodes, id)
    }

    /// A conexão de id `id` (`None` se for um grupo ou não existir).
    pub fn connection(&self, id: u64) -> Option<&Connection> {
        match self.find(id) {
            Some(Node::Connection(c)) => Some(c),
            _ => None,
        }
    }

    /// A conexão de id `id`, para mudar.
    pub fn connection_mut(&mut self, id: u64) -> Option<&mut Connection> {
        match self.find_mut(id) {
            Some(Node::Connection(c)) => Some(c),
            _ => None,
        }
    }

    /// O grupo onde uma conexão ou grupo novo deve entrar, dado o item escolhido na árvore: o
    /// próprio grupo, o grupo de uma conexão, ou `None` (a raiz).
    pub fn group_for_new(&self, selected: Option<u64>) -> Option<u64> {
        let id = selected?;
        match self.find(id)? {
            Node::Group(group) => Some(group.id),
            Node::Connection(_) => parent_in(&self.nodes, id, None).flatten(),
        }
    }

    /// Os grupos que contêm o item `id`, do mais externo ao mais interno (vazio na raiz ou se o
    /// id não existe).
    pub fn ancestors(&self, id: u64) -> Vec<u64> {
        let mut cadeia = Vec::new();
        let mut atual = id;
        while let Some(Some(pai)) = parent_in(&self.nodes, atual, None) {
            cadeia.push(pai);
            atual = pai;
        }
        cadeia.reverse();
        cadeia
    }

    /// Todas as conexões, em ordem de leitura.
    pub fn connections(&self) -> Vec<&Connection> {
        let mut out = Vec::new();
        collect(&self.nodes, &mut out);
        out
    }

    /// As conexões do grupo `id`, em qualquer profundidade e na ordem da árvore (vazio se `id`
    /// não é um grupo).
    pub fn connections_in(&self, id: u64) -> Vec<&Connection> {
        let mut out = Vec::new();
        if let Some(Node::Group(g)) = self.find(id) {
            collect(&g.children, &mut out);
        }
        out
    }

    /// Quantas conexões a árvore tem.
    pub fn connection_count(&self) -> usize {
        self.connections().len()
    }
}

fn collect<'a>(nodes: &'a [Node], out: &mut Vec<&'a Connection>) {
    for node in nodes {
        match node {
            Node::Connection(c) => out.push(c),
            Node::Group(g) => collect(&g.children, out),
        }
    }
}

fn find_in(nodes: &[Node], id: u64) -> Option<&Node> {
    for node in nodes {
        if node.id() == id {
            return Some(node);
        }
        if let Node::Group(group) = node
            && let Some(found) = find_in(&group.children, id)
        {
            return Some(found);
        }
    }
    None
}

fn find_in_mut(nodes: &mut [Node], id: u64) -> Option<&mut Node> {
    for node in nodes {
        if node.id() == id {
            return Some(node);
        }
        if let Node::Group(group) = node
            && let Some(found) = find_in_mut(&mut group.children, id)
        {
            return Some(found);
        }
    }
    None
}

/// O grupo pai do item `id`: `Some(None)` se está na raiz, `Some(Some(g))` se está no grupo `g`
/// e `None` se o id não existe.
fn parent_in(nodes: &[Node], id: u64, parent: Option<u64>) -> Option<Option<u64>> {
    for node in nodes {
        if node.id() == id {
            return Some(parent);
        }
        if let Node::Group(group) = node
            && let Some(found) = parent_in(&group.children, id, Some(group.id))
        {
            return Some(found);
        }
    }
    None
}

/// A lista que contém o item `id` (os irmãos dele, ele incluído).
fn siblings_of(nodes: &mut Vec<Node>, id: u64) -> Option<&mut Vec<Node>> {
    if nodes.iter().any(|n| n.id() == id) {
        return Some(nodes);
    }
    for node in nodes {
        if let Node::Group(group) = node
            && let Some(found) = siblings_of(&mut group.children, id)
        {
            return Some(found);
        }
    }
    None
}

fn remove_in(nodes: &mut Vec<Node>, id: u64) -> bool {
    if let Some(pos) = nodes.iter().position(|n| n.id() == id) {
        nodes.remove(pos);
        return true;
    }
    nodes.iter_mut().any(|node| match node {
        Node::Group(group) => remove_in(&mut group.children, id),
        Node::Connection(_) => false,
    })
}

/// Tira o item `id` da árvore e o devolve.
fn take_in(nodes: &mut Vec<Node>, id: u64) -> Option<Node> {
    if let Some(pos) = nodes.iter().position(|n| n.id() == id) {
        return Some(nodes.remove(pos));
    }
    nodes.iter_mut().find_map(|node| match node {
        Node::Group(group) => take_in(&mut group.children, id),
        Node::Connection(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_provedor_de_ia_e_opcional_no_json() {
        let mut c = Connection::new("web", Kind::Ssh);
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("perfil_ia"), "{json}");
        let antiga: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(antiga.perfil_ia, None);
        c.perfil_ia = Some(4);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"perfil_ia\":4"), "{json}");
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta.perfil_ia, Some(4));
    }

    #[test]
    fn o_arquivo_de_chave_vai_para_o_json_com_til_e_volta_expandido() {
        let Some(home) = crate::armazenamento::pasta_pessoal() else {
            return;
        };
        let mut c = Connection::new("web", Kind::Ssh);
        c.key_file = Some(home.join(".ssh/id_ed25519"));
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"~/.ssh/id_ed25519\""), "{json}");
        assert!(!json.contains("chave_do_cofre"), "{json}");
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta.key_file, c.key_file);

        // Um caminho antigo, absoluto e fora da pasta pessoal, continua valendo.
        c.key_file = Some("/etc/chaves/id".into());
        let volta: Connection = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(volta.key_file, c.key_file);

        c.key_file = None;
        c.chave_do_cofre = Some("0123abcd".into());
        let volta: Connection = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(volta.chave_do_cofre.as_deref(), Some("0123abcd"));
    }

    #[test]
    fn ativar_resolve_pela_conexao_ou_segue_a_global() {
        for global in [false, true] {
            assert_eq!(Ativar::Padrao.resolver(global), global);
            assert!(Ativar::Sim.resolver(global));
            assert!(!Ativar::Nao.resolver(global));
        }
    }

    #[test]
    fn ativar_vai_para_o_json_em_minusculas() {
        for (valor, texto) in [
            (Ativar::Padrao, "\"padrao\""),
            (Ativar::Sim, "\"sim\""),
            (Ativar::Nao, "\"nao\""),
        ] {
            assert_eq!(serde_json::to_string(&valor).unwrap(), texto);
            assert_eq!(serde_json::from_str::<Ativar>(texto).unwrap(), valor);
        }
    }

    #[test]
    fn json_antigo_sem_as_pastas_padrao_abre_com_os_padroes() {
        let c: Connection = serde_json::from_str(r#"{"name": "web", "kind": "Ssh"}"#).unwrap();
        assert_eq!(c.pasta_local, "");
        assert_eq!(c.pasta_remota, "");
        assert_eq!(c.comparar, Ativar::Padrao);
        assert_eq!(c.sincronizar, Ativar::Padrao);
    }

    #[test]
    fn os_padroes_de_arquivos_nao_sujam_o_json() {
        let c = Connection::new("web", Kind::Ssh);
        let json = serde_json::to_string(&c).unwrap();
        for campo in ["pasta_local", "pasta_remota", "comparar", "sincronizar"] {
            assert!(!json.contains(campo), "{campo} em {json}");
        }
    }

    #[test]
    fn as_pastas_padrao_e_os_interruptores_fazem_ida_e_volta() {
        let mut c = Connection::new("web", Kind::Ssh);
        c.pasta_local = "/home/ana/projetos".into();
        c.pasta_remota = "/var/www".into();
        c.comparar = Ativar::Sim;
        c.sincronizar = Ativar::Nao;
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"comparar\":\"sim\""), "{json}");
        assert!(json.contains("\"sincronizar\":\"nao\""), "{json}");
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta, c);
    }

    #[test]
    fn json_antigo_sem_os_campos_de_colar_abre_com_os_padroes() {
        let c: Connection = serde_json::from_str(r#"{"name": "web", "kind": "Ssh"}"#).unwrap();
        assert_eq!(c.colar_arquivos, None);
        assert_eq!(c.pasta_colar, "");
        assert_eq!(c.shell_remoto, ShellRemoto::Automatico);
        assert!(!c.comprimir);
    }

    #[test]
    fn os_padroes_de_colar_nao_sujam_o_json() {
        let c = Connection::new("web", Kind::Ssh);
        let json = serde_json::to_string(&c).unwrap();
        for campo in ["colar_arquivos", "pasta_colar", "shell_remoto", "comprimir"] {
            assert!(!json.contains(campo), "{campo} em {json}");
        }
    }

    #[test]
    fn os_campos_de_colar_fazem_ida_e_volta() {
        let mut c = Connection::new("web", Kind::Ssh);
        c.colar_arquivos = Some(ColarArquivos::CaminhoLocal);
        c.pasta_colar = "~/colados".into();
        c.shell_remoto = ShellRemoto::PowerShell;
        c.comprimir = true;
        let json = serde_json::to_string(&c).unwrap();
        assert!(
            json.contains("\"colar_arquivos\":\"caminho_local\""),
            "{json}"
        );
        assert!(json.contains("\"shell_remoto\":\"power_shell\""), "{json}");
        assert!(json.contains("\"comprimir\":true"), "{json}");
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta, c);
    }

    fn exemplo() -> Tree {
        let mut tree = Tree::default();
        let producao = tree.add_group(None, "Produção");
        let mut web = Connection::new("web-01", Kind::Ssh);
        web.host = "web-01.exemplo".into();
        web.user = "ana".into();
        tree.add_connection(Some(producao), web);
        tree.add_connection(Some(producao), Connection::new("db-01", Kind::Ssh));
        tree.add_connection(None, Connection::new("Shell", Kind::Local));
        tree
    }

    fn nomes(nodes: &[Node]) -> Vec<&str> {
        nodes.iter().map(Node::name).collect()
    }

    #[test]
    fn ids_sao_estaveis_e_nunca_reaproveitados() {
        let mut tree = exemplo();
        let ultimo = tree.next_id;
        let id = tree.add_group(None, "Novo");
        assert_eq!(id, ultimo);
        assert!(tree.remove(id));
        assert_eq!(tree.add_group(None, "Outro"), ultimo + 1);
    }

    #[test]
    fn json_de_ida_e_volta_mantem_a_arvore() {
        let tree = exemplo();
        let json = serde_json::to_string_pretty(&tree).unwrap();
        let volta: Tree = serde_json::from_str(&json).unwrap();
        assert_eq!(volta, tree);
    }

    #[test]
    fn campos_que_faltam_no_json_ganham_o_padrao() {
        let json = r#"{ "nodes": [ { "Connection": { "name": "antiga" } } ] }"#;
        let tree: Tree = serde_json::from_str(json).unwrap();
        let c = tree.connections()[0];
        assert_eq!(c.name, "antiga");
        assert_eq!(c.kind, Kind::Ssh);
        assert_eq!(c.term, "xterm-256color");
        assert_eq!(c.keepalive_secs, 30);
        assert_eq!(tree.version, VERSAO);
        // O arquivo do M24 não tem `credencial`.
        assert_eq!(c.credencial, None);
    }

    #[test]
    fn credencial_vai_e_volta_pelo_json_inclusive_no_salto() {
        let mut c = Connection::new("web", Kind::Ssh);
        c.credencial = Some(3);
        c.jump = Some(Jump {
            host: "bastiao".into(),
            port: 22,
            user: String::new(),
            credencial: Some(5),
        });
        let json = serde_json::to_string(&c).unwrap();
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta, c);
        let sem: Jump = serde_json::from_str(r#"{"host":"b","port":22,"user":""}"#).unwrap();
        assert_eq!(sem.credencial, None);
    }

    #[test]
    fn modo_da_credencial_vai_e_volta_pelo_json_e_o_padrao_nao_e_gravado() {
        for modo in [ModoCredencial::Agente, ModoCredencial::AgenteOuSenha] {
            let mut c = Connection::new("web", Kind::Ssh);
            c.credencial_modo = modo;
            let json = serde_json::to_string(&c).unwrap();
            let volta: Connection = serde_json::from_str(&json).unwrap();
            assert_eq!(volta, c);
            assert_eq!(volta.credencial_modo, modo);
        }
        let json = serde_json::to_string(&Connection::new("web", Kind::Ssh)).unwrap();
        assert!(!json.contains("credencial_modo"), "o padrão fica de fora");
        let c: Connection =
            serde_json::from_str(r#"{"name":"web","credencial_modo":"agente_ou_senha"}"#).unwrap();
        assert_eq!(c.credencial_modo, ModoCredencial::AgenteOuSenha);
    }

    #[test]
    fn arquivo_antigo_sem_o_modo_da_credencial_abre_como_perguntar() {
        let json = r#"{ "nodes": [ { "Connection": { "name": "antiga", "credencial": null } } ] }"#;
        let tree: Tree = serde_json::from_str(json).unwrap();
        assert_eq!(
            tree.connections()[0].credencial_modo,
            ModoCredencial::Perguntar
        );
    }

    #[test]
    fn o_item_do_cofre_vence_o_modo_e_so_o_ssh_usa_o_modo() {
        let mut c = Connection::new("web", Kind::Ssh);
        c.credencial_modo = ModoCredencial::Agente;
        assert_eq!(c.modo_da_credencial(), ModoCredencial::Agente);
        c.credencial = Some(4);
        assert_eq!(c.modo_da_credencial(), ModoCredencial::Perguntar);
        let mut sftp = Connection::new("arq", Kind::Sftp);
        sftp.credencial_modo = ModoCredencial::AgenteOuSenha;
        assert_eq!(sftp.modo_da_credencial(), ModoCredencial::AgenteOuSenha);
        let mut ftp = Connection::new("ftp", Kind::Ftp);
        ftp.credencial_modo = ModoCredencial::Agente;
        assert_eq!(ftp.modo_da_credencial(), ModoCredencial::Perguntar);
    }

    #[test]
    fn arquivo_sem_comandos_abre_como_herdar() {
        let json = r#"{ "nodes": [ { "Group": { "name": "G", "children": [
            { "Connection": { "name": "web" } } ] } } ] }"#;
        let tree: Tree = serde_json::from_str(json).unwrap();
        let c = tree.connections()[0];
        assert_eq!(c.comandos, ComandosAoConectar::default());
        assert_eq!(c.comandos.modo, ModoComandos::Herdar);
        assert!(c.comandos.reconectar);
        let Some(Node::Group(g)) = tree.nodes.first() else {
            panic!("sem a pasta");
        };
        assert!(g.comandos.e_padrao());
    }

    #[test]
    fn comandos_vao_e_voltam_pelo_json() {
        let mut c = Connection::new("web", Kind::Ssh);
        c.comandos = ComandosAoConectar {
            modo: ModoComandos::Proprios,
            reconectar: false,
            passos: vec![
                Passo::comando("sudo su -", 15),
                Passo::senha(OrigemDaSenha::Conexao, 20),
                Passo {
                    acao: AcaoDoPasso::Senha {
                        origem: OrigemDaSenha::Cofre { id: 7 },
                        texto: "kennwort".into(),
                    },
                    espera: Espera::TelaParar,
                    max_secs: 5,
                },
                Passo {
                    acao: AcaoDoPasso::Comando {
                        texto: "cd /srv".into(),
                    },
                    espera: Espera::Texto {
                        texto: "# |$ ".into(),
                    },
                    max_secs: 8,
                },
                Passo {
                    acao: AcaoDoPasso::Comando { texto: "ls".into() },
                    espera: Espera::Logo,
                    max_secs: 1,
                },
            ],
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("segredo"));
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta, c);
    }

    #[test]
    fn passo_sem_espera_nem_tempo_ganha_os_padroes() {
        let passo: Passo =
            serde_json::from_str(r#"{"acao":{"tipo":"comando","texto":"ls"}}"#).unwrap();
        assert_eq!(passo, Passo::comando("ls", MAX_SECS_PADRAO));
    }

    #[test]
    fn conexao_sem_comandos_nao_grava_o_campo() {
        let c = Connection::new("web", Kind::Ssh);
        assert!(!serde_json::to_string(&c).unwrap().contains("comandos"));
        let mut tree = Tree::default();
        tree.add_group(None, "G");
        assert!(!serde_json::to_string(&tree).unwrap().contains("comandos"));
    }

    #[test]
    fn versao_continua_1() {
        assert_eq!(VERSAO, 1);
    }

    #[test]
    fn descricao_vai_e_volta_pelo_json_e_arquivo_antigo_nao_tem() {
        let mut c = Connection::new("web", Kind::Ssh);
        let json = serde_json::to_string(&c).unwrap();
        assert!(
            !json.contains("descricao") && !json.contains("nota"),
            "{json}"
        );
        c.descricao = "servidor do ERP".into();
        let json = serde_json::to_string(&c).unwrap();
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta.descricao, "servidor do ERP");
        assert_eq!(volta, c);
        let antiga: Connection = serde_json::from_str(r#"{"name":"velha","nota":12}"#).unwrap();
        assert_eq!((antiga.descricao.as_str(), antiga.nota), ("", Some(12)));
    }

    #[test]
    fn mover_troca_com_o_irmao_e_para_no_limite() {
        let mut tree = exemplo();
        let Some(Node::Group(producao)) = tree.nodes.first() else {
            panic!("o primeiro item é o grupo");
        };
        let (web, db) = (producao.children[0].id(), producao.children[1].id());
        assert!(!tree.move_up(web), "no topo não sobe");
        assert!(tree.move_down(web));
        let Some(Node::Group(producao)) = tree.nodes.first() else {
            panic!();
        };
        assert_eq!(nomes(&producao.children), ["db-01", "web-01"]);
        assert!(!tree.move_down(web), "no fim não desce");
        assert!(tree.move_up(web));
        assert!(tree.move_up(db));
        assert!(!tree.move_up(9999), "id inexistente");
    }

    #[test]
    fn os_ancestrais_vao_do_grupo_mais_externo_ao_mais_interno() {
        let mut tree = Tree::default();
        let fora = tree.add_group(None, "Fora");
        let dentro = tree.add_group(Some(fora), "Dentro");
        let c = tree.add_connection(Some(dentro), Connection::new("c", Kind::Ssh));
        let solta = tree.add_connection(None, Connection::new("solta", Kind::Ssh));
        assert_eq!(tree.ancestors(c), [fora, dentro]);
        assert_eq!(tree.ancestors(dentro), [fora]);
        assert!(tree.ancestors(solta).is_empty());
        assert!(tree.ancestors(9999).is_empty());
    }

    #[test]
    fn apagar_um_grupo_leva_o_conteudo_junto() {
        let mut tree = exemplo();
        assert_eq!(tree.connection_count(), 3);
        let producao = tree.nodes[0].id();
        assert!(tree.remove(producao));
        assert_eq!(tree.connection_count(), 1);
        assert_eq!(nomes(&tree.nodes), ["Shell"]);
        assert!(!tree.remove(producao), "já foi");
    }

    #[test]
    fn conexao_nova_entra_no_grupo_escolhido_ou_na_raiz() {
        let mut tree = exemplo();
        let producao = tree.nodes[0].id();
        let web = tree.connections()[0].id;
        let shell = tree.connections()[2].id;
        assert_eq!(tree.group_for_new(Some(producao)), Some(producao));
        assert_eq!(tree.group_for_new(Some(web)), Some(producao));
        assert_eq!(tree.group_for_new(Some(shell)), None);
        assert_eq!(tree.group_for_new(None), None);
        assert_eq!(tree.group_for_new(Some(9999)), None);
        // Um pai que não é grupo cai na raiz.
        let id = tree.add_connection(Some(web), Connection::new("solta", Kind::Telnet));
        assert!(tree.nodes.iter().any(|n| n.id() == id));
    }

    #[test]
    fn rotulo_leva_usuario_e_host_para_o_filtro() {
        let tree = exemplo();
        let rotulos: Vec<String> = tree.connections().iter().map(|c| c.label()).collect();
        assert_eq!(rotulos, ["web-01 (ana@web-01.exemplo)", "db-01", "Shell"]);
        let mut c = Connection::new("x", Kind::Telnet);
        c.host = "roteador".into();
        assert_eq!(c.label(), "x (roteador)");
    }

    #[test]
    fn servidor_e_rotulo_de_cada_tipo_so_levam_o_que_vale() {
        let mut ssh = Connection::new("web", Kind::Ssh);
        ssh.host = "web.exemplo".into();
        ssh.user = "ana".into();
        assert_eq!(ssh.servidor(), "ana@web.exemplo");
        assert_eq!(ssh.label(), "web (ana@web.exemplo)");
        ssh.user.clear();
        assert_eq!(ssh.servidor(), "web.exemplo");
        let mut sem_host = Connection::new("novo", Kind::Ssh);
        sem_host.user = "ana".into();
        assert_eq!(sem_host.servidor(), "");
        assert_eq!(sem_host.label(), "novo");

        for tipo in [Kind::Sftp, Kind::Ftp, Kind::Telnet] {
            let mut c = Connection::new("x", tipo);
            c.host = "h.exemplo".into();
            c.user = "u".into();
            assert_eq!(c.label(), "x (u@h.exemplo)", "{tipo:?}");
        }

        // O shell local não usa servidor, mesmo se já foi SSH.
        let mut local = Connection::new("Shell", Kind::Local);
        local.host = "antigo.exemplo".into();
        local.user = "ana".into();
        assert_eq!(local.servidor(), "");
        assert_eq!(local.label(), "Shell");

        // O S3 mostra o serviço e o bucket, nunca o Access Key ID.
        let mut s3 = Connection::new("nuvem", Kind::S3);
        s3.user = "AKIAEXEMPLO".into();
        s3.s3.endpoint.clear();
        assert_eq!(s3.servidor(), "");
        s3.s3.endpoint = "https://minio.exemplo:9000".into();
        assert_eq!(s3.servidor(), "minio.exemplo:9000");
        s3.s3.bucket = "bkt".into();
        assert_eq!(s3.servidor(), "bkt@minio.exemplo:9000");
        assert_eq!(s3.label(), "nuvem (bkt@minio.exemplo:9000)");
        assert!(!s3.label().contains("AKIA"));

        let mut rclone = Connection::new("drive", Kind::Rclone);
        assert_eq!(rclone.servidor(), "");
        rclone.rclone.remote = "gdrive".into();
        rclone.rclone.path = "Backup".into();
        assert_eq!(rclone.servidor(), "gdrive:Backup");
        assert_eq!(rclone.label(), "drive (gdrive:Backup)");
    }

    #[test]
    fn portas_padrao_por_tipo() {
        assert_eq!(Connection::new("a", Kind::Ssh).port, 22);
        assert_eq!(Connection::new("a", Kind::Telnet).port, 23);
        assert_eq!(Connection::new("a", Kind::Local).port, 0);
    }

    #[test]
    fn mover_para_outro_grupo_e_posicao() {
        let mut tree = exemplo();
        let producao = tree.nodes[0].id();
        let shell = tree.connections()[2].id;
        assert!(tree.move_to(shell, Some(producao), 0));
        let Node::Group(g) = &tree.nodes[0] else {
            panic!()
        };
        assert_eq!(nomes(&g.children), ["Shell", "web-01", "db-01"]);
        assert!(
            tree.move_to(shell, None, 5),
            "a posição é limitada ao tamanho"
        );
        assert_eq!(nomes(&tree.nodes), ["Produção", "Shell"]);
    }

    #[test]
    fn duplicar_poe_a_copia_logo_abaixo_com_id_novo() {
        let mut tree = exemplo();
        let producao = tree.nodes[0].id();
        let web = tree.connections()[0].id;
        let copia = tree.duplicate_connection(web, "web-01 (cópia)").unwrap();
        assert_ne!(copia, web);
        let Node::Group(g) = &tree.nodes[0] else {
            panic!()
        };
        assert_eq!(nomes(&g.children), ["web-01", "web-01 (cópia)", "db-01"]);
        let (a, b) = (
            tree.connection(web).unwrap(),
            tree.connection(copia).unwrap(),
        );
        assert_eq!(
            (a.host.as_str(), a.user.as_str()),
            (b.host.as_str(), b.user.as_str())
        );
        assert_eq!(a.credencial, b.credencial);
        assert!(tree.duplicate_connection(producao, "x").is_none(), "pasta");
        assert!(tree.duplicate_connection(9999, "x").is_none());
    }

    #[test]
    fn mover_recusa_destino_invalido() {
        let mut tree = exemplo();
        let antes = tree.clone();
        let producao = tree.nodes[0].id();
        let web = tree.connections()[0].id;
        assert!(
            !tree.move_to(producao, Some(producao), 0),
            "para dentro de si"
        );
        assert!(!tree.move_to(web, Some(web), 0), "destino que não é grupo");
        assert!(!tree.move_to(9999, None, 0), "id que não existe");
        let sub = tree.add_group(Some(producao), "Sub");
        assert!(
            !tree.move_to(producao, Some(sub), 0),
            "para dentro de um descendente"
        );
        assert!(tree.remove(sub));
        assert_eq!(tree.nodes, antes.nodes);
    }

    #[test]
    fn renomear_conexao_e_grupo() {
        let mut tree = exemplo();
        let producao = tree.nodes[0].id();
        let web = tree.connections()[0].id;
        assert!(tree.rename(producao, "Prod"));
        assert!(tree.rename(web, "web"));
        assert!(!tree.rename(9999, "x"));
        assert_eq!(tree.nodes[0].name(), "Prod");
        assert_eq!(tree.connection(web).unwrap().name, "web");
    }
}
