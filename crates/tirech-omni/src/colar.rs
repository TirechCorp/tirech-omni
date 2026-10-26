//! Colar imagem e arquivos no terminal (M45): a parte pura. Os nomes dos arquivos enviados, a
//! colisão, as datas e a limpeza por idade, as aspas do caminho para cada shell, a escolha entre
//! a fila e o pacote `tar`, e os comandos que o servidor roda. O que fala com a rede, a fila e a
//! tela mora em `tela/colar.rs`.

pub mod pacote;
pub mod servidor_falso;

use std::io;

use jiff::ToSpan;
use jiff::civil::DateTime;
use tomni_ui::files::{EntryKind, Fs, path};
use tomni_ui::terminal::{ShellQuote, shell_quote};

use crate::modelo::ShellRemoto;

/// O mínimo de arquivos, todos pequenos, para a pasta ir pelo `tar` mesmo abaixo do mínimo da
/// configuração.
pub const TAR_PEQUENOS_MIN: u64 = 10;

/// Até quantos bytes, em média, um arquivo conta como pequeno para o [`usar_tar`].
pub const TAR_PEQUENO_BYTES: u64 = 64 * 1024;

/// O maior nome de arquivo, em bytes, que os sistemas de arquivos aceitam.
pub const NOME_MAX: usize = 255;

/// A pasta no servidor quando nada foi configurado.
pub const PASTA_PADRAO: &str = "/tmp/tirech-colar";

/// A pasta para onde o app cai quando a configurada não dá para usar.
pub const PASTA_RESERVA: &str = "~/.cache/tirech-colar";

/// A hora local de agora, sem fuso (a fonte das datas dos nomes).
pub fn agora() -> DateTime {
    jiff::Zoned::now().datetime()
}

fn carimbo(quando: DateTime) -> String {
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        quando.year(),
        quando.month(),
        quando.day(),
        quando.hour(),
        quando.minute(),
        quando.second()
    )
}

/// O nome de um arquivo local limpo para o servidor: sem `/` nem caracteres de controle, e, se o
/// servidor for Windows, sem os que ele não aceita no nome.
fn limpar(nome: &str, windows: bool) -> String {
    let proibido = |c: char| {
        c.is_control()
            || c == '/'
            || (windows && matches!(c, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    };
    let mut limpo: String = nome
        .chars()
        .map(|c| if proibido(c) { '_' } else { c })
        .collect();
    if windows {
        // O Windows não guarda nome terminado em ponto ou espaço.
        if limpo.ends_with(['.', ' ']) {
            limpo.pop();
            limpo.push('_');
        }
    }
    if limpo.is_empty() {
        limpo.push_str("arquivo");
    }
    limpo
}

/// Separa o radical da extensão (com o ponto): `a.tar.gz` dá `a.tar` e `.gz`; `.bashrc` e `x`
/// não têm extensão.
fn separar(nome: &str) -> (&str, &str) {
    match nome.rfind('.') {
        Some(i) if i > 0 && nome.len() - i <= 17 => nome.split_at(i),
        _ => (nome, ""),
    }
}

/// Corta `radical` para que `radical + extensao` caiba em `max` bytes, sem quebrar um caractere
/// no meio.
fn cortar(radical: &str, extensao: &str, max: usize) -> String {
    let sobra = max.saturating_sub(extensao.len());
    let mut fim = radical.len().min(sobra);
    while !radical.is_char_boundary(fim) {
        fim -= 1;
    }
    format!("{}{extensao}", &radical[..fim])
}

/// O nome com que um arquivo local chega ao servidor: `AAAAMMDD-HHMMSS-<nome>`. A data deixa a
/// limpeza por idade achar o que o app mandou, e o nome fica legível para quem lê o prompt.
/// `windows` diz que o servidor é Windows (nomes mais restritos).
pub fn nome_remoto(nome_local: &str, quando: DateTime, windows: bool) -> String {
    let prefixo = format!("{}-", carimbo(quando));
    let limpo = limpar(nome_local, windows);
    let (radical, extensao) = separar(&limpo);
    // Sobra para o radical o que o prefixo e a extensão deixam; um nome só de extensão enorme
    // é cortado inteiro.
    let cabe = NOME_MAX.saturating_sub(prefixo.len());
    if extensao.len() >= cabe {
        return format!("{prefixo}{}", cortar(&limpo, "", cabe));
    }
    format!("{prefixo}{}", cortar(radical, extensao, cabe))
}

/// O nome de uma imagem colada: `colado-AAAAMMDD-HHMMSS.png`.
pub fn nome_da_imagem(quando: DateTime) -> String {
    format!("colado-{}.png", carimbo(quando))
}

/// Dá a `nome` o primeiro `-2`, `-3`… (antes da extensão) que não está em `existentes`. O nome
/// livre sai como veio. Quem chama põe o escolhido de volta em `existentes` para o próximo.
pub fn sem_colisao(nome: &str, existentes: &[impl AsRef<str>]) -> String {
    let ocupado = |candidato: &str| existentes.iter().any(|e| e.as_ref() == candidato);
    if !ocupado(nome) {
        return nome.to_owned();
    }
    let (radical, extensao) = separar(nome);
    (2u32..)
        .map(|n| cortar(radical, &format!("-{n}{extensao}"), NOME_MAX))
        .find(|candidato| !ocupado(candidato))
        .unwrap_or_else(|| nome.to_owned())
}

fn numero(texto: &str) -> Option<i16> {
    if texto.bytes().all(|b| b.is_ascii_digit()) {
        texto.parse().ok()
    } else {
        None
    }
}

/// A data de um carimbo `AAAAMMDD-HHMMSS` (exatamente 15 caracteres), se for uma data real.
fn ler_carimbo(texto: &str) -> Option<DateTime> {
    if texto.len() != 15 || texto.as_bytes()[8] != b'-' || !texto.is_ascii() {
        return None;
    }
    let ano = numero(&texto[0..4])?;
    let campo = |de, ate| numero(&texto[de..ate]).and_then(|n| i8::try_from(n).ok());
    DateTime::new(
        ano,
        campo(4, 6)?,
        campo(6, 8)?,
        campo(9, 11)?,
        campo(11, 13)?,
        campo(13, 15)?,
        0,
    )
    .ok()
}

/// A data que o app pôs no nome de algo que enviou: `AAAAMMDD-HHMMSS-<nome>` ou
/// `colado-AAAAMMDD-HHMMSS<…>`. Qualquer outro nome (e uma data que não existe) dá `None`, e a
/// limpeza não toca nele.
pub fn data_do_nome(nome: &str) -> Option<DateTime> {
    if let Some(resto) = nome.strip_prefix("colado-") {
        let carimbo = resto.get(..15)?;
        let depois = &resto[15..];
        if depois.is_empty() || depois.starts_with('.') {
            return ler_carimbo(carimbo);
        }
        return None;
    }
    let carimbo = nome.get(..15)?;
    let depois = nome.get(15..)?;
    if depois.len() > 1 && depois.starts_with('-') {
        return ler_carimbo(carimbo);
    }
    None
}

/// Os nomes que o app enviou há mais de `dias` dias. Com 0 dias, nenhum (nunca apaga).
pub fn vencidos(nomes: &[String], agora: DateTime, dias: u32) -> Vec<String> {
    if dias == 0 {
        return Vec::new();
    }
    let Ok(limite) = agora.checked_sub(i64::from(dias).days()) else {
        return Vec::new();
    };
    nomes
        .iter()
        .filter(|nome| data_do_nome(nome).is_some_and(|data| data < limite))
        .cloned()
        .collect()
}

/// O caminho do Windows que o SFTP do OpenSSH mostra (`/C:/Users/ana`) vira `C:\Users\ana`;
/// outro caminho não muda.
fn do_windows(remoto: &str) -> Option<String> {
    let b = remoto.as_bytes();
    if b.len() >= 4 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' && b[3] == b'/' {
        Some(remoto[1..].replace('/', "\\"))
    } else {
        None
    }
}

/// Se o caminho remoto é de um servidor Windows (`/C:/Users/ana`, como o SFTP do OpenSSH o mostra).
pub fn e_windows(remoto: &str) -> bool {
    do_windows(remoto).is_some()
}

/// O shell e o caminho, já na forma que ele entende. No Automático, um caminho do Windows vai
/// de `Cmd` (que o PowerShell também lê, com aspas duplas) se não tem `$`, `` ` `` nem `'`; senão
/// de `PowerShell`.
fn escolher(remoto: &str, shell: ShellRemoto) -> (String, ShellQuote) {
    let windows = do_windows(remoto);
    match shell {
        ShellRemoto::Automatico => match windows {
            Some(caminho) if caminho.contains(['$', '`', '\'']) => {
                (caminho, ShellQuote::PowerShell)
            }
            Some(caminho) => (caminho, ShellQuote::Cmd),
            None => (remoto.to_owned(), ShellQuote::Posix),
        },
        ShellRemoto::Posix => (remoto.to_owned(), ShellQuote::Posix),
        ShellRemoto::PowerShell => (
            windows.unwrap_or_else(|| remoto.to_owned()),
            ShellQuote::PowerShell,
        ),
        ShellRemoto::Cmd => (
            windows.unwrap_or_else(|| remoto.to_owned()),
            ShellQuote::Cmd,
        ),
    }
}

/// O caminho remoto como o shell do servidor o lê, com as aspas que ele precisar.
pub fn caminho_para_o_shell(remoto: &str, shell: ShellRemoto) -> String {
    let (caminho, aspas) = escolher(remoto, shell);
    shell_quote(&caminho, aspas)
}

/// Como [`caminho_para_o_shell`], mas sempre entre aspas (os comandos que o app monta).
fn entre_aspas(remoto: &str, shell: ShellRemoto) -> String {
    let (caminho, aspas) = escolher(remoto, shell);
    let citado = shell_quote(&caminho, aspas);
    if citado != caminho {
        return citado;
    }
    match aspas {
        ShellQuote::Cmd => format!("\"{citado}\""),
        _ => format!("'{citado}'"),
    }
}

/// O caminho de um arquivo **desta máquina** para o shell local (o que se cola quando não há
/// envio). No Windows o caminho é `C:\…` e segue a regra do Automático; no resto, é POSIX.
pub fn caminho_local_para_o_shell(local: &str, windows: bool) -> String {
    if !windows {
        return shell_quote(local, ShellQuote::Posix);
    }
    let aspas = if local.contains(['$', '`', '\'']) {
        ShellQuote::PowerShell
    } else {
        ShellQuote::Cmd
    };
    shell_quote(local, aspas)
}

/// Os pedaços que vão ao terminal, na ordem: um por caminho remoto, cada um com as aspas do
/// shell. Quem cola manda `Session::paste` de cada um e um espaço depois (também do último).
pub fn texto_para_colar(remotos: &[String], shell: ShellRemoto) -> Vec<String> {
    remotos
        .iter()
        .map(|caminho| caminho_para_o_shell(caminho, shell))
        .collect()
}

/// Se uma pasta vai num pacote `tar` só: tem `minimo` arquivos ou mais, ou pelo menos
/// [`TAR_PEQUENOS_MIN`] arquivos que, em média, não passam de [`TAR_PEQUENO_BYTES`]. `minimo`
/// 0 desliga.
pub fn usar_tar(arquivos: u64, bytes: u64, minimo: u32) -> bool {
    minimo > 0
        && (arquivos >= u64::from(minimo)
            || (arquivos >= TAR_PEQUENOS_MIN && bytes / arquivos <= TAR_PEQUENO_BYTES))
}

/// O que o servidor sabe de `tar`, sondado uma vez por sessão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sondagem {
    /// Lê um pacote comprimido com gzip.
    TarGzip,
    /// Só lê o pacote puro.
    Tar,
    /// Não tem `tar` (ou não deixou perguntar): a pasta vai pela fila.
    SemTar,
}

/// As extensões que já vêm comprimidas: gastar processador nelas não encolhe o envio.
const JA_COMPRIMIDAS: &[&str] = &[
    "png", "jpg", "jpeg", "webp", "gif", "avif", "heic", "mp3", "mp4", "mkv", "mov", "webm", "zip",
    "gz", "tgz", "bz2", "xz", "zst", "7z", "rar", "pdf", "docx", "xlsx", "pptx", "odt", "ods",
    "jar", "apk", "whl",
];

/// Se o fluxo `tar` vai comprimido com gzip. `arquivos` são o nome e o tamanho de cada um.
/// Não comprime quando o servidor não lê gzip, quando a conexão já comprime (duas vezes só
/// gasta processador) ou quando 70% dos bytes ou mais são de formatos já comprimidos.
pub fn comprimir_fluxo(
    arquivos: &[(String, u64)],
    sondagem: Sondagem,
    conexao_comprimida: bool,
) -> bool {
    if sondagem != Sondagem::TarGzip || conexao_comprimida {
        return false;
    }
    let total: u64 = arquivos.iter().map(|(_, bytes)| bytes).sum();
    let comprimidos: u64 = arquivos
        .iter()
        .filter(|(nome, _)| {
            let base = nome.rsplit('/').next().unwrap_or(nome);
            let (_, extensao) = separar(base);
            let extensao = extensao.trim_start_matches('.').to_lowercase();
            JA_COMPRIMIDAS.contains(&extensao.as_str())
        })
        .map(|(_, bytes)| bytes)
        .sum();
    // 70% ou mais já comprimidos: sem gzip.
    comprimidos.saturating_mul(10) < total.saturating_mul(7)
}

/// O comando que extrai o pacote em `destino` (a pasta já existe). `-o` não restaura o dono
/// gravado no pacote. Sem `gzip`, sai o `-z`.
pub fn comando_tar(destino: &str, shell: ShellRemoto, gzip: bool) -> String {
    let z = if gzip { " -z" } else { "" };
    format!("tar -x{z} -o -f - -C {}", entre_aspas(destino, shell))
}

/// O caminho como o `sh` do plano B o lê: entre aspas simples e, se começa com `~/`, com o
/// `"$HOME"` fora delas (o `sh` não expande o `~` dentro de aspas).
pub fn alvo_do_plano_b(caminho: &str) -> String {
    match caminho.strip_prefix('~') {
        Some(resto) if resto.is_empty() || resto.starts_with('/') => {
            if resto.is_empty() {
                "\"$HOME\"".to_owned()
            } else {
                format!("\"$HOME\"{}", entre_aspas(resto, ShellRemoto::Posix))
            }
        }
        _ => entre_aspas(caminho, ShellRemoto::Posix),
    }
}

/// O comando do plano B (sem SFTP, o shell é POSIX): cria a pasta com `umask 077` e extrai.
pub fn comando_tar_criando(destino: &str, gzip: bool) -> String {
    let alvo = alvo_do_plano_b(destino);
    let z = if gzip { " -z" } else { "" };
    format!("umask 077 && mkdir -p -- {alvo} && tar -x{z} -o -f - -C {alvo}")
}

/// O comando do plano B que grava a entrada padrão em `arquivo`, criando a pasta dele antes.
pub fn comando_enviar_arquivo(arquivo: &str) -> String {
    let pai = arquivo.rsplit_once('/').map_or("", |(pai, _)| pai);
    let pai = if pai.is_empty() { "/" } else { pai };
    format!(
        "umask 077 && mkdir -p -- {} && cat > {}",
        alvo_do_plano_b(pai),
        alvo_do_plano_b(arquivo)
    )
}

/// O comando do plano B que cria `pasta` (e o que faltar acima dela) só para o dono.
pub fn comando_criar_pasta(pasta: &str) -> String {
    format!("umask 077 && mkdir -p -- {}", alvo_do_plano_b(pasta))
}

/// O comando do plano B que lista os nomes de `pasta`, um por linha.
pub fn comando_listar(pasta: &str) -> String {
    format!("ls -1A -- {}", alvo_do_plano_b(pasta))
}

/// O comando do plano B que apaga `nomes` de dentro de `pasta` (e o que houver dentro deles).
pub fn comando_apagar(pasta: &str, nomes: &[String]) -> String {
    let base = pasta.trim_end_matches('/');
    let alvos: Vec<String> = nomes
        .iter()
        .map(|nome| alvo_do_plano_b(&format!("{base}/{nome}")))
        .collect();
    format!("rm -rf -- {}", alvos.join(" "))
}

/// Apaga `caminho` e o que houver dentro, pelo `Fs` (o SFTP só remove pasta vazia). Um caminho
/// que já não existe não é erro.
pub fn apagar_arvore(fs: &mut dyn Fs, caminho: &str) -> io::Result<()> {
    let entrada = match fs.lstat(caminho) {
        Ok(entrada) => entrada,
        Err(erro) if erro.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(erro) => return Err(erro),
    };
    if entrada.kind != EntryKind::Dir {
        return fs.remove_file(caminho);
    }
    for filho in fs.list(caminho)? {
        apagar_arvore(fs, &path::join(caminho, &filho.name))?;
    }
    fs.remove_dir(caminho)
}

/// A sondagem com gzip: lista um pacote vazio comprimido.
pub const SONDA_GZIP: &str = "tar -tzf -";

/// A sondagem sem gzip: lista um pacote vazio.
pub const SONDA_TAR: &str = "tar -tf -";

#[cfg(test)]
mod testes {
    use super::*;

    fn quando() -> DateTime {
        DateTime::new(2026, 10, 19, 14, 30, 12, 0).unwrap()
    }

    #[test]
    fn nome_remoto_leva_a_data_e_o_nome() {
        assert_eq!(
            nome_remoto("relatório final.pdf", quando(), false),
            "20261019-143012-relatório final.pdf"
        );
    }

    #[test]
    fn nome_remoto_do_windows_troca_o_que_ele_nao_aceita() {
        assert_eq!(
            nome_remoto("a:b?.txt", quando(), true),
            "20261019-143012-a_b_.txt"
        );
        assert_eq!(
            nome_remoto("a:b?.txt", quando(), false),
            "20261019-143012-a:b?.txt"
        );
    }

    #[test]
    fn nome_remoto_limpa_controle_e_barra_nos_dois() {
        for windows in [false, true] {
            assert_eq!(
                nome_remoto("a/b\nc\0d", quando(), windows),
                "20261019-143012-a_b_c_d"
            );
        }
    }

    #[test]
    fn nome_remoto_corta_o_radical_e_guarda_a_extensao() {
        let nome = format!("{}.txt", "é".repeat(300));
        let remoto = nome_remoto(&nome, quando(), false);
        assert!(remoto.len() <= NOME_MAX, "{}", remoto.len());
        assert!(remoto.ends_with(".txt"));
        assert!(remoto.starts_with("20261019-143012-é"));
        // Um caractere de dois bytes nunca é partido: o texto continua válido.
        assert!(remoto.is_char_boundary(remoto.len() - 4));
        // Só extensão enorme: corta tudo, sem estourar.
        let so_extensao = format!("x.{}", "a".repeat(300));
        assert!(nome_remoto(&so_extensao, quando(), false).len() <= NOME_MAX);
    }

    #[test]
    fn nome_da_imagem_tem_o_formato_colado() {
        assert_eq!(nome_da_imagem(quando()), "colado-20261019-143012.png");
    }

    #[test]
    fn sem_colisao_numera_antes_da_extensao() {
        let nenhum: [&str; 0] = [];
        assert_eq!(sem_colisao("x.png", &nenhum), "x.png");
        assert_eq!(sem_colisao("x.png", &["x.png"]), "x-2.png");
        assert_eq!(sem_colisao("x.png", &["x.png", "x-2.png"]), "x-3.png");
        assert_eq!(sem_colisao("pasta", &["pasta"]), "pasta-2");
        assert_eq!(sem_colisao(".bashrc", &[".bashrc"]), ".bashrc-2");
    }

    #[test]
    fn data_do_nome_reconhece_os_dois_formatos() {
        let esperada = DateTime::new(2026, 10, 1, 10, 0, 0, 0).unwrap();
        assert_eq!(data_do_nome("20261001-100000-velho.txt"), Some(esperada));
        assert_eq!(data_do_nome("20261001-100000-pasta"), Some(esperada));
        assert_eq!(data_do_nome("colado-20261001-100000.png"), Some(esperada));
        assert_eq!(data_do_nome("colado-20261001-100000"), Some(esperada));
    }

    #[test]
    fn data_do_nome_recusa_data_falsa_e_nome_alheio() {
        for nome in [
            "20261399-100000-x",
            "20261001-250000-x",
            ".X11-unix",
            "notas.txt",
            "20261001-100000",
            "20261001-100000x",
            "colado-20261001-100000x.png",
            "colado-20261399-100000.png",
            "",
            "é",
        ] {
            assert_eq!(data_do_nome(nome), None, "{nome:?}");
        }
    }

    #[test]
    fn vencidos_so_devolve_os_mais_velhos_que_os_dias() {
        let agora = DateTime::new(2026, 10, 19, 12, 0, 0, 0).unwrap();
        let nomes: Vec<String> = [
            "20261001-100000-velho.txt",
            "colado-20261018-090000.png",
            "notas.txt",
            "20261012-110000-no-limite",
        ]
        .map(String::from)
        .into();
        assert_eq!(
            vencidos(&nomes, agora, 7),
            vec![
                "20261001-100000-velho.txt".to_owned(),
                "20261012-110000-no-limite".to_owned()
            ]
        );
        assert!(vencidos(&nomes, agora, 0).is_empty());
        assert_eq!(vencidos(&nomes, agora, 30), Vec::<String>::new());
    }

    #[test]
    fn caminho_do_windows_no_automatico_usa_aspas_duplas() {
        assert_eq!(
            caminho_para_o_shell("/C:/Users/Ana Maria/x.png", ShellRemoto::Automatico),
            r#""C:\Users\Ana Maria\x.png""#
        );
    }

    #[test]
    fn caminho_do_windows_com_cifrao_vai_de_powershell() {
        assert_eq!(
            caminho_para_o_shell("/C:/Users/Ana/$x y.png", ShellRemoto::Automatico),
            r"'C:\Users\Ana\$x y.png'"
        );
        assert_eq!(
            caminho_para_o_shell("/C:/Users/o'neil/x.png", ShellRemoto::Automatico),
            r"'C:\Users\o''neil\x.png'"
        );
    }

    #[test]
    fn caminho_posix_com_espaco_vai_entre_aspas_simples() {
        assert_eq!(
            caminho_para_o_shell("/tmp/tirech-colar/a b.png", ShellRemoto::Automatico),
            "'/tmp/tirech-colar/a b.png'"
        );
        assert_eq!(
            caminho_para_o_shell("/tmp/tirech-colar/a.png", ShellRemoto::Automatico),
            "/tmp/tirech-colar/a.png"
        );
    }

    #[test]
    fn shell_escolhido_nao_converte_caminho_posix() {
        assert_eq!(
            caminho_para_o_shell("/tmp/a b.png", ShellRemoto::PowerShell),
            "'/tmp/a b.png'"
        );
        assert_eq!(
            caminho_para_o_shell("/C:/Users/ana/x y.png", ShellRemoto::PowerShell),
            r"'C:\Users\ana\x y.png'"
        );
        assert_eq!(
            caminho_para_o_shell("/C:/Users/ana/x y.png", ShellRemoto::Posix),
            "'/C:/Users/ana/x y.png'"
        );
    }

    #[test]
    fn caminho_local_segue_o_sistema() {
        assert_eq!(
            caminho_local_para_o_shell("/home/ana/a b", false),
            "'/home/ana/a b'"
        );
        assert_eq!(
            caminho_local_para_o_shell(r"C:\Users\Ana Maria\x.png", true),
            r#""C:\Users\Ana Maria\x.png""#
        );
        assert_eq!(
            caminho_local_para_o_shell(r"C:\Users\Ana\$x.png", true),
            r"'C:\Users\Ana\$x.png'"
        );
    }

    #[test]
    fn texto_para_colar_cita_cada_caminho() {
        let remotos = vec!["/tmp/a b".to_owned(), "/tmp/c".to_owned()];
        assert_eq!(
            texto_para_colar(&remotos, ShellRemoto::Automatico),
            vec!["'/tmp/a b'".to_owned(), "/tmp/c".to_owned()]
        );
    }

    #[test]
    fn usar_tar_decide_pelo_numero_e_pelo_tamanho() {
        const KIB: u64 = 1024;
        const MIB: u64 = 1024 * KIB;
        assert!(usar_tar(50, 50 * MIB, 50));
        assert!(!usar_tar(49, 49 * MIB, 50));
        assert!(usar_tar(12, 12 * KIB, 50));
        assert!(!usar_tar(9, 9 * KIB, 50));
        assert!(!usar_tar(1000, 1000, 0));
        // Dez arquivos grandes não ganham nada com o pacote.
        assert!(!usar_tar(12, 12 * MIB, 50));
    }

    fn arquivos(lista: &[(&str, u64)]) -> Vec<(String, u64)> {
        lista.iter().map(|(n, b)| ((*n).to_owned(), *b)).collect()
    }

    #[test]
    fn comprimir_fluxo_pula_o_que_ja_vem_comprimido() {
        let imagens = arquivos(&[("a.PNG", 80), ("b.txt", 20)]);
        assert!(!comprimir_fluxo(&imagens, Sondagem::TarGzip, false));
        let texto = arquivos(&[("a.rs", 90), ("b.png", 10)]);
        assert!(comprimir_fluxo(&texto, Sondagem::TarGzip, false));
        assert!(!comprimir_fluxo(&texto, Sondagem::Tar, false));
        assert!(!comprimir_fluxo(&texto, Sondagem::TarGzip, true));
        let limite = arquivos(&[("a.zip", 70), ("b.txt", 30)]);
        assert!(!comprimir_fluxo(&limite, Sondagem::TarGzip, false));
    }

    #[test]
    fn comando_tar_nas_quatro_formas() {
        assert_eq!(
            comando_tar(
                "/tmp/tirech-colar/20261019-143012-projeto",
                ShellRemoto::Automatico,
                true
            ),
            "tar -x -z -o -f - -C '/tmp/tirech-colar/20261019-143012-projeto'"
        );
        assert_eq!(
            comando_tar(
                "/C:/Users/ana/.cache/tirech-colar/20261019-143012-projeto",
                ShellRemoto::Automatico,
                true
            ),
            r#"tar -x -z -o -f - -C "C:\Users\ana\.cache\tirech-colar\20261019-143012-projeto""#
        );
        assert_eq!(
            comando_tar(
                "/C:/Users/ana/.cache/tirech-colar/20261019-143012-projeto",
                ShellRemoto::PowerShell,
                true
            ),
            r"tar -x -z -o -f - -C 'C:\Users\ana\.cache\tirech-colar\20261019-143012-projeto'"
        );
        assert_eq!(
            comando_tar_criando("/tmp/tirech-colar/20261019-143012-projeto", true),
            "umask 077 && mkdir -p -- '/tmp/tirech-colar/20261019-143012-projeto' && tar -x -z -o -f - -C '/tmp/tirech-colar/20261019-143012-projeto'"
        );
    }

    #[test]
    fn comando_tar_sem_gzip_perde_o_z() {
        assert_eq!(
            comando_tar("/tmp/x", ShellRemoto::Posix, false),
            "tar -x -o -f - -C '/tmp/x'"
        );
        assert_eq!(
            comando_tar_criando("/tmp/x", false),
            "umask 077 && mkdir -p -- '/tmp/x' && tar -x -o -f - -C '/tmp/x'"
        );
    }

    #[test]
    fn comando_tar_cita_espaco_e_aspa_em_cada_shell() {
        assert_eq!(
            comando_tar("/tmp/a b", ShellRemoto::Posix, true),
            "tar -x -z -o -f - -C '/tmp/a b'"
        );
        assert_eq!(
            comando_tar("/tmp/o'neil", ShellRemoto::Posix, true),
            r"tar -x -z -o -f - -C '/tmp/o'\''neil'"
        );
        assert_eq!(
            comando_tar("/C:/Users/a b/x", ShellRemoto::Cmd, true),
            r#"tar -x -z -o -f - -C "C:\Users\a b\x""#
        );
        assert_eq!(
            comando_tar("/C:/Users/o'neil/x", ShellRemoto::PowerShell, true),
            r"tar -x -z -o -f - -C 'C:\Users\o''neil\x'"
        );
    }

    #[test]
    fn comando_criar_pasta_cria_so_para_o_dono_e_protege_o_caminho() {
        assert_eq!(
            comando_criar_pasta("/tmp/tirech-colar"),
            "umask 077 && mkdir -p -- '/tmp/tirech-colar'"
        );
        assert_eq!(
            comando_criar_pasta("/tmp/o'neil"),
            r"umask 077 && mkdir -p -- '/tmp/o'\''neil'"
        );
    }

    #[test]
    fn e_windows_so_reconhece_a_letra_da_unidade() {
        assert!(e_windows("/C:/Users/ana"));
        assert!(!e_windows("/tmp/x"));
        assert!(!e_windows("C:/Users"));
    }

    #[test]
    fn apagar_arvore_leva_a_pasta_e_o_que_ha_dentro() {
        let servidor = tomni_ui::files::MemFs::new();
        servidor
            .put("/p/a.txt", "a")
            .put("/p/sub/b.txt", "b")
            .dir("/p/vazia")
            .put("/fica.txt", "f");
        let mut fs = servidor.connect();
        apagar_arvore(&mut fs, "/p").unwrap();
        assert!(fs.lstat("/p").is_err());
        assert!(fs.lstat("/fica.txt").is_ok());
        // O que já não existe não é erro.
        apagar_arvore(&mut fs, "/p").unwrap();
        // Um arquivo solto também.
        apagar_arvore(&mut fs, "/fica.txt").unwrap();
        assert!(fs.lstat("/fica.txt").is_err());
    }
}
