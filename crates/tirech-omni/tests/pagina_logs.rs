//! A página Logs (M53): a lista, o leitor com busca, o cofre trancado, a cauda dos logs grandes,
//! a exportação como texto e o apagar.

mod comum;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use tirech_omni::log_cifrado::{EscritorCifrado, ler_publica};
use tirech_omni::logs::garantir_chave;
use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

/// A pasta com a árvore de exemplo e a chave dos logs no cofre.
fn pasta(nome: &str) -> Pasta {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    let mut cofre = pasta.cofre();
    garantir_chave(&mut cofre).expect("criar a chave dos logs");
    pasta
}

fn pasta_de_logs(pasta: &Pasta) -> PathBuf {
    let p = pasta.caminho().join("logs");
    fs::create_dir_all(&p).unwrap();
    p
}

/// Cria um log de texto de `nome` com `texto`, modificado há `ha` segundos.
fn log_de_texto(pasta: &Pasta, nome: &str, texto: &str, ha: u64) -> PathBuf {
    let caminho = pasta_de_logs(pasta).join(nome);
    fs::write(&caminho, texto).unwrap();
    let quando = SystemTime::now() - Duration::from_secs(ha);
    fs::File::options()
        .write(true)
        .open(&caminho)
        .unwrap()
        .set_modified(quando)
        .unwrap();
    caminho
}

/// Cria um log cifrado com a chave do cofre da pasta.
fn log_cifrado(pasta: &Pasta, nome: &str, texto: &str, ha: u64) -> PathBuf {
    let caminho = pasta_de_logs(pasta).join(nome);
    let publica = ler_publica(pasta.caminho())
        .expect("a pública dos logs")
        .publica;
    {
        let mut e = EscritorCifrado::abrir(&caminho, &publica).unwrap();
        e.write_all(texto.as_bytes()).unwrap();
    }
    let quando = SystemTime::now() - Duration::from_secs(ha);
    fs::File::options()
        .write(true)
        .open(&caminho)
        .unwrap()
        .set_modified(quando)
        .unwrap();
    caminho
}

fn chave(caminho: &Path) -> String {
    caminho.display().to_string()
}

/// Vai à página Logs.
fn ir_aos_logs(m: &mut Montagem) {
    m.app.click("navegacao-logs");
    m.app.frame();
}

fn escolher(m: &mut Montagem, caminhos: &[&Path]) {
    let chaves: Vec<String> = caminhos.iter().map(|c| chave(c)).collect();
    let refs: Vec<&str> = chaves.iter().map(String::as_str).collect();
    m.app.grid_select("logs-lista", &refs);
    m.app.frame();
}

fn texto_do_leitor(app: &TestApp) -> String {
    app.text_editor("logs-texto").text.to_string()
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn a_lista_mostra_os_logs_do_mais_novo_ao_mais_velho_com_o_cadeado_nos_cifrados() {
    let pasta = pasta("pl-lista");
    let velho = log_de_texto(&pasta, "Web-20260101-100000-1.log", "velho\n", 300);
    let novo = log_cifrado(&pasta, "Web-20260102-100000-1.log.cifrado", "novo\n", 10);
    let meio = log_de_texto(&pasta, "Banco-20260101-120000-1.log", "meio\n", 100);
    fs::write(pasta_de_logs(&pasta).join("leiame.md"), "não é log").unwrap();
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    let grade = m.app.grid("logs-lista");
    assert_eq!(
        grade.keys(),
        [chave(&novo), chave(&meio), chave(&velho)],
        "ordem e filtro por extensão"
    );
    assert_eq!(grade.rows[0].cells[1], "", "sem conexão no nome do cifrado");
    assert_eq!(grade.rows[1].cells[0], "Banco-20260101-120000-1.log");
    // Nada aberto: o leitor pede para escolher.
    assert!(m.app.visible("logs-escolha-um"));
}

#[test]
fn o_filtro_acha_pelo_nome() {
    let pasta = pasta("pl-filtro");
    let web = log_de_texto(&pasta, "Web-20260101-100000-1.log", "a\n", 30);
    log_de_texto(&pasta, "Banco-20260101-120000-1.log", "b\n", 10);
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    m.app.type_into("logs-filtro", "web");
    m.app.frame();
    assert_eq!(m.app.grid("logs-lista").keys(), [chave(&web)]);
}

#[test]
fn escolher_um_log_de_texto_mostra_o_conteudo() {
    let pasta = pasta("pl-texto");
    let log = log_de_texto(
        &pasta,
        "Web-20260101-100000-1.log",
        "alfa\nbeta\ngama\n",
        30,
    );
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&log]);
    assert_eq!(texto_do_leitor(&m.app), "alfa\nbeta\ngama\n");
    assert!(!m.app.visible("logs-trancado"));
    assert!(!m.app.visible("logs-cauda"));
}

#[test]
fn a_busca_conta_os_acertos_e_anda_entre_eles() {
    let pasta = pasta("pl-busca");
    let log = log_de_texto(
        &pasta,
        "Web-20260101-100000-1.log",
        "um erro\ntudo bem\nOutro ERRO\n",
        30,
    );
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&log]);
    assert!(!m.app.visible("logs-busca-barra"));
    m.app.click("logs-buscar");
    assert!(m.app.visible("logs-busca-barra"));
    m.app.type_into("logs-busca", "erro");
    m.app.frame();
    assert_eq!(m.app.text_editor("logs-texto").marks.len(), 2);
    assert!(m.app.text("logs-busca-contagem").contains('1'));
    m.app.click("logs-busca-proxima");
    assert!(m.app.text("logs-busca-contagem").contains('2'));
    // Diferenciar maiúsculas deixa só o primeiro.
    m.app.click("logs-diferenciar");
    m.app.frame();
    assert_eq!(m.app.text_editor("logs-texto").marks.len(), 1);
    m.app.type_into("logs-busca", "zzz");
    m.app.frame();
    assert_eq!(m.app.text_editor("logs-texto").marks.len(), 0);
    m.app.key(Key::Escape);
    m.app.frame();
    assert!(!m.app.visible("logs-busca-barra"));
}

#[test]
fn o_cifrado_abre_com_o_cofre_aberto() {
    let pasta = pasta("pl-cifrado");
    let log = log_cifrado(
        &pasta,
        "Web-20260101-100000-1.log.cifrado",
        "segredo no log\n",
        30,
    );
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&log]);
    assert_eq!(texto_do_leitor(&m.app), "segredo no log\n");
    // O arquivo no disco continua ilegível.
    assert!(!String::from_utf8_lossy(&fs::read(&log).unwrap()).contains("segredo"));
}

#[test]
fn trancado_pede_para_destravar_e_abre_o_log_depois() {
    let pasta = pasta("pl-trancado");
    let log = log_cifrado(
        &pasta,
        "Web-20260101-100000-1.log.cifrado",
        "texto secreto\n",
        30,
    );
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("cofre-desbloquear-close");
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&log]);
    assert!(m.app.visible("logs-trancado"));
    assert!(texto_do_leitor(&m.app).is_empty());

    m.app.click("logs-destravar");
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    esperar(&mut m.app, "o log abrir", |a| {
        a.text_editor("logs-texto").text.contains("texto secreto")
    });
    assert!(!m.app.visible("logs-trancado"));
}

#[test]
fn o_log_sem_a_chave_no_cofre_mostra_o_erro() {
    let pasta = pasta("pl-sem-chave");
    let log = log_cifrado(&pasta, "Web-20260101-100000-1.log.cifrado", "x\n", 30);
    // Outro cofre, sem a chave que cifrou o log.
    let outra = Pasta::nova("pl-sem-chave-outra");
    outra.semear(&exemplo());
    let mut cofre = outra.cofre();
    garantir_chave(&mut cofre).unwrap();
    let destino = pasta_de_logs(&outra).join("Web-20260101-100000-1.log.cifrado");
    fs::copy(&log, &destino).unwrap();
    let mut m = montar(&outra, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&destino]);
    assert!(m.app.visible("logs-aviso-erro"));
    assert!(texto_do_leitor(&m.app).is_empty());
}

#[test]
fn um_log_cifrado_sem_fim_avisa_e_mostra_o_texto() {
    let pasta = pasta("pl-sem-fim");
    let caminho = pasta_de_logs(&pasta).join("Web-20260101-100000-1.log.cifrado");
    let publica = ler_publica(pasta.caminho()).unwrap().publica;
    {
        let mut e = EscritorCifrado::abrir(&caminho, &publica).unwrap();
        e.write_all(b"ate aqui\n").unwrap();
        // A queda do app: o buffer vai ao disco, mas o fim não é escrito.
        e.flush().unwrap();
        std::mem::forget(e);
    }
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&caminho]);
    assert_eq!(texto_do_leitor(&m.app), "ate aqui\n");
    assert!(m.app.visible("logs-aviso-info"));
}

#[test]
fn log_grande_mostra_o_fim_e_mostrar_tudo_traz_o_resto() {
    let pasta = pasta("pl-cauda");
    pasta.configurar(
        "terminal",
        "logs",
        serde_json::json!({ "visualizar_mib": 1 }),
    );
    let texto: String = (0..400_000).map(|i| format!("linha {i:06}\n")).collect();
    assert!(texto.len() > 4 * 1024 * 1024);
    let log = log_de_texto(&pasta, "Web-20260101-100000-1.log", &texto, 30);
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&log]);
    esperar(&mut m.app, "o fim do log", |a| a.visible("logs-cauda"));
    let mostrado = texto_do_leitor(&m.app);
    assert!(mostrado.len() <= 1024 * 1024 && texto.ends_with(&mostrado));
    assert!(mostrado.ends_with("linha 399999\n"));

    m.app.click("logs-mostrar-tudo");
    esperar(&mut m.app, "o log inteiro", |a| {
        a.text_editor("logs-texto").text.len() == texto.len()
    });
    assert!(!m.app.visible("logs-cauda"));
}

#[test]
fn trancar_o_cofre_tira_o_texto_decifrado_da_tela() {
    let pasta = pasta("pl-trancar");
    let log = log_cifrado(
        &pasta,
        "Web-20260101-100000-1.log.cifrado",
        "texto secreto\n",
        30,
    );
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar(&pasta, Theme::Light);
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&log]);
    assert_eq!(texto_do_leitor(&m.app), "texto secreto\n");

    m.app.click("navegacao-cofre").click("cofre-bloquear");
    m.app.click("navegacao-logs");
    m.app.frame();
    assert!(texto_do_leitor(&m.app).is_empty(), "o texto ficou na tela");
    assert!(m.app.visible("logs-trancado"));
}

#[test]
fn exportar_um_log_cifrado_grava_o_texto_e_nao_sobrescreve() {
    let pasta = pasta("pl-exportar");
    let log = log_cifrado(
        &pasta,
        "Web-20260101-100000-1.log.cifrado",
        "linha 1\nlinha 2\n",
        30,
    );
    let saida = Pasta::nova("pl-exportar-saida");
    fs::create_dir_all(saida.caminho()).unwrap();
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&log]);
    let destino = saida.caminho().join("web.log");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("logs-exportar");
    esperar(&mut m.app, "a exportação", |a| a.visible("aviso-sucesso"));
    assert_eq!(fs::read_to_string(&destino).unwrap(), "linha 1\nlinha 2\n");
    // O log cifrado segue como estava.
    assert!(log.exists());
}

#[test]
fn exportar_varios_vai_para_uma_pasta_e_repete_o_nome_com_numero() {
    let pasta = pasta("pl-exportar-lote");
    let a = log_cifrado(&pasta, "Web-20260101-100000-1.log.cifrado", "a\n", 30);
    let b = log_de_texto(&pasta, "Web-20260101-100000-1.log", "b\n", 20);
    let saida = Pasta::nova("pl-exportar-lote-saida");
    fs::create_dir_all(saida.caminho()).unwrap();
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&a, &b]);
    assert!(
        m.app.visible("logs-escolha-um"),
        "vários: o leitor se esvazia"
    );
    m.app.file_dialog_answer(Some(saida.caminho().to_owned()));
    m.app.click("logs-exportar");
    esperar(&mut m.app, "a exportação", |a| a.visible("aviso-sucesso"));
    let mut nomes: Vec<String> = fs::read_dir(saida.caminho())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    nomes.sort();
    assert_eq!(nomes.len(), 2, "{nomes:?}");
    assert!(nomes.iter().any(|n| n.contains("(2)")), "{nomes:?}");
}

#[test]
fn apagar_pede_confirmacao_e_leva_embora_so_o_escolhido() {
    let pasta = pasta("pl-apagar");
    let a = log_de_texto(&pasta, "Web-20260101-100000-1.log", "a\n", 30);
    let b = log_de_texto(&pasta, "Banco-20260101-120000-1.log", "b\n", 20);
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&a]);
    m.app.click("logs-apagar");
    m.app.frame();

    assert!(m.app.is_open("logs-apagar-dialogo"));
    m.app.click("logs-apagar-dialogo-close");
    assert!(a.exists(), "cancelar não apaga");

    m.app.click("logs-apagar");
    m.app.click("logs-apagar-dialogo-primary");
    m.app.frame();
    assert!(!a.exists());
    assert!(b.exists());
    assert_eq!(m.app.grid("logs-lista").keys(), [chave(&b)]);
}

#[test]
fn o_menu_da_aba_abre_o_log_da_sessao() {
    let pasta = Pasta::nova("pl-aba");
    pasta.semear(&exemplo());
    pasta.configurar(
        "terminal",
        "logs",
        serde_json::json!({ "todas_as_sessoes": true }),
    );
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").double_click("arvore-2");
    m.app.type_into("paineis", "no-log-da-aba\n");
    m.app.frame();
    let aba = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    m.app.context_menu(&format!("paineis-painel-0-{aba}"));
    m.app.click("abrir-log-da-sessao");
    m.app.frame();
    assert!(m.app.visible("logs-pagina"));
    esperar(&mut m.app, "o texto da sessão", |a| {
        a.text_editor("logs-texto").text.contains("no-log-da-aba")
    });
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn o_log_que_a_sessao_grava_aparece_como_gravando_e_nao_se_apaga() {
    let pasta = Pasta::nova("pl-gravando");
    pasta.semear(&exemplo());
    pasta.configurar(
        "terminal",
        "logs",
        serde_json::json!({ "todas_as_sessoes": true }),
    );
    let velho = log_de_texto(&pasta, "Velho-20200101-100000-1.log", "v\n", 5000);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").double_click("arvore-2");
    m.app.type_into("paineis", "x\n");
    ir_aos_logs(&mut m);
    let grade = m.app.grid("logs-lista");
    let gravando: Vec<_> = grade
        .rows
        .iter()
        .filter(|r| r.cells[4] == "Gravando")
        .collect();
    assert_eq!(gravando.len(), 1, "{:?}", grade.rows);
    let chave_gravando = gravando[0].key.clone();

    // Só o que grava: avisa e não abre o diálogo.
    m.app.grid_select("logs-lista", &[&chave_gravando]);
    m.app.click("logs-apagar");
    assert!(!m.app.is_open("logs-apagar-dialogo"));
    assert!(m.app.visible("aviso-atencao"));

    // Os dois: o diálogo conta só um e o que grava fica.
    m.app
        .grid_select("logs-lista", &[&chave_gravando, &chave(&velho)]);
    m.app.click("logs-apagar");
    assert!(m.app.is_open("logs-apagar-dialogo"));
    m.app.click("logs-apagar-dialogo-primary");
    m.app.frame();
    assert!(!velho.exists());
    assert!(Path::new(&chave_gravando).exists());
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

/// Escolhe "Cifrar os escolhidos…" (ou outro item) no menu da página.
fn menu_de_cifragem(m: &mut Montagem, item: &str) {
    m.app.click("logs-cifragem");
    m.app.click(item);
    m.app.frame();
}

#[test]
fn cifrar_os_escolhidos_troca_o_texto_pelo_cifrado_e_o_leitor_abre() {
    let pasta = pasta("pl-cifrar");
    let a = log_de_texto(&pasta, "Web-20260101-100000-1.log", "linha a\n", 30);
    let b = log_de_texto(&pasta, "Banco-20260101-120000-1.log", "linha b\n", 20);
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&a]);
    menu_de_cifragem(&mut m, "logs-cifrar");

    assert!(m.app.is_open("logs-converter-dialogo"));
    assert!(m.app.visible("logs-converter-apagar-originais"));
    assert!(!m.app.visible("logs-converter-apagar-cifrados"));
    // Cancelar não mexe em nada.
    m.app.click("logs-converter-dialogo-close");
    assert!(a.exists());

    menu_de_cifragem(&mut m, "logs-cifrar");
    m.app.click("logs-converter-dialogo-primary");
    esperar(&mut m.app, "a conversão", |a| a.visible("aviso-sucesso"));

    let cifrado = a.with_file_name("Web-20260101-100000-1.log.cifrado");
    assert!(cifrado.exists());
    assert!(!a.exists(), "o texto saiu, como a caixa vem marcada");
    assert!(b.exists(), "o que não foi escolhido fica");
    let bytes = fs::read(&cifrado).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("linha a"));
    // O cifrado novo abre no leitor.
    escolher(&mut m, &[&cifrado]);
    assert_eq!(texto_do_leitor(&m.app), "linha a\n");
}

#[test]
fn cifrar_sem_apagar_guarda_o_original_e_decifrar_devolve_o_texto() {
    let pasta = pasta("pl-ida-e-volta");
    let a = log_de_texto(&pasta, "Web-20260101-100000-1.log", "ida e volta\n", 30);
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&a]);
    menu_de_cifragem(&mut m, "logs-cifrar");
    m.app.click("logs-converter-apagar-originais");
    m.app.click("logs-converter-dialogo-primary");
    esperar(&mut m.app, "cifrar", |a| a.visible("aviso-sucesso"));
    let cifrado = a.with_file_name("Web-20260101-100000-1.log.cifrado");
    assert!(a.exists() && cifrado.exists());

    // Decifrar escolhe o cifrado, vem sem apagar e não sobrescreve o texto que já existe.
    escolher(&mut m, &[&cifrado]);
    menu_de_cifragem(&mut m, "logs-decifrar");
    assert!(m.app.visible("logs-converter-apagar-cifrados"));
    m.app.click("logs-converter-dialogo-primary");
    esperar(&mut m.app, "decifrar", |a| a.visible("aviso-sucesso"));
    assert!(cifrado.exists(), "decifrar guarda o cifrado por padrão");
    let segundo = a.with_file_name("Web-20260101-100000-1 (2).log");
    assert_eq!(fs::read_to_string(&segundo).unwrap(), "ida e volta\n");
    assert_eq!(fs::read_to_string(&a).unwrap(), "ida e volta\n");
}

#[test]
fn cifrar_todos_pega_so_os_de_texto_e_pula_o_que_grava() {
    let pasta = pasta("pl-cifrar-todos");
    let a = log_de_texto(&pasta, "Web-20260101-100000-1.log", "a\n", 30);
    let b = log_de_texto(&pasta, "Banco-20260101-120000-1.log", "b\n", 20);
    let c = log_cifrado(&pasta, "Dns-20260101-130000-1.log.cifrado", "c\n", 10);
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    // Sem escolher nada, o "todos" olha a lista inteira.
    menu_de_cifragem(&mut m, "logs-cifrar-todos");
    m.app.click("logs-converter-dialogo-primary");
    esperar(&mut m.app, "a conversão", |a| a.visible("aviso-sucesso"));
    assert!(!a.exists() && !b.exists());
    assert!(c.exists());
    let nomes: Vec<String> = fs::read_dir(pasta_de_logs(&pasta))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(nomes.len(), 3, "{nomes:?}");
    assert!(
        nomes.iter().all(|n| n.ends_with(".log.cifrado")),
        "{nomes:?}"
    );
}

#[test]
fn nada_a_converter_avisa_e_nao_abre_o_dialogo() {
    let pasta = pasta("pl-nada");
    let c = log_cifrado(&pasta, "Dns-20260101-130000-1.log.cifrado", "c\n", 10);
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&c]);
    menu_de_cifragem(&mut m, "logs-cifrar");
    assert!(!m.app.is_open("logs-converter-dialogo"));
    assert!(m.app.visible("aviso-atencao"));
}

#[test]
fn um_log_cifrado_sem_a_chave_dá_erro_e_fica_como_estava() {
    let pasta = pasta("pl-decifrar-sem-chave");
    let c = pasta_de_logs(&pasta).join("Fora-20260101-130000-1.log.cifrado");
    {
        let (_, outra) = tirech_omni::log_cifrado::par_novo().unwrap();
        let mut e = EscritorCifrado::abrir(&c, &outra).unwrap();
        e.write_all(b"de outro cofre\n").unwrap();
    }
    let antes = fs::read(&c).unwrap();
    let mut m = montar(&pasta, Theme::Light);
    ir_aos_logs(&mut m);
    escolher(&mut m, &[&c]);
    menu_de_cifragem(&mut m, "logs-decifrar");
    m.app.click("logs-converter-dialogo-primary");
    esperar(&mut m.app, "o erro", |a| a.visible("aviso-erro"));
    assert_eq!(fs::read(&c).unwrap(), antes);
    let nomes: Vec<String> = fs::read_dir(pasta_de_logs(&pasta))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(nomes.len(), 1, "sobrou arquivo: {nomes:?}");
}
