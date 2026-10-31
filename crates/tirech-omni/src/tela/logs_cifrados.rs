//! Os logs de sessão cifrados na tela: ligar a opção (a chave nasce no cofre) e o selo periódico
//! dos que estão sendo gravados.

use std::rc::Rc;
use std::time::Duration;

use tomni_ui::prelude::*;

use super::Tela;
use crate::{logs, t};

impl Tela {
    /// Liga o que a opção `terminal.logs.cifrar` pede da tela (seção 6.3 do plano do M53):
    ///
    /// - ao **ligar**, com o cofre trancado, abre o diálogo de senha mestra; destravado, a chave
    ///   dos logs é criada (ou a de outra máquina é adotada) e o `.pub` é gravado. Se a pessoa
    ///   desiste do diálogo, a opção volta a desligada, com um aviso;
    /// - com a opção ligada, **ao destravar** o cofre, o `.pub` ausente ou fora do cofre é
    ///   regravado.
    pub(super) fn vigiar_chave_dos_logs(self: &Rc<Self>) {
        let antes = std::cell::Cell::new(self.config.with_untracked(|c| c.terminal.logs.cifrar));
        let t = Rc::clone(self);
        effect(move || {
            let agora = t.config.with(|c| c.terminal.logs.cifrar);
            let ligou = agora && !antes.replace(agora);
            if ligou {
                untrack(|| t.ao_ligar_os_logs_cifrados());
            }
        });
        // A InfoBar das Configurações precisa saber quantos logs de texto a pasta tem: conta ao
        // ligar a opção, ao mudar a pasta e ao abrir a página.
        let t = Rc::clone(self);
        effect(move || {
            let (cifrar, _pasta) = t
                .config
                .with(|c| (c.terminal.logs.cifrar, c.terminal.logs.pasta.clone()));
            let na_pagina = t.janelas.alguma_em("configuracoes");
            if cifrar && na_pagina {
                untrack(|| t.atualizar_logs());
            }
        });
        // Destravou (o cofre mudou) com a opção ligada: a chave e o `.pub` precisam combinar.
        let t = Rc::clone(self);
        effect(move || {
            let _ = t.cofre.versao();
            untrack(|| {
                let precisa = t.config.with_untracked(|c| c.terminal.logs.cifrar)
                    && t.cofre.agora(|c| !c.bloqueado() && !logs::chave_pronta(c));
                if precisa {
                    t.garantir_chave_dos_logs();
                }
            });
        });
        // O diálogo de senha fechou e o cofre segue trancado: não dá para cifrar.
        let t = Rc::clone(self);
        effect(move || {
            if t.desbloquear.get() || t.refazer_desbloqueio.get() {
                return;
            }
            untrack(|| {
                if t.logs_esperando_o_cofre.replace(false) && t.cofre.agora(|c| c.bloqueado()) {
                    t.config.update(|c| c.terminal.logs.cifrar = false);
                    t.avisar(InfoBarSeverity::Warning, t!("log-destrave-para-cifrar"));
                }
            });
        });
    }

    fn ao_ligar_os_logs_cifrados(&self) {
        if self.cofre.agora(|c| c.bloqueado()) {
            self.logs_esperando_o_cofre.set(true);
        }
        self.com_cofre_aberto(Tela::garantir_chave_dos_logs);
    }

    /// Cria (ou adota) a chave dos logs e grava o `.pub`. Falhou: avisa e desliga a opção, para
    /// as sessões novas não abrirem sem log.
    fn garantir_chave_dos_logs(&self) {
        if self.cofre.agora(logs::chave_pronta) {
            return;
        }
        if let Err(erro) = self.cofre.mudar(logs::garantir_chave) {
            self.config.update(|c| c.terminal.logs.cifrar = false);
            self.avisar(
                InfoBarSeverity::Error,
                t!("log-nao-criou-a-chave", erro = erro.to_string()),
            );
        }
    }

    /// Liga o selo dos logs: de tempos em tempos (`terminal.logs.selar_secs`) o que está no
    /// buffer de cada log vai para o disco, e quem olha o arquivo vê o que a sessão já fez.
    pub(super) fn vigiar_logs(self: &Rc<Self>) {
        self.agendar_selo_dos_logs();
    }

    fn agendar_selo_dos_logs(self: &Rc<Self>) {
        let segundos = self
            .config
            .with_untracked(|c| c.terminal.logs.selar_secs)
            .max(1);
        let t = Rc::clone(self);
        self.alarmes
            .depois(Duration::from_secs(u64::from(segundos)), move || {
                t.selar_logs();
                t.agendar_selo_dos_logs();
            });
    }

    /// Sela o log de toda sessão que está gravando, em todas as janelas.
    pub(super) fn selar_logs(&self) {
        for janela in self.janelas.todas_agora() {
            for sessao in janela.layout.with_untracked(|l| l.sessions()) {
                if sessao.is_logging() {
                    sessao.flush_log();
                }
            }
        }
    }
}
