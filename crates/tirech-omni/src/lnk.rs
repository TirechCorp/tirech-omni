//! O atalho `.lnk` do Windows (M65), escrito e lido sem COM e sem `unsafe`.
//!
//! O formato é o MS-SHLLINK, o mínimo que o Explorer aceita: o cabeçalho de 0x4C bytes, o
//! `LinkTargetIDList` simples do destino (Meu Computador, a unidade e cada pasta e arquivo, pelo
//! nome em ANSI), o `LinkInfo` com o caminho local (em ANSI e em Unicode) e os textos (nome, pasta
//! de trabalho, argumentos e ícone) em UTF-16. O `LinkInfo` sozinho não basta: no Windows 10 o
//! Explorer não abre um atalho sem `LinkTargetIDList` (confirmado na VM). O Windows prefere a
//! lista; se o caminho dela não existir (nome com caracteres fora da página de código), cai no
//! `LinkInfo`, que tem o caminho certo em Unicode.
//!
//! [`gerar`] devolve os bytes; [`ler`] lê de volta o destino de qualquer `.lnk` (inclusive os que o
//! próprio Windows gerou, como os fixados da barra de tarefas).

/// O `LinkCLSID` do cabeçalho: `00021401-0000-0000-C000-000000000046`.
const CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

/// O tamanho do `ShellLinkHeader`.
const TAMANHO_DO_CABECALHO: u32 = 0x4C;

const TEM_ID_LIST: u32 = 0x01;
const TEM_LINK_INFO: u32 = 0x02;
const TEM_NOME: u32 = 0x04;
const TEM_CAMINHO_RELATIVO: u32 = 0x08;
const TEM_PASTA_DE_TRABALHO: u32 = 0x10;
const TEM_ARGUMENTOS: u32 = 0x20;
const TEM_ICONE: u32 = 0x40;
const E_UNICODE: u32 = 0x80;

/// Tamanho do cabeçalho do `LinkInfo` com os campos Unicode.
const CABECALHO_DO_LINK_INFO: u32 = 0x24;

/// O que um atalho guarda.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Atalho {
    /// O arquivo que o atalho abre (`C:\Users\ana\AppData\Local\Programs\Tirech Omni\tirech-omni.exe`).
    pub alvo: String,
    /// Os argumentos da linha de comando.
    pub argumentos: String,
    /// A pasta de trabalho.
    pub pasta: String,
    /// O arquivo do ícone (o índice é sempre 0).
    pub icone: String,
    /// A descrição (a dica do atalho).
    pub nome: String,
}

fn u16_le(bytes: &mut Vec<u8>, valor: u16) {
    bytes.extend_from_slice(&valor.to_le_bytes());
}

fn u32_le(bytes: &mut Vec<u8>, valor: u32) {
    bytes.extend_from_slice(&valor.to_le_bytes());
}

/// O texto em ANSI: o que não é ASCII vira `?` (o caminho certo vai também em Unicode).
fn ansi(texto: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = texto
        .chars()
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    bytes.push(0);
    bytes
}

/// O texto em UTF-16LE com o terminador nulo.
fn utf16_com_nulo(texto: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = texto.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

/// Um `StringData`: o tamanho em caracteres (`u16`) e o texto em UTF-16LE, sem terminador.
fn texto_de_dados(bytes: &mut Vec<u8>, texto: &str) {
    let unidades: Vec<u16> = texto.encode_utf16().collect();
    u16_le(bytes, u16::try_from(unidades.len()).unwrap_or(u16::MAX));
    for unidade in unidades.iter().take(usize::from(u16::MAX)) {
        u16_le(bytes, *unidade);
    }
}

/// Um item da `LinkTargetIDList` de arquivo (`0x32`) ou pasta (`0x31`): o nome em ANSI (o que
/// não é ASCII vira `?`; o `LinkInfo` guarda o caminho certo).
fn item_de_arquivo(pasta: bool, nome: &str) -> Vec<u8> {
    let mut corpo = Vec::with_capacity(16 + nome.len());
    corpo.push(if pasta { 0x31 } else { 0x32 });
    corpo.push(0);
    u32_le(&mut corpo, 0); // tamanho do arquivo
    u16_le(&mut corpo, 0x5821); // data no formato do DOS (2024-01-01)
    u16_le(&mut corpo, 0); // hora
    u16_le(&mut corpo, if pasta { 0x10 } else { 0x20 }); // atributos
    corpo.extend(ansi(nome));
    if corpo.len() % 2 != 0 {
        corpo.push(0);
    }
    let mut item = Vec::with_capacity(corpo.len() + 2);
    u16_le(&mut item, (corpo.len() + 2) as u16);
    item.extend(corpo);
    item
}

/// A `LinkTargetIDList` de um caminho de unidade (`C:\pasta\arquivo.exe`), com o tamanho na
/// frente. `None` para o que não tem unidade (caminho de rede, relativo) ou não cabe nos 64 KiB.
fn id_list(alvo: &str) -> Option<Vec<u8>> {
    let partes: Vec<&str> = alvo.split(['\\', '/']).collect();
    let unidade = partes.first()?;
    let [letra, b':'] = unidade.as_bytes() else {
        return None;
    };
    if !letra.is_ascii_alphabetic()
        || partes.len() < 2
        || partes.iter().skip(1).any(|p| p.is_empty())
    {
        return None;
    }
    let mut itens: Vec<u8> = Vec::new();
    // Meu Computador (20D04FE0-3AEA-1069-A2D8-08002B30309D).
    u16_le(&mut itens, 0x14);
    itens.extend_from_slice(&[0x1F, 0x50]);
    itens.extend_from_slice(&[
        0xE0, 0x4F, 0xD0, 0x20, 0xEA, 0x3A, 0x69, 0x10, 0xA2, 0xD8, 0x08, 0x00, 0x2B, 0x30, 0x30,
        0x9D,
    ]);
    // A unidade: `C:\` com enchimento até os 0x19 bytes.
    u16_le(&mut itens, 0x19);
    itens.push(0x2F);
    itens.push(*letra);
    itens.extend_from_slice(b":\\");
    itens.extend_from_slice(&[0u8; 19]);
    let ultimo = partes.len() - 1;
    for (i, parte) in partes.iter().enumerate().skip(1) {
        itens.extend(item_de_arquivo(i != ultimo, parte));
    }
    u16_le(&mut itens, 0); // TerminalID
    let tamanho = u16::try_from(itens.len()).ok()?;
    let mut bytes = Vec::with_capacity(itens.len() + 2);
    u16_le(&mut bytes, tamanho);
    bytes.extend(itens);
    Some(bytes)
}

/// O `LinkInfo` de um destino em disco fixo: `VolumeID` sem rótulo e o caminho local.
fn link_info(alvo: &str) -> Vec<u8> {
    let volume_id_tamanho: u32 = 0x11;
    let base_ansi = ansi(alvo);
    let base_unicode = utf16_com_nulo(alvo);
    let sufixo_ansi = [0u8];
    let sufixo_unicode = [0u8, 0u8];

    let volume_id = CABECALHO_DO_LINK_INFO;
    let base_local = volume_id + volume_id_tamanho;
    let sufixo = base_local + base_ansi.len() as u32;
    let base_local_unicode = sufixo + sufixo_ansi.len() as u32;
    let sufixo_unicode_pos = base_local_unicode + base_unicode.len() as u32;
    let total = sufixo_unicode_pos + sufixo_unicode.len() as u32;

    let mut bytes = Vec::with_capacity(total as usize);
    u32_le(&mut bytes, total);
    u32_le(&mut bytes, CABECALHO_DO_LINK_INFO);
    u32_le(&mut bytes, 1); // VolumeIDAndLocalBasePath
    u32_le(&mut bytes, volume_id);
    u32_le(&mut bytes, base_local);
    u32_le(&mut bytes, 0); // CommonNetworkRelativeLinkOffset
    u32_le(&mut bytes, sufixo);
    u32_le(&mut bytes, base_local_unicode);
    u32_le(&mut bytes, sufixo_unicode_pos);
    // VolumeID: disco fixo, série 0, rótulo vazio (logo depois dos 0x10 bytes do cabeçalho).
    u32_le(&mut bytes, volume_id_tamanho);
    u32_le(&mut bytes, 3); // DRIVE_FIXED
    u32_le(&mut bytes, 0);
    u32_le(&mut bytes, 0x10);
    bytes.push(0);
    bytes.extend_from_slice(&base_ansi);
    bytes.extend_from_slice(&sufixo_ansi);
    bytes.extend_from_slice(&base_unicode);
    bytes.extend_from_slice(&sufixo_unicode);
    debug_assert_eq!(bytes.len() as u32, total);
    bytes
}

/// Os bytes de um `.lnk` que abre `atalho.alvo`.
pub fn gerar(atalho: &Atalho) -> Vec<u8> {
    let mut flags = TEM_LINK_INFO | E_UNICODE;
    let lista = id_list(&atalho.alvo);
    if lista.is_some() {
        flags |= TEM_ID_LIST;
    }
    if !atalho.nome.is_empty() {
        flags |= TEM_NOME;
    }
    if !atalho.pasta.is_empty() {
        flags |= TEM_PASTA_DE_TRABALHO;
    }
    if !atalho.argumentos.is_empty() {
        flags |= TEM_ARGUMENTOS;
    }
    if !atalho.icone.is_empty() {
        flags |= TEM_ICONE;
    }
    let mut bytes = Vec::with_capacity(512);
    u32_le(&mut bytes, TAMANHO_DO_CABECALHO);
    bytes.extend_from_slice(&CLSID);
    u32_le(&mut bytes, flags);
    u32_le(&mut bytes, 0x20); // FILE_ATTRIBUTE_ARCHIVE
    bytes.extend_from_slice(&[0u8; 24]); // as três datas
    u32_le(&mut bytes, 0); // FileSize
    u32_le(&mut bytes, 0); // IconIndex
    u32_le(&mut bytes, 1); // SW_SHOWNORMAL
    u16_le(&mut bytes, 0); // HotKey
    u16_le(&mut bytes, 0); // Reserved1
    u32_le(&mut bytes, 0); // Reserved2
    u32_le(&mut bytes, 0); // Reserved3
    debug_assert_eq!(bytes.len() as u32, TAMANHO_DO_CABECALHO);
    if let Some(lista) = lista {
        bytes.extend(lista);
    }
    bytes.extend(link_info(&atalho.alvo));
    if flags & TEM_NOME != 0 {
        texto_de_dados(&mut bytes, &atalho.nome);
    }
    if flags & TEM_PASTA_DE_TRABALHO != 0 {
        texto_de_dados(&mut bytes, &atalho.pasta);
    }
    if flags & TEM_ARGUMENTOS != 0 {
        texto_de_dados(&mut bytes, &atalho.argumentos);
    }
    if flags & TEM_ICONE != 0 {
        texto_de_dados(&mut bytes, &atalho.icone);
    }
    u32_le(&mut bytes, 0); // TerminalBlock
    bytes
}

/// Um leitor com limites: nenhuma leitura passa do fim dos bytes.
struct Leitor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Leitor<'a> {
    fn pegar(&mut self, n: usize) -> Option<&'a [u8]> {
        let fim = self.pos.checked_add(n)?;
        let parte = self.bytes.get(self.pos..fim)?;
        self.pos = fim;
        Some(parte)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.pegar(2)?.try_into().ok()?))
    }

    fn texto(&mut self, unicode: bool) -> Option<String> {
        let n = usize::from(self.u16()?);
        if unicode {
            let bruto = self.pegar(n.checked_mul(2)?)?;
            let unidades: Vec<u16> = bruto
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect();
            Some(String::from_utf16_lossy(&unidades))
        } else {
            Some(String::from_utf8_lossy(self.pegar(n)?).into_owned())
        }
    }
}

fn u32_em(bytes: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(pos..pos.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// O texto nulo-terminado que começa em `pos`, em ANSI.
fn ansi_em(bytes: &[u8], pos: usize) -> Option<String> {
    let resto = bytes.get(pos..)?;
    let fim = resto.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&resto[..fim]).into_owned())
}

/// O texto nulo-terminado que começa em `pos`, em UTF-16LE.
fn utf16_em(bytes: &[u8], pos: usize) -> Option<String> {
    let resto = bytes.get(pos..)?;
    let unidades: Vec<u16> = resto
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
        .take_while(|&u| u != 0)
        .collect();
    Some(String::from_utf16_lossy(&unidades))
}

/// Lê o destino, os argumentos, a pasta, o ícone e a descrição de um `.lnk`. `None` se os bytes
/// não são um atalho, ou se ele não guarda o caminho em `LinkInfo` (um atalho só com
/// `LinkTargetIDList`, como o de um aplicativo da Loja).
pub fn ler(bytes: &[u8]) -> Option<Atalho> {
    if u32_em(bytes, 0)? != TAMANHO_DO_CABECALHO || bytes.get(4..20)? != CLSID {
        return None;
    }
    let flags = u32_em(bytes, 20)?;
    let unicode = flags & E_UNICODE != 0;
    let mut leitor = Leitor {
        bytes,
        pos: TAMANHO_DO_CABECALHO as usize,
    };
    if flags & TEM_ID_LIST != 0 {
        let n = usize::from(leitor.u16()?);
        leitor.pegar(n)?;
    }
    let mut alvo = String::new();
    if flags & TEM_LINK_INFO != 0 {
        let inicio = leitor.pos;
        let total = u32_em(bytes, inicio)? as usize;
        let cabecalho = u32_em(bytes, inicio + 4)? as usize;
        let info_flags = u32_em(bytes, inicio + 8)?;
        let info = bytes.get(inicio..inicio.checked_add(total)?)?;
        if info_flags & 1 != 0 {
            let base = u32_em(info, 16)? as usize;
            let sufixo = u32_em(info, 24)? as usize;
            let (base_texto, sufixo_texto) = if cabecalho >= CABECALHO_DO_LINK_INFO as usize {
                let base_u = u32_em(info, 28)? as usize;
                let sufixo_u = u32_em(info, 32)? as usize;
                (utf16_em(info, base_u)?, utf16_em(info, sufixo_u)?)
            } else {
                (ansi_em(info, base)?, ansi_em(info, sufixo)?)
            };
            alvo = format!("{base_texto}{sufixo_texto}");
        }
        leitor.pegar(total)?;
    }
    let mut atalho = Atalho {
        alvo,
        ..Atalho::default()
    };
    if flags & TEM_NOME != 0 {
        atalho.nome = leitor.texto(unicode)?;
    }
    if flags & TEM_CAMINHO_RELATIVO != 0 {
        leitor.texto(unicode)?;
    }
    if flags & TEM_PASTA_DE_TRABALHO != 0 {
        atalho.pasta = leitor.texto(unicode)?;
    }
    if flags & TEM_ARGUMENTOS != 0 {
        atalho.argumentos = leitor.texto(unicode)?;
    }
    if flags & TEM_ICONE != 0 {
        atalho.icone = leitor.texto(unicode)?;
    }
    (!atalho.alvo.is_empty()).then_some(atalho)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exemplo() -> Atalho {
        Atalho {
            alvo: r"C:\Users\ana\AppData\Local\Programs\Tirech Omni\tirech-omni.exe".to_owned(),
            argumentos: String::new(),
            pasta: r"C:\Users\ana\AppData\Local\Programs\Tirech Omni".to_owned(),
            icone: r"C:\Users\ana\AppData\Local\Programs\Tirech Omni\tirech-omni.exe".to_owned(),
            nome: "Tirech Omni".to_owned(),
        }
    }

    #[test]
    fn o_cabecalho_tem_o_tamanho_o_clsid_e_as_flags_do_formato() {
        let bytes = gerar(&exemplo());
        assert_eq!(&bytes[..4], &[0x4C, 0, 0, 0]);
        assert_eq!(
            &bytes[4..20],
            &[
                0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x46
            ]
        );
        // HasLinkTargetIDList | HasLinkInfo | HasName | HasWorkingDir | HasIconLocation | IsUnicode
        // (sem argumentos).
        assert_eq!(
            u32_em(&bytes, 20),
            Some(0x01 | 0x02 | 0x04 | 0x10 | 0x40 | 0x80)
        );
        assert_eq!(u32_em(&bytes, 24), Some(0x20), "FileAttributes");
        assert_eq!(u32_em(&bytes, 60), Some(1), "ShowCommand");
        // O bloco terminal fecha o arquivo.
        assert_eq!(&bytes[bytes.len() - 4..], &[0, 0, 0, 0]);
    }

    #[test]
    fn o_link_info_tem_os_deslocamentos_certos() {
        let bytes = gerar(&exemplo());
        let lista = usize::from(u16::from_le_bytes([bytes[0x4C], bytes[0x4D]]));
        let info = &bytes[0x4C + 2 + lista..];
        let total = u32_em(info, 0).unwrap() as usize;
        assert_eq!(u32_em(info, 4), Some(0x24), "cabeçalho do LinkInfo");
        assert_eq!(u32_em(info, 8), Some(1), "VolumeIDAndLocalBasePath");
        assert_eq!(u32_em(info, 12), Some(0x24), "VolumeIDOffset");
        let base = u32_em(info, 16).unwrap() as usize;
        assert_eq!(base, 0x24 + 0x11);
        assert_eq!(
            ansi_em(info, base).as_deref(),
            Some(exemplo().alvo.as_str())
        );
        let base_unicode = u32_em(info, 28).unwrap() as usize;
        assert_eq!(
            utf16_em(info, base_unicode).as_deref(),
            Some(exemplo().alvo.as_str())
        );
        assert!(total < info.len(), "ainda há os StringData depois");
        // O VolumeID: tamanho, disco fixo, série 0 e o rótulo logo depois do cabeçalho.
        assert_eq!(u32_em(info, 0x24), Some(0x11));
        assert_eq!(u32_em(info, 0x28), Some(3));
        assert_eq!(u32_em(info, 0x30), Some(0x10));
    }

    #[test]
    fn a_lista_de_destino_tem_o_computador_a_unidade_as_pastas_e_o_arquivo() {
        let bytes = gerar(&Atalho {
            alvo: r"C:\m65\tirech-omni.exe".to_owned(),
            ..Atalho::default()
        });
        let tamanho = usize::from(u16::from_le_bytes([bytes[0x4C], bytes[0x4D]]));
        let lista = &bytes[0x4E..0x4E + tamanho];
        // Meu Computador: 0x14 bytes, tipo 0x1F e o CLSID.
        assert_eq!(&lista[..4], &[0x14, 0, 0x1F, 0x50]);
        assert_eq!(&lista[4..8], &[0xE0, 0x4F, 0xD0, 0x20]);
        // A unidade: 0x19 bytes, tipo 0x2F e `C:\`.
        assert_eq!(&lista[0x14..0x14 + 6], &[0x19, 0, 0x2F, b'C', b':', b'\\']);
        // A pasta e o arquivo, e o item nulo que fecha.
        let resto = &lista[0x14 + 0x19..];
        assert_eq!(resto[2], 0x31);
        assert!(resto.windows(4).any(|j| j == b"m65\0"));
        assert!(resto.windows(16).any(|j| j == b"tirech-omni.exe\0"));
        assert_eq!(&lista[lista.len() - 2..], &[0, 0]);
        // Sem unidade (rede, relativo): sem lista, só o `LinkInfo`.
        assert!(id_list(r"\\servidor\pasta\x.exe").is_none());
        assert!(id_list("relativo.exe").is_none());
        assert!(id_list(r"C:\").is_none());
    }

    #[test]
    fn le_de_volta_o_que_escreveu() {
        let mut atalho = exemplo();
        atalho.argumentos = "--bandeja".to_owned();
        assert_eq!(ler(&gerar(&atalho)), Some(atalho));
    }

    #[test]
    fn caminho_com_acento_e_espaco_volta_inteiro() {
        let atalho = Atalho {
            alvo: r"C:\Users\João da Silva\AppData\Local\Programs\Tirech Omni\tirech-omni.exe"
                .to_owned(),
            nome: "Tirech Omni — terminal".to_owned(),
            ..Atalho::default()
        };
        let bytes = gerar(&atalho);
        // O ANSI perde o acento (vira `?`); o Unicode é o que vale na leitura.
        assert!(ler(&bytes).is_some_and(|lido| lido == atalho));
    }

    #[test]
    fn bytes_que_nao_sao_um_atalho_dao_none() {
        assert_eq!(ler(&[]), None);
        assert_eq!(ler(b"nao sou um lnk, sou so um texto qualquer"), None);
        let mut bytes = gerar(&exemplo());
        bytes[4] = 0xFF;
        assert_eq!(ler(&bytes), None);
        // Cortado no meio: não passa dos limites.
        let inteiro = gerar(&exemplo());
        for corte in [10, 80, 100, 150, inteiro.len() - 6] {
            let _ = ler(&inteiro[..corte]);
        }
    }

    #[test]
    fn atalho_com_id_list_do_windows_acha_o_destino_no_link_info() {
        // Um atalho com `LinkTargetIDList` de 4 bytes de enchimento na frente do `LinkInfo`
        // (o destino sem unidade não ganha lista própria).
        let atalho = Atalho {
            alvo: r"\\servidor\pasta\tirech-omni.exe".to_owned(),
            ..Atalho::default()
        };
        let mut bytes = gerar(&atalho);
        assert_eq!(u32_em(&bytes, 20).unwrap() & TEM_ID_LIST, 0);
        let flags = u32_em(&bytes, 20).unwrap() | TEM_ID_LIST;
        bytes[20..24].copy_from_slice(&flags.to_le_bytes());
        let id_list = [4u8, 0, 1, 2, 3, 4];
        bytes.splice(0x4C..0x4C, id_list);
        assert_eq!(ler(&bytes).map(|a| a.alvo), Some(atalho.alvo));
    }

    /// O Windows de verdade resolve o `.lnk` que o app grava (só roda no Windows). Sem a
    /// `LinkTargetIDList` o `TargetPath` volta vazio e o Explorer não abre o atalho.
    #[cfg(windows)]
    #[test]
    fn o_windows_resolve_o_atalho_que_o_app_grava() {
        let pasta = std::env::temp_dir().join(format!("tirech-lnk-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let exe = pasta.join("tirech omni teste.exe");
        std::fs::write(&exe, b"x").unwrap();
        let arquivo = pasta.join("atalho.lnk");
        let atalho = Atalho {
            alvo: exe.display().to_string(),
            pasta: pasta.display().to_string(),
            ..Atalho::default()
        };
        std::fs::write(&arquivo, gerar(&atalho)).unwrap();
        let saida = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(format!(
                "(New-Object -ComObject WScript.Shell).CreateShortcut('{}').TargetPath",
                arquivo.display()
            ))
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&pasta);
        let texto = String::from_utf8_lossy(&saida.stdout);
        assert_eq!(texto.trim(), atalho.alvo);
    }
}
