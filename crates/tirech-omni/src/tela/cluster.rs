//! Modo cluster (M42) na tela: o diálogo que escolhe as conexões e a janela nova com elas lado a
//! lado, com o envio ligado em "Todos". As funções puras (grade, linhas, filtro) estão em
//! [`crate::cluster`].
//!
//! O cluster com IA usa o mesmo diálogo, com o assistente escolhido: a janela nova não tem envio
//! para todas, e um só assistente do computador (Claude Code, Codex ou OpenCode), num painel ao
//! lado, ganha as abas das máquinas conforme elas abrem. Ele não fica guardado em lugar nenhum,
//! a não ser que a pessoa salve a janela num espaço de trabalho.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::controlar::{Conversar, Encomenda};
use super::{Lugar, Tela, geometria_de, icone_do_cartao, paineis_de};
use crate::abrir_em::AlvoAbrir;
use crate::cluster::{self, grade_do_cluster};
use crate::comandos::tem_terminal;
use crate::espacos::{Aba, ClusterIaSalvo, JanelaSalva, Resolvida, id_da_chave};
use crate::ia::{Ferramenta, ficha};
use crate::janelas::Janela;
use crate::mcp::clientes::NOME_DA_ABA;
use crate::mcp::servidor::SessaoLigada;
use crate::modelo::Node;
use crate::t;

/// O que o diálogo "Abrir em modo cluster" guarda enquanto está na tela.
pub(super) struct Estado {
    /// O diálogo está na tela.
    aberto: Signal<bool>,
    /// O texto do filtro.
    filtro: Signal<String>,
    /// Os ids (como texto) das conexões escolhidas.
    escolhidas: Signal<BTreeSet<String>>,
    /// As linhas da grade: as conexões de terminal que o filtro deixa passar.
    linhas: Signal<Vec<DataGridRow>>,
    /// O diálogo abriu para o cluster com IA.
    com_ia: Signal<bool>,
    /// O nome do assistente escolhido no diálogo, como a ficha dele dá.
    assistente: Signal<String>,
    /// Os clusters com IA abertos.
    ias: RefCell<Vec<ClusterIa>>,
}

/// Um cluster com IA aberto: a janela, as máquinas e o assistente que controla todas.
pub(super) struct ClusterIa {
    /// A janela do cluster.
    pub(super) janela: Janela,
    /// As conexões das máquinas: as abas delas nesta janela vão para o assistente.
    ids: Vec<u64>,
    /// As abas que já estavam na janela antes do cluster (na principal, onde só cabe uma).
    ja_havia: Vec<Session>,
    /// O assistente.
    pub(super) ferramenta: Ferramenta,
    /// A aba do assistente.
    pub(super) assistente: Session,
    /// O código do lançamento, que a ponte do assistente manda ao ligar.
    lancamento: String,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            filtro: signal(String::new()),
            escolhidas: signal(BTreeSet::new()),
            linhas: signal(Vec::new()),
            com_ia: signal(false),
            assistente: signal(String::new()),
            ias: RefCell::new(Vec::new()),
        }
    }
}

impl Tela {
    /// Abre o diálogo com `previas` já escolhidas; `com_ia` para o cluster com IA.
    pub(super) fn abrir_dialogo_do_cluster(&self, previas: &[u64], com_ia: bool) {
        if com_ia {
            let mostrados = self.assistentes_mostrados();
            let atual = self.cluster.assistente.get_untracked();
            if !mostrados.iter().any(|f| ficha(*f).nome == atual) {
                let primeiro = mostrados.first().map(|f| ficha(*f).nome);
                self.cluster
                    .assistente
                    .set(primeiro.unwrap_or_default().to_owned());
            }
        }
        self.cluster.com_ia.set(com_ia);
        self.cluster.filtro.set(String::new());
        self.cluster
            .escolhidas
            .set(previas.iter().map(u64::to_string).collect());
        self.cluster.aberto.set(true);
    }

    /// As conexões de terminal que o item `id` da árvore sugere para o cluster: as do grupo, ou
    /// a própria conexão. Vazio se `id` não existe ou não abre terminal.
    pub(super) fn previas_do_cluster(&self, id: u64) -> Vec<u64> {
        let alvo = self.tree.with_untracked(|t| match t.find(id) {
            Some(Node::Group(_)) => Some(AlvoAbrir::Pasta(id)),
            Some(Node::Connection(_)) => Some(AlvoAbrir::Conexao(id)),
            None => None,
        });
        alvo.map(|a| self.conexoes_de_terminal(a))
            .unwrap_or_default()
    }

    /// "Modo cluster ›" do menu Ferramentas: a seleção da árvore, se houver, vem escolhida.
    pub(super) fn abrir_dialogo_do_cluster_da_selecao(&self, com_ia: bool) {
        let previas = self
            .id_selecionado()
            .map(|id| self.previas_do_cluster(id))
            .unwrap_or_default();
        self.abrir_dialogo_do_cluster(&previas, com_ia);
    }

    /// Os assistentes que as Configurações mostram, na ordem de sempre.
    fn assistentes_mostrados(&self) -> Vec<Ferramenta> {
        self.config.with_untracked(|c| {
            Ferramenta::TODAS
                .into_iter()
                .filter(|f| c.ia.mostra(*f))
                .collect()
        })
    }

    /// O botão "Abrir" do diálogo: abre as conexões escolhidas na ordem da árvore.
    fn confirmar_cluster(self: &Rc<Self>) {
        let escolhidas = self.cluster.escolhidas.get_untracked();
        let ids = self.tree.with_untracked(|t| {
            cluster::linhas(t)
                .into_iter()
                .filter(|l| escolhidas.contains(&l.id.to_string()))
                .map(|l| l.id)
                .collect::<Vec<_>>()
        });
        self.cluster.aberto.set(false);
        if !self.cluster.com_ia.get_untracked() {
            self.abrir_cluster(ids);
            return;
        }
        let nome = self.cluster.assistente.get_untracked();
        if let Some(f) = Ferramenta::TODAS
            .into_iter()
            .find(|f| ficha(*f).nome == nome)
        {
            self.pedir_cluster_ia(ids, f);
        }
    }

    /// Só as conexões de `ids` que abrem terminal.
    fn so_terminais(&self, ids: Vec<u64>) -> Vec<u64> {
        ids.into_iter()
            .filter(|id| {
                self.tree
                    .with_untracked(|t| t.connection(*id).is_some_and(|c| tem_terminal(c.kind)))
            })
            .collect()
    }

    /// Abre `ids` lado a lado numa janela nova, com o que se digita indo para todas. Onde só
    /// cabe uma janela, usa a principal: as abas que já estavam continuam e o envio vai só para
    /// os terminais à vista.
    pub(super) fn abrir_cluster(self: &Rc<Self>, ids: Vec<u64>) {
        let ids = self.so_terminais(ids);
        if ids.is_empty() {
            return;
        }
        let (colunas, linhas) = grade_do_cluster(ids.len());
        let paineis = colunas * linhas;
        let titulo = t!("cluster-titulo-da-janela", n = ids.len() as i64);
        let janela = match self.abrir_janela_titulada(
            Some(titulo),
            (colunas, linhas),
            None,
            DockLayout::default(),
        ) {
            Some(janela) => {
                janela.envio.todos.set(true);
                janela
            }
            None => {
                let principal = self.janelas.principal();
                self.aplicar_grade(principal, colunas, linhas);
                principal.envio.visiveis.set(true);
                self.avisar(InfoBarSeverity::Warning, t!("cluster-so-uma-janela"));
                principal
            }
        };
        for (i, id) in ids.into_iter().enumerate() {
            self.abrir_com(id, Lugar::painel(janela, i % paineis), true);
        }
        janela.handle.focus();
        self.mostrar_terminais();
    }

    /// O cluster com IA de `ids` com o assistente `f`. Com o controle por assistentes desligado,
    /// passa antes pelo aviso de ligar, como o "Controlar com".
    pub(super) fn pedir_cluster_ia(self: &Rc<Self>, ids: Vec<u64>, f: Ferramenta) {
        let depois = {
            let ids = ids.clone();
            move |t: &Rc<Tela>| t.abrir_cluster_ia(ids, f)
        };
        if self.controle_ligado(depois) {
            self.abrir_cluster_ia(ids, f);
        }
    }

    /// "Abrir todas controladas por IA ▸ {assistente}" da pasta `id`: o cluster com IA das
    /// conexões de terminal dela, direto, sem o diálogo.
    pub(super) fn abrir_pasta_com_ia(self: &Rc<Self>, id: u64, f: Ferramenta) {
        let ids = self.previas_do_cluster(id);
        if !ids.is_empty() {
            self.pedir_cluster_ia(ids, f);
        }
    }

    /// Abre `ids` lado a lado numa janela nova, sem envio para todas, com o assistente `f` no
    /// último painel. Onde só cabe uma janela, usa a principal, e as abas que já estavam lá
    /// não vão para o assistente.
    fn abrir_cluster_ia(self: &Rc<Self>, ids: Vec<u64>, f: Ferramenta) {
        let ids = self.so_terminais(ids);
        if ids.is_empty() {
            return;
        }
        let (colunas, linhas) = grade_do_cluster(ids.len() + 1);
        let do_assistente = ids.len().min(colunas * linhas - 1);
        let titulo = t!(
            "cluster-ia-titulo-da-janela",
            n = ids.len() as i64,
            nome = ficha(f).nome
        );
        let janela = match self.abrir_janela_titulada(
            Some(titulo),
            (colunas, linhas),
            None,
            DockLayout::default(),
        ) {
            Some(janela) => janela,
            None => {
                let principal = self.janelas.principal();
                self.aplicar_grade(principal, colunas, linhas);
                self.avisar(InfoBarSeverity::Warning, t!("cluster-so-uma-janela"));
                principal
            }
        };
        let ja_havia = self.abas_da_janela(janela);
        for (i, id) in ids.iter().enumerate() {
            self.abrir_com(*id, Lugar::painel(janela, i % do_assistente), true);
        }
        self.ligar_cluster_ia(janela, ids, ja_havia, f, do_assistente, Conversar::Nova);
        janela.handle.focus();
        self.mostrar_terminais();
    }

    /// Reabre o cluster com IA que um espaço de trabalho (ou o `estado.json`) guardou: as
    /// máquinas de `resolvida` numa janela dele, com a grade de `salva`, e o assistente de `ia`
    /// no painel dele, continuando a conversa se ela ainda existe.
    pub(super) fn reabrir_cluster_ia(
        self: &Rc<Self>,
        salva: &JanelaSalva,
        resolvida: &Resolvida,
        ia: &ClusterIaSalvo,
    ) {
        let Some(f) = Ferramenta::da_chave(&ia.ferramenta) else {
            return;
        };
        let conversa = self.conversa_salva(f, ia.conversa.as_deref(), &self.verificador(false));
        let ids: Vec<u64> = resolvida
            .paineis
            .iter()
            .flatten()
            .filter_map(|(aba, _)| match aba {
                Aba::Conexao(id, _) => Some(*id),
                Aba::Local => None,
            })
            .collect();
        let titulo = t!(
            "cluster-ia-titulo-da-janela",
            n = ids.len() as i64,
            nome = ficha(f).nome
        );
        let (colunas, linhas) = salva.grade;
        let janela = match self.abrir_janela_titulada(
            Some(titulo),
            salva.grade,
            geometria_de(salva),
            paineis_de(salva),
        ) {
            Some(janela) => janela,
            None => {
                let principal = self.janelas.principal();
                self.aplicar_grade(principal, colunas, linhas);
                principal
            }
        };
        let ja_havia = self.abas_da_janela(janela);
        self.encher(janela, resolvida, false);
        let painel = ia.painel.min((colunas * linhas).max(1) - 1);
        let depois = {
            let (ids, ja_havia, conversa) = (ids.clone(), ja_havia.clone(), conversa.clone());
            move |t: &Rc<Tela>| t.ligar_cluster_ia(janela, ids, ja_havia, f, painel, conversa)
        };
        if self.controle_ligado(depois) {
            self.ligar_cluster_ia(janela, ids, ja_havia, f, painel, conversa);
        }
        janela.handle.focus();
        self.mostrar_terminais();
    }

    /// As abas de `janela`.
    fn abas_da_janela(&self, janela: Janela) -> Vec<Session> {
        self.sessoes_das_janelas(Some(janela))
            .into_iter()
            .map(|(_, s)| s)
            .collect()
    }

    /// Abre o assistente `f` no painel `painel` de `janela`, para controlar as máquinas `ids`
    /// que estão (ou vão estar) nela, na `conversa`. O espaço de trabalho chama ao reabrir um
    /// cluster com IA.
    pub(super) fn ligar_cluster_ia(
        self: &Rc<Self>,
        janela: Janela,
        ids: Vec<u64>,
        ja_havia: Vec<Session>,
        f: Ferramenta,
        painel: usize,
        conversa: Conversar,
    ) {
        let maquinas = self.tree.with_untracked(|t| {
            ids.iter()
                .filter_map(|id| t.connection(*id).map(|c| c.name.clone()))
                .collect::<Vec<_>>()
                .join(", ")
        });
        let encomenda = Encomenda {
            rotulo: t!("cluster-ia-rotulo", n = ids.len() as i64),
            ponte: vec!["--cluster".to_owned()],
            instrucoes: t!(
                "controlar-instrucoes-cluster",
                maquinas = maquinas,
                servidor = NOME_DA_ABA
            ),
            lugar: Lugar::painel(janela, painel),
            conversa,
            chave: None,
        };
        let Some((assistente, lancamento)) = self.lancar_local(f, encomenda) else {
            return;
        };
        self.anotar_assistente(None, f, assistente);
        self.cluster.ias.borrow_mut().push(ClusterIa {
            janela,
            ids,
            ja_havia,
            ferramenta: f,
            assistente,
            lancamento,
        });
    }

    /// O cluster com IA vivo de `janela`, se ela tem um: o assistente, o painel e a aba dele.
    pub(super) fn cluster_ia_da_janela(
        &self,
        janela: Janela,
    ) -> Option<(Ferramenta, usize, Session)> {
        let ias = self.cluster.ias.borrow();
        let ia = ias
            .iter()
            .find(|c| c.janela.handle == janela.handle && !c.assistente.closed())?;
        let painel = janela
            .layout
            .try_with_untracked(|l| l.pane_of(ia.assistente))
            .flatten()?;
        Some((ia.ferramenta, painel, ia.assistente))
    }

    /// Dá ao assistente de cada cluster com IA as abas das máquinas que já abriram na janela
    /// dele, uma vez cada. Roda com a sincronização das sessões do servidor.
    pub(super) fn dar_abas_dos_clusters(&self, ligadas: &[SessaoLigada]) {
        self.cluster
            .ias
            .borrow_mut()
            .retain(|c| !c.assistente.closed());
        let ias = self.cluster.ias.borrow();
        for ia in ias.iter() {
            let Some(ligada) = ligadas
                .iter()
                .find(|l| l.lancamento.as_deref() == Some(ia.lancamento.as_str()))
            else {
                continue;
            };
            for sessao in self.abas_da_janela(ia.janela) {
                if sessao.closed() || sessao == ia.assistente || ia.ja_havia.contains(&sessao) {
                    continue;
                }
                let chave = sessao.key_str();
                if id_da_chave(&chave).is_some_and(|id| ia.ids.contains(&id)) {
                    self.dar_aba_do_cluster(ligada.ligacao, &chave);
                }
            }
        }
    }
}

/// O diálogo "Abrir em modo cluster": o efeito das linhas liga uma vez, e a função devolvida
/// monta a caixa em cada janela.
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl Fn() -> ContentDialog + use<> {
    let estado = &tela.cluster;
    let (aberto, filtro, escolhidas, linhas, com_ia, assistente) = (
        estado.aberto,
        estado.filtro,
        estado.escolhidas,
        estado.linhas,
        estado.com_ia,
        estado.assistente,
    );
    // As linhas acompanham a árvore e o filtro.
    let arvore = tela.tree;
    effect(move || {
        let busca = filtro.get();
        let novas = arvore.with(|t| {
            cluster::linhas(t)
                .into_iter()
                .filter(|l| l.combina(&busca))
                .map(|l| {
                    DataGridRow::new(l.id.to_string())
                        .cell(l.nome.clone(), CellValue::Text(l.nome))
                        .cell(l.grupo.clone(), CellValue::Text(l.grupo))
                        .cell(l.host.clone(), CellValue::Text(l.host))
                        .icon(icone_do_cartao(l.kind))
                })
                .collect::<Vec<_>>()
        });
        linhas.set(novas);
    });
    let tela = Rc::clone(tela);
    move || {
        let tela = &tela;
        let grade = DataGrid::bind(linhas, escolhidas)
            .id("cluster-grade")
            .height(220)
            .columns([
                DataGridColumn::new("nome", t!("comum-nome")).width(ColumnWidth::Star(1.0)),
                DataGridColumn::new("grupo", t!("comum-grupo")).width(ColumnWidth::Star(0.8)),
                DataGridColumn::new("host", t!("comum-host")).width(ColumnWidth::Star(1.6)),
            ])
            .empty_text(t!("cluster-nenhuma-conexao-de-terminal"));
        let contagem = move || cluster::contagem(escolhidas.with(BTreeSet::len));
        let abrir = {
            let t = Rc::clone(tela);
            Button::new(t!("comum-abrir"))
                .id("cluster-abrir")
                .accent()
                .disabled(move || {
                    escolhidas.with(BTreeSet::is_empty)
                        || (com_ia.get() && assistente.with(String::is_empty))
                })
                .on_click(move || t.confirmar_cluster())
        };
        // As opções seguem as Configurações: a escolha é montada de novo a cada abertura.
        let escolha_do_assistente = {
            let t = Rc::clone(tela);
            Show::when(
                move || aberto.get() && com_ia.get(),
                move || {
                    let nomes: Vec<String> = t
                        .assistentes_mostrados()
                        .into_iter()
                        .map(|f| ficha(f).nome.to_owned())
                        .collect();
                    Row::new()
                        .gap(8)
                        .align(Align::Center)
                        .child(Text::new(t!("cluster-ia-assistente")))
                        .child(
                            ComboBox::bind(assistente, nomes)
                                .id("cluster-assistente")
                                .placeholder(t!("cluster-ia-nenhum-assistente"))
                                .grow(1),
                        )
                },
            )
        };
        let titulo = move || {
            if com_ia.get() {
                t!("cluster-ia-abrir-em-cluster-com-ia")
            } else {
                t!("cluster-abrir-em-modo-cluster")
            }
        };
        let explicacao = move || {
            if com_ia.get() {
                t!("cluster-ia-explicacao")
            } else {
                t!("cluster-as-conexoes-escolhidas-abrem-lado-a")
            }
        };
        ContentDialog::new(titulo, aberto)
            .id("cluster-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(explicacao).wrap())
                    .child(escolha_do_assistente)
                    .child(
                        Edit::bind(filtro)
                            .id("cluster-filtro")
                            .placeholder(t!("comum-filtrar"))
                            .height(32),
                    )
                    .child(grade)
                    .child(
                        Row::new()
                            .gap(8)
                            .align(Align::Center)
                            .child(
                                Row::new()
                                    .grow(1)
                                    .child(Text::new(contagem).id("cluster-contagem")),
                            )
                            .child(abrir),
                    ),
            )
            .close(t!("comum-cancelar"))
    }
}

/// O submenu "Abrir todas controladas por IA ▸" da pasta `id`, com os assistentes que as
/// Configurações mostram. `prefixo` abre os ids (`{prefixo}-claude`, `-codex` e `-opencode`).
pub(super) fn submenu_da_pasta(tela: &Rc<Tela>, prefixo: &str, id: u64) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id(format!("{prefixo}-menu"));
    for (ferramenta, icone) in super::controlar::ICONES {
        let (abrir, visivel) = (Rc::clone(tela), Rc::clone(tela));
        menu = menu
            .item(
                format!("{prefixo}-{}", ferramenta.chave()),
                ficha(ferramenta).nome.to_owned(),
                move || abrir.abrir_pasta_com_ia(id, ferramenta),
            )
            .icon(icone)
            .item_visible(move || visivel.config.with(|c| c.ia.mostra(ferramenta)));
    }
    menu
}
