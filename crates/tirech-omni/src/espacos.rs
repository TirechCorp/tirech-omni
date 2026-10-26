//! Espaços de trabalho: um jeito de guardar e reabrir um conjunto de abas. O mesmo formato
//! serve a um painel (uma janela 1 × 1), a uma janela, a todas as janelas e ao `estado.json`.
//!
//! Um espaço guarda referências às conexões (o id, mais o nome, o host e o usuário para achar a
//! conexão se o id sumir), nunca uma cópia, e nunca senhas: quem abre pede ao cofre. Fica num
//! arquivo `espacos/**/<nome>.json` da pasta do app; as subpastas são pastas de verdade.
//! Este módulo é dado puro: não usa widget nem signal além do `PaneLayout` que lê.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tomni_ui::terminal::{PaneLayout, Session};

use crate::configuracoes::ModoNotas;
use crate::modelo::{Connection, Multiplexador, SessaoDaAba, Tree};
use crate::paineis::PaineisSalvos;
use crate::t;

/// A versão do formato dos espaços.
pub const VERSAO: u32 = 1;

/// A pasta (dentro da pasta do app) onde ficam os arquivos dos espaços.
pub const PASTA: &str = "espacos";

/// O tamanho máximo do nome de um espaço ou de uma pasta, em caracteres.
const NOME_MAXIMO: usize = 100;

/// O que o espaço foi quando se salvou. Só informa a pessoa e ajuda a abrir: a estrutura é a
/// mesma nos três casos.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origem {
    /// Um painel (vira uma janela 1 × 1).
    #[default]
    Painel,
    /// Uma janela.
    Janela,
    /// Todas as janelas (também é o `estado.json`).
    Todas,
}

/// Uma aba: a conexão que ela abre. `id: None` é o shell local avulso (Ctrl+Shift+T).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// O id da conexão na árvore; `None` no shell local avulso.
    #[serde(default)]
    pub id: Option<u64>,
    /// O nome da conexão na hora de salvar.
    #[serde(default)]
    pub nome: String,
    /// O servidor na hora de salvar.
    #[serde(default)]
    pub host: String,
    /// O usuário na hora de salvar.
    #[serde(default)]
    pub user: String,
    /// A sessão do tmux em que a aba rodava (a sessão persistente), para a restauração voltar
    /// a ela.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux: Option<String>,
    /// A sessão do screen em que a aba rodava, como a do `tmux` (uma aba tem uma ou outra).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<String>,
    /// Os assistentes do "Controlar com" que controlavam a aba, para voltarem com ela,
    /// continuando a conversa.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assistentes: Vec<AssistenteSalvo>,
}

/// Um assistente do "Controlar com" que controlava uma aba.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssistenteSalvo {
    /// O assistente (`claude`, `codex` ou `opencode`).
    pub ferramenta: String,
    /// O id da conversa, para continuar; `None` quando ainda não se sabia (ou o diário está
    /// desligado), e o assistente volta numa conversa nova.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversa: Option<String>,
}

impl Item {
    /// O item de uma conexão da árvore.
    pub fn da_conexao(c: &Connection) -> Self {
        Self {
            id: Some(c.id),
            nome: c.name.clone(),
            host: c.host.clone(),
            user: c.user.clone(),
            tmux: None,
            screen: None,
            assistentes: Vec::new(),
        }
    }

    /// A sessão do servidor em que a aba rodava, se rodava numa.
    pub fn sessao(&self) -> Option<SessaoDaAba> {
        match (&self.tmux, &self.screen) {
            (Some(nome), _) => Some(SessaoDaAba::new(Multiplexador::Tmux, nome.clone())),
            (None, Some(nome)) => Some(SessaoDaAba::new(Multiplexador::Screen, nome.clone())),
            (None, None) => None,
        }
    }

    /// Guarda a sessão do servidor em que a aba roda.
    pub fn com_sessao(mut self, sessao: Option<SessaoDaAba>) -> Self {
        (self.tmux, self.screen) = match sessao {
            Some(s) if s.programa == Multiplexador::Tmux => (Some(s.nome), None),
            Some(s) => (None, Some(s.nome)),
            None => (None, None),
        };
        self
    }

    /// Guarda os assistentes que controlam a aba.
    pub fn com_assistentes(mut self, assistentes: Vec<AssistenteSalvo>) -> Self {
        self.assistentes = assistentes;
        self
    }

    /// O shell local avulso.
    pub fn shell_local() -> Self {
        Self {
            id: None,
            nome: t!("dica-shell-local"),
            host: String::new(),
            user: String::new(),
            tmux: None,
            screen: None,
            assistentes: Vec::new(),
        }
    }
}

/// O que uma janela guarda: a grade, as abas de cada painel e, se o sistema informa, onde a
/// janela estava.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JanelaSalva {
    /// Colunas e linhas da grade.
    pub grade: (usize, usize),
    /// As abas de cada painel, na ordem.
    #[serde(default)]
    pub paineis: Vec<Vec<Item>>,
    /// A posição na tela, em pixels lógicos (o Wayland não informa).
    #[serde(default)]
    pub posicao: Option<(i32, i32)>,
    /// O tamanho do conteúdo, em pixels lógicos.
    #[serde(default)]
    pub tamanho: Option<(u32, u32)>,
    /// A janela é um cluster com IA: ao reabrir, o assistente volta para controlar as máquinas,
    /// continuando a conversa. O `estado.json` só guarda quando as Configurações reabrem os
    /// assistentes (`conversas_reabrir`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cluster_ia: Option<ClusterIaSalvo>,
    /// Os painéis Conexões e Propriedades de uma janela secundária. A principal guarda os dela
    /// em `Espaco.paineis`, como as outras páginas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub laterais: Option<PaineisSalvos>,
}

/// O assistente de um cluster com IA num espaço de trabalho.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterIaSalvo {
    /// O assistente (`claude`, `codex` ou `opencode`).
    pub ferramenta: String,
    /// O painel da aba dele.
    pub painel: usize,
    /// O id da conversa dele, para continuar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversa: Option<String>,
}

/// Uma aba da página Arquivos no `estado.json`: a conexão e as pastas abertas nos dois
/// painéis. Uma aba que nasceu de uma sessão volta como a conexão própria.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArquivoSalvo {
    /// O id da conexão na árvore.
    #[serde(default)]
    pub conexao: u64,
    /// A pasta do painel local (vazia: o início).
    #[serde(default)]
    pub local: String,
    /// A pasta do painel remoto (vazia: o início).
    #[serde(default)]
    pub remoto: String,
}

/// O que a página Notas lembra no `estado.json`: o id da nota aberta, o modo e os tamanhos
/// (em milésimos). Nunca o texto.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotasSalvas {
    /// O id da nota aberta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nota: Option<u64>,
    /// Editar, Dividir ou Ler.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modo: Option<ModoNotas>,
    /// A largura da lateral, em pixels. Desde o M54 o app não grava mais este campo (a largura
    /// mora em `Espaco.paineis`); ele só é lido, para um `estado.json` antigo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lateral: Option<u32>,
    /// A parte do corpo que o editor ocupa, em milésimos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub divisor: Option<u32>,
}

/// O formato único: de um painel ao `estado.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Espaco {
    /// A versão do formato.
    #[serde(default = "versao_atual")]
    pub versao: u32,
    /// O que foi salvo.
    #[serde(default)]
    pub origem: Origem,
    /// As janelas, na ordem.
    #[serde(default)]
    pub janelas: Vec<JanelaSalva>,
    /// As abas da página Arquivos (só o `estado.json` guarda).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arquivos: Vec<ArquivoSalvo>,
    /// A página Notas (só o `estado.json` guarda).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notas: Option<NotasSalvas>,
    /// Os painéis encaixáveis de cada página (só o `estado.json` guarda), pela chave da
    /// página: `terminais`, `notas` e `arquivos` (M54).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub paineis: BTreeMap<String, PaineisSalvos>,
}

fn versao_atual() -> u32 {
    VERSAO
}

impl Default for Espaco {
    fn default() -> Self {
        Self {
            versao: VERSAO,
            origem: Origem::default(),
            janelas: Vec::new(),
            arquivos: Vec::new(),
            notas: None,
            paineis: BTreeMap::new(),
        }
    }
}

impl Espaco {
    /// Um espaço de `origem` com as `janelas`.
    pub fn new(origem: Origem, janelas: Vec<JanelaSalva>) -> Self {
        Self {
            versao: VERSAO,
            origem,
            janelas,
            arquivos: Vec::new(),
            notas: None,
            paineis: BTreeMap::new(),
        }
    }

    /// Há alguma aba para reabrir?
    pub fn tem_abas(&self) -> bool {
        self.janelas.iter().any(JanelaSalva::tem_abas)
    }

    /// O total de abas.
    pub fn total_de_abas(&self) -> usize {
        self.janelas
            .iter()
            .flat_map(|j| &j.paineis)
            .map(Vec::len)
            .sum()
    }
}

impl JanelaSalva {
    /// A janela de `layout` e `grade`. Sessões cuja chave não começa pelo id de uma conexão
    /// (`"{id}-{n}"`) ou cuja conexão não está mais na árvore ficam de fora; o id 0 é o shell
    /// local avulso.
    /// As abas de `pular` (as dos assistentes, que são shells locais por baixo) ficam de fora.
    /// `completar` acrescenta ao item o que a tela sabe da aba: a sessão do servidor (tmux ou
    /// screen) em que ela roda e os assistentes que a controlam.
    pub fn do_layout(
        grade: (usize, usize),
        layout: &PaneLayout,
        pular: &[Session],
        tree: &Tree,
        completar: &dyn Fn(Session, Item) -> Item,
    ) -> Self {
        let paineis = (0..layout.pane_count())
            .map(|n| itens_do_painel(layout, n, pular, tree, completar))
            .collect();
        Self {
            grade,
            paineis,
            posicao: None,
            tamanho: None,
            cluster_ia: None,
            laterais: None,
        }
    }

    /// Uma janela 1 × 1 só com as abas do painel `painel` de `layout`, menos as de `pular`.
    pub fn do_painel(
        layout: &PaneLayout,
        painel: usize,
        pular: &[Session],
        tree: &Tree,
        completar: &dyn Fn(Session, Item) -> Item,
    ) -> Self {
        Self {
            grade: (1, 1),
            paineis: vec![itens_do_painel(layout, painel, pular, tree, completar)],
            posicao: None,
            tamanho: None,
            cluster_ia: None,
            laterais: None,
        }
    }

    /// Há alguma aba para reabrir?
    pub fn tem_abas(&self) -> bool {
        self.paineis.iter().any(|p| !p.is_empty())
    }

    /// Acha as conexões de cada aba na árvore de agora.
    pub fn resolver(&self, tree: &Tree) -> Resolvida {
        let mut faltando = Vec::new();
        let paineis = self
            .paineis
            .iter()
            .map(|painel| {
                painel
                    .iter()
                    .filter_map(|item| match resolver_item(item, tree) {
                        Some(aba) => Some((aba, item.assistentes.clone())),
                        None => {
                            faltando.push(item.nome.clone());
                            None
                        }
                    })
                    .collect()
            })
            .collect();
        Resolvida { paineis, faltando }
    }
}

fn itens_do_painel(
    layout: &PaneLayout,
    painel: usize,
    pular: &[Session],
    tree: &Tree,
    completar: &dyn Fn(Session, Item) -> Item,
) -> Vec<Item> {
    layout
        .pane(painel)
        .tabs
        .iter()
        .filter(|sessao| !pular.contains(sessao))
        .filter_map(|&sessao| {
            let id = id_da_chave(&sessao.key_str())?;
            let item = if id == 0 {
                Item::shell_local()
            } else {
                Item::da_conexao(tree.connection(id)?)
            };
            Some(completar(sessao, item))
        })
        .collect()
}

/// O id da conexão numa chave de sessão `"{id}-{n}"`.
pub(crate) fn id_da_chave(chave: &str) -> Option<u64> {
    chave.split('-').next()?.parse().ok()
}

/// A aba que o espaço abre, já ligada a uma conexão da árvore.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Aba {
    /// O shell local avulso.
    Local,
    /// A conexão de id dado, com a sessão do servidor (tmux ou screen) em que a aba rodava.
    Conexao(u64, Option<SessaoDaAba>),
}

/// O resultado de achar, na árvore de agora, as conexões de uma janela salva.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolvida {
    /// As abas de cada painel (as que sumiram ficam de fora), com os assistentes que voltam
    /// com elas.
    pub paineis: Vec<Vec<(Aba, Vec<AssistenteSalvo>)>>,
    /// Os nomes das conexões que não existem mais, na ordem.
    pub faltando: Vec<String>,
}

/// A conexão de um item: pelo id e, se ele sumiu, pelo nome, host e usuário.
pub fn resolver_item(item: &Item, tree: &Tree) -> Option<Aba> {
    let Some(id) = item.id else {
        return Some(Aba::Local);
    };
    let tmux = item.sessao();
    if tree.connection(id).is_some() {
        return Some(Aba::Conexao(id, tmux));
    }
    if item.nome.is_empty() {
        return None;
    }
    tree.connections()
        .into_iter()
        .find(|c| c.name == item.nome && c.host == item.host && c.user == item.user)
        .map(|c| Aba::Conexao(c.id, tmux))
}

/// O aviso das conexões que o espaço não achou, ou `None` se achou todas.
pub fn aviso_de_faltantes(faltando: &[String]) -> Option<String> {
    match faltando {
        [] => None,
        [uma] => Some(t!("espacos-faltante", n = 1, lista = uma.as_str())),
        varias => Some(t!(
            "espacos-faltante",
            n = varias.len() as i64,
            lista = varias.join(", ")
        )),
    }
}

/// Por que uma operação nos arquivos dos espaços não deu.
#[derive(Debug)]
pub enum Erro {
    /// O nome tem caracteres que o sistema de arquivos não aceita, ou está vazio.
    NomeInvalido,
    /// Já existe um item com esse nome na pasta.
    JaExiste,
    /// A pasta tem espaços ou subpastas.
    NaoVazia,
    /// O item não existe (mais).
    NaoExiste,
    /// Mover uma pasta para dentro dela mesma.
    DentroDeSiMesma,
    /// O arquivo não é um espaço válido.
    Invalido(String),
    /// Falha do sistema de arquivos.
    Io(io::Error),
}

impl fmt::Display for Erro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Erro::NomeInvalido => write!(f, "{}", t!("espacos-nome-invalido")),
            Erro::JaExiste => write!(f, "{}", t!("espacos-ja-existe")),
            Erro::NaoVazia => write!(f, "{}", t!("espacos-nao-vazia")),
            Erro::NaoExiste => write!(f, "{}", t!("espacos-nao-existe")),
            Erro::DentroDeSiMesma => write!(f, "{}", t!("espacos-dentro-de-si-mesma")),
            Erro::Invalido(motivo) => {
                write!(f, "{}", t!("espacos-invalido", motivo = motivo.as_str()))
            }
            Erro::Io(erro) => write!(f, "{erro}"),
        }
    }
}

impl std::error::Error for Erro {}

impl From<io::Error> for Erro {
    fn from(erro: io::Error) -> Self {
        if erro.kind() == io::ErrorKind::NotFound {
            Erro::NaoExiste
        } else {
            Erro::Io(erro)
        }
    }
}

/// Se `nome` serve de nome de espaço ou de pasta: não vazio, sem `/ \ : * ? " < > |` nem
/// caracteres de controle, sem ser `.` ou `..` e sem terminar em ponto ou espaço.
pub fn nome_valido(nome: &str) -> bool {
    let nome = nome.trim();
    !nome.is_empty()
        && nome.chars().count() <= NOME_MAXIMO
        && nome != "."
        && nome != ".."
        && !nome.ends_with('.')
        && !nome
            .chars()
            .any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}

/// Um item da lista dos espaços: uma pasta (com o que tem dentro) ou um espaço.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entrada {
    /// Uma pasta.
    Pasta {
        /// O nome.
        nome: String,
        /// O caminho a partir da pasta dos espaços, com `/` (`"Produção/diário"`).
        caminho: String,
        /// O que tem dentro: pastas primeiro, depois espaços, cada grupo em ordem alfabética.
        filhos: Vec<Entrada>,
    },
    /// Um espaço.
    Espaco {
        /// O nome (o do arquivo, sem o `.json`).
        nome: String,
        /// O caminho a partir da pasta dos espaços, sem o `.json`.
        caminho: String,
    },
}

impl Entrada {
    /// O nome.
    pub fn nome(&self) -> &str {
        match self {
            Entrada::Pasta { nome, .. } | Entrada::Espaco { nome, .. } => nome,
        }
    }

    /// O caminho.
    pub fn caminho(&self) -> &str {
        match self {
            Entrada::Pasta { caminho, .. } | Entrada::Espaco { caminho, .. } => caminho,
        }
    }
}

/// Os arquivos dos espaços numa pasta do app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Espacos {
    raiz: PathBuf,
}

impl Espacos {
    /// Os espaços de `pasta_do_app` (a subpasta [`PASTA`], criada ao gravar).
    pub fn new(pasta_do_app: &Path) -> Self {
        Self {
            raiz: pasta_do_app.join(PASTA),
        }
    }

    /// Os espaços guardados em `raiz` mesmo (`janelas.pasta_espacos`), sem a subpasta [`PASTA`].
    pub fn na_pasta(raiz: PathBuf) -> Self {
        Self { raiz }
    }

    /// A pasta raiz dos espaços.
    pub fn raiz(&self) -> &Path {
        &self.raiz
    }

    /// A pasta no disco de um caminho relativo (vazio é a raiz). `None` se algum pedaço não é
    /// um nome válido (isso barra `..` e caminhos absolutos).
    fn pasta_no_disco(&self, caminho: &str) -> Option<PathBuf> {
        let mut pasta = self.raiz.clone();
        for parte in caminho.split('/').filter(|p| !p.is_empty()) {
            if !nome_valido(parte) {
                return None;
            }
            pasta.push(parte);
        }
        Some(pasta)
    }

    /// O arquivo de um espaço.
    fn arquivo(&self, caminho: &str) -> Option<PathBuf> {
        let (pasta, nome) = separar(caminho);
        if !nome_valido(nome) {
            return None;
        }
        Some(
            self.pasta_no_disco(pasta)?
                .join(format!("{}.json", nome.trim())),
        )
    }

    /// O caminho relativo de `nome` dentro de `pasta`.
    pub fn juntar(pasta: &str, nome: &str) -> String {
        if pasta.is_empty() {
            nome.trim().to_owned()
        } else {
            format!("{pasta}/{}", nome.trim())
        }
    }

    /// Já existe um espaço `nome` em `pasta`?
    pub fn existe(&self, pasta: &str, nome: &str) -> bool {
        self.arquivo(&Self::juntar(pasta, nome))
            .is_some_and(|a| a.is_file())
    }

    /// Existe o espaço de `caminho` (relativo à pasta dos espaços, sem o `.json`)?
    pub fn existe_caminho(&self, caminho: &str) -> bool {
        self.arquivo(caminho).is_some_and(|a| a.is_file())
    }

    /// Grava `espaco` como `nome` em `pasta` (atômico: escreve num `.tmp` e renomeia) e devolve
    /// o caminho. Se já existe e `substituir` é falso, devolve [`Erro::JaExiste`].
    pub fn gravar(
        &self,
        pasta: &str,
        nome: &str,
        espaco: &Espaco,
        substituir: bool,
    ) -> Result<String, Erro> {
        crate::armazenamento::conferir_gravacao(&self.raiz)?;
        if !nome_valido(nome) {
            return Err(Erro::NomeInvalido);
        }
        let caminho = Self::juntar(pasta, nome);
        let arquivo = self.arquivo(&caminho).ok_or(Erro::NomeInvalido)?;
        if !substituir && arquivo.exists() {
            return Err(Erro::JaExiste);
        }
        if let Some(dir) = arquivo.parent() {
            fs::create_dir_all(dir)?;
        }
        let texto = serde_json::to_string_pretty(espaco).map_err(io::Error::other)?;
        crate::armazenamento::gravar_atomico(&arquivo, texto.as_bytes(), false)?;
        Ok(caminho)
    }

    /// Lê o espaço de `caminho`.
    pub fn ler(&self, caminho: &str) -> Result<Espaco, Erro> {
        let arquivo = self.arquivo(caminho).ok_or(Erro::NomeInvalido)?;
        let texto = fs::read_to_string(arquivo)?;
        serde_json::from_str(&texto).map_err(|e| Erro::Invalido(e.to_string()))
    }

    /// Os espaços e as pastas, a partir da raiz. Sem a pasta ainda, a lista é vazia. Arquivos
    /// que não são `.json` (e os `.tmp` de uma gravação interrompida) não entram.
    pub fn listar(&self) -> Vec<Entrada> {
        listar_pasta(&self.raiz, "")
    }

    /// Os caminhos de todas as pastas, em ordem de árvore, sem a raiz (que é `""`).
    pub fn pastas(&self) -> Vec<String> {
        fn visita(entradas: &[Entrada], saida: &mut Vec<String>) {
            for entrada in entradas {
                if let Entrada::Pasta {
                    caminho, filhos, ..
                } = entrada
                {
                    saida.push(caminho.clone());
                    visita(filhos, saida);
                }
            }
        }
        let mut saida = Vec::new();
        visita(&self.listar(), &mut saida);
        saida
    }

    /// Cria a pasta `nome` dentro de `pai` e devolve o caminho dela. Uma pasta que já existe
    /// vale como criada.
    pub fn criar_pasta(&self, pai: &str, nome: &str) -> Result<String, Erro> {
        crate::armazenamento::conferir_gravacao(&self.raiz)?;
        let caminho = Self::juntar(pai, nome);
        if !nome_valido(nome) {
            return Err(Erro::NomeInvalido);
        }
        let dir = self.pasta_no_disco(&caminho).ok_or(Erro::NomeInvalido)?;
        fs::create_dir_all(dir)?;
        Ok(caminho)
    }

    /// Dá o nome `novo` ao espaço ou à pasta `caminho` e devolve o caminho novo.
    pub fn renomear(&self, caminho: &str, novo: &str, pasta: bool) -> Result<String, Erro> {
        if !nome_valido(novo) {
            return Err(Erro::NomeInvalido);
        }
        let (pai, _) = separar(caminho);
        let destino = Self::juntar(pai, novo);
        self.mover_arquivo(caminho, &destino, pasta)?;
        Ok(destino)
    }

    /// Leva o espaço ou a pasta `caminho` para dentro da pasta `para` e devolve o caminho novo.
    pub fn mover(&self, caminho: &str, para: &str, pasta: bool) -> Result<String, Erro> {
        let (_, nome) = separar(caminho);
        if pasta && (para == caminho || para.starts_with(&format!("{caminho}/"))) {
            return Err(Erro::DentroDeSiMesma);
        }
        let destino = Self::juntar(para, nome);
        self.mover_arquivo(caminho, &destino, pasta)?;
        Ok(destino)
    }

    fn mover_arquivo(&self, de: &str, para: &str, pasta: bool) -> Result<(), Erro> {
        crate::armazenamento::conferir_gravacao(&self.raiz)?;
        let (origem, destino) = if pasta {
            (self.pasta_no_disco(de), self.pasta_no_disco(para))
        } else {
            (self.arquivo(de), self.arquivo(para))
        };
        let (origem, destino) = origem.zip(destino).ok_or(Erro::NomeInvalido)?;
        if !origem.exists() {
            return Err(Erro::NaoExiste);
        }
        if origem != destino && destino.exists() {
            return Err(Erro::JaExiste);
        }
        if let Some(dir) = destino.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::rename(origem, destino)?;
        Ok(())
    }

    /// Apaga o espaço `caminho`.
    pub fn apagar(&self, caminho: &str) -> Result<(), Erro> {
        crate::armazenamento::conferir_gravacao(&self.raiz)?;
        let arquivo = self.arquivo(caminho).ok_or(Erro::NomeInvalido)?;
        fs::remove_file(arquivo)?;
        Ok(())
    }

    /// Apaga a pasta `caminho`. Com `tudo` falso, só se estiver vazia ([`Erro::NaoVazia`]).
    pub fn apagar_pasta(&self, caminho: &str, tudo: bool) -> Result<(), Erro> {
        crate::armazenamento::conferir_gravacao(&self.raiz)?;
        let dir = self.pasta_no_disco(caminho).ok_or(Erro::NomeInvalido)?;
        if dir == self.raiz {
            return Err(Erro::NomeInvalido);
        }
        if tudo {
            fs::remove_dir_all(dir)?;
        } else {
            match fs::remove_dir(&dir) {
                Ok(()) => {}
                Err(erro) if erro.kind() == io::ErrorKind::NotFound => {
                    return Err(Erro::NaoExiste);
                }
                Err(_) => return Err(Erro::NaoVazia),
            }
        }
        Ok(())
    }

    /// Os nomes dos espaços que há dentro da pasta `caminho` e das subpastas dela, com o
    /// caminho a partir dela (`"diário"`, `"sub/outro"`).
    pub fn espacos_dentro(&self, caminho: &str) -> Vec<String> {
        fn visita(entradas: &[Entrada], prefixo: &str, saida: &mut Vec<String>) {
            for entrada in entradas {
                match entrada {
                    Entrada::Espaco { nome, .. } => saida.push(format!("{prefixo}{nome}")),
                    Entrada::Pasta { nome, filhos, .. } => {
                        visita(filhos, &format!("{prefixo}{nome}/"), saida);
                    }
                }
            }
        }
        let Some(dir) = self.pasta_no_disco(caminho) else {
            return Vec::new();
        };
        let mut saida = Vec::new();
        visita(&listar_pasta(&dir, caminho), "", &mut saida);
        saida
    }
}

/// O caminho separado em (pasta, nome).
fn separar(caminho: &str) -> (&str, &str) {
    caminho.rsplit_once('/').unwrap_or(("", caminho))
}

fn listar_pasta(dir: &Path, prefixo: &str) -> Vec<Entrada> {
    let Ok(lista) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut pastas = Vec::new();
    let mut espacos = Vec::new();
    for entrada in lista.flatten() {
        let caminho_no_disco = entrada.path();
        let Some(nome) = caminho_no_disco.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(tipo) = entrada.file_type() else {
            continue;
        };
        if tipo.is_dir() {
            if !nome_valido(nome) {
                continue;
            }
            let caminho = Espacos::juntar(prefixo, nome);
            let filhos = listar_pasta(&caminho_no_disco, &caminho);
            pastas.push(Entrada::Pasta {
                nome: nome.to_owned(),
                caminho,
                filhos,
            });
        } else if tipo.is_file()
            && let Some(nome) = nome.strip_suffix(".json")
            && nome_valido(nome)
        {
            espacos.push(Entrada::Espaco {
                nome: nome.to_owned(),
                caminho: Espacos::juntar(prefixo, nome),
            });
        }
    }
    let chave = |e: &Entrada| e.nome().to_lowercase();
    pastas.sort_by_key(chave);
    espacos.sort_by_key(chave);
    pastas.extend(espacos);
    pastas
}

/// O rótulo de um `Item` para a frase "nome (usuário@host)", quando se quer mostrar algo mais
/// que o nome.
pub fn rotulo_do_item(item: &Item) -> String {
    match (item.user.is_empty(), item.host.is_empty()) {
        (_, true) => item.nome.clone(),
        (true, false) => format!("{} ({})", item.nome, item.host),
        (false, false) => format!("{} ({}@{})", item.nome, item.user, item.host),
    }
}

#[cfg(test)]
mod testes {
    use tomni_ui::terminal::{Loopback, Session};

    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::modelo::Kind;

    fn arvore() -> Tree {
        let mut tree = Tree::default();
        let mut web = Connection::new("web-01", Kind::Ssh);
        web.host = "web-01.exemplo".into();
        web.user = "deploy".into();
        tree.add_connection(None, web);
        let mut db = Connection::new("db-02", Kind::Ssh);
        db.host = "10.0.0.2".into();
        db.user = "dba".into();
        tree.add_connection(None, db);
        tree
    }

    fn espaco() -> Espaco {
        Espaco::new(
            Origem::Janela,
            vec![JanelaSalva {
                grade: (2, 1),
                paineis: vec![
                    vec![Item {
                        id: Some(1),
                        nome: "web-01".into(),
                        host: "web-01.exemplo".into(),
                        user: "deploy".into(),
                        tmux: None,
                        screen: None,
                        assistentes: Vec::new(),
                    }],
                    vec![Item::shell_local()],
                ],
                posicao: Some((10, -20)),
                tamanho: Some((800, 600)),
                cluster_ia: None,
                laterais: None,
            }],
        )
    }

    #[test]
    fn ida_e_volta_pelo_json() {
        let espaco = espaco();
        let texto = serde_json::to_string_pretty(&espaco).unwrap();
        assert_eq!(serde_json::from_str::<Espaco>(&texto).unwrap(), espaco);
        assert!(texto.contains("\"origem\": \"janela\""), "{texto}");
        assert!(espaco.tem_abas());
        assert_eq!(espaco.total_de_abas(), 2);
        assert!(!Espaco::default().tem_abas());
    }

    #[test]
    fn campos_que_faltam_viram_o_padrao() {
        let espaco: Espaco = serde_json::from_str(r#"{"janelas":[{"grade":[1,1]}]}"#).unwrap();
        assert_eq!(espaco.versao, VERSAO);
        assert_eq!(espaco.origem, Origem::Painel);
        assert_eq!(espaco.janelas[0].paineis, Vec::<Vec<Item>>::new());
        assert_eq!(espaco.janelas[0].posicao, None);
    }

    #[test]
    fn nomes_com_caracteres_proibidos_sao_recusados() {
        for ruim in [
            "",
            "  ",
            "a/b",
            "a\\b",
            "a:b",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            ".",
            "..",
            "fim.",
            "tab\tulação",
        ] {
            assert!(!nome_valido(ruim), "{ruim:?} devia ser recusado");
        }
        for bom in ["Produção", "web 01", "diário-1", "a.b"] {
            assert!(nome_valido(bom), "{bom:?} devia valer");
        }
        let pasta = PastaTemporaria::nova("nome-ruim");
        let espacos = Espacos::new(&pasta.0);
        assert!(matches!(
            espacos.gravar("", "a/b", &espaco(), false),
            Err(Erro::NomeInvalido)
        ));
        assert!(matches!(
            espacos.gravar("../fora", "x", &espaco(), false),
            Err(Erro::NomeInvalido)
        ));
        assert!(!pasta.0.exists(), "nada foi criado");
    }

    #[test]
    fn gravar_cria_as_subpastas_e_ler_devolve_o_mesmo_espaco() {
        let pasta = PastaTemporaria::nova("gravar");
        let espacos = Espacos::new(&pasta.0);
        let caminho = espacos
            .gravar("Produção/diário", "manhã", &espaco(), false)
            .unwrap();
        assert_eq!(caminho, "Produção/diário/manhã");
        assert!(pasta.0.join("espacos/Produção/diário/manhã.json").is_file());
        assert_eq!(espacos.ler(&caminho).unwrap(), espaco());
        assert!(espacos.existe("Produção/diário", "manhã"));
        assert!(!espacos.existe("", "manhã"));
    }

    #[test]
    fn gravar_sobre_um_que_existe_pede_para_substituir() {
        let pasta = PastaTemporaria::nova("substituir");
        let espacos = Espacos::new(&pasta.0);
        espacos.gravar("", "um", &espaco(), false).unwrap();
        assert!(matches!(
            espacos.gravar("", "um", &Espaco::default(), false),
            Err(Erro::JaExiste)
        ));
        assert_eq!(espacos.ler("um").unwrap(), espaco(), "o antigo ficou");
        espacos.gravar("", "um", &Espaco::default(), true).unwrap();
        assert_eq!(espacos.ler("um").unwrap(), Espaco::default());
        let nomes: Vec<String> = fs::read_dir(pasta.0.join("espacos"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(nomes, ["um.json"], "sem sobra do .tmp");
    }

    #[test]
    fn a_lista_tem_pastas_primeiro_e_ordem_alfabetica() {
        let pasta = PastaTemporaria::nova("listar");
        let espacos = Espacos::new(&pasta.0);
        for (dir, nome) in [
            ("", "zebra"),
            ("", "Abelha"),
            ("Beta", "dentro"),
            ("alfa/sub", "fundo"),
        ] {
            espacos.gravar(dir, nome, &espaco(), false).unwrap();
        }
        espacos.criar_pasta("", "vazia").unwrap();
        fs::write(pasta.0.join("espacos/ignorado.txt"), "x").unwrap();
        fs::write(pasta.0.join("espacos/meio.json.tmp"), "x").unwrap();

        let lista = espacos.listar();
        let nomes: Vec<&str> = lista.iter().map(Entrada::nome).collect();
        assert_eq!(nomes, ["alfa", "Beta", "vazia", "Abelha", "zebra"]);
        let Entrada::Pasta { filhos, .. } = &lista[0] else {
            panic!("alfa é pasta");
        };
        assert_eq!(filhos[0].caminho(), "alfa/sub");
        assert_eq!(
            espacos.pastas(),
            ["alfa", "alfa/sub", "Beta", "vazia"],
            "as pastas, em ordem de árvore"
        );
        assert_eq!(espacos.espacos_dentro("alfa"), ["sub/fundo"]);
    }

    #[test]
    fn sem_a_pasta_dos_espacos_a_lista_e_vazia() {
        let pasta = PastaTemporaria::nova("sem-pasta");
        assert!(Espacos::new(&pasta.0).listar().is_empty());
    }

    #[test]
    fn renomear_e_mover_mexem_no_arquivo() {
        let pasta = PastaTemporaria::nova("mover");
        let espacos = Espacos::new(&pasta.0);
        espacos.gravar("", "um", &espaco(), false).unwrap();
        espacos.gravar("", "dois", &espaco(), false).unwrap();
        espacos.criar_pasta("", "caixa").unwrap();

        assert_eq!(
            espacos.renomear("um", "primeiro", false).unwrap(),
            "primeiro"
        );
        assert!(matches!(
            espacos.renomear("primeiro", "dois", false),
            Err(Erro::JaExiste)
        ));
        assert!(matches!(
            espacos.renomear("primeiro", "a/b", false),
            Err(Erro::NomeInvalido)
        ));
        assert_eq!(
            espacos.mover("primeiro", "caixa", false).unwrap(),
            "caixa/primeiro"
        );
        assert!(pasta.0.join("espacos/caixa/primeiro.json").is_file());
        assert!(!pasta.0.join("espacos/primeiro.json").exists());
        assert_eq!(
            espacos.mover("caixa/primeiro", "", false).unwrap(),
            "primeiro"
        );

        // Pastas: renomear leva o conteúdo; ir para dentro de si mesma não pode.
        espacos.mover("primeiro", "caixa", false).unwrap();
        assert_eq!(espacos.renomear("caixa", "gaveta", true).unwrap(), "gaveta");
        assert!(pasta.0.join("espacos/gaveta/primeiro.json").is_file());
        espacos.criar_pasta("gaveta", "interna").unwrap();
        assert!(matches!(
            espacos.mover("gaveta", "gaveta/interna", true),
            Err(Erro::DentroDeSiMesma)
        ));
        assert!(matches!(
            espacos.mover("gaveta", "gaveta", true),
            Err(Erro::DentroDeSiMesma)
        ));
        assert_eq!(
            espacos.mover("gaveta/interna", "", true).unwrap(),
            "interna"
        );
        assert!(matches!(
            espacos.mover("fantasma", "", false),
            Err(Erro::NaoExiste)
        ));
    }

    #[test]
    fn apagar_tira_o_arquivo_e_a_pasta_so_vazia_ou_com_confirmacao() {
        let pasta = PastaTemporaria::nova("apagar");
        let espacos = Espacos::new(&pasta.0);
        espacos.gravar("caixa", "um", &espaco(), false).unwrap();
        espacos.criar_pasta("", "vazia").unwrap();

        assert!(matches!(
            espacos.apagar_pasta("caixa", false),
            Err(Erro::NaoVazia)
        ));
        espacos.apagar_pasta("vazia", false).unwrap();
        assert!(!pasta.0.join("espacos/vazia").exists());
        espacos.apagar("caixa/um").unwrap();
        assert!(!pasta.0.join("espacos/caixa/um.json").exists());
        assert!(matches!(espacos.apagar("caixa/um"), Err(Erro::NaoExiste)));

        espacos.gravar("caixa", "dois", &espaco(), false).unwrap();
        espacos.apagar_pasta("caixa", true).unwrap();
        assert!(!pasta.0.join("espacos/caixa").exists());
        assert!(matches!(
            espacos.apagar_pasta("", true),
            Err(Erro::NomeInvalido) | Err(Erro::NaoExiste)
        ));
    }

    #[test]
    fn arquivo_que_nao_e_um_espaco_da_erro_claro() {
        let pasta = PastaTemporaria::nova("invalido");
        let espacos = Espacos::new(&pasta.0);
        fs::create_dir_all(pasta.0.join("espacos")).unwrap();
        fs::write(pasta.0.join("espacos/ruim.json"), "isto não é json").unwrap();
        let Err(erro) = espacos.ler("ruim") else {
            panic!("devia dar erro");
        };
        assert!(
            erro.to_string()
                .starts_with("O arquivo do espaço não serve")
        );
        assert!(matches!(espacos.ler("nao-tem"), Err(Erro::NaoExiste)));
    }

    #[test]
    fn o_id_vem_do_comeco_da_chave() {
        assert_eq!(id_da_chave("12-3"), Some(12));
        assert_eq!(id_da_chave("0-1"), Some(0));
        assert_eq!(id_da_chave("terminal"), None);
        assert_eq!(id_da_chave(""), None);
    }

    #[test]
    fn id_que_sumiu_e_achado_por_nome_host_e_usuario() {
        let tree = arvore();
        // O id 77 não existe; o nome, o host e o usuário batem com a conexão 1.
        let item = Item {
            id: Some(77),
            nome: "web-01".into(),
            host: "web-01.exemplo".into(),
            user: "deploy".into(),
            tmux: None,
            screen: None,
            assistentes: Vec::new(),
        };
        assert_eq!(resolver_item(&item, &tree), Some(Aba::Conexao(1, None)));
        // O id certo vence, mesmo com o nome mudado.
        let renomeada = Item {
            id: Some(2),
            nome: "outro-nome".into(),
            ..Item::default()
        };
        assert_eq!(
            resolver_item(&renomeada, &tree),
            Some(Aba::Conexao(2, None))
        );
        assert_eq!(resolver_item(&Item::shell_local(), &tree), Some(Aba::Local));
    }

    #[test]
    fn item_sem_correspondencia_e_pulado_e_listado() {
        let tree = arvore();
        let janela = JanelaSalva {
            grade: (2, 1),
            paineis: vec![
                vec![
                    Item {
                        id: Some(1),
                        nome: "web-01".into(),
                        ..Item::default()
                    },
                    Item {
                        id: Some(50),
                        nome: "web-99".into(),
                        host: "x".into(),
                        user: "y".into(),
                        tmux: None,
                        screen: None,
                        assistentes: Vec::new(),
                    },
                ],
                vec![
                    Item::shell_local(),
                    Item {
                        id: Some(51),
                        nome: "db-02".into(),
                        host: "outro-host".into(),
                        user: "dba".into(),
                        tmux: None,
                        screen: None,
                        assistentes: Vec::new(),
                    },
                    // Sem nome, o id que sumiu não tem como ser achado.
                    Item {
                        id: Some(52),
                        ..Item::default()
                    },
                ],
            ],
            ..JanelaSalva::default()
        };
        let r = janela.resolver(&tree);
        assert_eq!(
            r.paineis,
            [
                vec![(Aba::Conexao(1, None), Vec::new())],
                vec![(Aba::Local, Vec::new())]
            ],
            "as que não casam ficam de fora"
        );
        assert_eq!(r.faltando, ["web-99", "db-02", ""]);
    }

    #[test]
    fn o_aviso_conta_e_nomeia_as_conexoes_que_faltam() {
        assert_eq!(aviso_de_faltantes(&[]), None);
        assert_eq!(
            aviso_de_faltantes(&["web-01".into()]).unwrap(),
            "1 conexão deste espaço não existe mais: web-01"
        );
        assert_eq!(
            aviso_de_faltantes(&["web-01".into(), "db-02".into()]).unwrap(),
            "2 conexões deste espaço não existem mais: web-01, db-02"
        );
    }

    #[test]
    fn a_janela_salva_segue_os_paineis_e_a_ordem_das_abas() {
        let tree = arvore();
        let a = Session::new(Loopback::new()).key("1-1");
        let b = Session::new(Loopback::new()).key("2-1");
        let c = Session::new(Loopback::new()).key("1-2");
        let local = Session::new(Loopback::new()).key("0-3");
        let assistente = Session::new(Loopback::new()).key("0-4");
        let avulsa = Session::new(Loopback::new()).key("sem-id");
        let apagada = Session::new(Loopback::new()).key("9-1");
        let mut layout = PaneLayout::with(Vec::new());
        layout.grid(2, 1);
        for s in [a, c, avulsa, apagada] {
            layout.add_tab(s, 0);
        }
        layout.add_tab(b, 1);
        layout.add_tab(local, 1);
        layout.add_tab(assistente, 1);

        // A aba do assistente é um shell local por baixo, mas não entra.
        let janela = JanelaSalva::do_layout((2, 1), &layout, &[assistente], &tree, &|_, i| i);
        let nomes: Vec<Vec<&str>> = janela
            .paineis
            .iter()
            .map(|p| p.iter().map(|i| i.nome.as_str()).collect())
            .collect();
        assert_eq!(
            nomes,
            [vec!["web-01", "web-01"], vec!["db-02", "Shell local"]]
        );
        assert_eq!(janela.paineis[1][1].id, None);
        assert_eq!(janela.paineis[0][0].host, "web-01.exemplo");

        let so_o_segundo = JanelaSalva::do_painel(&layout, 1, &[assistente], &tree, &|_, i| i);
        assert_eq!(so_o_segundo.grade, (1, 1));
        assert_eq!(so_o_segundo.paineis.len(), 1);
        assert_eq!(so_o_segundo.paineis[0], janela.paineis[1]);
        for s in [a, b, c, local, assistente, avulsa, apagada] {
            s.dispose();
        }
    }

    #[test]
    fn rotulo_do_item_mostra_usuario_e_host() {
        let mut item = Item {
            id: Some(1),
            nome: "web".into(),
            host: "h".into(),
            user: "u".into(),
            tmux: None,
            screen: None,
            assistentes: Vec::new(),
        };
        assert_eq!(rotulo_do_item(&item), "web (u@h)");
        item.user.clear();
        assert_eq!(rotulo_do_item(&item), "web (h)");
        item.host.clear();
        assert_eq!(rotulo_do_item(&item), "web");
    }
}
