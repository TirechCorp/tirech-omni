//! O autocompletar do terminal (Ctrl+Espaço): os comandos que cabem na aba, de onde vem cada um
//! e o filtro. Os túneis dão os comandos prontos para usar a porta (o cliente de área de trabalho
//! remota numa porta que leva à 3389, o `curl` pelo SOCKS5 do dinâmico); o resto vem do que a
//! pessoa já digitou. A tela fica em `tela/autocompletar.rs`; nada aqui executa: o comando escolhido
//! vai para a linha do terminal, sem Enter.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;

use crate::t;

/// Quantas linhas do fim de cada histórico de shell entram.
const LINHAS_DO_HISTORICO: usize = 500;

/// Quanto do fim de um arquivo de histórico se lê (um histórico de anos não trava a abertura).
const BYTES_DO_HISTORICO: u64 = 512 * 1024;

/// Quantas sugestões a lista mostra.
pub const LIMITE: usize = 200;

/// Uma linha da lista: o comando e de onde ele veio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sugestao {
    /// O que vai para o terminal.
    pub comando: String,
    /// De onde veio, para a coluna "Origem".
    pub origem: String,
}

impl Sugestao {
    fn nova(comando: impl Into<String>, origem: &str) -> Self {
        Self {
            comando: comando.into(),
            origem: origem.to_owned(),
        }
    }
}

/// Um túnel que a aba enxerga: os do shell local pela rede do perfil e os `-L` e `-D` das abas
/// SSH abertas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunelVisto {
    /// Onde a porta escuta de verdade (a que o sistema escolheu, com "Usar a próxima porta livre").
    pub escuta: SocketAddr,
    /// O destino do túnel local; `None` no dinâmico (SOCKS5).
    pub destino: Option<(String, u16)>,
    /// O título da aba SSH que abriu o túnel; `None` se é desta aba.
    pub aba: Option<String>,
}

/// O sistema em que o app roda, que escolhe o programa de área de trabalho remota.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sistema {
    /// Linux e os outros Unix: o `xfreerdp`.
    Linux,
    /// Windows: o `mstsc`.
    Windows,
    /// macOS: o `open` com a URL `rdp://`.
    Mac,
}

impl Sistema {
    /// O deste computador.
    pub fn atual() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}

/// O endereço que um programa daqui usa para chegar à porta: quem escuta em todas as
/// interfaces (`0.0.0.0`) também atende no `127.0.0.1`.
fn host_local(escuta: SocketAddr) -> String {
    match escuta.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".to_owned(),
        IpAddr::V6(ip) if ip.is_unspecified() || ip.is_loopback() => "::1".to_owned(),
        ip => ip.to_string(),
    }
}

/// `host:porta`, com colchetes no IPv6.
fn com_porta(host: &str, porta: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{porta}")
    } else {
        format!("{host}:{porta}")
    }
}

/// As portas de destino que costumam falar HTTP e HTTPS.
const HTTP: &[u16] = &[80, 3000, 5000, 8000, 8008, 8080, 8081, 8888, 9000];
const HTTPS: &[u16] = &[443, 8443, 9443];

/// Os comandos para a porta local `porta` (em `host`) que leva à `destino` do outro lado.
/// `no_path` diz se um programa está instalado.
fn comandos_da_porta(
    host: &str,
    porta: u16,
    destino: u16,
    sistema: Sistema,
    no_path: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let endereco = com_porta(host, porta);
    match destino {
        3389 => match sistema {
            Sistema::Windows => vec![format!("mstsc /v:{endereco}")],
            Sistema::Mac => vec![format!("open \"rdp://full%20address=s:{endereco}\"")],
            Sistema::Linux => {
                let achados: Vec<&str> = ["xfreerdp", "xfreerdp3"]
                    .into_iter()
                    .filter(|p| no_path(p))
                    .collect();
                let programas = if achados.is_empty() {
                    vec!["xfreerdp"]
                } else {
                    achados
                };
                programas
                    .into_iter()
                    .map(|p| format!("{p} /v:{endereco} /dynamic-resolution /u:"))
                    .collect()
            }
        },
        5900..=5999 => vec![format!("vncviewer {host}::{porta}")],
        22 => vec![format!("ssh -p {porta} {host}")],
        3306 => vec![format!("mysql -h {host} -P {porta} -u ")],
        5432 => vec![format!("psql -h {host} -p {porta} -U ")],
        6379 => vec![format!("redis-cli -h {host} -p {porta}")],
        27017 => vec![format!("mongosh --host {host} --port {porta}")],
        1433 => vec![format!("sqlcmd -S {host},{porta} -U ")],
        d if HTTPS.contains(&d) => vec![format!("curl -k https://{endereco}/")],
        d if HTTP.contains(&d) => vec![format!("curl http://{endereco}/")],
        _ => vec![format!("nc {host} {porta}")],
    }
}

/// As sugestões dos túneis, na ordem deles.
pub fn dos_tuneis(
    tuneis: &[TunelVisto],
    sistema: Sistema,
    no_path: &dyn Fn(&str) -> bool,
) -> Vec<Sugestao> {
    let mut lista = Vec::new();
    for tunel in tuneis {
        let host = host_local(tunel.escuta);
        let porta = tunel.escuta.port();
        let escuta = com_porta(&host, porta);
        let (origem, comandos) = match &tunel.destino {
            Some((destino_host, destino_porta)) => (
                t!(
                    "autocompletar-origem-tunel",
                    escuta = escuta.as_str(),
                    destino = com_porta(destino_host, *destino_porta)
                ),
                comandos_da_porta(&host, porta, *destino_porta, sistema, no_path),
            ),
            None => {
                let proxy = format!("socks5h://{escuta}");
                let mut comandos = vec![format!("curl --proxy {proxy} ")];
                if sistema != Sistema::Windows {
                    comandos.push(format!("export ALL_PROXY={proxy}"));
                }
                (
                    t!("autocompletar-origem-dinamico", escuta = escuta.as_str()),
                    comandos,
                )
            }
        };
        let origem = match &tunel.aba {
            Some(aba) => t!(
                "autocompletar-origem-da-aba",
                origem = origem.as_str(),
                aba = aba.as_str()
            ),
            None => origem,
        };
        lista.extend(comandos.into_iter().map(|c| Sugestao::nova(c, &origem)));
    }
    lista
}

/// Os comandos de uma lista (o histórico da barra, os escolhidos aqui, os passos ao conectar),
/// todos com a mesma origem.
pub fn da_lista<'a>(comandos: impl IntoIterator<Item = &'a str>, origem: &str) -> Vec<Sugestao> {
    comandos
        .into_iter()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(|c| Sugestao::nova(c, origem))
        .collect()
}

/// As linhas do `~/.bash_history`, sem as marcas de hora (`#1700000000`).
fn linhas_bash(texto: &str) -> Vec<String> {
    texto
        .lines()
        .filter(|l| {
            !l.strip_prefix('#')
                .is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
        })
        .map(str::to_owned)
        .collect()
}

/// As linhas do `~/.zsh_history`, sem o `: 1700000000:0;` do formato estendido. Comandos de
/// várias linhas (terminados em `\`) ficam de fora.
fn linhas_zsh(texto: &str) -> Vec<String> {
    let mut linhas = Vec::new();
    let mut continua = false;
    for linha in texto.lines() {
        let antes = continua;
        continua = linha.ends_with('\\');
        if antes || continua {
            continue;
        }
        let comando = linha
            .strip_prefix(": ")
            .and_then(|r| r.split_once(';'))
            .map_or(linha, |(_, c)| c);
        linhas.push(comando.to_owned());
    }
    linhas
}

/// Os comandos do `fish_history` (as linhas `- cmd: ...`), com o `\\` desfeito. Os de várias
/// linhas (com `\n`) ficam de fora.
fn linhas_fish(texto: &str) -> Vec<String> {
    texto
        .lines()
        .filter_map(|l| l.strip_prefix("- cmd: "))
        .filter(|c| !c.replace("\\\\", "").contains("\\n"))
        .map(|c| c.replace("\\\\", "\\"))
        .collect()
}

/// O fim do arquivo `caminho` (até [`BYTES_DO_HISTORICO`]), começando numa linha inteira.
fn ler_o_fim(caminho: &Path) -> Option<String> {
    let mut arquivo = File::open(caminho).ok()?;
    let tamanho = arquivo.metadata().ok()?.len();
    let inicio = tamanho.saturating_sub(BYTES_DO_HISTORICO);
    arquivo.seek(SeekFrom::Start(inicio)).ok()?;
    let mut bytes = Vec::new();
    arquivo.read_to_end(&mut bytes).ok()?;
    let mut texto = String::from_utf8_lossy(&bytes).into_owned();
    if inicio > 0 {
        let corte = texto.find('\n').map_or(texto.len(), |i| i + 1);
        texto.drain(..corte);
    }
    Some(texto)
}

/// Os históricos dos shells desta máquina, o mais recente primeiro em cada um: o do bash e o do
/// zsh em `home`, o do fish em `dados/fish` (o `XDG_DATA_HOME`).
pub fn dos_shells(home: &Path, dados: &Path) -> Vec<Sugestao> {
    type Leitor = fn(&str) -> Vec<String>;
    let fontes: [(std::path::PathBuf, &str, Leitor); 3] = [
        (home.join(".bash_history"), "bash", linhas_bash),
        (home.join(".zsh_history"), "zsh", linhas_zsh),
        (dados.join("fish").join("fish_history"), "fish", linhas_fish),
    ];
    let mut lista = Vec::new();
    for (caminho, shell, ler) in fontes {
        let Some(texto) = ler_o_fim(&caminho) else {
            continue;
        };
        let origem = t!("autocompletar-origem-shell", shell = shell);
        let linhas = ler(&texto);
        let recentes = linhas.iter().rev().take(LINHAS_DO_HISTORICO);
        lista.extend(da_lista(recentes.map(String::as_str), &origem));
    }
    lista
}

/// Junta as fontes na ordem dada, sem repetir comando (fica o primeiro).
pub fn juntar(fontes: impl IntoIterator<Item = Vec<Sugestao>>) -> Vec<Sugestao> {
    let mut vistos = std::collections::HashSet::new();
    fontes
        .into_iter()
        .flatten()
        .filter(|s| vistos.insert(s.comando.clone()))
        .collect()
}

/// As sugestões que têm todas as palavras de `digitado`, sem diferenciar maiúsculas, até
/// [`LIMITE`]. Com o campo vazio, todas.
pub fn filtrar(lista: &[Sugestao], digitado: &str) -> Vec<Sugestao> {
    let palavras: Vec<String> = digitado.split_whitespace().map(str::to_lowercase).collect();
    lista
        .iter()
        .filter(|s| {
            let comando = s.comando.to_lowercase();
            palavras.iter().all(|p| comando.contains(p.as_str()))
        })
        .take(LIMITE)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tunel(escuta: &str, destino: Option<(&str, u16)>) -> TunelVisto {
        TunelVisto {
            escuta: escuta.parse().unwrap(),
            destino: destino.map(|(h, p)| (h.to_owned(), p)),
            aba: None,
        }
    }

    fn comandos(lista: &[Sugestao]) -> Vec<&str> {
        lista.iter().map(|s| s.comando.as_str()).collect()
    }

    #[test]
    fn o_rdp_usa_a_porta_de_verdade_e_o_programa_instalado() {
        let rdp = [tunel("127.0.0.1:13390", Some(("10.0.0.5", 3389)))];
        let nada = |_: &str| false;
        assert_eq!(
            comandos(&dos_tuneis(&rdp, Sistema::Linux, &nada)),
            ["xfreerdp /v:127.0.0.1:13390 /dynamic-resolution /u:"]
        );
        let so_o_3 = |p: &str| p == "xfreerdp3";
        assert_eq!(
            comandos(&dos_tuneis(&rdp, Sistema::Linux, &so_o_3)),
            ["xfreerdp3 /v:127.0.0.1:13390 /dynamic-resolution /u:"]
        );
        let os_dois = |_: &str| true;
        assert_eq!(dos_tuneis(&rdp, Sistema::Linux, &os_dois).len(), 2);
        assert_eq!(
            comandos(&dos_tuneis(&rdp, Sistema::Windows, &nada)),
            ["mstsc /v:127.0.0.1:13390"]
        );
        assert_eq!(
            comandos(&dos_tuneis(&rdp, Sistema::Mac, &nada)),
            ["open \"rdp://full%20address=s:127.0.0.1:13390\""]
        );
        let origem = &dos_tuneis(&rdp, Sistema::Linux, &nada)[0].origem;
        assert!(origem.contains("127.0.0.1:13390"), "{origem}");
        assert!(origem.contains("10.0.0.5:3389"), "{origem}");
    }

    #[test]
    fn quem_escuta_em_todas_as_interfaces_e_usado_pelo_127() {
        let lista = dos_tuneis(
            &[tunel("0.0.0.0:2222", Some(("bastiao", 22)))],
            Sistema::Linux,
            &|_| false,
        );
        assert_eq!(comandos(&lista), ["ssh -p 2222 127.0.0.1"]);
        let lista = dos_tuneis(
            &[tunel("[::]:8080", Some(("web", 80)))],
            Sistema::Linux,
            &|_| false,
        );
        assert_eq!(comandos(&lista), ["curl http://[::1]:8080/"]);
    }

    #[test]
    fn cada_porta_conhecida_tem_o_seu_cliente() {
        let casos = [
            (5901, "vncviewer 127.0.0.1::4000"),
            (3306, "mysql -h 127.0.0.1 -P 4000 -u "),
            (5432, "psql -h 127.0.0.1 -p 4000 -U "),
            (6379, "redis-cli -h 127.0.0.1 -p 4000"),
            (27017, "mongosh --host 127.0.0.1 --port 4000"),
            (1433, "sqlcmd -S 127.0.0.1,4000 -U "),
            (443, "curl -k https://127.0.0.1:4000/"),
            (8080, "curl http://127.0.0.1:4000/"),
            (25, "nc 127.0.0.1 4000"),
        ];
        for (destino, esperado) in casos {
            let lista = dos_tuneis(
                &[tunel("127.0.0.1:4000", Some(("alvo", destino)))],
                Sistema::Linux,
                &|_| false,
            );
            assert_eq!(comandos(&lista), [esperado], "destino {destino}");
        }
    }

    #[test]
    fn o_dinamico_da_o_proxy_e_a_origem_diz_a_aba() {
        let mut d = tunel("127.0.0.1:1080", None);
        d.aba = Some("bastião".into());
        let lista = dos_tuneis(&[d], Sistema::Linux, &|_| false);
        assert_eq!(
            comandos(&lista),
            [
                "curl --proxy socks5h://127.0.0.1:1080 ",
                "export ALL_PROXY=socks5h://127.0.0.1:1080"
            ]
        );
        assert!(lista[0].origem.contains("bastião"), "{}", lista[0].origem);
        let no_windows = dos_tuneis(&[tunel("127.0.0.1:1080", None)], Sistema::Windows, &|_| {
            false
        });
        assert_eq!(no_windows.len(), 1);
    }

    #[test]
    fn os_historicos_dos_shells_perdem_as_marcas() {
        assert_eq!(
            linhas_bash("#1700000000\nls -la\n#\ngit status\n"),
            ["ls -la", "#", "git status"]
        );
        assert_eq!(
            linhas_zsh(": 1700000000:0;ls -la\nuptime\n: 1:0;for i in 1 \\\n2; do\n: 2:0;pwd\n"),
            ["ls -la", "uptime", "pwd"]
        );
        assert_eq!(
            linhas_fish("- cmd: ls\n  when: 1\n- cmd: echo a\\\\b\n- cmd: echo 1\\necho 2\n"),
            ["ls", "echo a\\b"]
        );
    }

    #[test]
    fn os_shells_vem_do_mais_recente_e_o_fim_do_arquivo_comeca_numa_linha() {
        let pasta = std::env::temp_dir().join(format!("autocompletar-{}", std::process::id()));
        let dados = pasta.join("dados");
        std::fs::create_dir_all(dados.join("fish")).unwrap();
        std::fs::write(pasta.join(".bash_history"), "antigo\nnovo\n").unwrap();
        std::fs::write(dados.join("fish/fish_history"), "- cmd: peixe\n").unwrap();
        let lista = dos_shells(&pasta, &dados);
        assert_eq!(comandos(&lista), ["novo", "antigo", "peixe"]);
        assert!(lista[0].origem.contains("bash"), "{}", lista[0].origem);

        let grande = format!("{}\nfim\n", "x".repeat(BYTES_DO_HISTORICO as usize));
        std::fs::write(pasta.join(".bash_history"), grande).unwrap();
        let lista = dos_shells(&pasta, &dados);
        assert_eq!(comandos(&lista), ["fim", "peixe"]);
        std::fs::remove_dir_all(&pasta).unwrap();
    }

    #[test]
    fn juntar_nao_repete_e_o_filtro_quer_todas_as_palavras() {
        let lista = juntar([
            da_lista(["xfreerdp /v:x", "ls"], "a"),
            da_lista(["ls", "  ", "git log"], "b"),
        ]);
        assert_eq!(comandos(&lista), ["xfreerdp /v:x", "ls", "git log"]);
        assert_eq!(lista[1].origem, "a");
        assert_eq!(comandos(&filtrar(&lista, "")), comandos(&lista));
        assert_eq!(comandos(&filtrar(&lista, "RDP x")), ["xfreerdp /v:x"]);
        assert!(filtrar(&lista, "nada").is_empty());
    }
}
