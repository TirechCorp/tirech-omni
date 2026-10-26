//! A lista "Sessões no servidor…" da conexão SSH: as sessões do tmux e do screen que estão vivas
//! no servidor, abertas por este app ou não, com "Abrir" (uma aba nova ligada a ela, ou a aba
//! que já está nela) e "Encerrar". A parte pura (o script e a leitura) está em [`crate::tmux`].
//!
//! A lista roda pelo `sh -s` na conexão de uma aba viva da conexão, se há uma; senão abre outra
//! conexão com a senha do cofre (sem ela e sem aba, avisa: não há como pedir a senha aqui). O
//! script roda numa thread e entrega pelo `pronto`; um pedido novo descarta a entrega do velho.

use std::cell::Cell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::ExecOutput;

use super::Tela;
use crate::abrir::{ExecDaSessao, Executor, precisa_de_senha};
use crate::agente::{acordar, ler_contador};
use crate::modelo::{Multiplexador, SessaoDaAba};
use crate::t;
use crate::tmux::{self, NoServidor, SCRIPT_LISTAR, SessaoNoServidor};

/// Como a lista está.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Situacao {
    Carregando,
    Erro(String),
    Pronta(NoServidor),
}

/// O que as threads entregam: o número do pedido e a saída do script.
type Entregas = Arc<Mutex<Vec<(u64, Result<String, String>)>>>;

/// O estado da lista na tela.
pub(super) struct Estado {
    aberto: Signal<bool>,
    titulo: Signal<String>,
    situacao: Signal<Situacao>,
    /// A conexão da lista.
    conexao: Cell<u64>,
    /// O número do último pedido: a entrega de outro fica de fora.
    pedido: Cell<u64>,
    entregas: Entregas,
    pronto: Signal<u64>,
    /// Sobe quando abre ou fecha uma aba, para "aberta aqui" e "Ir para a aba".
    abas: Signal<u64>,
}

impl Estado {
    /// As abas nas sessões mudaram (abriu, fechou ou esqueceu uma).
    pub(super) fn abas_mudaram(&self) {
        self.abas.update(|n| *n += 1);
    }

    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            titulo: signal(String::new()),
            situacao: signal(Situacao::Carregando),
            conexao: Cell::new(0),
            pedido: Cell::new(0),
            entregas: Arc::default(),
            pronto: signal(0),
            abas: signal(0),
        }
    }
}

/// Por onde o script chega ao servidor.
enum Via {
    Sessao(ExecDaSessao),
    Executor(Executor),
}

impl Via {
    /// Roda `script` pelo `sh -s`. Bloqueia.
    fn rodar(self, script: &str) -> Result<ExecOutput, String> {
        match self {
            Via::Sessao(exec) => exec("sh -s", &mut Cursor::new(script.to_owned()), &mut |_| {}),
            Via::Executor(executor) => executor("sh -s", script.as_bytes()),
        }
    }
}

/// O texto da saída, ou o erro se o `sh` falhou.
fn saida_do_script(saida: Result<ExecOutput, String>) -> Result<String, String> {
    let saida = saida?;
    if saida.success() {
        return Ok(saida.stdout_text());
    }
    let erro = saida.stderr_text();
    Err(match erro.trim() {
        "" => t!(
            "sessoes-erro-codigo",
            codigo = saida.status.map_or(-1, i64::from)
        ),
        texto => texto.to_owned(),
    })
}

impl Tela {
    /// Abre a lista das sessões do servidor da conexão `id`.
    pub(super) fn abrir_sessoes_do_servidor(self: &Rc<Self>, id: u64) {
        let Some(c) = self.conexao_ssh(id) else {
            return;
        };
        let est = &self.sessoes_servidor;
        est.conexao.set(id);
        est.titulo
            .set(t!("sessoes-titulo", conexao = c.name.as_str()));
        est.aberto.set(true);
        self.listar_sessoes(None);
    }

    /// Pede a lista ao servidor numa thread; com `antes`, roda esse script primeiro (o encerrar).
    fn listar_sessoes(&self, antes: Option<String>) {
        let est = &self.sessoes_servidor;
        let id = est.conexao.get();
        let pedido = est.pedido.get() + 1;
        est.pedido.set(pedido);
        let Some(c) = self.conexao_ssh(id) else {
            return;
        };
        let senhas = self.senhas_do_cofre(&c);
        let viva = self
            .sessao_viva_da_conexao(id)
            .and_then(|sessao| self.opener.exec_da_sessao(sessao).ok());
        let vias: Vec<Via> = if let Some(exec) = viva {
            vec![Via::Sessao(Arc::clone(&exec)), Via::Sessao(exec)]
        } else if precisa_de_senha(&c) && senhas.conexao.is_none() {
            est.situacao.set(Situacao::Erro(t!("sessoes-sem-senha")));
            return;
        } else {
            let efetiva = self.conexao_efetiva(&c);
            let quantas = if antes.is_some() { 2 } else { 1 };
            (0..quantas)
                .map(|_| Via::Executor(self.opener.executor(&efetiva, senhas.clone())))
                .collect()
        };
        est.situacao.set(Situacao::Carregando);
        let (entregas, pronto) = (Arc::clone(&est.entregas), est.pronto);
        std::thread::spawn(move || {
            let mut vias = vias.into_iter();
            let mut saida = Ok(());
            if let Some(script) = antes
                && let Some(via) = vias.next()
            {
                saida = saida_do_script(via.rodar(&script)).map(|_| ());
            }
            let lista = saida.and_then(|()| match vias.next() {
                Some(via) => saida_do_script(via.rodar(SCRIPT_LISTAR)),
                None => Err(String::new()),
            });
            entregas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((pedido, lista));
            acordar(pronto);
        });
    }

    /// Guarda o que as threads entregaram (só a do último pedido vale).
    fn receber_sessoes(&self) {
        let est = &self.sessoes_servidor;
        let lote =
            std::mem::take(&mut *est.entregas.lock().unwrap_or_else(PoisonError::into_inner));
        for (pedido, lista) in lote {
            if pedido != est.pedido.get() {
                continue;
            }
            est.situacao.set(match lista {
                Ok(texto) => Situacao::Pronta(tmux::ler_lista(&texto)),
                Err(erro) => Situacao::Erro(erro),
            });
        }
    }

    /// "Abrir" de uma sessão da lista: vai para a aba que já está nela ou abre uma aba nova
    /// ligada a ela.
    fn abrir_sessao_do_servidor(self: &Rc<Self>, alvo: &SessaoDaAba) {
        let est = &self.sessoes_servidor;
        est.aberto.set(false);
        if let Some(sessao) = self.tmux.aba_de(alvo) {
            let achada = self
                .sessoes_das_janelas(None)
                .into_iter()
                .find(|(_, s)| *s == sessao);
            if let Some((janela, sessao)) = achada {
                janela.layout.update(|l| l.select(sessao));
                janela.handle.focus();
                self.mostrar_terminais();
                return;
            }
        }
        let id = est.conexao.get();
        self.tmux.forcar(id, alvo.clone());
        self.abrir_e_mostrar(id);
    }

    /// "Encerrar" de uma sessão da lista: encerra no servidor, sem pergunta, e atualiza a lista.
    /// Uma aba daqui que estava nela deixa de ser da sessão (fechar não pergunta mais).
    fn encerrar_sessao_do_servidor(&self, s: &SessaoNoServidor) {
        if let Some(aba) = s.da_aba()
            && let Some(sessao) = self.tmux.aba_de(&aba)
        {
            self.esquecer_tmux(sessao);
        }
        self.listar_sessoes(Some(tmux::script_encerrar(s.programa, &s.alvo)));
    }

    /// Se uma aba daqui está na sessão `s`.
    fn aberta_aqui(&self, s: &SessaoNoServidor) -> bool {
        s.da_aba()
            .is_some_and(|aba| self.tmux.aba_de(&aba).is_some())
    }

    fn linhas_de_sessoes(&self) -> Vec<Linha> {
        let _ = self.sessoes_servidor.abas.get();
        self.sessoes_servidor.situacao.with(|s| match s {
            Situacao::Pronta(lido) => lido
                .sessoes
                .iter()
                .map(|s| Linha {
                    sessao: s.clone(),
                    aqui: self.aberta_aqui(s),
                })
                .collect(),
            _ => Vec::new(),
        })
    }
}

/// Uma linha da lista.
#[derive(Clone, PartialEq)]
struct Linha {
    sessao: SessaoNoServidor,
    aqui: bool,
}

impl Linha {
    fn chave(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.sessao.programa.programa(),
            self.sessao.alvo,
            self.sessao.clientes,
            self.aqui
        )
    }
}

/// O id de widget de uma sessão: só letras, números e `-`.
fn id_da_sessao(s: &SessaoNoServidor) -> String {
    let alvo: String = s
        .alvo
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("sessoes-{}-{alvo}", s.programa.programa())
}

fn vista_da_linha(tela: &Rc<Tela>, l: &Linha) -> impl View + use<> {
    let s = &l.sessao;
    let id = id_da_sessao(s);
    let mut detalhes = vec![match s.clientes {
        0 => t!("sessoes-desligada"),
        n => t!("sessoes-ligada", n = i64::from(n)),
    }];
    if l.aqui {
        detalhes.push(t!("sessoes-aberta-aqui"));
    }
    if s.alvo != s.nome {
        detalhes.push(s.alvo.clone());
    }
    let aba = s.da_aba();
    if aba.is_none() {
        detalhes.push(t!("sessoes-nome-invalido"));
    }
    let abrir = {
        let (t, aba) = (Rc::clone(tela), aba.clone());
        move || {
            if let Some(aba) = &aba {
                t.abrir_sessao_do_servidor(aba);
            }
        }
    };
    let encerrar = {
        let (t, s) = (Rc::clone(tela), s.clone());
        move || t.encerrar_sessao_do_servidor(&s)
    };
    let icone = match s.programa {
        Multiplexador::Tmux => Symbol::WindowConsole,
        Multiplexador::Screen => Symbol::Desktop,
    };
    Row::new()
        .id(id.clone())
        .gap(12)
        .align(Align::Center)
        .padding(Insets::symmetric(4.0, 6.0))
        .child(Icon::new(icone).size(20).decorative())
        .child(
            Column::new()
                .gap(2)
                .grow(1)
                .child(
                    Row::new()
                        .gap(8)
                        .align(Align::Center)
                        .child(Text::new(s.nome.clone()).body_strong().max_lines(1))
                        .child(
                            Text::new(s.programa.programa())
                                .caption()
                                .secondary()
                                .max_lines(1),
                        ),
                )
                .child(
                    Text::new(detalhes.join(" · "))
                        .caption()
                        .secondary()
                        .max_lines(1),
                ),
        )
        .child(
            Button::new(if l.aqui {
                t!("sessoes-ir-para-a-aba")
            } else {
                t!("sessoes-abrir")
            })
            .id(format!("{id}-abrir"))
            .disabled(aba.is_none())
            .on_click(abrir),
        )
        .child(
            Button::new(t!("sessoes-encerrar"))
                .id(format!("{id}-encerrar"))
                .on_click(encerrar),
        )
}

/// Liga o efeito que recebe as entregas das threads. Chame uma vez, ao montar a tela.
pub(super) fn ligar(tela: &Rc<Tela>) {
    let (eu, pronto) = (tela.eu.clone(), tela.sessoes_servidor.pronto);
    effect(move || {
        let _ = ler_contador(pronto);
        if let Some(t) = eu.upgrade() {
            untrack(|| t.receber_sessoes());
        }
    });
}

/// O diálogo da lista. Montado na janela principal.
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl View + use<> {
    let est = &tela.sessoes_servidor;
    let (aberto, titulo, situacao) = (est.aberto, est.titulo, est.situacao);
    let carregando = move || situacao.with(|s| *s == Situacao::Carregando);
    let erro = move || match situacao.get() {
        Situacao::Erro(erro) => Some(erro),
        _ => None,
    };
    let lido = move || match situacao.get() {
        Situacao::Pronta(lido) => Some(lido),
        _ => None,
    };
    let atualizar = {
        let t = Rc::clone(tela);
        move || t.listar_sessoes(None)
    };
    let cabecalho = Row::new()
        .gap(8)
        .align(Align::Center)
        .child(
            Text::new(t!("sessoes-explicacao"))
                .caption()
                .secondary()
                .wrap()
                .grow(1),
        )
        .child(
            Button::icon_only(Symbol::ArrowClockwise)
                .id("sessoes-atualizar")
                .label(t!("sessoes-atualizar"))
                .tooltip(t!("sessoes-atualizar"))
                .disabled(carregando)
                .on_click(atualizar),
        );
    let aviso_erro = InfoBar::new(InfoBarSeverity::Warning)
        .id("sessoes-erro")
        .title(t!("sessoes-erro-titulo"))
        .message(move || erro().unwrap_or_default())
        .visible(move || erro().is_some());
    let sem_programas = Text::new(t!("sessoes-sem-programas"))
        .id("sessoes-sem-programas")
        .secondary()
        .wrap()
        .visible(move || lido().is_some_and(|l| l.programas.is_empty()));
    let vazia = Text::new(move || {
        let programas = lido()
            .map(|l| {
                l.programas
                    .iter()
                    .map(|p| p.programa())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        t!("sessoes-vazia", programas = programas)
    })
    .id("sessoes-vazia")
    .secondary()
    .wrap()
    .visible(move || lido().is_some_and(|l| !l.programas.is_empty() && l.sessoes.is_empty()));
    let linhas = For::each(
        {
            let t = Rc::clone(tela);
            move || t.linhas_de_sessoes()
        },
        Linha::chave,
        {
            let t = Rc::clone(tela);
            move |l: Linha| vista_da_linha(&t, &l)
        },
    );
    ContentDialog::new(move || titulo.get(), aberto)
        .id("sessoes-dialogo")
        .content(
            Column::new()
                .gap(8)
                .width(520)
                .child(cabecalho)
                .child(
                    ProgressRing::new()
                        .id("sessoes-carregando")
                        .width(32)
                        .height(32)
                        .visible(carregando),
                )
                .child(aviso_erro)
                .child(sem_programas)
                .child(vazia)
                .child(
                    ScrollViewer::new()
                        .height(280)
                        .visible(move || lido().is_some_and(|l| !l.sessoes.is_empty()))
                        .child(Column::new().id("sessoes-lista").gap(4).child(linhas)),
                ),
        )
        .close(t!("comum-fechar"))
}
