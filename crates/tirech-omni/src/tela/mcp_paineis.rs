//! As ferramentas do MCP que olham e mudam os painéis de uma janela (M51, seção 6.8):
//! `listar_paineis` e `organizar_paineis`. A permissão delas vem de todas as conexões liberadas
//! (`permissao_mais_alta`), não de uma só; a decisão já aconteceu quando o código daqui roda.

use std::rc::Rc;
use std::time::Instant;

use serde_json::{Value, json};
use tomni_ui::prelude::*;

use super::mcp::{Plano, Saida, Trabalho};
use super::mcp_terminal::numero;
use super::{Janela, Tela};
use crate::cluster::LIMITE_DA_GRADE;
use crate::espacos::id_da_chave;
use crate::mcp::permissao::{PermissaoIa, permissao_efetiva};
use crate::t;

impl Tela {
    /// A janela que o pedido aponta: `"atual"` (ou nada) ou o número dela, a partir de 1.
    pub(super) fn janela_numerada(&self, valor: &Value) -> Result<Janela, String> {
        match valor {
            Value::Null => Ok(self.janela_ativa()),
            Value::String(s) if s == "atual" => Ok(self.janela_ativa()),
            outro => {
                let n = numero(outro).ok_or_else(|| t!("mcp-janela-invalida"))?;
                self.janelas
                    .todas_agora()
                    .get(n - 1)
                    .copied()
                    .ok_or_else(|| t!("mcp-janela-nao-existe", n = n.to_string()))
            }
        }
    }

    /// O número da janela (a partir de 1) na lista de janelas.
    pub(super) fn numero_da_janela(&self, janela: Janela) -> usize {
        self.janelas.numero_agora(janela.handle).unwrap_or(1)
    }

    /// A janela, as colunas e as linhas de um `organizar_paineis`.
    pub(super) fn grade_pedida(&self, args: &Value) -> Result<(Janela, usize, usize), String> {
        let janela = self.janela_numerada(&args["janela"])?;
        let valida = |n: Option<usize>| n.filter(|&n| n <= LIMITE_DA_GRADE);
        let (colunas, linhas) = (
            valida(numero(&args["colunas"])),
            valida(numero(&args["linhas"])),
        );
        match (colunas, linhas) {
            (Some(c), Some(l)) if c * l <= LIMITE_DA_GRADE => Ok((janela, c, l)),
            _ => Err(t!(
                "mcp-grade-invalida",
                limite = LIMITE_DA_GRADE.to_string()
            )),
        }
    }

    /// `listar_paineis`: como as abas das conexões liberadas estão nos painéis da janela, com o
    /// nome das sessões de assistente que são donas delas.
    pub(super) fn mcp_listar_paineis(self: &Rc<Self>, t: Rc<Trabalho>) {
        let janela = match self.janela_numerada(&t.pedido.args["janela"]) {
            Ok(janela) => janela,
            Err(erro) => return self.terminar_mcp(&t, Saida::Erro(erro)),
        };
        let liberada = |chave: &str| {
            id_da_chave(chave).is_some_and(|id| {
                self.tree
                    .with_untracked(|tr| permissao_efetiva(tr, id) != PermissaoIa::Desligada)
            })
        };
        let (total, paineis) = janela.layout.with_untracked(|l| {
            let paineis: Vec<(Vec<String>, Option<String>)> = (0..l.pane_count())
                .map(|i| {
                    let painel = l.pane(i);
                    let abas = painel
                        .tabs
                        .iter()
                        .map(|s| s.key_str())
                        .filter(|chave| liberada(chave))
                        .collect();
                    let ativa = painel
                        .selected
                        .map(|s| s.key_str())
                        .filter(|chave| liberada(chave));
                    (abas, ativa)
                })
                .collect();
            (l.pane_count(), paineis)
        });
        let (colunas, linhas) = {
            let (c, l) = janela.grade.get_untracked();
            if c * l == total { (c, l) } else { (total, 1) }
        };
        let donos = self.mcp.donos.borrow();
        let nome_do_dono = |chave: &str| {
            donos
                .get(chave)
                .and_then(|&ligacao| self.sessao_ia(ligacao))
                .map(|s| s.nome)
        };
        let paineis: Vec<Value> = paineis
            .into_iter()
            .enumerate()
            .map(|(i, (abas, ativa))| {
                let mut nomes: Vec<String> = Vec::new();
                for nome in abas.iter().filter_map(|a| nome_do_dono(a)) {
                    if !nomes.contains(&nome) {
                        nomes.push(nome);
                    }
                }
                json!({
                    "painel": i + 1,
                    "abas": abas,
                    "ativa": ativa,
                    "donos": nomes,
                })
            })
            .collect();
        drop(donos);
        let resumo = json!({
            "janela": self.numero_da_janela(janela),
            "colunas": colunas,
            "linhas": linhas,
            "paineis": paineis,
        });
        let texto = serde_json::to_string_pretty(&resumo).unwrap_or_else(|_| "{}".to_owned());
        self.terminar_mcp(&t, Saida::servidor("paineis", texto, None));
    }

    /// `organizar_paineis`: refaz a janela como uma grade de `colunas` × `linhas`. As abas
    /// continuam abertas: vão para os painéis na ordem, uma por painel, e as que sobram ficam
    /// no último (é o que `PaneLayout::grid` faz).
    pub(super) fn mcp_organizar_paineis(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let (janela, colunas, linhas) = match self.grade_pedida(&t.pedido.args) {
            Ok(grade) => grade,
            Err(erro) => return self.terminar_mcp(&t, Saida::Erro(erro)),
        };
        let numero = plano
            .grade
            .map_or_else(|| self.numero_da_janela(janela), |(j, _, _)| j);
        self.aplicar_grade(janela, colunas, linhas);
        self.mcp.grades.borrow_mut().insert(
            numero,
            (t.pedido.ligacao, (colunas, linhas), Instant::now()),
        );
        let texto = t!(
            "mcp-paineis-organizados",
            janela = numero.to_string(),
            colunas = colunas.to_string(),
            linhas = linhas.to_string()
        );
        self.terminar_mcp(&t, Saida::App(texto));
    }
}
