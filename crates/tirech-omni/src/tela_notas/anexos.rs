//! Os anexos da página Notas (M47): a fila de gravações e leituras em segundo plano, a lista dos
//! anexos da nota aberta e o colar e soltar de arquivos e imagens.
//!
//! Cada operação roda numa thread (`anexos::gravar` e `anexos::ler` cifram e decifram em
//! fluxo) e devolve o resultado à thread da interface pelo mesmo caminho dos alarmes
//! (`relogio.rs`): a thread guarda o resultado, avisa por um `Live` e o efeito da fila roda o
//! que a tela pediu para o fim da operação.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::Instant;

use tomni_ui::dialogs;
use tomni_ui::files::Live;
use tomni_ui::prelude::*;
use tomni_ui::{ClipboardImage, TextEditorController};

use super::{INTERVALO_DE_USO, Pagina};
use crate::anexos::{self, ErroAnexo, Progresso};
use crate::arquivos::formato;
use crate::cofre::Anexo;
use crate::configuracoes::ModoNotas;
use crate::notas;
use crate::notas_imagens::EstadoImagem;
use crate::t;

/// O maior tamanho que o diálogo da imagem toma; a imagem maior rola.
const LARGURA_DO_DIALOGO: f64 = 760.0;
const ALTURA_DO_DIALOGO: f64 = 440.0;

/// As extensões de anexo que "Copiar o conteúdo" aceita além dos tipos `text/*`: chaves,
/// certificados e arquivos de configuração.
const EXTENSOES_DE_TEXTO: [&str; 17] = [
    "pem", "key", "pub", "crt", "cer", "csr", "ppk", "txt", "md", "json", "yml", "yaml", "toml",
    "conf", "ini", "sh", "ps1",
];

/// Se o conteúdo do anexo é texto que dá para copiar: o tipo começa com `text/` ou a extensão
/// é de chave, certificado ou configuração.
fn e_texto(anexo: &Anexo) -> bool {
    anexo.tipo.starts_with("text/")
        || anexo.nome.rsplit_once('.').is_some_and(|(_, extensao)| {
            EXTENSOES_DE_TEXTO.contains(&extensao.to_ascii_lowercase().as_str())
        })
}

// ---------------------------------------------------------------------------------------------
// A fila
// ---------------------------------------------------------------------------------------------

/// Uma operação em andamento: uma linha da `notas-anexo-fila`.
#[derive(Clone)]
pub(super) struct Operacao {
    pub(super) id: u64,
    rotulo: String,
    progresso: Live<Progresso>,
    cancelado: Arc<AtomicBool>,
}

type AoFim = Box<dyn FnOnce()>;

/// As operações de anexo em andamento. Crie uma vez, no escopo da tela.
pub(super) struct Fila {
    operacoes: Signal<Vec<Operacao>>,
    pendentes: Rc<RefCell<HashMap<u64, AoFim>>>,
    prontas: Live<Vec<u64>>,
    proximo: Cell<u64>,
}

impl Fila {
    /// A fila, com o efeito que tira da lista a operação que acabou e roda o que a tela pediu
    /// para o fim dela.
    pub(super) fn nova() -> Self {
        let fila = Self {
            operacoes: signal(Vec::new()),
            pendentes: Rc::default(),
            prontas: Live::new(Vec::new()),
            proximo: Cell::new(0),
        };
        let pendentes = Rc::clone(&fila.pendentes);
        let prontas = fila.prontas.clone();
        let operacoes = fila.operacoes;
        effect(move || {
            let acabadas = prontas.get();
            if acabadas.is_empty() {
                return;
            }
            untrack(|| {
                prontas.update(Vec::clear);
                for id in acabadas {
                    operacoes.update(|o| o.retain(|op| op.id != id));
                    let ao_fim = pendentes.borrow_mut().remove(&id);
                    if let Some(ao_fim) = ao_fim {
                        ao_fim();
                    }
                }
            });
        });
        fila
    }

    /// Roda `trabalho` numa thread, com uma linha na fila. Quando ele acaba, `ao_fim` roda na
    /// thread da interface com o resultado. Devolve o número da operação.
    pub(super) fn iniciar<T: Send + 'static>(
        &self,
        rotulo: String,
        trabalho: impl FnOnce(&dyn Fn(Progresso), &AtomicBool) -> Result<T, ErroAnexo> + Send + 'static,
        ao_fim: impl FnOnce(Result<T, ErroAnexo>) + 'static,
    ) -> u64 {
        let id = self.proximo.get();
        self.proximo.set(id + 1);
        let progresso = Live::detached(Progresso::default());
        let cancelado = Arc::new(AtomicBool::new(false));
        self.operacoes.update(|o| {
            o.push(Operacao {
                id,
                rotulo,
                progresso: progresso.clone(),
                cancelado: Arc::clone(&cancelado),
            })
        });
        let resultado: Arc<Mutex<Option<Result<T, ErroAnexo>>>> = Arc::default();
        let entrega = Arc::clone(&resultado);
        self.pendentes.borrow_mut().insert(
            id,
            Box::new(move || {
                let r = resultado
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                if let Some(r) = r {
                    ao_fim(r);
                }
            }),
        );
        let prontas = self.prontas.clone();
        thread::spawn(move || {
            let r = trabalho(&|p| progresso.set(p), &cancelado);
            *entrega.lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
            prontas.update(|v| v.push(id));
        });
        id
    }

    /// Liga o `cancelado` da operação `id`.
    pub(super) fn cancelar(&self, id: u64) {
        self.operacoes.with_untracked(|ops| {
            if let Some(op) = ops.iter().find(|op| op.id == id) {
                op.cancelado.store(true, Ordering::Relaxed);
            }
        });
    }

    /// Liga o `cancelado` de todas.
    pub(super) fn cancelar_tudo(&self) {
        self.operacoes.with_untracked(|ops| {
            for op in ops {
                op.cancelado.store(true, Ordering::Relaxed);
            }
        });
    }

    /// As operações em andamento, com rastreamento (para efeitos e views).
    pub(super) fn operacoes(&self) -> Vec<Operacao> {
        self.operacoes.get()
    }
}

/// Um link que a tela pôs no texto antes de a gravação acabar.
pub(super) struct Inserido {
    nota: u64,
    link: String,
}

/// Onde o link de um anexo novo entra no texto.
pub(super) enum Destino<'a> {
    /// Na seleção do editor (o ponto de colar ou de soltar).
    Cursor(&'a TextEditorController),
    /// No fim da nota, numa linha própria.
    Fim,
    /// Em lugar nenhum: só a ficha.
    SemLink,
}

/// O trecho do texto a apagar para tirar `link`: o link, mais a quebra de linha que vem
/// depois dele quando ele ocupa a linha sozinho.
fn trecho_do_link(texto: &str, link: &str) -> Option<Range<usize>> {
    let inicio = texto.find(link)?;
    let fim = inicio + link.len();
    let comeco_de_linha = inicio == 0 || texto[..inicio].ends_with('\n');
    let fim = if comeco_de_linha && texto[fim..].starts_with('\n') {
        fim + 1
    } else {
        fim
    };
    Some(inicio..fim)
}

impl Pagina {
    /// A pasta `vault/` do cofre.
    pub(super) fn vault(&self) -> PathBuf {
        self.cofre.agora(|c| c.pasta_do_cofre().join(anexos::PASTA))
    }

    /// A ficha `id` da nota aberta.
    pub(super) fn ficha(&self, id: &str) -> Option<Anexo> {
        let nota = self.carregada.get()?;
        self.cofre.agora(|c| {
            c.item(nota)
                .and_then(|i| i.anexos.iter().find(|a| a.id == id).cloned())
        })
    }

    /// Marca uso do cofre, no máximo uma vez a cada 10 s: uma cópia longa não pode deixar o
    /// bloqueio por tempo cair no meio.
    pub(super) fn usar(&self) {
        let agora = Instant::now();
        if self
            .ultimo_uso
            .get()
            .is_none_or(|t| agora.duration_since(t) >= INTERVALO_DE_USO)
        {
            self.ultimo_uso.set(Some(agora));
            self.cofre.marcar_uso();
        }
    }

    // --- Cancelar ---

    /// Cancela uma operação e tira o link que ela pôs no texto.
    pub(super) fn cancelar_operacao(&self, id: u64) {
        self.fila.cancelar(id);
        self.desfazer_link(id);
    }

    /// Cancela tudo (bloquear o cofre, fechar a janela) e tira os links das gravações.
    pub(super) fn cancelar_anexos(&self) {
        self.fila.cancelar_tudo();
        let ids: Vec<u64> = self.inseridos.borrow().keys().copied().collect();
        for id in ids {
            self.desfazer_link(id);
        }
    }

    /// Tira do texto o link da operação `id`, se ele ainda está lá.
    fn desfazer_link(&self, id: u64) {
        let Some(inserido) = self.inseridos.borrow_mut().remove(&id) else {
            return;
        };
        self.tirar_do_texto(inserido.nota, &inserido.link);
    }

    /// Apaga `link` do texto da nota `nota`: do editor, se ela está aberta, senão do cofre.
    fn tirar_do_texto(&self, nota: u64, link: &str) {
        if self.carregada.get() == Some(nota) {
            let texto = self.rascunho.get_untracked();
            let Some(trecho) = trecho_do_link(&texto, link) else {
                return;
            };
            if self.modo() == ModoNotas::Ler {
                let mut novo = texto;
                novo.replace_range(trecho, "");
                self.rascunho.set(novo);
            } else {
                self.controlador.replace(trecho, "");
            }
            return;
        }
        if self.cofre.agora(|c| c.bloqueado()) {
            return;
        }
        let mudou = self.cofre.agora(|c| {
            c.item(nota)
                .is_some_and(|i| trecho_do_link(&i.notas, link).is_some())
        });
        if !mudou {
            return;
        }
        let _ = self.cofre.mudar(|c| -> io::Result<()> {
            let Some(mut item) = c.item(nota).cloned() else {
                return Ok(());
            };
            if let Some(trecho) = trecho_do_link(&item.notas, link) {
                item.notas.replace_range(trecho, "");
                c.salvar(item)?;
            }
            Ok(())
        });
    }

    // --- Anexar ---

    /// Põe `links` (um por linha) no texto, no `destino`.
    fn inserir_links(&self, links: &str, destino: &Destino<'_>) {
        match destino {
            Destino::SemLink => {}
            Destino::Cursor(controle) if self.modo() != ModoNotas::Ler => {
                controle.replace_selection(links);
            }
            Destino::Cursor(_) | Destino::Fim => self.inserir_no_fim(links),
        }
    }

    /// Acrescenta `links` ao fim da nota, depois de uma linha em branco.
    fn inserir_no_fim(&self, links: &str) {
        let texto = self.rascunho.get_untracked();
        let antes = if texto.is_empty() || texto.ends_with("\n\n") {
            ""
        } else if texto.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        let novo = format!("{antes}{links}\n");
        if self.modo() == ModoNotas::Ler {
            self.rascunho.set(format!("{texto}{novo}"));
        } else {
            let fim = texto.len();
            self.controlador.replace(fim..fim, &novo);
        }
    }

    /// Anexa os arquivos `caminhos` à nota aberta, com o link de cada um no `destino`. Pastas
    /// não entram. Com o cofre bloqueado ou sem nota aberta, não faz nada.
    pub(super) fn anexar_arquivos(self: &Rc<Self>, caminhos: Vec<PathBuf>, destino: &Destino<'_>) {
        let Some(nota) = self.carregada.get() else {
            return;
        };
        if self.cofre.agora(|c| c.bloqueado()) {
            return;
        }
        let mut arquivos = Vec::new();
        let mut achou_pasta = false;
        for caminho in caminhos {
            if caminho.is_dir() {
                achou_pasta = true;
            } else {
                arquivos.push(caminho);
            }
        }
        if achou_pasta {
            self.falhar(t!("anexos-pastas-nao-podem"));
        }
        if arquivos.is_empty() {
            return;
        }
        let itens: Vec<(PathBuf, String, String, String)> = arquivos
            .into_iter()
            .map(|caminho| {
                let id = anexos::novo_id();
                let nome = caminho
                    .file_name()
                    .map_or_else(|| "anexo".to_owned(), |n| n.to_string_lossy().into_owned());
                let link = notas::link_do_anexo(&nome, &id);
                (caminho, id, nome, link)
            })
            .collect();
        let links: Vec<&str> = itens.iter().map(|(_, _, _, l)| l.as_str()).collect();
        self.inserir_links(&links.join("\n"), destino);
        let com_link = !matches!(destino, Destino::SemLink);
        let vault = self.vault();
        for (caminho, id, nome, link) in itens {
            let (vault, id_thread) = (vault.clone(), id.clone());
            let fim = (
                Rc::downgrade(self),
                nota,
                id,
                nome.clone(),
                com_link.then_some(link),
            );
            self.comecar_gravacao(
                t!("anexos-anexando", nome = nome.as_str()),
                move |progresso, cancelado| {
                    notas::anexar_arquivo_com_id(
                        &vault, &id_thread, &caminho, None, progresso, cancelado,
                    )
                },
                fim,
            );
        }
    }

    /// Anexa a imagem colada como `colado-AAAAMMDD-HHMMSS.png`, com o link na seleção.
    pub(super) fn anexar_imagem(
        self: &Rc<Self>,
        imagem: ClipboardImage,
        controle: &TextEditorController,
    ) {
        let Some(nota) = self.carregada.get() else {
            return;
        };
        if self.cofre.agora(|c| c.bloqueado()) {
            return;
        }
        let id = anexos::novo_id();
        let nome = notas::nome_colado_agora();
        let link = notas::link_do_anexo(&nome, &id);
        self.inserir_links(&link, &Destino::Cursor(controle));
        let vault = self.vault();
        let (id_thread, nome_thread) = (id.clone(), nome.clone());
        let fim = (Rc::downgrade(self), nota, id, nome.clone(), Some(link));
        self.comecar_gravacao(
            t!("anexos-anexando", nome = nome.as_str()),
            move |progresso, cancelado| {
                let png = imagem
                    .to_png()
                    .map_err(|e| ErroAnexo::Io(io::Error::other(e)))?;
                notas::anexar_bytes(&vault, &id_thread, &nome_thread, &png, progresso, cancelado)
            },
            fim,
        );
    }

    /// O que as gravações de anexo têm em comum: anota o link inserido, avisa a prévia que o
    /// anexo está em gravação e, no fim, põe a ficha no item (ou desfaz o link).
    fn comecar_gravacao(
        self: &Rc<Self>,
        rotulo: String,
        trabalho: impl FnOnce(&dyn Fn(Progresso), &AtomicBool) -> Result<Anexo, ErroAnexo>
        + Send
        + 'static,
        (fraca, nota, id, nome, link): (Weak<Pagina>, u64, String, String, Option<String>),
    ) {
        self.gravando.update(|g| {
            g.insert(id.clone());
        });
        let fim_id = id.clone();
        // O número da operação só existe depois de `iniciar`; o fim a descobre pela célula.
        let numero: Rc<Cell<Option<u64>>> = Rc::new(Cell::new(None));
        let numero_fim = Rc::clone(&numero);
        let op = self.fila.iniciar(rotulo, trabalho, move |resultado| {
            let Some(pagina) = fraca.upgrade() else {
                return;
            };
            pagina.gravando.update(|g| {
                g.remove(&fim_id);
            });
            pagina.fim_da_gravacao(numero_fim.get(), nota, &fim_id, &nome, resultado);
        });
        numero.set(Some(op));
        if let Some(link) = link {
            self.inseridos
                .borrow_mut()
                .insert(op, Inserido { nota, link });
        }
    }

    /// A gravação acabou: a ficha entra no item pelo id (a nota pode já não estar aberta); se
    /// falhou, o link sai do texto e o erro aparece.
    fn fim_da_gravacao(
        &self,
        op: Option<u64>,
        nota: u64,
        id: &str,
        nome: &str,
        resultado: Result<Anexo, ErroAnexo>,
    ) {
        let desfazer = |pagina: &Self| {
            if let Some(op) = op {
                pagina.desfazer_link(op);
            }
        };
        match resultado {
            Ok(ficha) => {
                if let Some(op) = op {
                    self.inseridos.borrow_mut().remove(&op);
                }
                let anexado = self.cofre.mudar(|c| c.anexar(nota, ficha));
                if let Err(erro) = anexado {
                    let _ = fs::remove_file(anexos::caminho(&self.vault(), id));
                    // O salvar falhou, mas a ficha já entrou no cofre em memória: sai também,
                    // para não apontar para um arquivo que acabou de ser apagado.
                    self.cofre.mudar(|c| c.esquecer_anexos(&[id.to_owned()]));
                    self.tirar_link_do_anexo(nota, id, nome);
                    self.falhar(t!(
                        "anexos-nao-foi-possivel-anexar-erro",
                        nome = nome,
                        erro = erro.to_string()
                    ));
                }
            }
            Err(ErroAnexo::Cancelado) => desfazer(self),
            Err(erro) => {
                desfazer(self);
                self.falhar(t!(
                    "anexos-nao-foi-possivel-anexar-frase",
                    nome = nome,
                    erro = erro.to_string()
                ));
            }
        }
    }

    /// Tira o link do anexo `id` (seja qual for o nome dele no texto) depois que a ficha não
    /// pôde ser gravada.
    fn tirar_link_do_anexo(&self, nota: u64, id: &str, nome: &str) {
        self.tirar_do_texto(nota, &notas::link_do_anexo(nome, id));
    }

    /// "Anexar arquivo…": pede o arquivo e anexa, sem link no texto.
    pub(super) fn anexar_pelo_dialogo(self: &Rc<Self>) {
        if self.carregada.get().is_none() {
            return;
        }
        let pagina = Rc::clone(self);
        dialogs::open_file()
            .remember("anexos")
            .title(t!("comum-anexar-arquivo"))
            .show(move |caminho| {
                if let Some(caminho) = caminho {
                    pagina.anexar_arquivos(vec![caminho], &Destino::SemLink);
                }
            });
    }

    // --- Os anexos já na nota ---

    /// Um link `anexo:<id>` da pré-visualização: abre o "Salvar como…" do anexo.
    pub(super) fn abrir_anexo(self: &Rc<Self>, id: &str) {
        if self.ficha(id).is_none() {
            self.falhar(t!("anexos-nao-esta-nesta-nota"));
            return;
        }
        self.salvar_anexo_como(id);
    }

    /// O clique numa imagem da pré-visualização: abre o `notas-imagem-dialogo`.
    pub(super) fn abrir_imagem(&self, id: &str) {
        self.imagem_aberta.set(Some(id.to_owned()));
        self.imagem_dialogo.set(true);
    }

    /// "Salvar como…": pede onde gravar e decifra o anexo para lá, em fluxo.
    pub(super) fn salvar_anexo_como(self: &Rc<Self>, id: &str) {
        let Some(ficha) = self.ficha(id) else {
            self.falhar(t!("anexos-nao-esta-nesta-nota"));
            return;
        };
        if !anexos::existe(&self.vault(), id) {
            self.falhar(ErroAnexo::Ausente.to_string());
            return;
        }
        let pagina = Rc::clone(self);
        dialogs::save_file()
            .remember("anexos-salvar")
            .title(t!("anexos-salvar-anexo"))
            .file_name(ficha.nome.clone())
            .show(move |destino| {
                if let Some(destino) = destino {
                    pagina.gravar_anexo(ficha, destino);
                }
            });
    }

    /// Decifra `ficha` para `destino` numa thread, com uma linha na fila.
    fn gravar_anexo(self: &Rc<Self>, ficha: Anexo, destino: PathBuf) {
        let vault = self.vault();
        let fraca = Rc::downgrade(self);
        let rotulo = t!("anexos-salvando", nome = ficha.nome.as_str());
        self.fila.iniciar(
            rotulo,
            move |progresso, cancelado| {
                anexos::ler_para_arquivo(&vault, &ficha, &destino, progresso, cancelado)
                    .map(|()| destino)
            },
            move |resultado| {
                let Some(pagina) = fraca.upgrade() else {
                    return;
                };
                match resultado {
                    Ok(destino) => {
                        pagina.informar(t!(
                            "anexos-salvo-em",
                            caminho = destino.display().to_string()
                        ));
                    }
                    Err(ErroAnexo::Cancelado) => {}
                    Err(erro) => pagina.falhar(erro.to_string()),
                }
            },
        );
    }

    /// "Copiar o conteúdo": decifra o anexo de texto para a memória e copia pelo
    /// `copiar_segredo` (com a limpeza da área de transferência).
    pub(super) fn copiar_anexo(self: &Rc<Self>, id: &str) {
        let Some(ficha) = self.ficha(id) else {
            self.falhar(t!("anexos-nao-esta-nesta-nota"));
            return;
        };
        if !anexos::existe(&self.vault(), id) {
            self.falhar(ErroAnexo::Ausente.to_string());
            return;
        }
        let vault = self.vault();
        let fraca = Rc::downgrade(self);
        let rotulo = t!("anexos-lendo", nome = ficha.nome.as_str());
        self.fila.iniciar(
            rotulo,
            move |_, cancelado| anexos::ler_para_memoria(&vault, &ficha, cancelado),
            move |resultado| {
                let Some(pagina) = fraca.upgrade() else {
                    return;
                };
                match resultado {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(texto) => {
                            pagina.cofre.copiar_segredo(texto);
                            pagina.informar(t!("anexos-conteudo-copiado"));
                        }
                        Err(_) => pagina.falhar(t!("anexos-conteudo-nao-e-texto")),
                    },
                    Err(ErroAnexo::Cancelado) => {}
                    Err(erro) => pagina.falhar(erro.to_string()),
                }
            },
        );
    }

    /// "Inserir link na nota": no cursor (ou no fim, em modo Ler).
    pub(super) fn inserir_link_do_anexo(&self, id: &str) {
        let Some(ficha) = self.ficha(id) else {
            return;
        };
        let link = notas::link_do_anexo(&ficha.nome, &ficha.id);
        if self.modo() == ModoNotas::Ler {
            self.inserir_no_fim(&link);
        } else {
            self.controlador.replace_selection(&link);
        }
    }

    /// "Renomear…": abre o diálogo com o nome atual.
    pub(super) fn pedir_renomear_anexo(&self, id: &str) {
        let Some(ficha) = self.ficha(id) else {
            return;
        };
        self.anexo_alvo.set(Some(id.to_owned()));
        self.anexo_nome.set(ficha.nome);
        self.renomear_anexo_aberto.set(true);
    }

    /// O "Renomear" do diálogo.
    pub(super) fn renomear_anexo(&self) {
        let (Some(nota), Some(id)) = (self.carregada.get(), self.anexo_alvo.get_untracked()) else {
            return;
        };
        let nome = self.anexo_nome.get_untracked();
        if nome.trim().is_empty() {
            return;
        }
        if let Err(erro) = self.cofre.mudar(|c| c.renomear_anexo(nota, &id, &nome)) {
            self.falhar(t!(
                "anexos-nao-foi-possivel-renomear",
                erro = erro.to_string()
            ));
        }
    }

    /// "Apagar": abre a confirmação.
    pub(super) fn pedir_apagar_anexo(&self, id: &str) {
        if self.ficha(id).is_none() {
            return;
        }
        self.anexo_alvo.set(Some(id.to_owned()));
        self.apagar_anexo_aberto.set(true);
    }

    /// O "Apagar" da confirmação: tira a ficha; o arquivo sai pela coleta.
    pub(super) fn apagar_anexo(&self) {
        let (Some(nota), Some(id)) = (self.carregada.get(), self.anexo_alvo.get_untracked()) else {
            return;
        };
        if let Err(erro) = self.cofre.mudar(|c| c.tirar_anexo(nota, &id)) {
            self.falhar(t!(
                "anexos-nao-foi-possivel-apagar",
                erro = erro.to_string()
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// As views
// ---------------------------------------------------------------------------------------------

/// Uma linha da lista de anexos.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Linha {
    indice: usize,
    id: String,
    nome: String,
    tamanho: u64,
    texto: bool,
    ausente: bool,
}

/// A linha da fila de operações (`notas-anexo-fila`): uma por operação, com o andamento e o
/// botão "Cancelar".
pub(super) fn fila_de_operacoes(pagina: &Rc<Pagina>) -> impl View + use<> {
    let operacoes = pagina.fila.operacoes;
    let pagina = Rc::clone(pagina);
    Column::new()
        .id("notas-anexo-fila")
        .gap(4)
        .visible(move || !operacoes.get().is_empty())
        .child(For::each(
            move || operacoes.get(),
            |op: &Operacao| op.id,
            move |op: Operacao| {
                let id = op.id;
                let rotulo = op.rotulo.clone();
                let andamento = op.progresso.clone();
                let barra = op.progresso.clone();
                let pagina = Rc::clone(&pagina);
                Row::new()
                    .gap(8)
                    .align(Align::Center)
                    .child(
                        Text::new(move || {
                            let p = andamento.get();
                            if p.total == 0 {
                                rotulo.clone()
                            } else {
                                format!(
                                    "{rotulo} {} de {}",
                                    formato::tamanho(p.feito),
                                    formato::tamanho(p.total)
                                )
                            }
                        })
                        .id(format!("notas-anexo-texto-{id}"))
                        .secondary(),
                    )
                    .child(
                        ProgressBar::new()
                            .id(format!("notas-anexo-progresso-{id}"))
                            .width(160)
                            .value(move || {
                                let p = barra.get();
                                if p.total == 0 {
                                    0.0
                                } else {
                                    p.feito as f64 / p.total as f64
                                }
                            }),
                    )
                    .child(
                        Button::new(t!("comum-cancelar"))
                            .id(format!("notas-anexo-cancelar-{id}"))
                            .on_click(move || pagina.cancelar_operacao(id)),
                    )
            },
        ))
}

/// O menu de um anexo.
fn menu_do_anexo(pagina: &Rc<Pagina>, linha: &Linha) -> MenuFlyout {
    let prefixo = format!("notas-anexo-{}", linha.indice);
    let acao = |f: fn(&Rc<Pagina>, &str)| {
        let (pagina, id) = (Rc::clone(pagina), linha.id.clone());
        move || f(&pagina, &id)
    };
    let (presente, texto) = (!linha.ausente, linha.texto);
    MenuFlyout::new()
        .id(format!("{prefixo}-menu"))
        .item(
            format!("{prefixo}-salvar"),
            t!("anexos-salvar-como"),
            acao(|p, id| p.salvar_anexo_como(id)),
        )
        .icon(Symbol::Save)
        .item_visible(presente)
        .item(
            format!("{prefixo}-copiar"),
            t!("anexos-copiar-o-conteudo"),
            acao(|p, id| p.copiar_anexo(id)),
        )
        .icon(Symbol::Copy)
        .item_visible(presente && texto)
        .item(
            format!("{prefixo}-link"),
            t!("anexos-inserir-link-na-nota"),
            acao(|p, id| p.inserir_link_do_anexo(id)),
        )
        .icon(Symbol::Link)
        .item(
            format!("{prefixo}-renomear"),
            t!("anexos-renomear-reticencias"),
            acao(|p, id| p.pedir_renomear_anexo(id)),
        )
        .icon(Symbol::Rename)
        .separator()
        .item(
            format!("{prefixo}-apagar"),
            t!("comum-apagar"),
            acao(|p, id| p.pedir_apagar_anexo(id)),
        )
        .icon(Symbol::Delete)
}

/// A lista dos anexos da nota aberta (`notas-anexos`).
pub(super) fn lista_de_anexos(pagina: &Rc<Pagina>) -> impl View + use<> {
    let linhas = signal(Vec::<Linha>::new());
    {
        let pagina = Rc::clone(pagina);
        effect(move || {
            let _ = pagina.aberta.get();
            let fichas: Vec<Anexo> = pagina.cofre.ler(|c| {
                pagina
                    .aberta
                    .get_untracked()
                    .and_then(|id| c.item(id).map(|i| i.anexos.clone()))
                    .unwrap_or_default()
            });
            let vault = pagina.vault();
            let novas: Vec<Linha> = fichas
                .iter()
                .enumerate()
                .map(|(indice, a)| Linha {
                    indice,
                    id: a.id.clone(),
                    nome: a.nome.clone(),
                    tamanho: a.tamanho,
                    texto: e_texto(a),
                    ausente: !anexos::existe(&vault, &a.id),
                })
                .collect();
            untrack(|| linhas.set(novas));
        });
    }
    let pagina = Rc::clone(pagina);
    Row::new()
        .id("notas-anexos")
        .wrap()
        .gap(4)
        .visible(move || !linhas.get().is_empty())
        .child(For::each(
            move || linhas.get(),
            |l: &Linha| l.clone(),
            move |linha: Linha| {
                let rotulo = if linha.ausente {
                    t!("anexos-rotulo-ausente", nome = linha.nome.as_str())
                } else {
                    format!("{} · {}", linha.nome, formato::tamanho(linha.tamanho))
                };
                let botao = Button::new(rotulo)
                    .id(format!("notas-anexo-{}", linha.indice))
                    .icon(Symbol::Document)
                    .flyout(menu_do_anexo(&pagina, &linha));
                if linha.ausente {
                    botao.tooltip(t!("anexos-dica-ausente"))
                } else {
                    botao
                }
            },
        ))
}

/// Os diálogos de renomear e de apagar um anexo.
pub(super) fn dialogos_de_anexo(pagina: &Rc<Pagina>) -> impl View + use<> {
    let (alvo, nome) = (pagina.anexo_alvo, pagina.anexo_nome);
    let nome_do_alvo = {
        let pagina = Rc::clone(pagina);
        move || {
            let _ = pagina.cofre.versao();
            alvo.get()
                .and_then(|id| pagina.ficha(&id))
                .map(|f| f.nome)
                .unwrap_or_default()
        }
    };
    let renomear = {
        let p = Rc::clone(pagina);
        ContentDialog::new(t!("anexos-renomear-anexo"), pagina.renomear_anexo_aberto)
            .id("notas-anexo-renomear-dialogo")
            .content(
                Edit::bind(nome)
                    .id("notas-anexo-nome")
                    .placeholder(t!("anexos-nome-do-anexo"))
                    .width(280),
            )
            .primary(t!("anexos-renomear"), move || p.renomear_anexo())
            .close(t!("comum-cancelar"))
    };
    let apagar = {
        let p = Rc::clone(pagina);
        let nome_do_alvo = nome_do_alvo.clone();
        ContentDialog::new(t!("anexos-apagar-anexo"), pagina.apagar_anexo_aberto)
            .id("notas-anexo-apagar-dialogo")
            .content(
                Text::new(move || t!("anexos-apagar-texto", nome = nome_do_alvo()))
                    .id("notas-anexo-apagar-texto")
                    .wrap(),
            )
            .primary(t!("comum-apagar"), move || p.apagar_anexo())
            .close(t!("comum-cancelar"))
    };
    Column::new().child(renomear).child(apagar)
}

/// O diálogo que mostra a imagem no tamanho original, numa rolagem nas duas direções.
pub(super) fn dialogo_da_imagem(pagina: &Rc<Pagina>) -> impl View + use<> {
    let (aberta, dialogo) = (pagina.imagem_aberta, pagina.imagem_dialogo);
    let imagens = pagina.imagens.clone();
    let nome = {
        let pagina = Rc::clone(pagina);
        move || {
            let _ = pagina.cofre.versao();
            aberta
                .get()
                .and_then(|id| untrack(|| pagina.ficha(&id)))
                .map(|f| f.nome)
                .unwrap_or_else(|| "Imagem".to_owned())
        }
    };
    let pronta = {
        let imagens = imagens.clone();
        move || {
            matches!(
                aberta.get().and_then(|id| imagens.estado(&id)),
                Some(EstadoImagem::Pronta(..))
            )
        }
    };
    let conteudo = {
        let nome = nome.clone();
        move || {
            let Some(EstadoImagem::Pronta(origem, (largura, altura))) =
                aberta.get().and_then(|id| imagens.estado(&id))
            else {
                return ScrollViewer::both();
            };
            // O diálogo cresce com a imagem até o limite; o resto rola. Os 12 px são da barra.
            ScrollViewer::both()
                .id("notas-imagem-rolagem")
                .width((f64::from(largura) + 12.0).min(LARGURA_DO_DIALOGO))
                .height((f64::from(altura) + 12.0).min(ALTURA_DO_DIALOGO))
                .child(Image::bytes(origem).alt(untrack(&nome)))
        }
    };
    let salvar = Rc::clone(pagina);
    ContentDialog::new(nome, dialogo)
        .id("notas-imagem-dialogo")
        .content(Show::when(pronta, conteudo))
        .primary(t!("anexos-salvar-como"), move || {
            if let Some(id) = aberta.get_untracked() {
                salvar.salvar_anexo_como(&id);
            }
        })
        .close(t!("comum-fechar"))
}
