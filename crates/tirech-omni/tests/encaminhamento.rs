//! X11 e túneis (M37) no abridor: o que cada conexão pede ao abrir e o que o `FakeOpener`
//! registra.

mod comum;

use std::path::PathBuf;

use tirech_omni::abrir::{
    AjustesSsh, Encaminhamento, encaminhamento, encaminhamento_de_programa, forward_do_tunel,
    pasta_dos_sockets,
};
use tirech_omni::configuracoes::{Configuracoes, PaginaInicial, X11Padrao};
use tirech_omni::modelo::{Connection, Kind, ModoX11, TipoTunel, Tree, Tunel};
use tomni_ui::terminal::Session;
use tomni_ui::terminal::ssh::{Bind, Forward, LocalSocket, X11Mode};
use tomni_ui::theme::Theme;

use comum::{Pasta, montar};

fn ssh(nome: &str) -> Connection {
    let mut c = Connection::new(nome, Kind::Ssh);
    c.id = 7;
    c.host = format!("{nome}.exemplo");
    c.user = "deploy".into();
    c.key_file = Some("/chaves/web".into());
    c
}

fn tunel(tipo: TipoTunel, porta: u16, host: &str, destino: u16) -> Tunel {
    let mut t = Tunel::novo(tipo, false);
    t.porta = porta;
    host.clone_into(&mut t.destino_host);
    t.destino_porta = destino;
    t
}

fn ajustes(padrao: X11Padrao) -> AjustesSsh {
    AjustesSsh {
        x11_padrao: padrao,
        ..AjustesSsh::default()
    }
}

#[test]
fn o_padrao_e_o_x11_confiavel_sem_tuneis() {
    let c = ssh("web");
    let configuracoes = Configuracoes::default();
    let enc = encaminhamento(&c, &AjustesSsh::de(&configuracoes));
    assert_eq!(
        enc,
        Encaminhamento {
            x11: Some((X11Mode::Trusted, None)),
            ..Encaminhamento::default()
        }
    );
}

#[test]
fn o_modo_da_conexao_vale_mais_que_o_padrao_global() {
    let mut c = ssh("web");
    let modo = |c: &Connection, p| encaminhamento(c, &ajustes(p)).x11.map(|(m, _)| m);
    assert_eq!(modo(&c, X11Padrao::Desligado), None);
    assert_eq!(modo(&c, X11Padrao::NaoConfiavel), Some(X11Mode::Untrusted));
    assert_eq!(modo(&c, X11Padrao::Confiavel), Some(X11Mode::Trusted));
    c.x11 = ModoX11::Desligado;
    assert_eq!(modo(&c, X11Padrao::Confiavel), None);
    c.x11 = ModoX11::NaoConfiavel;
    assert_eq!(modo(&c, X11Padrao::Confiavel), Some(X11Mode::Untrusted));
    c.x11 = ModoX11::Confiavel;
    assert_eq!(modo(&c, X11Padrao::Desligado), Some(X11Mode::Trusted));
}

#[test]
fn o_display_das_configuracoes_vai_junto() {
    let mut a = ajustes(X11Padrao::Confiavel);
    a.x11_display = "localhost:0".into();
    assert_eq!(
        encaminhamento(&ssh("web"), &a).x11,
        Some((X11Mode::Trusted, Some("localhost:0".to_owned())))
    );
}

#[test]
fn so_o_ssh_pede_x11_e_tuneis() {
    for tipo in [Kind::Sftp, Kind::Telnet, Kind::Local, Kind::Ftp] {
        let mut c = ssh("outro");
        c.kind = tipo;
        c.tuneis.push(tunel(TipoTunel::Local, 8080, "db", 5432));
        assert_eq!(
            encaminhamento(&c, &ajustes(X11Padrao::Confiavel)),
            Encaminhamento::default(),
            "{tipo:?}"
        );
    }
}

#[test]
fn os_tuneis_ligados_viram_forwards() {
    let mut c = ssh("web");
    c.tuneis = vec![
        tunel(TipoTunel::Local, 8080, "db", 5432),
        {
            let mut t = tunel(TipoTunel::Local, 8081, "off", 1);
            t.ligado = false;
            t
        },
        tunel(TipoTunel::Remoto, 9000, "localhost", 3000),
        tunel(TipoTunel::Dinamico, 1080, "", 0),
        // Sem destino: não vale e não vai.
        tunel(TipoTunel::Local, 8082, "", 0),
    ];
    c.so_tuneis = true;
    c.parar_se_tunel_falhar = true;
    let enc = encaminhamento(&c, &ajustes(X11Padrao::Desligado));
    assert_eq!(enc.x11, None);
    assert!(enc.so_tuneis && enc.parar_se_falhar);
    assert_eq!(
        enc.tuneis,
        [
            Forward::local(8080, "db", 5432),
            Forward::remote(9000, "localhost", 3000),
            Forward::dynamic(1080),
        ]
    );
}

#[test]
fn outras_maquinas_escuta_em_todas_as_interfaces() {
    let mut t = tunel(TipoTunel::Local, 8080, "db", 5432);
    t.outras_maquinas = true;
    assert_eq!(
        forward_do_tunel(7, 0, &t),
        Forward::local(8080, "db", 5432).open_to_network()
    );
}

#[test]
fn o_socket_unix_tem_caminho_automatico_por_conexao_e_posicao() {
    let mut t = Tunel::novo(TipoTunel::SocketUnix, false);
    let automatico = pasta_dos_sockets().join("sock-7-2.sock");
    let esperado = if cfg!(windows) {
        LocalSocket::Tcp(Bind::loopback(0))
    } else {
        LocalSocket::Path(automatico)
    };
    assert_eq!(
        forward_do_tunel(7, 2, &t),
        Forward::unix(esperado, "/var/run/libvirt/libvirt-sock")
    );
    if !cfg!(windows) {
        t.socket_local = "/tmp/meu.sock".into();
        assert_eq!(
            forward_do_tunel(7, 2, &t),
            Forward::unix(
                LocalSocket::Path(PathBuf::from("/tmp/meu.sock")),
                "/var/run/libvirt/libvirt-sock"
            )
        );
    }
    t.socket_local.clear();
    t.porta = 5555;
    assert_eq!(
        forward_do_tunel(7, 2, &t),
        Forward::unix(
            LocalSocket::Tcp(Bind::loopback(5555)),
            "/var/run/libvirt/libvirt-sock"
        )
    );
}

#[test]
fn o_programa_grafico_sempre_leva_x11_e_nunca_e_so_tuneis() {
    let mut c = ssh("web");
    c.x11 = ModoX11::Desligado;
    c.so_tuneis = true;
    let enc = encaminhamento_de_programa(&c, &ajustes(X11Padrao::NaoConfiavel));
    assert_eq!(enc.x11, Some((X11Mode::Trusted, None)));
    assert!(!enc.so_tuneis);
    c.x11 = ModoX11::NaoConfiavel;
    assert_eq!(
        encaminhamento_de_programa(&c, &ajustes(X11Padrao::Desligado)).x11,
        Some((X11Mode::Untrusted, None))
    );
}

/// Uma árvore com a conexão `web` (id 1), com um túnel e o X11 não confiável.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    let mut c = ssh("web");
    c.x11 = ModoX11::NaoConfiavel;
    c.tuneis.push(tunel(TipoTunel::Local, 8080, "db", 5432));
    tree.add_connection(None, c);
    tree
}

#[test]
fn a_aba_abre_com_o_encaminhamento_da_conexao() {
    let pasta = Pasta::nova("encaminhamento-abrir");
    pasta.semear(&arvore());
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    let registrados = m.opener.encaminhamentos.borrow().clone();
    assert_eq!(registrados.len(), 1);
    let (id, enc) = &registrados[0];
    assert_eq!(*id, 1);
    assert_eq!(enc.x11, Some((X11Mode::Untrusted, None)));
    assert_eq!(enc.tuneis, [Forward::local(8080, "db", 5432)]);
    for n in 1..=3 {
        if let Some(s) = Session::find(&format!("1-{n}")) {
            s.dispose();
        }
    }
}

#[test]
fn as_configuracoes_x11_chegam_ao_abridor() {
    let pasta = Pasta::nova("encaminhamento-config");
    let mut tree = Tree::default();
    tree.add_connection(None, ssh("web"));
    pasta.semear(&tree);
    let mut configuracoes = Configuracoes::default();
    configuracoes.geral.pagina_inicial = PaginaInicial::Terminais;
    configuracoes.agente.ligado = false;
    configuracoes.x11.padrao = X11Padrao::NaoConfiavel;
    configuracoes.x11.display = "localhost:0".into();
    configuracoes
        .salvar(&pasta.armazenamento())
        .expect("gravar as configurações");
    let mut m = montar(&pasta, Theme::Light);
    m.app.double_click("arvore-1");
    let registrados = m.opener.encaminhamentos.borrow().clone();
    assert_eq!(
        registrados[0].1.x11,
        Some((X11Mode::Untrusted, Some("localhost:0".to_owned())))
    );
    for n in 1..=3 {
        if let Some(s) = Session::find(&format!("1-{n}")) {
            s.dispose();
        }
    }
}
