//! Um servidor SSH de teste com SFTP, shell e senha/chave, sobre uma pasta de verdade.
//!
//! Sobe em `127.0.0.1:0`, aceita o usuário "ana" com a senha "senha" ou com a chave `cliente`,
//! e serve o SFTP numa raiz virtual: `/` é a pasta do teste e `/home` é a casa de quem entra.
//! Imita o OpenSSH nas respostas "Failure" (pasta que já existe, pasta cheia, renomear por cima).
//!
//! Também atende `exec` (o canal sem terminal que o M51 usa para `bash`, `buscar` e `glob`):
//! roda o comando no `sh` do computador do teste. Como a raiz virtual é uma pasta de verdade, o
//! servidor troca, no texto do comando, o caminho virtual que vem depois de `cd ` pelo caminho
//! real (`traduzir_comando`). Essa tradução é só do teste: um servidor de verdade já enxerga os
//! caminhos como o app os escreveu.

#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::net::TcpStream;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use russh::keys::ssh_key::LineEnding;
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode, Version,
};

/// O que o servidor e os testes dividem.
#[derive(Default)]
pub struct Estado {
    /// Caminhos (da raiz virtual) que dão "permissão negada" ao abrir ou listar.
    negados: Mutex<HashSet<String>>,
    /// Conexões TCP aceitas.
    aceitas: AtomicUsize,
    /// Autenticações que deram certo.
    autenticadas: AtomicUsize,
    /// Pedidos de subsistema `sftp` atendidos.
    sftps: AtomicUsize,
    /// As conexões abertas, para derrubar.
    abertas: Mutex<Vec<TcpStream>>,
    /// O que o servidor viu no shell, em ordem.
    log: Mutex<Vec<String>>,
    /// Recusa os pedidos `exec` (um servidor que só dá SFTP).
    sem_exec: AtomicBool,
    /// Os comandos que o `exec` recebeu, como chegaram (com os caminhos virtuais).
    comandos: Mutex<Vec<String>>,
    /// Os `exec` que ainda estavam rodando quando o canal fechou (e o servidor matou).
    execs_cancelados: AtomicUsize,
    /// Os `exec` que terminaram sozinhos.
    execs_prontos: AtomicUsize,
    /// O pid do `exec` mais recente que ainda roda (0: nenhum).
    ultimo_pid: AtomicU32,
    /// Os comandos programados (M58): com algum, o `exec` responde só por esta tabela e o que
    /// ficar fora dela dá o código 127, sem rodar o `sh`.
    tabela: Mutex<HashMap<String, (String, u32)>>,
    /// Recusa os túneis `direct-tcpip`.
    nega_direct: AtomicBool,
    /// Os destinos que os túneis pediram (`127.0.0.1:3306`), em ordem.
    tuneis: Mutex<Vec<String>>,
}

pub struct Servidor {
    pub porta: u16,
    pub pasta: PathBuf,
    pub raiz: PathBuf,
    pub cliente: PrivateKey,
    pub publica_servidor: PublicKey,
    pub estado: Arc<Estado>,
}

impl Servidor {
    /// Sobe o servidor com a raiz em `pasta/raiz` (e a casa em `pasta/raiz/home`).
    pub fn novo(pasta: &Path) -> Servidor {
        let raiz = pasta.join("raiz");
        fs::create_dir_all(raiz.join("home")).unwrap();
        let cliente = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let chave = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let publica_servidor = chave.public_key().clone();
        let estado = Arc::new(Estado::default());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = listener.local_addr().unwrap().port();
        let (estado_aceita, raiz_aceita, publica) = (
            Arc::clone(&estado),
            raiz.clone(),
            cliente.public_key().clone(),
        );
        thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                estado_aceita.aceitas.fetch_add(1, Ordering::SeqCst);
                if let Ok(copia) = stream.try_clone() {
                    estado_aceita.abertas.lock().unwrap().push(copia);
                }
                let handler = Ssh {
                    cliente: publica.clone(),
                    raiz: raiz_aceita.clone(),
                    estado: Arc::clone(&estado_aceita),
                    canais: HashMap::new(),
                    shells: HashSet::new(),
                    execs: HashMap::new(),
                };
                let chave = chave.clone();
                thread::spawn(move || {
                    stream.set_nonblocking(true).unwrap();
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    runtime.block_on(async move {
                        let config = Arc::new(server::Config {
                            keys: vec![chave],
                            auth_rejection_time: Duration::ZERO,
                            auth_rejection_time_initial: Some(Duration::ZERO),
                            ..Default::default()
                        });
                        let stream = tokio::net::TcpStream::from_std(stream).unwrap();
                        if let Ok(sessao) = server::run_stream(config, stream, handler).await {
                            let _ = sessao.await;
                        }
                    });
                });
            }
        });
        Servidor {
            porta,
            pasta: pasta.to_owned(),
            raiz,
            cliente,
            publica_servidor,
            estado,
        }
    }

    /// O `known_hosts` do teste, com a chave do servidor já gravada.
    pub fn known_hosts(&self) -> PathBuf {
        let caminho = self.pasta.join("known_hosts");
        if !caminho.exists() {
            russh::keys::known_hosts::learn_known_hosts_path(
                "127.0.0.1",
                self.porta,
                &self.publica_servidor,
                &caminho,
            )
            .unwrap();
        }
        caminho
    }

    /// Grava a chave privada do cliente e devolve o caminho.
    pub fn arquivo_da_chave(&self) -> PathBuf {
        let caminho = self.pasta.join("id_ed25519");
        let pem = self.cliente.to_openssh(LineEnding::LF).unwrap();
        fs::write(&caminho, pem.as_bytes()).unwrap();
        caminho
    }

    /// Um builder que entra com a senha.
    pub fn builder(&self) -> tomni_ui_terminal::ssh::SshBuilder {
        tomni_ui_terminal::ssh::SshBuilder::new("127.0.0.1", self.porta, "ana")
            .known_hosts(self.known_hosts())
            .password("senha")
    }

    /// Sobe o servidor sem `exec`: o canal é recusado e o app cai nas reservas pelo SFTP.
    pub fn sem_exec(pasta: &Path) -> Servidor {
        let servidor = Servidor::novo(pasta);
        servidor.estado.sem_exec.store(true, Ordering::SeqCst);
        servidor
    }

    /// Os comandos que o `exec` recebeu, como o app os escreveu.
    pub fn comandos(&self) -> Vec<String> {
        self.estado.comandos.lock().unwrap().clone()
    }

    /// Programa a resposta de um `exec`: com a tabela em uso, comando fora dela dá 127.
    pub fn programar(&self, comando: &str, saida: &str, codigo: u32) {
        self.estado
            .tabela
            .lock()
            .unwrap()
            .insert(comando.to_owned(), (saida.to_owned(), codigo));
    }

    /// Passa a recusar (ou volta a aceitar) os túneis `direct-tcpip`.
    pub fn negar_direct(&self, negar: bool) {
        self.estado.nega_direct.store(negar, Ordering::SeqCst);
    }

    /// Os destinos que os túneis pediram, como `host:porta`.
    pub fn tuneis(&self) -> Vec<String> {
        self.estado.tuneis.lock().unwrap().clone()
    }

    /// Quantos `exec` foram mortos porque o canal fechou antes do fim.
    pub fn execs_cancelados(&self) -> usize {
        self.estado.execs_cancelados.load(Ordering::SeqCst)
    }

    /// Quantos `exec` terminaram sozinhos.
    pub fn execs_prontos(&self) -> usize {
        self.estado.execs_prontos.load(Ordering::SeqCst)
    }

    /// O arquivo de verdade de um caminho virtual.
    pub fn real(&self, virtual_: &str) -> PathBuf {
        self.raiz.join(virtual_.trim_start_matches('/'))
    }

    /// Grava um arquivo na raiz virtual.
    pub fn escrever(&self, virtual_: &str, dados: &[u8]) {
        let caminho = self.real(virtual_);
        fs::create_dir_all(caminho.parent().unwrap()).unwrap();
        fs::write(caminho, dados).unwrap();
    }

    /// Passa a dar "permissão negada" em `virtual_`.
    pub fn negar(&self, virtual_: &str) {
        self.estado
            .negados
            .lock()
            .unwrap()
            .insert(virtual_.to_owned());
    }

    /// Quantas conexões TCP o servidor aceitou.
    pub fn aceitas(&self) -> usize {
        self.estado.aceitas.load(Ordering::SeqCst)
    }

    /// Quantas autenticações deram certo.
    pub fn autenticadas(&self) -> usize {
        self.estado.autenticadas.load(Ordering::SeqCst)
    }

    /// Quantos pedidos de `sftp` o servidor atendeu.
    pub fn sftps(&self) -> usize {
        self.estado.sftps.load(Ordering::SeqCst)
    }

    /// Corta todas as conexões abertas (o cabo foi puxado).
    pub fn derrubar(&self) {
        for stream in self.estado.abertas.lock().unwrap().drain(..) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }

    /// O que o shell recebeu.
    pub fn log(&self) -> Vec<String> {
        self.estado.log.lock().unwrap().clone()
    }
}

struct Ssh {
    cliente: PublicKey,
    raiz: PathBuf,
    estado: Arc<Estado>,
    canais: HashMap<ChannelId, Channel<Msg>>,
    shells: HashSet<ChannelId>,
    /// Os `exec` em andamento: por onde chega o que o cliente manda para o `stdin` (`None` é o
    /// fim) e se o comando terminou.
    execs: HashMap<ChannelId, ExecEmAndamento>,
}

/// Um `exec` em andamento: a entrada, se terminou e o código de saída.
type ExecEmAndamento = (Sender<Option<Vec<u8>>>, Arc<AtomicBool>, Arc<AtomicU32>);

/// Troca, no texto de um comando, o caminho virtual que vem depois de `cd ` pelo caminho real da
/// pasta do teste. Entende o caminho solto, entre aspas simples e entre aspas simples dentro de
/// outra (`'\''…'\''`), que é como o app escreve o script dentro de `sh -c '…'`.
fn traduzir_comando(comando: &str, raiz: &Path) -> String {
    let raiz = raiz.to_string_lossy();
    let raiz = raiz.trim_end_matches('/');
    let mut saida = String::new();
    let mut resto = comando;
    while let Some(posicao) = resto.find("cd ") {
        let (antes, depois) = resto.split_at(posicao + 3);
        saida.push_str(antes);
        let aspas = ["'\\''", "'"]
            .into_iter()
            .find(|a| depois.starts_with(a))
            .unwrap_or("");
        saida.push_str(aspas);
        let caminho = &depois[aspas.len()..];
        if let Some(sem_barra) = caminho.strip_prefix('/') {
            saida.push_str(raiz);
            saida.push('/');
            resto = sem_barra;
        } else {
            resto = caminho;
        }
    }
    saida.push_str(resto);
    saida
}

impl server::Handler for Ssh {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(if user == "ana" && password == "senha" {
            self.estado.autenticadas.fetch_add(1, Ordering::SeqCst);
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(
            if user == "ana" && key.key_data() == self.cliente.key_data() {
                self.estado.autenticadas.fetch_add(1, Ordering::SeqCst);
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.canais.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shells.insert(channel);
        session.channel_success(channel)?;
        session.data(
            channel,
            "bem-vinda, ana\r\nana@servidor:~$ ".as_bytes().to_vec(),
        )?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some((entrada, _, _)) = self.execs.get(&channel) {
            let _ = entrada.send(Some(data.to_vec()));
            return Ok(());
        }
        if !self.shells.contains(&channel) {
            return Ok(());
        }
        let texto = String::from_utf8_lossy(data).into_owned();
        self.estado.log.lock().unwrap().push(texto.clone());
        if texto.trim() == "exit" {
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            session.close(channel)?;
        } else {
            // Um eco com prompt: o que se digita volta, e a linha de comando termina num `$ `.
            let mut volta = data.to_vec();
            if volta.contains(&b'\r') {
                volta.extend_from_slice(b"\nana@servidor:~$ ");
            }
            session.data(channel, volta)?;
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if self.estado.sem_exec.load(Ordering::SeqCst) {
            session.channel_failure(channel)?;
            return Ok(());
        }
        let comando = String::from_utf8_lossy(data).into_owned();
        self.estado.comandos.lock().unwrap().push(comando.clone());
        session.channel_success(channel)?;
        let programada = {
            let tabela = self.estado.tabela.lock().unwrap();
            (!tabela.is_empty()).then(|| tabela.get(&comando).cloned())
        };
        if let Some(resposta) = programada {
            let (saida, codigo) = resposta.unwrap_or_else(|| (String::new(), 127));
            let handle = session.handle();
            tokio::spawn(async move {
                let _ = handle.data(channel, saida.into_bytes()).await;
                let _ = handle.exit_status_request(channel, codigo).await;
                let _ = handle.eof(channel).await;
                let _ = handle.close(channel).await;
            });
            return Ok(());
        }
        let traduzido = traduzir_comando(&comando, &self.raiz);
        let (entrada, recebe) = channel_de_entrada();
        let pronto = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicU32::new(0));
        self.execs
            .insert(channel, (entrada, Arc::clone(&pronto), Arc::clone(&pid)));
        let handle = session.handle();
        let estado = Arc::clone(&self.estado);
        tokio::spawn(async move {
            let rodou = tokio::task::spawn_blocking(move || {
                rodar(&traduzido, recebe, &estado, &pronto, &pid)
            })
            .await;
            let (saida, erros, codigo) = rodou.unwrap_or_else(|_| (Vec::new(), Vec::new(), 255));
            let _ = handle.data(channel, saida).await;
            if !erros.is_empty() {
                let _ = handle.extended_data(channel, 1, erros).await;
            }
            let _ = handle.exit_status_request(channel, codigo).await;
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        canal: Channel<Msg>,
        host: &str,
        porta: u32,
        _: &str,
        _: u32,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        self.estado
            .tuneis
            .lock()
            .unwrap()
            .push(format!("{host}:{porta}"));
        if self.estado.nega_direct.load(Ordering::SeqCst) {
            reply
                .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                .await;
            return Ok(());
        }
        let Ok(porta) = u16::try_from(porta) else {
            return Ok(());
        };
        let Ok(mut destino) = tokio::net::TcpStream::connect((host, porta)).await else {
            reply.reject(russh::ChannelOpenFailure::ConnectFailed).await;
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            let mut canal = canal.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut canal, &mut destino).await;
        });
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some((entrada, _, _)) = self.execs.get(&channel) {
            let _ = entrada.send(None);
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some((entrada, pronto, pid)) = self.execs.remove(&channel) {
            let _ = entrada.send(None);
            let pid = pid.load(Ordering::SeqCst);
            if !pronto.load(Ordering::SeqCst) && pid != 0 {
                self.estado.execs_cancelados.fetch_add(1, Ordering::SeqCst);
                // O comando roda sob `sh -c`: mata o grupo inteiro. Sem o `--`, o
                // kill do procps lê `-1234` como a opção `-1` e mata tudo do usuário.
                let _ = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{pid}")])
                    .status();
            }
        }
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != "sftp" {
            session.channel_failure(channel)?;
            return Ok(());
        }
        let Some(canal) = self.canais.remove(&channel) else {
            session.channel_failure(channel)?;
            return Ok(());
        };
        self.estado.sftps.fetch_add(1, Ordering::SeqCst);
        session.channel_success(channel)?;
        let handler = Sftp {
            raiz: self.raiz.clone(),
            estado: Arc::clone(&self.estado),
            handles: HashMap::new(),
            proximo: 0,
        };
        russh_sftp::server::run(canal.into_stream(), handler).await;
        Ok(())
    }
}

/// As duas pontas da entrada de um `exec`.
type EntradaDoExec = (
    Sender<Option<Vec<u8>>>,
    std::sync::mpsc::Receiver<Option<Vec<u8>>>,
);

fn channel_de_entrada() -> EntradaDoExec {
    channel()
}

/// Roda `comando` no `sh`, num grupo de processos próprio (para o `kill` do cancelamento pegar
/// os filhos também), com o `stdin` alimentado por `entrada`.
fn rodar(
    comando: &str,
    entrada: std::sync::mpsc::Receiver<Option<Vec<u8>>>,
    estado: &Estado,
    pronto: &AtomicBool,
    pid: &AtomicU32,
) -> (Vec<u8>, Vec<u8>, u32) {
    use std::os::unix::process::CommandExt;
    let filho = Command::new("sh")
        .arg("-c")
        .arg(comando)
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut filho) = filho else {
        pronto.store(true, Ordering::SeqCst);
        return (Vec::new(), b"sh: nao deu para rodar\n".to_vec(), 127);
    };
    pid.store(filho.id(), Ordering::SeqCst);
    estado.ultimo_pid.store(filho.id(), Ordering::SeqCst);
    let mut stdin = filho.stdin.take();
    std::thread::spawn(move || {
        while let Ok(Some(dados)) = entrada.recv() {
            let Some(pipe) = stdin.as_mut() else { break };
            if pipe.write_all(&dados).is_err() {
                break;
            }
        }
        drop(stdin);
    });
    let resultado = filho.wait_with_output();
    pronto.store(true, Ordering::SeqCst);
    match resultado {
        Ok(saida) => {
            if saida.status.code().is_some() {
                estado.execs_prontos.fetch_add(1, Ordering::SeqCst);
            }
            (
                saida.stdout,
                saida.stderr,
                saida.status.code().unwrap_or(255) as u32,
            )
        }
        Err(_) => (Vec::new(), Vec::new(), 255),
    }
}

enum Aberto {
    Arquivo(fs::File),
    Pasta { restantes: Vec<File>, enviada: bool },
}

struct Sftp {
    raiz: PathBuf,
    estado: Arc<Estado>,
    handles: HashMap<String, Aberto>,
    proximo: u32,
}

fn status(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

/// O que o OpenSSH responde: `NoSuchFile`, `PermissionDenied` e `Failure` para o resto.
fn codigo(error: &std::io::Error) -> StatusCode {
    match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        _ => StatusCode::Failure,
    }
}

fn atributos(meta: &fs::Metadata) -> FileAttributes {
    FileAttributes {
        size: Some(meta.len()),
        uid: Some(meta.uid()),
        user: None,
        gid: Some(meta.gid()),
        group: None,
        permissions: Some(meta.mode()),
        atime: Some(meta.atime() as u32),
        mtime: Some(meta.mtime() as u32),
    }
}

impl Sftp {
    /// Normaliza um caminho virtual (`.` é a casa).
    fn virtual_(&self, caminho: &str) -> String {
        let completo = if caminho == "." || caminho.is_empty() {
            "/home".to_owned()
        } else if caminho.starts_with('/') {
            caminho.to_owned()
        } else {
            format!("/home/{caminho}")
        };
        let mut partes: Vec<&str> = Vec::new();
        for parte in completo.split('/') {
            match parte {
                "" | "." => {}
                ".." => {
                    partes.pop();
                }
                outra => partes.push(outra),
            }
        }
        format!("/{}", partes.join("/"))
    }

    fn real(&self, caminho: &str) -> PathBuf {
        self.raiz
            .join(self.virtual_(caminho).trim_start_matches('/'))
    }

    fn negado(&self, caminho: &str) -> bool {
        self.estado
            .negados
            .lock()
            .unwrap()
            .contains(&self.virtual_(caminho))
    }

    fn novo_handle(&mut self, aberto: Aberto) -> String {
        self.proximo += 1;
        let handle = format!("h{}", self.proximo);
        self.handles.insert(handle.clone(), aberto);
        handle
    }
}

impl russh_sftp::server::Handler for Sftp {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn init(&mut self, _: u32, _: HashMap<String, String>) -> Result<Version, Self::Error> {
        Ok(Version::new())
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let mut virtual_ = self.virtual_(&path);
        // Como o OpenSSH, resolve os links: um caminho que existe volta sem eles. O que sai da
        // raiz do teste volta com o caminho de verdade.
        if let (Ok(real), Ok(base)) = (
            fs::canonicalize(self.real(&path)),
            fs::canonicalize(&self.raiz),
        ) {
            virtual_ = match real.strip_prefix(&base) {
                Ok(dentro) => format!("/{}", dentro.to_string_lossy()),
                Err(_) => real.to_string_lossy().into_owned(),
            };
        }
        Ok(Name {
            id,
            files: vec![File::dummy(virtual_)],
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = fs::metadata(self.real(&path)).map_err(|e| codigo(&e))?;
        Ok(Attrs {
            id,
            attrs: atributos(&meta),
        })
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, Self::Error> {
        let meta = fs::symlink_metadata(self.real(&path)).map_err(|e| codigo(&e))?;
        Ok(Attrs {
            id,
            attrs: atributos(&meta),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, Self::Error> {
        match self.handles.get(&handle) {
            Some(Aberto::Arquivo(arquivo)) => Ok(Attrs {
                id,
                attrs: atributos(&arquivo.metadata().map_err(|e| codigo(&e))?),
            }),
            _ => Err(StatusCode::Failure),
        }
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, Self::Error> {
        if self.negado(&path) {
            return Err(StatusCode::PermissionDenied);
        }
        let mut restantes = Vec::new();
        for item in fs::read_dir(self.real(&path)).map_err(|e| codigo(&e))? {
            let item = item.map_err(|e| codigo(&e))?;
            let Ok(meta) = fs::symlink_metadata(item.path()) else {
                continue;
            };
            restantes.push(File::new(
                item.file_name().to_string_lossy().into_owned(),
                atributos(&meta),
            ));
        }
        let handle = self.novo_handle(Aberto::Pasta {
            restantes,
            enviada: false,
        });
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, Self::Error> {
        match self.handles.get_mut(&handle) {
            Some(Aberto::Pasta { restantes, enviada }) => {
                if *enviada {
                    return Err(StatusCode::Eof);
                }
                *enviada = true;
                Ok(Name {
                    id,
                    files: std::mem::take(restantes),
                })
            }
            _ => Err(StatusCode::Failure),
        }
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        pflags: OpenFlags,
        _: FileAttributes,
    ) -> Result<Handle, Self::Error> {
        if self.negado(&filename) {
            return Err(StatusCode::PermissionDenied);
        }
        let mut opcoes = fs::OpenOptions::new();
        opcoes
            .read(pflags.contains(OpenFlags::READ))
            .write(pflags.contains(OpenFlags::WRITE))
            .create(pflags.contains(OpenFlags::CREATE))
            .truncate(pflags.contains(OpenFlags::TRUNCATE));
        let arquivo = opcoes.open(self.real(&filename)).map_err(|e| codigo(&e))?;
        let handle = self.novo_handle(Aberto::Arquivo(arquivo));
        Ok(Handle { id, handle })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, Self::Error> {
        self.handles.remove(&handle);
        Ok(status(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, Self::Error> {
        let Some(Aberto::Arquivo(arquivo)) = self.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        let mut buffer = vec![0u8; len as usize];
        let lidos = arquivo
            .read_at(&mut buffer, offset)
            .map_err(|e| codigo(&e))?;
        if lidos == 0 {
            return Err(StatusCode::Eof);
        }
        buffer.truncate(lidos);
        Ok(Data { id, data: buffer })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, Self::Error> {
        let Some(Aberto::Arquivo(arquivo)) = self.handles.get(&handle) else {
            return Err(StatusCode::Failure);
        };
        arquivo
            .write_all_at(&data, offset)
            .map_err(|e| codigo(&e))?;
        Ok(status(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, Self::Error> {
        fs::remove_file(self.real(&filename)).map_err(|e| codigo(&e))?;
        Ok(status(id))
    }

    async fn mkdir(
        &mut self,
        id: u32,
        path: String,
        _: FileAttributes,
    ) -> Result<Status, Self::Error> {
        fs::create_dir(self.real(&path)).map_err(|e| codigo(&e))?;
        Ok(status(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, Self::Error> {
        fs::remove_dir(self.real(&path)).map_err(|e| codigo(&e))?;
        Ok(status(id))
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, Self::Error> {
        let destino = self.real(&newpath);
        // O SFTP 3 do OpenSSH não troca o destino.
        if fs::symlink_metadata(&destino).is_ok() {
            return Err(StatusCode::Failure);
        }
        fs::rename(self.real(&oldpath), destino).map_err(|e| codigo(&e))?;
        Ok(status(id))
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, Self::Error> {
        use std::os::unix::fs::PermissionsExt;
        let real = self.real(&path);
        if let Some(modo) = attrs.permissions {
            fs::set_permissions(&real, fs::Permissions::from_mode(modo & 0o7777))
                .map_err(|e| codigo(&e))?;
        }
        if let Some(mtime) = attrs.mtime {
            let tempo = std::time::UNIX_EPOCH + Duration::from_secs(u64::from(mtime));
            let arquivo = fs::File::open(&real).map_err(|e| codigo(&e))?;
            arquivo.set_modified(tempo).map_err(|e| codigo(&e))?;
        }
        Ok(status(id))
    }

    async fn readlink(&mut self, id: u32, path: String) -> Result<Name, Self::Error> {
        let alvo = fs::read_link(self.real(&path)).map_err(|e| codigo(&e))?;
        Ok(Name {
            id,
            files: vec![File::dummy(alvo.to_string_lossy().into_owned())],
        })
    }

    async fn symlink(
        &mut self,
        id: u32,
        linkpath: String,
        targetpath: String,
    ) -> Result<Status, Self::Error> {
        // Como o OpenSSH: o primeiro campo do pacote é o alvo, o segundo é o link.
        std::os::unix::fs::symlink(&linkpath, self.real(&targetpath)).map_err(|e| codigo(&e))?;
        Ok(status(id))
    }
}
