//! O que as exportações recebem: a árvore achatada, com a rede, o salto, as senhas e o
//! arquivo de chave de cada conexão já resolvidos. Montar o plano é o único lugar que olha o
//! cofre; os formatos só leem o plano.

use std::collections::HashMap;
use std::path::PathBuf;

use super::Opcoes;
use crate::cofre::{CAMPO_TOKEN, Cofre, Item};
use crate::modelo::{Connection, Jump, Kind, Node, Tree};
use crate::redes::{TipoProxy, rede_efetiva};

/// O proxy por onde a conexão sai (o do perfil de rede).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyPlano {
    /// SOCKS5 ou HTTP.
    pub tipo: TipoProxy,
    /// O servidor do proxy.
    pub host: String,
    /// A porta do proxy.
    pub port: u16,
    /// O usuário do proxy (vem do item do cofre).
    pub usuario: String,
    /// A senha do proxy, só com "incluir senhas".
    pub senha: Option<String>,
}

/// Um servidor de salto, com a senha quando a exportação leva senhas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaltoPlano {
    /// O salto.
    pub salto: Jump,
    /// A senha dele, só com "incluir senhas".
    pub senha: Option<String>,
}

/// Uma conexão pronta para exportar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemPlano {
    /// As pastas por onde ela passa, de fora para dentro.
    pub pasta: Vec<String>,
    /// A conexão, com `key_file` já trocado pelo arquivo da chave do cofre, se houver.
    pub conexao: Connection,
    /// A senha, só com "incluir senhas".
    pub senha: Option<String>,
    /// O token de sessão do S3, só com "incluir senhas".
    pub token_s3: Option<String>,
    /// O proxy da rede por onde ela sai.
    pub proxy: Option<ProxyPlano>,
    /// Os saltos, do primeiro ao último: o do perfil de rede e depois o da própria conexão.
    pub saltos: Vec<SaltoPlano>,
    /// Se a conexão usa uma chave do cofre (`chave_do_cofre`).
    pub usa_chave_do_cofre: bool,
}

impl ItemPlano {
    /// O salto que fica mais perto do servidor, o único que os formatos de um salto só levam.
    pub fn ultimo_salto(&self) -> Option<&SaltoPlano> {
        self.saltos.last()
    }
}

/// As pastas e as conexões, na ordem da árvore.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plano {
    /// Todas as pastas, também as vazias, de cima para baixo.
    pub pastas: Vec<Vec<String>>,
    /// As conexões.
    pub itens: Vec<ItemPlano>,
}

impl Plano {
    /// Achata a árvore. `cofre` é `None` quando ele está bloqueado ou não existe (sem senhas e
    /// sem notas). `chaves` leva o arquivo de cada chave do cofre já gravada (uid → caminho).
    pub fn montar(
        arvore: &Tree,
        cofre: Option<&Cofre>,
        opcoes: Opcoes,
        chaves: &HashMap<String, PathBuf>,
    ) -> Self {
        let mut plano = Plano::default();
        let contexto = Contexto {
            arvore,
            cofre,
            opcoes,
            chaves,
        };
        contexto.andar(&arvore.nodes, &mut Vec::new(), &mut plano);
        plano
    }
}

struct Contexto<'a> {
    arvore: &'a Tree,
    cofre: Option<&'a Cofre>,
    opcoes: Opcoes,
    chaves: &'a HashMap<String, PathBuf>,
}

impl Contexto<'_> {
    fn item(&self, id: Option<u64>) -> Option<&Item> {
        self.cofre.zip(id).and_then(|(cofre, id)| cofre.item(id))
    }

    /// A senha de um item, se a exportação leva senhas e ela não é vazia.
    fn senha(&self, item: Option<&Item>) -> Option<String> {
        item.filter(|i| self.opcoes.senhas && !i.senha.is_empty())
            .map(|i| i.senha.clone())
    }

    fn andar(&self, nos: &[Node], caminho: &mut Vec<String>, plano: &mut Plano) {
        for no in nos {
            match no {
                Node::Group(g) => {
                    caminho.push(g.name.clone());
                    plano.pastas.push(caminho.clone());
                    self.andar(&g.children, caminho, plano);
                    caminho.pop();
                }
                Node::Connection(c) => plano.itens.push(self.conexao(c, caminho)),
            }
        }
    }

    fn conexao(&self, c: &Connection, caminho: &[String]) -> ItemPlano {
        let mut conexao = c.clone();
        let credencial = self.item(c.credencial);
        if conexao.user.is_empty()
            && let Some(item) = credencial
        {
            conexao.user.clone_from(&item.usuario);
        }
        // A chave do cofre vale mais que o arquivo; sem ela gravada, fica o arquivo que houver.
        if let Some(arquivo) = c
            .chave_do_cofre
            .as_ref()
            .and_then(|uid| self.chaves.get(uid))
        {
            conexao.key_file = Some(arquivo.clone());
        }
        let token_s3 = credencial
            .filter(|_| self.opcoes.senhas && c.kind == Kind::S3)
            .and_then(|i| i.campos.iter().find(|campo| campo.nome == CAMPO_TOKEN))
            .map(|campo| campo.valor.clone())
            .filter(|t| !t.is_empty());
        let perfil = rede_efetiva(self.arvore, c.id);
        let proxy = perfil.and_then(|p| p.proxy.as_ref()).map(|proxy| {
            let item = self.item(proxy.credencial);
            ProxyPlano {
                tipo: proxy.tipo,
                host: proxy.host.clone(),
                port: proxy.port,
                usuario: item.map(|i| i.usuario.clone()).unwrap_or_default(),
                senha: self.senha(item),
            }
        });
        let saltos = perfil
            .and_then(|p| p.salto.as_ref())
            .into_iter()
            .chain(c.jump.as_ref())
            .map(|salto| SaltoPlano {
                salto: salto.clone(),
                senha: self.senha(self.item(salto.credencial)),
            })
            .collect();
        ItemPlano {
            pasta: caminho.to_vec(),
            usa_chave_do_cofre: c.chave_do_cofre.is_some(),
            senha: self.senha(credencial),
            token_s3,
            proxy,
            saltos,
            conexao,
        }
    }
}

/// Um nó da árvore que os formatos com pastas aninhadas (FileZilla, WinSSHTerm, mRemoteNG)
/// escrevem.
#[derive(Debug)]
pub enum NoPlano<'a> {
    /// Uma pasta e o que tem dentro.
    Pasta(PastaPlano<'a>),
    /// Uma conexão.
    Item(&'a ItemPlano),
}

/// Uma pasta da árvore dos formatos.
#[derive(Debug)]
pub struct PastaPlano<'a> {
    /// O nome.
    pub nome: String,
    /// As pastas de dentro e as conexões, as pastas primeiro.
    pub filhos: Vec<NoPlano<'a>>,
}

fn descer<'a, 'b>(nos: &'b mut Vec<NoPlano<'a>>, caminho: &[String]) -> &'b mut Vec<NoPlano<'a>> {
    let Some((nome, resto)) = caminho.split_first() else {
        return nos;
    };
    let posicao = nos
        .iter()
        .position(|n| matches!(n, NoPlano::Pasta(p) if &p.nome == nome))
        .unwrap_or_else(|| {
            // As pastas ficam antes das conexões.
            let lugar = nos
                .iter()
                .position(|n| matches!(n, NoPlano::Item(_)))
                .unwrap_or(nos.len());
            nos.insert(
                lugar,
                NoPlano::Pasta(PastaPlano {
                    nome: nome.clone(),
                    filhos: Vec::new(),
                }),
            );
            lugar
        });
    match &mut nos[posicao] {
        NoPlano::Pasta(p) => descer(&mut p.filhos, resto),
        NoPlano::Item(_) => unreachable!("a posição é de uma pasta"),
    }
}

impl Plano {
    /// A árvore aninhada com as pastas do plano (também as vazias) e só as conexões `itens`
    /// (as que o formato leva).
    pub fn aninhar<'a>(&'a self, itens: &[&'a ItemPlano]) -> Vec<NoPlano<'a>> {
        let mut raiz = Vec::new();
        for pasta in &self.pastas {
            descer(&mut raiz, pasta);
        }
        for item in itens {
            descer(&mut raiz, &item.pasta).push(NoPlano::Item(item));
        }
        raiz
    }
}
