//! Os espaços de trabalho (M51, seção 8): uma pasta de um servidor SSH que o assistente trata
//! como se fosse local. Aqui ficam o modelo ([`EspacoIa`]) e a parte de texto, sem rede, de
//! `ler` e `editar`: a numeração `cat -n`, o recorte por linhas, a detecção de arquivo
//! binário ou fora do UTF-8 e a troca de um trecho. As operações com o servidor estão em
//! [`super::espaco_fs`] e [`super::espaco_cmd`].

use std::io::{self, Read};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::arquivos::formato::tamanho;
use crate::t;

/// Quantos bytes do começo do arquivo se olham atrás de um NUL para saber se é binário.
const BYTES_DO_BINARIO: usize = 8 * 1024;

/// O marcador que a redação põe no lugar de um segredo; um trecho a editar com ele não existe
/// no arquivo.
const MARCADOR_DE_SEGREDO: &str = "[segredo removido";

/// Uma pasta de um servidor SSH que um assistente usa como espaço de trabalho.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EspacoIa {
    /// O nome, único: letras, números, `-` e `_`.
    pub nome: String,
    /// A conexão (só SSH) cuja permissão vale para o espaço.
    pub conexao: u64,
    /// A pasta no servidor, absoluta.
    pub raiz: String,
    /// A pasta local que espelha a raiz (a fatia 9 do plano; hoje sempre vazia).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub espelho: Option<PathBuf>,
}

/// Se `nome` serve de nome de espaço: não vazio, só letras, números, `-` e `_`.
///
/// ```
/// use tirech_omni::mcp::espaco::nome_valido;
///
/// assert!(nome_valido("api-2_beta"));
/// assert!(!nome_valido("minha api"));
/// assert!(!nome_valido(""));
/// ```
pub fn nome_valido(nome: &str) -> bool {
    !nome.is_empty()
        && nome
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

/// Um nome livre a partir de `base`: o próprio, ou com `-2`, `-3`… se já existe. Letras que o
/// nome de espaço não aceita viram `-`.
pub fn nome_livre(base: &str, existentes: &[EspacoIa]) -> String {
    let limpo: String = base
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let limpo = limpo.trim_matches('-');
    let base = if limpo.is_empty() { "espaco" } else { limpo };
    let usado = |n: &str| existentes.iter().any(|e| e.nome.eq_ignore_ascii_case(n));
    if !usado(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|n| !usado(n))
        .unwrap_or_else(|| base.to_owned())
}

/// Se a raiz é de um servidor Windows: começa com a letra de uma unidade (`C:/…`, `/C:/…` ou
/// `C:\…`).
pub fn raiz_windows(raiz: &str) -> bool {
    let sem_barra = raiz.strip_prefix('/').unwrap_or(raiz);
    let mut letras = sem_barra.chars();
    matches!(
        (letras.next(), letras.next()),
        (Some(letra), Some(':')) if letra.is_ascii_alphabetic()
    )
}

/// O que o SFTP contou de um arquivo na hora em que o assistente o leu; `editar` e `escrever`
/// conferem que continua igual.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lido {
    /// A data de modificação, em segundos desde 1970 (`None`: o servidor não disse).
    pub mtime: Option<u64>,
    /// O tamanho.
    pub tamanho: u64,
}

/// Uma linha com o número alinhado à direita em 6 colunas, um TAB e o texto (o `cat -n`).
pub fn linha_numerada(numero: usize, texto: &str) -> String {
    format!("{numero:>6}\t{texto}")
}

/// Numera `texto` a partir de `primeira` (`cat -n`). Sem texto, devolve vazio.
///
/// ```
/// use tirech_omni::mcp::espaco::formato_cat_n;
///
/// assert_eq!(formato_cat_n("a\nb", 1), "     1\ta\n     2\tb");
/// ```
pub fn formato_cat_n(texto: &str, primeira: usize) -> String {
    if texto.is_empty() {
        return String::new();
    }
    texto
        .split('\n')
        .enumerate()
        .map(|(i, linha)| linha_numerada(primeira + i, linha.strip_suffix('\r').unwrap_or(linha)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// O que sobrou de uma leitura por linhas.
#[derive(Debug, PartialEq, Eq)]
pub struct Trecho {
    /// As linhas pedidas, já numeradas, juntas por `\n`.
    pub texto: String,
    /// Quantas linhas o arquivo tem.
    pub total: usize,
    /// A primeira linha que não coube, quando o arquivo continua.
    pub proxima: Option<usize>,
    /// Se havia bytes que não são UTF-8.
    pub nao_utf8: bool,
}

/// Por que um arquivo não vira texto.
#[derive(Debug, PartialEq, Eq)]
pub enum Leitura {
    /// O trecho pedido.
    Texto(Trecho),
    /// Há um NUL nos primeiros 8 KiB.
    Binario,
}

/// Se `inicio` e `linhas` (os argumentos do assistente) viram uma janela válida: a primeira
/// linha (a partir de 1) e quantas ler. `padrao` vale sem `linhas`. `inicio` 0 ou ausente é 1.
///
/// ```
/// use tirech_omni::mcp::espaco::inicio_e_linhas;
///
/// assert_eq!(inicio_e_linhas(None, None, 2000), (1, 2000));
/// assert_eq!(inicio_e_linhas(Some(10), Some(5), 2000), (10, 5));
/// assert_eq!(inicio_e_linhas(Some(0), Some(0), 2000), (1, 2000));
/// ```
pub fn inicio_e_linhas(inicio: Option<u64>, linhas: Option<u64>, padrao: usize) -> (usize, usize) {
    let inicio = inicio
        .and_then(|n| usize::try_from(n).ok())
        .filter(|&n| n >= 1)
        .unwrap_or(1);
    let linhas = linhas
        .and_then(|n| usize::try_from(n).ok())
        .filter(|&n| n >= 1)
        .unwrap_or(padrao.max(1));
    (inicio, linhas)
}

/// Se os primeiros bytes têm um NUL.
pub fn binario_por_nul(bytes: &[u8]) -> bool {
    bytes.iter().take(BYTES_DO_BINARIO).any(|&b| b == 0)
}

/// Lê `leitor` em fluxo e guarda as `linhas` linhas a partir da `inicio` (a partir de 1),
/// numeradas. Segue contando as outras sem guardá-las, para dizer quantas são.
pub fn ler_trecho(leitor: &mut dyn Read, inicio: usize, linhas: usize) -> io::Result<Leitura> {
    let fim = inicio.saturating_add(linhas); // a primeira linha que não entra
    let entra = |numero: usize| numero >= inicio && numero < fim;
    let mut bloco = vec![0u8; 64 * 1024];
    let mut saida: Vec<String> = Vec::new();
    // A linha em andamento: seus bytes (só os que vão entrar) e se tem algum byte.
    let mut pendente: Vec<u8> = Vec::new();
    let (mut numero, mut parcial, mut nao_utf8, mut primeiro) = (0usize, false, false, true);
    let mut guardar = |numero: usize, linha: &[u8], saida: &mut Vec<String>| {
        let linha = linha.strip_suffix(b"\r").unwrap_or(linha);
        let texto = String::from_utf8_lossy(linha);
        nao_utf8 |= matches!(texto, std::borrow::Cow::Owned(_));
        saida.push(linha_numerada(numero, &texto));
    };
    loop {
        let lidos = match leitor.read(&mut bloco) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        let mut resto = &bloco[..lidos];
        if primeiro {
            primeiro = false;
            if binario_por_nul(resto) {
                return Ok(Leitura::Binario);
            }
        }
        while let Some(i) = resto.iter().position(|&b| b == b'\n') {
            numero += 1;
            if entra(numero) {
                pendente.extend_from_slice(&resto[..i]);
                guardar(numero, &pendente, &mut saida);
            }
            pendente.clear();
            parcial = false;
            resto = &resto[i + 1..];
        }
        if !resto.is_empty() {
            parcial = true;
            if entra(numero + 1) {
                pendente.extend_from_slice(resto);
            }
        }
    }
    if parcial {
        numero += 1;
        if entra(numero) {
            guardar(numero, &pendente, &mut saida);
        }
    }
    Ok(Leitura::Texto(Trecho {
        texto: saida.join("\n"),
        total: numero,
        proxima: (numero >= fim).then_some(fim),
        nao_utf8,
    }))
}

/// A linha que avisa que o arquivo tem mais linhas do que as pedidas.
pub fn aviso_de_continua(total: usize, proxima: usize) -> String {
    t!("mcp-espaco-continua", total = total, proxima = proxima)
}

/// Os bytes como texto: UTF-8 como está, o resto com `�` no lugar do inválido. O `bool` diz se
/// houve troca.
pub fn decodificar(bytes: &[u8]) -> (String, bool) {
    match String::from_utf8_lossy(bytes) {
        std::borrow::Cow::Borrowed(texto) => (texto.to_owned(), false),
        std::borrow::Cow::Owned(texto) => (texto, true),
    }
}

/// A linha que avisa que o arquivo não é UTF-8.
pub fn aviso_nao_utf8() -> String {
    t!("mcp-espaco-nao-utf8")
}

/// O texto que o assistente recebe de uma leitura.
pub fn resposta_da_leitura(leitura: Leitura, total_de_bytes: u64, inicio: usize) -> String {
    let trecho = match leitura {
        Leitura::Binario => return t!("mcp-espaco-binario", tamanho = tamanho(total_de_bytes)),
        Leitura::Texto(trecho) => trecho,
    };
    let mut partes = Vec::new();
    if trecho.nao_utf8 {
        partes.push(aviso_nao_utf8());
    }
    if trecho.total == 0 {
        partes.push(t!("mcp-espaco-vazio"));
    } else if trecho.texto.is_empty() {
        partes.push(t!(
            "mcp-espaco-alem-do-fim",
            inicio = inicio,
            total = trecho.total
        ));
    } else {
        partes.push(trecho.texto);
    }
    if let Some(proxima) = trecho.proxima {
        partes.push(aviso_de_continua(trecho.total, proxima));
    }
    partes.join("\n")
}

/// O resultado de trocar um trecho.
#[derive(Debug, PartialEq, Eq)]
pub struct Troca {
    /// O arquivo inteiro depois da troca.
    pub conteudo: String,
    /// Quantas trocas houve.
    pub trocas: usize,
    /// Onde a primeira começa, em bytes (a mesma posição no antes e no depois).
    pub primeira: usize,
}

/// Troca `antigo` por `novo` em `conteudo`. Sem `todas`, o trecho precisa aparecer uma vez só.
/// O erro é o texto para o assistente.
pub fn trocar(conteudo: &str, antigo: &str, novo: &str, todas: bool) -> Result<Troca, String> {
    if antigo.is_empty() {
        return Err(t!("mcp-espaco-antigo-vazio"));
    }
    if antigo == novo {
        return Err(t!("mcp-espaco-nada-a-mudar"));
    }
    if antigo.contains(MARCADOR_DE_SEGREDO) {
        return Err(t!("mcp-espaco-segredo-no-trecho"));
    }
    let achados: Vec<usize> = conteudo.match_indices(antigo).map(|(i, _)| i).collect();
    match achados.len() {
        0 => {
            let crlf = conteudo.contains("\r\n") && antigo.contains('\n') && !antigo.contains('\r');
            Err(if crlf {
                t!("mcp-espaco-trecho-ausente-crlf")
            } else {
                t!("mcp-espaco-trecho-ausente")
            })
        }
        n if n > 1 && !todas => Err(t!("mcp-espaco-trecho-varias", n = n)),
        n => Ok(Troca {
            conteudo: if todas {
                conteudo.replace(antigo, novo)
            } else {
                conteudo.replacen(antigo, novo, 1)
            },
            trocas: n,
            primeira: achados[0],
        }),
    }
}

/// O aviso de que o trecho novo tem quebras `\n` num arquivo que usa `\r\n`, se for o caso.
pub fn aviso_crlf(conteudo: &str, novo: &str) -> Option<String> {
    (conteudo.contains("\r\n") && novo.replace("\r\n", "").contains('\n'))
        .then(|| t!("mcp-espaco-aviso-crlf"))
}

/// A resposta de uma edição: quantas trocas e três linhas antes e depois da primeira, numeradas.
pub fn contexto_da_resposta(troca: &Troca, novo: &str) -> String {
    const VIZINHAS: usize = 3;
    let linhas: Vec<&str> = troca.conteudo.split('\n').collect();
    let primeira = troca.conteudo[..troca.primeira].matches('\n').count();
    let ultima = primeira + novo.matches('\n').count();
    let de = primeira.saturating_sub(VIZINHAS);
    let ate = (ultima + VIZINHAS).min(linhas.len().saturating_sub(1));
    let corpo = (de..=ate)
        .map(|i| linha_numerada(i + 1, linhas[i].strip_suffix('\r').unwrap_or(linhas[i])))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n{corpo}", t!("mcp-espaco-editado", n = troca.trocas))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn trecho(texto: &str, inicio: usize, linhas: usize) -> Trecho {
        match ler_trecho(&mut texto.as_bytes(), inicio, linhas).unwrap() {
            Leitura::Texto(t) => t,
            Leitura::Binario => panic!("não devia ser binário"),
        }
    }

    #[test]
    fn formato_cat_n() {
        assert_eq!(
            super::formato_cat_n("a\nb\n", 1),
            "     1\ta\n     2\tb\n     3\t"
        );
        assert_eq!(super::formato_cat_n("x", 99), "    99\tx");
        assert_eq!(super::formato_cat_n("", 1), "");
        assert_eq!(super::formato_cat_n("a\r\nb", 1), "     1\ta\n     2\tb");
    }

    #[test]
    fn inicio_e_linhas() {
        assert_eq!(super::inicio_e_linhas(None, None, 2000), (1, 2000));
        assert_eq!(super::inicio_e_linhas(Some(5), Some(3), 2000), (5, 3));
        assert_eq!(super::inicio_e_linhas(Some(0), Some(0), 10), (1, 10));
        assert_eq!(super::inicio_e_linhas(None, None, 0), (1, 1));
    }

    #[test]
    fn ler_pelo_recorte() {
        let t = trecho("a\nb\nc\nd\ne\n", 2, 2);
        assert_eq!(t.texto, "     2\tb\n     3\tc");
        assert_eq!(t.total, 5);
        assert_eq!(t.proxima, Some(4));
        let t = trecho("a\nb\nc", 1, 10);
        assert_eq!(t.texto, "     1\ta\n     2\tb\n     3\tc");
        assert_eq!((t.total, t.proxima), (3, None));
        let t = trecho("a\nb\n", 9, 2);
        assert_eq!((t.texto.as_str(), t.total, t.proxima), ("", 2, None));
        let t = trecho("", 1, 5);
        assert_eq!((t.texto.as_str(), t.total), ("", 0));
    }

    #[test]
    fn aviso_de_continua() {
        let texto = super::aviso_de_continua(120, 51);
        assert!(
            texto.contains("120") && texto.contains("inicio=51"),
            "{texto}"
        );
    }

    #[test]
    fn binario_por_nul() {
        assert!(super::binario_por_nul(b"ab\0cd"));
        assert!(!super::binario_por_nul("texto comum ção".as_bytes()));
        let mut longo = vec![b'a'; BYTES_DO_BINARIO];
        longo.push(0);
        assert!(
            !super::binario_por_nul(&longo),
            "o NUL depois dos 8 KiB não conta"
        );
        let leitura = ler_trecho(&mut &b"PK\0\0abc"[..], 1, 10).unwrap();
        assert_eq!(leitura, Leitura::Binario);
    }

    #[test]
    fn nao_utf8_avisa() {
        let leitura = ler_trecho(&mut &b"ol\xe1\nmundo\n"[..], 1, 10).unwrap();
        let Leitura::Texto(t) = leitura else { panic!() };
        assert!(t.nao_utf8);
        assert!(t.texto.contains("ol\u{fffd}"));
        let resposta = resposta_da_leitura(Leitura::Texto(t), 10, 1);
        assert!(resposta.starts_with(&aviso_nao_utf8()), "{resposta}");
        assert_eq!(decodificar(b"ok"), ("ok".to_owned(), false));
        assert_eq!(decodificar(b"o\xffk"), ("o\u{fffd}k".to_owned(), true));
    }

    #[test]
    fn trocar_unica() {
        let r = trocar("um\ndois\ntrês\n", "dois", "2", false).unwrap();
        assert_eq!(r.conteudo, "um\n2\ntrês\n");
        assert_eq!((r.trocas, r.primeira), (1, 3));
    }

    #[test]
    fn trocar_nenhuma_da_erro() {
        let e = trocar("abc", "x", "y", false).unwrap_err();
        assert_eq!(e, t!("mcp-espaco-trecho-ausente"));
        let e = trocar("a\r\nb\r\n", "a\nb", "x", false).unwrap_err();
        assert_eq!(e, t!("mcp-espaco-trecho-ausente-crlf"));
    }

    #[test]
    fn trocar_varias_sem_todas_da_erro() {
        let e = trocar("aa", "a", "b", false).unwrap_err();
        assert!(e.contains('2'), "{e}");
    }

    #[test]
    fn trocar_todas() {
        let r = trocar("a-a-a", "a", "b", true).unwrap();
        assert_eq!((r.conteudo.as_str(), r.trocas), ("b-b-b", 3));
    }

    #[test]
    fn antigo_igual_novo() {
        assert_eq!(
            trocar("abc", "b", "b", false).unwrap_err(),
            t!("mcp-espaco-nada-a-mudar")
        );
        assert_eq!(
            trocar("abc", "", "b", false).unwrap_err(),
            t!("mcp-espaco-antigo-vazio")
        );
    }

    #[test]
    fn antigo_com_marcador_de_segredo_recusado() {
        let e = trocar(
            "token=[segredo removido: senha]",
            "token=[segredo removido",
            "x",
            false,
        )
        .unwrap_err();
        assert_eq!(e, t!("mcp-espaco-segredo-no-trecho"));
    }

    #[test]
    fn aviso_crlf() {
        assert!(super::aviso_crlf("a\r\nb", "x\ny").is_some());
        assert!(super::aviso_crlf("a\r\nb", "x\r\ny").is_none());
        assert!(super::aviso_crlf("a\nb", "x\ny").is_none());
    }

    #[test]
    fn contexto_da_resposta() {
        let antes: String = (1..=10).map(|n| format!("l{n}\n")).collect();
        let r = trocar(&antes, "l5", "cinco\nmeio", false).unwrap();
        let texto = super::contexto_da_resposta(&r, "cinco\nmeio");
        assert!(
            texto.starts_with(&t!("mcp-espaco-editado", n = 1)),
            "{texto}"
        );
        assert!(texto.contains("     2\tl2"), "{texto}");
        assert!(!texto.contains("     1\tl1"), "{texto}");
        assert!(texto.contains("     5\tcinco"), "{texto}");
        assert!(texto.contains("     6\tmeio"), "{texto}");
        assert!(texto.contains("     9\tl8"), "{texto}");
        assert!(!texto.contains("l9"), "{texto}");
    }

    #[test]
    fn nomes_de_espaco() {
        assert!(nome_valido("api"));
        assert!(!nome_valido("a b"));
        let existentes = vec![EspacoIa {
            nome: "api".into(),
            ..EspacoIa::default()
        }];
        assert_eq!(nome_livre("api", &existentes), "api-2");
        assert_eq!(nome_livre("meu site", &existentes), "meu-site");
        assert_eq!(nome_livre("///", &existentes), "espaco");
    }

    #[test]
    fn raiz_de_windows() {
        assert!(raiz_windows("C:/Users/ana"));
        assert!(raiz_windows("/C:/Users"));
        assert!(raiz_windows("d:\\x"));
        assert!(!raiz_windows("/srv/app"));
        assert!(!raiz_windows("/c/x"));
    }
}
