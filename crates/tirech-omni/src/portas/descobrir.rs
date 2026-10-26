//! Quem conversa com o servidor (M58, seções 3.1, 3.5 e 3.10 do plano): lê as portas por `ss`,
//! pelo `/proc` (SFTP) ou pelo `netstat`, lista os contêineres, sonda cada porta nova pelo canal
//! SSH e entrega tudo a uma thread por sessão. A parte que só olha bytes está em `ler.rs` e
//! `sonda.rs`.

use std::collections::{HashMap, HashSet};
use std::io::{self, Cursor, Read};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tomni_ui::files::Fs;
use tomni_ui::terminal::ssh::{ExecOutput, SshChannelOpener};

use super::catalogo::{self, APPS};
use super::ler;
use super::sonda::{self, RespostaHttp};
use super::{Atalho, Descoberta, Fonte, PortaDoServidor, Processo, Servico};
use crate::configuracoes::PortasCfg;
use crate::t;

/// O tempo que um comando de leitura (`ss`, `netstat`, `docker`) tem para terminar.
const TEMPO_COMANDO: Duration = Duration::from_secs(15);

/// Quantas sondas correm ao mesmo tempo. Não é um teto de resultados: as portas além do lote
/// esperam a vez.
const SONDAS_EM_PARALELO: usize = 8;

/// O script dos contêineres, entregue ao `sh -s` pela entrada padrão (as aspas do formato do
/// `docker inspect` não passam por um `sh -c` sem sofrimento). `exit 127`: sem docker nem podman.
pub const SCRIPT_CONTEINERES: &str = r#"c=docker
command -v docker >/dev/null 2>&1 || c=podman
command -v "$c" >/dev/null 2>&1 || exit 127
ids=$("$c" ps -q 2>&1) || { printf '%s\n' "$ids" >&2; exit 1; }
[ -n "$ids" ] || exit 0
"$c" inspect --format '{{.Name}}\t{{.State.Status}}\t{{range $k, $v := .NetworkSettings.Networks}}{{$k}}={{$v.IPAddress}} {{end}}\t{{json .Config.ExposedPorts}}\t{{json .NetworkSettings.Ports}}' $ids
"#;

/// O abridor de canais da sessão, que a tela renova a cada leitura: depois de um `restart` o
/// abridor antigo só devolve erro.
pub type AbridorCompartilhado = Arc<Mutex<Option<SshChannelOpener>>>;

/// Os [`Canais`] de uma sessão que pode reiniciar: usam o abridor de agora.
pub struct CanaisDaSessao(pub AbridorCompartilhado);

impl CanaisDaSessao {
    fn abridor(&self) -> io::Result<SshChannelOpener> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, t!("portas-sem-ssh")))
    }
}

impl Canais for CanaisDaSessao {
    fn exec(&self, comando: &str, entrada: &[u8], tempo: Duration) -> io::Result<ExecOutput> {
        Canais::exec(&self.abridor()?, comando, entrada, tempo)
    }

    fn sftp(&self) -> io::Result<Box<dyn Fs>> {
        Canais::sftp(&self.abridor()?)
    }

    fn sondar(
        &self,
        host: &str,
        porta: u16,
        caminho: &str,
        host_http: &str,
        tempo: Duration,
    ) -> io::Result<ResultadoSonda> {
        self.abridor()?
            .sondar(host, porta, caminho, host_http, tempo)
    }
}

/// A conversa de uma sonda com uma porta: o que se achou e as respostas que vieram.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultadoSonda {
    pub servico: Option<Servico>,
    pub resposta: Option<RespostaHttp>,
}

/// O que `descobrir` precisa da conexão SSH. A real é o [`SshChannelOpener`]; os testes põem uma
/// que responde por tabela.
pub trait Canais: Send + Sync {
    /// Roda um comando no servidor, com `entrada` na entrada padrão.
    fn exec(&self, comando: &str, entrada: &[u8], tempo: Duration) -> io::Result<ExecOutput>;

    /// Abre o SFTP.
    fn sftp(&self) -> io::Result<Box<dyn Fs>>;

    /// Abre `host:porta` pelo servidor e conversa: espera uma saudação, manda o `GET` e lê.
    fn sondar(
        &self,
        host: &str,
        porta: u16,
        caminho: &str,
        host_http: &str,
        tempo: Duration,
    ) -> io::Result<ResultadoSonda>;
}

impl Canais for SshChannelOpener {
    fn exec(&self, comando: &str, entrada: &[u8], tempo: Duration) -> io::Result<ExecOutput> {
        let mut entrada = Cursor::new(entrada.to_vec());
        SshChannelOpener::exec(self, comando, &mut entrada, &mut |_| {}, tempo)
    }

    fn sftp(&self) -> io::Result<Box<dyn Fs>> {
        Ok(Box::new(tomni_ui::files::sftp::Sftp::from_channel(self)?))
    }

    fn sondar(
        &self,
        host: &str,
        porta: u16,
        caminho: &str,
        host_http: &str,
        tempo: Duration,
    ) -> io::Result<ResultadoSonda> {
        let inicio = Instant::now();
        let mut canal = self.open_direct(host, porta, tempo)?;
        let restante = tempo.saturating_sub(inicio.elapsed());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        Ok(runtime.block_on(async {
            let resultado = conversar(&mut canal, porta, caminho, host_http, restante).await;
            soltar_canal(canal).await;
            resultado
        }))
    }
}

/// Solta um canal SSH dentro do `runtime` que o usa: o `Drop` dele manda o fechamento por uma
/// tarefa do tokio e panica fora de um. O `yield_now` deixa essa tarefa rodar antes de o runtime acabar.
pub async fn soltar_canal(canal: tomni_ui::terminal::ssh::SshChannel) {
    drop(canal);
    tokio::task::yield_now().await;
}

/// A conversa da sonda por um fluxo qualquer (o canal SSH, ou um `duplex` nos testes): uma
/// espera passiva de um terço do tempo por saudações, e então o `GET` se a porta aceita.
pub async fn conversar<S>(
    fluxo: &mut S,
    porta: u16,
    caminho: &str,
    host_http: &str,
    tempo: Duration,
) -> ResultadoSonda
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let inicio = Instant::now();
    let mut lido: Vec<u8> = Vec::new();

    // Saudações: SSH, MySQL, FTP, SMTP… falam primeiro.
    let passivo = tempo / 3;
    let fechou = ler_ate(fluxo, &mut lido, passivo, |_| false).await;
    if !lido.is_empty() {
        if let Some(servico) = sonda::classificar_saudacao(&lido) {
            return ResultadoSonda {
                servico: Some(servico),
                resposta: None,
            };
        }
        if sonda::parece_tls(&lido) {
            return resultado_http(sonda::ler_resposta_http(&lido));
        }
    }
    if fechou && lido.is_empty() {
        return ResultadoSonda::default();
    }
    if !sonda::manda_get(porta) {
        return ResultadoSonda::default();
    }

    // Em silêncio: quem fala HTTP espera o pedido.
    let pedido = sonda::pedido_http(porta, caminho, host_http);
    if fluxo.write_all(&pedido).await.is_err() || fluxo.flush().await.is_err() {
        return ResultadoSonda::default();
    }
    let restante = tempo.saturating_sub(inicio.elapsed());
    lido.clear();
    ler_ate(fluxo, &mut lido, restante, cabeca_completa).await;
    if lido.is_empty() {
        return ResultadoSonda::default();
    }
    if let Some(servico) = sonda::classificar_saudacao(&lido)
        && sonda::ler_resposta_http(&lido).is_none()
    {
        return ResultadoSonda {
            servico: Some(servico),
            resposta: None,
        };
    }
    resultado_http(sonda::ler_resposta_http(&lido))
}

fn resultado_http(resposta: Option<RespostaHttp>) -> ResultadoSonda {
    ResultadoSonda {
        servico: resposta.as_ref().map(|r| r.servico(false)),
        resposta,
    }
}

/// Se já se leu o título (`</title>`) ou o fim do `<head>`.
fn cabeca_completa(bytes: &[u8]) -> bool {
    let texto = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    texto.contains("</title>") || texto.contains("</head>")
}

/// Lê para `destino` até `pronto` aceitar o que há, até o fluxo fechar ou até o tempo acabar.
/// Devolve se o fluxo fechou.
async fn ler_ate<S>(
    fluxo: &mut S,
    destino: &mut Vec<u8>,
    tempo: Duration,
    pronto: impl Fn(&[u8]) -> bool,
) -> bool
where
    S: AsyncRead + Unpin,
{
    let fim = tokio::time::Instant::now() + tempo;
    let mut bloco = [0u8; 4096];
    loop {
        match tokio::time::timeout_at(fim, fluxo.read(&mut bloco)).await {
            Ok(Ok(0) | Err(_)) => return true,
            Ok(Ok(n)) => {
                destino.extend_from_slice(&bloco[..n]);
                if pronto(destino) {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
}

// ----- /proc e /etc/passwd por SFTP -----

/// Lê um arquivo inteiro até o fim: os de `/proc` dizem ter tamanho 0.
fn ler_arquivo(fs: &mut dyn Fs, caminho: &str) -> io::Result<String> {
    let mut arquivo = fs.open_read(caminho, 0)?;
    let mut bytes = Vec::new();
    arquivo.read_to_end(&mut bytes)?;
    arquivo.finish()?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// As escutas de `/proc/net/tcp` e `/proc/net/tcp6`. `Err` só se nenhum dos dois abre.
pub fn ler_proc_por_sftp(fs: &mut dyn Fs) -> io::Result<Vec<super::Escuta>> {
    let v4 = ler_arquivo(fs, "/proc/net/tcp");
    let v6 = ler_arquivo(fs, "/proc/net/tcp6");
    if let (Err(erro), Err(_)) = (&v4, &v6) {
        return Err(io::Error::new(erro.kind(), erro.to_string()));
    }
    let mut escutas = Vec::new();
    if let Ok(texto) = v4 {
        escutas.extend(ler::ler_proc_tcp(&texto, false));
    }
    if let Ok(texto) = v6 {
        escutas.extend(ler::ler_proc_tcp(&texto, true));
    }
    Ok(escutas)
}

/// O `/etc/passwd` do servidor (uid → nome).
pub fn ler_passwd_por_sftp(fs: &mut dyn Fs) -> io::Result<HashMap<u32, String>> {
    Ok(ler::ler_passwd(&ler_arquivo(fs, "/etc/passwd")?))
}

/// Procura o inode do socket em `/proc/<pid>/fd/*` e devolve o dono. Só enxerga os processos do
/// próprio usuário (ou todos, se for root).
pub fn processo_do_inode(fs: &mut dyn Fs, inode: u64) -> Option<Processo> {
    let alvo = format!("socket:[{inode}]");
    for entrada in fs.list("/proc").ok()? {
        let Ok(pid) = entrada.name.parse::<u32>() else {
            continue;
        };
        let Ok(fds) = fs.list(&format!("/proc/{pid}/fd")) else {
            continue;
        };
        for fd in fds {
            let caminho = format!("/proc/{pid}/fd/{}", fd.name);
            if fs.read_link(&caminho).is_ok_and(|destino| destino == alvo) {
                let nome = ler_arquivo(fs, &format!("/proc/{pid}/comm"))
                    .map(|n| n.trim().to_owned())
                    .unwrap_or_default();
                return Some(Processo {
                    pid: Some(pid),
                    nome,
                });
            }
        }
    }
    None
}

// ----- a fonte -----

/// Quem lê as portas de um servidor. A real é a [`FonteSsh`]; as telas dos testes usam a
/// [`FonteFalsa`].
pub trait FontePortas: Send {
    /// Lê as portas agora. `anterior` é a leitura passada: as sondas já feitas não se repetem.
    fn descobrir(&mut self, anterior: Option<&Descoberta>) -> io::Result<Descoberta>;

    /// Procura o programa dono do socket `inode` (o "Descobrir o processo" do menu).
    fn descobrir_processo(&mut self, inode: u64) -> Option<Processo>;

    /// Muda o que vale a partir da próxima leitura (as Configurações).
    fn configurar(&mut self, cfg: &PortasCfg);

    /// Esquece o que aprendeu do servidor (a sessão reiniciou: pode ter outro `ss`, outro docker).
    fn esquecer(&mut self);
}

/// O que se aprendeu do servidor durante a sessão.
#[derive(Default)]
pub struct EstadoDaSessao {
    pub ss_ausente: bool,
    pub conteineres_ausentes: bool,
    pub passwd: Option<HashMap<u32, String>>,
    /// A sonda de cada porta, por (porta, destino, inode): uma porta que muda de dono é sondada
    /// de novo.
    pub sondas: HashMap<(u16, String, Option<u64>), SondaFeita>,
}

/// O que uma sonda achou, guardado até a porta sumir.
#[derive(Clone, Debug)]
pub struct SondaFeita {
    pub servico: Option<Servico>,
    pub atalhos: Vec<Atalho>,
}

/// A fonte pela conexão SSH de uma sessão.
pub struct FonteSsh {
    canais: Arc<dyn Canais>,
    cfg: PortasCfg,
    estado: EstadoDaSessao,
    fs: Option<Box<dyn Fs>>,
}

impl FonteSsh {
    #[must_use]
    pub fn new(canais: Arc<dyn Canais>, cfg: PortasCfg) -> Self {
        Self {
            canais,
            cfg,
            estado: EstadoDaSessao::default(),
            fs: None,
        }
    }

    /// O SFTP da fonte, aberto na primeira vez e reaberto depois de um erro.
    fn sftp(&mut self) -> io::Result<&mut Box<dyn Fs>> {
        if self.fs.is_none() {
            self.fs = Some(self.canais.sftp()?);
        }
        self.fs.as_mut().ok_or_else(|| io::Error::other("sem SFTP"))
    }

    fn comando(&self, comando: &str) -> io::Result<ExecOutput> {
        self.canais.exec(comando, &[], TEMPO_COMANDO)
    }

    /// As escutas e de onde vieram: `ss`, depois o `/proc`, depois o `netstat`.
    fn escutas(&mut self, avisos: &mut Vec<String>) -> io::Result<(Vec<super::Escuta>, Fonte)> {
        if self.cfg.usar_ferramentas && !self.estado.ss_ausente {
            // Um erro do canal (a sessão caiu) vale para a leitura toda; só o `ss` que roda e
            // falha, ou que não existe, marca a sessão.
            let saida = self.comando("ss -ltnpeH")?;
            if saida.success() {
                let texto = String::from_utf8_lossy(&saida.stdout);
                return Ok((ler::ler_ss(&texto), Fonte::Ss));
            }
            self.estado.ss_ausente = true;
        }
        let proc = self.sftp().and_then(|fs| ler_proc_por_sftp(&mut **fs));
        match proc {
            Ok(escutas) => return Ok((escutas, Fonte::Proc)),
            Err(_) => self.fs = None,
        }
        if self.cfg.usar_ferramentas
            && let Ok(saida) = self.comando("netstat -an")
            && saida.success()
        {
            let texto = String::from_utf8_lossy(&saida.stdout);
            return Ok((ler::ler_netstat(&texto), Fonte::Netstat));
        }
        avisos.push(t!("portas-aviso-sem-leitura"));
        Ok((Vec::new(), Fonte::Nenhuma))
    }

    fn conteineres(&mut self, avisos: &mut Vec<String>) -> Vec<super::Conteiner> {
        if !self.cfg.usar_ferramentas || self.estado.conteineres_ausentes {
            return Vec::new();
        }
        let saida = self
            .canais
            .exec("sh -s", SCRIPT_CONTEINERES.as_bytes(), TEMPO_COMANDO);
        let Ok(saida) = saida else {
            return Vec::new();
        };
        if saida.status == Some(127) {
            self.estado.conteineres_ausentes = true;
            return Vec::new();
        }
        if !saida.success() {
            let erro = String::from_utf8_lossy(&saida.stderr).to_lowercase();
            if erro.contains("permission denied") {
                avisos.push(t!("portas-aviso-sem-docker"));
            }
            return Vec::new();
        }
        ler::ler_docker(&String::from_utf8_lossy(&saida.stdout))
    }

    fn passwd(&mut self) -> HashMap<u32, String> {
        if self.estado.passwd.is_none() {
            let lido = self
                .sftp()
                .and_then(|fs| ler_passwd_por_sftp(&mut **fs))
                .unwrap_or_default();
            self.estado.passwd = Some(lido);
        }
        self.estado.passwd.clone().unwrap_or_default()
    }

    /// Sonda as portas novas, em lotes, e junta o que se achou à lista.
    fn sondar(&mut self, portas: &mut [PortaDoServidor]) {
        let chave = |p: &PortaDoServidor| (p.porta, p.destino.clone(), p.inode);
        let agora: HashSet<_> = portas.iter().map(chave).collect();
        self.estado.sondas.retain(|k, _| agora.contains(k));

        if self.cfg.sondar {
            let novas: Vec<(usize, u16, String)> = portas
                .iter()
                .enumerate()
                .filter(|(_, p)| !self.estado.sondas.contains_key(&chave(p)))
                .filter(|(_, p)| !p.de_sistema())
                .map(|(i, p)| (i, p.porta, p.destino.clone()))
                .collect();
            let tempo = Duration::from_millis(u64::from(self.cfg.sonda_ms));
            let canais = Arc::clone(&self.canais);
            let mut feitas: Vec<(usize, SondaFeita)> = Vec::new();
            for lote in novas.chunks(SONDAS_EM_PARALELO) {
                thread::scope(|escopo| {
                    let tarefas: Vec<_> = lote
                        .iter()
                        .map(|(i, porta, destino)| {
                            let canais = Arc::clone(&canais);
                            let sondar = move || sondar_porta(&*canais, destino, *porta, tempo);
                            (*i, escopo.spawn(sondar))
                        })
                        .collect();
                    for (i, tarefa) in tarefas {
                        if let Ok(feita) = tarefa.join() {
                            feitas.push((i, feita));
                        }
                    }
                });
            }
            for (i, feita) in feitas {
                self.estado.sondas.insert(chave(&portas[i]), feita);
            }
        }

        for porta in portas.iter_mut() {
            if let Some(feita) = self.estado.sondas.get(&chave(porta)) {
                if let Some(servico) = &feita.servico {
                    porta.servico = mesclar(&porta.servico, servico);
                }
                porta.atalhos.clone_from(&feita.atalhos);
            }
        }
    }
}

/// Sonda uma porta e, se é web, procura os apps conhecidos nos caminhos deles.
fn sondar_porta(canais: &dyn Canais, destino: &str, porta: u16, tempo: Duration) -> SondaFeita {
    let Ok(base) = canais.sondar(destino, porta, "/", "", tempo) else {
        return SondaFeita {
            servico: None,
            atalhos: Vec::new(),
        };
    };
    let mut atalhos = Vec::new();
    let e_web = matches!(base.servico, Some(Servico::Web { https: false, .. }));
    if e_web {
        for app in APPS {
            for caminho in app.caminhos {
                let Ok(achado) = canais.sondar(destino, porta, caminho, "", tempo) else {
                    break;
                };
                if let Some(atalho) = achado
                    .resposta
                    .as_ref()
                    .and_then(|r| sonda::atalho(app, caminho, r))
                {
                    atalhos.push(atalho);
                    break;
                }
            }
        }
    }
    SondaFeita {
        servico: base.servico,
        atalhos,
    }
}

/// O que a sonda achou vale mais que o catálogo, exceto quando a sonda não viu nada de novo
/// (o catálogo sabe que a 3306 é MySQL mesmo que o banco só fale depois do login).
fn mesclar(catalogo: &Servico, sonda: &Servico) -> Servico {
    match (catalogo, sonda) {
        (
            Servico::Conhecido {
                tipo: catalogo::Tipo::Banco | catalogo::Tipo::Sistema,
                ..
            },
            Servico::Web { .. },
        ) => catalogo.clone(),
        (Servico::Conhecido { nome, tipo, .. }, Servico::Conhecido { versao, .. }) => {
            Servico::Conhecido {
                nome: nome.clone(),
                tipo: *tipo,
                versao: versao.clone(),
            }
        }
        _ => sonda.clone(),
    }
}

impl FontePortas for FonteSsh {
    fn descobrir(&mut self, _anterior: Option<&Descoberta>) -> io::Result<Descoberta> {
        let mut avisos = Vec::new();
        let (escutas, fonte) = self.escutas(&mut avisos)?;
        if fonte == Fonte::Nenhuma {
            return Ok(Descoberta::vazia(fonte, avisos));
        }
        let conteineres = self.conteineres(&mut avisos);
        let passwd = self.passwd();
        let mut portas = ler::juntar(&escutas, &conteineres, &passwd);
        self.sondar(&mut portas);
        Ok(Descoberta {
            portas,
            conteineres,
            fonte,
            avisos,
        })
    }

    fn descobrir_processo(&mut self, inode: u64) -> Option<Processo> {
        let fs = self.sftp().ok()?;
        processo_do_inode(&mut **fs, inode)
    }

    fn configurar(&mut self, cfg: &PortasCfg) {
        self.cfg = cfg.clone();
    }

    fn esquecer(&mut self) {
        self.estado = EstadoDaSessao::default();
        self.fs = None;
    }
}

/// Uma fonte que responde o que lhe mandaram: para os testes de tela e do trabalhador.
#[doc(hidden)]
#[derive(Clone)]
pub struct FonteFalsa {
    pub respostas: Arc<Mutex<Vec<io::Result<Descoberta>>>>,
    pub leituras: Arc<Mutex<usize>>,
    pub processos: HashMap<u64, Processo>,
}

impl FonteFalsa {
    /// Uma fonte que devolve `respostas` na ordem e repete a última.
    #[must_use]
    pub fn new(respostas: Vec<io::Result<Descoberta>>) -> Self {
        Self {
            respostas: Arc::new(Mutex::new(respostas)),
            leituras: Arc::new(Mutex::new(0)),
            processos: HashMap::new(),
        }
    }
}

impl FontePortas for FonteFalsa {
    fn descobrir(&mut self, _anterior: Option<&Descoberta>) -> io::Result<Descoberta> {
        *self.leituras.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        let mut respostas = self
            .respostas
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if respostas.len() > 1 {
            respostas.remove(0)
        } else {
            match respostas.first() {
                Some(Ok(d)) => Ok(d.clone()),
                Some(Err(e)) => Err(io::Error::new(e.kind(), e.to_string())),
                None => Ok(Descoberta::vazia(Fonte::Nenhuma, Vec::new())),
            }
        }
    }

    fn descobrir_processo(&mut self, inode: u64) -> Option<Processo> {
        self.processos.get(&inode).cloned()
    }

    fn configurar(&mut self, _cfg: &PortasCfg) {}

    fn esquecer(&mut self) {}
}

// ----- o trabalhador -----

/// O que o trabalhador entrega à tela.
#[derive(Clone, Debug)]
pub enum Evento {
    /// Uma leitura nova (ou o motivo de não ter conseguido; a lista antiga fica na tela).
    Lista(Result<Descoberta, String>),
    /// O dono de um socket, achado sob demanda.
    Processo(u64, Option<Processo>),
}

enum Pedido {
    Atualizar,
    Intervalo(Duration),
    Processo(u64),
    Configurar(PortasCfg),
    Esquecer,
    Parar,
}

/// Uma thread por sessão que lê as portas a cada `intervalo` (zero: só quando pedem).
pub struct Trabalhador {
    pedidos: Sender<Pedido>,
    saida: Arc<Mutex<Vec<Evento>>>,
    thread: Option<JoinHandle<()>>,
}

impl Trabalhador {
    /// Sobe a thread. `acordar` roda a cada entrega, de dentro da thread: é por ele que a tela
    /// sabe que há o que recolher.
    pub fn novo(
        mut fonte: Box<dyn FontePortas>,
        intervalo: Duration,
        acordar: impl Fn() + Send + 'static,
    ) -> Self {
        let (pedidos, recebe) = mpsc::channel::<Pedido>();
        let saida: Arc<Mutex<Vec<Evento>>> = Arc::new(Mutex::new(Vec::new()));
        let entrega = Arc::clone(&saida);
        let thread = thread::Builder::new()
            .name("portas".to_owned())
            .spawn(move || {
                let mut intervalo = intervalo;
                let mut anterior: Option<Descoberta> = None;
                let enviar = |evento: Evento| {
                    entrega
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(evento);
                    acordar();
                };
                let ler = |fonte: &mut Box<dyn FontePortas>, anterior: &mut Option<Descoberta>| {
                    match fonte.descobrir(anterior.as_ref()) {
                        Ok(lista) => {
                            *anterior = Some(lista.clone());
                            enviar(Evento::Lista(Ok(lista)));
                        }
                        Err(erro) => enviar(Evento::Lista(Err(erro.to_string()))),
                    }
                };
                loop {
                    let pedido = if intervalo.is_zero() {
                        recebe.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    } else {
                        recebe.recv_timeout(intervalo)
                    };
                    match pedido {
                        Ok(Pedido::Atualizar) | Err(RecvTimeoutError::Timeout) => {
                            ler(&mut fonte, &mut anterior);
                        }
                        Ok(Pedido::Intervalo(novo)) => intervalo = novo,
                        Ok(Pedido::Processo(inode)) => {
                            let dono = fonte.descobrir_processo(inode);
                            enviar(Evento::Processo(inode, dono));
                        }
                        Ok(Pedido::Configurar(cfg)) => fonte.configurar(&cfg),
                        Ok(Pedido::Esquecer) => {
                            fonte.esquecer();
                            anterior = None;
                        }
                        Ok(Pedido::Parar) | Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .ok();
        Self {
            pedidos,
            saida,
            thread,
        }
    }

    /// Lê agora (o botão Atualizar, ou o painel que abriu).
    pub fn atualizar(&self) {
        let _ = self.pedidos.send(Pedido::Atualizar);
    }

    /// Muda o intervalo entre as leituras; zero lê só quando pedem.
    pub fn definir_intervalo(&self, intervalo: Duration) {
        let _ = self.pedidos.send(Pedido::Intervalo(intervalo));
    }

    /// Procura o dono do socket `inode`.
    pub fn processo(&self, inode: u64) {
        let _ = self.pedidos.send(Pedido::Processo(inode));
    }

    /// Passa as Configurações novas à fonte.
    pub fn configurar(&self, cfg: &PortasCfg) {
        let _ = self.pedidos.send(Pedido::Configurar(cfg.clone()));
    }

    /// A sessão reiniciou: a fonte esquece o que aprendeu.
    pub fn esquecer(&self) {
        let _ = self.pedidos.send(Pedido::Esquecer);
    }

    /// Tira da fila o que a thread já entregou.
    #[must_use]
    pub fn recolher(&self) -> Vec<Evento> {
        std::mem::take(&mut *self.saida.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Drop for Trabalhador {
    fn drop(&mut self) {
        let _ = self.pedidos.send(Pedido::Parar);
        // Não se espera a thread: uma leitura em curso pode levar o tempo do comando. Ela sai
        // sozinha ao ver o canal fechado.
        drop(self.thread.take());
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use tomni_ui::files::MemFs;

    /// Canais de mentira: um comando devolve a saída da tabela, o SFTP é um `MemFs` e a sonda
    /// responde o que a tabela manda.
    struct CanaisFalsos {
        comandos: Vec<(&'static str, ExecOutput)>,
        fs: MemFs,
        sondas: HashMap<u16, ResultadoSonda>,
        executados: Mutex<Vec<String>>,
    }

    fn saida(status: u32, stdout: &str, stderr: &str) -> ExecOutput {
        ExecOutput {
            status: Some(status),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    impl Canais for CanaisFalsos {
        fn exec(&self, comando: &str, _entrada: &[u8], _tempo: Duration) -> io::Result<ExecOutput> {
            self.executados.lock().unwrap().push(comando.to_owned());
            let achado = self.comandos.iter().find(|(c, _)| *c == comando);
            Ok(match achado {
                Some((_, s)) => ExecOutput {
                    status: s.status,
                    stdout: s.stdout.clone(),
                    stderr: s.stderr.clone(),
                },
                None => saida(127, "", "not found"),
            })
        }

        fn sftp(&self) -> io::Result<Box<dyn Fs>> {
            Ok(Box::new(self.fs.connect()))
        }

        fn sondar(
            &self,
            _host: &str,
            porta: u16,
            _caminho: &str,
            _host_http: &str,
            _tempo: Duration,
        ) -> io::Result<ResultadoSonda> {
            Ok(self.sondas.get(&porta).cloned().unwrap_or_default())
        }
    }

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1 0000000000000000 100 0 0 10 0
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 11 1 0000000000000000 100 0 0 10 0
";
    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\nana:x:1000:1000::/home/ana:/bin/sh\n";

    fn canais(comandos: Vec<(&'static str, ExecOutput)>) -> CanaisFalsos {
        let fs = MemFs::new();
        fs.put("/proc/net/tcp", TCP);
        fs.put("/etc/passwd", PASSWD);
        CanaisFalsos {
            comandos,
            fs,
            sondas: HashMap::new(),
            executados: Mutex::new(Vec::new()),
        }
    }

    fn fonte(canais: CanaisFalsos, cfg: PortasCfg) -> (FonteSsh, Arc<CanaisFalsos>) {
        let canais = Arc::new(canais);
        (FonteSsh::new(canais.clone(), cfg), canais)
    }

    #[test]
    fn sem_ss_le_o_proc_pelo_sftp_e_acha_o_usuario() {
        let (mut f, _) = fonte(canais(vec![]), PortasCfg::default());
        let d = f.descobrir(None).unwrap();
        assert_eq!(d.fonte, Fonte::Proc);
        let portas: Vec<u16> = d.portas.iter().map(|p| p.porta).collect();
        assert_eq!(portas, vec![22, 3000]);
        let node = d.portas.iter().find(|p| p.porta == 3000).unwrap();
        assert_eq!(node.usuario.as_deref(), Some("ana"));
        assert_eq!(node.inode, Some(4242));
    }

    #[test]
    fn o_ss_que_falha_e_lembrado_na_sessao() {
        let (mut f, c) = fonte(canais(vec![]), PortasCfg::default());
        f.descobrir(None).unwrap();
        f.descobrir(None).unwrap();
        let ss = c
            .executados
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.starts_with("ss"))
            .count();
        assert_eq!(ss, 1);
        f.esquecer();
        f.descobrir(None).unwrap();
        let ss = c
            .executados
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.starts_with("ss"))
            .count();
        assert_eq!(ss, 2);
    }

    #[test]
    fn o_ss_com_sucesso_vence_o_proc() {
        let ss =
            "LISTEN 0 511 127.0.0.1:8080 0.0.0.0:* users:((\"nginx\",pid=7,fd=6)) uid:33 ino:99\n";
        let (mut f, _) = fonte(
            canais(vec![("ss -ltnpeH", saida(0, ss, ""))]),
            PortasCfg::default(),
        );
        let d = f.descobrir(None).unwrap();
        assert_eq!(d.fonte, Fonte::Ss);
        assert_eq!(d.portas.len(), 1);
        assert_eq!(d.portas[0].processo.as_ref().unwrap().nome, "nginx");
    }

    #[test]
    fn sem_ferramentas_so_o_sftp_e_usado() {
        let cfg = PortasCfg {
            usar_ferramentas: false,
            ..PortasCfg::default()
        };
        let (mut f, c) = fonte(canais(vec![]), cfg);
        let d = f.descobrir(None).unwrap();
        assert_eq!(d.fonte, Fonte::Proc);
        assert!(c.executados.lock().unwrap().is_empty());
    }

    #[test]
    fn sem_nenhuma_fonte_o_aviso_explica() {
        let c = CanaisFalsos {
            comandos: vec![],
            fs: MemFs::new(),
            sondas: HashMap::new(),
            executados: Mutex::new(Vec::new()),
        };
        let (mut f, _) = fonte(c, PortasCfg::default());
        let d = f.descobrir(None).unwrap();
        assert_eq!(d.fonte, Fonte::Nenhuma);
        assert_eq!(d.avisos.len(), 1);
    }

    #[test]
    fn a_sonda_so_roda_uma_vez_por_porta() {
        let mut c = canais(vec![]);
        c.sondas.insert(
            3000,
            ResultadoSonda {
                servico: Some(Servico::Web {
                    https: false,
                    titulo: Some("Minha loja".into()),
                    servidor: None,
                    codigo: 200,
                }),
                resposta: None,
            },
        );
        let (mut f, _) = fonte(c, PortasCfg::default());
        let d = f.descobrir(None).unwrap();
        let node = d.portas.iter().find(|p| p.porta == 3000).unwrap();
        assert!(matches!(
            &node.servico,
            Servico::Web { titulo: Some(t), .. } if t == "Minha loja"
        ));
        assert!(
            f.estado
                .sondas
                .contains_key(&(3000, "127.0.0.1".into(), Some(4242)))
        );
    }

    #[test]
    fn a_porta_do_sistema_nao_e_sondada() {
        let mut c = canais(vec![]);
        c.sondas.insert(
            22,
            ResultadoSonda {
                servico: Some(Servico::Desconhecido),
                resposta: None,
            },
        );
        let (mut f, _) = fonte(c, PortasCfg::default());
        f.descobrir(None).unwrap();
        assert!(!f.estado.sondas.keys().any(|(porta, ..)| *porta == 22));
    }

    #[test]
    fn docker_sem_permissao_vira_aviso_e_ausente_vira_nada() {
        let negado = saida(
            1,
            "",
            "permission denied while trying to connect to docker.sock",
        );
        let (mut f, _) = fonte(canais(vec![("sh -s", negado)]), PortasCfg::default());
        let d = f.descobrir(None).unwrap();
        assert_eq!(d.avisos.len(), 1);

        let (mut f, _) = fonte(
            canais(vec![("sh -s", saida(127, "", ""))]),
            PortasCfg::default(),
        );
        let d = f.descobrir(None).unwrap();
        assert!(d.avisos.is_empty());
        assert!(f.estado.conteineres_ausentes);
    }

    #[test]
    fn o_dono_do_socket_sai_do_proc_fd() {
        let fs = MemFs::new();
        fs.put("/proc/812/comm", "node\n");
        let mut conexao = fs.connect();
        // O `MemFs` não tem /proc/<pid>/fd: sem achar, não inventa dono.
        assert_eq!(processo_do_inode(&mut conexao, 4242), None);
    }

    #[test]
    fn a_leitura_do_arquivo_vai_ate_o_fim() {
        let fs = MemFs::new();
        fs.put("/proc/net/tcp", TCP);
        let mut conexao = fs.connect();
        let escutas = ler_proc_por_sftp(&mut conexao).unwrap();
        assert_eq!(escutas.len(), 2);
    }

    #[tokio::test]
    async fn a_sonda_reconhece_a_saudacao_do_ssh() {
        let (mut cliente, mut servidor) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            servidor
                .write_all(b"SSH-2.0-OpenSSH_9.6\r\n")
                .await
                .unwrap();
            let mut resto = Vec::new();
            let _ = servidor.read_to_end(&mut resto).await;
        });
        let r = conversar(&mut cliente, 2222, "/", "", Duration::from_millis(600)).await;
        assert!(matches!(
            r.servico,
            Some(Servico::Conhecido { ref nome, .. }) if nome == "SSH"
        ));
    }

    #[tokio::test]
    async fn a_sonda_pede_o_titulo_de_quem_fala_http() {
        let (mut cliente, mut servidor) = tokio::io::duplex(4096);
        tokio::spawn(async move {
            let mut pedido = [0u8; 512];
            let n = servidor.read(&mut pedido).await.unwrap();
            assert!(String::from_utf8_lossy(&pedido[..n]).starts_with("GET / HTTP/1.1"));
            servidor
                .write_all(b"HTTP/1.1 200 OK\r\nServer: nginx\r\n\r\n<html><head><title>Ol\xc3\xa1</title></head>")
                .await
                .unwrap();
        });
        let r = conversar(&mut cliente, 3000, "/", "", Duration::from_millis(900)).await;
        assert!(matches!(
            r.servico,
            Some(Servico::Web { codigo: 200, titulo: Some(ref t), .. }) if t == "Olá"
        ));
    }

    #[tokio::test]
    async fn a_sonda_nao_manda_get_a_banco_do_catalogo() {
        let (mut cliente, mut servidor) = tokio::io::duplex(1024);
        let leu = tokio::spawn(async move {
            let mut pedido = [0u8; 16];
            tokio::time::timeout(Duration::from_millis(400), servidor.read(&mut pedido))
                .await
                .is_ok_and(|n| n.unwrap_or(0) > 0)
        });
        let r = conversar(&mut cliente, 5432, "/", "", Duration::from_millis(300)).await;
        assert_eq!(r.servico, None);
        assert!(!leu.await.unwrap());
    }

    #[test]
    fn o_trabalhador_entrega_a_lista_e_o_erro_sem_perder_a_anterior() {
        let boa = Descoberta::vazia(Fonte::Ss, vec![]);
        let falsa = FonteFalsa::new(vec![Ok(boa.clone()), Err(io::Error::other("caiu"))]);
        let acordou = Arc::new(Mutex::new(0usize));
        let marca = Arc::clone(&acordou);
        let trabalhador = Trabalhador::novo(Box::new(falsa), Duration::ZERO, move || {
            *marca.lock().unwrap() += 1
        });
        trabalhador.atualizar();
        trabalhador.atualizar();
        let inicio = Instant::now();
        let mut eventos = Vec::new();
        while eventos.len() < 2 && inicio.elapsed() < Duration::from_secs(5) {
            eventos.extend(trabalhador.recolher());
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(eventos.len(), 2);
        assert!(matches!(&eventos[0], Evento::Lista(Ok(d)) if *d == boa));
        assert!(matches!(&eventos[1], Evento::Lista(Err(m)) if m == "caiu"));
        assert_eq!(*acordou.lock().unwrap(), 2);
    }

    #[test]
    fn o_trabalhador_com_intervalo_le_sozinho() {
        let falsa = FonteFalsa::new(vec![Ok(Descoberta::vazia(Fonte::Ss, vec![]))]);
        let leituras = Arc::clone(&falsa.leituras);
        let trabalhador = Trabalhador::novo(Box::new(falsa), Duration::from_millis(20), || {});
        let inicio = Instant::now();
        while *leituras.lock().unwrap() < 3 && inicio.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(*leituras.lock().unwrap() >= 3);
        trabalhador.definir_intervalo(Duration::ZERO);
        drop(trabalhador);
    }
}
