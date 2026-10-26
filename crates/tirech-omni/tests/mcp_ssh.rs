//! O espaço de trabalho do MCP (M51, fatia 6) contra um servidor SSH local de verdade, com SFTP
//! e `exec`: `listar_espacos`, `ler`, `editar`, `escrever`, `buscar`, `glob` e `bash`, e as
//! ferramentas de operação que passam pelo canal da aba (`executar` separado, `listar_arquivos`,
//! `baixar_arquivo`). O assistente é um cliente do canal; o servidor é o de `tests/comum`.

#![cfg(unix)]

#[path = "comum/assistente.rs"]
mod assistente;
mod comum;
#[path = "comum/servidor.rs"]
mod servidor;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};
use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
use tirech_omni::mcp::permissao::PermissaoIa;
use tirech_omni::modelo::{Connection, Kind, Tree};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use assistente::Assistente;
use comum::{Pasta, item_ssh, montar_com};
use servidor::Servidor;

/// Abre a sessão SSH de verdade no servidor de teste, sem perguntar nada.
struct AbreSsh(Rc<Servidor>);

impl Opener for AbreSsh {
    fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
        Session::new(self.0.builder().connect())
            .key(chave_da_sessao(conexao, n))
            .name(conexao.name.clone())
    }
}

/// O app, o servidor e o assistente ligado ao espaço `proj`.
struct Cenario {
    pasta: Pasta,
    servidor: Rc<Servidor>,
    app: TestApp,
    ia: Assistente,
}

impl Cenario {
    /// Um espaço `proj` na raiz `/proj` do servidor, com a conexão em `permissao`.
    fn novo(nome: &str, permissao: PermissaoIa) -> Self {
        Self::com(nome, permissao, "/proj", Servidor::novo)
    }

    fn com(
        nome: &str,
        permissao: PermissaoIa,
        raiz: &str,
        sobe: impl FnOnce(&std::path::Path) -> Servidor,
    ) -> Self {
        Self::preparado(nome, permissao, raiz, sobe, |_| {})
    }

    /// Como `com`, mas deixa preparar a pasta (o cofre, por exemplo) antes de o app abrir.
    fn preparado(
        nome: &str,
        permissao: PermissaoIa,
        raiz: &str,
        sobe: impl FnOnce(&std::path::Path) -> Servidor,
        prepara: impl FnOnce(&Pasta),
    ) -> Self {
        let pasta = Pasta::nova(nome);
        prepara(&pasta);
        let servidor = Rc::new(sobe(&pasta.caminho().join("servidor")));
        std::fs::create_dir_all(servidor.real("/proj")).unwrap();
        let mut tree = Tree::default();
        let mut c = Connection::new("local", Kind::Ssh);
        c.host = "127.0.0.1".into();
        c.port = servidor.porta;
        c.user = "ana".into();
        c.key_file = Some("/chaves/ana".into());
        c.mcp = permissao;
        tree.add_connection(None, c);
        pasta.semear(&tree);
        pasta.configurar("ia", "mcp_ligado", json!(true));
        // A sondagem dos assistentes de IA (M46) também é um `exec`: fora daqui, para a lista dos
        // comandos ter só os do MCP.
        pasta.configurar("ia", "verificar_ao_abrir", json!(false));
        pasta.configurar(
            "ia",
            "espacos",
            json!([{ "nome": "proj", "conexao": 1, "raiz": raiz }]),
        );
        pasta.configurar(
            "ia",
            "pasta_troca",
            json!(pasta.caminho().join("troca").to_string_lossy()),
        );
        let endereco = pasta.0.join("mcp.sock");
        let (mut app, _fila) = montar_com(&pasta, Theme::Light, AbreSsh(Rc::clone(&servidor)));
        let ia = Assistente::ligar_com(&mut app, &endereco, Some("Teste"), Some("proj"));
        Self {
            pasta,
            servidor,
            app,
            ia,
        }
    }

    /// Chama uma ferramenta do espaço e espera a resposta.
    fn chamar(&mut self, ferramenta: &str, args: Value) -> assistente::Resposta {
        self.ia.chamar(&mut self.app, ferramenta, args)
    }

    fn esperar(&mut self, o_que: &str, mut pronto: impl FnMut(&mut TestApp) -> bool) {
        let limite = Instant::now() + Duration::from_secs(15);
        while !pronto(&mut self.app) {
            assert!(Instant::now() < limite, "esperando {o_que}");
            self.app.wait(Duration::from_millis(20));
        }
    }
}

/// As linhas do `texto` que terminam com `fim` (os caminhos que o `glob` achou).
fn caminhos(texto: &str, fim: &str) -> Vec<String> {
    let mut achados: Vec<String> = texto
        .lines()
        .map(str::trim)
        .filter(|l| l.ends_with(fim))
        .map(str::to_owned)
        .collect();
    achados.sort();
    achados
}

fn git_existe() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn espaco_abre_aba_e_resolve_a_raiz() {
    let mut c = Cenario::novo("mcp-ssh-abre", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"oi\n");
    assert!(
        Session::find("1-1").is_none(),
        "nenhuma aba antes do pedido"
    );

    let lista = c.ia.chamar(&mut c.app, "listar_espacos", json!({}));
    let _ = lista; // com `--espaco` a ferramenta some; o pedido volta com erro de protocolo

    let r = c.chamar("ler", json!({ "caminho": "a.txt" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("oi"), "{}", r.texto());
    assert!(
        Session::find("1-1").is_some(),
        "o app abriu a aba da conexão"
    );
    assert_eq!(c.servidor.sftps(), 1);
    let abas = c.chamar("listar_abas", json!({}));
    assert!(abas.texto().contains("(IA)"), "{}", abas.texto());
}

#[test]
fn raiz_inexistente_da_erro() {
    let mut c = Cenario::com("mcp-ssh-sem-raiz", PermissaoIa::Livre, "/nada", |p| {
        Servidor::novo(p)
    });
    let r = c.chamar("ler", json!({ "caminho": "a.txt" }));
    assert!(r.e_erro());
    assert!(
        r.texto().contains("A pasta /nada não existe no servidor."),
        "{}",
        r.texto()
    );
}

#[test]
fn espaco_que_nao_existe_diz_onde_criar() {
    let mut c = Cenario::novo("mcp-ssh-sem-espaco", PermissaoIa::Livre);
    let endereco = c.pasta.0.join("mcp.sock");
    let mut outra = Assistente::ligar_com(&mut c.app, &endereco, Some("Outra"), Some("fantasma"));
    let r = outra.chamar(&mut c.app, "ler", json!({ "caminho": "a.txt" }));
    assert!(r.e_erro());
    assert!(
        r.texto()
            .contains("O espaço fantasma não existe. Crie em Configurações > Assistentes de IA."),
        "{}",
        r.texto()
    );
}

#[test]
fn listar_espacos_mostra_nome_conexao_host_e_raiz() {
    let mut c = Cenario::novo("mcp-ssh-lista", PermissaoIa::Livre);
    let endereco = c.pasta.0.join("mcp.sock");
    let mut livre = Assistente::ligar(&mut c.app, &endereco, "Sem espaço");
    let r = livre.chamar(&mut c.app, "listar_espacos", json!({}));
    assert!(!r.e_erro(), "{}", r.texto());
    let texto = r.texto();
    assert!(texto.contains("\"nome\": \"proj\""), "{texto}");
    assert!(texto.contains("\"conexao\": \"local\""), "{texto}");
    assert!(texto.contains("127.0.0.1"), "{texto}");
    assert!(texto.contains("\"raiz\": \"/proj\""), "{texto}");
}

#[test]
fn ler_com_inicio_e_linhas() {
    let mut c = Cenario::novo("mcp-ssh-ler", PermissaoIa::Livre);
    let texto: String = (1..=10).map(|n| format!("linha{n}\n")).collect();
    c.servidor.escrever("/proj/dez.txt", texto.as_bytes());
    let r = c.chamar(
        "ler",
        json!({ "caminho": "dez.txt", "inicio": 3, "linhas": 2 }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    let t = r.texto();
    assert!(t.contains("linha3") && t.contains("linha4"), "{t}");
    assert!(!t.contains("linha2") && !t.contains("linha5"), "{t}");
    assert!(t.contains("inicio=5"), "diz de onde continuar: {t}");
}

#[test]
fn ler_binario_recusa() {
    let mut c = Cenario::novo("mcp-ssh-binario", PermissaoIa::Livre);
    c.servidor.escrever("/proj/b.bin", &[0u8, 1, 2, 0, 255, 0]);
    let r = c.chamar("ler", json!({ "caminho": "b.bin" }));
    assert!(r.texto().contains("binário"), "{}", r.texto());
    assert!(!r.texto().contains('\u{0}'));
}

#[test]
fn editar_sem_ler_recusa() {
    let mut c = Cenario::novo("mcp-ssh-editar-sem-ler", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"foo\n");
    let r = c.chamar(
        "editar",
        json!({ "caminho": "a.txt", "antigo": "foo", "novo": "bar" }),
    );
    assert!(r.e_erro());
    assert!(
        r.texto().contains("Leia o arquivo antes de editar."),
        "{}",
        r.texto()
    );
    assert_eq!(
        std::fs::read(c.servidor.real("/proj/a.txt")).unwrap(),
        b"foo\n"
    );
}

#[test]
fn editar_troca_e_preserva_o_modo() {
    let mut c = Cenario::novo("mcp-ssh-editar", PermissaoIa::Livre);
    c.servidor.escrever("/proj/run.sh", b"echo foo\n");
    let real = c.servidor.real("/proj/run.sh");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!c.chamar("ler", json!({ "caminho": "run.sh" })).e_erro());
    let r = c.chamar(
        "editar",
        json!({ "caminho": "run.sh", "antigo": "foo", "novo": "bar" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("Editado: 1 troca(s)."), "{}", r.texto());
    assert_eq!(std::fs::read(&real).unwrap(), b"echo bar\n");
    let modo = std::fs::metadata(&real).unwrap().permissions().mode() & 0o777;
    assert_eq!(modo, 0o755, "o modo continua o mesmo");
    // Nenhuma reserva fica para trás.
    let sobras: Vec<_> = std::fs::read_dir(c.servidor.real("/proj"))
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(sobras, ["run.sh"]);
}

#[test]
fn editar_depois_de_mudanca_no_servidor_recusa() {
    let mut c = Cenario::novo("mcp-ssh-editar-mudou", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"foo\n");
    assert!(!c.chamar("ler", json!({ "caminho": "a.txt" })).e_erro());
    c.servidor
        .escrever("/proj/a.txt", "foo, e alguém mexeu\n".as_bytes());
    let r = c.chamar(
        "editar",
        json!({ "caminho": "a.txt", "antigo": "foo", "novo": "bar" }),
    );
    assert!(r.e_erro());
    assert!(
        r.texto().contains("O arquivo mudou desde que você leu."),
        "{}",
        r.texto()
    );
}

#[test]
fn escrever_cria_as_pastas_dentro_da_raiz() {
    let mut c = Cenario::novo("mcp-ssh-escrever", PermissaoIa::Livre);
    let r = c.chamar(
        "escrever",
        json!({ "caminho": "novo/fundo/x.txt", "conteudo": "um\ndois\n" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(
        r.texto().contains("Gravado: 8 bytes, 2 linhas."),
        "{}",
        r.texto()
    );
    assert_eq!(
        std::fs::read(c.servidor.real("/proj/novo/fundo/x.txt")).unwrap(),
        b"um\ndois\n"
    );
    let fora = c.chamar(
        "escrever",
        json!({ "caminho": "../fora.txt", "conteudo": "não" }),
    );
    assert!(fora.e_erro(), "{}", fora.texto());
    assert!(!c.servidor.real("/fora.txt").exists());
}

#[test]
fn escrever_por_cima_exige_leitura() {
    let mut c = Cenario::novo("mcp-ssh-escrever-por-cima", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"antigo\n");
    let sem_ler = c.chamar(
        "escrever",
        json!({ "caminho": "a.txt", "conteudo": "novo\n" }),
    );
    assert!(sem_ler.e_erro());
    assert!(
        sem_ler
            .texto()
            .contains("Leia o arquivo antes de sobrescrever."),
        "{}",
        sem_ler.texto()
    );
    assert!(!c.chamar("ler", json!({ "caminho": "a.txt" })).e_erro());
    let lido = c.chamar(
        "escrever",
        json!({ "caminho": "a.txt", "conteudo": "novo\n" }),
    );
    assert!(!lido.e_erro(), "{}", lido.texto());
    assert_eq!(
        std::fs::read(c.servidor.real("/proj/a.txt")).unwrap(),
        b"novo\n"
    );
}

#[test]
fn link_para_fora_e_recusado() {
    let mut c = Cenario::novo("mcp-ssh-link", PermissaoIa::Livre);
    c.servidor.escrever("/segredo.txt", b"fora da raiz\n");
    std::os::unix::fs::symlink("../segredo.txt", c.servidor.real("/proj/fora")).unwrap();
    let r = c.chamar("ler", json!({ "caminho": "fora" }));
    assert!(r.e_erro(), "{}", r.texto());
    assert!(!r.texto().contains("fora da raiz"), "{}", r.texto());
    let ate = c.chamar("ler", json!({ "caminho": "../segredo.txt" }));
    assert!(ate.e_erro(), "{}", ate.texto());
}

#[test]
fn buscar_texto_pelo_grep() {
    let mut c = Cenario::novo("mcp-ssh-buscar", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"um\nalvo.aqui\ntres\n");
    c.servidor.escrever("/proj/sub/b.txt", b"nada\n");
    let r = c.chamar("buscar", json!({ "padrao": "alvo.aqui" }));
    assert!(!r.e_erro(), "{}", r.texto());
    let t = r.texto();
    assert!(t.contains("[busca:"), "{t}");
    assert!(t.contains("a.txt") && t.contains("alvo.aqui"), "{t}");
    assert!(!t.contains("b.txt"), "{t}");
    // O texto é literal: o ponto não casa qualquer letra.
    let nao = c.chamar("buscar", json!({ "padrao": "alvo-aqui" }));
    assert!(nao.texto().contains("Nada encontrado."), "{}", nao.texto());
    assert!(
        c.servidor.comandos().iter().any(|c| c.contains("grep")),
        "a busca foi pelo servidor"
    );
}

#[test]
fn buscar_regex() {
    let mut c = Cenario::novo("mcp-ssh-regex", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"caso 42\ncaso x\n");
    let r = c.chamar(
        "buscar",
        json!({ "padrao": "caso [0-9]+", "modo": "regex" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("caso 42"), "{}", r.texto());
    assert!(!r.texto().contains("caso x"), "{}", r.texto());
    let ruim = c.chamar("buscar", json!({ "padrao": "caso [", "modo": "regex" }));
    assert!(ruim.e_erro(), "{}", ruim.texto());
}

#[test]
fn buscar_sem_exec_vai_pelo_sftp() {
    let mut c = Cenario::com("mcp-ssh-sem-exec", PermissaoIa::Livre, "/proj", |p| {
        Servidor::sem_exec(p)
    });
    c.servidor.escrever("/proj/a.txt", b"um\nalvo\n");
    let r = c.chamar("buscar", json!({ "padrao": "alvo" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("[busca: pelo SFTP]"), "{}", r.texto());
    assert!(r.texto().contains("a.txt") && r.texto().contains("alvo"));
    assert!(c.servidor.comandos().is_empty(), "o exec foi recusado");
}

#[test]
fn buscar_com_git_grep() {
    if !git_existe() {
        eprintln!("sem o git no PATH: teste pulado");
        return;
    }
    let mut c = Cenario::novo("mcp-ssh-git-grep", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"um\nalvo\n");
    let iniciou = Command::new("git")
        .args(["init", "-q"])
        .current_dir(c.servidor.real("/proj"))
        .status()
        .unwrap();
    assert!(iniciou.success());
    let r = c.chamar("buscar", json!({ "padrao": "alvo" }));
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("[busca: git grep]"), "{}", r.texto());
    assert!(r.texto().contains("a.txt"), "{}", r.texto());
}

#[test]
fn glob_pelo_find_e_pelo_sftp_dao_o_mesmo() {
    let arquivos = |c: &Cenario| {
        c.servidor.escrever("/proj/a.rs", b"");
        c.servidor.escrever("/proj/src/b.rs", b"");
        c.servidor.escrever("/proj/src/fundo/c.rs", b"");
        c.servidor.escrever("/proj/src/d.txt", b"");
    };
    let com_exec = {
        let mut c = Cenario::novo("mcp-ssh-glob-exec", PermissaoIa::Livre);
        arquivos(&c);
        let r = c.chamar("glob", json!({ "padrao": "**/*.rs" }));
        assert!(!r.e_erro(), "{}", r.texto());
        assert!(
            c.servidor
                .comandos()
                .iter()
                .any(|c| c.contains("find") || c.contains("git")),
            "foi pelo servidor"
        );
        caminhos(&r.texto(), ".rs")
    };
    let pelo_sftp = {
        let mut c = Cenario::com("mcp-ssh-glob-sftp", PermissaoIa::Livre, "/proj", |p| {
            Servidor::sem_exec(p)
        });
        arquivos(&c);
        let r = c.chamar("glob", json!({ "padrao": "**/*.rs" }));
        assert!(!r.e_erro(), "{}", r.texto());
        caminhos(&r.texto(), ".rs")
    };
    assert_eq!(com_exec.len(), 3, "{com_exec:?}");
    assert_eq!(com_exec, pelo_sftp);
}

#[test]
fn glob_ordena_pelo_mais_recente() {
    let mut c = Cenario::novo("mcp-ssh-glob-data", PermissaoIa::Livre);
    c.servidor.escrever("/proj/velho.txt", b"");
    c.servidor.escrever("/proj/novo.txt", b"");
    let agora = SystemTime::now();
    let marcar = |nome: &str, antes: u64| {
        let arquivo = std::fs::File::open(c.servidor.real(nome)).unwrap();
        arquivo
            .set_modified(agora - Duration::from_secs(antes))
            .unwrap();
    };
    marcar("/proj/velho.txt", 3600);
    marcar("/proj/novo.txt", 10);
    let r = c.chamar("glob", json!({ "padrao": "*.txt" }));
    assert!(!r.e_erro(), "{}", r.texto());
    let t = r.texto();
    assert!(
        t.find("novo.txt").unwrap() < t.find("velho.txt").unwrap(),
        "{t}"
    );
}

#[test]
fn bash_roda_na_raiz() {
    let mut c = Cenario::novo("mcp-ssh-bash", PermissaoIa::Livre);
    let r = c.chamar("bash", json!({ "comando": "pwd" }));
    assert!(!r.e_erro(), "{}", r.texto());
    let real = c.servidor.real("/proj");
    assert!(
        r.texto().contains(&*real.to_string_lossy()),
        "{} em {}",
        real.display(),
        r.texto()
    );
    assert!(r.texto().contains("\"codigo\":0"), "{}", r.texto());
    // O comando segue o formato `cd <raiz> && <comando>`, sem terminal.
    let comandos = c.servidor.comandos();
    assert!(comandos[0].starts_with("cd "), "{comandos:?}");
    assert!(comandos[0].ends_with("&& pwd"), "{comandos:?}");
}

#[test]
fn bash_devolve_o_codigo() {
    let mut c = Cenario::novo("mcp-ssh-bash-codigo", PermissaoIa::Livre);
    let r = c.chamar(
        "bash",
        json!({ "comando": "echo parcial; echo falhou >&2; exit 3" }),
    );
    assert!(r.texto().contains("\"codigo\":3"), "{}", r.texto());
    assert!(r.texto().contains("parcial"), "{}", r.texto());
    assert!(r.texto().contains("falhou"), "{}", r.texto());
}

#[test]
fn bash_cancelado_fecha_o_canal() {
    let mut c = Cenario::novo("mcp-ssh-bash-cancela", PermissaoIa::Livre);
    let id = c.ia.pedir("bash", json!({ "comando": "sleep 60" }));
    let servidor = Rc::clone(&c.servidor);
    c.esperar("o comando chegar ao servidor", |_| {
        servidor.comandos().iter().any(|x| x.contains("sleep 60"))
    });
    c.ia.cancelar(id);
    c.esperar("o servidor ver o canal fechar", |_| {
        servidor.execs_cancelados() == 1
    });
    assert_eq!(
        c.servidor.execs_prontos(),
        0,
        "o comando não terminou sozinho"
    );
}

#[test]
fn bash_bloqueado_pergunta() {
    let mut c = Cenario::novo("mcp-ssh-bash-bloqueado", PermissaoIa::Livre);
    let id = c.ia.pedir("bash", json!({ "comando": "rm -rf /" }));
    c.esperar("o diálogo abrir", |app| app.visible("mcp-aprovar"));
    assert!(c.app.visible("mcp-aprovar-perigo"));
    c.app.click("mcp-aprovar-primary");
    let r = c.ia.resposta(&mut c.app, id);
    assert!(r.e_erro(), "{}", r.texto());
    assert!(
        !c.servidor.comandos().iter().any(|x| x.contains("rm -rf")),
        "o comando não chegou ao servidor"
    );
}

#[test]
fn editar_em_aprovar_mostra_o_antes_e_o_depois() {
    let mut c = Cenario::novo("mcp-ssh-aprovar-editar", PermissaoIa::Aprovar);
    c.servidor.escrever("/proj/a.txt", b"foo\n");
    // Ler não pergunta em Aprovar; só a edição.
    assert!(!c.chamar("ler", json!({ "caminho": "a.txt" })).e_erro());
    let id = c.ia.pedir(
        "editar",
        json!({ "caminho": "a.txt", "antigo": "foo", "novo": "bar" }),
    );
    c.esperar("o diálogo da edição", |app| {
        app.visible("mcp-aprovar") && app.text("mcp-aprovar-detalhe").contains("Antes:")
    });
    let detalhe = c.app.text("mcp-aprovar-detalhe");
    assert!(detalhe.contains("a.txt"), "{detalhe}");
    assert!(
        detalhe.contains("foo") && detalhe.contains("bar"),
        "{detalhe}"
    );
    assert_eq!(
        std::fs::read(c.servidor.real("/proj/a.txt")).unwrap(),
        b"foo\n"
    );
    c.app.click("mcp-aprovar-secondary");
    assert!(!c.ia.resposta(&mut c.app, id).e_erro());
    assert_eq!(
        std::fs::read(c.servidor.real("/proj/a.txt")).unwrap(),
        b"bar\n"
    );
}

#[test]
fn so_ler_deixa_ler_e_recusa_o_resto() {
    let mut c = Cenario::novo("mcp-ssh-so-ler", PermissaoIa::SoLer);
    c.servidor.escrever("/proj/a.txt", b"foo\n");
    assert!(!c.chamar("ler", json!({ "caminho": "a.txt" })).e_erro());
    assert!(!c.chamar("buscar", json!({ "padrao": "foo" })).e_erro());
    assert!(!c.chamar("glob", json!({ "padrao": "*" })).e_erro());
    for (ferramenta, args) in [
        ("bash", json!({ "comando": "ls" })),
        ("escrever", json!({ "caminho": "x", "conteudo": "y" })),
        (
            "editar",
            json!({ "caminho": "a.txt", "antigo": "foo", "novo": "bar" }),
        ),
    ] {
        let r = c.chamar(ferramenta, args);
        assert!(r.e_erro(), "{ferramenta}: {}", r.texto());
    }
    assert!(!c.app.visible("mcp-aprovar"));
}

#[test]
fn conexao_desligada_esconde_o_espaco() {
    let mut c = Cenario::novo("mcp-ssh-desligada", PermissaoIa::Desligada);
    let r = c.chamar("ler", json!({ "caminho": "a.txt" }));
    assert!(r.e_erro());
    assert!(
        r.texto().contains("O espaço proj não existe."),
        "{}",
        r.texto()
    );
}

/// Abre a aba da conexão pela árvore e espera a sessão estar de pé.
fn abrir_aba(c: &mut Cenario) {
    c.app.double_click("arvore-1");
    c.esperar("a aba abrir", |_| {
        Session::find("1-1").is_some_and(|s| s.ssh_channels().is_some())
    });
}

#[test]
fn executar_separado_devolve_codigo() {
    // Com "rodar os comandos na aba" desligada, o separado roda à parte mesmo com a aba livre.
    let mut c = Cenario::preparado(
        "mcp-ssh-separado",
        PermissaoIa::Livre,
        "/proj",
        Servidor::novo,
        |pasta| pasta.configurar("ia", "comandos_na_aba", json!(false)),
    );
    abrir_aba(&mut c);
    esperar_o_prompt(&mut c);
    let r = c.chamar(
        "executar",
        json!({ "aba": "1-1", "comando": "echo separado; exit 5", "modo": "separado" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(r.texto().contains("separado"), "{}", r.texto());
    assert!(r.texto().contains("\"codigo\":5"), "{}", r.texto());
    assert!(r.texto().contains("\"modo\":\"separado\""), "{}", r.texto());
    assert!(!c.servidor.log().iter().any(|l| l.contains("exit 5")));
}

/// Espera o prompt do servidor de teste aparecer na aba.
fn esperar_o_prompt(c: &mut Cenario) {
    c.esperar("o prompt aparecer", |_| {
        Session::find("1-1").is_some_and(|s| s.received() >= 32)
    });
}

#[test]
fn executar_separado_vai_para_a_aba_livre() {
    // O padrão de "rodar os comandos na aba": o pedido "separado" aparece na aba, à vista.
    let mut c = Cenario::novo("mcp-ssh-separado-na-aba", PermissaoIa::Livre);
    abrir_aba(&mut c);
    esperar_o_prompt(&mut c);
    let r = c.ia.pedir(
        "executar",
        json!({ "aba": "1-1", "comando": "echo visivel", "modo": "separado", "prazo_secs": 10 }),
    );
    let servidor = Rc::clone(&c.servidor);
    c.esperar("o comando chegar ao shell", |_| {
        servidor.log().iter().any(|l| l.contains("echo visivel"))
    });
    let resposta = c.ia.resposta(&mut c.app, r);
    assert!(!resposta.e_erro(), "{}", resposta.texto());
    assert!(
        resposta.texto().contains("\"modo\":\"aba\""),
        "{}",
        resposta.texto()
    );
    assert!(
        !c.servidor
            .comandos()
            .iter()
            .any(|x| x.contains("echo visivel"))
    );
}

#[test]
fn executar_na_aba_espera_o_prompt() {
    let mut c = Cenario::novo("mcp-ssh-aba-prompt", PermissaoIa::Livre);
    abrir_aba(&mut c);
    // O servidor de teste mostra um prompt e devolve o que se digita, como um terminal em eco.
    esperar_o_prompt(&mut c);
    let r = c.ia.pedir(
        "executar",
        json!({ "aba": "1-1", "comando": "echo oi", "prazo_secs": 10 }),
    );
    let servidor = Rc::clone(&c.servidor);
    c.esperar("o comando chegar ao shell", |_| {
        servidor.log().iter().any(|l| l.contains("echo oi"))
    });
    let resposta = c.ia.resposta(&mut c.app, r);
    assert!(!resposta.e_erro(), "{}", resposta.texto());
    assert!(
        resposta.texto().contains("\"motivo\":\"prompt\""),
        "{}",
        resposta.texto()
    );
}

#[test]
fn arquivos_de_operacao_pelo_canal_da_aba() {
    let mut c = Cenario::novo("mcp-ssh-canal-da-aba", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"conteudo\n");
    abrir_aba(&mut c);
    let aceitas = c.servidor.aceitas();
    let lista = c.chamar(
        "listar_arquivos",
        json!({ "aba": "1-1", "caminho": "/proj" }),
    );
    assert!(!lista.e_erro(), "{}", lista.texto());
    assert!(lista.texto().contains("a.txt"), "{}", lista.texto());
    let baixa = c.chamar(
        "baixar_arquivo",
        json!({ "aba": "1-1", "caminho": "/proj/a.txt" }),
    );
    assert!(baixa.texto().contains("conteudo"), "{}", baixa.texto());
    assert_eq!(
        c.servidor.aceitas(),
        aceitas,
        "o SFTP foi pela conexão da aba, sem outra ligação"
    );
    assert!(c.servidor.sftps() >= 1);
}

#[test]
fn destino_fora_da_pasta_de_troca_recusado() {
    let mut c = Cenario::novo("mcp-ssh-destino", PermissaoIa::Livre);
    c.servidor.escrever("/proj/a.txt", b"conteudo\n");
    abrir_aba(&mut c);
    let fora: PathBuf = c.pasta.caminho().join("fora.txt");
    let r = c.chamar(
        "baixar_arquivo",
        json!({ "aba": "1-1", "caminho": "/proj/a.txt", "destino": fora.to_string_lossy() }),
    );
    assert!(r.e_erro(), "{}", r.texto());
    assert!(!fora.exists());
    let dentro = c.pasta.caminho().join("troca").join("dentro.txt");
    let ok = c.chamar(
        "baixar_arquivo",
        json!({ "aba": "1-1", "caminho": "/proj/a.txt", "destino": dentro.to_string_lossy() }),
    );
    assert!(!ok.e_erro(), "{}", ok.texto());
    assert_eq!(std::fs::read(&dentro).unwrap(), b"conteudo\n");
}

#[test]
fn redacao_no_bash() {
    let mut c = Cenario::preparado(
        "mcp-ssh-redacao",
        PermissaoIa::Livre,
        "/proj",
        Servidor::novo,
        |pasta| {
            pasta.semear_cofre(vec![item_ssh("servidor", "root", "s3gr3d0-xyz-123")]);
        },
    );
    let r = c.chamar(
        "bash",
        json!({ "comando": "echo a senha é s3gr3d0-xyz-123 mesmo" }),
    );
    assert!(!r.e_erro(), "{}", r.texto());
    assert!(!r.texto().contains("s3gr3d0-xyz-123"), "{}", r.texto());
    assert!(r.texto().contains("a senha é"), "{}", r.texto());
}

#[test]
fn arquivo_solto_no_app_vale_como_origem_do_enviar_arquivo() {
    let mut c = Cenario::preparado(
        "mcp-ssh-solto-origem",
        PermissaoIa::Livre,
        "/proj",
        Servidor::novo,
        |pasta| pasta.configurar("terminal", "colar_arquivos", json!("caminho_local")),
    );
    abrir_aba(&mut c);
    std::fs::create_dir_all(c.pasta.caminho().join("docs")).unwrap();
    let solto = c.pasta.caminho().join("docs").join("instruções.md");
    std::fs::write(&solto, b"leia isto\n").unwrap();
    let outro = c.pasta.caminho().join("outro.md");
    std::fs::write(&outro, b"segredo\n").unwrap();
    let enviar = |c: &mut Cenario, origem: &std::path::Path| {
        c.chamar(
            "enviar_arquivo",
            json!({ "aba": "1-1", "caminho": "/proj/i.md", "origem": origem.to_string_lossy() }),
        )
    };

    // Fora da pasta de troca e sem a pessoa soltar: recusado.
    let r = enviar(&mut c, &solto);
    assert!(r.e_erro(), "{}", r.texto());
    // Soltar a pasta libera o que está dentro dela, e só isso.
    c.app
        .drop_files("paineis-terminal-1-1", &[c.pasta.caminho().join("docs")]);
    c.app.wait(Duration::from_millis(50));
    let ok = enviar(&mut c, &solto);
    assert!(!ok.e_erro(), "{}", ok.texto());
    assert_eq!(
        std::fs::read(c.servidor.real("/proj/i.md")).unwrap(),
        b"leia isto\n"
    );
    let fora = enviar(&mut c, &outro);
    assert!(fora.e_erro(), "{}", fora.texto());
    let subindo = c.pasta.caminho().join("docs").join("..").join("outro.md");
    let r = enviar(&mut c, &subindo);
    assert!(r.e_erro(), "{}", r.texto());
}
