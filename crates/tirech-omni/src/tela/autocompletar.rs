//! O autocompletar (Ctrl+Espaço): um Flyout sobre o terminal ativo com os comandos que cabem na
//! aba ([`crate::autocompletar`]). O campo de cima filtra, ↓ desce para a lista, e Enter (ou o
//! clique duplo) **insere o comando na linha do terminal, sem Enter**. O Tab do shell continua
//! com o shell.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::request_focus;
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::Forward;

use super::Tela;
use super::ia_comando::{aceita_colar_em_bloco, id_do_terminal};
use crate::autocompletar::{self, Sistema, Sugestao, TunelVisto};
use crate::janelas::Janela;
use crate::modelo::{AcaoDoPasso, Kind};
use crate::shell_na_rede::Porta;
use crate::{envio, t, texto_limpo};

/// O que o Flyout guarda entre uma abertura e outra.
pub(super) struct EstadoAutocompletar {
    /// O texto do filtro.
    filtro: Signal<String>,
    /// As linhas da lista, já filtradas.
    linhas: Signal<Vec<DataGridRow>>,
    /// A linha escolhida (a chave é a posição em `vistas`).
    escolhida: Signal<BTreeSet<String>>,
    /// Tudo o que a aba tem para sugerir, montado ao abrir.
    todas: RefCell<Vec<Sugestao>>,
    /// O que a lista mostra agora.
    vistas: RefCell<Vec<Sugestao>>,
    /// Os comandos escolhidos aqui, por conexão, o mais recente primeiro. Só na memória.
    escolhidos: RefCell<HashMap<u64, Vec<String>>>,
    /// A aba de onde o Flyout abriu.
    sessao: Cell<Option<Session>>,
    /// A conexão dessa aba.
    conexao: Cell<Option<u64>>,
    /// A janela do Flyout aberto.
    janela: Cell<Option<Janela>>,
}

impl EstadoAutocompletar {
    pub(super) fn novo() -> Self {
        Self {
            filtro: signal(String::new()),
            linhas: signal(Vec::new()),
            escolhida: signal(BTreeSet::new()),
            todas: RefCell::default(),
            vistas: RefCell::default(),
            escolhidos: RefCell::default(),
            sessao: Cell::new(None),
            conexao: Cell::new(None),
            janela: Cell::new(None),
        }
    }

    /// Refaz a lista com o filtro de agora e escolhe a primeira linha.
    fn filtrar(&self) {
        let vistas = self
            .filtro
            .with_untracked(|f| autocompletar::filtrar(&self.todas.borrow(), f));
        let linhas = vistas
            .iter()
            .enumerate()
            .map(|(i, s)| {
                DataGridRow::new(i.to_string())
                    .cell(s.comando.clone(), CellValue::Text(s.comando.clone()))
                    .cell(s.origem.clone(), CellValue::Text(s.origem.clone()))
            })
            .collect();
        let primeira = if vistas.is_empty() {
            BTreeSet::new()
        } else {
            BTreeSet::from(["0".to_owned()])
        };
        *self.vistas.borrow_mut() = vistas;
        self.linhas.set(linhas);
        self.escolhida.set(primeira);
    }
}

/// A pasta de dados do usuário (`XDG_DATA_HOME`, ou `~/.local/share`), onde o fish guarda o
/// histórico.
fn pasta_de_dados(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("share"))
}

impl Tela {
    /// Os túneis que um shell local enxerga: os que abriu pela rede do perfil e os `-L` e `-D`
    /// das outras abas SSH abertas.
    fn tuneis_vistos(&self, sessao: Session) -> Vec<TunelVisto> {
        let mut tuneis: Vec<TunelVisto> = sessao
            .held::<Vec<Porta>, _>(|portas| portas.iter().map(Porta::vista).collect())
            .unwrap_or_default();
        for (_, outra) in self.sessoes_das_janelas(None) {
            if outra == sessao || outra.closed() {
                continue;
            }
            let Some(forwards) = outra.ssh_forwards() else {
                continue;
            };
            for status in forwards.list() {
                let Some(escuta) = status
                    .listening
                    .as_deref()
                    .and_then(|l| l.parse::<SocketAddr>().ok())
                else {
                    continue;
                };
                let destino = match status.forward {
                    Forward::Local { host, port, .. } => Some((host, port)),
                    Forward::Dynamic { .. } => None,
                    _ => continue,
                };
                tuneis.push(TunelVisto {
                    escuta,
                    destino,
                    aba: Some(outra.title()),
                });
            }
        }
        tuneis
    }

    /// Abre o Flyout no terminal ativo de `janela`, com o que a aba tem para sugerir.
    pub(super) fn abrir_autocompletar(self: &Rc<Self>, janela: Janela) {
        let Some(sessao) = self.sessao_ativa_de(janela) else {
            self.avisar(
                InfoBarSeverity::Informational,
                t!("autocompletar-sem-terminal"),
            );
            return;
        };
        let conexao = self.conexao_da_aba(&sessao.key_str(), sessao);
        let local = conexao.kind == Kind::Local;
        let a = &self.autocompletar;
        let escolhidos = a
            .escolhidos
            .borrow()
            .get(&conexao.id)
            .cloned()
            .unwrap_or_default();
        let ao_conectar: Vec<String> = self.tree.with_untracked(|t| {
            crate::comandos::comandos_efetivos(t, conexao.id)
                .map(|c| {
                    c.passos
                        .iter()
                        .filter_map(|p| match &p.acao {
                            AcaoDoPasso::Comando { texto } => Some(texto.clone()),
                            AcaoDoPasso::Senha { .. } => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        });
        let barra = janela.envio.historico.get_untracked();
        let (tuneis, shells) = if local {
            let tuneis =
                autocompletar::dos_tuneis(&self.tuneis_vistos(sessao), Sistema::atual(), &|p| {
                    crate::navegador::no_path(p).is_some()
                });
            let shells = crate::armazenamento::pasta_pessoal()
                .map(|home| autocompletar::dos_shells(&home, &pasta_de_dados(&home)))
                .unwrap_or_default();
            (tuneis, shells)
        } else {
            (Vec::new(), Vec::new())
        };
        *a.todas.borrow_mut() = autocompletar::juntar([
            tuneis,
            autocompletar::da_lista(
                escolhidos.iter().map(String::as_str),
                &t!("autocompletar-origem-escolhidos"),
            ),
            autocompletar::da_lista(
                barra.iter().map(String::as_str),
                &t!("autocompletar-origem-envio"),
            ),
            autocompletar::da_lista(
                ao_conectar.iter().map(String::as_str),
                &t!("autocompletar-origem-ao-conectar"),
            ),
            shells,
        ]);
        a.sessao.set(Some(sessao));
        a.conexao.set(Some(conexao.id));
        a.janela.set(Some(janela));
        a.filtro.set(String::new());
        a.filtrar();
        janela.autocompletar.set(true);
        request_focus("autocompletar-filtro");
    }

    /// Fecha o Flyout e devolve o foco ao terminal.
    fn fechar_autocompletar(&self) {
        let a = &self.autocompletar;
        if let Some(janela) = a.janela.get() {
            janela.autocompletar.set(false);
        }
        if let Some(sessao) = a.sessao.get() {
            request_focus(id_do_terminal(&sessao.key_str()));
        }
    }

    /// Põe na linha do terminal o comando da linha `chave` da lista, sem Enter, e o guarda nos
    /// escolhidos da conexão.
    fn inserir_autocompletar(&self, chave: &str) {
        let a = &self.autocompletar;
        let Some(sugestao) = chave
            .parse::<usize>()
            .ok()
            .and_then(|i| a.vistas.borrow().get(i).cloned())
        else {
            return;
        };
        let Some(sessao) = a.sessao.get() else {
            return;
        };
        if let Some(mut bytes) =
            texto_limpo::para_inserir(&sugestao.comando, aceita_colar_em_bloco(&sessao))
        {
            // O espaço do fim fica: o `-u `, o `curl --proxy … ` esperam o resto.
            if sugestao.comando.ends_with(' ') {
                bytes.push(b' ');
            }
            sessao.send(&bytes);
        }
        if let Some(id) = a.conexao.get() {
            envio::lembrar(
                a.escolhidos.borrow_mut().entry(id).or_default(),
                &sugestao.comando,
            );
        }
        self.fechar_autocompletar();
    }

    /// O Enter no campo do filtro: insere a linha escolhida.
    fn inserir_a_escolhida(&self) {
        let chave = self
            .autocompletar
            .escolhida
            .with_untracked(|e| e.iter().next().cloned());
        if let Some(chave) = chave {
            self.inserir_autocompletar(&chave);
        }
    }
}

/// O Flyout `autocompletar`, preso a uma coluna sem altura no fim da área de terminais de
/// `janela`.
pub(super) fn flyout(tela: &Rc<Tela>, janela: Janela) -> impl View {
    let a = &tela.autocompletar;
    let filtro = a.filtro;
    {
        let t = Rc::clone(tela);
        effect(move || {
            filtro.with(|_| ());
            untrack(|| t.autocompletar.filtrar());
        });
    }
    let lista = {
        let t = Rc::clone(tela);
        DataGrid::bind(a.linhas, a.escolhida)
            .id("autocompletar-lista")
            .height(280)
            .columns([
                DataGridColumn::new("comando", t!("autocompletar-coluna-comando"))
                    .width(ColumnWidth::Star(3.0))
                    .not_sortable(),
                DataGridColumn::new("origem", t!("autocompletar-coluna-origem"))
                    .width(ColumnWidth::Star(2.0))
                    .not_sortable(),
            ])
            .empty_text(t!("autocompletar-vazio"))
            .on_invoke(move |chave| t.inserir_autocompletar(&chave))
    };
    let conteudo = Column::new()
        .gap(8)
        .width(640)
        .child(
            Edit::bind(filtro)
                .id("autocompletar-filtro")
                .placeholder(t!("autocompletar-filtro")),
        )
        .child(lista)
        .child(
            Text::new(t!("autocompletar-dica"))
                .id("autocompletar-dica")
                .caption()
                .secondary(),
        )
        // A lista trata o Enter e as setas quando tem o foco; no campo, sobem até aqui.
        .accelerator(Shortcut::key(Key::Enter), {
            let t = Rc::clone(tela);
            move || t.inserir_a_escolhida()
        })
        .accelerator(Shortcut::key(Key::Down), || {
            request_focus("autocompletar-lista")
        });
    // A camada se ancora no widget de cima: uma coluna sem altura, sem nada que a pessoa clique.
    Column::new().height(0).child(
        Flyout::new(conteudo)
            .id("autocompletar")
            .open(janela.autocompletar),
    )
}
