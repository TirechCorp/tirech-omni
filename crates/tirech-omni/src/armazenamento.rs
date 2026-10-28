//! Onde o app guarda as conexões e o estado das abas: dois JSON numa pasta. Quem decide a pasta
//! é só o `main` (por [`pasta`]); o resto recebe a pasta por parâmetro, e os testes usam uma
//! pasta temporária própria.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::modelo::{Tree, VERSAO};
use crate::t;

/// A variável de ambiente que troca a pasta de configuração (o lint e os screenshots rodam o
/// `main` de verdade e não podem ler as conexões de quem os roda).
pub const ENV: &str = "TIRECH_OMNI_CONFIG";

/// O nome que a variável tinha antes do M55 (o app se chamava Tirech SSH). Continua valendo, para
/// scripts e atalhos de quem já usava; se as duas existem, vale [`ENV`].
pub const ENV_ANTIGA: &str = "TIRECH_SSH_CONFIG";

/// O arquivo que, na pasta padrão, aponta para outra pasta de dados (M35): o caminho absoluto,
/// numa linha. Serve para levar os dados para um pendrive ou uma pasta sincronizada.
pub const ARQUIVO_PASTA: &str = "pasta.txt";

/// A pasta que, ao lado do executável, liga o modo portable (M66): o app grava tudo nela e não
/// escreve nada no sistema. O nome é longo de propósito, para não colidir com outra pasta.
pub const PASTA_PORTATIL: &str = "tirech-omnidata";

/// O arquivo que o modo portable trava enquanto o app está aberto (M66).
pub const ARQUIVO_TRAVA: &str = "trava.lock";

/// O arquivo das conexões, na pasta de dados.
pub(crate) const ARQUIVO_CONEXOES: &str = "connections.json";

/// A pasta das cópias automáticas (uma por dia de cada arquivo), dentro da pasta de dados e, se o
/// cofre mora em outra, dentro da do cofre também.
pub const PASTA_COPIAS: &str = "copias";

/// Quantos dias de cópias automáticas ficam de cada arquivo.
pub const DIAS_DE_COPIAS: usize = 7;

/// De onde veio a pasta de dados.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrigemDaPasta {
    /// Da variável `TIRECH_OMNI_CONFIG` (ou da antiga, `TIRECH_SSH_CONFIG`).
    Variavel,
    /// Da pasta [`PASTA_PORTATIL`] ao lado do executável (M66).
    Portatil,
    /// Do `pasta.txt` na pasta padrão.
    PastaTxt,
    /// A pasta padrão do sistema.
    Padrao,
}

/// A pasta de dados: `TIRECH_OMNI_CONFIG` (ou a antiga `TIRECH_SSH_CONFIG`), se definida; senão
/// a pasta portable ao lado do executável, se houver; senão o caminho do `pasta.txt` da pasta
/// padrão, se houver; senão `tirech-omni/` na pasta de configuração do sistema (`~/.config` no
/// Linux).
pub fn pasta() -> PathBuf {
    resolver_pasta(variavel_da_pasta(), portatil(), &pasta_padrao()).0
}

/// A origem da pasta de dados de [`pasta`].
pub fn origem_da_pasta() -> OrigemDaPasta {
    resolver_pasta(variavel_da_pasta(), portatil(), &pasta_padrao()).1
}

/// A pasta portable ao lado do executável, se existir; procurada uma vez só.
pub fn portatil() -> Option<PathBuf> {
    static PORTATIL: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    PORTATIL
        .get_or_init(|| {
            let exe = std::env::current_exe().ok()?;
            portatil_em(exe.parent()?)
        })
        .clone()
}

/// A pasta [`PASTA_PORTATIL`] dentro de `pasta_do_executavel`, se for uma pasta (um arquivo com
/// esse nome não conta).
pub fn portatil_em(pasta_do_executavel: &Path) -> Option<PathBuf> {
    let pasta = pasta_do_executavel.join(PASTA_PORTATIL);
    pasta.is_dir().then_some(pasta)
}

/// Se o app roda no modo portable: a pasta de dados veio de [`portatil`].
pub fn modo_portatil() -> bool {
    variavel_da_pasta().is_none() && portatil().is_some()
}

/// O resultado de [`travar`].
#[derive(Debug)]
pub enum Trava {
    /// A trava é desta cópia do app enquanto o arquivo estiver aberto.
    Travada(fs::File),
    /// Outra cópia do app já trava a pasta.
    Ocupada,
}

/// Trava o [`ARQUIVO_TRAVA`] de `pasta`, sem esperar. A trava some quando o arquivo devolvido
/// fecha, inclusive se o app cair.
pub fn travar(pasta: &Path) -> io::Result<Trava> {
    fs::create_dir_all(pasta)?;
    let arquivo = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(pasta.join(ARQUIVO_TRAVA))?;
    match arquivo.try_lock() {
        Ok(()) => Ok(Trava::Travada(arquivo)),
        Err(fs::TryLockError::WouldBlock) => Ok(Trava::Ocupada),
        Err(fs::TryLockError::Error(erro)) => Err(erro),
    }
}

/// As pastas (e os arquivos) que esta cópia do app só lê. Uma segunda cópia aberta enquanto a
/// primeira trava a pasta de dados registra aqui a pasta de dados, a do cofre e a dos espaços: o
/// que se muda nela fica na memória e se perde ao fechar, como no WinSSHTerm, e nenhum arquivo
/// da configuração é tocado. Um `connections.json` de uma versão mais nova do app entra sozinho.
static SO_LEITURA: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Registra `caminho` (uma pasta, ou um arquivo) como só de leitura até o processo acabar. As
/// gravações da configuração dentro dele não acontecem: as automáticas (o `connections.json`, o
/// `vault.json`, as cópias do dia) são puladas em silêncio, e as que a pessoa pede (trocar a senha
/// mestra, levar o cofre, salvar um espaço) devolvem [`erro_so_leitura`].
pub fn somente_leitura_em(caminho: &Path) {
    let mut lista = SO_LEITURA.lock().unwrap_or_else(PoisonError::into_inner);
    for forma in [Some(caminho.to_owned()), canonico(caminho)]
        .into_iter()
        .flatten()
    {
        if !lista.contains(&forma) {
            lista.push(forma);
        }
    }
}

/// Se `caminho` fica dentro de algo registrado por [`somente_leitura_em`]. Confere o caminho
/// como veio e o canônico (com os links resolvidos), para um `..` ou um link não escaparem.
pub fn so_leitura(caminho: &Path) -> bool {
    let lista = SO_LEITURA.lock().unwrap_or_else(PoisonError::into_inner);
    if lista.is_empty() {
        return false;
    }
    if lista.iter().any(|p| caminho.starts_with(p)) {
        return true;
    }
    canonico(caminho).is_some_and(|c| lista.iter().any(|p| c.starts_with(p)))
}

/// O erro de uma gravação recusada por [`so_leitura`].
pub fn erro_so_leitura() -> io::Error {
    io::Error::new(
        io::ErrorKind::ReadOnlyFilesystem,
        t!("armazenamento-somente-leitura"),
    )
}

/// `Err(`[`erro_so_leitura`]`)` se `caminho` é só de leitura; o começo das gravações que a pessoa
/// pede.
pub fn conferir_gravacao(caminho: &Path) -> io::Result<()> {
    if so_leitura(caminho) {
        Err(erro_so_leitura())
    } else {
        Ok(())
    }
}

/// O caminho canônico de `caminho`, que ainda pode não existir: o do maior antepassado que
/// existe, com o resto do nome no fim.
fn canonico(caminho: &Path) -> Option<PathBuf> {
    let mut resto = Vec::new();
    let mut atual = caminho;
    loop {
        if let Ok(mut real) = fs::canonicalize(atual) {
            real.extend(resto.iter().rev());
            return Some(real);
        }
        resto.push(atual.file_name()?);
        atual = atual.parent()?;
    }
}

/// O valor da variável que troca a pasta: [`ENV`] ou, sem ela, [`ENV_ANTIGA`]. Vazia vale como
/// ausente.
pub fn variavel_da_pasta() -> Option<std::ffi::OsString> {
    escolher_variavel(std::env::var_os(ENV), std::env::var_os(ENV_ANTIGA))
}

/// A regra da escolha entre a variável nova e a antiga, separada para os testes não mexerem no
/// ambiente.
pub fn escolher_variavel(
    nova: Option<std::ffi::OsString>,
    antiga: Option<std::ffi::OsString>,
) -> Option<std::ffi::OsString> {
    nova.filter(|v| !v.is_empty())
        .or_else(|| antiga.filter(|v| !v.is_empty()))
}

/// A pasta padrão do sistema (`~/.config/tirech-omni` no Linux), sem olhar a variável nem o
/// `pasta.txt`.
pub fn pasta_padrao() -> PathBuf {
    pasta_de_configuracao()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tirech-omni")
}

/// A regra da ordem, separada para os testes não mexerem no ambiente.
pub fn resolver_pasta(
    variavel: Option<std::ffi::OsString>,
    portatil: Option<PathBuf>,
    padrao: &Path,
) -> (PathBuf, OrigemDaPasta) {
    if let Some(pasta) = variavel.filter(|v| !v.is_empty()) {
        return (PathBuf::from(pasta), OrigemDaPasta::Variavel);
    }
    if let Some(pasta) = portatil {
        return (pasta, OrigemDaPasta::Portatil);
    }
    match ler_pasta_txt(padrao) {
        Some(pasta) => (pasta, OrigemDaPasta::PastaTxt),
        None => (padrao.to_path_buf(), OrigemDaPasta::Padrao),
    }
}

/// O caminho do `pasta.txt` de `padrao`, se existir com um caminho absoluto.
fn ler_pasta_txt(padrao: &Path) -> Option<PathBuf> {
    let texto = fs::read_to_string(padrao.join(ARQUIVO_PASTA)).ok()?;
    let linha = texto.lines().next()?.trim();
    let caminho = PathBuf::from(linha);
    (!linha.is_empty() && caminho.is_absolute()).then_some(caminho)
}

/// O que [`mudar_pasta`] fez.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MudancaDePasta {
    /// Copiou os arquivos da pasta atual para a nova (que estava vazia) e apontou para ela.
    Copiou,
    /// A pasta nova já tem dados do app (um `connections.json`): só apontou para ela.
    Apontou,
}

/// Troca a pasta de dados para `destino`: se ela está vazia (ou não existe), copia os arquivos
/// de `atual` para lá; se já tem um `connections.json`, só aponta. Grava o `pasta.txt` em
/// `padrao`. A pasta antiga fica como estava. Uma pasta com outras coisas é recusada.
pub fn mudar_pasta(atual: &Path, destino: &Path, padrao: &Path) -> io::Result<MudancaDePasta> {
    if !destino.is_absolute() {
        return Err(io::Error::other(t!("armazenamento-caminho-absoluto")));
    }
    let mudanca = if destino.join("connections.json").is_file() {
        MudancaDePasta::Apontou
    } else {
        let vazia = match fs::read_dir(destino) {
            Ok(mut itens) => itens.next().is_none(),
            Err(erro) if erro.kind() == io::ErrorKind::NotFound => true,
            Err(erro) => return Err(erro),
        };
        if !vazia {
            return Err(io::Error::other(t!("armazenamento-pasta-nao-vazia")));
        }
        copiar_pasta(atual, destino)?;
        MudancaDePasta::Copiou
    };
    fs::create_dir_all(padrao)?;
    fs::write(
        padrao.join(ARQUIVO_PASTA),
        format!("{}\n", destino.display()),
    )?;
    Ok(mudanca)
}

/// Apaga o `pasta.txt` de `padrao`: a pasta de dados volta a ser a padrão.
pub fn voltar_ao_padrao(padrao: &Path) -> io::Result<()> {
    match fs::remove_file(padrao.join(ARQUIVO_PASTA)) {
        Err(erro) if erro.kind() != io::ErrorKind::NotFound => Err(erro),
        _ => Ok(()),
    }
}

/// Copia os arquivos e as pastas de `origem` para `destino` (sem o socket do agente nem os
/// temporários de uma gravação em curso).
fn copiar_pasta(origem: &Path, destino: &Path) -> io::Result<()> {
    fs::create_dir_all(destino)?;
    let Ok(itens) = fs::read_dir(origem) else {
        return Ok(());
    };
    for item in itens {
        let item = item?;
        let nome = item.file_name();
        let texto = nome.to_string_lossy();
        if texto.ends_with(".tmp") || texto.ends_with(".sock") || texto == ARQUIVO_PASTA {
            continue;
        }
        let tipo = item.file_type()?;
        if tipo.is_dir() {
            copiar_pasta(&item.path(), &destino.join(&nome))?;
        } else if tipo.is_file() {
            fs::copy(item.path(), destino.join(&nome))?;
        }
    }
    Ok(())
}

/// A pasta pessoal da pessoa (`HOME`, ou `USERPROFILE` no Windows).
pub fn pasta_pessoal() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .find_map(|v| std::env::var_os(v).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
}

/// Escreve `caminho` com `~/` no lugar da pasta pessoal `home`, quando ele está dentro dela
/// (`/home/ana/.ssh/id` vira `~/.ssh/id`), com `/` em qualquer sistema. Os dados levados a
/// outra máquina, com outro usuário, continuam certos.
pub fn com_til_em(caminho: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home.filter(|h| !h.as_os_str().is_empty())
        && let Ok(resto) = caminho.strip_prefix(home)
    {
        let partes: Vec<_> = resto
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect();
        return if partes.is_empty() {
            "~".to_owned()
        } else {
            format!("~/{}", partes.join("/"))
        };
    }
    caminho.display().to_string()
}

/// O contrário de [`com_til_em`]: `~` e `~/…` passam a ser da pasta pessoal `home`; o resto fica.
pub fn expandir_til_em(texto: &str, home: Option<&Path>) -> PathBuf {
    match (texto, home) {
        ("~", Some(home)) => home.to_owned(),
        (texto, Some(home)) if texto.starts_with("~/") => home.join(&texto[2..]),
        _ => PathBuf::from(texto),
    }
}

/// [`com_til_em`] com a pasta pessoal deste usuário.
pub fn com_til(caminho: &Path) -> String {
    com_til_em(caminho, pasta_pessoal().as_deref())
}

/// [`expandir_til_em`] com a pasta pessoal deste usuário.
pub fn expandir_til(texto: &str) -> PathBuf {
    expandir_til_em(texto, pasta_pessoal().as_deref())
}

/// Para o `#[serde(with = "…")]` de um `PathBuf`: grava com `~/` e lê expandindo.
pub mod caminho_com_til {
    use std::path::{Path, PathBuf};

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(caminho: &Path, saida: S) -> Result<S::Ok, S::Error> {
        saida.serialize_str(&super::com_til(caminho))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(entrada: D) -> Result<PathBuf, D::Error> {
        Ok(super::expandir_til(&String::deserialize(entrada)?))
    }
}

/// Como [`caminho_com_til`], para um `Option<PathBuf>`.
pub mod caminho_opcional_com_til {
    use std::path::PathBuf;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        caminho: &Option<PathBuf>,
        saida: S,
    ) -> Result<S::Ok, S::Error> {
        match caminho {
            Some(caminho) => saida.serialize_some(&super::com_til(caminho)),
            None => saida.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(entrada: D) -> Result<Option<PathBuf>, D::Error> {
        Ok(Option::<String>::deserialize(entrada)?.map(|t| super::expandir_til(&t)))
    }
}

/// A pasta de configuração do sistema: `XDG_CONFIG_HOME` ou `~/.config` no Linux,
/// `~/Library/Application Support` no macOS e `%APPDATA%` no Windows. Escrita à mão: o crate
/// `dirs` puxa uma dependência com licença fora da lista do `deny.toml`.
pub(crate) fn pasta_de_configuracao() -> Option<PathBuf> {
    let variavel = |nome| {
        std::env::var_os(nome)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(windows) {
        variavel("APPDATA")
    } else if cfg!(target_os = "macos") {
        Some(pasta_pessoal()?.join("Library").join("Application Support"))
    } else {
        variavel("XDG_CONFIG_HOME")
            .filter(|p| p.is_absolute())
            .or_else(|| Some(pasta_pessoal()?.join(".config")))
    }
}

/// Os arquivos do app numa pasta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Armazenamento {
    pasta: PathBuf,
}

impl Armazenamento {
    /// Usa `pasta` (ela é criada ao gravar).
    pub fn new(pasta: impl Into<PathBuf>) -> Self {
        Self {
            pasta: pasta.into(),
        }
    }

    /// A pasta dos arquivos.
    pub fn pasta(&self) -> &Path {
        &self.pasta
    }

    /// O `connections.json`.
    pub fn arquivo_conexoes(&self) -> PathBuf {
        self.pasta.join(ARQUIVO_CONEXOES)
    }

    /// O `estado.json`.
    pub fn arquivo_estado(&self) -> PathBuf {
        self.pasta.join("estado.json")
    }

    /// O `recentes.json`.
    pub fn arquivo_recentes(&self) -> PathBuf {
        self.pasta.join("recentes.json")
    }

    /// O `pastas.json`: a última pasta de cada seletor de arquivo.
    pub fn arquivo_pastas(&self) -> PathBuf {
        self.pasta.join("pastas.json")
    }

    /// O `configuracoes.json`.
    pub fn arquivo_configuracoes(&self) -> PathBuf {
        self.pasta.join("configuracoes.json")
    }

    /// O `rclone.json` (o binário e o arquivo de configuração do rclone).
    pub fn arquivo_rclone(&self) -> PathBuf {
        self.pasta.join("rclone.json")
    }

    /// O `agente.json`.
    pub fn arquivo_agente(&self) -> PathBuf {
        self.pasta.join("agente.json")
    }

    /// O `ia.json` (os provedores de IA, M52). A chave de API nunca entra aqui: fica no cofre.
    pub fn arquivo_ia(&self) -> PathBuf {
        self.pasta.join("ia.json")
    }

    /// Lê um arquivo JSON do app. Ausente dá o valor padrão, sem aviso. Inválido vai para
    /// `{arquivo}.bak`, o valor começa no padrão e o aviso (`o que` é o nome para a frase, por
    /// exemplo "de recentes") diz onde ficou a cópia.
    pub(crate) fn carregar_json<T: DeserializeOwned + Default>(
        &self,
        arquivo: &Path,
        o_que: &str,
    ) -> (T, Option<String>) {
        let texto = match fs::read_to_string(arquivo) {
            Ok(texto) => texto,
            Err(erro) if erro.kind() == io::ErrorKind::NotFound => {
                crate::instancias::anotar_base(arquivo, None);
                return (T::default(), None);
            }
            Err(erro) => {
                return (
                    T::default(),
                    Some(t!(
                        "comum-nao-consegui-ler",
                        arquivo = arquivo.display().to_string(),
                        erro = erro.to_string()
                    )),
                );
            }
        };
        crate::instancias::anotar_base(arquivo, Some(texto.as_bytes()));
        match serde_json::from_str::<T>(&texto) {
            Ok(valor) => (valor, None),
            Err(_) => {
                let aviso = match guardar_ilegivel(arquivo) {
                    Ok(copia) => t!(
                        "armazenamento-corrompido-copia",
                        o_que = o_que,
                        copia = copia.display().to_string()
                    ),
                    Err(erro) => t!(
                        "armazenamento-corrompido-sem-copia",
                        o_que = o_que,
                        erro = erro.to_string()
                    ),
                };
                (T::default(), Some(aviso))
            }
        }
    }

    /// Lê as conexões. Arquivo ausente dá uma árvore vazia. JSON inválido vai para
    /// `connections.json.bak` (ou `.bak-<data>`, se aquele já existe), a árvore começa vazia e o
    /// aviso (para a tela mostrar) diz onde ficou a cópia. O arquivo bom ganha a cópia do dia
    /// ([`copia_do_dia`]). Um arquivo de uma versão mais nova do app (`version` maior que
    /// [`VERSAO`]) não é tocado: fica só de leitura ([`somente_leitura_em`]), com o aviso.
    pub fn carregar(&self) -> (Tree, Option<String>) {
        let (tree, aviso, _) = self.ler_visto(false);
        (tree, aviso)
    }

    /// O [`Armazenamento::carregar`], com o que se viu do arquivo, para a vigia
    /// ([`Armazenamento::reler_se_mudou`]) e a gravação ([`Armazenamento::salvar_visto`])
    /// saberem se outro programa o mudou depois. É a árvore da tela: numa cópia secundária, o
    /// lido é a base da junção com o que a principal grava ([`crate::instancias`]).
    pub fn carregar_visto(&self) -> (Tree, Option<String>, Visto) {
        self.ler_visto(true)
    }

    fn ler_visto(&self, da_tela: bool) -> (Tree, Option<String>, Visto) {
        let arquivo = self.arquivo_conexoes();
        let visto = match Visto::ler(&arquivo) {
            Ok(visto) => visto,
            Err(erro) => {
                return (
                    Tree::default(),
                    Some(t!(
                        "comum-nao-consegui-ler",
                        arquivo = arquivo.display().to_string(),
                        erro = erro.to_string()
                    )),
                    Visto::default(),
                );
            }
        };
        if da_tela {
            crate::instancias::anotar_base(&arquivo, visto.bytes.as_deref());
        }
        let Some(bytes) = &visto.bytes else {
            return (Tree::default(), None, visto);
        };
        if let Some((tree, aviso)) = self.entender(bytes) {
            return (tree, aviso, visto);
        }
        match guardar_ilegivel(&arquivo) {
            Ok(copia) => (
                Tree::default(),
                Some(t!(
                    "armazenamento-conexoes-corrompido-copia",
                    copia = copia.display().to_string()
                )),
                Visto::default(),
            ),
            // Sem tirar o arquivo do lugar (só de leitura, ou um erro de disco), o que se viu
            // continua lá: a vigia só relê quando ele mudar de novo.
            Err(erro) => (
                Tree::default(),
                Some(t!(
                    "armazenamento-conexoes-corrompido-sem-copia",
                    erro = erro.to_string()
                )),
                visto,
            ),
        }
    }

    /// A árvore de um `connections.json` lido, com o aviso; `None` se não é JSON, ou se não é uma
    /// árvore desta versão. Um arquivo de uma versão mais nova fica só de leitura, entendido ou
    /// não; um desta versão ganha a cópia do dia.
    fn entender(&self, bytes: &[u8]) -> Option<(Tree, Option<String>)> {
        let valor: Value = serde_json::from_slice(bytes).ok()?;
        let tree = serde_json::from_value::<Tree>(valor.clone()).ok();
        if versao_do_json(&valor) > VERSAO {
            somente_leitura_em(&self.arquivo_conexoes());
            let mut tree = tree.unwrap_or_default();
            tree.next_id = tree.next_id.max(maior_id(&tree) + 1);
            return Some((tree, Some(t!("armazenamento-conexoes-versao-mais-nova"))));
        }
        let mut tree = tree?;
        // Um `next_id` perdido ou velho não pode repetir um id que já existe.
        tree.next_id = tree.next_id.max(maior_id(&tree) + 1);
        let _ = copia_do_dia(&self.pasta, ARQUIVO_CONEXOES, bytes, false);
        Some((tree, None))
    }

    /// Relê as conexões se o arquivo mudou desde `visto` (outra cópia do app, outro programa,
    /// a sincronização), e anota em `visto` o que se viu. Confere primeiro o tamanho e a data;
    /// só lê o conteúdo se um deles mudou, e só devolve a árvore se o conteúdo mudou de fato e se
    /// lê. Um arquivo que sumiu ou que não se lê (um editor no meio da gravação) não muda nada
    /// aqui e não é tirado do lugar.
    pub fn reler_se_mudou(&self, visto: &mut Visto) -> Option<(Tree, Option<String>)> {
        let arquivo = self.arquivo_conexoes();
        // A mudança desta cópia ainda vai para a principal: o disco ainda não a tem.
        if crate::instancias::pendente(&arquivo) {
            return None;
        }
        let meta = fs::metadata(&arquivo).ok()?;
        if visto.mesma_data(&meta) {
            return None;
        }
        let novo = Visto::ler(&arquivo).ok()?;
        let bytes = novo.bytes.clone()?;
        if novo.bytes == visto.bytes {
            *visto = novo;
            return None;
        }
        *visto = novo;
        crate::instancias::anotar_base(&arquivo, Some(&bytes));
        self.entender(&bytes)
    }

    /// Grava as conexões (atômico: escreve num `.tmp` e renomeia). A primeira gravação do dia
    /// guarda antes o arquivo de agora como a cópia do dia, se ela ainda não existe. Não confere
    /// se outro programa mudou o arquivo: a tela usa [`Armazenamento::salvar_visto`].
    pub fn salvar(&self, tree: &Tree) -> io::Result<()> {
        self.salvar_com(tree, None).map(|_| ())
    }

    /// O [`Armazenamento::salvar`] que confere antes se o arquivo ainda é o de `visto`. Se outro
    /// programa o mudou e a tela ainda não releu, a versão de fora não se perde: fica ao lado,
    /// como `connections.json.conflito-<data>`, e volta o caminho dela para o aviso. Depois de
    /// gravar, `visto` passa a ser o que se gravou.
    pub fn salvar_visto(&self, tree: &Tree, visto: &mut Visto) -> io::Result<Option<PathBuf>> {
        self.salvar_com(tree, Some(visto))
    }

    fn salvar_com(&self, tree: &Tree, visto: Option<&mut Visto>) -> io::Result<Option<PathBuf>> {
        let arquivo = self.arquivo_conexoes();
        // Só de leitura (outra cópia do app é a dona, ou o arquivo é de uma versão mais nova):
        // a árvore fica na memória.
        if so_leitura(&arquivo) {
            mandar_a_principal(&arquivo, tree);
            return Ok(None);
        }
        fs::create_dir_all(&self.pasta)?;
        let atual = Visto::ler(&arquivo)?;
        let mut disco = None;
        let mut conflito = None;
        if let Some(antes) = &atual.bytes {
            match serde_json::from_slice::<Value>(antes) {
                Ok(valor) => {
                    // Gravado por uma versão mais nova depois de lido: daqui em diante, só leitura.
                    if versao_do_json(&valor) > VERSAO {
                        somente_leitura_em(&arquivo);
                        return Err(io::Error::other(t!(
                            "armazenamento-conexoes-versao-mais-nova"
                        )));
                    }
                    if let Some(visto) = &visto
                        && visto.bytes != atual.bytes
                    {
                        let copia = nome_livre(&arquivo, &format!(".conflito-{}", carimbo()));
                        gravar_atomico(&copia, antes, false)?;
                        conflito = Some(copia);
                    }
                    if !copia_do_dia_existe(&self.pasta, ARQUIVO_CONEXOES) {
                        let _ = copia_do_dia(&self.pasta, ARQUIVO_CONEXOES, antes, false);
                    }
                    disco = Some(valor);
                }
                // Não é JSON (um editor que gravou pela metade): vai para um `.bak`, e não some
                // debaixo da gravação.
                Err(_) => {
                    guardar_ilegivel(&arquivo)?;
                }
            }
        }
        let texto = texto_com_enxerto(tree, disco.as_ref())?;
        gravar_atomico(&arquivo, texto.as_bytes(), false)?;
        if let Some(visto) = visto {
            *visto = Visto::gravado(&arquivo, texto.into_bytes());
        }
        Ok(conflito)
    }

    /// Grava um valor como JSON bonito, de forma atômica ([`gravar_atomico`]). O que o arquivo
    /// tem e este app não conhece (os campos de uma versão mais nova) continua lá
    /// ([`enxertar`]). Num arquivo só de leitura ([`so_leitura`]), não grava nada e não é erro:
    /// o valor fica na memória, e vai para a cópia principal se esta é uma secundária
    /// ([`crate::instancias`]).
    pub(crate) fn gravar<T: Serialize + DeserializeOwned>(
        &self,
        arquivo: &Path,
        valor: &T,
    ) -> io::Result<()> {
        if so_leitura(arquivo) {
            mandar_a_principal(arquivo, valor);
            return Ok(());
        }
        fs::create_dir_all(&self.pasta)?;
        let disco = fs::read(arquivo)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let texto = texto_com_enxerto(valor, disco.as_ref())?;
        gravar_atomico(arquivo, texto.as_bytes(), false)
    }

    /// Lê um JSON; `None` se o arquivo não existe ou não serve.
    pub(crate) fn ler<T: DeserializeOwned>(&self, arquivo: &Path) -> Option<T> {
        serde_json::from_str(&fs::read_to_string(arquivo).ok()?).ok()
    }
}

/// Um nome de `.tmp` só desta gravação, ao lado de `arquivo`. Duas cópias do app (ou duas
/// threads) que gravam o mesmo arquivo ao mesmo tempo não escrevem no mesmo `.tmp`, e uma não
/// renomeia para o lugar do arquivo o `.tmp` que a outra ainda está escrevendo.
fn tmp_unico(arquivo: &Path) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let mut nome = arquivo.as_os_str().to_owned();
    nome.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    PathBuf::from(nome)
}

/// Grava `bytes` em `arquivo` de forma atômica e durável: um `.tmp` só desta gravação
/// ([`tmp_unico`]), `fsync` e `rename`. Quem lê o arquivo, inclusive depois de o computador
/// desligar no meio, vê o conteúdo velho ou o novo, nunca a metade. Com `privado`, no Unix o
/// arquivo nasce só do dono (`0o600`).
pub(crate) fn gravar_atomico(arquivo: &Path, bytes: &[u8], privado: bool) -> io::Result<()> {
    conferir_gravacao(arquivo)?;
    let tmp = tmp_unico(arquivo);
    let mut opcoes = fs::OpenOptions::new();
    opcoes.write(true).create_new(true);
    #[cfg(unix)]
    if privado {
        use std::os::unix::fs::OpenOptionsExt;
        opcoes.mode(0o600);
    }
    #[cfg(not(unix))]
    let _ = privado;
    let resultado = opcoes.open(&tmp).and_then(|mut f| {
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, arquivo)
    });
    if resultado.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    resultado
}

/// Os segundos de agora como `aaaammdd-hhmmss` (UTC), para o nome das cópias.
pub(crate) fn carimbo() -> String {
    let t = crate::recentes::agora();
    let (ano, mes, dia) = crate::recentes::data_civil(t / 86_400);
    let s = t % 86_400;
    format!(
        "{ano:04}{mes:02}{dia:02}-{:02}{:02}{:02}",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// `arquivo` com `sufixo` no fim do nome, e `-2`, `-3`… depois dele se o nome já existe.
fn nome_livre(arquivo: &Path, sufixo: &str) -> PathBuf {
    let mut nome = com_sufixo(arquivo, sufixo);
    let mut n = 2;
    while nome.exists() {
        nome = com_sufixo(arquivo, &format!("{sufixo}-{n}"));
        n += 1;
    }
    nome
}

/// `arquivo` com `sufixo` no fim do nome.
fn com_sufixo(arquivo: &Path, sufixo: &str) -> PathBuf {
    let mut nome = arquivo.as_os_str().to_owned();
    nome.push(sufixo);
    PathBuf::from(nome)
}

/// O final do nome das cópias de `arquivos` (que andam juntos, como o `vault.json` e o
/// `vault.key`): `.bak` se nenhum `.bak` deles existe e, se algum existe, `.bak-<data>` (com
/// `-2`, `-3`… se preciso). Nunca um nome que já existe: a cópia de uma vez não apaga a de outra.
pub(crate) fn sufixo_da_copia(arquivos: &[PathBuf]) -> String {
    let livre = |sufixo: &str| arquivos.iter().all(|a| !com_sufixo(a, sufixo).exists());
    if livre(".bak") {
        return ".bak".into();
    }
    let carimbo = carimbo();
    let mut sufixo = format!(".bak-{carimbo}");
    let mut n = 2;
    while !livre(&sufixo) {
        sufixo = format!(".bak-{carimbo}-{n}");
        n += 1;
    }
    sufixo
}

/// Tira do caminho o `arquivo` que não se lê, para um nome de cópia livre
/// ([`sufixo_da_copia`]), e devolve onde ficou.
pub(crate) fn guardar_ilegivel(arquivo: &Path) -> io::Result<PathBuf> {
    conferir_gravacao(arquivo)?;
    let copia = com_sufixo(arquivo, &sufixo_da_copia(&[arquivo.to_owned()]));
    fs::rename(arquivo, &copia)?;
    Ok(copia)
}

/// O caminho da cópia de hoje de `nome` em `pasta`: `copias/<aaaammdd>-<nome>`.
fn copia_de_hoje(pasta: &Path, nome: &str) -> PathBuf {
    pasta
        .join(PASTA_COPIAS)
        .join(format!("{}-{nome}", &carimbo()[..8]))
}

/// Se `nome` já tem a cópia de hoje em `pasta`.
pub(crate) fn copia_do_dia_existe(pasta: &Path, nome: &str) -> bool {
    copia_de_hoje(pasta, nome).exists()
}

/// Guarda `bytes` (um conteúdo bom de `nome`) como a cópia de hoje, em
/// `pasta/copias/<aaaammdd>-<nome>`, se ela ainda não existe, e apaga as de `nome` além dos
/// [`DIAS_DE_COPIAS`] dias mais recentes. Um arquivo que se perde ou estraga volta com, no
/// máximo, um dia de mudanças a menos. A pasta `copias` é só do dono no Unix. Numa pasta só de
/// leitura ([`so_leitura`]), não faz nada: as cópias são da cópia do app que é a dona.
pub(crate) fn copia_do_dia(
    pasta: &Path,
    nome: &str,
    bytes: &[u8],
    privado: bool,
) -> io::Result<()> {
    if so_leitura(&pasta.join(PASTA_COPIAS)) {
        return Ok(());
    }
    let hoje = copia_de_hoje(pasta, nome);
    if hoje.exists() {
        return Ok(());
    }
    let copias = pasta.join(PASTA_COPIAS);
    fs::create_dir_all(&copias)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&copias, fs::Permissions::from_mode(0o700))?;
    }
    gravar_atomico(&hoje, bytes, privado)?;
    let sufixo = format!("-{nome}");
    let mut dias: Vec<PathBuf> = fs::read_dir(&copias)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.len() == 8 + sufixo.len()
                    && n.ends_with(&sufixo)
                    && n.as_bytes()[..8].iter().all(u8::is_ascii_digit)
            })
        })
        .collect();
    dias.sort();
    let sobra = dias.len().saturating_sub(DIAS_DE_COPIAS);
    for velha in &dias[..sobra] {
        let _ = fs::remove_file(velha);
    }
    Ok(())
}

/// O maior id de qualquer item (grupos incluídos).
fn maior_id(tree: &Tree) -> u64 {
    fn visita(nodes: &[crate::modelo::Node]) -> u64 {
        nodes
            .iter()
            .map(|node| match node {
                crate::modelo::Node::Group(g) => g.id.max(visita(&g.children)),
                crate::modelo::Node::Connection(c) => c.id,
            })
            .max()
            .unwrap_or(0)
    }
    visita(&tree.nodes)
}

/// O que se viu de um arquivo: o tamanho, a data e o conteúdo. A vigia compara o tamanho e a data
/// para não ler o arquivo à toa; a gravação compara o conteúdo, para não passar por cima do que
/// outro programa gravou depois.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Visto {
    tamanho: u64,
    modificado: Option<SystemTime>,
    /// O conteúdo; `None` se o arquivo não existia.
    bytes: Option<Vec<u8>>,
}

impl Visto {
    /// Lê `arquivo`: a data antes do conteúdo, para uma gravação no meio deixar a data velha e a
    /// próxima olhada reler. Um arquivo que não existe dá o `Visto` vazio.
    pub fn ler(arquivo: &Path) -> io::Result<Self> {
        let ausente = |erro: &io::Error| erro.kind() == io::ErrorKind::NotFound;
        let meta = match fs::metadata(arquivo) {
            Ok(meta) => meta,
            Err(erro) if ausente(&erro) => return Ok(Self::default()),
            Err(erro) => return Err(erro),
        };
        let bytes = match fs::read(arquivo) {
            Ok(bytes) => bytes,
            Err(erro) if ausente(&erro) => return Ok(Self::default()),
            Err(erro) => return Err(erro),
        };
        Ok(Self {
            tamanho: meta.len(),
            modificado: meta.modified().ok(),
            bytes: Some(bytes),
        })
    }

    /// O que se acabou de gravar em `arquivo`. Se o tamanho no disco já não é o de `bytes`
    /// (outro programa gravou logo depois), a data fica de fora, e a próxima olhada relê.
    fn gravado(arquivo: &Path, bytes: Vec<u8>) -> Self {
        let meta = fs::metadata(arquivo).ok();
        let modificado = meta
            .as_ref()
            .filter(|m| m.len() == bytes.len() as u64)
            .and_then(|m| m.modified().ok());
        Self {
            tamanho: bytes.len() as u64,
            modificado,
            bytes: Some(bytes),
        }
    }

    /// Se `meta` tem o mesmo tamanho e a mesma data do que se viu (sem precisar ler de novo).
    fn mesma_data(&self, meta: &fs::Metadata) -> bool {
        self.bytes.is_some()
            && self.tamanho == meta.len()
            && self.modificado.is_some()
            && self.modificado == meta.modified().ok()
    }
}

/// A versão de um `connections.json` lido (`version`); sem ela, a 1.
pub(crate) fn versao_do_json(valor: &Value) -> u32 {
    valor
        .get("version")
        .and_then(Value::as_u64)
        .map_or(1, |v| u32::try_from(v).unwrap_or(u32::MAX))
}

/// Numa cópia secundária ([`crate::instancias`]), manda `valor` para a principal gravar em
/// `arquivo`, com o que o arquivo tem e este app não conhece ([`valor_com_enxerto`]). Um arquivo
/// de uma versão mais nova não se toca.
fn mandar_a_principal<T: Serialize + DeserializeOwned>(arquivo: &Path, valor: &T) {
    if !crate::instancias::segue_a_principal(arquivo) {
        return;
    }
    let disco = fs::read(arquivo)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    if disco.as_ref().is_some_and(|d| versao_do_json(d) > VERSAO) {
        return;
    }
    if let Ok(nosso) = valor_com_enxerto(valor, disco.as_ref()) {
        crate::instancias::enviar_json(arquivo, nosso);
    }
}

/// O JSON de `valor` com o que `disco` tem e este app não conhece ([`enxertar`]).
pub(crate) fn valor_com_enxerto<T: Serialize + DeserializeOwned>(
    valor: &T,
    disco: Option<&Value>,
) -> io::Result<Value> {
    let mut nosso = serde_json::to_value(valor)?;
    if let Some(disco) = disco {
        let entendido = serde_json::from_value::<T>(disco.clone())
            .ok()
            .and_then(|t| serde_json::to_value(t).ok());
        enxertar(&mut nosso, disco, entendido.as_ref());
    }
    Ok(nosso)
}

/// O JSON bonito de `valor` com o que `disco` (o arquivo de agora) tem e este app não conhece:
/// os campos de uma versão mais nova continuam lá depois que esta grava ([`enxertar`]). Sem nada
/// a enxertar, é o JSON de `valor` como sempre, na ordem dos campos do struct.
fn texto_com_enxerto<T: Serialize + DeserializeOwned>(
    valor: &T,
    disco: Option<&Value>,
) -> io::Result<String> {
    if let Some(disco) = disco {
        let mut nosso = serde_json::to_value(valor)?;
        // O que este app entende do disco: o que ficou de fora daqui foi apagado de propósito
        // (um campo zerado, um item tirado de um mapa), e não volta.
        let entendido = serde_json::from_value::<T>(disco.clone())
            .ok()
            .and_then(|t| serde_json::to_value(t).ok());
        if enxertar(&mut nosso, disco, entendido.as_ref()) {
            return Ok(serde_json::to_string_pretty(&nosso)?);
        }
    }
    Ok(serde_json::to_string_pretty(valor)?)
}

/// Copia para `nosso` as chaves de `disco` que este app não conhece: as que faltam em `nosso` e
/// em `entendido` (o `disco` lido e escrito de novo por este app; `None` se não se lê, e então
/// tudo o que falta volta). Desce nos objetos e nas listas: um item de lista acha o seu no disco
/// pelo `id` (ou `Variante:id`, no enum marcado por fora, como os `Node`) e, sem id, pela
/// igualdade com o entendido. Devolve se copiou algo.
fn enxertar(nosso: &mut Value, disco: &Value, entendido: Option<&Value>) -> bool {
    match (nosso, disco) {
        (Value::Object(nosso), Value::Object(disco)) => {
            let entendido = entendido.and_then(Value::as_object);
            let mut copiou = false;
            for (chave, do_disco) in disco {
                let entendido_aqui = entendido.and_then(|e| e.get(chave));
                match nosso.get_mut(chave) {
                    Some(valor) => copiou |= enxertar(valor, do_disco, entendido_aqui),
                    None if entendido.is_none_or(|e| !e.contains_key(chave)) => {
                        nosso.insert(chave.clone(), do_disco.clone());
                        copiou = true;
                    }
                    None => {}
                }
            }
            copiou
        }
        (Value::Array(nosso), Value::Array(disco)) => {
            let entendido = entendido.and_then(Value::as_array);
            let mut copiou = false;
            for item in nosso {
                let par = match identidade(item) {
                    Some(id) => disco
                        .iter()
                        .position(|d| identidade(d).as_ref() == Some(&id)),
                    None => entendido.and_then(|e| e.iter().position(|d| d == &*item)),
                };
                if let Some(j) = par {
                    copiou |= enxertar(item, &disco[j], entendido.and_then(|e| e.get(j)));
                }
            }
            copiou
        }
        _ => false,
    }
}

/// A identidade de um item de lista para o [`enxertar`]: o `id` dele ou, num enum marcado por
/// fora (`{"Group": {"id": 3, ..}}`), a variante com o `id` de dentro.
pub(crate) fn identidade(valor: &Value) -> Option<String> {
    let objeto = valor.as_object()?;
    if let Some(id) = objeto.get("id").filter(|id| !id.is_null()) {
        return Some(id.to_string());
    }
    let mut campos = objeto.iter();
    let (variante, dentro) = campos.next()?;
    if campos.next().is_some() {
        return None;
    }
    let id = dentro.as_object()?.get("id").filter(|id| !id.is_null())?;
    Some(format!("{variante}:{id}"))
}

#[cfg(test)]
pub(crate) mod testes {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// Uma pasta temporária só deste teste, apagada ao sair de escopo.
    pub struct PastaTemporaria(pub PathBuf);

    impl PastaTemporaria {
        pub fn nova(nome: &str) -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let pasta = std::env::temp_dir().join(format!(
                "tirech-omni-{}-{}-{nome}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&pasta);
            Self(pasta)
        }
    }

    impl Drop for PastaTemporaria {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testes::PastaTemporaria;
    use super::*;
    use crate::modelo::{Connection, Kind};

    fn exemplo() -> Tree {
        let mut tree = Tree::default();
        let grupo = tree.add_group(None, "Produção");
        tree.add_connection(Some(grupo), Connection::new("web-01", Kind::Ssh));
        tree
    }

    #[test]
    fn o_til_vai_e_volta_com_a_pasta_pessoal() {
        let home = Path::new("/home/ana");
        assert_eq!(
            com_til_em(Path::new("/home/ana/.ssh/id_ed25519"), Some(home)),
            "~/.ssh/id_ed25519"
        );
        assert_eq!(com_til_em(Path::new("/home/ana"), Some(home)), "~");
        // Fora da pasta pessoal, ou sem ela, o caminho fica como está.
        assert_eq!(
            com_til_em(Path::new("/etc/chave"), Some(home)),
            "/etc/chave"
        );
        assert_eq!(com_til_em(Path::new("/home/ana/x"), None), "/home/ana/x");
        // Uma pasta de nome parecido não conta.
        assert_eq!(
            com_til_em(Path::new("/home/anabela/x"), Some(home)),
            "/home/anabela/x"
        );
        let outro = Path::new("/Users/bia");
        assert_eq!(
            expandir_til_em("~/.ssh/id_ed25519", Some(outro)),
            Path::new("/Users/bia/.ssh/id_ed25519")
        );
        assert_eq!(expandir_til_em("~", Some(outro)), outro);
        assert_eq!(expandir_til_em("/abs/x", Some(outro)), Path::new("/abs/x"));
        assert_eq!(expandir_til_em("~/x", None), Path::new("~/x"));
    }

    #[test]
    fn sem_arquivo_a_arvore_comeca_vazia_e_sem_aviso() {
        let pasta = PastaTemporaria::nova("vazio");
        let (tree, aviso) = Armazenamento::new(&pasta.0).carregar();
        assert!(tree.nodes.is_empty());
        assert_eq!(aviso, None);
    }

    #[test]
    fn salvar_e_carregar_devolvem_a_mesma_arvore() {
        let pasta = PastaTemporaria::nova("ida-e-volta");
        let armazenamento = Armazenamento::new(&pasta.0);
        let tree = exemplo();
        armazenamento.salvar(&tree).unwrap();
        let (volta, aviso) = armazenamento.carregar();
        assert_eq!(volta, tree);
        assert_eq!(aviso, None);
    }

    #[test]
    fn gravacao_e_atomica_e_nao_deixa_o_tmp() {
        let pasta = PastaTemporaria::nova("atomico");
        let armazenamento = Armazenamento::new(&pasta.0);
        armazenamento.salvar(&exemplo()).unwrap();
        armazenamento.salvar(&Tree::default()).unwrap();
        let arquivos: Vec<String> = fs::read_dir(&pasta.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let mut arquivos = arquivos;
        arquivos.sort();
        assert_eq!(arquivos, ["connections.json", PASTA_COPIAS]);
    }

    #[test]
    fn arquivo_corrompido_vai_para_o_bak_com_aviso() {
        let pasta = PastaTemporaria::nova("corrompido");
        fs::create_dir_all(&pasta.0).unwrap();
        let armazenamento = Armazenamento::new(&pasta.0);
        fs::write(armazenamento.arquivo_conexoes(), "{ isto não é json").unwrap();
        let (tree, aviso) = armazenamento.carregar();
        assert!(tree.nodes.is_empty());
        let aviso = aviso.expect("deve avisar");
        assert!(aviso.contains("corrompido"), "{aviso}");
        assert!(aviso.contains("connections.json.bak"), "{aviso}");
        assert_eq!(
            fs::read_to_string(pasta.0.join("connections.json.bak")).unwrap(),
            "{ isto não é json"
        );
        assert!(!armazenamento.arquivo_conexoes().exists());
    }

    #[test]
    fn next_id_velho_nao_repete_id_existente() {
        let pasta = PastaTemporaria::nova("next-id");
        let armazenamento = Armazenamento::new(&pasta.0);
        let mut tree = exemplo();
        tree.next_id = 1;
        armazenamento.salvar(&tree).unwrap();
        let (mut volta, _) = armazenamento.carregar();
        let novo = volta.add_group(None, "Novo");
        assert!(novo > 2, "o id {novo} repete um existente");
    }

    #[test]
    fn a_senha_nunca_vai_para_o_json() {
        let pasta = PastaTemporaria::nova("sem-senha");
        let armazenamento = Armazenamento::new(&pasta.0);
        armazenamento.salvar(&exemplo()).unwrap();
        let texto = fs::read_to_string(armazenamento.arquivo_conexoes()).unwrap();
        assert!(!texto.to_lowercase().contains("pass"), "{texto}");
        assert!(!texto.to_lowercase().contains("senha"), "{texto}");
    }

    #[test]
    fn a_variavel_antiga_ainda_vale() {
        assert_eq!(ENV_ANTIGA, "TIRECH_SSH_CONFIG");
        let escolhida = escolher_variavel(None, Some("/da/antiga".into()));
        assert_eq!(escolhida, Some("/da/antiga".into()));
        let padrao = PastaTemporaria::nova("variavel-antiga");
        assert_eq!(
            resolver_pasta(escolhida, None, &padrao.0),
            (PathBuf::from("/da/antiga"), OrigemDaPasta::Variavel)
        );
        // Vazia, como ausente.
        assert_eq!(escolher_variavel(Some("".into()), None), None);
        assert_eq!(
            escolher_variavel(Some("".into()), Some("/da/antiga".into())),
            Some("/da/antiga".into())
        );
    }

    #[test]
    fn a_nova_vence_a_antiga() {
        assert_eq!(ENV, "TIRECH_OMNI_CONFIG");
        assert_eq!(
            escolher_variavel(Some("/da/nova".into()), Some("/da/antiga".into())),
            Some("/da/nova".into())
        );
    }

    #[test]
    fn a_pasta_vem_da_variavel_depois_do_pasta_txt_depois_do_padrao() {
        let padrao = PastaTemporaria::nova("pasta-padrao");
        let outra = PastaTemporaria::nova("pasta-outra");
        fs::create_dir_all(&padrao.0).unwrap();
        let sem_txt = resolver_pasta(None, None, &padrao.0);
        assert_eq!(sem_txt, (padrao.0.clone(), OrigemDaPasta::Padrao));
        fs::write(
            padrao.0.join(ARQUIVO_PASTA),
            format!("{}\n", outra.0.display()),
        )
        .unwrap();
        assert_eq!(
            resolver_pasta(None, None, &padrao.0),
            (outra.0.clone(), OrigemDaPasta::PastaTxt)
        );
        assert_eq!(
            resolver_pasta(Some("/da/variavel".into()), None, &padrao.0),
            (PathBuf::from("/da/variavel"), OrigemDaPasta::Variavel)
        );
        // Variável vazia não conta.
        assert_eq!(
            resolver_pasta(Some("".into()), None, &padrao.0).1,
            OrigemDaPasta::PastaTxt
        );
    }

    #[test]
    fn pasta_txt_vazio_ou_relativo_e_ignorado() {
        let padrao = PastaTemporaria::nova("pasta-txt-ruim");
        fs::create_dir_all(&padrao.0).unwrap();
        for texto in ["", "\n", "relativo/pasta", "   "] {
            fs::write(padrao.0.join(ARQUIVO_PASTA), texto).unwrap();
            assert_eq!(
                resolver_pasta(None, None, &padrao.0).1,
                OrigemDaPasta::Padrao,
                "{texto:?}"
            );
        }
    }

    #[test]
    fn a_pasta_portable_vem_depois_da_variavel_e_antes_do_pasta_txt() {
        let padrao = PastaTemporaria::nova("portatil-padrao");
        let outra = PastaTemporaria::nova("portatil-outra");
        let exe = PastaTemporaria::nova("portatil-exe");
        fs::create_dir_all(&padrao.0).unwrap();
        fs::write(
            padrao.0.join(ARQUIVO_PASTA),
            format!("{}\n", outra.0.display()),
        )
        .unwrap();
        // Sem a pasta ao lado do executável, nada muda.
        fs::create_dir_all(&exe.0).unwrap();
        assert_eq!(portatil_em(&exe.0), None);
        // Um arquivo com o nome não liga o modo.
        fs::write(exe.0.join(PASTA_PORTATIL), "").unwrap();
        assert_eq!(portatil_em(&exe.0), None);
        fs::remove_file(exe.0.join(PASTA_PORTATIL)).unwrap();
        fs::create_dir(exe.0.join(PASTA_PORTATIL)).unwrap();
        let dados = portatil_em(&exe.0);
        assert_eq!(dados, Some(exe.0.join("tirech-omnidata")));
        assert_eq!(
            resolver_pasta(None, dados.clone(), &padrao.0),
            (exe.0.join(PASTA_PORTATIL), OrigemDaPasta::Portatil)
        );
        assert_eq!(
            resolver_pasta(Some("/da/variavel".into()), dados, &padrao.0),
            (PathBuf::from("/da/variavel"), OrigemDaPasta::Variavel)
        );
    }

    #[test]
    fn a_trava_recusa_a_segunda_copia_e_solta_quando_a_primeira_fecha() {
        let pasta = PastaTemporaria::nova("trava");
        let primeira = travar(&pasta.0).unwrap();
        assert!(matches!(primeira, Trava::Travada(_)));
        assert!(pasta.0.join(ARQUIVO_TRAVA).is_file());
        assert!(matches!(travar(&pasta.0).unwrap(), Trava::Ocupada));
        drop(primeira);
        assert!(matches!(travar(&pasta.0).unwrap(), Trava::Travada(_)));
    }

    #[test]
    fn mudar_para_uma_pasta_vazia_copia_os_dados_e_aponta() {
        let atual = PastaTemporaria::nova("mudar-atual");
        let destino = PastaTemporaria::nova("mudar-destino");
        let padrao = PastaTemporaria::nova("mudar-padrao");
        exemplo_em(&atual.0);
        fs::create_dir_all(atual.0.join("espacos")).unwrap();
        fs::write(atual.0.join("espacos/um.json"), "{}").unwrap();
        fs::write(atual.0.join("agent.sock"), "").unwrap();
        let mudanca = mudar_pasta(&atual.0, &destino.0, &padrao.0).unwrap();
        assert_eq!(mudanca, MudancaDePasta::Copiou);
        assert!(destino.0.join("connections.json").is_file());
        assert!(destino.0.join("espacos/um.json").is_file());
        assert!(!destino.0.join("agent.sock").exists());
        assert!(atual.0.join("connections.json").is_file(), "a antiga fica");
        assert_eq!(resolver_pasta(None, None, &padrao.0).0, destino.0);
        voltar_ao_padrao(&padrao.0).unwrap();
        assert_eq!(
            resolver_pasta(None, None, &padrao.0).1,
            OrigemDaPasta::Padrao
        );
        voltar_ao_padrao(&padrao.0).unwrap();
    }

    #[test]
    fn mudar_para_uma_pasta_que_ja_tem_dados_so_aponta() {
        let atual = PastaTemporaria::nova("aponta-atual");
        let destino = PastaTemporaria::nova("aponta-destino");
        let padrao = PastaTemporaria::nova("aponta-padrao");
        exemplo_em(&atual.0);
        exemplo_em(&destino.0);
        fs::write(destino.0.join("marca.txt"), "dela").unwrap();
        let mudanca = mudar_pasta(&atual.0, &destino.0, &padrao.0).unwrap();
        assert_eq!(mudanca, MudancaDePasta::Apontou);
        assert_eq!(
            fs::read_to_string(destino.0.join("marca.txt")).unwrap(),
            "dela"
        );
    }

    #[test]
    fn mudar_para_uma_pasta_com_outras_coisas_e_recusado() {
        let atual = PastaTemporaria::nova("recusa-atual");
        let destino = PastaTemporaria::nova("recusa-destino");
        let padrao = PastaTemporaria::nova("recusa-padrao");
        exemplo_em(&atual.0);
        fs::create_dir_all(&destino.0).unwrap();
        fs::write(destino.0.join("foto.jpg"), "x").unwrap();
        assert!(mudar_pasta(&atual.0, &destino.0, &padrao.0).is_err());
        assert!(!padrao.0.join(ARQUIVO_PASTA).exists(), "nada apontado");
        assert!(mudar_pasta(&atual.0, Path::new("relativo"), &padrao.0).is_err());
    }

    fn exemplo_em(pasta: &Path) {
        Armazenamento::new(pasta).salvar(&exemplo()).unwrap();
    }

    /// Os nomes de `pasta`, em ordem.
    fn nomes(pasta: &Path) -> Vec<String> {
        let mut nomes: Vec<String> = fs::read_dir(pasta)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        nomes.sort();
        nomes
    }

    #[test]
    fn cada_gravacao_tem_o_seu_tmp_e_nao_deixa_sobra() {
        let pasta = PastaTemporaria::nova("tmp-unico");
        fs::create_dir_all(&pasta.0).unwrap();
        let arquivo = pasta.0.join("a.json");
        assert_ne!(tmp_unico(&arquivo), tmp_unico(&arquivo));
        // O `.tmp` largado por outra cópia do app não atrapalha nem é apagado por esta.
        let alheio = pasta.0.join("a.json.1-0.tmp");
        fs::write(&alheio, "da outra").unwrap();
        gravar_atomico(&arquivo, b"um", false).unwrap();
        gravar_atomico(&arquivo, b"dois", true).unwrap();
        assert_eq!(fs::read(&arquivo).unwrap(), b"dois");
        assert_eq!(nomes(&pasta.0), ["a.json", "a.json.1-0.tmp"]);
    }

    #[test]
    fn gravacoes_ao_mesmo_tempo_deixam_um_arquivo_inteiro() {
        let pasta = PastaTemporaria::nova("ao-mesmo-tempo");
        let armazenamento = Armazenamento::new(&pasta.0);
        let mut grande = exemplo();
        for i in 0..300 {
            grande.add_connection(None, Connection::new(format!("host-{i}"), Kind::Ssh));
        }
        std::thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| {
                    for _ in 0..10 {
                        armazenamento.salvar(&grande).unwrap();
                    }
                });
            }
        });
        let (lida, aviso) = armazenamento.carregar();
        assert_eq!(aviso, None);
        assert_eq!(lida.connections().len(), grande.connections().len());
        assert!(!nomes(&pasta.0).iter().any(|n| n.ends_with(".tmp")));
    }

    #[test]
    fn duas_conexoes_ilegiveis_guardam_duas_copias() {
        let pasta = PastaTemporaria::nova("duas-ilegiveis");
        let armazenamento = Armazenamento::new(&pasta.0);
        fs::create_dir_all(&pasta.0).unwrap();
        fs::write(armazenamento.arquivo_conexoes(), "{ um").unwrap();
        assert!(armazenamento.carregar().1.is_some());
        fs::write(armazenamento.arquivo_conexoes(), "{ dois").unwrap();
        let aviso = armazenamento.carregar().1.unwrap();
        let copias: Vec<String> = nomes(&pasta.0)
            .into_iter()
            .filter(|n| n.starts_with("connections.json.bak"))
            .collect();
        assert_eq!(copias.len(), 2, "{copias:?}");
        assert_eq!(
            fs::read_to_string(pasta.0.join("connections.json.bak")).unwrap(),
            "{ um"
        );
        assert_eq!(
            fs::read_to_string(pasta.0.join(&copias[1])).unwrap(),
            "{ dois"
        );
        assert!(aviso.contains(&copias[1]), "{aviso}");
    }

    #[test]
    fn copia_do_dia_guarda_uma_por_dia_e_so_os_ultimos_sete() {
        const DIA: u64 = 86_400;
        let pasta = PastaTemporaria::nova("copia-do-dia");
        let copias = pasta.0.join(PASTA_COPIAS);
        fs::create_dir_all(&copias).unwrap();
        fs::write(copias.join("leia-me.txt"), "não é cópia").unwrap();
        for dia in 0..10u64 {
            crate::recentes::fixar_relogio(Some((20_000 + dia) * DIA));
            copia_do_dia(&pasta.0, "a.json", format!("dia {dia}").as_bytes(), false).unwrap();
            copia_do_dia(&pasta.0, "a.json", b"depois, no mesmo dia", false).unwrap();
            copia_do_dia(&pasta.0, "b.json", b"outro", false).unwrap();
        }
        crate::recentes::fixar_relogio(None);
        let todas = nomes(&copias);
        let de_a: Vec<&String> = todas.iter().filter(|n| n.ends_with("-a.json")).collect();
        assert_eq!(de_a.len(), DIAS_DE_COPIAS, "{todas:?}");
        assert_eq!(de_a[0], "20241007-a.json");
        assert_eq!(
            fs::read_to_string(copias.join("20241013-a.json")).unwrap(),
            "dia 9",
            "a primeira do dia fica"
        );
        assert_eq!(todas.iter().filter(|n| n.ends_with("-b.json")).count(), 7);
        assert!(todas.contains(&"leia-me.txt".to_owned()));
    }

    #[test]
    fn as_conexoes_ganham_a_copia_do_dia_de_antes_da_mudanca() {
        let pasta = PastaTemporaria::nova("conexoes-copia");
        let armazenamento = Armazenamento::new(&pasta.0);
        crate::recentes::fixar_relogio(Some(20_000 * 86_400));
        armazenamento.salvar(&Tree::default()).unwrap();
        assert!(
            !pasta.0.join(PASTA_COPIAS).exists(),
            "não havia o que copiar"
        );
        armazenamento.salvar(&exemplo()).unwrap();
        armazenamento.salvar(&Tree::default()).unwrap();
        crate::recentes::fixar_relogio(None);
        let copia = pasta.0.join(PASTA_COPIAS).join("20241004-connections.json");
        let guardada: Tree = serde_json::from_slice(&fs::read(copia).unwrap()).unwrap();
        assert_eq!(
            guardada.connections().len(),
            0,
            "a de antes da 1ª mudança do dia"
        );
    }

    #[test]
    fn uma_pasta_so_de_leitura_nao_e_tocada() {
        let pasta = PastaTemporaria::nova("so-leitura");
        let armazenamento = Armazenamento::new(&pasta.0);
        armazenamento.salvar(&exemplo()).unwrap();
        let antes = fs::read(armazenamento.arquivo_conexoes()).unwrap();
        somente_leitura_em(&pasta.0);
        assert!(so_leitura(&armazenamento.arquivo_conexoes()));
        assert!(so_leitura(&pasta.0.join("vault").join("ab12")));
        // As gravações automáticas são puladas em silêncio.
        armazenamento.salvar(&Tree::default()).unwrap();
        let mut visto = Visto::default();
        assert_eq!(
            armazenamento
                .salvar_visto(&Tree::default(), &mut visto)
                .unwrap(),
            None
        );
        armazenamento
            .gravar(&armazenamento.arquivo_estado(), &exemplo())
            .unwrap();
        assert_eq!(fs::read(armazenamento.arquivo_conexoes()).unwrap(), antes);
        assert!(!armazenamento.arquivo_estado().exists());
        // As que a pessoa pede devolvem o erro.
        let erro = gravar_atomico(&pasta.0.join("outro.json"), b"{}", false).unwrap_err();
        assert_eq!(erro.kind(), io::ErrorKind::ReadOnlyFilesystem);
        assert!(!pasta.0.join("outro.json").exists());
        // Um arquivo ilegível não sai do lugar.
        fs::write(armazenamento.arquivo_conexoes(), "{ pela metade").unwrap();
        let (tree, aviso) = armazenamento.carregar();
        assert!(tree.nodes.is_empty());
        assert!(aviso.is_some());
        assert_eq!(
            fs::read_to_string(armazenamento.arquivo_conexoes()).unwrap(),
            "{ pela metade"
        );
        assert!(!pasta.0.join("connections.json.bak").exists());
    }

    #[cfg(unix)]
    #[test]
    fn um_link_para_a_pasta_so_de_leitura_tambem_e_so_de_leitura() {
        let pasta = PastaTemporaria::nova("so-leitura-link");
        let outra = PastaTemporaria::nova("so-leitura-link-2");
        fs::create_dir_all(&pasta.0).unwrap();
        fs::create_dir_all(&outra.0).unwrap();
        let link = outra.0.join("atalho");
        std::os::unix::fs::symlink(&pasta.0, &link).unwrap();
        somente_leitura_em(&pasta.0);
        assert!(so_leitura(&link.join("connections.json")));
        assert!(so_leitura(&link.join("nova").join("vault.json")));
        assert!(!so_leitura(&outra.0.join("connections.json")));
    }

    #[test]
    fn o_que_esta_versao_nao_conhece_continua_no_arquivo() {
        let pasta = PastaTemporaria::nova("enxerto");
        let armazenamento = Armazenamento::new(&pasta.0);
        let mut tree = exemplo();
        let outro = tree.add_group(None, "Homologação");
        let mut valor = serde_json::to_value(&tree).unwrap();
        valor["campo_do_futuro"] = Value::from(7);
        valor["nodes"][0]["Group"]["children"][0]["Connection"]["opcao_nova"] = Value::from("sim");
        valor["nodes"][1]["Group"]["cor_nova"] = Value::from("azul");
        fs::create_dir_all(&pasta.0).unwrap();
        fs::write(armazenamento.arquivo_conexoes(), valor.to_string()).unwrap();

        let (mut lida, aviso) = armazenamento.carregar();
        assert_eq!(aviso, None);
        // Muda um nome, tira um grupo e põe outro.
        if let Some(crate::modelo::Node::Group(grupo)) = lida.nodes.first_mut() {
            grupo.name = "Prod".to_owned();
        }
        assert!(lida.remove(outro));
        lida.add_group(None, "Novo");
        armazenamento.salvar(&lida).unwrap();

        let gravado: Value =
            serde_json::from_slice(&fs::read(armazenamento.arquivo_conexoes()).unwrap()).unwrap();
        assert_eq!(gravado["campo_do_futuro"], 7);
        assert_eq!(gravado["nodes"][0]["Group"]["name"], "Prod");
        assert_eq!(
            gravado["nodes"][0]["Group"]["children"][0]["Connection"]["opcao_nova"],
            "sim"
        );
        let texto = gravado.to_string();
        assert!(
            !texto.contains("cor_nova"),
            "o grupo tirado não volta: {texto}"
        );
        assert!(!texto.contains("Homologação"), "{texto}");
        assert!(texto.contains("Novo"), "{texto}");
        // E o que se grava ainda se lê como a árvore desta versão.
        let (volta, _) = armazenamento.carregar();
        assert_eq!(volta, lida);
    }

    #[test]
    fn gravar_por_cima_de_uma_mudanca_de_fora_guarda_a_versao_dela() {
        let pasta = PastaTemporaria::nova("conflito");
        let armazenamento = Armazenamento::new(&pasta.0);
        armazenamento.salvar(&exemplo()).unwrap();
        let (mut nossa, _, mut visto) = armazenamento.carregar_visto();
        // Outro programa grava depois de lermos.
        let mut de_fora = exemplo();
        de_fora.add_group(None, "De fora");
        Armazenamento::new(&pasta.0).salvar(&de_fora).unwrap();
        let bytes_de_fora = fs::read(armazenamento.arquivo_conexoes()).unwrap();

        nossa.add_group(None, "Nossa");
        let copia = armazenamento
            .salvar_visto(&nossa, &mut visto)
            .unwrap()
            .expect("a versão de fora fica numa cópia");
        assert!(
            copia
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("connections.json.conflito-"),
            "{}",
            copia.display()
        );
        assert_eq!(fs::read(&copia).unwrap(), bytes_de_fora);
        let (lida, _) = armazenamento.carregar();
        assert_eq!(lida, nossa);
        // Sem nova mudança de fora, a próxima gravação não cria cópia.
        nossa.add_group(None, "Mais uma");
        assert_eq!(
            armazenamento.salvar_visto(&nossa, &mut visto).unwrap(),
            None
        );
    }

    #[test]
    fn um_arquivo_de_uma_versao_mais_nova_fica_so_de_leitura() {
        let pasta = PastaTemporaria::nova("versao-nova");
        let armazenamento = Armazenamento::new(&pasta.0);
        let mut valor = serde_json::to_value(exemplo()).unwrap();
        valor["version"] = Value::from(VERSAO + 1);
        let texto = valor.to_string();
        fs::create_dir_all(&pasta.0).unwrap();
        fs::write(armazenamento.arquivo_conexoes(), &texto).unwrap();
        let (lida, aviso) = armazenamento.carregar();
        assert_eq!(lida.connections().len(), 1, "o que se entende, aparece");
        assert_eq!(aviso, Some(t!("armazenamento-conexoes-versao-mais-nova")));
        armazenamento.salvar(&Tree::default()).unwrap();
        assert_eq!(
            fs::read_to_string(armazenamento.arquivo_conexoes()).unwrap(),
            texto
        );
    }

    #[test]
    fn uma_versao_mais_nova_gravada_depois_de_ler_nao_e_sobrescrita() {
        let pasta = PastaTemporaria::nova("versao-nova-depois");
        let armazenamento = Armazenamento::new(&pasta.0);
        armazenamento.salvar(&exemplo()).unwrap();
        let (tree, _, mut visto) = armazenamento.carregar_visto();
        let mut valor = serde_json::to_value(exemplo()).unwrap();
        valor["version"] = Value::from(VERSAO + 1);
        let texto = valor.to_string();
        fs::write(armazenamento.arquivo_conexoes(), &texto).unwrap();
        let erro = armazenamento.salvar_visto(&tree, &mut visto).unwrap_err();
        assert_eq!(
            erro.to_string(),
            t!("armazenamento-conexoes-versao-mais-nova")
        );
        // Daí em diante, só leitura, em silêncio.
        assert_eq!(armazenamento.salvar_visto(&tree, &mut visto).unwrap(), None);
        assert_eq!(
            fs::read_to_string(armazenamento.arquivo_conexoes()).unwrap(),
            texto
        );
    }

    #[test]
    fn reler_so_devolve_a_arvore_quando_o_arquivo_mudou_e_se_le() {
        let pasta = PastaTemporaria::nova("reler");
        let armazenamento = Armazenamento::new(&pasta.0);
        armazenamento.salvar(&exemplo()).unwrap();
        let (mut nossa, _, mut visto) = armazenamento.carregar_visto();
        assert_eq!(armazenamento.reler_se_mudou(&mut visto), None);

        let mut de_fora = exemplo();
        de_fora.add_group(None, "De fora");
        Armazenamento::new(&pasta.0).salvar(&de_fora).unwrap();
        let (lida, aviso) = armazenamento
            .reler_se_mudou(&mut visto)
            .expect("o arquivo mudou");
        assert_eq!(lida, de_fora);
        assert_eq!(aviso, None);
        assert_eq!(armazenamento.reler_se_mudou(&mut visto), None);

        // Um editor no meio da gravação: não muda nada aqui, e o arquivo não sai do lugar.
        fs::write(armazenamento.arquivo_conexoes(), "{ pela").unwrap();
        assert_eq!(armazenamento.reler_se_mudou(&mut visto), None);
        assert!(armazenamento.arquivo_conexoes().exists());
        assert!(!pasta.0.join("connections.json.bak").exists());

        // O que esta cópia grava não volta como mudança de fora.
        nossa.add_group(None, "Nossa");
        armazenamento.salvar_visto(&nossa, &mut visto).unwrap();
        assert_eq!(armazenamento.reler_se_mudou(&mut visto), None);
    }
}
