//! A segurança do MCP que aparece na tela (M51, seções 6.2, 6.4 e 6.8): o diálogo em que a
//! pessoa aprova ou recusa o que o assistente quer fazer, a fila desses pedidos e a faixa
//! "Parar a IA".
//!
//! Só um diálogo aparece por vez, na janela da aba (ou na ativa, quando o pedido não tem aba).
//! O botão primário é sempre "Recusar": o Enter e o Esc recusam, e só um clique em "Permitir"
//! deixa o pedido passar. Quando o pedido muda uma aba de outra sessão de assistente, o diálogo
//! ganha a terceira escolha "Passar a aba para {sessão}", que troca o dono dela.

use std::rc::Rc;
use std::time::{Duration, Instant};

use tomni_ui::prelude::*;

use super::mcp::{Acao, Plano, Saida, Trabalho};
use super::mcp_sessoes;
use super::{Janela, Tela};
use crate::mcp::auditoria::DecisaoRegistrada;
use crate::mcp::sessoes::{PALETA, cor_da_paleta};
use crate::t;
use crate::texto_limpo::{mostrar_sem_controle, tem_controle};

/// Um pedido à espera da pessoa.
pub(super) struct Aprovacao {
    /// O pedido em andamento.
    pub trabalho: Rc<Trabalho>,
    /// O que ele vai fazer.
    pub plano: Plano,
    /// A janela em que o diálogo aparece.
    pub janela: Janela,
}

/// O que o diálogo mostra; o mesmo diálogo serve para todos os pedidos, um de cada vez.
#[derive(Clone, Copy)]
pub(super) struct SinaisDoDialogo {
    titulo: Signal<String>,
    contexto: Signal<String>,
    detalhe: Signal<String>,
    invisiveis: Signal<bool>,
    perigo: Signal<String>,
    pode_sessao: Signal<bool>,
    marcar_sessao: Signal<bool>,
    /// A caixa "abrir as abas novas desta sessão sem perguntar", com o tempo no texto; vazio
    /// esconde a caixa.
    texto_auto: Signal<String>,
    marcar_auto: Signal<bool>,
    motivo: Signal<String>,
    /// O nome da sessão que pede e o lugar dela na paleta (a faixa colorida).
    sessao: Signal<String>,
    cor: Signal<usize>,
    /// O rótulo de "Passar a aba para {sessão}"; vazio quando o pedido não é de posse.
    passar: Signal<String>,
    /// O que está na fila atrás deste pedido, uma linha por pedido.
    fila: Signal<String>,
}

/// O que a pessoa escolheu no diálogo.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Resposta {
    Recusar,
    Permitir,
    /// Permitir e passar a aba para a sessão que pediu.
    Passar,
}

impl SinaisDoDialogo {
    pub(super) fn novos() -> Self {
        Self {
            titulo: signal(String::new()),
            contexto: signal(String::new()),
            detalhe: signal(String::new()),
            invisiveis: signal(false),
            perigo: signal(String::new()),
            pode_sessao: signal(false),
            marcar_sessao: signal(false),
            texto_auto: signal(String::new()),
            marcar_auto: signal(false),
            motivo: signal(String::new()),
            sessao: signal(String::new()),
            cor: signal(0),
            passar: signal(String::new()),
            fila: signal(String::new()),
        }
    }
}

/// O título do diálogo para o que o pedido faz. Numa aba de outra sessão, diz de quem é a aba;
/// ao organizar painéis, diz a grade.
fn titulo_da(a: &Aprovacao) -> String {
    let sessao = a.trabalho.pedido.sessao.as_str();
    if let Some(dono) = &a.plano.dono {
        let verbo = match a.plano.acao {
            Acao::Digitar => Some(t!("mcp-verbo-digitar")),
            Acao::Teclas => Some(t!("mcp-verbo-teclas")),
            Acao::Rodar => Some(t!("mcp-verbo-rodar")),
            _ => None,
        };
        return match verbo {
            Some(verbo) => t!(
                "mcp-aprovar-titulo-posse",
                sessao = sessao,
                verbo = verbo,
                dono = dono.nome.as_str()
            ),
            None => t!(
                "mcp-aprovar-titulo-posse-fechar",
                sessao = sessao,
                dono = dono.nome.as_str()
            ),
        };
    }
    if let (Acao::Organizar, Some((janela, colunas, linhas))) = (a.plano.acao, a.plano.grade) {
        return t!(
            "mcp-aprovar-titulo-organizar",
            sessao = sessao,
            janela = janela.to_string(),
            colunas = colunas.to_string(),
            linhas = linhas.to_string()
        );
    }
    match a.plano.acao {
        Acao::Organizar => t!("mcp-aprovar-titulo-organizar-sem-grade", sessao = sessao),
        Acao::Ver => t!("mcp-aprovar-titulo-ver"),
        Acao::Rodar => t!("mcp-aprovar-titulo-rodar"),
        Acao::Digitar => t!("mcp-aprovar-titulo-digitar"),
        Acao::Teclas => t!("mcp-aprovar-titulo-teclas"),
        Acao::Enviar => t!("mcp-aprovar-titulo-enviar"),
        Acao::Editar => t!("mcp-aprovar-titulo-editar"),
        Acao::Abrir => t!("mcp-aprovar-titulo-abrir"),
        Acao::Tunel => t!("mcp-aprovar-titulo-tunel"),
        Acao::Fechar => t!("mcp-aprovar-titulo-fechar"),
        Acao::Reconectar => t!("mcp-aprovar-titulo-reconectar"),
    }
}

impl Tela {
    /// Põe o pedido na fila e, se nenhum diálogo está aberto, mostra o primeiro.
    pub(super) fn enfileirar_aprovacao(self: &Rc<Self>, trabalho: Rc<Trabalho>, plano: Plano) {
        let janela = plano
            .sessao
            .and_then(|s| self.sessao_por_chave(&s.key_str()))
            .map_or_else(|| self.janela_ativa(), |(janela, _)| janela);
        self.mcp.aprovacoes.borrow_mut().push_back(Aprovacao {
            trabalho,
            plano,
            janela,
        });
        self.vigiar_aprovacoes();
        self.ativar_proxima_aprovacao();
        self.atualizar_fila();
    }

    /// Mostra o próximo pedido da fila, se não há diálogo aberto.
    pub(super) fn ativar_proxima_aprovacao(self: &Rc<Self>) {
        if self.mcp.ativa.borrow().is_some() {
            return;
        }
        loop {
            let Some(a) = self.mcp.aprovacoes.borrow_mut().pop_front() else {
                return;
            };
            if a.trabalho.pedido.foi_cancelado() || self.parado(&a.trabalho) {
                a.trabalho.decisao.set(DecisaoRegistrada::Recusada);
                let parada = t!("mcp-a-pessoa-parou");
                self.terminar_mcp(&a.trabalho, Saida::Erro(parada));
                continue;
            }
            self.mostrar_aprovacao(a);
            return;
        }
    }

    fn mostrar_aprovacao(&self, a: Aprovacao) {
        let s = self.mcp.dialogo;
        let destino = |c: &crate::modelo::Connection| {
            if c.host.is_empty() {
                c.kind.nome()
            } else if c.user.is_empty() {
                format!("{}:{}", c.host, c.port)
            } else {
                format!("{}@{}:{}", c.user, c.host, c.port)
            }
        };
        let contexto = match &a.plano.conexao {
            Some(c) => t!(
                "mcp-aprovar-contexto",
                assistente = a.trabalho.pedido.sessao.as_str(),
                conexao = c.name.as_str(),
                destino = destino(c)
            ),
            None => a.trabalho.pedido.sessao.clone(),
        };
        // Ligar um túnel ou usar um espaço que precisa de aba nova diz isso também.
        let contexto = if a.plano.abre_aba && a.plano.acao != Acao::Abrir {
            format!("{contexto}\n{}", t!("mcp-aprovar-abre-aba"))
        } else {
            contexto
        };
        s.titulo.set(titulo_da(&a));
        s.contexto.set(contexto);
        let quem = self.sessao_ia(a.trabalho.pedido.ligacao);
        s.sessao.set(a.trabalho.pedido.sessao.clone());
        s.cor.set(quem.as_ref().map_or(0, |q| q.cor % PALETA.len()));
        s.passar.set(match (&a.plano.dono, &quem) {
            (Some(_), Some(q)) => t!("mcp-passar-aba", sessao = q.nome.as_str()),
            _ => String::new(),
        });
        s.invisiveis.set(tem_controle(&a.plano.detalhe));
        s.detalhe.set(mostrar_sem_controle(&a.plano.detalhe));
        s.perigo.set(a.plano.bloqueio.clone().unwrap_or_default());
        s.pode_sessao.set(
            a.plano.sessao.is_some()
                && a.plano.bloqueio.is_none()
                && a.plano.dono.is_none()
                && !a.plano.abre_aba,
        );
        s.marcar_sessao.set(false);
        let auto = self.config.with_untracked(|c| c.ia.mcp_abrir_auto);
        s.texto_auto.set(match auto.duracao() {
            Some(_) if !a.plano.so_abertura || a.plano.dono.is_some() => String::new(),
            None => String::new(),
            Some(None) => t!("mcp-aprovar-abrir-auto-sempre"),
            Some(Some(_)) => t!("mcp-aprovar-abrir-auto", tempo = auto.nome()),
        });
        s.marcar_auto.set(false);
        s.motivo.set(String::new());
        let janela = a.janela;
        *self.mcp.ativa.borrow_mut() = Some(a);
        self.atualizar_fila();
        janela.mcp_aprovar.set(true);
        janela.handle.focus();
        janela.handle.request_attention();
    }

    /// Mostra no diálogo quem mais espera: a sessão e o que cada pedido da fila quer.
    fn atualizar_fila(&self) {
        let linhas: Vec<String> = self
            .mcp
            .aprovacoes
            .borrow()
            .iter()
            .map(|a| format!("{}: {}", a.trabalho.pedido.sessao, titulo_da(a)))
            .collect();
        self.mcp.dialogo.fila.set(if linhas.is_empty() {
            String::new()
        } else {
            t!("mcp-aprovar-fila", pedidos = linhas.join("\n"))
        });
    }

    /// A resposta da pessoa ao diálogo.
    pub(super) fn responder_aprovacao(self: &Rc<Self>, resposta: Resposta) {
        let Some(a) = self.mcp.ativa.borrow_mut().take() else {
            return;
        };
        let s = self.mcp.dialogo;
        let Aprovacao {
            trabalho, plano, ..
        } = a;
        if resposta == Resposta::Recusar {
            let motivo = s.motivo.get_untracked().trim().to_owned();
            let texto = match (&plano.dono, motivo.is_empty()) {
                (Some(dono), true) => t!("mcp-recusada-aba-de-outra", dono = dono.nome.as_str()),
                (Some(dono), false) => t!(
                    "mcp-recusada-aba-de-outra-motivo",
                    dono = dono.nome.as_str(),
                    motivo = motivo.as_str()
                ),
                (None, true) => t!("mcp-recusada"),
                (None, false) => t!("mcp-recusada-motivo", motivo = motivo.as_str()),
            };
            trabalho.decisao.set(if plano.bloqueio.is_some() {
                DecisaoRegistrada::BloqueadaRecusada
            } else {
                DecisaoRegistrada::Recusada
            });
            if !motivo.is_empty() {
                *trabalho.motivo.borrow_mut() = Some(motivo);
            }
            self.terminar_mcp(&trabalho, Saida::Erro(texto));
        } else {
            if s.pode_sessao.get_untracked()
                && s.marcar_sessao.get_untracked()
                && let Some(sessao) = plano.sessao
            {
                self.mcp
                    .aprovadas_sessao
                    .borrow_mut()
                    .insert((trabalho.pedido.ligacao, sessao.key_str()));
            }
            if !s.texto_auto.get_untracked().is_empty() && s.marcar_auto.get_untracked() {
                self.mcp
                    .abrir_auto
                    .borrow_mut()
                    .insert(trabalho.pedido.ligacao, Instant::now());
            }
            if resposta == Resposta::Passar
                && let Some(sessao) = plano.sessao
            {
                self.dar_aba(&sessao.key_str(), trabalho.pedido.ligacao);
            }
            trabalho.decisao.set(DecisaoRegistrada::Aprovada);
            self.executar_mcp(trabalho, plano);
        }
        // O diálogo seguinte espera um pouco: o `ContentDialog` fecha a caixa depois da ação, e
        // abrir outra na mesma hora seria fechada junto.
        let eu = self.eu.clone();
        self.alarmes.depois(Duration::from_millis(50), move || {
            if let Some(tela) = eu.upgrade() {
                tela.ativar_proxima_aprovacao();
            }
        });
    }

    /// Recusa o que a sessão `ligacao` espera, no diálogo e na fila, com `texto` de motivo. Os
    /// pedidos das outras sessões seguem: o diálogo passa para o próximo deles.
    pub(super) fn recusar_aprovacoes_de(self: &Rc<Self>, ligacao: u64, texto: &str) {
        let no_dialogo = self
            .mcp
            .ativa
            .borrow()
            .as_ref()
            .is_some_and(|a| a.trabalho.pedido.ligacao == ligacao);
        let mut recusadas: Vec<Aprovacao> = Vec::new();
        if no_dialogo && let Some(a) = self.mcp.ativa.borrow_mut().take() {
            a.janela.mcp_aprovar.set(false);
            recusadas.push(a);
        }
        {
            let mut fila = self.mcp.aprovacoes.borrow_mut();
            let (fora, dentro): (Vec<_>, Vec<_>) = fila
                .drain(..)
                .partition(|a| a.trabalho.pedido.ligacao == ligacao);
            fila.extend(dentro);
            recusadas.extend(fora);
        }
        let mudou = !recusadas.is_empty();
        for a in recusadas {
            a.trabalho.decisao.set(DecisaoRegistrada::Recusada);
            self.terminar_mcp(&a.trabalho, Saida::Erro(texto.to_owned()));
        }
        if mudou {
            self.atualizar_fila();
            self.ativar_proxima_aprovacao();
        }
    }

    /// Recusa o que está no diálogo e na fila, com `texto` de motivo, e fecha o diálogo.
    pub(super) fn recusar_todas_as_aprovacoes(self: &Rc<Self>, texto: &str) {
        let ativa = self.mcp.ativa.borrow_mut().take();
        let fila: Vec<Aprovacao> = self.mcp.aprovacoes.borrow_mut().drain(..).collect();
        if let Some(a) = &ativa {
            a.janela.mcp_aprovar.set(false);
        }
        for a in ativa.into_iter().chain(fila) {
            a.trabalho.decisao.set(DecisaoRegistrada::Recusada);
            self.terminar_mcp(&a.trabalho, Saida::Erro(texto.to_owned()));
        }
    }

    /// Liga o laço que descarta os pedidos que o assistente cancelou enquanto esperavam.
    fn vigiar_aprovacoes(self: &Rc<Self>) {
        if !self.mcp.vigiando.replace(true) {
            self.agendar_vigia();
        }
    }

    fn agendar_vigia(self: &Rc<Self>) {
        let eu = self.eu.clone();
        self.alarmes.depois(Duration::from_millis(500), move || {
            let Some(tela) = eu.upgrade() else {
                return;
            };
            tela.descartar_cancelados();
            let vazio =
                tela.mcp.aprovacoes.borrow().is_empty() && tela.mcp.ativa.borrow().is_none();
            if vazio {
                tela.mcp.vigiando.set(false);
            } else {
                tela.agendar_vigia();
            }
        });
    }

    fn descartar_cancelados(self: &Rc<Self>) {
        let cancelada_no_dialogo = self
            .mcp
            .ativa
            .borrow()
            .as_ref()
            .is_some_and(|a| a.trabalho.pedido.foi_cancelado());
        if cancelada_no_dialogo {
            if let Some(a) = self.mcp.ativa.borrow_mut().take() {
                a.janela.mcp_aprovar.set(false);
                a.trabalho.decisao.set(DecisaoRegistrada::Recusada);
                self.terminar_mcp(&a.trabalho, Saida::Erro(t!("mcp-a-pessoa-parou")));
            }
            self.ativar_proxima_aprovacao();
        }
        let canceladas: Vec<Aprovacao> = {
            let mut fila = self.mcp.aprovacoes.borrow_mut();
            let (fora, dentro): (Vec<_>, Vec<_>) = fila
                .drain(..)
                .partition(|a| a.trabalho.pedido.foi_cancelado());
            fila.extend(dentro);
            fora
        };
        for a in canceladas {
            a.trabalho.decisao.set(DecisaoRegistrada::Recusada);
            self.terminar_mcp(&a.trabalho, Saida::Erro(t!("mcp-a-pessoa-parou")));
        }
    }
}

/// O diálogo de aprovação da janela. Esc fecha sem ação, e isso vale como "Recusar".
pub(super) fn dialogo(tela: &Rc<Tela>, janela: Janela) -> impl View + use<> {
    let s = tela.mcp.dialogo;
    let aberto = janela.mcp_aprovar;
    // O Esc fecha a caixa sem rodar nenhuma ação. A conferência espera um instante: o botão
    // também fecha a caixa, um pouco antes de rodar a ação, e já terá tirado o pedido.
    let esc = Rc::clone(tela);
    effect(move || {
        if aberto.get() {
            return;
        }
        let eu = esc.eu.clone();
        untrack(|| {
            esc.alarmes.depois(Duration::from_millis(30), move || {
                let Some(tela) = eu.upgrade() else {
                    return;
                };
                let desta_janela = tela
                    .mcp
                    .ativa
                    .borrow()
                    .as_ref()
                    .is_some_and(|a| a.janela.handle == janela.handle);
                if desta_janela && !aberto.get_untracked() {
                    tela.responder_aprovacao(Resposta::Recusar);
                }
            });
        });
    });
    let (permitir, recusar, passar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
    // O nome da sessão na cor dela: um texto por lugar da paleta, só o da sessão aparece.
    let mut quem = Row::new().gap(6);
    for lugar in 0..PALETA.len() {
        quem = quem.child(
            Text::new(move || s.sessao.get())
                .id(format!("mcp-aprovar-sessao-{}", lugar + 1))
                .color(cor_da_paleta(lugar))
                .body_strong()
                .visible(move || s.cor.get() == lugar),
        );
    }
    ContentDialog::new(move || s.titulo.get(), aberto)
        .id("mcp-aprovar")
        .content(
            Column::new()
                .gap(8)
                .child(quem)
                .child(
                    Text::new(move || s.contexto.get())
                        .id("mcp-aprovar-contexto")
                        .secondary()
                        .wrap(),
                )
                .child(
                    TextEditor::bind(s.detalhe)
                        .id("mcp-aprovar-detalhe")
                        .read_only(true)
                        .monospace()
                        .wrap(true)
                        .min_height(60.0)
                        .max_height(220.0),
                )
                .child(
                    InfoBar::new(InfoBarSeverity::Warning)
                        .id("mcp-aprovar-invisiveis")
                        .message(t!("mcp-aprovar-invisiveis"))
                        .visible(move || s.invisiveis.get()),
                )
                .child(
                    InfoBar::new(InfoBarSeverity::Error)
                        .id("mcp-aprovar-perigo")
                        .message(move || t!("mcp-aprovar-perigo", motivo = s.perigo.get()))
                        .visible(move || !s.perigo.get().is_empty()),
                )
                .child(
                    Text::new(move || s.fila.get())
                        .id("mcp-aprovar-fila")
                        .secondary()
                        .wrap()
                        .visible(move || !s.fila.get().is_empty()),
                )
                .child(
                    CheckBox::bind(s.marcar_sessao, t!("mcp-aprovar-sessao"))
                        .id("mcp-aprovar-sessao")
                        .visible(move || s.pode_sessao.get()),
                )
                .child(
                    CheckBox::bind(s.marcar_auto, move || s.texto_auto.get())
                        .id("mcp-aprovar-abrir-auto")
                        .visible(move || !s.texto_auto.get().is_empty()),
                )
                .child(
                    Edit::bind(s.motivo)
                        .id("mcp-aprovar-motivo")
                        .placeholder(t!("mcp-aprovar-motivo-dica")),
                ),
        )
        .primary(t!("mcp-recusar"), move || {
            recusar.responder_aprovacao(Resposta::Recusar)
        })
        .secondary(
            move || {
                if s.passar.get().is_empty() {
                    t!("mcp-permitir")
                } else {
                    t!("mcp-permitir-uma-vez")
                }
            },
            move || permitir.responder_aprovacao(Resposta::Permitir),
        )
        .close_with(
            move || s.passar.get(),
            move || passar.responder_aprovacao(Resposta::Passar),
        )
        .close_visible(move || !s.passar.get().is_empty())
}

/// A faixa "um assistente está usando N aba(s)", com o "Parar a IA". Com mais de uma sessão
/// mexendo em abas da janela, diz quantas são e o "Parar" vira um menu: uma sessão ou todas.
pub(super) fn faixa_ativa(tela: &Rc<Tela>, janela: Janela) -> impl View + use<> {
    let (texto, visivel, parar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
    let (varias, varias_texto, varias_visivel) =
        (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
    let uma = InfoBar::new(InfoBarSeverity::Informational)
        .id("mcp-ativa")
        .message(move || match texto.abas_marcadas(janela) {
            1 => t!("mcp-ativa-uma"),
            n => t!("mcp-ativa-varias", n = n.to_string()),
        })
        .visible(move || visivel.sessoes_na_janela(janela) < 2 && visivel.abas_marcadas(janela) > 0)
        .action(t!("mcp-parar"), move || parar.parar_ia_mcp());
    let varias_barra = Row::new()
        .gap(8)
        .align(Align::Center)
        .visible(move || varias_visivel.sessoes_na_janela(janela) >= 2)
        .child(
            InfoBar::new(InfoBarSeverity::Informational)
                .id("mcp-ativas")
                .grow(1)
                .message(move || {
                    let n = varias_texto.sessoes_na_janela(janela).to_string();
                    match varias_texto.abas_marcadas(janela) {
                        1 => t!("mcp-ativas-uma-aba", n = n),
                        m => t!("mcp-ativas", n = n, m = m.to_string()),
                    }
                }),
        )
        .child(
            Button::new(t!("mcp-parar-varias"))
                .id("mcp-parar-varias")
                .icon(Symbol::Dismiss)
                .flyout(mcp_sessoes::menu_parar(&varias)),
        );
    Column::new().child(uma).child(varias_barra)
}
