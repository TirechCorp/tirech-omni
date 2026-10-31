//! Os processos de uma máquina, para as abas Processos, Disco e Rede da página Desempenho: este
//! computador ou o servidor do outro lado de uma conexão SSH.
//!
//! Uma [`FonteDeProcessos`] tira uma [`Foto`] (o que cada processo, cada núcleo, cada disco e
//! cada interface de rede gastou até agora); o [`Comparador`] compara com a foto de antes e
//! devolve a [`Leitura`], com a CPU em % da máquina, o disco por segundo de cada processo, o uso
//! de cada núcleo, as operações por segundo de cada disco e o tráfego de cada interface. O [`Leitor`] faz isso numa thread, a cada `ritmo`, e
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
    /// As operações de leitura e de escrita até agora (só o Windows conta por processo).
    pub operacoes: Option<(u64, u64)>,
}

/// O tempo de um núcleo lógico em cada modo, desde o boot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TemposDeCpu {
    pub usuario: Duration,
    /// No kernel (com as interrupções, no Linux).
    pub sistema: Duration,
    pub ocioso: Duration,
}

/// Uma interface de rede e os bytes que passaram por ela até agora.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interface {
    pub nome: String,
    pub recebidos: u64,
    pub enviados: u64,
    /// Se é uma placa de verdade, não uma VPN, ponte ou a interface de um contêiner.
    pub fisica: bool,
}

/// Um disco e o que passou por ele até agora.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Disco {
    pub nome: String,
    /// As operações de leitura e de escrita.
    pub leituras: u64,
    pub escritas: u64,
    /// Os bytes lidos e escritos.
    pub lidos: u64,
    pub escritos: u64,
    /// Se é um disco de verdade, não um volume lógico, um RAID por software ou o contêiner do
    /// APFS, que repetem o tráfego dos discos por baixo deles.
    pub fisico: bool,
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
    /// Os tempos de cada núcleo; vazio quando a fonte não diz.
    pub cpus: Vec<TemposDeCpu>,
    /// As interfaces ligadas, sem a de loopback.
    pub interfaces: Vec<Interface>,
    /// Os discos inteiros (sem as partições); vazio quando a fonte não diz.
    pub discos: Vec<Disco>,
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
    /// As operações de leitura e de escrita por segundo, onde a fonte conta por processo.
    pub operacoes: Option<(f64, f64)>,
}

/// O uso de um núcleo desde a foto de antes, em %.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsoDoNucleo {
    pub total: f64,
    /// A parte do kernel.
    pub sistema: f64,
}

/// Uma interface de rede com o tráfego desde a foto de antes.
#[derive(Clone, Debug, PartialEq)]
pub struct UsoDaInterface {
    pub nome: String,
    pub fisica: bool,
    /// Os bytes recebidos e enviados até agora.
    pub recebidos: u64,
    pub enviados: u64,
    /// Bytes por segundo; `None` na primeira foto em que a interface aparece.
    pub recebendo: Option<f64>,
    pub enviando: Option<f64>,
}

/// Um disco com as operações e os bytes desde a foto de antes.
#[derive(Clone, Debug, PartialEq)]
pub struct UsoDoDisco {
    pub nome: String,
    pub fisico: bool,
    /// Os bytes lidos e escritos até agora.
    pub lidos: u64,
    pub escritos: u64,
    /// As operações por segundo; `None` na primeira foto em que o disco aparece.
    pub leituras: Option<f64>,
    pub escritas: Option<f64>,
    /// Os bytes por segundo, como nas operações.
    pub lendo: Option<f64>,
    pub escrevendo: Option<f64>,
}

/// O que a tela mostra: as linhas e os números da máquina.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Leitura {
    pub linhas: Vec<Linha>,
    pub nucleos: usize,
    pub memoria_total: Option<u64>,
    pub memoria_disponivel: Option<u64>,
    /// O uso de cada núcleo; vazio sem os tempos da fonte ou na primeira foto.
    pub nucleos_uso: Vec<UsoDoNucleo>,
    /// As interfaces que já passaram algum byte.
    pub interfaces: Vec<UsoDaInterface>,
    /// Os discos que já fizeram alguma operação.
    pub discos: Vec<UsoDoDisco>,
}

impl Leitura {
    /// A CPU da máquina em %: a média dos núcleos quando a fonte dá os tempos deles, senão a
    /// soma dos processos.
    pub fn cpu(&self) -> f64 {
        if !self.nucleos_uso.is_empty() {
            return media(self.nucleos_uso.iter().map(|n| n.total));
        }
        self.linhas
            .iter()
            .filter_map(|l| l.cpu)
            .sum::<f64>()
            .min(100.0)
    }

    /// A parte do kernel na CPU da máquina, em %; `None` sem os tempos dos núcleos.
    pub fn cpu_sistema(&self) -> Option<f64> {
        (!self.nucleos_uso.is_empty()).then(|| media(self.nucleos_uso.iter().map(|n| n.sistema)))
    }

    /// As interfaces que contam no total da máquina: as físicas, ou todas se não houver
    /// nenhuma física (um contêiner, uma VM só com a ponte).
    pub fn interfaces_do_total(&self) -> impl Iterator<Item = &UsoDaInterface> {
        let alguma_fisica = self.interfaces.iter().any(|i| i.fisica);
        self.interfaces
            .iter()
            .filter(move |i| i.fisica || !alguma_fisica)
    }

    /// A rede da máquina por segundo: o recebido e o enviado, somados das
    /// [`interfaces_do_total`](Self::interfaces_do_total).
    pub fn rede(&self) -> (f64, f64) {
        self.interfaces_do_total().fold((0.0, 0.0), |(r, e), i| {
            (
                r + i.recebendo.unwrap_or(0.0),
                e + i.enviando.unwrap_or(0.0),
            )
        })
    }

    /// O que a máquina já recebeu e enviou, pelas mesmas interfaces de [`rede`](Self::rede).
    pub fn rede_total(&self) -> (u64, u64) {
        self.interfaces_do_total()
            .fold((0, 0), |(r, e), i| (r + i.recebidos, e + i.enviados))
    }

    /// A memória em uso: a da máquina quando a fonte diz, senão a soma dos processos.
    pub fn memoria_em_uso(&self) -> u64 {
        match (self.memoria_total, self.memoria_disponivel) {
            (Some(total), Some(livre)) => total.saturating_sub(livre),
            _ => self.linhas.iter().filter_map(|l| l.processo.memoria).sum(),
        }
    }

    /// Os discos que contam no total da máquina: os físicos, ou todos se não houver nenhum
    /// físico (uma VM só com o volume lógico).
    pub fn discos_do_total(&self) -> impl Iterator<Item = &UsoDoDisco> {
        let algum_fisico = self.discos.iter().any(|d| d.fisico);
        self.discos
            .iter()
            .filter(move |d| d.fisico || !algum_fisico)
    }

    /// Se a fonte disse as operações dos discos (e já há duas fotos para comparar).
    fn discos_com_taxa(&self) -> bool {
        self.discos.iter().any(|d| d.leituras.is_some())
    }

    /// O disco da máquina por segundo, a leitura e a escrita: o que passou pelos
    /// [`discos_do_total`](Self::discos_do_total) quando a fonte diz, senão a soma dos processos
    /// (que deixa de fora os de outros usuários quando não se é root).
    pub fn disco(&self) -> (f64, f64) {
        if self.discos_com_taxa() {
            return self.discos_do_total().fold((0.0, 0.0), |(l, e), d| {
                (l + d.lendo.unwrap_or(0.0), e + d.escrevendo.unwrap_or(0.0))
            });
        }
        self.linhas.iter().fold((0.0, 0.0), |(l, e), linha| {
            (
                l + linha.leitura.unwrap_or(0.0),
                e + linha.escrita.unwrap_or(0.0),
            )
        })
    }

    /// As operações por segundo da máquina (o IOPS), as leituras e as escritas, somadas dos
    /// [`discos_do_total`](Self::discos_do_total); `None` quando a fonte não diz.
    pub fn operacoes(&self) -> Option<(f64, f64)> {
        self.discos_com_taxa().then(|| {
            self.discos_do_total().fold((0.0, 0.0), |(l, e), d| {
                (l + d.leituras.unwrap_or(0.0), e + d.escritas.unwrap_or(0.0))
            })
        })
    }
}

/// A média de valores em %, sem passar de 100.
fn media(valores: impl ExactSizeIterator<Item = f64>) -> f64 {
    let quantos = valores.len().max(1) as f64;
    (valores.sum::<f64>() / quantos).clamp(0.0, 100.0)
}

/// Os processos de uma foto pelo pid e pelo início (o pid se repete depois que um acaba).
type PorProcesso = HashMap<(u32, u64), Processo>;

/// O que o comparador guarda da foto de antes.
struct Anterior {
    relogio: Duration,
    processos: PorProcesso,
    cpus: Vec<TemposDeCpu>,
    /// Os bytes recebidos e enviados pelo nome da interface.
    interfaces: HashMap<String, (u64, u64)>,
    /// Os contadores de cada disco pelo nome.
    discos: HashMap<String, Disco>,
}

/// Compara cada foto com a de antes.
#[derive(Default)]
pub struct Comparador {
    anterior: Option<Anterior>,
}

/// O uso de cada núcleo entre duas fotos, pelos tempos deles (que já dizem quanto tempo passou).
/// Se o número de núcleos mudou, não há uso.
fn uso_dos_nucleos(antes: &[TemposDeCpu], agora: &[TemposDeCpu]) -> Vec<UsoDoNucleo> {
    if antes.len() != agora.len() {
        return Vec::new();
    }
    antes
        .iter()
        .zip(agora)
        .map(|(antes, agora)| {
            let delta = |a: Duration, b: Duration| b.saturating_sub(a).as_secs_f64();
            let usuario = delta(antes.usuario, agora.usuario);
            let sistema = delta(antes.sistema, agora.sistema);
            let total = usuario + sistema + delta(antes.ocioso, agora.ocioso);
            if total <= 0.0 {
                return UsoDoNucleo::default();
            }
            UsoDoNucleo {
                total: ((usuario + sistema) / total * 100.0).clamp(0.0, 100.0),
                sistema: (sistema / total * 100.0).clamp(0.0, 100.0),
            }
        })
        .collect()
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
            cpus,
            interfaces,
            discos,
        } = foto;
        let nucleos = nucleos.max(1);
        let anterior = self.anterior.take();
        let intervalo = anterior
            .as_ref()
            .map(|a| relogio.saturating_sub(a.relogio).as_secs_f64())
            .filter(|s| *s > 0.0);
        let linhas = processos
            .iter()
            .map(|p| {
                let antes = anterior
                    .as_ref()
                    .and_then(|a| a.processos.get(&(p.pid, p.inicio)));
                let (cpu, leitura, escrita, operacoes) = match (antes, intervalo) {
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
                        let operacoes = p.operacoes.zip(antes.operacoes).map(|(agora, antes)| {
                            (
                                agora.0.saturating_sub(antes.0) as f64 / segundos,
                                agora.1.saturating_sub(antes.1) as f64 / segundos,
                            )
                        });
                        (cpu, disco.map(|d| d.0), disco.map(|d| d.1), operacoes)
                    }
                    _ => (None, None, None, None),
                };
                Linha {
                    processo: p.clone(),
                    cpu,
                    leitura,
                    escrita,
                    operacoes,
                }
            })
            .collect();
        let nucleos_uso = anterior
            .as_ref()
            .map(|a| uso_dos_nucleos(&a.cpus, &cpus))
            .unwrap_or_default();
        let uso_das_interfaces = interfaces
            .iter()
            .filter(|i| i.recebidos > 0 || i.enviados > 0)
            .map(|i| {
                let antes = anterior
                    .as_ref()
                    .and_then(|a| a.interfaces.get(&i.nome))
                    .zip(intervalo);
                UsoDaInterface {
                    nome: i.nome.clone(),
                    fisica: i.fisica,
                    recebidos: i.recebidos,
                    enviados: i.enviados,
                    recebendo: antes.map(|(a, s)| taxa(i.recebidos, a.0, s)),
                    enviando: antes.map(|(a, s)| taxa(i.enviados, a.1, s)),
                }
            })
            .collect();
        let uso_dos_discos = discos
            .iter()
            .filter(|d| d.leituras > 0 || d.escritas > 0)
            .map(|d| {
                let antes = anterior
                    .as_ref()
                    .and_then(|a| a.discos.get(&d.nome))
                    .zip(intervalo);
                UsoDoDisco {
                    nome: d.nome.clone(),
                    fisico: d.fisico,
                    lidos: d.lidos,
                    escritos: d.escritos,
                    leituras: antes.map(|(a, s)| taxa(d.leituras, a.leituras, s)),
                    escritas: antes.map(|(a, s)| taxa(d.escritas, a.escritas, s)),
                    lendo: antes.map(|(a, s)| taxa(d.lidos, a.lidos, s)),
                    escrevendo: antes.map(|(a, s)| taxa(d.escritos, a.escritos, s)),
                }
            })
            .collect();
        self.anterior = Some(Anterior {
            relogio,
            processos: processos
                .into_iter()
                .map(|p| ((p.pid, p.inicio), p))
                .collect(),
            cpus,
            interfaces: interfaces
                .into_iter()
                .map(|i| (i.nome, (i.recebidos, i.enviados)))
                .collect(),
            discos: discos.into_iter().map(|d| (d.nome.clone(), d)).collect(),
        });
        Leitura {
            linhas,
            nucleos,
            memoria_total,
            memoria_disponivel,
            nucleos_uso,
            interfaces: uso_das_interfaces,
            discos: uso_dos_discos,
        }
    }
}

/// Quanto um contador andou por segundo (um contador que voltou a zero não anda).
fn taxa(agora: u64, antes: u64, segundos: f64) -> f64 {
    agora.saturating_sub(antes) as f64 / segundos
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
    /// `sshd` (filho do 1) e o `rsync` que ele roda, que lê 512 KB/s e escreve 2 MB/s, em 40
    /// leituras e 120 escritas por segundo no `sda` (o volume lógico `dm-0` por cima repete o
    /// mesmo e fica fora do total). Os quatro núcleos somam os 14% deles, e a rede passa pela
    /// `eth0` e pela `wlan0` (e pela ponte `docker0`, que fica fora do total).
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
        operacoes: None,
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
    rsync.operacoes = Some((n * 40, n * 120));
    // O uso e a parte do kernel de cada núcleo, em milésimos de segundo por segundo.
    let cpus = [(400, 100), (100, 30), (40, 10), (20, 10)]
        .into_iter()
        .map(|(ocupado, kernel): (u64, u64)| TemposDeCpu {
            usuario: Duration::from_millis(n * (ocupado - kernel)),
            sistema: Duration::from_millis(n * kernel),
            ocioso: Duration::from_millis(n * (1000 - ocupado)),
        })
        .collect();
    let interface = |nome: &str, recebidos: u64, enviados: u64, fisica| Interface {
        nome: nome.to_owned(),
        recebidos: n * recebidos * 1024,
        enviados: n * enviados * 1024,
        fisica,
    };
    Foto {
        relogio: segundos,
        nucleos: 4,
        memoria_total: Some(8 * 1024 * 1024 * 1024),
        memoria_disponivel: Some(6 * 1024 * 1024 * 1024),
        processos: vec![init, sshd, rsync],
        cpus,
        interfaces: vec![
            interface("eth0", 1536, 256, true),
            interface("wlan0", 4, 1, true),
            interface("docker0", 64, 64, false),
        ],
        discos: ["sda", "dm-0"]
            .into_iter()
            .map(|nome| Disco {
                nome: nome.to_owned(),
                leituras: n * 40,
                escritas: n * 120,
                lidos: n * 512 * 1024,
                escritos: n * 2 * 1024 * 1024,
                fisico: nome == "sda",
            })
            .collect(),
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
            ..Foto::default()
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

    fn tempos(usuario_ms: u64, sistema_ms: u64, ocioso_ms: u64) -> TemposDeCpu {
        TemposDeCpu {
            usuario: Duration::from_millis(usuario_ms),
            sistema: Duration::from_millis(sistema_ms),
            ocioso: Duration::from_millis(ocioso_ms),
        }
    }

    #[test]
    fn os_nucleos_dao_o_uso_e_a_parte_do_kernel() {
        let mut c = Comparador::novo();
        let mut primeira = foto(0, vec![processo(1, 1, 0, (0, 0))]);
        primeira.cpus = vec![tempos(100, 100, 100), tempos(0, 0, 0)];
        let l = c.comparar(primeira);
        assert!(l.nucleos_uso.is_empty());
        let mut segunda = foto(1000, vec![processo(1, 1, 3000, (0, 0))]);
        // O primeiro núcleo: 600 ms ocupado (200 no kernel) de 1 s; o segundo, ocioso.
        segunda.cpus = vec![tempos(500, 300, 500), tempos(0, 0, 1000)];
        let l = c.comparar(segunda);
        let arredondar = |v: f64| (v * 1000.0).round() / 1000.0;
        let uso: Vec<(f64, f64)> = l
            .nucleos_uso
            .iter()
            .map(|n| (arredondar(n.total), arredondar(n.sistema)))
            .collect();
        assert_eq!(uso, vec![(60.0, 20.0), (0.0, 0.0)]);
        // A CPU da máquina vem dos núcleos, não da soma dos processos (que daria 75%).
        assert_eq!(arredondar(l.cpu()), 30.0);
        assert_eq!(l.cpu_sistema().map(arredondar), Some(10.0));
        // Um núcleo a mais (ou a menos) entre as fotos: sem uso, e a CPU volta à soma.
        let mut terceira = foto(2000, vec![processo(1, 1, 3000, (0, 0))]);
        terceira.cpus = vec![tempos(0, 0, 0)];
        let l = c.comparar(terceira);
        assert!(l.nucleos_uso.is_empty());
        assert_eq!(l.cpu_sistema(), None);
        assert_eq!(l.cpu(), 0.0);
    }

    #[test]
    fn a_rede_soma_as_placas_fisicas_por_segundo() {
        let interface = |nome: &str, recebidos, enviados, fisica| Interface {
            nome: nome.to_owned(),
            recebidos,
            enviados,
            fisica,
        };
        let mut c = Comparador::novo();
        let mut primeira = foto(0, Vec::new());
        primeira.interfaces = vec![
            interface("eth0", 1000, 100, true),
            interface("docker0", 50, 50, false),
            interface("tun0", 0, 0, false),
        ];
        let l = c.comparar(primeira);
        // A que nunca passou nada some; as outras ainda não têm taxa.
        let nomes: Vec<&str> = l.interfaces.iter().map(|i| i.nome.as_str()).collect();
        assert_eq!(nomes, ["eth0", "docker0"]);
        assert_eq!(l.rede(), (0.0, 0.0));
        assert_eq!(l.rede_total(), (1000, 100));
        let mut segunda = foto(2000, Vec::new());
        segunda.interfaces = vec![
            interface("eth0", 5000, 300, true),
            interface("docker0", 450, 50, false),
            interface("wg0", 10, 10, false),
        ];
        let l = c.comparar(segunda);
        let eth0 = &l.interfaces[0];
        assert_eq!((eth0.recebendo, eth0.enviando), (Some(2000.0), Some(100.0)));
        assert_eq!(l.interfaces[1].recebendo, Some(200.0));
        // A `wg0` apareceu agora: ainda sem taxa.
        assert_eq!(l.interfaces[2].recebendo, None);
        // O total da máquina é só da `eth0`.
        assert_eq!(l.rede(), (2000.0, 100.0));
        assert_eq!(l.rede_total(), (5000, 300));
        // Sem placa física (um contêiner), contam todas.
        let mut terceira = foto(4000, Vec::new());
        terceira.interfaces = vec![interface("docker0", 850, 250, false)];
        let l = c.comparar(terceira);
        assert_eq!(l.rede(), (200.0, 100.0));
    }

    #[test]
    fn a_foto_de_exemplo_tem_os_mesmos_14_por_cento_nos_nucleos() {
        let mut c = Comparador::novo();
        c.comparar(foto_de_exemplo(1));
        let l = c.comparar(foto_de_exemplo(2));
        let soma_dos_processos: f64 = l.linhas.iter().filter_map(|l| l.cpu).sum();
        assert!((l.cpu() - 14.0).abs() < 1e-9, "{}", l.cpu());
        assert!((soma_dos_processos - 14.0).abs() < 1e-9);
        assert!((l.cpu_sistema().unwrap_or(0.0) - 3.75).abs() < 1e-9);
        let (recebendo, enviando) = l.rede();
        assert_eq!((recebendo, enviando), (1540.0 * 1024.0, 257.0 * 1024.0));
        // O disco da máquina é o do `sda`, o mesmo que o rsync faz.
        assert_eq!(l.disco(), (512.0 * 1024.0, 2.0 * 1024.0 * 1024.0));
        assert_eq!(l.operacoes(), Some((40.0, 120.0)));
    }

    #[test]
    fn os_discos_dao_as_operacoes_por_segundo_dos_fisicos() {
        let disco = |nome: &str, leituras, escritas, fisico| Disco {
            nome: nome.to_owned(),
            leituras,
            escritas,
            lidos: leituras * 4096,
            escritos: escritas * 4096,
            fisico,
        };
        let mut c = Comparador::novo();
        let mut primeira = foto(0, vec![processo(1, 1, 0, (0, 0))]);
        primeira.discos = vec![
            disco("sda", 100, 50, true),
            disco("dm-0", 100, 50, false),
            disco("sr0", 0, 0, true),
        ];
        let l = c.comparar(primeira);
        // O que nunca fez nada some; os outros ainda não têm taxa, e o disco vem dos processos.
        let nomes: Vec<&str> = l.discos.iter().map(|d| d.nome.as_str()).collect();
        assert_eq!(nomes, ["sda", "dm-0"]);
        assert_eq!(l.operacoes(), None);
        let mut segunda = foto(2000, vec![processo(1, 1, 0, (2000, 0))]);
        segunda.discos = vec![
            disco("sda", 300, 450, true),
            disco("dm-0", 300, 450, false),
            disco("nvme0n1", 10, 10, true),
        ];
        let l = c.comparar(segunda);
        let sda = &l.discos[0];
        assert_eq!((sda.leituras, sda.escritas), (Some(100.0), Some(200.0)));
        assert_eq!(sda.lendo, Some(100.0 * 4096.0));
        assert_eq!(l.discos[2].leituras, None, "o nvme apareceu agora");
        // O total é só do `sda`; o `dm-0` repete o mesmo tráfego.
        assert_eq!(l.operacoes(), Some((100.0, 200.0)));
        assert_eq!(l.disco(), (100.0 * 4096.0, 200.0 * 4096.0));
        // Sem disco físico, contam todos.
        let mut terceira = foto(4000, Vec::new());
        terceira.discos = vec![disco("dm-0", 500, 450, false)];
        assert_eq!(c.comparar(terceira).operacoes(), Some((100.0, 0.0)));
    }

    #[test]
    fn as_operacoes_do_processo_por_segundo() {
        let mut c = Comparador::novo();
        let mut p = processo(9, 9, 0, (0, 0));
        p.operacoes = Some((10, 20));
        c.comparar(foto(0, vec![p.clone()]));
        p.operacoes = Some((30, 120));
        let l = c.comparar(foto(2000, vec![p]));
        assert_eq!(l.linhas[0].operacoes, Some((10.0, 50.0)));
        // Sem os contadores (Linux, macOS), sem operações.
        let l = c.comparar(foto(3000, vec![processo(9, 9, 0, (0, 0))]));
        assert_eq!(l.linhas[0].operacoes, None);
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
