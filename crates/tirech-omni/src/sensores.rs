//! Os sensores de temperatura e as ventoinhas de uma máquina, para a aba Sensores do
//! Desempenho e o número do medidor da barra de título.
//!
//! No Linux, tudo vem do `/sys/class/hwmon` (sem root): este computador lê os arquivos direto
//! ([`deste_computador`]) e o servidor manda as mesmas linhas pela seção `@hwmon` do
//! [`SCRIPT`](crate::processos::proc_linux::SCRIPT) do Desempenho, no formato do `grep -H .` (`hwmon5/temp1_input:43000`). As duas leituras passam
//! pelo mesmo [`do_hwmon`], que classifica os chips por grupo, dá nomes legíveis e tira o lixo
//! (entradas sem nada ligado, que leem −10 °C ou 0 °C).
//!
//! As temperaturas ficam em milésimos de grau (`i32`), como o kernel dá, para a `Foto`
//! continuar `Eq`.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

/// O grupo de um sensor, na ordem em que a tela mostra.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grupo {
    Cpu,
    Memoria,
    Disco,
    PlacaMae,
    Video,
    #[default]
    Outros,
}

impl Grupo {
    pub const TODOS: [Grupo; 6] = [
        Grupo::Cpu,
        Grupo::Memoria,
        Grupo::Disco,
        Grupo::PlacaMae,
        Grupo::Video,
        Grupo::Outros,
    ];

    /// A chave dos ids e do `.ftl` (`desempenho-sensores-{chave}`).
    pub fn chave(self) -> &'static str {
        match self {
            Grupo::Cpu => "cpu",
            Grupo::Memoria => "memoria",
            Grupo::Disco => "disco",
            Grupo::PlacaMae => "placa-mae",
            Grupo::Video => "video",
            Grupo::Outros => "outros",
        }
    }

    /// O grupo de um chip do hwmon, pelo `name` dele.
    pub fn do_chip(chip: &str) -> Grupo {
        let comeca = |prefixos: &[&str]| prefixos.iter().any(|p| chip.starts_with(p));
        match chip {
            "coretemp" | "k10temp" | "zenpower" | "cpu_thermal" | "cpu-thermal" => Grupo::Cpu,
            "spd5118" | "jc42" => Grupo::Memoria,
            "nvme" | "drivetemp" => Grupo::Disco,
            "amdgpu" | "radeon" | "nouveau" => Grupo::Video,
            "acpitz" | "asus_ec_sensors" | "asus_wmi_sensors" | "gigabyte_wmi" | "dell_smm"
            | "thinkpad" => Grupo::PlacaMae,
            _ if comeca(&["nct", "it87", "it8", "w83", "f71"]) => Grupo::PlacaMae,
            _ => Grupo::Outros,
        }
    }
}

/// O nome de um sensor; a tela traduz os conhecidos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Nome {
    /// O pacote da CPU (`Package id N`); o número só com mais de um soquete.
    Pacote(Option<u32>),
    /// `Core N`.
    Nucleo(u32),
    /// Um pente de memória, contado a partir de 1.
    Pente(u32),
    /// O `SYSTIN` dos chips da placa-mãe.
    PlacaMae,
    /// O `CPUTIN`: o sensor da placa embaixo do soquete.
    Soquete,
    /// A zona térmica do ACPI (`acpitz`).
    ZonaAcpi,
    /// A bateria (`gas gauge battery` no macOS).
    Bateria,
    /// O que o sistema chama (o rótulo, o modelo do disco ou o nome do chip).
    Texto(String),
}

/// Um sensor de temperatura.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sensor {
    pub grupo: Grupo,
    /// O `name` do chip (`coretemp`, `nvme`…).
    pub chip: String,
    pub nome: Nome,
    /// A temperatura, em milésimos de grau Celsius.
    pub mili: i32,
    /// O limite de "quente" do chip (`max`).
    pub alto: Option<i32>,
    /// O limite crítico do chip (`crit`).
    pub critico: Option<i32>,
}

impl Sensor {
    pub fn celsius(&self) -> f64 {
        f64::from(self.mili) / 1000.0
    }

    /// Quão perto do limite está: o crítico passou, o alto passou ou nada.
    pub fn nivel(&self) -> Nivel {
        if self.critico.is_some_and(|c| self.mili >= c) {
            Nivel::Critico
        } else if self.alto.is_some_and(|a| self.mili >= a) {
            Nivel::Alto
        } else {
            Nivel::Normal
        }
    }
}

/// A faixa de uma temperatura em relação aos limites do chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Nivel {
    Normal,
    Alto,
    Critico,
}

/// Uma ventoinha.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ventoinha {
    pub chip: String,
    /// O `N` do `fanN_input`.
    pub numero: u32,
    /// O rótulo do chip, quando ele dá um.
    pub rotulo: Option<String>,
    pub rpm: u32,
}

/// Tudo o que os sensores de uma máquina dizem; vazio quando ela não tem nenhum.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sensores {
    /// O nome da CPU (`model name` do `/proc/cpuinfo`).
    pub cpu: Option<String>,
    pub temperaturas: Vec<Sensor>,
    pub ventoinhas: Vec<Ventoinha>,
}

impl Sensores {
    /// Os grupos que têm algum sensor, na ordem da tela.
    pub fn grupos(&self) -> Vec<Grupo> {
        Grupo::TODOS
            .into_iter()
            .filter(|g| self.temperaturas.iter().any(|s| s.grupo == *g))
            .collect()
    }

    /// Os sensores de um grupo.
    pub fn do_grupo(&self, grupo: Grupo) -> impl Iterator<Item = &Sensor> {
        self.temperaturas.iter().filter(move |s| s.grupo == grupo)
    }

    /// O sensor que resume o grupo: na CPU, o pacote (se houver); nos outros, o mais quente.
    pub fn principal(&self, grupo: Grupo) -> Option<&Sensor> {
        if grupo == Grupo::Cpu
            && let Some(pacote) = self
                .do_grupo(grupo)
                .filter(|s| matches!(s.nome, Nome::Pacote(_)))
                .max_by_key(|s| s.mili)
        {
            return Some(pacote);
        }
        self.do_grupo(grupo).max_by_key(|s| s.mili)
    }
}

/// A temperatura que o kernel dá, se faz sentido: fora o que lê ≤ 0 °C ou ≥ 150 °C (as
/// entradas sem nada ligado e os registros vazios).
fn plausivel(mili: i32) -> bool {
    mili > 0 && mili < 150_000
}

/// Os rótulos que não são um sensor de verdade: as entradas auxiliares dos chips da
/// placa-mãe, quase sempre sem nada ligado, e o PECI (a CPU de novo, calibrada).
fn descartado(rotulo: &str) -> bool {
    rotulo.starts_with("AUXTIN") || rotulo.starts_with("PECI Agent")
}

/// Os arquivos de um `tempN`: entrada, rótulo, max e crit.
type Temp = (Option<i32>, Option<String>, Option<i32>, Option<i32>);

/// O que um chip do hwmon mostra, juntado das linhas.
#[derive(Default)]
struct Chip {
    nome: String,
    modelo: Option<String>,
    /// Por número do `tempN`.
    temps: BTreeMap<u32, Temp>,
    /// Por número do `fanN`: entrada e rótulo.
    fans: BTreeMap<u32, (Option<u32>, Option<String>)>,
}

/// O número de `prefixoN_sufixo` (`temp12_input` → 12, `input`).
fn numerado<'a>(arquivo: &'a str, prefixo: &str) -> Option<(u32, &'a str)> {
    let resto = arquivo.strip_prefix(prefixo)?;
    let (numero, sufixo) = resto.split_once('_')?;
    Some((numero.parse().ok()?, sufixo))
}

/// Os sensores das linhas `hwmonN/arquivo:valor` (o caminho pode vir inteiro, de
/// `/sys/class/hwmon`), com o nome da CPU à parte.
pub fn do_hwmon(linhas: &str, cpu: Option<String>) -> Sensores {
    let mut chips: BTreeMap<u32, Chip> = BTreeMap::new();
    for linha in linhas.lines() {
        let Some((caminho, valor)) = linha.split_once(':') else {
            continue;
        };
        let mut partes = caminho.split('/').skip_while(|p| {
            p.strip_prefix("hwmon")
                .is_none_or(|n| n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()))
        });
        let Some(indice) = partes
            .next()
            .and_then(|p| p.strip_prefix("hwmon"))
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let arquivo = partes.collect::<Vec<_>>().join("/");
        let valor = valor.trim();
        let chip = chips.entry(indice).or_default();
        let numero = || valor.parse::<i32>().ok();
        match arquivo.as_str() {
            "name" => chip.nome = valor.to_owned(),
            "device/model" => chip.modelo = Some(valor.to_owned()).filter(|m| !m.is_empty()),
            _ => {
                if let Some((n, sufixo)) = numerado(&arquivo, "temp") {
                    let t = chip.temps.entry(n).or_default();
                    match sufixo {
                        "input" => t.0 = numero(),
                        "label" => t.1 = Some(valor.to_owned()),
                        "max" => t.2 = numero(),
                        "crit" => t.3 = numero(),
                        _ => {}
                    }
                } else if let Some((n, sufixo)) = numerado(&arquivo, "fan") {
                    let f = chip.fans.entry(n).or_default();
                    match sufixo {
                        "input" => f.0 = valor.parse().ok(),
                        "label" => f.1 = Some(valor.to_owned()),
                        _ => {}
                    }
                }
            }
        }
    }

    let pacotes = chips
        .values()
        .flat_map(|c| c.temps.values())
        .filter(|t| t.1.as_deref().is_some_and(|r| r.starts_with("Package id")))
        .count();
    let mut pentes = 0;
    let mut sensores = Sensores {
        cpu: cpu.filter(|c| !c.is_empty()),
        ..Sensores::default()
    };
    for chip in chips.into_values() {
        let grupo = Grupo::do_chip(&chip.nome);
        let e_pente = grupo == Grupo::Memoria;
        for (entrada, rotulo, max, crit) in chip.temps.into_values() {
            let Some(mili) = entrada else { continue };
            if rotulo.as_deref().is_some_and(descartado) {
                continue;
            }
            // O pente conta mesmo com a leitura ruim, para "Pente 3" ser sempre o terceiro.
            if e_pente {
                pentes += 1;
            }
            if !plausivel(mili) {
                continue;
            }
            let nome = nome_do_sensor(
                &chip.nome,
                chip.modelo.as_deref(),
                rotulo,
                grupo,
                pacotes,
                pentes,
            );
            sensores.temperaturas.push(Sensor {
                grupo,
                chip: chip.nome.clone(),
                nome,
                mili,
                alto: max.filter(|m| plausivel(*m)),
                critico: crit.filter(|c| plausivel(*c)),
            });
        }
        for (numero, (entrada, rotulo)) in chip.fans {
            if let Some(rpm) = entrada {
                sensores.ventoinhas.push(Ventoinha {
                    chip: chip.nome.clone(),
                    numero,
                    rotulo,
                    rpm,
                });
            }
        }
    }
    sensores
}

fn nome_do_sensor(
    chip: &str,
    modelo: Option<&str>,
    rotulo: Option<String>,
    grupo: Grupo,
    pacotes: usize,
    pente: u32,
) -> Nome {
    if grupo == Grupo::Memoria {
        return Nome::Pente(pente);
    }
    if chip == "acpitz" {
        return Nome::ZonaAcpi;
    }
    if grupo == Grupo::Disco {
        let modelo = modelo.unwrap_or(chip);
        return Nome::Texto(match rotulo.as_deref() {
            None | Some("Composite") => modelo.to_owned(),
            Some(r) => format!("{modelo} ({r})"),
        });
    }
    let Some(rotulo) = rotulo else {
        return if grupo == Grupo::Cpu {
            Nome::Pacote(None)
        } else {
            Nome::Texto(chip.to_owned())
        };
    };
    if let Some(n) = rotulo.strip_prefix("Package id ") {
        let n = n.trim().parse().ok();
        return Nome::Pacote(n.filter(|_| pacotes > 1));
    }
    if let Some(n) = rotulo
        .strip_prefix("Core ")
        .and_then(|n| n.trim().parse().ok())
    {
        return Nome::Nucleo(n);
    }
    match rotulo.as_str() {
        "SYSTIN" => Nome::PlacaMae,
        "CPUTIN" => Nome::Soquete,
        _ => Nome::Texto(rotulo),
    }
}

/// O nome da CPU no `/proc/cpuinfo`: o `model name` (x86), ou o `Hardware` ou o `Model` (ARM).
pub fn nome_da_cpu(cpuinfo: &str) -> Option<String> {
    let campo = |chave: &str| {
        cpuinfo.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            (k.trim() == chave)
                .then(|| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        })
    };
    campo("model name")
        .or_else(|| campo("Hardware"))
        .or_else(|| campo("Model"))
}

/// O grupo de um sensor do Apple Silicon, pelo nome que o sistema dá; `None` descarta (o
/// `tcal` é a constante de calibração do PMU, não uma leitura).
fn grupo_do_macos(nome: &str) -> Option<Grupo> {
    let sensor = nome.split_whitespace().nth(1).unwrap_or(nome);
    let comeca = |prefixos: &[&str]| prefixos.iter().any(|p| nome.starts_with(p));
    Some(if sensor == "tcal" {
        return None;
    } else if nome.starts_with("PMU") && sensor.starts_with("tdie") {
        Grupo::Cpu
    } else if nome.starts_with("PMU") && sensor.starts_with("tdev") {
        Grupo::PlacaMae
    } else if comeca(&["pACC", "eACC", "SOC"]) {
        Grupo::Cpu
    } else if comeca(&["GPU"]) {
        Grupo::Video
    } else if comeca(&["NAND"]) {
        Grupo::Disco
    } else {
        Grupo::Outros
    })
}

/// O nome com os números em ordem (`PMU tdie2` antes de `PMU tdie11`).
fn ordem_natural(nome: &str) -> Vec<(String, u64)> {
    let mut partes = Vec::new();
    let mut texto = String::new();
    let mut numero: Option<u64> = None;
    for c in nome.chars() {
        if let Some(d) = c.to_digit(10) {
            numero = Some(numero.unwrap_or(0).saturating_mul(10) + u64::from(d));
        } else {
            if let Some(n) = numero.take() {
                partes.push((std::mem::take(&mut texto), n));
            }
            texto.push(c);
        }
    }
    partes.push((texto, numero.unwrap_or(0)));
    partes
}

/// Os sensores do macOS: a lista (nome, °C) do `IOHIDEventSystemClient`, com o nome da CPU
/// à parte. Os `PMU tdie*` (o die do chip) são a CPU; os `PMU tdev*`, a placa em volta dele;
/// `NAND*`, o disco; `pACC`/`eACC`/`SOC`, a CPU; `GPU`, o vídeo; a bateria e o resto, Outros.
/// Os nomes repetidos ganham " (2)", " (3)"…
pub fn do_macos(lista: &[(String, f64)], cpu: Option<String>) -> Sensores {
    let mut vistos: BTreeMap<&str, u32> = BTreeMap::new();
    let mut temperaturas: Vec<(Vec<(String, u64)>, Sensor)> = lista
        .iter()
        .filter_map(|(nome, celsius)| {
            let grupo = grupo_do_macos(nome)?;
            // Fora do `i32` (ou NaN), o `as` satura (ou dá 0), e o filtro descarta.
            let mili = (celsius * 1000.0).round() as i32;
            if !plausivel(mili) {
                return None;
            }
            let vez = vistos.entry(nome).or_insert(0);
            *vez += 1;
            let rotulo = if nome == "gas gauge battery" {
                Nome::Bateria
            } else if *vez > 1 {
                Nome::Texto(format!("{nome} ({vez})"))
            } else {
                Nome::Texto(nome.clone())
            };
            let chip = nome.split_whitespace().next().unwrap_or(nome).to_owned();
            let sensor = Sensor {
                grupo,
                chip,
                nome: rotulo,
                mili,
                alto: None,
                critico: None,
            };
            // A primeira palavra antes do resto: `PMU tdie2` antes de `PMU2 tdie1`.
            let (primeira, resto) = nome.split_once(' ').unwrap_or((nome, ""));
            let mut ordem = ordem_natural(primeira);
            ordem.extend(ordem_natural(resto));
            Some((ordem, sensor))
        })
        .collect();
    temperaturas.sort_by(|a, b| a.1.grupo.cmp(&b.1.grupo).then_with(|| a.0.cmp(&b.0)));
    Sensores {
        cpu,
        temperaturas: temperaturas.into_iter().map(|(_, s)| s).collect(),
        ventoinhas: Vec::new(),
    }
}

/// Uma temperatura do Windows, como o `tomni_ui_win32::sensors` dá (aqui para os testes rodarem
/// em qualquer sistema).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TemperaturaDoWindows {
    /// O identificador: `/storage/N`, `/thermalzone/<instância>` ou o do LibreHardwareMonitor
    /// (`/intelcpu/0/temperature/0`).
    pub caminho: String,
    /// O aparelho (o modelo do disco, a zona, o `Hardware` do LibreHardwareMonitor).
    pub aparelho: String,
    /// O nome do sensor; vazio quando o aparelho tem um só.
    pub nome: String,
    pub celsius: f64,
    pub alto: Option<f64>,
    pub critico: Option<f64>,
}

/// Uma ventoinha do Windows (só com o LibreHardwareMonitor).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VentoinhaDoWindows {
    pub caminho: String,
    pub aparelho: String,
    pub nome: String,
    pub rpm: f64,
}

/// O grupo de um sensor do Windows, pelo primeiro trecho do caminho (o tipo de aparelho no
/// LibreHardwareMonitor).
fn grupo_do_windows(caminho: &str) -> Grupo {
    let tipo = caminho
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("");
    match tipo {
        "intelcpu" | "amdcpu" | "cpu" => Grupo::Cpu,
        "ram" | "memory" | "dimm" => Grupo::Memoria,
        "nvme" | "hdd" | "ssd" | "storage" => Grupo::Disco,
        "lpc" | "motherboard" | "mainboard" | "thermalzone" => Grupo::PlacaMae,
        _ if tipo.contains("gpu") => Grupo::Video,
        _ => Grupo::Outros,
    }
}

/// O número de um nome como `Core #3` ou `Fan #2`.
fn numero_do_nome(nome: &str, prefixo: &str) -> Option<u32> {
    nome.strip_prefix(prefixo)?.trim().parse().ok()
}

/// Os sensores do Windows: o que o LibreHardwareMonitor dá (quando está aberto), os discos e as
/// zonas térmicas do ACPI. Os grupos vêm do caminho; os nomes conhecidos (`CPU Package`,
/// `Core #N`, `System`, `CPU Socket`) viram os do Linux, para a tela traduzir. Os discos do
/// sistema saem quando o LibreHardwareMonitor já deu os dele, e as "Distance to TjMax" (a folga
/// até o limite, não uma temperatura) saem sempre.
pub fn do_windows(
    temperaturas: &[TemperaturaDoWindows],
    ventoinhas: &[VentoinhaDoWindows],
    cpu: Option<String>,
) -> Sensores {
    let do_monitor = |t: &&TemperaturaDoWindows| {
        !t.caminho.starts_with("/storage/") && !t.caminho.starts_with("/thermalzone/")
    };
    let discos_do_monitor = temperaturas
        .iter()
        .filter(do_monitor)
        .any(|t| grupo_do_windows(&t.caminho) == Grupo::Disco);
    let processadores: BTreeMap<&str, ()> = temperaturas
        .iter()
        .filter(|t| grupo_do_windows(&t.caminho) == Grupo::Cpu)
        .filter_map(|t| t.caminho.trim_start_matches('/').split('/').nth(1))
        .map(|n| (n, ()))
        .collect();
    let mili = |celsius: f64| {
        let mili = (celsius * 1000.0).round();
        (mili.is_finite() && plausivel(mili as i32)).then_some(mili as i32)
    };
    let mut pentes = 0;
    let mut lista: Vec<Sensor> = Vec::new();
    for t in temperaturas {
        if t.nome.contains("Distance to TjMax")
            || (discos_do_monitor && t.caminho.starts_with("/storage/"))
        {
            continue;
        }
        let Some(graus) = mili(t.celsius) else {
            continue;
        };
        let grupo = grupo_do_windows(&t.caminho);
        let chip = if t.aparelho.is_empty() {
            t.caminho
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap_or("")
                .to_owned()
        } else {
            t.aparelho.clone()
        };
        let texto = || {
            Nome::Texto(if t.nome.is_empty() {
                chip.clone()
            } else {
                t.nome.clone()
            })
        };
        let nome = match grupo {
            Grupo::Cpu => match t.nome.as_str() {
                "CPU Package" | "Core (Tctl/Tdie)" | "Core (Tctl)" | "Core (Tdie)" | "" => {
                    let n = t.caminho.trim_start_matches('/').split('/').nth(1);
                    Nome::Pacote(
                        n.and_then(|n| n.parse().ok())
                            .filter(|_| processadores.len() > 1),
                    )
                }
                nome => numero_do_nome(nome, "Core #")
                    .or_else(|| numero_do_nome(nome, "CPU Core #"))
                    .map_or_else(texto, Nome::Nucleo),
            },
            Grupo::Memoria => {
                pentes += 1;
                Nome::Pente(pentes)
            }
            Grupo::Disco => Nome::Texto(match t.nome.as_str() {
                "" | "Temperature" | "Composite Temperature" => chip.clone(),
                nome => format!("{chip} ({nome})"),
            }),
            Grupo::PlacaMae if t.caminho.starts_with("/thermalzone/") => Nome::ZonaAcpi,
            Grupo::PlacaMae => match t.nome.as_str() {
                "System" | "Motherboard" | "SYSTIN" => Nome::PlacaMae,
                "CPU Socket" | "CPUTIN" => Nome::Soquete,
                _ => texto(),
            },
            Grupo::Video | Grupo::Outros => texto(),
        };
        lista.push(Sensor {
            grupo,
            chip,
            nome,
            mili: graus,
            alto: t.alto.and_then(mili),
            critico: t.critico.and_then(mili),
        });
    }
    lista.sort_by_key(|s| s.grupo);
    let mut por_aparelho: BTreeMap<String, u32> = BTreeMap::new();
    let ventoinhas = ventoinhas
        .iter()
        .filter(|v| v.rpm.is_finite() && v.rpm >= 0.0)
        .map(|v| {
            let chip = if v.aparelho.is_empty() {
                v.caminho
                    .trim_start_matches('/')
                    .split('/')
                    .next()
                    .unwrap_or("")
                    .to_owned()
            } else {
                v.aparelho.clone()
            };
            let contagem = por_aparelho.entry(chip.clone()).or_insert(0);
            *contagem += 1;
            let numero = numero_do_nome(&v.nome, "Fan #");
            Ventoinha {
                numero: numero.unwrap_or(*contagem),
                rotulo: numero
                    .is_none()
                    .then(|| v.nome.clone())
                    .filter(|r| !r.is_empty()),
                chip,
                rpm: v.rpm.round() as u32,
            }
        })
        .collect();
    Sensores {
        cpu,
        temperaturas: lista,
        ventoinhas,
    }
}

/// Os arquivos do hwmon que a leitura usa (o resto da pasta fica de fora).
#[cfg(target_os = "linux")]
fn interessa(arquivo: &str) -> bool {
    arquivo == "name"
        || numerado(arquivo, "temp")
            .is_some_and(|(_, s)| matches!(s, "input" | "label" | "max" | "crit"))
        || numerado(arquivo, "fan").is_some_and(|(_, s)| matches!(s, "input" | "label"))
}

/// Os sensores deste computador: no Linux, do `/sys/class/hwmon`; no macOS, do
/// `tomni_ui_macos` (o Apple Silicon; um Mac com Intel não mostra nenhum); no Windows, do
/// `tomni_ui_win32` (os discos, as zonas térmicas e o LibreHardwareMonitor, se estiver aberto).
/// Vazio sem permissão.
pub fn deste_computador() -> Sensores {
    #[cfg(target_os = "linux")]
    {
        use std::fmt::Write;
        use std::fs;
        let mut linhas = String::new();
        for entrada in fs::read_dir("/sys/class/hwmon")
            .into_iter()
            .flatten()
            .flatten()
        {
            let Some(pasta) = entrada.file_name().into_string().ok() else {
                continue;
            };
            let caminho = entrada.path();
            for arquivo in fs::read_dir(&caminho).into_iter().flatten().flatten() {
                let Some(nome) = arquivo
                    .file_name()
                    .into_string()
                    .ok()
                    .filter(|n| interessa(n))
                else {
                    continue;
                };
                if let Ok(valor) = fs::read_to_string(arquivo.path()) {
                    let _ = writeln!(linhas, "{pasta}/{nome}:{}", valor.trim_end());
                }
            }
            if let Ok(modelo) = fs::read_to_string(caminho.join("device/model")) {
                let _ = writeln!(linhas, "{pasta}/device/model:{}", modelo.trim_end());
            }
        }
        let cpu = fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|c| nome_da_cpu(&c));
        do_hwmon(&linhas, cpu)
    }
    #[cfg(target_os = "macos")]
    {
        let lista: Vec<(String, f64)> = tomni_ui_macos::temperatures()
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.name, t.celsius))
            .collect();
        do_macos(&lista, tomni_ui_macos::cpu_brand())
    }
    #[cfg(windows)]
    {
        let lidos = tomni_ui_win32::sensors();
        let temperaturas: Vec<TemperaturaDoWindows> = lidos
            .temperatures
            .into_iter()
            .map(|t| TemperaturaDoWindows {
                caminho: t.path,
                aparelho: t.hardware,
                nome: t.name,
                celsius: t.celsius,
                alto: t.high,
                critico: t.critical,
            })
            .collect();
        let ventoinhas: Vec<VentoinhaDoWindows> = lidos
            .fans
            .into_iter()
            .map(|v| VentoinhaDoWindows {
                caminho: v.path,
                aparelho: v.hardware,
                nome: v.name,
                rpm: v.rpm,
            })
            .collect();
        do_windows(&temperaturas, &ventoinhas, tomni_ui_win32::cpu_name())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        Sensores::default()
    }
}

/// Os sensores dos testes de tela: a CPU (o pacote acima do alto e dois núcleos), dois pentes,
/// um disco, a placa-mãe com uma ventoinha que gira e outra parada, e nada de vídeo nem de
/// outros. `quente` põe o pacote acima do crítico.
pub fn exemplo(quente: bool) -> Sensores {
    let sensor =
        |grupo, chip: &str, nome, graus: i32, alto: Option<i32>, critico: Option<i32>| Sensor {
            grupo,
            chip: chip.to_owned(),
            nome,
            mili: graus * 1000,
            alto: alto.map(|a| a * 1000),
            critico: critico.map(|c| c * 1000),
        };
    let pacote = if quente { 101 } else { 86 };
    Sensores {
        cpu: Some("Processador de Teste 8 núcleos".to_owned()),
        temperaturas: vec![
            sensor(
                Grupo::Cpu,
                "coretemp",
                Nome::Pacote(None),
                pacote,
                Some(80),
                Some(100),
            ),
            sensor(
                Grupo::Cpu,
                "coretemp",
                Nome::Nucleo(0),
                70,
                Some(80),
                Some(100),
            ),
            sensor(
                Grupo::Cpu,
                "coretemp",
                Nome::Nucleo(1),
                64,
                Some(80),
                Some(100),
            ),
            sensor(
                Grupo::Memoria,
                "spd5118",
                Nome::Pente(1),
                50,
                Some(55),
                Some(85),
            ),
            sensor(
                Grupo::Memoria,
                "spd5118",
                Nome::Pente(2),
                51,
                Some(55),
                Some(85),
            ),
            sensor(
                Grupo::Disco,
                "nvme",
                Nome::Texto("Disco de Teste".to_owned()),
                59,
                Some(77),
                Some(79),
            ),
            sensor(Grupo::PlacaMae, "nct6798", Nome::PlacaMae, 43, None, None),
            sensor(Grupo::PlacaMae, "nct6798", Nome::Soquete, 48, None, None),
        ],
        ventoinhas: vec![
            Ventoinha {
                chip: "nct6798".to_owned(),
                numero: 1,
                rotulo: None,
                rpm: 1200,
            },
            Ventoinha {
                chip: "nct6798".to_owned(),
                numero: 2,
                rotulo: None,
                rpm: 0,
            },
        ],
    }
}

/// O limite do aviso de calor sem o `crit` do sensor: 90 °C.
pub const LIMITE_SEM_CRITICO: i32 = 90_000;
/// Quanto abaixo do `crit` o aviso dispara: 10 °C.
pub const FOLGA_DO_CRITICO: i32 = 10_000;
/// Quanto a temperatura precisa descer abaixo do limite para o aviso valer de novo: 5 °C.
pub const HISTERESE: i32 = 5_000;
/// O intervalo mínimo entre dois avisos: 10 minutos.
pub const INTERVALO_DO_AVISO: Duration = Duration::from_secs(10 * 60);
/// De quanto em quanto tempo o [`LeitorDeSensores`] lê.
pub const RITMO: Duration = Duration::from_secs(5);

/// O limite do aviso de calor para `cpu`, em milésimos de grau: o `configurado` (em °C), se
/// não é zero; senão o `crit` do sensor menos [`FOLGA_DO_CRITICO`], ou [`LIMITE_SEM_CRITICO`].
pub fn limite_de_calor(cpu: &Sensor, configurado: u32) -> i32 {
    if configurado > 0 {
        return i32::try_from(configurado).map_or(i32::MAX, |c| c.saturating_mul(1000));
    }
    cpu.critico
        .map_or(LIMITE_SEM_CRITICO, |c| c - FOLGA_DO_CRITICO)
}

/// Quando avisar do calor: ao passar do limite; de novo, só depois de descer [`HISTERESE`]
/// abaixo dele e de [`INTERVALO_DO_AVISO`] desde o último aviso.
#[derive(Debug, Default)]
pub struct AvisoDeCalor {
    /// Já avisou desta vez (e a temperatura ainda não desceu o bastante).
    avisado: bool,
    /// A hora do último aviso.
    ultimo: Option<Instant>,
}

impl AvisoDeCalor {
    /// Olha a temperatura `mili` contra o `limite` às `agora`: `true` se é hora de avisar.
    pub fn ver(&mut self, mili: i32, limite: i32, agora: Instant) -> bool {
        if mili <= limite - HISTERESE {
            self.avisado = false;
            return false;
        }
        let descansou = self
            .ultimo
            .is_none_or(|u| agora.saturating_duration_since(u) >= INTERVALO_DO_AVISO);
        if mili < limite || self.avisado || !descansou {
            return false;
        }
        self.avisado = true;
        self.ultimo = Some(agora);
        true
    }
}

/// A leitura leve dos sensores deste computador, numa thread: lê na hora e depois a cada
/// `ritmo`, guarda só a mais nova e chama `acordar` a cada entrega. Para ao ser descartado.
pub struct LeitorDeSensores {
    parar: Sender<()>,
    saida: Arc<Mutex<Option<Sensores>>>,
}

impl LeitorDeSensores {
    pub fn novo(
        ler: Arc<dyn Fn() -> Sensores + Send + Sync>,
        ritmo: Duration,
        acordar: impl Fn() + Send + 'static,
    ) -> Self {
        let (parar, parada) = mpsc::channel::<()>();
        let saida: Arc<Mutex<Option<Sensores>>> = Arc::default();
        let entrega = Arc::clone(&saida);
        let _ = thread::Builder::new()
            .name("sensores".to_owned())
            .spawn(move || {
                loop {
                    let sensores = ler();
                    *entrega.lock().unwrap_or_else(PoisonError::into_inner) = Some(sensores);
                    acordar();
                    if !matches!(parada.recv_timeout(ritmo), Err(RecvTimeoutError::Timeout)) {
                        break;
                    }
                }
            });
        Self { parar, saida }
    }

    /// A leitura mais nova desde a última vez (`None` se não veio outra).
    pub fn recolher(&self) -> Option<Sensores> {
        self.saida
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}

impl Drop for LeitorDeSensores {
    fn drop(&mut self) {
        // A thread sai ao ver o pedido (ou o canal fechado) no fim da espera.
        let _ = self.parar.send(());
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// A saída do `grep` numa máquina de verdade (i5-14600K, placa ASUS com o nct6798, quatro
    /// pentes DDR5 e um NVMe), em 04/11/2026, com parte dos núcleos.
    const DESTA_MAQUINA: &str = "\
hwmon0/name:acpitz
hwmon0/temp1_input:27800
hwmon1/device/model:KINGSTON SNV2S1000G
hwmon1/name:nvme
hwmon1/temp1_crit:78850
hwmon1/temp1_input:58850
hwmon1/temp1_label:Composite
hwmon1/temp1_max:76850
hwmon1/temp3_input:68850
hwmon1/temp3_label:Sensor 2
hwmon2/name:coretemp
hwmon2/temp1_crit:100000
hwmon2/temp1_input:91000
hwmon2/temp1_label:Package id 0
hwmon2/temp1_max:80000
hwmon2/temp2_crit:100000
hwmon2/temp2_input:73000
hwmon2/temp2_label:Core 0
hwmon2/temp2_max:80000
hwmon2/temp33_crit:100000
hwmon2/temp33_input:76000
hwmon2/temp33_label:Core 31
hwmon2/temp33_max:80000
hwmon3/name:spd5118
hwmon3/temp1_crit:85000
hwmon3/temp1_input:52000
hwmon3/temp1_max:55000
hwmon4/name:spd5118
hwmon4/temp1_crit:85000
hwmon4/temp1_input:51000
hwmon4/temp1_max:55000
hwmon5/fan1_input:0
hwmon5/fan2_input:0
hwmon5/name:nct6798
hwmon5/temp1_crit:125000
hwmon5/temp1_input:43000
hwmon5/temp1_label:SYSTIN
hwmon5/temp1_max:80000
hwmon5/temp2_crit:125000
hwmon5/temp2_input:64500
hwmon5/temp2_label:CPUTIN
hwmon5/temp2_max:80000
hwmon5/temp3_input:26000
hwmon5/temp3_label:AUXTIN0
hwmon5/temp5_input:-11000
hwmon5/temp5_label:AUXTIN2
hwmon5/temp8_input:77000
hwmon5/temp8_label:PECI Agent 0 Calibration
hwmon5/temp9_input:0
hwmon5/temp9_label:PCH_CHIP_CPU_MAX_TEMP
hwmon6/name:spd5118
hwmon6/temp1_crit:85000
hwmon6/temp1_input:50750
hwmon6/temp1_max:55000
hwmon7/name:spd5118
hwmon7/temp1_crit:85000
hwmon7/temp1_input:50500
hwmon7/temp1_max:55000
hwmon8/name:iwlwifi_1
hwmon8/temp1_input:52000
hwmon9/name:asus
hwmon10/name:asus
";

    fn nomes(s: &Sensores, grupo: Grupo) -> Vec<(Nome, i32)> {
        s.do_grupo(grupo)
            .map(|s| (s.nome.clone(), s.mili))
            .collect()
    }

    #[test]
    fn a_maquina_de_verdade_vira_os_grupos() {
        let s = do_hwmon(DESTA_MAQUINA, Some("Intel(R) Core(TM) i5-14600K".into()));
        assert_eq!(s.cpu.as_deref(), Some("Intel(R) Core(TM) i5-14600K"));
        assert_eq!(
            s.grupos(),
            vec![
                Grupo::Cpu,
                Grupo::Memoria,
                Grupo::Disco,
                Grupo::PlacaMae,
                Grupo::Outros
            ]
        );
        assert_eq!(
            nomes(&s, Grupo::Cpu),
            vec![
                (Nome::Pacote(None), 91000),
                (Nome::Nucleo(0), 73000),
                (Nome::Nucleo(31), 76000)
            ]
        );
        assert_eq!(
            nomes(&s, Grupo::Memoria),
            vec![
                (Nome::Pente(1), 52000),
                (Nome::Pente(2), 51000),
                (Nome::Pente(3), 50750),
                (Nome::Pente(4), 50500)
            ]
        );
        assert_eq!(
            nomes(&s, Grupo::Disco),
            vec![
                (Nome::Texto("KINGSTON SNV2S1000G".into()), 58850),
                (Nome::Texto("KINGSTON SNV2S1000G (Sensor 2)".into()), 68850)
            ]
        );
        // Sem os AUXTIN, o PECI e os PCH em 0; a zona do ACPI fica com a placa-mãe.
        assert_eq!(
            nomes(&s, Grupo::PlacaMae),
            vec![
                (Nome::ZonaAcpi, 27800),
                (Nome::PlacaMae, 43000),
                (Nome::Soquete, 64500)
            ]
        );
        assert_eq!(
            nomes(&s, Grupo::Outros),
            vec![(Nome::Texto("iwlwifi_1".into()), 52000)]
        );
        // A ordem é a numérica do hwmon (o hwmon10 depois do hwmon9).
        assert_eq!(s.ventoinhas.len(), 2);
        assert!(
            s.ventoinhas
                .iter()
                .all(|v| v.rpm == 0 && v.chip == "nct6798")
        );
    }

    #[test]
    fn os_limites_e_o_principal() {
        let s = do_hwmon(DESTA_MAQUINA, None);
        let pacote = s.principal(Grupo::Cpu).unwrap();
        assert_eq!(pacote.nome, Nome::Pacote(None));
        assert_eq!((pacote.alto, pacote.critico), (Some(80000), Some(100000)));
        assert_eq!(pacote.nivel(), Nivel::Alto);
        assert_eq!(pacote.celsius(), 91.0);
        // Fora da CPU, o mais quente: o Sensor 2 do NVMe.
        assert_eq!(s.principal(Grupo::Disco).unwrap().mili, 68850);
        assert_eq!(s.principal(Grupo::Video), None);
        let systin = s.do_grupo(Grupo::PlacaMae).nth(1).unwrap();
        assert_eq!(systin.nivel(), Nivel::Normal);
        // A zona do ACPI não tem limites.
        assert_eq!(s.principal(Grupo::PlacaMae).unwrap().nome, Nome::Soquete);
    }

    #[test]
    fn dois_soquetes_numeram_o_pacote_e_o_caminho_inteiro_serve() {
        let s = do_hwmon(
            "/sys/class/hwmon/hwmon3/name:coretemp\n\
             /sys/class/hwmon/hwmon3/temp1_input:50000\n\
             /sys/class/hwmon/hwmon3/temp1_label:Package id 0\n\
             /sys/class/hwmon/hwmon4/name:coretemp\n\
             /sys/class/hwmon/hwmon4/temp1_input:55000\n\
             /sys/class/hwmon/hwmon4/temp1_label:Package id 1\n",
            None,
        );
        assert_eq!(
            nomes(&s, Grupo::Cpu),
            vec![
                (Nome::Pacote(Some(0)), 50000),
                (Nome::Pacote(Some(1)), 55000)
            ]
        );
        assert_eq!(s.principal(Grupo::Cpu).unwrap().mili, 55000);
    }

    #[test]
    fn amd_raspberry_e_placa_de_video() {
        let s = do_hwmon(
            "hwmon0/name:k10temp\nhwmon0/temp1_input:61250\nhwmon0/temp1_label:Tctl\n\
             hwmon1/name:cpu_thermal\nhwmon1/temp1_input:48000\n\
             hwmon2/name:amdgpu\nhwmon2/temp1_input:40000\nhwmon2/temp1_label:edge\n\
             hwmon2/temp1_crit:200000\nhwmon2/fan1_input:812\n",
            None,
        );
        assert_eq!(
            nomes(&s, Grupo::Cpu),
            vec![
                (Nome::Texto("Tctl".into()), 61250),
                (Nome::Pacote(None), 48000)
            ]
        );
        let video = s.principal(Grupo::Video).unwrap();
        assert_eq!(video.nome, Nome::Texto("edge".into()));
        // Um crítico de 200 °C não é um limite de verdade.
        assert_eq!(video.critico, None);
        assert_eq!(s.ventoinhas[0].rpm, 812);
    }

    #[test]
    fn sem_hwmon_fica_vazio() {
        assert_eq!(do_hwmon("", None), Sensores::default());
        assert_eq!(
            do_hwmon("lixo\nhwmonX/name:a\n", Some(String::new())),
            Sensores::default()
        );
    }

    #[test]
    fn o_nome_da_cpu() {
        assert_eq!(
            nome_da_cpu("processor\t: 0\nmodel name\t: AMD Ryzen 9 7950X\n").as_deref(),
            Some("AMD Ryzen 9 7950X")
        );
        assert_eq!(
            nome_da_cpu("Hardware\t: BCM2835\nModel\t\t: Raspberry Pi 4 Model B\n").as_deref(),
            Some("BCM2835")
        );
        assert_eq!(nome_da_cpu("processor : 0\n"), None);
    }

    /// O que o `IOHIDEventSystemClient` deu no Mac do usuário (Apple M6, 04/11), na ordem do
    /// sistema, com a bateria e um núcleo de outros modelos.
    const DO_M6: &[(&str, f64)] = &[
        ("PMU tdie11", 28.9),
        ("NAND CH0 temp", 28.0),
        ("PMU tcal", 51.8),
        ("PMU tdev1", 29.1),
        ("PMU tdie2", 28.3),
        ("PMU tdie1", 27.5),
        ("PMU2 tdie1", 28.0),
        ("PMU tdie1", 28.6),
        ("PMU2 tdev1", 27.7),
        ("gas gauge battery", 31.0),
        ("pACC MTR Temp Sensor2", 41.5),
        ("GPU MTR Temp Sensor1", 35.0),
        ("PMU2 tcal", 51.8),
        ("PMU tdie9", 0.0),
    ];

    #[test]
    fn os_sensores_do_mac_vao_para_os_grupos() {
        let lista: Vec<(String, f64)> = DO_M6.iter().map(|(n, c)| ((*n).to_owned(), *c)).collect();
        let s = do_macos(&lista, Some("Apple M6".to_owned()));
        assert_eq!(s.cpu.as_deref(), Some("Apple M6"));
        let nomes = |g| {
            s.do_grupo(g)
                .map(|t| match &t.nome {
                    Nome::Texto(n) => format!("{n}={}", t.mili),
                    outro => format!("{outro:?}={}", t.mili),
                })
                .collect::<Vec<_>>()
        };
        // O `tcal` (a calibração) e o que lê 0 °C somem; os repetidos ganham o número; os
        // números ficam em ordem.
        assert_eq!(
            nomes(Grupo::Cpu),
            [
                "PMU tdie1=27500",
                "PMU tdie1 (2)=28600",
                "PMU tdie2=28300",
                "PMU tdie11=28900",
                "PMU2 tdie1=28000",
                "pACC MTR Temp Sensor2=41500",
            ]
        );
        assert_eq!(
            nomes(Grupo::PlacaMae),
            ["PMU tdev1=29100", "PMU2 tdev1=27700"]
        );
        assert_eq!(nomes(Grupo::Disco), ["NAND CH0 temp=28000"]);
        assert_eq!(nomes(Grupo::Video), ["GPU MTR Temp Sensor1=35000"]);
        assert_eq!(nomes(Grupo::Outros), ["Bateria=31000"]);
        assert_eq!(nomes(Grupo::Memoria), Vec::<String>::new());
        // Sem pacote, a CPU se resume no mais quente, sem limites (o aviso fica nos 90 °C).
        let cpu = s.principal(Grupo::Cpu).unwrap();
        assert_eq!(cpu.mili, 41_500);
        assert_eq!(cpu.alto, None);
        assert_eq!(limite_de_calor(cpu, 0), 90_000);
        assert!(s.ventoinhas.is_empty());
    }

    /// Uma temperatura do Windows para os testes.
    fn windows(caminho: &str, aparelho: &str, nome: &str, celsius: f64) -> TemperaturaDoWindows {
        TemperaturaDoWindows {
            caminho: caminho.to_owned(),
            aparelho: aparelho.to_owned(),
            nome: nome.to_owned(),
            celsius,
            alto: None,
            critico: None,
        }
    }

    fn nomes_por_grupo(s: &Sensores, g: Grupo) -> Vec<String> {
        s.do_grupo(g)
            .map(|t| match &t.nome {
                Nome::Texto(n) => format!("{n}={}", t.mili),
                outro => format!("{outro:?}={}", t.mili),
            })
            .collect()
    }

    #[test]
    fn os_sensores_do_windows_com_o_monitor_de_hardware() {
        let i5 = "Intel Core i5-14600K";
        let placa = "Nuvoton NCT6798D";
        let ssd = "Samsung SSD 990 PRO 2TB";
        let temperaturas = [
            windows("/intelcpu/0/temperature/0", i5, "Core #1", 51.0),
            windows("/intelcpu/0/temperature/1", i5, "Core #2", 55.0),
            windows(
                "/intelcpu/0/temperature/14",
                i5,
                "CPU Core #1 Distance to TjMax",
                49.0,
            ),
            windows("/intelcpu/0/temperature/16", i5, "CPU Package", 58.0),
            windows("/intelcpu/0/temperature/17", i5, "Core Max", 55.0),
            windows("/lpc/nct6798d/0/temperature/0", placa, "CPU Core", 57.0),
            windows("/lpc/nct6798d/0/temperature/1", placa, "System", 34.0),
            windows("/lpc/nct6798d/0/temperature/2", placa, "CPU Socket", 38.0),
            windows(
                "/lpc/nct6798d/0/temperature/5",
                placa,
                "Temperature #6",
                -55.0,
            ),
            windows(
                "/memory/dimm/0/temperature/0",
                "DIMM #1",
                "Temperature",
                41.5,
            ),
            windows(
                "/memory/dimm/1/temperature/0",
                "DIMM #2",
                "Temperature",
                42.25,
            ),
            windows("/nvme/0/temperature/0", ssd, "Composite Temperature", 44.0),
            windows("/nvme/0/temperature/1", ssd, "Temperature #2", 52.0),
            windows(
                "/gpu-nvidia/0/temperature/0",
                "NVIDIA GeForce RTX 4070",
                "GPU Core",
                39.0,
            ),
            windows("/battery/0/temperature/0", "Bateria", "", 30.0),
            // O mesmo disco pelo sistema: sai, porque o monitor já deu os dele.
            TemperaturaDoWindows {
                alto: Some(82.0),
                critico: Some(85.0),
                ..windows("/storage/0", ssd, "", 44.0)
            },
            windows(r"/thermalzone/\_TZ.TZ00", r"\_TZ.TZ00", "", 27.85),
        ];
        let ventoinhas = [
            VentoinhaDoWindows {
                caminho: "/lpc/nct6798d/0/fan/0".to_owned(),
                aparelho: placa.to_owned(),
                nome: "Fan #1".to_owned(),
                rpm: 1203.6,
            },
            VentoinhaDoWindows {
                caminho: "/lpc/nct6798d/0/fan/1".to_owned(),
                aparelho: placa.to_owned(),
                nome: "Fan #2".to_owned(),
                rpm: 0.0,
            },
            VentoinhaDoWindows {
                caminho: "/gpu-nvidia/0/fan/0".to_owned(),
                aparelho: "NVIDIA GeForce RTX 4070".to_owned(),
                nome: "GPU Fan".to_owned(),
                rpm: 850.0,
            },
        ];
        let s = do_windows(&temperaturas, &ventoinhas, Some(i5.to_owned()));
        assert_eq!(s.cpu.as_deref(), Some(i5));
        assert_eq!(
            nomes_por_grupo(&s, Grupo::Cpu),
            [
                "Nucleo(1)=51000",
                "Nucleo(2)=55000",
                "Pacote(None)=58000",
                "Core Max=55000",
            ]
        );
        assert_eq!(
            nomes_por_grupo(&s, Grupo::Memoria),
            ["Pente(1)=41500", "Pente(2)=42250"]
        );
        assert_eq!(
            nomes_por_grupo(&s, Grupo::Disco),
            [
                "Samsung SSD 990 PRO 2TB=44000",
                "Samsung SSD 990 PRO 2TB (Temperature #2)=52000",
            ]
        );
        assert_eq!(
            nomes_por_grupo(&s, Grupo::PlacaMae),
            [
                "CPU Core=57000",
                "PlacaMae=34000",
                "Soquete=38000",
                "ZonaAcpi=27850",
            ]
        );
        assert_eq!(nomes_por_grupo(&s, Grupo::Video), ["GPU Core=39000"]);
        assert_eq!(nomes_por_grupo(&s, Grupo::Outros), ["Bateria=30000"]);
        // A CPU se resume no pacote; o chip é o aparelho.
        let cpu = s.principal(Grupo::Cpu).unwrap();
        assert_eq!((cpu.mili, cpu.chip.as_str()), (58_000, i5));
        assert_eq!(
            s.ventoinhas,
            [
                Ventoinha {
                    chip: placa.to_owned(),
                    numero: 1,
                    rotulo: None,
                    rpm: 1204,
                },
                Ventoinha {
                    chip: placa.to_owned(),
                    numero: 2,
                    rotulo: None,
                    rpm: 0,
                },
                Ventoinha {
                    chip: "NVIDIA GeForce RTX 4070".to_owned(),
                    numero: 1,
                    rotulo: Some("GPU Fan".to_owned()),
                    rpm: 850,
                },
            ]
        );
    }

    #[test]
    fn sem_o_monitor_o_windows_fica_com_os_discos_e_as_zonas() {
        let temperaturas = [
            TemperaturaDoWindows {
                alto: Some(82.0),
                critico: Some(85.0),
                ..windows("/storage/0", "Samsung SSD 990 PRO 2TB", "", 44.0)
            },
            windows("/storage/1", "WDC WD40EFRX", "", 36.0),
            windows("/storage/2", "Disco sem sensor", "", 0.0),
            windows(r"/thermalzone/\_TZ.THRM", r"\_TZ.THRM", "", 46.85),
        ];
        let s = do_windows(&temperaturas, &[], Some("AMD Ryzen 7 7800X3D".to_owned()));
        assert_eq!(
            nomes_por_grupo(&s, Grupo::Disco),
            ["Samsung SSD 990 PRO 2TB=44000", "WDC WD40EFRX=36000"]
        );
        let disco = s.principal(Grupo::Disco).unwrap();
        assert_eq!((disco.alto, disco.critico), (Some(82_000), Some(85_000)));
        assert_eq!(nomes_por_grupo(&s, Grupo::PlacaMae), ["ZonaAcpi=46850"]);
        // Sem CPU, o medidor fica sem o número e a aba mostra só o que há.
        assert!(s.principal(Grupo::Cpu).is_none());
        assert_eq!(s.grupos(), [Grupo::Disco, Grupo::PlacaMae]);
        // Sem nada (a VM comum), a aba some.
        assert!(do_windows(&[], &[], None).grupos().is_empty());
    }

    #[test]
    fn o_ryzen_e_os_dois_processadores_no_windows() {
        let ryzen = [windows(
            "/amdcpu/0/temperature/2",
            "AMD Ryzen 7 7800X3D",
            "Core (Tctl/Tdie)",
            71.25,
        )];
        let s = do_windows(&ryzen, &[], None);
        assert_eq!(nomes_por_grupo(&s, Grupo::Cpu), ["Pacote(None)=71250"]);
        let xeon = "Intel Xeon Gold 6430";
        let dois = [
            windows("/intelcpu/0/temperature/32", xeon, "CPU Package", 60.0),
            windows("/intelcpu/1/temperature/32", xeon, "CPU Package", 64.0),
        ];
        let s = do_windows(&dois, &[], None);
        assert_eq!(
            nomes_por_grupo(&s, Grupo::Cpu),
            ["Pacote(Some(0))=60000", "Pacote(Some(1))=64000"]
        );
        assert_eq!(s.principal(Grupo::Cpu).unwrap().mili, 64_000);
    }

    #[cfg(any(target_os = "linux", target_os = "macos", windows))]
    #[test]
    fn este_computador_le_sem_erro() {
        // O conteúdo depende da máquina (no CI, quase sempre vazio); só não pode quebrar.
        let s = deste_computador();
        assert!(s.temperaturas.iter().all(|t| plausivel(t.mili)));
    }

    fn cpu(mili: i32, critico: Option<i32>) -> Sensor {
        Sensor {
            grupo: Grupo::Cpu,
            chip: "coretemp".to_owned(),
            nome: Nome::Pacote(None),
            mili,
            alto: Some(80_000),
            critico,
        }
    }

    #[test]
    fn o_limite_vem_do_critico_ou_da_opcao() {
        assert_eq!(limite_de_calor(&cpu(50_000, Some(100_000)), 0), 90_000);
        assert_eq!(limite_de_calor(&cpu(50_000, Some(105_000)), 0), 95_000);
        assert_eq!(limite_de_calor(&cpu(50_000, None), 0), 90_000);
        assert_eq!(limite_de_calor(&cpu(50_000, Some(100_000)), 75), 75_000);
    }

    #[test]
    fn o_aviso_volta_so_depois_de_esfriar_e_de_dez_minutos() {
        let inicio = Instant::now();
        let em = |minutos: u64| inicio + Duration::from_secs(minutos * 60);
        let mut aviso = AvisoDeCalor::default();
        assert!(!aviso.ver(89_000, 90_000, em(0)));
        assert!(aviso.ver(90_000, 90_000, em(0)));
        // Continua quente: um aviso só.
        assert!(!aviso.ver(95_000, 90_000, em(1)));
        assert!(!aviso.ver(95_000, 90_000, em(20)));
        // Desceu, mas não 5 °C: ainda é o mesmo calor.
        assert!(!aviso.ver(86_000, 90_000, em(21)));
        assert!(!aviso.ver(91_000, 90_000, em(22)));
        // Esfriou de verdade e voltou: avisa de novo, já passados os 10 minutos.
        assert!(!aviso.ver(85_000, 90_000, em(23)));
        assert!(aviso.ver(91_000, 90_000, em(24)));
    }

    #[test]
    fn dois_avisos_tem_ao_menos_dez_minutos_entre_eles() {
        let inicio = Instant::now();
        let em = |minutos: u64| inicio + Duration::from_secs(minutos * 60);
        let mut aviso = AvisoDeCalor::default();
        assert!(aviso.ver(92_000, 90_000, em(0)));
        assert!(!aviso.ver(80_000, 90_000, em(2)));
        // Esfriou e voltou em 4 minutos: espera.
        assert!(!aviso.ver(92_000, 90_000, em(4)));
        // Ainda quente aos 10 minutos: agora avisa.
        assert!(aviso.ver(93_000, 90_000, em(10)));
    }

    #[test]
    fn o_leitor_entrega_a_leitura_mais_nova() {
        let (avisa, avisos) = mpsc::channel();
        let leitor = LeitorDeSensores::novo(
            Arc::new(|| exemplo(false)),
            Duration::from_millis(10),
            move || {
                let _ = avisa.send(());
            },
        );
        avisos
            .recv_timeout(Duration::from_secs(5))
            .expect("a primeira leitura");
        let leitura = leitor.recolher().expect("a leitura");
        assert_eq!(leitura.principal(Grupo::Cpu).map(|s| s.mili), Some(86_000));
        avisos
            .recv_timeout(Duration::from_secs(5))
            .expect("a segunda leitura");
        drop(leitor);
    }
}
