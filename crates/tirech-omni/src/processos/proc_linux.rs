//! Os leitores do `/proc` do Linux, os mesmos para este computador e para o servidor (pelo
//! [`SCRIPT`], que junta os arquivos numa saída só, separada em seções).
//!
//! As threads do kernel (o `kthreadd`, pid 2, e os filhos dele) ficam de fora, como no
//! monitor de processos de costume.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::{Disco, Estado, Foto, Interface, Processo, TemposDeCpu};
use crate::t;

/// Os tiques do `stat` por segundo (o `USER_HZ`, 100 em todo Linux de hoje).
const TIQUES: u64 = 100;

/// Quanto da linha de comando se lê de cada processo.
pub const COMANDO_MAXIMO: usize = 1024;

/// O script que o servidor roda (por `sh -s`): cada seção começa com uma linha `@nome`. Sem o
/// `/proc`, só a linha `@sem-proc`.
pub const SCRIPT: &str = r#"export LC_ALL=C
if [ ! -r /proc/stat ]; then echo @sem-proc; exit 0; fi
echo @uptime; cat /proc/uptime
echo @cpus; grep '^cpu[0-9]' /proc/stat
echo @meminfo; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo
echo @passwd; cut -d: -f1,3 /etc/passwd 2>/dev/null
echo @stat; cat /proc/[0-9]*/stat 2>/dev/null
echo @status; grep -H -E '^(Uid|VmRSS):' /proc/[0-9]*/status 2>/dev/null
echo @io; grep -H -E '^(read_bytes|write_bytes):' /proc/[0-9]*/io 2>/dev/null
echo @netdev; cat /proc/net/dev 2>/dev/null
echo @virtual; ls /sys/devices/virtual/net 2>/dev/null
echo @diskstats; cat /proc/diskstats 2>/dev/null
echo @blocos; ls /sys/block 2>/dev/null
echo @virtuais-bloco; ls /sys/devices/virtual/block 2>/dev/null
echo @cmdline; head -v -c 1024 /proc/[0-9]*/cmdline 2>/dev/null
echo
echo @fim
"#;

/// O que a linha do `/proc/<pid>/stat` diz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub pid: u32,
    pub nome: String,
    pub estado: Estado,
    pub pai: u32,
    pub cpu: Duration,
    pub threads: u32,
    /// Quando começou, em tiques desde o boot.
    pub inicio: u64,
}

/// Uma linha do `stat`. O nome vem entre parênteses e pode ter espaços e parênteses: o fim
/// dele é o último `)`.
pub fn do_stat(linha: &str) -> Option<Stat> {
    let abre = linha.find('(')?;
    let fecha = linha.rfind(')')?;
    let pid = linha[..abre].trim().parse().ok()?;
    let nome = linha.get(abre + 1..fecha)?.to_owned();
    let campos: Vec<&str> = linha[fecha + 1..].split_whitespace().collect();
    // O campo 3 (o estado) é o primeiro depois do nome.
    let campo = |n: usize| campos.get(n - 3).copied();
    let numero = |n: usize| campo(n).and_then(|c| c.parse::<u64>().ok());
    let tiques = numero(14)? + numero(15)?;
    Some(Stat {
        pid,
        nome,
        estado: Estado::da_letra(campo(3)?.chars().next()?),
        pai: u32::try_from(numero(4)?).ok()?,
        cpu: Duration::from_millis(tiques * 1000 / TIQUES),
        threads: u32::try_from(numero(20)?).ok()?,
        inicio: numero(22)?,
    })
}

/// O dono (o uid real) e a memória residente do `status`.
pub fn do_status(texto: &str) -> (Option<u32>, Option<u64>) {
    let mut uid = None;
    let mut memoria = None;
    for linha in texto.lines() {
        if let Some(resto) = linha.strip_prefix("Uid:") {
            uid = resto.split_whitespace().next().and_then(|u| u.parse().ok());
        } else if let Some(resto) = linha.strip_prefix("VmRSS:") {
            memoria = resto
                .split_whitespace()
                .next()
                .and_then(|kb| kb.parse::<u64>().ok())
                .map(|kb| kb * 1024);
        }
    }
    (uid, memoria)
}

/// Os bytes lidos e escritos no disco, do `io`.
pub fn do_io(texto: &str) -> Option<(u64, u64)> {
    let valor = |nome: &str| {
        texto
            .lines()
            .find_map(|l| l.strip_prefix(nome))
            .and_then(|v| v.trim().parse::<u64>().ok())
    };
    Some((valor("read_bytes:")?, valor("write_bytes:")?))
}

/// A linha de comando do `cmdline`: os argumentos vêm separados por NUL.
pub fn da_cmdline(bytes: &[u8]) -> String {
    let bytes = &bytes[..bytes.len().min(COMANDO_MAXIMO)];
    String::from_utf8_lossy(bytes)
        .split('\0')
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Os nomes dos usuários pelo uid, do `/etc/passwd` (ou do `cut -d: -f1,3` dele).
pub fn usuarios(passwd: &str) -> HashMap<u32, String> {
    passwd
        .lines()
        .filter_map(|l| {
            let mut campos = l.split(':');
            let nome = campos.next()?;
            // O `/etc/passwd` inteiro tem a senha no meio; o do script, não.
            let uid = campos.find_map(|c| c.parse::<u32>().ok())?;
            Some((uid, nome.to_owned()))
        })
        .collect()
}

/// O tempo desde o boot, do `/proc/uptime`.
pub fn do_uptime(texto: &str) -> Option<Duration> {
    let segundos: f64 = texto.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_secs_f64(segundos.max(0.0)))
}

/// Quantos núcleos, pelas linhas `cpuN` do `/proc/stat`.
pub fn nucleos(stat: &str) -> usize {
    stat.lines()
        .filter(|l| {
            l.strip_prefix("cpu")
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_digit()))
        })
        .count()
        .max(1)
}

/// Os tempos de cada núcleo, das linhas `cpuN` do `/proc/stat`: o usuário junta o `nice` e o
/// `steal`; o sistema, as interrupções; o ocioso, a espera do disco.
pub fn tempos_das_cpus(stat: &str) -> Vec<TemposDeCpu> {
    let tempo = |tiques: u64| Duration::from_millis(tiques * 1000 / TIQUES);
    stat.lines()
        .filter_map(|l| {
            let resto = l.strip_prefix("cpu")?;
            if !resto.starts_with(|c: char| c.is_ascii_digit()) {
                return None;
            }
            let campos: Vec<u64> = resto
                .split_whitespace()
                .skip(1)
                .map(|c| c.parse().unwrap_or(0))
                .collect();
            let campo = |i: usize| campos.get(i).copied().unwrap_or(0);
            Some(TemposDeCpu {
                usuario: tempo(campo(0) + campo(1) + campo(7)),
                sistema: tempo(campo(2) + campo(5) + campo(6)),
                ocioso: tempo(campo(3) + campo(4)),
            })
        })
        .collect()
}

/// As interfaces de rede do `/proc/net/dev`, sem a de loopback. As que estão em `virtuais`
/// (os nomes de `/sys/devices/virtual/net`: pontes, túneis, contêineres) não são físicas.
pub fn da_net_dev(texto: &str, virtuais: &HashSet<String>) -> Vec<Interface> {
    texto
        .lines()
        .filter_map(|l| {
            let (nome, numeros) = l.split_once(':')?;
            let nome = nome.trim();
            let campos: Vec<u64> = numeros
                .split_whitespace()
                .map(|c| c.parse().unwrap_or(0))
                .collect();
            if nome.is_empty() || nome == "lo" || campos.len() < 9 {
                return None;
            }
            Some(Interface {
                nome: nome.to_owned(),
                recebidos: campos[0],
                enviados: campos[8],
                fisica: !virtuais.contains(nome),
            })
        })
        .collect()
}

/// Os discos do `/proc/diskstats` (`maior menor nome` e os contadores): só os discos inteiros
/// (os `blocos`, de `/sys/block`; sem a lista, todos), sem os `loop` e os `ram`. Os de
/// `virtuais` (`/sys/devices/virtual/block`: `dm-`, `md`, `zram`) não são físicos. O
/// `/sys/block` troca a `/` do nome por `!` (`cciss!c0d0`).
pub fn da_diskstats(
    texto: &str,
    blocos: &HashSet<String>,
    virtuais: &HashSet<String>,
) -> Vec<Disco> {
    let sem_barra = |nome: &str| nome.replace('/', "!");
    texto
        .lines()
        .filter_map(|linha| {
            let mut partes = linha.split_whitespace().skip(2);
            let nome = partes.next()?;
            let inteiro = blocos.is_empty() || blocos.contains(&sem_barra(nome));
            if !inteiro || nome.starts_with("loop") || nome.starts_with("ram") {
                return None;
            }
            let campos: Vec<u64> = partes.map(|c| c.parse().unwrap_or(0)).collect();
            if campos.len() < 7 {
                return None;
            }
            // 0: leituras, 2: setores lidos, 4: escritas, 6: setores escritos (de 512 bytes).
            Some(Disco {
                nome: nome.to_owned(),
                leituras: campos[0],
                escritas: campos[4],
                lidos: campos[2] * 512,
                escritos: campos[6] * 512,
                fisico: !virtuais.contains(&sem_barra(nome)),
            })
        })
        .collect()
}

/// Os nomes de uma listagem (uma linha por nome), sem as linhas em branco.
fn nomes<'a>(linhas: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    linhas
        .into_iter()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect()
}

/// A memória total e a disponível, do `/proc/meminfo`.
pub fn da_meminfo(texto: &str) -> (Option<u64>, Option<u64>) {
    let valor = |nome: &str| {
        texto
            .lines()
            .find_map(|l| l.strip_prefix(nome))
            .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    (valor("MemTotal:"), valor("MemAvailable:"))
}

/// O que se leu de cada processo, antes de juntar.
#[derive(Default)]
pub struct Arquivos {
    pub stats: Vec<Stat>,
    pub status: HashMap<u32, String>,
    pub io: HashMap<u32, String>,
    pub cmdline: HashMap<u32, String>,
}

/// Junta os arquivos de cada processo, sem as threads do kernel.
pub fn montar(arquivos: Arquivos, usuarios: &HashMap<u32, String>) -> Vec<Processo> {
    let Arquivos {
        stats,
        status,
        io,
        cmdline,
    } = arquivos;
    stats
        .into_iter()
        .filter(|s| s.pid != 2 && s.pai != 2)
        .map(|s| {
            let (uid, memoria) = status.get(&s.pid).map_or((None, None), |t| do_status(t));
            let usuario = uid
                .map(|uid| {
                    usuarios
                        .get(&uid)
                        .cloned()
                        .unwrap_or_else(|| uid.to_string())
                })
                .unwrap_or_default();
            Processo {
                pid: s.pid,
                pai: s.pai,
                inicio: s.inicio,
                comando: cmdline.get(&s.pid).cloned().unwrap_or_default(),
                nome: s.nome,
                usuario,
                estado: Some(s.estado),
                threads: Some(s.threads),
                cpu: Some(s.cpu),
                memoria,
                disco: io.get(&s.pid).and_then(|t| do_io(t)),
                operacoes: None,
            }
        })
        .collect()
}

/// O pid de um caminho `/proc/<pid>/...`.
fn pid_do_caminho(caminho: &str) -> Option<u32> {
    caminho
        .strip_prefix("/proc/")?
        .split('/')
        .next()?
        .parse()
        .ok()
}

/// As linhas `grep -H` (`/proc/<pid>/arquivo:conteúdo`) juntadas por pid.
fn por_pid(linhas: &[&str]) -> HashMap<u32, String> {
    let mut mapa: HashMap<u32, String> = HashMap::new();
    for linha in linhas {
        let Some((caminho, conteudo)) = linha.split_once(':') else {
            continue;
        };
        if let Some(pid) = pid_do_caminho(caminho) {
            let texto = mapa.entry(pid).or_default();
            texto.push_str(conteudo);
            texto.push('\n');
        }
    }
    mapa
}

/// As linhas de comando da saída do `head -v`: cada arquivo começa com
/// `==> /proc/<pid>/cmdline <==`.
fn das_cmdlines(texto: &str) -> HashMap<u32, String> {
    let mut mapa = HashMap::new();
    let mut atual: Option<(u32, String)> = None;
    for linha in texto.split('\n') {
        let cabecalho = linha
            .strip_prefix("==> ")
            .and_then(|r| r.strip_suffix(" <=="))
            .and_then(pid_do_caminho);
        if let Some(pid) = cabecalho {
            if let Some((pid, conteudo)) = atual.take() {
                mapa.insert(pid, da_cmdline(conteudo.as_bytes()));
            }
            atual = Some((pid, String::new()));
        } else if let Some((_, conteudo)) = atual.as_mut() {
            if !conteudo.is_empty() {
                conteudo.push('\n');
            }
            conteudo.push_str(linha);
        }
    }
    if let Some((pid, conteudo)) = atual {
        mapa.insert(pid, da_cmdline(conteudo.trim_end_matches('\n').as_bytes()));
    }
    // O `head` separa os arquivos com uma linha em branco: ela some no `da_cmdline`.
    mapa.into_iter()
        .map(|(pid, c)| (pid, c.trim().to_owned()))
        .collect()
}

/// A foto da saída do [`SCRIPT`].
pub fn da_saida_do_script(saida: &str) -> Result<Foto, String> {
    let mut secoes: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut atual = "";
    let mut cmdline = String::new();
    for linha in saida.split('\n') {
        if let Some(nome) = linha.strip_prefix('@').filter(|n| {
            matches!(
                *n,
                "sem-proc"
                    | "uptime"
                    | "cpus"
                    | "meminfo"
                    | "passwd"
                    | "stat"
                    | "status"
                    | "io"
                    | "netdev"
                    | "virtual"
                    | "diskstats"
                    | "blocos"
                    | "virtuais-bloco"
                    | "cmdline"
                    | "fim"
            )
        }) {
            atual = nome;
            secoes.entry(nome).or_default();
            continue;
        }
        if atual == "cmdline" {
            cmdline.push_str(linha);
            cmdline.push('\n');
        } else if !atual.is_empty() {
            secoes.entry(atual).or_default().push(linha);
        }
    }
    if secoes.contains_key("sem-proc") || !secoes.contains_key("stat") {
        return Err(t!("processos-so-linux"));
    }
    let juntar = |nome: &str| secoes.get(nome).map(|l| l.join("\n")).unwrap_or_default();
    let (memoria_total, memoria_disponivel) = da_meminfo(&juntar("meminfo"));
    let listagem = |nome: &str| nomes(secoes.get(nome).into_iter().flatten().copied());
    let virtuais = listagem("virtual");
    let discos = da_diskstats(
        &juntar("diskstats"),
        &listagem("blocos"),
        &listagem("virtuais-bloco"),
    );
    let arquivos = Arquivos {
        stats: secoes["stat"].iter().filter_map(|l| do_stat(l)).collect(),
        status: por_pid(secoes.get("status").map_or(&[][..], Vec::as_slice)),
        io: por_pid(secoes.get("io").map_or(&[][..], Vec::as_slice)),
        cmdline: das_cmdlines(&cmdline),
    };
    Ok(Foto {
        relogio: do_uptime(&juntar("uptime")).ok_or_else(|| t!("processos-so-linux"))?,
        nucleos: nucleos(&juntar("cpus")),
        cpus: tempos_das_cpus(&juntar("cpus")),
        memoria_total,
        memoria_disponivel,
        interfaces: da_net_dev(&juntar("netdev"), &virtuais),
        discos,
        processos: montar(arquivos, &usuarios(&juntar("passwd"))),
    })
}

/// A foto deste computador, lendo o `/proc` direto.
#[cfg(target_os = "linux")]
pub fn deste_computador(usuarios: &HashMap<u32, String>) -> Result<Foto, String> {
    use std::fs;
    let ler = |caminho: &str| fs::read_to_string(caminho).map_err(|e| e.to_string());
    let stat = ler("/proc/stat")?;
    let relogio = do_uptime(&ler("/proc/uptime")?).ok_or_else(|| t!("processos-so-linux"))?;
    let (memoria_total, memoria_disponivel) = da_meminfo(&ler("/proc/meminfo").unwrap_or_default());
    let mut arquivos = Arquivos::default();
    for entrada in fs::read_dir("/proc").map_err(|e| e.to_string())?.flatten() {
        let Some(pid) = entrada
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let pasta = entrada.path();
        // O processo pode acabar no meio da leitura: o que faltar fica de fora.
        let Some(stat) = fs::read_to_string(pasta.join("stat"))
            .ok()
            .and_then(|l| do_stat(&l))
        else {
            continue;
        };
        if let Ok(status) = fs::read_to_string(pasta.join("status")) {
            arquivos.status.insert(pid, status);
        }
        if let Ok(io) = fs::read_to_string(pasta.join("io")) {
            arquivos.io.insert(pid, io);
        }
        if let Ok(cmdline) = fs::read(pasta.join("cmdline")) {
            arquivos.cmdline.insert(pid, da_cmdline(&cmdline));
        }
        arquivos.stats.push(stat);
    }
    let pasta = |caminho: &str| -> HashSet<String> {
        fs::read_dir(caminho)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    };
    let virtuais = pasta("/sys/devices/virtual/net");
    let discos = da_diskstats(
        &ler("/proc/diskstats").unwrap_or_default(),
        &pasta("/sys/block"),
        &pasta("/sys/devices/virtual/block"),
    );
    Ok(Foto {
        relogio,
        nucleos: nucleos(&stat),
        cpus: tempos_das_cpus(&stat),
        memoria_total,
        memoria_disponivel,
        interfaces: da_net_dev(&ler("/proc/net/dev").unwrap_or_default(), &virtuais),
        discos,
        processos: montar(arquivos, usuarios),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    const STAT_BASH: &str = "4242 (meu (bash)) S 4000 4242 4242 34816 4300 4194304 1200 0 0 0 \
        150 50 0 0 20 0 1 0 987654 10000000 900 18446744073709551615 0 0 0 0 0 0 0 0 0 0 0 0 17 \
        3 0 0 0 0 0";

    #[test]
    fn le_o_stat_com_parentese_no_nome() {
        let s = do_stat(STAT_BASH).expect("o stat");
        assert_eq!(s.pid, 4242);
        assert_eq!(s.nome, "meu (bash)");
        assert_eq!(s.estado, Estado::Dormindo);
        assert_eq!(s.pai, 4000);
        assert_eq!(s.cpu, Duration::from_secs(2));
        assert_eq!(s.threads, 1);
        assert_eq!(s.inicio, 987_654);
        assert_eq!(do_stat("lixo"), None);
    }

    #[test]
    fn le_o_status_o_io_e_a_cmdline() {
        let status = "Name:\tbash\nUid:\t1000\t1000\t1000\t1000\nVmRSS:\t   5120 kB\n";
        assert_eq!(do_status(status), (Some(1000), Some(5 * 1024 * 1024)));
        assert_eq!(do_status("Name:\tkworker\n"), (None, None));
        let io = "rchar: 9\nwchar: 9\nread_bytes: 4096\nwrite_bytes: 8192\n";
        assert_eq!(do_io(io), Some((4096, 8192)));
        assert_eq!(da_cmdline(b"rsync\0-a\0/dados\0"), "rsync -a /dados");
    }

    #[test]
    fn le_os_numeros_da_maquina() {
        assert_eq!(
            do_uptime("1234.56 999.0\n"),
            Some(Duration::from_secs_f64(1234.56))
        );
        assert_eq!(nucleos("cpu  1 2 3\ncpu0 1 2\ncpu1 1 2\nintr 5\n"), 2);
        assert_eq!(
            da_meminfo("MemTotal:  8000 kB\nMemFree: 1 kB\nMemAvailable:  2000 kB\n"),
            (Some(8000 * 1024), Some(2000 * 1024))
        );
        let tempos = tempos_das_cpus("cpu  9 9 9 9\ncpu0 100 50 30 800 20 10 10 25 0 0\nintr 5\n");
        assert_eq!(
            tempos,
            vec![TemposDeCpu {
                usuario: Duration::from_millis(1750),
                sistema: Duration::from_millis(500),
                ocioso: Duration::from_millis(8200),
            }]
        );
        let nomes = usuarios("root:x:0:0:root:/root:/bin/bash\nana:1000\n");
        assert_eq!(nomes.get(&0).map(String::as_str), Some("root"));
        assert_eq!(nomes.get(&1000).map(String::as_str), Some("ana"));
    }

    #[test]
    fn le_as_interfaces_do_net_dev() {
        let net_dev = "Inter-|   Receive                            |  Transmit\n \
            face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets\n    \
            lo: 500 5 0 0 0 0 0 0 500 5 0 0 0 0 0 0\n  \
            eth0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0\n\
            docker0: 30 1 0 0 0 0 0 0 40 1 0 0 0 0 0 0\n";
        let virtuais = HashSet::from(["docker0".to_owned(), "lo".to_owned()]);
        let interfaces = da_net_dev(net_dev, &virtuais);
        assert_eq!(
            interfaces,
            vec![
                Interface {
                    nome: "eth0".into(),
                    recebidos: 1000,
                    enviados: 2000,
                    fisica: true,
                },
                Interface {
                    nome: "docker0".into(),
                    recebidos: 30,
                    enviados: 40,
                    fisica: false,
                },
            ]
        );
    }

    #[test]
    fn le_os_discos_inteiros_do_diskstats() {
        let diskstats = "   8       0 sda 100 5 2000 50 300 7 4000 90 0 120 140\n   \
            8       1 sda1 90 5 1800 40 280 7 3900 80 0 100 120\n   \
            7       0 loop0 10 0 20 1 0 0 0 0 0 1 1\n \
            253       0 dm-0 99 0 1990 49 299 0 3990 89 0 119 139\n \
            104       0 cciss/c0d0 1 0 8 0 2 0 16 0 0 0 0\n";
        let blocos = HashSet::from(["sda", "loop0", "dm-0", "cciss!c0d0"].map(str::to_owned));
        let virtuais = HashSet::from(["loop0", "dm-0"].map(str::to_owned));
        let discos = da_diskstats(diskstats, &blocos, &virtuais);
        assert_eq!(
            discos[0],
            Disco {
                nome: "sda".into(),
                leituras: 100,
                escritas: 300,
                lidos: 2000 * 512,
                escritos: 4000 * 512,
                fisico: true,
            }
        );
        let nomes: Vec<(&str, bool)> = discos.iter().map(|d| (d.nome.as_str(), d.fisico)).collect();
        assert_eq!(
            nomes,
            [("sda", true), ("dm-0", false), ("cciss/c0d0", true)]
        );
        // Sem a lista do `/sys/block`, as partições entram, mas os `loop` não.
        assert_eq!(da_diskstats(diskstats, &HashSet::new(), &virtuais).len(), 4);
    }

    #[test]
    fn le_a_saida_do_script_sem_as_threads_do_kernel() {
        let saida = "@uptime\n500.00 100.00\n@cpus\ncpu0 1\ncpu1 1\n@meminfo\n\
            MemTotal: 1000 kB\nMemAvailable: 400 kB\n@passwd\nroot:0\nana:1000\n@stat\n\
            1 (systemd) S 0 1 1 0 -1 0 0 0 0 0 10 5 0 0 20 0 1 0 2 0 0\n\
            2 (kthreadd) S 0 0 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 2 0 0\n\
            30 (kworker/0:1) I 2 0 0 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 3 0 0\n\
            4242 (bash) R 1 4242 4242 0 -1 0 0 0 0 0 100 100 0 0 20 0 2 0 900 0 0\n\
            @status\n/proc/1/status:Uid:\t0\t0\t0\t0\n/proc/1/status:VmRSS:\t 100 kB\n\
            /proc/4242/status:Uid:\t1000\t1000\t1000\t1000\n/proc/4242/status:VmRSS:\t 2048 kB\n\
            @io\n/proc/4242/io:read_bytes: 10\n/proc/4242/io:write_bytes: 20\n\
            @netdev\nface |bytes\n  eth0: 7 0 0 0 0 0 0 0 9 0 0 0 0 0 0 0\n\
            @virtual\nlo\n\
            @diskstats\n 8 0 sda 4 0 16 0 6 0 24 0 0 0 0\n 8 1 sda1 4 0 16 0 6 0 24 0 0 0 0\n\
            @blocos\nsda\n@virtuais-bloco\n\
            @cmdline\n==> /proc/1/cmdline <==\n/sbin/init\0splash\0\n\
            ==> /proc/4242/cmdline <==\n-bash\0\n\n@fim\n";
        let foto = da_saida_do_script(saida).expect("a foto");
        assert_eq!(foto.relogio, Duration::from_secs(500));
        assert_eq!(foto.nucleos, 2);
        assert_eq!(foto.cpus.len(), 2);
        assert_eq!(foto.memoria_total, Some(1000 * 1024));
        assert_eq!(foto.interfaces.len(), 1);
        assert_eq!(
            (foto.interfaces[0].recebidos, foto.interfaces[0].enviados),
            (7, 9)
        );
        assert!(foto.interfaces[0].fisica);
        assert_eq!(foto.discos.len(), 1);
        assert_eq!(
            (foto.discos[0].leituras, foto.discos[0].escritos),
            (4, 24 * 512)
        );
        assert!(foto.discos[0].fisico);
        let pids: Vec<u32> = foto.processos.iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![1, 4242]);
        let init = &foto.processos[0];
        assert_eq!(init.usuario, "root");
        assert_eq!(init.comando, "/sbin/init splash");
        assert_eq!(init.disco, None);
        let bash = &foto.processos[1];
        assert_eq!(bash.usuario, "ana");
        assert_eq!(bash.memoria, Some(2 * 1024 * 1024));
        assert_eq!(bash.disco, Some((10, 20)));
        assert_eq!(bash.threads, Some(2));
        assert_eq!(bash.cpu, Some(Duration::from_secs(2)));
        assert_eq!(bash.comando, "-bash");
    }

    #[test]
    fn sem_proc_e_erro() {
        assert!(da_saida_do_script("@sem-proc\n").is_err());
        assert!(da_saida_do_script("sh: cat: not found\n").is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn este_computador_tem_o_proprio_processo() {
        let foto = deste_computador(&HashMap::new()).expect("a foto");
        let eu = foto
            .processos
            .iter()
            .find(|p| p.pid == std::process::id())
            .expect("o próprio processo");
        assert!(eu.cpu.is_some());
        assert!(eu.memoria.unwrap_or(0) > 0);
        assert!(!eu.comando.is_empty());
        assert!(foto.nucleos >= 1);
        assert_eq!(foto.cpus.len(), foto.nucleos);
    }

    /// O script de verdade, no `sh` deste computador: a mesma leitura que um servidor faria.
    #[cfg(target_os = "linux")]
    #[test]
    fn o_script_roda_no_sh_e_acha_este_processo() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut filho = Command::new("sh")
            .arg("-s")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("o sh");
        filho
            .stdin
            .take()
            .expect("a entrada")
            .write_all(SCRIPT.as_bytes())
            .expect("o script");
        let saida = filho.wait_with_output().expect("a saída");
        let foto = da_saida_do_script(&String::from_utf8_lossy(&saida.stdout)).expect("a foto");
        let eu = foto
            .processos
            .iter()
            .find(|p| p.pid == std::process::id())
            .expect("o próprio processo");
        assert!(eu.memoria.unwrap_or(0) > 0);
        assert!(eu.disco.is_some());
        assert!(!eu.comando.is_empty());
        assert!(!eu.usuario.is_empty());
        assert!(!foto.cpus.is_empty());
        // Este computador já leu e escreveu num disco desde o boot.
        assert!(foto.discos.iter().any(|d| d.leituras > 0 && d.lidos > 0));
    }
}
