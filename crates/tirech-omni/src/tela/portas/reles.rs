//! Os túneis do painel de portas que passam pelo relé do app (M58, seção 3.8 do plano): os de
//! "Nome do site" e os de servidor HTTPS. Os outros são `-L` do SSH (`acoes.rs`).
//!
//! A tela guarda cada relé aberto com a sessão dele; quando a sessão fecha, o relé para.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::rc::Rc;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::modelo::Connection;
use crate::portas::descobrir::AbridorCompartilhado;
use crate::portas::local::{escolher_porta_local, porta_livre};
use crate::portas::rele::{Discador, EstadoRele, Modo, Rele, ReleId};
use crate::t;

/// Quantas vezes tenta de novo quando outro programa toma a porta entre a conferência e o `bind`.
const TENTATIVAS: u8 = 3;

/// De quanto em quanto tempo os relés conferem se a sessão ainda existe.
const PODA: Duration = Duration::from_secs(2);

/// Um relé aberto por este painel.
pub(in crate::tela) struct ReleAberto {
    pub sessao: Session,
    pub id: ReleId,
    pub rele: &'static Rele,
    pub abridor: AbridorCompartilhado,
    pub destino: String,
    pub porta: u16,
    pub host_http: String,
    pub https: bool,
    pub local: u16,
}

impl ReleAberto {
    fn estado(&self) -> Option<EstadoRele> {
        self.rele.estado(self.id)
    }

    /// Um número para o id da linha na tela.
    fn numero(&self) -> u64 {
        self.id.numero()
    }
}

/// O que o relé atende: o destino visto do servidor e como falar com ele.
#[derive(Clone, Copy)]
pub(super) struct AlvoDoRele<'a> {
    pub destino: &'a str,
    pub porta: u16,
    pub host_http: &'a str,
    pub https: bool,
}

/// Uma linha de relé para a seção "Encaminhadas".
pub(super) struct DoRele {
    pub destino: String,
    pub porta: u16,
    pub host_http: String,
    pub https: bool,
    pub local: u16,
    pub estado: EstadoRele,
    pub id: u64,
}

impl Tela {
    /// A porta daqui do relé de `destino:porta` na sessão (qualquer nome de site).
    pub(super) fn rele_de(&self, sessao: Session, destino: &str, porta: u16) -> Option<u16> {
        self.portas
            .reles
            .borrow()
            .iter()
            .find(|r| r.sessao == sessao && r.destino == destino && r.porta == porta)
            .map(|r| r.local)
    }

    /// O relé que atende este pedido: o de mesmo nome de site e mesmo TLS; sem nome de site, o
    /// que houver para o destino. Devolve a porta daqui, o nome de site e se fala TLS.
    pub(super) fn rele_para(
        &self,
        sessao: Session,
        destino: &str,
        porta: u16,
        host_http: &str,
        https: bool,
    ) -> Option<(u16, String, bool)> {
        let reles = self.portas.reles.borrow();
        let do_destino = || {
            reles
                .iter()
                .filter(|r| r.sessao == sessao && r.destino == destino && r.porta == porta)
        };
        do_destino()
            .find(|r| r.host_http == host_http && r.https == https)
            .or_else(|| {
                if host_http.trim().is_empty() {
                    do_destino().next()
                } else {
                    None
                }
            })
            .map(|r| (r.local, r.host_http.clone(), r.https))
    }

    /// Abre o relé de `destino:porta` na sessão. `local` é a porta pedida (precisa estar livre);
    /// sem ela, a automática a partir de `preferida` (seção 3.4 do plano).
    pub(super) fn abrir_rele(
        self: &Rc<Self>,
        sessao: Session,
        alvo: AlvoDoRele<'_>,
        local: Option<u16>,
        preferida: u16,
    ) -> Result<u16, String> {
        let AlvoDoRele {
            destino,
            porta,
            host_http,
            https,
        } = alvo;
        let rele = Rele::global().map_err(|e| e.to_string())?;
        let abridor: AbridorCompartilhado = Arc::new(Mutex::new(sessao.ssh_channels()));
        let mut ultimo = String::new();
        for _ in 0..=TENTATIVAS {
            let escolhida = match local {
                Some(pedida) if porta_livre(pedida) => pedida,
                Some(pedida) => return Err(t!("portas-local-ocupada", porta = pedida)),
                None => escolher_porta_local(preferida, porta_livre)
                    .ok_or_else(|| t!("portas-sem-porta-livre"))?,
            };
            let bind = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), escolhida);
            match rele.abrir(
                bind,
                Discador::Ssh(Arc::clone(&abridor)),
                (destino.to_owned(), porta),
                Modo::web(https, host_http),
            ) {
                Ok(id) => {
                    self.portas.reles.borrow_mut().push(ReleAberto {
                        sessao,
                        id,
                        rele,
                        abridor,
                        destino: destino.to_owned(),
                        porta,
                        host_http: host_http.to_owned(),
                        https,
                        local: escolhida,
                    });
                    self.agendar_poda_dos_reles();
                    return Ok(escolhida);
                }
                Err(e) if local.is_some() => return Err(e.to_string()),
                Err(e) => ultimo = e.to_string(),
            }
        }
        Err(ultimo)
    }

    /// Para os relés de `destino:porta` na sessão. Devolve se havia algum.
    pub(super) fn parar_reles(&self, sessao: Session, destino: &str, porta: u16) -> bool {
        let mut reles = self.portas.reles.borrow_mut();
        let antes = reles.len();
        reles.retain(|r| {
            let casa = r.sessao == sessao && r.destino == destino && r.porta == porta;
            if casa {
                r.rele.parar(r.id);
            }
            !casa
        });
        reles.len() != antes
    }

    /// Os relés da sessão do painel, para a seção "Encaminhadas".
    pub(super) fn reles_da_sessao(&self, sessao: Session) -> Vec<DoRele> {
        self.portas
            .reles
            .borrow()
            .iter()
            .filter(|r| r.sessao == sessao)
            .filter_map(|r| {
                Some(DoRele {
                    destino: r.destino.clone(),
                    porta: r.porta,
                    host_http: r.host_http.clone(),
                    https: r.https,
                    local: r.local,
                    estado: r.estado()?,
                    id: r.numero(),
                })
            })
            .collect()
    }

    /// Sobe os túneis da conexão que passam pelo relé (os fixados com nome de site ou HTTPS), na
    /// sessão que acabou de abrir. Uma porta que não abre fica de fora, sem barulho: o túnel
    /// segue na conexão e o painel mostra o resto.
    pub(in crate::tela) fn subir_reles_da_conexao(
        self: &Rc<Self>,
        sessao: Session,
        conexao: &Connection,
    ) {
        for tunel in crate::abrir::tuneis_do_rele(conexao) {
            let alvo = AlvoDoRele {
                destino: &tunel.destino_host,
                porta: tunel.destino_porta,
                host_http: &tunel.host_http,
                https: tunel.https,
            };
            let _ = self.abrir_rele(sessao, alvo, None, tunel.porta);
        }
    }

    /// Confere de tempos em tempos se as sessões dos relés ainda existem, e renova o abridor de
    /// canais delas (depois de um reinício o antigo só devolve erro).
    pub(super) fn agendar_poda_dos_reles(self: &Rc<Self>) {
        if self.portas.podando.replace(true) {
            return;
        }
        let tela = Rc::clone(self);
        self.alarmes.depois(PODA, move || {
            tela.portas.podando.set(false);
            tela.podar_reles();
        });
    }

    fn podar_reles(self: &Rc<Self>) {
        let vivos = {
            let mut reles = self.portas.reles.borrow_mut();
            reles.retain(|r| {
                let fechada = r.sessao.closed();
                if fechada {
                    r.rele.parar(r.id);
                } else {
                    *r.abridor
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        r.sessao.ssh_channels();
                }
                !fechada
            });
            !reles.is_empty()
        };
        if vivos {
            self.agendar_poda_dos_reles();
        }
        self.portas.versao.update(|v| *v += 1);
    }
}
