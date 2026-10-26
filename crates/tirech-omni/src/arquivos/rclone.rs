//! O que o rclone tem a mais numa aba: o espaço usado no rodapé e o link público de um
//! arquivo. O resto (listar, copiar, apagar) passa pelo trait `Fs`, igual aos outros tipos.
//!
//! Como no S3, cada pedido roda numa thread (o `Fs` da aba é só `dyn Fs`) e volta por um
//! [`Live`], que o efeito de [`AcoesRclone::ligar`] aplica nos signals que a view lê.

use std::io;
use std::sync::Arc;

use tomni_ui::clipboard;
use tomni_ui::files::Live;
use tomni_ui::files::rclone::{Features, Rclone};
use tomni_ui::prelude::*;

use super::{ESCOPO_DAS_ABAS, formato};
use crate::rclone::Rcds;
use crate::t;

/// O que uma thread de trabalho devolveu.
#[derive(Clone, Debug)]
enum Resultado {
    /// O que o remoto sabe fazer e o espaço dele ("1,2 GB de 15,0 GB"; vazio: o serviço não diz).
    Remoto { features: Features, espaco: String },
    /// O link público pronto para copiar.
    Link { nome: String, url: String },
    /// Deu erro.
    Erro(String),
}

/// As ações do rclone de uma aba.
#[derive(Clone)]
pub struct AcoesRclone {
    rcds: Arc<Rcds>,
    spec: String,
    chegadas: Live<Vec<Resultado>>,
    /// O aviso da aba (`InfoBar`): se é erro e o texto.
    pub aviso: Signal<Option<(bool, String)>>,
    /// O rodapé: "{usado} de {total}" (vazio enquanto não se sabe, ou se o serviço não diz).
    pub espaco: Signal<String>,
    /// Se o serviço faz links públicos (o menu "Copiar link público").
    pub link_publico: Signal<bool>,
}

impl AcoesRclone {
    /// As ações do remoto `spec` (`gdrive:`). Crie na thread da interface.
    pub fn nova(rcds: Arc<Rcds>, spec: String) -> Self {
        ESCOPO_DAS_ABAS.with(|escopo| {
            escopo.run(|| Self {
                rcds,
                spec,
                chegadas: Live::detached(Vec::new()),
                aviso: signal(None),
                espaco: signal(String::new()),
                link_publico: signal(false),
            })
        })
    }

    /// Liga o efeito que aplica as respostas e pergunta o que o remoto sabe fazer. Chame uma
    /// vez, ao montar a aba.
    pub fn ligar(&self) {
        let acoes = self.clone();
        effect(move || {
            if !acoes.chegadas.with(|v| !v.is_empty()) {
                return;
            }
            untrack(|| {
                let mut recebidas = Vec::new();
                acoes.chegadas.update(|v| recebidas.append(v));
                for resultado in recebidas {
                    acoes.aplicar(resultado);
                }
            });
        });
        self.pedir(|rclone| {
            let features = rclone.features()?;
            let espaco = match rclone.about()? {
                Some(about) => match (about.used, about.total) {
                    (Some(usado), Some(total)) => {
                        t!(
                            "comum-parte-de-total",
                            parte = formato::tamanho(usado),
                            total = formato::tamanho(total)
                        )
                    }
                    (Some(usado), None) => {
                        t!("arquivos-rclone-usados", usado = formato::tamanho(usado))
                    }
                    _ => String::new(),
                },
                None => String::new(),
            };
            Ok(Resultado::Remoto { features, espaco })
        });
    }

    fn aplicar(&self, resultado: Resultado) {
        match resultado {
            Resultado::Remoto { features, espaco } => {
                self.link_publico.set(features.public_link);
                self.espaco.set(espaco);
            }
            Resultado::Link { nome, url } => {
                clipboard::write(url);
                self.aviso.set(Some((
                    false,
                    t!("arquivos-link-publico-copiado", nome = nome),
                )));
            }
            Resultado::Erro(motivo) => self.aviso.set(Some((true, motivo))),
        }
    }

    /// Roda `trabalho` num `Rclone` novo, numa thread.
    fn pedir(&self, trabalho: impl FnOnce(&Rclone) -> io::Result<Resultado> + Send + 'static) {
        let (rcds, spec, chegadas) = (
            Arc::clone(&self.rcds),
            self.spec.clone(),
            self.chegadas.clone(),
        );
        std::thread::spawn(move || {
            let resultado = rcds
                .obter(None)
                .map_err(io::Error::from)
                .and_then(|rcd| trabalho(&Rclone::new(rcd, &spec)))
                .unwrap_or_else(|erro| Resultado::Erro(erro.to_string()));
            chegadas.update(|v| v.push(resultado));
        });
    }

    /// Copia para a área de transferência o link público de `caminho`.
    pub fn copiar_link(&self, caminho: &str) {
        let caminho = caminho.to_owned();
        let nome = tomni_ui::files::path::file_name(&caminho).to_owned();
        self.pedir(move |rclone| {
            let url = rclone.public_link(&caminho, None)?;
            Ok(Resultado::Link { nome, url })
        });
    }
}
