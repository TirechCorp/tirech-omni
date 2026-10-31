//! Os remotos do rclone na casca (M33): a subpágina "Remotos do rclone" da página Arquivos, o
//! abrir de um remoto (do rclone ou da árvore), o "Pôr na árvore" e as opções do binário.
//!
//! Subir o `rcd` leva até 10 s, então tudo que fala com o rclone roda numa thread e volta por
//! um [`Live`]: o efeito de [`Tela::ligar_rclone`] aplica as chegadas na thread da tela.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use tomni_ui::files::Live;
use tomni_ui::files::rclone::config::{ConfigStep, Provider, RemoteInfo};
use tomni_ui::prelude::*;

use super::Tela;
use super::rclone_form::{self, Formulario, Retomar};
use crate::abrir::com_rede_pronta;
use crate::modelo::{Connection, Kind, RcloneOpcoes};
use crate::rclone::{self, Falha, OpcoesRclone, Rcds};
use crate::redes::RedeResolvida;
use crate::t;

/// O remoto que uma abertura quer mostrar numa aba.
#[derive(Clone, Debug)]
pub(super) struct Alvo {
    /// O título da aba.
    pub titulo: String,
    /// A conexão da árvore de onde veio (`None`: aberto direto da lista de remotos).
    pub conexao: Option<u64>,
    /// `remoto:caminho`.
    pub spec: String,
    /// A pasta em que o painel remoto abre.
    pub remoto: Option<String>,
    /// A pasta em que o painel local abre.
    pub local: Option<String>,
    /// A tela vai para a página Arquivos.
    pub mostrar: bool,
    /// A rede do perfil da conexão (M34): o `rcd` desta aba sobe com o proxy dela.
    pub rede: Option<RedeResolvida>,
}

/// O que uma thread de trabalho devolveu.
// Chega uma mensagem por vez, de uma ação da pessoa: pôr o `Alvo` numa `Box` mudaria cada uso
// sem ganho que se note.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub(super) enum Chegada {
    /// A lista de remotos (e o aviso de sucesso da operação que a pediu, se houve).
    Remotos(Result<Vec<RemoteInfo>, Falha>, Option<String>),
    /// O `rcd` subiu (ou não) para abrir o alvo.
    Pronto(Alvo, Result<(), Falha>),
    /// Os provedores do formulário (e, ao editar, os valores do remoto).
    Provedores(
        Option<String>,
        Result<(Vec<Provider>, BTreeMap<String, String>), Falha>,
    ),
    /// O passo seguinte da criação ou da edição de um remoto.
    Passo {
        nome: String,
        criando: bool,
        cancelado: bool,
        resultado: Result<ConfigStep, Falha>,
    },
}

/// O que a subpágina guarda.
pub(super) struct EstadoRclone {
    /// A subpágina está na frente da página Arquivos.
    pub aberta: Signal<bool>,
    /// A última falha de instalação (binário ausente ou antigo): o `InfoBar` de instalação.
    pub falha: Signal<Option<Falha>>,
    /// Os remotos configurados.
    pub remotos: Signal<Vec<RemoteInfo>>,
    /// A linha escolhida na grade (o nome do remoto).
    pub selecao: Signal<BTreeSet<String>>,
    /// As linhas da grade.
    pub linhas: Signal<Vec<DataGridRow>>,
    /// Uma thread está falando com o rclone.
    pub ocupado: Signal<bool>,
    /// O aviso da subpágina: se é erro e o texto.
    pub aviso: Signal<Option<(bool, String)>>,
    /// As respostas das threads.
    pub chegadas: Live<Vec<Chegada>>,
    /// A confirmação de apagar um remoto.
    pub apagar_aberto: Signal<bool>,
    /// O diálogo "Pôr na árvore".
    pub arvore_aberto: Signal<bool>,
    /// O nome da conexão que o diálogo cria.
    pub arvore_nome: Signal<String>,
    /// O diálogo "Opções do rclone".
    pub opcoes_aberto: Signal<bool>,
    /// O caminho do binário, no diálogo.
    pub opcoes_binario: Signal<String>,
    /// O arquivo de configuração, no diálogo.
    pub opcoes_config: Signal<String>,
    /// O formulário de Novo e Editar e os diálogos dele.
    pub form: Formulario,
}

impl EstadoRclone {
    pub(super) fn novo() -> Self {
        Self {
            aberta: signal(false),
            falha: signal(None),
            remotos: signal(Vec::new()),
            selecao: signal(BTreeSet::new()),
            linhas: signal(Vec::new()),
            ocupado: signal(false),
            aviso: signal(None),
            chegadas: Live::detached(Vec::new()),
            apagar_aberto: signal(false),
            arvore_aberto: signal(false),
            arvore_nome: signal(String::new()),
            opcoes_aberto: signal(false),
            opcoes_binario: signal(String::new()),
            opcoes_config: signal(String::new()),
            form: Formulario::novo(),
        }
    }
}

/// O tipo do rclone com um nome que a pessoa reconhece (`drive` → Google Drive é coisa do
/// formulário; aqui só o texto do rclone, com a primeira letra maiúscula nos conhecidos).
fn linha_do_remoto(r: &RemoteInfo) -> DataGridRow {
    DataGridRow::new(r.name.clone())
        .cell(r.name.clone(), CellValue::Text(r.name.clone()))
        .cell(r.kind.clone(), CellValue::Text(r.kind.clone()))
        .icon(Symbol::Cloud)
}

impl Tela {
    /// O remoto escolhido na grade.
    pub(super) fn remoto_escolhido(&self) -> Option<String> {
        self.rclone
            .selecao
            .with_untracked(|s| s.iter().next().cloned())
    }

    /// Liga o efeito que aplica as chegadas das threads e lê as opções do binário.
    pub(super) fn ligar_rclone(self: &Rc<Self>) {
        self.ligar_formulario();
        // `rclone.binario` e `rclone.arquivo_config`: valem já e a cada mudança (trocar derruba
        // os processos `rcd`, e o próximo pedido sobe outro com as opções novas).
        let (config, opener) = (self.config, Rc::clone(&self.opener));
        effect(move || {
            let opcoes = config.with(|c| OpcoesRclone::da_config(&c.rclone));
            untrack(|| {
                if let Some(rcds) = opener.rcds()
                    && rcds.opcoes() != opcoes
                {
                    rcds.definir_opcoes(opcoes);
                }
            });
        });
        let tela = Rc::clone(self);
        effect(move || {
            if !tela.rclone.chegadas.with(|v| !v.is_empty()) {
                return;
            }
            untrack(|| {
                let mut recebidas = Vec::new();
                tela.rclone.chegadas.update(|v| recebidas.append(v));
                for chegada in recebidas {
                    tela.aplicar_chegada_rclone(chegada);
                }
            });
        });
        let tela = Rc::clone(self);
        effect(move || {
            let remotos = tela.rclone.remotos.get();
            tela.rclone
                .linhas
                .set(remotos.iter().map(linha_do_remoto).collect());
        });
    }

    fn aplicar_chegada_rclone(&self, chegada: Chegada) {
        match chegada {
            Chegada::Remotos(resultado, ok) => {
                self.rclone.ocupado.set(false);
                match resultado {
                    Ok(remotos) => {
                        self.rclone.falha.set(None);
                        self.rclone.remotos.set(remotos);
                        self.rclone.aviso.set(ok.map(|texto| (false, texto)));
                    }
                    Err(falha) => self.falha_do_rclone(falha, Retomar::Listar),
                }
            }
            Chegada::Pronto(alvo, resultado) => match resultado {
                Ok(()) => {
                    self.rclone.falha.set(None);
                    self.abrir_aba_rclone(alvo);
                }
                Err(falha) => {
                    let motivo = falha.texto().to_owned();
                    let pede_senha = matches!(falha, Falha::SenhaDaConfig(_));
                    self.falha_do_rclone(falha, Retomar::Abrir(alvo.clone()));
                    if !pede_senha {
                        self.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "arquivos-nao-foi-possivel-abrir-de",
                                nome = alvo.titulo.as_str(),
                                motivo = motivo.to_string()
                            ),
                        );
                    }
                }
            },
            Chegada::Provedores(editando, resultado) => {
                self.aplicar_provedores(editando, resultado)
            }
            Chegada::Passo {
                nome,
                criando,
                cancelado,
                resultado,
            } => self.aplicar_passo(nome, criando, cancelado, resultado),
        }
    }

    /// Guarda a falha: a de instalação vira o `InfoBar` fixo, a outra um aviso da subpágina.
    pub(super) fn falha_do_rclone(&self, falha: Falha, retomar: Retomar) {
        self.rclone.ocupado.set(false);
        if matches!(falha, Falha::SenhaDaConfig(_)) {
            self.pedir_senha_da_config(retomar);
        } else if falha.de_instalacao() {
            self.rclone.falha.set(Some(falha));
        } else {
            self.rclone
                .aviso
                .set(Some((true, falha.texto().to_owned())));
        }
    }

    /// Roda `trabalho` numa thread com o `Rcds`; o que ele devolve chega pelo efeito.
    pub(super) fn em_thread_rclone(
        &self,
        trabalho: impl FnOnce(Arc<Rcds>) -> Chegada + Send + 'static,
    ) {
        let Some(rcds) = self.opener.rcds() else {
            self.rclone
                .aviso
                .set(Some((true, t!("rclone-tipo-indisponivel"))));
            return;
        };
        let chegadas = self.rclone.chegadas.clone();
        std::thread::spawn(move || {
            let chegada = trabalho(rcds);
            chegadas.update(|v| v.push(chegada));
        });
    }

    /// Lê de novo os remotos do `rclone.conf`.
    pub(super) fn listar_remotos(&self) {
        self.listar_remotos_com(None);
    }

    pub(super) fn listar_remotos_com(&self, ok: Option<String>) {
        self.rclone.ocupado.set(true);
        self.em_thread_rclone(move |rcds| Chegada::Remotos(listar(&rcds), ok));
    }

    /// Abre a subpágina na página Arquivos e lê os remotos.
    pub(super) fn abrir_remotos_do_rclone(&self) {
        self.ir_para("arquivos");
        self.rclone.aberta.set(true);
        self.listar_remotos();
    }

    /// Abre o remoto `nome` (e `caminho`, se vier) numa aba da página Arquivos. O `rcd` sobe numa
    /// thread; a aba nasce quando ele está pronto.
    pub(super) fn abrir_remoto(
        &self,
        titulo: String,
        conexao: Option<u64>,
        opcoes: RcloneOpcoes,
        remoto: Option<String>,
        local: Option<String>,
        mostrar: bool,
    ) {
        let rede = conexao
            .and_then(|id| self.tree.with_untracked(|t| t.connection(id).cloned()))
            .and_then(|c| self.rede_resolvida(&c));
        self.abrir_alvo(Alvo {
            titulo,
            conexao,
            spec: opcoes.spec(),
            remoto,
            local,
            mostrar,
            rede,
        });
    }

    /// Sobe o `rcd` numa thread e abre o `alvo` quando ele está pronto.
    pub(super) fn abrir_alvo(&self, alvo: Alvo) {
        self.em_thread_rclone(move |rcds| {
            let resultado = match &alvo.rede {
                // A rede sobe antes do `rcd`; o erro dela vai para o aviso como os outros.
                Some(rede) => rede
                    .preparar(&mut |_| {})
                    .map_err(Falha::Outra)
                    .and_then(|()| {
                        rcds.obter(rede.url_do_proxy().as_deref())
                            .map(drop)
                            .map_err(|e| Falha::de(&e))
                    }),
                None => rcds.obter(None).map(drop).map_err(|e| Falha::de(&e)),
            };
            Chegada::Pronto(alvo, resultado)
        });
    }

    /// O `rcd` está pronto: cria a aba.
    fn abrir_aba_rclone(&self, alvo: Alvo) {
        let Some(rcds) = self.opener.rcds() else {
            return;
        };
        let servidor = com_rede_pronta(
            rclone::abridor(
                Arc::clone(&rcds),
                alvo.spec.clone(),
                alvo.rede.as_ref().and_then(RedeResolvida::url_do_proxy),
            ),
            alvo.rede.as_ref(),
        );
        let mut aba = self.nova_aba(
            alvo.titulo.clone(),
            alvo.conexao,
            None,
            alvo.remoto.clone(),
            alvo.local.clone(),
            servidor,
        );
        aba.rclone = Some(crate::arquivos::rclone::AcoesRclone::nova(rcds, alvo.spec));
        self.entrar_na_aba(aba, alvo.mostrar);
        self.rclone.aberta.set(false);
    }

    /// "Abrir": o remoto escolhido na grade, sem estar na árvore.
    pub(super) fn abrir_remoto_escolhido(&self) {
        if let Some(nome) = self.remoto_escolhido() {
            self.abrir_remoto_da_lista(&nome);
        }
    }

    pub(super) fn abrir_remoto_da_lista(&self, nome: &str) {
        self.abrir_remoto(
            nome.to_owned(),
            None,
            RcloneOpcoes {
                remote: nome.to_owned(),
                path: String::new(),
            },
            None,
            None,
            true,
        );
    }

    /// "Apagar": a confirmação já foi dada.
    fn apagar_remoto(&self) {
        let Some(nome) = self.remoto_escolhido() else {
            return;
        };
        self.rclone.ocupado.set(true);
        let aviso = t!("rclone-remoto-apagado", nome = nome.as_str());
        self.em_thread_rclone(move |rcds| {
            let apagado = rcds
                .obter(None)
                .and_then(|rcd| rcd.config_delete(&nome))
                .map_err(|e| Falha::de(&e));
            match apagado {
                Ok(()) => Chegada::Remotos(listar(&rcds), Some(aviso)),
                Err(falha) => Chegada::Remotos(Err(falha), None),
            }
        });
    }

    /// "Pôr na árvore": cria a conexão do remoto no grupo escolhido da árvore.
    fn por_na_arvore(&self) {
        let Some(remoto) = self.remoto_escolhido() else {
            return;
        };
        let nome = self.rclone.arvore_nome.get_untracked().trim().to_owned();
        let nome = if nome.is_empty() {
            remoto.clone()
        } else {
            nome
        };
        let pai = self
            .tree
            .with_untracked(|t| t.group_for_new(self.id_selecionado()));
        let mut conexao = Connection::new(nome.clone(), Kind::Rclone);
        conexao.rclone = RcloneOpcoes {
            remote: remoto,
            path: String::new(),
        };
        let id = self.tree.try_update(|t| t.add_connection(pai, conexao));
        self.abre_grupo(pai);
        self.filtro.set(String::new());
        self.atualizar();
        self.selecionado.set(id.map(|id| id.to_string()));
        self.rclone.aviso.set(Some((
            false,
            t!("rclone-foi-para-a-arvore", nome = nome.as_str()),
        )));
    }

    /// Abre o diálogo "Opções do rclone" com o que está gravado.
    fn abrir_opcoes_rclone(&self) {
        let atuais = self.opener.rcds().map(|r| r.opcoes()).unwrap_or_default();
        self.rclone.opcoes_binario.set(atuais.binario);
        self.rclone.opcoes_config.set(atuais.config);
        self.rclone.opcoes_aberto.set(true);
    }

    /// Grava as opções, aplica ao `Rcds` (que derruba os processos) e lê os remotos de novo.
    fn gravar_opcoes_rclone(&self) {
        let opcoes = OpcoesRclone {
            binario: self.rclone.opcoes_binario.get_untracked().trim().to_owned(),
            config: self.rclone.opcoes_config.get_untracked().trim().to_owned(),
        };
        // Quem grava o arquivo é o efeito das configurações; aplicar já evita esperar por ele.
        self.config.update(|c| {
            c.rclone.binario = opcoes.binario.clone();
            c.rclone.arquivo_config = opcoes.config.clone();
        });
        if let Some(rcds) = self.opener.rcds() {
            rcds.definir_opcoes(opcoes);
        }
        self.rclone.falha.set(None);
        self.listar_remotos_com(Some(t!("rclone-opcoes-gravadas")));
    }
}

/// Os remotos de um `rcd` que sobe agora (ou já subiu).
pub(super) fn listar(rcds: &Rcds) -> Result<Vec<RemoteInfo>, Falha> {
    let mut remotos = rcds
        .obter(None)
        .and_then(|rcd| rcd.config_remotes())
        .map_err(|e| Falha::de(&e))?;
    remotos.sort_by_key(|r| r.name.to_lowercase());
    Ok(remotos)
}

/// O `InfoBar` de "instale o rclone" (binário ausente ou versão antiga). O `id` muda entre a
/// subpágina e a página Arquivos.
pub(super) fn aviso_de_instalacao(tela: &Rc<Tela>, id: &str) -> InfoBar {
    let falha = tela.rclone.falha;
    InfoBar::new(InfoBarSeverity::Warning)
        .id(id)
        .message(move || {
            falha.with(|f| f.as_ref().map(|f| f.texto().to_owned()).unwrap_or_default())
        })
        .visible(move || falha.with(Option::is_some))
        .on_close(move || falha.set(None))
}

/// A subpágina: a lista de remotos e as ações.
pub(super) fn subpagina(tela: &Rc<Tela>) -> Column {
    let r = &tela.rclone;
    let (aberta, aviso, ocupado, selecao) = (r.aberta, r.aviso, r.ocupado, r.selecao);
    let form_aberto = r.form.aberto;
    let sem_escolha = move || selecao.with(BTreeSet::is_empty);
    let texto = move |erro: bool| {
        aviso.with(|a| match a {
            Some((e, t)) if *e == erro => t.clone(),
            _ => String::new(),
        })
    };
    let tem = move |erro: bool| aviso.with(|a| a.as_ref().is_some_and(|(e, _)| *e == erro));

    let voltar = Button::new(t!("comum-voltar"))
        .id("rclone-voltar")
        .icon(Symbol::ArrowLeft)
        .on_click(move || aberta.set(false));

    let botoes = {
        let (t_abrir, t_novo, t_editar, t_apagar, t_arvore, t_atualizar, t_opcoes) = (
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
        );
        Row::new()
            .gap(4)
            .child(
                Button::new(t!("comum-abrir"))
                    .id("rclone-abrir")
                    .icon(Symbol::FolderOpen)
                    .disabled(sem_escolha)
                    .on_click(move || t_abrir.abrir_remoto_escolhido()),
            )
            .child(
                Button::new(t!("comum-novo"))
                    .id("rclone-novo")
                    .icon(Symbol::Add)
                    .on_click(move || t_novo.novo_remoto_rclone()),
            )
            .child(
                Button::new(t!("comum-editar"))
                    .id("rclone-editar")
                    .icon(Symbol::Edit)
                    .disabled(sem_escolha)
                    .on_click(move || t_editar.editar_remoto_rclone()),
            )
            .child(
                Button::new(t!("comum-apagar"))
                    .id("rclone-apagar")
                    .icon(Symbol::Delete)
                    .disabled(sem_escolha)
                    .on_click(move || t_apagar.rclone.apagar_aberto.set(true)),
            )
            .child(
                Button::new(t!("rclone-por-na-arvore"))
                    .id("rclone-arvore")
                    .icon(Symbol::Link)
                    .disabled(sem_escolha)
                    .on_click(move || {
                        if let Some(nome) = t_arvore.remoto_escolhido() {
                            t_arvore.rclone.arvore_nome.set(nome);
                            t_arvore.rclone.arvore_aberto.set(true);
                        }
                    }),
            )
            .child(
                Button::new(t!("comum-atualizar"))
                    .id("rclone-atualizar")
                    .icon(Symbol::ArrowSync)
                    .on_click(move || t_atualizar.listar_remotos()),
            )
            .child(
                Button::new(t!("rclone-opcoes"))
                    .id("rclone-opcoes")
                    .icon(Symbol::Settings)
                    .on_click(move || t_opcoes.abrir_opcoes_rclone()),
            )
    };

    let grade = {
        let t = Rc::clone(tela);
        DataGrid::bind(r.linhas, r.selecao)
            .id("rclone-remotos")
            .grow(1)
            .columns([
                DataGridColumn::new("nome", t!("comum-nome")).width(ColumnWidth::Star(1.0)),
                DataGridColumn::new("tipo", t!("comum-tipo")).width(ColumnWidth::Pixels(180.0)),
            ])
            .empty_text(t!("rclone-nenhum-remoto-configurado-use-novo-para"))
            .on_invoke(move |nome| t.abrir_remoto_da_lista(&nome))
    };

    let lista = Column::new()
        .id("rclone-lista")
        .grow(1)
        .gap(8)
        .padding(Insets::new(8.0, 8.0, 8.0, 8.0))
        .child(
            Row::new()
                .gap(8)
                .child(voltar)
                .child(Text::new(t!("comum-remotos-do-rclone")).title()),
        )
        .child(aviso_de_instalacao(tela, "rclone-instalar"))
        .child(
            InfoBar::new(InfoBarSeverity::Success)
                .id("rclone-aviso")
                .message(move || texto(false))
                .visible(move || tem(false))
                .on_close(move || aviso.set(None)),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("rclone-erro")
                .message(move || texto(true))
                .visible(move || tem(true))
                .on_close(move || aviso.set(None)),
        )
        .child(botoes)
        .child(
            Text::new(t!("rclone-lendo-os-remotos"))
                .id("rclone-ocupado")
                .secondary()
                .visible(move || ocupado.get()),
        )
        .child(grade)
        .visible(move || !form_aberto.get());
    Column::new()
        .id("rclone")
        .grow(1)
        .child(lista)
        .child(rclone_form::pagina(tela))
}

/// Os diálogos da subpágina: apagar, pôr na árvore e as opções do binário.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View {
    let r = &tela.rclone;
    let apagar = {
        let (t, t_nome) = (Rc::clone(tela), Rc::clone(tela));
        ContentDialog::new(t!("rclone-apagar-o-remoto"), r.apagar_aberto)
            .id("rclone-apagar-dialogo")
            .content(
                Text::new(move || {
                    t!(
                        "rclone-apagar-texto",
                        nome = t_nome.remoto_escolhido().unwrap_or_default()
                    )
                })
                .id("rclone-apagar-texto")
                .wrap(),
            )
            .primary(t!("comum-apagar"), move || t.apagar_remoto())
            .close(t!("comum-cancelar"))
    };
    let arvore = {
        let t = Rc::clone(tela);
        ContentDialog::new(t!("rclone-por-na-arvore"), r.arvore_aberto)
            .id("rclone-arvore-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(
                        Text::new(t!("rclone-a-conexao-fica-no-grupo-escolhido"))
                            .secondary()
                            .wrap(),
                    )
                    .child(Text::new(t!("rclone-nome-da-conexao")).caption())
                    .child(Edit::bind(r.arvore_nome).id("rclone-arvore-nome")),
            )
            .primary(t!("rclone-por-na-arvore"), move || t.por_na_arvore())
            .close(t!("comum-cancelar"))
    };
    let opcoes = {
        let t = Rc::clone(tela);
        ContentDialog::new(t!("rclone-opcoes-do-rclone"), r.opcoes_aberto)
            .id("rclone-opcoes-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(t!("rclone-caminho-do-rclone")).caption())
                    .child(
                        Edit::bind(r.opcoes_binario)
                            .id("rclone-opcoes-binario")
                            .placeholder(t!("rclone-vazio-procura-rclone-no-path")),
                    )
                    .child(Text::new(t!("rclone-arquivo-de-configuracao")).caption())
                    .child(
                        Edit::bind(r.opcoes_config)
                            .id("rclone-opcoes-config")
                            .placeholder(t!("rclone-vazio-o-padrao-do-rclone")),
                    ),
            )
            .primary(t!("rclone-gravar"), move || t.gravar_opcoes_rclone())
            .close(t!("comum-cancelar"))
    };
    Column::new()
        .child(apagar)
        .child(arvore)
        .child(opcoes)
        .child(rclone_form::dialogos(tela))
}
