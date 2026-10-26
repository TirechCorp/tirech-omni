//! O servidor de mentira dos testes do colar (M45): entende os comandos que o plano B e o fluxo
//! `tar` mandam pelo canal `exec` e age sobre um `MemFs`. Sem rede, sem `sh`, sem `tar` do
//! sistema. É o que o `FakeOpener` entrega no [`Opener::exec_da_sessao`](crate::abrir::Opener).

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use flate2::read::GzDecoder;
use tomni_ui::files::{EntryKind, Fs, MemFs};
use tomni_ui::terminal::ssh::ExecOutput;

use crate::abrir::Execucao;

/// A pasta pessoal do `MemFs`, o que o `"$HOME"` vira.
const HOME: &str = "/home/ana";

/// De quanto em quanto a entrada é lida (o do `exec` de verdade).
const BLOCO: usize = 32 * 1024;

/// Quanto o servidor segura a entrada no máximo, para um teste esquecido não travar.
const ESPERA_MAXIMA: Duration = Duration::from_secs(20);

/// Um servidor de arquivos de mentira com um `exec`.
pub struct ServidorFalso {
    /// O id da conexão, para anotar nos [`execs`](Self::execs).
    pub id: u64,
    /// Os arquivos do servidor.
    pub arquivos: MemFs,
    /// Os comandos rodados, com a entrada que chegou a ser lida.
    pub execs: Arc<Mutex<Vec<Execucao>>>,
    /// O servidor não tem `tar`.
    pub sem_tar: Arc<AtomicBool>,
    /// O `tar` do servidor não lê gzip.
    pub tar_sem_gzip: Arc<AtomicBool>,
    /// Com `true`, o `exec` para depois do primeiro bloco da entrada até soltarem.
    pub segurar: Arc<(Mutex<bool>, Condvar)>,
}

/// Como a execução de um comando acaba: o código e o que saiu no `stderr`.
type Saida = Result<(), (u32, String)>;

impl ServidorFalso {
    /// Roda `comando` com `entrada` na entrada padrão. `Err` é a entrada que deu erro (o
    /// cancelamento): o servidor vê o canal cair.
    pub fn rodar(
        &self,
        comando: &str,
        entrada: &mut dyn Read,
        enviado: &mut dyn FnMut(u64),
    ) -> Result<ExecOutput, String> {
        let mut lida = Vec::new();
        let mut saida = Vec::new();
        let resultado = self.executar(comando, entrada, enviado, &mut lida, &mut saida);
        self.execs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((self.id, comando.to_owned(), lida));
        let (status, stderr) = match resultado? {
            Ok(()) => (0, String::new()),
            Err((status, erro)) => (status, erro),
        };
        Ok(ExecOutput {
            status: Some(status),
            stdout: saida,
            stderr: stderr.into_bytes(),
        })
    }

    fn executar(
        &self,
        comando: &str,
        entrada: &mut dyn Read,
        enviado: &mut dyn FnMut(u64),
        lida: &mut Vec<u8>,
        saida: &mut Vec<u8>,
    ) -> Result<Saida, String> {
        for palavras in segmentos(comando) {
            let partes: Vec<&str> = palavras.iter().map(String::as_str).collect();
            let resultado = match partes.as_slice() {
                [] | ["umask", _] => Ok(()),
                ["mkdir", "-p", "--", pastas @ ..] => self.mkdir_p(pastas),
                ["echo", texto] => {
                    saida.extend_from_slice(texto.as_bytes());
                    saida.push(b'\n');
                    Ok(())
                }
                ["cat", ">", arquivo] => {
                    ler_tudo(entrada, enviado, lida, &self.segurar)?;
                    self.gravar(arquivo, lida)
                }
                ["ls", "-1A", "--", pasta] => self.listar(pasta, saida),
                ["rm", "-rf", "--", alvos @ ..] => {
                    self.apagar(alvos);
                    Ok(())
                }
                ["tar", opcoes @ ..] => self.tar(opcoes, entrada, enviado, lida)?,
                outro => Err((127, format!("{}: comando não encontrado", outro[0]))),
            };
            if resultado.is_err() {
                return Ok(resultado);
            }
        }
        Ok(Ok(()))
    }

    fn fs(&self) -> MemFs {
        self.arquivos.connect()
    }

    fn mkdir_p(&self, pastas: &[&str]) -> Saida {
        let mut fs = self.fs();
        for pasta in pastas {
            criar_pastas(&mut fs, pasta).map_err(|e| (1, format!("mkdir: {e}")))?;
        }
        Ok(())
    }

    fn gravar(&self, arquivo: &str, dados: &[u8]) -> Saida {
        let mut fs = self.fs();
        escrever(&mut fs, arquivo, dados, None).map_err(|e| (1, format!("cat: {e}")))
    }

    fn listar(&self, pasta: &str, saida: &mut Vec<u8>) -> Saida {
        let mut nomes: Vec<String> = self
            .fs()
            .list(pasta)
            .map_err(|e| (2, format!("ls: {e}")))?
            .into_iter()
            .map(|e| e.name)
            .collect();
        nomes.sort();
        for nome in nomes {
            saida.extend_from_slice(nome.as_bytes());
            saida.push(b'\n');
        }
        Ok(())
    }

    fn apagar(&self, alvos: &[&str]) {
        let mut fs = self.fs();
        for alvo in alvos {
            apagar_tudo(&mut fs, alvo);
        }
    }

    /// `tar -tzf -`, `tar -tf -` (a sondagem) e `tar -x [-z] -o -f - -C <destino>`.
    fn tar(
        &self,
        opcoes: &[&str],
        entrada: &mut dyn Read,
        enviado: &mut dyn FnMut(u64),
        lida: &mut Vec<u8>,
    ) -> Result<Saida, String> {
        if self.sem_tar.load(Ordering::SeqCst) {
            return Ok(Err((127, "tar: comando não encontrado".to_owned())));
        }
        let gzip = opcoes.contains(&"-z") || opcoes.contains(&"-tzf");
        ler_tudo(entrada, enviado, lida, &self.segurar)?;
        if gzip && self.tar_sem_gzip.load(Ordering::SeqCst) {
            return Ok(Err((2, "tar: opção -z não suportada".to_owned())));
        }
        let leitor: Box<dyn Read + '_> = if gzip {
            Box::new(GzDecoder::new(&lida[..]))
        } else {
            Box::new(&lida[..])
        };
        let mut pacote = tar::Archive::new(leitor);
        if opcoes.iter().any(|o| o.starts_with("-t")) {
            return Ok(percorrer(&mut pacote).map_err(|e| (2, format!("tar: {e}"))));
        }
        let Some(destino) = opcoes
            .iter()
            .position(|o| *o == "-C")
            .and_then(|i| opcoes.get(i + 1))
        else {
            return Ok(Err((2, "tar: falta o -C".to_owned())));
        };
        Ok(self.extrair(&mut pacote, destino))
    }

    fn extrair<R: Read>(&self, pacote: &mut tar::Archive<R>, destino: &str) -> Saida {
        let falha = |e: &dyn std::fmt::Display| (2, format!("tar: {e}"));
        let mut fs = self.fs();
        let entradas = pacote.entries().map_err(|e| falha(&e))?;
        for entrada in entradas {
            let mut entrada = entrada.map_err(|e| falha(&e))?;
            let nome = entrada.path().map_err(|e| falha(&e))?;
            let nome = nome.to_string_lossy().trim_end_matches('/').to_owned();
            if nome.is_empty() || nome.split('/').any(|p| p == "..") {
                return Err((2, format!("tar: nome recusado: {nome}")));
            }
            let caminho = format!("{}/{nome}", destino.trim_end_matches('/'));
            let modo = entrada.header().mode().unwrap_or(0o644);
            match entrada.header().entry_type() {
                tar::EntryType::Directory => {
                    criar_pastas(&mut fs, &caminho).map_err(|e| falha(&e))?;
                }
                tar::EntryType::Regular | tar::EntryType::GNUSparse => {
                    let mut dados = Vec::new();
                    entrada.read_to_end(&mut dados).map_err(|e| falha(&e))?;
                    escrever(&mut fs, &caminho, &dados, Some(modo)).map_err(|e| falha(&e))?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Percorre o pacote inteiro, sem guardar nada: o `tar -t`.
fn percorrer<R: Read>(pacote: &mut tar::Archive<R>) -> std::io::Result<()> {
    for entrada in pacote.entries()? {
        entrada?;
    }
    Ok(())
}

/// Lê a entrada até o fim, de bloco em bloco, e para depois do primeiro se o servidor está
/// seguro. O erro da entrada (o cancelamento) volta como `Err`.
fn ler_tudo(
    entrada: &mut dyn Read,
    enviado: &mut dyn FnMut(u64),
    lida: &mut Vec<u8>,
    segurar: &(Mutex<bool>, Condvar),
) -> Result<(), String> {
    let mut bloco = vec![0u8; BLOCO];
    let mut primeiro = true;
    loop {
        let n = entrada.read(&mut bloco).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        lida.extend_from_slice(&bloco[..n]);
        enviado(lida.len() as u64);
        if std::mem::take(&mut primeiro) {
            let (trava, aviso) = segurar;
            let mut ligado = trava.lock().unwrap_or_else(PoisonError::into_inner);
            while *ligado {
                let (novo, tempo) = aviso
                    .wait_timeout(ligado, ESPERA_MAXIMA)
                    .unwrap_or_else(PoisonError::into_inner);
                ligado = novo;
                if tempo.timed_out() {
                    break;
                }
            }
        }
    }
}

/// `mkdir -p`: cada nível que falta.
fn criar_pastas(fs: &mut MemFs, caminho: &str) -> std::io::Result<()> {
    let mut atual = String::new();
    for parte in caminho.split('/').filter(|p| !p.is_empty()) {
        atual.push('/');
        atual.push_str(parte);
        if fs.stat(&atual).is_ok_and(|e| e.kind == EntryKind::Dir) {
            continue;
        }
        fs.mkdir(&atual)?;
    }
    Ok(())
}

/// Grava `dados` em `caminho`, criando a pasta dele antes.
fn escrever(fs: &mut MemFs, caminho: &str, dados: &[u8], modo: Option<u32>) -> std::io::Result<()> {
    if let Some((pai, _)) = caminho.rsplit_once('/') {
        criar_pastas(fs, pai)?;
    }
    let mut arquivo = fs.open_write(caminho, 0)?;
    arquivo.write_all(dados)?;
    arquivo.finish()?;
    if let Some(modo) = modo {
        let _ = fs.set_permissions(caminho, modo & 0o7777);
    }
    Ok(())
}

/// `rm -rf`: o que houver, de dentro para fora; o que faltar não é erro.
fn apagar_tudo(fs: &mut MemFs, caminho: &str) {
    let Ok(entrada) = fs.lstat(caminho) else {
        return;
    };
    if entrada.kind == EntryKind::Dir {
        for filho in fs.list(caminho).unwrap_or_default() {
            apagar_tudo(
                fs,
                &format!("{}/{}", caminho.trim_end_matches('/'), filho.name),
            );
        }
        let _ = fs.remove_dir(caminho);
    } else {
        let _ = fs.remove_file(caminho);
    }
}

/// O comando em palavras, como o `sh` as leria (aspas simples, aspas duplas com o `$HOME`, a
/// barra invertida), separado em um grupo por `&&`.
fn segmentos(comando: &str) -> Vec<Vec<String>> {
    let mut grupos = Vec::new();
    let mut grupo: Vec<String> = Vec::new();
    let mut palavra = String::new();
    let mut aberta = false;
    let mut letras = comando.chars().peekable();
    while let Some(letra) = letras.next() {
        match letra {
            '\'' => {
                aberta = true;
                for dentro in letras.by_ref() {
                    if dentro == '\'' {
                        break;
                    }
                    palavra.push(dentro);
                }
            }
            '"' => {
                aberta = true;
                while let Some(dentro) = letras.next() {
                    match dentro {
                        '"' => break,
                        '\\' => palavra.extend(letras.next()),
                        '$' => {
                            let mut olhar = letras.clone();
                            if olhar.by_ref().take(4).collect::<String>() == "HOME" {
                                palavra.push_str(HOME);
                                letras.by_ref().take(4).for_each(drop);
                            } else {
                                palavra.push('$');
                            }
                        }
                        outra => palavra.push(outra),
                    }
                }
            }
            '\\' => {
                aberta = true;
                palavra.extend(letras.next());
            }
            '&' if !aberta && palavra.is_empty() && letras.peek() == Some(&'&') => {
                letras.next();
                grupos.push(std::mem::take(&mut grupo));
            }
            espaco if espaco.is_whitespace() => {
                if aberta || !palavra.is_empty() {
                    grupo.push(std::mem::take(&mut palavra));
                    aberta = false;
                }
            }
            outra => {
                aberta = true;
                palavra.push(outra);
            }
        }
    }
    if aberta || !palavra.is_empty() {
        grupo.push(palavra);
    }
    grupos.push(grupo);
    grupos
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_as_palavras_como_o_sh() {
        let g =
            segmentos(r#"umask 077 && mkdir -p -- "$HOME"'/.cache/a b' && cat > 'it'\''s.txt'"#);
        assert_eq!(
            g,
            vec![
                vec!["umask", "077"],
                vec!["mkdir", "-p", "--", "/home/ana/.cache/a b"],
                vec!["cat", ">", "it's.txt"],
            ]
        );
    }

    #[test]
    fn echo_escreve_o_texto_na_saida() {
        let servidor = ServidorFalso {
            id: 1,
            arquivos: MemFs::new(),
            execs: Arc::default(),
            sem_tar: Arc::default(),
            tar_sem_gzip: Arc::default(),
            segurar: Arc::default(),
        };
        let saida = servidor
            .rodar("echo 'olá mundo'", &mut std::io::empty(), &mut |_| {})
            .unwrap();
        assert_eq!(saida.status, Some(0));
        assert_eq!(saida.stdout, "olá mundo\n".as_bytes());
    }
}
