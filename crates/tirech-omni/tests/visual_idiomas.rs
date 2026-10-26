//! O app em inglês (M43) nos dois temas: o Início, os Terminais com a grade de propriedades, os
//! Arquivos sem abas, o topo das Configurações e o Cofre. Snapshots e lint: o inglês tem outro
//! comprimento e pode cortar texto.

mod comum;

use std::path::PathBuf;

use tirech_omni::cofre::{Item, Tipo};
use tirech_omni::configuracoes::Idioma;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::lint::Severity;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, fixar_idioma, montar, montar_pura};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

/// A pasta tem um nome fixo: a página de Configurações mostra o caminho e o snapshot precisa
/// dele igual em toda execução.
fn pasta_fixa(teste: &str) -> Pasta {
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-idiomas-{teste}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    pasta
}

/// O lint (menos a barra de título da casca, que não encolhe) e o snapshot.
fn conferir(app: &mut TestApp, nome: &str, tema: &str) {
    let erros: Vec<String> = app
        .lint()
        .iter()
        .filter(|i| i.severity == Severity::Error && !i.widget.starts_with("barra-de-titulo"))
        .map(ToString::to_string)
        .collect();
    assert!(
        erros.is_empty(),
        "a tela em inglês não passa no lint:\n  {}",
        erros.join("\n  ")
    );
    app.assert_snapshot(&format!("en-{nome}-{tema}"));
}

fn item(titulo: &str, usuario: &str, senha: &str, tipo: Tipo) -> Item {
    Item {
        tipo,
        titulo: titulo.into(),
        usuario: usuario.into(),
        senha: senha.into(),
        ..Item::default()
    }
}

fn montar_em_ingles(teste: &str, tema: Theme, tree: &Tree) -> (Pasta, Montagem) {
    fixar_idioma(Idioma::EnUs);
    let pasta = pasta_fixa(teste);
    pasta.semear(tree);
    let m = montar_pura(&pasta, tema);
    (pasta, m)
}

#[test]
fn o_inicio_em_ingles() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = montar_em_ingles(&format!("inicio-{nome}"), tema, &exemplo());
        assert_eq!(m.app.text("navegacao"), "inicio");
        conferir(&mut m.app, "inicio", nome);
    }
}

#[test]
fn os_terminais_com_propriedades_em_ingles() {
    for (nome, tema) in TEMAS {
        fixar_idioma(Idioma::EnUs);
        let pasta = pasta_fixa(&format!("terminais-{nome}"));
        pasta.semear(&exemplo());
        let mut m = montar(&pasta, tema);
        m.app.expand("arvore", "1");
        m.app.click("arvore-3");
        conferir(&mut m.app, "terminais", nome);
    }
}

#[test]
fn os_arquivos_sem_abas_em_ingles() {
    for (nome, tema) in TEMAS {
        fixar_idioma(Idioma::EnUs);
        let pasta = pasta_fixa(&format!("arquivos-{nome}"));
        let mut sftp = Connection::new("servidor", Kind::Sftp);
        sftp.host = "arquivos.exemplo.com.br".into();
        sftp.user = "ana".into();
        let mut tree = Tree::default();
        tree.add_connection(None, sftp);
        pasta.semear(&tree);
        pasta.pagina_inicial_padrao("inicio");
        let mut m = montar_pura(&pasta, tema);
        m.app.click("navegacao-arquivos");
        assert_eq!(m.app.text("navegacao"), "arquivos");
        conferir(&mut m.app, "arquivos-vazia", nome);
    }
}

#[test]
fn o_topo_das_configuracoes_em_ingles() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = montar_em_ingles(&format!("config-{nome}"), tema, &exemplo());
        m.app.resize(1000, 760);
        m.app.click("inicio-ferramenta-configuracoes");
        conferir(&mut m.app, "configuracoes-topo", nome);
    }
}

#[test]
fn o_cofre_em_ingles() {
    for (nome, tema) in TEMAS {
        fixar_idioma(Idioma::EnUs);
        let pasta = pasta_fixa(&format!("cofre-{nome}"));
        pasta.semear(&exemplo());
        pasta.semear_cofre_em(vec![
            ("Clientes", item("web-01", "root", "outra", Tipo::Ssh)),
            ("Pessoal", item("Banco", "bia", "1234", Tipo::Senha)),
        ]);
        pasta.pagina_inicial_padrao("inicio");
        let mut m = montar_pura(&pasta, tema);
        m.app.resize(1600, 800);
        m.app.click("navegacao-cofre");
        assert_eq!(m.app.text("navegacao"), "cofre");
        conferir(&mut m.app, "cofre", nome);
    }
}
