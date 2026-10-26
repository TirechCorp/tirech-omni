//! O X11 e os túneis na casca (M37): o editor de túneis de uma conexão, a lista dos túneis ativos
//! de uma aba e a linha "Encaminhar X11" que acompanha o modo padrão das configurações.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::{Forward, ForwardState, ForwardStatus, LocalSocket, X11Mode};

use super::{Tela, poe, rotulo_de_x11};
use crate::abrir::forward_do_tunel;
use crate::arquivos::formato;
use crate::espacos::id_da_chave;
use crate::links::DestinoDoLink;
use crate::modelo::{SOCKET_DO_LIBVIRT, TipoTunel, Tunel, dividir_destino};
use crate::t;

/// A chave da linha do X11 na lista dos túneis ativos.
const CHAVE_X11: &str = "x11";

/// O que o editor de túneis e a lista dos túneis ativos guardam.
pub(super) struct EstadoTuneis {
    /// O editor de túneis da conexão escolhida está aberto.
    pub aberto: Signal<bool>,
    /// As linhas da lista do editor.
    pub linhas: Signal<Vec<DataGridRow>>,
    /// A linha escolhida na lista do editor (a posição do túnel).
    pub selecao: Signal<BTreeSet<String>>,
    /// O formulário de um túnel está no lugar da lista.
    pub formulario: Signal<bool>,
    /// A posição do túnel que o formulário edita (`None`: um novo).
    pub indice: Signal<Option<usize>>,
    /// O tipo, pelo nome da lista de escolha.
    pub tipo: Signal<String>,
    /// A porta que escuta.
    pub porta: Signal<f64>,
    /// O destino, como `host:porta`.
    pub destino: Signal<String>,
    /// O caminho do socket no servidor.
    pub caminho: Signal<String>,
    /// O socket daqui.
    pub socket_local: Signal<String>,
    /// Escuta em todas as interfaces.
    pub outras: Signal<bool>,
    /// Sobe junto com a conexão.
    pub ligado: Signal<bool>,
    /// O nome da lista.
    pub nome: Signal<String>,
    /// Usa a próxima porta livre se a escolhida estiver ocupada (M58).
    pub proxima_livre: Signal<bool>,
    /// O caminho que "Abrir no navegador" usa (M58).
    pub caminho_web: Signal<String>,
    /// O nome do site, o `Host` do relé HTTP (M58).
    pub host_http: Signal<String>,
    /// O servidor fala HTTPS: o relé faz o TLS (M58).
    pub https: Signal<bool>,
    /// A sanfona "Opções da web" do formulário.
    pub opcoes_web: Signal<bool>,
    /// O formulário vem da lista dos túneis ativos: o túnel sobe na sessão aberta.
    pub na_sessao: Signal<bool>,
    /// Guardar também na conexão (só no formulário que vem dos túneis ativos).
    pub salvar_na_conexao: Signal<bool>,
    /// Ao fechar o editor, a lista dos túneis ativos volta.
    pub volta_aos_ativos: Cell<bool>,
    /// A lista dos túneis ativos está aberta.
    pub ativos_aberto: Signal<bool>,
    /// As linhas da lista dos túneis ativos.
    pub ativos_linhas: Signal<Vec<DataGridRow>>,
    /// A linha escolhida na lista dos túneis ativos.
    pub ativos_selecao: Signal<BTreeSet<String>>,
    /// O título da lista dos túneis ativos.
    pub ativos_titulo: Signal<String>,
    /// O que a linha escolhida tem a dizer (o motivo da falha, o último erro).
    pub ativos_detalhe: Signal<String>,
    /// Há linha escolhida que se pode parar (túnel escutando) ou ligar (parado ou com falha).
    pub ativos_pode_parar: Signal<bool>,
    /// A linha escolhida é um túnel parado ou que falhou.
    pub ativos_pode_ligar: Signal<bool>,
    /// A linha escolhida é um túnel local que escuta: "Abrir no navegador" vale (M58).
    pub ativos_pode_abrir: Signal<bool>,
    /// A sessão dos túneis ativos.
    pub ativos_sessao: Cell<Option<Session>>,
    /// Qual abertura da lista dos túneis ativos ainda vale (as antigas param de reler).
    pub ativos_ciclo: Cell<u64>,
}

impl EstadoTuneis {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            linhas: signal(Vec::new()),
            selecao: signal(BTreeSet::new()),
            formulario: signal(false),
            indice: signal(None),
            tipo: signal(TipoTunel::Local.nome().to_owned()),
            porta: signal(0.0),
            destino: signal(String::new()),
            caminho: signal(String::new()),
            socket_local: signal(String::new()),
            outras: signal(false),
            ligado: signal(true),
            nome: signal(String::new()),
            proxima_livre: signal(false),
            caminho_web: signal(String::new()),
            host_http: signal(String::new()),
            https: signal(false),
            opcoes_web: signal(false),
            na_sessao: signal(false),
            salvar_na_conexao: signal(false),
            volta_aos_ativos: Cell::new(false),
            ativos_aberto: signal(false),
            ativos_linhas: signal(Vec::new()),
            ativos_selecao: signal(BTreeSet::new()),
            ativos_titulo: signal(String::new()),
            ativos_detalhe: signal(String::new()),
            ativos_pode_parar: signal(false),
            ativos_pode_ligar: signal(false),
            ativos_pode_abrir: signal(false),
            ativos_sessao: Cell::new(None),
            ativos_ciclo: Cell::new(0),
        }
    }

    /// O tipo que a lista de escolha mostra.
    fn tipo_escolhido(&self) -> TipoTunel {
        tipo_do_nome(&self.tipo.get())
    }

    /// O túnel que o formulário descreve (pode estar incompleto: veja [`Tunel::problema`]).
    fn tunel_do_formulario(&self) -> Tunel {
        let tipo = self.tipo_escolhido();
        let mut tunel = Tunel::novo(tipo, self.outras.get());
        let porta = self.porta.get().round().clamp(0.0, f64::from(u16::MAX));
        tunel.porta = u16::try_from(porta as u32).unwrap_or(0);
        match tipo {
            TipoTunel::Local | TipoTunel::Remoto => match dividir_destino(&self.destino.get()) {
                Some((host, porta)) => {
                    tunel.destino_host = host;
                    tunel.destino_porta = porta;
                }
                None => tunel.destino_host = self.destino.get().trim().to_owned(),
            },
            TipoTunel::SocketUnix => {
                tunel.caminho_remoto = self.caminho.get().trim().to_owned();
                tunel.socket_local = self.socket_local.get().trim().to_owned();
            }
            TipoTunel::Dinamico => {}
        }
        tunel.ligado = self.ligado.get();
        tunel.nome = self.nome.get().trim().to_owned();
        if tipo == TipoTunel::Local {
            tunel.proxima_livre = self.proxima_livre.get();
            tunel.caminho_web = self.caminho_web.get().trim().to_owned();
            tunel.host_http = self.host_http.get().trim().to_owned();
            tunel.https = self.https.get();
        }
        tunel
    }

    /// Põe o túnel no formulário.
    fn carregar(&self, tunel: &Tunel) {
        self.tipo.set(tunel.tipo.nome().to_owned());
        self.porta.set(f64::from(tunel.porta));
        self.destino.set(if tunel.destino_host.is_empty() {
            String::new()
        } else {
            tunel.destino()
        });
        self.caminho.set(tunel.caminho_remoto.clone());
        self.socket_local.set(tunel.socket_local.clone());
        self.outras.set(tunel.outras_maquinas);
        self.ligado.set(tunel.ligado);
        self.nome.set(tunel.nome.clone());
        self.proxima_livre.set(tunel.proxima_livre);
        self.caminho_web.set(tunel.caminho_web.clone());
        self.host_http.set(tunel.host_http.clone());
        self.https.set(tunel.https);
        self.opcoes_web
            .set(tunel.https || !tunel.host_http.is_empty());
    }
}

/// O tipo de túnel que a lista de escolha chama de `nome`.
fn tipo_do_nome(nome: &str) -> TipoTunel {
    TipoTunel::TODOS
        .into_iter()
        .find(|tipo| tipo.nome() == nome)
        .unwrap_or_default()
}

/// A linha do túnel `n` na lista do editor.
fn linha_do_tunel(n: usize, tunel: &Tunel) -> DataGridRow {
    let texto = |s: String| CellValue::Text(s);
    let ligado = if tunel.ligado {
        t!("comum-sim")
    } else {
        t!("comum-nao")
    };
    DataGridRow::new(n.to_string())
        .cell(tunel.tipo.curto(), texto(tunel.tipo.curto().to_owned()))
        .cell(tunel.escuta(), texto(tunel.escuta()))
        .cell(tunel.destino(), texto(tunel.destino()))
        .cell(ligado.clone(), texto(ligado))
        .icon(Symbol::Link)
}

/// O tipo, onde escuta e o destino de um túnel que a lib subiu.
fn descricao_do_forward(forward: &Forward) -> (String, String, String) {
    match forward {
        Forward::Local { bind, host, port } => (
            "Local".to_owned(),
            bind.to_string(),
            format!("{host}:{port}"),
        ),
        Forward::Remote { bind, host, port } => (
            t!("comum-tunel-remoto-curto"),
            t!("tuneis-servidor-porta", porta = i64::from(bind.port)),
            format!("{host}:{port}"),
        ),
        Forward::Dynamic { bind } => ("SOCKS5".to_owned(), bind.to_string(), "SOCKS5".to_owned()),
        Forward::Unix { local, remote_path } => {
            let onde = match local {
                LocalSocket::Path(caminho) => caminho.display().to_string(),
                LocalSocket::Tcp(bind) => bind.to_string(),
            };
            ("Socket".to_owned(), onde, remote_path.clone())
        }
    }
}

/// O estado de um túnel, em português.
fn estado_do_forward(estado: &ForwardState) -> String {
    match estado {
        ForwardState::Starting => t!("tuneis-estado-subindo"),
        ForwardState::Listening => t!("tuneis-estado-escutando"),
        ForwardState::Stopped => t!("tuneis-estado-parado"),
        ForwardState::Failed(_) => t!("tuneis-estado-falhou"),
    }
}

/// A linha de um túnel ativo.
fn linha_do_ativo(status: &ForwardStatus) -> DataGridRow {
    let (tipo, escuta, destino) = descricao_do_forward(&status.forward);
    let escuta = status.listening.clone().unwrap_or(escuta);
    let estado = estado_do_forward(&status.state);
    let texto = |s: &str| CellValue::Text(s.to_owned());
    let enviado = formato::tamanho(status.sent);
    let recebido = formato::tamanho(status.received);
    DataGridRow::new(format!("t{}", status.id.get()))
        .cell(tipo.clone(), texto(&tipo))
        .cell(escuta.clone(), texto(&escuta))
        .cell(destino.clone(), texto(&destino))
        .cell(estado.clone(), texto(&estado))
        .cell(
            status.active.to_string(),
            CellValue::Number(status.active as f64),
        )
        .cell(
            status.total.to_string(),
            CellValue::Number(status.total as f64),
        )
        .cell(enviado.clone(), CellValue::Number(status.sent as f64))
        .cell(recebido.clone(), CellValue::Number(status.received as f64))
        .icon(Symbol::Link)
}

impl Tela {
    /// Põe na linha "Encaminhar X11" o modo da conexão escolhida, com o padrão das configurações
    /// no texto "Padrão (…)".
    pub(super) fn sincronizar_x11(&self) {
        let modo = self
            .id_selecionado()
            .and_then(|id| {
                self.tree
                    .with_untracked(|t| t.connection(id).map(|c| c.x11))
            })
            .unwrap_or_default();
        let padrao = self.config.with_untracked(|c| c.x11.padrao);
        poe(self.campos.x11, rotulo_de_x11(modo, padrao));
    }

    /// A conexão escolhida na árvore ganha o editor de túneis.
    pub(super) fn abrir_tuneis(&self) {
        let est = &self.tuneis;
        est.formulario.set(false);
        est.na_sessao.set(false);
        est.volta_aos_ativos.set(false);
        est.selecao.set(BTreeSet::new());
        self.refazer_linhas_dos_tuneis();
        est.aberto.set(true);
    }

    /// Escolhe a conexão `chave` na árvore e abre o editor dos túneis dela.
    pub(super) fn abrir_tuneis_de(&self, chave: &str) {
        self.selecionado.set(Some(chave.to_owned()));
        // A tela carrega as propriedades da conexão por um efeito; o editor lê os túneis dela.
        self.carregar_propriedades();
        self.abrir_tuneis();
    }

    fn refazer_linhas_dos_tuneis(&self) {
        let linhas = self.campos.tuneis.with_untracked(|t| {
            t.iter()
                .enumerate()
                .map(|(n, t)| linha_do_tunel(n, t))
                .collect()
        });
        self.tuneis.linhas.set(linhas);
    }

    /// A posição do túnel escolhido na lista do editor.
    fn tunel_escolhido(&self) -> Option<usize> {
        self.tuneis
            .selecao
            .with_untracked(|s| s.iter().next().and_then(|k| k.parse().ok()))
            .filter(|n| *n < self.campos.tuneis.with_untracked(Vec::len))
    }

    /// Abre o formulário vazio.
    pub(super) fn adicionar_tunel(&self) {
        let outras = self.config.with_untracked(|c| c.tuneis.outras_maquinas);
        let est = &self.tuneis;
        est.carregar(&Tunel::novo(TipoTunel::Local, outras));
        est.indice.set(None);
        est.na_sessao.set(false);
        est.formulario.set(true);
    }

    /// Abre o formulário com o túnel escolhido.
    pub(super) fn editar_tunel(&self) {
        let Some(n) = self.tunel_escolhido() else {
            return;
        };
        let Some(tunel) = self.campos.tuneis.with_untracked(|t| t.get(n).cloned()) else {
            return;
        };
        let est = &self.tuneis;
        est.carregar(&tunel);
        est.indice.set(Some(n));
        est.na_sessao.set(false);
        est.formulario.set(true);
    }

    /// Tira o túnel escolhido da conexão.
    pub(super) fn remover_tunel(&self) {
        let Some(n) = self.tunel_escolhido() else {
            return;
        };
        self.campos.tuneis.update(|t| {
            t.remove(n);
        });
        self.tuneis.selecao.set(BTreeSet::new());
        self.refazer_linhas_dos_tuneis();
    }

    /// Grava o túnel do formulário: na conexão (que o `guardar_propriedades` leva ao disco) ou,
    /// vindo da lista dos ativos, na sessão aberta e, se pedido, também na conexão.
    pub(super) fn salvar_tunel(self: &Rc<Self>) {
        let est = &self.tuneis;
        let tunel = est.tunel_do_formulario();
        if tunel.problema().is_some() {
            return;
        }
        if est.na_sessao.get_untracked() {
            self.subir_tunel_na_sessao(&tunel);
            est.formulario.set(false);
            est.aberto.set(false);
            self.voltar_aos_ativos();
            return;
        }
        match est.indice.get_untracked() {
            Some(n) => self.campos.tuneis.update(|t| {
                if let Some(atual) = t.get_mut(n) {
                    *atual = tunel;
                }
            }),
            None => self.campos.tuneis.update(|t| t.push(tunel)),
        }
        est.formulario.set(false);
        self.refazer_linhas_dos_tuneis();
    }

    /// Sobe o túnel na sessão dos ativos e, se a pessoa pediu, guarda na conexão dela.
    fn subir_tunel_na_sessao(&self, tunel: &Tunel) {
        let Some(sessao) = self.tuneis.ativos_sessao.get() else {
            return;
        };
        let Some(id) = id_da_chave(&sessao.key_str()) else {
            return;
        };
        if let Some(forwards) = sessao.ssh_forwards() {
            let posicao = forwards.list().len();
            forwards.add(forward_do_tunel(id, posicao, tunel));
        }
        if !self.tuneis.salvar_na_conexao.get_untracked() {
            return;
        }
        let guardado = tunel.clone();
        let mexeu = self.tree.try_update(|t| {
            t.connection_mut(id)
                .map(|c| c.tuneis.push(guardado))
                .is_some()
        });
        if mexeu == Some(true) {
            self.atualizar();
            if self.id_selecionado() == Some(id) {
                self.carregar_propriedades();
            }
        }
    }

    fn voltar_aos_ativos(self: &Rc<Self>) {
        if self.tuneis.volta_aos_ativos.replace(false)
            && let Some(sessao) = self.tuneis.ativos_sessao.get()
        {
            self.abrir_ativos_de(sessao);
        }
    }

    /// Abre o formulário de um túnel novo para a sessão dos túneis ativos.
    pub(super) fn adicionar_tunel_na_sessao(&self) {
        let outras = self.config.with_untracked(|c| c.tuneis.outras_maquinas);
        let est = &self.tuneis;
        est.carregar(&Tunel::novo(TipoTunel::Local, outras));
        est.indice.set(None);
        est.na_sessao.set(true);
        est.salvar_na_conexao.set(false);
        est.volta_aos_ativos.set(true);
        est.formulario.set(true);
        est.ativos_aberto.set(false);
        est.aberto.set(true);
    }

    /// Abre a lista dos túneis ativos da sessão (a aba, no menu).
    pub(super) fn abrir_ativos_de(self: &Rc<Self>, sessao: Session) {
        let est = &self.tuneis;
        est.ativos_sessao.set(Some(sessao));
        est.ativos_titulo
            .set(t!("tuneis-ativos-de", sessao = sessao.title()));
        est.ativos_selecao.set(BTreeSet::new());
        let ciclo = est.ativos_ciclo.get() + 1;
        est.ativos_ciclo.set(ciclo);
        self.atualizar_ativos();
        est.ativos_aberto.set(true);
        self.vigiar_ativos(ciclo);
    }

    /// Relê os túneis da sessão a cada segundo, enquanto a lista está aberta e a abertura é a
    /// mesma.
    fn vigiar_ativos(self: &Rc<Self>, ciclo: u64) {
        let t = Rc::clone(self);
        self.alarmes.depois(Duration::from_secs(1), move || {
            let est = &t.tuneis;
            if est.ativos_ciclo.get() != ciclo || !est.ativos_aberto.get_untracked() {
                return;
            }
            t.atualizar_ativos();
            t.vigiar_ativos(ciclo);
        });
    }

    /// Refaz as linhas dos túneis ativos com o estado de agora.
    pub(super) fn atualizar_ativos(&self) {
        let est = &self.tuneis;
        let Some(forwards) = est.ativos_sessao.get().and_then(|s| s.ssh_forwards()) else {
            est.ativos_linhas.set(Vec::new());
            est.ativos_detalhe.set(String::new());
            return;
        };
        let lista = forwards.list();
        let mut linhas = Vec::new();
        if let Some(x11) = forwards.x11() {
            let modo = match x11.mode {
                X11Mode::Trusted => t!("tuneis-x11-confiavel"),
                X11Mode::Untrusted => t!("tuneis-x11-nao-confiavel"),
            };
            let estado = if x11.error.is_some() {
                t!("tuneis-x11-com-erro")
            } else {
                t!("tuneis-x11-pronto")
            };
            let texto = |s: &str| CellValue::Text(s.to_owned());
            linhas.push(
                DataGridRow::new(CHAVE_X11)
                    .cell("X11", texto("X11"))
                    .cell(x11.display.clone(), texto(&x11.display))
                    .cell(modo.clone(), texto(&modo))
                    .cell(estado.clone(), texto(&estado))
                    .cell(x11.active.to_string(), CellValue::Number(x11.active as f64))
                    .cell(x11.total.to_string(), CellValue::Number(x11.total as f64))
                    .cell("", texto(""))
                    .cell("", texto(""))
                    .icon(Symbol::Link),
            );
        }
        linhas.extend(lista.iter().map(linha_do_ativo));
        est.ativos_linhas.set(linhas);

        // O que a linha escolhida diz e o que se pode fazer com ela.
        let chave = est
            .ativos_selecao
            .with_untracked(|s| s.iter().next().cloned());
        let escolhido = chave
            .as_deref()
            .and_then(|k| k.strip_prefix('t'))
            .and_then(|n| n.parse::<u64>().ok())
            .and_then(|n| lista.iter().find(|s| s.id.get() == n));
        let detalhe = match (&chave, escolhido) {
            (_, Some(s)) => match &s.state {
                ForwardState::Failed(motivo) => motivo.clone(),
                _ => s.last_error.clone().unwrap_or_default(),
            },
            (Some(k), None) if k == CHAVE_X11 => {
                forwards.x11().and_then(|x| x.error).unwrap_or_default()
            }
            _ => String::new(),
        };
        poe(est.ativos_detalhe, detalhe);
        let (parar, ligar) = match escolhido.map(|s| &s.state) {
            Some(ForwardState::Listening | ForwardState::Starting) => (true, false),
            Some(ForwardState::Stopped | ForwardState::Failed(_)) => (false, true),
            None => (false, false),
        };
        if est.ativos_pode_parar.get_untracked() != parar {
            est.ativos_pode_parar.set(parar);
        }
        if est.ativos_pode_ligar.get_untracked() != ligar {
            est.ativos_pode_ligar.set(ligar);
        }
        let abrir = escolhido.is_some_and(|s| {
            matches!(s.forward, Forward::Local { .. }) && s.state == ForwardState::Listening
        });
        if est.ativos_pode_abrir.get_untracked() != abrir {
            est.ativos_pode_abrir.set(abrir);
        }
    }

    /// Abre no navegador o túnel local escolhido na lista dos ativos, no caminho que a conexão
    /// guarda para ele.
    pub(super) fn abrir_ativo_no_navegador(&self) {
        let est = &self.tuneis;
        let Some(sessao) = est.ativos_sessao.get() else {
            return;
        };
        let Some(forwards) = sessao.ssh_forwards() else {
            return;
        };
        let numero = est
            .ativos_selecao
            .with_untracked(|s| s.iter().next().cloned())
            .and_then(|k| k.strip_prefix('t').and_then(|n| n.parse::<u64>().ok()));
        let Some(status) =
            numero.and_then(|n| forwards.list().into_iter().find(|s| s.id.get() == n))
        else {
            return;
        };
        let Forward::Local {
            bind, host, port, ..
        } = &status.forward
        else {
            return;
        };
        let local = status
            .listening
            .as_deref()
            .and_then(|escuta| escuta.rsplit(':').next())
            .and_then(|porta| porta.parse().ok())
            .unwrap_or(bind.port);
        let caminho = self
            .conexao_da_sessao(sessao)
            .and_then(|id| {
                self.tree.with_untracked(|t| {
                    t.connection(id).and_then(|c| {
                        c.tuneis
                            .iter()
                            .find(|x| {
                                x.tipo == TipoTunel::Local
                                    && x.destino_host == *host
                                    && x.destino_porta == *port
                            })
                            .map(|x| x.caminho_web.clone())
                    })
                })
            })
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| "/".to_owned());
        let url = crate::portas::url::url_local(local, &caminho, "");
        if let Err(erro) = self.opener.abrir_link(&DestinoDoLink::Navegador(url)) {
            self.avisar(
                InfoBarSeverity::Warning,
                t!("links-nao-abriu", erro = erro.to_string()),
            );
        }
    }

    /// Para ou liga o túnel escolhido na lista dos ativos.
    pub(super) fn mexer_no_ativo(&self, ligar: bool) {
        let est = &self.tuneis;
        let Some(forwards) = est.ativos_sessao.get().and_then(|s| s.ssh_forwards()) else {
            return;
        };
        let numero = est
            .ativos_selecao
            .with_untracked(|s| s.iter().next().cloned())
            .and_then(|k| k.strip_prefix('t').and_then(|n| n.parse::<u64>().ok()));
        let Some(status) =
            numero.and_then(|n| forwards.list().into_iter().find(|s| s.id.get() == n))
        else {
            return;
        };
        if ligar {
            forwards.restart(status.id);
        } else {
            forwards.stop(status.id);
        }
        self.atualizar_ativos();
    }

    /// Olha a aba por uns 20 segundos e avisa se algum túnel não subiu (a falha do `bind`, a
    /// porta ocupada, o destino que o servidor recusou).
    pub(super) fn vigiar_tuneis_da_sessao(&self, sessao: Session) {
        self.olhar_tuneis_da_sessao(sessao, 20);
    }

    fn olhar_tuneis_da_sessao(&self, sessao: Session, restantes: u32) {
        let eu = self.eu.clone();
        self.alarmes.depois(Duration::from_secs(1), move || {
            let Some(t) = eu.upgrade() else {
                return;
            };
            if let Some(falha) = falha_de_tunel(sessao) {
                t.avisar(
                    InfoBarSeverity::Warning,
                    t!("tuneis-falha-veja-os-ativos", falha = falha.as_str()),
                );
            } else if restantes > 1 {
                t.olhar_tuneis_da_sessao(sessao, restantes - 1);
            }
        });
    }

    /// Liga os efeitos do editor de túneis; chame uma vez, ao montar a tela.
    pub(super) fn ligar_tuneis(self: &Rc<Self>) {
        // A conexão mudou na árvore (outra escolhida, ou a edição aqui): a lista acompanha.
        let t = Rc::clone(self);
        effect(move || {
            let _ = t.campos.tuneis.get();
            untrack(|| t.refazer_linhas_dos_tuneis());
        });
        // Fechar o editor de dentro dos túneis ativos devolve a lista deles.
        let t = Rc::clone(self);
        effect(move || {
            if !t.tuneis.aberto.get() {
                untrack(|| {
                    t.tuneis.formulario.set(false);
                    t.voltar_aos_ativos();
                });
            }
        });
        // A seleção da lista dos ativos muda o que os botões podem.
        let t = Rc::clone(self);
        effect(move || {
            let _ = t.tuneis.ativos_selecao.get();
            untrack(|| t.atualizar_ativos());
        });
    }
}

/// O texto de ajuda do formulário: o que falta, ou o que o tipo escolhido faz.
fn ajuda_do_formulario(est: &EstadoTuneis) -> String {
    let tunel = est.tunel_do_formulario();
    if let Some(problema) = tunel.problema() {
        return problema;
    }
    match tunel.tipo {
        TipoTunel::Local => t!("tuneis-ajuda-local"),
        TipoTunel::Remoto => t!("tuneis-ajuda-remoto"),
        TipoTunel::Dinamico => t!("tuneis-ajuda-dinamico"),
        TipoTunel::SocketUnix => t!("tuneis-ajuda-socket"),
    }
}

/// A lista do editor com os botões.
fn lista_do_editor(tela: &Rc<Tela>) -> Column {
    let est = &tela.tuneis;
    let (selecao, linhas, formulario) = (est.selecao, est.linhas, est.formulario);
    let sem_escolha = move || selecao.with(BTreeSet::is_empty);
    let grade = {
        let t = Rc::clone(tela);
        DataGrid::bind(linhas, selecao)
            .id("tuneis-lista")
            .height(220)
            .columns([
                DataGridColumn::new("tipo", t!("comum-tipo")).width(ColumnWidth::Pixels(110.0)),
                DataGridColumn::new("escuta", t!("tuneis-escuta")).width(ColumnWidth::Star(1.0)),
                DataGridColumn::new("destino", t!("tuneis-destino")).width(ColumnWidth::Star(1.4)),
                DataGridColumn::new("ligado", t!("tuneis-ligado")).width(ColumnWidth::Pixels(70.0)),
            ])
            .empty_text(t!("tuneis-nenhum-tunel-use-adicionar-para-criar"))
            .on_invoke(move |_| t.editar_tunel())
    };
    let botoes = {
        let (adicionar, editar, remover) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        Row::new()
            .gap(8)
            .child(
                Button::new(t!("comum-adicionar"))
                    .id("tuneis-adicionar")
                    .icon(Symbol::Add)
                    .on_click(move || adicionar.adicionar_tunel()),
            )
            .child(
                Button::new(t!("comum-editar"))
                    .id("tuneis-editar")
                    .icon(Symbol::Edit)
                    .disabled(sem_escolha)
                    .on_click(move || editar.editar_tunel()),
            )
            .child(
                Button::new(t!("comum-remover"))
                    .id("tuneis-remover")
                    .icon(Symbol::Delete)
                    .disabled(sem_escolha)
                    .on_click(move || remover.remover_tunel()),
            )
    };
    Column::new()
        .id("tuneis-editor-lista")
        .gap(8)
        .child(
            Text::new(t!("tuneis-os-tuneis-ligados-sobem-junto-com"))
                .secondary()
                .wrap(),
        )
        .child(botoes)
        .child(grade)
        .visible(move || !formulario.get())
}

/// O formulário de um túnel.
fn formulario_do_editor(tela: &Rc<Tela>) -> Column {
    let est = &tela.tuneis;
    let (tipo, na_sessao, formulario) = (est.tipo, est.na_sessao, est.formulario);
    let tipos: Vec<String> = TipoTunel::TODOS
        .iter()
        .map(|t| t.nome().to_owned())
        .collect();
    let (t_salvar, t_voltar, t_ajuda, t_pode) = (
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
    );
    let usa_destino = move || {
        matches!(
            tipo_do_nome(&tipo.get()),
            TipoTunel::Local | TipoTunel::Remoto
        )
    };
    let usa_socket = move || tipo_do_nome(&tipo.get()) == TipoTunel::SocketUnix;
    let remoto = move || tipo_do_nome(&tipo.get()) == TipoTunel::Remoto;
    let local = move || tipo_do_nome(&tipo.get()) == TipoTunel::Local;
    let pode_salvar = move || t_pode.tuneis.tunel_do_formulario().problema().is_none();
    Column::new()
        .id("tuneis-editor-form")
        .gap(8)
        .child(Text::new(t!("tuneis-tipo")).caption())
        .child(ComboBox::bind(est.tipo, tipos).id("tunel-tipo"))
        .child(Text::new(t!("tuneis-porta")).caption())
        .child(
            NumberBox::bind(est.porta)
                .id("tunel-porta")
                .min(0)
                .max(65535)
                .precision(0)
                .small_change(1),
        )
        .child(
            Text::new(move || {
                if remoto() {
                    t!("tuneis-destino-visto-da-maquina")
                } else {
                    t!("tuneis-destino-visto-do-servidor")
                }
            })
            .caption()
            .visible(usa_destino),
        )
        .child(
            Edit::bind(est.destino)
                .id("tunel-destino")
                .placeholder("db.interno:5432")
                .visible(usa_destino),
        )
        .child(
            Text::new(t!("tuneis-caminho-do-socket-no-servidor"))
                .caption()
                .visible(usa_socket),
        )
        .child(
            Edit::bind(est.caminho)
                .id("tunel-caminho")
                .placeholder(SOCKET_DO_LIBVIRT)
                .visible(usa_socket),
        )
        .child(
            Text::new(t!("tuneis-socket-aqui-vazio-automatico"))
                .caption()
                .visible(move || usa_socket() && !cfg!(windows)),
        )
        .child(
            Edit::bind(est.socket_local)
                .id("tunel-socket-local")
                .visible(move || usa_socket() && !cfg!(windows)),
        )
        .child(
            CheckBox::bind(est.outras, t!("tuneis-permitir-outras-maquinas"))
                .id("tunel-outras")
                .visible(move || !remoto()),
        )
        .child(
            CheckBox::bind(est.proxima_livre, t!("tuneis-proxima-livre"))
                .id("tunel-proxima-livre")
                .visible(local),
        )
        .child(
            Text::new(t!("tuneis-caminho-no-navegador"))
                .caption()
                .visible(local),
        )
        .child(
            Edit::bind(est.caminho_web)
                .id("tunel-caminho-web")
                .placeholder("/phpmyadmin/")
                .visible(local),
        )
        .child(
            Expander::new(t!("tuneis-opcoes-web"), est.opcoes_web)
                .id("tunel-opcoes-web")
                .visible(local)
                .content(
                    Column::new()
                        .gap(8)
                        .child(Text::new(t!("tuneis-nome-do-site")).caption())
                        .child(
                            Edit::bind(est.host_http)
                                .id("tunel-host-http")
                                .placeholder("loja.exemplo.com.br"),
                        )
                        .child(CheckBox::bind(est.https, t!("tuneis-https")).id("tunel-https")),
                ),
        )
        .child(CheckBox::bind(est.ligado, t!("tuneis-ligado")).id("tunel-ligado"))
        .child(Text::new(t!("tuneis-nome-opcional")).caption())
        .child(Edit::bind(est.nome).id("tunel-nome"))
        .child(
            CheckBox::bind(est.salvar_na_conexao, t!("tuneis-salvar-na-conexao"))
                .id("tunel-salvar-na-conexao")
                .visible(move || na_sessao.get()),
        )
        .child(
            Text::new(move || ajuda_do_formulario(&t_ajuda.tuneis))
                .id("tunel-ajuda")
                .caption()
                .wrap(),
        )
        .child(
            Row::new()
                .gap(8)
                .child(
                    Button::new(t!("comum-salvar"))
                        .id("tunel-salvar")
                        .accent()
                        .disabled(move || !pode_salvar())
                        .on_click(move || t_salvar.salvar_tunel()),
                )
                .child(
                    Button::new(t!("comum-voltar"))
                        .id("tunel-voltar")
                        .on_click(move || {
                            let est = &t_voltar.tuneis;
                            est.formulario.set(false);
                            if est.na_sessao.get_untracked() {
                                est.aberto.set(false);
                            }
                        }),
                ),
        )
        .visible(move || formulario.get())
}

/// O diálogo do editor de túneis da conexão escolhida.
pub(super) fn dialogo_do_editor(tela: &Rc<Tela>) -> ContentDialog {
    let t = Rc::clone(tela);
    let nome = move || t.conexao_selecionada().map(|c| c.name).unwrap_or_default();
    let est = &tela.tuneis;
    let na_sessao = est.na_sessao;
    let titulo = move || {
        if na_sessao.get() {
            t!("tuneis-adicionar-um-tunel")
        } else {
            t!("tuneis-tuneis-de", nome = nome())
        }
    };
    ContentDialog::new(titulo, est.aberto)
        .id("tuneis-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(lista_do_editor(tela))
                .child(formulario_do_editor(tela)),
        )
        .close(t!("comum-fechar"))
}

/// O diálogo dos túneis ativos de uma aba: o estado ao vivo, com parar e ligar.
pub(super) fn dialogo_dos_ativos(tela: &Rc<Tela>) -> ContentDialog {
    let est = &tela.tuneis;
    let (selecao, linhas) = (est.ativos_selecao, est.ativos_linhas);
    let (pode_parar, pode_ligar) = (est.ativos_pode_parar, est.ativos_pode_ligar);
    let pode_abrir = est.ativos_pode_abrir;
    let detalhe = est.ativos_detalhe;
    let grade = DataGrid::bind(linhas, selecao)
        .id("ativos-lista")
        .height(240)
        .columns([
            DataGridColumn::new("tipo", t!("comum-tipo")).width(ColumnWidth::Pixels(100.0)),
            DataGridColumn::new("escuta", t!("tuneis-escuta")).width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("destino", t!("tuneis-destino")).width(ColumnWidth::Star(1.2)),
            DataGridColumn::new("estado", t!("comum-estado")).width(ColumnWidth::Pixels(100.0)),
            DataGridColumn::new("abertas", t!("tuneis-abertas"))
                .width(ColumnWidth::Pixels(70.0))
                .align_end(),
            DataGridColumn::new("total", t!("tuneis-total"))
                .width(ColumnWidth::Pixels(60.0))
                .align_end(),
            DataGridColumn::new("enviado", t!("tuneis-enviado"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("recebido", t!("tuneis-recebido"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
        ])
        .empty_text(t!("tuneis-esta-conexao-nao-tem-tuneis-nem"));
    let botoes = {
        let (parar, ligar, novo) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        let abrir = Rc::clone(tela);
        Row::new()
            .gap(8)
            .child(
                Button::new(t!("comum-parar"))
                    .id("ativos-parar")
                    .icon(Symbol::Stop)
                    .disabled(move || !pode_parar.get())
                    .on_click(move || parar.mexer_no_ativo(false)),
            )
            .child(
                Button::new(t!("tuneis-ligar"))
                    .id("ativos-ligar")
                    .icon(Symbol::Play)
                    .disabled(move || !pode_ligar.get())
                    .on_click(move || ligar.mexer_no_ativo(true)),
            )
            .child(
                Button::new(t!("portas-abrir-navegador"))
                    .id("ativos-navegador")
                    .icon(Symbol::Globe)
                    .disabled(move || !pode_abrir.get())
                    .on_click(move || abrir.abrir_ativo_no_navegador()),
            )
            .child(
                Button::new(t!("tuneis-adicionar-tunel"))
                    .id("ativos-adicionar")
                    .icon(Symbol::Add)
                    .on_click(move || novo.adicionar_tunel_na_sessao()),
            )
    };
    let titulo = est.ativos_titulo;
    ContentDialog::new(move || titulo.get(), est.ativos_aberto)
        .id("ativos-dialogo")
        .content(
            Column::new().gap(8).child(botoes).child(grade).child(
                Text::new(move || detalhe.get())
                    .id("ativos-detalhe")
                    .caption()
                    .wrap()
                    .visible(move || !detalhe.get().is_empty()),
            ),
        )
        .close(t!("comum-fechar"))
}

/// Uma frase para o `InfoBar` de um túnel que não subiu ao abrir a aba: o primeiro com falha.
pub(super) fn falha_de_tunel(sessao: Session) -> Option<String> {
    let forwards = sessao.ssh_forwards()?;
    forwards.list().into_iter().find_map(|status| {
        if let ForwardState::Failed(motivo) = status.state {
            let que = match &status.forward {
                Forward::Local { bind, .. } => t!(
                    "tuneis-nao-subiu-local",
                    porta = i64::from(bind.port),
                    motivo = motivo.as_str()
                ),
                Forward::Remote { bind, .. } => t!(
                    "tuneis-nao-subiu-remoto",
                    porta = i64::from(bind.port),
                    motivo = motivo.as_str()
                ),
                Forward::Dynamic { bind } => t!(
                    "tuneis-nao-subiu-socks5",
                    porta = i64::from(bind.port),
                    motivo = motivo.as_str()
                ),
                Forward::Unix { remote_path, .. } => t!(
                    "tuneis-nao-subiu-socket",
                    caminho = remote_path.as_str(),
                    motivo = motivo.as_str()
                ),
            };
            Some(que)
        } else {
            None
        }
    })
}
