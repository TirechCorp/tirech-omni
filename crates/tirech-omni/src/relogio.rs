//! Esperas que voltam à thread da interface: uma thread dorme e o efeito deste módulo roda o
//! que foi pedido, no quadro seguinte. Serve à limpeza da área de transferência, ao bloqueio do
//! cofre sem uso e às tentativas de reconectar.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use tomni_ui::files::Live;
use tomni_ui::prelude::*;

type Tarefa = Box<dyn FnOnce()>;

/// A fila de esperas da tela. Crie uma vez, no escopo da tela; as cópias dividem a mesma fila.
#[derive(Clone)]
pub struct Alarmes {
    pendentes: Rc<RefCell<HashMap<u64, Tarefa>>>,
    vencidos: Live<Vec<u64>>,
    proximo: Rc<Cell<u64>>,
}

impl Alarmes {
    /// A fila, com o efeito que roda os alarmes vencidos.
    pub fn nova() -> Self {
        let alarmes = Self {
            pendentes: Rc::default(),
            vencidos: Live::new(Vec::new()),
            proximo: Rc::default(),
        };
        let pendentes = Rc::clone(&alarmes.pendentes);
        let vencidos = alarmes.vencidos.clone();
        effect(move || {
            let prontos = vencidos.get();
            if prontos.is_empty() {
                return;
            }
            untrack(|| {
                vencidos.update(Vec::clear);
                for id in prontos {
                    let tarefa = pendentes.borrow_mut().remove(&id);
                    if let Some(tarefa) = tarefa {
                        tarefa();
                    }
                }
            });
        });
        alarmes
    }

    /// Roda `f` na thread da interface depois de `espera`.
    pub fn depois(&self, espera: Duration, f: impl FnOnce() + 'static) {
        let id = self.proximo.get() + 1;
        self.proximo.set(id);
        self.pendentes.borrow_mut().insert(id, Box::new(f));
        let vencidos = self.vencidos.clone();
        thread::spawn(move || {
            thread::sleep(espera);
            vencidos.update(|v| v.push(id));
        });
    }
}
