//! A pergunta da OpenVPN embutida (M56): a senha do `auth-user-pass`, o código de verificação ou
//! o desafio do servidor. Os motores perguntam de qualquer perfil, então o diálogo é global e
//! mostra uma pergunta por vez.

use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui_openvpn::{Prompt, PromptKind};

use super::Tela;
use crate::agente::ler_contador;
use crate::cofre::Tipo;
use crate::openvpn::rotulo_da_pergunta;
use crate::t;

/// O diálogo que faz a pergunta do motor e devolve a resposta (ou o cancelamento) a ele.
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl View + use<> {
    let aberto = signal(false);
    // O perfil que pergunta e o que ele pergunta.
    let atual: Signal<Option<(u64, Prompt)>> = signal(None);
    let resposta = signal(String::new());
    let salvar = signal(false);

    // Um motor perguntou, respondeu ou desistiu: mostra a próxima pergunta, se houver.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(t.nebula_mudou);
            untrack(|| {
                let proxima = t.motores_vpn.perguntas().into_iter().next();
                let igual = |a: &Option<(u64, Prompt)>, b: &Option<(u64, Prompt)>| match (a, b) {
                    (Some((pa, qa)), Some((pb, qb))) => pa == pb && qa.id == qb.id,
                    (None, None) => true,
                    _ => false,
                };
                if igual(&atual.get_untracked(), &proxima) {
                    return;
                }
                let ha = proxima.is_some();
                atual.set(proxima);
                resposta.set(String::new());
                salvar.set(false);
                if aberto.get_untracked() != ha {
                    aberto.set(ha);
                }
            });
        });
    }
    // Fechar sem responder (Cancelar ou Esc) cancela a pergunta: a VPN para com falha.
    {
        let t = Rc::clone(tela);
        effect(move || {
            if aberto.get() {
                return;
            }
            untrack(|| {
                if let Some((perfil, pergunta)) = atual.get_untracked() {
                    atual.set(None);
                    resposta.set(String::new());
                    t.motores_vpn.responder(perfil, pergunta.id, None);
                }
            });
        });
    }

    let titulo = move || {
        atual
            .get()
            .map_or_else(String::new, |(_, p)| rotulo_da_pergunta(p.kind))
    };
    let texto = move || atual.get().map(|(_, p)| p.text).unwrap_or_default();
    let nome_do_perfil = {
        let t = Rc::clone(tela);
        move || {
            atual
                .get()
                .and_then(|(id, _)| t.tree.with(|tr| tr.perfil(id).map(|p| p.name.clone())))
                .unwrap_or_default()
        }
    };
    let e_senha = move || {
        atual
            .get()
            .is_some_and(|(_, p)| p.kind == PromptKind::Password)
    };
    let ecoa = move || atual.get().is_some_and(|(_, p)| p.echo);

    ContentDialog::new(titulo, aberto)
        .id("openvpn-pergunta-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(nome_do_perfil)
                        .id("openvpn-pergunta-rede")
                        .secondary()
                        .wrap(),
                )
                .child(Text::new(texto).id("openvpn-pergunta-texto").wrap())
                .child(
                    Edit::bind(resposta)
                        .id("openvpn-pergunta-resposta")
                        .visible(ecoa),
                )
                .child(
                    Edit::bind(resposta)
                        .id("openvpn-pergunta-resposta-oculta")
                        .password()
                        .visible(move || !ecoa()),
                )
                .child(
                    CheckBox::bind(salvar, t!("openvpn-salvar-no-cofre"))
                        .id("openvpn-pergunta-salvar")
                        .visible(e_senha),
                ),
        )
        .primary(t!("openvpn-responder"), {
            let t = Rc::clone(tela);
            move || {
                let Some((perfil, pergunta)) = atual.get_untracked() else {
                    return;
                };
                let texto = resposta.get_untracked();
                if salvar.get_untracked() && pergunta.kind == PromptKind::Password {
                    guardar_senha(&t, perfil, &texto);
                }
                atual.set(None);
                resposta.set(String::new());
                t.motores_vpn.responder(perfil, pergunta.id, Some(texto));
            }
        })
        .close(t!("comum-cancelar"))
}

/// Guarda a senha respondida no item OpenVPN do cofre que o perfil usa.
fn guardar_senha(tela: &Tela, perfil: u64, senha: &str) {
    let item = tela.tree.with_untracked(|t| {
        t.perfil(perfil)
            .and_then(|p| p.openvpn.as_ref()?.identidade)
    });
    let Some(id) = item else {
        return;
    };
    let resultado = tela.cofre.mudar(|c| {
        let Some(mut item) = c.item(id).cloned() else {
            return Ok(0);
        };
        if item.tipo != Tipo::OpenVpn {
            return Ok(0);
        }
        item.senha = senha.to_owned();
        c.salvar(item)
    });
    if let Err(erro) = resultado {
        tela.avisar(
            InfoBarSeverity::Error,
            t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
        );
    }
}
