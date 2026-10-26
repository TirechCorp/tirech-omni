//! O cofre entre máquinas (M44), pela tela: "Levar o cofre…", "Abrir cofre de outra máquina…", a
//! mudança no disco, as cópias em conflito e o local do cofre.

mod comum;

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use tirech_omni::anexos;
use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::cofre::{Anexo, Cofre, Item, Tipo};
use tirech_omni::configuracoes::Configuracoes;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, item_ssh, montar_com_ambiente, montar_pura};

fn titulos(cofre: &Cofre) -> Vec<String> {
    let mut lista: Vec<String> = cofre.itens().iter().map(|i| i.titulo.clone()).collect();
    lista.sort();
    lista
}

fn gravada(pasta: &Pasta) -> Configuracoes {
    Configuracoes::carregar(&Armazenamento::new(pasta.caminho())).0
}

/// Passa quadros até `pronto` ser verdadeiro (as esperas do app voltam por threads).
fn esperar(app: &mut TestApp, o_que: &str, mut pronto: impl FnMut(&mut TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// A tela na página do cofre, com a pasta `pasta`.
fn na_pagina_do_cofre(pasta: &Pasta) -> TestApp {
    let mut m = montar_pura(pasta, Theme::Light);
    m.app.click("navegacao-cofre");
    m.app
}

/// Leva o cofre de `app` para `arquivo`, pelo menu "Mais".
fn levar(app: &mut TestApp, arquivo: &Path, senha: Option<&str>) {
    app.click("cofre-mais");
    app.click("cofre-levar");
    assert!(app.is_open("cofre-levar-dialogo"));
    if let Some(senha) = senha {
        app.type_into("cofre-levar-senha", senha);
    }
    app.file_dialog_answer(Some(arquivo.to_path_buf()));
    app.click("cofre-levar-dialogo-primary");
}

/// Escolhe `arquivo` em "Abrir cofre de outra máquina…".
fn abrir_de_fora(app: &mut TestApp, arquivo: &Path) {
    app.click("cofre-mais");
    app.click("cofre-abrir-outro");
    app.file_dialog_answer(Some(arquivo.to_path_buf()));
}

fn semear(nome: &str, itens: &[&str]) -> Pasta {
    let pasta = Pasta::nova(nome);
    pasta.semear_cofre(
        itens
            .iter()
            .map(|t| item_ssh(t, "root", "segredo"))
            .collect(),
    );
    pasta
}

#[test]
fn levar_o_cofre_salva_o_arquivo_e_abrir_num_app_novo_junta() {
    let a = semear("portatil-levar-a", &["srv-a", "srv-b"]);
    let b = semear("portatil-levar-b", &["so-do-b"]);
    let arquivo = a.caminho().join("cofre.tirech-vault");

    let mut app = na_pagina_do_cofre(&a);
    app.click("cofre-mais");
    app.click("cofre-levar");
    // Sem senha mestra e sem senha do arquivo, o diálogo diz o que isso significa.
    assert!(app.visible("cofre-levar-aviso"));
    assert!(
        app.text("cofre-levar-aviso")
            .contains("quem tiver o arquivo")
    );
    app.type_into("cofre-levar-senha", "x");
    assert!(!app.visible("cofre-levar-aviso"));
    app.key(tomni_ui::Key::Backspace);
    assert!(app.visible("cofre-levar-aviso"));
    app.file_dialog_answer(Some(arquivo.clone()));
    app.click("cofre-levar-dialogo-primary");

    let texto = std::fs::read_to_string(&arquivo).unwrap();
    assert!(texto.contains("\"formato\": \"tirech-vault\""), "{texto}");
    assert!(!texto.contains("srv-a"), "o arquivo é cifrado");
    assert!(app.text("aviso-sucesso").contains("Cofre salvo"));
    // O cofre daqui não mudou.
    assert_eq!(titulos(&a.cofre()), ["srv-a", "srv-b"]);

    let mut outro = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut outro, &arquivo);
    assert!(outro.is_open("cofre-abrir-dialogo"));
    assert!(outro.text("cofre-abrir-resumo").contains("2 itens"));
    assert!(outro.text("cofre-abrir-resumo").contains("0 chaves SSH"));
    outro.click("cofre-abrir-dialogo-primary");
    assert!(outro.text("aviso-sucesso").contains("2 novos"));
    assert_eq!(titulos(&b.cofre()), ["so-do-b", "srv-a", "srv-b"]);
}

#[test]
fn levar_com_senha_propria_pede_a_senha_ao_abrir() {
    let a = semear("portatil-senha-a", &["srv-a"]);
    let b = semear("portatil-senha-b", &["so-do-b"]);
    let arquivo = a.caminho().join("com-senha.tirech-vault");

    let mut app = na_pagina_do_cofre(&a);
    levar(&mut app, &arquivo, Some("a-senha-do-arquivo"));
    let texto = std::fs::read_to_string(&arquivo).unwrap();
    assert!(
        !texto.contains("\"chave\":"),
        "a chave não vai dentro: {texto}"
    );

    let mut outro = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut outro, &arquivo);
    assert!(outro.is_open("cofre-abrir-senha-dialogo"));
    outro.type_into("cofre-abrir-senha", "errada");
    outro.click("cofre-abrir-senha-dialogo-primary");
    // Com a senha errada o diálogo volta, com o aviso.
    esperar(&mut outro, "o diálogo da senha volta", |app| {
        app.is_open("cofre-abrir-senha-dialogo")
    });
    assert!(outro.visible("cofre-abrir-senha-erro"));
    outro.type_into("cofre-abrir-senha", "a-senha-do-arquivo");
    outro.click("cofre-abrir-senha-dialogo-primary");
    assert!(outro.is_open("cofre-abrir-dialogo"));
    outro.click("cofre-abrir-dialogo-primary");
    assert_eq!(titulos(&b.cofre()), ["so-do-b", "srv-a"]);
}

#[test]
fn usar_no_lugar_deste_guarda_o_cofre_de_agora_ao_lado() {
    let a = semear("portatil-lugar-a", &["srv-a"]);
    let b = semear("portatil-lugar-b", &["so-do-b"]);
    let arquivo = a.caminho().join("cofre.tirech-vault");

    let mut app = na_pagina_do_cofre(&a);
    levar(&mut app, &arquivo, None);

    let mut outro = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut outro, &arquivo);
    outro.click("cofre-abrir-dialogo-secondary");
    assert!(outro.is_open("cofre-abrir-substituir-dialogo"));
    assert!(
        outro
            .text("cofre-abrir-substituir-texto")
            .contains("vault.json.bak")
    );
    // Nada mudou antes de confirmar.
    assert_eq!(titulos(&b.cofre()), ["so-do-b"]);
    outro.click("cofre-abrir-substituir-dialogo-primary");
    assert_eq!(titulos(&b.cofre()), ["srv-a"]);
    let guardados: Vec<String> = std::fs::read_dir(b.caminho())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("vault.json.bak-"))
        .collect();
    assert_eq!(guardados.len(), 1, "{guardados:?}");
}

#[test]
fn cancelar_o_cofre_de_fora_nao_muda_nada() {
    let a = semear("portatil-cancela-a", &["srv-a"]);
    let b = semear("portatil-cancela-b", &["so-do-b"]);
    let arquivo = a.caminho().join("cofre.tirech-vault");
    let mut app = na_pagina_do_cofre(&a);
    levar(&mut app, &arquivo, None);

    let mut outro = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut outro, &arquivo);
    outro.click("cofre-abrir-dialogo-close");
    assert!(!outro.is_open("cofre-abrir-dialogo"));
    assert_eq!(titulos(&b.cofre()), ["so-do-b"]);
}

#[test]
fn um_arquivo_que_nao_e_cofre_vira_aviso() {
    let a = semear("portatil-invalido", &["srv-a"]);
    let lixo = a.caminho().join("lixo.json");
    std::fs::write(&lixo, "{\"outra\": \"coisa\"}").unwrap();
    let mut app = na_pagina_do_cofre(&a);
    abrir_de_fora(&mut app, &lixo);
    assert!(!app.is_open("cofre-abrir-dialogo"));
    assert!(app.text("aviso-erro").contains("não é um cofre"));
}

#[test]
fn so_o_vault_json_sem_a_chave_explica_o_que_falta() {
    let a = semear("portatil-sem-chave-a", &["srv-a"]);
    let b = semear("portatil-sem-chave-b", &["so-do-b"]);
    let sozinho = b.caminho().join("fora");
    std::fs::create_dir_all(&sozinho).unwrap();
    std::fs::copy(a.caminho().join("vault.json"), sozinho.join("vault.json")).unwrap();
    let mut app = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut app, &sozinho.join("vault.json"));
    assert!(app.text("aviso-erro").contains("vault.key"));
    assert_eq!(titulos(&b.cofre()), ["so-do-b"]);
}

#[test]
fn com_senha_mestra_o_arquivo_usa_a_do_cofre_e_bloqueado_nao_leva() {
    let a = semear("portatil-mestra", &["srv-a"]);
    a.cofre_com_senha_mestra("abc");
    let arquivo = a.caminho().join("cofre.tirech-vault");

    let mut m = montar_pura(&a, Theme::Light);
    // Bloqueado, "Levar" avisa em vez de abrir o diálogo.
    m.app.click("cofre-desbloquear-close");
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-cofre");
    m.app.click("config-acao-cofre_levar");
    assert!(!m.app.is_open("cofre-levar-dialogo"));
    assert!(m.app.text("aviso-atencao").contains("Desbloqueie"));

    m.app.click("navegacao-cofre");
    m.app.click("cofre-desbloquear-botao");
    m.app.type_into("cofre-pagina-desbloquear-campo", "abc");
    m.app.click("cofre-pagina-desbloquear-primary");
    m.app.click("cofre-mais");
    m.app.click("cofre-levar");
    assert!(m.app.is_open("cofre-levar-dialogo"));
    assert!(!m.app.visible("cofre-levar-senha"));
    assert!(!m.app.visible("cofre-levar-aviso"));
    assert!(m.app.visible("cofre-levar-com-mestra"));
    m.app.file_dialog_answer(Some(arquivo.clone()));
    m.app.click("cofre-levar-dialogo-primary");
    let texto = std::fs::read_to_string(&arquivo).unwrap();
    assert!(texto.contains("\"formato\": \"tirech-vault\""));
    assert!(
        !texto.contains("\"chave\":"),
        "a senha mestra protege: {texto}"
    );
}

/// Um `Cofre` na pasta `pasta`, aberto como outra máquina abriria, com mais um item.
fn gravar_de_outra_maquina(pasta: &Path, titulo: &str) {
    let (mut cofre, _) = Cofre::abrir(pasta);
    cofre
        .salvar(Item {
            tipo: Tipo::Ssh,
            titulo: titulo.to_owned(),
            ..Item::default()
        })
        .expect("gravar na pasta");
}

#[test]
fn a_mudanca_no_disco_entra_sozinha() {
    let a = semear("portatil-vigia", &["srv-a"]);
    let (mut m, _) = montar_com_ambiente(&a, Theme::Light, |amb| {
        amb.vigia_do_cofre = Duration::from_millis(100);
    });
    m.app.click("navegacao-cofre");
    gravar_de_outra_maquina(a.caminho(), "chegou-pela-sincronizacao");
    esperar(&mut m.app, "o item chegar", |app| {
        app.grid("cofre-lista")
            .rows
            .iter()
            .any(|l| l.cells[0] == "chegou-pela-sincronizacao")
    });
    assert!(m.app.text("aviso-info").contains("outra máquina"));
    // O que havia continua, e o disco tem os dois.
    assert_eq!(titulos(&a.cofre()), ["chegou-pela-sincronizacao", "srv-a"]);
}

#[test]
fn a_copia_em_conflito_vira_aviso_e_juntar_arquiva_a_copia() {
    let a = semear("portatil-conflito", &["srv-a"]);
    // A cópia que o Syncthing deixaria: o cofre de uma máquina que ganhou um item.
    let outra = a.caminho().join("outra-maquina");
    std::fs::create_dir_all(&outra).unwrap();
    for nome in ["vault.json", "vault.key"] {
        std::fs::copy(a.caminho().join(nome), outra.join(nome)).unwrap();
    }
    gravar_de_outra_maquina(&outra, "so-na-copia");
    let conflito = a
        .caminho()
        .join("vault.sync-conflict-20260101-120000-ABCDEFG.json");
    std::fs::copy(outra.join("vault.json"), &conflito).unwrap();

    let (mut m, _) = montar_com_ambiente(&a, Theme::Light, |amb| {
        amb.vigia_do_cofre = Duration::from_millis(100);
    });
    m.app.click("navegacao-cofre");
    esperar(&mut m.app, "o aviso da cópia", |app| {
        app.ui().find("cofre-conflito").is_some() && app.visible("cofre-conflito")
    });
    m.app.click("cofre-conflito-juntar");
    assert!(m.app.is_open("cofre-abrir-dialogo"));
    m.app.click("cofre-abrir-dialogo-primary");
    assert_eq!(titulos(&a.cofre()), ["so-na-copia", "srv-a"]);
    assert!(!conflito.exists(), "a cópia saiu do nome de conflito");
    assert!(PathBuf::from(format!("{}.juntado", conflito.display())).exists());
    esperar(&mut m.app, "o aviso sumir", |app| {
        !app.visible("cofre-conflito")
    });
}

#[test]
fn o_local_do_cofre_leva_e_volta() {
    let a = semear("portatil-local", &["srv-a"]);
    let destino = a.caminho().join("sincronizada");
    std::fs::create_dir_all(&destino).unwrap();

    let mut m = montar_pura(&a, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-cofre");
    m.app.click("config-acao-cofre_local-cabecalho");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("config-cofre-local-mudar");
    assert!(m.app.is_open("cofre-local-dialogo"));
    m.app.click("cofre-local-dialogo-primary");

    assert!(destino.join("vault.json").exists());
    assert!(destino.join("vault.key").exists());
    assert!(a.caminho().join("vault.json.bak").exists());
    assert!(!a.caminho().join("vault.json").exists());
    assert_eq!(
        gravada(&a).cofre.local,
        destino.display().to_string(),
        "o local fica na configuração"
    );

    // De volta à pasta de dados.
    m.app.click("config-cofre-local-padrao");
    assert!(m.app.is_open("cofre-local-dialogo"));
    m.app.click("cofre-local-dialogo-primary");
    assert!(a.caminho().join("vault.json").exists());
    assert_eq!(gravada(&a).cofre.local, "");
}

#[test]
fn o_local_com_um_cofre_la_pode_ser_usado_sem_levar() {
    let a = semear("portatil-local-existente", &["srv-a"]);
    let destino = a.caminho().join("ja-tem");
    std::fs::create_dir_all(&destino).unwrap();
    let (mut la, _) = Cofre::abrir(&destino);
    la.salvar(Item {
        tipo: Tipo::Ssh,
        titulo: "que-ja-estava-la".to_owned(),
        ..Item::default()
    })
    .unwrap();

    let mut m = montar_pura(&a, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-cofre");
    m.app.click("config-acao-cofre_local-cabecalho");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("config-cofre-local-mudar");
    assert!(m.app.text("cofre-local-aviso").contains("Já há um cofre"));
    m.app.click("cofre-local-dialogo-secondary");
    // O cofre daqui ficou como estava e o app passou a usar o de lá.
    assert_eq!(titulos(&a.cofre()), ["srv-a"]);
    assert_eq!(gravada(&a).cofre.local, destino.display().to_string());
    m.app.click("navegacao-cofre");
    assert!(
        m.app
            .grid("cofre-lista")
            .rows
            .iter()
            .any(|l| l.cells[0] == "que-ja-estava-la")
    );
}

#[test]
fn um_app_novo_abre_o_cofre_do_local_configurado() {
    let a = Pasta::nova("portatil-local-configurado");
    let destino = a.caminho().join("sincronizada");
    std::fs::create_dir_all(&destino).unwrap();
    let (mut la, _) = Cofre::abrir(&destino);
    la.salvar(Item {
        tipo: Tipo::Ssh,
        titulo: "guardado-la".to_owned(),
        ..Item::default()
    })
    .unwrap();
    let mut config = Configuracoes::default();
    config.geral.pagina_inicial = tirech_omni::configuracoes::PaginaInicial::Terminais;
    config.cofre.local = destino.display().to_string();
    config.salvar(&a.armazenamento()).unwrap();

    let mut m = montar_pura(&a, Theme::Light);
    m.app.click("navegacao-cofre");
    assert!(
        m.app
            .grid("cofre-lista")
            .rows
            .iter()
            .any(|l| l.cells[0] == "guardado-la")
    );
    // O app não criou um cofre na pasta de dados.
    assert!(!a.caminho().join("vault.json").exists());
}

/// Grava uma nota `titulo` com um anexo de `bytes` na pasta `pasta` (cifrado em `vault/`).
fn semear_com_anexo(pasta: &Pasta, titulo: &str, bytes: &[u8]) -> Anexo {
    let (mut cofre, _) = Cofre::abrir(pasta.caminho());
    let nota = cofre
        .salvar(Item {
            tipo: Tipo::Nota,
            titulo: titulo.to_owned(),
            ..Item::default()
        })
        .expect("gravar a nota");
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
    ficha.nome = "dados.bin".to_owned();
    ficha.tipo = "application/octet-stream".to_owned();
    cofre.anexar(nota, ficha.clone()).expect("anexar");
    ficha
}

/// O conteúdo decifrado do anexo `ficha` no `vault/` de `pasta`, se o arquivo existe lá.
fn ler_anexo(pasta: &Pasta, ficha: &Anexo) -> Option<Vec<u8>> {
    anexos::ler_para_memoria(
        &pasta.caminho().join(anexos::PASTA),
        ficha,
        &AtomicBool::new(false),
    )
    .ok()
}

/// Passa quadros até o diálogo da cópia dos anexos fechar.
fn esperar_a_copia(app: &mut TestApp) {
    esperar(app, "a cópia dos anexos acabar", |a| {
        !a.is_open("cofre-copia-dialogo")
    });
}

#[test]
fn levar_para_outra_maquina_copia_os_anexos_ao_lado_e_abrir_traz_os_arquivos() {
    let a = Pasta::nova("portatil-anexos-a");
    let b = semear("portatil-anexos-b", &["so-do-b"]);
    let dados: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let ficha = semear_com_anexo(&a, "Roteiro", &dados);
    let arquivo = a.caminho().join("cofre.tirech-vault");
    let pasta_dos_anexos = a.caminho().join("cofre.vault");

    let mut app = na_pagina_do_cofre(&a);
    app.click("cofre-mais");
    app.click("cofre-levar");
    assert!(app.visible("cofre-levar-anexos"));
    assert_eq!(
        app.text("cofre-levar-anexos"),
        "Leve também a pasta <nome>.vault, que fica ao lado do arquivo."
    );
    app.file_dialog_answer(Some(arquivo.clone()));
    app.click("cofre-levar-dialogo-primary");
    esperar_a_copia(&mut app);
    esperar(&mut app, "o aviso", |a| a.visible("aviso-sucesso"));
    assert!(app.text("aviso-sucesso").contains("cofre.vault"));
    // Os arquivos vão como estão: cifrados, iguais aos de `vault/`.
    assert_eq!(
        std::fs::read(pasta_dos_anexos.join(&ficha.id)).unwrap(),
        std::fs::read(a.caminho().join("vault").join(&ficha.id)).unwrap()
    );
    assert!(
        !std::fs::read(pasta_dos_anexos.join(&ficha.id))
            .unwrap()
            .windows(64)
            .any(|w| w == &dados[1000..1064])
    );

    let mut outro = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut outro, &arquivo);
    assert!(outro.is_open("cofre-abrir-dialogo"));
    outro.click("cofre-abrir-dialogo-primary");
    esperar_a_copia(&mut outro);
    esperar(&mut outro, "o aviso", |a| a.visible("aviso-sucesso"));
    assert!(!outro.text("aviso-sucesso").contains("não vieram"));
    assert_eq!(titulos(&b.cofre()), ["Roteiro", "so-do-b"]);
    assert_eq!(ler_anexo(&b, &ficha).as_deref(), Some(&dados[..]));
}

#[test]
fn sem_a_pasta_dos_anexos_o_cofre_abre_e_o_aviso_diz_quantos_faltam() {
    let a = Pasta::nova("portatil-anexos-sem-a");
    let b = semear("portatil-anexos-sem-b", &["so-do-b"]);
    let ficha = semear_com_anexo(&a, "Roteiro", b"abacaxi-de-teste");
    let arquivo = a.caminho().join("cofre.tirech-vault");
    let mut app = na_pagina_do_cofre(&a);
    levar(&mut app, &arquivo, None);
    esperar_a_copia(&mut app);
    let pasta = a.caminho().join("cofre.vault");
    let reserva = a.caminho().join("reserva-dos-anexos");
    std::fs::rename(&pasta, &reserva).unwrap();

    let mut outro = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut outro, &arquivo);
    outro.click("cofre-abrir-dialogo-primary");
    let aviso = outro.text("aviso-atencao");
    assert!(aviso.contains("1 anexo não veio junto"), "{aviso}");
    assert!(
        aviso.contains("Copie a pasta cofre.vault para o lado do arquivo"),
        "{aviso}"
    );
    assert_eq!(titulos(&b.cofre()), ["Roteiro", "so-do-b"]);
    assert_eq!(ler_anexo(&b, &ficha), None);

    // Com a pasta de volta, abrir de novo traz o arquivo.
    std::fs::rename(&reserva, &pasta).unwrap();
    abrir_de_fora(&mut outro, &arquivo);
    outro.click("cofre-abrir-dialogo-primary");
    esperar_a_copia(&mut outro);
    assert_eq!(
        ler_anexo(&b, &ficha).as_deref(),
        Some(&b"abacaxi-de-teste"[..])
    );
    assert!(!outro.text("aviso-sucesso").contains("não veio"));
}

#[test]
fn usar_no_lugar_deste_traz_os_anexos_e_nao_apaga_o_vault_daqui() {
    let a = Pasta::nova("portatil-anexos-lugar-a");
    let b = Pasta::nova("portatil-anexos-lugar-b");
    let de_a = semear_com_anexo(&a, "da-a", b"so-na-a");
    let de_b = semear_com_anexo(&b, "da-b", b"so-na-b");
    // O `vault.json` e o `vault.key` de A, com o `vault/` ao lado, como uma pasta sincronizada.
    let mut app = na_pagina_do_cofre(&b);
    abrir_de_fora(&mut app, &a.caminho().join("vault.json"));
    app.click("cofre-abrir-dialogo-secondary");
    app.click("cofre-abrir-substituir-dialogo-primary");
    esperar_a_copia(&mut app);
    assert_eq!(titulos(&b.cofre()), ["da-a"]);
    assert_eq!(ler_anexo(&b, &de_a).as_deref(), Some(&b"so-na-a"[..]));
    assert_eq!(
        ler_anexo(&b, &de_b).as_deref(),
        Some(&b"so-na-b"[..]),
        "o vault/ daqui fica: a coleta limpa depois da margem"
    );
}

#[test]
fn levar_o_cofre_para_o_local_copia_o_vault_e_deixa_o_antigo_como_vault_bak() {
    let a = Pasta::nova("portatil-anexos-local");
    let ficha = semear_com_anexo(&a, "Roteiro", b"abacaxi-de-teste");
    let destino = a.caminho().join("sincronizada");

    std::fs::create_dir_all(&destino).unwrap();

    let mut m = montar_pura(&a, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-cofre");
    m.app.click("config-acao-cofre_local-cabecalho");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("config-cofre-local-mudar");
    assert!(m.app.is_open("cofre-local-dialogo"));
    m.app.click("cofre-local-dialogo-primary");
    let app = &mut m.app;
    esperar_a_copia(app);
    assert!(destino.join("vault.json").exists());
    assert_eq!(
        std::fs::read(destino.join("vault").join(&ficha.id)).unwrap(),
        std::fs::read(a.caminho().join("vault.bak").join(&ficha.id)).unwrap()
    );
    assert!(
        !a.caminho().join("vault").exists(),
        "o antigo virou vault.bak"
    );
}
