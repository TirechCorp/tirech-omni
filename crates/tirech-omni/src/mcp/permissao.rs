//! A permissão por conexão (M51): o que um assistente pode fazer numa conexão pelo MCP.
//!
//! A pessoa escolhe por conexão ou por pasta ([`PermissaoIa`]); o que vale de verdade
//! ([`permissao_efetiva`]) sobe pelas pastas como a rede ([`crate::redes::rede_herdada`]). Cada
//! ferramenta pede [`Exigencia::Ver`] ou [`Exigencia::Mudar`] ([`exigencia`]) e [`decidir`] cruza
//! as duas coisas com a lista de bloqueio.
//!
//! Abrir uma aba nova tem uma escolha à parte ([`AbrirAbaIa`]), que sobe pelas pastas até
//! `ia.mcp_abrir` ([`abrir_aba_efetiva`]) e aperta a decisão ([`decidir_abertura`]).

use serde::{Deserialize, Serialize};

use crate::configuracoes::AbrirAbaIa;
use crate::modelo::{Node, Tree};

/// O que um assistente pode fazer numa conexão pelo MCP.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissaoIa {
    /// Herda da pasta (o padrão de conexões e pastas novas). Na raiz, vale `Desligada`.
    #[default]
    Herdar,
    /// A conexão não aparece para o assistente.
    Desligada,
    /// Vê a conexão, as abas abertas dela, a tela, os arquivos e os túneis. Não digita, não
    /// roda, não grava, não abre aba.
    SoLer,
    /// Tudo, mas cada ação que muda alguma coisa espera a aprovação da pessoa no app.
    Aprovar,
    /// Tudo sem perguntar, menos o que a lista de bloqueio pega.
    Livre,
}

impl PermissaoIa {
    /// Para o `skip_serializing_if`: o padrão não vai para o arquivo.
    pub fn is_herdar(&self) -> bool {
        *self == Self::Herdar
    }
}

/// O que uma ferramenta faz com a conexão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exigencia {
    /// Só olha: a tela, a lista de arquivos, os túneis.
    Ver,
    /// Muda alguma coisa: digita, roda, grava, abre ou fecha.
    Mudar,
}

/// O que o app faz com um pedido.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decisao {
    /// Faz sem perguntar.
    Permitir,
    /// Espera a aprovação da pessoa no app.
    Perguntar,
    /// Recusa, com o motivo em português para o assistente ler.
    Recusar(String),
}

/// As ferramentas que só olham.
const FERRAMENTAS_VER: [&str; 13] = [
    "listar_conexoes",
    "listar_abas",
    "ler_tela",
    "esperar",
    "listar_arquivos",
    "baixar_arquivo",
    "listar_tuneis",
    "listar_portas",
    "ler_saida",
    "listar_espacos",
    "ler",
    "buscar",
    "glob",
];

/// As ferramentas que mudam alguma coisa.
const FERRAMENTAS_MUDAR: [&str; 14] = [
    "abrir_conexao",
    "fechar_aba",
    "reconectar_aba",
    "digitar",
    "teclas",
    "executar",
    "enviar_arquivo",
    "ligar_tunel",
    "desligar_tunel",
    "encaminhar_porta",
    "parar_porta",
    "editar",
    "escrever",
    "bash",
];

/// O que a ferramenta `nome` exige. Uma que o app não conhece exige `Mudar` (o lado seguro).
pub fn exigencia(ferramenta: &str) -> Exigencia {
    if FERRAMENTAS_VER.contains(&ferramenta) {
        Exigencia::Ver
    } else {
        Exigencia::Mudar
    }
}

/// Todas as ferramentas que têm exigência (para o teste que confere a lista do servidor).
pub fn ferramentas_conhecidas() -> impl Iterator<Item = &'static str> {
    FERRAMENTAS_VER.into_iter().chain(FERRAMENTAS_MUDAR)
}

/// A permissão que vale para a conexão `conexao`: a dela, se não for `Herdar`; senão a da pasta
/// mais próxima que escolheu uma; na raiz, `Desligada`. Nunca devolve `Herdar`. O tipo sem a
/// linha "Assistentes (MCP)" (`campos::Campo::Mcp`, o link) fica sempre `Desligada`.
pub fn permissao_efetiva(tree: &Tree, conexao: u64) -> PermissaoIa {
    if let Some(c) = tree.connection(conexao) {
        if !crate::campos::Campo::Mcp.vale(c.kind) {
            return PermissaoIa::Desligada;
        }
        if !c.mcp.is_herdar() {
            return c.mcp;
        }
    }
    for pasta in tree.ancestors(conexao).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(pasta)
            && !g.mcp.is_herdar()
        {
            return g.mcp;
        }
    }
    PermissaoIa::Desligada
}

/// A permissão mais alta entre as conexões liberadas (Livre, depois Aprovar, depois Só ler), ou
/// `Desligada` se nenhuma está liberada. É a que vale para os painéis, que não são de uma
/// conexão só.
pub fn permissao_mais_alta(tree: &Tree) -> PermissaoIa {
    let ordem = |p: PermissaoIa| match p {
        PermissaoIa::Livre => 3,
        PermissaoIa::Aprovar => 2,
        PermissaoIa::SoLer => 1,
        PermissaoIa::Herdar | PermissaoIa::Desligada => 0,
    };
    tree.connections()
        .into_iter()
        .map(|c| permissao_efetiva(tree, c.id))
        .max_by_key(|&p| ordem(p))
        .filter(|&p| ordem(p) > 0)
        .unwrap_or(PermissaoIa::Desligada)
}

/// A permissão que a pasta `pasta` (ou a conexão, se for uma) herdaria de cima: a da pasta mais
/// próxima acima dela que escolheu uma, ou `Desligada`. É o que a opção "Herdar da pasta (…)"
/// mostra.
pub fn permissao_herdada(tree: &Tree, no: u64) -> PermissaoIa {
    for pasta in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(pasta)
            && !g.mcp.is_herdar()
        {
            return g.mcp;
        }
    }
    PermissaoIa::Desligada
}

/// A escolha de abrir abas novas que vale para a conexão `conexao`: a dela, a da pasta mais
/// próxima que escolheu uma ou, sem nenhuma, `padrao` (o `ia.mcp_abrir`).
pub fn abrir_aba_efetiva(tree: &Tree, conexao: u64, padrao: AbrirAbaIa) -> AbrirAbaIa {
    match tree.connection(conexao).and_then(|c| c.mcp_abrir) {
        Some(escolha) => escolha,
        None => abrir_aba_herdada(tree, conexao, padrao),
    }
}

/// A escolha de abrir abas que o nó `no` herdaria de cima: a da pasta mais próxima acima dele
/// que escolheu uma ou `padrao`. É o que a opção "Herdar (…)" mostra.
pub fn abrir_aba_herdada(tree: &Tree, no: u64, padrao: AbrirAbaIa) -> AbrirAbaIa {
    tree.ancestors(no)
        .into_iter()
        .rev()
        .find_map(|pasta| match tree.find(pasta) {
            Some(Node::Group(g)) => g.mcp_abrir,
            _ => None,
        })
        .unwrap_or(padrao)
}

/// A recusa de uma aba nova numa conexão em "Bloquear", para o assistente ler.
pub const ABRIR_ABA_BLOQUEADA: &str = "Esta conexão não deixa assistentes abrirem abas novas. \
     Peça à pessoa para abrir a aba e use a que ela abrir (listar_abas).";

/// A decisão de um pedido que abre uma aba nova: a da permissão (`decisao`) apertada pela
/// escolha `abrir`. Devolve também se o pedido pergunta sempre (sem valer o "resto desta
/// sessão" aprovado). Pura.
pub fn decidir_abertura(decisao: Decisao, abrir: AbrirAbaIa) -> (Decisao, bool) {
    match (decisao, abrir) {
        (recusa @ Decisao::Recusar(_), _) => (recusa, false),
        (_, AbrirAbaIa::Bloquear) => (Decisao::Recusar(ABRIR_ABA_BLOQUEADA.into()), false),
        (_, AbrirAbaIa::Perguntar) => (Decisao::Perguntar, true),
        (decisao, AbrirAbaIa::Permitir) => (decisao, false),
    }
}

/// A tabela da permissão: o que fazer com um pedido de `exigencia` numa conexão com `perm`,
/// sabendo se o comando `bloqueado` parece perigoso. Pura.
///
/// Um comando bloqueado nunca é permitido sem perguntar, nem em "Livre", nem com "Aprovar tudo
/// desta sessão".
pub fn decidir(perm: PermissaoIa, exigencia: Exigencia, bloqueado: bool) -> Decisao {
    match (perm, exigencia) {
        // Herdar não chega aqui (a efetiva nunca é Herdar); o lado seguro é recusar.
        (PermissaoIa::Herdar | PermissaoIa::Desligada, _) => {
            Decisao::Recusar("Esta conexão não está liberada para assistentes.".into())
        }
        (PermissaoIa::SoLer, Exigencia::Ver) => Decisao::Permitir,
        (PermissaoIa::SoLer, Exigencia::Mudar) => {
            Decisao::Recusar("Esta conexão está liberada só para leitura.".into())
        }
        (PermissaoIa::Aprovar | PermissaoIa::Livre, Exigencia::Ver) => Decisao::Permitir,
        (PermissaoIa::Aprovar, Exigencia::Mudar) => Decisao::Perguntar,
        (PermissaoIa::Livre, Exigencia::Mudar) if bloqueado => Decisao::Perguntar,
        (PermissaoIa::Livre, Exigencia::Mudar) => Decisao::Permitir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::{Connection, Kind};

    const NAO_LIBERADA: &str = "Esta conexão não está liberada para assistentes.";
    const SO_LEITURA: &str = "Esta conexão está liberada só para leitura.";

    #[test]
    fn a_tabela_inteira() {
        use Decisao::{Perguntar, Permitir};
        use Exigencia::{Mudar, Ver};
        let recusa = |m: &str| Decisao::Recusar(m.into());
        // (permissão, exigência, bloqueado, decisão)
        let casos = [
            (PermissaoIa::Desligada, Ver, false, recusa(NAO_LIBERADA)),
            (PermissaoIa::Desligada, Ver, true, recusa(NAO_LIBERADA)),
            (PermissaoIa::Desligada, Mudar, false, recusa(NAO_LIBERADA)),
            (PermissaoIa::Desligada, Mudar, true, recusa(NAO_LIBERADA)),
            (PermissaoIa::SoLer, Ver, false, Permitir),
            (PermissaoIa::SoLer, Ver, true, Permitir),
            (PermissaoIa::SoLer, Mudar, false, recusa(SO_LEITURA)),
            (PermissaoIa::SoLer, Mudar, true, recusa(SO_LEITURA)),
            (PermissaoIa::Aprovar, Ver, false, Permitir),
            (PermissaoIa::Aprovar, Ver, true, Permitir),
            (PermissaoIa::Aprovar, Mudar, false, Perguntar),
            (PermissaoIa::Aprovar, Mudar, true, Perguntar),
            (PermissaoIa::Livre, Ver, false, Permitir),
            (PermissaoIa::Livre, Ver, true, Permitir),
            (PermissaoIa::Livre, Mudar, false, Permitir),
            (PermissaoIa::Livre, Mudar, true, Perguntar),
        ];
        for (perm, exig, bloqueado, esperado) in casos {
            assert_eq!(
                decidir(perm, exig, bloqueado),
                esperado,
                "{perm:?} {exig:?} bloqueado={bloqueado}"
            );
        }
    }

    #[test]
    fn herdar_que_escapou_recusa() {
        assert_eq!(
            decidir(PermissaoIa::Herdar, Exigencia::Ver, false),
            Decisao::Recusar(NAO_LIBERADA.into())
        );
    }

    /// `Producao` (pasta) > `api` (conexão); `solta` fica na raiz.
    fn arvore() -> (Tree, u64, u64, u64) {
        let mut tree = Tree::default();
        let pasta = tree.add_group(None, "Produção");
        let api = tree.add_connection(Some(pasta), Connection::new("api", Kind::Ssh));
        let solta = tree.add_connection(None, Connection::new("solta", Kind::Ssh));
        (tree, pasta, api, solta)
    }

    fn grupo(tree: &mut Tree, id: u64, perm: PermissaoIa) {
        let Some(Node::Group(g)) = tree.find_mut(id) else {
            panic!("sem a pasta");
        };
        g.mcp = perm;
    }

    fn conexao(tree: &mut Tree, id: u64, perm: PermissaoIa) {
        let Some(Node::Connection(c)) = tree.find_mut(id) else {
            panic!("sem a conexão");
        };
        c.mcp = perm;
    }

    #[test]
    fn raiz_vale_desligada() {
        let (tree, _, api, solta) = arvore();
        assert_eq!(permissao_efetiva(&tree, solta), PermissaoIa::Desligada);
        assert_eq!(permissao_efetiva(&tree, api), PermissaoIa::Desligada);
        // Uma conexão que não existe também.
        assert_eq!(permissao_efetiva(&tree, 999), PermissaoIa::Desligada);
    }

    #[test]
    fn a_mais_alta_entre_as_liberadas() {
        let (mut tree, pasta, api, solta) = arvore();
        // Nenhuma liberada: desligada.
        assert_eq!(permissao_mais_alta(&tree), PermissaoIa::Desligada);
        conexao(&mut tree, solta, PermissaoIa::SoLer);
        assert_eq!(permissao_mais_alta(&tree), PermissaoIa::SoLer);
        conexao(&mut tree, api, PermissaoIa::Aprovar);
        assert_eq!(permissao_mais_alta(&tree), PermissaoIa::Aprovar);
        // Uma pasta que libera "Livre" vale para o que herda dela.
        conexao(&mut tree, api, PermissaoIa::Herdar);
        grupo(&mut tree, pasta, PermissaoIa::Livre);
        assert_eq!(permissao_mais_alta(&tree), PermissaoIa::Livre);
    }

    #[test]
    fn herda_da_pasta_mais_proxima() {
        let mut tree = Tree::default();
        let fora = tree.add_group(None, "Fora");
        let dentro = tree.add_group(Some(fora), "Dentro");
        let c = tree.add_connection(Some(dentro), Connection::new("c", Kind::Ssh));
        grupo(&mut tree, fora, PermissaoIa::Livre);
        assert_eq!(permissao_efetiva(&tree, c), PermissaoIa::Livre);
        // A pasta de dentro escolhe a sua e vence a de fora.
        grupo(&mut tree, dentro, PermissaoIa::SoLer);
        assert_eq!(permissao_efetiva(&tree, c), PermissaoIa::SoLer);
        // A conexão escolhe a dela e vence as duas.
        conexao(&mut tree, c, PermissaoIa::Aprovar);
        assert_eq!(permissao_efetiva(&tree, c), PermissaoIa::Aprovar);
        // O que a conexão herdaria, sem a escolha dela.
        assert_eq!(permissao_herdada(&tree, c), PermissaoIa::SoLer);
        assert_eq!(permissao_herdada(&tree, dentro), PermissaoIa::Livre);
        assert_eq!(permissao_herdada(&tree, fora), PermissaoIa::Desligada);
    }

    #[test]
    fn desligada_na_conexao_vence_a_pasta_livre() {
        let (mut tree, pasta, api, _) = arvore();
        grupo(&mut tree, pasta, PermissaoIa::Livre);
        conexao(&mut tree, api, PermissaoIa::Desligada);
        assert_eq!(permissao_efetiva(&tree, api), PermissaoIa::Desligada);
    }

    #[test]
    fn conexao_sem_campo_abre_como_herdar() {
        let antiga: Connection = serde_json::from_str(r#"{"name":"velha"}"#).unwrap();
        assert_eq!(antiga.mcp, PermissaoIa::Herdar);
        let pasta: crate::modelo::Group = serde_json::from_str(r#"{"name":"G"}"#).unwrap();
        assert_eq!(pasta.mcp, PermissaoIa::Herdar);
    }

    #[test]
    fn campo_herdar_nao_e_gravado_e_os_outros_voltam() {
        let mut c = Connection::new("web", Kind::Ssh);
        assert!(!serde_json::to_string(&c).unwrap().contains("mcp"));
        for (perm, texto) in [
            (PermissaoIa::Desligada, "desligada"),
            (PermissaoIa::SoLer, "so_ler"),
            (PermissaoIa::Aprovar, "aprovar"),
            (PermissaoIa::Livre, "livre"),
        ] {
            c.mcp = perm;
            let json = serde_json::to_string(&c).unwrap();
            assert!(json.contains(&format!(r#""mcp":"{texto}""#)), "{json}");
            let volta: Connection = serde_json::from_str(&json).unwrap();
            assert_eq!(volta.mcp, perm);
        }
        let mut tree = Tree::default();
        let g = tree.add_group(None, "G");
        assert!(!serde_json::to_string(&tree).unwrap().contains("mcp"));
        grupo(&mut tree, g, PermissaoIa::SoLer);
        assert!(
            serde_json::to_string(&tree)
                .unwrap()
                .contains(r#""mcp":"so_ler""#)
        );
    }

    #[test]
    fn abrir_aba_aperta_a_decisao() {
        use Decisao::{Perguntar, Permitir};
        let recusa = Decisao::Recusar(NAO_LIBERADA.into());
        // A recusa da permissão fica, seja qual for a escolha.
        for abrir in AbrirAbaIa::TODAS.iter().copied() {
            assert_eq!(
                decidir_abertura(recusa.clone(), abrir),
                (recusa.clone(), false)
            );
        }
        assert_eq!(
            decidir_abertura(Permitir, AbrirAbaIa::Permitir),
            (Permitir, false)
        );
        assert_eq!(
            decidir_abertura(Perguntar, AbrirAbaIa::Permitir),
            (Perguntar, false)
        );
        // Perguntar vale mesmo em Livre, e sempre.
        assert_eq!(
            decidir_abertura(Permitir, AbrirAbaIa::Perguntar),
            (Perguntar, true)
        );
        assert_eq!(
            decidir_abertura(Perguntar, AbrirAbaIa::Perguntar),
            (Perguntar, true)
        );
        for decisao in [Permitir, Perguntar] {
            let (bloqueada, sempre) = decidir_abertura(decisao, AbrirAbaIa::Bloquear);
            assert!(!sempre);
            assert!(
                matches!(&bloqueada, Decisao::Recusar(m) if m.contains("listar_abas")),
                "{bloqueada:?}"
            );
        }
    }

    #[test]
    fn abrir_aba_herda_da_pasta_e_das_configuracoes() {
        let mut tree = Tree::default();
        let fora = tree.add_group(None, "Fora");
        let dentro = tree.add_group(Some(fora), "Dentro");
        let c = tree.add_connection(Some(dentro), Connection::new("c", Kind::Ssh));
        let padrao = AbrirAbaIa::Perguntar;
        // Sem escolha em lugar nenhum: as Configurações.
        assert_eq!(abrir_aba_efetiva(&tree, c, padrao), AbrirAbaIa::Perguntar);
        assert_eq!(
            abrir_aba_efetiva(&tree, c, AbrirAbaIa::Permitir),
            AbrirAbaIa::Permitir
        );
        let mut escolher = |id: u64, abrir: Option<AbrirAbaIa>| match tree.find_mut(id) {
            Some(Node::Group(g)) => g.mcp_abrir = abrir,
            Some(Node::Connection(c)) => c.mcp_abrir = abrir,
            None => panic!("sem o nó"),
        };
        escolher(fora, Some(AbrirAbaIa::Bloquear));
        escolher(dentro, Some(AbrirAbaIa::Permitir));
        escolher(c, Some(AbrirAbaIa::Bloquear));
        // A conexão vence as pastas; a pasta de dentro vence a de fora.
        assert_eq!(abrir_aba_efetiva(&tree, c, padrao), AbrirAbaIa::Bloquear);
        assert_eq!(abrir_aba_herdada(&tree, c, padrao), AbrirAbaIa::Permitir);
        assert_eq!(
            abrir_aba_herdada(&tree, dentro, padrao),
            AbrirAbaIa::Bloquear
        );
        assert_eq!(
            abrir_aba_herdada(&tree, fora, padrao),
            AbrirAbaIa::Perguntar
        );
        // Uma conexão que não existe segue as Configurações.
        assert_eq!(abrir_aba_efetiva(&tree, 999, padrao), AbrirAbaIa::Perguntar);
    }

    #[test]
    fn abrir_aba_herdar_nao_e_gravado() {
        let mut c = Connection::new("web", Kind::Ssh);
        assert!(!serde_json::to_string(&c).unwrap().contains("mcp_abrir"));
        c.mcp_abrir = Some(AbrirAbaIa::Bloquear);
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains(r#""mcp_abrir":"bloquear""#), "{json}");
        let volta: Connection = serde_json::from_str(&json).unwrap();
        assert_eq!(volta.mcp_abrir, Some(AbrirAbaIa::Bloquear));
        let antiga: crate::modelo::Group = serde_json::from_str(r#"{"name":"G"}"#).unwrap();
        assert_eq!(antiga.mcp_abrir, None);
    }

    #[test]
    fn versao_continua_1() {
        assert_eq!(crate::modelo::VERSAO, 1);
    }

    #[test]
    fn exigencia_de_cada_ferramenta() {
        for nome in [
            "listar_conexoes",
            "listar_abas",
            "ler_tela",
            "esperar",
            "listar_arquivos",
            "baixar_arquivo",
            "listar_tuneis",
            "listar_portas",
            "ler_saida",
            "listar_espacos",
            "ler",
            "buscar",
            "glob",
        ] {
            assert_eq!(exigencia(nome), Exigencia::Ver, "{nome}");
        }
        for nome in [
            "abrir_conexao",
            "fechar_aba",
            "reconectar_aba",
            "digitar",
            "teclas",
            "executar",
            "enviar_arquivo",
            "ligar_tunel",
            "desligar_tunel",
            "encaminhar_porta",
            "parar_porta",
            "editar",
            "escrever",
            "bash",
        ] {
            assert_eq!(exigencia(nome), Exigencia::Mudar, "{nome}");
        }
        // Uma ferramenta que o app não conhece pede o mais forte.
        assert_eq!(exigencia("inventada"), Exigencia::Mudar);
        assert_eq!(ferramentas_conhecidas().count(), 27);
    }
}
