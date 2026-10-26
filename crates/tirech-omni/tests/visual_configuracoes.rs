//! A página Configurações do M35 nos dois temas: o topo, o Cofre com o gerador aberto, os logs
//! do terminal (M53), uma janela estreita e a busca. Snapshots e lint.

mod comum;

use std::path::PathBuf;

use tomni_ui::lint::Severity;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_pura};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

/// A pasta tem um nome fixo: o card da pasta de dados mostra o caminho e o snapshot precisa
/// dele igual em toda execução.
fn pasta_fixa(teste: &str) -> Pasta {
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-configuracoes-{teste}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    pasta
}

fn abrir(teste: &str, tema: Theme, largura: u32) -> (Pasta, Montagem) {
    let pasta = pasta_fixa(teste);
    let mut m = montar_pura(&pasta, tema);
    // O clique vem antes de estreitar: no Início a grade de cartões não cabe em 560 px e o
    // cartão pode cair fora da janela.
    m.app.resize(1000, 760);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.resize(largura, 760);
    (pasta, m)
}

/// Confere o lint (menos a barra de título da casca, que não encolhe abaixo de 640 px de
/// largura e que não é da página) e o snapshot.
fn conferir(app: &mut TestApp, nome: &str, tema: &str) {
    let erros: Vec<String> = app
        .lint()
        .iter()
        .filter(|i| i.severity == Severity::Error && !i.widget.starts_with("barra-de-titulo"))
        .map(ToString::to_string)
        .collect();
    assert!(
        erros.is_empty(),
        "a página não passa no lint:\n  {}",
        erros.join("\n  ")
    );
    app.assert_snapshot(&format!("configuracoes-{nome}-{tema}"));
}

#[test]
fn o_topo_da_pagina() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = abrir("topo", tema, 1000);
        conferir(&mut m.app, "topo", nome);
    }
}

#[test]
fn o_cofre_com_o_gerador_aberto() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = abrir("cofre", tema, 1000);
        m.app.click("config-secao-cofre");
        m.app.click("config-cofre-gerador-cabecalho");
        m.app.scroll("config-rolagem", 320.0);
        conferir(&mut m.app, "cofre", nome);
    }
}

#[test]
fn os_logs_do_terminal_com_o_cifrado() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = abrir("logs", tema, 1000);
        m.app.click("config-secao-terminal");
        let grupo = tirech_omni::configuracoes::catalogo::id_do_card("terminal.logs");
        // O cabeçalho do grupo está fora da tela: rola até ele aparecer.
        let cabecalho = format!("{grupo}-cabecalho");
        for _ in 0..40 {
            if m.app.ui().find(&cabecalho).is_some() && m.app.visible(&cabecalho) {
                break;
            }
            m.app.scroll("config-rolagem", 300.0);
        }
        m.app.click(&cabecalho);
        m.app.scroll("config-rolagem", 420.0);
        conferir(&mut m.app, "logs", nome);
    }
}

#[test]
fn a_janela_estreita() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = abrir("estreita", tema, 560);
        conferir(&mut m.app, "estreita", nome);
    }
}

#[test]
fn a_busca_por_fonte() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = abrir("busca", tema, 1000);
        m.app.type_into("config-busca", "fonte");
        // O Tab fecha a lista de sugestões, que cobriria o título da seção.
        m.app.tab();
        conferir(&mut m.app, "busca", nome);
    }
}
