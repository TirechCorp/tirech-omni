//! As ações do S3 que não cabem no trait `Fs`: o link pré-assinado, as propriedades do
//! objeto, a classe de armazenamento e os envios incompletos.
//!
//! Cada ação abre uma conexão S3 própria numa thread (o `Fs` da aba é só `dyn Fs`, sem esses
//! métodos) e devolve o resultado por um [`Live`]; o efeito de [`AcoesS3::ligar`] aplica o que
//! chegou nos signals que os diálogos e a `InfoBar` leem.

use std::io;
use std::time::{Duration, SystemTime};

use tomni_ui::clipboard;
use tomni_ui::files::Live;
use tomni_ui::files::s3::{CLASSES_DE_ARMAZENAMENTO, ObjectInfo, PendingUpload, S3, S3Config};
use tomni_ui::prelude::*;

use super::{ESCOPO_DAS_ABAS, formato};
use crate::t;

/// Quanto tempo o link pré-assinado vale: o rótulo do menu e a duração.
pub fn validades() -> [(String, Duration); 3] {
    [
        (t!("arquivos-validade-hora"), Duration::from_secs(3_600)),
        (t!("arquivos-validade-dia"), Duration::from_secs(86_400)),
        (
            t!("arquivos-validade-dias"),
            Duration::from_secs(7 * 86_400),
        ),
    ]
}

/// O que uma thread de trabalho devolveu.
#[derive(Clone, Debug)]
enum Resultado {
    /// O link pronto para copiar; vale até `ate`.
    Link {
        nome: String,
        url: String,
        ate: SystemTime,
    },
    /// Os dados de um objeto.
    Objeto { caminho: String, info: ObjectInfo },
    /// A classe de armazenamento mudou.
    Classe { nome: String, classe: String },
    /// Os envios incompletos de uma pasta.
    Pendentes(Vec<PendingUpload>),
    /// Um envio foi abortado.
    Abortado(String),
    /// Todos os envios da lista foram abortados (e quantos).
    AbortadosTodos(usize),
    /// Deu erro.
    Erro(String),
}

/// As ações S3 de uma aba.
#[derive(Clone)]
pub struct AcoesS3 {
    config: S3Config,
    chegadas: Live<Vec<Resultado>>,
    /// O aviso da aba (`InfoBar`): se é erro e o texto.
    pub aviso: Signal<Option<(bool, String)>>,
    /// O diálogo das propriedades do objeto.
    pub info_aberta: Signal<bool>,
    /// O título do diálogo das propriedades.
    pub info_titulo: Signal<String>,
    /// As linhas do diálogo das propriedades, como `(rótulo, valor)`.
    pub info_linhas: Signal<Vec<(String, String)>>,
    /// O diálogo dos envios incompletos.
    pub pendentes_aberto: Signal<bool>,
    /// Os envios incompletos da pasta.
    pub pendentes: Signal<Vec<PendingUpload>>,
    /// Se a lista dos envios incompletos ainda está carregando.
    pub pendentes_carregando: Signal<bool>,
}

impl AcoesS3 {
    /// As ações de uma conexão. Crie na thread da interface.
    pub fn nova(config: S3Config) -> Self {
        ESCOPO_DAS_ABAS.with(|escopo| {
            escopo.run(|| Self {
                config,
                chegadas: Live::detached(Vec::new()),
                aviso: signal(None),
                info_aberta: signal(false),
                info_titulo: signal(String::new()),
                info_linhas: signal(Vec::new()),
                pendentes_aberto: signal(false),
                pendentes: signal(Vec::new()),
                pendentes_carregando: signal(false),
            })
        })
    }

    /// Liga o efeito que aplica as respostas. Chame uma vez, ao montar a aba.
    pub fn ligar(&self) {
        let acoes = self.clone();
        effect(move || {
            let chegou = acoes.chegadas.with(|v| !v.is_empty());
            if !chegou {
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
    }

    fn aplicar(&self, resultado: Resultado) {
        match resultado {
            Resultado::Link { nome, url, ate } => {
                clipboard::write(url);
                self.avisar(
                    false,
                    t!(
                        "arquivos-link-copiado",
                        nome = nome,
                        ate = formato::data(Some(ate))
                    ),
                );
            }
            Resultado::Objeto { caminho, info } => {
                self.info_titulo.set(caminho);
                self.info_linhas.set(linhas_do_objeto(&info));
                self.info_aberta.set(true);
            }
            Resultado::Classe { nome, classe } => self.avisar(
                false,
                t!("arquivos-classe-agora", nome = nome, classe = classe),
            ),
            Resultado::Pendentes(lista) => {
                self.pendentes_carregando.set(false);
                self.pendentes.set(lista);
            }
            Resultado::Abortado(id) => {
                self.pendentes.update(|v| v.retain(|p| p.upload_id != id));
            }
            Resultado::AbortadosTodos(quantos) => {
                self.pendentes.set(Vec::new());
                self.avisar(false, t!("arquivos-abortados", n = quantos as i64));
            }
            Resultado::Erro(motivo) => {
                self.pendentes_carregando.set(false);
                self.avisar(true, motivo);
            }
        }
    }

    fn avisar(&self, erro: bool, texto: String) {
        self.aviso.set(Some((erro, texto)));
    }

    /// Roda `trabalho` numa conexão nova, numa thread.
    fn pedir(&self, trabalho: impl FnOnce(&mut S3) -> io::Result<Resultado> + Send + 'static) {
        let (config, chegadas) = (self.config.clone(), self.chegadas.clone());
        std::thread::spawn(move || {
            let resultado = S3::connect(config)
                .and_then(|mut s3| trabalho(&mut s3))
                .unwrap_or_else(|erro| Resultado::Erro(erro.to_string()));
            chegadas.update(|v| v.push(resultado));
        });
    }

    /// Copia para a área de transferência o link que baixa `caminho` por `validade`.
    pub fn copiar_link(&self, caminho: &str, validade: Duration) {
        let (caminho, nome) = (caminho.to_owned(), nome_de(caminho));
        self.pedir(move |s3| {
            let url = s3.presign_get(&caminho, validade)?;
            Ok(Resultado::Link {
                nome,
                url,
                ate: SystemTime::now() + validade,
            })
        });
    }

    /// Abre o diálogo com os dados do objeto.
    pub fn propriedades(&self, caminho: &str) {
        let caminho = caminho.to_owned();
        self.pedir(move |s3| {
            let info = s3.head(&caminho)?;
            Ok(Resultado::Objeto { caminho, info })
        });
    }

    /// Troca a classe de armazenamento do objeto.
    pub fn trocar_classe(&self, caminho: &str, classe: &str) {
        let (caminho, classe) = (caminho.to_owned(), classe.to_owned());
        self.pedir(move |s3| {
            s3.set_storage_class(&caminho, &classe)?;
            Ok(Resultado::Classe {
                nome: nome_de(&caminho),
                classe,
            })
        });
    }

    /// Abre o diálogo e carrega os envios incompletos sob `pasta`.
    pub fn listar_pendentes(&self, pasta: &str) {
        let pasta = if pasta.is_empty() { "/" } else { pasta }.to_owned();
        self.pendentes.set(Vec::new());
        self.pendentes_carregando.set(true);
        self.pendentes_aberto.set(true);
        self.pedir(move |s3| Ok(Resultado::Pendentes(s3.pending_uploads(&pasta)?)));
    }

    /// Aborta um envio incompleto.
    pub fn abortar(&self, envio: PendingUpload) {
        self.pedir(move |s3| {
            s3.abort_upload(&envio.path, &envio.upload_id)?;
            Ok(Resultado::Abortado(envio.upload_id))
        });
    }

    /// Aborta todos os envios incompletos da lista.
    pub fn abortar_todos(&self) {
        let envios = self.pendentes.get_untracked();
        if envios.is_empty() {
            return;
        }
        self.pedir(move |s3| {
            for envio in &envios {
                s3.abort_upload(&envio.path, &envio.upload_id)?;
            }
            Ok(Resultado::AbortadosTodos(envios.len()))
        });
    }
}

/// O último pedaço do caminho.
fn nome_de(caminho: &str) -> String {
    tomni_ui::files::path::file_name(caminho).to_owned()
}

/// As linhas do diálogo das propriedades.
fn linhas_do_objeto(info: &ObjectInfo) -> Vec<(String, String)> {
    let mut linhas = vec![
        ("Tamanho".to_owned(), formato::tamanho(info.size)),
        ("Classe".to_owned(), info.storage_class.clone()),
        (
            "Tipo".to_owned(),
            info.content_type.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "ETag".to_owned(),
            info.etag.clone().unwrap_or_else(|| "—".to_owned()),
        ),
        (
            "Modificado".to_owned(),
            formato::data(info.modified.or(info.last_modified)),
        ),
    ];
    for (nome, valor) in &info.metadata {
        linhas.push((t!("arquivos-s3-meta", nome = nome.as_str()), valor.clone()));
    }
    linhas
}

/// As classes do menu de "Classe de armazenamento".
pub fn classes() -> [&'static str; CLASSES_DE_ARMAZENAMENTO.len()] {
    CLASSES_DE_ARMAZENAMENTO
}

/// O texto de um envio incompleto na lista.
pub fn descricao(envio: &PendingUpload) -> String {
    let inicio = formato::data(envio.initiated);
    if inicio.is_empty() {
        envio.path.clone()
    } else {
        t!(
            "arquivos-envio-iniciado-em",
            caminho = envio.path.as_str(),
            inicio = inicio
        )
    }
}
