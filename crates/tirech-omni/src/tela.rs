//! A tela do app: menus, a árvore de conexões, as propriedades da conexão escolhida e os
//! terminais em abas e em grade. A lógica de dados fica no [`Tree`]; aqui só se liga o `Tree`
//! aos widgets e às sessões.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use tomni_ui::WindowGeometry;
use tomni_ui::dialogs;
use tomni_ui::files::Transfers;
use tomni_ui::prelude::*;
use tomni_ui::terminal::{PaneLayout, Session, Terminal, TerminalPanes, move_session};
use tomni_ui::{request_focus, request_focus_in};

use crate::abrir::{
    DadosCertificado, FilaChaves, Opener, PedidoChave, Senhas, ler_fila, mexer_na_fila,
    precisa_de_senha,
};
use crate::abrir_em::AlvoAbrir;
use crate::armazenamento::Armazenamento;
use crate::assistente::perfis::ArquivoIa;
use crate::atalhos;
use crate::aws_config;
use crate::busca::{self, Destino};
use crate::campos::Campo as CampoDaGrade;
use crate::cofre::{
    CAMPO_TOKEN, Campo, Cofre, CofreCompartilhado, IdentidadeNebula, IdentidadeOpenVpn,
    IdentidadeTailscale, IdentidadeWireGuard, IdentidadeZeroTier, Item, Tipo,
};
use crate::comandos::{Inicio, rotulo_da_linha};
use crate::configuracoes::catalogo;
use crate::configuracoes::{AbrirEm, AbrirEspaco, ColarArquivos, Configuracoes, X11Padrao};
use crate::dica_da_aba::EstadoDaAba;
use crate::envio::Envio;
use crate::espacos::{
    Aba, AssistenteSalvo, ClusterIaSalvo, Entrada, Erro as ErroDeEspaco, Espaco, Espacos,
    JanelaSalva, Origem, Resolvida, aviso_de_faltantes, id_da_chave, nome_valido,
};
use crate::esquemas;
use crate::estado;
use crate::ferramentas::{Ambiente, Ferramentas};
use crate::ia::Ferramenta;
use crate::icone;
use crate::inicio::{AcoesInicio, Cartao, cartao_ferramenta, pagina_inicio};
use crate::janelas::{self, Janela, Janelas};
use crate::marca::{self, NOME_DO_APP};
use crate::mcp::permissao::PermissaoIa;
use crate::modelo::{
    Ativar, ComandosAoConectar, Connection, FtpCodificacao, FtpModo, FtpSeguranca, FtpTls, Jump,
    Kind, ModoCredencial, ModoX11, Node, RcloneOpcoes, S3Certificado, S3Estilo, S3Opcoes,
    S3Provedor, ShellRemoto, Tree, Tunel, resumo_dos_tuneis,
};
use crate::nebula::Motores;
use crate::openvpn::{MotorOpenVpn, MotoresVpn};
use crate::paineis::{self, PaineisDaPagina};
use crate::recentes::{self, Alvo as AlvoRecente, Recentes};
use crate::redes::{
    MotorRede, PerfilRede, ProxyRede, Rede, RedeResolvida, TipoProxy, Usos, herdar,
    known_hosts_do_perfil, rede_efetiva,
};
use crate::relogio::Alarmes;
use crate::ssh_config;
use crate::t;
use crate::tailscale::{MotorTailscale, MotoresTailscale};
use crate::tela_agente::pagina_agente;
use crate::tela_chaves::pagina_chaves;
use crate::tela_cofre::{Portabilidade, dialogo_desbloquear_com, pagina_cofre_completa};
use crate::tela_configuracoes::{Contexto, pagina_configuracoes};
use crate::tela_notas::{AoFechar, ContextoNotas, EstadoNotas, pagina_notas};
use crate::tmux;
use crate::wireguard::{MotorWireGuard, MotoresWireGuard};
use crate::zerotier::{MotorZeroTier, MotoresZeroTier};
use crate::zoom_fonte::ZoomFonte;

mod abas_caidas;
mod abrir_em;
mod arquivos;
mod assistente;
mod baixar;
mod bandeja;
mod chaves;
mod cluster;
pub(crate) mod cofre_portatil;
mod colar;
mod comandos;
mod comandos_editor;
mod controlar;
mod credencial;
mod desempenho;
mod duplicar;
mod envio;
mod estado_conexoes;
mod ganchos;
mod ia;
mod ia_chat;
mod ia_comando;
mod ia_envio;
mod ia_perfis;
mod importacao;
mod instalar;
mod instancias;
mod lancador_links;
mod links;
mod logs_cifrados;
mod mcp;
mod mcp_aprovacao;
mod mcp_arquivos;
mod mcp_como;
mod mcp_espaco;
mod mcp_espacos;
mod mcp_paineis;
mod mcp_propriedades;
mod mcp_sessoes;
mod mcp_soltos;
mod mcp_terminal;
mod menus;
mod mouse;
mod notas_conexao;
mod openvpn_perguntas;
mod pagina_logs;
mod portas;
mod programas;
mod rclone;
mod rclone_form;
mod redes;
mod retentar;
mod scripts;
mod sessao_tmux;
mod sessoes_servidor;
mod tema;
mod tuneis;
mod wsl;

/// Se os assistentes de IA estão ligados em Geral, lido a cada chamada (prop de `.visible()` e
/// `.item_visible()` do que é deles).
pub(crate) fn com_ia(config: Signal<Configuracoes>) -> impl Fn() -> bool + Copy + 'static {
    move || config.with(|c| c.geral.assistente_ia)
}

/// Se o controle por assistentes (MCP) está ligado em Geral, como [`com_ia`].
pub(crate) fn com_mcp(config: Signal<Configuracoes>) -> impl Fn() -> bool + Copy + 'static {
    move || config.with(|c| c.geral.mcp)
}

/// Os tipos de terminal que o servidor SSH pode receber.
const TERMINAIS: [&str; 4] = ["xterm-256color", "xterm", "vt100", "linux"];

/// As opções da linha "Encaminhar X11": a primeira diz qual é o modo padrão das configurações.
fn opcoes_de_x11(padrao: X11Padrao) -> Vec<String> {
    ModoX11::TODOS
        .into_iter()
        .map(|modo| match modo {
            ModoX11::Padrao => format!("{} ({})", modo.nome(), padrao.nome()),
            outro => outro.nome().to_owned(),
        })
        .collect()
}

/// O `Ativar` que o texto de uma linha "Padrão/Ligado/Desligado" escolhe.
fn ativar_do_rotulo(rotulo: &str) -> Ativar {
    Ativar::TODOS
        .into_iter()
        .find(|a| a.nome() == rotulo)
        .unwrap_or_default()
}

/// O modo que o texto da linha "Encaminhar X11" escolhe (`Padrão (…)` é o `Padrao`).
fn modo_x11_do_rotulo(rotulo: &str) -> ModoX11 {
    ModoX11::TODOS
        .into_iter()
        .find(|modo| *modo != ModoX11::Padrao && modo.nome() == rotulo)
        .unwrap_or(ModoX11::Padrao)
}

/// O rótulo do modo `modo` na linha "Encaminhar X11".
fn rotulo_de_x11(modo: ModoX11, padrao: X11Padrao) -> String {
    match modo {
        ModoX11::Padrao => format!("{} ({})", modo.nome(), padrao.nome()),
        outro => outro.nome().to_owned(),
    }
}

/// As opções da linha "Ao colar": a primeira segue as Configurações.
fn opcoes_de_colar() -> Vec<String> {
    std::iter::once(rotulo_de_colar(None))
        .chain(ColarArquivos::nomes())
        .collect()
}

/// O modo que o texto da linha "Ao colar" escolhe (`None`: o das
/// Configurações).
fn colar_do_rotulo(rotulo: &str) -> Option<ColarArquivos> {
    ColarArquivos::do_nome(rotulo)
}

/// O rótulo do modo `modo` na linha "Ao colar".
fn rotulo_de_colar(modo: Option<ColarArquivos>) -> String {
    modo.map_or_else(|| t!("comum-colar-padrao"), ColarArquivos::nome)
}

/// O rótulo e a chave do grupo fixo, sempre o último da árvore, onde ficam os espaços de
/// trabalho.
fn raiz_dos_espacos() -> String {
    t!("casca-raiz-dos-espacos")
}
/// A chave do grupo fixo dos espaços.
const CHAVE_DA_RAIZ: &str = "espaco:raiz";
/// O começo da chave de uma pasta de espaços; o resto é o caminho dela.
const CHAVE_DE_PASTA: &str = "espaco:pasta:";
/// O começo da chave de um espaço; o resto é o caminho dele (sem o `.json`).
const CHAVE_DE_ESPACO: &str = "espaco:item:";
/// A opção do seletor de pasta do diálogo de salvar que cria uma pasta.
fn nova_pasta_opcao() -> String {
    t!("casca-nova-pasta-opcao")
}

/// O que uma chave da árvore guarda quando não é uma conexão ou um grupo de conexões.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ChaveDeEspaco {
    /// O grupo fixo "Espaços de trabalho".
    Raiz,
    /// Uma pasta de espaços, com o caminho dela.
    Pasta(String),
    /// Um espaço, com o caminho dele.
    Espaco(String),
}

impl ChaveDeEspaco {
    /// Lê uma chave da árvore; `None` se ela é de uma conexão ou de um grupo de conexões.
    fn de(chave: &str) -> Option<Self> {
        if chave == CHAVE_DA_RAIZ {
            Some(Self::Raiz)
        } else if let Some(caminho) = chave.strip_prefix(CHAVE_DE_PASTA) {
            Some(Self::Pasta(caminho.to_owned()))
        } else {
            chave
                .strip_prefix(CHAVE_DE_ESPACO)
                .map(|caminho| Self::Espaco(caminho.to_owned()))
        }
    }

    /// A chave da árvore.
    fn chave(&self) -> String {
        match self {
            Self::Raiz => CHAVE_DA_RAIZ.to_owned(),
            Self::Pasta(caminho) => format!("{CHAVE_DE_PASTA}{caminho}"),
            Self::Espaco(caminho) => format!("{CHAVE_DE_ESPACO}{caminho}"),
        }
    }

    /// A pasta (`""` é a raiz) onde a linha mora ou, numa pasta, a própria pasta.
    fn pasta(&self) -> String {
        match self {
            Self::Raiz => String::new(),
            Self::Pasta(caminho) => caminho.clone(),
            Self::Espaco(caminho) => caminho
                .rsplit_once('/')
                .map_or(String::new(), |(pai, _)| pai.to_owned()),
        }
    }
}

/// O último pedaço de um caminho de espaço ou de pasta.
fn nome_do_caminho(caminho: &str) -> &str {
    caminho.rsplit_once('/').map_or(caminho, |(_, nome)| nome)
}

/// O que fazer quando o diálogo de salvar um espaço fecha: ele fecha sozinho depois do botão,
/// então reabrir ou abrir o próximo espera esse fechamento.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Depois {
    #[default]
    Nada,
    /// O nome não serve: o diálogo volta com o erro.
    Reabrir,
    /// Já existe um espaço com esse nome: pergunta se substitui.
    Substituir,
}

/// Onde as abas de um espaço aberto entram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComoAbrir {
    /// Como o espaço foi salvo: um painel entra no painel ativo, uma janela ou todas abrem
    /// janelas novas.
    Padrao,
    /// Sempre em janelas novas.
    NovaJanela,
    /// Todas as abas no painel ativo da última janela ativa.
    PainelAtivo,
}

/// Quem recusou a senha salva, quando o pedido volta ao diálogo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recusa {
    /// A primeira vez: nada foi recusado.
    Nao,
    /// O servidor da conexão.
    Conexao,
    /// O servidor de salto.
    Salto,
}

/// Onde uma aba nova entra: a janela e o painel. O padrão é o painel livre (ou o ativo) da
/// última janela ativa.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Lugar {
    /// A janela (`None`: a última ativa).
    janela: Option<WindowHandle>,
    /// O painel (`None`: o primeiro livre ou o ativo).
    painel: Option<usize>,
}

impl Lugar {
    /// O painel livre (ou o ativo) da janela `janela`.
    fn na_janela(janela: Janela) -> Self {
        Self {
            janela: Some(janela.handle),
            painel: None,
        }
    }

    /// O painel `painel` da janela `janela`.
    fn painel(janela: Janela, painel: usize) -> Self {
        Self {
            janela: Some(janela.handle),
            painel: Some(painel),
        }
    }
}

/// Uma abertura que espera a senha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pedido {
    /// A conexão a abrir.
    id: u64,
    /// Onde a aba entra.
    lugar: Lugar,
    /// Se o servidor recusou a senha que estava salva.
    recusa: Recusa,
    /// Se a abertura entra nos recentes (a restauração do estado e a repetição depois de uma
    /// recusa não contam).
    registrar: bool,
    /// Abre a página Arquivos da conexão em vez de um terminal.
    arquivos: bool,
    /// Lança um programa gráfico sem aba em vez de abrir um terminal (M37).
    programa: Option<programas::QualPrograma>,
}

/// Uma sessão aberta com senha salva: se o servidor a recusar, a tela pergunta de novo.
#[derive(Clone, Copy, Debug)]
struct Vigiada {
    sessao: Session,
    id: u64,
    lugar: Lugar,
}

/// O que espera o cofre ser desbloqueado (abrir ou criar a nota de uma conexão).
type DepoisDoDesbloqueio = Box<dyn FnOnce(&Tela)>;

/// A senha de qual servidor: o da conexão ou o do salto.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Alvo {
    Conexao,
    Salto,
}

/// A opção da linha "Credencial" que não liga a conexão a nenhum item do cofre.
fn perguntar() -> String {
    t!("casca-perguntar-ao-conectar")
}

/// O texto de um item na linha "Credencial". O id no fim desfaz os títulos repetidos.
fn rotulo(item: &Item) -> String {
    format!("{} [{}]", item.titulo, item.id)
}

/// O id que o [`rotulo`] leva no fim.
fn id_do_rotulo(texto: &str) -> Option<u64> {
    let (_, id) = texto.strip_suffix(']')?.rsplit_once('[')?;
    id.parse().ok()
}

/// A opção da linha "Chave do cofre" que não liga a conexão a nenhuma chave.
fn sem_chave_do_cofre() -> String {
    t!("comum-nenhuma")
}

/// As opções da linha "Chave do cofre": nenhuma e as chaves que têm a privada, por título.
fn opcoes_de_chave_do_cofre(cofre: &CofreCompartilhado) -> Vec<String> {
    let mut itens: Vec<String> = cofre.ler(|c| {
        c.chaves_ssh()
            .into_iter()
            .filter(|i| {
                i.chave
                    .as_ref()
                    .is_some_and(|k| !k.privada.trim().is_empty())
            })
            .map(rotulo)
            .collect()
    });
    itens.sort_by_key(|r| r.to_lowercase());
    itens.insert(0, sem_chave_do_cofre());
    itens
}

/// O texto da linha "Chave do cofre" para a chave de `uid` (nenhuma, se ela não existe).
fn rotulo_da_chave_do_cofre(cofre: &CofreCompartilhado, uid: Option<&str>) -> String {
    uid.and_then(|uid| {
        cofre.agora(|c| {
            c.chaves_ssh()
                .into_iter()
                .find(|i| i.uid == uid)
                .map(rotulo)
        })
    })
    .unwrap_or_else(sem_chave_do_cofre)
}

/// O `uid` do item que o texto da linha "Chave do cofre" escolhe.
fn uid_da_chave_do_cofre(cofre: &CofreCompartilhado, texto: &str) -> Option<String> {
    let id = id_do_rotulo(texto)?;
    cofre.agora(|c| c.item(id).map(|i| i.uid.clone()))
}

/// Os itens do cofre que servem de credencial de uma conexão.
fn serve_de_credencial(tipo: Tipo) -> bool {
    matches!(tipo, Tipo::Ssh | Tipo::Sftp | Tipo::S3 | Tipo::Senha)
}

/// As opções da linha "Credencial": perguntar e os itens do cofre, por título.
fn opcoes_de_credencial(cofre: &CofreCompartilhado) -> Vec<String> {
    let mut itens: Vec<String> = cofre.ler(|c| {
        c.itens()
            .iter()
            .filter(|i| serve_de_credencial(i.tipo))
            .map(rotulo)
            .collect()
    });
    itens.sort_by_key(|r| r.to_lowercase());
    itens.insert(0, perguntar());
    itens
}

/// As opções da linha "Credencial" da conexão: as de [`opcoes_de_credencial`] com os modos do
/// agente logo depois de "Perguntar ao conectar" e, antes de tudo, o "Herdar" da pasta
/// (`herdar`), só nos tipos que usam SSH.
fn opcoes_de_credencial_da_conexao(
    cofre: &CofreCompartilhado,
    herdar: Option<String>,
) -> Vec<String> {
    let mut opcoes = opcoes_de_credencial(cofre);
    if let Some(herdar) = herdar {
        let modos = [ModoCredencial::Agente, ModoCredencial::AgenteOuSenha];
        for (n, modo) in modos.into_iter().enumerate() {
            opcoes.insert(1 + n, modo.nome());
        }
        opcoes.insert(0, herdar);
    }
    opcoes
}

/// O texto da linha "Credencial" da conexão: o item `id` ou, sem ele, o rótulo do `modo`.
fn rotulo_da_credencial_da_conexao(
    cofre: &CofreCompartilhado,
    id: Option<u64>,
    modo: ModoCredencial,
) -> String {
    id.and_then(|id| cofre.agora(|c| c.item(id).map(rotulo)))
        .unwrap_or_else(|| modo.nome())
}

/// O texto da linha "Credencial" para o item `id` (perguntar, se ele não existe).
fn rotulo_da_credencial(cofre: &CofreCompartilhado, id: Option<u64>) -> String {
    id.and_then(|id| cofre.agora(|c| c.item(id).map(rotulo)))
        .unwrap_or_else(perguntar)
}

/// A senha do item `id` (vazia, se ele não existe ou o cofre está bloqueado).
fn senha_da_credencial(cofre: &CofreCompartilhado, id: Option<u64>) -> String {
    id.and_then(|id| cofre.agora(|c| c.item(id).map(|i| i.senha.clone())))
        .unwrap_or_default()
}

/// O token de sessão (campo extra) do item da credencial.
fn token_da_credencial(cofre: &CofreCompartilhado, id: Option<u64>) -> String {
    id.and_then(|id| {
        cofre.agora(|c| {
            c.item(id).and_then(|i| {
                i.campos
                    .iter()
                    .find(|campo| campo.nome == CAMPO_TOKEN)
                    .map(|campo| campo.valor.clone())
            })
        })
    })
    .unwrap_or_default()
}

/// Troca o valor do signal só se for diferente: os efeitos que o leem não rodam à toa.
fn poe(signal: Signal<String>, valor: String) {
    if signal.get_untracked() != valor {
        signal.set(valor);
    }
}

/// Os valores da conexão escolhida, um signal por propriedade. A grade de propriedades edita
/// estes signals; o app os copia para a [`Tree`] (e para o disco) a cada mudança.
#[derive(Clone, Copy)]
struct Campos {
    nome: Signal<String>,
    descricao: Signal<String>,
    tipo: Signal<String>,
    host: Signal<String>,
    porta: Signal<f64>,
    usuario: Signal<String>,
    chave: Signal<String>,
    /// O texto da linha "Chave do cofre" (`sem_chave_do_cofre` ou `"título [id]"`).
    chave_cofre: Signal<String>,
    term: Signal<String>,
    log: Signal<String>,
    keepalive: Signal<f64>,
    /// O texto da linha "Ao colar imagem ou arquivo" (M45).
    colar_arquivos: Signal<String>,
    pasta_colar: Signal<String>,
    shell_remoto: Signal<String>,
    comprimir: Signal<bool>,
    jump_host: Signal<String>,
    jump_porta: Signal<f64>,
    jump_usuario: Signal<String>,
    /// O texto da linha "Credencial" (`perguntar`, um modo do agente ou `"título [id]"`).
    credencial: Signal<String>,
    /// A senha do item da credencial.
    senha: Signal<String>,
    jump_credencial: Signal<String>,
    jump_senha: Signal<String>,
    ftp_seguranca: Signal<String>,
    ftp_modo: Signal<String>,
    ftp_tls: Signal<String>,
    ftp_codificacao: Signal<String>,
    ftp_fuso: Signal<f64>,
    ftp_portas: Signal<String>,
    s3_provedor: Signal<String>,
    s3_endpoint: Signal<String>,
    s3_regiao: Signal<String>,
    s3_bucket: Signal<String>,
    s3_estilo: Signal<String>,
    s3_classe: Signal<String>,
    s3_certificado: Signal<String>,
    s3_ca: Signal<String>,
    /// O token de sessão do item da credencial.
    s3_token: Signal<String>,
    rclone_remoto: Signal<String>,
    rclone_caminho: Signal<String>,
    /// O endereço do link.
    url: Signal<String>,
    /// O texto da linha "Rede" da conexão ou do grupo escolhido (`Herdar (…)`, `Direta` ou o
    /// nome de um perfil).
    rede: Signal<String>,
    /// O texto da linha "Assistente de IA" da conexão: o nome do provedor ou `Padrão (…)`.
    perfil_ia: Signal<String>,
    /// O texto da linha "Assistentes (MCP)" da conexão ou do grupo escolhido (`Herdar
    /// (…)`, `Desligada`, `Só ler`, `Aprovar` ou `Livre`).
    mcp: Signal<String>,
    /// O texto da linha "Sessão persistente" da conexão ou do grupo escolhido (`Herdar (…)`,
    /// `Nenhuma`, `tmux` ou `screen`).
    persistente: Signal<String>,
    /// O texto da linha "Mouse no terminal" da conexão ou do grupo escolhido (`Herdar (…)`,
    /// `Copiar e colar` ou `Menu`).
    mouse: Signal<String>,
    /// O texto da linha "Tentar de novo após" da conexão ou do grupo escolhido (`Herdar (…)`,
    /// `Desligado` ou um tempo).
    retentar: Signal<String>,
    /// O texto da linha "Credencial" da pasta escolhida (`Herdar (…)` ou um modo).
    credencial_da_pasta: Signal<String>,
    /// O texto da linha "Encaminhar X11" (`Padrão (<modo>)`, `Desligado`, `Não confiável` ou
    /// `Confiável`).
    x11: Signal<String>,
    /// Os túneis da conexão; o editor de túneis escreve aqui.
    tuneis: Signal<Vec<Tunel>>,
    so_tuneis: Signal<bool>,
    parar_se_falhar: Signal<bool>,
    /// Os comandos ao conectar da conexão ou da pasta escolhida; o editor escreve aqui.
    comandos: Signal<ComandosAoConectar>,
    pasta_local: Signal<String>,
    pasta_remota: Signal<String>,
    comparar: Signal<String>,
    sincronizar_pastas: Signal<String>,
    /// O programa do terminal local (só no tipo Local).
    local_programa: Signal<String>,
    /// Os argumentos dele, numa linha.
    local_argumentos: Signal<String>,
    /// A pasta onde o terminal local começa.
    local_pasta: Signal<String>,
    /// A pasta em que os assistentes de IA abrem no servidor (M46).
    ia_pasta: Signal<String>,
    /// O texto da linha "Instalar por" dos assistentes.
    ia_metodo: Signal<String>,
    /// Os argumentos de cada assistente, na ordem de `Ferramenta::TODAS`.
    ia_argumentos: [Signal<String>; 3],
}

impl Campos {
    fn novo() -> Self {
        Self {
            nome: signal(String::new()),
            descricao: signal(String::new()),
            tipo: signal(Kind::Ssh.nome().to_owned()),
            host: signal(String::new()),
            porta: signal(22.0),
            usuario: signal(String::new()),
            chave: signal(String::new()),
            chave_cofre: signal(sem_chave_do_cofre()),
            term: signal(TERMINAIS[0].to_owned()),
            log: signal(String::new()),
            keepalive: signal(30.0),
            colar_arquivos: signal(rotulo_de_colar(None)),
            pasta_colar: signal(String::new()),
            shell_remoto: signal(ShellRemoto::default().nome()),
            comprimir: signal(false),
            jump_host: signal(String::new()),
            jump_porta: signal(22.0),
            jump_usuario: signal(String::new()),
            credencial: signal(perguntar()),
            senha: signal(String::new()),
            jump_credencial: signal(perguntar()),
            jump_senha: signal(String::new()),
            ftp_seguranca: signal(FtpSeguranca::default().nome().to_owned()),
            ftp_modo: signal(FtpModo::default().nome().to_owned()),
            ftp_tls: signal(FtpTls::Auto.nome().to_owned()),
            ftp_codificacao: signal(FtpCodificacao::Auto.nome().to_owned()),
            ftp_fuso: signal(0.0),
            ftp_portas: signal(String::new()),
            s3_provedor: signal(S3Provedor::Amazon.nome()),
            s3_endpoint: signal(String::new()),
            s3_regiao: signal(String::new()),
            s3_bucket: signal(String::new()),
            s3_estilo: signal(S3Estilo::Caminho.nome().to_owned()),
            s3_classe: signal(String::new()),
            s3_certificado: signal(S3Certificado::Verificar.nome().to_owned()),
            s3_ca: signal(String::new()),
            s3_token: signal(String::new()),
            rclone_remoto: signal(String::new()),
            rclone_caminho: signal(String::new()),
            url: signal(String::new()),
            rede: signal(herdar()),
            perfil_ia: signal(String::new()),
            mcp: signal(String::new()),
            persistente: signal(String::new()),
            mouse: signal(String::new()),
            retentar: signal(String::new()),
            credencial_da_pasta: signal(String::new()),
            x11: signal(ModoX11::Padrao.nome().to_owned()),
            tuneis: signal(Vec::new()),
            so_tuneis: signal(false),
            comandos: signal(ComandosAoConectar::default()),
            parar_se_falhar: signal(false),
            pasta_local: signal(String::new()),
            pasta_remota: signal(String::new()),
            comparar: signal(Ativar::Padrao.nome().to_owned()),
            sincronizar_pastas: signal(Ativar::Padrao.nome().to_owned()),
            local_programa: signal(String::new()),
            local_argumentos: signal(String::new()),
            local_pasta: signal(String::new()),
            ia_pasta: signal(String::new()),
            ia_metodo: signal(ia::rotulo_do_metodo(None)),
            ia_argumentos: [
                signal(String::new()),
                signal(String::new()),
                signal(String::new()),
            ],
        }
    }

    /// Mostra nas linhas de credencial e senha o que o cofre guarda para `c`.
    fn sincronizar(&self, c: &Connection, cofre: &CofreCompartilhado) {
        let salto = c.jump.as_ref().and_then(|j| j.credencial);
        poe(
            self.credencial,
            rotulo_da_credencial_da_conexao(cofre, c.credencial, c.credencial_modo),
        );
        poe(self.senha, senha_da_credencial(cofre, c.credencial));
        poe(
            self.chave_cofre,
            rotulo_da_chave_do_cofre(cofre, c.chave_do_cofre.as_deref()),
        );
        poe(self.jump_credencial, rotulo_da_credencial(cofre, salto));
        poe(self.jump_senha, senha_da_credencial(cofre, salto));
        poe(self.s3_token, token_da_credencial(cofre, c.credencial));
    }

    /// Mostra as opções do S3 nas propriedades.
    fn mostrar_s3(&self, s3: &S3Opcoes) {
        poe(self.s3_provedor, s3.provider.nome());
        poe(self.s3_endpoint, s3.endpoint.clone());
        poe(self.s3_regiao, s3.region.clone());
        poe(self.s3_bucket, s3.bucket.clone());
        poe(self.s3_estilo, s3.url_style.nome().to_owned());
        poe(self.s3_classe, s3.storage_class.clone());
        poe(self.s3_certificado, s3.certificate.nome().to_owned());
        poe(self.s3_ca, texto_do_caminho(s3.ca_file.as_ref()));
    }

    /// Mostra a conexão `c` nas propriedades.
    fn carregar(&self, c: &Connection, cofre: &CofreCompartilhado) {
        self.nome.set(c.name.clone());
        self.descricao.set(c.descricao.clone());
        self.tipo.set(c.kind.nome().to_owned());
        self.host.set(c.host.clone());
        self.porta.set(f64::from(c.port));
        self.usuario.set(c.user.clone());
        self.chave.set(texto_do_caminho(c.key_file.as_ref()));
        self.term.set(c.term.clone());
        self.log.set(texto_do_caminho(c.log_file.as_ref()));
        self.keepalive.set(f64::from(c.keepalive_secs));
        // O "(…)" do padrão das Configurações entra em `sincronizar_colar`.
        self.colar_arquivos.set(rotulo_de_colar(c.colar_arquivos));
        self.pasta_colar.set(c.pasta_colar.clone());
        self.shell_remoto.set(c.shell_remoto.nome());
        self.comprimir.set(c.comprimir);
        let salto = c.jump.clone().unwrap_or(Jump {
            host: String::new(),
            port: 22,
            user: String::new(),
            credencial: None,
        });
        self.jump_host.set(salto.host);
        self.jump_porta.set(f64::from(salto.port));
        self.jump_usuario.set(salto.user);
        self.ftp_seguranca.set(c.ftp_security.nome().to_owned());
        self.ftp_modo.set(c.ftp_mode.nome().to_owned());
        self.ftp_tls.set(c.ftp_tls.nome().to_owned());
        self.ftp_codificacao.set(c.ftp_encoding.nome().to_owned());
        self.ftp_fuso.set(f64::from(c.ftp_timezone_min));
        self.ftp_portas.set(c.ftp_active_ports.clone());
        // Fora do S3 o endereço fica em branco: o do provedor padrão é longo para a coluna e
        // as linhas estão desabilitadas.
        if c.kind == Kind::S3 {
            self.mostrar_s3(&c.s3);
        } else {
            self.mostrar_s3(&S3Opcoes {
                endpoint: String::new(),
                ..c.s3.clone()
            });
        }
        self.rclone_remoto.set(c.rclone.remote.clone());
        self.rclone_caminho.set(c.rclone.path.clone());
        self.url.set(c.url.clone());
        self.x11.set(c.x11.nome().to_owned());
        self.tuneis.set(c.tuneis.clone());
        self.so_tuneis.set(c.so_tuneis);
        self.comandos.set(c.comandos.clone());
        self.parar_se_falhar.set(c.parar_se_tunel_falhar);
        self.pasta_local.set(c.pasta_local.clone());
        self.pasta_remota.set(c.pasta_remota.clone());
        self.comparar.set(c.comparar.nome().to_owned());
        self.sincronizar_pastas.set(c.sincronizar.nome().to_owned());
        self.local_programa.set(c.local_programa.clone());
        self.local_argumentos.set(c.local_argumentos.clone());
        self.local_pasta.set(c.local_pasta.clone());
        self.ia_pasta.set(c.ia.pasta.clone());
        self.ia_metodo.set(ia::rotulo_do_metodo(c.ia.metodo));
        for (campo, f) in self.ia_argumentos.iter().zip(crate::ia::Ferramenta::TODAS) {
            campo.set(c.ia.argumentos.get(&f).cloned().unwrap_or_default());
        }
        self.sincronizar(c, cofre);
    }

    /// A conexão `base` com o que está nas propriedades. Lê os signals com rastreamento: quem
    /// chama de dentro de um efeito passa a depender deles.
    fn aplicar(&self, base: &Connection, cofre: &CofreCompartilhado) -> Connection {
        let tipo = self.tipo.get();
        let mut c = base.clone();
        c.name = self.nome.get();
        c.descricao = self.descricao.get();
        c.kind = Kind::TODOS
            .into_iter()
            .find(|k| k.nome() == tipo)
            .unwrap_or(base.kind);
        c.host = self.host.get().trim().to_owned();
        c.port = como_porta(self.porta.get());
        c.user = self.usuario.get().trim().to_owned();
        c.key_file = caminho_do_texto(&self.chave.get());
        c.term = self.term.get();
        c.log_file = caminho_do_texto(&self.log.get());
        c.keepalive_secs = self.keepalive.get().clamp(0.0, 3600.0).round() as u32;
        c.colar_arquivos = colar_do_rotulo(&self.colar_arquivos.get());
        c.pasta_colar = self.pasta_colar.get().trim().to_owned();
        c.shell_remoto = ShellRemoto::TODOS
            .into_iter()
            .find(|s| s.nome() == self.shell_remoto.get())
            .unwrap_or(base.shell_remoto);
        c.comprimir = self.comprimir.get();
        let escolha = |nome: String| move |rotulo: &str| rotulo == nome;
        let seguranca = escolha(self.ftp_seguranca.get());
        c.ftp_security = FtpSeguranca::TODAS
            .into_iter()
            .find(|k| seguranca(&k.nome()))
            .unwrap_or(base.ftp_security);
        let modo = escolha(self.ftp_modo.get());
        c.ftp_mode = FtpModo::TODOS
            .into_iter()
            .find(|k| modo(&k.nome()))
            .unwrap_or(base.ftp_mode);
        let tls = escolha(self.ftp_tls.get());
        c.ftp_tls = FtpTls::TODAS
            .into_iter()
            .find(|k| tls(&k.nome()))
            .unwrap_or(base.ftp_tls);
        let codificacao = escolha(self.ftp_codificacao.get());
        c.ftp_encoding = FtpCodificacao::TODAS
            .into_iter()
            .find(|k| codificacao(&k.nome()))
            .unwrap_or(base.ftp_encoding);
        c.ftp_timezone_min = self.ftp_fuso.get().clamp(-840.0, 840.0).round() as i32;
        c.ftp_active_ports = self.ftp_portas.get().trim().to_owned();
        let provedor = escolha(self.s3_provedor.get());
        let estilo = escolha(self.s3_estilo.get());
        let certificado = escolha(self.s3_certificado.get());
        c.s3 = S3Opcoes {
            provider: S3Provedor::TODOS
                .into_iter()
                .find(|k| provedor(&k.nome()))
                .unwrap_or(base.s3.provider),
            endpoint: self.s3_endpoint.get().trim().to_owned(),
            region: self.s3_regiao.get().trim().to_owned(),
            bucket: self.s3_bucket.get().trim().to_owned(),
            url_style: S3Estilo::TODOS
                .into_iter()
                .find(|k| estilo(&k.nome()))
                .unwrap_or(base.s3.url_style),
            storage_class: self.s3_classe.get().trim().to_owned(),
            certificate: S3Certificado::TODOS
                .into_iter()
                .find(|k| certificado(&k.nome()))
                .unwrap_or(base.s3.certificate),
            ca_file: caminho_do_texto(&self.s3_ca.get()),
        };
        // As linhas do S3 só valem para o S3 (fora dele ficam em branco, ver `carregar`).
        if c.kind != Kind::S3 {
            c.s3 = base.s3.clone();
        }
        c.rclone = RcloneOpcoes {
            remote: self.rclone_remoto.get().trim().to_owned(),
            path: self.rclone_caminho.get().trim().to_owned(),
        };
        c.url = self.url.get().trim().to_owned();
        c.x11 = modo_x11_do_rotulo(&self.x11.get());
        c.tuneis = self.tuneis.get();
        c.so_tuneis = self.so_tuneis.get();
        c.comandos = self.comandos.get();
        c.parar_se_tunel_falhar = self.parar_se_falhar.get();
        c.pasta_local = self.pasta_local.get().trim().to_owned();
        c.pasta_remota = self.pasta_remota.get().trim().to_owned();
        c.comparar = ativar_do_rotulo(&self.comparar.get());
        c.sincronizar = ativar_do_rotulo(&self.sincronizar_pastas.get());
        c.local_programa = self.local_programa.get().trim().to_owned();
        c.local_argumentos = self.local_argumentos.get().trim().to_owned();
        c.local_pasta = self.local_pasta.get().trim().to_owned();
        c.ia.pasta = self.ia_pasta.get().trim().to_owned();
        c.ia.metodo = ia::metodo_do_rotulo(&self.ia_metodo.get());
        c.ia.argumentos.clear();
        for (campo, f) in self.ia_argumentos.iter().zip(crate::ia::Ferramenta::TODAS) {
            let texto = campo.get().trim().to_owned();
            if !texto.is_empty() {
                c.ia.argumentos.insert(f, texto);
            }
        }
        let salto = self.jump_host.get().trim().to_owned();
        let escolhida = self.credencial.get();
        let escolhida_do_salto = self.jump_credencial.get();
        // Com o cofre bloqueado a lista só tem "Perguntar": não dá para saber o que a pessoa
        // quis, e a ligação que já existe fica como está.
        let bloqueado = cofre.agora(|c| c.bloqueado());
        if !bloqueado {
            c.credencial = id_do_rotulo(&escolhida);
            // Um item do cofre volta o modo ao padrão (o item vence), e fora do SSH e do SFTP o
            // modo não vale: fica o "Herdar", que não vai para o arquivo.
            c.credencial_modo = if c.kind.usa_ssh() {
                ModoCredencial::do_rotulo(&escolhida)
            } else {
                ModoCredencial::Herdar
            };
            c.chave_do_cofre = uid_da_chave_do_cofre(cofre, &self.chave_cofre.get());
        }
        c.jump = (!salto.is_empty()).then(|| Jump {
            host: salto,
            port: como_porta(self.jump_porta.get()),
            user: self.jump_usuario.get().trim().to_owned(),
            credencial: if bloqueado {
                base.jump.as_ref().and_then(|j| j.credencial)
            } else {
                id_do_rotulo(&escolhida_do_salto)
            },
        });
        c
    }
}

/// O id da credencial do servidor ou do salto.
fn alvo_credencial(c: &Connection, alvo: Alvo) -> Option<u64> {
    match alvo {
        Alvo::Conexao => c.credencial,
        Alvo::Salto => c.jump.as_ref().and_then(|j| j.credencial),
    }
}

fn como_porta(valor: f64) -> u16 {
    valor.clamp(0.0, 65535.0).round() as u16
}

fn texto_do_caminho(caminho: Option<&PathBuf>) -> String {
    caminho
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn caminho_do_texto(texto: &str) -> Option<PathBuf> {
    let texto = texto.trim();
    (!texto.is_empty()).then(|| PathBuf::from(texto))
}

/// A função que abre o diálogo de copiar a chave, com a privada já escolhida, se vier.
pub(crate) type AbrirCopiar = Rc<dyn Fn(Option<crate::tela_chaves::DeOnde>)>;

/// A conexão, a pasta remota e a pasta local em que uma aba de arquivos abre.
type InicialDeArquivos = (u64, Option<String>, Option<String>);

/// Escolhe a conexão `id` na árvore: abre os grupos que a escondem e rola até a linha, sem
/// tirar o foco de onde está. Devolve `false` se a conexão não está na árvore.
fn escolher_na_arvore(
    tree: Signal<Tree>,
    abertos: Signal<BTreeSet<String>>,
    selecionado: Signal<Option<String>>,
    id: u64,
) -> bool {
    let Some(grupos) = tree.with_untracked(|t| t.connection(id).map(|_| t.ancestors(id))) else {
        return false;
    };
    let faltam: Vec<String> = abertos.with_untracked(|a| {
        grupos
            .iter()
            .map(u64::to_string)
            .filter(|g| !a.contains(g))
            .collect()
    });
    if !faltam.is_empty() {
        abertos.update(|a| a.extend(faltam));
    }
    let chave = id.to_string();
    if selecionado.with_untracked(|s| s.as_deref() != Some(chave.as_str())) {
        selecionado.set(Some(chave.clone()));
    }
    tomni_ui::start_bring_into_view(format!("arvore-{chave}"));
    true
}

/// O que a tela guarda e usa nos botões, menus e efeitos.
struct Tela {
    armazenamento: Armazenamento,
    opener: Rc<dyn Opener>,
    tree: Signal<Tree>,
    itens: Signal<Vec<TreeItem>>,
    selecionado: Signal<Option<String>>,
    abertos: Signal<BTreeSet<String>>,
    filtro: Signal<String>,
    /// A lupa da barra da árvore: ligada, mostra o campo do filtro.
    filtro_aberto: Signal<bool>,
    /// O resumo da linha "Túneis" das propriedades ("Nenhum", "1 túnel"…).
    resumo_tuneis: Signal<String>,
    /// O resumo da linha "Comandos" das propriedades.
    resumo_comandos: Signal<String>,
    renomeando: Signal<Option<String>>,
    campos: Campos,
    /// As janelas de terminais abertas (a principal e as secundárias).
    janelas: Janelas,
    /// A última janela com o foco do sistema.
    ativa: Memo<WindowHandle>,
    /// Os espaços de trabalho guardados em `pasta/espacos`.
    espacos: Espacos,
    /// O diálogo de salvar um espaço está aberto.
    salvar_aberto: Signal<bool>,
    /// O nome digitado no diálogo de salvar.
    salvar_nome: Signal<String>,
    /// A pasta escolhida no diálogo de salvar: [`raiz_dos_espacos`], o caminho de uma pasta
    /// ou [`nova_pasta_opcao`].
    salvar_pasta: Signal<String>,
    /// O nome da pasta nova, quando a escolha é [`nova_pasta_opcao`].
    salvar_nova_pasta: Signal<String>,
    /// O que o diálogo de salvar não aceitou (nome inválido).
    salvar_erro: Signal<String>,
    /// "3 abas em 2 janelas": o que o diálogo vai salvar.
    salvar_resumo: Signal<String>,
    /// O espaço que o diálogo de salvar vai gravar.
    a_salvar: RefCell<Option<Espaco>>,
    /// O que fazer quando o diálogo de salvar fecha.
    depois_de_salvar: Cell<Depois>,
    /// A pergunta "Substituir?" está aberta.
    substituir_aberto: Signal<bool>,
    /// A pasta e o nome do espaço que já existe e que a pergunta quer substituir.
    a_substituir: RefCell<Option<(String, String)>>,
    /// A confirmação de apagar um espaço ou uma pasta de espaços está aberta.
    apagar_espaco: Signal<bool>,
    contador: Signal<u32>,
    aviso: Signal<Option<(InfoBarSeverity, String)>>,
    apagar: Signal<bool>,
    pendentes: Signal<Vec<Pedido>>,
    cofre: CofreCompartilhado,
    /// As sessões abertas com senha salva, à espera de uma recusa do servidor.
    vigiadas: Signal<Vec<Vigiada>>,
    /// A página da navegação (`inicio`, `terminais`, `cofre`, `sobre`, `configuracoes`).
    pagina: Signal<String>,
    /// O que foi aberto por último (o `recentes.json`).
    recentes: Signal<Recentes>,
    /// As configurações do app (o `configuracoes.json`).
    config: Signal<Configuracoes>,
    /// O texto da busca da página Configurações (a busca global também o escreve).
    busca_config: Signal<String>,
    /// O que a leitura do `configuracoes.json` corrigiu, para a página mostrar.
    avisos_config: Signal<Vec<String>>,
    /// As abas da última execução esperam o cofre ser desbloqueado.
    adiado: Signal<bool>,
    /// As ferramentas de chaves: o agente embutido, a pasta `~/.ssh` e a fila de passphrases.
    ferramentas: Rc<Ferramentas>,
    /// O diálogo de desbloqueio do cofre está aberto (ou a tela espera ele voltar).
    desbloquear: Signal<bool>,
    /// A senha mestra do cofre estava errada e o diálogo volta no mesmo instante.
    refazer_desbloqueio: Rc<Cell<bool>>,
    /// Abre o diálogo "Copiar a chave para o servidor" (a chave privada já escolhida, se vier).
    copiar_chave: RefCell<Option<AbrirCopiar>>,
    /// O "Fechar todas as janelas?" está na tela: as secundárias deixam a principal perguntar.
    fechar_tudo: Signal<bool>,
    /// O "Fechar o Tirech Omni?" (deixar na bandeja ou fechar) está na tela.
    fechar_ou_bandeja: Signal<bool>,
    /// A caixa "Não perguntar de novo" desse diálogo.
    nao_perguntar: Signal<bool>,
    /// O "Fechar todas as abas?" (M42).
    abas: abas_caidas::Estado,
    /// O zoom da fonte de cada aba (Ctrl+roda, Ctrl+`=`, Ctrl+`-`, Ctrl+`0`).
    zoom_fonte: ZoomFonte,
    /// O diálogo "Abrir em modo cluster" (M42).
    cluster: cluster::Estado,
    /// A página Scripts (M42).
    scripts: scripts::Estado,
    /// O lançador "Links…".
    links: lancador_links::Estado,
    /// As distribuições do WSL (M42).
    wsl: wsl::Estado,
    /// O diálogo "Instalar no computador" (M65).
    instalar: instalar::Estado,
    /// Os assistentes de IA no servidor (M46).
    ia: ia::Estado,
    /// Duplicar a aba na pasta atual (M64).
    duplicar: duplicar::Estado,
    /// A sessão persistente (tmux ou screen) das abas SSH.
    tmux: sessao_tmux::Estado,
    /// A lista "Sessões no servidor…" de uma conexão SSH.
    sessoes_servidor: sessoes_servidor::Estado,
    /// Os downloads do arquivo marcado no terminal.
    baixar: baixar::Estado,
    /// O "Controlar com" um assistente deste computador (M63).
    controlar: controlar::Estado,
    /// As janelas estão escondidas na bandeja.
    escondido: Signal<bool>,
    /// O diálogo "Atalhos de teclado", que mora na janela principal (o menu Ajuda de qualquer
    /// janela o abre).
    atalhos_aberto: Signal<bool>,
    /// Os estados das sessões de cada conexão, para o selo da árvore (M62).
    estados_das_conexoes: Memo<HashMap<u64, Vec<EstadoDaAba>>>,
    /// A principal foi mostrada só para uma pergunta do agente e volta a se esconder depois.
    so_para_pergunta: Cell<bool>,
    /// Já passou a espera de conferir a bandeja (o aviso "sem bandeja" só vale depois).
    bandeja_conferida: Signal<bool>,
    /// Os diálogos do cofre entre máquinas (M44).
    portatil: cofre_portatil::Estado,
    /// Os diálogos de importar e de exportar (M38).
    importacao: importacao::Estado,
    /// O que a página Notas lembra (M47): a nota aberta, o modo e os tamanhos.
    notas: EstadoNotas,
    /// Os painéis encaixáveis da página Terminais: a árvore e as propriedades (M54).
    paineis_terminais: PaineisDaPagina,
    /// Os painéis da página Notas: a lista de notas (M54).
    paineis_notas: PaineisDaPagina,
    /// Os painéis da página Arquivos: a fila de transferências (M54).
    paineis_arquivos: PaineisDaPagina,
    /// A nota que a página Notas deve abrir (pedida pelo Cofre, pela busca ou pelas conexões).
    pedido_de_nota: Signal<Option<u64>>,
    /// O que grava a nota pendente da página Notas, para a janela fechar sem perder texto.
    gravar_nota: AoFechar,
    /// O que espera o cofre ser desbloqueado (abrir ou criar a nota de uma conexão).
    depois_de_desbloquear: RefCell<Option<DepoisDoDesbloqueio>>,
    /// Ligar os logs cifrados abriu o diálogo de senha mestra: se ele fecha com o cofre trancado,
    /// a opção volta a desligada.
    logs_esperando_o_cofre: Cell<bool>,
    /// A página Logs (M53).
    logs_pagina: pagina_logs::Estado,
    /// A pessoa confirmou o fim do app: nenhuma janela pergunta mais.
    liberado: Cell<bool>,
    /// O app está acabando: o estado das janelas já foi gravado e não muda mais.
    encerrando: Cell<bool>,
    /// Outra cópia do app, aberta antes, é a dona da configuração (`Ambiente::dono_do_estado`):
    /// esta abre só de leitura ([`crate::armazenamento::somente_leitura_em`]), sem as abas da
    /// outra. As conexões, as configurações, os provedores de IA e o cofre que se mudam nela vão
    /// para a outra gravar ([`crate::instancias`]); o resto fica na memória e se perde ao fechar.
    so_leitura: bool,
    /// Numa cópia só de leitura, se a dona ainda está aberta. Quando ela fecha, a barra de só
    /// leitura convida a reiniciar.
    dona_aberta: Signal<bool>,
    /// O que se viu do `connections.json` na última leitura ou gravação: a vigia relê o arquivo
    /// quando outro programa (a outra cópia do app, um editor) o muda.
    visto_conexoes: RefCell<crate::armazenamento::Visto>,
    /// O id do item do menu em realce (o submenu "Abrir em" aberto): os painéis da janela de
    /// destino mostram o número deles enquanto isso. Um signal só, porque só um menu fica aberto.
    realce_menu: Signal<Option<String>>,
    /// O número do último pedido de "Identificar os painéis" que ainda vale (`None`: desligado):
    /// os números ficam na tela até o alarme desse pedido.
    identificando: Signal<Option<u64>>,
    /// A fila de transferências da página Arquivos.
    transferencias: Transfers,
    /// As esperas que voltam à thread da interface (limpar a área de transferência, bloquear o
    /// cofre, reconectar).
    alarmes: Alarmes,
    /// As abas da página Arquivos.
    abas_arquivos: Signal<Vec<crate::arquivos::Aba>>,
    /// A chave da aba escolhida na página Arquivos.
    aba_arquivos: Signal<String>,
    /// Quantas abas de arquivos já nasceram (entra na chave da aba).
    proxima_aba: Cell<u32>,
    /// As pastas em que as abas de arquivos esperando a senha devem abrir: a conexão, a pasta
    /// remota e a local.
    iniciais_arquivos: RefCell<Vec<InicialDeArquivos>>,
    /// Os arquivos remotos abertos no editor externo.
    edicoes: RefCell<Vec<crate::arquivos::editar::Edicao>>,
    /// Imagem e arquivos colados nos terminais (M45).
    colar: colar::EstadoColar,
    /// Quantas edições já nasceram (o número da próxima).
    proxima_edicao: Cell<u64>,
    /// Quem vigia a data dos arquivos das edições.
    vigia: crate::arquivos::editar::Vigia,
    /// As edições cujo arquivo mudou e esperam a resposta "Enviar?" (a primeira é a mostrada).
    edicao_pergunta: Signal<Vec<u64>>,
    /// A aba de arquivos que o diálogo "Há N transferências" quer fechar.
    fechando_aba: RefCell<Option<String>>,
    /// O diálogo "Há N transferências" está aberto.
    fechando_aberto: Signal<bool>,
    /// Quantas transferências o diálogo conta.
    fechando_quantas: Signal<usize>,
    /// Os remotos do rclone: a subpágina da página Arquivos.
    rclone: rclone::EstadoRclone,
    /// Os perfis de rede em uso: quando o último uso de um perfil acaba, roda o comando de descer.
    usos: Usos,
    /// Os motores da rede Nebula embutida (M48).
    motores: Motores,
    /// Sobe cada vez que o estado de um motor Nebula muda (a página Redes relê o estado).
    nebula_mudou: Signal<u64>,
    /// Os motores da rede OpenVPN embutida (M56). Acordam a tela pelo mesmo `nebula_mudou`.
    motores_vpn: MotoresVpn,
    /// Os motores da rede WireGuard embutida (M57). Acordam a tela pelo mesmo `nebula_mudou`.
    motores_wireguard: MotoresWireGuard,
    /// Os nós da rede Tailscale embutida (M57). Acordam a tela pelo mesmo `nebula_mudou`.
    motores_tailscale: MotoresTailscale,
    /// Os nós da rede ZeroTier embutida (M61). Acordam a tela pelo mesmo `nebula_mudou`.
    motores_zerotier: MotoresZeroTier,
    /// O perfil de rede por onde cada sessão de terminal saiu (só as que têm proxy).
    rede_das_sessoes: RefCell<HashMap<Session, PerfilRede>>,
    /// As abas caídas com uma reconexão sozinha marcada: a tentativa e o total
    /// (`conexoes.reconectar`). O `listar_abas` do MCP mostra como "reconectando".
    reconectando: Rc<RefCell<HashMap<Session, (u32, u32)>>>,
    /// O perfil de rede de cada aba de arquivos, pela chave da aba.
    rede_das_abas: RefCell<HashMap<String, PerfilRede>>,
    /// A própria tela, para os alarmes e os efeitos que precisam dela depois.
    eu: Weak<Tela>,
    /// Os comandos ao conectar que rodam nas sessões (M50).
    comandos: comandos::EstadoComandos,
    /// O editor de túneis e a lista dos túneis ativos (M37).
    tuneis: tuneis::EstadoTuneis,
    editor_comandos: comandos_editor::EstadoEditorComandos,
    /// Os programas gráficos sem aba (M37).
    programas: programas::EstadoProgramas,
    /// Os provedores de IA (o `ia.json`) e o que a página deles escolheu.
    assistente: assistente::EstadoAssistente,
    /// O canal entre as cópias do app na mesma pasta de dados.
    instancias: instancias::Estado,
    /// O servidor MCP dos assistentes de código (M51).
    mcp: mcp::EstadoMcp,
    /// O painel de portas do servidor (M58).
    portas: portas::EstadoPortas,
}

impl Tela {
    /// O id da conexão ou do grupo escolhido na árvore.
    fn id_selecionado(&self) -> Option<u64> {
        self.selecionado
            .get_untracked()
            .and_then(|chave| chave.parse().ok())
    }

    /// A conexão escolhida (`None` se for um grupo ou nada).
    fn conexao_selecionada(&self) -> Option<Connection> {
        let id = self.id_selecionado()?;
        self.tree.with_untracked(|t| t.connection(id).cloned())
    }

    fn avisar(&self, severidade: InfoBarSeverity, mensagem: impl Into<String>) {
        self.aviso.set(Some((severidade, mensagem.into())));
    }

    /// Grava o `connections.json`; se falhar, avisa na tela. Se outro programa o mudou depois da
    /// última leitura (e a vigia ainda não releu), a versão dele fica numa cópia ao lado, com o
    /// aviso.
    fn gravar(&self) {
        let resultado = self.tree.with_untracked(|t| {
            self.armazenamento
                .salvar_visto(t, &mut self.visto_conexoes.borrow_mut())
        });
        match resultado {
            Ok(None) => {}
            Ok(Some(copia)) => self.avisar(
                InfoBarSeverity::Warning,
                t!(
                    "armazenamento-conexoes-conflito",
                    copia = copia.display().to_string()
                ),
            ),
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!(
                    "comum-nao-consegui-gravar",
                    arquivo = self.armazenamento.arquivo_conexoes().display().to_string(),
                    erro = erro.to_string()
                ),
            ),
        }
    }

    /// Liga a vigia do `connections.json`: de tempos em tempos (`Ambiente.vigia_do_cofre`, o
    /// mesmo ritmo da do cofre) e quando a janela ganha o foco.
    fn vigiar_conexoes(self: &Rc<Self>) {
        self.agendar_vigia_das_conexoes();
        let t = Rc::clone(self);
        effect(move || {
            let _ = t.ativa.get();
            untrack(|| t.conferir_conexoes());
        });
    }

    fn agendar_vigia_das_conexoes(self: &Rc<Self>) {
        let espera = self.ferramentas.ambiente().vigia_do_cofre;
        let t = Rc::clone(self);
        self.alarmes.depois(espera, move || {
            t.conferir_conexoes();
            t.agendar_vigia_das_conexoes();
        });
    }

    /// Relê o `connections.json` se outro programa o mudou, e numa cópia só de leitura confere se
    /// a dona ainda está aberta.
    fn conferir_conexoes(&self) {
        let lido = self
            .armazenamento
            .reler_se_mudou(&mut self.visto_conexoes.borrow_mut());
        if let Some((arvore, aviso)) = lido {
            self.tree.set(arvore);
            self.refazer_itens();
            if let Some(aviso) = aviso {
                self.avisar(InfoBarSeverity::Warning, aviso);
            }
        }
        // A trava só é tomada para saber se está livre, e sai na hora: esta cópia continua só de
        // leitura até reiniciar.
        if self.so_leitura
            && self.dona_aberta.get_untracked()
            && let Ok(crate::armazenamento::Trava::Travada(trava)) =
                crate::armazenamento::travar(self.armazenamento.pasta())
        {
            drop(trava);
            self.dona_aberta.set(false);
        }
    }

    /// Depois de mudar a árvore: grava e redesenha a lista.
    fn atualizar(&self) {
        self.gravar();
        self.refazer_itens();
    }

    /// Redesenha a lista com as conexões e os espaços de agora (relê a pasta dos espaços).
    fn refazer_itens(&self) {
        let itens = self
            .tree
            .with_untracked(|t| itens_completos(&t.nodes, &self.espacos));
        self.itens.set(itens);
        self.podar_recentes();
    }

    /// Tira dos recentes as conexões que a árvore não tem mais.
    fn podar_recentes(&self) {
        let existentes: HashSet<u64> = self
            .tree
            .with_untracked(|t| t.connections().iter().map(|c| c.id).collect());
        let existe = |alvo: &AlvoRecente| match alvo {
            AlvoRecente::Conexao { id } => existentes.contains(id),
            AlvoRecente::Espaco { caminho } => self.espacos.existe_caminho(caminho),
            AlvoRecente::Aplicativo { conexao, .. } => existentes.contains(conexao),
            _ => true,
        };
        let sobra = self
            .recentes
            .with_untracked(|r| r.lista().iter().any(|i| !existe(&i.alvo)));
        if sobra {
            self.recentes.update(|r| r.manter_se(existe));
        }
    }

    /// `usuario@host:porta` da conexão (o nome do tipo, no shell local), o detalhe do cartão.
    fn detalhe_de(c: &Connection) -> String {
        if c.kind == Kind::Local {
            return t!("comum-tipo-shell-local");
        }
        if c.kind == Kind::Rclone {
            return c.rclone.spec();
        }
        if c.kind == Kind::S3 {
            let servico = c.s3.endpoint.split("://").last().unwrap_or_default();
            return match c.s3.bucket.is_empty() {
                true => servico.to_owned(),
                false => format!("{}@{servico}", c.s3.bucket),
            };
        }
        let servidor = if c.user.is_empty() {
            c.host.clone()
        } else {
            format!("{}@{}", c.user, c.host)
        };
        if servidor.is_empty() {
            return c.kind.nome().to_owned();
        }
        format!("{servidor}:{}", c.port)
    }

    /// Guarda a abertura de `c` nos recentes.
    fn registrar_recente(&self, c: &Connection) {
        let detalhe = Self::detalhe_de(c);
        self.recentes.update(|r| {
            r.registrar(AlvoRecente::Conexao { id: c.id }, &c.name, &detalhe);
        });
    }

    /// Os cartões do Início: os recentes de conexões que ainda existem, com o nome e o
    /// detalhe de agora (um renomear aparece na hora). A volta ao Início refaz o "há 5 min".
    fn cartoes(&self) -> Vec<Cartao> {
        let _ = self.pagina.get();
        let agora = recentes::agora();
        let lista = self.recentes.with(|r| r.lista().to_vec());
        let (mut fixadas, mut soltas) = (0, 0);
        let mut cartoes = Vec::new();
        for recente in lista {
            let (icone, titulo, detalhe) = match &recente.alvo {
                AlvoRecente::Conexao { id } => {
                    let Some(c) = self.tree.with(|t| t.connection(*id).cloned()) else {
                        continue;
                    };
                    (
                        icone_do_cartao(c.kind),
                        c.name.clone(),
                        Self::detalhe_de(&c),
                    )
                }
                AlvoRecente::Espaco { .. } => (
                    Symbol::Apps,
                    recente.titulo.clone(),
                    recente.detalhe.clone(),
                ),
                AlvoRecente::Local { .. } => (
                    Symbol::FolderOpen,
                    recente.titulo.clone(),
                    recente.detalhe.clone(),
                ),
                AlvoRecente::Aplicativo { conexao, .. } => {
                    if !self.tree.with(|t| t.connection(*conexao).is_some()) {
                        continue;
                    }
                    (
                        Symbol::Apps,
                        recente.titulo.clone(),
                        recente.detalhe.clone(),
                    )
                }
            };
            let indice = if recente.fixado {
                &mut fixadas
            } else {
                &mut soltas
            };
            cartoes.push(Cartao {
                indice: *indice,
                alvo: recente.alvo,
                icone,
                titulo,
                detalhe,
                quando: recentes::quando(recente.ultimo, agora),
                fixado: recente.fixado,
            });
            *indice += 1;
        }
        cartoes
    }

    /// Abre o alvo de um recente e vai para os Terminais.
    fn abrir_recente(self: &Rc<Self>, alvo: &AlvoRecente) {
        match alvo {
            AlvoRecente::Conexao { id } => self.abrir_e_mostrar(*id),
            AlvoRecente::Espaco { caminho } => self.abrir_espaco(caminho, ComoAbrir::Padrao),
            AlvoRecente::Local { conexao, caminho } => {
                self.abrir_arquivos_em(*conexao, Some(caminho.clone()), None, true);
            }
            AlvoRecente::Aplicativo { conexao, comando } => {
                self.executar_aplicativo(*conexao, comando.clone(), false);
            }
        }
    }

    /// Abre a conexão e troca para a página Terminais, onde a aba aparece.
    fn abrir_e_mostrar(self: &Rc<Self>, id: u64) {
        self.abrir(id, self.lugar_novo());
        // Os arquivos vão à página Arquivos e o link ao navegador: a tela fica onde está.
        let sem_aba = self.tree.with_untracked(|t| {
            t.connection(id)
                .is_some_and(|c| c.kind.so_arquivos() || c.kind == Kind::Link)
        });
        if !sem_aba {
            self.mostrar_terminais();
        }
    }

    /// Escolhe a conexão na árvore, abrindo os grupos que a escondem, e vai para os Terminais.
    fn mostrar_na_arvore(&self, id: u64) {
        self.filtro.set(String::new());
        escolher_na_arvore(self.tree, self.abertos, self.selecionado, id);
        self.pagina.set("terminais".to_owned());
    }

    /// O clique numa aba com "Mostrar a conexão da aba na árvore" ligado: mostra a conexão dela
    /// mesmo quando a aba já era a ativa (a troca de aba fica com `aplicar_mostrar_aba_na_arvore`).
    fn mostrar_aba_clicada(&self, sessao: Session) {
        if self
            .config
            .with_untracked(|c| c.conexoes.mostrar_aba_na_arvore)
            && let Some(id) = id_da_chave(&sessao.key_str())
        {
            escolher_na_arvore(self.tree, self.abertos, self.selecionado, id);
        }
    }

    /// O shell local aberto pelo Início ou pelo menu: vai para os Terminais.
    fn abrir_shell_e_mostrar(self: &Rc<Self>) {
        self.abrir_shell_local(self.lugar_novo());
        self.mostrar_terminais();
    }

    fn abre_grupo(&self, grupo: Option<u64>) {
        if let Some(grupo) = grupo {
            self.abertos.update(|a| {
                a.insert(grupo.to_string());
            });
        }
    }

    fn nova_conexao(&self) {
        let pai = self
            .tree
            .with_untracked(|t| t.group_for_new(self.id_selecionado()));
        let config = self.config.get_untracked();
        let id = self.tree.try_update(|t| {
            let n = t.connection_count() + 1;
            t.add_connection(
                pai,
                Connection::com_config(t!("casca-nova-conexao-n", n = n), Kind::Ssh, &config),
            )
        });
        self.abre_grupo(pai);
        self.filtro.set(String::new());
        self.atualizar();
        self.selecionado.set(id.map(|id| id.to_string()));
    }

    fn novo_grupo(&self) {
        let pai = self
            .tree
            .with_untracked(|t| t.group_for_new(self.id_selecionado()));
        let id = self
            .tree
            .try_update(|t| t.add_group(pai, t!("casca-novo-grupo")));
        self.abre_grupo(pai);
        self.filtro.set(String::new());
        self.atualizar();
        let chave = id.map(|id| id.to_string());
        self.selecionado.set(chave.clone());
        self.renomeando.set(chave);
    }

    fn subir(&self) {
        if let Some(id) = self.id_selecionado() {
            self.tree.update(|t| {
                t.move_up(id);
            });
            self.atualizar();
        }
    }

    fn descer(&self) {
        if let Some(id) = self.id_selecionado() {
            self.tree.update(|t| {
                t.move_down(id);
            });
            self.atualizar();
        }
    }

    fn confirmar_apagar(&self) {
        if let Some(id) = self.id_selecionado() {
            self.tree.update(|t| {
                t.remove(id);
            });
            self.selecionado.set(None);
            self.atualizar();
        }
    }

    /// Copia a conexão `id` para logo abaixo dela, escolhe a cópia e entra em renomear. A senha
    /// que só esta conexão usa ganha um item próprio no cofre (trocar a da cópia não muda a da
    /// original); a credencial que outras conexões também usam continua compartilhada.
    fn duplicar_conexao(&self, id: u64) {
        let Some(original) = self.tree.with_untracked(|t| t.connection(id).cloned()) else {
            return;
        };
        let nome = t!("comum-titulo-copia", titulo = original.name.as_str());
        let Some(copia) = self
            .tree
            .try_update(|t| t.duplicate_connection(id, nome))
            .flatten()
        else {
            return;
        };
        if !self.cofre.agora(|x| x.bloqueado()) {
            for alvo in [Alvo::Conexao, Alvo::Salto] {
                self.separar_credencial(&original, copia, alvo);
            }
        }
        self.filtro.set(String::new());
        self.atualizar();
        let chave = copia.to_string();
        self.selecionado.set(Some(chave.clone()));
        self.renomeando.set(Some(chave));
    }

    /// Dá à `copia` um item próprio do cofre, cópia do da `original` (do servidor ou do salto),
    /// se nenhuma outra conexão usa aquele item.
    fn separar_credencial(&self, original: &Connection, copia: u64, alvo: Alvo) {
        let Some(credencial) = alvo_credencial(original, alvo) else {
            return;
        };
        let usos = self.tree.with_untracked(|t| {
            t.connections()
                .iter()
                .flat_map(|c| {
                    [
                        alvo_credencial(c, Alvo::Conexao),
                        alvo_credencial(c, Alvo::Salto),
                    ]
                })
                .filter(|c| *c == Some(credencial))
                .count()
        });
        // A original e a cópia: 2. Mais que isso, o item já era de várias.
        if usos > 2 {
            return;
        }
        let Some(mut item) = self.cofre.agora(|x| x.item(credencial).cloned()) else {
            return;
        };
        item.id = 0;
        item.uid = String::new();
        item.titulo = t!("comum-titulo-copia", titulo = item.titulo.as_str());
        match self.cofre.mudar(|x| x.salvar(item)) {
            Ok(novo) => self.ligar(copia, alvo, novo),
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            ),
        }
    }

    fn mover(&self, chave: &str, pai: Option<&str>, posicao: usize) {
        let (Ok(id), pai) = (
            chave.parse::<u64>(),
            pai.and_then(|p| p.parse::<u64>().ok()),
        ) else {
            return;
        };
        let moveu = self
            .tree
            .try_update(|t| t.move_to(id, pai, posicao))
            .unwrap_or(false);
        if moveu {
            self.atualizar();
        }
    }

    /// O nome novo vindo do renomear da árvore. O rótulo da conexão leva ` (usuário@host)`;
    /// esse trecho não faz parte do nome.
    fn renomear(&self, chave: &str, texto: &str) {
        self.renomeando.set(None);
        let texto = texto.trim();
        let Ok(id) = chave.parse::<u64>() else {
            return;
        };
        let Some(no) = self.tree.with_untracked(|t| t.find(id).cloned()) else {
            return;
        };
        let nome = match &no {
            Node::Connection(c) => {
                let sufixo = c.label().strip_prefix(&c.name).unwrap_or("").to_owned();
                texto.strip_suffix(sufixo.as_str()).unwrap_or(texto).trim()
            }
            Node::Group(_) => texto,
        };
        if nome.is_empty() || nome == no.name() {
            self.atualizar();
            return;
        }
        if self.id_selecionado() == Some(id) && matches!(no, Node::Connection(_)) {
            // As propriedades têm o nome antigo: se só a árvore mudasse, o efeito das
            // propriedades desfaria a troca.
            self.campos.nome.set(nome.to_owned());
        } else {
            self.tree.update(|t| {
                t.rename(id, nome);
            });
            self.atualizar();
        }
    }

    /// Passa as propriedades para a conexão escolhida, grava e atualiza o rótulo.
    fn guardar_propriedades(&self) {
        let Some(atual) = self.conexao_selecionada() else {
            // Lê os signals mesmo assim, para o efeito continuar dependendo deles.
            let _ = self
                .campos
                .aplicar(&Connection::new("", Kind::Ssh), &self.cofre);
            let _ = self.campos.perfil_ia.get();
            self.guardar_rede_do_grupo();
            self.guardar_mcp_do_grupo();
            self.guardar_persistente_do_grupo();
            self.guardar_mouse_do_grupo();
            self.guardar_retentar_do_grupo();
            self.guardar_credencial_do_grupo();
            self.guardar_comandos_do_grupo();
            return;
        };
        let mut nova = self.campos.aplicar(&atual, &self.cofre);
        if nova.name.trim().is_empty() {
            nova.name = atual.name.clone();
        }
        nova.rede = self.rede_do_texto(atual.id, atual.rede, &self.campos.rede.get());
        nova.perfil_ia = self.perfil_ia_do_texto(atual.perfil_ia, &self.campos.perfil_ia.get());
        nova.mcp = self.mcp_do_texto(atual.id, atual.mcp, &self.campos.mcp.get());
        nova.persistente = tmux::do_rotulo(&self.campos.persistente.get());
        nova.mouse = crate::mouse::do_rotulo(&self.campos.mouse.get());
        nova.retentar = crate::retentar::do_rotulo(&self.campos.retentar.get(), atual.retentar);
        // Trocar o tipo (ou a segurança do FTP) troca a porta, se ela estava no padrão de antes.
        if (nova.kind != atual.kind || nova.ftp_security != atual.ftp_security)
            && atual.port == atual.porta_padrao()
        {
            nova.port = nova.porta_padrao();
            self.campos.porta.set(f64::from(nova.port));
        }
        // Virar S3 mostra as opções que a conexão já tinha (as da Amazon, numa conexão nova).
        if nova.kind == Kind::S3 && atual.kind != Kind::S3 {
            nova.s3 = atual.s3.clone();
            self.campos.mostrar_s3(&nova.s3);
        }
        // Trocar o provedor do S3 preenche o endereço, a região e o estilo; trocar a região leva
        // o endereço junto, se ele ainda era o do provedor.
        if nova.s3 != atual.s3 {
            let digitado = nova.s3.clone();
            atual.s3.acertar(&mut nova.s3);
            if nova.s3.provider != atual.s3.provider {
                self.config
                    .with_untracked(|c| nova.s3.padroes_do_outro(&c.s3));
            }
            if nova.s3 != digitado {
                self.campos.mostrar_s3(&nova.s3);
            }
        }
        if nova == atual {
            return;
        }
        self.tree.update(|t| {
            if let Some(c) = t.connection_mut(atual.id) {
                *c = nova.clone();
            }
        });
        self.atualizar();
        self.campos.sincronizar(&nova, &self.cofre);
    }

    /// Passa para o cofre as senhas editadas nas propriedades: grava no item da credencial ou,
    /// sem credencial, cria o item e liga.
    fn guardar_senhas(&self) {
        let senha = self.campos.senha.get();
        let senha_do_salto = self.campos.jump_senha.get();
        let token = self.campos.s3_token.get();
        untrack(|| {
            let Some(c) = self.conexao_selecionada() else {
                return;
            };
            if self.cofre.agora(|x| x.bloqueado()) {
                return;
            }
            self.gravar_senha_editada(&c, Alvo::Conexao, senha);
            self.gravar_senha_editada(&c, Alvo::Salto, senha_do_salto);
            if c.kind == Kind::S3 {
                self.gravar_token_editado(c.id, token);
            }
        });
    }

    /// Grava o token de sessão do S3 no campo extra do item da credencial. Só mexe se mudou:
    /// sem item, o token digitado cria um (como a senha faz).
    fn gravar_token_editado(&self, id: u64, novo: String) {
        let Some(c) = self.tree.with_untracked(|t| t.connection(id).cloned()) else {
            return;
        };
        if novo == token_da_credencial(&self.cofre, c.credencial) {
            return;
        }
        let existe = c
            .credencial
            .is_some_and(|id| self.cofre.agora(|x| x.item(id).is_some()));
        if !existe {
            if novo.is_empty() {
                return;
            }
            // O item nasce com a senha que já estiver digitada.
            self.guardar_senha(id, Alvo::Conexao, &self.campos.senha.get_untracked());
        }
        let Some(c) = self.tree.with_untracked(|t| t.connection(id).cloned()) else {
            return;
        };
        let Some(mut item) = c
            .credencial
            .and_then(|id| self.cofre.agora(|x| x.item(id).cloned()))
        else {
            return;
        };
        item.campos.retain(|campo| campo.nome != CAMPO_TOKEN);
        if !novo.is_empty() {
            item.campos.push(Campo {
                nome: CAMPO_TOKEN.to_owned(),
                valor: novo,
                oculto: true,
            });
        }
        if let Err(erro) = self.cofre.mudar(|x| x.salvar(item)) {
            self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            );
        }
    }

    fn gravar_senha_editada(&self, c: &Connection, alvo: Alvo, nova: String) {
        let credencial = alvo_credencial(c, alvo);
        if nova == senha_da_credencial(&self.cofre, credencial) {
            return;
        }
        let existe = credencial.is_some_and(|id| self.cofre.agora(|x| x.item(id).is_some()));
        if existe || !nova.is_empty() {
            self.guardar_senha(c.id, alvo, &nova);
        }
    }

    /// Grava `senha` no item da credencial da conexão `id` (do servidor ou do salto). Sem item,
    /// cria `Item { tipo: Ssh, titulo: "usuario@host", … }` na pasta "Conexões" e liga a
    /// credencial.
    fn guardar_senha(&self, id: u64, alvo: Alvo, senha: &str) {
        let Some(c) = self.tree.with_untracked(|t| t.connection(id).cloned()) else {
            return;
        };
        let (host, porta, usuario, credencial) = match alvo {
            Alvo::Conexao if c.kind == Kind::S3 => {
                (c.s3.endpoint.clone(), 0, c.user.clone(), c.credencial)
            }
            Alvo::Conexao => (c.host.clone(), c.port, c.user.clone(), c.credencial),
            Alvo::Salto => match &c.jump {
                Some(j) => (j.host.clone(), j.port, j.user.clone(), j.credencial),
                None => return,
            },
        };
        let existente = credencial.and_then(|id| self.cofre.agora(|x| x.item(id).cloned()));
        let novo = existente.is_none();
        let s3 = c.kind == Kind::S3 && alvo == Alvo::Conexao;
        let mut item = existente.unwrap_or_else(|| Item {
            tipo: if s3 { Tipo::S3 } else { Tipo::Ssh },
            titulo: if usuario.is_empty() {
                host.clone()
            } else {
                format!("{usuario}@{host}")
            },
            usuario,
            url: if s3 { host } else { format!("{host}:{porta}") },
            ..Item::default()
        });
        item.senha = senha.to_owned();
        let resultado = self.cofre.mudar(|x| {
            if item.id == 0 {
                item.pasta = x.garantir_caminho(None, &[t!("comum-conexoes").as_str()]);
            }
            x.salvar(item)
        });
        match resultado {
            Ok(item_id) if novo => self.ligar(id, alvo, item_id),
            Ok(_) => {}
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            ),
        }
    }

    /// Liga a conexão `id` (ou o salto dela) ao item `item` do cofre.
    fn ligar(&self, id: u64, alvo: Alvo, item: u64) {
        self.tree.update(|t| {
            if let Some(c) = t.connection_mut(id) {
                match alvo {
                    Alvo::Conexao => c.credencial = Some(item),
                    Alvo::Salto => {
                        if let Some(j) = &mut c.jump {
                            j.credencial = Some(item);
                        }
                    }
                }
            }
        });
        self.atualizar();
        if self.id_selecionado() == Some(id)
            && let Some(c) = self.tree.with_untracked(|t| t.connection(id).cloned())
        {
            self.campos.sincronizar(&c, &self.cofre);
            self.sincronizar_credencial_herdada(&c);
        }
    }

    /// As senhas que o cofre guarda para a conexão e o salto dela.
    fn senhas_do_cofre(&self, c: &Connection) -> Senhas {
        self.cofre.marcar_uso();
        let senha =
            |id: Option<u64>| Some(senha_da_credencial(&self.cofre, id)).filter(|s| !s.is_empty());
        Senhas {
            conexao: senha(c.credencial),
            salto: senha(c.jump.as_ref().and_then(|j| j.credencial)),
            chave: c
                .key_file
                .as_deref()
                .and_then(|arquivo| crate::agente::passphrase_do_cofre(&self.cofre, arquivo)),
            token: Some(token_da_credencial(&self.cofre, c.credencial)).filter(|t| !t.is_empty()),
            chave_cofre: c
                .chave_do_cofre
                .as_deref()
                .and_then(|uid| self.cofre.agora(|x| x.chave_privada_por_uid(uid))),
            rede: self.rede_resolvida(c),
        }
    }

    /// A rede do perfil por onde a conexão sai, com o usuário e a senha do proxy e do salto que
    /// o cofre guarda. `None`: sai direto (sem perfil, ou um tipo que o perfil não alcança: o
    /// shell local).
    fn rede_resolvida(&self, c: &Connection) -> Option<RedeResolvida> {
        if c.kind == Kind::Local {
            return None;
        }
        let perfil = self
            .tree
            .with_untracked(|t| rede_efetiva(t, c.id).cloned())?;
        Some(self.resolver_rede(perfil))
    }

    /// O perfil de rede com o usuário e a senha do proxy e do salto que o cofre guarda.
    pub(super) fn resolver_rede(&self, mut perfil: PerfilRede) -> RedeResolvida {
        let item = |id: Option<u64>| {
            id.and_then(|id| {
                self.cofre
                    .agora(|x| x.item(id).map(|i| (i.usuario.clone(), i.senha.clone())))
            })
        };
        let (proxy_usuario, proxy_senha) = item(perfil.proxy.as_ref().and_then(|p| p.credencial))
            .map_or((None, None), |(u, s)| {
                (
                    Some(u).filter(|u| !u.is_empty()),
                    Some(s).filter(|s| !s.is_empty()),
                )
            });
        let credencial_do_salto = perfil.salto.as_ref().and_then(|j| j.credencial);
        let salto_senha =
            Some(senha_da_credencial(&self.cofre, credencial_do_salto)).filter(|s| !s.is_empty());
        if let Some(j) = &mut perfil.salto
            && j.user.is_empty()
            && let Some((u, _)) = item(credencial_do_salto)
        {
            j.user = u;
        }
        let known_hosts = Some(known_hosts_do_perfil(self.armazenamento.pasta(), perfil.id));
        let mut rede = RedeResolvida {
            perfil,
            proxy_usuario,
            proxy_senha,
            salto_senha,
            known_hosts,
            nebula: None,
            nebula_erro: None,
            openvpn: None,
            openvpn_erro: None,
            motor: None,
            motor_erro: None,
            emprestimo: None,
        };
        if self.emprestar_da_principal(&mut rede) {
            return rede;
        }
        if rede.perfil.nebula.is_some() {
            self.ligar_nebula(&mut rede);
        } else if rede.perfil.openvpn.is_some() {
            self.ligar_openvpn(&mut rede);
        } else if rede.perfil.wireguard.is_some() {
            let resultado = self
                .preparar_wireguard(&rede.perfil)
                .map(MotorRede::WireGuard);
            self.usar_motor(&mut rede, resultado);
        } else if rede.perfil.tailscale.is_some() {
            let resultado = self
                .preparar_tailscale(&rede.perfil)
                .map(MotorRede::Tailscale);
            self.usar_motor(&mut rede, resultado);
        } else if rede.perfil.zerotier.is_some() {
            let resultado = self
                .preparar_zerotier(&rede.perfil)
                .map(MotorRede::ZeroTier);
            self.usar_motor(&mut rede, resultado);
        }
        rede
    }

    /// Faz da ponte do motor embutido (WireGuard ou Tailscale) o proxy da rede, com as mesmas
    /// regras do Nebula: se o motor não sobe, o proxy fica numa porta que não existe e o erro vai
    /// para o `preparar` (a conexão nunca sai direto).
    fn usar_motor(&self, rede: &mut RedeResolvida, resultado: Result<MotorRede, String>) {
        match resultado {
            Ok(motor) => {
                if let tomni_ui::terminal::Proxy::Socks5 {
                    host,
                    port,
                    user,
                    password,
                } = motor.proxy()
                {
                    rede.perfil.proxy = Some(ProxyRede {
                        tipo: TipoProxy::Socks5,
                        host,
                        port,
                        credencial: None,
                    });
                    rede.proxy_usuario = user;
                    rede.proxy_senha = password;
                }
                rede.motor = Some(motor);
            }
            Err(erro) => {
                rede.perfil.proxy = Some(ProxyRede {
                    port: 0,
                    ..ProxyRede::novo()
                });
                rede.proxy_usuario = None;
                rede.proxy_senha = None;
                rede.motor_erro = Some(erro);
            }
        }
    }

    /// O erro de uma identidade de rede que não está no cofre: `sumiu` se o item `id` não existe
    /// mais; se existe sem a identidade (criado à mão no cofre com o tipo), diz isso.
    fn identidade_que_falta(&self, id: u64, perfil: &PerfilRede, sumiu: String) -> String {
        match self
            .cofre
            .agora(|c| c.item(id).map(|i| (i.titulo.clone(), i.tipo)))
        {
            Some((item, tipo)) => t!(
                "redes-item-sem-identidade",
                item = item,
                tipo = tipo.nome(),
                nome = perfil.name.as_str()
            ),
            None => sumiu,
        }
    }

    /// Monta o `WgConfig` do perfil WireGuard com a chave que o cofre guarda e liga o motor.
    fn preparar_wireguard(&self, perfil: &PerfilRede) -> Result<MotorWireGuard, String> {
        let rede = perfil.wireguard.clone().unwrap_or_default();
        let identidade = self.identidade_wireguard(perfil)?;
        let assinatura = crate::wireguard::assinatura(&rede.config, &identidade);
        let config = crate::wireguard::config_de(&rede.config, &identidade);
        self.motores_wireguard.garantir(perfil, config, assinatura)
    }

    /// A identidade WireGuard do cofre que o perfil usa.
    fn identidade_wireguard(&self, perfil: &PerfilRede) -> Result<IdentidadeWireGuard, String> {
        if self.cofre.agora(|c| c.bloqueado()) {
            return Err(t!("wireguard-cofre-trancado"));
        }
        let id = perfil
            .wireguard
            .as_ref()
            .and_then(|n| n.identidade)
            .ok_or_else(|| t!("wireguard-sem-identidade"))?;
        self.cofre
            .agora(|c| c.item(id).and_then(|i| i.wireguard.clone()))
            .ok_or_else(|| self.identidade_que_falta(id, perfil, t!("wireguard-identidade-sumiu")))
    }

    /// Monta os parâmetros do nó Tailscale do perfil com o que o cofre guarda e liga o nó.
    fn preparar_tailscale(&self, perfil: &PerfilRede) -> Result<MotorTailscale, String> {
        // O estado que o nó mudou desde a última vez já vai para o item antes de ele ser lido.
        self.gravar_nos_tailscale();
        let rede = perfil.tailscale.clone().unwrap_or_default();
        let parametros = self.identidade_tailscale(perfil).and_then(|identidade| {
            let servidor = if rede.servidor.trim().is_empty() {
                identidade.servidor.as_str()
            } else {
                rede.servidor.as_str()
            };
            Ok(crate::tailscale::Parametros {
                servidor: crate::tailscale::url_do_servidor(servidor)?,
                nome_maquina: rede.nome_maquina.clone(),
                no: crate::tailscale::no_de(&identidade)?,
                chave_autenticacao: Some(identidade.chave_autenticacao.trim())
                    .filter(|c| !c.is_empty())
                    .map(|c| zeroize::Zeroizing::new(c.to_owned())),
                usar_sub_redes: rede.usar_sub_redes,
                efemero: rede.efemero,
                ca: rede.ca.clone(),
            })
        });
        self.motores_tailscale.garantir(perfil, parametros)
    }

    /// A identidade Tailscale do cofre que o perfil usa.
    fn identidade_tailscale(&self, perfil: &PerfilRede) -> Result<IdentidadeTailscale, String> {
        if self.cofre.agora(|c| c.bloqueado()) {
            return Err(t!("tailscale-cofre-trancado"));
        }
        let id = perfil
            .tailscale
            .as_ref()
            .and_then(|n| n.identidade)
            .ok_or_else(|| t!("tailscale-sem-identidade"))?;
        self.cofre
            .agora(|c| c.item(id).and_then(|i| i.tailscale.clone()))
            .ok_or_else(|| self.identidade_que_falta(id, perfil, t!("tailscale-identidade-sumiu")))
    }

    /// Monta os parâmetros do nó ZeroTier do perfil com o que o cofre guarda e liga o nó.
    fn preparar_zerotier(&self, perfil: &PerfilRede) -> Result<MotorZeroTier, String> {
        // O estado que o nó mudou desde a última vez já vai para o item antes de ele ser lido.
        self.gravar_nos_zerotier();
        let rede = perfil.zerotier.clone().unwrap_or_default();
        let parametros = self
            .identidade_zerotier(perfil)
            .and_then(|(id, identidade)| crate::zerotier::Parametros::de(&rede, id, &identidade));
        self.motores_zerotier.garantir(perfil, parametros)
    }

    /// O item de identidade ZeroTier do cofre que o perfil usa: o id e o conteúdo.
    fn identidade_zerotier(
        &self,
        perfil: &PerfilRede,
    ) -> Result<(u64, IdentidadeZeroTier), String> {
        if self.cofre.agora(|c| c.bloqueado()) {
            return Err(t!("zerotier-cofre-trancado"));
        }
        let id = perfil
            .zerotier
            .as_ref()
            .and_then(|n| n.identidade)
            .ok_or_else(|| t!("zerotier-sem-identidade"))?;
        self.cofre
            .agora(|c| c.item(id).and_then(|i| i.zerotier.clone()))
            .map(|identidade| (id, identidade))
            .ok_or_else(|| self.identidade_que_falta(id, perfil, t!("zerotier-identidade-sumiu")))
    }

    /// Liga o motor do perfil OpenVPN e faz da ponte dele o proxy da rede, com as mesmas regras do
    /// Nebula: se o motor não sobe, o proxy fica numa porta que não existe e o erro vai para o
    /// `preparar` (a conexão nunca sai direto).
    fn ligar_openvpn(&self, rede: &mut RedeResolvida) {
        let resultado = self.preparar_openvpn(&rede.perfil);
        match resultado {
            Ok(motor) => {
                if let tomni_ui::terminal::Proxy::Socks5 {
                    host,
                    port,
                    user,
                    password,
                } = motor.proxy()
                {
                    rede.perfil.proxy = Some(ProxyRede {
                        tipo: TipoProxy::Socks5,
                        host,
                        port,
                        credencial: None,
                    });
                    rede.proxy_usuario = user;
                    rede.proxy_senha = password;
                }
                rede.openvpn = Some(motor);
            }
            Err(erro) => {
                rede.perfil.proxy = Some(ProxyRede {
                    port: 0,
                    ..ProxyRede::novo()
                });
                rede.proxy_usuario = None;
                rede.proxy_senha = None;
                rede.openvpn_erro = Some(erro);
            }
        }
    }

    /// Monta o `Config` do perfil OpenVPN com o que o cofre guarda (identidade, usuário e senha) e
    /// liga o motor.
    fn preparar_openvpn(&self, perfil: &PerfilRede) -> Result<MotorOpenVpn, String> {
        let rede = perfil.openvpn.clone().unwrap_or_default();
        let (identidade, usuario, senha) = self.identidade_openvpn(perfil)?;
        let assinatura = crate::openvpn::assinatura(
            &rede.config,
            &identidade,
            &usuario,
            &senha,
            rede.ignorar_rotas,
        );
        let config = crate::openvpn::config_de(
            &rede.config,
            &identidade,
            &usuario,
            &senha,
            rede.ignorar_rotas,
            std::time::SystemTime::now(),
        );
        self.motores_vpn.garantir(perfil, config, assinatura)
    }

    /// A identidade OpenVPN do cofre que o perfil usa, com o usuário e a senha do item.
    fn identidade_openvpn(
        &self,
        perfil: &PerfilRede,
    ) -> Result<(IdentidadeOpenVpn, String, String), String> {
        let nome = perfil.name.as_str();
        if self.cofre.agora(|c| c.bloqueado()) {
            return Err(t!("openvpn-cofre-trancado", nome = nome));
        }
        let id = perfil
            .openvpn
            .as_ref()
            .and_then(|n| n.identidade)
            .ok_or_else(|| t!("openvpn-sem-identidade", nome = nome))?;
        self.cofre
            .agora(|c| {
                c.item(id).and_then(|i| {
                    i.openvpn
                        .clone()
                        .map(|o| (o, i.usuario.clone(), i.senha.clone()))
                })
            })
            .ok_or_else(|| {
                self.identidade_que_falta(id, perfil, t!("openvpn-identidade-sumiu", nome = nome))
            })
    }

    /// Liga o motor do perfil Nebula e faz da ponte dele o proxy da rede. Se o motor não sobe, o
    /// proxy fica numa porta que não existe e o erro vai para o `preparar`: a conexão nunca sai
    /// direto.
    fn ligar_nebula(&self, rede: &mut RedeResolvida) {
        let resultado = self.identidade_nebula(&rede.perfil).and_then(|identidade| {
            let agora = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            self.motores.garantir(&rede.perfil, &identidade, agora)
        });
        match resultado {
            Ok(motor) => {
                if let tomni_ui::terminal::Proxy::Socks5 {
                    host,
                    port,
                    user,
                    password,
                } = motor.proxy()
                {
                    rede.perfil.proxy = Some(ProxyRede {
                        tipo: TipoProxy::Socks5,
                        host,
                        port,
                        credencial: None,
                    });
                    rede.proxy_usuario = user;
                    rede.proxy_senha = password;
                }
                rede.nebula = Some(motor);
            }
            Err(erro) => {
                rede.perfil.proxy = Some(ProxyRede {
                    port: 0,
                    ..ProxyRede::novo()
                });
                rede.proxy_usuario = None;
                rede.proxy_senha = None;
                rede.nebula_erro = Some(erro);
            }
        }
    }

    /// A identidade Nebula do cofre que o perfil usa.
    fn identidade_nebula(&self, perfil: &PerfilRede) -> Result<IdentidadeNebula, String> {
        let nome = perfil.name.as_str();
        if self.cofre.agora(|c| c.bloqueado()) {
            return Err(t!("nebula-cofre-trancado", nome = nome));
        }
        let id = perfil
            .nebula
            .as_ref()
            .and_then(|n| n.identidade)
            .ok_or_else(|| t!("nebula-sem-identidade", nome = nome))?;
        self.cofre
            .agora(|c| c.item(id).and_then(|i| i.nebula.clone()))
            .ok_or_else(|| {
                self.identidade_que_falta(id, perfil, t!("nebula-identidade-sumiu", nome = nome))
            })
    }

    /// A conexão com o usuário da credencial onde o dela está vazio (o servidor e o salto).
    fn conexao_efetiva(&self, c: &Connection) -> Connection {
        let usuario = |id: Option<u64>| {
            id.and_then(|id| self.cofre.agora(|x| x.item(id).map(|i| i.usuario.clone())))
                .filter(|u| !u.is_empty())
        };
        let mut efetiva = c.clone();
        if efetiva.user.is_empty()
            && let Some(u) = usuario(c.credencial)
        {
            efetiva.user = u;
        }
        if let Some(j) = &mut efetiva.jump
            && j.user.is_empty()
            && let Some(u) = usuario(j.credencial)
        {
            j.user = u;
        }
        efetiva
    }

    /// Mostra nas propriedades a conexão escolhida (ou os valores padrão, num grupo).
    fn carregar_propriedades(&self) {
        let c = self
            .conexao_selecionada()
            .unwrap_or_else(|| Connection::new("", Kind::Ssh));
        self.campos.carregar(&c, &self.cofre);
        self.sincronizar_credencial_herdada(&c);
        self.sincronizar_credencial_do_grupo();
        self.sincronizar_rede();
        self.sincronizar_perfil_ia();
        self.sincronizar_mcp();
        self.sincronizar_persistente();
        self.sincronizar_mouse();
        self.sincronizar_retentar();
        self.sincronizar_comandos_do_grupo();
        self.sincronizar_x11();
        self.sincronizar_colar();
    }

    /// Os diálogos da árvore aparecem na janela `handle`? Sim se ela é a última ativa, ou se é
    /// a principal e a última ativa já fechou. Com rastreamento, para o `visible`.
    fn dialogos_na_janela(&self, handle: WindowHandle) -> bool {
        let ativa = self.ativa.get();
        self.janelas
            .todas()
            .iter()
            .position(|j| j.handle == ativa)
            .map_or_else(
                || self.janelas.principal().handle == handle,
                |_| ativa == handle,
            )
    }

    /// A janela das abas novas: a última ativa ou, se ela já fechou, a principal.
    fn janela_ativa(&self) -> Janela {
        self.janelas
            .achar(self.ativa.get_untracked())
            .unwrap_or_else(|| self.janelas.principal())
    }

    /// A janela de `lugar`.
    fn janela_de(&self, lugar: Lugar) -> Janela {
        lugar
            .janela
            .and_then(|h| self.janelas.achar(h))
            .unwrap_or_else(|| self.janela_ativa())
    }

    /// Onde uma conexão aberta pelo usuário entra: o painel livre da última janela ativa ou,
    /// com `conexoes.abrir_em` em "Nova janela", uma janela nova (a de sempre onde o sistema
    /// só tem uma janela).
    fn lugar_novo(self: &Rc<Self>) -> Lugar {
        let em_janela_nova = self
            .config
            .with_untracked(|c| c.conexoes.abrir_em == AbrirEm::NovaJanela);
        if em_janela_nova && let Some(janela) = self.abrir_janela((1, 1), None) {
            return Lugar::painel(janela, 0);
        }
        Lugar::default()
    }

    /// Como [`Tela::lugar_novo`], para o menu de `janela`: sem "Nova janela" configurada, a aba
    /// entra na janela do menu, não na última ativa.
    fn lugar_novo_em(self: &Rc<Self>, janela: Janela) -> Lugar {
        match self.lugar_novo() {
            Lugar { janela: None, .. } => Lugar::na_janela(janela),
            novo => novo,
        }
    }

    /// `lugar` com a janela já escolhida: um pedido que espera a senha não muda de janela se a
    /// pessoa trocar o foco enquanto isso.
    fn fixar(&self, lugar: Lugar) -> Lugar {
        Lugar {
            janela: Some(self.janela_de(lugar).handle),
            painel: lugar.painel,
        }
    }

    /// Vai para a página Terminais se as abas novas caem na janela principal (as das outras
    /// janelas aparecem lá, e a principal fica onde está).
    fn mostrar_terminais(&self) {
        if self.janela_ativa().handle == self.janelas.principal().handle {
            self.pagina.set("terminais".to_owned());
        }
    }

    /// Abre uma aba da conexão `id`. SSH sem chave usa a senha do cofre; sem ela (ou com o
    /// cofre bloqueado), pede a senha antes.
    fn abrir(&self, id: u64, lugar: Lugar) {
        self.abrir_com(id, lugar, true);
    }

    /// Como [`Tela::abrir`]; `registrar` diz se a abertura entra nos recentes.
    fn abrir_com(&self, id: u64, lugar: Lugar, registrar: bool) {
        let Some(c) = self.tree.with_untracked(|t| t.conexao_para_abrir(id)) else {
            return;
        };
        if c.kind == Kind::Link {
            self.abrir_link_da_arvore(&c);
            if registrar {
                self.registrar_recente(&c);
            }
            return;
        }
        if c.kind.so_arquivos() {
            self.abrir_arquivos_em(id, None, None, true);
            if registrar {
                self.registrar_recente(&c);
            }
            return;
        }
        let lugar = self.fixar(lugar);
        let senhas = self.senhas_do_cofre(&c);
        if precisa_de_senha(&c) && senhas.conexao.is_none() {
            self.pendentes.update(|p| {
                p.push(Pedido {
                    id,
                    lugar,
                    recusa: Recusa::Nao,
                    registrar,
                    arquivos: false,
                    programa: None,
                });
            });
        } else {
            // No "Agente SSH, senão a senha", a recusa do agente volta ao diálogo de senha.
            let vigiar =
                senhas.tem_segredo() || c.modo_da_credencial() == ModoCredencial::AgenteOuSenha;
            self.abrir_agora(&c, senhas, lugar, vigiar, registrar);
        }
    }

    /// Abre a aba. Com `vigiar`, uma recusa de autenticação volta ao diálogo de senha.
    fn abrir_agora(
        &self,
        c: &Connection,
        senhas: Senhas,
        lugar: Lugar,
        vigiar: bool,
        registrar: bool,
    ) {
        self.abrir_e_devolver(c, senhas, lugar, vigiar, registrar);
    }

    /// Como [`Tela::abrir_agora`], devolvendo a sessão da aba nova.
    fn abrir_e_devolver(
        &self,
        c: &Connection,
        senhas: Senhas,
        lugar: Lugar,
        vigiar: bool,
        registrar: bool,
    ) -> Session {
        let n = self.contador.get_untracked() + 1;
        self.contador.set(n);
        let perfil = senhas.rede.as_ref().map(|r| r.perfil.clone());
        let da_abertura = senhas.conexao.clone();
        let mut efetiva = self.conexao_efetiva(c);
        let tmux = self.tmux_para_abrir(c);
        efetiva.sessao_persistente = tmux.as_ref().map(|(aba, _)| aba.clone());
        let sessao = self.opener.open(&efetiva, senhas, n);
        // A sessão que a restauração (ou a lista do servidor) reusa já rodou os comandos ao
        // conectar.
        let restaurada = tmux.as_ref().is_some_and(|(_, restaurada)| *restaurada);
        if let Some((nome, _)) = tmux {
            self.lembrar_tmux(sessao, nome);
        }
        self.lembrar_se_wsl(sessao, c);
        if let Some(perfil) = perfil {
            self.marcar_rede_da_sessao(sessao, &c.name, perfil);
        }
        if c.kind == Kind::Ssh && c.so_tuneis {
            let nome = sessao.title();
            sessao.name(t!("casca-sessao-com-tuneis", nome = nome));
        }
        if c.kind == Kind::Ssh && !c.tuneis.is_empty() {
            self.vigiar_tuneis_da_sessao(sessao);
        }
        if c.kind == Kind::Ssh
            && let Some(tela) = self.eu.upgrade()
        {
            tela.subir_reles_da_conexao(sessao, c);
        }
        let lugar = self.fixar(lugar);
        if vigiar {
            self.vigiadas.update(|v| {
                v.push(Vigiada {
                    sessao,
                    id: c.id,
                    lugar,
                });
            });
        }
        self.entra_na_grade(sessao, lugar);
        // A restauração e os espaços abrem sem registrar: os assistentes da aba voltam com ela.
        if !registrar {
            self.aba_aberta(c.id, lugar, sessao);
        }
        if c.id != 0 && !restaurada {
            self.comecar_comandos(sessao, c.id, Inicio::Abertura, da_abertura.as_deref());
        }
        if registrar && c.id != 0 {
            self.registrar_recente(c);
        }
        sessao
    }

    /// A senha do diálogo chegou: guarda no cofre (se pediram e deu) e abre.
    fn concluir_senha(&self, pedido: Pedido, senha: String, salvar: bool) {
        let Some(c) = self
            .tree
            .with_untracked(|t| t.conexao_para_abrir(pedido.id))
        else {
            return;
        };
        let digitada = (!senha.is_empty()).then_some(senha.clone());
        let alvo = if pedido.recusa == Recusa::Salto {
            Alvo::Salto
        } else {
            Alvo::Conexao
        };
        let guarda = salvar && digitada.is_some() && !self.cofre.agora(|x| x.bloqueado());
        if guarda {
            self.guardar_senha(c.id, alvo, &senha);
        }
        // Relê a conexão: guardar a senha pode ter ligado a credencial.
        let c = self
            .tree
            .with_untracked(|t| t.conexao_para_abrir(pedido.id))
            .unwrap_or(c);
        let mut senhas = self.senhas_do_cofre(&c);
        let outra = match alvo {
            Alvo::Conexao => senhas.salto.is_some(),
            Alvo::Salto => senhas.conexao.is_some(),
        };
        match alvo {
            Alvo::Conexao => senhas.conexao = digitada,
            Alvo::Salto => senhas.salto = digitada,
        }
        if pedido.arquivos {
            self.concluir_senha_de_arquivos(&c, senhas, pedido.registrar);
            return;
        }
        if let Some(qual) = pedido.programa {
            self.lancar_programa_agora(&c, senhas, qual);
            return;
        }
        let vigiar = guarda || outra;
        self.abrir_agora(&c, senhas, pedido.lugar, vigiar, pedido.registrar);
    }

    /// De quem é a recusa de autenticação: do salto, se o motivo começa com o `host:porta` dele.
    fn recusa_de(&self, id: u64, motivo: &str) -> Recusa {
        let salto = self.tree.with_untracked(|t| {
            t.connection(id)
                .and_then(|c| c.jump.as_ref().map(|j| format!("{}:{}", j.host, j.port)))
        });
        match salto {
            Some(prefixo) if motivo.starts_with(&prefixo) => Recusa::Salto,
            _ => Recusa::Conexao,
        }
    }

    fn entra_na_grade(&self, sessao: Session, lugar: Lugar) {
        let janela = self.janela_de(lugar);
        janela.layout.update(|l| {
            let painel = lugar
                .painel
                .filter(|&p| p < l.pane_count())
                .unwrap_or_else(|| painel_livre(l));
            l.add_tab(sessao, painel);
            l.select(sessao);
        });
    }

    fn abrir_shell_local(&self, lugar: Lugar) {
        let c = Connection::new(t!("comum-tipo-shell-local"), Kind::Local);
        self.abrir_agora(&c, Senhas::default(), lugar, false, false);
    }

    /// A sessão à mostra no painel ativo da janela.
    fn sessao_ativa_de(&self, janela: Janela) -> Option<Session> {
        janela
            .layout
            .with_untracked(|l| l.pane(l.active()).selected)
    }

    /// A sessão à mostra no painel ativo da última janela ativa.
    fn sessao_ativa(&self) -> Option<Session> {
        self.sessao_ativa_de(self.janela_ativa())
    }

    fn aplicar_grade(&self, janela: Janela, colunas: usize, linhas: usize) {
        janela.grade.set((colunas, linhas));
        janela.layout.update(|l| {
            if (colunas, linhas) == (1, 1) {
                l.single();
            } else {
                l.grid(colunas, linhas);
            }
        });
    }

    /// Abre as abas de `resolvida` em `janela`, cada uma no painel dela. As que pedem senha
    /// esperam o diálogo.
    /// Os assistentes salvos com cada aba voltam junto, quando as Configurações deixam.
    fn encher(&self, janela: Janela, resolvida: &Resolvida, registrar: bool) {
        let primeiro = self.comecar_espaco();
        for (painel, abas) in resolvida.paineis.iter().enumerate() {
            for (aba, assistentes) in abas {
                self.abrir_aba_salva(aba, assistentes, Lugar::painel(janela, painel), registrar);
            }
        }
        if primeiro {
            self.terminar_espaco();
        }
    }

    /// Abre a aba salva `aba` em `lugar`, com os `assistentes` que a controlavam.
    fn abrir_aba_salva(
        &self,
        aba: &Aba,
        assistentes: &[AssistenteSalvo],
        lugar: Lugar,
        registrar: bool,
    ) {
        match aba {
            Aba::Local => {
                self.relancar_com(0, lugar, assistentes);
                self.abrir_shell_local(lugar);
            }
            Aba::Conexao(id, tmux) => {
                if let Some(aba) = tmux {
                    self.tmux.reservar(*id, aba.clone());
                }
                self.relancar_com(*id, lugar, assistentes);
                self.abrir_com(*id, lugar, registrar);
            }
        }
    }

    /// Abre uma janela secundária com a grade e a geometria de `salva`. `None` onde só cabe
    /// uma janela.
    fn abrir_janela(
        self: &Rc<Self>,
        grade: (usize, usize),
        geometria: Option<WindowGeometry>,
    ) -> Option<Janela> {
        self.abrir_janela_titulada(None, grade, geometria, DockLayout::default())
    }

    /// Como [`Tela::abrir_janela`], com um título que não muda (o cluster usa "Cluster (N)") e os
    /// painéis Conexões e Propriedades em `paineis` (o vazio é o padrão: os dois fixados à
    /// esquerda, como na principal). Sem título, ele é "Janela K", pela posição na lista.
    fn abrir_janela_titulada(
        self: &Rc<Self>,
        titulo_fixo: Option<String>,
        grade: (usize, usize),
        geometria: Option<WindowGeometry>,
        paineis: DockLayout,
    ) -> Option<Janela> {
        let numero = self.janelas.quantas_agora() + 1;
        let inicial = titulo_fixo
            .clone()
            .unwrap_or_else(|| t!("casca-titulo-da-janela", n = numero));
        let tela = Rc::clone(self);
        self.janelas
            .abrir(inicial, grade, geometria, paineis, move |janela| {
                janela_secundaria(&tela, janela, titulo_fixo)
            })
    }

    /// Abre `salva` numa janela nova: a grade, a geometria e as abas.
    fn abrir_salva(self: &Rc<Self>, salva: &JanelaSalva, registrar: bool) -> Option<Resolvida> {
        let resolvida = self.tree.with_untracked(|t| salva.resolver(t));
        let janela =
            self.abrir_janela_titulada(None, salva.grade, geometria_de(salva), paineis_de(salva))?;
        self.encher(janela, &resolvida, registrar);
        Some(resolvida)
    }

    /// "Nova janela": uma janela vazia, com a grade 1.
    fn nova_janela(self: &Rc<Self>) {
        if self.abrir_janela((1, 1), None).is_none() {
            self.avisar(InfoBarSeverity::Warning, t!("casca-so-uma-janela"));
        }
    }

    /// Reabre as janelas da última execução. A primeira é a principal: só recebe a grade e as
    /// abas. Conexões que sumiram são ignoradas; as que pedem senha esperam o diálogo, cada
    /// uma no painel de antes.
    fn restaurar(self: &Rc<Self>, estado: &Espaco) {
        self.restaurar_arquivos(&estado.arquivos);
        let Some((primeira, outras)) = estado.janelas.split_first() else {
            return;
        };
        let principal = self.janelas.principal();
        let (colunas, linhas) = primeira.grade;
        if colunas > 0 && linhas > 0 {
            self.aplicar_grade(principal, colunas, linhas);
            let resolvida = self.tree.with_untracked(|t| primeira.resolver(t));
            self.encher(principal, &resolvida, false);
        }
        let reabre = self.reabre_assistentes();
        for salva in outras.iter().filter(|s| s.tem_abas()) {
            if salva.grade.0 == 0 || salva.grade.1 == 0 {
                continue;
            }
            // Um cluster com IA volta como cluster, com o assistente, quando os assistentes
            // voltam com o app.
            if reabre
                && let Some(ia) = &salva.cluster_ia
                && Ferramenta::da_chave(&ia.ferramenta).is_some()
            {
                let resolvida = self.tree.with_untracked(|t| salva.resolver(t));
                self.reabrir_cluster_ia(salva, &resolvida, ia);
                continue;
            }
            self.abrir_salva(salva, false);
        }
    }

    /// Leva a sessão para o painel ativo da janela `destino` e dá o foco a ela.
    fn mover_sessao(&self, sessao: Session, destino: Janela) {
        let painel = destino.layout.with_untracked(|l| l.active());
        let chave = sessao.key_str();
        for outra in self.janelas.todas_agora() {
            if outra.handle != destino.handle {
                outra.envio.esquecer(&chave);
            }
        }
        move_session(sessao, destino.layout, painel);
        destino.layout.update(|l| l.select(sessao));
        destino.handle.focus();
    }

    /// Fecha a janela secundária que ficou sem sessões depois de uma mudança de aba.
    fn fechar_se_vazia(&self, janela: Janela) {
        let vazia = janela
            .layout
            .try_with_untracked(|l| l.sessions().is_empty())
            .unwrap_or(false);
        if vazia && janela.handle != self.janelas.principal().handle {
            janela.handle.close();
        }
    }

    /// A aba foi solta fora de todas as janelas: abre uma janela nova na posição solta com a
    /// sessão dentro.
    fn destacar(self: &Rc<Self>, sessao: Session, ponto: Option<(i32, i32)>, origem: Janela) {
        let geometria = ponto.map(|posicao| WindowGeometry {
            position: Some(posicao),
            size: janelas::TAMANHO_PADRAO,
        });
        let Some(nova) = self.abrir_janela((1, 1), geometria) else {
            return;
        };
        move_session(sessao, nova.layout, 0);
        nova.layout.update(|l| l.select(sessao));
        self.fechar_se_vazia(origem);
    }

    /// A janela no formato do espaço de trabalho: a grade, as abas, os painéis (de uma
    /// secundária) e, se o sistema informa, a posição e o tamanho de agora. As abas dos assistentes ficam de fora; com `com_ias`, os
    /// assistentes de cada aba e o de um cluster com IA vão junto (com a conversa), para voltar
    /// quando o espaço (ou o app) reabrir. `None` se a janela já foi descartada.
    fn salva_da_janela(&self, janela: Janela, com_ias: bool) -> Option<JanelaSalva> {
        let layout = janela.layout.try_get_untracked()?;
        let grade = janela.grade.try_get_untracked()?;
        let pular = self.abas_de_assistente();
        let mut salva = self.tree.with_untracked(|t| {
            JanelaSalva::do_layout(grade, &layout, &pular, t, &|s, item| {
                self.completar_item(s, item, com_ias)
            })
        });
        if let Some(g) = janela.handle.geometry() {
            salva.posicao = g.position;
            salva.tamanho = Some(g.size);
        }
        // Os painéis da principal vão em `Espaco.paineis`, com os das outras páginas.
        if janela.handle != self.janelas.principal().handle {
            salva.laterais = janela.paineis.try_with_untracked(|layout| {
                (*layout != DockLayout::default()).then(|| paineis::de_layout(layout))
            })?;
        }
        if com_ias {
            salva.cluster_ia = self
                .cluster_ia_da_janela(janela)
                .map(|(f, painel, assistente)| ClusterIaSalvo {
                    ferramenta: f.chave().to_owned(),
                    painel,
                    conversa: self.conversa_da_aba_de_assistente(assistente),
                });
        }
        Some(salva)
    }

    /// Completa o item da aba `sessao` com a sessão do tmux (ou do screen) e, com
    /// `com_assistentes`, os assistentes que a controlam.
    fn completar_item(
        &self,
        sessao: Session,
        item: crate::espacos::Item,
        com_assistentes: bool,
    ) -> crate::espacos::Item {
        let item = item.com_sessao(self.tmux.nome_de(sessao));
        if com_assistentes {
            item.com_assistentes(self.assistentes_salvos_de(sessao))
        } else {
            item
        }
    }

    /// O que as janelas mostram agora, no formato do espaço de trabalho. `com_ias` como em
    /// [`Tela::salva_da_janela`]: o `estado.json` só vai com eles se as Configurações mandam os
    /// assistentes voltarem com o app.
    fn estado_atual(&self, com_ias: bool) -> Espaco {
        let salvas = self
            .janelas
            .todas_agora()
            .into_iter()
            .filter_map(|j| self.salva_da_janela(j, com_ias))
            .collect();
        let mut estado = Espaco::new(Origem::Todas, salvas);
        estado.arquivos = self.arquivos_salvos();
        estado.notas = Some(self.notas.salvas());
        for (chave, pagina) in [
            (paineis::TERMINAIS, &self.paineis_terminais),
            (paineis::NOTAS, &self.paineis_notas),
            (paineis::ARQUIVOS, &self.paineis_arquivos),
        ] {
            if let Some(salvos) = pagina.salvos() {
                estado.paineis.insert(chave.to_owned(), salvos);
            }
        }
        estado
    }

    /// Grava o estado das janelas (as abas e a geometria). É só uma conveniência: se não
    /// gravar, as abas não voltam e fica por aí.
    fn gravar_estado(&self) {
        // Com a abertura de uma conexão esperando a senha, ou as abas esperando o cofre, o layout
        // ainda está incompleto: não grava, para não perder as abas que a pessoa ainda vai
        // desbloquear.
        if self.encerrando.get()
            || self.adiado.get_untracked()
            || self.pendentes.with_untracked(|p| !p.is_empty())
        {
            return;
        }
        self.resolver_conversas(false);
        let com_ias = self.config.with_untracked(|c| c.ia.conversas_reabrir);
        estado::gravar(&self.armazenamento, &self.estado_atual(com_ias));
    }

    // --- Espaços de trabalho ---

    /// Abre o diálogo de salvar `espaco`, com `nome` já escrito. A pasta que vem escolhida é a
    /// da linha selecionada na árvore, se for de espaços. O diálogo mora na janela principal,
    /// que ganha o foco.
    fn pedir_nome_do_espaco(&self, espaco: Espaco, nome: String) {
        if !espaco.tem_abas() {
            self.avisar(InfoBarSeverity::Warning, t!("casca-sem-abas-para-salvar"));
            return;
        }
        self.salvar_resumo.set(resumo_do_espaco(&espaco));
        *self.a_salvar.borrow_mut() = Some(espaco);
        self.salvar_nome.set(nome);
        let pasta = self
            .selecionado
            .get_untracked()
            .and_then(|c| ChaveDeEspaco::de(&c))
            .map(|c| c.pasta())
            .unwrap_or_default();
        self.salvar_pasta.set(if pasta.is_empty() {
            raiz_dos_espacos()
        } else {
            pasta
        });
        self.salvar_nova_pasta.set(String::new());
        self.salvar_erro.set(String::new());
        self.depois_de_salvar.set(Depois::Nada);
        self.janelas.principal().handle.focus();
        self.salvar_aberto.set(true);
    }

    /// "Salvar o painel como espaço…" no menu da aba `sessao` de `janela`.
    fn salvar_painel_como_espaco(&self, janela: Janela, sessao: Session) {
        let Some(layout) = janela.layout.try_get_untracked() else {
            return;
        };
        let Some(painel) =
            (0..layout.pane_count()).find(|&n| layout.pane(n).tabs.contains(&sessao))
        else {
            return;
        };
        let conexao = id_da_chave(&sessao.key_str())
            .and_then(|id| {
                self.tree
                    .with_untracked(|t| t.connection(id).map(|c| c.name.clone()))
            })
            .unwrap_or_else(|| t!("comum-tipo-shell-local"));
        let nome = nome_de_arquivo(&t!(
            "casca-painel-n-conexao",
            n = painel + 1,
            conexao = conexao
        ));
        let pular = self.abas_de_assistente();
        self.resolver_conversas(true);
        let salva = self.tree.with_untracked(|t| {
            JanelaSalva::do_painel(&layout, painel, &pular, t, &|s, item| {
                self.completar_item(s, item, true)
            })
        });
        self.pedir_nome_do_espaco(Espaco::new(Origem::Painel, vec![salva]), nome);
    }

    /// "Salvar esta janela como espaço…".
    fn salvar_janela_como_espaco(&self, janela: Janela) {
        self.resolver_conversas(true);
        let Some(salva) = self.salva_da_janela(janela, true) else {
            return;
        };
        let numero = self.janelas.numero_agora(janela.handle).unwrap_or(1);
        self.pedir_nome_do_espaco(
            Espaco::new(Origem::Janela, vec![salva]),
            t!("casca-janela-n", n = numero),
        );
    }

    /// "Salvar todas as janelas como espaço…".
    fn salvar_todas_como_espaco(&self) {
        // O espaço guarda as janelas e as abas; o lugar dos painéis é do `estado.json`.
        self.resolver_conversas(true);
        let mut espaco = self.estado_atual(true);
        espaco.paineis.clear();
        self.pedir_nome_do_espaco(espaco, t!("casca-todas-as-janelas"));
    }

    /// O botão "Salvar" do diálogo: confere o nome e a pasta, grava ou pergunta se substitui.
    fn confirmar_salvar_espaco(&self) {
        let nome = self.salvar_nome.get_untracked().trim().to_owned();
        if !nome_valido(&nome) {
            self.recusar_salvar(ErroDeEspaco::NomeInvalido.to_string());
            return;
        }
        let escolha = self.salvar_pasta.get_untracked();
        let pasta = if escolha == nova_pasta_opcao() {
            let nova = self.salvar_nova_pasta.get_untracked().trim().to_owned();
            if !nome_valido(&nova) {
                self.recusar_salvar(t!(
                    "casca-nome-da-pasta-invalido",
                    motivo = ErroDeEspaco::NomeInvalido.to_string()
                ));
                return;
            }
            nova
        } else if escolha == raiz_dos_espacos() {
            String::new()
        } else {
            escolha
        };
        if self.espacos.existe(&pasta, &nome) {
            *self.a_substituir.borrow_mut() = Some((pasta, nome));
            self.depois_de_salvar.set(Depois::Substituir);
            return;
        }
        self.gravar_espaco(&pasta, &nome, false);
    }

    /// O diálogo fecha depois do botão; esta volta a abri-lo, com o motivo.
    fn recusar_salvar(&self, motivo: String) {
        self.salvar_erro.set(motivo);
        self.depois_de_salvar.set(Depois::Reabrir);
    }

    /// O "Substituir" da pergunta.
    fn confirmar_substituir_espaco(&self) {
        let Some((pasta, nome)) = self.a_substituir.borrow_mut().take() else {
            return;
        };
        self.gravar_espaco(&pasta, &nome, true);
    }

    /// Grava o espaço do diálogo, atualiza a árvore e seleciona o espaço novo.
    fn gravar_espaco(&self, pasta: &str, nome: &str, substituir: bool) {
        let Some(espaco) = self.a_salvar.borrow().clone() else {
            return;
        };
        match self.espacos.gravar(pasta, nome, &espaco, substituir) {
            Ok(caminho) => {
                self.a_salvar.borrow_mut().take();
                self.refazer_itens();
                self.mostrar_espaco_na_arvore(&ChaveDeEspaco::Espaco(caminho.clone()));
                self.avisar(
                    InfoBarSeverity::Success,
                    t!("casca-espaco-salvo", caminho = caminho.as_str()),
                );
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!(
                    "casca-nao-consegui-salvar-o-espaco",
                    erro = erro.to_string()
                ),
            ),
        }
    }

    /// Abre os grupos que escondem `pasta` (o caminho de uma pasta de espaços; vazio é a raiz).
    fn abrir_ate_a_pasta(&self, pasta: &str) {
        self.abertos.update(|a| {
            a.insert(CHAVE_DA_RAIZ.to_owned());
            let mut acumulado = String::new();
            for parte in pasta.split('/').filter(|p| !p.is_empty()) {
                if !acumulado.is_empty() {
                    acumulado.push('/');
                }
                acumulado.push_str(parte);
                a.insert(ChaveDeEspaco::Pasta(acumulado.clone()).chave());
            }
        });
    }

    /// Escolhe a linha de um espaço ou pasta, abrindo o que a esconde, e vai para os Terminais.
    fn mostrar_espaco_na_arvore(&self, chave: &ChaveDeEspaco) {
        let pai = match chave {
            ChaveDeEspaco::Pasta(c) => c.rsplit_once('/').map_or("", |(pai, _)| pai).to_owned(),
            outra => outra.pasta(),
        };
        self.abrir_ate_a_pasta(&pai);
        self.filtro.set(String::new());
        self.selecionado.set(Some(chave.chave()));
        self.pagina.set("terminais".to_owned());
    }

    /// Abre o espaço de `caminho`, resolvendo as abas pelas conexões de agora. Conexões que
    /// não existem mais são puladas, com um aviso.
    fn abrir_espaco(self: &Rc<Self>, caminho: &str, como: ComoAbrir) {
        let espaco = match self.espacos.ler(caminho) {
            Ok(espaco) => espaco,
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "casca-nao-abri-o-espaco",
                        nome = nome_do_caminho(caminho),
                        erro = erro.to_string()
                    ),
                );
                // Um espaço apagado por fora sai da árvore e dos recentes.
                self.refazer_itens();
                return;
            }
        };
        let detalhe = resumo_do_espaco(&espaco);
        self.recentes.update(|r| {
            r.registrar(
                AlvoRecente::Espaco {
                    caminho: caminho.to_owned(),
                },
                nome_do_caminho(caminho),
                &detalhe,
            );
        });
        let na_janela_ativa = self
            .config
            .with_untracked(|c| c.janelas.abrir_espaco == AbrirEspaco::JanelaAtiva);
        let em_janelas_novas = match como {
            ComoAbrir::Padrao => espaco.origem != Origem::Painel && !na_janela_ativa,
            ComoAbrir::NovaJanela => true,
            ComoAbrir::PainelAtivo => false,
        };
        let mut faltando = Vec::new();
        let mut no_painel_ativo = false;
        for salva in &espaco.janelas {
            let resolvida = self.tree.with_untracked(|t| salva.resolver(t));
            faltando.extend(resolvida.faltando.iter().cloned());
            if resolvida.paineis.iter().all(Vec::is_empty) {
                continue;
            }
            // Um cluster com IA volta sempre como cluster, numa janela dele, com o assistente; com
            // o controle por assistentes desligado em Geral, volta como um cluster comum.
            if let Some(ia) = &salva.cluster_ia
                && self.config.with_untracked(|c| c.geral.mcp)
                && Ferramenta::da_chave(&ia.ferramenta).is_some()
            {
                self.reabrir_cluster_ia(salva, &resolvida, ia);
                continue;
            }
            if em_janelas_novas
                && let Some(janela) = self.abrir_janela_titulada(
                    None,
                    salva.grade,
                    geometria_de(salva),
                    paineis_de(salva),
                )
            {
                self.encher(janela, &resolvida, false);
            } else {
                self.encher_no_painel_ativo(&resolvida);
                no_painel_ativo = true;
            }
        }
        if let Some(aviso) = aviso_de_faltantes(&faltando) {
            self.avisar(InfoBarSeverity::Warning, aviso);
        }
        if no_painel_ativo {
            self.mostrar_terminais();
        }
    }

    /// Abre todas as abas de `resolvida` no painel ativo da última janela ativa.
    fn encher_no_painel_ativo(&self, resolvida: &Resolvida) {
        let janela = self.janela_ativa();
        let painel = janela.layout.with_untracked(|l| l.active());
        let primeiro = self.comecar_espaco();
        for (aba, assistentes) in resolvida.paineis.iter().flatten() {
            self.abrir_aba_salva(aba, assistentes, Lugar::painel(janela, painel), false);
        }
        if primeiro {
            self.terminar_espaco();
        }
    }

    /// Cria uma pasta de espaços dentro de `pai` e já a põe em renomeação.
    fn nova_pasta_de_espacos(&self, pai: &str) {
        let existentes = self.espacos.pastas();
        let nome = (1..)
            .map(|n| {
                if n == 1 {
                    t!("comum-nova-pasta")
                } else {
                    t!("casca-nova-pasta-n", n = n)
                }
            })
            .find(|nome| !existentes.contains(&Espacos::juntar(pai, nome)))
            .unwrap_or_else(|| t!("comum-nova-pasta"));
        match self.espacos.criar_pasta(pai, &nome) {
            Ok(caminho) => {
                self.abrir_ate_a_pasta(pai);
                self.filtro.set(String::new());
                self.refazer_itens();
                let chave = ChaveDeEspaco::Pasta(caminho).chave();
                self.selecionado.set(Some(chave.clone()));
                self.renomeando.set(Some(chave));
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("casca-nao-consegui-criar-a-pasta", erro = erro.to_string()),
            ),
        }
    }

    /// O nome novo de um espaço ou pasta, vindo do renomear da árvore.
    fn renomear_espaco(&self, chave: &ChaveDeEspaco, texto: &str) {
        self.renomeando.set(None);
        let texto = texto.trim();
        let (caminho, pasta) = match chave {
            ChaveDeEspaco::Raiz => return,
            ChaveDeEspaco::Pasta(c) => (c.as_str(), true),
            ChaveDeEspaco::Espaco(c) => (c.as_str(), false),
        };
        if texto.is_empty() || texto == nome_do_caminho(caminho) {
            self.refazer_itens();
            return;
        }
        match self.espacos.renomear(caminho, texto, pasta) {
            Ok(novo) => self.espaco_mudou_de_lugar(caminho, &novo, pasta),
            Err(erro) => {
                self.avisar(InfoBarSeverity::Error, erro.to_string());
                self.refazer_itens();
            }
        }
    }

    /// Um arrasto na árvore levou o espaço ou a pasta `chave` para `pai`.
    fn mover_espaco(&self, chave: &ChaveDeEspaco, pai: Option<&str>) {
        let (caminho, pasta) = match chave {
            ChaveDeEspaco::Raiz => return,
            ChaveDeEspaco::Pasta(c) => (c.as_str(), true),
            ChaveDeEspaco::Espaco(c) => (c.as_str(), false),
        };
        let destino = match pai.and_then(ChaveDeEspaco::de) {
            Some(ChaveDeEspaco::Raiz) => String::new(),
            Some(ChaveDeEspaco::Pasta(c)) => c,
            _ => {
                self.refazer_itens();
                return;
            }
        };
        let origem = caminho.rsplit_once('/').map_or("", |(pai, _)| pai);
        if destino == origem {
            self.refazer_itens();
            return;
        }
        match self.espacos.mover(caminho, &destino, pasta) {
            Ok(novo) => {
                self.abrir_ate_a_pasta(&destino);
                self.espaco_mudou_de_lugar(caminho, &novo, pasta);
            }
            Err(erro) => {
                self.avisar(InfoBarSeverity::Error, erro.to_string());
                self.refazer_itens();
            }
        }
    }

    /// Depois de renomear ou mover: os recentes, as pastas abertas e a escolha acompanham o
    /// caminho novo.
    fn espaco_mudou_de_lugar(&self, de: &str, para: &str, pasta: bool) {
        self.recentes.update(|r| r.reapontar_espacos(de, para));
        if pasta {
            self.abertos.update(|a| {
                *a = a
                    .iter()
                    .map(|chave| match chave.strip_prefix(CHAVE_DE_PASTA) {
                        Some(c) if c == de => format!("{CHAVE_DE_PASTA}{para}"),
                        Some(c) if c.starts_with(&format!("{de}/")) => {
                            format!("{CHAVE_DE_PASTA}{para}{}", &c[de.len()..])
                        }
                        _ => chave.clone(),
                    })
                    .collect();
            });
        }
        self.refazer_itens();
        let nova = if pasta {
            ChaveDeEspaco::Pasta(para.to_owned())
        } else {
            ChaveDeEspaco::Espaco(para.to_owned())
        };
        self.selecionado.set(Some(nova.chave()));
    }

    /// O "Apagar" do diálogo de espaços: apaga o espaço ou a pasta (com o que tem dentro).
    fn confirmar_apagar_espaco(&self) {
        let Some(chave) = self
            .selecionado
            .get_untracked()
            .and_then(|c| ChaveDeEspaco::de(&c))
        else {
            return;
        };
        let resultado = match &chave {
            ChaveDeEspaco::Raiz => return,
            ChaveDeEspaco::Espaco(c) => self.espacos.apagar(c),
            ChaveDeEspaco::Pasta(c) => self.espacos.apagar_pasta(c, true),
        };
        match resultado {
            Ok(()) => {
                self.selecionado.set(None);
                self.refazer_itens();
            }
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!("casca-nao-consegui-apagar", erro = erro.to_string()),
                );
                self.refazer_itens();
            }
        }
    }

    /// O que o diálogo de apagar mostra sobre a linha de espaços escolhida.
    fn texto_de_apagar_espaco(&self) -> String {
        match self.selecionado.get().and_then(|c| ChaveDeEspaco::de(&c)) {
            Some(ChaveDeEspaco::Espaco(_)) => {
                t!("casca-apagar-espaco-aviso")
            }
            Some(ChaveDeEspaco::Pasta(c)) => {
                let dentro = self.espacos.espacos_dentro(&c);
                if dentro.is_empty() {
                    t!("casca-apagar-pasta-vazia")
                } else {
                    t!("casca-apagar-pasta-com-espacos", lista = dentro.join(", "))
                }
            }
            _ => String::new(),
        }
    }

    /// Importa o `~/.ssh/config` (ou outro arquivo de mesmo formato) num grupo novo.
    fn importar(self: &Rc<Self>) {
        let tela = Rc::clone(self);
        let mut dialogo = dialogs::open_file()
            .remember("ssh-config")
            .title(t!("casca-importar-do-ssh-config"));
        if let Some(pasta) = crate::armazenamento::pasta_pessoal().map(|h| h.join(".ssh")) {
            dialogo = dialogo.directory(pasta);
        }
        dialogo.file_name("config").show(move |caminho| {
            let Some(caminho) = caminho else {
                return;
            };
            match std::fs::read_to_string(&caminho) {
                Ok(texto) => tela.importar_texto(&texto),
                Err(erro) => tela.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "comum-nao-consegui-ler",
                        arquivo = caminho.display().to_string(),
                        erro = erro.to_string()
                    ),
                ),
            }
        });
    }

    /// Importa os perfis do `~/.aws` (o `credentials` escolhido e o `config` ao lado dele) num
    /// grupo novo, com as chaves no cofre.
    fn importar_aws(self: &Rc<Self>) {
        let tela = Rc::clone(self);
        let mut dialogo = dialogs::open_file()
            .remember("aws")
            .title(t!("casca-importar-do-aws"));
        if let Some(pasta) = crate::armazenamento::pasta_pessoal().map(|h| h.join(".aws")) {
            dialogo = dialogo.directory(pasta);
        }
        dialogo.file_name("credentials").show(move |caminho| {
            let Some(caminho) = caminho else {
                return;
            };
            match std::fs::read_to_string(&caminho) {
                Ok(credenciais) => {
                    let config = caminho
                        .with_file_name("config")
                        .to_str()
                        .and_then(|c| std::fs::read_to_string(c).ok())
                        .unwrap_or_default();
                    tela.importar_aws_texto(&credenciais, &config);
                }
                Err(erro) => tela.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "comum-nao-consegui-ler",
                        arquivo = caminho.display().to_string(),
                        erro = erro.to_string()
                    ),
                ),
            }
        });
    }

    fn importar_aws_texto(&self, credenciais: &str, config: &str) {
        let importacao = aws_config::parse(credenciais, config);
        if importacao.perfis.is_empty() {
            let mut mensagem = t!("casca-aws-sem-perfis");
            for aviso in &importacao.avisos {
                mensagem.push(' ');
                mensagem.push_str(aviso);
            }
            self.avisar(InfoBarSeverity::Warning, mensagem);
            return;
        }
        let total = importacao.perfis.len();
        let cofre_aberto = !self.cofre.agora(|x| x.bloqueado());
        let Some((grupo, ids)) = self.tree.try_update(|t| {
            let grupo = t.add_group(None, t!("casca-aws-grupo"));
            let ids: Vec<u64> = importacao
                .perfis
                .iter()
                .map(|p| t.add_connection(Some(grupo), p.conexao()))
                .collect();
            (grupo, ids)
        }) else {
            return;
        };
        if cofre_aberto {
            for (perfil, id) in importacao.perfis.iter().zip(ids) {
                self.guardar_senha(id, Alvo::Conexao, &perfil.secret_key);
                if let Some(token) = &perfil.token {
                    self.gravar_token_editado(id, token.clone());
                }
            }
        }
        self.abre_grupo(Some(grupo));
        self.atualizar();
        let mut mensagem = t!("casca-importei-conexoes", n = total as i64);
        if !cofre_aberto {
            mensagem.push(' ');
            mensagem.push_str(&t!("casca-cofre-bloqueado-chaves-secretas"));
        }
        for aviso in &importacao.avisos {
            mensagem.push(' ');
            mensagem.push_str(aviso);
        }
        let severidade = if importacao.avisos.is_empty() && cofre_aberto {
            InfoBarSeverity::Success
        } else {
            InfoBarSeverity::Warning
        };
        self.avisar(severidade, mensagem);
        self.pagina.set("terminais".to_owned());
    }

    fn importar_texto(&self, texto: &str) {
        let importacao = ssh_config::parse(texto);
        if importacao.conexoes.is_empty() {
            self.avisar(
                InfoBarSeverity::Warning,
                t!("casca-sem-hosts-para-importar"),
            );
            return;
        }
        let total = importacao.conexoes.len();
        let mut conexoes = importacao.conexoes;
        self.cofre
            .agora(|cofre| ssh_config::ligar_ao_cofre(&mut conexoes, cofre));
        let grupo = self.tree.try_update(|t| {
            let grupo = t.add_group(None, t!("casca-ssh-config-grupo"));
            for c in conexoes {
                t.add_connection(Some(grupo), c);
            }
            grupo
        });
        self.abre_grupo(grupo);
        self.atualizar();
        let mut mensagem = t!("casca-importei-conexoes", n = total as i64);
        for aviso in &importacao.avisos {
            mensagem.push(' ');
            mensagem.push_str(aviso);
        }
        let severidade = if importacao.avisos.is_empty() {
            InfoBarSeverity::Success
        } else {
            InfoBarSeverity::Warning
        };
        self.avisar(severidade, mensagem);
        self.pagina.set("terminais".to_owned());
    }
}

/// O que o filtro da árvore procura além do rótulo: a descrição e o servidor de salto.
fn texto_da_busca(c: &Connection) -> String {
    match &c.jump {
        Some(salto) => format!("{}\n{}@{}", c.descricao, salto.user, salto.host),
        None => c.descricao.clone(),
    }
}

/// A lista da `TreeView` para os itens da árvore. A chave de cada linha é o id.
fn itens_da_arvore(nodes: &[Node]) -> Vec<TreeItem> {
    // A estrela depois do nome marca quem tem uma permissão própria para assistentes.
    let com_selo = |item: TreeItem, mcp: PermissaoIa| {
        if mcp.is_herdar() {
            item
        } else {
            item.badge(Symbol::Sparkle)
        }
    };
    nodes
        .iter()
        .map(|node| match node {
            Node::Group(g) => {
                let pasta = TreeItem::folder(g.id.to_string(), &g.name)
                    .children(itens_da_arvore(&g.children));
                // O globo marca quem tem um perfil de rede próprio; quem só herda fica como está.
                let pasta = if matches!(g.rede, Rede::Perfil(_)) {
                    pasta.icon(Symbol::Globe)
                } else {
                    pasta
                };
                com_selo(pasta, g.mcp)
            }
            Node::Connection(c) => {
                // O globo marca a conexão com um perfil de rede próprio. O rótulo já leva o nome, o
                // usuário e o host; a descrição e o servidor de salto entram no filtro por aqui.
                let icone = if c.kind != Kind::Local && matches!(c.rede, Rede::Perfil(_)) {
                    Symbol::Globe
                } else {
                    icone_do_tipo(c.kind)
                };
                com_selo(
                    TreeItem::leaf(c.id.to_string(), c.label())
                        .icon(icone)
                        .search_text(texto_da_busca(c)),
                    c.mcp,
                )
            }
        })
        .collect()
}

/// O ícone do tipo de conexão na árvore (e na pré-visualização da importação).
fn icone_do_tipo(kind: Kind) -> Symbol {
    match kind {
        Kind::Ssh => Symbol::Server,
        Kind::Sftp => Symbol::Folder,
        Kind::Ftp => Symbol::FolderOpen,
        Kind::S3 | Kind::Rclone => Symbol::Cloud,
        Kind::Local => Symbol::WindowConsole,
        Kind::Telnet => Symbol::Desktop,
        Kind::Link => Symbol::Globe,
    }
}

/// O ícone do tipo nos cartões do Início e da página Arquivos.
fn icone_do_cartao(kind: Kind) -> Symbol {
    match kind {
        Kind::Ssh => Symbol::WindowConsole,
        Kind::Sftp | Kind::Ftp => Symbol::Folder,
        Kind::S3 | Kind::Rclone => Symbol::Cloud,
        Kind::Local => Symbol::Desktop,
        Kind::Telnet | Kind::Link => Symbol::Globe,
    }
}

/// As linhas da árvore para as entradas da pasta dos espaços.
fn itens_dos_espacos(entradas: &[Entrada]) -> Vec<TreeItem> {
    entradas
        .iter()
        .map(|entrada| match entrada {
            Entrada::Pasta {
                nome,
                caminho,
                filhos,
            } => TreeItem::folder(ChaveDeEspaco::Pasta(caminho.clone()).chave(), nome)
                .icon(Symbol::Folder)
                .children(itens_dos_espacos(filhos)),
            Entrada::Espaco { nome, caminho } => {
                TreeItem::leaf(ChaveDeEspaco::Espaco(caminho.clone()).chave(), nome)
                    .icon(Symbol::Grid)
            }
        })
        .collect()
}

/// A lista completa da `TreeView`: as conexões e, sempre por último, o grupo fixo dos espaços
/// de trabalho.
fn itens_completos(nodes: &[Node], espacos: &Espacos) -> Vec<TreeItem> {
    let mut itens = itens_da_arvore(nodes);
    itens.push(
        TreeItem::folder(CHAVE_DA_RAIZ, raiz_dos_espacos())
            .icon(Symbol::Apps)
            .children(itens_dos_espacos(&espacos.listar())),
    );
    itens
}

/// Abre a confirmação de apagar da linha escolhida: a dos espaços ou a das conexões.
fn pedir_apagar(
    selecionado: Signal<Option<String>>,
    apagar: Signal<bool>,
    apagar_espaco: Signal<bool>,
) {
    match selecionado
        .get_untracked()
        .and_then(|c| ChaveDeEspaco::de(&c))
    {
        Some(ChaveDeEspaco::Raiz) => {}
        Some(_) => apagar_espaco.set(true),
        None => apagar.set(true),
    }
}

/// O menu de contexto de uma linha dos espaços de trabalho.
fn menu_de_espaco(tela: &Rc<Tela>, chave: &str, espaco: &ChaveDeEspaco) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id(format!("menu-{chave}"));
    let renomear = {
        let t = Rc::clone(tela);
        let chave = chave.to_owned();
        move || t.renomeando.set(Some(chave.clone()))
    };
    let apagar = {
        let t = Rc::clone(tela);
        let chave = chave.to_owned();
        move || {
            t.selecionado.set(Some(chave.clone()));
            t.apagar_espaco.set(true);
        }
    };
    match espaco {
        ChaveDeEspaco::Raiz => {
            let nova = Rc::clone(tela);
            let atualizar = Rc::clone(tela);
            menu = menu
                .item(
                    format!("menu-{chave}-nova-pasta"),
                    t!("comum-nova-pasta"),
                    move || nova.nova_pasta_de_espacos(""),
                )
                .icon(Symbol::FolderAdd)
                .item(
                    format!("menu-{chave}-atualizar"),
                    t!("comum-atualizar"),
                    move || atualizar.refazer_itens(),
                )
                .icon(Symbol::ArrowClockwise);
        }
        ChaveDeEspaco::Pasta(caminho) => {
            let nova = Rc::clone(tela);
            let caminho = caminho.clone();
            menu = menu
                .item(
                    format!("menu-{chave}-nova-pasta"),
                    t!("comum-nova-pasta"),
                    move || nova.nova_pasta_de_espacos(&caminho),
                )
                .icon(Symbol::FolderAdd)
                .item(
                    format!("menu-{chave}-renomear"),
                    t!("comum-renomear"),
                    renomear,
                )
                .icon(Symbol::Rename)
                .separator()
                .item(format!("menu-{chave}-apagar"), t!("comum-apagar"), apagar)
                .icon(Symbol::Delete);
        }
        ChaveDeEspaco::Espaco(caminho) => {
            let abrir = |como: ComoAbrir| {
                let t = Rc::clone(tela);
                let caminho = caminho.clone();
                move || t.abrir_espaco(&caminho, como)
            };
            menu = menu
                .item(
                    format!("menu-{chave}-abrir"),
                    t!("comum-abrir"),
                    abrir(ComoAbrir::Padrao),
                )
                .icon(Symbol::Open)
                .item(
                    format!("menu-{chave}-nova-janela"),
                    t!("casca-abrir-numa-nova-janela"),
                    abrir(ComoAbrir::NovaJanela),
                )
                .icon(Symbol::Window)
                .item(
                    format!("menu-{chave}-painel-ativo"),
                    t!("casca-abrir-no-painel-ativo"),
                    abrir(ComoAbrir::PainelAtivo),
                )
                .icon(Symbol::WindowConsole)
                .separator()
                .item(
                    format!("menu-{chave}-renomear"),
                    t!("comum-renomear"),
                    renomear,
                )
                .icon(Symbol::Rename)
                .separator()
                .item(format!("menu-{chave}-apagar"), t!("comum-apagar"), apagar)
                .icon(Symbol::Delete);
        }
    }
    menu
}

/// "3 abas em 2 janelas": o que um espaço abre.
fn resumo_do_espaco(espaco: &Espaco) -> String {
    let abas = espaco.total_de_abas();
    let abas = t!("casca-resumo-abas", n = abas as i64);
    if espaco.origem == Origem::Painel {
        return t!("casca-resumo-de-um-painel", abas = abas);
    }
    let janelas = espaco.janelas.len();
    t!("casca-resumo-em-janelas", n = janelas as i64, abas = abas)
}

/// O texto sem os caracteres que um nome de arquivo não pode ter.
fn nome_de_arquivo(texto: &str) -> String {
    let limpo: String = texto
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    limpo.trim().trim_end_matches('.').to_owned()
}

/// As opções do seletor de pasta do diálogo de salvar: a raiz, cada pasta e "Nova pasta…".
fn opcoes_de_pasta(espacos: &Espacos) -> Vec<String> {
    let mut opcoes = vec![raiz_dos_espacos()];
    opcoes.extend(espacos.pastas());
    opcoes.push(nova_pasta_opcao());
    opcoes
}

/// Para onde um arrasto de `origem` pode ir, com `pai` de destino (`None` é a raiz da lista).
/// As conexões ficam entre as conexões e os espaços entre os espaços; o grupo dos espaços não
/// se move.
fn pode_soltar(origem: &str, pai: Option<&str>) -> bool {
    let destino = pai.map(ChaveDeEspaco::de);
    match (ChaveDeEspaco::de(origem), destino) {
        (None, None | Some(None)) => true,
        (None, Some(Some(_))) => false,
        (Some(ChaveDeEspaco::Raiz), _) => false,
        (Some(_), Some(Some(ChaveDeEspaco::Raiz | ChaveDeEspaco::Pasta(_)))) => true,
        (Some(_), _) => false,
    }
}

/// O painel onde entra uma sessão nova: o primeiro vazio da grade ou, sem nenhum, o ativo.
fn painel_livre(l: &PaneLayout) -> usize {
    (0..l.pane_count())
        .find(|&n| l.pane(n).tabs.is_empty())
        .unwrap_or_else(|| l.active())
}

/// Os painéis Conexões e Propriedades que `salva` guardou, ou o vazio (o padrão).
fn paineis_de(salva: &JanelaSalva) -> DockLayout {
    salva
        .laterais
        .as_ref()
        .map(paineis::para_layout)
        .unwrap_or_default()
}

/// A geometria de uma janela salva, sem posições e tamanhos absurdos (um monitor que saiu).
fn geometria_de(salva: &JanelaSalva) -> Option<WindowGeometry> {
    let position = salva
        .posicao
        .filter(|&(x, y)| x.abs() <= 20_000 && y.abs() <= 20_000);
    let size = salva
        .tamanho
        .filter(|&(l, a)| (200..=10_000).contains(&l) && (150..=10_000).contains(&a));
    if position.is_none() && size.is_none() {
        return None;
    }
    Some(WindowGeometry {
        position,
        size: size.unwrap_or(janelas::TAMANHO_PADRAO),
    })
}

/// Roda `f` com a tela, como um callback de botão ou de menu.
fn acao(tela: &Rc<Tela>, f: impl Fn(&Rc<Tela>) + 'static) -> impl FnMut() + 'static {
    let tela = Rc::clone(tela);
    move || f(&tela)
}

/// Os itens a mais do menu de uma aba: mover para outra janela. O menu da aba é montado quando
/// a aba nasce, não quando abre; por isso os itens são reservados para as janelas que podem
/// existir e só aparecem os das que existem na hora de abrir (e o clique procura a janela no
/// momento).
fn extra_da_aba(tela: &Rc<Tela>, janela: Janela, sessao: Session, menu: MenuFlyout) -> MenuFlyout {
    let (ligada_ia, ligada_mcp) = (com_ia(tela.config), com_mcp(tela.config));
    let mut mover = MenuFlyout::new().id("mover-janela-menu");
    for n in 0..janelas::NO_MENU {
        let t = Rc::clone(tela);
        let janelas = tela.janelas.clone();
        mover = mover
            .item(
                format!("mover-janela-{}", n + 1),
                janelas::no_menu(n),
                move || {
                    if let Some(outra) = t.janelas.todas_agora().get(n).copied() {
                        t.mover_sessao(sessao, outra);
                        t.fechar_se_vazia(janela);
                    }
                },
            )
            .item_visible(move || {
                janelas
                    .todas()
                    .get(n)
                    .is_some_and(|outra| outra.handle != janela.handle)
            });
    }
    let t = Rc::clone(tela);
    mover = mover.item("mover-janela-nova", t!("casca-nova-janela"), move || {
        t.destacar(sessao, None, janela);
    });
    let t = Rc::clone(tela);
    let tem_arquivos = Rc::clone(tela);
    let menu = menu
        .item(
            "abrir-arquivos-da-sessao",
            t!("casca-abrir-arquivos-desta-sessao"),
            {
                let t = Rc::clone(tela);
                move || t.abrir_arquivos_da_sessao(sessao)
            },
        )
        .icon(Symbol::Folder)
        .item_visible(move || tem_arquivos.opener.sessao_tem_arquivos(sessao))
        .submenu(
            "aba-ia",
            t!("ia-remoto-menu"),
            ia::submenu_ia(tela, "aba-ia", ia::AlvoIa::Sessao(sessao)),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || ligada_ia() && t.aba_ssh(ia::AlvoIa::Sessao(sessao))
        })
        .item("tuneis-ativos-da-sessao", t!("casca-tuneis-ativos"), {
            let t = Rc::clone(tela);
            move || t.abrir_ativos_de(sessao)
        })
        .icon(Symbol::Link)
        .item_visible(move || sessao.ssh_forwards().is_some())
        .item("portas-da-sessao", t!("portas-menu-da-aba"), {
            let t = Rc::clone(tela);
            move || t.abrir_portas_de(sessao)
        })
        .icon(Symbol::PlugConnected)
        .item_visible({
            let t = Rc::clone(tela);
            move || t.opener.sessao_tem_arquivos(sessao)
        })
        .item("mcp-parar-da-aba", t!("mcp-parar"), {
            let t = Rc::clone(tela);
            move || t.parar_sessoes_da_aba(&sessao.key_str())
        })
        .icon(Symbol::Dismiss)
        .item_visible({
            let t = Rc::clone(tela);
            move || ligada_mcp() && t.aba_marcada(&sessao.key_str())
        })
        .item(
            "mcp-tirar-da-ia",
            {
                let t = Rc::clone(tela);
                move || {
                    t.dono_da_aba(&sessao.key_str())
                        .map_or_else(String::new, |d| t!("mcp-tirar-da", nome = d.nome))
                }
            },
            {
                let t = Rc::clone(tela);
                move || t.tirar_aba(&sessao.key_str())
            },
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || ligada_mcp() && t.dono_da_aba(&sessao.key_str()).is_some()
        })
        .submenu(
            "aba-controlar",
            t!("controlar-menu"),
            controlar::submenu(tela, "aba-controlar", controlar::Alvo::Aba(janela, sessao)),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || controlar::aparece(&t, controlar::Alvo::Aba(janela, sessao))
        })
        .submenu(
            "aba-controlar-dar",
            t!("controlar-dar-menu"),
            controlar::dar_tambem(tela, "aba-controlar", controlar::Alvo::Aba(janela, sessao)),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || controlar::aparece_dar(&t, controlar::Alvo::Aba(janela, sessao))
        })
        .submenu(
            "mcp-dar-para",
            t!("mcp-dar-para"),
            mcp_sessoes::dar_para(tela, sessao),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || ligada_mcp() && t.mcp.sessoes.with(|todas| todas.len() > 1)
        })
        .item("abrir-log-da-sessao", t!("logs-abrir-da-aba"), {
            let t = Rc::clone(tela);
            move || {
                if let Some(caminho) = crate::logs::ativo_da_sessao(&sessao.key_str()) {
                    // O que a sessão gravou até agora precisa estar no disco para o leitor ver.
                    t.selar_logs();
                    t.mostrar_log(&caminho);
                }
            }
        })
        .icon(Symbol::DocumentText)
        .item_visible(move || crate::logs::ativo_da_sessao(&sessao.key_str()).is_some())
        .item("comandos-rodar-de-novo", t!("comandos-rodar-de-novo"), {
            let t = Rc::clone(tela);
            move || {
                if let Some(id) = t.conexao_da_sessao(sessao) {
                    t.comecar_comandos(sessao, id, Inicio::Manual, None);
                }
            }
        })
        .icon(Symbol::Play)
        .item_visible({
            let t = Rc::clone(tela);
            move || t.tem_comandos_para_rodar(sessao)
        })
        .item("comandos-cancelar", t!("comandos-cancelar"), {
            let t = Rc::clone(tela);
            move || t.cancelar_comandos(sessao)
        })
        .icon(Symbol::Stop)
        .item_visible({
            let t = Rc::clone(tela);
            move || t.comandos_rodando(sessao).is_some()
        });
    let (reconectar, fechar_caidas, fechar_todas) =
        (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
    envio::receber_envio(janela, sessao, menu)
        .submenu("mover-janela", t!("casca-mover-para-a-janela"), mover)
        .item("aba-duplicar", t!("casca-duplicar-aba"), {
            let t = Rc::clone(tela);
            move || {
                let painel = janela.layout.with_untracked(|l| l.pane_of(sessao));
                let lugar = Lugar {
                    janela: Some(janela.handle),
                    painel,
                };
                t.duplicar(sessao, duplicar::Destino::Lugar(lugar));
            }
        })
        .icon(Symbol::Copy)
        .item("aba-sessoes", t!("sessoes-menu"), {
            let t = Rc::clone(tela);
            move || {
                if let Some(id) = t.conexao_da_sessao(sessao) {
                    t.abrir_sessoes_do_servidor(id);
                }
            }
        })
        .icon(Symbol::AppsList)
        .item_visible({
            let t = Rc::clone(tela);
            move || {
                t.conexao_da_sessao(sessao).is_some_and(|id| {
                    t.tree
                        .with_untracked(|tr| tr.connection(id).is_some_and(tmux::aceita_sessao))
                })
            }
        })
        .item(
            "salvar-painel-espaco",
            t!("casca-salvar-o-painel-como-espaco"),
            move || t.salvar_painel_como_espaco(janela, sessao),
        )
        .icon(Symbol::Save)
        .separator()
        .item(
            "reconectar-caidas-aba",
            t!("casca-reconectar-as-abas-caidas"),
            move || reconectar.reconectar_caidas(Some(janela)),
        )
        .icon(Symbol::ArrowSync)
        .item(
            "fechar-caidas-aba",
            t!("casca-fechar-as-abas-caidas"),
            move || fechar_caidas.fechar_caidas(Some(janela)),
        )
        .icon(Symbol::DismissCircle)
        .item(
            "fechar-todas-aba",
            t!("casca-fechar-todas-as-abas"),
            move || fechar_todas.fechar_todas(Some(janela)),
        )
        .icon(Symbol::Dismiss)
}

/// Os painéis de terminais de uma janela. Na principal, a área some enquanto não há sessão e
/// as boas-vindas aparecem no lugar; numa secundária ela fica sempre, para receber abas
/// arrastadas.
fn area_de_terminais(tela: &Rc<Tela>, janela: Janela, principal: bool) -> Column {
    let layout = janela.layout;
    let envio = janela.envio;
    let sem_sessoes = move || layout.with(|l| l.sessions().is_empty());
    let paineis = {
        let nova = Rc::clone(tela);
        let duplica = Rc::clone(tela);
        let destacar = Rc::clone(tela);
        let extra = Rc::clone(tela);
        let dica = Rc::clone(tela);
        let marca = Rc::clone(tela);
        let cor_da_marca = Rc::clone(tela);
        let identifica = Rc::clone(tela);
        let destaca = Rc::clone(tela);
        let fechada = Rc::clone(tela);
        let paineis = TerminalPanes::bind(layout)
            .id("paineis")
            .identify(move || identifica.identificacao_da(janela).is_some())
            .highlight_pane(move || destaca.identificacao_da(janela).flatten())
            .drag_group(janelas::GRUPO)
            .broadcast(envio.modo)
            .tab_scheme({
                // A aba escolhida com o fundo e o texto dos terminais, como no Windows Terminal.
                let config = tela.config;
                move || Some(config.with(|c| esquemas::esquema(c.terminal.esquema)))
            })
            .active_tab_indicator(true)
            .shortcuts(atalhos::paineis(tela.config))
            .on_duplicate(move |origem, lugar| {
                duplica.duplicar(origem, duplicar::Destino::Atalho(lugar))
            })
            .new_session(move || {
                match nova
                    .sessao_ativa_de(janela)
                    .and_then(|s| nova.copia_da_aba(s))
                {
                    Some(nova_sessao) => {
                        let origem = nova.sessao_ativa_de(janela);
                        nova.herdar_da_origem(origem, nova_sessao, None);
                        nova_sessao
                    }
                    None => {
                        let c = Connection::new(t!("comum-tipo-shell-local"), Kind::Local);
                        let n = nova.contador.get_untracked() + 1;
                        nova.contador.set(n);
                        nova.opener.open(&c, Senhas::default(), n)
                    }
                }
            })
            .terminal({
                let configurar = terminal_configurado(tela.config, Some((tela, janela)));
                let tela = Rc::clone(tela);
                move |t| {
                    let config = tela.config;
                    let colar = Rc::clone(&tela);
                    configurar(t)
                        .rich_ctrl_v(move || config.with(|c| c.terminal.ctrl_v_arquivos))
                        .on_rich_paste(move |colagem| colar.colar_rico(colagem, janela))
                }
            })
            .on_session_closed(move |sessao| {
                envio.esquecer(&sessao.key_str());
                crate::logs::esquecer_ativo(&sessao.key_str());
                fechada.assistente.motor.esquecer_aba(&sessao.key_str());
                fechada.esquecer_conversa(&sessao.key_str());
                fechada.esquecer_portas(&sessao.key_str());
                fechada.zoom_fonte.esquecer(sessao);
                fechada.duplicar.esquecer(sessao);
                fechada.fechar_abas_tmux(vec![sessao]);
                fechada.fechar_assistentes_de(sessao);
            })
            .on_tear_off(move |sessao, ponto| {
                if destacar.config.with_untracked(|c| c.janelas.tear_off) {
                    destacar.destacar(sessao, ponto, janela);
                }
            })
            .tab_menu_extra(move |sessao, menu| extra_da_aba(&extra, janela, sessao, menu))
            .on_tab_tooltip(move |sessao| dica.dica_da_sessao(sessao))
            .on_tab_click({
                let tela = Rc::clone(tela);
                move |sessao| tela.mostrar_aba_clicada(sessao)
            })
            .on_tab_mark(move |sessao| {
                marca
                    .aba_marcada(&sessao.key_str())
                    .then_some(Symbol::Sparkle)
            })
            .on_tab_mark_tint(move |sessao| cor_da_marca.cor_da_aba(&sessao.key_str()))
            .grow(1);
        if principal {
            paineis.visible(move || !sem_sessoes())
        } else {
            paineis
        }
    };
    let aviso_vazio = if principal {
        Column::new()
            .grow(1)
            .align(Align::Center)
            .justify(Justify::Center)
            .gap(8)
            .visible(sem_sessoes)
            .child(Icon::new(Symbol::WindowConsole).size(24).decorative())
            .child(Text::new(t!("casca-nenhuma-sessao-aberta")).subtitle())
            .child(
                Text::new(t!("casca-clique-duas-vezes-numa-conexao-da"))
                    .id("boas-vindas")
                    .secondary(),
            )
    } else {
        Column::new().padding(8).visible(sem_sessoes).child(
            Text::new(t!("casca-arraste-uma-aba-para-ca-ou"))
                .id("boas-vindas")
                .secondary(),
        )
    };
    let area = Column::new()
        .grow(1)
        .child(paineis)
        .child(aviso_vazio)
        .child(colar::barras(tela, janela))
        .child(baixar::barras(tela, janela))
        .child(portas::avisos_de_porta_nova(tela))
        .child(envio::barra(tela, janela))
        .child(envio::dialogo(janela))
        .child(colar::dialogo(tela, janela))
        .child(mcp_aprovacao::faixa_ativa(tela, janela))
        .child(mcp_aprovacao::dialogo(tela, janela))
        .child(ia_comando::flyout(tela, janela))
        .accelerator(atalhos::atalho(tela.config, "ia_comando"), {
            let t = Rc::clone(tela);
            move || t.abrir_comando_ia(janela)
        })
        .accelerator(atalhos::atalho(tela.config, "ia_abrir"), {
            let t = Rc::clone(tela);
            move || t.abrir_preferido(ia::AlvoIa::Janela(janela))
        });
    com_atalhos_da_fonte(area, tela, janela)
}

/// Põe na `coluna` os atalhos do zoom da fonte do terminal (aumentar, diminuir e voltar ao
/// tamanho das configurações), que agem na aba do painel ativo de `janela`. Valem antes de o
/// terminal receber a tecla.
fn com_atalhos_da_fonte(mut coluna: Column, tela: &Rc<Tela>, janela: Janela) -> Column {
    let acao = |f: fn(&Tela, Session)| {
        let tela = Rc::clone(tela);
        move || {
            if let Some(sessao) = tela.sessao_ativa_de(janela) {
                f(&tela, sessao);
            }
        }
    };
    // `aumentar_fonte` tem três atalhos de fábrica (`=`, `+` e `Shift++`, que é como o `+` sai
    // nos teclados US e ABNT2); `.accelerator()` aceita um só, então vai um por posição.
    for n in 0..3 {
        coluna = coluna.accelerator(
            atalhos::atalho_n(tela.config, "aumentar_fonte", n),
            acao(|t, s| t.zoom_fonte.mudar(t.config, s, 1)),
        );
    }
    for n in 0..2 {
        coluna = coluna.accelerator(
            atalhos::atalho_n(tela.config, "diminuir_fonte", n),
            acao(|t, s| t.zoom_fonte.mudar(t.config, s, -1)),
        );
    }
    coluna.accelerator(
        atalhos::atalho(tela.config, "fonte_padrao"),
        acao(|t, s| t.zoom_fonte.restaurar(s)),
    )
}

/// O que cada terminal montado recebe das configurações. Tudo é reativo, menos o histórico, que
/// o terminal só lê ao nascer (vale das próximas sessões em diante).
///
/// Com a `tela`, o Ctrl+clique abre os links da aba (M49); sem ela (a saída dos scripts, que não
/// é uma sessão de conexão), os links não são usados com o ponteiro e o menu só copia.
fn terminal_configurado(
    config: Signal<Configuracoes>,
    tela: Option<(&Rc<Tela>, Janela)>,
) -> impl Fn(Terminal) -> Terminal + 'static {
    let tela = tela.map(|(tela, janela)| (Rc::clone(tela), janela));
    move |t| {
        let historico = config.with_untracked(|c| c.terminal.historico) as usize;
        let sessao = t.session_ref();
        let t = match &tela {
            Some((tela, janela)) => {
                let (rotulo, abrir) = (Rc::clone(tela), Rc::clone(tela));
                let zoom = tela.zoom_fonte;
                t.on_font_zoom(move |passos| zoom.mudar(config, sessao, passos))
                    .links(move || config.with(|c| c.terminal.links_com_ctrl))
                    .link_action(move |acerto| rotulo.rotulo_do_link(sessao, acerto))
                    .on_link(move |acerto| abrir.abrir_link(sessao, acerto))
                    .context_flyout(menu_ia_do_terminal(tela, *janela, sessao))
                    .context_copy_paste()
                    .on_ctrl_secondary(
                        {
                            let t = Rc::clone(tela);
                            move || t.pode_baixar_marcado(sessao)
                        },
                        {
                            let (t, janela) = (Rc::clone(tela), *janela);
                            move || t.baixar_marcado(janela, sessao)
                        },
                    )
            }
            None => t.links(false),
        };
        let zoom = tela.as_ref().map(|(tela, _)| tela.zoom_fonte);
        t.font_size(move || match zoom {
            Some(zoom) => zoom.tamanho(config, sessao),
            None => config.with(|c| c.terminal.tamanho_fonte) as f64,
        })
        .scheme(move || config.with(|c| esquemas::esquema(c.terminal.esquema)))
        .cursor_shape(move || config.with(|c| esquemas::cursor(c.terminal.cursor)))
        .cursor_blink(move || config.with(|c| c.terminal.cursor_piscando))
        .copy_on_select({
            let tela = tela.clone();
            move || match &tela {
                Some((tela, _)) => tela.copiar_ao_selecionar(sessao),
                None => config.with(|c| c.terminal.copiar_ao_selecionar),
            }
        })
        .paste_warning(move || config.with(|c| c.terminal.avisar_colar))
        .right_click({
            let tela = tela.clone();
            move || {
                esquemas::botao_direito(match &tela {
                    Some((tela, _)) => tela.botao_direito(sessao),
                    None => config.with(|c| c.terminal.botao_direito),
                })
            }
        })
        .bell(move || config.with(|c| esquemas::campainha(c.terminal.campainha)))
        .scrollback(historico)
    }
}

/// Os itens de IA do menu do clique direito do terminal da `sessao` (M52): explicar a seleção ou
/// o erro, gerar um comando, perguntar e abrir o chat. Somem com `ia.mostrar_no_menu` ou os
/// assistentes de IA desligados em Geral.
fn menu_ia_do_terminal(tela: &Rc<Tela>, janela: Janela, sessao: Session) -> MenuFlyout {
    let config = tela.config;
    let mostrar = move || config.with(|c| c.geral.assistente_ia && c.ia.mostrar_no_menu);
    let com_selecao = move || mostrar() && sessao.has_selection();
    let sem_selecao = move || mostrar() && !sessao.has_selection();
    let acao = |f: fn(&Rc<Tela>, Janela, Session)| {
        let t = Rc::clone(tela);
        move || f(&t, janela, sessao)
    };
    let baixar = Rc::clone(tela);
    MenuFlyout::new()
        .item(
            "terminal-baixar",
            t!("baixar-menu"),
            acao(|t, j, s| t.baixar_marcado(j, s)),
        )
        .icon(Symbol::ArrowDownload)
        .item_visible(move || baixar.pode_baixar_marcado(sessao))
        .item(
            "terminal-ia-explicar-selecao",
            t!("terminal-ia-explicar-selecao"),
            acao(|t, _, s| t.explicar_selecao_no_chat(s)),
        )
        .icon(Symbol::Sparkle)
        .item_visible(com_selecao)
        .item(
            "terminal-ia-explicar-erro",
            t!("terminal-ia-explicar-erro"),
            acao(|t, _, s| t.explicar_erro_no_chat(s)),
        )
        .icon(Symbol::Sparkle)
        .item_visible(sem_selecao)
        .item(
            "terminal-ia-gerar",
            t!("terminal-ia-gerar"),
            acao(|t, j, _| t.abrir_comando_ia(j)),
        )
        .icon(Symbol::Sparkle)
        .item_visible(mostrar)
        .item(
            "terminal-ia-perguntar",
            t!("terminal-ia-perguntar"),
            acao(|t, _, s| t.perguntar_no_chat(s)),
        )
        .icon(Symbol::Chat)
        .item_visible(mostrar)
        .item(
            "terminal-ia-chat",
            t!("terminal-ia-chat"),
            acao(|t, _, s| t.abrir_chat_da(s)),
        )
        .icon(Symbol::Chat)
        .item_visible(mostrar)
        .submenu(
            "terminal-controlar",
            t!("controlar-menu"),
            controlar::submenu(
                tela,
                "terminal-controlar",
                controlar::Alvo::Aba(janela, sessao),
            ),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || controlar::aparece(&t, controlar::Alvo::Aba(janela, sessao))
        })
        .submenu(
            "terminal-controlar-dar",
            t!("controlar-dar-menu"),
            controlar::dar_tambem(
                tela,
                "terminal-controlar",
                controlar::Alvo::Aba(janela, sessao),
            ),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            move || controlar::aparece_dar(&t, controlar::Alvo::Aba(janela, sessao))
        })
}

/// O título de uma janela secundária. A plataforma lê o título a cada quadro, fora de um efeito:
/// por isso a leitura não rastreia.
fn titulo_da_janela(
    janelas: Janelas,
    handle: WindowHandle,
    fixo: Option<String>,
) -> impl Fn() -> String + Clone {
    move || {
        fixo.clone().unwrap_or_else(|| {
            t!(
                "casca-titulo-da-janela",
                n = janelas.numero_agora(handle).unwrap_or(1)
            )
        })
    }
}

/// O subtítulo da barra de uma janela secundária: o título fixo (o cluster) ou "Janela K". O
/// nome do app fica no selo; o título da janela no sistema continua o de [`titulo_da_janela`].
fn subtitulo_da_janela(
    janelas: Janelas,
    handle: WindowHandle,
    fixo: Option<String>,
) -> impl Fn() -> String + Clone {
    move || {
        fixo.clone().unwrap_or_else(|| {
            t!(
                "casca-subtitulo-da-janela",
                n = janelas.numero_agora(handle).unwrap_or(1)
            )
        })
    }
}

/// A tela de uma janela secundária: a barra de título, a mesma barra de menus da principal, os
/// painéis de terminais e, encaixados, as Conexões e as Propriedades. A árvore, a seleção e o
/// filtro são os mesmos da principal; o arranjo dos painéis é de cada janela (`Janela.paineis`),
/// com o menu Exibir dela. A navegação, o chat e as portas só existem na principal.
fn janela_secundaria(
    tela: &Rc<Tela>,
    janela: Janela,
    titulo_fixo: Option<String>,
) -> impl View + use<> {
    let handle = janela.handle;
    let envio = janela.envio;
    let fechar_aberto = signal(false);
    let liberada = Rc::new(Cell::new(false));

    // A janela sai da lista (e libera o layout dela) quando a tela some.
    {
        let janelas = tela.janelas.clone();
        on_cleanup(move || janelas.remover(handle));
    }
    // Fechar com sessões abertas pergunta o que fazer com elas; a principal já perguntou por
    // todas quando o app vai acabar.
    {
        let t = Rc::clone(tela);
        let liberada = Rc::clone(&liberada);
        on_close_requested(move || {
            if t.liberado.get()
                || t.fechar_tudo.get_untracked()
                || t.fechar_ou_bandeja.get_untracked()
                || t.escondido.get_untracked()
                || liberada.get()
                || !t.config.with_untracked(|c| c.geral.confirmar_fechar)
            {
                return true;
            }
            let vazia = janela
                .layout
                .try_with_untracked(|l| l.sessions().is_empty())
                .unwrap_or(true);
            if vazia {
                return true;
            }
            fechar_aberto.set(true);
            false
        });
    }

    let controle = DockController::new();
    let menus = menus::barra_de_menus(tela, janela, &controle);
    let cabecalho = Row::new()
        .gap(8)
        .align(Align::Center)
        .padding(Insets::new(0.0, 8.0, 0.0, 0.0))
        .child(Row::new().grow(1).child(menus))
        .child(ia::botao(tela, janela))
        .child(envio.botoes());

    let subtitulo = subtitulo_da_janela(tela.janelas.clone(), handle, titulo_fixo.clone());
    let titulo = titulo_da_janela(tela.janelas.clone(), handle, titulo_fixo);
    let dialogo = {
        let mover = Rc::clone(tela);
        let liberada_mover = Rc::clone(&liberada);
        let liberada_fechar = Rc::clone(&liberada);
        ContentDialog::new(
            move || {
                let n = janela
                    .layout
                    .try_with(|l| l.sessions().len())
                    .unwrap_or_default();
                t!("casca-janela-tem-sessoes", n = n as i64)
            },
            fechar_aberto,
        )
        .id("fechar-janela-dialogo")
        .content(Text::new(t!("casca-mova-as-sessoes-para-a-janela")).wrap())
        .primary(t!("casca-mover-para-a-janela-principal"), move || {
            let principal = mover.janelas.principal();
            for sessao in janela.layout.with_untracked(|l| l.sessions()) {
                mover.mover_sessao(sessao, principal);
            }
            liberada_mover.set(true);
            handle.close();
        })
        .secondary(t!("casca-fechar-as-sessoes"), move || {
            for sessao in janela.layout.with_untracked(|l| l.sessions()) {
                janela.layout.update(|l| {
                    l.remove(sessao);
                });
                sessao.dispose();
            }
            liberada_fechar.set(true);
            handle.close();
        })
        .close(t!("comum-cancelar"))
    };

    Form::new(titulo.clone())
        .density(Density::Compact)
        .title_bar(
            TitleBar::new(NOME_DO_APP)
                .icon_image(icone::PEQUENO)
                .title_badge(marca::selo())
                .subtitle(subtitulo),
        )
        .child(
            Column::new()
                .child(cabecalho)
                .child(paineis_da_secundaria(tela, janela, &controle))
                .child(dialogos_da_arvore(tela, handle))
                .child(dialogo),
        )
}

/// Os terminais de uma janela secundária com as Conexões e as Propriedades encaixadas, no
/// arranjo de `janela.paineis`.
fn paineis_da_secundaria(tela: &Rc<Tela>, janela: Janela, controle: &DockController) -> DockHost {
    let conexoes = {
        let tela = Rc::clone(tela);
        move || painel_conexoes(&tela)
    };
    let propriedades = {
        let tela = Rc::clone(tela);
        move || painel_propriedades(&tela)
    };
    DockHost::new(area_de_terminais(tela, janela, false))
        .id("terminais-paineis")
        .layout(janela.paineis)
        .controller(controle)
        .min_content(320.0)
        .pane(
            DockPane::new(
                "conexoes",
                Symbol::Server,
                t!("casca-painel-conexoes"),
                conexoes,
            )
            .size(340.0)
            .min_size(160.0),
        )
        .pane(
            DockPane::new(
                "propriedades",
                Symbol::Info,
                t!("casca-painel-propriedades"),
                propriedades,
            )
            .min_size(160.0),
        )
}

/// A tela de uma janela secundária sozinha, para as ferramentas (`tomni-ui lint --view
/// janela_terminais`, `screenshot`): monta o estado do app como a principal faria e mostra a
/// janela secundária no lugar da casca.
pub fn janela_terminais() -> impl View {
    let pasta = crate::armazenamento::pasta();
    let ambiente = Ambiente::do_sistema(&pasta);
    let (tela, _, _) = nova_tela(pasta, ambiente, |fila| {
        Rc::new(crate::abrir::RealOpener::new(fila, None))
    });
    let janela = tela.janelas.principal();
    janela_secundaria(&tela, janela, None)
}

/// A página Terminais: os menus, a árvore, as propriedades e os terminais em abas e em grade.
/// Devolve a página e, à parte, os diálogos, que precisam valer em qualquer página da
/// navegação (um pedido de senha ou de chave pode chegar com o Início na frente).
fn pagina_terminais(tela: &Rc<Tela>, fila: FilaChaves) -> (Column, Column) {
    let tela = Rc::clone(tela);
    let cofre = tela.cofre.clone();
    let Tela {
        tree,
        selecionado,
        filtro,
        filtro_aberto,
        campos,
        pendentes,
        vigiadas,
        ..
    } = *tela;

    let principal = tela.janelas.principal();
    let envio = principal.envio;
    let atalhos = tela.atalhos_aberto;

    // Trocar a escolha na árvore mostra a conexão nas propriedades; mexer nas propriedades
    // grava na conexão. Cada efeito lê os signals do outro sem rastrear, para um não disparar
    // o outro em círculo.
    {
        let t = Rc::clone(&tela);
        effect(move || {
            let _ = selecionado.get();
            untrack(|| t.carregar_propriedades());
        });
        let t = Rc::clone(&tela);
        effect(move || t.guardar_propriedades());
        // As senhas editadas nas propriedades vão para o cofre.
        let t = Rc::clone(&tela);
        effect(move || t.guardar_senhas());
        // O modo padrão do X11 mudou nas configurações: o texto "Padrão (…)" acompanha.
        let t = Rc::clone(&tela);
        effect(move || {
            let _ = t.config.with(|c| c.x11.padrao);
            untrack(|| t.sincronizar_x11());
        });
        // O modo de colar das configurações mudou: a linha de colar da conexão se sincroniza de novo.
        let t = Rc::clone(&tela);
        effect(move || {
            let _ = t.config.with(|c| c.terminal.colar_arquivos);
            untrack(|| t.sincronizar_colar());
        });
        // O cofre mudou (outro item, outro título, desbloqueio): refaz as linhas de credencial.
        let t = Rc::clone(&tela);
        effect(move || {
            let _ = t.cofre.versao();
            untrack(|| {
                if let Some(c) = t.conexao_selecionada() {
                    t.campos.sincronizar(&c, &t.cofre);
                }
            });
        });
    }

    Tela::vigiar_desbloqueio(&tela);
    Tela::vigiar_notas_ligadas(&tela);
    // O cofre com senha mestra começa bloqueado: o diálogo vem antes das abas da última
    // execução, que esperam por ele.
    let desbloquear_aberto = tela.desbloquear;
    let refazer_desbloqueio = Rc::clone(&tela.refazer_desbloqueio);
    let estado_adiado: Rc<RefCell<Option<Espaco>>> = Rc::default();
    let adiado = tela.adiado;
    // Uma segunda cópia do app abre sem as abas: elas já estão abertas na primeira.
    let estado_anterior = (tela.config.with_untracked(|c| c.geral.restaurar_sessoes)
        && !tela.so_leitura)
        .then(|| estado::ler(&tela.armazenamento))
        .flatten();

    // As abas da última execução voltam antes de começar a gravar o estado novo.
    if let Some(estado) = estado_anterior {
        if desbloquear_aberto.get_untracked() {
            *estado_adiado.borrow_mut() = Some(estado);
            adiado.set(true);
        } else {
            tela.restaurar(&estado);
        }
    }
    {
        let t = Rc::clone(&tela);
        let guardado = Rc::clone(&estado_adiado);
        let refazer = Rc::clone(&refazer_desbloqueio);
        effect(move || {
            // Com a senha errada o diálogo fecha e volta no mesmo instante: as abas esperam.
            if desbloquear_aberto.get() || refazer.get() || !adiado.get_untracked() {
                return;
            }
            untrack(|| {
                adiado.set(false);
                if let Some(estado) = guardado.borrow_mut().take() {
                    t.restaurar(&estado);
                }
            });
        });
    }
    // Uma recusa de autenticação numa sessão com senha salva (ou no "Agente SSH, senão a
    // senha") volta ao diálogo de senha. No "Agente SSH", a recusa do servidor da conexão só
    // fica na aba.
    {
        let t = Rc::clone(&tela);
        effect(move || {
            let lista = vigiadas.get();
            let mut recusadas = Vec::new();
            let mut encerradas = Vec::new();
            for v in &lista {
                if let Some(motivo) = v.sessao.close_reason() {
                    encerradas.push(v.sessao);
                    if crate::abrir::e_recusa_de_autenticacao(&motivo) {
                        recusadas.push((*v, motivo));
                    }
                } else if v.sessao.closed() {
                    encerradas.push(v.sessao);
                }
            }
            if encerradas.is_empty() {
                return;
            }
            untrack(|| {
                vigiadas.update(|l| l.retain(|v| !encerradas.contains(&v.sessao)));
                for (v, motivo) in recusadas {
                    let recusa = t.recusa_de(v.id, &motivo);
                    let so_o_agente = t.tree.with_untracked(|tree| {
                        tree.conexao_para_abrir(v.id)
                            .is_some_and(|c| c.modo_da_credencial() == ModoCredencial::Agente)
                    });
                    if recusa == Recusa::Conexao && so_o_agente {
                        continue;
                    }
                    pendentes.update(|p| {
                        p.push(Pedido {
                            id: v.id,
                            lugar: v.lugar,
                            recusa,
                            registrar: false,
                            arquivos: false,
                            programa: None,
                        });
                    });
                }
            });
        });
    }
    tela.acompanhar_edicoes();
    tela.acompanhar_colagens();
    tela.acompanhar_downloads();
    tela.ligar_rclone();
    {
        // Qualquer mudança nas janelas (abas, grade, painéis, janela aberta ou fechada) grava o
        // estado.
        let t = Rc::clone(&tela);
        effect(move || {
            for janela in t.janelas.todas() {
                let _ = janela.layout.try_get();
                let _ = janela.grade.try_get();
                let _ = janela.paineis.try_get();
            }
            let _ = t.pendentes.get();
            let _ = adiado.get();
            let _ = t.notas.salvas();
            let _ = t.paineis_terminais.layout.get();
            let _ = t.paineis_notas.layout.get();
            let _ = t.paineis_arquivos.layout.get();
            for aba in t.abas_arquivos.get() {
                let _ = aba.local.caminho.try_get();
                let _ = aba.remoto.caminho.try_get();
            }
            untrack(|| t.gravar_estado());
        });
    }
    // O pedido de fechar da principal: deixa o app na bandeja, pergunta o que fazer ou, com
    // outras janelas abertas e sem bandeja, pergunta se fecha tudo.
    {
        let t = Rc::clone(&tela);
        on_close_requested(move || t.principal_pode_fechar());
    }

    // O diálogo de salvar fecha sozinho depois do botão: se o nome não serviu, ele volta; se o
    // espaço já existe, vem a pergunta "Substituir?".
    {
        let t = Rc::clone(&tela);
        effect(move || {
            if t.salvar_aberto.get() {
                return;
            }
            let depois = t.depois_de_salvar.replace(Depois::Nada);
            untrack(|| match depois {
                Depois::Nada => {}
                Depois::Reabrir => t.salvar_aberto.set(true),
                Depois::Substituir => t.substituir_aberto.set(true),
            });
        });
    }

    // --- Diálogo da senha ---
    let senha_aberta = signal(false);
    let senha_texto = signal(String::new());
    let senha_salvar = signal(true);
    let senha_confirmada: Rc<RefCell<Option<(String, bool)>>> = Rc::default();
    {
        let t = Rc::clone(&tela);
        let mostrando = Cell::new(false);
        let confirmada_ao_fechar = Rc::clone(&senha_confirmada);
        effect(move || {
            let aberta = senha_aberta.get();
            let esperando = pendentes.with(|p| p.len());
            if aberta {
                return;
            }
            if mostrando.get() {
                mostrando.set(false);
                let pedido = pendentes.try_update(|p| p.remove(0));
                let confirmada = confirmada_ao_fechar.borrow_mut().take();
                if let (Some(pedido), Some((senha, salvar))) = (pedido, confirmada) {
                    t.concluir_senha(pedido, senha, salvar);
                }
            } else if esperando > 0 {
                mostrando.set(true);
                senha_texto.set(String::new());
                // Marcada a cada abertura; com o cofre bloqueado não há onde salvar.
                senha_salvar.set(!t.cofre.agora(|c| c.bloqueado()));
                senha_aberta.set(true);
            }
        });
    }

    // --- Diálogos da chave do servidor ---
    let chave_nova = signal(false);
    let chave_mudou = signal(false);
    let certificado_aberto = signal(false);
    let chave_confirmada = Rc::new(Cell::new(false));
    {
        let mostrando = Cell::new(false);
        let confirmada = Rc::clone(&chave_confirmada);
        effect(move || {
            let aberta = chave_nova.get() || chave_mudou.get() || certificado_aberto.get();
            let frente: Option<Arc<PedidoChave>> = ler_fila(fila, |f| f.first().cloned());
            if aberta {
                return;
            }
            if mostrando.get() {
                mostrando.set(false);
                let tirado = mexer_na_fila(fila, |f| (!f.is_empty()).then(|| f.remove(0)));
                if let Some(pedido) = tirado {
                    // A conexão que esperava pode ter desistido: o erro não importa.
                    let _ = pedido.resposta.send(confirmada.replace(false));
                }
            } else if let Some(pedido) = frente {
                mostrando.set(true);
                if pedido.certificado.is_some() {
                    certificado_aberto.set(true);
                } else if pedido.mudou {
                    chave_mudou.set(true);
                } else {
                    chave_nova.set(true);
                }
            }
        });
    }
    // --- Menus ---
    let barra_menus = menus::barra_de_menus(&tela, principal, &tela.paineis_terminais.controle);

    let botoes_envio = envio.botoes();

    // --- Árvore ---
    // A lupa ligada põe o cursor no campo da janela onde ela foi ligada (a ativa: o efeito roda
    // no quadro de qualquer janela); desligada, o filtro se limpa e a árvore volta inteira.
    let ativa = tela.ativa;
    effect(move || {
        if filtro_aberto.get() {
            request_focus_in(untrack(|| ativa.get()), "filtro");
        } else {
            untrack(|| filtro.set(String::new()));
        }
    });
    // A árvore é montada de novo quando o painel muda de lugar (M54): o estado dela já está
    // em signals da `Tela`.
    let montar_lista = {
        let tela = Rc::clone(&tela);
        move || painel_conexoes(&tela)
    };

    // --- Propriedades ---
    let Tela {
        resumo_tuneis,
        resumo_comandos,
        ..
    } = *tela;
    // O resumo da linha "Túneis" ("Nenhum", "1 túnel"…) acompanha a lista da conexão.
    effect(move || resumo_tuneis.set(resumo_dos_tuneis(campos.tuneis.with(Vec::len))));
    // O resumo da linha "Comandos" acompanha os passos e o que a pasta passa.
    effect(move || {
        let comandos = campos.comandos.get();
        let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
        resumo_comandos.set(match id {
            Some(id) => tree.with(|t| rotulo_da_linha(t, id, &comandos)),
            None => String::new(),
        });
    });
    let montar_propriedades = {
        let tela = Rc::clone(&tela);
        move || painel_propriedades(&tela)
    };

    // --- Terminais ---
    let terminais = area_de_terminais(&tela, principal, true);

    let paineis = tela.paineis_terminais.clone();
    let area = DockHost::new(terminais)
        .id("terminais-paineis")
        .layout(paineis.layout)
        .controller(&paineis.controle)
        .min_content(320.0)
        .pane(
            DockPane::new(
                "conexoes",
                Symbol::Server,
                t!("casca-painel-conexoes"),
                montar_lista,
            )
            .size(340.0)
            .min_size(160.0),
        )
        .pane(
            DockPane::new(
                "propriedades",
                Symbol::Info,
                t!("casca-painel-propriedades"),
                montar_propriedades,
            )
            .min_size(160.0),
        )
        .pane(
            DockPane::new("chat", Symbol::Chat, t!("ia-chat-painel"), {
                let t = Rc::clone(&tela);
                move || ia_chat::painel(&t)
            })
            .side(DockSide::Right)
            .mode(DockMode::Closed)
            .size(380.0)
            .min_size(280.0),
        )
        .pane(
            DockPane::new("portas", Symbol::PlugConnected, t!("portas-painel"), {
                let t = Rc::clone(&tela);
                move || portas::painel(&t)
            })
            .side(DockSide::Right)
            .mode(DockMode::Closed)
            .size(360.0)
            .min_size(260.0),
        );

    // --- Diálogos ---
    let dialogo_salvar_espaco = {
        let t = Rc::clone(&tela);
        let espacos = tela.espacos.clone();
        let Tela {
            salvar_aberto,
            salvar_nome,
            salvar_pasta,
            salvar_nova_pasta,
            salvar_erro,
            salvar_resumo,
            ..
        } = *tela;
        ContentDialog::new(t!("casca-salvar-como-espaco-de-trabalho"), salvar_aberto)
            .id("salvar-espaco-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(
                        Text::new(move || salvar_resumo.get())
                            .id("espaco-resumo")
                            .secondary(),
                    )
                    .child(Text::new(t!("casca-nome")).caption())
                    .child(
                        Edit::bind(salvar_nome)
                            .id("espaco-nome")
                            .placeholder(t!("casca-nome-do-espaco")),
                    )
                    .child(Text::new(t!("comum-pasta")).caption())
                    // As pastas mudam entre uma abertura e outra: o seletor nasce com o diálogo.
                    .child(Show::when(salvar_aberto, move || {
                        ComboBox::bind(salvar_pasta, opcoes_de_pasta(&espacos)).id("espaco-pasta")
                    }))
                    .child(
                        Edit::bind(salvar_nova_pasta)
                            .id("espaco-pasta-nova")
                            .placeholder(t!("casca-nome-da-pasta-nova"))
                            .visible(move || salvar_pasta.get() == nova_pasta_opcao()),
                    )
                    .child(
                        Text::new(move || salvar_erro.get())
                            .id("espaco-erro")
                            .caption()
                            .wrap()
                            .visible(move || !salvar_erro.get().is_empty()),
                    ),
            )
            .primary(t!("comum-salvar"), move || t.confirmar_salvar_espaco())
            .close(t!("comum-cancelar"))
    };

    let dialogo_substituir_espaco = {
        let t = Rc::clone(&tela);
        let nome = Rc::clone(&tela);
        ContentDialog::new(t!("casca-substituir-o-espaco"), tela.substituir_aberto)
            .id("substituir-espaco-dialogo")
            .content(
                Text::new(move || match nome.a_substituir.borrow().as_ref() {
                    Some((pasta, nome)) => t!(
                        "casca-espaco-ja-existe",
                        nome = nome.as_str(),
                        pasta = if pasta.is_empty() {
                            raiz_dos_espacos()
                        } else {
                            pasta.clone()
                        }
                    ),
                    None => String::new(),
                })
                .wrap(),
            )
            .primary(t!("comum-substituir"), move || {
                t.confirmar_substituir_espaco()
            })
            .close(t!("comum-cancelar"))
    };

    // O pedido que o diálogo de senha mostra: o primeiro da fila.
    let pedido_atual = move || pendentes.with(|p| p.first().copied());
    let conexao_do_pedido =
        move || pedido_atual().and_then(|p| tree.with(|t| t.conexao_para_abrir(p.id)));
    let dialogo_senha = ContentDialog::new(
        move || match (pedido_atual(), conexao_do_pedido()) {
            (Some(p), Some(c)) if p.recusa == Recusa::Salto => t!(
                "casca-senha-do-salto",
                host = c.jump.map(|j| j.host).unwrap_or_default()
            ),
            (_, Some(c)) => t!("casca-senha-de", nome = c.name),
            _ => t!("casca-senha-de-vazio"),
        },
        senha_aberta,
    )
    .id("senha-dialogo")
    .content(
        Column::new()
            .gap(8)
            .child(
                Text::new(move || match (pedido_atual(), conexao_do_pedido()) {
                    (Some(p), Some(c)) if p.recusa == Recusa::Salto => {
                        let j = c.jump.unwrap_or(Jump {
                            host: String::new(),
                            port: 22,
                            user: String::new(),
                            credencial: None,
                        });
                        if j.user.is_empty() {
                            j.host
                        } else {
                            format!("{}@{}", j.user, j.host)
                        }
                    }
                    (_, Some(c)) if !c.user.is_empty() => format!("{}@{}", c.user, c.host),
                    (_, Some(c)) => c.host,
                    _ => String::new(),
                })
                .id("senha-destino")
                .secondary(),
            )
            .child(
                Text::new(move || match (pedido_atual(), conexao_do_pedido()) {
                    // Sem senha salva do lado recusado, quem o servidor recusou foi o agente (e
                    // as chaves): no "Agente SSH, senão a senha" e no salto sem credencial.
                    (Some(p), Some(c)) if p.recusa == Recusa::Salto => {
                        if c.jump.and_then(|j| j.credencial).is_some() {
                            t!("casca-a-senha-salva-foi-recusada-pelo")
                        } else {
                            t!("casca-o-agente-foi-recusado")
                        }
                    }
                    (_, Some(c)) if c.modo_da_credencial() == ModoCredencial::AgenteOuSenha => {
                        t!("casca-o-agente-foi-recusado")
                    }
                    _ => t!("casca-a-senha-salva-foi-recusada-pelo"),
                })
                .id("senha-recusada")
                .caption()
                .wrap()
                .visible(move || pedido_atual().is_some_and(|p| p.recusa != Recusa::Nao)),
            )
            .child(
                Edit::bind(senha_texto)
                    .id("senha-campo")
                    .password()
                    .placeholder(t!("casca-senha")),
            )
            .child({
                let cofre = cofre.clone();
                CheckBox::bind(senha_salvar, t!("casca-salvar-a-senha"))
                    .id("senha-salvar")
                    .disabled(move || cofre.ler(|c| c.bloqueado()))
            })
            .child({
                let cofre = cofre.clone();
                Text::new(move || {
                    if cofre.ler(|c| c.bloqueado()) {
                        t!("casca-desbloqueie-para-salvar-senha")
                    } else {
                        t!("casca-salva-no-cofre-ou-agente")
                    }
                })
                .id("senha-ajuda")
                .caption()
                .secondary()
                .wrap()
            }),
    )
    .primary(t!("comum-conectar"), move || {
        *senha_confirmada.borrow_mut() =
            Some((senha_texto.get_untracked(), senha_salvar.get_untracked()));
    })
    .close(t!("comum-cancelar"));

    let dialogo_cofre_bloqueado =
        dialogo_desbloquear_com(cofre.clone(), desbloquear_aberto, refazer_desbloqueio);

    let dialogo_chave_nova = ContentDialog::new(t!("casca-confiar-neste-servidor"), chave_nova)
        .id("chave-dialogo")
        .content(conteudo_da_chave(fila, false))
        .primary(t!("casca-confiar-e-conectar"), {
            let confirmada = Rc::clone(&chave_confirmada);
            move || confirmada.set(true)
        })
        .close(t!("comum-cancelar"));

    let dialogo_chave_mudou =
        ContentDialog::new(t!("casca-a-chave-do-servidor-mudou"), chave_mudou)
            .id("chave-mudou-dialogo")
            .content(conteudo_da_chave(fila, true))
            .close(t!("comum-fechar"));

    let dialogo_certificado =
        ContentDialog::new(t!("casca-certificado-nao-confiavel"), certificado_aberto)
            .id("certificado-dialogo")
            .content(conteudo_do_certificado(fila))
            .primary(t!("casca-confiar-e-continuar"), {
                let confirmada = Rc::clone(&chave_confirmada);
                move || confirmada.set(true)
            })
            .close(t!("comum-cancelar"));

    let dialogo_atalhos = ContentDialog::new(t!("casca-atalhos-de-teclado"), atalhos)
        .id("atalhos-dialogo")
        .content(
            Column::new()
                .gap(4)
                .child(Text::new(t!("casca-ctrl-shift-n-nova-conexao")))
                .child(Text::new(t!("casca-ctrl-shift-g-novo-grupo")))
                .child(Text::new(t!("casca-ctrl-shift-t-abrir-um-shell")))
                .child(Text::new(t!("casca-ctrl-shift-k-abrir-o-cofre")))
                .child(Text::new(t!("casca-ctrl-shift-j-abrir-as-notas")))
                .child(Text::new(t!("casca-ctrl-alt-1-2-3-layout")))
                .child(Text::new(t!("casca-ctrl-alt-n-nova-janela")))
                .child(Text::new(t!("casca-enter-ou-clique-duplo-abrir-a")))
                .child(Text::new(t!("casca-f2-renomear-delete-apagar")))
                .child(Text::new(t!("casca-alt-shift-d-duplicar-o-painel")))
                .child(Text::new(t!("casca-alt-setas-mudar-de-painel")))
                .child(Text::new(t!("casca-ctrl-shift-w-fechar-o-painel")))
                .child(Text::new(t!("casca-f6-sair-do-terminal"))),
        )
        .close(t!("comum-fechar"));

    let dialogo_fechar_tudo = {
        let t = Rc::clone(&tela);
        ContentDialog::new(t!("casca-fechar-todas-as-janelas"), tela.fechar_tudo)
            .id("fechar-tudo-dialogo")
            .content(
                Text::new(move || {
                    let n = t.janelas.quantas();
                    t!("casca-ha-n-janelas-abertas", n = n as i64)
                })
                .wrap(),
            )
            .primary(t!("casca-fechar-tudo"), {
                let t = Rc::clone(&tela);
                move || {
                    t.liberado.set(true);
                    WindowHandle::main().close();
                }
            })
            .close(t!("comum-cancelar"))
    };

    let cabecalho = Row::new()
        .gap(8)
        .align(Align::Center)
        .padding(Insets::new(0.0, 8.0, 0.0, 0.0))
        .child(Row::new().grow(1).child(barra_menus))
        .child(ia::botao(&tela, tela.janelas.principal()))
        .child(botoes_envio);
    let pagina = Column::new().grow(1).child(cabecalho).child(area);
    let dialogos = Column::new()
        .child(dialogos_da_arvore(&tela, principal.handle))
        .child(bandeja::dialogo_fechar_ou_bandeja(&tela))
        .child(dialogo_fechar_tudo)
        .child(abas_caidas::dialogo(&tela))
        .child(sessao_tmux::dialogo(&tela))
        .child(controlar::dialogo(&tela))
        .child(openvpn_perguntas::dialogo(&tela))
        .child(cluster::dialogo(&tela))
        .child(scripts::dialogo_novo(&tela))
        .child(lancador_links::dialogo(&tela))
        .child(dialogo_salvar_espaco)
        .child(dialogo_substituir_espaco)
        .child(dialogo_senha)
        .child(dialogo_cofre_bloqueado)
        .child(dialogo_chave_nova)
        .child(dialogo_chave_mudou)
        .child(dialogo_certificado)
        .child(dialogo_atalhos)
        .child(tuneis::dialogo_dos_ativos(&tela))
        .child(portas::dialogo(&tela))
        .child(ia::dialogo_instalar(&tela))
        .child(ia::dialogo_pasta(&tela))
        .child(instalar::dialogos(&tela));
    (pagina, dialogos)
}

/// Os diálogos que a árvore e as Propriedades abrem: apagar, túneis, comandos, executar e as
/// sessões do servidor. Cada janela de terminais tem os seus, e só os da última janela ativa
/// aparecem: o diálogo abre onde a pessoa clicou. Trocar de janela com um deles aberto leva o
/// diálogo junto.
fn dialogos_da_arvore(tela: &Rc<Tela>, handle: WindowHandle) -> Column {
    let Tela {
        tree,
        selecionado,
        apagar,
        apagar_espaco,
        ..
    } = **tela;
    let dialogo_apagar = {
        let t = Rc::clone(tela);
        ContentDialog::new(
            move || {
                let nome = selecionado.get().and_then(|k| {
                    let id = k.parse().ok()?;
                    tree.with(|t| t.find(id).map(|n| n.name().to_owned()))
                });
                t!("comum-apagar-titulo", nome = nome.unwrap_or_default())
            },
            apagar,
        )
        .id("apagar-dialogo")
        .content(
            Text::new(move || {
                let grupo = selecionado.get().is_some_and(|k| {
                    k.parse()
                        .ok()
                        .is_some_and(|id| tree.with(|t| matches!(t.find(id), Some(Node::Group(_)))))
                });
                if grupo {
                    t!("casca-apagar-grupo-aviso")
                } else {
                    t!("casca-apagar-conexao-aviso")
                }
            })
            .wrap(),
        )
        .primary(t!("comum-apagar"), move || t.confirmar_apagar())
        .close(t!("comum-cancelar"))
    };

    let dialogo_apagar_espaco = {
        let t = Rc::clone(tela);
        let texto = Rc::clone(tela);
        ContentDialog::new(
            move || match selecionado.get().and_then(|c| ChaveDeEspaco::de(&c)) {
                Some(ChaveDeEspaco::Pasta(c)) => {
                    t!("casca-apagar-a-pasta-titulo", nome = nome_do_caminho(&c))
                }
                Some(ChaveDeEspaco::Espaco(c)) => {
                    t!("casca-apagar-o-espaco-titulo", nome = nome_do_caminho(&c))
                }
                _ => t!("casca-apagar-interrogacao"),
            },
            apagar_espaco,
        )
        .id("apagar-espaco-dialogo")
        .content(Text::new(move || texto.texto_de_apagar_espaco()).wrap())
        .primary(t!("comum-apagar"), move || t.confirmar_apagar_espaco())
        .close(t!("comum-cancelar"))
    };

    let aqui = {
        let t = Rc::clone(tela);
        move || t.dialogos_na_janela(handle)
    };
    Column::new()
        .visible(aqui)
        .child(dialogo_apagar)
        .child(dialogo_apagar_espaco)
        .child(tuneis::dialogo_do_editor(tela))
        .child(comandos_editor::dialogo_do_editor(tela))
        .child(programas::dialogo_executar(tela))
        .child(sessoes_servidor::dialogo(tela))
}

/// A árvore de conexões com os botões e o filtro: o painel Conexões, montado em cada janela de
/// terminais (e de novo quando o painel muda de lugar, M54). A escolha, as pastas abertas e o
/// filtro são signals da `Tela`, os mesmos em todas as janelas.
fn painel_conexoes(tela: &Rc<Tela>) -> Column {
    let tela = Rc::clone(tela);
    let Tela {
        tree,
        itens,
        selecionado,
        abertos,
        filtro,
        filtro_aberto,
        renomeando,
        apagar,
        apagar_espaco,
        ..
    } = *tela;
    // Subir e descer só valem para as conexões; o grupo dos espaços não se apaga.
    let sem_conexao_escolhida = move || {
        selecionado
            .get()
            .is_none_or(|c| ChaveDeEspaco::de(&c).is_some())
    };
    let sem_apagar = move || {
        selecionado
            .get()
            .is_none_or(|c| ChaveDeEspaco::de(&c) == Some(ChaveDeEspaco::Raiz))
    };
    let botoes = Row::new()
        .gap(2)
        .child(
            Button::icon_only(Symbol::Add)
                .id("nova-conexao")
                .label(t!("comum-nova-conexao"))
                .on_click(acao(&tela, |t| t.nova_conexao())),
        )
        .child(
            Button::icon_only(Symbol::FolderAdd)
                .id("novo-grupo")
                .label(t!("casca-novo-grupo"))
                .on_click(acao(&tela, |t| t.novo_grupo())),
        )
        .child(
            Button::icon_only(Symbol::ArrowUp)
                .id("subir")
                .label(t!("casca-subir"))
                .disabled(sem_conexao_escolhida)
                .on_click(acao(&tela, |t| t.subir())),
        )
        .child(
            Button::icon_only(Symbol::ArrowDown)
                .id("descer")
                .label(t!("casca-descer"))
                .disabled(sem_conexao_escolhida)
                .on_click(acao(&tela, |t| t.descer())),
        )
        .child(
            Button::icon_only(Symbol::Delete)
                .id("apagar")
                .label(t!("comum-apagar"))
                .disabled(sem_apagar)
                .on_click(move || pedir_apagar(selecionado, apagar, apagar_espaco)),
        )
        .child(
            ToggleButton::icon_only(filtro_aberto, Symbol::Search)
                .id("filtrar")
                .label(t!("casca-filtrar-as-conexoes")),
        );
    // O campo do filtro fica logo abaixo da barra, enquanto a lupa está ligada; o Esc
    // desliga a lupa e devolve o foco à árvore.
    let campo_filtro = Column::new()
        .visible(move || filtro_aberto.get())
        .accelerator(Shortcut::key(Key::Escape), move || {
            filtro_aberto.set(false);
            request_focus("arvore");
        })
        .child(
            Edit::bind(filtro)
                .id("filtro")
                .placeholder(t!("casca-nome-ip-usuario-ou-descricao")),
        );

    let arvore = {
        let invocar = Rc::clone(&tela);
        let renomear = Rc::clone(&tela);
        let mover = Rc::clone(&tela);
        estado_conexoes::com_estado(TreeView::bind(itens, selecionado), &tela)
            .id("arvore")
            .grow(1)
            .expanded(abertos)
            .filter(filtro)
            .renaming(renomeando)
            .on_invoke(move |chave| match ChaveDeEspaco::de(&chave) {
                Some(ChaveDeEspaco::Espaco(caminho)) => {
                    invocar.abrir_espaco(&caminho, ComoAbrir::Padrao);
                }
                Some(_) => {}
                None => {
                    if let Ok(id) = chave.parse() {
                        invocar.abrir(id, invocar.lugar_novo());
                    }
                }
            })
            .on_rename(move |chave, texto| match ChaveDeEspaco::de(&chave) {
                Some(espaco) => renomear.renomear_espaco(&espaco, &texto),
                None => renomear.renomear(&chave, &texto),
            })
            .on_delete(move |chave| {
                selecionado.set(Some(chave));
                pedir_apagar(selecionado, apagar, apagar_espaco);
            })
            .on_move(move |chave, pai, posicao| match ChaveDeEspaco::de(&chave) {
                Some(espaco) => mover.mover_espaco(&espaco, pai.as_deref()),
                None => mover.mover(&chave, pai.as_deref(), posicao),
            })
            .can_drop(pode_soltar)
            .item_context_flyout({
                let t = Rc::clone(&tela);
                move |chave| {
                    if let Some(espaco) = ChaveDeEspaco::de(chave) {
                        return menu_de_espaco(&t, chave, &espaco);
                    }
                    let id = chave.parse().unwrap_or(0);
                    let abrir = Rc::clone(&t);
                    let chave_renomear = chave.to_owned();
                    let chave_copiar = chave.to_owned();
                    let eh_ssh = t
                        .tree
                        .with_untracked(|tr| tr.connection(id).map(|c| c.kind == Kind::Ssh))
                        == Some(true);
                    let aceita_sessao = t
                        .tree
                        .with_untracked(|tr| tr.connection(id).is_some_and(tmux::aceita_sessao));
                    let usa_ssh = t
                        .tree
                        .with_untracked(|tr| tr.connection(id).map(|c| c.kind.usa_ssh()))
                        == Some(true);
                    // Numa pasta, "Abrir" e "Abrir todas em" valem para as conexões de terminal
                    // dela (em qualquer profundidade).
                    let eh_grupo = t
                        .tree
                        .with_untracked(|tr| matches!(tr.find(id), Some(Node::Group(_))));
                    let alvo = if eh_grupo {
                        AlvoAbrir::Pasta(id)
                    } else {
                        AlvoAbrir::Conexao(id)
                    };
                    let com_terminal = Rc::clone(&t);
                    let mut menu = MenuFlyout::new()
                        .id(format!("menu-{chave}"))
                        .item(
                            format!("menu-{chave}-abrir"),
                            if eh_grupo {
                                t!("casca-abrir-todas")
                            } else {
                                t!("comum-abrir")
                            },
                            move || {
                                if eh_grupo {
                                    abrir.abrir_pasta(id);
                                } else {
                                    abrir.abrir(id, abrir.lugar_novo());
                                }
                            },
                        )
                        .icon(Symbol::Open);
                    if eh_grupo {
                        menu = menu.item_visible(move || com_terminal.tem_terminal(alvo));
                    }
                    let base = if eh_grupo {
                        format!("menu-{chave}-todas")
                    } else {
                        format!("menu-{chave}")
                    };
                    abrir_em::com_abrir_em(&t, menu, &base, alvo)
                        .submenu(
                            format!("menu-{chave}-todas-ia"),
                            t!("casca-abrir-todas-com-ia"),
                            cluster::submenu_da_pasta(&t, &format!("menu-{chave}-todas-ia"), id),
                        )
                        .icon(Symbol::Sparkle)
                        .item_visible({
                            let t = Rc::clone(&t);
                            move || {
                                eh_grupo
                                    && t.tem_terminal(alvo)
                                    && t.config.with(|c| {
                                        c.geral.mcp
                                            && Ferramenta::TODAS.into_iter().any(|f| c.ia.mostra(f))
                                    })
                            }
                        })
                        .item(
                            format!("menu-{chave}-ir-para-aba"),
                            t!("casca-ir-para-a-aba"),
                            {
                                let t = Rc::clone(&t);
                                move || t.ir_para_a_aba(id)
                            },
                        )
                        .icon(Symbol::Next)
                        .item_visible({
                            let t = Rc::clone(&t);
                            move || !eh_grupo && t.linha_com_sessao(id)
                        })
                        .item(
                            format!("menu-{chave}-reconectar"),
                            t!("casca-reconectar"),
                            {
                                let t = Rc::clone(&t);
                                move || t.reconectar_linha(id)
                            },
                        )
                        .icon(Symbol::ArrowSync)
                        .item_visible({
                            let t = Rc::clone(&t);
                            move || t.linha_com_queda(id)
                        })
                        .item(
                            format!("menu-{chave}-cluster"),
                            t!("casca-abrir-em-modo-cluster"),
                            {
                                let t = Rc::clone(&t);
                                move || {
                                    let previas = t.previas_do_cluster(id);
                                    t.na_principal(|t| t.abrir_dialogo_do_cluster(&previas, false));
                                }
                            },
                        )
                        .icon(Symbol::Grid)
                        .item_visible({
                            let t = Rc::clone(&t);
                            move || t.tem_terminal(alvo)
                        })
                        .item(
                            format!("menu-{chave}-cluster-ia"),
                            t!("casca-abrir-em-cluster-com-ia"),
                            {
                                let t = Rc::clone(&t);
                                move || {
                                    let previas = t.previas_do_cluster(id);
                                    t.na_principal(|t| t.abrir_dialogo_do_cluster(&previas, true));
                                }
                            },
                        )
                        .icon(Symbol::Sparkle)
                        .item_visible({
                            let t = Rc::clone(&t);
                            move || t.config.with(|c| c.geral.mcp) && t.tem_terminal(alvo)
                        })
                        .item(format!("menu-{chave}-script"), t!("casca-rodar-script"), {
                            let t = Rc::clone(&t);
                            move || t.na_principal(|t| t.abrir_scripts(&t.previas_de_scripts(id)))
                        })
                        .icon(Symbol::Code)
                        .item_visible({
                            let t = Rc::clone(&t);
                            move || {
                                t.tree.with(|tr| {
                                    if eh_grupo {
                                        tr.connections_in(id).iter().any(|c| c.kind == Kind::Ssh)
                                    } else {
                                        tr.connection(id).is_some_and(|c| c.kind == Kind::Ssh)
                                    }
                                })
                            }
                        })
                        .item(
                            format!("menu-{chave}-arquivos"),
                            t!("casca-abrir-arquivos-sftp"),
                            {
                                let t = Rc::clone(&t);
                                move || t.na_principal(|t| t.abrir_arquivos(id))
                            },
                        )
                        .icon(Symbol::Folder)
                        .item_visible(usa_ssh)
                        .submenu(
                            format!("menu-{chave}-ia"),
                            t!("ia-remoto-menu"),
                            ia::submenu_ia(
                                &t,
                                &format!("menu-{chave}-ia"),
                                ia::AlvoIa::Conexao(id),
                            ),
                        )
                        .icon(Symbol::Sparkle)
                        .item_visible({
                            let ligada = com_ia(t.config);
                            move || eh_ssh && ligada()
                        })
                        .item(
                            format!("menu-{chave}-virt-manager"),
                            t!("casca-abrir-virt-manager"),
                            {
                                let t = Rc::clone(&t);
                                move || t.abrir_virt_manager(id)
                            },
                        )
                        .icon(Symbol::Desktop)
                        .item_visible(eh_ssh)
                        .item(
                            format!("menu-{chave}-virt-manager-local"),
                            t!("casca-abrir-virt-manager-local"),
                            {
                                let t = Rc::clone(&t);
                                move || t.abrir_virt_manager_local(id)
                            },
                        )
                        .icon(Symbol::Desktop)
                        .item_visible({
                            let t = Rc::clone(&t);
                            move || eh_ssh && t.opener.virt_manager_local()
                        })
                        .item(
                            format!("menu-{chave}-executar"),
                            t!("casca-executar-aplicativo-grafico-remoto"),
                            {
                                let t = Rc::clone(&t);
                                move || t.abrir_executar(id)
                            },
                        )
                        .icon(Symbol::Play)
                        .item_visible(eh_ssh)
                        .item(format!("menu-{chave}-tuneis"), t!("casca-tuneis"), {
                            let t = Rc::clone(&t);
                            let chave = chave.to_owned();
                            move || t.abrir_tuneis_de(&chave)
                        })
                        .icon(Symbol::Link)
                        .item_visible(eh_ssh)
                        .item(format!("menu-{chave}-sessoes"), t!("sessoes-menu"), {
                            let t = Rc::clone(&t);
                            move || t.abrir_sessoes_do_servidor(id)
                        })
                        .icon(Symbol::AppsList)
                        .item_visible(aceita_sessao)
                        .submenu(
                            format!("menu-{chave}-mcp"),
                            t!("mcp-arvore-menu"),
                            mcp_propriedades::menu_da_arvore(&t, chave, id),
                        )
                        .icon(Symbol::Sparkle)
                        .item_visible({
                            let t = Rc::clone(&t);
                            // O link não tem a linha "Assistentes (MCP)".
                            move || {
                                t.config.with(Configuracoes::mcp_no_ar)
                                    && t.tree.with(|tr| {
                                        tr.connection(id)
                                            .is_none_or(|c| CampoDaGrade::Mcp.vale(c.kind))
                                    })
                            }
                        })
                        .item(
                            format!("menu-{chave}-renomear"),
                            t!("comum-renomear"),
                            move || {
                                renomeando.set(Some(chave_renomear.clone()));
                            },
                        )
                        .icon(Symbol::Rename)
                        .item(format!("menu-{chave}-duplicar"), t!("comum-duplicar"), {
                            let t = Rc::clone(&t);
                            move || t.duplicar_conexao(id)
                        })
                        .icon(Symbol::Copy)
                        .item_visible(move || !eh_grupo)
                        .item(
                            format!("menu-{chave}-copiar-chave"),
                            t!("casca-copiar-a-chave-para-o-servidor"),
                            {
                                let t = Rc::clone(&t);
                                move || {
                                    t.selecionado.set(Some(chave_copiar.clone()));
                                    t.na_principal(|t| t.abrir_copiar_chave(None));
                                }
                            },
                        )
                        .icon(Symbol::Send)
                        .item_visible(eh_ssh)
                        .separator()
                        .item(
                            format!("menu-{chave}-apagar"),
                            t!("comum-apagar"),
                            move || apagar.set(true),
                        )
                        .icon(Symbol::Delete)
                }
            })
    };

    let arvore_vazia = move || tree.with(|t| t.nodes.is_empty());
    Column::new()
        .gap(4)
        .padding(Insets::new(4.0, 4.0, 4.0, 8.0))
        .child(botoes)
        .child(campo_filtro)
        .child(
            Text::new(t!("casca-nenhuma-conexao-use-arquivo-nova-conexao"))
                .id("dica-vazia")
                .secondary()
                .wrap()
                .visible(arvore_vazia),
        )
        .child(arvore)
}

/// A grade das propriedades da conexão escolhida: o painel Propriedades, montado em cada janela
/// de terminais. Os campos são os da `Tela`, os mesmos em todas as janelas.
fn painel_propriedades(tela: &Rc<Tela>) -> Column {
    let tela = Rc::clone(tela);
    let cofre = tela.cofre.clone();
    let Tela {
        tree,
        selecionado,
        campos,
        resumo_tuneis,
        resumo_comandos,
        config,
        ..
    } = *tela;
    // O tipo da conexão escolhida e a tabela de campos (M60): a grade esconde o que nunca vale
    // para o tipo e só desabilita o que depende de outro campo. Os valores escondidos ficam.
    let tipo = move || {
        Kind::TODOS
            .into_iter()
            .find(|k| campos.tipo.get() == k.nome())
    };
    let esconde = move |campo: CampoDaGrade| move || !tipo().is_some_and(|k| campo.vale(k));
    // As linhas da IA e dos assistentes (MCP) somem também com a área desligada em Geral.
    let (ligada_ia, ligada_mcp) = (com_ia(config), com_mcp(config));
    let usa_ssh = move || tipo().is_some_and(Kind::usa_ssh);
    // A porta padrão do tipo (a da segurança, no FTP): a Porta só fica em negrito se difere.
    let porta_padrao = move || {
        let porta = match tipo() {
            Some(Kind::Ftp) => FtpSeguranca::TODAS
                .into_iter()
                .find(|s| campos.ftp_seguranca.get() == s.nome())
                .unwrap_or_default()
                .porta_padrao(),
            Some(k) => k.porta_padrao(),
            None => Kind::Ssh.porta_padrao(),
        };
        i32::from(porta)
    };
    let sem_ca = move || campos.s3_certificado.get() != S3Certificado::CaPropria.nome();
    let ftp_ativo = move || campos.ftp_modo.get() == FtpModo::Ativo.nome();
    let sem_salto = move || campos.jump_host.get().trim().is_empty();
    let sem_conexao = move || {
        let _ = selecionado.get();
        tree.with(|t| {
            selecionado
                .get_untracked()
                .and_then(|k| k.parse::<u64>().ok())
                .and_then(|id| t.connection(id))
                .is_none()
        })
    };
    // O perfil de rede efetivo da conexão escolhida tem proxy (o FTP ativo não passa por ele).
    let com_proxy = move || {
        let _ = selecionado.get();
        tree.with(|t| {
            selecionado
                .get_untracked()
                .and_then(|k| k.parse::<u64>().ok())
                .and_then(|id| rede_efetiva(t, id))
                .is_some_and(|p| p.proxy.is_some() || p.e_embutida())
        })
    };
    let e_grupo = move || {
        let _ = selecionado.get();
        tree.with(|t| {
            selecionado
                .get_untracked()
                .and_then(|k| k.parse::<u64>().ok())
                .is_some_and(|id| matches!(t.find(id), Some(Node::Group(_))))
        })
    };
    let perfis_de_ia = tela.assistente.perfis;
    let propriedades = PropertyGrid::new()
        .id("props")
        .grow(1)
        .category(t!("casca-conexao"))
        .text(t!("comum-nome"), move || campos.nome)
        .description(t!("casca-o-nome-da-conexao-na-arvore"))
        .text(t!("casca-descricao"), move || campos.descricao)
        .description(t!("casca-um-texto-livre-sobre-a-conexao"))
        .choice(
            t!("comum-tipo"),
            move || campos.tipo,
            Kind::TODOS.map(Kind::nome),
        )
        .default(Kind::Ssh.nome())
        .description(t!("casca-ssh-sftp-ftp-s3-ou-rclone"))
        .text(t!("casca-endereco-do-link"), move || campos.url)
        .hidden(esconde(CampoDaGrade::Url))
        .description(t!("casca-o-endereco-do-link"))
        .text(t!("comum-host"), move || campos.host)
        .hidden(esconde(CampoDaGrade::Host))
        .description(t!("casca-nome-ou-endereco-do-servidor-no"))
        .number(t!("comum-porta"), move || campos.porta, 0.0..=65535.0)
        .default_with(porta_padrao)
        .hidden(esconde(CampoDaGrade::Porta))
        .description(t!("casca-a-porta-do-servidor-ssh-e"))
        .text(t!("comum-usuario"), move || campos.usuario)
        .hidden(esconde(CampoDaGrade::Usuario))
        .description(t!("casca-o-usuario-no-servidor-no-s3"))
        .choice_from(t!("comum-credencial"), move || campos.credencial, {
            let cofre = cofre.clone();
            move || {
                let herdar = usa_ssh().then(|| credencial::herdar_da_selecao(tree, selecionado));
                opcoes_de_credencial_da_conexao(&cofre, herdar)
            }
        })
        .disabled({
            let cofre = cofre.clone();
            move || cofre.ler(|c| c.bloqueado())
        })
        .default_with(move || {
            if usa_ssh() {
                credencial::herdar_da_selecao(tree, selecionado)
            } else {
                perguntar()
            }
        })
        .hidden(esconde(CampoDaGrade::Login))
        .description(t!("casca-credencial-da-conexao-descricao"))
        .password(t!("comum-senha"), move || campos.senha)
        .disabled({
            let cofre = cofre.clone();
            move || cofre.ler(|c| c.bloqueado())
        })
        .hidden(esconde(CampoDaGrade::Login))
        .description(t!("casca-a-senha-do-item-da-credencial"))
        .choice_from(t!("casca-chave-do-cofre"), move || campos.chave_cofre, {
            let cofre = cofre.clone();
            move || opcoes_de_chave_do_cofre(&cofre)
        })
        .disabled({
            let cofre = cofre.clone();
            move || cofre.ler(|c| c.bloqueado())
        })
        .default(sem_chave_do_cofre())
        .hidden(esconde(CampoDaGrade::ChaveSsh))
        .description(t!("casca-uma-chave-ssh-guardada-no-cofre"))
        .file(t!("casca-chave-privada"), move || campos.chave)
        .hidden(esconde(CampoDaGrade::ChaveSsh))
        .description(t!("casca-o-arquivo-da-chave-openssh-pem"))
        .category(t!("comum-rede"))
        .choice_from(
            t!("comum-rede"),
            move || campos.rede,
            move || redes::opcoes_da_selecao(tree, selecionado),
        )
        .hidden(esconde(CampoDaGrade::Rede))
        .description(t!("casca-por-onde-a-conexao-sai-a"))
        .category(t!("ia-conexao-categoria"))
        .choice_from(
            t!("ia-conexao-linha"),
            move || campos.perfil_ia,
            move || assistente::opcoes_de_perfil_ia(perfis_de_ia),
        )
        .hidden(move || !ligada_ia() || esconde(CampoDaGrade::Ia)())
        .description(t!("ia-conexao-descricao"))
        .category(t!("mcp-categoria"))
        .choice_from(
            t!("mcp-linha"),
            move || campos.mcp,
            move || mcp_propriedades::opcoes_da_selecao(tree, selecionado),
        )
        .hidden(move || !ligada_mcp() || esconde(CampoDaGrade::Mcp)())
        .description(t!("mcp-linha-descricao"))
        .category("S3")
        .hidden(esconde(CampoDaGrade::S3))
        .choice(
            t!("casca-provedor"),
            move || campos.s3_provedor,
            S3Provedor::TODOS.map(S3Provedor::nome),
        )
        .default(S3Provedor::Amazon.nome())
        .description(t!("casca-s3-provedor-descricao"))
        .text(t!("comum-endereco"), move || campos.s3_endpoint)
        .description(t!("casca-o-endereco-do-servico-https-s3"))
        .text(t!("casca-regiao"), move || campos.s3_regiao)
        .default("us-east-1")
        .description(t!("casca-a-regiao-do-bucket-us-east"))
        .text(t!("casca-bucket"), move || campos.s3_bucket)
        .description(t!("casca-o-bucket-que-a-conexao-abre"))
        .choice(
            t!("casca-estilo-da-url"),
            move || campos.s3_estilo,
            S3Estilo::TODOS.map(S3Estilo::nome),
        )
        .default(S3Estilo::Caminho.nome())
        .description(t!("casca-caminho-servidor-bucket-minio-ceph-virtual"))
        .password(t!("casca-token-de-sessao"), move || campos.s3_token)
        .disabled({
            let cofre = cofre.clone();
            move || cofre.ler(|c| c.bloqueado())
        })
        .description(t!("casca-o-token-das-chaves-temporarias-da"))
        .category(t!("casca-s3-avancado"))
        .hidden(esconde(CampoDaGrade::S3))
        .collapsed()
        .text(t!("casca-classe-de-armazenamento"), move || {
            campos.s3_classe
        })
        .description(t!("casca-a-classe-dos-arquivos-enviados-standard"))
        .choice(
            t!("casca-certificado"),
            move || campos.s3_certificado,
            S3Certificado::TODOS.map(S3Certificado::nome),
        )
        .default(S3Certificado::Verificar.nome())
        .description(t!("casca-como-o-certificado-do-servidor-e"))
        .file(t!("casca-arquivo-da-ca"), move || campos.s3_ca)
        .disabled(sem_ca)
        .description(t!("casca-o-arquivo-pem-da-autoridade-que"))
        .category(t!("casca-rclone"))
        .hidden(esconde(CampoDaGrade::Rclone))
        .text(t!("comum-remoto"), move || campos.rclone_remoto)
        .description(t!("casca-o-nome-do-remoto-do-rclone"))
        .text(t!("casca-caminho"), move || campos.rclone_caminho)
        .description(t!("casca-a-pasta-do-remoto-em-que"))
        .category("FTP")
        .hidden(esconde(CampoDaGrade::Ftp))
        .choice(
            t!("casca-seguranca"),
            move || campos.ftp_seguranca,
            FtpSeguranca::TODAS.map(FtpSeguranca::nome),
        )
        .default(FtpSeguranca::default().nome())
        .description(t!("casca-como-a-conexao-e-protegida-ftp"))
        .choice(
            t!("casca-modo"),
            move || campos.ftp_modo,
            FtpModo::TODOS.map(FtpModo::nome),
        )
        .disabled(com_proxy)
        .default(FtpModo::default().nome())
        .description(t!("casca-passivo-este-computador-abre-a-conexao"))
        .category(t!("casca-ftp-avancado"))
        .hidden(esconde(CampoDaGrade::Ftp))
        .collapsed()
        .choice(
            t!("casca-versao-do-tls"),
            move || campos.ftp_tls,
            FtpTls::TODAS.map(FtpTls::nome),
        )
        .default(FtpTls::Auto.nome())
        .description(t!("casca-automatica-usa-o-tls-1-3"))
        .choice(
            t!("casca-codificacao"),
            move || campos.ftp_codificacao,
            FtpCodificacao::TODAS.map(FtpCodificacao::nome),
        )
        .default(FtpCodificacao::Auto.nome())
        .description(t!("casca-a-codificacao-dos-nomes-de-arquivo"))
        .number(
            t!("casca-fuso-do-servidor"),
            move || campos.ftp_fuso,
            -840.0..=840.0,
        )
        .default(0)
        .description(t!("casca-minutos-a-leste-do-utc-em"))
        .text(t!("casca-portas-do-modo-ativo"), move || campos.ftp_portas)
        .disabled(move || !ftp_ativo() || com_proxy())
        .description(t!("casca-a-faixa-de-portas-em-que"))
        .category(t!("casca-sessao"))
        .choice(t!("casca-tipo-de-terminal"), move || campos.term, TERMINAIS)
        .default(TERMINAIS[0])
        .hidden(esconde(CampoDaGrade::TipoDeTerminal))
        .description(t!("casca-o-term-que-o-servidor-recebe"))
        .file(t!("casca-log"), move || campos.log)
        .hidden(esconde(CampoDaGrade::Log))
        .description(t!("casca-o-arquivo-onde-a-saida-da"))
        .number(
            t!("casca-keepalive"),
            move || campos.keepalive,
            0.0..=3600.0,
        )
        .default(30)
        .hidden(esconde(CampoDaGrade::SessaoSsh))
        .description(t!("casca-segundos-entre-os-sinais-de-vida"))
        .check(t!("casca-comprimir-a-conexao"), move || campos.comprimir)
        .hidden(esconde(CampoDaGrade::SessaoSsh))
        .description(t!("casca-comprime-o-que-passa-pela-conexao"))
        .choice_from(
            t!("tmux-linha"),
            move || campos.persistente,
            move || sessao_tmux::opcoes_da_selecao(tree, selecionado),
        )
        .hidden(esconde(CampoDaGrade::Persistente))
        .description(t!("tmux-linha-descricao"))
        .choice_from(
            t!("mouse-linha"),
            move || campos.mouse,
            move || mouse::opcoes_da_selecao(tree, selecionado),
        )
        .hidden(esconde(CampoDaGrade::Mouse))
        .description(t!("mouse-linha-descricao"))
        .choice_from(
            t!("retentar-linha"),
            move || campos.retentar,
            move || retentar::opcoes_da_selecao(tree, selecionado),
        )
        .hidden(esconde(CampoDaGrade::Reconectar))
        .description(t!("retentar-linha-descricao"))
        .text(t!("casca-programa"), move || campos.local_programa)
        .hidden(esconde(CampoDaGrade::Programa))
        .description(t!("casca-o-programa-que-o-terminal-desta"))
        .text(t!("casca-argumentos"), move || campos.local_argumentos)
        .hidden(esconde(CampoDaGrade::Programa))
        .description(t!("casca-os-argumentos-do-programa-numa-linha"))
        .folder(t!("casca-pasta-inicial"), move || campos.local_pasta)
        .hidden(esconde(CampoDaGrade::Programa))
        .description(t!("casca-a-pasta-onde-o-terminal-local-comeca"))
        .category(t!("casca-colar-no-terminal"))
        .hidden(esconde(CampoDaGrade::Colar))
        .choice_from(
            t!("casca-ao-colar"),
            move || campos.colar_arquivos,
            opcoes_de_colar,
        )
        .default(rotulo_de_colar(None))
        .description(t!("casca-o-que-fazer-ao-colar-imagem"))
        .text(t!("casca-pasta-no-servidor"), move || campos.pasta_colar)
        .default("")
        .description(t!("casca-onde-o-que-e-colado-fica"))
        .choice(
            t!("casca-shell-do-servidor"),
            move || campos.shell_remoto,
            ShellRemoto::TODOS.map(ShellRemoto::nome),
        )
        .default(ShellRemoto::Automatico.nome())
        .description(t!("casca-o-shell-que-recebe-o-caminho"))
        .category(t!("ia-remoto-categoria"))
        .hidden(move || !ligada_ia() || esconde(CampoDaGrade::IaNoServidor)())
        .collapsed()
        .text(t!("ia-remoto-pasta"), move || campos.ia_pasta)
        .default("")
        .description(t!("ia-remoto-pasta-descricao"))
        .choice_from(
            t!("ia-remoto-instalar-por"),
            move || campos.ia_metodo,
            ia::opcoes_de_metodo,
        )
        .default(ia::rotulo_do_metodo(None))
        .description(t!("ia-remoto-instalar-por-descricao"))
        .text(t!("ia-remoto-argumentos-claude"), move || {
            campos.ia_argumentos[0]
        })
        .default("")
        .description(t!("ia-remoto-argumentos-descricao"))
        .text(t!("ia-remoto-argumentos-codex"), move || {
            campos.ia_argumentos[1]
        })
        .default("")
        .description(t!("ia-remoto-argumentos-descricao"))
        .text(t!("ia-remoto-argumentos-opencode"), move || {
            campos.ia_argumentos[2]
        })
        .default("")
        .description(t!("ia-remoto-argumentos-descricao"))
        .category(t!("casca-arquivos"))
        .hidden(esconde(CampoDaGrade::Arquivos))
        .folder(t!("casca-pasta-local-padrao"), move || campos.pasta_local)
        .description(t!("casca-a-pasta-do-computador-em-que"))
        .text(t!("casca-pasta-remota-padrao"), move || campos.pasta_remota)
        .description(t!("casca-a-pasta-do-servidor-em-que"))
        .choice(
            t!("comum-comparar-pastas"),
            move || campos.comparar,
            Ativar::TODOS.map(Ativar::nome),
        )
        .default(Ativar::Padrao.nome())
        .description(t!("casca-abre-os-dois-paineis-ja-comparando"))
        .choice(
            t!("casca-sincronizar-pastas"),
            move || campos.sincronizar_pastas,
            Ativar::TODOS.map(Ativar::nome),
        )
        .default(Ativar::Padrao.nome())
        .description(t!("casca-abre-os-dois-paineis-navegando-juntos"))
        .category(t!("comandos-categoria"))
        .hidden(esconde(CampoDaGrade::Comandos))
        .button(t!("comandos-linha"), move || resumo_comandos, {
            let t = Rc::clone(&tela);
            move || t.abrir_comandos()
        })
        .description(t!("comandos-descricao-da-linha"))
        .category(t!("casca-encaminhamento"))
        .hidden(esconde(CampoDaGrade::Encaminhamento))
        .choice_from(
            t!("casca-encaminhar-x11"),
            move || campos.x11,
            move || opcoes_de_x11(config.with(|c| c.x11.padrao)),
        )
        .default(ModoX11::Padrao.nome())
        .description(t!("casca-mostra-aqui-as-janelas-dos-programas"))
        .button(t!("casca-tuneis-linha"), move || resumo_tuneis, {
            let t = Rc::clone(&tela);
            move || t.abrir_tuneis()
        })
        .description(t!("casca-portas-que-o-ssh-encaminha-local"))
        .check(t!("casca-so-tuneis-sem-shell"), move || campos.so_tuneis)
        .description(t!("casca-a-conexao-so-segura-os-tuneis"))
        .check(t!("casca-parar-se-tunel-falhar"), move || {
            campos.parar_se_falhar
        })
        .description(t!("casca-como-o-exitonforwardfailure-se-uma-porta"))
        .category(t!("casca-jump-server"))
        .hidden(esconde(CampoDaGrade::Salto))
        .text(t!("casca-host-do-jump"), move || campos.jump_host)
        .description(t!("casca-o-servidor-de-salto-vazio-conexao"))
        .number(
            t!("casca-porta-do-jump"),
            move || campos.jump_porta,
            1.0..=65535.0,
        )
        .default(22)
        .description(t!("comum-a-porta-do-servidor-de-salto"))
        .text(t!("casca-usuario-do-jump"), move || campos.jump_usuario)
        .description(t!("casca-o-usuario-no-servidor-de-salto"))
        .choice_from(
            t!("casca-credencial-do-jump"),
            move || campos.jump_credencial,
            {
                let cofre = cofre.clone();
                move || opcoes_de_credencial(&cofre)
            },
        )
        .disabled({
            let cofre = cofre.clone();
            move || sem_salto() || cofre.ler(|c| c.bloqueado())
        })
        .default(perguntar())
        .description(t!("casca-o-item-do-cofre-com-o"))
        .password(t!("casca-senha-do-jump"), move || campos.jump_senha)
        .disabled({
            let cofre = cofre.clone();
            move || sem_salto() || cofre.ler(|c| c.bloqueado())
        })
        .description(t!("casca-a-senha-do-servidor-de-salto"))
        .visible(move || !sem_conexao());

    // Um grupo só tem a rede, os comandos, a permissão dos assistentes, a sessão persistente, o
    // mouse no terminal, a rodada nova de reconexão e o modo da credencial: valem para tudo o
    // que está dentro dele.
    let propriedades_do_grupo = PropertyGrid::new()
        .id("props-grupo")
        .grow(1)
        .category(t!("comum-rede"))
        .choice_from(
            t!("comum-rede"),
            move || campos.rede,
            move || redes::opcoes_da_selecao(tree, selecionado),
        )
        .description(t!("casca-por-onde-saem-as-conexoes-do"))
        .category(t!("mcp-categoria"))
        .hidden(move || !ligada_mcp())
        .choice_from(
            t!("mcp-linha"),
            move || campos.mcp,
            move || mcp_propriedades::opcoes_da_selecao(tree, selecionado),
        )
        .description(t!("mcp-linha-descricao-pasta"))
        .category(t!("casca-sessao"))
        .choice_from(
            t!("tmux-linha"),
            move || campos.persistente,
            move || sessao_tmux::opcoes_da_selecao(tree, selecionado),
        )
        .description(t!("tmux-linha-descricao-pasta"))
        .choice_from(
            t!("mouse-linha"),
            move || campos.mouse,
            move || mouse::opcoes_da_selecao(tree, selecionado),
        )
        .description(t!("mouse-linha-descricao-pasta"))
        .choice_from(
            t!("retentar-linha"),
            move || campos.retentar,
            move || retentar::opcoes_da_selecao(tree, selecionado),
        )
        .description(t!("retentar-linha-descricao-pasta"))
        .choice_from(
            t!("comum-credencial"),
            move || campos.credencial_da_pasta,
            move || credencial::opcoes_da_selecao(tree, selecionado),
        )
        .description(t!("comum-credencial-descricao-pasta"))
        .category(t!("comandos-categoria"))
        .button(t!("comandos-linha"), move || resumo_comandos, {
            let t = Rc::clone(&tela);
            move || t.abrir_comandos()
        })
        .description(t!("comandos-descricao-da-pasta"))
        .visible(e_grupo);

    Column::new()
        .child(propriedades)
        .child(propriedades_do_grupo)
        .child(
            Column::new()
                .padding(16)
                .grow(1)
                .visible(move || sem_conexao() && !e_grupo())
                .child(
                    Text::new(move || match selecionado.get() {
                        Some(chave) => match ChaveDeEspaco::de(&chave) {
                            Some(ChaveDeEspaco::Espaco(_)) => t!("casca-espaco-reabre"),
                            Some(_) => t!("casca-pastas-organizam"),
                            None => t!("casca-escolha-uma-conexao-editar"),
                        },
                        None => {
                            t!("casca-escolha-uma-conexao-propriedades")
                        }
                    })
                    .id("sem-propriedades")
                    .secondary()
                    .wrap(),
                ),
        )
}

/// A página Sobre.
/// O repositório do Tomni UI, a lib da interface. O do app vem do `Cargo.toml`.
const REPOSITORIO_DA_LIB: &str = "https://github.com/TirechCorp/tomni-ui";

fn pagina_sobre(tela: &Rc<Tela>) -> impl View {
    let link = |id: &str, texto: String, url: &'static str| {
        let tela = Rc::clone(tela);
        Button::new(texto).id(id).hyperlink().on_click(move || {
            let destino = crate::links::DestinoDoLink::Navegador(url.to_owned());
            if let Err(erro) = tela.opener.abrir_link(&destino) {
                tela.avisar(
                    InfoBarSeverity::Warning,
                    t!("links-nao-abriu", erro = erro.to_string()),
                );
            }
        })
    };
    Column::new()
        .id("sobre-pagina")
        .padding(Insets::new(28.0, 40.0, 24.0, 40.0))
        .gap(12)
        .child(Text::new(t!("casca-sobre-o-app")).title())
        .child(
            Text::new(t!("casca-versao", versao = env!("CARGO_PKG_VERSION")))
                .id("sobre-versao")
                .secondary(),
        )
        .child(Text::new(t!("casca-um-gerenciador-de-conexoes-ssh-shell")).wrap())
        .child(Text::new(t!("casca-licenca-mit-ou-apache-2-0")).secondary())
        .child(
            Column::new()
                .gap(0)
                .child(link(
                    "sobre-repositorio-app",
                    t!("sobre-repositorio-app"),
                    env!("CARGO_PKG_REPOSITORY"),
                ))
                .child(link(
                    "sobre-repositorio-lib",
                    t!("sobre-repositorio-lib"),
                    REPOSITORIO_DA_LIB,
                )),
        )
        .child(
            Text::new(t!(
                "sobre-idioma",
                nome = match crate::i18n::atual() {
                    crate::configuracoes::Idioma::PtBr => t!("opcao-idioma-pt-br"),
                    crate::configuracoes::Idioma::EsEs => t!("opcao-idioma-es-es"),
                    _ => t!("opcao-idioma-en-us"),
                }
            ))
            .id("sobre-idioma")
            .secondary(),
        )
}

/// O estado do app: as conexões, o cofre, os recentes e a janela principal. `opener` recebe a
/// fila de pedidos de chave do servidor (criada aqui, porque um signal só vive dentro do
/// `App`) e devolve quem abre as sessões. Devolve também a fila e se a última execução deixou
/// abas para reabrir.
fn nova_tela(
    pasta: PathBuf,
    ambiente: Ambiente,
    opener: impl FnOnce(FilaChaves) -> Rc<dyn Opener>,
) -> (Rc<Tela>, FilaChaves, bool) {
    let armazenamento = Armazenamento::new(pasta);
    // Uma segunda cópia do app não grava nada da configuração: a pasta de dados fica só de
    // leitura antes de qualquer leitura (que pode migrar, ou tirar do lugar um arquivo ilegível),
    // e a do cofre e a dos espaços, assim que se sabe onde estão.
    let so_leitura = !ambiente.dono_do_estado;
    if so_leitura {
        crate::armazenamento::somente_leitura_em(armazenamento.pasta());
        // O que ela muda vai para a principal: a leitura anota a base da junção.
        crate::instancias::seguir_a_principal(armazenamento.pasta());
    }
    controlar::limpar_arquivos_antigos(armazenamento.pasta());
    tomni_ui::dialogs::set_folder_memory(Rc::new(crate::pastas::Pastas::carregar(&armazenamento)));
    let (arvore_lida, aviso_inicial, visto_conexoes) = armazenamento.carregar_visto();
    let (configuracoes, avisos_da_configuracao) = Configuracoes::carregar(&armazenamento);
    let pasta_do_cofre = configuracoes.pasta_do_cofre(armazenamento.pasta());
    if so_leitura {
        crate::armazenamento::somente_leitura_em(&pasta_do_cofre);
    }
    let (cofre_lido, aviso_do_cofre) = Cofre::abrir(&pasta_do_cofre);
    let cofre = CofreCompartilhado::new(cofre_lido);
    let aviso_inicial = aviso_inicial.or(aviso_do_cofre);
    let estado_lido = estado::ler(&armazenamento);
    let dono_do_estado = ambiente.dono_do_estado;
    let tinha_abas = configuracoes.geral.restaurar_sessoes
        && dono_do_estado
        && estado_lido.as_ref().is_some_and(|e| e.tem_abas());
    let aviso_da_configuracao =
        (!avisos_da_configuracao.is_empty()).then(|| avisos_da_configuracao.join(" "));
    let aviso_inicial = aviso_inicial.or(aviso_da_configuracao);
    let (mut recentes_lidos, aviso_dos_recentes) = Recentes::carregar(&armazenamento);
    recentes_lidos.definir_limite(configuracoes.limite_de_recentes());
    let aviso_inicial = aviso_inicial.or(aviso_dos_recentes);
    let (ia_lido, aviso_da_ia) = ArquivoIa::carregar(&armazenamento);
    let aviso_inicial = aviso_inicial.or(aviso_da_ia);
    let pagina = signal(
        configuracoes
            .geral
            .pagina_inicial
            .chave(tinha_abas)
            .to_owned(),
    );

    let fila: FilaChaves = signal(Vec::new());
    // As Conexões e as Propriedades da principal: o arranjo delas é o da página Terminais.
    let paineis_terminais = PaineisDaPagina::novo(paineis::layout_inicial(
        estado_lido.as_ref(),
        paineis::TERMINAIS,
        None,
    ));
    let principal = Janela {
        handle: WindowHandle::current(),
        layout: signal(PaneLayout::with(Vec::new())),
        grade: signal((1, 1)),
        envio: Envio::novo(),
        ia_comando: signal(false),
        colar_dialogo: signal(false),
        mcp_aprovar: signal(false),
        paineis: paineis_terminais.layout,
    };
    let espacos = match configuracoes.janelas.pasta_espacos.trim() {
        "" => Espacos::new(armazenamento.pasta()),
        escolhida => Espacos::na_pasta(PathBuf::from(escolhida)),
    };
    if so_leitura {
        crate::armazenamento::somente_leitura_em(espacos.raiz());
    }
    let opener = opener(fila);
    let aviso: Signal<Option<(InfoBarSeverity, String)>> =
        signal(aviso_inicial.map(|a| (InfoBarSeverity::Warning, a)));
    let config = signal(configuracoes);
    let ferramentas = Ferramentas::new(config, cofre.clone(), Rc::clone(&opener), aviso, ambiente);
    let desbloquear = signal(cofre.agora(|c| c.bloqueado()));
    let alarmes = Alarmes::nova();
    cofre.proteger(config, alarmes.clone());
    cofre.coletar_anexos(
        armazenamento
            .pasta()
            .join(crate::anexos::REGISTRO_DE_ORFAOS),
    );
    let janelas = Janelas::new(principal);
    let estados_das_conexoes = estado_conexoes::estados_das_conexoes(janelas.clone());
    let tela = Rc::new_cyclic(|eu| Tela {
        armazenamento,
        opener,
        itens: signal(itens_completos(&arvore_lida.nodes, &espacos)),
        tree: signal(arvore_lida),
        selecionado: signal(None),
        abertos: signal(BTreeSet::new()),
        filtro: signal(String::new()),
        filtro_aberto: signal(false),
        resumo_tuneis: signal(String::new()),
        resumo_comandos: signal(String::new()),
        renomeando: signal(None),
        campos: Campos::novo(),
        janelas,
        ativa: active_window(),
        espacos,
        salvar_aberto: signal(false),
        salvar_nome: signal(String::new()),
        salvar_pasta: signal(raiz_dos_espacos()),
        salvar_nova_pasta: signal(String::new()),
        salvar_erro: signal(String::new()),
        salvar_resumo: signal(String::new()),
        a_salvar: RefCell::new(None),
        depois_de_salvar: Cell::new(Depois::Nada),
        substituir_aberto: signal(false),
        a_substituir: RefCell::new(None),
        apagar_espaco: signal(false),
        contador: signal(0),
        aviso,
        apagar: signal(false),
        pendentes: signal(Vec::new()),
        cofre,
        vigiadas: signal(Vec::new()),
        pagina,
        recentes: signal(recentes_lidos),
        config,
        busca_config: signal(String::new()),
        avisos_config: signal(avisos_da_configuracao),
        adiado: signal(false),
        ferramentas,
        desbloquear,
        refazer_desbloqueio: Rc::new(Cell::new(false)),
        copiar_chave: RefCell::new(None),
        fechar_tudo: signal(false),
        fechar_ou_bandeja: signal(false),
        nao_perguntar: signal(false),
        abas: abas_caidas::Estado::novo(),
        zoom_fonte: ZoomFonte::novo(),
        cluster: cluster::Estado::novo(),
        scripts: scripts::Estado::novo(),
        links: lancador_links::Estado::novo(),
        wsl: wsl::Estado::novo(),
        instalar: instalar::Estado::novo(),
        ia: ia::Estado::novo(),
        duplicar: duplicar::Estado::novo(),
        tmux: sessao_tmux::Estado::novo(),
        sessoes_servidor: sessoes_servidor::Estado::novo(),
        baixar: baixar::Estado::novo(),
        controlar: controlar::Estado::novo(),
        escondido: signal(false),
        atalhos_aberto: signal(false),
        estados_das_conexoes,
        so_para_pergunta: Cell::new(false),
        bandeja_conferida: signal(false),
        portatil: cofre_portatil::Estado::novo(),
        importacao: importacao::Estado::novo(),
        notas: EstadoNotas::novo(
            estado_lido.as_ref().and_then(|e| e.notas.as_ref()),
            config.with_untracked(|c| c.notas.modo),
        ),
        paineis_terminais,
        paineis_notas: PaineisDaPagina::novo(paineis::layout_inicial(
            estado_lido.as_ref(),
            paineis::NOTAS,
            estado_lido
                .as_ref()
                .and_then(|e| e.notas.as_ref())
                .and_then(|n| n.lateral),
        )),
        paineis_arquivos: PaineisDaPagina::novo(paineis::layout_inicial(
            estado_lido.as_ref(),
            paineis::ARQUIVOS,
            None,
        )),
        pedido_de_nota: signal(None),
        gravar_nota: Rc::new(RefCell::new(None)),
        depois_de_desbloquear: RefCell::new(None),
        logs_esperando_o_cofre: Cell::new(false),
        logs_pagina: pagina_logs::Estado::novo(),
        liberado: Cell::new(false),
        encerrando: Cell::new(false),
        so_leitura,
        dona_aberta: signal(so_leitura),
        visto_conexoes: RefCell::new(visto_conexoes),
        alarmes,
        realce_menu: signal(None),
        identificando: signal(None),
        transferencias: {
            Transfers::new(config.with_untracked(|c| c.arquivos.paralelas as usize))
        },
        abas_arquivos: signal(Vec::new()),
        aba_arquivos: signal(String::new()),
        proxima_aba: Cell::new(0),
        iniciais_arquivos: RefCell::default(),
        edicoes: RefCell::default(),
        colar: colar::EstadoColar::novo(),
        proxima_edicao: Cell::new(0),
        vigia: crate::arquivos::editar::Vigia::novo(),
        edicao_pergunta: signal(Vec::new()),
        fechando_aba: RefCell::default(),
        fechando_aberto: signal(false),
        fechando_quantas: signal(0),
        rclone: rclone::EstadoRclone::novo(),
        usos: Usos::default(),
        motores: Motores::default(),
        nebula_mudou: signal(0),
        motores_vpn: MotoresVpn::default(),
        motores_wireguard: MotoresWireGuard::default(),
        motores_tailscale: MotoresTailscale::default(),
        motores_zerotier: MotoresZeroTier::default(),
        rede_das_sessoes: RefCell::default(),
        reconectando: Rc::default(),
        rede_das_abas: RefCell::default(),
        eu: eu.clone(),
        comandos: comandos::EstadoComandos::novo(),
        tuneis: tuneis::EstadoTuneis::novo(),
        editor_comandos: comandos_editor::EstadoEditorComandos::novo(),
        programas: programas::EstadoProgramas::novo(),
        assistente: assistente::EstadoAssistente::novo(ia_lido),
        instancias: instancias::Estado::novo(),
        mcp: mcp::EstadoMcp::novo(),
        portas: portas::EstadoPortas::novo(),
    });
    let abridor_de_login = Arc::clone(&tela.ferramentas.ambiente().abridor_de_login);
    tela.motores_tailscale
        .definir_abridor(move |url| abridor_de_login(url));
    tela.ligar_configuracoes();
    tela.ligar_tuneis();
    tela.ligar_comandos();
    tela.ligar_assistente();
    tela.ligar_mcp();
    tela.ligar_instancias();
    tela.ligar_portas();
    tela.podar_recentes();
    tela.ferramentas.iniciar();
    (tela, fila, tinha_abas)
}

/// A tela do app, com as conexões guardadas em `pasta`. `opener` recebe a fila de pedidos de
/// chave do servidor e devolve quem abre as sessões. É a casca: a barra de título com a busca e
/// o painel de navegação, cujas páginas são o Início, os Terminais, o Cofre, as Configurações
/// e o Sobre.
pub fn tela_com(pasta: PathBuf, opener: impl FnOnce(FilaChaves) -> Rc<dyn Opener>) -> impl View {
    let ambiente = Ambiente::do_sistema(&pasta);
    tela_com_ambiente(pasta, ambiente, opener)
}

/// Como [`tela_com`], com a pasta das chaves SSH em `pasta_ssh` em vez de `~/.ssh` (os testes
/// usam uma pasta temporária).
pub fn tela_com_ssh(
    pasta: PathBuf,
    pasta_ssh: PathBuf,
    opener: impl FnOnce(FilaChaves) -> Rc<dyn Opener>,
) -> impl View {
    let ambiente = Ambiente::de_teste(&pasta, pasta_ssh);
    tela_com_ambiente(pasta, ambiente, opener)
}

/// Como [`tela_com`], com o ambiente (pastas, endereço do agente e programas do sistema) dado
/// por quem chama: os testes põem um executor que anota os comandos.
pub fn tela_com_ambiente(
    pasta: PathBuf,
    ambiente: Ambiente,
    opener: impl FnOnce(FilaChaves) -> Rc<dyn Opener>,
) -> impl View {
    tela_com_ferramentas(pasta, ambiente, opener, |_| {})
}

/// Como [`tela_com_ambiente`], e entrega as [`Ferramentas`] (o agente, a configuração) a
/// `receber`: os testes mexem no agente por elas.
pub fn tela_com_ferramentas(
    pasta: PathBuf,
    ambiente: Ambiente,
    opener: impl FnOnce(FilaChaves) -> Rc<dyn Opener>,
    receber: impl FnOnce(Rc<Ferramentas>),
) -> impl View {
    let (tela, fila, _) = nova_tela(pasta, ambiente, opener);
    receber(Rc::clone(&tela.ferramentas));
    tela.ligar_bandeja();
    tela.vigiar_cofre();
    tela.vigiar_conexoes();
    tela.vigiar_logs();
    tela.vigiar_chave_dos_logs();
    tela.vigiar_redes();
    tela.vigiar_assistentes();
    let pagina = tela.pagina;
    let cofre = tela.cofre.clone();
    let aviso = tela.aviso;
    let (terminais, dialogos) = pagina_terminais(&tela, fila);

    // O painel fica só com os ícones em Terminais, para a área de trabalho ter a largura; nas
    // outras páginas segue a largura da janela.
    let modo_do_painel = move || (pagina.get() == "terminais").then_some(NavigationMode::Compact);

    let barra_aviso = move |id: &str, severidade: InfoBarSeverity| {
        InfoBar::new(severidade)
            .id(id)
            .message(move || aviso.get().map(|(_, m)| m).unwrap_or_default())
            .visible(move || aviso.get().is_some_and(|(s, _)| s == severidade))
            .on_close(move || aviso.set(None))
    };

    let escolher_item: Signal<Option<u64>> = signal(None);
    let exportar_item: Signal<Option<u64>> = signal(None);

    let pagina_das_notas = pagina_notas(ContextoNotas {
        cofre: tela.cofre.clone(),
        config: tela.config,
        alarmes: tela.alarmes.clone(),
        estado: tela.notas,
        pedido: tela.pedido_de_nota,
        abridor: Rc::clone(&tela.ferramentas.ambiente().abridor),
        espera: tela.ferramentas.ambiente().espera_da_nota,
        ao_fechar: Rc::clone(&tela.gravar_nota),
        paineis: tela.paineis_notas.clone(),
    });

    let acoes = AcoesInicio {
        nova: {
            let tela = Rc::clone(&tela);
            Rc::new(move || {
                tela.nova_conexao();
                tela.pagina.set("terminais".to_owned());
            })
        },
        importar: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.importar())
        },
        shell: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.abrir_shell_e_mostrar())
        },
        abrir: {
            let tela = Rc::clone(&tela);
            Rc::new(move |alvo: &AlvoRecente| tela.abrir_recente(alvo))
        },
        abrir_em: {
            let tela = Rc::clone(&tela);
            Rc::new(move |menu, alvo: &AlvoRecente, base: &str| match alvo {
                AlvoRecente::Conexao { id } => {
                    abrir_em::com_abrir_em(&tela, menu, base, AlvoAbrir::Conexao(*id))
                }
                _ => menu,
            })
        },
        fixar: {
            let tela = Rc::clone(&tela);
            Rc::new(move |alvo: &AlvoRecente, fixado| {
                tela.recentes.update(|r| r.fixar(alvo, fixado));
            })
        },
        mostrar: {
            let tela = Rc::clone(&tela);
            Rc::new(move |alvo: &AlvoRecente| match alvo {
                AlvoRecente::Conexao { id } => tela.mostrar_na_arvore(*id),
                AlvoRecente::Espaco { caminho } => {
                    tela.mostrar_espaco_na_arvore(&ChaveDeEspaco::Espaco(caminho.clone()));
                }
                AlvoRecente::Local { conexao, .. } | AlvoRecente::Aplicativo { conexao, .. } => {
                    tela.mostrar_na_arvore(*conexao);
                }
            })
        },
        remover: {
            let tela = Rc::clone(&tela);
            Rc::new(move |alvo: &AlvoRecente| tela.recentes.update(|r| r.remover(alvo)))
        },
    };
    let ferramentas = vec![
        cartao_ferramenta(
            Symbol::LockClosed,
            &t!("inicio-ferramenta-cofre"),
            &t!("inicio-ferramenta-cofre-detalhe"),
            "cofre",
            pagina,
        ),
        cartao_ferramenta(
            Symbol::Key,
            &t!("inicio-ferramenta-chaves"),
            &t!("inicio-ferramenta-chaves-detalhe"),
            "chaves",
            pagina,
        ),
        cartao_ferramenta(
            Symbol::Shield,
            &t!("inicio-ferramenta-agente"),
            &t!("inicio-ferramenta-agente-detalhe"),
            "agente",
            pagina,
        ),
        cartao_ferramenta(
            Symbol::Globe,
            &t!("inicio-ferramenta-redes"),
            &t!("inicio-ferramenta-redes-detalhe"),
            "redes",
            pagina,
        ),
        cartao_ferramenta(
            Symbol::Code,
            &t!("inicio-ferramenta-scripts"),
            &t!("inicio-ferramenta-scripts-detalhe"),
            "scripts",
            pagina,
        ),
        cartao_ferramenta(
            Symbol::Settings,
            &t!("inicio-ferramenta-configuracoes"),
            &t!("inicio-ferramenta-configuracoes-detalhe"),
            "configuracoes",
            pagina,
        ),
    ];
    let inicio = {
        let tela = Rc::clone(&tela);
        pagina_inicio(move || tela.cartoes(), acoes, ferramentas)
    };
    let configuracoes_pagina = pagina_configuracoes(Contexto {
        config: tela.config,
        recentes: tela.recentes,
        pagina,
        busca: tela.busca_config,
        avisos_da_leitura: tela.avisos_config,
        armazenamento: tela.armazenamento.clone(),
        pasta_padrao: crate::armazenamento::pasta_padrao(),
        origem_da_pasta: if tela.ferramentas.ambiente().portatil.is_some() {
            crate::armazenamento::OrigemDaPasta::Portatil
        } else {
            crate::armazenamento::origem_da_pasta()
        },
        cofre: tela.cofre.clone(),
        importar_ssh_config: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.importar())
        },
        importar_aws: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.importar_aws())
        },
        levar_cofre: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.levar_cofre())
        },
        abrir_cofre_de_fora: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.abrir_cofre_de_fora())
        },
        logs_a_cifrar: tela.logs_pagina.a_cifrar,
        cifrar_logs_existentes: {
            let tela = Rc::clone(&tela);
            Rc::new(move || {
                // A confirmação mora na página Logs.
                tela.pagina.set("logs".to_owned());
                tela.pedir_para_converter_logs(crate::logs::Sentido::Cifrar, true);
            })
        },
        mudar_local_do_cofre: {
            let tela = Rc::clone(&tela);
            Rc::new(move |destino| tela.pedir_local_do_cofre(destino))
        },
        arvore: tela.tree,
        mcp_estado: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.estado_do_controle())
        },
        mcp_aviso: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.mostrar_aviso_mcp())
        },
        mcp_como: {
            let tela = Rc::clone(&tela);
            Rc::new(move |espaco| tela.abrir_como_ligar(espaco.as_deref()))
        },
        espaco_novo: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.novo_espaco_ia(None, ""))
        },
        espaco_editar: {
            let tela = Rc::clone(&tela);
            Rc::new(move |nome| tela.editar_espaco_ia(&nome))
        },
        abrir_instalacao: {
            let tela = Rc::clone(&tela);
            Rc::new(move || tela.na_principal(|t| t.abrir_instalar(false)))
        },
        resumo_da_instalacao: instalar::resumo_para_configuracoes(&tela),
        pasta_dos_assistentes: {
            let tela = Rc::clone(&tela);
            Rc::new(move |escolhida| tela.ferramentas.ambiente().pasta_dos_assistentes(escolhida))
        },
        pasta_pessoal: tela.ferramentas.ambiente().pessoal.clone(),
    });

    let contexto_arquivos = arquivos::contexto(&tela);
    let pagina_arquivos = arquivos::pagina(&tela, &contexto_arquivos);
    let dialogos_arquivos = arquivos::dialogos(&tela, &contexto_arquivos);
    let dialogos_rclone = rclone::dialogos(&tela);
    let dialogos_ia = assistente::dialogos(&tela);
    let dialogos_mcp = Column::new()
        .child(mcp_como::dialogos(&tela))
        .child(mcp_espacos::dialogos(&tela));

    let chaves_pagina = pagina_chaves(
        Rc::clone(&tela.ferramentas),
        tela.acoes_de_chaves(escolher_item, exportar_item),
    );
    let agente_pagina = pagina_agente(Rc::clone(&tela.ferramentas));
    let dialogos_chaves = chaves::dialogos_de_chaves(&tela);
    let redes_pagina = redes::pagina(&tela);
    let ia_pagina = ia_perfis::pagina(&tela);
    let logs_pagina = pagina_logs::pagina(&tela);
    let scripts_pagina = scripts::pagina(&tela);
    wsl::ligar(&tela);
    instalar::ligar(&tela);
    ia::ligar(&tela);
    duplicar::ligar(&tela);
    sessao_tmux::ligar(&tela);
    sessoes_servidor::ligar(&tela);
    let medidas = desempenho::ligar(&tela);

    let navegacao = NavigationView::bind(pagina)
        .id("navegacao")
        .pane_mode_with(modo_do_painel)
        .item("inicio", Symbol::Home, t!("casca-inicio"), inicio)
        .item(
            "terminais",
            Symbol::WindowConsole,
            t!("casca-terminais"),
            terminais,
        )
        .item(
            "arquivos",
            Symbol::Folder,
            t!("casca-arquivos"),
            pagina_arquivos,
        )
        .item(
            "cofre",
            Symbol::LockClosed,
            t!("casca-cofre-de-senhas"),
            pagina_cofre_completa(
                cofre,
                escolher_item,
                Some(Rc::new(move |id: u64| {
                    exportar_item.set(Some(id));
                    pagina.set("chaves".to_owned());
                })),
                Some(Portabilidade {
                    levar: {
                        let tela = Rc::clone(&tela);
                        Rc::new(move || tela.levar_cofre())
                    },
                    abrir: {
                        let tela = Rc::clone(&tela);
                        Rc::new(move || tela.abrir_cofre_de_fora())
                    },
                }),
                Some(Rc::new({
                    let tela = Rc::clone(&tela);
                    move |id: u64| {
                        tela.pedido_de_nota.set(Some(id));
                        pagina.set("notas".to_owned());
                    }
                })),
            ),
        )
        .item(
            "notas",
            Symbol::Notepad,
            t!("comum-notas"),
            pagina_das_notas,
        )
        .item("chaves", Symbol::Key, t!("casca-chaves-ssh"), chaves_pagina)
        .item("agente", Symbol::Shield, t!("casca-agente"), agente_pagina)
        .item("redes", Symbol::Globe, t!("casca-redes"), redes_pagina)
        .item("ia", Symbol::Sparkle, t!("casca-ia"), ia_pagina)
        .item("logs", Symbol::DocumentText, t!("casca-logs"), logs_pagina)
        .item("scripts", Symbol::Code, t!("casca-scripts"), scripts_pagina)
        .item(
            desempenho::PAGINA,
            Symbol::DataArea,
            t!("casca-desempenho"),
            desempenho::pagina(&tela, medidas),
        )
        .footer_item(
            "sobre",
            Symbol::Info,
            t!("casca-sobre"),
            pagina_sobre(&tela),
        )
        .settings(configuracoes_pagina);

    let busca = campo_de_busca(&tela, pagina, escolher_item);

    // Com outras janelas abertas, a principal se diz principal, no título do sistema (o que a
    // lista de janelas e o Alt+Tab mostram) e na barra. A plataforma lê o título a cada quadro,
    // fora de um efeito: essa leitura não rastreia. O subtítulo rastreia, para a barra refazer o
    // layout quando ele aparece ou some.
    let titulo = {
        let janelas = tela.janelas.clone();
        move || {
            if janelas.quantas_agora() > 1 {
                t!("casca-titulo-da-principal")
            } else {
                NOME_DO_APP.to_owned()
            }
        }
    };
    let janelas = tela.janelas.clone();
    let subtitulo = move || {
        if janelas.quantas() > 1 {
            t!("casca-subtitulo-da-principal")
        } else {
            String::new()
        }
    };
    Form::new(titulo)
        .density(Density::Compact)
        .title_bar(
            TitleBar::new(NOME_DO_APP)
                .icon_image(icone::PEQUENO)
                .title_badge(marca::selo())
                .subtitle(subtitulo)
                .content(busca)
                .right_header(
                    Row::new()
                        .gap(4)
                        .align(Align::Center)
                        .child(desempenho::medidor(&tela, medidas))
                        .child(tema::botao_de_tema(tela.config)),
                ),
        )
        .child(
            Column::new()
                .accelerator(atalhos::atalho(tela.config, "cofre"), move || {
                    pagina.set("cofre".to_owned())
                })
                .accelerator(atalhos::atalho(tela.config, "notas"), move || {
                    pagina.set("notas".to_owned())
                })
                .accelerator(atalhos::atalho(tela.config, "configuracoes"), move || {
                    pagina.set("configuracoes".to_owned())
                })
                .child(barra_aviso("aviso-erro", InfoBarSeverity::Error))
                .child(barra_aviso("aviso-atencao", InfoBarSeverity::Warning))
                .child(barra_aviso("aviso-sucesso", InfoBarSeverity::Success))
                .child(barra_aviso("aviso-info", InfoBarSeverity::Informational))
                .child(aviso_so_leitura(&tela))
                .child(bandeja::aviso_sem_bandeja(&tela))
                .child(cofre_portatil::aviso_de_conflito(&tela))
                .child(programas::avisos(&tela))
                .child(ia::avisos(&tela))
                .child(controlar::avisos(&tela))
                .child(instalar::aviso(&tela))
                .child(navegacao)
                .child(dialogos)
                .child(dialogos_chaves)
                .child(dialogos_arquivos)
                .child(dialogos_rclone)
                .child(dialogos_ia)
                .child(dialogos_mcp)
                .child(cofre_portatil::dialogos(&tela))
                .child(importacao::dialogos(&tela)),
        )
}

/// A barra fixa de uma cópia só de leitura do app (outra cópia é a dona da configuração), com o
/// "Reiniciar": enquanto a dona está aberta, a cópia nova também abre só de leitura; depois que
/// ela fecha, a cópia nova é a dona.
fn aviso_so_leitura(tela: &Rc<Tela>) -> impl View + use<> {
    let dona_aberta = tela.dona_aberta;
    InfoBar::new(InfoBarSeverity::Warning)
        .id("so-leitura")
        .title(t!("armazenamento-so-leitura-titulo"))
        .message(move || {
            if dona_aberta.get() {
                t!("armazenamento-so-leitura-aviso")
            } else {
                t!("armazenamento-so-leitura-dona-fechou")
            }
        })
        .action(
            t!("armazenamento-so-leitura-reiniciar"),
            crate::reinicio::pedir,
        )
        .visible(tela.so_leitura)
}

/// A busca global da barra de título: conexões e itens do cofre. Escolher uma conexão a abre
/// nos Terminais; escolher um item do cofre abre o Cofre com ele selecionado.
fn campo_de_busca(
    tela: &Rc<Tela>,
    pagina: Signal<String>,
    escolher_item: Signal<Option<u64>>,
) -> AutoSuggestBox {
    let texto = signal(String::new());
    let ultimas: Rc<RefCell<HashMap<String, Destino>>> = Rc::default();

    let sugestoes = {
        let tela = Rc::clone(tela);
        move |procurado: &str| -> Vec<(String, Destino)> {
            untrack(|| {
                tela.tree.with_untracked(|t| {
                    let conexoes: Vec<&Connection> = t.connections();
                    let mut lista = tela.cofre.agora(|c| {
                        let itens: Option<Vec<(Item, String)>> = (!c.bloqueado()).then(|| {
                            c.itens()
                                .iter()
                                .map(|i| (i.clone(), c.caminho(i.pasta)))
                                .collect()
                        });
                        busca::sugerir(procurado, &conexoes, itens.as_deref())
                    });
                    lista.extend(
                        tela.config
                            .with_untracked(|c| busca::sugerir_configuracoes(procurado, c)),
                    );
                    lista
                })
            })
        }
    };

    let sugerir = {
        let (sugestoes, ultimas) = (sugestoes.clone(), Rc::clone(&ultimas));
        move |procurado: &str| -> Vec<String> {
            let lista = sugestoes(procurado);
            let mut mapa = ultimas.borrow_mut();
            mapa.clear();
            mapa.extend(lista.iter().cloned());
            lista.into_iter().map(|(rotulo, _)| rotulo).collect()
        }
    };

    let tela = Rc::clone(tela);
    AutoSuggestBox::bind(texto, sugerir)
        .id("busca")
        .placeholder(t!("casca-buscar-conexoes-e-itens-do-cofre"))
        .query_icon(Symbol::Search)
        .width(360)
        .focus_shortcut(atalhos::atalho(tela.config, "busca"))
        .on_submit(move |enviado: String| {
            let destino = ultimas
                .borrow()
                .get(&enviado)
                .copied()
                .or_else(|| sugestoes(&enviado).first().map(|(_, destino)| *destino));
            let Some(destino) = destino else {
                return;
            };
            match destino {
                Destino::Conexao(id) => tela.abrir_e_mostrar(id),
                Destino::Cofre(id) => {
                    let e_nota = tela
                        .cofre
                        .agora(|c| c.item(id).is_some_and(|i| i.tipo == Tipo::Nota));
                    if e_nota {
                        tela.pedido_de_nota.set(Some(id));
                        pagina.set("notas".to_owned());
                    } else {
                        escolher_item.set(Some(id));
                        pagina.set("cofre".to_owned());
                    }
                }
                Destino::Configuracao(chave) => {
                    let titulo =
                        catalogo::definicao(chave).map_or_else(String::new, |d| d.titulo());
                    tela.busca_config.set(titulo);
                    pagina.set("configuracoes".to_owned());
                }
            }
            texto.set(String::new());
        })
}

/// O que o diálogo da chave mostra: o servidor, o algoritmo e a impressão digital. Se a chave
/// mudou, um aviso de erro no alto.
fn conteudo_da_chave(fila: FilaChaves, mudou: bool) -> impl View {
    // O texto do pedido que está na frente da fila.
    let dado = move |f: fn(&PedidoChave) -> String| {
        move || ler_fila(fila, |fila| fila.first().map(|p| f(p)).unwrap_or_default())
    };
    let mut coluna = Column::new().gap(8);
    if mudou {
        coluna = coluna.child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("chave-mudou-aviso")
                .title(t!("casca-possivel-ataque"))
                .message(t!("casca-o-servidor-apresentou-uma-chave-diferente")),
        );
    } else {
        coluna = coluna.child(Text::new(t!("casca-e-a-primeira-vez-que-este")).wrap());
    }
    coluna
        .child(
            Text::new(dado(|p| {
                t!("casca-chave-servidor", valor = p.host_porta.as_str())
            }))
            .id("chave-servidor"),
        )
        .child(
            Text::new(dado(|p| {
                t!("casca-chave-algoritmo", valor = p.algoritmo.as_str())
            }))
            .id("chave-algoritmo"),
        )
        .child(
            Text::new(dado(|p| {
                t!(
                    "casca-chave-impressao-digital",
                    valor = p.fingerprint.as_str()
                )
            }))
            .id("chave-fingerprint")
            .wrap(),
        )
}

/// O que o diálogo do certificado do FTPS mostra: o motivo da desconfiança, o servidor, o
/// assunto, o emissor, a validade e a impressão digital.
fn conteudo_do_certificado(fila: FilaChaves) -> impl View {
    let dado = move |f: fn(&PedidoChave, &DadosCertificado) -> String| {
        move || {
            ler_fila(fila, |fila| {
                fila.first()
                    .and_then(|p| p.certificado.as_ref().map(|c| f(p, c)))
                    .unwrap_or_default()
            })
        }
    };
    Column::new()
        .gap(8)
        .child(
            InfoBar::new(InfoBarSeverity::Warning)
                .id("certificado-aviso")
                .title(t!("casca-o-sistema-nao-confia-neste-certificado"))
                .message(dado(|_, c| c.motivo.clone())),
        )
        .child(
            Text::new(dado(|p, _| {
                t!("casca-chave-servidor", valor = p.host_porta.as_str())
            }))
            .id("certificado-servidor"),
        )
        .child(
            Text::new(dado(|_, c| {
                t!("casca-certificado-assunto", valor = c.assunto.as_str())
            }))
            .id("certificado-assunto")
            .wrap(),
        )
        .child(
            Text::new(dado(|_, c| {
                t!("casca-certificado-emissor", valor = c.emissor.as_str())
            }))
            .id("certificado-emissor")
            .wrap(),
        )
        .child(
            Text::new(dado(|_, c| {
                t!("casca-certificado-validade", valor = c.validade.as_str())
            }))
            .id("certificado-validade"),
        )
        .child(
            Text::new(dado(|p, _| {
                t!(
                    "casca-chave-impressao-digital",
                    valor = p.fingerprint.as_str()
                )
            }))
            .id("certificado-impressao")
            .wrap(),
        )
}
