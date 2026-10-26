//! Os logs de sessão cifrados (M53): o formato `.log.cifrado`, o escritor que a sessão do
//! terminal usa no lugar do arquivo de texto e a leitura de volta.
//!
//! O módulo é puro: não conhece a tela nem o cofre, e recebe as chaves por parâmetro. A chave é
//! um par X25519 só dos logs: gravar usa só a **pública** (então o cofre trancado não atrapalha),
//! e ler usa a **privada**, que mora num item do cofre.
//!
//! ```text
//! arquivo     = segmento+
//! segmento    = cabeçalho · registro*
//! cabeçalho   = "TRCHLOGS" (8) · versão (1, hoje 1) · id da chave (8) · pública efêmera (32)
//!                                                                    = 49 bytes
//! registro n  = tamanho (u32 BE, do cifrado com a etiqueta) · tipo (1) · cifrado
//! tipo        = 0 dados, 1 fim
//! chave       = HKDF-SHA256(sal = efêmera ‖ pública dos logs,
//!                           ikm = X25519(efêmera privada, pública dos logs),
//!                           info = "tirech-ssh log 1") → 32 bytes
//! nonce       = 4 zeros · n (u64 BE)      (n conta de 0 dentro do segmento)
//! autenticado = cabeçalho · tipo · n (u64 BE)
//! cifrado     = AES-256-GCM(chave, nonce, claro, autenticado), com a etiqueta de 16 bytes
//! ```
//!
//! - O **id da chave** são os 8 primeiros bytes do SHA-256 da pública dos logs: na leitura, aponta
//!   a privada certa.
//! - Cada segmento tem a sua chave (a efêmera é nova), então o nonce por contador não se repete.
//!   Um arquivo tem mais de um segmento quando um `log_file` fixo recebe uma sessão nova.
//! - O tipo e o número entram nos dados autenticados: trocar, repetir ou reordenar registros
//!   quebra a etiqueta. Cortar registros inteiros do fim só se percebe pela falta do `fim`.
//! - Um segmento de `tamanho` maior que [`TAMANHO_MAXIMO`] não existe: o escritor nunca passa de
//!   [`REGISTRO`] mais a etiqueta. Isso também separa, sem ambiguidade, um registro de um
//!   cabeçalho novo (`"TRCH"` lido como tamanho dá mais de 1 GiB).

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use aes_gcm::aead::consts::U12;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use curve25519_dalek::montgomery::MontgomeryPoint;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::t;

/// Quanto o escritor acumula antes de selar um registro: 64 KiB. É do formato, não um limite.
pub const REGISTRO: usize = 64 * 1024;

/// O maior `tamanho` de registro que a leitura aceita: um arquivo mexido não pode pedir memória
/// sem fim.
pub const TAMANHO_MAXIMO: usize = 64 * 1024 * 1024;

/// O tamanho do cabeçalho de um segmento.
pub const CABECALHO: usize = 49;

const MARCA: &[u8; 8] = b"TRCHLOGS";
const VERSAO_DO_FORMATO: u8 = 1;
// Identificador de formato: não muda com o nome do app (está gravado nos arquivos de quem usa).
// Entra no HKDF: mudar faz os logs cifrados antes não abrirem mais.
pub(crate) const INFO: &[u8] = b"tirech-ssh log 1";
const ETIQUETA: usize = 16;
/// `tamanho` (4) mais `tipo` (1).
const TOPO_DO_REGISTRO: usize = 5;
const DADOS: u8 = 0;
const FIM: u8 = 1;

/// Um par de chaves dos logs: a privada (32 bytes) e a pública (32 bytes).
pub type ParDeChaves = ([u8; 32], [u8; 32]);

/// Um par X25519 novo, para a chave dos logs.
pub fn par_novo() -> io::Result<ParDeChaves> {
    let privada = aleatorio::<32>()?;
    Ok((privada, publica_da(&privada)))
}

/// A pública que corresponde a uma privada.
pub fn publica_da(privada: &[u8; 32]) -> [u8; 32] {
    MontgomeryPoint::mul_base_clamped(*privada).0
}

/// O id de uma pública: os 8 primeiros bytes do SHA-256 dela.
pub fn id_da_chave(publica: &[u8; 32]) -> [u8; 8] {
    let resumo = Sha256::digest(publica);
    let mut id = [0u8; 8];
    id.copy_from_slice(&resumo[..8]);
    id
}

/// O arquivo, na pasta de dados, com a chave pública dos logs. Gravar um log cifrado só precisa
/// dele; a privada fica no cofre.
pub const ARQUIVO_PUBLICO: &str = "logs-cifrados.pub";

/// O que o `logs-cifrados.pub` guarda.
#[derive(Serialize, Deserialize)]
struct PublicaNoDisco {
    versao: u32,
    publica: String,
    id: String,
    uid: String,
}

/// A chave pública dos logs como o `logs-cifrados.pub` a traz.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicaDosLogs {
    /// A pública X25519.
    pub publica: [u8; 32],
    /// O `uid` do item do cofre de onde ela veio.
    pub uid: String,
}

/// Lê o `logs-cifrados.pub` de `pasta_de_dados`. Ausente, ilegível ou com o `id` que não bate com
/// a pública (um arquivo mexido) conta como ausente.
pub fn ler_publica(pasta_de_dados: &Path) -> Option<PublicaDosLogs> {
    let bytes = fs::read(pasta_de_dados.join(ARQUIVO_PUBLICO)).ok()?;
    let no_disco: PublicaNoDisco = serde_json::from_slice(&bytes).ok()?;
    if no_disco.versao != 1 {
        return None;
    }
    let publica: [u8; 32] = STANDARD.decode(&no_disco.publica).ok()?.try_into().ok()?;
    (id_de_hex(&no_disco.id)? == id_da_chave(&publica)).then_some(PublicaDosLogs {
        publica,
        uid: no_disco.uid,
    })
}

/// Grava o `logs-cifrados.pub` em `pasta_de_dados` (atômico e só do dono, como o `vault.json`).
pub fn gravar_publica(pasta_de_dados: &Path, publica: &[u8; 32], uid: &str) -> io::Result<()> {
    let no_disco = PublicaNoDisco {
        versao: 1,
        publica: STANDARD.encode(publica),
        id: id_em_hex(&id_da_chave(publica)),
        uid: uid.to_owned(),
    };
    let bytes = serde_json::to_vec_pretty(&no_disco).map_err(io::Error::other)?;
    crate::cofre::gravar_arquivo_privado(&pasta_de_dados.join(ARQUIVO_PUBLICO), &bytes)
}

/// Um id em hexadecimal minúsculo (16 caracteres).
pub fn id_em_hex(id: &[u8; 8]) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

/// Um id escrito em hexadecimal, de volta aos 8 bytes.
pub fn id_de_hex(texto: &str) -> Option<[u8; 8]> {
    if texto.len() != 16 || !texto.is_ascii() {
        return None;
    }
    let mut id = [0u8; 8];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&texto[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(id)
}

fn aleatorio<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|e| io::Error::other(t!("log-sem-aleatorio", erro = e.to_string())))?;
    Ok(bytes)
}

/// A chave AES do segmento, a partir do segredo X25519 e das duas públicas. `None` se o segredo
/// vier todo zero (um ponto de ordem baixa).
fn chave_do_segmento(
    segredo: &[u8; 32],
    efemera: &[u8; 32],
    publica: &[u8; 32],
) -> Option<Aes256Gcm> {
    if segredo.iter().all(|&b| b == 0) {
        return None;
    }
    let mut sal = [0u8; 64];
    sal[..32].copy_from_slice(efemera);
    sal[32..].copy_from_slice(publica);
    let mut chave = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&sal), segredo)
        .expand(INFO, &mut chave)
        .ok()?;
    Some(Aes256Gcm::new((&chave).into()))
}

fn nonce_do_registro(n: u64) -> Nonce<U12> {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&n.to_be_bytes());
    Nonce::<U12>::from(nonce)
}

fn dados_autenticados(cabecalho: &[u8; CABECALHO], tipo: u8, n: u64) -> [u8; CABECALHO + 9] {
    let mut dados = [0u8; CABECALHO + 9];
    dados[..CABECALHO].copy_from_slice(cabecalho);
    dados[CABECALHO] = tipo;
    dados[CABECALHO + 1..].copy_from_slice(&n.to_be_bytes());
    dados
}

/// Lê até encher `buf` ou acabar o arquivo; devolve quantos bytes vieram.
fn ler_ate(origem: &mut dyn Read, buf: &mut [u8]) -> io::Result<usize> {
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

fn invalido(texto: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, texto)
}

/// Percorre um arquivo de log cifrado só pelos tamanhos, sem chave, e devolve onde acaba o último
/// registro (ou cabeçalho) inteiro. O que vem depois é o resto de uma queda e pode ser cortado.
fn fim_do_ultimo_inteiro(arquivo: &mut fs::File) -> io::Result<u64> {
    arquivo.seek(SeekFrom::Start(0))?;
    let mut r = BufReader::new(arquivo);
    let mut pos = 0u64;
    let mut em_segmento = false;
    loop {
        let mut inicio = [0u8; 8];
        let n = ler_ate(&mut r, &mut inicio)?;
        if n == 0 {
            return Ok(pos);
        }
        if n < 8 {
            // Sobra curta: um cabeçalho ou um registro cortado.
            return if MARCA.starts_with(&inicio[..n]) || em_segmento {
                Ok(pos)
            } else {
                Err(invalido(t!("log-nao-e-do-app")))
            };
        }
        if &inicio == MARCA {
            let mut resto = [0u8; CABECALHO - 8];
            if ler_ate(&mut r, &mut resto)? < resto.len() {
                return Ok(pos);
            }
            pos += CABECALHO as u64;
            em_segmento = true;
        } else if em_segmento {
            let tamanho = u32::from_be_bytes([inicio[0], inicio[1], inicio[2], inicio[3]]) as usize;
            let tipo = inicio[4];
            if !(ETIQUETA..=TAMANHO_MAXIMO).contains(&tamanho) || tipo > FIM {
                return Err(invalido(t!("log-alterado-no-meio")));
            }
            // Três bytes do cifrado já vieram em `inicio`.
            let falta = (tamanho - (8 - TOPO_DO_REGISTRO)) as u64;
            let pulados = io::copy(&mut (&mut r).take(falta), &mut io::sink())?;
            if pulados < falta {
                return Ok(pos);
            }
            pos += (TOPO_DO_REGISTRO + tamanho) as u64;
            if tipo == FIM {
                em_segmento = false;
            }
        } else {
            return Err(invalido(t!("log-nao-e-do-app")));
        }
    }
}

/// Grava um segmento de log cifrado no fim de um arquivo. Implementa [`Write`]: o `write`
/// acumula, o `flush` sela o que houver e o `Drop` sela o resto e grava o registro `fim`.
///
/// Um erro de disco volta pelo `write` e o escritor para: não se grava mais nada depois de uma
/// escrita pela metade, para não deixar um registro quebrado no meio do arquivo.
pub struct EscritorCifrado {
    arquivo: fs::File,
    cifra: Aes256Gcm,
    cabecalho: [u8; CABECALHO],
    n: u64,
    buffer: Vec<u8>,
    quebrado: bool,
}

impl EscritorCifrado {
    /// Abre (ou cria, com permissão só do dono) `caminho` e acrescenta nele um segmento novo,
    /// cifrado para `publica`.
    ///
    /// Se o arquivo já existe, o que sobrou de um registro cortado pela queda anterior é tirado
    /// antes, para o segmento novo nunca ficar colado num registro pela metade. Um arquivo que não
    /// é um log cifrado do app não é tocado, e a abertura falha.
    pub fn abrir(caminho: &Path, publica: &[u8; 32]) -> io::Result<Self> {
        // A chave sai antes de o arquivo ser tocado: uma pública inválida não cria nada.
        let efemera_privada = aleatorio::<32>()?;
        let efemera = publica_da(&efemera_privada);
        let segredo = MontgomeryPoint(*publica).mul_clamped(efemera_privada).0;
        let cifra = chave_do_segmento(&segredo, &efemera, publica)
            .ok_or_else(|| invalido(t!("log-chave-invalida")))?;

        let mut opcoes = fs::OpenOptions::new();
        opcoes.read(true).append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opcoes.mode(0o600);
        }
        let mut arquivo = opcoes.open(caminho)?;
        let tamanho = arquivo.metadata()?.len();
        if tamanho > 0 {
            let inteiro = fim_do_ultimo_inteiro(&mut arquivo)?;
            if inteiro < tamanho {
                arquivo.set_len(inteiro)?;
            }
        }

        let mut cabecalho = [0u8; CABECALHO];
        cabecalho[..8].copy_from_slice(MARCA);
        cabecalho[8] = VERSAO_DO_FORMATO;
        cabecalho[9..17].copy_from_slice(&id_da_chave(publica));
        cabecalho[17..].copy_from_slice(&efemera);
        arquivo.write_all(&cabecalho)?;
        Ok(EscritorCifrado {
            arquivo,
            cifra,
            cabecalho,
            n: 0,
            buffer: Vec::new(),
            quebrado: false,
        })
    }

    /// Sela `claro` num registro do `tipo` dado e o acrescenta ao arquivo, de uma vez só.
    fn selar(&mut self, tipo: u8, claro: &[u8]) -> io::Result<()> {
        let cifrado = self
            .cifra
            .encrypt(
                &nonce_do_registro(self.n),
                Payload {
                    msg: claro,
                    aad: &dados_autenticados(&self.cabecalho, tipo, self.n),
                },
            )
            .map_err(|_| io::Error::other(t!("log-nao-cifrou")))?;
        let mut registro = Vec::with_capacity(TOPO_DO_REGISTRO + cifrado.len());
        registro.extend_from_slice(&(cifrado.len() as u32).to_be_bytes());
        registro.push(tipo);
        registro.extend_from_slice(&cifrado);
        if let Err(e) = self.arquivo.write_all(&registro) {
            self.quebrado = true;
            return Err(e);
        }
        self.n += 1;
        Ok(())
    }

    /// Sela o que está acumulado, se houver.
    fn selar_o_acumulado(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let claro = std::mem::take(&mut self.buffer);
        self.selar(DADOS, &claro)
    }

    fn se_quebrado(&self) -> io::Result<()> {
        if self.quebrado {
            Err(io::Error::other(t!("log-escrita-parou")))
        } else {
            Ok(())
        }
    }
}

impl Write for EscritorCifrado {
    fn write(&mut self, dados: &[u8]) -> io::Result<usize> {
        self.se_quebrado()?;
        self.buffer.extend_from_slice(dados);
        while self.buffer.len() >= REGISTRO {
            let pedaco: Vec<u8> = self.buffer.drain(..REGISTRO).collect();
            self.selar(DADOS, &pedaco)?;
        }
        Ok(dados.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.se_quebrado()?;
        self.selar_o_acumulado()?;
        self.arquivo.sync_data()
    }
}

impl Drop for EscritorCifrado {
    fn drop(&mut self) {
        if self.quebrado {
            return;
        }
        if self.selar_o_acumulado().is_ok() {
            let _ = self.selar(FIM, &[]);
            let _ = self.arquivo.sync_data();
        }
    }
}

/// O que um segmento ou um arquivo tem de estranho, sem ser erro: o texto lido continua valendo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AvisoLog {
    /// O segmento acabou sem o registro `fim`: o app caiu, ou o log ainda está sendo gravado.
    SemFim {
        /// O número do segmento, de 1 em diante.
        segmento: usize,
    },
    /// O último registro (ou cabeçalho) estava pela metade e foi ignorado.
    Cortado {
        /// Quantos bytes ficaram de fora.
        bytes: u64,
    },
}

/// Por que um log não foi lido (inteiro ou até o fim). O texto lido até ali já está em `saida`.
#[derive(Debug)]
pub enum ErroLog {
    /// Não começa com `TRCHLOGS`: não é um log cifrado do app.
    NaoE,
    /// Gravado por uma versão mais nova do formato.
    VersaoNova(u8),
    /// A chave privada do segmento não está no mapa.
    ChaveAusente {
        /// O id da chave, em hexadecimal.
        id: String,
    },
    /// A etiqueta não confere, ou o registro é impossível: o arquivo foi mexido ou estragou.
    Alterado {
        /// O número do segmento, de 1 em diante.
        segmento: usize,
        /// O número do registro dentro do segmento, de 0 em diante.
        registro: u64,
    },
    /// O disco falhou (na origem ou na saída).
    Io(io::Error),
    /// A pessoa cancelou.
    Cancelado,
}

impl fmt::Display for ErroLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErroLog::NaoE => write!(f, "{}", t!("log-erro-nao-e")),
            ErroLog::VersaoNova(_) => write!(f, "{}", t!("log-erro-versao-nova")),
            ErroLog::ChaveAusente { id } => {
                write!(f, "{}", t!("log-erro-chave-ausente", id = id.as_str()))
            }
            ErroLog::Alterado { segmento, registro } => write!(
                f,
                "{}",
                t!(
                    "log-erro-alterado",
                    segmento = *segmento,
                    registro = *registro
                )
            ),
            ErroLog::Io(e) => write!(f, "{}", t!("log-erro-io", erro = e.to_string())),
            ErroLog::Cancelado => write!(f, "{}", t!("log-erro-cancelado")),
        }
    }
}

impl std::error::Error for ErroLog {}

impl From<io::Error> for ErroLog {
    fn from(e: io::Error) -> Self {
        ErroLog::Io(e)
    }
}

/// O que a leitura de um log trouxe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Leitura {
    /// Quantos segmentos o arquivo tem.
    pub segmentos: usize,
    /// Quantos bytes de texto foram para a saída.
    pub bytes: u64,
    /// O que ficou estranho no caminho.
    pub avisos: Vec<AvisoLog>,
}

/// Decifra `origem` (um arquivo de log cifrado) para `saida`, um registro por vez, sem carregar o
/// arquivo inteiro. `chaves` são as privadas dos logs pelo id; `cancelar` interrompe.
///
/// Num erro no meio, o texto lido até ali já está em `saida`.
pub fn ler(
    origem: &mut dyn Read,
    chaves: &HashMap<[u8; 8], [u8; 32]>,
    saida: &mut dyn Write,
    cancelar: &AtomicBool,
) -> Result<Leitura, ErroLog> {
    let mut leitura = Leitura::default();
    // Os bytes de um cabeçalho novo que a leitura do registro anterior já consumiu.
    let mut adiantado: Vec<u8> = Vec::new();
    loop {
        // O cabeçalho do segmento.
        let mut cabecalho = [0u8; CABECALHO];
        cabecalho[..adiantado.len()].copy_from_slice(&adiantado);
        let ja = adiantado.len();
        adiantado.clear();
        let n = ja + ler_ate(origem, &mut cabecalho[ja..])?;
        if n == 0 {
            return Ok(leitura);
        }
        let marca_vista = n.min(8);
        if cabecalho[..marca_vista] != MARCA[..marca_vista] || (n < 8 && leitura.segmentos > 0) {
            return Err(if leitura.segmentos == 0 {
                ErroLog::NaoE
            } else {
                ErroLog::Alterado {
                    segmento: leitura.segmentos + 1,
                    registro: 0,
                }
            });
        }
        if n < CABECALHO {
            if leitura.segmentos == 0 && n < 8 {
                return Err(ErroLog::NaoE);
            }
            leitura.avisos.push(AvisoLog::Cortado { bytes: n as u64 });
            return Ok(leitura);
        }
        let versao = cabecalho[8];
        if versao > VERSAO_DO_FORMATO {
            return Err(ErroLog::VersaoNova(versao));
        }
        let segmento = leitura.segmentos + 1;
        if versao == 0 {
            return Err(ErroLog::Alterado {
                segmento,
                registro: 0,
            });
        }
        let mut id = [0u8; 8];
        id.copy_from_slice(&cabecalho[9..17]);
        let Some(privada) = chaves.get(&id) else {
            return Err(ErroLog::ChaveAusente { id: id_em_hex(&id) });
        };
        let mut efemera = [0u8; 32];
        efemera.copy_from_slice(&cabecalho[17..]);
        let publica = publica_da(privada);
        let segredo = MontgomeryPoint(efemera).mul_clamped(*privada).0;
        let Some(cifra) = chave_do_segmento(&segredo, &efemera, &publica) else {
            return Err(ErroLog::Alterado {
                segmento,
                registro: 0,
            });
        };
        leitura.segmentos = segmento;

        // Os registros.
        let mut numero = 0u64;
        loop {
            if cancelar.load(Ordering::Relaxed) {
                return Err(ErroLog::Cancelado);
            }
            let mut topo = [0u8; TOPO_DO_REGISTRO];
            let lidos = ler_ate(origem, &mut topo)?;
            if lidos == 0 {
                leitura.avisos.push(AvisoLog::SemFim { segmento });
                return Ok(leitura);
            }
            if lidos == TOPO_DO_REGISTRO && &topo[..4] == b"TRCH" {
                // Um segmento novo, colado num que não foi fechado.
                leitura.avisos.push(AvisoLog::SemFim { segmento });
                adiantado.extend_from_slice(&topo);
                break;
            }
            if lidos < TOPO_DO_REGISTRO {
                leitura.avisos.push(AvisoLog::Cortado {
                    bytes: lidos as u64,
                });
                return Ok(leitura);
            }
            let tamanho = u32::from_be_bytes([topo[0], topo[1], topo[2], topo[3]]) as usize;
            let tipo = topo[4];
            if !(ETIQUETA..=TAMANHO_MAXIMO).contains(&tamanho) || tipo > FIM {
                return Err(ErroLog::Alterado {
                    segmento,
                    registro: numero,
                });
            }
            let mut cifrado = Vec::new();
            (&mut *origem)
                .take(tamanho as u64)
                .read_to_end(&mut cifrado)?;
            if cifrado.len() < tamanho {
                leitura.avisos.push(AvisoLog::Cortado {
                    bytes: (TOPO_DO_REGISTRO + cifrado.len()) as u64,
                });
                return Ok(leitura);
            }
            let claro = cifra
                .decrypt(
                    &nonce_do_registro(numero),
                    Payload {
                        msg: &cifrado,
                        aad: &dados_autenticados(&cabecalho, tipo, numero),
                    },
                )
                .map_err(|_| ErroLog::Alterado {
                    segmento,
                    registro: numero,
                })?;
            if tipo == DADOS {
                saida.write_all(&claro)?;
                leitura.bytes += claro.len() as u64;
            }
            numero += 1;
            if tipo == FIM {
                break;
            }
        }
    }
}

/// Copia um log de texto para `saida` como [`ler`] faz com os cifrados, para a página tratar os
/// dois do mesmo jeito. A leitura sempre tem um segmento e nenhum aviso.
pub fn texto_simples(
    origem: &mut dyn Read,
    saida: &mut dyn Write,
    cancelar: &AtomicBool,
) -> Result<Leitura, ErroLog> {
    let mut leitura = Leitura {
        segmentos: 1,
        ..Leitura::default()
    };
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if cancelar.load(Ordering::Relaxed) {
            return Err(ErroLog::Cancelado);
        }
        let n = match origem.read(&mut buf) {
            Ok(0) => return Ok(leitura),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        saida.write_all(&buf[..n])?;
        leitura.bytes += n as u64;
    }
}

/// Se o arquivo começa com a marca dos logs cifrados. A página não confia só na extensão.
pub fn e_cifrado(caminho: &Path) -> bool {
    let mut marca = [0u8; 8];
    fs::File::open(caminho)
        .and_then(|mut f| f.read_exact(&mut marca))
        .is_ok()
        && &marca == MARCA
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use std::io::Cursor;

    fn pasta(nome: &str) -> PastaTemporaria {
        let pasta = PastaTemporaria::nova(nome);
        fs::create_dir_all(&pasta.0).unwrap();
        pasta
    }

    fn sem_cancelar() -> AtomicBool {
        AtomicBool::new(false)
    }

    fn chaves_de(pares: &[&ParDeChaves]) -> HashMap<[u8; 8], [u8; 32]> {
        pares
            .iter()
            .map(|(privada, publica)| (id_da_chave(publica), *privada))
            .collect()
    }

    fn texto_de(n: usize) -> Vec<u8> {
        (0..n).map(|i| b'a' + (i % 26) as u8).collect()
    }

    /// Grava `partes` num segmento novo de `caminho` e fecha.
    fn gravar(caminho: &Path, par: &ParDeChaves, partes: &[&[u8]]) {
        let mut escritor = EscritorCifrado::abrir(caminho, &par.1).unwrap();
        for parte in partes {
            escritor.write_all(parte).unwrap();
        }
    }

    fn decifrar(
        bytes: &[u8],
        chaves: &HashMap<[u8; 8], [u8; 32]>,
    ) -> (Vec<u8>, Result<Leitura, ErroLog>) {
        let mut saida = Vec::new();
        let r = ler(
            &mut Cursor::new(bytes.to_vec()),
            chaves,
            &mut saida,
            &sem_cancelar(),
        );
        (saida, r)
    }

    fn ok(bytes: &[u8], chaves: &HashMap<[u8; 8], [u8; 32]>) -> (Vec<u8>, Leitura) {
        let (saida, r) = decifrar(bytes, chaves);
        (saida, r.unwrap())
    }

    fn alterado(r: Result<Leitura, ErroLog>) {
        assert!(matches!(r, Err(ErroLog::Alterado { .. })), "{r:?}");
    }

    #[test]
    fn ida_e_volta_de_um_segmento_curto() {
        let pasta = pasta("log-curto");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"ol\xc3\xa1 ", b"mundo\n"]);
        let (texto, leitura) = ok(&fs::read(&caminho).unwrap(), &chaves_de(&[&par]));
        assert_eq!(texto, "olá mundo\n".as_bytes());
        assert_eq!(leitura.segmentos, 1);
        assert_eq!(leitura.bytes, texto.len() as u64);
        assert!(leitura.avisos.is_empty(), "{:?}", leitura.avisos);
    }

    #[test]
    fn ida_e_volta_com_varios_registros() {
        let pasta = pasta("log-varios");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        let claro = texto_de(REGISTRO * 2 + 1234);
        // Em pedaços de tamanhos que não casam com o do registro.
        let pedacos: Vec<&[u8]> = claro.chunks(10_007).collect();
        gravar(&caminho, &par, &pedacos);
        let bytes = fs::read(&caminho).unwrap();
        // Cabeçalho, 2 registros cheios, 1 com o resto e o `fim`.
        let esperado = CABECALHO
            + 2 * (TOPO_DO_REGISTRO + REGISTRO + ETIQUETA)
            + (TOPO_DO_REGISTRO + 1234 + ETIQUETA)
            + (TOPO_DO_REGISTRO + ETIQUETA);
        assert_eq!(bytes.len(), esperado);
        let (texto, leitura) = ok(&bytes, &chaves_de(&[&par]));
        assert_eq!(texto, claro);
        assert!(leitura.avisos.is_empty());
    }

    #[test]
    fn um_segmento_vazio_so_tem_o_fim() {
        let pasta = pasta("log-vazio");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[]);
        let bytes = fs::read(&caminho).unwrap();
        assert_eq!(bytes.len(), CABECALHO + TOPO_DO_REGISTRO + ETIQUETA);
        let (texto, leitura) = ok(&bytes, &chaves_de(&[&par]));
        assert!(texto.is_empty());
        assert_eq!(leitura.segmentos, 1);
        assert!(leitura.avisos.is_empty());
    }

    #[test]
    fn dois_segmentos_com_chaves_de_logs_diferentes() {
        let pasta = pasta("log-dois");
        let caminho = pasta.0.join("a.log.cifrado");
        let (a, b) = (par_novo().unwrap(), par_novo().unwrap());
        gravar(&caminho, &a, &[b"primeiro\n"]);
        gravar(&caminho, &b, &[b"segundo\n"]);
        let bytes = fs::read(&caminho).unwrap();
        let (texto, leitura) = ok(&bytes, &chaves_de(&[&a, &b]));
        assert_eq!(texto, b"primeiro\nsegundo\n");
        assert_eq!(leitura.segmentos, 2);
        // Só com a chave do primeiro: o texto do primeiro sai e a leitura para no segundo.
        let (parcial, r) = decifrar(&bytes, &chaves_de(&[&a]));
        assert_eq!(parcial, b"primeiro\n");
        match r {
            Err(ErroLog::ChaveAusente { id }) => assert_eq!(id, id_em_hex(&id_da_chave(&b.1))),
            outro => panic!("{outro:?}"),
        }
    }

    #[test]
    fn abrir_de_novo_apos_registro_cortado_comeca_limpo() {
        let pasta = pasta("log-reabrir");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        let mut escritor = EscritorCifrado::abrir(&caminho, &par.1).unwrap();
        escritor.write_all(b"antes da queda\n").unwrap();
        escritor.flush().unwrap();
        escritor.write_all(b"vai ficar pela metade\n").unwrap();
        escritor.flush().unwrap();
        std::mem::forget(escritor);
        // A queda: o último registro ficou pela metade.
        let inteiro = fs::read(&caminho).unwrap();
        fs::write(&caminho, &inteiro[..inteiro.len() - 10]).unwrap();

        gravar(&caminho, &par, &[b"depois\n"]);
        let (texto, leitura) = ok(&fs::read(&caminho).unwrap(), &chaves_de(&[&par]));
        assert_eq!(texto, b"antes da queda\ndepois\n");
        assert_eq!(leitura.segmentos, 2);
        // O primeiro segmento nunca foi fechado; o corte em si não aparece.
        assert_eq!(leitura.avisos, vec![AvisoLog::SemFim { segmento: 1 }]);
    }

    #[test]
    fn abrir_de_novo_apos_cabecalho_cortado() {
        let pasta = pasta("log-cabecalho-cortado");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"um\n"]);
        let mut bytes = fs::read(&caminho).unwrap();
        bytes.extend_from_slice(&MARCA[..]);
        bytes.extend_from_slice(&[1, 2, 3]);
        fs::write(&caminho, &bytes).unwrap();
        gravar(&caminho, &par, &[b"dois\n"]);
        let (texto, leitura) = ok(&fs::read(&caminho).unwrap(), &chaves_de(&[&par]));
        assert_eq!(texto, b"um\ndois\n");
        assert!(leitura.avisos.is_empty(), "{:?}", leitura.avisos);
    }

    #[test]
    fn arquivo_cortado_no_meio_de_um_registro_da_aviso() {
        let pasta = pasta("log-cortado");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        let claro = texto_de(REGISTRO + 500);
        gravar(&caminho, &par, &[&claro]);
        let bytes = fs::read(&caminho).unwrap();
        let chaves = chaves_de(&[&par]);
        // Corta dentro do registro de dados do resto (500 bytes), mantendo o primeiro inteiro.
        let primeiro = CABECALHO + TOPO_DO_REGISTRO + REGISTRO + ETIQUETA;
        for corte in [primeiro + 3, primeiro + 5, primeiro + 100] {
            let (texto, leitura) = ok(&bytes[..corte], &chaves);
            assert_eq!(texto, &claro[..REGISTRO], "{corte}");
            assert_eq!(
                leitura.avisos,
                vec![AvisoLog::Cortado {
                    bytes: (corte - primeiro) as u64
                }],
                "{corte}"
            );
        }
        // Cortado no cabeçalho.
        let (texto, leitura) = ok(&bytes[..20], &chaves);
        assert!(texto.is_empty());
        assert_eq!(leitura.avisos, vec![AvisoLog::Cortado { bytes: 20 }]);
    }

    #[test]
    fn segmento_sem_fim_da_aviso() {
        let pasta = pasta("log-sem-fim");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        let mut escritor = EscritorCifrado::abrir(&caminho, &par.1).unwrap();
        escritor.write_all(b"ainda gravando\n").unwrap();
        escritor.flush().unwrap();
        let (texto, leitura) = ok(&fs::read(&caminho).unwrap(), &chaves_de(&[&par]));
        assert_eq!(texto, b"ainda gravando\n");
        assert_eq!(leitura.avisos, vec![AvisoLog::SemFim { segmento: 1 }]);
        std::mem::forget(escritor);
    }

    #[test]
    fn flush_sem_nada_novo_nao_grava_registro() {
        let pasta = pasta("log-flush");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        let mut escritor = EscritorCifrado::abrir(&caminho, &par.1).unwrap();
        escritor.flush().unwrap();
        assert_eq!(fs::metadata(&caminho).unwrap().len(), CABECALHO as u64);
        escritor.write_all(b"x").unwrap();
        // Acumulado ainda não vai ao disco.
        assert_eq!(fs::metadata(&caminho).unwrap().len(), CABECALHO as u64);
        escritor.flush().unwrap();
        let depois = fs::metadata(&caminho).unwrap().len();
        assert_eq!(depois, (CABECALHO + TOPO_DO_REGISTRO + 1 + ETIQUETA) as u64);
        escritor.flush().unwrap();
        assert_eq!(fs::metadata(&caminho).unwrap().len(), depois);
    }

    #[test]
    fn mexer_em_qualquer_byte_quebra_a_etiqueta() {
        let pasta = pasta("log-mexido");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"segredo-de-teste\n"]);
        let original = fs::read(&caminho).unwrap();
        let chaves = chaves_de(&[&par]);
        // No cifrado do registro de dados.
        let mut b = original.clone();
        b[CABECALHO + TOPO_DO_REGISTRO + 2] ^= 1;
        alterado(decifrar(&b, &chaves).1);
        // Na etiqueta (último byte do registro de dados).
        let mut b = original.clone();
        b[CABECALHO + TOPO_DO_REGISTRO + 17 + ETIQUETA - 1] ^= 1;
        alterado(decifrar(&b, &chaves).1);
        // No tipo: dados vira fim.
        let mut b = original.clone();
        b[CABECALHO + 4] = FIM;
        alterado(decifrar(&b, &chaves).1);
        // No tamanho: um byte a menos no cifrado.
        let mut b = original.clone();
        b[CABECALHO + 3] -= 1;
        alterado(decifrar(&b, &chaves).1);
        // Na efêmera do cabeçalho: outra chave, etiqueta não confere.
        let mut b = original.clone();
        b[20] ^= 1;
        alterado(decifrar(&b, &chaves).1);
        // Nos dados do cabeçalho que não entram na chave: o id muda, e a chave some.
        let mut b = original.clone();
        b[10] ^= 1;
        assert!(matches!(
            decifrar(&b, &chaves).1,
            Err(ErroLog::ChaveAusente { .. })
        ));
    }

    #[test]
    fn registros_trocados_ou_repetidos_dao_alterado() {
        let pasta = pasta("log-troca");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        let mut escritor = EscritorCifrado::abrir(&caminho, &par.1).unwrap();
        for texto in [&b"um\n"[..], b"dois\n", b"tres\n"] {
            escritor.write_all(texto).unwrap();
            escritor.flush().unwrap();
        }
        drop(escritor);
        let bytes = fs::read(&caminho).unwrap();
        let chaves = chaves_de(&[&par]);
        let tam = |texto: usize| TOPO_DO_REGISTRO + texto + ETIQUETA;
        let (r0, r1, r2) = (
            CABECALHO..CABECALHO + tam(3),
            CABECALHO + tam(3)..CABECALHO + tam(3) + tam(5),
            CABECALHO + tam(3) + tam(5)..CABECALHO + tam(3) + tam(5) + tam(5),
        );
        let resto = &bytes[r2.end..];
        let montar = |ordem: &[&std::ops::Range<usize>]| {
            let mut b = bytes[..CABECALHO].to_vec();
            for r in ordem {
                b.extend_from_slice(&bytes[(*r).clone()]);
            }
            b.extend_from_slice(resto);
            b
        };
        // Do jeito certo, lê.
        assert_eq!(
            ok(&montar(&[&r0, &r1, &r2]), &chaves).0,
            b"um\ndois\ntres\n"
        );
        // Trocados.
        alterado(decifrar(&montar(&[&r1, &r0, &r2]), &chaves).1);
        // Um repetido.
        alterado(decifrar(&montar(&[&r0, &r1, &r1, &r2]), &chaves).1);
        // Um sumido do meio.
        alterado(decifrar(&montar(&[&r0, &r2]), &chaves).1);
    }

    #[test]
    fn tamanho_absurdo_nao_aloca() {
        let pasta = pasta("log-gigante");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"x"]);
        let mut bytes = fs::read(&caminho).unwrap();
        bytes.truncate(CABECALHO + TOPO_DO_REGISTRO + 20);
        bytes[CABECALHO..CABECALHO + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(bytes.len() < 100);
        alterado(decifrar(&bytes, &chaves_de(&[&par])).1);
        // Um tamanho pequeno demais para a etiqueta também.
        bytes[CABECALHO..CABECALHO + 4].copy_from_slice(&3u32.to_be_bytes());
        alterado(decifrar(&bytes, &chaves_de(&[&par])).1);
    }

    #[test]
    fn id_desconhecido_e_versao_nova() {
        let pasta = pasta("log-id-versao");
        let caminho = pasta.0.join("a.log.cifrado");
        let (a, outra) = (par_novo().unwrap(), par_novo().unwrap());
        gravar(&caminho, &a, &[b"x"]);
        let bytes = fs::read(&caminho).unwrap();
        match decifrar(&bytes, &chaves_de(&[&outra])).1 {
            Err(ErroLog::ChaveAusente { id }) => {
                assert_eq!(id, id_em_hex(&id_da_chave(&a.1)));
                assert_eq!(id.len(), 16);
            }
            outro => panic!("{outro:?}"),
        }
        let mut v2 = bytes.clone();
        v2[8] = 2;
        assert!(matches!(
            decifrar(&v2, &chaves_de(&[&a])).1,
            Err(ErroLog::VersaoNova(2))
        ));
    }

    #[test]
    fn publica_de_ordem_baixa_e_recusada() {
        let pasta = pasta("log-ordem-baixa");
        let caminho = pasta.0.join("a.log.cifrado");
        let e = EscritorCifrado::abrir(&caminho, &[0u8; 32]).err().unwrap();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert!(!caminho.exists(), "uma pública inválida não cria o arquivo");

        // Na leitura: um cabeçalho com a efêmera toda zero.
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"x"]);
        let mut bytes = fs::read(&caminho).unwrap();
        bytes[17..CABECALHO].fill(0);
        alterado(decifrar(&bytes, &chaves_de(&[&par])).1);
    }

    #[test]
    fn arquivo_que_nao_e_do_formato() {
        let pasta = pasta("log-nao-e");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        let texto = b"isto e um log de texto comum, com mais de oito bytes\n";
        fs::write(&caminho, texto).unwrap();
        let e = EscritorCifrado::abrir(&caminho, &par.1).err().unwrap();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(&caminho).unwrap(), texto, "o arquivo não é tocado");
        assert!(!e_cifrado(&caminho));
        assert!(matches!(
            decifrar(texto, &chaves_de(&[&par])).1,
            Err(ErroLog::NaoE)
        ));
        // Curto demais, também.
        assert!(matches!(
            decifrar(b"oi", &chaves_de(&[&par])).1,
            Err(ErroLog::NaoE)
        ));
        // Vazio: sem segmentos.
        assert_eq!(ok(b"", &chaves_de(&[&par])).1.segmentos, 0);
    }

    #[test]
    fn e_cifrado_olha_a_marca_e_nao_o_nome() {
        let pasta = pasta("log-marca");
        let par = par_novo().unwrap();
        let cifrado = pasta.0.join("sem-extensao");
        gravar(&cifrado, &par, &[b"x"]);
        assert!(e_cifrado(&cifrado));
        assert!(!e_cifrado(&pasta.0.join("nao-existe")));
    }

    #[test]
    fn o_texto_claro_nao_aparece_no_arquivo() {
        let pasta = pasta("log-claro");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"senha: segredo-de-teste\n"]);
        let bytes = fs::read(&caminho).unwrap();
        let procurado = b"segredo-de-teste";
        assert!(!bytes.windows(procurado.len()).any(|j| j == procurado));
    }

    #[test]
    fn texto_simples_copia_tudo() {
        let claro = texto_de(200_000);
        let mut saida = Vec::new();
        let leitura =
            texto_simples(&mut Cursor::new(claro.clone()), &mut saida, &sem_cancelar()).unwrap();
        assert_eq!(saida, claro);
        assert_eq!(leitura.bytes, 200_000);
        assert_eq!(leitura.segmentos, 1);
    }

    #[test]
    fn cancelar_interrompe_a_leitura() {
        let pasta = pasta("log-cancelar");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"x"]);
        let cancelar = AtomicBool::new(true);
        let mut saida = Vec::new();
        let r = ler(
            &mut Cursor::new(fs::read(&caminho).unwrap()),
            &chaves_de(&[&par]),
            &mut saida,
            &cancelar,
        );
        assert!(matches!(r, Err(ErroLog::Cancelado)), "{r:?}");
        let r = texto_simples(&mut Cursor::new(b"x".to_vec()), &mut saida, &cancelar);
        assert!(matches!(r, Err(ErroLog::Cancelado)), "{r:?}");
    }

    #[test]
    #[cfg(unix)]
    fn o_arquivo_novo_so_e_do_dono() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = pasta("log-modo");
        let caminho = pasta.0.join("a.log.cifrado");
        let par = par_novo().unwrap();
        gravar(&caminho, &par, &[b"x"]);
        let modo = fs::metadata(&caminho).unwrap().permissions().mode();
        assert_eq!(modo & 0o777, 0o600);
    }

    #[test]
    fn o_arquivo_publico_vai_e_volta_e_um_arquivo_mexido_conta_como_ausente() {
        let pasta = pasta("log-pub");
        assert_eq!(ler_publica(&pasta.0), None);
        let par = par_novo().unwrap();
        gravar_publica(&pasta.0, &par.1, "uid-1").unwrap();
        assert_eq!(
            ler_publica(&pasta.0),
            Some(PublicaDosLogs {
                publica: par.1,
                uid: "uid-1".into()
            })
        );
        // O id não bate com a pública.
        let outra = par_novo().unwrap();
        let mexido = fs::read_to_string(pasta.0.join(ARQUIVO_PUBLICO))
            .unwrap()
            .replace(&STANDARD.encode(par.1), &STANDARD.encode(outra.1));
        fs::write(pasta.0.join(ARQUIVO_PUBLICO), mexido).unwrap();
        assert_eq!(ler_publica(&pasta.0), None);
        // Lixo.
        fs::write(pasta.0.join(ARQUIVO_PUBLICO), b"{ isto nao e json").unwrap();
        assert_eq!(ler_publica(&pasta.0), None);
    }

    #[test]
    fn ids_em_hexadecimal_vao_e_voltam() {
        let par = par_novo().unwrap();
        let id = id_da_chave(&par.1);
        let hex = id_em_hex(&id);
        assert_eq!(hex.len(), 16);
        assert_eq!(id_de_hex(&hex), Some(id));
        assert_eq!(id_de_hex("zz"), None);
        assert_eq!(id_de_hex(&"g".repeat(16)), None);
        assert_eq!(publica_da(&par.0), par.1);
    }
}
