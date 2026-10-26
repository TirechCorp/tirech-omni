//! As cifras dos outros programas, nos dois sentidos quando o formato deixa:
//!
//! * **WinSCP:** a senha do `WinSCP.ini` não é criptografia, é embaralhamento reversível (cada
//!   byte vira dois dígitos hexa); a chave é o usuário mais o servidor. Lê e escreve.
//! * **mRemoteNG:** AES-256-GCM com a chave tirada da senha-mestra por PBKDF2-HMAC-SHA1 (o
//!   formato atual, lê e escreve) e AES-128-CBC com a chave MD5 da senha-mestra (o formato
//!   antigo, só lê).
//!
//! Nada aqui entra em pânico com entrada torta: devolve `Err`/`None`. Os bytes aleatórios da
//! escrita entram por parâmetro, para os testes fixarem a semente.

use crate::t;
use aes::Aes256;
use aes::cipher::{BlockModeDecrypt, KeyIvInit, block_padding::Pkcs7};
use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{AesGcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use md5::{Digest, Md5};

/// A senha-mestra que o mRemoteNG usa quando ninguém definiu outra.
pub const SENHA_MESTRA_PADRAO: &str = "mR3m";

/// O texto que o `Protected` do cabeçalho do mRemoteNG decifra quando a senha-mestra está certa
/// e o arquivo não está protegido por uma senha de verdade.
pub const PROTEGIDO_NAO: &str = "ThisIsNotProtected";

/// O texto que o `Protected` decifra quando o arquivo tem senha-mestra própria.
pub const PROTEGIDO_SIM: &str = "ThisIsProtected";

/// As rodadas do PBKDF2 que o mRemoteNG usa quando o cabeçalho não diz.
pub const ITERACOES_PADRAO: u32 = 1000;

/// As rodadas que a leitura aceita no máximo (um cabeçalho torto não pode travar o app).
const ITERACOES_MAXIMAS: u32 = 10_000_000;

// ---------------------------------------------------------------- WinSCP

const MAGICO: u8 = 0xA3;
const MARCA: u8 = 0xFF;
const DIGITOS: &[u8; 16] = b"0123456789ABCDEF";
/// O tamanho mínimo, em dígitos hexa, que o WinSCP dá ao texto embaralhado.
const TAMANHO_MINIMO_HEXA: usize = 100;

/// Lê um byte (dois dígitos hexa) do embaralhamento do WinSCP.
fn proximo_byte(hexa: &[u8], pos: &mut usize) -> Option<u8> {
    let alto = digito(*hexa.get(*pos)?)?;
    let baixo = digito(*hexa.get(*pos + 1)?)?;
    *pos += 2;
    let valor = (alto << 4) + baixo;
    Some(!(valor ^ MAGICO))
}

fn digito(c: u8) -> Option<u8> {
    DIGITOS
        .iter()
        .position(|&d| d == c.to_ascii_uppercase())
        .map(|p| p as u8)
}

/// Desembaralha o `Password` do WinSCP. `chave` é o usuário seguido do servidor. `Ok(None)` se
/// o prefixo da chave não bate (a senha era de outra sessão); `Err` se o texto está torto.
pub fn winscp_desembaralhar(hexa: &str, chave: &str) -> Result<Option<String>, String> {
    let torto = || t!("cripto-winscp-formato");
    let hexa = hexa.trim().as_bytes();
    let mut pos = 0;
    let marca = proximo_byte(hexa, &mut pos).ok_or_else(torto)?;
    let tamanho = if marca == MARCA {
        proximo_byte(hexa, &mut pos).ok_or_else(torto)?;
        proximo_byte(hexa, &mut pos).ok_or_else(torto)?
    } else {
        marca
    };
    let descarte = proximo_byte(hexa, &mut pos).ok_or_else(torto)?;
    pos = pos
        .checked_add(usize::from(descarte) * 2)
        .ok_or_else(torto)?;
    let mut bytes = Vec::with_capacity(usize::from(tamanho));
    for _ in 0..tamanho {
        bytes.push(proximo_byte(hexa, &mut pos).ok_or_else(torto)?);
    }
    if marca == MARCA {
        return Ok(bytes
            .strip_prefix(chave.as_bytes())
            .map(|resto| String::from_utf8_lossy(resto).into_owned()));
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn escrever_byte(saida: &mut String, byte: u8) {
    let valor = !byte ^ MAGICO;
    saida.push(char::from(DIGITOS[usize::from(valor >> 4)]));
    saida.push(char::from(DIGITOS[usize::from(valor & 0x0F)]));
}

/// Embaralha `chave + senha` do jeito que o WinSCP lê (formato novo). `None` se a cadeia passa
/// de 255 bytes (o tamanho cabe num byte só). `aleatorio` dá os bytes de enchimento.
pub fn winscp_embaralhar(
    chave: &str,
    senha: &str,
    aleatorio: &mut dyn FnMut() -> u8,
) -> Option<String> {
    let cadeia = [chave.as_bytes(), senha.as_bytes()].concat();
    let tamanho = u8::try_from(cadeia.len()).ok()?;
    let descarte = if tamanho < 50 {
        aleatorio() % (50 - tamanho)
    } else {
        0
    };
    let mut saida = String::new();
    escrever_byte(&mut saida, MARCA);
    escrever_byte(&mut saida, 0);
    escrever_byte(&mut saida, tamanho);
    escrever_byte(&mut saida, descarte);
    for _ in 0..descarte {
        escrever_byte(&mut saida, aleatorio());
    }
    for byte in cadeia {
        escrever_byte(&mut saida, byte);
    }
    while saida.len() < TAMANHO_MINIMO_HEXA {
        escrever_byte(&mut saida, aleatorio());
    }
    Some(saida)
}

// ------------------------------------------------------------- mRemoteNG

type Aes256Gcm16 = AesGcm<Aes256, U16>;

fn chave_gcm(senha_mestra: &str, sal: &[u8], iteracoes: u32) -> [u8; 32] {
    pbkdf2::pbkdf2_hmac_array::<sha1::Sha1, 32>(senha_mestra.as_bytes(), sal, iteracoes)
}

/// Decifra o `Password` (ou o `Protected`) de um `confCons.xml` no formato GCM: base64 de
/// `sal(16) + nonce(16) + cifrado + etiqueta(16)`. Erro se a senha-mestra está errada.
pub fn mremoteng_decifrar(
    base64: &str,
    senha_mestra: &str,
    iteracoes: u32,
) -> Result<String, String> {
    let dados = STANDARD
        .decode(base64.trim())
        .map_err(|_| t!("cripto-mremoteng-sem-base64"))?;
    if dados.len() < 16 + 16 + 16 {
        return Err(t!("cripto-mremoteng-curta"));
    }
    if iteracoes == 0 || iteracoes > ITERACOES_MAXIMAS {
        return Err(t!("cripto-mremoteng-rodadas"));
    }
    let (sal, resto) = dados.split_at(16);
    let (nonce, cifrado) = resto.split_at(16);
    let chave = chave_gcm(senha_mestra, sal, iteracoes);
    let cifra = Aes256Gcm16::new((&chave).into());
    let nonce = Nonce::<U16>::try_from(nonce).map_err(|_| t!("cripto-nonce-torto"))?;
    // O dado associado é o sal; se o arquivo veio de uma versão que não o usa, tenta sem.
    let claro = [sal, &[][..]]
        .iter()
        .find_map(|aad| cifra.decrypt(&nonce, Payload { msg: cifrado, aad }).ok());
    match claro {
        Some(bytes) => Ok(String::from_utf8_lossy(&bytes).into_owned()),
        None => Err(t!("cripto-mremoteng-senha-mestra")),
    }
}

/// Cifra `texto` no formato GCM do mRemoteNG, com o `sal` e o `nonce` dados (aleatórios de 16
/// bytes cada; os testes os fixam).
pub fn mremoteng_cifrar(
    texto: &str,
    senha_mestra: &str,
    iteracoes: u32,
    sal: [u8; 16],
    nonce: [u8; 16],
) -> Result<String, String> {
    let chave = chave_gcm(senha_mestra, &sal, iteracoes);
    let cifra = Aes256Gcm16::new((&chave).into());
    let cifrado = cifra
        .encrypt(
            &Nonce::<U16>::from(nonce),
            Payload {
                msg: texto.as_bytes(),
                aad: &sal,
            },
        )
        .map_err(|_| t!("cripto-mremoteng-nao-cifrou"))?;
    let mut tudo = Vec::with_capacity(32 + cifrado.len());
    tudo.extend_from_slice(&sal);
    tudo.extend_from_slice(&nonce);
    tudo.extend_from_slice(&cifrado);
    Ok(STANDARD.encode(tudo))
}

/// Decifra o `Password` do formato antigo do mRemoteNG: base64 de `IV(16) + cifrado`, AES-128 em
/// CBC com a chave MD5 da senha-mestra. Sem etiqueta de autenticação: uma senha-mestra errada
/// costuma falhar no preenchimento e vira `Err`, mas pode sair lixo que ainda é UTF-8 válido.
pub fn mremoteng_decifrar_antigo(base64: &str, senha_mestra: &str) -> Result<String, String> {
    let dados = STANDARD
        .decode(base64.trim())
        .map_err(|_| t!("cripto-mremoteng-sem-base64"))?;
    if dados.len() < 32 || !dados.len().is_multiple_of(16) {
        return Err(t!("cripto-mremoteng-curta-ou-torta"));
    }
    let (iv, cifrado) = dados.split_at(16);
    let chave = Md5::digest(senha_mestra.as_bytes());
    let mut buffer = cifrado.to_vec();
    let decifrador = cbc::Decryptor::<aes::Aes128>::new_from_slices(&chave, iv)
        .map_err(|_| t!("cripto-chave-ou-iv"))?;
    let claro = decifrador
        .decrypt_padded::<Pkcs7>(&mut buffer)
        .map_err(|_| t!("cripto-mremoteng-senha-mestra"))?;
    String::from_utf8(claro.to_vec()).map_err(|_| t!("cripto-mremoteng-senha-mestra"))
}

#[cfg(test)]
mod testes {
    use super::*;
    use aes::cipher::BlockModeEncrypt;

    fn sequencia(semente: u8) -> impl FnMut() -> u8 {
        let mut n = semente;
        move || {
            n = n.wrapping_mul(73).wrapping_add(41);
            n
        }
    }

    #[test]
    fn winscp_ida_e_volta_com_acento_e_200_bytes() {
        for senha in ["", "s3nha", "açaí-ñ", &"x".repeat(200 - 15)] {
            let chave = "rootexemplo.com";
            let hexa = winscp_embaralhar(chave, senha, &mut sequencia(7)).expect("cabe");
            assert!(hexa.len() >= 100);
            assert_eq!(
                winscp_desembaralhar(&hexa, chave).unwrap().as_deref(),
                Some(senha)
            );
        }
    }

    #[test]
    fn winscp_prefixo_de_outra_sessao_vale_nada_e_tamanho_demais_nao_cabe() {
        let hexa = winscp_embaralhar("anah1", "abc", &mut sequencia(1)).unwrap();
        assert_eq!(winscp_desembaralhar(&hexa, "outrah2").unwrap(), None);
        assert!(winscp_embaralhar("a", &"y".repeat(255), &mut sequencia(1)).is_none());
        assert!(winscp_embaralhar("a", &"y".repeat(254), &mut sequencia(1)).is_some());
    }

    #[test]
    fn winscp_valor_torto_nao_entra_em_panico() {
        for torto in [
            "",
            "A",
            "ZZ",
            "FF",
            "0123",
            "A3A3A3A3A3",
            "FFFFFFFFFFFFFFFF",
            "é",
        ] {
            let _ = winscp_desembaralhar(torto, "x");
        }
    }

    #[test]
    fn winscp_formato_antigo_sem_prefixo() {
        // Monta à mão: sem a marca 0xFF, o primeiro byte já é o tamanho.
        let mut hexa = String::new();
        escrever_byte(&mut hexa, 3);
        escrever_byte(&mut hexa, 0);
        for b in *b"abc" {
            escrever_byte(&mut hexa, b);
        }
        assert_eq!(
            winscp_desembaralhar(&hexa, "qualquer").unwrap().as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn mremoteng_gcm_ida_e_volta_e_senha_errada() {
        let base64 =
            mremoteng_cifrar("s3nha ç", SENHA_MESTRA_PADRAO, 1000, [1; 16], [2; 16]).unwrap();
        assert_eq!(
            mremoteng_decifrar(&base64, SENHA_MESTRA_PADRAO, 1000).unwrap(),
            "s3nha ç"
        );
        assert!(mremoteng_decifrar(&base64, "outra", 1000).is_err());
        assert!(mremoteng_decifrar(&base64, SENHA_MESTRA_PADRAO, 999).is_err());
    }

    #[test]
    fn mremoteng_gcm_sem_dado_associado_tambem_abre() {
        let sal = [9u8; 16];
        let nonce = [4u8; 16];
        let chave = chave_gcm("mR3m", &sal, 1000);
        let cifra = Aes256Gcm16::new((&chave).into());
        let cifrado = cifra
            .encrypt(&Nonce::<U16>::from(nonce), b"abc".as_slice())
            .unwrap();
        let tudo = [&sal[..], &nonce[..], &cifrado[..]].concat();
        assert_eq!(
            mremoteng_decifrar(&STANDARD.encode(tudo), "mR3m", 1000).unwrap(),
            "abc"
        );
    }

    #[test]
    fn mremoteng_antigo_cbc_com_md5() {
        let iv = [5u8; 16];
        let chave = Md5::digest(b"mR3m");
        let mut buffer = [0u8; 32];
        buffer[..6].copy_from_slice(b"segred");
        let cifrado = cbc::Encryptor::<aes::Aes128>::new_from_slices(&chave, &iv)
            .unwrap()
            .encrypt_padded::<Pkcs7>(&mut buffer, 6)
            .unwrap()
            .to_vec();
        let base64 = STANDARD.encode([&iv[..], &cifrado[..]].concat());
        assert_eq!(
            mremoteng_decifrar_antigo(&base64, "mR3m").unwrap(),
            "segred"
        );
        assert!(mremoteng_decifrar_antigo(&base64, "errada").is_err());
    }

    #[test]
    fn mremoteng_entrada_torta_devolve_erro() {
        for torto in [
            "",
            "***",
            "AAAA",
            &STANDARD.encode([0u8; 20]),
            &STANDARD.encode([0u8; 48]),
        ] {
            assert!(mremoteng_decifrar(torto, "mR3m", 1000).is_err());
            assert!(mremoteng_decifrar_antigo(torto, "mR3m").is_err());
        }
        let ok = mremoteng_cifrar("a", "mR3m", 10, [0; 16], [0; 16]).unwrap();
        assert!(mremoteng_decifrar(&ok, "mR3m", 0).is_err());
        assert!(mremoteng_decifrar(&ok, "mR3m", u32::MAX).is_err());
    }
}
