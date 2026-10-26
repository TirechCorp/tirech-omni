//! O contexto que a IA recebe do terminal (M52): a seleção, as últimas linhas, o "último erro" e o
//! sistema do outro lado. É puro: quem lê a `Session` e o SFTP monta as entradas e chama estas
//! funções.
//!
//! Nada aqui executa comando para descobrir o sistema: o `/etc/os-release` e o `/etc/passwd` são
//! lidos pelo SFTP da própria sessão (em `tela/assistente.rs`) e chegam aqui como texto.

use crate::comandos;

/// Os caracteres que fecham um prompt de shell.
const FIM_DE_PROMPT: [char; 4] = ['$', '#', '%', '>'];

/// De onde veio o que sabemos do sistema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FonteSistema {
    /// Lido do servidor (SFTP) ou deste computador.
    Servidor,
    /// Suposto (Telnet, serial ou servidor sem SFTP): vale o `ia.comando_shell`.
    Suposto,
}

/// O sistema onde o terminal roda.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SistemaRemoto {
    /// O nome do sistema ("Ubuntu 24.04.1 LTS"); vazio quando desconhecido.
    pub so: String,
    /// O shell ("/bin/bash", "bash").
    pub shell: String,
    /// De onde isso veio.
    pub fonte: FonteSistema,
}

impl SistemaRemoto {
    /// O sistema suposto, pelo `ia.comando_shell`.
    pub fn suposto(shell: &str) -> Self {
        Self {
            so: String::new(),
            shell: shell.trim().to_owned(),
            fonte: FonteSistema::Suposto,
        }
    }

    /// A frase do prompt de sistema: "Sistema: Ubuntu 24.04.1 LTS; shell: /bin/bash (lidos do
    /// servidor)." ou "Sistema desconhecido; suponha bash.".
    pub fn frase(&self) -> String {
        let shell = if self.shell.trim().is_empty() {
            "bash"
        } else {
            self.shell.trim()
        };
        if self.so.trim().is_empty() {
            return format!("Sistema desconhecido; suponha {shell}.");
        }
        let origem = match self.fonte {
            FonteSistema::Servidor => "lidos do servidor",
            FonteSistema::Suposto => "supostos",
        };
        format!("Sistema: {}; shell: {shell} ({origem}).", self.so.trim())
    }
}

/// O que se junta do terminal para uma pergunta.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contexto {
    /// O texto selecionado.
    pub selecao: Option<String>,
    /// As últimas `ia.linhas_contexto` linhas.
    pub linhas: Option<String>,
    /// O último comando que deu erro, com a saída.
    pub ultimo_erro: Option<UltimoErro>,
    /// O sistema do terminal.
    pub sistema: Option<SistemaRemoto>,
    /// Quantas linhas antigas o `ia.max_caracteres` cortou.
    pub cortadas: usize,
}

/// O "último erro" (seção 7.3 do plano): o comando e o que ele escreveu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UltimoErro {
    /// O comando digitado no prompt anterior, quando foi reconhecido.
    pub comando: Option<String>,
    /// A saída do comando.
    pub saida: String,
    /// Se o prompt foi reconhecido. `false`: foram as últimas linhas todas, e a interface avisa.
    pub achou: bool,
    /// Quantas linhas tem a saída (para a frase "mandei as últimas {n} linhas").
    pub linhas: usize,
}

/// Se a linha termina num prompt de shell (`#`, `$`, `%` ou `>`). É a mesma regra do M50
/// (`comandos::eh_prompt`): o passo de senha dos comandos de conexão também usa.
pub fn eh_prompt(linha: &str) -> bool {
    comandos::eh_prompt(linha)
}

/// As últimas `n` linhas de `texto`, sem as linhas vazias do fim, e quantas ficaram de fora.
pub fn ultimas_linhas(texto: &str, n: usize) -> (String, usize) {
    let linhas: Vec<&str> = linhas_sem_fim_vazio(texto);
    let n = n.max(1);
    let inicio = linhas.len().saturating_sub(n);
    (linhas[inicio..].join("\n"), inicio)
}

/// As linhas do texto sem as vazias do fim (e sem os espaços do fim de cada uma).
fn linhas_sem_fim_vazio(texto: &str) -> Vec<&str> {
    let mut linhas: Vec<&str> = texto.lines().map(str::trim_end).collect();
    while linhas.last().is_some_and(|l| l.is_empty()) {
        linhas.pop();
    }
    linhas
}

/// Limita `texto` a `max` caracteres, cortando as linhas **mais antigas** (o começo). `max == 0`
/// é sem limite. Devolve o texto e quantas linhas inteiras saíram; se nem a última linha cabe,
/// ela perde o começo.
pub fn limitar_caracteres(texto: &str, max: usize) -> (String, usize) {
    if max == 0 || texto.chars().count() <= max {
        return (texto.to_owned(), 0);
    }
    let linhas: Vec<&str> = texto.lines().collect();
    let mut total = 0usize;
    let mut inicio = linhas.len();
    for (i, linha) in linhas.iter().enumerate().rev() {
        // +1 da quebra de linha entre as linhas.
        let custo = linha.chars().count() + usize::from(i + 1 < linhas.len());
        if total + custo > max {
            break;
        }
        total += custo;
        inicio = i;
    }
    if inicio == linhas.len() {
        // Nem a última linha cabe: fica o fim dela.
        let ultima = linhas.last().copied().unwrap_or("");
        let pular = ultima.chars().count().saturating_sub(max);
        let fim: String = ultima.chars().skip(pular).collect();
        return (fim, linhas.len().saturating_sub(1));
    }
    (linhas[inicio..].join("\n"), inicio)
}

/// O começo de um prompt: até o primeiro `:`, espaço ou `]` (`root@web` em `root@web:~# `,
/// `[ana@db` em `[ana@db tmp]$ `).
fn comeco_do_prompt(prompt: &str) -> &str {
    let fim = prompt.find([':', ' ', ']']).unwrap_or(prompt.len());
    &prompt[..fim]
}

/// Se `linha` é um prompt **anterior** (começa como o atual e tem o fim de prompt seguido do
/// comando), o comando digitado depois dele (vazio se a pessoa só deu Enter).
fn comando_da_linha_de_prompt(linha: &str, comeco: &str) -> Option<String> {
    let resto = linha.strip_prefix(comeco)?;
    let ate = resto.trim_end();
    // O primeiro fim de prompt seguido de espaço (ou do fim da linha).
    let mut posicao = None;
    let mut chars = ate.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if FIM_DE_PROMPT.contains(&c) && chars.peek().is_none_or(|(_, seguinte)| *seguinte == ' ') {
            posicao = Some(i + c.len_utf8());
            break;
        }
    }
    let i = posicao?;
    Some(ate[i..].trim().to_owned())
}

/// O "último erro" das últimas `n` linhas do histórico (seção 7.3 do plano).
///
/// - O prompt atual é a última linha não vazia, se for um prompt.
/// - O prompt anterior é a última linha acima dela que começa como o atual e tem um comando
///   depois do fim do prompt. O comando é o que vem depois; a saída é o que está entre os dois.
/// - Sem prompt anterior (a saída é maior que o histórico, ou o prompt é exótico), a saída é
///   tudo o que está acima do prompt atual, sem comando.
/// - Sem prompt nenhum, a saída são as `n` linhas todas e `achou` fica `false`.
pub fn ultimo_erro(historico: &str, n: usize) -> UltimoErro {
    let (janela, _) = ultimas_linhas(historico, n);
    let linhas: Vec<&str> = janela.lines().collect();
    let sem_prompt = |linhas: &[&str]| UltimoErro {
        comando: None,
        saida: linhas.join("\n"),
        achou: false,
        linhas: linhas.len(),
    };
    let Some(atual) = linhas.last().copied().filter(|l| eh_prompt(l)) else {
        return sem_prompt(&linhas);
    };
    let acima = &linhas[..linhas.len() - 1];
    let comeco = comeco_do_prompt(atual);
    // Um começo curto demais (um `$ ` sozinho) casaria com qualquer linha.
    let anterior = (comeco.chars().count() >= 2)
        .then(|| {
            acima.iter().enumerate().rev().find_map(|(i, linha)| {
                let comando = comando_da_linha_de_prompt(linha, comeco)?;
                (!comando.is_empty()).then_some((i, comando))
            })
        })
        .flatten();
    match anterior {
        Some((i, comando)) => {
            let saida: Vec<&str> = acima[i + 1..]
                .iter()
                .copied()
                // Os Enter dados no prompt vazio não são saída.
                .filter(|l| comando_da_linha_de_prompt(l, comeco).is_none_or(|c| !c.is_empty()))
                .collect();
            UltimoErro {
                comando: Some(comando),
                linhas: saida.len(),
                saida: saida.join("\n"),
                achou: true,
            }
        }
        None => UltimoErro {
            comando: None,
            linhas: acima.len(),
            saida: acima.join("\n"),
            achou: true,
        },
    }
}

/// O `PRETTY_NAME` do `/etc/os-release` (ou o `NAME` com o `VERSION_ID`).
pub fn nome_do_os_release(texto: &str) -> Option<String> {
    let valor = |chave: &str| {
        texto.lines().find_map(|linha| {
            let (c, v) = linha.trim().split_once('=')?;
            (c.trim() == chave).then(|| v.trim().trim_matches(['"', '\'']).trim().to_owned())
        })
    };
    if let Some(bonito) = valor("PRETTY_NAME").filter(|v| !v.is_empty()) {
        return Some(bonito);
    }
    let nome = valor("NAME").filter(|v| !v.is_empty())?;
    Some(match valor("VERSION_ID").filter(|v| !v.is_empty()) {
        Some(versao) => format!("{nome} {versao}"),
        None => nome,
    })
}

/// O shell de `usuario` no `/etc/passwd` (o último campo da linha dele).
pub fn shell_do_passwd(texto: &str, usuario: &str) -> Option<String> {
    texto.lines().find_map(|linha| {
        let campos: Vec<&str> = linha.trim().split(':').collect();
        (campos.len() >= 7 && campos[0] == usuario && !campos[6].is_empty())
            .then(|| campos[6].to_owned())
    })
}

/// O nome e a versão do `SystemVersion.plist` do macOS ("macOS 14.5").
pub fn nome_do_plist(texto: &str) -> Option<String> {
    let valor = |chave: &str| {
        let marca = format!("<key>{chave}</key>");
        let depois = &texto[texto.find(&marca)? + marca.len()..];
        let inicio = depois.find("<string>")? + "<string>".len();
        let fim = depois[inicio..].find("</string>")?;
        Some(depois[inicio..inicio + fim].trim().to_owned())
    };
    let nome = valor("ProductName").filter(|v| !v.is_empty())?;
    Some(match valor("ProductVersion").filter(|v| !v.is_empty()) {
        Some(versao) => format!("{nome} {versao}"),
        None => nome,
    })
}

/// O que se leu do servidor pelo SFTP.
#[derive(Clone, Debug, Default)]
pub struct ArquivosDoServidor {
    /// O `/etc/os-release`.
    pub os_release: Option<String>,
    /// O `/etc/passwd`.
    pub passwd: Option<String>,
    /// O `/System/Library/CoreServices/SystemVersion.plist`.
    pub plist: Option<String>,
    /// Se `C:/Windows` existe (o OpenSSH do Windows).
    pub windows: bool,
}

/// O sistema do servidor, pelo que o SFTP deu. Sem nada, "Unix" com o shell suposto.
pub fn sistema_do_servidor(
    arquivos: &ArquivosDoServidor,
    usuario: &str,
    shell_suposto: &str,
) -> SistemaRemoto {
    if arquivos.windows {
        return SistemaRemoto {
            so: "Windows".to_owned(),
            shell: "PowerShell ou cmd".to_owned(),
            fonte: FonteSistema::Servidor,
        };
    }
    let shell_lido = arquivos
        .passwd
        .as_deref()
        .and_then(|p| shell_do_passwd(p, usuario));
    let so = arquivos
        .os_release
        .as_deref()
        .and_then(nome_do_os_release)
        .or_else(|| arquivos.plist.as_deref().and_then(nome_do_plist));
    SistemaRemoto {
        fonte: if shell_lido.is_some() || so.is_some() {
            FonteSistema::Servidor
        } else {
            FonteSistema::Suposto
        },
        so: so.unwrap_or_else(|| "Unix".to_owned()),
        shell: shell_lido.unwrap_or_else(|| shell_suposto.trim().to_owned()),
    }
}

/// O sistema deste computador: o `std::env::consts::OS` e o shell (a variável `SHELL`, ou `ComSpec`
/// no Windows). `os` e `shell` entram por parâmetro para os testes.
pub fn sistema_local_de(os: &str, shell: Option<&str>, shell_suposto: &str) -> SistemaRemoto {
    let so = match os {
        "linux" => "Linux",
        "macos" => "macOS",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "netbsd" => "NetBSD",
        outro => outro,
    };
    let shell = shell
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if os == "windows" {
                "PowerShell ou cmd".to_owned()
            } else {
                shell_suposto.trim().to_owned()
            }
        });
    SistemaRemoto {
        so: so.to_owned(),
        shell,
        fonte: FonteSistema::Servidor,
    }
}

/// O sistema deste computador, lendo as variáveis de ambiente.
pub fn sistema_local(shell_suposto: &str) -> SistemaRemoto {
    let variavel = if cfg!(windows) { "ComSpec" } else { "SHELL" };
    let shell = std::env::var(variavel).ok();
    sistema_local_de(std::env::consts::OS, shell.as_deref(), shell_suposto)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn frase_do_sistema_lido_e_suposto() {
        let lido = SistemaRemoto {
            so: "Ubuntu 24.04.1 LTS".into(),
            shell: "/bin/bash".into(),
            fonte: FonteSistema::Servidor,
        };
        assert_eq!(
            lido.frase(),
            "Sistema: Ubuntu 24.04.1 LTS; shell: /bin/bash (lidos do servidor)."
        );
        assert_eq!(
            SistemaRemoto::suposto("zsh").frase(),
            "Sistema desconhecido; suponha zsh."
        );
        assert_eq!(
            SistemaRemoto::suposto("  ").frase(),
            "Sistema desconhecido; suponha bash."
        );
        let meio = SistemaRemoto {
            so: "Unix".into(),
            shell: "sh".into(),
            fonte: FonteSistema::Suposto,
        };
        assert_eq!(meio.frase(), "Sistema: Unix; shell: sh (supostos).");
    }

    #[test]
    fn ultimas_linhas_corta_o_comeco_e_o_fim_vazio() {
        let (texto, cortadas) = ultimas_linhas("a\nb\nc\nd\n\n  \n", 2);
        assert_eq!(texto, "c\nd");
        assert_eq!(cortadas, 2);
        let (tudo, nenhuma) = ultimas_linhas("a\nb", 10);
        assert_eq!((tudo.as_str(), nenhuma), ("a\nb", 0));
        // Zero vale como um: nunca manda texto nenhum por engano.
        assert_eq!(ultimas_linhas("a\nb", 0).0, "b");
    }

    #[test]
    fn limitar_caracteres_corta_as_linhas_mais_antigas() {
        let texto = "aaaa\nbbbb\ncccc";
        assert_eq!(limitar_caracteres(texto, 0), (texto.to_owned(), 0));
        assert_eq!(limitar_caracteres(texto, 100), (texto.to_owned(), 0));
        assert_eq!(limitar_caracteres(texto, 9), ("bbbb\ncccc".to_owned(), 1));
        assert_eq!(limitar_caracteres(texto, 4), ("cccc".to_owned(), 2));
    }

    #[test]
    fn limitar_caracteres_com_linha_maior_que_o_limite_fica_com_o_fim() {
        let (texto, cortadas) = limitar_caracteres("antes\n0123456789", 4);
        assert_eq!(texto, "6789");
        assert_eq!(cortadas, 1);
        // Acentos contam por caractere, não por byte.
        assert_eq!(limitar_caracteres("coração", 3).0, "ção");
    }

    #[test]
    fn eh_prompt_e_a_regra_do_m50() {
        assert!(eh_prompt("root@web:~# "));
        assert!(eh_prompt("[ana@db tmp]$"));
        assert!(eh_prompt("PS C:\\> "));
        assert!(!eh_prompt("root@web:~# ls"));
    }

    #[test]
    fn ultimo_erro_acha_o_comando_e_a_saida() {
        let historico = "root@web:~# ls /nada\nls: cannot access '/nada': No such file or directory\nroot@web:~# ";
        let erro = ultimo_erro(historico, 200);
        assert!(erro.achou);
        assert_eq!(erro.comando.as_deref(), Some("ls /nada"));
        assert_eq!(
            erro.saida,
            "ls: cannot access '/nada': No such file or directory"
        );
        assert_eq!(erro.linhas, 1);
    }

    #[test]
    fn ultimo_erro_pega_so_o_ultimo_comando() {
        let historico = "[ana@db tmp]$ pwd\n/tmp\n[ana@db tmp]$ make\nmake: *** No targets.  Stop.\n[ana@db tmp]$ ";
        let erro = ultimo_erro(historico, 200);
        assert_eq!(erro.comando.as_deref(), Some("make"));
        assert_eq!(erro.saida, "make: *** No targets.  Stop.");
    }

    #[test]
    fn ultimo_erro_ignora_enter_no_prompt_vazio() {
        let historico = "u@h:~$ falha\nerro feio\nu@h:~$ \nu@h:~$ \nu@h:~$ ";
        let erro = ultimo_erro(historico, 200);
        assert_eq!(erro.comando.as_deref(), Some("falha"));
        assert_eq!(erro.saida, "erro feio");
    }

    #[test]
    fn ultimo_erro_sem_prompt_anterior_vale_o_que_esta_acima() {
        let historico = "linha 1\nlinha 2\nroot@web:~# ";
        let erro = ultimo_erro(historico, 200);
        assert!(erro.achou);
        assert_eq!(erro.comando, None);
        assert_eq!(erro.saida, "linha 1\nlinha 2");
        assert_eq!(erro.linhas, 2);
    }

    #[test]
    fn ultimo_erro_sem_prompt_manda_as_linhas_todas() {
        let erro = ultimo_erro("a\nb\nc\nprograma rodando", 3);
        assert!(!erro.achou);
        assert_eq!(erro.comando, None);
        assert_eq!(erro.saida, "b\nc\nprograma rodando");
        assert_eq!(erro.linhas, 3);
    }

    #[test]
    fn ultimo_erro_respeita_o_limite_de_linhas() {
        // O prompt anterior ficou fora das 2 linhas: vale o que está acima do atual.
        let historico = "u@h:~$ cmd\nsaida 1\nsaida 2\nu@h:~$ ";
        let erro = ultimo_erro(historico, 3);
        assert_eq!(erro.comando, None);
        assert_eq!(erro.saida, "saida 1\nsaida 2");
    }

    #[test]
    fn prompt_curto_demais_nao_casa_com_qualquer_linha() {
        let historico = "$ ls\nx\n$ ";
        let erro = ultimo_erro(historico, 200);
        assert_eq!(erro.comando, None);
        assert_eq!(erro.saida, "$ ls\nx");
    }

    #[test]
    fn os_release_pretty_name_e_alternativas() {
        let ubuntu = "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\n";
        assert_eq!(
            nome_do_os_release(ubuntu).as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
        assert_eq!(
            nome_do_os_release("NAME=Alpine\nVERSION_ID=3.20.1\n").as_deref(),
            Some("Alpine 3.20.1")
        );
        assert_eq!(nome_do_os_release("NAME=Arch\n").as_deref(), Some("Arch"));
        assert_eq!(nome_do_os_release("ID=x\n"), None);
    }

    #[test]
    fn shell_do_usuario_no_passwd() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\nana:x:1000:1000:Ana,,,:/home/ana:/usr/bin/zsh\nsem:x:1:1::/:\n";
        assert_eq!(
            shell_do_passwd(passwd, "ana").as_deref(),
            Some("/usr/bin/zsh")
        );
        assert_eq!(
            shell_do_passwd(passwd, "root").as_deref(),
            Some("/bin/bash")
        );
        assert_eq!(shell_do_passwd(passwd, "sem"), None);
        assert_eq!(shell_do_passwd(passwd, "ninguem"), None);
    }

    #[test]
    fn plist_do_macos() {
        let plist = "<dict>\n<key>ProductName</key>\n<string>macOS</string>\n<key>ProductVersion</key>\n<string>14.5</string>\n</dict>";
        assert_eq!(nome_do_plist(plist).as_deref(), Some("macOS 14.5"));
        assert_eq!(nome_do_plist("<dict></dict>"), None);
    }

    #[test]
    fn sistema_do_servidor_combina_o_que_o_sftp_deu() {
        let arquivos = ArquivosDoServidor {
            os_release: Some("PRETTY_NAME=\"Debian GNU/Linux 12\"".into()),
            passwd: Some("ana:x:1000:1000::/home/ana:/bin/zsh".into()),
            ..ArquivosDoServidor::default()
        };
        let sistema = sistema_do_servidor(&arquivos, "ana", "bash");
        assert_eq!(sistema.so, "Debian GNU/Linux 12");
        assert_eq!(sistema.shell, "/bin/zsh");
        assert_eq!(sistema.fonte, FonteSistema::Servidor);

        let windows = ArquivosDoServidor {
            windows: true,
            ..ArquivosDoServidor::default()
        };
        let sistema = sistema_do_servidor(&windows, "ana", "bash");
        assert_eq!(sistema.so, "Windows");
        assert_eq!(sistema.shell, "PowerShell ou cmd");

        let nada = sistema_do_servidor(&ArquivosDoServidor::default(), "ana", "bash");
        assert_eq!(nada.so, "Unix");
        assert_eq!(nada.shell, "bash");
        assert_eq!(nada.fonte, FonteSistema::Suposto);
    }

    #[test]
    fn sistema_local_pelo_ambiente() {
        let linux = sistema_local_de("linux", Some("/bin/zsh"), "bash");
        assert_eq!(
            (linux.so.as_str(), linux.shell.as_str()),
            ("Linux", "/bin/zsh")
        );
        let sem_shell = sistema_local_de("macos", None, "zsh");
        assert_eq!(
            (sem_shell.so.as_str(), sem_shell.shell.as_str()),
            ("macOS", "zsh")
        );
        let windows = sistema_local_de("windows", None, "bash");
        assert_eq!(windows.shell, "PowerShell ou cmd");
        let cmd = sistema_local_de("windows", Some("C:\\Windows\\system32\\cmd.exe"), "bash");
        assert_eq!(cmd.shell, "C:\\Windows\\system32\\cmd.exe");
    }
}
