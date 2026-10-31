//! Grava uma [`Importacao`] na árvore e no cofre: o grupo de destino, os subgrupos, as
//! duplicatas, os perfis de rede, as senhas e o bloco `cofre` do JSON nativo.
//!
//! É lógica pura sobre uma [`Tree`] e um [`Cofre`] já abertos (a tela passa os dois dentro de
//! `update`/`mudar`), por isso se testa sem tela.

use crate::cofre::{CAMPO_TOKEN, Campo, Cofre, Item, Tipo};
use crate::importar::{Entrada, Importacao, PerfilTrazido, Trazida};
use crate::modelo::{Connection, Kind, Node, Tree};
use crate::redes::{PerfilRede, Rede, nome_livre};
use crate::t;

/// O que fazer com uma conexão que já existe no grupo de destino (mesmo nome, endereço, usuário
/// e tipo).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Duplicadas {
    /// Deixa a que está lá.
    #[default]
    Pular,
    /// Troca os dados da que está lá pelos do arquivo.
    Substituir,
    /// Traz a do arquivo com um número no nome (`ACME (2)`).
    ManterAsDuas,
}

/// As escolhas da pessoa no diálogo.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opcoes {
    /// O nome do grupo que recebe tudo; vazio põe na raiz da árvore.
    pub destino: String,
    /// Se as senhas vão para o cofre (precisa dele aberto).
    pub senhas: bool,
    /// O que fazer com as que já existem.
    pub duplicadas: Duplicadas,
    /// Os ids dos provedores de IA desta instalação (M52): o `perfil_ia` de uma conexão trazida
    /// que não está aqui volta a usar o padrão.
    pub perfis_de_ia: Vec<u64>,
}

/// O que a gravação fez.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Aplicado {
    /// Conexões novas.
    pub importadas: usize,
    /// Conexões que já existiam e ficaram como estavam.
    pub puladas: usize,
    /// Conexões que já existiam e foram trocadas.
    pub substituidas: usize,
    /// Itens do cofre criados ou mudados pelas senhas.
    pub senhas: usize,
    /// O grupo de destino, se houve (para a tela abrir).
    pub grupo: Option<u64>,
    /// O que ficou de fora e a pessoa precisa saber.
    pub avisos: Vec<String>,
}

impl Aplicado {
    /// "Importei 10 conexões, 2 puladas." para a barra de aviso.
    pub fn resumo(&self) -> String {
        let mut partes = vec![t!("importar-importei", n = self.importadas as i64)];
        if self.substituidas > 0 {
            partes.push(t!("importar-trocadas", n = self.substituidas as i64));
        }
        if self.puladas > 0 {
            partes.push(t!("importar-puladas", n = self.puladas as i64));
        }
        let mut texto = partes.join(", ");
        texto.push('.');
        texto
    }
}

fn filhos(arvore: &Tree, pai: Option<u64>) -> &[Node] {
    match pai.and_then(|id| arvore.find(id)) {
        Some(Node::Group(g)) => &g.children,
        _ => &arvore.nodes,
    }
}

/// O grupo `nome` dentro de `pai`: o que já existe ou um novo. O segundo valor diz se é novo.
fn garantir_grupo(arvore: &mut Tree, pai: Option<u64>, nome: &str) -> (u64, bool) {
    let existente = filhos(arvore, pai).iter().find_map(|n| match n {
        Node::Group(g) if g.name == nome => Some(g.id),
        _ => None,
    });
    match existente {
        Some(id) => (id, false),
        None => (arvore.add_group(pai, nome), true),
    }
}

fn endereco(c: &Connection) -> &str {
    if c.kind == Kind::S3 {
        &c.s3.endpoint
    } else {
        &c.host
    }
}

/// O id da conexão de `pai` que é a mesma que `nova` (nome, endereço, usuário e tipo).
fn igual_a(arvore: &Tree, pai: Option<u64>, nova: &Connection) -> Option<u64> {
    filhos(arvore, pai).iter().find_map(|n| match n {
        Node::Connection(c)
            if c.name == nova.name
                && c.kind == nova.kind
                && c.user == nova.user
                && endereco(c) == endereco(nova) =>
        {
            Some(c.id)
        }
        _ => None,
    })
}

/// `nome (2)`, `nome (3)`… o primeiro que nenhuma conexão de `pai` usa.
fn nome_com_numero(arvore: &Tree, pai: Option<u64>, nome: &str) -> String {
    let usado = |candidato: &str| {
        filhos(arvore, pai)
            .iter()
            .any(|n| matches!(n, Node::Connection(c) if c.name == candidato))
    };
    (2..)
        .map(|n| format!("{nome} ({n})"))
        .find(|candidato| !usado(candidato))
        .unwrap_or_else(|| nome.to_owned())
}

/// O cofre aberto, se houver, e quantos itens as senhas criaram.
struct Guarda<'a> {
    cofre: Option<&'a mut Cofre>,
    itens: usize,
    avisos: Vec<String>,
    senhas_de_fora: usize,
}

impl Guarda<'_> {
    /// Cria o item (na pasta "Conexões" do cofre) e devolve o id.
    fn criar(&mut self, mut item: Item) -> Option<u64> {
        let cofre = self.cofre.as_deref_mut()?;
        item.pasta = cofre.garantir_caminho(None, &["Conexões"]);
        match cofre.salvar(item) {
            Ok(id) => {
                self.itens += 1;
                Some(id)
            }
            Err(erro) => {
                self.avisos.push(t!(
                    "comum-nao-consegui-gravar-o-cofre",
                    erro = erro.to_string()
                ));
                None
            }
        }
    }

    /// Muda a senha de um item que já existe.
    fn trocar_senha(&mut self, id: u64, senha: &str, token: Option<&str>) -> bool {
        let Some(cofre) = self.cofre.as_deref_mut() else {
            return false;
        };
        let Some(mut item) = cofre.item(id).cloned() else {
            return false;
        };
        item.senha = senha.to_owned();
        if let Some(token) = token {
            item.campos.retain(|c| c.nome != CAMPO_TOKEN);
            item.campos.push(Campo {
                nome: CAMPO_TOKEN.to_owned(),
                valor: token.to_owned(),
                oculto: true,
            });
        }
        match cofre.salvar(item) {
            Ok(_) => {
                self.itens += 1;
                true
            }
            Err(erro) => {
                self.avisos.push(t!(
                    "comum-nao-consegui-gravar-o-cofre",
                    erro = erro.to_string()
                ));
                false
            }
        }
    }
}

/// O item do cofre de uma senha de servidor, proxy ou salto.
fn item_de_senha(tipo: Tipo, usuario: &str, endereco: &str, url: String, senha: &str) -> Item {
    Item {
        tipo,
        titulo: if usuario.is_empty() {
            endereco.to_owned()
        } else {
            format!("{usuario}@{endereco}")
        },
        usuario: usuario.to_owned(),
        url,
        senha: senha.to_owned(),
        ..Item::default()
    }
}

/// Acha ou cria o perfil de rede de uma conexão ou pasta: o que já tem o mesmo nome e o mesmo
/// proxy é reaproveitado.
fn perfil_para(arvore: &mut Tree, guarda: &mut Guarda, trazido: &PerfilTrazido) -> u64 {
    let mesmo = |p: &PerfilRede| {
        p.name == trazido.perfil.name
            && p.proxy.as_ref().map(|x| (x.tipo, &x.host, x.port))
                == trazido
                    .perfil
                    .proxy
                    .as_ref()
                    .map(|x| (x.tipo, &x.host, x.port))
    };
    if let Some(existente) = arvore.redes.iter().find(|p| mesmo(p)) {
        return existente.id;
    }
    let mut perfil = trazido.perfil.clone();
    perfil.name = nome_livre(arvore, &perfil.name, None);
    if let Some(proxy) = perfil.proxy.as_mut()
        && proxy.credencial.is_none()
        && (!trazido.usuario.is_empty() || trazido.senha.is_some())
    {
        match trazido.senha.as_deref() {
            Some(senha) => {
                let item = item_de_senha(
                    Tipo::Ssh,
                    &trazido.usuario,
                    &proxy.endereco(),
                    proxy.endereco(),
                    senha,
                );
                proxy.credencial = guarda.criar(item);
            }
            None if guarda.cofre.is_some() => {
                // Só o usuário: o cofre guarda o item sem senha.
                let item = item_de_senha(
                    Tipo::Ssh,
                    &trazido.usuario,
                    &proxy.endereco(),
                    proxy.endereco(),
                    "",
                );
                proxy.credencial = guarda.criar(item);
            }
            None => {}
        }
    }
    arvore.add_perfil(perfil)
}

/// Grava as senhas e o token de uma conexão que acabou de entrar na árvore. Sem o cofre (bloqueado),
/// só conta as que ficaram de fora, para o aviso.
fn guardar_do_item(
    arvore: &mut Tree,
    guarda: &mut Guarda,
    id: u64,
    trazida: &Trazida,
    senhas: bool,
) {
    let Some(c) = arvore.connection(id).cloned() else {
        return;
    };
    let mut nova = c.clone();
    if senhas {
        if let Some(senha) = trazida
            .senha
            .as_deref()
            .or(trazida.token_s3.as_deref().map(|_| ""))
        {
            if guarda.cofre.is_none() {
                guarda.senhas_de_fora += 1;
            } else if let Some(existente) = c.credencial.filter(|i| {
                guarda
                    .cofre
                    .as_deref()
                    .is_some_and(|x| x.item(*i).is_some())
            }) {
                guarda.trocar_senha(existente, senha, trazida.token_s3.as_deref());
            } else {
                let s3 = c.kind == Kind::S3;
                let mut item = item_de_senha(
                    if s3 { Tipo::S3 } else { Tipo::Ssh },
                    &c.user,
                    endereco(&c),
                    if s3 {
                        c.s3.endpoint.clone()
                    } else {
                        format!("{}:{}", c.host, c.port)
                    },
                    senha,
                );
                if let Some(token) = trazida.token_s3.as_deref() {
                    item.campos.push(Campo {
                        nome: CAMPO_TOKEN.to_owned(),
                        valor: token.to_owned(),
                        oculto: true,
                    });
                }
                nova.credencial = guarda.criar(item);
            }
        }
        if let (Some(senha), Some(salto)) = (trazida.senha_salto.as_deref(), c.jump.as_ref()) {
            if guarda.cofre.is_none() {
                guarda.senhas_de_fora += 1;
            } else if let Some(existente) = salto.credencial.filter(|i| {
                guarda
                    .cofre
                    .as_deref()
                    .is_some_and(|x| x.item(*i).is_some())
            }) {
                guarda.trocar_senha(existente, senha, None);
            } else {
                let item = item_de_senha(
                    Tipo::Ssh,
                    &salto.user,
                    &salto.host,
                    format!("{}:{}", salto.host, salto.port),
                    senha,
                );
                let id_item = guarda.criar(item);
                if let Some(j) = nova.jump.as_mut() {
                    j.credencial = id_item;
                }
            }
        }
    }
    if nova != c
        && let Some(destino) = arvore.connection_mut(id)
    {
        *destino = nova;
    }
}

/// Grava a importação. `cofre` é `None` com o cofre bloqueado (ou ausente): as senhas ficam de
/// fora e a conexão pede a senha ao conectar. Com `Importacao::cofre` (JSON nativo) e as senhas
/// ligadas, o bloco entra no cofre e as ligações dele são religadas; sem elas, as ligações são
/// soltas.
pub fn aplicar(
    arvore: &mut Tree,
    cofre: Option<&mut Cofre>,
    mut importacao: Importacao,
    opcoes: &Opcoes,
) -> Aplicado {
    let mut aplicado = Aplicado::default();
    let senhas = opcoes.senhas && cofre.is_some();
    let nome_destino = opcoes.destino.trim();
    let mut guarda = Guarda {
        cofre,
        itens: 0,
        avisos: Vec::new(),
        senhas_de_fora: 0,
    };

    // O bloco do cofre do JSON nativo: entra inteiro ou as ligações dele se soltam.
    let lote = importacao.cofre.take();
    let mapa = match (lote, senhas) {
        (Some(lote), true) => {
            let total = lote.itens.len();
            let cofre = guarda.cofre.as_deref_mut().expect("senhas pedem o cofre");
            let pasta = (!nome_destino.is_empty())
                .then(|| cofre.garantir_caminho(None, &[nome_destino]))
                .flatten();
            match cofre.importar(lote, pasta) {
                Ok(mapa) => {
                    guarda.itens += total;
                    mapa
                }
                Err(erro) => {
                    guarda.avisos.push(t!(
                        "comum-nao-consegui-gravar-o-cofre",
                        erro = erro.to_string()
                    ));
                    Default::default()
                }
            }
        }
        _ => Default::default(),
    };
    importacao.religar(&mapa);
    if mapa.itens.is_empty() {
        for t in importacao.conexoes_mut() {
            t.conexao.chave_do_cofre = None;
        }
        for entrada in &mut importacao.entradas {
            if let Entrada::Pasta(p) = entrada {
                p.chave_do_cofre = None;
            }
        }
    }

    let raiz = (!nome_destino.is_empty()).then(|| garantir_grupo(arvore, None, nome_destino).0);
    aplicado.grupo = raiz;

    for entrada in importacao.entradas {
        match entrada {
            Entrada::Pasta(p) => {
                let (grupo, novo) = descer(arvore, raiz, &p.caminho);
                if novo {
                    let rede = match p.perfil.as_ref() {
                        Some(perfil) => Rede::Perfil(perfil_para(arvore, &mut guarda, perfil)),
                        None => p.rede,
                    };
                    if let Some(Node::Group(g)) = grupo.and_then(|id| arvore.find_mut(id)) {
                        g.rede = rede;
                        g.comandos = p.comandos;
                        g.persistente = p.persistente;
                        g.mouse = p.mouse;
                        g.retentar = p.retentar;
                        g.ocioso = p.ocioso;
                        g.credencial_modo = p.credencial_modo;
                        g.key_file = p.key_file;
                        g.chave_do_cofre = p.chave_do_cofre;
                    }
                }
            }
            Entrada::Conexao(trazida) => {
                let mut trazida = *trazida;
                let (pai, _) = descer(arvore, raiz, &trazida.pasta);
                let pai = pai.or(raiz);
                trazida.conexao.perfil_ia = trazida
                    .conexao
                    .perfil_ia
                    .filter(|id| opcoes.perfis_de_ia.contains(id));
                if let Some(perfil) = trazida.perfil.as_ref() {
                    trazida.conexao.rede = Rede::Perfil(perfil_para(arvore, &mut guarda, perfil));
                }
                let id = match igual_a(arvore, pai, &trazida.conexao) {
                    Some(existente) => match opcoes.duplicadas {
                        Duplicadas::Pular => {
                            aplicado.puladas += 1;
                            continue;
                        }
                        Duplicadas::Substituir => {
                            if let Some(c) = arvore.connection_mut(existente) {
                                let mut nova = trazida.conexao.clone();
                                nova.id = existente;
                                nova.credencial = nova.credencial.or(c.credencial);
                                // Os formatos de fora não trazem o modo: o da conexão fica.
                                if nova.credencial_modo.is_herdar() {
                                    nova.credencial_modo = c.credencial_modo;
                                }
                                // O formato sem descrição não apaga a que a conexão tinha.
                                if nova.descricao.trim().is_empty() {
                                    nova.descricao = c.descricao.clone();
                                }
                                nova.nota = nova.nota.or(c.nota);
                                *c = nova;
                            }
                            aplicado.substituidas += 1;
                            existente
                        }
                        Duplicadas::ManterAsDuas => {
                            let mut nova = trazida.conexao.clone();
                            nova.name = nome_com_numero(arvore, pai, &nova.name);
                            aplicado.importadas += 1;
                            arvore.add_connection(pai, nova)
                        }
                    },
                    None => {
                        aplicado.importadas += 1;
                        arvore.add_connection(pai, trazida.conexao.clone())
                    }
                };
                guardar_do_item(arvore, &mut guarda, id, &trazida, opcoes.senhas);
            }
        }
    }

    aplicado.senhas = guarda.itens;
    aplicado.avisos = importacao.avisos;
    aplicado.avisos.append(&mut guarda.avisos);
    if guarda.senhas_de_fora > 0 {
        aplicado.avisos.push(t!(
            "importar-cofre-bloqueado-senhas",
            n = guarda.senhas_de_fora as i64
        ));
    }
    aplicado
}

/// Desce por `caminho` a partir de `raiz`, criando os grupos que faltam. Devolve o último e se
/// ele é novo; um caminho vazio é a própria `raiz`.
fn descer(arvore: &mut Tree, raiz: Option<u64>, caminho: &[String]) -> (Option<u64>, bool) {
    let mut atual = raiz;
    let mut novo = false;
    for nome in caminho {
        let (id, criado) = garantir_grupo(arvore, atual, nome);
        atual = Some(id);
        novo = criado;
    }
    (atual, novo && !caminho.is_empty())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::cofre::Lote;
    use crate::importar::PastaTrazida;
    use crate::modelo::{Jump, ModoCredencial};
    use crate::redes::{ProxyRede, TipoProxy};

    fn ssh(nome: &str, host: &str, pasta: &[&str]) -> Trazida {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.host = host.to_owned();
        c.user = "root".to_owned();
        Trazida::nova(c, pasta.iter().map(|p| (*p).to_owned()).collect())
    }

    fn com_senha(mut t: Trazida, senha: &str) -> Trazida {
        t.senha = Some(senha.to_owned());
        t
    }

    fn importacao(conexoes: Vec<Trazida>) -> Importacao {
        let mut i = Importacao::default();
        for c in conexoes {
            i.conexao(c);
        }
        i
    }

    fn opcoes(destino: &str, senhas: bool, duplicadas: Duplicadas) -> Opcoes {
        Opcoes {
            destino: destino.to_owned(),
            senhas,
            duplicadas,
            perfis_de_ia: Vec::new(),
        }
    }

    fn cofre(nome: &str) -> (Cofre, PastaTemporaria) {
        let pasta = PastaTemporaria::nova(nome);
        let (cofre, _) = Cofre::abrir(&pasta.0);
        (cofre, pasta)
    }

    fn nomes(arvore: &Tree) -> Vec<String> {
        arvore
            .connections()
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    #[test]
    fn cria_o_grupo_de_destino_os_subgrupos_e_as_pastas_vazias() {
        let mut arvore = Tree::default();
        let mut i = importacao(vec![ssh("web", "w.exemplo", &["Clientes", "ACME"])]);
        i.entradas.push(Entrada::Pasta(PastaTrazida {
            caminho: vec!["Vazia".to_owned()],
            ..PastaTrazida::default()
        }));
        let r = aplicar(
            &mut arvore,
            None,
            i,
            &opcoes("Importado", false, Duplicadas::Pular),
        );
        assert_eq!(r.importadas, 1);
        let Some(Node::Group(raiz)) = arvore.nodes.first() else {
            panic!("sem o grupo de destino");
        };
        assert_eq!(raiz.name, "Importado");
        assert_eq!(r.grupo, Some(raiz.id));
        let nomes_dos_grupos: Vec<&str> = raiz
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Group(g) => Some(g.name.as_str()),
                Node::Connection(_) => None,
            })
            .collect();
        assert_eq!(nomes_dos_grupos, ["Clientes", "Vazia"]);
        assert_eq!(arvore.connection_count(), 1);
    }

    #[test]
    fn o_provedor_de_ia_que_nao_existe_aqui_volta_ao_padrao() {
        let mut daqui = ssh("daqui", "a.exemplo", &[]);
        daqui.conexao.perfil_ia = Some(2);
        let mut de_fora = ssh("de-fora", "b.exemplo", &[]);
        de_fora.conexao.perfil_ia = Some(9);
        let mut arvore = Tree::default();
        let mut o = opcoes("", false, Duplicadas::Pular);
        o.perfis_de_ia = vec![1, 2];
        aplicar(&mut arvore, None, importacao(vec![daqui, de_fora]), &o);
        let ids: Vec<_> = arvore
            .connections()
            .iter()
            .map(|c| (c.name.clone(), c.perfil_ia))
            .collect();
        assert_eq!(
            ids,
            [("daqui".to_owned(), Some(2)), ("de-fora".to_owned(), None)]
        );
    }

    #[test]
    fn a_pasta_importada_leva_os_comandos() {
        use crate::modelo::{ComandosAoConectar, ModoComandos, Passo};
        let comandos = ComandosAoConectar {
            modo: ModoComandos::Proprios,
            passos: vec![Passo::comando("cd /srv", 15)],
            reconectar: true,
        };
        let mut arvore = Tree::default();
        let mut i = importacao(vec![]);
        i.entradas.push(Entrada::Pasta(PastaTrazida {
            caminho: vec!["ACME".to_owned()],
            comandos: comandos.clone(),
            ..PastaTrazida::default()
        }));
        aplicar(&mut arvore, None, i, &opcoes("", false, Duplicadas::Pular));
        let Some(Node::Group(g)) = arvore.nodes.first() else {
            panic!("sem a pasta");
        };
        assert_eq!(g.comandos, comandos);
    }

    #[test]
    fn sem_grupo_de_destino_vai_para_a_raiz() {
        let mut arvore = Tree::default();
        let r = aplicar(
            &mut arvore,
            None,
            importacao(vec![ssh("web", "w.exemplo", &[])]),
            &opcoes("", false, Duplicadas::Pular),
        );
        assert_eq!(r.grupo, None);
        assert!(matches!(arvore.nodes.first(), Some(Node::Connection(_))));
    }

    #[test]
    fn duplicadas_pular_substituir_e_manter_as_duas() {
        let mut arvore = Tree::default();
        let primeira = importacao(vec![ssh("web", "w.exemplo", &["A"])]);
        aplicar(
            &mut arvore,
            None,
            primeira,
            &opcoes("D", false, Duplicadas::Pular),
        );

        let mut outra = ssh("web", "w.exemplo", &["A"]);
        outra.conexao.port = 2222;
        let r = aplicar(
            &mut arvore,
            None,
            importacao(vec![outra.clone()]),
            &opcoes("D", false, Duplicadas::Pular),
        );
        assert_eq!((r.importadas, r.puladas), (0, 1));
        assert_eq!(arvore.connections()[0].port, 22);

        // Substituir mantém o modo da credencial e a descrição que o arquivo de fora não traz.
        let id = arvore.connections()[0].id;
        arvore.connection_mut(id).unwrap().credencial_modo = ModoCredencial::AgenteOuSenha;
        arvore.connection_mut(id).unwrap().descricao = "a de antes".to_owned();
        let r = aplicar(
            &mut arvore,
            None,
            importacao(vec![outra.clone()]),
            &opcoes("D", false, Duplicadas::Substituir),
        );
        assert_eq!((r.importadas, r.substituidas), (0, 1));
        assert_eq!(arvore.connection_count(), 1);
        assert_eq!(arvore.connections()[0].port, 2222);
        assert_eq!(
            arvore.connections()[0].credencial_modo,
            ModoCredencial::AgenteOuSenha
        );
        assert_eq!(arvore.connections()[0].descricao, "a de antes");

        let r = aplicar(
            &mut arvore,
            None,
            importacao(vec![outra.clone(), outra]),
            &opcoes("D", false, Duplicadas::ManterAsDuas),
        );
        assert_eq!(r.importadas, 2);
        assert_eq!(nomes(&arvore), ["web", "web (2)", "web (3)"]);
        assert_eq!(r.resumo(), "Importei 2 conexões.");
    }

    #[test]
    fn as_senhas_vao_para_o_cofre_e_ligam_a_conexao() {
        let (mut cofre, _pasta) = cofre("aplicar-senhas");
        let mut arvore = Tree::default();
        let mut t = com_senha(ssh("web", "w.exemplo", &[]), "segredo");
        t.conexao.jump = Some(Jump {
            host: "gw.exemplo".to_owned(),
            port: 2200,
            user: "salto".to_owned(),
            credencial: None,
        });
        t.senha_salto = Some("do-salto".to_owned());
        t.conexao.descricao = "a descrição".to_owned();
        let r = aplicar(
            &mut arvore,
            Some(&mut cofre),
            importacao(vec![t]),
            &opcoes("D", true, Duplicadas::Pular),
        );
        assert_eq!(r.senhas, 2, "a descrição não vai ao cofre");
        let c = arvore.connections()[0].clone();
        let item = cofre.item(c.credencial.unwrap()).unwrap();
        assert_eq!(
            (item.usuario.as_str(), item.senha.as_str()),
            ("root", "segredo")
        );
        let salto = cofre.item(c.jump.unwrap().credencial.unwrap()).unwrap();
        assert_eq!(salto.senha, "do-salto");
        assert_eq!((c.descricao.as_str(), c.nota), ("a descrição", None));
    }

    #[test]
    fn cofre_bloqueado_deixa_as_senhas_de_fora_e_avisa() {
        let mut arvore = Tree::default();
        let mut t = com_senha(ssh("web", "w.exemplo", &[]), "segredo");
        t.conexao.descricao = "a descrição".to_owned();
        let r = aplicar(
            &mut arvore,
            None,
            importacao(vec![t]),
            &opcoes("D", true, Duplicadas::Pular),
        );
        assert_eq!(r.importadas, 1);
        assert_eq!(r.senhas, 0);
        let c = &arvore.connections()[0];
        assert!(c.credencial.is_none());
        assert_eq!(
            c.descricao, "a descrição",
            "a descrição não depende do cofre"
        );
        assert_eq!(r.avisos.len(), 1);
    }

    #[test]
    fn o_token_do_s3_vai_no_campo_do_item() {
        let (mut cofre, _pasta) = cofre("aplicar-s3");
        let mut arvore = Tree::default();
        let mut c = Connection::new("bucket", Kind::S3);
        c.s3.endpoint = "s3.exemplo".to_owned();
        c.user = "AKIA".to_owned();
        let mut t = Trazida::nova(c, Vec::new());
        t.senha = Some("secreta".to_owned());
        t.token_s3 = Some("token".to_owned());
        aplicar(
            &mut arvore,
            Some(&mut cofre),
            importacao(vec![t]),
            &opcoes("", true, Duplicadas::Pular),
        );
        let item = cofre
            .item(arvore.connections()[0].credencial.unwrap())
            .unwrap();
        assert_eq!(item.tipo, Tipo::S3);
        assert_eq!(item.senha, "secreta");
        assert!(
            item.campos
                .iter()
                .any(|c| c.nome == CAMPO_TOKEN && c.valor == "token")
        );
    }

    #[test]
    fn o_mesmo_proxy_vira_um_perfil_so() {
        let mut arvore = Tree::default();
        let perfil = || {
            let mut p = PerfilRede::new("Proxy importado (127.0.0.1:1081)");
            p.proxy = Some(ProxyRede {
                tipo: TipoProxy::Socks5,
                host: "127.0.0.1".to_owned(),
                port: 1081,
                credencial: None,
            });
            PerfilTrazido {
                perfil: p,
                usuario: String::new(),
                senha: None,
            }
        };
        let mut a = ssh("a", "a.exemplo", &[]);
        a.perfil = Some(perfil());
        let mut b = ssh("b", "b.exemplo", &[]);
        b.perfil = Some(perfil());
        aplicar(
            &mut arvore,
            None,
            importacao(vec![a, b]),
            &opcoes("", false, Duplicadas::Pular),
        );
        assert_eq!(arvore.redes.len(), 1);
        let id = arvore.redes[0].id;
        assert!(
            arvore
                .connections()
                .iter()
                .all(|c| c.rede == Rede::Perfil(id))
        );
    }

    #[test]
    fn a_senha_do_proxy_vai_para_o_cofre() {
        let (mut cofre, _pasta) = cofre("aplicar-proxy");
        let mut arvore = Tree::default();
        let mut p = PerfilRede::new("Proxy importado (10.0.0.1:1080)");
        p.proxy = Some(ProxyRede {
            tipo: TipoProxy::Socks5,
            host: "10.0.0.1".to_owned(),
            port: 1080,
            credencial: None,
        });
        let mut t = ssh("a", "a.exemplo", &[]);
        t.perfil = Some(PerfilTrazido {
            perfil: p,
            usuario: "pxy".to_owned(),
            senha: Some("pxy-senha".to_owned()),
        });
        aplicar(
            &mut arvore,
            Some(&mut cofre),
            importacao(vec![t]),
            &opcoes("", true, Duplicadas::Pular),
        );
        let credencial = arvore.redes[0].proxy.as_ref().unwrap().credencial.unwrap();
        let item = cofre.item(credencial).unwrap();
        assert_eq!(
            (item.usuario.as_str(), item.senha.as_str()),
            ("pxy", "pxy-senha")
        );
    }

    #[test]
    fn o_bloco_do_cofre_do_json_nativo_e_religado() {
        let (mut cofre, _pasta) = cofre("aplicar-nativo");
        let mut arvore = Tree::default();
        let mut t = ssh("web", "w.exemplo", &[]);
        t.conexao.credencial = Some(7);
        let mut i = importacao(vec![t]);
        i.cofre = Some(Lote {
            pastas: Vec::new(),
            itens: vec![Item {
                id: 7,
                tipo: Tipo::Ssh,
                titulo: "web".to_owned(),
                senha: "da-origem".to_owned(),
                ..Item::default()
            }],
        });
        let r = aplicar(
            &mut arvore,
            Some(&mut cofre),
            i,
            &opcoes("D", true, Duplicadas::Pular),
        );
        assert_eq!(r.senhas, 1);
        let id = arvore.connections()[0].credencial.unwrap();
        assert_eq!(cofre.item(id).unwrap().senha, "da-origem");
    }

    #[test]
    fn sem_senhas_as_ligacoes_do_json_nativo_se_soltam() {
        let (mut cofre, _pasta) = cofre("aplicar-nativo-solto");
        let mut arvore = Tree::default();
        let mut t = ssh("web", "w.exemplo", &[]);
        t.conexao.credencial = Some(7);
        t.conexao.nota = Some(8);
        t.conexao.chave_do_cofre = Some("uid-x".to_owned());
        let mut i = importacao(vec![t]);
        i.cofre = Some(Lote::default());
        aplicar(
            &mut arvore,
            Some(&mut cofre),
            i,
            &opcoes("D", false, Duplicadas::Pular),
        );
        let c = &arvore.connections()[0];
        assert_eq!(
            (c.credencial, c.nota, c.chave_do_cofre.clone()),
            (None, None, None)
        );
    }
}
