//! Os logs de sessão (M53) sem tela: a chave dos logs cifrados no cofre, os logs que estão sendo
//! gravados e, nas fatias seguintes, a lista da página Logs e a conversão entre texto e cifrado.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::abrir::SUFIXO_CIFRADO;
use crate::cofre::Cofre;
use crate::log_cifrado::{self, AvisoLog, ErroLog, EscritorCifrado, Leitura, PublicaDosLogs};
use crate::modelo::Connection;

/// Garante que a chave dos logs existe no cofre e que o `logs-cifrados.pub` a cita, e devolve a
/// pública. O cofre precisa estar aberto.
///
/// - Se o `.pub` existe e o `uid` dele ainda está no cofre, nada muda.
/// - Senão, se o cofre já tem chaves dos logs (de outra máquina, pelo cofre portátil ou pelo
///   juntar), usa a mais nova e regrava o `.pub`.
/// - Senão cria a chave e grava o `.pub`.
pub fn garantir_chave(cofre: &mut Cofre) -> io::Result<PublicaDosLogs> {
    let pasta = cofre.pasta_de_dados().to_owned();
    if let Some(publica) = publica_valida(cofre) {
        return Ok(publica);
    }
    let chaves = cofre.chaves_dos_logs();
    let chave = match chaves.into_iter().max_by_key(|c| c.criado) {
        Some(chave) => chave,
        None => cofre.criar_chave_dos_logs()?,
    };
    log_cifrado::gravar_publica(&pasta, &chave.publica, &chave.uid)?;
    Ok(PublicaDosLogs {
        publica: chave.publica,
        uid: chave.uid,
    })
}

/// A pública do `logs-cifrados.pub`, se o `uid` dela ainda está no cofre com a mesma pública.
/// Com o cofre trancado não há chaves para conferir, então volta `None`.
fn publica_valida(cofre: &Cofre) -> Option<PublicaDosLogs> {
    let publica = log_cifrado::ler_publica(cofre.pasta_de_dados())?;
    cofre
        .chaves_dos_logs()
        .iter()
        .any(|c| c.uid == publica.uid && c.publica == publica.publica)
        .then_some(publica)
}

/// Se o cofre (aberto) e o `.pub` já combinam: [`garantir_chave`] não teria o que fazer.
pub fn chave_pronta(cofre: &Cofre) -> bool {
    publica_valida(cofre).is_some()
}

/// As privadas dos logs que o cofre tem, pelo id: o que [`log_cifrado::ler`] pede.
pub fn mapa_de_chaves(cofre: &Cofre) -> HashMap<[u8; 8], [u8; 32]> {
    cofre
        .chaves_dos_logs()
        .into_iter()
        .map(|c| (c.id, c.privada))
        .collect()
}

thread_local! {
    /// Os logs que as sessões da tela estão gravando: da chave da sessão para o arquivo. A tela e o
    /// `configura` rodam na thread da interface, e cada teste tem a sua.
    static ATIVOS: RefCell<HashMap<String, PathBuf>> = RefCell::default();
}

/// Anota que a sessão `chave` grava em `caminho`.
pub fn registrar_ativo(chave: &str, caminho: &Path) {
    ATIVOS.with(|a| a.borrow_mut().insert(chave.to_owned(), caminho.to_owned()));
}

/// Esquece o log da sessão `chave` (parou, ou a sessão foi descartada).
pub fn esquecer_ativo(chave: &str) {
    ATIVOS.with(|a| a.borrow_mut().remove(chave));
}

/// O arquivo que a sessão `chave` grava, se grava.
pub fn ativo_da_sessao(chave: &str) -> Option<PathBuf> {
    ATIVOS.with(|a| a.borrow().get(chave).cloned())
}

/// Se alguma sessão grava em `caminho`.
pub fn em_uso(caminho: &Path) -> bool {
    ATIVOS.with(|a| a.borrow().values().any(|c| c == caminho))
}

/// Todos os logs que estão sendo gravados.
pub fn ativos() -> Vec<PathBuf> {
    ATIVOS.with(|a| a.borrow().values().cloned().collect())
}

/// O caminho de um log cifrado que ninguém está gravando: `caminho` se estiver livre, senão
/// `nome (2).ext.cifrado`, `nome (3).ext.cifrado`… Dois escritores no mesmo arquivo misturariam os
/// registros.
pub fn caminho_livre(caminho: &Path) -> PathBuf {
    numerado(caminho, em_uso)
}

/// `caminho`, ou o primeiro `nome (n).ext[.cifrado]` que `ocupado` não recusa.
fn numerado(caminho: &Path, ocupado: impl Fn(&Path) -> bool) -> PathBuf {
    if !ocupado(caminho) {
        return caminho.to_owned();
    }
    let nome = caminho
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = nome.strip_suffix(".cifrado").unwrap_or(&nome);
    let (radical, extensao) = match base.rfind('.') {
        Some(i) if i > 0 => base.split_at(i),
        _ => (base, ""),
    };
    let sufixo = if nome.ends_with(".cifrado") {
        ".cifrado"
    } else {
        ""
    };
    (2..)
        .map(|n| caminho.with_file_name(format!("{radical} ({n}){extensao}{sufixo}")))
        .find(|c| !ocupado(c))
        .expect("há um número livre")
}

/// Um log da lista da página Logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntradaLog {
    /// O caminho do arquivo.
    pub caminho: PathBuf,
    /// O nome do arquivo.
    pub nome: String,
    /// O nome da conexão, quando se sabe.
    pub conexao: Option<String>,
    /// A data de modificação.
    pub modificado: Option<SystemTime>,
    /// O tamanho do arquivo no disco, em bytes.
    pub tamanho: u64,
    /// O arquivo é um log cifrado (pela extensão `.cifrado`).
    pub cifrado: bool,
    /// Alguma sessão grava nele agora.
    pub gravando: bool,
}

/// Se o nome de arquivo é de um log: `.log`, `.txt` ou `.log.cifrado` (e `.txt.cifrado`).
fn nome_de_log(nome: &str) -> bool {
    let base = nome.strip_suffix(SUFIXO_CIFRADO).unwrap_or(nome);
    base.ends_with(".log") || base.ends_with(".txt")
}

/// O prefixo (o nome da conexão) de um arquivo `nome-AAAAMMDD-HHMMSS-N.log[.cifrado]`.
fn prefixo_do_nome(nome: &str) -> Option<&str> {
    let base = nome.strip_suffix(SUFIXO_CIFRADO).unwrap_or(nome);
    let base = base.strip_suffix(".log")?;
    // `(2)` de um caminho que já estava em uso.
    let base = base
        .strip_suffix(')')
        .and_then(|b| b.rsplit_once(" (").map(|(a, _)| a))
        .unwrap_or(base);
    let (resto, n) = base.rsplit_once('-')?;
    n.chars().all(|c| c.is_ascii_digit()).then_some(())?;
    let (resto, hora) = resto.rsplit_once('-')?;
    (hora.len() == 6 && hora.chars().all(|c| c.is_ascii_digit())).then_some(())?;
    let (prefixo, dia) = resto.rsplit_once('-')?;
    (dia.len() == 8 && dia.chars().all(|c| c.is_ascii_digit())).then_some(prefixo)
}

/// Os logs que a página mostra: os arquivos de log da `pasta` e os `log_file` das `conexoes`
/// (cifrados ou não, mesmo fora da pasta), do mais novo para o mais velho. `ativos` são os que
/// as sessões gravam agora.
pub fn listar(pasta: &Path, conexoes: &[Connection], ativos: &[PathBuf]) -> Vec<EntradaLog> {
    let mut caminhos: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = fs::read_dir(pasta) {
        for entrada in dir.flatten() {
            let caminho = entrada.path();
            let nome = entrada.file_name().to_string_lossy().into_owned();
            if caminho.is_file() && nome_de_log(&nome) {
                caminhos.push(caminho);
            }
        }
    }
    for c in conexoes {
        if let Some(arquivo) = &c.log_file {
            for candidato in [arquivo.clone(), crate::abrir::com_sufixo_cifrado(arquivo)] {
                if candidato.is_file() && !caminhos.contains(&candidato) {
                    caminhos.push(candidato);
                }
            }
        }
    }
    let por_nome: HashMap<String, &Connection> = conexoes
        .iter()
        .map(|c| (crate::abrir::nome_limpo(&c.name), c))
        .collect();
    let mut entradas: Vec<EntradaLog> = caminhos
        .into_iter()
        .filter_map(|caminho| {
            let meta = fs::metadata(&caminho).ok()?;
            let nome = caminho.file_name()?.to_string_lossy().into_owned();
            let conexao = conexoes
                .iter()
                .find(|c| {
                    c.log_file.as_ref().is_some_and(|f| {
                        *f == caminho || crate::abrir::com_sufixo_cifrado(f) == caminho
                    })
                })
                .map(|c| c.name.clone())
                .or_else(|| {
                    prefixo_do_nome(&nome)
                        .and_then(|p| por_nome.get(p))
                        .map(|c| c.name.clone())
                });
            Some(EntradaLog {
                cifrado: nome.ends_with(SUFIXO_CIFRADO),
                gravando: ativos.contains(&caminho),
                modificado: meta.modified().ok(),
                tamanho: meta.len(),
                nome,
                conexao,
                caminho,
            })
        })
        .collect();
    entradas.sort_by(|a, b| {
        b.modificado
            .cmp(&a.modificado)
            .then_with(|| a.nome.cmp(&b.nome))
    });
    entradas
}

/// Guarda só o fim do que recebe: no máximo `maximo` bytes (0 guarda tudo). `total` conta tudo
/// o que passou.
struct Cauda {
    maximo: usize,
    bytes: Vec<u8>,
    total: u64,
}

impl Write for Cauda {
    fn write(&mut self, dados: &[u8]) -> io::Result<usize> {
        self.total += dados.len() as u64;
        self.bytes.extend_from_slice(dados);
        // Corta quando passa do dobro, para não copiar a cada escrita.
        if self.maximo > 0 && self.bytes.len() > self.maximo * 2 {
            let sobra = self.bytes.len() - self.maximo;
            self.bytes.drain(..sobra);
        }
        Ok(dados.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Um leitor que conta o que passou e avisa `progresso(lido, total)` a cada leitura.
struct Contado<'a, R> {
    dentro: R,
    lido: u64,
    total: u64,
    progresso: &'a dyn Fn(u64, u64),
}

impl<R: io::Read> io::Read for Contado<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.dentro.read(buf)?;
        self.lido += n as u64;
        (self.progresso)(self.lido, self.total);
        Ok(n)
    }
}

/// O que a página mostra de um log.
#[derive(Debug)]
pub struct Lido {
    /// O texto (o fim dele, se passou do limite).
    pub texto: String,
    /// Quantos bytes de texto o log tem no total.
    pub total: u64,
    /// O texto mostrado é só o fim do log.
    pub so_o_fim: bool,
    /// Segmentos, bytes e avisos da leitura.
    pub leitura: Leitura,
    /// Por que a leitura parou antes do fim, se parou. O `texto` é o que veio até ali.
    pub erro: Option<ErroLog>,
}

/// Lê o texto de um log, de texto ou cifrado (pela marca no começo do arquivo, e não pela
/// extensão), guardando só os últimos `maximo` bytes (0 guarda tudo). `progresso(lido, total)` diz
/// quantos bytes do arquivo (no disco) já foram lidos.
pub fn ler_para_ver(
    caminho: &Path,
    chaves: &HashMap<[u8; 8], [u8; 32]>,
    maximo: u64,
    cancelar: &AtomicBool,
    progresso: &dyn Fn(u64, u64),
) -> Lido {
    let mut cauda = Cauda {
        maximo: usize::try_from(maximo).unwrap_or(usize::MAX),
        bytes: Vec::new(),
        total: 0,
    };
    let resultado = match fs::File::open(caminho) {
        Err(e) => Err(ErroLog::Io(e)),
        Ok(arquivo) => {
            let total = arquivo.metadata().map_or(0, |m| m.len());
            let mut origem = BufReader::new(Contado {
                dentro: arquivo,
                lido: 0,
                total,
                progresso,
            });
            if log_cifrado::e_cifrado(caminho) {
                log_cifrado::ler(&mut origem, chaves, &mut cauda, cancelar)
            } else if caminho.to_string_lossy().ends_with(SUFIXO_CIFRADO) {
                Err(ErroLog::NaoE)
            } else {
                log_cifrado::texto_simples(&mut origem, &mut cauda, cancelar)
            }
        }
    };
    let (leitura, erro) = match resultado {
        Ok(l) => (l, None),
        Err(e) => (Leitura::default(), Some(e)),
    };
    let total = cauda.total;
    let mut bytes = cauda.bytes;
    let so_o_fim = cauda.maximo > 0 && total > cauda.maximo as u64;
    if so_o_fim {
        let sobra = bytes.len().saturating_sub(cauda.maximo);
        bytes.drain(..sobra);
        // Começa numa linha inteira, se houver uma quebra por perto.
        if let Some(i) = bytes.iter().take(4096).position(|b| *b == b'\n') {
            bytes.drain(..=i);
        }
    }
    Lido {
        texto: String::from_utf8_lossy(&bytes).into_owned(),
        total,
        so_o_fim,
        leitura,
        erro,
    }
}

/// O nome de um log exportado: sem o `.cifrado` e com `.log` se não sobrar extensão de texto.
pub fn nome_exportado(nome: &str) -> String {
    let base = nome.strip_suffix(SUFIXO_CIFRADO).unwrap_or(nome);
    if base.ends_with(".log") || base.ends_with(".txt") {
        base.to_owned()
    } else {
        format!("{base}.log")
    }
}

/// `nome` dentro de `pasta`, ou `nome (2).ext`, `nome (3).ext`… se já existe.
pub fn sem_repetir(pasta: &Path, nome: &str) -> PathBuf {
    let direto = pasta.join(nome);
    if !direto.exists() {
        return direto;
    }
    let (radical, extensao) = match nome.rfind('.') {
        Some(i) if i > 0 => nome.split_at(i),
        _ => (nome, ""),
    };
    (2..)
        .map(|n| pasta.join(format!("{radical} ({n}){extensao}")))
        .find(|c| !c.exists())
        .expect("há um número livre")
}

/// Grava em `destino` o texto de um log: decifrado, se for cifrado, ou uma cópia, se não. Só
/// deixa o arquivo no lugar se a leitura chegou ao fim; num erro, não sobra nada.
pub fn exportar_texto(
    origem: &Path,
    destino: &Path,
    chaves: &HashMap<[u8; 8], [u8; 32]>,
    cancelar: &AtomicBool,
) -> Result<Leitura, ErroLog> {
    let provisorio = {
        let mut p = destino.as_os_str().to_owned();
        p.push(".tmp");
        PathBuf::from(p)
    };
    let feito = (|| {
        let mut entrada = BufReader::new(fs::File::open(origem)?);
        let mut saida = io::BufWriter::new(fs::File::create(&provisorio)?);
        let leitura = if log_cifrado::e_cifrado(origem) {
            log_cifrado::ler(&mut entrada, chaves, &mut saida, cancelar)?
        } else if origem.to_string_lossy().ends_with(SUFIXO_CIFRADO) {
            return Err(ErroLog::NaoE);
        } else {
            log_cifrado::texto_simples(&mut entrada, &mut saida, cancelar)?
        };
        saida.flush()?;
        saida.into_inner().map_err(|e| e.into_error())?.sync_all()?;
        fs::rename(&provisorio, destino)?;
        Ok(leitura)
    })();
    if feito.is_err() {
        let _ = fs::remove_file(&provisorio);
    }
    feito
}

/// O que o apagar fez.
#[derive(Debug, Default)]
pub struct Apagados {
    /// Quantos arquivos saíram.
    pub apagados: usize,
    /// Quantos ficaram porque uma sessão grava neles.
    pub gravando: usize,
    /// Os que o sistema não deixou apagar.
    pub erros: Vec<(PathBuf, io::Error)>,
}

/// Apaga os `caminhos`, menos os que `ativos` diz que estão sendo gravados.
pub fn apagar(caminhos: &[PathBuf], ativos: &[PathBuf]) -> Apagados {
    let mut r = Apagados::default();
    for c in caminhos {
        if ativos.contains(c) {
            r.gravando += 1;
        } else {
            match fs::remove_file(c) {
                Ok(()) => r.apagados += 1,
                Err(e) => r.erros.push((c.clone(), e)),
            }
        }
    }
    r
}

/// O sentido de uma conversão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sentido {
    /// De texto para cifrado.
    Cifrar,
    /// De cifrado para texto.
    Decifrar,
}

/// O que uma conversão deixou.
#[derive(Debug)]
pub struct Convertido {
    /// O arquivo novo.
    pub destino: PathBuf,
    /// Avisos da leitura (só ao decifrar): o texto saiu, mas o log tinha o que notar.
    pub avisos: Vec<AvisoLog>,
    /// O original saiu (foi pedido e deu para apagar).
    pub original_apagado: bool,
}

/// O `.tmp` ao lado de `destino`.
fn provisorio_de(destino: &Path) -> PathBuf {
    let mut p = destino.as_os_str().to_owned();
    p.push(".tmp");
    PathBuf::from(p)
}

/// Passa a data de modificação do original para o arquivo novo, para a lista não trocar de
/// ordem. Não conseguir não é erro: é só a ordem.
fn copiar_data(modificado: Option<SystemTime>, destino: &Path) {
    let Some(data) = modificado else { return };
    if let Ok(arquivo) = fs::File::options().write(true).open(destino) {
        let _ = arquivo.set_modified(data);
    }
}

/// Só soma o que passa por ele no SHA-256.
struct Somador(Sha256);

impl Write for Somador {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Cifra o log de texto `origem` para `X.log.cifrado`, ao lado dele.
///
/// Grava num `.tmp`, relê com `chaves` (a privada precisa estar nelas) e compara o SHA-256 do texto
/// com o do original antes de pôr o nome definitivo; num erro, o original fica como estava e não
/// sobra `.tmp`. Com `apagar_original`, o `X.log` sai no fim. A data de modificação do novo é a do
/// original.
pub fn cifrar_log(
    origem: &Path,
    publica: &[u8; 32],
    chaves: &HashMap<[u8; 8], [u8; 32]>,
    apagar_original: bool,
    cancelar: &AtomicBool,
) -> Result<Convertido, ErroLog> {
    let nome = origem
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let destino = numerado(
        &origem.with_file_name(format!("{nome}{SUFIXO_CIFRADO}")),
        |c| c.exists() || em_uso(c),
    );
    let provisorio = provisorio_de(&destino);
    let _ = fs::remove_file(&provisorio);
    let feito = (|| {
        let modificado = fs::metadata(origem)?.modified().ok();
        let mut soma = Sha256::new();
        {
            let mut entrada = fs::File::open(origem)?;
            let mut saida = EscritorCifrado::abrir(&provisorio, publica)?;
            let mut buf = vec![0u8; log_cifrado::REGISTRO];
            loop {
                if cancelar.load(Ordering::Relaxed) {
                    return Err(ErroLog::Cancelado);
                }
                let n = match entrada.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(e.into()),
                };
                soma.update(&buf[..n]);
                saida.write_all(&buf[..n])?;
            }
            // O `Drop` grava o registro `fim`.
        }
        fs::File::open(&provisorio)?.sync_all()?;
        let mut relido = Somador(Sha256::new());
        log_cifrado::ler(
            &mut BufReader::new(fs::File::open(&provisorio)?),
            chaves,
            &mut relido,
            cancelar,
        )?;
        if relido.0.finalize() != soma.finalize() {
            return Err(ErroLog::Alterado {
                segmento: 1,
                registro: 0,
            });
        }
        fs::rename(&provisorio, &destino)?;
        copiar_data(modificado, &destino);
        let original_apagado = apagar_original && fs::remove_file(origem).is_ok();
        Ok(Convertido {
            destino: destino.clone(),
            avisos: Vec::new(),
            original_apagado,
        })
    })();
    if feito.is_err() {
        let _ = fs::remove_file(&provisorio);
    }
    feito
}

/// Decifra o log `origem` para `X.log` (ou `X (2).log`…), ao lado dele, com a data de
/// modificação do cifrado. Com `apagar_cifrado`, o cifrado sai no fim. Um log alterado ou sem
/// chave dá erro, sem `.tmp` e com o cifrado onde estava.
pub fn decifrar_log(
    origem: &Path,
    chaves: &HashMap<[u8; 8], [u8; 32]>,
    apagar_cifrado: bool,
    cancelar: &AtomicBool,
) -> Result<Convertido, ErroLog> {
    let nome = origem
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let destino = numerado(&origem.with_file_name(nome_exportado(&nome)), |c| {
        c.exists()
    });
    let modificado = fs::metadata(origem).and_then(|m| m.modified()).ok();
    let leitura = exportar_texto(origem, &destino, chaves, cancelar)?;
    copiar_data(modificado, &destino);
    let original_apagado = apagar_cifrado && fs::remove_file(origem).is_ok();
    Ok(Convertido {
        destino,
        avisos: leitura.avisos,
        original_apagado,
    })
}

/// O que um lote de conversões fez.
#[derive(Debug, Default)]
pub struct Convertidos {
    /// Quantos foram convertidos.
    pub convertidos: usize,
    /// Quantos já estavam no formato pedido (e ficaram como estão).
    pub ignorados: usize,
    /// Quantos ficaram porque uma sessão grava neles.
    pub gravando: usize,
    /// Os avisos de leitura, por arquivo.
    pub avisos: Vec<(PathBuf, AvisoLog)>,
    /// Os que deram erro, com o motivo.
    pub erros: Vec<(PathBuf, String)>,
    /// A pessoa cancelou: os que faltavam não foram tocados.
    pub cancelado: bool,
}

/// Converte `caminhos` no `sentido`, um por vez. `chaves` abre os cifrados (e confere os que
/// `Sentido::Cifrar` acaba de gravar); `publica` é a chave dos logs de agora, e `Cifrar` precisa
/// dela. `progresso(feitos, total, arquivo)` diz onde está, antes de cada item.
#[allow(clippy::too_many_arguments)]
pub fn converter_lote(
    caminhos: &[PathBuf],
    sentido: Sentido,
    publica: Option<&[u8; 32]>,
    chaves: &HashMap<[u8; 8], [u8; 32]>,
    apagar_originais: bool,
    ativos: &[PathBuf],
    cancelar: &AtomicBool,
    progresso: &dyn Fn(usize, usize, &Path),
) -> Convertidos {
    let mut r = Convertidos::default();
    for (i, caminho) in caminhos.iter().enumerate() {
        if cancelar.load(Ordering::Relaxed) {
            r.cancelado = true;
            break;
        }
        progresso(i, caminhos.len(), caminho);
        if ativos.contains(caminho) {
            r.gravando += 1;
            continue;
        }
        let cifrado =
            caminho.to_string_lossy().ends_with(SUFIXO_CIFRADO) || log_cifrado::e_cifrado(caminho);
        let resultado = match sentido {
            Sentido::Cifrar if cifrado => {
                r.ignorados += 1;
                continue;
            }
            Sentido::Decifrar if !cifrado => {
                r.ignorados += 1;
                continue;
            }
            Sentido::Cifrar => match publica {
                Some(publica) => cifrar_log(caminho, publica, chaves, apagar_originais, cancelar),
                None => Err(ErroLog::ChaveAusente { id: String::new() }),
            },
            Sentido::Decifrar => decifrar_log(caminho, chaves, apagar_originais, cancelar),
        };
        match resultado {
            Ok(c) => {
                r.convertidos += 1;
                r.avisos
                    .extend(c.avisos.into_iter().map(|a| (caminho.clone(), a)));
            }
            Err(ErroLog::Cancelado) => {
                r.cancelado = true;
                break;
            }
            Err(e) => r.erros.push((caminho.clone(), e.to_string())),
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::modelo::Kind;
    use std::time::Duration;

    #[test]
    fn caminho_livre_numera_o_segundo_log() {
        let a = Path::new("/x/prod.log.cifrado");
        assert_eq!(caminho_livre(a), a);
        registrar_ativo("1-1", a);
        assert_eq!(caminho_livre(a), Path::new("/x/prod (2).log.cifrado"));
        registrar_ativo("1-2", Path::new("/x/prod (2).log.cifrado"));
        assert_eq!(caminho_livre(a), Path::new("/x/prod (3).log.cifrado"));
        esquecer_ativo("1-1");
        assert_eq!(caminho_livre(a), a);
        assert!(!em_uso(a));
        assert_eq!(
            ativo_da_sessao("1-2").unwrap(),
            Path::new("/x/prod (2).log.cifrado")
        );
        esquecer_ativo("1-2");
        assert!(ativos().is_empty());
        // Sem extensão.
        let b = Path::new("/x/diario.cifrado");
        registrar_ativo("2-1", b);
        assert_eq!(caminho_livre(b), Path::new("/x/diario (2).cifrado"));
    }

    fn pasta(nome: &str) -> PastaTemporaria {
        let p = PastaTemporaria::nova(nome);
        fs::create_dir_all(&p.0).unwrap();
        p
    }

    /// Escreve `texto` em `caminho` e põe a data de modificação `segundos` depois de 1970.
    fn escrever(caminho: &Path, texto: &str, segundos: u64) {
        fs::write(caminho, texto).unwrap();
        let arquivo = fs::File::options().write(true).open(caminho).unwrap();
        arquivo
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(segundos))
            .unwrap();
    }

    fn sem_cancelar() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn o_prefixo_do_nome_e_o_da_conexao() {
        assert_eq!(
            prefixo_do_nome("web-01-20261020-101500-1.log"),
            Some("web-01")
        );
        assert_eq!(
            prefixo_do_nome("web-20261020-101500-12.log.cifrado"),
            Some("web")
        );
        assert_eq!(
            prefixo_do_nome("web-20261020-101500-1 (2).log.cifrado"),
            Some("web")
        );
        assert_eq!(prefixo_do_nome("diario.log"), None);
        assert_eq!(prefixo_do_nome("web-2026-1015-1.log"), None);
    }

    #[test]
    fn a_lista_traz_os_logs_do_mais_novo_ao_mais_velho_com_a_conexao() {
        let p = pasta("logs-lista");
        let fora = pasta("logs-lista-fora");
        escrever(&p.0.join("web-20261020-101500-1.log"), "a", 300);
        escrever(&p.0.join("db-20261020-101500-1.log.cifrado"), "b", 500);
        escrever(&p.0.join("anotacoes.txt"), "c", 100);
        escrever(&p.0.join("foto.png"), "d", 900);
        escrever(&fora.0.join("fixo.log"), "e", 400);
        let mut web = Connection::new("web", Kind::Ssh);
        web.log_file = None;
        let mut db = Connection::new("db", Kind::Ssh);
        db.log_file = None;
        let mut fixa = Connection::new("Fixa", Kind::Ssh);
        fixa.log_file = Some(fora.0.join("fixo.log"));
        let ativos = vec![p.0.join("db-20261020-101500-1.log.cifrado")];
        let lista = listar(&p.0, &[web, db, fixa], &ativos);
        let nomes: Vec<&str> = lista.iter().map(|e| e.nome.as_str()).collect();
        assert_eq!(
            nomes,
            [
                "db-20261020-101500-1.log.cifrado",
                "fixo.log",
                "web-20261020-101500-1.log",
                "anotacoes.txt"
            ]
        );
        assert_eq!(lista[0].conexao.as_deref(), Some("db"));
        assert!(lista[0].cifrado && lista[0].gravando);
        assert_eq!(lista[1].conexao.as_deref(), Some("Fixa"));
        assert!(!lista[1].cifrado && !lista[1].gravando);
        assert_eq!(lista[2].conexao.as_deref(), Some("web"));
        assert_eq!(lista[3].conexao, None);
        assert_eq!(lista[3].tamanho, 1);
    }

    #[test]
    fn o_log_fixo_cifrado_da_conexao_entra_na_lista() {
        let fora = pasta("logs-fixo-cifrado");
        escrever(&fora.0.join("fixo.log.cifrado"), "x", 10);
        let mut c = Connection::new("Fixa", Kind::Ssh);
        c.log_file = Some(fora.0.join("fixo.log"));
        let lista = listar(&fora.0.join("nao-existe"), &[c], &[]);
        assert_eq!(lista.len(), 1);
        assert!(lista[0].cifrado);
        assert_eq!(lista[0].conexao.as_deref(), Some("Fixa"));
    }

    #[test]
    fn so_o_fim_do_log_de_texto_e_comecando_numa_linha() {
        let p = pasta("logs-cauda");
        let texto: String = (0..400).map(|i| format!("linha {i:04}\n")).collect();
        escrever(&p.0.join("a.log"), &texto, 1);
        let todo = ler_para_ver(
            &p.0.join("a.log"),
            &HashMap::new(),
            0,
            &sem_cancelar(),
            &|_, _| {},
        );
        assert_eq!(todo.texto, texto);
        assert!(!todo.so_o_fim && todo.erro.is_none());
        assert_eq!(todo.total, texto.len() as u64);
        let fim = ler_para_ver(
            &p.0.join("a.log"),
            &HashMap::new(),
            500,
            &sem_cancelar(),
            &|_, _| {},
        );
        assert!(fim.so_o_fim);
        assert_eq!(fim.total, texto.len() as u64);
        assert!(fim.texto.len() <= 500);
        assert!(texto.ends_with(&fim.texto));
        assert!(fim.texto.starts_with("linha "));
        assert!(fim.texto.ends_with("linha 0399\n"));
    }

    #[test]
    fn o_log_cifrado_abre_com_a_chave_e_so_o_fim_quando_grande() {
        let p = pasta("logs-cifrado-ver");
        let (privada, publica) = log_cifrado::par_novo().unwrap();
        let caminho = p.0.join("a.log.cifrado");
        let texto: String = (0..4000).map(|i| format!("linha {i:05}\n")).collect();
        {
            let mut e = log_cifrado::EscritorCifrado::abrir(&caminho, &publica).unwrap();
            e.write_all(texto.as_bytes()).unwrap();
        }
        let chaves = HashMap::from([(log_cifrado::id_da_chave(&publica), privada)]);
        let tudo = ler_para_ver(&caminho, &chaves, 0, &sem_cancelar(), &|_, _| {});
        assert_eq!(tudo.texto, texto);
        assert!(tudo.erro.is_none() && tudo.leitura.avisos.is_empty());
        let fim = ler_para_ver(&caminho, &chaves, 1000, &sem_cancelar(), &|_, _| {});
        assert!(fim.so_o_fim && texto.ends_with(&fim.texto) && fim.texto.len() <= 1000);
        assert_eq!(fim.total, texto.len() as u64);
        // Sem a chave: erro, e nenhum texto.
        let sem = ler_para_ver(&caminho, &HashMap::new(), 0, &sem_cancelar(), &|_, _| {});
        assert!(matches!(sem.erro, Some(ErroLog::ChaveAusente { .. })));
        assert!(sem.texto.is_empty());
    }

    #[test]
    fn um_cifrado_alterado_mostra_o_texto_de_antes_do_estrago() {
        let p = pasta("logs-alterado");
        let (privada, publica) = log_cifrado::par_novo().unwrap();
        let caminho = p.0.join("a.log.cifrado");
        {
            let mut e = log_cifrado::EscritorCifrado::abrir(&caminho, &publica).unwrap();
            e.write_all(&vec![b'x'; log_cifrado::REGISTRO]).unwrap();
            e.write_all(&vec![b'y'; log_cifrado::REGISTRO]).unwrap();
        }
        let mut bytes = fs::read(&caminho).unwrap();
        // Um byte no cifrado do segundo registro.
        let n = bytes.len();
        bytes[n - 40] ^= 1;
        fs::write(&caminho, bytes).unwrap();
        let chaves = HashMap::from([(log_cifrado::id_da_chave(&publica), privada)]);
        let lido = ler_para_ver(&caminho, &chaves, 0, &sem_cancelar(), &|_, _| {});
        assert!(matches!(lido.erro, Some(ErroLog::Alterado { .. })));
        assert_eq!(lido.texto.len(), log_cifrado::REGISTRO);
        assert!(lido.texto.bytes().all(|b| b == b'x'));
    }

    #[test]
    fn um_arquivo_cifrado_que_nao_e_do_formato_da_nao_e() {
        let p = pasta("logs-nao-e");
        escrever(&p.0.join("a.log.cifrado"), "texto puro", 1);
        let lido = ler_para_ver(
            &p.0.join("a.log.cifrado"),
            &HashMap::new(),
            0,
            &sem_cancelar(),
            &|_, _| {},
        );
        assert!(matches!(lido.erro, Some(ErroLog::NaoE)));
        // Um que não existe dá erro de disco.
        let lido = ler_para_ver(
            &p.0.join("x.log"),
            &HashMap::new(),
            0,
            &sem_cancelar(),
            &|_, _| {},
        );
        assert!(matches!(lido.erro, Some(ErroLog::Io(_))));
    }

    #[test]
    fn o_nome_exportado_perde_o_cifrado_e_o_repetido_ganha_numero() {
        assert_eq!(nome_exportado("web-1.log.cifrado"), "web-1.log");
        assert_eq!(nome_exportado("web-1.txt"), "web-1.txt");
        assert_eq!(nome_exportado("diario.cifrado"), "diario.log");
        let p = pasta("logs-sem-repetir");
        assert_eq!(sem_repetir(&p.0, "a.log"), p.0.join("a.log"));
        fs::write(p.0.join("a.log"), "").unwrap();
        assert_eq!(sem_repetir(&p.0, "a.log"), p.0.join("a (2).log"));
        fs::write(p.0.join("a (2).log"), "").unwrap();
        assert_eq!(sem_repetir(&p.0, "a.log"), p.0.join("a (3).log"));
    }

    #[test]
    fn exportar_decifra_copia_e_nao_deixa_resto_num_erro() {
        let p = pasta("logs-exportar");
        let (privada, publica) = log_cifrado::par_novo().unwrap();
        let cifrado = p.0.join("a.log.cifrado");
        {
            let mut e = log_cifrado::EscritorCifrado::abrir(&cifrado, &publica).unwrap();
            e.write_all(b"segredo\n").unwrap();
        }
        let chaves = HashMap::from([(log_cifrado::id_da_chave(&publica), privada)]);
        let destino = p.0.join("saida.log");
        let leitura = exportar_texto(&cifrado, &destino, &chaves, &sem_cancelar()).unwrap();
        assert_eq!(leitura.bytes, 8);
        assert_eq!(fs::read_to_string(&destino).unwrap(), "segredo\n");
        // Um de texto vira cópia.
        escrever(&p.0.join("b.log"), "claro\n", 1);
        let copia = p.0.join("copia.log");
        exportar_texto(&p.0.join("b.log"), &copia, &chaves, &sem_cancelar()).unwrap();
        assert_eq!(fs::read_to_string(&copia).unwrap(), "claro\n");
        // Sem a chave: erro e nada no destino nem `.tmp`.
        let erro = p.0.join("erro.log");
        let r = exportar_texto(&cifrado, &erro, &HashMap::new(), &sem_cancelar());
        assert!(matches!(r, Err(ErroLog::ChaveAusente { .. })));
        assert!(!erro.exists() && !p.0.join("erro.log.tmp").exists());
    }

    #[test]
    fn apagar_pula_o_que_esta_gravando() {
        let p = pasta("logs-apagar");
        escrever(&p.0.join("a.log"), "a", 1);
        escrever(&p.0.join("b.log"), "b", 1);
        let todos = vec![p.0.join("a.log"), p.0.join("b.log"), p.0.join("c.log")];
        let r = apagar(&todos, &[p.0.join("b.log")]);
        assert_eq!((r.apagados, r.gravando, r.erros.len()), (1, 1, 1));
        assert!(!p.0.join("a.log").exists() && p.0.join("b.log").exists());
    }

    fn par_e_chaves() -> ([u8; 32], HashMap<[u8; 8], [u8; 32]>) {
        let (privada, publica) = log_cifrado::par_novo().unwrap();
        (
            publica,
            HashMap::from([(log_cifrado::id_da_chave(&publica), privada)]),
        )
    }

    fn data_de(caminho: &Path) -> SystemTime {
        fs::metadata(caminho).unwrap().modified().unwrap()
    }

    #[test]
    fn cifrar_um_log_de_200_kib_decifra_para_os_mesmos_bytes_com_a_data_do_original() {
        let p = pasta("logs-cifrar");
        let (publica, chaves) = par_e_chaves();
        let texto: String = (0..20_000).map(|i| format!("linha {i:06}\n")).collect();
        assert!(texto.len() > 200 * 1024);
        let origem = p.0.join("web-20261020-101500-1.log");
        escrever(&origem, &texto, 12_345);
        let r = cifrar_log(&origem, &publica, &chaves, true, &sem_cancelar()).unwrap();
        assert_eq!(r.destino, p.0.join("web-20261020-101500-1.log.cifrado"));
        assert!(r.original_apagado && !origem.exists());
        assert!(!p.0.join("web-20261020-101500-1.log.cifrado.tmp").exists());
        assert_eq!(
            data_de(&r.destino),
            SystemTime::UNIX_EPOCH + Duration::from_secs(12_345)
        );
        let lido = ler_para_ver(&r.destino, &chaves, 0, &sem_cancelar(), &|_, _| {});
        assert_eq!(lido.texto, texto);
        assert!(lido.erro.is_none());
    }

    #[test]
    fn cifrar_sem_apagar_o_original_mantem_os_dois() {
        let p = pasta("logs-cifrar-manter");
        let (publica, chaves) = par_e_chaves();
        let origem = p.0.join("a.log");
        escrever(&origem, "um\n", 5);
        let r = cifrar_log(&origem, &publica, &chaves, false, &sem_cancelar()).unwrap();
        assert!(!r.original_apagado && origem.exists() && r.destino.exists());
    }

    #[test]
    fn a_conferencia_que_falha_deixa_o_original_e_nao_sobra_tmp() {
        let p = pasta("logs-cifrar-confere");
        let (publica, _) = par_e_chaves();
        let origem = p.0.join("a.log");
        escrever(&origem, "importante\n", 5);
        // Sem a privada no mapa, a releitura falha: é a conferência recusando.
        let r = cifrar_log(&origem, &publica, &HashMap::new(), true, &sem_cancelar());
        assert!(matches!(r, Err(ErroLog::ChaveAusente { .. })));
        assert_eq!(fs::read_to_string(&origem).unwrap(), "importante\n");
        let nomes: Vec<String> = fs::read_dir(&p.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(nomes, ["a.log"]);
    }

    #[test]
    fn cifrar_com_o_destino_existente_numera_o_novo() {
        let p = pasta("logs-cifrar-numera");
        let (publica, chaves) = par_e_chaves();
        escrever(&p.0.join("a.log"), "um\n", 5);
        escrever(&p.0.join("a.log.cifrado"), "ocupado", 5);
        let r = cifrar_log(&p.0.join("a.log"), &publica, &chaves, true, &sem_cancelar()).unwrap();
        assert_eq!(r.destino, p.0.join("a (2).log.cifrado"));
        assert_eq!(
            fs::read_to_string(p.0.join("a.log.cifrado")).unwrap(),
            "ocupado"
        );
    }

    #[test]
    fn decifrar_mantem_o_cifrado_por_padrao_e_numera_se_o_texto_ja_existe() {
        let p = pasta("logs-decifrar");
        let (publica, chaves) = par_e_chaves();
        let cifrado = p.0.join("a.log.cifrado");
        {
            let mut e = EscritorCifrado::abrir(&cifrado, &publica).unwrap();
            e.write_all(b"segredo\n").unwrap();
        }
        fs::File::options()
            .write(true)
            .open(&cifrado)
            .unwrap()
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(777))
            .unwrap();
        escrever(&p.0.join("a.log"), "já existia", 1);
        let r = decifrar_log(&cifrado, &chaves, false, &sem_cancelar()).unwrap();
        assert_eq!(r.destino, p.0.join("a (2).log"));
        assert!(!r.original_apagado && cifrado.exists());
        assert_eq!(fs::read_to_string(&r.destino).unwrap(), "segredo\n");
        assert_eq!(
            data_de(&r.destino),
            SystemTime::UNIX_EPOCH + Duration::from_secs(777)
        );
        let r = decifrar_log(&cifrado, &chaves, true, &sem_cancelar()).unwrap();
        assert!(r.original_apagado && !cifrado.exists());
    }

    #[test]
    fn decifrar_um_alterado_falha_sem_tmp_e_o_cifrado_fica() {
        let p = pasta("logs-decifrar-alterado");
        let (publica, chaves) = par_e_chaves();
        let cifrado = p.0.join("a.log.cifrado");
        {
            let mut e = EscritorCifrado::abrir(&cifrado, &publica).unwrap();
            e.write_all(b"segredo\n").unwrap();
        }
        let mut bytes = fs::read(&cifrado).unwrap();
        let n = bytes.len();
        bytes[n - 30] ^= 1;
        fs::write(&cifrado, &bytes).unwrap();
        let r = decifrar_log(&cifrado, &chaves, true, &sem_cancelar());
        assert!(matches!(r, Err(ErroLog::Alterado { .. })));
        assert_eq!(fs::read(&cifrado).unwrap(), bytes);
        let nomes: Vec<String> = fs::read_dir(&p.0)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(nomes, ["a.log.cifrado"]);
    }

    #[test]
    fn o_lote_pula_o_que_grava_ignora_o_que_ja_esta_no_formato_e_conta_os_erros() {
        let p = pasta("logs-lote");
        let (publica, chaves) = par_e_chaves();
        escrever(&p.0.join("a.log"), "a\n", 1);
        escrever(&p.0.join("b.log"), "b\n", 2);
        escrever(&p.0.join("c.log"), "c\n", 3);
        {
            let mut e = EscritorCifrado::abrir(&p.0.join("d.log.cifrado"), &publica).unwrap();
            e.write_all(b"d\n").unwrap();
        }
        let todos: Vec<PathBuf> = ["a.log", "b.log", "c.log", "d.log.cifrado", "x.log"]
            .iter()
            .map(|n| p.0.join(n))
            .collect();
        let vistos = RefCell::new(Vec::new());
        let r = converter_lote(
            &todos,
            Sentido::Cifrar,
            Some(&publica),
            &chaves,
            true,
            &[p.0.join("b.log")],
            &sem_cancelar(),
            &|feitos, total, _| vistos.borrow_mut().push((feitos, total)),
        );
        assert_eq!(
            (r.convertidos, r.gravando, r.ignorados, r.erros.len()),
            (2, 1, 1, 1)
        );
        assert!(!r.cancelado);
        assert_eq!(*vistos.borrow(), [(0, 5), (1, 5), (2, 5), (3, 5), (4, 5)]);
        assert!(p.0.join("b.log").exists() && !p.0.join("a.log").exists());
        assert!(p.0.join("a.log.cifrado").exists() && p.0.join("c.log.cifrado").exists());
    }

    #[test]
    fn cancelar_no_meio_do_lote_deixa_os_primeiros_prontos_sem_tmp() {
        let p = pasta("logs-lote-cancela");
        let (publica, chaves) = par_e_chaves();
        for n in ["a", "b", "c"] {
            escrever(&p.0.join(format!("{n}.log")), "x\n", 1);
        }
        let todos: Vec<PathBuf> = ["a.log", "b.log", "c.log"]
            .iter()
            .map(|n| p.0.join(n))
            .collect();
        let cancelar = AtomicBool::new(false);
        let r = converter_lote(
            &todos,
            Sentido::Cifrar,
            Some(&publica),
            &chaves,
            true,
            &[],
            &cancelar,
            &|feitos, _, _| {
                if feitos == 1 {
                    cancelar.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(r.cancelado);
        assert_eq!(r.convertidos, 1);
        assert!(p.0.join("a.log.cifrado").exists() && p.0.join("b.log").exists());
        assert!(p.0.join("c.log").exists());
        assert!(
            fs::read_dir(&p.0)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp"))
        );
    }
}
