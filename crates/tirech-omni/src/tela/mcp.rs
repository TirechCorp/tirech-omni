//! O app como servidor MCP na casca (M51): liga e desliga o servidor com `ia.mcp_ligado`, atende
//! na thread da interface os pedidos que as pontes mandam e leva cada um pelo mesmo caminho:
//!
//! 1. acha o alvo (a aba, a conexão) e monta o [`Plano`] do que vai acontecer;
//! 2. decide pela permissão da conexão e pela lista de bloqueio (`mcp/permissao.rs`);
//! 3. se for o caso, espera a pessoa no diálogo de aprovação (`tela/mcp_aprovacao.rs`);
//! 4. executa (`tela/mcp_terminal.rs`, `tela/mcp_arquivos.rs`);
//! 5. termina em [`Tela::terminar_mcp`]: tira o controle e os segredos da saída, corta o que
//!    passa do limite, põe no envelope e grava a auditoria.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use tomni_ui::files::Live;
use tomni_ui::mcp::{error_result, text_result};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::mcp_aprovacao::Aprovacao;
use super::{Janela, Tela};
use crate::configuracoes::{AbrirAbaIa, Configuracoes};
use crate::espacos::id_da_chave;
use crate::mcp::alvo::achar_conexao;
use crate::mcp::auditoria::{self, DecisaoRegistrada, Registro};
use crate::mcp::bloqueio::{perigoso, perigoso_digitado};
use crate::mcp::operacao::{self, Resposta, caminho_na_arvore};
use crate::mcp::permissao::{
    ABRIR_ABA_BLOQUEADA, Decisao, Exigencia, PermissaoIa, abrir_aba_efetiva, decidir,
    decidir_abertura, exigencia, permissao_efetiva, permissao_mais_alta,
};
use crate::mcp::saidas::Saidas;
use crate::mcp::servidor::{self, Fila, Pedido, ServidorMcp};
use crate::mcp::sessoes::SessaoIa;
use crate::mcp::teclas::eh_enter;
use crate::modelo::Connection;
use crate::redacao::Redator;
use crate::t;
use crate::texto_limpo::{envelope, sem_controle};

/// As ferramentas que passam pela casca. A única que não passa é `listar_conexoes`, que vai
/// direto para `operacao::atender`.
const DA_CASCA: [&str; 28] = [
    "abrir_conexao",
    "listar_abas",
    "fechar_aba",
    "reconectar_aba",
    "digitar",
    "teclas",
    "ler_tela",
    "esperar",
    "executar",
    "listar_arquivos",
    "baixar_arquivo",
    "enviar_arquivo",
    "listar_tuneis",
    "ligar_tunel",
    "desligar_tunel",
    "listar_portas",
    "encaminhar_porta",
    "parar_porta",
    "ler_saida",
    "listar_paineis",
    "organizar_paineis",
    "listar_espacos",
    "ler",
    "editar",
    "escrever",
    "buscar",
    "glob",
    "bash",
];

/// As ferramentas que trabalham num espaço de trabalho (todas, menos `listar_espacos`).
const DO_ESPACO: [&str; 6] = ["ler", "editar", "escrever", "buscar", "glob", "bash"];

/// O que o pedido vai fazer, para o título do diálogo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Acao {
    /// Só olha.
    Ver,
    /// Roda um comando.
    Rodar,
    /// Digita texto.
    Digitar,
    /// Aperta teclas.
    Teclas,
    /// Manda um arquivo para o servidor.
    Enviar,
    /// Muda um arquivo do espaço de trabalho.
    Editar,
    /// Abre uma aba.
    Abrir,
    /// Liga ou desliga um túnel.
    Tunel,
    /// Fecha uma aba.
    Fechar,
    /// Reabre uma aba caída.
    Reconectar,
    /// Muda a grade de painéis de uma janela.
    Organizar,
}

/// O que se vai fazer e com quem, antes de decidir.
pub(super) struct Plano {
    /// O que é.
    pub acao: Acao,
    /// A conexão alvo, quando há.
    pub conexao: Option<Connection>,
    /// A aba alvo, quando há.
    pub sessao: Option<Session>,
    /// O que vai acontecer, como o diálogo mostra (texto cru, que o diálogo limpa).
    pub detalhe: String,
    /// Por que a lista de bloqueio pegou, se pegou.
    pub bloqueio: Option<String>,
    /// A sessão que é dona da aba, quando o pedido muda uma aba de outra sessão.
    pub dono: Option<SessaoIa>,
    /// A janela (a partir de 1), as colunas e as linhas de um `organizar_paineis`.
    pub grade: Option<(usize, usize, usize)>,
    /// Se o pedido abre uma aba nova da conexão (o `abrir_conexao`, ligar um túnel ou usar um
    /// espaço sem aba de pé), o que passa pela escolha "Abrir abas novas".
    pub abre_aba: bool,
    /// Se só a escolha "Abrir abas novas" (em Perguntar) faz o pedido perguntar: é quando o
    /// diálogo oferece abrir as próximas abas da sessão sem perguntar.
    pub so_abertura: bool,
}

/// Um pedido em andamento.
pub(super) struct Trabalho {
    /// O pedido da ponte.
    pub pedido: Pedido,
    /// Quando chegou.
    pub inicio: Instant,
    /// A geração de "Parar a IA" em que chegou: se mudar, o pedido acaba.
    pub geracao: u64,
    /// A geração de "Parar" da sessão que pediu, quando chegou.
    pub geracao_sessao: u64,
    /// A conexão alvo.
    pub conexao: Cell<Option<u64>>,
    /// O nome da conexão, para o envelope.
    pub nome_conexao: RefCell<String>,
    /// A chave da aba alvo.
    pub aba: RefCell<Option<String>>,
    /// O que se decidiu.
    pub decisao: Cell<DecisaoRegistrada>,
    /// O motivo da recusa dado pela pessoa.
    pub motivo: RefCell<Option<String>>,
    /// O limite de caracteres pedido (`caracteres`), se veio.
    pub caracteres: Option<usize>,
    /// Se já respondeu.
    pub terminado: Cell<bool>,
}

impl Trabalho {
    fn novo(pedido: &Pedido, geracao: u64, geracao_sessao: u64) -> Rc<Self> {
        let caracteres = pedido.args["caracteres"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok());
        Rc::new(Self {
            pedido: pedido.clone(),
            inicio: Instant::now(),
            geracao,
            geracao_sessao,
            conexao: Cell::new(None),
            nome_conexao: RefCell::new(String::new()),
            aba: RefCell::new(None),
            decisao: Cell::new(DecisaoRegistrada::Permitida),
            motivo: RefCell::new(None),
            caracteres,
            terminado: Cell::new(false),
        })
    }
}

/// O que um pedido devolve ao terminar.
pub(super) enum Saida {
    /// Um texto que o app escreveu.
    App(String),
    /// Um texto que veio de um servidor: vai limpo, redigido, cortado e no envelope.
    Servidor {
        /// O que o app escreveu antes do envelope (JSON de estado, por exemplo).
        cabecalho: Option<String>,
        /// De onde veio (`tela`, `comando`, `arquivo`…).
        origem: String,
        /// O texto.
        texto: String,
        /// Se passa pelo limite de `ia.caracteres_por_resposta`.
        limitar: bool,
    },
    /// Um erro de ferramenta, em português.
    Erro(String),
    /// Um `result` pronto.
    Bruto(Value),
    /// Um erro JSON-RPC.
    Jsonrpc(i64, String),
}

impl Saida {
    /// Um texto de servidor com `origem`, que passa pelo limite.
    pub(super) fn servidor(origem: &str, texto: String, cabecalho: Option<String>) -> Self {
        Self::Servidor {
            cabecalho,
            origem: origem.to_owned(),
            texto,
            limitar: true,
        }
    }
}

/// Um passo de uma tarefa que espera.
pub(super) type Passo = Box<dyn FnMut(&Rc<Tela>) -> Sonda>;

/// A última grade pedida numa janela: a sessão, as colunas e linhas, e quando.
type GradePedida = (u64, (usize, usize), Instant);

/// O que um passo de espera diz.
pub(super) enum Sonda {
    /// Olhar de novo depois deste tempo.
    Depois(Duration),
    /// Acabou.
    Fim(Saida),
    /// Acabou esta tarefa e outra já assumiu o pedido, que ela mesma termina.
    Seguiu,
}

/// O que uma ferramenta exige do alvo.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Exige {
    /// Nada (ou uma conexão opcional).
    Nada,
    /// Uma conexão.
    Conexao,
    /// Uma aba com terminal.
    Aba,
}

/// O que se sabe de um alvo achado.
pub(super) struct Alvo {
    pub conexao: Option<Connection>,
    pub sessao: Option<Session>,
}

/// A última coisa que um assistente fez numa aba.
pub(super) struct Atividade {
    /// Quando.
    pub quando: Instant,
    /// O nome da sessão do assistente que mexeu por último.
    pub assistente: String,
    /// A ligação dessa sessão.
    pub ligacao: u64,
    /// A ferramenta que ela usou por último.
    pub acao: String,
    /// As ligações que mexeram na aba, para a cor de uma aba sem dono.
    pub vistas: Vec<u64>,
    /// Quantos pedidos estão em andamento na aba.
    pub em_curso: u32,
}

/// O que a tela guarda do servidor MCP.
pub(super) struct EstadoMcp {
    /// O servidor, enquanto o MCP está ligado.
    pub servidor: RefCell<Option<ServidorMcp>>,
    /// Os pedidos das pontes, à espera da interface.
    pub fila: Fila,
    /// Por que o servidor não subiu (outro app já atende, endereço ocupado…).
    pub erro: Signal<Option<String>>,
    /// A geração de "Parar a IA".
    pub geracao: Cell<u64>,
    /// As abas em que um assistente mexeu há pouco.
    pub atividade: RefCell<HashMap<String, Atividade>>,
    /// Muda quando a marca das abas muda.
    pub marca_v: Signal<u64>,
    /// As sessões de assistente ligadas, da mais antiga para a mais nova.
    pub sessoes: Signal<Vec<SessaoIa>>,
    /// O dono de cada aba: a ligação que a abriu ou que mexeu nela primeiro.
    pub donos: RefCell<HashMap<String, u64>>,
    /// A aba a que cada sessão está presa (`--aba`, M63): sessão -> chave da aba. Vale o Livre
    /// só nela e é a aba das ferramentas que não dizem outra. Some quando a ponte sai.
    pub alvos: RefCell<HashMap<u64, String>>,
    /// As abas que o "Dar esta aba também a" deu a cada sessão, além da alvo: sessão -> chaves.
    /// Valem o Livre só nelas, mas não mudam a aba das ferramentas que não dizem outra. Somem
    /// quando a ponte sai.
    pub dadas: RefCell<HashMap<u64, Vec<String>>>,
    /// Os recados para cada sessão (a aba que a pessoa deu a ela), à espera da próxima resposta
    /// ou do hook dos avisos. Somem quando a ponte sai.
    pub recados: RefCell<HashMap<u64, Vec<String>>>,
    /// A geração de "Parar" de cada sessão: muda quando a pessoa para só ela.
    pub paradas: RefCell<HashMap<u64, u64>>,
    /// As sessões de arquivos dos espaços de trabalho, pelo nome do espaço.
    pub espacos: RefCell<HashMap<String, super::mcp_espaco::SessaoEspaco>>,
    /// A última grade pedida por janela (número): a sessão, a grade e quando.
    pub grades: RefCell<HashMap<usize, GradePedida>>,
    /// Se o laço que confere as sessões ligadas está de pé.
    pub sincronizando: Cell<bool>,
    /// As abas cujas próximas ações a pessoa aprovou, por ligação.
    pub aprovadas_sessao: RefCell<HashSet<(u64, String)>>,
    /// Quando a pessoa deixou cada sessão (ligação) abrir abas novas sem perguntar; vale pelo
    /// tempo de `ia.mcp_abrir_auto`, contado daí.
    pub abrir_auto: RefCell<HashMap<u64, Instant>>,
    /// As saídas cortadas, para o `ler_saida`.
    pub saidas: RefCell<Saidas>,
    /// As aprovações à espera, na ordem; a primeira vai para o diálogo.
    pub aprovacoes: RefCell<VecDeque<Aprovacao>>,
    /// A que está no diálogo agora.
    pub ativa: RefCell<Option<Aprovacao>>,
    /// Se o laço que descarta os pedidos cancelados está de pé.
    pub vigiando: Cell<bool>,
    /// Os arquivos soltos nas abas que um assistente usa, pela chave da aba, à espera da próxima
    /// resposta sobre ela.
    pub soltos: RefCell<HashMap<String, Vec<super::mcp_soltos::Solto>>>,
    /// Os arquivos e pastas deste computador que a pessoa colou ou soltou no app: o
    /// `enviar_arquivo` os aceita como origem, além da pasta de troca.
    pub entregues: RefCell<Vec<std::path::PathBuf>>,
    /// O que o diálogo mostra.
    pub dialogo: super::mcp_aprovacao::SinaisDoDialogo,
    /// O diálogo do espaço de trabalho.
    pub espaco: super::mcp_espacos::SinaisEspaco,
    /// O "Como ligar um assistente" e o aviso de ligar.
    pub como: super::mcp_como::SinaisComo,
}

impl EstadoMcp {
    pub(super) fn novo() -> Self {
        Self {
            servidor: RefCell::new(None),
            fila: Live::new(Vec::new()),
            erro: signal(None),
            geracao: Cell::new(0),
            atividade: RefCell::new(HashMap::new()),
            marca_v: signal(0),
            sessoes: signal(Vec::new()),
            donos: RefCell::new(HashMap::new()),
            alvos: RefCell::new(HashMap::new()),
            dadas: RefCell::new(HashMap::new()),
            recados: RefCell::new(HashMap::new()),
            paradas: RefCell::new(HashMap::new()),
            espacos: RefCell::new(HashMap::new()),
            grades: RefCell::new(HashMap::new()),
            sincronizando: Cell::new(false),
            aprovadas_sessao: RefCell::new(HashSet::new()),
            abrir_auto: RefCell::new(HashMap::new()),
            saidas: RefCell::new(Saidas::default()),
            aprovacoes: RefCell::new(VecDeque::new()),
            ativa: RefCell::new(None),
            vigiando: Cell::new(false),
            soltos: RefCell::new(HashMap::new()),
            entregues: RefCell::new(Vec::new()),
            dialogo: super::mcp_aprovacao::SinaisDoDialogo::novos(),
            espaco: super::mcp_espacos::SinaisEspaco::novos(),
            como: super::mcp_como::SinaisComo::novos(),
        }
    }
}

impl Tela {
    /// Sobe e derruba o servidor conforme `ia.mcp_ligado` e `geral.mcp`, e atende os pedidos que chegam.
    pub(super) fn ligar_mcp(self: &Rc<Self>) {
        let (tela, config) = (Rc::clone(self), self.config);
        effect(move || {
            let ligado = config.with(Configuracoes::mcp_no_ar);
            untrack(|| tela.alternar_servidor_mcp(ligado));
        });
        let (tela, fila) = (Rc::clone(self), self.mcp.fila.clone());
        effect(move || {
            // Só rastreia a versão: `tirar` pega o mutex de novo, e dentro do `with` travaria.
            fila.with(|_| ());
            let pedidos = servidor::tirar(&fila);
            untrack(|| {
                for pedido in pedidos {
                    tela.atender_pedido_mcp(pedido);
                }
            });
        });
    }

    fn alternar_servidor_mcp(self: &Rc<Self>, ligado: bool) {
        if !ligado {
            let tinha = self.mcp.servidor.borrow_mut().take().is_some();
            self.mcp.erro.set(None);
            if tinha {
                self.parar_ia_mcp();
                self.mcp.saidas.borrow_mut().limpar();
            }
            self.mcp.espacos.borrow_mut().clear();
            self.sincronizar_sessoes();
            return;
        }
        if self.mcp.servidor.borrow().is_some() {
            return;
        }
        let endereco = crate::mcp::endereco_da_pasta(self.armazenamento.pasta());
        match ServidorMcp::iniciar(&endereco, self.mcp.fila.clone()) {
            Ok(servidor) => {
                *self.mcp.servidor.borrow_mut() = Some(servidor);
                self.mcp.erro.set(None);
                self.sincronizar_sessoes();
            }
            Err(erro) => self.mcp.erro.set(Some(erro.texto())),
        }
    }

    /// Quantas abas há abertas da conexão `id`, somando todas as janelas.
    fn abas_da_conexao(&self, id: u64) -> usize {
        let prefixo = format!("{id}-");
        self.janelas
            .todas_agora()
            .iter()
            .map(|janela| {
                janela.layout.with_untracked(|l| {
                    l.sessions()
                        .iter()
                        .filter(|s| s.key_str().starts_with(&prefixo))
                        .count()
                })
            })
            .sum()
    }

    /// A aba com a chave `chave`, em qualquer janela.
    pub(super) fn sessao_por_chave(&self, chave: &str) -> Option<(Janela, Session)> {
        self.sessoes_das_janelas(None)
            .into_iter()
            .find(|(_, s)| s.key_str() == chave)
    }

    /// A aba mais à mão da conexão `id`: a que está à mostra num painel ativo, senão a primeira
    /// de pé, senão a primeira.
    pub(super) fn aba_da_conexao(&self, id: u64) -> Option<(Janela, Session)> {
        let prefixo = format!("{id}-");
        let todas: Vec<(Janela, Session)> = self
            .sessoes_das_janelas(None)
            .into_iter()
            .filter(|(_, s)| s.key_str().starts_with(&prefixo))
            .collect();
        let a_mostra = todas.iter().find(|(j, s)| {
            j.layout
                .with_untracked(|l| l.pane(l.active()).selected == Some(*s))
                && !s.closed()
        });
        a_mostra
            .or_else(|| todas.iter().find(|(_, s)| !s.closed()))
            .or(todas.first())
            .copied()
    }

    /// Acha a conexão e a aba que o pedido da sessão `ligacao` aponta. A aba a que a sessão está
    /// presa (M63) e as que o "Dar esta aba também a" deu passam mesmo sem permissão na conexão;
    /// só a presa vale quando o pedido não diz outra.
    pub(super) fn alvo_mcp(
        &self,
        ligacao: u64,
        args: &Value,
        exige: Exige,
    ) -> Result<Alvo, String> {
        let liberada = |id: u64| {
            self.tree.with_untracked(|t| {
                (permissao_efetiva(t, id) != PermissaoIa::Desligada)
                    .then(|| t.conexao_para_abrir(id))
                    .flatten()
            })
        };
        let presa = self.aba_da_sessao(ligacao);
        let da_aba = |chave: &str, sessao: Session| {
            if self.liberada_pelo_clique(ligacao, chave) {
                return Some(self.conexao_da_aba(chave, sessao));
            }
            id_da_chave(chave).and_then(liberada)
        };
        let da_chave = |chave: &str| {
            let (_, sessao) = self
                .sessao_por_chave(chave)
                .ok_or_else(|| t!("mcp-aba-nao-existe", aba = chave))?;
            let conexao = da_aba(chave, sessao).ok_or_else(|| t!("mcp-conexao-nao-liberada"))?;
            Ok(Alvo {
                conexao: Some(conexao),
                sessao: Some(sessao),
            })
        };
        if let Some(chave) = args["aba"].as_str() {
            return da_chave(chave);
        }
        if let Some(valor) = args.get("conexao").filter(|v| !v.is_null()) {
            let conexao = self
                .tree
                .with_untracked(|t| achar_conexao(t, valor).cloned())?;
            let sessao = self.aba_da_conexao(conexao.id).map(|(_, s)| s);
            if exige == Exige::Aba {
                if conexao.kind.so_arquivos() {
                    return Err(t!("mcp-sem-terminal"));
                }
                if sessao.is_none() {
                    return Err(t!("mcp-sem-aba", conexao = conexao.name.as_str()));
                }
            }
            return Ok(Alvo {
                conexao: Some(conexao),
                sessao,
            });
        }
        if exige == Exige::Nada {
            return Ok(Alvo {
                conexao: None,
                sessao: None,
            });
        }
        if let Some(chave) = presa.as_deref() {
            return da_chave(chave);
        }
        let sessao = self.sessao_ativa().ok_or_else(|| t!("mcp-diga-a-aba"))?;
        let conexao = id_da_chave(&sessao.key_str())
            .and_then(liberada)
            .ok_or_else(|| t!("mcp-diga-a-aba"))?;
        Ok(Alvo {
            conexao: Some(conexao),
            sessao: Some(sessao),
        })
    }

    /// A linha do cursor sem o prompt: o que o Enter vai mandar rodar.
    pub(super) fn linha_do_cursor(sessao: Session) -> String {
        sessao
            .emulator(|e| {
                let (linha, _) = e.cursor();
                e.line_text(linha)
            })
            .unwrap_or_default()
    }

    /// Atende um pedido de ferramenta.
    fn atender_pedido_mcp(self: &Rc<Self>, pedido: Pedido) {
        if pedido.foi_cancelado() {
            return;
        }
        self.sincronizar_sessoes();
        if pedido.ferramenta == servidor::AVISOS {
            // O hook do Claude Code: só o texto, sem auditoria nem permissão.
            let (redator, _) = self.redator_da_ia();
            let redator = redator.unwrap_or_else(Redator::desligado);
            let texto = self.avisos_da_sessao(pedido.ligacao, &[]);
            pedido.responder(texto.map_or(Value::Null, |t| Value::String(redator.aplicar(&t))));
            return;
        }
        let trabalho = Trabalho::novo(
            &pedido,
            self.mcp.geracao.get(),
            self.geracao_da_sessao(pedido.ligacao),
        );
        if !DA_CASCA.contains(&pedido.ferramenta.as_str()) {
            let abas = |id: u64| self.abas_da_conexao(id);
            let resposta = self
                .tree
                .with_untracked(|t| operacao::atender(t, &abas, &pedido.ferramenta, &pedido.args));
            let saida = match resposta {
                Resposta::Resultado(resultado) => Saida::Bruto(resultado),
                Resposta::Recusa(codigo, mensagem) => Saida::Jsonrpc(codigo, mensagem),
            };
            self.terminar_mcp(&trabalho, saida);
            return;
        }
        match self.planejar_mcp(&trabalho) {
            Ok(plano) => self.decidir_mcp(trabalho, plano),
            Err(erro) => {
                trabalho.decisao.set(DecisaoRegistrada::Negada);
                self.terminar_mcp(&trabalho, Saida::Erro(erro));
            }
        }
    }

    /// Acha o alvo e diz o que o pedido vai fazer. Valida só o que a decisão precisa; o resto os
    /// executores conferem.
    fn planejar_mcp(self: &Rc<Self>, t: &Rc<Trabalho>) -> Result<Plano, String> {
        let nome = t.pedido.ferramenta.as_str();
        let args = &t.pedido.args;
        let extras = self.config.with_untracked(|c| c.ia.bloqueio_extras.clone());
        let (exige, acao) = match nome {
            "abrir_conexao" => (Exige::Conexao, Acao::Abrir),
            "listar_abas" | "ler_saida" | "listar_tuneis" | "listar_paineis" => {
                (Exige::Nada, Acao::Ver)
            }
            "organizar_paineis" => (Exige::Nada, Acao::Organizar),
            "fechar_aba" => (Exige::Aba, Acao::Fechar),
            "reconectar_aba" => (Exige::Aba, Acao::Reconectar),
            "digitar" => (Exige::Aba, Acao::Digitar),
            "teclas" => (Exige::Aba, Acao::Teclas),
            "executar" => (Exige::Aba, Acao::Rodar),
            "listar_espacos" => (Exige::Nada, Acao::Ver),
            "ler" | "buscar" | "glob" => (Exige::Nada, Acao::Ver),
            "editar" | "escrever" => (Exige::Nada, Acao::Editar),
            "bash" => (Exige::Nada, Acao::Rodar),
            "ler_tela" | "esperar" => (Exige::Aba, Acao::Ver),
            "enviar_arquivo" => (Exige::Conexao, Acao::Enviar),
            "ligar_tunel" | "desligar_tunel" | "encaminhar_porta" | "parar_porta" => {
                (Exige::Conexao, Acao::Tunel)
            }
            _ => (Exige::Conexao, Acao::Ver),
        };
        let mut abre_aba = nome == "abrir_conexao";
        let alvo = if DO_ESPACO.contains(&nome) {
            let (espaco, conexao) = self.espaco_do_pedido(&t.pedido)?;
            abre_aba = self.espaco_abre_aba(&espaco, conexao.id);
            Alvo {
                conexao: Some(conexao),
                sessao: None,
            }
        } else if nome == "abrir_conexao" {
            let valor = args
                .get("conexao")
                .filter(|v| !v.is_null())
                .ok_or_else(|| t!("mcp-conexao-invalida"))?;
            let conexao = self
                .tree
                .with_untracked(|tr| achar_conexao(tr, valor).cloned())?;
            Alvo {
                conexao: Some(conexao),
                sessao: None,
            }
        } else {
            self.alvo_mcp(t.pedido.ligacao, args, exige)?
        };
        let texto = |chave: &str| args[chave].as_str().unwrap_or_default().to_owned();
        let mut grade = None;
        let (detalhe, bloqueio) = match acao {
            Acao::Rodar => {
                let comando = texto("comando");
                let bloqueio = perigoso(&comando, &extras);
                (comando, bloqueio)
            }
            Acao::Digitar => {
                let digitado = texto("texto");
                let bloqueio = perigoso_digitado(&digitado, &extras);
                (digitado, bloqueio)
            }
            Acao::Teclas => {
                let nomes: Vec<String> = args["teclas"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                let bloqueio = if nomes.iter().any(|n| eh_enter(n)) {
                    alvo.sessao
                        .map(Self::linha_do_cursor)
                        .and_then(|linha| bloqueio_da_linha(&linha, &extras))
                } else {
                    None
                };
                (nomes.join(" "), bloqueio)
            }
            Acao::Enviar => (
                t!(
                    "mcp-detalhe-enviar",
                    origem = if args["conteudo"].is_string() {
                        t!("mcp-detalhe-texto-da-ia")
                    } else {
                        texto("origem")
                    },
                    destino = texto("caminho")
                ),
                None,
            ),
            Acao::Editar => (Self::detalhe_da_edicao(nome, args), None),
            Acao::Abrir => {
                let c = alvo.conexao.as_ref();
                let caminho = c
                    .map(|c| self.tree.with_untracked(|tr| caminho_na_arvore(tr, c)))
                    .unwrap_or_default();
                (t!("mcp-detalhe-abrir", conexao = caminho), None)
            }
            Acao::Tunel if nome == "encaminhar_porta" => (
                t!(
                    "mcp-detalhe-encaminhar",
                    destino = format!(
                        "{}:{}",
                        args["host"].as_str().unwrap_or("127.0.0.1"),
                        args["porta"]
                    )
                ),
                None,
            ),
            Acao::Tunel if nome == "parar_porta" => (
                t!(
                    "mcp-detalhe-parar-porta",
                    porta = args["porta_local"].to_string()
                ),
                None,
            ),
            Acao::Tunel => (
                t!(
                    "mcp-detalhe-tunel",
                    tunel = match &args["tunel"] {
                        Value::String(s) => s.clone(),
                        outro => outro.to_string(),
                    }
                ),
                None,
            ),
            Acao::Fechar | Acao::Reconectar => {
                (alvo.sessao.map(|s| s.title()).unwrap_or_default(), None)
            }
            Acao::Organizar => {
                let (janela, colunas, linhas) = self.grade_pedida(args)?;
                grade = Some((self.numero_da_janela(janela), colunas, linhas));
                (format!("{colunas} × {linhas}"), None)
            }
            Acao::Ver => (String::new(), None),
        };
        if nome == "ligar_tunel"
            && let Some(c) = &alvo.conexao
        {
            abre_aba = self.aba_com_tuneis(c.id, alvo.sessao).is_none();
        }
        if let Some(c) = &alvo.conexao {
            t.conexao.set(Some(c.id));
            *t.nome_conexao.borrow_mut() = c.name.clone();
        }
        if let Some(s) = alvo.sessao {
            self.marcar_aba_mcp(t, &s.key_str());
        }
        Ok(Plano {
            acao,
            conexao: alvo.conexao,
            sessao: alvo.sessao,
            detalhe,
            bloqueio,
            dono: None,
            grade,
            abre_aba,
            so_abertura: false,
        })
    }

    /// Decide pela permissão, pela lista de bloqueio e pela posse da aba, e segue: roda,
    /// pergunta ou recusa.
    fn decidir_mcp(self: &Rc<Self>, t: Rc<Trabalho>, mut plano: Plano) {
        let exig = exigencia(&t.pedido.ferramenta);
        let paineis = matches!(
            t.pedido.ferramenta.as_str(),
            "listar_paineis" | "organizar_paineis"
        );
        let pelo_clique = plano
            .sessao
            .is_some_and(|s| self.liberada_pelo_clique(t.pedido.ligacao, &s.key_str()));
        let perm = if paineis {
            Some(self.tree.with_untracked(permissao_mais_alta))
        } else if pelo_clique {
            Some(PermissaoIa::Livre)
        } else {
            plano
                .conexao
                .as_ref()
                .map(|c| self.tree.with_untracked(|tr| permissao_efetiva(tr, c.id)))
        };
        let exig = if t.pedido.ferramenta == "listar_paineis" {
            Exigencia::Ver
        } else {
            exig
        };
        let mut decisao = match perm {
            Some(p) => decidir(p, exig, plano.bloqueio.is_some()),
            None => Decisao::Permitir,
        };
        // Uma aba nova passa pela escolha "Abrir abas novas" da conexão. Quando só ela pergunta,
        // a licença que a pessoa deu à sessão no diálogo abre sem perguntar.
        let mut sempre = false;
        let mut pela_licenca = false;
        if plano.abre_aba
            && let Some(c) = &plano.conexao
        {
            let antes = decisao.clone();
            (decisao, sempre) = decidir_abertura(decisao, self.abrir_aba_da_conexao(c.id));
            plano.so_abertura = sempre && antes == Decisao::Permitir;
            if plano.so_abertura && self.abre_sem_perguntar(t.pedido.ligacao) {
                (decisao, sempre, pela_licenca) = (Decisao::Permitir, false, true);
            }
        }
        // Mudar uma aba de outra sessão pergunta sempre, mesmo em Livre e mesmo com o "resto
        // desta sessão" aprovado em outra aba. Duas sessões pedindo grades diferentes em
        // seguida também: a segunda pergunta.
        if !matches!(decisao, Decisao::Recusar(_)) {
            if let Some(dono) = self.dono_de_outra(&t, &plano) {
                plano.dono = Some(dono);
                sempre = true;
            } else if self.grade_disputada(&t, &plano) {
                sempre = true;
            }
            if sempre {
                decisao = Decisao::Perguntar;
            }
        }
        match decisao {
            Decisao::Permitir => {
                t.decisao.set(if pela_licenca {
                    DecisaoRegistrada::AprovadaSessao
                } else {
                    DecisaoRegistrada::Permitida
                });
                self.executar_mcp(t, plano);
            }
            Decisao::Recusar(motivo) => {
                t.decisao.set(DecisaoRegistrada::Negada);
                self.terminar_mcp(&t, Saida::Erro(motivo));
            }
            Decisao::Perguntar => {
                let chave = (t.pedido.ligacao, t.aba.borrow().clone().unwrap_or_default());
                let ja_aprovada = !sempre
                    && plano.bloqueio.is_none()
                    && plano.sessao.is_some()
                    && self.mcp.aprovadas_sessao.borrow().contains(&chave);
                if ja_aprovada {
                    t.decisao.set(DecisaoRegistrada::AprovadaSessao);
                    self.executar_mcp(t, plano);
                } else {
                    self.enfileirar_aprovacao(t, plano);
                }
            }
        }
    }

    /// A escolha "Abrir abas novas" que vale para a conexão `id` (a dela, a da pasta ou a das
    /// Configurações).
    pub(super) fn abrir_aba_da_conexao(&self, id: u64) -> AbrirAbaIa {
        let padrao = self.config.with_untracked(|c| c.ia.mcp_abrir);
        self.tree
            .with_untracked(|tr| abrir_aba_efetiva(tr, id, padrao))
    }

    /// Se a sessão `ligacao` tem licença para abrir abas novas sem perguntar: a pessoa marcou a
    /// caixa no diálogo há menos tempo que `ia.mcp_abrir_auto`. A licença vencida sai.
    fn abre_sem_perguntar(&self, ligacao: u64) -> bool {
        let duracao = self
            .config
            .with_untracked(|c| c.ia.mcp_abrir_auto.duracao());
        let mut licencas = self.mcp.abrir_auto.borrow_mut();
        let Some(desde) = licencas.get(&ligacao).copied() else {
            return false;
        };
        let vale = match duracao {
            None => false,
            Some(None) => true,
            Some(Some(d)) => desde.elapsed() < d,
        };
        if !vale {
            licencas.remove(&ligacao);
        }
        vale
    }

    /// Confere, na hora de abrir uma aba nova da conexão `id`, se pode. `decidida` diz se a
    /// decisão já contou com a aba nova; se não contou (a aba que o pedido ia usar fechou
    /// enquanto ele esperava), só "Permitir" abre.
    pub(super) fn pode_abrir_aba(&self, id: u64, decidida: bool) -> Result<(), String> {
        match self.abrir_aba_da_conexao(id) {
            AbrirAbaIa::Bloquear => Err(ABRIR_ABA_BLOQUEADA.to_owned()),
            AbrirAbaIa::Perguntar if !decidida => Err(t!("mcp-abrir-aba-fechou")),
            AbrirAbaIa::Perguntar | AbrirAbaIa::Permitir => Ok(()),
        }
    }

    /// Roda a ferramenta já decidida.
    pub(super) fn executar_mcp(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        self.tomar_a_aba_sem_dono(&t, &plano);
        match t.pedido.ferramenta.as_str() {
            "abrir_conexao" => self.mcp_abrir_conexao(t, plano),
            "listar_abas" => self.mcp_listar_abas(t),
            "fechar_aba" => self.mcp_fechar_aba(t, plano),
            "reconectar_aba" => self.mcp_reconectar_aba(t, plano),
            "digitar" => self.mcp_digitar(t, plano),
            "teclas" => self.mcp_teclas(t, plano),
            "ler_tela" => self.mcp_ler_tela(t, plano),
            "esperar" => self.mcp_esperar(t, plano),
            "executar" => self.mcp_executar(t, plano),
            "listar_arquivos" => self.mcp_listar_arquivos(t, plano),
            "baixar_arquivo" => self.mcp_baixar_arquivo(t, plano),
            "enviar_arquivo" => self.mcp_enviar_arquivo(t, plano),
            "listar_tuneis" => self.mcp_listar_tuneis(t, plano),
            "ligar_tunel" => self.mcp_mexer_no_tunel(t, plano, true),
            "desligar_tunel" => self.mcp_mexer_no_tunel(t, plano, false),
            "listar_portas" => self.mcp_listar_portas(t, plano),
            "encaminhar_porta" => self.mcp_encaminhar_porta(t, plano),
            "parar_porta" => self.mcp_parar_porta(t, plano),
            "ler_saida" => self.mcp_ler_saida(t),
            "listar_paineis" => self.mcp_listar_paineis(t),
            "organizar_paineis" => self.mcp_organizar_paineis(t, plano),
            "listar_espacos" => self.mcp_listar_espacos(t),
            "ler" => self.mcp_ler(t, plano),
            "editar" => self.mcp_editar(t, plano),
            "escrever" => self.mcp_escrever(t, plano),
            "buscar" => self.mcp_buscar(t, plano),
            "glob" => self.mcp_glob(t, plano),
            "bash" => self.mcp_bash(t, plano),
            outra => {
                let erro = t!("mcp-ponte-ferramenta-desconhecida", nome = outra);
                self.terminar_mcp(&t, Saida::Jsonrpc(-32602, erro));
            }
        }
    }

    /// Responde ao pedido: limpa, redige, corta, põe no envelope e grava a auditoria.
    pub(super) fn terminar_mcp(self: &Rc<Self>, t: &Rc<Trabalho>, saida: Saida) {
        if t.terminado.replace(true) {
            return;
        }
        self.soltar_aba_mcp(t);
        let (redator, _) = self.redator_da_ia();
        let redator = redator.unwrap_or_else(Redator::desligado);
        let (limite, auditar) = self.config.with_untracked(|c| {
            (
                t.caracteres
                    .unwrap_or(c.ia.caracteres_por_resposta as usize),
                c.ia.auditoria,
            )
        });
        let nome = t.nome_conexao.borrow().clone();
        let mut erro = None;
        let mut rpc = None;
        let mut resultado = match saida {
            Saida::App(texto) => text_result(&redator.aplicar(&texto)),
            Saida::Erro(texto) => {
                let texto = redator.aplicar(&texto);
                erro = Some(texto.clone());
                error_result(&texto)
            }
            Saida::Bruto(valor) => {
                if valor["isError"].as_bool().unwrap_or(false) {
                    erro = valor["content"][0]["text"].as_str().map(str::to_owned);
                }
                valor
            }
            Saida::Jsonrpc(codigo, mensagem) => {
                erro = Some(mensagem.clone());
                rpc = Some((codigo, mensagem));
                Value::Null
            }
            Saida::Servidor {
                cabecalho,
                origem,
                texto,
                limitar,
            } => {
                let redigido = redator.aplicar(&sem_controle(&texto));
                let cortado = if limitar {
                    self.mcp.saidas.borrow_mut().limitar(
                        t.conexao.get().unwrap_or(0),
                        redigido,
                        limite,
                    )
                } else {
                    redigido
                };
                let corpo = envelope(&nome, &origem, &cortado);
                let tudo = match cabecalho {
                    Some(c) => format!("{}\n{corpo}", redator.aplicar(&c)),
                    None => corpo,
                };
                text_result(&tudo)
            }
        };
        if rpc.is_none() {
            let aba: Vec<String> = t.aba.borrow().iter().cloned().collect();
            if let Some(nota) = self.avisos_da_sessao(t.pedido.ligacao, &aba) {
                super::mcp_soltos::somar_nota(&mut resultado, &redator.aplicar(&nota));
            }
        }
        if auditar {
            self.gravar_auditoria(t, &redator, erro.clone());
        }
        match rpc {
            Some((codigo, mensagem)) => t.pedido.recusar(codigo, &mensagem),
            None => t.pedido.responder(resultado),
        }
    }

    fn gravar_auditoria(&self, t: &Trabalho, redator: &Redator, erro: Option<String>) {
        let registro = Registro {
            ponte: t.pedido.pid,
            sessao: t.pedido.sessao.clone(),
            ferramenta: t.pedido.ferramenta.clone(),
            conexao: t.conexao.get(),
            aba: t.aba.borrow().clone(),
            args: auditoria::args_para_gravar(&t.pedido.args, redator),
            decisao: t.decisao.get(),
            motivo: t.motivo.borrow().clone(),
            resultado: if erro.is_some() { "erro" } else { "ok" }.to_owned(),
            erro,
            duracao_ms: u64::try_from(t.inicio.elapsed().as_millis()).unwrap_or(u64::MAX),
        };
        let agora = jiff::Zoned::now();
        let quando = agora.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string();
        let dia = agora.strftime("%Y-%m-%d").to_string();
        let linha = auditoria::linha(&quando, &registro);
        // Um registro que não grava não derruba o pedido: a pessoa já viu o que aconteceu.
        let _ = auditoria::gravar(self.armazenamento.pasta(), &dia, &linha);
    }

    /// Um passo de uma tarefa que espera: acaba se a pessoa parou a IA ou o assistente
    /// cancelou; senão roda `passo` e agenda o próximo.
    pub(super) fn passo_mcp(self: &Rc<Self>, t: Rc<Trabalho>, mut passo: Passo) {
        if t.pedido.foi_cancelado() || self.parado(&t) {
            let parada = t!("mcp-a-pessoa-parou");
            self.terminar_mcp(&t, Saida::Erro(parada));
            return;
        }
        match passo(self) {
            Sonda::Fim(saida) => self.terminar_mcp(&t, saida),
            Sonda::Seguiu => {}
            Sonda::Depois(espera) => {
                let eu = self.eu.clone();
                self.alarmes.depois(espera, move || {
                    if let Some(tela) = eu.upgrade() {
                        tela.passo_mcp(t, passo);
                    }
                });
            }
        }
    }

    /// Roda `trabalho` numa thread e termina o pedido com o que `fim` fizer do resultado.
    pub(super) fn em_thread<R: Send + 'static>(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        prazo: Option<Duration>,
        trabalho: impl FnOnce() -> R + Send + 'static,
        fim: impl FnOnce(&Rc<Tela>, R) -> Saida + 'static,
    ) {
        self.em_thread_cancelavel(t, prazo, |_| trabalho(), move |tela, r| Some(fim(tela, r)));
    }

    /// Como [`Tela::em_thread`], mas `trabalho` recebe a bandeira que vira `true` quando o
    /// pedido acaba sem ele (a pessoa parou, o assistente cancelou, o prazo passou), para ele
    /// largar o que faz. Se `fim` devolve `None`, ele mesmo cuida de terminar o pedido.
    pub(super) fn em_thread_cancelavel<R: Send + 'static>(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        prazo: Option<Duration>,
        trabalho: impl FnOnce(Arc<AtomicBool>) -> R + Send + 'static,
        fim: impl FnOnce(&Rc<Tela>, R) -> Option<Saida> + 'static,
    ) {
        /// Levanta a bandeira quando a tarefa de espera acaba, de qualquer jeito.
        struct Cancela(Arc<AtomicBool>);
        impl Drop for Cancela {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Relaxed);
            }
        }
        let inicio = Instant::now();
        let (envia, recebe) = mpsc::channel();
        let cancelado = Arc::new(AtomicBool::new(false));
        let da_thread = Arc::clone(&cancelado);
        thread::spawn(move || {
            let _ = envia.send(trabalho(da_thread));
        });
        let cancela = Cancela(cancelado);
        let mut fim = Some(fim);
        self.passo_mcp(
            t,
            Box::new(move |tela| {
                let _ = &cancela;
                match recebe.try_recv() {
                    Ok(resultado) => match fim.take() {
                        Some(fim) => match fim(tela, resultado) {
                            Some(saida) => Sonda::Fim(saida),
                            None => Sonda::Seguiu,
                        },
                        None => Sonda::Fim(Saida::Erro(t!("mcp-sem-resposta"))),
                    },
                    Err(TryRecvError::Empty) if prazo.is_some_and(|p| inicio.elapsed() >= p) => {
                        Sonda::Fim(Saida::Erro(t!("mcp-prazo-acabou")))
                    }
                    Err(TryRecvError::Empty) => Sonda::Depois(Duration::from_millis(50)),
                    Err(TryRecvError::Disconnected) => {
                        Sonda::Fim(Saida::Erro(t!("mcp-sem-resposta")))
                    }
                }
            }),
        );
    }

    /// Marca a aba `chave` como a de um assistente trabalhando, até o pedido acabar.
    pub(super) fn marcar_aba_mcp(&self, t: &Trabalho, chave: &str) {
        if t.aba.borrow().is_some() {
            return;
        }
        *t.aba.borrow_mut() = Some(chave.to_owned());
        let mut mapa = self.mcp.atividade.borrow_mut();
        let atividade = mapa.entry(chave.to_owned()).or_insert_with(|| Atividade {
            quando: Instant::now(),
            assistente: String::new(),
            ligacao: 0,
            acao: String::new(),
            vistas: Vec::new(),
            em_curso: 0,
        });
        atividade.em_curso += 1;
        atividade.quando = Instant::now();
        atividade.assistente.clone_from(&t.pedido.sessao);
        atividade.ligacao = t.pedido.ligacao;
        atividade.acao.clone_from(&t.pedido.ferramenta);
        if !atividade.vistas.contains(&t.pedido.ligacao) {
            atividade.vistas.push(t.pedido.ligacao);
        }
        drop(mapa);
        self.mexeu_a_marca();
    }

    /// Tira o pedido da conta da aba e agenda o fim da marca.
    fn soltar_aba_mcp(self: &Rc<Self>, t: &Trabalho) {
        let Some(chave) = t.aba.borrow().clone() else {
            return;
        };
        if let Some(atividade) = self.mcp.atividade.borrow_mut().get_mut(&chave) {
            atividade.em_curso = atividade.em_curso.saturating_sub(1);
            atividade.quando = Instant::now();
        }
        self.mexeu_a_marca();
        let marca = self.config.with_untracked(|c| c.ia.marca_secs);
        let eu = self.eu.clone();
        self.alarmes.depois(
            Duration::from_secs(u64::from(marca)) + Duration::from_millis(100),
            move || {
                if let Some(tela) = eu.upgrade() {
                    tela.mexeu_a_marca();
                }
            },
        );
    }

    pub(super) fn mexeu_a_marca(&self) {
        self.mcp.marca_v.update(|v| *v += 1);
    }

    /// Se um assistente mexeu na aba há menos que `ia.marca_secs`, ou ainda está mexendo. Lê a
    /// versão da marca: serve dentro de um efeito ou de uma view.
    pub(super) fn aba_marcada(&self, chave: &str) -> bool {
        let _ = self.mcp.marca_v.get();
        let marca = u64::from(self.config.with_untracked(|c| c.ia.marca_secs));
        self.mcp
            .atividade
            .borrow()
            .get(chave)
            .is_some_and(|a| a.em_curso > 0 || a.quando.elapsed() < Duration::from_secs(marca))
    }

    /// Se um assistente está usando a aba: a que ele abriu (e é dono), a que o "Controlar com"
    /// deu a uma ponte ligada (ou o "Dar esta aba também a") ou a que ele mexeu há pouco. Não lê
    /// signal nenhum.
    pub(super) fn aba_com_assistente(&self, chave: &str) -> bool {
        self.mcp.donos.borrow().contains_key(chave)
            || self.mcp.alvos.borrow().values().any(|a| a == chave)
            || self
                .mcp
                .dadas
                .borrow()
                .values()
                .any(|d| d.iter().any(|a| a == chave))
            || untrack(|| self.aba_marcada(chave))
    }

    /// Se a aba caída tem uma reconexão sozinha marcada: a tentativa e o total.
    pub(super) fn reconexao_marcada(&self, sessao: Session) -> Option<(u32, u32)> {
        self.reconectando.borrow().get(&sessao).copied()
    }

    /// O erro de uma ferramenta que pede a aba de pé: diz se o app já está reconectando.
    pub(super) fn erro_aba_caida(&self, sessao: Session) -> String {
        match self.reconexao_marcada(sessao) {
            Some((n, max)) => t!("mcp-aba-reconectando", n = n, max = max),
            None => t!("mcp-aba-caida"),
        }
    }

    /// O assistente que mexeu na aba, o que fez e há quantos segundos, para a dica.
    pub(super) fn atividade_da_aba(&self, chave: &str) -> Option<(String, String, u64)> {
        let _ = self.mcp.marca_v.get();
        if !self.aba_marcada(chave) {
            return None;
        }
        self.mcp.atividade.borrow().get(chave).map(|a| {
            (
                a.assistente.clone(),
                a.acao.clone(),
                a.quando.elapsed().as_secs(),
            )
        })
    }

    /// Se alguma aba, de qualquer janela, está marcada. Lê a versão da marca.
    pub(super) fn alguma_aba_marcada(&self) -> bool {
        let _ = self.mcp.marca_v.get();
        let marca = u64::from(self.config.with_untracked(|c| c.ia.marca_secs));
        self.mcp
            .atividade
            .borrow()
            .values()
            .any(|a| a.em_curso > 0 || a.quando.elapsed() < Duration::from_secs(marca))
    }

    /// Quantas abas da janela estão marcadas.
    pub(super) fn abas_marcadas(&self, janela: Janela) -> usize {
        let _ = self.mcp.marca_v.get();
        janela
            .layout
            .with(|l| l.sessions())
            .iter()
            .filter(|s| self.aba_marcada(&s.key_str()))
            .count()
    }

    /// "Parar a IA": recusa o que espera, acaba o que roda e esquece as aprovações da sessão.
    /// O MCP continua ligado.
    pub(super) fn parar_ia_mcp(self: &Rc<Self>) {
        self.mcp.geracao.set(self.mcp.geracao.get() + 1);
        self.mcp.aprovadas_sessao.borrow_mut().clear();
        self.mcp.abrir_auto.borrow_mut().clear();
        self.recusar_todas_as_aprovacoes(&t!("mcp-a-pessoa-parou"));
        self.mcp.atividade.borrow_mut().clear();
        self.mcp.donos.borrow_mut().clear();
        self.mexeu_a_marca();
    }

    /// O pedido `ler_saida`: o resto de uma saída que foi cortada.
    fn mcp_ler_saida(self: &Rc<Self>, t: Rc<Trabalho>) {
        let args = &t.pedido.args;
        let Some(id) = args["saida"].as_str() else {
            let erro = t!("mcp-falta-argumento", nome = "saida");
            return self.terminar_mcp(&t, Saida::Erro(erro));
        };
        let inicio = args["inicio"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0);
        let tamanho = args["tamanho"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok());
        let padrao = self
            .config
            .with_untracked(|c| c.ia.caracteres_por_resposta as usize);
        let conexao = self.mcp.saidas.borrow().conexao_de(id);
        let liberada = conexao.is_none_or(|c| {
            self.conexao_liberada_pelo_clique(t.pedido.ligacao, c)
                || self
                    .tree
                    .with_untracked(|tr| permissao_efetiva(tr, c) != PermissaoIa::Desligada)
        });
        if !liberada {
            let erro = t!("mcp-conexao-nao-liberada");
            return self.terminar_mcp(&t, Saida::Erro(erro));
        }
        let lido = self.mcp.saidas.borrow().ler(id, inicio, tamanho, padrao);
        match lido {
            Ok(trecho) => {
                t.conexao.set(Some(trecho.conexao));
                if let Some(nome) = self
                    .tree
                    .with_untracked(|tr| tr.connection(trecho.conexao).map(|c| c.name.clone()))
                {
                    *t.nome_conexao.borrow_mut() = nome;
                }
                self.terminar_mcp(
                    &t,
                    Saida::Servidor {
                        cabecalho: None,
                        origem: "saida".to_owned(),
                        texto: trecho.texto,
                        limitar: false,
                    },
                );
            }
            Err(erro) => self.terminar_mcp(&t, Saida::Erro(erro)),
        }
    }
}

/// Se a linha do cursor, que o Enter vai mandar rodar, parece perigosa. A linha tem o prompt na
/// frente (`ana@srv:~$ rm -rf /`), e o app não sabe onde ele acaba: confere a linha inteira e o
/// que vem depois de cada `$ `, `# `, `% ` e `> `.
pub(super) fn bloqueio_da_linha(linha: &str, extras: &[String]) -> Option<String> {
    if let Some(motivo) = perigoso(linha, extras) {
        return Some(motivo);
    }
    ["$ ", "# ", "% ", "> "]
        .iter()
        .flat_map(|sep| linha.match_indices(sep).map(move |(i, _)| i + sep.len()))
        .find_map(|depois| perigoso(&linha[depois..], extras))
}
