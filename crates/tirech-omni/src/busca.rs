//! A busca global da barra de título: acha conexões da árvore e itens do cofre pelo que se
//! digita. Só a lógica; o campo (`AutoSuggestBox`) fica na tela.

use crate::cofre::{Item, Tipo};
use crate::modelo::Connection;
use crate::t;

/// Quantas sugestões a lista mostra.
pub const MAXIMO: usize = 8;

/// O que uma sugestão abre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destino {
    /// A conexão de id dado: abre uma aba.
    Conexao(u64),
    /// O item do cofre de id dado: mostra a página Cofre com ele escolhido.
    Cofre(u64),
    /// A configuração de chave dada, do [`catalogo`](crate::configuracoes::catalogo): abre a
    /// página Configurações já filtrada por ela.
    Configuracao(&'static str),
}

/// O texto em minúsculas e sem acentos, para comparar sem diferenciá-los.
pub fn normalizar(texto: &str) -> String {
    texto
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            outro => outro,
        })
        .collect()
}

/// Acrescenta `[id]` ao rótulo que já apareceu antes, para a lista não repetir texto.
fn unico(rotulo: String, id: u64, vistos: &[(String, Destino)]) -> String {
    if vistos.iter().any(|(r, _)| *r == rotulo) {
        format!("{rotulo} [{id}]")
    } else {
        rotulo
    }
}

/// As sugestões para `texto`: até [`MAXIMO`], primeiro as conexões cujo nome ou servidor
/// ([`Connection::servidor`]: só os campos que valem para o tipo) contêm o texto
/// (`"{nome} — {servidor}"`) e depois os itens do cofre pelo título ou
/// pelo caminho da pasta (`"{titulo} — cofre"` na raiz, `"{titulo} — cofre/{caminho}"` numa
/// pasta). `itens` traz cada item com o caminho da pasta dele (vazio na raiz) e é `None` com o
/// cofre bloqueado. Texto vazio não sugere nada.
pub fn sugerir(
    texto: &str,
    conexoes: &[&Connection],
    itens: Option<&[(Item, String)]>,
) -> Vec<(String, Destino)> {
    let procurado = normalizar(texto.trim());
    if procurado.is_empty() {
        return Vec::new();
    }
    let mut lista: Vec<(String, Destino)> = Vec::new();
    for c in conexoes {
        if lista.len() >= MAXIMO {
            break;
        }
        let servidor = c.servidor();
        let casa = [c.name.as_str(), servidor.as_str()]
            .iter()
            .any(|campo| normalizar(campo).contains(&procurado));
        if !casa {
            continue;
        }
        let rotulo = match servidor {
            s if s.is_empty() => c.name.clone(),
            s => format!("{} — {s}", c.name),
        };
        let rotulo = unico(rotulo, c.id, &lista);
        lista.push((rotulo, Destino::Conexao(c.id)));
    }
    for (item, caminho) in itens.unwrap_or_default() {
        if lista.len() >= MAXIMO {
            break;
        }
        if normalizar(&item.titulo).contains(&procurado) || normalizar(caminho).contains(&procurado)
        {
            let base = if item.tipo == Tipo::Nota {
                "notas"
            } else {
                "cofre"
            };
            let onde = match caminho.as_str() {
                "" => base.to_owned(),
                caminho => format!("{base}/{caminho}"),
            };
            let rotulo = unico(format!("{} — {onde}", item.titulo), item.id, &lista);
            lista.push((rotulo, Destino::Cofre(item.id)));
        }
    }
    lista
}

/// Quantas configurações a busca global acrescenta no fim da lista.
const MAXIMO_CONFIGURACOES: usize = 3;

/// As configurações que casam com `texto` (`"{titulo} — configuração"`), para ir no fim da lista
/// de [`sugerir`]. Texto vazio não sugere nada.
pub fn sugerir_configuracoes(texto: &str) -> Vec<(String, Destino)> {
    if texto.trim().is_empty() {
        return Vec::new();
    }
    crate::configuracoes::catalogo::buscar(texto)
        .into_iter()
        .take(MAXIMO_CONFIGURACOES)
        .map(|d| {
            (
                t!("busca-configuracao", titulo = d.titulo()),
                Destino::Configuracao(d.chave),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::Kind;

    fn conexao(id: u64, nome: &str, host: &str, usuario: &str) -> Connection {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.id = id;
        c.host = host.into();
        c.user = usuario.into();
        c
    }

    /// Um item na pasta de `caminho` (vazio: na raiz), como a busca o recebe.
    fn item(id: u64, titulo: &str, caminho: &str) -> (Item, String) {
        (
            Item {
                id,
                titulo: titulo.into(),
                ..Item::default()
            },
            caminho.to_owned(),
        )
    }

    #[test]
    fn normalizar_tira_acentos_e_maiusculas() {
        assert_eq!(normalizar("Conexão ÁGUA"), "conexao agua");
    }

    #[test]
    fn acha_pelo_nome_pelo_host_e_pelo_usuario() {
        let web = conexao(1, "Servidor Web", "10.0.0.5", "ana");
        let banco = conexao(2, "Banco", "db.exemplo.com", "root");
        let todas = [&web, &banco];
        let nomes = |texto: &str| {
            sugerir(texto, &todas, None)
                .into_iter()
                .map(|(_, d)| d)
                .collect::<Vec<_>>()
        };
        assert_eq!(nomes("servidor"), [Destino::Conexao(1)]);
        assert_eq!(nomes("10.0.0"), [Destino::Conexao(1)]);
        assert_eq!(nomes("ROOT"), [Destino::Conexao(2)]);
        assert_eq!(nomes("exemplo"), [Destino::Conexao(2)]);
        assert!(nomes("nada").is_empty());
        assert!(nomes("   ").is_empty());
    }

    #[test]
    fn ignora_acentos_nos_dois_lados() {
        let c = conexao(1, "Produção", "h", "u");
        let todas = [&c];
        assert_eq!(sugerir("producao", &todas, None).len(), 1);
        assert_eq!(sugerir("PRODUÇÃO", &todas, None).len(), 1);
    }

    #[test]
    fn o_rotulo_leva_o_servidor() {
        let c = conexao(1, "Web", "web.exemplo", "ana");
        let sem_usuario = conexao(2, "Web 2", "web2.exemplo", "");
        let local = Connection {
            id: 3,
            ..Connection::new("Web local", Kind::Local)
        };
        let todas = [&c, &sem_usuario, &local];
        let rotulos: Vec<String> = sugerir("web", &todas, None)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert_eq!(
            rotulos,
            ["Web — ana@web.exemplo", "Web 2 — web2.exemplo", "Web local"]
        );
    }

    #[test]
    fn a_busca_so_usa_o_que_vale_para_o_tipo() {
        // O Access Key ID do S3 não aparece nem casa.
        let mut s3 = Connection::new("nuvem", Kind::S3);
        s3.id = 1;
        s3.user = "AKIAEXEMPLO".into();
        s3.s3.endpoint = "https://minio.exemplo".into();
        s3.s3.bucket = "bkt".into();
        // Um shell local que já foi SSH não casa pelo host antigo.
        let mut local = Connection::new("Shell", Kind::Local);
        local.id = 2;
        local.host = "antigo.exemplo".into();
        local.user = "ana".into();
        let ssh = conexao(3, "Web", "web.exemplo", "ana");
        let todas = [&s3, &local, &ssh];
        assert!(sugerir("akia", &todas, None).is_empty());
        assert!(sugerir("antigo", &todas, None).is_empty());
        assert!(sugerir("ana", &todas, None).len() == 1);
        let rotulos: Vec<String> = sugerir("nuvem", &todas, None)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert_eq!(rotulos, ["nuvem — bkt@minio.exemplo"]);
        let rotulos: Vec<String> = sugerir("bkt", &todas, None)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert_eq!(rotulos, ["nuvem — bkt@minio.exemplo"]);
        let rotulos: Vec<String> = sugerir("web", &todas, None)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        assert_eq!(rotulos, ["Web — ana@web.exemplo"]);
    }

    #[test]
    fn os_itens_do_cofre_vem_depois_das_conexoes() {
        let c = conexao(1, "Banco", "h", "u");
        let itens = [
            item(7, "Banco de testes", ""),
            item(8, "Outro", ""),
            item(9, "Dump", "Clientes/Banco do Brasil"),
        ];
        let lista = sugerir("banco", &[&c], Some(&itens));
        assert_eq!(
            lista,
            [
                ("Banco — u@h".to_owned(), Destino::Conexao(1)),
                ("Banco de testes — cofre".to_owned(), Destino::Cofre(7)),
                (
                    "Dump — cofre/Clientes/Banco do Brasil".to_owned(),
                    Destino::Cofre(9)
                ),
            ]
        );
        assert_eq!(sugerir("banco", &[&c], None).len(), 1, "cofre bloqueado");
    }

    #[test]
    fn o_item_tambem_casa_pelo_caminho_sem_acento() {
        let itens = [item(1, "Painel", "Clientes/Ação"), item(2, "Outro", "")];
        let lista = sugerir("acao", &[], Some(&itens));
        assert_eq!(
            lista,
            [("Painel — cofre/Clientes/Ação".to_owned(), Destino::Cofre(1))]
        );
    }

    #[test]
    fn mostra_no_maximo_oito_e_desfaz_repeticoes() {
        let todas: Vec<Connection> = (1..=12).map(|n| conexao(n, "Igual", "h", "u")).collect();
        let refs: Vec<&Connection> = todas.iter().collect();
        let lista = sugerir("igual", &refs, None);
        assert_eq!(lista.len(), MAXIMO);
        let mut rotulos: Vec<&String> = lista.iter().map(|(r, _)| r).collect();
        rotulos.sort();
        rotulos.dedup();
        assert_eq!(rotulos.len(), MAXIMO, "nenhum rótulo se repete");
    }
}
