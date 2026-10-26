//! A página do cofre de senhas: sozinha e dentro da tela (Ctrl+Shift+K).

mod comum;

use std::cell::RefCell;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tirech_omni::anexos;
use tirech_omni::cofre::{Cofre, CofreCompartilhado, Item, Tipo};
use tirech_omni::tela_cofre::{pagina_cofre, pagina_cofre_com};
use tomni_ui::Key;
use tomni_ui::kurbo::Point;
use tomni_ui::prelude::*;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, item_do_tipo, item_ssh, montar};

fn item(titulo: &str, usuario: &str, senha: &str) -> Item {
    Item {
        tipo: Tipo::Senha,
        titulo: titulo.into(),
        usuario: usuario.into(),
        senha: senha.into(),
        ..Item::default()
    }
}

fn pagina_sozinha(pasta: &Pasta) -> TestApp {
    let (cofre, _) = Cofre::abrir(pasta.caminho());
    let cofre = CofreCompartilhado::new(cofre);
    TestApp::new(move || {
        Form::new("Cofre")
            .density(Density::Compact)
            .child(pagina_cofre(cofre.clone()))
    })
    .size(1100, 640)
    .theme(Theme::Light)
}

const GRADE: &str = "cofre-propriedades";

/// Escolhe a linha como uma pessoa: clica no nome da primeira e desce com ↓ até a pedida.
fn escolher(app: &mut TestApp, nome: &str) {
    if app.focused().is_some_and(|f| f.starts_with(GRADE)) {
        app.key(Key::Home);
    } else {
        let r = app.rect(&format!("{GRADE}-Tipo"));
        app.click_at(Point::new(r.x0 + 24.0, (r.y0 + r.y1) / 2.0), 1);
    }
    for _ in 0..20 {
        let escolhida = app
            .properties(GRADE)
            .into_iter()
            .find(|p| p.selected)
            .map(|p| p.name);
        if escolhida.as_deref() == Some(nome) {
            return;
        }
        app.key(Key::Down);
    }
    panic!("a propriedade {nome} não foi escolhida");
}

/// Troca o valor de uma propriedade de texto: Enter abre o editor, Enter confirma.
fn editar(app: &mut TestApp, nome: &str, texto: &str) {
    escolher(app, nome);
    app.key(Key::Enter);
    let atual = app.text(&format!("{GRADE}-{nome}-valor"));
    app.key(Key::End);
    for _ in 0..atual.chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text(texto).key(Key::Enter);
}

/// As chaves das linhas da lista, na ordem da tela.
fn chaves(app: &TestApp) -> Vec<String> {
    app.grid("cofre-lista")
        .rows
        .into_iter()
        .map(|l| l.key)
        .collect()
}

/// O título (primeira coluna) das linhas da lista.
fn rotulos(app: &TestApp) -> Vec<String> {
    app.grid("cofre-lista")
        .rows
        .into_iter()
        .map(|l| l.cells[0].clone())
        .collect()
}

/// A célula `coluna` da linha `chave` da lista.
fn celula(app: &TestApp, chave: &str, coluna: usize) -> String {
    app.grid("cofre-lista")
        .rows
        .into_iter()
        .find(|l| l.key == chave)
        .unwrap_or_else(|| panic!("{chave} não está na lista"))
        .cells[coluna]
        .clone()
}

/// As linhas da árvore das pastas: o nível e o rótulo.
fn pastas(app: &TestApp) -> Vec<(usize, String)> {
    app.tree_items("cofre-pastas")
        .into_iter()
        .map(|l| (l.depth, l.label))
        .collect()
}

fn pasta_do(p: &Pasta, caminho: &str) -> String {
    format!("pasta:{}", p.id_da_pasta(caminho))
}

/// Os ids de um cofre semeado, na ordem em que foram gravados.
struct Semeado {
    pasta: Pasta,
    painel: u64,
    web: u64,
    cartao: u64,
    wifi: u64,
    roteador: u64,
}

impl Semeado {
    fn chave_painel(&self) -> String {
        format!("item:{}", self.painel)
    }
}

/// Clientes › ACME (Painel da ACME, web-01), Pessoal (Banco, Cartão) e, na raiz, Wi-Fi e
/// Roteador.
fn semeado(nome: &str) -> Semeado {
    let pasta = Pasta::nova(nome);
    let ids = pasta.semear_cofre_em(vec![
        ("Clientes/ACME", item("Painel da ACME", "ana", "s3nh4")),
        ("Clientes/ACME", item("web-01", "root", "outra")),
        ("Pessoal", item("Banco", "", "1234")),
        ("Pessoal", item("Cartão", "bia", "9999")),
        ("", item("Wi-Fi", "casa", "wifi-pw")),
        ("", item("Roteador", "admin", "admin")),
    ]);
    Semeado {
        pasta,
        painel: ids[0],
        web: ids[1],
        cartao: ids[3],
        wifi: ids[4],
        roteador: ids[5],
    }
}

/// Abre a pasta pelo clique duplo na lista, descendo a partir da raiz.
fn abrir(app: &mut TestApp, p: &Pasta, caminho: &str) {
    let mut acumulado = String::new();
    for parte in caminho.split('/') {
        if !acumulado.is_empty() {
            acumulado.push('/');
        }
        acumulado.push_str(parte);
        app.double_click(&format!("cofre-lista-{}", pasta_do(p, &acumulado)));
    }
}

#[test]
fn tres_paineis_e_a_raiz() {
    let s = semeado("paineis");
    let app = pagina_sozinha(&s.pasta);
    assert_eq!(
        pastas(&app),
        [
            (0, "Cofre".to_owned()),
            (1, "Clientes".to_owned()),
            (1, "Pessoal".to_owned())
        ]
    );
    // As pastas em cima, os itens da raiz embaixo.
    assert_eq!(
        chaves(&app),
        [
            pasta_do(&s.pasta, "Clientes"),
            pasta_do(&s.pasta, "Pessoal"),
            format!("item:{}", s.roteador),
            format!("item:{}", s.wifi)
        ]
    );
    assert_eq!(app.text("cofre-caminho"), "Cofre");
    assert!(app.visible("cofre-sem-item"));
    assert!(!app.enabled("cofre-duplicar"));
    assert!(!app.enabled("cofre-apagar"));
    assert!(!app.enabled("cofre-mover"));
}

#[test]
fn escolher_pasta_na_arvore_mostra_o_conteudo() {
    let s = semeado("arvore");
    let mut app = pagina_sozinha(&s.pasta);
    app.click(&format!("cofre-pastas-{}", pasta_do(&s.pasta, "Pessoal")));
    assert_eq!(rotulos(&app), ["Banco", "Cartão"]);
    assert_eq!(app.text("cofre-caminho"), "Cofre › Pessoal");
    app.click("cofre-pastas-raiz");
    assert_eq!(app.text("cofre-caminho"), "Cofre");
    assert_eq!(rotulos(&app).len(), 4);
}

#[test]
fn clique_duplo_em_pasta_abre_e_em_item_copia_a_senha() {
    let s = semeado("duplo");
    let mut app = pagina_sozinha(&s.pasta);
    app.double_click(&format!("cofre-lista-{}", pasta_do(&s.pasta, "Pessoal")));
    assert_eq!(app.text("cofre-caminho"), "Cofre › Pessoal");
    assert_eq!(rotulos(&app), ["Banco", "Cartão"]);
    app.double_click(&format!("cofre-lista-item:{}", s.cartao));
    app.frame();
    assert_eq!(app.clipboard().read().as_deref(), Some("9999"));
}

#[test]
fn nova_pasta_entra_em_renomear() {
    let s = semeado("nova-pasta");
    let mut app = pagina_sozinha(&s.pasta);
    app.click("cofre-nova-pasta");
    app.key(Key::End);
    for _ in 0.."Nova pasta".chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text("Trabalho").key(Key::Enter);
    assert!(
        s.pasta
            .cofre()
            .pastas()
            .iter()
            .any(|p| p.nome == "Trabalho")
    );
    assert!(pastas(&app).contains(&(1, "Trabalho".to_owned())));

    // Sem digitar, a pasta fica "Nova pasta"; a segunda, "Nova pasta (2)".
    app.click("cofre-nova-pasta").key(Key::Enter);
    app.click("cofre-nova-pasta").key(Key::Enter);
    let nomes: Vec<String> = s
        .pasta
        .cofre()
        .pastas()
        .iter()
        .map(|p| p.nome.clone())
        .collect();
    assert!(nomes.contains(&"Nova pasta".to_owned()), "{nomes:?}");
    assert!(nomes.contains(&"Nova pasta (2)".to_owned()), "{nomes:?}");
}

#[test]
fn renomear_pasta_pelo_f2_e_pelo_menu() {
    let s = semeado("renomear-pasta");
    let mut app = pagina_sozinha(&s.pasta);
    let pessoal = pasta_do(&s.pasta, "Pessoal");
    // Na lista: F2.
    app.click(&format!("cofre-lista-{pessoal}"));
    app.key(Key::F(2));
    app.key(Key::End);
    for _ in 0.."Pessoal".chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text("Casa").key(Key::Enter);
    assert!(pastas(&app).contains(&(1, "Casa".to_owned())));
    // Na árvore: pelo menu.
    app.right_click(&format!("cofre-pastas-{pessoal}"));
    app.click(&format!("menu-pastas-{pessoal}-renomear"));
    app.key(Key::End);
    for _ in 0.."Casa".chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text("Família").key(Key::Enter);
    assert!(pastas(&app).contains(&(1, "Família".to_owned())));
    // Um nome com barra é recusado, com o aviso.
    app.right_click(&format!("cofre-pastas-{pessoal}"));
    app.click(&format!("menu-pastas-{pessoal}-renomear"));
    app.key(Key::End);
    for _ in 0.."Família".chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text("A/B").key(Key::Enter);
    assert!(app.visible("cofre-erro"));
    assert!(pastas(&app).contains(&(1, "Família".to_owned())));
}

#[test]
fn arrastar_itens_para_pasta_da_lista() {
    let s = semeado("arrastar-lista");
    let mut app = pagina_sozinha(&s.pasta);
    let pessoal = pasta_do(&s.pasta, "Pessoal");
    let (wifi, roteador) = (format!("item:{}", s.wifi), format!("item:{}", s.roteador));
    app.drag_rows(
        "cofre-lista",
        &[&wifi, &roteador],
        "cofre-lista",
        Some(&pessoal),
    );
    let cofre = s.pasta.cofre();
    let id = s.pasta.id_da_pasta("Pessoal");
    assert_eq!(cofre.item(s.wifi).unwrap().pasta, Some(id));
    assert_eq!(cofre.item(s.roteador).unwrap().pasta, Some(id));
    assert_eq!(rotulos(&app), ["Clientes", "Pessoal"]);
}

#[test]
fn arrastar_itens_para_pasta_da_arvore() {
    let s = semeado("arrastar-arvore");
    let mut app = pagina_sozinha(&s.pasta);
    let pessoal = pasta_do(&s.pasta, "Pessoal");
    let wifi = format!("item:{}", s.wifi);
    app.drag_rows("cofre-lista", &[&wifi], "cofre-pastas", Some(&pessoal));
    assert_eq!(
        s.pasta.cofre().item(s.wifi).unwrap().pasta,
        Some(s.pasta.id_da_pasta("Pessoal"))
    );
}

#[test]
fn arrastar_pasta_na_arvore() {
    let s = semeado("arrastar-pasta");
    let mut app = pagina_sozinha(&s.pasta);
    let pessoal = pasta_do(&s.pasta, "Pessoal");
    let clientes = pasta_do(&s.pasta, "Clientes");
    app.drag(
        format!("cofre-pastas-{pessoal}"),
        format!("cofre-pastas-{clientes}"),
    );
    assert_eq!(
        s.pasta
            .cofre()
            .pasta(s.pasta.id_da_pasta("Clientes/Pessoal"))
            .map(|p| p.nome.clone()),
        Some("Pessoal".to_owned())
    );
}

#[test]
fn arrastar_pasta_para_dentro_de_uma_filha_nao_muda_nada() {
    let s = semeado("arrastar-ciclo");
    let mut app = pagina_sozinha(&s.pasta);
    let clientes = pasta_do(&s.pasta, "Clientes");
    app.double_click(&format!("cofre-lista-{clientes}"));
    let acme = pasta_do(&s.pasta, "Clientes/ACME");
    app.drag(
        format!("cofre-pastas-{clientes}"),
        format!("cofre-pastas-{acme}"),
    );
    let cofre = s.pasta.cofre();
    let id = s.pasta.id_da_pasta("Clientes");
    assert_eq!(cofre.pasta(id).unwrap().pai, None);
}

#[test]
fn mover_para_pelo_dialogo() {
    let s = semeado("mover-dialogo");
    let mut app = pagina_sozinha(&s.pasta);
    let wifi = format!("item:{}", s.wifi);
    app.right_click(&format!("cofre-lista-{wifi}"));
    app.click(&format!("menu-{wifi}-mover"));
    assert!(app.is_open("cofre-mover-dialogo"));
    app.click(&format!(
        "cofre-mover-arvore-{}",
        pasta_do(&s.pasta, "Pessoal")
    ));
    app.click("cofre-mover-dialogo-primary");
    assert!(!app.is_open("cofre-mover-dialogo"));
    assert_eq!(
        s.pasta.cofre().item(s.wifi).unwrap().pasta,
        Some(s.pasta.id_da_pasta("Pessoal"))
    );
}

#[test]
fn apagar_pasta_pergunta_e_conta() {
    let s = semeado("apagar-pasta");
    let mut app = pagina_sozinha(&s.pasta);
    let clientes = pasta_do(&s.pasta, "Clientes");
    app.click(&format!("cofre-lista-{clientes}"));
    app.key(Key::Delete);
    assert!(app.is_open("cofre-apagar-dialogo"));
    assert_eq!(
        app.text("cofre-apagar-texto"),
        "A pasta, 1 subpasta e 2 itens saem do cofre."
    );
    app.click("cofre-apagar-dialogo-close");
    assert!(s.pasta.cofre().item(s.painel).is_some());
    app.key(Key::Delete);
    app.click("cofre-apagar-dialogo-primary");
    let cofre = s.pasta.cofre();
    assert!(cofre.item(s.painel).is_none());
    assert!(cofre.item(s.web).is_none());
    assert_eq!(cofre.pastas().len(), 1, "só a Pessoal sobrou");
    assert_eq!(cofre.itens().len(), 4);
    assert!(!chaves(&app).contains(&clientes));
}

#[test]
fn apagar_varios_itens() {
    let s = semeado("apagar-varios");
    let mut app = pagina_sozinha(&s.pasta);
    let (wifi, roteador) = (format!("item:{}", s.wifi), format!("item:{}", s.roteador));
    app.grid_select("cofre-lista", &[&wifi, &roteador]);
    app.click("cofre-apagar");
    assert!(app.is_open("cofre-apagar-dialogo"));
    assert_eq!(app.text("cofre-apagar-texto"), "2 itens saem do cofre.");
    app.click("cofre-apagar-dialogo-primary");
    let cofre = s.pasta.cofre();
    assert!(cofre.item(s.wifi).is_none() && cofre.item(s.roteador).is_none());
    assert_eq!(cofre.itens().len(), 4);
}

/// Troca o texto do filtro da página.
fn filtrar(app: &mut TestApp, texto: &str) {
    app.click("cofre-filtro").key(Key::End);
    for _ in 0..app.text("cofre-filtro").chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text(texto);
}

#[test]
fn filtro_desce_nas_subpastas() {
    let s = semeado("filtro");
    let mut app = pagina_sozinha(&s.pasta);
    filtrar(&mut app, "painel");
    assert_eq!(chaves(&app), [s.chave_painel()]);
    assert_eq!(celula(&app, &s.chave_painel(), 3), "Clientes/ACME");
    // Sem acento acha com acento, e o filtro também casa pelo usuário.
    filtrar(&mut app, "cartao");
    assert_eq!(rotulos(&app), ["Cartão"]);
    filtrar(&mut app, "bia");
    assert_eq!(rotulos(&app), ["Cartão"]);
    filtrar(&mut app, "");
    assert_eq!(rotulos(&app).len(), 4);
}

#[test]
fn escolher_um_item_mostra_as_propriedades() {
    let s = semeado("props");
    let mut app = pagina_sozinha(&s.pasta);
    abrir(&mut app, &s.pasta, "Clientes/ACME");
    app.click(&format!("cofre-lista-{}", s.chave_painel()));
    let props = app.properties(GRADE);
    let valor = |nome: &str| props.iter().find(|p| p.name == nome).unwrap().value.clone();
    assert_eq!(valor("Tipo"), "Senha");
    assert_eq!(valor("Título"), "Painel da ACME");
    assert!(
        props.iter().all(|p| p.name != "Pasta"),
        "a pasta não é propriedade"
    );
    assert_eq!(valor("Usuário"), "ana");
    assert_ne!(valor("Senha"), "s3nh4", "a senha vem mascarada");
    assert_eq!(valor("Senha").chars().count(), 5);
    assert!(!app.visible("cofre-sem-item"));
    assert!(app.enabled("cofre-duplicar"));
}

#[test]
fn editar_uma_propriedade_grava_na_hora() {
    let s = semeado("editar");
    let mut app = pagina_sozinha(&s.pasta);
    abrir(&mut app, &s.pasta, "Clientes/ACME");
    app.click(&format!("cofre-lista-{}", s.chave_painel()));
    editar(&mut app, "Usuário", "carla");
    editar(&mut app, "Senha", "nova-senha");
    let cofre = s.pasta.cofre();
    let i = cofre.item(s.painel).unwrap();
    assert_eq!(i.usuario, "carla");
    assert_eq!(i.senha, "nova-senha");
    assert_eq!(i.titulo, "Painel da ACME");
    assert_eq!(celula(&app, &s.chave_painel(), 1), "carla");
}

#[test]
fn trocar_o_tipo_grava() {
    let s = semeado("tipo");
    let mut app = pagina_sozinha(&s.pasta);
    app.click(&format!("cofre-lista-item:{}", s.wifi));
    escolher(&mut app, "Tipo");
    app.key(Key::F(4));
    app.click(&format!("{GRADE}-Tipo-valor-ssh"));
    assert_eq!(s.pasta.cofre().item(s.wifi).unwrap().tipo, Tipo::Ssh);
}

#[test]
fn copiar_usuario_e_senha() {
    let s = semeado("copiar");
    let mut app = pagina_sozinha(&s.pasta);
    app.click(&format!("cofre-lista-item:{}", s.wifi));
    app.click("cofre-copiar-usuario");
    app.frame();
    assert_eq!(app.clipboard().read().as_deref(), Some("casa"));
    assert!(app.visible("cofre-aviso"));
    app.click("cofre-copiar-senha");
    app.frame();
    assert_eq!(app.clipboard().read().as_deref(), Some("wifi-pw"));
    assert!(app.text("cofre-aviso").contains("Senha copiada."));
}

#[test]
fn o_menu_de_contexto_copia_apaga_e_duplica() {
    let s = semeado("contexto");
    let mut app = pagina_sozinha(&s.pasta);
    let wifi = format!("item:{}", s.wifi);
    app.right_click(&format!("cofre-lista-{wifi}"));
    app.click(&format!("menu-{wifi}-usuario"));
    app.frame();
    assert_eq!(app.clipboard().read().as_deref(), Some("casa"));
    app.right_click(&format!("cofre-lista-{wifi}"));
    app.click(&format!("menu-{wifi}-senha"));
    app.frame();
    assert_eq!(app.clipboard().read().as_deref(), Some("wifi-pw"));
    app.right_click(&format!("cofre-lista-{wifi}"));
    app.click(&format!("menu-{wifi}-duplicar"));
    assert_eq!(s.pasta.cofre().itens().len(), 7);
    app.right_click(&format!("cofre-lista-{wifi}"));
    app.click(&format!("menu-{wifi}-apagar"));
    assert!(app.is_open("cofre-apagar-dialogo"));
    app.click("cofre-apagar-dialogo-primary");
    assert!(s.pasta.cofre().item(s.wifi).is_none());
}

#[test]
fn novo_cria_na_pasta_aberta_e_deixa_renomear() {
    let s = semeado("novo");
    let mut app = pagina_sozinha(&s.pasta);
    abrir(&mut app, &s.pasta, "Pessoal");
    app.click("cofre-novo");
    let cofre = s.pasta.cofre();
    assert_eq!(cofre.itens().len(), 7);
    let novo = cofre.itens().last().unwrap();
    assert_eq!(novo.titulo, "Novo item");
    assert_eq!(novo.pasta, Some(s.pasta.id_da_pasta("Pessoal")));
    assert_eq!(novo.tipo, Tipo::Senha);
    // O campo de renomear está aberto: digitar troca o título.
    app.key(Key::End);
    for _ in 0.."Novo item".chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text("Wi-Fi 2").key(Key::Enter);
    assert_eq!(s.pasta.cofre().itens().last().unwrap().titulo, "Wi-Fi 2");
}

#[test]
fn duplicar_poe_copia_no_titulo() {
    let s = semeado("duplicar");
    let mut app = pagina_sozinha(&s.pasta);
    abrir(&mut app, &s.pasta, "Clientes/ACME");
    app.click(&format!("cofre-lista-{}", s.chave_painel()));
    app.click("cofre-duplicar");
    let cofre = s.pasta.cofre();
    let copia = cofre.itens().last().unwrap();
    assert_eq!(copia.titulo, "Painel da ACME (cópia)");
    assert_eq!(copia.senha, "s3nh4");
    assert_eq!(copia.pasta, Some(s.pasta.id_da_pasta("Clientes/ACME")));
    assert_ne!(copia.id, s.painel);
}

#[test]
fn apagar_item_pede_confirmacao() {
    let s = semeado("apagar");
    let mut app = pagina_sozinha(&s.pasta);
    app.click(&format!("cofre-lista-item:{}", s.wifi));
    app.click("cofre-apagar");
    assert!(app.is_open("cofre-apagar-dialogo"));
    assert_eq!(app.text("cofre-apagar-texto"), "O item sai do cofre.");
    app.click("cofre-apagar-dialogo-close");
    assert!(s.pasta.cofre().item(s.wifi).is_some());
    app.click("cofre-apagar");
    app.click("cofre-apagar-dialogo-primary");
    assert!(s.pasta.cofre().item(s.wifi).is_none());
    assert!(!chaves(&app).contains(&format!("item:{}", s.wifi)));
    assert!(app.visible("cofre-sem-item"));
}

#[test]
fn o_gerador_poe_a_senha_no_item_escolhido() {
    let s = semeado("gerador");
    let mut app = pagina_sozinha(&s.pasta);
    app.click(&format!("cofre-lista-item:{}", s.wifi));
    app.click("cofre-gerar");
    assert!(app.is_open("cofre-gerador"));
    let primeira = app.text("cofre-gerador-senha");
    assert_eq!(primeira.chars().count(), 20);
    app.click("cofre-gerador-novo");
    assert_ne!(app.text("cofre-gerador-senha"), primeira);
    let escolhida = app.text("cofre-gerador-senha");
    app.click("cofre-gerador-primary");
    assert_eq!(s.pasta.cofre().item(s.wifi).unwrap().senha, escolhida);
}

#[test]
fn o_gerador_sem_item_escolhido_cria_um_na_pasta_aberta() {
    let s = semeado("gerador-novo");
    let mut app = pagina_sozinha(&s.pasta);
    abrir(&mut app, &s.pasta, "Pessoal");
    app.click("cofre-gerar");
    app.click("cofre-gerador-copiar");
    app.frame();
    let copiada = app.clipboard().read().unwrap();
    app.click("cofre-gerador-primary");
    let cofre = s.pasta.cofre();
    assert_eq!(cofre.itens().len(), 7);
    let novo = cofre.itens().last().unwrap();
    assert_eq!(novo.senha, copiada);
    assert_eq!(novo.pasta, Some(s.pasta.id_da_pasta("Pessoal")));
}

#[test]
fn o_gerador_respeita_as_opcoes() {
    let s = semeado("gerador-opcoes");
    let mut app = pagina_sozinha(&s.pasta);
    app.click("cofre-gerar");
    for id in [
        "cofre-gerador-maiusculas",
        "cofre-gerador-minusculas",
        "cofre-gerador-simbolos",
    ] {
        app.click(id);
    }
    let senha = app.text("cofre-gerador-senha");
    assert!(senha.chars().all(|c| c.is_ascii_digit()), "{senha}");
}

#[test]
fn novo_campo_aparece_na_grade_e_grava() {
    let s = semeado("campo");
    let mut app = pagina_sozinha(&s.pasta);
    app.click(&format!("cofre-lista-item:{}", s.wifi));
    app.click("cofre-campo-novo");
    app.type_into("cofre-campo-nome", "PIN");
    app.click("cofre-campo-oculto");
    app.click("cofre-campo-primary");
    let cofre = s.pasta.cofre();
    let campo = &cofre.item(s.wifi).unwrap().campos[0];
    assert_eq!((campo.nome.as_str(), campo.oculto), ("PIN", true));
    assert!(app.properties(GRADE).iter().any(|p| p.name == "PIN"));
    editar(&mut app, "PIN", "4321");
    assert_eq!(
        s.pasta.cofre().item(s.wifi).unwrap().campos[0].valor,
        "4321"
    );
}

/// Grava `texto` em `nome`, dentro da pasta do teste, e devolve o caminho.
fn escrever(p: &Pasta, nome: &str, texto: &str) -> std::path::PathBuf {
    let caminho = p.caminho().join(nome);
    std::fs::create_dir_all(p.caminho()).unwrap();
    std::fs::write(&caminho, texto).unwrap();
    caminho
}

#[test]
fn importar_json_do_bitwarden_na_pasta_aberta() {
    let s = semeado("importar-bw");
    let json = escrever(
        &s.pasta,
        "bw.json",
        r#"{"encrypted":false,
            "folders":[{"id":"f1","name":"Banco"}],
            "items":[
              {"type":1,"name":"Conta","folderId":"f1",
               "login":{"username":"ana","password":"pw","uris":[{"uri":"https://banco"}]}},
              {"type":2,"name":"Nota solta","notes":"texto"}]}"#,
    );
    let mut app = pagina_sozinha(&s.pasta);
    abrir(&mut app, &s.pasta, "Pessoal");
    app.file_dialog_answer(Some(json));
    app.click("cofre-importar");
    let aviso = app.text("cofre-aviso");
    assert!(aviso.contains("Importados 2 itens e 1 pasta"), "{aviso}");
    assert!(aviso.contains("JSON do Bitwarden"), "{aviso}");
    assert!(aviso.contains("Pessoal"), "{aviso}");
    let cofre = s.pasta.cofre();
    let conta = cofre.itens().iter().find(|i| i.titulo == "Conta").unwrap();
    assert_eq!(cofre.caminho(conta.pasta), "Pessoal/Banco");
    let nota = cofre
        .itens()
        .iter()
        .find(|i| i.titulo == "Nota solta")
        .unwrap();
    assert_eq!(cofre.caminho(nota.pasta), "Pessoal");
    assert!(
        rotulos(&app).contains(&"Banco".to_owned()),
        "a pasta nova aparece na lista"
    );
}

#[test]
fn importar_json_cifrado_do_bitwarden_avisa() {
    let s = semeado("importar-cifrado");
    let json = escrever(&s.pasta, "cifrado.json", r#"{"encrypted":true,"items":[]}"#);
    let mut app = pagina_sozinha(&s.pasta);
    app.file_dialog_answer(Some(json));
    app.click("cofre-importar");
    assert!(app.visible("cofre-erro"));
    assert!(app.text("cofre-erro").contains("cifrado"));
    assert_eq!(s.pasta.cofre().itens().len(), 6);
}

#[test]
fn importar_csv_do_keepassxc_cria_as_pastas() {
    let s = semeado("importar-kp");
    let csv = escrever(
        &s.pasta,
        "kp.csv",
        "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\"\n\
         \"Root/Banco/Contas\",\"Conta\",\"ana\",\"pw\",\"https://banco\",\"\"\n",
    );
    let mut app = pagina_sozinha(&s.pasta);
    app.file_dialog_answer(Some(csv));
    app.click("cofre-importar");
    assert!(app.text("cofre-aviso").contains("CSV do KeePassXC"));
    let cofre = s.pasta.cofre();
    let conta = cofre.itens().iter().find(|i| i.titulo == "Conta").unwrap();
    assert_eq!(cofre.caminho(conta.pasta), "Banco/Contas");
    assert!(pastas(&app).contains(&(1, "Banco".to_owned())));
}

#[test]
fn importar_um_arquivo_que_nao_serve_avisa_o_erro() {
    let s = semeado("importar-ruim");
    let csv = escrever(&s.pasta, "ruim.csv", "a,b\n1,2\n");
    let mut app = pagina_sozinha(&s.pasta);
    app.file_dialog_answer(Some(csv));
    app.click("cofre-importar");
    assert!(app.visible("cofre-erro"));
    assert_eq!(s.pasta.cofre().itens().len(), 6);
}

#[test]
fn exportar_json_do_tirech_e_reimportar() {
    let s = semeado("exportar-tirech");
    let mut app = pagina_sozinha(&s.pasta);
    let arquivo = s.pasta.caminho().join("saida.json");
    app.file_dialog_answer(Some(arquivo.clone()));
    app.click("cofre-exportar");
    app.click("cofre-exportar-tirech");
    let texto = std::fs::read_to_string(&arquivo).unwrap();
    assert!(texto.contains("s3nh4"), "a exportação leva as senhas");
    assert!(
        app.text("cofre-aviso")
            .contains("Exportados 6 itens e 3 pastas.")
    );

    // Num cofre de outra pasta temporária, o arquivo reconstrói a árvore.
    let outra = Pasta::nova("exportar-tirech-destino");
    let mut app = pagina_sozinha(&outra);
    app.file_dialog_answer(Some(arquivo));
    app.click("cofre-importar");
    assert!(app.text("cofre-aviso").contains("JSON do Tirech Omni"));
    let cofre = outra.cofre();
    assert_eq!(cofre.itens().len(), 6);
    let painel = cofre
        .itens()
        .iter()
        .find(|i| i.titulo == "Painel da ACME")
        .unwrap();
    assert_eq!(painel.senha, "s3nh4");
    assert_eq!(cofre.caminho(painel.pasta), "Clientes/ACME");
}

#[test]
fn exportar_json_do_bitwarden_leva_as_senhas() {
    let s = semeado("exportar-bw");
    let mut app = pagina_sozinha(&s.pasta);
    let arquivo = s.pasta.caminho().join("bw-saida.json");
    app.file_dialog_answer(Some(arquivo.clone()));
    app.click("cofre-exportar");
    app.click("cofre-exportar-bitwarden");
    // Nenhum diálogo no meio: o arquivo já está lá.
    let texto = std::fs::read_to_string(&arquivo).unwrap();
    assert!(texto.contains("\"encrypted\": false") || texto.contains("\"encrypted\":false"));
    assert!(texto.contains("s3nh4"));
    assert!(texto.contains("Painel da ACME"));
    assert!(texto.contains("Clientes/ACME"));
}

/// Anexa `bytes` (com o nome `nome`) ao item `item` do cofre de `pasta` e devolve o id do anexo.
fn anexar(pasta: &Pasta, item: u64, nome: &str, bytes: &[u8]) -> String {
    let (mut cofre, _) = Cofre::abrir(pasta.caminho());
    let mut ficha = anexos::gravar(
        &pasta.caminho().join(anexos::PASTA),
        &anexos::novo_id(),
        &mut Cursor::new(bytes.to_vec()),
        bytes.len() as u64,
        anexos::BLOCO,
        &|_| {},
        &AtomicBool::new(false),
    )
    .expect("gravar o anexo");
    ficha.nome = nome.to_owned();
    cofre.anexar(item, ficha.clone()).expect("anexar");
    ficha.id
}

/// Passa quadros até `pronto` valer (a gravação e a cifra dos anexos rodam em outra thread).
fn esperar_ate(app: &mut TestApp, o_que: &str, mut pronto: impl FnMut(&mut TestApp) -> bool) {
    for _ in 0..400 {
        if pronto(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
        app.wait(Duration::from_millis(20));
    }
    panic!("não chegou a: {o_que}");
}

#[test]
fn exportar_com_anexos_grava_os_arquivos_ao_lado_e_importar_cifra_de_novo() {
    let s = semeado("exportar-anexos");
    let id = anexar(&s.pasta, s.painel, "chave.pem", b"-----BEGIN-----abacaxi");
    anexar(&s.pasta, s.web, "foto.png", b"PNG-bytes");
    let mut app = pagina_sozinha(&s.pasta);
    let arquivo = s.pasta.caminho().join("saida.json");
    app.file_dialog_answer(Some(arquivo.clone()));
    app.click("cofre-exportar");
    app.click("cofre-exportar-tirech");
    esperar_ate(&mut app, "o JSON gravado", |a| {
        arquivo.exists() && a.visible("cofre-aviso")
    });
    let aviso = app.text("cofre-aviso");
    assert!(aviso.contains("Exportados 6 itens e 3 pastas."), "{aviso}");
    assert!(aviso.contains("2 anexos em saida-attachments"), "{aviso}");
    let texto = std::fs::read_to_string(&arquivo).unwrap();
    assert!(
        !texto.contains("abacaxi"),
        "os bytes ficam nos arquivos, não no JSON"
    );
    let ao_lado = s
        .pasta
        .caminho()
        .join("saida-attachments")
        .join(&id)
        .join("chave.pem");
    assert_eq!(std::fs::read(ao_lado).unwrap(), b"-----BEGIN-----abacaxi");

    // Num cofre de outra pasta: os itens vêm com os anexos, cifrados de novo.
    let outra = Pasta::nova("exportar-anexos-destino");
    std::fs::rename(
        s.pasta.caminho().join("saida-attachments"),
        outra.caminho().join("saida-attachments"),
    )
    .or_else(|_| {
        std::fs::create_dir_all(outra.caminho())?;
        std::fs::rename(
            s.pasta.caminho().join("saida-attachments"),
            outra.caminho().join("saida-attachments"),
        )
    })
    .unwrap();
    std::fs::copy(&arquivo, outra.caminho().join("saida.json")).unwrap();
    let mut app = pagina_sozinha(&outra);
    app.file_dialog_answer(Some(outra.caminho().join("saida.json")));
    app.click("cofre-importar");
    esperar_ate(&mut app, "a importação", |a| a.visible("cofre-aviso"));
    let aviso = app.text("cofre-aviso");
    assert!(aviso.contains("2 anexos vieram junto."), "{aviso}");
    assert!(!app.visible("cofre-atencao"));
    let cofre = outra.cofre();
    let painel = cofre
        .itens()
        .iter()
        .find(|i| i.titulo == "Painel da ACME")
        .unwrap();
    assert_eq!(painel.anexos.len(), 1);
    assert_ne!(painel.anexos[0].id, id, "id novo");
    let lido = anexos::ler_para_memoria(
        &outra.caminho().join(anexos::PASTA),
        &painel.anexos[0],
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(lido, b"-----BEGIN-----abacaxi");
}

#[test]
fn importar_o_json_sem_a_pasta_dos_anexos_avisa_quantos_faltaram() {
    let s = semeado("importar-sem-anexos");
    anexar(&s.pasta, s.painel, "chave.pem", b"abacaxi");
    anexar(&s.pasta, s.web, "foto.png", b"PNG-bytes");
    let mut app = pagina_sozinha(&s.pasta);
    let arquivo = s.pasta.caminho().join("saida.json");
    app.file_dialog_answer(Some(arquivo.clone()));
    app.click("cofre-exportar");
    app.click("cofre-exportar-tirech");
    esperar_ate(&mut app, "o JSON gravado", |a| {
        arquivo.exists() && a.visible("cofre-aviso")
    });

    // O JSON sozinho, num cofre vazio.
    let outra = Pasta::nova("importar-sem-anexos-destino");
    std::fs::create_dir_all(outra.caminho()).unwrap();
    std::fs::copy(&arquivo, outra.caminho().join("saida.json")).unwrap();
    let mut app = pagina_sozinha(&outra);
    app.file_dialog_answer(Some(outra.caminho().join("saida.json")));
    app.click("cofre-importar");
    esperar_ate(&mut app, "o aviso de anexos que faltam", |a| {
        a.visible("cofre-atencao")
    });
    let atencao = app.text("cofre-atencao");
    assert!(
        atencao.contains("2 anexos não foram encontrados ao lado do JSON."),
        "{atencao}"
    );
    let cofre = outra.cofre();
    assert_eq!(cofre.itens().len(), 6, "os itens entram mesmo assim");
    assert!(cofre.itens().iter().all(|i| i.anexos.is_empty()));
}

/// Como `pagina_sozinha`, devolvendo também o pedido de fora da busca global.
/// O sinal com que a busca global pede um item à página, depois que ela nasce.
type Pedido = Rc<RefCell<Option<Signal<Option<u64>>>>>;

fn pagina_com_pedido(pasta: &Pasta) -> (TestApp, Pedido) {
    let (cofre, _) = Cofre::abrir(pasta.caminho());
    let cofre = CofreCompartilhado::new(cofre);
    let pedido = Rc::new(RefCell::new(None));
    let guardado = Rc::clone(&pedido);
    let app = TestApp::new(move || {
        let escolher = signal(None);
        *guardado.borrow_mut() = Some(escolher);
        Form::new("Cofre")
            .density(Density::Compact)
            .child(pagina_cofre_com(cofre.clone(), escolher))
    })
    .size(1100, 640)
    .theme(Theme::Light);
    (app, pedido)
}

#[test]
fn busca_global_abre_a_pasta_do_item() {
    let s = semeado("busca");
    let (mut app, pedido) = pagina_com_pedido(&s.pasta);
    let escolher = pedido.borrow().unwrap();
    escolher.set(Some(s.painel));
    app.frame();
    assert_eq!(app.text("cofre-caminho"), "Cofre › Clientes › ACME");
    assert_eq!(app.grid("cofre-lista").selected, [s.chave_painel()]);
    assert!(
        pastas(&app).contains(&(2, "ACME".to_owned())),
        "a árvore abriu até a pasta"
    );
    assert!(app.properties(GRADE).iter().any(|p| p.name == "Título"));
    assert_eq!(escolher.get_untracked(), None, "o pedido foi atendido");
}

#[test]
fn cofre_v1_abre_com_pastas() {
    let pasta = Pasta::nova("v1");
    pasta.semear_cofre_v1(&[
        ("Clientes/ACME", "Painel", "s1"),
        ("Pessoal", "Banco", "s2"),
        ("", "Solto", "s3"),
    ]);
    let app = pagina_sozinha(&pasta);
    assert_eq!(
        pastas(&app),
        [
            (0, "Cofre".to_owned()),
            (1, "Clientes".to_owned()),
            (1, "Pessoal".to_owned())
        ]
    );
    assert_eq!(rotulos(&app), ["Clientes", "Pessoal", "Solto"]);
    assert!(pasta.caminho().join("vault.json").exists());
    assert!(!pasta.caminho().join("cofre.json").exists());
}

#[test]
fn definir_trocar_e_remover_a_senha_mestra() {
    let pasta = semeado("mestra").pasta;
    let mut app = pagina_sozinha(&pasta);
    assert!(
        !app.enabled("cofre-bloquear"),
        "sem senha mestra, nada a bloquear"
    );

    app.click("cofre-mestra");
    assert!(app.is_open("cofre-senha-mestra"));
    assert!(!app.visible("cofre-mestra-atual"));
    assert!(!app.visible("cofre-mestra-remover"));
    app.type_into("cofre-mestra-nova", "abc");
    app.type_into("cofre-mestra-confirme", "abd");
    app.click("cofre-senha-mestra-primary");
    assert!(app.is_open("cofre-senha-mestra"), "reabre com o erro");
    assert_eq!(app.text("cofre-mestra-erro"), "As senhas não conferem");
    assert!(!pasta.cofre().tem_senha_mestra());

    app.type_into("cofre-mestra-confirme", "abc");
    app.click("cofre-senha-mestra-primary");
    assert!(!app.is_open("cofre-senha-mestra"));
    assert!(pasta.cofre().tem_senha_mestra());
    assert!(!pasta.texto_cofre().contains("s3nh4"));
    assert!(app.enabled("cofre-bloquear"));

    // Trocar pede a atual.
    app.click("cofre-mestra");
    assert!(app.visible("cofre-mestra-atual"));
    app.type_into("cofre-mestra-atual", "errada");
    app.type_into("cofre-mestra-nova", "xyz");
    app.type_into("cofre-mestra-confirme", "xyz");
    app.click("cofre-senha-mestra-primary");
    assert!(app.is_open("cofre-senha-mestra"));
    assert_eq!(app.text("cofre-mestra-erro"), "Senha mestra incorreta");
    app.type_into("cofre-mestra-atual", "abc");
    app.click("cofre-senha-mestra-primary");
    assert!(!app.is_open("cofre-senha-mestra"));
    let mut aberto = pasta.cofre();
    assert!(!aberto.desbloquear("abc"));
    assert!(aberto.desbloquear("xyz"));

    // Remover.
    app.click("cofre-mestra");
    app.type_into("cofre-mestra-atual", "xyz");
    app.click("cofre-mestra-remover");
    assert!(!app.is_open("cofre-senha-mestra"));
    assert!(!pasta.cofre().tem_senha_mestra());
    assert_eq!(pasta.cofre().itens().len(), 6);
}

#[test]
fn bloquear_e_desbloquear() {
    let pasta = semeado("bloquear").pasta;
    pasta.cofre_com_senha_mestra("abc");
    let (mut cofre, _) = Cofre::abrir(pasta.caminho());
    assert!(cofre.desbloquear("abc"));
    let cofre = CofreCompartilhado::new(cofre);
    let mut app = TestApp::new(move || {
        Form::new("Cofre")
            .density(Density::Compact)
            .child(pagina_cofre(cofre.clone()))
    })
    .size(1100, 640)
    .theme(Theme::Light);
    assert_eq!(chaves(&app).len(), 4);
    assert!(!app.visible("cofre-bloqueado"));

    app.click("cofre-bloquear");
    assert!(app.visible("cofre-bloqueado"));
    assert!(!app.visible("cofre-principal"));
    assert!(!app.enabled("cofre-novo"));
    assert!(!app.enabled("cofre-bloquear"));

    app.click("cofre-desbloquear-botao");
    assert!(app.is_open("cofre-pagina-desbloquear"));
    app.type_into("cofre-pagina-desbloquear-campo", "errada");
    app.click("cofre-pagina-desbloquear-primary");
    assert!(app.is_open("cofre-pagina-desbloquear"));
    assert!(app.visible("cofre-pagina-desbloquear-erro"));
    app.type_into("cofre-pagina-desbloquear-campo", "abc");
    app.click("cofre-pagina-desbloquear-primary");
    assert!(!app.is_open("cofre-pagina-desbloquear"));
    assert!(!app.visible("cofre-bloqueado"));
    assert_eq!(chaves(&app).len(), 4);
}

#[test]
fn o_cofre_vazio_mostra_so_a_raiz() {
    let pasta = Pasta::nova("vazio");
    let app = pagina_sozinha(&pasta);
    assert!(chaves(&app).is_empty());
    assert_eq!(pastas(&app), [(0, "Cofre".to_owned())]);
}

// --- Dentro da tela ---

#[test]
fn ctrl_shift_k_escolhe_a_pagina_do_cofre_e_as_abas_continuam() {
    let pasta = Pasta::nova("modo");
    let ids = pasta.semear_cofre(vec![item_ssh("ana@web", "ana", "pw")]);
    let mut tree = comum::so_com_senha();
    tree.connection_mut(1).unwrap().credencial = Some(ids[0]);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    let abas = |app: &TestApp| {
        app.panes("paineis")
            .panes
            .iter()
            .map(|p| p.tabs.len())
            .sum::<usize>()
    };
    assert_eq!(abas(&m.app), 1);
    assert!(m.app.visible("arvore"));
    assert!(
        !m.app.visible("cofre-pagina"),
        "a página do cofre começa escondida"
    );

    m.app.shortcut(Shortcut::ctrl_shift('K'));
    assert_eq!(m.app.text("navegacao"), "cofre");
    assert!(m.app.visible("cofre-pagina"));
    assert!(!m.app.visible("arvore"));

    m.app.click("navegacao-terminais");
    assert!(!m.app.visible("cofre-pagina"));
    assert!(m.app.visible("arvore"));
    assert_eq!(abas(&m.app), 1, "as abas continuam");

    m.app.click("menus-ferramentas").click("abrir-cofre");
    assert_eq!(m.app.text("navegacao"), "cofre");
    assert!(m.app.visible("cofre-pagina"));
    // Do Início o atalho também vale.
    m.app.click("navegacao-inicio");
    m.app.shortcut(Shortcut::ctrl_shift('K'));
    assert_eq!(m.app.text("navegacao"), "cofre");
    m.app.click("navegacao-terminais");
    assert_eq!(abas(&m.app), 1);
}

#[test]
fn a_pagina_na_tela_edita_o_mesmo_cofre_das_conexoes() {
    let pasta = Pasta::nova("tela-edita");
    pasta.semear(&comum::so_com_senha());
    let ids = pasta.semear_cofre(vec![item_ssh("Servidor", "ana", "antiga")]);
    let mut tree = comum::so_com_senha();
    tree.connection_mut(1).unwrap().credencial = Some(ids[0]);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-ferramentas").click("abrir-cofre");
    m.app.click(&format!("cofre-lista-item:{}", ids[0]));
    editar(&mut m.app, "Senha", "nova");
    m.app.click("navegacao-terminais");
    m.app.double_click("arvore-1");
    assert!(!m.app.is_open("senha-dialogo"));
    assert_eq!(m.opener.senhas.borrow()[0].conexao.as_deref(), Some("nova"));
}

#[test]
fn o_cofre_bloqueado_na_tela_so_mostra_o_aviso() {
    let pasta = Pasta::nova("tela-bloqueado");
    pasta.semear_cofre(vec![item_do_tipo(Tipo::Nota, "Segredo")]);
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    assert!(m.app.is_open("cofre-desbloquear"));
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    m.app.click("menus-ferramentas").click("abrir-cofre");
    assert_eq!(chaves(&m.app), ["item:1"]);
    m.app.click("cofre-bloquear");
    assert!(m.app.visible("cofre-bloqueado"));
}

// ---------------------------------------------------------------------------------------------
// M44: o arquivo do cofre anda entre máquinas (`uid`, junção, `vault.key`, chaves SSH).
// ---------------------------------------------------------------------------------------------

use std::path::Path;

use tirech_omni::cofre::ChaveSsh;
use tomni_ui::terminal::ssh::keys::{KeyGen, KeyType};

/// Um item "Chave SSH" com uma Ed25519 recém-gerada.
fn chave_ssh(titulo: &str) -> Item {
    let gerada = KeyGen::new(KeyType::Ed25519)
        .comment(titulo)
        .generate()
        .expect("gerar a chave do teste");
    let aberta =
        tomni_ui::terminal::ssh::keys::KeyFile::from_text(gerada.private_openssh().as_str(), None)
            .expect("abrir a chave do teste");
    Item {
        tipo: Tipo::ChaveSsh,
        titulo: titulo.into(),
        chave: Some(ChaveSsh::de_chave(aberta.private_key()).expect("montar a chave")),
        ..Item::default()
    }
}

fn copiar(de: &Path, para: &Path, arquivos: &[&str]) {
    std::fs::create_dir_all(para).unwrap();
    for arquivo in arquivos {
        std::fs::copy(de.join(arquivo), para.join(arquivo)).unwrap();
    }
}

#[test]
fn cofre_copiado_para_outra_pasta_abre_igual() {
    let a = Pasta::nova("copia-a");
    let b = Pasta::nova("copia-b");
    let (mut cofre, _) = Cofre::abrir(a.caminho());
    let id = cofre.salvar(chave_ssh("github")).unwrap();
    cofre.salvar(item("Banco", "ana", "1234")).unwrap();
    let original = cofre.item(id).unwrap().clone();
    assert!(!original.uid.is_empty());

    copiar(a.caminho(), b.caminho(), &["vault.json", "vault.key"]);
    let (copia, aviso) = Cofre::abrir(b.caminho());
    assert_eq!(aviso, None);
    assert_eq!(copia.itens(), cofre.itens());
    let da_copia = copia.item_por_uid(&original.uid).unwrap();
    assert_eq!(da_copia.chave, original.chave);
    assert!(!da_copia.chave.as_ref().unwrap().privada.is_empty());
}

#[test]
fn cofre_com_senha_mestra_copiado_so_com_o_vault_json_abre_igual() {
    let a = Pasta::nova("copia-mestra-a");
    let b = Pasta::nova("copia-mestra-b");
    let (mut cofre, _) = Cofre::abrir(a.caminho());
    cofre.salvar(chave_ssh("github")).unwrap();
    cofre.definir_senha_mestra(Some("mestra-1")).unwrap();

    copiar(a.caminho(), b.caminho(), &["vault.json"]);
    let (mut copia, aviso) = Cofre::abrir(b.caminho());
    assert_eq!(aviso, None);
    assert!(copia.bloqueado());
    assert!(!copia.desbloquear("errada"));
    assert!(copia.desbloquear("mestra-1"));
    assert_eq!(copia.itens(), cofre.itens());
}

#[test]
fn so_o_cofre_json_sem_a_chave_da_o_erro_explicado() {
    let a = Pasta::nova("sem-chave-a");
    let b = Pasta::nova("sem-chave-b");
    let (mut cofre, _) = Cofre::abrir(a.caminho());
    cofre.salvar(item("Banco", "ana", "1234")).unwrap();

    copiar(a.caminho(), b.caminho(), &["vault.json"]);
    let antes = std::fs::read(b.caminho().join("vault.json")).unwrap();
    let (mut copia, aviso) = Cofre::abrir(b.caminho());
    let aviso = aviso.expect("avisa que falta o vault.key");
    assert!(aviso.contains("vault.key"), "{aviso}");
    assert!(copia.itens().is_empty());
    // Nada é gravado por cima nem vai para o `.bak`: o cofre da outra máquina continua inteiro.
    assert!(copia.salvar(item("Outro", "", "x")).is_err());
    assert_eq!(
        std::fs::read(b.caminho().join("vault.json")).unwrap(),
        antes
    );
    assert!(!b.caminho().join("vault.json.bak").exists());
    assert!(!b.caminho().join("vault.key").exists());
}

#[test]
fn juntar_pelo_uid() {
    let a = Pasta::nova("juntar-a");
    let b = Pasta::nova("juntar-b");
    let (mut ca, _) = Cofre::abrir(a.caminho());
    let comum = ca.salvar(item("Comum", "ana", "velha")).unwrap();
    let apagado = ca.salvar(item("Vai sumir", "", "x")).unwrap();
    let editado_nos_dois = ca.salvar(item("Disputa", "", "inicial")).unwrap();
    copiar(a.caminho(), b.caminho(), &["vault.json", "vault.key"]);
    let (mut cb, _) = Cofre::abrir(b.caminho());

    // Um segundo depois, porque a data de alteração é em segundos.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let uid_apagado = ca.item(apagado).unwrap().uid.clone();
    let uid_comum = ca.item(comum).unwrap().uid.clone();
    let uid_disputa = ca.item(editado_nos_dois).unwrap().uid.clone();

    // Em A: edita "Disputa" (fica o mais novo, o de B), cria um item novo e apaga um.
    let mut disputa = ca.item(editado_nos_dois).unwrap().clone();
    disputa.senha = "do-a".into();
    ca.salvar(disputa).unwrap();
    ca.salvar(item("Só no A", "", "a")).unwrap();
    ca.remover(apagado).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    // Em B: edita "Disputa" depois (vence), edita "Comum" e cria um item novo.
    let id_disputa_b = cb.item_por_uid(&uid_disputa).unwrap().id;
    let mut disputa = cb.item(id_disputa_b).unwrap().clone();
    disputa.senha = "do-b".into();
    cb.salvar(disputa).unwrap();
    let id_comum_b = cb.item_por_uid(&uid_comum).unwrap().id;
    let mut comum_b = cb.item(id_comum_b).unwrap().clone();
    comum_b.senha = "nova".into();
    cb.salvar(comum_b).unwrap();
    cb.salvar(item("Só no B", "", "b")).unwrap();

    let resumo = ca.juntar(cb.conteudo()).unwrap();
    assert!(resumo.mudou());
    let titulos: Vec<&str> = {
        let mut t: Vec<&str> = ca.itens().iter().map(|i| i.titulo.as_str()).collect();
        t.sort_unstable();
        t
    };
    assert_eq!(titulos, ["Comum", "Disputa", "Só no A", "Só no B"]);
    let senha = |c: &Cofre, uid: &str| c.item_por_uid(uid).unwrap().senha.clone();
    assert_eq!(senha(&ca, &uid_disputa), "do-b");
    assert_eq!(senha(&ca, &uid_comum), "nova");
    assert!(ca.item_por_uid(&uid_apagado).is_none());

    // Do outro lado a lápide faz o mesmo: o item apagado em A some de B.
    let resumo = cb.juntar(ca.conteudo()).unwrap();
    assert!(resumo.mudou());
    assert!(cb.item_por_uid(&uid_apagado).is_none());
    let mut de_b: Vec<&str> = cb.itens().iter().map(|i| i.titulo.as_str()).collect();
    de_b.sort_unstable();
    assert_eq!(de_b, titulos);
}

#[test]
fn mudanca_no_disco_e_juntada_antes_de_gravar() {
    let pasta = Pasta::nova("duas-copias");
    let (mut a, _) = Cofre::abrir(pasta.caminho());
    a.salvar(item("Base", "", "0")).unwrap();
    // O mesmo arquivo aberto duas vezes, como duas máquinas com a pasta sincronizada.
    let (mut b, _) = Cofre::abrir(pasta.caminho());

    a.salvar(item("Do A", "", "a")).unwrap();
    b.salvar(item("Do B", "", "b")).unwrap();

    let (relido, aviso) = Cofre::abrir(pasta.caminho());
    assert_eq!(aviso, None);
    let mut titulos: Vec<&str> = relido.itens().iter().map(|i| i.titulo.as_str()).collect();
    titulos.sort_unstable();
    assert_eq!(titulos, ["Base", "Do A", "Do B"]);
    // O B também ficou com o do A, sem reabrir.
    assert_eq!(b.itens().len(), 3);
}

#[cfg(unix)]
#[test]
fn gravar_e_0600() {
    use std::os::unix::fs::PermissionsExt;
    let pasta = Pasta::nova("modo-0600");
    let (mut cofre, _) = Cofre::abrir(pasta.caminho());
    cofre.salvar(chave_ssh("github")).unwrap();
    for arquivo in ["vault.json", "vault.key"] {
        let modo = std::fs::metadata(pasta.caminho().join(arquivo))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(modo & 0o777, 0o600, "{arquivo}");
    }
}

#[test]
fn passphrase_achada_pelo_fingerprint_em_outra_pasta() {
    use tirech_omni::agente::{guardar_passphrase, passphrase_do_cofre};
    let pasta = Pasta::nova("passphrase-fp");
    let (cofre, _) = Cofre::abrir(pasta.caminho());
    let cofre = CofreCompartilhado::new(cofre);

    let chave = KeyGen::new(KeyType::Ed25519)
        .comment("ana@teste")
        .passphrase("frase-do-teste")
        .generate()
        .expect("gerar a chave do teste");
    let aqui = pasta.caminho().join("casa-a/id_ed25519");
    chave.write(&aqui, false).unwrap();
    guardar_passphrase(&cofre, &aqui, "frase-do-teste").unwrap();
    assert_eq!(
        passphrase_do_cofre(&cofre, &aqui).as_deref(),
        Some("frase-do-teste")
    );

    // A mesma chave, copiada para outro caminho (outra máquina): ainda acha, pelo fingerprint.
    let la = pasta.caminho().join("casa-b/.ssh/id_ed25519");
    std::fs::create_dir_all(la.parent().unwrap()).unwrap();
    std::fs::copy(&aqui, &la).unwrap();
    std::fs::copy(aqui.with_extension("pub"), la.with_extension("pub")).unwrap();
    assert_eq!(
        passphrase_do_cofre(&cofre, &la).as_deref(),
        Some("frase-do-teste")
    );
}
