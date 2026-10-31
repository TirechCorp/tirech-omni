//! A bandeja do sistema (M44): o ícone com o menu e a dica, esconder e mostrar as janelas, o
//! diálogo de fechar e o aviso de quando o sistema não tem bandeja. Fechar a janela principal
//! deixa o app (e o agente de chaves) na bandeja; só o "Sair" encerra.

use std::rc::Rc;

use tomni_ui::WindowHandle;
use tomni_ui::prelude::*;
use tomni_ui::tray::{self, Tray, TrayImage};

use super::Tela;
use crate::configuracoes::AoFechar;
use crate::icone;
use crate::t;

/// O ícone da bandeja: o desenho pequeno do app, rasterizado no tamanho que a bandeja usa.
const ICONE: TrayImage = TrayImage::from_svg(icone::PEQUENO);

/// Se o sistema tem bandeja agora, sem rastrear.
fn bandeja_agora() -> bool {
    untrack(tray::available)
}

/// O texto de "N chaves" no menu e na dica.
fn chaves(n: usize) -> String {
    t!("bandeja-n-chaves", n = n as i64)
}

impl Tela {
    /// Esconde todas as janelas. As sessões, as transferências e as abas seguem vivas.
    pub(super) fn esconder(&self) {
        self.so_para_pergunta.set(false);
        for janela in self.janelas.todas_agora() {
            janela.handle.hide();
        }
        self.escondido.set(true);
    }

    /// Mostra todas as janelas e dá o foco à principal.
    pub(super) fn mostrar(&self) {
        self.so_para_pergunta.set(false);
        for janela in self.janelas.todas_agora() {
            janela.handle.show();
        }
        WindowHandle::main().focus();
        self.escondido.set(false);
    }

    /// Mostra só a principal, para uma pergunta do agente. Esconde de novo quando a pergunta
    /// é respondida ([`Tela::esconder_depois_da_pergunta`]).
    pub(super) fn mostrar_para_pergunta(&self) {
        if self.escondido.get_untracked() {
            self.so_para_pergunta.set(true);
        }
        let principal = WindowHandle::main();
        principal.show();
        principal.focus();
    }

    /// Volta a esconder o app se ele só foi mostrado para uma pergunta.
    pub(super) fn esconder_depois_da_pergunta(&self) {
        if self.so_para_pergunta.replace(false) {
            self.esconder();
        }
    }

    /// Encerra o app sem perguntar nada.
    pub(super) fn sair(&self) {
        self.liberado.set(true);
        WindowHandle::main().close();
    }

    /// O que fazer com um pedido de fechar a principal. `true` encerra o app; `false` o deixa
    /// aberto, porque a janela foi escondida ou um diálogo perguntou.
    pub(super) fn principal_pode_fechar(&self) -> bool {
        // A nota que ainda espera a pausa do salvamento automático não se perde, nem a das
        // outras janelas.
        self.gravar_notas(None);
        if self.liberado.get() {
            return true;
        }
        // O sistema pediu para fechar com tudo escondido (desligar o computador, por exemplo)
        // e ninguém está olhando: encerra.
        if self.escondido.get_untracked() && !self.so_para_pergunta.get() {
            return true;
        }
        if !bandeja_agora() {
            return self.fechar_pelo_caminho_de_sempre();
        }
        let como = self.config.with_untracked(|c| c.geral.ao_fechar);
        match como {
            AoFechar::Bandeja => {
                self.esconder();
                false
            }
            AoFechar::Perguntar => {
                self.fechar_ou_bandeja.set(true);
                false
            }
            AoFechar::Fechar => self.fechar_pelo_caminho_de_sempre(),
        }
    }

    /// Cria a bandeja, esconde o app se ele abriu com `--bandeja` e confere, depois de um
    /// tempo, se o sistema tem bandeja.
    pub(super) fn ligar_bandeja(self: &Rc<Self>) {
        let (t, tt) = (Rc::clone(self), Rc::clone(self));
        let tooltip = move || {
            let bloqueado = t.cofre.ler(|c| c.bloqueado());
            let _ = t.ferramentas.agente.versao();
            if bloqueado {
                t!("bandeja-dica-cofre-bloqueado")
            } else if t.ferramentas.agente.rodando() {
                t!(
                    "bandeja-dica-agente-com",
                    chaves = chaves(t.ferramentas.agente.chaves().len())
                )
            } else {
                t!("bandeja-dica-agente-desligado")
            }
        };
        let rotulo_do_agente = {
            let t = Rc::clone(self);
            move || {
                let _ = t.ferramentas.agente.versao();
                if t.ferramentas.agente.rodando() {
                    t!(
                        "bandeja-agente-ligado",
                        chaves = chaves(t.ferramentas.agente.chaves().len())
                    )
                } else {
                    t!("bandeja-agente-desligado")
                }
            }
        };
        let agente_ligado = {
            let t = Rc::clone(self);
            move || {
                let _ = t.ferramentas.agente.versao();
                t.ferramentas.agente.rodando()
            }
        };
        let travar_habilitado = {
            let t = Rc::clone(self);
            move || t.cofre.ler(|c| !c.bloqueado() && c.tem_senha_mestra())
        };
        let (abrir, ativar, travar, agente, sair) = (
            Rc::clone(self),
            Rc::clone(self),
            Rc::clone(self),
            Rc::clone(self),
            Rc::clone(self),
        );
        Tray::new(ICONE)
            .tooltip(tooltip)
            .on_activate(move || ativar.mostrar())
            .item(
                "bandeja-abrir",
                || t!("bandeja-abrir-o-app"),
                move || abrir.mostrar(),
            )
            .item(
                "bandeja-travar",
                || t!("bandeja-travar-o-cofre"),
                move || travar.cofre.bloquear(),
            )
            .enabled(travar_habilitado)
            .separator()
            .check(
                "bandeja-agente",
                rotulo_do_agente,
                agente_ligado,
                move |ligado| agente.ferramentas.definir_ligado(ligado),
            )
            .separator()
            .item("bandeja-sair", || t!("casca-sair"), move || sair.sair())
            .show()
            .keep();

        // Aberto pelo início com o sistema e com `geral.iniciar_na_bandeja`: começa escondido. O
        // `available()` só fica verdadeiro depois que o laço da janela começa; se a bandeja não
        // aparecer, a janela volta.
        if self.ferramentas.ambiente().bandeja_ao_abrir
            && self.config.with_untracked(|c| c.geral.iniciar_na_bandeja)
        {
            self.esconder();
        }
        let espera = self.ferramentas.ambiente().espera_da_bandeja;
        let t = Rc::clone(&tt);
        self.alarmes
            .depois(espera, move || t.bandeja_conferida.set(true));
        // Conferida a bandeja, se ela some (ou nunca veio) com o app escondido, a janela volta.
        let t = Rc::clone(&tt);
        effect(move || {
            if t.bandeja_conferida.get() && !tray::available() && t.escondido.get_untracked() {
                t.mostrar();
            }
        });
    }

    /// O pedido de fechar sem bandeja (ou com "Fechar o app"): com várias janelas, pergunta se
    /// fecha tudo; senão grava o estado e encerra. `true` encerra.
    fn fechar_pelo_caminho_de_sempre(&self) -> bool {
        let (confirmar, descer) = self
            .config
            .with_untracked(|c| (c.geral.confirmar_fechar, c.redes.descer_ao_fechar));
        if confirmar && self.janelas.quantas_agora() > 1 {
            self.fechar_tudo.set(true);
            return false;
        }
        // O id das conversas do OpenCode só sai da lista dele: é a última chance de anotar.
        self.resolver_conversas(true);
        self.gravar_estado();
        self.encerrando.set(true);
        self.ferramentas.encerrar();
        if descer {
            for perfil in self.usos.em_uso() {
                crate::redes::descer_ao_fechar(&perfil);
            }
        }
        // Os motores Nebula, OpenVPN, WireGuard, Tailscale e ZeroTier morrem com o app de qualquer jeito; esperar um instante deixa os
        // `CloseTunnel` saírem, e os pares não ficam esperando o tempo de um túnel morto.
        self.motores.parar_todos();
        self.motores_vpn.parar_todos();
        self.motores_wireguard.parar_todos();
        self.motores_tailscale.parar_todos();
        self.motores_zerotier.parar_todos();
        true
    }
}

/// O "Fechar o Tirech Omni?": deixar na bandeja ou fechar.
pub(super) fn dialogo_fechar_ou_bandeja(tela: &Rc<Tela>) -> impl View + use<> {
    let aberto = tela.fechar_ou_bandeja;
    let nao_perguntar = tela.nao_perguntar;
    let texto = {
        let t = Rc::clone(tela);
        move || {
            let pageant = t.ferramentas.ambiente().windows && t.config.with(|c| c.agente.pageant);
            if pageant {
                t!("bandeja-agente-e-pageant-param")
            } else {
                t!("bandeja-agente-para")
            }
        }
    };
    let janelas = {
        let t = Rc::clone(tela);
        move || {
            let n = t.janelas.quantas();
            if n > 1 {
                t!("bandeja-ha-n-janelas", n = n as i64)
            } else {
                String::new()
            }
        }
    };
    let ha_varias = {
        let t = Rc::clone(tela);
        move || t.janelas.quantas() > 1
    };
    let gravar = |t: &Tela, como: AoFechar| {
        if t.nao_perguntar.get_untracked() {
            t.config.update(|c| c.geral.ao_fechar = como);
        }
    };
    ContentDialog::new(t!("bandeja-fechar-o-app"), aberto)
        .id("fechar-ou-bandeja-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(texto).id("fechar-ou-bandeja-texto").wrap())
                .child(
                    Text::new(janelas)
                        .id("fechar-ou-bandeja-janelas")
                        .secondary()
                        .wrap()
                        .visible(ha_varias),
                )
                .child(
                    CheckBox::bind(nao_perguntar, t!("comum-nao-perguntar-de-novo"))
                        .id("fechar-ou-bandeja-nao-perguntar"),
                ),
        )
        .primary(t!("bandeja-deixar-na-bandeja"), {
            let t = Rc::clone(tela);
            move || {
                gravar(&t, AoFechar::Bandeja);
                t.esconder();
            }
        })
        .secondary(t!("comum-fechar"), {
            let t = Rc::clone(tela);
            move || {
                gravar(&t, AoFechar::Fechar);
                t.sair();
            }
        })
        .close(t!("comum-cancelar"))
}

/// O aviso de que o sistema não mostra bandeja, uma vez só.
pub(super) fn aviso_sem_bandeja(tela: &Rc<Tela>) -> impl View + use<> {
    let (t, entendi) = (Rc::clone(tela), Rc::clone(tela));
    let visivel = move || {
        t.bandeja_conferida.get()
            && !tray::available()
            && !t.config.with(|c| c.geral.aviso_sem_bandeja)
    };
    Row::new()
        .gap(8)
        .align(Align::Center)
        .visible(visivel)
        .child(
            InfoBar::new(InfoBarSeverity::Informational)
                .id("sem-bandeja-aviso")
                .message(t!("bandeja-este-sistema-nao-mostra-icones-na"))
                .grow(1),
        )
        .child(
            Button::new(t!("bandeja-entendi"))
                .id("sem-bandeja-entendi")
                .on_click(move || entendi.config.update(|c| c.geral.aviso_sem_bandeja = true)),
        )
}
