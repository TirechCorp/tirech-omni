//! As portas do servidor (M58): o que escuta numa conexão SSH, com nome, alcance, processo e
//! título da página, e o que fazer com cada porta (encaminhar, abrir no navegador, fixar).
//!
//! Tudo aqui é puro (sem tela e sem rede) e testado em unidade: os leitores do `ss`, do
//! `/proc/net/tcp`, do `netstat` e do `docker`, a sonda que só olha bytes, o catálogo, a escolha
//! da porta local e a reescrita do relé HTTP. Quem fala com o servidor é `descobrir.rs`.

pub mod catalogo;
pub mod descobrir;
pub mod http;
pub mod ler;
pub mod local;
pub mod rele;
pub mod sonda;
pub mod url;

use std::net::IpAddr;

pub use catalogo::Tipo;

/// Um programa que escuta uma porta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Processo {
    pub pid: Option<u32>,
    pub nome: String,
}

/// Uma porta TCP em LISTEN, como o servidor a conta (ainda sem juntar nem dar nome).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Escuta {
    pub endereco: IpAddr,
    pub porta: u16,
    pub uid: Option<u32>,
    pub inode: Option<u64>,
    pub processo: Option<Processo>,
}

/// Quem consegue chegar à porta, do ponto de vista do servidor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Alcance {
    /// Escuta só no loopback (`127.0.0.0/8` e `::1`).
    SoNoServidor,
    /// Escuta em `0.0.0.0` ou `::`.
    TodaARede,
    /// Escuta num IP específico.
    So(IpAddr),
}

/// Um caminho de um app conhecido nesta porta ("Abrir phpMyAdmin" → `/phpmyadmin/`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Atalho {
    pub nome: String,
    pub caminho: String,
}

/// O que há do outro lado da porta, pelo catálogo e pela sonda.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Servico {
    Web {
        https: bool,
        titulo: Option<String>,
        servidor: Option<String>,
        codigo: u16,
    },
    Conhecido {
        nome: String,
        tipo: Tipo,
        versao: Option<String>,
    },
    Desconhecido,
}

/// Uma porta do servidor, já juntada: endereços, alcance, destino do túnel, dono e serviço.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortaDoServidor {
    pub porta: u16,
    pub enderecos: Vec<IpAddr>,
    pub alcance: Alcance,
    /// O host do túnel (seção 3.3 do plano): para onde o servidor abre a conexão.
    pub destino: String,
    pub processo: Option<Processo>,
    pub usuario: Option<String>,
    pub conteiner: Option<String>,
    pub servico: Servico,
    pub atalhos: Vec<Atalho>,
    pub inode: Option<u64>,
}

impl PortaDoServidor {
    /// Se a porta é do tipo `Sistema` do catálogo (a 22 do próprio sshd, o `127.0.0.53:53`).
    #[must_use]
    pub fn de_sistema(&self) -> bool {
        matches!(
            &self.servico,
            Servico::Conhecido {
                tipo: Tipo::Sistema,
                ..
            }
        )
    }

    /// Se a conversa com a porta é HTTP (a sonda achou, ou o catálogo diz que é web).
    #[must_use]
    pub fn e_web(&self) -> bool {
        match &self.servico {
            Servico::Web { .. } => true,
            Servico::Conhecido { tipo, .. } => {
                matches!(tipo, Tipo::Web | Tipo::WebHttps | Tipo::Painel)
            }
            Servico::Desconhecido => false,
        }
    }

    /// Se o servidor fala TLS nesta porta.
    #[must_use]
    pub fn e_https(&self) -> bool {
        match &self.servico {
            Servico::Web { https, .. } => *https,
            Servico::Conhecido { tipo, .. } => *tipo == Tipo::WebHttps,
            Servico::Desconhecido => false,
        }
    }
}

/// Um contêiner Docker ou Podman em execução.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conteiner {
    pub nome: String,
    pub redes: Vec<(String, IpAddr)>,
    /// (porta do contêiner, porta publicada no servidor).
    pub portas: Vec<(u16, Option<u16>)>,
    /// Se o servidor chega ao IP do contêiner (Docker com root e rede bridge).
    pub alcancavel: bool,
}

impl Conteiner {
    /// O IP em que o servidor alcança o contêiner, o da primeira rede.
    #[must_use]
    pub fn ip(&self) -> Option<IpAddr> {
        self.redes.first().map(|(_, ip)| *ip)
    }
}

/// Como a lista foi lida.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fonte {
    Ss,
    Proc,
    Netstat,
    Nenhuma,
}

/// O resultado de uma leitura das portas do servidor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descoberta {
    pub portas: Vec<PortaDoServidor>,
    pub conteineres: Vec<Conteiner>,
    pub fonte: Fonte,
    /// Avisos para a pessoa ("Sem acesso ao Docker…").
    pub avisos: Vec<String>,
}

impl Descoberta {
    /// Uma leitura sem nada: a fonte que falhou e o aviso.
    #[must_use]
    pub fn vazia(fonte: Fonte, avisos: Vec<String>) -> Self {
        Self {
            portas: Vec::new(),
            conteineres: Vec::new(),
            fonte,
            avisos,
        }
    }
}
