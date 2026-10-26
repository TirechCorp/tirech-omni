//! A página Notas (M47): as notas em Markdown do cofre, com a árvore das pastas, o editor, a
//! pré-visualização, a busca na nota e o salvamento automático. Nada aqui lê a `tela.rs`: o cofre
//! e o que a casca sabe das conexões chegam pelo [`ContextoNotas`].

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io;
use std::ops::Range;
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use tomni_ui::markdown::edit::{self, Block, Change, Prefix};
use tomni_ui::markdown::{MarkdownView, highlight};
use tomni_ui::prelude::*;
use tomni_ui::{Key, KeyEvent, Mark, Modifiers, TextEditorController, Widget, request_focus};

use crate::busca::normalizar;
use crate::cofre::{Cofre, CofreCompartilhado, Item, Tipo};
use crate::configuracoes::{Configuracoes, ModoNotas};
use crate::espacos::NotasSalvas;
use crate::notas::{self, Modelo};
use crate::notas_imagens::{self, Fonte, Imagens};
use crate::paineis::PaineisDaPagina;
use crate::relogio::Alarmes;
use crate::tela_cofre::{
    contagem, contar_apagar, dialogo_desbloquear_id, maes, resumo_apagar, rotulo,
};
use crate::{t, tk};

mod anexos;
mod transferencia;

/// Quanto a nota espera, parada, antes de ser gravada no cofre.
pub const ESPERA_DA_NOTA: Duration = Duration::from_secs(1);

/// De quanto em quanto tempo, no máximo, escrever na nota conta como uso do cofre (para o
/// bloqueio por inatividade não pegar quem está digitando).
const INTERVALO_DE_USO: Duration = Duration::from_secs(10);

/// A chave da raiz, na árvore e como destino de mover.
const RAIZ: &str = "raiz";
const PREFIXO_PASTA: &str = "pasta-";
const PREFIXO_NOTA: &str = "nota-";

/// A largura inicial do painel da lista, em pixels.
const LATERAL_PADRAO: f64 = 260.0;
/// A divisão inicial entre o editor e a pré-visualização.
const DIVISOR_PADRAO: f64 = 0.5;

/// Onde a página guarda o que grava a nota pendente, para a casca chamar ao fechar a janela.
pub type AoFechar = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// O que a página lembra entre uma abertura do app e outra: a nota aberta, o modo e a divisão
/// do editor. Vai para o `estado.json`. A largura da lista é do painel (`PaineisDaPagina`).
#[derive(Clone, Copy)]
pub struct EstadoNotas {
    /// A nota aberta.
    pub nota: Signal<Option<u64>>,
    /// Editar, Dividir ou Ler.
    pub modo: Signal<ModoNotas>,
    /// A fração do corpo que o editor ocupa no modo Dividir.
    pub divisor: Signal<f64>,
}

impl EstadoNotas {
    /// O estado a partir do que o `estado.json` guardou; `modo_padrao` vale sem escolha salva.
    pub fn novo(salvas: Option<&NotasSalvas>, modo_padrao: ModoNotas) -> Self {
        let salvas = salvas.cloned().unwrap_or_default();
        Self {
            nota: signal(salvas.nota),
            modo: signal(salvas.modo.unwrap_or(modo_padrao)),
            divisor: signal(
                salvas
                    .divisor
                    .map_or(DIVISOR_PADRAO, |d| f64::from(d) / 1000.0)
                    .clamp(0.05, 0.95),
            ),
        }
    }

    /// O que gravar no `estado.json`. Lê os signals com rastreio, para o efeito de gravação da
    /// casca acompanhar cada um.
    pub fn salvas(&self) -> NotasSalvas {
        NotasSalvas {
            nota: self.nota.get(),
            modo: Some(self.modo.get()),
            lateral: None,
            divisor: Some((self.divisor.get() * 1000.0).round().max(0.0) as u32),
        }
    }
}

/// Tudo o que a página recebe da casca.
#[derive(Clone)]
pub struct ContextoNotas {
    /// O cofre.
    pub cofre: CofreCompartilhado,
    /// As configurações (o modo, a fonte e a quebra de linha do editor).
    pub config: Signal<Configuracoes>,
    /// A fila de esperas da tela, para o salvamento automático.
    pub alarmes: Alarmes,
    /// O que a página lembra.
    pub estado: EstadoNotas,
    /// Pede à página que abra a nota; ela zera ao atender.
    pub pedido: Signal<Option<u64>>,
    /// Abre um endereço (`http`, `https`, `mailto`) no sistema.
    pub abridor: Rc<dyn Fn(&str)>,
    /// A pausa antes de gravar a nota (nos testes, bem curta).
    pub espera: Duration,
    /// A página põe aqui o que grava a nota pendente, para a casca chamar ao fechar a janela.
    pub ao_fechar: AoFechar,
    /// Os painéis encaixáveis da página: a lista fica num painel com alfinete (M54).
    pub paineis: PaineisDaPagina,
}

/// A chave de uma pasta (`None` é a raiz).
fn chave_da_pasta(id: Option<u64>) -> String {
    match id {
        Some(id) => format!("{PREFIXO_PASTA}{id}"),
        None => RAIZ.to_owned(),
    }
}

fn chave_da_nota(id: u64) -> String {
    format!("{PREFIXO_NOTA}{id}")
}

fn id_da_chave(chave: &str, prefixo: &str) -> Option<u64> {
    chave.strip_prefix(prefixo)?.parse().ok()
}

/// O destino de uma chave de pasta: `Some(None)` é a raiz; `None`, uma chave que não é de pasta.
fn destino_da_chave(chave: &str) -> Option<Option<u64>> {
    if chave == RAIZ {
        return Some(None);
    }
    id_da_chave(chave, PREFIXO_PASTA).map(Some)
}

/// Separa as chaves em ids de pastas e de notas.
fn separar(chaves: &[String]) -> (Vec<u64>, Vec<u64>) {
    let pastas = chaves
        .iter()
        .filter_map(|c| id_da_chave(c, PREFIXO_PASTA))
        .collect();
    let notas = chaves
        .iter()
        .filter_map(|c| id_da_chave(c, PREFIXO_NOTA))
        .collect();
    (pastas, notas)
}

/// As chaves de `separar` na forma que `contar_apagar` do Cofre entende.
fn chaves_do_cofre(chaves: &[String]) -> Vec<String> {
    chaves
        .iter()
        .filter_map(|c| {
            id_da_chave(c, PREFIXO_PASTA)
                .map(|id| format!("pasta:{id}"))
                .or_else(|| id_da_chave(c, PREFIXO_NOTA).map(|id| format!("item:{id}")))
        })
        .collect()
}

/// O maior limite de `char` que não passa de `pos`.
fn limite(texto: &str, pos: usize) -> usize {
    let mut pos = pos.min(texto.len());
    while !texto.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// As pastas de dentro de `pai` e as notas dele, como linhas da árvore.
fn filhos_da_arvore(
    cofre: &Cofre,
    pai: Option<u64>,
    pastas: Option<&HashSet<u64>>,
) -> Vec<TreeItem> {
    let mut linhas: Vec<TreeItem> =
        cofre
            .filhas(pai)
            .into_iter()
            .filter(|p| pastas.is_none_or(|v| v.contains(&p.id)))
            .map(|p| {
                TreeItem::folder(chave_da_pasta(Some(p.id)), p.nome.clone())
                    .children(filhos_da_arvore(cofre, Some(p.id), pastas))
            })
            .collect();
    if pastas.is_some() {
        linhas.extend(
            cofre
                .itens_em(pai)
                .into_iter()
                .filter(|i| i.tipo == Tipo::Nota)
                .map(|i| TreeItem::leaf(chave_da_nota(i.id), rotulo(i)).icon(Symbol::DocumentText)),
        );
    }
    linhas
}

/// A árvore da página: a raiz "Cofre", as pastas que interessam às notas e as notas.
fn arvore_das_notas(cofre: &Cofre) -> Vec<TreeItem> {
    if cofre.bloqueado() {
        return Vec::new();
    }
    let visiveis = notas::pastas_visiveis(cofre);
    vec![
        TreeItem::folder(RAIZ, t!("comum-cofre")).children(filhos_da_arvore(
            cofre,
            None,
            Some(&visiveis),
        )),
    ]
}

/// A árvore do "Mover para…": todas as pastas, sem notas.
fn arvore_das_pastas(cofre: &Cofre) -> Vec<TreeItem> {
    vec![TreeItem::folder(RAIZ, t!("comum-cofre")).children(filhos_da_arvore(cofre, None, None))]
}

/// Os resultados do filtro: as notas cujo título ou corpo têm o texto (sem acento e sem caixa).
fn linhas_dos_resultados(cofre: &Cofre, filtro: &str) -> Vec<DataGridRow> {
    let procurado = normalizar(filtro.trim());
    if procurado.is_empty() || cofre.bloqueado() {
        return Vec::new();
    }
    let mut achadas: Vec<&Item> = cofre
        .notas()
        .filter(|n| {
            normalizar(&n.titulo).contains(&procurado) || normalizar(&n.notas).contains(&procurado)
        })
        .collect();
    achadas.sort_by_key(|n| normalizar(&n.titulo));
    achadas
        .into_iter()
        .map(|n| {
            let pasta = cofre.caminho(n.pasta).replace('/', " › ");
            let trecho = notas::trecho(&n.notas, filtro).unwrap_or_default();
            DataGridRow::new(chave_da_nota(n.id))
                .icon(Symbol::DocumentText)
                .cell(rotulo(n), CellValue::Text(rotulo(n)))
                .cell(pasta.clone(), CellValue::Text(pasta))
                .cell(trecho.clone(), CellValue::Text(trecho))
        })
        .collect()
}

/// O nível do título (`#`) da linha onde está `pos`, 0 se não é título.
fn nivel_do_titulo(texto: &str, pos: usize) -> u8 {
    let pos = limite(texto, pos);
    let inicio = texto[..pos].rfind('\n').map_or(0, |i| i + 1);
    let linha = texto[inicio..].lines().next().unwrap_or("");
    let marcas = linha.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&marcas) && linha[marcas..].starts_with(' ') {
        marcas as u8
    } else {
        0
    }
}

/// Aplica a troca no editor: o texto muda (num passo de desfazer) e a seleção vai para onde a
/// edição pede.
fn aplicar(controle: &TextEditorController, troca: &Change) {
    controle.replace(troca.range.clone(), &troca.insert);
    controle.set_selection(troca.selection.clone());
}

/// O estado da página e as ações dos botões.
struct Pagina {
    ctx: ContextoNotas,
    cofre: CofreCompartilhado,
    selecao_arvore: Signal<Option<String>>,
    abertos: Signal<BTreeSet<String>>,
    renomeando: Signal<Option<String>>,
    filtro: Signal<String>,
    resultado_sel: Signal<BTreeSet<String>>,
    titulo: Signal<String>,
    rascunho: Signal<String>,
    controlador: TextEditorController,
    status: Signal<String>,
    erro: Signal<Option<String>>,
    info: Signal<Option<String>>,
    /// A nota aberta, para a tela acompanhar (a mesma de `carregada`).
    aberta: Signal<Option<u64>>,
    /// A nota aberta, para o código que precisa do valor na hora.
    carregada: Cell<Option<u64>>,
    /// O título e o corpo como estão no cofre (ou como a página os gravou por último).
    ultimo: RefCell<(String, String)>,
    ultima_mudanca: Cell<Instant>,
    armado: Cell<bool>,
    salvando: Cell<bool>,
    ultimo_uso: Cell<Option<Instant>>,
    /// Os blocos de segredo que o usuário mostrou, por número.
    mostrados: Signal<BTreeSet<usize>>,
    // A busca na nota.
    busca_aberta: Signal<bool>,
    busca: Signal<String>,
    substituir_visivel: Signal<bool>,
    substituir: Signal<String>,
    marcas: Signal<Vec<Mark>>,
    atual: Signal<usize>,
    acertos: Signal<Vec<Range<usize>>>,
    ultima_busca: RefCell<(bool, String)>,
    // Os diálogos.
    apagar_aberto: Signal<bool>,
    apagar_chaves: Signal<Vec<String>>,
    mover_aberto: Signal<bool>,
    mover_chaves: Signal<Vec<String>>,
    mover_destino: Signal<Option<String>>,
    // Os anexos.
    /// As gravações e leituras de anexo em andamento.
    fila: anexos::Fila,
    /// Os ids dos anexos que ainda estão sendo gravados (a pré-visualização mostra "Carregando").
    gravando: Signal<BTreeSet<String>>,
    /// Os links que a página pôs no texto antes de a gravação acabar, por operação.
    inseridos: RefCell<HashMap<u64, anexos::Inserido>>,
    /// O anexo dos diálogos de renomear e de apagar.
    anexo_alvo: Signal<Option<String>>,
    anexo_nome: Signal<String>,
    renomear_anexo_aberto: Signal<bool>,
    apagar_anexo_aberto: Signal<bool>,
    /// As imagens decifradas da pré-visualização.
    imagens: Imagens,
    /// A imagem do `notas-imagem-dialogo` e se ele está aberto.
    imagem_aberta: Signal<Option<String>>,
    imagem_dialogo: Signal<bool>,
}

impl Pagina {
    fn modo(&self) -> ModoNotas {
        self.ctx.estado.modo.get_untracked()
    }

    fn informar(&self, mensagem: impl Into<String>) {
        self.erro.set(None);
        self.info.set(Some(mensagem.into()));
    }

    fn falhar(&self, mensagem: impl Into<String>) {
        self.info.set(None);
        self.erro.set(Some(mensagem.into()));
    }

    fn palavras(&self) -> String {
        let n = notas::contar_palavras(&self.rascunho.get_untracked());
        contagem(n, tk!("comum-n-palavras"))
    }

    fn status_salvo(&self) {
        self.status
            .set(t!("notas-salvo-palavras", palavras = self.palavras()));
    }

    // --- Abrir e fechar a nota ---

    /// Abre a nota `id`: grava a que estava aberta e mostra a nova. `false` se `id` não é uma
    /// nota do cofre.
    fn abrir(&self, id: u64) -> bool {
        if self.carregada.get() == Some(id) {
            return true;
        }
        self.salvar_agora();
        self.carregar(id)
    }

    /// Mostra a nota `id` no editor, sem gravar nada.
    fn carregar(&self, id: u64) -> bool {
        let item = self
            .cofre
            .agora(|c| c.item(id).filter(|i| i.tipo == Tipo::Nota).cloned());
        let Some(item) = item else {
            self.descarregar();
            return false;
        };
        // Enquanto os dois campos mudam, o efeito do salvamento não pode achar que há mudança.
        self.carregada.set(None);
        *self.ultimo.borrow_mut() = (item.titulo.clone(), item.notas.clone());
        self.titulo.set(item.titulo.clone());
        self.rascunho.set(item.notas.clone());
        self.controlador.clear_history();
        self.controlador.set_selection(0..0);
        self.mostrados.set(BTreeSet::new());
        self.imagens.esvaziar();
        self.erro.set(None);
        self.info.set(None);
        self.carregada.set(Some(id));
        self.aberta.set(Some(id));
        self.ctx.estado.nota.set(Some(id));
        self.status_salvo();
        self.mostrar_na_arvore(&item);
        true
    }

    /// Escolhe a nota na árvore, abrindo as pastas dela.
    fn mostrar_na_arvore(&self, item: &Item) {
        let mut abrir: Vec<String> = self.cofre.agora(|c| {
            let mut v: Vec<String> = maes(c, item.pasta)
                .into_iter()
                .map(|m| chave_da_pasta(Some(m)))
                .collect();
            if item.pasta.is_some() {
                v.push(chave_da_pasta(item.pasta));
            }
            v
        });
        abrir.push(RAIZ.to_owned());
        self.abertos.update(|a| a.extend(abrir));
        let chave = chave_da_nota(item.id);
        if self.selecao_arvore.get_untracked().as_ref() != Some(&chave) {
            self.selecao_arvore.set(Some(chave));
        }
    }

    /// Fecha a nota sem gravar: ela saiu do cofre, ou o cofre foi bloqueado.
    fn descarregar(&self) {
        self.carregada.set(None);
        self.aberta.set(None);
        self.ctx.estado.nota.set(None);
        self.esvaziar();
    }

    fn esvaziar(&self) {
        *self.ultimo.borrow_mut() = (String::new(), String::new());
        self.titulo.set(String::new());
        self.rascunho.set(String::new());
        self.controlador.clear_history();
        self.mostrados.set(BTreeSet::new());
        self.imagens.esvaziar();
        self.status.set(String::new());
    }

    /// Antes de o cofre bloquear: grava a nota e esvazia o editor (o texto não fica na tela nem
    /// na memória do editor). A nota continua lembrada, para voltar ao desbloquear.
    fn ao_bloquear(&self) {
        self.cancelar_anexos();
        self.salvar_agora();
        self.imagem_dialogo.set(false);
        self.carregada.set(None);
        self.aberta.set(None);
        self.esvaziar();
        self.busca_aberta.set(false);
        self.erro.set(None);
        self.info.set(None);
    }

    // --- Salvar ---

    /// O efeito do salvamento: o título ou o corpo mudou.
    fn alterado(self: &Rc<Self>, titulo: &str, corpo: &str) {
        if self.carregada.get().is_none() {
            return;
        }
        {
            let ultimo = self.ultimo.borrow();
            if ultimo.0 == titulo && ultimo.1 == corpo {
                return;
            }
        }
        self.status.set(t!("notas-salvando"));
        self.ultima_mudanca.set(Instant::now());
        let agora = Instant::now();
        if self
            .ultimo_uso
            .get()
            .is_none_or(|t| agora.duration_since(t) >= INTERVALO_DE_USO)
        {
            self.ultimo_uso.set(Some(agora));
            self.cofre.marcar_uso();
        }
        self.armar();
    }

    /// Marca o alarme que grava a nota depois da pausa.
    fn armar(self: &Rc<Self>) {
        if self.armado.replace(true) {
            return;
        }
        self.agendar(self.ctx.espera);
    }

    fn agendar(self: &Rc<Self>, depois: Duration) {
        let fraca: Weak<Pagina> = Rc::downgrade(self);
        self.ctx.alarmes.depois(depois, move || {
            let Some(pagina) = fraca.upgrade() else {
                return;
            };
            let parada = pagina.ultima_mudanca.get().elapsed();
            if parada < pagina.ctx.espera {
                pagina.agendar(pagina.ctx.espera - parada);
            } else {
                pagina.armado.set(false);
                pagina.salvar_agora();
            }
        });
    }

    /// Grava a nota aberta no cofre, se o título ou o corpo mudou.
    fn salvar_agora(&self) {
        let Some(id) = self.carregada.get() else {
            return;
        };
        let titulo = self.titulo.get_untracked();
        let corpo = self.rascunho.get_untracked();
        let anterior = self.ultimo.borrow().clone();
        let mudou_titulo = titulo != anterior.0;
        let mudou_corpo = corpo != anterior.1;
        if !mudou_titulo && !mudou_corpo {
            return;
        }
        let novo_titulo = titulo.trim().to_owned();
        let trocar_titulo = mudou_titulo && !novo_titulo.is_empty();
        *self.ultimo.borrow_mut() = (titulo, corpo.clone());
        self.salvando.set(true);
        let resultado = self.cofre.mudar(|c| -> io::Result<()> {
            let Some(mut item) = c.item(id).cloned() else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    t!("notas-a-nota-nao-existe-mais"),
                ));
            };
            if trocar_titulo {
                item.titulo = novo_titulo.clone();
            }
            if mudou_corpo {
                item.notas = corpo.clone();
            }
            c.salvar(item).map(|_| ())
        });
        self.salvando.set(false);
        match resultado {
            Ok(()) => self.status_salvo(),
            Err(erro) => {
                *self.ultimo.borrow_mut() = anterior;
                self.status
                    .set(t!("notas-nao-foi-possivel-salvar", erro = erro.to_string()));
            }
        }
    }

    /// O cofre mudou (por esta página ou por fora): confere se a nota aberta ainda existe e se
    /// o texto dela mudou.
    fn cofre_mudou(&self) {
        if self.salvando.get() || self.cofre.agora(|c| c.bloqueado()) {
            return;
        }
        match self.carregada.get() {
            None => {
                let Some(id) = self.ctx.estado.nota.get_untracked() else {
                    return;
                };
                if !self.carregar(id) {
                    self.ctx.estado.nota.set(None);
                }
            }
            Some(id) => {
                let item = self
                    .cofre
                    .agora(|c| c.item(id).filter(|i| i.tipo == Tipo::Nota).cloned());
                let Some(item) = item else {
                    self.descarregar();
                    self.informar(t!("notas-a-nota-foi-apagada"));
                    return;
                };
                let (titulo, corpo) = self.ultimo.borrow().clone();
                let igual = (item.titulo.trim().is_empty() || item.titulo == titulo.trim())
                    && item.notas == corpo;
                let pendente =
                    self.titulo.get_untracked() != titulo || self.rascunho.get_untracked() != corpo;
                if igual || pendente {
                    return;
                }
                // Mudou por fora (outra janela, importação): mostra o texto novo, no mesmo lugar.
                let selecao = self.controlador.selection();
                self.carregada.set(None);
                *self.ultimo.borrow_mut() = (item.titulo.clone(), item.notas.clone());
                self.titulo.set(item.titulo.clone());
                self.rascunho.set(item.notas.clone());
                self.carregada.set(Some(id));
                let fim = item.notas.len();
                let a = limite(&item.notas, selecao.start.min(fim));
                let b = limite(&item.notas, selecao.end.min(fim));
                self.controlador.set_selection(a..b);
                self.status_salvo();
            }
        }
    }

    // --- Criar, renomear, mover, apagar ---

    /// A pasta que "Nova nota" usa: a escolhida na árvore, ou a pasta da nota escolhida.
    fn pasta_da_selecao(&self) -> Option<u64> {
        let chave = self.selecao_arvore.get_untracked()?;
        if let Some(destino) = destino_da_chave(&chave) {
            return destino;
        }
        let id = id_da_chave(&chave, PREFIXO_NOTA)?;
        self.cofre.agora(|c| c.item(id).and_then(|i| i.pasta))
    }

    fn nova_nota(&self, modelo: Modelo) {
        let pasta = self.pasta_da_selecao();
        self.nova_nota_em(pasta, modelo);
    }

    fn nova_nota_em(&self, pasta: Option<u64>, modelo: Modelo) {
        self.filtro.set(String::new());
        self.salvar_agora();
        let corpo = modelo.corpo();
        let resultado = self
            .cofre
            .mudar(|c| c.nova_nota(pasta, &modelo.titulo(), &corpo.texto));
        match resultado {
            Ok(id) => {
                if self.carregar(id) {
                    let cursor = limite(&corpo.texto, corpo.cursor);
                    self.controlador.set_selection(cursor..cursor);
                    self.renomeando.set(Some(chave_da_nota(id)));
                }
            }
            Err(erro) => self.falhar(t!(
                "notas-nao-consegui-criar-a-nota",
                erro = erro.to_string()
            )),
        }
    }

    fn nova_pasta_em(&self, pai: Option<u64>) {
        self.filtro.set(String::new());
        match self
            .cofre
            .mudar(|c| c.nova_pasta(pai, &t!("comum-nova-pasta")))
        {
            Ok(id) => {
                let mut abrir = vec![RAIZ.to_owned()];
                abrir.extend(
                    self.cofre
                        .agora(|c| maes(c, Some(id)))
                        .into_iter()
                        .map(|m| chave_da_pasta(Some(m))),
                );
                self.abertos.update(|a| a.extend(abrir));
                let chave = chave_da_pasta(Some(id));
                self.selecao_arvore.set(Some(chave.clone()));
                self.renomeando.set(Some(chave));
            }
            Err(erro) => self.falhar(t!(
                "notas-nao-consegui-criar-a-pasta",
                erro = erro.to_string()
            )),
        }
    }

    fn nova_pasta(&self) {
        let pasta = self.pasta_da_selecao();
        self.nova_pasta_em(pasta);
    }

    /// O nome novo, do campo da árvore: pasta ou nota.
    fn renomear(&self, chave: &str, texto: &str) {
        self.erro.set(None);
        if let Some(id) = id_da_chave(chave, PREFIXO_PASTA) {
            if let Err(erro) = self.cofre.mudar(|c| c.renomear_pasta(id, texto)) {
                self.falhar(erro.to_string());
            }
            return;
        }
        let Some(id) = id_da_chave(chave, PREFIXO_NOTA) else {
            return;
        };
        let novo = texto.trim();
        if novo.is_empty() {
            return;
        }
        if self.carregada.get() == Some(id) {
            self.titulo.set(novo.to_owned());
            self.salvar_agora();
            return;
        }
        let resultado = self.cofre.mudar(|c| -> io::Result<()> {
            let Some(mut item) = c.item(id).cloned() else {
                return Ok(());
            };
            item.titulo = novo.to_owned();
            c.salvar(item).map(|_| ())
        });
        if let Err(erro) = resultado {
            self.falhar(t!("notas-nao-consegui-renomear", erro = erro.to_string()));
        }
    }

    fn duplicar(&self, id: u64) {
        self.salvar_agora();
        let Some(mut item) = self.cofre.agora(|c| c.item(id).cloned()) else {
            return;
        };
        item.id = 0;
        item.titulo = t!("comum-titulo-copia", titulo = item.titulo.as_str());
        match self.cofre.mudar(|c| c.salvar(item)) {
            Ok(novo) => {
                self.filtro.set(String::new());
                if self.carregar(novo) {
                    self.renomeando.set(Some(chave_da_nota(novo)));
                }
            }
            Err(erro) => self.falhar(t!("notas-nao-consegui-duplicar", erro = erro.to_string())),
        }
    }

    fn pedir_apagar(&self, chaves: Vec<String>) {
        let chaves: Vec<String> = chaves.into_iter().filter(|c| c != RAIZ).collect();
        if chaves.is_empty() {
            return;
        }
        self.apagar_chaves.set(chaves);
        self.apagar_aberto.set(true);
    }

    /// Se a nota `id` sai junto com `pastas` (está numa delas, ou embaixo).
    fn sai_com(&self, id: u64, pastas: &[u64]) -> bool {
        self.cofre.agora(|c| {
            let Some(item) = c.item(id) else {
                return false;
            };
            item.pasta.is_some_and(|p| {
                pastas.contains(&p) || maes(c, Some(p)).iter().any(|m| pastas.contains(m))
            })
        })
    }

    fn apagar(&self) {
        let chaves = self.apagar_chaves.get_untracked();
        let (pastas, itens) = separar(&chaves);
        if let Some(aberta) = self.carregada.get()
            && (itens.contains(&aberta) || self.sai_com(aberta, &pastas))
        {
            // A nota aberta vai embora: nada de gravá-la de novo depois.
            self.descarregar();
        }
        match self.cofre.mudar(|c| c.apagar(&pastas, &itens)) {
            Ok(()) => {
                if self
                    .selecao_arvore
                    .get_untracked()
                    .is_some_and(|c| chaves.contains(&c))
                {
                    self.selecao_arvore.set(Some(RAIZ.to_owned()));
                }
            }
            Err(erro) => self.falhar(t!("notas-nao-consegui-apagar", erro = erro.to_string())),
        }
    }

    fn pedir_mover(&self, chaves: Vec<String>) {
        let chaves: Vec<String> = chaves.into_iter().filter(|c| c != RAIZ).collect();
        if chaves.is_empty() {
            return;
        }
        self.mover_chaves.set(chaves);
        self.mover_destino.set(Some(RAIZ.to_owned()));
        self.mover_aberto.set(true);
    }

    /// Leva as linhas `chaves` para `destino` (`raiz` ou `pasta-<id>`).
    fn mover(&self, chaves: &[String], destino: &str) {
        self.erro.set(None);
        let Some(destino) = destino_da_chave(destino) else {
            return;
        };
        let (pastas, itens) = separar(chaves);
        if pastas.is_empty() && itens.is_empty() {
            return;
        }
        if let Err(erro) = self.cofre.mudar(|c| c.mover(&pastas, &itens, destino)) {
            self.falhar(erro.to_string());
        }
    }

    /// Se `alvo` aceita as linhas `chaves`: a raiz, ou uma pasta que não esteja entre as
    /// arrastadas nem embaixo delas.
    fn aceita(&self, chaves: &[String], alvo: &str) -> bool {
        let Some(destino) = destino_da_chave(alvo) else {
            return false;
        };
        let (pastas, _) = separar(chaves);
        self.cofre.agora(|c| {
            pastas.iter().all(|&p| {
                destino != Some(p)
                    && !destino.is_some_and(|d| c.conteudo_de_pasta(p).0.contains(&d))
            })
        })
    }

    // --- Formatação ---

    /// Aplica a edição que `f` calcula sobre o texto e a seleção do editor, e devolve o foco a
    /// ele. Não faz nada sem nota aberta ou no modo Ler.
    fn formatar(&self, f: impl FnOnce(&str, Range<usize>) -> Option<Change>) {
        if self.carregada.get().is_none() || self.modo() == ModoNotas::Ler {
            return;
        }
        let (texto, selecao) = untrack(|| (self.controlador.value(), self.controlador.selection()));
        if let Some(troca) = f(&texto, selecao) {
            aplicar(&self.controlador, &troca);
        }
        self.controlador.focus();
    }

    fn envolver(&self, marca: &'static str) {
        self.formatar(|t, s| edit::toggle_wrap(t, s, marca));
    }

    fn prefixo(&self, prefixo: Prefix) {
        self.formatar(|t, s| edit::toggle_prefix(t, s, prefixo));
    }

    /// O título de nível `nivel` na linha do cursor; 0 tira o título ("Texto normal").
    fn nivel(&self, nivel: u8) {
        self.formatar(|t, s| {
            let atual = nivel_do_titulo(t, s.start);
            let alvo = if nivel == 0 { atual } else { nivel };
            if alvo == 0 {
                return None;
            }
            edit::toggle_prefix(t, s, Prefix::Heading(alvo))
        });
    }

    fn bloco_de_codigo(&self, linguagem: &'static str) {
        self.formatar(|t, s| edit::surround(t, s, Block::Code(linguagem)));
    }

    fn link(&self) {
        self.formatar(|t, s| edit::surround(t, s, Block::Link));
    }

    fn tabela(&self) {
        self.formatar(|t, s| edit::insert_block(t, s.end, Block::Table { cols: 3, rows: 2 }));
    }

    fn regua(&self) {
        self.formatar(|t, s| edit::insert_block(t, s.end, Block::Rule));
    }

    /// Muda o modo de exibição.
    fn mudar_modo(&self, modo: ModoNotas) {
        self.ctx.estado.modo.set(modo);
    }

    /// Ctrl+Shift+P: Editar e Ler se alternam (Dividir vai para Ler).
    fn alternar_leitura(&self) {
        self.mudar_modo(if self.modo() == ModoNotas::Ler {
            ModoNotas::Editar
        } else {
            ModoNotas::Ler
        });
    }

    // --- Busca na nota ---

    fn abrir_busca(&self, com_substituir: bool) {
        if self.carregada.get().is_none() {
            return;
        }
        if self.modo() == ModoNotas::Ler {
            self.mudar_modo(ModoNotas::Dividir);
        }
        if !self.busca_aberta.get_untracked() {
            let selecao = untrack(|| (self.controlador.value(), self.controlador.selection()));
            let (texto, faixa) = selecao;
            if !faixa.is_empty()
                && let Some(trecho) = texto.get(faixa)
                && !trecho.contains('\n')
            {
                self.busca.set(trecho.to_owned());
            }
        }
        self.busca_aberta.set(true);
        if com_substituir {
            self.substituir_visivel.set(true);
        }
        request_focus("notas-busca");
    }

    fn fechar_busca(&self) {
        self.busca_aberta.set(false);
        self.substituir_visivel.set(false);
        self.controlador.focus();
    }

    /// Vai ao acerto `indice`: seleciona e rola até ele.
    fn ir_para(&self, indice: usize) {
        let acertos = self.acertos.get_untracked();
        let Some(faixa) = acertos.get(indice).cloned() else {
            return;
        };
        self.atual.set(indice);
        self.controlador.set_selection(faixa.clone());
        self.controlador.scroll_to(faixa.start);
    }

    /// Vai ao acerto seguinte (`+1`) ou anterior (`-1`), dando a volta.
    fn passar(&self, passo: isize) {
        let n = self.acertos.get_untracked().len();
        if n == 0 {
            return;
        }
        let atual = self.atual.get_untracked() as isize;
        let novo = (atual + passo).rem_euclid(n as isize) as usize;
        self.ir_para(novo);
    }

    /// Refaz os acertos e as marcas do editor. Quando o texto procurado (ou a barra) mudou, vai
    /// ao primeiro acerto a partir do cursor.
    fn refazer_busca(&self) {
        let aberta = self.busca_aberta.get();
        let termo = self.busca.get();
        let _ = self.rascunho.get();
        let atual = self.atual.get();
        untrack(|| {
            if !aberta || termo.is_empty() || self.carregada.get().is_none() {
                self.acertos.set(Vec::new());
                self.marcas.set(Vec::new());
                *self.ultima_busca.borrow_mut() = (aberta, termo);
                return;
            }
            let achados = self.controlador.find(&termo, true);
            let mudou = *self.ultima_busca.borrow() != (aberta, termo.clone());
            *self.ultima_busca.borrow_mut() = (aberta, termo);
            let indice = if achados.is_empty() {
                0
            } else if mudou {
                let cursor = self.controlador.selection().start;
                achados.iter().position(|r| r.start >= cursor).unwrap_or(0)
            } else {
                atual.min(achados.len() - 1)
            };
            self.marcas.set(
                achados
                    .iter()
                    .enumerate()
                    .map(|(i, r)| Mark {
                        range: r.clone(),
                        current: i == indice,
                    })
                    .collect(),
            );
            self.acertos.set(achados.clone());
            if mudou && !achados.is_empty() {
                self.atual.set(indice);
                let faixa = achados[indice].clone();
                self.controlador.set_selection(faixa.clone());
                self.controlador.scroll_to(faixa.start);
            } else if self.atual.get_untracked() != indice {
                self.atual.set(indice);
            }
        });
    }

    fn substituir_um(&self) {
        let acertos = self.acertos.get_untracked();
        let Some(faixa) = acertos.get(self.atual.get_untracked()).cloned() else {
            return;
        };
        let novo = self.substituir.get_untracked();
        self.controlador.replace(faixa.clone(), &novo);
        let fim = faixa.start + novo.len();
        self.controlador.set_selection(fim..fim);
    }

    fn substituir_todos(&self) {
        let acertos = self.acertos.get_untracked();
        let (Some(primeiro), Some(ultimo)) = (acertos.first(), acertos.last()) else {
            return;
        };
        let novo = self.substituir.get_untracked();
        let texto = untrack(|| self.controlador.value());
        let mut trecho = String::new();
        let mut cursor = primeiro.start;
        for faixa in &acertos {
            trecho.push_str(&texto[cursor..faixa.start]);
            trecho.push_str(&novo);
            cursor = faixa.end;
        }
        let total = acertos.len();
        let faixa = primeiro.start..ultimo.end;
        self.controlador.replace(faixa, &trecho);
        self.informar(t!("cofre-ocorrencias-substituidas", n = total as i64));
    }

    // --- Teclas do editor ---

    /// As teclas do editor: os atalhos da barra, o Enter que continua a lista e o Tab que recua.
    fn tecla(self: &Rc<Self>, tecla: &KeyEvent, controle: &TextEditorController) -> bool {
        let m = tecla.modifiers;
        let comando = m.command() && !m.alt;
        let caractere = match tecla.key {
            Key::Char(c) => Some(c.to_ascii_lowercase()),
            _ => None,
        };
        if comando && !m.shift {
            match caractere {
                Some('b') => self.envolver("**"),
                Some('i') => self.envolver("*"),
                Some('e') => self.envolver("`"),
                Some('k') => self.link(),
                Some('s') => {
                    self.armado.set(false);
                    self.salvar_agora();
                }
                Some('f') => self.abrir_busca(false),
                Some('h') => self.abrir_busca(true),
                Some('n') => self.nova_nota(Modelo::Branco),
                Some('1') => self.nivel(1),
                Some('2') => self.nivel(2),
                Some('3') => self.nivel(3),
                _ => return false,
            }
            return true;
        }
        if comando && m.shift {
            match caractere {
                Some('x') => self.envolver("~~"),
                Some('8' | '*') => self.prefixo(Prefix::Bullet),
                Some('7' | '&') => self.prefixo(Prefix::Numbered),
                Some('9' | '(') => self.prefixo(Prefix::Task),
                Some('.' | '>') => self.prefixo(Prefix::Quote),
                Some('e') => self.bloco_de_codigo(""),
                Some('s') => self.bloco_de_codigo("segredo"),
                Some('p') => self.alternar_leitura(),
                _ => return false,
            }
            return true;
        }
        let texto = controle.value();
        let selecao = controle.selection();
        let troca = match tecla.key {
            Key::Escape if self.busca_aberta.get_untracked() => {
                self.fechar_busca();
                return true;
            }
            Key::Enter if !m.shift && !m.ctrl && selecao.is_empty() => {
                edit::continue_list(&texto, selecao.end)
            }
            Key::Tab if !m.ctrl && !m.alt => edit::indent(&texto, selecao, m.shift),
            _ => None,
        };
        match troca {
            Some(troca) => {
                aplicar(controle, &troca);
                true
            }
            None => false,
        }
    }

    // --- A pré-visualização ---

    /// Marcou ou desmarcou a tarefa que começa em `byte`.
    fn tarefa(&self, byte: usize) {
        let atual = self.rascunho.get_untracked();
        let Some(troca) = edit::toggle_task(&atual, byte) else {
            return;
        };
        if self.modo() == ModoNotas::Ler {
            self.rascunho.set(troca.apply(&atual));
        } else {
            aplicar(&self.controlador, &troca);
        }
    }

    /// Um link da pré-visualização.
    fn abrir_link(self: &Rc<Self>, destino: &str) {
        if let Some(uid) = destino.strip_prefix("nota:") {
            let id = self.cofre.agora(|c| c.item_por_uid(uid).map(|i| i.id));
            match id {
                Some(id) if self.abrir(id) => {}
                _ => self.falhar(t!("notas-link-fora-do-cofre")),
            }
            return;
        }
        if let Some(anexo) = destino.strip_prefix("anexo:") {
            self.abrir_anexo(anexo);
            return;
        }
        (self.ctx.abridor)(destino);
    }
}

/// Quantos blocos de segredo o texto da prévia tem de largura mínima: o editor e a prévia no
/// modo Dividir não encolhem abaixo disto.
const LARGURA_MINIMA_DO_PAINEL: f64 = 160.0;

/// A página Notas. `ctx.estado.nota` é a nota que abre sozinha quando o cofre está aberto.
pub fn pagina_notas(ctx: ContextoNotas) -> impl View {
    let cofre = ctx.cofre.clone();
    let estado = ctx.estado;
    let contexto_paineis = ctx.paineis.clone();
    let pagina = Rc::new(Pagina {
        cofre: cofre.clone(),
        selecao_arvore: signal(None),
        abertos: signal(BTreeSet::from([RAIZ.to_owned()])),
        renomeando: signal(None),
        filtro: signal(String::new()),
        resultado_sel: signal(BTreeSet::new()),
        titulo: signal(String::new()),
        rascunho: signal(String::new()),
        controlador: TextEditorController::new(),
        status: signal(String::new()),
        erro: signal(None),
        info: signal(None),
        aberta: signal(None),
        carregada: Cell::new(None),
        ultimo: RefCell::new((String::new(), String::new())),
        ultima_mudanca: Cell::new(Instant::now()),
        armado: Cell::new(false),
        salvando: Cell::new(false),
        ultimo_uso: Cell::new(None),
        mostrados: signal(BTreeSet::new()),
        busca_aberta: signal(false),
        busca: signal(String::new()),
        substituir_visivel: signal(false),
        substituir: signal(String::new()),
        marcas: signal(Vec::new()),
        atual: signal(0),
        acertos: signal(Vec::new()),
        ultima_busca: RefCell::new((false, String::new())),
        apagar_aberto: signal(false),
        apagar_chaves: signal(Vec::new()),
        mover_aberto: signal(false),
        mover_chaves: signal(Vec::new()),
        mover_destino: signal(None),
        fila: anexos::Fila::nova(),
        gravando: signal(BTreeSet::new()),
        inseridos: RefCell::new(HashMap::new()),
        anexo_alvo: signal(None),
        anexo_nome: signal(String::new()),
        renomear_anexo_aberto: signal(false),
        apagar_anexo_aberto: signal(false),
        imagens: Imagens::nova(),
        imagem_aberta: signal(None),
        imagem_dialogo: signal(false),
        ctx: ctx.clone(),
    });
    let selecao_arvore = pagina.selecao_arvore;
    let abertos = pagina.abertos;
    let renomeando = pagina.renomeando;
    let filtro = pagina.filtro;
    let resultado_sel = pagina.resultado_sel;
    let titulo = pagina.titulo;
    let rascunho = pagina.rascunho;
    let aberta = pagina.aberta;
    let status = pagina.status;
    let erro = pagina.erro;
    let info = pagina.info;
    let busca_aberta = pagina.busca_aberta;
    let busca = pagina.busca;
    let substituir = pagina.substituir;
    let substituir_visivel = pagina.substituir_visivel;
    let atual = pagina.atual;
    let acertos = pagina.acertos;
    let apagar_aberto = pagina.apagar_aberto;
    let apagar_chaves = pagina.apagar_chaves;
    let mover_aberto = pagina.mover_aberto;
    let mover_chaves = pagina.mover_chaves;
    let mover_destino = pagina.mover_destino;

    // Ao bloquear o cofre, grava a nota e esvazia o editor; ao fechar a janela, só grava.
    {
        let fraca = Rc::downgrade(&pagina);
        cofre.antes_de_bloquear(move || {
            if let Some(pagina) = fraca.upgrade() {
                pagina.ao_bloquear();
            }
        });
        let fraca = Rc::downgrade(&pagina);
        *ctx.ao_fechar.borrow_mut() = Some(Rc::new(move || {
            if let Some(pagina) = fraca.upgrade() {
                pagina.cancelar_anexos();
                pagina.salvar_agora();
            }
        }));
    }

    // A árvore, os resultados do filtro e a árvore do "Mover para…" acompanham o cofre.
    let arvore_itens = signal(Vec::<TreeItem>::new());
    let mover_itens = signal(Vec::<TreeItem>::new());
    {
        let cofre = cofre.clone();
        effect(move || {
            let linhas = cofre.ler(arvore_das_notas);
            untrack(|| arvore_itens.set(linhas));
        });
    }
    {
        let cofre = cofre.clone();
        effect(move || {
            let linhas = cofre.ler(arvore_das_pastas);
            untrack(|| mover_itens.set(linhas));
        });
    }
    let mover_abertos = signal(BTreeSet::<String>::new());
    {
        let cofre = cofre.clone();
        effect(move || {
            if mover_aberto.get() {
                let todas: BTreeSet<String> = cofre.agora(|c| {
                    std::iter::once(RAIZ.to_owned())
                        .chain(c.pastas().iter().map(|p| chave_da_pasta(Some(p.id))))
                        .collect()
                });
                untrack(|| mover_abertos.set(todas));
            }
        });
    }
    let resultados = signal(Vec::<DataGridRow>::new());
    {
        let cofre = cofre.clone();
        effect(move || {
            let texto = filtro.get();
            let linhas = cofre.ler(|c| linhas_dos_resultados(c, &texto));
            untrack(|| resultados.set(linhas));
        });
    }
    // A raiz fica sempre aberta.
    effect(move || {
        if !abertos.get().contains(RAIZ) {
            untrack(|| {
                abertos.update(|a| {
                    a.insert(RAIZ.to_owned());
                });
            });
        }
    });

    // Escolher uma nota na árvore abre a nota.
    {
        let pagina = Rc::clone(&pagina);
        effect(move || {
            let Some(chave) = selecao_arvore.get() else {
                return;
            };
            untrack(|| {
                if let Some(id) = id_da_chave(&chave, PREFIXO_NOTA) {
                    pagina.abrir(id);
                }
            });
        });
    }
    // Escolher um resultado do filtro abre a nota e a busca nela, com o texto do filtro.
    {
        let pagina = Rc::clone(&pagina);
        effect(move || {
            let escolhidas = resultado_sel.get();
            untrack(|| {
                let mut chaves = escolhidas.iter();
                let (Some(chave), None) = (chaves.next(), chaves.next()) else {
                    return;
                };
                let Some(id) = id_da_chave(chave, PREFIXO_NOTA) else {
                    return;
                };
                if pagina.abrir(id) {
                    pagina.busca.set(filtro.get_untracked().trim().to_owned());
                    pagina.busca_aberta.set(true);
                }
            });
        });
    }
    // O pedido de fora: abre a nota e limpa o filtro.
    {
        let pagina = Rc::clone(&pagina);
        let pedido = ctx.pedido;
        effect(move || {
            let Some(id) = pedido.get() else {
                return;
            };
            untrack(|| {
                pedido.set(None);
                filtro.set(String::new());
                if !pagina.cofre.agora(|c| c.bloqueado()) && !pagina.abrir(id) {
                    pagina.falhar(t!("notas-nao-esta-mais-no-cofre"));
                }
            });
        });
    }
    // O cofre mudou: a nota aberta pode ter saído ou mudado, e a lembrada abre ao desbloquear.
    {
        let pagina = Rc::clone(&pagina);
        let cofre = cofre.clone();
        effect(move || {
            let _ = cofre.versao();
            untrack(|| pagina.cofre_mudou());
        });
    }
    // O título e o corpo mudaram: grava depois da pausa.
    {
        let pagina = Rc::clone(&pagina);
        effect(move || {
            let t = titulo.get();
            let c = rascunho.get();
            untrack(|| pagina.alterado(&t, &c));
        });
    }
    // A busca na nota.
    {
        let pagina = Rc::clone(&pagina);
        effect(move || pagina.refazer_busca());
    }
    // Enquanto há operação de anexo andando, o cofre conta como em uso.
    {
        let pagina = Rc::clone(&pagina);
        effect(move || {
            let operacoes = pagina.fila.operacoes();
            if !operacoes.is_empty() {
                untrack(|| pagina.usar());
            }
        });
    }
    // O foco volta ao editor quando a busca fecha por outro caminho.
    effect(move || {
        if !busca_aberta.get() {
            untrack(|| substituir_visivel.set(false));
        }
    });

    // Os três modos são botões de alternar: só um fica ligado.
    let modo_editar = signal(estado.modo.get_untracked() == ModoNotas::Editar);
    let modo_dividir = signal(estado.modo.get_untracked() == ModoNotas::Dividir);
    let modo_ler = signal(estado.modo.get_untracked() == ModoNotas::Ler);
    let botoes_de_modo = [
        (ModoNotas::Editar, modo_editar),
        (ModoNotas::Dividir, modo_dividir),
        (ModoNotas::Ler, modo_ler),
    ];
    let ligado_de = move |modo: ModoNotas| match modo {
        ModoNotas::Editar => modo_editar,
        ModoNotas::Dividir => modo_dividir,
        ModoNotas::Ler => modo_ler,
    };
    for (modo, ligado) in botoes_de_modo {
        effect(move || {
            let ligado = ligado.get();
            untrack(|| {
                if ligado {
                    if estado.modo.get_untracked() != modo {
                        estado.modo.set(modo);
                    }
                } else if estado.modo.get_untracked() == modo {
                    // O modo ativo não se desliga.
                    ligado_de(modo).set(true);
                }
            });
        });
    }
    effect(move || {
        let modo = estado.modo.get();
        untrack(|| {
            for (m, ligado) in botoes_de_modo {
                let quer = m == modo;
                if ligado.get_untracked() != quer {
                    ligado.set(quer);
                }
            }
        });
    });

    let bloqueado = {
        let cofre = cofre.clone();
        move || cofre.ler(|c| c.bloqueado())
    };
    let sem_nota = move || aberta.get().is_none();
    let sem_formato = move || aberta.get().is_none() || estado.modo.get() == ModoNotas::Ler;

    // --- A lista: botões, filtro, árvore e resultados ---
    // Montada de novo quando o painel muda de lugar: o estado dela está em signals de fora.
    let montar_lista = {
        let pagina = Rc::clone(&pagina);
        let bloqueado = bloqueado.clone();
        move || {
            let botoes = {
                let nova = Rc::clone(&pagina);
                let pasta = Rc::clone(&pagina);
                let travado = bloqueado.clone();
                let menu_lista = {
                    let (arquivo, pasta_importada, exportar) =
                        (Rc::clone(&pagina), Rc::clone(&pagina), Rc::clone(&pagina));
                    MenuFlyout::new()
                        .id("notas-mais-lista-menu")
                        .item(
                            "notas-importar-arquivo",
                            t!("notas-importar-arquivo-markdown"),
                            move || {
                                let destino = arquivo.pasta_da_selecao();
                                arquivo.importar_arquivo(destino);
                            },
                        )
                        .icon(Symbol::ArrowDownload)
                        .item(
                            "notas-importar-pasta",
                            t!("notas-importar-pasta-de-markdown"),
                            move || {
                                let destino = pasta_importada.pasta_da_selecao();
                                pasta_importada.importar_pasta(destino);
                            },
                        )
                        .icon(Symbol::FolderOpen)
                        .separator()
                        .item(
                            "notas-exportar-pasta",
                            t!("notas-exportar-esta-pasta-para-markdown"),
                            move || {
                                let pasta = exportar.pasta_da_selecao();
                                exportar.exportar_pasta(pasta);
                            },
                        )
                        .icon(Symbol::ArrowUpload)
                };
                let mut menu_novo = MenuFlyout::new().id("notas-nova-menu");
                for modelo in Modelo::TODOS {
                    let p = Rc::clone(&nova);
                    menu_novo = menu_novo.item(
                        format!("notas-modelo-{}", modelo.id()),
                        modelo.rotulo(),
                        move || p.nova_nota(modelo),
                    );
                }
                Row::new()
                    .gap(4)
                    .padding(Insets::new(8.0, 4.0, 4.0, 4.0))
                    .child(
                        Button::new(t!("notas-nova-nota"))
                            .id("notas-nova")
                            .icon(Symbol::Add)
                            .disabled(travado.clone())
                            .flyout(menu_novo),
                    )
                    .child(
                        Button::icon_only(Symbol::FolderAdd)
                            .id("notas-nova-pasta")
                            .label(t!("comum-nova-pasta"))
                            .tooltip(t!("comum-nova-pasta"))
                            .disabled(travado.clone())
                            .on_click(move || pasta.nova_pasta()),
                    )
                    .child(
                        Button::icon_only(Symbol::MoreHorizontal)
                            .id("notas-mais-lista")
                            .label(t!("notas-importar-e-exportar"))
                            .tooltip(t!("notas-importar-e-exportar"))
                            .disabled(travado)
                            .flyout(menu_lista),
                    )
            };

            let arvore = {
                let renomear = Rc::clone(&pagina);
                let apagar = Rc::clone(&pagina);
                let mover = Rc::clone(&pagina);
                let aceita = Rc::clone(&pagina);
                let menu = Rc::clone(&pagina);
                TreeView::bind(arvore_itens, selecao_arvore)
                    .id("notas-arvore")
                    .grow(1)
                    .expanded(abertos)
                    .renaming(renomeando)
                    .on_rename(move |chave, texto| renomear.renomear(&chave, &texto))
                    .on_delete(move |chave| apagar.pedir_apagar(vec![chave]))
                    .on_move(move |chave, pai, _| {
                        if let Some(pai) = pai {
                            mover.mover(&[chave], &pai);
                        }
                    })
                    .can_drop(move |origem, pai| {
                        origem != RAIZ
                            && pai.is_some_and(|pai| aceita.aceita(&[origem.to_owned()], pai))
                    })
                    .item_context_flyout(move |chave| {
                        let prefixo = format!("menu-notas-{chave}");
                        let acao = |f: fn(&Pagina, &str)| {
                            let (pagina, chave) = (Rc::clone(&menu), chave.to_owned());
                            move || f(&pagina, &chave)
                        };
                        // Para as ações que precisam do `Rc` (o que vai para outra thread ou para o diálogo).
                        let acao_rc = |f: fn(&Rc<Pagina>, &str)| {
                            let (pagina, chave) = (Rc::clone(&menu), chave.to_owned());
                            move || f(&pagina, &chave)
                        };
                        let flyout = MenuFlyout::new().id(prefixo.clone());
                        if let Some(id) = id_da_chave(chave, PREFIXO_NOTA) {
                            let _ = id;
                            return flyout
                                .item(
                                    format!("{prefixo}-abrir"),
                                    t!("comum-abrir"),
                                    acao(|p, c| {
                                        if let Some(id) = id_da_chave(c, PREFIXO_NOTA) {
                                            p.abrir(id);
                                        }
                                    }),
                                )
                                .icon(Symbol::DocumentText)
                                .item(
                                    format!("{prefixo}-renomear"),
                                    t!("comum-renomear"),
                                    acao(|p, c| p.renomeando.set(Some(c.to_owned()))),
                                )
                                .icon(Symbol::Rename)
                                .item(
                                    format!("{prefixo}-duplicar"),
                                    t!("comum-duplicar"),
                                    acao(|p, c| {
                                        if let Some(id) = id_da_chave(c, PREFIXO_NOTA) {
                                            p.duplicar(id);
                                        }
                                    }),
                                )
                                .icon(Symbol::Copy)
                                .item(
                                    format!("{prefixo}-mover"),
                                    t!("notas-mover-para"),
                                    acao(|p, c| p.pedir_mover(vec![c.to_owned()])),
                                )
                                .icon(Symbol::ArrowForward)
                                .item(
                                    format!("{prefixo}-exportar"),
                                    t!("notas-exportar-como-markdown"),
                                    acao_rc(|p, c| {
                                        if let Some(id) = id_da_chave(c, PREFIXO_NOTA) {
                                            p.exportar_nota(id);
                                        }
                                    }),
                                )
                                .icon(Symbol::ArrowUpload)
                                .item(
                                    format!("{prefixo}-apagar"),
                                    t!("comum-apagar"),
                                    acao(|p, c| p.pedir_apagar(vec![c.to_owned()])),
                                )
                                .icon(Symbol::Delete);
                        }
                        let nova = acao(|p, c| {
                            if let Some(pasta) = destino_da_chave(c) {
                                p.nova_nota_em(pasta, Modelo::Branco);
                            }
                        });
                        let subpasta = acao(|p, c| {
                            if let Some(pasta) = destino_da_chave(c) {
                                p.nova_pasta_em(pasta);
                            }
                        });
                        let flyout = flyout
                            .item(
                                format!("{prefixo}-nova-nota"),
                                t!("notas-nova-nota-aqui"),
                                nova,
                            )
                            .icon(Symbol::DocumentAdd)
                            .item(
                                format!("{prefixo}-subpasta"),
                                t!("comum-nova-pasta"),
                                subpasta,
                            )
                            .icon(Symbol::FolderAdd)
                            .item(
                                format!("{prefixo}-importar"),
                                t!("notas-importar-markdown-aqui"),
                                acao_rc(|p, c| {
                                    if let Some(pasta) = destino_da_chave(c) {
                                        p.importar_arquivo(pasta);
                                    }
                                }),
                            )
                            .icon(Symbol::ArrowDownload)
                            .item(
                                format!("{prefixo}-exportar"),
                                t!("notas-exportar-esta-pasta-para-markdown"),
                                acao_rc(|p, c| {
                                    if let Some(pasta) = destino_da_chave(c) {
                                        p.exportar_pasta(pasta);
                                    }
                                }),
                            )
                            .icon(Symbol::ArrowUpload);
                        if chave == RAIZ {
                            return flyout;
                        }
                        flyout
                            .item(
                                format!("{prefixo}-renomear"),
                                t!("comum-renomear"),
                                acao(|p, c| p.renomeando.set(Some(c.to_owned()))),
                            )
                            .icon(Symbol::Rename)
                            .item(
                                format!("{prefixo}-mover"),
                                t!("notas-mover-para"),
                                acao(|p, c| p.pedir_mover(vec![c.to_owned()])),
                            )
                            .icon(Symbol::ArrowForward)
                            .item(
                                format!("{prefixo}-apagar"),
                                t!("comum-apagar"),
                                acao(|p, c| p.pedir_apagar(vec![c.to_owned()])),
                            )
                            .icon(Symbol::Delete)
                    })
                    .visible(move || filtro.get().trim().is_empty())
            };
            let lista_de_resultados = DataGrid::bind(resultados, resultado_sel)
                .id("notas-resultados")
                .grow(1)
                .columns([
                    DataGridColumn::new("titulo", t!("comum-titulo")).width(ColumnWidth::Star(1.0)),
                    DataGridColumn::new("pasta", t!("comum-pasta"))
                        .width(ColumnWidth::Pixels(90.0)),
                    DataGridColumn::new("trecho", t!("notas-trecho"))
                        .width(ColumnWidth::Pixels(140.0)),
                ])
                .empty_text(t!("notas-nenhuma-nota-encontrada"))
                .visible(move || !filtro.get().trim().is_empty());
            Column::new()
                .child(botoes)
                .child(
                    Column::new()
                        .padding(Insets::new(8.0, 0.0, 4.0, 4.0))
                        .child(
                            Edit::bind(filtro)
                                .id("notas-filtro")
                                .placeholder(t!("comum-filtrar")),
                        ),
                )
                .child(
                    Column::new()
                        .grow(1)
                        .padding(Insets::new(8.0, 4.0, 4.0, 4.0))
                        .child(arvore)
                        .child(lista_de_resultados),
                )
        }
    };

    // --- A barra de formatação ---
    let barra = {
        let botao = |id: &'static str, simbolo: Symbol, dica: String, f: fn(&Pagina)| {
            let p = Rc::clone(&pagina);
            Button::icon_only(simbolo)
                .id(id)
                .label(dica.clone())
                .tooltip(dica)
                .disabled(sem_formato)
                .on_click(move || f(&p))
        };
        let titulos = {
            let (a, b, c, d) = (
                Rc::clone(&pagina),
                Rc::clone(&pagina),
                Rc::clone(&pagina),
                Rc::clone(&pagina),
            );
            Button::icon_only(Symbol::TextHeader1)
                .id("notas-titulos")
                .label(t!("notas-estilo-titulo"))
                .tooltip(t!("notas-titulo-ctrl-1-ctrl-2-ctrl"))
                .disabled(sem_formato)
                .flyout(
                    MenuFlyout::new()
                        .id("notas-titulos-menu")
                        .item("notas-titulo-1", t!("notas-titulo-1"), move || a.nivel(1))
                        .item("notas-titulo-2", t!("notas-titulo-2"), move || b.nivel(2))
                        .item("notas-titulo-3", t!("notas-titulo-3"), move || c.nivel(3))
                        .item("notas-titulo-0", t!("notas-texto-normal"), move || {
                            d.nivel(0)
                        }),
                )
        };
        Row::new()
            .wrap()
            .gap(2)
            .child(botao(
                "notas-negrito",
                Symbol::TextBold,
                t!("notas-dica-negrito"),
                |p| p.envolver("**"),
            ))
            .child(botao(
                "notas-italico",
                Symbol::TextItalic,
                t!("notas-dica-italico"),
                |p| p.envolver("*"),
            ))
            .child(botao(
                "notas-riscado",
                Symbol::TextStrikethrough,
                t!("notas-dica-riscado"),
                |p| p.envolver("~~"),
            ))
            .child(titulos)
            .child(botao(
                "notas-lista",
                Symbol::TextBulletList,
                t!("notas-dica-lista"),
                |p| p.prefixo(Prefix::Bullet),
            ))
            .child(botao(
                "notas-lista-numerada",
                Symbol::TextNumberList,
                t!("notas-dica-lista-numerada"),
                |p| p.prefixo(Prefix::Numbered),
            ))
            .child(botao(
                "notas-tarefa",
                Symbol::TaskListLtr,
                t!("notas-dica-tarefa"),
                |p| p.prefixo(Prefix::Task),
            ))
            .child(botao(
                "notas-citacao",
                Symbol::TextQuote,
                t!("notas-dica-citacao"),
                |p| p.prefixo(Prefix::Quote),
            ))
            .child(botao(
                "notas-codigo",
                Symbol::Code,
                t!("notas-dica-codigo"),
                |p| p.envolver("`"),
            ))
            .child(botao(
                "notas-bloco-codigo",
                Symbol::CodeBlock,
                t!("notas-dica-bloco-codigo"),
                |p| p.bloco_de_codigo(""),
            ))
            .child(botao(
                "notas-segredo",
                Symbol::LockClosed,
                t!("notas-dica-segredo"),
                |p| p.bloco_de_codigo("segredo"),
            ))
            .child(botao(
                "notas-link",
                Symbol::Link,
                t!("notas-dica-link"),
                Pagina::link,
            ))
            .child(botao(
                "notas-tabela",
                Symbol::Table,
                t!("notas-dica-tabela"),
                Pagina::tabela,
            ))
            .child(botao(
                "notas-regua",
                Symbol::LineHorizontal1,
                t!("notas-dica-regua"),
                Pagina::regua,
            ))
            .child(
                Button::icon_only(Symbol::Attach)
                    .id("notas-anexar")
                    .label(t!("comum-anexar-arquivo"))
                    .tooltip(t!("comum-anexar-arquivo"))
                    .disabled(sem_nota)
                    .on_click({
                        let p = Rc::clone(&pagina);
                        move || p.anexar_pelo_dialogo()
                    }),
            )
    };
    let modos = Row::new()
        .gap(2)
        .child(
            ToggleButton::bind(modo_editar, t!("notas-editar"))
                .id("notas-modo-editar")
                .icon(Symbol::Edit),
        )
        .child(
            ToggleButton::bind(modo_dividir, t!("notas-dividir"))
                .id("notas-modo-dividir")
                .icon(Symbol::SplitVertical),
        )
        .child(
            ToggleButton::bind(modo_ler, t!("notas-ler"))
                .id("notas-modo-ler")
                .icon(Symbol::Eye),
        );
    let mais = {
        let anexar = Rc::clone(&pagina);
        let exportar = Rc::clone(&pagina);
        let (duplicar, copiar, apagar, mover) = (
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
        );
        let menu = MenuFlyout::new()
            .id("notas-mais-menu")
            .item("notas-duplicar", t!("notas-duplicar"), move || {
                if let Some(id) = duplicar.carregada.get() {
                    duplicar.duplicar(id);
                }
            })
            .icon(Symbol::Copy)
            .item("notas-mover", t!("notas-mover-para"), move || {
                if let Some(id) = mover.carregada.get() {
                    mover.pedir_mover(vec![chave_da_nota(id)]);
                }
            })
            .icon(Symbol::ArrowForward)
            .item(
                "notas-copiar-markdown",
                t!("notas-copiar-como-markdown"),
                move || {
                    copiar.cofre.copiar_segredo(copiar.rascunho.get_untracked());
                    copiar.informar(t!("notas-copiada-como-markdown"));
                },
            )
            .icon(Symbol::Clipboard)
            .item(
                "notas-exportar",
                t!("notas-exportar-como-markdown"),
                move || {
                    if let Some(id) = exportar.carregada.get() {
                        exportar.exportar_nota(id);
                    }
                },
            )
            .icon(Symbol::ArrowUpload)
            .item("notas-anexar-menu", t!("notas-anexar-arquivo"), move || {
                anexar.anexar_pelo_dialogo()
            })
            .icon(Symbol::Attach)
            .separator()
            .item("notas-apagar", t!("notas-apagar"), move || {
                if let Some(id) = apagar.carregada.get() {
                    apagar.pedir_apagar(vec![chave_da_nota(id)]);
                }
            })
            .icon(Symbol::Delete);
        Button::icon_only(Symbol::MoreHorizontal)
            .id("notas-mais")
            .label(t!("comum-mais"))
            .tooltip(t!("comum-mais"))
            .flyout(menu)
    };

    // --- O corpo: editor e pré-visualização, conforme o modo ---
    let corpo = {
        let pagina = Rc::clone(&pagina);
        let config = ctx.config;
        let chaves = move || {
            let notas = config.get().notas;
            vec![(
                match estado.modo.get() {
                    ModoNotas::Editar => 0u8,
                    ModoNotas::Dividir => 1,
                    ModoNotas::Ler => 2,
                },
                notas.fonte_mono,
                notas.quebrar_linhas,
            )]
        };
        For::each(
            chaves,
            |k: &(u8, bool, bool)| *k,
            move |(modo, mono, quebrar): (u8, bool, bool)| {
                let editor = {
                    let tecla = Rc::clone(&pagina);
                    let copia = Rc::clone(&pagina);
                    let colar = Rc::clone(&pagina);
                    let imagem = Rc::clone(&pagina);
                    let soltar = Rc::clone(&pagina);
                    let mut editor = TextEditor::bind(pagina.rascunho)
                        .id("notas-editor")
                        .placeholder(t!("notas-escreva-em-markdown"))
                        .highlight(highlight)
                        .wrap(quebrar)
                        .marks(pagina.marcas)
                        .controller(pagina.controlador.clone())
                        .on_key(move |t, c| tecla.tecla(t, c))
                        .on_copy(move |texto| copia.cofre.copiar_segredo(texto))
                        .on_paste_files(move |arquivos, controle| {
                            colar.anexar_arquivos(arquivos, &anexos::Destino::Cursor(controle));
                        })
                        .on_paste_image(move |foto, controle| imagem.anexar_imagem(foto, controle))
                        .on_files_dropped(move |arquivos, controle| {
                            soltar.anexar_arquivos(arquivos, &anexos::Destino::Cursor(controle));
                        });
                    if mono {
                        editor = editor.monospace();
                    }
                    editor
                };
                let previa = previa_da_nota(&pagina);
                let coluna = Column::new().grow(1);
                match modo {
                    0 => coluna.child(editor),
                    1 => coluna.child(
                        Split::row(editor, previa)
                            .id("notas-corpo")
                            .ratio(estado.divisor)
                            .min_first(LARGURA_MINIMA_DO_PAINEL)
                            .min_second(LARGURA_MINIMA_DO_PAINEL)
                            .grow(1),
                    ),
                    _ => coluna.child(previa),
                }
            },
        )
    };

    // --- A busca na nota ---
    let busca_barra = {
        let (anterior, proxima, fechar, um, todos) = (
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
        );
        let (a_prox, a_ant, a_fechar, a_enter, a_shift_enter) = (
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
            Rc::clone(&pagina),
        );
        let contagem_texto = move || {
            let n = acertos.get().len();
            if busca.get().is_empty() {
                String::new()
            } else if n == 0 {
                t!("notas-nenhum-resultado")
            } else {
                t!(
                    "notas-resultado-n-de-m",
                    atual = atual.get() as i64 + 1,
                    total = n as i64
                )
            }
        };
        let sem_acertos = move || acertos.get().is_empty();
        Column::new()
            .id("notas-busca-barra")
            .gap(4)
            .padding(Insets::new(0.0, 4.0, 0.0, 0.0))
            .visible(move || busca_aberta.get())
            .accelerator(Shortcut::key(Key::Enter), move || a_enter.passar(1))
            .accelerator(Shortcut::new(Key::Enter, Modifiers::SHIFT), move || {
                a_shift_enter.passar(-1)
            })
            .accelerator(Shortcut::key(Key::F(3)), move || a_prox.passar(1))
            .accelerator(Shortcut::new(Key::F(3), Modifiers::SHIFT), move || {
                a_ant.passar(-1)
            })
            .accelerator(Shortcut::key(Key::Escape), move || a_fechar.fechar_busca())
            .child(
                Row::new()
                    .gap(4)
                    .align(Align::Center)
                    .child(
                        Edit::bind(busca)
                            .id("notas-busca")
                            .placeholder(t!("notas-buscar-na-nota"))
                            .width(220),
                    )
                    .child(
                        Text::new(contagem_texto)
                            .id("notas-busca-contagem")
                            .secondary(),
                    )
                    .child(
                        Button::icon_only(Symbol::ChevronUp)
                            .id("notas-busca-anterior")
                            .label(t!("notas-resultado-anterior"))
                            .tooltip(t!("notas-anterior-shift-f3"))
                            .disabled(sem_acertos)
                            .on_click(move || anterior.passar(-1)),
                    )
                    .child(
                        Button::icon_only(Symbol::ChevronDown)
                            .id("notas-busca-proxima")
                            .label(t!("notas-proximo-resultado"))
                            .tooltip(t!("notas-proximo-f3"))
                            .disabled(sem_acertos)
                            .on_click(move || proxima.passar(1)),
                    )
                    .child(
                        Button::icon_only(Symbol::Dismiss)
                            .id("notas-busca-fechar")
                            .label(t!("notas-fechar-a-busca"))
                            .tooltip(t!("notas-fechar-esc"))
                            .on_click(move || fechar.fechar_busca()),
                    ),
            )
            .child(
                Row::new()
                    .gap(4)
                    .align(Align::Center)
                    .visible(move || substituir_visivel.get())
                    .child(
                        Edit::bind(substituir)
                            .id("notas-substituir")
                            .placeholder(t!("notas-substituir-por"))
                            .width(220),
                    )
                    .child(
                        Button::new(t!("comum-substituir"))
                            .id("notas-substituir-um")
                            .disabled(sem_acertos)
                            .on_click(move || um.substituir_um()),
                    )
                    .child(
                        Button::new(t!("notas-substituir-tudo"))
                            .id("notas-substituir-todos")
                            .disabled(sem_acertos)
                            .on_click(move || todos.substituir_todos()),
                    ),
            )
    };

    // --- A direita: título, conexão, barra, corpo, busca, avisos e status ---
    let avisos = Column::new()
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("notas-erro")
                .message(move || erro.get().unwrap_or_default())
                .on_close(move || erro.set(None))
                .visible(move || erro.get().is_some()),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Informational)
                .id("notas-info")
                .message(move || info.get().unwrap_or_default())
                .on_close(move || info.set(None))
                .visible(move || info.get().is_some()),
        );
    let editor_aberto = Column::new()
        .grow(1)
        .gap(4)
        .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
        .visible(move || !sem_nota())
        .child(
            Edit::bind(titulo)
                .id("notas-titulo")
                .placeholder(t!("notas-titulo-da-nota")),
        )
        .child(
            Row::new()
                .wrap()
                .gap(8)
                .align(Align::Center)
                .child(barra)
                .child(modos)
                .child(mais),
        )
        .child(corpo)
        .child(busca_barra)
        .child(avisos)
        .child(anexos::fila_de_operacoes(&pagina))
        .child(anexos::lista_de_anexos(&pagina))
        .child(
            Text::new(move || status.get())
                .id("notas-status")
                .caption()
                .secondary(),
        );
    let vazio = {
        let nova = Rc::clone(&pagina);
        let cofre = cofre.clone();
        let texto = move || {
            let _ = cofre.versao();
            if cofre.agora(|c| c.bloqueado() || c.notas().next().is_some()) {
                t!("notas-nenhuma-aberta")
            } else {
                t!("notas-nenhuma-nota-ainda")
            }
        };
        Column::new()
            .grow(1)
            .align(Align::Center)
            .justify(Justify::Center)
            .gap(8)
            .visible(sem_nota)
            .child(Text::new(texto).id("notas-vazio").secondary())
            .child(
                Button::new(t!("notas-nova-nota"))
                    .id("notas-vazio-nova")
                    .icon(Symbol::Add)
                    .accent()
                    .on_click(move || nova.nova_nota(Modelo::Branco)),
            )
    };
    let direita = Column::new().grow(1).child(editor_aberto).child(vazio);

    let paineis = contexto_paineis;
    let aberto = DockHost::new(direita)
        .id("notas-paineis")
        .layout(paineis.layout)
        .controller(&paineis.controle)
        .min_content(360.0)
        .pane(
            DockPane::new(
                "lista",
                Symbol::Notepad,
                t!("casca-painel-notas"),
                montar_lista,
            )
            .size(LATERAL_PADRAO)
            .min_size(200.0),
        );

    // --- O cofre bloqueado ---
    let desbloqueio_aberto = signal(false);
    let bloqueado_painel = Column::new()
        .grow(1)
        .align(Align::Center)
        .justify(Justify::Center)
        .gap(8)
        .child(Icon::new(Symbol::LockClosed).size(24).decorative())
        .child(
            Text::new(t!("comum-o-cofre-esta-bloqueado"))
                .id("notas-bloqueado")
                .subtitle(),
        )
        .child(
            Button::new(t!("comum-desbloquear"))
                .id("notas-desbloquear-botao")
                .accent()
                .on_click(move || desbloqueio_aberto.set(true)),
        );
    let dialogo_desbloqueio = dialogo_desbloquear_id(
        "notas-desbloquear",
        cofre.clone(),
        desbloqueio_aberto,
        Rc::new(Cell::new(false)),
    );

    // --- Os diálogos ---
    let dialogo_apagar = {
        let p = Rc::clone(&pagina);
        let cofre_titulo = cofre.clone();
        let cofre_texto = cofre.clone();
        ContentDialog::new(
            move || {
                let chaves = apagar_chaves.get();
                let _ = cofre_titulo.versao();
                let nome = (chaves.len() == 1).then(|| {
                    cofre_titulo.agora(|c| {
                        let chave = &chaves[0];
                        id_da_chave(chave, PREFIXO_NOTA)
                            .and_then(|id| c.item(id).map(rotulo))
                            .or_else(|| {
                                id_da_chave(chave, PREFIXO_PASTA)
                                    .and_then(|id| c.pasta(id).map(|p| p.nome.clone()))
                            })
                            .unwrap_or_default()
                    })
                });
                match nome {
                    Some(nome) => t!("comum-apagar-titulo", nome = nome),
                    None => t!("cofre-apagar-n-itens", n = chaves.len() as i64),
                }
            },
            apagar_aberto,
        )
        .id("notas-apagar-dialogo")
        .content(
            Text::new(move || {
                let chaves = apagar_chaves.get();
                let _ = cofre_texto.versao();
                cofre_texto.agora(|c| {
                    if let [chave] = chaves.as_slice()
                        && let Some(id) = id_da_chave(chave, PREFIXO_NOTA)
                    {
                        let nome = c.item(id).map(rotulo).unwrap_or_default();
                        return t!("notas-apagar-texto", nome = nome);
                    }
                    let (pastas, itens, so_uma) = contar_apagar(c, &chaves_do_cofre(&chaves));
                    resumo_apagar(pastas, itens, so_uma)
                })
            })
            .id("notas-apagar-texto")
            .wrap(),
        )
        .primary(t!("comum-apagar"), move || p.apagar())
        .close(t!("comum-cancelar"))
    };
    let dialogo_mover = {
        let p = Rc::clone(&pagina);
        let cofre_titulo = cofre.clone();
        ContentDialog::new(
            move || {
                let chaves = mover_chaves.get();
                let _ = cofre_titulo.versao();
                let nome = (chaves.len() == 1).then(|| {
                    cofre_titulo.agora(|c| {
                        let chave = &chaves[0];
                        id_da_chave(chave, PREFIXO_NOTA)
                            .and_then(|id| c.item(id).map(rotulo))
                            .or_else(|| {
                                id_da_chave(chave, PREFIXO_PASTA)
                                    .and_then(|id| c.pasta(id).map(|p| p.nome.clone()))
                            })
                            .unwrap_or_default()
                    })
                });
                match nome {
                    Some(nome) => t!("cofre-mover-titulo", nome = nome),
                    None => t!("cofre-mover-n-itens", n = chaves.len() as i64),
                }
            },
            mover_aberto,
        )
        .id("notas-mover-dialogo")
        .content(
            TreeView::bind(mover_itens, mover_destino)
                .id("notas-mover-arvore")
                .expanded(mover_abertos)
                .width(320)
                .height(260),
        )
        .primary(t!("comum-mover"), move || {
            let destino = mover_destino
                .get_untracked()
                .unwrap_or_else(|| RAIZ.to_owned());
            p.mover(&mover_chaves.get_untracked(), &destino);
        })
        .close(t!("comum-cancelar"))
    };

    // --- Os atalhos da página (com o foco fora do editor) ---
    let (salvar, buscar, substituir_atalho, nova, leitura) = (
        Rc::clone(&pagina),
        Rc::clone(&pagina),
        Rc::clone(&pagina),
        Rc::clone(&pagina),
        Rc::clone(&pagina),
    );
    let corpo_da_pagina = {
        let bloqueado_a = bloqueado.clone();
        let bloqueado_b = bloqueado;
        Column::new()
            .grow(1)
            .child(
                Column::new()
                    .grow(1)
                    .visible(move || !bloqueado_a())
                    .child(aberto),
            )
            .child(bloqueado_painel.visible(bloqueado_b))
    };
    Column::new()
        .id("notas-pagina")
        .grow(1)
        .accelerator(Shortcut::ctrl('s'), move || {
            salvar.armado.set(false);
            salvar.salvar_agora();
        })
        .accelerator(Shortcut::ctrl('f'), move || buscar.abrir_busca(false))
        .accelerator(Shortcut::ctrl('h'), move || {
            substituir_atalho.abrir_busca(true)
        })
        .accelerator(Shortcut::ctrl('n'), move || nova.nova_nota(Modelo::Branco))
        .accelerator(Shortcut::ctrl_shift('p'), move || {
            leitura.alternar_leitura()
        })
        .child(corpo_da_pagina)
        .child(dialogo_apagar)
        .child(dialogo_mover)
        .child(anexos::dialogos_de_anexo(&pagina))
        .child(anexos::dialogo_da_imagem(&pagina))
        .child(dialogo_desbloqueio)
}

/// A pré-visualização (`notas-previa`), numa rolagem.
fn previa_da_nota(pagina: &Rc<Pagina>) -> impl View + use<> {
    let tarefa = Rc::clone(pagina);
    let link = Rc::clone(pagina);
    let copia = Rc::clone(pagina);
    let rascunho = pagina.rascunho;
    let mostrados = pagina.mostrados;
    let fence_pagina = Rc::clone(pagina);
    let soltar = Rc::clone(pagina);
    let imagens = pagina.imagens.clone();
    let fonte = Rc::new(Fonte {
        ficha: {
            let pagina = Rc::clone(pagina);
            Box::new(move |id| pagina.ficha(id))
        },
        vault: {
            let pagina = Rc::clone(pagina);
            Box::new(move || pagina.vault())
        },
        gravando: pagina.gravando,
        abrir: {
            let pagina = Rc::clone(pagina);
            Box::new(move |id| pagina.abrir_imagem(id))
        },
    });
    let previa = MarkdownView::new(move || rascunho.get())
        .id("notas-previa")
        .on_task(move |byte, _| tarefa.tarefa(byte))
        .on_link(move |destino| link.abrir_link(&destino))
        .on_copy(move |texto| copia.cofre.copiar_segredo(texto))
        .on_files_dropped(move |arquivos| soltar.anexar_arquivos(arquivos, &anexos::Destino::Fim))
        .image(move |cx| notas_imagens::desenhar(&imagens, &fonte, cx))
        .fence("segredo", move |cx| {
            let cofre = fence_pagina.cofre.clone();
            let visivel = signal(mostrados.get_untracked().contains(&cx.index));
            let indice = cx.index;
            effect(move || {
                let v = visivel.get();
                untrack(|| {
                    mostrados.update(|m| {
                        if v {
                            m.insert(indice);
                        } else {
                            m.remove(&indice);
                        }
                    });
                });
            });
            let texto = cx.text.clone();
            let para_copiar = cx.text.clone();
            let id_texto = format!("{}-texto", cx.id);
            let id_mascara = format!("{}-mascara", cx.id);
            Box::new(
                Row::new()
                    .gap(8)
                    .align(Align::Start)
                    .child(
                        Column::new().grow(1).child(
                            Show::when(
                                move || visivel.get(),
                                move || {
                                    RichText::new(vec![Span::new(texto.clone())])
                                        .id(id_texto.clone())
                                        .monospace()
                                        .wrap()
                                },
                            )
                            .otherwise({
                                let id_mascara = id_mascara.clone();
                                move || Text::new(notas::MASCARA).id(id_mascara.clone())
                            }),
                        ),
                    )
                    .child(
                        ToggleButton::bind(visivel, t!("comum-mostrar"))
                            .id(format!("{}-mostrar", cx.id))
                            .icon(Symbol::Eye),
                    )
                    .child(
                        Button::new(t!("comum-copiar"))
                            .id(format!("{}-copiar", cx.id))
                            .icon(Symbol::Copy)
                            .on_click(move || cofre.copiar_segredo(para_copiar.clone())),
                    ),
            ) as Box<dyn Widget>
        });
    ScrollViewer::new()
        .id("notas-previa-rolagem")
        .grow(1)
        .child(Border::new().padding(16).child(previa))
}
