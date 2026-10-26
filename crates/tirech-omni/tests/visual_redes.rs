//! A página Redes nos dois temas (M34): dois perfis, um deles escolhido. Snapshots e lint.

mod comum;

use std::time::{Duration, Instant, UNIX_EPOCH};

use tirech_omni::cofre::{IdentidadeTailscale, IdentidadeZeroTier, Item, Tipo};
use tirech_omni::modelo::{Node, Tree};
use tirech_omni::nebula::identidade_de_pem;
use tirech_omni::openvpn;
use tirech_omni::redes::{
    NebulaRede, OpenVpnRede, PerfilRede, ProxyRede, Rede, TailscaleRede, TipoProxy, WireGuardRede,
    ZeroTierRede,
};
use tirech_omni::wireguard;
use tomni_ui::theme::Theme;

use comum::{Pasta, exemplo, montar};

/// O `exemplo` com os perfis "Cliente A" (SOCKS5 1081, sobe e desce com o compose) e
/// "Cliente B" (HTTP 8118), e o grupo Produção ligado ao primeiro.
fn com_redes() -> (Tree, u64) {
    let mut tree = exemplo();
    let mut a = PerfilRede::new("Cliente A");
    a.proxy = Some(ProxyRede::novo());
    a.subir = Some("docker compose -f redes/cliente-a/compose.yml up -d".into());
    a.descer = Some("docker compose -f redes/cliente-a/compose.yml down".into());
    let a = tree.add_perfil(a);
    let mut b = PerfilRede::new("Cliente B");
    b.cor = 3;
    b.proxy = Some(ProxyRede {
        tipo: TipoProxy::Http,
        port: 8118,
        ..ProxyRede::novo()
    });
    tree.add_perfil(b);
    if let Some(Node::Group(g)) = tree.find_mut(1) {
        g.rede = Rede::Perfil(a);
    }
    (tree, a)
}

fn pagina_com_redes(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-{nome}"));
    let (tree, a) = com_redes();
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{a}"));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-{nome}"));
}

fn pagina_vazia(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-vazia-{nome}"));
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-vazia-{nome}"));
}

#[test]
fn a_pagina_redes_no_tema_claro() {
    pagina_com_redes("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_no_tema_escuro() {
    pagina_com_redes("escuro", Theme::Dark);
}

#[test]
fn a_pagina_redes_vazia_no_tema_claro() {
    pagina_vazia("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_vazia_no_tema_escuro() {
    pagina_vazia("escuro", Theme::Dark);
}

/// O `exemplo` com um perfil Nebula "Cliente N" ligado ao nó "Nó do cliente" do cofre (ou a
/// nenhum, em `com_identidade = false`). Devolve a árvore e o id do perfil.
fn com_nebula(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
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

fn pagina_com_nebula(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-nebula-{nome}"));
    let (tree, id) = com_nebula(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-nebula-{nome}"));
}

/// O "Testar" de um perfil Nebula sem identidade: o aviso de erro e o estado da rede.
fn pagina_com_nebula_sem_identidade(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-nebula-erro-{nome}"));
    let (tree, id) = com_nebula(&pasta, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    let limite = Instant::now() + Duration::from_secs(10);
    while !m.app.visible("aviso-erro") {
        assert!(Instant::now() < limite, "o aviso de erro não apareceu");
        m.app.wait(Duration::from_millis(20));
    }
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-nebula-erro-{nome}"));
}

#[test]
fn a_pagina_redes_com_nebula_no_tema_claro() {
    pagina_com_nebula("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_nebula_no_tema_escuro() {
    pagina_com_nebula("escuro", Theme::Dark);
}

#[test]
fn a_pagina_redes_com_nebula_sem_identidade_no_tema_claro() {
    pagina_com_nebula_sem_identidade("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_nebula_sem_identidade_no_tema_escuro() {
    pagina_com_nebula_sem_identidade("escuro", Theme::Dark);
}

/// O `exemplo` com um perfil OpenVPN "Cliente V" ligado ao item "VPN do cliente" do cofre (ou a
/// nada, sem `com_identidade`). O `remote` do `.ovpn` de teste não resolve.
fn com_openvpn(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
    let importacao = openvpn::importar(
        include_str!("dados/openvpn/tudo-embutido.ovpn"),
        None,
        UNIX_EPOCH + Duration::from_secs(1_900_000_000),
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

fn pagina_com_openvpn(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-openvpn-{nome}"));
    let (tree, id) = com_openvpn(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-openvpn-{nome}"));
}

/// O "Testar" de um perfil OpenVPN: sem identidade ou com o servidor que não responde, o aviso de
/// erro e o estado da VPN.
fn pagina_com_openvpn_testada(nome: &str, tema: Theme, com_identidade: bool, cena: &str) {
    let pasta = Pasta::nova(&format!("redes-visual-{cena}-{nome}"));
    let (tree, id) = com_openvpn(&pasta, com_identidade);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    let limite = Instant::now() + Duration::from_secs(10);
    while !m.app.visible("aviso-erro") {
        assert!(Instant::now() < limite, "o aviso de erro não apareceu");
        m.app.wait(Duration::from_millis(20));
    }
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-{cena}-{nome}"));
}

#[test]
fn a_pagina_redes_com_openvpn_no_tema_claro() {
    pagina_com_openvpn("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_openvpn_no_tema_escuro() {
    pagina_com_openvpn("escuro", Theme::Dark);
}

#[test]
fn a_pagina_redes_com_openvpn_sem_identidade_no_tema_claro() {
    pagina_com_openvpn_testada("claro", Theme::Light, false, "openvpn-erro");
}

#[test]
fn a_pagina_redes_com_openvpn_sem_identidade_no_tema_escuro() {
    pagina_com_openvpn_testada("escuro", Theme::Dark, false, "openvpn-erro");
}

#[test]
fn a_pagina_redes_com_openvpn_que_nao_conecta_no_tema_claro() {
    pagina_com_openvpn_testada("claro", Theme::Light, true, "openvpn-offline");
}

#[test]
fn a_pagina_redes_com_openvpn_que_nao_conecta_no_tema_escuro() {
    pagina_com_openvpn_testada("escuro", Theme::Dark, true, "openvpn-offline");
}

/// O `exemplo` com um perfil WireGuard "Cliente W" ligado ao item "VPN da empresa" do cofre (ou a
/// nada, sem `com_identidade`). O `Endpoint` do `.conf` de teste não resolve.
fn com_wireguard(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
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

fn pagina_com_wireguard(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-wireguard-{nome}"));
    let (tree, id) = com_wireguard(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-wireguard-{nome}"));
}

/// O "Testar" de um perfil WireGuard sem identidade: o aviso de erro e o estado do túnel.
fn pagina_com_wireguard_sem_identidade(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-wireguard-erro-{nome}"));
    let (tree, id) = com_wireguard(&pasta, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_erro(&mut m);
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("redes-wireguard-erro-{nome}"));
}

fn esperar_erro(m: &mut comum::Montagem) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !m.app.visible("aviso-erro") {
        assert!(Instant::now() < limite, "o aviso de erro não apareceu");
        m.app.wait(Duration::from_millis(20));
    }
}

#[test]
fn a_pagina_redes_com_wireguard_no_tema_claro() {
    pagina_com_wireguard("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_wireguard_no_tema_escuro() {
    pagina_com_wireguard("escuro", Theme::Dark);
}

#[test]
fn a_pagina_redes_com_wireguard_sem_identidade_no_tema_claro() {
    pagina_com_wireguard_sem_identidade("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_wireguard_sem_identidade_no_tema_escuro() {
    pagina_com_wireguard_sem_identidade("escuro", Theme::Dark);
}

/// O `exemplo` com um perfil Tailscale "Cliente T" (um Headscale) ligado ao item "Tailnet da
/// empresa" do cofre, que já conhece a máquina (ou a nada, sem `com_identidade`).
fn com_tailscale(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
    let ids = pasta.semear_cofre(vec![Item {
        tipo: Tipo::Tailscale,
        titulo: "Tailnet da empresa".into(),
        tailscale: Some(IdentidadeTailscale {
            servidor: "https://headscale.exemplo.test".into(),
            login: "ana@exemplo.test".into(),
            tailnet: "empresa.exemplo.test".into(),
            nome: "tirech".into(),
            enderecos: vec!["100.64.0.7".into()],
            vence_em: 1_900_000_000,
            ..IdentidadeTailscale::default()
        }),
        ..Item::default()
    }]);
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente T");
    perfil.espera_secs = 1;
    perfil.tailscale = Some(TailscaleRede {
        servidor: "https://headscale.exemplo.test".into(),
        nome_maquina: "tirech".into(),
        identidade: com_identidade.then_some(ids[0]),
        usar_sub_redes: true,
        efemero: false,
        ca: String::new(),
    });
    let id = tree.add_perfil(perfil);
    (tree, id)
}

fn pagina_com_tailscale(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-tailscale-{nome}"));
    let (tree, id) = com_tailscale(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-tailscale-{nome}"));
}

/// O "Testar" de um perfil Tailscale sem identidade: o aviso de erro e o estado da tailnet.
fn pagina_com_tailscale_sem_identidade(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-tailscale-erro-{nome}"));
    let (tree, id) = com_tailscale(&pasta, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_erro(&mut m);
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("redes-tailscale-erro-{nome}"));
}

#[test]
fn a_pagina_redes_com_tailscale_no_tema_claro() {
    pagina_com_tailscale("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_tailscale_no_tema_escuro() {
    pagina_com_tailscale("escuro", Theme::Dark);
}

#[test]
fn a_pagina_redes_com_tailscale_sem_identidade_no_tema_claro() {
    pagina_com_tailscale_sem_identidade("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_tailscale_sem_identidade_no_tema_escuro() {
    pagina_com_tailscale_sem_identidade("escuro", Theme::Dark);
}

/// O `exemplo` com um perfil ZeroTier "Cliente Z" ligado ao item "Membro da rede" do cofre, que
/// já tem a identidade (a do `tests/dados`) e conhece a rede (ou a nada, sem `com_identidade`).
fn com_zerotier(pasta: &Pasta, com_identidade: bool) -> (Tree, u64) {
    let secret = include_str!("dados/zerotier/id2.secret").trim();
    let ids = pasta.semear_cofre(vec![Item {
        tipo: Tipo::ZeroTier,
        titulo: "Membro da rede".into(),
        zerotier: Some(IdentidadeZeroTier {
            identidade: secret.into(),
            endereco: secret.split(':').next().unwrap_or_default().into(),
            nome_rede: "empresa".into(),
            enderecos: vec!["10.147.17.23/24".into()],
            ..IdentidadeZeroTier::default()
        }),
        ..Item::default()
    }]);
    let mut tree = exemplo();
    let mut perfil = PerfilRede::new("Cliente Z");
    perfil.espera_secs = 1;
    perfil.zerotier = Some(ZeroTierRede {
        rede: "8056c2e21c000001".into(),
        identidade: com_identidade.then_some(ids[0]),
        permitir_dns: true,
        moons: vec![("a1b2c3d4e5f60718".into(), "0102030405".into())],
        ..ZeroTierRede::default()
    });
    let id = tree.add_perfil(perfil);
    (tree, id)
}

fn pagina_com_zerotier(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-zerotier-{nome}"));
    let (tree, id) = com_zerotier(&pasta, true);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.assert_lint();
    m.app.assert_snapshot(&format!("redes-zerotier-{nome}"));
}

/// O "Testar" de um perfil ZeroTier sem identidade: o aviso de erro e o estado da rede.
fn pagina_com_zerotier_sem_identidade(nome: &str, tema: Theme) {
    let pasta = Pasta::nova(&format!("redes-visual-zerotier-erro-{nome}"));
    let (tree, id) = com_zerotier(&pasta, false);
    pasta.semear(&tree);
    let mut m = montar(&pasta, tema);
    m.app.click("navegacao-redes");
    m.app.click(&format!("redes-arvore-{id}"));
    m.app.click("rede-testar");
    esperar_erro(&mut m);
    m.app.assert_lint();
    m.app
        .assert_snapshot(&format!("redes-zerotier-erro-{nome}"));
}

#[test]
fn a_pagina_redes_com_zerotier_no_tema_claro() {
    pagina_com_zerotier("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_zerotier_no_tema_escuro() {
    pagina_com_zerotier("escuro", Theme::Dark);
}

#[test]
fn a_pagina_redes_com_zerotier_sem_identidade_no_tema_claro() {
    pagina_com_zerotier_sem_identidade("claro", Theme::Light);
}

#[test]
fn a_pagina_redes_com_zerotier_sem_identidade_no_tema_escuro() {
    pagina_com_zerotier_sem_identidade("escuro", Theme::Dark);
}
