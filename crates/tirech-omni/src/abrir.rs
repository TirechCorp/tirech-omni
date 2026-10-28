//! De uma [`Connection`] para uma [`Session`]: o [`RealOpener`] abre `Ssh`, `Process` e
//! `Telnet`; o [`FakeOpener`] devolve um `Loopback` com eco, para os testes e os snapshots.
//!
//! Nada aqui bloqueia a thread da tela. A senha vem do cofre ou é pedida pela tela **antes** de
//! chamar o [`Opener::open`]; a pergunta sobre a chave do servidor vem da thread da conexão, por uma
//! fila num signal ([`FilaChaves`]) que a tela lê e responde pelo `Sender` de cada pedido.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use ssh_key::PublicKey;
use tomni_ui::files::ftp::{
    CertificateInfo, FtpBuilder, FtpEncoding, FtpMode, FtpSecurity, TlsVersion,
};
use tomni_ui::files::rclone::{Rcd, RcloneError};
use tomni_ui::files::s3::{S3Config, S3UrlStyle, TlsMode};
use tomni_ui::files::sftp::Sftp;
use tomni_ui::files::{Endpoint, Fs, Local, MemFs, Opener as ArquivosOpener};
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::{
    Bind, CopyIdOptions, CopyIdOutcome, ExecOutput, Forward, ForwardState, LocalSocket, RemoteApp,
    RemoteAppStatus, X11, X11Mode, copy_id,
};
use tomni_ui::terminal::{
    HostKey, KnownHost, LogMode, Loopback, LoopbackHandle, Process, ProcessBuilder, Session, Ssh,
    SshBuilder, Status, Telnet, Transport,
};

use crate::colar::servidor_falso::ServidorFalso;
use crate::configuracoes::{ChaveNova, Configuracoes, S3Cfg, X11Padrao};
use crate::ia::COMANDO_SONDAGEM_WINDOWS;
use crate::links::DestinoDoLink;
use crate::modelo::{
    Connection, FtpCodificacao, FtpModo, FtpSeguranca, FtpTls, Jump, Kind, ModoCredencial, ModoX11,
    S3Certificado, S3Estilo, TipoTunel, Tunel,
};
use crate::portas::local::{escolher_porta_local, porta_livre};
use crate::rclone::{self, OpcoesRclone, Rcds};
use crate::redes::RedeResolvida;
use crate::t;

/// Quanto tempo a conexão espera a pessoa responder à pergunta da chave do servidor.
const ESPERA_DA_CHAVE: Duration = Duration::from_secs(300);

/// Confere se o servidor alcança `host:porta` dentro do prazo (M58). Vem de
/// [`Opener::testador_de_destino`] e roda numa thread.
pub type TestadorDeDestino = Arc<dyn Fn(&str, u16, Duration) -> Result<(), String> + Send + Sync>;

/// O que a thread da conexão pergunta à tela sobre a chave do servidor.
#[derive(Debug)]
pub struct PedidoChave {
    /// O nome da conexão (ou `salto host:porta`).
    pub conexao: String,
    /// `host:porta` do servidor.
    pub host_porta: String,
    /// O algoritmo da chave (`ssh-ed25519`).
    pub algoritmo: String,
    /// A impressão digital SHA-256.
    pub fingerprint: String,
    /// `true` se o `known_hosts` tem outra chave para este servidor.
    pub mudou: bool,
    /// Os dados do certificado, quando a pergunta é de um servidor FTPS (e não da chave SSH).
    pub certificado: Option<DadosCertificado>,
    /// Por onde a tela responde: `true` confia e conecta, `false` recusa.
    pub resposta: Sender<bool>,
}

/// O que a pergunta do certificado do FTPS mostra (tudo já em texto).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DadosCertificado {
    /// Por que o sistema não confia nele.
    pub motivo: String,
    /// Quem o certificado diz ser.
    pub assunto: String,
    /// Quem o assinou.
    pub emissor: String,
    /// O período de validade (`de 01/01/2026 a 01/01/2027`).
    pub validade: String,
}

/// A fila de pedidos de chave: a conexão acrescenta, a tela tira. Uma fila (e não um valor só)
/// porque o restauro das abas e os saltos podem perguntar várias vezes ao mesmo tempo.
pub type FilaChaves = Signal<Vec<Arc<PedidoChave>>>;

/// Serializa o acesso à fila entre as threads. O `Signal` não aguenta uma thread lendo enquanto
/// outra escreve: a leitura (e o `try_*`) entra em pânico dizendo "disposed" quando o lock
/// interno está com a outra ponta. A fila é escrita pelas threads de conexão e lida pela da
/// tela, então todo acesso passa por aqui.
static ACESSO_A_FILA: Mutex<()> = Mutex::new(());

fn travar() -> MutexGuard<'static, ()> {
    ACESSO_A_FILA.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Lê a fila de pedidos de qualquer thread, rastreando o signal como o `with` (use dentro de um
/// efeito ou de uma view).
pub fn ler_fila<R>(fila: FilaChaves, f: impl FnOnce(&[Arc<PedidoChave>]) -> R) -> R {
    let _trava = travar();
    fila.with(|v| f(v))
}

/// Lê a fila de pedidos de qualquer thread, sem rastrear (eventos, threads e testes).
pub fn ler_fila_agora<R>(fila: FilaChaves, f: impl FnOnce(&[Arc<PedidoChave>]) -> R) -> R {
    let _trava = travar();
    fila.with_untracked(|v| f(v))
}

/// Altera a fila de pedidos de qualquer thread.
pub fn mexer_na_fila<R>(fila: FilaChaves, f: impl FnOnce(&mut Vec<Arc<PedidoChave>>) -> R) -> R {
    let _trava = travar();
    fila.try_update(f)
        .expect("a fila de chaves do servidor foi descartada")
}

/// As senhas de uma abertura: a da conexão (no S3, a chave secreta), a do salto, a passphrase da
/// chave privada e o token de sessão do S3. `None` tenta só o `ssh-agent` e a chave. Junto vai a
/// rede por onde a conexão sai (M34), já resolvida pela tela.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Senhas {
    /// A senha da conexão.
    pub conexao: Option<String>,
    /// A senha do servidor de salto.
    pub salto: Option<String>,
    /// A passphrase do arquivo de chave da conexão (também usado no salto).
    pub chave: Option<String>,
    /// O token de sessão do S3 (chaves temporárias).
    pub token: Option<String>,
    /// A chave privada do cofre ligada à conexão (`chave_do_cofre`), já aberta: vale também
    /// para o salto, como o arquivo de chave.
    pub chave_cofre: Option<ssh_key::PrivateKey>,
    /// A rede do perfil da conexão; `None` sai direto.
    pub rede: Option<RedeResolvida>,
}

impl Senhas {
    /// Alguma senha (ou passphrase, ou token) veio junto? A rede não conta.
    pub fn tem_segredo(&self) -> bool {
        Self {
            rede: None,
            ..self.clone()
        } != Self::default()
    }
}

// O `Debug` não mostra as senhas: só se existem.
impl std::fmt::Debug for Senhas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let marca = |s: &Option<String>| if s.is_some() { "***" } else { "-" };
        f.debug_struct("Senhas")
            .field("conexao", &marca(&self.conexao))
            .field("salto", &marca(&self.salto))
            .field("chave", &marca(&self.chave))
            .field("token", &marca(&self.token))
            .field("chave_cofre", &self.chave_cofre.is_some())
            .field("rede", &self.rede)
            .finish()
    }
}

/// Um programa gráfico lançado sem aba (o virt-manager remoto, um aplicativo remoto, o
/// virt-manager local): a tela lê o estado dele e pode fechá-lo.
#[derive(Clone)]
pub struct Lancamento {
    origem: Origem,
}

#[derive(Clone)]
enum Origem {
    /// Um programa remoto do `SshBuilder::launch`.
    Remoto(RemoteApp),
    /// Um estado que o app mesmo mantém: o virt-manager local e o `FakeOpener`.
    Controlado(Arc<Controlado>),
}

struct Controlado {
    estado: Mutex<(RemoteAppStatus, String)>,
    fechado: AtomicBool,
}

impl fmt::Debug for Lancamento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lancamento")
            .field("status", &self.status())
            .finish()
    }
}

impl Lancamento {
    /// Embrulha o programa remoto de um `SshBuilder::launch`.
    pub fn remoto(app: RemoteApp) -> Self {
        Self {
            origem: Origem::Remoto(app),
        }
    }

    /// Um lançamento cujo estado quem o criou escreve ([`definir_status`](Self::definir_status)
    /// e [`definir_stderr`](Self::definir_stderr)).
    pub fn controlado(status: RemoteAppStatus) -> Self {
        Self {
            origem: Origem::Controlado(Arc::new(Controlado {
                estado: Mutex::new((status, String::new())),
                fechado: AtomicBool::new(false),
            })),
        }
    }

    /// Em que pé o programa está.
    pub fn status(&self) -> RemoteAppStatus {
        match &self.origem {
            Origem::Remoto(app) => app.status(),
            Origem::Controlado(c) => c
                .estado
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .0
                .clone(),
        }
    }

    /// O fim do stderr do programa, mais os avisos da conexão.
    pub fn stderr(&self) -> String {
        match &self.origem {
            Origem::Remoto(app) => app.stderr_tail(),
            Origem::Controlado(c) => c
                .estado
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .1
                .clone(),
        }
    }

    /// Fecha o programa e a conexão.
    pub fn fechar(&self) {
        match &self.origem {
            Origem::Remoto(app) => app.close(),
            Origem::Controlado(c) => c.fechado.store(true, Ordering::SeqCst),
        }
    }

    /// Se alguém pediu [`fechar`](Self::fechar) (só nos lançamentos controlados).
    pub fn fechado(&self) -> bool {
        match &self.origem {
            Origem::Remoto(_) => false,
            Origem::Controlado(c) => c.fechado.load(Ordering::SeqCst),
        }
    }

    /// Muda o estado de um lançamento controlado; nos remotos não faz nada.
    pub fn definir_status(&self, status: RemoteAppStatus) {
        if let Origem::Controlado(c) = &self.origem {
            c.estado.lock().unwrap_or_else(PoisonError::into_inner).0 = status;
        }
    }

    /// Troca o stderr de um lançamento controlado; nos remotos não faz nada.
    pub fn definir_stderr(&self, texto: &str) {
        if let Origem::Controlado(c) = &self.origem {
            texto.clone_into(&mut c.estado.lock().unwrap_or_else(PoisonError::into_inner).1);
        }
    }

    fn acrescentar_stderr(&self, texto: &str) {
        if let Origem::Controlado(c) = &self.origem {
            let mut estado = c.estado.lock().unwrap_or_else(PoisonError::into_inner);
            estado.1.push_str(texto);
            if estado.1.len() > 4096 {
                let mut corte = estado.1.len() - 4096;
                while !estado.1.is_char_boundary(corte) {
                    corte += 1;
                }
                estado.1.drain(..corte);
            }
        }
    }
}

/// O que a tela pede ao abrir um comando numa aba ([`Opener::open_comando`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PedidoComando {
    /// O título da aba (no lugar do nome da conexão).
    pub titulo: String,
    /// O comando que o servidor roda no lugar do shell.
    pub comando: String,
    /// Os túneis a mais, além dos da conexão.
    pub tuneis: Vec<Forward>,
}

/// Quem transforma uma conexão numa sessão. Roda na thread da tela e não bloqueia.
pub trait Opener {
    /// Abre a sessão. `senhas` vêm do cofre ou do diálogo da tela; `n` conta as abas da mesma
    /// conexão e entra na chave da sessão.
    fn open(&self, conexao: &Connection, senhas: Senhas, n: u32) -> Session;

    /// Entrega as configurações (e a pasta de dados) ao abridor, na partida e a cada mudança:
    /// o shell local, o `known_hosts`, a chave de servidor nova, o encaminhamento do agente e os
    /// logs valem da próxima sessão em diante.
    fn configurar(&self, _config: &Configuracoes, _pasta: &Path) {}

    /// Abre outra conexão ao servidor da sessão `origem`, com a mesma autenticação, rodando
    /// `comando` no lugar do shell (`None`: o shell). Serve à cópia de uma aba do tmux, que roda
    /// outra sessão, e a encerrar a sessão do tmux de uma aba que fechou. `None` se a sessão
    /// não é SSH.
    fn duplicar_rodando(&self, _origem: Session, _comando: Option<&str>) -> Option<Session> {
        None
    }

    /// Diz onde está o agente embutido: as conexões SSH passam a usá-lo (e o shell local o
    /// recebe no `SSH_AUTH_SOCK`). `None` volta ao agente do sistema.
    fn definir_agente(&self, _caminho: Option<PathBuf>) {}

    /// Prepara a instalação da chave pública no servidor da conexão (o `ssh-copy-id`). Quem
    /// chama roda o [`Instalador`] numa thread, porque ele bloqueia até o fim.
    fn instalador(&self, _conexao: &Connection, _senhas: Senhas) -> Instalador {
        Box::new(|_, _| Err(t!("abrir-nao-instala-chaves")))
    }

    /// Prepara a leitura da lista de distribuições do WSL (`wsl.exe -l -q`). Quem chama roda o
    /// [`ListarWsl`] numa thread, porque ele bloqueia até o fim (M42). Sem WSL, a lista é vazia.
    fn listar_wsl(&self) -> ListarWsl {
        Box::new(|| Ok(Vec::new()))
    }

    /// Prepara a execução de um comando no servidor da conexão (o canal `exec` do SSH, sem
    /// aba). Quem chama roda o [`Executor`] numa thread, porque ele bloqueia até o fim (M42).
    fn executor(&self, _conexao: &Connection, _senhas: Senhas) -> Executor {
        Box::new(|_, _| Err(t!("abrir-nao-roda-comandos")))
    }

    /// Quem abre os arquivos da conexão (SFTP em conexão própria): cada chamada do resultado
    /// abre uma conexão nova, e a página Arquivos chama de várias threads.
    fn arquivos(&self, _conexao: &Connection, _senhas: Senhas) -> Result<ArquivosOpener, String> {
        Err(t!("abrir-nao-abre-arquivos"))
    }

    /// As opções da conexão S3 com as chaves, para a página Arquivos abrir as ações que o
    /// `Fs` não tem (link pré-assinado, propriedades, classe, envios incompletos).
    fn s3_config(&self, conexao: &Connection, senhas: Senhas) -> Result<S3Config, String> {
        s3_config(conexao, senhas)
    }

    /// Os processos do rclone (M33). `None`: este abridor não conversa com o rclone.
    fn rcds(&self) -> Option<Arc<Rcds>> {
        None
    }

    /// Se a sessão aberta tem uma conexão SSH em que o SFTP pode abrir outro canal.
    fn sessao_tem_arquivos(&self, sessao: Session) -> bool {
        sessao.ssh_channels().is_some()
    }

    /// De onde o painel de portas lê o que escuta no servidor (M58). O padrão fala pelos canais
    /// SSH da sessão; os testes trocam por uma fonte falsa.
    fn fonte_de_portas(
        &self,
        abridor: crate::portas::descobrir::AbridorCompartilhado,
        cfg: &crate::configuracoes::PortasCfg,
    ) -> Box<dyn crate::portas::descobrir::FontePortas> {
        Box::new(crate::portas::descobrir::FonteSsh::new(
            Arc::new(crate::portas::descobrir::CanaisDaSessao(abridor)),
            cfg.clone(),
        ))
    }

    /// Quem confere se o servidor da sessão alcança `host:porta` (o "Encaminhar destino…" do
    /// painel de portas, M58): abre e fecha um canal `direct-tcpip`. Roda numa thread.
    fn testador_de_destino(&self, sessao: Session) -> Result<TestadorDeDestino, String> {
        let canais = sessao.ssh_channels().ok_or_else(|| t!("portas-sem-ssh"))?;
        let canais = Mutex::new(canais);
        Ok(Arc::new(move |host, porta, limite| {
            let canais = canais.lock().unwrap_or_else(PoisonError::into_inner);
            let canal = canais
                .open_direct(host, porta, limite)
                .map_err(|erro| erro.to_string())?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .map_err(|erro| erro.to_string())?;
            runtime.block_on(crate::portas::descobrir::soltar_canal(canal));
            Ok(())
        }))
    }

    /// Quem abre os arquivos pela conexão SSH de uma sessão aberta (sem autenticar de novo).
    fn arquivos_da_sessao(&self, sessao: Session) -> Result<ArquivosOpener, String> {
        let canais = sessao
            .ssh_channels()
            .ok_or_else(|| t!("abrir-sessao-sem-ssh"))?;
        Ok(Arc::new(move || {
            Ok(Box::new(Sftp::from_channel(&canais)?) as Box<dyn Fs>)
        }))
    }

    /// Roda um comando pela conexão SSH de uma sessão aberta, com `entrada` na entrada padrão
    /// (o plano B do colar, quando o servidor não tem SFTP, e o fluxo `tar`; M45). Quem chama
    /// roda o resultado numa thread, porque ele bloqueia até o fim. `enviado` recebe o total de
    /// bytes de entrada já entregues, e um erro de `entrada` fecha o canal sem o fim dela.
    fn exec_da_sessao(&self, sessao: Session) -> Result<ExecDaSessao, String> {
        let canais = sessao
            .ssh_channels()
            .ok_or_else(|| t!("abrir-sessao-sem-ssh"))?;
        Ok(Arc::new(move |comando, entrada, enviado| {
            canais
                .exec(comando, entrada, enviado, Duration::from_secs(30))
                .map_err(|erro| erro.to_string())
        }))
    }

    /// Abre um arquivo local no `editor` (vazio: o programa do sistema). Os testes só anotam.
    fn abrir_externo(&self, arquivo: &Path, editor: &str) -> Result<(), String> {
        crate::ferramentas::abrir_no_editor(arquivo, editor).map_err(|erro| {
            t!(
                "chaves-nao-consegui-abrir",
                caminho = arquivo.display().to_string(),
                motivo = erro.to_string()
            )
        })
    }

    /// Abre o destino de um link do terminal neste computador (M49): o endereço no navegador
    /// (ou no programa de e-mail) e a pasta no gerenciador de arquivos. O painel de arquivos
    /// da conexão é da tela, não passa por aqui; `SoCopiar` não abre nada. Os testes só anotam.
    fn abrir_link(&self, destino: &DestinoDoLink) -> std::io::Result<()> {
        match destino {
            DestinoDoLink::Navegador(uri) => crate::ferramentas::abrir_endereco(uri),
            DestinoDoLink::PastaLocal(caminho) => {
                crate::ferramentas::abrir_pasta(&crate::links::pasta_do_link(caminho))
            }
            // Quem encaminha a porta é a tela; o que chega aqui já é o endereço daqui.
            DestinoDoLink::PainelRemoto(_)
            | DestinoDoLink::PortaDoServidor { .. }
            | DestinoDoLink::SoCopiar => Ok(()),
        }
    }

    /// Abre `url` (uma conexão Link com perfil de rede) no navegador só do perfil, que sai pela
    /// `rede` do cliente ([`crate::navegador`]). Volta na hora; a mensagem diz o que faltou. Os
    /// testes só anotam.
    fn abrir_link_pela_rede(&self, _url: &str, _rede: RedeResolvida) -> Result<(), String> {
        Err(t!("navegador-nenhum"))
    }

    /// Roda `comando` no servidor da conexão sem aba, com o X11 ligado (o virt-manager e os
    /// programas gráficos). Volta na hora; o [`Lancamento`] mostra em que pé está.
    fn lancar(&self, _conexao: &Connection, _senhas: Senhas, _comando: &str) -> Lancamento {
        Lancamento::controlado(RemoteAppStatus::Failed(t!("abrir-nao-lanca-programas")))
    }

    /// Roda `comando` no servidor da conexão numa aba (a saída aparece no terminal), com o X11
    /// ligado como nas outras abas da conexão. Fora do SSH, é o mesmo que [`Opener::open`].
    fn abrir_comando(
        &self,
        conexao: &Connection,
        senhas: Senhas,
        n: u32,
        _comando: &str,
    ) -> Session {
        self.open(conexao, senhas, n)
    }

    /// Abre o `comando` do pedido numa aba da conexão (M46): como [`Opener::abrir_comando`], mas
    /// com um título próprio para a aba e túneis a mais (o `-L 1455` do login do Codex). Fora
    /// do SSH, é o mesmo que [`Opener::open`].
    fn open_comando(
        &self,
        conexao: &Connection,
        senhas: Senhas,
        n: u32,
        pedido: PedidoComando,
    ) -> Session {
        self.abrir_comando(conexao, senhas, n, &pedido.comando)
    }

    /// Se o virt-manager existe nesta máquina (para o item "Abrir virt-manager local").
    fn virt_manager_local(&self) -> bool {
        false
    }

    /// Abre o virt-manager desta máquina ligado ao libvirt do servidor da conexão, por um
    /// túnel do socket Unix. Volta na hora; o [`Lancamento`] mostra em que pé está.
    fn abrir_virt_local(&self, _conexao: &Connection, _senhas: Senhas) -> Lancamento {
        Lancamento::controlado(RemoteAppStatus::Failed(t!("abrir-nao-abre-virt-local")))
    }

    /// O disco desta máquina, o painel da esquerda.
    fn disco(&self) -> Disco {
        Disco {
            opener: Arc::new(|| Ok(Box::new(Local::new()) as Box<dyn Fs>)),
            endpoint: Endpoint::Local,
        }
    }
}

/// De onde vêm os arquivos do painel da esquerda: quem abre o `Fs` e o `Endpoint` que a fila
/// de transferências usa para chegar nele.
#[derive(Clone)]
pub struct Disco {
    /// Abre o `Fs` do painel.
    pub opener: ArquivosOpener,
    /// A ponta da fila: `Endpoint::Local` no disco de verdade.
    pub endpoint: Endpoint,
}

/// Instala a chave pública (e `true` força, mesmo que ela já esteja lá). Bloqueia.
pub type Instalador =
    Box<dyn FnOnce(PublicKey, bool) -> Result<CopyIdOutcome, String> + Send + 'static>;

/// Lê as distribuições instaladas do WSL. Bloqueia. Sem WSL, devolve a lista vazia.
pub type ListarWsl = Box<dyn FnOnce() -> Result<Vec<String>, String> + Send + 'static>;

/// Roda um comando na conexão, com `entrada` no stdin, e devolve a saída. Bloqueia.
pub type Executor = Box<dyn FnOnce(&str, &[u8]) -> Result<ExecOutput, String> + Send + 'static>;

/// Roda um comando na conexão de uma sessão aberta, lendo a entrada padrão de `entrada`.
/// Bloqueia; a thread de quem chama é a que lê.
pub type ExecDaSessao = Arc<
    dyn Fn(&str, &mut dyn Read, &mut dyn FnMut(u64)) -> Result<ExecOutput, String> + Send + Sync,
>;

/// Se a tela precisa pedir a senha antes de abrir: SSH sem chave privada, sem chave do cofre e
/// com a credencial em "Perguntar ao conectar" (os modos do agente não perguntam antes).
pub fn precisa_de_senha(conexao: &Connection) -> bool {
    match conexao.kind {
        // As chaves do S3 sempre vêm do cofre (ou do diálogo): não há chave privada nem agente.
        Kind::S3 => true,
        // O rclone guarda as credenciais na configuração dele: não há senha para pedir.
        Kind::Rclone => false,
        // O FTP anônimo (usuário vazio ou `anonymous`) não tem senha para pedir.
        Kind::Ftp => !usuario_anonimo(&conexao.user),
        _ => {
            conexao.kind.usa_ssh()
                && conexao.key_file.is_none()
                && conexao.chave_do_cofre.is_none()
                && conexao.modo_da_credencial() == ModoCredencial::Perguntar
        }
    }
}

/// Se uma recusa de autenticação volta ao diálogo de senha (em vez de só mostrar o erro): quem
/// pede a senha antes de abrir e o modo "Agente SSH, senão a senha".
pub fn pede_senha_na_recusa(conexao: &Connection) -> bool {
    precisa_de_senha(conexao) || conexao.modo_da_credencial() == ModoCredencial::AgenteOuSenha
}

/// Se o usuário do FTP é o anônimo.
fn usuario_anonimo(usuario: &str) -> bool {
    let usuario = usuario.trim();
    usuario.is_empty() || usuario.eq_ignore_ascii_case("anonymous") || usuario == "ftp"
}

/// A chave da sessão da `n`-ésima aba da conexão.
pub fn chave_da_sessao(conexao: &Connection, n: u32) -> String {
    format!("{}-{n}", conexao.id)
}

/// A pergunta da chave do servidor: acrescenta o pedido à fila, acorda a tela e espera a
/// resposta. `Known` passa direto. Sem resposta (a tela fechou, ou passou o tempo) recusa.
pub fn perguntar(
    fila: FilaChaves,
    conexao: &str,
    host_porta: &str,
    algoritmo: &str,
    fingerprint: &str,
    conhecida: KnownHost,
) -> bool {
    if conhecida == KnownHost::Known {
        return true;
    }
    let (resposta, espera) = mpsc::channel();
    let pedido = Arc::new(PedidoChave {
        conexao: conexao.to_owned(),
        host_porta: host_porta.to_owned(),
        algoritmo: algoritmo.to_owned(),
        fingerprint: fingerprint.to_owned(),
        mudou: matches!(conhecida, KnownHost::Changed { .. }),
        certificado: None,
        resposta,
    });
    mexer_na_fila(fila, |fila| fila.push(pedido));
    tomni_ui::wake();
    espera.recv_timeout(ESPERA_DA_CHAVE).unwrap_or(false)
}

/// A decisão do `check_host_key`: pergunta à tela e, se a pessoa confiou numa chave nova,
/// grava no `known_hosts`. Com `aceitar_nova` (`conexoes.chave_nova`), o servidor desconhecido
/// entra sem a pergunta. Uma chave mudada nunca é aceita.
fn conferir(
    fila: FilaChaves,
    conexao: &str,
    chave: &HostKey,
    conhecida: KnownHost,
    aceitar_nova: bool,
) -> bool {
    if aceitar_nova && conhecida == KnownHost::Unknown {
        return chave.remember().is_ok();
    }
    let host_porta = format!("{}:{}", chave.host(), chave.port());
    let aceita = perguntar(
        fila,
        conexao,
        &host_porta,
        &chave.algorithm(),
        &chave.fingerprint(),
        conhecida,
    );
    match conhecida {
        KnownHost::Known => true,
        KnownHost::Unknown => aceita && chave.remember().is_ok(),
        KnownHost::Changed { .. } => false,
    }
}

/// Os certificados que a pessoa já confiou: `host:porta` → impressão SHA-256.
type Certificados = std::collections::BTreeMap<String, String>;

fn ler_certificados(arquivo: &Path) -> Certificados {
    std::fs::read_to_string(arquivo)
        .ok()
        .and_then(|texto| serde_json::from_str(&texto).ok())
        .unwrap_or_default()
}

/// Se o certificado de `host_porta` com esta impressão já foi confiado.
pub fn certificado_confiado(arquivo: &Path, host_porta: &str, sha256: &str) -> bool {
    ler_certificados(arquivo)
        .get(host_porta)
        .is_some_and(|confiado| confiado == sha256)
}

/// Grava a confiança no certificado (substitui o anterior do mesmo servidor), de forma atômica.
/// Numa pasta só de leitura ([`crate::armazenamento::so_leitura`]), não grava: vale só para esta
/// conexão.
pub fn confiar_no_certificado(
    arquivo: &Path,
    host_porta: &str,
    sha256: &str,
) -> Result<(), String> {
    if crate::armazenamento::so_leitura(arquivo) {
        return Ok(());
    }
    let mut todos = ler_certificados(arquivo);
    todos.insert(host_porta.to_owned(), sha256.to_owned());
    if let Some(pasta) = arquivo.parent() {
        std::fs::create_dir_all(pasta).map_err(|erro| erro.to_string())?;
    }
    let texto = serde_json::to_string_pretty(&todos).map_err(|erro| erro.to_string())?;
    crate::armazenamento::gravar_atomico(arquivo, texto.as_bytes(), false)
        .map_err(|erro| erro.to_string())
}

/// A data (UTC) no formato do idioma.
fn data_curta(quando: std::time::SystemTime) -> String {
    let segundos = quando
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    crate::recentes::data(segundos)
}

/// A pergunta do certificado: confiado antes passa direto; senão pergunta à tela e, se a
/// pessoa confiou, grava.
pub fn conferir_certificado(
    fila: FilaChaves,
    conexao: &str,
    porta: u16,
    arquivo: Option<&Path>,
    info: &CertificateInfo,
) -> bool {
    let host_porta = format!("{}:{porta}", info.host);
    if let Some(arquivo) = arquivo
        && certificado_confiado(arquivo, &host_porta, &info.sha256)
    {
        return true;
    }
    let (resposta, espera) = mpsc::channel();
    let pedido = Arc::new(PedidoChave {
        conexao: conexao.to_owned(),
        host_porta: host_porta.clone(),
        algoritmo: String::new(),
        fingerprint: impressao_em_duas_linhas(&info.sha256),
        mudou: false,
        certificado: Some(DadosCertificado {
            motivo: info.error.clone(),
            assunto: info.subject.clone(),
            emissor: info.issuer.clone(),
            validade: t!(
                "abrir-validade",
                de = data_curta(info.not_before),
                ate = data_curta(info.not_after)
            ),
        }),
        resposta,
    });
    mexer_na_fila(fila, |fila| fila.push(pedido));
    tomni_ui::wake();
    let aceita = espera.recv_timeout(ESPERA_DA_CHAVE).unwrap_or(false);
    if aceita && let Some(arquivo) = arquivo {
        let _ = confiar_no_certificado(arquivo, &host_porta, &info.sha256);
    }
    aceita
}

/// O `FtpBuilder` de uma conexão FTP. Sem usuário (ou `anonymous`) entra como anônimo.
pub fn ftp_builder(
    conexao: &Connection,
    senhas: Senhas,
    fila: FilaChaves,
    certificados: Option<PathBuf>,
) -> FtpBuilder {
    let nome = conexao.name.clone();
    let proxy = senhas.rede.as_ref().and_then(RedeResolvida::proxy);
    let porta = if conexao.port == 0 {
        conexao.ftp_security.porta_padrao()
    } else {
        conexao.port
    };
    let mut ftp = FtpBuilder::new(&conexao.host, porta)
        .security(match conexao.ftp_security {
            FtpSeguranca::Explicita => FtpSecurity::Explicit,
            FtpSeguranca::ExplicitaSeDisponivel => FtpSecurity::ExplicitIfAvailable,
            FtpSeguranca::Implicita => FtpSecurity::Implicit,
            FtpSeguranca::Nenhuma => FtpSecurity::None,
        })
        .mode(match conexao.ftp_mode {
            FtpModo::Passivo => FtpMode::Passive,
            FtpModo::Ativo => FtpMode::Active,
        })
        .tls_version(match conexao.ftp_tls {
            FtpTls::Auto => TlsVersion::Auto,
            FtpTls::Tls12 => TlsVersion::Only12,
            FtpTls::Tls13 => TlsVersion::Only13,
        })
        .encoding(match conexao.ftp_encoding {
            FtpCodificacao::Auto => FtpEncoding::Auto,
            FtpCodificacao::Utf8 => FtpEncoding::Utf8,
            FtpCodificacao::Latin1 => FtpEncoding::Latin1,
        })
        .timezone_offset(conexao.ftp_timezone_min)
        .check_certificate(move |info| {
            conferir_certificado(fila, &nome, porta, certificados.as_deref(), info)
        });
    if let Some(proxy) = proxy {
        // Pelo proxy só o modo passivo funciona: o ativo pede que o servidor conecte de volta.
        ftp = ftp.mode(FtpMode::Passive).proxy(proxy);
    }
    if !usuario_anonimo(&conexao.user) {
        ftp = ftp.login(conexao.user.trim(), senhas.conexao.unwrap_or_default());
    }
    if let Some(faixa) = faixa_de_portas(&conexao.ftp_active_ports) {
        ftp = ftp.active_ports(faixa);
    }
    ftp
}

/// A [`S3Config`] de uma conexão S3: `conexao.user` é o Access Key ID, `senhas.conexao` a chave
/// secreta e `senhas.token` o token de sessão. Um endereço vazio é erro.
pub fn s3_config(conexao: &Connection, senhas: Senhas) -> Result<S3Config, String> {
    s3_config_com(conexao, senhas, &S3Cfg::default())
}

/// Como [`s3_config`], com o tamanho da parte do envio de `s3.parte_mib`.
pub fn s3_config_com(
    conexao: &Connection,
    senhas: Senhas,
    padroes: &S3Cfg,
) -> Result<S3Config, String> {
    let opcoes = &conexao.s3;
    let endereco = opcoes.endpoint.trim();
    if endereco.is_empty() {
        return Err(t!("abrir-s3-sem-endereco"));
    }
    let mut config = S3Config::new(
        endereco,
        conexao.user.trim(),
        senhas.conexao.unwrap_or_default(),
    );
    let regiao = opcoes.region.trim();
    if !regiao.is_empty() {
        regiao.clone_into(&mut config.region);
    }
    let bucket = opcoes.bucket.trim();
    config.bucket = (!bucket.is_empty()).then(|| bucket.to_owned());
    config.url_style = match opcoes.url_style {
        S3Estilo::Caminho => S3UrlStyle::Path,
        S3Estilo::Virtual => S3UrlStyle::VirtualHost,
    };
    config.part_size = u64::from(padroes.parte_mib.clamp(5, 512)) << 20;
    config.session_token = senhas.token.filter(|t| !t.is_empty());
    config.proxy = senhas.rede.as_ref().and_then(RedeResolvida::proxy);
    let classe = opcoes.storage_class.trim();
    config.storage_class = (!classe.is_empty()).then(|| classe.to_owned());
    config.tls = match (opcoes.certificate, &opcoes.ca_file) {
        (S3Certificado::NaoVerificar, _) => TlsMode::NoVerify,
        (S3Certificado::CaPropria, Some(arquivo)) => TlsMode::CustomCa(arquivo.clone()),
        (S3Certificado::CaPropria, None) => {
            return Err(t!("abrir-s3-sem-ca"));
        }
        (S3Certificado::Verificar, _) => TlsMode::Verify,
    };
    Ok(config)
}

/// Lê `10000-10100` (ou uma porta só) como faixa; texto inválido vale como "qualquer porta".
pub fn faixa_de_portas(texto: &str) -> Option<std::ops::RangeInclusive<u16>> {
    let texto = texto.trim();
    let (de, ate) = texto.split_once('-').unwrap_or((texto, texto));
    let de: u16 = de.trim().parse().ok()?;
    let ate: u16 = ate.trim().parse().ok()?;
    (de > 0 && de <= ate).then_some(de..=ate)
}

/// Se o motivo de uma sessão fechada é a recusa de autenticação. O texto vem do app, no idioma
/// de quando a sessão caiu, então vale o dos dois idiomas.
pub fn e_recusa_de_autenticacao(motivo: &str) -> bool {
    motivo.contains("autenticação recusada") || motivo.contains("authentication refused")
}

/// O usuário a usar quando a conexão não tem um: o do sistema.
fn usuario_efetivo(conexao: &Connection) -> String {
    if !conexao.user.is_empty() {
        return conexao.user.clone();
    }
    ["USER", "USERNAME"]
        .iter()
        .find_map(|nome| std::env::var(nome).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| "root".to_owned())
}

/// O que as configurações mudam no builder SSH.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AjustesSsh {
    /// Aceitar e gravar o servidor desconhecido sem perguntar (`conexoes.chave_nova`).
    pub aceitar_chave_nova: bool,
    /// Encaminhar o agente (`agente.encaminhar_padrao`).
    pub encaminhar_agente: bool,
    /// O X11 das conexões com "Padrão" (`x11.padrao`).
    pub x11_padrao: X11Padrao,
    /// O servidor X daqui (`x11.display`); vazio é o `DISPLAY` do ambiente.
    pub x11_display: String,
}

impl AjustesSsh {
    /// Os ajustes que as configurações pedem.
    pub fn de(config: &Configuracoes) -> Self {
        Self {
            aceitar_chave_nova: config.conexoes.chave_nova == ChaveNova::Aceitar,
            encaminhar_agente: config.agente.encaminhar_padrao,
            x11_padrao: config.x11.padrao,
            x11_display: config.x11.display.trim().to_owned(),
        }
    }
}

/// O X11 e os túneis que uma conexão pede ao abrir (M37).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Encaminhamento {
    /// O modo do X11 e o display de `x11.display` (`None` usa o `DISPLAY`); `None` sem X11.
    pub x11: Option<(X11Mode, Option<String>)>,
    /// Os túneis ligados.
    pub tuneis: Vec<Forward>,
    /// Sem shell: a conexão só segura os túneis.
    pub so_tuneis: bool,
    /// Desconectar se um túnel não subir.
    pub parar_se_falhar: bool,
}

/// A pasta dos sockets Unix automáticos: `$XDG_RUNTIME_DIR/tirech-omni` ou, sem ela, a pasta
/// temporária.
pub fn pasta_dos_sockets() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("tirech-omni")
}

/// O túnel do app como o `Forward` da lib. `n` é a posição dele na lista da conexão (entra no
/// nome do socket automático).
pub fn forward_do_tunel(conexao: u64, n: usize, tunel: &Tunel) -> Forward {
    let forward = match tunel.tipo {
        TipoTunel::Local => {
            let porta = if tunel.proxima_livre {
                escolher_porta_local(tunel.porta, porta_livre).unwrap_or(tunel.porta)
            } else {
                tunel.porta
            };
            Forward::local(porta, &tunel.destino_host, tunel.destino_porta)
        }
        TipoTunel::Remoto => Forward::remote(tunel.porta, &tunel.destino_host, tunel.destino_porta),
        TipoTunel::Dinamico => Forward::dynamic(tunel.porta),
        TipoTunel::SocketUnix => {
            let socket = tunel.socket_local.trim();
            let local = if !socket.is_empty() && !cfg!(windows) {
                LocalSocket::Path(PathBuf::from(socket))
            } else if tunel.porta > 0 || cfg!(windows) {
                LocalSocket::Tcp(Bind::loopback(tunel.porta))
            } else {
                let desejado = pasta_dos_sockets().join(format!("sock-{conexao}-{n}.sock"));
                LocalSocket::Path(crate::agente::endereco_que_cabe_aqui(desejado))
            };
            Forward::unix(local, tunel.caminho_remoto.trim())
        }
    };
    if tunel.outras_maquinas {
        forward.open_to_network()
    } else {
        forward
    }
}

/// O que a conexão pede de X11 e de túneis. Só o SSH pede; as outras conexões não pedem nada.
pub fn encaminhamento(conexao: &Connection, ajustes: &AjustesSsh) -> Encaminhamento {
    if conexao.kind != Kind::Ssh {
        return Encaminhamento::default();
    }
    let modo = match conexao.x11 {
        ModoX11::Padrao => match ajustes.x11_padrao {
            X11Padrao::Desligado => None,
            X11Padrao::NaoConfiavel => Some(X11Mode::Untrusted),
            X11Padrao::Confiavel => Some(X11Mode::Trusted),
        },
        ModoX11::Desligado => None,
        ModoX11::NaoConfiavel => Some(X11Mode::Untrusted),
        ModoX11::Confiavel => Some(X11Mode::Trusted),
    };
    let display = Some(ajustes.x11_display.clone()).filter(|d| !d.is_empty());
    Encaminhamento {
        x11: modo.map(|modo| (modo, display)),
        tuneis: conexao
            .tuneis
            .iter()
            .enumerate()
            .filter(|(_, t)| t.ligado && t.problema().is_none() && !t.usa_rele())
            .map(|(n, t)| forward_do_tunel(conexao.id, n, t))
            .collect(),
        so_tuneis: conexao.so_tuneis,
        parar_se_falhar: conexao.parar_se_tunel_falhar,
    }
}

/// Os túneis ligados da conexão que passam pelo relé do app (nome do site ou HTTPS, M58): a casca
/// os sobe quando a sessão abre, porque o `builder` do SSH não os conhece.
pub fn tuneis_do_rele(conexao: &Connection) -> Vec<Tunel> {
    if conexao.kind != Kind::Ssh {
        return Vec::new();
    }
    conexao
        .tuneis
        .iter()
        .filter(|t| t.ligado && t.problema().is_none() && t.usa_rele())
        .cloned()
        .collect()
}

/// Como [`encaminhamento`], para um programa gráfico: o X11 sempre vale (o item existe para
/// mostrar janelas; um modo `Desligado` vira o confiável) e nunca é "só túneis".
pub fn encaminhamento_de_programa(conexao: &Connection, ajustes: &AjustesSsh) -> Encaminhamento {
    let mut enc = encaminhamento(conexao, ajustes);
    if enc.x11.is_none() {
        let display = Some(ajustes.x11_display.clone()).filter(|d| !d.is_empty());
        enc.x11 = Some((X11Mode::Trusted, display));
    }
    enc.so_tuneis = false;
    enc
}

/// Põe o [`Encaminhamento`] no builder do destino.
pub fn aplicar_encaminhamento(mut ssh: SshBuilder, enc: &Encaminhamento) -> SshBuilder {
    if let Some((modo, display)) = &enc.x11 {
        let mut x11 = match modo {
            X11Mode::Trusted => X11::trusted(),
            X11Mode::Untrusted => X11::untrusted(),
        };
        if let Some(display) = display {
            x11 = x11.display(display.clone());
        }
        ssh = ssh.x11(x11);
    }
    for forward in &enc.tuneis {
        if let Forward::Unix {
            local: LocalSocket::Path(caminho),
            ..
        } = forward
            && let Some(pai) = caminho.parent()
        {
            let _ = std::fs::create_dir_all(pai);
        }
        ssh = ssh.forward(forward.clone());
    }
    ssh.exit_on_forward_failure(enc.parar_se_falhar)
        .no_shell(enc.so_tuneis)
}

/// Um servidor de salto: usa o agente, a mesma chave privada e a senha dele, se houver.
// Cada argumento vem de um lugar diferente de `ssh_builder`; agrupá-los num tipo só para esta
// função não deixaria a chamada mais clara.
#[allow(clippy::too_many_arguments)]
fn builder_do_salto(
    salto: &Jump,
    senha: Option<String>,
    usuario_padrao: String,
    senhas_chave: Option<&str>,
    chave: Option<&PathBuf>,
    chave_cofre: Option<&ssh_key::PrivateKey>,
    fila: FilaChaves,
    known_hosts: Option<&PathBuf>,
    agente: Option<&Path>,
    aceitar_chave_nova: bool,
) -> SshBuilder {
    let nome_do_salto = t!(
        "abrir-nome-do-salto",
        host = salto.host.as_str(),
        porta = i64::from(salto.port)
    );
    let usuario = if salto.user.is_empty() {
        usuario_padrao
    } else {
        salto.user.clone()
    };
    let mut via = Ssh::builder(&salto.host, salto.port, usuario)
        .agent_auth()
        .check_host_key(move |chave, conhecida| {
            conferir(fila, &nome_do_salto, chave, conhecida, aceitar_chave_nova)
        });
    if let Some(agente) = agente {
        via = via.agent_path(agente);
    }
    if let Some(chave) = chave {
        via = via.key_file(chave, senhas_chave);
    }
    if let Some(chave) = chave_cofre {
        via = via.key(chave.clone());
    }
    if let Some(senha) = senha {
        via = via.password(senha);
    }
    if let Some(known_hosts) = known_hosts {
        via = via.known_hosts(known_hosts);
    }
    via
}

/// O `SshBuilder` de uma conexão SSH, com o salto, a chave, o keepalive e a confirmação da
/// chave do servidor. Com a rede de um perfil nas `senhas` (M34), a cadeia vira perfil → salto
/// da conexão → destino, o proxy do perfil leva ao primeiro deles, o `known_hosts` é o do
/// perfil e o `before_connect` sobe a rede.
pub fn ssh_builder(
    conexao: &Connection,
    senhas: Senhas,
    fila: FilaChaves,
    known_hosts: Option<&PathBuf>,
    agente: Option<&Path>,
) -> SshBuilder {
    ssh_builder_com(
        conexao,
        senhas,
        fila,
        known_hosts,
        agente,
        AjustesSsh::default(),
    )
}

/// Como [`ssh_builder`], com o que as configurações mudam ([`AjustesSsh`]).
pub fn ssh_builder_com(
    conexao: &Connection,
    senhas: Senhas,
    fila: FilaChaves,
    known_hosts: Option<&PathBuf>,
    agente: Option<&Path>,
    ajustes: AjustesSsh,
) -> SshBuilder {
    let rede = senhas.rede.clone();
    let known_hosts = rede
        .as_ref()
        .and_then(|r| r.known_hosts.as_ref())
        .or(known_hosts);
    if let Some(pai) = known_hosts.and_then(|k| k.parent()) {
        let _ = std::fs::create_dir_all(pai);
    }
    let usuario = usuario_efetivo(conexao);
    let nome = conexao.name.clone();
    let aceitar_chave_nova = ajustes.aceitar_chave_nova;
    let mut ssh = Ssh::builder(&conexao.host, conexao.port, usuario)
        .agent_auth()
        .term(&conexao.term)
        .check_host_key(move |chave, conhecida| {
            conferir(fila, &nome, chave, conhecida, aceitar_chave_nova)
        });
    if let Some(agente) = agente {
        ssh = ssh.agent_path(agente);
    }
    if ajustes.encaminhar_agente {
        ssh = ssh.agent_forward(true);
    }
    if let Some(chave) = &conexao.key_file {
        ssh = ssh.key_file(chave, senhas.chave.as_deref());
    }
    if let Some(chave) = &senhas.chave_cofre {
        ssh = ssh.key(chave.clone());
    }
    if let Some(senha) = senhas.conexao {
        ssh = ssh.password(senha);
    }
    if conexao.keepalive_secs > 0 {
        ssh = ssh.keepalive(Duration::from_secs(u64::from(conexao.keepalive_secs)));
    }
    if conexao.comprimir {
        ssh = ssh.compression(true);
    }
    if let Some(known_hosts) = known_hosts {
        ssh = ssh.known_hosts(known_hosts);
    }
    let salto = |salto: &Jump, senha: Option<String>| {
        builder_do_salto(
            salto,
            senha,
            usuario_efetivo(conexao),
            senhas.chave.as_deref(),
            conexao.key_file.as_ref(),
            senhas.chave_cofre.as_ref(),
            fila,
            known_hosts,
            agente,
            aceitar_chave_nova,
        )
    };
    // O mais fundo da cadeia: o salto do perfil, se houver.
    let do_perfil = rede.as_ref().and_then(|r| {
        r.perfil
            .salto
            .as_ref()
            .map(|j| salto(j, r.salto_senha.clone()))
    });
    match (&conexao.jump, do_perfil) {
        (Some(j), fundo) => {
            let mut via = salto(j, senhas.salto);
            if let Some(fundo) = fundo {
                via = via.jump(fundo);
            }
            ssh = ssh.jump(via);
        }
        (None, Some(fundo)) => ssh = ssh.jump(fundo),
        (None, None) => {}
    }
    if let Some(rede) = &rede {
        if let Some(proxy) = rede.proxy() {
            ssh = ssh.proxy(proxy);
        }
        if rede.perfil.proxy.is_some() {
            ssh = ssh.before_connect(rede.gancho());
        }
    }
    ssh
}

/// Faz o `opener` das conexões de arquivos preparar a rede do perfil antes de abrir (na thread
/// do `Remote`, que é onde o `opener` roda). Sem proxy não há o que preparar.
pub(crate) fn com_rede_pronta(
    opener: ArquivosOpener,
    rede: Option<&RedeResolvida>,
) -> ArquivosOpener {
    let Some(rede) = rede.filter(|r| r.perfil.proxy.is_some()) else {
        return opener;
    };
    let rede = rede.clone();
    Arc::new(move || {
        rede.preparar(&mut |_| {}).map_err(std::io::Error::other)?;
        opener()
    })
}

/// A sessão Telnet da conexão: pelo proxy do perfil, se houver. O salto do perfil não vale aqui.
fn telnet(conexao: &Connection, senhas: &Senhas) -> Telnet {
    let mut telnet = Telnet::builder(&conexao.host, conexao.port);
    if let Some(rede) = senhas.rede.as_ref().filter(|r| r.perfil.proxy.is_some()) {
        if let Some(proxy) = rede.proxy() {
            telnet = telnet.proxy(proxy);
        }
        telnet = telnet.before_connect(rede.gancho());
    }
    telnet.connect()
}

/// Abre as conexões de verdade.
pub struct RealOpener {
    fila: FilaChaves,
    known_hosts: Option<PathBuf>,
    agente: RefCell<Option<PathBuf>>,
    rcds: Arc<Rcds>,
    config: RefCell<Configuracoes>,
    pasta: RefCell<PathBuf>,
    /// Se o virt-manager está no `PATH`, e quando isso foi visto: o menu de cada conexão pergunta
    /// a cada quadro, e procurar no `PATH` toda vez custa uma ida ao disco por pasta.
    virt_manager: Cell<Option<(Instant, bool)>>,
    /// Os proxies locais dos links com perfil de rede, um por perfil.
    proxies: crate::navegador::Proxies,
}

impl RealOpener {
    /// `fila` é a que a tela lê; `known_hosts` `None` usa o `~/.ssh/known_hosts` do OpenSSH.
    pub fn new(fila: FilaChaves, known_hosts: Option<PathBuf>) -> Self {
        Self {
            fila,
            known_hosts,
            agente: RefCell::new(None),
            rcds: Rcds::real(OpcoesRclone::default()),
            config: RefCell::default(),
            pasta: RefCell::default(),
            virt_manager: Cell::new(None),
            proxies: crate::navegador::Proxies::default(),
        }
    }
}

impl Opener for RealOpener {
    fn open(&self, conexao: &Connection, senhas: Senhas, n: u32) -> Session {
        let sessao = match conexao.kind {
            // O SFTP não abre terminal: a tela leva a conexão à página Arquivos. Se mesmo assim
            // chegar aqui, abre o shell do servidor.
            Kind::Ssh | Kind::Sftp => {
                let enc = encaminhamento(conexao, &AjustesSsh::de(&self.config.borrow()));
                let mut builder = aplicar_encaminhamento(self.builder(conexao, senhas), &enc);
                if conexao.kind == Kind::Ssh
                    && let Some(sessao) = &conexao.sessao_persistente
                {
                    builder = builder.command(crate::tmux::comando_da_aba(sessao));
                }
                Session::new(builder.connect())
            }
            Kind::Local => {
                let config = self.config.borrow();
                let mut processo = processo_local(conexao, &config);
                for (nome, valor) in &conexao.local_ambiente {
                    processo = processo.env(nome, valor);
                }
                if config.agente.exportar_no_shell
                    && let Some(socket) = self.agente.borrow().as_ref()
                {
                    processo = processo.env("SSH_AUTH_SOCK", socket);
                }
                Session::new(processo.spawn())
            }
            Kind::Telnet => Session::new(telnet(conexao, &senhas)),
            // O FTP não abre terminal: a tela leva a conexão à página Arquivos. O link abre no
            // navegador, sem aba.
            Kind::Ftp | Kind::S3 | Kind::Rclone | Kind::Link => Session::new(Loopback::new()),
        };
        configura(
            sessao,
            conexao,
            n,
            &self.config.borrow(),
            &self.pasta.borrow(),
        )
    }

    fn configurar(&self, config: &Configuracoes, pasta: &Path) {
        *self.config.borrow_mut() = config.clone();
        pasta.clone_into(&mut self.pasta.borrow_mut());
    }

    fn duplicar_rodando(&self, origem: Session, comando: Option<&str>) -> Option<Session> {
        origem.duplicate_running(comando)
    }

    fn abrir_link_pela_rede(&self, url: &str, rede: RedeResolvida) -> Result<(), String> {
        let navegador = self.config.borrow().redes.navegador.clone();
        let perfil = rede.perfil.id;
        let pasta = self.pasta.borrow().clone();
        crate::navegador::abrir(&self.proxies, &pasta, &navegador, perfil, rede, url)
    }

    fn definir_agente(&self, caminho: Option<PathBuf>) {
        *self.agente.borrow_mut() = caminho;
    }

    fn rcds(&self) -> Option<Arc<Rcds>> {
        Some(Arc::clone(&self.rcds))
    }

    fn arquivos(&self, conexao: &Connection, senhas: Senhas) -> Result<ArquivosOpener, String> {
        let rede = senhas.rede.clone();
        if conexao.kind == Kind::Rclone {
            let proxy = rede.as_ref().and_then(RedeResolvida::url_do_proxy);
            let opener = rclone::abridor(Arc::clone(&self.rcds), conexao.rclone.spec(), proxy);
            return Ok(com_rede_pronta(opener, rede.as_ref()));
        }
        if conexao.kind == Kind::Ftp {
            let builder = ftp_builder(conexao, senhas, self.fila, self.arquivo_de_certificados());
            return Ok(com_rede_pronta(builder.opener(), rede.as_ref()));
        }
        if conexao.kind == Kind::S3 {
            return Ok(com_rede_pronta(
                s3_config_com(conexao, senhas, &self.config.borrow().s3)?.opener(),
                rede.as_ref(),
            ));
        }
        let builder = self.builder(conexao, senhas);
        Ok(Arc::new(move || {
            Ok(Box::new(Sftp::connect(&builder)?) as Box<dyn Fs>)
        }))
    }

    fn lancar(&self, conexao: &Connection, senhas: Senhas, comando: &str) -> Lancamento {
        let enc = encaminhamento_de_programa(conexao, &AjustesSsh::de(&self.config.borrow()));
        let builder = aplicar_encaminhamento(self.builder(conexao, senhas), &enc);
        Lancamento::remoto(builder.launch(comando))
    }

    fn abrir_comando(
        &self,
        conexao: &Connection,
        senhas: Senhas,
        n: u32,
        comando: &str,
    ) -> Session {
        if !conexao.kind.usa_ssh() {
            return self.open(conexao, senhas, n);
        }
        let enc = encaminhamento_de_programa(conexao, &AjustesSsh::de(&self.config.borrow()));
        let builder = aplicar_encaminhamento(self.builder(conexao, senhas), &enc).command(comando);
        configura(
            Session::new(builder.connect()),
            conexao,
            n,
            &self.config.borrow(),
            &self.pasta.borrow(),
        )
    }

    fn open_comando(
        &self,
        conexao: &Connection,
        senhas: Senhas,
        n: u32,
        pedido: PedidoComando,
    ) -> Session {
        if !conexao.kind.usa_ssh() {
            return self.open(conexao, senhas, n);
        }
        let mut enc = encaminhamento_de_programa(conexao, &AjustesSsh::de(&self.config.borrow()));
        for tunel in pedido.tuneis {
            if !enc.tuneis.contains(&tunel) {
                enc.tuneis.push(tunel);
            }
        }
        let builder =
            aplicar_encaminhamento(self.builder(conexao, senhas), &enc).command(pedido.comando);
        configura(
            Session::new(builder.connect()),
            conexao,
            n,
            &self.config.borrow(),
            &self.pasta.borrow(),
        )
        .name(pedido.titulo)
    }

    fn virt_manager_local(&self) -> bool {
        if cfg!(windows) {
            return false;
        }
        if let Some((quando, tem)) = self.virt_manager.get()
            && quando.elapsed() < VALIDADE_DA_PROCURA
        {
            return tem;
        }
        let tem = programa_no_path("virt-manager");
        self.virt_manager.set(Some((Instant::now(), tem)));
        tem
    }

    fn abrir_virt_local(&self, conexao: &Connection, senhas: Senhas) -> Lancamento {
        let lancamento = Lancamento::controlado(RemoteAppStatus::Connecting);
        let builder = self.builder(conexao, senhas);
        let (nome, id) = (conexao.name.clone(), conexao.id);
        let dele = lancamento.clone();
        let iniciada = std::thread::Builder::new()
            .name("tirech-omni-virt-local".to_owned())
            .spawn(move || {
                let fim = virt_local(builder, &nome, id, &dele);
                dele.definir_status(fim);
            });
        if let Err(erro) = iniciada {
            lancamento.definir_status(RemoteAppStatus::Failed(erro.to_string()));
        }
        lancamento
    }

    fn listar_wsl(&self) -> ListarWsl {
        Box::new(listar_wsl_do_sistema)
    }

    fn executor(&self, conexao: &Connection, senhas: Senhas) -> Executor {
        let builder = self.builder(conexao, senhas);
        Box::new(move |comando, entrada| builder.exec(comando, entrada))
    }

    fn instalador(&self, conexao: &Connection, senhas: Senhas) -> Instalador {
        let builder = self.builder(conexao, senhas);
        Box::new(move |chave, forcar| {
            copy_id(
                &builder,
                &chave,
                CopyIdOptions {
                    force: forcar,
                    dry_run: false,
                },
            )
        })
    }
}

impl RealOpener {
    /// O `certificados.json`, ao lado do `known_hosts` (ou em `~/.ssh`).
    fn arquivo_de_certificados(&self) -> Option<PathBuf> {
        let pasta = match &self.known_hosts {
            Some(known_hosts) => known_hosts.parent()?.to_path_buf(),
            None => {
                PathBuf::from(std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?)
                    .join(".ssh")
            }
        };
        Some(pasta.join("certificados.json"))
    }

    /// O builder SSH desta conexão, com o agente embutido se ele roda.
    fn builder(&self, conexao: &Connection, senhas: Senhas) -> SshBuilder {
        let config = self.config.borrow();
        let escolhido = known_hosts_escolhido(&config);
        ssh_builder_com(
            conexao,
            senhas,
            self.fila,
            escolhido.as_ref().or(self.known_hosts.as_ref()),
            self.agente.borrow().as_deref(),
            AjustesSsh::de(&config),
        )
    }
}

/// Se `programa` existe numa das pastas do `PATH`.
/// Por quanto tempo vale a procura do virt-manager no `PATH` (o que se instala com o app aberto
/// aparece depois disso).
const VALIDADE_DA_PROCURA: Duration = Duration::from_secs(10);

fn programa_no_path(programa: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|pasta| pasta.join(programa).is_file())
    })
}

/// O que o servidor roda para achar o socket do libvirt.
const DESCOBRIR_LIBVIRT: &str = "for s in /var/run/libvirt/libvirt-sock /var/run/libvirt/virtqemud-sock; do test -S \"$s\" && echo \"$s\" && break; done";

/// Quanto o túnel do libvirt tem para subir.
const ESPERA_DO_TUNEL: Duration = Duration::from_secs(30);

/// O virt-manager local: acha o socket do libvirt no servidor, sobe o túnel e roda o programa
/// até ele sair (ou até pedirem [`Lancamento::fechar`]). Devolve o estado final.
fn virt_local(
    builder: SshBuilder,
    nome: &str,
    id: u64,
    lancamento: &Lancamento,
) -> RemoteAppStatus {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let falha = |texto: String| RemoteAppStatus::Failed(texto);
    let saida = match builder.exec(DESCOBRIR_LIBVIRT, b"") {
        Ok(saida) => saida,
        Err(erro) => return falha(erro),
    };
    let remoto = saida.stdout_text().trim().to_owned();
    if remoto.is_empty() {
        return falha(t!("abrir-libvirt-nao-achado", nome = nome.to_string()));
    }
    let pasta = pasta_dos_sockets();
    let _ = std::fs::create_dir_all(&pasta);
    let local = pasta.join(format!("libvirt-{id}.sock"));
    let _ = std::fs::remove_file(&local);
    let mut ssh = builder
        .no_shell(true)
        .exit_on_forward_failure(true)
        .forward(Forward::unix(LocalSocket::Path(local.clone()), remoto))
        .connect();
    let Some(tuneis) = ssh.ssh_forwards() else {
        return falha(t!("abrir-conexao-sem-tuneis"));
    };
    // Lê o que a conexão escreve (avisos, o motivo de uma queda) e espera o túnel subir.
    let mut lido = Vec::new();
    let mut ler = |ssh: &mut Ssh| -> Status {
        lido.clear();
        let estado = ssh.read(&mut lido, 4096);
        lancamento.acrescentar_stderr(&String::from_utf8_lossy(&lido));
        estado
    };
    let inicio = std::time::Instant::now();
    loop {
        if lancamento.fechado() {
            return RemoteAppStatus::Exited(None);
        }
        if let Status::Closed(motivo) = ler(&mut ssh) {
            return falha(
                motivo.unwrap_or_else(|| t!("abrir-conexao-acabou", nome = nome.to_string())),
            );
        }
        match tuneis.list().first().map(|t| t.state.clone()) {
            Some(ForwardState::Listening) => break,
            Some(ForwardState::Failed(erro)) => return falha(erro),
            _ => {}
        }
        if inicio.elapsed() > ESPERA_DO_TUNEL {
            return falha(t!("abrir-tunel-libvirt-nao-subiu", nome = nome.to_string()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let mut filho = match Command::new("virt-manager")
        .arg("--no-fork")
        .arg("-c")
        .arg(format!("qemu+unix:///system?socket={}", local.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(filho) => filho,
        Err(erro) => return falha(t!("abrir-nao-consegui-abrir-virt", erro = erro.to_string())),
    };
    lancamento.definir_status(RemoteAppStatus::Running);
    if let Some(mut erro) = filho.stderr.take() {
        let dele = lancamento.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 1024];
            while let Ok(n) = erro.read(&mut buf) {
                if n == 0 {
                    break;
                }
                dele.acrescentar_stderr(&String::from_utf8_lossy(&buf[..n]));
            }
        });
    }
    let fim = loop {
        if lancamento.fechado() {
            let _ = filho.kill();
            let _ = filho.wait();
            break RemoteAppStatus::Exited(None);
        }
        match filho.try_wait() {
            Ok(Some(codigo)) => {
                break RemoteAppStatus::Exited(codigo.code().and_then(|c| u32::try_from(c).ok()));
            }
            Ok(None) => {}
            Err(erro) => break falha(erro.to_string()),
        }
        if let Status::Closed(motivo) = ler(&mut ssh) {
            let _ = filho.kill();
            let _ = filho.wait();
            break falha(
                motivo.unwrap_or_else(|| t!("abrir-conexao-acabou", nome = nome.to_string())),
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    drop(ssh);
    let _ = std::fs::remove_file(&local);
    fim
}

/// O programa do shell local: `terminal.shell.programa` com os argumentos, ou o shell da pessoa,
/// na `terminal.shell.pasta`.
pub fn shell_local(config: &Configuracoes) -> ProcessBuilder {
    processo_local(&Connection::new("", Kind::Local), config)
}

/// O programa e os argumentos que o terminal local da `conexao` roda: o da própria conexão
/// (`local_programa`), senão o das Configurações. `None` é o shell da pessoa.
pub fn programa_local(
    conexao: &Connection,
    config: &Configuracoes,
) -> Option<(String, Vec<String>)> {
    let programa = conexao.local_programa.trim();
    if !programa.is_empty() {
        return Some((
            programa.to_owned(),
            dividir_argumentos(&conexao.local_argumentos),
        ));
    }
    let programa = config.terminal.shell.programa.trim();
    (!programa.is_empty()).then(|| {
        (
            programa.to_owned(),
            dividir_argumentos(&config.terminal.shell.argumentos),
        )
    })
}

/// A pasta onde o terminal local da `conexao` começa: a da própria conexão (`local_pasta`),
/// senão a das Configurações, com o `~` expandido. `None` é a pasta pessoal.
pub fn pasta_local(conexao: &Connection, config: &Configuracoes) -> Option<PathBuf> {
    [&conexao.local_pasta, &config.terminal.shell.pasta]
        .into_iter()
        .map(|pasta| pasta.trim())
        .find(|pasta| !pasta.is_empty())
        .map(crate::armazenamento::expandir_til)
}

/// O processo do terminal local da `conexao`, pela escolha de [`programa_local`], na pasta de
/// [`pasta_local`]. Uma pasta que não existe (um disco que saiu, um nome errado) não impede o
/// terminal: ele abre na pasta pessoal.
pub fn processo_local(conexao: &Connection, config: &Configuracoes) -> ProcessBuilder {
    let processo = match programa_local(conexao, config) {
        Some((programa, argumentos)) if crate::pasta_da_aba::eh_wsl(&programa) => {
            // O bash do WSL não manda a pasta: o OSC 7 vem pelo PROMPT_COMMAND (M64).
            let wslenv = std::env::var("WSLENV").ok();
            crate::pasta_da_aba::ambiente_do_osc7_no_wsl(wslenv.as_deref())
                .into_iter()
                .fold(
                    Process::builder(programa).args(argumentos),
                    |processo, (nome, valor)| processo.env(nome, valor),
                )
        }
        Some((programa, argumentos)) => Process::builder(programa).args(argumentos),
        None => ProcessBuilder::shell(),
    };
    match pasta_local(conexao, config) {
        Some(pasta) if pasta.is_dir() => processo.cwd(pasta),
        _ => processo,
    }
}

/// Roda `wsl.exe -l -q` (só no Windows, sem abrir janela) e lê a lista. Sem WSL, ou fora do
/// Windows, a lista é vazia: não é erro.
fn listar_wsl_do_sistema() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        let mut comando = std::process::Command::new("wsl.exe");
        let saida = crate::sistema::sem_janela(&mut comando)
            .args(["-l", "-q"])
            .output();
        match saida {
            Ok(saida) if saida.status.success() => Ok(crate::wsl::ler_lista(&saida.stdout)),
            _ => Ok(Vec::new()),
        }
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

/// Junta `argumentos` numa linha que [`dividir_argumentos`] devolve igual: cada um entre aspas
/// simples, e a aspa simples de dentro vira `"'"` colada ao resto.
pub fn juntar_argumentos(argumentos: &[String]) -> String {
    argumentos
        .iter()
        .map(|a| format!("'{}'", a.replace('\'', "'\"'\"'")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Divide a linha de argumentos por espaços; aspas simples ou duplas juntam o que está dentro.
pub fn dividir_argumentos(linha: &str) -> Vec<String> {
    let mut argumentos = Vec::new();
    let mut atual = String::new();
    let mut tem_argumento = false;
    let mut aspas: Option<char> = None;
    for c in linha.chars() {
        match (aspas, c) {
            (Some(q), c) if c == q => aspas = None,
            (Some(_), c) => atual.push(c),
            (None, '"' | '\'') => {
                aspas = Some(c);
                tem_argumento = true;
            }
            (None, c) if c.is_whitespace() => {
                if tem_argumento {
                    argumentos.push(std::mem::take(&mut atual));
                    tem_argumento = false;
                }
            }
            (None, c) => {
                atual.push(c);
                tem_argumento = true;
            }
        }
    }
    if tem_argumento {
        argumentos.push(atual);
    }
    argumentos
}

/// O `known_hosts` de `conexoes.known_hosts`; `None` quando vazio (vale o do `RealOpener`).
fn known_hosts_escolhido(config: &Configuracoes) -> Option<PathBuf> {
    let caminho = config.conexoes.known_hosts.trim();
    (!caminho.is_empty()).then(|| PathBuf::from(caminho))
}

/// A pasta dos logs: `terminal.logs.pasta` ou `logs` na pasta de dados.
pub fn pasta_dos_logs(config: &Configuracoes, pasta_de_dados: &Path) -> PathBuf {
    let escolhida = config.terminal.logs.pasta.trim();
    if escolhida.is_empty() {
        pasta_de_dados.join("logs")
    } else {
        PathBuf::from(escolhida)
    }
}

/// O nome de uma conexão como entra no nome do arquivo de log: só letras, números, `-` e `_`
/// (o resto vira `-`, e as pontas sem `-`), ou `sessao` se não sobrar nada.
pub fn nome_limpo(nome: &str) -> String {
    let limpo: String = nome
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let limpo = limpo.trim_matches('-');
    if limpo.is_empty() {
        "sessao".to_owned()
    } else {
        limpo.to_owned()
    }
}

/// O nome do arquivo de log de uma sessão: o nome da conexão (só letras, números, `-` e `_`) e
/// a data e a hora da abertura, com `.cifrado` no fim quando o log é cifrado (M53).
pub fn nome_do_log(conexao: &Connection, n: u32, cifrado: bool) -> String {
    let nome = nome_limpo(&conexao.name);
    let quando = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S");
    let sufixo = if cifrado { SUFIXO_CIFRADO } else { "" };
    format!("{nome}-{quando}-{n}.log{sufixo}")
}

/// O que um log cifrado ganha no fim do nome.
pub const SUFIXO_CIFRADO: &str = ".cifrado";

/// A chave, o nome e o log de uma sessão recém-aberta: o `log_file` da conexão ou, com
/// `terminal.logs.todas_as_sessoes`, um arquivo novo na pasta dos logs.
fn configura(
    sessao: Session,
    conexao: &Connection,
    n: u32,
    config: &Configuracoes,
    pasta_de_dados: &Path,
) -> Session {
    let sessao = sessao
        .key(chave_da_sessao(conexao, n))
        .name(conexao.name.clone());
    let cifrar = config.terminal.logs.cifrar;
    let arquivo = match &conexao.log_file {
        Some(arquivo) if cifrar => Some(com_sufixo_cifrado(arquivo)),
        Some(arquivo) => Some(arquivo.clone()),
        None if config.terminal.logs.todas_as_sessoes => {
            let pasta = pasta_dos_logs(config, pasta_de_dados);
            Some(pasta.join(nome_do_log(conexao, n, cifrar)))
        }
        None => None,
    };
    if let Some(arquivo) = arquivo {
        // Dois escritores no mesmo arquivo cifrado misturariam os registros: o segundo vai para
        // `nome (2)`.
        let arquivo = if cifrar {
            crate::logs::caminho_livre(&arquivo)
        } else {
            arquivo
        };
        if let Some(pai) = arquivo.parent() {
            let _ = std::fs::create_dir_all(pai);
        }
        let aberto = if cifrar {
            abrir_log_cifrado(&sessao, &arquivo, pasta_de_dados)
        } else {
            sessao.log_to(&arquivo, LogMode::Text)
        };
        match aberto {
            Ok(()) => crate::logs::registrar_ativo(&chave_da_sessao(conexao, n), &arquivo),
            Err(erro) => sessao.feed(
                format!(
                    "{}\r\n",
                    t!(
                        "abrir-log-falhou",
                        caminho = arquivo.display().to_string(),
                        erro = erro.to_string()
                    )
                )
                .as_bytes(),
            ),
        }
    }
    sessao
}

/// O caminho com `.cifrado` no fim, a menos que já termine assim.
pub fn com_sufixo_cifrado(arquivo: &Path) -> PathBuf {
    if arquivo.to_string_lossy().ends_with(SUFIXO_CIFRADO) {
        arquivo.to_owned()
    } else {
        let mut texto = arquivo.as_os_str().to_owned();
        texto.push(SUFIXO_CIFRADO);
        PathBuf::from(texto)
    }
}

/// Faz a sessão gravar o log cifrado em `arquivo`. Só precisa da chave pública
/// (`logs-cifrados.pub`, na pasta de dados), então o cofre trancado não atrapalha. Sem ela não
/// grava nada, nem em texto: a pessoa pediu cifrado.
pub fn abrir_log_cifrado(
    sessao: &Session,
    arquivo: &Path,
    pasta_de_dados: &Path,
) -> std::io::Result<()> {
    let publica = crate::log_cifrado::ler_publica(pasta_de_dados)
        .ok_or_else(|| std::io::Error::other(t!("log-sem-chave-publica")))?;
    let escritor = crate::log_cifrado::EscritorCifrado::abrir(arquivo, &publica.publica)?;
    sessao.log_with(Box::new(escritor), LogMode::Text);
    Ok(())
}

/// Um programa que o [`FakeOpener`] "lançou": o que a tela pediu e o lançamento que o teste
/// controla.
#[derive(Clone, Debug)]
pub struct Lancado {
    /// O id da conexão.
    pub conexao: u64,
    /// O comando (vazio no virt-manager local).
    pub comando: String,
    /// O X11 e os túneis que o `lancar` pediu.
    pub encaminhamento: Encaminhamento,
    /// O estado, que o teste muda.
    pub lancamento: Lancamento,
}

/// Um comando que o [`FakeOpener`] rodou: o id da conexão, o comando e a entrada.
pub type Execucao = (u64, String, Vec<u8>);

/// O abridor dos testes: uma sessão `Loopback` com eco, sem rede nem processo. Guarda as
/// senhas que a tela entregou, para o teste conferir.
#[derive(Clone)]
pub struct FakeOpener {
    /// As senhas recebidas, na ordem das chamadas.
    pub senhas: Rc<RefCell<Vec<Senhas>>>,
    /// A senha que o servidor falso aceita nas conexões SSH. `None` aceita qualquer uma; com
    /// `Some`, outra senha fecha a sessão com `autenticação recusada para {usuário}`.
    pub senha_certa: Rc<RefCell<Option<String>>>,
    /// Os ids das conexões abertas, na ordem das chamadas.
    pub abertas: Rc<RefCell<Vec<u64>>>,
    /// O último caminho de agente que a tela entregou.
    pub agente: Rc<RefCell<Option<PathBuf>>>,
    /// Os pedidos de instalação de chave: o id da conexão, a linha pública e se forçou.
    pub instalacoes: Arc<Mutex<Vec<(u64, String, bool)>>>,
    /// O que o servidor falso responde à instalação da chave.
    pub resposta_da_instalacao: Rc<RefCell<Result<CopyIdOutcome, String>>>,
    /// Os servidores de arquivos de mentira, um por id de conexão (nascem vazios no primeiro
    /// pedido). O teste os enche antes de abrir a aba.
    pub servidores: Rc<RefCell<HashMap<u64, Rc<MemFs>>>>,
    /// O disco de mentira do painel da esquerda.
    pub disco: Rc<MemFs>,
    /// Quem abre o `disco` (sempre o mesmo `Arc`, para a fila reconhecer a ponta).
    disco_opener: ArquivosOpener,
    /// As senhas que a tela entregou ao abrir arquivos, na ordem das chamadas.
    pub senhas_dos_arquivos: Rc<RefCell<Vec<Senhas>>>,
    /// De qual conexão é cada sessão aberta (para o "Abrir arquivos desta sessão").
    pub sessoes: Rc<RefCell<Vec<(Session, u64)>>>,
    /// Os comandos rodados pelo [`Opener::executor`]: o id da conexão, o comando e a entrada
    /// (M42).
    pub execucoes: Arc<Mutex<Vec<Execucao>>>,
    /// O que o servidor falso responde ao comando, por id de conexão. Sem resposta para o id,
    /// o comando sai com o código 0 e a saída `ok de {nome}`.
    pub respostas_exec: Arc<Mutex<HashMap<u64, Result<ExecOutput, String>>>>,
    /// A resposta da sondagem por PowerShell (M46), quando o `sh -s` falha num servidor Windows.
    pub respostas_exec_windows: Arc<Mutex<HashMap<u64, Result<ExecOutput, String>>>>,
    /// Quanto o comando demora (zero por padrão), para os testes da espera.
    pub atraso_exec: Arc<Mutex<Duration>>,
    /// Com `true`, o servidor falso recusa o subsistema SFTP das sessões: o colar cai no plano B
    /// (M45).
    pub sem_sftp: Rc<Cell<bool>>,
    /// Os comandos rodados pelo [`Opener::exec_da_sessao`]: o id da conexão, o comando e a
    /// entrada que chegou a ser lida (M45).
    pub execs: Arc<Mutex<Vec<Execucao>>>,
    /// O servidor falso não tem `tar`.
    pub sem_tar: Arc<AtomicBool>,
    /// O `tar` do servidor falso não lê gzip.
    pub tar_sem_gzip: Arc<AtomicBool>,
    /// Com `true`, o `exec_da_sessao` para depois do primeiro bloco da entrada até o teste
    /// soltar (e avisar a condição): para o teste do cancelamento.
    pub segurar_exec: Arc<(Mutex<bool>, Condvar)>,
    /// As sessões abertas que não são SSH (shell local, Telnet, serial): sem arquivos nem exec.
    sem_ssh: Rc<RefCell<Vec<Session>>>,
    /// As distribuições do WSL que o abridor falso lista (M42).
    pub distros_wsl: Arc<Mutex<Vec<String>>>,
    /// A ponta de cada sessão aberta, para o teste fechar a conexão ou escrever nela (M42).
    pub handles: Rc<RefCell<Vec<(Session, LoopbackHandle)>>>,
    /// Os arquivos que a tela mandou abrir no editor externo, na ordem das chamadas.
    pub externos: Rc<RefCell<Vec<PathBuf>>>,
    /// As conexões de shell local que o abridor falso abriu, inteiras (programa, argumentos e
    /// ambiente), na ordem das chamadas (M63).
    pub locais: Rc<RefCell<Vec<Connection>>>,
    /// Os links que o terminal mandou abrir neste computador, na ordem das chamadas (M49).
    pub links: Rc<RefCell<Vec<DestinoDoLink>>>,
    /// Os links abertos pela rede de um perfil: o endereço e o perfil, na ordem das chamadas.
    pub links_pela_rede: Rc<RefCell<Vec<(String, RedeResolvida)>>>,
    /// A fonte do painel de portas (M58): as sessões novas leem dela (as cópias dividem as
    /// respostas). Sem ela, o painel fala com os canais da sessão, que no abridor falso não têm
    /// SSH.
    pub fonte_de_portas: Rc<RefCell<Option<crate::portas::descobrir::FonteFalsa>>>,
    /// Os destinos `host:porta` que o servidor falso não alcança: o `testador_de_destino` os
    /// recusa (M58).
    pub destinos_fechados: Arc<Mutex<Vec<(String, u16)>>>,
    /// Se `true`, o abridor falso recusa o link com um erro (para o aviso).
    pub link_falha: Rc<Cell<bool>>,
    /// Um servidor S3 de verdade (o de teste) para as conexões S3: com ele, `arquivos` e
    /// `s3_config` apontam para lá, com o endereço e o bucket dele; sem ele, a conexão S3 abre
    /// o `MemFs` de mentira e as ações do S3 dão erro. As chaves recebidas ficam em
    /// `senhas_dos_arquivos`.
    pub s3: Rc<RefCell<Option<S3Config>>>,
    /// O `rcd` de mentira que as conexões do rclone usam. `None` (o padrão): o binário não
    /// existe, e o `Rcds` responde "O rclone não foi encontrado".
    pub rcd: Arc<Mutex<Option<Arc<Rcd>>>>,
    /// Com `Some`, a configuração do rclone é cifrada: o `rcd` só sobe com esta senha, e sem
    /// ela o `Rcds` responde `ConfigPassword`.
    pub senha_da_config: Arc<Mutex<Option<String>>>,
    /// Quantas vezes o `Rcds` foi pedido a subir um processo (as fábricas, não os pedidos).
    pub subidas: Arc<AtomicUsize>,
    /// As últimas configurações que a tela entregou ([`Opener::configurar`]).
    pub config: Rc<RefCell<Configuracoes>>,
    /// A pasta de dados que veio com elas.
    pub pasta: Rc<RefCell<PathBuf>>,
    /// O X11 e os túneis de cada `open`, com o id da conexão (M37).
    pub encaminhamentos: Rc<RefCell<Vec<(u64, Encaminhamento)>>>,
    /// Os programas lançados sem aba ([`Opener::lancar`]), na ordem das chamadas.
    pub lancados: Rc<RefCell<Vec<Lancado>>>,
    /// Os virt-managers locais abertos ([`Opener::abrir_virt_local`]).
    pub virts_locais: Rc<RefCell<Vec<Lancado>>>,
    /// Os comandos abertos numa aba ([`Opener::abrir_comando`]): o id da conexão e o comando.
    pub comandos_em_aba: Rc<RefCell<Vec<(u64, String)>>>,
    /// Os pedidos de [`Opener::open_comando`]: o id da conexão e o pedido.
    pub comandos: Rc<RefCell<Vec<(u64, PedidoComando)>>>,
    /// As senhas que a tela entregou aos lançamentos.
    pub senhas_dos_lancamentos: Rc<RefCell<Vec<Senhas>>>,
    /// O que [`Opener::virt_manager_local`] responde.
    pub tem_virt_manager_local: Rc<Cell<bool>>,
    rcds: Arc<Rcds>,
}

impl Default for FakeOpener {
    fn default() -> Self {
        let disco = Rc::new(MemFs::new());
        let rcd: Arc<Mutex<Option<Arc<Rcd>>>> = Arc::default();
        let senha_da_config: Arc<Mutex<Option<String>>> = Arc::default();
        let subidas = Arc::new(AtomicUsize::new(0));
        let rcds = {
            let (rcd, exigida, subidas) = (
                Arc::clone(&rcd),
                Arc::clone(&senha_da_config),
                Arc::clone(&subidas),
            );
            Rcds::com_fabrica(
                OpcoesRclone::default(),
                Arc::new(move |_, _, senha| {
                    let exigida = exigida.lock().unwrap_or_else(PoisonError::into_inner);
                    if let Some(certa) = exigida.as_deref()
                        && senha != Some(certa)
                    {
                        return Err(RcloneError::ConfigPassword);
                    }
                    let rcd = rcd
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone()
                        .ok_or(RcloneError::NotFound)?;
                    rcd.check_version()?;
                    subidas.fetch_add(1, Ordering::SeqCst);
                    Ok(rcd)
                }),
            )
        };
        Self {
            rcd,
            senha_da_config,
            subidas,
            rcds,
            disco_opener: disco.opener(),
            disco,
            senhas: Rc::default(),
            senha_certa: Rc::default(),
            abertas: Rc::default(),
            locais: Rc::default(),
            agente: Rc::default(),
            instalacoes: Arc::default(),
            resposta_da_instalacao: Rc::new(RefCell::new(Ok(CopyIdOutcome::Installed {
                path: ".ssh/authorized_keys".to_owned(),
            }))),
            servidores: Rc::default(),
            senhas_dos_arquivos: Rc::default(),
            sessoes: Rc::default(),
            handles: Rc::default(),
            execucoes: Arc::default(),
            respostas_exec: Arc::default(),
            respostas_exec_windows: Arc::default(),
            atraso_exec: Arc::default(),
            sem_sftp: Rc::default(),
            execs: Arc::default(),
            sem_tar: Arc::default(),
            tar_sem_gzip: Arc::default(),
            segurar_exec: Arc::default(),
            sem_ssh: Rc::default(),
            distros_wsl: Arc::default(),
            externos: Rc::default(),
            links: Rc::default(),
            links_pela_rede: Rc::default(),
            fonte_de_portas: Rc::default(),
            destinos_fechados: Arc::default(),
            link_falha: Rc::default(),
            s3: Rc::default(),
            config: Rc::default(),
            pasta: Rc::default(),
            encaminhamentos: Rc::default(),
            lancados: Rc::default(),
            virts_locais: Rc::default(),
            comandos_em_aba: Rc::default(),
            comandos: Rc::default(),
            senhas_dos_lancamentos: Rc::default(),
            tem_virt_manager_local: Rc::default(),
        }
    }
}

impl FakeOpener {
    /// Um abridor novo, sem nada registrado.
    pub fn new() -> Self {
        Self::default()
    }

    /// Os comandos que as sessões abertas por este abridor rodam no lugar do shell, na ordem
    /// em que abriram (as do tmux e as do [`Opener::duplicar_rodando`]).
    pub fn comandos_rodados(&self) -> Vec<String> {
        self.handles
            .borrow()
            .iter()
            .filter_map(|(_, h)| h.command())
            .collect()
    }

    /// A ponta da `sessao` aberta por este abridor.
    pub fn handle_de(&self, sessao: Session) -> Option<LoopbackHandle> {
        self.handles
            .borrow()
            .iter()
            .find(|(s, _)| *s == sessao)
            .map(|(_, h)| h.clone())
    }

    /// Fecha a conexão da `sessao`, com o motivo (a aba fica caída). O teste deixa o `TestApp`
    /// rodar um quadro para a queda valer.
    pub fn derrubar_sessao(&self, sessao: Session, motivo: &str) {
        if let Some(handle) = self.handle_de(sessao) {
            handle.close(Some(motivo.to_owned()));
        }
    }

    /// Fecha as conexões de todas as sessões abertas da conexão `id`, com o motivo.
    pub fn derrubar(&self, id: u64, motivo: &str) {
        let sessoes: Vec<Session> = self
            .sessoes
            .borrow()
            .iter()
            .filter(|(_, c)| *c == id)
            .map(|(s, _)| *s)
            .collect();
        for sessao in sessoes {
            self.derrubar_sessao(sessao, motivo);
        }
    }

    /// O id da conexão SSH da `sessao`; `None` se ela não foi aberta aqui ou não é SSH.
    fn id_da_sessao(&self, sessao: Session) -> Option<u64> {
        if self.sem_ssh.borrow().contains(&sessao) {
            return None;
        }
        self.sessoes
            .borrow()
            .iter()
            .find(|(s, _)| *s == sessao)
            .map(|(_, id)| *id)
    }

    /// O servidor de arquivos de mentira da conexão `id` (o mesmo a cada chamada).
    pub fn servidor(&self, id: u64) -> Rc<MemFs> {
        Rc::clone(
            self.servidores
                .borrow_mut()
                .entry(id)
                .or_insert_with(|| Rc::new(MemFs::new())),
        )
    }
}

impl Opener for FakeOpener {
    fn abrir_link(&self, destino: &DestinoDoLink) -> std::io::Result<()> {
        self.links.borrow_mut().push(destino.clone());
        if self.link_falha.get() {
            return Err(std::io::Error::other("sem navegador"));
        }
        Ok(())
    }

    fn abrir_link_pela_rede(&self, url: &str, rede: RedeResolvida) -> Result<(), String> {
        self.links_pela_rede
            .borrow_mut()
            .push((url.to_owned(), rede));
        if self.link_falha.get() {
            return Err("sem navegador".to_owned());
        }
        Ok(())
    }

    fn abrir_externo(&self, arquivo: &Path, _editor: &str) -> Result<(), String> {
        self.externos.borrow_mut().push(arquivo.to_owned());
        Ok(())
    }

    fn open(&self, conexao: &Connection, senhas: Senhas, n: u32) -> Session {
        let recusada = conexao.kind.usa_ssh()
            && self
                .senha_certa
                .borrow()
                .as_ref()
                .is_some_and(|certa| senhas.conexao.as_ref() != Some(certa));
        self.senhas.borrow_mut().push(senhas);
        self.abertas.borrow_mut().push(conexao.id);
        if conexao.kind == Kind::Local {
            self.locais.borrow_mut().push(conexao.clone());
        }
        self.encaminhamentos.borrow_mut().push((
            conexao.id,
            encaminhamento(conexao, &AjustesSsh::de(&self.config.borrow())),
        ));
        let loopback = match &conexao.sessao_persistente {
            Some(sessao) if conexao.kind == Kind::Ssh => Loopback::new()
                .echo()
                .running(crate::tmux::comando_da_aba(sessao)),
            _ => Loopback::new().echo(),
        };
        let handle = loopback.handle();
        let sessao = configura(
            Session::new(loopback),
            conexao,
            n,
            &self.config.borrow(),
            &self.pasta.borrow(),
        );
        self.sessoes.borrow_mut().push((sessao, conexao.id));
        if !conexao.kind.usa_ssh() {
            self.sem_ssh.borrow_mut().push(sessao);
        }
        self.handles.borrow_mut().push((sessao, handle.clone()));
        if recusada {
            handle.close(Some(t!(
                "abrir-autenticacao-recusada",
                usuario = usuario_efetivo(conexao).to_string()
            )));
            return sessao;
        }
        sessao.feed(
            format!(
                "{}\r\n$ ",
                t!("abrir-conectado", nome = conexao.name.as_str())
            )
            .as_bytes(),
        );
        sessao
    }

    fn configurar(&self, config: &Configuracoes, pasta: &Path) {
        *self.config.borrow_mut() = config.clone();
        pasta.clone_into(&mut self.pasta.borrow_mut());
    }

    fn duplicar_rodando(&self, origem: Session, comando: Option<&str>) -> Option<Session> {
        let id = self.id_da_sessao(origem)?;
        let loopback = match comando {
            Some(comando) => Loopback::new().echo().running(comando),
            None => Loopback::new().echo(),
        };
        let handle = loopback.handle();
        let copia = Session::new(loopback).name(origem.title());
        self.sessoes.borrow_mut().push((copia, id));
        self.handles.borrow_mut().push((copia, handle.clone()));
        // O encerrar da sessão (`kill-session` ou o `-X quit` do screen) acaba logo, como no
        // servidor.
        if comando.is_some_and(|c| c.starts_with("tmux kill-session") || c.contains(" -X quit")) {
            handle.close(None);
        }
        Some(copia)
    }

    fn definir_agente(&self, caminho: Option<PathBuf>) {
        *self.agente.borrow_mut() = caminho;
    }

    fn rcds(&self) -> Option<Arc<Rcds>> {
        Some(Arc::clone(&self.rcds))
    }

    fn arquivos(&self, conexao: &Connection, senhas: Senhas) -> Result<ArquivosOpener, String> {
        if conexao.kind == Kind::Rclone {
            return Ok(rclone::abridor(
                Arc::clone(&self.rcds),
                conexao.rclone.spec(),
                senhas.rede.as_ref().and_then(RedeResolvida::url_do_proxy),
            ));
        }
        let recusada = self
            .senha_certa
            .borrow()
            .as_ref()
            .is_some_and(|certa| senhas.conexao.as_ref() != Some(certa));
        self.senhas_dos_arquivos.borrow_mut().push(senhas);
        if recusada {
            return Err(t!(
                "abrir-autenticacao-recusada",
                usuario = usuario_efetivo(conexao).to_string()
            ));
        }
        if conexao.kind == Kind::S3
            && let Some(config) = self.s3.borrow().clone()
        {
            return Ok(config.opener());
        }
        Ok(self.servidor(conexao.id).opener())
    }

    fn lancar(&self, conexao: &Connection, senhas: Senhas, comando: &str) -> Lancamento {
        self.senhas_dos_lancamentos.borrow_mut().push(senhas);
        let lancamento = Lancamento::controlado(RemoteAppStatus::Running);
        self.lancados.borrow_mut().push(Lancado {
            conexao: conexao.id,
            comando: comando.to_owned(),
            encaminhamento: encaminhamento_de_programa(
                conexao,
                &AjustesSsh::de(&self.config.borrow()),
            ),
            lancamento: lancamento.clone(),
        });
        lancamento
    }

    fn abrir_comando(
        &self,
        conexao: &Connection,
        senhas: Senhas,
        n: u32,
        comando: &str,
    ) -> Session {
        self.comandos_em_aba
            .borrow_mut()
            .push((conexao.id, comando.to_owned()));
        self.open(conexao, senhas, n)
    }

    fn open_comando(
        &self,
        conexao: &Connection,
        senhas: Senhas,
        n: u32,
        pedido: PedidoComando,
    ) -> Session {
        let titulo = pedido.titulo.clone();
        self.comandos.borrow_mut().push((conexao.id, pedido));
        self.open(conexao, senhas, n).name(titulo)
    }

    fn virt_manager_local(&self) -> bool {
        self.tem_virt_manager_local.get()
    }

    fn abrir_virt_local(&self, conexao: &Connection, senhas: Senhas) -> Lancamento {
        self.senhas_dos_lancamentos.borrow_mut().push(senhas);
        let lancamento = Lancamento::controlado(RemoteAppStatus::Running);
        self.virts_locais.borrow_mut().push(Lancado {
            conexao: conexao.id,
            comando: String::new(),
            encaminhamento: Encaminhamento::default(),
            lancamento: lancamento.clone(),
        });
        lancamento
    }

    fn s3_config(&self, conexao: &Connection, senhas: Senhas) -> Result<S3Config, String> {
        match self.s3.borrow().clone() {
            Some(config) => Ok(config),
            None => s3_config_com(conexao, senhas, &self.config.borrow().s3),
        }
    }

    fn sessao_tem_arquivos(&self, sessao: Session) -> bool {
        self.id_da_sessao(sessao).is_some()
    }

    fn fonte_de_portas(
        &self,
        abridor: crate::portas::descobrir::AbridorCompartilhado,
        cfg: &crate::configuracoes::PortasCfg,
    ) -> Box<dyn crate::portas::descobrir::FontePortas> {
        match self.fonte_de_portas.borrow().as_ref() {
            Some(falsa) => Box::new(falsa.clone()),
            None => Box::new(crate::portas::descobrir::FonteSsh::new(
                Arc::new(crate::portas::descobrir::CanaisDaSessao(abridor)),
                cfg.clone(),
            )),
        }
    }

    fn testador_de_destino(&self, _sessao: Session) -> Result<TestadorDeDestino, String> {
        let fechados = Arc::clone(&self.destinos_fechados);
        Ok(Arc::new(move |host, porta, _limite| {
            let fechados = fechados.lock().unwrap_or_else(PoisonError::into_inner);
            if fechados.iter().any(|(h, p)| h == host && *p == porta) {
                Err(format!("Nada escuta em {host}:{porta} no servidor."))
            } else {
                Ok(())
            }
        }))
    }

    fn arquivos_da_sessao(&self, sessao: Session) -> Result<ArquivosOpener, String> {
        let id = self
            .id_da_sessao(sessao)
            .ok_or_else(|| t!("abrir-sessao-sem-ssh"))?;
        if self.sem_sftp.get() {
            return Ok(Arc::new(|| {
                Err(std::io::Error::other(
                    "o servidor recusou o subsistema sftp",
                ))
            }));
        }
        Ok(self.servidor(id).opener())
    }

    fn exec_da_sessao(&self, sessao: Session) -> Result<ExecDaSessao, String> {
        let id = self
            .id_da_sessao(sessao)
            .ok_or_else(|| t!("abrir-sessao-sem-ssh"))?;
        let servidor = ServidorFalso {
            id,
            arquivos: self.servidor(id).connect(),
            execs: Arc::clone(&self.execs),
            sem_tar: Arc::clone(&self.sem_tar),
            tar_sem_gzip: Arc::clone(&self.tar_sem_gzip),
            segurar: Arc::clone(&self.segurar_exec),
        };
        // A sondagem dos assistentes de IA (M46) responde por `respostas_exec`, como o `executor`.
        let execucoes = Arc::clone(&self.execucoes);
        let respostas = Arc::clone(&self.respostas_exec);
        let respostas_windows = Arc::clone(&self.respostas_exec_windows);
        let atraso = Arc::clone(&self.atraso_exec);
        Ok(Arc::new(move |comando, entrada, enviado| {
            if comando != "sh -s" && comando != COMANDO_SONDAGEM_WINDOWS {
                return servidor.rodar(comando, entrada, enviado);
            }
            let respostas = if comando == "sh -s" {
                &respostas
            } else {
                &respostas_windows
            };
            let mut lido = Vec::new();
            entrada
                .read_to_end(&mut lido)
                .map_err(|erro| erro.to_string())?;
            execucoes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((id, comando.to_owned(), lido));
            let espera = *atraso.lock().unwrap_or_else(PoisonError::into_inner);
            if !espera.is_zero() {
                std::thread::sleep(espera);
            }
            let resposta = respostas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&id)
                .cloned();
            resposta.unwrap_or_else(|| {
                Ok(ExecOutput {
                    status: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                })
            })
        }))
    }

    fn disco(&self) -> Disco {
        Disco {
            opener: Arc::clone(&self.disco_opener),
            endpoint: Endpoint::Remote(Arc::clone(&self.disco_opener)),
        }
    }

    fn listar_wsl(&self) -> ListarWsl {
        let distros = Arc::clone(&self.distros_wsl);
        Box::new(move || {
            Ok(distros
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone())
        })
    }

    fn executor(&self, conexao: &Connection, _senhas: Senhas) -> Executor {
        let (id, nome) = (conexao.id, conexao.name.clone());
        let execucoes = Arc::clone(&self.execucoes);
        let respostas = Arc::clone(&self.respostas_exec);
        let respostas_windows = Arc::clone(&self.respostas_exec_windows);
        let atraso = Arc::clone(&self.atraso_exec);
        Box::new(move |comando, entrada| {
            let respostas = if comando == COMANDO_SONDAGEM_WINDOWS {
                &respostas_windows
            } else {
                &respostas
            };
            execucoes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((id, comando.to_owned(), entrada.to_vec()));
            let espera = *atraso.lock().unwrap_or_else(PoisonError::into_inner);
            if !espera.is_zero() {
                std::thread::sleep(espera);
            }
            let resposta = respostas
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&id)
                .cloned();
            resposta.unwrap_or_else(|| {
                Ok(ExecOutput {
                    status: Some(0),
                    stdout: format!("ok de {nome}\n").into_bytes(),
                    stderr: Vec::new(),
                })
            })
        })
    }

    fn instalador(&self, conexao: &Connection, _senhas: Senhas) -> Instalador {
        let id = conexao.id;
        let instalacoes = Arc::clone(&self.instalacoes);
        let resposta = self.resposta_da_instalacao.borrow().clone();
        Box::new(move |chave, forcar| {
            let linha = tomni_ui::terminal::ssh::keys::public_openssh(&chave);
            instalacoes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((id, linha.trim_end().to_owned(), forcar));
            resposta
        })
    }
}

/// A impressão SHA-256 (32 bytes separados por `:`) com um espaço no meio, para o texto do
/// diálogo ter onde quebrar a linha.
fn impressao_em_duas_linhas(sha256: &str) -> String {
    let partes: Vec<&str> = sha256.split(':').collect();
    if partes.len() < 4 {
        return sha256.to_owned();
    }
    let (a, b) = partes.split_at(partes.len() / 2);
    format!("{} {}", a.join(":"), b.join(":"))
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    #[test]
    fn a_faixa_de_portas_do_modo_ativo() {
        assert_eq!(faixa_de_portas("10000-10100"), Some(10000..=10100));
        assert_eq!(faixa_de_portas(" 5000 "), Some(5000..=5000));
        assert_eq!(faixa_de_portas(""), None);
        assert_eq!(faixa_de_portas("200-100"), None);
        assert_eq!(faixa_de_portas("0-10"), None);
        assert_eq!(faixa_de_portas("a-b"), None);
    }

    #[test]
    fn a_impressao_ganha_um_espaco_no_meio() {
        assert_eq!(impressao_em_duas_linhas("AB:CD:EF:01"), "AB:CD EF:01");
        assert_eq!(impressao_em_duas_linhas("AB:CD"), "AB:CD");
    }

    #[test]
    fn so_o_ssh_sem_chave_pede_senha() {
        let mut c = Connection::new("a", Kind::Ssh);
        assert!(precisa_de_senha(&c));
        c.key_file = Some("/chave".into());
        assert!(!precisa_de_senha(&c));
        // A chave do cofre também dispensa a senha.
        c.key_file = None;
        c.chave_do_cofre = Some("abc".into());
        assert!(!precisa_de_senha(&c));
        assert!(!precisa_de_senha(&Connection::new("b", Kind::Local)));
        assert!(!precisa_de_senha(&Connection::new("c", Kind::Telnet)));
    }

    #[test]
    fn os_modos_do_agente_nao_pedem_a_senha_antes() {
        let mut c = Connection::new("a", Kind::Ssh);
        c.credencial_modo = ModoCredencial::Perguntar;
        assert!(precisa_de_senha(&c));
        assert!(pede_senha_na_recusa(&c));

        c.credencial_modo = ModoCredencial::Agente;
        assert!(!precisa_de_senha(&c));
        assert!(
            !pede_senha_na_recusa(&c),
            "só o agente: a recusa fica na aba"
        );

        c.credencial_modo = ModoCredencial::AgenteOuSenha;
        assert!(!precisa_de_senha(&c));
        assert!(pede_senha_na_recusa(&c), "a recusa volta ao diálogo");

        // Com a chave privada, o "senão a senha" também vale.
        c.key_file = Some("/chave".into());
        assert!(!precisa_de_senha(&c));
        assert!(pede_senha_na_recusa(&c));
        c.credencial_modo = ModoCredencial::Agente;
        assert!(!pede_senha_na_recusa(&c));

        // O SFTP segue o mesmo modo.
        let mut sftp = Connection::new("s", Kind::Sftp);
        sftp.credencial_modo = ModoCredencial::Agente;
        assert!(!precisa_de_senha(&sftp));
    }

    #[test]
    fn com_um_item_do_cofre_o_modo_nao_vale() {
        for modo in ModoCredencial::TODOS {
            let mut c = Connection::new("a", Kind::Ssh);
            c.credencial_modo = modo;
            c.credencial = Some(3);
            // Como antes: sem a senha no item, a tela pergunta.
            assert!(precisa_de_senha(&c), "{modo:?}");
            assert!(pede_senha_na_recusa(&c), "{modo:?}");
        }
        // Nos tipos sem SSH, o modo não muda nada.
        let mut ftp = Connection::new("f", Kind::Ftp);
        ftp.user = "ana".into();
        ftp.credencial_modo = ModoCredencial::Agente;
        assert!(precisa_de_senha(&ftp));
        let mut telnet = Connection::new("t", Kind::Telnet);
        telnet.credencial_modo = ModoCredencial::AgenteOuSenha;
        assert!(!pede_senha_na_recusa(&telnet));
    }

    #[test]
    fn chave_da_sessao_leva_o_id_e_o_numero_da_aba() {
        let mut c = Connection::new("a", Kind::Ssh);
        c.id = 7;
        assert_eq!(chave_da_sessao(&c, 1), "7-1");
        assert_eq!(chave_da_sessao(&c, 2), "7-2");
    }

    #[test]
    fn o_abridor_falso_guarda_o_pedido_de_comando_e_poe_o_titulo_na_aba() {
        let fake = FakeOpener::new();
        let mut c = Connection::new("web-01", Kind::Ssh);
        c.id = 3;
        let pedido = PedidoComando {
            titulo: "Claude Code · web-01".to_owned(),
            comando: "exec sh -c 'claude'".to_owned(),
            tuneis: vec![Forward::local(1455, "localhost", 1455)],
        };
        let sessao = fake.open_comando(&c, Senhas::default(), 1, pedido.clone());
        assert_eq!(*fake.comandos.borrow(), [(3, pedido)]);
        assert_eq!(sessao.title(), "Claude Code · web-01");
        assert_eq!(*fake.abertas.borrow(), [3]);
    }

    #[test]
    fn chave_conhecida_passa_sem_perguntar() {
        let fila: FilaChaves = signal(Vec::new());
        assert!(perguntar(
            fila,
            "web",
            "web:22",
            "ssh-ed25519",
            "SHA256:x",
            KnownHost::Known
        ));
        assert!(fila.get_untracked().is_empty());
    }

    /// Roda `perguntar` numa thread, espera o pedido chegar na fila e devolve o pedido.
    fn pergunta_em_outra_thread(
        fila: FilaChaves,
        conhecida: KnownHost,
    ) -> (std::thread::JoinHandle<bool>, Arc<PedidoChave>) {
        let thread = std::thread::spawn(move || {
            perguntar(
                fila,
                "web-01",
                "web-01.exemplo:22",
                "ssh-ed25519",
                "SHA256:abc",
                conhecida,
            )
        });
        let limite = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pedido) = ler_fila_agora(fila, |f| f.first().cloned()) {
                return (thread, pedido);
            }
            assert!(Instant::now() < limite, "o pedido não chegou à fila");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn chave_desconhecida_espera_a_resposta_da_tela() {
        let fila: FilaChaves = signal(Vec::new());
        let (thread, pedido) = pergunta_em_outra_thread(fila, KnownHost::Unknown);
        assert_eq!(pedido.host_porta, "web-01.exemplo:22");
        assert_eq!(pedido.algoritmo, "ssh-ed25519");
        assert_eq!(pedido.fingerprint, "SHA256:abc");
        assert!(!pedido.mudou);
        pedido.resposta.send(true).unwrap();
        assert!(thread.join().unwrap());
    }

    #[test]
    fn chave_mudada_chega_marcada_e_a_recusa_volta() {
        let fila: FilaChaves = signal(Vec::new());
        let (thread, pedido) = pergunta_em_outra_thread(fila, KnownHost::Changed { line: 3 });
        assert!(pedido.mudou);
        pedido.resposta.send(false).unwrap();
        assert!(!thread.join().unwrap());
    }

    #[test]
    fn pedido_descartado_sem_resposta_recusa() {
        let fila: FilaChaves = signal(Vec::new());
        let (thread, pedido) = pergunta_em_outra_thread(fila, KnownHost::Unknown);
        // A tela some com o pedido sem responder: o último `Sender` cai e a conexão recusa.
        mexer_na_fila(fila, Vec::clear);
        drop(pedido);
        assert!(!thread.join().unwrap());
    }

    #[test]
    fn varios_pedidos_ao_mesmo_tempo_ficam_na_fila() {
        let fila: FilaChaves = signal(Vec::new());
        let a = std::thread::spawn(move || {
            perguntar(
                fila,
                "a",
                "a:22",
                "ssh-ed25519",
                "SHA256:a",
                KnownHost::Unknown,
            )
        });
        let b = std::thread::spawn(move || {
            perguntar(
                fila,
                "b",
                "b:22",
                "ssh-ed25519",
                "SHA256:b",
                KnownHost::Unknown,
            )
        });
        let limite = Instant::now() + Duration::from_secs(5);
        while ler_fila_agora(fila, <[_]>::len) < 2 {
            assert!(Instant::now() < limite, "faltou um pedido na fila");
            std::thread::sleep(Duration::from_millis(5));
        }
        for pedido in ler_fila_agora(fila, <[_]>::to_vec) {
            pedido.resposta.send(pedido.conexao == "a").unwrap();
        }
        assert!(a.join().unwrap());
        assert!(!b.join().unwrap());
    }

    #[test]
    fn o_ssh_builder_monta_a_conexao_com_salto_chave_e_keepalive() {
        let fila: FilaChaves = signal(Vec::new());
        let mut c = Connection::new("interno", Kind::Ssh);
        c.host = "interno.exemplo".into();
        c.user = "ana".into();
        c.key_file = Some("/home/ana/.ssh/id".into());
        c.jump = Some(crate::modelo::Jump {
            host: "bastiao.exemplo".into(),
            port: 2200,
            user: String::new(),
            credencial: None,
        });
        let senhas = Senhas {
            conexao: Some("segredo".into()),
            salto: Some("do-salto".into()),
            chave: Some("da-chave".into()),
            token: None,
            ..Senhas::default()
        };
        let texto = format!("{:?}", ssh_builder(&c, senhas, fila, None, None));
        assert!(texto.contains("interno.exemplo"), "{texto}");
        assert!(texto.contains("bastiao.exemplo"), "{texto}");
        assert!(
            !texto.contains("segredo"),
            "o Debug não pode mostrar a senha: {texto}"
        );
        assert!(!texto.contains("do-salto"), "{texto}");
        assert!(!texto.contains("da-chave"), "{texto}");
    }

    fn rede_de_teste() -> RedeResolvida {
        use crate::redes::{PerfilRede, ProxyRede};
        let mut perfil = PerfilRede::new("Cliente A");
        perfil.id = 4;
        perfil.proxy = Some(ProxyRede::novo());
        perfil.salto = Some(crate::modelo::Jump {
            host: "entrada-vpn.exemplo".into(),
            port: 22,
            user: "vpn".into(),
            credencial: None,
        });
        RedeResolvida {
            perfil,
            proxy_usuario: Some("proxy-user".into()),
            proxy_senha: Some("p@ss:w/rd".into()),
            salto_senha: None,
            known_hosts: Some(PathBuf::from("/dados/redes/4/known_hosts")),
            nebula: None,
            nebula_erro: None,
            openvpn: None,
            openvpn_erro: None,
            motor: None,
            motor_erro: None,
            emprestimo: None,
        }
    }

    #[test]
    fn o_perfil_poe_o_proxy_o_gancho_e_o_salto_no_fundo_da_cadeia() {
        let fila: FilaChaves = signal(Vec::new());
        let mut c = Connection::new("interno", Kind::Ssh);
        c.host = "interno.exemplo".into();
        c.user = "ana".into();
        c.jump = Some(crate::modelo::Jump {
            host: "bastiao.exemplo".into(),
            port: 2200,
            user: String::new(),
            credencial: None,
        });
        let senhas = Senhas {
            rede: Some(rede_de_teste()),
            ..Senhas::default()
        };
        let texto = format!("{:?}", ssh_builder(&c, senhas, fila, None, None));
        let posicao = |trecho: &str| {
            texto
                .find(trecho)
                .unwrap_or_else(|| panic!("falta {trecho}: {texto}"))
        };
        // Destino → salto da conexão → salto do perfil (o mais fundo).
        assert!(
            posicao("interno.exemplo") < posicao("bastiao.exemplo"),
            "{texto}"
        );
        assert!(
            posicao("bastiao.exemplo") < posicao("entrada-vpn.exemplo"),
            "{texto}"
        );
        assert!(texto.contains("before_connect: Some"), "{texto}");
        assert!(texto.contains("Socks5"), "{texto}");
        assert!(texto.contains("/dados/redes/4/known_hosts"), "{texto}");
        assert!(
            !texto.contains("p@ss"),
            "o Debug não pode mostrar a senha: {texto}"
        );
    }

    #[test]
    fn sem_salto_na_conexao_o_do_perfil_vira_o_primeiro() {
        let fila: FilaChaves = signal(Vec::new());
        let mut c = Connection::new("interno", Kind::Ssh);
        c.host = "interno.exemplo".into();
        let senhas = Senhas {
            rede: Some(rede_de_teste()),
            ..Senhas::default()
        };
        let texto = format!("{:?}", ssh_builder(&c, senhas, fila, None, None));
        assert!(
            texto.find("interno.exemplo") < texto.find("entrada-vpn.exemplo"),
            "{texto}"
        );
    }

    #[test]
    fn sem_rede_o_ssh_builder_fica_como_era() {
        let fila: FilaChaves = signal(Vec::new());
        let mut c = Connection::new("interno", Kind::Ssh);
        c.host = "interno.exemplo".into();
        let texto = format!("{:?}", ssh_builder(&c, Senhas::default(), fila, None, None));
        assert!(texto.contains("before_connect: None"), "{texto}");
        assert!(texto.contains("proxy: None"), "{texto}");
        assert!(!texto.contains("entrada-vpn"), "{texto}");
    }

    #[test]
    fn a_url_do_proxy_do_rclone_escapa_o_usuario_e_a_senha() {
        let rede = rede_de_teste();
        assert_eq!(
            rede.url_do_proxy().as_deref(),
            Some("socks5h://proxy-user:p%40ss%3Aw%2Frd@127.0.0.1:1081")
        );
        let mut http = rede_de_teste();
        http.perfil.proxy.as_mut().unwrap().tipo = crate::redes::TipoProxy::Http;
        http.proxy_usuario = None;
        assert_eq!(
            http.url_do_proxy().as_deref(),
            Some("http://127.0.0.1:1081")
        );
    }

    #[test]
    fn o_s3_vai_pelo_proxy_do_perfil() {
        let mut c = Connection::new("bucket", Kind::S3);
        c.s3.endpoint = "https://s3.exemplo".into();
        let senhas = Senhas {
            rede: Some(rede_de_teste()),
            ..Senhas::default()
        };
        let config = s3_config(&c, senhas).unwrap();
        assert!(matches!(
            config.proxy,
            Some(tomni_ui::terminal::Proxy::Socks5 { port: 1081, .. })
        ));
        assert!(s3_config(&c, Senhas::default()).unwrap().proxy.is_none());
    }

    #[test]
    fn o_ftp_do_perfil_sai_pelo_proxy_e_sempre_passivo() {
        let fila: FilaChaves = signal(Vec::new());
        let mut c = Connection::new("ftp", Kind::Ftp);
        c.host = "ftp.exemplo".into();
        c.ftp_mode = FtpModo::Ativo;
        let senhas = Senhas {
            rede: Some(rede_de_teste()),
            ..Senhas::default()
        };
        let texto = format!("{:?}", ftp_builder(&c, senhas, fila, None));
        assert!(texto.contains("Socks5"), "{texto}");
        assert!(texto.contains("Passive"), "{texto}");
        let sem = format!("{:?}", ftp_builder(&c, Senhas::default(), fila, None));
        assert!(sem.contains("Active"), "{sem}");
    }

    #[test]
    fn so_a_rede_nao_conta_como_senha() {
        let so_rede = Senhas {
            rede: Some(rede_de_teste()),
            ..Senhas::default()
        };
        assert!(!so_rede.tem_segredo());
        let com_senha = Senhas {
            conexao: Some("x".into()),
            ..so_rede
        };
        assert!(com_senha.tem_segredo());
    }

    #[test]
    fn o_debug_das_senhas_nao_mostra_o_valor() {
        let senhas = Senhas {
            conexao: Some("segredo".into()),
            salto: None,
            chave: None,
            token: None,
            ..Senhas::default()
        };
        let texto = format!("{senhas:?}");
        assert!(!texto.contains("segredo"), "{texto}");
        assert!(texto.contains("***") && texto.contains('-'), "{texto}");
    }

    #[test]
    fn o_fake_guarda_senha_e_id_e_poe_chave_e_nome_na_sessao() {
        let fake = FakeOpener::new();
        let mut c = Connection::new("web-01", Kind::Ssh);
        c.id = 4;
        let senhas = Senhas {
            conexao: Some("s".into()),
            salto: Some("j".into()),
            chave: None,
            token: None,
            ..Senhas::default()
        };
        let sessao = fake.open(&c, senhas.clone(), 2);
        assert_eq!(sessao.key_str(), "4-2");
        assert_eq!(*fake.senhas.borrow(), [senhas]);
        assert_eq!(*fake.abertas.borrow(), [4]);
        sessao.dispose();
    }

    #[test]
    fn o_fake_recusa_a_senha_errada_so_no_ssh() {
        let fake = FakeOpener::new();
        *fake.senha_certa.borrow_mut() = Some("certa".into());
        let mut c = Connection::new("web", Kind::Ssh);
        c.user = "ana".into();
        let errada = Senhas {
            conexao: Some("errada".into()),
            salto: None,
            chave: None,
            token: None,
            ..Senhas::default()
        };
        let sessao = fake.open(&c, errada, 1);
        Session::poll_all();
        assert_eq!(
            sessao.close_reason().as_deref(),
            Some("autenticação recusada para ana")
        );
        sessao.dispose();

        let certa = Senhas {
            conexao: Some("certa".into()),
            salto: None,
            chave: None,
            token: None,
            ..Senhas::default()
        };
        let sessao = fake.open(&c, certa, 2);
        Session::poll_all();
        assert_eq!(sessao.close_reason(), None);
        assert!(sessao.text().contains("Conectado a web"));
        sessao.dispose();

        let local = Connection::new("sh", Kind::Local);
        let sessao = fake.open(&local, Senhas::default(), 1);
        Session::poll_all();
        assert_eq!(sessao.close_reason(), None);
        sessao.dispose();
    }

    #[test]
    fn o_terminal_local_usa_o_programa_da_conexao_e_senao_o_das_configuracoes() {
        let mut config = Configuracoes::default();
        let mut c = Connection::new("Shell", Kind::Local);
        // Nada na conexão nem nas Configurações: o shell da pessoa.
        assert_eq!(programa_local(&c, &config), None);
        // O das Configurações.
        config.terminal.shell.programa = "zsh".into();
        config.terminal.shell.argumentos = "-l".into();
        assert_eq!(
            programa_local(&c, &config),
            Some(("zsh".to_owned(), vec!["-l".to_owned()]))
        );
        // O da conexão ganha.
        c.local_programa = "wsl.exe".into();
        c.local_argumentos = "-d Ubuntu --cd ~".into();
        assert_eq!(
            programa_local(&c, &config),
            Some((
                "wsl.exe".to_owned(),
                vec!["-d".into(), "Ubuntu".into(), "--cd".into(), "~".into()]
            ))
        );
        // Só espaços no programa é o mesmo que vazio.
        c.local_programa = "   ".into();
        assert_eq!(
            programa_local(&c, &config).map(|(p, _)| p),
            Some("zsh".to_owned())
        );
    }

    #[test]
    fn o_terminal_local_comeca_na_pasta_da_conexao_e_senao_na_das_configuracoes() {
        let mut config = Configuracoes::default();
        let mut c = Connection::new("Shell", Kind::Local);
        // Nada na conexão nem nas Configurações: a pasta pessoal.
        assert_eq!(pasta_local(&c, &config), None);
        // A das Configurações.
        config.terminal.shell.pasta = "/srv/projetos".into();
        assert_eq!(
            pasta_local(&c, &config),
            Some(PathBuf::from("/srv/projetos"))
        );
        // A da conexão ganha; só espaços é o mesmo que vazio.
        c.local_pasta = "/var/www".into();
        assert_eq!(pasta_local(&c, &config), Some(PathBuf::from("/var/www")));
        c.local_pasta = "  ".into();
        assert_eq!(
            pasta_local(&c, &config),
            Some(PathBuf::from("/srv/projetos"))
        );
        // O `~` vira a pasta pessoal.
        c.local_pasta = "~/obra".into();
        let pasta = pasta_local(&c, &config).unwrap();
        assert!(!pasta.starts_with("~"), "{pasta:?}");
        assert!(pasta.ends_with("obra"), "{pasta:?}");
    }

    #[cfg(unix)]
    #[test]
    fn o_shell_local_abre_na_pasta_escolhida() {
        let pasta =
            std::env::temp_dir().join(format!("tirech-teste-pasta-shell-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let pasta = pasta.canonicalize().unwrap();
        let mut c = Connection::new("Shell", Kind::Local);
        c.local_programa = "sh".into();
        c.local_pasta = pasta.display().to_string();
        let sessao = configura(
            Session::new(processo_local(&c, &Configuracoes::default()).spawn()),
            &c,
            1,
            &Configuracoes::default(),
            Path::new("/"),
        );
        sessao.send(b"pwd -P\n");
        let esperado = pasta.display().to_string();
        let limite = Instant::now() + Duration::from_secs(10);
        loop {
            Session::poll_all();
            // A resposta é a linha da pasta: sozinha, se o prompt chegou antes do comando, ou com
            // o prompt na frente, se o comando chegou antes (o eco do terminal vem primeiro).
            if sessao.text().lines().any(|l| {
                let l = l.trim_end();
                l == esperado || l.ends_with(&format!(" {esperado}"))
            }) {
                break;
            }
            assert!(
                Instant::now() < limite,
                "o sh não abriu em {esperado}: {:?}",
                sessao.text()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        sessao.dispose();
        let _ = std::fs::remove_dir(&pasta);
    }

    #[test]
    fn uma_pasta_que_nao_existe_nao_impede_o_shell_local() {
        let mut c = Connection::new("Shell", Kind::Local);
        c.local_pasta = "/não/existe/tirech".into();
        let texto = format!("{:?}", processo_local(&c, &Configuracoes::default()));
        assert!(!texto.contains("/não/existe/tirech"), "{texto}");
    }

    #[cfg(unix)]
    #[test]
    fn o_shell_local_de_verdade_responde_a_um_comando() {
        let c = Connection::new("Shell", Kind::Local);
        let sessao = configura(
            Session::new(Process::builder("sh").spawn()),
            &c,
            1,
            &Configuracoes::default(),
            Path::new("/"),
        );
        sessao.send(b"echo oi-do-shell\n");
        let limite = Instant::now() + Duration::from_secs(10);
        loop {
            Session::poll_all();
            if sessao.text().contains("oi-do-shell") {
                break;
            }
            assert!(
                Instant::now() < limite,
                "sem resposta do sh: {:?}",
                sessao.text()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        sessao.dispose();
    }

    #[test]
    fn os_argumentos_do_shell_respeitam_aspas() {
        assert_eq!(dividir_argumentos(""), Vec::<String>::new());
        assert_eq!(dividir_argumentos("  -l   -i "), ["-l", "-i"]);
        assert_eq!(
            dividir_argumentos(r#"-c "echo oi mundo" 'a b' ""#),
            ["-c", "echo oi mundo", "a b", ""]
        );
    }

    #[test]
    fn juntar_e_dividir_os_argumentos_dao_o_mesmo() {
        let originais: Vec<String> = [
            "--mcp-config",
            "/tmp/a b/lancar.json",
            "d'Água",
            "\"citado\"",
            "",
            "mcp_servers.x.args=[\"mcp\", \"--aba\", \"1-2\"]",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            dividir_argumentos(&juntar_argumentos(&originais)),
            originais
        );
        assert_eq!(juntar_argumentos(&[]), "");
    }

    #[test]
    fn o_shell_local_vem_da_configuracao() {
        let mut config = Configuracoes::default();
        config.terminal.shell.programa = "/bin/zsh".to_owned();
        config.terminal.shell.argumentos = "-l -i".to_owned();
        let texto = format!("{:?}", shell_local(&config));
        assert!(texto.contains("/bin/zsh"), "{texto}");
        assert!(texto.contains("-l"), "{texto}");
    }

    #[test]
    fn os_logs_vao_para_a_pasta_escolhida_ou_para_logs() {
        let mut config = Configuracoes::default();
        assert_eq!(
            pasta_dos_logs(&config, Path::new("/dados")),
            Path::new("/dados/logs")
        );
        config.terminal.logs.pasta = "/meus/logs".to_owned();
        assert_eq!(
            pasta_dos_logs(&config, Path::new("/dados")),
            Path::new("/meus/logs")
        );
    }

    #[test]
    fn o_nome_do_log_limpa_o_nome_da_conexao() {
        let c = Connection::new("web 01 / prod", Kind::Ssh);
        let nome = nome_do_log(&c, 3, false);
        assert!(nome.starts_with("web-01---prod-"), "{nome}");
        assert!(nome.ends_with("-3.log"), "{nome}");
        let cifrado = nome_do_log(&c, 3, true);
        assert!(cifrado.ends_with("-3.log.cifrado"), "{cifrado}");
    }

    #[test]
    fn o_log_cifrado_ganha_o_sufixo_uma_vez_so() {
        assert_eq!(
            com_sufixo_cifrado(Path::new("/var/log/prod.log")),
            Path::new("/var/log/prod.log.cifrado")
        );
        assert_eq!(
            com_sufixo_cifrado(Path::new("/var/log/prod.log.cifrado")),
            Path::new("/var/log/prod.log.cifrado")
        );
    }

    /// Uma pasta de dados com o cofre aberto e a chave dos logs pronta, e a configuração com os
    /// logs cifrados ligados.
    fn dados_com_chave(
        nome: &str,
    ) -> (crate::armazenamento::testes::PastaTemporaria, Configuracoes) {
        let pasta = crate::armazenamento::testes::PastaTemporaria::nova(nome);
        std::fs::create_dir_all(&pasta.0).unwrap();
        let (mut cofre, _) = crate::cofre::Cofre::abrir(&pasta.0);
        crate::logs::garantir_chave(&mut cofre).unwrap();
        let mut config = Configuracoes::default();
        config.terminal.logs.cifrar = true;
        (pasta, config)
    }

    /// O texto que o log cifrado `arquivo` guarda, lido com as chaves do cofre da `pasta`.
    fn texto_do_log(pasta: &Path, arquivo: &Path) -> (String, crate::log_cifrado::Leitura) {
        let (cofre, _) = crate::cofre::Cofre::abrir(pasta);
        let chaves = crate::logs::mapa_de_chaves(&cofre);
        let mut origem = std::fs::File::open(arquivo).unwrap();
        let mut saida = Vec::new();
        let leitura = crate::log_cifrado::ler(
            &mut origem,
            &chaves,
            &mut saida,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .unwrap();
        (String::from_utf8(saida).unwrap(), leitura)
    }

    #[test]
    fn com_a_opcao_ligada_o_log_de_todas_as_sessoes_sai_cifrado() {
        let (pasta, mut config) = dados_com_chave("log-todas");
        config.terminal.logs.todas_as_sessoes = true;
        let c = Connection::new("Servidor", Kind::Ssh);
        let sessao = configura(Session::new(Loopback::new()), &c, 1, &config, &pasta.0);
        sessao.feed("segredo-no-log\r\n".as_bytes());
        sessao.flush_log();
        let arquivo = crate::logs::ativo_da_sessao(&sessao.key_str()).unwrap();
        assert!(
            arquivo.to_string_lossy().ends_with(".log.cifrado"),
            "{arquivo:?}"
        );
        assert!(arquivo.starts_with(pasta_dos_logs(&config, &pasta.0)));
        let bruto = std::fs::read(&arquivo).unwrap();
        assert!(!String::from_utf8_lossy(&bruto).contains("segredo-no-log"));
        sessao.stop_log();
        crate::logs::esquecer_ativo(&sessao.key_str());
        let (texto, leitura) = texto_do_log(&pasta.0, &arquivo);
        assert!(texto.contains("segredo-no-log"), "{texto:?}");
        assert_eq!(leitura.segmentos, 1);
        assert!(leitura.avisos.is_empty(), "{:?}", leitura.avisos);
        sessao.dispose();
    }

    #[test]
    fn com_a_opcao_desligada_o_log_continua_em_texto() {
        let (pasta, mut config) = dados_com_chave("log-texto");
        config.terminal.logs.cifrar = false;
        config.terminal.logs.todas_as_sessoes = true;
        let c = Connection::new("Servidor", Kind::Ssh);
        let sessao = configura(Session::new(Loopback::new()), &c, 1, &config, &pasta.0);
        sessao.feed("texto-claro\r\n".as_bytes());
        sessao.flush_log();
        let arquivo = crate::logs::ativo_da_sessao(&sessao.key_str()).unwrap();
        assert!(arquivo.to_string_lossy().ends_with(".log"), "{arquivo:?}");
        assert!(
            std::fs::read_to_string(&arquivo)
                .unwrap()
                .contains("texto-claro")
        );
        sessao.stop_log();
        crate::logs::esquecer_ativo(&sessao.key_str());
        sessao.dispose();
    }

    #[test]
    fn o_log_fixo_da_conexao_ganha_o_sufixo_e_a_segunda_sessao_vira_outro_segmento() {
        let (pasta, config) = dados_com_chave("log-fixo");
        let alvo = pasta.0.join("fixo").join("x.log");
        let mut c = Connection::new("Servidor", Kind::Ssh);
        c.log_file = Some(alvo.clone());
        let esperado = pasta.0.join("fixo").join("x.log.cifrado");
        for (n, frase) in [(1, "primeira"), (2, "segunda")] {
            let sessao = configura(Session::new(Loopback::new()), &c, n, &config, &pasta.0);
            sessao.feed(format!("{frase}\r\n").as_bytes());
            sessao.flush_log();
            assert_eq!(
                crate::logs::ativo_da_sessao(&sessao.key_str()).unwrap(),
                esperado
            );
            sessao.stop_log();
            crate::logs::esquecer_ativo(&sessao.key_str());
            sessao.dispose();
        }
        assert!(!alvo.exists(), "nada pode ir para o .log em texto");
        let (texto, leitura) = texto_do_log(&pasta.0, &esperado);
        assert_eq!(leitura.segmentos, 2);
        assert!(
            texto.contains("primeira") && texto.contains("segunda"),
            "{texto:?}"
        );
        assert!(leitura.avisos.is_empty(), "{:?}", leitura.avisos);
    }

    #[test]
    fn sem_a_chave_publica_a_sessao_abre_sem_log_e_avisa_no_terminal() {
        let (pasta, mut config) = dados_com_chave("log-sem-pub");
        config.terminal.logs.todas_as_sessoes = true;
        std::fs::remove_file(pasta.0.join(crate::log_cifrado::ARQUIVO_PUBLICO)).unwrap();
        let c = Connection::new("Servidor", Kind::Ssh);
        let sessao = configura(Session::new(Loopback::new()), &c, 1, &config, &pasta.0);
        assert!(!sessao.is_logging());
        assert!(crate::logs::ativo_da_sessao(&sessao.key_str()).is_none());
        // O terminal quebra a linha onde der, até no meio da palavra (conforme o tamanho do
        // caminho do log): compara sem nenhum espaço.
        let sem_espaco = |texto: &str| texto.split_whitespace().collect::<String>();
        let na_tela = sem_espaco(&sessao.text());
        assert!(
            na_tela.contains(&sem_espaco(&t!("log-sem-chave-publica"))),
            "{na_tela:?}"
        );
        let logs = pasta_dos_logs(&config, &pasta.0);
        let criados = std::fs::read_dir(&logs).map(|d| d.count()).unwrap_or(0);
        assert_eq!(criados, 0, "nenhum arquivo, nem em texto");
        sessao.dispose();
    }

    #[test]
    fn o_encaminhamento_do_agente_vai_para_o_builder() {
        let fila: FilaChaves = signal(Vec::new());
        let c = Connection::new("x", Kind::Ssh);
        let sem = format!("{:?}", ssh_builder(&c, Senhas::default(), fila, None, None));
        assert!(sem.contains("agent_forward: false"), "{sem}");
        let ajustes = AjustesSsh {
            encaminhar_agente: true,
            ..AjustesSsh::default()
        };
        let com = format!(
            "{:?}",
            ssh_builder_com(&c, Senhas::default(), fila, None, None, ajustes)
        );
        assert!(com.contains("agent_forward: true"), "{com}");
    }
}
