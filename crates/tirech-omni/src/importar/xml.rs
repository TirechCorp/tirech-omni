//! Um leitor de XML pequeno para os arquivos de configuração: monta uma árvore de
//! [`Elemento`]s a partir dos eventos do `quick-xml`. Os arquivos são pequenos e vêm de fora
//! (dado não confiável), então a profundidade tem um limite e nada aqui entra em pânico.

use crate::t;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

/// A profundidade máxima que o leitor aceita; mais que isso é arquivo torto.
const PROFUNDIDADE_MAXIMA: usize = 64;

/// Um elemento do XML, com o nome local (sem o prefixo de namespace), os atributos e o texto.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Elemento {
    /// O nome, sem o prefixo (`mrng:Connections` vira `Connections`).
    pub nome: String,
    /// Os atributos, na ordem do arquivo, com o nome sem o prefixo.
    pub atributos: Vec<(String, String)>,
    /// O texto direto do elemento (as partes soltas entre os filhos, juntas e sem as bordas).
    pub texto: String,
    /// Os elementos filhos.
    pub filhos: Vec<Elemento>,
}

impl Elemento {
    /// O valor do atributo `nome`.
    pub fn attr(&self, nome: &str) -> Option<&str> {
        self.atributos
            .iter()
            .find(|(n, _)| n == nome)
            .map(|(_, v)| v.as_str())
    }

    /// O atributo `nome`, ou vazio.
    pub fn attr_ou_vazio(&self, nome: &str) -> &str {
        self.attr(nome).unwrap_or_default()
    }

    /// O primeiro filho de nome `nome`.
    pub fn filho(&self, nome: &str) -> Option<&Elemento> {
        self.filhos.iter().find(|f| f.nome == nome)
    }

    /// O texto do primeiro filho `nome`, ou vazio.
    pub fn texto_do_filho(&self, nome: &str) -> &str {
        self.filho(nome).map_or("", |f| f.texto.as_str())
    }
}

fn nome_local(bruto: &[u8]) -> String {
    let nome = String::from_utf8_lossy(bruto);
    match nome.rsplit_once(':') {
        Some((_, local)) => local.to_owned(),
        None => nome.into_owned(),
    }
}

fn abrir(inicio: &BytesStart<'_>) -> Result<Elemento, String> {
    let mut elemento = Elemento {
        nome: nome_local(inicio.name().as_ref()),
        ..Elemento::default()
    };
    for atributo in inicio.attributes() {
        let atributo = atributo.map_err(|e| t!("importar-xml-atributo", erro = e.to_string()))?;
        let valor = atributo
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| t!("importar-xml-atributo", erro = e.to_string()))?;
        elemento
            .atributos
            .push((nome_local(atributo.key.as_ref()), valor.into_owned()));
    }
    Ok(elemento)
}

/// Lê o documento e devolve o elemento raiz.
pub fn ler(texto: &str) -> Result<Elemento, String> {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    let mut leitor = Reader::from_str(texto);
    let mut pilha: Vec<Elemento> = Vec::new();
    let mut raiz: Option<Elemento> = None;
    let fechar =
        |pilha: &mut Vec<Elemento>, raiz: &mut Option<Elemento>, pronto: Elemento| match pilha
            .last_mut()
        {
            Some(pai) => pai.filhos.push(pronto),
            None => {
                if raiz.is_none() {
                    *raiz = Some(pronto);
                }
            }
        };
    loop {
        let evento = leitor
            .read_event()
            .map_err(|e| t!("importar-xml-ler", erro = e.to_string()))?;
        match evento {
            Event::Start(inicio) => {
                if pilha.len() >= PROFUNDIDADE_MAXIMA {
                    return Err(t!("importar-xml-aninhado"));
                }
                pilha.push(abrir(&inicio)?);
            }
            Event::Empty(inicio) => {
                let pronto = abrir(&inicio)?;
                fechar(&mut pilha, &mut raiz, pronto);
            }
            Event::End(_) => {
                let Some(mut pronto) = pilha.pop() else {
                    return Err(t!("importar-xml-fecha-sem-abrir"));
                };
                pronto.texto = pronto.texto.trim().to_owned();
                fechar(&mut pilha, &mut raiz, pronto);
            }
            Event::Text(t) => {
                if let Some(atual) = pilha.last_mut() {
                    let t = t
                        .decode()
                        .map_err(|e| t!("importar-xml-texto", erro = e.to_string()))?;
                    atual.texto.push_str(&t);
                }
            }
            Event::CData(c) => {
                if let Some(atual) = pilha.last_mut() {
                    let c = c
                        .decode()
                        .map_err(|e| t!("importar-xml-texto", erro = e.to_string()))?;
                    atual.texto.push_str(&c);
                }
            }
            Event::GeneralRef(r) => {
                if let Some(atual) = pilha.last_mut() {
                    if let Ok(Some(c)) = r.resolve_char_ref() {
                        atual.texto.push(c);
                    } else {
                        match r.decode().as_deref() {
                            Ok("amp") => atual.texto.push('&'),
                            Ok("lt") => atual.texto.push('<'),
                            Ok("gt") => atual.texto.push('>'),
                            Ok("quot") => atual.texto.push('"'),
                            Ok("apos") => atual.texto.push('\''),
                            _ => {}
                        }
                    }
                }
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if !pilha.is_empty() {
        return Err(t!("importar-xml-termina-aberto"));
    }
    raiz.ok_or_else(|| t!("importar-xml-vazio"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_atributos_texto_e_prefixo() {
        let raiz = ler(
            "<?xml version=\"1.0\"?>\n<mrng:A xmlns:mrng=\"x\" n=\"a &amp; b\">\n <B>um &lt;dois&gt; &#233;</B><C v='1'/></mrng:A>",
        )
        .unwrap();
        assert_eq!(raiz.nome, "A");
        assert_eq!(raiz.attr("n"), Some("a & b"));
        assert_eq!(raiz.texto_do_filho("B"), "um <dois> é");
        assert_eq!(raiz.filho("C").unwrap().attr("v"), Some("1"));
    }

    #[test]
    fn xml_torto_devolve_erro() {
        assert!(ler("<a><b></a>").is_err());
        assert!(ler("</a>").is_err());
        assert!(ler("<a>").is_err());
        assert!(ler("").is_err());
        let fundo = "<a>".repeat(200);
        assert!(ler(&fundo).is_err());
    }
}
