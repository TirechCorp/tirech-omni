//! A página Notas (M47) nos dois temas: Dividir, Ler, busca e substituição, bloqueada, a prévia
//! da nota no Cofre e a pré-visualização com imagens. Snapshots e lint.

mod comum;

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tirech_omni::anexos;
use tirech_omni::cofre::{Cofre, Item, Tipo};
use tomni_ui::ClipboardImage;
use tomni_ui::prelude::*;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, item_ssh, montar_pura};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

/// O corpo da nota de exemplo: título, lista, tarefa, código, segredo mascarado, tabela e o
/// anexo (o link `anexo:<id>`).
fn corpo_de_exemplo(id_do_anexo: &str) -> String {
    format!(
        "# Servidor do cliente\n\n\
         Acesso à **VPN** e ao painel, com *cuidado*:\n\n\
         - abrir a VPN\n\
         - entrar com o usuário `ana`\n\n\
         - [x] pedir a senha nova\n\
         - [ ] avisar o time\n\n\
         ```sh\nssh ana@10.0.0.5\n```\n\n\
         ```segredo\ns3nh4-do-painel\n```\n\n\
         | Máquina | Função |\n|---|---|\n| web-01 | site |\n| db-01 | banco |\n\n\
         Chave: [chave.pem](anexo:{id_do_anexo})\n"
    )
}

/// Grava um anexo (`bytes`, com `nome` e `tipo`) na nota `nota` do cofre de `pasta`.
fn anexar(pasta: &Pasta, nota: u64, nome: &str, tipo: &str, bytes: &[u8]) -> String {
    // O id aparece no texto do editor: fixo, para a imagem ser sempre a mesma.
    let hash = nome.bytes().fold(7u128, |h, b| {
        h.wrapping_mul(131).wrapping_add(u128::from(b))
    });
    let id = format!("{hash:032x}");
    let (mut cofre, _) = Cofre::abrir(pasta.caminho());
    let mut ficha = anexos::gravar(
        &pasta.caminho().join(anexos::PASTA),
        &id,
        &mut Cursor::new(bytes.to_vec()),
        bytes.len() as u64,
        anexos::BLOCO,
        &|_| {},
        &AtomicBool::new(false),
    )
    .expect("gravar o anexo");
    ficha.nome = nome.to_owned();
    ficha.tipo = tipo.to_owned();
    cofre.anexar(nota, ficha).expect("anexar");
    id
}

/// Uma pasta fixa em `/tmp` (o diretório entra no estado da tela) e o relógio parado.
fn pasta_fixa(teste: &str) -> Pasta {
    tirech_omni::recentes::fixar_relogio(Some(1_760_000_000));
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-notas-{teste}"
    )));
    let _ = std::fs::remove_dir_all(&pasta.0);
    std::fs::create_dir_all(&pasta.0).unwrap();
    pasta
}

fn nota_vazia(titulo: &str) -> Item {
    Item {
        tipo: Tipo::Nota,
        titulo: titulo.into(),
        ..Item::default()
    }
}

/// Semeia a nota de exemplo, com um anexo, e devolve o id dela.
fn semear_exemplo(pasta: &Pasta) -> u64 {
    let ids = pasta.semear_cofre(vec![nota_vazia("Servidor do cliente")]);
    let anexo = anexar(
        pasta,
        ids[0],
        "chave.pem",
        "application/x-pem-file",
        b"-----BEGIN-----",
    );
    let (mut cofre, _) = Cofre::abrir(pasta.caminho());
    let mut item = cofre.item(ids[0]).unwrap().clone();
    item.notas = corpo_de_exemplo(&anexo);
    cofre.salvar(item).unwrap();
    ids[0]
}

/// A tela na página Notas, com a nota `id` aberta.
fn abrir(pasta: &Pasta, tema: Theme, id: u64) -> TestApp {
    let mut m = montar_pura(pasta, tema);
    m.app.shortcut(Shortcut::ctrl_shift('J'));
    m.app.click(&format!("notas-arvore-nota-{id}"));
    m.app
}

/// Passa quadros (as threads de decifrar imagens e de salvar dormem de verdade).
fn assentar(app: &mut TestApp) {
    for _ in 0..10 {
        std::thread::sleep(Duration::from_millis(20));
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn nota_em_dividir_e_em_ler() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("dividir-{nome}"));
        let id = semear_exemplo(&pasta);
        let mut app = abrir(&pasta, tema, id);
        app.click("notas-modo-dividir");
        assentar(&mut app);
        app.assert_lint();
        app.assert_snapshot(&format!("notas-dividir-{nome}"));
        app.click("notas-modo-ler");
        assentar(&mut app);
        app.assert_lint();
        app.assert_snapshot(&format!("notas-ler-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn nota_com_a_busca_e_a_substituicao_abertas() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("busca-{nome}"));
        let id = semear_exemplo(&pasta);
        let mut app = abrir(&pasta, tema, id);
        app.click("notas-modo-dividir");
        app.click("notas-editor");
        app.shortcut('h');
        app.type_text("VPN");
        assentar(&mut app);
        app.assert_lint();
        app.assert_snapshot(&format!("notas-busca-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn a_pagina_com_o_cofre_bloqueado() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("bloqueada-{nome}"));
        semear_exemplo(&pasta);
        pasta.cofre_com_senha_mestra("abc");
        let mut m = montar_pura(&pasta, tema);
        // Desbloqueia e bloqueia de novo, como uma pessoa faria, e volta para as Notas.
        m.app.type_into("cofre-desbloquear-campo", "abc");
        m.app.click("cofre-desbloquear-primary");
        m.app.click("navegacao-cofre").click("cofre-bloquear");
        m.app.click("navegacao-notas");
        assert!(m.app.visible("notas-bloqueado"));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("notas-bloqueada-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn o_cofre_com_uma_nota_selecionada() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("cofre-{nome}"));
        let id = semear_exemplo(&pasta);
        let (mut cofre, _) = Cofre::abrir(pasta.caminho());
        cofre.salvar(item_ssh("Servidor", "root", "s3nha")).unwrap();
        let mut m = montar_pura(&pasta, tema);
        // Larga o bastante para a lista mostrar a coluna do título ao lado do menu.
        m.app.resize(1600, 900);
        m.app.shortcut(Shortcut::ctrl_shift('K'));
        m.app.click(&format!("cofre-lista-item:{id}"));
        assentar(&mut m.app);
        assert!(m.app.visible("cofre-nota-previa"));
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("cofre-nota-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}

/// Um PNG de `largura` x `altura` com um degradê (para se ver a proporção).
fn png_degrade(largura: u32, altura: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((largura * altura * 4) as usize);
    for y in 0..altura {
        for x in 0..largura {
            rgba.extend([
                (x * 255 / largura) as u8,
                (y * 255 / altura) as u8,
                160,
                255,
            ]);
        }
    }
    ClipboardImage {
        width: largura,
        height: altura,
        rgba,
    }
    .to_png()
    .expect("o PNG codifica")
}

/// Se a tela desenha alguma `Image`.
fn ha_imagem(app: &TestApp) -> bool {
    fn anda(n: &tomni_ui::Node) -> bool {
        n.kind == "image" || n.children.iter().any(anda)
    }
    anda(&app.tree())
}

#[test]
fn a_previa_com_imagem_larga_e_com_imagem_corrompida() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("imagem-{nome}"));
        let ids = pasta.semear_cofre(vec![nota_vazia("Fotos")]);
        let foto = anexar(
            &pasta,
            ids[0],
            "foto.png",
            "image/png",
            &png_degrade(1400, 500),
        );
        let falsa = anexar(
            &pasta,
            ids[0],
            "quebrada.png",
            "image/png",
            b"isto nao e uma imagem",
        );
        let (mut cofre, _) = Cofre::abrir(pasta.caminho());
        let mut item = cofre.item(ids[0]).unwrap().clone();
        item.notas = format!(
            "# Fotos da visita\n\nO painel por dentro:\n\n![foto.png](anexo:{foto})\n\n\
             E uma que não abriu:\n\n![quebrada.png](anexo:{falsa})\n"
        );
        cofre.salvar(item).unwrap();
        let mut app = abrir(&pasta, tema, ids[0]);
        app.click("notas-modo-dividir");
        assentar(&mut app);
        assert!(
            ha_imagem(&app),
            "a imagem precisa estar pronta antes do quadro"
        );
        app.assert_lint();
        app.assert_snapshot(&format!("notas-imagem-{nome}"));
        let _ = std::fs::remove_dir_all(pasta.caminho());
    }
}
