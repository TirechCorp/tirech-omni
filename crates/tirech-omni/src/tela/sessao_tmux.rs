//! A sessão persistente (tmux ou screen) nas abas SSH. A parte pura (herança, nome da sessão e
//! comandos) está em [`crate::tmux`]; aqui fica o que liga à tela:
//!
//! - a linha "Sessão persistente" das propriedades da conexão e da pasta, no mesmo molde da linha
//!   "Assistentes (MCP)";
//! - a sessão de cada aba (o programa e o nome), escolhida ao abrir: a que a lista "Sessões no
//!   servidor" mandou abrir, a que o `estado.json` (ou o espaço de trabalho) guardou para a aba,
//!   senão o primeiro `tirech-<conexão>-<n>` livre. A reconexão reabre com o mesmo comando e
//!   volta à mesma sessão;
//! - a cópia de uma aba numa sessão, que roda outra sessão (senão as duas mostrariam a mesma);
//! - o fecho da aba: conforme `conexoes.tmux_ao_fechar`, pergunta, encerra a sessão no servidor
//!   (`tmux kill-session` ou `screen -X quit` por outra conexão, com a mesma autenticação) ou a
//!   deixa lá.
//!
//! Fechar o app, uma janela ou a aba pelo MCP não encerra nada: a sessão fica no servidor e a
//! restauração volta a ela.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::{Duration, Instant};

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{Tela, poe};
use crate::configuracoes::TmuxAoFechar;
use crate::modelo::{Connection, Node, SessaoDaAba, Tree};
use crate::pasta_da_aba::{PastaDaAba, pasta_atual};
use crate::t;
use crate::tmux;

/// Quanto o app espera o encerrar da sessão acabar antes de largar a conexão dele.
const LIMITE_DO_ENCERRAR: Duration = Duration::from_secs(20);

/// De quanto em quanto o app olha se o encerrar acabou.
const RITMO_DO_ENCERRAR: Duration = Duration::from_millis(250);

/// Uma aba numa sessão persistente que fechou e espera a resposta da pergunta.
struct Fechada {
    sessao: Session,
    /// O nome da conexão, para a pergunta.
    conexao: String,
    /// A sessão do servidor.
    aba: SessaoDaAba,
}

/// O estado da sessão persistente na tela.
pub(super) struct Estado {
    /// A sessão do servidor de cada aba que roda numa.
    nomes: RefCell<HashMap<Session, SessaoDaAba>>,
    /// As sessões que a restauração vai reusar, por conexão, na ordem das abas.
    restaurar: RefCell<HashMap<u64, VecDeque<SessaoDaAba>>>,
    /// As sessões que a lista "Sessões no servidor" mandou abrir, por conexão: valem mesmo
    /// com a conexão sem sessão persistente.
    forcar: RefCell<HashMap<u64, VecDeque<SessaoDaAba>>>,
    /// As abas de cada pergunta que espera vez (um "Fechar todas" pergunta uma vez só).
    fila: RefCell<VecDeque<Vec<Fechada>>>,
    /// As abas da pergunta à mostra.
    atual: RefCell<Option<Vec<Fechada>>>,
    aberto: Signal<bool>,
    texto: Signal<String>,
    lembrar: Signal<bool>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            nomes: RefCell::default(),
            restaurar: RefCell::default(),
            forcar: RefCell::default(),
            fila: RefCell::default(),
            atual: RefCell::default(),
            aberto: signal(false),
            texto: signal(String::new()),
            lembrar: signal(false),
        }
    }

    /// A sessão do servidor da aba `sessao`, se ela roda numa.
    pub(super) fn nome_de(&self, sessao: Session) -> Option<SessaoDaAba> {
        self.nomes.borrow().get(&sessao).cloned()
    }

    /// A aba aberta aqui que roda em `alvo`, se há.
    pub(super) fn aba_de(&self, alvo: &SessaoDaAba) -> Option<Session> {
        self.nomes
            .borrow()
            .iter()
            .find(|(_, s)| *s == alvo)
            .map(|(sessao, _)| *sessao)
    }

    /// Guarda as sessões que a restauração vai reusar para a conexão `id`, na ordem das abas.
    pub(super) fn reservar(&self, id: u64, aba: SessaoDaAba) {
        if tmux::nome_valido(&aba.nome) {
            self.restaurar
                .borrow_mut()
                .entry(id)
                .or_default()
                .push_back(aba);
        }
    }

    /// A próxima aba da conexão `id` abre na sessão `aba` do servidor.
    pub(super) fn forcar(&self, id: u64, aba: SessaoDaAba) {
        self.forcar
            .borrow_mut()
            .entry(id)
            .or_default()
            .push_back(aba);
    }

    /// Se `nome` já é de uma aba aberta ou está guardado para uma restauração (nos dois
    /// programas: o nome livre vale para os dois).
    fn em_uso(&self, nome: &str) -> bool {
        self.nomes.borrow().values().any(|n| n.nome == nome)
            || [&self.restaurar, &self.forcar].iter().any(|guardadas| {
                guardadas
                    .borrow()
                    .values()
                    .any(|fila| fila.iter().any(|n| n.nome == nome))
            })
    }

    /// Tira da fila `guardadas` da conexão `id` a primeira sessão que nenhuma aba aberta usa.
    fn tirar(
        &self,
        guardadas: &RefCell<HashMap<u64, VecDeque<SessaoDaAba>>>,
        id: u64,
    ) -> Option<SessaoDaAba> {
        loop {
            let proxima = guardadas
                .borrow_mut()
                .get_mut(&id)
                .and_then(VecDeque::pop_front);
            match proxima {
                Some(aba) if self.nomes.borrow().values().any(|n| *n == aba) => {}
                outra => return outra,
            }
        }
    }
}

/// As opções da linha do nó escolhido na árvore (as quatro, com um `Herdar` sem pasta se não há
/// nó).
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => tmux::opcoes(t, id),
        _ => tmux::opcoes(t, 0),
    })
}

impl Tela {
    /// Mostra na linha "Sessão persistente" a escolha do nó escolhido (conexão ou pasta).
    pub(super) fn sincronizar_persistente(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| {
            let atual = match t.find(id)? {
                Node::Connection(c) => c.persistente,
                Node::Group(g) => g.persistente,
            };
            Some(tmux::rotulo(t, id, atual))
        });
        if let Some(texto) = texto {
            poe(self.campos.persistente, texto);
        }
    }

    /// Passa a linha "Sessão persistente" para a pasta escolhida e grava. Lê o signal com
    /// rastreamento: serve a um efeito.
    pub(super) fn guardar_persistente_do_grupo(&self) {
        let texto = self.campos.persistente.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let nova = tmux::do_rotulo(&texto);
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id)
                    && g.persistente != nova
                {
                    g.persistente = nova;
                    mudou = true;
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }
}

impl Tela {
    /// A sessão do servidor para a aba nova da conexão `c`, e se ela já existia (a lista mandou
    /// abrir ou a restauração a guardou: os comandos ao conectar já rodaram nela). `None` se a
    /// aba roda o shell comum.
    pub(super) fn tmux_para_abrir(&self, c: &Connection) -> Option<(SessaoDaAba, bool)> {
        if c.id == 0 || !tmux::aceita_sessao(c) {
            return None;
        }
        if let Some(aba) = self.tmux.tirar(&self.tmux.forcar, c.id) {
            return Some((aba, true));
        }
        let programa = self.tree.with_untracked(|t| tmux::programa_de(t, c.id))?;
        match self.tmux.tirar(&self.tmux.restaurar, c.id) {
            Some(aba) => Some((aba, true)),
            None => {
                let nome = tmux::nome_livre(c.id, |n| self.tmux.em_uso(n));
                Some((SessaoDaAba::new(programa, nome), false))
            }
        }
    }

    /// A aba `sessao` roda na sessão `aba` do servidor.
    pub(super) fn lembrar_tmux(&self, sessao: Session, aba: SessaoDaAba) {
        self.tmux.nomes.borrow_mut().insert(sessao, aba);
        self.sessoes_servidor.abas_mudaram();
    }

    /// A pasta em que a aba está agora; numa aba numa sessão, a sonda pergunta ao programa.
    pub(super) fn pasta_da_aba(&self, sessao: Session) -> PastaDaAba {
        pasta_atual(sessao).no_tmux(self.tmux.nome_de(sessao))
    }

    /// Uma cópia da aba SSH `origem`, conectada de novo com a mesma autenticação. A cópia de uma
    /// aba numa sessão roda outra sessão do mesmo programa.
    pub(super) fn copia_da_aba(&self, origem: Session) -> Option<Session> {
        let da_origem = self
            .tmux
            .nome_de(origem)
            .and_then(|aba| Some((aba.programa, self.conexao_da_sessao(origem)?)));
        let Some((programa, id)) = da_origem else {
            return origem.duplicate_in(None);
        };
        let aba = SessaoDaAba::new(programa, tmux::nome_livre(id, |n| self.tmux.em_uso(n)));
        let comando = tmux::comando_da_aba(&aba);
        let nova = self.opener.duplicar_rodando(origem, Some(&comando))?;
        self.lembrar_tmux(nova, aba);
        Some(nova)
    }

    /// As abas `sessoes` fecharam (já saíram do layout): descarta cada uma e, nas do tmux, faz o
    /// que `conexoes.tmux_ao_fechar` manda com a sessão do servidor.
    pub(super) fn fechar_abas_tmux(&self, sessoes: Vec<Session>) {
        let mut do_tmux = Vec::new();
        for sessao in sessoes {
            let aba = self.tmux.nomes.borrow_mut().remove(&sessao);
            match aba {
                Some(aba) => do_tmux.push(Fechada {
                    sessao,
                    conexao: untrack(|| sessao.title()),
                    aba,
                }),
                None => sessao.dispose(),
            }
        }
        if do_tmux.is_empty() {
            return;
        }
        self.sessoes_servidor.abas_mudaram();
        match self.config.with_untracked(|c| c.conexoes.tmux_ao_fechar) {
            TmuxAoFechar::Manter => manter(do_tmux),
            TmuxAoFechar::Encerrar => self.encerrar_tmux(do_tmux),
            TmuxAoFechar::Perguntar => {
                self.tmux.fila.borrow_mut().push_back(do_tmux);
                self.proxima_pergunta_tmux();
            }
        }
    }

    /// Esquece a aba `sessao` sem mexer na sessão do servidor dela (o MCP e as abas tiradas por
    /// outro caminho).
    pub(super) fn esquecer_tmux(&self, sessao: Session) {
        if self.tmux.nomes.borrow_mut().remove(&sessao).is_some() {
            self.sessoes_servidor.abas_mudaram();
        }
    }

    /// Mostra a próxima pergunta da fila, se nenhuma está à mostra.
    fn proxima_pergunta_tmux(&self) {
        if self.tmux.atual.borrow().is_some() {
            return;
        }
        let Some(abas) = self.tmux.fila.borrow_mut().pop_front() else {
            return;
        };
        let texto = match abas.as_slice() {
            [uma] => t!(
                "tmux-fechar-texto",
                conexao = uma.conexao.as_str(),
                sessao = uma.aba.nome.as_str(),
                programa = uma.aba.programa.programa()
            ),
            varias => t!(
                "tmux-fechar-texto-varias",
                n = varias.len() as i64,
                lista = varias
                    .iter()
                    .map(|f| format!("{} ({})", f.aba.nome, f.aba.programa.programa()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        *self.tmux.atual.borrow_mut() = Some(abas);
        self.tmux.texto.set(texto);
        self.tmux.lembrar.set(false);
        self.tmux.aberto.set(true);
    }

    /// A resposta da pergunta à mostra: encerrar ou manter. Com "Lembrar a escolha", ela vira a
    /// configuração.
    fn responder_tmux(&self, encerrar: bool) {
        let Some(abas) = self.tmux.atual.borrow_mut().take() else {
            return;
        };
        if self.tmux.lembrar.get_untracked() {
            let escolha = if encerrar {
                TmuxAoFechar::Encerrar
            } else {
                TmuxAoFechar::Manter
            };
            self.config.update(|c| c.conexoes.tmux_ao_fechar = escolha);
        }
        if encerrar {
            self.encerrar_tmux(abas);
        } else {
            manter(abas);
        }
    }

    /// Encerra a sessão de cada aba: abre outra conexão ao servidor (a da aba pode ter caído)
    /// que roda o encerrar do programa, e larga a conexão quando ele acaba.
    fn encerrar_tmux(&self, abas: Vec<Fechada>) {
        for aba in abas {
            let comando = tmux::comando_encerrar(&aba.aba);
            let encerrando = self.opener.duplicar_rodando(aba.sessao, Some(&comando));
            aba.sessao.dispose();
            if let Some(encerrando) = encerrando {
                self.largar_quando_acabar(encerrando, Instant::now() + LIMITE_DO_ENCERRAR);
            }
        }
    }

    /// Descarta `sessao` quando a conexão dela acabar ou o `limite` passar.
    fn largar_quando_acabar(&self, sessao: Session, limite: Instant) {
        Session::poll_all();
        if untrack(|| sessao.closed()) || Instant::now() >= limite {
            sessao.dispose();
            return;
        }
        let eu = self.eu.clone();
        self.alarmes.depois(RITMO_DO_ENCERRAR, move || {
            if let Some(tela) = eu.upgrade() {
                tela.largar_quando_acabar(sessao, limite);
            }
        });
    }
}

/// Larga as abas e deixa as sessões no servidor.
fn manter(abas: Vec<Fechada>) {
    for aba in abas {
        aba.sessao.dispose();
    }
}

/// Liga o efeito que trata o Esc da pergunta (vale "Manter") e mostra a próxima. Chame uma vez,
/// ao montar a tela.
pub(super) fn ligar(tela: &Rc<Tela>) {
    let (eu, aberto) = (tela.eu.clone(), tela.tmux.aberto);
    effect(move || {
        if aberto.get() {
            return;
        }
        if let Some(t) = eu.upgrade() {
            untrack(|| {
                t.responder_tmux(false);
                t.proxima_pergunta_tmux();
            });
        }
    });
}

/// A pergunta ao fechar uma aba numa sessão persistente: "Encerrar a sessão" ou "Manter no servidor", com
/// "Lembrar a escolha". Montada na janela principal.
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl View + use<> {
    let (aberto, texto, lembrar) = (tela.tmux.aberto, tela.tmux.texto, tela.tmux.lembrar);
    ContentDialog::new(t!("tmux-fechar-titulo"), aberto)
        .id("tmux-fechar-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || texto.get())
                        .id("tmux-fechar-texto")
                        .wrap(),
                )
                .child(
                    CheckBox::bind(lembrar, t!("tmux-fechar-lembrar")).id("tmux-fechar-lembrar"),
                ),
        )
        .primary(t!("tmux-fechar-encerrar"), {
            let t = Rc::clone(tela);
            move || t.responder_tmux(true)
        })
        .secondary(t!("tmux-fechar-manter"), {
            let t = Rc::clone(tela);
            move || t.responder_tmux(false)
        })
}
