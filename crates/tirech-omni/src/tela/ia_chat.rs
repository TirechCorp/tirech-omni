//! O chat da IA ao lado do terminal (M52, fatias b e c): um painel encaixável (`chat`) com uma
//! conversa por aba. Os itens "Explicar…" do menu do terminal e o "Explicar no chat" do Flyout do
//! comando também caem aqui: a resposta chega no chat, nunca num popup.
//!
//! A IA **nunca executa** nada. O botão "Inserir no terminal" dos blocos de código põe o texto
//! na linha da aba, sem Enter.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use tomni_ui::ai::Message;
use tomni_ui::markdown::MarkdownView;
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;
use tomni_ui::{Key, ScrollLink, clipboard, request_focus};

use super::Tela;
use super::ia_comando::{aceita_colar_em_bloco, id_do_terminal};
use super::ia_envio::{Chegou, Envio, Incluir, Modo, Montar, Receptor};
use crate::assistente::envio::Preparado;
use crate::assistente::{prompts, resposta};
use crate::ferramentas;
use crate::paineis;
use crate::t;
use crate::texto_limpo;

/// O id do painel do chat nos painéis dos terminais.
const PAINEL_DO_CHAT: &str = "chat";

/// O que se pergunta ao modelo.
#[derive(Clone, Debug)]
pub(super) enum Pedido {
    /// O que a pessoa escreveu, com as partes do terminal que ela marcou.
    Pergunta { texto: String, incluir: Incluir },
    /// "Explicar a seleção" do menu do terminal.
    ExplicarSelecao,
    /// "Explicar o erro" do menu do terminal.
    ExplicarErro,
    /// "Explicar no chat" do Flyout do comando.
    ExplicarComando(String),
}

impl Pedido {
    /// O que do terminal entra no pedido.
    fn incluir(&self) -> Incluir {
        match self {
            Pedido::Pergunta { incluir, .. } => *incluir,
            Pedido::ExplicarSelecao => Incluir {
                selecao: true,
                sistema: true,
                ..Incluir::default()
            },
            Pedido::ExplicarErro => Incluir {
                erro: true,
                sistema: true,
                ..Incluir::default()
            },
            Pedido::ExplicarComando(_) => Incluir {
                sistema: true,
                ..Incluir::default()
            },
        }
    }

    /// O texto que o motor recebe como pedido (ele limpa e redige antes de montar a mensagem).
    fn texto_enviado(&self) -> String {
        match self {
            Pedido::Pergunta { texto, .. } => texto.clone(),
            Pedido::ExplicarSelecao => prompts::EXPLICAR_SELECAO.to_owned(),
            Pedido::ExplicarErro => prompts::EXPLICAR_ERRO.to_owned(),
            Pedido::ExplicarComando(comando) => prompts::explicar_comando(comando),
        }
    }

    /// O que o balão da pessoa mostra.
    fn rotulo(&self) -> String {
        match self {
            Pedido::Pergunta { texto, .. } => texto.trim().to_owned(),
            Pedido::ExplicarSelecao => t!("ia-chat-pedido-selecao"),
            Pedido::ExplicarErro => t!("ia-chat-pedido-erro"),
            Pedido::ExplicarComando(comando) => {
                t!("ia-chat-pedido-comando", comando = comando.as_str())
            }
        }
    }
}

/// O que saiu de verdade com uma mensagem da pessoa.
#[derive(Clone)]
struct Enviado {
    preparado: Rc<Preparado>,
    /// Quantas mensagens antigas da conversa ficaram de fora por causa do limite.
    cortadas: usize,
}

/// Uma mensagem da conversa. Os textos ficam em `Rc` para a vista de cada mensagem enxergar o
/// que chega sem criar um signal por mensagem.
#[derive(Clone)]
struct Msg {
    id: u64,
    /// A chave da aba da conversa.
    aba: String,
    /// Se é da pessoa (senão, da IA).
    pessoa: bool,
    texto: Rc<RefCell<String>>,
    /// Se a resposta já chegou inteira.
    pronta: Rc<Cell<bool>>,
    /// "Contexto: …", só nas mensagens da pessoa.
    contexto: String,
    /// O que saiu, preenchido quando o pedido sai.
    enviado: Rc<RefCell<Option<Enviado>>>,
    /// Quantas mensagens antigas o limite tirou deste envio.
    cortadas: usize,
}

/// A conversa de uma aba.
struct Conversa {
    sessao: Session,
    mensagens: Vec<Msg>,
    /// O pedido em andamento no motor.
    atual: Option<u64>,
    /// O erro do último pedido (vazio: nenhum).
    erro: String,
    /// O último pedido, para o "Tentar de novo" e para devolver a pergunta à caixa quando a
    /// pessoa desiste numa confirmação.
    ultimo: Option<Pedido>,
}

/// O que o painel guarda entre uma aba e outra.
pub(super) struct EstadoChat {
    /// A chave da aba cuja conversa o painel mostra (vazio: nenhuma).
    aba: Signal<String>,
    /// Sobe quando a lista de mensagens da aba mostrada muda.
    lista: Signal<u64>,
    /// Sobe a cada pedaço de resposta.
    texto: Signal<u64>,
    titulo: Signal<String>,
    entrada: Signal<String>,
    ctx_selecao: Signal<bool>,
    ctx_linhas: Signal<bool>,
    ctx_erro: Signal<bool>,
    ctx_sistema: Signal<bool>,
    /// O nome do provedor da aba mostrada.
    perfil: Signal<String>,
    aguardando: Signal<bool>,
    erro: Signal<String>,
    /// Se o último bloco inserido merece o aviso de perigo.
    perigo: Signal<bool>,
    /// Se há alguma resposta pronta na aba mostrada (liga o "Copiar").
    ha_resposta: Signal<bool>,
    ver_aberto: Signal<bool>,
    ver_texto: Signal<String>,
    ver_instrucoes: Signal<String>,
    ver_instrucoes_abertas: Signal<bool>,
    ver_avisos: Signal<String>,
    conversas: RefCell<HashMap<String, Conversa>>,
    proximo: Cell<u64>,
    rolagem: Rc<ScrollLink>,
    /// Se a rolagem acompanha o fim da conversa.
    seguindo: Cell<bool>,
    ultimo_offset: Cell<f64>,
}

impl EstadoChat {
    pub(super) fn novo() -> Self {
        Self {
            aba: signal(String::new()),
            lista: signal(0),
            texto: signal(0),
            titulo: signal(String::new()),
            entrada: signal(String::new()),
            ctx_selecao: signal(false),
            ctx_linhas: signal(false),
            ctx_erro: signal(true),
            ctx_sistema: signal(true),
            perfil: signal(String::new()),
            aguardando: signal(false),
            erro: signal(String::new()),
            perigo: signal(false),
            ha_resposta: signal(false),
            ver_aberto: signal(false),
            ver_texto: signal(String::new()),
            ver_instrucoes: signal(String::new()),
            ver_instrucoes_abertas: signal(false),
            ver_avisos: signal(String::new()),
            conversas: RefCell::default(),
            proximo: Cell::new(0),
            rolagem: ScrollLink::new(0.0),
            seguindo: Cell::new(true),
            ultimo_offset: Cell::new(0.0),
        }
    }

    fn novo_id(&self) -> u64 {
        let id = self.proximo.get() + 1;
        self.proximo.set(id);
        id
    }

    /// As mensagens da aba mostrada.
    fn mensagens_da_aba(&self) -> Vec<Msg> {
        let aba = self.aba.get_untracked();
        self.conversas
            .borrow()
            .get(&aba)
            .map(|c| c.mensagens.clone())
            .unwrap_or_default()
    }
}

/// O que de uma conversa vai de histórico no pedido seguinte: os pares pessoa e resposta que
/// saíram e foram respondidos. O que falhou ou ficou sem resposta não entra.
fn historico_da(conversa: &Conversa) -> Vec<Message> {
    let mut saida = Vec::new();
    let mensagens = &conversa.mensagens;
    for (i, m) in mensagens.iter().enumerate() {
        let Some(resposta) = mensagens.get(i + 1).filter(|r| !r.pessoa && r.pronta.get()) else {
            continue;
        };
        if !m.pessoa || resposta.texto.borrow().is_empty() {
            continue;
        }
        let enviado = m.enviado.borrow();
        let Some(ultima) = enviado.as_ref().and_then(|e| e.preparado.mensagens.last()) else {
            continue;
        };
        saida.push(Message::user(ultima.content.clone()));
        saida.push(Message::assistant(resposta.texto.borrow().clone()));
    }
    saida
}

/// "Contexto: …": o que foi junto da pergunta (vazio: nada).
fn linha_de_contexto(incluir: Incluir, linhas: u32) -> String {
    let mut itens = Vec::new();
    if incluir.selecao {
        itens.push(t!("ia-chat-ctx-nome-selecao"));
    }
    if incluir.linhas {
        itens.push(t!("ia-chat-ctx-nome-linhas", n = i64::from(linhas)));
    }
    if incluir.erro {
        itens.push(t!("ia-chat-ctx-nome-erro"));
    }
    if incluir.sistema {
        itens.push(t!("ia-chat-ctx-nome-sistema"));
    }
    if itens.is_empty() {
        String::new()
    } else {
        t!("ia-chat-contexto", itens = itens.join(", "))
    }
}

impl Tela {
    /// Liga o que reage sozinho: o painel segue a aba ativa da janela principal, o seletor de
    /// provedor vale para a aba mostrada e a rolagem pára de seguir quando a pessoa rola para cima.
    pub(super) fn ligar_chat(self: &Rc<Self>) {
        let (t, layout) = (Rc::clone(self), self.janelas.principal().layout);
        effect(move || {
            let sessao = layout.with(|l| l.pane(l.active()).selected);
            untrack(|| match sessao {
                Some(sessao) => t.mostrar_conversa(sessao),
                None => {
                    t.assistente.chat.aba.set(String::new());
                    t.sincronizar_chat();
                }
            });
        });
        // Abriu o painel (pelo atalho ou pelo menu Exibir): o foco vai para a caixa de pergunta.
        let aberto = self.paineis_terminais.controle.shown(PAINEL_DO_CHAT);
        effect(move || {
            if aberto.get() {
                untrack(|| request_focus("ia-chat-entrada"));
            }
        });
        // Os assistentes de IA desligados em Geral: o painel do chat fecha (o menu já some).
        let (t, ligada) = (Rc::clone(self), super::com_ia(self.config));
        effect(move || {
            if !ligada() {
                untrack(|| t.paineis_terminais.controle.close(PAINEL_DO_CHAT));
            }
        });
        let (t, perfil) = (Rc::clone(self), self.assistente.chat.perfil);
        effect(move || {
            let nome = perfil.get();
            untrack(|| t.trocar_perfil_do_chat(&nome));
        });
        let (t, rolagem) = (Rc::clone(self), Rc::clone(&self.assistente.chat.rolagem));
        effect(move || {
            let offset = rolagem.offset_y();
            let c = &t.assistente.chat;
            if offset + 2.0 < c.ultimo_offset.get() {
                c.seguindo.set(false);
            }
            c.ultimo_offset.set(offset);
        });
    }

    /// O seletor mudou: a aba mostrada passa a usar o provedor de nome `nome`.
    fn trocar_perfil_do_chat(&self, nome: &str) {
        let c = &self.assistente.chat;
        let aba = c.aba.get_untracked();
        let Some(sessao) = c.conversas.borrow().get(&aba).map(|c| c.sessao) else {
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
            self.trocar_perfil_da_aba(&aba, id);
        }
    }

    /// O painel passa a mostrar a conversa da aba de `sessao` (criada no primeiro uso).
    fn mostrar_conversa(&self, sessao: Session) {
        let c = &self.assistente.chat;
        let chave = sessao.key_str();
        c.conversas
            .borrow_mut()
            .entry(chave.clone())
            .or_insert_with(|| Conversa {
                sessao,
                mensagens: Vec::new(),
                atual: None,
                erro: String::new(),
                ultimo: None,
            });
        if c.aba.get_untracked() != chave {
            c.aba.set(chave);
            c.seguindo.set(true);
            c.rolagem.scroll_by_y(1.0e9);
        }
        self.sincronizar_chat();
    }

    /// Acerta o que o painel mostra com a conversa da aba mostrada.
    fn sincronizar_chat(&self) {
        let c = &self.assistente.chat;
        let aba = c.aba.get_untracked();
        let (sessao, erro, aguardando, ha_resposta) = {
            let conversas = c.conversas.borrow();
            match conversas.get(&aba) {
                Some(conv) => (
                    Some(conv.sessao),
                    conv.erro.clone(),
                    conv.atual.is_some(),
                    conv.mensagens
                        .iter()
                        .any(|m| !m.pessoa && m.pronta.get() && !m.texto.borrow().is_empty()),
                ),
                None => (None, String::new(), false, false),
            }
        };
        c.titulo.set(
            sessao
                .map(|s| t!("ia-chat-titulo", aba = s.title()))
                .unwrap_or_else(|| t!("ia-chat-titulo-sem-aba")),
        );
        if let Some(perfil) = self.perfil_da_aba(sessao.as_ref()) {
            c.perfil.set(perfil.nome);
        }
        c.erro.set(erro);
        c.aguardando.set(aguardando);
        c.ha_resposta.set(ha_resposta);
        c.perigo.set(false);
        c.lista.update(|n| *n += 1);
    }

    /// Abre o chat na conversa da aba de `sessao` e põe o foco na caixa de pergunta.
    pub(super) fn abrir_chat_da(&self, sessao: Session) {
        self.mostrar_conversa(sessao);
        self.ir_para(paineis::TERMINAIS);
        // `show` leva o foco para o painel; com ele já aberto, o foco fica onde está.
        if !self.paineis_terminais.controle.is_open(PAINEL_DO_CHAT) {
            self.paineis_terminais.controle.show(PAINEL_DO_CHAT);
        }
        request_focus("ia-chat-entrada");
    }

    /// "Perguntar à IA…" do menu: abre o chat com a seleção marcada, se houver.
    pub(super) fn perguntar_no_chat(&self, sessao: Session) {
        if !self.exigir_perfil_de_ia() {
            return;
        }
        self.assistente
            .chat
            .ctx_selecao
            .set(untrack(|| sessao.has_selection()));
        self.abrir_chat_da(sessao);
    }

    /// "Explicar a seleção" do menu.
    pub(super) fn explicar_selecao_no_chat(self: &Rc<Self>, sessao: Session) {
        if !untrack(|| sessao.has_selection()) {
            self.avisar(InfoBarSeverity::Informational, t!("ia-chat-sem-selecao"));
            return;
        }
        self.pedir_no_chat(sessao, Pedido::ExplicarSelecao);
    }

    /// "Explicar o erro" do menu.
    pub(super) fn explicar_erro_no_chat(self: &Rc<Self>, sessao: Session) {
        self.pedir_no_chat(sessao, Pedido::ExplicarErro);
    }

    /// "Explicar no chat" do Flyout do comando.
    pub(super) fn explicar_comando_no_chat(self: &Rc<Self>, sessao: Session, comando: String) {
        self.pedir_no_chat(sessao, Pedido::ExplicarComando(comando));
    }

    /// "Enviar" na caixa do chat.
    fn enviar_do_chat(self: &Rc<Self>) {
        let c = &self.assistente.chat;
        let texto = c.entrada.get_untracked();
        if texto.trim().is_empty() || c.aguardando.get_untracked() {
            return;
        }
        let aba = c.aba.get_untracked();
        let Some(sessao) = c.conversas.borrow().get(&aba).map(|c| c.sessao) else {
            return;
        };
        let incluir = Incluir {
            selecao: c.ctx_selecao.get_untracked(),
            linhas: c.ctx_linhas.get_untracked(),
            erro: c.ctx_erro.get_untracked(),
            sistema: c.ctx_sistema.get_untracked(),
        };
        self.pedir_no_chat(sessao, Pedido::Pergunta { texto, incluir });
        // O botão Enviar some enquanto responde: o foco volta para a caixa.
        request_focus("ia-chat-entrada");
    }

    /// Manda `pedido` pela conversa da aba de `sessao`: abre o chat, acrescenta a mensagem da
    /// pessoa e a da IA (vazia, que vai enchendo) e entrega ao motor.
    pub(super) fn pedir_no_chat(self: &Rc<Self>, sessao: Session, mut pedido: Pedido) {
        if !self.exigir_perfil_de_ia() {
            return;
        }
        let Some(perfil) = self.perfil_da_aba(Some(&sessao)) else {
            return;
        };
        let c = &self.assistente.chat;
        let chave = sessao.key_str();
        self.abrir_chat_da(sessao);
        self.parar_conversa(&chave);
        // A seleção marcada sem nada selecionado não vai.
        if let Pedido::Pergunta { incluir, .. } = &mut pedido {
            incluir.selecao &= untrack(|| sessao.has_selection());
        }
        let incluir = pedido.incluir();
        let (linhas, max) = self
            .config
            .with_untracked(|c| (c.ia.linhas_contexto, c.ia.max_caracteres as usize));
        let (historico, cortadas) = {
            let conversas = c.conversas.borrow();
            let Some(conversa) = conversas.get(&chave) else {
                return;
            };
            prompts::limitar_historico(historico_da(conversa), max)
        };
        let nova = |pessoa: bool, texto: String, contexto: String| Msg {
            id: c.novo_id(),
            aba: chave.clone(),
            pessoa,
            texto: Rc::new(RefCell::new(texto)),
            pronta: Rc::new(Cell::new(!pessoa && false)),
            contexto,
            enviado: Rc::default(),
            cortadas,
        };
        let da_pessoa = nova(true, pedido.rotulo(), linha_de_contexto(incluir, linhas));
        let da_ia = nova(false, String::new(), String::new());
        let (id_pessoa, id_ia) = (da_pessoa.id, da_ia.id);
        {
            let mut conversas = c.conversas.borrow_mut();
            let Some(conversa) = conversas.get_mut(&chave) else {
                return;
            };
            conversa.mensagens.push(da_pessoa);
            conversa.mensagens.push(da_ia);
            conversa.erro.clear();
            conversa.ultimo = Some(pedido.clone());
        }
        if matches!(pedido, Pedido::Pergunta { .. }) {
            c.entrada.set(String::new());
        }
        c.seguindo.set(true);
        c.rolagem.scroll_by_y(1.0e9);
        let tela = Rc::clone(self);
        let aba = chave.clone();
        let receptor: Receptor =
            Rc::new(move |chegou| tela.chegou_no_chat(&aba, id_pessoa, id_ia, chegou));
        let montar: Montar = Box::new(prompts::pergunta_do_chat);
        let id = self.enviar_ia(Envio {
            sessao: Some(sessao),
            perfil,
            modo: Modo::Conversa,
            incluir,
            historico,
            pedido: pedido.texto_enviado(),
            montar,
            receptor,
        });
        if self.ia_em_andamento(id)
            && let Some(conversa) = c.conversas.borrow_mut().get_mut(&chave)
        {
            conversa.atual = Some(id);
        }
        self.sincronizar_chat();
    }

    /// Corta o pedido em andamento da conversa `chave`. O que já chegou fica como resposta.
    fn parar_conversa(&self, chave: &str) {
        let c = &self.assistente.chat;
        let id = {
            let mut conversas = c.conversas.borrow_mut();
            let Some(conversa) = conversas.get_mut(chave) else {
                return;
            };
            let Some(id) = conversa.atual.take() else {
                return;
            };
            if let Some(ultima) = conversa.mensagens.last()
                && !ultima.pessoa
                && !ultima.pronta.get()
            {
                if ultima.texto.borrow().is_empty() {
                    conversa.mensagens.pop();
                } else {
                    ultima.pronta.set(true);
                }
            }
            id
        };
        self.cancelar_ia(id);
        if c.aba.get_untracked() == chave {
            self.sincronizar_chat();
        }
    }

    /// "Parar" na caixa do chat.
    fn parar_do_chat(&self) {
        let aba = self.assistente.chat.aba.get_untracked();
        self.parar_conversa(&aba);
    }

    /// "Tentar de novo" no erro: tira a tentativa que falhou e manda o mesmo pedido.
    fn tentar_de_novo_no_chat(self: &Rc<Self>) {
        let c = &self.assistente.chat;
        let aba = c.aba.get_untracked();
        let (sessao, pedido) = {
            let mut conversas = c.conversas.borrow_mut();
            let Some(conversa) = conversas.get_mut(&aba) else {
                return;
            };
            let Some(pedido) = conversa.ultimo.clone() else {
                return;
            };
            if conversa.mensagens.last().is_some_and(|m| !m.pessoa) {
                conversa.mensagens.pop();
            }
            if conversa.mensagens.last().is_some_and(|m| m.pessoa) {
                conversa.mensagens.pop();
            }
            (conversa.sessao, pedido)
        };
        self.pedir_no_chat(sessao, pedido);
    }

    /// Chega algo do motor para a conversa `chave`.
    fn chegou_no_chat(&self, chave: &str, id_pessoa: u64, id_ia: u64, chegou: Chegou) {
        let c = &self.assistente.chat;
        let mut devolver: Option<String> = None;
        let mut pedaco = false;
        {
            let mut conversas = c.conversas.borrow_mut();
            let Some(conversa) = conversas.get_mut(chave) else {
                return;
            };
            match chegou {
                Chegou::Saiu(preparado) => {
                    if let Some(m) = conversa.mensagens.iter().find(|m| m.id == id_pessoa) {
                        *m.enviado.borrow_mut() = Some(Enviado {
                            preparado,
                            cortadas: m.cortadas,
                        });
                    }
                }
                Chegou::Texto(texto) => {
                    if let Some(m) = conversa.mensagens.iter().find(|m| m.id == id_ia) {
                        m.texto.borrow_mut().push_str(&texto);
                        pedaco = true;
                    }
                }
                Chegou::Fim(resultado) => {
                    conversa.atual = None;
                    let vazia = conversa
                        .mensagens
                        .iter()
                        .find(|m| m.id == id_ia)
                        .is_none_or(|m| m.texto.borrow().is_empty());
                    if let Some(m) = conversa.mensagens.iter().find(|m| m.id == id_ia) {
                        m.pronta.set(true);
                    }
                    if vazia {
                        conversa.mensagens.retain(|m| m.id != id_ia);
                    }
                    match resultado {
                        Err(erro) => conversa.erro = erro,
                        Ok(()) if vazia => conversa.erro = t!("ia-chat-resposta-vazia"),
                        Ok(()) => {}
                    }
                    pedaco = true;
                }
                Chegou::Desistiu => {
                    conversa.atual = None;
                    conversa
                        .mensagens
                        .retain(|m| m.id != id_pessoa && m.id != id_ia);
                    if let Some(Pedido::Pergunta { texto, .. }) = &conversa.ultimo {
                        devolver = Some(texto.clone());
                    }
                }
            }
        }
        if let Some(texto) = devolver
            && c.aba.get_untracked() == chave
            && c.entrada.with_untracked(String::is_empty)
        {
            c.entrada.set(texto);
        }
        if c.aba.get_untracked() != chave {
            return;
        }
        if pedaco {
            c.texto.update(|n| *n += 1);
        }
        self.sincronizar_chat();
        if c.seguindo.get() {
            c.rolagem.scroll_by_y(1.0e9);
        }
    }

    /// A aba `chave` fechou: corta o pedido e descarta a conversa.
    pub(super) fn esquecer_conversa(&self, chave: &str) {
        let c = &self.assistente.chat;
        let conversa = c.conversas.borrow_mut().remove(chave);
        if let Some(id) = conversa.and_then(|c| c.atual) {
            self.cancelar_ia(id);
        }
        if c.aba.get_untracked() == chave {
            c.aba.set(String::new());
            self.sincronizar_chat();
        }
    }

    /// "Inserir no terminal" de um bloco de código: põe o texto na linha da aba da conversa, sem
    /// Enter, ativa a aba e dá o foco ao terminal.
    fn inserir_do_chat(&self, chave: &str, bloco: &str) {
        let c = &self.assistente.chat;
        let sessao = c.conversas.borrow().get(chave).map(|c| c.sessao);
        let Some(sessao) = sessao else {
            self.avisar(InfoBarSeverity::Informational, t!("ia-chat-aba-fechada"));
            return;
        };
        let em_bloco = aceita_colar_em_bloco(&sessao);
        let uma_linha = texto_limpo::tem_varias_linhas(bloco) && !em_bloco;
        let a_inserir = if uma_linha {
            texto_limpo::em_uma_linha(bloco)
        } else {
            texto_limpo::sem_controle(bloco).trim().to_owned()
        };
        if let Some(bytes) = texto_limpo::para_inserir(&a_inserir, em_bloco) {
            sessao.send(&bytes);
        }
        c.perigo.set(resposta::perigoso(&a_inserir).is_some());
        if uma_linha {
            self.avisar(InfoBarSeverity::Informational, t!("ia-chat-uma-linha"));
        }
        for janela in self.janelas.todas_agora() {
            let tem = janela
                .layout
                .with_untracked(|l| l.pane_of(sessao).is_some());
            if tem {
                janela.layout.update(|l| l.select(sessao));
            }
        }
        request_focus(id_do_terminal(chave));
    }

    /// "Ver o que foi enviado": o texto exato de uma mensagem, num diálogo sem botão de enviar.
    fn ver_enviado(&self, enviado: &Enviado) {
        let c = &self.assistente.chat;
        let preparado = &enviado.preparado;
        c.ver_texto
            .set(texto_limpo::mostrar_sem_controle(&preparado.texto_exato()));
        c.ver_instrucoes.set(preparado.sistema.clone());
        let mut avisos = Vec::new();
        let frase = preparado.contagem.frase();
        if !frase.is_empty() {
            avisos.push(frase);
        }
        if enviado.cortadas > 0 {
            avisos.push(t!("ia-chat-cortou", n = enviado.cortadas as i64));
        }
        if preparado.cortadas > 0 {
            let max = self.config.with_untracked(|c| c.ia.max_caracteres);
            avisos.push(t!(
                "ia-sai-cortei",
                n = preparado.cortadas.to_string(),
                max = max.to_string()
            ));
        }
        if preparado.bloqueado {
            avisos.push(t!("ia-sai-cofre-bloqueado"));
        }
        c.ver_avisos.set(avisos.join("\n"));
        c.ver_aberto.set(true);
    }
}

/// Uma mensagem da pessoa: o balão à direita, com o contexto e o link "Ver o que foi enviado".
fn vista_da_pessoa(tela: &Rc<Tela>, m: &Msg) -> Column {
    let id = m.id;
    let lista = tela.assistente.chat.lista;
    let enviado = Rc::clone(&m.enviado);
    let tem_contexto = !m.contexto.is_empty();
    let ver = {
        let (t, enviado) = (Rc::clone(tela), Rc::clone(&enviado));
        Button::new(t!("ia-chat-ver-enviado"))
            .id(format!("ia-chat-ver-{id}"))
            .hyperlink()
            .visible({
                let enviado = Rc::clone(&enviado);
                move || {
                    let _ = lista.get();
                    enviado.borrow().is_some()
                }
            })
            .on_click(move || {
                let enviado = enviado.borrow().clone();
                if let Some(enviado) = enviado {
                    t.ver_enviado(&enviado);
                }
            })
    };
    let balao = Card::new().padding(10).child(
        Column::new()
            .gap(4)
            .child(
                Text::new(m.texto.borrow().clone())
                    .id(format!("ia-chat-pergunta-{id}"))
                    .wrap(),
            )
            .child(
                Text::new(m.contexto.clone())
                    .id(format!("ia-chat-contexto-{id}"))
                    .caption()
                    .secondary()
                    .wrap()
                    .visible(tem_contexto),
            )
            .child(Row::new().child(ver)),
    );
    Column::new()
        .id(format!("ia-chat-msg-{id}"))
        .align(Align::End)
        .child(Column::new().max_width(360).child(balao))
}

/// Uma resposta da IA: o Markdown sem balão, com o "Inserir no terminal" nos blocos de código.
fn vista_da_ia(tela: &Rc<Tela>, m: &Msg) -> Column {
    let id = m.id;
    let (texto, pronta, versao) = (
        Rc::clone(&m.texto),
        Rc::clone(&m.pronta),
        tela.assistente.chat.texto,
    );
    let (para_ver, pronta_ver) = (Rc::clone(&m.texto), Rc::clone(&m.pronta));
    let aba = m.aba.clone();
    let inserir = Rc::clone(tela);
    let markdown = MarkdownView::new(move || {
        // Depois de pronta, a resposta não muda mais: pára de acompanhar os pedaços.
        if !pronta.get() {
            let _ = versao.get();
        }
        texto.borrow().clone()
    })
    .id(format!("ia-chat-resposta-{id}"))
    .on_link(|endereco| {
        // Só http, https e mailto abrem; o resto é recusado por `abrir_endereco`.
        let _ = ferramentas::abrir_endereco(&endereco);
    })
    .code_action(t!("ia-chat-inserir"), Symbol::WindowConsole, move |bloco| {
        inserir.inserir_do_chat(&aba, &bloco.text);
    });
    Column::new()
        .id(format!("ia-chat-msg-{id}"))
        .gap(4)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .id(format!("ia-chat-aguardando-{id}"))
                .visible(move || {
                    let _ = versao.get();
                    !pronta_ver.get() && para_ver.borrow().is_empty()
                })
                .child(ProgressRing::new().width(16).height(16))
                .child(Text::new(t!("ia-chat-gerando")).secondary()),
        )
        .child(markdown)
}

/// O conteúdo do painel `chat`.
pub(super) fn painel(tela: &Rc<Tela>) -> impl View + use<> {
    let c = &tela.assistente.chat;
    let (aba, lista, titulo, entrada) = (c.aba, c.lista, c.titulo, c.entrada);
    let (aguardando, erro, perigo, ha_resposta, perfil) =
        (c.aguardando, c.erro, c.perigo, c.ha_resposta, c.perfil);
    let (ctx_selecao, ctx_linhas, ctx_erro, ctx_sistema) =
        (c.ctx_selecao, c.ctx_linhas, c.ctx_erro, c.ctx_sistema);
    let perfis = tela.assistente.perfis;
    let config = tela.config;

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
                .id("ia-chat-perfil")
                .width(140)
        },
    ));
    let cabecalho = Row::new()
        .gap(8)
        .align(Align::Center)
        .child(
            Text::new(move || titulo.get())
                .id("ia-chat-titulo")
                .body_strong()
                .max_lines(1)
                .grow(1),
        )
        .child(escolha_do_perfil)
        .child(
            Button::icon_only(Symbol::Copy)
                .id("ia-chat-copiar")
                .label(t!("ia-chat-copiar"))
                .tooltip(t!("ia-chat-copiar"))
                .visible(move || ha_resposta.get())
                .on_click({
                    let t = Rc::clone(tela);
                    move || {
                        let aba = t.assistente.chat.aba.get_untracked();
                        let texto = t
                            .assistente
                            .chat
                            .conversas
                            .borrow()
                            .get(&aba)
                            .and_then(|c| {
                                c.mensagens
                                    .iter()
                                    .rev()
                                    .find(|m| !m.pessoa && m.pronta.get())
                                    .map(|m| m.texto.borrow().clone())
                            });
                        if let Some(texto) = texto {
                            clipboard::write(texto);
                        }
                    }
                }),
        )
        .child(
            Button::icon_only(Symbol::Add)
                .id("ia-chat-nova")
                .label(t!("ia-chat-nova"))
                .tooltip(t!("ia-chat-nova"))
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.nova_conversa()
                }),
        );

    let vazio = Text::new(t!("ia-chat-vazio"))
        .id("ia-chat-vazio")
        .secondary()
        .wrap()
        .visible({
            let t = Rc::clone(tela);
            move || {
                let _ = lista.get();
                aba.with(|_| ());
                t.assistente.chat.mensagens_da_aba().is_empty()
            }
        });
    let mensagens = For::each(
        {
            let t = Rc::clone(tela);
            move || {
                let _ = lista.get();
                let _ = aba.get();
                t.assistente.chat.mensagens_da_aba()
            }
        },
        |m: &Msg| m.id,
        {
            let t = Rc::clone(tela);
            move |m: Msg| {
                if m.pessoa {
                    vista_da_pessoa(&t, &m)
                } else {
                    vista_da_ia(&t, &m)
                }
            }
        },
    );
    let conversa = ScrollViewer::new()
        .id("ia-chat-mensagens")
        .link_with_bars(Rc::clone(&c.rolagem))
        .grow(1)
        .child(
            Column::new()
                .gap(12)
                .padding(Insets::symmetric(8.0, 12.0))
                .child(vazio)
                .child(mensagens),
        );

    let tentar = Row::new()
        .gap(8)
        .align(Align::Center)
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("ia-chat-erro")
                .message(move || erro.get())
                .visible(move || !erro.with(String::is_empty))
                .grow(1),
        )
        .child(
            Button::new(t!("comum-tentar-de-novo"))
                .id("ia-chat-tentar")
                .visible(move || !erro.with(String::is_empty))
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.tentar_de_novo_no_chat()
                }),
        );
    let aviso_de_perigo = InfoBar::new(InfoBarSeverity::Warning)
        .id("ia-chat-perigo")
        .message(t!("ia-comando-perigo"))
        .visible(move || perigo.get());

    let contexto = Column::new()
        .gap(0)
        .child(
            Row::new()
                .gap(12)
                .child(
                    CheckBox::bind(ctx_selecao, t!("ia-chat-ctx-selecao"))
                        .id("ia-chat-ctx-selecao"),
                )
                .child(
                    CheckBox::bind(ctx_linhas, move || {
                        t!(
                            "ia-chat-ctx-linhas",
                            n = config.with(|c| c.ia.linhas_contexto) as i64
                        )
                    })
                    .id("ia-chat-ctx-linhas"),
                ),
        )
        .child(
            Row::new()
                .gap(12)
                .child(CheckBox::bind(ctx_erro, t!("ia-chat-ctx-erro")).id("ia-chat-ctx-erro"))
                .child(
                    CheckBox::bind(ctx_sistema, t!("ia-chat-ctx-sistema"))
                        .id("ia-chat-ctx-sistema"),
                ),
        );

    let caixa = {
        let t = Rc::clone(tela);
        TextEditor::bind(entrada)
            .id("ia-chat-entrada")
            .placeholder(t!("ia-chat-pergunte"))
            .wrap(true)
            .min_height(44.0)
            .max_height(140.0)
            .on_key(move |tecla, _| {
                if tecla.key == Key::Enter && !tecla.modifiers.shift {
                    t.enviar_do_chat();
                    true
                } else {
                    false
                }
            })
    };
    let entrada_e_botao = Row::new()
        .gap(8)
        .align(Align::End)
        .child(Column::new().grow(1).child(caixa))
        .child(
            Button::icon_only(Symbol::Send)
                .id("ia-chat-enviar")
                .label(t!("ia-chat-enviar"))
                .tooltip(t!("ia-chat-enviar"))
                .accent()
                .disabled(move || aguardando.get() || entrada.with(|e| e.trim().is_empty()))
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.enviar_do_chat()
                }),
        )
        .child(
            Button::icon_only(Symbol::Stop)
                .id("ia-chat-parar")
                .label(t!("comum-parar"))
                .tooltip(t!("comum-parar"))
                .visible(move || aguardando.get())
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.parar_do_chat()
                }),
        );

    Column::new()
        .id("ia-chat")
        .grow(1)
        .gap(8)
        .padding(8)
        .child(cabecalho)
        .child(conversa)
        .child(tentar)
        .child(aviso_de_perigo)
        .child(contexto)
        .child(entrada_e_botao)
}

impl Tela {
    /// "Nova conversa": para o pedido e apaga as mensagens da aba mostrada.
    fn nova_conversa(&self) {
        let c = &self.assistente.chat;
        let aba = c.aba.get_untracked();
        self.parar_conversa(&aba);
        if let Some(conversa) = c.conversas.borrow_mut().get_mut(&aba) {
            conversa.mensagens.clear();
            conversa.erro.clear();
            conversa.ultimo = None;
        }
        self.sincronizar_chat();
        request_focus("ia-chat-entrada");
    }
}

/// O diálogo "Ver o que foi enviado".
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View + use<> {
    let c = &tela.assistente.chat;
    let (aberto, texto, instrucoes, instrucoes_abertas, avisos) = (
        c.ver_aberto,
        c.ver_texto,
        c.ver_instrucoes,
        c.ver_instrucoes_abertas,
        c.ver_avisos,
    );
    ContentDialog::new(t!("ia-chat-ver-titulo"), aberto)
        .id("ia-chat-ver")
        .content(
            Column::new()
                .gap(8)
                .width(496)
                .child(
                    TextEditor::bind(texto)
                        .id("ia-chat-ver-texto")
                        .read_only(true)
                        .monospace()
                        .wrap(true)
                        .min_height(140.0)
                        .max_height(280.0),
                )
                .child(
                    Expander::new(t!("ia-sai-instrucoes"), instrucoes_abertas)
                        .id("ia-chat-ver-instrucoes")
                        .content(
                            TextEditor::bind(instrucoes)
                                .id("ia-chat-ver-sistema")
                                .read_only(true)
                                .wrap(true)
                                .max_height(160.0),
                        ),
                )
                .child(
                    Text::new(move || avisos.get())
                        .id("ia-chat-ver-avisos")
                        .secondary()
                        .wrap()
                        .visible(move || !avisos.with(String::is_empty)),
                ),
        )
        .close(t!("comum-fechar"))
}
