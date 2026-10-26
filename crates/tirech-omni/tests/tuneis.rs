//! Os túneis na tela (M37): a categoria "Encaminhamento", o editor de túneis da conexão e os
//! itens do menu da árvore.

mod comum;

use tirech_omni::modelo::{Connection, Kind, ModoX11, TipoTunel, Tree, Tunel};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, escolher_propriedade, montar, valor};

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar() {
    for id in 0..=9 {
        for n in 1..=8 {
            if let Some(s) = Session::find(&format!("{id}-{n}")) {
                s.dispose();
            }
        }
    }
}

fn ssh(nome: &str) -> Connection {
    let mut c = Connection::new(nome, Kind::Ssh);
    c.host = format!("{nome}.exemplo");
    c.user = "deploy".into();
    c.key_file = Some(format!("/chaves/{nome}").into());
    c
}

fn tunel(porta: u16, host: &str, destino: u16) -> Tunel {
    let mut t = Tunel::novo(TipoTunel::Local, false);
    t.porta = porta;
    host.clone_into(&mut t.destino_host);
    t.destino_porta = destino;
    t
}

/// Uma árvore com um SSH (1), um Telnet (2) e um shell local (3).
fn arvore() -> Tree {
    let mut tree = Tree::default();
    tree.add_connection(None, ssh("web"));
    let mut telnet = Connection::new("roteador", Kind::Telnet);
    telnet.host = "roteador.exemplo".into();
    tree.add_connection(None, telnet);
    tree.add_connection(None, Connection::new("shell", Kind::Local));
    tree
}

fn tela(nome: &str, tree: &Tree) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(tree);
    let m = montar(&pasta, Theme::Light);
    (pasta, m)
}

/// Os túneis da conexão `id` como o `connections.json` guarda.
fn gravados(pasta: &Pasta, id: u64) -> Vec<Tunel> {
    pasta
        .conexoes()
        .connection(id)
        .map(|c| c.tuneis.clone())
        .unwrap_or_default()
}

fn abrir_editor(app: &mut TestApp, id: u64) {
    app.context_menu(&format!("arvore-{id}"));
    app.click(&format!("menu-{id}-tuneis"));
}

#[test]
fn a_categoria_encaminhamento_so_vale_no_ssh() {
    let (_pasta, mut m) = tela("tuneis-categoria", &arvore());
    m.app.click("arvore-1");
    let props = m.app.properties("props");
    for nome in [
        "Encaminhar X11",
        "Túneis",
        "Só túneis (sem shell)",
        "Parar se túnel falhar",
    ] {
        let p = props
            .iter()
            .find(|p| p.name == nome)
            .unwrap_or_else(|| panic!("sem {nome}"));
        assert_eq!(p.category.as_deref(), Some("Encaminhamento"));
        assert!(p.enabled, "{nome} vale no SSH");
    }
    m.app.click("arvore-3");
    let local = m.app.properties("props");
    for nome in ["Encaminhar X11", "Túneis", "Só túneis (sem shell)"] {
        let p = local.iter().find(|p| p.name == nome).unwrap();
        assert!(p.hidden, "{nome} não vale no shell local");
    }
    encerrar();
}

#[test]
fn sem_tuneis_a_linha_mostra_nenhum_e_o_botao_abre_o_editor() {
    let (_pasta, mut m) = tela("tuneis-nenhum", &arvore());
    m.app.click("arvore-1");
    assert_eq!(valor(&m.app, "Túneis"), "Nenhum");
    escolher_propriedade(&mut m.app, "Túneis");
    m.app.click("props-Túneis-botao");
    assert!(m.app.visible("tuneis-dialogo"));
    assert!(m.app.visible("tuneis-lista"));
    assert!(m.app.grid("tuneis-lista").rows.is_empty());
    encerrar();
}

#[test]
fn adicionar_editar_e_remover_gravam_na_arvore() {
    let (pasta, mut m) = tela("tuneis-editar", &arvore());
    m.app.click("arvore-1");
    abrir_editor(&mut m.app, 1);
    m.app.click("tuneis-adicionar");
    assert!(m.app.visible("tunel-porta"));
    assert!(!m.app.enabled("tunel-salvar"), "a porta 0 não salva");
    m.app.set_value("tunel-porta", 8080.0);
    m.app.click("tunel-destino").type_text("db.interno:5432");
    assert!(m.app.enabled("tunel-salvar"));
    m.app.click("tunel-salvar");
    let salvos = gravados(&pasta, 1);
    assert_eq!(salvos.len(), 1);
    assert_eq!(salvos[0].tipo, TipoTunel::Local);
    assert_eq!(salvos[0].porta, 8080);
    assert_eq!(salvos[0].destino_host, "db.interno");
    assert_eq!(salvos[0].destino_porta, 5432);
    assert_eq!(m.app.grid("tuneis-lista").rows.len(), 1);
    assert_eq!(valor(&m.app, "Túneis"), "1 túnel");

    // Editar a porta.
    let chave = m.app.grid("tuneis-lista").rows[0].key.clone();
    m.app.grid_select("tuneis-lista", &[chave.as_str()]);
    m.app.click("tuneis-editar");
    assert_eq!(m.app.value("tunel-porta"), 8080.0);
    m.app.set_value("tunel-porta", 9090.0);
    m.app.click("tunel-salvar");
    assert_eq!(gravados(&pasta, 1)[0].porta, 9090);

    // Remover.
    let chave = m.app.grid("tuneis-lista").rows[0].key.clone();
    m.app.grid_select("tuneis-lista", &[chave.as_str()]);
    m.app.click("tuneis-remover");
    assert!(gravados(&pasta, 1).is_empty());
    assert_eq!(valor(&m.app, "Túneis"), "Nenhum");
    encerrar();
}

#[test]
fn o_menu_da_arvore_so_tem_os_itens_de_x11_e_tuneis_no_ssh() {
    let (_pasta, mut m) = tela("tuneis-menu", &arvore());
    m.app.context_menu("arvore-1");
    for item in ["virt-manager", "executar", "tuneis"] {
        assert!(m.app.visible(&format!("menu-1-{item}")), "{item} no SSH");
    }
    m.app.key(tomni_ui::Key::Escape);
    for id in [2, 3] {
        m.app.context_menu(&format!("arvore-{id}"));
        for item in ["virt-manager", "virt-manager-local", "executar", "tuneis"] {
            assert!(
                !m.app.visible(&format!("menu-{id}-{item}")),
                "{item} em {id}"
            );
        }
        m.app.key(tomni_ui::Key::Escape);
    }
    encerrar();
}

#[test]
fn o_virt_manager_local_fica_escondido_sem_o_programa_na_maquina() {
    let (_pasta, mut m) = tela("tuneis-virt-local", &arvore());
    m.app.context_menu("arvore-1");
    assert!(!m.app.visible("menu-1-virt-manager-local"));
    assert!(m.app.visible("menu-1-virt-manager"));
    encerrar();
}

#[test]
fn a_conexao_so_tuneis_leva_o_sufixo_no_titulo_da_aba() {
    let mut tree = Tree::default();
    let mut c = ssh("web");
    c.so_tuneis = true;
    c.x11 = ModoX11::Desligado;
    c.tuneis.push(tunel(8080, "db", 5432));
    tree.add_connection(None, c);
    let (_pasta, mut m) = tela("tuneis-so", &tree);
    m.app.double_click("arvore-1");
    let titulos: Vec<String> = m
        .app
        .panes("paineis")
        .panes
        .iter()
        .flat_map(|p| p.tabs.iter().map(|(_, t)| t.clone()))
        .collect();
    assert_eq!(titulos, ["web · túneis"]);
    encerrar();
}
