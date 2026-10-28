//! Os campos por tipo de conexão (M60): a grade de propriedades mostra só o que vale para o
//! tipo, pela tabela de `src/campos.rs`; o que depende de outro campo continua desabilitado;
//! nada se apaga ao trocar o tipo.

mod comum;

use tirech_omni::campos::Campo;
use tirech_omni::modelo::{Connection, FtpSeguranca, Jump, Kind, Tree};
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, escolher_propriedade, montar, valor};

/// Cada linha da grade da conexão com o campo da tabela que a governa (`None`: vale sempre).
const LINHAS: &[(&str, Option<Campo>)] = &[
    ("Nome", None),
    ("Descrição", None),
    ("Tipo", None),
    ("Endereço do link", Some(Campo::Url)),
    ("Host", Some(Campo::Host)),
    ("Porta", Some(Campo::Porta)),
    ("Usuário", Some(Campo::Usuario)),
    ("Credencial", Some(Campo::Login)),
    ("Senha", Some(Campo::Login)),
    ("Chave do cofre", Some(Campo::ChaveSsh)),
    ("Chave privada", Some(Campo::ChaveSsh)),
    ("Rede", Some(Campo::Rede)),
    ("Provedor de IA", Some(Campo::Ia)),
    ("Assistentes (MCP)", Some(Campo::Mcp)),
    ("Provedor", Some(Campo::S3)),
    ("Endereço", Some(Campo::S3)),
    ("Região", Some(Campo::S3)),
    ("Bucket", Some(Campo::S3)),
    ("Estilo da URL", Some(Campo::S3)),
    ("Token de sessão", Some(Campo::S3)),
    ("Classe de armazenamento", Some(Campo::S3)),
    ("Certificado", Some(Campo::S3)),
    ("Arquivo da CA", Some(Campo::S3)),
    ("Remoto", Some(Campo::Rclone)),
    ("Caminho", Some(Campo::Rclone)),
    ("Segurança", Some(Campo::Ftp)),
    ("Modo", Some(Campo::Ftp)),
    ("Versão do TLS", Some(Campo::Ftp)),
    ("Codificação", Some(Campo::Ftp)),
    ("Fuso do servidor", Some(Campo::Ftp)),
    ("Portas do modo ativo", Some(Campo::Ftp)),
    ("Tipo de terminal", Some(Campo::TipoDeTerminal)),
    ("Log", Some(Campo::Log)),
    ("Keepalive", Some(Campo::SessaoSsh)),
    ("Comprimir a conexão", Some(Campo::SessaoSsh)),
    ("Sessão persistente", Some(Campo::Persistente)),
    ("Mouse no terminal", Some(Campo::Mouse)),
    ("Tentar de novo após", Some(Campo::Reconectar)),
    ("Programa", Some(Campo::Programa)),
    ("Argumentos", Some(Campo::Programa)),
    ("Pasta inicial", Some(Campo::Programa)),
    ("Ao colar", Some(Campo::Colar)),
    ("Pasta no servidor", Some(Campo::Colar)),
    ("Shell do servidor", Some(Campo::Colar)),
    ("Pasta", Some(Campo::IaNoServidor)),
    ("Instalar por", Some(Campo::IaNoServidor)),
    ("Argumentos do Claude Code", Some(Campo::IaNoServidor)),
    ("Argumentos do Codex", Some(Campo::IaNoServidor)),
    ("Argumentos do OpenCode", Some(Campo::IaNoServidor)),
    ("Pasta local padrão", Some(Campo::Arquivos)),
    ("Pasta remota padrão", Some(Campo::Arquivos)),
    ("Comparar pastas", Some(Campo::Arquivos)),
    ("Sincronizar pastas", Some(Campo::Arquivos)),
    ("Comandos", Some(Campo::Comandos)),
    ("Encaminhar X11", Some(Campo::Encaminhamento)),
    ("Túneis", Some(Campo::Encaminhamento)),
    ("Só túneis (sem shell)", Some(Campo::Encaminhamento)),
    ("Parar se túnel falhar", Some(Campo::Encaminhamento)),
    ("Host do jump", Some(Campo::Salto)),
    ("Porta do jump", Some(Campo::Salto)),
    ("Usuário do jump", Some(Campo::Salto)),
    ("Credencial do jump", Some(Campo::Salto)),
    ("Senha do jump", Some(Campo::Salto)),
];

/// Uma conexão de cada tipo (os ids são 1 a 7, na ordem de `Kind::TODOS`).
fn uma_de_cada() -> Tree {
    let mut tree = Tree::default();
    for tipo in Kind::TODOS {
        tree.add_connection(None, Connection::new(format!("c-{tipo:?}"), tipo));
    }
    tree
}

fn linha(app: &TestApp, nome: &str) -> tomni_ui::PropertyInfo {
    app.properties("props")
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a propriedade {nome}"))
}

fn escolher_tipo(app: &mut TestApp, tipo: Kind) {
    escolher_propriedade(app, "Tipo");
    app.select("props-Tipo-valor", &tipo.nome());
}

#[test]
fn a_grade_segue_a_tabela_em_cada_tipo() {
    let pasta = Pasta::nova("campos-tabela");
    pasta.semear(&uma_de_cada());
    let mut m = montar(&pasta, Theme::Light);
    for (n, tipo) in Kind::TODOS.into_iter().enumerate() {
        m.app.click(&format!("arvore-{}", n + 1));
        assert_eq!(valor(&m.app, "Tipo"), tipo.nome());
        let props = m.app.properties("props");
        assert_eq!(
            props.len(),
            LINHAS.len(),
            "linhas novas na grade de {tipo:?}"
        );
        for (nome, campo) in LINHAS {
            let p = props
                .iter()
                .find(|p| p.name == *nome)
                .unwrap_or_else(|| panic!("sem a propriedade {nome}"));
            let esperado = campo.is_some_and(|c| !c.vale(tipo));
            assert_eq!(p.hidden, esperado, "{nome} no tipo {tipo:?}");
        }
    }
}

#[test]
fn o_shell_local_mostra_so_o_que_vale() {
    let pasta = Pasta::nova("campos-local");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("Shell", Kind::Local));
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    let visiveis: Vec<String> = m
        .app
        .properties("props")
        .into_iter()
        .filter(|p| !p.hidden)
        .map(|p| p.name)
        .collect();
    assert_eq!(
        visiveis,
        [
            "Nome",
            "Descrição",
            "Tipo",
            "Provedor de IA",
            "Assistentes (MCP)",
            "Log",
            "Mouse no terminal",
            "Programa",
            "Argumentos",
            "Pasta inicial",
            "Comandos",
        ]
    );
    for p in m.app.properties("props").into_iter().filter(|p| !p.hidden) {
        assert_ne!(p.value, "0", "{} mostra 0", p.name);
    }
    m.app.assert_lint();
}

#[test]
fn trocar_o_tipo_e_voltar_devolve_o_que_estava_preenchido() {
    let pasta = Pasta::nova("campos-volta");
    let mut tree = Tree::default();
    let mut s3 = Connection::new("bucket", Kind::S3);
    s3.s3.endpoint = "https://minio.exemplo:9000".into();
    s3.s3.bucket = "bkt".into();
    s3.s3.region = "sa-east-1".into();
    tree.add_connection(None, s3);
    let mut ssh = Connection::new("web", Kind::Ssh);
    ssh.host = "web.exemplo".into();
    ssh.port = 2222;
    ssh.jump = Some(Jump {
        host: "bastiao.exemplo".into(),
        port: 2200,
        user: "ana".into(),
        credencial: None,
    });
    tree.add_connection(None, ssh);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);

    m.app.click("arvore-1");
    escolher_tipo(&mut m.app, Kind::Ssh);
    for nome in ["Endereço", "Bucket", "Região"] {
        assert!(linha(&m.app, nome).hidden, "{nome} deveria sumir no SSH");
    }
    assert!(!linha(&m.app, "Host").hidden);
    escolher_tipo(&mut m.app, Kind::S3);
    assert_eq!(valor(&m.app, "Endereço"), "https://minio.exemplo:9000");
    assert_eq!(valor(&m.app, "Bucket"), "bkt");
    assert_eq!(valor(&m.app, "Região"), "sa-east-1");
    let c = pasta.conexoes().connection(1).cloned().unwrap();
    assert_eq!(c.s3.bucket, "bkt");
    assert_eq!(c.s3.endpoint, "https://minio.exemplo:9000");

    m.app.click("arvore-2");
    escolher_tipo(&mut m.app, Kind::Local);
    for nome in ["Host", "Porta", "Host do jump"] {
        assert!(linha(&m.app, nome).hidden, "{nome} deveria sumir no Local");
    }
    // Os valores escondidos continuam no arquivo.
    let c = pasta.conexoes().connection(2).cloned().unwrap();
    assert_eq!(c.host, "web.exemplo");
    assert_eq!(c.jump.as_ref().map(|j| j.port), Some(2200));
    escolher_tipo(&mut m.app, Kind::Ssh);
    assert_eq!(valor(&m.app, "Host"), "web.exemplo");
    assert_eq!(valor(&m.app, "Host do jump"), "bastiao.exemplo");
    assert_eq!(valor(&m.app, "Porta do jump"), "2200");
    let c = pasta.conexoes().connection(2).cloned().unwrap();
    assert_eq!(c.kind, Kind::Ssh);
    assert_eq!(c.host, "web.exemplo");
    assert_eq!(
        c.jump.as_ref().map(|j| j.host.as_str()),
        Some("bastiao.exemplo")
    );
}

#[test]
fn a_porta_padrao_segue_o_tipo_e_so_a_diferente_fica_em_negrito() {
    let pasta = Pasta::nova("campos-porta");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("ssh", Kind::Ssh));
    tree.add_connection(None, Connection::new("telnet", Kind::Telnet));
    tree.add_connection(None, Connection::new("ftp", Kind::Ftp));
    let mut implicito = Connection::new("ftps", Kind::Ftp);
    implicito.ftp_security = FtpSeguranca::Implicita;
    implicito.port = 990;
    tree.add_connection(None, implicito);
    let mut outra = Connection::new("ssh-2222", Kind::Ssh);
    outra.port = 2222;
    tree.add_connection(None, outra);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    for (id, porta, mudou) in [
        (1, "22", false),
        (2, "23", false),
        (3, "21", false),
        (4, "990", false),
        (5, "2222", true),
    ] {
        m.app.click(&format!("arvore-{id}"));
        let p = linha(&m.app, "Porta");
        assert_eq!(
            (p.value.as_str(), p.changed),
            (porta, mudou),
            "conexão {id}"
        );
    }

    // Trocar a segurança troca a porta que estava no padrão e continua sem negrito.
    m.app.click("arvore-3");
    escolher_propriedade(&mut m.app, "Segurança");
    m.app.select("props-Segurança-valor", "FTPS implícito");
    let p = linha(&m.app, "Porta");
    assert_eq!((p.value.as_str(), p.changed), ("990", false));
}

#[test]
fn o_telnet_tem_credencial_e_senha() {
    let pasta = Pasta::nova("campos-telnet");
    let ids = pasta.semear_cofre(vec![comum::item_ssh("roteador", "admin", "s3nh4")]);
    let mut tree = Tree::default();
    let mut telnet = Connection::new("Roteador", Kind::Telnet);
    telnet.host = "192.168.0.1".into();
    tree.add_connection(None, telnet);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    for nome in ["Credencial", "Senha"] {
        let p = linha(&m.app, nome);
        assert!(!p.hidden && p.enabled, "{nome} no Telnet");
    }
    assert!(
        linha(&m.app, "Usuário").enabled && !linha(&m.app, "Usuário").hidden,
        "o usuário também vale no Telnet"
    );
    escolher_propriedade(&mut m.app, "Credencial");
    m.app
        .key(tomni_ui::Key::F(4))
        .click(&format!("props-Credencial-valor-roteador--{}-", ids[0]));
    assert_eq!(
        pasta.conexoes().connection(1).unwrap().credencial,
        Some(ids[0])
    );
}

#[test]
fn com_o_cofre_bloqueado_a_credencial_do_telnet_fica_desabilitada_mas_visivel() {
    let pasta = Pasta::nova("campos-telnet-bloqueado");
    pasta.semear_cofre(vec![comum::item_ssh("roteador", "admin", "s3nh4")]);
    pasta.cofre_com_senha_mestra("mestra");
    let mut tree = Tree::default();
    tree.add_connection(None, Connection::new("Roteador", Kind::Telnet));
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("cofre-desbloquear-close");
    m.app.click("arvore-1");
    for nome in ["Credencial", "Senha"] {
        let p = linha(&m.app, nome);
        assert!(!p.hidden && !p.enabled, "{nome} com o cofre bloqueado");
    }
}

#[test]
fn o_que_depende_de_outro_campo_continua_visivel_e_desabilitado() {
    let pasta = Pasta::nova("campos-dependencias");
    let mut tree = Tree::default();
    let mut ftp = Connection::new("arquivos", Kind::Ftp);
    ftp.host = "ftp.exemplo".into();
    tree.add_connection(None, ftp);
    tree.add_connection(None, Connection::new("bucket", Kind::S3));
    let mut ssh = Connection::new("web", Kind::Ssh);
    ssh.host = "web.exemplo".into();
    tree.add_connection(None, ssh);
    pasta.semear(&tree);
    let mut m = montar(&pasta, Theme::Light);
    let desabilitada = |app: &TestApp, nome: &str| {
        let p = linha(app, nome);
        !p.hidden && !p.enabled
    };
    m.app.click("arvore-1");
    assert!(
        desabilitada(&m.app, "Portas do modo ativo"),
        "só no modo ativo"
    );
    assert!(!linha(&m.app, "Modo").hidden && linha(&m.app, "Modo").enabled);
    m.app.click("arvore-2");
    assert!(desabilitada(&m.app, "Arquivo da CA"), "só com CA própria");
    m.app.click("arvore-3");
    assert!(
        desabilitada(&m.app, "Credencial do jump"),
        "sem host do jump"
    );
    assert!(desabilitada(&m.app, "Senha do jump"), "sem host do jump");
}

#[test]
fn a_grade_da_pasta_nao_muda() {
    let pasta = Pasta::nova("campos-pasta");
    pasta.semear(&comum::exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-1");
    let linhas = m.app.properties("props-grupo");
    let nomes: Vec<&str> = linhas.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        nomes,
        [
            "Rede",
            "Assistentes (MCP)",
            "Sessão persistente",
            "Mouse no terminal",
            "Tentar de novo após",
            "Credencial",
            "Comandos"
        ]
    );
    assert!(linhas.iter().all(|p| !p.hidden && p.enabled));
}
