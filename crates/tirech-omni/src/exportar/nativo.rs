//! O JSON do próprio Tirech Omni: a árvore inteira (pastas, redes e conexões com tudo) e, com
//! "incluir senhas", o bloco `cofre` com os itens que a árvore usa (credenciais, notas e chaves),
//! no mesmo documento do `cofre_json`. Sem o bloco, as ligações com o cofre saem da árvore, para
//! não apontarem para o item errado de outro cofre.

use std::path::Path;

use serde_json::Value;

use super::{Opcoes, Resultado};
use crate::anexos;
use crate::cofre::Cofre;
use crate::cofre_json::{Documento, pasta_dos_anexos};
use crate::modelo::{Connection, Group, Node, Tree};
use crate::t;

/// Chama `f` em cada conexão da árvore, nas pastas também.
fn cada_conexao(nos: &mut [Node], f: &mut dyn FnMut(&mut Connection)) {
    for no in nos {
        match no {
            Node::Group(g) => cada_conexao(&mut g.children, f),
            Node::Connection(c) => f(c),
        }
    }
}

/// Chama `f` em cada pasta da árvore, nas aninhadas também.
fn cada_pasta(nos: &mut [Node], f: &mut dyn FnMut(&mut Group)) {
    for no in nos {
        if let Node::Group(g) = no {
            f(g);
            cada_pasta(&mut g.children, f);
        }
    }
}

/// Tira da árvore o que aponta para o cofre.
fn soltar_do_cofre(arvore: &mut Tree) {
    cada_conexao(&mut arvore.nodes, &mut |c| {
        c.credencial = None;
        c.nota = None;
        c.chave_do_cofre = None;
        c.comandos.religar_senhas(|_| None);
        if let Some(salto) = c.jump.as_mut() {
            salto.credencial = None;
        }
    });
    cada_pasta(&mut arvore.nodes, &mut |g| {
        g.comandos.religar_senhas(|_| None);
        g.chave_do_cofre = None;
    });
    for perfil in &mut arvore.redes {
        if let Some(proxy) = perfil.proxy.as_mut() {
            proxy.credencial = None;
        }
        if let Some(salto) = perfil.salto.as_mut() {
            salto.credencial = None;
        }
        if let Some(nebula) = perfil.nebula.as_mut() {
            nebula.identidade = None;
        }
        if let Some(vpn) = perfil.openvpn.as_mut() {
            vpn.identidade = None;
        }
        if let Some(wg) = perfil.wireguard.as_mut() {
            wg.identidade = None;
        }
        if let Some(ts) = perfil.tailscale.as_mut() {
            ts.identidade = None;
        }
        if let Some(zt) = perfil.zerotier.as_mut() {
            zt.identidade = None;
        }
    }
}

/// Os ids dos itens do cofre que a árvore usa.
fn itens_usados(arvore: &mut Tree, cofre: &Cofre) -> Vec<u64> {
    let mut ids: Vec<u64> = Vec::new();
    cada_conexao(&mut arvore.nodes, &mut |c| {
        ids.extend(c.credencial);
        ids.extend(c.nota);
        ids.extend(c.jump.as_ref().and_then(|j| j.credencial));
        ids.extend(c.comandos.ids_do_cofre());
        if let Some(uid) = &c.chave_do_cofre
            && let Some(item) = cofre.itens().iter().find(|i| &i.uid == uid)
        {
            ids.push(item.id);
        }
    });
    cada_pasta(&mut arvore.nodes, &mut |g| {
        ids.extend(g.comandos.ids_do_cofre());
        if let Some(uid) = &g.chave_do_cofre
            && let Some(item) = cofre.itens().iter().find(|i| &i.uid == uid)
        {
            ids.push(item.id);
        }
    });
    for perfil in &arvore.redes {
        ids.extend(perfil.proxy.as_ref().and_then(|p| p.credencial));
        ids.extend(perfil.salto.as_ref().and_then(|j| j.credencial));
        ids.extend(perfil.nebula.as_ref().and_then(|n| n.identidade));
        ids.extend(perfil.openvpn.as_ref().and_then(|n| n.identidade));
        ids.extend(perfil.wireguard.as_ref().and_then(|n| n.identidade));
        ids.extend(perfil.tailscale.as_ref().and_then(|n| n.identidade));
        ids.extend(perfil.zerotier.as_ref().and_then(|n| n.identidade));
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn contar(nos: &[Node]) -> usize {
    nos.iter()
        .map(|no| match no {
            Node::Group(g) => contar(&g.children),
            Node::Connection(_) => 1,
        })
        .sum()
}

/// Escreve o arquivo. `destino` é o arquivo que vai receber o texto: o nome dele dá o da pasta
/// dos anexos das notas.
pub fn exportar(arvore: &Tree, cofre: Option<&Cofre>, opcoes: Opcoes, destino: &Path) -> Resultado {
    let mut resultado = Resultado::default();
    let mut arvore = arvore.clone();
    resultado.exportadas = contar(&arvore.nodes);
    let cofre = cofre.filter(|_| opcoes.senhas);
    if opcoes.senhas && cofre.is_none() {
        resultado.avisar(t!("exportar-nativo-cofre-bloqueado"));
    }
    let mut documento = None;
    if let Some(cofre) = cofre {
        if !opcoes.chaves {
            cada_conexao(&mut arvore.nodes, &mut |c| c.chave_do_cofre = None);
            cada_pasta(&mut arvore.nodes, &mut |g| g.chave_do_cofre = None);
        }
        let ids = itens_usados(&mut arvore, cofre);
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        let exportacao =
            Documento::parcial(cofre, &ids).com_anexos(&pasta_dos_anexos(destino), &vault);
        if exportacao.ausentes > 0 {
            resultado.avisar(t!("exportar-nativo-anexos", n = exportacao.ausentes as i64));
        }
        resultado.anexos = exportacao.arquivos;
        documento = serde_json::from_str::<Value>(&exportacao.texto).ok();
    } else {
        soltar_do_cofre(&mut arvore);
    }
    let mut valor = serde_json::to_value(&arvore).unwrap_or(Value::Null);
    if let (Some(documento), Some(objeto)) = (documento, valor.as_object_mut()) {
        objeto.insert("cofre".to_owned(), documento);
    }
    resultado.texto = serde_json::to_string_pretty(&valor).unwrap_or_default();
    resultado
}
