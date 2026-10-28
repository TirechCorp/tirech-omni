//! "Controlar com" (M63): o clique que abre o Claude Code, o Codex ou o OpenCode deste
//! computador numa aba de shell local, à direita da aba que a pessoa quer controlar, já ligado ao
//! app pelo MCP (`tirech-omni mcp --aba <chave>`) e preso a essa aba.
//!
//! O MCP vai pela linha de comando de cada assistente (veja [`crate::mcp::clientes`]): a
//! configuração da pessoa nunca é alterada. Este módulo monta a linha, procura o programa, abre a
//! aba e avisa quando o assistente não está instalado. A parte da sessão presa à aba (dono,
//! liberação pelo clique, aba padrão) é de `tela/mcp*.rs`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::{PaneDirection, Session};

use super::mcp_sessoes::LUGARES;
use super::{Lugar, Tela};
use crate::abrir::{Senhas, dividir_argumentos, juntar_argumentos};
use crate::configuracoes::{Configuracoes, ConversaAoControlar};
use crate::conversas::{
    self, Conversa, Diario, SessaoDoOpenCode, chave_da_aba, conversa_do_claude, conversa_do_codex,
    ler_aviso_do_codex, notify_da_pessoa, sessao_do_lancamento, sessoes_do_opencode,
};
use crate::espacos::{AssistenteSalvo, id_da_chave};
use crate::ia::{Ferramenta, aspas_externas, ficha};
use crate::janelas::Janela;
use crate::links::DestinoDoLink;
use crate::mcp::clientes::{
    NOME_DA_ABA, conversa_claude, json_claude_na_aba, lancar_claude, lancar_codex, lancar_opencode,
    notify_codex, settings_claude_avisos,
};
use crate::modelo::{Connection, Kind};
use crate::t;

/// Os fim de mensagem que o shell de login dá quando o programa não existe.
const SAIDA_NAO_ENCONTRADO: &str = "código 127";

/// O começo do comando quando o assistente vem pelo nome: as pastas onde os instaladores põem o
/// programa vão no fim do `PATH` do shell de login. O instalador do OpenCode (e o do npm sem
/// sudo, e outros) escreve o `PATH` no `~/.bashrc`, depois da linha que sai quando o shell não é
/// interativo, e o `-l -c` não chega lá. São as pastas da sondagem dos servidores
/// (`ia::SCRIPT_SONDAGEM`), menos as do nvm, que pedem um laço. O `env` procura o programa já com
/// o `PATH` novo, e a linha vale também no fish.
const EXEC_COM_O_PATH: &str = "exec env PATH=\"$PATH:$HOME/.local/bin:$HOME/.opencode/bin:\
     $HOME/.npm-global/bin:$HOME/.bun/bin:$HOME/.volta/bin:$HOME/.asdf/shims:\
     $HOME/.local/share/mise/shims:/opt/homebrew/bin:/usr/local/bin\"";

/// Os arquivos `lancar-<pid>-<n>.json` e `lancar-<pid>-<n>-avisos.json` do Claude Code ficam nesta pasta de dentro da pasta de dados.
const PASTA_DOS_ARQUIVOS: &str = "mcp";

/// As extensões que o Windows tenta, na ordem, quando o nome do programa não tem uma.
const EXTENSOES_DO_WINDOWS: [&str; 3] = [".exe", ".cmd", ".bat"];

/// Quantas conversas anteriores o diálogo de escolha mostra.
const CONVERSAS_NO_DIALOGO: usize = 5;

/// Uma aba de assistente aberta pelo "Controlar com".
#[derive(Clone, Copy)]
struct Lancada {
    /// A aba que o assistente controla; `None` no cluster com IA, que controla várias.
    alvo: Option<Session>,
    ferramenta: Ferramenta,
    /// A aba do próprio assistente.
    sessao: Session,
}

/// O estado do "Controlar com" da tela.
pub(super) struct Estado {
    /// As abas de assistente abertas (as que já se foram saem na próxima abertura).
    lancadas: Signal<Vec<Lancada>>,
    /// O que espera o aviso de ligar o controle pela primeira vez.
    pendente: RefCell<Option<Pendente>>,
    /// O assistente que o último lançamento não achou neste computador.
    nao_achado: Signal<Option<Ferramenta>>,
    /// Se o aviso "O controle foi ligado" está à mostra.
    ligado_agora: Signal<bool>,
    /// As abas que o "Reconectar" pode reabrir: o arquivo de configuração do Claude Code de cada
    /// aba fica no disco até o app abrir de novo.
    arquivos: RefCell<Vec<PathBuf>>,
    /// A conversa de cada aba de assistente, para o diário e para reabrir com o app.
    rastros: RefCell<Vec<Rastro>>,
    /// As abas que a restauração abre e cujos assistentes voltam junto (seção "diário" do
    /// plano do M63): a conexão, o lugar e os assistentes.
    a_relancar: RefCell<Vec<(u64, Lugar, Vec<AssistenteSalvo>)>>,
    /// Enquanto as abas de um espaço abrem, as que já abriram esperam aqui para os assistentes
    /// voltarem só no fim: o painel novo à direita mexeria nos painéis que ainda vão encher.
    relancar_depois: RefCell<Option<Vec<AbaComAssistentes>>>,
    /// O diálogo "Continuar a conversa?".
    pub(super) escolha: Escolha,
}

/// Uma aba aberta pela restauração e os assistentes que voltam com ela.
type AbaComAssistentes = (Session, Vec<AssistenteSalvo>);

/// O que o diálogo abre com a resposta: a janela, a aba, o assistente e a conversa de cada opção.
type PedidoDeConversa = (Janela, Session, Ferramenta, Vec<Conversar>);

/// O diálogo que pergunta qual conversa o "Controlar com" abre.
pub(super) struct Escolha {
    pub(super) aberto: Signal<bool>,
    /// "A aba … já conversou com o …".
    texto: Signal<String>,
    /// As opções, na ordem: as conversas anteriores e, por último, "Nova conversa".
    rotulos: Signal<Vec<String>>,
    /// A opção marcada (o próprio rótulo).
    marcada: Signal<String>,
    lembrar: Signal<bool>,
    /// O que abrir com a resposta.
    pedido: RefCell<Option<PedidoDeConversa>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            lancadas: signal(Vec::new()),
            pendente: RefCell::new(None),
            nao_achado: signal(None),
            ligado_agora: signal(false),
            arquivos: RefCell::new(Vec::new()),
            rastros: RefCell::new(Vec::new()),
            a_relancar: RefCell::new(Vec::new()),
            relancar_depois: RefCell::new(None),
            escolha: Escolha {
                aberto: signal(false),
                texto: signal(String::new()),
                rotulos: signal(Vec::new()),
                marcada: signal(String::new()),
                lembrar: signal(false),
                pedido: RefCell::new(None),
            },
        }
    }
}

/// A conversa que o assistente abre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Conversar {
    Nova,
    /// Continua a conversa deste id.
    Continuar(String),
}

/// O que a linha de comando leva da conversa.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Retomada {
    /// Sem o diário (ou o OpenCode numa conversa nova): a linha de sempre.
    Nenhuma,
    /// Uma conversa nova do Claude Code, com o id que o app escolheu.
    NovaComId(String),
    /// Uma conversa nova do Codex: o `notify` grava o id em `aviso` e chama o da pessoa.
    NovaComAviso { aviso: PathBuf, notify: Vec<String> },
    /// Continua a conversa deste id.
    Continuar(String),
}

/// A conversa de uma aba de assistente.
struct Rastro {
    /// A aba do assistente.
    sessao: Session,
    /// A chave da aba controlada no diário; `None` no cluster com IA.
    chave: Option<String>,
    ferramenta: Ferramenta,
    /// O id da conversa, quando já se sabe.
    id: Option<String>,
    /// Quando o assistente abriu, em segundos desde 1970.
    inicio: u64,
    /// Onde o `notify` do Codex grava o id.
    aviso: Option<PathBuf>,
    /// A pasta de trabalho, já sem o `~`.
    pasta: PathBuf,
}

/// A abertura de um assistente que espera o aviso de ligar o controle.
type Pendente = Box<dyn FnOnce(&Rc<Tela>)>;

/// Onde o programa do assistente está.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Onde {
    /// Um nome que o shell de login do Linux e do macOS acha no `PATH` dele.
    Nome(String),
    /// Um arquivo que já se sabe onde está.
    Arquivo(PathBuf),
}

impl Onde {
    fn texto(&self) -> String {
        match self {
            Self::Nome(nome) => nome.clone(),
            Self::Arquivo(caminho) => caminho.to_string_lossy().into_owned(),
        }
    }
}

/// Como o assistente é aberto: o programa, os argumentos e o ambiente da aba de shell local.
#[derive(Debug, PartialEq, Eq)]
struct Linha {
    programa: String,
    argumentos: Vec<String>,
    ambiente: Vec<(String, String)>,
}

/// O que a montagem da linha precisa de fora.
struct Pedido<'a> {
    ferramenta: Ferramenta,
    /// O executável do app (o da ponte).
    exe: &'a str,
    /// O nome da sessão MCP (`--nome`).
    nome: &'a str,
    /// Os argumentos da ponte (`mcp --nome … --aba …`, ou `--cluster`).
    ponte: &'a [String],
    /// O que vai para o prompt de sistema do Claude Code: a aba é o alvo, não este computador. O
    /// Codex e o OpenCode recebem o mesmo texto pela ponte (`--instrucoes`, já em `ponte`).
    instrucoes: &'a str,
    /// O arquivo de configuração do Claude Code (só ele usa).
    arquivo: &'a Path,
    /// O arquivo dos settings com o hook dos avisos (só o Claude Code usa).
    avisos: &'a Path,
    onde: &'a Onde,
    /// Os argumentos extras das Configurações, já divididos.
    extras: &'a [String],
    /// O shell de login do Linux e do macOS.
    shell: &'a str,
    windows: bool,
    retomada: &'a Retomada,
}

/// Monta a linha de comando do assistente (seção 3.3 do plano do M63).
///
/// No Linux e no macOS roda pelo shell de login (`$SHELL -l -c 'exec …'`), para o `PATH` ser o do
/// terminal da pessoa, mais as pastas dos instaladores ([`EXEC_COM_O_PATH`]). No Windows roda o arquivo achado; um `.cmd` ou `.bat` (os atalhos do npm)
/// roda por `cmd /c`.
///
/// A conversa (o diário) entra assim: o Claude Code com `--session-id` ou `--resume`; o Codex com
/// o `notify` que avisa o id, ou como `codex resume … <id>`; o OpenCode com `--session`.
fn montar_linha(p: &Pedido) -> Linha {
    let ponte = p.ponte;
    let (mut argumentos, ambiente) = match p.ferramenta {
        Ferramenta::Claude => {
            let mut argumentos = lancar_claude(p.arquivo, p.avisos, p.nome, p.instrucoes);
            match p.retomada {
                Retomada::NovaComId(id) => argumentos.extend(conversa_claude(id, false)),
                Retomada::Continuar(id) => argumentos.extend(conversa_claude(id, true)),
                Retomada::Nenhuma | Retomada::NovaComAviso { .. } => {}
            }
            (argumentos, Vec::new())
        }
        Ferramenta::Codex => {
            let mut argumentos = lancar_codex(p.exe, ponte);
            match p.retomada {
                Retomada::NovaComAviso { aviso, notify } => {
                    argumentos.extend(notify_codex(p.exe, aviso, notify));
                }
                Retomada::Continuar(id) => {
                    argumentos.insert(0, "resume".to_owned());
                    argumentos.push(id.clone());
                }
                Retomada::Nenhuma | Retomada::NovaComId(_) => {}
            }
            (argumentos, Vec::new())
        }
        Ferramenta::OpenCode => {
            let (mut argumentos, ambiente) = lancar_opencode(p.exe, ponte);
            if let Retomada::Continuar(id) = p.retomada {
                argumentos.extend(["--session".to_owned(), id.clone()]);
            }
            (argumentos, ambiente)
        }
    };
    argumentos.extend(p.extras.iter().cloned());
    linha_do_programa(&p.onde.texto(), argumentos, ambiente, p.shell, p.windows)
}

/// A linha que roda `programa` com `argumentos`: pelo shell de login no Linux e no macOS; no
/// Windows, direto, ou por `cmd /c` se for um `.cmd` ou `.bat`.
fn linha_do_programa(
    programa: &str,
    argumentos: Vec<String>,
    ambiente: Vec<(String, String)>,
    shell: &str,
    windows: bool,
) -> Linha {
    let programa = programa.to_owned();
    if windows {
        let em_lote = programa.to_ascii_lowercase().ends_with(".cmd")
            || programa.to_ascii_lowercase().ends_with(".bat");
        return if em_lote {
            let mut todos = vec!["/c".to_owned(), programa];
            todos.extend(argumentos);
            Linha {
                programa: "cmd".to_owned(),
                argumentos: todos,
                ambiente,
            }
        } else {
            Linha {
                programa,
                argumentos,
                ambiente,
            }
        };
    }
    let exec = if programa.contains('/') {
        "exec"
    } else {
        EXEC_COM_O_PATH
    };
    let palavras: Vec<String> = std::iter::once(programa)
        .chain(argumentos)
        .map(|a| aspas_externas(&a))
        .collect();
    Linha {
        programa: shell.to_owned(),
        argumentos: vec![
            "-l".to_owned(),
            "-c".to_owned(),
            format!("{exec} {}", palavras.join(" ")),
        ],
        ambiente,
    }
}

/// Procura `nome` nas `pastas`, com as extensões do Windows quando ele não tem uma.
fn procurar_no_windows(nome: &str, pastas: &[PathBuf]) -> Option<PathBuf> {
    pastas.iter().find_map(|pasta| {
        let exato = pasta.join(nome);
        if exato.is_file() {
            return Some(exato);
        }
        EXTENSOES_DO_WINDOWS
            .iter()
            .map(|ext| pasta.join(format!("{nome}{ext}")))
            .find(|c| c.is_file())
    })
}

/// Apaga os `lancar-*.json` e os `conversa-*.json` (o id da conversa do Codex) que os usos
/// anteriores do app deixaram. Chamado na abertura do app: durante o uso eles ficam, porque o
/// "Reconectar" reabre o mesmo comando.
pub(super) fn limpar_arquivos_antigos(pasta_de_dados: &Path) {
    // Numa segunda cópia do app, os arquivos são da primeira, que pode estar abrindo um assistente.
    if crate::armazenamento::so_leitura(pasta_de_dados) {
        return;
    }
    let Ok(entradas) = std::fs::read_dir(pasta_de_dados.join(PASTA_DOS_ARQUIVOS)) else {
        return;
    };
    for entrada in entradas.flatten() {
        let nome = entrada.file_name();
        let nome = nome.to_string_lossy();
        if (nome.starts_with("lancar-") || nome.starts_with("conversa-")) && nome.ends_with(".json")
        {
            let _ = std::fs::remove_file(entrada.path());
        }
    }
}

/// O que muda entre o "Controlar com" e o cluster com IA na abertura do assistente.
pub(super) struct Encomenda {
    /// O que vem depois do nome do assistente no título da aba e no nome da sessão.
    pub(super) rotulo: String,
    /// Os argumentos da ponte depois do `--nome`: `--aba <chave>` ou `--cluster`.
    pub(super) ponte: Vec<String>,
    /// O que vai para o prompt de sistema do assistente: o do Claude Code pelo
    /// `--append-system-prompt`; o do Codex e o do OpenCode pela ponte, no `initialize`.
    pub(super) instrucoes: String,
    /// Onde a aba do assistente abre.
    pub(super) lugar: Lugar,
    /// A conversa que o assistente abre.
    pub(super) conversa: Conversar,
    /// A chave da aba controlada no diário; `None` no cluster com IA.
    pub(super) chave: Option<String>,
}

/// Quem o submenu controla: uma aba certa ou a aba atual na hora do clique. Vive só o tempo
/// de um clique e precisa ser `Copy` para entrar nos closures dos menus.
#[derive(Clone, Copy)]
#[allow(clippy::large_enum_variant)]
pub(super) enum Alvo {
    Aba(Janela, Session),
    Atual,
}

/// A chave de `alvo` no diário das conversas: a da conexão da aba.
fn chave_do_alvo(alvo: Session) -> Option<String> {
    id_da_chave(&alvo.key_str()).map(chave_da_aba)
}

/// O `notify` que a pessoa pôs no `config.toml` do Codex, para o do app chamar depois.
fn notify_do_codex(pessoal: Option<&Path>) -> Vec<String> {
    conversas::config_do_codex(pessoal)
        .and_then(|pasta| std::fs::read_to_string(pasta.join("config.toml")).ok())
        .and_then(|texto| notify_da_pessoa(&texto))
        .unwrap_or_default()
}

/// `rotulo`, com um número no fim se já está em `rotulos` (as opções do diálogo são os rótulos).
fn sem_repetir(rotulos: &[String], rotulo: String) -> String {
    if !rotulos.contains(&rotulo) {
        return rotulo;
    }
    (2..)
        .map(|n| format!("{rotulo} ({n})"))
        .find(|r| !rotulos.contains(r))
        .unwrap_or(rotulo)
}

/// Confere se as conversas ainda existem, cada assistente do seu jeito. Sem como conferir (a
/// pasta pessoal não é conhecida, o OpenCode não listou), a conversa vale.
pub(super) struct Verificador {
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
    opencode: Option<Vec<SessaoDoOpenCode>>,
    /// A pasta de trabalho, onde o Claude Code guarda as conversas.
    pasta: PathBuf,
}

impl Verificador {
    /// O título da conversa `id` de `f` (vazio se não se sabe), se ela ainda existe.
    pub(super) fn titulo(&self, f: Ferramenta, id: &str) -> Option<String> {
        match f {
            Ferramenta::Claude => match &self.claude {
                Some(config) => conversa_do_claude(config, &self.pasta, id),
                None => Some(String::new()),
            },
            Ferramenta::Codex => match &self.codex {
                Some(config) => conversa_do_codex(config, id),
                None => Some(String::new()),
            },
            Ferramenta::OpenCode => match &self.opencode {
                Some(lista) => lista.iter().find(|s| s.id == id).map(|s| s.titulo.clone()),
                None => Some(String::new()),
            },
        }
    }
}

impl Tela {
    /// A janela e a sessão que `alvo` indica agora.
    fn resolver_controle(&self, alvo: Alvo) -> Option<(Janela, Session)> {
        match alvo {
            Alvo::Aba(janela, sessao) => Some((janela, sessao)),
            Alvo::Atual => {
                let janela = self.janela_ativa();
                self.sessao_ativa_de(janela).map(|s| (janela, s))
            }
        }
    }

    /// Se `sessao` é a aba de um assistente aberto pelo "Controlar com".
    pub(super) fn e_aba_de_assistente(&self, sessao: Session) -> bool {
        self.controlar
            .lancadas
            .with(|todas| todas.iter().any(|l| l.sessao == sessao))
    }

    /// O clique em "Controlar com ▸ {ferramenta}" (seção 3 do plano do M63).
    pub(super) fn controlar_com(self: &Rc<Self>, janela: Janela, alvo: Session, f: Ferramenta) {
        if alvo.closed() || self.e_aba_de_assistente(alvo) {
            return;
        }
        // Clicar de novo, com o mesmo assistente ainda aberto, só traz a aba dele para a frente.
        if self.trazer_assistente(alvo, f) {
            return;
        }
        let depois = move |t: &Rc<Tela>| {
            if !alvo.closed() {
                t.escolher_conversa(janela, alvo, f);
            }
        };
        if self.controle_ligado(depois) {
            self.escolher_conversa(janela, alvo, f);
        }
    }

    /// Escolhe a conversa do assistente `f` para `alvo` e o abre. Se a aba já teve conversas com
    /// ele, pergunta qual (ou segue o que as Configurações mandam); senão, abre uma nova.
    fn escolher_conversa(self: &Rc<Self>, janela: Janela, alvo: Session, f: Ferramenta) {
        let (lembrar, modo) = self
            .config
            .with_untracked(|c| (c.ia.conversas_lembrar, c.ia.conversas_ao_controlar));
        let anteriores = match chave_do_alvo(alvo) {
            Some(chave) if lembrar && modo != ConversaAoControlar::Nova => {
                self.conversas_da_aba(&chave, f)
            }
            _ => Vec::new(),
        };
        let Some(primeira) = anteriores.first() else {
            self.lancar_assistente(janela, alvo, f, Conversar::Nova);
            return;
        };
        if modo == ConversaAoControlar::Continuar {
            let id = primeira.id.clone();
            self.lancar_assistente(janela, alvo, f, Conversar::Continuar(id));
            return;
        }
        self.perguntar_conversa(janela, alvo, f, &anteriores);
    }

    /// As conversas de `f` com a aba `chave` que ainda existem, a mais recente primeiro, com o
    /// título de agora. As que sumiram saem do diário; as que um assistente aberto está usando
    /// ficam nele, mas não na lista.
    fn conversas_da_aba(&self, chave: &str, f: Ferramenta) -> Vec<Conversa> {
        self.resolver_conversas(f == Ferramenta::OpenCode);
        let verificador = self.verificador(f == Ferramenta::OpenCode);
        let abertas: Vec<String> = self
            .controlar
            .rastros
            .borrow()
            .iter()
            .filter(|r| !r.sessao.closed())
            .filter_map(|r| r.id.clone())
            .collect();
        let mut diario = Diario::ler(&self.armazenamento);
        let mut ficam = Vec::new();
        let mut vivas = Vec::new();
        for mut conversa in diario.da_aba(chave, f.chave()) {
            if abertas.contains(&conversa.id) {
                ficam.push(conversa.id);
                continue;
            }
            let Some(titulo) = verificador.titulo(f, &conversa.id) else {
                continue;
            };
            if !titulo.is_empty() {
                conversa.titulo = titulo;
            }
            ficam.push(conversa.id.clone());
            vivas.push(conversa);
        }
        diario.esquecer_sumidas(chave, f.chave(), &ficam);
        diario.atualizar_titulos(chave, &vivas);
        diario.gravar(&self.armazenamento);
        vivas
    }

    /// Mostra o diálogo "Continuar a conversa?" com as `anteriores` (as primeiras) e "Nova
    /// conversa".
    fn perguntar_conversa(
        &self,
        janela: Janela,
        alvo: Session,
        f: Ferramenta,
        anteriores: &[Conversa],
    ) {
        let formato = t!("conversas-formato-da-data");
        let mut rotulos: Vec<String> = Vec::new();
        let mut opcoes = Vec::new();
        for conversa in anteriores.iter().take(CONVERSAS_NO_DIALOGO) {
            let data = conversas::data(conversa.quando, &formato);
            let rotulo = if conversa.titulo.is_empty() {
                t!("conversas-continuar-sem-titulo", data = data.as_str())
            } else {
                t!(
                    "conversas-continuar",
                    titulo = conversa.titulo.as_str(),
                    data = data.as_str()
                )
            };
            rotulos.push(sem_repetir(&rotulos, rotulo));
            opcoes.push(Conversar::Continuar(conversa.id.clone()));
        }
        rotulos.push(t!("conversas-nova"));
        opcoes.push(Conversar::Nova);
        let escolha = &self.controlar.escolha;
        let aba = alvo.title();
        escolha.texto.set(t!(
            "conversas-texto",
            nome = ficha(f).nome,
            aba = aba.as_str()
        ));
        escolha.marcada.set(rotulos[0].clone());
        escolha.rotulos.set(rotulos);
        escolha.lembrar.set(false);
        *escolha.pedido.borrow_mut() = Some((janela, alvo, f, opcoes));
        escolha.aberto.set(true);
        self.janelas.principal().handle.focus();
    }

    /// A resposta do diálogo "Continuar a conversa?": `abrir` com a opção marcada, ou nada.
    fn responder_conversa(self: &Rc<Self>, abrir: bool) {
        let escolha = &self.controlar.escolha;
        escolha.aberto.set(false);
        let Some((janela, alvo, f, opcoes)) = escolha.pedido.borrow_mut().take() else {
            return;
        };
        if !abrir || alvo.closed() {
            return;
        }
        let marcada = escolha.marcada.get_untracked();
        let n = escolha
            .rotulos
            .with_untracked(|r| r.iter().position(|x| *x == marcada))
            .unwrap_or(0);
        let conversa = opcoes.get(n).cloned().unwrap_or(Conversar::Nova);
        if escolha.lembrar.get_untracked() {
            let modo = match conversa {
                Conversar::Nova => ConversaAoControlar::Nova,
                Conversar::Continuar(_) => ConversaAoControlar::Continuar,
            };
            self.config.update(|c| c.ia.conversas_ao_controlar = modo);
        }
        if !self.trazer_assistente(alvo, f) {
            self.lancar_assistente(janela, alvo, f, conversa);
        }
    }

    /// Quem confere se as conversas ainda existem. `listar_opencode` roda o
    /// `opencode session list` (devagar); sem ele, as do OpenCode valem.
    pub(super) fn verificador(&self, listar_opencode: bool) -> Verificador {
        let pessoal = self.ferramentas.ambiente().pessoal.clone();
        let pasta =
            crate::armazenamento::expandir_til_em(&self.pasta_de_trabalho(), pessoal.as_deref());
        Verificador {
            claude: conversas::config_do_claude(pessoal.as_deref()),
            codex: conversas::config_do_codex(pessoal.as_deref()),
            opencode: if listar_opencode {
                self.sessoes_do_opencode_em(&pasta)
            } else {
                None
            },
            pasta,
        }
    }

    /// As sessões do OpenCode na pasta de trabalho `pasta` (`opencode session list --format
    /// json`, rodado nela). `None` se não deu para rodar.
    fn sessoes_do_opencode_em(&self, pasta: &Path) -> Option<Vec<SessaoDoOpenCode>> {
        let onde = self.onde_esta(Ferramenta::OpenCode)?;
        let ambiente = self.ferramentas.ambiente();
        let shell = ambiente
            .shell
            .clone()
            .unwrap_or_else(|| "/bin/sh".to_owned());
        let argumentos = ["session", "list", "--format", "json"]
            .map(str::to_owned)
            .to_vec();
        let mut linha = linha_do_programa(
            &onde.texto(),
            argumentos,
            Vec::new(),
            &shell,
            ambiente.windows,
        );
        // A lista é a do projeto da pasta onde roda.
        let pasta = pasta.to_string_lossy().into_owned();
        if ambiente.windows {
            let mut todos = ["/c", "cd", "/d", pasta.as_str(), "&&"]
                .map(str::to_owned)
                .to_vec();
            todos.push(linha.programa);
            todos.extend(linha.argumentos);
            linha.programa = "cmd".to_owned();
            linha.argumentos = todos;
        } else if let Some(comando) = linha.argumentos.last_mut() {
            *comando = format!("cd {} && {comando}", aspas_externas(&pasta));
        }
        let argumentos: Vec<&str> = linha.argumentos.iter().map(String::as_str).collect();
        let saida = (ambiente.executor)(&linha.programa, &argumentos).ok()?;
        saida.contains('[').then(|| sessoes_do_opencode(&saida))
    }

    /// Descobre o id das conversas que ainda não se sabe: o do Codex pelo arquivo do `notify`
    /// (barato) e, com `com_opencode`, o do OpenCode pela lista de sessões. As achadas entram no
    /// diário. Os rastros de abas fechadas que já têm o id saem.
    pub(super) fn resolver_conversas(&self, com_opencode: bool) {
        let lembrar = self.config.with_untracked(|c| c.ia.conversas_lembrar);
        let mut achadas = Vec::new();
        {
            let mut rastros = self.controlar.rastros.borrow_mut();
            rastros.retain(|r| r.id.is_none() || !r.sessao.closed());
            let mut listas: Vec<(PathBuf, Vec<SessaoDoOpenCode>)> = Vec::new();
            for n in 0..rastros.len() {
                if rastros[n].id.is_some() {
                    continue;
                }
                let achada = match rastros[n].ferramenta {
                    Ferramenta::Codex => rastros[n]
                        .aviso
                        .as_deref()
                        .and_then(ler_aviso_do_codex)
                        .map(|a| (a.id, a.titulo)),
                    Ferramenta::OpenCode if com_opencode => {
                        let pasta = rastros[n].pasta.clone();
                        if !listas.iter().any(|(p, _)| *p == pasta) {
                            let lista = self.sessoes_do_opencode_em(&pasta).unwrap_or_default();
                            listas.push((pasta.clone(), lista));
                        }
                        let pegas: Vec<String> = rastros
                            .iter()
                            .filter(|r| r.ferramenta == Ferramenta::OpenCode)
                            .filter_map(|r| r.id.clone())
                            .collect();
                        listas
                            .iter()
                            .find(|(p, _)| *p == pasta)
                            .and_then(|(_, lista)| {
                                sessao_do_lancamento(
                                    lista,
                                    &pasta.to_string_lossy(),
                                    rastros[n].inicio,
                                    &pegas,
                                )
                            })
                            .map(|s| (s.id.clone(), s.titulo.clone()))
                    }
                    _ => None,
                };
                let Some((id, titulo)) = achada else {
                    continue;
                };
                let rastro = &mut rastros[n];
                rastro.id = Some(id.clone());
                if lembrar && let Some(chave) = &rastro.chave {
                    achadas.push((
                        chave.clone(),
                        Conversa {
                            ferramenta: rastro.ferramenta.chave().to_owned(),
                            id,
                            quando: rastro.inicio,
                            titulo,
                        },
                    ));
                }
            }
        }
        if achadas.is_empty() {
            return;
        }
        let mut diario = Diario::ler(&self.armazenamento);
        for (chave, conversa) in achadas {
            diario.anotar(&chave, conversa);
        }
        diario.gravar(&self.armazenamento);
    }

    /// Os assistentes vivos que controlam `alvo`, com a conversa de cada um, para o espaço de
    /// trabalho (e o `estado.json`) os trazer de volta.
    pub(super) fn assistentes_salvos_de(&self, alvo: Session) -> Vec<AssistenteSalvo> {
        self.controlar.lancadas.with_untracked(|todas| {
            todas
                .iter()
                .filter(|l| l.alvo == Some(alvo) && !l.sessao.closed())
                .map(|l| AssistenteSalvo {
                    ferramenta: l.ferramenta.chave().to_owned(),
                    conversa: self.conversa_da_aba_de_assistente(l.sessao),
                })
                .collect()
        })
    }

    /// O id da conversa da aba de assistente `sessao`, se já se sabe.
    pub(super) fn conversa_da_aba_de_assistente(&self, sessao: Session) -> Option<String> {
        self.controlar
            .rastros
            .borrow()
            .iter()
            .find(|r| r.sessao == sessao)
            .and_then(|r| r.id.clone())
    }

    /// A conversa que um assistente salvo abre de novo: a mesma, se o diário está ligado e ela
    /// ainda existe; senão, uma nova.
    pub(super) fn conversa_salva(
        &self,
        f: Ferramenta,
        id: Option<&str>,
        verificador: &Verificador,
    ) -> Conversar {
        let lembrar = self.config.with_untracked(|c| c.ia.conversas_lembrar);
        match id {
            Some(id) if lembrar && verificador.titulo(f, id).is_some() => {
                Conversar::Continuar(id.to_owned())
            }
            _ => Conversar::Nova,
        }
    }

    /// Se os assistentes salvos voltam quando as abas reabrem (com o app ou com um espaço).
    pub(super) fn reabre_assistentes(&self) -> bool {
        self.config
            .with_untracked(|c| c.ia.conversas_reabrir && c.mcp_no_ar())
    }

    /// Guarda os `assistentes` que voltam com a aba da conexão `id` (0 é o shell local) que vai
    /// abrir em `lugar`.
    pub(super) fn relancar_com(&self, id: u64, lugar: Lugar, assistentes: &[AssistenteSalvo]) {
        if assistentes.is_empty() || !self.reabre_assistentes() {
            return;
        }
        self.controlar
            .a_relancar
            .borrow_mut()
            .push((id, self.fixar(lugar), assistentes.to_vec()));
    }

    /// A aba da conexão `id` abriu em `lugar`: os assistentes que esperavam por ela voltam, ou
    /// esperam o fim do espaço que está abrindo.
    pub(super) fn aba_aberta(&self, id: u64, lugar: Lugar, sessao: Session) {
        let assistentes = {
            let mut fila = self.controlar.a_relancar.borrow_mut();
            let Some(n) = fila.iter().position(|(i, l, _)| *i == id && *l == lugar) else {
                return;
            };
            fila.remove(n).2
        };
        if let Some(prontas) = self.controlar.relancar_depois.borrow_mut().as_mut() {
            prontas.push((sessao, assistentes));
            return;
        }
        if let Some(tela) = self.eu.upgrade() {
            tela.relancar(sessao, &assistentes);
        }
    }

    /// Começa a abrir as abas de um espaço: os assistentes delas esperam o fim. `false` se já
    /// havia um começado (quem começou é quem termina).
    pub(super) fn comecar_espaco(&self) -> bool {
        let mut prontas = self.controlar.relancar_depois.borrow_mut();
        if prontas.is_some() {
            return false;
        }
        *prontas = Some(Vec::new());
        true
    }

    /// As abas do espaço já abriram: os assistentes delas voltam. Os das abas que não abriram
    /// nem esperam a senha saem da fila.
    pub(super) fn terminar_espaco(&self) {
        let prontas = self
            .controlar
            .relancar_depois
            .borrow_mut()
            .take()
            .unwrap_or_default();
        let esperando: Vec<(u64, Lugar)> = self
            .pendentes
            .with_untracked(|p| p.iter().map(|p| (p.id, p.lugar)).collect());
        self.controlar
            .a_relancar
            .borrow_mut()
            .retain(|(id, lugar, _)| esperando.contains(&(*id, *lugar)));
        let Some(tela) = self.eu.upgrade() else {
            return;
        };
        for (sessao, assistentes) in prontas {
            tela.relancar(sessao, &assistentes);
        }
    }

    /// Abre de novo, à direita de `alvo`, os `assistentes` que a controlavam.
    fn relancar(self: &Rc<Self>, alvo: Session, assistentes: &[AssistenteSalvo]) {
        let Some((janela, _)) = self
            .sessoes_das_janelas(None)
            .into_iter()
            .find(|(_, s)| *s == alvo)
        else {
            return;
        };
        let verificador = self.verificador(false);
        for salvo in assistentes {
            let Some(f) = Ferramenta::da_chave(&salvo.ferramenta) else {
                continue;
            };
            if alvo.closed() || self.trazer_assistente(alvo, f) {
                continue;
            }
            let conversa = self.conversa_salva(f, salvo.conversa.as_deref(), &verificador);
            self.lancar_assistente(janela, alvo, f, conversa);
        }
    }

    /// Se o controle por assistentes está ligado para abrir um agora. Desligado, liga sem
    /// perguntar quando a pessoa já viu o aviso; senão mostra o aviso, guarda `depois` para
    /// quando ela aceitar e devolve `false`.
    pub(super) fn controle_ligado(&self, depois: impl FnOnce(&Rc<Tela>) + 'static) -> bool {
        let (ligado, avisado) = self
            .config
            .with_untracked(|c| (c.ia.mcp_ligado, c.ia.mcp_avisado));
        if !ligado {
            if !avisado {
                self.mostrar_aviso_mcp();
                *self.controlar.pendente.borrow_mut() = Some(Box::new(depois));
                return false;
            }
            self.config.update(|c| c.ia.mcp_ligado = true);
            self.controlar.ligado_agora.set(true);
        }
        true
    }

    /// Esquece o pedido que esperava o aviso de ligar o controle.
    pub(super) fn limpar_controle_pendente(&self) {
        self.controlar.pendente.borrow_mut().take();
    }

    /// O aviso de ligar o controle foi aceito: segue com o lançamento que estava esperando.
    pub(super) fn retomar_controle(self: &Rc<Self>) {
        let pendente = self.controlar.pendente.borrow_mut().take();
        if let Some(depois) = pendente {
            depois(self);
        }
    }

    /// Anota `sessao` como a aba de um assistente: o "Controlar com" não aparece nela, ela não
    /// entra nos espaços de trabalho e o código 127 do shell vira o aviso de não instalado.
    pub(super) fn anotar_assistente(
        &self,
        alvo: Option<Session>,
        ferramenta: Ferramenta,
        sessao: Session,
    ) {
        let vivas: Vec<Session> = self
            .sessoes_das_janelas(None)
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        self.controlar.lancadas.update(|todas| {
            todas.retain(|l| vivas.contains(&l.sessao));
            todas.push(Lancada {
                alvo,
                ferramenta,
                sessao,
            });
        });
    }

    /// As abas de assistente vivas, sem rastrear.
    pub(super) fn abas_de_assistente(&self) -> Vec<Session> {
        self.controlar.lancadas.with_untracked(|todas| {
            todas
                .iter()
                .filter(|l| !l.sessao.closed())
                .map(|l| l.sessao)
                .collect()
        })
    }

    /// Traz para a frente a aba do assistente `f` que já controla `alvo`, se há uma viva.
    fn trazer_assistente(&self, alvo: Session, f: Ferramenta) -> bool {
        let existente = self
            .controlar
            .lancadas
            .with_untracked(|todas| {
                todas
                    .iter()
                    .find(|l| l.alvo == Some(alvo) && l.ferramenta == f && !l.sessao.closed())
                    .map(|l| l.sessao)
            })
            .and_then(|sessao| {
                self.sessoes_das_janelas(None)
                    .into_iter()
                    .find(|(_, s)| *s == sessao)
            });
        let Some((janela, sessao)) = existente else {
            return false;
        };
        janela.layout.update(|l| l.select(sessao));
        janela.handle.focus();
        self.mostrar_terminais();
        true
    }

    /// Onde está o programa de `f`: o caminho das Configurações vence a procura.
    fn onde_esta(&self, f: Ferramenta) -> Option<Onde> {
        let configurado = self
            .config
            .with_untracked(|c| c.ia.caminho_local_de(f).trim().to_owned());
        let ambiente = self.ferramentas.ambiente();
        let nome = if configurado.is_empty() {
            ficha(f).binario.to_owned()
        } else if configurado.contains(['/', '\\']) || configurado.starts_with('~') {
            let caminho = crate::armazenamento::expandir_til(&configurado);
            return caminho.is_file().then_some(Onde::Arquivo(caminho));
        } else {
            configurado
        };
        if ambiente.windows {
            procurar_no_windows(&nome, &ambiente.pastas_de_programas).map(Onde::Arquivo)
        } else {
            Some(Onde::Nome(nome))
        }
    }

    /// A segunda metade do clique: abre a aba do assistente à direita de `alvo`, na `conversa`.
    fn lancar_assistente(
        self: &Rc<Self>,
        janela: Janela,
        alvo: Session,
        f: Ferramenta,
        conversa: Conversar,
    ) {
        let titulo_do_alvo = alvo.title();
        let encomenda = Encomenda {
            rotulo: titulo_do_alvo.clone(),
            ponte: vec!["--aba".to_owned(), alvo.key_str()],
            instrucoes: t!(
                "controlar-instrucoes",
                aba = titulo_do_alvo.as_str(),
                servidor = NOME_DA_ABA
            ),
            lugar: self.lugar_a_direita(janela, alvo),
            conversa,
            chave: chave_do_alvo(alvo),
        };
        let Some((sessao, _)) = self.lancar_local(f, encomenda) else {
            return;
        };
        self.anotar_assistente(Some(alvo), f, sessao);
        self.mostrar_terminais();
    }

    /// Abre o assistente `f` numa aba de shell local em `encomenda.lugar`, já ligado ao app pela
    /// ponte. Devolve a aba e o código do lançamento, que a ponte manda ao app ao ligar; `None`
    /// se o programa não foi achado (o aviso já está na tela) ou o arquivo do Claude Code não
    /// foi gravado.
    pub(super) fn lancar_local(
        self: &Rc<Self>,
        f: Ferramenta,
        encomenda: Encomenda,
    ) -> Option<(Session, String)> {
        let ficha = ficha(f);
        let Some(onde) = self.onde_esta(f) else {
            self.controlar.nao_achado.set(Some(f));
            return None;
        };
        self.controlar.nao_achado.set(None);
        let ambiente = self.ferramentas.ambiente();
        let exe = ambiente
            .exe
            .as_ref()
            .map_or_else(|| "tirech-omni".to_owned(), |c| c.display().to_string());
        let rotulo = encomenda.rotulo.as_str();
        let nome = t!("controlar-nome-da-sessao", nome = ficha.nome, aba = rotulo);
        let n = self.contador.get_untracked() + 1;
        // Único entre as cópias do app e as aberturas: o hook e o cluster acham a sessão por ele,
        // e os arquivos de uma cópia não passam por cima dos da outra.
        let lancamento = format!("{}-{n}", std::process::id());
        let arquivo = self
            .armazenamento
            .pasta()
            .join(PASTA_DOS_ARQUIVOS)
            .join(format!("lancar-{lancamento}.json"));
        let avisos = arquivo.with_file_name(format!("lancar-{lancamento}-avisos.json"));
        let mut ponte = ["mcp", "--nome", nome.as_str()].map(str::to_owned).to_vec();
        ponte.extend(encomenda.ponte);
        ponte.extend(["--lancamento".to_owned(), lancamento.clone()]);
        // O endereço do canal vai escrito: o Codex roda a ponte sem o `XDG_RUNTIME_DIR` (e sem
        // o `TIRECH_OMNI_CONFIG`), e ela não acharia o app.
        let canal = crate::mcp::endereco_da_pasta(self.armazenamento.pasta());
        ponte.extend(["--canal".to_owned(), canal.to_string_lossy().into_owned()]);
        // O Codex e o OpenCode não têm o `--append-system-prompt` do Claude Code: as instruções
        // vão à ponte, que as devolve no `initialize`, e eles as põem no prompt de sistema.
        if f != Ferramenta::Claude {
            ponte.extend(["--instrucoes".to_owned(), encomenda.instrucoes.clone()]);
        }
        if f == Ferramenta::Claude {
            let settings = settings_claude_avisos(&exe, &lancamento, ambiente.windows);
            let gravar = |caminho: &Path, conteudo: String| {
                caminho
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(caminho, conteudo))
                    .map_err(|erro| (caminho.to_owned(), erro))
            };
            let gravou = gravar(&arquivo, json_claude_na_aba(&exe, &ponte))
                .and_then(|()| gravar(&avisos, settings));
            if let Err((caminho, erro)) = gravou {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "comum-nao-consegui-gravar",
                        arquivo = caminho.display().to_string(),
                        erro = erro.to_string()
                    ),
                );
                return None;
            }
            self.controlar
                .arquivos
                .borrow_mut()
                .extend([arquivo.clone(), avisos.clone()]);
        }
        let extras = dividir_argumentos(
            &self
                .config
                .with_untracked(|c| c.ia.argumentos_locais_de(f).to_owned()),
        );
        let shell = ambiente
            .shell
            .clone()
            .unwrap_or_else(|| "/bin/sh".to_owned());
        let lembrar = self.config.with_untracked(|c| c.ia.conversas_lembrar);
        let retomada = match (encomenda.conversa, f) {
            (Conversar::Continuar(id), _) => Retomada::Continuar(id),
            (Conversar::Nova, _) if !lembrar => Retomada::Nenhuma,
            (Conversar::Nova, Ferramenta::Claude) => Retomada::NovaComId(conversas::uuid_novo()),
            (Conversar::Nova, Ferramenta::Codex) => Retomada::NovaComAviso {
                aviso: arquivo.with_file_name(format!("conversa-{lancamento}.json")),
                notify: notify_do_codex(ambiente.pessoal.as_deref()),
            },
            (Conversar::Nova, Ferramenta::OpenCode) => Retomada::Nenhuma,
        };
        let pasta = self.pasta_de_trabalho();
        let linha = montar_linha(&Pedido {
            ferramenta: f,
            exe: &exe,
            nome: &nome,
            ponte: &ponte,
            instrucoes: &encomenda.instrucoes,
            arquivo: &arquivo,
            avisos: &avisos,
            onde: &onde,
            extras: &extras,
            shell: &shell,
            windows: ambiente.windows,
            retomada: &retomada,
        });
        let mut conexao = Connection::new(
            t!("controlar-titulo", nome = ficha.nome, aba = rotulo),
            Kind::Local,
        );
        conexao.local_programa = linha.programa;
        conexao.local_argumentos = juntar_argumentos(&linha.argumentos);
        conexao.local_ambiente = linha.ambiente;
        conexao.local_pasta = pasta.clone();
        let sessao =
            self.abrir_e_devolver(&conexao, Senhas::default(), encomenda.lugar, false, false);
        let (id, aviso) = match retomada {
            Retomada::NovaComId(id) | Retomada::Continuar(id) => (Some(id), None),
            Retomada::NovaComAviso { aviso, .. } => (None, Some(aviso)),
            Retomada::Nenhuma => (None, None),
        };
        if lembrar || id.is_some() {
            let inicio = conversas::agora();
            if lembrar && let (Some(chave), Some(id)) = (&encomenda.chave, &id) {
                let mut diario = Diario::ler(&self.armazenamento);
                diario.anotar(
                    chave,
                    Conversa {
                        ferramenta: f.chave().to_owned(),
                        id: id.clone(),
                        quando: inicio,
                        titulo: String::new(),
                    },
                );
                diario.gravar(&self.armazenamento);
            }
            self.controlar.rastros.borrow_mut().push(Rastro {
                sessao,
                chave: encomenda.chave,
                ferramenta: f,
                id,
                inicio,
                aviso,
                pasta: crate::armazenamento::expandir_til_em(&pasta, ambiente.pessoal.as_deref()),
            });
        }
        Some((sessao, lancamento))
    }

    /// A pasta onde o assistente começa: a de `ia.local_pasta`, criada se falta. Sem ela (não deu
    /// para criar), a pasta pessoal, com um aviso.
    fn pasta_de_trabalho(&self) -> String {
        let escolhida = self.config.with_untracked(|c| c.ia.local_pasta.clone());
        let Some(pasta) = self
            .ferramentas
            .ambiente()
            .pasta_dos_assistentes(&escolhida)
        else {
            return "~".to_owned();
        };
        match std::fs::create_dir_all(&pasta) {
            Ok(()) => pasta.display().to_string(),
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Warning,
                    t!(
                        "controlar-pasta-de-trabalho-falhou",
                        pasta = pasta.display().to_string(),
                        erro = erro.to_string()
                    ),
                );
                "~".to_owned()
            }
        }
    }

    /// O painel à direita da aba `alvo`: o vizinho, se a janela tem um, ou um painel novo.
    fn lugar_a_direita(&self, janela: Janela, alvo: Session) -> Lugar {
        let janela = self
            .sessoes_das_janelas(None)
            .into_iter()
            .find(|(_, s)| *s == alvo)
            .map_or(janela, |(j, _)| j);
        let painel = Cell::new(None);
        janela.layout.update(|l| {
            if let Some(origem) = l.pane_of(alvo) {
                let destino = l
                    .neighbor(origem, PaneDirection::Right)
                    .unwrap_or_else(|| l.split(origem, PaneDirection::Right));
                painel.set(Some(destino));
            }
        });
        Lugar {
            janela: Some(janela.handle),
            painel: painel.get(),
        }
    }

    /// Abre a página de instalação de `f` no navegador (a primeira instalação da ficha).
    fn abrir_instalacao(&self, f: Ferramenta) {
        let Some(instalacao) = ficha(f).instalacoes.first() else {
            return;
        };
        let destino = DestinoDoLink::Navegador(instalacao.doc.to_owned());
        if let Err(erro) = self.opener.abrir_link(&destino) {
            self.avisar(
                InfoBarSeverity::Warning,
                t!("links-nao-abriu", erro = erro.to_string()),
            );
        }
    }

    /// Fecha a aba de um assistente que o shell de login não achou (código 127) e avisa.
    fn assistente_nao_achado(&self, lancada: Lancada) {
        self.controlar.nao_achado.set(Some(lancada.ferramenta));
        self.controlar
            .lancadas
            .update(|todas| todas.retain(|l| l.sessao != lancada.sessao));
        let da_aba = self
            .sessoes_das_janelas(None)
            .into_iter()
            .filter(|(_, s)| *s == lancada.sessao)
            .collect::<Vec<_>>();
        for janela in self.tirar_sessoes(da_aba, false) {
            self.fechar_se_vazia(janela);
        }
    }

    /// Fecha as abas dos assistentes que controlavam `alvo`, cuja aba acabou de ser fechada: sem
    /// a aba, o assistente não tem mais o que controlar.
    pub(super) fn fechar_assistentes_de(&self, alvo: Session) {
        let mut deles = Vec::new();
        self.controlar.lancadas.update(|todas| {
            todas.retain(|l| {
                let dele = l.alvo == Some(alvo);
                if dele {
                    deles.push(l.sessao);
                }
                !dele
            });
        });
        if deles.is_empty() {
            return;
        }
        let abas = self
            .sessoes_das_janelas(None)
            .into_iter()
            .filter(|(_, s)| deles.contains(s))
            .collect::<Vec<_>>();
        for janela in self.tirar_sessoes(abas, false) {
            self.fechar_se_vazia(janela);
        }
    }

    /// Vigia as abas de assistente: o código 127 do shell de login é "programa não encontrado".
    pub(super) fn vigiar_assistentes(self: &Rc<Self>) {
        let t = Rc::clone(self);
        effect(move || {
            let achadas: Vec<Lancada> = t
                .controlar
                .lancadas
                .get()
                .into_iter()
                .filter(|l| {
                    l.sessao
                        .close_reason()
                        .is_some_and(|motivo| motivo.ends_with(SAIDA_NAO_ENCONTRADO))
                })
                .collect();
            if achadas.is_empty() {
                return;
            }
            untrack(|| {
                for lancada in achadas {
                    t.assistente_nao_achado(lancada);
                }
            });
        });
    }
}

/// Os avisos do "Controlar com", no alto da janela: o assistente que não foi achado (com "Como
/// instalar") e o controle que foi ligado pelo clique (com "Desligar").
pub(super) fn avisos(tela: &Rc<Tela>) -> impl View {
    let (nao_achado, ligado) = (tela.controlar.nao_achado, tela.controlar.ligado_agora);
    let (instalar, desligar) = (Rc::clone(tela), Rc::clone(tela));
    Column::new()
        .child(Show::when(
            move || nao_achado.get().is_some(),
            move || {
                let agir = Rc::clone(&instalar);
                InfoBar::new(InfoBarSeverity::Warning)
                    .id("controlar-aviso-nao-achado")
                    .message(move || match nao_achado.get() {
                        Some(f) => t!("controlar-nao-achado", nome = ficha(f).nome),
                        None => String::new(),
                    })
                    .on_close(move || nao_achado.set(None))
                    .action(t!("controlar-como-instalar"), move || {
                        if let Some(f) = nao_achado.get_untracked() {
                            nao_achado.set(None);
                            agir.abrir_instalacao(f);
                        }
                    })
            },
        ))
        .child(Show::when(
            move || ligado.get(),
            move || {
                let agir = Rc::clone(&desligar);
                InfoBar::new(InfoBarSeverity::Informational)
                    .id("controlar-aviso-ligado")
                    .message(move || t!("controlar-ligado"))
                    .on_close(move || ligado.set(false))
                    .action(t!("controlar-desligar"), move || {
                        ligado.set(false);
                        agir.config.update(|c| c.ia.mcp_ligado = false);
                    })
            },
        ))
}

/// O diálogo "Continuar a conversa?" do "Controlar com": as conversas anteriores da aba com o
/// assistente, "Nova conversa" e "Lembrar a escolha". Mora na janela principal.
pub(super) fn dialogo(tela: &Rc<Tela>) -> impl View + use<> {
    let escolha = &tela.controlar.escolha;
    let (aberto, texto, rotulos, marcada, lembrar) = (
        escolha.aberto,
        escolha.texto,
        escolha.rotulos,
        escolha.marcada,
        escolha.lembrar,
    );
    ContentDialog::new(t!("conversas-titulo"), aberto)
        .id("conversas-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(move || texto.get()).id("conversas-texto").wrap())
                // As opções de um RadioButtons são fixas: o grupo nasce de novo a cada abertura.
                .child(Show::when(
                    move || aberto.get(),
                    move || {
                        RadioButtons::bind(marcada, rotulos.get_untracked()).id("conversas-opcoes")
                    },
                ))
                .child(CheckBox::bind(lembrar, t!("conversas-lembrar")).id("conversas-lembrar")),
        )
        .primary(t!("comum-abrir"), {
            let t = Rc::clone(tela);
            move || t.responder_conversa(true)
        })
        .close_with(t!("comum-cancelar"), {
            let t = Rc::clone(tela);
            move || t.responder_conversa(false)
        })
}

/// Os assistentes dos submenus, cada um com o seu ícone, na ordem de sempre.
pub(super) const ICONES: [(Ferramenta, Symbol); 3] = [
    (Ferramenta::Claude, Symbol::Sparkle),
    (Ferramenta::Codex, Symbol::Code),
    (Ferramenta::OpenCode, Symbol::WindowConsole),
];

/// O submenu "Controlar com ▸" com os três assistentes. `prefixo` abre os ids
/// (`{prefixo}-claude`, `-codex` e `-opencode`).
pub(super) fn submenu(tela: &Rc<Tela>, prefixo: &str, alvo: Alvo) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id(format!("{prefixo}-menu"));
    for (ferramenta, icone) in ICONES {
        let (abrir, visivel) = (Rc::clone(tela), Rc::clone(tela));
        menu = menu
            .item(
                format!("{prefixo}-{}", ferramenta.chave()),
                ficha(ferramenta).nome.to_owned(),
                move || {
                    if let Some((janela, sessao)) = abrir.resolver_controle(alvo) {
                        abrir.controlar_com(janela, sessao, ferramenta);
                    }
                },
            )
            .icon(icone)
            .item_visible(move || visivel.config.with(|c| c.ia.mostra(ferramenta)));
    }
    menu
}

/// Se o submenu "Controlar com ▸" aparece para `alvo`: o controle por assistentes está ligado em
/// Geral, há uma aba, ela não é a de um assistente e ao menos um assistente está à mostra nas
/// Configurações.
pub(super) fn aparece(tela: &Rc<Tela>, alvo: Alvo) -> bool {
    let Some((_, sessao)) = tela.resolver_controle_rastreado(alvo) else {
        return false;
    };
    !tela.e_aba_de_assistente(sessao)
        && tela
            .config
            .with(|c| c.geral.mcp && Ferramenta::TODAS.into_iter().any(|f| c.ia.mostra(f)))
}

impl Tela {
    /// Como [`Tela::resolver_controle`], com rastreamento: o item do menu acompanha a aba ativa.
    fn resolver_controle_rastreado(&self, alvo: Alvo) -> Option<(Janela, Session)> {
        match alvo {
            Alvo::Aba(janela, sessao) => Some((janela, sessao)),
            Alvo::Atual => {
                let janela = self.janela_ativa();
                janela
                    .layout
                    .try_with(|l| l.pane(l.active()).selected)
                    .flatten()
                    .map(|s| (janela, s))
            }
        }
    }
}

/// O submenu "Dar esta aba também a ▸": uma linha por sessão ligada, pelo nome, no mesmo lugar
/// dos outros menus de sessões (`{prefixo}-dar-1`, `-2`…). Clicar prende a aba àquela sessão sem
/// trocar a aba padrão dela; a que já tem a aba não aparece.
pub(super) fn dar_tambem(tela: &Rc<Tela>, prefixo: &str, alvo: Alvo) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id(format!("{prefixo}-dar-menu"));
    for n in 0..LUGARES {
        let (nome, visivel, dar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        menu = menu
            .item(
                format!("{prefixo}-dar-{}", n + 1),
                move || nome.sessao_do_lugar(n).map_or_else(String::new, |s| s.nome),
                move || {
                    if let Some((_, sessao)) = dar.resolver_controle(alvo)
                        && !sessao.closed()
                        && !dar.e_aba_de_assistente(sessao)
                        && let Some(ia) = dar.sessao_do_lugar(n)
                    {
                        dar.dar_aba_tambem(ia.ligacao, &sessao.key_str());
                    }
                },
            )
            .icon(Symbol::Sparkle)
            .item_visible(move || {
                let Some((_, sessao)) = visivel.resolver_controle_rastreado(alvo) else {
                    return false;
                };
                let _ = visivel.mcp.marca_v.get();
                visivel
                    .sessao_do_lugar(n)
                    .is_some_and(|ia| !visivel.liberada_pelo_clique(ia.ligacao, &sessao.key_str()))
            });
    }
    menu
}

/// Se o submenu "Dar esta aba também a ▸" aparece para `alvo`: o controle por assistentes está
/// ligado, a aba não é a de um assistente e há uma sessão ligada que ainda não tem esta aba.
pub(super) fn aparece_dar(tela: &Rc<Tela>, alvo: Alvo) -> bool {
    let Some((_, sessao)) = tela.resolver_controle_rastreado(alvo) else {
        return false;
    };
    let _ = tela.mcp.marca_v.get();
    tela.config.with(Configuracoes::mcp_no_ar)
        && !tela.e_aba_de_assistente(sessao)
        && tela.mcp.sessoes.with(|todas| {
            todas
                .iter()
                .take(LUGARES)
                .any(|s| !tela.liberada_pelo_clique(s.ligacao, &sessao.key_str()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::clientes::args_da_ponte_na_aba;

    fn ponte() -> Vec<String> {
        args_da_ponte_na_aba("Claude Code · api", "7-2")
    }

    fn pedido<'a>(
        ferramenta: Ferramenta,
        onde: &'a Onde,
        extras: &'a [String],
        windows: bool,
        ponte: &'a [String],
    ) -> Pedido<'a> {
        Pedido {
            ferramenta,
            exe: "/opt/tirech omni/tirech-omni",
            nome: "Claude Code · api",
            ponte,
            instrucoes: "Use a aba api.",
            arquivo: Path::new("/cfg/mcp/lancar-3.json"),
            avisos: Path::new("/cfg/mcp/lancar-3-avisos.json"),
            onde,
            extras,
            shell: "/bin/zsh",
            windows,
            retomada: &Retomada::Nenhuma,
        }
    }

    #[test]
    fn no_linux_o_assistente_roda_pelo_shell_de_login() {
        let onde = Onde::Nome("claude".to_owned());
        let linha = montar_linha(&pedido(Ferramenta::Claude, &onde, &[], false, &ponte()));
        assert_eq!(linha.programa, "/bin/zsh");
        assert_eq!(
            linha.argumentos,
            [
                "-l",
                "-c",
                &format!(
                    "{EXEC_COM_O_PATH} 'claude' '--mcp-config' '/cfg/mcp/lancar-3.json' \
                     '--settings' '/cfg/mcp/lancar-3-avisos.json' '-n' 'Claude Code · api' \
                     '--append-system-prompt' 'Use a aba api.'"
                )
            ]
        );
        assert!(linha.ambiente.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn o_shell_de_login_acha_o_assistente_que_o_instalador_pos_fora_do_path() {
        use std::os::unix::fs::PermissionsExt;
        let casa = std::env::temp_dir().join(format!("tirech-path-{}", std::process::id()));
        let pasta = casa.join(".opencode/bin");
        std::fs::create_dir_all(&pasta).unwrap();
        let programa = pasta.join("opencode");
        std::fs::write(&programa, "#!/bin/sh\necho achou \"$@\"\n").unwrap();
        std::fs::set_permissions(&programa, std::fs::Permissions::from_mode(0o755)).unwrap();
        let linha = linha_do_programa(
            "opencode",
            vec!["--session".to_owned(), "ses 1".to_owned()],
            Vec::new(),
            "/bin/sh",
            false,
        );
        let saida = std::process::Command::new(&linha.programa)
            .args(&linha.argumentos)
            .env("HOME", &casa)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&casa);
        assert_eq!(
            String::from_utf8_lossy(&saida.stdout),
            "achou --session ses 1\n",
            "{}",
            String::from_utf8_lossy(&saida.stderr)
        );
    }

    #[test]
    fn os_argumentos_extras_vao_no_fim() {
        let onde = Onde::Arquivo(PathBuf::from("/home/ana/bin/claude"));
        let extras = vec!["--dangerously-skip-permissions".to_owned()];
        let linha = montar_linha(&pedido(Ferramenta::Claude, &onde, &extras, false, &ponte()));
        assert!(
            linha.argumentos[2].starts_with("exec '/home/ana/bin/claude' '--mcp-config'"),
            "{:?}",
            linha.argumentos
        );
        assert!(
            linha.argumentos[2].ends_with("'--dangerously-skip-permissions'"),
            "{:?}",
            linha.argumentos
        );
    }

    #[test]
    fn o_codex_sobrepoe_o_servidor_com_tres_c() {
        let onde = Onde::Nome("codex".to_owned());
        let linha = montar_linha(&pedido(Ferramenta::Codex, &onde, &[], false, &ponte()));
        let comando = &linha.argumentos[2];
        assert!(
            comando.starts_with(&format!("{EXEC_COM_O_PATH} 'codex' '-c' ")),
            "{comando}"
        );
        assert!(
            comando.contains("mcp_servers.tirech-omni.command="),
            "{comando}"
        );
        assert!(comando.contains("--aba"), "{comando}");
        assert!(
            comando.contains("'mcp_servers.tirech-omni.tool_timeout_sec=86400'"),
            "{comando}"
        );
    }

    #[test]
    fn o_opencode_leva_a_configuracao_no_ambiente() {
        let onde = Onde::Nome("opencode".to_owned());
        let linha = montar_linha(&pedido(Ferramenta::OpenCode, &onde, &[], false, &ponte()));
        assert_eq!(linha.argumentos[2], format!("{EXEC_COM_O_PATH} 'opencode'"));
        let [(nome, valor)] = linha.ambiente.as_slice() else {
            panic!("{:?}", linha.ambiente);
        };
        assert_eq!(nome, "OPENCODE_CONFIG_CONTENT");
        let json: serde_json::Value = serde_json::from_str(valor).unwrap();
        let comando = &json["mcp"]["tirech-omni"]["command"];
        assert_eq!(comando[3], "Claude Code · api");
        assert_eq!(comando[5], "7-2");
    }

    #[test]
    fn no_windows_o_exe_roda_direto_e_o_cmd_por_cmd_c() {
        let exe = Onde::Arquivo(PathBuf::from(r"C:\Users\Ana\.local\bin\claude.exe"));
        let linha = montar_linha(&pedido(Ferramenta::Claude, &exe, &[], true, &ponte()));
        assert_eq!(linha.programa, r"C:\Users\Ana\.local\bin\claude.exe");
        assert_eq!(linha.argumentos[0], "--mcp-config");

        let lote = Onde::Arquivo(PathBuf::from(r"C:\Users\Ana\AppData\Roaming\npm\codex.CMD"));
        let linha = montar_linha(&pedido(Ferramenta::Codex, &lote, &[], true, &ponte()));
        assert_eq!(linha.programa, "cmd");
        assert_eq!(
            linha.argumentos[..2],
            ["/c", r"C:\Users\Ana\AppData\Roaming\npm\codex.CMD"]
        );
        assert_eq!(linha.argumentos[2], "-c");
    }

    #[test]
    fn a_procura_no_windows_tenta_as_extensoes() {
        let pasta = std::env::temp_dir().join(format!("tirech-controlar-{}", std::process::id()));
        let outra = pasta.join("outra");
        std::fs::create_dir_all(&outra).unwrap();
        std::fs::write(pasta.join("codex.cmd"), "").unwrap();
        std::fs::write(outra.join("claude.exe"), "").unwrap();
        let pastas = vec![pasta.clone(), outra.clone()];
        assert_eq!(
            procurar_no_windows("claude", &pastas),
            Some(outra.join("claude.exe"))
        );
        assert_eq!(
            procurar_no_windows("codex", &pastas),
            Some(pasta.join("codex.cmd"))
        );
        assert_eq!(
            procurar_no_windows("codex.cmd", &pastas),
            Some(pasta.join("codex.cmd"))
        );
        assert_eq!(procurar_no_windows("opencode", &pastas), None);
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn a_limpeza_so_apaga_os_arquivos_de_lancamento() {
        let pasta = std::env::temp_dir().join(format!("tirech-limpar-{}", std::process::id()));
        let mcp = pasta.join(PASTA_DOS_ARQUIVOS);
        std::fs::create_dir_all(&mcp).unwrap();
        std::fs::write(mcp.join("lancar-1.json"), "{}").unwrap();
        std::fs::write(mcp.join("lancar-22.json"), "{}").unwrap();
        std::fs::write(mcp.join("lancar-22-avisos.json"), "{}").unwrap();
        std::fs::write(mcp.join("conversa-22.json"), "{}").unwrap();
        std::fs::write(mcp.join("outro.json"), "{}").unwrap();
        limpar_arquivos_antigos(&pasta);
        assert!(!mcp.join("conversa-22.json").exists());
        assert!(!mcp.join("lancar-1.json").exists());
        assert!(!mcp.join("lancar-22.json").exists());
        assert!(!mcp.join("lancar-22-avisos.json").exists());
        assert!(mcp.join("outro.json").exists());
        let _ = std::fs::remove_dir_all(&pasta);
        // Sem a pasta, não faz nada.
        limpar_arquivos_antigos(&pasta);
    }

    /// A linha de `ferramenta` com a `retomada`, só a parte depois do `exec`.
    fn comando_com(ferramenta: Ferramenta, nome: &str, retomada: Retomada) -> String {
        let onde = Onde::Nome(nome.to_owned());
        let extras = vec!["--extra".to_owned()];
        let ponte = ponte();
        let linha = montar_linha(&Pedido {
            retomada: &retomada,
            ..pedido(ferramenta, &onde, &extras, false, &ponte)
        });
        linha.argumentos[2].clone()
    }

    #[test]
    fn o_claude_comeca_com_o_id_escolhido_ou_continua() {
        let nova = comando_com(
            Ferramenta::Claude,
            "claude",
            Retomada::NovaComId("abc-1".to_owned()),
        );
        assert!(nova.ends_with("'--session-id' 'abc-1' '--extra'"), "{nova}");
        let continuar = comando_com(
            Ferramenta::Claude,
            "claude",
            Retomada::Continuar("abc-1".to_owned()),
        );
        assert!(
            continuar.ends_with("'--resume' 'abc-1' '--extra'"),
            "{continuar}"
        );
        assert!(!continuar.contains("--session-id"), "{continuar}");
    }

    #[test]
    fn o_codex_continua_com_resume_e_avisa_o_id_da_nova_pelo_notify() {
        let continuar = comando_com(
            Ferramenta::Codex,
            "codex",
            Retomada::Continuar("0199-aa".to_owned()),
        );
        assert!(
            continuar.starts_with(&format!("{EXEC_COM_O_PATH} 'codex' 'resume' '-c' ")),
            "{continuar}"
        );
        assert!(continuar.ends_with("'0199-aa' '--extra'"), "{continuar}");

        let nova = comando_com(
            Ferramenta::Codex,
            "codex",
            Retomada::NovaComAviso {
                aviso: PathBuf::from("/cfg/mcp/conversa-3.json"),
                notify: vec!["notify-send".to_owned()],
            },
        );
        assert!(
            nova.starts_with(&format!("{EXEC_COM_O_PATH} 'codex' '-c' ")),
            "{nova}"
        );
        assert!(!nova.contains("'resume'"), "{nova}");
        assert!(nova.contains("notify=["), "{nova}");
        assert!(nova.contains("--conversa"), "{nova}");
        assert!(nova.contains("/cfg/mcp/conversa-3.json"), "{nova}");
        assert!(nova.contains("notify-send"), "{nova}");
        assert!(nova.ends_with("'--extra'"), "{nova}");
    }

    #[test]
    fn o_opencode_continua_com_session() {
        let continuar = comando_com(
            Ferramenta::OpenCode,
            "opencode",
            Retomada::Continuar("ses_1".to_owned()),
        );
        assert_eq!(
            continuar,
            format!("{EXEC_COM_O_PATH} 'opencode' '--session' 'ses_1' '--extra'")
        );
        let nova = comando_com(Ferramenta::OpenCode, "opencode", Retomada::Nenhuma);
        assert_eq!(nova, format!("{EXEC_COM_O_PATH} 'opencode' '--extra'"));
    }

    #[test]
    fn rotulos_repetidos_ganham_um_numero() {
        let rotulos = vec!["Continuar: a".to_owned(), "Continuar: a (2)".to_owned()];
        assert_eq!(
            sem_repetir(&rotulos, "Continuar: b".to_owned()),
            "Continuar: b"
        );
        assert_eq!(
            sem_repetir(&rotulos, "Continuar: a".to_owned()),
            "Continuar: a (3)"
        );
    }
}
