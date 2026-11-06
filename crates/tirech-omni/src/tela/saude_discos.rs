//! O diagnóstico dos discos na tela: ao conectar, em segundo plano, roda o script pelo canal da
//! própria sessão (nada aparece no terminal e nenhuma conexão nova é aberta), guarda o resultado
//! por servidor em `saude_discos.json` e, se há alerta, avisa por uma notificação do sistema.
//! A parte pura (o script, a leitura, os alertas, a herança) está em [`crate::saude_discos`], e o
//! bloco da aba Disco, em `processos/saude.rs`.
//!
//! Sozinho, nunca chama o `sudo` num servidor em que ele ainda não funcionou: um `sudo -n` negado
//! fica no log e pode avisar o administrador. Só "Verificar de novo" tenta; dando certo, o cache
//! lembra e as próximas leituras em segundo plano usam o `sudo` também.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use tomni_ui::notification::Notification;
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{Tela, poe};
use crate::agente::{acordar, ler_contador};
use crate::modelo::{Node, ShellRemoto, Tree};
use crate::saude_discos::{self, CHAVE_LOCAL, Cache, Diagnostico, Entrada, SCRIPT};
use crate::t;

/// O que uma thread de diagnóstico entrega: a chave, o alvo, se podia chamar o `sudo` e o resultado
/// (`Err`: não deu para rodar, como a conexão que caiu; fica em silêncio e não se guarda).
type Entrega = (String, String, bool, Result<Diagnostico, String>);

/// O estado do diagnóstico.
pub(super) struct Estado {
    /// As leituras, carregadas de `saude_discos.json` no primeiro uso.
    cache: RefCell<Option<Cache>>,
    /// O que está rodando agora, pela chave, com a aba que pediu.
    correndo: RefCell<HashMap<String, Option<Session>>>,
    /// O que as threads entregaram e a tela ainda não pegou.
    entregas: Arc<Mutex<Vec<Entrega>>>,
    /// Sobe a cada entrega.
    pronto: Signal<u64>,
    /// Sobe quando o cache muda: é o que a aba Disco acompanha.
    mudou: Signal<u64>,
    /// Os alertas já avisados nesta execução, pela chave: repete só o que mudou.
    avisados: RefCell<HashMap<String, Vec<String>>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            cache: RefCell::default(),
            correndo: RefCell::default(),
            entregas: Arc::default(),
            pronto: signal(0),
            mudou: signal(0),
            avisados: RefCell::default(),
        }
    }
}

/// As opções da linha do nó escolhido na árvore.
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => saude_discos::opcoes(t, id),
        _ => saude_discos::opcoes(t, 0),
    })
}

/// Agora, em segundos desde 1970.
fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Tela {
    /// Mostra na linha "Saúde dos discos" a escolha do nó escolhido (conexão ou pasta).
    pub(super) fn sincronizar_diagnostico_discos(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| {
            let atual = match t.find(id)? {
                Node::Connection(c) => c.diagnostico_discos,
                Node::Group(g) => g.diagnostico_discos,
            };
            Some(saude_discos::rotulo(t, id, atual))
        });
        if let Some(texto) = texto {
            poe(self.campos.diagnostico_discos, texto);
        }
    }

    /// Passa a linha "Saúde dos discos" para a pasta escolhida e grava. Lê o signal com
    /// rastreamento: serve a um efeito.
    pub(super) fn guardar_diagnostico_discos_do_grupo(&self) {
        let texto = self.campos.diagnostico_discos.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let nova = saude_discos::do_rotulo(&texto);
            let mut mudou = false;
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id)
                    && g.diagnostico_discos != nova
                {
                    g.diagnostico_discos = nova;
                    mudou = true;
                }
            });
            if mudou {
                self.atualizar();
            }
        });
    }

    /// Liga o que dispara o diagnóstico: cada aba SSH que termina de conectar, em todas as
    /// janelas. Acompanha as Configurações e a árvore: ligar a opção depois também dispara.
    pub(super) fn ligar_saude_discos(self: &Rc<Self>) {
        let tela = Rc::clone(self);
        // As abas que já passaram por aqui. Uma aba com a opção desligada não entra: ela é
        // lida quando a opção ligar.
        let tentadas: RefCell<HashSet<Session>> = RefCell::default();
        effect(move || {
            tela.config.with(|c| c.geral.diagnostico_discos);
            tela.tree.with(|_| ());
            let mut vivas = HashSet::new();
            let mut novas = Vec::new();
            for janela in tela.janelas.todas() {
                for sessao in janela.layout.with(|l| l.sessions()) {
                    vivas.insert(sessao);
                    if !sessao.connecting()
                        && !sessao.closed()
                        && !tentadas.borrow().contains(&sessao)
                        && tela.alvo_da_sessao(sessao).is_some()
                    {
                        novas.push(sessao);
                    }
                }
            }
            {
                let mut tentadas = tentadas.borrow_mut();
                tentadas.retain(|s| vivas.contains(s));
                tentadas.extend(novas.iter().copied());
            }
            untrack(|| {
                for sessao in novas {
                    tela.diagnosticar_sessao(sessao, false);
                }
            });
        });
        let tela = Rc::clone(self);
        let pronto = self.saude_discos.pronto;
        effect(move || {
            ler_contador(pronto);
            untrack(|| tela.receber_diagnosticos());
        });
    }

    /// A chave do cache e o alvo (`usuario@host:porta`) de uma aba em que o diagnóstico vale:
    /// SSH, num servidor com `sh` e com a opção ligada.
    fn alvo_da_sessao(&self, sessao: Session) -> Option<(String, String)> {
        let id = self.conexao_da_sessao(sessao)?;
        let conexao = self.conexao_ssh(id)?;
        let efetiva = self.conexao_efetiva(&conexao);
        if matches!(
            efetiva.shell_remoto,
            ShellRemoto::PowerShell | ShellRemoto::Cmd
        ) {
            return None;
        }
        let padrao = self.config.with_untracked(|c| c.geral.diagnostico_discos);
        self.tree
            .with_untracked(|t| saude_discos::ligado(t, id, padrao))
            .then(|| {
                (
                    id.to_string(),
                    format!("{}@{}:{}", efetiva.user, efetiva.host, efetiva.port),
                )
            })
    }

    /// Roda `f` com o cache, carregando-o na primeira vez. Sem `saude_discos.json`, ou com ele
    /// ilegível, começa vazio: o cache só evita repetir a leitura.
    fn com_cache_de_saude<R>(&self, f: impl FnOnce(&mut Cache) -> R) -> R {
        let mut cache = self.saude_discos.cache.borrow_mut();
        let cache = cache.get_or_insert_with(|| {
            self.armazenamento
                .carregar_json(
                    &self.armazenamento.arquivo_saude_discos(),
                    &t!("saude-discos-titulo"),
                )
                .0
        });
        f(cache)
    }

    /// A leitura de `chave` que ainda vale para `alvo`.
    fn leitura_valida(&self, chave: &str, alvo: &str) -> Option<Diagnostico> {
        self.com_cache_de_saude(|c| {
            c.leituras
                .get(chave)
                .filter(|e| e.vale(alvo, agora()))
                .map(|e| e.diagnostico.clone())
        })
    }

    /// Se a leitura pode chamar o `sudo`: quem pede é o botão, ou ele já funcionou neste alvo.
    fn pode_chamar_o_sudo(&self, chave: &str, alvo: &str, forcar: bool) -> bool {
        forcar || self.com_cache_de_saude(|c| c.sudo_provado(chave, alvo))
    }

    /// Diagnostica o servidor da aba `sessao`, numa thread. Sem `forcar`, uma leitura que ainda
    /// vale só repassa os alertas dela (ao abrir a conexão, a notificação vem do mesmo jeito).
    fn diagnosticar_sessao(self: &Rc<Self>, sessao: Session, forcar: bool) {
        let Some((chave, alvo)) = self.alvo_da_sessao(sessao) else {
            return;
        };
        if !forcar && let Some(diagnostico) = self.leitura_valida(&chave, &alvo) {
            self.avisar_da_saude(&chave, sessao, &diagnostico);
            return;
        }
        if self.saude_discos.correndo.borrow().contains_key(&chave) {
            return;
        }
        // Só pelo canal da sessão aberta: nunca abre uma conexão nova para isso.
        let Ok(exec) = self.opener.exec_da_sessao(sessao) else {
            return;
        };
        let com_sudo = self.pode_chamar_o_sudo(&chave, &alvo, forcar);
        self.rodar_diagnostico(chave, alvo, com_sudo, Some(sessao), move || {
            let saida = exec(
                saude_discos::comando(com_sudo),
                &mut SCRIPT.as_bytes(),
                &mut |_| {},
            )?;
            if saida.status.is_none() && saida.stdout.is_empty() {
                // O canal fechou sem resposta (a conexão caiu): não diz nada do servidor.
                return Err(String::new());
            }
            Ok(saude_discos::ler_saida(&saida.stdout_text()))
        });
    }

    /// Roda `ler` numa thread e entrega o resultado.
    fn rodar_diagnostico(
        &self,
        chave: String,
        alvo: String,
        com_sudo: bool,
        sessao: Option<Session>,
        ler: impl FnOnce() -> Result<Diagnostico, String> + Send + 'static,
    ) {
        self.saude_discos
            .correndo
            .borrow_mut()
            .insert(chave.clone(), sessao);
        let (entregas, pronto) = (
            Arc::clone(&self.saude_discos.entregas),
            self.saude_discos.pronto,
        );
        std::thread::spawn(move || {
            let resultado = ler();
            entregas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((chave, alvo, com_sudo, resultado));
            acordar(pronto);
        });
    }

    /// Guarda o que as threads entregaram, grava e avisa dos alertas.
    fn receber_diagnosticos(self: &Rc<Self>) {
        let entregas = std::mem::take(
            &mut *self
                .saude_discos
                .entregas
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        for (chave, alvo, com_sudo, resultado) in entregas {
            let sessao = self
                .saude_discos
                .correndo
                .borrow_mut()
                .remove(&chave)
                .flatten();
            let Ok(diagnostico) = resultado else {
                continue;
            };
            let existe = |id: &str| {
                id == CHAVE_LOCAL
                    || id
                        .parse::<u64>()
                        .is_ok_and(|id| self.tree.with_untracked(|t| t.connection(id).is_some()))
            };
            self.com_cache_de_saude(|cache| {
                cache.leituras.insert(
                    chave.clone(),
                    Entrada {
                        alvo,
                        quando: agora(),
                        // Uma recusa desfaz a prova: não insiste num `sudo` que deixou de valer.
                        com_sudo: com_sudo && matches!(diagnostico, Diagnostico::Discos(_)),
                        diagnostico: diagnostico.clone(),
                    },
                );
                cache.leituras.retain(|id, _| existe(id));
                // Gravar é só para a próxima vez: uma falha aqui faz ler de novo, mais nada.
                let _ = self
                    .armazenamento
                    .gravar(&self.armazenamento.arquivo_saude_discos(), &*cache);
            });
            self.saude_discos.mudou.update(|n| *n += 1);
            if let Some(sessao) = sessao {
                self.avisar_da_saude(&chave, sessao, &diagnostico);
            }
        }
    }

    /// A notificação dos alertas do servidor da aba `sessao`, se mudaram desde a última vez
    /// nesta execução. O clique traz a aba.
    fn avisar_da_saude(self: &Rc<Self>, chave: &str, sessao: Session, diagnostico: &Diagnostico) {
        let alertas = diagnostico.alertas();
        let antes = self
            .saude_discos
            .avisados
            .borrow_mut()
            .insert(chave.to_owned(), alertas.clone());
        if alertas.is_empty() || antes.as_ref() == Some(&alertas) {
            return;
        }
        let nome = self
            .conexao_da_sessao(sessao)
            .and_then(|id| {
                self.tree
                    .with_untracked(|t| t.connection(id).map(|c| c.name.clone()))
            })
            .unwrap_or_else(|| sessao.title());
        let tela = Rc::clone(self);
        Notification::new(t!("saude-discos-notificacao-titulo", nome = nome))
            .body(alertas.join("\n"))
            .tag(format!("saude-discos-{chave}"))
            .on_click(move || tela.trazer_aba(sessao))
            .show();
    }

    /// O que a aba Disco mostra da máquina: `None` é este computador. Reativo. `None` de
    /// resposta: nada lido, ou a opção desligada.
    pub(super) fn saude_da_maquina(&self, sessao: Option<Session>) -> Option<Diagnostico> {
        ler_contador(self.saude_discos.mudou);
        let padrao = self.config.with(|c| c.geral.diagnostico_discos);
        self.tree.with(|_| ());
        let (chave, alvo) = match sessao {
            Some(sessao) => self.alvo_da_sessao(sessao)?,
            None if padrao => (CHAVE_LOCAL.to_owned(), String::new()),
            None => return None,
        };
        self.com_cache_de_saude(|c| {
            c.leituras
                .get(&chave)
                .filter(|e| e.alvo == alvo)
                .map(|e| e.diagnostico.clone())
        })
    }

    /// A aba Disco abriu (ou o botão "Verificar de novo", com `forcar`): lê a máquina.
    pub(super) fn pedir_saude(self: &Rc<Self>, sessao: Option<Session>, forcar: bool) {
        if let Some(sessao) = sessao {
            self.diagnosticar_sessao(sessao, forcar);
            return;
        }
        let ligado = self.config.with_untracked(|c| c.geral.diagnostico_discos);
        let (chave, alvo) = (CHAVE_LOCAL.to_owned(), String::new());
        if !ligado
            || self.saude_discos.correndo.borrow().contains_key(&chave)
            || (!forcar && self.leitura_valida(&chave, &alvo).is_some())
        {
            return;
        }
        let ler = Arc::clone(&self.ferramentas.ambiente().saude_local);
        let com_sudo = self.pode_chamar_o_sudo(&chave, &alvo, forcar);
        self.rodar_diagnostico(chave, alvo, com_sudo, None, move || Ok(ler(com_sudo)));
    }
}
