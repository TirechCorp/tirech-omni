//! A VPN embutida dividida entre as cópias do app abertas na mesma pasta de dados. Só uma cópia
//! pode ter o motor de um perfil ligado (a trava do perfil, em [`crate::redes::TravasDeRede`]):
//! dois motores com o mesmo usuário derrubam um ao outro no servidor. A secundária não liga o
//! dela: pede à principal pelo canal ([`super`]) e sai pela ponte do motor da principal.
//!
//! - **O pedido:** a secundária liga, manda [`PedidoDeRede`] e lê as linhas de [`Linha`]: a
//!   ponte (o SOCKS5 em `127.0.0.1`, com a credencial dela; o socket é só do usuário), os avisos
//!   do motor subindo e, por fim, `pronta` ou `recusada`.
//! - **O empréstimo:** a ligação fica aberta enquanto a secundária usa a rede. A principal conta o
//!   perfil como em uso até ela fechar, e o motor não para por falta de aba. Fechou (a secundária
//!   soltou o [`Emprestimo`] ou saiu), a principal esquece o uso.
//! - **Sem principal:** se ninguém atende o canal, a secundária liga o motor ela mesma (a trava
//!   do perfil está livre). Se a principal fecha, a ponte some com ela; a próxima conexão da
//!   secundária pede de novo e, sem principal, liga o próprio motor.

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tomni_ui::files::Live;
use tomni_ui::terminal::Proxy;

use super::{ESPERA_DA_PRINCIPAL, Resposta, VERSAO};
use crate::mcp::canal::{self, Fluxo};
use crate::redes::RedeResolvida;
use crate::t;

/// A espera da secundária pela ponte. Ela espera na thread da tela, então é curta: a principal
/// responde entre um quadro e outro.
const ESPERA_DA_PONTE: Duration = Duration::from_secs(5);

/// O pedido de rede, da secundária para a principal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedidoDeRede {
    /// A versão do canal ([`VERSAO`]).
    pub instancias: u64,
    /// O id do perfil de rede.
    pub rede: u64,
}

/// O que a principal manda à secundária, uma linha por vez.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rede", rename_all = "snake_case")]
pub enum Linha {
    /// A ponte do motor: o SOCKS5 por onde a secundária sai.
    Ponte {
        /// O endereço (`127.0.0.1`).
        host: String,
        /// A porta.
        port: u16,
        /// O usuário da ponte.
        user: Option<String>,
        /// A senha da ponte.
        password: Option<String>,
    },
    /// Um aviso do motor subindo (o mesmo que a aba da principal mostraria).
    Aviso {
        /// O texto, para a pessoa.
        texto: String,
    },
    /// O motor conectou.
    Pronta,
    /// O motor não subiu, ou a principal não empresta.
    Recusada {
        /// O motivo, para a pessoa.
        erro: String,
    },
}

// O `Debug` não mostra a senha da ponte.
impl std::fmt::Debug for Linha {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ponte { host, port, .. } => write!(f, "Ponte({host}:{port})"),
            Self::Aviso { texto } => write!(f, "Aviso({texto})"),
            Self::Pronta => write!(f, "Pronta"),
            Self::Recusada { erro } => write!(f, "Recusada({erro})"),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A principal.

/// Um pedido de rede de uma secundária, a caminho da tela da principal.
pub enum ChamadoDeRede {
    /// Ligar (ou achar ligado) o motor do perfil `rede` e contar o uso `emprestimo`.
    Pedir {
        /// O número do empréstimo, para o [`ChamadoDeRede::Soltar`].
        emprestimo: u64,
        /// O id do perfil.
        rede: u64,
        /// A rede resolvida (com o motor), ou por que não.
        responder: Sender<Result<RedeResolvida, String>>,
    },
    /// A secundária fechou: o uso `0` acabou.
    Soltar(u64),
}

/// A fila dos pedidos de rede para a tela da principal.
pub type FilaDeRedes = Live<Vec<ChamadoDeRede>>;

/// Atende um pedido de rede numa ligação já lida: pede o motor à tela, manda a ponte, espera o
/// motor conectar (com os avisos) e segura o uso até a secundária fechar.
pub(super) fn atender(
    pedido: &PedidoDeRede,
    leitura: BufReader<Fluxo>,
    escrita: Fluxo,
    fila: &FilaDeRedes,
) {
    static PROXIMO: AtomicU64 = AtomicU64::new(1);
    let emprestimo = PROXIMO.fetch_add(1, Ordering::SeqCst);
    let (responder, resposta) = mpsc::channel();
    fila.update(|f| {
        f.push(ChamadoDeRede::Pedir {
            emprestimo,
            rede: pedido.rede,
            responder,
        });
    });
    let resultado = resposta
        .recv_timeout(ESPERA_DA_PRINCIPAL)
        .unwrap_or_else(|_| Err(t!("instancias-sem-resposta")));
    segurar(resultado, leitura, escrita);
    // A tela pode ter contado o uso mesmo quando a ligação caiu no meio.
    fila.update(|f| f.push(ChamadoDeRede::Soltar(emprestimo)));
}

fn segurar(
    resultado: Result<RedeResolvida, String>,
    mut leitura: BufReader<Fluxo>,
    mut escrita: Fluxo,
) {
    let mut mandar =
        |linha: &Linha| writeln!(escrita, "{}", json!(linha)).and_then(|()| escrita.flush());
    let rede = match resultado {
        Ok(rede) => rede,
        Err(erro) => {
            let _ = mandar(&Linha::Recusada { erro });
            return;
        }
    };
    let Some(Proxy::Socks5 {
        host,
        port,
        user,
        password,
    }) = rede.proxy()
    else {
        let _ = mandar(&Linha::Recusada {
            erro: t!("instancias-rede-sem-ponte"),
        });
        return;
    };
    if mandar(&Linha::Ponte {
        host,
        port,
        user,
        password,
    })
    .is_err()
    {
        return;
    }
    let mut caiu = false;
    let pronta = rede.preparar(&mut |aviso| {
        caiu |= mandar(&Linha::Aviso {
            texto: aviso.to_owned(),
        })
        .is_err();
    });
    let fim = match pronta {
        Ok(()) => Linha::Pronta,
        Err(erro) => Linha::Recusada { erro },
    };
    if caiu || mandar(&fim).is_err() || fim != Linha::Pronta {
        return;
    }
    // O empréstimo: espera a secundária fechar (ou sair).
    let _ = leitura.get_ref().set_read_timeout(None);
    let mut resto = Vec::new();
    while leitura.read_until(b'\n', &mut resto).unwrap_or(0) > 0 {
        resto.clear();
    }
}

// ---------------------------------------------------------------------------------------------
// A secundária.

/// Uma rede emprestada pela principal. A ligação fica aberta (e a principal conta o uso) até o
/// último dono soltar este valor.
pub struct Emprestimo {
    /// A ponte do motor da principal.
    pub proxy: Proxy,
    fluxo: Fluxo,
    estado: Arc<(Mutex<Estado>, Condvar)>,
}

#[derive(Default)]
struct Estado {
    avisos: Vec<String>,
    fim: Option<Result<(), String>>,
}

impl std::fmt::Debug for Emprestimo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.proxy {
            Proxy::Socks5 { host, port, .. } | Proxy::Http { host, port, .. } => {
                write!(f, "Emprestimo({host}:{port})")
            }
        }
    }
}

// Um empréstimo é igual só a ele mesmo (a ligação é uma só), como os motores.
impl PartialEq for Emprestimo {
    fn eq(&self, outro: &Self) -> bool {
        std::ptr::eq(self, outro)
    }
}

impl Eq for Emprestimo {}

impl Emprestimo {
    /// Pede à principal que atende `endereco` a rede `rede`. `Err` se ninguém atende (a
    /// secundária liga o próprio motor); `Ok(Err)` se a principal recusou, com o motivo.
    ///
    /// # Errors
    ///
    /// O erro da ligação, quando não há principal.
    pub fn pedir(endereco: &Path, rede: u64) -> io::Result<Result<Arc<Self>, String>> {
        let mut fluxo = canal::conectar(endereco)?;
        Ok(Self::pedir_em(&mut fluxo, rede).map(|(proxy, leitura)| {
            let estado = Arc::new((Mutex::new(Estado::default()), Condvar::new()));
            let acompanha = Arc::clone(&estado);
            thread::spawn(move || acompanhar(leitura, &acompanha));
            Arc::new(Self {
                proxy,
                fluxo,
                estado,
            })
        }))
    }

    fn pedir_em(fluxo: &mut Fluxo, rede: u64) -> Result<(Proxy, BufReader<Fluxo>), String> {
        let sem_resposta = |_| t!("instancias-sem-resposta");
        let pedido = PedidoDeRede {
            instancias: VERSAO,
            rede,
        };
        fluxo
            .set_read_timeout(Some(ESPERA_DA_PONTE))
            .map_err(sem_resposta)?;
        writeln!(fluxo, "{}", json!(pedido))
            .and_then(|()| fluxo.flush())
            .map_err(sem_resposta)?;
        let mut leitura = BufReader::new(fluxo.try_clone().map_err(sem_resposta)?);
        let mut linha = String::new();
        leitura.read_line(&mut linha).map_err(sem_resposta)?;
        match serde_json::from_str::<Linha>(&linha) {
            Ok(Linha::Ponte {
                host,
                port,
                user,
                password,
            }) => {
                // O motor da principal pode levar o tempo dele (e perguntar algo lá).
                let _ = fluxo.set_read_timeout(None);
                Ok((
                    Proxy::Socks5 {
                        host,
                        port,
                        user,
                        password,
                    },
                    leitura,
                ))
            }
            Ok(Linha::Recusada { erro }) => Err(erro),
            // Uma principal de antes do empréstimo recusa o pedido como de outra versão.
            _ => Err(match serde_json::from_str::<Resposta>(&linha) {
                Ok(Resposta::Recusado { erro }) => erro,
                _ => t!("instancias-outra-versao"),
            }),
        }
    }

    /// Espera o motor da principal conectar, passando os avisos dele para `aviso`.
    ///
    /// # Errors
    ///
    /// Por que a rede não ficou pronta (na principal, ou porque ela fechou).
    pub fn esperar(&self, aviso: &mut dyn FnMut(&str)) -> Result<(), String> {
        let (estado, mudou) = &*self.estado;
        let mut vistos = 0;
        let mut e = estado.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            for texto in &e.avisos[vistos..] {
                aviso(texto);
            }
            vistos = e.avisos.len();
            if let Some(fim) = &e.fim {
                return fim.clone();
            }
            e = mudou.wait(e).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Se a rede ainda vale: a principal não recusou nem fechou.
    pub fn valendo(&self) -> bool {
        let (estado, _) = &*self.estado;
        !matches!(
            estado.lock().unwrap_or_else(PoisonError::into_inner).fim,
            Some(Err(_))
        )
    }
}

impl Drop for Emprestimo {
    fn drop(&mut self) {
        // A principal lê o fim da ligação e esquece o uso; a thread que acompanha também sai.
        let _ = self.fluxo.shutdown(std::net::Shutdown::Both);
    }
}

/// Lê o que a principal manda depois da ponte, até a ligação fechar.
fn acompanhar(mut leitura: BufReader<Fluxo>, estado: &(Mutex<Estado>, Condvar)) {
    let (estado, mudou) = estado;
    let mut linha = String::new();
    loop {
        linha.clear();
        let lido = leitura.read_line(&mut linha);
        let mut e = estado.lock().unwrap_or_else(PoisonError::into_inner);
        match lido
            .ok()
            .filter(|n| *n > 0)
            .map(|_| serde_json::from_str::<Linha>(&linha))
        {
            Some(Ok(Linha::Aviso { texto })) => e.avisos.push(texto),
            Some(Ok(Linha::Pronta)) => e.fim = Some(Ok(())),
            Some(Ok(Linha::Recusada { erro })) => {
                e.fim = Some(Err(erro));
                mudou.notify_all();
                return;
            }
            Some(_) => {}
            None => {
                e.fim = Some(Err(t!("instancias-rede-principal-fechou")));
                mudou.notify_all();
                return;
            }
        }
        mudou.notify_all();
    }
}
