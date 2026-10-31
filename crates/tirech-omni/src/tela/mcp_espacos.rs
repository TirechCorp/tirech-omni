//! Os espaços de trabalho dos assistentes na interface (M51, seção 9.4): o diálogo de criar e
//! editar um espaço (uma pasta de um servidor SSH que o assistente trata como se fosse local).
//! A lista fica em Configurações > Assistentes de IA, e a pasta do painel de arquivos e o menu
//! Ferramentas abrem o mesmo diálogo, com a conexão e a pasta já preenchidas. O app não tem um
//! seletor de pasta remota, então a pasta do servidor é só o texto.
//!
//! O botão do `ContentDialog` fecha a caixa depois de rodar a ação: se o espaço não serve, a
//! caixa volta a abrir com o motivo (o mesmo molde do "Salvar como espaço de trabalho").

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use tomni_ui::prelude::*;

use super::Tela;
use crate::mcp::espaco::{EspacoIa, nome_livre, nome_valido, raiz_windows};
use crate::mcp::permissao::{PermissaoIa, permissao_efetiva};
use crate::modelo::Kind;
use crate::t;

/// O que o diálogo do espaço mostra e guarda enquanto está aberto.
pub(super) struct SinaisEspaco {
    /// Se a caixa está aberta.
    pub aberto: Signal<bool>,
    /// O nome do espaço.
    pub nome: Signal<String>,
    /// O rótulo da conexão escolhida na `ComboBox`.
    pub conexao: Signal<String>,
    /// A pasta no servidor.
    pub raiz: Signal<String>,
    /// Por que o espaço não serviu, ou vazio.
    pub erro: Signal<String>,
    /// Muda quando as conexões da `ComboBox` precisam ser listadas de novo.
    pub versao: Signal<u64>,
    /// O nome do espaço que se edita, ou `None` num espaço novo.
    pub editando: RefCell<Option<String>>,
    /// As conexões da `ComboBox`: o rótulo e o id.
    pub conexoes: RefCell<Vec<(String, u64)>>,
    /// Se o botão pediu para a caixa abrir de novo (o motivo está em `erro`).
    pub reabrir: Cell<bool>,
}

impl SinaisEspaco {
    pub(super) fn novos() -> Self {
        Self {
            aberto: signal(false),
            nome: signal(String::new()),
            conexao: signal(String::new()),
            raiz: signal(String::new()),
            erro: signal(String::new()),
            versao: signal(0),
            editando: RefCell::new(None),
            conexoes: RefCell::new(Vec::new()),
            reabrir: Cell::new(false),
        }
    }
}

/// A pasta sem a barra do fim (a raiz `/` fica como está).
fn sem_barra_final(raiz: &str) -> String {
    let raiz = raiz.trim();
    let aparada = raiz.trim_end_matches(['/', '\\']);
    if aparada.is_empty() && raiz.starts_with('/') {
        "/".to_owned()
    } else {
        aparada.to_owned()
    }
}

/// O nome da última pasta de `raiz`, para sugerir o nome do espaço.
fn ultima_pasta(raiz: &str) -> String {
    raiz.trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Se `raiz` é uma pasta absoluta: `/…` ou a letra de uma unidade do Windows.
fn raiz_absoluta(raiz: &str) -> bool {
    raiz.starts_with('/') || raiz_windows(raiz)
}

impl Tela {
    /// As conexões SSH da árvore, com o rótulo da `ComboBox` (as desligadas para assistentes
    /// levam o aviso no fim).
    fn conexoes_para_espaco(&self) -> Vec<(String, u64)> {
        self.tree.with_untracked(|tree| {
            tree.connections()
                .into_iter()
                .filter(|c| c.kind == Kind::Ssh)
                .map(|c| {
                    let rotulo = if permissao_efetiva(tree, c.id) == PermissaoIa::Desligada {
                        t!("mcp-espaco-conexao-desligada", nome = c.name.as_str())
                    } else {
                        c.name.clone()
                    };
                    (rotulo, c.id)
                })
                .collect()
        })
    }

    /// O id da conexão escolhida na caixa.
    fn conexao_do_espaco(&self) -> Option<u64> {
        let escolhida = self.mcp.espaco.conexao.get_untracked();
        self.mcp
            .espaco
            .conexoes
            .borrow()
            .iter()
            .find(|(rotulo, _)| *rotulo == escolhida)
            .map(|(_, id)| *id)
    }

    /// Se a conexão escolhida está desligada para assistentes (a caixa avisa e oferece liberar).
    fn conexao_do_espaco_desligada(&self) -> bool {
        let _ = self.mcp.espaco.versao.get();
        let escolhida = self.mcp.espaco.conexao.get();
        let id = self
            .mcp
            .espaco
            .conexoes
            .borrow()
            .iter()
            .find(|(rotulo, _)| *rotulo == escolhida)
            .map(|(_, id)| *id);
        id.is_some_and(|id| {
            self.tree
                .with(|tree| permissao_efetiva(tree, id) == PermissaoIa::Desligada)
        })
    }

    /// Abre o diálogo de um espaço novo. `conexao` e `raiz` já vêm preenchidos quando o pedido
    /// parte de uma pasta do painel de arquivos ou do terminal.
    pub(super) fn novo_espaco_ia(self: &Rc<Self>, conexao: Option<u64>, raiz: &str) {
        let raiz = sem_barra_final(raiz);
        let existentes = self.config.with_untracked(|c| c.ia.espacos.clone());
        let sugestao = if raiz.is_empty() || raiz == "/" {
            String::new()
        } else {
            nome_livre(&ultima_pasta(&raiz), &existentes)
        };
        self.abrir_dialogo_do_espaco(None, &sugestao, conexao, &raiz);
    }

    /// Abre o diálogo para editar o espaço `nome`.
    pub(super) fn editar_espaco_ia(self: &Rc<Self>, nome: &str) {
        let Some(espaco) = self
            .config
            .with_untracked(|c| c.ia.espacos.iter().find(|e| e.nome == nome).cloned())
        else {
            return;
        };
        self.abrir_dialogo_do_espaco(
            Some(nome.to_owned()),
            &espaco.nome,
            Some(espaco.conexao),
            &espaco.raiz,
        );
    }

    fn abrir_dialogo_do_espaco(
        self: &Rc<Self>,
        editando: Option<String>,
        nome: &str,
        conexao: Option<u64>,
        raiz: &str,
    ) {
        let s = &self.mcp.espaco;
        let conexoes = self.conexoes_para_espaco();
        let escolhida = conexao
            .and_then(|id| conexoes.iter().find(|(_, c)| *c == id))
            .or(conexoes.first())
            .map(|(rotulo, _)| rotulo.clone())
            .unwrap_or_default();
        *s.conexoes.borrow_mut() = conexoes;
        *s.editando.borrow_mut() = editando;
        s.nome.set(nome.to_owned());
        s.conexao.set(escolhida);
        s.raiz.set(raiz.to_owned());
        s.erro.set(String::new());
        s.versao.update(|v| *v += 1);
        s.aberto.set(true);
    }

    /// "Liberar com aprovação": a conexão do espaço passa a "Aprovar cada ação".
    fn liberar_conexao_do_espaco(self: &Rc<Self>) {
        let Some(id) = self.conexao_do_espaco() else {
            return;
        };
        self.mudar_permissao_mcp(id, PermissaoIa::Aprovar);
        // A caixa de conexões se refaz sem o aviso de "desligada".
        let conexoes = self.conexoes_para_espaco();
        let s = &self.mcp.espaco;
        if let Some((rotulo, _)) = conexoes.iter().find(|(_, c)| *c == id) {
            s.conexao.set(rotulo.clone());
        }
        *s.conexoes.borrow_mut() = conexoes;
        s.versao.update(|v| *v += 1);
    }

    /// O "Salvar" do diálogo: confere o espaço, grava na configuração e avisa.
    fn salvar_espaco_ia(self: &Rc<Self>) {
        let s = &self.mcp.espaco;
        let nome = s.nome.get_untracked().trim().to_owned();
        let raiz = sem_barra_final(&s.raiz.get_untracked());
        let editando = s.editando.borrow().clone();
        let recusar = |motivo: String| {
            s.erro.set(motivo);
            s.reabrir.set(true);
        };
        if !nome_valido(&nome) {
            return recusar(t!("mcp-espaco-erro-nome"));
        }
        let repetido = self.config.with_untracked(|c| {
            c.ia.espacos.iter().any(|e| {
                e.nome.eq_ignore_ascii_case(&nome)
                    && editando.as_deref().is_none_or(|antigo| antigo != e.nome)
            })
        });
        if repetido {
            return recusar(t!("mcp-espaco-erro-repetido", nome = nome.as_str()));
        }
        let Some(conexao) = self.conexao_do_espaco() else {
            return recusar(t!("mcp-espaco-erro-sem-conexao"));
        };
        if !raiz_absoluta(&raiz) {
            return recusar(t!("mcp-espaco-erro-raiz"));
        }
        let novo = EspacoIa {
            nome: nome.clone(),
            conexao,
            raiz,
            espelho: None,
        };
        self.config.update(|c| {
            let lugar = editando
                .as_deref()
                .and_then(|antigo| c.ia.espacos.iter().position(|e| e.nome == antigo));
            match lugar {
                Some(i) => c.ia.espacos[i] = novo,
                None => c.ia.espacos.push(novo),
            }
        });
        s.erro.set(String::new());
        self.avisar(
            InfoBarSeverity::Success,
            t!("mcp-espaco-salvo", nome = nome.as_str()),
        );
    }
}

/// O diálogo do espaço de trabalho (`mcp-espaco-dialogo`). O efeito que reabre a caixa liga uma
/// vez, e a função devolvida monta a caixa em cada janela.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl Fn() -> ContentDialog + use<> {
    let s = &tela.mcp.espaco;
    let (aberto, nome, conexao, raiz, erro, versao) =
        (s.aberto, s.nome, s.conexao, s.raiz, s.erro, s.versao);
    // O botão fecha a caixa: se o espaço não serviu, ela abre de novo com o motivo.
    {
        let tela = Rc::clone(tela);
        effect(move || {
            if aberto.get() {
                return;
            }
            let reabrir = tela.mcp.espaco.reabrir.replace(false);
            untrack(|| {
                if reabrir {
                    aberto.set(true);
                }
            });
        });
    }
    let tela = Rc::clone(tela);
    move || {
        let tela = &tela;
        let opcoes = {
            let tela = Rc::clone(tela);
            // A lista nasce com a caixa e se refaz ao liberar uma conexão.
            For::each(
                move || vec![versao.get()],
                |v| *v,
                move |_| {
                    let rotulos: Vec<String> = tela
                        .mcp
                        .espaco
                        .conexoes
                        .borrow()
                        .iter()
                        .map(|(rotulo, _)| rotulo.clone())
                        .collect();
                    ComboBox::bind(conexao, rotulos).id("mcp-espaco-conexao")
                },
            )
        };
        let (desligada, liberar) = (Rc::clone(tela), Rc::clone(tela));
        let salvar = Rc::clone(tela);
        ContentDialog::new(
            {
                let tela = Rc::clone(tela);
                move || {
                    if tela.mcp.espaco.editando.borrow().is_some() && aberto.get() {
                        t!("mcp-espaco-titulo-editar")
                    } else {
                        t!("mcp-espaco-titulo-novo")
                    }
                }
            },
            aberto,
        )
        .id("mcp-espaco-dialogo")
        .content(
            Column::new()
                .gap(8)
                .width(440)
                .child(Text::new(t!("mcp-espaco-explicacao")).secondary().wrap())
                .child(Text::new(t!("mcp-espaco-nome")).caption())
                .child(
                    Edit::bind(nome)
                        .id("mcp-espaco-nome")
                        .placeholder(t!("mcp-espaco-nome-dica")),
                )
                .child(Text::new(t!("mcp-espaco-conexao")).caption())
                .child(opcoes)
                .child(
                    Column::new()
                        .gap(4)
                        .visible(move || desligada.conexao_do_espaco_desligada())
                        .child(
                            Text::new(t!("mcp-espaco-liberar"))
                                .id("mcp-espaco-liberar-texto")
                                .secondary()
                                .wrap(),
                        )
                        .child(
                            Row::new().child(
                                Button::new(t!("mcp-espaco-liberar-botao"))
                                    .id("mcp-espaco-liberar")
                                    .on_click(move || liberar.liberar_conexao_do_espaco()),
                            ),
                        ),
                )
                .child(Text::new(t!("mcp-espaco-raiz")).caption())
                .child(
                    Edit::bind(raiz)
                        .id("mcp-espaco-raiz")
                        .placeholder(t!("mcp-espaco-raiz-dica")),
                )
                .child(
                    Text::new(move || erro.get())
                        .id("mcp-espaco-erro")
                        .caption()
                        .wrap()
                        .visible(move || !erro.get().is_empty()),
                ),
        )
        .primary(t!("comum-salvar"), move || salvar.salvar_espaco_ia())
        .close(t!("comum-cancelar"))
    }
}
