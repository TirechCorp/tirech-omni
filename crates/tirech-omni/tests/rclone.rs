//! O rclone no app (M33): a subpágina "Remotos do rclone" da página Arquivos, o aviso de
//! binário ausente, "Pôr na árvore", abrir um remoto e o formulário gerado pelo provedor.
//! Tudo contra o `rcd` falso do `tomni-ui-files`, sem rclone instalado.

mod comum;

#[path = "comum/rcd_falso.rs"]
mod rcd_falso;

use std::time::{Duration, Instant};

use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};
use rcd_falso::{Opcoes, RcdFalso};

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Espera o diálogo da pergunta abrir com um texto de ajuda que contém `trecho`. O controle da
/// resposta (o `For` do diálogo) chega no quadro seguinte ao do texto, então espera mais um.
fn esperar_pergunta(app: &mut TestApp, trecho: &str) {
    esperar(app, "a pergunta do rclone", |a| {
        a.is_open("rclone-pergunta-dialogo") && a.text("rclone-pergunta-ajuda").contains(trecho)
    });
    app.wait(Duration::from_millis(60));
}

/// Troca o texto do campo `id` por `texto`.
fn escrever(app: &mut TestApp, id: &str, texto: &str) {
    use tomni_ui::Modifiers;
    use tomni_ui::prelude::*;
    app.click(id);
    app.shortcut(Shortcut::new('A', Modifiers::CTRL));
    app.key(Key::Delete);
    app.type_text(texto);
}

/// A tela com o `rcd` falso ligado, dois remotos (`drive` e `fotos`) e um arquivo em `drive:`.
fn montar_rclone(nome: &str) -> (Pasta, Montagem, RcdFalso) {
    let pasta = Pasta::nova(nome);
    let falso = RcdFalso::iniciar();
    falso.configurar("drive", "drive");
    falso.configurar("fotos", "s3");
    falso.semear("drive:", "docs/a.txt", b"ola");
    falso.semear("drive:", "notas.txt", b"notas");
    let m = montar(&pasta, Theme::Light);
    *m.opener.rcd.lock().unwrap() = Some(falso.rcd());
    (pasta, m, falso)
}

/// Abre a subpágina e espera a lista.
fn abrir_remotos(m: &mut Montagem) {
    m.app.click("navegacao-arquivos");
    m.app.click("arquivos-remotos-rclone");
    esperar(&mut m.app, "a lista de remotos", |a| {
        !a.grid("rclone-remotos").keys().is_empty()
    });
}

#[test]
fn sem_o_binario_o_aviso_aparece_na_subpagina_e_na_pagina_arquivos() {
    let pasta = Pasta::nova("rclone-ausente");
    let mut m = montar(&pasta, Theme::Light);
    // O abridor falso sem `rcd` responde "não encontrado", como a busca no PATH sem o rclone.
    m.app.click("navegacao-arquivos");
    m.app.click("arquivos-remotos-rclone");
    esperar(&mut m.app, "o aviso de instalação", |a| {
        a.visible("rclone-instalar")
    });
    assert!(m.app.text("rclone-instalar").contains("não foi encontrado"));
    m.app.click("rclone-voltar");
    assert!(m.app.visible("arquivos-rclone-instalar"));
    // O resto do app segue funcionando.
    m.app.click("navegacao-terminais");
    assert_eq!(m.app.text("navegacao"), "terminais");
}

#[test]
fn a_subpagina_lista_os_remotos_em_ordem() {
    let (_pasta, mut m, _falso) = montar_rclone("rclone-lista");
    abrir_remotos(&mut m);
    assert_eq!(m.app.grid("rclone-remotos").keys(), ["drive", "fotos"]);
    assert!(!m.app.visible("rclone-instalar"));
}

#[test]
fn por_na_arvore_cria_a_conexao_e_abrir_mostra_os_arquivos_e_entra_nos_recentes() {
    let (pasta, mut m, _falso) = montar_rclone("rclone-arvore");
    abrir_remotos(&mut m);
    m.app.grid_select("rclone-remotos", &["drive"]);
    m.app.click("rclone-arvore");
    escrever(&mut m.app, "rclone-arvore-nome", "Meu drive");
    m.app.click("rclone-arvore-dialogo-primary");

    let tree = pasta.conexoes();
    assert_eq!(tree.connection_count(), 1);
    let c = tree.connections()[0].clone();
    assert_eq!((c.name.as_str(), c.kind), ("Meu drive", Kind::Rclone));
    assert_eq!(c.rclone.remote, "drive");
    assert!(!pasta.texto_conexoes().contains("senha"));

    m.app.click("navegacao-terminais");
    m.app.double_click(&format!("arvore-{}", c.id));
    // As linhas do DataGrid nascem um quadro depois das chaves: espera a linha da pasta.
    esperar(&mut m.app, "a listagem do remoto", |a| {
        a.tree().find("a1-remoto-docs").is_some()
            && a.grid("a1-remoto").keys().contains(&"notas.txt")
    });
    assert_eq!(m.app.tab_titles("arquivos-abas"), ["Meu drive"]);
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
fn abrir_pela_lista_cria_a_aba_sem_a_arvore() {
    let (pasta, mut m, _falso) = montar_rclone("rclone-abrir");
    abrir_remotos(&mut m);
    m.app.grid_select("rclone-remotos", &["drive"]);
    m.app.click("rclone-abrir");
    // A aba pode nascer um quadro depois do clique: espera ela existir antes de olhar.
    esperar(&mut m.app, "a listagem do remoto", |a| {
        a.tree().find("a1-remoto").is_some()
            && a.visible("a1-remoto")
            && a.grid("a1-remoto").keys().contains(&"notas.txt")
    });
    assert_eq!(m.app.tab_titles("arquivos-abas"), ["drive"]);
    assert_eq!(pasta.conexoes().connection_count(), 0);
}

#[test]
fn apagar_o_remoto_pede_confirmacao_e_tira_da_lista() {
    let (_pasta, mut m, falso) = montar_rclone("rclone-apagar");
    abrir_remotos(&mut m);
    m.app.grid_select("rclone-remotos", &["fotos"]);
    m.app.click("rclone-apagar");
    assert!(m.app.is_open("rclone-apagar-dialogo"));
    m.app.click("rclone-apagar-dialogo-primary");
    esperar(&mut m.app, "a lista sem o remoto", |a| {
        a.grid("rclone-remotos").keys() == ["drive"]
    });
    assert!(falso.valores_do_remoto("fotos").is_none());
}

// ---------------------------------------------------------------------------
// O formulário gerado pelo provedor
// ---------------------------------------------------------------------------

const GRADE: &str = "rclone-form-grade";

/// Os nomes das linhas do formulário, na ordem.
fn linhas_do_formulario(app: &TestApp) -> Vec<String> {
    app.properties(GRADE).into_iter().map(|p| p.name).collect()
}

fn valor_do_formulario(app: &TestApp, nome: &str) -> String {
    app.properties(GRADE)
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a linha {nome}"))
        .value
}

/// Escolhe a linha como uma pessoa: clica na primeira ("Nome") e desce com ↓.
fn escolher_linha(app: &mut TestApp, nome: &str) {
    use tomni_ui::Key;
    if app.focused().is_some_and(|f| f.starts_with(GRADE)) {
        app.key(Key::Home);
    } else {
        let alvo = app
            .properties(GRADE)
            .into_iter()
            .find(|p| p.enabled)
            .map(|p| p.name)
            .expect("alguma linha habilitada");
        let r = app.rect(&format!("{GRADE}-{alvo}"));
        app.click_at(
            tomni_ui::kurbo::Point::new(r.x0 + 24.0, (r.y0 + r.y1) / 2.0),
            1,
        );
    }
    for _ in 0..40 {
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
    panic!(
        "a linha {nome} não foi escolhida: {:?}",
        linhas_do_formulario(app)
    );
}

fn escolher_no_formulario(app: &mut TestApp, nome: &str, opcao: &str) {
    escolher_linha(app, nome);
    app.select(&format!("{GRADE}-{nome}-valor"), opcao);
}

fn editar_no_formulario(app: &mut TestApp, nome: &str, texto: &str) {
    use tomni_ui::Key;
    escolher_linha(app, nome);
    app.key(Key::Enter);
    let atual = app.text(&format!("{GRADE}-{nome}-valor"));
    app.key(Key::End);
    for _ in 0..atual.chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text(texto).key(Key::Enter);
}

/// Abre o formulário de um remoto novo e espera os tipos chegarem.
fn abrir_formulario(m: &mut Montagem) {
    abrir_remotos(m);
    m.app.click("rclone-novo");
    esperar(&mut m.app, "os tipos do rclone", |a| {
        a.visible("rclone-form") && !a.visible("rclone-form-carregando")
    });
}

#[test]
fn o_formulario_mostra_so_as_opcoes_que_valem_para_o_provedor() {
    let (_pasta, mut m, _falso) = montar_rclone("rclone-form-opcoes");
    abrir_formulario(&mut m);
    assert_eq!(linhas_do_formulario(&m.app), ["Nome", "Tipo"]);

    escolher_no_formulario(&mut m.app, "Tipo", "Provedor falso");
    // A opção escondida e a avançada não aparecem; `so_aws` só com a AWS.
    assert_eq!(
        linhas_do_formulario(&m.app),
        [
            "Nome",
            "Tipo",
            "provider",
            "url (obrigatório)",
            "usar_tls",
            "modo",
            "exemplo",
            "senha",
            "menos_aws"
        ]
    );
    assert_eq!(valor_do_formulario(&m.app, "modo"), "a — Modo A");

    m.app.click("rclone-form-avancadas");
    assert!(linhas_do_formulario(&m.app).contains(&"avancada".to_owned()));
    assert_eq!(valor_do_formulario(&m.app, "avancada"), "4");
    m.app.click("rclone-form-avancadas");
    assert!(!linhas_do_formulario(&m.app).contains(&"avancada".to_owned()));

    // Escolher o provedor troca as opções que valem.
    escolher_no_formulario(&mut m.app, "provider", "AWS — Amazon");
    let linhas = linhas_do_formulario(&m.app);
    assert!(linhas.contains(&"so_aws".to_owned()));
    assert!(!linhas.contains(&"menos_aws".to_owned()));
    escolher_no_formulario(&mut m.app, "provider", "Outro — Qualquer outro");
    let linhas = linhas_do_formulario(&m.app);
    assert!(!linhas.contains(&"so_aws".to_owned()));
    assert!(linhas.contains(&"menos_aws".to_owned()));
    m.app.assert_lint();
}

#[test]
fn criar_pelo_formulario_responde_as_duas_perguntas_e_guarda_a_senha_no_cofre() {
    let (pasta, mut m, falso) = montar_rclone("rclone-form-criar");
    abrir_formulario(&mut m);
    editar_no_formulario(&mut m.app, "Nome", "meu falso");
    escolher_no_formulario(&mut m.app, "Tipo", "Provedor falso");
    editar_no_formulario(&mut m.app, "url (obrigatório)", "https://exemplo.test");
    editar_no_formulario(&mut m.app, "senha", "segredo123");
    m.app.click("rclone-form-salvar");

    esperar_pergunta(&mut m.app, "Primeira pergunta");
    m.app.assert_lint();
    escrever(&mut m.app, "rclone-pergunta-texto", "um");
    m.app.click("rclone-pergunta-dialogo-primary");

    esperar_pergunta(&mut m.app, "Segunda");
    escrever(&mut m.app, "rclone-pergunta-texto", "dois");
    m.app.click("rclone-pergunta-dialogo-primary");

    esperar(&mut m.app, "o remoto na lista", |a| {
        a.visible("rclone-lista") && a.grid("rclone-remotos").keys().contains(&"meu falso")
    });
    assert!(m.app.text("rclone-aviso").contains("meu falso"));

    // O rclone recebeu os valores que mudaram do padrão, e só eles.
    let valores = falso.valores_do_remoto("meu falso").unwrap();
    assert_eq!(
        valores.get("url").map(String::as_str),
        Some("https://exemplo.test")
    );
    assert!(!valores.contains_key("usar_tls") && !valores.contains_key("modo"));

    // A senha foi para o cofre; nenhum arquivo do app a guarda em claro.
    let itens = pasta.cofre().itens().to_vec();
    let item = itens
        .iter()
        .find(|i| i.titulo == "rclone: meu falso")
        .expect("o item do remoto no cofre");
    assert_eq!(item.campos.len(), 1);
    assert_eq!(
        (
            item.campos[0].nome.as_str(),
            item.campos[0].valor.as_str(),
            item.campos[0].oculto
        ),
        ("senha", "segredo123", true)
    );
    assert!(!pasta.texto_cofre().contains("segredo123"));
    assert!(!pasta.texto_conexoes().contains("segredo123"));
}

#[test]
fn sem_a_caixa_do_cofre_a_senha_nao_e_guardada() {
    let (pasta, mut m, _falso) = montar_rclone("rclone-form-sem-cofre");
    abrir_formulario(&mut m);
    m.app.click("rclone-form-cofre");
    editar_no_formulario(&mut m.app, "Nome", "nuvem1");
    escolher_no_formulario(&mut m.app, "Tipo", "Memória falsa");
    m.app.click("rclone-form-salvar");
    esperar(&mut m.app, "o remoto na lista", |a| {
        a.visible("rclone-lista") && a.grid("rclone-remotos").keys().contains(&"nuvem1")
    });
    assert!(pasta.cofre().itens().is_empty());
}

#[test]
fn o_formulario_recusa_nome_repetido_obrigatoria_vazia_e_sem_tipo() {
    let (_pasta, mut m, falso) = montar_rclone("rclone-form-erros");
    abrir_formulario(&mut m);
    m.app.click("rclone-form-salvar");
    assert!(m.app.text("rclone-form-erro").contains("Escolha o tipo"));

    escolher_no_formulario(&mut m.app, "Tipo", "Provedor falso");
    editar_no_formulario(&mut m.app, "Nome", "drive");
    m.app.click("rclone-form-salvar");
    assert!(m.app.text("rclone-form-erro").contains("Já existe"));

    editar_no_formulario(&mut m.app, "Nome", "novo");
    m.app.click("rclone-form-salvar");
    assert!(
        m.app.text("rclone-form-erro").contains("url"),
        "a obrigatória"
    );
    assert!(falso.valores_do_remoto("novo").is_none(), "nada foi criado");
}

#[test]
fn cancelar_a_pergunta_apaga_o_remoto_inacabado() {
    let (_pasta, mut m, falso) = montar_rclone("rclone-form-cancelar");
    abrir_formulario(&mut m);
    editar_no_formulario(&mut m.app, "Nome", "meio");
    escolher_no_formulario(&mut m.app, "Tipo", "Provedor falso");
    editar_no_formulario(&mut m.app, "url (obrigatório)", "https://exemplo.test");
    m.app.click("rclone-form-salvar");
    esperar(&mut m.app, "a pergunta", |a| {
        a.is_open("rclone-pergunta-dialogo")
    });
    assert!(
        falso.valores_do_remoto("meio").is_some(),
        "o rclone já gravou"
    );
    m.app.click("rclone-pergunta-dialogo-close");
    esperar(&mut m.app, "o remoto apagado", |_| {
        falso.valores_do_remoto("meio").is_none()
    });
}

#[test]
fn o_login_do_navegador_mostra_o_andamento_e_termina_com_o_remoto() {
    let (_pasta, mut m, _falso) = montar_rclone("rclone-form-oauth");
    abrir_formulario(&mut m);
    editar_no_formulario(&mut m.app, "Nome", "nuvem");
    escolher_no_formulario(&mut m.app, "Tipo", "Nuvem falsa com login");
    m.app.click("rclone-form-salvar");
    esperar_pergunta(&mut m.app, "auto config");
    assert!(
        m.app.visible("rclone-pergunta-login"),
        "o texto explicativo"
    );
    // O padrão é "Sim": abre o navegador e a chamada só volta no fim do login.
    assert_eq!(m.app.text("rclone-pergunta-escolha"), "Sim");
    m.app.click("rclone-pergunta-dialogo-primary");
    esperar(&mut m.app, "o diálogo do login", |a| {
        a.is_open("rclone-login-dialogo")
    });
    m.app.assert_lint();
    esperar(&mut m.app, "o remoto na lista", |a| {
        a.visible("rclone-lista") && a.grid("rclone-remotos").keys().contains(&"nuvem")
    });
    assert!(!m.app.is_open("rclone-login-dialogo"));
}

#[test]
fn sem_o_navegador_o_codigo_do_authorize_e_pedido() {
    let (_pasta, mut m, _falso) = montar_rclone("rclone-form-authorize");
    abrir_formulario(&mut m);
    editar_no_formulario(&mut m.app, "Nome", "remota");
    escolher_no_formulario(&mut m.app, "Tipo", "Nuvem falsa com login");
    m.app.click("rclone-form-salvar");
    esperar_pergunta(&mut m.app, "auto config");
    m.app.select("rclone-pergunta-escolha", "Não");
    m.app.click("rclone-pergunta-dialogo-primary");
    esperar_pergunta(&mut m.app, "authorize");
    escrever(
        &mut m.app,
        "rclone-pergunta-texto",
        "{\"access_token\":\"x\"}",
    );
    m.app.click("rclone-pergunta-dialogo-primary");
    esperar(&mut m.app, "o remoto na lista", |a| {
        a.visible("rclone-lista") && a.grid("rclone-remotos").keys().contains(&"remota")
    });
}

#[test]
fn editar_traz_os_valores_do_remoto_e_so_manda_o_que_mudou() {
    let (_pasta, mut m, falso) = montar_rclone("rclone-form-editar");
    // Um remoto `falso` já configurado, com a senha ofuscada pelo rclone.
    falso.configurar("velho", "falso");
    abrir_remotos(&mut m);
    m.app.grid_select("rclone-remotos", &["velho"]);
    m.app.click("rclone-editar");
    esperar(&mut m.app, "o formulário preenchido", |a| {
        a.visible("rclone-form")
            && linhas_do_formulario(a).contains(&"url (obrigatório)".to_owned())
    });
    assert_eq!(valor_do_formulario(&m.app, "Nome"), "velho");
    assert_eq!(valor_do_formulario(&m.app, "Tipo"), "Provedor falso");
    let propriedades = m.app.properties(GRADE);
    assert!(
        !propriedades[0].enabled && !propriedades[1].enabled,
        "nome e tipo fixos"
    );
    // A senha aparece vazia: deixar assim mantém a atual.
    assert_eq!(valor_do_formulario(&m.app, "senha"), "");
    let antes = falso.valores_do_remoto("velho").unwrap();

    editar_no_formulario(&mut m.app, "url (obrigatório)", "https://novo.test");
    m.app.click("rclone-form-salvar");
    esperar(&mut m.app, "voltar à lista", |a| a.visible("rclone-lista"));
    let depois = falso.valores_do_remoto("velho").unwrap();
    assert_eq!(
        depois.get("url").map(String::as_str),
        Some("https://novo.test")
    );
    assert_eq!(
        depois.get("senha"),
        antes.get("senha"),
        "a senha não foi tocada"
    );
}

#[test]
fn a_configuracao_cifrada_pede_a_senha_e_guarda_no_cofre() {
    let (pasta, mut m, _falso) = montar_rclone("rclone-config-cifrada");
    *m.opener.senha_da_config.lock().unwrap() = Some("certa".to_owned());
    m.app.click("navegacao-arquivos");
    m.app.click("arquivos-remotos-rclone");
    esperar(&mut m.app, "o pedido da senha", |a| {
        a.is_open("rclone-senha-dialogo")
    });
    m.app.assert_lint();
    escrever(&mut m.app, "rclone-senha-texto", "errada");
    m.app.click("rclone-senha-dialogo-primary");
    esperar(&mut m.app, "o pedido de novo", |a| {
        a.is_open("rclone-senha-dialogo") && a.visible("rclone-senha-erro")
    });
    escrever(&mut m.app, "rclone-senha-texto", "certa");
    m.app.click("rclone-senha-dialogo-primary");
    esperar(&mut m.app, "a lista de remotos", |a| {
        !a.grid("rclone-remotos").keys().is_empty()
    });
    let itens = pasta.cofre().itens().to_vec();
    let item = itens
        .iter()
        .find(|i| i.titulo == "Senha da configuração do rclone")
        .expect("a senha no cofre");
    assert_eq!(item.senha, "certa");
}

#[test]
fn com_a_senha_no_cofre_a_configuracao_cifrada_abre_sem_perguntar() {
    use tirech_omni::cofre::{Item, Tipo};
    let pasta = Pasta::nova("rclone-config-do-cofre");
    pasta.semear_cofre(vec![Item {
        tipo: Tipo::Rclone,
        titulo: "Senha da configuração do rclone".to_owned(),
        senha: "certa".to_owned(),
        ..Item::default()
    }]);
    let falso = RcdFalso::iniciar();
    falso.configurar("drive", "drive");
    let mut m = montar(&pasta, Theme::Light);
    *m.opener.rcd.lock().unwrap() = Some(falso.rcd());
    *m.opener.senha_da_config.lock().unwrap() = Some("certa".to_owned());
    abrir_remotos(&mut m);
    assert!(!m.app.is_open("rclone-senha-dialogo"));
    assert_eq!(m.app.grid("rclone-remotos").keys(), ["drive"]);
}

#[test]
fn a_versao_antiga_do_rclone_mostra_o_aviso_de_instalacao() {
    let pasta = Pasta::nova("rclone-antigo");
    let falso = RcdFalso::com(Opcoes {
        versao: [1, 50, 0],
        ..Opcoes::default()
    });
    let mut m = montar(&pasta, Theme::Light);
    *m.opener.rcd.lock().unwrap() = Some(falso.rcd());
    m.app.click("navegacao-arquivos");
    m.app.click("arquivos-remotos-rclone");
    esperar(&mut m.app, "o aviso de instalação", |a| {
        a.visible("rclone-instalar")
    });
    assert!(m.app.text("rclone-instalar").contains("1.50"));
}

/// A pasta remota padrão (M41) é relativa à raiz do remoto e vale também no rclone.
#[test]
fn pasta_remota_padrao_no_rclone() {
    let pasta = Pasta::nova("rclone-pasta-padrao");
    let falso = RcdFalso::iniciar();
    falso.configurar("drive", "drive");
    falso.semear("drive:", "docs/a.txt", b"ola");
    falso.semear("drive:", "notas.txt", b"notas");
    let mut c = Connection::new("Meu drive", Kind::Rclone);
    c.rclone.remote = "drive".into();
    c.pasta_remota = "docs".into();
    let mut tree = Tree::default();
    let id = tree.add_connection(None, c);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    *m.opener.rcd.lock().unwrap() = Some(falso.rcd());

    m.app.double_click(&format!("arvore-{id}"));
    esperar(&mut m.app, "a pasta docs do remoto", |a| {
        a.tree().find("a1-remoto").is_some() && a.grid("a1-remoto").keys().contains(&"a.txt")
    });
    assert!(!m.app.grid("a1-remoto").keys().contains(&"notas.txt"));
}
