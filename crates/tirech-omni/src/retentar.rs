//! A nova rodada de reconexão de cada conexão: depois das tentativas de reconectar sozinho
//! (`conexoes.reconectar`), esperar uns minutos e começar outra rodada, até a conexão voltar, ou
//! parar. A conexão escolhe, ou herda da pasta mais perto que escolheu; sem nenhuma, vale
//! Configurações › Conexões › Tentar de novo após. A parte pura: a herança e os textos da linha.
//! O que liga à tela mora em `tela/retentar.rs`, e a rodada nova, em `tela/ganchos.rs`.

use std::time::Duration;

use crate::modelo::{Node, Retentar, Tree};
use crate::t;

/// Os tempos que a linha oferece, em minutos.
pub const MINUTOS: [u32; 7] = [1, 2, 5, 10, 15, 30, 60];

/// O que uma escolha decide: `None` segue subindo. `Minutos(0)` é `Desligado`.
fn decide(retentar: Retentar) -> Option<Retentar> {
    match retentar {
        Retentar::Herdar => None,
        Retentar::Minutos(0) => Some(Retentar::Desligado),
        outra => Some(outra),
    }
}

/// A pasta que decide a rodada nova do item `no`, subindo a partir da de cima, e a escolha dela.
/// `None` se nenhuma pasta decide.
pub fn herdada(tree: &Tree, no: u64) -> Option<(&str, Retentar)> {
    for grupo in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(grupo)
            && let Some(retentar) = decide(g.retentar)
        {
            return Some((&g.name, retentar));
        }
    }
    None
}

/// A rodada nova da conexão `conexao`: a dela ou a da pasta. `None` segue as Configurações
/// (também para uma conexão que não existe).
pub fn da_conexao(tree: &Tree, conexao: u64) -> Option<Retentar> {
    let c = tree.connection(conexao)?;
    decide(c.retentar).or_else(|| herdada(tree, conexao).map(|(_, retentar)| retentar))
}

/// Quanto esperar antes da rodada nova, com a escolha `retentar` (`None`: os `padrao_mins` das
/// Configurações, em que 0 desliga). `None` não tenta de novo.
pub fn pausa(retentar: Option<Retentar>, padrao_mins: u32) -> Option<Duration> {
    let minutos = match retentar.and_then(decide) {
        Some(Retentar::Minutos(m)) => m,
        Some(Retentar::Desligado) => 0,
        Some(Retentar::Herdar) | None => padrao_mins,
    };
    (minutos > 0).then(|| Duration::from_secs(u64::from(minutos) * 60))
}

/// O tempo por extenso: "5 min" ou, em horas cheias, "1 h".
pub fn tempo(minutos: u32) -> String {
    if minutos >= 60 && minutos.is_multiple_of(60) {
        t!("retentar-horas", n = minutos / 60)
    } else {
        t!("retentar-minutos", n = minutos)
    }
}

/// A opção "Desligado" da linha.
pub fn desligado() -> String {
    t!("retentar-desligado")
}

/// O texto de uma escolha que vale.
fn rotulo_da_escolha(retentar: Retentar) -> String {
    match retentar {
        Retentar::Minutos(m) if m > 0 => tempo(m),
        Retentar::Minutos(_) | Retentar::Desligado => desligado(),
        Retentar::Herdar => t!("retentar-herdar-configuracoes"),
    }
}

/// O texto da opção "Herdar" do item `no`: diz de onde vem (a escolha junto não cabe na coluna,
/// como na linha Credencial).
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    match herdada(tree, no) {
        Some((nome, _)) => t!("retentar-herdar-de", nome = nome),
        None => t!("retentar-herdar-configuracoes"),
    }
}

/// O texto da linha para `retentar` no item `no`.
pub fn rotulo(tree: &Tree, no: u64, retentar: Retentar) -> String {
    match decide(retentar) {
        None => rotulo_de_herdar(tree, no),
        Some(retentar) => rotulo_da_escolha(retentar),
    }
}

/// Os minutos que a linha oferece para quem está em `atual`: os de [`MINUTOS`] e, se o arquivo
/// tem outro, também ele, no lugar dele.
fn minutos_com(atual: Retentar) -> Vec<u32> {
    let mut minutos = MINUTOS.to_vec();
    if let Retentar::Minutos(m) = atual
        && m > 0
        && !minutos.contains(&m)
    {
        minutos.push(m);
        minutos.sort_unstable();
    }
    minutos
}

/// As opções da linha do item `no`, que está em `atual`.
pub fn opcoes(tree: &Tree, no: u64, atual: Retentar) -> Vec<String> {
    let mut opcoes = vec![rotulo_de_herdar(tree, no), desligado()];
    opcoes.extend(minutos_com(atual).into_iter().map(tempo));
    opcoes
}

/// A escolha que o texto da linha faz, para quem está em `atual`; um texto que não é de nenhuma
/// opção é "Herdar".
pub fn do_rotulo(texto: &str, atual: Retentar) -> Retentar {
    if texto == desligado() {
        return Retentar::Desligado;
    }
    minutos_com(atual)
        .into_iter()
        .find(|&m| tempo(m) == texto)
        .map_or(Retentar::Herdar, Retentar::Minutos)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::{Connection, Kind};

    fn arvore() -> (Tree, u64, u64, u64) {
        let mut tree = Tree::default();
        let pasta = tree.add_group(None, "Clientes");
        let dentro = tree.add_group(Some(pasta), "ACME");
        let id = tree.add_connection(Some(dentro), Connection::new("web", Kind::Ssh));
        (tree, pasta, dentro, id)
    }

    fn mudar(tree: &mut Tree, id: u64, retentar: Retentar) {
        match tree.find_mut(id) {
            Some(Node::Group(g)) => g.retentar = retentar,
            Some(Node::Connection(c)) => c.retentar = retentar,
            None => panic!("o item {id} não existe"),
        }
    }

    #[test]
    fn herda_da_pasta_mais_perto_que_decide() {
        let (mut tree, pasta, dentro, id) = arvore();
        assert_eq!(
            da_conexao(&tree, id),
            None,
            "sem escolha, valem as Configurações"
        );
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Configurações)");

        mudar(&mut tree, pasta, Retentar::Minutos(30));
        assert_eq!(da_conexao(&tree, id), Some(Retentar::Minutos(30)));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Clientes)");

        mudar(&mut tree, dentro, Retentar::Desligado);
        assert_eq!(da_conexao(&tree, id), Some(Retentar::Desligado));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (ACME)");
        // A pasta mostra de onde ela herda, sem contar a si mesma.
        assert_eq!(rotulo(&tree, dentro, Retentar::Herdar), "Herdar (Clientes)");

        mudar(&mut tree, id, Retentar::Minutos(1));
        assert_eq!(da_conexao(&tree, id), Some(Retentar::Minutos(1)));
        assert_eq!(da_conexao(&tree, 999), None);
    }

    #[test]
    fn zero_minutos_na_pasta_desliga_e_nao_deixa_passar_para_cima() {
        let (mut tree, pasta, dentro, id) = arvore();
        mudar(&mut tree, pasta, Retentar::Minutos(10));
        mudar(&mut tree, dentro, Retentar::Minutos(0));
        assert_eq!(da_conexao(&tree, id), Some(Retentar::Desligado));
        assert_eq!(rotulo(&tree, dentro, Retentar::Minutos(0)), "Desligado");
    }

    #[test]
    fn a_pausa_segue_a_escolha_e_sem_ela_as_configuracoes() {
        let min = |m: u64| Some(Duration::from_secs(m * 60));
        assert_eq!(pausa(None, 5), min(5));
        assert_eq!(pausa(None, 0), None, "0 nas Configurações desliga");
        assert_eq!(pausa(Some(Retentar::Herdar), 7), min(7));
        assert_eq!(pausa(Some(Retentar::Desligado), 5), None);
        assert_eq!(pausa(Some(Retentar::Minutos(0)), 5), None);
        assert_eq!(pausa(Some(Retentar::Minutos(15)), 0), min(15));
    }

    #[test]
    fn os_rotulos_voltam_a_escolha() {
        let (tree, _, _, id) = arvore();
        let opcoes = opcoes(&tree, id, Retentar::Herdar);
        assert_eq!(
            opcoes,
            [
                "Herdar (Configurações)",
                "Desligado",
                "1 min",
                "2 min",
                "5 min",
                "10 min",
                "15 min",
                "30 min",
                "1 h"
            ]
        );
        assert_eq!(do_rotulo(&opcoes[0], Retentar::Herdar), Retentar::Herdar);
        assert_eq!(do_rotulo(&opcoes[1], Retentar::Herdar), Retentar::Desligado);
        assert_eq!(do_rotulo("5 min", Retentar::Herdar), Retentar::Minutos(5));
        assert_eq!(do_rotulo("1 h", Retentar::Herdar), Retentar::Minutos(60));
        assert_eq!(do_rotulo("qualquer", Retentar::Herdar), Retentar::Herdar);
        for escolha in MINUTOS.map(Retentar::Minutos) {
            let texto = rotulo(&tree, id, escolha);
            assert_eq!(do_rotulo(&texto, Retentar::Herdar), escolha, "{texto}");
        }
    }

    #[test]
    fn um_tempo_que_so_o_arquivo_tem_entra_nas_opcoes_no_lugar_dele() {
        let (tree, _, _, id) = arvore();
        let atual = Retentar::Minutos(120);
        let opcoes = opcoes(&tree, id, atual);
        assert_eq!(opcoes.last().map(String::as_str), Some("2 h"));
        assert_eq!(rotulo(&tree, id, atual), "2 h");
        assert_eq!(do_rotulo("2 h", atual), atual);
        let atual = Retentar::Minutos(7);
        let opcoes = super::opcoes(&tree, id, atual);
        assert_eq!(opcoes[4..6], ["5 min", "7 min"]);
        assert_eq!(do_rotulo("7 min", atual), atual);
    }
}
