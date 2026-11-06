//! O diagnóstico dos discos na aba Disco do Desempenho: uma linha com o resumo (ou, quando não dá
//! para ler, dizendo por quê), os alertas e o botão "Detalhes", que troca as tabelas da aba pela
//! tabela com a vida restante, o total gravado, as horas ligado e as vezes que o disco foi ligado.
//! Um disco com alerta abre a tabela sozinho. Sem nada a dizer, o bloco inteiro sai do layout. A
//! leitura e os textos estão em [`crate::saude_discos`]; o Omni guarda por servidor (`tela/saude_discos.rs`) e
//! o `htop-omni` lê uma vez, sem gravar ([`FonteDaSaude::so_deste_computador`]).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{DISCO, Fonte, Processos};
use crate::agente::{acordar, ler_contador};
use crate::saude_discos::{self as saude, Diagnostico, SaudeDoDisco};
use crate::t;

/// De onde a aba Disco tira o diagnóstico da máquina (`None` é este computador).
#[derive(Clone)]
pub(in crate::tela) struct FonteDaSaude {
    /// O que já se sabe da máquina. Reativo.
    pub(in crate::tela) ler: Rc<dyn Fn(Option<Session>) -> Option<Diagnostico>>,
    /// Pede a leitura; com `true`, mesmo com uma que ainda vale.
    pub(in crate::tela) pedir: Rc<dyn Fn(Option<Session>, bool)>,
}

impl FonteDaSaude {
    /// A do `htop-omni`: só este computador, lido numa thread quando a aba Disco abre e guardado
    /// na memória (o `htop-omni` não grava nada). Com `ligado` falso, não lê nada. O `sudo` fica
    /// para o botão, ou para quando `sudo_provado` diz que ele já funcionou aqui.
    pub(in crate::tela) fn so_deste_computador(
        ler: Arc<dyn Fn(bool) -> Diagnostico + Send + Sync>,
        ligado: bool,
        sudo_provado: bool,
    ) -> Self {
        let lido: Signal<Option<Diagnostico>> = signal(None);
        let pronto: Signal<u64> = signal(0);
        let entrega: Arc<Mutex<Option<Diagnostico>>> = Arc::default();
        let (correndo, ja_leu) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
        let (e, c) = (Arc::clone(&entrega), Rc::clone(&correndo));
        effect(move || {
            ler_contador(pronto);
            let novo = e.lock().unwrap_or_else(PoisonError::into_inner).take();
            if novo.is_some() {
                c.set(false);
                lido.set(novo);
            }
        });
        Self {
            ler: Rc::new(move |_| lido.get()),
            pedir: Rc::new(move |_, forcar| {
                if !ligado || correndo.get() || (ja_leu.get() && !forcar) {
                    return;
                }
                correndo.set(true);
                ja_leu.set(true);
                let (ler, entrega) = (Arc::clone(&ler), Arc::clone(&entrega));
                std::thread::spawn(move || {
                    let diagnostico = ler(forcar || sudo_provado);
                    *entrega.lock().unwrap_or_else(PoisonError::into_inner) = Some(diagnostico);
                    acordar(pronto);
                });
            }),
        }
    }
}

/// A sessão da máquina com este rótulo no seletor.
fn sessao_da_maquina(fonte: &Fonte, maquina: &str) -> Option<Session> {
    fonte
        .maquinas()
        .into_iter()
        .find(|(rotulo, _)| rotulo == maquina)
        .and_then(|(_, sessao)| sessao)
}

/// O texto de um valor que o disco pode não informar: um traço, como nos Sensores.
const SEM_VALOR: &str = "—";

/// Uma linha da tabela. O que o disco não informa fica com um traço e, na ordenação, antes dos
/// demais.
fn linha(disco: &SaudeDoDisco) -> DataGridRow {
    // A ordenação não precisa de mais que a precisão do `f64`.
    let numero = |valor: Option<u64>| CellValue::Number(valor.map_or(-1.0, |v| v as f64));
    let texto = |valor: Option<String>| valor.unwrap_or_else(|| SEM_VALOR.to_owned());
    let inteiro = |valor: Option<u64>| texto(valor.map(crate::i18n::inteiro));
    let vida = texto(disco.vida_restante.map(|v| format!("{v}%")));
    DataGridRow::new(format!("{}-{}", disco.dispositivo, disco.tipo))
        .cell(disco.nome(), CellValue::Text(disco.nome()))
        .cell(
            texto(disco.modelo.clone()),
            CellValue::Text(disco.modelo.clone().unwrap_or_default()),
        )
        .cell(disco.estado(), CellValue::Text(disco.estado()))
        .cell(vida, numero(disco.vida_restante.map(u64::from)))
        .cell(
            texto(disco.gravado.map(saude::texto_do_gravado)),
            numero(disco.gravado),
        )
        .cell(
            texto(disco.horas.map(saude::texto_das_horas)),
            numero(disco.horas),
        )
        .cell(inteiro(disco.ligacoes), numero(disco.ligacoes))
        .cell(inteiro(disco.realocados), numero(disco.realocados))
}

/// O texto do estado acima da tabela. `None`: não há o que dizer.
fn legenda(diagnostico: &Diagnostico) -> Option<String> {
    match diagnostico {
        Diagnostico::Discos(discos) if discos.is_empty() => Some(t!("saude-discos-nenhum")),
        Diagnostico::Discos(discos) => {
            let com_alerta = discos.iter().filter(|d| !d.alertas().is_empty()).count();
            Some(t!(
                "saude-discos-resumo",
                n = discos.len() as u64,
                alerta = com_alerta as u64
            ))
        }
        Diagnostico::Virtual => Some(t!("saude-discos-virtual")),
        Diagnostico::SemPermissao => Some(t!("saude-discos-sem-permissao")),
        Diagnostico::SemSmartctl => Some(t!("saude-discos-sem-smartctl")),
        Diagnostico::OutroSistema => None,
    }
}

/// Os efeitos: pede a leitura quando a aba Disco aparece com uma máquina, e passa o que se sabe
/// da máquina escolhida para o bloco.
pub(super) fn ligar(fonte: &Fonte, p: Processos) {
    let f = fonte.clone();
    // A máquina já pedida: a leitura do Desempenho anda a cada segundo e não pede de novo.
    let pedida: RefCell<Option<String>> = RefCell::new(None);
    effect(move || {
        let maquina = p.maquina.get();
        if !p.aba.with(|a| a == DISCO) || !p.quadro.with(Option::is_some) {
            *pedida.borrow_mut() = None;
            return;
        }
        if pedida.borrow().as_deref() == Some(maquina.as_str()) {
            return;
        }
        *pedida.borrow_mut() = Some(maquina.clone());
        untrack(|| (f.saude.pedir)(sessao_da_maquina(&f, &maquina), false));
    });
    let f = fonte.clone();
    effect(move || {
        let maquina = p.maquina.get();
        let diagnostico = (f.saude.ler)(sessao_da_maquina(&f, &maquina));
        if p.saude.with_untracked(|atual| *atual == diagnostico) {
            return;
        }
        let discos = diagnostico.as_ref().map_or(&[][..], Diagnostico::discos);
        p.linhas_da_saude.set(discos.iter().map(linha).collect());
        // Um disco com problema abre a tabela, que traz o alerta: não passa despercebido.
        if discos.iter().any(|d| !d.alertas().is_empty()) {
            p.saude_aberta.set(true);
        }
        p.saude.set(diagnostico);
    });
}

/// Se a tabela está à mostra: o botão ligado e algum disco lido. No lugar dela ficam as tabelas
/// da aba, porque em 1280×720 não há altura para as duas.
pub(super) fn tabela_a_mostra(p: Processos) -> bool {
    p.saude_aberta.get()
        && p.saude
            .with(|d| d.as_ref().is_some_and(|d| !d.discos().is_empty()))
}

/// A linha do estado e, com a tabela à mostra, o alerta da aba Disco.
pub(super) fn bloco(fonte: &Fonte, p: Processos) -> impl View + use<> {
    let f = fonte.clone();
    let texto = move || p.saude.with(|d| d.as_ref().and_then(legenda));
    let alertas = move || {
        p.saude
            .with(|d| d.as_ref().map(Diagnostico::alertas).unwrap_or_default())
    };
    let tem_discos = move || {
        p.saude
            .with(|d| d.as_ref().is_some_and(|d| !d.discos().is_empty()))
    };
    Column::new()
        .gap(8)
        .visible(move || texto().is_some())
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Text::new(move || texto().unwrap_or_default())
                        .id("desempenho-disco-saude-estado")
                        .grow(1),
                )
                .child(
                    ToggleButton::bind(p.saude_aberta, t!("saude-discos-detalhes"))
                        .id("desempenho-disco-saude-detalhes")
                        .visible(tem_discos),
                )
                .child(
                    Button::new(t!("saude-discos-verificar"))
                        .id("desempenho-disco-saude-verificar")
                        .on_click(move || {
                            let maquina = p.maquina.get_untracked();
                            (f.saude.pedir)(sessao_da_maquina(&f, &maquina), true);
                        }),
                ),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Warning)
                .id("desempenho-disco-saude-alerta")
                .title(t!("saude-discos-alerta-titulo"))
                .message(move || alertas().join("; "))
                .visible(move || tabela_a_mostra(p) && !alertas().is_empty()),
        )
}

/// A tabela dos discos, com o botão "Detalhes" ligado.
pub(super) fn tabela(p: Processos) -> impl View + use<> {
    DataGrid::bind(p.linhas_da_saude, p.escolhidos_da_saude)
        .id("desempenho-disco-saude")
        .grow(1)
        .visible(move || tabela_a_mostra(p))
        .columns([
            DataGridColumn::new("disco", t!("saude-discos-coluna-disco"))
                .width(ColumnWidth::Pixels(100.0)),
            DataGridColumn::new("modelo", t!("saude-discos-coluna-modelo"))
                .width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("saude", t!("saude-discos-coluna-saude"))
                .width(ColumnWidth::Pixels(90.0)),
            DataGridColumn::new("vida", t!("saude-discos-coluna-vida"))
                .width(ColumnWidth::Pixels(110.0))
                .align_end(),
            DataGridColumn::new("gravado", t!("saude-discos-coluna-gravado"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("horas", t!("saude-discos-coluna-horas"))
                .width(ColumnWidth::Pixels(160.0))
                .align_end(),
            DataGridColumn::new("ligacoes", t!("saude-discos-coluna-ligacoes"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("realocados", t!("saude-discos-coluna-realocados"))
                .width(ColumnWidth::Pixels(100.0))
                .align_end(),
        ])
}
