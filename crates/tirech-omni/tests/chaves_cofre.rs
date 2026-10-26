//! A página "Chaves SSH" com o cofre: importar (arquivo e texto colado), a lista de chaves do
//! cofre, exportar e converter para outros formatos, e trazer as chaves de `~/.ssh`.

mod comum;

use std::path::{Path, PathBuf};

use tirech_omni::cofre::Tipo;
use tomni_ui::terminal::ssh::keys::{
    self, GeneratedKey, KdfLevel, KeyGen, KeyType, PrivateFormat, RsaBits,
};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use tomni_ui::Key;

use comum::{Montagem, Pasta, escolher_propriedade, montar, so_com_senha, valor};

fn chave() -> GeneratedKey {
    KeyGen::new(KeyType::Ed25519)
        .comment("ana@teste")
        .generate()
        .expect("gerar a chave do teste")
}

/// A privada da chave no formato dado, com ou sem passphrase.
fn exportada(chave: &GeneratedKey, formato: PrivateFormat, passphrase: Option<&str>) -> String {
    keys::export_private(chave.private_key(), formato, passphrase, KdfLevel::Standard)
        .expect("exportar a chave do teste")
        .to_string()
}

/// Grava `texto` em `nome`, dentro de uma pasta de arquivos soltos da `pasta`.
fn arquivo_solto(pasta: &Pasta, nome: &str, texto: &str) -> PathBuf {
    let dir = pasta.caminho().join("soltos");
    std::fs::create_dir_all(&dir).unwrap();
    let caminho = dir.join(nome);
    std::fs::write(&caminho, texto).unwrap();
    caminho
}

/// A tela na página Chaves SSH.
fn na_pagina(pasta: &Pasta) -> Montagem {
    let mut m = montar(pasta, Theme::Light);
    m.app.click("navegacao-chaves");
    assert!(m.app.visible("chaves-pagina"));
    m
}

/// Importa um arquivo pelo botão "Importar…" e devolve com o diálogo aberto.
fn importar_arquivo(app: &mut TestApp, arquivo: &Path) {
    app.file_dialog_answer(Some(arquivo.to_path_buf()));
    app.click("chaves-importar");
    assert!(app.is_open("chaves-importar-dialogo"));
}

/// O id do primeiro item de chave SSH do cofre.
fn id_da_chave(pasta: &Pasta) -> u64 {
    let cofre = pasta.cofre();
    let chaves = cofre.chaves_ssh();
    assert_eq!(chaves.len(), 1, "uma chave no cofre");
    chaves[0].id
}

#[test]
fn importar_um_ppk_v3_com_passphrase_guarda_a_chave_no_cofre() {
    let pasta = Pasta::nova("cc-ppk");
    let gerada = chave();
    let arquivo = arquivo_solto(
        &pasta,
        "servidor.ppk",
        &exportada(&gerada, PrivateFormat::PpkV3, Some("frase-do-ppk")),
    );
    let mut m = na_pagina(&pasta);
    importar_arquivo(&mut m.app, &arquivo);
    assert!(m.app.visible("chaves-importar-passphrase"));
    m.app.assert_lint();
    m.app
        .type_into("chaves-importar-passphrase", "frase-do-ppk");
    m.app.click("chaves-importar-dialogo-primary");

    assert!(!m.app.is_open("chaves-importar-dialogo"));
    let id = id_da_chave(&pasta);
    assert!(m.app.visible(&format!("chave-cofre-{id}")));
    assert!(
        m.app
            .text(&format!("chave-cofre-{id}-fingerprint"))
            .contains(&gerada.fingerprint())
    );
    let cofre = pasta.cofre();
    let item = cofre.item(id).unwrap();
    assert_eq!(item.tipo, Tipo::ChaveSsh);
    let chave_ssh = item.chave.as_ref().unwrap();
    assert!(chave_ssh.privada.contains("BEGIN OPENSSH PRIVATE KEY"));
    assert_eq!(chave_ssh.fingerprint, gerada.fingerprint());
    assert_eq!(
        item.senha, "frase-do-ppk",
        "a passphrase de origem fica no item"
    );
    // O arquivo continua onde estava: só some se a pessoa pedir.
    assert!(arquivo.is_file());
}

#[test]
fn importar_com_a_passphrase_errada_reabre_com_erro() {
    let pasta = Pasta::nova("cc-ppk-errada");
    let arquivo = arquivo_solto(
        &pasta,
        "servidor.ppk",
        &exportada(&chave(), PrivateFormat::PpkV3, Some("certa")),
    );
    let mut m = na_pagina(&pasta);
    importar_arquivo(&mut m.app, &arquivo);
    m.app.type_into("chaves-importar-passphrase", "errada");
    m.app.click("chaves-importar-dialogo-primary");

    assert!(m.app.is_open("chaves-importar-dialogo"), "reabre no erro");
    assert!(m.app.visible("chaves-importar-erro"));
    assert!(pasta.cofre().chaves_ssh().is_empty());
}

#[test]
fn colar_uma_publica_cria_o_item_so_com_a_publica() {
    let pasta = Pasta::nova("cc-publica");
    let gerada = chave();
    let mut m = na_pagina(&pasta);
    m.app.click("chaves-colar");
    assert!(m.app.is_open("chaves-importar-dialogo"));
    m.app
        .type_into("chaves-importar-texto", &gerada.public_openssh());
    assert!(m.app.text("chaves-importar-formato").contains("pública"));
    m.app.click("chaves-importar-dialogo-primary");

    let id = id_da_chave(&pasta);
    let cofre = pasta.cofre();
    let chave_ssh = cofre.item(id).unwrap().chave.as_ref().unwrap();
    assert!(chave_ssh.privada.is_empty());
    assert_eq!(chave_ssh.fingerprint, gerada.fingerprint());
    assert!(
        m.app
            .text(&format!("chave-cofre-{id}-marcas"))
            .contains("só a pública")
    );
}

#[test]
fn exportar_para_ppk_v2_grava_um_arquivo_que_abre_de_volta() {
    let pasta = Pasta::nova("cc-exportar");
    let gerada = chave();
    let origem = arquivo_solto(
        &pasta,
        "id_ed25519",
        &exportada(&gerada, PrivateFormat::OpenSsh, None),
    );
    let mut m = na_pagina(&pasta);
    importar_arquivo(&mut m.app, &origem);
    m.app.click("chaves-importar-dialogo-primary");
    let id = id_da_chave(&pasta);

    m.app.click(&format!("chave-cofre-{id}-mais"));
    m.app.click(&format!("chave-cofre-{id}-exportar"));
    assert!(m.app.is_open("chaves-exportar-dialogo"));
    m.app.assert_lint();
    m.app.type_into("chaves-exportar-passphrase", "nova");
    m.app.type_into("chaves-exportar-repetir", "nova");
    m.app.select(
        "chaves-exportar-formato",
        "PuTTY (.ppk) versão 2 (programas antigos)",
    );
    let destino = pasta.caminho().join("soltos").join("saida.ppk");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("chaves-exportar-salvar");

    let texto = std::fs::read_to_string(&destino).expect("o arquivo exportado");
    assert!(texto.starts_with("PuTTY-User-Key-File-2"));
    let volta = keys::KeyFile::open(&destino, Some("nova")).expect("abrir o .ppk de volta");
    assert_eq!(
        keys::fingerprint(volta.private_key().public_key()),
        gerada.fingerprint()
    );
}

#[test]
fn converter_uma_pem_para_openssh_com_passphrase() {
    let pasta = Pasta::nova("cc-converter");
    let gerada = KeyGen::new(KeyType::Rsa(RsaBits::B3072))
        .comment("ana@teste")
        .generate()
        .expect("gerar a RSA do teste");
    let origem = arquivo_solto(
        &pasta,
        "id_rsa",
        &exportada(&gerada, PrivateFormat::PemLegacy, None),
    );
    let mut m = na_pagina(&pasta);
    m.app.file_dialog_answer(Some(origem));
    m.app.click("chaves-converter");
    assert!(m.app.is_open("chaves-converter-dialogo"));
    m.app.select(
        "chaves-converter-formato",
        "OpenSSH (padrão do Linux e do macOS)",
    );
    m.app.type_into("chaves-converter-passphrase", "nova-frase");
    m.app.type_into("chaves-converter-repetir", "nova-frase");
    let destino = pasta.caminho().join("soltos").join("id_rsa_openssh");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("chaves-converter-salvar");

    let texto = std::fs::read_to_string(&destino).unwrap();
    assert!(texto.contains("BEGIN OPENSSH PRIVATE KEY"));
    assert!(keys::KeyFile::open(&destino, Some("nova-frase")).is_ok());
    assert!(keys::KeyFile::open(&destino, Some("outra")).is_err());
}

#[test]
fn pem_tradicional_para_ed25519_avisa_e_desabilita_salvar() {
    let pasta = Pasta::nova("cc-pem-ed");
    let origem = arquivo_solto(
        &pasta,
        "id_ed25519",
        &exportada(&chave(), PrivateFormat::OpenSsh, None),
    );
    let mut m = na_pagina(&pasta);
    m.app.file_dialog_answer(Some(origem));
    m.app.click("chaves-converter");
    m.app.select(
        "chaves-converter-formato",
        "PEM tradicional (AWS, Paramiko)",
    );

    assert!(m.app.visible("chaves-converter-aviso"));
    assert!(
        m.app
            .text("chaves-converter-aviso")
            .contains("Ed25519 não tem PEM")
    );
    assert!(!m.app.enabled("chaves-converter-salvar"));
    assert!(!m.app.enabled("chaves-converter-copiar"));
    m.app.assert_lint();
}

#[test]
fn trazer_todas_de_ssh_pede_a_passphrase_e_lista_a_falha() {
    let pasta = Pasta::nova("cc-trazer");
    std::fs::create_dir_all(pasta.ssh()).unwrap();
    let aberta = chave();
    let protegida = chave();
    std::fs::write(
        pasta.ssh().join("id_aberta"),
        exportada(&aberta, PrivateFormat::OpenSsh, None),
    )
    .unwrap();
    std::fs::write(
        pasta.ssh().join("id_protegida"),
        exportada(&protegida, PrivateFormat::OpenSsh, Some("certa")),
    )
    .unwrap();
    let mut m = na_pagina(&pasta);
    m.app.click("chaves-de-ssh");
    assert!(m.app.is_open("chaves-de-ssh-dialogo"));
    m.app.assert_lint();
    m.app
        .type_into("chaves-de-ssh-id_protegida-passphrase", "errada");
    m.app.click("chaves-de-ssh-dialogo-primary");

    // A errada falha e fica no diálogo; a outra entrou.
    assert!(m.app.is_open("chaves-de-ssh-dialogo"), "reabre com a falha");
    assert!(m.app.visible("chaves-de-ssh-erro"));
    assert_eq!(pasta.cofre().chaves_ssh().len(), 1);

    m.app.click("chaves-de-ssh-id_protegida-passphrase");
    m.app.shortcut('a');
    m.app.type_text("certa");
    m.app.click("chaves-de-ssh-dialogo-primary");
    assert!(!m.app.is_open("chaves-de-ssh-dialogo"));
    assert_eq!(pasta.cofre().chaves_ssh().len(), 2);
    // Os arquivos ficam, porque ninguém pediu para apagar.
    assert!(pasta.ssh().join("id_aberta").is_file());
    assert!(pasta.ssh().join("id_protegida").is_file());
}

/// Uma chave Ed25519 com o comentário dado.
fn chave_de(comentario: &str) -> GeneratedKey {
    KeyGen::new(KeyType::Ed25519)
        .comment(comentario)
        .generate()
        .expect("gerar a chave do teste")
}

#[test]
fn importar_varios_arquivos_traz_todos_e_tenta_a_passphrase_nas_outras() {
    let pasta = Pasta::nova("cc-varios");
    let aberta = arquivo_solto(
        &pasta,
        "id_aberta",
        &exportada(&chave_de("aberta"), PrivateFormat::OpenSsh, None),
    );
    let uma = arquivo_solto(
        &pasta,
        "id_uma",
        &exportada(&chave_de("uma"), PrivateFormat::OpenSsh, Some("certa")),
    );
    // A mesma passphrase, num PPK: só a primeira linha recebe o que foi digitado.
    let outra = arquivo_solto(
        &pasta,
        "id_outra.ppk",
        &exportada(&chave_de("outra"), PrivateFormat::PpkV3, Some("certa")),
    );
    let lixo = arquivo_solto(&pasta, "leia-me.txt", "não é uma chave");
    let mut m = na_pagina(&pasta);
    m.app
        .file_dialog_answer_many([&aberta, &uma, &outra, &lixo]);
    m.app.click("chaves-importar");

    assert!(m.app.is_open("chaves-de-ssh-dialogo"));
    assert!(m.app.visible("chaves-de-ssh-id_outra.ppk-passphrase"));
    assert!(
        m.app
            .text("chaves-de-ssh-ilegiveis")
            .contains("leia-me.txt")
    );
    assert!(m.app.visible("chaves-de-ssh-destino"));
    assert!(
        !m.app.visible("chaves-de-ssh-apagar"),
        "nenhum arquivo é de ~/.ssh"
    );
    m.app.assert_lint();
    m.app.type_into("chaves-de-ssh-id_uma-passphrase", "certa");
    m.app.click("chaves-de-ssh-dialogo-primary");

    assert!(!m.app.is_open("chaves-de-ssh-dialogo"));
    let mut titulos: Vec<String> = pasta
        .cofre()
        .chaves_ssh()
        .iter()
        .map(|i| i.titulo.clone())
        .collect();
    titulos.sort();
    // A OpenSSH cifrada esconde o comentário: o título é o nome do arquivo.
    assert_eq!(titulos, ["aberta", "id_uma", "outra"]);
    assert!(aberta.is_file() && uma.is_file() && outra.is_file());
}

#[test]
fn importar_varios_para_ssh_grava_os_arquivos_com_o_nome_de_origem() {
    let pasta = Pasta::nova("cc-varios-ssh");
    let um = arquivo_solto(
        &pasta,
        "cliente_a",
        &exportada(&chave(), PrivateFormat::OpenSsh, None),
    );
    let dois = arquivo_solto(
        &pasta,
        "cliente_b",
        &exportada(&chave(), PrivateFormat::Pkcs8, None),
    );
    let mut m = na_pagina(&pasta);
    m.app.file_dialog_answer_many([&um, &dois]);
    m.app.click("chaves-importar");
    m.app
        .select("chaves-de-ssh-destino", "Arquivo em ~/.ssh (OpenSSH)");
    m.app.click("chaves-de-ssh-dialogo-primary");

    assert!(!m.app.is_open("chaves-de-ssh-dialogo"));
    assert!(pasta.ssh().join("cliente_a").is_file());
    assert!(pasta.ssh().join("cliente_b.pub").is_file());
    assert!(pasta.cofre().chaves_ssh().is_empty());
}

#[test]
fn o_seletor_de_importar_reabre_na_pasta_da_ultima_importacao() {
    let pasta = Pasta::nova("cc-pasta-lembrada");
    let origem = arquivo_solto(
        &pasta,
        "id_ed25519",
        &exportada(&chave(), PrivateFormat::OpenSsh, None),
    );
    let mut m = na_pagina(&pasta);
    importar_arquivo(&mut m.app, &origem);
    m.app.click("chaves-importar-dialogo-close");

    m.app.click("chaves-importar");
    let pedidos = m.app.file_dialogs();
    let ultimo = pedidos.last().expect("o seletor abriu");
    assert_eq!(ultimo.directory.as_deref(), origem.parent());
    // E a pasta fica gravada para a próxima vez que o app abrir.
    let gravado = std::fs::read_to_string(pasta.caminho().join("pastas.json")).unwrap();
    assert!(gravado.contains("soltos"), "{gravado}");
}

#[test]
fn remover_do_cofre_pergunta_e_apaga_o_item() {
    let pasta = Pasta::nova("cc-remover");
    let origem = arquivo_solto(
        &pasta,
        "id_ed25519",
        &exportada(&chave(), PrivateFormat::OpenSsh, None),
    );
    let mut m = na_pagina(&pasta);
    importar_arquivo(&mut m.app, &origem);
    m.app.click("chaves-importar-dialogo-primary");
    let id = id_da_chave(&pasta);

    m.app.click(&format!("chave-cofre-{id}-mais"));
    m.app.click(&format!("chave-cofre-{id}-remover"));
    assert!(m.app.is_open("chaves-remover-dialogo"));
    m.app.assert_lint();
    m.app.click("chaves-remover-dialogo-primary");

    assert!(pasta.cofre().chaves_ssh().is_empty());
    assert!(m.app.visible("chaves-cofre-vazio"));
}

/// Importa uma Ed25519 sem passphrase pela página e devolve o id do item no cofre.
fn importar_ed25519(m: &mut Montagem, pasta: &Pasta) -> u64 {
    let origem = arquivo_solto(
        pasta,
        "id_ed25519",
        &exportada(&chave(), PrivateFormat::OpenSsh, None),
    );
    importar_arquivo(&mut m.app, &origem);
    m.app.click("chaves-importar-dialogo-primary");
    id_da_chave(pasta)
}

#[test]
fn o_item_de_chave_ssh_no_cofre_mostra_os_dados_e_grava_a_politica() {
    let pasta = Pasta::nova("cc-item");
    let mut m = na_pagina(&pasta);
    let id = importar_ed25519(&mut m, &pasta);

    m.app.click(&format!("chave-cofre-{id}-mais"));
    m.app.click(&format!("chave-cofre-{id}-cofre"));
    assert!(m.app.visible("cofre-chave-fingerprint"));
    assert!(m.app.text("cofre-chave-fingerprint").starts_with("SHA256:"));
    assert!(m.app.text("cofre-chave-tipo").contains("Ed25519"));
    assert!(
        m.app
            .text("cofre-chave-publica")
            .starts_with("ssh-ed25519 ")
    );
    assert!(
        !format!("{:?}", m.app.tree()).contains("cofre-chave-privada"),
        "a privada fica escondida"
    );
    m.app.assert_lint();

    m.app.click("cofre-chave-mostrar");
    assert!(
        m.app
            .text("cofre-chave-privada")
            .contains("BEGIN OPENSSH PRIVATE KEY")
    );
    m.app.assert_lint();

    m.app.click("cofre-chave-confirmar");
    let cofre = pasta.cofre();
    assert!(cofre.item(id).unwrap().chave.as_ref().unwrap().confirmar);
    m.app.click("cofre-chave-agente");
    let cofre = pasta.cofre();
    assert!(!cofre.item(id).unwrap().chave.as_ref().unwrap().no_agente);

    m.app.click("cofre-chave-exportar");
    assert!(m.app.visible("chaves-pagina"));
    assert!(m.app.is_open("chaves-exportar-dialogo"));
}

#[test]
fn a_conexao_escolhe_uma_chave_do_cofre_pela_uid() {
    let pasta = Pasta::nova("cc-conexao");
    pasta.semear(&so_com_senha());
    let mut m = na_pagina(&pasta);
    let id = importar_ed25519(&mut m, &pasta);
    let uid = pasta.cofre().item(id).unwrap().uid.clone();

    m.app.click("navegacao-terminais");
    m.app.click("arvore-1");
    assert_eq!(valor(&m.app, "Chave do cofre"), "Nenhuma");
    escolher_propriedade(&mut m.app, "Chave do cofre");
    m.app.key(Key::F(4));
    assert!(m.app.is_open("props-Chave do cofre-valor-lista"));
    m.app.assert_lint();
    let opcao = format!("props-Chave do cofre-valor-ana-teste--{id}-");
    assert!(m.app.visible(&opcao));
    m.app.click(&opcao);

    assert_eq!(
        pasta
            .conexoes()
            .connection(1)
            .unwrap()
            .chave_do_cofre
            .as_deref(),
        Some(uid.as_str())
    );
    assert!(valor(&m.app, "Chave do cofre").starts_with("ana@teste ["));
}
