//! O resumo da fila de transferências nos dois temas (M40): a linha de textos e a barra, a partir
//! de resumos fixos. Snapshots e lint.

use std::time::Duration;

use tirech_omni::arquivos::fila;
use tomni_ui::files::{Eta, QueueState, TransferSummary};
use tomni_ui::prelude::*;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

/// Os tamanhos do app são em base 1024: 3,4 GB, 42% deles e 12,4 MB/s.
const GIB: f64 = 1_073_741_824.0;
const MIB: f64 = 1_048_576.0;

/// Uma leva andando: 42%, 128 de 340 arquivos, 12,4 MB/s e de 3 a 5 minutos pela frente.
fn andando() -> TransferSummary {
    TransferSummary {
        state: QueueState::Running,
        files_total: 340,
        files_done: 128,
        bytes_total: (3.4 * GIB) as u64,
        bytes_done: (3.4 * GIB * 0.42) as u64,
        speed: 12.4 * MIB,
        eta: Some(Eta {
            low: Duration::from_secs(180),
            likely: Duration::from_secs(240),
            high: Duration::from_secs(300),
        }),
        at: Duration::from_secs(60),
        ..TransferSummary::default()
    }
}

fn pausada() -> TransferSummary {
    TransferSummary {
        state: QueueState::Paused,
        speed: 0.0,
        eta: None,
        ..andando()
    }
}

/// A leva ainda achando o que copiar: sem total, sem tempo e com a barra indeterminada.
fn contando() -> TransferSummary {
    TransferSummary {
        state: QueueState::Running,
        files_total: 120,
        counting: true,
        speed: 0.0,
        eta: None,
        at: Duration::from_secs(5),
        ..TransferSummary::default()
    }
}

fn tela(resumo: TransferSummary, tema: Theme) -> TestApp {
    TestApp::new(move || {
        Form::new("Fila").density(Density::Compact).child(
            Column::new()
                .padding(Insets::symmetric(12.0, 12.0))
                .child(fila::resumo_vista(signal(resumo.clone()))),
        )
    })
    .size(900, 80)
    .theme(tema)
}

/// Confere o lint e o snapshot `{prefixo}-{tema}` do `resumo`, no tema `tema`.
fn conferir(prefixo: &str, resumo: TransferSummary, nome: &str, tema: Theme) {
    let mut app = tela(resumo, tema);
    app.assert_lint();
    app.assert_snapshot(&format!("{prefixo}-{nome}"));
}

/// Um teste por resumo e por tema: assim cada snapshot sem referência (ou que mudou) deixa o seu
/// `.snap.new.png` para ser aberto antes de aprovar.
macro_rules! snapshots {
    ($($teste:ident: $prefixo:literal, $resumo:ident, $nome:literal, $tema:expr;)*) => {
        $(
            #[test]
            fn $teste() {
                conferir($prefixo, $resumo(), $nome, $tema);
            }
        )*
    };
}

snapshots! {
    o_resumo_andando_no_tema_claro: "fila-resumo", andando, "claro", Theme::Light;
    o_resumo_andando_no_tema_escuro: "fila-resumo", andando, "escuro", Theme::Dark;
    o_resumo_pausado_no_tema_claro: "fila-resumo-pausada", pausada, "claro", Theme::Light;
    o_resumo_pausado_no_tema_escuro: "fila-resumo-pausada", pausada, "escuro", Theme::Dark;
    o_resumo_contando_no_tema_claro: "fila-resumo-contando", contando, "claro", Theme::Light;
    o_resumo_contando_no_tema_escuro: "fila-resumo-contando", contando, "escuro", Theme::Dark;
}
