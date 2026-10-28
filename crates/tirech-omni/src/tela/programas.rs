//! Programas gráficos sem aba (M37): o virt-manager aberto no servidor por X11, o virt-manager
//! desta máquina ligado ao libvirt do servidor e qualquer aplicativo gráfico remoto. Cada um
//! ganha um aviso no alto da janela enquanto roda; fechar o aviso fecha o programa.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::RemoteAppStatus;

use super::{Lugar, Pedido, Recusa, Tela};
use crate::abrir::{Lancamento, precisa_de_senha};
use crate::recentes::Alvo as AlvoRecente;
use crate::t;

/// O comando do virt-manager quando a configuração está vazia.
const VIRT_MANAGER: &str = "virt-manager --no-fork";

/// Qual programa um pedido (ou a senha que ele espera) lança.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QualPrograma {
    /// O virt-manager do servidor, com as janelas aqui pelo X11.
    VirtManager,
    /// O virt-manager desta máquina, ligado ao libvirt do servidor por um túnel.
    VirtManagerLocal,
    /// Um aplicativo gráfico remoto, pelo comando que a pessoa digitou.
    Aplicativo,
    /// Um comando remoto com a saída numa aba.
    AplicativoEmAba,
}

/// O que o aviso de um programa mostra.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LinhaPrograma {
    /// O número do programa (chave do aviso).
    pub numero: u64,
    /// A gravidade: informação enquanto roda, erro se falhou.
    pub severidade: InfoBarSeverity,
    /// O texto.
    pub mensagem: String,
}

/// Um programa lançado.
struct Ativo {
    numero: u64,
    /// "virt-manager em servidor1".
    titulo: String,
    /// O nome do programa, para "não está instalado".
    programa: String,
    /// O nome da conexão.
    conexao: String,
    lancamento: Lancamento,
}

/// O que a tela guarda dos programas gráficos.
pub(super) struct EstadoProgramas {
    ativos: RefCell<Vec<Ativo>>,
    /// O que os avisos mostram.
    pub linhas: Signal<Vec<LinhaPrograma>>,
    proximo: Cell<u64>,
    /// Há uma releitura agendada.
    vigiando: Cell<bool>,
    /// Os comandos que esperam a senha da conexão, pelo id dela.
    comando_pendente: RefCell<HashMap<u64, String>>,
    /// O diálogo "Executar aplicativo gráfico remoto" está aberto.
    pub executar_aberto: Signal<bool>,
    /// O comando digitado no diálogo.
    pub executar_comando: Signal<String>,
    /// Mostrar a saída numa aba em vez de só abrir as janelas.
    pub executar_aba: Signal<bool>,
    /// A conexão do diálogo.
    executar_conexao: Cell<Option<u64>>,
    /// O nome da conexão do diálogo (para o título).
    pub executar_nome: Signal<String>,
}

impl EstadoProgramas {
    pub(super) fn novo() -> Self {
        Self {
            ativos: RefCell::default(),
            linhas: signal(Vec::new()),
            proximo: Cell::new(0),
            vigiando: Cell::new(false),
            comando_pendente: RefCell::default(),
            executar_aberto: signal(false),
            executar_comando: signal(String::new()),
            executar_aba: signal(false),
            executar_conexao: Cell::new(None),
            executar_nome: signal(String::new()),
        }
    }
}

impl Ativo {
    /// O aviso do programa, ou `None` se ele acabou bem e o aviso some.
    fn linha(&self) -> Option<LinhaPrograma> {
        let status = self.lancamento.status();
        let (severidade, mensagem) = match status {
            RemoteAppStatus::Connecting => (
                InfoBarSeverity::Informational,
                t!("programas-abrindo", titulo = self.titulo.as_str()),
            ),
            RemoteAppStatus::Running => (InfoBarSeverity::Informational, self.titulo.clone()),
            RemoteAppStatus::Exited(Some(0)) => return None,
            RemoteAppStatus::Exited(codigo) => (
                InfoBarSeverity::Error,
                explicar(
                    &self.programa,
                    &self.conexao,
                    &self.titulo,
                    &self.lancamento.stderr(),
                    None,
                    codigo,
                ),
            ),
            RemoteAppStatus::Failed(motivo) => (
                InfoBarSeverity::Error,
                explicar(
                    &self.programa,
                    &self.conexao,
                    &self.titulo,
                    &self.lancamento.stderr(),
                    Some(&motivo),
                    None,
                ),
            ),
        };
        Some(LinhaPrograma {
            numero: self.numero,
            severidade,
            mensagem,
        })
    }

    /// Ainda pode mudar de estado.
    fn em_andamento(&self) -> bool {
        matches!(
            self.lancamento.status(),
            RemoteAppStatus::Connecting | RemoteAppStatus::Running
        )
    }
}

/// A última linha não vazia do texto.
fn ultima_linha(texto: &str) -> &str {
    texto
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
}

/// O que deu errado, escrito para a pessoa: o programa que não existe, o X11 que não achou
/// servidor, o libvirt que negou, ou a última linha do que o programa disse.
fn explicar(
    programa: &str,
    conexao: &str,
    titulo: &str,
    stderr: &str,
    falha: Option<&str>,
    codigo: Option<u32>,
) -> String {
    let texto = stderr.to_lowercase();
    if codigo == Some(127) || texto.contains("command not found") || texto.contains(": not found") {
        return t!(
            "programas-nao-instalado",
            programa = programa,
            conexao = conexao
        );
    }
    if texto.contains("cannot open display")
        || texto.contains("can't open display")
        || texto.contains("unable to open display")
        || texto.contains("could not connect to display")
    {
        return t!("programas-sem-x11");
    }
    if texto.contains("authentication unavailable")
        || texto.contains("permission denied")
        || texto.contains("polkit")
    {
        return t!("programas-sem-permissao-libvirt");
    }
    let ultima = ultima_linha(stderr);
    match (falha, codigo) {
        (Some(motivo), _) => t!(
            "programas-nao-consegui-abrir",
            titulo = titulo,
            motivo = motivo
        ),
        (None, Some(codigo)) if ultima.is_empty() => {
            t!(
                "programas-terminou-com-codigo",
                titulo = titulo,
                codigo = i64::from(codigo)
            )
        }
        (None, Some(codigo)) => t!(
            "programas-terminou-com-codigo-e-saida",
            titulo = titulo,
            codigo = i64::from(codigo),
            ultima = ultima
        ),
        (None, None) if ultima.is_empty() => t!("programas-terminou", titulo = titulo),
        (None, None) => t!(
            "programas-terminou-com-saida",
            titulo = titulo,
            ultima = ultima
        ),
    }
}

/// O primeiro termo do comando, sem a pasta: o nome do programa.
fn nome_do_programa(comando: &str) -> String {
    let primeiro = comando.split_whitespace().next().unwrap_or("");
    primeiro.rsplit('/').next().unwrap_or(primeiro).to_owned()
}

impl Tela {
    /// Pede o programa da conexão `id`; sem a senha dela, o diálogo de senha vem antes.
    pub(super) fn iniciar_programa(&self, id: u64, qual: QualPrograma) {
        let Some(c) = self.tree.with_untracked(|t| t.conexao_para_abrir(id)) else {
            return;
        };
        let senhas = self.senhas_do_cofre(&c);
        if precisa_de_senha(&c) && senhas.conexao.is_none() {
            self.pendentes.update(|p| {
                p.push(Pedido {
                    id,
                    lugar: Lugar::default(),
                    recusa: Recusa::Nao,
                    registrar: false,
                    arquivos: false,
                    programa: Some(qual),
                });
            });
        } else {
            self.lancar_programa_agora(&c, senhas, qual);
        }
    }

    /// O item "Abrir virt-manager".
    pub(super) fn abrir_virt_manager(&self, id: u64) {
        self.iniciar_programa(id, QualPrograma::VirtManager);
    }

    /// O item "Abrir virt-manager local".
    pub(super) fn abrir_virt_manager_local(&self, id: u64) {
        self.iniciar_programa(id, QualPrograma::VirtManagerLocal);
    }

    /// Roda `comando` no servidor da conexão `id` (um recente, ou o diálogo), sem aba.
    pub(super) fn executar_aplicativo(&self, id: u64, comando: String, registrar: bool) {
        let comando = comando.trim().to_owned();
        if comando.is_empty() {
            return;
        }
        let Some(nome) = self
            .tree
            .with_untracked(|t| t.connection(id).map(|c| c.name.clone()))
        else {
            return;
        };
        if registrar {
            let titulo = t!(
                "programas-programa-em-conexao",
                programa = nome_do_programa(&comando),
                conexao = nome.as_str()
            );
            self.recentes.update(|r| {
                r.registrar(
                    AlvoRecente::Aplicativo {
                        conexao: id,
                        comando: comando.clone(),
                    },
                    &titulo,
                    &comando,
                );
            });
        }
        self.programas
            .comando_pendente
            .borrow_mut()
            .insert(id, comando);
        self.iniciar_programa(id, QualPrograma::Aplicativo);
    }

    /// Lança o programa com as senhas já na mão.
    pub(super) fn lancar_programa_agora(
        &self,
        c: &Connection,
        senhas: crate::abrir::Senhas,
        qual: QualPrograma,
    ) {
        let efetiva = self.conexao_efetiva(c);
        let (lancamento, titulo, programa) = match qual {
            QualPrograma::VirtManager => {
                let comando = self
                    .config
                    .with_untracked(|cfg| cfg.x11.virt_manager.trim().to_owned());
                let comando = if comando.is_empty() {
                    VIRT_MANAGER.to_owned()
                } else {
                    comando
                };
                (
                    self.opener.lancar(&efetiva, senhas, &comando),
                    t!(
                        "programas-programa-em-conexao",
                        programa = "virt-manager",
                        conexao = c.name.as_str()
                    ),
                    "virt-manager".to_owned(),
                )
            }
            QualPrograma::VirtManagerLocal => (
                self.opener.abrir_virt_local(&efetiva, senhas),
                t!("programas-virt-manager-local", conexao = c.name.as_str()),
                "virt-manager".to_owned(),
            ),
            QualPrograma::Aplicativo => {
                let Some(comando) = self.programas.comando_pendente.borrow_mut().remove(&c.id)
                else {
                    return;
                };
                let programa = nome_do_programa(&comando);
                (
                    self.opener.lancar(&efetiva, senhas, &comando),
                    t!(
                        "programas-programa-em-conexao",
                        programa = programa.as_str(),
                        conexao = c.name.as_str()
                    ),
                    programa,
                )
            }
            QualPrograma::AplicativoEmAba => {
                let Some(comando) = self.programas.comando_pendente.borrow_mut().remove(&c.id)
                else {
                    return;
                };
                let n = self.contador.get_untracked() + 1;
                self.contador.set(n);
                let sessao = self.opener.abrir_comando(&efetiva, senhas, n, &comando);
                let lugar = self.fixar(Lugar::default());
                self.entra_na_grade(sessao, lugar);
                self.mostrar_terminais();
                return;
            }
        };
        let numero = self.programas.proximo.get() + 1;
        self.programas.proximo.set(numero);
        self.programas.ativos.borrow_mut().push(Ativo {
            numero,
            titulo,
            programa,
            conexao: c.name.clone(),
            lancamento,
        });
        self.refazer_linhas_dos_programas();
        self.vigiar_programas();
    }

    /// Fecha o programa e tira o aviso dele.
    pub(super) fn fechar_programa(&self, numero: u64) {
        self.programas.ativos.borrow_mut().retain(|a| {
            let fica = a.numero != numero;
            if !fica {
                a.lancamento.fechar();
            }
            fica
        });
        self.refazer_linhas_dos_programas();
    }

    /// Refaz os avisos; o programa que acabou bem sai da lista.
    fn refazer_linhas_dos_programas(&self) {
        let mut linhas = Vec::new();
        self.programas
            .ativos
            .borrow_mut()
            .retain(|a| match a.linha() {
                Some(linha) => {
                    linhas.push(linha);
                    true
                }
                None => false,
            });
        if self.programas.linhas.with_untracked(|v| *v != linhas) {
            self.programas.linhas.set(linhas);
        }
    }

    /// Relê os programas a cada segundo enquanto algum ainda anda.
    fn vigiar_programas(&self) {
        if self.programas.vigiando.replace(true) {
            return;
        }
        self.agendar_programas();
    }

    fn agendar_programas(&self) {
        let eu = self.eu.clone();
        self.alarmes.depois(Duration::from_secs(1), move || {
            let Some(t) = eu.upgrade() else {
                return;
            };
            t.refazer_linhas_dos_programas();
            if t.programas.ativos.borrow().iter().any(Ativo::em_andamento) {
                t.agendar_programas();
            } else {
                t.programas.vigiando.set(false);
            }
        });
    }

    /// Abre o diálogo "Executar aplicativo gráfico remoto" para a conexão `id`.
    pub(super) fn abrir_executar(&self, id: u64) {
        let Some(nome) = self
            .tree
            .with_untracked(|t| t.connection(id).map(|c| c.name.clone()))
        else {
            return;
        };
        let est = &self.programas;
        est.executar_conexao.set(Some(id));
        est.executar_nome.set(nome);
        est.executar_comando.set(String::new());
        est.executar_aba.set(false);
        est.executar_aberto.set(true);
    }

    /// Os comandos que a conexão do diálogo já executou, os mais recentes primeiro, que contêm
    /// `procurado`.
    fn comandos_recentes(&self, procurado: &str) -> Vec<String> {
        let Some(id) = self.programas.executar_conexao.get() else {
            return Vec::new();
        };
        let procurado = procurado.trim().to_lowercase();
        self.recentes.with_untracked(|r| {
            r.lista()
                .iter()
                .filter_map(|i| match &i.alvo {
                    AlvoRecente::Aplicativo { conexao, comando } if *conexao == id => {
                        Some(comando.clone())
                    }
                    _ => None,
                })
                .filter(|c| c.to_lowercase().contains(&procurado))
                .collect()
        })
    }

    /// O "Executar" do diálogo.
    fn confirmar_executar(&self) {
        let est = &self.programas;
        let Some(id) = est.executar_conexao.get() else {
            return;
        };
        let comando = est.executar_comando.get_untracked().trim().to_owned();
        if comando.is_empty() {
            return;
        }
        if est.executar_aba.get_untracked() {
            est.comando_pendente.borrow_mut().insert(id, comando);
            self.iniciar_programa(id, QualPrograma::AplicativoEmAba);
        } else {
            self.executar_aplicativo(id, comando, true);
        }
    }
}

use crate::modelo::Connection;

/// O diálogo "Executar aplicativo gráfico remoto".
pub(super) fn dialogo_executar(tela: &Rc<Tela>) -> ContentDialog {
    let est = &tela.programas;
    let (aberto, comando, aba, nome) = (
        est.executar_aberto,
        est.executar_comando,
        est.executar_aba,
        est.executar_nome,
    );
    let (t_sugestoes, t_executar) = (Rc::clone(tela), Rc::clone(tela));
    ContentDialog::new(
        move || t!("programas-executar-em", nome = nome.get()),
        aberto,
    )
    .id("executar-dialogo")
    .content(
        Column::new()
            .gap(8)
            .child(
                Text::new(t!("programas-o-aplicativo-roda-no-servidor-e"))
                    .secondary()
                    .wrap(),
            )
            .child(Text::new(t!("programas-comando")).caption())
            .child(
                AutoSuggestBox::bind(comando, move |procurado: &str| {
                    untrack(|| t_sugestoes.comandos_recentes(procurado))
                })
                .id("executar-comando")
                .placeholder("gitk"),
            )
            .child(
                CheckBox::bind(aba, t!("programas-mostrar-a-saida-numa-aba")).id("executar-aba"),
            ),
    )
    .primary(t!("programas-executar"), move || {
        t_executar.confirmar_executar()
    })
    .close(t!("comum-cancelar"))
}

/// Os avisos dos programas em andamento, no alto da janela.
pub(super) fn avisos(tela: &Rc<Tela>) -> impl View {
    let linhas = tela.programas.linhas;
    let t = Rc::clone(tela);
    For::each(
        linhas,
        |l: &LinhaPrograma| (l.numero, l.mensagem.clone()),
        move |l: LinhaPrograma| {
            let (t, numero) = (Rc::clone(&t), l.numero);
            InfoBar::new(l.severidade)
                .id(format!("programa-{}", l.numero))
                .message(l.mensagem)
                .on_close(move || t.fechar_programa(numero))
        },
    )
}
