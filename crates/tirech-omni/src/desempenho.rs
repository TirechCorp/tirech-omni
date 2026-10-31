//! O medidor de CPU e a página Desempenho: quanto o app gasta, no total e por aba.
//!
//! A cada segundo a tela lê uma [`Fonte`] e passa as leituras ao [`Amostrador`], que compara com
//! a leitura de antes e devolve a [`Amostra`]: a CPU do processo (em % da máquina inteira, como
//! no Gerenciador de Tarefas), a memória, os dados por segundo e o peso de cada aba. O peso de
//! uma aba é o tempo da interface com ela (ler, interpretar e pintar, que a lib conta em
//! `Session::busy`) mais a CPU das threads da conexão (`Session::threads`, só no Linux). O que
//! sobra do total é a linha "Interface". O programa do shell local (o filho do pseudoterminal e
//! os filhos dele) é de outro processo e fica numa coluna à parte, fora do total.
//!
//! A [`FonteDoSistema`] lê o `/proc` no Linux, o `GetProcessTimes` no Windows (pelo
//! `tomni-ui-win32`) e o `proc_pidinfo` no macOS (pelo `tomni-ui-macos`); nos outros sistemas,
//! só o tempo da interface por aba. A [`FonteFixa`] é a
//! dos testes, com os números que eles põem.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use tomni_ui::files::Live;
use tomni_ui::terminal::Session;

/// Quantos pontos os gráficos guardam: um minuto, com uma amostra por segundo.
pub const PONTOS: usize = 60;

/// O intervalo entre as amostras.
pub const RITMO: Duration = Duration::from_secs(1);

/// O que o processo inteiro usou até agora.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UsoDoProcesso {
    /// O tempo de CPU somado de todas as threads (usuário e sistema).
    pub cpu: Duration,
    /// A memória em uso (residente), em bytes.
    pub memoria: u64,
}

/// O que uma aba usou até agora, somando desde que a sessão abriu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LeituraDaAba {
    /// O tempo da interface com a sessão.
    pub ocupado: Duration,
    /// Os bytes que chegaram e que foram pela conexão.
    pub bytes: u64,
    /// A CPU de cada thread da conexão agora, pelo id no sistema.
    pub threads: Vec<(u32, Duration)>,
    /// A CPU do programa do shell local e dos filhos dele; `None` sem programa local.
    pub programa: Option<Duration>,
}

/// De onde vêm as leituras: o sistema ou, nos testes, números fixos.
pub trait Fonte {
    /// Um relógio que só anda para a frente.
    fn agora(&self) -> Duration;
    /// Quantos núcleos a máquina tem (a CPU em % divide por eles).
    fn nucleos(&self) -> usize;
    /// O uso do processo; `None` onde o sistema não diz.
    fn processo(&self) -> Option<UsoDoProcesso>;
    /// A leitura de uma aba.
    fn aba(&self, sessao: Session) -> LeituraDaAba;
}

/// As leituras de verdade.
pub struct FonteDoSistema {
    inicio: Instant,
}

impl FonteDoSistema {
    /// A fonte, com o relógio começando agora.
    pub fn nova() -> Self {
        Self {
            inicio: Instant::now(),
        }
    }
}

impl Fonte for FonteDoSistema {
    fn agora(&self) -> Duration {
        self.inicio.elapsed()
    }

    fn nucleos(&self) -> usize {
        std::thread::available_parallelism().map_or(1, usize::from)
    }

    fn processo(&self) -> Option<UsoDoProcesso> {
        sistema::processo()
    }

    fn aba(&self, sessao: Session) -> LeituraDaAba {
        LeituraDaAba {
            ocupado: sessao.busy(),
            bytes: sessao.received() + sessao.sent(),
            threads: sessao
                .threads()
                .into_iter()
                .filter_map(|id| Some((id, sistema::thread(id)?)))
                .collect(),
            programa: sessao.process_id().and_then(sistema::programa),
        }
    }
}

/// A fonte dos testes: o relógio e as leituras são os que o teste põe.
#[derive(Default)]
pub struct FonteFixa {
    agora: RefCell<Duration>,
    processo: RefCell<Option<UsoDoProcesso>>,
    abas: RefCell<HashMap<String, LeituraDaAba>>,
}

impl FonteFixa {
    /// Sem leituras: o processo não diz nada e as abas estão zeradas.
    pub fn nova() -> Self {
        Self::default()
    }

    /// Anda o relógio.
    pub fn andar(&self, tempo: Duration) {
        *self.agora.borrow_mut() += tempo;
    }

    /// O uso do processo até agora.
    pub fn processo(&self, uso: UsoDoProcesso) {
        *self.processo.borrow_mut() = Some(uso);
    }

    /// A leitura da aba de chave `chave` até agora.
    pub fn aba(&self, chave: &str, leitura: LeituraDaAba) {
        self.abas.borrow_mut().insert(chave.to_owned(), leitura);
    }
}

impl Fonte for FonteFixa {
    fn agora(&self) -> Duration {
        *self.agora.borrow()
    }

    fn nucleos(&self) -> usize {
        4
    }

    fn processo(&self) -> Option<UsoDoProcesso> {
        *self.processo.borrow()
    }

    fn aba(&self, sessao: Session) -> LeituraDaAba {
        self.abas
            .borrow()
            .get(&sessao.key_str())
            .cloned()
            .unwrap_or_default()
    }
}

/// O peso de uma aba numa amostra.
#[derive(Clone, Debug, PartialEq)]
pub struct UsoDaAba {
    /// A chave da sessão.
    pub chave: String,
    /// A CPU que o app gastou com ela, em % da máquina.
    pub cpu: f64,
    /// Os bytes por segundo, chegando e indo.
    pub dados: f64,
    /// A CPU do programa local, em % da máquina; `None` sem programa local.
    pub programa: Option<f64>,
}

/// Uma amostra: o que mudou desde a anterior.
#[derive(Clone, Debug, PartialEq)]
pub struct Amostra {
    /// A CPU do processo, em % da máquina (de 0 a 100). Sem a leitura do processo, a soma das
    /// abas.
    pub cpu: f64,
    /// A memória em uso, em bytes, se o sistema diz.
    pub memoria: Option<u64>,
    /// Os bytes por segundo de todas as abas.
    pub dados: f64,
    /// O peso de cada aba, na ordem em que vieram.
    pub abas: Vec<UsoDaAba>,
    /// O que sobra da CPU do processo fora das abas (a interface, o cofre, o agente…).
    pub interface: f64,
}

/// A leitura de antes de uma aba.
#[derive(Default)]
struct Antes {
    ocupado: Duration,
    bytes: u64,
    threads: HashMap<u32, Duration>,
    programa: Option<Duration>,
}

/// Compara cada leitura com a anterior.
#[derive(Default)]
pub struct Amostrador {
    antes: Option<(Duration, Option<UsoDoProcesso>)>,
    abas: HashMap<String, Antes>,
}

impl Amostrador {
    /// Sem leitura anterior: a primeira amostra só guarda.
    pub fn novo() -> Self {
        Self::default()
    }

    /// Passa as leituras de agora. Devolve `None` na primeira vez e quando o relógio não andou.
    /// Uma aba nova conta desde zero; a que sumiu sai da memória.
    pub fn amostrar(
        &mut self,
        agora: Duration,
        nucleos: usize,
        processo: Option<UsoDoProcesso>,
        abas: Vec<(String, LeituraDaAba)>,
    ) -> Option<Amostra> {
        let anterior = self.antes.replace((agora, processo));
        let mut antes_das_abas = std::mem::take(&mut self.abas);
        let mut usos = Vec::with_capacity(abas.len());
        let intervalo = anterior.and_then(|(t, _)| agora.checked_sub(t));
        let escala = intervalo
            .filter(|i| !i.is_zero())
            .map(|i| i.as_secs_f64() * nucleos.max(1) as f64);
        for (chave, leitura) in abas {
            let antes = antes_das_abas.remove(&chave).unwrap_or_default();
            if let Some(escala) = escala {
                let mut gasto = leitura.ocupado.saturating_sub(antes.ocupado);
                for (id, tempo) in &leitura.threads {
                    let de_antes = antes.threads.get(id).copied().unwrap_or_default();
                    gasto += tempo.saturating_sub(de_antes);
                }
                let programa = leitura.programa.map(|agora| {
                    let gasto = agora.saturating_sub(antes.programa.unwrap_or_default());
                    porcento(gasto, escala)
                });
                usos.push(UsoDaAba {
                    chave: chave.clone(),
                    cpu: porcento(gasto, escala),
                    dados: leitura.bytes.saturating_sub(antes.bytes) as f64 * nucleos.max(1) as f64
                        / escala,
                    programa,
                });
            }
            self.abas.insert(
                chave,
                Antes {
                    ocupado: leitura.ocupado,
                    bytes: leitura.bytes,
                    threads: leitura.threads.into_iter().collect(),
                    programa: leitura.programa,
                },
            );
        }
        let escala = escala?;
        let das_abas: f64 = usos.iter().map(|u| u.cpu).sum();
        let total = match (anterior.and_then(|(_, p)| p), processo) {
            (Some(antes), Some(agora)) => porcento(agora.cpu.saturating_sub(antes.cpu), escala),
            _ => das_abas,
        };
        // O tempo das abas e o do processo são lidos em momentos um pouco diferentes: o total
        // nunca fica abaixo da soma delas.
        let cpu = total.max(das_abas).min(100.0);
        Some(Amostra {
            cpu,
            memoria: processo.map(|p| p.memoria),
            dados: usos.iter().map(|u| u.dados).sum(),
            interface: (cpu - das_abas).max(0.0),
            abas: usos,
        })
    }
}

/// `gasto` em % de `escala` (segundos × núcleos), de 0 a 100.
fn porcento(gasto: Duration, escala: f64) -> f64 {
    (gasto.as_secs_f64() / escala * 100.0).clamp(0.0, 100.0)
}

/// O quanto a tela pede de amostras: a fonte e quem bate o ritmo.
#[derive(Clone)]
pub struct Medicao {
    /// De onde vêm as leituras.
    pub fonte: std::rc::Rc<dyn Fonte>,
    /// `None`: uma thread do app bate a cada [`RITMO`]. `Some`: quem tem o `Live` bate (os
    /// testes, e o app com a pasta trocada por variável, em que nada bate e o medidor fica
    /// parado, para as capturas de tela não mudarem).
    pub batida: Option<Live<u64>>,
}

impl Medicao {
    /// As leituras do sistema, a cada segundo.
    pub fn do_sistema() -> Self {
        Self {
            fonte: std::rc::Rc::new(FonteDoSistema::nova()),
            batida: None,
        }
    }

    /// As leituras do sistema, sem ritmo: só amostra quando `batida` muda.
    pub fn parada() -> Self {
        Self {
            fonte: std::rc::Rc::new(FonteDoSistema::nova()),
            batida: Some(Live::detached(0)),
        }
    }

    /// As leituras de `fonte`, que o teste bate com `batida`.
    pub fn de_teste(fonte: std::rc::Rc<FonteFixa>, batida: Live<u64>) -> Self {
        Self {
            fonte,
            batida: Some(batida),
        }
    }
}

/// O texto de um número de bytes por segundo ("0 B/s", "12,3 KB/s", "4,5 MB/s").
pub fn bytes_por_segundo(bytes: f64) -> String {
    let (valor, unidade) = if bytes >= 1024.0 * 1024.0 {
        (bytes / (1024.0 * 1024.0), "MB/s")
    } else if bytes >= 1024.0 {
        (bytes / 1024.0, "KB/s")
    } else {
        return format!("{bytes:.0} B/s");
    };
    format!("{} {unidade}", crate::i18n::decimal(valor, 1))
}

/// O texto de uma memória em bytes ("512 MB", "1,2 GB").
pub fn memoria(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{} GB", crate::i18n::decimal(mb / 1024.0, 1))
    } else {
        format!("{mb:.0} MB")
    }
}

/// O texto de uma CPU em % ("0%", "3,5%", "42%"): uma casa abaixo de 10%.
pub fn cpu(valor: f64) -> String {
    if valor > 0.0 && valor < 10.0 {
        format!("{}%", crate::i18n::decimal(valor, 1))
    } else {
        format!("{valor:.0}%")
    }
}

/// As leituras do sistema: o `/proc` no Linux, o Win32 no Windows e o `libproc` no macOS.
mod sistema {
    use super::UsoDoProcesso;
    use std::time::Duration;

    /// Os tiques do `/proc/<pid>/stat` por segundo (o `USER_HZ`, fixo em 100 no Linux).
    #[cfg(target_os = "linux")]
    const TIQUES: u64 = 100;

    /// O tempo de CPU (usuário e sistema) de uma linha do `stat`: os campos 14 e 15, contados
    /// depois do nome entre parênteses (que pode ter espaços).
    pub(super) fn tempo_do_stat(linha: &str) -> Option<(Duration, u32)> {
        let resto = &linha[linha.rfind(')')? + 1..];
        let campos: Vec<&str> = resto.split_whitespace().collect();
        // O campo 3 (o estado) é o primeiro depois do nome.
        let campo = |n: usize| campos.get(n - 3).and_then(|c| c.parse::<u64>().ok());
        let tiques = campo(14)? + campo(15)?;
        let pai = u32::try_from(campo(4)?).ok()?;
        Some((tempo_de_tiques(tiques), pai))
    }

    fn tempo_de_tiques(tiques: u64) -> Duration {
        #[cfg(target_os = "linux")]
        let por_segundo = TIQUES;
        #[cfg(not(target_os = "linux"))]
        let por_segundo = 100;
        Duration::from_millis(tiques * 1000 / por_segundo)
    }

    /// A memória residente do `/proc/self/status` (a linha `VmRSS`, em kB).
    #[cfg(any(target_os = "linux", test))]
    pub(super) fn memoria_do_status(texto: &str) -> Option<u64> {
        let linha = texto.lines().find(|l| l.starts_with("VmRSS:"))?;
        let kb: u64 = linha.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }

    #[cfg(target_os = "linux")]
    pub(super) fn processo() -> Option<UsoDoProcesso> {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        Some(UsoDoProcesso {
            cpu: tempo_do_stat(&stat)?.0,
            memoria: memoria_do_status(&status).unwrap_or(0),
        })
    }

    #[cfg(windows)]
    pub(super) fn processo() -> Option<UsoDoProcesso> {
        let uso = tomni_ui_win32::process_usage(std::process::id()).ok()?;
        Some(UsoDoProcesso {
            cpu: uso.cpu,
            memoria: uso.memory,
        })
    }

    #[cfg(target_os = "macos")]
    pub(super) fn processo() -> Option<UsoDoProcesso> {
        let uso = tomni_ui_macos::process_usage(std::process::id()).ok()?;
        Some(UsoDoProcesso {
            cpu: uso.cpu,
            memoria: uso.memory,
        })
    }

    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    pub(super) fn processo() -> Option<UsoDoProcesso> {
        None
    }

    /// A CPU de uma thread do próprio processo.
    pub(super) fn thread(id: u32) -> Option<Duration> {
        if !cfg!(target_os = "linux") {
            return None;
        }
        let stat = std::fs::read_to_string(format!("/proc/self/task/{id}/stat")).ok()?;
        Some(tempo_do_stat(&stat)?.0)
    }

    /// A CPU do processo `pid` e dos descendentes dele (o shell e o que roda nele).
    #[cfg(target_os = "linux")]
    pub(super) fn programa(pid: u32) -> Option<Duration> {
        let mut total = Duration::ZERO;
        let mut fila = vec![pid];
        let mut vistos = 0;
        while let Some(pid) = fila.pop() {
            // Um teto contra ciclos (um pid reaproveitado no meio da leitura).
            vistos += 1;
            if vistos > 4096 {
                break;
            }
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            total += tempo_do_stat(&stat).map_or(Duration::ZERO, |(t, _)| t);
            // Os filhos de cada thread (`CONFIG_PROC_CHILDREN`, ligado nos kernels comuns).
            let Ok(threads) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
                continue;
            };
            for thread in threads.flatten() {
                let filhos = std::fs::read_to_string(thread.path().join("children"));
                fila.extend(
                    filhos
                        .iter()
                        .flat_map(|f| f.split_whitespace())
                        .filter_map(|p| p.parse::<u32>().ok()),
                );
            }
        }
        (vistos > 0).then_some(total)
    }

    #[cfg(windows)]
    pub(super) fn programa(pid: u32) -> Option<Duration> {
        let mut total = tomni_ui_win32::process_usage(pid).ok()?.cpu;
        // Os descendentes, pela tabela de processos (o pid do pai pode ter sido reaproveitado:
        // o teto evita andar em círculos).
        let tabela = tomni_ui_win32::process_table().unwrap_or_default();
        let mut fila = vec![pid];
        let mut vistos = 0;
        while let Some(pai) = fila.pop() {
            vistos += 1;
            if vistos > 4096 {
                break;
            }
            for filho in tabela
                .iter()
                .filter(|p| p.parent_pid == pai && p.pid != pai)
            {
                if let Ok(uso) = tomni_ui_win32::process_usage(filho.pid) {
                    total += uso.cpu;
                }
                fila.push(filho.pid);
            }
        }
        Some(total)
    }

    #[cfg(target_os = "macos")]
    pub(super) fn programa(pid: u32) -> Option<Duration> {
        let mut total = tomni_ui_macos::process_usage(pid).ok()?.cpu;
        // Os descendentes, filho a filho (um pid reaproveitado no meio da leitura poderia fazer
        // um ciclo: o teto evita andar em círculos). O que não dá para ler (o `sudo`, de outro
        // usuário) fica de fora, mas os filhos dele entram.
        let mut fila = vec![pid];
        let mut vistos = 0;
        while let Some(pai) = fila.pop() {
            vistos += 1;
            if vistos > 4096 {
                break;
            }
            for filho in tomni_ui_macos::child_pids(pai).unwrap_or_default() {
                if filho == pai {
                    continue;
                }
                if let Ok(uso) = tomni_ui_macos::process_usage(filho) {
                    total += uso.cpu;
                }
                fila.push(filho);
            }
        }
        Some(total)
    }

    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    pub(super) fn programa(_pid: u32) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn aba(ocupado_ms: u64, bytes: u64) -> LeituraDaAba {
        LeituraDaAba {
            ocupado: Duration::from_millis(ocupado_ms),
            bytes,
            ..LeituraDaAba::default()
        }
    }

    fn processo(cpu_ms: u64) -> Option<UsoDoProcesso> {
        Some(UsoDoProcesso {
            cpu: Duration::from_millis(cpu_ms),
            memoria: 100 << 20,
        })
    }

    #[test]
    fn a_primeira_leitura_so_guarda() {
        let mut a = Amostrador::novo();
        assert_eq!(a.amostrar(Duration::ZERO, 4, processo(0), vec![]), None);
        // O relógio parado também não dá amostra.
        assert_eq!(a.amostrar(Duration::ZERO, 4, processo(10), vec![]), None);
    }

    #[test]
    fn a_cpu_e_da_maquina_inteira_e_a_interface_e_o_que_sobra() {
        let mut a = Amostrador::novo();
        a.amostrar(
            Duration::ZERO,
            4,
            processo(1_000),
            vec![("a".into(), aba(0, 0))],
        );
        let mut com_thread = aba(200, 4_096);
        com_thread.threads = vec![(7, Duration::from_millis(200))];
        let amostra = a
            .amostrar(
                Duration::from_secs(1),
                4,
                processo(1_000 + 800),
                vec![("a".into(), com_thread)],
            )
            .unwrap();
        // 0,8 s de CPU num segundo com 4 núcleos: 20% da máquina.
        assert!((amostra.cpu - 20.0).abs() < 1e-9, "{amostra:?}");
        // A aba: 0,2 s da interface e 0,2 s da thread nova.
        assert!((amostra.abas[0].cpu - 10.0).abs() < 1e-9, "{amostra:?}");
        assert!((amostra.interface - 10.0).abs() < 1e-9, "{amostra:?}");
        assert!((amostra.dados - 4_096.0).abs() < 1e-9);
        assert_eq!(amostra.memoria, Some(100 << 20));
        assert_eq!(amostra.abas[0].programa, None);
    }

    #[test]
    fn a_thread_conta_so_o_que_andou_e_a_aba_nova_comeca_do_zero() {
        let mut a = Amostrador::novo();
        let mut leitura = aba(0, 0);
        leitura.threads = vec![(7, Duration::from_millis(500))];
        leitura.programa = Some(Duration::from_millis(300));
        a.amostrar(Duration::ZERO, 2, None, vec![("a".into(), leitura.clone())]);
        leitura.threads = vec![(7, Duration::from_millis(700))];
        leitura.programa = Some(Duration::from_millis(1_300));
        let amostra = a
            .amostrar(
                Duration::from_secs(2),
                2,
                None,
                vec![("a".into(), leitura), ("b".into(), aba(400, 0))],
            )
            .unwrap();
        // 0,2 s em 2 s × 2 núcleos.
        assert!((amostra.abas[0].cpu - 5.0).abs() < 1e-9, "{amostra:?}");
        assert!((amostra.abas[0].programa.unwrap() - 25.0).abs() < 1e-9);
        assert!((amostra.abas[1].cpu - 10.0).abs() < 1e-9, "{amostra:?}");
        // Sem a leitura do processo, o total é a soma das abas.
        assert!((amostra.cpu - 15.0).abs() < 1e-9);
        assert_eq!(amostra.interface, 0.0);
        assert_eq!(amostra.memoria, None);
    }

    #[test]
    fn o_total_nunca_fica_abaixo_das_abas() {
        let mut a = Amostrador::novo();
        a.amostrar(Duration::ZERO, 1, processo(0), vec![]);
        let amostra = a
            .amostrar(
                Duration::from_secs(1),
                1,
                processo(100),
                vec![("a".into(), aba(300, 0))],
            )
            .unwrap();
        assert!((amostra.cpu - 30.0).abs() < 1e-9);
        assert_eq!(amostra.interface, 0.0);
    }

    #[test]
    fn le_o_stat_com_espaco_e_parentese_no_nome() {
        let linha = "4242 (meu (app) x) S 17 4242 4242 0 -1 4194560 100 0 0 0 250 50 3 4 20 0 \
                     12 0 1000 0 0";
        let (tempo, pai) = sistema::tempo_do_stat(linha).unwrap();
        assert_eq!(tempo, Duration::from_millis(3_000));
        assert_eq!(pai, 17);
        assert_eq!(sistema::tempo_do_stat("lixo"), None);
        let status = "Name:\tapp\nVmPeak:\t  9000 kB\nVmRSS:\t  2048 kB\n";
        assert_eq!(sistema::memoria_do_status(status), Some(2 << 20));
    }

    #[test]
    fn o_proprio_processo_aparece_no_linux() {
        if !cfg!(target_os = "linux") {
            return;
        }
        let uso = sistema::processo().expect("o /proc/self existe");
        assert!(uso.memoria > 0);
        let eu = tomni_ui::terminal::current_thread_id().unwrap();
        assert!(sistema::thread(eu).is_some());
        assert!(sistema::programa(std::process::id()).is_some());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn o_processo_e_o_neto_do_shell_aparecem_no_macos() {
        let uso = sistema::processo().expect("o proc_pidinfo do próprio processo");
        assert!(uso.memoria > 0);
        assert!(uso.cpu > Duration::ZERO);
        // O shell só espera; quem gasta é o neto, que para sozinho em 2 s.
        let mut shell = std::process::Command::new("/bin/sh")
            .args([
                "-c",
                "/bin/bash -c 'while [ $SECONDS -lt 2 ]; do :; done' & wait",
            ])
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_secs(1));
        let programa = sistema::programa(shell.id());
        shell.kill().unwrap();
        shell.wait().unwrap();
        let programa = programa.expect("o shell ainda roda");
        assert!(programa >= Duration::from_millis(300), "{programa:?}");
        assert_eq!(sistema::programa(999_999), None);
    }

    #[test]
    fn os_textos_dos_numeros() {
        assert_eq!(cpu(0.0), "0%");
        assert_eq!(cpu(42.4), "42%");
        assert_eq!(memoria(512 << 20), "512 MB");
        assert_eq!(bytes_por_segundo(10.0), "10 B/s");
    }
}
