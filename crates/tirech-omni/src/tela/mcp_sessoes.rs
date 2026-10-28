//! Várias sessões de assistente ao mesmo tempo (M51, seção 6.8): quais estão ligadas, de quem é
//! cada aba, e o que fazer quando uma sessão acaba ou a pessoa para só uma delas.
//!
//! A tela conhece as sessões pelo servidor (`ServidorMcp::sessoes`), que dá o nome a cada uma.
//! Um laço de meio segundo e a chegada de cada pedido conferem a lista: a sessão nova ganha a cor
//! livre da paleta, e a que saiu solta as abas, perde as aprovações e some da marca.

use std::rc::Rc;
use std::time::{Duration, Instant};

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::mcp::{Acao, Plano, Trabalho};
use super::{Janela, Tela};
use crate::mcp::sessoes::{SessaoIa, cor_da_paleta, cor_livre};
use crate::modelo::{Connection, Kind};
use crate::t;

/// Quanto tempo uma grade pedida por uma sessão vale como "a última": se outra sessão pede uma
/// grade diferente nesse prazo, ela pergunta à pessoa.
pub(super) const PRAZO_DA_GRADE: Duration = Duration::from_secs(10);

/// Quantas sessões os menus reservam lugar. Os menus nascem antes das sessões, então cada lugar
/// existe desde o começo e só aparece enquanto há uma sessão nele; a 11ª em diante não tem item.
pub(super) const LUGARES: usize = 10;

/// As ações que mudam o que há numa aba, e por isso respeitam o dono dela.
fn muda_a_aba(acao: Acao) -> bool {
    matches!(
        acao,
        Acao::Digitar | Acao::Teclas | Acao::Rodar | Acao::Fechar | Acao::Reconectar
    )
}

impl Tela {
    /// A geração de "Parar" da sessão `ligacao` (0 enquanto a pessoa não parou só ela).
    pub(super) fn geracao_da_sessao(&self, ligacao: u64) -> u64 {
        self.mcp
            .paradas
            .borrow()
            .get(&ligacao)
            .copied()
            .unwrap_or(0)
    }

    /// Se a pessoa parou a IA (todas) ou a sessão do pedido depois que ele chegou.
    pub(super) fn parado(&self, t: &Trabalho) -> bool {
        t.geracao != self.mcp.geracao.get()
            || t.geracao_sessao != self.geracao_da_sessao(t.pedido.ligacao)
    }

    /// A sessão ligada `ligacao`, se ainda está.
    pub(super) fn sessao_ia(&self, ligacao: u64) -> Option<SessaoIa> {
        self.mcp
            .sessoes
            .with_untracked(|todas| todas.iter().find(|s| s.ligacao == ligacao).cloned())
    }

    /// A aba a que a sessão `ligacao` está presa (`--aba`, M63), se está.
    pub(super) fn aba_da_sessao(&self, ligacao: u64) -> Option<String> {
        self.mcp.alvos.borrow().get(&ligacao).cloned()
    }

    /// Se a aba `chave` é a que o clique em "Controlar com" deu à sessão `ligacao`, ou uma que o
    /// "Dar esta aba também a" deu: nelas, e só nelas, a sessão tem o Livre enquanto a ponte
    /// está ligada.
    pub(super) fn liberada_pelo_clique(&self, ligacao: u64, chave: &str) -> bool {
        self.mcp
            .alvos
            .borrow()
            .get(&ligacao)
            .is_some_and(|a| a == chave)
            || self
                .mcp
                .dadas
                .borrow()
                .get(&ligacao)
                .is_some_and(|d| d.iter().any(|a| a == chave))
    }

    /// Se alguma aba da conexão `id` está liberada pelo clique para a sessão `ligacao`.
    pub(super) fn conexao_liberada_pelo_clique(&self, ligacao: u64, id: u64) -> bool {
        let da_conexao = |chave: &String| crate::espacos::id_da_chave(chave) == Some(id);
        self.mcp
            .alvos
            .borrow()
            .get(&ligacao)
            .is_some_and(da_conexao)
            || self
                .mcp
                .dadas
                .borrow()
                .get(&ligacao)
                .is_some_and(|d| d.iter().any(da_conexao))
    }

    /// A conexão da aba `chave`: a da árvore; no shell local avulso (id 0) e numa conexão que a
    /// pessoa já apagou, uma conexão local com o título da aba.
    pub(super) fn conexao_da_aba(&self, chave: &str, sessao: Session) -> Connection {
        let id = crate::espacos::id_da_chave(chave).unwrap_or(0);
        self.tree
            .with_untracked(|t| t.conexao_para_abrir(id))
            .unwrap_or_else(|| {
                let mut conexao = Connection::new(sessao.title(), Kind::Local);
                conexao.id = id;
                conexao
            })
    }

    /// Confere as sessões que o servidor tem com as que a tela conhece: dá cor às novas e
    /// encerra as que saíram. Enquanto o servidor está no ar, repete a cada meio segundo.
    pub(super) fn sincronizar_sessoes(self: &Rc<Self>) {
        let (ligadas, no_ar) = match &*self.mcp.servidor.borrow() {
            Some(servidor) => (servidor.sessoes(), true),
            None => (Vec::new(), false),
        };
        let atuais = self.mcp.sessoes.get_untracked();
        let sairam: Vec<u64> = atuais
            .iter()
            .filter(|s| !ligadas.iter().any(|l| l.ligacao == s.ligacao))
            .map(|s| s.ligacao)
            .collect();
        let mut novas: Vec<SessaoIa> = atuais
            .iter()
            .filter(|s| !sairam.contains(&s.ligacao))
            .cloned()
            .collect();
        for ligada in &ligadas {
            if novas.iter().all(|s| s.ligacao != ligada.ligacao) {
                let em_uso: Vec<usize> = novas.iter().map(|s| s.cor).collect();
                novas.push(SessaoIa::nova(ligada, cor_livre(&em_uso)));
                if let Some(aba) = &ligada.aba {
                    self.prender_a_aba(ligada.ligacao, aba);
                }
            }
        }
        if novas != atuais {
            self.mcp.sessoes.set(novas);
        }
        self.dar_abas_dos_clusters(&ligadas);
        for ligacao in sairam {
            self.encerrar_sessao(ligacao);
        }
        if no_ar && !self.mcp.sincronizando.replace(true) {
            let eu = self.eu.clone();
            self.alarmes.depois(Duration::from_millis(500), move || {
                if let Some(tela) = eu.upgrade() {
                    tela.mcp.sincronizando.set(false);
                    tela.sincronizar_sessoes();
                }
            });
        }
    }

    /// O clique em "Controlar com" (M63): a sessão vira a dona da aba, sem perguntar, mesmo que
    /// outra fosse, e ganha o Livre nela e a aba como a padrão.
    fn prender_a_aba(&self, ligacao: u64, aba: &str) {
        self.mcp.alvos.borrow_mut().insert(ligacao, aba.to_owned());
        self.dar_aba(aba, ligacao);
    }

    /// O clique em "Dar esta aba também a": a sessão vira a dona da aba, sem perguntar, mesmo que
    /// outra fosse, e ganha o Livre nela. A aba padrão dela continua a do "Controlar com".
    /// A sessão fica sabendo pelo recado, na próxima resposta ou na próxima mensagem da pessoa.
    pub(super) fn dar_aba_tambem(&self, ligacao: u64, aba: &str) {
        self.anotar_aba_dada(ligacao, aba, |titulo| {
            t!("mcp-aba-dada", aba = aba, titulo = titulo)
        });
        self.dar_aba(aba, ligacao);
    }

    /// O cluster com IA: a aba de uma máquina do cluster abriu e vai para o assistente dele,
    /// uma vez só (se a pessoa a der depois a outra sessão, ela fica com a outra).
    pub(super) fn dar_aba_do_cluster(&self, ligacao: u64, aba: &str) {
        let nova = self.anotar_aba_dada(ligacao, aba, |titulo| {
            t!("mcp-aba-do-cluster", aba = aba, titulo = titulo)
        });
        if nova {
            self.dar_aba(aba, ligacao);
        }
    }

    /// Põe `aba` entre as dadas à sessão `ligacao`, com o recado que `recado` escreve a partir do
    /// título da aba. Devolve `false` se ela já estava lá.
    fn anotar_aba_dada(
        &self,
        ligacao: u64,
        aba: &str,
        recado: impl FnOnce(&str) -> String,
    ) -> bool {
        let nova = {
            let mut dadas = self.mcp.dadas.borrow_mut();
            let abas = dadas.entry(ligacao).or_default();
            let nova = !abas.iter().any(|a| a == aba);
            if nova {
                abas.push(aba.to_owned());
            }
            nova
        };
        if nova {
            let titulo = self
                .sessao_por_chave(aba)
                .map(|(_, s)| s.title())
                .unwrap_or_default();
            self.mcp
                .recados
                .borrow_mut()
                .entry(ligacao)
                .or_default()
                .push(recado(&titulo));
        }
        nova
    }

    /// A ponte da sessão `ligacao` caiu: recusa o que ela tinha esperando, solta as abas dela e
    /// tira a marca. O que estava em andamento já foi cancelado pelo servidor.
    fn encerrar_sessao(self: &Rc<Self>, ligacao: u64) {
        self.esquecer_sessao(ligacao);
        self.mcp.alvos.borrow_mut().remove(&ligacao);
        self.mcp.dadas.borrow_mut().remove(&ligacao);
        self.mcp.recados.borrow_mut().remove(&ligacao);
        self.mcp.paradas.borrow_mut().remove(&ligacao);
        self.mcp
            .grades
            .borrow_mut()
            .retain(|_, (sessao, _, _)| *sessao != ligacao);
    }

    /// Recusa os pedidos da sessão, tira as aprovações "até eu parar", a marca e a posse das abas.
    fn esquecer_sessao(self: &Rc<Self>, ligacao: u64) {
        self.recusar_aprovacoes_de(ligacao, &t!("mcp-a-pessoa-parou"));
        self.mcp
            .aprovadas_sessao
            .borrow_mut()
            .retain(|(sessao, _)| *sessao != ligacao);
        self.mcp
            .donos
            .borrow_mut()
            .retain(|_, dono| *dono != ligacao);
        self.mcp.atividade.borrow_mut().retain(|_, a| {
            a.vistas.retain(|v| *v != ligacao);
            match a.vistas.last() {
                Some(ultima) => {
                    if a.ligacao == ligacao {
                        a.ligacao = *ultima;
                    }
                    true
                }
                None => false,
            }
        });
        self.mexeu_a_marca();
    }

    /// "Parar" numa sessão: o que ela pediu e espera é recusado, o que roda acaba, as aprovações
    /// dela caem e as abas ficam sem dono. As outras sessões e a ponte seguem como estavam.
    pub(super) fn parar_sessao(self: &Rc<Self>, ligacao: u64) {
        let geracao = self.geracao_da_sessao(ligacao) + 1;
        self.mcp.paradas.borrow_mut().insert(ligacao, geracao);
        self.esquecer_sessao(ligacao);
    }

    /// "Parar a IA" no menu de uma aba: para a sessão dona dela; sem dono, as que mexeram nela.
    pub(super) fn parar_sessoes_da_aba(self: &Rc<Self>, chave: &str) {
        let dona = self.mcp.donos.borrow().get(chave).copied();
        let ligacoes: Vec<u64> = match dona {
            Some(dona) => vec![dona],
            None => self
                .mcp
                .atividade
                .borrow()
                .get(chave)
                .map(|a| a.vistas.clone())
                .unwrap_or_default(),
        };
        for ligacao in ligacoes {
            self.parar_sessao(ligacao);
        }
    }

    /// Quantas sessões têm pedidos ou marca em abas desta janela agora.
    pub(super) fn sessoes_marcando(&self, chaves: &[String]) -> Vec<u64> {
        let _ = self.mcp.marca_v.get();
        let mut ligacoes: Vec<u64> = Vec::new();
        for chave in chaves {
            if !self.aba_marcada(chave) {
                continue;
            }
            let Some(atividade_ligacoes) = self
                .mcp
                .atividade
                .borrow()
                .get(chave)
                .map(|a| a.vistas.clone())
            else {
                continue;
            };
            for ligacao in atividade_ligacoes {
                if !ligacoes.contains(&ligacao) {
                    ligacoes.push(ligacao);
                }
            }
        }
        ligacoes
    }

    /// A sessão dona da aba `chave`. Rastreia as sessões e a marca: serve numa view.
    pub(super) fn dono_da_aba(&self, chave: &str) -> Option<SessaoIa> {
        let _ = self.mcp.marca_v.get();
        let ligacao = self.mcp.donos.borrow().get(chave).copied()?;
        self.mcp
            .sessoes
            .with(|todas| todas.iter().find(|s| s.ligacao == ligacao).cloned())
    }

    /// A cor da marca da aba `chave`: a da sessão dona; sem dono, a da única sessão que mexeu
    /// nela; vista por várias sem dono, `None` (a cor do título).
    pub(super) fn cor_da_aba(&self, chave: &str) -> Option<ThemeColor> {
        let _ = self.mcp.marca_v.get();
        if let Some(dono) = self.dono_da_aba(chave) {
            return Some(cor_da_paleta(dono.cor));
        }
        let ligacao = {
            let mapa = self.mcp.atividade.borrow();
            let atividade = mapa.get(chave)?;
            match atividade.vistas.as_slice() {
                [unica] => *unica,
                _ => return None,
            }
        };
        self.mcp.sessoes.with(|todas| {
            todas
                .iter()
                .find(|s| s.ligacao == ligacao)
                .map(|s| cor_da_paleta(s.cor))
        })
    }

    /// Passa a aba `chave` para a sessão `ligacao`.
    pub(super) fn dar_aba(&self, chave: &str, ligacao: u64) {
        self.mcp
            .donos
            .borrow_mut()
            .insert(chave.to_owned(), ligacao);
        self.mexeu_a_marca();
    }

    /// Deixa a aba `chave` sem dono.
    pub(super) fn tirar_aba(&self, chave: &str) {
        self.mcp.donos.borrow_mut().remove(chave);
        self.mexeu_a_marca();
    }

    /// A sessão dona da aba do plano, se o pedido muda a aba e o dono é outra sessão.
    pub(super) fn dono_de_outra(&self, t: &Trabalho, plano: &Plano) -> Option<SessaoIa> {
        if !muda_a_aba(plano.acao) {
            return None;
        }
        let chave = plano.sessao?.key_str();
        let dono = self.mcp.donos.borrow().get(&chave).copied()?;
        if dono == t.pedido.ligacao {
            return None;
        }
        self.sessao_ia(dono)
    }

    /// A primeira sessão que muda uma aba sem dono (a da pessoa) passa a ser a dona dela.
    pub(super) fn tomar_a_aba_sem_dono(&self, t: &Trabalho, plano: &Plano) {
        if !muda_a_aba(plano.acao) {
            return;
        }
        let Some(sessao) = plano.sessao else {
            return;
        };
        let chave = sessao.key_str();
        if !self.mcp.donos.borrow().contains_key(&chave) {
            self.dar_aba(&chave, t.pedido.ligacao);
        }
    }

    /// Se outra sessão pediu uma grade diferente para a mesma janela há pouco.
    pub(super) fn grade_disputada(&self, t: &Trabalho, plano: &Plano) -> bool {
        let Some((janela, colunas, linhas)) = plano.grade else {
            return false;
        };
        self.mcp
            .grades
            .borrow()
            .get(&janela)
            .is_some_and(|(sessao, grade, quando)| {
                *sessao != t.pedido.ligacao
                    && *grade != (colunas, linhas)
                    && quando.elapsed() < PRAZO_DA_GRADE
            })
    }
}

/// Há quanto tempo, em palavras curtas: "12 s", "3 min", "2 h".
fn ha(desde: Instant) -> String {
    let segundos = desde.elapsed().as_secs();
    if segundos < 60 {
        t!("mcp-ha-segundos", n = segundos.to_string())
    } else if segundos < 3600 {
        t!("mcp-ha-minutos", n = (segundos / 60).to_string())
    } else {
        t!("mcp-ha-horas", n = (segundos / 3600).to_string())
    }
}

impl Tela {
    /// A sessão que ocupa o lugar `n` dos menus. Rastreia as sessões: serve numa view.
    pub(super) fn sessao_do_lugar(&self, n: usize) -> Option<SessaoIa> {
        self.mcp.sessoes.with(|todas| todas.get(n).cloned())
    }

    /// Quantas abas são da sessão `ligacao`.
    fn abas_da_sessao(&self, ligacao: u64) -> usize {
        let _ = self.mcp.marca_v.get();
        self.mcp
            .donos
            .borrow()
            .values()
            .filter(|&&dono| dono == ligacao)
            .count()
    }

    /// Quantas sessões estão mexendo em abas da janela agora. Rastreia a marca.
    pub(super) fn sessoes_na_janela(&self, janela: Janela) -> usize {
        let chaves: Vec<String> = janela
            .layout
            .with(|l| l.sessions())
            .iter()
            .map(|s| s.key_str())
            .collect();
        self.sessoes_marcando(&chaves).len()
    }

    /// A linha do menu "Sessões ligadas" para a sessão do lugar `n`.
    fn resumo_da_sessao(&self, n: usize) -> String {
        let Some(sessao) = self.sessao_do_lugar(n) else {
            return String::new();
        };
        let abas = match self.abas_da_sessao(sessao.ligacao) {
            1 => t!("mcp-uma-aba"),
            abas => t!("mcp-n-abas", n = abas.to_string()),
        };
        t!(
            "mcp-sessao-resumo",
            nome = sessao.nome.as_str(),
            pid = sessao.pid.to_string(),
            abas = abas,
            desde = ha(sessao.desde)
        )
    }
}

/// "Sessões ligadas": uma linha por sessão (nome, pid, abas e desde quando), cada uma com o
/// "Parar" dela.
pub(super) fn sessoes_ligadas(tela: &Rc<Tela>) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id("mcp-sessoes-menu");
    for n in 0..LUGARES {
        let (resumo, existe, parar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        let acoes = MenuFlyout::new().item(
            format!("mcp-sessao-{}-parar", n + 1),
            t!("mcp-parar-sessao-menu"),
            move || {
                if let Some(sessao) = parar.sessao_do_lugar(n) {
                    parar.parar_sessao(sessao.ligacao);
                }
            },
        );
        menu = menu
            .submenu(
                format!("mcp-sessao-{}", n + 1),
                move || resumo.resumo_da_sessao(n),
                acoes,
            )
            .icon(Symbol::Sparkle)
            .item_visible(move || existe.sessao_do_lugar(n).is_some());
    }
    let vazio = Rc::clone(tela);
    menu.item("mcp-nenhuma-sessao", t!("mcp-nenhuma-sessao"), || {})
        .item_visible(move || vazio.mcp.sessoes.with(Vec::is_empty))
}

/// O submenu "Assistentes de IA" do menu Ferramentas.
pub(super) fn menu_assistentes(tela: &Rc<Tela>) -> MenuFlyout {
    let (ligar, rotulo, parar, visivel) = (
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
    );
    let (como, espaco) = (Rc::clone(tela), Rc::clone(tela));
    MenuFlyout::new()
        .id("mcp-assistentes-menu")
        .item(
            "mcp-ligar",
            move || {
                if rotulo.config.with(|c| c.ia.mcp_ligado) {
                    t!("mcp-menu-desligar")
                } else {
                    t!("mcp-menu-ligar")
                }
            },
            move || ligar.alternar_controle_mcp(),
        )
        .icon(Symbol::Sparkle)
        .item("mcp-parar", t!("mcp-parar"), move || parar.parar_ia_mcp())
        .icon(Symbol::Dismiss)
        .item_visible(move || visivel.alguma_aba_marcada())
        .separator()
        .submenu(
            "mcp-controlar",
            t!("controlar-menu-aba-atual"),
            super::controlar::submenu(tela, "mcp-controlar", super::controlar::Alvo::Atual),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || super::controlar::aparece(&t, super::controlar::Alvo::Atual)
        })
        .submenu(
            "mcp-controlar-dar",
            t!("controlar-dar-menu-aba-atual"),
            super::controlar::dar_tambem(tela, "mcp-controlar", super::controlar::Alvo::Atual),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || super::controlar::aparece_dar(&t, super::controlar::Alvo::Atual)
        })
        .submenu(
            "mcp-sessoes",
            t!("mcp-sessoes-ligadas"),
            sessoes_ligadas(tela),
        )
        .item("mcp-como", t!("mcp-menu-como"), move || {
            como.abrir_como_ligar(None)
        })
        .item("mcp-espaco", t!("mcp-espaco-do-terminal"), move || {
            let conexao = espaco
                .sessao_ativa()
                .and_then(|s| espaco.conexao_da_sessao(s));
            espaco.novo_espaco_ia(conexao, "");
        })
}

/// O menu "Parar" da faixa com várias sessões: "Parar a {sessão}" para cada uma e "Parar todas".
pub(super) fn menu_parar(tela: &Rc<Tela>) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id("mcp-parar-menu");
    for n in 0..LUGARES {
        let (nome, existe, parar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        menu = menu
            .item(
                format!("mcp-parar-sessao-{}", n + 1),
                move || {
                    nome.sessao_do_lugar(n).map_or_else(String::new, |s| {
                        t!("mcp-parar-sessao", nome = s.nome.as_str())
                    })
                },
                move || {
                    if let Some(sessao) = parar.sessao_do_lugar(n) {
                        parar.parar_sessao(sessao.ligacao);
                    }
                },
            )
            .icon(Symbol::Sparkle)
            .item_visible(move || existe.sessao_do_lugar(n).is_some());
    }
    let todas = Rc::clone(tela);
    menu.separator()
        .item("mcp-parar-todas", t!("mcp-parar-todas"), move || {
            todas.parar_ia_mcp()
        })
        .icon(Symbol::Dismiss)
}

/// "Dar para ▸" no menu de uma aba: uma linha por sessão, menos a dona.
pub(super) fn dar_para(tela: &Rc<Tela>, sessao: Session) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id("mcp-dar-para-menu");
    for n in 0..LUGARES {
        let (nome, existe, dar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        menu = menu
            .item(
                format!("mcp-dar-para-{}", n + 1),
                move || nome.sessao_do_lugar(n).map_or_else(String::new, |s| s.nome),
                move || {
                    if let Some(destino) = dar.sessao_do_lugar(n) {
                        dar.dar_aba(&sessao.key_str(), destino.ligacao);
                    }
                },
            )
            .icon(Symbol::Sparkle)
            .item_visible(move || {
                let dona = existe.dono_da_aba(&sessao.key_str()).map(|d| d.ligacao);
                existe
                    .sessao_do_lugar(n)
                    .is_some_and(|s| Some(s.ligacao) != dona)
            });
    }
    menu
}
