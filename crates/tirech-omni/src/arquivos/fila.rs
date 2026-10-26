//! A fila de transferências: o `DataGrid` com o andamento de cada uma, os botões e o diálogo
//! "O arquivo já existe". A fila é a mesma para todas as abas.

use std::cell::{Cell, RefCell};

use crate::t;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::time::Duration;

use tomni_ui::files::{
    Conflict, QueueState, TransferInfo, TransferKind, TransferState, TransferSummary, path,
};
use tomni_ui::prelude::*;

use super::Contexto;
use super::formato::{self, Estavel};

/// O estado em português.
pub fn estado_em_texto(estado: &TransferState) -> String {
    match estado {
        TransferState::Queued => t!("fila-estado-na-fila"),
        TransferState::Running => t!("fila-estado-em-andamento"),
        TransferState::Paused => t!("fila-estado-pausada"),
        TransferState::Done => t!("fila-estado-concluida"),
        TransferState::Failed(motivo) => t!("fila-estado-falhou", motivo = motivo.to_string()),
        TransferState::Skipped => t!("fila-estado-pulada"),
        TransferState::Cancelled => t!("fila-estado-cancelada"),
        TransferState::Asking => t!("fila-estado-esperando"),
    }
}

/// O estado da linha: o estado e, se a transferência deixou um aviso (o servidor que limita as
/// conexões, o FTPS que passou ao TLS 1.2), o aviso depois de um ponto.
fn estado_da_linha(t: &TransferInfo) -> String {
    match &t.notice {
        Some(aviso) => format!("{} · {aviso}", estado_em_texto(&t.state)),
        None => estado_em_texto(&t.state),
    }
}

/// A direção em português.
fn direcao(tipo: TransferKind) -> String {
    match tipo {
        TransferKind::Upload => t!("fila-direcao-envio"),
        TransferKind::Download => t!("fila-direcao-download"),
        TransferKind::Copy => t!("fila-direcao-copia"),
        TransferKind::Delete => t!("fila-direcao-apagar"),
    }
}

/// O ícone da direção.
fn icone(tipo: TransferKind) -> Symbol {
    match tipo {
        TransferKind::Upload => Symbol::ArrowUpload,
        TransferKind::Download => Symbol::ArrowDownload,
        TransferKind::Copy => Symbol::Copy,
        TransferKind::Delete => Symbol::Delete,
    }
}

/// O que o resumo da leva diz de arquivos: "itens" se há exclusões, "arquivo" no singular.
fn substantivo(r: &TransferSummary, quantos: usize) -> String {
    match (r.deletes, quantos) {
        (true, _) => t!("fila-substantivo-itens"),
        (false, 1) => t!("fila-substantivo-arquivo"),
        (false, _) => t!("fila-substantivo-arquivos"),
    }
}

/// O percentual da leva, arredondado para baixo: 100% só no fim. Pelos bytes, ou pelos arquivos
/// se a leva não tem bytes (só exclusões).
fn percentual(r: &TransferSummary) -> u64 {
    let (feito, total) = if r.bytes_total > 0 {
        (u128::from(r.bytes_done), u128::from(r.bytes_total))
    } else {
        (r.files_done as u128, r.files_total as u128)
    };
    if total == 0 {
        return 0;
    }
    (feito * 100 / total).min(99) as u64
}

/// O texto do resumo da leva, em cima da fila; vazio se não há leva.
pub fn texto_do_resumo(r: &TransferSummary) -> String {
    if r.files_total == 0 && r.failed == 0 && !r.counting {
        return String::new();
    }
    if r.counting {
        let mut texto = t!("fila-contando", n = r.files_total as i64);
        if r.bytes_total > 0 {
            texto.push_str(&format!(" · {}", formato::tamanho(r.bytes_total)));
        }
        return texto;
    }
    if r.state == QueueState::Idle {
        if r.failed == 0 {
            let mut texto = t!(
                "fila-concluido",
                n = r.files_total as i64,
                substantivo = substantivo(r, r.files_total)
            );
            if r.bytes_total > 0 {
                texto.push_str(&format!(" · {}", formato::tamanho(r.bytes_total)));
            }
            return texto;
        }
        let todos = r.files_total + r.failed;
        return t!(
            "fila-concluido-com-falhas",
            falhas = t!("fila-falhas", n = r.failed as i64),
            feitos = r.files_done as i64,
            todos = todos as i64,
            substantivo = substantivo(r, todos)
        );
    }
    let mut texto = t!(
        "fila-andamento",
        pct = percentual(r) as i64,
        feitos = r.files_done as i64,
        total = r.files_total as i64,
        substantivo = substantivo(r, r.files_total)
    );
    if r.bytes_total > 0 {
        texto.push_str(" · ");
        texto.push_str(&t!(
            "comum-parte-de-total",
            parte = formato::tamanho(r.bytes_done),
            total = formato::tamanho(r.bytes_total)
        ));
    }
    texto
}

/// O texto do tempo restante da leva; vazio se a fila está parada. `estavel` segura a faixa
/// para ela não tremer.
pub fn texto_do_restante(r: &TransferSummary, estavel: &mut Estavel) -> String {
    if r.state == QueueState::Idle || (r.files_total == 0 && r.failed == 0 && !r.counting) {
        return String::new();
    }
    match (r.state, r.stalled, &r.eta) {
        (QueueState::Paused, _, _) => t!("fila-estado-pausada"),
        (QueueState::Asking, _, _) => t!("fila-estado-esperando"),
        (_, Some(parada), _) => t!("fila-parada-ha", tempo = formato::duracao_curta(parada)),
        (_, None, Some(eta)) => formato::texto_da_faixa(&formato::estabilizar(estavel, eta, r.at)),
        _ => t!("fila-calculando"),
    }
}

/// As linhas do `DataGrid` da fila: a chave é o número da transferência. `estaveis` guarda a
/// faixa do tempo restante de cada linha (a histerese), e `agora` é o relógio da fila.
pub fn linhas_da_fila(
    itens: &[TransferInfo],
    estaveis: &mut HashMap<u64, Estavel>,
    agora: Duration,
) -> Vec<DataGridRow> {
    estaveis.retain(|id, _| {
        itens
            .iter()
            .any(|t| t.id == *id && t.state == TransferState::Running)
    });
    itens
        .iter()
        .map(|t| {
            let caminho = match t.kind {
                TransferKind::Delete => &t.from,
                _ => &t.to,
            };
            let nome = path::file_name(caminho).to_owned();
            let fracao = match (t.size, &t.state) {
                (_, TransferState::Done) => 1.0,
                (Some(total), _) if total > 0 => (t.done as f64 / total as f64).clamp(0.0, 1.0),
                _ => 0.0,
            };
            let tamanho = t.size.map_or_else(String::new, formato::tamanho);
            let em_curso = matches!(t.state, TransferState::Running);
            let restante = match (&t.remaining, em_curso) {
                (Some(eta), true) => formato::corpo_da_faixa(&formato::estabilizar(
                    estaveis.entry(t.id).or_default(),
                    eta,
                    agora,
                )),
                _ => String::new(),
            };
            DataGridRow::new(t.id.to_string())
                .cell(nome.clone(), CellValue::Text(nome))
                .cell(direcao(t.kind), CellValue::Text(direcao(t.kind)))
                .cell(tamanho, CellValue::Number(t.size.unwrap_or(0) as f64))
                .cell(
                    format!("{:.0}%", fracao * 100.0),
                    CellValue::Progress(fracao),
                )
                .cell(
                    if em_curso {
                        formato::velocidade(t.speed)
                    } else {
                        String::new()
                    },
                    CellValue::Number(if em_curso { t.speed } else { 0.0 }),
                )
                .cell(restante.clone(), CellValue::Text(restante))
                .cell(estado_da_linha(t), CellValue::Text(estado_da_linha(t)))
                .icon(icone(t.kind))
        })
        .collect()
}

/// A fração da barra do resumo: pelos bytes, ou pelos arquivos se não há bytes; cheia no fim.
fn fracao_da_barra(r: &TransferSummary) -> f64 {
    if r.state == QueueState::Idle {
        return 1.0;
    }
    let (feito, total) = if r.bytes_total > 0 {
        (r.bytes_done as f64, r.bytes_total as f64)
    } else {
        (r.files_done as f64, r.files_total as f64)
    };
    if total > 0.0 {
        (feito / total).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Os textos do resumo (a leva, a velocidade e o tempo restante), para ficarem ao lado dos
/// botões. A histerese do tempo restante só anda dentro do efeito.
fn textos_do_resumo(resumo: Signal<TransferSummary>) -> Row {
    let estavel = Rc::new(RefCell::new(Estavel::default()));
    let texto = signal(String::new());
    let restante = signal(String::new());
    effect(move || {
        let r = resumo.get();
        texto.set(texto_do_resumo(&r));
        untrack(|| restante.set(texto_do_restante(&r, &mut estavel.borrow_mut())));
    });
    Row::new()
        .grow(1)
        .gap(12)
        .align(Align::Center)
        .child(
            Text::new(move || texto.get())
                .id("fila-resumo")
                .max_lines(1)
                .grow(1),
        )
        .child(
            Text::new(move || {
                let r = resumo.get();
                if r.state == QueueState::Running {
                    formato::velocidade(r.speed)
                } else {
                    String::new()
                }
            })
            .id("fila-velocidade")
            .secondary(),
        )
        .child(
            Text::new(move || restante.get())
                .id("fila-restante")
                .body_strong(),
        )
}

/// As duas barras do resumo, uma sobre a outra: a que enche e a indeterminada de quando ainda
/// se conta. Somem quando não há leva. São duas porque o `.value` de uma `ProgressBar` não
/// vira indeterminado depois de criado.
fn barras_do_resumo(resumo: Signal<TransferSummary>) -> Column {
    let vazio = move || resumo.with(|r| texto_do_resumo(r).is_empty());
    Column::new()
        .child(
            ProgressBar::new()
                .id("fila-barra")
                .value(move || resumo.with(fracao_da_barra))
                .paused(move || resumo.with(|r| r.state == QueueState::Paused))
                .error(move || resumo.with(|r| r.state == QueueState::Idle && r.failed > 0))
                .visible(move || !vazio() && !resumo.with(|r| r.counting)),
        )
        .child(
            ProgressBar::new()
                .id("fila-barra-contando")
                .visible(move || !vazio() && resumo.with(|r| r.counting)),
        )
}

/// O resumo da fila (a linha de textos e as barras), a partir de um signal: os testes e os
/// snapshots passam um resumo fixo.
pub fn resumo_vista(resumo: Signal<TransferSummary>) -> Column {
    Column::new()
        .gap(4)
        .child(textos_do_resumo(resumo))
        .child(barras_do_resumo(resumo))
}

/// A fila: a tabela e, em cima, os botões.
pub fn vista(ctx: &Rc<Contexto>) -> Column {
    let transferencias = ctx.transferencias.clone();
    let linhas = signal(Vec::<DataGridRow>::new());
    let selecao = signal(BTreeSet::<String>::new());
    let ordem = signal(None::<(String, SortDirection)>);
    let pausada = signal(false);

    // O resumo da leva, recalculado pela fila a cada 250 ms enquanto anda.
    let resumo = signal(TransferSummary::default());
    {
        let vivo = transferencias.summary();
        effect(move || resumo.set(vivo.get()));
    }
    {
        let itens = transferencias.items();
        let estaveis = Rc::new(RefCell::new(HashMap::new()));
        effect(move || {
            let itens = itens.get();
            let agora = resumo.with(|r| r.at);
            linhas.set(linhas_da_fila(&itens, &mut estaveis.borrow_mut(), agora));
        });
    }

    let escolhidas = move || -> Vec<u64> {
        selecao.with_untracked(|s| s.iter().filter_map(|c| c.parse().ok()).collect())
    };

    let pausar = {
        let t = transferencias.clone();
        Button::new(t!("fila-pausar"))
            .id("fila-pausar")
            .icon(Symbol::Pause)
            .visible(move || !pausada.get())
            .on_click(move || {
                t.pause_all();
                pausada.set(true);
            })
    };
    let continuar = {
        let t = transferencias.clone();
        Button::new(t!("comum-continuar"))
            .id("fila-continuar")
            .icon(Symbol::Play)
            .visible(move || pausada.get())
            .on_click(move || {
                t.resume_all();
                pausada.set(false);
            })
    };
    let cancelar = {
        let t = transferencias.clone();
        Button::new(t!("comum-cancelar"))
            .id("fila-cancelar")
            .icon(Symbol::Dismiss)
            .on_click(move || {
                let ids = escolhidas();
                if ids.is_empty() {
                    t.cancel_all();
                } else {
                    for id in ids {
                        t.cancel(id);
                    }
                }
            })
    };
    let tentar = {
        let t = transferencias.clone();
        Button::new(t!("comum-tentar-de-novo"))
            .id("fila-tentar")
            .icon(Symbol::ArrowSync)
            .on_click(move || {
                for id in escolhidas() {
                    t.retry(id);
                }
            })
    };
    let limpar = {
        let t = transferencias.clone();
        Button::new(t!("fila-limpar-concluidas"))
            .id("fila-limpar")
            .icon(Symbol::Delete)
            .on_click(move || t.clear_finished())
    };

    let grade = DataGrid::bind(linhas, selecao)
        .id("fila")
        .grow(1)
        .columns([
            DataGridColumn::new("arquivo", t!("comum-arquivo")).width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("direcao", t!("fila-direcao")).width(ColumnWidth::Pixels(90.0)),
            DataGridColumn::new("tamanho", t!("comum-tamanho"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("progresso", t!("fila-progresso"))
                .width(ColumnWidth::Pixels(130.0)),
            DataGridColumn::new("velocidade", t!("fila-velocidade"))
                .width(ColumnWidth::Pixels(100.0))
                .align_end(),
            DataGridColumn::new("restante", t!("fila-restante"))
                .width(ColumnWidth::Pixels(110.0))
                .align_end(),
            DataGridColumn::new("estado", t!("comum-estado")).width(ColumnWidth::Pixels(260.0)),
        ])
        .sort(ordem)
        .empty_text(t!("fila-nenhuma-transferencia"));

    Column::new()
        .grow(1)
        .gap(4)
        .child(
            Row::new()
                .gap(4)
                .align(Align::Center)
                .child(pausar)
                .child(continuar)
                .child(cancelar)
                .child(tentar)
                .child(limpar)
                .child(textos_do_resumo(resumo)),
        )
        .child(barras_do_resumo(resumo))
        .child(grade)
}

/// O diálogo "O arquivo já existe": sobrescrever, retomar, pular ou renomear, e "Fazer o mesmo
/// nos próximos". Fechar sem escolher (Esc) cancela aquela transferência.
pub fn dialogo_de_conflito(ctx: &Rc<Contexto>) -> ContentDialog {
    let transferencias = ctx.transferencias.clone();
    let aberto = signal(false);
    let todos = signal(false);
    let texto = signal(String::new());
    // A pergunta que o diálogo mostra e que ainda não foi respondida.
    let atual: Rc<Cell<Option<u64>>> = Rc::default();
    // A última pergunta que abriu o diálogo: `asking` guarda a pergunta até a fila notar a
    // resposta, e ela não pode reabrir o diálogo.
    let ultima: Rc<Cell<Option<u64>>> = Rc::default();

    {
        let (t, atual, ultima) = (
            transferencias.clone(),
            Rc::clone(&atual),
            Rc::clone(&ultima),
        );
        let pergunta = transferencias.asking();
        effect(move || {
            let aberta = aberto.get();
            let pergunta = pergunta.get();
            untrack(|| {
                if aberta {
                    return;
                }
                if let Some(id) = atual.take() {
                    t.cancel(id);
                }
                let Some(p) = pergunta else { return };
                if ultima.get() == Some(p.id) {
                    return;
                }
                ultima.set(Some(p.id));
                atual.set(Some(p.id));
                todos.set(false);
                texto.set(t!(
                    "fila-conflito-detalhe",
                    caminho = p.path.as_str(),
                    origem_tamanho = formato::tamanho(p.source.size),
                    origem_data = formato::data(p.source.modified),
                    destino_tamanho = formato::tamanho(p.target.size),
                    destino_data = formato::data(p.target.modified),
                ));
                aberto.set(true);
            });
        });
    }

    let responder = {
        let (t, atual) = (transferencias, Rc::clone(&atual));
        Rc::new(move |escolha: Conflict| {
            if let Some(id) = atual.take() {
                t.answer(id, escolha, todos.get_untracked());
            }
        })
    };
    let botao = |id: &'static str, rotulo: &'static str, escolha: Conflict| {
        let responder = Rc::clone(&responder);
        Button::new(rotulo).id(id).on_click(move || {
            responder(escolha);
            aberto.set(false);
        })
    };

    ContentDialog::new(t!("fila-o-arquivo-ja-existe"), aberto)
        .id("conflito-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(move || texto.get()).id("conflito-texto").wrap())
                .child(
                    Row::new()
                        .gap(4)
                        .child(botao("conflito-retomar", "Retomar", Conflict::Resume))
                        .child(botao("conflito-renomear", "Renomear", Conflict::Rename)),
                )
                .child(
                    CheckBox::bind(todos, t!("fila-fazer-o-mesmo-nos-proximos"))
                        .id("conflito-todos"),
                ),
        )
        .primary(t!("fila-sobrescrever"), {
            let responder = Rc::clone(&responder);
            move || responder(Conflict::Overwrite)
        })
        .secondary(t!("fila-pular"), move || responder(Conflict::Skip))
        .close(t!("comum-cancelar"))
}

#[cfg(test)]
mod testes {
    use tomni_ui::files::Eta;

    use super::*;

    fn info(
        id: u64,
        tipo: TransferKind,
        para: &str,
        tamanho: Option<u64>,
        feito: u64,
    ) -> TransferInfo {
        TransferInfo {
            id,
            kind: tipo,
            from: "/origem/a.txt".to_owned(),
            to: para.to_owned(),
            size: tamanho,
            done: feito,
            speed: 0.0,
            state: TransferState::Running,
            notice: None,
            is_dir: false,
            remaining: None,
        }
    }

    fn linhas(itens: &[TransferInfo]) -> Vec<DataGridRow> {
        linhas_da_fila(itens, &mut HashMap::new(), Duration::ZERO)
    }

    fn eta(baixo: u64, provavel: u64, alto: u64) -> Eta {
        Eta {
            low: Duration::from_secs(baixo),
            likely: Duration::from_secs(provavel),
            high: Duration::from_secs(alto),
        }
    }

    /// Uma leva andando: 128 de 340 arquivos, 1,2 GB de 3,4 GB.
    fn andando() -> TransferSummary {
        TransferSummary {
            state: QueueState::Running,
            files_total: 340,
            files_done: 128,
            bytes_total: 3_650_722_201,
            bytes_done: 1_288_490_188,
            speed: 12.4 * 1024.0 * 1024.0,
            eta: Some(eta(170, 230, 290)),
            ..TransferSummary::default()
        }
    }

    #[test]
    fn a_linha_leva_o_nome_do_destino_e_o_progresso() {
        let linhas = linhas(&[info(7, TransferKind::Upload, "/srv/a.txt", Some(200), 50)]);
        assert_eq!(linhas.len(), 1);
        assert_eq!(linhas[0].key, "7");
        assert_eq!(linhas[0].cells[0].0, "a.txt");
        assert_eq!(linhas[0].cells[1].0, "Envio");
        assert_eq!(linhas[0].cells[3].1, CellValue::Progress(0.25));
    }

    #[test]
    fn apagar_mostra_o_nome_da_origem_e_sem_tamanho_o_progresso_e_zero() {
        let linhas = linhas(&[info(1, TransferKind::Delete, "", None, 0)]);
        assert_eq!(linhas[0].cells[0].0, "a.txt");
        assert_eq!(linhas[0].cells[3].1, CellValue::Progress(0.0));
    }

    #[test]
    fn concluida_vale_cem_por_cento() {
        let mut t = info(2, TransferKind::Download, "/tmp/b", Some(10), 10);
        t.state = TransferState::Done;
        assert_eq!(linhas(&[t])[0].cells[3].1, CellValue::Progress(1.0));
    }

    #[test]
    fn o_aviso_da_transferencia_vai_no_estado() {
        let mut t = info(3, TransferKind::Upload, "/srv/c.txt", Some(10), 0);
        t.notice = Some("o servidor aceita só 1 conexão".to_owned());
        assert_eq!(
            linhas(&[t])[0].cells[6].0,
            "Em andamento · o servidor aceita só 1 conexão"
        );
    }

    #[test]
    fn o_resumo_andando_diz_percentual_arquivos_e_bytes() {
        assert_eq!(
            texto_do_resumo(&andando()),
            "35% · 128 de 340 arquivos · 1,2 GB de 3,4 GB"
        );
        let um = TransferSummary {
            files_total: 1,
            files_done: 0,
            bytes_total: 2048,
            bytes_done: 1024,
            ..andando()
        };
        assert_eq!(
            texto_do_resumo(&um),
            "50% · 0 de 1 arquivo · 1,0 KB de 2,0 KB"
        );
    }

    #[test]
    fn cem_por_cento_so_no_fim() {
        let quase = TransferSummary {
            bytes_done: andando().bytes_total,
            ..andando()
        };
        assert!(texto_do_resumo(&quase).starts_with("99% "));
    }

    #[test]
    fn o_resumo_contando_mostra_o_que_ja_se_achou() {
        let r = TransferSummary {
            counting: true,
            files_total: 120,
            bytes_total: 0,
            ..andando()
        };
        assert_eq!(texto_do_resumo(&r), "Contando arquivos… 120 encontrados");
        let com_bytes = TransferSummary {
            bytes_total: 1_503_238_554,
            ..r
        };
        assert_eq!(
            texto_do_resumo(&com_bytes),
            "Contando arquivos… 120 encontrados · 1,4 GB"
        );
    }

    #[test]
    fn o_resumo_concluido_e_o_com_falhas() {
        let pronto = TransferSummary {
            state: QueueState::Idle,
            files_done: 340,
            bytes_done: 3_650_722_201,
            eta: None,
            ..andando()
        };
        assert_eq!(
            texto_do_resumo(&pronto),
            "Concluído · 340 arquivos · 3,4 GB"
        );
        let duas = TransferSummary {
            files_total: 338,
            files_done: 338,
            failed: 2,
            ..pronto.clone()
        };
        assert_eq!(
            texto_do_resumo(&duas),
            "Concluído com 2 falhas · 338 de 340 arquivos"
        );
        let uma = TransferSummary {
            files_total: 1,
            files_done: 1,
            failed: 1,
            ..pronto
        };
        assert_eq!(
            texto_do_resumo(&uma),
            "Concluído com 1 falha · 1 de 2 arquivos"
        );
    }

    #[test]
    fn so_falhas_ainda_dizem_o_que_houve() {
        let r = TransferSummary {
            state: QueueState::Idle,
            files_total: 0,
            files_done: 0,
            failed: 2,
            ..TransferSummary::default()
        };
        assert_eq!(
            texto_do_resumo(&r),
            "Concluído com 2 falhas · 0 de 2 arquivos"
        );
    }

    #[test]
    fn so_exclusoes_dizem_itens() {
        let r = TransferSummary {
            state: QueueState::Running,
            deletes: true,
            files_total: 5,
            files_done: 2,
            bytes_total: 0,
            bytes_done: 0,
            ..andando()
        };
        assert_eq!(texto_do_resumo(&r), "40% · 2 de 5 itens");
        let pronto = TransferSummary {
            state: QueueState::Idle,
            files_done: 5,
            ..r
        };
        assert_eq!(texto_do_resumo(&pronto), "Concluído · 5 itens");
    }

    #[test]
    fn sem_leva_o_resumo_e_vazio() {
        let vazio = TransferSummary::default();
        assert_eq!(texto_do_resumo(&vazio), "");
        assert_eq!(texto_do_restante(&vazio, &mut Estavel::default()), "");
    }

    #[test]
    fn o_restante_segue_o_estado_da_fila() {
        let mut e = Estavel::default();
        assert_eq!(texto_do_restante(&andando(), &mut e), "Faltam 3–5 min");
        let pausada = TransferSummary {
            state: QueueState::Paused,
            ..andando()
        };
        assert_eq!(texto_do_restante(&pausada, &mut e), "Pausada");
        let perguntando = TransferSummary {
            state: QueueState::Asking,
            ..andando()
        };
        assert_eq!(
            texto_do_restante(&perguntando, &mut e),
            "Esperando resposta"
        );
        let parada = TransferSummary {
            stalled: Some(Duration::from_secs(12)),
            eta: None,
            ..andando()
        };
        assert_eq!(texto_do_restante(&parada, &mut e), "Parada há 12 s");
        let calculando = TransferSummary {
            eta: None,
            ..andando()
        };
        assert_eq!(texto_do_restante(&calculando, &mut e), "Calculando…");
        let parou = TransferSummary {
            state: QueueState::Idle,
            ..andando()
        };
        assert_eq!(texto_do_restante(&parou, &mut e), "");
    }

    #[test]
    fn a_celula_restante_so_aparece_em_andamento() {
        let mut andando = info(1, TransferKind::Upload, "/srv/a.txt", Some(200), 50);
        andando.remaining = Some(eta(170, 230, 290));
        assert_eq!(linhas(&[andando.clone()])[0].cells[5].0, "3–5 min");

        let mut pronta = andando.clone();
        pronta.state = TransferState::Done;
        assert_eq!(linhas(&[pronta])[0].cells[5].0, "");
        let mut na_fila = andando;
        na_fila.state = TransferState::Queued;
        assert_eq!(linhas(&[na_fila])[0].cells[5].0, "");
    }

    #[test]
    fn a_faixa_da_linha_nao_treme_e_a_da_linha_que_saiu_se_apaga() {
        let mut estaveis = HashMap::new();
        let mut t = info(1, TransferKind::Upload, "/srv/a.txt", Some(200), 50);
        t.remaining = Some(eta(180, 240, 300));
        let primeira = linhas_da_fila(&[t.clone()], &mut estaveis, Duration::ZERO)[0].cells[5]
            .0
            .clone();
        t.remaining = Some(eta(180, 250, 330));
        let depois = linhas_da_fila(&[t], &mut estaveis, Duration::from_secs(1))[0].cells[5]
            .0
            .clone();
        assert_eq!(primeira, depois);
        assert_eq!(estaveis.len(), 1);
        linhas_da_fila(&[], &mut estaveis, Duration::from_secs(2));
        assert!(estaveis.is_empty());
    }

    #[test]
    fn a_fracao_da_barra_enche_no_fim() {
        assert!((fracao_da_barra(&andando()) - 0.353).abs() < 0.001);
        let pronto = TransferSummary {
            state: QueueState::Idle,
            ..andando()
        };
        assert_eq!(fracao_da_barra(&pronto), 1.0);
        assert_eq!(fracao_da_barra(&TransferSummary::default()), 1.0);
    }
}
