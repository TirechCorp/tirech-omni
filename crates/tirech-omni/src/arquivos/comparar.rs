//! A comparação das duas pastas abertas: o que só existe de um lado, o que é mais novo e o que
//! tem tamanho diferente. É puro (sem signal e sem `Fs`): a aba chama [`comparar`] num efeito e
//! entrega o resultado aos painéis.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime};

use tomni_ui::files::Entry;
use tomni_ui::prelude::RowTone;

pub use crate::configuracoes::ModoComparar;

/// O que a comparação achou de uma entrada.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marca {
    /// Só existe deste lado.
    SoAqui,
    /// Existe dos dois lados e este é o mais novo (modo "data").
    MaisNovo,
    /// Existe dos dois lados com tamanho diferente (modo "tamanho").
    TamanhoDiferente,
}

impl Marca {
    /// A cor da linha no `DataGrid`.
    pub fn tom(self) -> RowTone {
        match self {
            Self::SoAqui => RowTone::Caution,
            Self::MaisNovo => RowTone::Success,
            Self::TamanhoDiferente => RowTone::Critical,
        }
    }
}

/// O resultado da comparação, por nome.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resultado {
    /// As marcas das entradas do painel local.
    pub local: HashMap<String, Marca>,
    /// As marcas das entradas do painel remoto.
    pub remoto: HashMap<String, Marca>,
    /// Os nomes que existem dos dois lados e não ganharam marca em nenhum (nem o `..`, que não
    /// entra na comparação).
    pub identicos: HashSet<String>,
}

/// O que um painel faz com o resultado: as marcas do seu lado e os nomes a esconder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Realce {
    /// A marca de cada nome marcado.
    pub marcas: HashMap<String, Marca>,
    /// Os nomes que somem da lista (os idênticos, com "Ocultar idênticos" ligado).
    pub ocultar: HashSet<String>,
}

impl Resultado {
    /// O realce do painel local (`remoto` falso) ou do remoto, com ou sem os idênticos.
    pub fn realce(&self, remoto: bool, ocultar_identicos: bool) -> Realce {
        Realce {
            marcas: if remoto { &self.remoto } else { &self.local }.clone(),
            ocultar: if ocultar_identicos {
                self.identicos.clone()
            } else {
                HashSet::new()
            },
        }
    }
}

/// Compara as entradas das duas pastas. Os nomes diferenciam maiúsculas de minúsculas.
///
/// - Quem falta do outro lado é [`Marca::SoAqui`], arquivo ou pasta.
/// - Duas pastas só comparam a existência. Uma pasta contra um arquivo (ou outro tipo) conta
///   como tamanho diferente no modo "tamanho" e como idêntico no modo "data".
/// - Um link compara pelo próprio tamanho e data, sem ser seguido.
/// - No modo "data", uma diferença de até `tolerancia` conta como a mesma data, e um lado sem
///   data deixa o par idêntico.
pub fn comparar(
    local: &[Entry],
    remoto: &[Entry],
    modo: ModoComparar,
    tolerancia: Duration,
) -> Resultado {
    let por_nome = |entradas: &'_ [Entry]| -> HashMap<String, Entry> {
        entradas
            .iter()
            .filter(|e| e.name != "..")
            .map(|e| (e.name.clone(), e.clone()))
            .collect()
    };
    let (l, r) = (por_nome(local), por_nome(remoto));
    let mut resultado = Resultado::default();
    for (nome, a) in &l {
        let Some(b) = r.get(nome) else {
            resultado.local.insert(nome.clone(), Marca::SoAqui);
            continue;
        };
        let (marca_local, marca_remota) = par(a, b, modo, tolerancia);
        if let Some(m) = marca_local {
            resultado.local.insert(nome.clone(), m);
        }
        if let Some(m) = marca_remota {
            resultado.remoto.insert(nome.clone(), m);
        }
        if marca_local.is_none() && marca_remota.is_none() {
            resultado.identicos.insert(nome.clone());
        }
    }
    for nome in r.keys().filter(|n| !l.contains_key(*n)) {
        resultado.remoto.insert(nome.clone(), Marca::SoAqui);
    }
    resultado
}

/// As marcas de um par que existe dos dois lados: `(local, remoto)`.
fn par(
    a: &Entry,
    b: &Entry,
    modo: ModoComparar,
    tolerancia: Duration,
) -> (Option<Marca>, Option<Marca>) {
    if a.is_dir() && b.is_dir() {
        return (None, None);
    }
    match modo {
        ModoComparar::Tamanho => {
            if a.is_dir() != b.is_dir() || a.size != b.size {
                (Some(Marca::TamanhoDiferente), Some(Marca::TamanhoDiferente))
            } else {
                (None, None)
            }
        }
        ModoComparar::Data => {
            if a.is_dir() || b.is_dir() {
                return (None, None);
            }
            let (Some(da), Some(db)) = (a.modified, b.modified) else {
                return (None, None);
            };
            match diferenca(da, db) {
                (d, _) if d <= tolerancia => (None, None),
                (_, true) => (Some(Marca::MaisNovo), None),
                (_, false) => (None, Some(Marca::MaisNovo)),
            }
        }
    }
}

/// A distância entre as duas datas e se a primeira é a mais nova.
fn diferenca(a: SystemTime, b: SystemTime) -> (Duration, bool) {
    match a.duration_since(b) {
        Ok(d) => (d, true),
        Err(e) => (e.duration(), false),
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use tomni_ui::files::EntryKind;

    use super::*;

    const MIN: Duration = Duration::from_secs(60);

    fn arquivo(nome: &str, tamanho: u64, segundos: Option<u64>) -> Entry {
        let mut e = Entry::new(nome, EntryKind::File);
        e.size = tamanho;
        e.modified = segundos.map(|s| UNIX_EPOCH + Duration::from_secs(s));
        e
    }

    fn pasta(nome: &str) -> Entry {
        Entry::new(nome, EntryKind::Dir)
    }

    fn por_tamanho(l: &[Entry], r: &[Entry]) -> Resultado {
        comparar(l, r, ModoComparar::Tamanho, MIN)
    }

    fn por_data(l: &[Entry], r: &[Entry]) -> Resultado {
        comparar(l, r, ModoComparar::Data, MIN)
    }

    #[test]
    fn quem_so_existe_de_um_lado_e_amarelo() {
        for modo in ModoComparar::TODAS {
            let r = comparar(
                &[arquivo("a", 1, None), pasta("p")],
                &[arquivo("b", 1, None)],
                *modo,
                MIN,
            );
            assert_eq!(r.local["a"], Marca::SoAqui, "{modo:?}");
            assert_eq!(r.local["p"], Marca::SoAqui, "{modo:?}");
            assert_eq!(r.remoto["b"], Marca::SoAqui, "{modo:?}");
            assert!(r.identicos.is_empty(), "{modo:?}");
        }
    }

    #[test]
    fn tamanho_diferente_marca_os_dois_lados() {
        let r = por_tamanho(&[arquivo("a", 1, None)], &[arquivo("a", 2, None)]);
        assert_eq!(r.local["a"], Marca::TamanhoDiferente);
        assert_eq!(r.remoto["a"], Marca::TamanhoDiferente);
        assert!(r.identicos.is_empty());
    }

    #[test]
    fn tamanho_igual_e_identico_mesmo_com_datas_diferentes() {
        let r = por_tamanho(
            &[arquivo("a", 5, Some(0))],
            &[arquivo("a", 5, Some(10_000))],
        );
        assert!(r.local.is_empty() && r.remoto.is_empty());
        assert!(r.identicos.contains("a"));
    }

    #[test]
    fn no_modo_data_o_mais_novo_e_verde_e_o_velho_fica_sem_cor() {
        let r = por_data(
            &[arquivo("a", 1, Some(1_000))],
            &[arquivo("a", 1, Some(100))],
        );
        assert_eq!(r.local["a"], Marca::MaisNovo);
        assert!(!r.remoto.contains_key("a"));
        // Não é idêntico: o outro lado tem cor, a linha não some com "Ocultar idênticos".
        assert!(r.identicos.is_empty());

        let r = por_data(
            &[arquivo("a", 1, Some(100))],
            &[arquivo("a", 1, Some(1_000))],
        );
        assert!(!r.local.contains_key("a"));
        assert_eq!(r.remoto["a"], Marca::MaisNovo);
    }

    #[test]
    fn no_modo_data_o_tamanho_nao_conta() {
        let r = por_data(&[arquivo("a", 1, Some(5))], &[arquivo("a", 999, Some(5))]);
        assert!(r.identicos.contains("a"));
    }

    #[test]
    fn a_tolerancia_vale_ate_o_limite() {
        // Exatamente a tolerância: mesma data. Um segundo a mais: diferente.
        let r = por_data(&[arquivo("a", 1, Some(160))], &[arquivo("a", 1, Some(100))]);
        assert!(r.identicos.contains("a"));
        let r = por_data(&[arquivo("a", 1, Some(161))], &[arquivo("a", 1, Some(100))]);
        assert_eq!(r.local["a"], Marca::MaisNovo);
    }

    #[test]
    fn tolerancia_zero_exige_a_mesma_data() {
        let l = [arquivo("a", 1, Some(101))];
        let r = [arquivo("a", 1, Some(100))];
        let res = comparar(&l, &r, ModoComparar::Data, Duration::ZERO);
        assert_eq!(res.local["a"], Marca::MaisNovo);
        let res = comparar(&r, &r, ModoComparar::Data, Duration::ZERO);
        assert!(res.identicos.contains("a"));
    }

    #[test]
    fn sem_data_o_modo_data_trata_como_identico() {
        let r = por_data(&[arquivo("a", 1, None)], &[arquivo("a", 1, Some(5))]);
        assert!(r.identicos.contains("a"));
        let r = por_data(&[arquivo("a", 1, None)], &[arquivo("a", 1, None)]);
        assert!(r.identicos.contains("a"));
        // O modo tamanho não depende da data.
        let r = por_tamanho(&[arquivo("a", 1, None)], &[arquivo("a", 2, Some(5))]);
        assert_eq!(r.local["a"], Marca::TamanhoDiferente);
    }

    #[test]
    fn pastas_so_comparam_a_existencia() {
        for modo in ModoComparar::TODAS {
            let r = comparar(&[pasta("p")], &[pasta("p")], *modo, MIN);
            assert!(r.identicos.contains("p"), "{modo:?}");
            assert!(r.local.is_empty() && r.remoto.is_empty(), "{modo:?}");
        }
    }

    #[test]
    fn o_tamanho_e_a_data_de_uma_pasta_nao_entram() {
        let mut a = pasta("p");
        a.size = 4096;
        a.modified = Some(UNIX_EPOCH + Duration::from_secs(9_999));
        let b = pasta("p");
        for modo in ModoComparar::TODAS {
            let r = comparar(&[a.clone()], std::slice::from_ref(&b), *modo, MIN);
            assert!(r.identicos.contains("p"), "{modo:?}");
        }
    }

    #[test]
    fn pasta_contra_arquivo_e_tamanho_diferente_so_no_modo_tamanho() {
        let r = por_tamanho(&[pasta("x")], &[arquivo("x", 0, None)]);
        assert_eq!(r.local["x"], Marca::TamanhoDiferente);
        assert_eq!(r.remoto["x"], Marca::TamanhoDiferente);
        let r = por_data(&[pasta("x")], &[arquivo("x", 0, Some(5))]);
        assert!(r.identicos.contains("x"));
    }

    #[test]
    fn os_nomes_diferenciam_maiusculas() {
        let r = por_tamanho(
            &[arquivo("Index.html", 1, None)],
            &[arquivo("index.html", 1, None)],
        );
        assert_eq!(r.local["Index.html"], Marca::SoAqui);
        assert_eq!(r.remoto["index.html"], Marca::SoAqui);
    }

    #[test]
    fn um_link_compara_pelo_proprio_tamanho() {
        let mut a = Entry::new("atalho", EntryKind::Symlink);
        a.size = 7;
        let mut b = Entry::new("atalho", EntryKind::Symlink);
        b.size = 9;
        let r = por_tamanho(&[a], &[b]);
        assert_eq!(r.local["atalho"], Marca::TamanhoDiferente);
    }

    #[test]
    fn o_dois_pontos_nunca_entra() {
        let r = por_tamanho(&[arquivo("..", 1, None)], &[]);
        assert!(r.local.is_empty() && r.identicos.is_empty());
    }

    #[test]
    fn identicos_sao_os_sem_marca_dos_dois_lados() {
        let r = por_tamanho(
            &[arquivo("igual", 3, None), arquivo("muda", 1, None)],
            &[arquivo("igual", 3, None), arquivo("muda", 2, None)],
        );
        assert_eq!(r.identicos.len(), 1);
        assert!(r.identicos.contains("igual"));
    }

    #[test]
    fn o_realce_leva_as_marcas_do_lado_e_os_identicos_so_se_pedidos() {
        let r = por_tamanho(
            &[arquivo("igual", 3, None), arquivo("so_aqui", 1, None)],
            &[arquivo("igual", 3, None), arquivo("so_la", 1, None)],
        );
        let local = r.realce(false, true);
        assert_eq!(local.marcas.len(), 1);
        assert_eq!(local.marcas["so_aqui"], Marca::SoAqui);
        assert!(local.ocultar.contains("igual"));
        let remoto = r.realce(true, false);
        assert_eq!(remoto.marcas["so_la"], Marca::SoAqui);
        assert!(remoto.ocultar.is_empty());
    }

    #[test]
    fn cada_marca_tem_o_seu_tom() {
        assert_eq!(Marca::SoAqui.tom(), RowTone::Caution);
        assert_eq!(Marca::MaisNovo.tom(), RowTone::Success);
        assert_eq!(Marca::TamanhoDiferente.tom(), RowTone::Critical);
    }
}
