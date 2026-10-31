//! A tela do cofre de senhas: o diálogo que desbloqueia com a senha mestra e, na página do
//! cofre, as pastas, a lista da pasta aberta e as propriedades do item. Nada aqui lê a
//! `tela.rs`: o cofre chega por parâmetro, para a página servir a qualquer casca que a hospede.

use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, UNIX_EPOCH};

use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::keys;
use tomni_ui::{Widget, clipboard, dialogs};

use crate::andamento::Andamento;
use crate::anexos::{self, ErroAnexo};
use crate::arquivos::formato;
use crate::bitwarden;
use crate::busca::normalizar;
use crate::chaves_cofre::tipo_legivel;
use crate::cofre::{
    Campo, ChaveSsh, Cofre, CofreCompartilhado, IdentidadeNebula, IdentidadeOpenVpn,
    IdentidadeTailscale, IdentidadeWireGuard, IdentidadeZeroTier, Item, Lote, Pasta, Tipo,
};
use crate::cofre_json::{self, Trazidos};
use crate::gerador::{Gerador, gerar_senha};
use crate::nebula::{self, Validade};
use crate::notas;
use crate::notas_imagens::{self, Fonte, Imagens};
use crate::openvpn;
use crate::recentes;
use crate::zerotier;
use crate::{t, tk};

/// O diálogo `cofre-desbloquear`: pede a senha mestra e desbloqueia o `cofre`. A senha errada
/// reabre o diálogo com "Senha mestra incorreta" (o `ContentDialog` fecha a cada botão, então
/// um efeito o abre de novo); "Agora não" só fecha.
pub fn dialogo_desbloquear(cofre: CofreCompartilhado, aberto: Signal<bool>) -> impl View {
    dialogo_desbloquear_com(cofre, aberto, Rc::new(Cell::new(false)))
}

/// Como [`dialogo_desbloquear`], mas com o `refazer` à vista de quem hospeda o diálogo: ele
/// fica `true` entre a senha errada e a reabertura, e quem espera o fechamento do diálogo para
/// agir (as abas que voltam ao iniciar) precisa saber que o diálogo ainda vai voltar.
pub fn dialogo_desbloquear_com(
    cofre: CofreCompartilhado,
    aberto: Signal<bool>,
    refazer: Rc<Cell<bool>>,
) -> impl View {
    dialogo_desbloquear_id("cofre-desbloquear", cofre, aberto, refazer)
}

/// O diálogo de desbloqueio com o prefixo de ids `id` (`{id}-campo`, `{id}-erro`, …): a página
/// do cofre usa outro, para não repetir os ids do diálogo que a tela abre ao iniciar.
pub(crate) fn dialogo_desbloquear_id(
    id: &str,
    cofre: CofreCompartilhado,
    aberto: Signal<bool>,
    refazer: Rc<Cell<bool>>,
) -> impl View {
    montar_desbloquear(id, cofre, aberto, refazer)()
}

/// Como [`dialogo_desbloquear_com`], para montar em várias janelas: o campo, o erro e o efeito
/// que reabre o diálogo nascem uma vez, e a função devolvida monta a caixa em cada janela.
pub(crate) fn montar_desbloquear(
    id: &str,
    cofre: CofreCompartilhado,
    aberto: Signal<bool>,
    refazer: Rc<Cell<bool>>,
) -> impl Fn() -> ContentDialog + use<> {
    let id = id.to_owned();
    let senha = signal(String::new());
    let errou = signal(false);
    {
        let refazer = Rc::clone(&refazer);
        effect(move || {
            if aberto.get() {
                return;
            }
            if refazer.replace(false) {
                aberto.set(true);
            } else {
                untrack(|| {
                    senha.set(String::new());
                    errou.set(false);
                });
            }
        });
    }
    move || {
        let (cofre, refazer) = (cofre.clone(), Rc::clone(&refazer));
        ContentDialog::new(t!("cofre-desbloquear-o-cofre"), aberto)
            .id(id.as_str())
            .content(
                Column::new()
                    .gap(8)
                    .child(
                        Text::new(t!("cofre-digite-a-senha-mestra-do-cofre"))
                            .secondary()
                            .wrap(),
                    )
                    .child(
                        Edit::bind(senha)
                            .id(format!("{id}-campo"))
                            .password()
                            .placeholder(t!("cofre-senha-mestra")),
                    )
                    .child(
                        Text::new(t!("cofre-senha-mestra-incorreta"))
                            .id(format!("{id}-erro"))
                            .caption()
                            .visible(move || errou.get()),
                    ),
            )
            .primary(t!("comum-desbloquear"), move || {
                let certa = cofre.mudar(|c| c.desbloquear(&senha.get_untracked()));
                senha.set(String::new());
                errou.set(!certa);
                refazer.set(!certa);
            })
            .close(t!("comum-agora-nao"))
    }
}

// --- A página ---

/// Uma grade de propriedades: o item e a geração, que sobe quando o item muda por fora da
/// grade e ela precisa ser refeita.
#[derive(Clone)]
struct Grade {
    id: u64,
    geracao: u64,
    /// O item é uma nota: a grade perde o campo "Notas" e ganha a prévia em Markdown.
    nota: bool,
}

const PREFIXO_ITEM: &str = "item:";
const PREFIXO_PASTA: &str = "pasta:";
/// A chave da raiz do cofre, na árvore e como destino de mover.
const RAIZ: &str = "raiz";

/// A chave da pasta `id` (`None` é a raiz) na árvore e na lista.
fn chave_da_pasta(id: Option<u64>) -> String {
    match id {
        Some(id) => format!("{PREFIXO_PASTA}{id}"),
        None => RAIZ.to_owned(),
    }
}

/// O número de uma chave `prefixo<id>`.
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

/// Separa as chaves da lista em ids de pastas e de itens.
fn separar(chaves: &[String]) -> (Vec<u64>, Vec<u64>) {
    let pastas = chaves
        .iter()
        .filter_map(|c| id_da_chave(c, PREFIXO_PASTA))
        .collect();
    let itens = chaves
        .iter()
        .filter_map(|c| id_da_chave(c, PREFIXO_ITEM))
        .collect();
    (pastas, itens)
}

/// "1 item", "3 itens": `chave` é uma das `comum-n-*`, que traz o singular e o plural.
pub(crate) fn contagem(n: usize, chave: &str) -> String {
    t!(chave, n = n as i64)
}

/// Junta com vírgulas e "e" antes do último: "a", "a e b", "a, b e c".
pub(crate) fn juntar(partes: &[String]) -> String {
    match partes {
        [] => String::new(),
        [so] => so.clone(),
        [ate @ .., ultimo] => {
            let chave = if ate.len() == 1 {
                tk!("comum-juntar-dois")
            } else {
                tk!("comum-juntar-fim")
            };
            t!(chave, ate = ate.join(", "), ultimo = ultimo.as_str())
        }
    }
}

/// O texto do diálogo de apagar. `pastas` e `itens` são o total que sai do cofre (as escolhidas
/// e as de dentro delas); `so_uma_pasta` diz que a seleção é uma pasta só, que o texto chama de
/// "a pasta".
pub(crate) fn resumo_apagar(pastas: usize, itens: usize, so_uma_pasta: bool) -> String {
    if pastas == 0 && itens == 1 {
        return t!("cofre-apagar-um-item");
    }
    let mut partes = Vec::new();
    if so_uma_pasta {
        partes.push(t!("cofre-apagar-a-pasta"));
        if pastas > 1 {
            partes.push(contagem(pastas - 1, tk!("comum-n-subpastas")));
        }
    } else if pastas > 0 {
        partes.push(contagem(pastas, tk!("comum-n-pastas")));
    }
    if itens > 0 {
        partes.push(contagem(itens, tk!("comum-n-itens")));
    }
    let singular = partes.len() == 1 && (so_uma_pasta || pastas + itens == 1);
    t!(
        "cofre-apagar-resumo",
        lista = juntar(&partes),
        n = if singular { 1 } else { 2 }
    )
}

/// O que o apagar leva: as pastas (as escolhidas e as de dentro) e os itens (os de dentro e
/// os soltos, sem contar duas vezes).
fn o_que_sai(cofre: &Cofre, chaves: &[String]) -> (BTreeSet<u64>, BTreeSet<u64>) {
    let (pastas, itens) = separar(chaves);
    let mut pastas_fora: BTreeSet<u64> = BTreeSet::new();
    let mut itens_fora: BTreeSet<u64> = itens
        .into_iter()
        .filter(|&i| cofre.item(i).is_some())
        .collect();
    for &pasta in &pastas {
        if cofre.pasta(pasta).is_none() {
            continue;
        }
        pastas_fora.insert(pasta);
        let (subpastas, dentro) = cofre.conteudo_de_pasta(pasta);
        pastas_fora.extend(subpastas);
        itens_fora.extend(dentro);
    }
    (pastas_fora, itens_fora)
}

/// O que o apagar leva: quantas pastas (as escolhidas e as de dentro), quantos itens (os de
/// dentro e os soltos, sem contar duas vezes) e se a seleção é uma pasta só.
pub(crate) fn contar_apagar(cofre: &Cofre, chaves: &[String]) -> (usize, usize, bool) {
    let (pastas, _) = separar(chaves);
    let (pastas_fora, itens_fora) = o_que_sai(cofre, chaves);
    (
        pastas_fora.len(),
        itens_fora.len(),
        chaves.len() == 1 && pastas.len() == 1,
    )
}

/// Se o apagar leva uma chave dos logs cifrados (M53): sem ela, os logs gravados com ela
/// não se leem mais.
pub(crate) fn apagar_leva_chave_dos_logs(cofre: &Cofre, chaves: &[String]) -> bool {
    let (_, itens) = o_que_sai(cofre, chaves);
    let dos_logs = cofre.chaves_dos_logs();
    dos_logs.iter().any(|chave| {
        cofre
            .item_por_uid(&chave.uid)
            .is_some_and(|item| itens.contains(&item.id))
    })
}

/// As mães de `pasta`, da mais próxima à mais distante.
pub(crate) fn maes(cofre: &Cofre, pasta: Option<u64>) -> Vec<u64> {
    let mut maes = Vec::new();
    let mut atual = pasta.and_then(|id| cofre.pasta(id)).and_then(|p| p.pai);
    while let Some(id) = atual {
        // O limite protege de um ciclo que o cofre não pegou.
        if maes.len() > cofre.pastas().len() {
            break;
        }
        maes.push(id);
        atual = cofre.pasta(id).and_then(|p| p.pai);
    }
    maes
}

/// O estado da página e as ações dos botões.
struct Pagina {
    cofre: CofreCompartilhado,
    /// A pasta cuja lista aparece (`None` é a raiz).
    pasta_aberta: Signal<Option<u64>>,
    selecao_arvore: Signal<Option<String>>,
    abertos: Signal<BTreeSet<String>>,
    /// As linhas escolhidas na lista.
    selecao: Signal<BTreeSet<String>>,
    /// A linha da lista em edição de nome.
    renomeando: Signal<Option<String>>,
    /// A pasta da árvore em edição de nome.
    renomeando_arvore: Signal<Option<String>>,
    filtro: Signal<String>,
    aviso: Signal<Option<(InfoBarSeverity, String)>>,
    geracao: Signal<u64>,
    apagar_aberto: Signal<bool>,
    apagar_chaves: Signal<Vec<String>>,
    mover_aberto: Signal<bool>,
    mover_chaves: Signal<Vec<String>>,
    mover_destino: Signal<Option<String>>,
    /// Leva o item de chave SSH ao "Exportar como…" da página Chaves SSH, se a casca tem uma.
    exportar_chave: Option<Rc<dyn Fn(u64)>>,
    /// O "Levar o cofre…" e o "Abrir cofre de outra máquina…" do menu "Mais", se a casca tem.
    portabilidade: Option<Portabilidade>,
    /// Abre a nota `id` na página Notas, se a casca tem uma (M47).
    abrir_nota: Option<Rc<dyn Fn(u64)>>,
    /// O que o cofre roda antes de bloquear (esvaziar as imagens, cancelar o que anda); o cofre
    /// só guarda uma referência fraca.
    _antes_de_bloquear: Rc<dyn Fn()>,
    /// As imagens decifradas da nota que está na tela.
    imagens: Imagens,
    /// A gravação dos arquivos dos anexos ao lado do JSON exportado (M47).
    exportando: Andamento<usize>,
    /// A cifragem dos arquivos dos anexos de um JSON importado (M47).
    importando: Andamento<(Lote, Trazidos)>,
}

/// O que a casca faz com o cofre entre máquinas (M44): os dois itens do menu "Mais".
#[derive(Clone)]
pub struct Portabilidade {
    /// "Levar o cofre para outra máquina…".
    pub levar: Rc<dyn Fn()>,
    /// "Abrir cofre de outra máquina…".
    pub abrir: Rc<dyn Fn()>,
}

impl Pagina {
    fn informar(&self, mensagem: impl Into<String>) {
        self.aviso
            .set(Some((InfoBarSeverity::Informational, mensagem.into())));
    }

    fn avisar(&self, severidade: InfoBarSeverity, mensagem: impl Into<String>) {
        self.aviso.set(Some((severidade, mensagem.into())));
    }

    fn falhar(&self, mensagem: impl Into<String>) {
        self.aviso
            .set(Some((InfoBarSeverity::Error, mensagem.into())));
    }

    fn falhou_ao_gravar(&self, erro: impl std::fmt::Display) {
        self.falhar(t!(
            "comum-nao-consegui-gravar-o-cofre",
            erro = erro.to_string()
        ));
    }

    /// O id do item escolhido na lista, se for um só e ele ainda existir.
    fn item_escolhido(&self) -> Option<u64> {
        let selecao = self.selecao.get_untracked();
        if selecao.len() != 1 {
            return None;
        }
        let id = id_da_chave(selecao.iter().next()?, PREFIXO_ITEM)?;
        self.cofre.agora(|c| c.item(id).map(|_| id))
    }

    /// Abre a pasta `id` (`None` é a raiz): escolhe na árvore, abre as mães e a própria pasta, e
    /// limpa a seleção e o renomear da lista se a pasta mudou.
    fn abrir_pasta(&self, id: Option<u64>) {
        let mut abrir: Vec<String> = self.cofre.agora(|c| {
            let mut v: Vec<String> = maes(c, id)
                .into_iter()
                .map(|m| chave_da_pasta(Some(m)))
                .collect();
            v.push(chave_da_pasta(id));
            v
        });
        abrir.push(RAIZ.to_owned());
        self.abertos.update(|a| a.extend(abrir));
        if self.pasta_aberta.get_untracked() != id {
            self.pasta_aberta.set(id);
            self.selecao.update(|s| s.clear());
            self.renomeando.set(None);
        }
        let chave = chave_da_pasta(id);
        if self.selecao_arvore.get_untracked().as_ref() != Some(&chave) {
            self.selecao_arvore.set(Some(chave));
        }
    }

    /// Sobe para a mãe da pasta aberta.
    fn subir(&self) {
        let Some(id) = self.pasta_aberta.get_untracked() else {
            return;
        };
        let pai = self.cofre.agora(|c| c.pasta(id).and_then(|p| p.pai));
        self.abrir_pasta(pai);
    }

    /// Escolhe só a linha `chave` da lista.
    fn escolher_linha(&self, chave: String) {
        self.selecao.set(BTreeSet::from([chave]));
    }

    /// Grava `item`; devolve o id, ou avisa o erro.
    fn salvar(&self, item: Item) -> Option<u64> {
        match self.cofre.mudar(|c| c.salvar(item)) {
            Ok(id) => Some(id),
            Err(erro) => {
                self.falhou_ao_gravar(erro);
                None
            }
        }
    }

    fn novo(&self) {
        self.aviso.set(None);
        self.filtro.set(String::new());
        let item = Item {
            tipo: Tipo::Senha,
            titulo: t!("cofre-novo-item"),
            pasta: self.pasta_aberta.get_untracked(),
            ..Item::default()
        };
        if let Some(id) = self.salvar(item) {
            self.entrar_em_renomear(format!("{PREFIXO_ITEM}{id}"));
        }
    }

    /// Escolhe a linha na lista e abre o campo de nome nela.
    fn entrar_em_renomear(&self, chave: String) {
        self.escolher_linha(chave.clone());
        self.renomeando.set(Some(chave));
    }

    fn nova_pasta(&self) {
        self.nova_pasta_em(self.pasta_aberta.get_untracked());
    }

    /// Cria "Nova pasta" dentro de `pai` e já abre o campo de nome nela.
    fn nova_pasta_em(&self, pai: Option<u64>) {
        self.aviso.set(None);
        self.filtro.set(String::new());
        match self
            .cofre
            .mudar(|c| c.nova_pasta(pai, &t!("comum-nova-pasta")))
        {
            Ok(id) => {
                self.abrir_pasta(pai);
                self.entrar_em_renomear(chave_da_pasta(Some(id)));
            }
            Err(erro) => self.falhou_ao_gravar(erro),
        }
    }

    fn duplicar(&self) {
        if let Some(id) = self.item_escolhido() {
            self.duplicar_item(id);
        }
    }

    fn duplicar_item(&self, id: u64) {
        self.aviso.set(None);
        let Some(mut item) = self.cofre.agora(|c| c.item(id).cloned()) else {
            return;
        };
        item.id = 0;
        item.titulo = t!("comum-titulo-copia", titulo = item.titulo.as_str());
        if let Some(id) = self.salvar(item) {
            self.entrar_em_renomear(format!("{PREFIXO_ITEM}{id}"));
        }
    }

    fn copiar_usuario(&self) {
        if let Some(id) = self.item_escolhido() {
            self.copiar_usuario_de(id);
        }
    }

    fn copiar_usuario_de(&self, id: u64) {
        if let Some(item) = self.cofre.agora(|c| c.item(id).cloned()) {
            clipboard::write(item.usuario);
            self.informar(t!("cofre-usuario-copiado"));
        }
    }

    fn copiar_senha(&self) {
        if let Some(id) = self.item_escolhido() {
            self.copiar_senha_de(id);
        }
    }

    fn copiar_senha_de(&self, id: u64) {
        if let Some(item) = self.cofre.agora(|c| c.item(id).cloned()) {
            self.cofre.copiar_segredo(item.senha);
            self.informar(t!("cofre-senha-copiada"));
        }
    }

    /// O clique duplo: numa pasta, abre; numa nota, abre a página Notas (se a casca tem uma);
    /// nos outros itens, copia a senha.
    fn invocar(&self, chave: &str) {
        if let Some(id) = id_da_chave(chave, PREFIXO_PASTA) {
            self.abrir_pasta(Some(id));
        } else if let Some(id) = id_da_chave(chave, PREFIXO_ITEM) {
            self.escolher_linha(chave.to_owned());
            let e_nota = self
                .cofre
                .agora(|c| c.item(id).is_some_and(|i| i.tipo == Tipo::Nota));
            match (&self.abrir_nota, e_nota) {
                (Some(abrir), true) => abrir(id),
                _ => self.copiar_senha_de(id),
            }
        }
    }

    /// O texto digitado no campo de nome da lista.
    fn renomear(&self, chave: &str, texto: &str) {
        self.aviso.set(None);
        if let Some(id) = id_da_chave(chave, PREFIXO_ITEM) {
            let titulo = texto.trim();
            let Some(mut item) = self.cofre.agora(|c| c.item(id).cloned()) else {
                return;
            };
            if titulo.is_empty() || item.titulo == titulo {
                return;
            }
            item.titulo = titulo.to_owned();
            if self.salvar(item).is_some() {
                self.geracao.update(|g| *g += 1);
            }
        } else if let Some(id) = id_da_chave(chave, PREFIXO_PASTA) {
            self.renomear_pasta(id, texto);
        }
    }

    fn renomear_pasta(&self, id: u64, texto: &str) {
        self.aviso.set(None);
        if let Err(erro) = self.cofre.mudar(|c| c.renomear_pasta(id, texto)) {
            self.falhar(erro.to_string());
        }
    }

    /// Leva as linhas `chaves` para `destino` (`raiz` ou `pasta:<id>`). Mover para onde já está
    /// não muda nada e não avisa.
    fn soltar(&self, chaves: &[String], destino: &str) {
        self.aviso.set(None);
        let Some(destino) = destino_da_chave(destino) else {
            return;
        };
        let (pastas, itens) = separar(chaves);
        if pastas.is_empty() && itens.is_empty() {
            return;
        }
        self.selecao.update(|s| s.clear());
        if let Err(erro) = self.cofre.mudar(|c| c.mover(&pastas, &itens, destino)) {
            self.falhar(erro.to_string());
        }
    }

    /// Se `alvo` aceita as linhas `chaves`: precisa ser a raiz ou uma pasta que não esteja entre
    /// as arrastadas nem embaixo delas.
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

    /// As linhas a que "Mover para…" e "Apagar" do menu de contexto valem: a seleção, se a linha
    /// clicada está nela (o `DataGrid` escolhe a linha antes de abrir o menu), senão só a linha.
    fn chaves_do_menu(&self, chave: &str) -> Vec<String> {
        let selecao = self.selecao.get_untracked();
        if selecao.contains(chave) {
            selecao.into_iter().collect()
        } else {
            vec![chave.to_owned()]
        }
    }

    fn mover_escolhidas(&self) {
        let chaves: Vec<String> = self.selecao.get_untracked().into_iter().collect();
        self.pedir_mover(chaves);
    }

    fn apagar_escolhidas(&self) {
        let chaves: Vec<String> = self.selecao.get_untracked().into_iter().collect();
        self.pedir_apagar(chaves);
    }

    /// Abre o diálogo "Mover para…" com a pasta atual das linhas escolhida.
    fn pedir_mover(&self, chaves: Vec<String>) {
        let chaves: Vec<String> = chaves.into_iter().filter(|c| c != RAIZ).collect();
        let Some(primeira) = chaves.first() else {
            return;
        };
        self.aviso.set(None);
        let atual = self.cofre.agora(|c| {
            if let Some(id) = id_da_chave(primeira, PREFIXO_ITEM) {
                c.item(id).and_then(|i| i.pasta)
            } else {
                id_da_chave(primeira, PREFIXO_PASTA)
                    .and_then(|id| c.pasta(id))
                    .and_then(|p| p.pai)
            }
        });
        self.mover_destino.set(Some(chave_da_pasta(atual)));
        self.mover_chaves.set(chaves);
        self.mover_aberto.set(true);
    }

    /// Abre o diálogo de apagar.
    fn pedir_apagar(&self, chaves: Vec<String>) {
        let chaves: Vec<String> = chaves.into_iter().filter(|c| c != RAIZ).collect();
        if chaves.is_empty() {
            return;
        }
        self.aviso.set(None);
        self.apagar_chaves.set(chaves);
        self.apagar_aberto.set(true);
    }

    /// O "Apagar" do diálogo: leva as pastas com tudo o que têm dentro, e os itens soltos. Se a
    /// pasta aberta saiu, abre a mãe que sobrou.
    fn apagar(&self) {
        self.aviso.set(None);
        let chaves = self.apagar_chaves.get_untracked();
        let (pastas, itens) = separar(&chaves);
        let aberta = self.pasta_aberta.get_untracked();
        let maes_da_aberta = self.cofre.agora(|c| maes(c, aberta));
        match self.cofre.mudar(|c| c.apagar(&pastas, &itens)) {
            Ok(()) => {
                self.selecao.update(|s| s.clear());
                if aberta.is_some_and(|id| self.cofre.agora(|c| c.pasta(id).is_none())) {
                    let sobrou = self
                        .cofre
                        .agora(|c| maes_da_aberta.into_iter().find(|&m| c.pasta(m).is_some()));
                    self.abrir_pasta(sobrou);
                }
            }
            Err(erro) => self.falhou_ao_gravar(erro),
        }
    }

    /// Põe `senha` no item escolhido (ou num item novo, se nenhum está escolhido).
    fn usar_senha(&self, senha: String) {
        self.aviso.set(None);
        let item = self
            .item_escolhido()
            .and_then(|id| self.cofre.agora(|c| c.item(id).cloned()));
        match item {
            Some(mut item) => {
                item.senha = senha;
                if self.salvar(item).is_some() {
                    self.geracao.update(|g| *g += 1);
                }
            }
            None => {
                let item = Item {
                    tipo: Tipo::Senha,
                    titulo: t!("cofre-novo-item"),
                    pasta: self.pasta_aberta.get_untracked(),
                    senha,
                    ..Item::default()
                };
                if let Some(id) = self.salvar(item) {
                    self.escolher_linha(format!("{PREFIXO_ITEM}{id}"));
                }
            }
        }
    }

    /// Acrescenta um campo ao item escolhido.
    fn novo_campo(&self, nome: String, oculto: bool) {
        self.aviso.set(None);
        let nome = nome.trim().to_owned();
        let Some(mut item) = self
            .item_escolhido()
            .and_then(|id| self.cofre.agora(|c| c.item(id).cloned()))
        else {
            return;
        };
        if nome.is_empty() {
            return;
        }
        item.campos.push(Campo {
            nome,
            valor: String::new(),
            oculto,
        });
        if self.salvar(item).is_some() {
            self.geracao.update(|g| *g += 1);
        }
    }

    /// "Importar": um JSON (do Tirech Omni ou do Bitwarden) ou um CSV, para dentro da pasta aberta.
    /// Os anexos do JSON do Tirech Omni vêm da pasta `<nome>-attachments` ao lado dele.
    fn importar(self: &Rc<Self>) {
        self.aviso.set(None);
        let pagina = Rc::clone(self);
        dialogs::open_file()
            .remember("cofre-importar")
            .title(t!("cofre-importar-para-o-cofre"))
            .filter(t!("cofre-json-ou-csv"), &["json", "csv"])
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let texto = match std::fs::read_to_string(&caminho) {
                    Ok(texto) => texto,
                    Err(erro) => {
                        pagina.falhar(t!(
                            "comum-nao-consegui-ler",
                            arquivo = caminho.display().to_string(),
                            erro = erro.to_string()
                        ));
                        return;
                    }
                };
                let (lote, formato, avisos) = match cofre_json::importar_arquivo(&texto) {
                    Ok(lido) => lido,
                    Err(erro) => {
                        pagina.falhar(erro);
                        return;
                    }
                };
                if lote.itens.iter().all(|i| i.anexos.is_empty()) {
                    pagina.concluir_importacao(lote, &formato, avisos, Trazidos::default());
                    return;
                }
                let vault = pagina.vault();
                let pasta = caminho.parent().map(Path::to_path_buf).unwrap_or_default();
                pagina.cofre.marcar_uso();
                let fraca = Rc::downgrade(&pagina);
                pagina.importando.iniciar(
                    t!("cofre-cifrando-anexos-do-json"),
                    move |progresso, cancelado| {
                        let mut lote = lote;
                        let trazidos = cofre_json::trazer_anexos(
                            &mut lote, &pasta, &vault, progresso, cancelado,
                        )?;
                        Ok((lote, trazidos))
                    },
                    move |resultado| {
                        let Some(pagina) = fraca.upgrade() else {
                            return;
                        };
                        match resultado {
                            Ok((lote, trazidos)) => {
                                pagina.concluir_importacao(lote, &formato, avisos, trazidos);
                            }
                            Err(ErroAnexo::Cancelado) => {
                                pagina.informar(t!("cofre-importacao-cancelada"))
                            }
                            Err(erro) => {
                                pagina.falhar(t!(
                                    "cofre-nao-foi-possivel-importar",
                                    erro = erro.to_string()
                                ));
                            }
                        }
                    },
                );
            });
    }

    /// A pasta `vault/` do cofre.
    fn vault(&self) -> PathBuf {
        self.cofre.agora(|c| c.pasta_do_cofre().join(anexos::PASTA))
    }

    /// Põe o `lote` (já com os anexos cifrados em `vault/`) no cofre e conta o que veio.
    fn concluir_importacao(
        &self,
        lote: Lote,
        formato: &str,
        avisos: Vec<String>,
        trazidos: Trazidos,
    ) {
        let (itens, pastas) = (lote.itens.len(), lote.pastas.len());
        let novos = cofre_json::ids_dos_anexos(&lote);
        let destino = self.pasta_aberta.get_untracked();
        match self.cofre.mudar(|c| c.importar(lote, destino)) {
            Ok(_) => {
                let onde = self.cofre.agora(|c| c.caminho(destino));
                let onde = if onde.is_empty() {
                    t!("cofre-na-raiz")
                } else {
                    t!("cofre-em-pasta", pasta = onde)
                };
                let mut mensagem = t!(
                    "cofre-importado",
                    n = itens as i64,
                    itens = contagem(itens, tk!("comum-n-itens")),
                    pastas = contagem(pastas, tk!("comum-n-pastas")),
                    formato = formato,
                    onde = onde
                );
                if trazidos.trazidos > 0 {
                    mensagem.push(' ');
                    mensagem.push_str(&t!(
                        "cofre-anexos-vieram-junto",
                        n = trazidos.trazidos as i64,
                        anexos = contagem(trazidos.trazidos, tk!("comum-n-anexos"))
                    ));
                }
                for aviso in avisos {
                    mensagem.push('\n');
                    mensagem.push_str(&aviso);
                }
                if trazidos.nao_encontrados > 0 {
                    mensagem.push('\n');
                    mensagem.push_str(&t!(
                        "cofre-anexos-nao-encontrados",
                        n = trazidos.nao_encontrados as i64
                    ));
                    self.avisar(InfoBarSeverity::Warning, mensagem);
                } else {
                    self.informar(mensagem);
                }
            }
            Err(erro) => {
                cofre_json::descartar(&self.vault(), &novos);
                self.cofre.mudar(|c| c.esquecer_anexos(&novos));
                self.falhou_ao_gravar(erro);
            }
        }
    }

    /// "Exportar": o cofre inteiro, com as senhas, no JSON do Tirech Omni ou no do Bitwarden. Os
    /// anexos do JSON do Tirech Omni vão em claro para `<nome>-attachments`, ao lado dele.
    fn exportar(self: &Rc<Self>, no_bitwarden: bool) {
        self.aviso.set(None);
        let pagina = Rc::clone(self);
        dialogs::save_file()
            .remember("cofre-exportar")
            .title(if no_bitwarden {
                t!("cofre-exportar-bitwarden")
            } else {
                t!("cofre-exportar-tirech")
            })
            .filter("JSON", &["json"])
            .file_name(if no_bitwarden {
                "bitwarden.json"
            } else {
                "tirech-omni-vault.json"
            })
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let (exportacao, itens, pastas) = pagina.cofre.agora(|c| {
                    let exportacao = if no_bitwarden {
                        cofre_json::Exportacao {
                            texto: bitwarden::exportar(c),
                            arquivos: Vec::new(),
                            ausentes: 0,
                        }
                    } else {
                        cofre_json::exportar(c, &caminho)
                    };
                    (exportacao, c.itens().len(), c.pastas().len())
                });
                let resumo = t!(
                    "cofre-exportado",
                    n = itens as i64,
                    itens = contagem(itens, tk!("comum-n-itens")),
                    pastas = contagem(pastas, tk!("comum-n-pastas"))
                );
                let cofre_json::Exportacao {
                    texto,
                    arquivos,
                    ausentes,
                } = exportacao;
                if arquivos.is_empty() {
                    pagina.gravar_exportacao(&caminho, &texto, resumo, 0, ausentes);
                    return;
                }
                let (vault, destino) = (pagina.vault(), caminho.clone());
                pagina.cofre.marcar_uso();
                let fraca = Rc::downgrade(&pagina);
                pagina.exportando.iniciar(
                    t!("cofre-gravando-anexos-ao-lado"),
                    move |progresso, cancelado| {
                        cofre_json::gravar_arquivos(
                            &arquivos, &vault, &destino, progresso, cancelado,
                        )
                    },
                    move |resultado| {
                        let Some(pagina) = fraca.upgrade() else {
                            return;
                        };
                        match resultado {
                            Ok(gravados) => {
                                pagina.gravar_exportacao(
                                    &caminho, &texto, resumo, gravados, ausentes,
                                );
                            }
                            Err(ErroAnexo::Cancelado) => {
                                pagina.informar(t!("cofre-exportacao-cancelada"))
                            }
                            Err(erro) => {
                                pagina.falhar(t!(
                                    "cofre-nao-foi-possivel-exportar",
                                    erro = erro.to_string()
                                ));
                            }
                        }
                    },
                );
            });
    }

    /// Grava o texto do JSON (depois dos anexos, para cancelar não deixar nada) e avisa.
    fn gravar_exportacao(
        &self,
        caminho: &Path,
        texto: &str,
        resumo: String,
        anexos: usize,
        ausentes: usize,
    ) {
        if let Err(erro) = std::fs::write(caminho, texto) {
            self.falhar(t!(
                "comum-nao-consegui-gravar",
                arquivo = caminho.display().to_string(),
                erro = erro.to_string()
            ));
            return;
        }
        let mut mensagem = resumo;
        if anexos > 0 {
            mensagem.push(' ');
            mensagem.push_str(&t!(
                "cofre-anexos-exportados-em",
                anexos = contagem(anexos, tk!("comum-n-anexos")),
                pasta = cofre_json::pasta_dos_anexos(caminho)
            ));
        }
        if ausentes > 0 {
            mensagem.push(' ');
            mensagem.push_str(&t!(
                "cofre-anexos-ausentes",
                n = ausentes as i64,
                anexos = contagem(ausentes, tk!("comum-n-anexos"))
            ));
            self.avisar(InfoBarSeverity::Warning, mensagem);
        } else {
            self.informar(mensagem);
        }
    }
}

/// O rótulo do item na lista: o título, ou "(sem título)".
pub(crate) fn rotulo(item: &Item) -> String {
    if item.titulo.trim().is_empty() {
        t!("comum-sem-titulo")
    } else {
        item.titulo.clone()
    }
}

/// O símbolo do tipo do item na lista.
fn simbolo(tipo: Tipo) -> Symbol {
    match tipo {
        Tipo::Senha => Symbol::LockClosed,
        Tipo::Ssh => Symbol::Server,
        Tipo::Sftp | Tipo::Ftp => Symbol::Folder,
        Tipo::S3 | Tipo::Rclone => Symbol::Cloud,
        Tipo::Chave | Tipo::ChaveSsh => Symbol::Key,
        Tipo::Nota => Symbol::DocumentText,
        Tipo::Nebula => Symbol::Globe,
        Tipo::OpenVpn | Tipo::WireGuard => Symbol::Shield,
        Tipo::Tailscale | Tipo::ZeroTier => Symbol::Globe,
    }
}

/// As pastas de dentro de `pai` como linhas da `TreeView`, com as filhas delas.
fn pastas_da_arvore(cofre: &Cofre, pai: Option<u64>) -> Vec<TreeItem> {
    cofre
        .filhas(pai)
        .into_iter()
        .map(|p| {
            TreeItem::folder(chave_da_pasta(Some(p.id)), p.nome.clone())
                .children(pastas_da_arvore(cofre, Some(p.id)))
        })
        .collect()
}

/// A árvore das pastas: uma raiz "Cofre" com todas as pastas embaixo.
fn itens_da_arvore(cofre: &Cofre) -> Vec<TreeItem> {
    vec![TreeItem::folder(RAIZ, t!("comum-cofre")).children(pastas_da_arvore(cofre, None))]
}

/// A chave da raiz e de todas as pastas, para abrir a árvore do "Mover para…" inteira.
fn chaves_das_pastas(cofre: &Cofre) -> BTreeSet<String> {
    cofre
        .pastas()
        .iter()
        .map(|p| chave_da_pasta(Some(p.id)))
        .chain([RAIZ.to_owned()])
        .collect()
}

fn linha_de_pasta(pasta: &Pasta, local: String) -> DataGridRow {
    DataGridRow::new(chave_da_pasta(Some(pasta.id)))
        .cell(pasta.nome.clone(), CellValue::Text(pasta.nome.clone()))
        .cell("", CellValue::Text(String::new()))
        .cell("", CellValue::Text(String::new()))
        .cell(local.clone(), CellValue::Text(local))
        .cell("", CellValue::Time(None))
        .icon(Symbol::Folder)
        .rank(0)
        .drop_target()
}

fn linha_de_item(item: &Item, local: String) -> DataGridRow {
    let titulo = rotulo(item);
    let alterado = (item.alterado > 0).then(|| UNIX_EPOCH + Duration::from_secs(item.alterado));
    DataGridRow::new(format!("{PREFIXO_ITEM}{}", item.id))
        .cell(titulo.clone(), CellValue::Text(titulo))
        .cell(item.usuario.clone(), CellValue::Text(item.usuario.clone()))
        .cell(item.url.clone(), CellValue::Text(item.url.clone()))
        .cell(local.clone(), CellValue::Text(local))
        .cell(formato::data(alterado), CellValue::Time(alterado))
        .icon(simbolo(item.tipo))
        .rank(1)
}

/// As linhas da lista. Sem `filtro`, a pasta `aberta` (`None` é a raiz) com as subpastas e os
/// itens dela. Com `filtro`, tudo o que casa na pasta e embaixo dela: o nome das pastas e o
/// título, o usuário, o endereço e as notas dos itens, sem diferenciar maiúsculas nem acentos;
/// a coluna "Pasta" leva o caminho relativo à pasta aberta.
fn linhas_da_lista(cofre: &Cofre, aberta: Option<u64>, filtro: &str) -> Vec<DataGridRow> {
    let filtro = normalizar(filtro.trim());
    if filtro.is_empty() {
        let pastas = cofre
            .filhas(aberta)
            .into_iter()
            .map(|p| linha_de_pasta(p, String::new()));
        let itens = cofre
            .itens_em(aberta)
            .into_iter()
            .map(|i| linha_de_item(i, String::new()));
        return pastas.chain(itens).collect();
    }
    let base = cofre.caminho(aberta);
    let relativo = |pasta: Option<u64>| -> String {
        let completo = cofre.caminho(pasta);
        match completo.strip_prefix(&base) {
            Some(resto) if !base.is_empty() => resto.trim_start_matches('/').to_owned(),
            _ => completo,
        }
    };
    let dentro = |pasta: Option<u64>| match aberta {
        None => true,
        Some(a) => pasta == Some(a) || maes(cofre, pasta).contains(&a),
    };
    let casa = |texto: &str| normalizar(texto).contains(&filtro);
    let mut linhas = Vec::new();
    for pasta in cofre.pastas() {
        if dentro(pasta.pai) && casa(&pasta.nome) {
            linhas.push(linha_de_pasta(pasta, relativo(pasta.pai)));
        }
    }
    for item in cofre.itens() {
        if dentro(item.pasta)
            && (casa(&item.titulo) || casa(&item.usuario) || casa(&item.url) || casa(&item.notas))
        {
            linhas.push(linha_de_item(item, relativo(item.pasta)));
        }
    }
    linhas
}

/// O texto no lugar dos detalhes, quando a seleção não é um item só.
fn texto_sem_item(cofre: &Cofre, selecao: &BTreeSet<String>) -> String {
    match selecao.len() {
        0 => t!("cofre-escolha-um-item"),
        1 => selecao
            .iter()
            .next()
            .and_then(|chave| id_da_chave(chave, PREFIXO_PASTA))
            .and_then(|id| cofre.pasta(id))
            .map(|pasta| {
                t!(
                    "cofre-pasta-resumo",
                    nome = pasta.nome.as_str(),
                    subpastas =
                        contagem(cofre.filhas(Some(pasta.id)).len(), tk!("comum-n-subpastas")),
                    itens = contagem(cofre.itens_em(Some(pasta.id)).len(), tk!("comum-n-itens"))
                )
            })
            .unwrap_or_default(),
        n => t!("cofre-n-itens-escolhidos", n = n as i64),
    }
}

/// A grade de propriedades do item `id`. Cada mudança grava na hora.
fn grade_do_item(pagina: &Rc<Pagina>, id: u64) -> impl View + use<> {
    let cofre = pagina.cofre.clone();
    let item = cofre.agora(|c| c.item(id).cloned()).unwrap_or_default();
    let painel_da_chave = item
        .chave
        .clone()
        .map(|chave| painel_da_chave_ssh(pagina, id, &chave));
    let painel_da_nebula = item
        .nebula
        .clone()
        .map(|identidade| painel_da_nebula(pagina, id, &identidade));
    let painel_do_openvpn = item
        .openvpn
        .clone()
        .map(|identidade| painel_do_openvpn(pagina, id, &identidade));
    let painel_do_wireguard = item
        .wireguard
        .clone()
        .map(|identidade| painel_do_wireguard(&identidade));
    let painel_do_tailscale = item
        .tailscale
        .clone()
        .map(|identidade| painel_do_tailscale(pagina, id, &identidade));
    let painel_do_zerotier = item
        .zerotier
        .clone()
        .map(|identidade| painel_do_zerotier(pagina, id, &identidade));
    let tipo = signal(item.tipo.nome().to_owned());
    let titulo = signal(item.titulo.clone());
    let usuario = signal(item.usuario.clone());
    let senha = signal(item.senha.clone());
    let url = signal(item.url.clone());
    let notas = signal(item.notas.clone());
    let extras: Vec<Signal<String>> = item
        .campos
        .iter()
        .map(|c| signal(c.valor.clone()))
        .collect();

    {
        let pagina = Rc::clone(pagina);
        let extras = extras.clone();
        effect(move || {
            let tipo = tipo.get();
            let titulo = titulo.get();
            let usuario = usuario.get();
            let senha = senha.get();
            let url = url.get();
            let notas = notas.get();
            let valores: Vec<String> = extras.iter().map(|s| s.get()).collect();
            untrack(|| {
                let Some(atual) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                    return;
                };
                let mut novo = atual.clone();
                if let Some(tipo) = Tipo::do_nome(&tipo) {
                    novo.tipo = tipo;
                }
                novo.titulo = titulo;
                novo.usuario = usuario;
                novo.senha = senha;
                novo.url = url;
                novo.notas = notas;
                for (campo, valor) in novo.campos.iter_mut().zip(valores) {
                    campo.valor = valor;
                }
                if novo != atual {
                    pagina.salvar(novo);
                }
            });
        });
    }

    let e_chave = painel_da_chave.is_some();
    let e_nebula = painel_da_nebula.is_some();
    let e_openvpn = painel_do_openvpn.is_some();
    let e_rede_embutida = painel_do_wireguard.is_some()
        || painel_do_tailscale.is_some()
        || painel_do_zerotier.is_some();
    let mut grade = PropertyGrid::new()
        .id("cofre-propriedades")
        .grow(1)
        .category(t!("cofre-item"))
        .choice(t!("comum-tipo"), tipo, Tipo::TODOS.map(Tipo::nome))
        .default(Tipo::Senha.nome())
        .text(t!("comum-titulo"), titulo);
    // A identidade Nebula, a WireGuard e a Tailscale não têm usuário, senha nem URL: o painel delas
    // mostra o que importa.
    if !e_nebula && !e_rede_embutida {
        grade = grade.category(t!("cofre-acesso"));
    }
    grade = if e_nebula || e_rede_embutida {
        grade
    } else if e_openvpn {
        // O `auth-user-pass`: o usuário e a senha ficam nos campos do próprio item.
        grade
            .text(t!("comum-usuario"), usuario)
            .password(t!("comum-senha"), senha)
            .description(t!("openvpn-cofre-senha-descricao"))
    } else if e_chave {
        grade
            .password("Passphrase", senha)
            .description(t!("cofre-a-passphrase-que-a-chave-tinha"))
    } else {
        grade
            .text(t!("comum-usuario"), usuario)
            .password(t!("comum-senha"), senha)
            .text(t!("cofre-url"), url)
            .description(t!("cofre-o-host-o-endereco-do-site"))
    };
    let e_nota = item.tipo == Tipo::Nota;
    let mut grade = if e_nota {
        grade
    } else {
        grade
            .category(t!("comum-notas"))
            .text(t!("comum-notas"), notas)
    };
    if !item.campos.is_empty() {
        grade = grade.category(t!("cofre-campos"));
        for (campo, valor) in item.campos.iter().zip(extras) {
            grade = if campo.oculto {
                grade.password(campo.nome.clone(), valor)
            } else {
                grade.text(campo.nome.clone(), valor)
            };
        }
    }
    let mut coluna = Column::new().grow(1).gap(8).child(grade);
    if e_nota {
        coluna = coluna.child(previa_da_nota(pagina, id));
    }
    if let Some(painel) = painel_da_chave {
        coluna = coluna.child(painel);
    }
    if let Some(painel) = painel_da_nebula {
        coluna = coluna.child(painel);
    }
    if let Some(painel) = painel_do_openvpn {
        coluna = coluna.child(painel);
    }
    if let Some(painel) = painel_do_wireguard {
        coluna = coluna.child(painel);
    }
    if let Some(painel) = painel_do_tailscale {
        coluna = coluna.child(painel);
    }
    if let Some(painel) = painel_do_zerotier {
        coluna = coluna.child(painel);
    }
    coluna
}

/// O corpo da nota `id` em Markdown, só para ler (sem tarefas marcáveis), com os segredos
/// mascarados e as imagens dos anexos, e o botão que abre a página Notas (M47, 4.2).
fn previa_da_nota(pagina: &Rc<Pagina>, id: u64) -> impl View + use<> {
    let cofre = pagina.cofre.clone();
    let copia = pagina.cofre.clone();
    let imagens = pagina.imagens.clone();
    let abrir_nota = pagina.abrir_nota.clone();
    let abrir_imagem = pagina.abrir_nota.clone();
    let abrir_link = pagina.abrir_nota.clone();
    let fonte = Rc::new(Fonte {
        ficha: {
            let cofre = cofre.clone();
            Box::new(move |anexo| {
                cofre.agora(|c| {
                    c.item(id)
                        .and_then(|i| i.anexos.iter().find(|a| a.id == anexo).cloned())
                })
            })
        },
        vault: {
            let cofre = cofre.clone();
            Box::new(move || cofre.agora(|c| c.pasta_do_cofre().join(anexos::PASTA)))
        },
        gravando: signal(BTreeSet::new()),
        abrir: Box::new(move |_| {
            if let Some(abrir) = &abrir_imagem {
                abrir(id);
            }
        }),
    });
    let mostrados = signal(BTreeSet::<usize>::new());
    let previa = MarkdownView::new({
        let cofre = cofre.clone();
        move || {
            let _ = cofre.versao();
            cofre.agora(|c| c.item(id).map(|i| i.notas.clone()).unwrap_or_default())
        }
    })
    .id("cofre-nota-previa")
    // Os links só se seguem na página Notas, onde ficam o abridor e as notas ligadas.
    .on_link(move |_| {
        if let Some(abrir) = &abrir_link {
            abrir(id);
        }
    })
    .on_copy(move |texto| copia.copiar_segredo(texto))
    .image(move |cx| notas_imagens::desenhar(&imagens, &fonte, cx))
    .fence("segredo", move |cx| {
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
        let cofre = cofre.clone();
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
                        .otherwise(move || Text::new(notas::MASCARA).id(id_mascara.clone())),
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
    let mut coluna = Column::new()
        .gap(8)
        .child(Border::new().padding(8).child(previa));
    if let Some(abrir) = abrir_nota {
        coluna = coluna.child(
            Button::new(t!("cofre-abrir-em-notas"))
                .id("cofre-abrir-nota")
                .icon(Symbol::Notepad)
                .on_click(move || abrir(id)),
        );
    }
    coluna
}

/// O que o item de chave SSH mostra além das propriedades: o tipo, o fingerprint, a pública, a
/// política do agente e a privada (escondida até pedirem).
fn painel_da_chave_ssh(pagina: &Rc<Pagina>, id: u64, chave: &ChaveSsh) -> impl View + use<> {
    let detalhe = match keys::parse_public(&chave.publica) {
        Ok((publica, comentario)) => {
            let (tipo, bits) = keys::kind_and_bits(&publica);
            [tipo_legivel(&tipo, bits), comentario]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        }
        Err(_) => String::new(),
    };
    let publica = signal(chave.publica.trim().to_owned());
    let privada = signal(chave.privada.trim().to_owned());
    let tem_privada = !chave.privada.trim().is_empty();
    let token = chave.e_token();
    let mostrar = signal(false);
    let no_agente = signal(chave.no_agente);
    let confirmar = signal(chave.confirmar);
    let minutos = signal((chave.vida_secs / 60) as f64);

    {
        let pagina = Rc::clone(pagina);
        effect(move || {
            let (agente, confirmar, minutos) = (no_agente.get(), confirmar.get(), minutos.get());
            untrack(|| {
                let Some(atual) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                    return;
                };
                let mut novo = atual.clone();
                if let Some(chave) = novo.chave.as_mut() {
                    chave.no_agente = agente;
                    chave.confirmar = confirmar;
                    let vida = minutos.clamp(0.0, 10_080.0).round() as u64 * 60;
                    // Os segundos que não fecham minuto (de um arquivo importado) ficam como
                    // estão, a menos que a pessoa mexa no campo.
                    if vida / 60 != chave.vida_secs / 60 {
                        chave.vida_secs = vida;
                    }
                }
                if novo != atual {
                    pagina.salvar(novo);
                }
            });
        });
    }

    let copiar = move || clipboard::write(publica.get_untracked());
    let exportar = pagina.exportar_chave.clone();
    let mut acoes = Row::new().gap(8).wrap().child(
        Button::new(t!("comum-copiar-a-publica"))
            .id("cofre-chave-copiar")
            .icon(Symbol::Copy)
            .on_click(copiar),
    );
    if let Some(exportar) = exportar {
        acoes = acoes.child(
            Button::new(t!("cofre-exportar-como"))
                .id("cofre-chave-exportar")
                .icon(Symbol::ArrowUpload)
                .on_click(move || exportar(id)),
        );
    }
    let ver_privada = Column::new()
        .gap(4)
        .child(
            Button::new(move || {
                if mostrar.get() {
                    t!("cofre-ocultar-a-privada")
                } else {
                    t!("cofre-mostrar-a-privada")
                }
            })
            .id("cofre-chave-mostrar")
            .icon(Symbol::Key)
            .on_click(move || mostrar.update(|m| *m = !*m)),
        )
        .child(Show::when(mostrar, move || {
            Edit::bind(privada)
                .id("cofre-chave-privada")
                .multiline()
                .read_only()
                .height(160)
        }))
        .visible(tem_privada);

    Column::new()
        .gap(8)
        .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
        .child(Text::new(t!("cofre-chave-ssh")).body_strong())
        .child(Text::new(detalhe).id("cofre-chave-tipo").secondary())
        .child(
            Edit::bind(signal(chave.fingerprint.clone()))
                .id("cofre-chave-fingerprint")
                .read_only(),
        )
        .child(
            Edit::bind(publica)
                .id("cofre-chave-publica")
                .multiline()
                .read_only()
                .height(72),
        )
        .child(acoes)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(Text::new(t!("comum-no-agente")))
                .child(
                    ToggleSwitch::bind(no_agente)
                        .id("cofre-chave-agente")
                        .disabled(token || !tem_privada),
                ),
        )
        .child(
            CheckBox::bind(confirmar, t!("comum-pedir-confirmacao-a-cada-uso"))
                .id("cofre-chave-confirmar")
                .disabled(token || !tem_privada),
        )
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(Text::new(t!("cofre-validade-no-agente-minutos-0-sem")))
                .child(
                    NumberBox::bind(minutos)
                        .id("cofre-chave-vida")
                        .min(0.0)
                        .max(10_080.0),
                ),
        )
        .child(ver_privada)
}

/// Uma linha de rótulo e valor somente leitura (nome, endereço, grupos, impressão).
fn linha_de_leitura(rotulo: String, id: &'static str, valor: Signal<String>) -> impl View + use<> {
    Row::new()
        .gap(8)
        .align(Align::Center)
        .child(Text::new(rotulo).secondary().width(120))
        .child(Edit::bind(valor).id(id).read_only().grow(1))
}

/// O que o item de identidade Nebula mostra além das propriedades: o nome do certificado, o
/// endereço, os grupos, a validade (laranja perto de vencer, vermelha vencida), a impressão
/// digital, a chave privada (escondida até pedirem) e a troca do certificado (M48).
fn painel_da_nebula(
    pagina: &Rc<Pagina>,
    id: u64,
    identidade: &IdentidadeNebula,
) -> impl View + use<> {
    let nome = signal(identidade.nome.clone());
    let endereco = signal(identidade.endereco.clone());
    let grupos = signal(identidade.grupos.join(", "));
    let impressao = signal(identidade.impressao.clone());
    let privada = signal(identidade.chave.trim().to_owned());
    let mostrar = signal(false);
    let vence_em = signal(identidade.vence_em);
    let agora = || {
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    };
    let situacao = move || nebula::validade(vence_em.get(), agora());
    let texto_da_validade = move || {
        let quando = recentes::data(vence_em.get());
        match situacao() {
            Validade::Sem => t!("nebula-sem-validade"),
            Validade::Vencido => t!("nebula-vencido-em", data = quando),
            Validade::EmDia | Validade::PertoDeVencer => t!("nebula-valido-ate", data = quando),
        }
    };
    let validade_do_tipo = |tipo: Validade, cor: Option<Token>, id: &'static str| {
        let texto = Text::new(texto_da_validade).id(id);
        let texto = match cor {
            Some(cor) => texto.color(cor),
            None => texto,
        };
        texto.visible(move || {
            let atual = situacao();
            atual == tipo || (tipo == Validade::EmDia && atual == Validade::Sem)
        })
    };

    let copiar = move || clipboard::write(impressao.get_untracked());
    let substituir = {
        let pagina = Rc::clone(pagina);
        move || {
            pagina.aviso.set(None);
            let pagina = Rc::clone(&pagina);
            dialogs::open_file()
                .remember("certificados")
                .title(t!("nebula-substituir-titulo"))
                .filter(t!("nebula-certificados"), &["crt", "pem", "cert"])
                .show(move |caminho| {
                    let Some(caminho) = caminho else {
                        return;
                    };
                    let texto = match std::fs::read_to_string(&caminho) {
                        Ok(texto) => texto,
                        Err(erro) => {
                            pagina.falhar(t!(
                                "comum-nao-consegui-ler",
                                arquivo = caminho.display().to_string(),
                                erro = erro.to_string()
                            ));
                            return;
                        }
                    };
                    let Some(atual) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                        return;
                    };
                    let Some(antiga) = atual.nebula.clone() else {
                        return;
                    };
                    let agora = agora() as i64;
                    let nova = match nebula::substituir_certificado(&antiga, &texto, None, agora) {
                        Ok(nova) => nova,
                        Err(erro) => {
                            pagina.falhar(erro);
                            return;
                        }
                    };
                    let mut novo = atual;
                    novo.nebula = Some(nova.clone());
                    if pagina.salvar(novo).is_none() {
                        return;
                    }
                    nome.set(nova.nome);
                    endereco.set(nova.endereco);
                    grupos.set(nova.grupos.join(", "));
                    impressao.set(nova.impressao);
                    vence_em.set(nova.vence_em);
                    pagina.avisar(InfoBarSeverity::Success, t!("nebula-certificado-trocado"));
                });
        }
    };

    Column::new()
        .gap(8)
        .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
        .child(Text::new(t!("nebula-titulo")).body_strong())
        .child(linha_de_leitura(
            t!("comum-nome"),
            "cofre-nebula-nome",
            nome,
        ))
        .child(linha_de_leitura(
            t!("comum-endereco"),
            "cofre-nebula-endereco",
            endereco,
        ))
        .child(linha_de_leitura(
            t!("nebula-grupos"),
            "cofre-nebula-grupos",
            grupos,
        ))
        .child(validade_do_tipo(
            Validade::EmDia,
            None,
            "cofre-nebula-validade",
        ))
        .child(validade_do_tipo(
            Validade::PertoDeVencer,
            Some(Token::SystemFillColorCaution),
            "cofre-nebula-validade-perto",
        ))
        .child(validade_do_tipo(
            Validade::Vencido,
            Some(Token::SystemFillColorCritical),
            "cofre-nebula-validade-vencida",
        ))
        .child(linha_de_leitura(
            t!("nebula-impressao"),
            "cofre-nebula-impressao",
            impressao,
        ))
        .child(
            Row::new()
                .gap(8)
                .wrap()
                .child(
                    Button::new(t!("nebula-copiar-impressao"))
                        .id("cofre-nebula-copiar")
                        .icon(Symbol::Copy)
                        .on_click(copiar),
                )
                .child(
                    Button::new(t!("nebula-substituir"))
                        .id("cofre-nebula-substituir")
                        .icon(Symbol::ArrowUpload)
                        .on_click(substituir),
                ),
        )
        .child(
            Column::new()
                .gap(4)
                .child(
                    Button::new(move || {
                        if mostrar.get() {
                            t!("nebula-ocultar-chave")
                        } else {
                            t!("nebula-mostrar-chave")
                        }
                    })
                    .id("cofre-nebula-mostrar")
                    .icon(Symbol::Key)
                    .on_click(move || mostrar.update(|m| *m = !*m)),
                )
                .child(Show::when(mostrar, move || {
                    Edit::bind(privada)
                        .id("cofre-nebula-chave")
                        .multiline()
                        .read_only()
                        .height(120)
                })),
        )
}

/// O que o item de identidade WireGuard mostra além das propriedades: a chave pública, os
/// endereços do nó e a chave privada (escondida até pedirem) (M57).
fn painel_do_wireguard(identidade: &IdentidadeWireGuard) -> impl View + use<> {
    let publica = signal(identidade.chave_publica.clone());
    let enderecos = signal(identidade.enderecos.join(", "));
    let privada = signal(identidade.chave_privada.trim().to_owned());
    let mostrar = signal(false);
    let copiar = move || clipboard::write(publica.get_untracked());
    Column::new()
        .gap(8)
        .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
        .child(Text::new(t!("wireguard-titulo")).body_strong())
        .child(linha_de_leitura(
            t!("wireguard-chave-publica"),
            "cofre-wireguard-publica",
            publica,
        ))
        .child(linha_de_leitura(
            t!("wireguard-enderecos"),
            "cofre-wireguard-enderecos",
            enderecos,
        ))
        .child(
            Button::new(t!("wireguard-copiar-chave-publica"))
                .id("cofre-wireguard-copiar")
                .icon(Symbol::Copy)
                .on_click(copiar),
        )
        .child(
            Column::new()
                .gap(4)
                .child(
                    Button::new(move || {
                        if mostrar.get() {
                            t!("wireguard-ocultar-chave")
                        } else {
                            t!("wireguard-mostrar-chave")
                        }
                    })
                    .id("cofre-wireguard-mostrar")
                    .icon(Symbol::Key)
                    .on_click(move || mostrar.update(|m| *m = !*m)),
                )
                .child(Show::when(mostrar, move || {
                    Edit::bind(privada)
                        .id("cofre-wireguard-chave")
                        .read_only()
                        .height(32)
                })),
        )
}

/// O que o item de identidade Tailscale mostra além das propriedades: o servidor, o login, a
/// tailnet, o nome e os endereços da máquina, até quando a chave dela vale, a chave de
/// autenticação (a pessoa pode guardar uma) e o "Esquecer esta máquina" (M57).
fn painel_do_tailscale(
    pagina: &Rc<Pagina>,
    id: u64,
    identidade: &IdentidadeTailscale,
) -> impl View + use<> {
    let servidor = signal(if identidade.servidor.trim().is_empty() {
        t!("tailscale-servidor-oficial")
    } else {
        identidade.servidor.clone()
    });
    let login = signal(identidade.login.clone());
    let tailnet = signal(identidade.tailnet.clone());
    let nome = signal(identidade.nome.clone());
    let enderecos = signal(identidade.enderecos.join(", "));
    let validade = |vence_em: u64| {
        if vence_em == 0 {
            t!("tailscale-sem-maquina")
        } else {
            t!("tailscale-chave-vale-ate", data = recentes::data(vence_em))
        }
    };
    let vence = signal(validade(identidade.vence_em));
    let chave = signal(identidade.chave_autenticacao.clone());
    {
        let pagina = Rc::clone(pagina);
        effect(move || {
            let chave = chave.get();
            untrack(|| {
                let Some(atual) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                    return;
                };
                let mut novo = atual.clone();
                if let Some(ts) = novo.tailscale.as_mut() {
                    ts.chave_autenticacao = chave;
                }
                if novo != atual {
                    pagina.salvar(novo);
                }
            });
        });
    }
    let esquecer = {
        let pagina = Rc::clone(pagina);
        move || {
            pagina.aviso.set(None);
            let Some(atual) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                return;
            };
            let mut novo = atual;
            if let Some(ts) = novo.tailscale.as_mut() {
                crate::tailscale::esquecer_maquina(ts);
            }
            if pagina.salvar(novo).is_none() {
                return;
            }
            login.set(String::new());
            tailnet.set(String::new());
            nome.set(String::new());
            enderecos.set(String::new());
            vence.set(validade(0));
            pagina.avisar(InfoBarSeverity::Success, t!("tailscale-maquina-esquecida"));
        }
    };
    Column::new()
        .gap(8)
        .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
        .child(Text::new(t!("tailscale-titulo")).body_strong())
        .child(linha_de_leitura(
            t!("tailscale-servidor"),
            "cofre-tailscale-servidor",
            servidor,
        ))
        .child(linha_de_leitura(
            t!("tailscale-login"),
            "cofre-tailscale-login",
            login,
        ))
        .child(linha_de_leitura(
            t!("tailscale-tailnet"),
            "cofre-tailscale-tailnet",
            tailnet,
        ))
        .child(linha_de_leitura(
            t!("comum-nome"),
            "cofre-tailscale-nome",
            nome,
        ))
        .child(linha_de_leitura(
            t!("tailscale-enderecos"),
            "cofre-tailscale-enderecos",
            enderecos,
        ))
        .child(linha_de_leitura(
            t!("tailscale-chave-da-maquina"),
            "cofre-tailscale-validade",
            vence,
        ))
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Text::new(t!("tailscale-chave-autenticacao"))
                        .secondary()
                        .width(120),
                )
                .child(
                    Edit::bind(chave)
                        .id("cofre-tailscale-chave-autenticacao")
                        .password()
                        .placeholder(t!("tailscale-chave-autenticacao-dica"))
                        .grow(1),
                ),
        )
        .child(
            Button::new(t!("tailscale-esquecer"))
                .id("cofre-tailscale-esquecer")
                .icon(Symbol::Delete)
                .on_click(esquecer),
        )
}

/// O que o item de identidade ZeroTier mostra além das propriedades: o endereço do membro (que
/// quem administra a rede autoriza) com o "Copiar", o nome e os endereços na rede, a identidade
/// (escondida até pedirem), o "Exportar identity.secret…" e o "Esquecer o estado" (M61).
fn painel_do_zerotier(
    pagina: &Rc<Pagina>,
    id: u64,
    identidade: &IdentidadeZeroTier,
) -> impl View + use<> {
    let endereco = signal(if identidade.endereco.is_empty() {
        t!("zerotier-identidade-nasce")
    } else {
        identidade.endereco.clone()
    });
    let nome = signal(identidade.nome_rede.clone());
    let enderecos = signal(identidade.enderecos.join(", "));
    let estado = signal(estado_do_item(identidade));
    let segredo = signal(identidade.identidade.trim().to_owned());
    let mostrar = signal(false);
    let copiar = {
        let puro = identidade.endereco.clone();
        move || clipboard::write(puro.clone())
    };
    let exportar = {
        let pagina = Rc::clone(pagina);
        move || {
            pagina.aviso.set(None);
            let Some(texto) = pagina
                .cofre
                .agora(|c| c.item(id).and_then(|i| i.zerotier.clone()))
                .and_then(|z| zerotier::texto_da_identidade(&z))
            else {
                pagina.avisar(InfoBarSeverity::Warning, t!("zerotier-sem-identidade"));
                return;
            };
            let pagina = Rc::clone(&pagina);
            dialogs::save_file()
                .remember("identidades-exportar")
                .title(t!("zerotier-exportar"))
                .file_name("identity.secret")
                .show(move |caminho| {
                    let Some(caminho) = caminho else {
                        return;
                    };
                    match crate::cofre::gravar_arquivo_privado(
                        &caminho,
                        format!("{texto}\n").as_bytes(),
                    ) {
                        Ok(()) => pagina.avisar(
                            InfoBarSeverity::Success,
                            t!(
                                "zerotier-exportada",
                                arquivo = caminho.display().to_string()
                            ),
                        ),
                        Err(erro) => pagina.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "comum-nao-consegui-gravar",
                                arquivo = caminho.display().to_string(),
                                erro = erro.to_string()
                            ),
                        ),
                    }
                });
        }
    };
    let esquecer = {
        let pagina = Rc::clone(pagina);
        move || {
            pagina.aviso.set(None);
            let Some(atual) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                return;
            };
            let mut novo = atual;
            if let Some(zt) = novo.zerotier.as_mut() {
                zerotier::esquecer_estado(zt);
            }
            if pagina.salvar(novo).is_none() {
                return;
            }
            nome.set(String::new());
            enderecos.set(String::new());
            estado.set(t!("zerotier-sem-estado"));
            pagina.avisar(InfoBarSeverity::Success, t!("zerotier-estado-esquecido"));
        }
    };
    Column::new()
        .gap(8)
        .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
        .child(Text::new(t!("zerotier-titulo")).body_strong())
        .child(linha_de_leitura(
            t!("zerotier-endereco"),
            "cofre-zerotier-endereco",
            endereco,
        ))
        .child(linha_de_leitura(
            t!("zerotier-nome-rede"),
            "cofre-zerotier-nome-rede",
            nome,
        ))
        .child(linha_de_leitura(
            t!("zerotier-enderecos"),
            "cofre-zerotier-enderecos",
            enderecos,
        ))
        .child(linha_de_leitura(
            t!("zerotier-estado"),
            "cofre-zerotier-estado",
            estado,
        ))
        .child(
            Row::new()
                .gap(8)
                .child(
                    Button::new(t!("zerotier-copiar-endereco"))
                        .id("cofre-zerotier-copiar")
                        .icon(Symbol::Copy)
                        .disabled(identidade.endereco.is_empty())
                        .on_click(copiar),
                )
                .child(
                    Button::new(t!("zerotier-exportar"))
                        .id("cofre-zerotier-exportar")
                        .icon(Symbol::ArrowDownload)
                        .disabled(identidade.identidade.trim().is_empty())
                        .on_click(exportar),
                )
                .child(
                    Button::new(t!("zerotier-esquecer"))
                        .id("cofre-zerotier-esquecer")
                        .icon(Symbol::Delete)
                        .on_click(esquecer),
                ),
        )
        .child(
            Column::new()
                .gap(4)
                .child(
                    Button::new(move || {
                        if mostrar.get() {
                            t!("zerotier-ocultar-identidade")
                        } else {
                            t!("zerotier-mostrar-identidade")
                        }
                    })
                    .id("cofre-zerotier-mostrar")
                    .icon(Symbol::Key)
                    .on_click(move || mostrar.update(|m| *m = !*m)),
                )
                .child(Show::when(mostrar, move || {
                    Edit::bind(segredo)
                        .id("cofre-zerotier-identidade")
                        .read_only()
                        .height(32)
                })),
        )
}

/// "Com estado guardado" ou "Sem estado": o que o item sabe da última conexão.
fn estado_do_item(identidade: &IdentidadeZeroTier) -> String {
    if zerotier::tem_estado(identidade) {
        t!("zerotier-com-estado")
    } else {
        t!("zerotier-sem-estado")
    }
}

/// O que o item de identidade OpenVPN mostra além das propriedades: o servidor, o nome do
/// certificado, a validade (laranja perto de vencer, vermelha vencida), a impressão digital, a
/// chave privada (escondida até pedirem) e a troca do certificado (M56).
fn painel_do_openvpn(
    pagina: &Rc<Pagina>,
    id: u64,
    identidade: &IdentidadeOpenVpn,
) -> impl View + use<> {
    let servidor = signal(identidade.servidor.clone());
    let nome = signal(identidade.cn.clone());
    let impressao = signal(identidade.impressao.clone());
    let privada = signal(identidade.chave.trim().to_owned());
    let mostrar = signal(false);
    let vence_em = signal(identidade.vence_em);
    let agora = || {
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    };
    let situacao = move || openvpn::validade(vence_em.get(), agora());
    let texto_da_validade = move || {
        let quando = recentes::data(vence_em.get());
        match situacao() {
            Validade::Sem => t!("openvpn-sem-validade"),
            Validade::Vencido => t!("openvpn-vencido-em", data = quando),
            Validade::EmDia | Validade::PertoDeVencer => t!("openvpn-valido-ate", data = quando),
        }
    };
    let validade_do_tipo = |tipo: Validade, cor: Option<Token>, id: &'static str| {
        let texto = Text::new(texto_da_validade).id(id);
        let texto = match cor {
            Some(cor) => texto.color(cor),
            None => texto,
        };
        texto.visible(move || {
            let atual = situacao();
            atual == tipo || (tipo == Validade::EmDia && atual == Validade::Sem)
        })
    };

    let copiar = move || clipboard::write(impressao.get_untracked());
    // A renovação: o certificado novo e, se a chave também mudou, a chave nova (cancelar o
    // segundo diálogo mantém a chave de agora).
    let substituir = {
        let pagina = Rc::clone(pagina);
        move || {
            pagina.aviso.set(None);
            let pagina = Rc::clone(&pagina);
            dialogs::open_file()
                .remember("certificados")
                .title(t!("openvpn-substituir-titulo"))
                .filter(t!("openvpn-certificados"), &["crt", "pem", "cert"])
                .show(move |caminho| {
                    let Some(caminho) = caminho else {
                        return;
                    };
                    let Some(certificado) = ler_texto(&pagina, &caminho) else {
                        return;
                    };
                    let pagina = Rc::clone(&pagina);
                    dialogs::open_file()
                        .remember("certificados")
                        .title(t!("openvpn-substituir-chave-titulo"))
                        .filter(t!("openvpn-chaves"), &["key", "pem"])
                        .show(move |caminho_da_chave| {
                            let chave = match caminho_da_chave {
                                Some(caminho) => match ler_texto(&pagina, &caminho) {
                                    Some(texto) => Some(texto),
                                    None => return,
                                },
                                None => None,
                            };
                            let Some(atual) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                                return;
                            };
                            let Some(antiga) = atual.openvpn.clone() else {
                                return;
                            };
                            let nova = match openvpn::substituir_certificado(
                                &antiga,
                                &certificado,
                                chave.as_deref(),
                                std::time::SystemTime::now(),
                            ) {
                                Ok(nova) => nova,
                                Err(erro) => {
                                    pagina.falhar(erro);
                                    return;
                                }
                            };
                            let mut novo = atual;
                            novo.openvpn = Some(nova.clone());
                            if pagina.salvar(novo).is_none() {
                                return;
                            }
                            nome.set(nova.cn);
                            impressao.set(nova.impressao);
                            vence_em.set(nova.vence_em);
                            privada.set(nova.chave.trim().to_owned());
                            pagina.avisar(
                                InfoBarSeverity::Success,
                                t!("openvpn-certificado-trocado"),
                            );
                        });
                });
        }
    };

    Column::new()
        .gap(8)
        .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
        .child(Text::new(t!("openvpn-titulo")).body_strong())
        .child(linha_de_leitura(
            t!("openvpn-servidor"),
            "cofre-openvpn-servidor",
            servidor,
        ))
        .child(linha_de_leitura(
            t!("comum-nome"),
            "cofre-openvpn-nome",
            nome,
        ))
        .child(validade_do_tipo(
            Validade::EmDia,
            None,
            "cofre-openvpn-validade",
        ))
        .child(validade_do_tipo(
            Validade::PertoDeVencer,
            Some(Token::SystemFillColorCaution),
            "cofre-openvpn-validade-perto",
        ))
        .child(validade_do_tipo(
            Validade::Vencido,
            Some(Token::SystemFillColorCritical),
            "cofre-openvpn-validade-vencida",
        ))
        .child(linha_de_leitura(
            t!("openvpn-impressao"),
            "cofre-openvpn-impressao",
            impressao,
        ))
        .child(
            Row::new()
                .gap(8)
                .wrap()
                .child(
                    Button::new(t!("openvpn-copiar-impressao"))
                        .id("cofre-openvpn-copiar")
                        .icon(Symbol::Copy)
                        .on_click(copiar),
                )
                .child(
                    Button::new(t!("openvpn-substituir"))
                        .id("cofre-openvpn-substituir")
                        .icon(Symbol::ArrowUpload)
                        .on_click(substituir),
                ),
        )
        .child(
            Column::new()
                .gap(4)
                .child(
                    Button::new(move || {
                        if mostrar.get() {
                            t!("openvpn-ocultar-chave")
                        } else {
                            t!("openvpn-mostrar-chave")
                        }
                    })
                    .id("cofre-openvpn-mostrar")
                    .icon(Symbol::Key)
                    .on_click(move || mostrar.update(|m| *m = !*m)),
                )
                .child(Show::when(mostrar, move || {
                    Edit::bind(privada)
                        .id("cofre-openvpn-chave")
                        .multiline()
                        .read_only()
                        .height(120)
                })),
        )
}

/// Lê um arquivo de texto da escolha da pessoa; o erro vai para a faixa de avisos da página.
fn ler_texto(pagina: &Rc<Pagina>, caminho: &std::path::Path) -> Option<String> {
    match std::fs::read_to_string(caminho) {
        Ok(texto) => Some(texto),
        Err(erro) => {
            pagina.falhar(t!(
                "comum-nao-consegui-ler",
                arquivo = caminho.display().to_string(),
                erro = erro.to_string()
            ));
            None
        }
    }
}

/// O item escolhido na lista, se a seleção é um item só e ele existe (acompanha as mudanças).
fn item_unico(cofre: &CofreCompartilhado, selecao: Signal<BTreeSet<String>>) -> Option<u64> {
    let _ = cofre.versao();
    let selecao = selecao.get();
    if selecao.len() != 1 {
        return None;
    }
    let id = id_da_chave(selecao.iter().next()?, PREFIXO_ITEM)?;
    cofre.agora(|c| c.item(id).map(|_| id))
}

/// A página do cofre de senhas: barra de ações, as pastas, a lista da pasta aberta, as
/// propriedades do item escolhido e os diálogos do gerador, da senha mestra e do desbloqueio.
/// Autossuficiente: só precisa do `cofre`, para servir a qualquer casca que a hospede.
pub fn pagina_cofre(cofre: CofreCompartilhado) -> impl View {
    pagina_cofre_com(cofre, signal(None))
}

/// Como [`pagina_cofre`], com um pedido de fora: pôr o id de um item em `escolher` abre a pasta
/// dele e o escolhe na lista (a busca global usa). A página zera o signal ao atender.
pub fn pagina_cofre_com(cofre: CofreCompartilhado, escolher: Signal<Option<u64>>) -> impl View {
    pagina_cofre_completa(cofre, escolher, None, None, None)
}

/// Como [`pagina_cofre_com`], com `exportar_chave`: o que o botão "Exportar como…" de uma chave
/// SSH chama com o id do item (a casca leva à página Chaves SSH, onde fica o diálogo), e com
/// `portabilidade`: o que o menu "Mais" chama para levar o cofre e para abrir o de outra máquina,
/// e com `abrir_nota`: o que o clique duplo numa nota e o "Abrir em Notas" chamam com o id da
/// nota (a casca leva à página Notas).
pub fn pagina_cofre_completa(
    cofre: CofreCompartilhado,
    escolher: Signal<Option<u64>>,
    exportar_chave: Option<Rc<dyn Fn(u64)>>,
    portabilidade: Option<Portabilidade>,
    abrir_nota: Option<Rc<dyn Fn(u64)>>,
) -> impl View {
    let imagens = Imagens::nova();
    let exportando = Andamento::nova();
    let importando = Andamento::nova();
    // O cofre guarda o aviso enquanto o app vive; a página (há uma por janela) pode sumir antes.
    let antes_de_bloquear: Rc<dyn Fn()> = {
        let imagens = imagens.clone();
        let (exportar, importar) = (
            exportando.gancho_de_cancelar(),
            importando.gancho_de_cancelar(),
        );
        Rc::new(move || {
            imagens.esvaziar();
            exportar();
            importar();
        })
    };
    {
        let fraco = Rc::downgrade(&antes_de_bloquear);
        cofre.antes_de_bloquear(move || {
            if let Some(aviso) = fraco.upgrade() {
                aviso();
            }
        });
    }
    let pagina = Rc::new(Pagina {
        _antes_de_bloquear: antes_de_bloquear,
        exportando,
        importando,
        exportar_chave,
        portabilidade,
        abrir_nota,
        imagens,
        cofre: cofre.clone(),
        pasta_aberta: signal(None),
        selecao_arvore: signal(Some(RAIZ.to_owned())),
        abertos: signal(BTreeSet::from([RAIZ.to_owned()])),
        selecao: signal(BTreeSet::new()),
        renomeando: signal(None),
        renomeando_arvore: signal(None),
        filtro: signal(String::new()),
        aviso: signal(None),
        geracao: signal(0),
        apagar_aberto: signal(false),
        apagar_chaves: signal(Vec::new()),
        mover_aberto: signal(false),
        mover_chaves: signal(Vec::new()),
        mover_destino: signal(None),
    });
    let pasta_aberta = pagina.pasta_aberta;
    let selecao_arvore = pagina.selecao_arvore;
    let abertos = pagina.abertos;
    let selecao = pagina.selecao;
    let renomeando = pagina.renomeando;
    let renomeando_arvore = pagina.renomeando_arvore;
    let filtro = pagina.filtro;
    let aviso = pagina.aviso;
    let geracao = pagina.geracao;
    let apagar_aberto = pagina.apagar_aberto;
    let apagar_chaves = pagina.apagar_chaves;
    let mover_aberto = pagina.mover_aberto;
    let mover_chaves = pagina.mover_chaves;
    let mover_destino = pagina.mover_destino;

    // A árvore das pastas e a lista da pasta aberta acompanham o cofre.
    let arvore_itens = signal(Vec::<TreeItem>::new());
    {
        let cofre = cofre.clone();
        effect(move || {
            let linhas = cofre.ler(itens_da_arvore);
            untrack(|| arvore_itens.set(linhas));
        });
    }
    let linhas = signal(Vec::<DataGridRow>::new());
    {
        let cofre = cofre.clone();
        effect(move || {
            let aberta = pasta_aberta.get();
            let texto = filtro.get();
            let novas = cofre.ler(|c| linhas_da_lista(c, aberta, &texto));
            untrack(|| linhas.set(novas));
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
    // Escolher uma linha da árvore abre a pasta dela.
    {
        let pagina = Rc::clone(&pagina);
        effect(move || {
            let Some(chave) = selecao_arvore.get() else {
                return;
            };
            untrack(|| {
                if let Some(destino) = destino_da_chave(&chave)
                    && destino != pagina.pasta_aberta.get_untracked()
                {
                    pagina.abrir_pasta(destino);
                }
            });
        });
    }
    // O pedido de fora: abre a pasta do item, limpa o filtro e escolhe o item na lista.
    {
        let pagina = Rc::clone(&pagina);
        effect(move || {
            let Some(id) = escolher.get() else {
                return;
            };
            untrack(|| {
                escolher.set(None);
                let Some(item) = pagina.cofre.agora(|c| c.item(id).cloned()) else {
                    return;
                };
                filtro.set(String::new());
                pagina.abrir_pasta(item.pasta);
                pagina.escolher_linha(format!("{PREFIXO_ITEM}{id}"));
            });
        });
    }

    let bloqueado = {
        let cofre = cofre.clone();
        move || cofre.ler(|c| c.bloqueado())
    };
    let sem_item = {
        let cofre = cofre.clone();
        move || item_unico(&cofre, selecao).is_none()
    };
    let sem_selecao = move || selecao.get().is_empty();

    let gerador_aberto = signal(false);
    let mestra_aberta = signal(false);
    let campo_aberto = signal(false);
    let desbloqueio_aberto = signal(false);

    // --- A barra de ações ---
    let barra = {
        let acao = |f: fn(&Rc<Pagina>)| {
            let pagina = Rc::clone(&pagina);
            move || f(&pagina)
        };
        let travado = bloqueado.clone();
        Row::new()
            .wrap()
            .gap(4)
            .padding(Insets::new(8.0, 4.0, 8.0, 4.0))
            .child(
                Button::new(t!("comum-novo"))
                    .id("cofre-novo")
                    .icon(Symbol::Add)
                    .disabled(travado.clone())
                    .on_click(acao(|p| p.novo())),
            )
            .child(
                Button::new(t!("comum-nova-pasta"))
                    .id("cofre-nova-pasta")
                    .icon(Symbol::FolderAdd)
                    .disabled(travado.clone())
                    .on_click(acao(|p| p.nova_pasta())),
            )
            .child(
                Button::new(t!("cofre-mover-para"))
                    .id("cofre-mover")
                    .icon(Symbol::ArrowForward)
                    .disabled({
                        let travado = travado.clone();
                        move || travado() || sem_selecao()
                    })
                    .on_click(acao(|p| p.mover_escolhidas())),
            )
            .child(
                Button::new(t!("comum-duplicar"))
                    .id("cofre-duplicar")
                    .icon(Symbol::Copy)
                    .disabled({
                        let sem_item = sem_item.clone();
                        move || sem_item()
                    })
                    .on_click(acao(|p| p.duplicar())),
            )
            .child(
                Button::new(t!("comum-apagar"))
                    .id("cofre-apagar")
                    .icon(Symbol::Delete)
                    .disabled({
                        let travado = travado.clone();
                        move || travado() || sem_selecao()
                    })
                    .on_click(acao(|p| p.apagar_escolhidas())),
            )
            .child(
                Button::new(t!("cofre-gerar-senha"))
                    .id("cofre-gerar")
                    .icon(Symbol::Sparkle)
                    .disabled(travado.clone())
                    .on_click(move || gerador_aberto.set(true)),
            )
            .child(
                Button::new(t!("comum-importar"))
                    .id("cofre-importar")
                    .icon(Symbol::ArrowDownload)
                    .disabled(travado.clone())
                    .on_click(acao(|p| p.importar())),
            )
            .child({
                let tirech = Rc::clone(&pagina);
                let bitwarden = Rc::clone(&pagina);
                Button::new(t!("cofre-exportar"))
                    .id("cofre-exportar")
                    .icon(Symbol::ArrowUpload)
                    .disabled(travado.clone())
                    .flyout(
                        MenuFlyout::new()
                            .id("cofre-exportar-menu")
                            .item(
                                "cofre-exportar-tirech",
                                t!("cofre-json-do-app"),
                                move || tirech.exportar(false),
                            )
                            .item(
                                "cofre-exportar-bitwarden",
                                t!("cofre-json-do-bitwarden"),
                                move || bitwarden.exportar(true),
                            ),
                    )
            })
            .child({
                let tem = pagina.portabilidade.is_some();
                let (levar, abrir) = pagina.portabilidade.clone().map_or_else(
                    || {
                        (
                            Rc::new(|| {}) as Rc<dyn Fn()>,
                            Rc::new(|| {}) as Rc<dyn Fn()>,
                        )
                    },
                    |p| (p.levar, p.abrir),
                );
                Button::new(t!("comum-mais"))
                    .id("cofre-mais")
                    .icon(Symbol::MoreHorizontal)
                    .visible(tem)
                    .flyout(
                        MenuFlyout::new()
                            .id("cofre-mais-menu")
                            .item(
                                "cofre-levar",
                                t!("cofre-levar-o-cofre-para-outra-maquina"),
                                move || levar(),
                            )
                            .item(
                                "cofre-abrir-outro",
                                t!("cofre-abrir-cofre-de-outra-maquina"),
                                move || abrir(),
                            ),
                    )
            })
            .child(
                Button::new(t!("cofre-senha-mestra-2"))
                    .id("cofre-mestra")
                    .icon(Symbol::Key)
                    .disabled(travado)
                    .on_click(move || mestra_aberta.set(true)),
            )
            .child({
                let cofre = cofre.clone();
                Button::new(t!("cofre-bloquear"))
                    .id("cofre-bloquear")
                    .icon(Symbol::LockClosed)
                    .disabled({
                        let cofre = cofre.clone();
                        move || cofre.ler(|c| c.bloqueado() || !c.tem_senha_mestra())
                    })
                    .on_click(move || {
                        cofre.bloquear();
                        selecao.update(|s| s.clear());
                        aviso.set(None);
                    })
            })
    };

    // --- A árvore das pastas ---
    let arvore = {
        let renomear = Rc::clone(&pagina);
        let apagar = Rc::clone(&pagina);
        let mover = Rc::clone(&pagina);
        let soltar = Rc::clone(&pagina);
        let aceita = Rc::clone(&pagina);
        let menu = Rc::clone(&pagina);
        TreeView::bind(arvore_itens, selecao_arvore)
            .id("cofre-pastas")
            .grow(1)
            .expanded(abertos)
            .renaming(renomeando_arvore)
            .on_rename(move |chave, texto| {
                if let Some(id) = id_da_chave(&chave, PREFIXO_PASTA) {
                    renomear.renomear_pasta(id, &texto);
                }
            })
            .on_delete(move |chave| apagar.pedir_apagar(vec![chave]))
            .on_move(move |chave, pai, _| {
                if let Some(pai) = pai {
                    mover.soltar(&[chave], &pai);
                }
            })
            .can_drop(|origem, pai| origem != RAIZ && pai.is_some())
            .on_grid_drop("cofre", move |s| {
                soltar.soltar(&s.keys, s.target.as_deref().unwrap_or(RAIZ));
            })
            .can_grid_drop(move |s| {
                s.target
                    .as_deref()
                    .is_some_and(|alvo| aceita.aceita(&s.keys, alvo))
            })
            .item_context_flyout(move |chave| {
                let prefixo = format!("menu-pastas-{chave}");
                let acao = |f: fn(&Pagina, &str)| {
                    let (pagina, chave) = (Rc::clone(&menu), chave.to_owned());
                    move || f(&pagina, &chave)
                };
                let subpasta = acao(|p, c| p.nova_pasta_em(destino_da_chave(c).flatten()));
                let flyout = MenuFlyout::new()
                    .id(prefixo.clone())
                    .item(
                        format!("{prefixo}-subpasta"),
                        if chave == RAIZ {
                            t!("comum-nova-pasta")
                        } else {
                            t!("cofre-nova-subpasta")
                        },
                        subpasta,
                    )
                    .icon(Symbol::FolderAdd);
                if chave == RAIZ {
                    return flyout;
                }
                flyout
                    .item(
                        format!("{prefixo}-renomear"),
                        t!("comum-renomear"),
                        acao(|p, c| p.renomeando_arvore.set(Some(c.to_owned()))),
                    )
                    .icon(Symbol::Rename)
                    .item(
                        format!("{prefixo}-mover"),
                        t!("cofre-mover-para"),
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
    };
    let esquerda = Column::new()
        .padding(Insets::new(8.0, 4.0, 4.0, 4.0))
        .child(arvore);

    // --- A lista ---
    let ordem = signal(Some(("titulo".to_owned(), SortDirection::Ascending)));
    let lista = {
        let invocar = Rc::clone(&pagina);
        let renomear = Rc::clone(&pagina);
        let apagar = Rc::clone(&pagina);
        let teclas = Rc::clone(&pagina);
        let aceita = Rc::clone(&pagina);
        let soltar = Rc::clone(&pagina);
        let menu = Rc::clone(&pagina);
        DataGrid::bind(linhas, selecao)
            .id("cofre-lista")
            .grow(1)
            .columns([
                DataGridColumn::new("titulo", t!("comum-titulo")).width(ColumnWidth::Star(1.0)),
                DataGridColumn::new("usuario", t!("comum-usuario"))
                    .width(ColumnWidth::Pixels(100.0)),
                DataGridColumn::new("url", t!("comum-endereco")).width(ColumnWidth::Pixels(110.0)),
                DataGridColumn::new("pasta", t!("comum-pasta")).width(ColumnWidth::Pixels(100.0)),
                DataGridColumn::new("alterado", t!("cofre-alterado"))
                    .width(ColumnWidth::Pixels(145.0)),
            ])
            .sort(ordem)
            .renaming(renomeando)
            .empty_text(t!("cofre-nada-nesta-pasta-use-novo-nova"))
            .drag_group("cofre")
            .on_invoke(move |chave| invocar.invocar(&chave))
            .on_rename(move |chave, texto| renomear.renomear(&chave, &texto))
            .on_delete(move |chaves| apagar.pedir_apagar(chaves))
            .on_key(move |tecla, mods| match tecla {
                Key::Backspace => {
                    teclas.subir();
                    true
                }
                Key::Char('n') if mods.command() && mods.shift => {
                    teclas.nova_pasta();
                    true
                }
                _ => false,
            })
            .can_drop(move |s| {
                s.target.as_deref().is_some_and(|alvo| {
                    alvo.starts_with(PREFIXO_PASTA) && aceita.aceita(&s.keys, alvo)
                })
            })
            .on_drop(move |s| {
                if let Some(alvo) = s.target.as_deref() {
                    soltar.soltar(&s.keys, alvo);
                }
            })
            .item_context_flyout(move |chave| {
                let prefixo = format!("menu-{chave}");
                let acao = |f: fn(&Pagina, &str)| {
                    let (pagina, chave) = (Rc::clone(&menu), chave.to_owned());
                    move || f(&pagina, &chave)
                };
                let flyout = MenuFlyout::new().id(prefixo.clone());
                let mover = acao(|p, c| p.pedir_mover(p.chaves_do_menu(c)));
                let apagar = acao(|p, c| p.pedir_apagar(p.chaves_do_menu(c)));
                let renomear = acao(|p, c| p.renomeando.set(Some(c.to_owned())));
                if id_da_chave(chave, PREFIXO_PASTA).is_some() {
                    flyout
                        .item(
                            format!("{prefixo}-abrir"),
                            t!("comum-abrir"),
                            acao(|p, c| p.invocar(c)),
                        )
                        .icon(Symbol::FolderOpen)
                        .item(
                            format!("{prefixo}-subpasta"),
                            t!("cofre-nova-subpasta"),
                            acao(|p, c| p.nova_pasta_em(id_da_chave(c, PREFIXO_PASTA))),
                        )
                        .icon(Symbol::FolderAdd)
                        .item(
                            format!("{prefixo}-renomear"),
                            t!("comum-renomear"),
                            renomear,
                        )
                        .icon(Symbol::Rename)
                        .item(format!("{prefixo}-mover"), t!("cofre-mover-para"), mover)
                        .icon(Symbol::ArrowForward)
                        .item(format!("{prefixo}-apagar"), t!("comum-apagar"), apagar)
                        .icon(Symbol::Delete)
                } else {
                    flyout
                        .item(
                            format!("{prefixo}-usuario"),
                            t!("cofre-copiar-usuario"),
                            acao(|p, c| {
                                if let Some(id) = id_da_chave(c, PREFIXO_ITEM) {
                                    p.copiar_usuario_de(id);
                                }
                            }),
                        )
                        .icon(Symbol::Copy)
                        .item(
                            format!("{prefixo}-senha"),
                            t!("cofre-copiar-senha"),
                            acao(|p, c| {
                                if let Some(id) = id_da_chave(c, PREFIXO_ITEM) {
                                    p.copiar_senha_de(id);
                                }
                            }),
                        )
                        .icon(Symbol::Key)
                        .separator()
                        .item(format!("{prefixo}-mover"), t!("cofre-mover-para"), mover)
                        .icon(Symbol::ArrowForward)
                        .item(
                            format!("{prefixo}-duplicar"),
                            t!("comum-duplicar"),
                            acao(|p, c| {
                                if let Some(id) = id_da_chave(c, PREFIXO_ITEM) {
                                    p.duplicar_item(id);
                                }
                            }),
                        )
                        .icon(Symbol::Copy)
                        .item(
                            format!("{prefixo}-renomear"),
                            t!("comum-renomear"),
                            renomear,
                        )
                        .icon(Symbol::Rename)
                        .item(format!("{prefixo}-apagar"), t!("comum-apagar"), apagar)
                        .icon(Symbol::Delete)
                }
            })
    };
    let caminho = {
        let cofre = cofre.clone();
        move || {
            let _ = cofre.versao();
            let caminho = cofre.agora(|c| c.caminho(pasta_aberta.get()));
            if caminho.is_empty() {
                t!("comum-cofre")
            } else {
                t!(
                    "cofre-caminho-da-pasta",
                    caminho = caminho.replace('/', " › ")
                )
            }
        }
    };
    let centro = Column::new()
        .gap(4)
        .padding(Insets::new(4.0, 4.0, 4.0, 4.0))
        .child(Text::new(caminho).id("cofre-caminho").secondary())
        .child(
            Edit::bind(filtro)
                .id("cofre-filtro")
                .placeholder(t!("comum-filtrar")),
        )
        .child(lista);

    // --- Os detalhes ---
    let grades = {
        let cofre = cofre.clone();
        move || -> Vec<Grade> {
            let _ = geracao.get();
            item_unico(&cofre, selecao)
                .map(|id| Grade {
                    id,
                    geracao: geracao.get_untracked(),
                    nota: cofre.agora(|c| c.item(id).is_some_and(|i| i.tipo == Tipo::Nota)),
                })
                .into_iter()
                .collect()
        }
    };
    let direita = {
        let copiar_usuario = Rc::clone(&pagina);
        let copiar_senha = Rc::clone(&pagina);
        let grade_pagina = Rc::clone(&pagina);
        let cofre_texto = cofre.clone();
        let coluna = Column::new()
            .gap(4)
            .padding(Insets::new(4.0, 4.0, 8.0, 4.0))
            .child(
                Row::new()
                    .wrap()
                    .gap(4)
                    .visible({
                        let sem_item = sem_item.clone();
                        move || !sem_item()
                    })
                    .child(
                        Button::new(t!("cofre-copiar-usuario"))
                            .id("cofre-copiar-usuario")
                            .icon(Symbol::Copy)
                            .on_click(move || copiar_usuario.copiar_usuario()),
                    )
                    .child(
                        Button::new(t!("cofre-copiar-senha"))
                            .id("cofre-copiar-senha")
                            .icon(Symbol::Key)
                            .on_click(move || copiar_senha.copiar_senha()),
                    )
                    .child(
                        Button::new(t!("cofre-novo-campo"))
                            .id("cofre-campo-novo")
                            .icon(Symbol::Add)
                            .on_click(move || campo_aberto.set(true)),
                    ),
            )
            .child(For::each(
                grades,
                |g: &Grade| (g.id, g.geracao, g.nota),
                move |g: Grade| grade_do_item(&grade_pagina, g.id),
            ))
            .child(
                Text::new(move || {
                    let _ = cofre_texto.versao();
                    cofre_texto.agora(|c| texto_sem_item(c, &selecao.get()))
                })
                .id("cofre-sem-item")
                .secondary()
                .wrap()
                .visible(sem_item),
            );
        // O item de chave SSH é mais alto que a janela: o painel rola.
        ScrollViewer::new()
            .id("cofre-detalhe-rolagem")
            .grow(1)
            .child(coluna)
    };

    let conteudo = Split::row(centro, direita)
        .id("cofre-conteudo")
        .ratio(signal(0.6))
        .min_first(260.0)
        .min_second(280.0)
        .grow(1);
    let aberto = Split::row(esquerda, conteudo)
        .id("cofre-principal")
        .size(signal(220.0))
        .min_first(160.0)
        .min_second(480.0)
        .grow(1);

    // --- O estado bloqueado ---
    let refazer = Rc::new(Cell::new(false));
    let bloqueado_painel = Column::new()
        .grow(1)
        .align(Align::Center)
        .justify(Justify::Center)
        .gap(8)
        .child(Icon::new(Symbol::LockClosed).size(24).decorative())
        .child(
            Text::new(t!("comum-o-cofre-esta-bloqueado"))
                .id("cofre-bloqueado")
                .subtitle(),
        )
        .child(
            Button::new(t!("comum-desbloquear"))
                .id("cofre-desbloquear-botao")
                .accent()
                .on_click(move || desbloqueio_aberto.set(true)),
        );

    // --- Os avisos ---
    let avisos = Column::new()
        .child(
            InfoBar::new(InfoBarSeverity::Informational)
                .id("cofre-aviso")
                .message(move || {
                    aviso
                        .get()
                        .filter(|(sev, _)| *sev == InfoBarSeverity::Informational)
                        .map(|(_, m)| m)
                        .unwrap_or_default()
                })
                .on_close(move || aviso.set(None))
                .visible(move || {
                    aviso
                        .get()
                        .is_some_and(|(sev, _)| sev == InfoBarSeverity::Informational)
                }),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Warning)
                .id("cofre-atencao")
                .message(move || {
                    aviso
                        .get()
                        .filter(|(sev, _)| *sev == InfoBarSeverity::Warning)
                        .map(|(_, m)| m)
                        .unwrap_or_default()
                })
                .on_close(move || aviso.set(None))
                .visible(move || {
                    aviso
                        .get()
                        .is_some_and(|(sev, _)| sev == InfoBarSeverity::Warning)
                }),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("cofre-erro")
                .message(move || {
                    aviso
                        .get()
                        .filter(|(sev, _)| *sev == InfoBarSeverity::Error)
                        .map(|(_, m)| m)
                        .unwrap_or_default()
                })
                .on_close(move || aviso.set(None))
                .visible(move || {
                    aviso
                        .get()
                        .is_some_and(|(sev, _)| sev == InfoBarSeverity::Error)
                }),
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
                        id_da_chave(chave, PREFIXO_ITEM)
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
        .id("cofre-apagar-dialogo")
        .content(
            Text::new(move || {
                let chaves = apagar_chaves.get();
                let _ = cofre_texto.versao();
                let (pastas, itens, so_uma) = cofre_texto.agora(|c| contar_apagar(c, &chaves));
                let resumo = resumo_apagar(pastas, itens, so_uma);
                if cofre_texto.agora(|c| apagar_leva_chave_dos_logs(c, &chaves)) {
                    format!("{resumo} {}", t!("cofre-apagar-chave-dos-logs"))
                } else {
                    resumo
                }
            })
            .id("cofre-apagar-texto")
            .wrap(),
        )
        .primary(t!("comum-apagar"), move || p.apagar())
        .close(t!("comum-cancelar"))
    };
    let mover_abertos = signal(BTreeSet::new());
    {
        let cofre = cofre.clone();
        effect(move || {
            if mover_aberto.get() {
                let todas = cofre.agora(chaves_das_pastas);
                untrack(|| mover_abertos.set(todas));
            }
        });
    }
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
                        id_da_chave(chave, PREFIXO_ITEM)
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
        .id("cofre-mover-dialogo")
        .content(
            TreeView::bind(arvore_itens, mover_destino)
                .id("cofre-mover-arvore")
                .expanded(mover_abertos)
                .width(320)
                .height(260),
        )
        .primary(t!("comum-mover"), move || {
            let destino = mover_destino
                .get_untracked()
                .unwrap_or_else(|| RAIZ.to_owned());
            p.soltar(&mover_chaves.get_untracked(), &destino);
        })
        .close(t!("comum-cancelar"))
    };
    let dialogo_gerador = dialogo_gerador(Rc::clone(&pagina), gerador_aberto);
    let dialogo_mestra = dialogo_senha_mestra(Rc::clone(&pagina), mestra_aberta);
    let dialogo_campo = dialogo_novo_campo(Rc::clone(&pagina), campo_aberto);
    let dialogo_desbloqueio = dialogo_desbloquear_id(
        "cofre-pagina-desbloquear",
        cofre.clone(),
        desbloqueio_aberto,
        refazer,
    );

    let corpo = {
        let bloqueado_a = bloqueado.clone();
        let bloqueado_b = bloqueado.clone();
        Column::new()
            .grow(1)
            .child(aberto.visible(move || !bloqueado_a()))
            .child(bloqueado_painel.visible(bloqueado_b))
    };

    Column::new()
        .id("cofre-pagina")
        .grow(1)
        .child(barra)
        .child(avisos)
        .child(corpo)
        .child(dialogo_apagar)
        .child(dialogo_mover)
        .child(dialogo_gerador)
        .child(dialogo_mestra)
        .child(dialogo_campo)
        .child(dialogo_desbloqueio)
        .child(
            pagina
                .exportando
                .dialogo(t!("cofre-exportando-os-anexos"), "cofre-exportar"),
        )
        .child(
            pagina
                .importando
                .dialogo(t!("cofre-importando-os-anexos"), "cofre-importar"),
        )
}

/// O diálogo `cofre-gerador`: as opções do [`Gerador`], a senha sorteada e os botões que a
/// usam.
fn dialogo_gerador(pagina: Rc<Pagina>, aberto: Signal<bool>) -> impl View {
    let padrao = pagina.cofre.gerador_padrao();
    let tamanho = signal(padrao.tamanho as f64);
    let maiusculas = signal(padrao.maiusculas);
    let minusculas = signal(padrao.minusculas);
    let digitos = signal(padrao.digitos);
    let simbolos = signal(padrao.simbolos);
    let sem_ambiguos = signal(padrao.sem_ambiguos);
    let saida = signal(String::new());
    let de_novo = signal(0u64);
    // Cada abertura começa nas opções de `cofre.gerador`, que a pessoa pode ter mudado.
    let cofre = pagina.cofre.clone();
    effect(move || {
        if aberto.get() {
            let padrao = cofre.gerador_padrao();
            tamanho.set(padrao.tamanho as f64);
            maiusculas.set(padrao.maiusculas);
            minusculas.set(padrao.minusculas);
            digitos.set(padrao.digitos);
            simbolos.set(padrao.simbolos);
            sem_ambiguos.set(padrao.sem_ambiguos);
        }
    });
    effect(move || {
        let _ = de_novo.get();
        let g = Gerador {
            tamanho: tamanho.get().round().max(0.0) as usize,
            maiusculas: maiusculas.get(),
            minusculas: minusculas.get(),
            digitos: digitos.get(),
            simbolos: simbolos.get(),
            sem_ambiguos: sem_ambiguos.get(),
        };
        // Reabrir o diálogo sorteia outra senha.
        let _ = aberto.get();
        saida.set(gerar_senha(&g));
    });
    let copiar = Rc::clone(&pagina);
    let usar = pagina;
    ContentDialog::new(t!("cofre-gerar-senha"), aberto)
        .id("cofre-gerador")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Row::new()
                        .gap(8)
                        .align(Align::Center)
                        .child(Text::new(t!("cofre-tamanho")))
                        .child(
                            NumberBox::bind(tamanho)
                                .id("cofre-gerador-tamanho")
                                .min(Gerador::MIN as f64)
                                .max(Gerador::MAX as f64),
                        ),
                )
                .child(
                    CheckBox::bind(maiusculas, t!("cofre-maiusculas"))
                        .id("cofre-gerador-maiusculas"),
                )
                .child(
                    CheckBox::bind(minusculas, t!("cofre-minusculas"))
                        .id("cofre-gerador-minusculas"),
                )
                .child(CheckBox::bind(digitos, t!("cofre-digitos")).id("cofre-gerador-digitos"))
                .child(CheckBox::bind(simbolos, t!("cofre-simbolos")).id("cofre-gerador-simbolos"))
                .child(
                    CheckBox::bind(sem_ambiguos, t!("cofre-sem-caracteres-ambiguos-0-o-1"))
                        .id("cofre-gerador-ambiguos"),
                )
                .child(Edit::bind(saida).id("cofre-gerador-senha"))
                .child(
                    Row::new()
                        .gap(4)
                        .child(
                            Button::new(t!("cofre-gerar-de-novo"))
                                .id("cofre-gerador-novo")
                                .icon(Symbol::ArrowSync)
                                .on_click(move || de_novo.update(|n| *n += 1)),
                        )
                        .child(
                            Button::new(t!("comum-copiar"))
                                .id("cofre-gerador-copiar")
                                .icon(Symbol::Copy)
                                .on_click(move || {
                                    copiar.cofre.copiar_segredo(saida.get_untracked());
                                    copiar.informar(t!("cofre-senha-copiada"));
                                }),
                        ),
                ),
        )
        .primary(t!("cofre-usar"), move || {
            usar.usar_senha(saida.get_untracked())
        })
        .close(t!("comum-fechar"))
}

/// O diálogo `cofre-senha-mestra`: define, troca ou remove a senha mestra.
fn dialogo_senha_mestra(pagina: Rc<Pagina>, aberto: Signal<bool>) -> impl View {
    let atual = signal(String::new());
    let nova = signal(String::new());
    let confirme = signal(String::new());
    let erro = signal(None::<String>);
    let refazer = Rc::new(Cell::new(false));
    let existe = {
        let cofre = pagina.cofre.clone();
        move || cofre.ler(|c| c.tem_senha_mestra())
    };
    {
        let refazer = Rc::clone(&refazer);
        effect(move || {
            if aberto.get() {
                return;
            }
            if refazer.replace(false) {
                aberto.set(true);
            } else {
                untrack(|| {
                    atual.set(String::new());
                    nova.set(String::new());
                    confirme.set(String::new());
                    erro.set(None);
                });
            }
        });
    }
    // Confere a senha atual (se há) e grava `nova`; `false` mantém o diálogo aberto com o erro.
    let aplicar = {
        let pagina = Rc::clone(&pagina);
        move |nova: Option<String>| -> bool {
            let cofre = pagina.cofre.clone();
            if cofre.agora(|c| c.tem_senha_mestra()) {
                let certa = cofre.mudar(|c| c.desbloquear(&atual.get_untracked()));
                if !certa {
                    atual.set(String::new());
                    erro.set(Some(t!("cofre-senha-mestra-incorreta")));
                    return false;
                }
            }
            match cofre.mudar(|c| c.definir_senha_mestra(nova.as_deref())) {
                Ok(()) => {
                    pagina.aviso.set(None);
                    erro.set(None);
                    true
                }
                Err(e) => {
                    pagina.falhar(t!(
                        "comum-nao-consegui-gravar-o-cofre",
                        erro = e.to_string()
                    ));
                    true
                }
            }
        }
    };
    let salvar = {
        let aplicar = aplicar.clone();
        let refazer = Rc::clone(&refazer);
        move || {
            let n = nova.get_untracked();
            if n != confirme.get_untracked() {
                confirme.set(String::new());
                erro.set(Some(t!("cofre-senhas-nao-conferem")));
                refazer.set(true);
                return;
            }
            // Nova vazia é o mesmo que remover.
            let ok = aplicar((!n.is_empty()).then_some(n));
            refazer.set(!ok);
        }
    };
    let remover = {
        let existe = existe.clone();
        move || {
            if existe() {
                let ok = aplicar(None);
                if ok {
                    aberto.set(false);
                }
            }
        }
    };
    ContentDialog::new(t!("cofre-senha-mestra"), aberto)
        .id("cofre-senha-mestra")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(t!("cofre-a-senha-mestra-cifra-o-cofre"))
                        .secondary()
                        .wrap(),
                )
                .child(
                    Edit::bind(atual)
                        .id("cofre-mestra-atual")
                        .password()
                        .placeholder(t!("cofre-senha-mestra-atual"))
                        .visible({
                            let existe = existe.clone();
                            move || existe()
                        }),
                )
                .child(
                    Edit::bind(nova)
                        .id("cofre-mestra-nova")
                        .password()
                        .placeholder(t!("cofre-nova-senha-mestra")),
                )
                .child(
                    Edit::bind(confirme)
                        .id("cofre-mestra-confirme")
                        .password()
                        .placeholder(t!("cofre-confirme")),
                )
                .child(
                    Text::new(move || erro.get().unwrap_or_default())
                        .id("cofre-mestra-erro")
                        .caption()
                        .visible(move || erro.get().is_some()),
                )
                .child(
                    Button::new(t!("cofre-remover-a-senha-mestra"))
                        .id("cofre-mestra-remover")
                        .visible(existe)
                        .on_click(remover),
                ),
        )
        .primary(t!("comum-salvar"), salvar)
        .close(t!("comum-cancelar"))
}

/// O diálogo `cofre-campo`: o nome do campo novo do item escolhido.
fn dialogo_novo_campo(pagina: Rc<Pagina>, aberto: Signal<bool>) -> impl View {
    let nome = signal(String::new());
    let oculto = signal(false);
    effect(move || {
        if !aberto.get() {
            untrack(|| {
                nome.set(String::new());
                oculto.set(false);
            });
        }
    });
    ContentDialog::new(t!("cofre-novo-campo"), aberto)
        .id("cofre-campo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Edit::bind(nome)
                        .id("cofre-campo-nome")
                        .placeholder(t!("cofre-nome-do-campo")),
                )
                .child(
                    CheckBox::bind(oculto, t!("cofre-valor-oculto-como-senha"))
                        .id("cofre-campo-oculto"),
                ),
        )
        .primary(t!("cofre-acrescentar"), move || {
            pagina.novo_campo(nome.get_untracked(), oculto.get_untracked());
        })
        .close(t!("comum-cancelar"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    fn item_em(pasta: Option<u64>, titulo: &str, usuario: &str) -> Item {
        Item {
            pasta,
            titulo: titulo.into(),
            usuario: usuario.into(),
            ..Item::default()
        }
    }

    /// Clientes › ACME (Painel, web-01), Clientes (Contrato), Pessoal (Cartão) e, na raiz, Wi-Fi.
    fn cofre_de_teste(nome: &str) -> (PastaTemporaria, Cofre) {
        let pasta = PastaTemporaria::nova(nome);
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let acme = cofre.garantir_caminho(None, &["Clientes", "ACME"]);
        let clientes = cofre.garantir_caminho(None, &["Clientes"]);
        let pessoal = cofre.garantir_caminho(None, &["Pessoal"]);
        for (onde, titulo, usuario) in [
            (acme, "Painel", "ana"),
            (acme, "web-01", "root"),
            (clientes, "Contrato", ""),
            (pessoal, "Cartão", "bia"),
            (None, "Wi-Fi", ""),
        ] {
            cofre.salvar(item_em(onde, titulo, usuario)).unwrap();
        }
        (pasta, cofre)
    }

    fn id_da(cofre: &Cofre, nome: &str) -> u64 {
        cofre.pastas().iter().find(|p| p.nome == nome).unwrap().id
    }

    fn titulos(linhas: &[DataGridRow]) -> Vec<&str> {
        linhas.iter().map(|l| l.cells[0].0.as_str()).collect()
    }

    #[test]
    fn o_resumo_do_apagar_conta_no_singular_e_no_plural() {
        let casos = [
            ((0, 1, false), "O item sai do cofre."),
            ((0, 2, false), "2 itens saem do cofre."),
            ((1, 0, true), "A pasta sai do cofre."),
            ((1, 3, true), "A pasta e 3 itens saem do cofre."),
            ((2, 0, true), "A pasta e 1 subpasta saem do cofre."),
            ((3, 1, true), "A pasta, 2 subpastas e 1 item saem do cofre."),
            ((2, 5, false), "2 pastas e 5 itens saem do cofre."),
            ((1, 0, false), "1 pasta sai do cofre."),
        ];
        for ((pastas, itens, so_uma), esperado) in casos {
            assert_eq!(resumo_apagar(pastas, itens, so_uma), esperado);
        }
    }

    #[test]
    fn a_contagem_do_apagar_nao_conta_duas_vezes() {
        let (_pasta, cofre) = cofre_de_teste("tela-contar");
        let clientes = id_da(&cofre, "Clientes");
        let painel = cofre
            .itens()
            .iter()
            .find(|i| i.titulo == "Painel")
            .unwrap()
            .id;
        // Clientes leva ACME e os 3 itens de dentro; o Painel solto já está contado.
        let chaves = vec![
            chave_da_pasta(Some(clientes)),
            format!("{PREFIXO_ITEM}{painel}"),
        ];
        assert_eq!(contar_apagar(&cofre, &chaves), (2, 3, false));
        assert_eq!(contar_apagar(&cofre, &chaves[..1]), (2, 3, true));
    }

    #[test]
    fn o_apagar_avisa_quando_leva_a_chave_dos_logs() {
        let (_pasta, mut cofre) = cofre_de_teste("tela-chave-dos-logs");
        let wifi = cofre
            .itens()
            .iter()
            .find(|i| i.titulo == "Wi-Fi")
            .unwrap()
            .id;
        let chave = cofre.criar_chave_dos_logs().unwrap();
        let da_chave = cofre.item_por_uid(&chave.uid).unwrap().id;
        let chave_do_item = |id: u64| format!("{PREFIXO_ITEM}{id}");
        assert!(apagar_leva_chave_dos_logs(
            &cofre,
            &[chave_do_item(da_chave)]
        ));
        assert!(apagar_leva_chave_dos_logs(
            &cofre,
            &[chave_do_item(wifi), chave_do_item(da_chave)]
        ));
        assert!(!apagar_leva_chave_dos_logs(&cofre, &[chave_do_item(wifi)]));
        // Uma pasta que não tem a chave dentro também não avisa.
        let clientes = id_da(&cofre, "Clientes");
        assert!(!apagar_leva_chave_dos_logs(
            &cofre,
            &[chave_da_pasta(Some(clientes))]
        ));
    }

    #[test]
    fn a_raiz_lista_as_pastas_de_cima_e_depois_os_itens() {
        let (_pasta, cofre) = cofre_de_teste("tela-raiz");
        let linhas = linhas_da_lista(&cofre, None, "");
        assert_eq!(titulos(&linhas), ["Clientes", "Pessoal", "Wi-Fi"]);
        assert!(linhas[0].key.starts_with(PREFIXO_PASTA) && linhas[0].drop_target);
        assert!(linhas[2].key.starts_with(PREFIXO_ITEM) && !linhas[2].drop_target);
        assert!(linhas[0].rank < linhas[2].rank, "as pastas ficam em cima");
        assert!(
            linhas.iter().all(|l| l.cells[3].0.is_empty()),
            "sem filtro, a coluna Pasta é vazia"
        );
    }

    #[test]
    fn uma_pasta_lista_so_o_que_esta_direto_nela() {
        let (_pasta, cofre) = cofre_de_teste("tela-pasta");
        let clientes = id_da(&cofre, "Clientes");
        let linhas = linhas_da_lista(&cofre, Some(clientes), "  ");
        assert_eq!(titulos(&linhas), ["ACME", "Contrato"]);
    }

    #[test]
    fn o_filtro_desce_nas_subpastas_e_mostra_o_caminho_relativo() {
        let (_pasta, cofre) = cofre_de_teste("tela-filtro");
        // Da raiz: o caminho completo, e o filtro casa sem acento e pelo usuário.
        let linhas = linhas_da_lista(&cofre, None, "painel");
        assert_eq!(titulos(&linhas), ["Painel"]);
        assert_eq!(linhas[0].cells[3].0, "Clientes/ACME");
        assert_eq!(
            titulos(&linhas_da_lista(&cofre, None, "CARTAO")),
            ["Cartão"]
        );
        assert_eq!(titulos(&linhas_da_lista(&cofre, None, "bia")), ["Cartão"]);
        // Casa o nome da pasta também.
        let linhas = linhas_da_lista(&cofre, None, "acme");
        assert_eq!(titulos(&linhas), ["ACME"]);
        assert_eq!(linhas[0].cells[3].0, "Clientes");
        // De uma pasta: só o que está embaixo dela, com o caminho a partir dela.
        let clientes = id_da(&cofre, "Clientes");
        let linhas = linhas_da_lista(&cofre, Some(clientes), "r");
        assert_eq!(titulos(&linhas), ["web-01", "Contrato"]);
        let linhas = linhas_da_lista(&cofre, Some(clientes), "ana");
        assert_eq!(titulos(&linhas), ["Painel"]);
        assert_eq!(linhas[0].cells[3].0, "ACME");
        assert!(
            linhas_da_lista(&cofre, Some(clientes), "cartao").is_empty(),
            "Pessoal fica fora"
        );
    }
}
