//! O lançador "Links…" do menu Ferramentas: os links da árvore numa grade com filtro, para
//! abrir sem procurar na árvore. Cada um abre como pela árvore (`Tela::abrir_link_da_arvore`):
//! no navegador padrão ou no do perfil de rede.

use std::collections::BTreeSet;
use std::rc::Rc;

use tomni_ui::prelude::*;

use super::Tela;
use crate::cluster;
use crate::redes::rede_efetiva;
use crate::t;

/// O que o diálogo "Links" guarda enquanto está na tela.
pub(super) struct Estado {
    /// O diálogo está na tela.
    aberto: Signal<bool>,
    /// O texto do filtro.
    filtro: Signal<String>,
    /// Os ids (como texto) dos links escolhidos.
    escolhidos: Signal<BTreeSet<String>>,
    /// As linhas da grade: os links que o filtro deixa passar.
    linhas: Signal<Vec<DataGridRow>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            filtro: signal(String::new()),
            escolhidos: signal(BTreeSet::new()),
            linhas: signal(Vec::new()),
        }
    }
}

impl Tela {
    /// Mostra o lançador "Links…" com o filtro limpo.
    pub(super) fn abrir_lancador_de_links(&self) {
        self.links.filtro.set(String::new());
        self.links.escolhidos.set(BTreeSet::new());
        self.links.aberto.set(true);
    }

    /// Abre os links `ids` (na ordem da árvore) e fecha o lançador.
    fn abrir_links(self: &Rc<Self>, ids: &BTreeSet<String>) {
        let ordem: Vec<u64> = self.tree.with_untracked(|t| {
            cluster::links(t)
                .into_iter()
                .map(|l| l.id)
                .filter(|id| ids.contains(&id.to_string()))
                .collect()
        });
        self.links.aberto.set(false);
        for id in ordem {
            self.abrir(id, self.lugar_novo());
        }
    }
}

/// O diálogo dos Links: o efeito das linhas liga uma vez, e a função devolvida monta a caixa em
/// cada janela.
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl Fn() -> ContentDialog + use<> {
    let estado = &tela.links;
    let (aberto, filtro, escolhidos, linhas) = (
        estado.aberto,
        estado.filtro,
        estado.escolhidos,
        estado.linhas,
    );
    // As linhas acompanham a árvore e o filtro.
    let arvore = tela.tree;
    effect(move || {
        let busca = filtro.get();
        let novas = arvore.with(|t| {
            cluster::links(t)
                .into_iter()
                .filter(|l| l.combina(&busca))
                .map(|l| {
                    let rede = rede_efetiva(t, l.id)
                        .map_or_else(|| t!("redes-direta"), |p| p.name.clone());
                    DataGridRow::new(l.id.to_string())
                        .cell(l.nome.clone(), CellValue::Text(l.nome))
                        .cell(l.host.clone(), CellValue::Text(l.host))
                        .cell(rede.clone(), CellValue::Text(rede))
                        .cell(l.grupo.clone(), CellValue::Text(l.grupo))
                        .icon(Symbol::Globe)
                })
                .collect::<Vec<_>>()
        });
        linhas.set(novas);
    });
    let tela = Rc::clone(tela);
    move || {
        let tela = &tela;
        let grade = {
            let t = Rc::clone(tela);
            DataGrid::bind(linhas, escolhidos)
                .id("links-grade")
                .height(260)
                .columns([
                    DataGridColumn::new("nome", t!("comum-nome")).width(ColumnWidth::Star(1.0)),
                    DataGridColumn::new("endereco", t!("comum-endereco"))
                        .width(ColumnWidth::Star(1.6)),
                    DataGridColumn::new("rede", t!("comum-rede")).width(ColumnWidth::Star(0.8)),
                    DataGridColumn::new("grupo", t!("comum-grupo")).width(ColumnWidth::Star(0.8)),
                ])
                .empty_text(t!("links-nenhum-link"))
                .on_invoke(move |chave| t.abrir_links(&BTreeSet::from([chave])))
        };
        let abrir = {
            let t = Rc::clone(tela);
            Button::new(t!("comum-abrir"))
                .id("links-abrir")
                .accent()
                .disabled(move || escolhidos.with(BTreeSet::is_empty))
                .on_click(move || t.abrir_links(&escolhidos.get_untracked()))
        };
        ContentDialog::new(t!("links-titulo"), aberto)
            .id("links-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(t!("links-explicacao")).wrap())
                    .child(
                        Edit::bind(filtro)
                            .id("links-filtro")
                            .placeholder(t!("comum-filtrar"))
                            .height(32),
                    )
                    .child(grade)
                    .child(
                        Row::new()
                            .gap(8)
                            .align(Align::Center)
                            .child(Row::new().grow(1))
                            .child(abrir),
                    ),
            )
            .close(t!("comum-fechar"))
    }
}
