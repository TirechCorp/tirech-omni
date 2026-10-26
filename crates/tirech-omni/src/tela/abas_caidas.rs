//! Abas caídas e "Fechar todas as abas" (M42): reconectar ou fechar de uma vez as abas cuja
//! sessão acabou (o SSH caiu, o shell saiu, a senha foi recusada) e esvaziar as janelas.
//! Os comandos agem em todas as janelas (menu Ferramentas) ou só na da aba (menu da aba).

use std::cell::Cell;
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::dica_da_aba::{DadosDaAba, EstadoDaAba, dica_da_aba};
use crate::espacos::id_da_chave;
use crate::janelas::Janela;
use crate::t;

/// O que o diálogo "Fechar todas as abas?" guarda enquanto está na tela.
pub(super) struct Estado {
    /// O diálogo está na tela.
    aberto: Signal<bool>,
    /// A caixa "Não perguntar de novo".
    nao_perguntar: Signal<bool>,
    /// Quantas sessões ainda estão abertas, para o texto.
    vivas: Signal<usize>,
    /// Onde confirmar: `Some(None)` é em todas as janelas, `Some(Some(j))` só na `j`.
    alvo: Cell<Option<Option<Janela>>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            nao_perguntar: signal(false),
            vivas: signal(0),
            alvo: Cell::new(None),
        }
    }
}

impl Tela {
    /// Todas as sessões das janelas: de `janela`, ou de todas com `None`.
    pub(super) fn sessoes_das_janelas(&self, janela: Option<Janela>) -> Vec<(Janela, Session)> {
        let janelas = match janela {
            Some(j) => vec![j],
            None => self.janelas.todas_agora(),
        };
        janelas
            .into_iter()
            .flat_map(|j| {
                j.layout
                    .try_with_untracked(|l| l.sessions())
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |s| (j, s))
            })
            .collect()
    }

    /// A dica da aba de `sessao`: o destino e se a conexão está de pé. Lê `closed()` e a árvore,
    /// então muda com a queda, a reconexão e a troca de nome; a rede é lida sem rastrear (o
    /// perfil não muda com a sessão aberta).
    pub(super) fn dica_da_sessao(&self, sessao: Session) -> Option<String> {
        let titulo = sessao.title();
        let estado = EstadoDaAba::da_sessao(sessao);
        let rede = self.rede_das_sessoes.borrow().get(&sessao).map(|p| {
            if p.openvpn.is_some() {
                // Perfil OpenVPN: o endereço do túnel, se o motor está de pé.
                let ip = match self.motores_vpn.estado(p.id) {
                    Some(tomni_ui_openvpn::State::Connected { address, .. }) => Some(address),
                    _ => None,
                };
                return match ip {
                    Some(ip) => t!(
                        "dica-rede-openvpn",
                        nome = p.name.as_str(),
                        ip = ip.to_string()
                    ),
                    None => t!("dica-rede-openvpn-sem-ip", nome = p.name.as_str()),
                };
            }
            if p.wireguard.is_some() {
                // Perfil WireGuard: o endereço do túnel, se o motor está de pé.
                let ip = match self.motores_wireguard.estado(p.id) {
                    Some(tomni_ui_wireguard::State::Connected { address, .. }) => Some(address),
                    _ => None,
                };
                return match ip {
                    Some(ip) => t!(
                        "dica-rede-wireguard",
                        nome = p.name.as_str(),
                        ip = ip.to_string()
                    ),
                    None => t!("dica-rede-wireguard-sem-ip", nome = p.name.as_str()),
                };
            }
            if p.tailscale.is_some() {
                // Perfil Tailscale: o endereço do nó na tailnet, se o motor está de pé.
                let ip = match self.motores_tailscale.estado(p.id) {
                    Some(tomni_ui_tailscale::State::Connected { address, .. }) => Some(address),
                    _ => None,
                };
                return match ip {
                    Some(ip) => t!(
                        "dica-rede-tailscale",
                        nome = p.name.as_str(),
                        ip = ip.to_string()
                    ),
                    None => t!("dica-rede-tailscale-sem-ip", nome = p.name.as_str()),
                };
            }
            if p.zerotier.is_some() {
                // Perfil ZeroTier: o endereço do nó na rede, se o motor está de pé.
                let ip = crate::zerotier::ip_do_no(self.motores_zerotier.estado(p.id).as_ref());
                return match ip {
                    Some(ip) => t!("dica-rede-zerotier", nome = p.name.as_str(), ip = ip),
                    None => t!("dica-rede-zerotier-sem-ip", nome = p.name.as_str()),
                };
            }
            if p.nebula.is_none() {
                return p.name.clone();
            }
            // Perfil Nebula: o endereço do nó, se o motor está de pé.
            match self.motores.estado(p.id).and_then(|e| e.address()) {
                Some(ip) => t!(
                    "dica-rede-nebula",
                    nome = p.name.as_str(),
                    ip = ip.to_string()
                ),
                None => t!("dica-rede-nebula-sem-ip", nome = p.name.as_str()),
            }
        });
        let progresso = sessao.progress();
        // O aviso só entra com o ponto aceso: sem ele, a pessoa já viu.
        let aviso = if sessao.attention() {
            sessao.last_notice()
        } else {
            None
        };
        let id = id_da_chave(&sessao.key_str());
        let comandos = self.comandos_rodando(sessao);
        let mut dica = self.tree.with(|arvore| {
            dica_da_aba(&DadosDaAba {
                titulo: &titulo,
                conexao: id.and_then(|id| arvore.connection(id)),
                rede: rede.as_deref(),
                estado,
                progresso,
                aviso,
                comandos,
            })
        });
        if let Some((assistente, acao, segundos)) = self.atividade_da_aba(&sessao.key_str()) {
            dica.push('\n');
            dica.push_str(&t!(
                "mcp-dica-da-aba",
                assistente = assistente,
                acao = acao,
                segundos = segundos.to_string()
            ));
        }
        Some(dica)
    }

    /// As abas caídas (a sessão encerrada) de `janela`, ou de todas com `None`.
    pub(super) fn abas_caidas(&self, janela: Option<Janela>) -> Vec<(Janela, Session)> {
        self.sessoes_das_janelas(janela)
            .into_iter()
            .filter(|(_, s)| untrack(|| s.closed()))
            .collect()
    }

    /// Reabre as abas caídas e diz quantas voltaram.
    pub(super) fn reconectar_caidas(&self, janela: Option<Janela>) {
        let caidas = self.abas_caidas(janela);
        if caidas.is_empty() {
            self.avisar(InfoBarSeverity::Informational, t!("abas-caidas-nenhuma"));
            return;
        }
        let reabertas = caidas.iter().filter(|(_, s)| s.restart()).count();
        let falhas = caidas.len() - reabertas;
        if falhas == 0 {
            self.avisar(
                InfoBarSeverity::Success,
                t!("abas-caidas-n-reconectadas", n = reabertas as i64),
            );
        } else {
            let nao = t!("abas-caidas-nao-reabertas", n = falhas as i64);
            let reconectadas = t!("abas-caidas-reconectadas-curto", n = reabertas as i64);
            self.avisar(
                InfoBarSeverity::Warning,
                t!(
                    "abas-caidas-parcial",
                    reconectadas = reconectadas,
                    nao = nao
                ),
            );
        }
    }

    /// Tira as abas caídas dos painéis e fecha a janela secundária que ficar sem abas.
    pub(super) fn fechar_caidas(&self, janela: Option<Janela>) {
        let caidas = self.abas_caidas(janela);
        if caidas.is_empty() {
            self.avisar(InfoBarSeverity::Informational, t!("abas-caidas-nenhuma"));
            return;
        }
        let n = caidas.len();
        for janela in self.tirar_sessoes(caidas, true) {
            self.fechar_se_vazia(janela);
        }
        self.avisar(
            InfoBarSeverity::Success,
            t!("abas-caidas-n-fechadas", n = n as i64),
        );
    }

    /// Tira cada sessão do painel dela e a descarta (o mesmo que o ✕ da aba faz). Com
    /// `encerrar_tmux`, as abas do tmux seguem `conexoes.tmux_ao_fechar` (uma pergunta só para
    /// todas); sem, a sessão do tmux fica no servidor. Devolve as janelas mexidas, uma vez cada,
    /// para quem chamou fechar as secundárias que esvaziaram.
    pub(super) fn tirar_sessoes(
        &self,
        sessoes: Vec<(Janela, Session)>,
        encerrar_tmux: bool,
    ) -> Vec<Janela> {
        let mut janelas: Vec<Janela> = Vec::new();
        let mut fechadas = Vec::new();
        for (janela, sessao) in sessoes {
            janela.envio.esquecer(&sessao.key_str());
            janela.layout.update(|l| {
                l.remove(sessao);
            });
            if !janelas.iter().any(|j| j.handle == janela.handle) {
                janelas.push(janela);
            }
            fechadas.push(sessao);
        }
        if !encerrar_tmux {
            for sessao in &fechadas {
                self.esquecer_tmux(*sessao);
            }
        }
        self.fechar_abas_tmux(fechadas.clone());
        // Depois de todas saírem: o assistente que já estava na lista não é tirado duas vezes.
        for sessao in fechadas {
            self.fechar_assistentes_de(sessao);
        }
        janelas
    }

    /// "Fechar todas as abas": pergunta antes se há sessão aberta (e a configuração manda).
    pub(super) fn fechar_todas(&self, janela: Option<Janela>) {
        let vivas = self
            .sessoes_das_janelas(janela)
            .into_iter()
            .filter(|(_, s)| !untrack(|| s.closed()))
            .count();
        let pergunta = self
            .config
            .with_untracked(|c| c.conexoes.confirmar_fechar_todas);
        if vivas == 0 || !pergunta {
            self.fechar_todas_agora(janela);
            return;
        }
        self.abas.vivas.set(vivas);
        self.abas.nao_perguntar.set(false);
        self.abas.alvo.set(Some(janela));
        self.abas.aberto.set(true);
    }

    /// Fecha todas as abas de `janela` (ou de todas) e volta cada janela a um painel só.
    fn fechar_todas_agora(&self, janela: Option<Janela>) {
        let janelas = match janela {
            Some(j) => vec![j],
            None => self.janelas.todas_agora(),
        };
        let sessoes = self.sessoes_das_janelas(janela);
        self.tirar_sessoes(sessoes, true);
        for j in &janelas {
            j.layout.update(|l| l.single());
            j.grade.set((1, 1));
        }
        // Uma janela que já estava sem abas também some, se for secundária.
        for j in janelas {
            self.fechar_se_vazia(j);
        }
    }
}

/// O "Fechar todas as abas?".
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl View + use<> {
    let aberto = tela.abas.aberto;
    let nao_perguntar = tela.abas.nao_perguntar;
    let vivas = tela.abas.vivas;
    let texto = move || {
        let n = vivas.get();
        t!("abas-caidas-sessoes-abertas", n = n as i64)
    };
    ContentDialog::new(t!("abas-caidas-fechar-todas-as-abas"), aberto)
        .id("fechar-todas-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(texto).id("fechar-todas-texto").wrap())
                .child(
                    CheckBox::bind(nao_perguntar, t!("comum-nao-perguntar-de-novo"))
                        .id("fechar-todas-nao-perguntar"),
                ),
        )
        .primary(t!("abas-caidas-fechar-todas"), {
            let t = Rc::clone(tela);
            move || {
                if t.abas.nao_perguntar.get_untracked() {
                    t.config
                        .update(|c| c.conexoes.confirmar_fechar_todas = false);
                }
                if let Some(alvo) = t.abas.alvo.take() {
                    t.fechar_todas_agora(alvo);
                }
            }
        })
        .close(t!("comum-cancelar"))
}
