//! Os links na árvore: o duplo clique abre no navegador padrão (sem perfil de rede) ou no
//! navegador separado do perfil, o endereço inválido e o erro do abridor viram aviso, as
//! propriedades mostram o endereço e o lançador "Links…" do menu Ferramentas abre pela grade.

mod comum;

use tirech_omni::links::DestinoDoLink;
use tirech_omni::modelo::{Connection, Kind, Node, Tree};
use tirech_omni::redes::{PerfilRede, ProxyRede, Rede};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, editar_propriedade, montar, valor};

/// Os ids da árvore de `cena`.
struct Cena {
    tree: Tree,
    perfil: u64,
    grupo: u64,
    intranet: u64,
    site: u64,
    ruim: u64,
}

/// Um grupo "Cliente A" ligado a um perfil com proxy, com o link `intranet` dentro; na raiz, o
/// link direto `site` e o `ruim`, com um endereço que não é http.
fn cena() -> Cena {
    let mut tree = Tree::default();
    let mut perfil = PerfilRede::new("Cliente A");
    perfil.proxy = Some(ProxyRede::novo());
    let perfil = tree.add_perfil(perfil);
    let grupo = tree.add_group(None, "Cliente A");
    if let Some(Node::Group(g)) = tree.find_mut(grupo) {
        g.rede = Rede::Perfil(perfil);
    }
    let mut intranet = Connection::new("intranet", Kind::Link);
    intranet.url = "intranet.cliente/painel".into();
    let intranet = tree.add_connection(Some(grupo), intranet);
    let mut site = Connection::new("site", Kind::Link);
    site.url = "https://exemplo.com/".into();
    let site = tree.add_connection(None, site);
    let mut ruim = Connection::new("ruim", Kind::Link);
    ruim.url = "ftp://arquivos.exemplo".into();
    let ruim = tree.add_connection(None, ruim);
    Cena {
        tree,
        perfil,
        grupo,
        intranet,
        site,
        ruim,
    }
}

fn abas(app: &TestApp) -> usize {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.len())
        .sum()
}

#[test]
fn o_link_direto_vai_ao_navegador_padrao_sem_abrir_aba() {
    let pasta = Pasta::nova("links-arvore-direto");
    let c = cena();
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click(&format!("arvore-{}", c.site));
    assert_eq!(
        *m.opener.links.borrow(),
        [DestinoDoLink::Navegador("https://exemplo.com/".into())]
    );
    assert!(m.opener.links_pela_rede.borrow().is_empty());
    assert_eq!(abas(&m.app), 0, "o link não vira aba");
    assert_eq!(m.app.text("navegacao"), "terminais");
}

#[test]
fn o_link_com_perfil_vai_ao_navegador_da_rede_com_http_na_frente() {
    let pasta = Pasta::nova("links-arvore-perfil");
    let c = cena();
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", &c.grupo.to_string());
    m.app.double_click(&format!("arvore-{}", c.intranet));
    let pedidos = m.opener.links_pela_rede.borrow();
    assert_eq!(pedidos.len(), 1);
    assert_eq!(pedidos[0].0, "http://intranet.cliente/painel");
    assert_eq!(pedidos[0].1.perfil.id, c.perfil);
    assert!(m.opener.links.borrow().is_empty());
    assert_eq!(abas(&m.app), 0);
}

#[test]
fn o_endereco_que_nao_e_http_vira_aviso_e_nao_pede_nada() {
    let pasta = Pasta::nova("links-arvore-ruim");
    let c = cena();
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click(&format!("arvore-{}", c.ruim));
    assert_eq!(
        m.app.text("aviso-atencao"),
        "O link \"ruim\" não tem um endereço http ou https."
    );
    assert!(m.opener.links.borrow().is_empty());
    assert!(m.opener.links_pela_rede.borrow().is_empty());
}

#[test]
fn o_erro_do_navegador_da_rede_vira_aviso() {
    let pasta = Pasta::nova("links-arvore-falha");
    let c = cena();
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.opener.link_falha.set(true);
    m.app.expand("arvore", &c.grupo.to_string());
    m.app.double_click(&format!("arvore-{}", c.intranet));
    assert_eq!(
        m.app.text("aviso-atencao"),
        "Não foi possível abrir o link: sem navegador"
    );
}

#[test]
fn as_propriedades_do_link_mostram_e_gravam_o_endereco() {
    let pasta = Pasta::nova("links-arvore-props");
    let c = cena();
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click(&format!("arvore-{}", c.site));
    assert!(m.app.visible("props"));
    let nomes: Vec<String> = m
        .app
        .properties("props")
        .into_iter()
        .filter(|p| !p.hidden)
        .map(|p| p.name)
        .collect();
    assert_eq!(
        nomes,
        ["Nome", "Descrição", "Tipo", "Endereço do link", "Rede"],
        "o link só mostra o que vale para ele"
    );
    assert_eq!(valor(&m.app, "Endereço do link"), "https://exemplo.com/");

    editar_propriedade(&mut m.app, "Endereço do link", "  https://outro.exemplo/  ");
    let salvo = pasta.conexoes().connection(c.site).cloned().unwrap();
    assert_eq!(salvo.url, "https://outro.exemplo/");
}

#[test]
fn o_lancador_lista_os_links_filtra_e_abre_o_escolhido() {
    let pasta = Pasta::nova("links-arvore-lancador");
    let c = cena();
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-ferramentas").click("links-menu");
    m.app.frame();
    assert!(m.app.is_open("links-dialogo"));
    assert_eq!(
        m.app.grid("links-grade").keys(),
        [
            c.intranet.to_string(),
            c.site.to_string(),
            c.ruim.to_string()
        ]
    );
    assert!(!m.app.enabled("links-abrir"));

    // O filtro olha o endereço também.
    m.app.type_into("links-filtro", "INTRANET.cliente");
    m.app.frame();
    let chave = c.intranet.to_string();
    assert_eq!(m.app.grid("links-grade").keys(), [chave.as_str()]);
    m.app.grid_select("links-grade", &[chave.as_str()]);
    assert!(m.app.enabled("links-abrir"));
    m.app.click("links-abrir");
    m.app.frame();
    assert!(!m.app.is_open("links-dialogo"));
    let pedidos = m.opener.links_pela_rede.borrow();
    assert_eq!(pedidos.len(), 1);
    assert_eq!(pedidos[0].0, "http://intranet.cliente/painel");
}

#[test]
fn o_menu_do_link_so_tem_o_que_vale_para_ele() {
    let pasta = Pasta::nova("links-arvore-menu");
    let c = cena();
    pasta.semear(&c.tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.context_menu(&format!("arvore-{}", c.site));
    let sufixos = [
        "abrir",
        "todas",
        "todas-ia",
        "ir-para-aba",
        "reconectar",
        "cluster",
        "cluster-ia",
        "script",
        "arquivos",
        "ia",
        "virt-manager",
        "virt-manager-local",
        "executar",
        "tuneis",
        "sessoes",
        "mcp",
        "renomear",
        "duplicar",
        "copiar-chave",
        "apagar",
    ];
    let visiveis: Vec<&str> = sufixos
        .into_iter()
        .filter(|s| {
            let id = format!("menu-{}-{s}", c.site);
            m.app.ui().find(&id).is_some() && m.app.visible(&id)
        })
        .collect();
    assert_eq!(visiveis, ["abrir", "renomear", "duplicar", "apagar"]);
}
