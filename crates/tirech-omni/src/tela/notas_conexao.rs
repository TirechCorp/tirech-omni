//! O que a casca usa do cofre e das abas para as conexões, e a migração das notas que o M47
//! ligava a elas: desde então a conexão tem só uma descrição, e as notas ficam para guardar
//! segredos e textos soltos na página Notas (`crate::tela_notas`).

use std::io;
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use crate::cofre::{Cofre, Tipo};
use crate::modelo::Tree;
use crate::t;

/// Alguma conexão da árvore ainda aponta para uma nota do cofre (arquivo de antes da descrição).
fn tem_nota_ligada(arvore: &Tree) -> bool {
    arvore.connections().iter().any(|c| c.nota.is_some())
}

/// Leva cada nota ligada a uma conexão para a `descricao` dela e solta a ligação. O texto da nota
/// entra na descrição (depois da que já existe, se for outro) e a nota sai do cofre; a que tem
/// anexos fica, só sem a ligação, para não perder os arquivos. Uma nota que não existe mais só
/// solta a ligação. Com o cofre bloqueado não faz nada; num erro ao apagar, a árvore já mudou.
pub(crate) fn migrar_notas_ligadas(arvore: &mut Tree, cofre: &mut Cofre) -> io::Result<()> {
    if cofre.bloqueado() {
        return Ok(());
    }
    let ligadas: Vec<(u64, u64)> = arvore
        .connections()
        .iter()
        .filter_map(|c| c.nota.map(|n| (c.id, n)))
        .collect();
    let mut apagar = Vec::new();
    for (id, nota) in ligadas {
        let item = cofre.item(nota).filter(|i| i.tipo == Tipo::Nota);
        let Some(c) = arvore.connection_mut(id) else {
            continue;
        };
        if let Some(item) = item {
            let texto = item.notas.trim();
            let atual = c.descricao.trim();
            if atual.is_empty() {
                c.descricao = texto.to_owned();
            } else if !texto.is_empty() && atual != texto {
                c.descricao = format!("{atual}\n{texto}");
            }
            if item.anexos.is_empty() && !apagar.contains(&nota) {
                apagar.push(nota);
            }
        }
        c.nota = None;
    }
    if apagar.is_empty() {
        Ok(())
    } else {
        cofre.apagar(&[], &apagar)
    }
}

impl Tela {
    /// A conexão que abriu a aba `sessao` (o início da chave da sessão é o id dela).
    pub(super) fn conexao_da_sessao(&self, sessao: Session) -> Option<u64> {
        sessao
            .key_str()
            .split('-')
            .next()
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| self.tree.with_untracked(|t| t.connection(*id).is_some()))
    }

    /// Com o cofre bloqueado, pede a senha mestra e roda `f` quando ele abrir; aberto, roda já.
    pub(super) fn com_cofre_aberto(&self, f: impl FnOnce(&Tela) + 'static) {
        if self.cofre.agora(|c| c.bloqueado()) {
            *self.depois_de_desbloquear.borrow_mut() = Some(Box::new(f));
            self.desbloquear.set(true);
        } else {
            f(self);
        }
    }

    /// Liga o efeito que roda o que ficou esperando o desbloqueio do cofre; se o diálogo fecha
    /// com o cofre ainda bloqueado, a espera acaba.
    pub(super) fn vigiar_desbloqueio(tela: &Rc<Tela>) {
        let t = Rc::clone(tela);
        effect(move || {
            if t.desbloquear.get() || t.refazer_desbloqueio.get() {
                return;
            }
            untrack(|| {
                let Some(depois) = t.depois_de_desbloquear.borrow_mut().take() else {
                    return;
                };
                if !t.cofre.agora(|c| c.bloqueado()) {
                    depois(&t);
                }
            });
        });
    }

    /// Liga o efeito que, com o cofre aberto, passa as notas ligadas às conexões para a
    /// descrição ([`migrar_notas_ligadas`]). Roda uma vez por arquivo antigo: depois nenhuma
    /// conexão aponta para nota, e o efeito só confere isso.
    pub(super) fn vigiar_notas_ligadas(tela: &Rc<Tela>) {
        let t = Rc::clone(tela);
        effect(move || {
            let _ = t.cofre.versao();
            untrack(|| {
                if t.cofre.agora(Cofre::bloqueado) {
                    return;
                }
                let Some(mut arvore) = t.tree.with(|a| tem_nota_ligada(a).then(|| a.clone()))
                else {
                    return;
                };
                let resultado = t.cofre.mudar(|c| migrar_notas_ligadas(&mut arvore, c));
                t.tree.set(arvore);
                t.atualizar();
                if let Some(c) = t.conexao_selecionada() {
                    t.campos.descricao.set(c.descricao);
                }
                if let Err(erro) = resultado {
                    t.avisar(
                        InfoBarSeverity::Error,
                        t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
                    );
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::cofre::{Anexo, Item};
    use crate::modelo::{Connection, Kind};

    fn arvore_com(conexoes: Vec<Connection>) -> Tree {
        let mut arvore = Tree::default();
        for c in conexoes {
            arvore.add_connection(None, c);
        }
        arvore
    }

    fn nota(cofre: &mut Cofre, texto: &str, anexos: Vec<Anexo>) -> u64 {
        cofre
            .salvar(Item {
                tipo: Tipo::Nota,
                titulo: "srv".into(),
                notas: texto.into(),
                anexos,
                ..Item::default()
            })
            .unwrap()
    }

    #[test]
    fn a_nota_ligada_vira_descricao_e_sai_do_cofre() {
        let pasta = PastaTemporaria::nova("migrar-notas");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let (n1, n2, n3) = (
            nota(&mut cofre, "  banco & site\n", Vec::new()),
            nota(&mut cofre, "segunda", Vec::new()),
            nota(&mut cofre, "com anexo", vec![Anexo::default()]),
        );
        let mut a = Connection::new("a", Kind::Ssh);
        a.nota = Some(n1);
        let mut b = Connection::new("b", Kind::Ssh);
        b.descricao = "já tinha".into();
        b.nota = Some(n2);
        let mut c = Connection::new("c", Kind::Ssh);
        c.nota = Some(n3);
        let mut d = Connection::new("d", Kind::Ssh);
        d.nota = Some(9999);
        let mut arvore = arvore_com(vec![a, b, c, d]);
        assert!(tem_nota_ligada(&arvore));
        migrar_notas_ligadas(&mut arvore, &mut cofre).unwrap();
        assert!(!tem_nota_ligada(&arvore));
        let descricoes: Vec<&str> = arvore
            .connections()
            .iter()
            .map(|c| c.descricao.as_str())
            .collect();
        assert_eq!(
            descricoes,
            ["banco & site", "já tinha\nsegunda", "com anexo", ""]
        );
        assert!(cofre.item(n1).is_none() && cofre.item(n2).is_none());
        assert!(cofre.item(n3).is_some(), "a nota com anexos fica");
    }

    #[test]
    fn com_o_cofre_bloqueado_nada_muda() {
        let pasta = PastaTemporaria::nova("migrar-notas-bloqueado");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let n = nota(&mut cofre, "texto", Vec::new());
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        cofre.bloquear();
        let mut c = Connection::new("c", Kind::Ssh);
        c.nota = Some(n);
        let mut arvore = arvore_com(vec![c]);
        migrar_notas_ligadas(&mut arvore, &mut cofre).unwrap();
        assert!(tem_nota_ligada(&arvore));
    }
}
