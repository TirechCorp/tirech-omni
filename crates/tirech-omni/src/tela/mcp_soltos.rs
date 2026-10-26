//! Os arquivos que a pessoa solta (ou cola) no app e o assistente precisa saber.
//!
//! - Numa aba que um assistente está usando, o caminho não vai ao shell (atrapalharia o próximo
//!   `executar` dele): o app guarda o aviso, e a próxima resposta do MCP sobre aquela aba o leva.
//! - O que vai como caminho deste computador (na aba do próprio assistente, por exemplo) passa a
//!   valer como origem do `enviar_arquivo`, além da pasta de troca: soltar o arquivo já é
//!   entregá-lo.

use std::path::{Component, Path, PathBuf};

use serde_json::{Value, json};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::t;

/// Um arquivo solto numa aba que um assistente usa, à espera de ele ficar sabendo.
pub(super) struct Solto {
    /// O caminho no servidor (enviado) ou neste computador.
    pub caminho: String,
    /// Se foi enviado ao servidor.
    pub enviado: bool,
}

impl Tela {
    /// Se a colagem na `sessao` vai para o assistente em vez do shell: guarda o aviso, avisa a
    /// pessoa e devolve `true`. `enviado` diz se os `caminhos` são do servidor.
    pub(super) fn entregar_ao_assistente(
        &self,
        sessao: Session,
        janela: WindowHandle,
        caminhos: &[String],
        enviado: bool,
    ) -> bool {
        let chave = sessao.key_str();
        if caminhos.is_empty() || !self.aba_com_assistente(&chave) {
            return false;
        }
        self.mcp
            .soltos
            .borrow_mut()
            .entry(chave)
            .or_default()
            .extend(caminhos.iter().map(|caminho| Solto {
                caminho: caminho.clone(),
                enviado,
            }));
        let lista = caminhos.join(", ");
        let texto = if enviado {
            t!("colar-entregue-enviado", caminhos = lista)
        } else {
            t!("colar-entregue-local", caminhos = lista)
        };
        self.avisar_entrega(janela, texto);
        true
    }

    /// O aviso dos arquivos soltos nas abas `chaves`, que sai da fila: cada um é dito uma vez.
    pub(super) fn nota_dos_soltos(&self, chaves: &[String]) -> Option<String> {
        let mut soltos = self.mcp.soltos.borrow_mut();
        let linhas: Vec<String> = chaves
            .iter()
            .filter_map(|chave| soltos.remove(chave).map(|lista| (chave, lista)))
            .flat_map(|(chave, lista)| {
                let (enviados, locais): (Vec<Solto>, Vec<Solto>) =
                    lista.into_iter().partition(|s| s.enviado);
                let juntar = |v: Vec<Solto>| {
                    v.into_iter()
                        .map(|s| s.caminho)
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let mut linhas = Vec::new();
                if !enviados.is_empty() {
                    linhas.push(t!(
                        "mcp-soltos-enviados",
                        aba = chave.as_str(),
                        caminhos = juntar(enviados)
                    ));
                }
                if !locais.is_empty() {
                    linhas.push(t!(
                        "mcp-soltos-locais",
                        aba = chave.as_str(),
                        caminhos = juntar(locais)
                    ));
                }
                linhas
            })
            .collect();
        (!linhas.is_empty()).then(|| linhas.join("\n\n"))
    }

    /// O que a sessão `ligacao` ainda não sabe, e que sai da fila: os recados dela (as abas que
    /// a pessoa deu) e os arquivos soltos nas abas `outras` (a do pedido), na aba presa a ela e
    /// nas que a pessoa deu. Vai somado à próxima resposta ou pelo hook dos avisos.
    pub(super) fn avisos_da_sessao(&self, ligacao: u64, outras: &[String]) -> Option<String> {
        let dadas = self
            .mcp
            .dadas
            .borrow()
            .get(&ligacao)
            .cloned()
            .unwrap_or_default();
        let chaves: Vec<String> = outras
            .iter()
            .cloned()
            .chain(self.aba_da_sessao(ligacao))
            .chain(dadas)
            .collect();
        let mut partes = self
            .mcp
            .recados
            .borrow_mut()
            .remove(&ligacao)
            .unwrap_or_default();
        partes.extend(self.nota_dos_soltos(&chaves));
        (!partes.is_empty()).then(|| partes.join("\n\n"))
    }

    /// Os arquivos soltos na aba `chave`, para o `listar_abas` (saem da fila).
    pub(super) fn soltos_da_aba(&self, chave: &str) -> Option<Value> {
        let lista = self.mcp.soltos.borrow_mut().remove(chave)?;
        Some(Value::Array(
            lista
                .into_iter()
                .map(|s| json!({ "caminho": s.caminho, "enviado": s.enviado }))
                .collect(),
        ))
    }

    /// Os caminhos deste computador que a pessoa colou ou soltou: valem como origem do
    /// `enviar_arquivo`.
    pub(super) fn liberar_para_o_envio(&self, caminhos: &[String]) {
        let mut entregues = self.mcp.entregues.borrow_mut();
        for caminho in caminhos {
            let caminho = PathBuf::from(caminho);
            if caminho.is_absolute() && !entregues.contains(&caminho) {
                entregues.push(caminho);
            }
        }
    }

    /// `pedido` como um arquivo que a pessoa entregou: ele mesmo ou um que está dentro de uma
    /// pasta entregue. Os links simbólicos são resolvidos antes de conferir.
    pub(super) fn origem_entregue(&self, pedido: &str) -> Option<PathBuf> {
        let pedido = Path::new(pedido);
        if !pedido.is_absolute() || pedido.components().any(|c| c == Component::ParentDir) {
            return None;
        }
        let real = pedido.canonicalize().ok()?;
        self.mcp
            .entregues
            .borrow()
            .iter()
            .filter_map(|e| e.canonicalize().ok())
            .any(|e| real.starts_with(&e))
            .then_some(real)
    }
}

/// Soma a `nota` ao fim do primeiro texto do resultado de uma ferramenta.
pub(super) fn somar_nota(resultado: &mut Value, nota: &str) {
    let Some(conteudo) = resultado.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    let antes = conteudo
        .first()
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    match antes {
        Some(antes) => conteudo[0]["text"] = Value::String(format!("{antes}\n\n{nota}")),
        None => conteudo.push(json!({ "type": "text", "text": nota })),
    }
}
