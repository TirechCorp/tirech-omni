//! As preferências do app (`configuracoes.json`, M35). Um objeto por categoria, todas com
//! padrão para tudo: um campo que falta, ou que tem o tipo errado, volta ao padrão sozinho e
//! os outros ficam. Campos que este código não conhece sobrevivem a uma gravação, para uma
//! versão nova do app não perder o que a velha não entende.
//!
//! O arquivo **não guarda segredo**: os segredos moram no cofre, e a configuração guarda no
//! máximo o id do item (`rclone.credencial`). É isso que deixa exportar sem segredos.

pub mod catalogo;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::agente::ChaveSalva;
use crate::armazenamento::Armazenamento;
use crate::modelo::{FtpCodificacao, FtpModo, FtpSeguranca, FtpTls, S3Estilo};
use crate::{t, tk};

/// Menor e maior número de recentes que se pode guardar.
pub const RECENTES_MIN: u32 = 5;
/// Ver [`RECENTES_MIN`].
pub const RECENTES_MAX: u32 = 100;

/// Os caminhos dos assistentes deste computador (M63): são da máquina e não vão na exportação.
const CAMINHOS_LOCAIS_DA_IA: [&str; 3] = [
    "local_claude_caminho",
    "local_codex_caminho",
    "local_opencode_caminho",
];

/// A versão do formato do arquivo (o da casca, sem `versao`, era a 1).
pub const VERSAO: u32 = 2;

/// Declara uma escolha da configuração: o enum em `snake_case` no arquivo, com o primeiro
/// valor como padrão, o nome na tela (uma chave dos `.ftl`, escrita `tk!("…")`) e a busca pelo
/// nome.
macro_rules! escolha {
    (
        $(#[$meta:meta])*
        $nome:ident {
            $( $(#[$vmeta:meta])* $variante:ident => $chave:expr ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $nome {
            $( $(#[$vmeta])* $variante ),+
        }

        impl $nome {
            /// Todas as opções, na ordem em que a página mostra.
            pub const TODAS: &'static [Self] = &[ $( Self::$variante ),+ ];

            /// A chave do nome nos `.ftl`.
            pub fn chave(self) -> &'static str {
                match self { $( Self::$variante => $chave ),+ }
            }

            /// O nome na tela, no idioma atual.
            pub fn nome(self) -> String {
                $crate::i18n::texto(self.chave(), None)
            }

            /// A opção de um nome da tela.
            pub fn do_nome(nome: &str) -> Option<Self> {
                Self::TODAS.iter().copied().find(|o| o.nome() == nome)
            }

            /// Os nomes de todas as opções, para a lista de escolha.
            pub fn nomes() -> Vec<String> {
                Self::TODAS.iter().map(|o| o.nome()).collect()
            }
        }
    };
}

/// Qual página abre ao iniciar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaginaInicial {
    /// Sempre o Início.
    Inicio,
    /// Sempre os Terminais.
    Terminais,
    /// Terminais se havia abas abertas na última vez; senão, Início.
    // Último por exigência do serde: um valor que esta versão não conhece também vira `Auto`.
    #[default]
    #[serde(other)]
    Auto,
}

impl PaginaInicial {
    /// As três opções, na ordem em que a página de Configurações mostra.
    pub const TODAS: [Self; 3] = [Self::Auto, Self::Inicio, Self::Terminais];

    /// O nome na tela, no idioma atual.
    pub fn nome(self) -> String {
        let chave = match self {
            Self::Auto => tk!("opcao-pagina-inicial-auto"),
            Self::Inicio => tk!("opcao-pagina-inicial-inicio"),
            Self::Terminais => tk!("opcao-pagina-inicial-terminais"),
        };
        t!(chave)
    }

    /// A opção de um nome da tela.
    pub fn do_nome(nome: &str) -> Option<Self> {
        Self::TODAS.into_iter().find(|p| p.nome() == nome)
    }

    /// A chave da página do `NavigationView`: `tinha_abas` diz se o estado restaurou alguma aba.
    pub fn chave(self, tinha_abas: bool) -> &'static str {
        match self {
            Self::Inicio => "inicio",
            Self::Terminais => "terminais",
            Self::Auto if tinha_abas => "terminais",
            Self::Auto => "inicio",
        }
    }
}

escolha! {
    /// O tema do app.
    Tema {
        /// O do sistema.
        #[default]
        Sistema => tk!("opcao-tema-sistema"),
        /// Sempre claro.
        Claro => tk!("opcao-tema-claro"),
        /// Sempre escuro.
        Escuro => tk!("opcao-tema-escuro"),
    }
}

escolha! {
    /// O idioma da interface (vale ao reiniciar). No arquivo: `"sistema"`, `"pt_br"`, `"en_us"` ou
    /// `"es_es"`.
    /// Os nomes saem na própria língua, como no Windows.
    Idioma {
        /// O idioma do sistema: `pt*` dá pt-BR, `es*` dá espanhol e qualquer outro dá en-US.
        #[default]
        Sistema => tk!("opcao-idioma-sistema"),
        /// Português do Brasil.
        PtBr => tk!("opcao-idioma-pt-br"),
        /// Inglês dos Estados Unidos.
        EnUs => tk!("opcao-idioma-en-us"),
        /// Espanhol.
        EsEs => tk!("opcao-idioma-es-es"),
    }
}

escolha! {
    /// O esquema de cores do terminal.
    Esquema {
        /// O Campbell, o padrão.
        #[default]
        Campbell => tk!("opcao-esquema-campbell"),
        /// O fundo e o texto do tema do app.
        Tema => tk!("opcao-esquema-tema"),
        /// O Campbell Powershell.
        CampbellPowershell => tk!("opcao-esquema-campbell-powershell"),
        /// O Vintage.
        Vintage => tk!("opcao-esquema-vintage"),
        /// O One Half Dark.
        OneHalfDark => tk!("opcao-esquema-one-half-dark"),
        /// O One Half Light.
        OneHalfLight => tk!("opcao-esquema-one-half-light"),
        /// O Tango Dark.
        TangoDark => tk!("opcao-esquema-tango-dark"),
        /// O Tango Light.
        TangoLight => tk!("opcao-esquema-tango-light"),
    }
}

escolha! {
    /// A forma do cursor do terminal.
    Cursor {
        /// Barra vertical.
        #[default]
        Barra => tk!("opcao-cursor-barra"),
        /// Bloco.
        Bloco => tk!("opcao-cursor-bloco"),
        /// Sublinhado.
        Sublinhado => tk!("opcao-cursor-sublinhado"),
    }
}

escolha! {
    /// O que o botão direito faz no terminal.
    BotaoDireito {
        /// Abre o menu.
        Menu => tk!("opcao-botao-direito-menu"),
        /// Copia com seleção e cola sem; o Ctrl+clique direito abre o menu.
        #[default]
        CopiarColar => tk!("opcao-botao-direito-copiar-colar"),
    }
}

escolha! {
    /// O que fazer ao colar uma imagem ou arquivos numa sessão SSH (M45).
    ColarArquivos {
        /// Envia para o servidor e cola o caminho de lá.
        #[default]
        Enviar => tk!("opcao-colar-arquivos-enviar"),
        /// Cola o caminho do arquivo no computador.
        CaminhoLocal => tk!("opcao-colar-arquivos-caminho-local"),
        /// Pergunta a cada vez.
        Perguntar => tk!("opcao-colar-arquivos-perguntar"),
    }
}

escolha! {
    /// O que a campainha faz.
    Campainha {
        /// Pisca a tela.
        #[default]
        Piscar => tk!("opcao-campainha-piscar"),
        /// Nada.
        Nenhuma => tk!("opcao-campainha-nenhuma"),
    }
}

escolha! {
    /// O que fazer com uma chave de servidor que o `known_hosts` ainda não tem.
    ChaveNova {
        /// Pergunta (o diálogo do M24).
        #[default]
        Perguntar => tk!("opcao-chave-nova-perguntar"),
        /// Aceita e grava, sem diálogo. A chave **mudada** continua recusada.
        Aceitar => tk!("opcao-chave-nova-aceitar"),
    }
}

escolha! {
    /// O que fazer com a sessão do `tmux` no servidor ao fechar uma aba que roda nela.
    TmuxAoFechar {
        /// Pergunta a cada aba fechada.
        #[default]
        Perguntar => tk!("opcao-tmux-ao-fechar-perguntar"),
        /// Encerra a sessão no servidor.
        Encerrar => tk!("opcao-tmux-ao-fechar-encerrar"),
        /// Deixa a sessão rodando no servidor.
        Manter => tk!("opcao-tmux-ao-fechar-manter"),
    }
}

escolha! {
    /// O que o "Controlar com" faz quando a aba já teve uma conversa com o assistente.
    ConversaAoControlar {
        /// Mostra as conversas da aba e deixa escolher entre continuar uma ou começar outra.
        #[default]
        Perguntar => tk!("opcao-conversa-ao-controlar-perguntar"),
        /// Continua a conversa mais recente, sem diálogo.
        Continuar => tk!("opcao-conversa-ao-controlar-continuar"),
        /// Começa sempre uma conversa nova.
        Nova => tk!("opcao-conversa-ao-controlar-nova"),
    }
}

escolha! {
    /// O X11 das conexões SSH que não escolhem (`x11.padrao`).
    X11Padrao {
        /// Sem X11.
        Desligado => tk!("opcao-x11-padrao-desligado"),
        /// O `ssh -X`: o servidor X só dá o que a extensão SECURITY permite.
        NaoConfiavel => tk!("opcao-x11-padrao-nao-confiavel"),
        /// O `ssh -Y`: o cookie de verdade; o que o virt-manager e a maioria dos programas pedem.
        #[default]
        Confiavel => tk!("opcao-x11-padrao-confiavel"),
    }
}

escolha! {
    /// O que fazer quando a conexão cai.
    ModoReconectar {
        /// Mostra o aviso de sessão encerrada e espera a pessoa.
        #[default]
        Oferecer => tk!("opcao-modo-reconectar-oferecer"),
        /// Nada além do aviso.
        Nada => tk!("opcao-modo-reconectar-nada"),
        /// Reconecta sozinho, algumas vezes.
        Sozinho => tk!("opcao-modo-reconectar-sozinho"),
    }
}

escolha! {
    /// Onde abre uma conexão.
    AbrirEm {
        /// No painel ativo da última janela ativa.
        #[default]
        PainelAtivo => tk!("opcao-abrir-em-painel-ativo"),
        /// Numa janela nova.
        NovaJanela => tk!("opcao-abrir-em-nova-janela"),
    }
}

escolha! {
    /// O tipo das chaves novas.
    TipoChave {
        /// Ed25519.
        #[default]
        Ed25519 => tk!("opcao-tipo-chave-ed25519"),
        /// ECDSA P-256.
        Ecdsa256 => tk!("opcao-tipo-chave-ecdsa256"),
        /// ECDSA P-384.
        Ecdsa384 => tk!("opcao-tipo-chave-ecdsa384"),
        /// ECDSA P-521.
        Ecdsa521 => tk!("opcao-tipo-chave-ecdsa521"),
        /// RSA de 3072 bits.
        Rsa3072 => tk!("opcao-tipo-chave-rsa3072"),
        /// RSA de 4096 bits.
        Rsa4096 => tk!("opcao-tipo-chave-rsa4096"),
    }
}

escolha! {
    /// O que fazer quando o destino de uma cópia já existe.
    ConflitoCfg {
        /// Perguntar.
        #[default]
        Perguntar => tk!("opcao-conflito-cfg-perguntar"),
        /// Trocar o arquivo.
        Sobrescrever => tk!("opcao-conflito-cfg-sobrescrever"),
        /// Continuar de onde o destino parou.
        Retomar => tk!("opcao-conflito-cfg-retomar"),
        /// Deixar como está.
        Pular => tk!("opcao-conflito-cfg-pular"),
        /// Gravar com outro nome.
        Renomear => tk!("opcao-conflito-cfg-renomear"),
    }
}

impl ConflitoCfg {
    /// O que a fila de transferências faz com o destino que já existe.
    pub fn conflito(self) -> tomni_ui::files::Conflict {
        use tomni_ui::files::Conflict;
        match self {
            ConflitoCfg::Perguntar => Conflict::Ask,
            ConflitoCfg::Sobrescrever => Conflict::Overwrite,
            ConflitoCfg::Retomar => Conflict::Resume,
            ConflitoCfg::Pular => Conflict::Skip,
            ConflitoCfg::Renomear => Conflict::Rename,
        }
    }
}

escolha! {
    /// O que fazer com um arquivo editado fora do app.
    EnviarEditado {
        /// Mostra o aviso "foi alterado. Enviar?".
        #[default]
        Perguntar => tk!("opcao-enviar-editado-perguntar"),
        /// Envia sozinho.
        Sempre => tk!("opcao-enviar-editado-sempre"),
    }
}

escolha! {
    /// A validade do link pré-assinado do S3.
    ValidadeLink {
        /// Uma hora.
        #[default]
        UmaHora => tk!("opcao-validade-link-uma-hora"),
        /// Um dia.
        UmDia => tk!("opcao-validade-link-um-dia"),
        /// Sete dias.
        SeteDias => tk!("opcao-validade-link-sete-dias"),
    }
}

impl ValidadeLink {
    /// A validade em segundos.
    pub fn segundos(self) -> u64 {
        match self {
            Self::UmaHora => 3600,
            Self::UmDia => 86_400,
            Self::SeteDias => 7 * 86_400,
        }
    }
}

escolha! {
    /// A abertura do gerador de espaços: onde abre um espaço salvo de uma janela.
    AbrirEspaco {
        /// Numa janela nova.
        #[default]
        NovaJanela => tk!("opcao-abrir-espaco-nova-janela"),
        /// Na janela ativa.
        JanelaAtiva => tk!("opcao-abrir-espaco-janela-ativa"),
    }
}

escolha! {
    /// O que o X da janela principal faz (`geral.ao_fechar`).
    AoFechar {
        /// Abre a pergunta "Fechar o Tirech Omni?".
        #[default]
        Perguntar => tk!("opcao-ao-fechar-perguntar"),
        /// Esconde o app na bandeja, sem perguntar.
        Bandeja => tk!("opcao-ao-fechar-bandeja"),
        /// Fecha o app.
        Fechar => tk!("opcao-ao-fechar-fechar"),
    }
}

escolha! {
    /// O assistente que o Ctrl+Alt+A abre (`ia.preferido`, M46).
    AssistentePreferido {
        /// O Claude Code.
        #[default]
        Claude => tk!("opcao-assistente-claude"),
        /// O Codex.
        Codex => tk!("opcao-assistente-codex"),
        /// O OpenCode.
        OpenCode => tk!("opcao-assistente-opencode"),
    }
}

impl AssistentePreferido {
    /// A ferramenta do catálogo.
    pub fn ferramenta(self) -> crate::ia::Ferramenta {
        match self {
            Self::Claude => crate::ia::Ferramenta::Claude,
            Self::Codex => crate::ia::Ferramenta::Codex,
            Self::OpenCode => crate::ia::Ferramenta::OpenCode,
        }
    }
}

escolha! {
    /// Onde abre um assistente de IA no servidor (`ia.abrir_em`, M46).
    AbrirIa {
        /// Numa aba nova, ao lado da de onde veio o pedido.
        #[default]
        AbaNova => tk!("opcao-abrir-ia-aba-nova"),
        /// Num painel livre da grade (ou num novo).
        PainelLivre => tk!("opcao-abrir-ia-painel-livre"),
        /// Digitando o comando na aba em que a pessoa está.
        NestaAba => tk!("opcao-abrir-ia-nesta-aba"),
    }
}

escolha! {
    /// O que fazer quando um assistente quer abrir uma aba nova de uma conexão pelo MCP
    /// (`ia.mcp_abrir`; a conexão e a pasta podem escolher outra coisa). Reconectar uma aba que
    /// caiu não conta.
    AbrirAbaIa {
        /// Abre, se a permissão da conexão deixa.
        #[default]
        Permitir => tk!("opcao-mcp-abrir-permitir"),
        /// Pergunta à pessoa a cada aba nova, mesmo com a conexão em Livre.
        Perguntar => tk!("opcao-mcp-abrir-perguntar"),
        /// Não abre: o assistente usa só as abas que a pessoa abriu.
        Bloquear => tk!("opcao-mcp-abrir-bloquear"),
    }
}

escolha! {
    /// Por quanto tempo vale o "abrir as abas novas desta sessão sem perguntar" que a pessoa
    /// marca no diálogo de aprovação de uma aba nova (`ia.mcp_abrir_auto`). Desligado esconde a
    /// caixa; "Sempre abrir" vale até a sessão do assistente acabar.
    AbrirAbaAuto {
        Desligado => tk!("opcao-mcp-abrir-auto-desligado"),
        Min15 => tk!("opcao-mcp-abrir-auto-15m"),
        Min30 => tk!("opcao-mcp-abrir-auto-30m"),
        #[default]
        Hora1 => tk!("opcao-mcp-abrir-auto-1h"),
        Horas4 => tk!("opcao-mcp-abrir-auto-4h"),
        Horas8 => tk!("opcao-mcp-abrir-auto-8h"),
        Horas12 => tk!("opcao-mcp-abrir-auto-12h"),
        Horas24 => tk!("opcao-mcp-abrir-auto-24h"),
        Sempre => tk!("opcao-mcp-abrir-auto-sempre"),
    }
}

impl AbrirAbaAuto {
    /// Quanto vale a licença: `None` quando está desligada, `Some(None)` quando vale até a
    /// sessão acabar.
    pub fn duracao(self) -> Option<Option<std::time::Duration>> {
        let minutos = match self {
            Self::Desligado => return None,
            Self::Sempre => return Some(None),
            Self::Min15 => 15,
            Self::Min30 => 30,
            Self::Hora1 => 60,
            Self::Horas4 => 4 * 60,
            Self::Horas8 => 8 * 60,
            Self::Horas12 => 12 * 60,
            Self::Horas24 => 24 * 60,
        };
        Some(Some(std::time::Duration::from_secs(minutos * 60)))
    }
}

escolha! {
    /// Como instalar um assistente de IA no servidor (`ia.metodo_*`, M46).
    MetodoIa {
        /// O que o servidor permite: o instalador oficial, senão o npm.
        #[default]
        Automatico => tk!("opcao-metodo-ia-automatico"),
        /// O instalador oficial.
        Nativo => tk!("opcao-metodo-ia-nativo"),
        /// O npm.
        Npm => tk!("opcao-metodo-ia-npm"),
        /// O Homebrew.
        Homebrew => tk!("opcao-metodo-ia-homebrew"),
    }
}

impl MetodoIa {
    /// O método do catálogo; `None` no automático.
    pub fn metodo(self) -> Option<crate::ia::Metodo> {
        match self {
            Self::Automatico => None,
            Self::Nativo => Some(crate::ia::Metodo::Nativo),
            Self::Npm => Some(crate::ia::Metodo::Npm),
            Self::Homebrew => Some(crate::ia::Metodo::Homebrew),
        }
    }
}

/// Define uma categoria: os campos, o `Default` e o `resto` que sobrevive à gravação.
macro_rules! categoria {
    (
        $(#[$meta:meta])*
        $nome:ident {
            $( $(#[$fmeta:meta])* $campo:ident : $tipo:ty = $padrao:expr ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default)]
        pub struct $nome {
            $( $(#[$fmeta])* pub $campo: $tipo, )*
            /// O que este código não conhece, devolvido como veio.
            #[serde(flatten)]
            pub resto: Map<String, Value>,
        }

        impl Default for $nome {
            fn default() -> Self {
                Self { $( $campo: $padrao, )* resto: Map::new() }
            }
        }
    };
}

categoria! {
    /// Geral (`geral`).
    Geral {
        /// O tema.
        tema: Tema = Tema::Sistema,
        /// O idioma da interface (vale ao reiniciar).
        idioma: Idioma = Idioma::Sistema,
        /// A página ao iniciar.
        pagina_inicial: PaginaInicial = PaginaInicial::Auto,
        /// Reabrir as abas e janelas do `estado.json` ao iniciar.
        restaurar_sessoes: bool = true,
        /// Perguntar antes de fechar com sessões abertas.
        confirmar_fechar: bool = true,
        /// O que fechar a janela principal faz (só vale com bandeja no sistema).
        ao_fechar: AoFechar = AoFechar::Perguntar,
        /// Abrir o app com o sistema (grava a entrada de início, que passa o `--bandeja`).
        iniciar_com_sistema: bool = false,
        /// Aberto pelo início com o sistema, ficar só na bandeja, com a janela escondida.
        /// Desligado, a janela abre.
        iniciar_na_bandeja: bool = false,
        /// Mostrar o app no menu e no dock do Linux, com o ícone próprio (grava o `.desktop`).
        atalho_no_menu: bool = true,
        /// O medidor de CPU na barra de título, que abre a página Desempenho.
        medidor_cpu: bool = true,
        /// O aviso "este sistema não mostra ícones na bandeja" já foi visto.
        aviso_sem_bandeja: bool = false,
        /// Quantos recentes guardar, sem contar os fixados (de 5 a 100).
        recentes_max: u32 = 30,
        /// Os assistentes de IA: o chat e os perfis, e os assistentes no servidor. Desligado,
        /// os menus, as linhas, as seções e os atalhos deles somem; o que está gravado fica.
        assistente_ia: bool = true,
        /// O controle do app por assistentes (MCP), o "Controlar com" e o cluster com IA.
        /// Desligado, somem como os da IA, e o servidor MCP não sobe.
        mcp: bool = true,
    }
}

categoria! {
    /// O shell local (`terminal.shell`).
    ShellCfg {
        /// O programa; vazio é o `$SHELL` (ou o `cmd.exe`).
        programa: String = String::new(),
        /// Os argumentos, separados por espaço.
        argumentos: String = String::new(),
        /// A pasta onde o shell começa; vazia é a pasta pessoal. Aceita `~`.
        pasta: String = String::new(),
    }
}

categoria! {
    /// Os logs das sessões (`terminal.logs`).
    LogsCfg {
        /// A pasta; vazia é `logs` na pasta de dados.
        pasta: String = String::new(),
        /// Gravar todas as sessões.
        todas_as_sessoes: bool = false,
        /// Cifrar os logs novos (M53): o arquivo ganha `.cifrado` e só abre com o cofre.
        cifrar: bool = false,
        /// De quantos em quantos segundos o log cifrado é selado no disco (de 1 a 3600).
        selar_secs: u32 = 2,
        /// Quantos MiB do fim do log a página Logs mostra (de 0 a 1024; 0 mostra tudo).
        visualizar_mib: u32 = 16,
    }
}

categoria! {
    /// O terminal (`terminal`).
    TerminalCfg {
        /// O tamanho da fonte, de 8 a 48.
        tamanho_fonte: u32 = 16,
        /// O esquema de cores.
        esquema: Esquema = Esquema::Campbell,
        /// A forma do cursor.
        cursor: Cursor = Cursor::Barra,
        /// O cursor pisca. Desligado, fica fixo mesmo que o programa peça para piscar.
        cursor_piscando: bool = true,
        /// As linhas de histórico, de 0 a 100 000.
        historico: u32 = 9001,
        /// Copiar ao selecionar (ligado por padrão).
        copiar_ao_selecionar: bool = true,
        /// Avisar ao colar várias linhas.
        avisar_colar: bool = true,
        /// O que fazer ao colar imagem ou arquivos numa sessão SSH (M45).
        colar_arquivos: ColarArquivos = ColarArquivos::Enviar,
        /// A pasta do servidor para o que é colado; `~` é a pasta pessoal.
        pasta_colar: String = "/tmp/tirech-colar".to_owned(),
        /// O Ctrl+V sem texto na área de transferência envia a imagem ou os arquivos.
        ctrl_v_arquivos: bool = true,
        /// Apaga do servidor o que foi colado há mais de tantos dias (de 0 a 365; 0 nunca apaga).
        limpar_colados_dias: u32 = 7,
        /// Uma pasta com pelo menos tantos arquivos vai num pacote `tar` só (de 0 a 100 000; 0
        /// envia sempre pelo SFTP).
        colar_tar_min_arquivos: u32 = 50,
        /// O que o botão direito faz.
        botao_direito: BotaoDireito = BotaoDireito::CopiarColar,
        /// O que a campainha faz.
        campainha: Campainha = Campainha::Piscar,
        /// Fechar o painel quando a última aba dele sair (havendo outro painel).
        fechar_painel_vazio: bool = false,
        /// A aba duplicada abre na pasta em que a original está (M64).
        duplicar_na_pasta: bool = true,
        /// Ctrl+clique abre os links do terminal (M49).
        links_com_ctrl: bool = true,
        /// O shell local.
        shell: ShellCfg = ShellCfg::default(),
        /// Os logs.
        logs: LogsCfg = LogsCfg::default(),
    }
}

categoria! {
    /// Ao cair a conexão (`conexoes.reconectar`).
    ReconectarCfg {
        /// O que fazer.
        modo: ModoReconectar = ModoReconectar::Oferecer,
        /// Quantas vezes tentar sozinho, de 1 a 20.
        tentativas: u32 = 3,
        /// Quantos segundos esperar entre as tentativas, de 1 a 300.
        espera_secs: u32 = 5,
        /// Depois das tentativas, quantos minutos esperar para começar outra rodada, até a
        /// conexão voltar; 0 desliga. De 0 a 1440. Cada conexão ou pasta pode escolher outro.
        retentar_mins: u32 = 5,
    }
}

categoria! {
    /// As conexões (`conexoes`).
    Conexoes {
        /// O usuário das conexões novas; vazio é o do sistema.
        usuario_padrao: String = String::new(),
        /// A porta SSH das conexões novas.
        porta_padrao: u16 = 22,
        /// O `TERM` das conexões novas.
        term_padrao: String = "xterm-256color".to_owned(),
        /// O keepalive das conexões novas, em segundos (0 desliga).
        keepalive_padrao_secs: u32 = 30,
        /// O arquivo `known_hosts`; vazio é `~/.ssh/known_hosts`.
        known_hosts: String = String::new(),
        /// O que fazer com um servidor desconhecido.
        chave_nova: ChaveNova = ChaveNova::Perguntar,
        /// Ao cair a conexão.
        reconectar: ReconectarCfg = ReconectarCfg::default(),
        /// Fechar a aba depois destes minutos sem uso (nada digitado, colado ou mandado por ela);
        /// 0, o padrão, não fecha. De 0 a 1440. Cada conexão ou pasta pode escolher outro.
        ocioso_mins: u32 = 0,
        /// Onde abrir uma conexão.
        abrir_em: AbrirEm = AbrirEm::PainelAtivo,
        /// Deixar marcada a caixa "Importar as senhas no cofre" ao importar de outro programa.
        importar_senhas: bool = true,
        /// Perguntar antes de fechar todas as abas quando há sessões abertas.
        confirmar_fechar_todas: bool = true,
        /// O que fazer com a sessão do `tmux` ao fechar uma aba que roda nela.
        tmux_ao_fechar: TmuxAoFechar = TmuxAoFechar::Perguntar,
        /// Ao escolher uma aba, escolher a conexão dela na árvore (menu Exibir), como o
        /// "sincronizar a árvore" do WinSSHTerm.
        mostrar_aba_na_arvore: bool = false,
        /// Quanto tempo sem saída conta como tela parada nos comandos ao conectar, em
        /// milissegundos, de 50 a 60 000 (M50).
        comandos_quieto_ms: u32 = 600,
        /// O tempo máximo, em segundos, dos passos novos dos comandos ao conectar, de 1 a 86 400
        /// (M50).
        comandos_max_secs: u32 = 15,
    }
}

categoria! {
    /// Os scripts rodados em vários servidores (`scripts`).
    ScriptsCfg {
        /// O programa que lê o arquivo do script, na entrada; a linha digitada não passa por ele.
        interpretador: String = "sh -s".to_owned(),
        /// Quantos servidores rodam ao mesmo tempo, de 1 a 64.
        paralelo: u32 = 8,
        /// A pasta dos scripts; vazia é `scripts` na pasta de dados.
        pasta: String = String::new(),
    }
}

categoria! {
    /// O gerador de senhas (`cofre.gerador`).
    GeradorCfg {
        /// O tamanho, de 8 a 128.
        tamanho: u32 = 20,
        /// Letras maiúsculas.
        maiusculas: bool = true,
        /// Letras minúsculas.
        minusculas: bool = true,
        /// Dígitos.
        digitos: bool = true,
        /// Símbolos.
        simbolos: bool = true,
        /// Tira os caracteres que se confundem.
        sem_ambiguos: bool = false,
    }
}

escolha! {
    /// Como a página Notas abre uma nota.
    ModoNotas {
        /// Só o texto em Markdown.
        Editar => tk!("opcao-modo-notas-editar"),
        /// O texto e a prévia lado a lado.
        #[default]
        Dividir => tk!("opcao-modo-notas-dividir"),
        /// Só a prévia.
        Ler => tk!("opcao-modo-notas-ler"),
    }
}

categoria! {
    /// A página Notas (`notas`).
    NotasCfg {
        /// O modo em que a página abre.
        modo: ModoNotas = ModoNotas::Dividir,
        /// O editor em fonte monoespaçada.
        fonte_mono: bool = true,
        /// Quebrar as linhas longas no editor.
        quebrar_linhas: bool = true,
    }
}

categoria! {
    /// O cofre de senhas (`cofre`). A proteção em si (chave ou senha mestra) vive no
    /// `vault.json`.
    CofreCfg {
        /// Depois de quantos segundos limpar a área de transferência (0: nunca).
        limpar_area_transferencia_secs: u32 = 0,
        /// Depois de quantos minutos sem uso bloquear (0: nunca).
        bloquear_apos_min: u32 = 0,
        /// A pasta do `vault.json` e do `vault.key`; vazia é a pasta de dados. Aponte para uma
        /// pasta sincronizada para dividir o cofre entre máquinas.
        local: String = String::new(),
        /// O gerador.
        gerador: GeradorCfg = GeradorCfg::default(),
    }
}

categoria! {
    /// As chaves SSH e o agente (`agente`); absorve o `agente.json` do M29.
    AgenteCfg {
        /// Se o app sobe o agente embutido.
        ligado: bool = true,
        /// O endereço do socket; vazio é o padrão do app.
        socket: String = String::new(),
        /// Pôr o `SSH_AUTH_SOCK` nos shells locais.
        exportar_no_shell: bool = true,
        /// Carregar as chaves da lista ao iniciar.
        carregar_ao_iniciar: bool = true,
        /// A validade das chaves acrescentadas sem uma própria (0: nunca vencem).
        vida_padrao_secs: u64 = 0,
        /// Pedir confirmação a cada uso das chaves acrescentadas.
        confirmar_padrao: bool = false,
        /// Encaminhar o agente nas conexões novas.
        encaminhar_padrao: bool = false,
        /// A pasta das chaves; vazia é `~/.ssh`.
        pasta_chaves: String = String::new(),
        /// O tipo das chaves novas.
        tipo_padrao: TipoChave = TipoChave::Ed25519,
        /// As rodadas do `bcrypt-pbkdf` das chaves novas (16, 64 ou 100).
        rodadas_padrao: u32 = 16,
        /// Escrever o agente no `~/.ssh/config` (o `ssh` do sistema passa a usá-lo).
        sistema_ssh_config: bool = false,
        /// Também pôr o `SSH_AUTH_SOCK` no `.bashrc`, `.zshrc` e no fish.
        sistema_shells: bool = false,
        /// Também pôr o `SSH_AUTH_SOCK` na sessão gráfica (`systemctl --user`/`launchctl`).
        sistema_sessao: bool = false,
        /// Windows: fazer o Git usar o OpenSSH do Windows (`core.sshCommand`).
        sistema_git: bool = false,
        /// Ao bloquear o cofre, tirar do agente as chaves que vieram dele.
        tirar_ao_bloquear: bool = true,
        /// Windows: atender também como o Pageant (PuTTY, WinSCP, FileZilla).
        pageant: bool = true,
        /// As chaves carregadas ao iniciar (editadas em Ferramentas, Agente).
        chaves: Vec<ChaveSalva> = Vec::new(),
    }
}

impl AgenteCfg {
    /// Põe (ou troca) a chave na lista de quem carrega ao iniciar.
    pub fn guardar(&mut self, chave: ChaveSalva) {
        match self.chaves.iter_mut().find(|c| c.arquivo == chave.arquivo) {
            Some(existente) => *existente = chave,
            None => self.chaves.push(chave),
        }
    }

    /// Tira a chave da lista. Devolve se estava.
    pub fn esquecer(&mut self, arquivo: &std::path::Path) -> bool {
        let antes = self.chaves.len();
        self.chaves.retain(|c| c.arquivo != arquivo);
        self.chaves.len() != antes
    }
}

escolha! {
    /// O critério da comparação das duas pastas da página Arquivos.
    ModoComparar {
        /// Marca os arquivos de tamanho diferente.
        #[default]
        Tamanho => tk!("opcao-modo-comparar-tamanho"),
        /// Marca o arquivo mais novo.
        Data => tk!("opcao-modo-comparar-data"),
    }
}

categoria! {
    /// Os arquivos (`arquivos`); absorve o módulo `padroes` do M30.
    Arquivos {
        /// Transferências ao mesmo tempo, de 1 a 8.
        paralelas: u32 = 2,
        /// O que fazer quando o destino existe.
        conflito: ConflitoCfg = ConflitoCfg::Perguntar,
        /// Mostrar os arquivos ocultos nos painéis novos.
        mostrar_ocultos: bool = false,
        /// O editor externo; vazio é o programa do sistema.
        editor: String = String::new(),
        /// O que fazer com o arquivo editado.
        enviar_editado: EnviarEditado = EnviarEditado::Perguntar,
        /// Manter a data de modificação.
        preservar_data: bool = true,
        /// Manter as permissões.
        preservar_permissoes: bool = false,
        /// A pasta local inicial; vazia é a pasta pessoal.
        pasta_local: String = String::new(),
        /// Perguntar antes de apagar.
        confirmar_apagar: bool = true,
        /// O keepalive do navegador de arquivos, em segundos (0 desliga).
        keepalive_secs: u32 = 60,
        /// Ligar a comparação das pastas ao abrir uma aba.
        comparar: bool = false,
        /// O critério da comparação.
        comparar_modo: ModoComparar = ModoComparar::Tamanho,
        /// A diferença de data, em minutos, que ainda conta como a mesma data.
        comparar_tolerancia_min: u32 = 1,
        /// Esconder os arquivos idênticos enquanto compara.
        ocultar_identicos: bool = false,
        /// Ligar a navegação sincronizada ao abrir uma aba.
        sincronizar: bool = false,
    }
}

categoria! {
    /// O que uma conexão FTP nova recebe (`ftp`).
    FtpCfg {
        /// Como a conexão é protegida.
        seguranca: FtpSeguranca = FtpSeguranca::Explicita,
        /// Quem abre a conexão de dados.
        modo: FtpModo = FtpModo::Passivo,
        /// A versão do TLS.
        tls: FtpTls = FtpTls::Auto,
        /// A codificação dos nomes.
        codificacao: FtpCodificacao = FtpCodificacao::Auto,
    }
}

categoria! {
    /// O que uma conexão S3 nova recebe (`s3`).
    S3Cfg {
        /// A região do provedor "outro".
        regiao: String = "us-east-1".to_owned(),
        /// O estilo de endereço do provedor "outro".
        estilo: S3Estilo = S3Estilo::Caminho,
        /// O tamanho da parte do envio em partes, em MiB (de 5 a 512).
        parte_mib: u32 = 8,
        /// A classe de armazenamento dos envios; vazia é a do bucket.
        classe: String = String::new(),
        /// A validade do link pré-assinado.
        validade_link: ValidadeLink = ValidadeLink::UmaHora,
    }
}

categoria! {
    /// O rclone (`rclone`); absorve o `rclone.json` do M33.
    RcloneCfg {
        /// O binário; vazio procura no `PATH`.
        binario: String = String::new(),
        /// O arquivo de configuração; vazio é o padrão do rclone.
        arquivo_config: String = String::new(),
        /// O id do item do cofre com a senha da configuração cifrada. A senha não entra aqui.
        credencial: Option<u64> = None,
    }
}

categoria! {
    /// As redes (`redes`).
    RedesCfg {
        /// A espera do proxy nos perfis novos, em segundos (de 1 a 600).
        espera_padrao_secs: u32 = 30,
        /// Rodar o `descer` dos perfis em uso ao fechar o app.
        descer_ao_fechar: bool = true,
        /// O navegador dos links com perfil de rede (`crate::navegador`); vazio procura um
        /// Chrome, Edge, Brave, Chromium ou Firefox.
        navegador: String = String::new(),
    }
}

categoria! {
    /// O X11 (`x11`).
    X11Cfg {
        /// O modo das conexões SSH com "Padrão".
        padrao: X11Padrao = X11Padrao::Confiavel,
        /// O servidor X daqui (o `DISPLAY`); vazio usa a variável `DISPLAY` do ambiente.
        display: String = String::new(),
        /// O comando do virt-manager remoto.
        virt_manager: String = "virt-manager --no-fork".to_owned(),
    }
}

categoria! {
    /// Os túneis (`tuneis`).
    TuneisCfg {
        /// Escutar em todas as interfaces nos túneis novos de `-L` e `-D`, não só em
        /// `127.0.0.1`.
        outras_maquinas: bool = false,
    }
}

categoria! {
    /// As portas do servidor (`portas`, M58).
    PortasCfg {
        /// Segundos entre as atualizações do painel; 0, só no botão.
        intervalo_secs: u32 = 5,
        /// Manda a sonda às portas novas (o título, os apps conhecidos).
        sondar: bool = true,
        /// O tempo total da sonda de uma porta, em milissegundos.
        sonda_ms: u32 = 1500,
        /// Usa `ss`, `netstat`, `docker` e `podman` do servidor quando existem.
        usar_ferramentas: bool = true,
        /// Avisa quando uma porta nova aparece numa sessão SSH.
        avisar_novas: bool = false,
        /// Abre os links locais de uma aba SSH pelo túnel.
        links_pelo_tunel: bool = true,
        /// Mostra as portas do sistema (SSH, DNS local…).
        mostrar_sistema: bool = false,
    }
}

categoria! {
    /// As janelas e os espaços (`janelas`).
    JanelasCfg {
        /// A pasta dos espaços de trabalho; vazia é `espacos` na pasta de dados.
        pasta_espacos: String = String::new(),
        /// Onde abre um espaço salvo de uma janela.
        abrir_espaco: AbrirEspaco = AbrirEspaco::NovaJanela,
        /// Soltar uma aba fora da janela abre outra.
        tear_off: bool = true,
    }
}

categoria! {
    /// O que a pessoa pediu no diálogo "Instalar no computador" (`instalacao`, M65). Não é um
    /// ajuste da página: só deixa o diálogo lembrar o que foi pedido. A fonte da verdade do
    /// estado é o disco.
    InstalacaoCfg {
        /// Pediu o atalho na área de trabalho.
        area_de_trabalho: bool = false,
        /// Pediu fixar na barra de tarefas (no Windows, o passo final é da pessoa).
        barra_de_tarefas: bool = false,
    }
}

categoria! {
    /// Os assistentes de IA (`ia`, M52). Os provedores ficam no `ia.json`, não aqui.
    IaCfg {
        /// Tirar senhas e chaves do que vai para a IA.
        redigir: bool = true,
        /// Outros textos a tirar, um por item; `re:` no começo marca uma expressão regular.
        redigir_extras: Vec<String> = Vec::new(),
        /// Mostrar os itens da IA no menu do terminal.
        mostrar_no_menu: bool = true,
        /// Mostrar o que vai sair antes do primeiro envio de cada aba.
        mostrar_o_que_sai: bool = true,
        /// Quantas linhas do terminal entram no contexto (de 1 a 100 000).
        linhas_contexto: u32 = 200,
        /// O tamanho máximo do contexto, em caracteres (0 não limita).
        max_caracteres: u32 = 200_000,
        /// O shell que se supõe quando não dá para saber qual é.
        comando_shell: String = "bash".to_owned(),
        /// Quantos segundos esperar a resposta (0 espera sem limite).
        espera_resposta: u32 = 0,
        /// Deixar assistentes (Claude Code, Codex) controlarem o app pelo MCP (M51).
        mcp_ligado: bool = false,
        /// Se um assistente pode abrir abas novas pelo MCP, quando a conexão e as pastas dela não
        /// escolhem.
        mcp_abrir: AbrirAbaIa = AbrirAbaIa::Permitir,
        /// Por quanto tempo o diálogo de uma aba nova pode liberar as próximas da mesma sessão.
        mcp_abrir_auto: AbrirAbaAuto = AbrirAbaAuto::Hora1,
        /// Comandos que sempre pedem aprovação pelo MCP, além da lista de bloqueio: o nome de um
        /// programa ou, com `re:` no começo, uma expressão regular (M51).
        bloqueio_extras: Vec<String> = Vec::new(),
        /// Gravar o registro de auditoria do MCP, um arquivo por dia (M51).
        auditoria: bool = true,
        /// Por quantos segundos a aba fica marcada depois da última ação de um assistente (M51).
        marca_secs: u32 = 30,
        /// O tamanho máximo de uma resposta do MCP, em caracteres (0 não limita; M51).
        caracteres_por_resposta: u32 = 100_000,
        /// Rodar na aba, à vista, o `executar` que o assistente pede "separado", quando a aba está
        /// livre no prompt (M51, depois da entrega).
        comandos_na_aba: bool = true,
        /// Finais de prompt a mais, além de `#`, `$`, `%` e `>`, para o `executar` na aba (M51).
        finais_de_prompt: Vec<String> = Vec::new(),
        /// A pasta para trocar arquivos com o assistente; vazia é `Downloads/Tirech Omni` (M51).
        pasta_troca: String = String::new(),
        /// Quantas linhas o `ler` do espaço de trabalho devolve por padrão (de 1 a 100 000; M51).
        linhas_por_leitura: u32 = 2000,
        /// O aviso de "assistentes poderão usar as conexões que você liberar" já foi visto (M51).
        mcp_avisado: bool = false,
        /// As pastas de servidor que os assistentes usam como espaço de trabalho (M51).
        espacos: Vec<crate::mcp::espaco::EspacoIa> = Vec::new(),
        /// O assistente que o Ctrl+Alt+A abre numa aba SSH (M46).
        preferido: AssistentePreferido = AssistentePreferido::Claude,
        /// Onde abre um assistente no servidor (M46).
        abrir_em: AbrirIa = AbrirIa::AbaNova,
        /// Ficar num shell quando o assistente termina, em vez de fechar a aba (M46).
        shell_ao_sair: bool = true,
        /// Abrir a porta 1455 até o servidor no login do Codex (M46).
        tunel_codex: bool = true,
        /// Verificar os assistentes quando uma aba SSH fica ativa (M46).
        verificar_ao_abrir: bool = true,
        /// O botão dos assistentes na barra (M46).
        mostrar_botao: bool = true,
        /// Mostrar o Claude Code nos menus (M46).
        mostrar_claude: bool = true,
        /// Mostrar o Codex nos menus (M46).
        mostrar_codex: bool = true,
        /// Mostrar o OpenCode nos menus (M46).
        mostrar_opencode: bool = true,
        /// Como instalar o Claude Code (M46).
        metodo_claude: MetodoIa = MetodoIa::Automatico,
        /// Como instalar o Codex (M46).
        metodo_codex: MetodoIa = MetodoIa::Automatico,
        /// Como instalar o OpenCode (M46).
        metodo_opencode: MetodoIa = MetodoIa::Automatico,
        /// Os argumentos do `claude`, numa linha (M46).
        argumentos_claude: String = String::new(),
        /// Os argumentos do `codex`, numa linha (M46).
        argumentos_codex: String = String::new(),
        /// Os argumentos do `opencode`, numa linha (M46).
        argumentos_opencode: String = String::new(),
        /// A pasta onde o "Controlar com" abre os assistentes deste computador; vazia é a do app,
        /// na pasta de cache do sistema (veja `Ambiente::pasta_dos_assistentes_padrao`).
        local_pasta: String = String::new(),
        /// O caminho do `claude` deste computador; vazio procura no `PATH` (M63).
        local_claude_caminho: String = String::new(),
        /// Os argumentos do `claude` deste computador, numa linha (M63).
        local_claude_argumentos: String = String::new(),
        /// O caminho do `codex` deste computador; vazio procura no `PATH` (M63).
        local_codex_caminho: String = String::new(),
        /// Os argumentos do `codex` deste computador, numa linha (M63).
        local_codex_argumentos: String = String::new(),
        /// O caminho do `opencode` deste computador; vazio procura no `PATH` (M63).
        local_opencode_caminho: String = String::new(),
        /// Os argumentos do `opencode` deste computador, numa linha (M63).
        local_opencode_argumentos: String = String::new(),
        /// Guarda o id das conversas dos assistentes deste computador com cada aba, no
        /// `conversas.json`; desligado não guarda, não pergunta e não reabre.
        conversas_lembrar: bool = true,
        /// Ao abrir o app ou um espaço, reabre os assistentes que estavam nas abas, continuando
        /// a mesma conversa.
        conversas_reabrir: bool = true,
        /// O que o "Controlar com" faz quando a aba já teve conversa com o assistente.
        conversas_ao_controlar: ConversaAoControlar = ConversaAoControlar::Perguntar,
    }
}

impl IaCfg {
    /// Copia de `outra` só os ajustes dos assistentes no servidor (M46), que dividem a
    /// categoria `ia` do arquivo com a IA do terminal (M52) e o controle por assistentes (M51).
    fn copiar_servidor_de(&mut self, outra: &IaCfg) {
        self.preferido = outra.preferido;
        self.abrir_em = outra.abrir_em;
        self.shell_ao_sair = outra.shell_ao_sair;
        self.tunel_codex = outra.tunel_codex;
        self.verificar_ao_abrir = outra.verificar_ao_abrir;
        self.mostrar_botao = outra.mostrar_botao;
        self.mostrar_claude = outra.mostrar_claude;
        self.mostrar_codex = outra.mostrar_codex;
        self.mostrar_opencode = outra.mostrar_opencode;
        self.metodo_claude = outra.metodo_claude;
        self.metodo_codex = outra.metodo_codex;
        self.metodo_opencode = outra.metodo_opencode;
        self.argumentos_claude = outra.argumentos_claude.clone();
        self.argumentos_codex = outra.argumentos_codex.clone();
        self.argumentos_opencode = outra.argumentos_opencode.clone();
    }

    /// Copia de `outra` só os ajustes dos assistentes deste computador (M63).
    fn copiar_local_de(&mut self, outra: &IaCfg) {
        self.local_pasta = outra.local_pasta.clone();
        self.local_claude_caminho = outra.local_claude_caminho.clone();
        self.local_claude_argumentos = outra.local_claude_argumentos.clone();
        self.local_codex_caminho = outra.local_codex_caminho.clone();
        self.local_codex_argumentos = outra.local_codex_argumentos.clone();
        self.local_opencode_caminho = outra.local_opencode_caminho.clone();
        self.local_opencode_argumentos = outra.local_opencode_argumentos.clone();
        self.conversas_lembrar = outra.conversas_lembrar;
        self.conversas_reabrir = outra.conversas_reabrir;
        self.conversas_ao_controlar = outra.conversas_ao_controlar;
    }

    /// O caminho do executável da ferramenta neste computador, pelas Configurações (M63).
    pub fn caminho_local_de(&self, ferramenta: crate::ia::Ferramenta) -> &str {
        use crate::ia::Ferramenta;
        match ferramenta {
            Ferramenta::Claude => &self.local_claude_caminho,
            Ferramenta::Codex => &self.local_codex_caminho,
            Ferramenta::OpenCode => &self.local_opencode_caminho,
        }
    }

    /// Os argumentos extras da ferramenta neste computador, pelas Configurações (M63).
    pub fn argumentos_locais_de(&self, ferramenta: crate::ia::Ferramenta) -> &str {
        use crate::ia::Ferramenta;
        match ferramenta {
            Ferramenta::Claude => &self.local_claude_argumentos,
            Ferramenta::Codex => &self.local_codex_argumentos,
            Ferramenta::OpenCode => &self.local_opencode_argumentos,
        }
    }

    /// Se a ferramenta aparece nos menus.
    pub fn mostra(&self, ferramenta: crate::ia::Ferramenta) -> bool {
        use crate::ia::Ferramenta;
        match ferramenta {
            Ferramenta::Claude => self.mostrar_claude,
            Ferramenta::Codex => self.mostrar_codex,
            Ferramenta::OpenCode => self.mostrar_opencode,
        }
    }

    /// Como instalar a ferramenta, pelas Configurações.
    pub fn metodo_de(&self, ferramenta: crate::ia::Ferramenta) -> MetodoIa {
        use crate::ia::Ferramenta;
        match ferramenta {
            Ferramenta::Claude => self.metodo_claude,
            Ferramenta::Codex => self.metodo_codex,
            Ferramenta::OpenCode => self.metodo_opencode,
        }
    }

    /// Os argumentos da ferramenta, pelas Configurações.
    pub fn argumentos_de(&self, ferramenta: crate::ia::Ferramenta) -> &str {
        use crate::ia::Ferramenta;
        match ferramenta {
            Ferramenta::Claude => &self.argumentos_claude,
            Ferramenta::Codex => &self.argumentos_codex,
            Ferramenta::OpenCode => &self.argumentos_opencode,
        }
    }
}

/// O conteúdo do `configuracoes.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Configuracoes {
    /// A versão do formato (2; o arquivo da casca não tem e é a 1).
    pub versao: u32,
    /// Geral.
    pub geral: Geral,
    /// O terminal.
    pub terminal: TerminalCfg,
    /// As conexões.
    pub conexoes: Conexoes,
    /// Os scripts.
    pub scripts: ScriptsCfg,
    /// O cofre.
    pub cofre: CofreCfg,
    /// As notas.
    pub notas: NotasCfg,
    /// As chaves e o agente.
    pub agente: AgenteCfg,
    /// Os arquivos.
    pub arquivos: Arquivos,
    /// O FTP.
    pub ftp: FtpCfg,
    /// O S3.
    pub s3: S3Cfg,
    /// O rclone.
    pub rclone: RcloneCfg,
    /// As redes.
    pub redes: RedesCfg,
    /// O X11.
    pub x11: X11Cfg,
    /// Os túneis.
    pub tuneis: TuneisCfg,
    /// As portas do servidor.
    pub portas: PortasCfg,
    /// As janelas e os espaços.
    pub janelas: JanelasCfg,
    /// Os assistentes de IA.
    pub ia: IaCfg,
    /// O que se pediu em "Instalar no computador".
    pub instalacao: InstalacaoCfg,
    /// Os atalhos mudados: o id da ação e as teclas (`["Ctrl+Shift+T"]`). Só os que diferem do
    /// padrão do catálogo.
    pub atalhos: BTreeMap<String, Vec<String>>,
    /// O que este código não conhece, devolvido como veio.
    #[serde(flatten)]
    pub resto: Map<String, Value>,
}

impl Default for Configuracoes {
    fn default() -> Self {
        Self {
            versao: VERSAO,
            geral: Geral::default(),
            terminal: TerminalCfg::default(),
            conexoes: Conexoes::default(),
            scripts: ScriptsCfg::default(),
            cofre: CofreCfg::default(),
            notas: NotasCfg::default(),
            agente: AgenteCfg::default(),
            arquivos: Arquivos::default(),
            ftp: FtpCfg::default(),
            s3: S3Cfg::default(),
            rclone: RcloneCfg::default(),
            redes: RedesCfg::default(),
            x11: X11Cfg::default(),
            tuneis: TuneisCfg::default(),
            portas: PortasCfg::default(),
            janelas: JanelasCfg::default(),
            ia: IaCfg::default(),
            instalacao: InstalacaoCfg::default(),
            atalhos: BTreeMap::new(),
            resto: Map::new(),
        }
    }
}

/// As categorias do arquivo, na ordem em que a página mostra (a chave no JSON).
pub const CATEGORIAS: [&str; 17] = [
    "geral", "terminal", "conexoes", "scripts", "cofre", "notas", "agente", "arquivos", "ftp",
    "s3", "rclone", "redes", "x11", "tuneis", "portas", "janelas", "ia",
];

impl Configuracoes {
    /// Se o servidor MCP fica no ar: ligado na seção dele e com o controle por assistentes
    /// ligado em Geral.
    pub fn mcp_no_ar(&self) -> bool {
        self.geral.mcp && self.ia.mcp_ligado
    }

    /// A pasta do cofre: `cofre.local`, ou a pasta de dados quando está vazio. Um `~` no começo
    /// é a pasta pessoal.
    pub fn pasta_do_cofre(&self, dados: &Path) -> PathBuf {
        match self.cofre.local.trim() {
            "" => dados.to_owned(),
            local => crate::armazenamento::expandir_til(local),
        }
    }

    /// Lê o `configuracoes.json` e devolve também os avisos (o que voltou ao padrão e por quê).
    ///
    /// - Sem arquivo: os padrões, sem gravar (o arquivo nasce na primeira mudança).
    /// - Ilegível (não é JSON, ou não é um objeto): vai para `configuracoes.json.bak`.
    /// - Legível: mescla sobre os padrões campo a campo; um campo com o tipo errado volta ao
    ///   padrão e os outros ficam. Depois prende os números nas faixas e roda as migrações
    ///   (o arquivo da casca, o `agente.json` e o `rclone.json`), gravando só se mudou algo.
    pub fn carregar(armazenamento: &Armazenamento) -> (Self, Vec<String>) {
        let arquivo = armazenamento.arquivo_configuracoes();
        let mut avisos = Vec::new();
        let texto = match fs::read_to_string(&arquivo) {
            Ok(texto) => Some(texto),
            Err(erro) if erro.kind() == std::io::ErrorKind::NotFound => None,
            Err(erro) => {
                avisos.push(t!(
                    "comum-nao-consegui-ler",
                    arquivo = arquivo.display().to_string(),
                    erro = erro.to_string()
                ));
                return (Self::default(), avisos);
            }
        };
        crate::instancias::anotar_base(&arquivo, texto.as_deref().map(str::as_bytes));
        let mut migrou = false;
        let mut raiz = match texto {
            None => Map::new(),
            Some(texto) => match serde_json::from_str::<Value>(&texto) {
                Ok(Value::Object(mapa)) => mapa,
                _ => {
                    avisos.push(guardar_copia(&arquivo));
                    Map::new()
                }
            },
        };
        migrou |= migrar_da_casca(&mut raiz);
        migrou |= migrar_arquivo(
            armazenamento,
            &mut raiz,
            "agente",
            &armazenamento.arquivo_agente(),
        );
        migrou |= migrar_rclone(armazenamento, &mut raiz);

        let (mut config, mut avisos_do_mescla) = mesclar(raiz);
        avisos.append(&mut avisos_do_mescla);
        avisos.extend(config.validar());
        if migrou && let Err(erro) = config.salvar(armazenamento) {
            avisos.push(t!(
                "comum-nao-consegui-gravar",
                arquivo = arquivo.display().to_string(),
                erro = erro.to_string()
            ));
        }
        (config, avisos)
    }

    /// Grava o `configuracoes.json` (atômico).
    pub fn salvar(&self, armazenamento: &Armazenamento) -> std::io::Result<()> {
        armazenamento.gravar(&armazenamento.arquivo_configuracoes(), self)
    }

    /// O limite de recentes, dentro da faixa permitida.
    pub fn limite_de_recentes(&self) -> usize {
        self.geral.recentes_max.clamp(RECENTES_MIN, RECENTES_MAX) as usize
    }

    /// Prende cada número na sua faixa e tira o atalho que o `FromStr` não lê. Devolve um aviso
    /// por ajuste.
    pub fn validar(&mut self) -> Vec<String> {
        let mut avisos = Vec::new();
        let mut prender = |nome: &str, valor: &mut u32, min: u32, max: u32| {
            let preso = (*valor).clamp(min, max);
            if preso != *valor {
                avisos.push(t!(
                    "config-fora-da-faixa",
                    nome = nome,
                    min = min.to_string(),
                    max = max.to_string(),
                    preso = preso.to_string()
                ));
                *valor = preso;
            }
        };
        prender(
            "geral.recentes_max",
            &mut self.geral.recentes_max,
            RECENTES_MIN,
            RECENTES_MAX,
        );
        prender(
            "terminal.tamanho_fonte",
            &mut self.terminal.tamanho_fonte,
            8,
            48,
        );
        prender(
            "terminal.historico",
            &mut self.terminal.historico,
            0,
            100_000,
        );
        let mut porta = u32::from(self.conexoes.porta_padrao);
        prender("conexoes.porta_padrao", &mut porta, 1, 65_535);
        self.conexoes.porta_padrao = porta as u16;
        prender(
            "conexoes.keepalive_padrao_secs",
            &mut self.conexoes.keepalive_padrao_secs,
            0,
            3600,
        );
        prender(
            "conexoes.reconectar.tentativas",
            &mut self.conexoes.reconectar.tentativas,
            1,
            20,
        );
        prender(
            "conexoes.reconectar.espera_secs",
            &mut self.conexoes.reconectar.espera_secs,
            1,
            300,
        );
        prender(
            "conexoes.reconectar.retentar_mins",
            &mut self.conexoes.reconectar.retentar_mins,
            0,
            1440,
        );
        prender(
            "conexoes.ocioso_mins",
            &mut self.conexoes.ocioso_mins,
            0,
            1440,
        );
        prender(
            "conexoes.comandos_quieto_ms",
            &mut self.conexoes.comandos_quieto_ms,
            50,
            60_000,
        );
        prender(
            "conexoes.comandos_max_secs",
            &mut self.conexoes.comandos_max_secs,
            1,
            86_400,
        );
        prender("scripts.paralelo", &mut self.scripts.paralelo, 1, 64);
        prender(
            "cofre.limpar_area_transferencia_secs",
            &mut self.cofre.limpar_area_transferencia_secs,
            0,
            3600,
        );
        prender(
            "cofre.bloquear_apos_min",
            &mut self.cofre.bloquear_apos_min,
            0,
            1440,
        );
        prender(
            "cofre.gerador.tamanho",
            &mut self.cofre.gerador.tamanho,
            8,
            128,
        );
        prender(
            "agente.rodadas_padrao",
            &mut self.agente.rodadas_padrao,
            16,
            1000,
        );
        prender("arquivos.paralelas", &mut self.arquivos.paralelas, 1, 8);
        prender(
            "arquivos.keepalive_secs",
            &mut self.arquivos.keepalive_secs,
            0,
            3600,
        );
        prender(
            "arquivos.comparar_tolerancia_min",
            &mut self.arquivos.comparar_tolerancia_min,
            0,
            1440,
        );
        prender("s3.parte_mib", &mut self.s3.parte_mib, 5, 512);
        prender(
            "redes.espera_padrao_secs",
            &mut self.redes.espera_padrao_secs,
            1,
            600,
        );
        prender(
            "portas.intervalo_secs",
            &mut self.portas.intervalo_secs,
            0,
            u32::MAX,
        );
        prender("portas.sonda_ms", &mut self.portas.sonda_ms, 100, u32::MAX);
        prender(
            "ia.linhas_contexto",
            &mut self.ia.linhas_contexto,
            1,
            100_000,
        );
        prender(
            "ia.max_caracteres",
            &mut self.ia.max_caracteres,
            0,
            100_000_000,
        );
        prender("ia.espera_resposta", &mut self.ia.espera_resposta, 0, 3600);
        prender("ia.marca_secs", &mut self.ia.marca_secs, 1, 3600);
        prender(
            "ia.linhas_por_leitura",
            &mut self.ia.linhas_por_leitura,
            1,
            100_000,
        );
        prender(
            "ia.caracteres_por_resposta",
            &mut self.ia.caracteres_por_resposta,
            0,
            100_000_000,
        );
        self.atalhos.retain(|id, teclas| {
            let ruim = teclas
                .iter()
                .find(|t| t.parse::<tomni_ui::prelude::Shortcut>().is_err());
            if let Some(ruim) = ruim {
                avisos.push(t!(
                    "config-atalho-invalido",
                    id = id.to_string(),
                    ruim = ruim.to_string()
                ));
            }
            ruim.is_none()
        });
        avisos
    }

    /// O arquivo como valor JSON (para mesclar, exportar e comparar).
    pub fn para_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Volta uma categoria (`"terminal"`) ao padrão. Devolve se a chave é uma categoria.
    pub fn restaurar_categoria(&mut self, categoria: &str) -> bool {
        let padrao = Self::default();
        match categoria {
            "geral" => self.geral = padrao.geral,
            "terminal" => self.terminal = padrao.terminal,
            "conexoes" => self.conexoes = padrao.conexoes,
            "scripts" => self.scripts = padrao.scripts,
            "cofre" => self.cofre = padrao.cofre,
            "notas" => self.notas = padrao.notas,
            "agente" => {
                // As chaves da lista não têm card: ficam.
                let chaves = std::mem::take(&mut self.agente.chaves);
                self.agente = padrao.agente;
                self.agente.chaves = chaves;
            }
            "arquivos" => self.arquivos = padrao.arquivos,
            "ftp" => self.ftp = padrao.ftp,
            "s3" => self.s3 = padrao.s3,
            "rclone" => self.rclone = padrao.rclone,
            "redes" => self.redes = padrao.redes,
            "x11" => self.x11 = padrao.x11,
            "tuneis" => self.tuneis = padrao.tuneis,
            "portas" => self.portas = padrao.portas,
            "janelas" => self.janelas = padrao.janelas,
            "ia" => {
                // O controle por assistentes (`mcp`) é outra seção da página: fica como está.
                let ia = std::mem::take(&mut self.ia);
                self.ia = IaCfg {
                    mcp_ligado: ia.mcp_ligado,
                    mcp_abrir: ia.mcp_abrir,
                    mcp_abrir_auto: ia.mcp_abrir_auto,
                    bloqueio_extras: ia.bloqueio_extras.clone(),
                    auditoria: ia.auditoria,
                    marca_secs: ia.marca_secs,
                    caracteres_por_resposta: ia.caracteres_por_resposta,
                    comandos_na_aba: ia.comandos_na_aba,
                    finais_de_prompt: ia.finais_de_prompt.clone(),
                    pasta_troca: ia.pasta_troca.clone(),
                    linhas_por_leitura: ia.linhas_por_leitura,
                    mcp_avisado: ia.mcp_avisado,
                    espacos: ia.espacos.clone(),
                    ..padrao.ia
                };
                // Os assistentes no servidor (M46) também são outra seção.
                self.ia.copiar_servidor_de(&ia);
                self.ia.copiar_local_de(&ia);
            }
            "ia_servidor" => self.ia.copiar_servidor_de(&padrao.ia),
            "ia_local" => self.ia.copiar_local_de(&padrao.ia),
            "mcp" => {
                // Só os ajustes do controle por assistentes: os espaços de trabalho e o aviso
                // visto não são ajustes, e os da IA (M52) são de outra seção.
                let ia = std::mem::take(&mut self.ia);
                let p = padrao.ia;
                self.ia = IaCfg {
                    mcp_ligado: p.mcp_ligado,
                    mcp_abrir: p.mcp_abrir,
                    mcp_abrir_auto: p.mcp_abrir_auto,
                    bloqueio_extras: p.bloqueio_extras,
                    auditoria: p.auditoria,
                    marca_secs: p.marca_secs,
                    caracteres_por_resposta: p.caracteres_por_resposta,
                    comandos_na_aba: p.comandos_na_aba,
                    finais_de_prompt: p.finais_de_prompt,
                    pasta_troca: p.pasta_troca,
                    linhas_por_leitura: p.linhas_por_leitura,
                    ..ia
                };
            }
            "atalhos" => self.atalhos.clear(),
            _ => return false,
        }
        true
    }

    /// O JSON da exportação (`tirech-omni-configuracoes.json`): tudo, menos o que liga a um
    /// segredo ou à máquina (`rclone.credencial`, `agente.chaves`, `agente.socket`,
    /// `ia.espacos`, `ia.mcp_avisado` e os caminhos dos assistentes deste computador).
    pub fn exportar(&self) -> String {
        let mut valor = self.para_json();
        if let Some(rclone) = valor.get_mut("rclone").and_then(Value::as_object_mut) {
            rclone.remove("credencial");
        }
        if let Some(agente) = valor.get_mut("agente").and_then(Value::as_object_mut) {
            agente.remove("chaves");
            agente.remove("socket");
        }
        // O que se instalou é deste computador.
        if let Some(raiz) = valor.as_object_mut() {
            raiz.remove("instalacao");
        }
        if let Some(ia) = valor.get_mut("ia").and_then(Value::as_object_mut) {
            ia.remove("espacos");
            ia.remove("mcp_avisado");
            for chave in CAMINHOS_LOCAIS_DA_IA {
                ia.remove(chave);
            }
        }
        let mut texto = serde_json::to_string_pretty(&valor).unwrap_or_default();
        texto.push('\n');
        texto
    }

    /// Lê um arquivo exportado e devolve as configurações novas, mais os avisos do que voltou
    /// ao padrão. O que a exportação não leva (`rclone.credencial`, `agente.chaves`,
    /// `agente.socket`, `ia.espacos`, `ia.mcp_avisado` e os caminhos dos assistentes deste
    /// computador) fica como está aqui, mesmo que o
    /// arquivo traga.
    pub fn importar(&self, texto: &str) -> Result<(Self, Vec<String>), String> {
        let valor = serde_json::from_str::<Value>(texto)
            .map_err(|erro| t!("config-json-invalido", erro = erro.to_string()))?;
        let Value::Object(mapa) = valor else {
            return Err(t!("config-json-nao-e-objeto"));
        };
        let (mut nova, mut avisos) = mesclar(mapa);
        avisos.extend(nova.validar());
        nova.versao = VERSAO;
        nova.rclone.credencial = self.rclone.credencial;
        nova.agente.chaves = self.agente.chaves.clone();
        nova.agente.socket = self.agente.socket.clone();
        nova.ia.espacos = self.ia.espacos.clone();
        nova.ia.mcp_avisado = self.ia.mcp_avisado;
        nova.ia.local_claude_caminho = self.ia.local_claude_caminho.clone();
        nova.ia.local_codex_caminho = self.ia.local_codex_caminho.clone();
        nova.ia.local_opencode_caminho = self.ia.local_opencode_caminho.clone();
        nova.instalacao = self.instalacao.clone();
        Ok((nova, avisos))
    }

    /// Volta tudo ao padrão. Mantém o que este código não conhece e as chaves do agente.
    pub fn restaurar_tudo(&mut self) {
        for categoria in CATEGORIAS
            .iter()
            .chain(&["ia_servidor", "ia_local", "mcp", "atalhos"])
        {
            self.restaurar_categoria(categoria);
        }
    }
}

/// Move o `configuracoes.json` ilegível para `.bak` (ou `.bak-<data>`, se aquele já existe) e
/// devolve o aviso.
fn guardar_copia(arquivo: &std::path::Path) -> String {
    match crate::armazenamento::guardar_ilegivel(arquivo) {
        Ok(copia) => t!("config-ilegivel-copia", copia = copia.display().to_string()),
        Err(erro) => t!("config-ilegivel-sem-copia", erro = erro.to_string()),
    }
}

/// O arquivo da casca guardava `pagina_inicial` e `recentes_max` na raiz; agora ficam em
/// `geral`. Devolve se moveu algo.
fn migrar_da_casca(raiz: &mut Map<String, Value>) -> bool {
    let mut moveu = false;
    for chave in ["pagina_inicial", "recentes_max"] {
        let Some(valor) = raiz.remove(chave) else {
            continue;
        };
        moveu = true;
        let geral = raiz
            .entry("geral")
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(geral) = geral {
            geral.entry(chave).or_insert(valor);
        }
    }
    moveu
}

/// Copia o conteúdo de um JSON antigo para a categoria `chave`, se o `configuracoes.json` ainda
/// não a tem, e renomeia o antigo para `.migrado`. Devolve se migrou.
fn migrar_arquivo(
    armazenamento: &Armazenamento,
    raiz: &mut Map<String, Value>,
    chave: &str,
    antigo: &std::path::Path,
) -> bool {
    if raiz.contains_key(chave) || !antigo.is_file() {
        return false;
    }
    let Ok(texto) = fs::read_to_string(antigo) else {
        return false;
    };
    let Ok(Value::Object(valores)) = serde_json::from_str::<Value>(&texto) else {
        // Ilegível: fica onde está, para a pessoa ver.
        return false;
    };
    raiz.insert(chave.to_owned(), Value::Object(valores));
    renomear_para_migrado(armazenamento, antigo);
    true
}

/// O `rclone.json` tinha `binario` e `config`; agora são `rclone.binario` e
/// `rclone.arquivo_config`.
fn migrar_rclone(armazenamento: &Armazenamento, raiz: &mut Map<String, Value>) -> bool {
    let antigo = armazenamento.arquivo_rclone();
    if raiz.contains_key("rclone") || !antigo.is_file() {
        return false;
    }
    let Ok(texto) = fs::read_to_string(&antigo) else {
        return false;
    };
    let Ok(Value::Object(valores)) = serde_json::from_str::<Value>(&texto) else {
        return false;
    };
    let mut rclone = Map::new();
    if let Some(binario) = valores.get("binario") {
        rclone.insert("binario".to_owned(), binario.clone());
    }
    if let Some(config) = valores.get("config") {
        rclone.insert("arquivo_config".to_owned(), config.clone());
    }
    raiz.insert("rclone".to_owned(), Value::Object(rclone));
    renomear_para_migrado(armazenamento, &antigo);
    true
}

fn renomear_para_migrado(_armazenamento: &Armazenamento, antigo: &std::path::Path) {
    if crate::armazenamento::so_leitura(antigo) {
        return;
    }
    let mut novo = antigo.as_os_str().to_owned();
    novo.push(".migrado");
    let _ = fs::rename(antigo, novo);
}

/// Mescla o que veio do arquivo sobre os padrões, **folha a folha**: cada valor do arquivo
/// entra se o conjunto continua legível, e senão fica de fora com um aviso. Um campo ruim não
/// derruba os outros.
///
/// É o que o plano pedia ao `serde_path_to_error`; aqui o caminho do campo ruim sai de testar as
/// folhas uma a uma, que funciona também dentro dos `#[serde(flatten)]` (onde o caminho do
/// `serde_path_to_error` se perde) e não acrescenta dependência.
pub fn mesclar(arquivo: Map<String, Value>) -> (Configuracoes, Vec<String>) {
    let padrao = serde_json::to_value(Configuracoes::default()).unwrap_or(Value::Null);
    let mut atual = padrao.clone();
    let mut avisos = Vec::new();
    let mut folhas = Vec::new();
    colher_folhas(
        &Value::Object(arquivo),
        Some(&padrao),
        &mut Vec::new(),
        &mut folhas,
    );
    for (caminho, valor) in folhas {
        let mut teste = atual.clone();
        definir(&mut teste, &caminho, valor);
        if serde_json::from_value::<Configuracoes>(teste.clone()).is_ok() {
            atual = teste;
        } else {
            let nome = caminho.join(".");
            let valor_padrao = caminho
                .iter()
                .try_fold(&padrao, |v, chave| v.get(chave))
                .map_or_else(String::new, mostrar);
            avisos.push(t!(
                "config-invalido-padrao",
                nome = nome.to_string(),
                valor_padrao = valor_padrao.to_string()
            ));
        }
    }
    let config = serde_json::from_value(atual).unwrap_or_default();
    (config, avisos)
}

/// As folhas de `valor`: objetos se abrem (quando o padrão também é um objeto, ou não existe);
/// o resto é uma folha.
fn colher_folhas(
    valor: &Value,
    padrao: Option<&Value>,
    caminho: &mut Vec<String>,
    saida: &mut Vec<(Vec<String>, Value)>,
) {
    match valor {
        Value::Object(mapa) if padrao.is_none_or(Value::is_object) => {
            for (chave, filho) in mapa {
                caminho.push(chave.clone());
                colher_folhas(filho, padrao.and_then(|p| p.get(chave)), caminho, saida);
                caminho.pop();
            }
        }
        _ if caminho.is_empty() => {}
        _ => saida.push((caminho.clone(), valor.clone())),
    }
}

/// Põe `valor` em `caminho`, criando os objetos do meio.
fn definir(raiz: &mut Value, caminho: &[String], valor: Value) {
    let Some((ultimo, meio)) = caminho.split_last() else {
        return;
    };
    let mut atual = raiz;
    for chave in meio {
        if !atual.get(chave).is_some_and(Value::is_object) {
            atual[chave.as_str()] = Value::Object(Map::new());
        }
        atual = &mut atual[chave.as_str()];
    }
    atual[ultimo.as_str()] = valor;
}

/// Um valor para o texto de um aviso.
fn mostrar(valor: &Value) -> String {
    match valor {
        Value::String(s) if s.is_empty() => "vazio".to_owned(),
        Value::String(s) => s.clone(),
        outro => outro.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    fn escrever(armazenamento: &Armazenamento, texto: &str) {
        fs::create_dir_all(armazenamento.pasta()).unwrap();
        fs::write(armazenamento.arquivo_configuracoes(), texto).unwrap();
    }

    #[test]
    fn sem_arquivo_vale_o_padrao_e_nada_e_gravado() {
        let pasta = PastaTemporaria::nova("cfg-vazio");
        let armazenamento = Armazenamento::new(&pasta.0);
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c, Configuracoes::default());
        assert_eq!(c.geral.recentes_max, 30);
        assert!(avisos.is_empty());
        assert!(!armazenamento.arquivo_configuracoes().exists());
    }

    #[test]
    fn campos_que_faltam_ganham_o_padrao() {
        let pasta = PastaTemporaria::nova("cfg-faltam");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(&armazenamento, r#"{"geral": {"recentes_max": 12}}"#);
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.geral.recentes_max, 12);
        assert_eq!(c.geral.pagina_inicial, PaginaInicial::Auto);
        assert_eq!(c.terminal.tamanho_fonte, 16);
        assert!(avisos.is_empty(), "{avisos:?}");
    }

    #[test]
    fn pagina_desconhecida_vira_automatica() {
        let pasta = PastaTemporaria::nova("cfg-pagina");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(
            &armazenamento,
            r#"{"geral": {"pagina_inicial": "arquivos"}}"#,
        );
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.geral.pagina_inicial, PaginaInicial::Auto);
        assert!(avisos.is_empty());
    }

    #[test]
    fn campo_com_tipo_errado_volta_ao_padrao_e_os_outros_ficam() {
        let pasta = PastaTemporaria::nova("cfg-tipo");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(
            &armazenamento,
            r#"{"terminal": {"tamanho_fonte": "grande", "historico": 500, "cursor": "bloco"},
                "arquivos": {"paralelas": 4}}"#,
        );
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.terminal.tamanho_fonte, 16);
        assert_eq!(c.terminal.historico, 500);
        assert_eq!(c.terminal.cursor, Cursor::Bloco);
        assert_eq!(c.arquivos.paralelas, 4);
        assert_eq!(
            avisos,
            ["terminal.tamanho_fonte inválido; voltou ao padrão (16)."]
        );
        assert!(!pasta.0.join("configuracoes.json.bak").exists());
    }

    #[test]
    fn enum_desconhecido_volta_ao_padrao_com_aviso() {
        let pasta = PastaTemporaria::nova("cfg-enum");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(&armazenamento, r#"{"geral": {"tema": "roxo"}}"#);
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.geral.tema, Tema::Sistema);
        assert_eq!(avisos, ["geral.tema inválido; voltou ao padrão (sistema)."]);
    }

    #[test]
    fn um_objeto_no_lugar_de_uma_categoria_volta_ao_padrao() {
        let pasta = PastaTemporaria::nova("cfg-categoria");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(
            &armazenamento,
            r#"{"terminal": "oi", "redes": {"descer_ao_fechar": false}}"#,
        );
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.terminal, TerminalCfg::default());
        assert!(!c.redes.descer_ao_fechar);
        assert_eq!(avisos.len(), 1, "{avisos:?}");
        assert!(avisos[0].starts_with("terminal inválido"), "{avisos:?}");
    }

    #[test]
    fn numero_fora_da_faixa_e_preso_com_aviso() {
        let pasta = PastaTemporaria::nova("cfg-faixa");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(
            &armazenamento,
            r#"{"terminal": {"tamanho_fonte": 500}, "arquivos": {"paralelas": 0},
                "conexoes": {"porta_padrao": 0}, "scripts": {"paralelo": 500}}"#,
        );
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.terminal.tamanho_fonte, 48);
        assert_eq!(c.arquivos.paralelas, 1);
        assert_eq!(c.conexoes.porta_padrao, 1);
        assert_eq!(c.scripts.paralelo, 64);
        assert_eq!(avisos.len(), 4, "{avisos:?}");
        assert!(
            avisos
                .iter()
                .any(|a| a.contains("terminal.tamanho_fonte fora da faixa (8 a 48)"))
        );
    }

    #[test]
    fn atalho_ilegivel_e_tirado_com_aviso() {
        let pasta = PastaTemporaria::nova("cfg-atalho");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(
            &armazenamento,
            r#"{"atalhos": {"shell_local": ["Ctrl+Shift+L"], "cofre": ["Ctrl+Banana"]}}"#,
        );
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.atalhos["shell_local"], ["Ctrl+Shift+L"]);
        assert!(!c.atalhos.contains_key("cofre"));
        assert_eq!(avisos.len(), 1, "{avisos:?}");
        assert!(avisos[0].contains("Ctrl+Banana"), "{avisos:?}");
    }

    #[test]
    fn campos_desconhecidos_sobrevivem_a_uma_gravacao() {
        let pasta = PastaTemporaria::nova("cfg-resto");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(
            &armazenamento,
            r#"{"geral": {"pagina_inicial": "terminais", "futuro": 1},
                "terminal": {"ligatura": true}, "zoom": 1.5, "tema_novo": {"escuro": true}}"#,
        );
        let (mut c, avisos) = Configuracoes::carregar(&armazenamento);
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(c.geral.pagina_inicial, PaginaInicial::Terminais);
        c.geral.recentes_max = 10;
        c.salvar(&armazenamento).unwrap();
        let texto = fs::read_to_string(armazenamento.arquivo_configuracoes()).unwrap();
        let json: Value = serde_json::from_str(&texto).unwrap();
        assert_eq!(json["tema_novo"]["escuro"], true);
        assert_eq!(json["zoom"], 1.5);
        assert_eq!(json["geral"]["futuro"], 1);
        assert_eq!(json["terminal"]["ligatura"], true);
        assert_eq!(json["geral"]["recentes_max"], 10);
        assert_eq!(json["geral"]["pagina_inicial"], "terminais");
        assert_eq!(json["versao"], 2);
    }

    #[test]
    fn arquivo_corrompido_vai_para_o_bak_com_aviso() {
        let pasta = PastaTemporaria::nova("cfg-corrompido");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(&armazenamento, "{ não é json");
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c, Configuracoes::default());
        assert_eq!(avisos.len(), 1);
        assert!(avisos[0].contains("configuracoes.json.bak"), "{avisos:?}");
        assert_eq!(
            fs::read_to_string(pasta.0.join("configuracoes.json.bak")).unwrap(),
            "{ não é json"
        );
    }

    #[test]
    fn json_que_nao_e_objeto_tambem_vai_para_o_bak() {
        let pasta = PastaTemporaria::nova("cfg-lista");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(&armazenamento, "[1, 2]");
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c, Configuracoes::default());
        assert_eq!(avisos.len(), 1);
        assert!(pasta.0.join("configuracoes.json.bak").is_file());
    }

    #[test]
    fn o_arquivo_da_casca_migra_para_geral() {
        let pasta = PastaTemporaria::nova("cfg-casca");
        let armazenamento = Armazenamento::new(&pasta.0);
        escrever(
            &armazenamento,
            r#"{"pagina_inicial": "inicio", "recentes_max": 12, "zoom": 2}"#,
        );
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(c.geral.pagina_inicial, PaginaInicial::Inicio);
        assert_eq!(c.geral.recentes_max, 12);
        let json: Value = serde_json::from_str(
            &fs::read_to_string(armazenamento.arquivo_configuracoes()).unwrap(),
        )
        .unwrap();
        assert_eq!(json["geral"]["recentes_max"], 12);
        assert!(json.get("recentes_max").is_none(), "saiu da raiz");
        assert_eq!(json["zoom"], 2, "o desconhecido ficou");
        assert_eq!(json["versao"], 2);
    }

    #[test]
    fn o_agente_json_migra_e_vira_migrado() {
        let pasta = PastaTemporaria::nova("cfg-agente");
        let armazenamento = Armazenamento::new(&pasta.0);
        fs::create_dir_all(&pasta.0).unwrap();
        fs::write(
            armazenamento.arquivo_agente(),
            r#"{"ligado": false, "vida_padrao_secs": 900,
                "chaves": [{"arquivo": "/k/id", "confirmar": true, "vida_secs": 60}]}"#,
        )
        .unwrap();
        let (c, avisos) = Configuracoes::carregar(&armazenamento);
        assert!(avisos.is_empty(), "{avisos:?}");
        assert!(!c.agente.ligado);
        assert_eq!(c.agente.vida_padrao_secs, 900);
        assert_eq!(c.agente.chaves.len(), 1);
        assert!(c.agente.chaves[0].confirmar);
        assert!(!armazenamento.arquivo_agente().exists());
        assert!(pasta.0.join("agente.json.migrado").is_file());
        assert!(armazenamento.arquivo_configuracoes().is_file(), "gravou");
        // Na segunda leitura não há o que migrar.
        let (de_novo, _) = Configuracoes::carregar(&armazenamento);
        assert_eq!(de_novo, c);
    }

    #[test]
    fn o_rclone_json_migra_para_a_categoria() {
        let pasta = PastaTemporaria::nova("cfg-rclone");
        let armazenamento = Armazenamento::new(&pasta.0);
        fs::create_dir_all(&pasta.0).unwrap();
        fs::write(
            armazenamento.arquivo_rclone(),
            r#"{"binario": "/opt/rclone", "config": "/etc/rclone.conf"}"#,
        )
        .unwrap();
        let (c, _) = Configuracoes::carregar(&armazenamento);
        assert_eq!(c.rclone.binario, "/opt/rclone");
        assert_eq!(c.rclone.arquivo_config, "/etc/rclone.conf");
        assert!(pasta.0.join("rclone.json.migrado").is_file());
    }

    #[test]
    fn gravar_e_atomico_e_ida_e_volta() {
        let pasta = PastaTemporaria::nova("cfg-atomico");
        let armazenamento = Armazenamento::new(&pasta.0);
        let mut c = Configuracoes::default();
        c.terminal.tamanho_fonte = 20;
        c.atalhos
            .insert("shell_local".into(), vec!["Ctrl+Shift+L".into()]);
        c.rclone.credencial = Some(7);
        c.salvar(&armazenamento).unwrap();
        let mut arquivos: Vec<_> = fs::read_dir(&pasta.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        arquivos.sort();
        assert_eq!(arquivos, ["configuracoes.json"], "sem sobra do .tmp");
        let (lido, avisos) = Configuracoes::carregar(&armazenamento);
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(lido, c);
    }

    #[test]
    fn o_arquivo_gravado_tem_indentacao_e_a_ordem_dos_structs() {
        let pasta = PastaTemporaria::nova("cfg-formato");
        let armazenamento = Armazenamento::new(&pasta.0);
        Configuracoes::default().salvar(&armazenamento).unwrap();
        let texto = fs::read_to_string(armazenamento.arquivo_configuracoes()).unwrap();
        assert!(
            texto.starts_with("{\n  \"versao\": 2,\n  \"geral\": {"),
            "{texto}"
        );
    }

    #[test]
    fn o_limite_fica_na_faixa() {
        let com = |recentes_max| {
            let mut c = Configuracoes::default();
            c.geral.recentes_max = recentes_max;
            c
        };
        assert_eq!(com(1).limite_de_recentes(), 5);
        assert_eq!(com(5000).limite_de_recentes(), 100);
        assert_eq!(com(30).limite_de_recentes(), 30);
    }

    #[test]
    fn a_ia_tem_os_padroes_do_plano() {
        let ia = IaCfg::default();
        assert!(ia.redigir && ia.mostrar_no_menu && ia.mostrar_o_que_sai);
        assert!(ia.redigir_extras.is_empty());
        assert_eq!((ia.linhas_contexto, ia.max_caracteres), (200, 200_000));
        assert_eq!(ia.comando_shell, "bash");
        assert_eq!(ia.espera_resposta, 0);
        assert!(!ia.mcp_ligado && ia.auditoria);
        assert_eq!((ia.marca_secs, ia.caracteres_por_resposta), (30, 100_000));
        assert!(ia.bloqueio_extras.is_empty() && ia.finais_de_prompt.is_empty());
        assert!(ia.pasta_troca.is_empty());
        assert_eq!(ia.linhas_por_leitura, 2000);
        assert!(!ia.mcp_avisado && ia.espacos.is_empty());
    }

    #[test]
    fn validar_prende_os_numeros_da_ia() {
        let mut c = Configuracoes::default();
        c.ia.linhas_contexto = 0;
        c.ia.espera_resposta = 99_999;
        let avisos = c.validar();
        assert_eq!(c.ia.linhas_contexto, 1);
        assert_eq!(c.ia.espera_resposta, 3600);
        assert!(avisos.len() >= 2, "{avisos:?}");
        // Zero em "máximo de caracteres" é válido: não limita.
        c.ia.max_caracteres = 0;
        assert!(c.validar().is_empty());
        assert_eq!(c.ia.max_caracteres, 0);
    }

    #[test]
    fn restaurar_a_ia_volta_ao_padrao() {
        let mut c = Configuracoes::default();
        c.ia.redigir = false;
        c.ia.redigir_extras = vec!["x".into()];
        c.ia.comando_shell = "zsh".into();
        c.terminal.tamanho_fonte = 30;
        assert!(c.restaurar_categoria("ia"));
        assert_eq!(c.ia, IaCfg::default());
        assert_eq!(c.terminal.tamanho_fonte, 30);
    }

    #[test]
    fn restaurar_o_mcp_mexe_so_nos_ajustes_dele() {
        let mut c = Configuracoes::default();
        c.ia.mcp_ligado = true;
        c.ia.auditoria = false;
        c.ia.marca_secs = 5;
        c.ia.comandos_na_aba = false;
        c.ia.bloqueio_extras = vec!["docker".into()];
        c.ia.mcp_avisado = true;
        c.ia.espacos.push(crate::mcp::espaco::EspacoIa {
            nome: "site".into(),
            conexao: 1,
            raiz: "/var/www".into(),
            espelho: None,
        });
        c.ia.redigir = false;
        assert!(c.restaurar_categoria("mcp"));
        assert!(!c.ia.mcp_ligado && c.ia.auditoria && c.ia.comandos_na_aba);
        assert_eq!(c.ia.marca_secs, 30);
        assert!(c.ia.bloqueio_extras.is_empty());
        assert!(c.ia.mcp_avisado, "o aviso visto não é ajuste");
        assert_eq!(c.ia.espacos.len(), 1);
        assert!(!c.ia.redigir, "os ajustes da IA ficam");
    }

    #[test]
    fn restaurar_a_ia_nao_mexe_no_mcp() {
        let mut c = Configuracoes::default();
        c.ia.redigir = false;
        c.ia.mcp_ligado = true;
        c.ia.marca_secs = 5;
        c.ia.mcp_avisado = true;
        assert!(c.restaurar_categoria("ia"));
        assert!(c.ia.redigir);
        assert!(c.ia.mcp_ligado && c.ia.mcp_avisado);
        assert_eq!(c.ia.marca_secs, 5);
    }

    #[test]
    fn restaurar_os_assistentes_no_servidor_mexe_so_neles() {
        let mut c = Configuracoes::default();
        c.ia.preferido = AssistentePreferido::Codex;
        c.ia.abrir_em = AbrirIa::NestaAba;
        c.ia.argumentos_claude = "--model sonnet".into();
        c.ia.metodo_codex = MetodoIa::Npm;
        c.ia.mostrar_opencode = false;
        c.ia.redigir = false;
        c.ia.mcp_ligado = true;
        assert!(c.restaurar_categoria("ia_servidor"));
        assert_eq!(c.ia.preferido, AssistentePreferido::Claude);
        assert_eq!(c.ia.abrir_em, AbrirIa::AbaNova);
        assert!(c.ia.argumentos_claude.is_empty());
        assert_eq!(c.ia.metodo_codex, MetodoIa::Automatico);
        assert!(c.ia.mostrar_opencode);
        assert!(!c.ia.redigir, "os ajustes da IA do terminal ficam");
        assert!(c.ia.mcp_ligado, "o controle por assistentes fica");
    }

    #[test]
    fn restaurar_a_ia_e_o_mcp_nao_mexem_nos_assistentes_no_servidor() {
        let mut c = Configuracoes::default();
        c.ia.preferido = AssistentePreferido::OpenCode;
        c.ia.argumentos_codex = "--full-auto".into();
        c.ia.redigir = false;
        c.ia.marca_secs = 5;
        assert!(c.restaurar_categoria("ia"));
        assert!(c.restaurar_categoria("mcp"));
        assert!(c.ia.redigir);
        assert_eq!(c.ia.marca_secs, 30);
        assert_eq!(c.ia.preferido, AssistentePreferido::OpenCode);
        assert_eq!(c.ia.argumentos_codex, "--full-auto");
        c.restaurar_tudo();
        assert_eq!(c.ia, IaCfg::default());
    }

    #[test]
    fn restaurar_uma_categoria_so_mexe_nela() {
        let mut c = Configuracoes::default();
        c.terminal.tamanho_fonte = 30;
        c.arquivos.paralelas = 5;
        c.agente.chaves.push(ChaveSalva::new("/k/id"));
        c.agente.ligado = false;
        assert!(c.restaurar_categoria("terminal"));
        assert_eq!(c.terminal.tamanho_fonte, 16);
        assert_eq!(c.arquivos.paralelas, 5);
        assert!(c.restaurar_categoria("agente"));
        assert!(c.agente.ligado);
        assert_eq!(c.agente.chaves.len(), 1, "a lista de chaves não tem card");
        assert!(!c.restaurar_categoria("nao_existe"));
    }

    #[test]
    fn restaurar_tudo_mantem_o_resto_desconhecido() {
        let mut c = Configuracoes::default();
        c.resto.insert("zoom".into(), Value::from(2));
        c.terminal.tamanho_fonte = 30;
        c.atalhos.insert("cofre".into(), vec!["Ctrl+K".into()]);
        c.restaurar_tudo();
        assert_eq!(c.terminal.tamanho_fonte, 16);
        assert!(c.atalhos.is_empty());
        assert_eq!(c.resto["zoom"], 2);
    }

    #[test]
    fn a_exportacao_nao_leva_segredo_nem_o_que_e_da_maquina() {
        let mut c = Configuracoes::default();
        c.terminal.tamanho_fonte = 22;
        c.rclone.credencial = Some(7);
        c.agente.socket = "/tmp/meu-agente.sock".into();
        let texto = c.exportar();
        assert!(texto.ends_with('\n'));
        let json: Value = serde_json::from_str(&texto).unwrap();
        assert_eq!(json["terminal"]["tamanho_fonte"], 22);
        assert!(json["rclone"].get("credencial").is_none(), "{texto}");
        assert!(json["agente"].get("chaves").is_none(), "{texto}");
        assert!(json["agente"].get("socket").is_none(), "{texto}");
        assert!(!texto.contains("meu-agente"), "{texto}");
    }

    #[test]
    fn importar_troca_o_resto_e_mantem_o_que_e_da_maquina() {
        let mut origem = Configuracoes::default();
        origem.terminal.tamanho_fonte = 22;
        origem.geral.recentes_max = 12;
        let texto = origem.exportar();

        let mut aqui = Configuracoes::default();
        aqui.terminal.tamanho_fonte = 10;
        aqui.rclone.credencial = Some(3);
        aqui.agente.socket = "/tmp/daqui.sock".into();
        let (nova, avisos) = aqui.importar(&texto).unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(nova.terminal.tamanho_fonte, 22);
        assert_eq!(nova.geral.recentes_max, 12);
        assert_eq!(nova.rclone.credencial, Some(3));
        assert_eq!(nova.agente.socket, "/tmp/daqui.sock");
    }

    #[test]
    fn a_exportacao_nao_leva_os_caminhos_dos_assistentes_deste_computador() {
        let mut origem = Configuracoes::default();
        origem.ia.local_claude_caminho = "/home/ana/.local/bin/claude".into();
        origem.ia.local_codex_caminho = "/home/ana/bin/codex".into();
        origem.ia.local_opencode_caminho = "/home/ana/bin/opencode".into();
        origem.ia.local_claude_argumentos = "--model sonnet".into();
        let texto = origem.exportar();
        assert!(!texto.contains("/home/ana"), "{texto}");
        assert!(!texto.contains("local_claude_caminho"), "{texto}");
        // Os argumentos são preferência e vão.
        assert!(texto.contains("local_claude_argumentos"), "{texto}");

        let mut aqui = Configuracoes::default();
        aqui.ia.local_claude_caminho = "/usr/bin/claude".into();
        let (nova, _) = aqui.importar(&texto).unwrap();
        assert_eq!(nova.ia.local_claude_caminho, "/usr/bin/claude");
        assert_eq!(nova.ia.local_claude_argumentos, "--model sonnet");
    }

    #[test]
    fn restaurar_os_assistentes_deste_computador_mexe_so_neles() {
        let mut c = Configuracoes::default();
        c.ia.local_claude_caminho = "/opt/claude".into();
        c.ia.local_codex_argumentos = "--model o3".into();
        c.ia.argumentos_claude = "--model sonnet".into();
        c.ia.redigir = false;
        assert!(c.restaurar_categoria("ia_local"));
        assert!(c.ia.local_claude_caminho.is_empty());
        assert!(c.ia.local_codex_argumentos.is_empty());
        assert_eq!(
            c.ia.argumentos_claude, "--model sonnet",
            "os do servidor ficam"
        );
        assert!(!c.ia.redigir, "os ajustes da IA do terminal ficam");
    }

    #[test]
    fn importar_ignora_o_que_o_arquivo_traz_da_maquina_e_avisa_do_que_voltou_ao_padrao() {
        let aqui = Configuracoes::default();
        let texto = r#"{"rclone": {"credencial": 9}, "agente": {"socket": "/x"},
                        "terminal": {"tamanho_fonte": 500}}"#;
        let (nova, avisos) = aqui.importar(texto).unwrap();
        assert_eq!(nova.rclone.credencial, None);
        assert_eq!(nova.agente.socket, aqui.agente.socket);
        assert_eq!(nova.terminal.tamanho_fonte, 48, "preso na faixa");
        assert!(!avisos.is_empty());
    }

    #[test]
    fn importar_recusa_o_que_nao_e_um_objeto_json() {
        let c = Configuracoes::default();
        let erro = c.importar("isto não é json").unwrap_err();
        assert!(erro.contains("não é um JSON válido"), "{erro}");
        let erro = c.importar("[1, 2]").unwrap_err();
        assert!(erro.contains("não é um objeto"), "{erro}");
    }

    #[test]
    fn as_escolhas_ida_e_volta_pelo_nome_e_pelo_json() {
        for esquema in Esquema::TODAS {
            assert_eq!(Esquema::do_nome(&esquema.nome()), Some(*esquema));
            let json = serde_json::to_string(esquema).unwrap();
            assert_eq!(serde_json::from_str::<Esquema>(&json).unwrap(), *esquema);
        }
        assert_eq!(
            serde_json::to_string(&Esquema::OneHalfDark).unwrap(),
            "\"one_half_dark\""
        );
        assert_eq!(
            serde_json::to_string(&Tema::Sistema).unwrap(),
            "\"sistema\""
        );
        assert_eq!(ValidadeLink::SeteDias.segundos(), 604_800);
    }

    #[test]
    fn a_pagina_ao_iniciar_segue_a_escolha_e_o_estado() {
        assert_eq!(PaginaInicial::Auto.chave(true), "terminais");
        assert_eq!(PaginaInicial::Auto.chave(false), "inicio");
        assert_eq!(PaginaInicial::Inicio.chave(true), "inicio");
        assert_eq!(PaginaInicial::Terminais.chave(false), "terminais");
        for p in PaginaInicial::TODAS {
            assert_eq!(PaginaInicial::do_nome(&p.nome()), Some(p));
        }
    }
}
