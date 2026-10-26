//! As notas em Markdown (M47): o que não depende da tela. Modelos de nota, nomes de arquivo,
//! texto dos links de anexo, trecho do filtro, a troca dos destinos ao exportar e ao importar, e
//! a exportação e a importação de arquivos e pastas `.md`.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tomni_ui::markdown::links;

use crate::anexos::{self, ErroAnexo, Progresso};
use crate::busca::normalizar;
use crate::cofre::{Anexo, Cofre, Item, Lote, Pasta, Tipo, nome_de_anexo_unico};
use crate::recentes::{agora, data_civil};
use crate::t;

/// A marca do lugar do cursor nos modelos; o código a tira e devolve a posição.
const CURSOR: char = '\u{1}';

/// Os pontos que o trecho do filtro mostra no lugar de um bloco `segredo`.
pub(crate) const MASCARA: &str = "••••••••";

/// O sufixo da pasta dos anexos de uma nota exportada: `<título>-attachments`.
const SUFIXO_DE_ANEXOS: &str = "-attachments";

/// A profundidade máxima de pastas que a importação percorre (um laço de links simbólicos não
/// pode prendê-la).
const PROFUNDIDADE_MAXIMA: usize = 32;

// ---------------------------------------------------------------------------------------------
// Modelos
// ---------------------------------------------------------------------------------------------

/// Um ponto de partida para uma nota nova.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modelo {
    /// Uma nota vazia.
    Branco,
    /// A ficha de um servidor.
    Servidor,
    /// Uma chave ou um certificado.
    Chave,
    /// Um procedimento passo a passo.
    Procedimento,
}

/// O texto de um modelo e onde o cursor começa (em bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Corpo {
    /// O texto da nota.
    pub texto: String,
    /// A posição do cursor em `texto`.
    pub cursor: usize,
}

impl Modelo {
    /// Os modelos, na ordem do menu.
    pub const TODOS: [Modelo; 4] = [
        Modelo::Branco,
        Modelo::Servidor,
        Modelo::Chave,
        Modelo::Procedimento,
    ];

    /// O nome no menu "Nova nota".
    pub fn rotulo(self) -> String {
        match self {
            Modelo::Branco => t!("modelo-em-branco"),
            Modelo::Servidor => t!("modelo-servidor"),
            Modelo::Chave => t!("modelo-chave"),
            Modelo::Procedimento => t!("modelo-procedimento"),
        }
    }

    /// O final do id do item de menu (`notas-modelo-<id>`).
    pub fn id(self) -> &'static str {
        match self {
            Modelo::Branco => "branco",
            Modelo::Servidor => "servidor",
            Modelo::Chave => "chave",
            Modelo::Procedimento => "procedimento",
        }
    }

    /// O título sugerido para a nota nova.
    pub fn titulo(self) -> String {
        match self {
            Modelo::Branco => t!("notas-nova-nota"),
            Modelo::Servidor => t!("modelo-titulo-servidor"),
            Modelo::Chave => t!("modelo-titulo-chave"),
            Modelo::Procedimento => t!("modelo-titulo-procedimento"),
        }
    }

    /// O texto do modelo, com o cursor no primeiro lugar de escrever.
    pub fn corpo(self) -> Corpo {
        let marcado = match self {
            Modelo::Branco => String::new(),
            Modelo::Servidor => servidor_marcado("", 22, ""),
            Modelo::Chave => format!(
                "# {}\n\n\u{1}\n\n# {}\n\n- \n\n# {}\n\n\n\n# {}\n\n{}\n\n```segredo\n\n```\n",
                t!("md-para-que-serve"),
                t!("md-onde-esta-instalada"),
                t!("md-validade"),
                t!("md-chave"),
                t!("md-cole-a-chave"),
            ),
            Modelo::Procedimento => format!(
                "# {}\n\n- [ ] \u{1}\n- [ ] \n- [ ] \n\n# {}\n\n```sh\n\n```\n\n# {}\n\n",
                t!("md-passos"),
                t!("md-comandos"),
                t!("md-se-der-errado"),
            ),
        };
        tirar_cursor(&marcado)
    }
}

/// O modelo de servidor, com o endereço, a porta e o usuário que já se sabe. O cursor vai para o
/// primeiro campo ainda vazio.
fn servidor_marcado(endereco: &str, porta: u16, usuario: &str) -> String {
    let porta = if porta == 0 { 22 } else { porta };
    let endereco = endereco.trim();
    let usuario = usuario.trim();
    // O cursor vai ao primeiro campo que falta; com os dois preenchidos, ao bloco do segredo.
    let (c_endereco, c_usuario, c_segredo) = if endereco.is_empty() {
        (CURSOR.to_string(), String::new(), String::new())
    } else if usuario.is_empty() {
        (String::new(), CURSOR.to_string(), String::new())
    } else {
        (String::new(), String::new(), CURSOR.to_string())
    };
    format!(
        "# {}\n\n- {}: {endereco}{c_endereco}\n- {}: {porta}\n- {}: {usuario}{c_usuario}\n\n\
         ```segredo\n{c_segredo}\n```\n\n# {}\n\n- [ ] \n\n# {}\n\n",
        t!("md-acesso"),
        t!("md-endereco"),
        t!("md-porta"),
        t!("md-usuario"),
        t!("md-servicos"),
        t!("md-observacoes"),
    )
}

/// Tira a marca do cursor do texto e devolve o texto e onde ela estava (o fim, sem marca).
fn tirar_cursor(marcado: &str) -> Corpo {
    match marcado.find(CURSOR) {
        Some(posicao) => Corpo {
            texto: marcado.replace(CURSOR, ""),
            cursor: posicao,
        },
        None => Corpo {
            texto: marcado.to_owned(),
            cursor: marcado.len(),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// Nomes e textos pequenos
// ---------------------------------------------------------------------------------------------

/// Um nome que serve de arquivo em qualquer sistema: troca `/ \ : * ? " < > |` e os controles por
/// `-`, tira pontos e espaços do fim, põe `_` depois dos nomes reservados do Windows e corta em
/// 120 caracteres. Vazio vira "Sem título".
pub fn nome_de_arquivo(nome: &str) -> String {
    let trocado: String = nome
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let limpo = trocado.trim_end_matches(['.', ' ']);
    if limpo.is_empty() {
        return t!("notas-sem-titulo");
    }
    let (base, resto) = match limpo.find('.') {
        Some(i) => limpo.split_at(i),
        None => (limpo, ""),
    };
    if e_nome_reservado(base) {
        format!("{base}_{resto}")
    } else {
        limpo.to_owned()
    }
}

fn e_nome_reservado(base: &str) -> bool {
    let maiusculo = base.to_ascii_uppercase();
    if matches!(maiusculo.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    maiusculo
        .strip_prefix("COM")
        .or_else(|| maiusculo.strip_prefix("LPT"))
        .is_some_and(|numero| matches!(numero.as_bytes(), [b'1'..=b'9']))
}

/// O nome da imagem colada: `colado-AAAAMMDD-HHMMSS.png`, em UTC (o app não conhece o fuso do
/// sistema), a partir dos segundos desde 1970.
pub fn nome_colado(segundos: u64) -> String {
    let (ano, mes, dia) = data_civil(segundos / 86_400);
    let resto = segundos % 86_400;
    format!(
        "colado-{ano:04}{mes:02}{dia:02}-{:02}{:02}{:02}.png",
        resto / 3_600,
        resto % 3_600 / 60,
        resto % 60
    )
}

/// O nome da imagem colada agora.
pub fn nome_colado_agora() -> String {
    nome_colado(agora())
}

/// Se o nome é de uma imagem que a nota mostra, pela extensão (sem diferenciar a caixa).
pub fn e_imagem(nome: &str) -> bool {
    nome.rsplit_once('.').is_some_and(|(_, extensao)| {
        matches!(
            extensao.to_ascii_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp"
        )
    })
}

/// O texto do link de um anexo: `![nome](anexo:<id>)` para imagem e `[nome](anexo:<id>)` para o
/// resto. `[`, `]` e `\` do nome ganham uma barra antes.
pub fn link_do_anexo(nome: &str, id: &str) -> String {
    let mut texto = String::with_capacity(nome.len() + id.len() + 12);
    if e_imagem(nome) {
        texto.push('!');
    }
    texto.push('[');
    for c in nome.chars() {
        if matches!(c, '[' | ']' | '\\') {
            texto.push('\\');
        }
        texto.push(c);
    }
    texto.push_str("](anexo:");
    texto.push_str(id);
    texto.push(')');
    texto
}

/// Quantas palavras o texto tem (separadas por espaço ou quebra de linha).
pub fn contar_palavras(texto: &str) -> usize {
    texto.split_whitespace().count()
}

/// Onde estão os blocos ` ```segredo `: a faixa em bytes do que fica entre a cerca de abrir e a
/// de fechar (sem as duas linhas das cercas). Um bloco sem cerca de fechar vai até o fim.
pub fn blocos_segredo(corpo: &str) -> Vec<Range<usize>> {
    let mut blocos = Vec::new();
    // `Some(true)` dentro de um bloco `segredo`, `Some(false)` dentro de outro bloco de código.
    let mut dentro: Option<bool> = None;
    let mut inicio = 0;
    let mut posicao = 0;
    for linha in corpo.split_inclusive('\n') {
        let fim_da_linha = posicao + linha.len();
        let cerca = linha
            .trim_end_matches(['\n', '\r'])
            .trim_start_matches(' ')
            .strip_prefix("```");
        match (dentro, cerca) {
            (None, Some(informacao)) => {
                dentro = Some(informacao.trim() == "segredo");
                inicio = fim_da_linha;
            }
            (Some(segredo), Some(informacao)) if informacao.trim().is_empty() => {
                if segredo {
                    blocos.push(inicio..posicao);
                }
                dentro = None;
            }
            _ => {}
        }
        posicao = fim_da_linha;
    }
    if dentro == Some(true) {
        blocos.push(inicio..corpo.len());
    }
    blocos
}

/// O pedaço do texto em volta do primeiro acerto do filtro (sem diferenciar caixa nem acento),
/// numa linha só, de uns 80 caracteres. O que é de um bloco `segredo` aparece como `••••••••`,
/// mas o filtro também o procura. `None` se o texto não tem o filtro.
pub fn trecho(corpo: &str, filtro: &str) -> Option<String> {
    let procurado: Vec<char> = normalizar(filtro.trim()).chars().collect();
    if procurado.is_empty() {
        return None;
    }
    let originais: Vec<(usize, char)> = corpo.char_indices().collect();
    // O texto normalizado e, para cada caractere dele, o do original de onde veio.
    let mut normalizado: Vec<char> = Vec::with_capacity(originais.len());
    let mut origem: Vec<usize> = Vec::with_capacity(originais.len());
    for (i, &(_, c)) in originais.iter().enumerate() {
        for n in normalizar(c.encode_utf8(&mut [0; 4])).chars() {
            normalizado.push(n);
            origem.push(i);
        }
    }
    let acerto = normalizado
        .windows(procurado.len())
        .position(|janela| janela == procurado.as_slice())?;
    let primeiro = origem[acerto];
    let ultimo = origem[acerto + procurado.len() - 1];

    // Os caracteres de blocos `segredo` viram `\0`, que depois viram os pontos.
    let mut escondido = vec![false; originais.len()];
    let blocos = blocos_segredo(corpo);
    for (i, &(byte, _)) in originais.iter().enumerate() {
        escondido[i] = blocos.iter().any(|b| b.contains(&byte));
    }
    let comeco = primeiro.saturating_sub(24);
    let fim = (ultimo + 1 + 56).min(originais.len());
    let mut texto = String::new();
    let mut em_pontos = false;
    for i in comeco..fim {
        if escondido[i] {
            if !em_pontos {
                texto.push_str(MASCARA);
                em_pontos = true;
            }
            continue;
        }
        em_pontos = false;
        let c = originais[i].1;
        texto.push(if c.is_whitespace() { ' ' } else { c });
    }
    // Espaços repetidos (de quebras de linha e recuos) viram um só.
    let mut compacto = String::with_capacity(texto.len());
    for c in texto.trim().chars() {
        if c == ' ' && compacto.ends_with(' ') {
            continue;
        }
        compacto.push(c);
    }
    let prefixo = if comeco > 0 { "…" } else { "" };
    let sufixo = if fim < originais.len() { "…" } else { "" };
    Some(format!("{prefixo}{compacto}{sufixo}"))
}

/// As pastas que a árvore de notas mostra: as que têm uma nota em qualquer nível e as que não
/// têm item nenhum. As que só guardam senhas, conexões e chaves ficam de fora.
pub fn pastas_visiveis(cofre: &Cofre) -> HashSet<u64> {
    cofre
        .pastas()
        .iter()
        .filter(|pasta| {
            let (_, itens) = cofre.conteudo_de_pasta(pasta.id);
            itens.is_empty()
                || itens
                    .iter()
                    .any(|&id| cofre.item(id).is_some_and(|i| i.tipo == Tipo::Nota))
        })
        .map(|pasta| pasta.id)
        .collect()
}

/// O caminho de uma nota para mostrar numa lista: `Clientes › ACME › Acesso`.
pub fn caminho_da_nota(cofre: &Cofre, nota: u64) -> Option<String> {
    let item = cofre.item(nota)?;
    let pasta = cofre.caminho(item.pasta);
    let mut partes: Vec<&str> = pasta.split('/').filter(|p| !p.is_empty()).collect();
    partes.push(&item.titulo);
    Some(partes.join(" › "))
}

// ---------------------------------------------------------------------------------------------
// A troca dos destinos dos links, nos dois sentidos
// ---------------------------------------------------------------------------------------------

/// Se o byte sai como está num caminho relativo em percent-encoding.
fn byte_livre(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'-' | b'_'
                | b'.'
                | b'~'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
        )
}

/// O percent-encoding de um nome (um trecho só do caminho: a `/` também é codificada).
fn codificar(nome: &str) -> String {
    let mut saida = String::with_capacity(nome.len());
    for b in nome.bytes() {
        if byte_livre(b) {
            saida.push(char::from(b));
        } else {
            saida.push_str(&format!("%{b:02X}"));
        }
    }
    saida
}

/// Desfaz o percent-encoding; um `%` sem dois dígitos depois fica como está. `None` se o
/// resultado não é UTF-8.
fn decodificar(texto: &str) -> Option<String> {
    let bytes = texto.as_bytes();
    let mut saida = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(par) = bytes.get(i + 1..i + 3)
            && let Ok(hex) = std::str::from_utf8(par)
            && let Ok(valor) = u8::from_str_radix(hex, 16)
        {
            saida.push(valor);
            i += 3;
            continue;
        }
        saida.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(saida).ok()
}

/// O texto da nota para o arquivo exportado: cada destino `anexo:<id>` (de link e de imagem, fora
/// de código) vira `<pasta_de_anexos>/<nome>`, com o nome que o arquivo recebeu em `arquivos`
/// (id → nome) e em percent-encoding. Um `anexo:<id>` que `arquivos` não conhece fica como está.
pub fn links_para_exportar(
    corpo: &str,
    arquivos: &HashMap<String, String>,
    pasta_de_anexos: &str,
) -> String {
    let mut trocas: Vec<(Range<usize>, String)> = links(corpo)
        .into_iter()
        .filter_map(|link| {
            let id = link.dest.strip_prefix("anexo:")?.trim();
            let nome = arquivos.get(id)?;
            Some((
                link.dest_range,
                format!("{}/{}", codificar(pasta_de_anexos), codificar(nome)),
            ))
        })
        .collect();
    trocas.sort_by_key(|(faixa, _)| faixa.start);
    // Os links de referência que dividem uma definição repetem a faixa.
    trocas.dedup_by(|a, b| a.0 == b.0);
    let mut saida = corpo.to_owned();
    for (faixa, novo) in trocas.into_iter().rev() {
        saida.replace_range(faixa, &novo);
    }
    saida
}

/// Os destinos do texto que podem ser arquivos ao lado do `.md`: a faixa do destino e o caminho
/// relativo (com `/`, sem percent-encoding e sem `#…` nem `?…` do fim). Ficam de fora os que têm
/// esquema (`http:`, `mailto:`, `anexo:`…), os absolutos, os que começam com `#` e os que saem
/// da pasta por `..`.
pub fn caminhos_para_importar(corpo: &str) -> Vec<(Range<usize>, String)> {
    links(corpo)
        .into_iter()
        .filter_map(|link| Some((link.dest_range, caminho_relativo(&link.dest)?)))
        .collect()
}

fn caminho_relativo(destino: &str) -> Option<String> {
    let destino = destino.trim();
    if destino.is_empty() || destino.starts_with(['#', '/', '\\']) {
        return None;
    }
    let fim_do_esquema = destino.find(['/', '?', '#']).unwrap_or(destino.len());
    if destino[..fim_do_esquema].contains(':') {
        return None;
    }
    let sem_ancora = destino.split(['#', '?']).next()?;
    let decodificado = decodificar(sem_ancora)?;
    let mut partes = Vec::new();
    for parte in decodificado.split('/') {
        match parte {
            "" | "." => {}
            ".." => return None,
            p if p.contains([':', '\0', '\\']) => return None,
            p => partes.push(p),
        }
    }
    (!partes.is_empty()).then(|| partes.join("/"))
}

// ---------------------------------------------------------------------------------------------
// Anexar um arquivo
// ---------------------------------------------------------------------------------------------

/// O primeiro pedaço do caminho que existe (a pasta `vault/` pode ainda não existir).
fn primeiro_existente(caminho: &Path) -> &Path {
    caminho
        .ancestors()
        .find(|p| !p.as_os_str().is_empty() && p.exists())
        .unwrap_or(caminho)
}

/// Cifra o arquivo `origem` em `vault/<id novo>` e devolve a ficha, com o nome (o do arquivo, ou
/// `nome`), o tipo pela extensão e a data. O espaço do disco é conferido antes. Quem chama
/// põe a ficha no item (`Cofre::anexar`) e apaga o arquivo se isso falhar.
pub fn anexar_arquivo(
    vault: &Path,
    origem: &Path,
    nome: Option<&str>,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Anexo, ErroAnexo> {
    anexar_arquivo_com_id(
        vault,
        &anexos::novo_id(),
        origem,
        nome,
        progresso,
        cancelado,
    )
}

/// Como [`anexar_arquivo`], com o `id` já escolhido: a tela põe o link no texto antes de a
/// gravação acabar e precisa saber o `id` de antemão.
pub fn anexar_arquivo_com_id(
    vault: &Path,
    id: &str,
    origem: &Path,
    nome: Option<&str>,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Anexo, ErroAnexo> {
    let metadados = fs::metadata(origem)?;
    if !metadados.is_file() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, t!("notas-pasta-nao-anexa")).into(),
        );
    }
    let nome = match nome {
        Some(n) if !n.trim().is_empty() => n.trim().to_owned(),
        _ => origem
            .file_name()
            .map_or_else(|| "anexo".to_owned(), |n| n.to_string_lossy().into_owned()),
    };
    let mut arquivo = fs::File::open(origem)?;
    anexar_leitura(
        vault,
        id,
        &nome,
        &mut arquivo,
        metadados.len(),
        progresso,
        cancelado,
    )
}

/// Cifra `bytes` (uma imagem colada, por exemplo) em `vault/<id>`, com o nome `nome`.
pub fn anexar_bytes(
    vault: &Path,
    id: &str,
    nome: &str,
    bytes: &[u8],
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Anexo, ErroAnexo> {
    let mut leitura = io::Cursor::new(bytes);
    anexar_leitura(
        vault,
        id,
        nome,
        &mut leitura,
        bytes.len() as u64,
        progresso,
        cancelado,
    )
}

/// O que as duas variantes têm em comum: conferir o espaço, cifrar e preencher a ficha.
fn anexar_leitura(
    vault: &Path,
    id: &str,
    nome: &str,
    origem: &mut dyn io::Read,
    tamanho: u64,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Anexo, ErroAnexo> {
    anexos::conferir_espaco_em(
        primeiro_existente(vault),
        anexos::tamanho_cifrado(tamanho, anexos::BLOCO),
    )?;
    let mut ficha = anexos::gravar(
        vault,
        id,
        origem,
        tamanho,
        anexos::BLOCO,
        progresso,
        cancelado,
    )?;
    ficha.nome = nome.to_owned();
    ficha.tipo = anexos::tipo_do_nome(nome).to_owned();
    ficha.criado = agora();
    Ok(ficha)
}

// ---------------------------------------------------------------------------------------------
// Exportar
// ---------------------------------------------------------------------------------------------

/// O que uma exportação gravou.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exportacao {
    /// O arquivo `.md` (de uma nota) ou a pasta (de uma pasta) que ficou.
    pub caminho: PathBuf,
    /// Quantas notas foram gravadas.
    pub notas: usize,
    /// Quantos anexos foram gravados.
    pub anexos: usize,
    /// Quantos anexos ficaram de fora (o arquivo não está nesta máquina ou não abriu).
    pub pulados: usize,
}

impl Exportacao {
    /// A mensagem para a barra de aviso.
    pub fn mensagem(&self, pasta: bool) -> String {
        let mut texto = if pasta {
            t!(
                "notas-exportadas-em",
                n = self.notas as i64,
                caminho = self.caminho.display().to_string()
            )
        } else {
            t!(
                "notas-exportada-em",
                caminho = self.caminho.display().to_string()
            )
        };
        if self.pulados > 0 {
            texto.push(' ');
            texto.push_str(&t!("notas-anexos-pulados", n = self.pulados as i64));
        }
        texto
    }
}

fn nota_sumiu() -> ErroAnexo {
    io::Error::new(io::ErrorKind::NotFound, t!("notas-nota-sumiu")).into()
}

/// O andamento de uma exportação ou importação inteira (vários arquivos).
struct Andamento<'a> {
    total: u64,
    feito: u64,
    progresso: &'a dyn Fn(Progresso),
    cancelado: &'a AtomicBool,
}

impl Andamento<'_> {
    /// O progresso de um arquivo, somado ao que os anteriores já fizeram.
    fn de_um_arquivo(&self) -> impl Fn(Progresso) + '_ {
        let base = self.feito;
        move |p| {
            (self.progresso)(Progresso {
                feito: base + p.feito,
                total: self.total,
            });
        }
    }

    fn concluir(&mut self, bytes: u64) {
        self.feito += bytes;
        (self.progresso)(Progresso {
            feito: self.feito,
            total: self.total,
        });
    }

    fn cancelado(&self) -> bool {
        self.cancelado.load(Ordering::Relaxed)
    }
}

/// `nome` + `extensao` sem repetir nenhum de `usados` (sem diferenciar a caixa, como o Windows e o
/// macOS): o que repete ganha " (2)", " (3)"…
fn nome_livre(usados: &HashSet<String>, base: &str, extensao: &str) -> String {
    let livre = |candidato: &str| !usados.contains(&candidato.to_lowercase());
    let primeiro = format!("{base}{extensao}");
    if livre(&primeiro) {
        return primeiro;
    }
    (2..)
        .map(|n| format!("{base} ({n}){extensao}"))
        .find(|c| livre(c))
        .unwrap_or(primeiro)
}

/// Os nomes que já existem em `pasta`, em minúsculas.
fn nomes_em(pasta: &Path) -> HashSet<String> {
    fs::read_dir(pasta)
        .map(|entradas| {
            entradas
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_lowercase())
                .collect()
        })
        .unwrap_or_default()
}

/// Quanto espaço a exportação de `notas` precisa: o texto e os anexos que existem aqui.
fn bytes_a_exportar(cofre: &Cofre, notas: &[u64]) -> u64 {
    let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
    notas
        .iter()
        .filter_map(|&id| cofre.item(id))
        .map(|nota| {
            nota.notas.len() as u64
                + nota
                    .anexos
                    .iter()
                    .filter(|a| anexos::existe(&vault, &a.id))
                    .map(|a| a.tamanho)
                    .sum::<u64>()
        })
        .sum()
}

fn apagar_exportado(caminhos: &[&Path]) {
    for caminho in caminhos {
        if caminho.is_dir() {
            let _ = fs::remove_dir_all(caminho);
        } else {
            let _ = fs::remove_file(caminho);
        }
    }
}

struct Saida<'a> {
    cofre: &'a Cofre,
    vault: PathBuf,
    andamento: Andamento<'a>,
    notas: usize,
    anexos: usize,
    pulados: usize,
    /// O que a exportação criou, para desfazer se ela falhar.
    criados: Vec<PathBuf>,
}

impl Saida<'_> {
    /// Grava a nota em `pasta`: `<título>.md` e, se ela tem anexos, `<título>-attachments/`.
    fn nota(&mut self, nota: u64, pasta: &Path) -> Result<PathBuf, ErroAnexo> {
        let item = self.cofre.item(nota).ok_or_else(nota_sumiu)?.clone();
        let usados = nomes_em(pasta);
        let base = nome_de_arquivo(&item.titulo);
        // O `.md` e a pasta dos anexos andam juntos: nenhum dos dois pode já existir.
        let mut nome = base.clone();
        for n in 2.. {
            let md = nome_livre(&usados, &nome, ".md");
            let anexos = nome_livre(&usados, &nome, SUFIXO_DE_ANEXOS);
            if md == format!("{nome}.md") && anexos == format!("{nome}{SUFIXO_DE_ANEXOS}") {
                break;
            }
            nome = format!("{base} ({n})");
        }
        let caminho_md = pasta.join(format!("{nome}.md"));
        let nome_da_pasta_de_anexos = format!("{nome}{SUFIXO_DE_ANEXOS}");
        let pasta_de_anexos = pasta.join(&nome_da_pasta_de_anexos);

        let mut arquivos: HashMap<String, String> = HashMap::new();
        let mut gravados = 0;
        if !item.anexos.is_empty() {
            fs::create_dir_all(&pasta_de_anexos)?;
            self.criados.push(pasta_de_anexos.clone());
            let mut usados_nos_anexos: HashSet<String> = HashSet::new();
            for anexo in &item.anexos {
                if self.andamento.cancelado() {
                    return Err(ErroAnexo::Cancelado);
                }
                if !anexos::existe(&self.vault, &anexo.id) {
                    self.pulados += 1;
                    continue;
                }
                let nome_do_arquivo = nome_de_arquivo(&anexo.nome);
                let (base_do_anexo, extensao) = match nome_do_arquivo.rfind('.') {
                    Some(0) | None => (nome_do_arquivo.as_str(), ""),
                    Some(i) => nome_do_arquivo.split_at(i),
                };
                let nome_do_arquivo = nome_livre(&usados_nos_anexos, base_do_anexo, extensao);
                let destino = pasta_de_anexos.join(&nome_do_arquivo);
                let resultado = anexos::ler_para_arquivo(
                    &self.vault,
                    anexo,
                    &destino,
                    &self.andamento.de_um_arquivo(),
                    self.andamento.cancelado,
                );
                match resultado {
                    Ok(()) => {
                        usados_nos_anexos.insert(nome_do_arquivo.to_lowercase());
                        arquivos.entry(anexo.id.clone()).or_insert(nome_do_arquivo);
                        gravados += 1;
                        self.andamento.concluir(anexo.tamanho);
                    }
                    // Um anexo que não abre não derruba a exportação das notas.
                    Err(ErroAnexo::Ausente | ErroAnexo::Corrompido) => self.pulados += 1,
                    Err(erro) => return Err(erro),
                }
            }
            if gravados == 0 {
                // Nada para guardar ali: a pasta vazia só atrapalharia.
                let _ = fs::remove_dir(&pasta_de_anexos);
                self.criados.pop();
            }
        }
        let texto = links_para_exportar(&item.notas, &arquivos, &nome_da_pasta_de_anexos);
        fs::write(&caminho_md, texto)?;
        self.criados.push(caminho_md.clone());
        self.notas += 1;
        self.anexos += gravados;
        Ok(caminho_md)
    }

    /// Grava a pasta (ou a raiz, com `None`) como `<nome>/` dentro de `pai`.
    fn pasta(&mut self, pasta: Option<u64>, pai: &Path) -> Result<PathBuf, ErroAnexo> {
        let nome = match pasta {
            Some(id) => self
                .cofre
                .pasta(id)
                .map(|p| nome_de_arquivo(&p.nome))
                .ok_or_else(nota_sumiu)?,
            None => "Cofre".to_owned(),
        };
        let nome = nome_livre(&nomes_em(pai), &nome, "");
        let aqui = pai.join(nome);
        fs::create_dir(&aqui)?;
        self.criados.push(aqui.clone());
        let notas: Vec<u64> = self
            .cofre
            .itens_em(pasta)
            .into_iter()
            .filter(|i| i.tipo == Tipo::Nota)
            .map(|i| i.id)
            .collect();
        for nota in notas {
            self.nota(nota, &aqui)?;
        }
        let filhas: Vec<u64> = self.cofre.filhas(pasta).into_iter().map(|p| p.id).collect();
        for filha in filhas {
            self.pasta(Some(filha), &aqui)?;
        }
        Ok(aqui)
    }
}

/// Grava a nota em `destino` (uma pasta): `<título>.md` com o texto como está no cofre, mas com os
/// links `anexo:<id>` trocados pelos caminhos dos arquivos, e `<título>-attachments/` com os
/// anexos em claro. Um nome que já existe ganha " (2)".
pub fn exportar_nota(
    cofre: &Cofre,
    nota: u64,
    destino: &Path,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Exportacao, ErroAnexo> {
    exportar(cofre, &[nota], destino, progresso, cancelado, |saida| {
        saida.nota(nota, destino)
    })
}

/// Grava a pasta do cofre (`None`, a raiz inteira) como `<nome>/` dentro de `destino`, com as
/// subpastas e as notas (só elas, com os anexos). Senhas e conexões não vão.
pub fn exportar_pasta(
    cofre: &Cofre,
    pasta: Option<u64>,
    destino: &Path,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Exportacao, ErroAnexo> {
    if pasta.is_some_and(|id| cofre.pasta(id).is_none()) {
        return Err(nota_sumiu());
    }
    let notas: Vec<u64> = match pasta {
        Some(id) => {
            let (_, itens) = cofre.conteudo_de_pasta(id);
            itens
        }
        None => cofre.itens().iter().map(|i| i.id).collect(),
    }
    .into_iter()
    .filter(|&id| cofre.item(id).is_some_and(|i| i.tipo == Tipo::Nota))
    .collect();
    exportar(cofre, &notas, destino, progresso, cancelado, |saida| {
        saida.pasta(pasta, destino)
    })
}

fn exportar(
    cofre: &Cofre,
    notas: &[u64],
    destino: &Path,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
    gravar: impl FnOnce(&mut Saida) -> Result<PathBuf, ErroAnexo>,
) -> Result<Exportacao, ErroAnexo> {
    fs::create_dir_all(destino)?;
    anexos::conferir_espaco_em(destino, bytes_a_exportar(cofre, notas))?;
    let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
    let total = notas
        .iter()
        .filter_map(|&id| cofre.item(id))
        .flat_map(|n| n.anexos.iter())
        .filter(|a| anexos::existe(&vault, &a.id))
        .map(|a| a.tamanho)
        .sum();
    let mut saida = Saida {
        cofre,
        vault,
        andamento: Andamento {
            total,
            feito: 0,
            progresso,
            cancelado,
        },
        notas: 0,
        anexos: 0,
        pulados: 0,
        criados: Vec::new(),
    };
    match gravar(&mut saida) {
        Ok(caminho) => Ok(Exportacao {
            caminho,
            notas: saida.notas,
            anexos: saida.anexos,
            pulados: saida.pulados,
        }),
        Err(erro) => {
            // Cancelou ou faltou espaço: não deixa a metade para trás.
            let criados: Vec<PathBuf> = saida.criados.iter().rev().cloned().collect();
            apagar_exportado(&criados.iter().map(PathBuf::as_path).collect::<Vec<_>>());
            Err(erro)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Importar
// ---------------------------------------------------------------------------------------------

/// O que uma importação trouxe.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Importacao {
    /// Os ids das notas novas no cofre, na ordem em que foram lidas.
    pub notas: Vec<u64>,
    /// Quantos anexos entraram.
    pub anexos: usize,
    /// Quantos arquivos citados não puderam ser lidos (e ficaram como estavam).
    pub nao_lidos: usize,
}

impl Importacao {
    /// A mensagem para a barra de aviso: "3 notas e 2 anexos importados."
    pub fn mensagem(&self) -> String {
        let notas = self.notas.len();
        if notas == 0 {
            return t!("notas-nenhuma-para-importar");
        }
        let mut texto = match (notas, self.anexos) {
            (n, 0) => t!("notas-importadas", n = n as i64),
            (n, a) => t!("notas-e-anexos-importados", n = n as i64, a = a as i64),
        };
        if self.nao_lidos > 0 {
            texto.push(' ');
            texto.push_str(&t!("notas-anexos-nao-lidos", n = self.nao_lidos as i64));
        }
        texto
    }
}

/// Uma nota lida do disco, antes de os anexos entrarem.
struct Lida {
    titulo: String,
    corpo: String,
    /// A pasta do lote (`None` é o destino da importação).
    pasta: Option<u64>,
    /// Os arquivos que viram anexo: o caminho citado no texto (se houve) e o arquivo.
    arquivos: Vec<(Option<String>, PathBuf)>,
    caminhos: Vec<(Range<usize>, String)>,
}

fn e_texto(arquivo: &Path) -> bool {
    arquivo
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt"))
}

fn e_pasta_de_anexos(nome: &str) -> bool {
    nome.to_lowercase().ends_with(SUFIXO_DE_ANEXOS)
}

fn chave_do_arquivo(caminho: &Path) -> PathBuf {
    fs::canonicalize(caminho).unwrap_or_else(|_| caminho.to_owned())
}

/// Lê o `.md` e separa o que vira anexo: os arquivos que o texto cita e os de
/// `<nome>-attachments/` ao lado que ninguém cita.
fn ler_nota(arquivo: &Path, pasta: Option<u64>) -> io::Result<Lida> {
    let bytes = fs::read(arquivo)?;
    let corpo = String::from_utf8_lossy(&bytes).into_owned();
    let titulo = arquivo
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ao_lado = arquivo.parent().unwrap_or_else(|| Path::new("."));
    let caminhos = caminhos_para_importar(&corpo);
    let mut arquivos: Vec<(Option<String>, PathBuf)> = Vec::new();
    let mut vistos: HashSet<PathBuf> = HashSet::new();
    for (_, relativo) in &caminhos {
        let completo = relativo
            .split('/')
            .fold(ao_lado.to_owned(), |c, parte| c.join(parte));
        if !completo.is_file() || !vistos.insert(chave_do_arquivo(&completo)) {
            continue;
        }
        arquivos.push((Some(relativo.clone()), completo));
    }
    let da_pasta = ao_lado.join(format!("{titulo}{SUFIXO_DE_ANEXOS}"));
    if let Ok(entradas) = fs::read_dir(&da_pasta) {
        let mut extras: Vec<PathBuf> = entradas
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        extras.sort();
        for extra in extras {
            if vistos.insert(chave_do_arquivo(&extra)) {
                arquivos.push((None, extra));
            }
        }
    }
    Ok(Lida {
        titulo,
        corpo,
        pasta,
        arquivos,
        caminhos,
    })
}

fn tamanho_do_arquivo(caminho: &Path) -> u64 {
    fs::metadata(caminho).map_or(0, |m| m.len())
}

/// Importa o `.md`, `.markdown` ou `.txt` como nota de `destino`, com os arquivos que ele cita.
pub fn importar_arquivo(
    cofre: &mut Cofre,
    arquivo: &Path,
    destino: Option<u64>,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Importacao, ErroAnexo> {
    let lida = ler_nota(arquivo, None)?;
    importar_lidas(
        cofre,
        Lote::default(),
        vec![lida],
        destino,
        progresso,
        cancelado,
    )
}

/// Importa a pasta inteira: ela e cada subpasta viram pastas do cofre (juntando as de mesmo
/// nome), e cada `.md`, `.markdown` e `.txt` vira nota. As pastas `*-attachments` e as que
/// começam com ponto não viram pastas.
pub fn importar_pasta(
    cofre: &mut Cofre,
    pasta: &Path,
    destino: Option<u64>,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Importacao, ErroAnexo> {
    let mut lote = Lote::default();
    let mut lidas = Vec::new();
    let nome = pasta.file_name().map_or_else(
        || "Importadas".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    let raiz = lote.caminho(&nome);
    percorrer(pasta, raiz, &mut lote, &mut lidas, 0)?;
    importar_lidas(cofre, lote, lidas, destino, progresso, cancelado)
}

fn percorrer(
    pasta: &Path,
    pasta_do_lote: Option<u64>,
    lote: &mut Lote,
    lidas: &mut Vec<Lida>,
    profundidade: usize,
) -> io::Result<()> {
    if profundidade > PROFUNDIDADE_MAXIMA {
        return Ok(());
    }
    let mut entradas: Vec<_> = fs::read_dir(pasta)?.flatten().collect();
    entradas.sort_by_key(|e| e.file_name());
    for entrada in entradas {
        let nome = entrada.file_name().to_string_lossy().into_owned();
        let caminho = entrada.path();
        let Ok(tipo) = entrada.file_type() else {
            continue;
        };
        if tipo.is_dir() {
            if e_pasta_de_anexos(&nome) || nome.starts_with('.') {
                continue;
            }
            // O nome da pasta pode ter `/` no Windows? Não: o sistema não deixa. Mas o `caminho`
            // do lote parte por `/`, e um nome só com espaços não vira pasta nenhuma.
            let Some(filha) = nova_pasta_do_lote(lote, pasta_do_lote, &nome) else {
                continue;
            };
            percorrer(&caminho, Some(filha), lote, lidas, profundidade + 1)?;
        } else if caminho.is_file() && e_texto(&caminho) {
            // Um arquivo ilegível é pulado, não derruba a importação das outras notas.
            if let Ok(lida) = ler_nota(&caminho, pasta_do_lote) {
                lidas.push(lida);
            }
        }
    }
    Ok(())
}

fn nova_pasta_do_lote(lote: &mut Lote, pai: Option<u64>, nome: &str) -> Option<u64> {
    let nome = nome.trim();
    if nome.is_empty() {
        return None;
    }
    let id = lote.pastas.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    lote.pastas.push(Pasta {
        id,
        nome: nome.replace('/', "-"),
        pai,
        ..Pasta::default()
    });
    Some(id)
}

fn importar_lidas(
    cofre: &mut Cofre,
    mut lote: Lote,
    lidas: Vec<Lida>,
    destino: Option<u64>,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Importacao, ErroAnexo> {
    if lidas.is_empty() {
        return Ok(Importacao::default());
    }
    let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
    let total: u64 = lidas
        .iter()
        .flat_map(|l| l.arquivos.iter())
        .map(|(_, c)| tamanho_do_arquivo(c))
        .sum();
    let precisa: u64 = lidas
        .iter()
        .flat_map(|l| l.arquivos.iter())
        .map(|(_, c)| anexos::tamanho_cifrado(tamanho_do_arquivo(c), anexos::BLOCO))
        .sum();
    anexos::conferir_espaco_em(primeiro_existente(&vault), precisa)?;

    let mut andamento = Andamento {
        total,
        feito: 0,
        progresso,
        cancelado,
    };
    let mut gravados: Vec<String> = Vec::new();
    let mut nao_lidos = 0;
    let desfazer = |gravados: &[String]| {
        for id in gravados {
            let _ = fs::remove_file(anexos::caminho(&vault, id));
        }
    };
    for (indice, lida) in lidas.into_iter().enumerate() {
        let Lida {
            titulo,
            corpo,
            pasta,
            arquivos,
            caminhos,
        } = lida;
        let mut fichas: Vec<Anexo> = Vec::new();
        let mut ids_por_caminho: HashMap<String, String> = HashMap::new();
        for (citado, arquivo) in arquivos {
            if andamento.cancelado() {
                desfazer(&gravados);
                return Err(ErroAnexo::Cancelado);
            }
            let tamanho = tamanho_do_arquivo(&arquivo);
            let resultado = {
                let andamento_do_arquivo = andamento.de_um_arquivo();
                anexar_arquivo(&vault, &arquivo, None, &andamento_do_arquivo, cancelado)
            };
            match resultado {
                Ok(mut ficha) => {
                    ficha.nome = nome_de_anexo_unico(&fichas, &ficha.nome, None);
                    gravados.push(ficha.id.clone());
                    if let Some(citado) = citado {
                        ids_por_caminho.insert(citado, ficha.id.clone());
                    }
                    fichas.push(ficha);
                    andamento.concluir(tamanho);
                }
                Err(ErroAnexo::Cancelado) => {
                    desfazer(&gravados);
                    return Err(ErroAnexo::Cancelado);
                }
                Err(ErroAnexo::SemEspaco { precisa, livre }) => {
                    desfazer(&gravados);
                    return Err(ErroAnexo::SemEspaco { precisa, livre });
                }
                // O arquivo sumiu ou não abre: o link fica como estava.
                Err(_) => nao_lidos += 1,
            }
        }
        let mut texto = corpo;
        let mut trocas: Vec<&(Range<usize>, String)> = caminhos
            .iter()
            .filter(|(_, caminho)| ids_por_caminho.contains_key(caminho))
            .collect();
        trocas.sort_by_key(|(faixa, _)| faixa.start);
        trocas.dedup_by(|a, b| a.0 == b.0);
        for (faixa, caminho) in trocas.into_iter().rev() {
            texto.replace_range(
                faixa.clone(),
                &format!("anexo:{}", ids_por_caminho[caminho]),
            );
        }
        lote.itens.push(Item {
            id: indice as u64 + 1,
            tipo: Tipo::Nota,
            titulo,
            pasta,
            notas: texto,
            anexos: fichas,
            ..Item::default()
        });
    }
    let quantos_anexos = gravados.len();
    let ordem: Vec<u64> = lote.itens.iter().map(|i| i.id).collect();
    match cofre.importar(lote, destino) {
        Ok(mapa) => Ok(Importacao {
            notas: ordem
                .iter()
                .filter_map(|id| mapa.itens.get(id).copied())
                .collect(),
            anexos: quantos_anexos,
            nao_lidos,
        }),
        Err(erro) => {
            desfazer(&gravados);
            Err(erro.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::cofre::Cofre;

    fn nao() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn nome_de_arquivo_troca_os_proibidos() {
        assert_eq!(
            nome_de_arquivo("a/b\\c:d*e?f\"g<h>i|j"),
            "a-b-c-d-e-f-g-h-i-j"
        );
        assert_eq!(nome_de_arquivo("com\ncontrole\t"), "com-controle-");
        assert_eq!(nome_de_arquivo("fim com ponto. . "), "fim com ponto");
        assert_eq!(nome_de_arquivo("acentuação ✓"), "acentuação ✓");
    }

    #[test]
    fn nome_de_arquivo_protege_os_reservados_do_windows() {
        assert_eq!(nome_de_arquivo("CON"), "CON_");
        assert_eq!(nome_de_arquivo("nul"), "nul_");
        assert_eq!(nome_de_arquivo("com1"), "com1_");
        assert_eq!(nome_de_arquivo("LPT9.txt"), "LPT9_.txt");
        assert_eq!(nome_de_arquivo("COM10"), "COM10");
        assert_eq!(nome_de_arquivo("CONSOLE"), "CONSOLE");
        assert_eq!(nome_de_arquivo("COM0"), "COM0");
    }

    #[test]
    fn nome_de_arquivo_vazio_e_longo() {
        assert_eq!(nome_de_arquivo(""), "Sem título");
        assert_eq!(nome_de_arquivo("  "), "Sem título");
        assert_eq!(nome_de_arquivo("..."), "Sem título");
        let longo = "ã".repeat(300);
        assert_eq!(nome_de_arquivo(&longo).chars().count(), 120);
    }

    #[test]
    fn nome_colado_usa_a_data_em_utc() {
        assert_eq!(nome_colado(0), "colado-19700101-000000.png");
        // 2024-02-29 13:45:09 UTC.
        assert_eq!(nome_colado(1_709_214_309), "colado-20240229-134509.png");
    }

    #[test]
    fn e_imagem_pela_extensao() {
        for nome in ["a.png", "A.PNG", "b.jpg", "c.JPEG", "d.gif", "e.webp"] {
            assert!(e_imagem(nome), "{nome}");
        }
        for nome in ["a.pdf", "png", "a.png.txt", "", ".bmp"] {
            assert!(!e_imagem(nome), "{nome}");
        }
    }

    #[test]
    fn link_do_anexo_escapa_o_nome() {
        assert_eq!(link_do_anexo("foto.png", "ab12"), "![foto.png](anexo:ab12)");
        assert_eq!(
            link_do_anexo("chave.pem", "ab12"),
            "[chave.pem](anexo:ab12)"
        );
        assert_eq!(
            link_do_anexo("a[1]\\b.png", "x"),
            "![a\\[1\\]\\\\b.png](anexo:x)"
        );
        // O link escrito volta pelo leitor com o mesmo destino.
        let texto = link_do_anexo("a[1].txt", "x1");
        let achados = links(&texto);
        assert_eq!(achados.len(), 1);
        assert_eq!(achados[0].dest, "anexo:x1");
    }

    #[test]
    fn contagem_de_palavras() {
        assert_eq!(contar_palavras(""), 0);
        assert_eq!(contar_palavras("  um   dois\ntrês\t quatro "), 4);
        assert_eq!(contar_palavras("# Título\n\n- item"), 4);
    }

    #[test]
    fn modelos_tem_titulo_e_cursor() {
        assert_eq!(Modelo::Branco.titulo(), "Nova nota");
        assert_eq!(Modelo::Servidor.titulo(), "Novo servidor");
        assert_eq!(Modelo::Chave.titulo(), "Nova chave");
        assert_eq!(Modelo::Procedimento.titulo(), "Novo procedimento");
        let branco = Modelo::Branco.corpo();
        assert_eq!((branco.texto.as_str(), branco.cursor), ("", 0));
        for modelo in Modelo::TODOS {
            let corpo = modelo.corpo();
            assert!(!corpo.texto.contains(CURSOR), "{modelo:?}");
            assert!(corpo.texto.is_char_boundary(corpo.cursor));
        }
        // Servidor: o cursor fica no fim de "- Endereço: ".
        let servidor = Modelo::Servidor.corpo();
        assert!(
            servidor
                .texto
                .starts_with("# Acesso\n\n- Endereço: \n- Porta: 22\n- Usuário: \n")
        );
        assert_eq!(
            &servidor.texto[..servidor.cursor],
            "# Acesso\n\n- Endereço: "
        );
        assert_eq!(blocos_segredo(&servidor.texto).len(), 1);
        // Chave: a frase fica acima do bloco do segredo.
        let chave = Modelo::Chave.corpo().texto;
        let frase = chave.find("Cole a chave aqui ou anexe o arquivo.").unwrap();
        assert!(frase < chave.find("```segredo").unwrap());
        assert!(chave.contains("# Para que serve"));
        assert!(chave.contains("# Onde está instalada\n\n- "));
        assert!(chave.contains("# Validade"));
        // Procedimento: três tarefas vazias e um bloco de shell.
        let procedimento = Modelo::Procedimento.corpo();
        assert_eq!(procedimento.texto.matches("- [ ] ").count(), 3);
        assert!(procedimento.texto.contains("```sh\n\n```"));
        assert!(procedimento.texto.contains("# Se der errado"));
        assert_eq!(
            &procedimento.texto[..procedimento.cursor],
            "# Passos\n\n- [ ] "
        );
    }

    #[test]
    fn modelo_de_servidor_preenchido_pela_conexao() {
        let corpo_do_servidor = |e, p, u| tirar_cursor(&servidor_marcado(e, p, u));
        let c = corpo_do_servidor("10.0.0.5", 2222, "root");
        assert!(
            c.texto
                .contains("- Endereço: 10.0.0.5\n- Porta: 2222\n- Usuário: root\n")
        );
        // Os dois preenchidos: o cursor vai ao bloco do segredo.
        assert!(c.texto[..c.cursor].ends_with("```segredo\n"));
        let sem_usuario = corpo_do_servidor("host", 0, "");
        assert!(sem_usuario.texto.contains("- Porta: 22\n"));
        assert!(sem_usuario.texto[..sem_usuario.cursor].ends_with("- Usuário: "));
        let sem_endereco = corpo_do_servidor("", 22, "eu");
        assert!(sem_endereco.texto[..sem_endereco.cursor].ends_with("- Endereço: "));
    }

    #[test]
    fn blocos_segredo_acha_so_os_do_tipo_certo() {
        let texto = "a\n```segredo\nsenha\nmais\n```\nb\n```sh\n```segredo\n```\nc\n";
        let blocos = blocos_segredo(texto);
        assert_eq!(blocos.len(), 1);
        assert_eq!(&texto[blocos[0].clone()], "senha\nmais\n");
        // Sem cerca de fechar, vai até o fim.
        let aberto = "x\n```segredo\nabc";
        let blocos = blocos_segredo(aberto);
        assert_eq!(&aberto[blocos[0].clone()], "abc");
        assert!(blocos_segredo("sem nada").is_empty());
    }

    #[test]
    fn trecho_acha_sem_caixa_nem_acento() {
        let texto = "Primeira linha\nO Servidor de Produção fica em Curitiba\nfim";
        let t = trecho(texto, "producao").unwrap();
        assert!(t.contains("Produção"), "{t}");
        assert!(!t.contains('\n'));
        assert_eq!(trecho(texto, "inexistente"), None);
        assert_eq!(trecho(texto, "   "), None);
    }

    #[test]
    fn trecho_tem_uns_oitenta_caracteres_com_reticencias() {
        let texto = format!("{}ALVO{}", "a".repeat(200), "b".repeat(200));
        let t = trecho(&texto, "alvo").unwrap();
        assert!(t.starts_with('…') && t.ends_with('…'), "{t}");
        assert!(t.contains("ALVO"));
        assert!(t.chars().count() <= 90, "{}", t.chars().count());
        let curto = trecho("ALVO no começo", "alvo").unwrap();
        assert_eq!(curto, "ALVO no começo");
    }

    #[test]
    fn trecho_mascara_o_segredo_mas_o_filtro_o_acha() {
        let texto = "Acesso do banco\n```segredo\nhunter2-senha\n```\nfim";
        let t = trecho(texto, "hunter2").unwrap();
        assert!(!t.contains("hunter2"), "{t}");
        assert!(t.contains("••••••••"), "{t}");
        // O texto de fora do bloco continua legível.
        let fora = trecho(texto, "banco").unwrap();
        assert!(fora.contains("banco"));
        assert!(fora.contains("••••••••"));
        assert!(!fora.contains("hunter2"));
    }

    fn cofre_vazio(nome: &str) -> (Cofre, PastaTemporaria) {
        let pasta = PastaTemporaria::nova(nome);
        let (cofre, _) = Cofre::abrir(&pasta.0);
        (cofre, pasta)
    }

    #[test]
    fn pastas_da_arvore_de_notas() {
        let (mut cofre, _pasta) = cofre_vazio("notas-arvore");
        let clientes = cofre.nova_pasta(None, "Clientes").unwrap();
        let acme = cofre.nova_pasta(Some(clientes), "ACME").unwrap();
        let fundo = cofre.nova_pasta(Some(acme), "Fundo").unwrap();
        let vazia = cofre.nova_pasta(None, "Vazia").unwrap();
        let senhas = cofre.nova_pasta(None, "Só senhas").unwrap();
        let mista = cofre.nova_pasta(None, "Mista").unwrap();
        cofre.nova_nota(Some(fundo), "Acesso", "x").unwrap();
        cofre
            .salvar(Item {
                titulo: "banco".to_owned(),
                pasta: Some(senhas),
                ..Item::default()
            })
            .unwrap();
        cofre
            .salvar(Item {
                titulo: "outra".to_owned(),
                pasta: Some(mista),
                ..Item::default()
            })
            .unwrap();
        cofre.nova_nota(Some(mista), "Leia", "").unwrap();
        let visiveis = pastas_visiveis(&cofre);
        // Uma nota funda mostra o caminho todo.
        for pasta in [clientes, acme, fundo, vazia, mista] {
            assert!(visiveis.contains(&pasta), "{pasta}");
        }
        assert!(!visiveis.contains(&senhas));
    }

    #[test]
    fn caminho_da_nota_usa_setas() {
        let (mut cofre, _pasta) = cofre_vazio("notas-caminho");
        let clientes = cofre.nova_pasta(None, "Clientes").unwrap();
        let acme = cofre.nova_pasta(Some(clientes), "ACME").unwrap();
        let nota = cofre.nova_nota(Some(acme), "Acesso", "").unwrap();
        let raiz = cofre.nova_nota(None, "Solta", "").unwrap();
        assert_eq!(
            caminho_da_nota(&cofre, nota).as_deref(),
            Some("Clientes › ACME › Acesso")
        );
        assert_eq!(caminho_da_nota(&cofre, raiz).as_deref(), Some("Solta"));
        assert_eq!(caminho_da_nota(&cofre, 9999), None);
    }

    fn mapa(pares: &[(&str, &str)]) -> HashMap<String, String> {
        pares
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn links_para_exportar_troca_so_o_que_conhece() {
        let corpo = "![a](anexo:1) e [b](anexo:2) e [web](https://x.com/a b) e [c](anexo:9)";
        let arquivos = mapa(&[("1", "foto (1) é.png"), ("2", "dois.txt")]);
        let saida = links_para_exportar(corpo, &arquivos, "Meu servidor-attachments");
        assert_eq!(
            saida,
            "![a](Meu%20servidor-attachments/foto%20%281%29%20%C3%A9.png) e \
             [b](Meu%20servidor-attachments/dois.txt) e [web](https://x.com/a b) e [c](anexo:9)"
        );
        // O texto sem links passa igual.
        assert_eq!(links_para_exportar("nada", &arquivos, "p"), "nada");
    }

    #[test]
    fn links_para_exportar_ignora_codigo() {
        let corpo = "`[x](anexo:1)`\n\n```\n[x](anexo:1)\n```\n\n[ok](anexo:1)";
        let saida = links_para_exportar(corpo, &mapa(&[("1", "a.txt")]), "t-attachments");
        assert_eq!(
            saida,
            "`[x](anexo:1)`\n\n```\n[x](anexo:1)\n```\n\n[ok](t-attachments/a.txt)"
        );
    }

    #[test]
    fn caminhos_para_importar_so_relativos() {
        let corpo = "![a](img/a%20b.png) [w](http://x/y.png) [m](mailto:a@b.c) [h](#ancora) \
                     [abs](/etc/passwd) [fora](../fora.png) [ok](doc.pdf#pagina=2) [q](dados.csv?v=1) \
                     [anexo](anexo:1) [win](C:/x.png) [barra](\\\\srv\\x)";
        let achados: Vec<String> = caminhos_para_importar(corpo)
            .into_iter()
            .map(|(_, c)| c)
            .collect();
        assert_eq!(achados, ["img/a b.png", "doc.pdf", "dados.csv"]);
        // A faixa é a do destino no texto.
        let (faixa, _) = &caminhos_para_importar("x ![a](img/a%20b.png)")[0];
        assert_eq!(&"x ![a](img/a%20b.png)"[faixa.clone()], "img/a%20b.png");
        // `./` e barras repetidas se ajeitam; só "." não é arquivo.
        let c = caminhos_para_importar("[a](./x//y.png) [b](.)");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].1, "x/y.png");
    }

    #[test]
    fn percent_encoding_vai_e_volta() {
        for nome in ["a b.png", "é(1)%.txt", "x<y>.md", "a#b?c", "ok-_.~"] {
            assert_eq!(
                decodificar(&codificar(nome)).as_deref(),
                Some(nome),
                "{nome}"
            );
        }
        assert_eq!(codificar("a b"), "a%20b");
        assert_eq!(decodificar("100%"), Some("100%".to_owned()));
        assert_eq!(decodificar("%zz"), Some("%zz".to_owned()));
        assert_eq!(decodificar("%FF"), None);
    }

    fn escrever(caminho: &Path, bytes: &[u8]) {
        fs::create_dir_all(caminho.parent().unwrap()).unwrap();
        fs::write(caminho, bytes).unwrap();
    }

    /// Anexa o arquivo à nota e devolve o id do anexo.
    fn anexar(cofre: &mut Cofre, nota: u64, nome: &str, bytes: &[u8], onde: &Path) -> String {
        let origem = onde.join(format!("origem-{nome}"));
        escrever(&origem, bytes);
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        let ficha = anexar_arquivo(&vault, &origem, Some(nome), &|_| {}, &nao()).unwrap();
        let id = ficha.id.clone();
        cofre.anexar(nota, ficha).unwrap();
        id
    }

    #[test]
    fn anexar_arquivo_cifra_e_preenche_a_ficha() {
        let (mut cofre, pasta) = cofre_vazio("notas-anexar-arquivo");
        let nota = cofre.nova_nota(None, "n", "").unwrap();
        let id = anexar(&mut cofre, nota, "dados.json", b"{\"a\":1}", &pasta.0);
        let ficha = cofre.item(nota).unwrap().anexos[0].clone();
        assert_eq!(ficha.id, id);
        assert_eq!(ficha.nome, "dados.json");
        assert_eq!(ficha.tipo, "application/json");
        assert_eq!(ficha.tamanho, 7);
        assert!(ficha.criado > 0);
        let vault = pasta.0.join("vault");
        let lido = anexos::ler_para_memoria(&vault, &ficha, &nao()).unwrap();
        assert_eq!(lido, b"{\"a\":1}");
        // O que está no disco não é o claro.
        assert!(
            !fs::read(vault.join(&id))
                .unwrap()
                .windows(7)
                .any(|j| j == b"{\"a\":1}")
        );
        // Pasta não se anexa.
        let erro = anexar_arquivo(&vault, &pasta.0, None, &|_| {}, &nao());
        assert!(matches!(erro, Err(ErroAnexo::Io(_))));
        // Arquivo que não existe.
        let erro = anexar_arquivo(&vault, &pasta.0.join("nao-existe"), None, &|_| {}, &nao());
        assert!(matches!(erro, Err(ErroAnexo::Io(_))));
    }

    #[test]
    fn exportar_nota_grava_md_e_anexos_em_claro() {
        let (mut cofre, pasta) = cofre_vazio("notas-exportar-nota");
        let nota = cofre.nova_nota(None, "Meu: servidor?", "").unwrap();
        let foto = anexar(&mut cofre, nota, "foto (1).png", b"PNG-bytes", &pasta.0);
        let solto = anexar(&mut cofre, nota, "sem-link.txt", b"texto solto", &pasta.0);
        let _ = solto;
        let mut item = cofre.item(nota).unwrap().clone();
        item.notas = format!(
            "# Oi\n\n![foto](anexo:{foto})\n\n[web](https://x.com)\n\n[some](anexo:naoexiste)\n"
        );
        cofre.salvar(item).unwrap();
        let destino = pasta.0.join("saida");
        let feito = std::cell::Cell::new(Progresso { feito: 0, total: 0 });
        let e = exportar_nota(&cofre, nota, &destino, &|p| feito.set(p), &nao()).unwrap();
        assert_eq!(e.caminho, destino.join("Meu- servidor-.md"));
        assert_eq!((e.notas, e.anexos, e.pulados), (1, 2, 0));
        assert_eq!(feito.get().feito, feito.get().total);
        let md = fs::read_to_string(&e.caminho).unwrap();
        assert!(
            md.contains("![foto](Meu-%20servidor--attachments/foto%20%281%29.png)"),
            "{md}"
        );
        assert!(md.contains("[some](anexo:naoexiste)"));
        let anexos_dir = destino.join("Meu- servidor--attachments");
        assert_eq!(
            fs::read(anexos_dir.join("foto (1).png")).unwrap(),
            b"PNG-bytes"
        );
        assert_eq!(
            fs::read(anexos_dir.join("sem-link.txt")).unwrap(),
            b"texto solto"
        );
        // O texto no cofre não mudou.
        assert!(
            cofre
                .item(nota)
                .unwrap()
                .notas
                .contains(&format!("anexo:{foto}"))
        );
        // Exportar de novo para o mesmo lugar não sobrescreve.
        let e2 = exportar_nota(&cofre, nota, &destino, &|_| {}, &nao()).unwrap();
        assert_eq!(e2.caminho, destino.join("Meu- servidor- (2).md"));
        assert_eq!(
            fs::read(
                destino
                    .join("Meu- servidor- (2)-attachments")
                    .join("sem-link.txt")
            )
            .unwrap(),
            b"texto solto"
        );
        assert!(e.mensagem(false).starts_with("Nota exportada em "));
    }

    #[test]
    fn exportar_pula_anexo_ausente_e_conta() {
        let (mut cofre, pasta) = cofre_vazio("notas-exportar-ausente");
        let nota = cofre.nova_nota(None, "n", "texto").unwrap();
        let a = anexar(&mut cofre, nota, "a.txt", b"aaa", &pasta.0);
        anexar(&mut cofre, nota, "b.txt", b"bbb", &pasta.0);
        fs::remove_file(pasta.0.join("vault").join(&a)).unwrap();
        let destino = pasta.0.join("saida");
        let e = exportar_nota(&cofre, nota, &destino, &|_| {}, &nao()).unwrap();
        assert_eq!((e.anexos, e.pulados), (1, 1));
        assert!(!destino.join("n-attachments").join("a.txt").exists());
        assert!(destino.join("n-attachments").join("b.txt").exists());
        assert!(e.mensagem(false).ends_with("1 anexo não foi exportado."));
    }

    #[test]
    fn exportar_sem_anexo_vivo_nao_deixa_pasta_vazia() {
        let (mut cofre, pasta) = cofre_vazio("notas-exportar-sem-pasta");
        let nota = cofre.nova_nota(None, "n", "texto").unwrap();
        let a = anexar(&mut cofre, nota, "a.txt", b"aaa", &pasta.0);
        fs::remove_file(pasta.0.join("vault").join(&a)).unwrap();
        let destino = pasta.0.join("saida");
        let e = exportar_nota(&cofre, nota, &destino, &|_| {}, &nao()).unwrap();
        assert_eq!((e.anexos, e.pulados), (0, 1));
        assert!(!destino.join("n-attachments").exists());
        assert!(destino.join("n.md").exists());
    }

    #[test]
    fn exportar_cancelado_nao_deixa_nada() {
        let (mut cofre, pasta) = cofre_vazio("notas-exportar-cancelar");
        let nota = cofre.nova_nota(None, "n", "texto").unwrap();
        anexar(&mut cofre, nota, "a.txt", &vec![7u8; 4000], &pasta.0);
        let destino = pasta.0.join("saida");
        let cancelar = AtomicBool::new(true);
        let erro = exportar_nota(&cofre, nota, &destino, &|_| {}, &cancelar);
        assert!(matches!(erro, Err(ErroAnexo::Cancelado)));
        assert!(!destino.join("n.md").exists());
        assert!(!destino.join("n-attachments").exists());
    }

    #[test]
    fn exportar_nota_que_nao_existe_da_erro() {
        let (cofre, pasta) = cofre_vazio("notas-exportar-sumiu");
        let erro = exportar_nota(&cofre, 77, &pasta.0.join("s"), &|_| {}, &nao());
        assert!(matches!(erro, Err(ErroAnexo::Io(_))));
    }

    /// Resumo de um cofre: (caminho da pasta, título, corpo, nomes dos anexos), de cada nota.
    fn resumo(cofre: &Cofre) -> Vec<(String, String, String, Vec<String>)> {
        let mut r: Vec<_> = cofre
            .notas()
            .map(|n| {
                (
                    cofre.caminho(n.pasta),
                    n.titulo.clone(),
                    n.notas.clone(),
                    n.anexos.iter().map(|a| a.nome.clone()).collect(),
                )
            })
            .collect();
        r.sort();
        r
    }

    #[test]
    fn exportar_pasta_e_importar_de_volta_devolve_o_mesmo() {
        let (mut origem, pasta_a) = cofre_vazio("notas-ida-e-volta-a");
        let clientes = origem.nova_pasta(None, "Clientes").unwrap();
        let acme = origem.nova_pasta(Some(clientes), "ACME").unwrap();
        let raiz = origem
            .nova_nota(Some(clientes), "Geral", "# Geral\n\nsem anexo")
            .unwrap();
        let _ = raiz;
        let acesso = origem.nova_nota(Some(acme), "Acesso", "").unwrap();
        let foto = anexar(&mut origem, acesso, "foto (1).png", b"PNG1", &pasta_a.0);
        let leia = anexar(&mut origem, acesso, "leia me.txt", b"LEIA", &pasta_a.0);
        let _ = leia;
        let mut item = origem.item(acesso).unwrap().clone();
        item.notas =
            format!("![f](anexo:{foto})\n\n[repetido](anexo:{foto})\n\n```segredo\nsenha\n```\n");
        origem.salvar(item).unwrap();
        // Uma senha na mesma pasta não vai.
        origem
            .salvar(Item {
                titulo: "banco".to_owned(),
                pasta: Some(acme),
                senha: "xyz".to_owned(),
                ..Item::default()
            })
            .unwrap();
        let destino = pasta_a.0.join("export");
        let e = exportar_pasta(&origem, Some(clientes), &destino, &|_| {}, &nao()).unwrap();
        assert_eq!((e.notas, e.anexos, e.pulados), (2, 2, 0));
        assert_eq!(e.caminho, destino.join("Clientes"));
        assert!(e.mensagem(true).starts_with("Exportadas 2 notas em "));
        assert!(
            destino
                .join("Clientes")
                .join("ACME")
                .join("Acesso.md")
                .exists()
        );
        assert!(
            destino
                .join("Clientes")
                .join("ACME")
                .join("Acesso-attachments")
                .join("foto (1).png")
                .exists()
        );
        assert!(
            !destino
                .join("Clientes")
                .join("ACME")
                .join("banco.md")
                .exists()
        );

        let (mut novo, pasta_b) = cofre_vazio("notas-ida-e-volta-b");
        let i =
            importar_pasta(&mut novo, &destino.join("Clientes"), None, &|_| {}, &nao()).unwrap();
        assert_eq!(i.notas.len(), 2);
        assert_eq!(i.anexos, 2);
        assert_eq!(i.nao_lidos, 0);
        assert_eq!(i.mensagem(), "2 notas e 2 anexos importados.");
        // Mesmas pastas, títulos e anexos; nenhuma pasta `*-attachments`.
        let nomes_das_pastas: Vec<String> = {
            let mut v: Vec<String> = novo
                .pastas()
                .iter()
                .map(|p| novo.caminho(Some(p.id)))
                .collect();
            v.sort();
            v
        };
        assert_eq!(nomes_das_pastas, ["Clientes", "Clientes/ACME"]);
        let antes = resumo(&origem);
        let depois = resumo(&novo);
        assert_eq!(antes.len(), depois.len());
        for (a, d) in antes.iter().zip(&depois) {
            assert_eq!((&a.0, &a.1, &a.3), (&d.0, &d.1, &d.3));
        }
        // O corpo é o mesmo, com o id velho trocado pelo novo, na mesma posição.
        let nota_nova = novo.notas().find(|n| n.titulo == "Acesso").unwrap();
        let novo_id = nota_nova
            .anexos
            .iter()
            .find(|a| a.nome == "foto (1).png")
            .unwrap()
            .id
            .clone();
        assert_ne!(novo_id, foto);
        assert_eq!(
            nota_nova.notas,
            origem
                .notas()
                .find(|n| n.titulo == "Acesso")
                .unwrap()
                .notas
                .replace(&foto, &novo_id)
        );
        // Os bytes voltam iguais.
        let vault = pasta_b.0.join("vault");
        for ficha in &nota_nova.anexos {
            let lido = anexos::ler_para_memoria(&vault, ficha, &nao()).unwrap();
            let esperado: &[u8] = if ficha.nome == "foto (1).png" {
                b"PNG1"
            } else {
                b"LEIA"
            };
            assert_eq!(lido, esperado);
        }
    }

    #[test]
    fn importar_arquivo_troca_caminhos_por_anexos() {
        let (mut cofre, pasta) = cofre_vazio("notas-importar-arquivo");
        let origem = pasta.0.join("origem");
        escrever(&origem.join("img").join("a b.png"), b"AAA");
        escrever(&origem.join("doc.pdf"), b"PDF");
        let md = "# Titulo\n\n![x](img/a%20b.png)\n\n![de novo](img/a%20b.png)\n\n[pdf](doc.pdf#p=2)\n\n\
                  ![web](https://x.com/y.png)\n\n![nao existe](img/nada.png)\n\n![fora](../fora.png)\n";
        escrever(&origem.join("Minha nota.md"), md.as_bytes());
        let destino = cofre.nova_pasta(None, "Importadas").unwrap();
        let i = importar_arquivo(
            &mut cofre,
            &origem.join("Minha nota.md"),
            Some(destino),
            &|_| {},
            &nao(),
        )
        .unwrap();
        assert_eq!(i.notas.len(), 1);
        // O mesmo caminho duas vezes é um anexo só.
        assert_eq!(i.anexos, 2);
        let nota = cofre.item(i.notas[0]).unwrap();
        assert_eq!(nota.titulo, "Minha nota");
        assert_eq!(nota.pasta, Some(destino));
        assert_eq!(nota.anexos.len(), 2);
        let png = nota.anexos.iter().find(|a| a.nome == "a b.png").unwrap();
        let pdf = nota.anexos.iter().find(|a| a.nome == "doc.pdf").unwrap();
        assert_eq!(nota.notas.matches(&format!("anexo:{}", png.id)).count(), 2);
        assert!(nota.notas.contains(&format!("[pdf](anexo:{})", pdf.id)));
        // O que não virou anexo fica como estava.
        assert!(nota.notas.contains("![web](https://x.com/y.png)"));
        assert!(nota.notas.contains("![nao existe](img/nada.png)"));
        assert!(nota.notas.contains("![fora](../fora.png)"));
        assert_eq!(i.mensagem(), "1 nota e 2 anexos importados.");
    }

    #[test]
    fn importar_traz_os_arquivos_da_pasta_attachments_que_ninguem_cita() {
        let (mut cofre, pasta) = cofre_vazio("notas-importar-attachments");
        let origem = pasta.0.join("origem");
        escrever(&origem.join("n-attachments").join("citado.png"), b"C");
        escrever(&origem.join("n-attachments").join("solto.txt"), b"S");
        escrever(&origem.join("n.md"), b"![c](n-attachments/citado.png)\n");
        let i = importar_arquivo(&mut cofre, &origem.join("n.md"), None, &|_| {}, &nao()).unwrap();
        // O citado entra uma vez só (pelo link); o solto entra sem link.
        assert_eq!(i.anexos, 2);
        let nota = cofre.item(i.notas[0]).unwrap();
        let mut nomes: Vec<&str> = nota.anexos.iter().map(|a| a.nome.as_str()).collect();
        nomes.sort_unstable();
        assert_eq!(nomes, ["citado.png", "solto.txt"]);
        let solto = nota.anexos.iter().find(|a| a.nome == "solto.txt").unwrap();
        assert!(!nota.notas.contains(&solto.id));
    }

    #[test]
    fn importar_pasta_ignora_o_que_nao_e_nota_e_une_pastas_de_mesmo_nome() {
        let (mut cofre, pasta) = cofre_vazio("notas-importar-pasta");
        let existente = cofre.nova_pasta(None, "Docs").unwrap();
        let origem = pasta.0.join("Docs");
        escrever(&origem.join("a.md"), b"um");
        escrever(&origem.join("b.markdown"), b"dois");
        escrever(&origem.join("c.TXT"), "três ç".as_bytes());
        escrever(&origem.join("d.pdf"), b"ignorado");
        escrever(&origem.join("sub").join("e.md"), b"quatro");
        escrever(
            &origem.join("sub").join("e-attachments").join("x.bin"),
            b"X",
        );
        escrever(&origem.join(".git").join("f.md"), b"oculta");
        escrever(&origem.join("solta-attachments").join("y.bin"), b"Y");
        // Texto que não é UTF-8.
        escrever(&origem.join("g.md"), &[b'o', b'k', 0xFF, b'!']);
        let i = importar_pasta(&mut cofre, &origem, None, &|_| {}, &nao()).unwrap();
        assert_eq!(i.notas.len(), 5);
        let nomes: Vec<String> = {
            let mut v: Vec<String> = cofre
                .pastas()
                .iter()
                .map(|p| cofre.caminho(Some(p.id)))
                .collect();
            v.sort();
            v
        };
        // Juntou com "Docs", que já existia, e não criou pasta `*-attachments` nem `.git`.
        assert_eq!(nomes, ["Docs", "Docs/sub"]);
        assert!(cofre.pasta(existente).is_some());
        let g = cofre.notas().find(|n| n.titulo == "g").unwrap();
        assert_eq!(g.notas, "ok\u{FFFD}!");
        let e = cofre.notas().find(|n| n.titulo == "e").unwrap();
        assert_eq!(e.anexos.len(), 1);
        assert_eq!(e.anexos[0].nome, "x.bin");
        assert_eq!(cofre.caminho(e.pasta), "Docs/sub");
    }

    #[test]
    fn importar_cancelado_ou_sem_notas_nao_deixa_rastro() {
        let (mut cofre, pasta) = cofre_vazio("notas-importar-cancelar");
        let origem = pasta.0.join("origem");
        escrever(&origem.join("a.txt"), &[1u8; 100]);
        escrever(&origem.join("n.md"), b"[a](a.txt)");
        let cancelar = AtomicBool::new(true);
        let erro = importar_arquivo(&mut cofre, &origem.join("n.md"), None, &|_| {}, &cancelar);
        assert!(matches!(erro, Err(ErroAnexo::Cancelado)));
        assert_eq!(cofre.notas().count(), 0);
        let vault = pasta.0.join("vault");
        assert!(!vault.exists() || fs::read_dir(&vault).unwrap().next().is_none());
        // Pasta sem nota nenhuma.
        let vazia = pasta.0.join("vazia");
        fs::create_dir_all(&vazia).unwrap();
        let i = importar_pasta(&mut cofre, &vazia, None, &|_| {}, &nao()).unwrap();
        assert!(i.notas.is_empty());
        assert_eq!(i.mensagem(), "Nenhuma nota para importar.");
        assert!(cofre.pastas().is_empty());
    }

    #[test]
    fn mensagens_da_importacao() {
        let uma = Importacao {
            notas: vec![1],
            anexos: 0,
            nao_lidos: 0,
        };
        assert_eq!(uma.mensagem(), "1 nota importada.");
        let duas = Importacao {
            notas: vec![1, 2],
            anexos: 0,
            nao_lidos: 0,
        };
        assert_eq!(duas.mensagem(), "2 notas importadas.");
        let com = Importacao {
            notas: vec![1],
            anexos: 1,
            nao_lidos: 1,
        };
        assert_eq!(
            com.mensagem(),
            "1 nota e 1 anexo importados. 1 anexo não pôde ser lido."
        );
        let varios = Importacao {
            notas: vec![1],
            anexos: 3,
            nao_lidos: 2,
        };
        assert!(
            varios
                .mensagem()
                .ends_with("2 anexos não puderam ser lidos.")
        );
    }
}
