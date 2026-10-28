//! O catálogo da página de Configurações: uma [`Definicao`] por card. A página, a busca da
//! própria página, a busca global da casca e o teste de cobertura leem o mesmo catálogo, então
//! um card novo nasce aqui.
//!
//! A chave é o caminho no JSON (`terminal.tamanho_fonte`); as ações que não gravam nada
//! começam por `acao.`. O id do card na tela é [`id_do_card`] e o do controle de dentro é o do
//! card mais `-valor`.

use crate::busca::normalizar;
use crate::configuracoes::{Configuracoes, Idioma};
use crate::{i18n, t, tk};

/// O que um card é.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tipo {
    /// Uma configuração que grava um valor.
    Valor,
    /// Um grupo (um `SettingsExpander` sem valor próprio): só agrupa filhos.
    Grupo,
    /// Um botão ou card clicável que faz algo e não grava um valor.
    Acao,
}

/// Um card da página. O título, a descrição e as palavras da busca moram nos `.ftl`, na mensagem
/// `config-<chave com . trocado por ->` (a mesma regra do [`id_do_card`]), com os atributos
/// `.descricao` e `.palavras`.
#[derive(Clone, Copy, Debug)]
pub struct Definicao {
    /// O caminho no JSON, ou `acao.<nome>`.
    pub chave: &'static str,
    /// A chave do nome da seção na página (`tk!("config-secao-geral")`).
    pub secao: &'static str,
    /// O que o card é.
    pub tipo: Tipo,
}

/// As opções que só existem no Windows.
const SO_WINDOWS: &[&str] = &["agente.sistema_git", "agente.pageant"];

/// As opções que só existem no Linux.
const SO_LINUX: &[&str] = &["geral.atalho_no_menu"];

/// As opções que escrevem fora da pasta de dados (o início com o sistema, o atalho, a
/// instalação, as integrações do agente): somem no modo portable (M66).
const DO_SISTEMA: &[&str] = &[
    "geral.iniciar_com_sistema",
    "geral.atalho_no_menu",
    "acao.instalacao",
    "agente.sistema_ssh_config",
    "agente.sistema_shells",
    "agente.sistema_sessao",
    "agente.sistema_git",
];

impl Definicao {
    /// Se a opção existe neste sistema (algumas são só do Windows, outras só do Linux) e neste
    /// modo (o portable não tem as que escrevem no sistema).
    pub fn disponivel(&self) -> bool {
        self.disponivel_em(crate::armazenamento::modo_portatil())
    }

    /// [`Self::disponivel`] com o modo portable dado (a página o tira do `Ambiente`).
    pub fn disponivel_em(&self, portatil: bool) -> bool {
        (cfg!(windows) || !SO_WINDOWS.contains(&self.chave))
            && (cfg!(target_os = "linux") || !SO_LINUX.contains(&self.chave))
            && !(portatil && DO_SISTEMA.contains(&self.chave))
    }

    /// Se o card aparece com o que `config` liga em Geral: as seções dos assistentes de IA
    /// somem com o `geral.assistente_ia` desligado, e as do controle por assistentes com o
    /// `geral.mcp`.
    pub fn ligada(&self, config: &Configuracoes) -> bool {
        if [tk!("config-secao-ia"), tk!("config-secao-ia-servidor")].contains(&self.secao) {
            config.geral.assistente_ia
        } else if [tk!("config-secao-ia-local"), tk!("config-secao-mcp")].contains(&self.secao) {
            config.geral.mcp
        } else {
            true
        }
    }

    /// O id do card na tela.
    pub fn id(&self) -> String {
        id_do_card(self.chave)
    }

    /// O título do card, no idioma atual.
    pub fn titulo(&self) -> String {
        t!(&self.id())
    }

    /// A descrição do card, no idioma atual.
    pub fn descricao(&self) -> String {
        t!(&format!("{}.descricao", self.id()))
    }

    /// As outras palavras que levam ao card na busca, no idioma atual.
    pub fn palavras(&self) -> String {
        t!(&format!("{}.palavras", self.id()))
    }

    /// O nome da seção na página, no idioma atual.
    pub fn nome_da_secao(&self) -> String {
        t!(self.secao)
    }

    /// O texto onde a busca procura, já normalizado: o do idioma atual **e** o do pt-BR, para
    /// quem muda de idioma não perder o hábito (em inglês, "fonte" e "font" acham o mesmo card).
    pub fn texto_da_busca(&self) -> String {
        let id = self.id();
        let em = |idioma| {
            format!(
                "{} {} {} {}",
                i18n::texto_em(idioma, &id, None),
                i18n::texto_em(idioma, &format!("{id}.descricao"), None),
                i18n::texto_em(idioma, self.secao, None),
                i18n::texto_em(idioma, &format!("{id}.palavras"), None),
            )
        };
        let atual = i18n::atual();
        if atual == Idioma::PtBr {
            normalizar(&em(Idioma::PtBr))
        } else {
            normalizar(&format!("{} {}", em(atual), em(Idioma::PtBr)))
        }
    }
}

/// O id do card de `chave`: `config-` e a chave com `.` trocado por `-`.
pub fn id_do_card(chave: &str) -> String {
    format!("config-{}", chave.replace('.', "-"))
}

/// O id do controle de dentro do card de `chave`.
pub fn id_do_valor(chave: &str) -> String {
    format!("{}-valor", id_do_card(chave))
}

/// A definição de `chave`.
pub fn definicao(chave: &str) -> Option<&'static Definicao> {
    CATALOGO.iter().find(|d| d.chave == chave)
}

/// Se o texto casa com todos os `termos` (já normalizados). Sem termos, casa tudo.
pub fn casa(termos: &[String], texto_normalizado: &str) -> bool {
    termos
        .iter()
        .all(|t| texto_normalizado.contains(t.as_str()))
}

/// Os termos de uma busca: normalizados e separados por espaço.
pub fn termos(busca: &str) -> Vec<String> {
    normalizar(busca)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// As definições que casam com a `busca`, na ordem da página, sem as que `config` desliga.
pub fn buscar(busca: &str, config: &Configuracoes) -> Vec<&'static Definicao> {
    buscar_em(busca, crate::armazenamento::modo_portatil(), config)
}

/// [`buscar`] com o modo portable dado.
pub fn buscar_em(busca: &str, portatil: bool, config: &Configuracoes) -> Vec<&'static Definicao> {
    let termos = termos(busca);
    CATALOGO
        .iter()
        .filter(|d| {
            d.tipo != Tipo::Grupo
                && d.disponivel_em(portatil)
                && d.ligada(config)
                && casa(&termos, &d.texto_da_busca())
        })
        .collect()
}

/// Um card do catálogo; o texto fica nos `.ftl` (`config-<chave>`).
macro_rules! def {
    ($chave:literal, $secao:expr, $tipo:ident) => {
        Definicao {
            chave: $chave,
            secao: $secao,
            tipo: Tipo::$tipo,
        }
    };
}

/// Todos os cards, na ordem da página.
pub const CATALOGO: &[Definicao] = &[
    // Geral
    def!("geral.tema", tk!("config-secao-geral"), Valor),
    def!("geral.idioma", tk!("config-secao-geral"), Valor),
    def!("geral.pagina_inicial", tk!("config-secao-geral"), Valor),
    def!("geral.restaurar_sessoes", tk!("config-secao-geral"), Valor),
    def!("geral.confirmar_fechar", tk!("config-secao-geral"), Valor),
    def!("geral.ao_fechar", tk!("config-secao-geral"), Valor),
    def!(
        "geral.iniciar_com_sistema",
        tk!("config-secao-geral"),
        Valor
    ),
    def!("geral.iniciar_na_bandeja", tk!("config-secao-geral"), Valor),
    def!("geral.atalho_no_menu", tk!("config-secao-geral"), Valor),
    def!("geral.medidor_cpu", tk!("config-secao-geral"), Valor),
    def!("geral.recentes_max", tk!("config-secao-geral"), Valor),
    def!("acao.limpar_recentes", tk!("config-secao-geral"), Acao),
    def!("geral.assistente_ia", tk!("config-secao-geral"), Valor),
    def!("geral.mcp", tk!("config-secao-geral"), Valor),
    def!("acao.instalacao", tk!("config-secao-geral"), Acao),
    def!("acao.pasta_dados", tk!("config-secao-geral"), Acao),
    def!("acao.sobre", tk!("config-secao-geral"), Acao),
    // Terminal
    def!(
        "terminal.tamanho_fonte",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!("terminal.esquema", tk!("config-secao-terminal"), Valor),
    def!("terminal.cursor", tk!("config-secao-terminal"), Valor),
    def!(
        "terminal.cursor_piscando",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!("terminal.historico", tk!("config-secao-terminal"), Valor),
    def!(
        "terminal.copiar_ao_selecionar",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!("terminal.avisar_colar", tk!("config-secao-terminal"), Valor),
    def!(
        "terminal.colar_arquivos",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!("terminal.pasta_colar", tk!("config-secao-terminal"), Valor),
    def!(
        "terminal.ctrl_v_arquivos",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.limpar_colados_dias",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.colar_tar_min_arquivos",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.fechar_painel_vazio",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.duplicar_na_pasta",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.links_com_ctrl",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.botao_direito",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!("terminal.campainha", tk!("config-secao-terminal"), Valor),
    def!("terminal.shell", tk!("config-secao-terminal"), Grupo),
    def!(
        "terminal.shell.programa",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.shell.argumentos",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!("terminal.shell.pasta", tk!("config-secao-terminal"), Valor),
    def!("terminal.logs", tk!("config-secao-terminal"), Grupo),
    def!("terminal.logs.pasta", tk!("config-secao-terminal"), Valor),
    def!(
        "terminal.logs.todas_as_sessoes",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!("terminal.logs.cifrar", tk!("config-secao-terminal"), Valor),
    def!(
        "terminal.logs.selar_secs",
        tk!("config-secao-terminal"),
        Valor
    ),
    def!(
        "terminal.logs.visualizar_mib",
        tk!("config-secao-terminal"),
        Valor
    ),
    // Conexões
    def!(
        "conexoes.usuario_padrao",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!("conexoes.porta_padrao", tk!("config-secao-conexoes"), Valor),
    def!("conexoes.term_padrao", tk!("config-secao-conexoes"), Valor),
    def!(
        "conexoes.keepalive_padrao_secs",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!("conexoes.known_hosts", tk!("config-secao-conexoes"), Valor),
    def!("conexoes.chave_nova", tk!("config-secao-conexoes"), Valor),
    def!("conexoes.reconectar", tk!("config-secao-conexoes"), Grupo),
    def!(
        "conexoes.reconectar.modo",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!(
        "conexoes.reconectar.tentativas",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!(
        "conexoes.reconectar.espera_secs",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!(
        "conexoes.reconectar.retentar_mins",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!("conexoes.abrir_em", tk!("config-secao-conexoes"), Valor),
    def!(
        "acao.importar_ssh_config",
        tk!("config-secao-conexoes"),
        Acao
    ),
    def!("acao.importar_aws", tk!("config-secao-conexoes"), Acao),
    def!(
        "conexoes.confirmar_fechar_todas",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!(
        "conexoes.tmux_ao_fechar",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!(
        "conexoes.importar_senhas",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!(
        "conexoes.comandos_quieto_ms",
        tk!("config-secao-conexoes"),
        Valor
    ),
    def!(
        "conexoes.comandos_max_secs",
        tk!("config-secao-conexoes"),
        Valor
    ),
    // Scripts
    def!("scripts.interpretador", tk!("config-secao-scripts"), Valor),
    def!("scripts.paralelo", tk!("config-secao-scripts"), Valor),
    def!("scripts.pasta", tk!("config-secao-scripts"), Valor),
    // Cofre de senhas
    def!("acao.protecao_cofre", tk!("config-secao-cofre"), Acao),
    def!(
        "cofre.limpar_area_transferencia_secs",
        tk!("config-secao-cofre"),
        Valor
    ),
    def!("cofre.bloquear_apos_min", tk!("config-secao-cofre"), Valor),
    def!("acao.cofre_local", tk!("config-secao-cofre"), Acao),
    def!("acao.cofre_levar", tk!("config-secao-cofre"), Acao),
    def!("acao.cofre_abrir_outro", tk!("config-secao-cofre"), Acao),
    def!("cofre.gerador", tk!("config-secao-cofre"), Grupo),
    def!("cofre.gerador.tamanho", tk!("config-secao-cofre"), Valor),
    def!("cofre.gerador.maiusculas", tk!("config-secao-cofre"), Valor),
    def!("cofre.gerador.minusculas", tk!("config-secao-cofre"), Valor),
    def!("cofre.gerador.digitos", tk!("config-secao-cofre"), Valor),
    def!("cofre.gerador.simbolos", tk!("config-secao-cofre"), Valor),
    def!(
        "cofre.gerador.sem_ambiguos",
        tk!("config-secao-cofre"),
        Valor
    ),
    def!("acao.csv_cofre", tk!("config-secao-cofre"), Acao),
    // Notas
    def!("notas.modo", tk!("config-secao-notas"), Valor),
    def!("notas.fonte_mono", tk!("config-secao-notas"), Valor),
    def!("notas.quebrar_linhas", tk!("config-secao-notas"), Valor),
    // Chaves SSH e agente
    def!("agente.ligado", tk!("config-secao-agente"), Valor),
    def!("agente.socket", tk!("config-secao-agente"), Valor),
    def!(
        "agente.exportar_no_shell",
        tk!("config-secao-agente"),
        Valor
    ),
    def!(
        "agente.carregar_ao_iniciar",
        tk!("config-secao-agente"),
        Valor
    ),
    def!("agente.vida_padrao_secs", tk!("config-secao-agente"), Valor),
    def!("agente.confirmar_padrao", tk!("config-secao-agente"), Valor),
    def!(
        "agente.encaminhar_padrao",
        tk!("config-secao-agente"),
        Valor
    ),
    def!("agente.pasta_chaves", tk!("config-secao-agente"), Valor),
    def!("agente.chaves_novas", tk!("config-secao-agente"), Grupo),
    def!("agente.tipo_padrao", tk!("config-secao-agente"), Valor),
    def!("agente.rodadas_padrao", tk!("config-secao-agente"), Valor),
    def!(
        "agente.sistema_ssh_config",
        tk!("config-secao-agente"),
        Valor
    ),
    def!("agente.sistema_shells", tk!("config-secao-agente"), Valor),
    def!("agente.sistema_sessao", tk!("config-secao-agente"), Valor),
    def!("agente.sistema_git", tk!("config-secao-agente"), Valor),
    def!(
        "agente.tirar_ao_bloquear",
        tk!("config-secao-agente"),
        Valor
    ),
    def!("agente.pageant", tk!("config-secao-agente"), Valor),
    def!("acao.gerenciar_chaves", tk!("config-secao-agente"), Acao),
    // Arquivos
    def!("arquivos.paralelas", tk!("config-secao-arquivos"), Valor),
    def!("arquivos.conflito", tk!("config-secao-arquivos"), Valor),
    def!(
        "arquivos.mostrar_ocultos",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!("arquivos.editor", tk!("config-secao-arquivos"), Valor),
    def!(
        "arquivos.enviar_editado",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!(
        "arquivos.preservar_data",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!(
        "arquivos.preservar_permissoes",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!("arquivos.pasta_local", tk!("config-secao-arquivos"), Valor),
    def!(
        "arquivos.confirmar_apagar",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!(
        "arquivos.keepalive_secs",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!("arquivos.comparar", tk!("config-secao-arquivos"), Valor),
    def!(
        "arquivos.comparar_modo",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!(
        "arquivos.comparar_tolerancia_min",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!(
        "arquivos.ocultar_identicos",
        tk!("config-secao-arquivos"),
        Valor
    ),
    def!("arquivos.sincronizar", tk!("config-secao-arquivos"), Valor),
    // FTP
    def!("ftp.seguranca", tk!("config-secao-ftp"), Valor),
    def!("ftp.modo", tk!("config-secao-ftp"), Valor),
    def!("ftp.tls", tk!("config-secao-ftp"), Valor),
    def!("ftp.codificacao", tk!("config-secao-ftp"), Valor),
    // S3
    def!("s3.regiao", tk!("config-secao-s3"), Valor),
    def!("s3.estilo", tk!("config-secao-s3"), Valor),
    def!("s3.parte_mib", tk!("config-secao-s3"), Valor),
    def!("s3.classe", tk!("config-secao-s3"), Valor),
    def!("s3.validade_link", tk!("config-secao-s3"), Valor),
    // rclone
    def!("rclone.binario", tk!("config-secao-rclone"), Valor),
    def!("rclone.arquivo_config", tk!("config-secao-rclone"), Valor),
    def!("rclone.credencial", tk!("config-secao-rclone"), Valor),
    def!("acao.testar_rclone", tk!("config-secao-rclone"), Acao),
    // Redes
    def!("acao.perfis_de_rede", tk!("config-secao-redes"), Acao),
    def!("redes.espera_padrao_secs", tk!("config-secao-redes"), Valor),
    def!("redes.descer_ao_fechar", tk!("config-secao-redes"), Valor),
    def!("redes.navegador", tk!("config-secao-redes"), Valor),
    // X11 e túneis
    def!("x11.padrao", tk!("config-secao-x11"), Valor),
    def!("x11.display", tk!("config-secao-x11"), Valor),
    def!("x11.virt_manager", tk!("config-secao-x11"), Valor),
    def!("tuneis.outras_maquinas", tk!("config-secao-x11"), Valor),
    // Portas do servidor (M58)
    def!("portas.intervalo_secs", tk!("config-secao-portas"), Valor),
    def!("portas.sondar", tk!("config-secao-portas"), Valor),
    def!("portas.sonda_ms", tk!("config-secao-portas"), Valor),
    def!("portas.usar_ferramentas", tk!("config-secao-portas"), Valor),
    def!("portas.avisar_novas", tk!("config-secao-portas"), Valor),
    def!("portas.links_pelo_tunel", tk!("config-secao-portas"), Valor),
    def!("portas.mostrar_sistema", tk!("config-secao-portas"), Valor),
    // Janelas e espaços
    def!("janelas.pasta_espacos", tk!("config-secao-janelas"), Valor),
    def!("janelas.abrir_espaco", tk!("config-secao-janelas"), Valor),
    def!("janelas.tear_off", tk!("config-secao-janelas"), Valor),
    // Assistentes de IA
    def!("acao.perfis_de_ia", tk!("config-secao-ia"), Acao),
    def!("ia.redigir", tk!("config-secao-ia"), Valor),
    def!("ia.redigir_extras", tk!("config-secao-ia"), Valor),
    def!("ia.mostrar_no_menu", tk!("config-secao-ia"), Valor),
    def!("ia.mostrar_o_que_sai", tk!("config-secao-ia"), Valor),
    def!("ia.linhas_contexto", tk!("config-secao-ia"), Valor),
    def!("ia.max_caracteres", tk!("config-secao-ia"), Valor),
    def!("ia.comando_shell", tk!("config-secao-ia"), Valor),
    def!("ia.espera_resposta", tk!("config-secao-ia"), Valor),
    // Assistentes no servidor (M46)
    def!("ia.preferido", tk!("config-secao-ia-servidor"), Valor),
    def!("ia.abrir_em", tk!("config-secao-ia-servidor"), Valor),
    def!("ia.shell_ao_sair", tk!("config-secao-ia-servidor"), Valor),
    def!("ia.tunel_codex", tk!("config-secao-ia-servidor"), Valor),
    def!(
        "ia.verificar_ao_abrir",
        tk!("config-secao-ia-servidor"),
        Valor
    ),
    def!("ia.mostrar_botao", tk!("config-secao-ia-servidor"), Valor),
    def!("ia.mostrar_claude", tk!("config-secao-ia-servidor"), Valor),
    def!("ia.mostrar_codex", tk!("config-secao-ia-servidor"), Valor),
    def!(
        "ia.mostrar_opencode",
        tk!("config-secao-ia-servidor"),
        Valor
    ),
    def!("ia.metodo_claude", tk!("config-secao-ia-servidor"), Valor),
    def!("ia.metodo_codex", tk!("config-secao-ia-servidor"), Valor),
    def!("ia.metodo_opencode", tk!("config-secao-ia-servidor"), Valor),
    def!(
        "ia.argumentos_claude",
        tk!("config-secao-ia-servidor"),
        Valor
    ),
    def!(
        "ia.argumentos_codex",
        tk!("config-secao-ia-servidor"),
        Valor
    ),
    def!(
        "ia.argumentos_opencode",
        tk!("config-secao-ia-servidor"),
        Valor
    ),
    // Assistentes deste computador (M63)
    def!("ia.local_pasta", tk!("config-secao-ia-local"), Valor),
    def!(
        "acao.ia_local_abrir_pasta",
        tk!("config-secao-ia-local"),
        Acao
    ),
    def!("acao.ia_local_limpar", tk!("config-secao-ia-local"), Acao),
    def!(
        "ia.local_claude_caminho",
        tk!("config-secao-ia-local"),
        Valor
    ),
    def!(
        "ia.local_claude_argumentos",
        tk!("config-secao-ia-local"),
        Valor
    ),
    def!(
        "ia.local_codex_caminho",
        tk!("config-secao-ia-local"),
        Valor
    ),
    def!(
        "ia.local_codex_argumentos",
        tk!("config-secao-ia-local"),
        Valor
    ),
    def!(
        "ia.local_opencode_caminho",
        tk!("config-secao-ia-local"),
        Valor
    ),
    def!(
        "ia.local_opencode_argumentos",
        tk!("config-secao-ia-local"),
        Valor
    ),
    def!("ia.conversas_lembrar", tk!("config-secao-ia-local"), Valor),
    def!("ia.conversas_reabrir", tk!("config-secao-ia-local"), Valor),
    def!(
        "ia.conversas_ao_controlar",
        tk!("config-secao-ia-local"),
        Valor
    ),
    // Controle por assistentes (MCP)
    def!("ia.mcp_ligado", tk!("config-secao-mcp"), Valor),
    def!("acao.mcp_como", tk!("config-secao-mcp"), Acao),
    def!("acao.mcp_espaco_novo", tk!("config-secao-mcp"), Acao),
    def!("ia.bloqueio_extras", tk!("config-secao-mcp"), Valor),
    def!("ia.auditoria", tk!("config-secao-mcp"), Valor),
    def!("acao.mcp_registro", tk!("config-secao-mcp"), Acao),
    def!("acao.mcp_pasta_registro", tk!("config-secao-mcp"), Acao),
    def!("ia.marca_secs", tk!("config-secao-mcp"), Valor),
    def!("ia.linhas_por_leitura", tk!("config-secao-mcp"), Valor),
    def!("ia.caracteres_por_resposta", tk!("config-secao-mcp"), Valor),
    def!("ia.comandos_na_aba", tk!("config-secao-mcp"), Valor),
    def!("ia.finais_de_prompt", tk!("config-secao-mcp"), Valor),
    def!("ia.pasta_troca", tk!("config-secao-mcp"), Valor),
    // Fim da página
    def!("acao.exportar", tk!("config-secao-fim"), Acao),
    def!("acao.importar", tk!("config-secao-fim"), Acao),
    def!("acao.restaurar_tudo", tk!("config-secao-fim"), Acao),
];

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn as_chaves_do_catalogo_nao_se_repetem() {
        for (i, a) in CATALOGO.iter().enumerate() {
            assert!(
                CATALOGO[i + 1..].iter().all(|b| b.chave != a.chave),
                "{} repetida",
                a.chave
            );
        }
    }

    #[test]
    fn as_acoes_comecam_por_acao_e_os_valores_nao() {
        for d in CATALOGO {
            assert_eq!(
                d.chave.starts_with("acao."),
                d.tipo == Tipo::Acao,
                "{}",
                d.chave
            );
        }
    }

    #[test]
    fn todo_valor_do_catalogo_e_um_caminho_do_json() {
        let json = serde_json::to_value(crate::configuracoes::Configuracoes::default()).unwrap();
        for d in CATALOGO.iter().filter(|d| d.tipo == Tipo::Valor) {
            // Os cards de dentro de um grupo sem campo próprio (as chaves novas) não têm caminho.
            let existe = d
                .chave
                .split('.')
                .try_fold(&json, |v, parte| v.get(parte))
                .is_some();
            assert!(existe, "{} não está no JSON", d.chave);
        }
    }

    #[test]
    fn o_id_troca_o_ponto_por_hifen() {
        assert_eq!(
            id_do_card("terminal.tamanho_fonte"),
            "config-terminal-tamanho_fonte"
        );
        assert_eq!(
            id_do_valor("cofre.gerador.tamanho"),
            "config-cofre-gerador-tamanho-valor"
        );
    }

    #[test]
    fn a_busca_ignora_acentos_e_exige_todos_os_termos() {
        let padrao = Configuracoes::default();
        let chaves =
            |q: &str| -> Vec<&str> { buscar(q, &padrao).iter().map(|d| d.chave).collect() };
        assert!(chaves("FONTE").contains(&"terminal.tamanho_fonte"));
        assert!(chaves("pagina iniciar").contains(&"geral.pagina_inicial"));
        assert!(chaves("página ao iniciar").contains(&"geral.pagina_inicial"));
        assert!(chaves("fonte cursor zzz").is_empty());
        assert!(buscar("xyz-nada", &padrao).is_empty());
    }

    #[test]
    fn uma_secao_inteira_casa_pelo_nome() {
        let ftp: Vec<_> = buscar("ftp", &Configuracoes::default())
            .iter()
            .map(|d| d.chave)
            .collect();
        assert!(ftp.contains(&"ftp.modo") && ftp.contains(&"ftp.tls"));
    }

    #[test]
    fn as_areas_desligadas_em_geral_somem_da_busca() {
        let chaves = |c: &Configuracoes, q: &str| -> Vec<&str> {
            buscar(q, c).iter().map(|d| d.chave).collect()
        };
        let mut c = Configuracoes::default();
        assert!(chaves(&c, "ollama").contains(&"acao.perfis_de_ia"));
        assert!(chaves(&c, "auditoria").contains(&"ia.auditoria"));

        c.geral.assistente_ia = false;
        assert!(!chaves(&c, "ollama").contains(&"acao.perfis_de_ia"));
        assert!(!chaves(&c, "claude").contains(&"ia.mostrar_claude"));
        assert!(chaves(&c, "auditoria").contains(&"ia.auditoria"));
        assert!(
            chaves(&c, "assistente").contains(&"geral.assistente_ia"),
            "o interruptor fica achável"
        );

        c.geral.mcp = false;
        assert!(chaves(&c, "auditoria").is_empty());
        assert!(
            chaves(&c, "conversas")
                .iter()
                .all(|k| !k.starts_with("ia.conversas"))
        );
        assert!(chaves(&c, "mcp").contains(&"geral.mcp"));
    }
}
