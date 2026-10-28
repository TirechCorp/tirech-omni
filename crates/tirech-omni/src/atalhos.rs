//! Os atalhos de teclado do app: o catálogo das ações (com ids estáveis, que são a chave do
//! `atalhos` das configurações) e a leitura do que a pessoa mudou.
//!
//! As configurações guardam só os atalhos que diferem do padrão do catálogo. Quem monta a tela
//! pede o atalho por [`atalho`] ou [`paineis`], que devolvem closures: o atalho é lido na hora
//! da tecla, então mudar a configuração troca o atalho e o texto do menu sem montar a tela de
//! novo.

use tomni_ui::prelude::*;
use tomni_ui::terminal::PaneShortcuts;

use crate::configuracoes::Configuracoes;
use crate::i18n;
use crate::{t, tk};

/// De onde vem uma ação: o grupo da página e o padrão.
#[derive(Clone, Copy)]
enum Origem {
    /// Uma ação do app, com os atalhos padrão separados por vírgula (`""` é sem atalho de
    /// fábrica).
    App(&'static str),
    /// Um comando de `PaneShortcuts`.
    Painel {
        padrao: fn(&PaneShortcuts) -> &Vec<Shortcut>,
        mudar: fn(&mut PaneShortcuts) -> &mut Vec<Shortcut>,
    },
}

/// Uma ação com atalho.
#[derive(Clone, Copy)]
pub struct Acao {
    /// O id estável, em `snake_case`: a chave de `atalhos` no `configuracoes.json`.
    pub id: &'static str,
    /// A chave do nome mostrado na página (use [`Acao::nome`]).
    nome: &'static str,
    /// O grupo da página: "Tirech Omni" ou "Painéis".
    pub grupo: Grupo,
    origem: Origem,
}

/// O grupo (o expander) de uma ação na página de Configurações.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grupo {
    /// As ações do app.
    TirechOmni,
    /// Os comandos dos painéis do terminal.
    Paineis,
}

impl Acao {
    /// O nome mostrado na página, no idioma de agora.
    pub fn nome(&self) -> String {
        t!(self.nome)
    }

    /// Se a ação existe com o que `config` liga em Geral: as dos assistentes de IA somem com o
    /// `geral.assistente_ia` desligado, e o cluster com IA com o `geral.mcp`.
    pub fn ligada(&self, config: &Configuracoes) -> bool {
        match self.id {
            "ia_comando" | "ia_chat" | "ia_abrir" => config.geral.assistente_ia,
            "cluster_ia" => config.geral.mcp,
            _ => true,
        }
    }

    /// O nome em `idioma` (a busca de Configurações olha os dois).
    pub fn nome_em(&self, idioma: crate::configuracoes::Idioma) -> String {
        i18n::texto_em(idioma, self.nome, None)
    }

    /// Os atalhos de fábrica da ação.
    pub fn padrao(&self) -> Vec<Shortcut> {
        match self.origem {
            Origem::App("") => Vec::new(),
            Origem::App(texto) => ler(texto).expect("atalho do catálogo"),
            Origem::Painel { padrao, .. } => padrao(&PaneShortcuts::default()).clone(),
        }
    }
}

const fn app(id: &'static str, nome: &'static str, padrao: &'static str) -> Acao {
    Acao {
        id,
        nome,
        grupo: Grupo::TirechOmni,
        origem: Origem::App(padrao),
    }
}

macro_rules! painel {
    ($id:literal, $nome:expr, $($campo:tt)+) => {
        Acao {
            id: $id,
            nome: $nome,
            grupo: Grupo::Paineis,
            origem: Origem::Painel {
                padrao: |p| &p.$($campo)+,
                mudar: |p| &mut p.$($campo)+,
            },
        }
    };
}

/// Todas as ações com atalho, na ordem em que a página as mostra (e em que um atalho repetido
/// vale: a primeira).
pub const CATALOGO: &[Acao] = &[
    app("nova_conexao", tk!("atalho-nova-conexao"), "Ctrl+Shift+N"),
    app("novo_grupo", tk!("atalho-novo-grupo"), "Ctrl+Shift+G"),
    app("shell_local", tk!("atalho-shell-local"), "Ctrl+Shift+T"),
    app("layout_1", tk!("atalho-layout-1"), "Ctrl+Alt+1"),
    app("layout_2x2", tk!("atalho-layout-2x2"), "Ctrl+Alt+2"),
    app("layout_3x3", tk!("atalho-layout-3x3"), "Ctrl+Alt+3"),
    app(
        "identificar_paineis",
        tk!("atalho-identificar-paineis"),
        "Ctrl+Alt+I",
    ),
    app(
        "painel_conexoes",
        tk!("atalho-painel-conexoes"),
        "Ctrl+Alt+B",
    ),
    app("painel_propriedades", tk!("atalho-painel-propriedades"), ""),
    app("painel_notas", tk!("atalho-painel-notas"), ""),
    app("painel_fila", tk!("atalho-painel-fila"), ""),
    app("nova_janela", tk!("atalho-nova-janela"), "Ctrl+Alt+N"),
    app("multi_entrada", tk!("atalho-multi-entrada"), "Ctrl+Shift+I"),
    app("cluster", tk!("atalho-cluster"), ""),
    app("cluster_ia", tk!("atalho-cluster-ia"), ""),
    app("scripts", tk!("atalho-scripts"), ""),
    app("links", tk!("atalho-links"), ""),
    app(
        "reconectar_caidas",
        tk!("atalho-reconectar-caidas"),
        "Ctrl+Shift+R",
    ),
    app("fechar_caidas", tk!("atalho-fechar-caidas"), ""),
    app("fechar_todas", tk!("atalho-fechar-todas"), ""),
    app("cofre", tk!("atalho-cofre"), "Ctrl+Shift+K"),
    app("notas", tk!("atalho-notas"), "Ctrl+Shift+J"),
    app("ia_comando", tk!("atalho-ia-comando"), "Ctrl+Shift+Space"),
    app("ia_chat", tk!("atalho-ia-chat"), "Ctrl+Shift+A"),
    app("ia_abrir", tk!("atalho-ia-abrir"), "Ctrl+Alt+A"),
    app("busca", tk!("atalho-busca"), "Ctrl+E"),
    app("configuracoes", tk!("atalho-configuracoes"), "Ctrl+,"),
    app(
        "aumentar_fonte",
        tk!("atalho-aumentar-fonte"),
        "Ctrl+=, Ctrl++, Ctrl+Shift++",
    ),
    app("diminuir_fonte", tk!("atalho-diminuir-fonte"), "Ctrl+-"),
    app("fonte_padrao", tk!("atalho-fonte-padrao"), "Ctrl+0"),
    painel!("dividir_abaixo", tk!("atalho-dividir-abaixo"), split_down),
    painel!(
        "dividir_direita",
        tk!("atalho-dividir-direita"),
        split_right
    ),
    painel!("duplicar_sessao", tk!("atalho-duplicar-sessao"), duplicate),
    painel!("focar_esquerda", tk!("atalho-focar-esquerda"), focus[0]),
    painel!("focar_direita", tk!("atalho-focar-direita"), focus[1]),
    painel!("focar_acima", tk!("atalho-focar-acima"), focus[2]),
    painel!("focar_abaixo", tk!("atalho-focar-abaixo"), focus[3]),
    painel!(
        "redimensionar_esquerda",
        tk!("atalho-redimensionar-esquerda"),
        resize[0]
    ),
    painel!(
        "redimensionar_direita",
        tk!("atalho-redimensionar-direita"),
        resize[1]
    ),
    painel!(
        "redimensionar_acima",
        tk!("atalho-redimensionar-acima"),
        resize[2]
    ),
    painel!(
        "redimensionar_abaixo",
        tk!("atalho-redimensionar-abaixo"),
        resize[3]
    ),
    painel!("fechar_painel", tk!("atalho-fechar-painel"), close_pane),
    painel!("zoom_painel", tk!("atalho-zoom-painel"), zoom),
];

/// A ação de `id`.
pub fn acao(id: &str) -> Option<&'static Acao> {
    CATALOGO.iter().find(|a| a.id == id)
}

/// Os atalhos de `id` nas `config`: os escolhidos que o `FromStr` lê ou, sem eles, os de
/// fábrica. A ação desligada em Geral ([`Acao::ligada`]) fica sem atalho.
pub fn atalhos_de(config: &Configuracoes, id: &str) -> Vec<Shortcut> {
    let Some(acao) = acao(id).filter(|a| a.ligada(config)) else {
        return Vec::new();
    };
    let escolhidos: Vec<Shortcut> = config
        .atalhos
        .get(id)
        .map(|textos| textos.iter().filter_map(|t| t.parse().ok()).collect())
        .unwrap_or_default();
    if escolhidos.is_empty() {
        acao.padrao()
    } else {
        escolhidos
    }
}

/// O atalho principal de `id` (o primeiro), lido das `config` a cada chamada: serve de prop de
/// `.shortcut()`, `.accelerator()` e `.focus_shortcut()`. Sem atalho, é o `Shortcut::NONE`.
pub fn atalho(config: Signal<Configuracoes>, id: &'static str) -> impl Fn() -> Shortcut + 'static {
    move || config.with(|c| principal(c, id))
}

/// O `n`-ésimo atalho de `id` (a partir de 0), lido das `config` a cada chamada: serve para a
/// ação que vale por mais de um atalho (`.accelerator()` aceita um só, então a tela põe um por
/// posição). Sem o `n`-ésimo, é o `Shortcut::NONE`, que nunca casa.
pub fn atalho_n(
    config: Signal<Configuracoes>,
    id: &'static str,
    n: usize,
) -> impl Fn() -> Shortcut + 'static {
    move || config.with(|c| atalhos_de(c, id).get(n).copied().unwrap_or(Shortcut::NONE))
}

/// O atalho principal de `id` nas `config`, ou `Shortcut::NONE`.
pub fn principal(config: &Configuracoes, id: &str) -> Shortcut {
    atalhos_de(config, id)
        .into_iter()
        .next()
        .unwrap_or(Shortcut::NONE)
}

/// Os atalhos dos painéis do terminal, das `config` (prop de `TerminalPanes::shortcuts`).
pub fn paineis(config: Signal<Configuracoes>) -> impl Fn() -> PaneShortcuts + 'static {
    move || config.with(paineis_de)
}

/// Os atalhos dos painéis de `config`: o padrão da lib com o que a pessoa mudou por cima.
pub fn paineis_de(config: &Configuracoes) -> PaneShortcuts {
    let mut atalhos = PaneShortcuts::default();
    for acao in CATALOGO {
        if let Origem::Painel { mudar, .. } = acao.origem
            && config.atalhos.contains_key(acao.id)
        {
            *mudar(&mut atalhos) = atalhos_de(config, acao.id);
        }
    }
    atalhos
}

/// O texto de uma lista de atalhos, como a página mostra: `Ctrl+Shift+T, Alt+Shift+D`.
pub fn texto(atalhos: &[Shortcut]) -> String {
    atalhos
        .iter()
        .map(Shortcut::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Se a vírgula que vem depois de `lido` é a tecla (`Ctrl+,`) e não o separador: é quando
/// `lido` termina num `+` que separa um modificador (o `+` de `Alt+Shift++` já é a tecla).
fn virgula_e_tecla(lido: &str) -> bool {
    let lido = lido.trim_end();
    let Some(antes) = lido.strip_suffix('+') else {
        return false;
    };
    !antes.is_empty() && !antes.ends_with('+')
}

/// Lê o texto digitado: atalhos separados por vírgula (a vírgula depois de um `+` é a tecla,
/// como em `Ctrl+,`). Vazio vale por "nenhum".
pub fn ler(texto: &str) -> Result<Vec<Shortcut>, String> {
    let mut pedacos = Vec::new();
    let mut atual = String::new();
    for c in texto.chars() {
        if c == ',' && !virgula_e_tecla(&atual) {
            pedacos.push(std::mem::take(&mut atual));
        } else {
            atual.push(c);
        }
    }
    pedacos.push(atual);
    pedacos
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<Shortcut>().map_err(|e| e.to_string()))
        .collect()
}

/// Grava os `atalhos` de `id` em `config`: só o que difere do padrão do catálogo fica no
/// arquivo (e vazio volta ao padrão).
pub fn definir(config: &mut Configuracoes, id: &str, atalhos: &[Shortcut]) {
    let Some(acao) = acao(id) else { return };
    if atalhos.is_empty() || atalhos == acao.padrao() {
        config.atalhos.remove(id);
    } else {
        config.atalhos.insert(
            id.to_owned(),
            atalhos.iter().map(Shortcut::to_string).collect(),
        );
    }
}

/// Se algum atalho de `id` é o de outra ação, o nome da primeira ação que o usa antes dele no
/// catálogo (vale a que vem primeiro).
pub fn tambem_usado_por(config: &Configuracoes, id: &str) -> Option<String> {
    let meus = atalhos_de(config, id);
    let posicao = CATALOGO.iter().position(|a| a.id == id)?;
    CATALOGO
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != posicao)
        .find(|(_, a)| {
            atalhos_de(config, a.id)
                .iter()
                .any(|outro| meus.contains(outro))
        })
        .map(|(_, a)| a.nome())
}

/// O aviso de um atalho que o terminal usa (Ctrl e uma letra, sem Shift nem Alt): o programa
/// do terminal deixa de recebê-lo.
pub fn tira_do_terminal(atalhos: &[Shortcut]) -> Option<String> {
    atalhos
        .iter()
        .find(|a| {
            a.modifiers == tomni_ui::Modifiers::CTRL
                && matches!(a.key, Key::Char(c) if c.is_ascii_alphabetic())
        })
        .map(|a| t!("atalho-terminal-deixa-de-receber", atalho = a.to_string()))
}

#[cfg(test)]
mod testes {
    use tomni_ui::Modifiers;

    use super::*;

    #[test]
    fn os_ids_do_catalogo_nao_se_repetem() {
        for (i, a) in CATALOGO.iter().enumerate() {
            assert!(
                CATALOGO[i + 1..].iter().all(|b| b.id != a.id),
                "{} repetido",
                a.id
            );
        }
    }

    #[test]
    fn todo_padrao_do_catalogo_le_e_escreve_igual() {
        for a in CATALOGO {
            let padrao = a.padrao();
            assert_eq!(
                ler(&texto(&padrao)).as_deref(),
                Ok(padrao.as_slice()),
                "{}",
                a.id
            );
        }
    }

    #[test]
    fn acao_sem_atalho_de_fabrica_nao_dispara_nem_aparece() {
        let mut c = Configuracoes::default();
        assert!(acao("fechar_todas").unwrap().padrao().is_empty());
        assert!(principal(&c, "fechar_todas").is_none());
        // A pessoa pode pôr um atalho nela e tirar de novo.
        definir(&mut c, "fechar_todas", &[Shortcut::ctrl_shift('q')]);
        assert_eq!(principal(&c, "fechar_todas"), Shortcut::ctrl_shift('q'));
        definir(&mut c, "fechar_todas", &[]);
        assert!(principal(&c, "fechar_todas").is_none());
    }

    #[test]
    fn as_acoes_desligadas_em_geral_ficam_sem_atalho() {
        let mut c = Configuracoes::default();
        definir(&mut c, "cluster_ia", &[Shortcut::ctrl_shift('y')]);
        assert_eq!(principal(&c, "ia_chat"), Shortcut::ctrl_shift('a'));
        assert_eq!(principal(&c, "cluster_ia"), Shortcut::ctrl_shift('y'));

        c.geral.assistente_ia = false;
        for id in ["ia_comando", "ia_chat", "ia_abrir"] {
            assert!(atalhos_de(&c, id).is_empty(), "{id}");
        }
        assert_eq!(principal(&c, "cluster_ia"), Shortcut::ctrl_shift('y'));

        c.geral.mcp = false;
        assert!(principal(&c, "cluster_ia").is_none());
        assert_eq!(principal(&c, "multi_entrada"), Shortcut::ctrl_shift('i'));
        assert!(
            c.atalhos.contains_key("cluster_ia"),
            "o atalho escolhido continua gravado"
        );
    }

    #[test]
    fn a_virgula_depois_do_mais_e_a_tecla() {
        assert_eq!(ler("Ctrl+,"), Ok(vec![Shortcut::ctrl(',')]));
        assert_eq!(
            ler("Ctrl+, , Alt+K"),
            Ok(vec![Shortcut::ctrl(','), Shortcut::alt('k')])
        );
        assert_eq!(
            ler("Ctrl+A, Ctrl+B"),
            Ok(vec![Shortcut::ctrl('a'), Shortcut::ctrl('b')])
        );
        assert_eq!(
            ler("Alt+Shift++, Alt+Shift+="),
            Ok(vec![
                Shortcut::new('+', Modifiers::ALT | Modifiers::SHIFT),
                Shortcut::new('=', Modifiers::ALT | Modifiers::SHIFT)
            ])
        );
        assert_eq!(ler(""), Ok(Vec::new()));
        assert!(ler("Ctrl+Xyz").is_err());
    }

    #[test]
    fn sem_mudanca_vale_o_padrao_e_so_o_mudado_vai_para_o_arquivo() {
        let mut c = Configuracoes::default();
        assert_eq!(
            atalhos_de(&c, "shell_local"),
            vec![Shortcut::ctrl_shift('t')]
        );
        definir(&mut c, "shell_local", &[Shortcut::ctrl_shift('l')]);
        assert_eq!(c.atalhos["shell_local"], ["Ctrl+Shift+L"]);
        assert_eq!(
            atalhos_de(&c, "shell_local"),
            vec![Shortcut::ctrl_shift('l')]
        );
        definir(&mut c, "shell_local", &[Shortcut::ctrl_shift('t')]);
        assert!(c.atalhos.is_empty());
    }

    #[test]
    fn os_paineis_seguem_a_configuracao() {
        let mut c = Configuracoes::default();
        assert_eq!(
            paineis_de(&c).close_pane,
            PaneShortcuts::default().close_pane
        );
        definir(&mut c, "fechar_painel", &[Shortcut::ctrl_shift('x')]);
        definir(&mut c, "focar_acima", &[Shortcut::alt('k')]);
        let p = paineis_de(&c);
        assert_eq!(p.close_pane, vec![Shortcut::ctrl_shift('x')]);
        assert_eq!(p.focus[2], vec![Shortcut::alt('k')]);
        assert_eq!(p.focus[0], PaneShortcuts::default().focus[0]);
    }

    #[test]
    fn um_atalho_repetido_diz_com_quem() {
        let mut c = Configuracoes::default();
        assert_eq!(tambem_usado_por(&c, "shell_local"), None);
        definir(&mut c, "novo_grupo", &[Shortcut::ctrl_shift('t')]);
        assert_eq!(
            tambem_usado_por(&c, "novo_grupo"),
            Some("Abrir shell local".to_owned())
        );
        assert_eq!(
            tambem_usado_por(&c, "shell_local"),
            Some("Novo grupo".to_owned())
        );
    }

    #[test]
    fn o_zoom_da_fonte_tem_tres_atalhos_para_aumentar_e_um_para_o_resto() {
        let c = Configuracoes::default();
        assert_eq!(
            atalhos_de(&c, "aumentar_fonte"),
            vec![
                Shortcut::ctrl('='),
                Shortcut::ctrl('+'),
                Shortcut::new('+', Modifiers::CTRL | Modifiers::SHIFT),
            ]
        );
        assert_eq!(atalhos_de(&c, "diminuir_fonte"), vec![Shortcut::ctrl('-')]);
        assert_eq!(atalhos_de(&c, "fonte_padrao"), vec![Shortcut::ctrl('0')]);
        // Nenhum deles pega a tecla de quem usa Ctrl+letra no terminal.
        for id in ["aumentar_fonte", "diminuir_fonte", "fonte_padrao"] {
            assert_eq!(tira_do_terminal(&atalhos_de(&c, id)), None, "{id}");
        }
    }

    #[test]
    fn o_atalho_da_posicao_que_falta_nao_casa_com_nada() {
        let config = signal(Configuracoes::default());
        assert_eq!(atalho_n(config, "aumentar_fonte", 0)(), Shortcut::ctrl('='));
        assert_eq!(atalho_n(config, "diminuir_fonte", 0)(), Shortcut::ctrl('-'));
        assert!(atalho_n(config, "diminuir_fonte", 1)().is_none());
        config.update(|c| definir(c, "aumentar_fonte", &[Shortcut::alt('z')]));
        assert_eq!(atalho_n(config, "aumentar_fonte", 0)(), Shortcut::alt('z'));
        assert!(atalho_n(config, "aumentar_fonte", 1)().is_none());
    }

    #[test]
    fn ctrl_e_uma_letra_tira_a_tecla_do_terminal() {
        assert_eq!(
            tira_do_terminal(&[Shortcut::ctrl('c')]).as_deref(),
            Some("O terminal deixa de receber Ctrl+C.")
        );
        assert_eq!(tira_do_terminal(&[Shortcut::ctrl_shift('c')]), None);
        assert_eq!(tira_do_terminal(&[Shortcut::key(Key::F(5))]), None);
    }
}
