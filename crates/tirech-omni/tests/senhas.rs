//! As senhas no cofre: abrir sem diálogo, "Salvar a senha", a recusa do servidor, as linhas
//! "Credencial" e "Senha" das propriedades, os modos do agente SSH e o desbloqueio ao iniciar.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::abrir::Senhas;
use tirech_omni::cofre::Tipo;
use tirech_omni::modelo::{Connection, Jump, Kind, ModoCredencial, Tree};
use tomni_ui::Key;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{
    Pasta, editar_propriedade, escolher_propriedade, item_do_tipo, item_ssh, montar, so_com_senha,
    valor,
};

fn abas(app: &TestApp) -> usize {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.len())
        .sum()
}

fn esperar_dialogo(app: &mut TestApp, id: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !app.is_open(id) {
        assert!(Instant::now() < limite, "o diálogo {id} não abriu");
        app.wait(Duration::from_millis(20));
    }
}

/// Uma conexão SSH sem chave (id 1) ligada ao item `credencial` do cofre.
fn ligada_ao_cofre(credencial: u64) -> Tree {
    let mut tree = so_com_senha();
    tree.connection_mut(1).unwrap().credencial = Some(credencial);
    tree
}

#[test]
fn senha_salva_abre_sem_dialogo() {
    let pasta = Pasta::nova("salva-abre");
    let ids = pasta.semear_cofre(vec![item_ssh("ana@sem-chave", "ana", "s3nh4")]);
    pasta.semear(&ligada_ao_cofre(ids[0]));
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(!m.app.is_open("senha-dialogo"));
    assert_eq!(abas(&m.app), 1);
    assert_eq!(
        *m.opener.senhas.borrow(),
        [Senhas {
            conexao: Some("s3nh4".to_owned()),
            salto: None,
            chave: None,
            token: None,
            chave_cofre: None,
            rede: None,
        }]
    );
}

#[test]
fn salvar_a_senha_vem_marcada_cria_o_item_e_liga_a_conexao() {
    let pasta = Pasta::nova("salvar-marcada");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert!(m.app.checked("senha-salvar"), "marcada por padrão");
    assert!(m.app.enabled("senha-salvar"));
    assert!(m.app.text("senha-ajuda").contains("Salva no cofre"));
    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-primary");
    assert_eq!(abas(&m.app), 1);

    let cofre = pasta.cofre();
    assert_eq!(cofre.itens().len(), 1);
    let item = &cofre.itens()[0];
    assert_eq!(item.titulo, "ana@sem-chave.exemplo");
    assert_eq!(item.usuario, "ana");
    assert_eq!(item.senha, "segredo");
    assert_eq!(item.url, "sem-chave.exemplo:22");
    assert_eq!(cofre.caminho(item.pasta), "Conexões");
    assert_eq!(
        pasta.conexoes().connection(1).unwrap().credencial,
        Some(item.id)
    );

    // A senha não está em claro em nenhum arquivo.
    for texto in [pasta.texto_cofre(), pasta.texto_conexoes()] {
        assert!(!texto.contains("segredo"), "a senha foi gravada em claro");
    }

    // A próxima abertura não pergunta.
    m.app.double_click("arvore-1");
    assert!(!m.app.is_open("senha-dialogo"));
    assert_eq!(abas(&m.app), 2);
    assert_eq!(
        m.opener.senhas.borrow()[1].conexao.as_deref(),
        Some("segredo")
    );
}

#[test]
fn com_a_caixa_desmarcada_nada_e_gravado_e_o_proximo_clique_pergunta() {
    let pasta = Pasta::nova("salvar-desmarcada");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    m.app.click("senha-salvar");
    assert!(!m.app.checked("senha-salvar"));
    m.app.type_into("senha-campo", "segredo");
    m.app.click("senha-dialogo-primary");
    assert_eq!(abas(&m.app), 1);
    assert!(pasta.cofre().itens().is_empty());
    assert_eq!(pasta.conexoes().connection(1).unwrap().credencial, None);

    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert!(m.app.checked("senha-salvar"), "marcada a cada abertura");
}

#[test]
fn salvar_com_credencial_atualiza_o_item_em_vez_de_criar_outro() {
    let pasta = Pasta::nova("salvar-atualiza");
    // O item existe, mas sem senha: o diálogo aparece.
    let ids = pasta.semear_cofre(vec![item_ssh("ana", "ana", "")]);
    pasta.semear(&ligada_ao_cofre(ids[0]));
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    m.app.type_into("senha-campo", "nova");
    m.app.click("senha-dialogo-primary");
    let cofre = pasta.cofre();
    assert_eq!(cofre.itens().len(), 1);
    assert_eq!(cofre.itens()[0].senha, "nova");
}

#[test]
fn senha_recusada_pelo_servidor_volta_ao_dialogo_e_atualiza_o_item() {
    let pasta = Pasta::nova("recusada");
    let ids = pasta.semear_cofre(vec![item_ssh("ana", "ana", "errada")]);
    pasta.semear(&ligada_ao_cofre(ids[0]));
    let mut m = montar(&pasta, Theme::Light);
    *m.opener.senha_certa.borrow_mut() = Some("certa".to_owned());

    m.app.double_click("arvore-1");
    esperar_dialogo(&mut m.app, "senha-dialogo");
    assert!(m.app.visible("senha-recusada"));
    assert_eq!(
        m.app.text("senha-recusada"),
        "A senha salva foi recusada pelo servidor."
    );
    assert_eq!(abas(&m.app), 1, "a aba antiga, fechada, fica");

    m.app.type_into("senha-campo", "certa");
    m.app.click("senha-dialogo-primary");
    assert_eq!(abas(&m.app), 2);
    assert_eq!(pasta.cofre().itens()[0].senha, "certa");
    m.app.wait(Duration::from_millis(100));
    assert!(!m.app.is_open("senha-dialogo"), "a senha certa não volta");
    assert_eq!(m.opener.senhas.borrow().len(), 2);
}

#[test]
fn na_primeira_vez_o_dialogo_nao_fala_de_recusa() {
    let pasta = Pasta::nova("sem-recusa");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert!(!m.app.visible("senha-recusada"));
}

#[test]
fn o_salto_com_credencial_manda_a_senha_dele() {
    let pasta = Pasta::nova("salto");
    let ids = pasta.semear_cofre(vec![
        item_ssh("ana", "ana", "da-conexao"),
        item_ssh("bastião", "bia", "do-salto"),
    ]);
    let mut tree = ligada_ao_cofre(ids[0]);
    tree.connection_mut(1).unwrap().jump = Some(Jump {
        host: "bastiao.exemplo".into(),
        port: 2200,
        user: String::new(),
        credencial: Some(ids[1]),
    });
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(!m.app.is_open("senha-dialogo"));
    assert_eq!(
        *m.opener.senhas.borrow(),
        [Senhas {
            conexao: Some("da-conexao".to_owned()),
            salto: Some("do-salto".to_owned()),
            chave: None,
            token: None,
            chave_cofre: None,
            rede: None,
        }]
    );
}

#[test]
fn a_linha_senha_grava_no_item_e_sem_credencial_cria_e_liga() {
    let pasta = Pasta::nova("linha-senha");
    let ids = pasta.semear_cofre(vec![item_ssh("ana", "ana", "velha")]);
    pasta.semear(&ligada_ao_cofre(ids[0]));
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    assert_eq!(
        valor(&m.app, "Senha").chars().count(),
        "velha".len(),
        "mascarada"
    );
    editar_propriedade(&mut m.app, "Senha", "nova");
    assert_eq!(pasta.cofre().itens()[0].senha, "nova");

    // Apagar o texto apaga a senha; o item fica.
    editar_propriedade(&mut m.app, "Senha", "");
    assert_eq!(pasta.cofre().itens().len(), 1);
    assert_eq!(pasta.cofre().itens()[0].senha, "");

    // Uma conexão sem credencial: editar a senha cria o item e liga.
    let pasta = Pasta::nova("linha-senha-nova");
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    editar_propriedade(&mut m.app, "Senha", "criada");
    let cofre = pasta.cofre();
    assert_eq!(cofre.itens().len(), 1);
    assert_eq!(cofre.itens()[0].senha, "criada");
    assert_eq!(
        pasta.conexoes().connection(1).unwrap().credencial,
        Some(cofre.itens()[0].id)
    );
}

#[test]
fn a_linha_credencial_lista_os_itens_compativeis_e_liga_a_escolhida() {
    let pasta = Pasta::nova("linha-credencial");
    let ids = pasta.semear_cofre(vec![
        item_ssh("Beta", "beta", "pb"),
        item_do_tipo(Tipo::Nota, "Anotação"),
        item_ssh("Alfa", "alfa", "pa"),
    ]);
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    assert_eq!(valor(&m.app, "Credencial"), "Perguntar ao conectar");

    escolher_propriedade(&mut m.app, "Credencial");
    m.app.key(Key::F(4));
    assert!(m.app.is_open("props-Credencial-valor-lista"));
    assert!(
        m.app
            .visible("props-Credencial-valor-perguntar-ao-conectar")
    );
    assert!(
        m.app
            .visible(&format!("props-Credencial-valor-alfa--{}-", ids[2]))
    );
    assert!(
        m.app
            .visible(&format!("props-Credencial-valor-beta--{}-", ids[0]))
    );
    assert!(
        m.app
            .tree()
            .find(&format!("props-Credencial-valor-anotacao--{}-", ids[1]))
            .is_none(),
        "a nota não serve de credencial"
    );
    m.app
        .click(&format!("props-Credencial-valor-beta--{}-", ids[0]));
    assert_eq!(
        pasta.conexoes().connection(1).unwrap().credencial,
        Some(ids[0])
    );
    assert_eq!(valor(&m.app, "Credencial"), format!("Beta [{}]", ids[0]));
}

#[test]
fn credencial_apagada_do_cofre_vale_como_perguntar() {
    let pasta = Pasta::nova("credencial-sumiu");
    pasta.semear(&ligada_ao_cofre(99));
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert_eq!(valor(&m.app, "Credencial"), "Perguntar ao conectar");
}

#[test]
fn senha_mestra_ao_iniciar_pede_o_desbloqueio_antes_das_abas() {
    let pasta = Pasta::nova("mestra-inicio");
    let ids = pasta.semear_cofre(vec![item_ssh("ana", "ana", "s3nh4")]);
    pasta.cofre_com_senha_mestra("mestra");
    pasta.semear(&ligada_ao_cofre(ids[0]));
    std::fs::write(
        pasta.caminho().join("estado.json"),
        r#"{"grade":[1,1],"paineis":[[1]]}"#,
    )
    .unwrap();

    let mut m = montar(&pasta, Theme::Light);
    assert!(m.app.is_open("cofre-desbloquear"));
    assert_eq!(abas(&m.app), 0, "as abas esperam o desbloqueio");

    m.app.type_into("cofre-desbloquear-campo", "errada");
    m.app.click("cofre-desbloquear-primary");
    esperar_dialogo(&mut m.app, "cofre-desbloquear");
    assert!(m.app.visible("cofre-desbloquear-erro"));
    assert_eq!(abas(&m.app), 0);

    m.app.wait(Duration::from_millis(100));
    m.app.type_into("cofre-desbloquear-campo", "mestra");
    m.app.click("cofre-desbloquear-primary");
    m.app.wait(Duration::from_millis(100));
    assert!(!m.app.is_open("cofre-desbloquear"));
    assert!(!m.app.is_open("senha-dialogo"), "a senha vem do cofre");
    assert_eq!(abas(&m.app), 1);
    assert_eq!(
        m.opener.senhas.borrow()[0].conexao.as_deref(),
        Some("s3nh4")
    );
    if let Some(s) = Session::find("1-1") {
        s.dispose();
    }
}

#[test]
fn agora_nao_deixa_o_cofre_bloqueado_e_as_conexoes_perguntam() {
    let pasta = Pasta::nova("mestra-agora-nao");
    let ids = pasta.semear_cofre(vec![item_ssh("ana", "ana", "s3nh4")]);
    pasta.cofre_com_senha_mestra("mestra");
    pasta.semear(&ligada_ao_cofre(ids[0]));
    let mut m = montar(&pasta, Theme::Light);
    assert!(m.app.is_open("cofre-desbloquear"));
    m.app.click("cofre-desbloquear-close");
    assert!(!m.app.is_open("cofre-desbloquear"));

    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    assert!(!m.app.enabled("senha-salvar"));
    assert!(!m.app.checked("senha-salvar"));
    assert!(m.app.text("senha-ajuda").contains("Desbloqueie o cofre"));
    m.app.type_into("senha-campo", "digitada");
    m.app.click("senha-dialogo-primary");
    assert_eq!(abas(&m.app), 1);
    assert_eq!(
        m.opener.senhas.borrow()[0].conexao.as_deref(),
        Some("digitada")
    );

    // A linha "Senha" fica desabilitada e a ligação com o cofre continua.
    m.app.click("arvore-1");
    let linha = m
        .app
        .properties("props")
        .into_iter()
        .find(|p| p.name == "Senha")
        .unwrap();
    assert!(!linha.enabled);
    assert_eq!(
        pasta.conexoes().connection(1).unwrap().credencial,
        Some(ids[0])
    );
}

#[test]
fn conexao_ligada_a_uma_chave_do_cofre_abre_sem_pedir_senha_e_leva_a_chave() {
    use tirech_omni::cofre::{ChaveSsh, Item};
    use tomni_ui::terminal::ssh::keys::{KeyFile, KeyGen, KeyType};

    let gerada = KeyGen::new(KeyType::Ed25519)
        .comment("ana@teste")
        .generate()
        .unwrap();
    let aberta = KeyFile::from_text(gerada.private_openssh().as_str(), None).unwrap();

    let pasta = Pasta::nova("chave-do-cofre-abre");
    let ids = pasta.semear_cofre(vec![Item {
        tipo: Tipo::ChaveSsh,
        titulo: "github".to_owned(),
        chave: Some(ChaveSsh::de_chave(aberta.private_key()).unwrap()),
        ..Item::default()
    }]);
    let (cofre, _) = tirech_omni::cofre::Cofre::abrir(pasta.caminho());
    let uid = cofre.item(ids[0]).unwrap().uid.clone();

    let mut tree = so_com_senha();
    tree.connection_mut(1).unwrap().chave_do_cofre = Some(uid);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert!(
        !m.app.is_open("senha-dialogo"),
        "a chave do cofre dispensa a senha"
    );
    assert_eq!(abas(&m.app), 1);
    let senhas = m.opener.senhas.borrow();
    assert_eq!(senhas.len(), 1);
    assert_eq!(senhas[0].chave_cofre.as_ref(), Some(aberta.private_key()));
    assert_eq!(senhas[0].conexao, None);
}

/// A conexão SSH sem chave (id 1) com a credencial no modo `modo`.
fn no_modo(modo: ModoCredencial) -> Tree {
    let mut tree = so_com_senha();
    tree.connection_mut(1).unwrap().credencial_modo = modo;
    tree
}

/// Espera um pouco sem que o diálogo `id` abra.
fn nao_abre(app: &mut TestApp, id: &str) {
    for _ in 0..10 {
        app.wait(Duration::from_millis(20));
        assert!(!app.is_open(id), "o diálogo {id} abriu");
    }
}

#[test]
fn agente_ssh_abre_sem_dialogo_e_a_recusa_so_fica_na_aba() {
    let pasta = Pasta::nova("modo-agente");
    pasta.semear(&no_modo(ModoCredencial::Agente));
    let mut m = montar(&pasta, Theme::Light);
    // O servidor falso recusa quem chega sem a senha: o agente não basta.
    *m.opener.senha_certa.borrow_mut() = Some("certa".to_owned());
    m.app.double_click("arvore-1");
    assert!(!m.app.is_open("senha-dialogo"), "não pergunta antes");
    assert_eq!(abas(&m.app), 1);
    assert_eq!(m.opener.senhas.borrow()[0].conexao, None);
    nao_abre(&mut m.app, "senha-dialogo");
    assert_eq!(m.opener.senhas.borrow().len(), 1, "não reconecta");
    assert_eq!(abas(&m.app), 1, "a aba fica, com o erro");
}

#[test]
fn agente_ssh_senao_a_senha_tenta_o_agente_e_a_recusa_pede_a_senha() {
    let pasta = Pasta::nova("modo-agente-ou-senha");
    pasta.semear(&no_modo(ModoCredencial::AgenteOuSenha));
    let mut m = montar(&pasta, Theme::Light);
    *m.opener.senha_certa.borrow_mut() = Some("certa".to_owned());
    m.app.double_click("arvore-1");
    // Abriu sem a senha (o diálogo não veio antes): só o agente.
    assert_eq!(abas(&m.app), 1);
    assert_eq!(m.opener.senhas.borrow()[0].conexao, None);

    esperar_dialogo(&mut m.app, "senha-dialogo");
    assert!(m.app.visible("senha-recusada"));
    assert_eq!(
        m.app.text("senha-recusada"),
        "O servidor não aceitou o agente SSH nem as chaves."
    );
    m.app.click("senha-salvar");
    m.app.type_into("senha-campo", "certa");
    m.app.click("senha-dialogo-primary");
    assert_eq!(abas(&m.app), 2);
    assert_eq!(
        m.opener.senhas.borrow()[1].conexao.as_deref(),
        Some("certa"),
        "a reconexão leva a senha"
    );
    nao_abre(&mut m.app, "senha-dialogo");
    // Sem "Salvar a senha", nada vai para o cofre e o modo continua.
    assert!(pasta.cofre().itens().is_empty());
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!(c.credencial, None);
    assert_eq!(c.credencial_modo, ModoCredencial::AgenteOuSenha);
}

#[test]
fn agente_ssh_senao_a_senha_aceito_pelo_servidor_nao_pergunta_nada() {
    let pasta = Pasta::nova("modo-agente-ou-senha-aceito");
    pasta.semear(&no_modo(ModoCredencial::AgenteOuSenha));
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    assert_eq!(abas(&m.app), 1);
    nao_abre(&mut m.app, "senha-dialogo");
}

#[test]
fn a_linha_credencial_escolhe_o_modo_e_perguntar_volta_a_pedir_antes() {
    let pasta = Pasta::nova("linha-modo");
    let ids = pasta.semear_cofre(vec![item_ssh("Alfa", "alfa", "pa")]);
    pasta.semear(&so_com_senha());
    let mut m = montar(&pasta, Theme::Light);

    // "Perguntar ao conectar" continua pedindo antes de abrir.
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
    m.app.click("senha-dialogo-close");

    m.app.click("arvore-1");
    escolher_propriedade(&mut m.app, "Credencial");
    m.app.key(Key::F(4));
    assert!(m.app.is_open("props-Credencial-valor-lista"));
    assert!(
        m.app
            .visible("props-Credencial-valor-perguntar-ao-conectar")
    );
    assert!(m.app.visible("props-Credencial-valor-agente-ssh"));
    assert!(
        m.app
            .visible("props-Credencial-valor-agente-ssh--senão-a-senha")
    );
    assert!(
        m.app
            .visible(&format!("props-Credencial-valor-alfa--{}-", ids[0]))
    );
    m.app.click("props-Credencial-valor-agente-ssh");
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!(c.credencial_modo, ModoCredencial::Agente);
    assert_eq!(c.credencial, None);
    assert_eq!(valor(&m.app, "Credencial"), "Agente SSH");

    m.app.double_click("arvore-1");
    assert!(!m.app.is_open("senha-dialogo"), "o agente não pergunta");
    assert_eq!(abas(&m.app), 1);

    // Um item do cofre vence: o modo volta ao padrão.
    escolher_propriedade(&mut m.app, "Credencial");
    m.app.key(Key::F(4));
    m.app
        .click(&format!("props-Credencial-valor-alfa--{}-", ids[0]));
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!(c.credencial, Some(ids[0]));
    assert_eq!(c.credencial_modo, ModoCredencial::Perguntar);

    // De volta a "Perguntar ao conectar": sem item, a senha é pedida antes.
    escolher_propriedade(&mut m.app, "Credencial");
    m.app.key(Key::F(4));
    m.app.click("props-Credencial-valor-perguntar-ao-conectar");
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!(c.credencial, None);
    assert_eq!(c.credencial_modo, ModoCredencial::Perguntar);
    m.app.double_click("arvore-1");
    assert!(m.app.is_open("senha-dialogo"));
}

#[test]
fn um_tipo_sem_ssh_nao_lista_os_modos_do_agente() {
    let pasta = Pasta::nova("modo-ftp");
    let mut tree = Tree::default();
    let mut c = Connection::new("ftp", Kind::Ftp);
    c.host = "ftp.exemplo".into();
    c.user = "ana".into();
    tree.add_connection(None, c);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    escolher_propriedade(&mut m.app, "Credencial");
    m.app.key(Key::F(4));
    assert!(m.app.is_open("props-Credencial-valor-lista"));
    assert!(
        m.app
            .visible("props-Credencial-valor-perguntar-ao-conectar")
    );
    for id in [
        "props-Credencial-valor-agente-ssh",
        "props-Credencial-valor-agente-ssh--senão-a-senha",
    ] {
        assert!(m.app.tree().find(id).is_none(), "{id} não vale no FTP");
    }
}
