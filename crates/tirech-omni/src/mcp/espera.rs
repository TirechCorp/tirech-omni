//! A espera de `esperar` e do modo `aba` de `executar` (M51, seção 7.2): quando o que chegou na
//! aba basta. É puro: recebe o que aconteceu e a hora, e diz se espera mais ou termina. Quem
//! chama (a casca, em `tela/mcp_terminal.rs`) olha a sessão, passa o texto novo e marca o
//! próximo alarme.

use std::time::{Duration, Instant};

use regex::{Regex, RegexBuilder};

use crate::comandos::eh_prompt;
use crate::t;

/// De quanto em quanto tempo a casca olha a aba enquanto espera.
pub const INTERVALO: Duration = Duration::from_millis(100);

/// O silêncio que vale por "o comando terminou" quando o prompt voltou.
pub const SILENCIO_DO_PROMPT: Duration = Duration::from_millis(300);

/// Por que a espera acabou.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motivo {
    /// O texto (ou a expressão) apareceu.
    Texto,
    /// A aba ficou em silêncio pelo tempo pedido.
    Silencio,
    /// O prompt voltou e a saída parou.
    Prompt,
    /// O prazo acabou.
    Prazo,
    /// A aba caiu.
    Caida,
}

impl Motivo {
    /// O nome que o assistente lê.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Texto => "texto",
            Self::Silencio => "silencio",
            Self::Prompt => "prompt",
            Self::Prazo => "prazo",
            Self::Caida => "caida",
        }
    }
}

/// O que fazer agora.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Passo {
    /// Olhar de novo depois deste tempo.
    Esperar(Duration),
    /// Acabou.
    Fim(Motivo),
}

/// O que se espera.
pub struct Pedido {
    /// Um texto a esperar.
    pub texto: Option<String>,
    /// Uma expressão regular a esperar.
    pub regex: Option<Regex>,
    /// Termina depois deste tempo sem saída.
    pub silencio: Option<Duration>,
    /// Termina quando o prompt voltar (e a saída parar por [`SILENCIO_DO_PROMPT`]); os finais
    /// a mais da pessoa vão aqui.
    pub prompt: Option<Vec<String>>,
    /// O prazo; `None` espera até acontecer outra coisa ou cancelarem.
    pub prazo: Option<Duration>,
}

/// Monta a expressão de `regex`, com o erro em português.
pub fn compilar(regex: &str) -> Result<Regex, String> {
    RegexBuilder::new(regex)
        .size_limit(16 << 20)
        .build()
        .map_err(|e| t!("mcp-regex-invalida", erro = e.to_string()))
}

/// Se a linha acaba num prompt: o de [`eh_prompt`] ou um dos `finais` a mais da pessoa.
pub fn linha_de_prompt(linha: &str, finais: &[String]) -> bool {
    let linha = linha.trim_end();
    !linha.is_empty()
        && (eh_prompt(linha)
            || finais
                .iter()
                .any(|f| !f.trim().is_empty() && linha.ends_with(f.trim_end())))
}

/// Uma espera em andamento.
pub struct Espera {
    pedido: Pedido,
    inicio: Instant,
    ultima_saida: Instant,
    recebidos_no_inicio: u64,
    acumulado: String,
}

impl Espera {
    /// Começa a esperar em `agora`, com `recebidos` bytes já chegados à aba.
    pub fn nova(pedido: Pedido, agora: Instant, recebidos: u64) -> Self {
        Self {
            pedido,
            inicio: agora,
            ultima_saida: agora,
            recebidos_no_inicio: recebidos,
            acumulado: String::new(),
        }
    }

    /// Tudo o que chegou desde o início.
    pub fn saida(&self) -> &str {
        &self.acumulado
    }

    /// Olha a aba em `agora`: `novo` é o texto que chegou desde a última olhada, `recebidos` o
    /// total de bytes da sessão, `linha` a linha do cursor e `fechada` se a aba caiu.
    pub fn avancar(
        &mut self,
        agora: Instant,
        novo: &str,
        recebidos: u64,
        linha: &str,
        fechada: bool,
    ) -> Passo {
        if !novo.is_empty() {
            self.acumulado.push_str(novo);
            self.ultima_saida = agora;
        }
        let achou = self
            .pedido
            .texto
            .as_deref()
            .is_some_and(|t| self.acumulado.contains(t))
            || self
                .pedido
                .regex
                .as_ref()
                .is_some_and(|r| r.is_match(&self.acumulado));
        if achou {
            return Passo::Fim(Motivo::Texto);
        }
        if fechada {
            return Passo::Fim(Motivo::Caida);
        }
        let quieto = agora.saturating_duration_since(self.ultima_saida);
        if let Some(finais) = &self.pedido.prompt
            && recebidos > self.recebidos_no_inicio
            && quieto >= SILENCIO_DO_PROMPT
            && linha_de_prompt(linha, finais)
        {
            return Passo::Fim(Motivo::Prompt);
        }
        if self.pedido.silencio.is_some_and(|s| quieto >= s) {
            return Passo::Fim(Motivo::Silencio);
        }
        let gasto = agora.saturating_duration_since(self.inicio);
        let mut proximo = INTERVALO;
        if let Some(prazo) = self.pedido.prazo {
            if gasto >= prazo {
                return Passo::Fim(Motivo::Prazo);
            }
            proximo = proximo.min(prazo - gasto);
        }
        Passo::Esperar(proximo.max(Duration::from_millis(10)))
    }
}

/// A saída de um comando digitado na aba: sem a linha do eco do comando, no começo, e sem o
/// prompt de volta, no fim.
pub fn saida_do_comando(texto: &str, comando: &str, finais: &[String]) -> String {
    let primeira_do_comando = comando.lines().next().unwrap_or("").trim();
    let mut linhas: Vec<&str> = texto.lines().collect();
    if let Some(primeira) = linhas.first()
        && !primeira_do_comando.is_empty()
        && primeira.trim_end().ends_with(primeira_do_comando)
    {
        linhas.remove(0);
    }
    if let Some(ultima) = linhas.last()
        && linha_de_prompt(ultima, finais)
    {
        linhas.pop();
    }
    linhas.join("\n").trim_end().to_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pedido() -> Pedido {
        Pedido {
            texto: None,
            regex: None,
            silencio: None,
            prompt: None,
            prazo: Some(Duration::from_secs(5)),
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn acha_o_texto_mesmo_dividido_em_dois_pedacos() {
        let t0 = Instant::now();
        let mut e = Espera::nova(
            Pedido {
                texto: Some("pronto".into()),
                ..pedido()
            },
            t0,
            0,
        );
        assert!(matches!(
            e.avancar(t0 + ms(100), "pro", 3, "", false),
            Passo::Esperar(_)
        ));
        assert_eq!(
            e.avancar(t0 + ms(200), "nto!", 7, "", false),
            Passo::Fim(Motivo::Texto)
        );
        assert_eq!(e.saida(), "pronto!");
    }

    #[test]
    fn a_regex_casa_no_acumulado() {
        let t0 = Instant::now();
        let mut e = Espera::nova(
            Pedido {
                regex: Some(compilar(r"\d+ passed").unwrap()),
                ..pedido()
            },
            t0,
            0,
        );
        assert_eq!(
            e.avancar(t0 + ms(50), "12 passed", 9, "", false),
            Passo::Fim(Motivo::Texto)
        );
        assert!(compilar("(").is_err());
    }

    #[test]
    fn o_prazo_acaba_a_espera_e_o_ultimo_passo_cabe_nele() {
        let t0 = Instant::now();
        let mut e = Espera::nova(
            Pedido {
                prazo: Some(ms(150)),
                ..pedido()
            },
            t0,
            0,
        );
        assert_eq!(
            e.avancar(t0 + ms(100), "", 0, "", false),
            Passo::Esperar(ms(50))
        );
        assert_eq!(
            e.avancar(t0 + ms(150), "", 0, "", false),
            Passo::Fim(Motivo::Prazo)
        );
    }

    #[test]
    fn sem_prazo_espera_para_sempre() {
        let t0 = Instant::now();
        let mut e = Espera::nova(
            Pedido {
                prazo: None,
                ..pedido()
            },
            t0,
            0,
        );
        assert!(matches!(
            e.avancar(t0 + Duration::from_secs(3600), "", 0, "", false),
            Passo::Esperar(_)
        ));
    }

    #[test]
    fn o_silencio_conta_da_ultima_saida() {
        let t0 = Instant::now();
        let mut e = Espera::nova(
            Pedido {
                silencio: Some(Duration::from_secs(1)),
                ..pedido()
            },
            t0,
            0,
        );
        assert!(matches!(
            e.avancar(t0 + ms(900), "x", 1, "", false),
            Passo::Esperar(_)
        ));
        assert!(matches!(
            e.avancar(t0 + ms(1800), "", 1, "", false),
            Passo::Esperar(_)
        ));
        assert_eq!(
            e.avancar(t0 + ms(1900), "", 1, "", false),
            Passo::Fim(Motivo::Silencio)
        );
    }

    #[test]
    fn a_aba_caida_acaba_e_o_texto_vence_a_queda() {
        let t0 = Instant::now();
        let mut e = Espera::nova(pedido(), t0, 0);
        assert_eq!(
            e.avancar(t0 + ms(10), "", 0, "", true),
            Passo::Fim(Motivo::Caida)
        );
        let mut e = Espera::nova(
            Pedido {
                texto: Some("bye".into()),
                ..pedido()
            },
            t0,
            0,
        );
        assert_eq!(
            e.avancar(t0 + ms(10), "bye", 3, "", true),
            Passo::Fim(Motivo::Texto)
        );
    }

    #[test]
    fn o_prompt_so_vale_depois_de_chegar_saida_e_de_300_ms_quieto() {
        let t0 = Instant::now();
        let mut e = Espera::nova(
            Pedido {
                prompt: Some(Vec::new()),
                ..pedido()
            },
            t0,
            10,
        );
        // O prompt velho ainda está na tela e nada chegou: não vale.
        assert!(matches!(
            e.avancar(t0 + ms(500), "", 10, "ana@srv:~$ ", false),
            Passo::Esperar(_)
        ));
        // Chegou o eco e a saída; o prompt voltou, mas ainda não ficou quieto.
        assert!(matches!(
            e.avancar(t0 + ms(600), "ls\numa\n", 30, "ana@srv:~$ ", false),
            Passo::Esperar(_)
        ));
        assert_eq!(
            e.avancar(t0 + ms(900), "", 30, "ana@srv:~$ ", false),
            Passo::Fim(Motivo::Prompt)
        );
    }

    #[test]
    fn finais_de_prompt_a_mais() {
        assert!(linha_de_prompt("ana@srv ~ ❯ ", &["❯".to_owned()]));
        assert!(!linha_de_prompt("ana@srv ~ ❯ ", &[]));
        assert!(linha_de_prompt("root@srv:~# ", &[]));
        assert!(!linha_de_prompt("   ", &[]));
    }

    #[test]
    fn a_saida_do_comando_perde_o_eco_e_o_prompt() {
        let texto = "ana@srv:~$ ls -la\narquivo1\narquivo2\nana@srv:~$ ";
        assert_eq!(saida_do_comando(texto, "ls -la", &[]), "arquivo1\narquivo2");
        // Sem eco nem prompt, nada se perde.
        assert_eq!(saida_do_comando("a\nb", "ls", &[]), "a\nb");
    }
}
