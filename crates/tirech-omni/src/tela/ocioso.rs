//! Fechar a aba sem uso, na tela. A parte pura (herança, textos e o relógio de cada aba) está em
//! [`crate::ocioso`]; aqui ficam a linha das propriedades da conexão e da pasta, no mesmo molde
//! da linha "Tentar de novo após", e a vigia: enquanto alguma aba aberta tem um tempo, ela olha
//! as abas de tempos em tempos e fecha as que passaram dele sem nada digitado, colado ou mandado
//! por um assistente. A sessão persistente (tmux ou screen) fica no servidor.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::{Duration, Instant};

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{Tela, poe};
use crate::janelas::Janela;
use crate::modelo::{Node, Ocioso, Tree};
use crate::ocioso::{self, Uso};
use crate::retentar::tempo;
use crate::t;

/// De quanto em quanto a vigia olha as abas.
const PASSO: Duration = Duration::from_secs(15);

/// O que a tela guarda da vigia.
pub(super) struct EstadoOcioso {
    /// O último uso visto de cada aba com tempo.
    usos: RefCell<HashMap<Session, Uso>>,
    /// Se a vigia está andando.
    vigiando: Cell<bool>,
}

impl EstadoOcioso {
    pub(super) fn novo() -> Self {
        Self {
            usos: RefCell::default(),
            vigiando: Cell::new(false),
        }
    }
}

/// As opções da linha do nó escolhido na árvore (com um `Herdar` sem pasta se não há nó).
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => ocioso::opcoes(t, id, ocioso::do_no(t, id)),
        _ => ocioso::opcoes(t, 0, Ocioso::Herdar),
    })
}

impl Tela {
    /// Mostra na linha "Fechar sem uso após" a escolha do nó escolhido (conexão ou pasta).
    pub(super) fn sincronizar_ocioso(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| {
            t.find(id)?;
            Some(ocioso::rotulo(t, id, ocioso::do_no(t, id)))
        });
        if let Some(texto) = texto {
            poe(self.campos.ocioso, texto);
        }
    }

    /// Passa a linha "Fechar sem uso após" para a pasta escolhida e grava. Lê o signal com
    /// rastreamento: serve a um efeito.
    pub(super) fn guardar_ocioso_do_grupo(&self) {
        let texto = self.campos.ocioso.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id) {
                    let nova = ocioso::do_rotulo(&texto, g.ocioso);
                    if g.ocioso != nova {
                        g.ocioso = nova;
                        mudou = true;
                    }
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }

    /// Depois de quanto tempo sem uso a aba `sessao` fecha: a escolha da conexão (a dela ou a da
    /// pasta) ou, sem nenhuma, as Configurações. `None` não fecha (e a aba que não é de uma
    /// conexão guardada só segue as Configurações).
    fn limite_da_aba(&self, sessao: Session, padrao_mins: u32) -> Option<Duration> {
        let escolha = self
            .conexao_da_sessao(sessao)
            .and_then(|id| self.tree.with_untracked(|t| ocioso::da_conexao(t, id)));
        ocioso::limite(escolha, padrao_mins)
    }

    /// Acorda a vigia quando muda o que decide os tempos: as Configurações, a árvore ou as abas
    /// de alguma janela.
    pub(super) fn ligar_ociosas(&self) {
        let (eu, config, tree, janelas) = (
            self.eu.clone(),
            self.config,
            self.tree,
            self.janelas.clone(),
        );
        effect(move || {
            config.with(|c| c.conexoes.ocioso_mins);
            tree.with(|_| ());
            for janela in janelas.todas() {
                let _ = janela.layout.try_with(|l| l.sessions().len());
            }
            if let Some(t) = eu.upgrade() {
                untrack(|| t.vigiar_ociosas());
            }
        });
    }

    /// Põe a vigia para andar, se alguma aba aberta tem tempo e ela ainda não anda.
    fn vigiar_ociosas(&self) {
        if self.ociosas.vigiando.get() {
            return;
        }
        let padrao = self.config.with_untracked(|c| c.conexoes.ocioso_mins);
        let alguma = self
            .sessoes_das_janelas(None)
            .into_iter()
            .any(|(_, s)| self.limite_da_aba(s, padrao).is_some());
        if alguma {
            self.ociosas.vigiando.set(true);
            self.passo_das_ociosas();
        }
    }

    fn passo_das_ociosas(&self) {
        let eu = self.eu.clone();
        self.alarmes.depois(PASSO, move || {
            if let Some(t) = eu.upgrade() {
                untrack(|| t.olhar_ociosas());
            }
        });
    }

    /// Olha as abas, fecha as que passaram do tempo e marca a próxima olhada (ou para, se
    /// nenhuma aba tem tempo).
    fn olhar_ociosas(&self) {
        let padrao = self.config.with_untracked(|c| c.conexoes.ocioso_mins);
        let agora = Instant::now();
        let abas = self.sessoes_das_janelas(None);
        let mut alguma = false;
        let mut vencidas = Vec::new();
        {
            let mut usos = self.ociosas.usos.borrow_mut();
            usos.retain(|s, _| abas.iter().any(|(_, aberta)| aberta == s));
            for (janela, sessao) in &abas {
                let Some(limite) = self.limite_da_aba(*sessao, padrao) else {
                    usos.remove(sessao);
                    continue;
                };
                alguma = true;
                // A aba caída ou abrindo não está em uso nem parada: o relógio recomeça quando
                // ela voltar.
                if sessao.closed() || sessao.connecting() {
                    usos.remove(sessao);
                    continue;
                }
                let enviados = sessao.sent();
                let mexida = self.mexida_por_assistente(&sessao.key_str(), agora);
                let uso = usos
                    .entry(*sessao)
                    .or_insert_with(|| Uso::novo(enviados, agora));
                if uso.passou(enviados, mexida, agora, limite) {
                    vencidas.push((*janela, *sessao, limite));
                }
            }
            for (_, sessao, _) in &vencidas {
                usos.remove(sessao);
            }
        }
        if !vencidas.is_empty() {
            self.fechar_ociosas(vencidas);
        }
        if alguma {
            self.passo_das_ociosas();
        } else {
            self.ociosas.vigiando.set(false);
        }
    }

    /// Quando um assistente mexeu na aba `chave` por último (agora, se ele ainda está mexendo).
    fn mexida_por_assistente(&self, chave: &str, agora: Instant) -> Option<Instant> {
        let mapa = self.mcp.atividade.borrow();
        let atividade = mapa.get(chave)?;
        Some(if atividade.em_curso > 0 {
            agora
        } else {
            atividade.quando
        })
    }

    /// Fecha as abas vencidas (como o ✕, sem encerrar a sessão persistente) e avisa.
    fn fechar_ociosas(&self, vencidas: Vec<(Janela, Session, Duration)>) {
        for (_, sessao, limite) in &vencidas {
            let minutos = u32::try_from(limite.as_secs() / 60).unwrap_or(u32::MAX);
            self.avisar(
                InfoBarSeverity::Informational,
                t!(
                    "ocioso-fechou",
                    aba = sessao.title(),
                    tempo = tempo(minutos)
                ),
            );
        }
        let abas = vencidas.into_iter().map(|(j, s, _)| (j, s)).collect();
        for janela in self.tirar_sessoes(abas, false) {
            self.fechar_se_vazia(janela);
        }
    }
}
