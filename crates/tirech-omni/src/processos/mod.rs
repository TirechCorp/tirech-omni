//! Os processos de uma máquina, para as abas Processos e Disco da página Desempenho: este
//! computador ou o servidor do outro lado de uma conexão SSH.
//!
//! Uma [`FonteDeProcessos`] tira uma [`Foto`] (o que cada processo gastou até agora); o
//! [`Comparador`] compara com a foto de antes e devolve a [`Leitura`], com a CPU em % da máquina
//! e o disco por segundo de cada um. O [`Leitor`] faz isso numa thread, a cada `ritmo`, e
//! encerra processos quando a tela pede.
//!
//! As fontes: [`local::FonteLocal`] (o `/proc` no Linux, o Win32 no Windows e o `libproc` no
//! macOS) e [`remoto::FonteRemota`] (um script que lê o `/proc` do servidor por um canal
//! `exec`). Os leitores do `/proc` são os mesmos nas duas, em [`proc_linux`].

pub mod local;
pub mod proc_linux;
pub mod remoto;

use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::t;

/// De quanto em quanto tempo o leitor lê no app de verdade.
pub const RITMO: Duration = Duration::from_secs(2);

/// Quantas leituras os gráficos guardam: o último minuto no [`RITMO`].
pub const PONTOS: usize = 30;

/// O estado de um processo, nos nomes do `/proc` (o macOS e o Windows dizem menos).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Estado {
    Executando,
    Dormindo,
    /// Esperando o disco (o `D`, que não se interrompe).
    Disco,
    Parado,
    Zumbi,
    Ocioso,
    Desconhecido,
}

impl Estado {
    /// O estado da letra do `/proc/<pid>/stat`.
    pub fn da_letra(letra: char) -> Self {
        match letra {
            'R' => Estado::Executando,
            'S' => Estado::Dormindo,
            'D' => Estado::Disco,
            'T' | 't' => Estado::Parado,
            'Z' | 'X' | 'x' => Estado::Zumbi,
            'I' => Estado::Ocioso,
            _ => Estado::Desconhecido,
        }
    }

    /// O nome na tabela.
    pub fn texto(self) -> String {
        match self {
            Estado::Executando => t!("processos-estado-executando"),
            Estado::Dormindo => t!("processos-estado-dormindo"),
            Estado::Disco => t!("processos-estado-disco"),
            Estado::Parado => t!("processos-estado-parado"),
            Estado::Zumbi => t!("processos-estado-zumbi"),
            Estado::Ocioso => t!("processos-estado-ocioso"),
            Estado::Desconhecido => t!("processos-estado-desconhecido"),
        }
    }
}

/// Um processo numa foto. O que o sistema não diz (ou não deixa ler, como o disco dos
/// processos de outro usuário sem ser root) fica `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Processo {
    pub pid: u32,
    pub pai: u32,
    /// Quando o processo começou, na unidade da fonte. Só serve para, junto com o pid, saber se
    /// é o mesmo processo da foto de antes (os pids se reaproveitam).
    pub inicio: u64,
    pub nome: String,
    /// A linha de comando; vazia quando não dá para ler.
    pub comando: String,
    pub usuario: String,
    pub estado: Option<Estado>,
    pub threads: Option<u32>,
    /// O tempo de CPU até agora (usuário e sistema).
    pub cpu: Option<Duration>,
    /// A memória residente, em bytes.
    pub memoria: Option<u64>,
    /// Os bytes lidos e escritos até agora.
    pub disco: Option<(u64, u64)>,
}

/// Uma foto da máquina: os processos e o relógio de quando foi tirada.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Foto {
    /// Um relógio da máquina que só anda para a frente (o `uptime` no Linux).
    pub relogio: Duration,
    pub nucleos: usize,
    pub memoria_total: Option<u64>,
    pub memoria_disponivel: Option<u64>,
    pub processos: Vec<Processo>,
}

/// Um processo com o que gastou desde a foto de antes.
#[derive(Clone, Debug, PartialEq)]
pub struct Linha {
    pub processo: Processo,
    /// A CPU em % da máquina inteira; `None` na primeira foto ou sem o tempo de CPU.
    pub cpu: Option<f64>,
    /// Bytes lidos do disco por segundo.
    pub leitura: Option<f64>,
    /// Bytes escritos no disco por segundo.
    pub escrita: Option<f64>,
}

/// O que a tela mostra: as linhas e os números da máquina.
#[derive(Clone, Debug, PartialEq)]
pub struct Leitura {
    pub linhas: Vec<Linha>,
    pub nucleos: usize,
    pub memoria_total: Option<u64>,
    pub memoria_disponivel: Option<u64>,
}

impl Leitura {
    /// A CPU somada de todos os processos, em % da máquina.
    pub fn cpu(&self) -> f64 {
        self.linhas
            .iter()
            .filter_map(|l| l.cpu)
            .sum::<f64>()
            .min(100.0)
    }

    /// A memória em uso: a da máquina quando a fonte diz, senão a soma dos processos.
    pub fn memoria_em_uso(&self) -> u64 {
        match (self.memoria_total, self.memoria_disponivel) {
            (Some(total), Some(livre)) => total.saturating_sub(livre),
            _ => self.linhas.iter().filter_map(|l| l.processo.memoria).sum(),
        }
    }

    /// O disco da máquina por segundo: a leitura e a escrita somadas.
    pub fn disco(&self) -> (f64, f64) {
        self.linhas.iter().fold((0.0, 0.0), |(l, e), linha| {
            (
                l + linha.leitura.unwrap_or(0.0),
                e + linha.escrita.unwrap_or(0.0),
            )
        })
    }
}

/// Os processos de uma foto pelo pid e pelo início (o pid se repete depois que um acaba).
type PorProcesso = HashMap<(u32, u64), Processo>;

/// Compara cada foto com a de antes.
#[derive(Default)]
pub struct Comparador {
    anterior: Option<(Duration, PorProcesso)>,
}

impl Comparador {
    pub fn novo() -> Self {
        Self::default()
    }

    /// A leitura de `foto`. Um processo que não estava na foto de antes (ou um pid
    /// reaproveitado) não tem taxas ainda.
    pub fn comparar(&mut self, foto: Foto) -> Leitura {
        let Foto {
            relogio,
            nucleos,
            memoria_total,
            memoria_disponivel,
            processos,
        } = foto;
        let nucleos = nucleos.max(1);
        let anterior = self.anterior.take();
        let intervalo = anterior
            .as_ref()
            .map(|(antes, _)| relogio.saturating_sub(*antes).as_secs_f64())
            .filter(|s| *s > 0.0);
        let linhas = processos
            .iter()
            .map(|p| {
                let antes = anterior
                    .as_ref()
                    .and_then(|(_, mapa)| mapa.get(&(p.pid, p.inicio)));
                let (cpu, leitura, escrita) = match (antes, intervalo) {
                    (Some(antes), Some(segundos)) => {
                        let cpu = p.cpu.zip(antes.cpu).map(|(agora, antes)| {
                            let gasto = agora.saturating_sub(antes).as_secs_f64();
                            (gasto / segundos / nucleos as f64 * 100.0).clamp(0.0, 100.0)
                        });
                        let disco = p.disco.zip(antes.disco).map(|(agora, antes)| {
                            (
                                agora.0.saturating_sub(antes.0) as f64 / segundos,
                                agora.1.saturating_sub(antes.1) as f64 / segundos,
                            )
                        });
                        (cpu, disco.map(|d| d.0), disco.map(|d| d.1))
                    }
                    _ => (None, None, None),
                };
                Linha {
                    processo: p.clone(),
                    cpu,
                    leitura,
                    escrita,
                }
            })
            .collect();
        self.anterior = Some((
            relogio,
            processos
                .into_iter()
                .map(|p| ((p.pid, p.inicio), p))
                .collect(),
        ));
        Leitura {
            linhas,
            nucleos,
            memoria_total,
            memoria_disponivel,
        }
    }
}

/// De qual máquina a fonte lê.
#[derive(Clone)]
pub enum Maquina {
    /// Este computador.
    Local,
    /// O servidor de uma conexão SSH, pelo abridor de canais da sessão (que a tela renova).
    Remota(crate::portas::descobrir::AbridorCompartilhado),
}

/// A fonte de verdade de cada máquina.
pub fn fonte_do_sistema(maquina: Maquina) -> Box<dyn FonteDeProcessos> {
    match maquina {
        Maquina::Local => Box::new(local::FonteLocal::nova()),
        Maquina::Remota(abridor) => Box::new(remoto::FonteRemota::nova(Box::new(
            crate::portas::descobrir::CanaisDaSessao(abridor),
        ))),
    }
}

/// De onde vêm as fotos e quem encerra os processos. Roda na thread do [`Leitor`].
pub trait FonteDeProcessos: Send {
    /// Tira uma foto agora.
    fn ler(&mut self) -> Result<Foto, String>;
    /// Pede ao processo `pid` que termine; `forcar` mata sem dar a chance de terminar.
    fn encerrar(&mut self, pid: u32, forcar: bool) -> Result<(), String>;
}

/// O que o leitor entrega à tela.
#[derive(Clone, Debug, PartialEq)]
pub enum Evento {
    Leitura(Result<Leitura, String>),
    /// O fim de um pedido de encerrar: o erro do primeiro que falhou, se algum falhou.
    Encerrado(Result<(), String>),
}

enum Pedido {
    Encerrar(Vec<u32>, bool),
    Parar,
}

/// Uma thread que tira uma foto a cada `ritmo` e entrega as leituras.
pub struct Leitor {
    pedidos: Sender<Pedido>,
    saida: Arc<Mutex<Vec<Evento>>>,
    thread: Option<JoinHandle<()>>,
}

impl Leitor {
    /// Sobe a thread. A primeira leitura já vem com as taxas: a thread tira uma foto, espera um
    /// pouco e tira outra. `acordar` roda a cada entrega, de dentro da thread.
    pub fn novo(
        mut fonte: Box<dyn FonteDeProcessos>,
        ritmo: Duration,
        acordar: impl Fn() + Send + 'static,
    ) -> Self {
        let (pedidos, recebe) = mpsc::channel::<Pedido>();
        let saida: Arc<Mutex<Vec<Evento>>> = Arc::default();
        let entrega = Arc::clone(&saida);
        let thread = thread::Builder::new()
            .name("processos".to_owned())
            .spawn(move || {
                let enviar = |evento: Evento| {
                    entrega
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(evento);
                    acordar();
                };
                let mut comparador = Comparador::novo();
                let mut ler =
                    |fonte: &mut Box<dyn FonteDeProcessos>, entregar: bool| match fonte.ler() {
                        Ok(foto) => {
                            let leitura = comparador.comparar(foto);
                            if entregar {
                                enviar(Evento::Leitura(Ok(leitura)));
                            }
                        }
                        Err(erro) => enviar(Evento::Leitura(Err(erro))),
                    };
                ler(&mut fonte, false);
                let mut espera = ritmo.min(Duration::from_millis(500));
                loop {
                    match recebe.recv_timeout(espera) {
                        Err(RecvTimeoutError::Timeout) => {
                            ler(&mut fonte, true);
                            espera = ritmo;
                        }
                        Ok(Pedido::Encerrar(pids, forcar)) => {
                            let mut resultado = Ok(());
                            for pid in pids {
                                if let Err(erro) = fonte.encerrar(pid, forcar) {
                                    resultado = resultado.and(Err(erro));
                                }
                            }
                            enviar(Evento::Encerrado(resultado));
                            ler(&mut fonte, true);
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

    /// Encerra os processos `pids` (`forcar`: na marra).
    pub fn encerrar(&self, pids: Vec<u32>, forcar: bool) {
        let _ = self.pedidos.send(Pedido::Encerrar(pids, forcar));
    }

    /// Tira da fila o que a thread já entregou.
    #[must_use]
    pub fn recolher(&self) -> Vec<Evento> {
        std::mem::take(&mut *self.saida.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl Drop for Leitor {
    fn drop(&mut self) {
        let _ = self.pedidos.send(Pedido::Parar);
        // Não se espera a thread: uma leitura remota pode levar o tempo do comando. Ela sai
        // sozinha ao ver o canal fechado.
        drop(self.thread.take());
    }
}

/// A fonte dos testes: a foto de número `n` (contando do zero) é a que `fotos(n)` devolve, e os
/// pedidos de encerrar ficam anotados.
#[derive(Clone)]
pub struct FonteDeTeste {
    fotos: Arc<dyn Fn(u64) -> Result<Foto, String> + Send + Sync>,
    lidas: Arc<Mutex<u64>>,
    /// Os pedidos de encerrar: o pid e se foi forçado.
    pub encerrados: Arc<Mutex<Vec<(u32, bool)>>>,
}

impl FonteDeTeste {
    pub fn nova(fotos: impl Fn(u64) -> Result<Foto, String> + Send + Sync + 'static) -> Self {
        Self {
            fotos: Arc::new(fotos),
            lidas: Arc::default(),
            encerrados: Arc::default(),
        }
    }

    /// Três processos que gastam sempre o mesmo, um segundo por foto: `init` (pid 1), o
    /// `sshd` (filho do 1) e o `rsync` que ele roda, que escreve 2 MB/s.
    pub fn exemplo() -> Self {
        Self::nova(|n| Ok(foto_de_exemplo(n)))
    }

    /// Os pedidos de encerrar até agora.
    pub fn pedidos(&self) -> Vec<(u32, bool)> {
        self.encerrados
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// A foto `n` da [`FonteDeTeste::exemplo`].
pub fn foto_de_exemplo(n: u64) -> Foto {
    let segundos = Duration::from_secs(n);
    let processo = |pid, pai, nome: &str, comando: &str, usuario: &str| Processo {
        pid,
        pai,
        inicio: u64::from(pid),
        nome: nome.to_owned(),
        comando: comando.to_owned(),
        usuario: usuario.to_owned(),
        estado: Some(Estado::Dormindo),
        threads: Some(1),
        cpu: Some(Duration::ZERO),
        memoria: Some(8 * 1024 * 1024),
        disco: Some((0, 0)),
    };
    let mut init = processo(1, 0, "init", "/sbin/init", "root");
    init.cpu = Some(segundos / 100);
    let mut sshd = processo(400, 1, "sshd", "/usr/sbin/sshd -D", "root");
    sshd.cpu = Some(segundos / 20);
    sshd.memoria = Some(24 * 1024 * 1024);
    let mut rsync = processo(900, 400, "rsync", "rsync -a /dados /copia", "ana");
    rsync.estado = Some(Estado::Executando);
    rsync.threads = Some(3);
    rsync.cpu = Some(segundos / 2);
    rsync.memoria = Some(120 * 1024 * 1024);
    rsync.disco = Some((n * 512 * 1024, n * 2 * 1024 * 1024));
    Foto {
        relogio: segundos,
        nucleos: 4,
        memoria_total: Some(8 * 1024 * 1024 * 1024),
        memoria_disponivel: Some(6 * 1024 * 1024 * 1024),
        processos: vec![init, sshd, rsync],
    }
}

impl FonteDeProcessos for FonteDeTeste {
    fn ler(&mut self) -> Result<Foto, String> {
        let n = {
            let mut lidas = self.lidas.lock().unwrap_or_else(PoisonError::into_inner);
            *lidas += 1;
            *lidas - 1
        };
        (self.fotos)(n)
    }

    fn encerrar(&mut self, pid: u32, forcar: bool) -> Result<(), String> {
        self.encerrados
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((pid, forcar));
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn foto(relogio_ms: u64, processos: Vec<Processo>) -> Foto {
        Foto {
            relogio: Duration::from_millis(relogio_ms),
            nucleos: 4,
            memoria_total: None,
            memoria_disponivel: None,
            processos,
        }
    }

    fn processo(pid: u32, inicio: u64, cpu_ms: u64, disco: (u64, u64)) -> Processo {
        Processo {
            pid,
            inicio,
            cpu: Some(Duration::from_millis(cpu_ms)),
            disco: Some(disco),
            ..Processo::default()
        }
    }

    #[test]
    fn a_primeira_foto_nao_tem_taxas() {
        let mut c = Comparador::novo();
        let l = c.comparar(foto(0, vec![processo(1, 1, 500, (0, 0))]));
        assert_eq!(l.linhas[0].cpu, None);
        assert_eq!(l.linhas[0].escrita, None);
    }

    #[test]
    fn a_cpu_e_da_maquina_inteira_e_o_disco_por_segundo() {
        let mut c = Comparador::novo();
        c.comparar(foto(1000, vec![processo(7, 3, 0, (0, 0))]));
        // Dois segundos depois: 2 s de CPU nos 4 núcleos (25%), 1 MB lido e 4 MB escritos.
        let l = c.comparar(foto(
            3000,
            vec![processo(7, 3, 2000, (1024 * 1024, 4 * 1024 * 1024))],
        ));
        assert_eq!(l.linhas[0].cpu, Some(25.0));
        assert_eq!(l.linhas[0].leitura, Some(512.0 * 1024.0));
        assert_eq!(l.linhas[0].escrita, Some(2.0 * 1024.0 * 1024.0));
        assert_eq!(l.cpu(), 25.0);
        assert_eq!(l.disco(), (512.0 * 1024.0, 2.0 * 1024.0 * 1024.0));
    }

    #[test]
    fn um_pid_reaproveitado_comeca_do_zero() {
        let mut c = Comparador::novo();
        c.comparar(foto(0, vec![processo(7, 3, 9000, (0, 0))]));
        let l = c.comparar(foto(1000, vec![processo(7, 50, 100, (0, 0))]));
        assert_eq!(l.linhas[0].cpu, None);
    }

    #[test]
    fn a_memoria_em_uso_vem_da_maquina_ou_da_soma() {
        let mut p = processo(1, 1, 0, (0, 0));
        p.memoria = Some(100);
        let mut l = Comparador::novo().comparar(foto(0, vec![p.clone(), p]));
        assert_eq!(l.memoria_em_uso(), 200);
        l.memoria_total = Some(1000);
        l.memoria_disponivel = Some(300);
        assert_eq!(l.memoria_em_uso(), 700);
    }

    #[test]
    fn o_leitor_entrega_com_taxas_e_encerra() {
        let fonte = FonteDeTeste::exemplo();
        let leitor = Leitor::novo(Box::new(fonte.clone()), Duration::from_millis(20), || {});
        let esperar = |leitor: &Leitor, procura: &dyn Fn(&Evento) -> bool| {
            let limite = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(e) = leitor.recolher().into_iter().find(|e| procura(e)) {
                    return e;
                }
                assert!(std::time::Instant::now() < limite, "o leitor não entregou");
                thread::sleep(Duration::from_millis(5));
            }
        };
        let Evento::Leitura(Ok(leitura)) = esperar(&leitor, &|e| matches!(e, Evento::Leitura(_)))
        else {
            panic!("a leitura falhou");
        };
        let rsync = leitura
            .linhas
            .iter()
            .find(|l| l.processo.pid == 900)
            .expect("o rsync");
        assert_eq!(rsync.cpu, Some(12.5));
        assert_eq!(rsync.escrita, Some(2.0 * 1024.0 * 1024.0));
        leitor.encerrar(vec![900, 400], true);
        assert_eq!(
            esperar(&leitor, &|e| matches!(e, Evento::Encerrado(_))),
            Evento::Encerrado(Ok(()))
        );
        assert_eq!(fonte.pedidos(), vec![(900, true), (400, true)]);
    }
}
