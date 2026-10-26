//! O que o painel de portas faz com uma porta (M58): encaminhar, abrir no navegador, copiar,
//! parar, fixar na conexão e o diálogo "Encaminhar destino…".
//!
//! Os túneis moram na sessão (`SshForwards`): o painel não guarda tabela própria. "Já está
//! encaminhada" é "há um túnel local para o mesmo destino que ainda não parou".

use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use tomni_ui::clipboard;
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::{Forward, ForwardId, ForwardState, ForwardStatus};

use super::Tela;
use super::reles::AlvoDoRele;
use crate::links::DestinoDoLink;
use crate::modelo::{TipoTunel, Tunel};
use crate::portas::catalogo::{self, Tipo};
use crate::portas::local::{escolher_porta_local, porta_livre};
use crate::portas::url::{endereco_local, url_local};
use crate::portas::{Atalho, Conteiner, PortaDoServidor, Servico};
use crate::t;

/// Quantas vezes o painel escolhe outra porta se a primeira foi tomada entre a conferência e o
/// `bind` (outro programa pegou no meio).
const TENTATIVAS: u8 = 3;

/// A espera pelo túnel, em passos de 50 ms: 20 s, o tempo de uma conexão lenta terminar de abrir.
const PASSOS_DE_ESPERA: u32 = 400;

/// O destino de uma porta: o que o menu e o botão principal encaminham.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Alvo {
    /// O host visto do servidor (`127.0.0.1`, o IP do contêiner).
    pub destino: String,
    pub porta: u16,
    pub nome: String,
    pub web: bool,
    pub https: bool,
    /// `mysql`, `postgresql`…: o esquema do "Copiar como".
    pub esquema: Option<&'static str>,
    pub atalhos: Vec<Atalho>,
    pub inode: Option<u64>,
    /// Se falta saber de quem é a porta (a leitura veio sem o `ss`).
    pub sem_processo: bool,
}

impl Alvo {
    pub(super) fn da_porta(porta: &PortaDoServidor, nome: String) -> Self {
        let esquema = match (&porta.servico, porta.porta) {
            (
                Servico::Conhecido {
                    tipo: Tipo::Banco, ..
                },
                n,
            ) => match n {
                3306 => Some("mysql"),
                5432 => Some("postgresql"),
                6379 => Some("redis"),
                27017 => Some("mongodb"),
                1433 => Some("sqlserver"),
                _ => None,
            },
            _ => None,
        };
        Self {
            destino: porta.destino.clone(),
            porta: porta.porta,
            nome,
            web: porta.e_web(),
            https: porta.e_https(),
            esquema,
            atalhos: porta.atalhos.clone(),
            inode: porta.inode,
            sem_processo: porta.processo.is_none(),
        }
    }

    /// Uma porta de contêiner: a publicada vai pelo loopback do servidor; a interna, pelo IP
    /// do contêiner (seção 3.6 do plano).
    pub(super) fn de_conteiner(conteiner: &Conteiner, dentro: u16, fora: Option<u16>) -> Self {
        let (destino, porta) = match (fora, conteiner.ip()) {
            (Some(fora), _) => ("127.0.0.1".to_owned(), fora),
            (None, Some(ip)) => (ip.to_string(), dentro),
            (None, None) => ("127.0.0.1".to_owned(), dentro),
        };
        let item = catalogo::porta(dentro);
        let tipo = item.map(|i| i.tipo);
        let esquema = match (tipo, dentro) {
            (Some(Tipo::Banco), 3306) => Some("mysql"),
            (Some(Tipo::Banco), 5432) => Some("postgresql"),
            (Some(Tipo::Banco), 6379) => Some("redis"),
            (Some(Tipo::Banco), 27017) => Some("mongodb"),
            (Some(Tipo::Banco), 1433) => Some("sqlserver"),
            _ => None,
        };
        Self {
            destino,
            porta,
            nome: format!("{} :{dentro}", conteiner.nome),
            web: matches!(tipo, Some(Tipo::Web | Tipo::WebHttps | Tipo::Painel)),
            https: tipo == Some(Tipo::WebHttps),
            esquema,
            atalhos: Vec::new(),
            inode: None,
            sem_processo: false,
        }
    }

    /// O caminho que "Abrir" usa: o do primeiro app que a sonda achou, ou a raiz.
    fn caminho_inicial(&self) -> String {
        self.atalhos
            .first()
            .map_or_else(|| "/".to_owned(), |a| a.caminho.clone())
    }
}

/// O que fazer quando o túnel estiver escutando.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Depois {
    /// Só avisa onde ficou.
    Nada,
    /// Abre `http://localhost:{p}{caminho}` no navegador.
    Abrir,
    CopiarEndereco,
    CopiarUrl,
    /// `mysql://127.0.0.1:{p}`.
    CopiarEsquema(&'static str),
}

/// Um pedido de encaminhamento.
#[derive(Clone, Debug)]
pub(super) struct Pedido {
    pub destino: String,
    pub porta: u16,
    pub nome: String,
    /// Se o servidor fala TLS nesta porta.
    pub https: bool,
    pub caminho: String,
    pub host_http: String,
    /// A porta daqui que a pessoa pediu; `None`, a automática (seção 3.4 do plano).
    pub local: Option<u16>,
    pub depois: Depois,
    pub fixar: bool,
    tentativas: u8,
}

impl Pedido {
    /// Se o pedido passa pelo relé do app (seção 3.8 do plano): com nome de site, ou quando o
    /// servidor fala TLS e o que se quer é o endereço do navegador (ou fixar o túnel).
    pub(super) fn usa_rele(&self) -> bool {
        !self.host_http.trim().is_empty()
            || (self.https
                && (self.fixar || matches!(self.depois, Depois::Abrir | Depois::CopiarUrl)))
    }

    /// O pedido de `encaminhar_porta` do MCP. O assistente quer a URL daqui, então um servidor
    /// HTTPS passa pelo relé, como no "Copiar URL" do painel.
    pub(super) fn do_mcp(
        destino: String,
        porta: u16,
        https: bool,
        host_http: String,
        local: Option<u16>,
        fixar: bool,
    ) -> Self {
        Self {
            nome: format!("{destino}:{porta}"),
            destino,
            porta,
            https,
            caminho: "/".to_owned(),
            host_http,
            local,
            depois: Depois::CopiarUrl,
            fixar,
            tentativas: 0,
        }
    }

    pub(super) fn novo(alvo: &Alvo, depois: Depois, caminho: Option<String>, fixar: bool) -> Self {
        Self {
            destino: alvo.destino.clone(),
            porta: alvo.porta,
            nome: alvo.nome.clone(),
            https: alvo.https,
            caminho: caminho.unwrap_or_else(|| alvo.caminho_inicial()),
            host_http: String::new(),
            local: None,
            depois,
            fixar,
            tentativas: 0,
        }
    }
}

/// O estado do diálogo "Encaminhar destino…".
pub(super) struct EstadoDialogo {
    aberto: Signal<bool>,
    host: Signal<String>,
    porta: Signal<f64>,
    local: Signal<f64>,
    caminho: Signal<String>,
    /// "Nome do site" (o `Host` que o servidor recebe) e "HTTPS", nas "Opções da web".
    site: Signal<String>,
    https: Signal<bool>,
    opcoes: Signal<bool>,
    abrir: Signal<bool>,
    fixar: Signal<bool>,
    erro: Signal<String>,
    testando: Signal<bool>,
    /// Depois de um erro de teste, o botão "Encaminhar mesmo assim" aparece.
    mesmo: Signal<bool>,
}

impl EstadoDialogo {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            host: signal(String::new()),
            porta: signal(0.0),
            local: signal(0.0),
            caminho: signal(String::new()),
            site: signal(String::new()),
            https: signal(false),
            opcoes: signal(false),
            abrir: signal(false),
            fixar: signal(false),
            erro: signal(String::new()),
            testando: signal(false),
            mesmo: signal(false),
        }
    }

    /// A porta do campo como `u16` (`None` fora de 1 a 65535).
    fn porta_do_destino(&self) -> Option<u16> {
        porta_de(self.porta.get())
    }

    /// A porta daqui: `Ok(None)` é a automática; `Err` é um valor que não cabe.
    fn porta_daqui(&self) -> Result<Option<u16>, ()> {
        let valor = self.local.get();
        if valor < 0.5 {
            return Ok(None);
        }
        porta_de(valor).map(Some).ok_or(())
    }

    fn valido(&self) -> bool {
        !self.host.with(|h| h.trim().is_empty())
            && self.porta_do_destino().is_some()
            && self.porta_daqui().is_ok()
    }
}

fn porta_de(valor: f64) -> Option<u16> {
    let inteiro = valor.round();
    ((1.0..=65535.0).contains(&inteiro)).then_some(inteiro as u16)
}

/// Se o túnel é o `-L` para `destino:porta` e ainda não parou.
pub(super) fn casa(status: &ForwardStatus, destino: &str, porta: u16) -> bool {
    matches!(&status.forward, Forward::Local { host, port, .. } if host == destino && *port == porta)
        && matches!(
            status.state,
            ForwardState::Starting | ForwardState::Listening
        )
}

/// A porta daqui onde o túnel escuta (a que o sistema escolheu, ou a pedida).
pub(super) fn porta_local_de(status: &ForwardStatus) -> Option<u16> {
    status
        .listening
        .as_deref()
        .and_then(|escuta| escuta.rsplit(':').next())
        .and_then(|porta| porta.parse().ok())
        .or(match &status.forward {
            Forward::Local { bind, .. } if bind.port != 0 => Some(bind.port),
            _ => None,
        })
}

/// A linha da seção "Encaminhadas".
#[derive(Clone)]
pub(super) struct LinhaEncaminhada {
    pub id: String,
    pub chave: String,
    pub local: u16,
    pub destino: String,
    pub porta: u16,
    pub estado: String,
    pub web: bool,
    pub https: bool,
    pub caminho: String,
    pub fixada: bool,
    /// O nome de site do relé (vazio nos túneis `-L`).
    pub host_http: String,
}

impl Tela {
    /// A sessão que o painel mostra.
    pub(super) fn sessao_do_painel(&self) -> Option<Session> {
        let aba = self.portas.aba.get_untracked();
        self.portas.sessoes.borrow().get(&aba).map(|d| d.sessao)
    }

    /// O túnel que já leva a `destino:porta`, se houver. Não é reativo: quem chama lê o
    /// `relogio` e a `versao` do painel antes.
    pub(super) fn encaminhada(&self, destino: &str, porta: u16) -> Option<ForwardStatus> {
        let sessao = self.sessao_do_painel()?;
        sessao
            .ssh_forwards()?
            .list()
            .into_iter()
            .find(|s| casa(s, destino, porta))
    }

    /// A porta daqui de `destino:porta`, quando já escuta (pelo `-L` ou pelo relé).
    pub(super) fn porta_local_de(&self, destino: &str, porta: u16) -> Option<u16> {
        self.encaminhada(destino, porta)
            .and_then(|s| porta_local_de(&s))
            .or_else(|| {
                let sessao = self.sessao_do_painel()?;
                self.rele_de(sessao, destino, porta)
            })
    }

    /// Se há túnel (`-L` ou relé) para `destino:porta`.
    pub(super) fn ligada(&self, destino: &str, porta: u16) -> bool {
        self.porta_local_de(destino, porta).is_some()
    }

    /// O Ctrl+clique num `http://localhost:N/` do terminal da aba `sessao`: encaminha a porta
    /// do servidor (ou reaproveita o túnel) e abre o endereço daqui.
    pub(in crate::tela) fn abrir_porta_do_servidor(
        self: &Rc<Self>,
        sessao: Session,
        host: &str,
        porta: u16,
        caminho: &str,
        https: bool,
    ) {
        let pedido = Pedido {
            destino: host.to_owned(),
            porta,
            nome: format!("{host}:{porta}"),
            https,
            caminho: caminho.to_owned(),
            host_http: String::new(),
            local: None,
            depois: Depois::Abrir,
            fixar: false,
            tentativas: 0,
        };
        self.encaminhar_porta(sessao, pedido);
    }

    /// Encaminha (ou reaproveita o túnel) e faz o `depois` quando escutar.
    pub(super) fn encaminhar_porta(self: &Rc<Self>, sessao: Session, mut pedido: Pedido) {
        let Some(forwards) = sessao.ssh_forwards() else {
            self.avisar(InfoBarSeverity::Warning, t!("portas-sem-ssh"));
            return;
        };
        let existente = forwards
            .list()
            .into_iter()
            .find(|s| casa(s, &pedido.destino, pedido.porta));
        let rele_pronto = || {
            self.rele_de(sessao, &pedido.destino, pedido.porta)
                .is_some()
        };
        if pedido.usa_rele() || (existente.is_none() && rele_pronto()) {
            self.encaminhar_por_rele(sessao, pedido);
            return;
        }
        if let Some(existente) = existente {
            self.esperar_tunel(sessao, existente.id, pedido, 0);
            return;
        }
        let local = match pedido.local {
            Some(porta) if porta_livre(porta) => porta,
            Some(porta) => {
                self.avisar(
                    InfoBarSeverity::Warning,
                    t!("portas-local-ocupada", porta = porta),
                );
                return;
            }
            None => match escolher_porta_local(pedido.porta, porta_livre) {
                Some(porta) => porta,
                None => {
                    self.avisar(InfoBarSeverity::Warning, t!("portas-sem-porta-livre"));
                    return;
                }
            },
        };
        let id = forwards.add(Forward::local(local, pedido.destino.clone(), pedido.porta));
        pedido.tentativas = pedido.tentativas.max(1);
        self.esperar_tunel(sessao, id, pedido, 0);
    }

    /// Encaminha pelo relé do app (nome de site ou TLS): reaproveita o que já atende o pedido e,
    /// sem ele, abre outro. O relé escuta na hora; não há o que esperar.
    fn encaminhar_por_rele(self: &Rc<Self>, sessao: Session, mut pedido: Pedido) {
        if let Some((local, host_http, https)) = self.rele_para(
            sessao,
            &pedido.destino,
            pedido.porta,
            &pedido.host_http,
            pedido.https,
        ) {
            pedido.host_http = host_http;
            pedido.https = https;
            self.concluir_encaminhamento(sessao, local, &pedido);
            return;
        }
        let alvo = AlvoDoRele {
            destino: &pedido.destino,
            porta: pedido.porta,
            host_http: &pedido.host_http,
            https: pedido.https,
        };
        match self.abrir_rele(sessao, alvo, pedido.local, pedido.porta) {
            Ok(local) => self.concluir_encaminhamento(sessao, local, &pedido),
            Err(erro) => self.avisar(InfoBarSeverity::Warning, erro),
        }
    }

    /// Confere o túnel a cada 50 ms, sem bloquear a tela, até ele escutar ou falhar.
    fn esperar_tunel(self: &Rc<Self>, sessao: Session, id: ForwardId, pedido: Pedido, passo: u32) {
        let t = Rc::clone(self);
        self.alarmes.depois(Duration::from_millis(50), move || {
            let Some(forwards) = sessao.ssh_forwards() else {
                return;
            };
            let Some(status) = forwards.list().into_iter().find(|s| s.id == id) else {
                return;
            };
            match status.state.clone() {
                ForwardState::Listening => match porta_local_de(&status) {
                    Some(local) => t.concluir_encaminhamento(sessao, local, &pedido),
                    None => t.avisar(InfoBarSeverity::Warning, t!("portas-parou")),
                },
                ForwardState::Failed(motivo) => {
                    t.falhou_o_tunel(sessao, id, &status, motivo, pedido);
                }
                ForwardState::Starting if passo < PASSOS_DE_ESPERA => {
                    t.esperar_tunel(sessao, id, pedido, passo + 1);
                }
                ForwardState::Starting => t.avisar(InfoBarSeverity::Warning, t!("portas-demorou")),
                ForwardState::Stopped => t.avisar(InfoBarSeverity::Warning, t!("portas-parou")),
            }
        });
    }

    /// O túnel não subiu. Se foi porque outro programa tomou a porta que o app tinha achado
    /// livre, tenta de novo com a seguinte; do contrário, mostra o motivo da lib.
    fn falhou_o_tunel(
        self: &Rc<Self>,
        sessao: Session,
        id: ForwardId,
        status: &ForwardStatus,
        motivo: String,
        pedido: Pedido,
    ) {
        let tentada = match &status.forward {
            Forward::Local { bind, .. } => bind.port,
            _ => 0,
        };
        let tomada = tentada != 0 && !porta_livre(tentada);
        if pedido.local.is_none() && tomada && pedido.tentativas < TENTATIVAS {
            if let Some(forwards) = sessao.ssh_forwards() {
                forwards.stop(id);
            }
            let mut outro = pedido;
            outro.tentativas += 1;
            self.encaminhar_porta(sessao, outro);
            return;
        }
        self.avisar(InfoBarSeverity::Warning, motivo);
    }

    /// O túnel escuta em `local`: fixa, se pedido, e faz o que a pessoa quis.
    fn concluir_encaminhamento(&self, sessao: Session, local: u16, pedido: &Pedido) {
        if pedido.fixar {
            self.fixar_tunel(sessao, local, pedido);
        }
        let url = || url_local(local, &pedido.caminho, &pedido.host_http);
        match &pedido.depois {
            Depois::Nada => self.avisar(
                InfoBarSeverity::Success,
                t!(
                    "portas-encaminhada",
                    local = local,
                    destino = pedido.destino.as_str(),
                    porta = pedido.porta
                ),
            ),
            Depois::Abrir => {
                if let Err(erro) = self.opener.abrir_link(&DestinoDoLink::Navegador(url())) {
                    self.avisar(
                        InfoBarSeverity::Warning,
                        t!("links-nao-abriu", erro = erro.to_string()),
                    );
                }
            }
            Depois::CopiarEndereco => self.copiar_da_porta(endereco_local(local)),
            Depois::CopiarUrl => self.copiar_da_porta(url()),
            Depois::CopiarEsquema(esquema) => {
                self.copiar_da_porta(format!("{esquema}://{}", endereco_local(local)));
            }
        }
        self.portas.versao.update(|v| *v += 1);
    }

    fn copiar_da_porta(&self, texto: String) {
        clipboard::write(texto.clone());
        self.avisar(
            InfoBarSeverity::Informational,
            t!("portas-copiado", texto = texto),
        );
    }

    /// Para o túnel de `destino:porta`.
    pub(super) fn parar_porta(&self, destino: &str, porta: u16) {
        let Some(sessao) = self.sessao_do_painel() else {
            return;
        };
        if let Some(forwards) = sessao.ssh_forwards() {
            for status in forwards.list().iter().filter(|s| casa(s, destino, porta)) {
                forwards.stop(status.id);
            }
        }
        self.parar_reles(sessao, destino, porta);
        self.portas.versao.update(|v| *v += 1);
    }

    /// Se a conexão da aba já guarda o túnel para `destino:porta`. Lê a árvore: serve em vista.
    pub(super) fn esta_fixada(&self, destino: &str, porta: u16) -> bool {
        let Some(id) = self
            .sessao_do_painel()
            .and_then(|s| self.conexao_da_sessao(s))
        else {
            return false;
        };
        self.tree.with(|t| {
            t.connection(id)
                .is_some_and(|c| c.tuneis.iter().any(|x| tunel_casa(x, destino, porta)))
        })
    }

    /// Grava o túnel na conexão da aba (seção 3.9 do plano): vale nas próximas conexões, na
    /// porta de sempre ou na seguinte livre.
    pub(super) fn fixar_tunel(&self, sessao: Session, local: u16, pedido: &Pedido) {
        let Some(id) = self.conexao_da_sessao(sessao) else {
            return;
        };
        let ja = self.tree.with_untracked(|t| {
            t.connection(id).is_some_and(|c| {
                c.tuneis.iter().any(|x| {
                    tunel_casa(x, &pedido.destino, pedido.porta) && x.host_http == pedido.host_http
                })
            })
        });
        if ja {
            return;
        }
        let mut tunel = Tunel::novo(TipoTunel::Local, false);
        tunel.porta = local;
        tunel.destino_host = pedido.destino.clone();
        tunel.destino_porta = pedido.porta;
        tunel.nome = pedido.nome.clone();
        tunel.proxima_livre = true;
        tunel.caminho_web = if pedido.caminho == "/" {
            String::new()
        } else {
            pedido.caminho.clone()
        };
        tunel.host_http = pedido.host_http.clone();
        tunel.https = pedido.https && pedido.usa_rele();
        self.guardar_tunel_na_conexao(id, move |tuneis| tuneis.push(tunel));
    }

    /// Tira da conexão da aba o túnel fixado para `destino:porta`.
    pub(super) fn desafixar(&self, destino: &str, porta: u16) {
        let Some(id) = self
            .sessao_do_painel()
            .and_then(|s| self.conexao_da_sessao(s))
        else {
            return;
        };
        let host_http = self
            .sessao_do_painel()
            .and_then(|s| self.rele_para(s, destino, porta, "", false))
            .map(|(_, host_http, _)| host_http)
            .unwrap_or_default();
        let destino = destino.to_owned();
        self.guardar_tunel_na_conexao(id, move |tuneis| {
            tuneis.retain(|x| !(tunel_casa(x, &destino, porta) && x.host_http == host_http));
        });
    }

    /// Mexe nos túneis da conexão `id` e grava (o mesmo caminho do editor de túneis).
    fn guardar_tunel_na_conexao(&self, id: u64, mexer: impl FnOnce(&mut Vec<Tunel>)) {
        let mexeu = self
            .tree
            .try_update(|t| t.connection_mut(id).map(|c| mexer(&mut c.tuneis)).is_some());
        if mexeu == Some(true) {
            self.atualizar();
            if self.id_selecionado() == Some(id) {
                self.carregar_propriedades();
            }
            self.portas.versao.update(|v| *v += 1);
        }
    }

    /// Pede a leitura do dono da porta (sem o `ss`, só o `/proc` mostra o processo).
    pub(super) fn descobrir_processo(&self, inode: u64) {
        let aba = self.portas.aba.get_untracked();
        if let Some(d) = self.portas.sessoes.borrow().get(&aba) {
            d.trabalhador.processo(inode);
        }
    }

    /// Os túneis `-L` da aba que ainda não pararam, para a seção "Encaminhadas". Lê o
    /// `relogio` e a `versao`: renova a cada segundo.
    pub(super) fn linhas_encaminhadas(&self) -> Vec<LinhaEncaminhada> {
        let _ = self.portas.relogio.get();
        let _ = self.portas.versao.get();
        let Some(sessao) = self.sessao_do_painel() else {
            return Vec::new();
        };
        let Some(forwards) = sessao.ssh_forwards() else {
            return Vec::new();
        };
        let mut linhas = Vec::new();
        for status in forwards.list() {
            let Forward::Local { host, port, .. } = &status.forward else {
                continue;
            };
            let estado = match &status.state {
                ForwardState::Listening if status.active > 0 => {
                    t!("portas-enc-conexoes", n = status.active as i64)
                }
                ForwardState::Listening => t!("portas-enc-escutando"),
                ForwardState::Starting => t!("portas-enc-subindo"),
                ForwardState::Failed(motivo) => motivo.clone(),
                ForwardState::Stopped => continue,
            };
            let local = porta_local_de(&status).unwrap_or(0);
            let (web, https, caminho) = self.o_que_ha_em(host, *port);
            linhas.push(LinhaEncaminhada {
                id: format!("portas-enc-{}", status.id.get()),
                chave: format!("{}|{local}|{host}|{port}|{estado}", status.id.get()),
                local,
                destino: host.clone(),
                porta: *port,
                estado,
                web,
                https,
                caminho,
                fixada: self.esta_fixada(host, *port),
                host_http: String::new(),
            });
        }
        for r in self.reles_da_sessao(sessao) {
            let base = if r.estado.ativas > 0 {
                t!("portas-enc-conexoes", n = r.estado.ativas as i64)
            } else {
                t!("portas-enc-escutando")
            };
            let estado = match (&r.estado.erro, &r.estado.certificado) {
                (Some(erro), _) => erro.clone(),
                (None, Some(motivo)) => {
                    format!(
                        "{base} · {}",
                        t!("portas-enc-certificado", motivo = motivo.as_str())
                    )
                }
                (None, None) => base,
            };
            let (web, https, caminho) = self.o_que_ha_em(&r.destino, r.porta);
            linhas.push(LinhaEncaminhada {
                id: format!("portas-enc-r{}", r.local),
                chave: format!("r{}|{}|{estado}", r.id, r.local),
                local: r.local,
                fixada: self.esta_fixada(&r.destino, r.porta),
                destino: r.destino,
                porta: r.porta,
                estado,
                web: web || r.https || !r.host_http.is_empty(),
                https: https || r.https,
                caminho,
                host_http: r.host_http,
            });
        }
        linhas
    }

    /// Se `destino:porta` é web, se fala TLS e qual o caminho do primeiro app, pelo que o
    /// painel leu do servidor (o catálogo, se a leitura não tem a porta).
    fn o_que_ha_em(&self, destino: &str, porta: u16) -> (bool, bool, String) {
        let aba = self.portas.aba.get_untracked();
        let da_lista = self.portas.sessoes.borrow().get(&aba).and_then(|d| {
            d.descoberta.as_ref().and_then(|l| {
                l.portas
                    .iter()
                    .find(|p| p.destino == destino && p.porta == porta)
                    .map(|p| {
                        let alvo = Alvo::da_porta(p, String::new());
                        (alvo.web, alvo.https, alvo.caminho_inicial())
                    })
            })
        });
        da_lista.unwrap_or_else(|| {
            let tipo = catalogo::porta(porta).map(|i| i.tipo);
            (
                matches!(tipo, Some(Tipo::Web | Tipo::WebHttps | Tipo::Painel)),
                tipo == Some(Tipo::WebHttps),
                "/".to_owned(),
            )
        })
    }

    // O diálogo "Encaminhar destino…".

    /// Abre o diálogo, vazio ou já com o destino de uma porta.
    pub(super) fn abrir_dialogo_destino(&self, previo: Option<&Alvo>) {
        let d = &self.portas.dialogo;
        d.host
            .set(previo.map_or_else(String::new, |a| a.destino.clone()));
        d.porta.set(previo.map_or(0.0, |a| f64::from(a.porta)));
        d.local.set(0.0);
        d.caminho
            .set(previo.map_or_else(String::new, Alvo::caminho_inicial));
        d.site.set(String::new());
        d.https.set(previo.is_some_and(|a| a.https));
        d.opcoes.set(false);
        d.abrir.set(false);
        d.fixar.set(false);
        d.erro.set(String::new());
        d.testando.set(false);
        d.mesmo.set(false);
        d.aberto.set(true);
    }

    /// "Nome do site…": o diálogo da porta com as "Opções da web" abertas.
    pub(super) fn abrir_dialogo_site(&self, alvo: &Alvo) {
        self.abrir_dialogo_destino(Some(alvo));
        self.portas.dialogo.opcoes.set(true);
    }

    /// O botão "Encaminhar": testa o destino numa thread e, se o servidor chega lá, encaminha.
    fn testar_destino_do_dialogo(self: &Rc<Self>) {
        let d = &self.portas.dialogo;
        let (Some(porta), Some(sessao)) = (d.porta_do_destino(), self.sessao_do_painel()) else {
            d.erro.set(t!("portas-dialogo-invalido"));
            return;
        };
        let host = d.host.get_untracked().trim().to_owned();
        let testador = match self.opener.testador_de_destino(sessao) {
            Ok(testador) => testador,
            Err(erro) => {
                d.erro.set(erro);
                return;
            }
        };
        let limite = Duration::from_millis(u64::from(
            self.config.with_untracked(|c| c.portas.sonda_ms).max(200),
        ));
        d.erro.set(String::new());
        d.mesmo.set(false);
        d.testando.set(true);
        let resultado: Arc<Mutex<Option<Result<(), String>>>> = Arc::default();
        let entrega = Arc::clone(&resultado);
        thread::spawn(move || {
            let r = testador(&host, porta, limite);
            *entrega.lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
        });
        self.esperar_teste(resultado, 0);
    }

    fn esperar_teste(
        self: &Rc<Self>,
        resultado: Arc<Mutex<Option<Result<(), String>>>>,
        passo: u32,
    ) {
        let t = Rc::clone(self);
        self.alarmes.depois(Duration::from_millis(50), move || {
            let d = &t.portas.dialogo;
            if !d.aberto.get_untracked() {
                d.testando.set(false);
                return;
            }
            let pronto = resultado
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            match pronto {
                Some(Ok(())) => {
                    d.testando.set(false);
                    t.encaminhar_do_dialogo();
                }
                Some(Err(erro)) => {
                    d.testando.set(false);
                    d.erro
                        .set(t!("portas-dialogo-destino-fechado", erro = erro));
                    d.mesmo.set(true);
                }
                None if passo < PASSOS_DE_ESPERA => t.esperar_teste(resultado, passo + 1),
                None => {
                    d.testando.set(false);
                    d.erro.set(t!("portas-demorou"));
                    d.mesmo.set(true);
                }
            }
        });
    }

    /// Encaminha o que o diálogo descreve e o fecha.
    fn encaminhar_do_dialogo(self: &Rc<Self>) {
        let d = &self.portas.dialogo;
        let (Some(porta), Ok(local), Some(sessao)) = (
            d.porta_do_destino(),
            d.porta_daqui(),
            self.sessao_do_painel(),
        ) else {
            d.erro.set(t!("portas-dialogo-invalido"));
            return;
        };
        let host = d.host.get_untracked().trim().to_owned();
        let caminho = d.caminho.get_untracked();
        let https = d.https.get_untracked();
        let site = d.site.get_untracked().trim().to_owned();
        let pedido = Pedido {
            nome: format!("{host}:{porta}"),
            destino: host,
            porta,
            https,
            caminho: if caminho.trim().is_empty() {
                "/".to_owned()
            } else {
                caminho.trim().to_owned()
            },
            host_http: site,
            local,
            depois: if d.abrir.get_untracked() {
                Depois::Abrir
            } else {
                Depois::Nada
            },
            fixar: d.fixar.get_untracked(),
            tentativas: 0,
        };
        d.aberto.set(false);
        self.encaminhar_porta(sessao, pedido);
    }
}

fn tunel_casa(tunel: &Tunel, destino: &str, porta: u16) -> bool {
    tunel.tipo == TipoTunel::Local && tunel.destino_host == destino && tunel.destino_porta == porta
}

/// O menu "…" de uma porta (o mesmo do clique direito). Os itens têm o id `{base}-{ação}`.
pub(super) fn menu_da_porta(tela: &Rc<Tela>, base: &str, alvo: &Alvo) -> MenuFlyout {
    let id = |acao: &str| format!("{base}-{acao}");
    let mut menu = MenuFlyout::new().id(id("menu"));
    // Os rótulos e a visibilidade leem o relógio do painel: o menu acompanha o túnel.
    let ligado = {
        let (t, alvo) = (Rc::clone(tela), alvo.clone());
        move || {
            let _ = (t.portas.relogio.get(), t.portas.versao.get());
            t.ligada(&alvo.destino, alvo.porta)
        }
    };
    let fixada = {
        let (t, alvo) = (Rc::clone(tela), alvo.clone());
        move || {
            let _ = t.portas.versao.get();
            t.esta_fixada(&alvo.destino, alvo.porta)
        }
    };
    let agir = |depois: Depois, caminho: Option<String>| {
        let (t, alvo) = (Rc::clone(tela), alvo.clone());
        move || {
            if let Some(sessao) = t.sessao_do_painel() {
                t.encaminhar_porta(
                    sessao,
                    Pedido::novo(&alvo, depois.clone(), caminho.clone(), false),
                );
            }
        }
    };
    if alvo.web {
        menu = menu
            .item(
                id("abrir"),
                t!("portas-abrir-navegador"),
                agir(Depois::Abrir, None),
            )
            .icon(Symbol::Globe);
        for (n, atalho) in alvo.atalhos.iter().enumerate() {
            menu = menu.item(
                id(&format!("app-{n}")),
                t!("portas-abrir-app", app = atalho.nome.as_str()),
                agir(Depois::Abrir, Some(atalho.caminho.clone())),
            );
        }
        menu = menu.separator();
    }
    menu = menu
        .item(
            id("encaminhar"),
            t!("portas-encaminhar"),
            agir(Depois::Nada, None),
        )
        .icon(Symbol::PlugConnected);
    let (t, previo) = (Rc::clone(tela), alvo.clone());
    menu = menu.item(
        id("encaminhar-para"),
        t!("portas-encaminhar-para"),
        move || t.abrir_dialogo_destino(Some(&previo)),
    );
    if alvo.web {
        let (t, previo) = (Rc::clone(tela), alvo.clone());
        menu = menu.item(id("site"), t!("portas-nome-do-site"), move || {
            t.abrir_dialogo_site(&previo)
        });
    }
    menu = menu
        .item(
            id("copiar"),
            t!("portas-copiar-endereco"),
            agir(Depois::CopiarEndereco, None),
        )
        .icon(Symbol::Copy);
    if alvo.web {
        menu = menu.item(
            id("copiar-url"),
            t!("portas-copiar-url"),
            agir(Depois::CopiarUrl, None),
        );
    }
    if let Some(esquema) = alvo.esquema {
        menu = menu.item(
            id("copiar-esquema"),
            t!("portas-copiar-como", esquema = esquema),
            agir(Depois::CopiarEsquema(esquema), None),
        );
    }
    menu = menu.separator();
    let (fixar, visivel) = (Rc::clone(tela), fixada.clone());
    let alvo_fixar = alvo.clone();
    menu = menu
        .item(id("fixar"), t!("portas-fixar"), move || {
            if let Some(sessao) = fixar.sessao_do_painel() {
                let caminho = alvo_fixar.web.then(|| alvo_fixar.caminho_inicial());
                fixar.encaminhar_porta(
                    sessao,
                    Pedido::novo(&alvo_fixar, Depois::Nada, caminho, true),
                );
            }
        })
        .icon(Symbol::Pin)
        .item_visible(move || !visivel());
    let (desafixar, visivel) = (Rc::clone(tela), fixada);
    let (destino, porta) = (alvo.destino.clone(), alvo.porta);
    menu = menu
        .item(id("desafixar"), t!("portas-desafixar"), move || {
            desafixar.desafixar(&destino, porta);
        })
        .icon(Symbol::Pin)
        .item_visible(visivel);
    let (parar, visivel) = (Rc::clone(tela), ligado);
    let (destino, porta) = (alvo.destino.clone(), alvo.porta);
    menu = menu
        .item(id("parar"), t!("portas-parar"), move || {
            parar.parar_porta(&destino, porta);
        })
        .icon(Symbol::Stop)
        .item_visible(visivel);
    if let (true, Some(inode)) = (alvo.sem_processo, alvo.inode) {
        let t = Rc::clone(tela);
        menu = menu
            .separator()
            .item(id("processo"), t!("portas-descobrir-processo"), move || {
                t.descobrir_processo(inode)
            });
    }
    menu
}

/// O botão principal de uma porta: "Abrir" na web, "Encaminhar" nas outras; com o túnel de pé,
/// "localhost:{p}" (abre na web, copia nas outras).
pub(super) fn botao_principal(tela: &Rc<Tela>, id: String, alvo: &Alvo) -> Button {
    let rotulo = {
        let (t, alvo) = (Rc::clone(tela), alvo.clone());
        move || {
            let _ = (t.portas.relogio.get(), t.portas.versao.get());
            match t.porta_local_de(&alvo.destino, alvo.porta) {
                Some(local) => t!("portas-principal-local", porta = local),
                None if alvo.web => t!("portas-principal-abrir"),
                None => t!("portas-principal-encaminhar"),
            }
        }
    };
    let (t, alvo) = (Rc::clone(tela), alvo.clone());
    Button::new(rotulo).id(id).on_click(move || {
        let Some(sessao) = t.sessao_do_painel() else {
            return;
        };
        let depois = if alvo.web {
            Depois::Abrir
        } else if t.ligada(&alvo.destino, alvo.porta) {
            Depois::CopiarEndereco
        } else {
            Depois::Nada
        };
        t.encaminhar_porta(sessao, Pedido::novo(&alvo, depois, None, false));
    })
}

/// A seção "Encaminhadas": os túneis que a aba tem de pé.
pub(super) fn encaminhadas(tela: &Rc<Tela>) -> impl View + use<> {
    let t = Rc::clone(tela);
    let titulo = Text::new(t!("portas-encaminhadas"))
        .id("portas-encaminhadas")
        .body_strong()
        .visible({
            let t = Rc::clone(tela);
            move || !t.linhas_encaminhadas().is_empty()
        });
    let lista = For::each(
        move || t.linhas_encaminhadas(),
        |l: &LinhaEncaminhada| l.chave.clone(),
        {
            let tela = Rc::clone(tela);
            move |l: LinhaEncaminhada| linha_encaminhada(&tela, &l)
        },
    );
    Column::new().gap(8).child(titulo).child(lista)
}

fn linha_encaminhada(tela: &Rc<Tela>, l: &LinhaEncaminhada) -> impl View + use<> {
    let alvo = Alvo {
        destino: l.destino.clone(),
        porta: l.porta,
        nome: format!("{}:{}", l.destino, l.porta),
        web: l.web,
        https: l.https,
        esquema: None,
        atalhos: vec![Atalho {
            nome: String::new(),
            caminho: l.caminho.clone(),
        }],
        inode: None,
        sem_processo: false,
    };
    let host_http = l.host_http.clone();
    let acao = |depois: Depois| {
        let (t, alvo, host_http) = (Rc::clone(tela), alvo.clone(), host_http.clone());
        move || {
            if let Some(sessao) = t.sessao_do_painel() {
                let mut pedido = Pedido::novo(&alvo, depois.clone(), None, false);
                pedido.host_http = host_http.clone();
                t.encaminhar_porta(sessao, pedido);
            }
        }
    };
    let (parar, destino, porta) = (Rc::clone(tela), l.destino.clone(), l.porta);
    let titulo = Row::new()
        .gap(8)
        .align(Align::Center)
        .child(
            RichText::new(vec![Span::new(format!(
                "{} → {}:{}",
                url_local(l.local, "/", &l.host_http)
                    .trim_start_matches("http://")
                    .trim_end_matches('/'),
                l.destino,
                l.porta
            ))])
            .monospace(),
        )
        .child(
            Icon::new(Symbol::Pin)
                .size(14)
                .decorative()
                .visible(l.fixada),
        );
    Row::new()
        .id(l.id.clone())
        .gap(8)
        .align(Align::Center)
        .padding(Insets::symmetric(4.0, 6.0))
        .child(
            Column::new().gap(2).grow(1).child(titulo).child(
                Text::new(l.estado.clone())
                    .id(format!("{}-estado", l.id))
                    .caption()
                    .secondary()
                    .max_lines(1),
            ),
        )
        .child(
            Button::new(t!("portas-principal-abrir"))
                .id(format!("{}-abrir", l.id))
                .visible(l.web)
                .on_click(acao(Depois::Abrir)),
        )
        .child(
            Button::new(t!("comum-copiar"))
                .id(format!("{}-copiar", l.id))
                .on_click(acao(Depois::CopiarEndereco)),
        )
        .child(
            Button::new(t!("portas-parar"))
                .id(format!("{}-parar", l.id))
                .on_click(move || parar.parar_porta(&destino, porta)),
        )
}

/// O diálogo "Encaminhar destino…".
pub(super) fn dialogo(tela: &Rc<Tela>) -> ContentDialog {
    let d = &tela.portas.dialogo;
    let (host, porta, local, caminho) = (d.host, d.porta, d.local, d.caminho);
    let (site, https, opcoes) = (d.site, d.https, d.opcoes);
    let (abrir, fixar, erro, testando, mesmo) = (d.abrir, d.fixar, d.erro, d.testando, d.mesmo);
    let sugerir = {
        let t = Rc::clone(tela);
        move |digitado: &str| t.sugestoes_de_host(digitado)
    };
    let valido = {
        let t = Rc::clone(tela);
        move || t.portas.dialogo.valido()
    };
    let (t_testar, t_mesmo) = (Rc::clone(tela), Rc::clone(tela));
    let corpo = Column::new()
        .gap(8)
        .child(Text::new(t!("portas-dialogo-host")).caption())
        .child(
            AutoSuggestBox::bind(host, sugerir)
                .id("portas-dialogo-host")
                .placeholder("127.0.0.1"),
        )
        .child(Text::new(t!("portas-dialogo-porta")).caption())
        .child(
            NumberBox::bind(porta)
                .id("portas-dialogo-porta")
                .min(0)
                .max(65535)
                .precision(0)
                .small_change(1),
        )
        .child(Text::new(t!("portas-dialogo-local")).caption())
        .child(
            NumberBox::bind(local)
                .id("portas-dialogo-local")
                .min(0)
                .max(65535)
                .precision(0)
                .small_change(1),
        )
        .child(Text::new(t!("portas-dialogo-caminho")).caption())
        .child(
            Edit::bind(caminho)
                .id("portas-dialogo-caminho")
                .placeholder("/phpmyadmin/"),
        )
        .child(
            Expander::new(t!("portas-dialogo-opcoes-web"), opcoes)
                .id("portas-dialogo-opcoes-web")
                .content(
                    Column::new()
                        .gap(8)
                        .child(Text::new(t!("portas-dialogo-site")).caption())
                        .child(
                            Edit::bind(site)
                                .id("portas-dialogo-site")
                                .placeholder("loja.exemplo.com.br"),
                        )
                        .child(
                            CheckBox::bind(https, t!("portas-dialogo-https"))
                                .id("portas-dialogo-https"),
                        ),
                ),
        )
        .child(CheckBox::bind(abrir, t!("portas-dialogo-abrir")).id("portas-dialogo-abrir"))
        .child(CheckBox::bind(fixar, t!("portas-dialogo-fixar")).id("portas-dialogo-fixar"))
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("portas-dialogo-erro")
                .message(move || erro.get())
                .visible(move || !erro.with(String::is_empty)),
        )
        .child(
            Text::new(t!("portas-dialogo-testando"))
                .id("portas-dialogo-testando")
                .caption()
                .secondary()
                .visible(move || testando.get()),
        )
        .child(
            Row::new()
                .gap(8)
                .child(
                    Button::new(t!("portas-dialogo-encaminhar"))
                        .id("portas-dialogo-encaminhar")
                        .accent()
                        .disabled(move || !valido() || testando.get())
                        .on_click(move || t_testar.testar_destino_do_dialogo()),
                )
                .child(
                    Button::new(t!("portas-dialogo-mesmo"))
                        .id("portas-dialogo-mesmo")
                        .disabled(move || testando.get())
                        .visible(move || mesmo.get())
                        .on_click(move || t_mesmo.encaminhar_do_dialogo()),
                ),
        );
    ContentDialog::new(t!("portas-dialogo-titulo"), d.aberto)
        .id("portas-dialogo")
        .content(corpo)
        .close(t!("comum-cancelar"))
}

impl Tela {
    /// As sugestões do campo Host do diálogo: os IPs dos contêineres, os destinos dos túneis
    /// da conexão e o loopback, filtrados pelo que foi digitado.
    fn sugestoes_de_host(&self, digitado: &str) -> Vec<String> {
        let mut todas: Vec<String> = vec!["127.0.0.1".to_owned()];
        let aba = self.portas.aba.get_untracked();
        if let Some(lista) = self
            .portas
            .sessoes
            .borrow()
            .get(&aba)
            .and_then(|d| d.descoberta.clone())
        {
            todas.extend(
                lista
                    .conteineres
                    .iter()
                    .filter_map(|c| c.ip())
                    .map(|ip| ip.to_string()),
            );
        }
        if let Some(id) = self
            .sessao_do_painel()
            .and_then(|s| self.conexao_da_sessao(s))
        {
            self.tree.with_untracked(|t| {
                if let Some(c) = t.connection(id) {
                    todas.extend(
                        c.tuneis
                            .iter()
                            .filter(|x| x.tipo == TipoTunel::Local)
                            .map(|x| x.destino_host.clone()),
                    );
                }
            });
        }
        let digitado = digitado.trim().to_lowercase();
        let mut vistas: Vec<String> = Vec::new();
        for host in todas {
            if !host.is_empty()
                && host.to_lowercase().contains(&digitado)
                && host.to_lowercase() != digitado
                && !vistas.contains(&host)
            {
                vistas.push(host);
            }
        }
        vistas
    }
}
