//! Os logs de sessão cifrados (M53), pela tela: a gravação, o selo periódico, o fim do segmento
//! ao fechar a aba e o que acontece sem a chave pública.

mod comum;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::configuracoes::Configuracoes;
use tirech_omni::configuracoes::catalogo::{self, Tipo, id_do_valor};
use tirech_omni::log_cifrado::{ARQUIVO_PUBLICO, AvisoLog, Leitura, ler};
use tirech_omni::logs::{garantir_chave, mapa_de_chaves};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar, montar_pura};

/// Uma pasta com a árvore de exemplo e os logs de todas as sessões ligados. Com `cifrado`, a
/// chave dos logs já existe no cofre e a pública no disco.
fn pasta(nome: &str, cifrado: bool) -> Pasta {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    pasta.configurar(
        "terminal",
        "logs",
        serde_json::json!({
            "todas_as_sessoes": true,
            "cifrar": cifrado,
            "selar_secs": 1,
        }),
    );
    if cifrado {
        let mut cofre = pasta.cofre();
        garantir_chave(&mut cofre).expect("criar a chave dos logs");
    }
    pasta
}

/// Os arquivos da pasta de logs, em ordem.
fn arquivos_de_log(pasta: &Pasta) -> Vec<PathBuf> {
    let mut lista: Vec<PathBuf> = fs::read_dir(pasta.caminho().join("logs"))
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    lista.sort();
    lista
}

/// O texto e os avisos de um log cifrado, lidos com as chaves do cofre da pasta.
fn lido(pasta: &Pasta, arquivo: &Path) -> (String, Leitura) {
    let chaves: HashMap<[u8; 8], [u8; 32]> = mapa_de_chaves(&pasta.cofre());
    let mut origem = fs::File::open(arquivo).expect("abrir o log");
    let mut saida = Vec::new();
    let leitura = ler(&mut origem, &chaves, &mut saida, &AtomicBool::new(false)).expect("ler");
    (String::from_utf8(saida).unwrap(), leitura)
}

fn esperar(app: &mut TestApp, o_que: &str, mut pronto: impl FnMut() -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto() {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(50));
    }
}

fn encerrar(m: &Montagem) {
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

#[test]
fn o_log_sai_cifrado_o_selo_grava_e_fechar_a_aba_fecha_o_segmento() {
    let pasta = pasta("logs-cifrado", true);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").double_click("arvore-2");
    m.app.type_into("paineis", "segredo-no-log\n");
    assert!(m.app.session_text("2-1").contains("segredo-no-log"));

    let arquivos = arquivos_de_log(&pasta);
    assert_eq!(arquivos.len(), 1, "{arquivos:?}");
    let arquivo = arquivos[0].clone();
    assert!(
        arquivo.to_string_lossy().ends_with("-1.log.cifrado"),
        "{arquivo:?}"
    );

    // O selo periódico (1 s) leva o buffer ao disco sem ninguém pedir.
    esperar(&mut m.app, "o selo gravar o texto", || {
        lido(&pasta, &arquivo).0.contains("segredo-no-log")
    });
    let bruto = fs::read(&arquivo).unwrap();
    assert!(
        !String::from_utf8_lossy(&bruto).contains("segredo-no-log"),
        "o texto claro foi parar no disco"
    );
    // Com a sessão viva, o segmento ainda não tem o fim.
    let (_, leitura) = lido(&pasta, &arquivo);
    assert_eq!(leitura.avisos, [AvisoLog::SemFim { segmento: 1 }]);

    // Fechar a aba fecha o segmento, sem aviso.
    m.app.close_tab("paineis-painel-0", "2-1");
    m.app.frame();
    let (texto, leitura) = lido(&pasta, &arquivo);
    assert!(texto.contains("segredo-no-log"), "{texto:?}");
    assert_eq!(leitura.segmentos, 1);
    assert!(leitura.avisos.is_empty(), "{:?}", leitura.avisos);
    encerrar(&m);
}

#[test]
fn com_a_opcao_desligada_o_log_continua_em_texto() {
    let pasta = pasta("logs-texto", false);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1").double_click("arvore-2");
    m.app.type_into("paineis", "texto-claro\n");
    let arquivos = arquivos_de_log(&pasta);
    assert_eq!(arquivos.len(), 1, "{arquivos:?}");
    assert!(arquivos[0].to_string_lossy().ends_with(".log"));
    esperar(&mut m.app, "o selo gravar o texto", || {
        fs::read_to_string(&arquivos[0]).is_ok_and(|t| t.contains("texto-claro"))
    });
    encerrar(&m);
}

#[test]
fn sem_a_chave_publica_a_aba_abre_sem_log_e_avisa_no_terminal() {
    let pasta = pasta("logs-sem-pub", true);
    pasta.cofre_com_senha_mestra("abc");
    fs::remove_file(pasta.caminho().join(ARQUIVO_PUBLICO)).unwrap();
    let mut m = montar(&pasta, Theme::Light);
    // Trancado, o app não tem de onde regravar o .pub.
    m.app.click("cofre-desbloquear-close");
    m.app.expand("arvore", "1").double_click("arvore-2");
    m.app.frame();
    let tela = m.app.session_text("2-1");
    let tela = tela.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(tela.contains("chave pública dos logs cifrados"), "{tela:?}");
    assert!(arquivos_de_log(&pasta).is_empty(), "nada em texto também");
    encerrar(&m);
}

#[test]
fn a_chave_publica_de_outra_chave_vale_como_ausente() {
    let pasta = pasta("logs-pub-errada", true);
    pasta.cofre_com_senha_mestra("abc");
    // Troca o id: a leitura recusa e o app trata como se não houvesse.
    let arquivo = pasta.caminho().join(ARQUIVO_PUBLICO);
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&arquivo).unwrap()).unwrap();
    json["id"] = "0000000000000000".into();
    fs::write(&arquivo, serde_json::to_vec(&json).unwrap()).unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("cofre-desbloquear-close");
    m.app.expand("arvore", "1").double_click("arvore-2");
    m.app.frame();
    assert!(arquivos_de_log(&pasta).is_empty());
    encerrar(&m);
}

// ---------------------------------------------------------------------------------------
// Ligar a opção
// ---------------------------------------------------------------------------------------

const CIFRAR: &str = "terminal.logs.cifrar";

/// Vai à página Configurações e abre os grupos até o interruptor de cifrar os logs aparecer.
fn ir_ao_interruptor(app: &mut TestApp) -> String {
    app.click("inicio-ferramenta-configuracoes");
    app.click("config-secao-terminal");
    let id = id_do_valor(CIFRAR);
    for grupo in catalogo::CATALOGO.iter().filter(|d| d.tipo == Tipo::Grupo) {
        if app.ui().find(&id).is_some() && app.visible(&id) {
            break;
        }
        let cabecalho = format!("{}-cabecalho", grupo.id());
        if app.ui().find(&cabecalho).is_some() {
            app.click(&cabecalho);
        }
    }
    assert!(app.visible(&id), "o interruptor não aparece");
    id
}

fn gravada(pasta: &Pasta) -> Configuracoes {
    Configuracoes::carregar(&Armazenamento::new(pasta.caminho())).0
}

fn chaves_no_cofre(pasta: &Pasta) -> usize {
    pasta.cofre().chaves_dos_logs().len()
}

#[test]
fn ligar_com_o_cofre_aberto_cria_a_chave_e_o_pub_uma_vez_so() {
    let pasta = Pasta::nova("logs-ligar");
    pasta.semear(&exemplo());
    let mut m = montar_pura(&pasta, Theme::Light);
    let id = ir_ao_interruptor(&mut m.app);
    assert_eq!(chaves_no_cofre(&pasta), 0);
    assert!(!pasta.caminho().join(ARQUIVO_PUBLICO).exists());

    m.app.click(&id);
    assert!(m.app.checked(&id));
    assert_eq!(chaves_no_cofre(&pasta), 1);
    assert!(pasta.caminho().join(ARQUIVO_PUBLICO).exists());

    // Desligar deixa a chave e o .pub; ligar de novo não cria outra.
    m.app.click(&id);
    assert!(!m.app.checked(&id));
    assert_eq!(chaves_no_cofre(&pasta), 1);
    m.app.click(&id);
    assert_eq!(chaves_no_cofre(&pasta), 1);
    assert!(gravada(&pasta).terminal.logs.cifrar);
}

#[test]
fn ligar_com_o_cofre_trancado_pede_a_senha_e_cancelar_desliga_com_aviso() {
    let pasta = Pasta::nova("logs-trancado-cancela");
    pasta.semear(&exemplo());
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar_pura(&pasta, Theme::Light);
    // O diálogo do início: adiar deixa o cofre trancado.
    m.app.click("cofre-desbloquear-close");
    let id = ir_ao_interruptor(&mut m.app);
    m.app.click(&id);
    assert!(
        m.app.is_open("cofre-desbloquear"),
        "o diálogo de senha não abriu"
    );

    m.app.click("cofre-desbloquear-close");
    assert!(!m.app.checked(&id), "a opção devia voltar a desligada");
    assert!(m.app.text("aviso-atencao").contains("destrave o cofre"));
    assert!(!gravada(&pasta).terminal.logs.cifrar);
    assert!(!pasta.caminho().join(ARQUIVO_PUBLICO).exists());
}

#[test]
fn ligar_com_o_cofre_trancado_e_destravar_cria_a_chave() {
    let pasta = Pasta::nova("logs-trancado-destrava");
    pasta.semear(&exemplo());
    pasta.cofre_com_senha_mestra("abc");
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("cofre-desbloquear-close");
    let id = ir_ao_interruptor(&mut m.app);
    m.app.click(&id);
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    assert!(m.app.checked(&id));
    assert!(gravada(&pasta).terminal.logs.cifrar);
    assert!(pasta.caminho().join(ARQUIVO_PUBLICO).exists());
    // O cofre tem a chave (aberto de novo com a senha).
    let (mut cofre, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    assert!(cofre.desbloquear("abc"));
    assert_eq!(cofre.chaves_dos_logs().len(), 1);
}

#[test]
fn destravar_com_o_pub_apagado_regrava_o_pub() {
    let pasta = pasta("logs-regrava-pub", true);
    pasta.cofre_com_senha_mestra("abc");
    // A senha mestra reescreve o cofre com a chave dentro; o .pub some.
    let publica = pasta.caminho().join(ARQUIVO_PUBLICO);
    fs::remove_file(&publica).unwrap();
    let mut m = montar_pura(&pasta, Theme::Light);
    // Trancado: a sessão abriria sem log, e nada regrava ainda.
    assert!(!publica.exists());
    m.app.type_into("cofre-desbloquear-campo", "abc");
    m.app.click("cofre-desbloquear-primary");
    assert!(publica.exists(), "o .pub devia voltar ao destravar");
    let (mut cofre, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    assert!(cofre.desbloquear("abc"));
    assert_eq!(cofre.chaves_dos_logs().len(), 1, "sem chave nova");
}

#[test]
fn ligar_oferece_cifrar_os_logs_de_texto_que_ja_existem() {
    let pasta = Pasta::nova("logs-ja-existem");
    pasta.semear(&exemplo());
    let logs = pasta.caminho().join("logs");
    fs::create_dir_all(&logs).unwrap();
    for n in 1..=3 {
        fs::write(
            logs.join(format!("Web-20260101-10000{n}-1.log")),
            format!("linha {n}\n"),
        )
        .unwrap();
    }
    let mut m = montar_pura(&pasta, Theme::Light);
    let id = ir_ao_interruptor(&mut m.app);
    // Desligado: nada é oferecido.
    assert!(
        m.app.ui().find("config-terminal-logs-existentes").is_none()
            || !m.app.visible("config-terminal-logs-existentes")
    );

    m.app.click(&id);
    m.app.frame();
    assert!(m.app.visible("config-terminal-logs-existentes"));
    assert_eq!(
        m.app.label("config-terminal-logs-existentes-cifrar"),
        "Cifrar os 3 logs de texto que já existem"
    );

    // O botão abre a confirmação da página Logs; confirmar cifra os três e tira a oferta.
    m.app.click("config-terminal-logs-existentes-cifrar");
    assert!(m.app.is_open("logs-converter-dialogo"));
    m.app.click("logs-converter-dialogo-primary");
    let limite = Instant::now() + Duration::from_secs(10);
    while arquivos_de_log(&pasta)
        .iter()
        .any(|a| a.extension().is_some_and(|e| e == "log"))
    {
        assert!(Instant::now() < limite, "a conversão não terminou");
        m.app.wait(Duration::from_millis(20));
    }
    let nomes = arquivos_de_log(&pasta);
    assert_eq!(nomes.len(), 3, "{nomes:?}");
    assert!(
        nomes
            .iter()
            .all(|n| n.to_string_lossy().ends_with(".log.cifrado"))
    );
    let (texto, _) = lido(&pasta, &nomes[0]);
    assert!(texto.starts_with("linha "), "{texto:?}");
    m.app.frame();
    assert!(!m.app.visible("config-terminal-logs-existentes"));
}

// ---------------------------------------------------------------------------------------
// Cofres que se juntam e senha mestra que muda: os logs seguem abrindo
// ---------------------------------------------------------------------------------------

/// Grava um log cifrado em `arquivo` com a pública dada.
fn gravar_cifrado(arquivo: &Path, publica: &[u8; 32], texto: &str) {
    use std::io::Write;
    let mut e = tirech_omni::log_cifrado::EscritorCifrado::abrir(arquivo, publica).unwrap();
    e.write_all(texto.as_bytes()).unwrap();
}

/// Lê o texto de um log cifrado com as chaves do `cofre`.
fn texto_com(cofre: &tirech_omni::cofre::Cofre, arquivo: &Path) -> String {
    let chaves = mapa_de_chaves(cofre);
    let mut origem = fs::File::open(arquivo).unwrap();
    let mut saida = Vec::new();
    ler(&mut origem, &chaves, &mut saida, &AtomicBool::new(false)).expect("ler o log");
    String::from_utf8(saida).unwrap()
}

#[test]
fn juntar_cofres_com_chaves_diferentes_abre_os_logs_das_duas_maquinas() {
    let a = Pasta::nova("logs-juntar-a");
    let b = Pasta::nova("logs-juntar-b");
    let (mut ca, _) = tirech_omni::cofre::Cofre::abrir(a.caminho());
    let (mut cb, _) = tirech_omni::cofre::Cofre::abrir(b.caminho());
    let publica_a = garantir_chave(&mut ca).unwrap();
    let publica_b = garantir_chave(&mut cb).unwrap();
    assert_ne!(publica_a.uid, publica_b.uid);
    let log_a = a.caminho().join("do-a.log.cifrado");
    let log_b = a.caminho().join("do-b.log.cifrado");
    gravar_cifrado(&log_a, &publica_a.publica, "texto da máquina A\n");
    gravar_cifrado(&log_b, &publica_b.publica, "texto da máquina B\n");

    // Antes de juntar, A não abre o log de B.
    assert_eq!(mapa_de_chaves(&ca).len(), 1);
    ca.juntar(cb.conteudo()).unwrap();
    cb.juntar(ca.conteudo()).unwrap();
    for cofre in [&ca, &cb] {
        assert_eq!(cofre.chaves_dos_logs().len(), 2);
        assert_eq!(texto_com(cofre, &log_a), "texto da máquina A\n");
        assert_eq!(texto_com(cofre, &log_b), "texto da máquina B\n");
    }
    // Com as duas chaves no cofre, não nasce uma terceira.
    garantir_chave(&mut ca).unwrap();
    assert_eq!(ca.chaves_dos_logs().len(), 2);
}

#[test]
fn trocar_a_senha_mestra_nao_quebra_os_logs() {
    let pasta = Pasta::nova("logs-senha-mestra");
    let (mut cofre, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    let publica = garantir_chave(&mut cofre).unwrap();
    let log = pasta.caminho().join("a.log.cifrado");
    gravar_cifrado(&log, &publica.publica, "segue legível\n");

    cofre.definir_senha_mestra(Some("primeira")).unwrap();
    let (mut aberto, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    assert!(aberto.bloqueado());
    assert!(
        mapa_de_chaves(&aberto).is_empty(),
        "trancado não tem chaves"
    );
    assert!(aberto.desbloquear("primeira"));
    assert_eq!(texto_com(&aberto, &log), "segue legível\n");

    aberto.definir_senha_mestra(Some("segunda")).unwrap();
    let (mut de_novo, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    assert!(de_novo.desbloquear("segunda"));
    assert_eq!(texto_com(&de_novo, &log), "segue legível\n");

    de_novo.definir_senha_mestra(None).unwrap();
    assert_eq!(texto_com(&pasta.cofre(), &log), "segue legível\n");
    assert_eq!(pasta.cofre().chaves_dos_logs().len(), 1);
}
