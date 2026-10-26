//! O estado das conexões na árvore (M62): um selo no canto do ícone (verde, âmbar, vermelho ou
//! cinza), a dica com o estado e o motivo da queda, e no menu de contexto o "Reconectar" e o
//! "Ir para a aba". A pasta recolhida mostra o estado mais grave de dentro, para uma queda não
//! sumir num grupo fechado.

use std::collections::HashMap;
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::dica_da_aba::EstadoDaAba;
use crate::espacos::id_da_chave;
use crate::janelas::Janelas;
use crate::modelo::Node;
use crate::t;

/// Os estados das sessões de cada conexão (o id da conexão), lidos de todas as janelas. Sessões
/// sem conexão na árvore (o shell local avulso, o WSL) ficam de fora. Reativo: muda com a
/// abertura, a queda e o fim de cada sessão.
pub(super) fn estados_das_conexoes(janelas: Janelas) -> Memo<HashMap<u64, Vec<EstadoDaAba>>> {
    memo(move || {
        let mut estados: HashMap<u64, Vec<EstadoDaAba>> = HashMap::new();
        for janela in janelas.todas() {
            let sessoes = janela.layout.try_with(|l| l.sessions()).unwrap_or_default();
            for sessao in sessoes {
                if let Some(id) = id_da_chave(&sessao.key_str()) {
                    estados
                        .entry(id)
                        .or_default()
                        .push(EstadoDaAba::da_sessao(sessao));
                }
            }
        }
        estados
    })
}

/// O estado que a árvore mostra para uma conexão e o texto da dica, a partir das sessões dela.
/// O selo é o do estado mais grave (caiu, conectando, conectada, encerrada); `None` sem sessão.
/// Com mais de uma sessão a dica conta cada estado ("2 conectadas, 1 caiu: motivo") e, com
/// várias quedas, mostra o motivo da primeira e "e mais N".
pub fn estado_da_conexao(sessoes: &[EstadoDaAba]) -> Option<(ItemStatus, String)> {
    let primeira = sessoes.first()?;
    let (mut conectadas, mut conectando, mut encerradas) = (0_i64, 0_i64, 0_i64);
    let mut motivos: Vec<&str> = Vec::new();
    for sessao in sessoes {
        match sessao {
            EstadoDaAba::Caiu(motivo) => motivos.push(motivo),
            EstadoDaAba::Conectando => conectando += 1,
            EstadoDaAba::Conectada => conectadas += 1,
            EstadoDaAba::Encerrada => encerradas += 1,
        }
    }
    let status = if !motivos.is_empty() {
        ItemStatus::Critical
    } else if conectando > 0 {
        ItemStatus::Caution
    } else if conectadas > 0 {
        ItemStatus::Success
    } else {
        ItemStatus::Neutral
    };
    if sessoes.len() == 1 {
        let texto = match primeira {
            EstadoDaAba::Caiu(motivo) => t!("dica-caiu", motivo = motivo.as_str()),
            EstadoDaAba::Conectando => t!("dica-conectando"),
            EstadoDaAba::Conectada => t!("dica-conectada"),
            EstadoDaAba::Encerrada => t!("dica-encerrada"),
        };
        return Some((status, texto));
    }
    let mut partes = Vec::new();
    if conectadas > 0 {
        partes.push(t!("estado-conexao-conectadas", n = conectadas));
    }
    if conectando > 0 {
        partes.push(t!("estado-conexao-conectando", n = conectando));
    }
    if encerradas > 0 {
        partes.push(t!("estado-conexao-encerradas", n = encerradas));
    }
    if !motivos.is_empty() {
        partes.push(t!("estado-conexao-caidas", n = motivos.len() as i64));
    }
    let partes = partes.join(", ");
    let texto = match motivos.split_first() {
        None => partes,
        Some((motivo, [])) => t!(
            "estado-conexao-com-motivo",
            partes = partes.as_str(),
            motivo = *motivo
        ),
        Some((motivo, resto)) => {
            // Fora do `t!` de fora: na 1.89 o temporário do `t!` de dentro não vive até o uso.
            let e_mais = t!(
                "estado-conexao-e-mais",
                motivo = *motivo,
                n = resto.len() as i64
            );
            t!(
                "estado-conexao-com-motivo",
                partes = partes.as_str(),
                motivo = e_mais.as_str()
            )
        }
    };
    Some((status, texto))
}

impl Tela {
    /// Os ids das conexões que a linha `id` da árvore cobre: ela mesma, ou as de dentro da pasta
    /// (em qualquer profundidade).
    fn conexoes_da_linha(&self, id: u64) -> Vec<u64> {
        self.tree.with(|arvore| match arvore.find(id) {
            Some(Node::Group(_)) => arvore.connections_in(id).iter().map(|c| c.id).collect(),
            Some(Node::Connection(_)) => vec![id],
            None => Vec::new(),
        })
    }

    /// O selo e a dica da linha `chave` da árvore. A conexão mostra o estado das sessões dela; a
    /// pasta recolhida, o das sessões de todas as conexões de dentro; a pasta aberta, nada (cada
    /// filho mostra o seu). Lê a árvore, as pastas abertas e o estado das sessões.
    pub(super) fn estado_da_linha(&self, chave: &str) -> Option<(ItemStatus, String)> {
        let id: u64 = chave.parse().ok()?;
        let pasta = self
            .tree
            .with(|arvore| matches!(arvore.find(id), Some(Node::Group(_))));
        if pasta && self.abertos.with(|abertos| abertos.contains(chave)) {
            return None;
        }
        let ids = self.conexoes_da_linha(id);
        let sessoes: Vec<EstadoDaAba> = self.estados_das_conexoes.with(|estados| {
            ids.iter()
                .filter_map(|id| estados.get(id))
                .flatten()
                .cloned()
                .collect()
        });
        estado_da_conexao(&sessoes)
    }

    /// A dica da linha `chave`: a descrição da conexão e, embaixo, o estado das abas dela.
    fn dica_da_linha(&self, chave: &str) -> Option<String> {
        let descricao = chave.parse::<u64>().ok().and_then(|id| {
            self.tree.with(|arvore| {
                arvore
                    .connection(id)
                    .map(|c| c.descricao.trim().to_owned())
                    .filter(|d| !d.is_empty())
            })
        });
        let estado = self.estado_da_linha(chave).map(|(_, texto)| texto);
        match (descricao, estado) {
            (Some(d), Some(e)) => Some(format!("{d}\n\n{e}")),
            (d, e) => d.or(e),
        }
    }

    /// As sessões abertas das conexões da linha `id`, com a janela de cada uma. Lê só o que as
    /// janelas têm agora (para botões e eventos).
    fn sessoes_da_linha(&self, id: u64) -> Vec<(crate::janelas::Janela, Session)> {
        let ids = self.conexoes_da_linha(id);
        self.sessoes_das_janelas(None)
            .into_iter()
            .filter(|(_, s)| id_da_chave(&s.key_str()).is_some_and(|i| ids.contains(&i)))
            .collect()
    }

    /// Se alguma sessão da linha `id` caiu com motivo. Reativo: o item "Reconectar" some quando
    /// a última volta.
    pub(super) fn linha_com_queda(&self, id: u64) -> bool {
        let ids = self.conexoes_da_linha(id);
        self.estados_das_conexoes.with(|estados| {
            ids.iter()
                .filter_map(|id| estados.get(id))
                .flatten()
                .any(|e| matches!(e, EstadoDaAba::Caiu(_)))
        })
    }

    /// Se a linha `id` tem alguma sessão aberta numa aba. Reativo.
    pub(super) fn linha_com_sessao(&self, id: u64) -> bool {
        let ids = self.conexoes_da_linha(id);
        self.estados_das_conexoes
            .with(|estados| ids.iter().any(|id| estados.contains_key(id)))
    }

    /// "Reconectar" da árvore: reabre as sessões caídas da linha `id`, as que têm motivo (a que o
    /// shell encerrou de propósito não volta sozinha).
    pub(super) fn reconectar_linha(&self, id: u64) {
        for (_, sessao) in self.sessoes_da_linha(id) {
            if sessao.closed() && sessao.close_reason().is_some() {
                sessao.restart();
            }
        }
    }

    /// "Ir para a aba": seleciona a primeira aba da conexão `id` e dá o foco à janela dela.
    pub(super) fn ir_para_a_aba(&self, id: u64) {
        let Some((janela, sessao)) = self.sessoes_da_linha(id).into_iter().next() else {
            return;
        };
        janela.layout.update(|l| l.select(sessao));
        janela.handle.focus();
    }
}

/// Registra o estado na árvore: o selo e a dica de cada linha.
pub(super) fn com_estado(arvore: TreeView, tela: &Rc<Tela>) -> TreeView {
    let para_o_selo = Rc::clone(tela);
    let para_a_dica = Rc::clone(tela);
    arvore
        .item_status(move |chave| para_o_selo.estado_da_linha(chave).map(|(status, _)| status))
        .item_tooltip(move |chave| para_a_dica.dica_da_linha(chave))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn caiu(motivo: &str) -> EstadoDaAba {
        EstadoDaAba::Caiu(motivo.to_owned())
    }

    fn estado(sessoes: &[EstadoDaAba]) -> (ItemStatus, String) {
        estado_da_conexao(sessoes).expect("com sessão há estado")
    }

    #[test]
    fn sem_sessao_nao_ha_estado() {
        assert_eq!(estado_da_conexao(&[]), None);
    }

    #[test]
    fn uma_sessao_mostra_o_estado_dela() {
        assert_eq!(
            estado(&[EstadoDaAba::Conectada]),
            (ItemStatus::Success, "Conectada".to_owned())
        );
        assert_eq!(
            estado(&[EstadoDaAba::Conectando]),
            (ItemStatus::Caution, "Conectando…".to_owned())
        );
        assert_eq!(
            estado(&[caiu("conexão recusada")]),
            (ItemStatus::Critical, "Caiu: conexão recusada".to_owned())
        );
        assert_eq!(
            estado(&[EstadoDaAba::Encerrada]),
            (ItemStatus::Neutral, "Encerrada".to_owned())
        );
    }

    #[test]
    fn o_selo_e_o_do_estado_mais_grave() {
        use EstadoDaAba::{Conectada, Conectando, Encerrada};
        let casos = [
            (vec![Conectada, caiu("x")], ItemStatus::Critical),
            (vec![Conectando, Conectada], ItemStatus::Caution),
            (vec![Conectando, caiu("x")], ItemStatus::Critical),
            (vec![Encerrada, Conectada], ItemStatus::Success),
            (vec![Encerrada, Conectando], ItemStatus::Caution),
            (vec![Encerrada, caiu("x")], ItemStatus::Critical),
            (vec![Encerrada, Encerrada], ItemStatus::Neutral),
            (vec![Conectada, Conectada], ItemStatus::Success),
        ];
        for (sessoes, esperado) in casos {
            assert_eq!(estado(&sessoes).0, esperado, "{sessoes:?}");
        }
    }

    #[test]
    fn a_dica_conta_cada_estado_e_traz_o_motivo() {
        use EstadoDaAba::{Conectada, Conectando, Encerrada};
        assert_eq!(
            estado(&[Conectada, Conectada, caiu("tempo esgotado")]).1,
            "2 conectadas, 1 caiu: tempo esgotado"
        );
        assert_eq!(
            estado(&[Conectada, Conectando, Encerrada, caiu("x")]).1,
            "1 conectada, 1 conectando, 1 encerrada, 1 caiu: x"
        );
        assert_eq!(
            estado(&[Conectada, Encerrada]).1,
            "1 conectada, 1 encerrada"
        );
        assert_eq!(estado(&[Conectando, Conectando]).1, "2 conectando");
    }

    #[test]
    fn varias_quedas_mostram_o_primeiro_motivo_e_quantas_mais() {
        use EstadoDaAba::Conectada;
        assert_eq!(estado(&[caiu("a"), caiu("b")]).1, "2 caíram: a e mais 1");
        assert_eq!(
            estado(&[Conectada, caiu("a"), caiu("b"), caiu("c")]).1,
            "1 conectada, 3 caíram: a e mais 2"
        );
    }
}
