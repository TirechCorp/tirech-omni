//! O envio ao modelo (M52): junta o contexto do terminal, tira os segredos, mostra "O que vai
//! sair" e o aviso de internet, abre o pedido numa thread e entrega a resposta aos poucos.
//!
//! As três telas da IA (o Flyout do comando, as explicações e o chat) usam [`Tela::enviar_ia`]:
//! cada uma entrega um [`Envio`] e recebe [`Chegou`] num receptor. A IA **nunca executa** nada:
//! este módulo só manda texto e traz texto.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::ai::{self, AiErrorKind, Event, Locality, Message, Request, StreamHandle, Zeroizing};
use tomni_ui::files::Opener as ArquivosOpener;
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use super::ia_perfis::{host_da_url, selo_da_localidade};
use crate::agente::{acordar, ler_contador};
use crate::assistente::contexto::{self, ArquivosDoServidor, Contexto, SistemaRemoto};
use crate::assistente::envio::{Preparado, preparar_contexto};
use crate::assistente::perfis::PerfilIa;
use crate::assistente::prompts::{self, Partes};
use crate::cofre::Cofre;
use crate::espacos::id_da_chave;
use crate::modelo::Kind;
use crate::redacao::Redator;
use crate::t;

/// Para que o pedido serve: muda o prompt de sistema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Modo {
    /// Gerar um comando.
    Comando,
    /// Explicar e conversar.
    Conversa,
}

/// O que do terminal entra no pedido.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Incluir {
    /// O texto selecionado.
    pub selecao: bool,
    /// As últimas linhas do terminal.
    pub linhas: bool,
    /// O último comando que deu erro, com a saída.
    pub erro: bool,
    /// A frase do sistema (SO e shell) no prompt de sistema.
    pub sistema: bool,
}

/// Como o texto do usuário é montado a partir do pedido e das partes do terminal.
pub(super) type Montar = Box<dyn FnOnce(&str, &Partes<'_>) -> String>;

/// O que se entrega ao motor.
pub(super) struct Envio {
    /// A aba de onde vem o contexto (`None`: sem terminal, só a conversa).
    pub sessao: Option<Session>,
    /// O provedor.
    pub perfil: PerfilIa,
    /// Para que serve.
    pub modo: Modo,
    /// O que do terminal entra.
    pub incluir: Incluir,
    /// As mensagens anteriores da conversa, já limpas e sem segredos.
    pub historico: Vec<Message>,
    /// O que a pessoa escreveu (ainda sem redação).
    pub pedido: String,
    /// Monta o texto da mensagem nova.
    pub montar: Montar,
    /// Quem recebe a resposta.
    pub receptor: Receptor,
}

/// O que chega a quem pediu.
pub(super) enum Chegou {
    /// O pedido saiu: isto é exatamente o que foi mandado.
    Saiu(Rc<Preparado>),
    /// Mais um pedaço da resposta.
    Texto(String),
    /// A resposta acabou (`Err`: a mensagem do erro).
    Fim(Result<(), String>),
    /// A pessoa desistiu numa das caixas de confirmação: nada saiu.
    Desistiu,
}

/// Quem recebe o que chega, na thread da interface.
pub(super) type Receptor = Rc<dyn Fn(Chegou)>;

/// O que a thread do pedido empilha.
enum DaThread {
    Texto(String),
    Fim(Result<(), String>),
}

/// Em que passo das confirmações o pedido está parado.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Etapa {
    Internet,
    OQueSai,
}

/// Um pedido parado numa caixa de confirmação.
struct Pendente {
    id: u64,
    etapa: Etapa,
    /// A chave da aba (vazia: sem terminal).
    aba: String,
    perfil: PerfilIa,
    preparado: Rc<Preparado>,
    /// Se "O que vai sair" ainda vai aparecer depois do aviso de internet.
    mostrar: bool,
}

/// Um pedido em andamento.
struct Ativo {
    cancelar: Arc<AtomicBool>,
    handle: Arc<Mutex<Option<StreamHandle>>>,
}

/// O estado do motor, na `Tela`.
pub(super) struct Motor {
    pronto: Signal<u64>,
    fila: Arc<Mutex<Vec<(u64, DaThread)>>>,
    proximo: Cell<u64>,
    receptores: RefCell<HashMap<u64, Receptor>>,
    ativos: RefCell<HashMap<u64, Ativo>>,
    pendente: RefCell<Option<Pendente>>,
    /// As abas que já viram "O que vai sair" e marcaram para não perguntar de novo.
    confirmadas: RefCell<HashSet<String>>,
    /// O provedor que a pessoa escolheu para a aba (a chave da sessão), no lugar do da conexão.
    escolhas: RefCell<HashMap<String, u64>>,
    /// O sistema lido do servidor de cada aba.
    sistemas: Arc<Mutex<HashMap<String, SistemaRemoto>>>,
    /// As abas cujo sistema está sendo lido.
    lendo: RefCell<HashSet<String>>,
    pub internet_aberto: Signal<bool>,
    pub internet_texto: Signal<String>,
    pub sai_aberto: Signal<bool>,
    pub sai_titulo: Signal<String>,
    pub sai_destino: Signal<String>,
    pub sai_texto: Signal<String>,
    pub sai_instrucoes: Signal<String>,
    pub sai_instrucoes_abertas: Signal<bool>,
    pub sai_avisos: Signal<String>,
    pub sai_nao_aba: Signal<bool>,
    pub sai_nao_mais: Signal<bool>,
}

impl Motor {
    pub(super) fn novo() -> Self {
        Self {
            pronto: signal(0),
            fila: Arc::default(),
            proximo: Cell::new(0),
            receptores: RefCell::default(),
            ativos: RefCell::default(),
            pendente: RefCell::default(),
            confirmadas: RefCell::default(),
            escolhas: RefCell::default(),
            sistemas: Arc::default(),
            lendo: RefCell::default(),
            internet_aberto: signal(false),
            internet_texto: signal(String::new()),
            sai_aberto: signal(false),
            sai_titulo: signal(String::new()),
            sai_destino: signal(String::new()),
            sai_texto: signal(String::new()),
            sai_instrucoes: signal(String::new()),
            sai_instrucoes_abertas: signal(false),
            sai_avisos: signal(String::new()),
            sai_nao_aba: signal(true),
            sai_nao_mais: signal(false),
        }
    }

    fn empilhar(&self) -> impl Fn(u64, DaThread) + Send + 'static {
        let (fila, pronto) = (Arc::clone(&self.fila), self.pronto);
        move |id, item| {
            fila.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((id, item));
            acordar(pronto);
        }
    }

    /// O provedor que a pessoa escolheu para a aba `chave`.
    pub(super) fn escolha_da_aba(&self, chave: &str) -> Option<u64> {
        self.escolhas.borrow().get(chave).copied()
    }

    /// Esquece o que se sabe da aba `chave` (ela fechou).
    pub(super) fn esquecer_aba(&self, chave: &str) {
        self.escolhas.borrow_mut().remove(chave);
        self.confirmadas.borrow_mut().remove(chave);
        self.lendo.borrow_mut().remove(chave);
        self.sistemas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(chave);
    }
}

/// Os valores do cofre que não podem sair: senhas, campos ocultos e o corpo das chaves privadas.
fn segredos_do_cofre(cofre: &Cofre) -> Vec<Zeroizing<String>> {
    let mut segredos = Vec::new();
    let mut guardar = |texto: &str| {
        if !texto.is_empty() {
            segredos.push(Zeroizing::new(texto.to_owned()));
        }
    };
    for item in cofre.itens() {
        guardar(&item.senha);
        for campo in item.campos.iter().filter(|c| c.oculto) {
            guardar(&campo.valor);
        }
        if let Some(chave) = &item.chave {
            guardar(&chave.privada);
            // Uma linha do corpo da chave também não pode sair, caso o terminal a mostre quebrada.
            for linha in chave.privada.lines().filter(|l| !l.starts_with("-----")) {
                guardar(linha.trim());
            }
        }
    }
    segredos
}

/// Lê um arquivo de texto do servidor, no máximo 256 KiB.
fn ler_arquivo(fs: &mut dyn tomni_ui::files::Fs, caminho: &str) -> Option<String> {
    let mut baixando = fs.open_read(caminho, 0).ok()?;
    let mut texto = String::new();
    baixando
        .by_ref()
        .take(256 * 1024)
        .read_to_string(&mut texto)
        .ok()?;
    Some(texto)
}

/// Os arquivos que dizem o sistema do servidor, pelo SFTP.
fn arquivos_do_servidor(abrir: &ArquivosOpener) -> ArquivosDoServidor {
    let Ok(mut fs) = abrir() else {
        return ArquivosDoServidor::default();
    };
    ArquivosDoServidor {
        os_release: ler_arquivo(fs.as_mut(), "/etc/os-release"),
        passwd: ler_arquivo(fs.as_mut(), "/etc/passwd"),
        plist: ler_arquivo(
            fs.as_mut(),
            "/System/Library/CoreServices/SystemVersion.plist",
        ),
        windows: fs.stat("C:/Windows").is_ok(),
    }
}

impl Tela {
    /// O provedor de uma aba: o que a pessoa escolheu para ela, senão o da conexão, senão o
    /// padrão.
    pub(super) fn perfil_da_aba(&self, sessao: Option<&Session>) -> Option<PerfilIa> {
        let chave = sessao.map(Session::key_str).unwrap_or_default();
        let escolhido = self.assistente.motor.escolha_da_aba(&chave).or_else(|| {
            let id = id_da_chave(&chave)?;
            self.tree.with_untracked(|t| t.connection(id)?.perfil_ia)
        });
        self.assistente
            .perfis
            .with_untracked(|a| a.perfil_da_conexao(escolhido).cloned())
    }

    /// A aba `chave` passa a usar o provedor `id`.
    pub(super) fn trocar_perfil_da_aba(&self, chave: &str, id: u64) {
        self.assistente
            .motor
            .escolhas
            .borrow_mut()
            .insert(chave.to_owned(), id);
    }

    /// O redator da redação: o cofre e os extras (`ia.redigir_extras`). `None`: a redação está
    /// desligada. O segundo valor diz se o cofre estava bloqueado (só os padrões comuns).
    pub(super) fn redator_da_ia(&self) -> (Option<Redator>, bool) {
        let (redigir, extras) = self
            .config
            .with_untracked(|c| (c.ia.redigir, c.ia.redigir_extras.clone()));
        if !redigir {
            return (None, false);
        }
        let bloqueado = self.cofre.agora(Cofre::bloqueado);
        let segredos = if bloqueado {
            Vec::new()
        } else {
            self.cofre.agora(segredos_do_cofre)
        };
        (Some(Redator::novo(segredos, &extras)), bloqueado)
    }

    /// O sistema do terminal da aba, para o prompt: lido do servidor (SFTP) quando já deu, senão
    /// suposto. Um shell local diz o próprio sistema.
    fn sistema_da_aba(&self, sessao: &Session) -> SistemaRemoto {
        let suposto = self.config.with_untracked(|c| c.ia.comando_shell.clone());
        let chave = sessao.key_str();
        let conexao = id_da_chave(&chave)
            .and_then(|id| self.tree.with_untracked(|t| t.conexao_para_abrir(id)));
        let Some(conexao) = conexao else {
            return contexto::sistema_local(&suposto);
        };
        if conexao.kind == Kind::Local {
            return contexto::sistema_local(&suposto);
        }
        let lido = self
            .assistente
            .motor
            .sistemas
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&chave)
            .cloned();
        lido.unwrap_or_else(|| {
            self.ler_sistema_da_aba(sessao);
            SistemaRemoto::suposto(&suposto)
        })
    }

    /// Lê o sistema e o shell do servidor pelo SFTP, numa thread, e guarda para os próximos
    /// pedidos. Chamar ao abrir uma tela da IA adianta a resposta do primeiro pedido.
    pub(super) fn ler_sistema_da_aba(&self, sessao: &Session) {
        let motor = &self.assistente.motor;
        let chave = sessao.key_str();
        let ja = motor.lendo.borrow().contains(&chave)
            || motor
                .sistemas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .contains_key(&chave);
        if ja {
            return;
        }
        let Some(conexao) = id_da_chave(&chave)
            .and_then(|id| self.tree.with_untracked(|t| t.conexao_para_abrir(id)))
        else {
            return;
        };
        if !self.opener.sessao_tem_arquivos(*sessao) {
            return;
        }
        let Ok(abrir) = self.opener.arquivos_da_sessao(*sessao) else {
            return;
        };
        motor.lendo.borrow_mut().insert(chave.clone());
        let suposto = self.config.with_untracked(|c| c.ia.comando_shell.clone());
        let guardados = Arc::clone(&motor.sistemas);
        std::thread::spawn(move || {
            let arquivos = arquivos_do_servidor(&abrir);
            let sistema = contexto::sistema_do_servidor(&arquivos, &conexao.user, &suposto);
            guardados
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(chave, sistema);
        });
    }

    /// Junta o contexto do terminal da aba, do jeito que `incluir` pede.
    fn contexto_da_aba(&self, sessao: Option<&Session>, incluir: Incluir) -> Contexto {
        let Some(sessao) = sessao else {
            return Contexto::default();
        };
        let (n, max) = self
            .config
            .with_untracked(|c| (c.ia.linhas_contexto as usize, c.ia.max_caracteres as usize));
        let mut cortadas = 0;
        let mut limitar = |texto: String| {
            let (texto, cortou) = contexto::limitar_caracteres(&texto, max);
            cortadas += cortou;
            texto
        };
        let selecao = if incluir.selecao {
            sessao.selection_text()
        } else {
            None
        };
        let historico = (incluir.linhas || incluir.erro).then(|| sessao.history());
        let linhas = historico
            .as_deref()
            .filter(|_| incluir.linhas)
            .map(|h| limitar(contexto::ultimas_linhas(h, n).0));
        let ultimo_erro = historico
            .as_deref()
            .filter(|_| incluir.erro)
            .map(|h| contexto::ultimo_erro(h, n))
            .map(|mut e| {
                e.saida = limitar(std::mem::take(&mut e.saida));
                e
            });
        Contexto {
            selecao,
            linhas,
            ultimo_erro,
            sistema: incluir.sistema.then(|| self.sistema_da_aba(sessao)),
            cortadas,
        }
    }

    /// Prepara o que vai sair: contexto limpo e sem segredos, prompt de sistema e a mensagem nova.
    fn preparar_envio(&self, envio: Envio) -> (Preparado, PerfilIa, String, Receptor) {
        let Envio {
            sessao,
            perfil,
            modo,
            incluir,
            historico,
            pedido,
            montar,
            receptor,
        } = envio;
        let (redator, bloqueado) = self.redator_da_ia();
        let ctx = self.contexto_da_aba(sessao.as_ref(), incluir);
        let (partes, mut contagem) = preparar_contexto(&ctx, redator.as_ref());
        let (pedido, do_pedido) = prompts::preparar(&pedido, redator.as_ref());
        contagem.somar(do_pedido);
        let usuario = montar(&pedido, &partes.partes());
        let sistema = match modo {
            Modo::Comando => prompts::sistema_comando(ctx.sistema.as_ref()),
            Modo::Conversa => prompts::sistema_conversa(ctx.sistema.as_ref()),
        };
        let mut mensagens = historico;
        mensagens.push(Message::user(usuario));
        let preparado = Preparado {
            sistema,
            mensagens,
            contagem,
            cortadas: ctx.cortadas,
            bloqueado: bloqueado && redator.is_some(),
        };
        let aba = sessao.as_ref().map(Session::key_str).unwrap_or_default();
        (preparado, perfil, aba, receptor)
    }

    /// Manda o pedido. Passa pelo aviso de internet (uma vez por provedor) e por "O que vai sair"
    /// (antes do primeiro envio de cada aba) e só então abre a thread. Devolve o id do pedido, que
    /// [`Tela::cancelar_ia`] usa.
    pub(super) fn enviar_ia(self: &Rc<Self>, envio: Envio) -> u64 {
        let motor = &self.assistente.motor;
        let id = motor.proximo.get() + 1;
        motor.proximo.set(id);
        let (preparado, perfil, aba, receptor) = self.preparar_envio(envio);
        motor.receptores.borrow_mut().insert(id, receptor);
        let mostrar = self.config.with_untracked(|c| c.ia.mostrar_o_que_sai)
            && !motor.confirmadas.borrow().contains(&aba);
        let pendente = Pendente {
            id,
            etapa: Etapa::Internet,
            aba,
            perfil,
            preparado: Rc::new(preparado),
            mostrar,
        };
        let internet = pendente.perfil.localidade() == Locality::Internet
            && !pendente.perfil.aviso_saida_aceito;
        if internet {
            motor.internet_texto.set(t!(
                "ia-internet-texto",
                host = host_da_url(&pendente.perfil.url)
            ));
            *motor.pendente.borrow_mut() = Some(pendente);
            motor.internet_aberto.set(true);
        } else {
            self.depois_do_aviso(pendente);
        }
        id
    }

    /// O passo seguinte ao aviso de internet: "O que vai sair" ou direto o pedido.
    fn depois_do_aviso(self: &Rc<Self>, mut pendente: Pendente) {
        let motor = &self.assistente.motor;
        if !pendente.mostrar {
            self.despachar(pendente);
            return;
        }
        pendente.etapa = Etapa::OQueSai;
        let preparado = &pendente.preparado;
        motor
            .sai_titulo
            .set(t!("ia-sai-titulo", nome = pendente.perfil.nome.as_str()));
        motor.sai_destino.set(t!(
            "ia-sai-destino",
            url = pendente.perfil.url_limpa(),
            onde = selo_da_localidade(&pendente.perfil.url)
        ));
        motor
            .sai_texto
            .set(crate::texto_limpo::mostrar_sem_controle(
                &preparado.texto_exato(),
            ));
        motor.sai_instrucoes.set(preparado.sistema.clone());
        let mut avisos = Vec::new();
        let frase = preparado.contagem.frase();
        if !frase.is_empty() {
            avisos.push(frase);
        }
        if preparado.cortadas > 0 {
            let max = self.config.with_untracked(|c| c.ia.max_caracteres);
            avisos.push(t!(
                "ia-sai-cortei",
                n = preparado.cortadas.to_string(),
                max = max.to_string()
            ));
        }
        if preparado.bloqueado {
            avisos.push(t!("ia-sai-cofre-bloqueado"));
        }
        motor.sai_avisos.set(avisos.join("\n"));
        motor.sai_nao_aba.set(true);
        motor.sai_nao_mais.set(false);
        *motor.pendente.borrow_mut() = Some(pendente);
        motor.sai_aberto.set(true);
    }

    /// Abre a thread do pedido e avisa o receptor de que saiu.
    fn despachar(self: &Rc<Self>, pendente: Pendente) {
        let motor = &self.assistente.motor;
        let Pendente {
            id,
            perfil,
            preparado,
            ..
        } = pendente;
        let receptor = motor.receptores.borrow().get(&id).cloned();
        let Some(receptor) = receptor else {
            return;
        };
        if !perfil.usavel() {
            self.avisar(InfoBarSeverity::Warning, t!("ia-url-invalida"));
            motor.receptores.borrow_mut().remove(&id);
            receptor(Chegou::Desistiu);
            return;
        }
        let (ponto, rede) = self.endpoint_do_perfil(&perfil);
        let rede = rede.map(|r| self.resolver_rede(r));
        let pedido = Request {
            system: preparado.sistema.clone(),
            messages: preparado.mensagens.clone(),
            max_tokens: perfil.max_tokens,
            temperature: perfil.temperatura,
        };
        let ativo = Ativo {
            cancelar: Arc::new(AtomicBool::new(false)),
            handle: Arc::new(Mutex::new(None)),
        };
        let (cancelar, handle) = (Arc::clone(&ativo.cancelar), Arc::clone(&ativo.handle));
        motor.ativos.borrow_mut().insert(id, ativo);
        let empilhar = Arc::new(motor.empilhar());
        let dica = perfil.preset.dica_de_conexao();
        std::thread::spawn(move || {
            if let Some(rede) = &rede
                && let Err(erro) = rede.preparar(&mut |_| {})
            {
                empilhar(id, DaThread::Fim(Err(erro)));
                return;
            }
            if cancelar.load(Ordering::SeqCst) {
                return;
            }
            let saida = Arc::clone(&empilhar);
            let aberto = ai::stream(ponto, pedido, move |evento| match evento {
                Event::Text(texto) => saida(id, DaThread::Texto(texto)),
                Event::Done(_) => saida(id, DaThread::Fim(Ok(()))),
                Event::Error(erro) => {
                    let mut mensagem = erro.message.clone();
                    if erro.kind == AiErrorKind::Connect
                        && let Some(dica) = &dica
                    {
                        mensagem = format!("{mensagem} {dica}");
                    }
                    saida(id, DaThread::Fim(Err(mensagem)));
                }
                Event::Reasoning(_) => {}
            });
            if cancelar.load(Ordering::SeqCst) {
                aberto.cancel();
            }
            *handle.lock().unwrap_or_else(PoisonError::into_inner) = Some(aberto);
        });
        receptor(Chegou::Saiu(preparado));
    }

    /// Para o pedido `id`: fecha as caixas de confirmação dele e corta a resposta. O receptor não
    /// recebe mais nada.
    pub(super) fn cancelar_ia(&self, id: u64) {
        let motor = &self.assistente.motor;
        motor.receptores.borrow_mut().remove(&id);
        if let Some(ativo) = motor.ativos.borrow_mut().remove(&id) {
            ativo.cancelar.store(true, Ordering::SeqCst);
            if let Some(handle) = ativo
                .handle
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .as_ref()
            {
                handle.cancel();
            }
        }
        let parado = motor.pendente.borrow().as_ref().map(|p| p.id) == Some(id);
        if parado {
            *motor.pendente.borrow_mut() = None;
            motor.internet_aberto.set(false);
            motor.sai_aberto.set(false);
        }
    }

    /// Se o pedido `id` ainda espera resposta (ou confirmação).
    pub(super) fn ia_em_andamento(&self, id: u64) -> bool {
        self.assistente.motor.receptores.borrow().contains_key(&id)
    }

    /// Liga o motor: entrega o que as threads empilham e trata as caixas fechadas sem resposta.
    pub(super) fn ligar_motor(self: &Rc<Self>) {
        let (t, pronto) = (Rc::clone(self), self.assistente.motor.pronto);
        effect(move || {
            let _ = ler_contador(pronto);
            let itens = std::mem::take(
                &mut *t
                    .assistente
                    .motor
                    .fila
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner),
            );
            untrack(|| {
                let motor = &t.assistente.motor;
                for (id, item) in itens {
                    let receptor = motor.receptores.borrow().get(&id).cloned();
                    let Some(receptor) = receptor else {
                        continue;
                    };
                    match item {
                        DaThread::Texto(texto) => receptor(Chegou::Texto(texto)),
                        DaThread::Fim(resultado) => {
                            motor.ativos.borrow_mut().remove(&id);
                            motor.receptores.borrow_mut().remove(&id);
                            receptor(Chegou::Fim(resultado));
                        }
                    }
                }
            });
        });
        for (aberto, etapa) in [
            (self.assistente.motor.internet_aberto, Etapa::Internet),
            (self.assistente.motor.sai_aberto, Etapa::OQueSai),
        ] {
            let t = Rc::clone(self);
            effect(move || {
                if aberto.get() {
                    return;
                }
                untrack(|| t.desistiu_na_etapa(etapa));
            });
        }
    }

    /// A caixa da `etapa` fechou: se o pedido continua parado nela, a pessoa desistiu.
    fn desistiu_na_etapa(&self, etapa: Etapa) {
        let motor = &self.assistente.motor;
        let parado = motor
            .pendente
            .borrow()
            .as_ref()
            .is_some_and(|p| p.etapa == etapa);
        if !parado {
            return;
        }
        let Some(pendente) = motor.pendente.borrow_mut().take() else {
            return;
        };
        let receptor = motor.receptores.borrow_mut().remove(&pendente.id);
        if let Some(receptor) = receptor {
            receptor(Chegou::Desistiu);
        }
    }

    /// "Continuar" no aviso de internet: o provedor não pergunta mais.
    fn continuar_internet(self: &Rc<Self>) {
        let motor = &self.assistente.motor;
        let Some(pendente) = motor.pendente.borrow_mut().take() else {
            return;
        };
        let id = pendente.perfil.id;
        self.assistente.perfis.update(|a| {
            if let Some(p) = a.perfil_mut(id) {
                p.aviso_saida_aceito = true;
            }
        });
        let mut pendente = pendente;
        pendente.perfil.aviso_saida_aceito = true;
        self.depois_do_aviso(pendente);
    }

    /// "Enviar" em "O que vai sair": segue com o pedido e guarda as duas caixas de não perguntar.
    fn enviar_o_que_sai(self: &Rc<Self>) {
        let motor = &self.assistente.motor;
        let Some(pendente) = motor.pendente.borrow_mut().take() else {
            return;
        };
        if motor.sai_nao_aba.get_untracked() {
            motor.confirmadas.borrow_mut().insert(pendente.aba.clone());
        }
        if motor.sai_nao_mais.get_untracked() {
            self.config.update(|c| c.ia.mostrar_o_que_sai = false);
        }
        self.despachar(pendente);
    }
}

/// As caixas "Enviar para {provedor}?" e o aviso de internet.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View {
    let motor = &tela.assistente.motor;
    let (internet, internet_texto) = (motor.internet_aberto, motor.internet_texto);
    let aviso = ContentDialog::new(t!("ia-internet-titulo"), internet)
        .id("ia-internet")
        .content(
            Text::new(move || internet_texto.get())
                .id("ia-internet-texto")
                .wrap(),
        )
        .primary(t!("comum-continuar"), {
            let t = Rc::clone(tela);
            move || t.continuar_internet()
        })
        .close(t!("comum-cancelar"));

    let (aberto, titulo, destino, texto) = (
        motor.sai_aberto,
        motor.sai_titulo,
        motor.sai_destino,
        motor.sai_texto,
    );
    let (instrucoes, instrucoes_abertas, avisos) = (
        motor.sai_instrucoes,
        motor.sai_instrucoes_abertas,
        motor.sai_avisos,
    );
    let (nao_aba, nao_mais) = (motor.sai_nao_aba, motor.sai_nao_mais);
    let sai = ContentDialog::new(move || titulo.get(), aberto)
        .id("ia-o-que-sai")
        .content(
            Column::new()
                .gap(8)
                .width(496)
                .child(
                    Text::new(move || destino.get())
                        .id("ia-o-que-sai-destino")
                        .secondary()
                        .wrap(),
                )
                .child(
                    TextEditor::bind(texto)
                        .id("ia-o-que-sai-texto")
                        .read_only(true)
                        .monospace()
                        .wrap(true)
                        .min_height(140.0)
                        .max_height(280.0),
                )
                .child(
                    Expander::new(t!("ia-sai-instrucoes"), instrucoes_abertas)
                        .id("ia-o-que-sai-instrucoes")
                        .content(
                            TextEditor::bind(instrucoes)
                                .id("ia-o-que-sai-sistema")
                                .read_only(true)
                                .wrap(true)
                                .max_height(160.0),
                        ),
                )
                .child(
                    Text::new(move || avisos.get())
                        .id("ia-o-que-sai-avisos")
                        .secondary()
                        .wrap()
                        .visible(move || !avisos.with(String::is_empty)),
                )
                .child(
                    CheckBox::bind(nao_aba, t!("ia-sai-nao-perguntar-aba"))
                        .id("ia-o-que-sai-nao-aba"),
                )
                .child(
                    CheckBox::bind(nao_mais, t!("ia-sai-nao-perguntar-mais"))
                        .id("ia-o-que-sai-nao-mais"),
                ),
        )
        .primary(t!("ia-sai-enviar"), {
            let t = Rc::clone(tela);
            move || t.enviar_o_que_sai()
        })
        .close(t!("comum-cancelar"));

    Column::new().child(aviso).child(sai)
}
