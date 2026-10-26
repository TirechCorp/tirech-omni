//! Para onde vai o que se digita (M42): só o terminal com o foco, os visíveis, todos ou as abas
//! escolhidas, mais a barra de envio para vários. Cada janela tem o seu `Envio`, que mora no
//! escopo dela (junto do layout, em [`crate::janelas::Janela`]).
//!
//! Aqui ficam o estado e as funções puras; a tela da barra e o diálogo de escolher as abas
//! estão em `tela/envio.rs`.

use std::collections::BTreeSet;

use crate::t;

use tomni_ui::prelude::*;
use tomni_ui::terminal::{Broadcast, Session};

/// Quantas linhas o histórico da barra guarda.
pub const LIMITE_DO_HISTORICO: usize = 50;

/// Posição do modo "Escolhidas" entre os botões de rádio.
const ESCOLHIDAS: usize = 3;

/// O estado do envio de uma janela.
#[derive(Clone, Copy, Debug)]
pub struct Envio {
    /// Para onde vai o que se digita (entregue ao `TerminalPanes`).
    pub modo: Signal<Broadcast>,
    /// Os quatro botões de rádio do modo.
    pub nenhum: Signal<bool>,
    pub visiveis: Signal<bool>,
    pub todos: Signal<bool>,
    pub escolhidas: Signal<bool>,
    /// As chaves (`Session::key_str`) das abas escolhidas.
    pub grupo: Signal<BTreeSet<String>>,
    /// A barra de entrada está à mostra.
    pub barra: Signal<bool>,
    /// O que a barra já enviou, o mais recente primeiro.
    pub historico: Signal<Vec<String>>,
    /// O texto do campo da barra.
    pub texto: Signal<String>,
    /// Se a barra manda `Enter` no fim do texto.
    pub enter: Signal<bool>,
    /// O diálogo "Abas que recebem o envio" está aberto.
    pub dialogo: Signal<bool>,
    /// As linhas desse diálogo: uma por aba da janela.
    pub linhas: Signal<Vec<DataGridRow>>,
}

impl Envio {
    /// O envio de uma janela nova: só o terminal com o foco, a barra fechada.
    pub fn novo() -> Self {
        let envio = Self {
            modo: signal(Broadcast::None),
            nenhum: signal(true),
            visiveis: signal(false),
            todos: signal(false),
            escolhidas: signal(false),
            grupo: signal(BTreeSet::new()),
            barra: signal(false),
            historico: signal(Vec::new()),
            texto: signal(String::new()),
            enter: signal(true),
            dialogo: signal(false),
            linhas: signal(Vec::new()),
        };
        // Os botões funcionam como rádio: um ligado de cada vez. O que mudou por último ganha.
        effect(move || {
            let ligados = [
                envio.nenhum.get(),
                envio.visiveis.get(),
                envio.todos.get(),
                envio.escolhidas.get(),
            ];
            let antes = envio.modo.get_untracked();
            let indice_antes = match antes {
                Broadcast::Visible => 1,
                Broadcast::All => 2,
                Broadcast::Group(_) => ESCOLHIDAS,
                Broadcast::None => 0,
            };
            let novo = (0..ligados.len())
                .find(|&i| ligados[i] && i != indice_antes)
                .unwrap_or(indice_antes);
            let sinais = [envio.nenhum, envio.visiveis, envio.todos, envio.escolhidas];
            for (i, s) in sinais.iter().enumerate() {
                if ligados[i] != (i == novo) {
                    s.set(i == novo);
                }
            }
            if novo != indice_antes {
                envio.modo.set(match novo {
                    1 => Broadcast::Visible,
                    2 => Broadcast::All,
                    ESCOLHIDAS => {
                        Broadcast::Group(envio.grupo.get_untracked().into_iter().collect())
                    }
                    _ => Broadcast::None,
                });
            }
        });
        // Com "Escolhidas" ligado, o grupo que muda leva o modo junto. Só escreve se difere.
        effect(move || {
            let grupo: Vec<String> = envio.grupo.get().into_iter().collect();
            if let Broadcast::Group(atual) = envio.modo.get_untracked()
                && atual != grupo
            {
                envio.modo.set(Broadcast::Group(grupo));
            }
        });
        envio
    }

    /// O submenu "Enviar o que se digita para" do Exibir.
    pub fn submenu(&self) -> MenuFlyout {
        MenuFlyout::new()
            .id("envio-menu")
            .toggle(
                "envio-menu-nenhum",
                t!("envio-so-o-terminal-com-o-foco"),
                self.nenhum,
            )
            .toggle(
                "envio-menu-visiveis",
                t!("envio-os-terminais-visiveis"),
                self.visiveis,
            )
            .toggle(
                "envio-menu-todos",
                t!("envio-todos-os-terminais"),
                self.todos,
            )
            .toggle(
                "envio-menu-escolhidas",
                t!("envio-as-abas-escolhidas"),
                self.escolhidas,
            )
    }

    /// Os botões ao lado dos menus.
    pub fn botoes(&self) -> Row {
        Row::new()
            .gap(4)
            .align(Align::Center)
            .child(Text::new(t!("comum-enviar-para")).caption().secondary())
            .child(
                ToggleButton::bind(self.nenhum, t!("envio-nenhum"))
                    .id("envio-nenhum")
                    .tooltip(t!("envio-o-que-se-digita-vai-so")),
            )
            .child(
                ToggleButton::bind(self.visiveis, t!("envio-visiveis"))
                    .id("envio-visiveis")
                    .tooltip(t!("envio-o-que-se-digita-vai-para")),
            )
            .child(
                ToggleButton::bind(self.todos, t!("envio-todos"))
                    .id("envio-todos")
                    .tooltip(t!("envio-o-que-se-digita-vai-para-2")),
            )
            .child(
                ToggleButton::bind(self.escolhidas, t!("envio-escolhidas"))
                    .id("envio-escolhidas")
                    .tooltip(t!("envio-o-que-se-digita-vai-para")),
            )
    }

    /// Tira a aba do grupo (ela fechou ou saiu da janela).
    pub fn esquecer(&self, chave: &str) {
        if self.grupo.with_untracked(|g| g.contains(chave)) {
            self.grupo.update(|g| {
                g.remove(chave);
            });
        }
    }
}

/// As sessões da janela que recebem o envio: as que estão no `grupo` e não caíram, na ordem das
/// abas.
pub fn alvos(grupo: &BTreeSet<String>, sessoes: &[Session]) -> Vec<Session> {
    sessoes
        .iter()
        .copied()
        .filter(|s| grupo.contains(&s.key_str()) && !untrack(|| s.closed()))
        .collect()
}

/// O que a barra manda: o texto e, se `enter`, o Enter (`\r`) no fim.
pub fn bytes_do_envio(texto: &str, enter: bool) -> Vec<u8> {
    let mut bytes = texto.as_bytes().to_vec();
    if enter {
        bytes.push(b'\r');
    }
    bytes
}

/// Põe `texto` no começo do histórico, sem repetir e até [`LIMITE_DO_HISTORICO`]. Texto em
/// branco não entra.
pub fn lembrar(historico: &mut Vec<String>, texto: &str) {
    if texto.trim().is_empty() {
        return;
    }
    historico.retain(|h| h != texto);
    historico.insert(0, texto.to_owned());
    historico.truncate(LIMITE_DO_HISTORICO);
}

/// As sugestões do campo: o que o histórico tem com o texto digitado, sem diferenciar
/// maiúsculas, o mais recente primeiro. Com o campo vazio, nada.
pub fn sugestoes(historico: &[String], digitado: &str) -> Vec<String> {
    let busca = digitado.trim().to_lowercase();
    if busca.is_empty() {
        return Vec::new();
    }
    historico
        .iter()
        .filter(|h| h.to_lowercase().contains(&busca) && h.as_str() != digitado)
        .cloned()
        .collect()
}

/// As teclas do menu "Teclas" da barra: (id, rótulo, bytes).
pub const TECLAS: [(&str, &str, &[u8]); 6] = [
    ("multi-tecla-ctrl-c", "Ctrl+C", b"\x03"),
    ("multi-tecla-ctrl-d", "Ctrl+D", b"\x04"),
    ("multi-tecla-ctrl-z", "Ctrl+Z", b"\x1a"),
    ("multi-tecla-tab", "Tab", b"\t"),
    ("multi-tecla-esc", "Esc", b"\x1b"),
    ("multi-tecla-enter", "Enter", b"\r"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn o_enter_vai_no_fim_so_quando_pedido() {
        assert_eq!(bytes_do_envio("uptime", true), b"uptime\r");
        assert_eq!(bytes_do_envio("uptime", false), b"uptime");
        assert_eq!(bytes_do_envio("", true), b"\r");
    }

    #[test]
    fn o_historico_nao_repete_e_tem_limite() {
        let mut h = Vec::new();
        lembrar(&mut h, "ls");
        lembrar(&mut h, "uptime");
        lembrar(&mut h, "ls");
        assert_eq!(h, ["ls", "uptime"]);
        lembrar(&mut h, "   ");
        assert_eq!(h.len(), 2);
        for n in 0..80 {
            lembrar(&mut h, &format!("cmd {n}"));
        }
        assert_eq!(h.len(), LIMITE_DO_HISTORICO);
        assert_eq!(h[0], "cmd 79");
    }

    #[test]
    fn as_sugestoes_filtram_o_historico_sem_olhar_maiusculas() {
        let h = vec![
            "Uptime".to_owned(),
            "ls -la".to_owned(),
            "uptime -p".to_owned(),
        ];
        assert_eq!(sugestoes(&h, "UPT"), ["Uptime", "uptime -p"]);
        assert!(sugestoes(&h, "").is_empty());
        assert!(sugestoes(&h, "   ").is_empty());
        // O que já está digitado por inteiro não se sugere de novo.
        assert_eq!(sugestoes(&h, "Uptime"), ["uptime -p"]);
        assert!(sugestoes(&h, "zzz").is_empty());
    }

    #[test]
    fn os_alvos_sao_as_sessoes_do_grupo_na_ordem_das_abas() {
        use tomni_ui::terminal::Loopback;
        let sessoes = [(); 3].map(|()| Session::new(Loopback::new()));
        let grupo: BTreeSet<String> = [sessoes[2], sessoes[0], sessoes[1]]
            .iter()
            .map(Session::key_str)
            .collect();
        assert_eq!(alvos(&grupo, &sessoes), sessoes);

        // A aba fora do grupo não recebe (a que caiu também não: ver os testes da tela).
        let sem_a_do_meio: BTreeSet<String> = [sessoes[0], sessoes[2]]
            .iter()
            .map(Session::key_str)
            .collect();
        assert_eq!(alvos(&sem_a_do_meio, &sessoes), [sessoes[0], sessoes[2]]);
        assert!(alvos(&BTreeSet::new(), &sessoes).is_empty());
        for s in sessoes {
            s.dispose();
        }
    }

    #[test]
    fn as_teclas_tem_ids_unicos_e_os_bytes_certos() {
        let ids: BTreeSet<_> = TECLAS.iter().map(|t| t.0).collect();
        assert_eq!(ids.len(), TECLAS.len());
        assert_eq!(TECLAS[0].2, b"\x03");
        assert_eq!(TECLAS[5].2, b"\r");
    }
}
