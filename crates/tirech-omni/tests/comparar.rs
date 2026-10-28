//! A comparação das duas pastas da página Arquivos: as cores das linhas, o critério, o "Ocultar
//! idênticos", a navegação sincronizada e o que as Configurações e a conexão decidem ao abrir. O
//! servidor é um `MemFs` e o disco também (os dois em `/home/ana`, salvo onde o teste diz).

mod comum;

use std::time::{Duration, Instant, UNIX_EPOCH};

use serde_json::json;
use tirech_omni::modelo::{Ativar, Connection, Kind, Tree};
use tomni_ui::Key;
use tomni_ui::files::MemFs;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar};

/// A conexão 1 (`servidor`, SFTP com chave), com `ajustar` aplicado.
fn arvore(ajustar: impl FnOnce(&mut Connection)) -> Tree {
    let mut tree = Tree::default();
    let mut sftp = Connection::new("servidor", Kind::Sftp);
    sftp.host = "arquivos.exemplo".into();
    sftp.user = "ana".into();
    sftp.key_file = Some("/chaves/ana".into());
    ajustar(&mut sftp);
    tree.add_connection(None, sftp);
    tree
}

/// Espera `pronto` ficar verdadeiro, deixando o app processar as respostas das threads.
fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// A tela com a conexão 1 salva, o `ajustar` das Configurações já gravado e o disco e o servidor
/// semeados por `semear`. Ainda sem abrir a aba.
fn montar_com(
    nome: &str,
    conexao: impl FnOnce(&mut Connection),
    config: &[(&str, serde_json::Value)],
    semear: impl FnOnce(&MemFs, &MemFs),
) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore(conexao));
    for (campo, valor) in config {
        pasta.configurar("arquivos", campo, valor.clone());
    }
    let m = montar(&pasta, Theme::Light);
    semear(&m.opener.disco, &m.opener.servidor(1));
    (pasta, m)
}

/// Abre os arquivos da conexão 1 e espera a listagem dos dois painéis.
fn abrir(m: &mut Montagem) {
    m.app.double_click("arvore-1");
    esperar(&mut m.app, "os dois painéis", |a| {
        a.grid("a1-local").keys().len() > 1 || a.grid("a1-remoto").keys().len() > 1
    });
    m.app.wait(Duration::from_millis(100));
}

/// O tom de cada linha de um painel, na ordem da tela, como `chave:tom` (`-` sem tom).
fn tons(app: &TestApp, id: &str) -> Vec<String> {
    app.grid(id)
        .rows
        .into_iter()
        .map(|l| format!("{}:{}", l.key, l.tone.as_deref().unwrap_or("-")))
        .collect()
}

fn chaves(app: &TestApp, id: &str) -> Vec<String> {
    app.grid(id).keys().into_iter().map(str::to_owned).collect()
}

/// Escolhe o critério da comparação pelo menu da aba 1.
fn escolher(app: &mut TestApp, item: &str) {
    app.click("a1-comparar-opcoes");
    app.click(item);
}

/// `a.txt` igual, `b.txt` com tamanho diferente, `so_local.txt` e `so_remoto.txt`.
fn arquivos_basicos(disco: &MemFs, servidor: &MemFs) {
    disco.put("/home/ana/a.txt", "igual");
    servidor.put("/home/ana/a.txt", "igual");
    disco.put("/home/ana/b.txt", "curto");
    servidor.put("/home/ana/b.txt", "bem mais comprido");
    disco.put("/home/ana/so_local.txt", "x");
    servidor.put("/home/ana/so_remoto.txt", "y");
}

#[test]
fn comparar_pinta_as_linhas_dos_dois_lados() {
    let (_pasta, mut m) = montar_com("comparar-pinta", |_| {}, &[], arquivos_basicos);
    abrir(&mut m);
    assert_eq!(
        tons(&m.app, "a1-local"),
        ["..:-", "a.txt:-", "b.txt:-", "so_local.txt:-"]
    );

    m.app.click("a1-comparar");
    assert!(m.app.checked("a1-comparar"));
    assert_eq!(
        tons(&m.app, "a1-local"),
        ["..:-", "a.txt:-", "b.txt:critical", "so_local.txt:caution"]
    );
    assert_eq!(
        tons(&m.app, "a1-remoto"),
        ["..:-", "a.txt:-", "b.txt:critical", "so_remoto.txt:caution"]
    );

    m.app.click("a1-comparar");
    assert_eq!(tons(&m.app, "a1-local")[3], "so_local.txt:-");
    m.app.assert_lint();
}

#[test]
fn comparar_por_data_marca_o_mais_novo() {
    let (_pasta, mut m) = montar_com(
        "comparar-data",
        |_| {},
        &[],
        |disco, servidor| {
            disco.put("/home/ana/novo.txt", "n");
            servidor.put("/home/ana/novo.txt", "n");
            disco.put("/home/ana/perto.txt", "p");
            servidor.put("/home/ana/perto.txt", "p");
            disco.put("/home/ana/velho.txt", "v");
            servidor.put("/home/ana/velho.txt", "v");
            let base = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
            // O local é mais novo em `novo`; o remoto, em `velho`; `perto` difere em 30 s.
            disco.touch("/home/ana/novo.txt", base + Duration::from_secs(3600));
            servidor.touch("/home/ana/novo.txt", base);
            disco.touch("/home/ana/velho.txt", base);
            servidor.touch("/home/ana/velho.txt", base + Duration::from_secs(7200));
            disco.touch("/home/ana/perto.txt", base + Duration::from_secs(30));
            servidor.touch("/home/ana/perto.txt", base);
        },
    );
    abrir(&mut m);
    m.app.click("a1-comparar");
    escolher(&mut m.app, "a1-comparar-data");

    assert_eq!(
        tons(&m.app, "a1-local"),
        ["..:-", "novo.txt:success", "perto.txt:-", "velho.txt:-"]
    );
    assert_eq!(
        tons(&m.app, "a1-remoto"),
        ["..:-", "novo.txt:-", "perto.txt:-", "velho.txt:success"]
    );

    m.app.assert_lint();
}

#[test]
fn ocultar_identicos_esconde_e_volta() {
    let (_pasta, mut m) = montar_com("comparar-ocultar", |_| {}, &[], arquivos_basicos);
    abrir(&mut m);
    m.app.click("a1-comparar");
    m.app.click("a1-comparar-opcoes");
    m.app.click("a1-ocultar-identicos");

    assert_eq!(chaves(&m.app, "a1-local"), ["..", "b.txt", "so_local.txt"]);
    assert_eq!(
        chaves(&m.app, "a1-remoto"),
        ["..", "b.txt", "so_remoto.txt"]
    );

    // Com a comparação desligada, os idênticos voltam.
    m.app.click("a1-comparar");
    assert_eq!(
        chaves(&m.app, "a1-local"),
        ["..", "a.txt", "b.txt", "so_local.txt"]
    );
}

#[test]
fn ocultar_identicos_tira_da_selecao_o_que_some() {
    let (_pasta, mut m) = montar_com("comparar-selecao", |_| {}, &[], arquivos_basicos);
    abrir(&mut m);
    m.app.grid_select("a1-local", &["a.txt", "b.txt"]);
    m.app.click("a1-comparar");
    m.app.click("a1-comparar-opcoes");
    m.app.click("a1-ocultar-identicos");
    assert_eq!(m.app.grid("a1-local").selected, ["b.txt"]);
}

#[test]
fn comparar_acompanha_a_navegacao() {
    let (_pasta, mut m) = montar_com(
        "comparar-navega",
        |_| {},
        &[],
        |disco, servidor| {
            disco.dir("/home/ana/sub");
            servidor.dir("/home/ana/sub");
            disco.put("/home/ana/sub/dado.bin", "1");
            servidor.put("/home/ana/sub/dado.bin", "12345");
        },
    );
    abrir(&mut m);
    m.app.click("a1-comparar");
    // As pastas só comparam a existência: `sub` está dos dois lados.
    assert_eq!(tons(&m.app, "a1-local"), ["..:-", "sub:-"]);

    // Num lado só: `dado.bin` ainda não existe no outro.
    duplo(&mut m.app, "a1-local-sub");
    esperar(&mut m.app, "a pasta sub local", |a| {
        a.grid("a1-local").keys().contains(&"dado.bin")
    });
    assert_eq!(tons(&m.app, "a1-local"), ["..:-", "dado.bin:caution"]);
    assert_eq!(tons(&m.app, "a1-remoto"), ["..:-", "sub:caution"]);

    // Nos dois lados, com tamanhos diferentes.
    duplo(&mut m.app, "a1-remoto-sub");
    esperar(&mut m.app, "a pasta sub remota", |a| {
        a.grid("a1-remoto").keys().contains(&"dado.bin")
    });
    assert_eq!(tons(&m.app, "a1-local"), ["..:-", "dado.bin:critical"]);
    assert_eq!(tons(&m.app, "a1-remoto"), ["..:-", "dado.bin:critical"]);
}

#[test]
fn o_dois_pontos_nao_e_pintado_nem_escondido() {
    let (_pasta, mut m) = montar_com(
        "comparar-dois-pontos",
        |_| {},
        &[],
        |disco, servidor| {
            disco.dir("/home/ana/sub");
            servidor.dir("/home/ana/sub");
            disco.put("/home/ana/sub/x", "1");
            servidor.put("/home/ana/sub/x", "1");
        },
    );
    abrir(&mut m);
    duplo(&mut m.app, "a1-local-sub");
    duplo(&mut m.app, "a1-remoto-sub");
    esperar(&mut m.app, "as duas pastas sub", |a| {
        a.grid("a1-local").keys().contains(&"x") && a.grid("a1-remoto").keys().contains(&"x")
    });
    m.app.click("a1-comparar");
    m.app.click("a1-comparar-opcoes");
    m.app.click("a1-ocultar-identicos");
    assert_eq!(tons(&m.app, "a1-local"), ["..:-"]);
    assert_eq!(tons(&m.app, "a1-remoto"), ["..:-"]);
}

#[test]
fn a_configuracao_liga_a_comparacao_ao_abrir() {
    let config = [
        ("comparar", json!(true)),
        ("comparar_modo", json!("data")),
        ("comparar_tolerancia_min", json!(0)),
        ("ocultar_identicos", json!(true)),
    ];
    let (_pasta, mut m) = montar_com(
        "comparar-config",
        |_| {},
        &config,
        |disco, servidor| {
            disco.put("/home/ana/mesma.txt", "m");
            servidor.put("/home/ana/mesma.txt", "m");
            disco.put("/home/ana/mudou.txt", "m");
            servidor.put("/home/ana/mudou.txt", "m");
            let base = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
            disco.touch("/home/ana/mesma.txt", base);
            servidor.touch("/home/ana/mesma.txt", base);
            disco.touch("/home/ana/mudou.txt", base + Duration::from_secs(30));
            servidor.touch("/home/ana/mudou.txt", base);
        },
    );
    abrir(&mut m);
    assert!(m.app.checked("a1-comparar"));
    // Modo data, tolerância 0 (30 s contam) e idênticos escondidos.
    assert_eq!(tons(&m.app, "a1-local"), ["..:-", "mudou.txt:success"]);
    assert_eq!(tons(&m.app, "a1-remoto"), ["..:-", "mudou.txt:-"]);
}

#[test]
fn a_conexao_decide_sobre_a_comparacao_e_padrao_segue_a_global() {
    for (conexao, global, esperado) in [
        (Ativar::Sim, false, true),
        (Ativar::Nao, true, false),
        (Ativar::Padrao, true, true),
        (Ativar::Padrao, false, false),
    ] {
        let (_pasta, mut m) = montar_com(
            &format!("comparar-conexao-{conexao:?}-{global}"),
            |c| c.comparar = conexao,
            &[("comparar", json!(global))],
            arquivos_basicos,
        );
        abrir(&mut m);
        assert_eq!(
            m.app.checked("a1-comparar"),
            esperado,
            "conexão {conexao:?}, global {global}"
        );
    }
}

/// Escreve `texto` no campo `id` e confirma, como uma pessoa.
fn escrever(app: &mut TestApp, id: &str, texto: &str) {
    app.click(id).key(Key::End);
    for _ in 0..app.text(id).chars().count() + 4 {
        app.key(Key::Backspace);
    }
    app.type_text(texto).key(Key::Enter);
}

/// O clique duplo na linha `id` de um painel, depois de ela aparecer: o campo do caminho muda
/// antes de a listagem nova chegar.
fn duplo(app: &mut TestApp, id: &str) {
    esperar(app, id, |a| a.tree().find(id).is_some());
    app.double_click(id);
}

/// Espera os campos de caminho dos dois painéis terem `local` e `remoto`.
fn esperar_caminhos(app: &mut TestApp, local: &str, remoto: &str) {
    esperar(app, &format!("{local} e {remoto}"), |a| {
        a.text("a1-local-caminho") == local && a.text("a1-remoto-caminho") == remoto
    });
}

/// Um site no disco (`/home/ana/site`) e outro no servidor (`/srv/www`), com as mesmas pastas
/// `css` e `js` e uma pasta só do servidor.
fn semear_o_site(disco: &MemFs, servidor: &MemFs) {
    disco.put("/home/ana/site/css/tema.css", "t");
    disco.put("/home/ana/site/js/app.js", "a");
    disco.put("/home/ana/site/leia.txt", "l");
    servidor.put("/srv/www/css/tema.css", "t");
    servidor.put("/srv/www/js/app.js", "a");
    servidor.dir("/srv/www/apenas-remota");
}

/// A conexão 1 abrindo no site do disco e no do servidor.
fn nas_pastas_do_site(c: &mut Connection) {
    c.pasta_local = "/home/ana/site".into();
    c.pasta_remota = "/srv/www".into();
}

/// Abre a aba com a sincronização desligada e espera os dois painéis nas pastas do site.
fn abrir_o_site(nome: &str) -> (Pasta, Montagem) {
    let (pasta, mut m) = montar_com(nome, nas_pastas_do_site, &[], semear_o_site);
    abrir(&mut m);
    esperar_caminhos(&mut m.app, "/home/ana/site", "/srv/www");
    (pasta, m)
}

#[test]
fn sincronizar_espelha_entrar_e_acima() {
    let (_pasta, mut m) = abrir_o_site("sincronizar-espelha");
    assert!(!m.app.checked("a1-sincronizar"));
    m.app.click("a1-sincronizar");
    assert!(m.app.checked("a1-sincronizar"));

    // Entrar no remoto leva o local junto.
    duplo(&mut m.app, "a1-remoto-css");
    esperar_caminhos(&mut m.app, "/home/ana/site/css", "/srv/www/css");
    assert!(m.app.checked("a1-sincronizar"));

    // Acima no local leva o remoto de volta.
    m.app.click("a1-local-acima");
    esperar_caminhos(&mut m.app, "/home/ana/site", "/srv/www");

    // Entrar no local, e o `..` do remoto.
    duplo(&mut m.app, "a1-local-js");
    esperar_caminhos(&mut m.app, "/home/ana/site/js", "/srv/www/js");
    esperar(&mut m.app, "a listagem de js", |a| {
        a.tree().find("a1-remoto-app.js").is_some()
    });
    duplo(&mut m.app, "a1-remoto-..");
    esperar_caminhos(&mut m.app, "/home/ana/site", "/srv/www");

    // O caminho digitado também espelha.
    escrever(&mut m.app, "a1-local-caminho", "/home/ana/site/css");
    esperar_caminhos(&mut m.app, "/home/ana/site/css", "/srv/www/css");

    assert!(m.app.checked("a1-sincronizar"));
    assert!(!m.app.visible("a1-aviso"));
    m.app.assert_lint();
}

#[test]
fn sem_sincronizar_cada_painel_anda_sozinho() {
    let (_pasta, mut m) = abrir_o_site("sincronizar-desligada");
    duplo(&mut m.app, "a1-remoto-css");
    esperar(&mut m.app, "o remoto em css", |a| {
        a.text("a1-remoto-caminho") == "/srv/www/css"
    });
    m.app.wait(Duration::from_millis(200));
    assert_eq!(m.app.text("a1-local-caminho"), "/home/ana/site");
}

#[test]
fn sincronizar_desliga_se_a_pasta_falta() {
    let (_pasta, mut m) = abrir_o_site("sincronizar-pasta-falta");
    m.app.click("a1-sincronizar");
    duplo(&mut m.app, "a1-remoto-apenas-remota");
    esperar(&mut m.app, "o aviso", |a| a.visible("a1-aviso"));

    assert!(!m.app.checked("a1-sincronizar"));
    assert_eq!(
        m.app.text("a1-aviso"),
        "Navegação sincronizada desligada: `/home/ana/site/apenas-remota` não existe no disco."
    );
    // O painel do usuário foi para o destino; o outro ficou onde estava, sem erro próprio.
    assert_eq!(m.app.text("a1-remoto-caminho"), "/srv/www/apenas-remota");
    assert_eq!(m.app.text("a1-local-caminho"), "/home/ana/site");
    assert!(!m.app.visible("a1-local-erro"));

    // Desligada, o outro painel não acompanha mais.
    m.app.click("a1-remoto-acima");
    esperar_caminhos(&mut m.app, "/home/ana/site", "/srv/www");
    m.app.assert_lint();
}

#[test]
fn sincronizar_desliga_se_a_pasta_falta_no_servidor() {
    let (_pasta, mut m) = abrir_o_site("sincronizar-pasta-falta-servidor");
    m.app.click("a1-sincronizar");
    duplo(&mut m.app, "a1-local-leia.txt");
    m.app.wait(Duration::from_millis(100));
    // Um arquivo não é pasta: nada acontece, a sincronização continua.
    assert!(m.app.checked("a1-sincronizar"));

    // O disco ganha uma pasta que o servidor não tem.
    m.opener.disco.dir("/home/ana/site/so-no-disco");
    m.app.click("a1-local-atualizar");
    esperar(&mut m.app, "a pasta nova", |a| {
        a.tree().find("a1-local-so-no-disco").is_some()
    });
    duplo(&mut m.app, "a1-local-so-no-disco");
    esperar(&mut m.app, "o aviso", |a| a.visible("a1-aviso"));
    assert!(!m.app.checked("a1-sincronizar"));
    assert_eq!(
        m.app.text("a1-aviso"),
        "Navegação sincronizada desligada: `/srv/www/so-no-disco` não existe no servidor."
    );
    assert_eq!(m.app.text("a1-remoto-caminho"), "/srv/www");
}

#[test]
fn sincronizar_desliga_fora_da_base() {
    let (_pasta, mut m) = abrir_o_site("sincronizar-fora-da-base");
    m.app.click("a1-sincronizar");

    // Um caminho digitado fora da base do servidor.
    escrever(&mut m.app, "a1-remoto-caminho", "/home/ana");
    esperar(&mut m.app, "o aviso", |a| a.visible("a1-aviso"));
    assert!(!m.app.checked("a1-sincronizar"));
    assert_eq!(
        m.app.text("a1-aviso"),
        "Navegação sincronizada desligada: `/home/ana` fica fora de `/srv/www`."
    );
    assert_eq!(m.app.text("a1-remoto-caminho"), "/home/ana");
    assert_eq!(m.app.text("a1-local-caminho"), "/home/ana/site");

    // Ligar de novo guarda o par de agora; o "Início" do disco (`/home/ana`) fica fora da
    // base local (`/home/ana/site`).
    m.app.click("a1-sincronizar");
    assert!(m.app.checked("a1-sincronizar"));
    m.app.click("a1-local-inicio");
    esperar(&mut m.app, "o início do disco", |a| {
        a.text("a1-local-caminho") == "/home/ana"
    });
    assert!(!m.app.checked("a1-sincronizar"));
    assert_eq!(
        m.app.text("a1-aviso"),
        "Navegação sincronizada desligada: `/home/ana` fica fora de `/home/ana/site`."
    );
}

#[test]
fn o_inicio_dentro_da_base_espelha_como_outra_navegacao() {
    let (_pasta, mut m) = montar_com(
        "sincronizar-inicio-dentro",
        |c| {
            c.pasta_local = "/home/ana".into();
            c.pasta_remota = "/home/ana".into();
        },
        &[],
        |disco, servidor| {
            disco.dir("/home/ana/sub");
            servidor.dir("/home/ana/sub");
        },
    );
    abrir(&mut m);
    esperar_caminhos(&mut m.app, "/home/ana", "/home/ana");
    m.app.click("a1-sincronizar");
    duplo(&mut m.app, "a1-local-sub");
    esperar_caminhos(&mut m.app, "/home/ana/sub", "/home/ana/sub");

    // O início é a base: cabe nela, então os dois voltam e a sincronização segue ligada.
    m.app.click("a1-remoto-inicio");
    esperar_caminhos(&mut m.app, "/home/ana", "/home/ana");
    assert!(m.app.checked("a1-sincronizar"));
    assert!(!m.app.visible("a1-aviso"));
}

#[test]
fn sincronizar_nao_ecoa() {
    let (_pasta, mut m) = abrir_o_site("sincronizar-sem-eco");
    m.app.click("a1-sincronizar");
    let servidor = m.opener.servidor(1);
    let (disco_antes, servidor_antes) = (m.opener.disco.lists(), servidor.lists());

    duplo(&mut m.app, "a1-remoto-css");
    esperar_caminhos(&mut m.app, "/home/ana/site/css", "/srv/www/css");
    // Deixa passar o que viesse depois: um eco listaria de novo.
    m.app.wait(Duration::from_millis(300));

    assert_eq!(
        servidor.lists() - servidor_antes,
        1,
        "listagens do servidor"
    );
    assert_eq!(
        m.opener.disco.lists() - disco_antes,
        1,
        "listagens do disco"
    );
}

#[test]
fn conexao_com_comparar_e_sincronizar_liga_ao_abrir() {
    for (conexao, global, esperado) in [
        (Ativar::Sim, false, true),
        (Ativar::Nao, true, false),
        (Ativar::Padrao, true, true),
        (Ativar::Padrao, false, false),
    ] {
        let (_pasta, mut m) = montar_com(
            &format!("sincronizar-conexao-{conexao:?}-{global}"),
            |c| {
                nas_pastas_do_site(c);
                c.comparar = conexao;
                c.sincronizar = conexao;
            },
            &[("comparar", json!(global)), ("sincronizar", json!(global))],
            semear_o_site,
        );
        abrir(&mut m);
        esperar_caminhos(&mut m.app, "/home/ana/site", "/srv/www");
        let caso = format!("conexão {conexao:?}, global {global}");
        assert_eq!(m.app.checked("a1-comparar"), esperado, "comparar, {caso}");
        assert_eq!(
            m.app.checked("a1-sincronizar"),
            esperado,
            "sincronizar, {caso}"
        );

        // A base é o par das pastas padrão: entrar em `css` no servidor leva o disco junto
        // (ligada), ou o deixa onde está (desligada).
        duplo(&mut m.app, "a1-remoto-css");
        esperar(&mut m.app, "o servidor em css", |a| {
            a.text("a1-remoto-caminho") == "/srv/www/css"
        });
        if esperado {
            esperar_caminhos(&mut m.app, "/home/ana/site/css", "/srv/www/css");
        } else {
            m.app.wait(Duration::from_millis(200));
            assert_eq!(m.app.text("a1-local-caminho"), "/home/ana/site", "{caso}");
        }
    }
}
