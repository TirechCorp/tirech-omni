//! O que os testes do painel de portas (M58) dividem: uma descoberta de exemplo, a árvore com
//! uma conexão SSH e o jeito de abrir o painel dela com uma fonte falsa no lugar do servidor.

use std::io;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};

use tirech_omni::modelo::{Connection, Kind, Tree};
use tirech_omni::portas::descobrir::FonteFalsa;
use tirech_omni::portas::{
    Alcance, Atalho, Conteiner, Descoberta, Fonte, PortaDoServidor, Processo, Servico, Tipo,
};
use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use crate::comum::{Montagem, Pasta, montar};

pub const LOCAL: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

pub fn porta(n: u16, servico: Servico) -> PortaDoServidor {
    PortaDoServidor {
        porta: n,
        enderecos: vec![LOCAL],
        alcance: Alcance::SoNoServidor,
        destino: "127.0.0.1".into(),
        processo: None,
        usuario: None,
        conteiner: None,
        servico,
        atalhos: Vec::new(),
        inode: None,
    }
}

pub fn conhecido(nome: &str, tipo: Tipo, versao: Option<&str>) -> Servico {
    Servico::Conhecido {
        nome: nome.into(),
        tipo,
        versao: versao.map(str::to_owned),
    }
}

/// MySQL, um site com phpMyAdmin, o sshd e o resolvedor do sistema.
pub fn descoberta() -> Descoberta {
    let mut mysql = porta(3306, conhecido("MySQL/MariaDB", Tipo::Banco, Some("10.11")));
    mysql.processo = Some(Processo {
        pid: Some(812),
        nome: "mariadbd".into(),
    });
    mysql.usuario = Some("mysql".into());
    let mut site = porta(
        8080,
        Servico::Web {
            https: false,
            titulo: Some("Painel da loja".into()),
            servidor: None,
            codigo: 200,
        },
    );
    site.alcance = Alcance::TodaARede;
    site.enderecos = vec![IpAddr::V4(Ipv4Addr::UNSPECIFIED)];
    let mut php = porta(
        8081,
        Servico::Web {
            https: false,
            titulo: None,
            servidor: None,
            codigo: 200,
        },
    );
    php.atalhos = vec![Atalho {
        nome: "phpMyAdmin".into(),
        caminho: "/phpmyadmin/".into(),
    }];
    let mut ssh = porta(22, conhecido("SSH", Tipo::Sistema, None));
    ssh.alcance = Alcance::TodaARede;
    let dns = porta(53, conhecido("DNS", Tipo::Sistema, None));
    Descoberta {
        portas: vec![ssh, dns, mysql, site, php],
        conteineres: vec![Conteiner {
            nome: "loja_db".into(),
            redes: vec![("bridge".into(), IpAddr::V4(Ipv4Addr::new(172, 17, 0, 2)))],
            portas: vec![(5432, None), (80, Some(8082))],
            alcancavel: true,
        }],
        fonte: Fonte::Ss,
        avisos: vec!["Sem acesso ao Docker.".into()],
    }
}

pub fn arvore() -> Tree {
    let mut tree = Tree::default();
    let mut web = Connection::new("web", Kind::Ssh);
    web.host = "web.exemplo".into();
    web.user = "ana".into();
    web.key_file = Some("/chaves/ana".into());
    tree.add_connection(None, web);
    tree
}

/// Se o widget existe e aparece (as linhas da lista só existem enquanto estão nela).
pub fn aparece(app: &TestApp, id: &str) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.visible(id))).unwrap_or(false)
}

/// Apaga o texto do campo `id` e digita `texto`.
pub fn escrever(app: &mut TestApp, id: &str, texto: &str) {
    app.click(id).key(Key::End);
    for _ in 0..app.text(id).chars().count() + 4 {
        app.key(Key::Backspace);
    }
    app.type_text(texto);
}

pub fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Abre a aba SSH `web` com a fonte `respostas` e o painel de portas dela.
pub fn abrir(
    nome: &str,
    respostas: Vec<io::Result<Descoberta>>,
    tema: Theme,
) -> (Pasta, Montagem, FonteFalsa) {
    abrir_configurando(nome, respostas, tema, |_| {})
}

/// Como [`abrir`], com `configurar` rodando na pasta antes de o app subir.
pub fn abrir_configurando(
    nome: &str,
    respostas: Vec<io::Result<Descoberta>>,
    tema: Theme,
    configurar: impl FnOnce(&Pasta),
) -> (Pasta, Montagem, FonteFalsa) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&arvore());
    configurar(&pasta);
    let mut m = montar(&pasta, tema);
    let fonte = FonteFalsa::new(respostas);
    m.opener.fonte_de_portas.replace(Some(fonte.clone()));
    m.app.double_click("arvore-1");
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("portas-da-sessao");
    (pasta, m, fonte)
}
