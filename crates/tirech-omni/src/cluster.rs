//! Modo cluster (M42): abrir várias conexões lado a lado numa janela nova, com o que se digita
//! indo para todas. Aqui ficam as funções puras (a grade, as linhas do diálogo e o filtro); a
//! tela está em `tela/cluster.rs`.

use crate::comandos::tem_terminal;
use crate::modelo::{Kind, Node, Tree};
use crate::t;

/// Até quantas conexões cabem como painéis; as que passam disso viram abas dos painéis.
pub const LIMITE_DA_GRADE: usize = 16;

/// Colunas e linhas da grade para `n` conexões: a mais quadrada que cabe, com no máximo 4 × 4.
pub fn grade_do_cluster(n: usize) -> (usize, usize) {
    if n <= 1 {
        return (1, 1);
    }
    if n > LIMITE_DA_GRADE {
        return (4, 4);
    }
    let colunas = (1..=4).find(|c| c * c >= n).unwrap_or(4);
    (colunas, n.div_ceil(colunas))
}

/// O texto "N conexões" do diálogo, com o aviso das abas quando passa do limite da grade.
pub fn contagem(n: usize) -> String {
    let base = t!("cluster-n-conexoes", n = n as i64);
    if n > LIMITE_DA_GRADE {
        t!(
            "cluster-viram-abas",
            base = base,
            limite = LIMITE_DA_GRADE as i64
        )
    } else {
        base
    }
}

/// Uma linha do diálogo: uma conexão de terminal da árvore.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linha {
    /// O id da conexão.
    pub id: u64,
    /// O nome na árvore.
    pub nome: String,
    /// O tipo, para o ícone.
    pub kind: Kind,
    /// O caminho dos grupos, separado por " › " (vazio na raiz).
    pub grupo: String,
    /// `usuario@host` (só o host sem usuário; vazio no shell local; o endereço no link).
    pub host: String,
}

impl Linha {
    /// Se o filtro (sem diferenciar maiúsculas) aparece no nome, no grupo ou no host. Filtro em
    /// branco combina com tudo.
    pub fn combina(&self, filtro: &str) -> bool {
        let busca = filtro.trim().to_lowercase();
        busca.is_empty()
            || [&self.nome, &self.grupo, &self.host]
                .iter()
                .any(|campo| campo.to_lowercase().contains(&busca))
    }
}

/// As conexões de terminal da árvore (SSH, Telnet e shell local; as só de arquivos ficam de
/// fora), na ordem de leitura.
pub fn linhas(arvore: &Tree) -> Vec<Linha> {
    linhas_onde(arvore, tem_terminal)
}

/// Os links da árvore (o lançador "Links…"), na ordem de leitura; o `host` é o endereço.
pub fn links(arvore: &Tree) -> Vec<Linha> {
    linhas_onde(arvore, |k| k == Kind::Link)
}

fn linhas_onde(arvore: &Tree, aceita: fn(Kind) -> bool) -> Vec<Linha> {
    let mut saida = Vec::new();
    juntar(&arvore.nodes, aceita, &mut Vec::new(), &mut saida);
    saida
}

fn juntar<'a>(
    nos: &'a [Node],
    aceita: fn(Kind) -> bool,
    caminho: &mut Vec<&'a str>,
    saida: &mut Vec<Linha>,
) {
    for no in nos {
        match no {
            Node::Group(g) => {
                caminho.push(&g.name);
                juntar(&g.children, aceita, caminho, saida);
                caminho.pop();
            }
            Node::Connection(c) if aceita(c.kind) => {
                let host = match (c.kind, c.user.is_empty()) {
                    (Kind::Local, _) => String::new(),
                    (Kind::Link, _) => c.url.clone(),
                    (_, true) => c.host.clone(),
                    (_, false) => format!("{}@{}", c.user, c.host),
                };
                saida.push(Linha {
                    id: c.id,
                    nome: c.name.clone(),
                    kind: c.kind,
                    grupo: caminho.join(" › "),
                    host,
                });
            }
            Node::Connection(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::Connection;

    #[test]
    fn a_grade_segue_a_tabela_do_plano() {
        let esperado = [
            (0, (1, 1)),
            (1, (1, 1)),
            (2, (2, 1)),
            (3, (2, 2)),
            (4, (2, 2)),
            (5, (3, 2)),
            (6, (3, 2)),
            (7, (3, 3)),
            (9, (3, 3)),
            (10, (4, 3)),
            (12, (4, 3)),
            (13, (4, 4)),
            (16, (4, 4)),
            (17, (4, 4)),
            (80, (4, 4)),
        ];
        for (n, grade) in esperado {
            assert_eq!(grade_do_cluster(n), grade, "para {n} conexões");
        }
    }

    #[test]
    fn a_contagem_avisa_das_abas_acima_de_16() {
        assert_eq!(contagem(0), "0 conexões");
        assert_eq!(contagem(1), "1 conexão");
        assert_eq!(contagem(16), "16 conexões");
        assert_eq!(
            contagem(17),
            "17 conexões (mais de 16 viram abas nos painéis)"
        );
    }

    fn arvore() -> Tree {
        let mut t = Tree::default();
        let prod = t.add_group(None, "Produção");
        let web = t.add_group(Some(prod), "Web");
        let mut a = Connection::new("web-01", Kind::Ssh);
        (a.host, a.user) = ("10.0.0.1".into(), "root".into());
        t.add_connection(Some(web), a);
        let mut b = Connection::new("roteador", Kind::Telnet);
        b.host = "10.0.0.254".into();
        t.add_connection(Some(prod), b);
        t.add_connection(None, Connection::new("shell", Kind::Local));
        t.add_connection(None, Connection::new("arquivos", Kind::Sftp));
        let mut intranet = Connection::new("intranet", Kind::Link);
        intranet.url = "https://intranet.cliente/".into();
        t.add_connection(Some(prod), intranet);
        t
    }

    #[test]
    fn as_linhas_trazem_o_caminho_o_host_e_deixam_os_arquivos_e_os_links_de_fora() {
        let l = linhas(&arvore());
        let nomes: Vec<_> = l.iter().map(|x| x.nome.as_str()).collect();
        assert_eq!(nomes, ["web-01", "roteador", "shell"]);
        assert_eq!(l[0].grupo, "Produção › Web");
        assert_eq!(l[0].host, "root@10.0.0.1");
        assert_eq!(l[1].grupo, "Produção");
        assert_eq!(l[1].host, "10.0.0.254");
        assert_eq!((l[2].grupo.as_str(), l[2].host.as_str()), ("", ""));
    }

    #[test]
    fn o_filtro_olha_nome_grupo_e_host_sem_diferenciar_maiusculas() {
        let l = linhas(&arvore());
        assert!(l[0].combina(""));
        assert!(l[0].combina("  "));
        assert!(l[0].combina("WEB"));
        assert!(l[0].combina("produção"));
        assert!(l[0].combina("10.0.0.1"));
        assert!(!l[0].combina("roteador"));
        assert!(l[2].combina("shell"));
        assert!(!l[2].combina("produção"));
    }

    #[test]
    fn os_links_trazem_o_endereco_no_lugar_do_host() {
        let l = links(&arvore());
        assert_eq!(l.len(), 1);
        assert_eq!(
            (l[0].nome.as_str(), l[0].grupo.as_str(), l[0].host.as_str()),
            ("intranet", "Produção", "https://intranet.cliente/")
        );
        assert!(l[0].combina("INTRANET.cliente"));
    }
}
