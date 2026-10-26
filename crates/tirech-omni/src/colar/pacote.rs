//! O pacote `tar` de uma pasta (M45). O app monta o `tar`, e o gzip, **dentro dele**, com os
//! crates `tar` e `flate2`, e escreve num fluxo qualquer (o canal `exec` da conexão): nenhum
//! programa da máquina local é chamado, e o pacote nunca vai para o disco. Só a extração roda o
//! `tar` do servidor.

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use flate2::Compression;
use flate2::write::GzEncoder;
use tar::{Builder, EntryType, Header};
use tomni_ui::files::{Download, Entry, EntryKind, Fs, path};

/// De quantos em quantos bytes o leitor confere o cancelamento.
const BLOCO: usize = 64 * 1024;

/// O modo de um arquivo comum e de um executável. O `umask` do servidor e a pasta 0700 deixam
/// tudo privado.
const MODO_ARQUIVO: u32 = 0o644;
const MODO_EXECUTAVEL: u32 = 0o755;
const MODO_PASTA: u32 = 0o755;

/// Um pacote vazio (dois blocos de 512 bytes zerados), com ou sem gzip: o que a sondagem manda
/// ao `tar` do servidor para ver se ele lê o fluxo.
pub fn vazio(gzip: bool) -> Vec<u8> {
    let zeros = [0u8; 1024];
    if !gzip {
        return zeros.to_vec();
    }
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    // Escrever num `Vec` não falha.
    let _ = gz.write_all(&zeros);
    gz.finish().unwrap_or_default()
}

/// O caminho de uma entrada dentro do pacote: as partes juntas por `/`. Recusa parte vazia,
/// `.`, `..`, e `/`, `\` ou NUL dentro de uma parte. O `Fs` local não devolve isso, mas um nome
/// assim faria o servidor escrever fora do destino.
pub fn caminho_no_pacote(partes: &[impl AsRef<str>]) -> Result<String, String> {
    if partes.is_empty() {
        return Err("caminho vazio".to_owned());
    }
    for parte in partes {
        let parte = parte.as_ref();
        if parte.is_empty() || parte == "." || parte == ".." {
            return Err(format!("nome inválido no pacote: {parte:?}"));
        }
        if parte.contains(['/', '\\', '\0']) {
            return Err(format!("nome inválido no pacote: {parte:?}"));
        }
    }
    Ok(partes
        .iter()
        .map(AsRef::as_ref)
        .collect::<Vec<&str>>()
        .join("/"))
}

/// Embrulha a saída: depois que `parado` liga, nada mais é escrito. O `Builder` e o
/// `GzEncoder` escrevem o fim do pacote ao serem soltos, e num pacote cancelado ou com erro isso
/// faria o servidor achar que o fluxo acabou bem.
struct Guarda<W> {
    dentro: W,
    parado: Arc<AtomicBool>,
}

impl<W: Write> Write for Guarda<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.parado.load(Ordering::SeqCst) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "pacote interrompido",
            ));
        }
        self.dentro.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.parado.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.dentro.flush()
    }
}

/// Lê um arquivo do `Fs` somando os bytes lidos em `enviados` (antes da compressão) e parando
/// quando `cancelar` liga.
struct Leitor<'a> {
    arquivo: Box<dyn Download + 'a>,
    enviados: &'a AtomicU64,
    cancelar: &'a AtomicBool,
}

impl Read for Leitor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        parar_se_cancelado(self.cancelar)?;
        let limite = buf.len().min(BLOCO);
        let lidos = self.arquivo.read(&mut buf[..limite])?;
        self.enviados.fetch_add(lidos as u64, Ordering::SeqCst);
        Ok(lidos)
    }
}

fn parar_se_cancelado(cancelar: &AtomicBool) -> io::Result<()> {
    if cancelar.load(Ordering::SeqCst) {
        // Não é `Interrupted`: o `io::copy` e o `read_exact` repetem a leitura nesse caso, e o
        // cancelamento viraria um laço sem fim.
        Err(io::Error::other("cancelado"))
    } else {
        Ok(())
    }
}

fn invalido(motivo: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, motivo)
}

/// Escreve em `saida` o pacote de tudo o que há em `raiz`, no `fs` (o disco). Os caminhos são
/// relativos a `raiz`, em ordem de nome. Arquivos e pastas entram; o link para arquivo vira o
/// arquivo, e o link para pasta e os "outros" ficam de fora (para não entrar em ciclo).
///
/// Com `gzip`, o fluxo sai comprimido (nível 1). `enviados` soma os bytes dos arquivos lidos,
/// antes da compressão, e `cancelar` ligado interrompe com um erro (`ErrorKind::Other`, "cancelado") sem
/// escrever mais nada.
pub fn escrever<W: Write>(
    fs: &mut dyn Fs,
    raiz: &str,
    saida: W,
    gzip: bool,
    enviados: &AtomicU64,
    cancelar: &AtomicBool,
) -> io::Result<()> {
    let parado = Arc::new(AtomicBool::new(false));
    let guarda = Guarda {
        dentro: saida,
        parado: Arc::clone(&parado),
    };
    if gzip {
        let codificador = GzEncoder::new(guarda, Compression::fast());
        let codificador = empacotar(fs, raiz, codificador, &parado, enviados, cancelar)?;
        codificador.finish()?;
    } else {
        empacotar(fs, raiz, guarda, &parado, enviados, cancelar)?;
    }
    Ok(())
}

fn empacotar<S: Write>(
    fs: &mut dyn Fs,
    raiz: &str,
    saida: S,
    parado: &AtomicBool,
    enviados: &AtomicU64,
    cancelar: &AtomicBool,
) -> io::Result<S> {
    let mut pacote = Builder::new(saida);
    let mut partes = Vec::new();
    if let Err(erro) = percorrer(&mut pacote, fs, raiz, &mut partes, enviados, cancelar) {
        // Antes de soltar o `Builder`, que escreveria o fim do pacote.
        parado.store(true, Ordering::SeqCst);
        return Err(erro);
    }
    pacote.into_inner()
}

fn percorrer<S: Write>(
    pacote: &mut Builder<S>,
    fs: &mut dyn Fs,
    pasta: &str,
    partes: &mut Vec<String>,
    enviados: &AtomicU64,
    cancelar: &AtomicBool,
) -> io::Result<()> {
    let mut itens = fs.list(pasta)?;
    itens.sort_by(|a, b| a.name.cmp(&b.name));
    for item in itens {
        parar_se_cancelado(cancelar)?;
        let aqui = path::join(pasta, &item.name);
        let item = match item.kind {
            EntryKind::File | EntryKind::Dir => item,
            // Um link para arquivo vira o arquivo; para pasta, ou quebrado, fica de fora.
            EntryKind::Symlink => match fs.stat(&aqui) {
                Ok(alvo) if alvo.is_file() => Entry {
                    name: item.name,
                    ..alvo
                },
                _ => continue,
            },
            EntryKind::Other => continue,
        };
        partes.push(item.name.clone());
        let feito = entrada(pacote, fs, &aqui, partes, &item, enviados, cancelar).and_then(|()| {
            if item.is_dir() {
                percorrer(pacote, fs, &aqui, partes, enviados, cancelar)
            } else {
                Ok(())
            }
        });
        partes.pop();
        feito?;
    }
    Ok(())
}

/// Grava uma entrada PAX (`x`) com um campo só, que vale para a entrada seguinte. O `tar` 0.4
/// não traz isso pronto. Cada registro é `<tamanho> <campo>=<valor>\n`, e o tamanho conta os
/// próprios dígitos.
fn extensao_pax<S: Write>(pacote: &mut Builder<S>, campo: &str, valor: &str) -> io::Result<()> {
    let resto = format!(" {campo}={valor}\n");
    let mut tamanho = resto.len();
    // Os dígitos do tamanho entram na conta; repete até estabilizar (no máximo duas voltas).
    while tamanho != resto.len() + tamanho.to_string().len() {
        tamanho = resto.len() + tamanho.to_string().len();
    }
    let registro = format!("{tamanho}{resto}");
    let mut cabecalho = Header::new_gnu();
    cabecalho.set_entry_type(EntryType::XHeader);
    cabecalho.set_path("PaxHeader/path")?;
    cabecalho.set_mode(MODO_ARQUIVO);
    cabecalho.set_uid(0);
    cabecalho.set_gid(0);
    cabecalho.set_mtime(0);
    cabecalho.set_size(registro.len() as u64);
    cabecalho.set_cksum();
    pacote.append(&cabecalho, registro.as_bytes())
}

/// Grava a entrada de um arquivo ou de uma pasta.
fn entrada<S: Write>(
    pacote: &mut Builder<S>,
    fs: &mut dyn Fs,
    aqui: &str,
    partes: &[String],
    item: &Entry,
    enviados: &AtomicU64,
    cancelar: &AtomicBool,
) -> io::Result<()> {
    let mut nome = caminho_no_pacote(partes).map_err(invalido)?;
    let mut cabecalho = Header::new_gnu();
    cabecalho.set_uid(0);
    cabecalho.set_gid(0);
    let mtime = item
        .modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    cabecalho.set_mtime(mtime);
    if item.is_dir() {
        cabecalho.set_entry_type(EntryType::Directory);
        cabecalho.set_mode(MODO_PASTA);
        cabecalho.set_size(0);
        nome.push('/');
    } else {
        cabecalho.set_entry_type(EntryType::Regular);
        let executavel = item.permissions.is_some_and(|modo| modo & 0o111 != 0);
        cabecalho.set_mode(if executavel {
            MODO_EXECUTAVEL
        } else {
            MODO_ARQUIVO
        });
        cabecalho.set_size(item.size);
    }
    // O GNU tar e o bsdtar leem o nome em UTF-8 sem ajuda, mas um bsdtar no Windows lê pelo
    // código de página da máquina: o `path` do PAX é UTF-8 por definição.
    if !nome.is_ascii() {
        extensao_pax(pacote, "path", nome.trim_end_matches('/'))?;
    }
    if item.is_dir() {
        return pacote.append_data(&mut cabecalho, &nome, io::empty());
    }
    let mut leitor = Leitor {
        arquivo: fs.open_read(aqui, 0)?,
        enviados,
        cancelar,
    };
    pacote.append_data(&mut cabecalho, &nome, &mut leitor)?;
    leitor.arquivo.finish()
}

#[cfg(test)]
mod testes {
    use tomni_ui::files::MemFs;

    use super::*;

    #[test]
    fn vazio_tem_dois_blocos_zerados() {
        assert_eq!(vazio(false), vec![0u8; 1024]);
        let comprimido = vazio(true);
        assert_eq!(&comprimido[..2], &[0x1f, 0x8b]);
        let mut lido = Vec::new();
        flate2::read::GzDecoder::new(&comprimido[..])
            .read_to_end(&mut lido)
            .unwrap();
        assert_eq!(lido, vec![0u8; 1024]);
    }

    #[test]
    fn caminho_no_pacote_recusa_o_que_sai_do_destino() {
        for partes in [
            vec![".."],
            vec!["."],
            vec![""],
            vec!["a", "", "b"],
            vec!["/x"],
            vec!["a\\b"],
            vec!["a\0b"],
            vec!["a", ".."],
        ] {
            assert!(caminho_no_pacote(&partes).is_err(), "{partes:?}");
        }
        assert!(caminho_no_pacote(&[] as &[&str]).is_err());
        assert_eq!(
            caminho_no_pacote(&["a b", "relatório.txt"]).unwrap(),
            "a b/relatório.txt"
        );
    }

    /// Uma pasta de teste: o que o plano pede.
    fn pasta() -> MemFs {
        let fs = MemFs::new();
        let nome_longo = format!("{}.txt", "n".repeat(120));
        fs.put("/p/relatório final.txt", "oi, relatório")
            .put("/p/a b/c.txt", "conteúdo de c")
            .dir("/p/vazia")
            .put("/p/script.sh", "#!/bin/sh\n")
            .put(&format!("/p/{nome_longo}"), "longo")
            .put("/p/alvo.txt", "o alvo")
            .link("/p/atalho.txt", "/p/alvo.txt")
            .dir("/p/outra")
            .put("/p/outra/x.txt", "x")
            .link("/p/atalho-pasta", "/p/outra");
        fs.connect().set_permissions("/p/script.sh", 0o750).unwrap();
        fs
    }

    struct Lido {
        caminho: String,
        tipo: EntryType,
        modo: u32,
        uid: u64,
        mtime: u64,
        dados: Vec<u8>,
    }

    fn ler<R: Read>(fluxo: R) -> Vec<Lido> {
        let mut pacote = tar::Archive::new(fluxo);
        pacote
            .entries()
            .unwrap()
            .map(|entrada| {
                let mut entrada = entrada.unwrap();
                let mut dados = Vec::new();
                entrada.read_to_end(&mut dados).unwrap();
                Lido {
                    caminho: entrada
                        .path()
                        .unwrap()
                        .to_string_lossy()
                        .trim_end_matches('/')
                        .to_owned(),
                    tipo: entrada.header().entry_type(),
                    modo: entrada.header().mode().unwrap(),
                    uid: entrada.header().uid().unwrap(),
                    mtime: entrada.header().mtime().unwrap(),
                    dados,
                }
            })
            .collect()
    }

    fn pacote_de(fs: &MemFs, gzip: bool) -> (Vec<u8>, u64) {
        let enviados = AtomicU64::new(0);
        let mut saida = Vec::new();
        escrever(
            &mut fs.connect(),
            "/p",
            &mut saida,
            gzip,
            &enviados,
            &AtomicBool::new(false),
        )
        .unwrap();
        (saida, enviados.load(Ordering::SeqCst))
    }

    fn achar<'a>(lidos: &'a [Lido], caminho: &str) -> &'a Lido {
        lidos
            .iter()
            .find(|l| l.caminho == caminho)
            .unwrap_or_else(|| panic!("falta {caminho:?}"))
    }

    #[test]
    fn o_tar_volta_com_os_mesmos_caminhos_e_conteudos() {
        let fs = pasta();
        let (saida, _) = pacote_de(&fs, false);
        let lidos = ler(&saida[..]);
        let nome_longo = format!("{}.txt", "n".repeat(120));

        let mut caminhos: Vec<&str> = lidos.iter().map(|l| l.caminho.as_str()).collect();
        caminhos.sort_unstable();
        let mut esperados = vec![
            "a b",
            "a b/c.txt",
            "alvo.txt",
            "atalho.txt",
            nome_longo.as_str(),
            "outra",
            "outra/x.txt",
            "relatório final.txt",
            "script.sh",
            "vazia",
        ];
        esperados.sort_unstable();
        assert_eq!(caminhos, esperados);

        assert_eq!(
            achar(&lidos, "relatório final.txt").dados,
            "oi, relatório".as_bytes()
        );
        assert_eq!(achar(&lidos, "a b/c.txt").dados, "conteúdo de c".as_bytes());
        assert_eq!(achar(&lidos, &nome_longo).dados, b"longo");
        // O link para arquivo virou o arquivo; o link para pasta não aparece.
        let atalho = achar(&lidos, "atalho.txt");
        assert_eq!(atalho.tipo, EntryType::Regular);
        assert_eq!(atalho.dados, b"o alvo");
        assert!(lidos.iter().all(|l| l.caminho != "atalho-pasta"));
        assert!(
            lidos
                .iter()
                .all(|l| !l.caminho.starts_with("atalho-pasta/"))
        );
        // A pasta vazia chega.
        assert_eq!(achar(&lidos, "vazia").tipo, EntryType::Directory);
    }

    #[test]
    fn modos_dono_e_data_sao_os_do_plano() {
        let fs = pasta();
        let (saida, _) = pacote_de(&fs, false);
        let lidos = ler(&saida[..]);
        assert_eq!(achar(&lidos, "relatório final.txt").modo & 0o777, 0o644);
        assert_eq!(achar(&lidos, "script.sh").modo & 0o777, 0o755);
        assert_eq!(achar(&lidos, "a b").modo & 0o777, 0o755);
        assert_eq!(achar(&lidos, "vazia").modo & 0o777, 0o755);
        for l in &lidos {
            assert_eq!(l.uid, 0, "{}", l.caminho);
        }
        let esperada = fs
            .connect()
            .stat("/p/relatório final.txt")
            .unwrap()
            .modified
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(esperada > 0);
        assert_eq!(achar(&lidos, "relatório final.txt").mtime, esperada);
    }

    #[test]
    fn com_gzip_o_fluxo_comeca_por_1f_8b_e_le_igual() {
        let fs = pasta();
        let (puro, _) = pacote_de(&fs, false);
        let (comprimido, _) = pacote_de(&fs, true);
        assert_eq!(&comprimido[..2], &[0x1f, 0x8b]);
        assert_ne!(&puro[..2], &[0x1f, 0x8b]);
        let a = ler(&puro[..]);
        let b = ler(flate2::read::GzDecoder::new(&comprimido[..]));
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((&x.caminho, &x.dados), (&y.caminho, &y.dados));
        }
    }

    #[test]
    fn enviados_soma_os_tamanhos_e_nao_o_fluxo() {
        let fs = MemFs::new();
        fs.put("/p/a.txt", "a".repeat(100_000))
            .put("/p/b/c.txt", "b".repeat(50_000));
        let (comprimido, enviados) = pacote_de(&fs, true);
        assert_eq!(enviados, 150_000);
        assert_ne!(comprimido.len() as u64, enviados);
        let (puro, enviados) = pacote_de(&fs, false);
        assert_eq!(enviados, 150_000);
        assert!(
            puro.len() as u64 > enviados,
            "o tar puro leva os cabeçalhos"
        );
    }

    /// Uma saída que liga o `cancelar` na primeira escrita.
    struct CancelaAoEscrever<'a> {
        cancelar: &'a AtomicBool,
        escritos: usize,
        ligou: bool,
        depois: usize,
    }

    impl Write for CancelaAoEscrever<'_> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.ligou {
                self.depois += buf.len();
            }
            self.escritos += buf.len();
            if !self.ligou {
                self.ligou = true;
                self.cancelar.store(true, Ordering::SeqCst);
            }
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Bytes que o gzip não consegue encolher, para a saída andar durante a leitura.
    fn ruido(tamanho: usize) -> Vec<u8> {
        let mut x: u32 = 0x9e37_79b9;
        (0..tamanho)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (x >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn cancelar_interrompe_e_nao_escreve_o_fim_do_pacote() {
        let fs = MemFs::new();
        fs.put("/p/grande.bin", ruido(400_000));
        for gzip in [false, true] {
            let cancelar = AtomicBool::new(false);
            let enviados = AtomicU64::new(0);
            let mut saida = CancelaAoEscrever {
                cancelar: &cancelar,
                escritos: 0,
                ligou: false,
                depois: 0,
            };
            let erro = escrever(
                &mut fs.connect(),
                "/p",
                &mut saida,
                gzip,
                &enviados,
                &cancelar,
            )
            .unwrap_err();
            assert_eq!(erro.kind(), io::ErrorKind::Other, "gzip={gzip}");
            // Parou antes do fim do arquivo, e nem o fim do pacote saiu.
            assert!(saida.escritos < 300_000, "gzip={gzip}: {}", saida.escritos);
            assert!(enviados.load(Ordering::SeqCst) < 400_000, "gzip={gzip}");
            if !gzip {
                // O cabeçalho foi a primeira escrita; depois dele, nada.
                assert_eq!(saida.depois, 0);
            }
        }
    }

    #[test]
    fn cancelar_antes_de_comecar_nao_escreve_nada() {
        let fs = pasta();
        let mut saida = Vec::new();
        let erro = escrever(
            &mut fs.connect(),
            "/p",
            &mut saida,
            false,
            &AtomicU64::new(0),
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert_eq!(erro.kind(), io::ErrorKind::Other);
        assert!(saida.is_empty());
    }

    #[test]
    fn nome_fora_do_ascii_leva_a_extensao_pax() {
        let fs = MemFs::new();
        fs.put("/p/ação/relatório.txt", "x");
        let (saida, _) = pacote_de(&fs, false);
        // O cabeçalho PAX (tipo `x`) carrega o `path` em UTF-8.
        let texto = String::from_utf8_lossy(&saida);
        assert!(texto.contains("path=ação/relatório.txt"), "{texto:?}");
        let lidos = ler(&saida[..]);
        assert_eq!(achar(&lidos, "ação/relatório.txt").dados, b"x");
    }

    #[test]
    fn pasta_sem_nada_vira_um_pacote_vazio() {
        let fs = MemFs::new();
        fs.dir("/p");
        let (saida, enviados) = pacote_de(&fs, false);
        assert_eq!(enviados, 0);
        assert_eq!(saida, vec![0u8; 1024]);
    }
}
