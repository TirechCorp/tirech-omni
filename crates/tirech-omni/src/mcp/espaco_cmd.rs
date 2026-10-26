//! Os comandos que o espaço de trabalho manda o servidor rodar (M51, seção 8): a busca por
//! `grep` ou `git grep`, a lista de arquivos por `find` ou `git ls-files` e o `bash`. Aqui só se
//! monta o texto do comando e se lê a saída dele; quem roda é [`super::espaco_fs`].
//!
//! Os scripts de busca e de lista vão dentro de `sh -c '…'`, porque o `exec` do SSH usa o shell
//! de login da pessoa, que pode ser o `fish` ou o `csh`. O `bash` não: ele roda o que o
//! assistente escreveu no shell da pessoa, como o terminal faria.

use tomni_ui::terminal::{ShellQuote, shell_quote};

use super::espaco::raiz_windows;
use super::glob;
use crate::t;

/// O que a busca devolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaidaDaBusca {
    /// `caminho:linha:texto`.
    Linhas,
    /// Só os caminhos que têm um achado.
    Arquivos,
    /// `caminho:quantidade`.
    Contagem,
}

impl SaidaDaBusca {
    /// Lê o argumento `saida` (qualquer outro valor é `linhas`).
    pub fn do_argumento(valor: Option<&str>) -> Self {
        match valor {
            Some("arquivos") => Self::Arquivos,
            Some("contagem") => Self::Contagem,
            _ => Self::Linhas,
        }
    }
}

/// Um pedido de `buscar`, já com o caminho conferido (relativo à raiz, `.` para a raiz).
#[derive(Clone, Debug)]
pub struct PedidoDeBusca {
    /// O que procurar.
    pub padrao: String,
    /// Se `padrao` é uma expressão regular (senão, texto puro).
    pub regex: bool,
    /// A pasta ou o arquivo onde procurar, relativo à raiz.
    pub caminho: String,
    /// Só os arquivos que casam.
    pub glob: Option<String>,
    /// Se maiúsculas e minúsculas contam.
    pub maiusculas: bool,
    /// Linhas antes e depois de cada achado.
    pub contexto: usize,
    /// O que devolver.
    pub saida: SaidaDaBusca,
}

fn posix(texto: &str) -> String {
    shell_quote(texto, ShellQuote::Posix)
}

/// O comando inteiro (`sh -c '…'`) da busca pelo servidor. A primeira linha da saída é
/// `[busca: git grep]` ou `[busca: grep]`.
pub fn comando_de_busca(raiz: &str, p: &PedidoDeBusca) -> String {
    let mut opcoes = vec![if p.regex { "-E" } else { "-F" }.to_owned()];
    if !p.maiusculas {
        opcoes.push("-i".to_owned());
    }
    match p.saida {
        SaidaDaBusca::Arquivos => opcoes.push("-l".to_owned()),
        SaidaDaBusca::Contagem => opcoes.push("-c".to_owned()),
        SaidaDaBusca::Linhas if p.contexto > 0 => opcoes.push(format!("-C {}", p.contexto)),
        SaidaDaBusca::Linhas => {}
    }
    let opcoes = opcoes.join(" ");
    let padrao = posix(&p.padrao);
    // O `git grep` não tem `{a,b}` no pathspec: cada alternativa vira um pathspec.
    let git_path = match &p.glob {
        Some(g) => {
            let prefixo = if p.caminho == "." {
                String::new()
            } else {
                format!("{}/", p.caminho)
            };
            glob::chaves(g)
                .iter()
                .map(|g| posix(&format!(":(glob){prefixo}**/{g}")))
                .collect::<Vec<_>>()
                .join(" ")
        }
        None if p.caminho == "." => ".".to_owned(),
        None => posix(&format!(":(literal){}", p.caminho)),
    };
    let inclui: String = p
        .glob
        .iter()
        .flat_map(|g| glob::chaves(g))
        .map(|g| format!(" {}", posix(&format!("--include={g}"))))
        .collect();
    let script = format!(
        "cd {raiz} || exit 2\n\
         if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then\n\
         echo '[busca: git grep]'\n\
         git grep -n -I --untracked {opcoes} -e {padrao} -- {git_path}\n\
         else\n\
         echo '[busca: grep]'\n\
         grep -rnI --exclude-dir=.git{inclui} {opcoes} -e {padrao} -- {caminho}\n\
         fi",
        raiz = posix(raiz),
        caminho = posix(&p.caminho),
    );
    format!("sh -c {}", posix(&script))
}

/// O comando (`sh -c '…'`) que lista os arquivos de `caminho` (relativo à raiz). A primeira
/// linha da saída diz de onde vem: `#git`, `#find-printf` (cada linha é `<mtime> <caminho>`)
/// ou `#find`.
pub fn comando_de_lista(raiz: &str, caminho: &str) -> String {
    let c = posix(caminho);
    let script = format!(
        "cd {raiz} || exit 2\n\
         if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; then\n\
         echo '#git'\n\
         git -c core.quotepath=off ls-files -co --exclude-standard -- {c}\n\
         elif find . -maxdepth 0 -printf '' >/dev/null 2>&1; then\n\
         echo '#find-printf'\n\
         find {c} -name .git -prune -o \\( -type f -o -type l \\) -printf '%T@ %p\\n'\n\
         else\n\
         echo '#find'\n\
         find {c} -name .git -prune -o \\( -type f -o -type l \\) -print\n\
         fi",
        raiz = posix(raiz),
    );
    format!("sh -c {}", posix(&script))
}

/// Os arquivos que `comando_de_lista` achou: o caminho relativo à raiz e a data, quando veio.
#[derive(Debug, PartialEq)]
pub struct ArquivoListado {
    /// O caminho, relativo à raiz.
    pub caminho: String,
    /// A data de modificação, em segundos desde 1970, se o comando disse.
    pub mtime: Option<f64>,
}

/// Lê a saída de `comando_de_lista`.
pub fn ler_lista(saida: &str) -> Vec<ArquivoListado> {
    let mut linhas = saida.lines();
    let origem = linhas.next().unwrap_or_default();
    linhas
        .filter(|l| !l.is_empty())
        .map(|linha| {
            let (mtime, caminho) = match linha.split_once(' ') {
                Some((m, resto)) if origem == "#find-printf" => (m.parse::<f64>().ok(), resto),
                _ => (None, linha),
            };
            ArquivoListado {
                caminho: tirar_ponto_barra(caminho).to_owned(),
                mtime,
            }
        })
        .collect()
}

/// Tira o `./` do começo.
pub fn tirar_ponto_barra(caminho: &str) -> &str {
    caminho.strip_prefix("./").unwrap_or(caminho)
}

/// Tira o `./` do começo de cada linha de uma saída de `grep -r`.
pub fn sem_ponto_barra(saida: &str) -> String {
    saida
        .lines()
        .map(tirar_ponto_barra)
        .collect::<Vec<_>>()
        .join("\n")
}

/// O comando do `bash` do espaço: entra na raiz e roda o que o assistente escreveu. Num servidor
/// Windows usa o `cd /d` do `cmd`, que é o shell do OpenSSH de lá se ninguém mudou.
pub fn comando_do_bash(raiz: &str, comando: &str) -> String {
    if raiz_windows(raiz) {
        let unidade = raiz.strip_prefix('/').unwrap_or(raiz).replace('/', "\\");
        format!(
            "cd /d {} && {comando}",
            shell_quote(&unidade, ShellQuote::Cmd)
        )
    } else {
        format!("cd {} && {comando}", posix(raiz))
    }
}

/// O que a busca pelo servidor deu.
#[derive(Debug, PartialEq, Eq)]
pub enum RespostaDaBusca {
    /// O texto para o assistente (com a primeira linha dizendo qual comando buscou).
    Pronta(String),
    /// O comando rodou e reclamou (uma expressão regular inválida, por exemplo).
    Erro(String),
    /// O servidor não buscou (sem `sh`, `exec` recusado, a raiz sumiu): vale a reserva pelo SFTP.
    Reserva,
}

/// Lê a saída de `comando_de_busca`.
pub fn interpretar_busca(
    status: Option<u32>,
    stdout: &str,
    stderr: &str,
    saida: SaidaDaBusca,
) -> RespostaDaBusca {
    let mut linhas = stdout.lines();
    let marcador = match linhas.next() {
        Some(m @ ("[busca: git grep]" | "[busca: grep]")) => m,
        _ => return RespostaDaBusca::Reserva,
    };
    let corpo: Vec<&str> = linhas
        // O `grep -c` conta também os arquivos sem achado.
        .filter(|l| !(saida == SaidaDaBusca::Contagem && l.ends_with(":0")))
        .collect();
    if corpo.is_empty() {
        return match status {
            Some(0 | 1) => {
                RespostaDaBusca::Pronta(format!("{marcador}\n{}", t!("mcp-espaco-busca-nada")))
            }
            _ => {
                let erro = stderr.trim();
                RespostaDaBusca::Erro(t!(
                    "mcp-espaco-busca-erro",
                    erro = if erro.is_empty() {
                        format!("{status:?}")
                    } else {
                        erro.to_owned()
                    }
                ))
            }
        };
    }
    RespostaDaBusca::Pronta(format!(
        "{marcador}\n{}",
        sem_ponto_barra(&corpo.join("\n"))
    ))
}

/// Lê a saída de `comando_de_lista`; `None` se o servidor não listou (vale a reserva).
pub fn interpretar_lista(status: Option<u32>, stdout: &str) -> Option<Vec<ArquivoListado>> {
    let origem = stdout.lines().next()?;
    // O `find` sai com 1 quando não consegue entrar numa pasta, mas lista o resto.
    (matches!(origem, "#git" | "#find-printf" | "#find") && matches!(status, Some(0 | 1)))
        .then(|| ler_lista(stdout))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pedido(padrao: &str) -> PedidoDeBusca {
        PedidoDeBusca {
            padrao: padrao.to_owned(),
            regex: false,
            caminho: ".".to_owned(),
            glob: None,
            maiusculas: true,
            contexto: 0,
            saida: SaidaDaBusca::Linhas,
        }
    }

    #[test]
    fn busca_de_texto_usa_f_e_cita_o_padrao() {
        let comando = comando_de_busca("/srv/meu app", &pedido("it's"));
        assert!(comando.starts_with("sh -c '"), "{comando}");
        assert!(comando.contains("-F"));
        assert!(!comando.contains(" -i "));
        // O padrão cita o apóstrofo duas vezes: uma para o padrão, outra para o script.
        assert!(comando.contains("srv/meu app"));
    }

    #[test]
    fn busca_com_opcoes() {
        let mut p = pedido("a.b");
        p.regex = true;
        p.maiusculas = false;
        p.contexto = 2;
        p.glob = Some("*.{rs,toml}".to_owned());
        p.caminho = "src".to_owned();
        let c = comando_de_busca("/r", &p);
        assert!(c.contains("-E -i -C 2"), "{c}");
        assert!(
            c.contains("--include=*.rs") && c.contains("--include=*.toml"),
            "{c}"
        );
        assert!(
            c.contains(":(glob)src/**/*.rs") && c.contains(":(glob)src/**/*.toml"),
            "{c}"
        );
        p.saida = SaidaDaBusca::Arquivos;
        assert!(comando_de_busca("/r", &p).contains("-E -i -l"));
        p.saida = SaidaDaBusca::Contagem;
        let c = comando_de_busca("/r", &p);
        assert!(c.contains("-c") && !c.contains("-C"), "{c}");
    }

    #[test]
    fn lista_le_o_mtime_do_find() {
        let l = ler_lista("#find-printf\n1700000000.5 ./a b.rs\n1700000001.0 ./src/c.rs\n");
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].caminho, "a b.rs");
        assert_eq!(l[0].mtime, Some(1_700_000_000.5));
        let l = ler_lista("#git\nsrc/a.rs\n2024 x.txt\n");
        assert_eq!(
            l[0],
            ArquivoListado {
                caminho: "src/a.rs".into(),
                mtime: None
            }
        );
        assert_eq!(l[1].caminho, "2024 x.txt");
    }

    #[test]
    fn bash_entra_na_raiz() {
        assert_eq!(comando_do_bash("/srv/app", "ls"), "cd /srv/app && ls");
        assert_eq!(
            comando_do_bash("/srv/meu app", "ls"),
            "cd '/srv/meu app' && ls"
        );
        assert_eq!(
            comando_do_bash("/C:/Users/ana", "dir"),
            "cd /d C:\\Users\\ana && dir"
        );
    }

    #[test]
    fn tira_ponto_barra() {
        assert_eq!(sem_ponto_barra("./a:1:x\n./b:2:y"), "a:1:x\nb:2:y");
        assert_eq!(sem_ponto_barra("--\nsrc/a:1:x"), "--\nsrc/a:1:x");
    }

    #[test]
    fn interpreta_a_busca() {
        let ok = interpretar_busca(
            Some(0),
            "[busca: grep]\n./a.rs:3:fn x()\n./b.rs:1:fn y()\n",
            "",
            SaidaDaBusca::Linhas,
        );
        assert_eq!(
            ok,
            RespostaDaBusca::Pronta("[busca: grep]\na.rs:3:fn x()\nb.rs:1:fn y()".into())
        );
        let nada = interpretar_busca(Some(1), "[busca: git grep]\n", "", SaidaDaBusca::Linhas);
        let RespostaDaBusca::Pronta(texto) = nada else {
            panic!()
        };
        assert!(texto.starts_with("[busca: git grep]\n"), "{texto}");
        let erro = interpretar_busca(
            Some(2),
            "[busca: grep]\n",
            "grep: Unmatched ( or \\(",
            SaidaDaBusca::Linhas,
        );
        let RespostaDaBusca::Erro(texto) = erro else {
            panic!()
        };
        assert!(texto.contains("Unmatched"), "{texto}");
        assert_eq!(
            interpretar_busca(Some(127), "", "sh: not found", SaidaDaBusca::Linhas),
            RespostaDaBusca::Reserva
        );
        let c = interpretar_busca(
            Some(0),
            "[busca: grep]\n./a:2\n./b:0\n",
            "",
            SaidaDaBusca::Contagem,
        );
        assert_eq!(c, RespostaDaBusca::Pronta("[busca: grep]\na:2".into()));
    }

    #[test]
    fn interpreta_a_lista() {
        assert!(interpretar_lista(Some(127), "").is_none());
        assert!(interpretar_lista(Some(0), "erro qualquer\n").is_none());
        assert_eq!(interpretar_lista(Some(1), "#find\n./a\n").unwrap().len(), 1);
    }
}
