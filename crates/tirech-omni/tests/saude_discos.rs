//! O diagnóstico dos discos na tela: ao conectar, em segundo plano, pelo canal da sessão; a
//! notificação dos alertas; a opção com herança; o resultado guardado por servidor; e o bloco da
//! aba Disco do Desempenho. O servidor é o `FakeOpener`, que responde pelo que o teste põe em
//! `respostas_discos`; nenhum teste roda o `smartctl` nem o `sudo` de verdade (o script em si é
//! testado no `sh`, com programas falsos, em `src/saude_discos.rs`).

mod comum;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::json;
use tirech_omni::modelo::{Connection, DiagnosticoDeDiscos, Group, Kind, ShellRemoto, Tree};
use tirech_omni::processos::FonteDeTeste;
use tirech_omni::saude_discos::{COMANDO, COMANDO_COM_SUDO, Diagnostico, SCRIPT, ler_saida};
use tomni_ui::terminal::ssh::ExecOutput;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, montar_com_ambiente};

const TABELA: &str = "desempenho-disco-saude";
const ALERTA: &str = "desempenho-disco-saude-alerta";
const VERIFICAR: &str = "desempenho-disco-saude-verificar";
const DETALHES: &str = "desempenho-disco-saude-detalhes";
const ESTADO: &str = "desempenho-disco-saude-estado";
const GRADE: &str = "props";
const CARGA: &str = "desempenho-disco-tabela";

const SAUDAVEL: &str = r#"@tirech-discos discos
@disco /dev/nvme0 nvme
{"model_name":"SSD Rápido","smart_status":{"passed":true},"power_on_time":{"hours":26280},
 "power_cycle_count":412,"nvme_smart_health_information_log":{"critical_warning":0,
 "available_spare":100,"available_spare_threshold":10,"percentage_used":7,
 "data_units_written":2000000,"media_errors":0}}
@fim
"#;

const FALHANDO: &str = r#"@tirech-discos discos
@disco /dev/sda sat
{"model_name":"HD Velho","smart_status":{"passed":false},"power_on_time":{"hours":40000},
 "power_cycle_count":90,"ata_smart_attributes":{"table":[
 {"id":5,"name":"Reallocated_Sector_Ct","value":100,"raw":{"value":8}}]}}
@fim
"#;

/// 1 `web` e 2 `app` (SSH), 3 `Shell` (local), 4 `win` (SSH com PowerShell), e a pasta `Quieta`,
/// desligada, com 6 `dentro`.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    for nome in ["web", "app"] {
        tree.add_connection(None, ssh(nome));
    }
    tree.add_connection(None, Connection::new("Shell", Kind::Local));
    let mut win = ssh("win");
    win.shell_remoto = ShellRemoto::PowerShell;
    tree.add_connection(None, win);
    let quieta = tree.add_group(None, "Quieta");
    if let Some(tirech_omni::modelo::Node::Group(Group {
        diagnostico_discos, ..
    })) = tree.find_mut(quieta)
    {
        *diagnostico_discos = DiagnosticoDeDiscos::Desligado;
    }
    tree.add_connection(Some(quieta), ssh("dentro"));
    tree
}

fn ssh(nome: &str) -> Connection {
    let mut c = Connection::new(nome, Kind::Ssh);
    c.host = format!("{nome}.exemplo");
    c.user = "ana".into();
    c.key_file = Some("/chaves/ana".into());
    c
}

fn tela(pasta: &Pasta) -> Montagem {
    montar_com_ambiente(pasta, Theme::Light, |_| {}).0
}

fn pasta_com_arvore(nome: &str) -> Pasta {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore());
    pasta
}

/// Faz o servidor falso do `id` responder `saida` ao diagnóstico.
fn responder(m: &Montagem, id: u64, saida: &str) {
    m.opener.respostas_discos.lock().unwrap().insert(
        id,
        Ok(ExecOutput {
            status: Some(0),
            stdout: saida.as_bytes().to_vec(),
            stderr: Vec::new(),
        }),
    );
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não aconteceu: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Dá tempo de uma thread terminar, para conferir que nada aconteceu.
fn um_tempo(app: &mut TestApp) {
    for _ in 0..15 {
        app.wait(Duration::from_millis(20));
    }
}

/// Todas as chaves de aba.
fn abas(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()))
        .collect()
}

/// Abre a conexão `id` pela árvore e espera a aba nova.
fn abrir(m: &mut Montagem, id: u64) {
    let antes = abas(&m.app).len();
    m.app.double_click(&format!("arvore-{id}"));
    esperar(&mut m.app, "a aba nova", |a| abas(a).len() > antes);
}

/// Os ids das conexões em que o diagnóstico rodou.
fn rodou_em(m: &Montagem) -> Vec<u64> {
    m.opener
        .execucoes_discos
        .lock()
        .unwrap()
        .iter()
        .map(|(id, _, _)| *id)
        .collect()
}

/// Espera o diagnóstico rodar em `ids` e ser recebido pela tela.
fn esperar_rodar(m: &mut Montagem, ids: &[u64]) {
    let limite = Instant::now() + Duration::from_secs(10);
    while rodou_em(m) != ids {
        assert!(Instant::now() < limite, "rodou em {:?}", rodou_em(m));
        m.app.wait(Duration::from_millis(20));
    }
    um_tempo(&mut m.app);
}

/// Abre a aba Disco do Desempenho.
fn abrir_aba_disco(m: &mut Montagem) {
    m.app.click("menus-exibir").click("desempenho");
    m.app.click("desempenho-secoes-disco");
}

/// Se o widget `id` existe e está visível (um id que não existe não derruba o teste).
fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

#[test]
fn um_alerta_ao_conectar_vira_notificacao_e_o_clique_traz_a_aba() {
    let pasta = pasta_com_arvore("saude-alerta");
    let mut m = tela(&pasta);
    responder(&m, 1, FALHANDO);
    abrir(&mut m, 1);
    esperar(&mut m.app, "a notificação", |a| {
        !a.notifications().is_empty()
    });
    let [aviso] = m.app.notifications().try_into().expect("uma notificação");
    assert_eq!(aviso.title, "Disco com alerta em web");
    assert!(aviso.body.contains("sda"), "{}", aviso.body);
    assert!(aviso.body.contains("falhando"), "{}", aviso.body);
    assert!(
        aviso.body.contains("8 setores realocados"),
        "{}",
        aviso.body
    );
    assert_eq!(aviso.tag.as_deref(), Some("saude-discos-1"));

    // Rodou uma vez, com o script pela entrada: nada foi digitado na aba.
    let execucoes = m.opener.execucoes_discos.lock().unwrap().clone();
    assert_eq!(
        execucoes,
        [(1, COMANDO.to_owned(), SCRIPT.as_bytes().to_vec())],
        "sozinho, não pode chamar o sudo"
    );
    assert!(COMANDO.starts_with("sh -s"));

    // O clique traz a aba do servidor do alerta, mesmo com outra aberta depois.
    abrir(&mut m, 2);
    let segunda = abas(&m.app).last().cloned().expect("a segunda aba");
    assert_eq!(
        m.app.panes("paineis").panes[0].selected.as_deref(),
        Some(&*segunda)
    );
    m.app.notification_click("Disco com alerta em web");
    assert_eq!(
        m.app.panes("paineis").panes[0].selected.as_deref(),
        Some("1-1")
    );
}

#[test]
fn um_disco_saudavel_nao_notifica_mas_fica_guardado() {
    let pasta = pasta_com_arvore("saude-saudavel");
    let mut m = tela(&pasta);
    responder(&m, 1, SAUDAVEL);
    abrir(&mut m, 1);
    esperar_rodar(&mut m, &[1]);
    assert!(m.app.notifications().is_empty());
    let gravado = std::fs::read_to_string(pasta.caminho().join("saude_discos.json"))
        .expect("o saude_discos.json");
    assert!(gravado.contains("SSD Rápido"), "{gravado}");
    assert!(gravado.contains("ana@web.exemplo:22"), "{gravado}");
}

#[test]
fn com_o_resultado_guardado_nao_roda_de_novo_mas_o_alerta_volta_ao_abrir_o_app() {
    let pasta = pasta_com_arvore("saude-guardado");
    {
        let mut m = tela(&pasta);
        responder(&m, 1, FALHANDO);
        abrir(&mut m, 1);
        esperar_rodar(&mut m, &[1]);
        assert_eq!(m.app.notifications().len(), 1);
        // A mesma conexão em outra aba: nem roda, nem repete o aviso.
        abrir(&mut m, 1);
        um_tempo(&mut m.app);
        assert_eq!(rodou_em(&m), [1]);
        assert_eq!(m.app.notifications().len(), 1);
    }
    // Outra execução do app: o script não roda de novo e o alerta aparece.
    let mut m = tela(&pasta);
    abrir(&mut m, 1);
    esperar(&mut m.app, "a notificação", |a| {
        !a.notifications().is_empty()
    });
    assert!(rodou_em(&m).is_empty());
}

/// Uma leitura vencida (de 1970) em `saude_discos.json`.
fn semear_leitura_vencida(pasta: &Pasta, chave: &str, alvo: &str, com_sudo: bool) {
    let cache = json!({"leituras": {chave: {
        "alvo": alvo, "quando": 1, "diagnostico": "sem_permissao", "com_sudo": com_sudo,
    }}});
    std::fs::write(pasta.caminho().join("saude_discos.json"), cache.to_string()).unwrap();
}

/// Os comandos que rodaram no servidor falso.
fn comandos_rodados(m: &Montagem) -> Vec<String> {
    m.opener
        .execucoes_discos
        .lock()
        .unwrap()
        .iter()
        .map(|(_, comando, _)| comando.clone())
        .collect()
}

#[test]
fn vencida_a_leitura_so_chama_o_sudo_onde_ele_ja_funcionou() {
    let pasta = pasta_com_arvore("saude-sudo-provado");
    semear_leitura_vencida(&pasta, "1", "ana@web.exemplo:22", true);
    let mut m = tela(&pasta);
    responder(&m, 1, SAUDAVEL);
    abrir(&mut m, 1);
    esperar_rodar(&mut m, &[1]);
    assert_eq!(comandos_rodados(&m), [COMANDO_COM_SUDO]);
    drop(m);

    let pasta = pasta_com_arvore("saude-sudo-sem-prova");
    semear_leitura_vencida(&pasta, "1", "ana@web.exemplo:22", false);
    let mut m = tela(&pasta);
    responder(&m, 1, SAUDAVEL);
    abrir(&mut m, 1);
    esperar_rodar(&mut m, &[1]);
    assert_eq!(comandos_rodados(&m), [COMANDO]);
    drop(m);

    // A prova é do usuário e do servidor em que valeu.
    let pasta = pasta_com_arvore("saude-sudo-outro-alvo");
    semear_leitura_vencida(&pasta, "1", "root@web.exemplo:22", true);
    let mut m = tela(&pasta);
    responder(&m, 1, SAUDAVEL);
    abrir(&mut m, 1);
    esperar_rodar(&mut m, &[1]);
    assert_eq!(comandos_rodados(&m), [COMANDO]);
}

#[test]
fn trocar_o_servidor_da_conexao_le_de_novo() {
    let pasta = pasta_com_arvore("saude-outro-alvo");
    {
        let mut m = tela(&pasta);
        responder(&m, 1, SAUDAVEL);
        abrir(&mut m, 1);
        esperar_rodar(&mut m, &[1]);
    }
    let mut tree = arvore();
    tree.connection_mut(1).expect("a conexão").host = "outro.exemplo".into();
    pasta.semear(&tree);
    let mut m = tela(&pasta);
    responder(&m, 1, SAUDAVEL);
    abrir(&mut m, 1);
    esperar_rodar(&mut m, &[1]);
}

#[test]
fn sem_conseguir_rodar_fica_em_silencio_e_nao_guarda() {
    let pasta = pasta_com_arvore("saude-erro");
    let mut m = tela(&pasta);
    m.opener
        .respostas_discos
        .lock()
        .unwrap()
        .insert(1, Err("a conexão caiu".to_owned()));
    abrir(&mut m, 1);
    esperar_rodar(&mut m, &[1]);
    assert!(m.app.notifications().is_empty());
    assert!(!pasta.caminho().join("saude_discos.json").exists());
    // Na próxima aba, tenta de novo.
    abrir(&mut m, 1);
    esperar_rodar(&mut m, &[1, 1]);
}

#[test]
fn desligada_nas_configuracoes_nao_roda_e_a_conexao_pode_ligar_so_para_ela() {
    let pasta = pasta_com_arvore("saude-desligada");
    let mut tree = arvore();
    tree.connection_mut(2)
        .expect("a conexão")
        .diagnostico_discos = DiagnosticoDeDiscos::Ligado;
    pasta.semear(&tree);
    pasta.configurar("geral", "pagina_inicial", json!("terminais"));
    pasta.configurar("geral", "diagnostico_discos", json!(false));
    let mut m = tela(&pasta);
    abrir(&mut m, 1);
    um_tempo(&mut m.app);
    assert!(rodou_em(&m).is_empty());
    abrir(&mut m, 2);
    esperar_rodar(&mut m, &[2]);
}

#[test]
fn a_pasta_desligada_vale_para_as_conexoes_dela() {
    let pasta = pasta_com_arvore("saude-pasta");
    let mut m = tela(&pasta);
    m.app.expand("arvore", "5");
    abrir(&mut m, 6);
    um_tempo(&mut m.app);
    assert!(rodou_em(&m).is_empty());
}

#[test]
fn nem_tenta_num_shell_local_nem_num_servidor_windows() {
    let pasta = pasta_com_arvore("saude-sem-sh");
    let mut m = tela(&pasta);
    abrir(&mut m, 3);
    abrir(&mut m, 4);
    um_tempo(&mut m.app);
    assert!(rodou_em(&m).is_empty());
}

#[test]
fn a_linha_da_grade_grava_a_escolha_da_conexao() {
    let pasta = pasta_com_arvore("saude-linha");
    let mut m = tela(&pasta);
    m.app.click("arvore-1");
    m.app.frame();
    let linhas: Vec<String> = m
        .app
        .properties(GRADE)
        .into_iter()
        .filter(|p| !p.hidden)
        .map(|p| p.name)
        .collect();
    assert!(linhas.iter().any(|l| l == "Saúde dos discos"), "{linhas:?}");

    // Desligada na linha, a conexão abre sem ler os discos.
    escolher_propriedade(&mut m.app, "Saúde dos discos");
    m.app.select("props-Saúde dos discos-valor", "Desligado");
    m.app.frame();
    abrir(&mut m, 1);
    um_tempo(&mut m.app);
    assert!(
        m.opener.execucoes_discos.lock().unwrap().is_empty(),
        "leu desligada"
    );
}

/// A tela com a fonte de exemplo e o diagnóstico deste computador respondendo `diagnostico`.
fn tela_com_local(nome: &str, diagnostico: Diagnostico) -> (Pasta, Montagem) {
    let pasta = pasta_com_arvore(nome);
    let fonte = FonteDeTeste::exemplo();
    let (m, _) = montar_com_ambiente(&pasta, Theme::Light, move |a| {
        a.processos = std::rc::Rc::new(move |_| Box::new(fonte.clone()));
        a.ritmo_dos_processos = Duration::from_millis(50);
        a.saude_local = Arc::new(move |_| diagnostico.clone());
    });
    (pasta, m)
}

#[test]
fn a_aba_disco_le_este_computador_so_quando_abre_e_verificar_le_de_novo() {
    let pasta = pasta_com_arvore("saude-local");
    let fonte = FonteDeTeste::exemplo();
    let vezes = Arc::new(AtomicUsize::new(0));
    let v = Arc::clone(&vezes);
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, move |a| {
        a.processos = std::rc::Rc::new(move |_| Box::new(fonte.clone()));
        a.ritmo_dos_processos = Duration::from_millis(50);
        a.saude_local = Arc::new(move |_| {
            // A segunda leitura acha o disco falhando.
            let vez = v.fetch_add(1, Ordering::SeqCst);
            ler_saida(if vez == 0 { SAUDAVEL } else { FALHANDO })
        });
    });
    um_tempo(&mut m.app);
    assert_eq!(vezes.load(Ordering::SeqCst), 0, "leu antes de abrir a aba");
    abrir_aba_disco(&mut m);
    esperar(&mut m.app, "o estado", |a| aparece(a, DETALHES));
    assert_eq!(m.app.text(ESTADO), "Discos lidos: 1 · com alerta: 0");
    assert!(!aparece(&m.app, TABELA), "a tabela só com o botão");
    assert!(aparece(&m.app, CARGA), "as tabelas da aba ficam");
    m.app.assert_lint();
    m.app.click(DETALHES);
    esperar(&mut m.app, "a tabela", |a| {
        aparece(a, TABELA) && a.grid(TABELA).rows.len() == 1
    });
    assert!(!aparece(&m.app, CARGA), "a tabela fica no lugar delas");
    let celulas = &m.app.grid(TABELA).rows[0].cells;
    assert_eq!(celulas[0], "nvme0");
    assert_eq!(celulas[1], "SSD Rápido");
    assert_eq!(celulas[2], "OK");
    assert_eq!(celulas[3], "93%");
    assert_eq!(celulas[5], "26.280 h (3,0 anos)");
    assert_eq!(celulas[6], "412");
    assert_eq!(celulas[7], "—");
    assert!(!aparece(&m.app, ALERTA));
    assert_eq!(vezes.load(Ordering::SeqCst), 1);

    // "Verificar de novo" lê; o alerta abre a tabela, sem notificação: quem usa está olhando.
    m.app.click(DETALHES);
    assert!(aparece(&m.app, CARGA));
    m.app.click(VERIFICAR);
    esperar(&mut m.app, "o alerta", |a| aparece(a, ALERTA));
    assert_eq!(vezes.load(Ordering::SeqCst), 2);
    assert_eq!(m.app.grid(TABELA).rows[0].cells[2], "Falhando");
    assert_eq!(m.app.text(ESTADO), "Discos lidos: 1 · com alerta: 1");
    assert!(m.app.notifications().is_empty());
    m.app.assert_lint();

    // Quem fecha volta às tabelas da aba, com o resumo avisando.
    m.app.click(DETALHES);
    assert!(aparece(&m.app, CARGA));
    assert!(!aparece(&m.app, ALERTA));
    assert_eq!(m.app.text(ESTADO), "Discos lidos: 1 · com alerta: 1");
    m.app.assert_lint();
}

#[test]
fn desligado_a_aba_disco_nao_le_este_computador() {
    let pasta = pasta_com_arvore("saude-local-desligado");
    pasta.configurar("geral", "pagina_inicial", json!("terminais"));
    pasta.configurar("geral", "diagnostico_discos", json!(false));
    let fonte = FonteDeTeste::exemplo();
    let vezes = Arc::new(AtomicUsize::new(0));
    let v = Arc::clone(&vezes);
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, move |a| {
        a.processos = std::rc::Rc::new(move |_| Box::new(fonte.clone()));
        a.ritmo_dos_processos = Duration::from_millis(50);
        a.saude_local = Arc::new(move |_| {
            v.fetch_add(1, Ordering::SeqCst);
            Diagnostico::Virtual
        });
    });
    abrir_aba_disco(&mut m);
    um_tempo(&mut m.app);
    assert_eq!(vezes.load(Ordering::SeqCst), 0);
    assert!(!aparece(&m.app, VERIFICAR));
}

/// A tela com o diagnóstico deste computador respondendo `respostas`, em ordem, e anotando se cada
/// leitura podia chamar o `sudo`.
fn tela_que_anota_o_sudo(
    pasta: &Pasta,
    respostas: Vec<Diagnostico>,
) -> (Montagem, Arc<std::sync::Mutex<Vec<bool>>>) {
    let fonte = FonteDeTeste::exemplo();
    let pedidos: Arc<std::sync::Mutex<Vec<bool>>> = Arc::default();
    let anota = Arc::clone(&pedidos);
    let (m, _) = montar_com_ambiente(pasta, Theme::Light, move |a| {
        a.processos = std::rc::Rc::new(move |_| Box::new(fonte.clone()));
        a.ritmo_dos_processos = Duration::from_millis(50);
        a.saude_local = Arc::new(move |com_sudo| {
            let mut pedidos = anota.lock().unwrap();
            pedidos.push(com_sudo);
            respostas[pedidos.len() - 1].clone()
        });
    });
    (m, pedidos)
}

#[test]
fn so_o_botao_chama_o_sudo_e_o_resultado_fica_lembrado() {
    let pasta = pasta_com_arvore("saude-botao-sudo");
    let (mut m, pedidos) =
        tela_que_anota_o_sudo(&pasta, vec![Diagnostico::SemPermissao, ler_saida(SAUDAVEL)]);
    abrir_aba_disco(&mut m);
    esperar(&mut m.app, "o estado", |a| aparece(a, VERIFICAR));
    esperar(&mut m.app, "a leitura", |_| {
        pedidos.lock().unwrap().len() == 1
    });
    assert_eq!(*pedidos.lock().unwrap(), [false], "sozinho, sem sudo");
    m.app.click(VERIFICAR);
    esperar(&mut m.app, "os discos", |a| aparece(a, DETALHES));
    assert_eq!(*pedidos.lock().unwrap(), [false, true]);
    let gravado = std::fs::read_to_string(pasta.caminho().join("saude_discos.json")).unwrap();
    assert!(gravado.contains("\"com_sudo\": true"), "{gravado}");
}

#[test]
fn o_sudo_que_deixou_de_funcionar_nao_e_tentado_de_novo() {
    let pasta = pasta_com_arvore("saude-sudo-revogado");
    semear_leitura_vencida(&pasta, "local", "", true);
    let (mut m, pedidos) = tela_que_anota_o_sudo(&pasta, vec![Diagnostico::SemPermissao]);
    abrir_aba_disco(&mut m);
    esperar(&mut m.app, "a leitura", |_| {
        pedidos.lock().unwrap().len() == 1
    });
    assert_eq!(*pedidos.lock().unwrap(), [true], "já tinha funcionado aqui");
    esperar(&mut m.app, "o resultado gravado", |_| {
        std::fs::read_to_string(pasta.caminho().join("saude_discos.json"))
            .is_ok_and(|g| !g.contains("com_sudo"))
    });
}

#[test]
fn sem_o_que_mostrar_diz_por_que_e_sem_tabela() {
    for (nome, diagnostico, frase) in [
        ("saude-virtual", Diagnostico::Virtual, "Máquina virtual"),
        (
            "saude-permissao",
            Diagnostico::SemPermissao,
            "sudo sem senha",
        ),
        (
            "saude-sem-smartctl",
            Diagnostico::SemSmartctl,
            "não está instalado",
        ),
    ] {
        let (_pasta, mut m) = tela_com_local(nome, diagnostico);
        abrir_aba_disco(&mut m);
        esperar(&mut m.app, frase, |a| aparece(a, VERIFICAR));
        assert!(!aparece(&m.app, TABELA), "{nome}");
        assert!(m.app.notifications().is_empty(), "{nome}");
        assert!(m.app.text(ESTADO).contains(frase), "{nome}");
        assert!(!aparece(&m.app, DETALHES), "{nome}");
        m.app.assert_lint();
    }
}

#[test]
fn em_outro_sistema_o_bloco_nem_aparece() {
    let (_pasta, mut m) = tela_com_local("saude-outro", Diagnostico::OutroSistema);
    abrir_aba_disco(&mut m);
    um_tempo(&mut m.app);
    assert!(!aparece(&m.app, VERIFICAR));
}
