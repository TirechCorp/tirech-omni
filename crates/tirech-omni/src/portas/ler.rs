//! Os leitores do que o servidor diz (M58): `ss`, `/proc/net/tcp`, `netstat`, `/etc/passwd` e o
//! `docker inspect`, mais o `juntar`, que agrupa as escutas e dá nome, alcance e destino.

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::catalogo::{self, Tipo};
use super::{Alcance, Conteiner, Escuta, PortaDoServidor, Processo, Servico};

// ----- /proc/net/tcp -----

/// As linhas LISTEN de `/proc/net/tcp` (`v6` falso) ou `/proc/net/tcp6` (`v6` verdadeiro). O
/// endereço é lido na ordem de bytes da máquina do servidor: se algum endereço só faz sentido
/// do outro jeito (o `127.0.0.1` que vira `1.0.0.127`), o arquivo inteiro é relido como
/// big-endian. Linha torta é ignorada.
#[must_use]
pub fn ler_proc_tcp(texto: &str, v6: bool) -> Vec<Escuta> {
    let pequena = ler_proc(texto, v6, false);
    if pequena.iter().any(|e| parece_big_endian(e.endereco)) {
        ler_proc(texto, v6, true)
    } else {
        pequena
    }
}

/// Se um endereço lido como little-endian só se explica no outro sentido: `x.0.0.127` com `x`
/// de 1 a 255, ou `::x.0.0.0` e `::ffff:x.0.0.127` no IPv6.
fn parece_big_endian(endereco: IpAddr) -> bool {
    match endereco {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] != 0 && o[1] == 0 && o[2] == 0 && o[3] == 127
        }
        IpAddr::V6(v6) => {
            let o = v6.octets();
            let zeros = |faixa: &[u8]| faixa.iter().all(|&b| b == 0);
            // `::1` lido ao contrário vira `::100:0` (o último bloco é `x.0.0.0`).
            let loopback = zeros(&o[..12]) && o[12] != 0 && zeros(&o[13..]);
            // `::ffff:127.0.0.1` lido ao contrário traz os `ff ff` no lugar errado.
            let mapeado =
                zeros(&o[..8]) && o[8] == 0xff && o[9] == 0xff && o[10] == 0 && o[11] == 0;
            loopback || mapeado
        }
    }
}

fn ler_proc(texto: &str, v6: bool, big: bool) -> Vec<Escuta> {
    texto
        .lines()
        .skip(1)
        .filter_map(|linha| ler_linha_proc(linha, v6, big))
        .collect()
}

fn ler_linha_proc(linha: &str, v6: bool, big: bool) -> Option<Escuta> {
    let campos: Vec<&str> = linha.split_whitespace().collect();
    if campos.len() < 10 || campos[3] != "0A" {
        return None;
    }
    let (endereco, porta) = campos[1].split_once(':')?;
    let porta = u16::from_str_radix(porta, 16).ok()?;
    let endereco = if v6 {
        decodificar_v6(endereco, big)?
    } else {
        decodificar_v4(endereco, big)?
    };
    Some(Escuta {
        endereco,
        porta,
        uid: campos[7].parse().ok(),
        inode: campos[9].parse().ok(),
        processo: None,
    })
}

fn decodificar_v4(hex: &str, big: bool) -> Option<IpAddr> {
    if hex.len() != 8 {
        return None;
    }
    let valor = u32::from_str_radix(hex, 16).ok()?;
    let octetos = if big {
        valor.to_be_bytes()
    } else {
        valor.to_le_bytes()
    };
    Some(IpAddr::V4(Ipv4Addr::from(octetos)))
}

fn decodificar_v6(hex: &str, big: bool) -> Option<IpAddr> {
    if hex.len() != 32 {
        return None;
    }
    let mut octetos = [0u8; 16];
    for (i, bloco) in octetos.chunks_exact_mut(4).enumerate() {
        let valor = u32::from_str_radix(hex.get(i * 8..i * 8 + 8)?, 16).ok()?;
        bloco.copy_from_slice(&if big {
            valor.to_be_bytes()
        } else {
            valor.to_le_bytes()
        });
    }
    Some(IpAddr::V6(Ipv6Addr::from(octetos)))
}

// ----- ss -----

/// A saída de `ss -ltnpeH` (o `-e` traz o dono e o inode): `LISTEN 0 4096 127.0.0.1:3306 0.0.0.0:* users:(("mysqld",pid=812,fd=21))`.
/// Aceita `[::]:80`, `*:80`, `127.0.0.1%lo:53` e a linha sem `users`.
#[must_use]
pub fn ler_ss(texto: &str) -> Vec<Escuta> {
    texto.lines().filter_map(ler_linha_ss).collect()
}

fn ler_linha_ss(linha: &str) -> Option<Escuta> {
    let campos: Vec<&str> = linha.split_whitespace().collect();
    if campos.len() < 5 || !campos[0].eq_ignore_ascii_case("LISTEN") {
        return None;
    }
    let (host, porta) = campos[3].rsplit_once(':')?;
    let endereco = ler_host(host)?;
    let porta = porta.parse().ok()?;
    let processo = linha
        .find("users:((")
        .and_then(|i| ler_processo_ss(&linha[i..]));
    // O `ss -e` acrescenta `uid:110 ino:23456`.
    fn numero<T: std::str::FromStr>(campos: &[&str], prefixo: &str) -> Option<T> {
        campos
            .iter()
            .find_map(|campo| campo.strip_prefix(prefixo))
            .and_then(|n| n.parse().ok())
    }
    Some(Escuta {
        endereco,
        porta,
        uid: numero(&campos, "uid:"),
        inode: numero(&campos, "ino:"),
        processo,
    })
}

/// `users:(("mysqld",pid=812,fd=21),("outro",pid=9,fd=3))`: o primeiro processo.
fn ler_processo_ss(texto: &str) -> Option<Processo> {
    let resto = texto.strip_prefix("users:((\"")?;
    let (nome, resto) = resto.split_once('"')?;
    let pid = resto
        .split_once("pid=")
        .map(|(_, p)| {
            p.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .and_then(|p| p.parse().ok());
    Some(Processo {
        pid,
        nome: nome.to_owned(),
    })
}

/// O endereço de uma linha do `ss` ou do `netstat`: `*` é `0.0.0.0`, os colchetes e o `%lo`
/// saem.
fn ler_host(host: &str) -> Option<IpAddr> {
    let host = host.trim_matches(['[', ']']);
    let host = host.split_once('%').map_or(host, |(h, _)| h);
    if host == "*" {
        return Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    }
    host.parse().ok()
}

// ----- netstat -----

/// A saída de `netstat -an`, do Linux (`0.0.0.0:22`, `:::22`) e dos BSD e do macOS
/// (`127.0.0.1.3306`, `*.80`, `::1.5432`).
#[must_use]
pub fn ler_netstat(texto: &str) -> Vec<Escuta> {
    texto.lines().filter_map(ler_linha_netstat).collect()
}

fn ler_linha_netstat(linha: &str) -> Option<Escuta> {
    let campos: Vec<&str> = linha.split_whitespace().collect();
    if campos.len() < 6 || !campos[0].starts_with("tcp") || campos[campos.len() - 1] != "LISTEN" {
        return None;
    }
    let local = campos[3];
    let corte = local.rfind([':', '.'])?;
    let (host, porta) = (&local[..corte], &local[corte + 1..]);
    Some(Escuta {
        endereco: ler_host(host)?,
        porta: porta.parse().ok()?,
        uid: None,
        inode: None,
        processo: None,
    })
}

// ----- /etc/passwd -----

/// `nome:x:uid:…` → uid → nome.
#[must_use]
pub fn ler_passwd(texto: &str) -> HashMap<u32, String> {
    let mut usuarios = HashMap::new();
    for linha in texto.lines() {
        if linha.starts_with('#') {
            continue;
        }
        let mut campos = linha.split(':');
        let (Some(nome), Some(_), Some(uid)) = (campos.next(), campos.next(), campos.next()) else {
            continue;
        };
        if let (false, Ok(uid)) = (nome.is_empty(), uid.parse::<u32>()) {
            usuarios.entry(uid).or_insert_with(|| nome.to_owned());
        }
    }
    usuarios
}

// ----- docker / podman -----

/// A saída do `docker inspect --format '{{.Name}}\t{{.State.Status}}\t{{range …}}rede=ip {{end}}\t
/// {{json .Config.ExposedPorts}}\t{{json .NetworkSettings.Ports}}'`, uma linha por contêiner. A
/// separação é por tabulação, ou pelo `\t` escrito, que algumas versões deixam passar.
#[must_use]
pub fn ler_docker(texto: &str) -> Vec<Conteiner> {
    texto.lines().filter_map(ler_linha_docker).collect()
}

fn ler_linha_docker(linha: &str) -> Option<Conteiner> {
    let campos: Vec<&str> = if linha.contains('\t') {
        linha.split('\t').collect()
    } else {
        linha.split("\\t").collect()
    };
    if campos.len() < 5 {
        return None;
    }
    let nome = campos[0].trim().trim_start_matches('/');
    if nome.is_empty() || !campos[1].trim().eq_ignore_ascii_case("running") {
        return None;
    }
    let redes: Vec<(String, IpAddr)> = campos[2]
        .split_whitespace()
        .filter_map(|par| {
            let (rede, ip) = par.split_once('=')?;
            Some((rede.to_owned(), ip.parse().ok()?))
        })
        .collect();

    // Porta do contêiner → porta publicada. As expostas entram sem publicação; as do
    // `NetworkSettings.Ports` trazem o `HostPort`.
    let mut portas: BTreeMap<u16, Option<u16>> = BTreeMap::new();
    if let Ok(serde_json::Value::Object(expostas)) = serde_json::from_str(campos[3].trim()) {
        for chave in expostas.keys() {
            if let Some(porta) = porta_tcp(chave) {
                portas.entry(porta).or_insert(None);
            }
        }
    }
    if let Ok(serde_json::Value::Object(mapa)) = serde_json::from_str(campos[4].trim()) {
        for (chave, destinos) in &mapa {
            let Some(porta) = porta_tcp(chave) else {
                continue;
            };
            let publicada = destinos.as_array().and_then(|lista| {
                lista.iter().find_map(|d| {
                    d.get("HostPort")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|p| p.parse::<u16>().ok())
                })
            });
            let entrada = portas.entry(porta).or_insert(None);
            if publicada.is_some() {
                *entrada = publicada;
            }
        }
    }
    Some(Conteiner {
        nome: nome.to_owned(),
        alcancavel: !redes.is_empty(),
        redes,
        portas: portas.into_iter().collect(),
    })
}

/// `80/tcp` → 80; `53/udp` → nada.
fn porta_tcp(chave: &str) -> Option<u16> {
    let (porta, protocolo) = chave.split_once('/')?;
    (protocolo == "tcp").then(|| porta.parse().ok())?
}

// ----- juntar -----

/// Agrupa as escutas por porta (e por processo, quando a mesma porta tem donos diferentes) e dá
/// a cada grupo o alcance, o destino do túnel, o usuário, o contêiner e o serviço do catálogo.
/// As portas publicadas por um contêiner que o servidor não lista (o Docker sem
/// `userland-proxy`) entram pela saída do próprio `docker`. A ordem é a da porta.
#[must_use]
pub fn juntar(
    escutas: &[Escuta],
    conteineres: &[Conteiner],
    passwd: &HashMap<u32, String>,
) -> Vec<PortaDoServidor> {
    // A chave: porta e dono (o nome do processo ou nada).
    let mut grupos: BTreeMap<(u16, Option<String>), Vec<&Escuta>> = BTreeMap::new();
    for escuta in escutas {
        let dono = escuta.processo.as_ref().map(|p| p.nome.clone());
        grupos.entry((escuta.porta, dono)).or_default().push(escuta);
    }

    let mut portas: Vec<PortaDoServidor> = grupos
        .into_values()
        .map(|grupo| porta_do_grupo(&grupo, conteineres, passwd))
        .collect();

    for conteiner in conteineres {
        for (_, publicada) in &conteiner.portas {
            let Some(publicada) = *publicada else {
                continue;
            };
            if portas.iter().any(|p| p.porta == publicada) {
                continue;
            }
            let endereco = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
            portas.push(PortaDoServidor {
                porta: publicada,
                enderecos: vec![endereco],
                alcance: Alcance::TodaARede,
                destino: "127.0.0.1".to_owned(),
                processo: None,
                usuario: None,
                conteiner: Some(conteiner.nome.clone()),
                servico: servico_do_catalogo(publicada),
                atalhos: Vec::new(),
                inode: None,
            });
        }
    }
    portas.sort_by_key(|p| p.porta);
    portas
}

fn porta_do_grupo(
    grupo: &[&Escuta],
    conteineres: &[Conteiner],
    passwd: &HashMap<u32, String>,
) -> PortaDoServidor {
    let porta = grupo[0].porta;
    let mut enderecos: Vec<IpAddr> = Vec::new();
    for escuta in grupo {
        let endereco = escuta.endereco.to_canonical();
        if !enderecos.contains(&endereco) {
            enderecos.push(endereco);
        }
    }
    enderecos.sort();
    let (alcance, destino) = alcance_e_destino(&enderecos);
    let uid = grupo.iter().find_map(|e| e.uid);
    let usuario = uid.map(|uid| passwd.get(&uid).cloned().unwrap_or_else(|| uid.to_string()));
    let conteiner = conteineres
        .iter()
        .find(|c| {
            c.portas
                .iter()
                .any(|(_, publicada)| *publicada == Some(porta))
        })
        .map(|c| c.nome.clone());
    PortaDoServidor {
        porta,
        enderecos,
        alcance,
        destino,
        processo: grupo.iter().find_map(|e| e.processo.clone()),
        usuario,
        conteiner,
        servico: servico_do_catalogo(porta),
        atalhos: Vec::new(),
        inode: grupo.iter().find_map(|e| e.inode),
    }
}

/// O alcance e o host do túnel (a tabela 3.3 do plano).
fn alcance_e_destino(enderecos: &[IpAddr]) -> (Alcance, String) {
    let aberto_v4 = enderecos
        .iter()
        .any(|e| matches!(e, IpAddr::V4(v4) if v4.is_unspecified()));
    let aberto = aberto_v4 || enderecos.iter().any(|e| e.is_unspecified());
    if aberto {
        let destino = if aberto_v4 { "127.0.0.1" } else { "::1" };
        return (Alcance::TodaARede, destino.to_owned());
    }
    if enderecos.iter().all(IpAddr::is_loopback) {
        let destino = enderecos
            .iter()
            .find(|e| e.is_ipv4())
            .or_else(|| enderecos.first())
            .map_or_else(|| "127.0.0.1".to_owned(), ToString::to_string);
        return (Alcance::SoNoServidor, destino);
    }
    let especifico = enderecos
        .iter()
        .find(|e| !e.is_loopback())
        .copied()
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    (Alcance::So(especifico), especifico.to_string())
}

/// O serviço que o catálogo dá à porta, antes da sonda.
fn servico_do_catalogo(porta: u16) -> Servico {
    match catalogo::porta(porta) {
        Some(item) => Servico::Conhecido {
            nome: item.nome.to_owned(),
            tipo: item.tipo,
            versao: None,
        },
        None => Servico::Desconhecido,
    }
}

/// O tipo do catálogo para a porta, se ela está lá.
#[must_use]
pub fn tipo_da_porta(porta: u16) -> Option<Tipo> {
    catalogo::porta(porta).map(|item| item.tipo)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:0CEA 00000000:0000 0A 00000000:00000000 00:00000000 00000000   110        0 23456 1 0000000000000000 100 0 0 10 0
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 11111 1 0000000000000000 100 0 0 10 0
   2: 0100007F:1F90 0100007F:D6E4 01 00000000:00000000 00:00000000 00000000  1000        0 22222 1 0000000000000000 20 4 30 10 -1
   3: 3500007F:0035 00000000:0000 0A 00000000:00000000 00:00000000 00000000   101        0 33333 1 0000000000000000 100 0 0 10 0
";

    const PROC_TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:1538 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000   999        0 44444 1 0000000000000000 100 0 0 10 0
   1: 00000000000000000000000000000000:0016 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 11112 1 0000000000000000 100 0 0 10 0
";

    #[test]
    fn o_proc_tcp_traz_so_o_listen_com_dono_e_inode() {
        let escutas = ler_proc_tcp(PROC_TCP, false);
        assert_eq!(escutas.len(), 3, "a linha ESTABLISHED não entra");
        assert_eq!(escutas[0].endereco, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[0].porta, 3306);
        assert_eq!(escutas[0].uid, Some(110));
        assert_eq!(escutas[0].inode, Some(23456));
        assert_eq!(escutas[1].endereco, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[1].porta, 22);
        assert_eq!(escutas[2].endereco, "127.0.0.53".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[2].porta, 53);
    }

    #[test]
    fn o_proc_tcp6_le_o_loopback_e_o_aberto() {
        let escutas = ler_proc_tcp(PROC_TCP6, true);
        assert_eq!(escutas.len(), 2);
        assert_eq!(escutas[0].endereco, "::1".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[0].porta, 5432);
        assert_eq!(escutas[1].endereco, "::".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn o_proc_de_um_servidor_big_endian_e_relido_do_outro_lado() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 7F000001:0CEA 00000000:0000 0A 00000000:00000000 00:00000000 00000000   110        0 23456 1 0
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 11111 1 0
";
        let escutas = ler_proc_tcp(tcp, false);
        assert_eq!(escutas[0].endereco, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[0].porta, 3306);
        assert_eq!(escutas[1].endereco, "0.0.0.0".parse::<IpAddr>().unwrap());
        let tcp6 = "  sl  local_address remote_address st tx_queue rx_queue tr tm->when retrnsmt uid timeout inode
   0: 00000000000000000000000000000001:1538 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000   999        0 44444 1 0
";
        let escutas = ler_proc_tcp(tcp6, true);
        assert_eq!(escutas[0].endereco, "::1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn linha_torta_do_proc_e_ignorada() {
        let tcp = "cabeçalho\nlixo\n   0: 0100007F:0CEA 00000000:0000 0A\n   1: ZZ:ZZ 0 0A 0 0 0 0 0 0 0\n";
        assert!(ler_proc_tcp(tcp, false).is_empty());
        assert!(ler_proc_tcp("", true).is_empty());
    }

    #[test]
    fn o_ss_traz_o_processo_e_aceita_as_formas_de_endereco() {
        let saida = "LISTEN 0      4096       127.0.0.1:3306       0.0.0.0:*    users:((\"mysqld\",pid=812,fd=21))
LISTEN 0      128          0.0.0.0:22         0.0.0.0:*
LISTEN 0      511             [::]:80            [::]:*    users:((\"nginx\",pid=900,fd=7),(\"nginx\",pid=901,fd=7))
LISTEN 0      4096   127.0.0.53%lo:53         0.0.0.0:*
LISTEN 0      4096               *:3000               *:*
LISTEN 0      244            [::1]:5432            [::]:*
ESTAB  0 0 1.1.1.1:22 2.2.2.2:5555
";
        let escutas = ler_ss(saida);
        assert_eq!(escutas.len(), 6);
        assert_eq!(escutas[0].porta, 3306);
        assert_eq!(
            escutas[0].processo,
            Some(Processo {
                pid: Some(812),
                nome: "mysqld".into()
            })
        );
        assert_eq!(escutas[1].processo, None);
        assert_eq!(escutas[2].endereco, "::".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[2].processo.as_ref().unwrap().pid, Some(900));
        assert_eq!(escutas[3].endereco, "127.0.0.53".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[4].endereco, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[5].endereco, "::1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn o_ss_com_e_traz_o_dono_e_o_inode() {
        let saida = "LISTEN 0 4096 127.0.0.1:3306 0.0.0.0:* users:((\"mysqld\",pid=812,fd=21)) uid:110 ino:23456 sk:1a cgroup:/system.slice/mysql.service <->
LISTEN 0 128 0.0.0.0:22 0.0.0.0:* uid:0 ino:11111 sk:2
";
        let escutas = ler_ss(saida);
        assert_eq!(escutas.len(), 2);
        assert_eq!((escutas[0].uid, escutas[0].inode), (Some(110), Some(23456)));
        assert_eq!(escutas[0].processo.as_ref().unwrap().pid, Some(812));
        assert_eq!((escutas[1].uid, escutas[1].inode), (Some(0), Some(11111)));
    }

    #[test]
    fn o_netstat_do_linux_e_dos_bsd() {
        let linux = "Active Internet connections (servers and established)
Proto Recv-Q Send-Q Local Address           Foreign Address         State
tcp        0      0 0.0.0.0:22              0.0.0.0:*               LISTEN
tcp        0      0 127.0.0.1:3306          0.0.0.0:*               LISTEN
tcp6       0      0 :::80                   :::*                    LISTEN
tcp        0      0 10.0.0.5:22             10.0.0.9:5555           ESTABLISHED
udp        0      0 0.0.0.0:68              0.0.0.0:*
";
        let escutas = ler_netstat(linux);
        assert_eq!(escutas.len(), 3);
        assert_eq!(escutas[2].endereco, "::".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[2].porta, 80);

        let bsd = "Active Internet connections (including servers)
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)
tcp4       0      0  127.0.0.1.3306         *.*                    LISTEN
tcp4       0      0  *.22                   *.*                    LISTEN
tcp6       0      0  ::1.5432               *.*                    LISTEN
tcp46      0      0  *.80                   *.*                    LISTEN
tcp4       0      0  10.0.0.5.22            10.0.0.9.5555          ESTABLISHED
";
        let escutas = ler_netstat(bsd);
        assert_eq!(escutas.len(), 4);
        assert_eq!(escutas[0].endereco, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[0].porta, 3306);
        assert_eq!(escutas[1].endereco, "0.0.0.0".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[2].endereco, "::1".parse::<IpAddr>().unwrap());
        assert_eq!(escutas[3].porta, 80);
    }

    #[test]
    fn o_passwd_vira_um_mapa_de_uid_para_nome() {
        let mapa = ler_passwd(
            "# comentário\nroot:x:0:0:root:/root:/bin/bash\nmysql:x:110:116:MySQL:/nonexistent:/bin/false\n\nruim\n:x:5:5\n",
        );
        assert_eq!(mapa.get(&0).map(String::as_str), Some("root"));
        assert_eq!(mapa.get(&110).map(String::as_str), Some("mysql"));
        assert_eq!(mapa.len(), 2);
    }

    #[test]
    fn o_docker_traz_ip_portas_expostas_e_publicadas() {
        let saida = "/web\trunning\tbridge=172.17.0.2 \t{\"80/tcp\":{}}\t{\"80/tcp\":null}
/banco\trunning\tbridge=172.17.0.3 rede2=172.20.0.4 \t{\"3306/tcp\":{},\"33060/tcp\":{}}\t{\"3306/tcp\":[{\"HostIp\":\"0.0.0.0\",\"HostPort\":\"3307\"}],\"33060/tcp\":null}
/parado\texited\t\tnull\tnull
/semrede\trunning\t\tnull\t{}
";
        let conteineres = ler_docker(saida);
        assert_eq!(conteineres.len(), 3, "o parado não entra");
        assert_eq!(conteineres[0].nome, "web");
        assert_eq!(conteineres[0].portas, vec![(80, None)]);
        assert!(conteineres[0].alcancavel);
        assert_eq!(conteineres[0].ip(), Some("172.17.0.2".parse().unwrap()));
        assert_eq!(conteineres[1].redes.len(), 2);
        assert_eq!(
            conteineres[1].portas,
            vec![(3306, Some(3307)), (33060, None)]
        );
        assert_eq!(conteineres[2].nome, "semrede");
        assert!(!conteineres[2].alcancavel);
        assert!(conteineres[2].portas.is_empty());
    }

    #[test]
    fn o_docker_com_o_tab_escrito_tambem_le() {
        let saida = "/web\\trunning\\tbridge=172.17.0.2 \\t{\"80/tcp\":{}}\\tnull\n";
        let conteineres = ler_docker(saida);
        assert_eq!(conteineres.len(), 1);
        assert_eq!(conteineres[0].portas, vec![(80, None)]);
    }

    fn escuta(endereco: &str, porta: u16, uid: Option<u32>, processo: Option<&str>) -> Escuta {
        Escuta {
            endereco: endereco.parse().unwrap(),
            porta,
            uid,
            inode: uid.map(u64::from),
            processo: processo.map(|nome| Processo {
                pid: Some(1),
                nome: nome.into(),
            }),
        }
    }

    #[test]
    fn juntar_agrupa_a_porta_e_decide_alcance_e_destino() {
        let passwd = ler_passwd("mysql:x:110:1:::\n");
        let portas = juntar(
            &[
                escuta("0.0.0.0", 80, Some(0), None),
                escuta("::", 80, Some(0), None),
                escuta("127.0.0.1", 3306, Some(110), None),
                escuta("::1", 5432, Some(999), None),
                escuta("127.0.0.53", 53, None, None),
                escuta("10.0.0.5", 9999, None, None),
                escuta("::", 8000, None, None),
            ],
            &[],
            &passwd,
        );
        let por_porta = |p: u16| portas.iter().find(|x| x.porta == p).unwrap();
        let http = por_porta(80);
        assert_eq!(http.alcance, Alcance::TodaARede);
        assert_eq!(http.destino, "127.0.0.1");
        assert_eq!(http.enderecos.len(), 2, "IPv4 e IPv6 viram uma linha");
        assert_eq!(http.usuario.as_deref(), Some("0"));
        let mysql = por_porta(3306);
        assert_eq!(mysql.alcance, Alcance::SoNoServidor);
        assert_eq!(mysql.destino, "127.0.0.1");
        assert_eq!(mysql.usuario.as_deref(), Some("mysql"));
        assert_eq!(por_porta(5432).destino, "::1");
        assert_eq!(por_porta(53).destino, "127.0.0.53");
        let ip = por_porta(9999);
        assert_eq!(ip.alcance, Alcance::So("10.0.0.5".parse().unwrap()));
        assert_eq!(ip.destino, "10.0.0.5");
        assert_eq!(por_porta(8000).destino, "::1", "só o `::` vai por `::1`");
        let ordem: Vec<u16> = portas.iter().map(|p| p.porta).collect();
        assert!(ordem.windows(2).all(|par| par[0] <= par[1]), "{ordem:?}");
    }

    #[test]
    fn juntar_separa_a_mesma_porta_de_donos_diferentes() {
        let portas = juntar(
            &[
                escuta("127.0.0.1", 8080, None, Some("node")),
                escuta("::1", 8080, None, Some("node")),
                escuta("10.0.0.5", 8080, None, Some("nginx")),
            ],
            &[],
            &HashMap::new(),
        );
        assert_eq!(portas.len(), 2);
        let nomes: Vec<&str> = portas
            .iter()
            .map(|p| p.processo.as_ref().unwrap().nome.as_str())
            .collect();
        assert!(
            nomes.contains(&"node") && nomes.contains(&"nginx"),
            "{nomes:?}"
        );
    }

    #[test]
    fn juntar_marca_o_conteiner_e_traz_a_publicada_que_o_proc_nao_lista() {
        let conteineres = ler_docker(
            "/banco\trunning\tbridge=172.17.0.3 \t{\"3306/tcp\":{}}\t{\"3306/tcp\":[{\"HostIp\":\"0.0.0.0\",\"HostPort\":\"3307\"}]}\n",
        );
        let portas = juntar(
            &[escuta("0.0.0.0", 3307, None, None)],
            &conteineres,
            &HashMap::new(),
        );
        assert_eq!(portas.len(), 1);
        assert_eq!(portas[0].conteiner.as_deref(), Some("banco"));
        // Sem `userland-proxy`, a 3307 não está no /proc: entra pela saída do docker.
        let portas = juntar(&[], &conteineres, &HashMap::new());
        assert_eq!(portas.len(), 1);
        assert_eq!(portas[0].porta, 3307);
        assert_eq!(portas[0].conteiner.as_deref(), Some("banco"));
        assert_eq!(portas[0].destino, "127.0.0.1");
    }

    #[test]
    fn juntar_da_nome_pelo_catalogo_e_deixa_o_resto_desconhecido() {
        let portas = juntar(
            &[
                escuta("127.0.0.1", 3306, None, None),
                escuta("127.0.0.1", 4567, None, None),
                escuta("0.0.0.0", 22, None, None),
            ],
            &[],
            &HashMap::new(),
        );
        assert!(matches!(
            &portas.iter().find(|p| p.porta == 3306).unwrap().servico,
            Servico::Conhecido { nome, tipo: Tipo::Banco, .. } if nome == "MySQL/MariaDB"
        ));
        assert_eq!(
            portas.iter().find(|p| p.porta == 4567).unwrap().servico,
            Servico::Desconhecido
        );
        assert!(portas.iter().find(|p| p.porta == 22).unwrap().de_sistema());
    }
}
