//! O diagnóstico dos discos: a vida útil, o quanto já foi gravado, as horas ligado, as vezes que
//! o disco foi ligado e os alertas do SMART, pelo `smartctl` do servidor (ou deste computador).
//!
//! Roda em segundo plano, num canal da sessão SSH que já está aberta: nada aparece no terminal e
//! nada pede senha. Em máquina virtual e em contêiner não tenta nada (o disco de lá não é físico).
//!
//! **O `sudo` não incomoda.** Para quem tem regra no sudoers mas precisa de senha, qualquer
//! `sudo -n` (até o `-l`) grava a recusa no log e pode mandar e-mail ao administrador, e nada que
//! se leia sem o `sudo` revela se ele pede senha. Por isso a leitura automática só lê como root e
//! nem chama o `sudo`. Só o [`COMANDO_COM_SUDO`] chama: quem pede é o botão "Verificar de novo" e,
//! depois de dar certo, a leitura automática do mesmo alvo ([`Entrada::com_sudo`]). Ele sonda com
//! `sudo -n -l`, que não deixa rastro quando há regra sem senha, e para na primeira recusa.
//!
//! A parte pura: o script, a leitura da saída, os alertas, o cache e a herança. O que liga à tela
//! mora em `tela/saude_discos.rs`.
//!
//! O nominal de TBW não vem do disco: a vida restante é o que o próprio disco informa
//! (`100 − Percentage Used` no NVMe e os atributos de desgaste no SATA).

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::armazenamento::Armazenamento;
use crate::modelo::{DiagnosticoDeDiscos, Node, Tree};
use crate::t;

/// O comando do canal: um `sh` que lê o script da entrada. O argumento marca o que é (e o separa
/// da sondagem dos assistentes, que é o `sh -s` puro). Este não chama o `sudo`.
pub const COMANDO: &str = "sh -s -- tirech-discos";

/// O [`COMANDO`] que pode chamar o `sudo -n`, para quem não entrou como root.
pub const COMANDO_COM_SUDO: &str = "sh -s -- tirech-discos com-sudo";

/// O comando do canal.
pub fn comando(com_sudo: bool) -> &'static str {
    if com_sudo { COMANDO_COM_SUDO } else { COMANDO }
}

/// A vida restante, em %, a partir da qual o disco entra em alerta.
pub const VIDA_BAIXA: u8 = 10;

/// Por quanto tempo uma leitura com discos vale: o desgaste anda devagar.
pub const VALIDADE_COM_DISCOS: Duration = Duration::from_secs(24 * 60 * 60);

/// Por quanto tempo vale saber que não dá para ler: uma máquina virtual ou um servidor sem o
/// `smartctl` não mudam de uma conexão para a outra.
pub const VALIDADE_SEM_DISCOS: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// A chave do cache deste computador.
pub const CHAVE_LOCAL: &str = "local";

/// O script. Roda dentro de uma função, com a entrada em `/dev/null`, para nenhum programa comer
/// o resto do script (o `sh -s` o lê da mesma entrada).
pub const SCRIPT: &str = r#"
export LC_ALL=C
COM_SUDO=$2
PATH="$PATH:/usr/sbin:/sbin:/usr/local/sbin"
marca() { printf '@tirech-discos %s\n' "$1"; }
e_virtual() {
    if command -v systemd-detect-virt >/dev/null 2>&1; then
        v=$(systemd-detect-virt 2>/dev/null)
        [ -n "$v" ] && [ "$v" != none ] && return 0
    fi
    [ -e /.dockerenv ] && return 0
    [ -e /run/.containerenv ] && return 0
    [ -e /run/systemd/container ] && return 0
    grep -Eq 'docker|lxc|kubepods|containerd' /proc/1/cgroup 2>/dev/null && return 0
    grep -Eqi 'microsoft|wsl' /proc/version 2>/dev/null && return 0
    grep -q '^flags.*hypervisor' /proc/cpuinfo 2>/dev/null && return 0
    return 1
}
rodar() {
    case "$MODO" in
        su) printf '%s\n' "$T $SMART $*" | sudo -n su - 2>/dev/null ;;
        sudo) $T sudo -n "$SMART" "$@" </dev/null ;;
        *) $T "$SMART" "$@" </dev/null ;;
    esac
}
principal() {
    [ "$(uname -s 2>/dev/null)" = Linux ] || { marca outro-sistema; return 0; }
    if e_virtual; then marca virtual; return 0; fi
    SMART=$(command -v smartctl 2>/dev/null)
    [ -n "$SMART" ] || { marca sem-smartctl; return 0; }
    T=
    command -v timeout >/dev/null 2>&1 && T='timeout 30'
    if [ "$(id -u)" = 0 ]; then
        MODO=root
    elif [ "$COM_SUDO" != com-sudo ] || ! command -v sudo >/dev/null 2>&1; then
        marca sem-permissao; return 0
    elif erro=$(sudo -n -l "$SMART" 2>&1 >/dev/null); then
        MODO=sudo
    elif [ -z "$erro" ] && sudo -n -l su - >/dev/null 2>&1; then
        MODO=su
    else
        marca sem-permissao; return 0
    fi
    J=
    rodar -j --version >/dev/null 2>&1 && J=-j
    marca discos
    rodar --scan 2>/dev/null | while read -r disp opcao tipo resto; do
        case "$disp" in /dev/*) ;; *) continue ;; esac
        [ "$opcao" = -d ] || tipo=
        printf '@disco %s %s\n' "$disp" "$tipo"
        case "$tipo" in
            ''|scsi) rodar -n standby -a $J "$disp" 2>/dev/null ;;
            *) rodar -n standby -a $J -d "$tipo" "$disp" 2>/dev/null ;;
        esac
        printf '\n@fim\n'
    done
}
principal </dev/null
"#;

/// O que o `smartctl` diz de um disco. O que o disco não informa fica `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaudeDoDisco {
    /// O dispositivo (`/dev/sda`, `/dev/nvme0`, `/dev/bus/0`).
    pub dispositivo: String,
    /// O tipo do `smartctl --scan` (`nvme`, `scsi`, `megaraid,0`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tipo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modelo: Option<String>,
    /// O autoteste do SMART: passou ou não.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passou: Option<bool>,
    /// O `Critical Warning` do NVMe (diferente de 0 é problema).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aviso_critico: Option<u64>,
    /// A reserva disponível do NVMe, em %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserva: Option<u64>,
    /// O limite da reserva do NVMe, em %.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserva_limite: Option<u64>,
    /// A vida restante, em % (0 a 100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vida_restante: Option<u8>,
    /// O total gravado, em bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gravado: Option<u64>,
    /// As horas ligado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horas: Option<u64>,
    /// As vezes que o disco foi ligado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ligacoes: Option<u64>,
    /// Os setores realocados.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realocados: Option<u64>,
    /// Os setores pendentes ou que não dá para corrigir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pendentes: Option<u64>,
    /// Os erros de mídia do NVMe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub erros_midia: Option<u64>,
}

/// O resultado de um diagnóstico.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Diagnostico {
    /// Deu para ler: os discos que responderam.
    Discos(Vec<SaudeDoDisco>),
    /// Máquina virtual ou contêiner: nem tentou.
    Virtual,
    /// Sem root nem `sudo` que não peça senha.
    SemPermissao,
    /// O `smartctl` não está instalado.
    SemSmartctl,
    /// Não é Linux, ou a saída não veio do script (um shell restrito, por exemplo).
    OutroSistema,
}

/// O que merece alerta num disco.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alerta {
    /// O autoteste do SMART não passou.
    Falhando,
    /// O `Critical Warning` do NVMe está ligado.
    AvisoCritico,
    /// A reserva do NVMe chegou ao limite.
    ReservaBaixa,
    /// Pouca vida restante.
    VidaBaixa(u8),
    /// Setores realocados.
    Realocados(u64),
    /// Setores pendentes ou que não dá para corrigir.
    Pendentes(u64),
    /// Erros de mídia do NVMe.
    ErrosDeMidia(u64),
}

impl Alerta {
    /// O texto do alerta.
    pub fn texto(self) -> String {
        match self {
            Alerta::Falhando => t!("saude-discos-alerta-falhando"),
            Alerta::AvisoCritico => t!("saude-discos-alerta-critico"),
            Alerta::ReservaBaixa => t!("saude-discos-alerta-reserva"),
            Alerta::VidaBaixa(vida) => t!("saude-discos-alerta-vida", vida = u64::from(vida)),
            Alerta::Realocados(n) => t!("saude-discos-alerta-realocados", n = n),
            Alerta::Pendentes(n) => t!("saude-discos-alerta-pendentes", n = n),
            Alerta::ErrosDeMidia(n) => t!("saude-discos-alerta-midia", n = n),
        }
    }
}

impl SaudeDoDisco {
    /// O nome do disco: o dispositivo sem o `/dev/`, com o tipo de um disco atrás de uma
    /// controladora (`bus/0 (megaraid,0)`).
    pub fn nome(&self) -> String {
        let curto = self
            .dispositivo
            .strip_prefix("/dev/")
            .unwrap_or(&self.dispositivo);
        if self.tipo.contains(',') {
            format!("{curto} ({})", self.tipo)
        } else {
            curto.to_owned()
        }
    }

    /// Se o `smartctl` disse alguma coisa deste disco (um disco dormindo ou um adaptador USB
    /// que não fala SMART não dizem nada).
    fn tem_dados(&self) -> bool {
        self.passou.is_some()
            || self.vida_restante.is_some()
            || self.gravado.is_some()
            || self.horas.is_some()
            || self.ligacoes.is_some()
            || self.realocados.is_some()
            || self.pendentes.is_some()
            || self.erros_midia.is_some()
    }

    /// Os alertas deste disco, do mais grave para o menos.
    pub fn alertas(&self) -> Vec<Alerta> {
        let mut alertas = Vec::new();
        if self.passou == Some(false) {
            alertas.push(Alerta::Falhando);
        }
        if self.aviso_critico.is_some_and(|v| v != 0) {
            alertas.push(Alerta::AvisoCritico);
        }
        if let (Some(reserva), Some(limite)) = (self.reserva, self.reserva_limite)
            && reserva <= limite
        {
            alertas.push(Alerta::ReservaBaixa);
        }
        if let Some(vida) = self.vida_restante
            && vida <= VIDA_BAIXA
        {
            alertas.push(Alerta::VidaBaixa(vida));
        }
        if let Some(n) = self.pendentes.filter(|n| *n > 0) {
            alertas.push(Alerta::Pendentes(n));
        }
        if let Some(n) = self.realocados.filter(|n| *n > 0) {
            alertas.push(Alerta::Realocados(n));
        }
        if let Some(n) = self.erros_midia.filter(|n| *n > 0) {
            alertas.push(Alerta::ErrosDeMidia(n));
        }
        alertas
    }

    /// O texto da coluna Saúde.
    pub fn estado(&self) -> String {
        match self.passou {
            Some(false) => t!("saude-discos-falhando"),
            _ if !self.alertas().is_empty() => t!("saude-discos-em-alerta"),
            Some(true) => t!("saude-discos-ok"),
            None => t!("saude-discos-sem-dado"),
        }
    }

    /// Se este disco é a linha `nome` da tabela dos discos (`/dev/sda` é `sda` e `/dev/nvme0` é
    /// `nvme0n1`).
    pub fn e_a_linha(&self, nome: &str) -> bool {
        let curto = self
            .dispositivo
            .strip_prefix("/dev/")
            .unwrap_or(&self.dispositivo);
        nome == curto
            || nome
                .strip_prefix(curto)
                .is_some_and(|resto| resto.starts_with('n') && curto.starts_with("nvme"))
    }
}

impl Diagnostico {
    /// Os discos lidos (nenhum nos outros estados).
    pub fn discos(&self) -> &[SaudeDoDisco] {
        match self {
            Diagnostico::Discos(discos) => discos,
            _ => &[],
        }
    }

    /// Por quanto tempo este resultado vale.
    pub fn validade(&self) -> Duration {
        match self {
            Diagnostico::Discos(discos) if !discos.is_empty() => VALIDADE_COM_DISCOS,
            _ => VALIDADE_SEM_DISCOS,
        }
    }

    /// Cada alerta dos discos, como "sda: 3 setores realocados".
    pub fn alertas(&self) -> Vec<String> {
        self.discos()
            .iter()
            .flat_map(|d| {
                d.alertas().into_iter().map(|a| {
                    t!(
                        "saude-discos-alerta-de",
                        disco = d.nome(),
                        alerta = a.texto()
                    )
                })
            })
            .collect()
    }
}

/// Lê a saída do [`SCRIPT`]. Uma saída que não é do script (um shell restrito, um `sh` que falhou)
/// é [`Diagnostico::OutroSistema`].
pub fn ler_saida(saida: &str) -> Diagnostico {
    let Some(marca) = saida
        .lines()
        .find_map(|linha| linha.trim().strip_prefix("@tirech-discos "))
    else {
        return Diagnostico::OutroSistema;
    };
    match marca.trim() {
        "discos" => Diagnostico::Discos(ler_discos(saida)),
        "virtual" => Diagnostico::Virtual,
        "sem-permissao" => Diagnostico::SemPermissao,
        "sem-smartctl" => Diagnostico::SemSmartctl,
        _ => Diagnostico::OutroSistema,
    }
}

/// Os blocos `@disco … @fim` da saída; quem não disse nada fica de fora.
fn ler_discos(saida: &str) -> Vec<SaudeDoDisco> {
    let mut discos = Vec::new();
    let mut atual: Option<(String, String, String)> = None;
    for linha in saida.lines() {
        if let Some(resto) = linha.strip_prefix("@disco ") {
            let mut partes = resto.split_whitespace();
            let dispositivo = partes.next().unwrap_or_default().to_owned();
            let tipo = partes.next().unwrap_or_default().to_owned();
            atual = Some((dispositivo, tipo, String::new()));
        } else if linha.trim() == "@fim" {
            if let Some((dispositivo, tipo, corpo)) = atual.take() {
                let disco = ler_disco(dispositivo, tipo, &corpo);
                if disco.tem_dados() {
                    discos.push(disco);
                }
            }
        } else if let Some((_, _, corpo)) = &mut atual {
            corpo.push_str(linha);
            corpo.push('\n');
        }
    }
    discos
}

/// Um disco, pelo JSON do `smartctl -j` (versão 7 em diante) ou pelo texto das versões antigas.
fn ler_disco(dispositivo: String, tipo: String, corpo: &str) -> SaudeDoDisco {
    let mut disco = SaudeDoDisco {
        dispositivo,
        tipo,
        ..SaudeDoDisco::default()
    };
    // Um `/etc/profile` pode escrever antes do JSON, no caminho pelo `su -`.
    let json = corpo
        .find('{')
        .zip(corpo.rfind('}'))
        .filter(|(inicio, fim)| inicio < fim)
        .and_then(|(inicio, fim)| serde_json::from_str::<Value>(&corpo[inicio..=fim]).ok());
    match json {
        Some(json) => ler_json(&mut disco, &json),
        None => ler_texto(&mut disco, corpo),
    }
    disco
}

/// Um número do JSON, que pode vir como texto.
fn numero(valor: &Value) -> Option<u64> {
    valor.as_u64().or_else(|| valor.as_str()?.parse().ok())
}

/// A vida restante a partir do "% usado", que no NVMe pode passar de 100.
fn vida_de_usado(usado: u64) -> u8 {
    u8::try_from(100u64.saturating_sub(usado)).unwrap_or(0)
}

/// Os atributos de desgaste do SATA: o valor normalizado é a vida restante.
const ATRIBUTOS_DE_VIDA: [&str; 5] = [
    "SSD_Life_Left",
    "Percent_Lifetime_Remain",
    "Remaining_Lifetime_Perc",
    "Media_Wearout_Indicator",
    "Wear_Leveling_Count",
];

/// Quantos bytes vale uma unidade de um atributo de escrita do SATA (`None`: o atributo não é de
/// escrita). `setor` é o tamanho do bloco lógico.
fn unidade_de_escrita(nome: &str, setor: u64) -> Option<u64> {
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;
    match nome {
        "Total_LBAs_Written" => Some(setor),
        "Host_Writes_32MiB" => Some(32 * MIB),
        "Host_Writes_GiB" | "Lifetime_Writes_GiB" | "Total_Writes_GiB" => Some(GIB),
        "Total_Writes_GB" => Some(1_000_000_000),
        "Host_Writes_MiB" => Some(MIB),
        _ => None,
    }
}

/// Passa um atributo da tabela do SATA para o disco.
fn ler_atributo(
    disco: &mut SaudeDoDisco,
    atributo: (u64, &str),
    normalizado: Option<u64>,
    bruto: Option<u64>,
    setor: u64,
) {
    let (id, nome) = atributo;
    match id {
        5 => disco.realocados = disco.realocados.or(bruto),
        9 => disco.horas = disco.horas.or(bruto),
        12 => disco.ligacoes = disco.ligacoes.or(bruto),
        197 | 198 => {
            if let Some(n) = bruto {
                disco.pendentes = Some(disco.pendentes.unwrap_or(0).max(n));
            }
        }
        _ => {}
    }
    if disco.vida_restante.is_none()
        && ATRIBUTOS_DE_VIDA.contains(&nome)
        && let Some(valor) = normalizado.filter(|v| (1..=100).contains(v))
    {
        disco.vida_restante = u8::try_from(valor).ok();
    }
    if disco.gravado.is_none()
        && let Some(unidade) = unidade_de_escrita(nome, setor)
        && let Some(n) = bruto
    {
        disco.gravado = Some(n.saturating_mul(unidade));
    }
}

/// Lê o JSON do `smartctl -j -a`.
fn ler_json(disco: &mut SaudeDoDisco, json: &Value) {
    disco.modelo = json
        .get("model_name")
        .or_else(|| json.get("scsi_model_name"))
        .and_then(Value::as_str)
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty());
    disco.passou = json["smart_status"]["passed"].as_bool();
    disco.horas = numero(&json["power_on_time"]["hours"]);
    disco.ligacoes = numero(&json["power_cycle_count"]);

    let nvme = &json["nvme_smart_health_information_log"];
    if nvme.is_object() {
        disco.aviso_critico = numero(&nvme["critical_warning"]);
        disco.reserva = numero(&nvme["available_spare"]);
        disco.reserva_limite = numero(&nvme["available_spare_threshold"]);
        disco.vida_restante = numero(&nvme["percentage_used"]).map(vida_de_usado);
        // A unidade do NVMe é de mil blocos de 512 bytes.
        disco.gravado = numero(&nvme["data_units_written"]).map(|n| n.saturating_mul(512_000));
        disco.erros_midia = numero(&nvme["media_errors"]);
        disco.horas = disco.horas.or_else(|| numero(&nvme["power_on_hours"]));
        disco.ligacoes = disco.ligacoes.or_else(|| numero(&nvme["power_cycles"]));
    }

    let setor = numero(&json["logical_block_size"]).unwrap_or(512);
    if let Some(tabela) = json["ata_smart_attributes"]["table"].as_array() {
        for atributo in tabela {
            let Some(id) = numero(&atributo["id"]) else {
                continue;
            };
            ler_atributo(
                disco,
                (id, atributo["name"].as_str().unwrap_or_default()),
                numero(&atributo["value"]),
                numero(&atributo["raw"]["value"]),
                setor,
            );
        }
    }

    // O SCSI e o SAS.
    disco.vida_restante = disco.vida_restante.or_else(|| {
        numero(&json["scsi_percentage_used_endurance_indicator"])
            .or_else(|| numero(&json["endurance_used"]["current_percent"]))
            .map(vida_de_usado)
    });
    disco.ligacoes = disco.ligacoes.or_else(|| {
        numero(&json["scsi_start_stop_cycle_counter"]["accumulated_start_stop_cycles"])
    });
    disco.realocados = disco
        .realocados
        .or_else(|| numero(&json["scsi_grown_defect_list"]));
    disco.gravado = disco.gravado.or_else(|| {
        let gigabytes = json["scsi_error_counter_log"]["write"]["gigabytes_processed"]
            .as_str()?
            .parse::<f64>()
            .ok()?;
        // O disco informa em gigabytes com casas; a conversão só perde o que é menor que 1 byte.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some((gigabytes * 1e9) as u64)
    });
}

/// O primeiro número de `texto`, sem os separadores de milhar do `smartctl` (`1,234,567 [2 TB]`).
fn primeiro_numero(texto: &str) -> Option<u64> {
    let limpo: String = texto
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(char::is_ascii_digit)
        .collect();
    limpo.parse().ok()
}

/// O que vem depois de `chave` numa linha `chave: valor`.
fn valor_de<'a>(linha: &'a str, chave: &str) -> Option<&'a str> {
    linha.trim_start().strip_prefix(chave).map(str::trim)
}

/// Lê o texto do `smartctl -a`, das versões sem o `-j`.
fn ler_texto(disco: &mut SaudeDoDisco, corpo: &str) {
    let mut setor = 512;
    for linha in corpo.lines() {
        if let Some(v) = valor_de(linha, "Sector Size:").or(valor_de(linha, "Sector Sizes:"))
            && let Some(n) = primeiro_numero(v)
        {
            setor = n;
        }
    }
    for linha in corpo.lines() {
        let texto = |chave: &str| valor_de(linha, chave);
        let numero = |chave: &str| valor_de(linha, chave).and_then(primeiro_numero);
        if let Some(v) = texto("Device Model:")
            .or(texto("Model Number:"))
            .or(texto("Product:"))
        {
            disco.modelo = disco.modelo.take().or_else(|| Some(v.to_owned()));
        } else if let Some(v) = texto("SMART overall-health self-assessment test result:") {
            disco.passou = Some(v.starts_with("PASSED"));
        } else if let Some(v) = texto("SMART Health Status:") {
            disco.passou = Some(v.starts_with("OK"));
        } else if let Some(n) = numero("Percentage Used:") {
            disco.vida_restante = Some(vida_de_usado(n));
        } else if let Some(n) = numero("Data Units Written:") {
            disco.gravado = Some(n.saturating_mul(512_000));
        } else if let Some(n) = numero("Available Spare:") {
            disco.reserva = Some(n);
        } else if let Some(n) = numero("Available Spare Threshold:") {
            disco.reserva_limite = Some(n);
        } else if let Some(v) = texto("Critical Warning:") {
            let hexa = v.strip_prefix("0x").unwrap_or(v);
            disco.aviso_critico = u64::from_str_radix(hexa, 16).ok();
        } else if let Some(n) = numero("Media and Data Integrity Errors:") {
            disco.erros_midia = Some(n);
        } else if let Some(n) = numero("Power On Hours:") {
            disco.horas = Some(n);
        } else if let Some(n) = numero("Power Cycles:") {
            disco.ligacoes = Some(n);
        } else if let Some(n) = numero("Elements in grown defect list:") {
            disco.realocados = Some(n);
        } else if let Some(n) = numero("Accumulated start-stop cycles:") {
            disco.ligacoes = Some(n);
        } else if let Some(n) = numero("Accumulated power on time, hours:minutes") {
            disco.horas = Some(n);
        } else if let Some(n) = numero("Percentage used endurance indicator:") {
            disco.vida_restante = Some(vida_de_usado(n));
        } else {
            ler_linha_de_atributo(disco, linha, setor);
        }
    }
}

/// Uma linha da tabela de atributos do SATA:
/// `  9 Power_On_Hours  0x0032  099  099  000  Old_age  Always  -  1234`.
fn ler_linha_de_atributo(disco: &mut SaudeDoDisco, linha: &str, setor: u64) {
    let colunas: Vec<&str> = linha.split_whitespace().collect();
    if colunas.len() < 10 || !colunas[2].starts_with("0x") {
        return;
    }
    let Ok(id) = colunas[0].parse::<u64>() else {
        return;
    };
    ler_atributo(
        disco,
        (id, colunas[1]),
        primeiro_numero(colunas[3]),
        primeiro_numero(colunas[9]),
        setor,
    );
}

/// O total gravado em texto (`12,3 TB`), nas unidades decimais que os discos usam para o TBW.
pub fn texto_do_gravado(bytes: u64) -> String {
    let valor = bytes as f64;
    let (valor, unidade) = if valor >= 1e12 {
        (valor / 1e12, "TB")
    } else {
        (valor / 1e9, "GB")
    };
    format!("{} {unidade}", crate::i18n::decimal(valor, 1))
}

/// As horas ligado em texto (`26.280 h (3,0 anos)`); menos de um ano, só as horas.
pub fn texto_das_horas(horas: u64) -> String {
    // O ano do calendário, com o bissexto: 365,25 dias.
    let anos = horas as f64 / 8766.0;
    let (horas, texto_anos) = (crate::i18n::inteiro(horas), crate::i18n::decimal(anos, 1));
    if anos < 1.0 {
        t!("saude-discos-horas", horas = horas)
    } else if anos < 2.0 {
        t!(
            "saude-discos-horas-um-ano",
            horas = horas,
            anos = texto_anos
        )
    } else {
        t!("saude-discos-horas-anos", horas = horas, anos = texto_anos)
    }
}

/// O que se guarda de um servidor em `saude_discos.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrada {
    /// `usuario@host:porta` da leitura: mudou, a leitura não vale mais.
    pub alvo: String,
    /// Quando foi lida, em segundos desde 1970.
    pub quando: u64,
    pub diagnostico: Diagnostico,
    /// O `sudo -n` já funcionou neste `alvo`: a leitura automática pode chamá-lo sem incomodar.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub com_sudo: bool,
}

impl Entrada {
    /// Se a leitura ainda vale, para o mesmo `alvo`, em `agora` (segundos desde 1970). Um relógio
    /// que voltou não estica a validade: uma leitura "do futuro" vence.
    pub fn vale(&self, alvo: &str, agora: u64) -> bool {
        self.alvo == alvo
            && self.quando <= agora
            && agora - self.quando < self.diagnostico.validade().as_secs()
    }
}

/// O conteúdo de `saude_discos.json`: a chave é o id da conexão (a rede de um cliente pode
/// repetir o IP de outra, então o endereço não serve de chave), ou [`CHAVE_LOCAL`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cache {
    #[serde(default)]
    pub leituras: BTreeMap<String, Entrada>,
}

impl Cache {
    /// Se o `sudo -n` já funcionou em `chave`, com este `alvo`.
    pub fn sudo_provado(&self, chave: &str, alvo: &str) -> bool {
        self.leituras
            .get(chave)
            .is_some_and(|e| e.alvo == alvo && e.com_sudo)
    }
}

/// Se o `sudo -n` já funcionou neste computador, pelo `saude_discos.json` do Omni. É para o
/// `htop-omni`, que só lê.
pub fn sudo_provado_neste_computador(armazenamento: &Armazenamento) -> bool {
    let (cache, _): (Cache, _) =
        armazenamento.carregar_json(&armazenamento.arquivo_saude_discos(), "");
    cache.sudo_provado(CHAVE_LOCAL, "")
}

/// O que uma escolha decide: `None` segue subindo.
fn decide(escolha: DiagnosticoDeDiscos) -> Option<DiagnosticoDeDiscos> {
    (!escolha.is_herdar()).then_some(escolha)
}

/// A pasta que decide o diagnóstico do item `no`, subindo a partir da de cima, e a escolha dela.
pub fn herdada(tree: &Tree, no: u64) -> Option<(&str, DiagnosticoDeDiscos)> {
    for grupo in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(grupo)
            && let Some(escolha) = decide(g.diagnostico_discos)
        {
            return Some((&g.name, escolha));
        }
    }
    None
}

/// Se o diagnóstico vale para a conexão `conexao`: a escolha dela, a da pasta ou o `padrao` das
/// Configurações.
pub fn ligado(tree: &Tree, conexao: u64, padrao: bool) -> bool {
    let escolha = tree.connection(conexao).and_then(|c| {
        decide(c.diagnostico_discos).or_else(|| herdada(tree, conexao).map(|(_, e)| e))
    });
    match escolha {
        Some(DiagnosticoDeDiscos::Ligado) => true,
        Some(DiagnosticoDeDiscos::Desligado) => false,
        Some(DiagnosticoDeDiscos::Herdar) | None => padrao,
    }
}

/// A opção "Ligado" da linha.
pub fn rotulo_ligado() -> String {
    t!("saude-discos-opcao-ligado")
}

/// A opção "Desligado" da linha.
pub fn rotulo_desligado() -> String {
    t!("saude-discos-opcao-desligado")
}

/// O texto da opção "Herdar" do item `no`: diz de onde vem.
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    match herdada(tree, no) {
        Some((nome, _)) => t!("saude-discos-herdar-de", nome = nome),
        None => t!("saude-discos-herdar-configuracoes"),
    }
}

/// O texto da linha para `escolha` no item `no`.
pub fn rotulo(tree: &Tree, no: u64, escolha: DiagnosticoDeDiscos) -> String {
    match escolha {
        DiagnosticoDeDiscos::Herdar => rotulo_de_herdar(tree, no),
        DiagnosticoDeDiscos::Ligado => rotulo_ligado(),
        DiagnosticoDeDiscos::Desligado => rotulo_desligado(),
    }
}

/// As opções da linha do item `no`.
pub fn opcoes(tree: &Tree, no: u64) -> Vec<String> {
    vec![
        rotulo_de_herdar(tree, no),
        rotulo_ligado(),
        rotulo_desligado(),
    ]
}

/// A escolha que o texto da linha faz; um texto que não é de nenhuma opção é "Herdar".
pub fn do_rotulo(texto: &str) -> DiagnosticoDeDiscos {
    if texto == rotulo_ligado() {
        DiagnosticoDeDiscos::Ligado
    } else if texto == rotulo_desligado() {
        DiagnosticoDeDiscos::Desligado
    } else {
        DiagnosticoDeDiscos::Herdar
    }
}

/// O diagnóstico deste computador: o [`SCRIPT`] num `sh`, chamando o `sudo` só com `com_sudo`.
/// Bloqueia; chame de uma thread. Só o Linux tem o que ler; nos outros sistemas, nem tenta.
pub fn deste_computador(com_sudo: bool) -> Diagnostico {
    #[cfg(target_os = "linux")]
    {
        use std::io::Write as _;
        use std::process::{Command, Stdio};
        let mut comando = Command::new("sh");
        comando.args(["-s", "--", "tirech-discos"]);
        if com_sudo {
            comando.arg("com-sudo");
        }
        let filho = comando
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut filho) = filho else {
            return Diagnostico::OutroSistema;
        };
        if let Some(mut entrada) = filho.stdin.take() {
            let _ = entrada.write_all(SCRIPT.as_bytes());
        }
        match filho.wait_with_output() {
            Ok(saida) => ler_saida(&String::from_utf8_lossy(&saida.stdout)),
            Err(_) => Diagnostico::OutroSistema,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = com_sudo;
        Diagnostico::OutroSistema
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::{Connection, Kind};

    /// Roda o script num `sh` de verdade, com um `PATH` de programas falsos.
    #[cfg(unix)]
    mod no_sh {
        use super::*;
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        use std::path::{Path, PathBuf};
        use std::process::{Command, Stdio};

        struct Pasta(PathBuf);

        impl Drop for Pasta {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }

        fn pasta(nome: &str) -> Pasta {
            let caminho = std::env::temp_dir()
                .join(format!("tirech-omni-saude-{nome}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&caminho);
            fs::create_dir_all(&caminho).unwrap();
            Pasta(caminho)
        }

        fn programa(pasta: &Path, nome: &str, corpo: &str) {
            let caminho = pasta.join(nome);
            fs::write(&caminho, format!("#!/bin/sh\n{corpo}\n")).unwrap();
            fs::set_permissions(&caminho, fs::Permissions::from_mode(0o755)).unwrap();
        }

        /// O script só com os programas falsos de `pasta` e o básico do sistema: troca a linha
        /// do `PATH` e usa um `uname` falso, para não depender de onde o teste roda. Os arquivos
        /// que denunciam um contêiner não têm como ser falsificados (o CI costuma ser um): por
        /// isso a chamada de `e_virtual` também é trocada.
        fn rodar_script(pasta: &Path, e_virtual: bool) -> Diagnostico {
            rodar_script_com(pasta, e_virtual, false)
        }

        /// O [`rodar_script`] com o `sudo` permitido ou não.
        fn rodar_script_com(pasta: &Path, e_virtual: bool, com_sudo: bool) -> Diagnostico {
            programa(pasta, "uname", "echo Linux");
            let script = SCRIPT
                .replace(
                    r#"PATH="$PATH:/usr/sbin:/sbin:/usr/local/sbin""#,
                    &format!("PATH=\"{}:/usr/bin:/bin\"", pasta.display()),
                )
                .replace(
                    "if e_virtual; then",
                    if e_virtual {
                        "if true; then"
                    } else {
                        "if false; then"
                    },
                );
            let mut filho = Command::new("sh")
                .args(["-s", "--", "tirech-discos"])
                .args(com_sudo.then_some("com-sudo"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            {
                use std::io::Write as _;
                filho
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(script.as_bytes())
                    .unwrap();
            }
            let saida = filho.wait_with_output().unwrap();
            ler_saida(&String::from_utf8_lossy(&saida.stdout))
        }

        const SMARTCTL: &str = r#"
case "$1" in
  --version) echo smartctl 7.4; exit 0 ;;
  -j) [ "$2" = --version ] && exit 0 ;;
  --scan) echo '/dev/sda -d scsi # /dev/sda, SCSI device'; echo '/dev/nvme0 -d nvme # nvme'; exit 0 ;;
esac
for a in "$@"; do
  case "$a" in
    /dev/sda) echo '{"model_name":"HD","smart_status":{"passed":true},"power_on_time":{"hours":100}}' ;;
    /dev/nvme0) echo '{"nvme_smart_health_information_log":{"percentage_used":3}}' ;;
  esac
done
"#;

        #[test]
        fn em_maquina_virtual_nem_procura_o_smartctl() {
            let p = pasta("virtual");
            // Se chegasse a rodar, deixaria o arquivo.
            let marca = p.0.join("rodou");
            programa(&p.0, "smartctl", &format!("touch '{}'", marca.display()));
            assert_eq!(rodar_script(&p.0, true), Diagnostico::Virtual);
            assert!(!marca.exists(), "tentou o smartctl numa máquina virtual");
        }

        #[test]
        fn sem_smartctl_diz_e_nao_chama_o_sudo() {
            let p = pasta("sem");
            let marca = p.0.join("rodou");
            programa(&p.0, "sudo", &format!("touch '{}'", marca.display()));
            if Command::new("sh")
                .args(["-c", "PATH=/usr/bin:/bin command -v smartctl"])
                .status()
                .unwrap()
                .success()
            {
                // Este computador tem o `smartctl` em /usr/bin: o caso não dá para montar aqui.
                return;
            }
            assert_eq!(
                rodar_script_com(&p.0, false, true),
                Diagnostico::SemSmartctl
            );
            assert!(!marca.exists());
        }

        /// O `id` falso escolhe root ou não.
        fn id(pasta: &Path, uid: u32) {
            programa(pasta, "id", &format!("echo {uid}"));
        }

        #[test]
        fn como_root_le_os_discos_sem_chamar_o_sudo() {
            let p = pasta("root");
            id(&p.0, 0);
            programa(&p.0, "smartctl", SMARTCTL);
            let marca = p.0.join("chamou-o-sudo");
            programa(&p.0, "sudo", &format!("touch '{}'", marca.display()));
            let d = rodar_script(&p.0, false);
            let discos = d.discos();
            assert_eq!(discos.len(), 2, "{d:?}");
            assert_eq!(discos[0].dispositivo, "/dev/sda");
            assert_eq!(discos[0].horas, Some(100));
            assert_eq!(discos[1].vida_restante, Some(97));
            assert!(!marca.exists(), "chamou o sudo sendo root");
        }

        #[test]
        fn a_leitura_automatica_sem_root_nem_chama_o_sudo() {
            let p = pasta("automatica");
            id(&p.0, 1000);
            programa(&p.0, "smartctl", SMARTCTL);
            // Um `sudo` que funcionaria: mesmo assim, é a tentativa que deixaria a recusa no log.
            let marca = p.0.join("chamou-o-sudo");
            programa(
                &p.0,
                "sudo",
                &format!("touch '{}'\nexit 0", marca.display()),
            );
            assert_eq!(rodar_script(&p.0, false), Diagnostico::SemPermissao);
            assert!(!marca.exists(), "chamou o sudo sem ter sido pedido");
        }

        #[test]
        fn com_sudo_sem_senha_usa_o_sudo_com_n() {
            let p = pasta("sudo");
            id(&p.0, 1000);
            programa(&p.0, "smartctl", SMARTCTL);
            // Só aceita com `-n`: nunca pode perguntar a senha.
            programa(
                &p.0,
                "sudo",
                "[ \"$1\" = -n ] || exit 9\nshift\n[ \"$1\" = -l ] && exit 0\nexec \"$@\"",
            );
            assert_eq!(rodar_script_com(&p.0, false, true).discos().len(), 2);
        }

        #[test]
        fn com_so_o_su_sem_senha_passa_pelo_su() {
            let p = pasta("su");
            id(&p.0, 1000);
            programa(&p.0, "smartctl", SMARTCTL);
            // O sudoers só deixa `sudo su -`, que roda o que vem na entrada. O `-l` de um comando
            // que a regra não cobre sai com 1 e sem texto, como o de verdade.
            programa(
                &p.0,
                "sudo",
                "[ \"$1\" = -n ] || exit 1\n\
                 if [ \"$2\" = -l ]; then [ \"$3\" = su ] && [ \"$4\" = - ]; exit $?; fi\n\
                 [ \"$2\" = su ] && [ \"$3\" = - ] || exit 1\nexec sh",
            );
            let d = rodar_script_com(&p.0, false, true);
            assert_eq!(d.discos().len(), 2, "{d:?}");
        }

        #[test]
        fn sudo_que_pede_senha_para_na_primeira_recusa() {
            let p = pasta("negado");
            id(&p.0, 1000);
            let marca = p.0.join("rodou");
            programa(&p.0, "smartctl", &format!("touch '{}'", marca.display()));
            // Cada recusa vai para o log e pode virar e-mail: uma basta.
            let chamadas = p.0.join("chamadas");
            programa(
                &p.0,
                "sudo",
                &format!(
                    "echo \"$*\" >> '{}'\necho 'sudo: a password is required' >&2\nexit 1",
                    chamadas.display()
                ),
            );
            assert_eq!(
                rodar_script_com(&p.0, false, true),
                Diagnostico::SemPermissao
            );
            assert!(!marca.exists(), "rodou o smartctl sem permissão");
            let chamadas = fs::read_to_string(&chamadas).unwrap();
            assert_eq!(chamadas.lines().count(), 1, "{chamadas}");
            assert!(chamadas.starts_with("-n -l "), "{chamadas}");
        }

        #[test]
        fn o_programa_que_le_a_entrada_nao_come_o_script() {
            let p = pasta("entrada");
            id(&p.0, 0);
            // Um smartctl que lê a entrada toda: sem o `</dev/null`, o resto do script sumiria.
            programa(&p.0, "smartctl", &format!("cat >/dev/null\n{SMARTCTL}"));
            assert_eq!(rodar_script(&p.0, false).discos().len(), 2);
        }
    }

    const NVME_JSON: &str = r#"{
      "model_name": "Samsung SSD 980 PRO 1TB",
      "smart_status": {"passed": true},
      "power_on_time": {"hours": 26280},
      "power_cycle_count": 412,
      "nvme_smart_health_information_log": {
        "critical_warning": 0, "available_spare": 100, "available_spare_threshold": 10,
        "percentage_used": 7, "data_units_written": 24000000, "media_errors": 0,
        "power_on_hours": 1, "power_cycles": 2
      }
    }"#;

    const SATA_SSD_JSON: &str = r#"{
      "model_name": "Samsung SSD 860 EVO 500GB",
      "smart_status": {"passed": true},
      "logical_block_size": 512,
      "power_on_time": {"hours": 9000},
      "power_cycle_count": 55,
      "ata_smart_attributes": {"table": [
        {"id": 5, "name": "Reallocated_Sector_Ct", "value": 100, "raw": {"value": 0}},
        {"id": 9, "name": "Power_On_Hours", "value": 90, "raw": {"value": 123456789}},
        {"id": 12, "name": "Power_Cycle_Count", "value": 99, "raw": {"value": 1}},
        {"id": 177, "name": "Wear_Leveling_Count", "value": 88, "raw": {"value": 120}},
        {"id": 241, "name": "Total_LBAs_Written", "value": 99, "raw": {"value": 2000000000}}
      ]}
    }"#;

    const HD_FALHANDO_JSON: &str = r#"{
      "model_name": "WDC WD40EFRX",
      "smart_status": {"passed": false},
      "ata_smart_attributes": {"table": [
        {"id": 5, "name": "Reallocated_Sector_Ct", "value": 100, "raw": {"value": 8}},
        {"id": 9, "name": "Power_On_Hours", "value": 50, "raw": {"value": 40000}},
        {"id": 12, "name": "Power_Cycle_Count", "value": 99, "raw": {"value": 30}},
        {"id": 197, "name": "Current_Pending_Sector", "value": 200, "raw": {"value": 2}},
        {"id": 198, "name": "Offline_Uncorrectable", "value": 200, "raw": {"value": 5}}
      ]}
    }"#;

    const NVME_TEXTO: &str = "\
smartctl 6.6 2016-05-31
=== START OF INFORMATION SECTION ===
Model Number:                       INTEL SSDPE2KX010T8
=== START OF SMART DATA SECTION ===
SMART overall-health self-assessment test result: PASSED

SMART/Health Information (NVMe Log 0x02)
Critical Warning:                   0x04
Available Spare:                    5%
Available Spare Threshold:          10%
Percentage Used:                    93%
Data Units Written:                 1,234,567 [632 GB]
Power On Hours:                     26,280
Power Cycles:                       1,412
Media and Data Integrity Errors:    3
";

    const SATA_TEXTO: &str = "\
Device Model:     CT500MX500SSD1
Sector Size:      512 bytes logical/physical
SMART overall-health self-assessment test result: FAILED!
ID# ATTRIBUTE_NAME          FLAG     VALUE WORST THRESH TYPE      UPDATED  WHEN_FAILED RAW_VALUE
  5 Reallocated_Sector_Ct   0x0033   100   100   010    Pre-fail  Always       -       4
  9 Power_On_Hours          0x0032   099   099   000    Old_age   Always       -       1234h+05m
 12 Power_Cycle_Count       0x0032   100   100   000    Old_age   Always       -       77
202 Percent_Lifetime_Remain 0x0030   088   088   001    Old_age   Offline      -       12
246 Total_LBAs_Written      0x0030   100   253   000    Old_age   Offline      -       1000
";

    const SAS_JSON: &str = r#"{
      "scsi_model_name": "SEAGATE ST600MM",
      "smart_status": {"passed": true},
      "power_on_time": {"hours": 50000},
      "scsi_percentage_used_endurance_indicator": 4,
      "scsi_grown_defect_list": 2,
      "scsi_start_stop_cycle_counter": {"accumulated_start_stop_cycles": 99},
      "scsi_error_counter_log": {"write": {"gigabytes_processed": "1500.250"}}
    }"#;

    fn saida_de(blocos: &[(&str, &str, &str)]) -> String {
        let mut s = String::from("@tirech-discos discos\n");
        for (dispositivo, tipo, corpo) in blocos {
            s.push_str(&format!("@disco {dispositivo} {tipo}\n{corpo}\n@fim\n"));
        }
        s
    }

    fn um_disco(corpo: &str) -> SaudeDoDisco {
        let d = ler_saida(&saida_de(&[("/dev/x", "", corpo)]));
        d.discos().first().cloned().expect("um disco")
    }

    #[test]
    fn nvme_em_json_prefere_o_power_on_time_e_converte_as_unidades() {
        let d = um_disco(NVME_JSON);
        assert_eq!(d.modelo.as_deref(), Some("Samsung SSD 980 PRO 1TB"));
        assert_eq!(d.passou, Some(true));
        assert_eq!(d.vida_restante, Some(93));
        assert_eq!(d.gravado, Some(24_000_000 * 512_000));
        assert_eq!(d.horas, Some(26280));
        assert_eq!(d.ligacoes, Some(412));
        assert_eq!(d.reserva, Some(100));
        assert_eq!(d.reserva_limite, Some(10));
        assert_eq!(d.erros_midia, Some(0));
        assert!(d.alertas().is_empty());
    }

    #[test]
    fn ssd_sata_em_json_le_o_desgaste_e_o_gravado_pelo_nome_do_atributo() {
        let d = um_disco(SATA_SSD_JSON);
        assert_eq!(d.vida_restante, Some(88));
        assert_eq!(d.gravado, Some(2_000_000_000 * 512));
        // O `power_on_time` já vem decodificado; o bruto do atributo 9 varia por fabricante.
        assert_eq!(d.horas, Some(9000));
        assert_eq!(d.ligacoes, Some(55));
        assert_eq!(d.realocados, Some(0));
    }

    #[test]
    fn hd_falhando_da_alertas_e_nao_tem_vida_nem_gravado() {
        let d = um_disco(HD_FALHANDO_JSON);
        assert_eq!(d.vida_restante, None);
        assert_eq!(d.gravado, None);
        assert_eq!(d.horas, Some(40000));
        assert_eq!(d.ligacoes, Some(30));
        assert_eq!(d.pendentes, Some(5));
        assert_eq!(
            d.alertas(),
            vec![
                Alerta::Falhando,
                Alerta::Pendentes(5),
                Alerta::Realocados(8)
            ]
        );
    }

    #[test]
    fn sas_em_json_usa_os_campos_do_scsi() {
        let d = um_disco(SAS_JSON);
        assert_eq!(d.modelo.as_deref(), Some("SEAGATE ST600MM"));
        assert_eq!(d.vida_restante, Some(96));
        assert_eq!(d.realocados, Some(2));
        assert_eq!(d.ligacoes, Some(99));
        assert_eq!(d.gravado, Some(1_500_250_000_000));
    }

    #[test]
    fn nvme_em_texto_das_versoes_antigas() {
        let d = um_disco(NVME_TEXTO);
        assert_eq!(d.modelo.as_deref(), Some("INTEL SSDPE2KX010T8"));
        assert_eq!(d.passou, Some(true));
        assert_eq!(d.aviso_critico, Some(4));
        assert_eq!(d.vida_restante, Some(7));
        assert_eq!(d.gravado, Some(1_234_567 * 512_000));
        assert_eq!(d.horas, Some(26280));
        assert_eq!(d.ligacoes, Some(1412));
        assert_eq!(d.erros_midia, Some(3));
        assert_eq!(
            d.alertas(),
            vec![
                Alerta::AvisoCritico,
                Alerta::ReservaBaixa,
                Alerta::VidaBaixa(7),
                Alerta::ErrosDeMidia(3)
            ]
        );
    }

    #[test]
    fn sata_em_texto_le_a_tabela_de_atributos() {
        let d = um_disco(SATA_TEXTO);
        assert_eq!(d.modelo.as_deref(), Some("CT500MX500SSD1"));
        assert_eq!(d.passou, Some(false));
        assert_eq!(d.horas, Some(1234));
        assert_eq!(d.ligacoes, Some(77));
        assert_eq!(d.realocados, Some(4));
        assert_eq!(d.vida_restante, Some(88));
    }

    #[test]
    fn o_percentual_usado_acima_de_100_nao_estoura() {
        assert_eq!(vida_de_usado(180), 0);
        assert_eq!(vida_de_usado(100), 0);
        assert_eq!(vida_de_usado(0), 100);
    }

    #[test]
    fn valor_normalizado_fora_de_1_a_100_nao_e_vida() {
        let corpo = r#"{"ata_smart_attributes": {"table": [
            {"id": 177, "name": "Wear_Leveling_Count", "value": 253, "raw": {"value": 1}},
            {"id": 9, "name": "Power_On_Hours", "value": 90, "raw": {"value": 7}}
        ]}}"#;
        let d = um_disco(corpo);
        assert_eq!(d.vida_restante, None);
        assert_eq!(d.horas, Some(7));
    }

    #[test]
    fn disco_que_nao_disse_nada_fica_de_fora() {
        let saida = saida_de(&[
            ("/dev/sda", "scsi", "Device is in STANDBY mode, exit(2)"),
            ("/dev/sdb", "sat", "{\"smartctl\": {\"exit_status\": 2}}"),
            ("/dev/sdc", "scsi", HD_FALHANDO_JSON),
        ]);
        let d = ler_saida(&saida);
        assert_eq!(d.discos().len(), 1);
        assert_eq!(d.discos()[0].dispositivo, "/dev/sdc");
    }

    #[test]
    fn o_texto_antes_do_json_do_su_nao_atrapalha() {
        let corpo = format!("Bem-vindo ao servidor\n{NVME_JSON}");
        assert_eq!(um_disco(&corpo).vida_restante, Some(93));
    }

    #[test]
    fn as_marcas_e_a_saida_que_nao_e_do_script() {
        assert_eq!(ler_saida("@tirech-discos virtual\n"), Diagnostico::Virtual);
        assert_eq!(
            ler_saida("@tirech-discos sem-permissao\n"),
            Diagnostico::SemPermissao
        );
        assert_eq!(
            ler_saida("@tirech-discos sem-smartctl\n"),
            Diagnostico::SemSmartctl
        );
        assert_eq!(
            ler_saida("@tirech-discos outro-sistema\n"),
            Diagnostico::OutroSistema
        );
        assert_eq!(ler_saida(""), Diagnostico::OutroSistema);
        assert_eq!(
            ler_saida("rbash: sh: restricted"),
            Diagnostico::OutroSistema
        );
        assert_eq!(
            ler_saida("@tirech-discos discos\n"),
            Diagnostico::Discos(Vec::new())
        );
    }

    #[test]
    fn o_nome_e_a_linha_da_tabela_dos_discos() {
        let d = |dispositivo: &str, tipo: &str| SaudeDoDisco {
            dispositivo: dispositivo.to_owned(),
            tipo: tipo.to_owned(),
            ..SaudeDoDisco::default()
        };
        assert_eq!(d("/dev/sda", "scsi").nome(), "sda");
        assert_eq!(d("/dev/bus/0", "megaraid,3").nome(), "bus/0 (megaraid,3)");
        assert!(d("/dev/sda", "").e_a_linha("sda"));
        assert!(!d("/dev/sda", "").e_a_linha("sdaa"));
        assert!(d("/dev/nvme0", "nvme").e_a_linha("nvme0n1"));
        assert!(!d("/dev/nvme1", "nvme").e_a_linha("nvme0n1"));
    }

    #[test]
    fn a_validade_depende_do_que_veio_e_o_alvo_conta() {
        let com_discos = Entrada {
            alvo: "a@b:22".into(),
            quando: 1000,
            diagnostico: Diagnostico::Discos(vec![SaudeDoDisco::default()]),
            com_sudo: false,
        };
        let dia = VALIDADE_COM_DISCOS.as_secs();
        assert!(com_discos.vale("a@b:22", 1000 + dia - 1));
        assert!(!com_discos.vale("a@b:22", 1000 + dia));
        assert!(!com_discos.vale("c@b:22", 1001), "outro usuário: não vale");
        assert!(!com_discos.vale("a@b:22", 999), "relógio que voltou: vence");
        let negado = Entrada {
            diagnostico: Diagnostico::SemPermissao,
            ..com_discos
        };
        assert!(negado.vale("a@b:22", 1000 + dia));
        assert!(!negado.vale("a@b:22", 1000 + VALIDADE_SEM_DISCOS.as_secs()));
    }

    #[test]
    fn o_cache_vai_e_volta_em_json() {
        let mut cache = Cache::default();
        cache.leituras.insert(
            "7".into(),
            Entrada {
                alvo: "a@b:22".into(),
                quando: 5,
                diagnostico: Diagnostico::Discos(vec![um_disco(NVME_JSON)]),
                com_sudo: true,
            },
        );
        cache.leituras.insert(
            CHAVE_LOCAL.into(),
            Entrada {
                alvo: String::new(),
                quando: 6,
                diagnostico: Diagnostico::Virtual,
                com_sudo: false,
            },
        );
        let json = serde_json::to_string(&cache).unwrap();
        assert_eq!(serde_json::from_str::<Cache>(&json).unwrap(), cache);
        assert!(cache.sudo_provado("7", "a@b:22"));
        assert!(!cache.sudo_provado("7", "c@b:22"), "outro alvo");
        assert!(!cache.sudo_provado(CHAVE_LOCAL, ""));
        assert!(!cache.sudo_provado("8", "a@b:22"));
    }

    #[test]
    fn a_heranca_sobe_ate_a_pasta_que_decide_e_cai_nas_configuracoes() {
        let mut tree = Tree::default();
        let cima = tree.add_group(None, "Cima");
        let meio = tree.add_group(Some(cima), "Meio");
        let id = tree.add_connection(Some(meio), Connection::new("c", Kind::Ssh));
        assert!(ligado(&tree, id, true));
        assert!(!ligado(&tree, id, false));
        if let Some(Node::Group(g)) = tree.find_mut(cima) {
            g.diagnostico_discos = DiagnosticoDeDiscos::Desligado;
        }
        assert!(!ligado(&tree, id, true));
        if let Some(Node::Group(g)) = tree.find_mut(meio) {
            g.diagnostico_discos = DiagnosticoDeDiscos::Ligado;
        }
        assert!(ligado(&tree, id, false));
        if let Some(Node::Connection(c)) = tree.find_mut(id) {
            c.diagnostico_discos = DiagnosticoDeDiscos::Desligado;
        }
        assert!(!ligado(&tree, id, true));
    }

    #[test]
    fn os_rotulos_vao_e_voltam() {
        for escolha in [
            DiagnosticoDeDiscos::Ligado,
            DiagnosticoDeDiscos::Desligado,
            DiagnosticoDeDiscos::Herdar,
        ] {
            let tree = Tree::default();
            assert_eq!(do_rotulo(&rotulo(&tree, 0, escolha)), escolha);
        }
    }
}
