//! Os links do terminal na casca (M49, 14.6): o rótulo e a ação do Ctrl+clique de cada aba. O
//! que cada endereço vira está em [`crate::links`]; aqui se decide quem abre.

use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::{LinkHit, Session};

use super::Tela;
use crate::links::{self, DestinoDoLink};
use crate::modelo::Kind;
use crate::t;

impl Tela {
    /// O tipo da conexão que abriu a aba `sessao`; o shell sem conexão no cofre é local.
    fn tipo_da_sessao(&self, sessao: Session) -> Kind {
        self.conexao_da_sessao(sessao)
            .and_then(|id| {
                self.tree
                    .with_untracked(|t| t.connection(id).map(|c| c.kind))
            })
            .unwrap_or(Kind::Local)
    }

    /// Para onde o link `acerto` leva, clicado na aba `sessao`.
    fn destino_do_link(&self, sessao: Session, acerto: &LinkHit) -> DestinoDoLink {
        links::destino(
            &acerto.uri,
            self.tipo_da_sessao(sessao),
            links::nome_desta_maquina(),
            self.config.with_untracked(|c| c.portas.links_pelo_tunel),
        )
    }

    /// O rótulo do item "abrir" do link no menu e na dica; `None` quando ele só copia.
    pub(super) fn rotulo_do_link(&self, sessao: Session, acerto: &LinkHit) -> Option<String> {
        links::rotulo(&self.destino_do_link(sessao, acerto))
    }

    /// O Ctrl+clique (ou "Abrir" do menu) num link da aba `sessao`.
    pub(super) fn abrir_link(self: &Rc<Self>, sessao: Session, acerto: &LinkHit) {
        match self.destino_do_link(sessao, acerto) {
            DestinoDoLink::SoCopiar => {}
            DestinoDoLink::PainelRemoto(caminho) => {
                if let Some(id) = self.conexao_da_sessao(sessao) {
                    self.abrir_arquivos_em(id, Some(caminho), None, true);
                }
            }
            DestinoDoLink::PortaDoServidor {
                host,
                porta,
                caminho,
                https,
            } => self.abrir_porta_do_servidor(sessao, &host, porta, &caminho, https),
            destino => {
                if let Err(erro) = self.opener.abrir_link(&destino) {
                    self.avisar(
                        InfoBarSeverity::Warning,
                        t!("links-nao-abriu", erro = erro.to_string()),
                    );
                }
            }
        }
    }
}
