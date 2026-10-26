//! Colar imagem e arquivos no terminal (M45): o que liga o terminal à fila, ao plano B e ao
//! fluxo `tar`. A parte pura (nomes, aspas, limpeza, comandos) está em [`crate::colar`].
//!
//! Cada sessão que recebe a colagem ganha uma [`Colagem`] e uma thread. A thread prepara tudo o
//! que bloqueia (a pasta no servidor, os nomes, a medida das pastas, a sondagem do `tar`) e
//! avisa a tela por um [`Live`] de [`Evento`]s: primeiro o [`Preparo`], com o que a fila deve
//! enviar e o que colar, depois o [`Evento::Fim`] (a thread também envia, em fluxo, o que não
//! vai pela fila). A tela põe os itens na fila, confere a fila e a thread a cada mudança e, com
//! tudo pronto, cola os caminhos. O progresso que a fila não vê (plano B e `tar`) vai por outro
//! [`Live`], o da própria colagem.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use jiff::civil::DateTime;
use tomni_ui::ClipboardImage;
use tomni_ui::files::{
    Conflict, Endpoint, Fs, Live, Opener as ArquivosOpener, TransferInfo, TransferState, path,
};
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::ExecOutput;
use tomni_ui::terminal::{RichPaste, RichPasteContent, Session};

use super::Tela;
use crate::abrir::ExecDaSessao;
use crate::arquivos::editar::caminho_em_texto;
use crate::arquivos::formato::{faixa, tamanho, texto_da_faixa};
use crate::colar::{self as pura, Sondagem, pacote};
use crate::configuracoes::ColarArquivos;
use crate::janelas::Janela;
use crate::modelo::{Connection, Kind, ShellRemoto};
use crate::t;

/// Acima disto (100 MB) a barra avisa que pode demorar.
const GRANDE: u64 = 100 * 1024 * 1024;

/// De quanto em quanto a thread atualiza o progresso que a fila não vê.
const RITMO: Duration = Duration::from_millis(100);

/// Por que o envio não terminou.
#[derive(Clone, Debug)]
enum Falha {
    /// A pessoa cancelou.
    Cancelada,
    /// Deu errado, com o motivo como veio.
    Erro(String),
}

impl From<io::Error> for Falha {
    fn from(erro: io::Error) -> Self {
        Self::Erro(erro.to_string())
    }
}

impl From<String> for Falha {
    fn from(erro: String) -> Self {
        Self::Erro(erro)
    }
}

type Etapa<T> = Result<T, Falha>;

fn parar_se(cancelar: &AtomicBool) -> Etapa<()> {
    if cancelar.load(Ordering::Relaxed) {
        Err(Falha::Cancelada)
    } else {
        Ok(())
    }
}

/// A pasta configurada não deu: a que foi usada, a configurada e o motivo.
type AvisoDaPasta = (String, String, String);

/// O que a thread entrega à tela quando acaba de preparar.
struct Preparo {
    /// Os caminhos a colar, na ordem em que a pessoa deu os arquivos.
    remotos: Vec<String>,
    /// O que a fila envia: o índice em `remotos` e o caminho local.
    fila: Vec<(usize, String)>,
    /// De onde a fila abre o servidor; vazio fora do envio pelo SFTP.
    destino: Option<ArquivosOpener>,
    /// O aviso de que a pasta configurada não deu.
    aviso: Option<AvisoDaPasta>,
    /// A sondagem do `tar` feita agora (a tela a guarda por sessão).
    sondagem: Option<Sondagem>,
    /// A limpeza por idade já foi feita nesta sessão.
    limpou: bool,
    /// As cópias locais (imagem colada) que a tela apaga quando o envio acaba.
    temporarias: Vec<String>,
    /// `remotos` são caminhos desta máquina (colar sem enviar).
    local: bool,
    /// O total em bytes, para a barra.
    total: u64,
    /// Os nomes mostrados na barra.
    nomes: Vec<String>,
}

enum Evento {
    Pronta(Preparo),
    Fim(Result<(), Falha>),
}

/// Os eventos de todas as colagens, na ordem em que as threads os gravaram.
type Eventos = Live<Vec<(u64, Evento)>>;

/// O que a barra de uma colagem lê.
#[derive(Clone, Debug, Default)]
struct Progresso {
    /// O total em bytes (0: ainda não se sabe).
    total: u64,
    /// Os bytes já enviados fora da fila (plano B e `tar`).
    fora: u64,
    /// Os nomes mostrados.
    nomes: Vec<String>,
    /// Os itens que a fila recebeu.
    ids: Vec<u64>,
    /// A preparação acabou.
    pronta: bool,
}

/// Uma colagem em andamento.
pub(super) struct Colagem {
    id: u64,
    sessao: Session,
    /// A janela em que a barra aparece.
    janela: WindowHandle,
    /// O nome da conexão.
    conexao: String,
    /// O fim dos ids da barra: vazio na primeira da janela, `-2`, `-3`… nas outras.
    sufixo: String,
    /// A barra aparece (colar o caminho de uma imagem local não mostra nada).
    visivel: bool,
    /// O shell do servidor, para as aspas.
    shell: ShellRemoto,
    cancelar: Arc<AtomicBool>,
    progresso: Live<Progresso>,
    interno: RefCell<Interno>,
}

#[derive(Default)]
struct Interno {
    /// Os caminhos a colar e, de cada um, o item da fila que o leva.
    remotos: Vec<(String, Option<u64>)>,
    local: bool,
    temporarias: Vec<String>,
    /// Os ids da fila.
    ids: Vec<u64>,
    pronta: bool,
    fim: Option<Result<(), Falha>>,
}

/// Um aviso (erro ou observação) na parte de cima dos terminais.
#[derive(Clone)]
pub(super) struct AvisoColar {
    id: u64,
    janela: WindowHandle,
    /// O id do widget: `colar-erro`, `colar-aviso` ou `colar-entregue`, com `-2`… se há mais
    /// de um.
    widget: String,
    severidade: InfoBarSeverity,
    titulo: String,
    texto: String,
}

/// O estado do colar na tela.
pub(super) struct EstadoColar {
    eventos: Eventos,
    ativas: RefCell<Vec<Rc<Colagem>>>,
    /// Muda quando `ativas` muda, para as barras acompanharem.
    versao: Signal<u64>,
    avisos: Signal<Vec<AvisoColar>>,
    proxima: Cell<u64>,
    proximo_aviso: Cell<u64>,
    /// As sessões que já tiveram a limpeza por idade.
    limpas: RefCell<HashSet<Session>>,
    /// O que cada sessão sabe do `tar` do servidor.
    sondagens: RefCell<HashMap<Session, Sondagem>>,
    /// As sessões que já ouviram que a conexão não recebe arquivos.
    avisadas: RefCell<HashSet<Session>>,
    /// A pasta local das imagens coladas já passou pela limpeza nesta execução.
    local_limpo: Cell<bool>,
    /// A colagem à espera da resposta do "Perguntar".
    pendente: RefCell<Option<(RichPaste, Janela)>>,
    dialogo_texto: Signal<String>,
    dialogo_itens: Signal<String>,
    lembrar: Signal<bool>,
}

impl EstadoColar {
    pub(super) fn novo() -> Self {
        Self {
            eventos: Live::new(Vec::new()),
            ativas: RefCell::default(),
            versao: signal(0),
            avisos: signal(Vec::new()),
            proxima: Cell::new(0),
            proximo_aviso: Cell::new(0),
            limpas: RefCell::default(),
            sondagens: RefCell::default(),
            avisadas: RefCell::default(),
            local_limpo: Cell::new(false),
            pendente: RefCell::default(),
            dialogo_texto: signal(String::new()),
            dialogo_itens: signal(String::new()),
            lembrar: signal(false),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// A thread

/// O que se cola.
enum Conteudo {
    Arquivos(Vec<PathBuf>),
    Imagem(ClipboardImage),
}

/// O que a conexão pede do envio.
struct Envio {
    sftp: Result<ArquivosOpener, String>,
    exec: Result<ExecDaSessao, String>,
    /// A pasta no servidor, como está nas configurações (pode começar com `~`).
    pasta: String,
    dias: u32,
    tar_min: u32,
    /// A conexão já comprime: o `tar` não precisa.
    comprimida: bool,
    /// É o primeiro envio da sessão: faz a limpeza por idade.
    limpar: bool,
    sondagem: Option<Sondagem>,
}

enum Modo {
    /// Só guarda a imagem aqui e cola o caminho local.
    Local,
    Enviar(Envio),
}

struct Pedido {
    colagem: u64,
    conteudo: Conteudo,
    modo: Modo,
    /// O disco desta máquina.
    disco: ArquivosOpener,
    /// A pasta local das imagens coladas.
    pasta_dados: PathBuf,
    /// Quando limpar essa pasta (em dias); `None` se já limpou nesta execução.
    limpar_local: Option<u32>,
    quando: DateTime,
    cancelar: Arc<AtomicBool>,
    eventos: Eventos,
    progresso: Live<Progresso>,
}

/// Um arquivo ou pasta a colar.
struct Item {
    /// O caminho no `Fs` do disco.
    local: String,
    /// O caminho como o shell desta máquina o conhece.
    nativo: String,
    nome: String,
    pasta: bool,
    bytes: u64,
    /// É a cópia local de uma imagem colada.
    temporaria: bool,
}

/// O que há dentro de uma pasta local.
#[derive(Default)]
struct Medida {
    arquivos: u64,
    bytes: u64,
    /// Cada entrada: o caminho relativo, se é pasta e o tamanho.
    lista: Vec<(String, bool, u64)>,
}

fn trabalhar(pedido: Pedido) {
    let resultado = rodar(&pedido);
    pedido
        .eventos
        .update(|lista| lista.push((pedido.colagem, Evento::Fim(resultado))));
}

fn rodar(p: &Pedido) -> Etapa<()> {
    let mut disco = (p.disco)()?;
    let itens = reunir(p, &mut *disco)?;
    match &p.modo {
        Modo::Local => {
            let remotos = itens.iter().map(|i| i.nativo.clone()).collect();
            let nomes = itens.iter().map(|i| i.nome.clone()).collect();
            avisar_preparo(
                p,
                Preparo {
                    remotos,
                    fila: Vec::new(),
                    destino: None,
                    aviso: None,
                    sondagem: None,
                    limpou: false,
                    temporarias: Vec::new(),
                    local: true,
                    total: 0,
                    nomes,
                },
            );
            Ok(())
        }
        Modo::Enviar(envio) => enviar(p, envio, itens, &mut *disco),
    }
}

fn avisar_preparo(p: &Pedido, preparo: Preparo) {
    p.eventos
        .update(|lista| lista.push((p.colagem, Evento::Pronta(preparo))));
}

/// Cria `pasta` e o que faltar acima dela. Devolve se criou a última.
fn garantir_pasta(fs: &mut dyn Fs, pasta: &str) -> io::Result<bool> {
    match fs.stat(pasta) {
        Ok(entrada) if entrada.is_dir() => return Ok(false),
        Ok(_) => return Err(io::Error::other("não é uma pasta")),
        Err(_) => {}
    }
    let pai = path::parent(pasta);
    if pai != pasta {
        garantir_pasta(fs, &pai)?;
    }
    match fs.mkdir(pasta) {
        Ok(()) => Ok(true),
        Err(erro) if erro.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(erro) => Err(erro),
    }
}

/// Junta o que a pessoa deu num só jeito de ver: o caminho, o nome e o tamanho. A imagem é
/// gravada aqui, na pasta de dados, e vira o arquivo a enviar.
fn reunir(p: &Pedido, fs: &mut dyn Fs) -> Etapa<Vec<Item>> {
    match &p.conteudo {
        Conteudo::Arquivos(caminhos) => caminhos
            .iter()
            .map(|caminho| {
                parar_se(&p.cancelar)?;
                let local = caminho_em_texto(caminho);
                let entrada = fs
                    .stat(&local)
                    .map_err(|erro| Falha::Erro(format!("{local}: {erro}")))?;
                let nome = caminho.file_name().map_or_else(
                    || path::file_name(&local).to_owned(),
                    |nome| nome.to_string_lossy().into_owned(),
                );
                Ok(Item {
                    local,
                    nativo: caminho.display().to_string(),
                    nome,
                    pasta: entrada.is_dir(),
                    bytes: entrada.size,
                    temporaria: false,
                })
            })
            .collect(),
        Conteudo::Imagem(imagem) => {
            let png = imagem.to_png()?;
            let pasta = caminho_em_texto(&p.pasta_dados);
            garantir_pasta(fs, &pasta)?;
            let mut existentes: Vec<String> = fs
                .list(&pasta)
                .map(|lista| lista.into_iter().map(|e| e.name).collect())
                .unwrap_or_default();
            if let Some(dias) = p.limpar_local {
                for nome in pura::vencidos(&existentes, p.quando, dias) {
                    let _ = pura::apagar_arvore(fs, &path::join(&pasta, &nome));
                    existentes.retain(|e| e != &nome);
                }
            }
            let nome = pura::sem_colisao(&pura::nome_da_imagem(p.quando), &existentes);
            let local = path::join(&pasta, &nome);
            let mut arquivo = fs.open_write(&local, 0)?;
            arquivo.write_all(&png)?;
            arquivo.finish()?;
            Ok(vec![Item {
                nativo: p.pasta_dados.join(&nome).display().to_string(),
                local,
                nome,
                pasta: false,
                bytes: png.len() as u64,
                temporaria: true,
            }])
        }
    }
}

/// Percorre uma pasta local com as regras da fila: um link para arquivo vira o arquivo, e um
/// link para pasta (ou o que não é arquivo nem pasta) fica de fora.
fn medir(
    fs: &mut dyn Fs,
    pasta: &str,
    relativo: &str,
    saida: &mut Medida,
    cancelar: &AtomicBool,
) -> Etapa<()> {
    let mut filhos = fs.list(pasta)?;
    filhos.sort_by(|a, b| a.name.cmp(&b.name));
    for filho in filhos {
        parar_se(cancelar)?;
        let aqui = path::join(pasta, &filho.name);
        let entrada = match filho.kind {
            tomni_ui::files::EntryKind::File | tomni_ui::files::EntryKind::Dir => filho,
            tomni_ui::files::EntryKind::Symlink => match fs.stat(&aqui) {
                Ok(alvo) if alvo.is_file() => tomni_ui::files::Entry {
                    name: filho.name,
                    ..alvo
                },
                _ => continue,
            },
            tomni_ui::files::EntryKind::Other => continue,
        };
        let rel = if relativo.is_empty() {
            entrada.name.clone()
        } else {
            format!("{relativo}/{}", entrada.name)
        };
        if entrada.is_dir() {
            saida.lista.push((rel.clone(), true, 0));
            medir(fs, &aqui, &rel, saida, cancelar)?;
        } else {
            saida.arquivos += 1;
            saida.bytes += entrada.size;
            saida.lista.push((rel, false, entrada.size));
        }
    }
    Ok(())
}

/// Pergunta ao servidor o que ele sabe de `tar`: lê um pacote vazio, com gzip e sem.
fn sondar(exec: &Result<ExecDaSessao, String>) -> Sondagem {
    let Ok(exec) = exec else {
        return Sondagem::SemTar;
    };
    let aceita = |comando: &str, entrada: Vec<u8>| {
        exec(comando, &mut entrada.as_slice(), &mut |_| {}).is_ok_and(|saida| saida.success())
    };
    if aceita(pura::SONDA_GZIP, pacote::vazio(true)) {
        Sondagem::TarGzip
    } else if aceita(pura::SONDA_TAR, pacote::vazio(false)) {
        Sondagem::Tar
    } else {
        Sondagem::SemTar
    }
}

/// Os nomes no servidor, na ordem dos itens, sem repetir nenhum que já esteja lá.
fn nomear(
    itens: &[Item],
    existentes: &mut Vec<String>,
    pasta: &str,
    quando: DateTime,
    windows: bool,
) -> Vec<String> {
    itens
        .iter()
        .map(|item| {
            let nome = if item.temporaria {
                item.nome.clone()
            } else {
                pura::nome_remoto(&item.nome, quando, windows)
            };
            let nome = pura::sem_colisao(&nome, existentes);
            existentes.push(nome.clone());
            path::join(pasta, &nome)
        })
        .collect()
}

/// A pasta com `~` aberto.
fn expandir(home: &str, pasta: &str) -> String {
    match pasta.strip_prefix('~') {
        Some("") => home.to_owned(),
        Some(resto) if resto.starts_with('/') => path::join(home, &resto[1..]),
        _ => pasta.to_owned(),
    }
}

/// Cria a pasta (0700, se foi criada agora) e confere que dá para escrever nela.
fn preparar_pasta(fs: &mut dyn Fs, pasta: &str) -> io::Result<()> {
    if garantir_pasta(fs, pasta)? {
        let _ = fs.set_permissions(pasta, 0o700);
    }
    let teste = path::join(pasta, ".tirech-teste");
    fs.open_write(&teste, 0)?.finish()?;
    fs.remove_file(&teste)
}

/// A pasta da configuração; se ela não dá, `~/.cache/tirech-colar`, com o aviso do motivo.
fn escolher_pasta(
    fs: &mut dyn Fs,
    home: &str,
    configurada: &str,
) -> Etapa<(String, Option<AvisoDaPasta>)> {
    let pasta = expandir(home, configurada);
    match preparar_pasta(fs, &pasta) {
        Ok(()) => Ok((pasta, None)),
        Err(motivo) => {
            let reserva = expandir(home, pura::PASTA_RESERVA);
            preparar_pasta(fs, &reserva)
                .map_err(|erro| Falha::Erro(format!("{reserva}: {erro}")))?;
            let aviso = (reserva.clone(), pasta, motivo.to_string());
            Ok((reserva, Some(aviso)))
        }
    }
}

/// A conta do progresso que a fila não vê: soma o que já terminou e o que o item atual andou.
struct Ritmo<'a> {
    progresso: &'a Live<Progresso>,
    base: u64,
    ultimo: Option<Instant>,
}

impl Ritmo<'_> {
    /// Anota `feito` (do item atual), no máximo a cada [`RITMO`].
    fn marcar(&mut self, feito: u64) {
        if self.ultimo.is_none_or(|t| t.elapsed() >= RITMO) {
            self.ultimo = Some(Instant::now());
            self.fim(feito);
        }
    }

    fn fim(&self, feito: u64) {
        let total = self.base + feito;
        self.progresso.update(|p| p.fora = total);
    }
}

/// Um leitor que para quando a colagem é cancelada.
struct Leitor<'a, R> {
    dentro: R,
    cancelar: &'a AtomicBool,
    depois: Box<dyn FnMut() + 'a>,
}

impl<R: Read> Read for Leitor<'_, R> {
    fn read(&mut self, saida: &mut [u8]) -> io::Result<usize> {
        if self.cancelar.load(Ordering::Relaxed) {
            return Err(io::Error::other("cancelado"));
        }
        let lidos = self.dentro.read(saida)?;
        (self.depois)();
        Ok(lidos)
    }
}

/// A primeira linha do que o comando escreveu em `stderr`, ou o código de saída.
fn motivo_do_comando(saida: &ExecOutput) -> String {
    let erro = saida.stderr_text();
    match erro.lines().find(|l| !l.trim().is_empty()) {
        Some(linha) => linha.trim().to_owned(),
        None => match saida.status {
            Some(codigo) => format!("o comando terminou com o código {codigo}"),
            None => "o comando terminou sem código".to_owned(),
        },
    }
}

/// Manda a pasta `local` num fluxo `tar` (gzip ou não) para o `comando`, que a extrai.
fn enviar_tar(
    exec: &ExecDaSessao,
    abrir_disco: &ArquivosOpener,
    local: &str,
    comando: &str,
    gzip: bool,
    p: &Pedido,
    ritmo: &mut Ritmo,
) -> Etapa<()> {
    let (leitor, escritor) = io::pipe()?;
    let enviados = Arc::new(AtomicU64::new(0));
    let produtora = {
        let (abrir_disco, local) = (Arc::clone(abrir_disco), local.to_owned());
        let (enviados, cancelar) = (Arc::clone(&enviados), Arc::clone(&p.cancelar));
        std::thread::spawn(move || {
            let mut fs = abrir_disco()?;
            pacote::escrever(&mut *fs, &local, escritor, gzip, &enviados, &cancelar)
        })
    };
    let saida = {
        let contados = Arc::clone(&enviados);
        let mut entrada = Leitor {
            dentro: leitor,
            cancelar: &p.cancelar,
            depois: Box::new(|| ritmo.marcar(contados.load(Ordering::Relaxed))),
        };
        exec(comando, &mut entrada, &mut |_| {})
    };
    // O leitor já foi solto: se o servidor acabou antes, a produtora para no `BrokenPipe`.
    let escrita = produtora
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("a thread do pacote caiu")));
    parar_se(&p.cancelar)?;
    let saida = saida.map_err(Falha::Erro)?;
    if !saida.success() {
        return Err(Falha::Erro(motivo_do_comando(&saida)));
    }
    escrita?;
    ritmo.fim(enviados.load(Ordering::Relaxed));
    Ok(())
}

/// Os itens que vão num fluxo `tar` e a sondagem que valeu.
fn escolher_tar(
    p: &Pedido,
    envio: &Envio,
    itens: &[Item],
    medidas: &HashMap<usize, Medida>,
) -> (Vec<usize>, Option<Sondagem>, Sondagem) {
    let candidatas: Vec<usize> = itens
        .iter()
        .enumerate()
        .filter(|(i, item)| {
            item.pasta
                && medidas
                    .get(i)
                    .is_some_and(|m| pura::usar_tar(m.arquivos, m.bytes, envio.tar_min))
        })
        .map(|(i, _)| i)
        .collect();
    let _ = p;
    if candidatas.is_empty() {
        return (candidatas, None, envio.sondagem.unwrap_or(Sondagem::SemTar));
    }
    let (sondagem, nova) = match envio.sondagem {
        Some(sondagem) => (sondagem, None),
        None => {
            let sondagem = sondar(&envio.exec);
            (sondagem, Some(sondagem))
        }
    };
    if sondagem == Sondagem::SemTar {
        return (Vec::new(), nova, sondagem);
    }
    (candidatas, nova, sondagem)
}

fn gzip_da_pasta(envio: &Envio, medida: &Medida, sondagem: Sondagem) -> bool {
    let arquivos: Vec<(String, u64)> = medida
        .lista
        .iter()
        .filter(|(_, pasta, _)| !pasta)
        .map(|(nome, _, bytes)| (nome.clone(), *bytes))
        .collect();
    pura::comprimir_fluxo(&arquivos, sondagem, envio.comprimida)
}

fn medir_pastas(
    p: &Pedido,
    itens: &mut [Item],
    disco: &mut dyn Fs,
) -> Etapa<HashMap<usize, Medida>> {
    let mut medidas = HashMap::new();
    for (i, item) in itens.iter_mut().enumerate() {
        if !item.pasta {
            continue;
        }
        let mut medida = Medida::default();
        medir(disco, &item.local, "", &mut medida, &p.cancelar)?;
        item.bytes = medida.bytes;
        medidas.insert(i, medida);
    }
    Ok(medidas)
}

fn enviar(p: &Pedido, envio: &Envio, mut itens: Vec<Item>, disco: &mut dyn Fs) -> Etapa<()> {
    let medidas = medir_pastas(p, &mut itens, disco)?;
    let aberto = match &envio.sftp {
        Ok(abrir) => abrir()
            .map(|fs| (fs, Arc::clone(abrir)))
            .map_err(|e| e.to_string()),
        Err(motivo) => Err(motivo.clone()),
    };
    match aberto {
        Ok((fs, abrir)) => com_sftp(p, envio, itens, &medidas, disco, fs, abrir),
        // O servidor não tem SFTP (ou o recusou): tudo vai pelo shell.
        Err(motivo) => match &envio.exec {
            Ok(exec) => plano_b(p, envio, exec, itens, &medidas, disco),
            Err(_) => Err(Falha::Erro(motivo)),
        },
    }
}

fn com_sftp(
    p: &Pedido,
    envio: &Envio,
    itens: Vec<Item>,
    medidas: &HashMap<usize, Medida>,
    disco: &mut dyn Fs,
    mut fs: Box<dyn Fs>,
    abrir: ArquivosOpener,
) -> Etapa<()> {
    let _ = disco;
    let home = fs.home()?;
    let (pasta, aviso) = escolher_pasta(&mut *fs, &home, &envio.pasta)?;
    let mut existentes: Vec<String> = fs.list(&pasta)?.into_iter().map(|e| e.name).collect();
    if envio.limpar {
        for nome in pura::vencidos(&existentes, p.quando, envio.dias) {
            let _ = pura::apagar_arvore(&mut *fs, &path::join(&pasta, &nome));
            existentes.retain(|e| e != &nome);
        }
    }
    let remotos = nomear(
        &itens,
        &mut existentes,
        &pasta,
        p.quando,
        pura::e_windows(&pasta),
    );
    let (por_tar, nova, sondagem) = escolher_tar(p, envio, &itens, medidas);
    let fila = itens
        .iter()
        .enumerate()
        .filter(|(i, _)| !por_tar.contains(i))
        .map(|(i, item)| (i, item.local.clone()))
        .collect();
    avisar_preparo(
        p,
        Preparo {
            remotos: remotos.clone(),
            fila,
            destino: Some(abrir),
            aviso,
            sondagem: nova,
            limpou: envio.limpar,
            temporarias: temporarias(&itens),
            local: false,
            total: itens.iter().map(|i| i.bytes).sum(),
            nomes: itens.iter().map(|i| i.nome.clone()).collect(),
        },
    );
    let Ok(exec) = &envio.exec else {
        return Ok(());
    };
    let mut feito = 0;
    for i in por_tar {
        parar_se(&p.cancelar)?;
        let destino = &remotos[i];
        let gzip = gzip_da_pasta(envio, &medidas[&i], sondagem);
        let comando = pura::comando_tar(destino, ShellRemoto::Automatico, gzip);
        let enviado = (|| {
            fs.mkdir(destino)?;
            let mut ritmo = Ritmo {
                progresso: &p.progresso,
                base: feito,
                ultimo: None,
            };
            enviar_tar(
                exec,
                &p.disco,
                &itens[i].local,
                &comando,
                gzip,
                p,
                &mut ritmo,
            )
        })();
        if let Err(falha) = enviado {
            let _ = pura::apagar_arvore(&mut *fs, destino);
            return Err(falha);
        }
        feito += itens[i].bytes;
        p.progresso.update(|pr| pr.fora = feito);
    }
    Ok(())
}

fn temporarias(itens: &[Item]) -> Vec<String> {
    itens
        .iter()
        .filter(|i| i.temporaria)
        .map(|i| i.local.clone())
        .collect()
}

/// Roda `comando` sem entrada e devolve a saída, ou o erro como `Falha`.
fn comando_simples(exec: &ExecDaSessao, comando: &str) -> Etapa<ExecOutput> {
    exec(comando, &mut io::empty(), &mut |_| {}).map_err(Falha::Erro)
}

fn plano_b(
    p: &Pedido,
    envio: &Envio,
    exec: &ExecDaSessao,
    itens: Vec<Item>,
    medidas: &HashMap<usize, Medida>,
    disco: &mut dyn Fs,
) -> Etapa<()> {
    let home = || -> Etapa<String> {
        let saida = comando_simples(exec, r#"echo "$HOME""#)?;
        if !saida.success() {
            return Err(Falha::Erro(motivo_do_comando(&saida)));
        }
        Ok(saida.stdout_text().trim().to_owned())
    };
    let mut pasta = if envio.pasta.starts_with('~') {
        expandir(&home()?, &envio.pasta)
    } else {
        envio.pasta.clone()
    };
    let mut aviso = None;
    if !comando_simples(exec, &pura::comando_criar_pasta(&pasta))?.success() {
        let reserva = expandir(&home()?, pura::PASTA_RESERVA);
        let saida = comando_simples(exec, &pura::comando_criar_pasta(&reserva))?;
        if !saida.success() {
            return Err(Falha::Erro(format!(
                "{reserva}: {}",
                motivo_do_comando(&saida)
            )));
        }
        aviso = Some((reserva.clone(), pasta, motivo_do_comando(&saida)));
        pasta = reserva;
    }
    let listagem = comando_simples(exec, &pura::comando_listar(&pasta))?;
    let mut existentes: Vec<String> = if listagem.success() {
        listagem.stdout_text().lines().map(str::to_owned).collect()
    } else {
        Vec::new()
    };
    if envio.limpar {
        let vencidos = pura::vencidos(&existentes, p.quando, envio.dias);
        if !vencidos.is_empty() {
            let _ = comando_simples(exec, &pura::comando_apagar(&pasta, &vencidos));
            existentes.retain(|e| !vencidos.contains(e));
        }
    }
    let remotos = nomear(&itens, &mut existentes, &pasta, p.quando, false);
    let (por_tar, nova, sondagem) = escolher_tar(p, envio, &itens, medidas);
    avisar_preparo(
        p,
        Preparo {
            remotos: remotos.clone(),
            fila: Vec::new(),
            destino: None,
            aviso,
            sondagem: nova,
            limpou: envio.limpar,
            temporarias: temporarias(&itens),
            local: false,
            total: itens.iter().map(|i| i.bytes).sum(),
            nomes: itens.iter().map(|i| i.nome.clone()).collect(),
        },
    );
    let mut feito = 0;
    for (i, item) in itens.iter().enumerate() {
        parar_se(&p.cancelar)?;
        let destino = &remotos[i];
        let resultado = if por_tar.contains(&i) {
            let gzip = gzip_da_pasta(envio, &medidas[&i], sondagem);
            let comando = pura::comando_tar_criando(destino, gzip);
            let mut ritmo = Ritmo {
                progresso: &p.progresso,
                base: feito,
                ultimo: None,
            };
            let r = enviar_tar(exec, &p.disco, &item.local, &comando, gzip, p, &mut ritmo);
            if r.is_err() {
                let nome = path::file_name(destino).to_owned();
                let _ = comando_simples(exec, &pura::comando_apagar(&pasta, &[nome]));
            }
            r
        } else if item.pasta {
            enviar_pasta_por_cat(p, exec, disco, item, destino, &medidas[&i], feito)
        } else {
            enviar_arquivo_por_cat(p, exec, disco, &item.local, destino, feito)
        };
        resultado?;
        feito += item.bytes;
        p.progresso.update(|pr| pr.fora = feito);
    }
    Ok(())
}

fn enviar_arquivo_por_cat(
    p: &Pedido,
    exec: &ExecDaSessao,
    disco: &mut dyn Fs,
    local: &str,
    destino: &str,
    base: u64,
) -> Etapa<()> {
    let arquivo = disco.open_read(local, 0)?;
    let mut entrada = Leitor {
        dentro: arquivo,
        cancelar: &p.cancelar,
        depois: Box::new(|| {}),
    };
    let mut ritmo = Ritmo {
        progresso: &p.progresso,
        base,
        ultimo: None,
    };
    let saida = exec(
        &pura::comando_enviar_arquivo(destino),
        &mut entrada,
        &mut |enviado| ritmo.marcar(enviado),
    );
    drop(entrada);
    parar_se(&p.cancelar)?;
    let saida = saida.map_err(Falha::Erro)?;
    if !saida.success() {
        return Err(Falha::Erro(motivo_do_comando(&saida)));
    }
    Ok(())
}

/// Uma pasta pequena no plano B: cria as pastas e manda um arquivo por vez.
fn enviar_pasta_por_cat(
    p: &Pedido,
    exec: &ExecDaSessao,
    disco: &mut dyn Fs,
    item: &Item,
    destino: &str,
    medida: &Medida,
    base: u64,
) -> Etapa<()> {
    let criar = |pasta: &str| -> Etapa<()> {
        let saida = comando_simples(exec, &pura::comando_criar_pasta(pasta))?;
        if saida.success() {
            Ok(())
        } else {
            Err(Falha::Erro(motivo_do_comando(&saida)))
        }
    };
    criar(destino)?;
    let mut feito = base;
    for (relativo, pasta, bytes) in &medida.lista {
        parar_se(&p.cancelar)?;
        let aqui = path::join(destino, relativo);
        if *pasta {
            criar(&aqui)?;
        } else {
            let local = path::join(&item.local, relativo);
            enviar_arquivo_por_cat(p, exec, disco, &local, &aqui, feito)?;
            feito += bytes;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// A tela

impl Tela {
    /// A conexão de uma sessão aberta pela lista de conexões.
    pub(super) fn conexao_do_colar(&self, sessao: Session) -> Option<Connection> {
        let id = self.conexao_da_sessao(sessao)?;
        self.tree
            .with_untracked(|arvore| arvore.connection(id).cloned())
    }

    /// Trata o que o terminal colou além de texto: imagem ou arquivos. Cada sessão recebe a sua
    /// cópia e o seu caminho.
    pub(super) fn colar_rico(&self, colagem: RichPaste, janela: Janela) {
        let global = self.config.with_untracked(|c| c.terminal.colar_arquivos);
        let mut perguntar = Vec::new();
        for &sessao in &colagem.sessions {
            let conexao = self.conexao_do_colar(sessao);
            let modo = if self.opener.sessao_tem_arquivos(sessao) {
                conexao
                    .as_ref()
                    .and_then(|c| c.colar_arquivos)
                    .unwrap_or(global)
            } else {
                self.avisar_sem_envio(sessao, conexao.as_ref(), janela);
                ColarArquivos::CaminhoLocal
            };
            match modo {
                ColarArquivos::Enviar => {
                    self.colar_enviar(sessao, &colagem.content, janela, conexao);
                }
                ColarArquivos::CaminhoLocal => {
                    self.colar_local(sessao, &colagem.content, janela, conexao);
                }
                ColarArquivos::Perguntar => perguntar.push(sessao),
            }
        }
        if !perguntar.is_empty() {
            self.perguntar_colar(
                RichPaste {
                    sessions: perguntar,
                    ..colagem
                },
                janela,
            );
        }
    }

    /// No Telnet a colagem vai como caminho local, e a pessoa fica sabendo uma vez por sessão.
    fn avisar_sem_envio(&self, sessao: Session, conexao: Option<&Connection>, janela: Janela) {
        let Some(conexao) = conexao.filter(|c| c.kind == Kind::Telnet) else {
            return;
        };
        if !self.colar.avisadas.borrow_mut().insert(sessao) {
            return;
        }
        self.avisar_colar(
            janela.handle,
            InfoBarSeverity::Warning,
            "colar-aviso",
            t!("colar-aviso-titulo"),
            t!("colar-aviso-sem-envio", conexao = conexao.name.as_str()),
        );
    }

    /// A linha de colar acompanha o modo das Configurações.
    pub(super) fn sincronizar_colar(&self) {
        let modo = self
            .id_selecionado()
            .and_then(|id| {
                self.tree
                    .with_untracked(|t| t.connection(id).map(|c| c.colar_arquivos))
            })
            .unwrap_or_default();
        super::poe(self.campos.colar_arquivos, super::rotulo_de_colar(modo));
    }

    /// Põe um aviso na parte de cima dos terminais de `janela`.
    pub(super) fn avisar_colar(
        &self,
        janela: WindowHandle,
        severidade: InfoBarSeverity,
        base: &str,
        titulo: String,
        texto: String,
    ) {
        let id = self.colar.proximo_aviso.get() + 1;
        self.colar.proximo_aviso.set(id);
        let iguais = self.colar.avisos.with_untracked(|avisos| {
            avisos
                .iter()
                .filter(|a| a.janela == janela && a.widget.starts_with(base))
                .count()
        });
        let widget = if iguais == 0 {
            base.to_owned()
        } else {
            format!("{base}-{}", iguais + 1)
        };
        self.colar.avisos.update(|avisos| {
            avisos.push(AvisoColar {
                id,
                janela,
                widget,
                severidade,
                titulo,
                texto,
            });
        });
    }

    /// O aviso de que o que a pessoa colou foi para o assistente da aba, e não para o shell.
    pub(super) fn avisar_entrega(&self, janela: WindowHandle, texto: String) {
        self.avisar_colar(
            janela,
            InfoBarSeverity::Informational,
            "colar-entregue",
            t!("colar-entregue-titulo"),
            texto,
        );
    }

    /// Cola o caminho desta máquina: o dos arquivos como estão, o da imagem depois de gravá-la.
    fn colar_local(
        &self,
        sessao: Session,
        conteudo: &RichPasteContent,
        janela: Janela,
        conexao: Option<Connection>,
    ) {
        match conteudo {
            RichPasteContent::Files(caminhos) => {
                let locais: Vec<String> =
                    caminhos.iter().map(|c| c.display().to_string()).collect();
                self.colar_locais(sessao, janela.handle, &locais);
            }
            RichPasteContent::Image(imagem) => {
                let nome = conexao.map_or_else(|| sessao.title(), |c| c.name);
                let colagem =
                    self.nova_colagem(sessao, janela, nome, ShellRemoto::Automatico, false);
                let dias = self
                    .config
                    .with_untracked(|c| c.terminal.limpar_colados_dias);
                self.iniciar(
                    &colagem,
                    Conteudo::Imagem(imagem.clone()),
                    Modo::Local,
                    (!self.colar.local_limpo.replace(true)).then_some(dias),
                );
            }
        }
    }

    /// Cola os caminhos desta máquina, que passam a valer como origem do `enviar_arquivo`. Numa
    /// aba que um assistente usa, vão para ele em vez do shell.
    fn colar_locais(&self, sessao: Session, janela: WindowHandle, locais: &[String]) {
        self.liberar_para_o_envio(locais);
        if self.entregar_ao_assistente(sessao, janela, locais, false) {
            return;
        }
        let windows = self.ferramentas.ambiente().windows;
        for local in locais {
            sessao.paste(&pura::caminho_local_para_o_shell(local, windows));
            sessao.send(b" ");
        }
    }

    /// Envia a imagem ou os arquivos para o servidor da sessão e cola o caminho de lá.
    fn colar_enviar(
        &self,
        sessao: Session,
        conteudo: &RichPasteContent,
        janela: Janela,
        conexao: Option<Connection>,
    ) {
        let Some(conexao) = conexao else {
            return self.colar_local(sessao, conteudo, janela, None);
        };
        let (pasta, dias, tar_min) = self.config.with_untracked(|c| {
            let global = c.terminal.pasta_colar.clone();
            (
                global,
                c.terminal.limpar_colados_dias,
                c.terminal.colar_tar_min_arquivos,
            )
        });
        let pasta = if conexao.pasta_colar.trim().is_empty() {
            pasta
        } else {
            conexao.pasta_colar.clone()
        };
        let envio = Envio {
            sftp: self.opener.arquivos_da_sessao(sessao),
            exec: self.opener.exec_da_sessao(sessao),
            pasta: if pasta.trim().is_empty() {
                pura::PASTA_PADRAO.to_owned()
            } else {
                pasta
            },
            dias,
            tar_min,
            comprimida: conexao.comprimir,
            limpar: !self.colar.limpas.borrow().contains(&sessao),
            sondagem: self.colar.sondagens.borrow().get(&sessao).copied(),
        };
        let colagem = self.nova_colagem(
            sessao,
            janela,
            conexao.name.clone(),
            conexao.shell_remoto,
            true,
        );
        let conteudo = match conteudo {
            RichPasteContent::Files(caminhos) => Conteudo::Arquivos(caminhos.clone()),
            RichPasteContent::Image(imagem) => Conteudo::Imagem(imagem.clone()),
        };
        let limpar_local = (!self.colar.local_limpo.replace(true)).then_some(dias);
        self.iniciar(&colagem, conteudo, Modo::Enviar(envio), limpar_local);
    }

    fn nova_colagem(
        &self,
        sessao: Session,
        janela: Janela,
        conexao: String,
        shell: ShellRemoto,
        visivel: bool,
    ) -> Rc<Colagem> {
        let id = self.colar.proxima.get() + 1;
        self.colar.proxima.set(id);
        let outras = self
            .colar
            .ativas
            .borrow()
            .iter()
            .filter(|c| c.visivel && c.janela == janela.handle)
            .count();
        let colagem = Rc::new(Colagem {
            id,
            sessao,
            janela: janela.handle,
            conexao,
            sufixo: if outras == 0 {
                String::new()
            } else {
                format!("-{}", outras + 1)
            },
            visivel,
            shell,
            cancelar: Arc::new(AtomicBool::new(false)),
            progresso: Live::detached(Progresso::default()),
            interno: RefCell::default(),
        });
        self.colar.ativas.borrow_mut().push(Rc::clone(&colagem));
        self.colar.versao.update(|v| *v += 1);
        colagem
    }

    /// Dispara a thread da colagem.
    fn iniciar(
        &self,
        colagem: &Rc<Colagem>,
        conteudo: Conteudo,
        modo: Modo,
        limpar_local: Option<u32>,
    ) {
        let pedido = Pedido {
            colagem: colagem.id,
            conteudo,
            modo,
            disco: Arc::clone(&self.opener.disco().opener),
            pasta_dados: self.armazenamento.pasta().join("colar"),
            limpar_local,
            quando: (self.ferramentas.ambiente().agora)(),
            cancelar: Arc::clone(&colagem.cancelar),
            eventos: self.colar.eventos.clone(),
            progresso: colagem.progresso.clone(),
        };
        std::thread::spawn(move || trabalhar(pedido));
    }

    /// Abre a pergunta "Enviar ou colar o caminho local?" na janela.
    fn perguntar_colar(&self, colagem: RichPaste, janela: Janela) {
        let conexao = colagem
            .sessions
            .first()
            .and_then(|&s| self.conexao_do_colar(s))
            .map_or_else(String::new, |c| c.name);
        let nomes: Vec<String> = match &colagem.content {
            RichPasteContent::Image(_) => {
                vec![pura::nome_da_imagem((self.ferramentas.ambiente().agora)())]
            }
            RichPasteContent::Files(caminhos) => caminhos
                .iter()
                .map(|c| {
                    c.file_name().map_or_else(
                        || c.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    )
                })
                .collect(),
        };
        let mut itens = nomes.iter().take(3).cloned().collect::<Vec<_>>().join("\n");
        if nomes.len() > 3 {
            itens.push('\n');
            itens.push_str(&t!("colar-dialogo-e-mais", n = (nomes.len() - 3) as i64));
        }
        self.colar
            .dialogo_texto
            .set(t!("colar-dialogo-texto", conexao = conexao.as_str()));
        self.colar.dialogo_itens.set(itens);
        self.colar.lembrar.set(false);
        *self.colar.pendente.borrow_mut() = Some((colagem, janela));
        janela.colar_dialogo.set(true);
    }

    /// A resposta do "Perguntar": `enviar` manda para o servidor, senão cola o caminho local.
    pub(super) fn responder_colar(&self, enviar: bool) {
        let Some((colagem, janela)) = self.colar.pendente.borrow_mut().take() else {
            return;
        };
        if self.colar.lembrar.get_untracked() {
            let modo = if enviar {
                ColarArquivos::Enviar
            } else {
                ColarArquivos::CaminhoLocal
            };
            self.config.update(|c| c.terminal.colar_arquivos = modo);
        }
        for &sessao in &colagem.sessions {
            let conexao = self.conexao_do_colar(sessao);
            if enviar {
                self.colar_enviar(sessao, &colagem.content, janela, conexao);
            } else {
                self.colar_local(sessao, &colagem.content, janela, conexao);
            }
        }
    }

    /// "Cancelar" da barra: para a thread e a fila. Nada é colado.
    fn cancelar_colagem(&self, colagem: &Colagem) {
        colagem.cancelar.store(true, Ordering::Relaxed);
        for id in colagem.interno.borrow().ids.iter().copied() {
            self.transferencias.cancel(id);
        }
    }

    /// Liga a tela ao que acontece com as colagens: põe na fila o que a thread preparou, e cola
    /// quando tudo chegou.
    pub(super) fn acompanhar_colagens(self: &Rc<Self>) {
        let (tela, eventos, itens) = (
            Rc::clone(self),
            self.colar.eventos.clone(),
            self.transferencias.items(),
        );
        effect(move || {
            eventos.with(|_| ());
            let itens = itens.get();
            untrack(|| tela.andar_colagens(&itens));
        });
    }

    fn andar_colagens(&self, itens: &[TransferInfo]) {
        // Esvaziar a lista avisa o próprio efeito: só mexe quando há o que tirar.
        let mut recebidos = Vec::new();
        if self.colar.eventos.with_untracked(|l| !l.is_empty()) {
            self.colar
                .eventos
                .update(|lista| recebidos = std::mem::take(lista));
        }
        for (id, evento) in recebidos {
            let colagem = self
                .colar
                .ativas
                .borrow()
                .iter()
                .find(|c| c.id == id)
                .cloned();
            if let Some(colagem) = colagem {
                self.colar_evento(&colagem, evento);
            }
        }
        let ativas = self.colar.ativas.borrow().clone();
        for colagem in ativas {
            self.conferir_colagem(&colagem, itens);
        }
    }

    fn colar_evento(&self, colagem: &Rc<Colagem>, evento: Evento) {
        match evento {
            Evento::Fim(resultado) => colagem.interno.borrow_mut().fim = Some(resultado),
            Evento::Pronta(preparo) => self.preparo_pronto(colagem, preparo),
        }
    }

    fn preparo_pronto(&self, colagem: &Rc<Colagem>, preparo: Preparo) {
        let Preparo {
            remotos,
            fila,
            destino,
            aviso,
            sondagem,
            limpou,
            temporarias,
            local,
            total,
            nomes,
        } = preparo;
        if let Some(sondagem) = sondagem {
            self.colar
                .sondagens
                .borrow_mut()
                .insert(colagem.sessao, sondagem);
        }
        if limpou {
            self.colar.limpas.borrow_mut().insert(colagem.sessao);
        }
        if let Some((usada, configurada, motivo)) = aviso {
            self.avisar_colar(
                colagem.janela,
                InfoBarSeverity::Warning,
                "colar-aviso",
                t!("colar-aviso-titulo"),
                t!(
                    "colar-aviso-pasta",
                    pasta = usada.as_str(),
                    configurada = configurada.as_str(),
                    motivo = motivo.as_str()
                ),
            );
        }
        let mut ids = Vec::new();
        let mut por_indice: Vec<Option<u64>> = vec![None; remotos.len()];
        if let Some(destino) = destino {
            let disco = self.opener.disco();
            for (indice, origem) in fila {
                let id = self.transferencias.add(
                    (disco.endpoint.clone(), origem),
                    (
                        Endpoint::Remote(Arc::clone(&destino)),
                        remotos[indice].clone(),
                    ),
                    Conflict::Rename,
                );
                por_indice[indice] = Some(id);
                ids.push(id);
            }
        }
        colagem.progresso.update(|p| {
            p.total = total;
            p.nomes = nomes;
            p.ids = ids.clone();
            p.pronta = true;
        });
        let mut interno = colagem.interno.borrow_mut();
        interno.remotos = remotos.into_iter().zip(por_indice).collect();
        interno.local = local;
        interno.temporarias = temporarias;
        interno.ids = ids;
        interno.pronta = true;
    }

    /// Vê em que pé a colagem está: cola, desiste ou espera.
    fn conferir_colagem(&self, colagem: &Rc<Colagem>, itens: &[TransferInfo]) {
        let (falha, cancelada, pronta) = {
            let interno = colagem.interno.borrow();
            let da_fila: Vec<Option<&TransferInfo>> = interno
                .ids
                .iter()
                .map(|id| itens.iter().find(|i| i.id == *id))
                .collect();
            // Uma pasta fica "pronta" na fila assim que enfileira o que tem dentro: a colagem só
            // termina quando o conteúdo também chegou.
            let conteudo = conteudo_das_pastas(&da_fila, itens);
            let todos = || {
                da_fila
                    .iter()
                    .flatten()
                    .copied()
                    .chain(conteudo.iter().copied())
            };
            let falha = match &interno.fim {
                Some(Err(Falha::Erro(motivo))) => Some(motivo.clone()),
                _ => todos().find_map(|i| match &i.state {
                    TransferState::Failed(motivo) => Some(motivo.clone()),
                    _ => None,
                }),
            };
            let cancelada = matches!(interno.fim, Some(Err(Falha::Cancelada)))
                || todos()
                    .any(|i| matches!(i.state, TransferState::Cancelled | TransferState::Skipped));
            let fila_pronta = da_fila.iter().all(|i| i.is_some())
                && todos().all(|i| i.state == TransferState::Done);
            let pronta = interno.pronta && matches!(interno.fim, Some(Ok(()))) && fila_pronta;
            (falha, cancelada, pronta)
        };
        if let Some(motivo) = falha {
            self.encerrar_colagem(colagem);
            let titulo = if colagem.visivel {
                t!("colar-erro-titulo", conexao = colagem.conexao.as_str())
            } else {
                t!("colar-erro-imagem-titulo")
            };
            self.avisar_colar(
                colagem.janela,
                InfoBarSeverity::Error,
                "colar-erro",
                titulo,
                motivo,
            );
        } else if cancelada {
            self.encerrar_colagem(colagem);
        } else if pronta {
            let (remotos, local) = {
                let interno = colagem.interno.borrow();
                let finais = interno
                    .remotos
                    .iter()
                    .map(|(remoto, id)| {
                        // Se a fila renomeou, vale o nome novo.
                        id.and_then(|id| itens.iter().find(|i| i.id == id))
                            .map_or_else(|| remoto.clone(), |i| i.to.clone())
                    })
                    .collect::<Vec<_>>();
                (finais, interno.local)
            };
            self.encerrar_colagem(colagem);
            if colagem.sessao.closed() {
                return;
            }
            if local {
                self.colar_locais(colagem.sessao, colagem.janela, &remotos);
            } else if !self.entregar_ao_assistente(colagem.sessao, colagem.janela, &remotos, true) {
                for texto in pura::texto_para_colar(&remotos, colagem.shell) {
                    colagem.sessao.paste(&texto);
                    colagem.sessao.send(b" ");
                }
            }
        }
    }

    /// Tira a colagem da lista, para a thread, cancela o que sobrou na fila e apaga as cópias
    /// locais da imagem.
    fn encerrar_colagem(&self, colagem: &Rc<Colagem>) {
        colagem.cancelar.store(true, Ordering::Relaxed);
        self.colar
            .ativas
            .borrow_mut()
            .retain(|c| c.id != colagem.id);
        self.colar.versao.update(|v| *v += 1);
        let interno = colagem.interno.borrow();
        for id in &interno.ids {
            self.transferencias.cancel(*id);
        }
        if !interno.temporarias.is_empty() {
            let disco = self.opener.disco();
            for local in &interno.temporarias {
                self.transferencias.delete(disco.endpoint.clone(), local);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// As telas

/// Os itens da fila que estão dentro das pastas de `da_fila`: a fila enfileira cada um à parte
/// e marca a pasta como pronta antes deles. Estão sob o destino que a pasta acabou ficando.
fn conteudo_das_pastas<'a>(
    da_fila: &[Option<&TransferInfo>],
    itens: &'a [TransferInfo],
) -> Vec<&'a TransferInfo> {
    let pastas: Vec<String> = da_fila
        .iter()
        .flatten()
        .filter(|i| i.is_dir)
        .map(|i| format!("{}/", i.to.trim_end_matches('/')))
        .collect();
    if pastas.is_empty() {
        return Vec::new();
    }
    itens
        .iter()
        .filter(|i| pastas.iter().any(|pasta| i.to.starts_with(pasta.as_str())))
        .collect()
}

/// Os itens da fila de uma colagem: os dela e o conteúdo das pastas.
fn itens_da_colagem<'a>(p: &Progresso, itens: &'a [TransferInfo]) -> Vec<&'a TransferInfo> {
    let diretos: Vec<Option<&TransferInfo>> = p
        .ids
        .iter()
        .map(|id| itens.iter().find(|i| i.id == *id))
        .collect();
    let mut todos: Vec<&TransferInfo> = diretos.iter().flatten().copied().collect();
    todos.extend(conteudo_das_pastas(&diretos, itens));
    todos
}

/// O texto da barra: o que vai, quanto já foi, e o tempo que falta quando a fila sabe.
fn texto_do_progresso(p: &Progresso, itens: &[TransferInfo]) -> String {
    if !p.pronta {
        return t!("colar-preparando");
    }
    let da_fila = itens_da_colagem(p, itens);
    let feito = (p.fora + da_fila.iter().map(|i| i.done).sum::<u64>()).min(p.total.max(p.fora));
    let (feito_txt, total_txt) = (tamanho(feito), tamanho(p.total));
    let mut texto = match p.nomes.as_slice() {
        [nome] => t!(
            "colar-um-item",
            nome = nome.as_str(),
            feito = feito_txt,
            total = total_txt
        ),
        nomes => t!(
            "colar-varios-itens",
            n = nomes.len() as i64,
            feito = feito_txt,
            total = total_txt
        ),
    };
    if p.total > GRANDE {
        texto.push(' ');
        texto.push_str(&t!("colar-grande"));
    }
    if let Some(eta) = da_fila.iter().find_map(|i| i.remaining.as_ref()) {
        texto.push_str(" · ");
        texto.push_str(&texto_da_faixa(&faixa(eta)));
    }
    texto
}

fn fracao_do_progresso(p: &Progresso, itens: &[TransferInfo]) -> f64 {
    if p.total == 0 {
        return 0.0;
    }
    let feito: u64 = p.fora
        + itens_da_colagem(p, itens)
            .iter()
            .map(|i| i.done)
            .sum::<u64>();
    (feito as f64 / p.total as f64).clamp(0.0, 1.0)
}

/// A barra de uma colagem: título, texto, "Cancelar" e a barra de progresso.
fn barra_de_progresso(tela: &Rc<Tela>, colagem: Rc<Colagem>) -> impl View + use<> {
    let itens = tela.transferencias.items();
    let sufixo = colagem.sufixo.clone();
    let texto = {
        let (progresso, itens) = (colagem.progresso.clone(), itens.clone());
        move || progresso.with(|p| itens.with(|i| texto_do_progresso(p, i)))
    };
    let enche = {
        let (progresso, itens) = (colagem.progresso.clone(), itens.clone());
        move || progresso.with(|p| itens.with(|i| fracao_do_progresso(p, i)))
    };
    let contando = {
        let progresso = colagem.progresso.clone();
        move || progresso.with(|p| !p.pronta || p.total == 0)
    };
    let contando_2 = contando.clone();
    let cancelar = {
        let (tela, colagem) = (Rc::clone(tela), Rc::clone(&colagem));
        move || tela.cancelar_colagem(&colagem)
    };
    Column::new()
        .gap(4)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    InfoBar::new(InfoBarSeverity::Informational)
                        .id(format!("colar-progresso{sufixo}"))
                        .title(t!(
                            "colar-enviando-para",
                            conexao = colagem.conexao.as_str()
                        ))
                        .message(texto)
                        .grow(1),
                )
                .child(
                    Button::new(t!("comum-cancelar"))
                        .id(format!("colar-cancelar{sufixo}"))
                        .on_click(cancelar),
                ),
        )
        .child(
            ProgressBar::new()
                .id(format!("colar-progresso-barra{sufixo}"))
                .value(enche)
                .visible(move || !contando()),
        )
        .child(
            ProgressBar::new()
                .id(format!("colar-progresso-barra-contando{sufixo}"))
                .visible(contando_2),
        )
}

/// As barras de envio e os avisos do colar, em cima dos terminais de `janela`.
pub(super) fn barras(tela: &Rc<Tela>, janela: Janela) -> impl View + use<> {
    let handle = janela.handle;
    let (versao, avisos) = (tela.colar.versao, tela.colar.avisos);
    let (da_lista, da_barra) = (Rc::clone(tela), Rc::clone(tela));
    Column::new()
        .gap(4)
        .child(For::each(
            move || {
                versao.get();
                da_lista
                    .colar
                    .ativas
                    .borrow()
                    .iter()
                    .filter(|c| c.visivel && c.janela == handle)
                    .cloned()
                    .collect::<Vec<_>>()
            },
            |c: &Rc<Colagem>| c.id,
            move |c: Rc<Colagem>| barra_de_progresso(&da_barra, c),
        ))
        .child(For::each(
            move || {
                avisos
                    .get()
                    .into_iter()
                    .filter(|a| a.janela == handle)
                    .collect::<Vec<_>>()
            },
            |a: &AvisoColar| a.id,
            move |a: AvisoColar| {
                let id = a.id;
                InfoBar::new(a.severidade)
                    .id(a.widget)
                    .title(a.titulo)
                    .message(a.texto)
                    .on_close(move || avisos.update(|l| l.retain(|x| x.id != id)))
            },
        ))
}

/// O "Perguntar": enviar para o servidor ou colar o caminho desta máquina.
pub(super) fn dialogo(tela: &Rc<Tela>, janela: Janela) -> impl View + use<> {
    let (texto, itens, lembrar) = (
        tela.colar.dialogo_texto,
        tela.colar.dialogo_itens,
        tela.colar.lembrar,
    );
    let (enviar, local) = (Rc::clone(tela), Rc::clone(tela));
    ContentDialog::new(t!("colar-dialogo-titulo"), janela.colar_dialogo)
        .id("colar-arquivos-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || texto.get())
                        .id("colar-arquivos-texto")
                        .wrap(),
                )
                .child(
                    Text::new(move || itens.get())
                        .id("colar-arquivos-itens")
                        .secondary()
                        .wrap(),
                )
                .child(
                    CheckBox::bind(lembrar, t!("comum-nao-perguntar-de-novo"))
                        .id("colar-arquivos-lembrar"),
                ),
        )
        .primary(t!("colar-dialogo-enviar"), move || {
            enviar.responder_colar(true)
        })
        .secondary(t!("colar-dialogo-local"), move || {
            local.responder_colar(false)
        })
        .close(t!("comum-cancelar"))
}
