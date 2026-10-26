//! Os perfis de rede (M34) na tela: o sufixo na aba, o comando de descer quando o último uso
//! fecha e a página Redes.

mod comum;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tirech_omni::armazenamento::{self, Trava};
use tirech_omni::cofre::{IdentidadeTailscale, IdentidadeZeroTier, Item, Tipo};
use tirech_omni::modelo::{Connection, FtpModo, Kind, Node, Tree};
use tirech_omni::nebula::identidade_de_pem;
use tirech_omni::openvpn::{self, MotoresVpn};
use tirech_omni::redes::{
    NebulaRede, OpenVpnRede, PerfilRede, ProxyRede, Rede, TailscaleRede, WireGuardRede,
    ZeroTierRede,
};
use tirech_omni::wireguard;
use tirech_omni::zerotier::{self, MotoresZeroTier};
use tomni_ui::theme::Theme;
use tomni_ui_netstack::{UdpUnderlay, Underlay};
use tomni_ui_tailscale::testing::{ControlFake, FakeOptions};
use tomni_ui_tailscale::{NodeState, TailscaleBuilder};
use tomni_ui_test::TestApp;
use tomni_ui_zerotier::testing::RootFake;
use tomni_ui_zerotier::testing::controller::{ControllerFake, RedeTeste};
use tomni_ui_zerotier::world::Root;
use tomni_ui_zerotier::{Identity, NodeState as NoZeroTier, World, WorldKind};
use zeroize::Zeroizing;

use comum::{
    Pasta, editar_propriedade_em, escolher_propriedade_em, exemplo, montar, montar_com_ambiente,
    valor,
};

/// Os ids da árvore de `cena`.
struct Cena {
    tree: Tree,
    perfil: u64,
    grupo: u64,
    web: u64,
    solta: u64,
}

/// Um perfil "Cliente A" (SOCKS5 em 127.0.0.1:1081, `descer` escreve uma linha em `marcador`),
/// um grupo "Cliente A" ligado a ele com a conexão `web`, e uma conexão `solta` fora do grupo.
fn cena(marcador: &str) -> Cena {
    let mut tree = Tree::default();
    let mut perfil = PerfilRede::new("Cliente A");
    perfil.proxy = Some(ProxyRede::novo());
    perfil.descer = Some(format!("echo desceu >> '{marcador}'"));
    let perfil = tree.add_perfil(perfil);
    let grupo = tree.add_group(None, "Cliente A");
    if let Some(Node::Group(g)) = tree.find_mut(grupo) {
        g.rede = Rede::Perfil(perfil);
    }
    let mut web = Connection::new("web", Kind::Ssh);
    web.host = "10.99.0.5".into();
    web.user = "deploy".into();
    web.key_file = Some("/chaves/web".into());
    let web = tree.add_connection(Some(grupo), web);
    let mut solta = Connection::new("solta", Kind::Ssh);
    solta.host = "solta.exemplo".into();
    solta.user = "deploy".into();
    solta.key_file = Some("/chaves/solta".into());
    let solta = tree.add_connection(None, solta);
    Cena {
        tree,
        perfil,
        grupo,
        web,
        solta,
    }
}

fn titulos(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()))
        .collect()
}

#[cfg(unix)]
fn chaves(app: &TestApp) -> Vec<String> {
    app.panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(chave, _)| chave.clone()))
        .collect()
}

/// Dá quadros até a condição valer (o `descer` roda numa thread).
#[cfg(unix)]
fn esperar(app: &mut TestApp, mut condicao: impl FnMut() -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !condicao() {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn a_aba_da_conexao_com_perfil_ganha_o_nome_da_rede() {
    let pasta = Pasta::nova("redes-sufixo");
    let marcador = pasta.caminho().join("desceu");
    let c = cena(&marcador.to_string_lossy());
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", &c.grupo.to_string());
    m.app.double_click(&format!("arvore-{}", c.web));
    m.app.double_click(&format!("arvore-{}", c.solta));
    assert_eq!(titulos(&m.app), ["web · Cliente A", "solta"]);
    // O perfil chega ao abridor, com o endereço do proxy.
    let senhas = m.opener.senhas.borrow();
    let rede = senhas[0].rede.as_ref().expect("a rede do perfil");
    assert_eq!(rede.perfil.id, c.perfil);
    assert!(senhas[1].rede.is_none());
}

#[cfg(unix)]
#[test]
fn o_comando_de_descer_roda_so_quando_a_ultima_aba_do_perfil_fecha() {
    let pasta = Pasta::nova("redes-descer");
    let marcador = pasta.caminho().join("desceu");
    let c = cena(&marcador.to_string_lossy());
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", &c.grupo.to_string());
    m.app.double_click(&format!("arvore-{}", c.web));
    m.app.double_click(&format!("arvore-{}", c.web));
    m.app.frame();
    let abas = chaves(&m.app);
    assert_eq!(abas.len(), 2);

    // Fechou uma das duas: a rede segue de pé.
    m.app.close_tab("paineis-painel-0", &abas[0]);
    m.app.frame();
    m.app.wait(Duration::from_millis(300));
    assert!(!marcador.exists(), "desceu com uma aba ainda aberta");

    // Fechou a última: o comando roda uma vez.
    m.app.close_tab("paineis-painel-0", &abas[1]);
    m.app.frame();
    esperar(
        &mut m.app,
        || marcador.exists(),
        "o comando de descer não rodou",
    );
    m.app.wait(Duration::from_millis(300));
    let texto = std::fs::read_to_string(&marcador).unwrap();
    assert_eq!(texto.lines().count(), 1, "rodou mais de uma vez: {texto:?}");
}

#[cfg(unix)]
#[test]
fn uma_aba_de_outro_perfil_ou_direta_nao_segura_a_rede() {
    let pasta = Pasta::nova("redes-descer-outra");
    let marcador = pasta.caminho().join("desceu");
    let c = cena(&marcador.to_string_lossy());
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", &c.grupo.to_string());
    m.app.double_click(&format!("arvore-{}", c.web));
    m.app.double_click(&format!("arvore-{}", c.solta));
    m.app.frame();
    let abas = chaves(&m.app);
    m.app.close_tab("paineis-painel-0", &abas[0]);
    m.app.frame();
    esperar(
        &mut m.app,
        || marcador.exists(),
        "o comando de descer não rodou",
    );
}

/// Dá quadros até o aviso `id` aparecer (o teste da rede roda numa thread).
fn esperar_aviso(app: &mut TestApp, id: &str) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !app.visible(id) {
        assert!(Instant::now() < limite, "o aviso {id} não apareceu");
        app.wait(Duration::from_millis(20));
    }
}

fn valor_em(app: &TestApp, grade: &str, nome: &str) -> String {
    app.properties(grade)
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a propriedade {nome}"))
        .value
}

fn rede_do_grupo(tree: &Tree, id: u64) -> Rede {
    match tree.find(id) {
        Some(Node::Group(g)) => g.rede,
        outro => panic!("não é um grupo: {outro:?}"),
    }
}

#[test]
fn criar_uma_rede_na_pagina_e_ligar_um_grupo_a_ela() {
    let pasta = Pasta::nova("redes-pagina");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);

    m.app.click("navegacao-redes");
    assert!(m.app.visible("redes-vazia"));
    assert!(!m.app.visible("redes-props"));
    m.app.click("rede-nova");
    m.app.click("rede-nova-proxy");
    assert!(m.app.visible("redes-props"));
    assert!(!m.app.visible("redes-vazia"));
    let salva = pasta.conexoes();
    assert_eq!(salva.redes.len(), 1);
    assert_eq!(salva.redes[0].name, "Nova rede");
    assert_eq!(valor_em(&m.app, "redes-props", "Proxy"), "SOCKS5");
    assert_eq!(valor_em(&m.app, "redes-props", "Porta"), "1081");

    editar_propriedade_em(&mut m.app, "redes-props", "Nome", "Cliente A");
    editar_propriedade_em(&mut m.app, "redes-props", "Porta", "1090");
    let salva = pasta.conexoes();
    let perfil = salva.redes[0].clone();
    assert_eq!(perfil.name, "Cliente A");
    assert_eq!(perfil.proxy.as_ref().map(|p| p.port), Some(1090));

    // O grupo escolhe o perfil na linha Rede; a conexão de dentro herda.
    m.app.click("navegacao-terminais").click("arvore-1");
    assert_eq!(valor_em(&m.app, "props-grupo", "Rede"), "Herdar (Direta)");
    m.app.click("props-grupo-Rede");
    m.app.select("props-grupo-Rede-valor", "Cliente A");
    assert_eq!(rede_do_grupo(&pasta.conexoes(), 1), Rede::Perfil(perfil.id));

    m.app.expand("arvore", "1").click("arvore-2");
    assert_eq!(valor(&m.app, "Rede"), "Herdar (Cliente A)");
    // O shell local não sai por rede nenhuma: a linha some.
    m.app.click("arvore-5");
    let rede = m
        .app
        .properties("props")
        .into_iter()
        .find(|p| p.name == "Rede")
        .unwrap();
    assert!(rede.hidden);
}

#[test]
fn apagar_a_rede_pede_confirmacao_e_solta_quem_usava() {
    let pasta = Pasta::nova("redes-apagar");
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente A");
    perfil.proxy = Some(ProxyRede::novo());
    let id = tree.add_perfil(perfil);
    if let Some(Node::Group(g)) = tree.find_mut(1) {
        g.rede = Rede::Perfil(id);
    }
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);

    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    assert_eq!(valor_em(&m.app, "redes-props", "Nome"), "Cliente A");
    m.app
        .click("rede-apagar")
        .click("rede-apagar-dialogo-close");
    assert_eq!(pasta.conexoes().redes.len(), 1);

    m.app
        .click("rede-apagar")
        .click("rede-apagar-dialogo-primary");
    let salva = pasta.conexoes();
    assert!(salva.redes.is_empty());
    assert!(!m.app.visible("redes-props"));
    // O grupo ainda aponta para o id, que agora não existe: a linha mostra Herdar.
    m.app.click("navegacao-terminais").click("arvore-1");
    assert_eq!(valor_em(&m.app, "props-grupo", "Rede"), "Herdar (Direta)");
}

#[test]
fn duplicar_a_rede_da_outro_nome() {
    let pasta = Pasta::nova("redes-duplicar");
    let c = cena("/tmp/nao-usado");
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{}", c.perfil));
    m.app.click("rede-duplicar");
    let nomes: Vec<String> = pasta
        .conexoes()
        .redes
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(nomes.len(), 2);
    assert_ne!(nomes[0], nomes[1]);
    assert!(nomes[1].starts_with("Cliente A"));
}

#[test]
fn o_ftp_com_proxy_desabilita_o_modo_ativo() {
    let pasta = Pasta::nova("redes-ftp");
    let c = cena("/tmp/nao-usado");
    let mut tree = c.tree.clone();
    let mut ftp = Connection::new("arquivos", Kind::Ftp);
    ftp.host = "10.99.0.6".into();
    ftp.ftp_mode = FtpModo::Ativo;
    let ftp_id = tree.add_connection(Some(c.grupo), ftp);
    let fora = {
        let mut f = Connection::new("fora", Kind::Ftp);
        f.host = "fora.exemplo".into();
        tree.add_connection(None, f)
    };
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", &c.grupo.to_string());
    let ativo = |app: &TestApp, nome: &str| {
        app.properties("props")
            .into_iter()
            .find(|p| p.name == nome)
            .unwrap()
            .enabled
    };
    m.app.click(&format!("arvore-{ftp_id}"));
    assert!(!ativo(&m.app, "Modo"));
    m.app.click(&format!("arvore-{fora}"));
    assert!(ativo(&m.app, "Modo"));
}

#[test]
fn testar_a_rede_avisa_quando_o_proxy_responde() {
    let escuta = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let pasta = Pasta::nova("redes-testar");
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente A");
    let mut proxy = ProxyRede::novo();
    proxy.port = escuta.local_addr().unwrap().port();
    perfil.proxy = Some(proxy);
    let id = tree.add_perfil(perfil);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-sucesso");
    assert!(m.app.text("aviso-sucesso").contains("Cliente A"));
    assert!(m.app.enabled("rede-testar"));
}

#[test]
fn testar_a_rede_que_nao_responde_mostra_o_erro() {
    // Uma porta que acabou de ser liberada e sem comando para subir.
    let porta = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let pasta = Pasta::nova("redes-testar-erro");
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente A");
    let mut proxy = ProxyRede::novo();
    proxy.port = porta;
    perfil.proxy = Some(proxy);
    perfil.espera_secs = 1;
    let id = tree.add_perfil(perfil);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
}

/// Um item `Tipo::Nebula` no cofre da `pasta` (a identidade de teste, do certificado v2) e um
/// perfil Nebula que aponta para ele (`com_identidade`) ou para nada. Devolve a árvore e o id do
/// perfil.
fn cena_nebula(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
    let identidade = identidade_de_pem(
        include_str!("dados/nebula/ca-25519.crt"),
        include_str!("dados/nebula/no-v2.crt"),
        include_str!("dados/nebula/no-v2.key"),
    )
    .expect("a identidade de teste");
    let ids = pasta.semear_cofre(vec![Item {
        tipo: Tipo::Nebula,
        titulo: "Nó do cliente".into(),
        nebula: Some(identidade),
        ..Item::default()
    }]);
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente N");
    perfil.nebula = Some(NebulaRede {
        config: include_str!("dados/nebula/config-no.yml").into(),
        identidade: com_identidade.then_some(ids[0]),
        origem: "config.yml".into(),
    });
    let id = tree.add_perfil(perfil);
    (tree, id)
}

fn habilitada(app: &TestApp, grade: &str, nome: &str) -> bool {
    app.properties(grade)
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a propriedade {nome}"))
        .enabled
}

/// Se a linha `nome` aparece na grade (a de outro tipo de rede fica escondida).
fn aparece(app: &TestApp, grade: &str, nome: &str) -> bool {
    !app.properties(grade)
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a propriedade {nome}"))
        .hidden
}

#[test]
fn o_perfil_nebula_mostra_a_identidade_e_desliga_as_linhas_do_proxy() {
    let pasta = Pasta::nova("redes-nebula-linhas");
    let (tree, id) = cena_nebula(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    assert_eq!(valor_em(&m.app, "redes-props", "Tipo"), "Nebula embutida");
    assert_eq!(
        valor_em(&m.app, "redes-props", "Identidade"),
        "Nó do cliente [1]"
    );
    assert!(
        valor_em(&m.app, "redes-props", "Endereço").contains('/'),
        "o endereço do certificado, com o prefixo"
    );
    assert!(!valor_em(&m.app, "redes-props", "Válido até").is_empty());
    assert!(!valor_em(&m.app, "redes-props", "Lighthouses").is_empty());
    assert!(habilitada(&m.app, "redes-props", "Identidade"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
    // Nada do segredo na página.
    for p in m.app.properties("redes-props") {
        assert!(!p.value.contains("PRIVATE KEY"), "{} vazou a chave", p.name);
    }
}

#[test]
fn trocar_o_tipo_para_nebula_liga_as_linhas_da_nebula() {
    let pasta = Pasta::nova("redes-nebula-tipo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click("rede-nova");
    m.app.click("rede-nova-proxy");
    assert_eq!(valor_em(&m.app, "redes-props", "Tipo"), "Proxy");
    assert!(habilitada(&m.app, "redes-props", "Porta"));
    assert!(!aparece(&m.app, "redes-props", "Identidade"));

    escolher_propriedade_em(&mut m.app, "redes-props", "Tipo");
    m.app.select("redes-props-Tipo-valor", "Nebula embutida");
    let perfil = pasta.conexoes().redes[0].clone();
    // O proxy digitado fica guardado (voltar ao tipo Proxy não perde nada); a Nebula é que manda.
    assert!(perfil.nebula.is_some());
    assert!(habilitada(&m.app, "redes-props", "Identidade"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
}

#[test]
fn testar_a_nebula_sem_identidade_falha_sem_sair_direto() {
    let pasta = Pasta::nova("redes-nebula-sem-identidade");
    let (tree, id) = cena_nebula(&pasta, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
}

/// Um item `Tipo::OpenVpn` no cofre da `pasta` (a identidade do `.ovpn` de teste, todo embutido)
/// e um perfil OpenVPN que aponta para ele (`com_identidade`) ou para nada. O `remote` do arquivo
/// não resolve, então o motor nunca conecta. Devolve a árvore e o id do perfil.
fn cena_openvpn(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
    let agora = UNIX_EPOCH + Duration::from_secs(1_900_000_000);
    let importacao = openvpn::importar(
        include_str!("dados/openvpn/tudo-embutido.ovpn"),
        None,
        agora,
    )
    .expect("o .ovpn de teste");
    let ids = pasta.semear_cofre(vec![Item {
        tipo: Tipo::OpenVpn,
        titulo: "VPN do cliente".into(),
        openvpn: Some(importacao.identidade),
        ..Item::default()
    }]);
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente V");
    perfil.espera_secs = 1;
    perfil.openvpn = Some(OpenVpnRede {
        config: importacao.ovpn,
        identidade: com_identidade.then_some(ids[0]),
        origem: "tudo-embutido.ovpn".into(),
        ignorar_rotas: false,
    });
    let id = tree.add_perfil(perfil);
    (tree, id)
}

#[test]
fn o_perfil_openvpn_mostra_a_identidade_e_desliga_as_linhas_do_proxy() {
    let pasta = Pasta::nova("redes-openvpn-linhas");
    let (tree, id) = cena_openvpn(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    assert_eq!(valor_em(&m.app, "redes-props", "Tipo"), "OpenVPN embutida");
    assert_eq!(
        valor_em(&m.app, "redes-props", "Identidade OpenVPN"),
        "VPN do cliente [1]"
    );
    assert!(
        valor_em(&m.app, "redes-props", "Servidor").contains("vpn.exemplo.test"),
        "o remote do .ovpn"
    );
    assert!(!valor_em(&m.app, "redes-props", "Certificado vale até").is_empty());
    assert!(habilitada(&m.app, "redes-props", "Identidade OpenVPN"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
    // Nada de segredo nem de ponte na página.
    for p in m.app.properties("redes-props") {
        assert!(!p.value.contains("PRIVATE KEY"), "{} vazou a chave", p.name);
        // "Host" e "Porta" são do formulário do proxy (desligado), não da ponte.
        if p.name != "Host" && p.name != "Porta" {
            assert!(!p.value.contains("127.0.0.1"), "{} mostra a ponte", p.name);
        }
    }
    // O que o perfil guarda é o .ovpn sem segredo.
    let perfil = pasta.conexoes().redes[0].clone();
    let guardado = perfil.openvpn.expect("o perfil OpenVPN").config;
    assert!(!guardado.contains("PRIVATE KEY"));
    assert!(!guardado.contains("OpenVPN Static key"));
}

#[test]
fn trocar_o_tipo_para_openvpn_liga_as_linhas_da_vpn() {
    let pasta = Pasta::nova("redes-openvpn-tipo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click("rede-nova");
    m.app.click("rede-nova-proxy");
    escolher_propriedade_em(&mut m.app, "redes-props", "Tipo");
    m.app.select("redes-props-Tipo-valor", "OpenVPN embutida");
    let perfil = pasta.conexoes().redes[0].clone();
    assert!(perfil.openvpn.is_some());
    assert!(perfil.nebula.is_none());
    assert!(habilitada(&m.app, "redes-props", "Identidade OpenVPN"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
}

#[test]
fn testar_a_openvpn_sem_identidade_falha_sem_sair_direto() {
    let pasta = Pasta::nova("redes-openvpn-sem-identidade");
    let (tree, id) = cena_openvpn(&pasta, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
}

#[test]
fn testar_a_openvpn_que_nao_conecta_falha_e_nao_sai_direto() {
    let pasta = Pasta::nova("redes-openvpn-nao-conecta");
    let (tree, id) = cena_openvpn(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
}

#[test]
fn duas_copias_do_app_nao_ligam_a_mesma_openvpn_juntas() {
    let pasta = Pasta::nova("redes-openvpn-duas-copias");
    let agora = UNIX_EPOCH + Duration::from_secs(1_900_000_000);
    let importacao = openvpn::importar(
        include_str!("dados/openvpn/tudo-embutido.ovpn"),
        None,
        agora,
    )
    .expect("o .ovpn de teste");
    let mut perfil = PerfilRede::new("Cliente V");
    perfil.id = 7;
    let config = || {
        openvpn::config_de(
            &importacao.ovpn,
            &importacao.identidade,
            "",
            "",
            false,
            SystemTime::now(),
        )
    };
    // Os motores de duas cópias do app, com a mesma pasta de configuração.
    let (primeira, segunda) = (MotoresVpn::default(), MotoresVpn::default());
    primeira.na_pasta(pasta.caminho());
    segunda.na_pasta(pasta.caminho());

    primeira
        .garantir(&perfil, config(), 1)
        .expect("a primeira cópia liga");
    let erro = segunda
        .garantir(&perfil, config(), 1)
        .expect_err("a segunda cópia não liga a mesma VPN");
    assert!(erro.contains("outro Tirech Omni"), "{erro}");
    assert!(!segunda.ligado(perfil.id));
    // Trocar a config na primeira (para e liga de novo) não perde a VPN para a segunda.
    primeira
        .garantir(&perfil, config(), 2)
        .expect("a primeira cópia liga de novo");

    primeira.parar(perfil.id);
    segunda
        .garantir(&perfil, config(), 1)
        .expect("livre, a segunda cópia liga");
    segunda.parar_todos();
}

#[test]
fn testar_a_openvpn_ligada_em_outra_copia_do_app_avisa() {
    let pasta = Pasta::nova("redes-openvpn-outra-copia");
    let (tree, id) = cena_openvpn(&pasta, true);
    pasta.semear(&tree);
    // A outra cópia do app está com a VPN ligada.
    let pasta_do_perfil = pasta.caminho().join("redes").join(id.to_string());
    let Ok(Trava::Travada(_trava)) = armazenamento::travar(&pasta_do_perfil) else {
        panic!("a trava da outra cópia");
    };
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(
        m.app.text("aviso-erro").contains("outro Tirech Omni"),
        "{}",
        m.app.text("aviso-erro")
    );
}

/// Um item OpenVPN criado à mão no cofre: o tipo, o usuário e a senha, sem o certificado.
fn item_openvpn_feito_a_mao() -> Item {
    Item {
        tipo: Tipo::OpenVpn,
        titulo: "openvpn-aws".into(),
        usuario: "fulano".into(),
        senha: "segredo".into(),
        ..Item::default()
    }
}

#[test]
fn o_item_openvpn_criado_a_mao_no_cofre_nao_entra_na_lista_da_identidade() {
    let pasta = Pasta::nova("redes-openvpn-a-mao-lista");
    let (tree, id) = cena_openvpn(&pasta, false);
    let a_mao = pasta.semear_cofre(vec![item_openvpn_feito_a_mao()])[0];
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    escolher_propriedade_em(&mut m.app, "redes-props", "Identidade OpenVPN");
    let escolher = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        m.app.select(
            "redes-props-Identidade OpenVPN-valor",
            &format!("openvpn-aws [{a_mao}]"),
        );
    }));
    assert!(escolher.is_err(), "o item sem certificado não sobe a rede");
    m.app
        .select("redes-props-Identidade OpenVPN-valor", "VPN do cliente [1]");
    let perfil = pasta.conexoes().redes[0].clone();
    assert_eq!(
        perfil.openvpn.expect("o perfil OpenVPN").identidade,
        Some(1)
    );
}

#[test]
fn a_rede_que_aponta_para_o_item_openvpn_feito_a_mao_diz_o_que_falta() {
    let pasta = Pasta::nova("redes-openvpn-a-mao-erro");
    let (mut tree, id) = cena_openvpn(&pasta, false);
    let a_mao = pasta.semear_cofre(vec![item_openvpn_feito_a_mao()])[0];
    if let Some(vpn) = tree.perfil_mut(id).and_then(|p| p.openvpn.as_mut()) {
        vpn.identidade = Some(a_mao);
    }
    let grupo = tree.add_group(None, "Cliente V");
    if let Some(Node::Group(g)) = tree.find_mut(grupo) {
        g.rede = Rede::Perfil(id);
    }
    let mut web = Connection::new("web", Kind::Ssh);
    web.host = "10.99.0.5".into();
    web.user = "deploy".into();
    web.key_file = Some("/chaves/web".into());
    let web = tree.add_connection(Some(grupo), web);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", &grupo.to_string());
    m.app.double_click(&format!("arvore-{web}"));
    let senhas = m.opener.senhas.borrow();
    let rede = senhas[0].rede.as_ref().expect("a rede do perfil");
    let erro = rede.openvpn_erro.clone().expect("o erro da OpenVPN");
    assert!(erro.contains("openvpn-aws"), "{erro}");
    assert!(erro.contains("criado à mão"), "{erro}");
    assert!(!erro.contains("não está mais no cofre"), "{erro}");
}

/// Os endereços locais (`IP:PORTA` em hexadecimal) dos sockets TCP em escuta que este processo
/// abriu. O `/proc/self/net` lista a rede toda do sistema, então só valem os descritores próprios.
#[cfg(target_os = "linux")]
fn sockets_em_escuta_do_processo() -> Vec<String> {
    let proprios: Vec<String> = std::fs::read_dir("/proc/self/fd")
        .expect("os descritores do processo")
        .filter_map(Result::ok)
        .filter_map(|e| std::fs::read_link(e.path()).ok())
        .filter_map(|l| {
            l.to_string_lossy()
                .strip_prefix("socket:[")
                .and_then(|n| n.strip_suffix(']'))
                .map(str::to_owned)
        })
        .collect();
    let mut locais = Vec::new();
    for arquivo in ["/proc/self/net/tcp", "/proc/self/net/tcp6"] {
        let texto = std::fs::read_to_string(arquivo).unwrap_or_default();
        for linha in texto.lines().skip(1) {
            let campos: Vec<&str> = linha.split_whitespace().collect();
            // Estado 0A: LISTEN; o inode do socket é a décima coluna.
            if campos.get(3) == Some(&"0A")
                && campos
                    .get(9)
                    .is_some_and(|i| proprios.contains(&(*i).to_owned()))
            {
                locais.push(campos[1].to_owned());
            }
        }
    }
    locais
}

/// Só a ponte do motor, em `127.0.0.1`, pode estar em escuta: o app não expõe a rede embutida
/// (sem porta fixa nem endereço que não seja o do próprio computador).
#[cfg(target_os = "linux")]
#[test]
fn com_a_openvpn_ligada_o_unico_socket_em_escuta_e_a_ponte_em_loopback() {
    let pasta = Pasta::nova("redes-openvpn-escuta");
    let (tree, id) = cena_openvpn(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    // O motor abre a ponte antes de conectar e o Testar o desliga ao falhar: olha enquanto espera.
    let limite = Instant::now() + Duration::from_secs(10);
    let mut vistos = Vec::new();
    while vistos.is_empty() && Instant::now() < limite {
        m.app.wait(Duration::from_millis(5));
        vistos = sockets_em_escuta_do_processo();
    }
    assert!(
        !vistos.is_empty(),
        "a ponte do motor não apareceu em escuta"
    );
    for local in vistos {
        let ip = local.split(':').next().unwrap_or_default();
        assert!(
            ip == "0100007F" || ip == "00000000000000000000000001000000",
            "socket em escuta fora do loopback: {local}"
        );
    }
    esperar_aviso(&mut m.app, "aviso-erro");
}

/// Um item `Tipo::WireGuard` no cofre da `pasta` (o `.conf` de teste) e um perfil WireGuard que
/// aponta para ele (`com_identidade`) ou para nada. O `Endpoint` do arquivo não resolve, então o
/// túnel nunca conecta. Devolve a árvore e o id do perfil.
fn cena_wireguard(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
    let importacao = wireguard::importar(include_str!("dados/wireguard/cliente.conf"))
        .expect("o .conf de teste");
    let ids = pasta.semear_cofre(vec![Item {
        tipo: Tipo::WireGuard,
        titulo: "VPN da empresa".into(),
        wireguard: Some(importacao.identidade),
        ..Item::default()
    }]);
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente W");
    perfil.espera_secs = 1;
    perfil.wireguard = Some(WireGuardRede {
        config: importacao.conf,
        identidade: com_identidade.then_some(ids[0]),
        origem: "cliente.conf".into(),
    });
    let id = tree.add_perfil(perfil);
    (tree, id)
}

#[test]
fn o_perfil_wireguard_mostra_a_identidade_e_desliga_as_linhas_do_proxy() {
    let pasta = Pasta::nova("redes-wireguard-linhas");
    let (tree, id) = cena_wireguard(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    assert_eq!(valor_em(&m.app, "redes-props", "Tipo"), "WireGuard");
    assert_eq!(
        valor_em(&m.app, "redes-props", "Identidade WireGuard"),
        "VPN da empresa [1]"
    );
    assert!(
        valor_em(&m.app, "redes-props", "Servidores").contains("vpn.exemplo.test"),
        "o Endpoint do .conf"
    );
    assert!(
        valor_em(&m.app, "redes-props", "Endereços").contains("10.8.0.2"),
        "o endereço do nó"
    );
    assert!(valor_em(&m.app, "redes-props", "Rotas permitidas").contains("192.168.50.0/24"));
    assert!(!valor_em(&m.app, "redes-props", "Chave pública").is_empty());
    assert!(habilitada(&m.app, "redes-props", "Identidade WireGuard"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
    // Nada de segredo nem de ponte na página.
    let importacao = wireguard::importar(include_str!("dados/wireguard/cliente.conf")).unwrap();
    for p in m.app.properties("redes-props") {
        assert!(
            !p.value.contains(&importacao.identidade.chave_privada),
            "{} vazou a chave privada",
            p.name
        );
        if p.name != "Host" && p.name != "Porta" {
            assert!(!p.value.contains("127.0.0.1"), "{} mostra a ponte", p.name);
        }
    }
    // O que o perfil guarda é o .conf sem segredo.
    let guardado = pasta.conexoes().redes[0]
        .wireguard
        .clone()
        .expect("o perfil WireGuard")
        .config;
    assert!(!guardado.contains("PrivateKey"));
    assert!(!guardado.contains("PresharedKey"));
    assert!(!guardado.contains(&importacao.identidade.chave_privada));
}

#[test]
fn trocar_o_tipo_para_wireguard_liga_as_linhas_do_tunel() {
    let pasta = Pasta::nova("redes-wireguard-tipo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click("rede-nova");
    m.app.click("rede-nova-proxy");
    escolher_propriedade_em(&mut m.app, "redes-props", "Tipo");
    m.app.select("redes-props-Tipo-valor", "WireGuard");
    let perfil = pasta.conexoes().redes[0].clone();
    assert!(perfil.wireguard.is_some());
    assert!(perfil.tailscale.is_none());
    assert!(perfil.openvpn.is_none());
    assert!(habilitada(&m.app, "redes-props", "Identidade WireGuard"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
}

#[test]
fn testar_a_wireguard_sem_identidade_falha_sem_sair_direto() {
    let pasta = Pasta::nova("redes-wireguard-sem-identidade");
    let (tree, id) = cena_wireguard(&pasta, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
}

#[test]
fn testar_a_wireguard_que_nao_conecta_falha_e_nao_sai_direto() {
    let pasta = Pasta::nova("redes-wireguard-nao-conecta");
    let (tree, id) = cena_wireguard(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
}

/// Só o `Endpoint` UDP do túnel e a ponte em `127.0.0.1`: nenhum socket TCP em escuta fora do
/// loopback.
#[cfg(target_os = "linux")]
#[test]
fn com_a_wireguard_ligada_o_unico_socket_em_escuta_e_a_ponte_em_loopback() {
    let pasta = Pasta::nova("redes-wireguard-escuta");
    let (tree, id) = cena_wireguard(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    let limite = Instant::now() + Duration::from_secs(10);
    let mut vistos = Vec::new();
    while vistos.is_empty() && Instant::now() < limite {
        m.app.wait(Duration::from_millis(5));
        vistos = sockets_em_escuta_do_processo();
    }
    assert!(
        !vistos.is_empty(),
        "a ponte do motor não apareceu em escuta"
    );
    for local in vistos {
        let ip = local.split(':').next().unwrap_or_default();
        assert!(
            ip == "0100007F" || ip == "00000000000000000000000001000000",
            "socket em escuta fora do loopback: {local}"
        );
    }
    esperar_aviso(&mut m.app, "aviso-erro");
}

/// O que o abridor de login do Tailscale anotou.
type Aberturas = Arc<Mutex<Vec<String>>>;

/// Um item `Tipo::Tailscale` no cofre da `pasta` (com `identidade`) e um perfil Tailscale que
/// aponta para ele (`com_identidade`) e para o `servidor`. Devolve a árvore e o id do perfil.
fn cena_tailscale(
    pasta: &Pasta,
    servidor: &str,
    identidade: IdentidadeTailscale,
    com_identidade: bool,
) -> (Tree, u64) {
    let ids = pasta.semear_cofre(vec![Item {
        tipo: Tipo::Tailscale,
        titulo: "Tailnet da empresa".into(),
        tailscale: Some(IdentidadeTailscale {
            servidor: servidor.into(),
            ..identidade
        }),
        ..Item::default()
    }]);
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente T");
    perfil.espera_secs = 3;
    perfil.tailscale = Some(TailscaleRede {
        servidor: servidor.into(),
        nome_maquina: "tirech-teste".into(),
        identidade: com_identidade.then_some(ids[0]),
        usar_sub_redes: true,
        efemero: false,
        ca: String::new(),
    });
    let id = tree.add_perfil(perfil);
    (tree, id)
}

/// A tela com o abridor de login trocado por um que só anota as URLs.
fn montar_tailscale(pasta: &Pasta) -> (comum::Montagem, Aberturas) {
    let abertas: Aberturas = Aberturas::default();
    let anota = Arc::clone(&abertas);
    let (m, _) = montar_com_ambiente(pasta, Theme::Light, move |ambiente| {
        ambiente.abridor_de_login = Arc::new(move |url| anota.lock().unwrap().push(url.to_owned()));
    });
    (m, abertas)
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("o runtime do servidor de controle falso")
}

#[test]
fn o_perfil_tailscale_mostra_a_identidade_e_desliga_as_linhas_do_proxy() {
    let pasta = Pasta::nova("redes-tailscale-linhas");
    let identidade = IdentidadeTailscale {
        login: "ana@exemplo.test".into(),
        tailnet: "empresa.ts.net".into(),
        nome: "tirech-teste".into(),
        enderecos: vec!["100.64.0.7".into()],
        chave_autenticacao: "tskey-auth-segredo-que-nao-aparece".into(),
        ..IdentidadeTailscale::default()
    };
    let (tree, id) = cena_tailscale(&pasta, "https://headscale.exemplo.test", identidade, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    assert_eq!(valor_em(&m.app, "redes-props", "Tipo"), "Tailscale");
    assert_eq!(
        valor_em(&m.app, "redes-props", "Identidade Tailscale"),
        "Tailnet da empresa [1]"
    );
    assert_eq!(
        valor_em(&m.app, "redes-props", "Servidor de controle"),
        "https://headscale.exemplo.test"
    );
    assert_eq!(
        valor_em(&m.app, "redes-props", "Nome da máquina"),
        "tirech-teste"
    );
    assert_eq!(valor_em(&m.app, "redes-props", "Login"), "ana@exemplo.test");
    assert_eq!(valor_em(&m.app, "redes-props", "Tailnet"), "empresa.ts.net");
    assert!(valor_em(&m.app, "redes-props", "Endereços na tailnet").contains("100.64.0.7"));
    assert!(habilitada(&m.app, "redes-props", "Identidade Tailscale"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
    assert!(m.app.visible("redes-tailscale-painel"));
    // A chave de autenticação não aparece em lugar nenhum, nem a ponte.
    for p in m.app.properties("redes-props") {
        assert!(
            !p.value.contains("tskey-auth-segredo"),
            "{} vazou a chave de autenticação",
            p.name
        );
        if p.name != "Host" && p.name != "Porta" {
            assert!(!p.value.contains("127.0.0.1"), "{} mostra a ponte", p.name);
        }
    }
    // Nada do cofre vai para o perfil.
    let texto = pasta.texto_conexoes();
    assert!(!texto.contains("tskey-auth"), "o perfil guardou a chave");
}

#[test]
fn trocar_o_tipo_para_tailscale_liga_as_linhas_da_tailnet() {
    let pasta = Pasta::nova("redes-tailscale-tipo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click("rede-nova");
    m.app.click("rede-nova-proxy");
    assert!(!m.app.visible("redes-tailscale-painel"));
    escolher_propriedade_em(&mut m.app, "redes-props", "Tipo");
    m.app.select("redes-props-Tipo-valor", "Tailscale");
    let perfil = pasta.conexoes().redes[0].clone();
    let ts = perfil.tailscale.expect("o perfil Tailscale");
    assert!(ts.usar_sub_redes, "usar as sub-redes vem ligado");
    assert!(perfil.wireguard.is_none());
    assert!(habilitada(&m.app, "redes-props", "Identidade Tailscale"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
    assert!(m.app.visible("redes-tailscale-painel"));
    assert!(m.app.visible("rede-tailscale-sem-maquinas"));
}

#[test]
fn o_login_da_tailscale_abre_a_url_uma_vez_e_o_botao_abre_de_novo() {
    let tempo = runtime();
    let controle = tempo.block_on(ControlFake::start());
    let pasta = Pasta::nova("redes-tailscale-login");
    let (tree, id) = cena_tailscale(
        &pasta,
        &controle.url(),
        IdentidadeTailscale::default(),
        true,
    );
    pasta.semear(&tree);
    let (mut m, abertas) = montar_tailscale(&pasta);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    assert!(!m.app.visible("rede-tailscale-login"));

    m.app.click("rede-testar");
    esperar_que(
        &mut m.app,
        |app| app.visible("rede-tailscale-login") && !abertas.lock().unwrap().is_empty(),
        "a InfoBar de login não apareceu",
    );
    let url = abertas.lock().unwrap()[0].clone();
    assert!(
        controle
            .login_urls()
            .iter()
            .any(|u| url.contains(u.as_str()) || u == &url),
        "a URL aberta {url} não é a que o servidor entregou: {:?}",
        controle.login_urls()
    );
    assert!(m.app.text("rede-tailscale-login").contains(&url));
    // Os quadros seguintes (o estado repete) não abrem a mesma URL de novo.
    m.app.wait(Duration::from_millis(300));
    assert_eq!(abertas.lock().unwrap().len(), 1);

    m.app.click("rede-tailscale-abrir");
    let abertas_agora = abertas.lock().unwrap().clone();
    assert_eq!(abertas_agora, [url.clone(), url]);
    esperar_aviso(&mut m.app, "aviso-erro");
}

/// Dá quadros até a condição valer, com a tela na mão (as esperas de rede levam alguns segundos).
fn esperar_que(app: &mut TestApp, mut condicao: impl FnMut(&TestApp) -> bool, o_que: &str) {
    let limite = Instant::now() + Duration::from_secs(15);
    while !condicao(app) {
        assert!(Instant::now() < limite, "{o_que}");
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn a_chave_de_autenticacao_entra_na_tailnet_e_o_estado_vai_para_o_cofre() {
    let tempo = runtime();
    let controle = tempo.block_on(ControlFake::with_options(FakeOptions {
        tailnet: true,
        ..FakeOptions::default()
    }));
    let pasta = Pasta::nova("redes-tailscale-chave");
    let (tree, id) = cena_tailscale(
        &pasta,
        &controle.url(),
        IdentidadeTailscale::default(),
        true,
    );
    pasta.semear(&tree);
    let (mut m, abertas) = montar_tailscale(&pasta);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    m.app.click("rede-tailscale-chave");
    m.app
        .type_into("rede-tailscale-chave-texto", "tskey-auth-valida");
    m.app.click("rede-tailscale-chave-dialogo-primary");
    let item = pasta.cofre().itens().first().cloned().expect("o item");
    assert_eq!(
        item.tailscale.expect("a identidade").chave_autenticacao,
        "tskey-auth-valida"
    );
    // O perfil não guarda a chave.
    assert!(!pasta.texto_conexoes().contains("tskey-auth"));

    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-sucesso");
    assert!(
        abertas.lock().unwrap().is_empty(),
        "com a chave, sem navegador"
    );
    // O nó registrou a máquina e o app guardou o estado no cofre.
    esperar_que(
        &mut m.app,
        |_| {
            pasta
                .cofre()
                .itens()
                .first()
                .and_then(|i| i.tailscale.clone())
                .is_some_and(|t| !t.chave_no.is_empty() && !t.enderecos.is_empty())
        },
        "o estado do nó não foi para o cofre",
    );
    let guardado = pasta.cofre().itens().first().cloned().unwrap();
    assert_eq!(
        guardado.tailscale.unwrap().chave_autenticacao,
        "tskey-auth-valida",
        "gravar o nó não apaga a chave"
    );
}

#[test]
fn esquecer_a_maquina_apaga_o_no_e_guarda_a_chave_de_autenticacao() {
    let pasta = Pasta::nova("redes-tailscale-esquecer");
    let identidade = IdentidadeTailscale {
        chave_maquina: "mkey:abc".into(),
        chave_no: "nodekey:abc".into(),
        login: "ana@exemplo.test".into(),
        tailnet: "empresa.ts.net".into(),
        nome: "tirech-teste".into(),
        enderecos: vec!["100.64.0.7".into()],
        chave_autenticacao: "tskey-auth-valida".into(),
        ..IdentidadeTailscale::default()
    };
    let (tree, id) = cena_tailscale(&pasta, "", identidade, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-tailscale-esquecer");
    esperar_aviso(&mut m.app, "aviso-sucesso");
    let t = pasta
        .cofre()
        .itens()
        .first()
        .and_then(|i| i.tailscale.clone())
        .expect("a identidade");
    assert!(t.chave_no.is_empty() && t.chave_maquina.is_empty());
    assert!(t.login.is_empty() && t.tailnet.is_empty() && t.enderecos.is_empty());
    assert_eq!(t.chave_autenticacao, "tskey-auth-valida");
    assert_eq!(valor_em(&m.app, "redes-props", "Login"), "");
}

#[test]
fn testar_a_tailscale_sem_identidade_falha_sem_sair_direto() {
    let pasta = Pasta::nova("redes-tailscale-sem-identidade");
    let (tree, id) = cena_tailscale(&pasta, "", IdentidadeTailscale::default(), false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
}

/// As máquinas da tailnet aparecem na página e "Nova conexão…" cria a conexão SSH que sai pelo
/// perfil; uma máquina já cadastrada (mesmo endereço) vem marcada.
#[test]
fn as_maquinas_da_tailnet_viram_conexoes_que_saem_pelo_perfil() {
    let tempo = runtime();
    let controle = tempo.block_on(ControlFake::with_options(FakeOptions {
        tailnet: true,
        ..FakeOptions::default()
    }));
    // Outra máquina da tailnet, ligada de verdade contra o mesmo servidor falso.
    let beto = tempo
        .block_on(
            TailscaleBuilder::new(controle.url(), NodeState::default())
                .hostname("beto")
                .auth_key(Zeroizing::new("tskey-auth-valida".to_owned()))
                .start(),
        )
        .expect("a máquina beto");
    let ip_do_beto = tempo
        .block_on(beto.wait_connected(Duration::from_secs(20)))
        .expect("o beto conecta")
        .to_string();
    let pasta = Pasta::nova("redes-tailscale-maquinas");
    let identidade = IdentidadeTailscale {
        chave_autenticacao: "tskey-auth-valida".into(),
        ..IdentidadeTailscale::default()
    };
    let (mut tree, id) = cena_tailscale(&pasta, &controle.url(), identidade, true);
    // Uma aba aberta pelo perfil mantém o nó ligado (o "Testar" o desligaria ao acabar).
    let mut doc = Connection::new("docs", Kind::Ssh);
    doc.host = "100.64.0.99".into();
    doc.user = "deploy".into();
    doc.key_file = Some("/chaves/docs".into());
    doc.rede = Rede::Perfil(id);
    let doc = tree.add_connection(None, doc);
    pasta.semear(&tree);
    let (mut m, _) = montar_tailscale(&pasta);
    m.app.double_click(&format!("arvore-{doc}"));
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    let existe = |app: &TestApp, id: &str| format!("{:?}", app.tree()).contains(id);
    esperar_que(
        &mut m.app,
        |app| existe(app, "rede-maquina-beto-texto"),
        "a máquina beto não apareceu na lista",
    );
    assert!(!m.app.visible("rede-tailscale-sem-maquinas"));
    assert!(m.app.text("rede-maquina-beto-texto").contains(&ip_do_beto));
    assert!(!m.app.visible("rede-maquina-beto-cadastrada"));

    m.app.click("rede-maquina-beto-conectar");
    let conexoes = pasta.conexoes();
    let nova = conexoes
        .connections()
        .into_iter()
        .find(|c| c.host == ip_do_beto)
        .expect("a conexão para a máquina")
        .clone();
    assert_eq!(nova.name, "beto");
    assert_eq!(nova.kind, Kind::Ssh);
    assert_eq!(nova.rede, Rede::Perfil(id));
    // Agora ela já está cadastrada.
    esperar_que(
        &mut m.app,
        |app| app.visible("rede-maquina-beto-cadastrada"),
        "a máquina cadastrada não ficou marcada",
    );
    assert!(!m.app.visible("rede-maquina-beto-conectar"));
}

/// Com o nó ligado, os únicos sockets TCP em escuta do processo são a ponte e o servidor de
/// controle falso do próprio teste, os dois no loopback: o app não expõe a rede embutida.
#[cfg(target_os = "linux")]
#[test]
fn com_a_tailscale_ligada_o_unico_socket_em_escuta_e_a_ponte_em_loopback() {
    let tempo = runtime();
    let controle = tempo.block_on(ControlFake::with_options(FakeOptions {
        tailnet: true,
        ..FakeOptions::default()
    }));
    let porta_do_controle = controle
        .url()
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
        .expect("a porta do servidor falso");
    let pasta = Pasta::nova("redes-tailscale-escuta");
    let identidade = IdentidadeTailscale {
        chave_autenticacao: "tskey-auth-valida".into(),
        ..IdentidadeTailscale::default()
    };
    let (tree, id) = cena_tailscale(&pasta, &controle.url(), identidade, true);
    pasta.semear(&tree);
    let (mut m, _) = montar_tailscale(&pasta);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    // O "Testar" desliga o nó ao acabar: olha enquanto ele sobe.
    let hex_do_controle = format!("{porta_do_controle:04X}");
    let limite = Instant::now() + Duration::from_secs(15);
    let mut vistos = Vec::new();
    while vistos.is_empty() && Instant::now() < limite {
        m.app.wait(Duration::from_millis(5));
        vistos = sockets_em_escuta_do_processo()
            .into_iter()
            .filter(|local| local.rsplit(':').next() != Some(hex_do_controle.as_str()))
            .collect();
    }
    assert!(!vistos.is_empty(), "a ponte do nó não apareceu em escuta");
    for local in vistos {
        let ip = local.split(':').next().unwrap_or_default();
        assert!(
            ip == "0100007F" || ip == "00000000000000000000000001000000",
            "socket em escuta fora do loopback: {local}"
        );
    }
    esperar_aviso(&mut m.app, "aviso-sucesso");
}

// ---------------------------------------------------------------------------------------------
// ZeroTier embutido (M61): um raiz e um controlador falsos, em UDP de verdade no loopback.
// ---------------------------------------------------------------------------------------------

/// A identidade `n` (1 a 4) das fixtures: a 1 é o raiz, a 4 o controlador, a 2 o membro do app.
fn identidade_zt(n: u32) -> Identity {
    let texto = match n {
        1 => include_str!("dados/zerotier/id1.secret"),
        2 => include_str!("dados/zerotier/id2.secret"),
        3 => include_str!("dados/zerotier/id3.secret"),
        _ => include_str!("dados/zerotier/id4.secret"),
    };
    Identity::from_secret_str(texto.trim()).expect("a identidade da fixture")
}

/// O raiz e o controlador falsos de um teste, com o planet que aponta para o raiz. Fica de pé
/// enquanto o `runtime` vive.
struct MundoZeroTier {
    _tempo: tokio::runtime::Runtime,
    controlador: ControllerFake,
    planet: World,
    raiz: RootFake,
}

fn mundo_zerotier() -> MundoZeroTier {
    let tempo = runtime();
    let (raiz, controlador, planet) = tempo.block_on(async {
        let (sob_raiz, _) = UdpUnderlay::bind("127.0.0.1", 0)
            .await
            .expect("o UDP do raiz");
        let porta = sob_raiz.local_addr().expect("a porta do raiz").port();
        let publica = Identity::from_public_str(&identidade_zt(1).to_public_string()).unwrap();
        // O id da Terra e um carimbo bem à frente do embutido: é o que o nó do app aceita do
        // estado guardado, sem precisar de um gancho de teste.
        let planet = World::make(
            WorldKind::Planet,
            149_604_618,
            9_999_999_999_999,
            &identidade_zt(3),
            vec![Root {
                identity: publica,
                endpoints: vec![format!("127.0.0.1:{porta}").parse().unwrap()],
            }],
            &identidade_zt(3),
        )
        .expect("o planet de teste");
        let raiz = RootFake::start(identidade_zt(1), planet.clone(), sob_raiz);
        let (sob_controlador, _) = UdpUnderlay::bind("127.0.0.1", 0)
            .await
            .expect("o UDP do controlador");
        let controlador = ControllerFake::start(
            identidade_zt(4),
            planet.clone(),
            1,
            RedeTeste::padrao("rede-de-teste"),
            sob_controlador,
        );
        (raiz, controlador, planet)
    });
    MundoZeroTier {
        _tempo: tempo,
        controlador,
        planet,
        raiz,
    }
}

/// O item `Tipo::ZeroTier` do membro 2, com o planet de teste no estado guardado.
fn item_zerotier(mundo: &MundoZeroTier) -> Item {
    let mut identidade =
        zerotier::importar_identidade(include_str!("dados/zerotier/id2.secret")).unwrap();
    zerotier::gravar_no(
        &mut identidade,
        &NoZeroTier {
            identity: Some(identidade_zt(2)),
            planet: mundo.planet.to_bytes(),
            ..NoZeroTier::default()
        },
    );
    Item {
        tipo: Tipo::ZeroTier,
        titulo: "Membro de teste".into(),
        zerotier: Some(identidade),
        ..Item::default()
    }
}

/// O item do membro no cofre da `pasta` e um perfil ZeroTier ligado a ele (`com_identidade`)
/// e à rede do controlador. Devolve a árvore e o id do perfil.
fn cena_zerotier(pasta: &Pasta, mundo: &MundoZeroTier, com_identidade: bool) -> (Tree, u64) {
    let ids = pasta.semear_cofre(vec![item_zerotier(mundo)]);
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente Z");
    perfil.espera_secs = 25;
    perfil.zerotier = Some(ZeroTierRede {
        rede: mundo.controlador.network().to_string(),
        identidade: com_identidade.then_some(ids[0]),
        ..ZeroTierRede::default()
    });
    let id = tree.add_perfil(perfil);
    (tree, id)
}

#[test]
fn o_perfil_zerotier_mostra_a_identidade_e_desliga_as_linhas_do_proxy() {
    let mundo = mundo_zerotier();
    let pasta = Pasta::nova("redes-zerotier-linhas");
    let (tree, id) = cena_zerotier(&pasta, &mundo, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    assert_eq!(valor_em(&m.app, "redes-props", "Tipo"), "ZeroTier");
    assert_eq!(
        valor_em(&m.app, "redes-props", "Identidade ZeroTier"),
        "Membro de teste [1]"
    );
    assert_eq!(
        valor_em(&m.app, "redes-props", "ID da rede"),
        mundo.controlador.network().to_string()
    );
    assert_eq!(
        valor_em(&m.app, "redes-props", "Endereço do membro"),
        identidade_zt(2).address().to_string()
    );
    assert!(habilitada(&m.app, "redes-props", "Identidade ZeroTier"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
    assert!(m.app.visible("redes-zerotier-painel"));
    assert!(!m.app.visible("rede-zerotier-autorizacao"));
    // O segredo da identidade não aparece em lugar nenhum, nem no perfil gravado.
    let segredo = include_str!("dados/zerotier/id2.secret")
        .trim()
        .rsplit(':')
        .next()
        .unwrap()
        .to_owned();
    for p in m.app.properties("redes-props") {
        assert!(!p.value.contains(&segredo), "{} vazou a identidade", p.name);
    }
    assert!(!pasta.texto_conexoes().contains(&segredo));
}

#[test]
fn trocar_o_tipo_para_zerotier_liga_as_linhas_da_rede() {
    let pasta = Pasta::nova("redes-zerotier-tipo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click("rede-nova");
    m.app.click("rede-nova-proxy");
    assert!(!m.app.visible("redes-zerotier-painel"));
    escolher_propriedade_em(&mut m.app, "redes-props", "Tipo");
    m.app.select("redes-props-Tipo-valor", "ZeroTier");
    let perfil = pasta.conexoes().redes[0].clone();
    assert!(perfil.zerotier.is_some());
    assert!(perfil.tailscale.is_none() && perfil.wireguard.is_none());
    assert!(
        perfil.espera_secs >= zerotier::ESPERA_PADRAO_SECS,
        "a autorização demora: a espera do perfil novo é longa"
    );
    assert!(habilitada(&m.app, "redes-props", "ID da rede"));
    assert!(!aparece(&m.app, "redes-props", "Porta"));
    assert!(m.app.visible("redes-zerotier-painel"));
}

#[test]
fn o_novo_ja_cria_a_rede_zerotier_com_o_id_da_rede_e_o_conectar_a_vista() {
    let pasta = Pasta::nova("redes-zerotier-novo");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click("rede-nova");
    for tipo in ["proxy", "nebula", "openvpn", "wireguard", "tailscale"] {
        assert!(
            m.app.visible(&format!("rede-nova-{tipo}")),
            "falta o {tipo}"
        );
    }
    m.app.click("rede-nova-zerotier");
    assert_eq!(valor_em(&m.app, "redes-props", "Tipo"), "ZeroTier");
    let perfil = pasta.conexoes().redes[0].clone();
    assert_eq!(perfil.zerotier, Some(Default::default()));
    assert!(perfil.tailscale.is_none() && perfil.nebula.is_none());
    assert!(perfil.espera_secs >= zerotier::ESPERA_PADRAO_SECS);
    assert!(habilitada(&m.app, "redes-props", "ID da rede"));
    assert!(!aparece(&m.app, "redes-props", "Login"));
    assert!(m.app.visible("redes-zerotier-painel"));

    // O ID digitado vai para o perfil, como na troca pela linha "Tipo".
    editar_propriedade_em(&mut m.app, "redes-props", "ID da rede", "8056c2e21c000001");
    assert_eq!(
        pasta.conexoes().redes[0].zerotier.as_ref().unwrap().rede,
        "8056c2e21c000001"
    );
}

#[test]
fn testar_a_zerotier_sem_identidade_ainda_cria_o_item_e_nao_sai_direto() {
    let mundo = mundo_zerotier();
    let pasta = Pasta::nova("redes-zerotier-sem-identidade");
    let (tree, id) = cena_zerotier(&pasta, &mundo, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    // "Testar" não conhece a identidade; o "Conectar" é que cria o item do cofre.
    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(m.app.enabled("rede-testar"));
    assert_eq!(
        pasta.cofre().itens().len(),
        1,
        "o Testar não inventa item no cofre"
    );
}

#[test]
fn sem_autorizacao_a_pagina_pede_ao_administrador_e_autorizar_conecta() {
    let mundo = mundo_zerotier();
    let pasta = Pasta::nova("redes-zerotier-autorizar");
    let (tree, id) = cena_zerotier(&pasta, &mundo, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    m.app.click("rede-testar");
    esperar_que(
        &mut m.app,
        |app| app.visible("rede-zerotier-autorizacao"),
        "o aviso de autorização não apareceu",
    );
    let aviso = m.app.text("rede-zerotier-autorizacao");
    assert!(
        aviso.contains(&identidade_zt(2).address().to_string()),
        "o aviso não traz o endereço do membro: {aviso}"
    );
    assert!(aviso.contains(&mundo.controlador.network().to_string()));
    assert_eq!(
        valor_em(&m.app, "redes-props", "Estado da rede"),
        "Aguardando autorização"
    );

    // O administrador autoriza: a configuração chega, o "Testar" termina bem e o aviso some.
    mundo.controlador.authorize(identidade_zt(2).address());
    mundo.controlador.push(identidade_zt(2).address());
    esperar_aviso(&mut m.app, "aviso-sucesso");
    esperar_que(
        &mut m.app,
        |app| !app.visible("rede-zerotier-autorizacao"),
        "o aviso de autorização não saiu",
    );
}

#[test]
fn o_membro_autorizado_conecta_e_o_estado_do_no_vai_para_o_cofre() {
    let mundo = mundo_zerotier();
    mundo.controlador.authorize(identidade_zt(2).address());
    let pasta = Pasta::nova("redes-zerotier-conecta");
    let (tree, id) = cena_zerotier(&pasta, &mundo, true);
    pasta.semear(&tree);
    let antes = pasta.cofre().itens()[0].zerotier.clone().unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    m.app.click("rede-testar");
    esperar_aviso(&mut m.app, "aviso-sucesso");
    esperar_que(
        &mut m.app,
        |_| {
            pasta
                .cofre()
                .itens()
                .first()
                .and_then(|i| i.zerotier.clone())
                .is_some_and(|z| !z.enderecos.is_empty() && !z.nome_rede.is_empty())
        },
        "o estado do nó não foi para o cofre",
    );
    let depois: IdentidadeZeroTier = pasta.cofre().itens()[0].zerotier.clone().unwrap();
    assert_eq!(depois.nome_rede, "rede-de-teste");
    assert_eq!(depois.identidade, antes.identidade, "a identidade não muda");
    assert_eq!(depois.endereco, antes.endereco);
    assert!(zerotier::tem_estado(&depois));
    // O perfil não guarda nada do nó.
    assert!(!pasta.texto_conexoes().contains(&depois.identidade));
}

#[test]
fn esquecer_o_estado_apaga_a_configuracao_e_guarda_a_identidade() {
    let mundo = mundo_zerotier();
    let pasta = Pasta::nova("redes-zerotier-esquecer");
    let (tree, id) = cena_zerotier(&pasta, &mundo, true);
    pasta.semear(&tree);
    let identidade = pasta.cofre().itens()[0]
        .zerotier
        .clone()
        .unwrap()
        .identidade;
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-zerotier-esquecer");
    esperar_aviso(&mut m.app, "aviso-sucesso");
    let z = pasta.cofre().itens()[0].zerotier.clone().unwrap();
    assert!(!zerotier::tem_estado(&z));
    assert_eq!(z.identidade, identidade, "a identidade fica");
    assert!(z.enderecos.is_empty() && z.nome_rede.is_empty());
}

#[test]
fn importar_o_identity_secret_cria_o_item_e_liga_o_perfil_a_ele() {
    let pasta = Pasta::nova("redes-zerotier-importar");
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente Z");
    perfil.zerotier = Some(ZeroTierRede {
        rede: "8056c2e21c000001".into(),
        ..ZeroTierRede::default()
    });
    let id = tree.add_perfil(perfil);
    pasta.semear(&tree);
    let arquivo = pasta.caminho().join("identity.secret");
    std::fs::write(&arquivo, include_str!("dados/zerotier/id2.secret")).unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));

    m.app.file_dialog_answer(Some(arquivo));
    m.app.click("rede-zerotier-importar");
    esperar_aviso(&mut m.app, "aviso-sucesso");
    let itens = pasta.cofre().itens().to_vec();
    assert_eq!(itens.len(), 1);
    assert_eq!(itens[0].tipo, Tipo::ZeroTier);
    let z = itens[0].zerotier.clone().unwrap();
    assert_eq!(z.endereco, identidade_zt(2).address().to_string());
    let perfil = pasta.conexoes().perfil(id).cloned().unwrap();
    assert_eq!(perfil.zerotier.unwrap().identidade, Some(itens[0].id));
    assert_eq!(
        valor_em(&m.app, "redes-props", "Endereço do membro"),
        z.endereco
    );
}

#[test]
fn um_arquivo_que_nao_e_identidade_e_recusado_sem_criar_item() {
    let pasta = Pasta::nova("redes-zerotier-importar-ruim");
    let mut tree = exemplo();
    let id = tree.add_perfil({
        let mut p = PerfilRede::new("Cliente Z");
        p.zerotier = Some(ZeroTierRede::default());
        p
    });
    pasta.semear(&tree);
    let arquivo = pasta.caminho().join("identity.secret");
    std::fs::write(&arquivo, "isto não é uma identidade").unwrap();
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.file_dialog_answer(Some(arquivo));
    m.app.click("rede-zerotier-importar");
    esperar_aviso(&mut m.app, "aviso-erro");
    assert!(pasta.cofre().itens().is_empty());
}

#[test]
fn o_motor_liga_na_primeira_vez_para_na_ultima_e_recusa_a_mesma_identidade_em_dois_perfis() {
    let mundo = mundo_zerotier();
    mundo.controlador.authorize(identidade_zt(2).address());
    let item = item_zerotier(&mundo);
    let identidade = item.zerotier.clone().unwrap();
    let mut tree = Tree::default();
    let mut ids = Vec::new();
    for nome in ["Primeiro", "Segundo"] {
        let mut perfil = PerfilRede::new(nome);
        perfil.zerotier = Some(ZeroTierRede {
            rede: mundo.controlador.network().to_string(),
            identidade: Some(1),
            ..ZeroTierRede::default()
        });
        ids.push(tree.add_perfil(perfil));
    }
    let perfis: Vec<PerfilRede> = ids
        .iter()
        .map(|id| tree.perfil(*id).cloned().unwrap())
        .collect();
    let parametros = |perfil: &PerfilRede| {
        zerotier::Parametros::de(perfil.zerotier.as_ref().unwrap(), 1, &identidade)
    };

    let motores = MotoresZeroTier::default();
    assert!(!motores.ligado(ids[0]));
    let motor = motores
        .garantir(&perfis[0], parametros(&perfis[0]))
        .expect("o primeiro perfil liga");
    assert!(motores.ligado(ids[0]));
    // Pedir de novo, igual, devolve o mesmo nó (sem subir outro).
    let de_novo = motores
        .garantir(&perfis[0], parametros(&perfis[0]))
        .expect("o mesmo perfil de novo");
    assert_eq!(motor.endereco(), de_novo.endereco());
    // A mesma identidade em outro perfil: recusada, com o nome de quem a usa.
    let erro = motores
        .garantir(&perfis[1], parametros(&perfis[1]))
        .expect_err("a identidade está em uso");
    assert!(erro.contains("Primeiro"), "{erro}");
    assert!(!motores.ligado(ids[1]));

    motores.parar(ids[0]);
    assert!(!motores.ligado(ids[0]));
    // Livre de novo, o segundo perfil liga.
    motores
        .garantir(&perfis[1], parametros(&perfis[1]))
        .expect("o segundo perfil liga depois que o primeiro parou");
    motores.parar_todos();
    assert!(!motores.ligado(ids[1]));
    drop(mundo.raiz);
}

#[test]
fn com_a_zerotier_ligada_o_unico_socket_em_escuta_e_a_ponte_em_loopback() {
    let mundo = mundo_zerotier();
    mundo.controlador.authorize(identidade_zt(2).address());
    let pasta = Pasta::nova("redes-zerotier-escuta");
    let (tree, id) = cena_zerotier(&pasta, &mundo, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    // O "Testar" desliga o nó ao acabar: olha enquanto ele sobe.
    let limite = Instant::now() + Duration::from_secs(15);
    let mut vistos = Vec::new();
    while vistos.is_empty() && Instant::now() < limite {
        m.app.wait(Duration::from_millis(5));
        vistos = sockets_em_escuta_do_processo();
    }
    assert!(!vistos.is_empty(), "a ponte do nó não apareceu em escuta");
    for local in vistos {
        let ip = local.split(':').next().unwrap_or_default();
        assert!(
            ip == "0100007F" || ip == "00000000000000000000000001000000",
            "socket em escuta fora do loopback: {local}"
        );
    }
    esperar_aviso(&mut m.app, "aviso-sucesso");
}
