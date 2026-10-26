//! O editor dos comandos ao conectar (M50): o `ContentDialog` que a linha "Comandos" das
//! propriedades abre, no molde do editor de túneis. Trabalha numa cópia: Salvar grava em
//! `campos.comandos` (que o formulário passa à árvore) e Cancelar descarta.

use std::cell::Cell;
use std::rc::Rc;

use tomni_ui::prelude::*;

use super::{Tela, id_do_rotulo, opcoes_de_credencial, rotulo_da_credencial};
use crate::campos::Campo;
use crate::comandos::{comandos_herdados, rotulo_de_herdar, sugestoes};
use crate::modelo::{
    AcaoDoPasso, ComandosAoConectar, Espera, Kind, ModoComandos, Node, OrigemDaSenha, Passo,
};
use crate::recentes::Recentes;
use crate::t;

/// O tempo máximo de um passo novo, quando as configurações não dizem outra coisa.
const MAX_MAXIMO: f64 = 86_400.0;

/// O que o editor guarda enquanto está aberto.
pub(super) struct EstadoEditorComandos {
    pub aberto: Signal<bool>,
    /// O modo, pelo texto da lista de escolha.
    modo: Signal<String>,
    /// As três opções do modo, com o "Herdar (…)" de quando o editor abriu.
    opcoes_modo: Signal<Vec<String>>,
    reconectar: Signal<bool>,
    /// Os passos da cópia. Cada linha da lista escreve no passo dela.
    rascunho: Signal<Vec<Passo>>,
    /// Uma chave nova para cada linha a cada mudança de estrutura (adicionar, mover, remover):
    /// as linhas são refeitas com o que o rascunho tem, e os ids seguem a posição.
    chaves: Signal<Vec<(u64, usize)>>,
    proxima_chave: Cell<u64>,
    /// Uma chave por abertura: a lista de escolha do modo, que tem opções fixas, é refeita com o
    /// "Herdar (…)" de cada vez.
    abertura: Signal<Vec<u64>>,
    /// O nó (conexão ou pasta) que o editor edita.
    alvo: Cell<Option<u64>>,
}

impl EstadoEditorComandos {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            modo: signal(String::new()),
            opcoes_modo: signal(Vec::new()),
            reconectar: signal(true),
            rascunho: signal(Vec::new()),
            chaves: signal(Vec::new()),
            proxima_chave: Cell::new(0),
            abertura: signal(Vec::new()),
            alvo: Cell::new(None),
        }
    }

    fn modo_escolhido(&self) -> ModoComandos {
        let atual = self.modo.get();
        match self
            .opcoes_modo
            .with(|o| o.iter().position(|x| *x == atual))
        {
            Some(1) => ModoComandos::Proprios,
            Some(2) => ModoComandos::Nenhum,
            _ => ModoComandos::Herdar,
        }
    }

    fn escolher_modo(&self, modo: ModoComandos) {
        let posicao = match modo {
            ModoComandos::Herdar => 0,
            ModoComandos::Proprios => 1,
            ModoComandos::Nenhum => 2,
        };
        if let Some(texto) = self.opcoes_modo.with_untracked(|o| o.get(posicao).cloned()) {
            self.modo.set(texto);
        }
    }

    /// Dá uma chave nova a cada linha: o `For` refaz todas.
    fn refazer_linhas(&self) {
        let n = self.rascunho.with_untracked(Vec::len);
        let base = self.proxima_chave.get();
        self.proxima_chave.set(base + n as u64 + 1);
        self.chaves
            .set((0..n).map(|i| (base + i as u64, i)).collect());
    }
}

/// O rótulo da espera de um passo de comando.
fn rotulo_da_espera(espera: &Espera) -> String {
    match espera {
        Espera::TelaParar => t!("comandos-espera-tela"),
        Espera::Texto { .. } => t!("comandos-espera-texto"),
        Espera::Logo => t!("comandos-espera-logo"),
    }
}

fn rotulos_da_espera() -> Vec<String> {
    vec![
        t!("comandos-espera-tela"),
        t!("comandos-espera-texto"),
        t!("comandos-espera-logo"),
    ]
}

/// A origem que o texto da lista de escolha diz.
fn origem_do_rotulo(texto: &str) -> OrigemDaSenha {
    match id_do_rotulo(texto) {
        Some(id) => OrigemDaSenha::Cofre { id },
        None => OrigemDaSenha::Conexao,
    }
}

impl Tela {
    /// Abre o editor para o nó escolhido, com o que a linha "Comandos" tem agora.
    pub(super) fn abrir_comandos(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let est = &self.editor_comandos;
        let atuais = self.campos.comandos.get_untracked();
        est.alvo.set(Some(id));
        est.opcoes_modo.set(vec![
            self.tree.with_untracked(|t| rotulo_de_herdar(t, id)),
            t!("comandos-modo-proprios"),
            t!("comandos-modo-nenhum"),
        ]);
        est.escolher_modo(atuais.modo);
        est.reconectar.set(atuais.reconectar);
        est.rascunho.set(atuais.passos);
        est.refazer_linhas();
        let chave = est.proxima_chave.get();
        est.proxima_chave.set(chave + 1);
        est.abertura.set(vec![chave]);
        est.aberto.set(true);
    }

    /// Mostra na linha "Comandos" os comandos do grupo escolhido (numa conexão, quem põe é o
    /// `Campos::carregar`).
    pub(super) fn sincronizar_comandos_do_grupo(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let comandos = self.tree.with_untracked(|t| match t.find(id) {
            Some(Node::Group(g)) => Some(g.comandos.clone()),
            _ => None,
        });
        if let Some(comandos) = comandos
            && self.campos.comandos.get_untracked() != comandos
        {
            self.campos.comandos.set(comandos);
        }
    }

    /// Passa a linha "Comandos" para o grupo escolhido e grava. Lê o signal com rastreamento:
    /// serve a um efeito.
    pub(super) fn guardar_comandos_do_grupo(&self) {
        let comandos = self.campos.comandos.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let muda = self.tree.with_untracked(|t| match t.find(id) {
                Some(Node::Group(g)) => g.comandos != comandos,
                _ => false,
            });
            if !muda {
                return;
            }
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id) {
                    g.comandos = comandos;
                }
            });
            self.atualizar();
        });
    }

    /// Grava a cópia na linha "Comandos" (sem os comandos vazios) e fecha.
    fn salvar_comandos(&self) {
        let est = &self.editor_comandos;
        let mut passos = est.rascunho.get_untracked();
        passos.retain(|p| match &p.acao {
            AcaoDoPasso::Comando { texto } => !texto.trim().is_empty(),
            AcaoDoPasso::Senha { .. } => true,
        });
        self.campos.comandos.set(ComandosAoConectar {
            modo: est.modo_escolhido(),
            passos,
            reconectar: est.reconectar.get_untracked(),
        });
        est.aberto.set(false);
    }

    /// Acrescenta `novos` no fim (ou no começo, com `no_comeco`) e liga o modo "Próprios".
    fn adicionar_passos(&self, novos: Vec<Passo>, no_comeco: bool) {
        let est = &self.editor_comandos;
        let max = self.max_secs_dos_passos();
        let mut novos = novos;
        for p in &mut novos {
            p.max_secs = max;
        }
        est.rascunho.update(|v| {
            if no_comeco {
                v.splice(0..0, novos);
            } else {
                v.extend(novos);
            }
        });
        est.escolher_modo(ModoComandos::Proprios);
        est.refazer_linhas();
    }

    fn max_secs_dos_passos(&self) -> u32 {
        self.config
            .with_untracked(|c| c.conexoes.comandos_max_secs)
            .max(1)
    }

    fn mover_passo(&self, de: usize, para: usize) {
        let est = &self.editor_comandos;
        est.rascunho.update(|v| {
            if de < v.len() && para < v.len() {
                v.swap(de, para);
            }
        });
        est.refazer_linhas();
    }

    fn remover_passo(&self, n: usize) {
        let est = &self.editor_comandos;
        est.rascunho.update(|v| {
            if n < v.len() {
                v.remove(n);
            }
        });
        est.refazer_linhas();
    }

    /// Troca os passos da cópia pelos da pasta de onde a conexão herda.
    fn copiar_passos_da_pasta(&self) {
        let Some(id) = self.editor_comandos.alvo.get() else {
            return;
        };
        let passos = self.tree.with_untracked(|t| {
            comandos_herdados(t, id)
                .map(|(_, c)| c.passos.clone())
                .unwrap_or_default()
        });
        let est = &self.editor_comandos;
        est.rascunho.set(passos);
        est.escolher_modo(ModoComandos::Proprios);
        est.refazer_linhas();
    }
}

/// Se a pasta de onde o nó herda tem passos (o link "Copiar os da pasta" só aparece então).
fn pasta_tem_passos(tela: &Tela) -> bool {
    let Some(id) = tela.editor_comandos.alvo.get() else {
        return false;
    };
    tela.tree
        .with(|t| comandos_herdados(t, id).is_some_and(|(_, c)| !c.passos.is_empty()))
}

/// Se "Senha da conexão" entra na lista de origens da senha: numa pasta sempre (as conexões
/// dela decidem), numa conexão só se o tipo tem Credencial e Senha (`Campo::Login`). Um passo
/// já salvo com essa origem continua valendo: o rótulo dele entra na lista de qualquer jeito.
fn oferece_senha_da_conexao(tela: &Tela) -> bool {
    let Some(id) = tela.editor_comandos.alvo.get() else {
        return true;
    };
    let e_pasta = tela
        .tree
        .with_untracked(|t| matches!(t.find(id), Some(Node::Group(_))));
    if e_pasta {
        return true;
    }
    let tipo = tela.campos.tipo.get_untracked();
    Kind::TODOS
        .into_iter()
        .find(|k| k.nome() == tipo)
        .is_none_or(|k| Campo::Login.vale(k))
}

/// As opções da origem da senha: "Senha da conexão" (se `oferece`), os itens do cofre
/// (`do_cofre` vem de [`opcoes_de_credencial`], sem o "Perguntar ao conectar" do começo) e, no
/// fim, o valor que o passo já tem se não estiver na lista (item removido, ou "Senha da
/// conexão" gravada num tipo que não a oferece mais).
fn opcoes_da_origem(oferece: bool, do_cofre: Vec<String>, atual: String) -> Vec<String> {
    let mut opcoes = Vec::new();
    if oferece {
        opcoes.push(t!("comandos-senha-da-conexao"));
    }
    opcoes.extend(do_cofre.into_iter().skip(1));
    if !opcoes.contains(&atual) {
        opcoes.push(atual);
    }
    opcoes
}

/// Uma linha da lista: o passo de posição `n` (de 0).
fn linha_do_passo(tela: &Rc<Tela>, n: usize) -> Column {
    let est = &tela.editor_comandos;
    let (rascunho, modo, opcoes_modo) = (est.rascunho, est.modo, est.opcoes_modo);
    let passo = rascunho
        .with_untracked(|v| v.get(n).cloned())
        .unwrap_or_else(|| Passo::comando("", 15));
    let posicao = n + 1;
    let id = |parte: &str| format!("comandos-passo-{posicao}-{parte}");
    let e_senha = matches!(passo.acao, AcaoDoPasso::Senha { .. });
    let espera_original = passo.espera.clone();

    let (texto_inicial, esperado_inicial, senha_inicial) = match (&passo.acao, &passo.espera) {
        (AcaoDoPasso::Comando { texto }, Espera::Texto { texto: esperado }) => {
            (texto.clone(), esperado.clone(), String::new())
        }
        (AcaoDoPasso::Comando { texto }, _) => (texto.clone(), String::new(), String::new()),
        (AcaoDoPasso::Senha { origem, texto }, _) => {
            let rotulo = match origem {
                OrigemDaSenha::Conexao => t!("comandos-senha-da-conexao"),
                OrigemDaSenha::Cofre { id } => {
                    let r = rotulo_da_credencial(&tela.cofre, Some(*id));
                    if id_do_rotulo(&r) == Some(*id) {
                        r
                    } else {
                        t!("comandos-senha-item-removido", id = *id)
                    }
                }
            };
            (String::new(), texto.clone(), rotulo)
        }
    };
    let texto = signal(texto_inicial);
    let esperado = signal(esperado_inicial);
    let senha = signal(senha_inicial);
    let espera = signal(rotulo_da_espera(&passo.espera));
    let max = signal(f64::from(passo.max_secs));

    // A linha escreve no passo dela; a estrutura (as chaves) não muda por isso.
    effect(move || {
        let max_secs = max.get().round().clamp(1.0, MAX_MAXIMO) as u32;
        let novo = if e_senha {
            Passo {
                acao: AcaoDoPasso::Senha {
                    origem: origem_do_rotulo(&senha.get()),
                    texto: esperado.get(),
                },
                espera: espera_original.clone(),
                max_secs,
            }
        } else {
            let escolhida = espera.get();
            let opcoes = rotulos_da_espera();
            Passo {
                acao: AcaoDoPasso::Comando { texto: texto.get() },
                espera: if escolhida == opcoes[1] {
                    Espera::Texto {
                        texto: esperado.get(),
                    }
                } else if escolhida == opcoes[2] {
                    Espera::Logo
                } else {
                    Espera::TelaParar
                },
                max_secs,
            }
        };
        untrack(|| {
            if rascunho.with_untracked(|v| v.get(n).is_some_and(|p| *p != novo)) {
                rascunho.update(|v| v[n] = novo);
            }
        });
    });

    let proprios = move || {
        let atual = modo.get();
        opcoes_modo.with(|o| o.get(1) == Some(&atual))
    };
    let mostra_esperado = {
        let opcao_texto = t!("comandos-espera-texto");
        move || e_senha || espera.get() == opcao_texto
    };

    let primeira = {
        let linha = Row::new().gap(8).child(
            Text::new(posicao.to_string())
                .id(id("numero"))
                .secondary()
                .width(24),
        );
        let linha = if e_senha {
            let atual = senha.get_untracked();
            let opcoes = opcoes_da_origem(
                oferece_senha_da_conexao(tela),
                opcoes_de_credencial(&tela.cofre),
                atual,
            );
            linha
                .child(Text::new(t!("comandos-enviar-a-senha")).width(120))
                .child(
                    ComboBox::bind(senha, opcoes)
                        .id(id("senha"))
                        .disabled(move || !proprios())
                        .grow(1),
                )
        } else {
            let sugerir = {
                let (tree, recentes, envio) = (
                    tela.tree,
                    tela.recentes,
                    tela.janelas.principal().envio.historico,
                );
                move |digitado: &str| {
                    tree.with_untracked(|t| {
                        recentes.with_untracked(|r: &Recentes| {
                            envio.with_untracked(|h| sugestoes(t, r.lista(), h, digitado))
                        })
                    })
                }
            };
            linha.child(
                AutoSuggestBox::bind(texto, sugerir)
                    .id(id("texto"))
                    .placeholder(t!("comandos-placeholder-comando"))
                    .disabled(move || !proprios())
                    .grow(1),
            )
        };
        let (t_sobe, t_desce, t_remove) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        linha
            .child(
                Button::icon_only(Symbol::ArrowUp)
                    .id(id("subir"))
                    .label(t!("comandos-subir"))
                    .disabled(move || !proprios() || n == 0)
                    .on_click(move || t_sobe.mover_passo(n, n - 1)),
            )
            .child(
                Button::icon_only(Symbol::ArrowDown)
                    .id(id("descer"))
                    .label(t!("comandos-descer"))
                    .disabled(move || !proprios() || n + 1 >= rascunho.with(Vec::len))
                    .on_click(move || t_desce.mover_passo(n, n + 1)),
            )
            .child(
                Button::icon_only(Symbol::Delete)
                    .id(id("remover"))
                    .label(t!("comum-remover"))
                    .disabled(move || !proprios())
                    .on_click(move || t_remove.remover_passo(n)),
            )
    };

    let mut segunda = Row::new().gap(8).padding(Insets::new(0.0, 0.0, 0.0, 32.0));
    if !e_senha {
        segunda = segunda.child(
            ComboBox::bind(espera, rotulos_da_espera())
                .id(id("espera"))
                .disabled(move || !proprios())
                .width(240),
        );
    }
    let segunda = segunda.child(
        Edit::bind(esperado)
            .id(id("esperado"))
            .placeholder(t!("comandos-placeholder-esperado"))
            .disabled(move || !proprios())
            .visible(mostra_esperado)
            .grow(1),
    );
    let terceira = Row::new()
        .gap(8)
        .padding(Insets::new(0.0, 0.0, 0.0, 32.0))
        .child(Text::new(t!("comandos-esperar-ate")).caption())
        .child(
            NumberBox::bind(max)
                .id(id("max"))
                .min(1)
                .max(MAX_MAXIMO)
                .precision(0)
                .small_change(1)
                .disabled(move || !proprios())
                .width(110),
        );

    Column::new()
        .gap(8)
        .child(primeira)
        .child(segunda)
        .child(terceira)
}

/// O diálogo do editor.
pub(super) fn dialogo_do_editor(tela: &Rc<Tela>) -> ContentDialog {
    let est = &tela.editor_comandos;
    let (aberto, modo, opcoes_modo, reconectar) =
        (est.aberto, est.modo, est.opcoes_modo, est.reconectar);
    let (chaves, abertura) = (est.chaves, est.abertura);
    let titulo = {
        let t = Rc::clone(tela);
        move || {
            // `alvo` não é um signal: a abertura é que avisa que mudou.
            abertura.with(|_| ());
            let nome = t
                .editor_comandos
                .alvo
                .get()
                .and_then(|id| {
                    t.tree
                        .with(|tree| tree.find(id).map(|n| n.name().to_owned()))
                })
                .unwrap_or_default();
            t!("comandos-editor-titulo", nome = nome)
        }
    };
    let proprios = move || {
        let atual = modo.get();
        opcoes_modo.with(|o| o.get(1) == Some(&atual))
    };

    let lista = {
        let t = Rc::clone(tela);
        For::each(
            chaves,
            |(chave, _)| *chave,
            move |(_, n)| linha_do_passo(&t, n),
        )
    };
    let atalhos = {
        let (t_cmd, t_senha, t_su, t_i, t_pasta) = (
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
        );
        let virar_root = |t: Rc<Tela>, comando: &'static str| {
            move || {
                t.adicionar_passos(
                    vec![
                        Passo::comando(comando, 15),
                        Passo::senha(OrigemDaSenha::Conexao, 15),
                    ],
                    true,
                );
            }
        };
        let pasta = Rc::clone(tela);
        Column::new()
            .gap(8)
            .child(
                Row::new()
                    .gap(8)
                    .child(
                        Button::new(t!("comandos-adicionar"))
                            .id("comandos-adicionar")
                            .icon(Symbol::Add)
                            .on_click(move || {
                                t_cmd.adicionar_passos(vec![Passo::comando("", 15)], false)
                            }),
                    )
                    .child(
                        Button::new(t!("comandos-adicionar-senha"))
                            .id("comandos-adicionar-senha")
                            .icon(Symbol::Add)
                            .on_click(move || {
                                t_senha.adicionar_passos(
                                    vec![Passo::senha(OrigemDaSenha::Conexao, 15)],
                                    false,
                                );
                            }),
                    ),
            )
            .child(
                Row::new()
                    .gap(8)
                    .child(
                        Button::new(t!("comandos-root-su"))
                            .id("comandos-root-su")
                            .on_click(virar_root(t_su, "sudo su -")),
                    )
                    .child(
                        Button::new(t!("comandos-root-i"))
                            .id("comandos-root-i")
                            .on_click(virar_root(t_i, "sudo -i")),
                    ),
            )
            .child(
                Button::new(t!("comandos-copiar-da-pasta"))
                    .id("comandos-copiar-da-pasta")
                    .hyperlink()
                    .visible(move || {
                        // `alvo` não é um signal: a abertura é que avisa que mudou.
                        abertura.with(|_| ());
                        pasta_tem_passos(&pasta)
                    })
                    .on_click(move || t_pasta.copiar_passos_da_pasta()),
            )
    };
    let confirmar = {
        let t = Rc::clone(tela);
        move || t.salvar_comandos()
    };
    ContentDialog::new(titulo, aberto)
        .id("comandos-dialogo")
        .content(
            Column::new()
                .gap(8)
                .width(500)
                .child(Text::new(t!("comandos-modo-titulo")).caption())
                .child(For::each(
                    abertura,
                    |chave| *chave,
                    move |_| {
                        ComboBox::bind(modo, opcoes_modo.get_untracked())
                            .id("comandos-modo")
                            .width(360)
                    },
                ))
                .child(
                    CheckBox::bind(reconectar, t!("comandos-reconectar"))
                        .id("comandos-reconectar")
                        .disabled(move || !proprios()),
                )
                .child(
                    ScrollViewer::new()
                        .height(300)
                        .child(Column::new().id("comandos-lista").gap(12).child(lista)),
                )
                .child(atalhos),
        )
        .primary(t!("comum-salvar"), confirmar)
        .close(t!("comum-cancelar"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn senha_da_conexao_so_entra_na_lista_quando_o_tipo_a_oferece() {
        let cofre = vec![
            "Perguntar ao conectar".to_string(),
            "Chave do servidor".into(),
        ];
        let senha = t!("comandos-senha-da-conexao");
        let sem = opcoes_da_origem(false, cofre.clone(), "Chave do servidor".into());
        assert_eq!(sem, ["Chave do servidor"]);
        let com = opcoes_da_origem(true, cofre.clone(), senha.clone());
        assert_eq!(com, [senha.clone(), "Chave do servidor".to_string()]);
        // Um passo gravado com a origem que o tipo não oferece mais continua na lista.
        let gravado = opcoes_da_origem(false, cofre, senha.clone());
        assert_eq!(gravado, ["Chave do servidor".to_string(), senha]);
    }
}
