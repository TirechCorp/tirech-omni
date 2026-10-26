//! O ícone do app: os dois desenhos em SVG (o grande, de 32 px para cima, e o pequeno, para 16 e
//! 24 px) e os arquivos que saem deles: `assets/icones/tirech-omni-<n>.png` (o tema `hicolor` do
//! Linux) e `assets/icone.ico` (o executável do Windows). O app não depende dos PNGs em
//! `assets/`: eles existem para o `build.rs` e para o atalho do menu; o teste de referência
//! confere que são os que estes desenhos geram.
//!
//! Para regravar depois de mexer num SVG: `TIRECH_ATUALIZAR_ICONES=1 cargo test -p tirech-omni
//! --lib icone`, e abra os PNGs antes de seguir.

/// O desenho grande, para 32 px ou mais.
pub const GRANDE: &[u8] = include_bytes!("../assets/icone.svg");

/// O desenho pequeno (o fundo e só o `T`-chave, mais grosso), para 16 e 24 px.
pub const PEQUENO: &[u8] = include_bytes!("../assets/icone-pequeno.svg");

/// Os tamanhos do tema de ícones `hicolor` e do `.ico`. É a lista dos tamanhos padrão, não um
/// limite.
pub const TAMANHOS: &[u32] = &[16, 24, 32, 48, 64, 128, 256];

/// O PNG gravado em `assets/icones/` para um dos [`TAMANHOS`], já embutido no executável: o
/// atalho do menu usa estes bytes e não rasteriza o desenho a cada início. Os testes conferem que
/// são os de [`png`].
pub fn png_gravado(tamanho: u32) -> Option<&'static [u8]> {
    Some(match tamanho {
        16 => include_bytes!("../assets/icones/tirech-omni-16.png"),
        24 => include_bytes!("../assets/icones/tirech-omni-24.png"),
        32 => include_bytes!("../assets/icones/tirech-omni-32.png"),
        48 => include_bytes!("../assets/icones/tirech-omni-48.png"),
        64 => include_bytes!("../assets/icones/tirech-omni-64.png"),
        128 => include_bytes!("../assets/icones/tirech-omni-128.png"),
        256 => include_bytes!("../assets/icones/tirech-omni-256.png"),
        _ => return None,
    })
}

/// O PNG do ícone com `tamanho` × `tamanho` pixels: o desenho pequeno abaixo de 32 px, o grande
/// de 32 para cima.
pub fn png(tamanho: u32) -> Vec<u8> {
    let desenho = if tamanho < 32 { PEQUENO } else { GRANDE };
    let (pixels, largura, altura) =
        tomni_ui::rgba(desenho, tamanho).expect("o desenho do ícone é um SVG válido");
    let mut saida = Vec::new();
    let mut codificador = png::Encoder::new(&mut saida, largura, altura);
    codificador.set_color(png::ColorType::Rgba);
    codificador.set_depth(png::BitDepth::Eight);
    let mut escritor = codificador
        .write_header()
        .expect("o cabeçalho do PNG cabe na memória");
    escritor
        .write_image_data(&pixels)
        .expect("os pixels do ícone têm o tamanho do cabeçalho");
    escritor.finish().expect("o PNG fecha na memória");
    saida
}

/// O `.ico` com uma imagem para cada lado de `tamanhos` (de 1 a 256), cada uma como PNG
/// embutido. O formato é simples e vai escrito à mão: o cabeçalho de 6 bytes, uma entrada de 16
/// bytes por imagem e os PNGs, tudo em little-endian.
pub fn ico(tamanhos: &[u32]) -> Vec<u8> {
    let imagens: Vec<Vec<u8>> = tamanhos.iter().map(|&t| png(t)).collect();
    let mut saida = Vec::new();
    saida.extend_from_slice(&0u16.to_le_bytes());
    saida.extend_from_slice(&1u16.to_le_bytes());
    saida.extend_from_slice(&(imagens.len() as u16).to_le_bytes());
    let mut deslocamento = 6 + 16 * imagens.len();
    for (&lado, imagem) in tamanhos.iter().zip(&imagens) {
        // 256 cabe em um byte como 0.
        let byte = if lado >= 256 { 0 } else { lado as u8 };
        saida.extend_from_slice(&[byte, byte, 0, 0]);
        saida.extend_from_slice(&1u16.to_le_bytes());
        saida.extend_from_slice(&32u16.to_le_bytes());
        saida.extend_from_slice(&(imagem.len() as u32).to_le_bytes());
        saida.extend_from_slice(&(deslocamento as u32).to_le_bytes());
        deslocamento += imagem.len();
    }
    for imagem in &imagens {
        saida.extend_from_slice(imagem);
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::{Path, PathBuf};

    fn assets() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
    }

    /// Compara `esperado` com o arquivo gravado; com `TIRECH_ATUALIZAR_ICONES=1`, regrava.
    fn confere(caminho: &Path, esperado: &[u8]) {
        let gravado = std::fs::read(caminho).ok();
        if gravado.as_deref() == Some(esperado) {
            return;
        }
        if std::env::var_os("TIRECH_ATUALIZAR_ICONES").is_some_and(|v| v == "1") {
            std::fs::create_dir_all(caminho.parent().unwrap()).unwrap();
            std::fs::write(caminho, esperado).unwrap();
            return;
        }
        panic!(
            "{} não é o que os desenhos geram. Regrave com \
             `TIRECH_ATUALIZAR_ICONES=1 cargo test -p tirech-omni --lib icone`, abra o arquivo e \
             confira se o desenho mudou de propósito",
            caminho.display()
        );
    }

    #[test]
    fn os_png_gravados_sao_os_que_os_desenhos_geram() {
        for &lado in TAMANHOS {
            confere(
                &assets().join(format!("icones/tirech-omni-{lado}.png")),
                &png(lado),
            );
        }
    }

    #[test]
    fn os_png_embutidos_sao_os_que_os_desenhos_geram() {
        for &lado in TAMANHOS {
            assert_eq!(
                png_gravado(lado).expect("todo tamanho da lista tem PNG"),
                png(lado),
                "{lado} px"
            );
        }
        assert!(png_gravado(20).is_none());
    }

    #[test]
    fn o_ico_gravado_e_o_que_os_desenhos_geram() {
        confere(&assets().join("icone.ico"), &ico(TAMANHOS));
    }

    #[test]
    fn cada_png_tem_o_tamanho_pedido_e_o_desenho_certo() {
        for &lado in TAMANHOS {
            let decodificador = png::Decoder::new(std::io::Cursor::new(png(lado)));
            let mut leitor = decodificador.read_info().unwrap();
            let mut pixels = vec![0; leitor.output_buffer_size().unwrap()];
            let info = leitor.next_frame(&mut pixels).unwrap();
            assert_eq!((info.width, info.height), (lado, lado));
            assert_eq!(info.color_type, png::ColorType::Rgba);
        }
        // Abaixo de 32 px vale o desenho pequeno, que é diferente do grande reduzido.
        let (pequeno, ..) = tomni_ui::rgba(PEQUENO, 24).unwrap();
        let (grande, ..) = tomni_ui::rgba(GRANDE, 24).unwrap();
        assert_ne!(pequeno, grande);
        let (do_png, ..) = tomni_ui::rgba(&png(24), 24).unwrap();
        assert_eq!(do_png, pequeno);
    }

    #[test]
    fn o_ico_tem_o_cabecalho_e_uma_entrada_por_png_valido() {
        let ico = ico(TAMANHOS);
        let u16_em = |i: usize| u16::from_le_bytes([ico[i], ico[i + 1]]);
        let u32_em = |i: usize| u32::from_le_bytes([ico[i], ico[i + 1], ico[i + 2], ico[i + 3]]);
        assert_eq!((u16_em(0), u16_em(2)), (0, 1));
        assert_eq!(usize::from(u16_em(4)), TAMANHOS.len());
        for (i, &lado) in TAMANHOS.iter().enumerate() {
            let entrada = 6 + 16 * i;
            let byte = if lado == 256 { 0 } else { lado as u8 };
            assert_eq!((ico[entrada], ico[entrada + 1]), (byte, byte));
            assert_eq!(ico[entrada + 2], 0, "cores");
            assert_eq!(ico[entrada + 3], 0, "reservado");
            assert_eq!(u16_em(entrada + 4), 1, "planos");
            assert_eq!(u16_em(entrada + 6), 32, "bits");
            let (tamanho, onde) = (u32_em(entrada + 8) as usize, u32_em(entrada + 12) as usize);
            let imagem = &ico[onde..onde + tamanho];
            let leitor = png::Decoder::new(std::io::Cursor::new(imagem))
                .read_info()
                .unwrap();
            assert_eq!(leitor.info().width, lado);
            assert_eq!(leitor.info().height, lado);
        }
    }
}
