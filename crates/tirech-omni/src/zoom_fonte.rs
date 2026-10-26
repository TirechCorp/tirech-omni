//! O zoom da fonte do terminal, por aba: Ctrl+roda, Ctrl+`=`/`+`, Ctrl+`-` e Ctrl+`0`.
//!
//! O tamanho de base é o das configurações (`terminal.tamanho_fonte`). Cada sessão guarda só a
//! diferença para ele (em pontos): mudar o tamanho nas configurações continua valendo para
//! todas as abas, com o zoom de cada uma por cima. O zoom vale enquanto a sessão existe (não
//! vai para o disco) e não tem relação com o "zoom do painel" (ampliar um painel do
//! `TerminalPanes`).

use std::collections::HashMap;

use tomni_ui::prelude::*;
use tomni_ui::terminal::{Session, TERMINAL_FONT_MAX, TERMINAL_FONT_MIN};

use crate::configuracoes::Configuracoes;

/// O menor tamanho da fonte, em pontos.
const MINIMO: i32 = TERMINAL_FONT_MIN as i32;
/// O maior tamanho da fonte, em pontos.
const MAXIMO: i32 = TERMINAL_FONT_MAX as i32;

/// A diferença de cada sessão para o tamanho das configurações. Quem está em zero não consta.
#[derive(Clone, Copy)]
pub struct ZoomFonte(Signal<HashMap<Session, i32>>);

impl ZoomFonte {
    /// Sem zoom em nenhuma sessão.
    pub fn novo() -> Self {
        Self(signal(HashMap::new()))
    }

    /// O tamanho da fonte da `sessao`, em pontos: a base das `config` mais a diferença, entre o
    /// mínimo e o máximo do terminal. Rastreia as configurações e o zoom.
    pub fn tamanho(&self, config: Signal<Configuracoes>, sessao: Session) -> f64 {
        let base = config.with(|c| c.terminal.tamanho_fonte);
        let diferenca = self.0.with(|m| m.get(&sessao).copied().unwrap_or(0));
        efetivo(base, diferenca) as f64
    }

    /// A diferença da `sessao` para as configurações, em pontos (sem rastrear).
    pub fn diferenca(&self, sessao: Session) -> i32 {
        self.0
            .with_untracked(|m| m.get(&sessao).copied().unwrap_or(0))
    }

    /// Soma `passos` pontos ao tamanho da `sessao` (negativo diminui), sem passar dos limites.
    pub fn mudar(&self, config: Signal<Configuracoes>, sessao: Session, passos: i32) {
        let base = config.with_untracked(|c| c.terminal.tamanho_fonte);
        let nova = proxima(base, self.diferenca(sessao), passos);
        self.guardar(sessao, nova);
    }

    /// Volta a `sessao` ao tamanho das configurações.
    pub fn restaurar(&self, sessao: Session) {
        self.guardar(sessao, 0);
    }

    /// Dá à `sessao` o mesmo zoom da `origem` (a cópia de uma aba nasce do tamanho dela).
    pub fn herdar(&self, origem: Session, sessao: Session) {
        self.guardar(sessao, self.diferenca(origem));
    }

    /// Esquece a `sessao` (ela foi fechada).
    pub fn esquecer(&self, sessao: Session) {
        self.guardar(sessao, 0);
    }

    fn guardar(&self, sessao: Session, diferenca: i32) {
        if self.diferenca(sessao) == diferenca {
            return;
        }
        self.0.update(|m| {
            if diferenca == 0 {
                m.remove(&sessao);
            } else {
                m.insert(sessao, diferenca);
            }
        });
    }
}

/// O tamanho que vale: a `base` mais a `diferenca`, dentro dos limites do terminal.
fn efetivo(base: u32, diferenca: i32) -> i32 {
    (base as i32 + diferenca).clamp(MINIMO, MAXIMO)
}

/// A diferença nova depois de `passos` pontos: parte do tamanho que está na tela (já dentro dos
/// limites), então um clique além do limite não "acumula" e o passo de volta anda na hora.
fn proxima(base: u32, diferenca: i32, passos: i32) -> i32 {
    let atual = efetivo(base, diferenca);
    let alvo = (atual + passos).clamp(MINIMO, MAXIMO);
    alvo - base as i32
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_tamanho_fica_entre_6_e_72() {
        assert_eq!(efetivo(16, 0), 16);
        assert_eq!(efetivo(16, -100), 6);
        assert_eq!(efetivo(16, 100), 72);
        // Uma base fora da faixa das configurações também é contida.
        assert_eq!(efetivo(2, 0), 6);
    }

    #[test]
    fn passar_do_limite_nao_acumula() {
        let mut d = 0;
        for _ in 0..30 {
            d = proxima(16, d, -1);
        }
        assert_eq!(efetivo(16, d), 6);
        // Um passo para cima sai do mínimo na hora.
        d = proxima(16, d, 1);
        assert_eq!(efetivo(16, d), 7);
        d = proxima(16, d, 500);
        assert_eq!(efetivo(16, d), 72);
        d = proxima(16, d, -1);
        assert_eq!(efetivo(16, d), 71);
    }

    #[test]
    fn mudar_a_base_leva_o_zoom_junto() {
        let d = proxima(16, 0, 4);
        assert_eq!(efetivo(16, d), 20);
        assert_eq!(efetivo(12, d), 16);
    }
}
