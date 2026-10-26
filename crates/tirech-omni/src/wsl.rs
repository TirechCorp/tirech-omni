//! O WSL (M42): lê a lista de distribuições do `wsl.exe -l -q` e monta a conexão local que
//! abre cada uma. As funções aqui são puras; quem roda o `wsl.exe` é o [`Opener`].
//!
//! [`Opener`]: crate::abrir::Opener

use crate::modelo::{Connection, Kind};

/// Lê a saída do `wsl.exe -l -q`: um nome por linha, sem repetidos, na ordem em que vieram.
///
/// O `wsl.exe` escreve em UTF-16LE (com BOM, na maioria das versões). Decodifica como UTF-16LE
/// quando há o BOM `FF FE` ou quando a maioria dos bytes de posição ímpar é zero; senão, como
/// UTF-8, com perdas. Bytes tortos (número ímpar de bytes, surrogates soltos) viram `U+FFFD`.
pub fn ler_lista(bytes: &[u8]) -> Vec<String> {
    let texto = if e_utf16(bytes) {
        decodificar_utf16(bytes)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    let mut nomes: Vec<String> = Vec::new();
    for linha in texto.lines() {
        let nome = linha.trim_matches(|c: char| c == '\u{feff}' || c == '\0' || c.is_whitespace());
        if !nome.is_empty() && !nomes.iter().any(|n| n == nome) {
            nomes.push(nome.to_owned());
        }
    }
    nomes
}

/// Se os bytes parecem UTF-16LE: o BOM, ou zeros na maioria das posições ímpares.
fn e_utf16(bytes: &[u8]) -> bool {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return true;
    }
    let impares = bytes.len() / 2;
    if impares == 0 {
        return false;
    }
    let zeros = bytes.iter().skip(1).step_by(2).filter(|b| **b == 0).count();
    zeros * 2 > impares
}

/// UTF-16LE para texto; um byte sobrando no fim é descartado.
fn decodificar_utf16(bytes: &[u8]) -> String {
    let unidades: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|par| u16::from_le_bytes([par[0], par[1]]))
        .collect();
    String::from_utf16_lossy(&unidades)
}

/// A conexão local que abre a distribuição `distro`: `wsl.exe -d <distro> --cd ~`, com o nome
/// igual ao da distribuição. Nome com espaço leva aspas.
pub fn conexao_wsl(distro: &str) -> Connection {
    let mut c = Connection::new(distro, Kind::Local);
    c.local_programa = "wsl.exe".to_owned();
    let nome = if distro.contains(char::is_whitespace) {
        format!("\"{distro}\"")
    } else {
        distro.to_owned()
    };
    c.local_argumentos = format!("-d {nome} --cd ~");
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abrir::dividir_argumentos;

    fn utf16(texto: &str, com_bom: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        if com_bom {
            bytes.extend([0xFF, 0xFE]);
        }
        for unidade in texto.encode_utf16() {
            bytes.extend(unidade.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn le_a_lista_em_utf16_com_bom() {
        let bytes = utf16("Ubuntu\r\nDebian\r\n", true);
        assert_eq!(ler_lista(&bytes), ["Ubuntu", "Debian"]);
    }

    #[test]
    fn le_a_lista_em_utf16_sem_bom() {
        let bytes = utf16("Ubuntu-22.04\r\nkali-linux\r\n", false);
        assert_eq!(ler_lista(&bytes), ["Ubuntu-22.04", "kali-linux"]);
    }

    #[test]
    fn le_a_lista_em_utf8() {
        assert_eq!(ler_lista(b"Ubuntu\nDebian\n"), ["Ubuntu", "Debian"]);
    }

    #[test]
    fn descarta_vazias_e_repetidas_e_mantem_a_ordem() {
        let bytes = utf16("B\r\n\r\nA\r\n  B  \r\n\r\nC\r\n", true);
        assert_eq!(ler_lista(&bytes), ["B", "A", "C"]);
    }

    #[test]
    fn tira_zeros_e_bom_que_sobraram() {
        assert_eq!(ler_lista(b"\xEF\xBB\xBFUbuntu\0\n"), ["Ubuntu"]);
    }

    #[test]
    fn bytes_tortos_nao_causam_panico() {
        // Número ímpar de bytes e um surrogate solto.
        let mut bytes = utf16("Ubuntu\r\n", true);
        bytes.push(0x41);
        assert_eq!(ler_lista(&bytes), ["Ubuntu"]);
        let solto = [0xFF, 0xFE, 0x00, 0xD8, b'x', 0x00];
        let _ = ler_lista(&solto);
        assert!(ler_lista(&[]).is_empty());
        let _ = ler_lista(&[0xFF]);
    }

    #[test]
    fn a_conexao_do_wsl_e_local_e_abre_a_distribuicao() {
        let c = conexao_wsl("Ubuntu");
        assert_eq!(c.kind, Kind::Local);
        assert_eq!(c.name, "Ubuntu");
        assert_eq!(c.local_programa, "wsl.exe");
        assert_eq!(c.local_argumentos, "-d Ubuntu --cd ~");
        assert_eq!(
            dividir_argumentos(&c.local_argumentos),
            ["-d", "Ubuntu", "--cd", "~"]
        );
    }

    #[test]
    fn nome_com_espaco_leva_aspas() {
        let c = conexao_wsl("Meu Debian");
        assert_eq!(c.local_argumentos, "-d \"Meu Debian\" --cd ~");
        assert_eq!(
            dividir_argumentos(&c.local_argumentos),
            ["-d", "Meu Debian", "--cd", "~"]
        );
    }
}
