//! Baixar o arquivo marcado no terminal: a parte pura. O texto marcado vira um caminho do
//! servidor, que é resolvido pela pasta do shell ou pela home e copiado para o disco. O que fala
//! com a tela, a sonda da pasta e a thread mora em `tela/baixar.rs`.

use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tomni_ui::files::{EntryKind, Fs};

/// De quanto em quanto a cópia conta o que já andou.
pub const RITMO: Duration = Duration::from_millis(100);

/// Por que o download não terminou.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Falha {
    /// A pessoa cancelou.
    Cancelada,
    /// O caminho é de uma pasta.
    Pasta(String),
    /// Deu errado, com o motivo como veio.
    Erro(String),
}

impl From<io::Error> for Falha {
    fn from(erro: io::Error) -> Self {
        Self::Erro(erro.to_string())
    }
}

/// O caminho que o texto marcado `marcado` indica: sem os espaços das pontas, sem as aspas que
/// o cercam e com o `\ ` do `ls` virando espaço. `None` se não sobrar nada, se tiver mais de uma
/// linha ou se não tiver nome de arquivo (`/`, `~`, `.`, `..`).
pub fn caminho_marcado(marcado: &str) -> Option<String> {
    let texto = marcado.trim();
    if texto.is_empty() || texto.contains(['\n', '\r', '\0']) {
        return None;
    }
    let sem_aspas = ['"', '\'']
        .iter()
        .find_map(|&aspa| {
            texto
                .strip_prefix(aspa)
                .and_then(|resto| resto.strip_suffix(aspa))
        })
        .unwrap_or(texto);
    let caminho = sem_aspas.replace("\\ ", " ");
    nome_do_arquivo(&caminho)?;
    Some(caminho)
}

/// O último pedaço de `caminho`, o nome que o arquivo ganha no disco. `None` sem nome.
pub fn nome_do_arquivo(caminho: &str) -> Option<String> {
    let nome = caminho.trim_end_matches('/').rsplit('/').next()?;
    (!nome.is_empty() && !matches!(nome, "~" | "." | "..")).then(|| nome.to_owned())
}

/// Se `caminho` depende da pasta em que o shell está.
pub fn relativo(caminho: &str) -> bool {
    !caminho.starts_with('/') && !caminho.starts_with("~/")
}

/// O caminho absoluto no servidor: o que começa com `/` fica; `~/` vai para a home; o resto,
/// para a `pasta` do shell ou, sem ela, para a home. A home só é pedida quando precisa.
pub fn resolver(
    caminho: &str,
    pasta: Option<&str>,
    home: impl FnOnce() -> io::Result<String>,
) -> io::Result<String> {
    if caminho.starts_with('/') {
        return Ok(caminho.to_owned());
    }
    let (base, resto) = match (caminho.strip_prefix("~/"), pasta) {
        (Some(resto), _) => (home()?, resto),
        (None, Some(pasta)) => (pasta.to_owned(), caminho),
        (None, None) => (home()?, caminho),
    };
    Ok(format!("{}/{resto}", base.trim_end_matches('/')))
}

/// Copia o arquivo `caminho` do servidor `fs` para `destino` no `disco`, contando os bytes em
/// `andou` (`feito`, `total`) e parando quando `cancelar` liga. Uma cópia que não termina apaga
/// o arquivo que começou a gravar.
pub fn copiar(
    fs: &mut dyn Fs,
    caminho: &str,
    disco: &mut dyn Fs,
    destino: &str,
    cancelar: &AtomicBool,
    andou: &dyn Fn(u64, u64),
) -> Result<u64, Falha> {
    let info = fs.stat(caminho)?;
    if info.kind == EntryKind::Dir {
        return Err(Falha::Pasta(caminho.to_owned()));
    }
    andou(0, info.size);
    let mut lendo = fs.open_read(caminho, 0)?;
    let mut saida = disco.open_write(destino, 0)?;
    let mut gravar = || -> Result<u64, Falha> {
        let mut pedaco = vec![0u8; 64 * 1024];
        let (mut feito, mut contado) = (0u64, Instant::now());
        loop {
            if cancelar.load(Ordering::Relaxed) {
                return Err(Falha::Cancelada);
            }
            let n = lendo.read(&mut pedaco)?;
            if n == 0 {
                break;
            }
            saida.write_all(&pedaco[..n])?;
            feito += n as u64;
            if contado.elapsed() >= RITMO {
                andou(feito, info.size.max(feito));
                contado = Instant::now();
            }
        }
        andou(feito, info.size.max(feito));
        Ok(feito)
    };
    let resultado = gravar().and_then(|feito| {
        saida.finish()?;
        lendo.finish()?;
        Ok(feito)
    });
    if resultado.is_err() {
        let _ = disco.remove_file(destino);
    }
    resultado
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_texto_marcado_vira_caminho() {
        assert_eq!(caminho_marcado("log.txt\n").as_deref(), Some("log.txt"));
        assert_eq!(caminho_marcado("  log.txt  ").as_deref(), Some("log.txt"));
        assert_eq!(
            caminho_marcado("'/var/log/a b.log'").as_deref(),
            Some("/var/log/a b.log")
        );
        assert_eq!(caminho_marcado("\"x.tar.gz\"").as_deref(), Some("x.tar.gz"));
        assert_eq!(
            caminho_marcado("meu\\ arquivo.txt").as_deref(),
            Some("meu arquivo.txt")
        );
        assert_eq!(caminho_marcado("a\nb"), None);
        assert_eq!(caminho_marcado("   "), None);
        assert_eq!(caminho_marcado("/"), None);
        assert_eq!(caminho_marcado("~"), None);
        assert_eq!(caminho_marcado(".."), None);
    }

    #[test]
    fn o_nome_e_o_ultimo_pedaco() {
        assert_eq!(nome_do_arquivo("/etc/hosts").as_deref(), Some("hosts"));
        assert_eq!(nome_do_arquivo("pasta/").as_deref(), Some("pasta"));
        assert_eq!(nome_do_arquivo("~/a.txt").as_deref(), Some("a.txt"));
    }

    #[test]
    fn resolve_pela_pasta_do_shell_ou_pela_home() {
        let home = || Ok("/home/ana".to_owned());
        let sem_home = || -> io::Result<String> { panic!("a home não devia ser pedida") };
        assert_eq!(
            resolver("/etc/hosts", Some("/tmp"), sem_home).unwrap(),
            "/etc/hosts"
        );
        assert_eq!(
            resolver("log.txt", Some("/var/log/"), sem_home).unwrap(),
            "/var/log/log.txt"
        );
        assert_eq!(
            resolver("log.txt", Some("/"), sem_home).unwrap(),
            "/log.txt"
        );
        assert_eq!(
            resolver("log.txt", None, home).unwrap(),
            "/home/ana/log.txt"
        );
        assert_eq!(
            resolver("~/a/b.txt", Some("/tmp"), home).unwrap(),
            "/home/ana/a/b.txt"
        );
        assert!(relativo("a.txt"));
        assert!(!relativo("~/a.txt"));
        assert!(!relativo("/a.txt"));
    }

    #[test]
    fn copia_conta_e_apaga_o_que_nao_terminou() {
        use tomni_ui::files::MemFs;
        let servidor = MemFs::new();
        servidor.put("/srv/a.txt", b"conteudo").dir("/srv/pasta");
        let disco = MemFs::new();
        disco.dir("/baixados");
        let (mut fs, mut local) = (servidor.connect(), disco.connect());
        let parar = AtomicBool::new(false);
        let contas = std::cell::RefCell::new(Vec::new());
        let andou = |feito, total| contas.borrow_mut().push((feito, total));
        let feito = copiar(
            &mut fs,
            "/srv/a.txt",
            &mut local,
            "/baixados/a.txt",
            &parar,
            &andou,
        );
        assert_eq!(feito, Ok(8));
        assert_eq!(
            disco.contents("/baixados/a.txt"),
            Some(b"conteudo".to_vec())
        );
        assert_eq!(contas.borrow().last(), Some(&(8, 8)));

        let pasta = copiar(
            &mut fs,
            "/srv/pasta",
            &mut local,
            "/baixados/p",
            &parar,
            &andou,
        );
        assert_eq!(pasta, Err(Falha::Pasta("/srv/pasta".into())));
        assert!(!disco.exists("/baixados/p"));
        assert!(matches!(
            copiar(
                &mut fs,
                "/srv/nao.txt",
                &mut local,
                "/baixados/n",
                &parar,
                &andou
            ),
            Err(Falha::Erro(_))
        ));

        parar.store(true, Ordering::Relaxed);
        let cancelada = copiar(
            &mut fs,
            "/srv/a.txt",
            &mut local,
            "/baixados/c.txt",
            &parar,
            &andou,
        );
        assert_eq!(cancelada, Err(Falha::Cancelada));
        assert!(!disco.exists("/baixados/c.txt"));
    }
}
