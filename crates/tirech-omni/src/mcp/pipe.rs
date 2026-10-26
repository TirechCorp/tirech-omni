//! O pipe com nome do Windows (M51, seção 5.2): o canal entre a ponte e o app onde o Unix usa um
//! socket.
//!
//! O app cria o pipe com dono (`tomni_ui_win32::private_pipe`): só o próprio usuário conecta, e
//! o servidor lê o pid de quem conectou (`named_pipe_client_pid`) para conferir com o que a ponte
//! declara. Os pipes do `tokio` são assíncronos, e um pipe síncrono não serve (uma leitura
//! parada trava a escrita no mesmo handle). Por isso cada [`Fluxo`] é uma ponte entre o código
//! síncrono do canal (`Read` e `Write`, em threads) e duas tarefas do `tokio` que levam os bytes
//! de e para o pipe, num runtime de uma thread só.

use std::collections::VecDeque;
use std::future::Future;
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::{mpsc as tokio_mpsc, watch};

use super::servidor::ErroDoServidor;
use crate::t;

/// O que a leitura síncrona já recebeu e ainda não entregou.
struct Entrada {
    chegando: mpsc::Receiver<Vec<u8>>,
    resto: VecDeque<u8>,
    espera: Option<Duration>,
}

struct Compartilhado {
    entrada: Mutex<Entrada>,
    saindo: tokio_mpsc::UnboundedSender<Vec<u8>>,
    fim: watch::Sender<bool>,
}

/// A conexão com o outro lado: o pipe, vista como um fluxo síncrono. As cópias
/// ([`Fluxo::try_clone`]) falam com o mesmo pipe; a conexão acaba quando a última é solta ou
/// quando alguém chama [`Fluxo::shutdown`].
pub struct Fluxo {
    dentro: Arc<Compartilhado>,
}

impl std::fmt::Debug for Fluxo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Fluxo")
    }
}

impl Fluxo {
    /// Faz o fluxo para `pipe`. A segunda parte são as tarefas que levam os bytes, que alguém
    /// precisa rodar no runtime onde o pipe nasceu.
    fn novo<P>(pipe: P) -> (Self, impl Future<Output = ()>)
    where
        P: AsyncRead + AsyncWrite + Send + 'static,
    {
        let (chega_tx, chega_rx) = mpsc::channel::<Vec<u8>>();
        let (sai_tx, mut sai_rx) = tokio_mpsc::unbounded_channel::<Vec<u8>>();
        let (fim_tx, mut fim_rx) = watch::channel(false);
        let mut fim_escrita = fim_tx.subscribe();
        let (mut leitura, mut escrita) = tokio::io::split(pipe);
        let tarefas = async move {
            let ler = async move {
                let mut buffer = [0u8; 8192];
                loop {
                    tokio::select! {
                        lido = leitura.read(&mut buffer) => match lido {
                            Ok(0) | Err(_) => return,
                            Ok(n) => {
                                if chega_tx.send(buffer[..n].to_vec()).is_err() {
                                    return;
                                }
                            }
                        },
                        () = ate_o_fim(&mut fim_rx) => return,
                    }
                }
            };
            let escrever = async move {
                loop {
                    tokio::select! {
                        dados = sai_rx.recv() => match dados {
                            Some(dados) => {
                                if escrita.write_all(&dados).await.is_err() {
                                    return;
                                }
                            }
                            None => return,
                        },
                        () = ate_o_fim(&mut fim_escrita) => return,
                    }
                }
            };
            // Quando um lado acaba (o outro fechou, ou o fluxo foi solto), o outro não tem mais
            // para quem falar: as duas metades do pipe fecham juntas.
            tokio::select! {
                () = ler => {}
                () = escrever => {}
            }
        };
        let fluxo = Self {
            dentro: Arc::new(Compartilhado {
                entrada: Mutex::new(Entrada {
                    chegando: chega_rx,
                    resto: VecDeque::new(),
                    espera: None,
                }),
                saindo: sai_tx,
                fim: fim_tx,
            }),
        };
        (fluxo, tarefas)
    }

    /// Uma cópia que fala com o mesmo pipe.
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            dentro: Arc::clone(&self.dentro),
        })
    }

    /// Fecha a conexão: quem lê recebe o fim do fluxo e quem escreve, um erro.
    pub fn shutdown(&self, _: std::net::Shutdown) -> io::Result<()> {
        self.dentro.fim.send_replace(true);
        Ok(())
    }

    /// A espera máxima de uma leitura (`None` espera para sempre).
    pub fn set_read_timeout(&self, espera: Option<Duration>) -> io::Result<()> {
        self.dentro
            .entrada
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .espera = espera;
        Ok(())
    }
}

impl Read for Fluxo {
    fn read(&mut self, destino: &mut [u8]) -> io::Result<usize> {
        if destino.is_empty() {
            return Ok(0);
        }
        let mut entrada = self
            .dentro
            .entrada
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if entrada.resto.is_empty() {
            let recebido = match entrada.espera {
                Some(espera) => entrada.chegando.recv_timeout(espera),
                None => entrada
                    .chegando
                    .recv()
                    .map_err(|_| RecvTimeoutError::Disconnected),
            };
            match recebido {
                Ok(dados) => entrada.resto.extend(dados),
                Err(RecvTimeoutError::Timeout) => return Err(io::ErrorKind::TimedOut.into()),
                Err(RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = destino.len().min(entrada.resto.len());
        for (lugar, byte) in destino.iter_mut().zip(entrada.resto.drain(..n)) {
            *lugar = byte;
        }
        Ok(n)
    }
}

impl Write for Fluxo {
    fn write(&mut self, dados: &[u8]) -> io::Result<usize> {
        self.dentro
            .saindo
            .send(dados.to_vec())
            .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
        Ok(dados.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Espera o fluxo ser fechado (ou solto).
async fn ate_o_fim(fim: &mut watch::Receiver<bool>) {
    let _ = fim.wait_for(|fechado| *fechado).await;
}

/// O runtime de uma thread só onde os pipes vivem.
fn runtime() -> io::Result<Runtime> {
    Builder::new_current_thread().enable_io().build()
}

/// Abre o pipe `nome` como cliente (a ponte). Um pipe ocupado (`ERROR_PIPE_BUSY`, o app está
/// atendendo outra ponte) é tentado de novo por um instante.
pub fn conectar(endereco: &Path) -> io::Result<Fluxo> {
    const PIPE_OCUPADO: i32 = 231;
    let nome = endereco.as_os_str().to_owned();
    let (pronto_tx, pronto_rx) = mpsc::channel::<io::Result<Fluxo>>();
    thread::spawn(move || {
        let rt = match runtime() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = pronto_tx.send(Err(e));
                return;
            }
        };
        let aberto = {
            let _dentro = rt.enter();
            let mut tentativas = 0;
            loop {
                match ClientOptions::new().open(&nome) {
                    Err(e) if e.raw_os_error() == Some(PIPE_OCUPADO) && tentativas < 40 => {
                        tentativas += 1;
                        thread::sleep(Duration::from_millis(50));
                    }
                    outro => break outro,
                }
            }
        };
        match aberto {
            Ok(cliente) => {
                let (fluxo, tarefas) = Fluxo::novo(cliente);
                if pronto_tx.send(Ok(fluxo)).is_ok() {
                    rt.block_on(tarefas);
                }
            }
            Err(e) => {
                let _ = pronto_tx.send(Err(e));
            }
        }
    });
    pronto_rx
        .recv()
        .unwrap_or_else(|_| Err(io::ErrorKind::BrokenPipe.into()))
}

/// O pipe do servidor, aberto e esperando a primeira ponte.
pub struct Escuta {
    runtime: Runtime,
    nome: String,
    proximo: NamedPipeServer,
}

impl Escuta {
    /// Cria o pipe `endereco`, com dono e só do usuário. Se o nome já existe, há outro app.
    pub fn abrir(endereco: &Path) -> Result<Self, ErroDoServidor> {
        let Some(nome) = endereco.to_str() else {
            return Err(ErroDoServidor::Falha(t!(
                "agente-endereco-invalido",
                endereco = endereco.display().to_string()
            )));
        };
        let runtime = runtime().map_err(|e| ErroDoServidor::Falha(e.to_string()))?;
        let proximo = {
            let _dentro = runtime.enter();
            tomni_ui_win32::private_pipe(nome, true)
        }
        .map_err(|e| {
            if e.kind() == io::ErrorKind::PermissionDenied {
                ErroDoServidor::OutroApp
            } else {
                ErroDoServidor::Falha(e.to_string())
            }
        })?;
        Ok(Self {
            runtime,
            nome: nome.to_owned(),
            proximo,
        })
    }

    /// Aceita pontes até `parar` devolver `true`. Cada uma chega com o pid que o Windows leu do
    /// pipe (`None` se não deu para ler) e o fluxo; `atender` roda numa thread.
    pub fn aceitar(
        self,
        parar: impl Fn() -> bool,
        atender: impl Fn(Fluxo, Option<u32>) + Send + Sync + 'static,
    ) {
        let Self {
            runtime,
            nome,
            mut proximo,
        } = self;
        let atender = Arc::new(atender);
        runtime.block_on(async {
            loop {
                let ligou = proximo.connect().await;
                if parar() {
                    return;
                }
                if ligou.is_err() {
                    continue;
                }
                // Já na vez da próxima ponte, antes de atender esta.
                let Ok(seguinte) = tomni_ui_win32::private_pipe(&nome, false) else {
                    return;
                };
                let conexao = std::mem::replace(&mut proximo, seguinte);
                let pid = tomni_ui_win32::named_pipe_client_pid(&conexao).ok();
                let (fluxo, tarefas) = Fluxo::novo(conexao);
                tokio::spawn(tarefas);
                let atender = Arc::clone(&atender);
                thread::spawn(move || atender(fluxo, pid));
            }
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_ponte_e_o_servidor_se_falam_pelo_pipe() {
        let nome = format!(r"\\.\pipe\tirech-omni-teste-{}", std::process::id());
        let escuta = Escuta::abrir(Path::new(&nome)).expect("abre o pipe");
        // O mesmo nome, de novo: há outro dono.
        assert_eq!(
            Escuta::abrir(Path::new(&nome)).err(),
            Some(ErroDoServidor::OutroApp)
        );
        let (visto_tx, visto_rx) = mpsc::channel();
        let parar = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let parar_la = Arc::clone(&parar);
        thread::spawn(move || {
            escuta.aceitar(
                || parar_la.load(std::sync::atomic::Ordering::SeqCst),
                move |mut fluxo, pid| {
                    let mut dados = [0u8; 5];
                    fluxo.read_exact(&mut dados).unwrap();
                    fluxo.write_all(&dados).unwrap();
                    let _ = visto_tx.send(pid);
                },
            );
        });
        let mut ponte = conectar(Path::new(&nome)).expect("conecta");
        ponte.write_all(b"ola!!").unwrap();
        let mut volta = [0u8; 5];
        ponte.read_exact(&mut volta).unwrap();
        assert_eq!(&volta, b"ola!!");
        // O pid que o servidor leu do pipe é o desta ponte.
        assert_eq!(
            visto_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Some(std::process::id())
        );
        // O fim: quem lê recebe o fim do fluxo.
        ponte.shutdown(std::net::Shutdown::Both).unwrap();
        assert_eq!(ponte.read(&mut volta).unwrap(), 0);
        parar.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = conectar(Path::new(&nome));
    }
}
