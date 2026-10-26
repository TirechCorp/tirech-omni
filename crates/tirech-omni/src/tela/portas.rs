//! O painel "Portas" (M58): o que escuta no servidor da aba SSH ativa, lido por uma thread por
//! sessão (`portas::descobrir::Trabalhador`). Aqui ficam o estado, o ciclo de leitura e a vista;
//! o que fazer com cada porta (encaminhar, abrir, fixar) vem nas fatias seguintes.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

mod acoes;
mod mcp;
mod reles;

use self::acoes::Alvo;
use super::Tela;
use crate::agente::{acordar, ler_contador};
use crate::busca::normalizar;
use crate::portas::descobrir::{AbridorCompartilhado, Evento, Trabalhador};
use crate::portas::{Conteiner, Descoberta, Fonte, PortaDoServidor, Servico, Tipo};
use crate::t;

/// O que o painel sabe de uma sessão SSH.
struct DaSessao {
    sessao: Session,
    trabalhador: Trabalhador,
    /// Os canais da sessão de agora: a tela renova antes de cada leitura, porque depois de um
    /// reinício o abridor antigo só devolve erro.
    abridor: AbridorCompartilhado,
    descoberta: Option<Descoberta>,
    erro: Option<String>,
    lida_em: Option<Instant>,
    pedida_em: Option<Instant>,
    carregando: bool,
    /// Os destinos já vistos nesta sessão (`None` até a primeira leitura): porta que aparece
    /// depois e nunca foi vista é "nova" (9.3 do plano).
    vistas: Option<HashSet<(String, u16)>>,
}

impl DaSessao {
    /// Anota as portas de `lista` e devolve o texto das que são novas: as que surgem depois da
    /// primeira leitura, sem as do sistema. A mesma porta só conta uma vez.
    fn portas_novas(&mut self, lista: &Descoberta) -> Vec<String> {
        let primeira = self.vistas.is_none();
        let vistas = self.vistas.get_or_insert_default();
        let mut novas = Vec::new();
        for porta in &lista.portas {
            if vistas.insert((porta.destino.clone(), porta.porta))
                && !primeira
                && !porta.de_sistema()
            {
                novas.push(match &porta.processo {
                    Some(processo) => format!("{} ({})", porta.porta, processo.nome),
                    None => porta.porta.to_string(),
                });
            }
        }
        novas
    }
}

/// Um aviso de porta nova na parte de cima dos terminais.
#[derive(Clone)]
struct AvisoDePortaNova {
    id: u64,
    sessao: Session,
    texto: String,
}

/// O estado do painel de portas.
pub(super) struct EstadoPortas {
    /// A chave da aba mostrada (vazia: nenhuma, ou não é SSH).
    aba: Signal<String>,
    /// Sobe a cada mudança dos dados de qualquer sessão.
    versao: Signal<u64>,
    /// Sobe a cada segundo enquanto há leitores: o "atualizado há n s".
    relogio: Signal<u64>,
    /// O contador que as threads de leitura acordam.
    pronto: Signal<u64>,
    filtro: Signal<String>,
    /// O botão "Mostrar todas" (espelha `portas.mostrar_sistema`).
    todas: Signal<bool>,
    sessoes: RefCell<HashMap<String, DaSessao>>,
    /// Se o relógio de leituras já está rodando.
    vigiando: Cell<bool>,
    /// O diálogo "Encaminhar destino…".
    dialogo: acoes::EstadoDialogo,
    /// Os avisos de porta nova na tela.
    novas: Signal<Vec<AvisoDePortaNova>>,
    proximo_aviso: Cell<u64>,
    /// Os túneis que passam pelo relé do app (nome do site ou HTTPS).
    reles: RefCell<Vec<reles::ReleAberto>>,
    /// Se a conferência periódica das sessões dos relés já está agendada.
    podando: Cell<bool>,
}

impl EstadoPortas {
    pub(super) fn novo() -> Self {
        Self {
            aba: signal(String::new()),
            versao: signal(0),
            relogio: signal(0),
            pronto: signal(0),
            filtro: signal(String::new()),
            todas: signal(false),
            sessoes: RefCell::default(),
            vigiando: Cell::new(false),
            dialogo: acoes::EstadoDialogo::novo(),
            novas: signal(Vec::new()),
            proximo_aviso: Cell::new(0),
            reles: RefCell::default(),
            podando: Cell::new(false),
        }
    }
}

/// Uma linha da lista "No servidor", já com os textos.
#[derive(Clone)]
struct Linha {
    id: String,
    /// Muda quando qualquer coisa da linha muda: é a chave do `For`.
    chave: String,
    icone: Symbol,
    nome: String,
    porta: u16,
    so_no_servidor: bool,
    detalhe: String,
    alvo: Alvo,
}

/// Um contêiner da lista, com as portas dele em texto.
#[derive(Clone)]
struct LinhaConteiner {
    id: String,
    cabecalho: String,
    portas: Vec<PortaDeConteiner>,
}

/// Uma porta de contêiner: o texto e o destino que o botão encaminha.
#[derive(Clone)]
struct PortaDeConteiner {
    id: String,
    texto: String,
    alvo: Alvo,
}

fn icone_do_tipo(porta: &PortaDoServidor) -> Symbol {
    match &porta.servico {
        Servico::Web { .. } => Symbol::Globe,
        Servico::Conhecido { tipo, .. } => match tipo {
            Tipo::Web | Tipo::WebHttps | Tipo::Painel => Symbol::Globe,
            Tipo::Banco => Symbol::Database,
            Tipo::Fila => Symbol::Box,
            Tipo::Sistema | Tipo::Outro => Symbol::Server,
        },
        Servico::Desconhecido if porta.conteiner.is_some() => Symbol::Box,
        Servico::Desconhecido => Symbol::PlugConnected,
    }
}

/// O nome que a lista dá à porta: o app que a sonda achou, o nome do catálogo (com a versão) ou
/// "Web — título".
fn nome_da_porta(porta: &PortaDoServidor) -> String {
    if let Some(atalho) = porta.atalhos.first() {
        return atalho.nome.clone();
    }
    match &porta.servico {
        Servico::Web {
            titulo: Some(titulo),
            ..
        } if !titulo.trim().is_empty() => t!("portas-nome-web-titulo", titulo = titulo.trim()),
        Servico::Web { .. } => t!("portas-nome-web"),
        Servico::Conhecido { nome, versao, .. } => match versao {
            Some(versao) => format!("{nome} {versao}"),
            None => nome.clone(),
        },
        Servico::Desconhecido => porta
            .processo
            .as_ref()
            .map_or_else(|| t!("portas-nome-desconhecido"), |p| p.nome.clone()),
    }
}

/// A segunda linha: o processo, o usuário, os endereços e o contêiner.
fn detalhe_da_porta(porta: &PortaDoServidor) -> String {
    let mut partes: Vec<String> = Vec::new();
    if let Some(processo) = &porta.processo {
        partes.push(match processo.pid {
            Some(pid) => format!("{} ({pid})", processo.nome),
            None => processo.nome.clone(),
        });
    }
    if let Some(usuario) = &porta.usuario {
        partes.push(usuario.clone());
    }
    let mut enderecos: Vec<String> = Vec::new();
    for endereco in &porta.enderecos {
        let texto = endereco.to_string();
        if !enderecos.contains(&texto) {
            enderecos.push(texto);
        }
    }
    if !enderecos.is_empty() {
        partes.push(enderecos.join(", "));
    }
    if let Some(conteiner) = &porta.conteiner {
        partes.push(t!("portas-no-conteiner", nome = conteiner.as_str()));
    }
    partes.join(" · ")
}

/// Tudo o que o filtro olha, sem maiúsculas nem acentos.
fn texto_de_busca(porta: &PortaDoServidor) -> String {
    let mut texto = format!(
        "{} {} {}",
        porta.porta,
        nome_da_porta(porta),
        detalhe_da_porta(porta)
    );
    for atalho in &porta.atalhos {
        texto.push(' ');
        texto.push_str(&atalho.nome);
    }
    normalizar(&texto)
}

fn nome_da_fonte(fonte: Fonte) -> &'static str {
    match fonte {
        Fonte::Ss => "ss",
        Fonte::Proc => "/proc",
        Fonte::Netstat => "netstat",
        Fonte::Nenhuma => "—",
    }
}

fn linha_da_porta(porta: &PortaDoServidor, repetida: usize) -> Linha {
    let id = if repetida == 0 {
        format!("portas-srv-{}", porta.porta)
    } else {
        format!("portas-srv-{}-{repetida}", porta.porta)
    };
    let nome = nome_da_porta(porta);
    Linha {
        chave: format!("{id}|{porta:?}"),
        id,
        icone: icone_do_tipo(porta),
        alvo: Alvo::da_porta(porta, nome.clone()),
        nome,
        porta: porta.porta,
        so_no_servidor: porta.alcance == crate::portas::Alcance::SoNoServidor,
        detalhe: detalhe_da_porta(porta),
    }
}

fn linha_do_conteiner(conteiner: &Conteiner) -> LinhaConteiner {
    let cabecalho = match conteiner.ip() {
        Some(ip) => format!("{} · {ip}", conteiner.nome),
        None => conteiner.nome.clone(),
    };
    let base = format!("portas-ctr-{}", conteiner.nome);
    let portas = conteiner
        .portas
        .iter()
        .map(|(dentro, fora)| {
            let texto = match fora {
                Some(fora) => t!("portas-ctr-publicada", dentro = *dentro, fora = *fora),
                None => t!("portas-ctr-interna", dentro = *dentro),
            };
            PortaDeConteiner {
                id: format!("{base}-{dentro}"),
                texto,
                alvo: Alvo::de_conteiner(conteiner, *dentro, *fora),
            }
        })
        .collect();
    LinhaConteiner {
        id: base,
        cabecalho,
        portas,
    }
}

impl Tela {
    /// Liga o painel: a aba ativa, as entregas das threads, o painel que abre e as
    /// configurações.
    pub(super) fn ligar_portas(self: &Rc<Self>) {
        let est = &self.portas;
        est.todas
            .set(self.config.with_untracked(|c| c.portas.mostrar_sistema));
        let (t, layout) = (Rc::clone(self), self.janelas.principal().layout);
        effect(move || {
            let sessao = layout.with(|l| l.pane(l.active()).selected);
            untrack(|| t.mostrar_portas(sessao));
        });
        let (eu, pronto) = (Rc::downgrade(self), est.pronto);
        effect(move || {
            let _ = ler_contador(pronto);
            if let Some(t) = eu.upgrade() {
                untrack(|| t.receber_portas());
            }
        });
        // Abriu o painel: lê o servidor da aba ativa.
        let (t, aberto) = (
            Rc::clone(self),
            self.paineis_terminais.controle.shown("portas"),
        );
        effect(move || {
            if aberto.get() {
                untrack(|| {
                    let chave = t.portas.aba.get_untracked();
                    t.atualizar_portas_se_velha(&chave);
                });
            }
        });
        let (t, config) = (Rc::clone(self), self.config);
        effect(move || {
            let cfg = config.with(|c| c.portas.clone());
            untrack(|| {
                if t.portas.todas.get_untracked() != cfg.mostrar_sistema {
                    t.portas.todas.set(cfg.mostrar_sistema);
                }
                for d in t.portas.sessoes.borrow().values() {
                    d.trabalhador.configurar(&cfg);
                }
            });
        });
        let (t, todas) = (Rc::clone(self), est.todas);
        effect(move || {
            let valor = todas.get();
            untrack(|| {
                if t.config.with_untracked(|c| c.portas.mostrar_sistema) != valor {
                    t.config.update(|c| c.portas.mostrar_sistema = valor);
                }
            });
        });
    }

    /// A aba ativa mudou: o painel passa a mostrar as portas dela (se for SSH).
    fn mostrar_portas(self: &Rc<Self>, sessao: Option<Session>) {
        let est = &self.portas;
        let Some(sessao) = sessao.filter(|s| self.opener.sessao_tem_arquivos(*s)) else {
            est.aba.set(String::new());
            return;
        };
        let chave = sessao.key_str();
        est.aba.set(chave.clone());
        let aberto = self
            .paineis_terminais
            .controle
            .shown("portas")
            .get_untracked();
        let avisar = self.config.with_untracked(|c| c.portas.avisar_novas);
        if aberto || avisar {
            self.garantir_portas(sessao);
        }
    }

    /// Cria o leitor da sessão, se ainda não existe, e pede a primeira leitura.
    fn garantir_portas(self: &Rc<Self>, sessao: Session) {
        let chave = sessao.key_str();
        if self.portas.sessoes.borrow().contains_key(&chave) {
            return;
        }
        let cfg = self.config.with_untracked(|c| c.portas.clone());
        let abridor: AbridorCompartilhado = Arc::new(Mutex::new(sessao.ssh_channels()));
        let fonte = self.opener.fonte_de_portas(Arc::clone(&abridor), &cfg);
        let pronto = self.portas.pronto;
        let trabalhador = Trabalhador::novo(fonte, Duration::ZERO, move || acordar(pronto));
        self.portas.sessoes.borrow_mut().insert(
            chave.clone(),
            DaSessao {
                sessao,
                trabalhador,
                abridor,
                descoberta: None,
                erro: None,
                lida_em: None,
                pedida_em: None,
                carregando: false,
                vistas: None,
            },
        );
        self.atualizar_portas_de(&chave);
        self.vigiar_portas();
    }

    /// Como [`Self::atualizar_portas_de`], mas só quando a última leitura já passou de dois
    /// segundos: abrir o painel não pode ler duas vezes seguidas.
    fn atualizar_portas_se_velha(self: &Rc<Self>, chave: &str) {
        let recente = self
            .portas
            .sessoes
            .borrow()
            .get(chave)
            .and_then(|d| d.pedida_em)
            .is_some_and(|em| em.elapsed() < Duration::from_secs(2));
        if !recente {
            self.atualizar_portas_de(chave);
        }
    }

    /// Pede uma leitura agora (o botão Atualizar, o relógio).
    pub(super) fn atualizar_portas_de(self: &Rc<Self>, chave: &str) {
        if chave.is_empty() {
            return;
        }
        if !self.portas.sessoes.borrow().contains_key(chave) {
            let sessao =
                self.janelas.principal().layout.with_untracked(|l| {
                    l.pane(l.active()).selected.filter(|s| s.key_str() == chave)
                });
            match sessao {
                Some(sessao) => self.garantir_portas(sessao),
                None => return,
            }
            return;
        }
        if let Some(d) = self.portas.sessoes.borrow_mut().get_mut(chave) {
            *d.abridor.lock().unwrap_or_else(|e| e.into_inner()) = d.sessao.ssh_channels();
            if d.carregando {
                return;
            }
            d.carregando = true;
            d.pedida_em = Some(Instant::now());
            d.trabalhador.atualizar();
        }
        self.portas.versao.update(|v| *v += 1);
    }

    /// Recolhe o que as threads entregaram.
    fn receber_portas(self: &Rc<Self>) {
        let mut mudou = false;
        let avisar = self.config.with_untracked(|c| c.portas.avisar_novas);
        let mut novas: Vec<(Session, Vec<String>)> = Vec::new();
        for d in self.portas.sessoes.borrow_mut().values_mut() {
            for evento in d.trabalhador.recolher() {
                mudou = true;
                match evento {
                    Evento::Lista(Ok(lista)) => {
                        let surgiram = d.portas_novas(&lista);
                        if avisar && !surgiram.is_empty() {
                            novas.push((d.sessao, surgiram));
                        }
                        d.descoberta = Some(lista);
                        d.erro = None;
                        d.lida_em = Some(Instant::now());
                        d.carregando = false;
                    }
                    Evento::Lista(Err(erro)) => {
                        d.erro = Some(erro);
                        d.carregando = false;
                    }
                    Evento::Processo(inode, Some(dono)) => {
                        if let Some(lista) = d.descoberta.as_mut() {
                            for porta in lista.portas.iter_mut().filter(|p| p.inode == Some(inode))
                            {
                                porta.processo = Some(dono.clone());
                            }
                        }
                    }
                    Evento::Processo(_, None) => {}
                }
            }
        }
        if mudou {
            self.portas.versao.update(|v| *v += 1);
        }
        for (sessao, portas) in novas {
            self.avisar_porta_nova(sessao, &portas);
        }
    }

    /// Põe o aviso "Porta nova em {conexão}: …" em cima dos terminais; some em 10 s.
    fn avisar_porta_nova(self: &Rc<Self>, sessao: Session, portas: &[String]) {
        let conexao = self
            .conexao_da_sessao(sessao)
            .and_then(|id| {
                self.tree
                    .with_untracked(|t| t.connection(id).map(|c| c.name.clone()))
            })
            .unwrap_or_else(|| sessao.title());
        let texto = t!(
            "portas-nova",
            n = portas.len() as i64,
            conexao = conexao.as_str(),
            portas = portas.join(", ")
        );
        let id = self.portas.proximo_aviso.get() + 1;
        self.portas.proximo_aviso.set(id);
        let novas = self.portas.novas;
        // Uma leitura seguinte com porta nova troca o aviso da mesma sessão.
        novas.update(|l| {
            l.retain(|a| a.sessao != sessao);
            l.push(AvisoDePortaNova { id, sessao, texto });
        });
        self.alarmes.depois(Duration::from_secs(10), move || {
            novas.update(|l| l.retain(|a| a.id != id));
        });
    }

    /// A cada segundo: relê as sessões cujo intervalo venceu e põe o relógio do painel para
    /// andar. Para sozinho quando não há mais leitores.
    fn vigiar_portas(self: &Rc<Self>) {
        if self.portas.vigiando.replace(true) {
            return;
        }
        self.passo_das_portas();
    }

    fn passo_das_portas(self: &Rc<Self>) {
        let t = Rc::clone(self);
        self.alarmes.depois(Duration::from_secs(1), move || {
            if t.portas.sessoes.borrow().is_empty() {
                t.portas.vigiando.set(false);
                return;
            }
            let (intervalo, avisar) = t
                .config
                .with_untracked(|c| (c.portas.intervalo_secs, c.portas.avisar_novas));
            let aba = t.portas.aba.get_untracked();
            let aberto = t.paineis_terminais.controle.shown("portas").get_untracked();
            if intervalo > 0 {
                let vencidas: Vec<String> = t
                    .portas
                    .sessoes
                    .borrow()
                    .iter()
                    .filter(|(chave, d)| {
                        let olhando = avisar || (aberto && **chave == aba);
                        let desde = d.pedida_em.map_or(Duration::MAX, |a| a.elapsed());
                        olhando && !d.carregando && desde >= Duration::from_secs(intervalo.into())
                    })
                    .map(|(chave, _)| chave.clone())
                    .collect();
                for chave in vencidas {
                    t.atualizar_portas_de(&chave);
                }
            }
            if aberto {
                t.portas.relogio.update(|v| *v += 1);
            }
            t.passo_das_portas();
        });
    }

    /// A sessão fechou: para a thread de leitura dela.
    pub(super) fn esquecer_portas(&self, chave: &str) {
        let tirada = self.portas.sessoes.borrow_mut().remove(chave);
        if tirada.is_some() {
            self.portas.versao.update(|v| *v += 1);
        }
        if self.portas.aba.get_untracked() == chave {
            self.portas.aba.set(String::new());
        }
    }

    /// O item "Portas do servidor" do menu da aba.
    pub(super) fn abrir_portas_de(self: &Rc<Self>, sessao: Session) {
        self.paineis_terminais.controle.show("portas");
        self.portas.aba.set(sessao.key_str());
        self.garantir_portas(sessao);
        self.atualizar_portas_se_velha(&sessao.key_str());
    }

    /// Roda `f` com a sessão mostrada (lê os signals do painel, então vale em vista e efeito).
    fn com_portas<R>(&self, f: impl FnOnce(&DaSessao) -> R) -> Option<R> {
        let aba = self.portas.aba.get();
        let _ = self.portas.versao.get();
        self.portas.sessoes.borrow().get(&aba).map(f)
    }

    /// As portas da lista "No servidor", já filtradas.
    fn linhas_das_portas(&self) -> Vec<Linha> {
        let filtro = normalizar(self.portas.filtro.get().trim());
        let todas = self.portas.todas.get();
        self.com_portas(|d| {
            let Some(lista) = &d.descoberta else {
                return Vec::new();
            };
            let mut vistas: HashMap<u16, usize> = HashMap::new();
            lista
                .portas
                .iter()
                .filter(|p| todas || !p.de_sistema())
                .filter(|p| filtro.is_empty() || texto_de_busca(p).contains(&filtro))
                .map(|p| {
                    let n = vistas.entry(p.porta).or_insert(0);
                    let linha = linha_da_porta(p, *n);
                    *n += 1;
                    linha
                })
                .collect()
        })
        .unwrap_or_default()
    }

    fn conteineres_das_portas(&self) -> Vec<LinhaConteiner> {
        self.com_portas(|d| {
            d.descoberta
                .as_ref()
                .map(|l| l.conteineres.iter().map(linha_do_conteiner).collect())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    fn avisos_das_portas(&self) -> Vec<String> {
        self.com_portas(|d| {
            d.descoberta
                .as_ref()
                .map(|l| l.avisos.clone())
                .unwrap_or_default()
        })
        .unwrap_or_default()
    }

    /// "atualizado há n s · por ss".
    fn estado_das_portas(&self) -> String {
        let _ = self.portas.relogio.get();
        self.com_portas(|d| match (&d.descoberta, d.lida_em) {
            (Some(lista), Some(em)) => t!(
                "portas-atualizado",
                segundos = em.elapsed().as_secs() as i64,
                fonte = nome_da_fonte(lista.fonte)
            ),
            _ => String::new(),
        })
        .unwrap_or_default()
    }
}

/// O conteúdo do painel `portas`.
pub(super) fn painel(tela: &Rc<Tela>) -> impl View + use<> {
    let est = &tela.portas;
    let (aba, filtro, todas, versao) = (est.aba, est.filtro, est.todas, est.versao);

    let titulo = {
        let t = Rc::clone(tela);
        Text::new(move || {
            t.com_portas(|d| t!("portas-titulo", conexao = d.sessao.title()))
                .unwrap_or_else(|| t!("portas-painel"))
        })
        .id("portas-titulo")
        .body_strong()
        .max_lines(1)
        .grow(1)
    };
    let cabecalho = Row::new().gap(8).align(Align::Center).child(titulo).child(
        Button::icon_only(Symbol::ArrowClockwise)
            .id("portas-atualizar")
            .label(t!("portas-atualizar"))
            .tooltip(t!("portas-atualizar"))
            .visible(move || !aba.with(String::is_empty))
            .on_click({
                let t = Rc::clone(tela);
                move || {
                    let chave = t.portas.aba.get_untracked();
                    t.atualizar_portas_de(&chave);
                }
            }),
    );
    let campo_filtro = Edit::bind(filtro)
        .id("portas-filtro")
        .placeholder(t!("portas-filtrar"))
        .visible(move || !aba.with(String::is_empty));
    let status = {
        let t = Rc::clone(tela);
        Text::new(move || t.estado_das_portas())
            .id("portas-status")
            .caption()
            .secondary()
            .visible({
                let t = Rc::clone(tela);
                move || !t.estado_das_portas().is_empty()
            })
    };

    let sem_aba = Text::new(t!("portas-sem-aba"))
        .id("portas-vazio-aba")
        .secondary()
        .wrap()
        .visible(move || aba.with(String::is_empty));
    let carregando = ProgressRing::new()
        .id("portas-carregando")
        .width(32)
        .height(32)
        .visible({
            let t = Rc::clone(tela);
            move || {
                t.com_portas(|d| d.descoberta.is_none() && d.erro.is_none())
                    .unwrap_or(false)
            }
        });
    let erro = {
        let t = Rc::clone(tela);
        let ver = move || t.com_portas(|d| d.erro.clone()).flatten();
        let ver_texto = ver.clone();
        InfoBar::new(InfoBarSeverity::Warning)
            .id("portas-erro")
            .message(move || ver_texto().unwrap_or_default())
            .action(t!("comum-tentar-de-novo"), {
                let t = Rc::clone(tela);
                move || {
                    let chave = t.portas.aba.get_untracked();
                    t.atualizar_portas_de(&chave);
                }
            })
            .visible(move || ver().is_some())
    };
    let sem_portas = Text::new(t!("portas-vazio"))
        .id("portas-lista-vazia")
        .secondary()
        .wrap()
        .visible({
            let t = Rc::clone(tela);
            move || {
                t.com_portas(|d| d.descoberta.is_some()).unwrap_or(false)
                    && t.linhas_das_portas().is_empty()
            }
        });

    let no_servidor = For::each(
        {
            let t = Rc::clone(tela);
            move || t.linhas_das_portas()
        },
        |l: &Linha| l.chave.clone(),
        {
            let tela = Rc::clone(tela);
            move |l: Linha| vista_da_linha(&tela, &l)
        },
    );
    let conteineres = For::each(
        {
            let t = Rc::clone(tela);
            move || t.conteineres_das_portas()
        },
        |c: &LinhaConteiner| {
            let textos: Vec<&str> = c.portas.iter().map(|p| p.texto.as_str()).collect();
            format!("{}|{}", c.cabecalho, textos.join(","))
        },
        {
            let tela = Rc::clone(tela);
            move |c: LinhaConteiner| {
                let tela = Rc::clone(&tela);
                let aberto = signal(false);
                Expander::new(c.cabecalho.clone(), aberto)
                    .id(c.id.clone())
                    .content(
                        Column::new()
                            .gap(4)
                            .padding(Insets::symmetric(8.0, 4.0))
                            .child(For::each(
                                {
                                    let portas = c.portas.clone();
                                    move || portas.clone()
                                },
                                |p: &PortaDeConteiner| p.texto.clone(),
                                {
                                    let tela = Rc::clone(&tela);
                                    move |p: PortaDeConteiner| {
                                        vista_da_porta_de_conteiner(&tela, &p)
                                    }
                                },
                            )),
                    )
            }
        },
    );
    let avisos = For::each(
        {
            let t = Rc::clone(tela);
            move || t.avisos_das_portas()
        },
        |a: &String| a.clone(),
        |a: String| Text::new(a).caption().secondary().wrap(),
    );

    let titulo_servidor = Text::new(t!("portas-no-servidor"))
        .id("portas-no-servidor")
        .body_strong()
        .visible({
            let t = Rc::clone(tela);
            move || !t.linhas_das_portas().is_empty()
        });
    let titulo_conteineres = Text::new(t!("portas-conteineres"))
        .id("portas-conteineres")
        .body_strong()
        .visible({
            let t = Rc::clone(tela);
            move || !t.conteineres_das_portas().is_empty()
        });
    let corpo = ScrollViewer::new().id("portas-lista").grow(1).child(
        Column::new()
            .gap(8)
            .padding(Insets::symmetric(0.0, 4.0))
            .child(sem_aba)
            .child(carregando)
            .child(erro)
            .child(sem_portas)
            .child(titulo_servidor)
            .child(no_servidor)
            .child(acoes::encaminhadas(tela))
            .child(titulo_conteineres)
            .child(conteineres),
    );
    let destino = {
        let t = Rc::clone(tela);
        Button::new(t!("portas-encaminhar-destino"))
            .id("portas-destino")
            .visible(move || !aba.with(String::is_empty))
            .on_click(move || t.abrir_dialogo_destino(None))
    };
    let rodape = Column::new().gap(4).child(avisos).child(destino).child(
        ToggleButton::bind(todas, t!("portas-mostrar-todas"))
            .id("portas-todas")
            .height(44)
            .visible(move || !aba.with(String::is_empty)),
    );

    let _ = versao;
    Column::new()
        .id("portas-painel")
        .gap(8)
        .padding(Insets::symmetric(12.0, 8.0))
        .child(cabecalho)
        .child(campo_filtro)
        .child(status)
        .child(corpo)
        .child(rodape)
}

/// Os avisos de porta nova, em cima dos terminais. A ação "Ver portas" abre o painel na aba.
pub(super) fn avisos_de_porta_nova(tela: &Rc<Tela>) -> impl View + use<> {
    let (novas, tela) = (tela.portas.novas, Rc::clone(tela));
    For::each(
        move || novas.get(),
        |a: &AvisoDePortaNova| a.id,
        move |a: AvisoDePortaNova| {
            let (id, sessao, ver) = (a.id, a.sessao, Rc::clone(&tela));
            InfoBar::new(InfoBarSeverity::Informational)
                .id("portas-nova")
                .message(a.texto)
                .on_close(move || novas.update(|l| l.retain(|x| x.id != id)))
                .action(t!("portas-nova-ver"), move || {
                    novas.update(|l| l.retain(|x| x.id != id));
                    ver.abrir_portas_de(sessao);
                })
        },
    )
}

/// O diálogo "Encaminhar destino…", para a tela pendurar junto dos outros.
pub(super) fn dialogo(tela: &Rc<Tela>) -> ContentDialog {
    acoes::dialogo(tela)
}

/// A linha de uma porta: ícone, nome, porta e selo na primeira linha; o detalhe embaixo; à
/// direita, o botão principal e o menu "…" (o mesmo do clique direito).
fn vista_da_linha(tela: &Rc<Tela>, l: &Linha) -> impl View + use<> {
    let primeira = Row::new()
        .gap(8)
        .align(Align::Center)
        .child(Text::new(l.nome.clone()).body_strong().max_lines(1))
        .child(RichText::new(vec![Span::new(format!(":{}", l.porta))]).monospace())
        .child(
            Text::new(t!("portas-so-no-servidor"))
                .caption()
                .secondary()
                .max_lines(1)
                .visible(l.so_no_servidor),
        );
    let mais = Button::icon_only(Symbol::MoreHorizontal)
        .id(format!("{}-mais", l.id))
        .label(t!("portas-mais"))
        .tooltip(t!("portas-mais"))
        .flyout(acoes::menu_da_porta(tela, &l.id, &l.alvo));
    Row::new()
        .id(l.id.clone())
        .gap(12)
        .align(Align::Center)
        .padding(Insets::symmetric(4.0, 6.0))
        .child(Icon::new(l.icone).size(20).decorative())
        .child(
            Column::new().gap(2).grow(1).child(primeira).child(
                Text::new(l.detalhe.clone())
                    .caption()
                    .secondary()
                    .max_lines(1),
            ),
        )
        .child(acoes::botao_principal(
            tela,
            format!("{}-principal", l.id),
            &l.alvo,
        ))
        .child(mais)
}

/// Uma porta de contêiner dentro do expansor: o texto e o botão de encaminhar.
fn vista_da_porta_de_conteiner(tela: &Rc<Tela>, p: &PortaDeConteiner) -> impl View + use<> {
    Row::new()
        .id(p.id.clone())
        .gap(8)
        .align(Align::Center)
        .child(Text::new(p.texto.clone()).secondary().max_lines(1).grow(1))
        .child(acoes::botao_principal(
            tela,
            format!("{}-principal", p.id),
            &p.alvo,
        ))
        .child(
            Button::icon_only(Symbol::MoreHorizontal)
                .id(format!("{}-mais", p.id))
                .label(t!("portas-mais"))
                .tooltip(t!("portas-mais"))
                .flyout(acoes::menu_da_porta(tela, &p.id, &p.alvo)),
        )
}
