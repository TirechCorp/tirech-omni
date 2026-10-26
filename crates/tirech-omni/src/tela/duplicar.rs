//! Duplicar a aba na pasta em que ela está (M64). O menu da sessão, o menu da aba e o
//! Alt+Shift+D passam todos por [`Tela::duplicar`]:
//!
//! 1. descobre a pasta da origem ([`crate::pasta_da_aba`]); no SSH a sonda do servidor roda numa
//!    thread (no máximo 2 s) e a cópia só nasce quando ela volta;
//! 2. abre a cópia: o shell local nasce na pasta, o WSL ganha o `--cd`, e no SSH e no Telnet o
//!    app digita um ` cd` depois do prompt e dos comandos ao conectar (o motor do M50);
//! 3. a cópia herda a rede, os comandos ao conectar e o zoom da fonte da origem.
//!
//! Com a opção `terminal.duplicar_na_pasta` desligada, a cópia sai como antes, sem pasta.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::prelude::*;
use tomni_ui::terminal::{DuplicatePlace, Session};

use super::{Lugar, Tela};
use crate::abrir::{juntar_argumentos, programa_local};
use crate::agente::{acordar, ler_contador};
use crate::modelo::{Connection, Kind};
use crate::pasta_da_aba::{PastaDaAba, argumentos_do_wsl_na_pasta, eh_wsl, pasta_do_osc7};

/// Onde a cópia entra.
pub(super) enum Destino {
    /// Numa aba do lugar (os menus).
    Lugar(Lugar),
    /// No painel que o Alt+Shift+D abriu (a lib divide pelo lado maior).
    Atalho(DuplicatePlace),
}

/// Uma cópia que espera a sonda do servidor.
struct Pendente {
    origem: Session,
    destino: Destino,
}

/// O que as threads de sondagem entregam: a pasta (ou `None`) de cada pedido.
type Entregas = Arc<Mutex<Vec<(u64, Option<String>)>>>;

/// O estado de "Duplicar aba".
pub(super) struct Estado {
    entregas: Entregas,
    /// Sobe quando uma thread entrega.
    pronto: Signal<u64>,
    pendentes: RefCell<HashMap<u64, Pendente>>,
    proximo: Cell<u64>,
    /// As abas que rodam o `wsl.exe`, com a conexão que as abriu: a pasta delas vai no `--cd`.
    wsl: RefCell<HashMap<Session, Connection>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            entregas: Arc::default(),
            pronto: signal(0),
            pendentes: RefCell::default(),
            proximo: Cell::new(0),
            wsl: RefCell::default(),
        }
    }

    /// A aba `sessao` foi fechada.
    pub(super) fn esquecer(&self, sessao: Session) {
        self.wsl.borrow_mut().remove(&sessao);
    }
}

impl Tela {
    /// Anota a aba que `conexao` abriu, se ela roda o `wsl.exe`: é por esta conexão que a cópia
    /// dela sai com a pasta no `--cd`.
    pub(super) fn lembrar_se_wsl(&self, sessao: Session, conexao: &Connection) {
        if conexao.kind != Kind::Local {
            return;
        }
        let wsl = self.config.with_untracked(|config| {
            programa_local(conexao, config).is_some_and(|(programa, _)| eh_wsl(&programa))
        });
        if wsl {
            self.duplicar
                .wsl
                .borrow_mut()
                .insert(sessao, conexao.clone());
        }
    }

    /// Duplica `origem` na pasta em que ela está agora e põe a cópia em `destino`.
    pub(super) fn duplicar(&self, origem: Session, destino: Destino) {
        let ligado = self.config.with_untracked(|c| c.terminal.duplicar_na_pasta);
        let pasta = if ligado {
            self.pasta_da_aba(origem)
        } else {
            PastaDaAba::Pronta(None)
        };
        match pasta {
            PastaDaAba::Pronta(pasta) => self.copiar(origem, pasta, destino),
            sondar @ PastaDaAba::Sondar(..) => {
                let pedido = self.duplicar.proximo.get() + 1;
                self.duplicar.proximo.set(pedido);
                self.duplicar
                    .pendentes
                    .borrow_mut()
                    .insert(pedido, Pendente { origem, destino });
                let (entregas, pronto) =
                    (Arc::clone(&self.duplicar.entregas), self.duplicar.pronto);
                std::thread::spawn(move || {
                    // Qualquer erro é "não achei": a cópia cai no OSC 7 ou no home.
                    let pasta = sondar.sondada();
                    entregas
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((pedido, pasta));
                    acordar(pronto);
                });
            }
        }
    }

    /// Cria as cópias das sondas que voltaram.
    fn receber_pastas(&self) {
        let prontas = std::mem::take(
            &mut *self
                .duplicar
                .entregas
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for (pedido, pasta) in prontas {
            let Some(pendente) = self.duplicar.pendentes.borrow_mut().remove(&pedido) else {
                continue;
            };
            let pasta = pasta.or_else(|| pasta_do_osc7(pendente.origem));
            self.copiar(pendente.origem, pasta, pendente.destino);
        }
    }

    /// Abre a cópia de `origem` com `pasta` (se há) e a entrega a `destino`.
    fn copiar(&self, origem: Session, pasta: Option<String>, destino: Destino) {
        let wsl = self.duplicar.wsl.borrow().get(&origem).cloned();
        // A pasta que o motor dos comandos digita num `cd`: só nas cópias que conectam de novo
        // (SSH e Telnet). O shell local nasce na pasta e o WSL recebe o `--cd`.
        let mut digitar = None;
        let nova = if let (Some(conexao), Some(pasta)) = (&wsl, &pasta) {
            self.abrir_wsl_na_pasta(conexao, pasta)
        } else if origem.process_id().is_some() {
            origem.duplicate_in(pasta.as_deref().map(Path::new))
        } else {
            digitar = pasta;
            self.copia_da_aba(origem)
        };
        let Some(nova) = nova else {
            return;
        };
        if let Some(conexao) = wsl
            && !self.duplicar.wsl.borrow().contains_key(&nova)
        {
            self.duplicar.wsl.borrow_mut().insert(nova, conexao);
        }
        self.herdar_da_origem(Some(origem), nova, digitar);
        match destino {
            Destino::Lugar(lugar) => self.entra_na_grade(nova, lugar),
            Destino::Atalho(lugar) => lugar.place(nova),
        }
    }

    /// O que toda cópia herda da origem: a rede, os comandos ao conectar (e o `cd` final) e o
    /// zoom da fonte.
    pub(super) fn herdar_da_origem(
        &self,
        origem: Option<Session>,
        nova: Session,
        cd: Option<String>,
    ) {
        self.herdar_rede_da_sessao(origem, nova);
        self.comecar_comandos_da_copia(origem, nova, cd);
        if let Some(origem) = origem {
            self.zoom_fonte.herdar(origem, nova);
        }
    }

    /// Abre outra aba da distribuição do WSL de `conexao`, com a pasta no `--cd`.
    fn abrir_wsl_na_pasta(&self, conexao: &Connection, pasta: &str) -> Option<Session> {
        let (programa, argumentos) = self
            .config
            .with_untracked(|config| programa_local(conexao, config))?;
        let mut copia = conexao.clone();
        copia.local_programa = programa;
        copia.local_argumentos = argumentos_do_wsl_na_pasta(&juntar_argumentos(&argumentos), pasta);
        let n = self.contador.get_untracked() + 1;
        self.contador.set(n);
        Some(self.opener.open(
            &self.conexao_efetiva(&copia),
            crate::abrir::Senhas::default(),
            n,
        ))
    }
}

/// Liga o efeito que cria as cópias quando a sonda volta. Chame uma vez, ao montar a tela.
pub(super) fn ligar(tela: &std::rc::Rc<Tela>) {
    let (eu, pronto) = (tela.eu.clone(), tela.duplicar.pronto);
    effect(move || {
        let _ = ler_contador(pronto);
        if let Some(t) = eu.upgrade() {
            untrack(|| t.receber_pastas());
        }
    });
}
