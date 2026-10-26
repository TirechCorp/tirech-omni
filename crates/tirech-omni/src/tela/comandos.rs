//! Os comandos ao conectar na casca (M50): liga o motor de [`crate::comandos`] às sessões. A
//! casca abre um `tap` na saída da sessão, pergunta ao motor o que fazer a cada olhada e manda
//! os bytes com `Session::send`, que não grava no log. As olhadas seguintes vêm de alarmes, só
//! enquanto a sequência roda.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use tomni_ui::prelude::*;
use tomni_ui::terminal::{OutputTap, Session, ShellQuote, shell_quote};

use super::{Tela, senha_da_credencial};
use crate::comandos::{
    Ajustes, Inicio, Motor, Observacao, Ordem, Parada, PassoPronto, comandos_efetivos, tem_terminal,
};
use crate::modelo::{AcaoDoPasso, Connection, Espera, MAX_SECS_PADRAO, OrigemDaSenha, Passo};
use crate::t;

/// O passo final de uma cópia (M64): ` cd '<pasta>'`, com um espaço na frente para o
/// `HISTCONTROL=ignorespace` deixar fora do histórico. Espera a tela parar, como os comandos.
fn passo_do_cd(pasta: &str) -> PassoPronto {
    let comando = format!(" cd {}", shell_quote(pasta, ShellQuote::Posix));
    PassoPronto {
        passo: Passo::comando(comando, MAX_SECS_PADRAO),
        senha: None,
    }
}

/// A menor espera entre duas olhadas: não deixa um alarme virar laço apertado.
const ESPERA_MINIMA: Duration = Duration::from_millis(10);

/// Uma sequência rodando numa sessão.
struct Rodando {
    motor: Motor,
    tap: OutputTap,
    geracao: u64,
}

/// O que a casca guarda dos comandos ao conectar.
pub(super) struct EstadoComandos {
    /// As sequências em andamento, por sessão.
    rodando: Rc<RefCell<HashMap<Session, Rodando>>>,
    /// Muda quando uma sequência começa, anda ou acaba: a dica da aba e o menu leem.
    versao: Signal<u64>,
    /// O número da próxima sequência: um alarme de uma sequência trocada confere o seu.
    proxima_geracao: Cell<u64>,
}

impl EstadoComandos {
    pub(super) fn novo() -> Self {
        Self {
            rodando: Rc::default(),
            versao: signal(0),
            proxima_geracao: Cell::new(0),
        }
    }

    fn mudou(&self) {
        self.versao.update(|v| *v += 1);
    }
}

impl Tela {
    /// Os ajustes do motor, das configurações.
    fn ajustes_dos_comandos(&self) -> Ajustes {
        let ms = self
            .config
            .with_untracked(|c| c.conexoes.comandos_quieto_ms);
        Ajustes {
            quieto: Duration::from_millis(u64::from(ms)),
        }
    }

    /// A senha que um passo "Enviar a senha" manda: a da conexão (a que a abertura usou, ou a
    /// do cofre) ou a de um item do cofre. `None`: não há.
    fn senha_do_passo(
        &self,
        c: &Connection,
        origem: OrigemDaSenha,
        da_abertura: Option<&str>,
    ) -> Option<String> {
        self.cofre.marcar_uso();
        match origem {
            OrigemDaSenha::Conexao => da_abertura
                .map(str::to_owned)
                .or_else(|| Some(senha_da_credencial(&self.cofre, c.credencial)))
                .filter(|s| !s.is_empty()),
            OrigemDaSenha::Cofre { id } => {
                Some(senha_da_credencial(&self.cofre, Some(id))).filter(|s| !s.is_empty())
            }
        }
    }

    /// Começa os comandos ao conectar da conexão `conexao` na `sessao`, trocando o que rodava
    /// nela. Sem passos (ou sem terminal, ou sem reconectar na reconexão), não faz nada.
    /// `da_abertura` é a senha da conexão que a abertura usou, quando a pessoa a digitou.
    pub(super) fn comecar_comandos(
        &self,
        sessao: Session,
        conexao: u64,
        inicio: Inicio,
        da_abertura: Option<&str>,
    ) {
        self.comecar_comandos_na_pasta(sessao, conexao, inicio, da_abertura, None);
    }

    /// Como [`Tela::comecar_comandos`], com um último passo: o ` cd` para `pasta`, depois do
    /// prompt e de todos os comandos da conexão (M64). Sem comandos configurados, a sequência é
    /// só esse passo.
    fn comecar_comandos_na_pasta(
        &self,
        sessao: Session,
        conexao: u64,
        inicio: Inicio,
        da_abertura: Option<&str>,
        pasta: Option<&str>,
    ) {
        let Some((passos, reconectar, c)) = self.tree.with_untracked(|t| {
            let c = t.connection(conexao)?;
            if !tem_terminal(c.kind) {
                return None;
            }
            let (passos, reconectar) = comandos_efetivos(t, conexao)
                .map(|e| (e.passos.clone(), e.reconectar))
                .unwrap_or_default();
            Some((passos, reconectar, c.clone()))
        }) else {
            return;
        };
        if (passos.is_empty() && pasta.is_none()) || (inicio == Inicio::Reconexao && !reconectar) {
            return;
        }
        let mut prontos: Vec<PassoPronto> = passos
            .into_iter()
            .map(|passo| {
                let senha = match &passo.acao {
                    AcaoDoPasso::Senha { origem, .. } => {
                        self.senha_do_passo(&c, *origem, da_abertura)
                    }
                    AcaoDoPasso::Comando { .. } => None,
                };
                PassoPronto { passo, senha }
            })
            .collect();
        prontos.extend(pasta.map(passo_do_cd));
        self.iniciar_sequencia(sessao, prontos, inicio);
    }

    /// Põe o motor para rodar `prontos` na `sessao`, trocando o que rodava nela.
    fn iniciar_sequencia(&self, sessao: Session, prontos: Vec<PassoPronto>, inicio: Inicio) {
        let geracao = self.comandos.proxima_geracao.get() + 1;
        self.comandos.proxima_geracao.set(geracao);
        let motor = Motor::novo(
            prontos,
            self.ajustes_dos_comandos(),
            Instant::now(),
            sessao.received(),
            inicio,
        );
        self.comandos.rodando.borrow_mut().insert(
            sessao,
            Rodando {
                motor,
                tap: sessao.tap(),
                geracao,
            },
        );
        self.comandos.mudou();
        self.tique_comandos(sessao, geracao);
    }

    /// A aba duplicada nasce como uma abertura: herda a conexão da `origem` (a chave da sessão
    /// leva o id dela, de onde o menu da aba e a reconexão a acham) e roda os comandos. `pasta`
    /// é a pasta da origem a digitar num `cd` no fim (SSH e Telnet); sem conexão salva, a
    /// sequência é só o `cd`.
    pub(super) fn comecar_comandos_da_copia(
        &self,
        origem: Option<Session>,
        nova: Session,
        pasta: Option<String>,
    ) {
        let Some(id) = origem.and_then(|s| self.conexao_da_sessao(s)) else {
            if let Some(pasta) = pasta {
                self.iniciar_sequencia(nova, vec![passo_do_cd(&pasta)], Inicio::Abertura);
            }
            return;
        };
        let n = self.contador.get_untracked() + 1;
        self.contador.set(n);
        nova.key(format!("{id}-{n}"));
        self.comecar_comandos_na_pasta(nova, id, Inicio::Abertura, None, pasta.as_deref());
    }

    /// Uma olhada na sessão: o motor diz o que fazer. `geracao` é a da sequência que pediu o
    /// alarme; se a sessão já tem outra, a olhada é de uma sequência velha e some.
    fn tique_comandos(&self, sessao: Session, geracao: u64) {
        let agora = Instant::now();
        let ordem = {
            let mut todas = self.comandos.rodando.borrow_mut();
            let Some(r) = todas.get_mut(&sessao).filter(|r| r.geracao == geracao) else {
                return;
            };
            let texto = r.tap.take();
            let linha = sessao
                .emulator(|e| e.line_text(e.cursor().0))
                .unwrap_or_default();
            let obs = Observacao {
                recebidos: sessao.received(),
                texto_novo: &texto,
                linha_do_cursor: &linha,
                fechada: untrack(|| sessao.closed()),
            };
            r.motor.avancar(agora, &obs)
        };
        match ordem {
            Ordem::Enviar(envio) => {
                sessao.send(&envio.bytes);
                self.comandos.mudou();
                self.tique_comandos(sessao, geracao);
            }
            Ordem::Esperar { ate } => {
                let eu = self.eu.clone();
                let espera = ate.saturating_duration_since(agora).max(ESPERA_MINIMA);
                self.alarmes.depois(espera, move || {
                    if let Some(tela) = eu.upgrade() {
                        tela.tique_comandos(sessao, geracao);
                    }
                });
            }
            Ordem::Terminou => self.acabar_comandos(sessao, None),
            Ordem::Parou(parada) => self.acabar_comandos(sessao, Some(parada)),
        }
    }

    /// Tira a sequência da sessão e, se parou por tempo ou por falta de senha, avisa.
    fn acabar_comandos(&self, sessao: Session, parada: Option<Parada>) {
        let Some(r) = self.comandos.rodando.borrow_mut().remove(&sessao) else {
            return;
        };
        self.comandos.mudou();
        let aviso = match parada {
            Some(Parada::TempoEsgotado { passo }) => {
                r.motor.passo(passo).map(|p| (passo, p, motivo_do_tempo(p)))
            }
            Some(Parada::SenhaIndisponivel { passo }) => r
                .motor
                .passo(passo)
                .map(|p| (passo, p, t!("comandos-motivo-sem-senha"))),
            _ => None,
        };
        if let Some((passo, p, motivo)) = aviso {
            self.avisar(
                InfoBarSeverity::Warning,
                t!(
                    "comandos-aviso-parou",
                    aba = sessao.title(),
                    n = passo + 1,
                    passo = resumo_do_passo(p),
                    motivo = motivo
                ),
            );
        }
    }

    /// Cancela os comandos que rodam na sessão (sem aviso).
    pub(super) fn cancelar_comandos(&self, sessao: Session) {
        let Some(mut r) = self.comandos.rodando.borrow_mut().remove(&sessao) else {
            return;
        };
        r.motor.cancelar();
        self.comandos.mudou();
    }

    /// (o passo atual, o total) se há uma sequência rodando na sessão. Reativo.
    pub(super) fn comandos_rodando(&self, sessao: Session) -> Option<(usize, usize)> {
        let _ = self.comandos.versao.get();
        self.comandos
            .rodando
            .borrow()
            .get(&sessao)
            .map(|r| r.motor.passo_atual())
    }

    /// Se a sessão tem comandos ao conectar para rodar de novo: a conexão dela abre um terminal,
    /// tem passos efetivos e a sessão está aberta. Reativo.
    pub(super) fn tem_comandos_para_rodar(&self, sessao: Session) -> bool {
        let Some(id) = self.conexao_da_sessao(sessao) else {
            return false;
        };
        let _ = self.comandos.versao.get();
        !sessao.closed()
            && self.tree.with(|t| {
                t.connection(id).is_some_and(|c| tem_terminal(c.kind))
                    && comandos_efetivos(t, id).is_some_and(|c| !c.passos.is_empty())
            })
    }

    /// Liga o efeito da reconexão: quando uma sessão passa de fechada a aberta (a reconexão
    /// sozinha e Sessão > Reconectar), os comandos rodam de novo, se a conexão pede. Chamado uma
    /// vez, ao montar a tela.
    pub(super) fn ligar_comandos(self: &Rc<Self>) {
        let (tela, janelas) = (Rc::clone(self), self.janelas.clone());
        // Se cada sessão estava fechada na última olhada.
        let fechadas: RefCell<HashMap<Session, bool>> = RefCell::default();
        effect(move || {
            let sessoes: Vec<Session> = janelas
                .todas()
                .iter()
                .flat_map(|j| j.layout.try_with(|l| l.sessions()).unwrap_or_default())
                .collect();
            let estados: Vec<(Session, bool)> = sessoes.iter().map(|s| (*s, s.closed())).collect();
            untrack(|| {
                let mut antes = fechadas.borrow_mut();
                antes.retain(|s, _| sessoes.contains(s));
                for (sessao, fechada) in estados {
                    let anterior = antes.insert(sessao, fechada);
                    match (anterior, fechada) {
                        // Reaberta: a sequência velha acaba e, se a conexão pede, uma nova começa.
                        // A aba numa sessão persistente volta à mesma sessão, onde os comandos já rodaram.
                        (Some(true), false) => {
                            tela.cancelar_comandos(sessao);
                            if tela.tmux.nome_de(sessao).is_some() {
                                continue;
                            }
                            if let Some(id) = tela.conexao_da_sessao(sessao) {
                                tela.comecar_comandos(sessao, id, Inicio::Reconexao, None);
                            }
                        }
                        // Caiu no meio: a sequência não tem mais para onde ir.
                        (Some(false), true) => tela.cancelar_comandos(sessao),
                        _ => {}
                    }
                }
            });
        });
    }
}

/// O passo numa frase: o comando, ou "Enviar a senha" (nunca a senha).
fn resumo_do_passo(passo: &Passo) -> String {
    match &passo.acao {
        AcaoDoPasso::Comando { texto } => texto.trim().to_owned(),
        AcaoDoPasso::Senha { .. } => t!("comandos-passo-senha"),
    }
}

/// Por que o passo esgotou o tempo, conforme o que ele esperava.
fn motivo_do_tempo(passo: &Passo) -> String {
    let s = i64::from(passo.max_secs);
    match (&passo.acao, &passo.espera) {
        (AcaoDoPasso::Senha { .. }, _) => t!("comandos-motivo-senha", s = s),
        (AcaoDoPasso::Comando { .. }, Espera::Texto { texto }) if !texto.trim().is_empty() => {
            t!("comandos-motivo-texto", texto = texto.as_str(), s = s)
        }
        _ => t!("comandos-motivo-tela", s = s),
    }
}
