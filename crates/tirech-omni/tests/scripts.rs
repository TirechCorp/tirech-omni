//! Rodar um script em vários servidores (M42, fase D): a página Scripts, os servidores
//! escolhidos, a linha digitada e o arquivo, os resultados e a saída.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::abrir::Execucao;
use tomni_ui::terminal::ssh::ExecOutput;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar, so_com_senha};

/// A tela com a árvore de exemplo (grupo "Produção" aberto; SSH nos ids 2 e 3).
fn tela(nome: &str) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    (pasta, m)
}

/// Vai à página Scripts pelo menu Ferramentas.
fn abrir_pagina(m: &mut Montagem) {
    m.app.click("menus-ferramentas").click("scripts-menu");
    m.app.frame();
}

/// Espera `pronto` ficar verdadeiro, deixando a tela processar as respostas das threads.
fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Espera a rodada terminar (o botão Rodar volta a ficar habilitado).
fn esperar_fim(app: &mut TestApp) {
    esperar(app, "o fim da rodada", |a| a.enabled("scripts-rodar"));
}

fn comandos(m: &Montagem) -> Vec<Execucao> {
    m.opener.execucoes.lock().unwrap().clone()
}

fn pagina_atual(app: &TestApp) -> bool {
    app.visible("scripts-pagina")
}

#[test]
fn o_menu_ferramentas_abre_a_pagina_scripts() {
    let (_pasta, mut m) = tela("scripts-menu");
    abrir_pagina(&mut m);
    assert!(pagina_atual(&m.app));
    assert_eq!(m.app.text("scripts-titulo"), "Scripts");
    // Os servidores são só as conexões SSH.
    assert_eq!(m.app.grid("scripts-servidores").keys(), ["2", "3"]);
    assert!(m.app.grid("scripts-servidores").selected.is_empty());
    // Sem servidor escolhido, não dá para rodar.
    assert!(!m.app.enabled("scripts-rodar"));
}

#[test]
fn a_seleção_da_árvore_vem_escolhida() {
    let (_pasta, mut m) = tela("scripts-selecao");
    m.app.click("arvore-2");
    abrir_pagina(&mut m);
    assert_eq!(m.app.grid("scripts-servidores").selected, ["2"]);
}

#[test]
fn o_menu_de_contexto_do_grupo_escolhe_os_servidores_dele() {
    let (_pasta, mut m) = tela("scripts-contexto");
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-script");
    m.app.frame();
    assert!(pagina_atual(&m.app));
    assert_eq!(m.app.grid("scripts-servidores").selected, ["2", "3"]);
}

#[test]
fn o_menu_de_contexto_da_conexao_escolhe_so_ela() {
    let (_pasta, mut m) = tela("scripts-contexto-conexao");
    m.app.context_menu("arvore-3");
    m.app.click("menu-3-script");
    m.app.frame();
    assert_eq!(m.app.grid("scripts-servidores").selected, ["3"]);
}

#[test]
fn o_cartao_da_pagina_inicial_abre_os_scripts() {
    let pasta = Pasta::nova("scripts-inicio");
    pasta.semear(&exemplo());
    pasta.pagina_inicial_padrao("inicio");
    let mut m = comum::montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-scripts");
    m.app.frame();
    assert!(pagina_atual(&m.app));
}

#[test]
fn rodar_uma_linha_digitada_manda_o_comando_a_cada_servidor() {
    let (_pasta, mut m) = tela("scripts-linha");
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-servidores", &["2", "3"]);
    m.app.type_into("scripts-comando", "uptime");
    assert!(m.app.enabled("scripts-rodar"));
    m.app.click("scripts-rodar");
    esperar_fim(&mut m.app);

    let mut feitos = comandos(&m);
    feitos.sort();
    assert_eq!(
        feitos,
        [
            (2, "uptime".to_owned(), Vec::new()),
            (3, "uptime".to_owned(), Vec::new()),
        ]
    );
    assert_eq!(
        m.app.text("scripts-resumo"),
        "Os 2 servidores terminaram — 2 ok"
    );
    let linhas = m.app.grid("scripts-resultados");
    assert_eq!(linhas.keys(), ["2", "3"]);
    assert!(
        linhas.rows[0]
            .cells
            .iter()
            .any(|c| c.contains("ok de web-01"))
    );
    assert!(
        linhas.rows[1]
            .cells
            .iter()
            .any(|c| c.contains("ok de db-01"))
    );
}

#[test]
fn escolher_um_resultado_mostra_a_saida_dele() {
    let (_pasta, mut m) = tela("scripts-saida");
    m.opener.respostas_exec.lock().unwrap().insert(
        3,
        Ok(ExecOutput {
            status: Some(0),
            stdout: b"linha um\nlinha dois\n".to_vec(),
            stderr: Vec::new(),
        }),
    );
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-servidores", &["2", "3"]);
    m.app.type_into("scripts-comando", "cat /etc/motd");
    m.app.click("scripts-rodar");
    esperar_fim(&mut m.app);

    m.app.grid_select("scripts-resultados", &["3"]);
    m.app.frame();
    let saida = m.app.terminal_text("scripts-saida");
    assert!(saida.contains("linha um"), "{saida:?}");
    assert!(saida.contains("linha dois"), "{saida:?}");
    m.app.grid_select("scripts-resultados", &["2"]);
    m.app.frame();
    let saida = m.app.terminal_text("scripts-saida");
    assert!(saida.contains("ok de web-01"), "{saida:?}");
    assert!(!saida.contains("linha um"), "{saida:?}");
}

#[test]
fn quem_sai_com_erro_ou_nao_conecta_aparece_na_conta() {
    let (_pasta, mut m) = tela("scripts-falhas");
    m.opener.respostas_exec.lock().unwrap().insert(
        2,
        Ok(ExecOutput {
            status: Some(3),
            stdout: Vec::new(),
            stderr: b"deu ruim\n".to_vec(),
        }),
    );
    m.opener
        .respostas_exec
        .lock()
        .unwrap()
        .insert(3, Err("sem rota".to_owned()));
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-servidores", &["2", "3"]);
    m.app.type_into("scripts-comando", "false");
    m.app.click("scripts-rodar");
    esperar_fim(&mut m.app);

    assert_eq!(
        m.app.text("scripts-resumo"),
        "Os 2 servidores terminaram — 0 ok, 2 falharam"
    );
    let linhas = m.app.grid("scripts-resultados");
    assert!(linhas.rows[0].cells.iter().any(|c| c.contains("deu ruim")));
    assert!(linhas.rows[1].cells.iter().any(|c| c.contains("sem rota")));
}

#[test]
fn rodar_um_arquivo_manda_o_conteudo_ao_interpretador() {
    let pasta = Pasta::nova("scripts-arquivo");
    pasta.semear(&exemplo());
    let dir = pasta.caminho().join("scripts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("limpar.sh"), "#!/bin/sh\nrm -rf /tmp/cache\n").unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    abrir_pagina(&mut m);
    assert_eq!(m.app.grid("scripts-lista").keys(), ["linha", "limpar.sh"],);
    m.app.grid_select("scripts-lista", &["limpar.sh"]);
    m.app.grid_select("scripts-servidores", &["2"]);
    m.app.frame();
    assert!(m.app.enabled("scripts-rodar"));
    m.app.click("scripts-rodar");
    esperar_fim(&mut m.app);

    assert_eq!(
        comandos(&m),
        [(
            2,
            "sh -s".to_owned(),
            b"#!/bin/sh\nrm -rf /tmp/cache\n".to_vec()
        )]
    );
    assert_eq!(m.app.text("scripts-resumo"), "1 servidor terminou — 1 ok");
}

#[test]
fn o_interpretador_das_configuracoes_vale_para_os_arquivos() {
    let pasta = Pasta::nova("scripts-interpretador");
    pasta.semear(&exemplo());
    pasta.configurar("scripts", "interpretador", serde_json::json!("bash -s"));
    let dir = pasta.caminho().join("scripts");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.sh"), "echo oi\n").unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-lista", &["a.sh"]);
    m.app.grid_select("scripts-servidores", &["3"]);
    m.app.click("scripts-rodar");
    esperar_fim(&mut m.app);
    assert_eq!(
        comandos(&m),
        [(3, "bash -s".to_owned(), b"echo oi\n".to_vec())]
    );
}

#[test]
fn parar_deixa_os_que_esperam_sem_comecar() {
    let pasta = Pasta::nova("scripts-parar");
    pasta.semear(&exemplo());
    // Um servidor por vez, devagar: o segundo ainda espera quando o Parar chega.
    pasta.configurar("scripts", "paralelo", serde_json::json!(1));
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    *m.opener.atraso_exec.lock().unwrap() = Duration::from_millis(600);
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-servidores", &["2", "3"]);
    m.app.type_into("scripts-comando", "sleep 1");
    m.app.click("scripts-rodar");
    m.app.frame();
    assert!(!m.app.enabled("scripts-rodar"));
    assert!(m.app.enabled("scripts-parar"));
    m.app.click("scripts-parar");
    esperar_fim(&mut m.app);

    assert_eq!(comandos(&m).len(), 1, "só o primeiro chegou a rodar");
    let linhas = m.app.grid("scripts-resultados");
    assert!(
        linhas.rows[1].cells.iter().any(|c| c.contains("parado")),
        "{:?}",
        linhas.rows[1].cells
    );
}

#[test]
fn rodar_sem_texto_na_linha_avisa_e_nao_roda() {
    let (_pasta, mut m) = tela("scripts-vazio");
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-servidores", &["2"]);
    m.app.click("scripts-rodar");
    m.app.frame();
    assert!(comandos(&m).is_empty());
}

#[test]
fn a_senha_digitada_vai_so_aos_servidores_sem_senha_no_cofre() {
    let pasta = Pasta::nova("scripts-senha");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-servidores", &["1"]);
    assert!(m.app.tree().find("scripts-senha").is_some());
    m.app.type_into("scripts-senha", "segredo");
    m.app.type_into("scripts-comando", "id");
    m.app.click("scripts-rodar");
    esperar_fim(&mut m.app);
    assert_eq!(comandos(&m).len(), 1);
    // Sem "Salvar no cofre", nada vai para o cofre.
    assert!(pasta.cofre().itens().is_empty());
}

#[test]
fn salvar_a_senha_no_cofre_guarda_e_some_com_o_campo() {
    let pasta = Pasta::nova("scripts-senha-cofre");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    abrir_pagina(&mut m);
    m.app.grid_select("scripts-servidores", &["1"]);
    m.app.type_into("scripts-senha", "segredo");
    m.app.click("scripts-salvar-senha");
    m.app.type_into("scripts-comando", "id");
    m.app.click("scripts-rodar");
    esperar_fim(&mut m.app);
    m.app.frame();
    // Com a senha no cofre, a caixa não é mais necessária.
    assert!(!m.app.tree().find("scripts-senha").is_some());
    assert!(pasta.cofre().itens().iter().any(|i| i.senha == "segredo"));
}

#[test]
fn novo_script_cria_o_arquivo_abre_no_editor_e_entra_na_lista() {
    let (pasta, mut m) = tela("scripts-novo");
    abrir_pagina(&mut m);
    m.app.click("scripts-novo");
    m.app.type_into("scripts-novo-nome", "reiniciar");
    m.app.click("scripts-novo-dialogo-primary");
    m.app.frame();

    let arquivo = pasta.caminho().join("scripts").join("reiniciar.sh");
    assert!(arquivo.exists(), "{arquivo:?}");
    let conteudo = std::fs::read_to_string(&arquivo).unwrap();
    assert!(conteudo.starts_with("#!/bin/sh"), "{conteudo:?}");
    assert_eq!(m.opener.externos.borrow().as_slice(), [arquivo]);
    assert!(m.app.grid("scripts-lista").keys().contains(&"reiniciar.sh"));
    assert_eq!(m.app.grid("scripts-lista").selected, ["reiniciar.sh"]);
}

#[test]
fn atualizar_le_a_pasta_de_novo() {
    let (pasta, mut m) = tela("scripts-atualizar");
    abrir_pagina(&mut m);
    assert_eq!(m.app.grid("scripts-lista").keys(), ["linha"]);
    std::fs::write(pasta.caminho().join("scripts").join("novo.sh"), "ls\n").unwrap();
    m.app.click("scripts-atualizar");
    m.app.frame();
    assert_eq!(m.app.grid("scripts-lista").keys(), ["linha", "novo.sh"]);
}

#[test]
fn a_pasta_dos_scripts_vem_das_configuracoes() {
    let pasta = Pasta::nova("scripts-pasta");
    pasta.semear(&exemplo());
    let outra = pasta.caminho().join("meus");
    std::fs::create_dir_all(&outra).unwrap();
    std::fs::write(outra.join("x.sh"), "ls\n").unwrap();
    pasta.configurar(
        "scripts",
        "pasta",
        serde_json::json!(outra.to_string_lossy()),
    );
    let mut m = montar(&pasta, Theme::Light);
    abrir_pagina(&mut m);
    assert_eq!(m.app.grid("scripts-lista").keys(), ["linha", "x.sh"]);
}
