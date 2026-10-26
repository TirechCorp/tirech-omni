//! O submenu WSL do menu Ferramentas (M42): abre uma distribuição num terminal local e salva a
//! lista como conexões. A lista vem do `wsl.exe -l -q`, lida numa thread ao iniciar e no
//! "Atualizar a lista"; as funções puras estão em [`crate::wsl`].

use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::prelude::*;

use super::Tela;
use crate::abrir::Senhas;
use crate::agente::{acordar, ler_contador};
use crate::modelo::Node;
use crate::t;
use crate::wsl::conexao_wsl;

/// Quantas distribuições o submenu mostra (os itens são reservados, como os do menu Janela).
pub(super) const NO_MENU: usize = 16;

/// O que a thread leu: as distribuições, ou o erro do `wsl.exe`.
type Lida = Result<Vec<String>, String>;

/// A lista de distribuições e a thread que a lê.
pub(super) struct Estado {
    /// As distribuições, na ordem do `wsl.exe`.
    pub(super) lista: Signal<Vec<String>>,
    /// Sobe quando a thread termina.
    pronto: Signal<u64>,
    /// O que a thread leu, à espera de a tela buscar.
    saida: Arc<Mutex<Option<Lida>>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            lista: signal(Vec::new()),
            pronto: signal(0),
            saida: Arc::default(),
        }
    }
}

impl Tela {
    /// Lê a lista de distribuições numa thread.
    pub(super) fn atualizar_wsl(&self) {
        let ler = self.opener.listar_wsl();
        let (saida, pronto) = (Arc::clone(&self.wsl.saida), self.wsl.pronto);
        std::thread::spawn(move || {
            *saida.lock().unwrap_or_else(PoisonError::into_inner) = Some(ler());
            acordar(pronto);
        });
    }

    /// Abre a distribuição `n` do submenu num terminal local.
    fn abrir_wsl(self: &Rc<Self>, n: usize) {
        let Some(nome) = self.wsl.lista.with_untracked(|l| l.get(n).cloned()) else {
            return;
        };
        let lugar = self.lugar_novo();
        self.abrir_agora(&conexao_wsl(&nome), Senhas::default(), lugar, false, false);
    }

    /// "Salvar como conexões": põe cada distribuição que ainda não está lá no grupo "WSL".
    fn salvar_wsl(&self) {
        let distros = self.wsl.lista.get_untracked();
        let mut novas = 0;
        self.tree.update(|t| {
            let grupo = t
                .nodes
                .iter()
                .find_map(|n| match n {
                    Node::Group(g) if g.name == "WSL" => Some(g.id),
                    _ => None,
                })
                .unwrap_or_else(|| t.add_group(None, "WSL"));
            let existentes: Vec<String> = t
                .connections_in(grupo)
                .into_iter()
                .map(|c| c.name.clone())
                .collect();
            for distro in &distros {
                if !existentes.contains(distro) {
                    t.add_connection(Some(grupo), conexao_wsl(distro));
                    novas += 1;
                }
            }
        });
        self.atualizar();
        self.avisar(
            InfoBarSeverity::Success,
            t!("wsl-distribuicoes-salvas", n = i64::from(novas)),
        );
    }
}

/// Lê a lista ao iniciar e guarda o que a thread entrega.
pub(super) fn ligar(tela: &Rc<Tela>) {
    let (lista, pronto, saida) = (tela.wsl.lista, tela.wsl.pronto, Arc::clone(&tela.wsl.saida));
    effect(move || {
        let _ = ler_contador(pronto);
        if let Some(lida) = saida.lock().unwrap_or_else(PoisonError::into_inner).take() {
            // Sem WSL (ou com erro) a lista fica vazia e o submenu some.
            lista.set(lida.unwrap_or_default());
        }
    });
    tela.atualizar_wsl();
}

/// O submenu "WSL": as distribuições, "Atualizar a lista" e "Salvar como conexões".
pub(super) fn submenu(tela: &Rc<Tela>) -> MenuFlyout {
    let lista = tela.wsl.lista;
    let mut menu = MenuFlyout::new().id("wsl");
    for n in 0..NO_MENU {
        let t = Rc::clone(tela);
        menu = menu
            .item(
                format!("wsl-{}", n + 1),
                move || lista.with(|l| l.get(n).cloned().unwrap_or_default()),
                move || t.abrir_wsl(n),
            )
            .icon(Symbol::WindowConsole)
            .item_visible(move || lista.with(|l| l.len() > n));
    }
    let (atualizar, salvar) = (Rc::clone(tela), Rc::clone(tela));
    menu.separator()
        .item("wsl-atualizar", t!("wsl-atualizar-a-lista"), move || {
            atualizar.atualizar_wsl();
        })
        .icon(Symbol::ArrowSync)
        .item("wsl-salvar", t!("wsl-salvar-como-conexoes"), move || {
            salvar.salvar_wsl();
        })
        .icon(Symbol::Save)
}
