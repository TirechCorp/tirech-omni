//! A página Notas (M47): árvore, editor, prévia, busca, anexos e as ligações com o cofre.

mod comum;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tirech_omni::anexos;
use tirech_omni::cofre::{Anexo, Item, Tipo};
use tomni_ui::prelude::*;
use tomni_ui::theme::Theme;
use tomni_ui::{ClipboardImage, Key};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, item_ssh, montar, valor};

fn nota(titulo: &str, corpo: &str) -> Item {
    Item {
        tipo: Tipo::Nota,
        titulo: titulo.into(),
        notas: corpo.into(),
        ..Item::default()
    }
}

/// A tela na página Notas, com o cofre da pasta.
fn abrir_notas(pasta: &Pasta, tema: Theme) -> Montagem {
    let mut m = montar(pasta, tema);
    m.app.shortcut(Shortcut::ctrl_shift('J'));
    assert_eq!(m.app.text("navegacao"), "notas");
    m
}

/// O texto da primeira nota do cofre em disco com `titulo`.
fn corpo_salvo(pasta: &Pasta, titulo: &str) -> Option<String> {
    pasta
        .cofre()
        .itens()
        .iter()
        .find(|i| i.titulo == titulo)
        .map(|i| i.notas.clone())
}

/// Os ids de todos os widgets da tela, para depurar.
fn ids(app: &TestApp) -> Vec<String> {
    fn anda(n: &tomni_ui::Node, saida: &mut Vec<String>) {
        if let Some(id) = &n.id {
            saida.push(format!("{id} [{}]", n.kind));
        }
        for f in &n.children {
            anda(f, saida);
        }
    }
    let mut v = Vec::new();
    anda(&app.tree(), &mut v);
    v
}

/// Troca o título da nota aberta.
fn titular(app: &mut TestApp, titulo: &str) {
    app.click("notas-titulo").shortcut('a').type_text(titulo);
}

/// Deixa o salvamento automático (150 ms nos testes) vencer: o alarme dorme numa thread de verdade.
fn esperar(app: &mut TestApp) {
    std::thread::sleep(Duration::from_millis(400));
    app.wait(Duration::from_millis(50));
}

#[test]
fn nova_nota_salva_sozinha_e_volta_ao_reabrir() {
    let pasta = Pasta::nova("notas-nova");
    {
        let mut m = abrir_notas(&pasta, Theme::Light);
        assert!(m.app.visible("notas-vazio"), "sem notas, o convite aparece");
        m.app.click("notas-vazio-nova");
        assert!(m.app.visible("notas-editor"));
        m.app.type_into("notas-editor", "Meu abacaxi-de-teste");
        esperar(&mut m.app);
        assert_eq!(
            m.app.text("notas-status").split(" · ").next(),
            Some("Salvo")
        );
        let salvas = pasta.cofre().itens().len();
        assert_eq!(salvas, 1);
    }
    // Reabrir o app no mesmo diretório.
    let mut m = abrir_notas(&pasta, Theme::Light);
    m.app.click("notas-arvore-nota-1");
    assert_eq!(
        &*m.app.text_editor("notas-editor").text,
        "Meu abacaxi-de-teste"
    );
}

#[test]
fn o_cofre_em_disco_nao_guarda_a_nota_em_claro() {
    let pasta = Pasta::nova("notas-claro");
    let mut m = abrir_notas(&pasta, Theme::Light);
    m.app.click("notas-vazio-nova");
    titular(&mut m.app, "Cartão-secreto");
    m.app.type_into("notas-editor", "abacaxi-de-teste");
    esperar(&mut m.app);
    m.app.shortcut(Shortcut::ctrl('S'));
    let disco = pasta.texto_cofre();
    assert!(!disco.is_empty());
    assert!(!disco.contains("abacaxi-de-teste"), "o corpo vazou");
    assert!(!disco.contains("Cartão-secreto"), "o título vazou");
    assert_eq!(
        corpo_salvo(&pasta, "Cartão-secreto").as_deref(),
        Some("abacaxi-de-teste")
    );
}

/// Abre a nota `titulo` pela árvore (a única com esse título).
fn selecionar(app: &mut TestApp, id: u64) {
    app.click(&format!("notas-arvore-nota-{id}"));
}

/// Uma nota semeada, aberta no editor.
fn com_nota(nome: &str, corpo: &str) -> (Pasta, Montagem, u64) {
    let pasta = Pasta::nova(nome);
    let ids = pasta.semear_cofre(vec![nota("Anotação", corpo)]);
    let mut m = abrir_notas(&pasta, Theme::Light);
    selecionar(&mut m.app, ids[0]);
    (pasta, m, ids[0])
}

fn texto_do_editor(app: &TestApp) -> String {
    app.text_editor("notas-editor").text.to_string()
}

#[test]
fn a_barra_e_os_atalhos_formatam_o_texto() {
    let (_pasta, mut m, _) = com_nota("notas-barra", "");
    m.app.type_into("notas-editor", "palavra");
    m.app.shortcut('a').shortcut('b');
    assert_eq!(texto_do_editor(&m.app), "**palavra**");

    // O botão de tarefa em duas linhas.
    m.app.shortcut('a').type_text("um\ndois");
    m.app.shortcut('a');
    m.app.click("notas-tarefa");
    assert_eq!(texto_do_editor(&m.app), "- [ ] um\n- [ ] dois");

    // Enter numa lista continua; Tab recua.
    m.app.shortcut('a').type_text("- a\nb");
    assert_eq!(texto_do_editor(&m.app), "- a\n- b");
    m.app.key(Key::Tab);
    assert_eq!(texto_do_editor(&m.app), "- a\n  - b");
}

#[test]
fn clicar_numa_tarefa_da_previa_marca_no_editor_e_ctrl_z_desfaz() {
    let (_pasta, mut m, _) = com_nota("notas-tarefa", "- [ ] pagar\n- [ ] ligar");
    assert!(m.app.visible("notas-previa"));
    m.app.click("notas-previa-tarefa-1");
    assert_eq!(texto_do_editor(&m.app), "- [ ] pagar\n- [x] ligar");
    m.app.click("notas-editor");
    m.app.shortcut('z');
    assert_eq!(texto_do_editor(&m.app), "- [ ] pagar\n- [ ] ligar");
}

#[test]
fn bloco_de_segredo_fica_mascarado_e_a_copia_limpa_depois() {
    let pasta = Pasta::nova("notas-segredo");
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    std::fs::write(
        pasta.armazenamento().arquivo_configuracoes(),
        r#"{"cofre":{"limpar_area_transferencia_secs":1}}"#,
    )
    .unwrap();
    let semeadas = pasta.semear_cofre(vec![nota(
        "Acesso",
        "Antes\n\n```segredo\nsenha-do-banco\n```\n\nDepois",
    )]);
    let mut m = abrir_notas(&pasta, Theme::Light);
    selecionar(&mut m.app, semeadas[0]);
    assert!(m.app.visible("notas-previa-bloco-0-mascara"));
    assert!(
        !ids(&m.app)
            .iter()
            .any(|i| i.contains("notas-previa-bloco-0-texto")),
        "o segredo não entra na tela antes de Mostrar"
    );
    m.app.click("notas-previa-bloco-0-mostrar");
    assert_eq!(m.app.text("notas-previa-bloco-0-texto"), "senha-do-banco");
    m.app.click("notas-previa-bloco-0-copiar");
    assert_eq!(m.app.clipboard().read().as_deref(), Some("senha-do-banco"));
    std::thread::sleep(Duration::from_millis(1300));
    // O alarme roda num quadro e a área de transferência esvazia no seguinte.
    m.app
        .wait(Duration::from_millis(50))
        .wait(Duration::from_millis(50));
    assert_eq!(
        m.app.clipboard().read().unwrap_or_default(),
        "",
        "a área de transferência esvazia"
    );

    // Ctrl+C no editor também passa pelo cofre e limpa depois do tempo.
    m.app.click("notas-editor");
    m.app.shortcut('a').shortcut('c');
    assert!(m.app.clipboard().read().unwrap().contains("senha-do-banco"));
    std::thread::sleep(Duration::from_millis(1300));
    // O alarme roda num quadro e a área de transferência esvazia no seguinte.
    m.app
        .wait(Duration::from_millis(50))
        .wait(Duration::from_millis(50));
    assert_eq!(m.app.clipboard().read().unwrap_or_default(), "");
}

#[test]
fn bloquear_com_salvamento_pendente_grava_antes_e_destravar_volta_na_nota() {
    let pasta = Pasta::nova("notas-bloqueio");
    let ids = pasta.semear_cofre(vec![nota("Anotação", "")]);
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    m.app.shortcut(Shortcut::ctrl_shift('J'));
    selecionar(&mut m.app, ids[0]);
    m.app.type_into("notas-editor", "texto pendente");
    // Sem esperar o alarme: o bloqueio grava primeiro.
    m.app.click("navegacao-cofre").click("cofre-bloquear");
    m.app.click("navegacao-notas");
    assert!(m.app.visible("notas-bloqueado"));

    // O cofre em disco tem o texto (abrir de novo com a senha o mostra).
    let (mut copia, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    assert!(copia.desbloquear("abc"));
    assert_eq!(copia.item(ids[0]).unwrap().notas, "texto pendente");

    m.app.click("notas-desbloquear-botao");
    m.app.type_into("notas-desbloquear-campo", "abc");
    m.app.click("notas-desbloquear-primary");
    assert!(!m.app.visible("notas-bloqueado"));
    assert_eq!(texto_do_editor(&m.app), "texto pendente");
}

#[test]
fn buscar_e_substituir_na_nota_com_um_ctrl_z_so() {
    let original = "ação um\nação dois\nAÇÃO três";
    let (_pasta, mut m, _) = com_nota("notas-busca", original);
    m.app.click("notas-editor");
    m.app.shortcut('f');
    assert!(m.app.visible("notas-busca-barra"));
    m.app.type_text("acao");
    assert_eq!(m.app.text("notas-busca-contagem"), "1 de 3");
    m.app.key(Key::F(3));
    assert_eq!(m.app.text("notas-busca-contagem"), "2 de 3");
    m.app.key_with(Key::F(3), tomni_ui::Modifiers::SHIFT);
    assert_eq!(m.app.text("notas-busca-contagem"), "1 de 3");

    m.app.shortcut('h');
    assert!(m.app.visible("notas-substituir"));
    m.app.type_into("notas-substituir", "x");
    m.app.click("notas-substituir-todos");
    assert_eq!(texto_do_editor(&m.app), "x um\nx dois\nx três");
    m.app.click("notas-editor");
    m.app.shortcut('z');
    assert_eq!(texto_do_editor(&m.app), original);
}

#[test]
fn o_filtro_acha_pelo_corpo_sem_acento_e_abrir_o_resultado_seleciona_o_acerto() {
    let pasta = Pasta::nova("notas-filtro");
    let ids = pasta.semear_cofre(vec![
        nota("Receita", "Misture o açúcar com a farinha."),
        nota("Outra", "Nada a ver"),
    ]);
    let mut m = abrir_notas(&pasta, Theme::Light);
    m.app.type_into("notas-filtro", "acucar");
    let linhas = m.app.grid("notas-resultados").rows;
    assert_eq!(linhas.len(), 1);
    assert_eq!(linhas[0].key, format!("nota-{}", ids[0]));
    assert_eq!(linhas[0].cells[0], "Receita");
    assert!(
        linhas[0].cells[2].contains("açúcar"),
        "{:?}",
        linhas[0].cells
    );

    m.app.grid_select("notas-resultados", &[&linhas[0].key]);
    assert_eq!(texto_do_editor(&m.app), "Misture o açúcar com a farinha.");
    assert!(m.app.visible("notas-busca-barra"));
    assert_eq!(m.app.text("notas-busca-contagem"), "1 de 1");
}

#[test]
fn a_arvore_esconde_conexoes_so_com_senhas_move_renomeia_e_apaga_com_confirmacao() {
    let pasta = Pasta::nova("notas-arvore");
    let ids = pasta.semear_cofre_em(vec![
        ("Conexões", item_ssh("srv", "ana", "x")),
        ("Clientes", nota("Contrato", "texto")),
        ("Outra", nota("Solta", "texto")),
    ]);
    let mut m = abrir_notas(&pasta, Theme::Light);
    for nome in ["Clientes", "Outra"] {
        m.app.expand(
            "notas-arvore",
            &format!("pasta-{}", pasta.id_da_pasta(nome)),
        );
    }
    let chaves: Vec<String> = m
        .app
        .tree_items("notas-arvore")
        .into_iter()
        .map(|l| l.key)
        .collect();
    let conexoes = pasta.id_da_pasta("Conexões");
    let clientes = pasta.id_da_pasta("Clientes");
    let outra = pasta.id_da_pasta("Outra");
    assert!(!chaves.contains(&format!("pasta-{conexoes}")), "{chaves:?}");
    assert!(chaves.contains(&format!("pasta-{clientes}")));

    // Arrastar a nota solta para a pasta Clientes.
    m.app.drag(
        format!("notas-arvore-nota-{}", ids[2]).as_str(),
        format!("notas-arvore-pasta-{clientes}").as_str(),
    );
    assert_eq!(pasta.cofre().item(ids[2]).unwrap().pasta, Some(clientes));
    let _ = outra;

    // F2 renomeia.
    selecionar(&mut m.app, ids[1]);
    m.app.key(Key::F(2));
    m.app.shortcut('a').type_text("Contrato novo");
    m.app.key(Key::Enter);
    assert_eq!(pasta.cofre().item(ids[1]).unwrap().titulo, "Contrato novo");

    // Delete pede confirmação; Cancelar não apaga; Apagar apaga.
    selecionar(&mut m.app, ids[1]);
    m.app.key(Key::Delete);
    assert!(m.app.is_open("notas-apagar-dialogo"));
    m.app.click("notas-apagar-dialogo-close");
    assert!(pasta.cofre().item(ids[1]).is_some());
    m.app.key(Key::Delete);
    m.app.click("notas-apagar-dialogo-primary");
    assert!(pasta.cofre().item(ids[1]).is_none());
}

// ---------------------------------------------------------------------------------------------
// Anexos (fatia 7)
// ---------------------------------------------------------------------------------------------

/// Espera a fila de operações de anexo esvaziar (as threads gravam e leem de verdade).
fn esperar_fila(app: &mut TestApp) {
    for _ in 0..400 {
        if !app.visible("notas-anexo-fila") {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
        app.wait(Duration::from_millis(20));
    }
    panic!("a fila de anexos não esvaziou");
}

/// Roda quadros até `pronto` valer (as respostas dos diálogos e as threads chegam em quadros
/// seguintes).
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

/// Se há um widget com esse id na tela.
fn existe(app: &TestApp, id: &str) -> bool {
    ids(app).iter().any(|i| i.split(' ').next() == Some(id))
}

/// Bytes variados (não comprimíveis à primeira vista), de `tamanho` bytes.
fn bytes_de(tamanho: usize) -> Vec<u8> {
    (0..tamanho)
        .map(|i| u8::try_from((i * 31 + i / 251) % 256).unwrap_or(0))
        .collect()
}

/// As fichas de anexo da nota `id` no cofre em disco.
fn fichas(pasta: &Pasta, id: u64) -> Vec<Anexo> {
    pasta
        .cofre()
        .item(id)
        .expect("a nota está no cofre")
        .anexos
        .clone()
}

/// O conteúdo decifrado de um anexo do `vault/` da pasta.
fn conteudo(pasta: &Pasta, ficha: &Anexo) -> Vec<u8> {
    anexos::ler_para_memoria(
        &pasta.caminho().join("vault"),
        ficha,
        &AtomicBool::new(false),
    )
    .expect("o anexo decifra")
}

/// Anexa `arquivo` pelo botão da barra (o seletor responde `arquivo`) e espera o anexo `n`
/// aparecer na lista, com a fila vazia.
fn anexar_pelo_botao(app: &mut TestApp, arquivo: &Path, n: usize) {
    app.file_dialog_answer(Some(arquivo.to_path_buf()));
    app.click("notas-anexar");
    let id = format!("notas-anexo-{n}");
    esperar_ate(app, "o anexo na lista", |a| {
        existe(a, &id) && !a.visible("notas-anexo-fila")
    });
}

/// Escreve `bytes` em `nome`, dentro da pasta do teste.
fn arquivo_de(pasta: &Pasta, nome: &str, bytes: &[u8]) -> PathBuf {
    let caminho = pasta.caminho().join(nome);
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    std::fs::write(&caminho, bytes).unwrap();
    caminho
}

#[test]
fn anexar_cifra_o_arquivo_e_salvar_como_devolve_os_mesmos_bytes() {
    let (pasta, mut m, id) = com_nota("notas-anexar", "texto");
    let pequeno = bytes_de(2048);
    let origem = arquivo_de(&pasta, "relatorio.bin", &pequeno);
    anexar_pelo_botao(&mut m.app, &origem, 0);
    assert!(m.app.visible("notas-anexos"));
    assert_eq!(m.app.label("notas-anexo-0"), "relatorio.bin · 2,0 KB");
    let ficha = fichas(&pasta, id).remove(0);
    assert_eq!(ficha.nome, "relatorio.bin");
    assert_eq!(ficha.tamanho, 2048);
    let guardado = std::fs::read(pasta.caminho().join("vault").join(&ficha.id)).unwrap();
    assert!(
        guardado.len() > 2048,
        "o arquivo guardado leva a etiqueta do GCM"
    );
    assert!(
        !guardado
            .windows(64)
            .any(|j| pequeno.windows(64).any(|p| p == j)),
        "os bytes não podem estar em claro"
    );
    assert_eq!(conteudo(&pasta, &ficha), pequeno);

    // Salvar como… grava os mesmos bytes.
    let destino = pasta.caminho().join("saida.bin");
    m.app.click("notas-anexo-0").click("notas-anexo-0-salvar");
    m.app.file_dialog_answer(Some(destino.clone()));
    esperar_ate(&mut m.app, "o arquivo salvo", |a| {
        a.visible("notas-info") && !a.visible("notas-anexo-fila")
    });
    assert_eq!(std::fs::read(&destino).unwrap(), pequeno);
    assert!(m.app.text("notas-info").contains("Anexo salvo em"));
}

#[test]
fn se_o_cofre_nao_grava_a_ficha_o_arquivo_cifrado_e_apagado_e_o_erro_aparece() {
    let (pasta, mut m, _) = com_nota("notas-anexar-falha", "texto");
    // Com `vault.json` virando uma pasta, nenhum salvar do cofre dá certo.
    let arquivo_do_cofre = pasta.caminho().join("vault.json");
    std::fs::remove_file(&arquivo_do_cofre).unwrap();
    std::fs::create_dir(&arquivo_do_cofre).unwrap();
    let origem = arquivo_de(&pasta, "relatorio.bin", &bytes_de(2048));
    m.app.file_dialog_answer(Some(origem));
    m.app.click("notas-anexar");
    esperar_ate(&mut m.app, "o erro", |a| a.visible("notas-erro"));
    assert!(
        m.app
            .text("notas-erro")
            .contains("Não foi possível anexar relatorio.bin"),
        "{}",
        m.app.text("notas-erro")
    );
    esperar_ate(&mut m.app, "a fila vazia", |a| {
        !a.visible("notas-anexo-fila")
    });
    let sobras = std::fs::read_dir(pasta.caminho().join("vault"))
        .map(|d| d.count())
        .unwrap_or(0);
    assert_eq!(sobras, 0, "o arquivo cifrado não fica sem dono");
    assert!(!existe(&m.app, "notas-anexo-0"));
}

#[test]
fn um_anexo_de_3_mib_passa_pelos_blocos_e_volta_igual() {
    let (pasta, mut m, id) = com_nota("notas-anexar-grande", "texto");
    let grande = bytes_de(3 * 1024 * 1024 + 17);
    anexar_pelo_botao(&mut m.app, &arquivo_de(&pasta, "grande.bin", &grande), 0);
    let ficha = fichas(&pasta, id).remove(0);
    assert_eq!(ficha.tamanho, grande.len() as u64);
    assert_eq!(conteudo(&pasta, &ficha), grande);
    assert!(!m.app.visible("notas-anexo-fila"));
}

#[test]
fn anexo_sem_arquivo_na_maquina_avisa_e_volta_ao_normal_quando_o_arquivo_chega() {
    let (pasta, mut m, id) = com_nota("notas-anexo-ausente", "texto");
    let origem = arquivo_de(&pasta, "a.txt", b"conteudo do anexo");
    anexar_pelo_botao(&mut m.app, &origem, 0);
    let ficha = fichas(&pasta, id).remove(0);
    let vault = pasta.caminho().join("vault");
    let guardado = vault.join(&ficha.id);
    let reserva = pasta.caminho().join("reserva");
    std::fs::rename(&guardado, &reserva).unwrap();

    // Trocar de nota e voltar refaz a lista de anexos.
    m.app.type_into("notas-editor", " mais");
    esperar(&mut m.app);
    assert_eq!(
        m.app.label("notas-anexo-0"),
        "a.txt · não está nesta máquina"
    );
    m.app.click("notas-anexo-0");
    assert!(!m.app.visible("notas-anexo-0-salvar"));
    assert!(!m.app.visible("notas-anexo-0-copiar"));
    assert!(m.app.visible("notas-anexo-0-renomear"));
    m.app.key(Key::Escape);

    std::fs::rename(&reserva, &guardado).unwrap();
    m.app.type_into("notas-editor", " e mais");
    esperar(&mut m.app);
    assert_eq!(m.app.label("notas-anexo-0"), "a.txt · 17 B");
    m.app.click("notas-anexo-0");
    assert!(m.app.visible("notas-anexo-0-salvar"));
    assert!(m.app.visible("notas-anexo-0-copiar"));
}

#[test]
fn colar_imagem_cria_o_anexo_png_e_o_link_no_cursor_e_texto_cola_texto() {
    let (pasta, mut m, id) = com_nota("notas-colar-imagem", "antes");
    let imagem = ClipboardImage {
        width: 2,
        height: 2,
        rgba: vec![
            255, 0, 0, 255, 0, 255, 0, 255, //
            0, 0, 255, 255, 255, 255, 255, 128,
        ],
    };
    m.app.click("notas-editor");
    m.app.clipboard().write_image(imagem.clone());
    m.app.shortcut('v');
    esperar_fila(&mut m.app);
    let texto = texto_do_editor(&m.app);
    let ficha = fichas(&pasta, id).remove(0);
    assert!(
        ficha.nome.starts_with("colado-") && ficha.nome.ends_with(".png"),
        "{}",
        ficha.nome
    );
    assert_eq!(ficha.tipo, "image/png");
    assert!(
        texto.contains(&format!("![{}](anexo:{})", ficha.nome, ficha.id)),
        "{texto}"
    );
    assert_eq!(conteudo(&pasta, &ficha), imagem.to_png().unwrap());
    assert!(m.app.visible("notas-anexo-0"));

    // A pré-visualização mostra a imagem (decifrada numa thread).
    m.app.click("notas-modo-dividir");
    esperar_ate(&mut m.app, "a imagem na prévia", |a| {
        existe(a, "notas-previa-imagem-0")
    });

    // Com texto na área de transferência, Ctrl+V cola texto e não cria anexo.
    m.app.clipboard().write("só texto".to_owned());
    m.app.click("notas-editor");
    m.app.shortcut('v');
    esperar_fila(&mut m.app);
    assert!(texto_do_editor(&m.app).contains("só texto"));
    assert_eq!(fichas(&pasta, id).len(), 1);
}

#[test]
fn soltar_arquivos_no_editor_poe_um_link_por_linha_e_na_previa_poe_no_fim() {
    let (pasta, mut m, id) = com_nota("notas-soltar", "começo");
    let png = ClipboardImage {
        width: 1,
        height: 1,
        rgba: vec![1, 2, 3, 255],
    }
    .to_png()
    .unwrap();
    let a = arquivo_de(&pasta, "a.png", &png);
    let b = arquivo_de(&pasta, "b.txt", b"bbb");
    m.app.drop_files("notas-editor", &[a, b]);
    esperar_fila(&mut m.app);
    let texto = texto_do_editor(&m.app);
    let fichas_ = fichas(&pasta, id);
    assert_eq!(fichas_.len(), 2);
    let ficha = |nome: &str| fichas_.iter().find(|f| f.nome == nome).unwrap();
    let (fa, fb) = (ficha("a.png"), ficha("b.txt"));
    let la = format!("![a.png](anexo:{})", fa.id);
    let lb = format!("[b.txt](anexo:{})", fb.id);
    let (pa, pb) = (texto.find(&la), texto.find(&lb));
    assert!(pa.is_some() && pb.is_some(), "{texto}");
    assert!(pa < pb, "a ordem dos arquivos soltos se mantém: {texto}");
    assert!(
        texto[pa.unwrap()..pb.unwrap()].contains('\n'),
        "um por linha: {texto}"
    );

    // Na prévia, o link vai para o fim do texto.
    let c = arquivo_de(&pasta, "c.png", &png);
    m.app.click("notas-modo-dividir");
    m.app.drop_files("notas-previa", &[c]);
    esperar_fila(&mut m.app);
    let texto = texto_do_editor(&m.app);
    let ficha_c = fichas(&pasta, id)
        .into_iter()
        .find(|f| f.nome == "c.png")
        .unwrap();
    assert!(
        texto
            .trim_end()
            .ends_with(&format!("![c.png](anexo:{})", ficha_c.id)),
        "{texto}"
    );
}

#[test]
fn soltar_pasta_mostra_o_erro_e_nao_cria_anexo() {
    let (pasta, mut m, id) = com_nota("notas-soltar-pasta", "texto");
    let dir = pasta.caminho().join("uma-pasta");
    std::fs::create_dir_all(&dir).unwrap();
    m.app.drop_files("notas-editor", &[dir]);
    esperar_fila(&mut m.app);
    assert!(m.app.visible("notas-erro"));
    assert!(
        m.app
            .text("notas-erro")
            .contains("Pastas não podem ser anexadas")
    );
    assert_eq!(texto_do_editor(&m.app), "texto");
    esperar(&mut m.app);
    assert!(fichas(&pasta, id).is_empty());
}

#[test]
fn cancelar_o_anexo_em_andamento_tira_o_link_e_nao_deixa_arquivo() {
    let (pasta, mut m, id) = com_nota("notas-cancelar", "texto");
    let enorme = pasta.caminho().join("enorme.bin");
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    std::fs::File::create(&enorme)
        .unwrap()
        .set_len(300 * 1024 * 1024)
        .unwrap();
    m.app.drop_files("notas-editor", &[enorme]);
    assert!(m.app.visible("notas-anexo-fila"));
    assert!(
        texto_do_editor(&m.app).contains("anexo:"),
        "o link entra na hora"
    );
    m.app.click("notas-anexo-cancelar-0");
    esperar_fila(&mut m.app);
    assert_eq!(texto_do_editor(&m.app), "texto", "cancelar tira o link");
    esperar(&mut m.app);
    assert!(fichas(&pasta, id).is_empty());
    // A thread desfaz o arquivo temporário: nada fica em vault/.
    std::thread::sleep(Duration::from_millis(300));
    let restos: Vec<_> = std::fs::read_dir(pasta.caminho().join("vault"))
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(restos.is_empty(), "{restos:?}");
}

#[test]
fn apagar_a_linha_do_link_nao_apaga_o_anexo_e_ctrl_z_traz_o_link() {
    let (pasta, mut m, id) = com_nota("notas-link-apagado", "texto");
    let a = arquivo_de(&pasta, "x.txt", b"xxx");
    m.app.drop_files("notas-editor", &[a]);
    esperar_fila(&mut m.app);
    let com_link = texto_do_editor(&m.app);
    assert!(com_link.contains("anexo:"));
    m.app
        .click("notas-editor")
        .shortcut('a')
        .key(Key::Backspace);
    esperar(&mut m.app);
    assert_eq!(texto_do_editor(&m.app), "");
    let ficha = fichas(&pasta, id).remove(0);
    assert!(pasta.caminho().join("vault").join(&ficha.id).is_file());
    assert!(m.app.visible("notas-anexo-0"));
    m.app.shortcut('z');
    assert_eq!(texto_do_editor(&m.app), com_link);
}

#[test]
fn renomear_e_apagar_o_anexo_pelo_menu() {
    let (pasta, mut m, id) = com_nota("notas-anexo-menu", "texto");
    anexar_pelo_botao(&mut m.app, &arquivo_de(&pasta, "velho.txt", b"abc"), 0);
    let ficha = fichas(&pasta, id).remove(0);

    m.app.click("notas-anexo-0").click("notas-anexo-0-renomear");
    m.app
        .click("notas-anexo-nome")
        .shortcut('a')
        .type_text("novo.txt");
    m.app.click("notas-anexo-renomear-dialogo-primary");
    assert_eq!(fichas(&pasta, id)[0].nome, "novo.txt");
    assert!(m.app.label("notas-anexo-0").starts_with("novo.txt"));

    m.app.click("notas-anexo-0").click("notas-anexo-0-apagar");
    assert!(m.app.text("notas-anexo-apagar-texto").contains("novo.txt"));
    m.app.click("notas-anexo-apagar-dialogo-primary");
    assert!(fichas(&pasta, id).is_empty());
    assert!(!m.app.visible("notas-anexos"));
    // O arquivo sai do vault/ na coleta (a próxima abertura do cofre).
    let _ = ficha;
}

#[test]
fn copiar_o_conteudo_de_texto_vai_para_a_area_de_transferencia_e_binario_nao() {
    let (pasta, mut m, _id) = com_nota("notas-anexo-copiar", "texto");
    anexar_pelo_botao(
        &mut m.app,
        &arquivo_de(&pasta, "chave.pem", b"-----BEGIN-----\nabc\n"),
        0,
    );
    m.app.click("notas-anexo-0").click("notas-anexo-0-copiar");
    esperar_ate(&mut m.app, "o aviso de cópia", |a| {
        (a.visible("notas-info") && a.text("notas-info").contains("copiado"))
            || a.visible("notas-erro")
    });
    assert!(!m.app.visible("notas-erro"), "{}", m.app.text("notas-erro"));
    // A escrita na área de transferência sai no quadro seguinte ao do aviso.
    m.app.wait(Duration::from_millis(50));
    assert_eq!(
        m.app.clipboard().read().as_deref(),
        Some("-----BEGIN-----\nabc\n")
    );
    assert!(
        m.app
            .text("notas-info")
            .contains("Conteúdo do anexo copiado")
    );

    // Um binário não tem o item "Copiar o conteúdo".
    anexar_pelo_botao(
        &mut m.app,
        &arquivo_de(&pasta, "dados.bin", &[0, 159, 146, 150, 255]),
        1,
    );
    m.app.click("notas-anexo-1");
    assert!(!m.app.visible("notas-anexo-1-copiar"));
}

#[test]
fn duplicar_a_nota_leva_as_fichas_dos_anexos() {
    let (pasta, mut m, id) = com_nota("notas-duplicar-anexo", "texto");
    anexar_pelo_botao(&mut m.app, &arquivo_de(&pasta, "d.txt", b"ddd"), 0);
    m.app.click("notas-mais").click("notas-duplicar");
    esperar(&mut m.app);
    let cofre = pasta.cofre();
    let copia = cofre
        .itens()
        .iter()
        .find(|i| i.titulo == "Anotação (cópia)")
        .expect("a cópia foi salva");
    assert_eq!(copia.anexos, fichas(&pasta, id));
    assert_eq!(m.app.label("notas-anexo-0"), "d.txt · 3 B");
}

#[test]
fn bloquear_o_cofre_cancela_o_anexo_em_andamento_e_desfaz_o_link() {
    let pasta = Pasta::nova("notas-bloqueio-anexo");
    let ids = pasta.semear_cofre(vec![nota("Anotação", "texto")]);
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    m.app.shortcut(Shortcut::ctrl_shift('J'));
    selecionar(&mut m.app, ids[0]);
    let enorme = pasta.caminho().join("enorme.bin");
    std::fs::File::create(&enorme)
        .unwrap()
        .set_len(300 * 1024 * 1024)
        .unwrap();
    m.app.drop_files("notas-editor", &[enorme]);
    assert!(m.app.visible("notas-anexo-fila"));
    m.app.click("navegacao-cofre").click("cofre-bloquear");
    std::thread::sleep(Duration::from_millis(300));
    m.app.wait(Duration::from_millis(50));

    let (mut copia, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    assert!(copia.desbloquear("abc"));
    let item = copia.item(ids[0]).unwrap();
    assert!(item.anexos.is_empty());
    assert_eq!(
        item.notas, "texto",
        "o link do anexo cancelado não vai para o disco"
    );
    let restos: Vec<_> = std::fs::read_dir(pasta.caminho().join("vault"))
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(restos.is_empty(), "{restos:?}");
}

#[test]
fn exportar_e_importar_o_markdown_leva_os_anexos_e_troca_os_links() {
    let (pasta, mut m, id) = com_nota("notas-ida-e-volta", "Veja:");
    let imagem = ClipboardImage {
        width: 2,
        height: 2,
        rgba: vec![9; 16],
    };
    m.app.click("notas-editor");
    m.app.clipboard().write_image(imagem.clone());
    m.app.shortcut('v');
    esperar_ate(&mut m.app, "a imagem anexada", |a| {
        existe(a, "notas-anexo-0") && !a.visible("notas-anexo-fila")
    });
    let b = arquivo_de(&pasta, "dados leves (1).txt", b"bbb");
    // Soltar na prévia põe o link no fim (soltar no editor põe no ponto do ponteiro).
    m.app.click("notas-modo-dividir");
    m.app.drop_files("notas-previa", &[b]);
    esperar_ate(&mut m.app, "o arquivo anexado", |a| {
        existe(a, "notas-anexo-1") && !a.visible("notas-anexo-fila")
    });
    esperar(&mut m.app);
    let antes = fichas(&pasta, id);
    let texto_no_cofre = corpo_salvo(&pasta, "Anotação").unwrap();
    assert!(texto_no_cofre.contains("anexo:"));

    // Exportar: o seletor de pasta responde `saida/`.
    let saida = pasta.caminho().join("saida");
    std::fs::create_dir_all(&saida).unwrap();
    m.app.file_dialog_answer(Some(saida.clone()));
    m.app.click("notas-mais").click("notas-exportar");
    esperar_ate(&mut m.app, "a exportação", |a| {
        a.visible("notas-info") && a.text("notas-info").starts_with("Nota exportada")
    });
    let md = std::fs::read_to_string(saida.join("Anotação.md")).unwrap();
    let colada = antes
        .iter()
        .find(|f| f.nome.starts_with("colado-"))
        .unwrap();
    assert!(
        md.contains(&format!(
            "![{}](Anota%C3%A7%C3%A3o-attachments/{})",
            colada.nome, colada.nome
        )),
        "{md}"
    );
    assert!(
        md.contains("(Anota%C3%A7%C3%A3o-attachments/dados%20leves%20%281%29.txt)"),
        "{md}"
    );
    assert!(!md.contains("anexo:"));
    let dir = saida.join("Anotação-attachments");
    assert_eq!(
        std::fs::read(dir.join(&colada.nome)).unwrap(),
        imagem.to_png().unwrap()
    );
    assert_eq!(
        std::fs::read(dir.join("dados leves (1).txt")).unwrap(),
        b"bbb"
    );
    assert_eq!(
        corpo_salvo(&pasta, "Anotação").unwrap(),
        texto_no_cofre,
        "o cofre não muda"
    );

    // Importar o `.md` exportado: uma nota nova com os mesmos anexos (ids novos).
    m.app.file_dialog_answer(Some(saida.join("Anotação.md")));
    m.app
        .click("notas-mais-lista")
        .click("notas-importar-arquivo");
    esperar_ate(&mut m.app, "a importação", |a| {
        a.visible("notas-erro")
            || (a.visible("notas-info") && a.text("notas-info").contains("importad"))
    });
    assert!(!m.app.visible("notas-erro"), "{}", m.app.text("notas-erro"));
    let cofre = pasta.cofre();
    let nova = cofre
        .itens()
        .iter()
        .find(|i| i.tipo == Tipo::Nota && i.id != id)
        .expect("a nota importada");
    assert_eq!(nova.titulo, "Anotação");
    assert_eq!(nova.anexos.len(), 2);
    for ficha in &nova.anexos {
        assert!(antes.iter().all(|a| a.id != ficha.id), "ids novos");
        let original = antes.iter().find(|a| a.nome == ficha.nome).unwrap();
        assert_eq!(ficha.sha256, original.sha256);
        assert_eq!(conteudo(&pasta, ficha), conteudo(&pasta, original));
        assert!(
            nova.notas.contains(&format!("anexo:{}", ficha.id)),
            "{}",
            nova.notas
        );
    }
    // A nota importada abre na tela e a prévia mostra a imagem importada.
    esperar_ate(&mut m.app, "a imagem da nota importada", |a| ha_imagem(a));
}

// ---------------------------------------------------------------------------------------------
// Imagens na pré-visualização (fatia 8)
// ---------------------------------------------------------------------------------------------

/// Um PNG de 2 × 2 pixels.
fn png_pequeno() -> Vec<u8> {
    ClipboardImage {
        width: 2,
        height: 2,
        rgba: vec![255; 2 * 2 * 4],
    }
    .to_png()
    .expect("o PNG codifica")
}

/// Se a tela desenha alguma `Image`.
fn ha_imagem(app: &TestApp) -> bool {
    // O ícone do app na barra de título também é uma imagem, mas não é da nota.
    fn anda(n: &tomni_ui::Node) -> bool {
        n.id.as_deref() != Some("barra-de-titulo")
            && (n.kind == "image" || n.children.iter().any(anda))
    }
    anda(&app.tree())
}

#[test]
fn imagem_corrompida_vira_link_e_o_clique_abre_salvar_como_e_endereco_de_fora_nao_vira_imagem() {
    let (pasta, mut m, id) = com_nota(
        "notas-imagem-corrompida",
        "![y](https://exemplo.invalid/y.png)\n",
    );
    let falsa = arquivo_de(&pasta, "x.png", b"isto nao e uma imagem");
    m.app.click("notas-modo-dividir");
    m.app.drop_files("notas-editor", &[falsa]);
    esperar_fila(&mut m.app);
    // O endereço de fora é o link 0 e o anexo que não decodifica é o link 1: nenhum vira imagem.
    esperar_ate(&mut m.app, "o link da imagem corrompida", |a| {
        existe(a, "notas-previa-link-1")
    });
    assert!(existe(&m.app, "notas-previa-link-0"));
    assert!(!ha_imagem(&m.app), "nenhuma das duas desenha uma imagem");
    assert_eq!(fichas(&pasta, id).len(), 1);

    // O clique no link abre o "Salvar como…" com o nome do anexo.
    let antes = m.app.file_dialogs().len();
    m.app.click("notas-previa-link-1");
    let pedidos = m.app.file_dialogs();
    assert_eq!(pedidos.len(), antes + 1);
    assert_eq!(pedidos[antes].file_name.as_deref(), Some("x.png"));
    m.app.file_dialog_answer(None::<&str>);
}

#[test]
fn clicar_na_imagem_abre_o_tamanho_original_e_esc_fecha() {
    let (pasta, mut m, _) = com_nota("notas-imagem-grande", "texto");
    let origem = arquivo_de(&pasta, "foto.png", &png_pequeno());
    m.app.click("notas-modo-dividir");
    m.app.drop_files("notas-editor", &[origem]);
    esperar_fila(&mut m.app);
    esperar_ate(&mut m.app, "a imagem na prévia", |a| {
        existe(a, "notas-previa-imagem-0")
    });
    assert!(!m.app.visible("notas-imagem-dialogo"));
    m.app.click("notas-previa-imagem-0");
    esperar_ate(&mut m.app, "o diálogo da imagem", |a| {
        a.visible("notas-imagem-dialogo")
    });
    assert!(existe(&m.app, "notas-imagem-rolagem"));
    assert!(m.app.visible("notas-imagem-dialogo-primary"));
    m.app.key(Key::Escape);
    assert!(!m.app.visible("notas-imagem-dialogo"));
}

#[test]
fn apagar_o_link_da_imagem_nao_apaga_o_anexo_e_ctrl_z_traz_a_imagem_de_volta() {
    let (pasta, mut m, id) = com_nota("notas-imagem-link-apagado", "");
    let origem = arquivo_de(&pasta, "foto.png", &png_pequeno());
    m.app.click("notas-modo-dividir");
    m.app.drop_files("notas-editor", &[origem]);
    esperar_fila(&mut m.app);
    esperar_ate(&mut m.app, "a imagem na prévia", |a| {
        existe(a, "notas-previa-imagem-0")
    });
    let com_link = texto_do_editor(&m.app);
    m.app
        .click("notas-editor")
        .shortcut('a')
        .key(Key::Backspace);
    esperar(&mut m.app);
    assert!(!existe(&m.app, "notas-previa-imagem-0"));
    let ficha = fichas(&pasta, id).remove(0);
    assert!(pasta.caminho().join("vault").join(&ficha.id).is_file());
    assert!(m.app.visible("notas-anexo-0"));
    m.app.shortcut('z');
    assert_eq!(texto_do_editor(&m.app), com_link);
    esperar_ate(&mut m.app, "a imagem de volta", |a| {
        existe(a, "notas-previa-imagem-0")
    });
}

#[test]
fn bloquear_esvazia_as_imagens_e_destravar_decifra_de_novo() {
    let pasta = Pasta::nova("notas-imagem-bloqueio");
    let ids = pasta.semear_cofre(vec![nota("Anotação", "")]);
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    m.app.shortcut(Shortcut::ctrl_shift('J'));
    selecionar(&mut m.app, ids[0]);
    let origem = arquivo_de(&pasta, "foto.png", &png_pequeno());
    m.app.click("notas-modo-dividir");
    m.app.drop_files("notas-editor", &[origem]);
    esperar_fila(&mut m.app);
    esperar_ate(&mut m.app, "a imagem na prévia", |a| {
        existe(a, "notas-previa-imagem-0")
    });

    m.app.click("navegacao-cofre").click("cofre-bloquear");
    m.app.click("navegacao-notas");
    assert!(m.app.visible("notas-bloqueado"));
    assert!(!ha_imagem(&m.app));

    m.app.click("notas-desbloquear-botao");
    m.app.type_into("notas-desbloquear-campo", "abc");
    m.app.click("notas-desbloquear-primary");
    assert!(!m.app.visible("notas-bloqueado"));
    esperar_ate(&mut m.app, "a imagem de volta", |a| ha_imagem(a));
}

/// Os rótulos das linhas de propriedades da tela.
fn linhas_da_grade(app: &TestApp) -> Vec<String> {
    fn anda(n: &tomni_ui::Node, saida: &mut Vec<String>) {
        if n.kind == "property_grid_row" {
            saida.push(
                n.label
                    .clone()
                    .or_else(|| n.title.clone())
                    .unwrap_or_default(),
            );
        }
        for f in &n.children {
            anda(f, saida);
        }
    }
    let mut v = Vec::new();
    anda(&app.tree(), &mut v);
    v
}

#[test]
fn no_cofre_a_nota_vira_previa_sem_o_campo_notas_e_o_clique_duplo_abre_a_pagina_notas() {
    let pasta = Pasta::nova("notas-no-cofre");
    let itens = pasta.semear_cofre(vec![
        nota(
            "Roteiro",
            "# Acesso\n\n- [ ] liberar\n\n```segredo\nabacaxi-de-teste\n```\n",
        ),
        item_ssh("Servidor", "root", "s3nha"),
    ]);
    let mut m = montar(&pasta, Theme::Light);
    m.app.shortcut(Shortcut::ctrl_shift('K'));
    assert_eq!(m.app.text("navegacao"), "cofre");

    // O item comum mantém o campo "Notas" e não tem prévia.
    m.app.click(&format!("cofre-lista-item:{}", itens[1]));
    assert!(linhas_da_grade(&m.app).iter().any(|l| l == "Notas"));
    assert!(!existe(&m.app, "cofre-nota-previa"));

    // A nota troca o campo pela prévia (só leitura, segredo mascarado) e pelo botão.
    m.app.click(&format!("cofre-lista-item:{}", itens[0]));
    assert!(
        !linhas_da_grade(&m.app).iter().any(|l| l == "Notas"),
        "{:?}",
        linhas_da_grade(&m.app)
    );
    assert!(existe(&m.app, "cofre-nota-previa"));
    assert!(m.app.visible("cofre-abrir-nota"));
    assert!(
        !m.app.enabled("cofre-nota-previa-tarefa-0"),
        "na prévia do Cofre as tarefas ficam desabilitadas"
    );
    let tudo = ids(&m.app).join("\n");
    assert!(
        tudo.contains("mascara"),
        "o segredo fica mascarado:\n{tudo}"
    );
    assert!(!tudo.contains("abacaxi-de-teste"));

    // "Abrir em Notas" leva à página Notas, na nota.
    m.app.click("cofre-abrir-nota");
    assert_eq!(m.app.text("navegacao"), "notas");
    esperar_ate(&mut m.app, "a nota no editor", |a| {
        existe(a, "notas-editor") && texto_do_editor(a).starts_with("# Acesso")
    });

    // O clique duplo na lista do Cofre faz o mesmo; no item comum, copia a senha.
    m.app.shortcut(Shortcut::ctrl_shift('K'));
    assert_eq!(m.app.text("navegacao"), "cofre");
    m.app
        .double_click(&format!("cofre-lista-item:{}", itens[0]));
    assert_eq!(m.app.text("navegacao"), "notas");
}

#[test]
fn a_nota_ligada_de_um_arquivo_antigo_vira_descricao_ao_abrir_o_cofre() {
    let pasta = Pasta::nova("notas-migrar-descricao");
    let notas = pasta.semear_cofre(vec![
        nota("web-01", "  banco & site\n"),
        nota("Chave do Google", "segredo"),
    ]);
    pasta.cofre_com_senha_mestra("mestra");
    let mut arvore = exemplo();
    arvore.connection_mut(2).unwrap().nota = Some(notas[0]);
    pasta.semear(&arvore);
    let mut m = montar(&pasta, Theme::Light);

    // Com o cofre bloqueado nada muda: a ligação espera a senha mestra.
    assert_eq!(pasta.conexoes().connection(2).unwrap().nota, Some(notas[0]));
    m.app.type_into("cofre-desbloquear-campo", "mestra");
    m.app.click("cofre-desbloquear-primary");
    esperar_ate(&mut m.app, "a descrição migrada", |_| {
        pasta.conexoes().connection(2).unwrap().nota.is_none()
    });
    let c = pasta.conexoes().connection(2).cloned().unwrap();
    assert_eq!(c.descricao, "banco & site");
    // A nota ligada sai do cofre; a solta, com o segredo, fica.
    let mut cofre = pasta.cofre();
    assert!(cofre.desbloquear("mestra"));
    assert!(cofre.item(notas[0]).is_none());
    assert!(cofre.item(notas[1]).is_some());

    m.app.expand("arvore", "1").click("arvore-2");
    assert_eq!(valor(&m.app, "Descrição"), "banco & site");
    // O menu da conexão não fala mais de notas.
    m.app.right_click("arvore-2");
    let menu: Vec<String> = ids(&m.app)
        .into_iter()
        .filter(|i| i.starts_with("menu-2-"))
        .collect();
    assert!(
        !menu.is_empty() && !menu.iter().any(|i| i.contains("nota")),
        "{menu:?}"
    );
}

#[test]
fn a_busca_global_acha_a_nota_pelo_titulo_e_abre_a_pagina_notas() {
    let pasta = Pasta::nova("notas-busca-global");
    pasta.semear(&exemplo());
    let ids = pasta.semear_cofre_em(vec![(
        "Clientes/ACME",
        nota("Roteiro-do-abacate", "# Passos"),
    )]);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("busca").type_text("abacate");
    m.app.key(Key::Enter);
    assert_eq!(m.app.text("navegacao"), "notas");
    esperar_ate(&mut m.app, "a nota no editor", |a| {
        existe(a, &format!("notas-arvore-nota-{}", ids[0]))
            && texto_do_editor(a).contains("# Passos")
    });
}

/// O que "outra máquina" faz com a nota `id`: abre o cofre da pasta, troca o corpo e grava.
fn alterar_de_outra_maquina(pasta: &Pasta, id: u64, corpo: &str) {
    let (mut cofre, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    let mut item = cofre.item(id).expect("a nota existe").clone();
    item.notas = corpo.to_owned();
    cofre.salvar(item).expect("gravar na outra máquina");
}

#[test]
fn a_nota_aberta_sem_pendencia_recarrega_com_o_texto_que_chegou_de_outra_maquina() {
    let pasta = Pasta::nova("notas-vigia-recarrega");
    let ids = pasta.semear_cofre(vec![nota("Roteiro", "versão daqui")]);
    let (mut m, _) = comum::montar_com_ambiente(&pasta, Theme::Light, |amb| {
        amb.vigia_do_cofre = Duration::from_millis(100);
    });
    m.app.shortcut(Shortcut::ctrl_shift('J'));
    selecionar(&mut m.app, ids[0]);
    assert_eq!(texto_do_editor(&m.app), "versão daqui");

    alterar_de_outra_maquina(&pasta, ids[0], "versão de lá");
    esperar_ate(&mut m.app, "o texto de fora no editor", |a| {
        texto_do_editor(a) == "versão de lá"
    });
    assert!(m.app.text("aviso-info").contains("outra máquina"));
    // Ninguém editou aqui: não há cópia em conflito.
    assert_eq!(pasta.cofre().itens().len(), 1);
}

#[test]
fn editar_aqui_e_la_ao_mesmo_tempo_guarda_a_que_perdeu_como_copia_e_avisa() {
    let (pasta, mut m, id) = com_nota("notas-conflito-no-salvamento", "base");
    m.app.type_into("notas-editor", " e a edição daqui");
    // Antes de o salvamento automático vencer, a outra máquina grava o mesmo arquivo.
    alterar_de_outra_maquina(&pasta, id, "base e a edição de lá");
    esperar(&mut m.app);
    esperar_ate(&mut m.app, "a cópia em conflito no disco", |_| {
        pasta
            .cofre()
            .itens()
            .iter()
            .any(|i| i.titulo.starts_with("Anotação (conflito de "))
    });
    let itens = pasta.cofre().itens().to_vec();
    assert_eq!(itens.len(), 2, "a original e a cópia do conflito");
    let textos: Vec<&str> = itens.iter().map(|i| i.notas.as_str()).collect();
    assert!(textos.contains(&"base e a edição de lá"), "{textos:?}");
    assert!(textos.contains(&"base e a edição daqui"), "{textos:?}");
    esperar_ate(&mut m.app, "o aviso do conflito", |a| {
        existe(a, "aviso-atencao")
    });
    assert!(
        m.app
            .text("aviso-atencao")
            .contains("alterada nas duas máquinas")
    );
}
