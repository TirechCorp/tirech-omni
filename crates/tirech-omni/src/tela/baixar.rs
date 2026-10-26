//! Baixar o arquivo marcado no terminal: o Ctrl+clique direito com texto marcado numa aba que
//! tem arquivos (o SFTP da sessão SSH), como no WinSSHTerm, e o "Baixar o arquivo marcado…" do
//! menu do terminal. A parte pura está em [`crate::baixar`].
//!
//! O clique pede onde gravar e solta uma thread, que resolve o caminho (a pasta do shell pela
//! sonda do servidor, o OSC 7 ou a home), copia e avisa pelo [`Live`] de fim. A barra de cada
//! download fica em cima dos terminais da janela, com "Cancelar"; o fim vira um aviso.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tomni_ui::dialogs;
use tomni_ui::files::{Live, Opener as ArquivosOpener};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::arquivos::editar::caminho_em_texto;
use crate::arquivos::formato::tamanho;
use crate::baixar::{Falha, caminho_marcado, copiar, nome_do_arquivo, relativo, resolver};
use crate::janelas::Janela;
use crate::pasta_da_aba::{PastaDaAba, pasta_do_osc7};
use crate::t;

/// Quanto um download andou e como acabou.
#[derive(Clone, Debug, Default)]
struct Andou {
    feito: u64,
    total: u64,
    fim: Option<Result<(), Falha>>,
}

/// Um download em andamento.
struct Baixando {
    id: u64,
    janela: WindowHandle,
    /// O caminho como foi marcado.
    caminho: String,
    conexao: String,
    destino: PathBuf,
    andou: Live<Andou>,
    cancelar: Arc<AtomicBool>,
}

/// O estado dos downloads na tela.
pub(super) struct Estado {
    ativos: RefCell<Vec<Rc<Baixando>>>,
    /// Sobe quando a lista muda, para as barras.
    versao: Signal<u64>,
    /// Sobe quando uma thread acaba.
    fim: Live<u64>,
    proximo: Cell<u64>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            ativos: RefCell::default(),
            versao: signal(0),
            fim: Live::new(0),
            proximo: Cell::new(0),
        }
    }
}

/// O que a thread precisa.
struct Pedido {
    abrir: ArquivosOpener,
    /// Abre o disco desta máquina, onde o arquivo é gravado.
    disco: ArquivosOpener,
    caminho: String,
    pasta: PastaDaAba,
    osc7: Option<String>,
    /// O destino no `Fs` do disco.
    destino: String,
    andou: Live<Andou>,
    cancelar: Arc<AtomicBool>,
    fim: Live<u64>,
}

fn trabalhar(p: Pedido) {
    let resultado = baixar(&p);
    p.andou.update(|a| a.fim = Some(resultado.map(|_| ())));
    p.fim.update(|n| *n += 1);
}

fn baixar(p: &Pedido) -> Result<u64, Falha> {
    let mut fs = (p.abrir)()?;
    let pasta = if relativo(&p.caminho) {
        // Qualquer erro da sonda é "não achei": vale o OSC 7 ou a home.
        p.pasta.sondada().or_else(|| p.osc7.clone())
    } else {
        None
    };
    let caminho = resolver(&p.caminho, pasta.as_deref(), || fs.home())?;
    let andou = |feito, total| {
        p.andou.update(|a| {
            a.feito = feito;
            a.total = total;
        });
    };
    let mut disco = (p.disco)()?;
    copiar(
        fs.as_mut(),
        &caminho,
        disco.as_mut(),
        &p.destino,
        &p.cancelar,
        &andou,
    )
}

impl Tela {
    /// Se o Ctrl+clique direito na `sessao` baixa o que está marcado: há texto marcado e a aba
    /// tem arquivos.
    pub(super) fn pode_baixar_marcado(&self, sessao: Session) -> bool {
        sessao.has_selection() && self.opener.sessao_tem_arquivos(sessao)
    }

    /// Baixa o arquivo cujo caminho está marcado no terminal da `sessao`: pergunta onde gravar e
    /// copia numa thread.
    pub(super) fn baixar_marcado(&self, janela: Janela, sessao: Session) {
        let marcado = untrack(|| sessao.selection_text()).unwrap_or_default();
        let avisar = |texto: String| {
            self.avisar_colar(
                janela.handle,
                InfoBarSeverity::Warning,
                "baixar-aviso",
                t!("baixar-erro-titulo"),
                texto,
            );
        };
        let Some(caminho) = caminho_marcado(&marcado) else {
            return avisar(t!("baixar-marcado-invalido"));
        };
        let abrir = match self.opener.arquivos_da_sessao(sessao) {
            Ok(abrir) => abrir,
            Err(erro) => return avisar(erro),
        };
        let conexao = self
            .conexao_do_colar(sessao)
            .map(|c| c.name)
            .unwrap_or_else(|| sessao.title());
        let (pasta, osc7) = (self.pasta_da_aba(sessao), pasta_do_osc7(sessao));
        let eu = self.eu.clone();
        dialogs::save_file()
            .remember("terminal-baixar")
            .title(t!("baixar-titulo"))
            .file_name(nome_do_arquivo(&caminho).unwrap_or_default())
            .show(move |destino| {
                let (Some(destino), Some(tela)) = (destino, eu.upgrade()) else {
                    return;
                };
                tela.iniciar_download(janela, conexao, caminho, abrir, pasta, osc7, destino);
            });
    }

    #[allow(clippy::too_many_arguments)]
    fn iniciar_download(
        &self,
        janela: Janela,
        conexao: String,
        caminho: String,
        abrir: ArquivosOpener,
        pasta: PastaDaAba,
        osc7: Option<String>,
        destino: PathBuf,
    ) {
        let id = self.baixar.proximo.get() + 1;
        self.baixar.proximo.set(id);
        let baixando = Rc::new(Baixando {
            id,
            janela: janela.handle,
            caminho: caminho.clone(),
            conexao,
            destino: destino.clone(),
            andou: Live::detached(Andou::default()),
            cancelar: Arc::default(),
        });
        let pedido = Pedido {
            abrir,
            disco: self.opener.disco().opener,
            caminho,
            pasta,
            osc7,
            destino: caminho_em_texto(&destino),
            andou: baixando.andou.clone(),
            cancelar: Arc::clone(&baixando.cancelar),
            fim: self.baixar.fim.clone(),
        };
        self.baixar.ativos.borrow_mut().push(baixando);
        self.baixar.versao.update(|v| *v += 1);
        std::thread::spawn(move || trabalhar(pedido));
    }

    /// Liga a tela ao fim dos downloads: tira a barra e avisa como foi.
    pub(super) fn acompanhar_downloads(self: &Rc<Self>) {
        let (tela, fim) = (Rc::clone(self), self.baixar.fim.clone());
        effect(move || {
            fim.with(|_| ());
            untrack(|| tela.downloads_acabados());
        });
    }

    fn downloads_acabados(&self) {
        let mut acabados = Vec::new();
        self.baixar.ativos.borrow_mut().retain(|b| {
            match b.andou.with_untracked(|a| a.fim.clone()) {
                Some(fim) => {
                    acabados.push((Rc::clone(b), fim));
                    false
                }
                None => true,
            }
        });
        if acabados.is_empty() {
            return;
        }
        self.baixar.versao.update(|v| *v += 1);
        for (b, fim) in acabados {
            let (severidade, base, titulo, texto) = match fim {
                Ok(()) => (
                    InfoBarSeverity::Success,
                    "baixar-feito",
                    t!("baixar-feito-titulo"),
                    t!(
                        "baixar-feito",
                        caminho = b.caminho.as_str(),
                        destino = b.destino.display().to_string()
                    ),
                ),
                Err(Falha::Cancelada) => continue,
                Err(Falha::Pasta(pasta)) => (
                    InfoBarSeverity::Warning,
                    "baixar-erro",
                    t!("baixar-erro-titulo"),
                    t!("baixar-e-pasta", caminho = pasta),
                ),
                Err(Falha::Erro(motivo)) => (
                    InfoBarSeverity::Error,
                    "baixar-erro",
                    t!("baixar-erro-titulo"),
                    t!("baixar-erro", caminho = b.caminho.as_str(), motivo = motivo),
                ),
            };
            self.avisar_colar(b.janela, severidade, base, titulo, texto);
        }
    }

    fn cancelar_download(&self, id: u64) {
        if let Some(b) = self.baixar.ativos.borrow().iter().find(|b| b.id == id) {
            b.cancelar.store(true, Ordering::Relaxed);
        }
    }
}

/// A barra de um download: título, o que andou, "Cancelar" e a barra de progresso.
fn barra(tela: &Rc<Tela>, b: Rc<Baixando>) -> impl View + use<> {
    let id = b.id;
    let texto = {
        let (andou, caminho) = (b.andou.clone(), b.caminho.clone());
        move || {
            andou.with(|a| {
                t!(
                    "baixar-andou",
                    caminho = caminho.as_str(),
                    feito = tamanho(a.feito),
                    total = tamanho(a.total)
                )
            })
        }
    };
    let enche = {
        let andou = b.andou.clone();
        move || andou.with(|a| a.feito as f64 / a.total.max(1) as f64)
    };
    let cancelar = {
        let tela = Rc::clone(tela);
        move || tela.cancelar_download(id)
    };
    Column::new()
        .gap(4)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    InfoBar::new(InfoBarSeverity::Informational)
                        .id(format!("baixar-progresso-{id}"))
                        .title(t!("baixar-baixando-de", conexao = b.conexao.as_str()))
                        .message(texto)
                        .grow(1),
                )
                .child(
                    Button::new(t!("comum-cancelar"))
                        .id(format!("baixar-cancelar-{id}"))
                        .on_click(cancelar),
                ),
        )
        .child(
            ProgressBar::new()
                .id(format!("baixar-progresso-barra-{id}"))
                .value(enche),
        )
}

/// As barras dos downloads de `janela`, em cima dos terminais.
pub(super) fn barras(tela: &Rc<Tela>, janela: Janela) -> impl View + use<> {
    let handle = janela.handle;
    let versao = tela.baixar.versao;
    let (da_lista, da_barra) = (Rc::clone(tela), Rc::clone(tela));
    For::each(
        move || {
            versao.get();
            da_lista
                .baixar
                .ativos
                .borrow()
                .iter()
                .filter(|b| b.janela == handle)
                .cloned()
                .collect::<Vec<_>>()
        },
        |b: &Rc<Baixando>| b.id,
        move |b: Rc<Baixando>| barra(&da_barra, b),
    )
}
