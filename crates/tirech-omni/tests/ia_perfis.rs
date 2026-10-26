//! A página Assistentes de IA (M52): criar um provedor pelo diálogo, a chave no cofre (nunca no
//! `ia.json`), "Testar" e "Atualizar" contra o servidor falso, o padrão, apagar e o aviso de
//! internet.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::assistente::perfis::{ArquivoIa, PerfilIa, Preset};
use tirech_omni::cofre::{Item, Tipo};
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::theme::Theme;
use tomni_ui_ai::test_util::FakeAi;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, montar, montar_pura, valor};

/// O `ia.json` lido de volta.
fn gravado(pasta: &Pasta) -> ArquivoIa {
    ArquivoIa::carregar(&pasta.armazenamento()).0
}

/// O texto do `ia.json`.
fn texto_do_arquivo(pasta: &Pasta) -> String {
    std::fs::read_to_string(pasta.armazenamento().arquivo_ia()).unwrap_or_default()
}

/// Um provedor personalizado que fala com `url`.
fn perfil(id: u64, nome: &str, url: &str) -> PerfilIa {
    PerfilIa {
        id,
        nome: nome.into(),
        url: url.into(),
        modelo: "modelo-falso".into(),
        ..PerfilIa::default()
    }
}

/// Grava o `ia.json` com `perfis` antes de a tela abrir.
fn semear(pasta: &Pasta, perfis: Vec<PerfilIa>) {
    let arquivo = ArquivoIa {
        perfis,
        ..ArquivoIa::default()
    };
    arquivo
        .salvar(&pasta.armazenamento())
        .expect("gravar o ia.json do teste");
}

/// A tela aberta na página Assistentes de IA.
fn abrir(pasta: &Pasta) -> Montagem {
    let mut m = montar_pura(pasta, Theme::Light);
    m.app.click("navegacao-ia");
    m
}

/// Dá quadros até a condição valer (as threads do pedido acordam a tela quando terminam).
fn esperar(app: &mut TestApp, mut condicao: impl FnMut(&mut TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

fn digitar(app: &mut TestApp, id: &str, texto: &str) {
    app.click(id).shortcut('a').type_text(texto);
}

#[test]
fn sem_provedor_a_lista_diz_como_comecar() {
    let pasta = Pasta::nova("ia-vazia");
    let m = abrir(&pasta);
    assert!(m.app.visible("ia-perfis-vazia"));
    assert!(m.app.visible("ia-perfis-sem-escolha"));
    assert!(!m.app.enabled("ia-perfil-testar"));
    assert!(!m.app.enabled("ia-perfil-excluir"));
}

#[test]
fn criar_um_provedor_pelo_dialogo_de_escolha() {
    let pasta = Pasta::nova("ia-criar");
    let mut m = abrir(&pasta);
    m.app.click("ia-perfil-novo");
    assert!(m.app.visible("ia-escolher"));
    // Um cartão por provedor, o local primeiro.
    for chave in [
        "ollama",
        "lmstudio",
        "openrouter",
        "openai",
        "anthropic",
        "groq",
        "personalizado",
    ] {
        assert!(
            m.app.visible(&format!("ia-escolher-{chave}")),
            "falta o cartão {chave}"
        );
    }
    m.app.click("ia-escolher-ollama");
    assert!(!m.app.visible("ia-escolher"));

    let arquivo = gravado(&pasta);
    assert_eq!(arquivo.perfis.len(), 1, "{}", texto_do_arquivo(&pasta));
    let novo = &arquivo.perfis[0];
    assert_eq!(novo.preset, Preset::Ollama);
    assert!(novo.url.contains("11434"), "{}", novo.url);
    assert!(novo.credencial.is_none());

    // O formulário abre com o provedor escolhido.
    assert!(m.app.visible("ia-perfil-nome"));
    assert_eq!(m.app.text("ia-perfil-nome"), novo.nome);
    assert!(!m.app.visible("ia-perfis-sem-escolha"));
    // Na máquina: nenhum aviso de internet.
    assert!(!m.app.visible("ia-perfil-aviso-rede"));

    digitar(&mut m.app, "ia-perfil-nome", "Meu Ollama");
    digitar(&mut m.app, "ia-perfil-modelo", "llama3");
    let arquivo = gravado(&pasta);
    assert_eq!(arquivo.perfis[0].nome, "Meu Ollama");
    assert_eq!(arquivo.perfis[0].modelo, "llama3");
}

#[test]
fn dois_provedores_do_mesmo_tipo_ganham_nomes_diferentes() {
    let pasta = Pasta::nova("ia-nome-livre");
    let mut m = abrir(&pasta);
    for _ in 0..2 {
        m.app.click("ia-perfil-novo").click("ia-escolher-ollama");
    }
    let nomes: Vec<String> = gravado(&pasta)
        .perfis
        .iter()
        .map(|p| p.nome.clone())
        .collect();
    assert_eq!(nomes.len(), 2);
    assert_ne!(nomes[0], nomes[1]);
}

#[test]
fn a_url_fora_do_preset_vira_personalizado_e_na_internet_avisa() {
    let pasta = Pasta::nova("ia-internet");
    let mut m = abrir(&pasta);
    m.app.click("ia-perfil-novo").click("ia-escolher-ollama");
    assert!(!m.app.visible("ia-perfil-aviso-rede"));
    digitar(&mut m.app, "ia-perfil-url", "https://api.exemplo.com/v1");
    assert!(m.app.visible("ia-perfil-aviso-rede"));
    assert!(
        m.app
            .text("ia-perfil-aviso-rede")
            .contains("api.exemplo.com")
    );
    let arquivo = gravado(&pasta);
    assert_eq!(arquivo.perfis[0].preset, Preset::Personalizado);
    assert_eq!(arquivo.perfis[0].url, "https://api.exemplo.com/v1");
}

#[test]
fn a_chave_vai_para_o_cofre_e_nunca_para_o_ia_json() {
    let pasta = Pasta::nova("ia-chave");
    semear(&pasta, vec![perfil(1, "Falso", "http://127.0.0.1:9/v1")]);
    let mut m = abrir(&pasta);
    m.app.click("ia-perfis-arvore-1");
    assert_eq!(m.app.text("ia-perfil-chave-estado"), "Sem chave.");

    digitar(&mut m.app, "ia-perfil-chave", "sk-segredo-123");
    m.app.click("ia-perfil-chave-guardar");

    let arquivo = gravado(&pasta);
    let item = arquivo.perfis[0]
        .credencial
        .expect("o provedor aponta para o item");
    assert!(
        !texto_do_arquivo(&pasta).contains("sk-segredo-123"),
        "a chave não pode ir para o ia.json: {}",
        texto_do_arquivo(&pasta)
    );
    let cofre = pasta.cofre();
    let guardado = cofre.item(item).expect("o item está no cofre");
    assert_eq!(guardado.tipo, Tipo::Senha);
    assert_eq!(guardado.senha, "sk-segredo-123");
    assert_eq!(guardado.url, "http://127.0.0.1:9/v1");
    // O campo é limpo depois de guardar, e a linha mostra onde a chave está.
    assert_eq!(m.app.text("ia-perfil-chave"), "");
    assert!(
        m.app
            .text("ia-perfil-chave-estado")
            .contains(&guardado.titulo)
    );

    // Guardar de novo troca a senha do mesmo item, sem criar outro.
    digitar(&mut m.app, "ia-perfil-chave", "sk-outra-456");
    m.app.click("ia-perfil-chave-guardar");
    let cofre = pasta.cofre();
    assert_eq!(cofre.itens().len(), 1);
    assert_eq!(
        cofre.item(item).map(|i| i.senha.as_str()),
        Some("sk-outra-456")
    );
}

#[test]
fn testar_manda_a_chave_do_cofre_e_mostra_o_resultado() {
    let falso = FakeAi::novo();
    falso.responder(&["ok"]).exigir_chave("sk-teste");
    let pasta = Pasta::nova("ia-testar");
    semear(&pasta, vec![perfil(1, "Falso", &falso.url())]);
    let mut m = abrir(&pasta);
    m.app.click("ia-perfis-arvore-1");

    // A chave digitada e ainda não guardada vai para o cofre antes de testar.
    digitar(&mut m.app, "ia-perfil-chave", "sk-teste");
    m.app.click("ia-perfil-testar");
    esperar(
        &mut m.app,
        |app| app.visible("aviso-sucesso"),
        "o teste não terminou com sucesso",
    );
    let pedidos = falso.pedidos();
    assert!(!pedidos.is_empty());
    assert_eq!(
        pedidos[0].cabecalho("authorization"),
        Some("Bearer sk-teste")
    );
    let corpo = pedidos[0].json().expect("o corpo é JSON");
    assert_eq!(corpo["model"], "modelo-falso");
    assert!(gravado(&pasta).perfis[0].credencial.is_some());
    assert!(!texto_do_arquivo(&pasta).contains("sk-teste"));
}

#[test]
fn testar_com_erro_do_provedor_avisa_o_erro() {
    let falso = FakeAi::novo();
    falso.erro(401, r#"{"error":{"message":"chave ruim"}}"#);
    let pasta = Pasta::nova("ia-testar-erro");
    semear(&pasta, vec![perfil(1, "Falso", &falso.url())]);
    let mut m = abrir(&pasta);
    m.app.click("ia-perfis-arvore-1").click("ia-perfil-testar");
    esperar(
        &mut m.app,
        |app| app.visible("aviso-erro"),
        "o erro não apareceu",
    );
    assert!(!m.app.visible("aviso-sucesso"));
}

#[test]
fn atualizar_a_lista_pede_os_modelos_ao_provedor() {
    let falso = FakeAi::novo();
    falso.modelos(&["llama3", "mistral"]);
    let pasta = Pasta::nova("ia-modelos");
    semear(&pasta, vec![perfil(1, "Falso", &falso.url())]);
    let mut m = abrir(&pasta);
    m.app
        .click("ia-perfis-arvore-1")
        .click("ia-modelos-atualizar");
    esperar(
        &mut m.app,
        |app| app.enabled("ia-modelos-atualizar") && !falso.pedidos().is_empty(),
        "a lista de modelos não chegou",
    );
    let pedidos = falso.pedidos();
    assert_eq!(pedidos[0].metodo, "GET");
    assert!(
        pedidos[0].caminho.ends_with("/models"),
        "{}",
        pedidos[0].caminho
    );
    assert!(!m.app.visible("aviso-erro"));
}

#[test]
fn escolher_o_padrao_grava_no_arquivo() {
    let pasta = Pasta::nova("ia-padrao");
    semear(
        &pasta,
        vec![
            perfil(1, "Um", "http://127.0.0.1:9/v1"),
            perfil(2, "Dois", "http://127.0.0.1:9/v1"),
        ],
    );
    let mut m = abrir(&pasta);
    m.app.click("ia-perfis-arvore-2");
    assert!(m.app.enabled("ia-perfil-padrao"));
    m.app.click("ia-perfil-padrao");
    assert_eq!(gravado(&pasta).padrao, Some(2));
    assert!(!m.app.enabled("ia-perfil-padrao"));
    // O rótulo da lista marca qual é o padrão.
    assert!(m.app.label("ia-perfis-arvore-2").contains("Dois"));
    assert_ne!(m.app.label("ia-perfis-arvore-2"), "Dois");
}

#[test]
fn apagar_pede_confirmacao_e_deixa_o_item_do_cofre_por_padrao() {
    let pasta = Pasta::nova("ia-apagar");
    let item = pasta.semear_cofre(vec![Item {
        tipo: Tipo::Senha,
        titulo: "Chave de API — Falso".into(),
        senha: "sk-x".into(),
        ..Item::default()
    }])[0];
    let mut com_chave = perfil(1, "Falso", "http://127.0.0.1:9/v1");
    com_chave.credencial = Some(item);
    semear(
        &pasta,
        vec![com_chave, perfil(2, "Outro", "http://127.0.0.1:9/v1")],
    );
    let mut m = abrir(&pasta);

    m.app
        .click("ia-perfis-arvore-1")
        .click("ia-perfil-excluir")
        .click("ia-perfil-excluir-dialogo-close");
    assert_eq!(gravado(&pasta).perfis.len(), 2);

    m.app
        .click("ia-perfil-excluir")
        .click("ia-perfil-excluir-dialogo-primary");
    assert_eq!(gravado(&pasta).perfis.len(), 1);
    assert_eq!(gravado(&pasta).perfis[0].id, 2);
    assert!(m.app.visible("ia-perfis-sem-escolha"));
    assert!(pasta.cofre().item(item).is_some(), "o item fica por padrão");
}

#[test]
fn apagar_com_a_caixa_marcada_leva_o_item_do_cofre_junto() {
    let pasta = Pasta::nova("ia-apagar-item");
    let item = pasta.semear_cofre(vec![Item {
        tipo: Tipo::Senha,
        titulo: "Chave de API — Falso".into(),
        senha: "sk-x".into(),
        ..Item::default()
    }])[0];
    let mut com_chave = perfil(1, "Falso", "http://127.0.0.1:9/v1");
    com_chave.credencial = Some(item);
    semear(&pasta, vec![com_chave]);
    let mut m = abrir(&pasta);

    m.app.click("ia-perfis-arvore-1").click("ia-perfil-excluir");
    assert!(!m.app.checked("ia-perfil-excluir-item"));
    m.app
        .click("ia-perfil-excluir-item")
        .click("ia-perfil-excluir-dialogo-primary");
    assert!(gravado(&pasta).perfis.is_empty());
    assert!(pasta.cofre().item(item).is_none());
}

#[test]
fn escolher_um_item_do_cofre_liga_o_provedor_a_ele() {
    let pasta = Pasta::nova("ia-item");
    let item = pasta.semear_cofre(vec![Item {
        tipo: Tipo::Senha,
        titulo: "Chave da casa".into(),
        senha: "sk-casa".into(),
        ..Item::default()
    }])[0];
    semear(&pasta, vec![perfil(1, "Falso", "http://127.0.0.1:9/v1")]);
    let mut m = abrir(&pasta);
    m.app.click("ia-perfis-arvore-1");
    digitar(&mut m.app, "ia-perfil-item", "Chave da casa");
    m.app.key(tomni_ui::Key::Enter);
    assert_eq!(gravado(&pasta).perfis[0].credencial, Some(item));
    assert!(
        m.app
            .text("ia-perfil-chave-estado")
            .contains("Chave da casa")
    );
}

#[test]
fn o_primeiro_uso_sem_provedor_abre_o_dialogo_de_escolha() {
    let pasta = Pasta::nova("ia-primeiro-uso");
    let mut m = abrir(&pasta);
    // O botão Novo é o mesmo caminho do primeiro uso.
    m.app.click("ia-perfil-novo");
    assert!(m.app.visible("ia-escolher"));
    m.app.click("ia-escolher-personalizado");
    assert_eq!(gravado(&pasta).perfis[0].preset, Preset::Personalizado);
}

/// Uma árvore com uma conexão SSH (o id dela vem de `Tree::add_connection`).
fn conexao_solta(pasta: &Pasta) -> u64 {
    let mut tree = Tree::default();
    let mut c = Connection::new("web", Kind::Ssh);
    c.host = "web.exemplo".into();
    c.user = "deploy".into();
    let id = tree.add_connection(None, c);
    pasta.semear(&tree);
    id
}

#[test]
fn a_conexao_escolhe_o_provedor_na_ficha() {
    let pasta = Pasta::nova("ia-conexao");
    let id = conexao_solta(&pasta);
    semear(
        &pasta,
        vec![
            perfil(1, "Um", "http://127.0.0.1:9/v1"),
            perfil(2, "Dois", "http://127.0.0.1:9/v1"),
        ],
    );
    let mut m = montar(&pasta, Theme::Light);
    m.app.click(&format!("arvore-{id}"));
    // Sem escolha, a linha mostra o padrão (o primeiro provedor).
    assert_eq!(valor(&m.app, "Provedor de IA"), "Padrão (Um)");

    escolher_propriedade(&mut m.app, "Provedor de IA");
    m.app.select("props-Provedor de IA-valor", "Dois");
    assert_eq!(pasta.conexoes().connections()[0].perfil_ia, Some(2));
    assert_eq!(valor(&m.app, "Provedor de IA"), "Dois");

    // Voltar ao padrão desfaz a escolha.
    escolher_propriedade(&mut m.app, "Provedor de IA");
    m.app.select("props-Provedor de IA-valor", "Padrão (Um)");
    assert_eq!(pasta.conexoes().connections()[0].perfil_ia, None);
}

#[test]
fn renomear_o_provedor_acerta_a_linha_da_conexao() {
    let pasta = Pasta::nova("ia-conexao-renomear");
    let id = conexao_solta(&pasta);
    semear(&pasta, vec![perfil(1, "Um", "http://127.0.0.1:9/v1")]);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click(&format!("arvore-{id}"));
    assert_eq!(valor(&m.app, "Provedor de IA"), "Padrão (Um)");
    m.app.click("navegacao-ia").click("ia-perfis-arvore-1");
    digitar(&mut m.app, "ia-perfil-nome", "Renomeado");
    m.app.click("navegacao-terminais");
    assert_eq!(valor(&m.app, "Provedor de IA"), "Padrão (Renomeado)");
}
