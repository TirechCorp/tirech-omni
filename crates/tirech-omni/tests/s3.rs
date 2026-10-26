//! O S3 no app (M32): as opções na grade de propriedades, a chave secreta no cofre e fora do
//! `connections.json`, a abertura na página Arquivos contra o servidor S3 de teste, o link
//! pré-assinado, as propriedades do objeto, a classe de armazenamento, os envios incompletos e
//! a importação do `~/.aws`.
#![cfg(unix)]

mod comum;

#[path = "comum/s3d.rs"]
mod s3d;

use std::io::Write;
use std::time::{Duration, Instant};

use tirech_omni::cofre::{CAMPO_TOKEN, Item, Tipo};
use tirech_omni::modelo::{Connection, Kind, S3Estilo, S3Provedor, Tree};
use tomni_ui::files::Fs;
use tomni_ui::files::s3::S3;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, editar_propriedade, escolher_propriedade, montar, valor};
use s3d::{ACESSO, S3d, SEGREDO};

const MIB: usize = 1 << 20;

/// Uma conexão S3 (id 1, `nuvem`) no bucket `bkt`, ligada ao item 1 do cofre.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    let mut s3 = Connection::new("nuvem", Kind::S3);
    s3.user = ACESSO.into();
    s3.s3.provider = S3Provedor::Minio;
    s3.s3.endpoint = "http://127.0.0.1:1".into();
    s3.s3.url_style = S3Estilo::Caminho;
    s3.s3.bucket = "bkt".into();
    s3.credencial = Some(1);
    tree.add_connection(None, s3);
    tree
}

fn item_s3() -> Item {
    Item {
        tipo: Tipo::S3,
        titulo: "nuvem".into(),
        usuario: ACESSO.into(),
        senha: SEGREDO.into(),
        ..Item::default()
    }
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// A tela com a conexão `nuvem`, o servidor de teste (com o bucket `bkt`) e o abridor falso
/// apontado para ele.
fn montar_s3(nome: &str) -> (Pasta, Montagem, S3d) {
    let pasta = Pasta::nova(nome);
    pasta.semear_cofre(vec![item_s3()]);
    pasta.semear(&arvore());
    let servidor = S3d::novo();
    servidor.criar_bucket("bkt");
    let m = montar(&pasta, Theme::Light);
    *m.opener.s3.borrow_mut() = Some(servidor.config(Some("bkt")));
    (pasta, m, servidor)
}

/// Abre a conexão e espera a listagem do bucket.
fn abrir(m: &mut Montagem, chave: &str) {
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "a listagem do bucket", |a| {
        a.visible("a1-remoto") && a.grid("a1-remoto").keys().contains(&chave)
    });
    m.app.wait(Duration::from_millis(100));
}

#[test]
fn a_grade_mostra_as_opcoes_do_s3_e_o_provedor_preenche_endereco_e_estilo() {
    let pasta = Pasta::nova("s3-opcoes");
    pasta.semear(&arvore());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    assert_eq!(valor(&m.app, "Tipo"), "S3");
    assert_eq!(valor(&m.app, "Provedor"), "MinIO");
    assert_eq!(valor(&m.app, "Bucket"), "bkt");
    assert_eq!(valor(&m.app, "Estilo da URL"), S3Estilo::Caminho.nome());
    let props = m.app.properties("props");
    let ativo = |nome: &str| props.iter().find(|p| p.name == nome).unwrap().enabled;
    let escondida = |nome: &str| props.iter().find(|p| p.name == nome).unwrap().hidden;
    assert!(ativo("Provedor") && ativo("Endereço") && ativo("Bucket"));
    assert!(escondida("Chave privada"), "o S3 não usa chave SSH");
    assert!(!ativo("Arquivo da CA"), "só no modo CA própria");
    m.app.assert_lint();

    escolher_propriedade(&mut m.app, "Provedor");
    m.app.select("props-Provedor-valor", "Wasabi");
    assert_eq!(
        valor(&m.app, "Endereço"),
        "https://s3.us-east-1.wasabisys.com"
    );
    assert_eq!(valor(&m.app, "Estilo da URL"), S3Estilo::Virtual.nome());
    let c = pasta.conexoes().connections()[0].clone();
    assert_eq!(c.s3.provider, S3Provedor::Wasabi);
    assert_eq!(c.s3.endpoint, "https://s3.us-east-1.wasabisys.com");
}

#[test]
fn virar_s3_mostra_as_opcoes_da_amazon_e_fora_do_s3_o_endereco_fica_em_branco() {
    let pasta = Pasta::nova("s3-virar");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("servidor", Kind::Ssh));
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    assert_eq!(valor(&m.app, "Endereço"), "");
    assert_eq!(pasta.conexoes().connections()[0].kind, Kind::Ssh);

    escolher_propriedade(&mut m.app, "Tipo");
    m.app.select("props-Tipo-valor", "S3");
    assert_eq!(
        valor(&m.app, "Endereço"),
        "https://s3.us-east-1.amazonaws.com"
    );
    let c = pasta.conexoes().connections()[0].clone();
    assert_eq!(c.kind, Kind::S3);
    assert_eq!(c.s3.provider, S3Provedor::Amazon);
    assert_eq!(c.s3.endpoint, "https://s3.us-east-1.amazonaws.com");
}

#[test]
fn a_chave_secreta_e_o_token_vao_para_o_cofre_e_nunca_para_o_json() {
    let pasta = Pasta::nova("s3-segredo");
    let mut tree = arvore();
    tree.connection_mut(1).unwrap().credencial = None;
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    editar_propriedade(&mut m.app, "Senha", "chave-secreta-xyz");
    editar_propriedade(&mut m.app, "Token de sessão", "token-sts-123");

    let cofre = pasta.cofre();
    assert_eq!(cofre.itens().len(), 1);
    let item = &cofre.itens()[0];
    assert_eq!(item.tipo, Tipo::S3);
    assert_eq!(item.usuario, ACESSO);
    assert_eq!(item.senha, "chave-secreta-xyz");
    let token = item.campos.iter().find(|c| c.nome == CAMPO_TOKEN).unwrap();
    assert_eq!(token.valor, "token-sts-123");
    assert!(token.oculto);

    let json = pasta.texto_conexoes();
    for segredo in ["chave-secreta-xyz", "token-sts-123"] {
        assert!(!json.contains(segredo), "{json}");
    }
    assert!(!json.to_lowercase().contains("senha"), "{json}");
}

#[test]
fn abrir_uma_conexao_s3_lista_o_bucket_e_entra_nos_recentes() {
    let (_pasta, mut m, servidor) = montar_s3("s3-abrir");
    servidor.colocar("bkt", "notas.txt", b"ola");
    servidor.colocar("bkt", "docs/a.txt", b"a");
    abrir(&mut m, "notas.txt");
    assert!(m.app.grid("a1-remoto").keys().contains(&"docs"));
    assert_eq!(m.app.tab_titles("arquivos-abas"), ["nuvem"]);
    assert!(
        m.app.visible("a1-s3-pendentes"),
        "o botão dos envios incompletos"
    );
    assert!(
        m.opener.abertas.borrow().is_empty(),
        "não abriu um terminal"
    );

    m.app.double_click("a1-remoto-docs");
    esperar(&mut m.app, "a pasta docs", |a| {
        a.grid("a1-remoto").keys().contains(&"a.txt")
    });
    m.app.close_tab("arquivos-abas", "a1");
    assert!(
        m.app.visible("arquivos-recente-0"),
        "a pasta docs nos recentes"
    );
}

#[test]
fn copiar_o_link_pre_assinado_poe_a_url_assinada_na_area_de_transferencia() {
    let (_pasta, mut m, servidor) = montar_s3("s3-link");
    servidor.colocar("bkt", "notas.txt", b"ola");
    abrir(&mut m, "notas.txt");
    m.app.context_menu("a1-remoto-notas.txt");
    m.app.click("a1-remoto-notas.txt-link");
    m.app.click("a1-remoto-notas.txt-link-1");
    esperar(&mut m.app, "o aviso do link", |a| a.visible("a1-s3-aviso"));
    m.app.frame();
    let link = m.app.clipboard().read().expect("o link foi copiado");
    assert!(link.starts_with(&servidor.endpoint()), "{link}");
    assert!(link.contains("/bkt/notas.txt"), "{link}");
    assert!(link.contains("X-Amz-Signature="), "{link}");
    assert!(link.contains("X-Amz-Expires=86400"), "{link}");
    assert!(
        !link.contains(SEGREDO) && !link.contains("segredo"),
        "o segredo não vai no link"
    );
    assert!(m.app.text("a1-s3-aviso").contains("copiado"));
    m.app.assert_lint();
}

#[test]
fn as_propriedades_do_objeto_mostram_tamanho_classe_e_tipo() {
    let (_pasta, mut m, servidor) = montar_s3("s3-objeto");
    servidor.colocar("bkt", "notas.txt", b"ola mundo");
    abrir(&mut m, "notas.txt");
    m.app.context_menu("a1-remoto-notas.txt");
    m.app.click("a1-remoto-notas.txt-objeto");
    esperar(&mut m.app, "o diálogo do objeto", |a| {
        a.is_open("a1-s3-objeto-dialogo")
    });
    let texto = m.app.text("a1-s3-objeto-linhas");
    assert!(texto.contains("Tamanho: 9 B"), "{texto}");
    assert!(texto.contains("Classe: STANDARD"), "{texto}");
    assert!(texto.contains("Tipo: "), "{texto}");
    m.app.assert_lint();
}

#[test]
fn trocar_a_classe_de_armazenamento_pelo_menu() {
    let (_pasta, mut m, servidor) = montar_s3("s3-classe");
    servidor.colocar("bkt", "notas.txt", b"ola");
    abrir(&mut m, "notas.txt");
    m.app.context_menu("a1-remoto-notas.txt");
    m.app.click("a1-remoto-notas.txt-classe");
    m.app.click("a1-remoto-notas.txt-classe-STANDARD_IA");
    esperar(&mut m.app, "a classe nova", |_| {
        servidor.classe("bkt", "notas.txt").as_deref() == Some("STANDARD_IA")
    });
    esperar(&mut m.app, "o aviso", |a| a.visible("a1-s3-aviso"));
    assert!(m.app.text("a1-s3-aviso").contains("STANDARD_IA"));
}

#[test]
fn lista_e_aborta_os_envios_incompletos() {
    let (_pasta, mut m, servidor) = montar_s3("s3-pendentes");
    servidor.colocar("bkt", "notas.txt", b"ola");
    {
        let mut config = servidor.config(Some("bkt"));
        config.part_size = 5 * MIB as u64;
        let mut s3 = S3::connect(config).unwrap();
        let mut envio = s3.open_write("/grande.bin", 0).unwrap();
        envio.write_all(&vec![7_u8; 6 * MIB]).unwrap();
        // Soltar sem `finish` deixa as partes no servidor.
    }
    assert_eq!(servidor.envios_abertos(), 1);

    abrir(&mut m, "notas.txt");
    m.app.click("a1-s3-pendentes");
    esperar(&mut m.app, "o diálogo", |a| {
        a.is_open("a1-s3-pendentes-dialogo")
    });
    esperar(&mut m.app, "a lista", |a| {
        a.text("a1-s3-pendentes-estado").contains("Abortar apaga")
    });
    m.app.assert_lint();
    m.app.click("a1-s3-pendentes-dialogo-primary");
    esperar(&mut m.app, "o envio abortado", |_| {
        servidor.envios_abertos() == 0
    });
}

#[test]
fn importar_do_aws_cria_uma_conexao_por_perfil_e_guarda_as_chaves_no_cofre() {
    let pasta = Pasta::nova("s3-importar-aws");
    let mut m = montar(&pasta, Theme::Light);
    let aws = pasta.caminho().join("aws");
    std::fs::create_dir_all(&aws).unwrap();
    std::fs::write(
        aws.join("credentials"),
        "[default]\naws_access_key_id = AKIADEFAULT\naws_secret_access_key = segredo-um\n\n\
         [minio]\naws_access_key_id = minioadmin\naws_secret_access_key = minioadmin\n\
         aws_session_token = tok\n",
    )
    .unwrap();
    std::fs::write(
        aws.join("config"),
        "[default]\nregion = sa-east-1\n\n[profile minio]\nregion = us-east-1\n\
         endpoint_url = http://127.0.0.1:9000\n",
    )
    .unwrap();

    m.app.click("menus-arquivo");
    m.app.hover("importar-menu");
    m.app.wait(std::time::Duration::from_millis(450));
    m.app.click("importar-aws");
    let pedidos = m.app.file_dialogs();
    assert_eq!(pedidos.len(), 1);
    assert_eq!(pedidos[0].file_name.as_deref(), Some("credentials"));
    m.app.file_dialog_answer(Some(aws.join("credentials")));

    let tree = pasta.conexoes();
    assert_eq!(tree.connection_count(), 2);
    let conexoes = tree.connections();
    assert_eq!(conexoes[0].name, "default");
    assert_eq!(conexoes[0].user, "AKIADEFAULT");
    assert_eq!(conexoes[0].s3.region, "sa-east-1");
    assert_eq!(conexoes[0].s3.provider, S3Provedor::Amazon);
    assert_eq!(conexoes[1].name, "minio");
    assert_eq!(conexoes[1].s3.endpoint, "http://127.0.0.1:9000");
    assert_eq!(conexoes[1].s3.provider, S3Provedor::Minio);

    let cofre = pasta.cofre();
    assert_eq!(cofre.itens().len(), 2);
    assert!(cofre.itens().iter().any(|i| i.senha == "segredo-um"));
    let minio = cofre
        .itens()
        .iter()
        .find(|i| i.usuario == "minioadmin")
        .unwrap();
    assert!(
        minio
            .campos
            .iter()
            .any(|c| c.nome == CAMPO_TOKEN && c.valor == "tok")
    );
    let json = pasta.texto_conexoes();
    assert!(!json.contains("segredo-um"), "{json}");
    assert!(!json.contains("\"tok\""), "{json}");
    let rotulos: Vec<String> = m
        .app
        .tree_items("arvore")
        .into_iter()
        .map(|r| r.label)
        .collect();
    assert_eq!(rotulos[0], "Importadas do ~/.aws");
}
