//! A aparência de colar imagens e arquivos no terminal remoto (M45) nos dois temas: o diálogo
//! "Perguntar", o andamento do envio, as propriedades da conexão e a opção em Configurações.
//! Snapshots e lint.

mod comum;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use jiff::civil::DateTime;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui::{Key, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_com_ambiente};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

fn agora_fixo() -> DateTime {
    jiff::civil::date(2026, 10, 19).at(14, 30, 12, 0)
}

/// Uma conexão SSH com chave, na raiz.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    let mut c = Connection::new("web-01", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, c);
    tree
}

/// A tela com o relógio fixo. A pasta tem nome fixo: a página Configurações mostra o caminho
/// da pasta de dados em outro card e o snapshot precisa dele igual em toda execução.
fn tela(teste: &str, tema: Theme, config: &[(&str, &str, serde_json::Value)]) -> (Pasta, Montagem) {
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-colar-{teste}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    pasta.semear(&arvore());
    for (categoria, campo, valor) in config {
        pasta.configurar(categoria, campo, valor.clone());
    }
    let (m, _) = montar_com_ambiente(&pasta, tema, |a| a.agora = agora_fixo);
    (pasta, m)
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&mut TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Se o widget `id` existe e está visível.
fn aparece(app: &mut TestApp, id: &str) -> bool {
    app.ui().find(id).is_some() && app.visible(id)
}

/// Abre `web-01` e cola o arquivo `origem` do disco falso (Ctrl+Shift+V).
fn colar_arquivo(m: &mut Montagem, nome: &str, dados: &[u8]) {
    m.opener.disco.put(&format!("/home/ana/{nome}"), dados);
    let antes = m
        .app
        .panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.len())
        .sum::<usize>();
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a aba nova", |a| {
        a.panes("paineis")
            .panes
            .iter()
            .map(|p| p.tabs.len())
            .sum::<usize>()
            > antes
    });
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    assert!(Session::find(&chave).is_some());
    m.app
        .clipboard()
        .write_files(vec![PathBuf::from(format!("/home/ana/{nome}"))]);
    m.app.click(&format!("paineis-terminal-{chave}"));
    m.app
        .key_with(Key::Char('v'), Modifiers::CTRL.with(Modifiers::SHIFT));
}

fn conferir(m: &mut Montagem, nome: &str, tema: &str) {
    // Deixa as animações (a do diálogo, a das dicas) terminarem.
    m.app.wait(Duration::from_millis(400));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("{nome}-{tema}"));
}

#[test]
fn o_dialogo_de_perguntar() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela(
            "dialogo",
            tema,
            &[("terminal", "colar_arquivos", serde_json::json!("perguntar"))],
        );
        colar_arquivo(&mut m, "relatório.pdf", b"0123456789");
        esperar(&mut m.app, "o diálogo", |a| {
            aparece(a, "colar-arquivos-dialogo")
        });
        conferir(&mut m, "colar-dialogo", nome);
    }
}

#[test]
fn o_andamento_do_envio() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("progresso", tema, &[]);
        // O servidor demora uma hora em cada bloco: o envio fica parado no começo.
        let servidor = m.opener.servidor(1);
        servidor.set_chunk(10);
        servidor.set_delay(Duration::from_secs(3600));
        colar_arquivo(&mut m, "captura.png", &vec![7u8; 4000]);
        esperar(&mut m.app, "o andamento", |a| aparece(a, "colar-cancelar"));
        conferir(&mut m, "colar-progresso", nome);
    }
}

#[test]
fn as_propriedades_da_conexao() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela("propriedades", tema, &[]);
        m.app.click("arvore-1");
        // As linhas de colar ficam no fim da categoria "Sessão": escolher uma rola a grade até ela.
        comum::escolher_propriedade(&mut m.app, "Ao colar");
        conferir(&mut m, "colar-propriedades", nome);
    }
}

#[test]
fn a_opcao_nas_configuracoes_do_terminal() {
    for (nome, tema) in TEMAS {
        let (_pasta, mut m) = tela(
            "configuracoes",
            tema,
            &[("geral", "pagina_inicial", serde_json::json!("inicio"))],
        );
        m.app.resize(1000, 760);
        m.app.click("inicio-ferramenta-configuracoes");
        m.app.click("config-secao-terminal");
        let card = tirech_omni::configuracoes::catalogo::id_do_card("terminal.colar_arquivos");
        // O card está fora da tela: rola até ele aparecer.
        for _ in 0..60 {
            if m.app.ui().find(&card).is_some() && m.app.rect(&card).y0 < 300.0 {
                break;
            }
            m.app.scroll("config-rolagem", 200.0);
        }
        assert!(aparece(&mut m.app, &card), "o card {card} não apareceu");
        conferir(&mut m, "configuracoes-terminal-colar", nome);
    }
}
