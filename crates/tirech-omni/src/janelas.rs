//! As janelas de terminais do app (M28): a principal e as que a pessoa abre com Janela > Nova
//! janela, arrasta uma aba para fora ou reabre de um espaço de trabalho. Cada uma tem o seu
//! layout de painéis, a sua grade e os seus painéis Conexões e Propriedades; este módulo guarda
//! a lista e o que cada janela precisa para viver depois que a tela dela some.
//!
//! O layout e a grade de uma janela secundária nascem num escopo à parte (`Scope::detached`),
//! não no da tela dela: assim a lista, que vive na janela principal, nunca guarda um signal já
//! descartado. O escopo é limpo quando a janela sai da lista.

use std::sync::{Arc, Mutex};

use tomni_ui::WindowGeometry;
use tomni_ui::prelude::*;
use tomni_ui::terminal::PaneLayout;

use crate::envio::Envio;
use crate::t;

/// O grupo de arrasto das abas: todos os `TerminalPanes` do app trocam abas entre si.
pub const GRUPO: &str = "tirech-omni";

/// O nome da janela de índice `n` (0 é a principal) nos menus que listam as janelas: "Janela 1
/// (principal)", "Janela 2"…
pub fn no_menu(n: usize) -> String {
    if n == 0 {
        t!("casca-janela-1-principal")
    } else {
        t!("casca-janela-n", n = n + 1)
    }
}

/// Quantas janelas a lista do menu Janela mostra. As que passam disso existem, mas só se chega
/// nelas pelo sistema.
pub const NO_MENU: usize = 12;

/// O tamanho de uma janela secundária que não veio de um espaço, em pixels lógicos.
pub const TAMANHO_PADRAO: (u32, u32) = (900, 600);

/// Uma janela de terminais: o que ela mostra.
#[derive(Clone, Copy, Debug)]
pub struct Janela {
    /// A janela do sistema.
    pub handle: WindowHandle,
    /// Os painéis e as abas.
    pub layout: Signal<PaneLayout>,
    /// Colunas e linhas da grade escolhida em Exibir.
    pub grade: Signal<(usize, usize)>,
    /// Para onde vai o que se digita nela e a barra de envio para vários (M42).
    pub envio: Envio,
    /// Se o Flyout "Gerar comando com IA" (M52) está aberto nesta janela.
    pub ia_comando: Signal<bool>,
    /// O Flyout do autocompletar (Ctrl+Espaço) está aberto.
    pub autocompletar: Signal<bool>,
    /// Se a pergunta "enviar ou colar o caminho local" (M45) está aberta nesta janela.
    pub colar_dialogo: Signal<bool>,
    /// Se o diálogo de aprovação de um pedido de assistente (M51) está aberto nesta janela.
    pub mcp_aprovar: Signal<bool>,
    /// Os painéis Conexões e Propriedades em volta dos terminais (na principal, também o chat
    /// e as portas), que o `DockHost` da janela mantém em dia.
    pub paineis: Signal<DockLayout>,
    /// A página da navegação que a janela mostra ("terminais", "cofre"…). Cada janela tem a sua;
    /// a da principal é a `pagina` da tela.
    pub pagina: Signal<String>,
}

/// A lista das janelas abertas, na ordem em que abriram. A primeira é a principal.
#[derive(Clone)]
pub struct Janelas {
    lista: Signal<Vec<Janela>>,
    /// Os escopos das secundárias, para limpar quando a janela fecha.
    escopos: Arc<Mutex<Vec<(WindowHandle, Scope)>>>,
}

impl Janelas {
    /// A lista só com a janela principal, que usa os signals dela.
    pub fn new(principal: Janela) -> Self {
        Self {
            lista: signal(vec![principal]),
            escopos: Arc::default(),
        }
    }

    /// A janela principal.
    pub fn principal(&self) -> Janela {
        self.lista.with_untracked(|l| l[0])
    }

    /// A janela principal, se a lista ainda existe e tem alguma: para as limpezas, que podem
    /// rodar com o app acabando.
    pub fn principal_se_houver(&self) -> Option<Janela> {
        self.lista
            .try_with_untracked(|l| l.first().copied())
            .flatten()
    }

    /// Registra, como a única janela, a que está sendo montada agora. É o que a tela de lint de
    /// uma janela secundária faz: ela é a principal da sessão de ferramentas.
    pub fn trocar_principal(&self, janela: Janela) {
        self.lista.set(vec![janela]);
    }

    /// As janelas abertas, lidas com rastreamento (para menus e efeitos).
    pub fn todas(&self) -> Vec<Janela> {
        self.lista.get()
    }

    /// As janelas abertas, sem rastrear (para botões e eventos).
    pub fn todas_agora(&self) -> Vec<Janela> {
        self.lista.get_untracked()
    }

    /// Alguma janela mostra a página `nome`? Com rastreamento.
    pub fn alguma_em(&self, nome: &str) -> bool {
        self.lista.with(|l| {
            l.iter()
                .any(|j| j.pagina.try_with(|p| p == nome).unwrap_or(false))
        })
    }

    /// Quantas janelas há, com rastreamento.
    pub fn quantas(&self) -> usize {
        self.lista.with(Vec::len)
    }

    /// Quantas janelas há, sem rastrear.
    pub fn quantas_agora(&self) -> usize {
        self.lista.with_untracked(Vec::len)
    }

    /// A janela de `handle`, se ela ainda está na lista.
    pub fn achar(&self, handle: WindowHandle) -> Option<Janela> {
        self.lista
            .with_untracked(|l| l.iter().find(|j| j.handle == handle).copied())
    }

    /// O número da janela (1 para a primeira) na lista, com rastreamento. O título e o menu
    /// Janela chamam as janelas de "Janela 1", "Janela 2"… nessa ordem.
    pub fn numero(&self, handle: WindowHandle) -> Option<usize> {
        self.lista
            .with(|l| l.iter().position(|j| j.handle == handle).map(|n| n + 1))
    }

    /// Como [`Janelas::numero`], sem rastrear.
    pub fn numero_agora(&self, handle: WindowHandle) -> Option<usize> {
        self.lista
            .with_untracked(|l| l.iter().position(|j| j.handle == handle).map(|n| n + 1))
    }

    /// Abre uma janela secundária com a grade `grade` e os painéis em `paineis` (o vazio: o
    /// padrão do `DockHost`). `tela` monta o conteúdo, já com a janela (o handle só existe
    /// quando o sistema a cria). `None` onde só cabe uma janela (o Android).
    pub fn abrir<V: View>(
        &self,
        titulo: String,
        grade: (usize, usize),
        geometria: Option<WindowGeometry>,
        paineis: DockLayout,
        tela: impl FnOnce(Janela) -> V + 'static,
    ) -> Option<Janela> {
        let escopo = Scope::detached();
        let (
            layout,
            grade_atual,
            envio,
            ia_comando,
            autocompletar,
            colar_dialogo,
            mcp_aprovar,
            paineis,
            pagina,
        ) = escopo.run(|| {
            let layout = signal(PaneLayout::with(Vec::new()));
            if grade.0 > 0 && grade.1 > 0 && grade != (1, 1) {
                layout.update(|l| l.grid(grade.0, grade.1));
            }
            (
                layout,
                signal(grade),
                Envio::novo(),
                signal(false),
                signal(false),
                signal(false),
                signal(false),
                signal(paineis),
                signal(crate::paineis::TERMINAIS.to_owned()),
            )
        });
        let (largura, altura) = geometria.map_or(TAMANHO_PADRAO, |g| g.size);
        let mut nova = open_window(move || {
            tela(Janela {
                handle: WindowHandle::current(),
                layout,
                grade: grade_atual,
                envio,
                ia_comando,
                autocompletar,
                colar_dialogo,
                mcp_aprovar,
                paineis,
                pagina,
            })
        })
        .title(titulo)
        .size(largura, altura);
        if let Some((x, y)) = geometria.and_then(|g| g.position) {
            nova = nova.position(x, y);
        }
        let Some(handle) = nova.open() else {
            escopo.clear();
            return None;
        };
        let janela = Janela {
            handle,
            layout,
            grade: grade_atual,
            envio,
            ia_comando,
            autocompletar,
            colar_dialogo,
            mcp_aprovar,
            paineis,
            pagina,
        };
        self.escopos
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((handle, escopo));
        self.lista.update(|l| l.push(janela));
        Some(janela)
    }

    /// Tira a janela da lista e libera o layout e a grade dela. Chamada quando a tela da janela
    /// é descartada; aguenta a lista já descartada (o app acabando).
    pub fn remover(&self, handle: WindowHandle) {
        let _ = self.lista.try_update(|l| l.retain(|j| j.handle != handle));
        let escopo = {
            let mut escopos = self.escopos.lock().unwrap_or_else(|e| e.into_inner());
            escopos
                .iter()
                .position(|(h, _)| *h == handle)
                .map(|n| escopos.remove(n).1)
        };
        if let Some(escopo) = escopo {
            escopo.clear();
        }
    }
}
