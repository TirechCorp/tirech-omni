//! As imagens da pré-visualização das notas (M47).
//!
//! O link `![texto](anexo:<id>)` aponta para um anexo cifrado: o app decifra o arquivo e mede a
//! imagem numa thread, guarda o resultado num cache só na memória e a pré-visualização desenha
//! a partir dele. A página Notas e o Cofre usam o mesmo cache e a mesma função [`desenhar`].
//! Nada é baixado: só `anexo:` vira imagem, e qualquer outro endereço fica como link.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

use tomni_ui::files::Live;
use tomni_ui::image::probe;
use tomni_ui::markdown::ImageCx;
use tomni_ui::prelude::*;
use tomni_ui::{ImageSource, Widget};

use crate::anexos;
use crate::cofre::Anexo;
use crate::t;

/// Onde uma imagem de anexo está.
#[derive(Clone)]
pub enum EstadoImagem {
    /// A thread ainda decifra e mede.
    Carregando,
    /// Os bytes decifrados e o tamanho em pixels.
    Pronta(ImageSource, (u32, u32)),
    /// Os bytes não são uma imagem, ou o arquivo está corrompido.
    Falhou,
}

/// O cache das imagens decifradas, pelo `id` do anexo. Um `id` nunca muda de conteúdo, então o
/// cache só se esvazia para soltar a memória: ao trocar de nota e ao bloquear o cofre.
#[derive(Clone)]
pub struct Imagens {
    estados: Live<HashMap<String, EstadoImagem>>,
    /// Sobe a cada esvaziar: o resultado de uma thread antiga não entra no cache novo.
    geracao: Arc<AtomicU64>,
}

impl Imagens {
    /// Cache vazio. Crie uma vez, no escopo da tela.
    pub fn nova() -> Self {
        Self {
            estados: Live::new(HashMap::new()),
            geracao: Arc::default(),
        }
    }

    /// Solta todas as imagens da memória.
    pub fn esvaziar(&self) {
        self.geracao.fetch_add(1, Ordering::SeqCst);
        self.estados.update(HashMap::clear);
    }

    /// O estado do anexo `id`, com rastreamento (`None`: ainda não pedido).
    pub fn estado(&self, id: &str) -> Option<EstadoImagem> {
        self.estados.with(|m| m.get(id).cloned())
    }

    /// Pede a imagem de `ficha` (em `vault`) se ainda não foi pedida: decifra e mede numa thread.
    pub fn pedir(&self, ficha: Anexo, vault: PathBuf) {
        let novo = self.estados.with_untracked(|m| !m.contains_key(&ficha.id));
        if !novo {
            return;
        }
        let geracao = self.geracao.load(Ordering::SeqCst);
        self.estados.update(|m| {
            m.insert(ficha.id.clone(), EstadoImagem::Carregando);
        });
        let estados = self.estados.clone();
        let atual = Arc::clone(&self.geracao);
        thread::spawn(move || {
            let nunca = AtomicBool::new(false);
            let estado = match anexos::ler_para_memoria(&vault, &ficha, &nunca) {
                Ok(bytes) => match probe(&bytes) {
                    Ok(tamanho) => EstadoImagem::Pronta(ImageSource::from(bytes), tamanho),
                    Err(_) => EstadoImagem::Falhou,
                },
                Err(_) => EstadoImagem::Falhou,
            };
            estados.update(|m| {
                if atual.load(Ordering::SeqCst) == geracao {
                    m.insert(ficha.id, estado);
                }
            });
        });
    }
}

/// Procura a ficha do anexo `id` na nota que está na tela.
pub type FichaDe = Box<dyn Fn(&str) -> Option<Anexo>>;

/// O que [`desenhar`] precisa saber da tela que mostra a nota.
pub struct Fonte {
    /// A ficha do anexo `id` na nota que está na tela.
    pub ficha: FichaDe,
    /// A pasta `vault/` do cofre.
    pub vault: Box<dyn Fn() -> PathBuf>,
    /// Os anexos que ainda estão sendo gravados.
    pub gravando: Signal<BTreeSet<String>>,
    /// O clique na imagem: recebe o `id` do anexo.
    pub abrir: Box<dyn Fn(&str)>,
}

/// O que a imagem mostra agora.
enum Vista {
    Carregando,
    Pronta(ImageSource, (u32, u32)),
    Padrao,
}

/// O desenho de uma imagem do `MarkdownView::image`:
/// - `anexo:<id>` com o arquivo na máquina: `Carregando` (anel e texto alternativo), depois a
///   imagem num cartão que abre no tamanho original;
/// - `anexo:<id>` sem ficha, sem arquivo ou corrompido: o padrão, o link com o texto alternativo;
/// - qualquer outro endereço: `None`, o padrão, sem nenhuma requisição.
pub fn desenhar(imagens: &Imagens, fonte: &Rc<Fonte>, cx: ImageCx) -> Option<Box<dyn Widget>> {
    let id = cx.url.strip_prefix("anexo:")?.to_owned();
    let gravando = fonte.gravando;
    let nome = (fonte.ficha)(&id).map(|f| f.nome);
    // A gravação de uma imagem colada ainda não acabou: o anel espera, e o pedido sai quando ela
    // acaba (o efeito roda de novo quando o `id` sai de `gravando`).
    if !gravando.get_untracked().contains(&id) && !existe_na_maquina(fonte, &id) {
        return Some(cx.fallback());
    }
    {
        let imagens = imagens.clone();
        let fonte = Rc::clone(fonte);
        let id = id.clone();
        effect(move || {
            if gravando.get().contains(&id) {
                return;
            }
            untrack(|| {
                if let Some(ficha) = (fonte.ficha)(&id)
                    && existe_na_maquina(&fonte, &id)
                {
                    imagens.pedir(ficha, (fonte.vault)());
                }
            });
        });
    }
    let vista = {
        let imagens = imagens.clone();
        let id = id.clone();
        move || {
            if gravando.get().contains(&id) {
                return Vista::Carregando;
            }
            match imagens.estado(&id) {
                Some(EstadoImagem::Pronta(fonte, tamanho)) => Vista::Pronta(fonte, tamanho),
                Some(EstadoImagem::Carregando) => Vista::Carregando,
                Some(EstadoImagem::Falhou) | None => Vista::Padrao,
            }
        }
    };
    let alt = if cx.alt.trim().is_empty() {
        nome.unwrap_or_else(|| "imagem".to_owned())
    } else {
        cx.alt.clone()
    };
    let pronta = {
        let vista = vista.clone();
        let (cx_id, alt, fonte, id) = (cx.id.clone(), alt.clone(), Rc::clone(fonte), id.clone());
        move || {
            let Vista::Pronta(origem, (largura, altura)) = vista() else {
                return Card::new();
            };
            let fonte = Rc::clone(&fonte);
            let id = id.clone();
            Card::new()
                .id(cx_id.clone())
                .padding(4)
                .label(t!("notas-abrir-imagem", alt = alt.as_str()))
                .tooltip(t!("notas-abrir-no-tamanho-original"))
                .on_click(move || (fonte.abrir)(&id))
                .child(
                    Image::bytes(origem)
                        .natural_size(largura, altura)
                        .alt(alt.clone()),
                )
        }
    };
    let carregando = {
        let (cx_id, alt) = (cx.id.clone(), alt);
        move || {
            Row::new()
                .id(format!("{cx_id}-carregando"))
                .gap(8)
                .align(Align::Center)
                .child(ProgressRing::new())
                .child(Text::new(alt.clone()).secondary())
        }
    };
    let padrao = cx.clone();
    let e_pronta = {
        let vista = vista.clone();
        move || matches!(vista(), Vista::Pronta(..))
    };
    let e_carregando = {
        let vista = vista.clone();
        move || matches!(vista(), Vista::Carregando)
    };
    Some(Box::new(Column::new().child(
        Show::when(e_pronta, pronta).otherwise(move || {
            let padrao = padrao.clone();
            let (carregando, e_carregando) = (carregando.clone(), e_carregando.clone());
            Show::when(e_carregando, carregando).otherwise(move || padrao.fallback_text())
        }),
    )))
}

fn existe_na_maquina(fonte: &Fonte, id: &str) -> bool {
    anexos::existe(&(fonte.vault)(), id)
}
