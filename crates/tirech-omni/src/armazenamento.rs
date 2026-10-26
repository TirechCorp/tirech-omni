//! Onde o app guarda as conexões e o estado das abas: dois JSON numa pasta. Quem decide a pasta
//! é só o `main` (por [`pasta`]); o resto recebe a pasta por parâmetro, e os testes usam uma
//! pasta temporária própria.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::modelo::Tree;
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
        self.pasta.join("connections.json")
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
            Err(erro) if erro.kind() == io::ErrorKind::NotFound => return (T::default(), None),
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
        match serde_json::from_str::<T>(&texto) {
            Ok(valor) => (valor, None),
            Err(_) => {
                let mut copia = arquivo.as_os_str().to_owned();
                copia.push(".bak");
                let copia = PathBuf::from(copia);
                let aviso = match fs::rename(arquivo, &copia) {
                    Ok(()) => t!(
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
    /// `connections.json.bak`, a árvore começa vazia e o aviso (para a tela mostrar) diz onde
    /// ficou a cópia.
    pub fn carregar(&self) -> (Tree, Option<String>) {
        let arquivo = self.arquivo_conexoes();
        let texto = match fs::read_to_string(&arquivo) {
            Ok(texto) => texto,
            Err(erro) if erro.kind() == io::ErrorKind::NotFound => return (Tree::default(), None),
            Err(erro) => {
                return (
                    Tree::default(),
                    Some(t!(
                        "comum-nao-consegui-ler",
                        arquivo = arquivo.display().to_string(),
                        erro = erro.to_string()
                    )),
                );
            }
        };
        match serde_json::from_str::<Tree>(&texto) {
            Ok(mut tree) => {
                // Um `next_id` perdido ou velho não pode repetir um id que já existe.
                tree.next_id = tree.next_id.max(maior_id(&tree) + 1);
                (tree, None)
            }
            Err(_) => {
                let copia = self.pasta.join("connections.json.bak");
                let aviso = match fs::rename(&arquivo, &copia) {
                    Ok(()) => t!(
                        "armazenamento-conexoes-corrompido-copia",
                        copia = copia.display().to_string()
                    ),
                    Err(erro) => t!(
                        "armazenamento-conexoes-corrompido-sem-copia",
                        erro = erro.to_string()
                    ),
                };
                (Tree::default(), Some(aviso))
            }
        }
    }

    /// Grava as conexões (atômico: escreve num `.tmp` e renomeia).
    pub fn salvar(&self, tree: &Tree) -> io::Result<()> {
        self.gravar(&self.arquivo_conexoes(), tree)
    }

    /// Grava um valor como JSON bonito, de forma atômica.
    pub(crate) fn gravar<T: Serialize>(&self, arquivo: &Path, valor: &T) -> io::Result<()> {
        fs::create_dir_all(&self.pasta)?;
        let texto = serde_json::to_string_pretty(valor).map_err(io::Error::other)?;
        let mut tmp = arquivo.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, texto)?;
        fs::rename(&tmp, arquivo)
    }

    /// Lê um JSON; `None` se o arquivo não existe ou não serve.
    pub(crate) fn ler<T: DeserializeOwned>(&self, arquivo: &Path) -> Option<T> {
        serde_json::from_str(&fs::read_to_string(arquivo).ok()?).ok()
    }
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
        assert_eq!(arquivos, ["connections.json"]);
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
}
