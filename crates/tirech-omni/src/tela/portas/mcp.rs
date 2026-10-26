//! As ferramentas do MCP das portas do servidor (M58, seção 14): `listar_portas`,
//! `encaminhar_porta` e `parar_porta`. Usam os mesmos túneis e relés do painel; o assistente
//! roda nesta máquina e recebe o endereço daqui (`127.0.0.1:3307`).

use std::net::IpAddr;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::{Forward, ForwardState, ForwardStatus};

use super::Tela;
use super::acoes::{Pedido, casa, porta_local_de};
use super::nome_da_porta;
use super::reles::AlvoDoRele;
use crate::modelo::Kind;
use crate::portas::descobrir::AbridorCompartilhado;
use crate::portas::local::{escolher_porta_local, porta_livre};
use crate::portas::url::{endereco_local, url_local};
use crate::portas::{Alcance, Descoberta, PortaDoServidor, Servico};
use crate::t;
use crate::tela::Lugar;
use crate::tela::mcp::{Plano, Saida, Sonda, Trabalho};

/// Quanto tempo se espera a aba abrir e o SSH subir.
const PRAZO_DA_ABERTURA: Duration = Duration::from_secs(60);

/// Quanto tempo se espera a leitura das portas (o `ss`, o `docker` e as sondas).
const PRAZO_DA_LEITURA: Duration = Duration::from_secs(90);

/// Quanto tempo se espera o túnel escutar.
const PRAZO_DO_TUNEL: Duration = Duration::from_secs(20);

/// Quantas vezes escolhe outra porta se a primeira foi tomada entre a conferência e o `bind`.
const TENTATIVAS: u8 = 3;

/// O que se faz com a sessão SSH da conexão, quando ela está pronta.
type Seguir = Box<dyn FnOnce(&Rc<Tela>, Rc<Trabalho>, Session)>;

/// Uma porta de 1 a 65535, de um número ou de um texto.
fn porta_do(valor: Option<&Value>) -> Option<u16> {
    let numero = match valor? {
        Value::Number(n) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| f.fract() == 0.0 && *f > 0.0)
                .map(|f| f as u64)
        })?,
        Value::String(s) => s.trim().parse().ok()?,
        _ => return None,
    };
    u16::try_from(numero).ok().filter(|p| *p != 0)
}

/// Se o túnel é um `-L` que ainda não parou.
fn local_ativo(status: &ForwardStatus) -> bool {
    matches!(status.forward, Forward::Local { .. })
        && matches!(
            status.state,
            ForwardState::Starting | ForwardState::Listening
        )
}

impl Tela {
    /// `listar_portas`: o que escuta no servidor da conexão. Abre a aba se preciso.
    pub(in crate::tela) fn mcp_listar_portas(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        self.mcp_com_sessao(
            t,
            plano,
            true,
            Box::new(|tela, t, sessao| tela.mcp_ler_portas(t, sessao)),
        );
    }

    /// `encaminhar_porta`: leva uma porta do servidor para esta máquina e devolve o endereço.
    pub(in crate::tela) fn mcp_encaminhar_porta(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(porta) = porta_do(args.get("porta")) else {
            return self.terminar_mcp(&t, Saida::Erro(t!("mcp-porta-invalida")));
        };
        let local = match args.get("porta_local").filter(|v| !v.is_null()) {
            None => None,
            Some(valor) => match porta_do(Some(valor)) {
                Some(local) => Some(local),
                None => return self.terminar_mcp(&t, Saida::Erro(t!("mcp-porta-invalida"))),
            },
        };
        let texto = |chave: &str| args[chave].as_str().unwrap_or_default().trim().to_owned();
        let host = Some(texto("host"))
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "127.0.0.1".to_owned());
        let pedido = Pedido::do_mcp(
            host,
            porta,
            args["https"].as_bool().unwrap_or(false),
            texto("host_http"),
            local,
            args["fixar"].as_bool().unwrap_or(false),
        );
        self.mcp_com_sessao(
            t,
            plano,
            true,
            Box::new(move |tela, t, sessao| tela.mcp_encaminhar(t, sessao, pedido)),
        );
    }

    /// `parar_porta`: para o túnel que escuta na porta daqui.
    pub(in crate::tela) fn mcp_parar_porta(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let Some(local) = porta_do(t.pedido.args.get("porta_local")) else {
            return self.terminar_mcp(&t, Saida::Erro(t!("mcp-porta-invalida")));
        };
        self.mcp_com_sessao(
            t,
            plano,
            false,
            Box::new(move |tela, t, sessao| {
                let saida = tela.mcp_parar(sessao, local);
                tela.terminar_mcp(&t, saida);
            }),
        );
    }

    /// Acha a sessão SSH da conexão do pedido (abrindo a aba, se `abrir`), espera o SSH subir e
    /// segue com `seguir`.
    fn mcp_com_sessao(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano, abrir: bool, seguir: Seguir) {
        let Some(conexao) = plano.conexao.clone() else {
            return self.terminar_mcp(&t, Saida::Erro(t!("mcp-conexao-invalida")));
        };
        if conexao.kind != Kind::Ssh {
            return self.terminar_mcp(&t, Saida::Erro(t!("portas-sem-ssh")));
        }
        let existente = plano.sessao.filter(|s| !s.closed()).or_else(|| {
            self.aba_da_conexao(conexao.id)
                .map(|(_, s)| s)
                .filter(|s| !s.closed())
        });
        if let Some(sessao) = existente.filter(|s| s.ssh_forwards().is_some()) {
            return seguir(self, t, sessao);
        }
        let antes: Vec<String> = self
            .sessoes_das_janelas(None)
            .iter()
            .map(|(_, s)| s.key_str())
            .collect();
        if existente.is_none() {
            if !abrir {
                return self.terminar_mcp(&t, Saida::Erro(t!("mcp-portas-sem-aba")));
            }
            self.abrir_com(conexao.id, Lugar::default(), true);
        }
        let prefixo = format!("{}-", conexao.id);
        let inicio = Instant::now();
        let mut escolhida = existente;
        let mut marcada = existente.is_some();
        let mut seguir = Some(seguir);
        let trabalho = Rc::clone(&t);
        self.passo_mcp(
            t,
            Box::new(move |tela| {
                if escolhida.is_none() {
                    escolhida = tela
                        .sessoes_das_janelas(None)
                        .into_iter()
                        .map(|(_, s)| s)
                        .find(|s| {
                            let chave = s.key_str();
                            chave.starts_with(&prefixo) && !antes.contains(&chave)
                        });
                }
                if let Some(sessao) = escolhida {
                    if !marcada {
                        marcada = true;
                        let chave = sessao.key_str();
                        tela.marcar_aba_mcp(&trabalho, &chave);
                        tela.dar_aba(&chave, trabalho.pedido.ligacao);
                    }
                    if sessao.closed() {
                        return Sonda::Fim(Saida::Erro(t!("mcp-portas-aba-caiu")));
                    }
                    if sessao.ssh_forwards().is_some() {
                        if let Some(seguir) = seguir.take() {
                            seguir(tela, Rc::clone(&trabalho), sessao);
                        }
                        return Sonda::Seguiu;
                    }
                }
                if inicio.elapsed() >= PRAZO_DA_ABERTURA {
                    return Sonda::Fim(Saida::Erro(t!("mcp-abrir-sem-aba")));
                }
                Sonda::Depois(Duration::from_millis(200))
            }),
        );
    }

    /// Lê as portas numa thread (o `ss` e as sondas demoram) e responde quando chegar.
    fn mcp_ler_portas(self: &Rc<Self>, t: Rc<Trabalho>, sessao: Session) {
        let cfg = self.config.with_untracked(|c| c.portas.clone());
        let abridor: AbridorCompartilhado = Arc::new(Mutex::new(sessao.ssh_channels()));
        let mut fonte = self.opener.fonte_de_portas(abridor, &cfg);
        let resposta: Arc<Mutex<Option<Result<Descoberta, String>>>> = Arc::default();
        let entrega = Arc::clone(&resposta);
        let subiu = thread::Builder::new()
            .name("portas-mcp".to_owned())
            .spawn(move || {
                let lida = fonte.descobrir(None).map_err(|e| e.to_string());
                *entrega.lock().unwrap_or_else(PoisonError::into_inner) = Some(lida);
            });
        if let Err(erro) = subiu {
            return self.terminar_mcp(&t, Saida::Erro(erro.to_string()));
        }
        let inicio = Instant::now();
        self.passo_mcp(
            t,
            Box::new(move |tela| {
                let lida = resposta
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                match lida {
                    Some(Ok(lista)) => Sonda::Fim(Saida::servidor(
                        "portas",
                        tela.mcp_json_das_portas(sessao, &lista),
                        None,
                    )),
                    Some(Err(erro)) => Sonda::Fim(Saida::Erro(erro)),
                    None if inicio.elapsed() >= PRAZO_DA_LEITURA => {
                        Sonda::Fim(Saida::Erro(t!("mcp-portas-demorou")))
                    }
                    None => Sonda::Depois(Duration::from_millis(100)),
                }
            }),
        );
    }

    /// A lista de `listar_portas`, em JSON. As portas de sistema ficam de fora, como no painel,
    /// a menos que as Configurações mandem mostrar.
    fn mcp_json_das_portas(&self, sessao: Session, lista: &Descoberta) -> String {
        let mostrar_sistema = self.config.with_untracked(|c| c.portas.mostrar_sistema);
        let forwards = sessao.ssh_forwards().map(|f| f.list()).unwrap_or_default();
        let reles = self.reles_da_sessao(sessao);
        let itens: Vec<Value> = lista
            .portas
            .iter()
            .filter(|p| mostrar_sistema || !p.de_sistema())
            .map(|p| {
                let mut item = json!({
                    "porta": p.porta,
                    "enderecos": enderecos_sem_repetir(p),
                    "alcance": match &p.alcance {
                        Alcance::SoNoServidor => "servidor",
                        Alcance::TodaARede => "rede",
                        Alcance::So(_) => "ip",
                    },
                    "servico": nome_da_porta(p),
                    "atalhos": p
                        .atalhos
                        .iter()
                        .map(|a| json!({ "nome": a.nome, "caminho": a.caminho }))
                        .collect::<Vec<_>>(),
                });
                if let Servico::Web {
                    titulo: Some(titulo),
                    ..
                } = &p.servico
                    && !titulo.trim().is_empty()
                {
                    item["titulo"] = json!(titulo.trim());
                }
                if let Some(processo) = &p.processo {
                    item["processo"] = json!(processo.nome);
                }
                if let Some(usuario) = &p.usuario {
                    item["usuario"] = json!(usuario);
                }
                if let Some(conteiner) = &p.conteiner {
                    item["conteiner"] = json!(conteiner);
                }
                let por_forward = forwards
                    .iter()
                    .find(|s| casa(s, &p.destino, p.porta))
                    .and_then(porta_local_de)
                    .map(|local| (local, String::new()));
                let por_rele = || {
                    reles
                        .iter()
                        .find(|r| r.destino == p.destino && r.porta == p.porta)
                        .map(|r| (r.local, r.host_http.clone()))
                };
                if let Some((local, host_http)) = por_forward.or_else(por_rele) {
                    let mut ligada = json!({
                        "porta_local": local,
                        "endereco": endereco_local(local),
                    });
                    if p.e_web() {
                        ligada["url"] = json!(url_local(local, "/", &host_http));
                    }
                    item["encaminhada"] = ligada;
                }
                item
            })
            .collect();
        serde_json::to_string_pretty(&itens).unwrap_or_else(|_| "[]".to_owned())
    }

    /// Leva `pedido` para esta máquina: reaproveita o túnel que já existe, passa pelo relé quando
    /// o pedido pede, ou abre um `-L` e espera ele escutar.
    fn mcp_encaminhar(self: &Rc<Self>, t: Rc<Trabalho>, sessao: Session, pedido: Pedido) {
        let Some(forwards) = sessao.ssh_forwards() else {
            return self.terminar_mcp(&t, Saida::Erro(t!("portas-sem-ssh")));
        };
        let existente = forwards
            .list()
            .into_iter()
            .find(|s| casa(s, &pedido.destino, pedido.porta));
        let rele_pronto = self
            .rele_de(sessao, &pedido.destino, pedido.porta)
            .is_some();
        if pedido.usa_rele() || (existente.is_none() && rele_pronto) {
            let saida = self.mcp_encaminhar_por_rele(sessao, pedido);
            return self.terminar_mcp(&t, saida);
        }
        if let Some(status) = existente {
            if let Some(local) =
                porta_local_de(&status).filter(|_| matches!(status.state, ForwardState::Listening))
            {
                let saida = self.mcp_concluir(sessao, local, &pedido);
                return self.terminar_mcp(&t, saida);
            }
            return self.mcp_esperar_tunel(t, sessao, status.id, pedido);
        }
        let local = match pedido.local {
            Some(pedida) if porta_livre(pedida) => pedida,
            Some(pedida) => {
                return self
                    .terminar_mcp(&t, Saida::Erro(t!("portas-local-ocupada", porta = pedida)));
            }
            None => match escolher_porta_local(pedido.porta, porta_livre) {
                Some(local) => local,
                None => {
                    return self.terminar_mcp(&t, Saida::Erro(t!("portas-sem-porta-livre")));
                }
            },
        };
        let id = forwards.add(Forward::local(local, pedido.destino.clone(), pedido.porta));
        self.mcp_esperar_tunel(t, sessao, id, pedido);
    }

    fn mcp_encaminhar_por_rele(self: &Rc<Self>, sessao: Session, mut pedido: Pedido) -> Saida {
        if let Some((local, host_http, https)) = self.rele_para(
            sessao,
            &pedido.destino,
            pedido.porta,
            &pedido.host_http,
            pedido.https,
        ) {
            pedido.host_http = host_http;
            pedido.https = https;
            return self.mcp_concluir(sessao, local, &pedido);
        }
        let alvo = AlvoDoRele {
            destino: &pedido.destino,
            porta: pedido.porta,
            host_http: &pedido.host_http,
            https: pedido.https,
        };
        match self.abrir_rele(sessao, alvo, pedido.local, pedido.porta) {
            Ok(local) => self.mcp_concluir(sessao, local, &pedido),
            Err(erro) => Saida::Erro(erro),
        }
    }

    /// Confere o túnel a cada 50 ms até ele escutar ou falhar. Se outro programa tomou a porta
    /// que o app achou livre, escolhe a seguinte.
    fn mcp_esperar_tunel(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        sessao: Session,
        mut id: tomni_ui::terminal::ssh::ForwardId,
        pedido: Pedido,
    ) {
        let inicio = Instant::now();
        let mut tentativas = 1_u8;
        self.passo_mcp(
            t,
            Box::new(move |tela| {
                let Some(forwards) = sessao.ssh_forwards() else {
                    return Sonda::Fim(Saida::Erro(t!("portas-sem-ssh")));
                };
                let Some(status) = forwards.list().into_iter().find(|s| s.id == id) else {
                    return Sonda::Fim(Saida::Erro(t!("portas-parou")));
                };
                match status.state.clone() {
                    ForwardState::Listening => Sonda::Fim(match porta_local_de(&status) {
                        Some(local) => tela.mcp_concluir(sessao, local, &pedido),
                        None => Saida::Erro(t!("portas-parou")),
                    }),
                    ForwardState::Failed(motivo) => {
                        let tentada = match &status.forward {
                            Forward::Local { bind, .. } => bind.port,
                            _ => 0,
                        };
                        let tomada = tentada != 0 && !porta_livre(tentada);
                        if pedido.local.is_none() && tomada && tentativas < TENTATIVAS {
                            forwards.stop(id);
                            return match escolher_porta_local(pedido.porta, porta_livre) {
                                Some(local) => {
                                    id = forwards.add(Forward::local(
                                        local,
                                        pedido.destino.clone(),
                                        pedido.porta,
                                    ));
                                    tentativas += 1;
                                    Sonda::Depois(Duration::from_millis(50))
                                }
                                None => Sonda::Fim(Saida::Erro(t!("portas-sem-porta-livre"))),
                            };
                        }
                        Sonda::Fim(Saida::Erro(motivo))
                    }
                    ForwardState::Starting if inicio.elapsed() < PRAZO_DO_TUNEL => {
                        Sonda::Depois(Duration::from_millis(50))
                    }
                    ForwardState::Starting => Sonda::Fim(Saida::Erro(t!("portas-demorou"))),
                    ForwardState::Stopped => Sonda::Fim(Saida::Erro(t!("portas-parou"))),
                }
            }),
        );
    }

    /// O túnel escuta em `local`: fixa, se pedido, e diz onde ficou.
    fn mcp_concluir(&self, sessao: Session, local: u16, pedido: &Pedido) -> Saida {
        if pedido.fixar {
            self.fixar_tunel(sessao, local, pedido);
        }
        self.portas.versao.update(|v| *v += 1);
        let mut resposta = json!({
            "porta_local": local,
            "endereco": endereco_local(local),
        });
        if pedido.usa_rele() || !pedido.host_http.trim().is_empty() {
            resposta["url"] = json!(url_local(local, "/", &pedido.host_http));
        }
        Saida::App(serde_json::to_string_pretty(&resposta).unwrap_or_default())
    }

    /// Para os túneis (`-L` e relés) que escutam na porta daqui `local`.
    fn mcp_parar(&self, sessao: Session, local: u16) -> Saida {
        let mut parou = false;
        if let Some(forwards) = sessao.ssh_forwards() {
            for status in forwards
                .list()
                .iter()
                .filter(|s| local_ativo(s) && porta_local_de(s) == Some(local))
            {
                forwards.stop(status.id);
                parou = true;
            }
        }
        let dos_reles: Vec<(String, u16)> = self
            .reles_da_sessao(sessao)
            .into_iter()
            .filter(|r| r.local == local)
            .map(|r| (r.destino, r.porta))
            .collect();
        for (destino, porta) in dos_reles {
            parou |= self.parar_reles(sessao, &destino, porta);
        }
        self.portas.versao.update(|v| *v += 1);
        if parou {
            Saida::App(t!("mcp-porta-parada"))
        } else {
            Saida::Erro(t!("mcp-porta-nao-encontrada", porta = local))
        }
    }
}

/// Os endereços em que a porta escuta, sem repetir.
fn enderecos_sem_repetir(porta: &PortaDoServidor) -> Vec<String> {
    let mut vistos: Vec<IpAddr> = Vec::new();
    for endereco in &porta.enderecos {
        if !vistos.contains(endereco) {
            vistos.push(*endereco);
        }
    }
    vistos.iter().map(ToString::to_string).collect()
}
