//! "Como ligar um assistente" (M51, seção 9.3) e o aviso de ligar o controle (seção 9.1).
//!
//! O diálogo mostra, para o Claude Code, o Codex e o OpenCode, o comando e o trecho de configuração que
//! abrem a ponte `tirech-omni mcp` (os textos vêm de [`crate::mcp::clientes`]), com o caminho do
//! executável que está rodando. Quem quer o assistente numa pasta do servidor escolhe o espaço
//! de trabalho embaixo das abas: os comandos ganham `--espaco <nome>` e aparece o bloco que
//! desliga as ferramentas locais do cliente.

use std::rc::Rc;

use tomni_ui::clipboard;
use tomni_ui::prelude::*;

use super::Tela;
use crate::mcp::clientes::{
    Escopo, comando_claude, comando_codex, config_codex_so_servidor, deny_claude, json_claude,
    json_opencode, toml_codex,
};
use crate::t;

/// O que o diálogo "Como ligar" e o aviso mostram.
pub(super) struct SinaisComo {
    /// Se o "Como ligar" está aberto.
    pub aberto: Signal<bool>,
    /// A aba escolhida: `claude`, `codex` ou `opencode`.
    pub aba: Signal<String>,
    /// O espaço de trabalho escolhido (o rótulo de "Nenhum" quando não há).
    pub espaco: Signal<String>,
    /// Se a explicação das permissões está à mostra.
    pub permissoes: Signal<bool>,
    /// Se o aviso de ligar o controle pela primeira vez está aberto.
    pub aviso: Signal<bool>,
    /// Os textos dos blocos, na ordem de `mcp-copiar-{n}` (de 1 a 8).
    pub blocos: [Signal<String>; 8],
}

impl SinaisComo {
    pub(super) fn novos() -> Self {
        Self {
            aberto: signal(false),
            aba: signal("claude".to_owned()),
            espaco: signal(t!("comum-nenhum")),
            permissoes: signal(false),
            aviso: signal(false),
            blocos: std::array::from_fn(|_| signal(String::new())),
        }
    }
}

/// O caminho do executável que está rodando, como o ambiente o conhece (os testes fixam um).
fn executavel(tela: &Tela) -> String {
    crate::instalacao::exe_alvo(tela.ferramentas.ambiente())
        .as_ref()
        .map_or_else(|| "tirech-omni".to_owned(), |c| c.display().to_string())
}

/// O texto dos oito blocos para o executável `exe` e o espaço `espaco`.
pub(super) fn textos(exe: &str, espaco: Option<&str>) -> [String; 8] {
    [
        comando_claude(exe, Escopo::Local, espaco),
        comando_claude(exe, Escopo::Usuario, espaco),
        json_claude(exe, espaco),
        deny_claude(),
        comando_codex(exe, espaco),
        toml_codex(exe, espaco),
        config_codex_so_servidor(),
        json_opencode(exe, espaco),
    ]
}

impl Tela {
    /// Abre o "Como ligar um assistente", com o espaço `espaco` já escolhido quando há.
    pub(super) fn abrir_como_ligar(&self, espaco: Option<&str>) {
        let s = &self.mcp.como;
        s.espaco.set(match espaco {
            Some(nome) => nome.to_owned(),
            None => t!("comum-nenhum"),
        });
        s.permissoes.set(false);
        s.aberto.set(true);
    }

    /// O espaço escolhido no "Como ligar", se há (e ainda existe).
    fn espaco_do_como(&self) -> Option<String> {
        let escolhido = self.mcp.como.espaco.get();
        self.config.with(|c| {
            c.ia.espacos
                .iter()
                .find(|e| e.nome == escolhido)
                .map(|e| e.nome.clone())
        })
    }

    /// Liga o controle depois do aviso: a pessoa viu e quer.
    pub(super) fn ligar_depois_do_aviso(&self) {
        self.config.update(|c| {
            c.ia.mcp_avisado = true;
            c.ia.mcp_ligado = true;
        });
    }

    /// A linha de estado do controle por assistentes, nas Configurações.
    pub(super) fn estado_do_controle(&self) -> String {
        if !self.config.with(|c| c.ia.mcp_ligado) {
            return t!("mcp-estado-desligado");
        }
        if let Some(erro) = self.mcp.erro.get() {
            return erro;
        }
        match self.mcp.sessoes.with(Vec::len) {
            0 => t!("mcp-estado-esperando"),
            1 => t!("mcp-estado-um"),
            n => t!("mcp-estado-varios", n = n),
        }
    }

    /// Mostra o aviso de ligar o controle pela primeira vez.
    pub(super) fn mostrar_aviso_mcp(&self) {
        // Um pedido do "Controlar com" que esperava um aviso anterior não vale mais.
        self.limpar_controle_pendente();
        self.mcp.como.aviso.set(true);
    }

    /// Liga ou desliga o controle por assistentes (o item do menu Ferramentas). A primeira
    /// ligação mostra o aviso.
    pub(super) fn alternar_controle_mcp(&self) {
        let (ligado, avisado) = self
            .config
            .with_untracked(|c| (c.ia.mcp_ligado, c.ia.mcp_avisado));
        if !ligado && !avisado {
            self.mostrar_aviso_mcp();
            return;
        }
        self.config.update(|c| c.ia.mcp_ligado = !ligado);
    }
}

/// Um bloco de texto monoespaçado, selecionável, com o botão "Copiar" `mcp-copiar-{n}`.
fn bloco(n: usize, texto: Signal<String>) -> Column {
    Column::new()
        .gap(4)
        .child(
            TextEditor::bind(texto)
                .id(format!("mcp-como-bloco-{n}"))
                .read_only(true)
                .monospace()
                .wrap(true)
                .min_height(36.0)
                .max_height(160.0),
        )
        .child(
            Row::new().child(
                Button::new(t!("comum-copiar"))
                    .id(format!("mcp-copiar-{n}"))
                    .icon(Symbol::Copy)
                    .on_click(move || clipboard::write(texto.get_untracked())),
            ),
        )
}

fn rotulo(texto: String) -> Text {
    Text::new(texto).secondary().wrap()
}

/// O "Como ligar um assistente" (`mcp-como-dialogo`) e o aviso `mcp-aviso`.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View + use<> {
    let s = &tela.mcp.como;
    let (aberto, aba, espaco, permissoes, aviso, blocos) =
        (s.aberto, s.aba, s.espaco, s.permissoes, s.aviso, s.blocos);
    let exe = executavel(tela);
    // Os textos acompanham o espaço escolhido.
    {
        let tela = Rc::clone(tela);
        effect(move || {
            let escolhido = tela.espaco_do_como();
            let novos = textos(&exe, escolhido.as_deref());
            untrack(|| {
                for (sinal, texto) in blocos.iter().zip(novos) {
                    if sinal.get_untracked() != texto {
                        sinal.set(texto);
                    }
                }
            });
        });
    }
    let com_espaco = {
        let tela = Rc::clone(tela);
        move || tela.espaco_do_como().is_some()
    };
    let claude = Column::new()
        .gap(8)
        .padding(Insets::new(8.0, 0.0, 0.0, 0.0))
        .child(rotulo(t!("mcp-como-terminal")))
        .child(bloco(1, blocos[0]))
        .child(rotulo(t!("mcp-como-todas-as-pastas")))
        .child(bloco(2, blocos[1]))
        .child(rotulo(t!("mcp-como-mcp-json")))
        .child(bloco(3, blocos[2]))
        .child(
            Column::new()
                .gap(8)
                .visible(com_espaco.clone())
                .child(rotulo(t!("mcp-como-claude-so-servidor")))
                .child(bloco(4, blocos[3]))
                .child(rotulo(t!("mcp-como-assim-ele-nao-mexe"))),
        );
    let codex = Column::new()
        .gap(8)
        .padding(Insets::new(8.0, 0.0, 0.0, 0.0))
        .child(rotulo(t!("mcp-como-terminal")))
        .child(bloco(5, blocos[4]))
        .child(rotulo(t!("mcp-como-codex-toml")))
        .child(bloco(6, blocos[5]))
        .child(
            Column::new()
                .gap(8)
                .visible(com_espaco)
                .child(rotulo(t!("mcp-como-codex-so-servidor")))
                .child(bloco(7, blocos[6]))
                .child(rotulo(t!("mcp-como-codex-aviso"))),
        );
    let opencode = Column::new()
        .gap(8)
        .padding(Insets::new(8.0, 0.0, 0.0, 0.0))
        .child(rotulo(t!("mcp-como-opencode-json")))
        .child(bloco(8, blocos[7]))
        .child(rotulo(t!("mcp-como-opencode-prazo")));
    let escolher_espaco = {
        let tela = Rc::clone(tela);
        // As opções nascem com a caixa: os espaços podem ter mudado desde a última vez.
        Show::when(aberto, move || {
            let mut opcoes = vec![t!("comum-nenhum")];
            tela.config
                .with_untracked(|c| opcoes.extend(c.ia.espacos.iter().map(|e| e.nome.clone())));
            ComboBox::bind(espaco, opcoes).id("mcp-como-espaco")
        })
    };
    let como = ContentDialog::new(t!("mcp-como-titulo"), aberto)
        .id("mcp-como-dialogo")
        .content(
            Column::new()
                .gap(8)
                .width(496)
                .child(rotulo(t!("mcp-como-intro")))
                .child(
                    TabView::bind(aba)
                        .id("mcp-como-abas")
                        .tab("claude", t!("mcp-como-claude"), claude)
                        .tab("codex", t!("mcp-como-codex"), codex)
                        .tab("opencode", t!("mcp-como-opencode"), opencode),
                )
                .child(Text::new(t!("mcp-como-pasta-do-servidor")).body_strong())
                .child(rotulo(t!("mcp-como-pasta-do-servidor-texto")))
                .child(escolher_espaco)
                .child(rotulo(t!("mcp-como-app-aberto")))
                .child(
                    Row::new().child(
                        Button::new(t!("mcp-como-permissoes"))
                            .id("mcp-como-permissoes")
                            .hyperlink()
                            .on_click(move || permissoes.update(|p| *p = !*p)),
                    ),
                )
                .child(
                    Text::new(t!("mcp-linha-descricao-pasta"))
                        .id("mcp-como-permissoes-texto")
                        .secondary()
                        .wrap()
                        .visible(move || permissoes.get()),
                ),
        )
        .close(t!("comum-fechar"));
    let t = Rc::clone(tela);
    let aviso_dialogo = ContentDialog::new(t!("mcp-aviso-titulo"), aviso)
        .id("mcp-aviso")
        .content(Text::new(t!("mcp-aviso-texto")).wrap().width(440))
        .primary(t!("mcp-aviso-ligar"), move || {
            t.ligar_depois_do_aviso();
            t.retomar_controle();
        })
        .close(t!("comum-cancelar"));
    Column::new().child(como).child(aviso_dialogo)
}
