//! Exportar e importar Markdown na página Notas (M47, 5.8).
//!
//! A exportação decifra os anexos em fluxo, então roda na fila de operações (com o andamento e
//! o Cancelar) sobre uma cópia do cofre; a importação muda o cofre, então roda na thread da
//! interface.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

use tomni_ui::dialogs;
use tomni_ui::prelude::*;

use super::Pagina;
use crate::anexos::{ErroAnexo, Progresso};
use crate::cofre::Cofre;
use crate::notas::{self, Exportacao, Importacao};
use crate::t;

/// O que a exportação grava.
#[derive(Clone, Copy)]
enum Alvo {
    Nota(u64),
    Pasta(Option<u64>),
}

/// `notas::importar_arquivo` ou `notas::importar_pasta`.
type Importador = fn(
    &mut Cofre,
    &Path,
    Option<u64>,
    &dyn Fn(Progresso),
    &AtomicBool,
) -> Result<Importacao, ErroAnexo>;

/// As extensões que a importação lê.
const EXTENSOES: [&str; 3] = ["md", "markdown", "txt"];

impl Pagina {
    /// "Exportar como Markdown…" da nota `id`: pede a pasta e grava `<título>.md` (e os anexos)
    /// lá dentro.
    pub(super) fn exportar_nota(self: &Rc<Self>, id: u64) {
        self.pedir_destino(Alvo::Nota(id), t!("transferencia-exportar-como-markdown"));
    }

    /// "Exportar esta pasta para Markdown…": `None` é o cofre inteiro (as notas dele).
    pub(super) fn exportar_pasta(self: &Rc<Self>, pasta: Option<u64>) {
        self.pedir_destino(
            Alvo::Pasta(pasta),
            t!("transferencia-exportar-para-markdown"),
        );
    }

    fn pedir_destino(self: &Rc<Self>, alvo: Alvo, titulo: String) {
        if self.carregada.get().is_none() && matches!(alvo, Alvo::Nota(_)) {
            return;
        }
        // O que está escrito e ainda não foi salvo entra no arquivo.
        self.salvar_agora();
        let pagina = Rc::clone(self);
        dialogs::pick_folder()
            .remember("notas-exportar")
            .title(titulo)
            .show(move |destino| {
                if let Some(destino) = destino {
                    pagina.exportar(alvo, destino);
                }
            });
    }

    fn exportar(self: &Rc<Self>, alvo: Alvo, destino: PathBuf) {
        let copia = self.cofre.agora(Clone::clone);
        let (rotulo, pasta) = match alvo {
            Alvo::Nota(id) => (
                match copia.item(id) {
                    Some(i) => t!("transferencia-exportando", nome = i.titulo.as_str()),
                    None => t!(
                        "transferencia-exportando",
                        nome = t!("transferencia-a-nota")
                    ),
                },
                false,
            ),
            Alvo::Pasta(_) => (t!("transferencia-exportando-as-notas"), true),
        };
        let fraca = Rc::downgrade(self);
        self.fila.iniciar(
            rotulo,
            move |progresso, cancelado| -> Result<Exportacao, ErroAnexo> {
                match alvo {
                    Alvo::Nota(id) => {
                        notas::exportar_nota(&copia, id, &destino, &progresso, cancelado)
                    }
                    Alvo::Pasta(p) => {
                        notas::exportar_pasta(&copia, p, &destino, &progresso, cancelado)
                    }
                }
            },
            move |resultado| {
                let Some(pagina) = fraca.upgrade() else {
                    return;
                };
                match resultado {
                    Ok(exportacao) => pagina.informar(exportacao.mensagem(pasta)),
                    Err(ErroAnexo::Cancelado) => {
                        pagina.informar(t!("transferencia-exportacao-cancelada"))
                    }
                    Err(erro) => pagina.falhar(t!(
                        "transferencia-nao-foi-possivel-exportar",
                        erro = erro.to_string()
                    )),
                }
            },
        );
    }

    /// "Importar arquivo Markdown…": a nota entra em `destino`.
    pub(super) fn importar_arquivo(self: &Rc<Self>, destino: Option<u64>) {
        let pagina = Rc::clone(self);
        dialogs::open_file()
            .remember("notas-importar")
            .title(t!("anexos-importar-arquivo-markdown"))
            .filter("Markdown", &EXTENSOES)
            .show(move |arquivo| {
                if let Some(arquivo) = arquivo {
                    pagina.importar(destino, &arquivo, notas::importar_arquivo);
                }
            });
    }

    /// "Importar pasta de Markdown…": as subpastas e as notas entram em `destino`.
    pub(super) fn importar_pasta(self: &Rc<Self>, destino: Option<u64>) {
        let pagina = Rc::clone(self);
        dialogs::pick_folder()
            .remember("notas-importar")
            .title(t!("anexos-importar-pasta-de-markdown"))
            .show(move |pasta| {
                if let Some(pasta) = pasta {
                    pagina.importar(destino, &pasta, notas::importar_pasta);
                }
            });
    }

    fn importar(&self, destino: Option<u64>, origem: &Path, importar: Importador) {
        self.salvar_agora();
        self.filtro.set(String::new());
        let nunca = AtomicBool::new(false);
        let resultado = self
            .cofre
            .mudar(|c| importar(c, origem, destino, &|_| {}, &nunca));
        match resultado {
            Ok(importacao) => {
                // `carregar` abre a nota na árvore e limpa os avisos: o aviso vem depois.
                if let Some(&primeira) = importacao.notas.first() {
                    self.carregar(primeira);
                }
                self.informar(importacao.mensagem());
            }
            Err(erro) => self.falhar(t!(
                "transferencia-nao-foi-possivel-importar",
                erro = erro.to_string()
            )),
        }
    }
}
