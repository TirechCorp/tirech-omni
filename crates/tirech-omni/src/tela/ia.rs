//! Os assistentes de IA no servidor remoto (M46): instalar e abrir o Claude Code, o Codex e o
//! OpenCode numa conexão SSH com um clique. A sondagem do servidor (o que está instalado, o que
//! falta para instalar) roda num canal `exec` numa thread; o resultado fica num cache de dez
//! minutos por conexão. A lógica pura (o script, a leitura, os comandos) está em [`crate::ia`].

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use tomni_ui::clipboard;
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::{ExecOutput, Forward};
use tomni_ui::terminal::{PaneDirection, Session};

use super::{Lugar, Tela};
use crate::abrir::{ExecDaSessao, Executor, PedidoComando, precisa_de_senha};
use crate::agente::{acordar, ler_contador};
use crate::configuracoes::{AbrirIa, MetodoIa};
use crate::ia::{
    self, COMANDO_SONDAGEM_WINDOWS, Falta, Ferramenta, Metodo, SCRIPT_SONDAGEM,
    SCRIPT_SONDAGEM_WINDOWS, Sondagem, VALIDADE_IA, ficha,
};
use crate::janelas::Janela;
use crate::modelo::{Connection, Kind, ShellRemoto};
use crate::t;

/// A porta em que o login do Codex espera a volta do navegador.
const PORTA_LOGIN_CODEX: u16 = 1455;

/// O método de instalação como a opção da lista das Configurações e das propriedades.
fn opcao_de(metodo: Option<Metodo>) -> MetodoIa {
    match metodo {
        None => MetodoIa::Automatico,
        Some(Metodo::Nativo) => MetodoIa::Nativo,
        Some(Metodo::Npm) => MetodoIa::Npm,
        Some(Metodo::Homebrew) => MetodoIa::Homebrew,
    }
}

/// O nome na tela do método de instalação (`None`: o automático).
pub(super) fn rotulo_do_metodo(metodo: Option<Metodo>) -> String {
    opcao_de(metodo).nome()
}

/// O inverso de [`rotulo_do_metodo`].
pub(super) fn metodo_do_rotulo(rotulo: &str) -> Option<Metodo> {
    MetodoIa::do_nome(rotulo).and_then(MetodoIa::metodo)
}

/// Os nomes de todos os métodos, para a lista das propriedades.
pub(super) fn opcoes_de_metodo() -> Vec<String> {
    MetodoIa::nomes()
}

/// De onde vem o pedido: define a conexão e a aba de que o assistente herda a pasta.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::large_enum_variant)]
pub(super) enum AlvoIa {
    /// Uma conexão da árvore (sem aba).
    Conexao(u64),
    /// Uma aba (o menu da aba).
    Sessao(Session),
    /// A aba ativa de uma janela (o botão da barra).
    Janela(Janela),
    /// A aba ativa da última janela ativa (o menu Ferramentas e o atalho).
    Ativa,
}

/// Uma conexão e, se o pedido veio de uma aba, a aba.
type Resolvido = (u64, Option<Session>);

/// O que a sondagem trouxe, ou o motivo de não ter trazido.
type Resultado = Result<Sondagem, String>;

/// O que o servidor tem de um assistente, para os rótulos dos menus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Achou {
    /// Ainda não se perguntou: o clique pergunta.
    Nao,
    Verificando,
    Instalada,
    Faltando,
    Falhou,
}

/// A sondagem de uma conexão e quando ela chegou.
struct Entrada {
    quando: Instant,
    resultado: Resultado,
}

/// O que fazer quando a sondagem de uma conexão chegar.
#[derive(Clone, Copy, Debug)]
enum Depois {
    /// Abrir o assistente (ou oferecer instalar, se não existe).
    Abrir(Ferramenta, Option<Session>),
    /// O diálogo "Abrir em outra pasta".
    OutraPasta(Option<Session>),
    /// Entrar (login) num assistente já instalado.
    Entrar(Ferramenta, Option<Session>),
    /// A aba de instalação acabou: conferir se deu certo.
    AposInstalar {
        ferramenta: Ferramenta,
        abrir_depois: bool,
        origem: Option<Session>,
    },
}

/// Uma instalação em andamento numa aba.
#[derive(Clone, Copy, Debug)]
struct Instalando {
    sessao: Session,
    conexao: u64,
    ferramenta: Ferramenta,
    abrir_depois: bool,
    origem: Option<Session>,
}

/// A mensagem do `InfoBar` de depois da instalação.
#[derive(Clone, Copy, Debug)]
struct AvisoIa {
    conexao: u64,
    ferramenta: Ferramenta,
    origem: Option<Session>,
}

/// Uma linha de aviso do diálogo de instalar: o que falta (ou merece aviso) no método escolhido.
#[derive(Clone, Debug, PartialEq)]
struct LinhaFalta {
    chave: &'static str,
    severidade: InfoBarSeverity,
    texto: String,
}

/// O que as threads de sondagem entregam à tela.
type Entregas = Arc<Mutex<Vec<(u64, Result<String, String>)>>>;

/// O estado dos assistentes de IA da tela: o cache, o diálogo de instalar, o de outra pasta e
/// os avisos.
pub(super) struct Estado {
    cache: RefCell<HashMap<u64, Entrada>>,
    /// As conexões com uma sondagem em andamento.
    verificando: RefCell<HashSet<u64>>,
    /// Sobe quando o cache ou a lista de "verificando" muda: os rótulos dos menus leem.
    versao: Signal<u64>,
    /// Sobe quando uma thread entrega.
    pronto: Signal<u64>,
    entregas: Entregas,
    esperando: RefCell<HashMap<u64, Vec<Depois>>>,
    instalando: Signal<Vec<Instalando>>,

    instalar_aberto: Signal<bool>,
    instalar_alvo: Signal<Option<(u64, Ferramenta)>>,
    instalar_metodo: Signal<String>,
    instalar_comando: Signal<String>,
    instalar_faltas: Signal<Vec<LinhaFalta>>,
    instalar_abrir_depois: Signal<bool>,
    /// Os métodos do diálogo, com o rótulo de cada um.
    instalar_opcoes: RefCell<Vec<(String, Metodo)>>,
    instalar_origem: Cell<Option<Session>>,

    pasta_aberto: Signal<bool>,
    pasta_ferramenta: Signal<String>,
    pasta_caminho: Signal<String>,
    pasta_lembrar: Signal<bool>,
    /// As ferramentas instaladas, na ordem da lista do diálogo.
    pasta_opcoes: RefCell<Vec<Ferramenta>>,
    pasta_alvo: Cell<Option<Resolvido>>,

    aviso_ok: Signal<Option<AvisoIa>>,
    aviso_erro: Signal<Option<AvisoIa>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            cache: RefCell::default(),
            verificando: RefCell::default(),
            versao: signal(0),
            pronto: signal(0),
            entregas: Arc::default(),
            esperando: RefCell::default(),
            instalando: signal(Vec::new()),
            instalar_aberto: signal(false),
            instalar_alvo: signal(None),
            instalar_metodo: signal(String::new()),
            instalar_comando: signal(String::new()),
            instalar_faltas: signal(Vec::new()),
            instalar_abrir_depois: signal(true),
            instalar_opcoes: RefCell::default(),
            instalar_origem: Cell::new(None),
            pasta_aberto: signal(false),
            pasta_ferramenta: signal(String::new()),
            pasta_caminho: signal(String::new()),
            pasta_lembrar: signal(false),
            pasta_opcoes: RefCell::default(),
            pasta_alvo: Cell::new(None),
            aviso_ok: signal(None),
            aviso_erro: signal(None),
        }
    }

    /// Avisa os rótulos de que o cache mudou.
    fn mudou(&self) {
        self.versao.update(|v| *v += 1);
    }
}

/// Como a sondagem chega ao servidor: pela conexão de uma aba aberta ou por uma nova (um
/// executor para o `sh -s` e outro para o PowerShell, que só roda se o primeiro mostrar um
/// servidor Windows).
enum Via {
    Sessao(ExecDaSessao),
    Executor(Executor, Executor),
}

type Rodada = Box<dyn FnOnce() -> Result<ExecOutput, String>>;

impl Via {
    /// Roda o script da sondagem no servidor. Bloqueia. Com `windows_primeiro` (o shell remoto
    /// da conexão é PowerShell ou Cmd), vai direto ao PowerShell; senão tenta o `sh -s` e cai
    /// no PowerShell se o servidor disser que não conhece o `sh`.
    fn rodar(self, windows_primeiro: bool) -> Result<String, String> {
        let (posix, windows): (Rodada, Rodada) = match self {
            Via::Sessao(exec) => {
                let outra = Arc::clone(&exec);
                (
                    Box::new(move || exec("sh -s", &mut Cursor::new(SCRIPT_SONDAGEM), &mut |_| {})),
                    Box::new(move || {
                        outra(
                            COMANDO_SONDAGEM_WINDOWS,
                            &mut Cursor::new(SCRIPT_SONDAGEM_WINDOWS),
                            &mut |_| {},
                        )
                    }),
                )
            }
            Via::Executor(sh, powershell) => (
                Box::new(move || sh("sh -s", SCRIPT_SONDAGEM.as_bytes())),
                Box::new(move || {
                    powershell(COMANDO_SONDAGEM_WINDOWS, SCRIPT_SONDAGEM_WINDOWS.as_bytes())
                }),
            ),
        };
        if windows_primeiro {
            return windows().map(|saida| saida.stdout_text());
        }
        let saida = posix()?;
        if Sondagem::ler(&saida.stdout_text()).is_none() && ia::parece_windows(&saida.stderr_text())
        {
            return windows().map(|saida| saida.stdout_text());
        }
        Ok(saida.stdout_text())
    }
}

impl Tela {
    /// A conexão `id`, se for SSH.
    pub(super) fn conexao_ssh(&self, id: u64) -> Option<Connection> {
        self.tree
            .with_untracked(|t| t.conexao_para_abrir(id))
            .filter(|c| c.kind == Kind::Ssh)
    }

    /// A conexão (SSH) e a aba de onde o pedido veio. `rastrear` diz se a leitura entra nas
    /// dependências de quem chama (rótulos e visibilidade de menus) ou não (cliques).
    fn resolver_ia(&self, alvo: AlvoIa, rastrear: bool) -> Option<Resolvido> {
        let ativa = |janela: Janela| {
            if rastrear {
                janela.layout.with(|l| l.pane(l.active()).selected)
            } else {
                self.sessao_ativa_de(janela)
            }
        };
        let sessao = match alvo {
            AlvoIa::Conexao(id) => return self.conexao_ssh(id).map(|_| (id, None)),
            AlvoIa::Sessao(s) => s,
            AlvoIa::Janela(j) => ativa(j)?,
            AlvoIa::Ativa => {
                let janela = if rastrear {
                    self.janelas
                        .achar(self.ativa.get())
                        .unwrap_or_else(|| self.janelas.principal())
                } else {
                    self.janela_ativa()
                };
                ativa(janela)?
            }
        };
        let id = self.conexao_da_sessao(sessao)?;
        self.conexao_ssh(id)?;
        Some((id, Some(sessao)))
    }

    /// O que a sondagem em cache diz de `ferramenta` na conexão `id`. Lê `versao`.
    fn achou(&self, id: u64, ferramenta: Ferramenta) -> Achou {
        let _ = self.ia.versao.get();
        match self.ia.cache.borrow().get(&id) {
            Some(Entrada {
                resultado: Ok(s), ..
            }) => {
                if s.instalada(ferramenta) {
                    Achou::Instalada
                } else {
                    Achou::Faltando
                }
            }
            Some(_) => Achou::Falhou,
            None if self.ia.verificando.borrow().contains(&id) => Achou::Verificando,
            None => Achou::Nao,
        }
    }

    /// Se o `alvo` é uma aba (ou conexão) SSH: os menus e o botão só aparecem então. Lê os
    /// signals da janela, para o menu acompanhar a aba ativa.
    pub(super) fn aba_ssh(&self, alvo: AlvoIa) -> bool {
        self.resolver_ia(alvo, true).is_some()
    }

    /// O rótulo do item de `ferramenta` nos menus.
    fn rotulo_ia(&self, alvo: AlvoIa, ferramenta: Ferramenta) -> String {
        let nome = ficha(ferramenta).nome;
        let estado = self
            .resolver_ia(alvo, true)
            .map_or(Achou::Nao, |(id, _)| self.achou(id, ferramenta));
        match estado {
            Achou::Nao | Achou::Instalada => t!("ia-remoto-abrir", nome = nome),
            Achou::Faltando => t!("ia-remoto-instalar", nome = nome),
            Achou::Verificando => t!("ia-remoto-verificando", nome = nome),
            Achou::Falhou => t!("ia-remoto-nao-verificou", nome = nome),
        }
    }

    /// A sondagem em cache de `id`, mesmo velha (o servidor raramente muda de uma hora para
    /// outra; a velha é renovada ao fundo).
    fn sondagem_de(&self, id: u64) -> Option<Resultado> {
        self.ia.cache.borrow().get(&id).map(|e| e.resultado.clone())
    }

    /// Uma aba aberta e viva da conexão `id`, para a sondagem reaproveitar a conexão dela.
    pub(super) fn sessao_viva_da_conexao(&self, id: u64) -> Option<Session> {
        self.sessoes_das_janelas(None)
            .into_iter()
            .map(|(_, s)| s)
            .find(|s| !s.closed() && self.conexao_da_sessao(*s) == Some(id))
    }

    /// Sonda o servidor da conexão `id` numa thread. Sem `forcar`, um cache de menos de dez
    /// minutos vale e uma sondagem em andamento não se repete.
    pub(super) fn sondar(self: &Rc<Self>, id: u64, forcar: bool) {
        if self.ia.verificando.borrow().contains(&id) {
            return;
        }
        let fresco = self
            .ia
            .cache
            .borrow()
            .get(&id)
            .is_some_and(|e| e.quando.elapsed() < VALIDADE_IA);
        if fresco && !forcar {
            return;
        }
        let Some(c) = self.conexao_ssh(id) else {
            return;
        };
        let efetiva = self.conexao_efetiva(&c);
        let senhas = self.senhas_do_cofre(&c);
        let via = if let Some(sessao) = self.sessao_viva_da_conexao(id)
            && let Ok(exec) = self.opener.exec_da_sessao(sessao)
        {
            Via::Sessao(exec)
        } else if precisa_de_senha(&c) && senhas.conexao.is_none() {
            // Sem a senha no cofre nem uma aba aberta, não há como perguntar sem diálogo.
            self.guardar_resultado(id, Err(t!("ia-remoto-sem-senha")));
            return;
        } else {
            Via::Executor(
                self.opener.executor(&efetiva, senhas.clone()),
                self.opener.executor(&efetiva, senhas),
            )
        };
        let windows_primeiro = matches!(
            efetiva.shell_remoto,
            ShellRemoto::PowerShell | ShellRemoto::Cmd
        );
        self.ia.verificando.borrow_mut().insert(id);
        self.ia.mudou();
        let (entregas, pronto) = (Arc::clone(&self.ia.entregas), self.ia.pronto);
        std::thread::spawn(move || {
            let saida = via.rodar(windows_primeiro);
            entregas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((id, saida));
            acordar(pronto);
        });
    }

    /// Guarda a sondagem de `id` no cache e roda o que esperava por ela.
    fn guardar_resultado(self: &Rc<Self>, id: u64, resultado: Resultado) {
        self.ia.verificando.borrow_mut().remove(&id);
        self.ia.cache.borrow_mut().insert(
            id,
            Entrada {
                quando: Instant::now(),
                resultado,
            },
        );
        self.ia.mudou();
        self.rodar_depois(id);
    }
}

impl Tela {
    /// Faz `depois` assim que a sondagem de `id` estiver à mão: já, se há uma no cache (a velha
    /// vale e é renovada ao fundo), ou quando a thread entregar.
    fn quando_sondado(self: &Rc<Self>, id: u64, depois: Depois) {
        match self.sondagem_de(id) {
            Some(resultado @ Ok(_)) => {
                self.sondar(id, false);
                self.executar_depois(id, depois, &resultado);
            }
            _ => {
                self.ia
                    .esperando
                    .borrow_mut()
                    .entry(id)
                    .or_default()
                    .push(depois);
                self.sondar(id, true);
            }
        }
    }

    /// Roda o que esperava pela sondagem de `id`.
    fn rodar_depois(self: &Rc<Self>, id: u64) {
        let pendentes = self
            .ia
            .esperando
            .borrow_mut()
            .remove(&id)
            .unwrap_or_default();
        let Some(resultado) = self.sondagem_de(id) else {
            return;
        };
        for depois in pendentes {
            self.executar_depois(id, depois, &resultado);
        }
    }

    fn executar_depois(self: &Rc<Self>, id: u64, depois: Depois, resultado: &Resultado) {
        let s = match resultado {
            Ok(s) => s,
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Warning,
                    t!("ia-remoto-nao-verificou-erro", erro = erro.as_str()),
                );
                return;
            }
        };
        match depois {
            Depois::Abrir(f, origem) => {
                if s.instalada(f) {
                    self.lancar(id, origem, f, s, None);
                } else {
                    self.pedir_instalacao(id, f, s, origem);
                }
            }
            Depois::OutraPasta(origem) => self.pedir_pasta(id, origem, s),
            Depois::Entrar(f, origem) => {
                if s.instalada(f) {
                    self.lancar_entrada(id, origem, f, s);
                } else {
                    self.pedir_instalacao(id, f, s, origem);
                }
            }
            Depois::AposInstalar {
                ferramenta,
                abrir_depois,
                origem,
            } => {
                if s.instalada(ferramenta) {
                    if abrir_depois {
                        self.lancar(id, origem, ferramenta, s, None);
                    } else {
                        self.ia.aviso_ok.set(Some(AvisoIa {
                            conexao: id,
                            ferramenta,
                            origem,
                        }));
                    }
                } else {
                    self.ia.aviso_erro.set(Some(AvisoIa {
                        conexao: id,
                        ferramenta,
                        origem,
                    }));
                }
            }
        }
    }

    /// Guarda o que as threads de sondagem entregaram.
    fn receber_sondagens(self: &Rc<Self>) {
        let lote = std::mem::take(
            &mut *self
                .ia
                .entregas
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for (id, saida) in lote {
            let resultado = saida.and_then(|texto| {
                Sondagem::ler(&texto).ok_or_else(|| t!("ia-remoto-resposta-ilegivel"))
            });
            self.guardar_resultado(id, resultado);
        }
    }

    /// Um assistente da conexão do `alvo`: abre, ou oferece instalar, ou sonda e depois age.
    pub(super) fn abrir_assistente(self: &Rc<Self>, alvo: AlvoIa, ferramenta: Ferramenta) {
        let Some((id, origem)) = self.resolver_ia(alvo, false) else {
            self.avisar(InfoBarSeverity::Informational, t!("ia-remoto-fora-do-ssh"));
            return;
        };
        self.quando_sondado(id, Depois::Abrir(ferramenta, origem));
    }

    /// O atalho: o assistente preferido na aba SSH ativa.
    pub(super) fn abrir_preferido(self: &Rc<Self>, alvo: AlvoIa) {
        let ferramenta = self.config.with_untracked(|c| c.ia.preferido.ferramenta());
        self.abrir_assistente(alvo, ferramenta);
    }

    /// "Entrar": o login sem navegador do assistente, numa aba nova.
    fn entrar_assistente(self: &Rc<Self>, alvo: AlvoIa, ferramenta: Ferramenta) {
        let Some((id, origem)) = self.resolver_ia(alvo, false) else {
            self.avisar(InfoBarSeverity::Informational, t!("ia-remoto-fora-do-ssh"));
            return;
        };
        self.quando_sondado(id, Depois::Entrar(ferramenta, origem));
    }

    /// "Abrir em outra pasta…".
    fn abrir_outra_pasta(self: &Rc<Self>, alvo: AlvoIa) {
        let Some((id, origem)) = self.resolver_ia(alvo, false) else {
            self.avisar(InfoBarSeverity::Informational, t!("ia-remoto-fora-do-ssh"));
            return;
        };
        self.quando_sondado(id, Depois::OutraPasta(origem));
    }

    /// "Verificar de novo": esquece o que se sabia e sonda.
    fn verificar_de_novo(self: &Rc<Self>, alvo: AlvoIa) {
        let Some((id, _)) = self.resolver_ia(alvo, false) else {
            self.avisar(InfoBarSeverity::Informational, t!("ia-remoto-fora-do-ssh"));
            return;
        };
        self.ia.cache.borrow_mut().remove(&id);
        self.ia.mudou();
        self.sondar(id, true);
    }

    /// A pasta em que o assistente abre: a do terminal de onde veio o pedido (OSC 7), a lembrada
    /// da conexão ou a pasta remota padrão. `None`: o home.
    fn pasta_da_ia(&self, c: &Connection, origem: Option<Session>) -> Option<String> {
        origem
            .filter(|o| self.conexao_da_sessao(*o) == Some(c.id))
            .and_then(|o| o.cwd())
            .filter(|p| !p.trim().is_empty())
            .or_else(|| Some(c.ia.pasta.trim().to_owned()).filter(|p| !p.is_empty()))
            .or_else(|| Some(c.pasta_remota.trim().to_owned()).filter(|p| !p.is_empty()))
    }

    /// Onde a aba do assistente entra: ao lado da aba de onde veio o pedido, num painel livre
    /// (ou num novo) com `PainelLivre`, ou onde uma conexão aberta entra.
    fn lugar_da_ia(self: &Rc<Self>, origem: Option<Session>, em: AbrirIa) -> Lugar {
        let da_origem = origem.and_then(|o| {
            self.sessoes_das_janelas(None)
                .into_iter()
                .find(|(_, s)| *s == o)
                .map(|(janela, _)| janela)
        });
        if em == AbrirIa::PainelLivre {
            let janela = da_origem.unwrap_or_else(|| self.janela_ativa());
            let painel = Cell::new(0);
            janela.layout.update(|l| {
                let livre = (0..l.pane_count()).find(|&n| l.pane(n).tabs.is_empty());
                painel.set(livre.unwrap_or_else(|| l.split(l.active(), PaneDirection::Right)));
            });
            return Lugar::painel(janela, painel.get());
        }
        match (origem, da_origem) {
            (Some(o), Some(janela)) => {
                let painel = janela.layout.with_untracked(|l| l.pane_of(o));
                Lugar {
                    janela: Some(janela.handle),
                    painel,
                }
            }
            _ => self.lugar_novo(),
        }
    }

    /// Abre `pedido` numa aba da conexão `c`. `None` se a conexão pede uma senha que não está
    /// no cofre (aqui não há como perguntar).
    fn abrir_pedido_ia(
        self: &Rc<Self>,
        c: &Connection,
        pedido: PedidoComando,
        lugar: Lugar,
    ) -> Option<Session> {
        let senhas = self.senhas_do_cofre(c);
        if precisa_de_senha(c) && senhas.conexao.is_none() {
            self.avisar(InfoBarSeverity::Warning, t!("ia-remoto-sem-senha"));
            return None;
        }
        let com_tuneis = !c.tuneis.is_empty() || !pedido.tuneis.is_empty();
        let n = self.contador.get_untracked() + 1;
        self.contador.set(n);
        let sessao = self
            .opener
            .open_comando(&self.conexao_efetiva(c), senhas, n, pedido);
        if com_tuneis {
            self.vigiar_tuneis_da_sessao(sessao);
        }
        let lugar = self.fixar(lugar);
        self.entra_na_grade(sessao, lugar);
        self.mostrar_terminais();
        Some(sessao)
    }

    /// Abre o assistente `ferramenta` na conexão `id`. `pasta` vence a da 8.2 do plano (a do
    /// terminal, a lembrada, a remota padrão).
    fn lancar(
        self: &Rc<Self>,
        id: u64,
        origem: Option<Session>,
        ferramenta: Ferramenta,
        s: &Sondagem,
        pasta: Option<String>,
    ) {
        let Some(c) = self.conexao_ssh(id) else {
            return;
        };
        let cfg = self.config.with_untracked(|c| c.ia.clone());
        let ficha = ficha(ferramenta);
        let argumentos =
            c.ia.argumentos
                .get(&ferramenta)
                .filter(|a| !a.trim().is_empty())
                .map_or_else(|| cfg.argumentos_de(ferramenta).to_owned(), Clone::clone);
        let pasta = pasta.or_else(|| self.pasta_da_ia(&c, origem));
        // Digitar na aba só vale num shell POSIX parado no prompt; senão, abre uma aba nova.
        let na_aba = origem.filter(|o| {
            cfg.abrir_em == AbrirIa::NestaAba
                && !s.windows
                && !o.closed()
                && !o.emulator(|e| e.alt_screen()).unwrap_or(false)
        });
        if let Some(sessao) = na_aba {
            let linha = ia::comando_digitar(ficha, s, pasta.as_deref(), &argumentos);
            sessao.send(format!("{linha}\r").as_bytes());
            return;
        }
        let mut tuneis = Vec::new();
        if ferramenta == Ferramenta::Codex && cfg.tunel_codex && !s.logado(Ferramenta::Codex) {
            if std::net::TcpListener::bind(("127.0.0.1", PORTA_LOGIN_CODEX)).is_ok() {
                tuneis.push(Forward::local(
                    PORTA_LOGIN_CODEX,
                    "localhost",
                    PORTA_LOGIN_CODEX,
                ));
            } else {
                self.avisar(InfoBarSeverity::Warning, t!("ia-remoto-porta-em-uso"));
            }
        }
        let pedido = PedidoComando {
            titulo: ficha.nome.to_owned(),
            comando: ia::comando_abrir(ficha, s, pasta.as_deref(), &argumentos, cfg.shell_ao_sair),
            tuneis,
        };
        let lugar = self.lugar_da_ia(origem, cfg.abrir_em);
        self.abrir_pedido_ia(&c, pedido, lugar);
    }

    /// O login de `ferramenta` numa aba nova.
    fn lancar_entrada(
        self: &Rc<Self>,
        id: u64,
        origem: Option<Session>,
        ferramenta: Ferramenta,
        s: &Sondagem,
    ) {
        let Some(c) = self.conexao_ssh(id) else {
            return;
        };
        let shell_ao_sair = self.config.with_untracked(|c| c.ia.shell_ao_sair);
        let ficha = ficha(ferramenta);
        let pedido = PedidoComando {
            titulo: t!("ia-remoto-aba-entrar", nome = ficha.nome),
            comando: ia::comando_entrar_aba(ficha, s, shell_ao_sair),
            tuneis: Vec::new(),
        };
        let lugar = self.lugar_da_ia(origem, AbrirIa::AbaNova);
        self.abrir_pedido_ia(&c, pedido, lugar);
    }

    /// O nome do método no diálogo de instalar, com o que falta, se algo impede.
    fn rotulo_do_metodo_ia(&self, ficha: &ia::Ficha, metodo: Metodo, s: &Sondagem) -> String {
        let base = match metodo {
            Metodo::Nativo => t!("ia-remoto-metodo-nativo"),
            Metodo::Npm => t!("ia-remoto-metodo-npm"),
            Metodo::Homebrew => t!("ia-remoto-metodo-homebrew"),
        };
        let falta = ia::faltas(ficha, metodo, s)
            .into_iter()
            .find(Falta::impede)
            .map(|f| match f {
                Falta::Curl => "curl",
                Falta::Tar => "tar",
                Falta::Bash => "bash",
                Falta::Npm => "npm",
                Falta::Brew => "Homebrew",
                _ => "Node",
            });
        match falta {
            Some(falta) => t!(
                "ia-remoto-metodo-falta",
                metodo = base.as_str(),
                falta = falta
            ),
            None => base,
        }
    }

    /// Abre o diálogo "Instalar X em <conexão>".
    fn pedir_instalacao(
        self: &Rc<Self>,
        id: u64,
        ferramenta: Ferramenta,
        s: &Sondagem,
        origem: Option<Session>,
    ) {
        let Some(c) = self.conexao_ssh(id) else {
            return;
        };
        let ficha = ficha(ferramenta);
        let metodos = ia::metodos_do_servidor(ficha, s);
        if metodos.is_empty() {
            return;
        }
        let da_config = self
            .config
            .with_untracked(|c| c.ia.metodo_de(ferramenta).metodo());
        let escolhido =
            c.ia.metodo
                .or(da_config)
                .filter(|m| metodos.contains(m))
                .unwrap_or_else(|| ia::metodo_automatico(ficha, s));
        let opcoes: Vec<(String, Metodo)> = metodos
            .iter()
            .map(|m| (self.rotulo_do_metodo_ia(ficha, *m, s), *m))
            .collect();
        let inicial = opcoes
            .iter()
            .find(|(_, m)| *m == escolhido)
            .or(opcoes.first())
            .map(|(r, _)| r.clone())
            .unwrap_or_default();
        *self.ia.instalar_opcoes.borrow_mut() = opcoes;
        self.ia.instalar_alvo.set(Some((id, ferramenta)));
        self.ia.instalar_origem.set(origem);
        self.ia.instalar_abrir_depois.set(true);
        self.ia.instalar_metodo.set(inicial);
        self.atualizar_instalacao();
        self.ia.instalar_aberto.set(true);
    }

    /// Refaz o comando e os avisos do diálogo para o método escolhido.
    fn atualizar_instalacao(&self) {
        let Some((id, ferramenta)) = self.ia.instalar_alvo.get_untracked() else {
            return;
        };
        let rotulo = self.ia.instalar_metodo.get_untracked();
        let metodo = self
            .ia
            .instalar_opcoes
            .borrow()
            .iter()
            .find(|(r, _)| *r == rotulo)
            .map(|(_, m)| *m);
        let (Some(metodo), Some(Ok(s))) = (metodo, self.sondagem_de(id)) else {
            return;
        };
        let ficha = ficha(ferramenta);
        self.ia
            .instalar_comando
            .set(ia::comando_instalar(ficha, metodo, &s));
        let linhas = ia::faltas(ficha, metodo, &s)
            .iter()
            .map(|f| linha_da_falta(ficha.nome, f))
            .collect();
        self.ia.instalar_faltas.set(linhas);
    }

    /// "Instalar": abre a aba que roda o instalador e passa a vigiá-la.
    fn confirmar_instalacao(self: &Rc<Self>) {
        let Some((id, ferramenta)) = self.ia.instalar_alvo.get_untracked() else {
            return;
        };
        let rotulo = self.ia.instalar_metodo.get_untracked();
        let metodo = self
            .ia
            .instalar_opcoes
            .borrow()
            .iter()
            .find(|(r, _)| *r == rotulo)
            .map(|(_, m)| *m);
        let (Some(metodo), Some(c), Some(Ok(s))) =
            (metodo, self.conexao_ssh(id), self.sondagem_de(id))
        else {
            return;
        };
        let origem = self.ia.instalar_origem.get();
        let abrir_depois = self.ia.instalar_abrir_depois.get_untracked();
        let ficha = ficha(ferramenta);
        let pedido = PedidoComando {
            titulo: t!("ia-remoto-aba-instalar", nome = ficha.nome),
            comando: ia::comando_instalar_aba(ficha, metodo, &s),
            tuneis: Vec::new(),
        };
        let lugar = self.lugar_da_ia(origem, AbrirIa::AbaNova);
        let Some(sessao) = self.abrir_pedido_ia(&c, pedido, lugar) else {
            return;
        };
        self.ia.aviso_ok.set(None);
        self.ia.aviso_erro.set(None);
        self.ia.instalando.update(|l| {
            l.push(Instalando {
                sessao,
                conexao: id,
                ferramenta,
                abrir_depois,
                origem,
            });
        });
    }

    /// As abas de instalação que fecharam: confere de novo o que o servidor tem.
    fn instalacoes_acabaram(self: &Rc<Self>, acabadas: Vec<Instalando>) {
        self.ia
            .instalando
            .update(|l| l.retain(|i| !acabadas.iter().any(|a| a.sessao == i.sessao)));
        for a in acabadas {
            self.ia.cache.borrow_mut().remove(&a.conexao);
            self.ia
                .esperando
                .borrow_mut()
                .entry(a.conexao)
                .or_default()
                .push(Depois::AposInstalar {
                    ferramenta: a.ferramenta,
                    abrir_depois: a.abrir_depois,
                    origem: a.origem,
                });
            self.sondar(a.conexao, true);
        }
    }

    /// Abre o diálogo "Abrir em outra pasta".
    fn pedir_pasta(self: &Rc<Self>, id: u64, origem: Option<Session>, s: &Sondagem) {
        let Some(c) = self.conexao_ssh(id) else {
            return;
        };
        let instaladas: Vec<Ferramenta> = Ferramenta::TODAS
            .into_iter()
            .filter(|f| s.instalada(*f))
            .collect();
        let primeira = instaladas.first().map_or_else(
            || t!("ia-remoto-pasta-nenhum"),
            |f| ficha(*f).nome.to_owned(),
        );
        *self.ia.pasta_opcoes.borrow_mut() = instaladas;
        self.ia.pasta_ferramenta.set(primeira);
        self.ia
            .pasta_caminho
            .set(self.pasta_da_ia(&c, origem).unwrap_or_default());
        self.ia.pasta_lembrar.set(false);
        self.ia.pasta_alvo.set(Some((id, origem)));
        self.ia.pasta_aberto.set(true);
    }

    /// "Abrir" do diálogo de outra pasta.
    fn confirmar_pasta(self: &Rc<Self>) {
        let Some((id, origem)) = self.ia.pasta_alvo.get() else {
            return;
        };
        let nome = self.ia.pasta_ferramenta.get_untracked();
        let Some(ferramenta) = self
            .ia
            .pasta_opcoes
            .borrow()
            .iter()
            .copied()
            .find(|f| ficha(*f).nome == nome)
        else {
            return;
        };
        let caminho = self.ia.pasta_caminho.get_untracked().trim().to_owned();
        if self.ia.pasta_lembrar.get_untracked() {
            let lembrada = caminho.clone();
            self.tree.update(|t| {
                if let Some(c) = t.connection_mut(id) {
                    c.ia.pasta = lembrada;
                }
            });
            self.atualizar();
        }
        let Some(Ok(s)) = self.sondagem_de(id) else {
            return;
        };
        self.lancar(
            id,
            origem,
            ferramenta,
            &s,
            Some(caminho).filter(|p| !p.is_empty()),
        );
    }

    /// O nome da conexão `id`.
    fn nome_da_conexao_ia(&self, id: u64) -> String {
        self.tree
            .with_untracked(|t| t.connection(id).map(|c| c.name.clone()))
            .unwrap_or_default()
    }
}

/// O aviso do diálogo de instalar para uma falta.
fn linha_da_falta(nome: &str, falta: &Falta) -> LinhaFalta {
    use InfoBarSeverity::{Informational, Warning};
    let (chave, severidade, texto) = match falta {
        Falta::Curl => ("curl", Warning, t!("ia-remoto-falta-curl")),
        Falta::Tar => ("tar", Warning, t!("ia-remoto-falta-tar")),
        Falta::Bash => ("bash", Warning, t!("ia-remoto-falta-bash")),
        Falta::Npm => ("npm", Warning, t!("ia-remoto-falta-npm")),
        Falta::Brew => ("brew", Warning, t!("ia-remoto-falta-brew")),
        Falta::Node {
            tem: None, minimo, ..
        } => (
            "node",
            Warning,
            t!(
                "ia-remoto-sem-node",
                nome = nome,
                minimo = i64::from(*minimo)
            ),
        ),
        Falta::Node {
            tem: Some(tem),
            minimo,
            em_execucao: true,
        } => (
            "node",
            Warning,
            t!(
                "ia-remoto-falta-node-rodar",
                nome = nome,
                minimo = i64::from(*minimo),
                tem = i64::from(*tem)
            ),
        ),
        Falta::Node {
            tem: Some(tem),
            minimo,
            em_execucao: false,
        } => (
            "node",
            Informational,
            t!(
                "ia-remoto-falta-node-instalar",
                nome = nome,
                minimo = i64::from(*minimo),
                tem = i64::from(*tem)
            ),
        ),
        Falta::Arquitetura(arq) => (
            "arquitetura",
            Warning,
            t!("ia-remoto-falta-arquitetura", arq = arq.as_str()),
        ),
        Falta::NpmSemPermissao => (
            "npm-sem-permissao",
            Informational,
            t!("ia-remoto-falta-npm-sem-permissao"),
        ),
        Falta::Alpine => ("alpine", Warning, t!("ia-remoto-falta-alpine")),
    };
    LinhaFalta {
        chave,
        severidade,
        texto,
    }
}

/// O submenu "Assistentes de IA" de um menu: os três assistentes, "Abrir em outra pasta…",
/// "Entrar ▸" e "Verificar de novo". `prefixo` abre os ids dos itens.
pub(super) fn submenu_ia(tela: &Rc<Tela>, prefixo: &str, alvo: AlvoIa) -> MenuFlyout {
    let icones = [
        (Ferramenta::Claude, Symbol::Sparkle),
        (Ferramenta::Codex, Symbol::Code),
        (Ferramenta::OpenCode, Symbol::WindowConsole),
    ];
    let mut menu = MenuFlyout::new().id(prefixo.to_owned());
    for (ferramenta, icone) in icones {
        let (rotulo, abrir, visivel) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        menu = menu
            .item(
                format!("{prefixo}-{}", ferramenta.chave()),
                move || rotulo.rotulo_ia(alvo, ferramenta),
                move || abrir.abrir_assistente(alvo, ferramenta),
            )
            .icon(icone)
            .item_visible(move || visivel.config.with(|c| c.ia.mostra(ferramenta)));
    }
    let mut entrar = MenuFlyout::new().id(format!("{prefixo}-entrar"));
    let entradas = [
        (Ferramenta::Claude, t!("ia-remoto-entrar-claude")),
        (Ferramenta::Codex, t!("ia-remoto-entrar-codex")),
        (Ferramenta::OpenCode, t!("ia-remoto-entrar-opencode")),
    ];
    for (ferramenta, rotulo) in entradas {
        let (age, visivel) = (Rc::clone(tela), Rc::clone(tela));
        entrar = entrar
            .item(
                format!("{prefixo}-entrar-{}", ferramenta.chave()),
                rotulo,
                move || age.entrar_assistente(alvo, ferramenta),
            )
            .item_visible(move || {
                // Só some quando se sabe que não está instalado: a ferramenta não existe para entrar.
                visivel.config.with(|c| c.ia.mostra(ferramenta))
                    && visivel
                        .resolver_ia(alvo, true)
                        .is_none_or(|(id, _)| visivel.achou(id, ferramenta) != Achou::Faltando)
            });
    }
    let (pasta, verificar) = (Rc::clone(tela), Rc::clone(tela));
    menu.separator()
        .item(
            format!("{prefixo}-outra-pasta"),
            t!("ia-remoto-outra-pasta"),
            move || pasta.abrir_outra_pasta(alvo),
        )
        .icon(Symbol::FolderOpen)
        .submenu(format!("{prefixo}-entrar"), t!("ia-remoto-entrar"), entrar)
        .item(
            format!("{prefixo}-verificar"),
            t!("ia-remoto-verificar"),
            move || verificar.verificar_de_novo(alvo),
        )
        .icon(Symbol::ArrowSync)
}

/// O botão da barra, com o submenu: aparece quando a aba ativa da janela é SSH e
/// `ia.mostrar_botao` está ligado.
pub(super) fn botao(tela: &Rc<Tela>, janela: Janela) -> Button {
    let visivel = Rc::clone(tela);
    let dica = Rc::clone(tela);
    Button::icon_only(Symbol::Sparkle)
        .id("ia-botao")
        .tooltip(move || {
            let atalho = dica
                .config
                .with(|c| crate::atalhos::texto(&crate::atalhos::atalhos_de(c, "ia_abrir")));
            t!("ia-remoto-botao-dica", atalho = atalho)
        })
        .flyout(submenu_ia(tela, "ia-botao", AlvoIa::Janela(janela)))
        .visible(move || {
            visivel
                .config
                .with(|c| c.geral.assistente_ia && c.ia.mostrar_botao)
                && visivel.aba_ssh(AlvoIa::Janela(janela))
        })
}

/// O diálogo "Instalar X em <conexão>".
pub(super) fn dialogo_instalar(tela: &Rc<Tela>) -> ContentDialog {
    let est = &tela.ia;
    let (aberto, alvo, metodo, comando, faltas, abrir_depois) = (
        est.instalar_aberto,
        est.instalar_alvo,
        est.instalar_metodo,
        est.instalar_comando,
        est.instalar_faltas,
        est.instalar_abrir_depois,
    );
    let (titulo, opcoes, confirmar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
    ContentDialog::new(
        move || match alvo.get() {
            Some((id, ferramenta)) => {
                let conexao = titulo.nome_da_conexao_ia(id);
                t!(
                    "ia-remoto-instalar-titulo",
                    nome = ficha(ferramenta).nome,
                    conexao = conexao.as_str()
                )
            }
            None => String::new(),
        },
        aberto,
    )
    .id("ia-instalar")
    .content(
        Column::new()
            .gap(8)
            .child(Text::new(t!("ia-remoto-instalar-texto")).secondary().wrap())
            .child(Show::when(
                move || aberto.get(),
                move || {
                    let rotulos: Vec<String> = opcoes
                        .ia
                        .instalar_opcoes
                        .borrow()
                        .iter()
                        .map(|(r, _)| r.clone())
                        .collect();
                    RadioButtons::bind(metodo, rotulos).id("ia-instalar-metodo")
                },
            ))
            .child(Text::new(t!("ia-remoto-instalar-comando")).caption())
            .child(
                TextEditor::bind(comando)
                    .id("ia-instalar-comando")
                    .read_only(true)
                    .monospace()
                    .wrap(true)
                    .min_height(36.0)
                    .max_height(120.0),
            )
            .child(
                Row::new().child(
                    Button::new(t!("ia-remoto-instalar-copiar"))
                        .id("ia-instalar-copiar")
                        .icon(Symbol::Copy)
                        .on_click(move || clipboard::write(comando.get_untracked())),
                ),
            )
            .child(For::each(
                faltas,
                |l: &LinhaFalta| (l.chave, l.texto.clone()),
                |l: LinhaFalta| {
                    InfoBar::new(l.severidade)
                        .id(format!("ia-instalar-falta-{}", l.chave))
                        .message(l.texto)
                },
            ))
            .child(
                CheckBox::bind(abrir_depois, t!("ia-remoto-instalar-abrir-depois"))
                    .id("ia-instalar-abrir-depois"),
            ),
    )
    .primary(t!("ia-remoto-instalar-ok"), move || {
        confirmar.confirmar_instalacao();
    })
    .close(t!("comum-cancelar"))
}

/// O diálogo "Abrir em outra pasta".
pub(super) fn dialogo_pasta(tela: &Rc<Tela>) -> ContentDialog {
    let est = &tela.ia;
    let (aberto, ferramenta, caminho, lembrar) = (
        est.pasta_aberto,
        est.pasta_ferramenta,
        est.pasta_caminho,
        est.pasta_lembrar,
    );
    let (opcoes, confirmar) = (Rc::clone(tela), Rc::clone(tela));
    ContentDialog::new(t!("ia-remoto-pasta-titulo"), aberto)
        .id("ia-pasta")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(t!("ia-remoto-pasta-ferramenta")).caption())
                .child(Show::when(
                    move || aberto.get(),
                    move || {
                        let mut nomes: Vec<String> = opcoes
                            .ia
                            .pasta_opcoes
                            .borrow()
                            .iter()
                            .map(|f| ficha(*f).nome.to_owned())
                            .collect();
                        if nomes.is_empty() {
                            nomes.push(t!("ia-remoto-pasta-nenhum"));
                        }
                        ComboBox::bind(ferramenta, nomes).id("ia-pasta-ferramenta")
                    },
                ))
                .child(Text::new(t!("ia-remoto-pasta-caminho")).caption())
                .child(
                    Edit::bind(caminho)
                        .id("ia-pasta-caminho")
                        .placeholder("/srv/app"),
                )
                .child(
                    CheckBox::bind(lembrar, t!("ia-remoto-pasta-lembrar")).id("ia-pasta-lembrar"),
                ),
        )
        .primary(t!("ia-remoto-pasta-abrir"), move || {
            confirmar.confirmar_pasta();
        })
        .close(t!("comum-cancelar"))
}

/// Os avisos do fim da instalação, no alto da janela.
pub(super) fn avisos(tela: &Rc<Tela>) -> impl View {
    let (ok, erro) = (tela.ia.aviso_ok, tela.ia.aviso_erro);
    let (t_ok, t_acao_ok, t_acao_erro) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
    Column::new()
        .child(Show::when(
            move || ok.get().is_some(),
            move || {
                let (mensagem, agir) = (Rc::clone(&t_ok), Rc::clone(&t_acao_ok));
                InfoBar::new(InfoBarSeverity::Success)
                    .id("ia-aviso")
                    .message(move || match ok.get() {
                        Some(a) => mensagem.mensagem_instalado(a),
                        None => String::new(),
                    })
                    .on_close(move || ok.set(None))
                    .action(t!("ia-remoto-abrir-agora"), move || {
                        if let Some(a) = ok.get_untracked() {
                            ok.set(None);
                            agir.quando_sondado(a.conexao, Depois::Abrir(a.ferramenta, a.origem));
                        }
                    })
            },
        ))
        .child(Show::when(
            move || erro.get().is_some(),
            move || {
                let agir = Rc::clone(&t_acao_erro);
                InfoBar::new(InfoBarSeverity::Error)
                    .id("ia-aviso-erro")
                    .message(move || match erro.get() {
                        Some(a) => {
                            let nome = ficha(a.ferramenta).nome;
                            let aba = t!("ia-remoto-aba-instalar", nome = nome);
                            t!(
                                "ia-remoto-instalacao-falhou",
                                nome = nome,
                                aba = aba.as_str()
                            )
                        }
                        None => String::new(),
                    })
                    .on_close(move || erro.set(None))
                    .action(t!("ia-remoto-tentar-de-novo"), move || {
                        if let Some(a) = erro.get_untracked() {
                            erro.set(None);
                            agir.quando_sondado(a.conexao, Depois::Abrir(a.ferramenta, a.origem));
                        }
                    })
            },
        ))
}

impl Tela {
    /// "Claude Code instalado em <conexão> (2.1.211)."
    fn mensagem_instalado(&self, a: AvisoIa) -> String {
        let nome = ficha(a.ferramenta).nome;
        let conexao = self.nome_da_conexao_ia(a.conexao);
        let versao = match self.sondagem_de(a.conexao) {
            Some(Ok(s)) => s.versao_de(a.ferramenta).map(str::to_owned),
            _ => None,
        };
        match versao {
            Some(versao) => t!(
                "ia-remoto-instalado",
                nome = nome,
                conexao = conexao.as_str(),
                versao = versao.as_str()
            ),
            None => t!(
                "ia-remoto-instalado-sem-versao",
                nome = nome,
                conexao = conexao.as_str()
            ),
        }
    }
}

/// Liga os efeitos dos assistentes: o que as threads entregam, o fim das instalações e a
/// verificação ao abrir uma aba SSH. Chame uma vez, ao montar a tela.
pub(super) fn ligar(tela: &Rc<Tela>) {
    let (eu, pronto) = (tela.eu.clone(), tela.ia.pronto);
    effect(move || {
        let _ = ler_contador(pronto);
        if let Some(t) = eu.upgrade() {
            untrack(|| t.receber_sondagens());
        }
    });

    let (eu, instalando) = (tela.eu.clone(), tela.ia.instalando);
    effect(move || {
        let acabadas: Vec<Instalando> =
            instalando.with(|l| l.iter().filter(|i| i.sessao.closed()).copied().collect());
        if acabadas.is_empty() {
            return;
        }
        if let Some(t) = eu.upgrade() {
            untrack(|| t.instalacoes_acabaram(acabadas));
        }
    });

    let (eu, metodo) = (tela.eu.clone(), tela.ia.instalar_metodo);
    effect(move || {
        let _ = metodo.get();
        if let Some(t) = eu.upgrade() {
            untrack(|| t.atualizar_instalacao());
        }
    });

    let eu = tela.eu.clone();
    effect(move || {
        let Some(t) = eu.upgrade() else {
            return;
        };
        if !t.config.with(|c| c.ia.verificar_ao_abrir) {
            return;
        }
        if let Some((id, _)) = t.resolver_ia(AlvoIa::Ativa, true) {
            untrack(|| t.sondar(id, false));
        }
    });
}
