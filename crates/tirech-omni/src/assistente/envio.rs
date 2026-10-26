//! O que vai para o modelo (M52): o contexto do terminal já limpo e sem segredos, e o pedido
//! montado. É puro: a tela lê a sessão e o cofre e chama estas funções.

use tomni_ui::ai::Message;

use super::contexto::Contexto;
use super::prompts::{self, Partes};
use crate::redacao::{Contagem, Redator};

/// As partes do terminal depois da limpeza e da redação, donas do texto.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PartesPreparadas {
    /// O texto selecionado.
    pub selecao: Option<String>,
    /// As últimas linhas do terminal.
    pub linhas: Option<String>,
    /// O comando do último erro.
    pub comando: Option<String>,
    /// A saída do último erro.
    pub saida: Option<String>,
}

impl PartesPreparadas {
    /// As partes como o `prompts` as pede.
    pub fn partes(&self) -> Partes<'_> {
        Partes {
            selecao: self.selecao.as_deref(),
            linhas: self.linhas.as_deref(),
            comando: self.comando.as_deref(),
            saida: self.saida.as_deref(),
        }
    }
}

/// Limpa e redige cada parte do contexto. Devolve as partes e quantos segredos saíram.
pub fn preparar_contexto(
    contexto: &Contexto,
    redator: Option<&Redator>,
) -> (PartesPreparadas, Contagem) {
    let mut total = Contagem::default();
    let mut passa = |texto: Option<&str>| {
        texto.map(|texto| {
            let (texto, contagem) = prompts::preparar(texto, redator);
            total.somar(contagem);
            texto
        })
    };
    let selecao = passa(contexto.selecao.as_deref());
    let linhas = passa(contexto.linhas.as_deref());
    let comando = passa(
        contexto
            .ultimo_erro
            .as_ref()
            .and_then(|e| e.comando.as_deref()),
    );
    let saida = passa(contexto.ultimo_erro.as_ref().map(|e| e.saida.as_str()));
    (
        PartesPreparadas {
            selecao,
            linhas,
            comando,
            saida,
        },
        total,
    )
}

/// Um pedido pronto para sair: o que o "O que vai sair" mostra é exatamente isto.
#[derive(Clone, Debug, Default)]
pub struct Preparado {
    /// O prompt de sistema.
    pub sistema: String,
    /// A conversa, a última mensagem é a nova.
    pub mensagens: Vec<Message>,
    /// Quantos segredos saíram do texto.
    pub contagem: Contagem,
    /// Quantas linhas antigas o limite de caracteres cortou.
    pub cortadas: usize,
    /// Se o cofre estava bloqueado (só os padrões comuns foram procurados).
    pub bloqueado: bool,
}

impl Preparado {
    /// O texto das mensagens, uma depois da outra, como vai para o modelo.
    pub fn texto_exato(&self) -> String {
        self.mensagens
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::assistente::contexto::UltimoErro;

    #[test]
    fn as_partes_saem_limpas_e_sem_segredos() {
        let contexto = Contexto {
            selecao: Some("\x1b[31mfalhou\x1b[0m token=abc123xyz".into()),
            linhas: Some("linha limpa".into()),
            ultimo_erro: Some(UltimoErro {
                comando: Some("make".into()),
                saida: "erro password=hunter22".into(),
                achou: true,
                linhas: 1,
            }),
            ..Contexto::default()
        };
        let redator = Redator::so_padroes();
        let (partes, contagem) = preparar_contexto(&contexto, Some(&redator));
        assert_eq!(
            partes.selecao.as_deref(),
            Some("falhou token=[segredo removido: senha]")
        );
        assert_eq!(partes.linhas.as_deref(), Some("linha limpa"));
        assert_eq!(partes.comando.as_deref(), Some("make"));
        assert_eq!(
            partes.saida.as_deref(),
            Some("erro password=[segredo removido: senha]")
        );
        assert_eq!(contagem.total(), 2);
    }

    #[test]
    fn sem_redator_so_limpa() {
        let contexto = Contexto {
            selecao: Some("\x1b[1mok\x1b[0m token=abc".into()),
            ..Contexto::default()
        };
        let (partes, contagem) = preparar_contexto(&contexto, None);
        assert_eq!(partes.selecao.as_deref(), Some("ok token=abc"));
        assert_eq!(contagem.total(), 0);
        assert!(partes.linhas.is_none() && partes.comando.is_none() && partes.saida.is_none());
    }

    #[test]
    fn o_texto_exato_junta_as_mensagens() {
        let preparado = Preparado {
            mensagens: vec![Message::user("um"), Message::assistant("dois")],
            ..Preparado::default()
        };
        assert_eq!(preparado.texto_exato(), "um\n\ndois");
    }
}
