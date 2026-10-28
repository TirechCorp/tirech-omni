//! O que as páginas "Chaves SSH" e "Agente" e o diálogo de copiar a chave compartilham: a pasta
//! `~/.ssh`, o agente embutido, as configurações e a fila de chaves que esperam a passphrase.
//! Aqui não há widget: as páginas chamam estes métodos, e os testes também.

use std::cell::{Cell, RefCell};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::agent::AgentKey;
use tomni_ui::terminal::ssh::keys::{self, KdfLevel, KeyError, KeyFile};

use crate::abrir::Opener;
use crate::agente::{
    AgenteApp, ChaveSalva, OrigemDaChave, endereco_padrao, guardar_passphrase, item_da_passphrase,
    passphrase_do_cofre,
};
use crate::cofre::CofreCompartilhado;
use crate::configuracoes::{AgenteCfg, Configuracoes};
use crate::sistema::{self, AtalhoDoMenu, EstadoDoInicio, Lugar, LugarDoInicio};
use crate::t;

/// O aviso da casca: a severidade e o texto.
pub type Aviso = Signal<Option<(InfoBarSeverity, String)>>;

/// Arquivos maiores que isto na `~/.ssh` não são chaves.
pub const TAMANHO_MAXIMO_DE_CHAVE: u64 = 64 * 1024;

/// Um arquivo de chave privada da pasta `~/.ssh`, com o que se sabe dele sem a passphrase.
#[derive(Clone, Debug, PartialEq)]
pub struct ChaveLocal {
    /// O caminho do arquivo privado.
    pub arquivo: PathBuf,
    /// O nome do arquivo (`id_ed25519`).
    pub nome: String,
    /// O tipo: `ED25519`, `RSA`…
    pub tipo: String,
    /// O tamanho em bits (0 se não deu para saber).
    pub bits: u32,
    /// O comentário.
    pub comentario: String,
    /// A impressão digital `SHA256:…` (vazia se a pública é desconhecida).
    pub fingerprint: String,
    /// Se há passphrase.
    pub protegida: bool,
    /// Se a chave está no agente embutido.
    pub no_agente: bool,
}

impl ChaveLocal {
    /// A chave que a lista usa para reconhecer a linha: muda com o que a linha mostra.
    pub fn chave(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.arquivo.display(),
            self.fingerprint,
            self.protegida,
            self.no_agente
        )
    }

    /// "Ed25519 256 bits" ou "RSA 3072 bits".
    pub fn descricao(&self) -> String {
        match (self.tipo.as_str(), self.bits) {
            ("", _) => t!("ferramentas-tipo-desconhecido"),
            (tipo, 0) => tipo.to_owned(),
            (tipo, bits) => t!("chave-tipo-bits", tipo = tipo, bits = i64::from(bits)),
        }
    }
}

/// Uma chave que o agente vai receber e que espera a passphrase.
#[derive(Clone, Debug, PartialEq)]
pub struct Carga {
    /// O arquivo da chave.
    pub arquivo: PathBuf,
    /// As restrições com que ela entra no agente.
    pub chave: ChaveSalva,
}

/// Roda um programa do sistema com os argumentos e devolve o que ele escreveu (a saída padrão).
pub type Executor = Rc<dyn Fn(&str, &[&str]) -> io::Result<String>>;

/// O mesmo que o [`Executor`], para quem roda fora da thread da interface (a instalação, M65).
pub type ExecutorEnviavel = Arc<dyn Fn(&str, &[&str]) -> io::Result<String> + Send + Sync>;

/// O que o app toca fora da própria pasta: as pastas da pessoa, o endereço do agente e os
/// programas que ele chama (`systemctl`, `git`…). Os testes dão o seu, para não mexer na máquina.
#[derive(Clone)]
pub struct Ambiente {
    /// A pasta `~/.ssh`.
    pub pasta_ssh: PathBuf,
    /// A pasta pessoal (`~`), onde ficam os arquivos dos shells.
    pub pessoal: Option<PathBuf>,
    /// O endereço do agente quando `agente.socket` está vazio.
    pub socket: PathBuf,
    /// Só para os testes: quando é `Some`, o agente liga neste caminho no lugar do `socket()`
    /// (a página continua mostrando o `socket()`). `None` no app de verdade.
    pub socket_real: Option<PathBuf>,
    /// O `XDG_RUNTIME_DIR`, onde mora o endereço curto do socket quando o pedido não cabe.
    pub xdg_runtime: Option<PathBuf>,
    /// A pasta temporária, para o endereço curto quando não há `XDG_RUNTIME_DIR`.
    pub temporaria: PathBuf,
    /// O nome do usuário, que entra na pasta do endereço curto.
    pub usuario: String,
    /// Se o sistema é o Windows (os testes forçam, para ver a página).
    pub windows: bool,
    /// Se o sistema é o macOS.
    pub macos: bool,
    /// A pasta de dados do modo portable (M66), quando o app roda nele: os padrões que ficariam
    /// no sistema (`known_hosts`, a pasta dos assistentes, os temporários) vão para dentro dela, e
    /// o que escreve no sistema some da tela. `None` fora dele.
    pub portatil: Option<PathBuf>,
    /// Roda `systemctl`, `launchctl`, `git`, `setx`.
    pub executor: Executor,
    /// O mesmo executor, para a instalação no computador (M65), que roda fora da thread da
    /// interface.
    pub executor_enviavel: ExecutorEnviavel,
    /// O `%LOCALAPPDATA%` (Windows), onde fica a cópia instalada (M65).
    pub local_app_data: Option<PathBuf>,
    /// O `%APPDATA%` (Windows), onde o Windows guarda os fixados da barra de tarefas (M65).
    pub app_data: Option<PathBuf>,
    /// A pasta da área de trabalho (a do registro no Windows, a do `user-dirs.dirs` no Linux).
    pub area_de_trabalho: Option<PathBuf>,
    /// A pasta de atalhos do menu Iniciar do Windows (`Programs`).
    pub menu_iniciar: Option<PathBuf>,
    /// O `XDG_CURRENT_DESKTOP`, que diz como fixar no painel (M65).
    pub desktop_atual: String,
    /// A chave do registro (`HKCU\...`) de "Aplicativos instalados" do Windows.
    pub chave_uninstall: String,
    /// Só para os testes visuais: quando não está vazia, o estado de cada item da instalação vem
    /// daqui e não do disco (o `lint` e os snapshots não dependem da máquina). Vazia no app.
    pub instalacao_fixa: Vec<(
        crate::instalacao::ItemDeInstalacao,
        crate::instalacao::EstadoDoItem,
    )>,
    /// O `XDG_CONFIG_HOME`, onde fica a pasta `autostart` do Linux.
    pub xdg_config: Option<PathBuf>,
    /// O `XDG_DATA_HOME`, onde ficam o `.desktop` e os ícones do atalho no menu do Linux.
    pub xdg_data: Option<PathBuf>,
    /// O `XDG_CACHE_HOME`, onde fica a pasta de trabalho padrão dos assistentes no Linux.
    pub xdg_cache: Option<PathBuf>,
    /// O executável do app, que a entrada de início com o sistema abre.
    pub exe: Option<PathBuf>,
    /// O shell de login da pessoa (`$SHELL`), que roda os assistentes do "Controlar com" (M63).
    pub shell: Option<String>,
    /// As pastas onde o Windows procura os assistentes do "Controlar com": o `PATH`,
    /// `~\.local\bin` e `%APPDATA%\npm`. Vazia fora do Windows (lá, o shell de login procura).
    pub pastas_de_programas: Vec<PathBuf>,
    /// A chave do registro (`HKCU\...`) do início com o sistema no Windows.
    pub chave_run: String,
    /// Se o app foi aberto com `--bandeja` (o início com o sistema): começa escondido se
    /// `geral.iniciar_na_bandeja` pedir.
    pub bandeja_ao_abrir: bool,
    /// Aberto com `--desinstalar`: a tela abre o diálogo de instalação no modo desinstalar.
    pub desinstalar_ao_abrir: bool,
    /// Se ao abrir o app reaponta o atalho e o fixado que ficaram apontando para um executável
    /// que sumiu (M65). Desligado com a pasta de dados trocada por variável, como a migração do
    /// nome (testes e `lint`).
    pub repontar_ao_abrir: bool,
    /// Se esta cópia do app é a dona da configuração. O `main` trava o `trava.lock` da pasta de
    /// dados: a primeira cópia aberta fica com ele, e as outras abrem só de leitura, como no
    /// WinSSHTerm: sem as abas da primeira (que já estão abertas nela) e sem gravar nada da pasta
    /// de dados, do cofre e dos espaços ([`crate::armazenamento::somente_leitura_em`]), até
    /// reiniciar depois de a primeira fechar. `true` nos testes.
    pub dono_do_estado: bool,
    /// Quanto esperar até conferir se o sistema tem bandeja (o `--bandeja` e o aviso).
    pub espera_da_bandeja: std::time::Duration,
    /// De quanto em quanto tempo o app confere se o `vault.json` mudou no disco (M44).
    pub vigia_do_cofre: std::time::Duration,
    /// Abre um endereço (`http`, `https`, `mailto`) no programa do sistema: os links das notas.
    pub abridor: Rc<dyn Fn(&str)>,
    /// Abre a URL de login do nó Tailscale (M57). Roda na thread do motor, por isso é `Send`.
    pub abridor_de_login: Arc<dyn Fn(&str) + Send + Sync>,
    /// A pausa depois da última tecla até a nota gravar sozinha (M47).
    pub espera_da_nota: std::time::Duration,
    /// A hora local de agora (a data dos nomes que o colar manda ao servidor, M45). Os testes
    /// fixam uma.
    pub agora: fn() -> jiff::civil::DateTime,
    /// De onde o medidor de CPU e a página Desempenho leem, e quem bate o ritmo. Com a pasta
    /// trocada por variável (os testes, o `lint`, as capturas), nada bate e o medidor fica em
    /// zero, para as imagens não variarem.
    pub medicao: crate::desempenho::Medicao,
}

impl Ambiente {
    /// O ambiente de verdade. Com a pasta de configuração trocada (`TIRECH_OMNI_CONFIG`: os
    /// testes, o `lint`, as capturas de tela) ou no modo portable, o agente escuta num endereço
    /// só dessa pasta e não disputa o endereço com um app aberto.
    pub fn do_sistema(pasta: &Path) -> Self {
        let portatil = crate::armazenamento::modo_portatil().then(|| pasta.to_owned());
        let socket = if crate::armazenamento::variavel_da_pasta().is_some() || portatil.is_some() {
            crate::agente::endereco_isolado(pasta, cfg!(windows))
        } else {
            endereco_padrao()
        };
        let pessoal = crate::armazenamento::pasta_pessoal();
        let xdg_config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        Self {
            pasta_ssh: pasta_ssh_padrao(),
            area_de_trabalho: crate::instalacao::area_de_trabalho_do_sistema(
                cfg!(windows),
                cfg!(target_os = "macos"),
                pessoal.as_deref(),
                xdg_config.as_deref(),
            ),
            menu_iniciar: crate::instalacao::menu_iniciar_do_sistema(
                cfg!(windows),
                pessoal.as_deref(),
            ),
            local_app_data: std::env::var_os("LOCALAPPDATA")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            app_data: std::env::var_os("APPDATA")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            desktop_atual: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
            chave_uninstall: crate::instalacao::CHAVE_UNINSTALL.to_owned(),
            instalacao_fixa: Vec::new(),
            executor_enviavel: Arc::new(executar),
            pessoal,
            socket,
            socket_real: None,
            xdg_runtime: std::env::var_os("XDG_RUNTIME_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
            temporaria: std::env::temp_dir(),
            usuario: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_default(),
            windows: cfg!(windows),
            macos: cfg!(target_os = "macos"),
            executor: Rc::new(executar),
            xdg_config,
            xdg_data: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            xdg_cache: std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from),
            exe: std::env::current_exe().ok(),
            shell: std::env::var("SHELL").ok().filter(|s| !s.is_empty()),
            pastas_de_programas: pastas_de_programas(),
            chave_run: crate::sistema::CHAVE_RUN.to_owned(),
            bandeja_ao_abrir: crate::sistema::aberto_na_bandeja(),
            desinstalar_ao_abrir: crate::instalacao::pedido_de_desinstalar(),
            repontar_ao_abrir: crate::armazenamento::variavel_da_pasta().is_none()
                && portatil.is_none(),
            portatil,
            dono_do_estado: true,
            espera_da_bandeja: crate::sistema::ESPERA_DA_BANDEJA,
            vigia_do_cofre: std::time::Duration::from_secs(5),
            abridor: Rc::new(|endereco| {
                let _ = abrir_endereco(endereco);
            }),
            abridor_de_login: Arc::new(|endereco| {
                let _ = abrir_endereco(endereco);
            }),
            espera_da_nota: crate::tela_notas::ESPERA_DA_NOTA,
            agora: crate::colar::agora,
            medicao: if crate::armazenamento::variavel_da_pasta().is_some() {
                crate::desempenho::Medicao::parada()
            } else {
                crate::desempenho::Medicao::do_sistema()
            },
        }
    }

    /// Um ambiente de teste: a pasta pessoal é a de cima de `pasta_ssh`, o socket fica na pasta
    /// do app e o executor não roda nada.
    pub fn de_teste(pasta: &Path, pasta_ssh: PathBuf) -> Self {
        Self {
            pessoal: pasta_ssh.parent().map(Path::to_owned),
            pasta_ssh,
            socket: pasta.join("agent.sock"),
            socket_real: None,
            xdg_runtime: None,
            temporaria: std::env::temp_dir(),
            usuario: "teste".to_owned(),
            windows: false,
            macos: false,
            executor: Rc::new(|_, _| Ok(String::new())),
            executor_enviavel: Arc::new(|_, _| Ok(String::new())),
            local_app_data: None,
            app_data: None,
            area_de_trabalho: None,
            menu_iniciar: None,
            desktop_atual: String::new(),
            chave_uninstall: "Software\\TirechOmniTeste\\Uninstall".to_owned(),
            instalacao_fixa: Vec::new(),
            xdg_config: None,
            xdg_data: None,
            xdg_cache: None,
            exe: Some(PathBuf::from("/opt/tirech-omni/tirech-omni")),
            shell: Some("/bin/sh".to_owned()),
            pastas_de_programas: Vec::new(),
            chave_run: crate::sistema::CHAVE_RUN.to_owned(),
            bandeja_ao_abrir: false,
            desinstalar_ao_abrir: false,
            repontar_ao_abrir: false,
            portatil: None,
            dono_do_estado: true,
            espera_da_bandeja: std::time::Duration::from_millis(100),
            vigia_do_cofre: std::time::Duration::from_secs(5),
            abridor: Rc::new(|_| {}),
            abridor_de_login: Arc::new(|_| {}),
            espera_da_nota: std::time::Duration::from_millis(150),
            agora: crate::colar::agora,
            medicao: crate::desempenho::Medicao::de_teste(
                Rc::new(crate::desempenho::FonteFixa::nova()),
                tomni_ui::files::Live::detached(0),
            ),
        }
    }

    /// A pasta de trabalho dos assistentes do "Controlar com" quando `ia.local_pasta` está vazia:
    /// `XDG_CACHE_HOME/tirech-omni/assistentes` (ou `~/.cache/…`) no Linux,
    /// `~/Library/Caches/tirech-omni/assistentes` no macOS e
    /// `%LOCALAPPDATA%\Tirech Omni\assistentes` no Windows. Fica longe dos arquivos da pessoa e
    /// num lugar que o sistema trata como descartável. No modo portable, `assistentes` na pasta
    /// de dados.
    pub fn pasta_dos_assistentes_padrao(&self) -> Option<PathBuf> {
        if let Some(dados) = &self.portatil {
            return Some(dados.join("assistentes"));
        }
        let pessoal = self.pessoal.as_deref();
        if self.windows {
            let local = self
                .local_app_data
                .clone()
                .or_else(|| Some(pessoal?.join("AppData").join("Local")))?;
            return Some(local.join("Tirech Omni").join("assistentes"));
        }
        let cache = if self.macos {
            pessoal?.join("Library").join("Caches")
        } else {
            match self.xdg_cache.as_deref().filter(|p| p.is_absolute()) {
                Some(cache) => cache.to_owned(),
                None => pessoal?.join(".cache"),
            }
        };
        Some(cache.join("tirech-omni").join("assistentes"))
    }

    /// Onde o app guarda as cópias que são só de passagem (a edição de arquivos remotos): a
    /// temporária do sistema, ou `temp` na pasta de dados no modo portable, para não deixar
    /// nada no computador.
    pub fn pasta_temporaria(&self) -> PathBuf {
        match &self.portatil {
            Some(dados) => dados.join("temp"),
            None => std::env::temp_dir(),
        }
    }

    /// A pasta de trabalho dos assistentes: a `escolhida` nas Configurações (`~` vale), ou a
    /// [padrão](Self::pasta_dos_assistentes_padrao) quando ela está vazia.
    pub fn pasta_dos_assistentes(&self, escolhida: &str) -> Option<PathBuf> {
        let escolhida = escolhida.trim();
        if escolhida.is_empty() {
            self.pasta_dos_assistentes_padrao()
        } else {
            Some(crate::armazenamento::expandir_til_em(
                escolhida,
                self.pessoal.as_deref(),
            ))
        }
    }
}

/// Se o "Limpar" pode esvaziar `pasta`: nunca a pasta pessoal nem a raiz de um disco.
pub fn pode_esvaziar(pasta: &Path, pessoal: Option<&Path>) -> bool {
    pasta.parent().is_some() && pessoal != Some(pasta)
}

/// Quantos itens `pasta` tem no primeiro nível (0 quando ela não existe).
pub fn itens_da_pasta(pasta: &Path) -> usize {
    fs::read_dir(pasta).map_or(0, |entradas| entradas.flatten().count())
}

/// Apaga tudo o que está dentro de `pasta`, sem apagar a própria. Os links saem sem ser seguidos.
/// Devolve quantos itens do primeiro nível saíram e o primeiro erro, se algum não saiu (um
/// arquivo aberto no Windows, por exemplo): os outros saem mesmo assim.
pub fn esvaziar_pasta(pasta: &Path) -> (usize, Option<io::Error>) {
    let entradas = match fs::read_dir(pasta) {
        Ok(entradas) => entradas,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (0, None),
        Err(e) => return (0, Some(e)),
    };
    let (mut apagados, mut primeiro_erro) = (0, None);
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        let apagou = match entrada.file_type() {
            Ok(tipo) if tipo.is_dir() => fs::remove_dir_all(&caminho),
            // No Windows, um link para pasta sai pelo `remove_dir`.
            Ok(_) => fs::remove_file(&caminho).or_else(|_| fs::remove_dir(&caminho)),
            Err(e) => Err(e),
        };
        match apagou {
            Ok(()) => apagados += 1,
            Err(e) => {
                primeiro_erro.get_or_insert(e);
            }
        }
    }
    (apagados, primeiro_erro)
}

/// Onde o Windows procura os assistentes do "Controlar com" (M63): o `PATH`, a pasta do
/// instalador nativo (`~\.local\bin`) e a do npm (`%APPDATA%\npm`). Fora do Windows é vazia.
fn pastas_de_programas() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut pastas: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(pessoal) = crate::armazenamento::pasta_pessoal() {
        pastas.push(pessoal.join(".local").join("bin"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        pastas.push(PathBuf::from(appdata).join("npm"));
    }
    pastas
}

/// O executor de verdade: roda o programa, espera e devolve a saída. Status de erro vira `Err`
/// com o que o programa escreveu.
pub(crate) fn executar(programa: &str, argumentos: &[&str]) -> io::Result<String> {
    let mut comando = std::process::Command::new(programa);
    let saida = sistema::sem_janela(&mut comando)
        .args(argumentos)
        .output()?;
    if saida.status.success() {
        Ok(String::from_utf8_lossy(&saida.stdout).into_owned())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&saida.stderr).trim().to_owned(),
        ))
    }
}

/// O que o app já escreveu no sistema nesta sessão (para só desfazer o que ele fez).
#[derive(Default)]
struct Aplicado {
    ssh_config: bool,
    shells: bool,
    /// O socket que está no ambiente da sessão gráfica.
    sessao: Option<String>,
    git: bool,
    /// O pipe que está no `SSH_AUTH_SOCK` do usuário (Windows).
    ambiente_do_usuario: Option<String>,
}

/// O valor do `core.sshCommand` que o interruptor do Git põe.
pub const SSH_DO_WINDOWS: &str = "C:/Windows/System32/OpenSSH/ssh.exe";

/// O estado compartilhado das ferramentas de chaves.
pub struct Ferramentas {
    cofre: CofreCompartilhado,
    opener: Rc<dyn Opener>,
    aviso: Aviso,
    /// O agente embutido.
    pub agente: Rc<AgenteApp>,
    /// As configurações do app; o agente usa a categoria `agente`, e quem grava o arquivo é o
    /// efeito da tela.
    pub config: Signal<Configuracoes>,
    /// A pasta das chaves quando `agente.pasta_chaves` está vazia (`~/.ssh`).
    pasta_ssh_base: PathBuf,
    /// As chaves à espera da passphrase, a da frente é a que o diálogo mostra.
    pub cargas: Signal<Vec<Carga>>,
    /// Sobe quando a lista da pasta `~/.ssh` precisa ser relida.
    pub pasta_mudou: Signal<u64>,
    /// As chaves de `agente.chaves` que precisam do cofre aberto para abrir.
    adiadas: RefCell<Vec<ChaveSalva>>,
    ambiente: Ambiente,
    aplicado: RefCell<Aplicado>,
    backup_feito: Cell<bool>,
}

impl Ferramentas {
    /// Prepara o agente com as configurações `config` (ainda parado: [`Ferramentas::iniciar`] o
    /// liga).
    pub fn new(
        config: Signal<Configuracoes>,
        cofre: CofreCompartilhado,
        opener: Rc<dyn Opener>,
        aviso: Aviso,
        ambiente: Ambiente,
    ) -> Rc<Self> {
        Rc::new(Self {
            cofre,
            opener,
            aviso,
            agente: Rc::new(AgenteApp::new()),
            config,
            pasta_ssh_base: ambiente.pasta_ssh.clone(),
            ambiente,
            cargas: signal(Vec::new()),
            pasta_mudou: signal(0),
            adiadas: RefCell::new(Vec::new()),
            aplicado: RefCell::default(),
            backup_feito: Cell::new(false),
        })
    }

    pub fn avisar(&self, severidade: InfoBarSeverity, mensagem: impl Into<String>) {
        self.aviso.set(Some((severidade, mensagem.into())));
    }

    /// Mostra um aviso de erro na barra da casca.
    pub fn avisar_erro(&self, mensagem: impl Into<String>) {
        self.avisar(InfoBarSeverity::Error, mensagem);
    }

    /// Mostra um aviso de sucesso na barra da casca.
    pub fn avisar_sucesso(&self, mensagem: impl Into<String>) {
        self.avisar(InfoBarSeverity::Success, mensagem);
    }

    /// O cofre de senhas da tela.
    pub fn cofre(&self) -> &CofreCompartilhado {
        &self.cofre
    }

    /// O ambiente em que o app roda.
    pub fn ambiente(&self) -> &Ambiente {
        &self.ambiente
    }

    /// O socket do agente embutido: `agente.socket` ou, vazio, o endereço padrão do sistema.
    ///
    /// Fora do Windows, um endereço que passa do tamanho que o sistema aceita para um socket vira
    /// um curto e estável ([`Ferramentas::socket_pedido`] guarda o que foi pedido).
    pub fn socket(&self) -> PathBuf {
        let pedido = self.socket_pedido();
        if self.ambiente.windows {
            return pedido;
        }
        let a = &self.ambiente;
        crate::agente::endereco_que_cabe(
            &pedido,
            a.macos,
            a.xdg_runtime.as_deref(),
            &a.temporaria,
            &a.usuario,
        )
        .unwrap_or(pedido)
    }

    /// O endereço que a configuração ou o ambiente pede, antes de encurtar.
    pub fn socket_pedido(&self) -> PathBuf {
        let escolhido = self.cfg().socket;
        if escolhido.trim().is_empty() {
            self.ambiente.socket.clone()
        } else {
            PathBuf::from(escolhido.trim())
        }
    }

    /// Se o endereço que vale é um encurtado do que foi pedido.
    pub fn socket_encurtado(&self) -> bool {
        self.socket() != self.socket_pedido()
    }

    /// A pasta das chaves: `agente.pasta_chaves` ou, vazia, `~/.ssh`. Lê sem rastrear.
    pub fn pasta_ssh(&self) -> PathBuf {
        let escolhida = self
            .config
            .with_untracked(|c| c.agente.pasta_chaves.clone());
        let escolhida = escolhida.trim();
        if escolhida.is_empty() {
            self.pasta_ssh_base.clone()
        } else {
            PathBuf::from(escolhida)
        }
    }

    /// Uma chave sem restrições próprias: só vale o `agente.confirmar_padrao` (a validade
    /// padrão é do agente).
    pub fn chave_nova(&self, arquivo: impl Into<PathBuf>) -> ChaveSalva {
        ChaveSalva {
            confirmar: self.cfg().confirmar_padrao,
            ..ChaveSalva::new(arquivo)
        }
    }

    /// A categoria `agente` das configurações, sem rastrear.
    pub fn cfg(&self) -> AgenteCfg {
        self.config.with_untracked(|c| c.agente.clone())
    }

    fn mudar_cfg(&self, f: impl FnOnce(&mut AgenteCfg)) {
        self.config.update(|c| f(&mut c.agente));
    }

    /// Liga o agente se `agente.ligado` manda, diz ao `Opener` onde ele está e carrega as chaves
    /// que não precisam de passphrase. As outras esperam o cofre ([`Ferramentas::carregar_adiadas`]).
    pub fn iniciar(&self) {
        let config = self.cfg();
        if config.ligado {
            self.ligar_agente();
        } else {
            self.opener.definir_agente(None);
        }
        self.aplicar_sistema();
        if !self.agente.rodando() {
            return;
        }
        self.sincronizar_cofre();
        if !config.carregar_ao_iniciar {
            return;
        }
        for chave in config.chaves {
            match self.tentar_carregar(&chave.arquivo.clone(), &chave, None, false) {
                Ok(_) => {}
                Err(KeyError::NeedsPassphrase | KeyError::WrongPassphrase) => {
                    self.adiadas.borrow_mut().push(chave);
                }
                Err(erro) => self.falhou_ao_carregar(&chave.arquivo, &erro),
            }
        }
    }

    /// Passa as chaves adiadas pelo cofre; as que ainda pedem passphrase entram na fila do
    /// diálogo.
    pub fn carregar_adiadas(&self) {
        let adiadas = std::mem::take(&mut *self.adiadas.borrow_mut());
        for chave in adiadas {
            self.pedir_ou_carregar(&chave.arquivo.clone(), chave, false);
        }
    }

    /// Sobe o agente embutido e o oferece às conexões. Se não sobe, avisa e fica com o do
    /// sistema.
    pub fn ligar_agente(&self) {
        let config = self.cfg();
        let onde = self
            .ambiente
            .socket_real
            .clone()
            .unwrap_or_else(|| self.socket());
        match self.agente.ligar(&onde, &config) {
            Ok(()) if self.agente.outro_app() => {
                // Outro Tirech Omni atende o endereço: as conexões usam as chaves dele.
                self.opener.definir_agente(Some(self.socket()));
            }
            Ok(()) => {
                self.opener.definir_agente(self.agente.caminho());
                self.agente
                    .definir_pageant(config.pageant && self.ambiente.windows);
            }
            Err(mensagem) => {
                self.opener.definir_agente(None);
                self.avisar(InfoBarSeverity::Warning, mensagem);
            }
        }
    }

    /// Desliga o agente embutido: as conexões voltam ao agente do sistema.
    pub fn desligar_agente(&self) {
        self.agente.desligar();
        self.opener.definir_agente(None);
        self.aplicar_sistema();
    }

    /// Para o que o app deixou no sistema ao sair: o `SSH_AUTH_SOCK` da sessão gráfica e o do
    /// usuário no Windows. Os blocos dos arquivos ficam, porque se apagam sozinhos de efeito
    /// (o socket some com o app).
    pub fn encerrar(&self) {
        self.desfazer_ambiente();
    }

    /// Liga ou desliga o agente e grava a escolha nas configurações.
    pub fn definir_ligado(&self, ligado: bool) {
        if self.cfg().ligado == ligado && self.agente.rodando() == ligado {
            return;
        }
        self.mudar_cfg(|c| c.ligado = ligado);
        if ligado {
            self.iniciar();
        } else {
            self.desligar_agente();
        }
    }

    /// Onde fica a entrada de início com o sistema e o executável que ela abre.
    fn lugar_do_inicio(&self) -> Option<(LugarDoInicio, PathBuf)> {
        let a = &self.ambiente;
        let lugar = LugarDoInicio::do_sistema(
            a.windows,
            a.macos,
            a.xdg_config.as_deref(),
            a.pessoal.as_deref(),
            &a.chave_run,
        )?;
        Some((lugar, crate::instalacao::exe_alvo(a)?))
    }

    /// Onde ficam o `.desktop` e os ícones do atalho no menu, e o executável que ele abre. Só no
    /// Linux, e só com a pasta pessoal conhecida.
    fn lugar_do_atalho(&self) -> Option<(AtalhoDoMenu, PathBuf)> {
        let a = &self.ambiente;
        let atalho = AtalhoDoMenu::do_sistema(
            a.windows,
            a.macos,
            a.xdg_data.as_deref(),
            a.pessoal.as_deref(),
        )?;
        Some((atalho, crate::instalacao::exe_alvo(a)?))
    }

    /// Liga ou desliga o atalho no menu e no dock (`geral.atalho_no_menu`). Ligado, grava o que
    /// falta ou mudou (o executável pode ter mudado de lugar); desligado na abertura, não mexe
    /// no disco (o `lint` e o desenvolvimento não apagam o atalho real); desligado depois de
    /// ligado, apaga o que o app gravou.
    pub fn aplicar_atalho_no_menu(&self, ligado: bool, mudou: bool) {
        // O modo portable não escreve no sistema (M66).
        if self.ambiente.portatil.is_some() {
            return;
        }
        let Some((atalho, exe)) = self.lugar_do_atalho() else {
            return;
        };
        let resultado = match (ligado, mudou) {
            (true, _) => atalho.instalar(&exe),
            (false, true) => atalho.remover(),
            (false, false) => return,
        };
        if let Err(erro) = resultado {
            self.avisar(
                InfoBarSeverity::Warning,
                t!("ferramentas-nao-mudou-atalho", erro = erro.to_string()),
            );
        }
    }

    /// Liga ou desliga o início com o sistema (`geral.iniciar_com_sistema`). Ligado, a entrada
    /// sempre aponta para o executável de agora: se o app foi movido, ela é regravada. Desligado
    /// na abertura, não mexe no disco (o `lint` e o desenvolvimento não apagam a entrada real);
    /// desligado depois de ligado, apaga.
    pub fn aplicar_inicio_com_sistema(&self, ligado: bool, mudou: bool) {
        // O modo portable não escreve no sistema (M66).
        if self.ambiente.portatil.is_some() {
            return;
        }
        let Some((lugar, exe)) = self.lugar_do_inicio() else {
            return;
        };
        let estado = lugar.estado(&exe);
        let resultado = match (ligado, estado) {
            (true, EstadoDoInicio::Ligado) | (false, EstadoDoInicio::Desligado) => return,
            (true, _) => lugar.ligar(&exe),
            (false, _) if mudou => lugar.desligar(),
            (false, _) => return,
        };
        if let Err(erro) = resultado {
            self.avisar(
                InfoBarSeverity::Warning,
                t!("ferramentas-nao-mudou-inicio", erro = erro.to_string()),
            );
        }
    }

    /// Muda o tempo de vida padrão das chaves novas.
    pub fn definir_vida_padrao(&self, segundos: u64) {
        self.mudar_cfg(|c| c.vida_padrao_secs = segundos);
        self.agente.vida_padrao(segundos);
    }

    /// As chaves privadas da pasta `~/.ssh`, em ordem de nome.
    pub fn listar_chaves(&self) -> Vec<ChaveLocal> {
        let Ok(entradas) = fs::read_dir(self.pasta_ssh()) else {
            return Vec::new();
        };
        let mut chaves: Vec<ChaveLocal> = entradas
            .filter_map(Result::ok)
            .filter_map(|e| {
                let arquivo = e.path();
                let nome = e.file_name().to_string_lossy().into_owned();
                let ignorado = nome.ends_with(".pub")
                    || nome.ends_with(".bak")
                    || nome.ends_with(".novo")
                    || nome.starts_with("known_hosts")
                    || nome.starts_with("authorized_keys")
                    || nome == "config"
                    || nome.ends_with(".tirech-bak")
                    || nome.ends_with(".sock");
                let metadados = e.metadata().ok()?;
                if ignorado || !metadados.is_file() || metadados.len() > TAMANHO_MAXIMO_DE_CHAVE {
                    return None;
                }
                let info = KeyFile::inspect(&arquivo).ok()?;
                let fingerprint = info.fingerprint().unwrap_or_default();
                Some(ChaveLocal {
                    no_agente: !fingerprint.is_empty() && self.agente.tem(&fingerprint),
                    arquivo,
                    nome,
                    tipo: info.kind,
                    bits: info.bits,
                    comentario: info.comment,
                    fingerprint,
                    protegida: info.encrypted,
                })
            })
            .collect();
        chaves.sort_by(|a, b| a.nome.cmp(&b.nome));
        chaves
    }

    /// Abre o arquivo e põe a chave no agente. Se deu, lembra de onde ela veio e a guarda no
    /// `agente.chaves` das configurações (se `lembrar`), para voltar no próximo início.
    fn tentar_carregar(
        &self,
        arquivo: &Path,
        chave: &ChaveSalva,
        passphrase: Option<&str>,
        lembrar: bool,
    ) -> Result<String, KeyError> {
        let do_cofre;
        let passphrase = match passphrase {
            Some(p) => Some(p),
            None => {
                do_cofre = passphrase_do_cofre(&self.cofre, arquivo);
                do_cofre.as_deref()
            }
        };
        let fingerprint = self.agente.carregar(arquivo, passphrase, chave)?;
        self.agente
            .registrar_origem(&fingerprint, OrigemDaChave::Arquivo(arquivo.to_owned()));
        if lembrar {
            self.mudar_cfg(|c| c.guardar(chave.clone()));
        }
        Ok(fingerprint)
    }

    fn falhou_ao_carregar(&self, arquivo: &Path, erro: &KeyError) {
        self.avisar(
            InfoBarSeverity::Error,
            t!(
                "ferramentas-nao-carregou-no-agente",
                arquivo = arquivo.display().to_string(),
                erro = erro.to_string()
            ),
        );
    }

    /// Carrega a chave no agente com a passphrase do cofre. Se ela pede uma que o cofre não
    /// tem, entra na fila do diálogo. `avisar` conta o sucesso na tela.
    pub fn pedir_ou_carregar(&self, arquivo: &Path, chave: ChaveSalva, avisar: bool) {
        if !self.agente.rodando() {
            self.avisar(
                InfoBarSeverity::Warning,
                t!("ferramentas-agente-desligado-aviso"),
            );
            return;
        }
        match self.tentar_carregar(arquivo, &chave, None, true) {
            Ok(_) if avisar => self.avisar(
                InfoBarSeverity::Success,
                t!("ferramentas-chave-carregada", nome = nome_do(arquivo)),
            ),
            Ok(_) => {}
            Err(KeyError::NeedsPassphrase | KeyError::WrongPassphrase) => {
                let carga = Carga {
                    arquivo: arquivo.to_owned(),
                    chave,
                };
                self.cargas.update(|c| {
                    if !c.contains(&carga) {
                        c.push(carga);
                    }
                });
            }
            Err(erro) => self.falhou_ao_carregar(arquivo, &erro),
        }
    }

    /// Carrega a chave com a passphrase digitada (o diálogo "Adicionar arquivo…"). Com
    /// `salvar`, ela vai para o cofre. Devolve o erro para o diálogo mostrar.
    pub fn carregar_com(
        &self,
        arquivo: &Path,
        chave: ChaveSalva,
        passphrase: &str,
        salvar: bool,
    ) -> Result<(), KeyError> {
        if !self.agente.rodando() {
            return Err(KeyError::Invalid(t!("ferramentas-agente-desligado")));
        }
        self.tentar_carregar(arquivo, &chave, Some(passphrase), true)?;
        if salvar && !passphrase.is_empty() {
            self.guardar_no_cofre(arquivo, passphrase);
        }
        Ok(())
    }

    /// O diálogo de passphrase confirmou: tenta com a que veio. Devolve `false` se a passphrase
    /// não serviu e o diálogo deve voltar com o erro. Qualquer outro resultado tira a chave da
    /// fila.
    pub fn concluir_carga(&self, passphrase: &str, salvar: bool) -> bool {
        let Some(carga) = self.cargas.with_untracked(|c| c.first().cloned()) else {
            return true;
        };
        match self.tentar_carregar(&carga.arquivo, &carga.chave, Some(passphrase), true) {
            Ok(_) => {
                if salvar && !passphrase.is_empty() {
                    self.guardar_no_cofre(&carga.arquivo, passphrase);
                }
            }
            Err(KeyError::WrongPassphrase) => return false,
            Err(erro) => self.falhou_ao_carregar(&carga.arquivo, &erro),
        }
        self.cargas.update(|c| {
            if !c.is_empty() {
                c.remove(0);
            }
        });
        true
    }

    /// O diálogo de passphrase foi cancelado: a chave fica fora do agente.
    pub fn cancelar_carga(&self) {
        self.cargas.update(|c| {
            if !c.is_empty() {
                c.remove(0);
            }
        });
    }

    /// Guarda a passphrase no cofre (se ele está aberto) e avisa se falhar.
    pub fn guardar_no_cofre(&self, arquivo: &Path, passphrase: &str) {
        if self.cofre.agora(|c| c.bloqueado()) {
            self.avisar(
                InfoBarSeverity::Warning,
                t!("ferramentas-passphrase-nao-salva"),
            );
            return;
        }
        if let Err(erro) = guardar_passphrase(&self.cofre, arquivo, passphrase) {
            self.avisar(
                InfoBarSeverity::Error,
                t!("ferramentas-nao-salvou-passphrase", erro = erro.to_string()),
            );
        }
    }

    /// Tira do cofre a passphrase guardada da chave, se há.
    pub fn esquecer_passphrase(&self, arquivo: &Path) {
        if self.cofre.agora(|c| c.bloqueado()) {
            return;
        }
        if let Some(id) = item_da_passphrase(&self.cofre, arquivo) {
            let _ = self.cofre.mudar(|c| c.remover(id));
        }
    }

    /// Tira a chave do agente e das configurações (ela não volta no próximo início).
    pub fn remover(&self, chave: &AgentKey) {
        let fingerprint = keys::fingerprint(&chave.public);
        let arquivo = self.agente.esquecer_arquivo_de(&fingerprint);
        self.agente.remover(chave);
        if let Some(arquivo) = arquivo {
            self.mudar_cfg(|c| {
                c.esquecer(&arquivo);
            });
        }
    }

    /// Tira todas as chaves do agente e das configurações.
    pub fn remover_todas(&self) {
        self.agente.remover_todas();
        if !self.cfg().chaves.is_empty() {
            self.mudar_cfg(|c| c.chaves.clear());
        }
    }

    /// Põe no agente uma chave recém-gerada (já aberta) e a guarda nas configurações.
    pub fn carregar_gerada(
        &self,
        arquivo: &Path,
        privada: ssh_key::PrivateKey,
        comentario: &str,
    ) -> Result<(), KeyError> {
        let chave = self.chave_nova(arquivo);
        let fingerprint = keys::fingerprint(privada.public_key());
        self.agente.guardar(privada, comentario, &chave)?;
        self.agente
            .registrar_origem(&fingerprint, OrigemDaChave::Arquivo(arquivo.to_owned()));
        self.mudar_cfg(|c| c.guardar(chave));
        Ok(())
    }

    /// Troca (ou tira, com `nova` vazia) a passphrase do arquivo. `salvar` guarda a nova no
    /// cofre; sem ele, a que o cofre tinha é esquecida, porque deixou de valer.
    pub fn trocar_passphrase(
        &self,
        arquivo: &Path,
        atual: &str,
        nova: &str,
        repetida: &str,
        salvar: bool,
    ) -> Result<(), String> {
        if nova != repetida {
            return Err(t!("chave-passphrases-nao-conferem"));
        }
        let aberta =
            KeyFile::open(arquivo, (!atual.is_empty()).then_some(atual)).map_err(|e| motivo(&e))?;
        aberta
            .rewrite(
                arquivo,
                (!nova.is_empty()).then_some(nova),
                KdfLevel::Standard,
            )
            .map_err(|e| motivo(&e))?;
        if salvar && !nova.is_empty() {
            self.guardar_no_cofre(arquivo, nova);
        } else {
            self.esquecer_passphrase(arquivo);
        }
        self.atualizar_pasta();
        Ok(())
    }

    /// Manda reler a lista da pasta `~/.ssh`.
    pub fn atualizar_pasta(&self) {
        self.pasta_mudou.update(|v| *v += 1);
    }

    /// O arquivo de onde veio a chave do agente, se foi o app que a carregou.
    pub fn arquivo_de(&self, chave: &AgentKey) -> Option<PathBuf> {
        self.agente.arquivo_de(chave)
    }
}

/// Os arquivos de shell que levam o bloco: o bash e o zsh só se existem; o fish é criado.
pub(crate) const ARQUIVOS_POSIX: [&str; 2] = [".bashrc", ".zshrc"];

impl Ferramentas {
    /// Põe as chaves do cofre marcadas "No agente" no agente e tira as que deixaram de estar
    /// (ver [`AgenteApp::carregar_do_cofre`]). Idempotente: o efeito da tela o chama a cada
    /// mudança do cofre.
    pub fn sincronizar_cofre(&self) {
        self.agente.definir_known_hosts(self.known_hosts());
        self.agente
            .carregar_do_cofre(&self.cofre, self.cfg().tirar_ao_bloquear);
    }

    /// Onde procurar o nome do servidor de um pedido de uso: o `known_hosts` da pessoa e o
    /// escolhido nas configurações.
    fn known_hosts(&self) -> Vec<PathBuf> {
        let mut arquivos = vec![self.pasta_ssh_base.join("known_hosts")];
        let escolhido = self
            .config
            .with_untracked(|c| c.conexoes.known_hosts.trim().to_owned());
        if !escolhido.is_empty() {
            arquivos.push(PathBuf::from(escolhido));
        }
        arquivos
    }

    /// O arquivo `config` do `ssh`.
    pub fn arquivo_ssh_config(&self) -> PathBuf {
        self.pasta_ssh_base.join("config")
    }

    /// Os arquivos de shell que levam o bloco `SSH_AUTH_SOCK` (os que existem, mais o do fish).
    pub fn arquivos_de_shell(&self) -> Vec<(PathBuf, bool)> {
        let Some(casa) = self.ambiente.pessoal.as_ref() else {
            return Vec::new();
        };
        let mut arquivos: Vec<(PathBuf, bool)> = ARQUIVOS_POSIX
            .iter()
            .map(|nome| (casa.join(nome), false))
            .collect();
        arquivos.push((
            casa.join(".config")
                .join("fish")
                .join("conf.d")
                .join("tirech-omni.fish"),
            true,
        ));
        arquivos
    }

    /// O socket que o `ssh` do sistema deve usar: o do agente que atende, ou o endereço
    /// configurado.
    pub fn socket_do_sistema(&self) -> PathBuf {
        self.socket()
    }

    /// O que as opções "Programas do sistema" escrevem, para a pessoa conferir antes: cada
    /// arquivo e o texto.
    pub fn texto_do_bloco(&self) -> String {
        let socket = self.socket_do_sistema();
        let mut texto = format!(
            "{}\n\n{}\n",
            self.arquivo_ssh_config().display(),
            sistema::bloco_ssh_config(&socket)
        );
        for (arquivo, fish) in self.arquivos_de_shell() {
            let bloco = if fish {
                sistema::bloco_shell_fish(&socket)
            } else {
                sistema::bloco_shell_posix(&socket)
            };
            texto.push_str(&format!("\n{}\n\n{bloco}\n", arquivo.display()));
        }
        texto
    }

    /// Se o `ssh` do sistema usa o agente do app agora: no Unix, o bloco do `~/.ssh/config`
    /// aponta para o socket que o agente atende; no Windows, o agente está no pipe do OpenSSH.
    pub fn sistema_usa_o_app(&self) -> bool {
        if !self.agente.rodando() && !self.agente.outro_app() {
            return false;
        }
        if self.ambiente.windows {
            return !self.agente.servico_ocupa();
        }
        sistema::bloco_do_arquivo(&self.arquivo_ssh_config())
            .is_some_and(|socket| socket == self.socket_do_sistema())
    }

    /// Escreve (ou tira) o que as opções de `agente.sistema_*` pedem. Idempotente: o efeito da
    /// tela o chama a cada mudança das opções e do agente. Erros viram aviso; uma opção cujo
    /// programa não existe (`systemctl`, `git`) é desligada com a explicação.
    pub fn aplicar_sistema(&self) {
        // O modo portable não escreve no sistema (M66).
        if self.ambiente.portatil.is_some() {
            return;
        }
        let config = self.cfg();
        if self.ambiente.windows {
            self.aplicar_no_windows(&config);
        } else {
            self.aplicar_no_unix(&config);
        }
    }

    fn aplicar_no_unix(&self, config: &AgenteCfg) {
        let socket = self.socket_do_sistema();
        let bloco = sistema::bloco_ssh_config(&socket);
        self.gravar_bloco(
            &self.arquivo_ssh_config(),
            config.sistema_ssh_config.then_some(bloco.as_str()),
            Lugar::Topo,
            true,
            |a| &mut a.ssh_config,
        );
        // Os blocos dos shells.
        let ligado = config.sistema_shells;
        let estava = self.aplicado.borrow().shells;
        if ligado || estava {
            for (arquivo, fish) in self.arquivos_de_shell() {
                let bloco = if fish {
                    sistema::bloco_shell_fish(&socket)
                } else {
                    sistema::bloco_shell_posix(&socket)
                };
                let existe = arquivo.is_file();
                let resultado = sistema::aplicar_bloco(
                    &arquivo,
                    ligado.then_some(bloco.as_str()),
                    Lugar::Fim,
                    fish,
                    None,
                );
                if let Err(erro) = resultado {
                    if ligado {
                        self.avisar_erro(t!(
                            "ferramentas-nao-consegui-escrever",
                            arquivo = arquivo.display().to_string(),
                            erro = erro.to_string()
                        ));
                    }
                    continue;
                }
                // O arquivo do fish é só nosso: sem o bloco, não sobra um arquivo vazio.
                if fish
                    && !ligado
                    && existe
                    && fs::read_to_string(&arquivo).is_ok_and(|t| t.trim().is_empty())
                {
                    let _ = fs::remove_file(&arquivo);
                }
            }
            self.aplicado.borrow_mut().shells = ligado;
        }
        // A sessão gráfica.
        let desejado = (config.sistema_sessao
            && (self.agente.rodando() || self.agente.outro_app()))
        .then(|| socket.display().to_string());
        let atual = self.aplicado.borrow().sessao.clone();
        if desejado == atual {
            return;
        }
        let programa = if self.ambiente.macos {
            "launchctl"
        } else {
            "systemctl"
        };
        let resultado = match &desejado {
            Some(socket) if self.ambiente.macos => {
                (self.ambiente.executor)(programa, &["setenv", "SSH_AUTH_SOCK", socket])
            }
            Some(socket) => (self.ambiente.executor)(
                programa,
                &[
                    "--user",
                    "set-environment",
                    &format!("SSH_AUTH_SOCK={socket}"),
                ],
            ),
            None if self.ambiente.macos => {
                (self.ambiente.executor)(programa, &["unsetenv", "SSH_AUTH_SOCK"])
            }
            None => (self.ambiente.executor)(
                programa,
                &["--user", "unset-environment", "SSH_AUTH_SOCK"],
            ),
        };
        match resultado {
            Ok(_) => self.aplicado.borrow_mut().sessao = desejado,
            Err(erro) if erro.kind() == io::ErrorKind::NotFound => {
                self.mudar_cfg(|c| c.sistema_sessao = false);
                self.avisar(
                    InfoBarSeverity::Warning,
                    t!(
                        "ferramentas-sem-programa-grafico",
                        programa = programa.to_string()
                    ),
                );
            }
            Err(erro) => {
                if desejado.is_some() {
                    self.avisar_erro(t!(
                        "ferramentas-nao-avisou-sessao-grafica",
                        erro = erro.to_string()
                    ));
                }
            }
        }
    }

    /// Põe ou tira `bloco` do `arquivo`, com o backup da primeira escrita da sessão.
    fn gravar_bloco(
        &self,
        arquivo: &Path,
        bloco: Option<&str>,
        lugar: Lugar,
        criar: bool,
        marca: impl Fn(&mut Aplicado) -> &mut bool,
    ) {
        let ligado = bloco.is_some();
        let estava = *marca(&mut self.aplicado.borrow_mut());
        if !ligado && !estava {
            return;
        }
        let backup = (!self.backup_feito.get()).then(|| {
            let mut nome = arquivo.as_os_str().to_owned();
            nome.push(".tirech-bak");
            PathBuf::from(nome)
        });
        match sistema::aplicar_bloco(arquivo, bloco, lugar, criar, backup.as_deref()) {
            Ok(mudou) => {
                if mudou && backup.is_some() && arquivo.exists() {
                    self.backup_feito.set(true);
                }
                *marca(&mut self.aplicado.borrow_mut()) = ligado;
            }
            Err(erro) => {
                if ligado {
                    self.avisar_erro(t!(
                        "ferramentas-nao-consegui-escrever",
                        arquivo = arquivo.display().to_string(),
                        erro = erro.to_string()
                    ));
                }
            }
        }
    }

    fn aplicar_no_windows(&self, config: &AgenteCfg) {
        // O Git com o OpenSSH do Windows.
        let estava = self.aplicado.borrow().git;
        if config.sistema_git != estava || config.sistema_git {
            let executor = &self.ambiente.executor;
            let resultado = if config.sistema_git {
                executor(
                    "git",
                    &["config", "--global", "core.sshCommand", SSH_DO_WINDOWS],
                )
            } else if executor("git", &["config", "--global", "--get", "core.sshCommand"])
                .is_ok_and(|v| v.trim() == SSH_DO_WINDOWS)
            {
                executor("git", &["config", "--global", "--unset", "core.sshCommand"])
            } else {
                Ok(String::new())
            };
            match resultado {
                Ok(_) => self.aplicado.borrow_mut().git = config.sistema_git,
                Err(erro) if erro.kind() == io::ErrorKind::NotFound => {
                    self.mudar_cfg(|c| c.sistema_git = false);
                    self.avisar(InfoBarSeverity::Warning, t!("ferramentas-sem-git"));
                }
                Err(erro) => {
                    if config.sistema_git {
                        self.avisar_erro(t!(
                            "ferramentas-nao-configurou-git",
                            erro = erro.to_string()
                        ));
                    }
                }
            }
        }
        // O `SSH_AUTH_SOCK` do usuário só serve com o pipe próprio.
        let desejado = (config.sistema_sessao && self.agente.servico_ocupa())
            .then(|| self.agente.caminho())
            .flatten()
            .map(|p| p.display().to_string());
        let atual = self.aplicado.borrow().ambiente_do_usuario.clone();
        if desejado == atual {
            return;
        }
        let resultado = match &desejado {
            Some(pipe) => (self.ambiente.executor)("setx", &["SSH_AUTH_SOCK", pipe]),
            None => (self.ambiente.executor)(
                "reg",
                &["delete", r"HKCU\Environment", "/v", "SSH_AUTH_SOCK", "/f"],
            ),
        };
        if resultado.is_ok() {
            self.aplicado.borrow_mut().ambiente_do_usuario = desejado;
        }
    }

    /// Tira do ambiente o que só vale com o app aberto.
    fn desfazer_ambiente(&self) {
        let mut config = self.cfg();
        config.sistema_sessao = false;
        if self.ambiente.windows {
            // Só o `SSH_AUTH_SOCK`: o Git continua configurado, e não depende do app aberto.
            let git = self.aplicado.borrow().git;
            config.sistema_git = git;
            self.aplicar_no_windows(&config);
        } else {
            // A sessão gráfica; os blocos ficam como estão.
            let ssh_config = self.aplicado.borrow().ssh_config;
            config.sistema_ssh_config = ssh_config;
            config.sistema_shells = self.aplicado.borrow().shells;
            self.aplicar_no_unix(&config);
        }
    }
}

/// O nome do arquivo, para as mensagens.
pub fn nome_do(arquivo: &Path) -> String {
    arquivo
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| arquivo.display().to_string())
}

/// A pasta `~/.ssh` da pessoa.
pub fn pasta_ssh_padrao() -> PathBuf {
    crate::armazenamento::pasta_pessoal()
        .map(|h| h.join(".ssh"))
        .unwrap_or_else(|| PathBuf::from(".ssh"))
}

/// Mostra o motivo de um [`KeyError`] para a pessoa: o erro de passphrase fica curto.
pub fn motivo(erro: &KeyError) -> String {
    match erro {
        KeyError::WrongPassphrase => t!("ferramentas-passphrase-incorreta"),
        KeyError::NeedsPassphrase => t!("ferramentas-chave-pede-passphrase"),
        outro => outro.to_string(),
    }
}

/// Abre a pasta no gerenciador de arquivos do sistema.
pub fn abrir_pasta(pasta: &Path) -> std::io::Result<()> {
    abrir_com(programa_do_sistema(), pasta)
}

/// Abre o arquivo no programa `editor` (só o nome do programa, sem argumentos); vazio, no que o
/// sistema associa ao arquivo.
pub fn abrir_no_editor(arquivo: &Path, editor: &str) -> std::io::Result<()> {
    let editor = editor.trim();
    abrir_com(
        if editor.is_empty() {
            programa_do_sistema()
        } else {
            editor
        },
        arquivo,
    )
}

/// Abre o endereço (`http`, `https` ou `mailto`) no programa que o sistema associa a ele, sem
/// esperar. Outros esquemas não abrem: um link de nota não manda rodar programa nenhum.
pub fn abrir_endereco(endereco: &str) -> std::io::Result<()> {
    let minusculo = endereco.trim().to_ascii_lowercase();
    if !["http://", "https://", "mailto:"]
        .iter()
        .any(|esquema| minusculo.starts_with(esquema))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            t!("ferramentas-endereco-nao-permitido"),
        ));
    }
    abrir_com(programa_do_sistema(), Path::new(endereco.trim()))
}

/// O programa que abre um caminho do jeito do sistema.
fn programa_do_sistema() -> &'static str {
    if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    }
}

/// Inicia `programa` com o `caminho`, sem esperar ele acabar.
fn abrir_com(programa: &str, caminho: &Path) -> std::io::Result<()> {
    let mut comando = std::process::Command::new(programa);
    // O `explorer`, o `open` e o `xdg-open` não precisam de console. O editor da pessoa pode ser
    // um programa de console (que precisa da janela dele), por isso só estes passam por
    // `sem_janela`.
    if programa == programa_do_sistema() {
        sistema::sem_janela(&mut comando);
    }
    comando.arg(caminho).spawn().map(|_| ())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn ambiente(pessoal: &str) -> Ambiente {
        Ambiente::de_teste(Path::new("/dados"), Path::new(pessoal).join(".ssh"))
    }

    #[test]
    fn no_portable_os_padroes_ficam_dentro_da_pasta() {
        let mut a = ambiente("/home/ana");
        a.portatil = Some(PathBuf::from("/pendrive/tirech-omnidata"));
        assert_eq!(
            a.pasta_temporaria(),
            Path::new("/pendrive/tirech-omnidata/temp")
        );
        assert_eq!(
            a.pasta_dos_assistentes_padrao().as_deref(),
            Some(Path::new("/pendrive/tirech-omnidata/assistentes"))
        );
        a.portatil = None;
        assert_eq!(a.pasta_temporaria(), std::env::temp_dir());
    }

    #[test]
    fn a_pasta_dos_assistentes_fica_no_cache_de_cada_sistema() {
        let mut a = ambiente("/home/ana");
        assert_eq!(
            a.pasta_dos_assistentes_padrao(),
            Some(PathBuf::from("/home/ana/.cache/tirech-omni/assistentes"))
        );
        a.xdg_cache = Some(PathBuf::from("/var/cache/ana"));
        assert_eq!(
            a.pasta_dos_assistentes_padrao(),
            Some(PathBuf::from("/var/cache/ana/tirech-omni/assistentes"))
        );
        // Um `XDG_CACHE_HOME` relativo não vale.
        a.xdg_cache = Some(PathBuf::from("cache"));
        assert_eq!(
            a.pasta_dos_assistentes_padrao(),
            Some(PathBuf::from("/home/ana/.cache/tirech-omni/assistentes"))
        );
        a.macos = true;
        assert_eq!(
            a.pasta_dos_assistentes_padrao(),
            Some(PathBuf::from(
                "/home/ana/Library/Caches/tirech-omni/assistentes"
            ))
        );
        a.macos = false;
        a.windows = true;
        a.local_app_data = Some(PathBuf::from("/c/Users/ana/AppData/Local"));
        assert_eq!(
            a.pasta_dos_assistentes_padrao(),
            Some(
                PathBuf::from("/c/Users/ana/AppData/Local")
                    .join("Tirech Omni")
                    .join("assistentes")
            )
        );
    }

    #[test]
    fn a_pasta_escolhida_vale_com_til() {
        let a = ambiente("/home/ana");
        assert_eq!(
            a.pasta_dos_assistentes("~/ia"),
            Some(PathBuf::from("/home/ana/ia"))
        );
        assert_eq!(
            a.pasta_dos_assistentes(" /srv/ia "),
            Some(PathBuf::from("/srv/ia"))
        );
        assert_eq!(
            a.pasta_dos_assistentes("  "),
            a.pasta_dos_assistentes_padrao()
        );
    }

    #[test]
    fn nao_esvazia_a_pasta_pessoal_nem_a_raiz() {
        let pessoal = Path::new("/home/ana");
        assert!(!pode_esvaziar(pessoal, Some(pessoal)));
        assert!(!pode_esvaziar(Path::new("/"), Some(pessoal)));
        assert!(pode_esvaziar(Path::new("/home/ana/ia"), Some(pessoal)));
    }

    #[test]
    fn esvaziar_apaga_o_que_esta_dentro_e_deixa_a_pasta() {
        let pasta = std::env::temp_dir().join(format!("tirech-esvaziar-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        fs::create_dir_all(pasta.join("a/b")).unwrap();
        fs::write(pasta.join("a/b/c.txt"), "c").unwrap();
        fs::write(pasta.join("d.txt"), "d").unwrap();
        assert_eq!(itens_da_pasta(&pasta), 2);
        let (n, erro) = esvaziar_pasta(&pasta);
        assert_eq!(n, 2);
        assert!(erro.is_none());
        assert!(pasta.is_dir());
        assert_eq!(itens_da_pasta(&pasta), 0);
        fs::remove_dir_all(&pasta).unwrap();
        // Sem a pasta, não há o que apagar.
        assert_eq!(esvaziar_pasta(&pasta).0, 0);
        assert_eq!(itens_da_pasta(&pasta), 0);
    }

    #[cfg(unix)]
    #[test]
    fn esvaziar_apaga_o_link_sem_seguir() {
        let pasta =
            std::env::temp_dir().join(format!("tirech-esvaziar-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        let (dentro, fora) = (pasta.join("dentro"), pasta.join("fora"));
        fs::create_dir_all(&dentro).unwrap();
        fs::create_dir_all(&fora).unwrap();
        fs::write(fora.join("fica.txt"), "x").unwrap();
        std::os::unix::fs::symlink(&fora, dentro.join("link")).unwrap();
        let (n, erro) = esvaziar_pasta(&dentro);
        assert_eq!(n, 1);
        assert!(erro.is_none());
        assert_eq!(itens_da_pasta(&dentro), 0);
        assert!(fora.join("fica.txt").is_file());
        fs::remove_dir_all(&pasta).unwrap();
    }
}
