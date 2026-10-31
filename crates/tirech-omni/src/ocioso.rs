//! Fechar a aba ociosa: depois de um tempo sem nada digitado, colado ou mandado por ela (por
//! você ou por um assistente), a aba fecha sozinha. A conexão escolhe, ou herda da pasta mais
//! perto que escolheu; sem nenhuma, vale Configurações › Conexões › Fechar a aba sem uso após,
//! que vem desligado. A parte pura: a herança, os textos da linha e o relógio de cada aba. O que
//! liga à tela mora em `tela/ocioso.rs`. Os textos dos tempos e do "Herdar" são os da linha
//! "Tentar de novo após" ([`crate::retentar`]).

use std::time::{Duration, Instant};

use crate::modelo::{Node, Ocioso, Tree};
use crate::retentar::{desligado, tempo};
use crate::t;

/// Os tempos que a linha oferece, em minutos.
pub const MINUTOS: [u32; 9] = [5, 10, 15, 30, 60, 120, 240, 480, 720];

/// O que uma escolha decide: `None` segue subindo. `Minutos(0)` é `Desligado`.
fn decide(ocioso: Ocioso) -> Option<Ocioso> {
    match ocioso {
        Ocioso::Herdar => None,
        Ocioso::Minutos(0) => Some(Ocioso::Desligado),
        outra => Some(outra),
    }
}

/// A escolha gravada no nó `id` (conexão ou pasta); `Herdar` se ele não existe.
pub fn do_no(tree: &Tree, id: u64) -> Ocioso {
    match tree.find(id) {
        Some(Node::Connection(c)) => c.ocioso,
        Some(Node::Group(g)) => g.ocioso,
        None => Ocioso::Herdar,
    }
}

/// A pasta que decide o tempo do item `no`, subindo a partir da de cima, e a escolha dela.
/// `None` se nenhuma pasta decide.
pub fn herdada(tree: &Tree, no: u64) -> Option<(&str, Ocioso)> {
    for grupo in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(grupo)
            && let Some(ocioso) = decide(g.ocioso)
        {
            return Some((&g.name, ocioso));
        }
    }
    None
}

/// O tempo da conexão `conexao`: o dela ou o da pasta. `None` segue as Configurações (também
/// para uma conexão que não existe).
pub fn da_conexao(tree: &Tree, conexao: u64) -> Option<Ocioso> {
    let c = tree.connection(conexao)?;
    decide(c.ocioso).or_else(|| herdada(tree, conexao).map(|(_, ocioso)| ocioso))
}

/// Depois de quanto tempo sem uso a aba fecha, com a escolha `ocioso` (`None`: os `padrao_mins`
/// das Configurações, em que 0 desliga). `None` não fecha.
pub fn limite(ocioso: Option<Ocioso>, padrao_mins: u32) -> Option<Duration> {
    let minutos = match ocioso.and_then(decide) {
        Some(Ocioso::Minutos(m)) => m,
        Some(Ocioso::Desligado) => 0,
        Some(Ocioso::Herdar) | None => padrao_mins,
    };
    (minutos > 0).then(|| Duration::from_secs(u64::from(minutos) * 60))
}

/// O texto de uma escolha que vale.
fn rotulo_da_escolha(ocioso: Ocioso) -> String {
    match ocioso {
        Ocioso::Minutos(m) if m > 0 => tempo(m),
        Ocioso::Minutos(_) | Ocioso::Desligado => desligado(),
        Ocioso::Herdar => t!("retentar-herdar-configuracoes"),
    }
}

/// O texto da opção "Herdar" do item `no`: diz de onde vem.
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    match herdada(tree, no) {
        Some((nome, _)) => t!("retentar-herdar-de", nome = nome),
        None => t!("retentar-herdar-configuracoes"),
    }
}

/// O texto da linha para `ocioso` no item `no`.
pub fn rotulo(tree: &Tree, no: u64, ocioso: Ocioso) -> String {
    match decide(ocioso) {
        None => rotulo_de_herdar(tree, no),
        Some(ocioso) => rotulo_da_escolha(ocioso),
    }
}

/// Os minutos que a linha oferece para quem está em `atual`: os de [`MINUTOS`] e, se o arquivo
/// tem outro, também ele, no lugar dele.
fn minutos_com(atual: Ocioso) -> Vec<u32> {
    let mut minutos = MINUTOS.to_vec();
    if let Ocioso::Minutos(m) = atual
        && m > 0
        && !minutos.contains(&m)
    {
        minutos.push(m);
        minutos.sort_unstable();
    }
    minutos
}

/// As opções da linha do item `no`, que está em `atual`.
pub fn opcoes(tree: &Tree, no: u64, atual: Ocioso) -> Vec<String> {
    let mut opcoes = vec![rotulo_de_herdar(tree, no), desligado()];
    opcoes.extend(minutos_com(atual).into_iter().map(tempo));
    opcoes
}

/// A escolha que o texto da linha faz, para quem está em `atual`; um texto que não é de nenhuma
/// opção é "Herdar".
pub fn do_rotulo(texto: &str, atual: Ocioso) -> Ocioso {
    if texto == desligado() {
        return Ocioso::Desligado;
    }
    minutos_com(atual)
        .into_iter()
        .find(|&m| tempo(m) == texto)
        .map_or(Ocioso::Herdar, Ocioso::Minutos)
}

/// O último uso visto de uma aba: quantos bytes já tinham ido por ela e desde quando.
#[derive(Clone, Copy, Debug)]
pub struct Uso {
    enviados: u64,
    desde: Instant,
}

impl Uso {
    /// A aba vista pela primeira vez: o relógio começa agora, mesmo que ela já estivesse parada.
    pub fn novo(enviados: u64, agora: Instant) -> Self {
        Self {
            enviados,
            desde: agora,
        }
    }

    /// Lê a aba de novo: `enviados` é o total de bytes que foram por ela, e `mexida`, quando um
    /// assistente mexeu nela por último. Devolve se ela passou de `limite` sem uso.
    pub fn passou(
        &mut self,
        enviados: u64,
        mexida: Option<Instant>,
        agora: Instant,
        limite: Duration,
    ) -> bool {
        if enviados != self.enviados {
            *self = Self::novo(enviados, agora);
            return false;
        }
        if let Some(mexida) = mexida
            && mexida > self.desde
        {
            self.desde = mexida;
        }
        agora.saturating_duration_since(self.desde) >= limite
    }
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

    fn mudar(tree: &mut Tree, id: u64, ocioso: Ocioso) {
        match tree.find_mut(id) {
            Some(Node::Group(g)) => g.ocioso = ocioso,
            Some(Node::Connection(c)) => c.ocioso = ocioso,
            None => panic!("o item {id} não existe"),
        }
    }

    #[test]
    fn herda_da_pasta_mais_perto_que_decide() {
        let (mut tree, pasta, dentro, id) = arvore();
        assert_eq!(da_conexao(&tree, id), None);
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Configurações)");

        mudar(&mut tree, pasta, Ocioso::Minutos(10));
        assert_eq!(da_conexao(&tree, id), Some(Ocioso::Minutos(10)));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Clientes)");

        mudar(&mut tree, dentro, Ocioso::Desligado);
        assert_eq!(da_conexao(&tree, id), Some(Ocioso::Desligado));
        assert_eq!(rotulo(&tree, dentro, Ocioso::Herdar), "Herdar (Clientes)");

        mudar(&mut tree, id, Ocioso::Minutos(30));
        assert_eq!(da_conexao(&tree, id), Some(Ocioso::Minutos(30)));
        assert_eq!(da_conexao(&tree, 999), None);
    }

    #[test]
    fn o_limite_segue_a_escolha_e_sem_ela_as_configuracoes() {
        let min = |m: u64| Some(Duration::from_secs(m * 60));
        assert_eq!(limite(None, 0), None, "o padrão não fecha");
        assert_eq!(limite(None, 10), min(10));
        assert_eq!(limite(Some(Ocioso::Herdar), 10), min(10));
        assert_eq!(limite(Some(Ocioso::Desligado), 10), None);
        assert_eq!(limite(Some(Ocioso::Minutos(0)), 10), None);
        assert_eq!(limite(Some(Ocioso::Minutos(15)), 0), min(15));
    }

    #[test]
    fn os_rotulos_voltam_a_escolha() {
        let (tree, _, _, id) = arvore();
        let opcoes = opcoes(&tree, id, Ocioso::Herdar);
        assert_eq!(
            opcoes,
            [
                "Herdar (Configurações)",
                "Desligado",
                "5 min",
                "10 min",
                "15 min",
                "30 min",
                "1 h",
                "2 h",
                "4 h",
                "8 h",
                "12 h"
            ]
        );
        assert_eq!(do_rotulo(&opcoes[0], Ocioso::Herdar), Ocioso::Herdar);
        assert_eq!(do_rotulo(&opcoes[1], Ocioso::Herdar), Ocioso::Desligado);
        for escolha in MINUTOS.map(Ocioso::Minutos) {
            let texto = rotulo(&tree, id, escolha);
            assert_eq!(do_rotulo(&texto, Ocioso::Herdar), escolha, "{texto}");
        }
        // Um tempo que só o arquivo tem entra nas opções.
        let atual = Ocioso::Minutos(7);
        assert!(super::opcoes(&tree, id, atual).contains(&"7 min".to_owned()));
        assert_eq!(do_rotulo("7 min", atual), atual);
    }

    #[test]
    fn o_relogio_volta_a_zero_com_o_uso() {
        let comeco = Instant::now();
        let min = |m: u64| comeco + Duration::from_secs(m * 60);
        let limite = Duration::from_secs(10 * 60);
        let mut uso = Uso::novo(100, comeco);
        assert!(!uso.passou(100, None, min(9), limite));
        // Digitou: o relógio recomeça.
        assert!(!uso.passou(105, None, min(9), limite));
        assert!(!uso.passou(105, None, min(18), limite));
        assert!(uso.passou(105, None, min(19), limite));
        // Um assistente mexeu (sem digitar): também recomeça.
        let mut uso = Uso::novo(0, comeco);
        assert!(!uso.passou(0, Some(min(8)), min(15), limite));
        assert!(uso.passou(0, Some(min(8)), min(18), limite));
    }
}
