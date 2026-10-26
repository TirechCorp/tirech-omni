//! "Gerar comando com IA" (M52, fatia a): um Flyout sobre o terminal ativo. A pessoa descreve o que
//! quer, o modelo sugere um comando e o app o **insere na linha do terminal, sem Enter**. A IA
//! nunca executa nada: quem aperta Enter é a pessoa.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;
use tomni_ui::{clipboard, request_focus};

use super::Tela;
use super::ia_envio::{Chegou, Envio, Incluir, Modo, Montar, Receptor};
use crate::assistente::{prompts, resposta};
use crate::janelas::Janela;
use crate::t;
use crate::texto_limpo;

/// O que o Flyout guarda entre uma abertura e outra.
pub(super) struct EstadoComando {
    /// O que a pessoa escreveu.
    pedido: Signal<String>,
    /// Se as últimas linhas do terminal vão junto.
    com_tela: Signal<bool>,
    /// Quantas linhas tem o texto selecionado que vai junto (0: não vai nada).
    linhas_da_selecao: Signal<usize>,
    /// O que o modelo respondeu: cru enquanto chega, depois só o comando.
    sugestao: Signal<String>,
    /// A frase que explica o comando.
    explicacao: Signal<String>,
    /// Se o comando merece o aviso de perigo.
    perigoso: Signal<bool>,
    /// O erro do pedido (vazio: nenhum).
    erro: Signal<String>,
    /// Se o pedido está em andamento.
    aguardando: Signal<bool>,
    /// Se a sugestão já está inteira.
    pronto: Signal<bool>,
    /// Se o comando vai ser inserido numa linha só (várias linhas sem bracketed paste).
    uma_linha: Signal<bool>,
    /// O nome do provedor desta aba.
    perfil: Signal<String>,
    /// Os pedidos anteriores desta sessão do app, o mais novo primeiro.
    historico: Signal<Vec<String>>,
    /// O pedido em andamento.
    atual: Cell<Option<u64>>,
    /// A aba de onde o Flyout abriu.
    sessao: Cell<Option<Session>>,
    /// A janela do Flyout aberto.
    janela: Cell<Option<Janela>>,
    /// A resposta como chegou.
    bruto: RefCell<String>,
    /// O que "Inserir" põe no terminal.
    a_inserir: RefCell<String>,
    /// O pedido da última vez, para o "De novo".
    ultimo: RefCell<String>,
}

impl EstadoComando {
    pub(super) fn novo() -> Self {
        Self {
            pedido: signal(String::new()),
            com_tela: signal(false),
            linhas_da_selecao: signal(0),
            sugestao: signal(String::new()),
            explicacao: signal(String::new()),
            perigoso: signal(false),
            erro: signal(String::new()),
            aguardando: signal(false),
            pronto: signal(false),
            uma_linha: signal(false),
            perfil: signal(String::new()),
            historico: signal(Vec::new()),
            atual: Cell::new(None),
            sessao: Cell::new(None),
            janela: Cell::new(None),
            bruto: RefCell::default(),
            a_inserir: RefCell::default(),
            ultimo: RefCell::default(),
        }
    }

    /// Volta ao estado de antes do pedido, sem mexer no que a pessoa escreveu.
    fn limpar_resposta(&self) {
        self.sugestao.set(String::new());
        self.explicacao.set(String::new());
        self.perigoso.set(false);
        self.erro.set(String::new());
        self.aguardando.set(false);
        self.pronto.set(false);
        self.uma_linha.set(false);
        self.bruto.borrow_mut().clear();
        self.a_inserir.borrow_mut().clear();
    }
}

/// O id do terminal da aba de chave `chave` (o mesmo que os painéis dão).
pub(super) fn id_do_terminal(chave: &str) -> String {
    let slug: String = chave
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .flat_map(char::to_lowercase)
        .collect();
    format!("paineis-terminal-{slug}")
}

/// Se a aba aceita colar entre `ESC[200~` e `ESC[201~`.
pub(super) fn aceita_colar_em_bloco(sessao: &Session) -> bool {
    sessao
        .emulator(|e| e.modes().bracketed_paste)
        .unwrap_or(false)
}

impl Tela {
    /// Liga o que reage sozinho: trocar o provedor no seletor e cancelar o pedido quando o Flyout
    /// fecha (Esc ou clique fora).
    pub(super) fn ligar_comando(self: &Rc<Self>) {
        let (t, perfil) = (Rc::clone(self), self.assistente.comando.perfil);
        effect(move || {
            let nome = perfil.get();
            untrack(|| t.trocar_perfil_pelo_nome(&nome));
        });
    }

    /// O seletor mudou: a aba passa a usar o provedor de nome `nome`.
    fn trocar_perfil_pelo_nome(&self, nome: &str) {
        let c = &self.assistente.comando;
        let Some(sessao) = c.sessao.get() else {
            return;
        };
        let Some(id) = self
            .assistente
            .perfis
            .with_untracked(|a| a.perfis.iter().find(|p| p.nome == nome).map(|p| p.id))
        else {
            return;
        };
        if self.perfil_da_aba(Some(&sessao)).map(|p| p.id) != Some(id) {
            self.trocar_perfil_da_aba(&sessao.key_str(), id);
        }
    }

    /// Abre o Flyout no terminal ativo de `janela`. Sem provedor, abre o "Escolher um provedor de
    /// IA" primeiro.
    pub(super) fn abrir_comando_ia(self: &Rc<Self>, janela: Janela) {
        let Some(sessao) = self.sessao_ativa_de(janela) else {
            self.avisar(
                InfoBarSeverity::Informational,
                t!("ia-comando-sem-terminal"),
            );
            return;
        };
        if !self.exigir_perfil_de_ia() {
            return;
        }
        let c = &self.assistente.comando;
        if let Some(id) = c.atual.take() {
            self.cancelar_ia(id);
        }
        c.limpar_resposta();
        c.pedido.set(String::new());
        c.com_tela.set(false);
        let selecao = sessao
            .selection_text()
            .filter(|s| !s.trim().is_empty())
            .map_or(0, |s| s.lines().count().max(1));
        c.linhas_da_selecao.set(selecao);
        c.sessao.set(Some(sessao));
        c.janela.set(Some(janela));
        if let Some(perfil) = self.perfil_da_aba(Some(&sessao)) {
            c.perfil.set(perfil.nome);
        }
        janela.ia_comando.set(true);
        request_focus("ia-comando-pedido");
    }

    /// Fecha o Flyout, corta o pedido em andamento e devolve o foco ao terminal.
    fn fechar_comando(&self, focar_terminal: bool) {
        let c = &self.assistente.comando;
        if let Some(id) = c.atual.take() {
            self.cancelar_ia(id);
        }
        c.aguardando.set(false);
        if let Some(janela) = c.janela.get() {
            janela.ia_comando.set(false);
        }
        if focar_terminal && let Some(sessao) = c.sessao.get() {
            request_focus(id_do_terminal(&sessao.key_str()));
        }
    }

    /// Manda o pedido escrito. `anterior` é a sugestão de que o "De novo" quer fugir.
    fn pedir_comando(self: &Rc<Self>, texto: &str, anterior: Option<String>) {
        let c = &self.assistente.comando;
        let texto = texto.trim().to_owned();
        let Some(sessao) = c.sessao.get() else {
            return;
        };
        if texto.is_empty() || c.aguardando.get_untracked() {
            return;
        }
        let Some(perfil) = self.perfil_da_aba(Some(&sessao)) else {
            self.exigir_perfil_de_ia();
            return;
        };
        if let Some(id) = c.atual.take() {
            self.cancelar_ia(id);
        }
        c.limpar_resposta();
        c.historico.update(|h| {
            h.retain(|p| *p != texto);
            h.insert(0, texto.clone());
            h.truncate(50);
        });
        *c.ultimo.borrow_mut() = texto.clone();
        c.aguardando.set(true);
        let montar: Montar = match anterior {
            Some(anterior) => {
                Box::new(move |pedido, partes| prompts::pedido_de_novo(pedido, &anterior, partes))
            }
            None => Box::new(prompts::pedido_comando),
        };
        let tela = Rc::clone(self);
        let receptor: Receptor = Rc::new(move |chegou| tela.chegou_comando(chegou));
        let id = self.enviar_ia(Envio {
            sessao: Some(sessao),
            perfil,
            modo: Modo::Comando,
            incluir: Incluir {
                selecao: c.linhas_da_selecao.get_untracked() > 0,
                linhas: c.com_tela.get_untracked(),
                erro: false,
                sistema: true,
            },
            historico: Vec::new(),
            pedido: texto,
            montar,
            receptor,
        });
        c.atual.set(Some(id));
    }

    /// Chega mais um pedaço da resposta (ou o fim dela).
    fn chegou_comando(&self, chegou: Chegou) {
        let c = &self.assistente.comando;
        match chegou {
            Chegou::Saiu(_) => {}
            Chegou::Texto(texto) => {
                let mut bruto = c.bruto.borrow_mut();
                bruto.push_str(&texto);
                c.sugestao.set(bruto.clone());
            }
            Chegou::Fim(Ok(())) => {
                c.atual.set(None);
                c.aguardando.set(false);
                self.fim_do_comando();
            }
            Chegou::Fim(Err(erro)) => {
                c.atual.set(None);
                c.aguardando.set(false);
                c.erro.set(erro);
            }
            Chegou::Desistiu => {
                c.atual.set(None);
                c.limpar_resposta();
            }
        }
    }

    /// A resposta acabou: separa o comando da frase e deixa tudo pronto para inserir.
    fn fim_do_comando(&self) {
        let c = &self.assistente.comando;
        let bruto = std::mem::take(&mut *c.bruto.borrow_mut());
        let (comando, explicacao) = resposta::comando_e_explicacao(&bruto);
        let Some(comando) = comando.filter(|s| !s.trim().is_empty()) else {
            c.sugestao.set(String::new());
            c.erro.set(t!("ia-comando-vazio"));
            return;
        };
        let em_bloco = c.sessao.get().is_some_and(|s| aceita_colar_em_bloco(&s));
        let uma_linha = texto_limpo::tem_varias_linhas(&comando) && !em_bloco;
        let a_inserir = if uma_linha {
            texto_limpo::em_uma_linha(&comando)
        } else {
            texto_limpo::sem_controle(&comando).trim().to_owned()
        };
        c.perigoso.set(resposta::perigoso(&a_inserir).is_some());
        c.uma_linha.set(uma_linha);
        c.sugestao.set(a_inserir.clone());
        c.explicacao.set(explicacao.trim().to_owned());
        *c.a_inserir.borrow_mut() = a_inserir;
        c.pronto.set(true);
        request_focus("ia-comando-inserir");
    }

    /// "Inserir": põe o comando na linha do terminal, sem Enter, e fecha.
    fn inserir_comando(&self) {
        let c = &self.assistente.comando;
        let Some(sessao) = c.sessao.get() else {
            return;
        };
        let texto = c.a_inserir.borrow().clone();
        if let Some(bytes) = texto_limpo::para_inserir(&texto, aceita_colar_em_bloco(&sessao)) {
            sessao.send(&bytes);
        }
        self.fechar_comando(true);
    }
}

impl Tela {
    /// "Explicar no chat": a explicação do comando gerado vai para o chat da aba, e o Flyout fecha.
    fn explicar_comando_gerado(self: &Rc<Self>) {
        let comando = self.assistente.comando.a_inserir.borrow().clone();
        let Some(sessao) = self.assistente.comando.sessao.get() else {
            return;
        };
        if comando.is_empty() {
            return;
        }
        self.fechar_comando(false);
        self.explicar_comando_no_chat(sessao, comando);
    }
}

/// O Flyout `ia-comando`, preso a uma coluna sem altura no fim da área de terminais de `janela`.
pub(super) fn flyout(tela: &Rc<Tela>, janela: Janela) -> impl View {
    let c = &tela.assistente.comando;
    let (pedido, com_tela, linhas) = (c.pedido, c.com_tela, c.linhas_da_selecao);
    let (sugestao, explicacao, perigoso) = (c.sugestao, c.explicacao, c.perigoso);
    let (erro, aguardando, pronto) = (c.erro, c.aguardando, c.pronto);
    let (uma_linha, perfil, historico) = (c.uma_linha, c.perfil, c.historico);
    let config = tela.config;
    let perfis = tela.assistente.perfis;

    // Fechou por Esc ou por clique fora: o pedido para.
    {
        let t = Rc::clone(tela);
        let aberto = janela.ia_comando;
        effect(move || {
            if !aberto.get() {
                untrack(|| {
                    if t.assistente.comando.atual.get().is_some() {
                        t.fechar_comando(false);
                    }
                });
            }
        });
    }

    let sugestoes = move |digitado: &str| {
        let digitado = digitado.to_lowercase();
        historico.with_untracked(|h| {
            h.iter()
                .filter(|p| p.to_lowercase().contains(&digitado) && p.to_lowercase() != digitado)
                .take(8)
                .cloned()
                .collect()
        })
    };
    let caixa = {
        let t = Rc::clone(tela);
        AutoSuggestBox::bind(pedido, sugestoes)
            .id("ia-comando-pedido")
            .placeholder(t!("ia-comando-exemplo"))
            .on_submit(move |texto| t.pedir_comando(&texto, None))
    };
    let com_selecao = Row::new()
        .gap(8)
        .align(Align::Center)
        .visible(move || linhas.get() > 0)
        .child(
            Text::new(move || t!("ia-comando-com-selecao", n = linhas.get() as i64))
                .id("ia-comando-com-selecao")
                .secondary(),
        )
        .child(
            Button::icon_only(Symbol::Dismiss)
                .id("ia-comando-sem-selecao")
                .label(t!("ia-comando-tirar-selecao"))
                .on_click(move || linhas.set(0)),
        );
    let escolha_do_perfil = Column::new().child(For::each(
        move || {
            vec![perfis.with(|a| {
                a.perfis
                    .iter()
                    .map(|p| p.nome.clone())
                    .collect::<Vec<String>>()
            })]
        },
        |nomes: &Vec<String>| nomes.join("\u{1}"),
        move |nomes: Vec<String>| {
            ComboBox::bind(perfil, nomes)
                .id("ia-comando-perfil")
                .width(160)
        },
    ));
    let botoes = Row::new()
        .gap(8)
        .align(Align::Center)
        .child(
            Button::new(move || {
                if uma_linha.get() {
                    t!("ia-comando-inserir-uma-linha")
                } else {
                    t!("ia-comando-inserir")
                }
            })
            .id("ia-comando-inserir")
            .accent()
            .visible(move || pronto.get())
            .on_click({
                let t = Rc::clone(tela);
                move || t.inserir_comando()
            }),
        )
        .child(
            Button::new(t!("comum-copiar"))
                .id("ia-comando-copiar")
                .visible(move || pronto.get())
                .on_click({
                    let t = Rc::clone(tela);
                    move || {
                        let texto = t.assistente.comando.a_inserir.borrow().clone();
                        clipboard::write(texto);
                    }
                }),
        )
        .child(
            Button::new(t!("ia-comando-de-novo"))
                .id("ia-comando-de-novo")
                .visible(move || pronto.get() || !erro.with(String::is_empty))
                .on_click({
                    let t = Rc::clone(tela);
                    move || {
                        let anterior = t.assistente.comando.a_inserir.borrow().clone();
                        let anterior = (!anterior.is_empty()).then_some(anterior);
                        let texto = t.assistente.comando.ultimo.borrow().clone();
                        t.pedir_comando(&texto, anterior);
                    }
                }),
        )
        .child(
            Button::new(t!("ia-comando-explicar"))
                .id("ia-comando-explicar")
                .visible(move || pronto.get())
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.explicar_comando_gerado()
                }),
        )
        .child(Column::new().grow(1))
        .child(escolha_do_perfil);
    let conteudo = Column::new()
        .gap(8)
        .width(560)
        .child(caixa)
        .child(
            CheckBox::bind(com_tela, move || {
                t!(
                    "ia-comando-com-tela",
                    n = config.with(|c| c.ia.linhas_contexto) as i64
                )
            })
            .id("ia-comando-com-tela"),
        )
        .child(com_selecao)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .visible(move || aguardando.get())
                .child(ProgressRing::new().width(16).height(16))
                .child(Text::new(t!("ia-comando-gerando")).secondary()),
        )
        .child(
            TextEditor::bind(sugestao)
                .id("ia-comando-sugestao")
                .read_only(true)
                .monospace()
                .wrap(true)
                .min_height(44.0)
                .max_height(160.0)
                .visible(move || !sugestao.with(String::is_empty)),
        )
        .child(
            Text::new(move || explicacao.get())
                .id("ia-comando-explicacao")
                .wrap()
                .visible(move || !explicacao.with(String::is_empty)),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Warning)
                .id("ia-comando-perigo")
                .message(t!("ia-comando-perigo"))
                .visible(move || perigoso.get()),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("ia-comando-erro")
                .message(move || erro.get())
                .visible(move || !erro.with(String::is_empty)),
        )
        .child(botoes);
    // A camada se ancora no widget de cima: uma coluna sem altura, sem nada que a pessoa clique.
    Column::new().height(0).child(
        Flyout::new(conteudo)
            .id("ia-comando")
            .open(janela.ia_comando),
    )
}
