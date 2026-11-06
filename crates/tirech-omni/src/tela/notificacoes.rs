//! As notificações do sistema: os avisos das abas (o `OSC 9` e o `OSC 777` dos programas, e o
//! "terminou" e o "espera por você" dos assistentes) viram notificação quando a pessoa não está
//! vendo a aba: o app sem o foco do sistema, outra janela ativa, outra página ou a aba escondida
//! atrás de outra. O clique traz a aba.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tomni_ui::notification::Notification;
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::janelas::Janela;
use crate::paineis;

impl Tela {
    /// Liga o efeito que observa os avisos de todas as abas de todas as janelas.
    pub(super) fn ligar_notificacoes(self: &Rc<Self>) {
        let tela = Rc::clone(self);
        // Quantos avisos cada aba já tinha da última vez: só os novos notificam, e a aba que
        // aparece agora (aberta, ou vinda de outra janela) não notifica os antigos.
        let vistos: RefCell<HashMap<Session, u64>> = RefCell::default();
        effect(move || {
            let mut novos = Vec::new();
            let mut agora = HashMap::new();
            for janela in tela.janelas.todas() {
                for sessao in janela.layout.with(|l| l.sessions()) {
                    let n = sessao.notices();
                    let antes = vistos.borrow().get(&sessao).copied();
                    if antes.is_some_and(|antes| n > antes) {
                        novos.push((janela, sessao));
                    }
                    agora.insert(sessao, n);
                }
            }
            *vistos.borrow_mut() = agora;
            untrack(|| {
                for (janela, sessao) in novos {
                    tela.notificar(janela, sessao);
                }
            });
        });
    }

    /// Mostra a notificação do último aviso de `sessao`, se a opção dela está ligada e a pessoa
    /// não está vendo a aba.
    fn notificar(self: &Rc<Self>, janela: Janela, sessao: Session) {
        let assistente = self.e_aba_de_assistente(sessao);
        let ligado = self.config.with_untracked(|c| {
            if assistente {
                c.ia.avisar_ao_terminar
            } else {
                c.terminal.notificar_avisos
            }
        });
        if !ligado || self.aba_a_vista(janela, sessao) {
            return;
        }
        let Some(aviso) = sessao.last_notice() else {
            return;
        };
        let (titulo, corpo) = match aviso.title {
            Some(titulo) => (titulo, aviso.body),
            None => (sessao.title(), aviso.body),
        };
        let tela = Rc::clone(self);
        Notification::new(titulo)
            .body(corpo)
            .tag(sessao.key_str())
            .on_click(move || tela.trazer_aba(sessao))
            .show();
    }

    /// A pessoa está vendo a aba: o app tem o foco do sistema, a janela dela é a ativa e mostra
    /// os terminais, e a aba está à mostra num painel.
    fn aba_a_vista(&self, janela: Janela, sessao: Session) -> bool {
        app_focused().get_untracked()
            && active_window().get_untracked() == janela.handle
            && janela.pagina.with_untracked(|p| p == paineis::TERMINAIS)
            && janela
                .layout
                .with_untracked(|l| l.visible().contains(&sessao))
    }

    /// O clique na notificação: a aba à mostra, na janela dela, com o foco.
    pub(super) fn trazer_aba(&self, sessao: Session) {
        let Some((janela, _)) = self
            .sessoes_das_janelas(None)
            .into_iter()
            .find(|(_, s)| *s == sessao)
        else {
            return;
        };
        janela.layout.update(|l| l.select(sessao));
        janela.pagina.set(paineis::TERMINAIS.to_owned());
        janela.handle.show();
        janela.handle.focus();
    }
}
