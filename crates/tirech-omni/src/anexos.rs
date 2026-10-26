//! Os arquivos anexados às notas (M47): cada anexo é um arquivo cifrado em `vault/<id>`, ao lado
//! do `vault.json`, e a ficha dele (nome, tamanho, chave…) vai dentro do item do cofre.
//!
//! O formato é um fluxo AES-256-GCM em blocos (como o STREAM): o anexo nunca é lido inteiro para a
//! memória na gravação nem na leitura para arquivo, e um arquivo não muda depois de gravado.
//!
//! ```text
//! cabeçalho (25 bytes) = "TRCHANEX" · versão (1) · tamanho do bloco (u32 BE) · prefixo do
//!                        nonce (8 aleatórios) · 4 zeros
//! bloco n              = AES-256-GCM(claro do bloco) + etiqueta de 16 bytes
//! nonce do bloco       = prefixo (8) · n (u32 BE)
//! dados autenticados   = cabeçalho · id (ASCII) · n (u32 BE) · 1 no último bloco, 0 nos outros
//! ```
//!
//! Todo bloco é cheio, menos o último (de 0 ao tamanho do bloco). Um anexo de 0 byte é um bloco
//! vazio marcado como último; um múltiplo exato do bloco termina num bloco cheio marcado como
//! último. Cortar, trocar ou repetir blocos, ou mexer no cabeçalho, quebra a etiqueta.

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use aes_gcm::aead::consts::U12;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};

use crate::arquivos::formato::tamanho as tamanho_legivel;
use crate::cofre::Anexo;
use crate::t;

/// O tamanho de um bloco do anexo: 1 MiB.
pub const BLOCO: usize = 1024 * 1024;

/// Quanto um arquivo sem ficha espera antes de a coleta apagá-lo: 24 horas. Dá tempo de a
/// sincronização trazer a ficha de outra máquina.
pub const MARGEM_ORFAO: u64 = 24 * 3600;

/// A idade a partir da qual a coleta apaga um `.tmp` esquecido: 1 hora.
const VIDA_DO_TMP: u64 = 3600;

/// O nome da pasta dos anexos, ao lado do `vault.json`.
pub const PASTA: &str = "vault";

/// O arquivo (fora do cofre, na pasta de dados) com os arquivos sem ficha e a hora em que foram
/// vistos pela primeira vez.
pub const REGISTRO_DE_ORFAOS: &str = "vault-orphans.json";

const MARCA: &[u8; 8] = b"TRCHANEX";
const VERSAO_DO_FORMATO: u8 = 1;
const CABECALHO: usize = 25;
const ETIQUETA: usize = 16;
/// O maior bloco que a leitura aceita: um cabeçalho mexido não pode pedir memória sem fim.
const BLOCO_MAXIMO: usize = 64 * 1024 * 1024;

/// O que um anexo em andamento já fez.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progresso {
    /// Bytes do claro já processados.
    pub feito: u64,
    /// Bytes do claro no total.
    pub total: u64,
}

/// Por que um anexo não foi gravado ou lido.
#[derive(Debug)]
pub enum ErroAnexo {
    /// O disco ou o arquivo de origem falhou.
    Io(io::Error),
    /// O disco não tem espaço para o anexo.
    SemEspaco {
        /// O que o anexo precisa, em bytes.
        precisa: u64,
        /// O que há livre, em bytes.
        livre: u64,
    },
    /// O arquivo do anexo foi cortado, trocado ou mexido.
    Corrompido,
    /// A ficha existe, mas o arquivo ainda não chegou a esta máquina.
    Ausente,
    /// A pessoa cancelou.
    Cancelado,
}

impl fmt::Display for ErroAnexo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroAnexo::Io(e) => write!(f, "{}", t!("anexo-io", erro = e.to_string())),
            ErroAnexo::SemEspaco { precisa, livre } => write!(
                f,
                "{}",
                t!(
                    "anexo-sem-espaco",
                    precisa = tamanho_legivel(*precisa),
                    livre = tamanho_legivel(*livre)
                )
            ),
            ErroAnexo::Corrompido => write!(f, "{}", t!("anexo-corrompido")),
            ErroAnexo::Ausente => write!(f, "{}", t!("anexo-ausente")),
            ErroAnexo::Cancelado => write!(f, "{}", t!("anexo-cancelado")),
        }
    }
}

impl std::error::Error for ErroAnexo {}

impl From<io::Error> for ErroAnexo {
    fn from(e: io::Error) -> Self {
        ErroAnexo::Io(e)
    }
}

/// Um identificador novo para um anexo: 128 bits aleatórios em hexadecimal (o mesmo gerador do
/// `uid` dos itens do cofre).
pub fn novo_id() -> String {
    crate::cofre::uid_novo()
}

/// Se `id` serve de nome de arquivo (só letras e dígitos): um `id` mexido não escapa de `vault/`.
fn id_valido(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// O caminho do arquivo de um anexo.
pub fn caminho(pasta: &Path, id: &str) -> PathBuf {
    pasta.join(id)
}

/// Se o arquivo do anexo está na pasta.
pub fn existe(pasta: &Path, id: &str) -> bool {
    id_valido(id) && caminho(pasta, id).is_file()
}

/// O tipo (MIME) de um arquivo pelo nome; `application/octet-stream` quando não se sabe.
pub fn tipo_do_nome(nome: &str) -> &'static str {
    let Some((_, extensao)) = nome.rsplit_once('.') else {
        return "application/octet-stream";
    };
    match extensao.to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "tar" => "application/x-tar",
        "json" => "application/json",
        "txt" | "log" | "pem" | "key" | "pub" | "crt" | "cer" | "csr" | "ppk" | "conf" | "ini"
        | "toml" | "cfg" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "yml" | "yaml" => "text/yaml",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "sh" | "ps1" => "text/x-shellscript",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

/// Quantos bytes tem o arquivo cifrado de um anexo de `tamanho` bytes com blocos de `bloco`.
pub fn tamanho_cifrado(tamanho: u64, bloco: usize) -> u64 {
    let bloco = bloco.max(1) as u64;
    let blocos = tamanho.div_ceil(bloco).max(1);
    CABECALHO as u64 + tamanho + blocos * ETIQUETA as u64
}

/// Confere se `livre` cobre `precisa`.
pub fn conferir_espaco(precisa: u64, livre: u64) -> Result<(), ErroAnexo> {
    if precisa > livre {
        Err(ErroAnexo::SemEspaco { precisa, livre })
    } else {
        Ok(())
    }
}

/// Confere o espaço livre do disco de `pasta` (ou de onde ela ficaria) para `precisa` bytes.
/// Se o sistema não diz quanto há, deixa passar.
pub fn conferir_espaco_em(pasta: &Path, precisa: u64) -> Result<(), ErroAnexo> {
    match tomni_ui::files::available_space(pasta) {
        Ok(livre) => conferir_espaco(precisa, livre),
        Err(_) => Ok(()),
    }
}

fn cifra(chave: &[u8; 32]) -> Aes256Gcm {
    Aes256Gcm::new(chave.into())
}

fn nonce_do_bloco(prefixo: &[u8], n: u32) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..8].copy_from_slice(prefixo);
    nonce[8..].copy_from_slice(&n.to_be_bytes());
    nonce
}

fn dados_autenticados(cabecalho: &[u8], id: &str, n: u32, ultimo: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(cabecalho.len() + id.len() + 5);
    aad.extend_from_slice(cabecalho);
    aad.extend_from_slice(id.as_bytes());
    aad.extend_from_slice(&n.to_be_bytes());
    aad.push(u8::from(ultimo));
    aad
}

/// Lê até encher `buf` ou acabar o arquivo; devolve quantos bytes leu.
fn ler_cheio(origem: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut lidos = 0;
    while lidos < buf.len() {
        match origem.read(&mut buf[lidos..]) {
            Ok(0) => break,
            Ok(n) => lidos += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(lidos)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A pasta `vault/`, só do dono no Unix.
fn criar_pasta(pasta: &Path) -> io::Result<()> {
    let mut construtor = fs::DirBuilder::new();
    construtor.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        construtor.mode(0o700);
    }
    construtor.create(pasta)
}

/// Cria um arquivo novo só do dono no Unix.
fn criar_arquivo(caminho: &Path) -> io::Result<fs::File> {
    let mut opcoes = fs::OpenOptions::new();
    opcoes.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opcoes.mode(0o600);
    }
    opcoes.open(caminho)
}

fn aleatorio<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|e| io::Error::other(t!("anexo-sem-aleatorio", erro = e.to_string())))?;
    Ok(bytes)
}

/// Cifra `origem` (de `tamanho` bytes, para o andamento) em `pasta/<id>`, em blocos de `bloco`
/// bytes, e devolve a ficha (sem `nome`, `tipo` e `criado`, que são de quem anexa). Grava num
/// `<id>.tmp` e só então o renomeia: um erro ou o cancelamento não deixam nada.
pub fn gravar(
    pasta: &Path,
    id: &str,
    origem: &mut dyn Read,
    tamanho: u64,
    bloco: usize,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Anexo, ErroAnexo> {
    if !id_valido(id) || bloco == 0 || bloco > BLOCO_MAXIMO {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, t!("anexo-invalido")).into());
    }
    criar_pasta(pasta)?;
    let destino = caminho(pasta, id);
    if destino.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, t!("anexo-ja-existe")).into());
    }
    let tmp = pasta.join(format!("{id}.tmp"));
    let _ = fs::remove_file(&tmp);
    let saida = criar_arquivo(&tmp)?;
    let resultado = gravar_blocos(saida, id, origem, tamanho, bloco, progresso, cancelado)
        .and_then(|ficha| {
            fs::rename(&tmp, &destino)
                .map(|()| ficha)
                .map_err(Into::into)
        });
    if resultado.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    resultado
}

fn gravar_blocos(
    mut saida: fs::File,
    id: &str,
    origem: &mut dyn Read,
    tamanho: u64,
    bloco: usize,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Anexo, ErroAnexo> {
    let chave = aleatorio::<32>()?;
    let prefixo = aleatorio::<8>()?;
    let mut cabecalho = Vec::with_capacity(CABECALHO);
    cabecalho.extend_from_slice(MARCA);
    cabecalho.push(VERSAO_DO_FORMATO);
    cabecalho.extend_from_slice(&(bloco as u32).to_be_bytes());
    cabecalho.extend_from_slice(&prefixo);
    cabecalho.extend_from_slice(&[0u8; 4]);
    saida.write_all(&cabecalho)?;

    let cifra = cifra(&chave);
    let mut hash = Sha256::new();
    let mut atual = vec![0u8; bloco];
    let mut proximo = vec![0u8; bloco];
    let mut n_atual = ler_cheio(origem, &mut atual)?;
    let mut numero: u32 = 0;
    let mut feito: u64 = 0;
    loop {
        if cancelado.load(Ordering::Relaxed) {
            return Err(ErroAnexo::Cancelado);
        }
        // O último bloco é o que não enche, ou o cheio depois do qual não há mais nada.
        let (ultimo, n_proximo) = if n_atual < bloco {
            (true, 0)
        } else {
            let n = ler_cheio(origem, &mut proximo)?;
            (n == 0, n)
        };
        let claro = &atual[..n_atual];
        hash.update(claro);
        let cifrado = cifra
            .encrypt(
                &Nonce::<U12>::from(nonce_do_bloco(&prefixo, numero)),
                Payload {
                    msg: claro,
                    aad: &dados_autenticados(&cabecalho, id, numero, ultimo),
                },
            )
            .map_err(|_| io::Error::other(t!("anexo-nao-cifrou")))?;
        saida.write_all(&cifrado)?;
        feito += n_atual as u64;
        progresso(Progresso {
            feito,
            total: tamanho.max(feito),
        });
        if ultimo {
            break;
        }
        numero = numero
            .checked_add(1)
            .ok_or_else(|| io::Error::other(t!("anexo-blocos-demais")))?;
        std::mem::swap(&mut atual, &mut proximo);
        n_atual = n_proximo;
    }
    if cancelado.load(Ordering::Relaxed) {
        return Err(ErroAnexo::Cancelado);
    }
    saida.sync_all()?;
    Ok(Anexo {
        id: id.to_owned(),
        tamanho: feito,
        sha256: hex(&hash.finalize()),
        chave: STANDARD.encode(chave),
        ..Anexo::default()
    })
}

/// Decifra o anexo para `destino`, em fluxo, conferindo cada bloco e, no fim, o tamanho e o
/// SHA-256 da ficha. Quem chama apaga o que `destino` recebeu se der erro (o
/// [`ler_para_arquivo`] faz isso).
pub fn ler(
    pasta: &Path,
    anexo: &Anexo,
    destino: &mut dyn Write,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<(), ErroAnexo> {
    if !id_valido(&anexo.id) {
        return Err(ErroAnexo::Corrompido);
    }
    let mut arquivo = match fs::File::open(caminho(pasta, &anexo.id)) {
        Ok(arquivo) => arquivo,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ErroAnexo::Ausente),
        Err(e) => return Err(e.into()),
    };
    let mut cabecalho = [0u8; CABECALHO];
    if ler_cheio(&mut arquivo, &mut cabecalho)? < CABECALHO {
        return Err(ErroAnexo::Corrompido);
    }
    let bloco =
        u32::from_be_bytes([cabecalho[9], cabecalho[10], cabecalho[11], cabecalho[12]]) as usize;
    if &cabecalho[..8] != MARCA
        || cabecalho[8] != VERSAO_DO_FORMATO
        || bloco == 0
        || bloco > BLOCO_MAXIMO
        || cabecalho[21..] != [0u8; 4]
    {
        return Err(ErroAnexo::Corrompido);
    }
    let chave: [u8; 32] = STANDARD
        .decode(&anexo.chave)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or(ErroAnexo::Corrompido)?;
    let prefixo = &cabecalho[13..21];
    let cifra = cifra(&chave);
    let cheio = bloco + ETIQUETA;
    let mut hash = Sha256::new();
    let mut atual = vec![0u8; cheio];
    let mut proximo = vec![0u8; cheio];
    let mut n_atual = ler_cheio(&mut arquivo, &mut atual)?;
    let mut numero: u32 = 0;
    let mut feito: u64 = 0;
    loop {
        if cancelado.load(Ordering::Relaxed) {
            return Err(ErroAnexo::Cancelado);
        }
        if n_atual < ETIQUETA {
            return Err(ErroAnexo::Corrompido);
        }
        let (ultimo, n_proximo) = if n_atual < cheio {
            (true, 0)
        } else {
            let n = ler_cheio(&mut arquivo, &mut proximo)?;
            (n == 0, n)
        };
        let claro = cifra
            .decrypt(
                &Nonce::<U12>::from(nonce_do_bloco(prefixo, numero)),
                Payload {
                    msg: &atual[..n_atual],
                    aad: &dados_autenticados(&cabecalho, &anexo.id, numero, ultimo),
                },
            )
            .map_err(|_| ErroAnexo::Corrompido)?;
        hash.update(&claro);
        destino.write_all(&claro)?;
        feito += claro.len() as u64;
        progresso(Progresso {
            feito,
            total: anexo.tamanho.max(feito),
        });
        if ultimo {
            break;
        }
        numero = numero.checked_add(1).ok_or(ErroAnexo::Corrompido)?;
        std::mem::swap(&mut atual, &mut proximo);
        n_atual = n_proximo;
    }
    if feito != anexo.tamanho || hex(&hash.finalize()) != anexo.sha256 {
        return Err(ErroAnexo::Corrompido);
    }
    Ok(())
}

/// Decifra o anexo para o arquivo `destino`, passando por `<destino>.tmp`: um erro ou o
/// cancelamento não deixam o destino pela metade nem o `.tmp`.
pub fn ler_para_arquivo(
    pasta: &Path,
    anexo: &Anexo,
    destino: &Path,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<(), ErroAnexo> {
    let mut nome = destino.as_os_str().to_owned();
    nome.push(".tmp");
    let tmp = PathBuf::from(nome);
    let _ = fs::remove_file(&tmp);
    let mut saida = io::BufWriter::new(fs::File::create(&tmp)?);
    let resultado = ler(pasta, anexo, &mut saida, progresso, cancelado)
        .and_then(|()| saida.flush().map_err(Into::into))
        .and_then(|()| {
            drop(saida);
            fs::rename(&tmp, destino).map_err(Into::into)
        });
    if resultado.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    resultado
}

/// Decifra o anexo para a memória (para uma imagem ou o "Copiar o conteúdo").
pub fn ler_para_memoria(
    pasta: &Path,
    anexo: &Anexo,
    cancelado: &AtomicBool,
) -> Result<Vec<u8>, ErroAnexo> {
    let mut bytes = Vec::with_capacity(usize::try_from(anexo.tamanho).unwrap_or(0).min(BLOCO * 64));
    ler(pasta, anexo, &mut bytes, &|_| {}, cancelado)?;
    Ok(bytes)
}

/// Os arquivos sem ficha que a coleta anotou: o id e os segundos (desde 1970) em que foi visto
/// pela primeira vez.
type Registro = BTreeMap<String, u64>;

fn ler_registro(caminho: &Path) -> Registro {
    fs::read(caminho)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn gravar_registro(caminho: &Path, registro: &Registro) {
    if registro.is_empty() {
        let _ = fs::remove_file(caminho);
        return;
    }
    let Ok(bytes) = serde_json::to_vec_pretty(registro) else {
        return;
    };
    let mut nome = caminho.as_os_str().to_owned();
    nome.push(".tmp");
    let tmp = PathBuf::from(nome);
    if fs::write(&tmp, bytes).is_ok() && fs::rename(&tmp, caminho).is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Apaga o que sobrou em `pasta` (`vault/`): os `.tmp` com mais de uma hora e os arquivos que
/// nenhuma ficha cita há mais de `margem` segundos. O primeiro olhar numa sobra só a anota em
/// `registro` (fora do cofre, nunca sincronizado). Devolve quantos arquivos apagou. Falhas de
/// disco são ignoradas: a próxima coleta tenta de novo.
pub fn coletar(
    pasta: &Path,
    referenciados: &HashSet<String>,
    registro: &Path,
    agora: u64,
    margem: u64,
) -> usize {
    let Ok(lista) = fs::read_dir(pasta) else {
        return 0;
    };
    let antes = ler_registro(registro);
    let mut depois = Registro::new();
    let mut apagados = 0;
    for entrada in lista.flatten() {
        let Ok(meta) = entrada.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if let Some(base) = nome.strip_suffix(".tmp") {
            if !id_valido(base) {
                continue;
            }
            let modificado = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map_or(agora, |d| d.as_secs());
            if agora.saturating_sub(modificado) > VIDA_DO_TMP
                && fs::remove_file(entrada.path()).is_ok()
            {
                apagados += 1;
            }
            continue;
        }
        if !id_valido(&nome) || referenciados.contains(&nome) {
            continue;
        }
        let visto = antes.get(&nome).copied().unwrap_or(agora);
        if agora.saturating_sub(visto) >= margem {
            if fs::remove_file(entrada.path()).is_ok() {
                apagados += 1;
            } else {
                depois.insert(nome, visto);
            }
        } else {
            depois.insert(nome, visto);
        }
    }
    if depois != antes {
        gravar_registro(registro, &depois);
    }
    apagados
}

/// O que [`copiar_pasta`] fez.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Copia {
    /// Quantos arquivos foram copiados.
    pub copiados: usize,
    /// Quantos já estavam no destino (o mesmo `id` é sempre o mesmo arquivo) e foram pulados.
    pub pulados: usize,
}

/// Copia para `destino` os arquivos de `origem` (um `vault/`) cujo `id` ainda não existe lá, como
/// estão (cifrados). Cada um vai para um `<id>.tmp` e só então é renomeado: um erro ou o
/// cancelamento não deixam arquivo pela metade. `origem` ausente não é erro: não há o que copiar.
/// O `progresso` conta os bytes de tudo o que falta copiar.
pub fn copiar_pasta(
    origem: &Path,
    destino: &Path,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Copia, ErroAnexo> {
    let lista = match fs::read_dir(origem) {
        Ok(lista) => lista,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Copia::default()),
        Err(e) => return Err(e.into()),
    };
    let mut copia = Copia::default();
    let mut a_copiar = Vec::new();
    let mut total = 0u64;
    for entrada in lista.flatten() {
        let Ok(meta) = entrada.metadata() else {
            continue;
        };
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if !meta.is_file() || !id_valido(&nome) {
            continue;
        }
        if existe(destino, &nome) {
            copia.pulados += 1;
        } else {
            total += meta.len();
            a_copiar.push(nome);
        }
    }
    if a_copiar.is_empty() {
        return Ok(copia);
    }
    conferir_espaco_em(destino, total)?;
    criar_pasta(destino)?;
    let mut feito = 0u64;
    progresso(Progresso { feito, total });
    let mut buf = vec![0u8; BLOCO];
    for nome in a_copiar {
        if cancelado.load(Ordering::Relaxed) {
            return Err(ErroAnexo::Cancelado);
        }
        let tmp = destino.join(format!("{nome}.tmp"));
        let _ = fs::remove_file(&tmp);
        let resultado = copiar_arquivo(
            &caminho(origem, &nome),
            &tmp,
            &mut buf,
            &mut feito,
            total,
            progresso,
            cancelado,
        )
        .and_then(|()| fs::rename(&tmp, caminho(destino, &nome)).map_err(Into::into));
        if let Err(erro) = resultado {
            let _ = fs::remove_file(&tmp);
            return Err(erro);
        }
        copia.copiados += 1;
    }
    Ok(copia)
}

fn copiar_arquivo(
    de: &Path,
    para: &Path,
    buf: &mut [u8],
    feito: &mut u64,
    total: u64,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<(), ErroAnexo> {
    let mut entrada = fs::File::open(de)?;
    let mut saida = criar_arquivo(para)?;
    loop {
        if cancelado.load(Ordering::Relaxed) {
            return Err(ErroAnexo::Cancelado);
        }
        let lidos = ler_cheio(&mut entrada, buf)?;
        if lidos == 0 {
            break;
        }
        saida.write_all(&buf[..lidos])?;
        *feito += lidos as u64;
        progresso(Progresso {
            feito: *feito,
            total,
        });
    }
    saida.flush()?;
    Ok(())
}

/// Quantos dos `ids` não têm arquivo em `pasta`.
pub fn faltam(pasta: &Path, ids: &HashSet<String>) -> usize {
    ids.iter().filter(|id| !existe(pasta, id)).count()
}

/// Se `pasta` tem ao menos um arquivo de anexo.
pub fn tem_anexos(pasta: &Path) -> bool {
    fs::read_dir(pasta).is_ok_and(|lista| {
        lista.flatten().any(|e| {
            e.metadata().is_ok_and(|m| m.is_file()) && id_valido(&e.file_name().to_string_lossy())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use std::io::Cursor;
    use std::sync::atomic::AtomicU64;

    fn sem_cancelar() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn bytes_de(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 7 + 3) as u8).collect()
    }

    fn gravar_bytes(pasta: &Path, id: &str, claro: &[u8], bloco: usize) -> Anexo {
        gravar(
            pasta,
            id,
            &mut Cursor::new(claro.to_vec()),
            claro.len() as u64,
            bloco,
            &|_| {},
            &sem_cancelar(),
        )
        .unwrap()
    }

    fn ler_bytes(pasta: &Path, anexo: &Anexo) -> Result<Vec<u8>, ErroAnexo> {
        ler_para_memoria(pasta, anexo, &sem_cancelar())
    }

    fn arquivo_do(pasta: &Path, id: &str) -> Vec<u8> {
        fs::read(pasta.join(id)).unwrap()
    }

    fn corrompido<T: fmt::Debug>(r: Result<T, ErroAnexo>) {
        assert!(matches!(r, Err(ErroAnexo::Corrompido)), "{r:?}");
    }

    #[test]
    fn ida_e_volta_em_varios_tamanhos() {
        let pasta = PastaTemporaria::nova("anexo-ida");
        for n in [0usize, 1, 15, 16, 32, 33, 1000] {
            let claro = bytes_de(n);
            let id = format!("aa{n}");
            let anexo = gravar_bytes(&pasta.0, &id, &claro, 16);
            assert_eq!(anexo.tamanho, n as u64, "{n}");
            assert_eq!(anexo.sha256, hex(&Sha256::digest(&claro)), "{n}");
            assert_eq!(ler_bytes(&pasta.0, &anexo).unwrap(), claro, "{n}");
            assert_eq!(
                arquivo_do(&pasta.0, &id).len() as u64,
                tamanho_cifrado(n as u64, 16),
                "{n}"
            );
            assert!(!pasta.0.join(format!("{id}.tmp")).exists());
        }
    }

    #[test]
    fn o_arquivo_nao_contem_o_claro() {
        let pasta = PastaTemporaria::nova("anexo-claro");
        let claro = b"abacaxi-de-teste abacaxi-de-teste abacaxi-de-teste".to_vec();
        gravar_bytes(&pasta.0, "ab01", &claro, 16);
        let cifrado = arquivo_do(&pasta.0, "ab01");
        assert!(!cifrado.windows(8).any(|w| w == b"abacaxi-"));
    }

    #[cfg(unix)]
    #[test]
    fn pasta_e_arquivo_so_do_dono() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = PastaTemporaria::nova("anexo-perm");
        let vault = pasta.0.join("vault");
        gravar_bytes(&vault, "ab01", b"x", 16);
        let modo = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo(&vault), 0o700);
        assert_eq!(modo(&vault.join("ab01")), 0o600);
    }

    #[test]
    fn tres_mib_e_um_byte_com_o_bloco_real() {
        let pasta = PastaTemporaria::nova("anexo-real");
        let claro = bytes_de(3 * BLOCO + 1);
        let anexo = gravar_bytes(&pasta.0, "cc01", &claro, BLOCO);
        assert_eq!(
            arquivo_do(&pasta.0, "cc01").len() as u64,
            tamanho_cifrado(claro.len() as u64, BLOCO)
        );
        assert_eq!(ler_bytes(&pasta.0, &anexo).unwrap(), claro);
    }

    #[test]
    fn multiplo_exato_nao_ganha_bloco_vazio() {
        let pasta = PastaTemporaria::nova("anexo-exato");
        gravar_bytes(&pasta.0, "dd01", &bytes_de(32), 16);
        // cabeçalho + dois blocos cheios (16 + 16 de etiqueta cada).
        assert_eq!(arquivo_do(&pasta.0, "dd01").len(), 25 + 2 * 32);
        gravar_bytes(&pasta.0, "dd02", &[], 16);
        assert_eq!(arquivo_do(&pasta.0, "dd02").len(), 25 + 16);
    }

    #[test]
    fn valores_de_tamanho_cifrado() {
        assert_eq!(tamanho_cifrado(0, 16), 41);
        assert_eq!(tamanho_cifrado(16, 16), 25 + 16 + 16);
        assert_eq!(tamanho_cifrado(17, 16), 25 + 17 + 32);
    }

    /// Grava 40 bytes (blocos 16, 16 e 8), dá a `mexer` o arquivo e confere que a leitura recusa.
    fn recusa(nome: &str, mexer: impl FnOnce(&mut Vec<u8>)) {
        let pasta = PastaTemporaria::nova(nome);
        let anexo = gravar_bytes(&pasta.0, "ee01", &bytes_de(40), 16);
        let mut bytes = arquivo_do(&pasta.0, "ee01");
        mexer(&mut bytes);
        fs::write(pasta.0.join("ee01"), bytes).unwrap();
        corrompido(ler_bytes(&pasta.0, &anexo));
    }

    #[test]
    fn blocos_trocados_de_lugar() {
        recusa("anexo-troca", |b| {
            let (a, c) = (25..25 + 32, 25 + 32..25 + 64);
            let primeiro = b[a.clone()].to_vec();
            let segundo = b[c.clone()].to_vec();
            b[a].copy_from_slice(&segundo);
            b[c].copy_from_slice(&primeiro);
        });
    }

    #[test]
    fn ultimo_bloco_cortado() {
        recusa("anexo-corte", |b| b.truncate(b.len() - 5));
        recusa("anexo-sem-ultimo", |b| b.truncate(25 + 64));
    }

    #[test]
    fn cortado_no_fim_exato_de_um_bloco_cheio() {
        recusa("anexo-fim-cheio", |b| b.truncate(25 + 32));
    }

    #[test]
    fn bytes_a_mais_no_fim() {
        recusa("anexo-sobra", |b| b.extend_from_slice(&[1, 2, 3]));
        recusa("anexo-sobra-cheia", |b| b.extend_from_slice(&[0u8; 32]));
    }

    #[test]
    fn um_bit_trocado_no_meio() {
        recusa("anexo-bit", |b| {
            let meio = b.len() / 2;
            b[meio] ^= 1;
        });
    }

    #[test]
    fn cabecalho_mexido() {
        for (i, nome) in [
            (0usize, "marca"),
            (8, "versao"),
            (11, "bloco"),
            (14, "prefixo"),
            (22, "reservado"),
        ] {
            recusa(&format!("anexo-cab-{nome}"), |b| b[i] ^= 1);
        }
    }

    #[test]
    fn arquivo_menor_que_o_cabecalho() {
        recusa("anexo-curto", |b| b.truncate(10));
        recusa("anexo-so-cabecalho", |b| b.truncate(25));
        recusa("anexo-vazio", |b| b.clear());
    }

    #[test]
    fn bloco_de_outro_anexo_e_arquivo_renomeado() {
        let pasta = PastaTemporaria::nova("anexo-outro");
        let a = gravar_bytes(&pasta.0, "ff01", &bytes_de(40), 16);
        let b = gravar_bytes(&pasta.0, "ff02", &bytes_de(40), 16);
        // Um bloco do ff02 dentro do ff01.
        let mut bytes = arquivo_do(&pasta.0, "ff01");
        let de_b = arquivo_do(&pasta.0, "ff02");
        bytes[25..57].copy_from_slice(&de_b[25..57]);
        fs::write(pasta.0.join("ff01"), bytes).unwrap();
        corrompido(ler_bytes(&pasta.0, &a));
        // O arquivo inteiro do ff02 com o nome do ff01, lido com a ficha do ff01 e do ff02.
        fs::write(pasta.0.join("ff01"), &de_b).unwrap();
        corrompido(ler_bytes(&pasta.0, &a));
        let b_no_lugar = Anexo {
            id: "ff01".to_owned(),
            ..b
        };
        corrompido(ler_bytes(&pasta.0, &b_no_lugar));
    }

    #[test]
    fn ficha_com_sha_ou_tamanho_errados() {
        let pasta = PastaTemporaria::nova("anexo-ficha");
        let anexo = gravar_bytes(&pasta.0, "ab02", &bytes_de(40), 16);
        corrompido(ler_bytes(
            &pasta.0,
            &Anexo {
                sha256: hex(&Sha256::digest(b"outro")),
                ..anexo.clone()
            },
        ));
        corrompido(ler_bytes(
            &pasta.0,
            &Anexo {
                tamanho: 41,
                ..anexo.clone()
            },
        ));
        corrompido(ler_bytes(
            &pasta.0,
            &Anexo {
                chave: STANDARD.encode([9u8; 32]),
                ..anexo.clone()
            },
        ));
        corrompido(ler_bytes(
            &pasta.0,
            &Anexo {
                chave: "não é base64".to_owned(),
                ..anexo
            },
        ));
    }

    #[test]
    fn arquivo_inexistente_e_ausente() {
        let pasta = PastaTemporaria::nova("anexo-ausente");
        let anexo = Anexo {
            id: "ab03".to_owned(),
            ..Anexo::default()
        };
        assert!(matches!(
            ler_bytes(&pasta.0, &anexo),
            Err(ErroAnexo::Ausente)
        ));
        assert!(!existe(&pasta.0, "ab03"));
    }

    #[test]
    fn id_que_escapa_da_pasta_e_recusado() {
        let pasta = PastaTemporaria::nova("anexo-id");
        let r = gravar(
            &pasta.0,
            "../fora",
            &mut Cursor::new(vec![1]),
            1,
            16,
            &|_| {},
            &sem_cancelar(),
        );
        assert!(matches!(r, Err(ErroAnexo::Io(_))));
        let anexo = Anexo {
            id: "../fora".to_owned(),
            ..Anexo::default()
        };
        corrompido(ler_bytes(&pasta.0, &anexo));
    }

    #[test]
    fn cancelar_na_gravacao_nao_deixa_nada() {
        let pasta = PastaTemporaria::nova("anexo-cancela");
        let cancelado = AtomicBool::new(false);
        let blocos = AtomicU64::new(0);
        let claro = bytes_de(100);
        let r = gravar(
            &pasta.0,
            "ab04",
            &mut Cursor::new(claro),
            100,
            16,
            &|_| {
                if blocos.fetch_add(1, Ordering::Relaxed) == 0 {
                    cancelado.store(true, Ordering::Relaxed);
                }
            },
            &cancelado,
        );
        assert!(matches!(r, Err(ErroAnexo::Cancelado)));
        assert_eq!(blocos.load(Ordering::Relaxed), 1);
        assert!(!pasta.0.join("ab04").exists());
        assert!(!pasta.0.join("ab04.tmp").exists());
    }

    #[test]
    fn cancelar_a_leitura_para_arquivo_nao_deixa_nada() {
        let pasta = PastaTemporaria::nova("anexo-cancela-ler");
        let anexo = gravar_bytes(&pasta.0, "ab05", &bytes_de(100), 16);
        let destino = pasta.0.join("saida.bin");
        let cancelado = AtomicBool::new(false);
        let r = ler_para_arquivo(
            &pasta.0,
            &anexo,
            &destino,
            &|p| {
                if p.feito >= 32 {
                    cancelado.store(true, Ordering::Relaxed);
                }
            },
            &cancelado,
        );
        assert!(matches!(r, Err(ErroAnexo::Cancelado)));
        assert!(!destino.exists());
        assert!(!pasta.0.join("saida.bin.tmp").exists());
    }

    #[test]
    fn ler_para_arquivo_grava_os_mesmos_bytes_e_um_erro_nao_deixa_pela_metade() {
        let pasta = PastaTemporaria::nova("anexo-arquivo");
        let claro = bytes_de(100);
        let anexo = gravar_bytes(&pasta.0, "ab06", &claro, 16);
        let destino = pasta.0.join("saida.bin");
        ler_para_arquivo(&pasta.0, &anexo, &destino, &|_| {}, &sem_cancelar()).unwrap();
        assert_eq!(fs::read(&destino).unwrap(), claro);
        // Um arquivo estragado no meio: o destino novo não aparece.
        let mut bytes = arquivo_do(&pasta.0, "ab06");
        let n = bytes.len();
        bytes[n - 40] ^= 1;
        fs::write(pasta.0.join("ab06"), bytes).unwrap();
        let outro = pasta.0.join("outro.bin");
        let r = ler_para_arquivo(&pasta.0, &anexo, &outro, &|_| {}, &sem_cancelar());
        assert!(matches!(r, Err(ErroAnexo::Corrompido)));
        assert!(!outro.exists());
        assert!(!pasta.0.join("outro.bin.tmp").exists());
    }

    #[test]
    fn o_progresso_chega_ao_total() {
        let pasta = PastaTemporaria::nova("anexo-progresso");
        let ultimo = std::cell::Cell::new(Progresso::default());
        let claro = bytes_de(100);
        let anexo = gravar(
            &pasta.0,
            "ab07",
            &mut Cursor::new(claro),
            100,
            16,
            &|p| ultimo.set(p),
            &sem_cancelar(),
        )
        .unwrap();
        assert_eq!(
            ultimo.get(),
            Progresso {
                feito: 100,
                total: 100
            }
        );
        ultimo.set(Progresso::default());
        ler(
            &pasta.0,
            &anexo,
            &mut Vec::new(),
            &|p| ultimo.set(p),
            &sem_cancelar(),
        )
        .unwrap();
        assert_eq!(
            ultimo.get(),
            Progresso {
                feito: 100,
                total: 100
            }
        );
    }

    #[test]
    fn espaco_confere() {
        assert!(matches!(
            conferir_espaco(10, 9),
            Err(ErroAnexo::SemEspaco {
                precisa: 10,
                livre: 9
            })
        ));
        assert!(conferir_espaco(10, 10).is_ok());
        let msg = conferir_espaco(2048, 1024).unwrap_err().to_string();
        assert_eq!(
            msg,
            "Não há espaço no disco: o anexo precisa de 2,0 KB e há 1,0 KB livres."
        );
    }

    #[test]
    fn tipos_pelo_nome() {
        assert_eq!(tipo_do_nome("a.PNG"), "image/png");
        assert_eq!(tipo_do_nome("id_rsa.ppk"), "text/plain");
        assert_eq!(tipo_do_nome("sem-extensao"), "application/octet-stream");
        assert_eq!(tipo_do_nome("x.desconhecido"), "application/octet-stream");
    }

    fn criar(pasta: &Path, nome: &str) {
        fs::create_dir_all(pasta).unwrap();
        fs::write(pasta.join(nome), b"x").unwrap();
    }

    fn agora_real() -> u64 {
        crate::recentes::agora()
    }

    #[test]
    fn coletar_espera_a_margem_antes_de_apagar() {
        let dados = PastaTemporaria::nova("coleta-margem");
        let vault = dados.0.join("vault");
        let registro = dados.0.join(REGISTRO_DE_ORFAOS);
        criar(&vault, "ab10");
        criar(&vault, "ab11");
        let dono: HashSet<String> = ["ab11".to_owned()].into();
        let t0 = agora_real() + 10;
        // Primeira vez: só anota.
        assert_eq!(coletar(&vault, &dono, &registro, t0, 100), 0);
        assert!(vault.join("ab10").exists());
        let anotado = ler_registro(&registro);
        assert_eq!(anotado.get("ab10"), Some(&t0));
        assert!(!anotado.contains_key("ab11"));
        // Antes da margem, continua.
        assert_eq!(coletar(&vault, &dono, &registro, t0 + 99, 100), 0);
        assert!(vault.join("ab10").exists());
        // Depois, some, e sai do registro.
        assert_eq!(coletar(&vault, &dono, &registro, t0 + 100, 100), 1);
        assert!(!vault.join("ab10").exists());
        assert!(vault.join("ab11").exists());
        assert!(!registro.exists());
    }

    #[test]
    fn orfao_que_volta_a_ter_dono_sai_do_registro() {
        let dados = PastaTemporaria::nova("coleta-volta");
        let vault = dados.0.join("vault");
        let registro = dados.0.join(REGISTRO_DE_ORFAOS);
        criar(&vault, "ab20");
        let t0 = agora_real() + 10;
        coletar(&vault, &HashSet::new(), &registro, t0, 100);
        assert!(ler_registro(&registro).contains_key("ab20"));
        let dono: HashSet<String> = ["ab20".to_owned()].into();
        assert_eq!(coletar(&vault, &dono, &registro, t0 + 50, 100), 0);
        assert!(ler_registro(&registro).is_empty());
        // Perdeu o dono de novo: a contagem recomeça.
        coletar(&vault, &HashSet::new(), &registro, t0 + 60, 100);
        assert_eq!(
            coletar(&vault, &HashSet::new(), &registro, t0 + 150, 100),
            0
        );
        assert!(vault.join("ab20").exists());
    }

    #[test]
    fn arquivo_antigo_sem_dono_visto_agora_espera_a_margem() {
        let dados = PastaTemporaria::nova("coleta-antigo");
        let vault = dados.0.join("vault");
        let registro = dados.0.join(REGISTRO_DE_ORFAOS);
        criar(&vault, "ab30");
        let velho = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86_400);
        fs::File::options()
            .write(true)
            .open(vault.join("ab30"))
            .unwrap()
            .set_modified(velho)
            .unwrap();
        let agora = agora_real();
        assert_eq!(
            coletar(&vault, &HashSet::new(), &registro, agora, MARGEM_ORFAO),
            0
        );
        assert!(vault.join("ab30").exists());
    }

    #[test]
    fn tmp_velho_some_e_novo_fica() {
        let dados = PastaTemporaria::nova("coleta-tmp");
        let vault = dados.0.join("vault");
        let registro = dados.0.join(REGISTRO_DE_ORFAOS);
        criar(&vault, "ab40.tmp");
        criar(&vault, "ab41.tmp");
        let velho = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
        fs::File::options()
            .write(true)
            .open(vault.join("ab40.tmp"))
            .unwrap()
            .set_modified(velho)
            .unwrap();
        assert_eq!(
            coletar(
                &vault,
                &HashSet::new(),
                &registro,
                agora_real(),
                MARGEM_ORFAO
            ),
            1
        );
        assert!(!vault.join("ab40.tmp").exists());
        assert!(vault.join("ab41.tmp").exists());
    }

    #[test]
    fn registro_ilegivel_vale_como_vazio_e_referenciados_nunca_somem() {
        let dados = PastaTemporaria::nova("coleta-registro");
        let vault = dados.0.join("vault");
        let registro = dados.0.join(REGISTRO_DE_ORFAOS);
        criar(&vault, "ab50");
        criar(&vault, "ab51");
        fs::write(&registro, b"isto nao e json").unwrap();
        let dono: HashSet<String> = ["ab50".to_owned()].into();
        let t0 = agora_real();
        // Mesmo com margem zero e muito tempo depois, o referenciado fica.
        assert_eq!(coletar(&vault, &dono, &registro, t0 + 1_000_000, 0), 1);
        assert!(vault.join("ab50").exists());
        assert!(!vault.join("ab51").exists());
        // Pasta que não existe: nada a fazer.
        assert_eq!(coletar(&dados.0.join("nada"), &dono, &registro, t0, 0), 0);
    }

    #[test]
    fn copiar_pasta_leva_so_o_que_falta_e_nao_deixa_tmp() {
        let dados = PastaTemporaria::nova("copiar-pasta");
        let (de, para) = (dados.0.join("de"), dados.0.join("para"));
        let a = gravar_bytes(&de, "ab60", &bytes_de(3000), 1024);
        gravar_bytes(&de, "ab61", &bytes_de(10), 1024);
        fs::write(de.join("ab62.tmp"), b"pela metade").unwrap();
        fs::create_dir_all(de.join("subpasta")).unwrap();
        gravar_bytes(&para, "ab61", &bytes_de(10), 1024);

        let vistos = std::cell::RefCell::new(Vec::new());
        let r = copiar_pasta(
            &de,
            &para,
            &|p| vistos.borrow_mut().push(p),
            &sem_cancelar(),
        )
        .unwrap();
        assert_eq!(
            r,
            Copia {
                copiados: 1,
                pulados: 1
            }
        );
        assert_eq!(arquivo_do(&para, "ab60"), arquivo_do(&de, "ab60"));
        assert_eq!(ler_bytes(&para, &a).unwrap(), bytes_de(3000));
        assert!(!para.join("ab62.tmp").exists(), "o .tmp não é copiado");
        assert!(!para.join("ab60.tmp").exists());
        let vistos = vistos.into_inner();
        assert_eq!(vistos.last().unwrap().feito, vistos.last().unwrap().total);
        assert_eq!(
            vistos.last().unwrap().total,
            arquivo_do(&de, "ab60").len() as u64
        );

        // De novo: tudo já está lá.
        let r = copiar_pasta(&de, &para, &|_| {}, &sem_cancelar()).unwrap();
        assert_eq!(
            r,
            Copia {
                copiados: 0,
                pulados: 2
            }
        );
        // Origem que não existe: nada a copiar.
        let r = copiar_pasta(&dados.0.join("nada"), &para, &|_| {}, &sem_cancelar()).unwrap();
        assert_eq!(r, Copia::default());
    }

    #[test]
    fn copiar_pasta_cancelada_nao_deixa_arquivo_pela_metade() {
        let dados = PastaTemporaria::nova("copiar-cancela");
        let (de, para) = (dados.0.join("de"), dados.0.join("para"));
        gravar_bytes(&de, "ab70", &bytes_de(5000), 1024);
        let cancelar = AtomicBool::new(false);
        let r = copiar_pasta(
            &de,
            &para,
            &|p| {
                if p.feito > 0 {
                    cancelar.store(true, Ordering::Relaxed);
                }
            },
            &cancelar,
        );
        assert!(matches!(r, Err(ErroAnexo::Cancelado)), "{r:?}");
        let sobrou: Vec<_> = fs::read_dir(&para).unwrap().flatten().collect();
        assert!(sobrou.is_empty(), "sobrou {sobrou:?}");
    }

    #[test]
    fn faltam_e_tem_anexos() {
        let dados = PastaTemporaria::nova("copiar-faltam");
        let vault = dados.0.join("vault");
        assert!(!tem_anexos(&vault));
        gravar_bytes(&vault, "ab80", &bytes_de(4), 16);
        fs::write(vault.join("ab81.tmp"), b"x").unwrap();
        assert!(tem_anexos(&vault));
        let ids: HashSet<String> = ["ab80", "ab81", "ab82"].map(str::to_owned).into();
        assert_eq!(faltam(&vault, &ids), 2);
    }
}
