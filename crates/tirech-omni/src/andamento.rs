//! Um trabalho em segundo plano com um diálogo de andamento (M47): o texto, os bytes e a barra,
//! e o Cancelar. É o que o cofre portátil e o exportar e importar do Cofre usam para mexer na
//! pasta `vault/`, que pode ter arquivos grandes.
//!
//! A thread guarda o resultado, avisa por um `Live` e o efeito de [`Andamento::nova`] roda o que a
//! tela pediu para o fim, na thread da interface (o mesmo caminho dos alarmes, `relogio.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;

use tomni_ui::files::Live;
use tomni_ui::prelude::*;

use crate::anexos::{ErroAnexo, Progresso};
use crate::arquivos::formato;
use crate::t;

/// Quando o trabalho acaba: roda na thread da interface com o resultado.
type AoFim<T> = Box<dyn FnOnce(Result<T, ErroAnexo>)>;

/// O trabalho em segundo plano e o diálogo que mostra o andamento. Fechar o diálogo enquanto a
/// thread trabalha cancela.
pub struct Andamento<T: Send + 'static> {
    aberto: Signal<bool>,
    texto: Signal<String>,
    progresso: Live<Progresso>,
    cancelado: Arc<AtomicBool>,
    /// Ligado enquanto a thread trabalha.
    rodando: Rc<Cell<bool>>,
    pronto: Live<u64>,
    resultado: Arc<Mutex<Option<Result<T, ErroAnexo>>>>,
    ao_fim: Rc<RefCell<Option<AoFim<T>>>>,
}

impl<T: Send + 'static> Andamento<T> {
    /// O andamento parado, com os efeitos que entregam o resultado e cancelam ao fechar. Crie uma
    /// vez, no escopo da tela.
    pub fn nova() -> Self {
        let andamento = Self {
            aberto: signal(false),
            texto: signal(String::new()),
            progresso: Live::detached(Progresso::default()),
            cancelado: Arc::default(),
            rodando: Rc::default(),
            pronto: Live::new(0),
            resultado: Arc::default(),
            ao_fim: Rc::default(),
        };
        {
            let (pronto, aberto, rodando) = (
                andamento.pronto.clone(),
                andamento.aberto,
                andamento.rodando.clone(),
            );
            let (resultado, ao_fim) = (
                Arc::clone(&andamento.resultado),
                Rc::clone(&andamento.ao_fim),
            );
            effect(move || {
                if pronto.get() == 0 {
                    return;
                }
                untrack(|| {
                    rodando.set(false);
                    aberto.set(false);
                    let r = resultado
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take();
                    let f = ao_fim.borrow_mut().take();
                    if let (Some(r), Some(f)) = (r, f) {
                        f(r);
                    }
                });
            });
        }
        {
            let (aberto, rodando, cancelado) = (
                andamento.aberto,
                andamento.rodando.clone(),
                Arc::clone(&andamento.cancelado),
            );
            effect(move || {
                if !aberto.get() && rodando.get() {
                    cancelado.store(true, Ordering::Relaxed);
                }
            });
        }
        andamento
    }

    /// Um gancho que cancela o trabalho em andamento, para quem guarda a ação além da tela
    /// (como o `antes_de_bloquear` do cofre).
    pub fn gancho_de_cancelar(&self) -> impl Fn() + 'static {
        let cancelado = Arc::clone(&self.cancelado);
        move || cancelado.store(true, Ordering::Relaxed)
    }

    /// O sinal de "o diálogo está aberto", para quem depende dele num efeito.
    pub fn aberto(&self) -> Signal<bool> {
        self.aberto
    }

    /// Abre o diálogo com `texto` e roda `trabalho` numa thread. Quando ele acaba, o diálogo
    /// fecha e `ao_fim` roda na thread da interface com o resultado.
    pub fn iniciar(
        &self,
        texto: impl Into<String>,
        trabalho: impl FnOnce(&dyn Fn(Progresso), &AtomicBool) -> Result<T, ErroAnexo> + Send + 'static,
        ao_fim: impl FnOnce(Result<T, ErroAnexo>) + 'static,
    ) {
        self.texto.set(texto.into());
        self.progresso.set(Progresso::default());
        self.cancelado.store(false, Ordering::Relaxed);
        self.rodando.set(true);
        *self.ao_fim.borrow_mut() = Some(Box::new(ao_fim));
        self.aberto.set(true);
        let (progresso, cancelado) = (self.progresso.clone(), Arc::clone(&self.cancelado));
        let (resultado, pronto) = (Arc::clone(&self.resultado), self.pronto.clone());
        thread::spawn(move || {
            let r = trabalho(&|p| progresso.set(p), &cancelado);
            *resultado.lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
            pronto.update(|n| *n += 1);
        });
    }

    /// O diálogo: o `titulo`, o texto, os bytes e a barra. Os ids são `<id>-dialogo`,
    /// `<id>-texto`, `<id>-progresso` e `<id>-bytes`; fechar cancela.
    pub fn dialogo(&self, titulo: String, id: &str) -> impl View + use<T> {
        let (aberto, texto) = (self.aberto, self.texto);
        let (andamento, barra) = (self.progresso.clone(), self.progresso.clone());
        ContentDialog::new(titulo, aberto)
            .id(format!("{id}-dialogo"))
            .content(
                Column::new()
                    .gap(8)
                    .child(
                        Text::new(move || texto.get())
                            .id(format!("{id}-texto"))
                            .wrap(),
                    )
                    .child(
                        ProgressBar::new()
                            .id(format!("{id}-progresso"))
                            .value(move || {
                                let p = barra.get();
                                if p.total == 0 {
                                    0.0
                                } else {
                                    p.feito as f64 / p.total as f64
                                }
                            }),
                    )
                    .child(
                        Text::new(move || {
                            let p = andamento.get();
                            t!(
                                "comum-parte-de-total",
                                parte = formato::tamanho(p.feito),
                                total = formato::tamanho(p.total)
                            )
                        })
                        .id(format!("{id}-bytes"))
                        .caption(),
                    ),
            )
            .close(t!("comum-cancelar"))
    }
}
