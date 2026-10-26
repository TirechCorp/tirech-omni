//! As chaves do cofre que as conexões usam, gravadas numa pasta ao lado do arquivo exportado
//! (`<arquivo>-chaves/`), para o outro programa achar o `IdentityFile`, o `PublicKeyFile` ou o
//! `Keyfile`. As chaves saem sem passphrase, no formato que o programa lê, e a pasta fica só
//! para a pessoa (0700, e 0600 nos arquivos, no Unix).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tomni_ui::terminal::ssh::keys::{PrivateFormat, PublicFormat};

use super::Alvo;
use crate::chaves_cofre::{Aberta, Parte, nome_de_arquivo};
use crate::cofre::Cofre;
use crate::modelo::{Node, Tree};
use crate::t;

/// O que gravar as chaves deu.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Gravadas {
    /// O arquivo de cada chave gravada, pelo `uid` do item do cofre (caminho absoluto).
    pub arquivos: HashMap<String, PathBuf>,
    /// A pasta onde ficaram (`None` se nenhuma foi gravada).
    pub pasta: Option<PathBuf>,
    /// O que não deu certo e o que a pessoa precisa saber.
    pub avisos: Vec<String>,
}

/// A pasta das chaves de uma exportação para `destino`: `<nome sem extensão>-chaves`, ao lado.
pub fn pasta_das_chaves(destino: &Path) -> PathBuf {
    let base = destino.file_stem().map_or_else(
        || "exportacao".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    destino.with_file_name(format!("{base}-chaves"))
}

/// Os `uid` das chaves do cofre que as conexões do `alvo` usam, sem repetir, na ordem da árvore.
pub fn usadas(arvore: &Tree, alvo: Alvo) -> Vec<String> {
    fn andar(nos: &[Node], alvo: Alvo, vistos: &mut HashSet<String>, uids: &mut Vec<String>) {
        for no in nos {
            match no {
                Node::Group(g) => andar(&g.children, alvo, vistos, uids),
                Node::Connection(c) => {
                    if alvo.aceita(c.kind)
                        && let Some(uid) = &c.chave_do_cofre
                        && vistos.insert(uid.clone())
                    {
                        uids.push(uid.clone());
                    }
                }
            }
        }
    }
    let mut uids = Vec::new();
    andar(&arvore.nodes, alvo, &mut HashSet::new(), &mut uids);
    uids
}

/// Cria a pasta só para a pessoa.
fn criar_pasta(pasta: &Path) -> std::io::Result<()> {
    let mut construtor = fs::DirBuilder::new();
    construtor.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        construtor.mode(0o700);
    }
    construtor.create(pasta)
}

/// Um nome de arquivo que ainda não saiu nesta exportação.
pub(super) fn nome_livre(base: &str, extensao: &str, usados: &mut HashSet<String>) -> String {
    let mut nome = format!("{base}{extensao}");
    let mut n = 2;
    while !usados.insert(nome.to_lowercase()) {
        nome = format!("{base}-{n}{extensao}");
        n += 1;
    }
    nome
}

/// Grava as chaves do cofre que as conexões do `alvo` usam, em `pasta_das_chaves(destino)`.
/// Sem cofre (bloqueado), sem formato de chave no `alvo` ou sem chaves usadas, não grava nada.
pub fn gravar(arvore: &Tree, cofre: Option<&Cofre>, alvo: Alvo, destino: &Path) -> Gravadas {
    let mut gravadas = Gravadas::default();
    let Some(formato) = alvo.formato_de_chave() else {
        return gravadas;
    };
    let uids = usadas(arvore, alvo);
    if uids.is_empty() {
        return gravadas;
    }
    let Some(cofre) = cofre else {
        gravadas.avisos.push(t!("exportar-chaves-cofre-bloqueado"));
        return gravadas;
    };
    let pasta = std::path::absolute(pasta_das_chaves(destino))
        .unwrap_or_else(|_| pasta_das_chaves(destino));
    let mut usados = HashSet::new();
    for uid in uids {
        match gravar_uma(cofre, &uid, formato, &pasta, &mut usados) {
            Ok(arquivo) => {
                gravadas.arquivos.insert(uid, arquivo);
            }
            Err(motivo) => gravadas.avisos.push(motivo),
        }
    }
    if !gravadas.arquivos.is_empty() {
        gravadas.avisos.push(t!(
            "exportar-chaves-gravadas",
            pasta = pasta.display().to_string()
        ));
        gravadas.pasta = Some(pasta);
    }
    gravadas
}

fn gravar_uma(
    cofre: &Cofre,
    uid: &str,
    formato: PrivateFormat,
    pasta: &Path,
    usados: &mut HashSet<String>,
) -> Result<PathBuf, String> {
    let item = cofre
        .item_por_uid(uid)
        .ok_or_else(|| t!("exportar-chave-sumiu"))?;
    let titulo = item.titulo.clone();
    let aberta = Aberta::do_item(item).map_err(|e| {
        t!(
            "exportar-chave-erro",
            titulo = titulo.as_str(),
            erro = e.to_string()
        )
    })?;
    if aberta.privada.is_none() {
        return Err(t!("exportar-chave-so-publica", titulo = titulo.as_str()));
    }
    if !aberta.formatos_privados().contains(&formato) {
        return Err(t!(
            "exportar-chave-sem-formato",
            titulo = titulo.as_str(),
            formato = formato.label()
        ));
    }
    let texto = aberta.exportar(Parte::Privada, formato, PublicFormat::OpenSsh, "", "")?;
    criar_pasta(pasta).map_err(|e| {
        t!(
            "exportar-nao-criou-pasta",
            pasta = pasta.display().to_string(),
            erro = e.to_string()
        )
    })?;
    let nome = nome_livre(&nome_de_arquivo(&titulo), formato.extension(), usados);
    let arquivo = pasta.join(nome);
    aberta
        .gravar(&arquivo, Parte::Privada, &texto)
        .map_err(|e| {
            t!(
                "exportar-chave-erro",
                titulo = titulo.as_str(),
                erro = e.to_string()
            )
        })?;
    Ok(arquivo)
}
