//! A sessão persistente: a aba SSH roda dentro de uma sessão do `tmux` ou do GNU `screen` no
//! servidor, que continua viva quando a conexão cai. A parte pura: a herança pela pasta, o nome
//! da sessão, os comandos que o app manda ao servidor e a lista das sessões que já estão lá. O que
//! liga à tela mora em `tela/sessao_tmux.rs` e `tela/sessoes_servidor.rs`.
//!
//! A aba roda o comando de [`comando`] no lugar do shell (o `SshBuilder::command`): a primeira
//! vez cria a sessão, e a reconexão (que reabre com o mesmo comando) volta a ela. Sem o programa
//! no servidor, a aba avisa e abre o shell comum.
//!
//! O `screen` não tem o `new-session -A` do tmux: o comando procura a sessão no `screen -ls` (o
//! nome exato, depois do `pid.`) e liga a ela com `-x` (como o tmux, sem desligar a outra
//! ponta), ou a cria com `-S`. A sessão nova ganha 10 000 linhas de histórico (`-h`), já que o
//! padrão do screen é 100.

use tomni_ui::terminal::{ShellQuote, shell_quote};

use crate::modelo::{Kind, Multiplexador, Node, SessaoDaAba, SessaoPersistente, ShellRemoto, Tree};
use crate::t;

/// O começo do nome das sessões que o app cria.
pub const PREFIXO: &str = "tirech-";

/// O que uma escolha decide: `None` segue subindo, `Some(programa)` para (`Some(None)`: o shell
/// comum).
fn decide(persistente: SessaoPersistente) -> Option<Option<Multiplexador>> {
    match persistente {
        SessaoPersistente::Herdar => None,
        SessaoPersistente::Nenhuma => Some(None),
        SessaoPersistente::Tmux => Some(Some(Multiplexador::Tmux)),
        SessaoPersistente::Screen => Some(Some(Multiplexador::Screen)),
    }
}

/// A pasta que decide a sessão persistente do item `no`, subindo a partir da de cima, e o
/// programa que ela escolhe (`None`: o shell comum). `None` se nenhuma pasta decide.
pub fn herdada(tree: &Tree, no: u64) -> Option<(&str, Option<Multiplexador>)> {
    for grupo in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(grupo)
            && let Some(programa) = decide(g.persistente)
        {
            return Some((&g.name, programa));
        }
    }
    None
}

/// O programa em que as abas da conexão `conexao` rodam: só SSH com shell (sem "só túneis"),
/// num servidor que não foi marcado como Windows, e com a opção dela ou da pasta ligada.
pub fn programa_de(tree: &Tree, conexao: u64) -> Option<Multiplexador> {
    let c = tree.connection(conexao)?;
    if !aceita_sessao(c) {
        return None;
    }
    decide(c.persistente)
        .or_else(|| herdada(tree, conexao).map(|(_, programa)| programa))
        .flatten()
}

/// Se as abas da conexão `conexao` rodam numa sessão persistente.
pub fn usa_tmux(tree: &Tree, conexao: u64) -> bool {
    programa_de(tree, conexao).is_some()
}

/// Se a conexão `c` pode rodar numa sessão persistente: SSH com shell, fora do Windows.
pub fn aceita_sessao(c: &crate::modelo::Connection) -> bool {
    c.kind == Kind::Ssh
        && !c.so_tuneis
        && !matches!(c.shell_remoto, ShellRemoto::PowerShell | ShellRemoto::Cmd)
}

/// A opção "Nenhuma" da linha "Sessão persistente".
pub fn nenhuma() -> String {
    t!("tmux-nenhuma")
}

/// A opção "tmux" da linha.
pub fn tmux() -> String {
    t!("tmux-tmux")
}

/// A opção "screen" da linha.
pub fn screen() -> String {
    t!("tmux-screen")
}

/// O texto de uma escolha que vale: o programa ou "Nenhuma".
fn rotulo_do_programa(programa: Option<Multiplexador>) -> String {
    match programa {
        None => nenhuma(),
        Some(Multiplexador::Tmux) => tmux(),
        Some(Multiplexador::Screen) => screen(),
    }
}

/// O texto da opção "Herdar" do item `no`: diz de onde vem e o que vale.
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    match herdada(tree, no) {
        Some((nome, programa)) => t!(
            "tmux-herdar-de",
            nome = nome,
            valor = rotulo_do_programa(programa)
        ),
        None => t!("tmux-herdar-nenhuma"),
    }
}

/// O texto da linha para `persistente` no item `no`.
pub fn rotulo(tree: &Tree, no: u64, persistente: SessaoPersistente) -> String {
    match decide(persistente) {
        None => rotulo_de_herdar(tree, no),
        Some(programa) => rotulo_do_programa(programa),
    }
}

/// As opções da linha do item `no`.
pub fn opcoes(tree: &Tree, no: u64) -> Vec<String> {
    vec![rotulo_de_herdar(tree, no), nenhuma(), tmux(), screen()]
}

/// A escolha que o texto da linha faz; um texto que não é de nenhuma opção é "Herdar".
pub fn do_rotulo(texto: &str) -> SessaoPersistente {
    if texto == nenhuma() {
        SessaoPersistente::Nenhuma
    } else if texto == tmux() {
        SessaoPersistente::Tmux
    } else if texto == screen() {
        SessaoPersistente::Screen
    } else {
        SessaoPersistente::Herdar
    }
}

/// Se `nome` serve de nome de sessão: só letras e números ASCII, `_` e `-` (o que o `tmux` e o
/// `screen` aceitam sem aspas e sem confundir com `sessão:janela.painel` ou `pid.nome`).
pub fn nome_valido(nome: &str) -> bool {
    !nome.is_empty()
        && nome.len() <= 64
        && nome
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// O primeiro nome `tirech-<conexão>-<n>` (n a partir de 1) que `em_uso` não tem.
pub fn nome_livre(conexao: u64, em_uso: impl Fn(&str) -> bool) -> String {
    (1u64..)
        .map(|n| format!("{PREFIXO}{conexao}-{n}"))
        .find(|nome| !em_uso(nome))
        .unwrap_or_default()
}

fn citar(texto: &str) -> String {
    shell_quote(texto, ShellQuote::Posix)
}

/// Um script do `sh` como um comando só: `sh -c '…'`.
fn pelo_sh(script: &str) -> String {
    format!("sh -c {}", citar(script))
}

/// O trecho que escreve, uma por linha, as sessões do `screen` (`pid.nome`) cujo nome é `alvo`
/// ou cujo `pid.nome` inteiro é `alvo`. As mortas (`Dead ???`) ficam de fora.
fn alvos_do_screen(alvo: &str) -> String {
    format!(
        "screen -ls 2>/dev/null | awk -v n={} '$1 ~ /^[0-9]+\\./ && !/Dead/ && \
         ($1 == n || substr($1, index($1, \".\") + 1) == n) {{ print $1 }}'",
        citar(alvo)
    )
}

/// O aviso que a aba escreve quando o servidor não tem o `programa`.
pub fn aviso_sem(programa: Multiplexador) -> String {
    t!("tmux-sem-programa", programa = programa.programa())
}

/// O comando que a aba roda no lugar do shell: volta à `sessao` (ou a cria). No tmux, liga o
/// mouse só nela (a roda rola o histórico do tmux). Sem o programa no servidor, escreve `aviso`
/// e abre o shell de login. Roda pelo `sh`, para valer igual com o fish e outros shells de login.
pub fn comando(sessao: &SessaoDaAba, aviso: &str) -> String {
    let nome = citar(&sessao.nome);
    let ligar = match sessao.programa {
        Multiplexador::Tmux => format!(
            "if command -v tmux >/dev/null 2>&1; then \
             exec tmux new-session -A -s {nome} \\; set-option -q mouse on; fi; "
        ),
        Multiplexador::Screen => format!(
            "if command -v screen >/dev/null 2>&1; then \
             alvo=$({} | head -n 1); \
             if [ -n \"$alvo\" ]; then exec screen -x \"$alvo\"; fi; \
             exec screen -h 10000 -S {nome}; fi; ",
            alvos_do_screen(&sessao.nome)
        ),
    };
    let script = format!(
        "{ligar}printf '%s\\n' {}; exec \"${{SHELL:-/bin/sh}}\" -l",
        citar(aviso)
    );
    format!("exec {}", pelo_sh(&script))
}

/// O comando da aba de `sessao`, com o aviso no idioma do app.
pub fn comando_da_aba(sessao: &SessaoDaAba) -> String {
    comando(sessao, &aviso_sem(sessao.programa))
}

/// O script do `sh` que encerra a sessão `alvo` do `programa` no servidor: o nome exato (no
/// tmux, `=nome`, para não pegar `tirech-1-10` pelo começo `tirech-1-1`) ou, no screen, também
/// o `pid.nome` que a lista mostra.
pub fn script_encerrar(programa: Multiplexador, alvo: &str) -> String {
    match programa {
        Multiplexador::Tmux => format!("tmux kill-session -t {}", citar(&format!("={alvo}"))),
        Multiplexador::Screen => format!(
            "for alvo in $({}); do screen -S \"$alvo\" -X quit; done",
            alvos_do_screen(alvo)
        ),
    }
}

/// O comando que encerra a sessão da aba no servidor.
pub fn comando_encerrar(sessao: &SessaoDaAba) -> String {
    match sessao.programa {
        Multiplexador::Tmux => script_encerrar(Multiplexador::Tmux, &sessao.nome),
        Multiplexador::Screen => pelo_sh(&script_encerrar(Multiplexador::Screen, &sessao.nome)),
    }
}

/// O comando que diz a pasta em que a sessão está. No tmux, a do painel ativo; no screen, que
/// não diz, a do shell da sessão pelo `/proc` (só no Linux, e só com uma janela: com mais, não
/// há como saber qual está à mostra, e vale o OSC 7).
pub fn comando_pasta(sessao: &SessaoDaAba) -> String {
    match sessao.programa {
        Multiplexador::Tmux => format!(
            "tmux display-message -p -t {} '#{{pane_current_path}}'",
            citar(&format!("={}:", sessao.nome))
        ),
        Multiplexador::Screen => pelo_sh(&format!(
            "alvo=$({} | head -n 1); [ -n \"$alvo\" ] || exit 1; pid=${{alvo%%.*}}; \
             filhos=$(pgrep -P \"$pid\" 2>/dev/null || ps -o pid= --ppid \"$pid\" 2>/dev/null); \
             set -- $filhos; [ \"$#\" -eq 1 ] || exit 1; readlink \"/proc/$1/cwd\"",
            alvos_do_screen(&sessao.nome)
        )),
    }
}

/// O script (pelo `sh -s`) que lista o que o servidor tem: uma linha `tirech-tem:PROGRAMA` para
/// cada programa instalado e uma `tirech-sessao:PROGRAMA:LIGADA:ALVO` para cada sessão, com
/// `LIGADA` o número de clientes ligados (no screen, 1 ou 0) e `ALVO` o nome (no screen,
/// `pid.nome`). O alvo fica no fim porque é o único campo que pode ter `:`.
pub const SCRIPT_LISTAR: &str = r#"for p in tmux screen; do
  command -v "$p" >/dev/null 2>&1 && echo "tirech-tem:$p"
done
if command -v tmux >/dev/null 2>&1; then
  tmux list-sessions -F 'tirech-sessao:tmux:#{session_attached}:#{session_name}' 2>/dev/null
fi
if command -v screen >/dev/null 2>&1; then
  screen -ls 2>/dev/null | awk '$1 ~ /^[0-9]+\./ && !/Dead/ { a = ($0 ~ /[Aa]ttached\)/) ? 1 : 0; print "tirech-sessao:screen:" a ":" $1 }'
fi
exit 0
"#;

/// Uma sessão que está no servidor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessaoNoServidor {
    pub programa: Multiplexador,
    /// O nome que o programa entende: no tmux, o nome; no screen, `pid.nome`.
    pub alvo: String,
    /// O nome da sessão, sem o `pid.` do screen.
    pub nome: String,
    /// Quantos clientes estão ligados a ela (no screen, 1 se algum).
    pub clientes: u32,
}

impl SessaoNoServidor {
    /// A sessão como uma aba a liga: o programa e o nome. `None` se o nome não serve para o app
    /// abrir (ver [`nome_valido`]).
    pub fn da_aba(&self) -> Option<SessaoDaAba> {
        nome_valido(&self.nome).then(|| SessaoDaAba::new(self.programa, self.nome.clone()))
    }
}

/// O que a lista achou no servidor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NoServidor {
    /// Os programas instalados.
    pub programas: Vec<Multiplexador>,
    /// As sessões, primeiro as do tmux, cada programa por nome.
    pub sessoes: Vec<SessaoNoServidor>,
}

fn programa_do_texto(texto: &str) -> Option<Multiplexador> {
    match texto {
        "tmux" => Some(Multiplexador::Tmux),
        "screen" => Some(Multiplexador::Screen),
        _ => None,
    }
}

/// Lê a saída do [`SCRIPT_LISTAR`]. Linhas que não são dele (um `.bashrc` que escreve) ficam
/// de fora.
pub fn ler_lista(saida: &str) -> NoServidor {
    let mut lido = NoServidor::default();
    for linha in saida.lines().map(|l| l.trim_end_matches('\r')) {
        if let Some(programa) = linha
            .strip_prefix("tirech-tem:")
            .and_then(programa_do_texto)
        {
            if !lido.programas.contains(&programa) {
                lido.programas.push(programa);
            }
            continue;
        }
        let Some(resto) = linha.strip_prefix("tirech-sessao:") else {
            continue;
        };
        let mut partes = resto.splitn(3, ':');
        let (Some(programa), Some(clientes), Some(alvo)) = (
            partes.next().and_then(programa_do_texto),
            partes.next().and_then(|n| n.trim().parse::<u32>().ok()),
            partes.next().filter(|a| !a.is_empty()),
        ) else {
            continue;
        };
        let nome = match programa {
            Multiplexador::Tmux => alvo,
            Multiplexador::Screen => alvo.split_once('.').map_or(alvo, |(_, nome)| nome),
        };
        lido.sessoes.push(SessaoNoServidor {
            programa,
            alvo: alvo.to_owned(),
            nome: nome.to_owned(),
            clientes,
        });
    }
    lido.sessoes.sort_by(|a, b| {
        (a.programa != Multiplexador::Tmux, &a.nome, &a.alvo).cmp(&(
            b.programa != Multiplexador::Tmux,
            &b.nome,
            &b.alvo,
        ))
    });
    lido
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::modelo::Connection;

    fn arvore() -> (Tree, u64, u64, u64) {
        let mut tree = Tree::default();
        let pasta = tree.add_group(None, "Produção");
        let dentro = tree.add_group(Some(pasta), "Web");
        let mut c = Connection::new("web", Kind::Ssh);
        c.host = "web".into();
        let id = tree.add_connection(Some(dentro), c);
        (tree, pasta, dentro, id)
    }

    fn mudar_grupo(tree: &mut Tree, id: u64, p: SessaoPersistente) {
        if let Some(Node::Group(g)) = tree.find_mut(id) {
            g.persistente = p;
        }
    }

    fn mudar_conexao(tree: &mut Tree, id: u64, f: impl FnOnce(&mut Connection)) {
        if let Some(Node::Connection(c)) = tree.find_mut(id) {
            f(c);
        }
    }

    #[test]
    fn herda_da_pasta_mais_perto_que_decide() {
        let (mut tree, pasta, dentro, id) = arvore();
        assert!(!usa_tmux(&tree, id), "desligado por padrão");
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Nenhuma)");

        mudar_grupo(&mut tree, pasta, SessaoPersistente::Tmux);
        assert!(usa_tmux(&tree, id));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Produção: tmux)");

        mudar_grupo(&mut tree, dentro, SessaoPersistente::Nenhuma);
        assert!(!usa_tmux(&tree, id));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Web: Nenhuma)");

        mudar_conexao(&mut tree, id, |c| c.persistente = SessaoPersistente::Tmux);
        assert!(usa_tmux(&tree, id));
        assert_eq!(
            rotulo(&tree, id, SessaoPersistente::Tmux),
            "tmux",
            "a escolha da conexão manda"
        );
    }

    #[test]
    fn so_ssh_com_shell_e_fora_do_windows() {
        let (mut tree, _, _, id) = arvore();
        mudar_conexao(&mut tree, id, |c| c.persistente = SessaoPersistente::Tmux);
        assert!(usa_tmux(&tree, id));
        mudar_conexao(&mut tree, id, |c| c.so_tuneis = true);
        assert!(!usa_tmux(&tree, id));
        mudar_conexao(&mut tree, id, |c| {
            c.so_tuneis = false;
            c.shell_remoto = ShellRemoto::PowerShell;
        });
        assert!(!usa_tmux(&tree, id));
        mudar_conexao(&mut tree, id, |c| {
            c.shell_remoto = ShellRemoto::Automatico;
            c.kind = Kind::Telnet;
        });
        assert!(!usa_tmux(&tree, id));
        assert!(!usa_tmux(&tree, 999));
    }

    #[test]
    fn os_rotulos_voltam_a_escolha() {
        let (tree, _, _, id) = arvore();
        let opcoes = opcoes(&tree, id);
        assert_eq!(opcoes.len(), 4);
        assert_eq!(do_rotulo(&opcoes[0]), SessaoPersistente::Herdar);
        assert_eq!(do_rotulo(&opcoes[1]), SessaoPersistente::Nenhuma);
        assert_eq!(do_rotulo(&opcoes[2]), SessaoPersistente::Tmux);
        assert_eq!(do_rotulo(&opcoes[3]), SessaoPersistente::Screen);
        assert_eq!(do_rotulo("qualquer"), SessaoPersistente::Herdar);
    }

    #[test]
    fn o_screen_herda_como_o_tmux() {
        let (mut tree, pasta, _, id) = arvore();
        mudar_grupo(&mut tree, pasta, SessaoPersistente::Screen);
        assert_eq!(programa_de(&tree, id), Some(Multiplexador::Screen));
        assert_eq!(rotulo_de_herdar(&tree, id), "Herdar (Produção: screen)");
        mudar_conexao(&mut tree, id, |c| c.persistente = SessaoPersistente::Tmux);
        assert_eq!(programa_de(&tree, id), Some(Multiplexador::Tmux));
    }

    #[test]
    fn o_nome_livre_pula_os_que_estao_em_uso() {
        let usados = ["tirech-7-1", "tirech-7-2", "tirech-7-4"];
        assert_eq!(nome_livre(7, |n| usados.contains(&n)), "tirech-7-3");
        assert_eq!(nome_livre(8, |n| usados.contains(&n)), "tirech-8-1");
        assert!(nome_valido("tirech-7-3"));
        assert!(nome_valido("Meu_nome-2"));
        assert!(!nome_valido(""));
        assert!(!nome_valido("a b"));
        assert!(!nome_valido("a:1"));
        assert!(!nome_valido("a;rm"));
        assert!(!nome_valido(&"a".repeat(65)));
    }

    #[test]
    fn os_comandos_citam_o_nome_e_o_aviso() {
        let tmux = SessaoDaAba::new(Multiplexador::Tmux, "tirech-1-2");
        assert_eq!(comando_encerrar(&tmux), "tmux kill-session -t =tirech-1-2");
        assert_eq!(
            comando_pasta(&tmux),
            "tmux display-message -p -t =tirech-1-2: '#{pane_current_path}'"
        );
        let c = comando(&tmux, "sem tmux: o shell comum");
        assert!(c.starts_with("exec sh -c '"), "{c}");
        assert!(c.contains("new-session -A -s tirech-1-2"), "{c}");
        assert!(c.contains("set-option -q mouse on"), "{c}");

        let screen = SessaoDaAba::new(Multiplexador::Screen, "tirech-1-2");
        let c = comando(&screen, "sem screen");
        assert!(c.starts_with("exec sh -c '"), "{c}");
        assert!(c.contains("screen -x"), "{c}");
        assert!(c.contains("screen -h 10000 -S tirech-1-2"), "{c}");
        assert!(comando_encerrar(&screen).contains("-X quit"));
        assert!(comando_pasta(&screen).contains("/cwd"));
    }

    #[test]
    fn a_lista_le_as_sessoes_dos_dois_programas() {
        let saida = "olá do bashrc\r\n\
            tirech-tem:tmux\n\
            tirech-tem:screen\n\
            tirech-sessao:tmux:1:tirech-3-1\n\
            tirech-sessao:tmux:0:obra:1\n\
            tirech-sessao:screen:0:4242.tirech-3-2\r\n\
            tirech-sessao:screen:1:77.build\n\
            tirech-sessao:nada:1:x\n\
            tirech-sessao:tmux:x:y\n";
        let lido = ler_lista(saida);
        assert_eq!(
            lido.programas,
            vec![Multiplexador::Tmux, Multiplexador::Screen]
        );
        let resumo: Vec<_> = lido
            .sessoes
            .iter()
            .map(|s| (s.programa, s.alvo.as_str(), s.nome.as_str(), s.clientes))
            .collect();
        assert_eq!(
            resumo,
            vec![
                (Multiplexador::Tmux, "obra:1", "obra:1", 0),
                (Multiplexador::Tmux, "tirech-3-1", "tirech-3-1", 1),
                (Multiplexador::Screen, "77.build", "build", 1),
                (Multiplexador::Screen, "4242.tirech-3-2", "tirech-3-2", 0),
            ]
        );
        assert_eq!(lido.sessoes[0].da_aba(), None, "o `:` não serve de nome");
        assert_eq!(
            lido.sessoes[3].da_aba(),
            Some(SessaoDaAba::new(Multiplexador::Screen, "tirech-3-2"))
        );
        assert_eq!(ler_lista(""), NoServidor::default());
    }

    /// O script da lista num `sh` sem os programas: não acha nada e sai com 0.
    #[cfg(unix)]
    #[test]
    fn a_lista_sem_programas_nao_acha_nada() {
        let saida = std::process::Command::new("/bin/sh")
            .args(["-c", SCRIPT_LISTAR])
            .env("PATH", "/nao-existe")
            .output()
            .unwrap();
        assert!(saida.status.success());
        assert_eq!(
            ler_lista(&String::from_utf8_lossy(&saida.stdout)),
            NoServidor::default()
        );
    }

    /// Os scripts do screen num `sh`, com um `screen` de mentira no `PATH` que responde ao
    /// `-ls` como o de verdade e anota os outros comandos.
    #[cfg(unix)]
    #[test]
    fn os_scripts_do_screen_leem_o_screen_ls() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = std::env::temp_dir().join(format!("tirech-screen-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let anotado = pasta.join("anotado");
        let falso = pasta.join("screen");
        std::fs::write(
            &falso,
            format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = -ls ]; then\n\
                 printf 'There are screens on:\\n'\n\
                 printf '\\t4242.tirech-1-1\\t(24/10/2026 10:00:00)\\t(Attached)\\n'\n\
                 printf '\\t4343.obras\\t(24/10/2026 09:00:00)\\t(Detached)\\n'\n\
                 printf '\\t4444.velha\\t(Dead ???)\\n'\n\
                 printf '3 Sockets in /run/screen/S-ana.\\n'\n\
                 exit 1\n\
                 fi\n\
                 echo \"$*\" >> {}\n",
                citar(&anotado.to_string_lossy())
            ),
        )
        .unwrap();
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:/usr/bin:/bin", pasta.display());
        let rodar = |script: &str| {
            let saida = std::process::Command::new("/bin/sh")
                .args(["-c", script])
                .env("PATH", &path)
                .output()
                .unwrap();
            String::from_utf8_lossy(&saida.stdout).into_owned()
        };
        let anotados = || {
            let texto = std::fs::read_to_string(&anotado).unwrap_or_default();
            let _ = std::fs::remove_file(&anotado);
            texto
        };

        let lido = ler_lista(&rodar(SCRIPT_LISTAR));
        assert!(lido.programas.contains(&Multiplexador::Screen), "{lido:?}");
        let do_screen: Vec<_> = lido
            .sessoes
            .iter()
            .filter(|s| s.programa == Multiplexador::Screen)
            .map(|s| (s.alvo.as_str(), s.nome.as_str(), s.clientes))
            .collect();
        assert_eq!(
            do_screen,
            [
                ("4343.obras", "obras", 0),
                ("4242.tirech-1-1", "tirech-1-1", 1)
            ]
        );

        // A aba entra na sessão que existe e cria a que não existe.
        rodar(&comando(
            &SessaoDaAba::new(Multiplexador::Screen, "tirech-1-1"),
            "aviso",
        ));
        assert_eq!(anotados(), "-x 4242.tirech-1-1\n");
        rodar(&comando(
            &SessaoDaAba::new(Multiplexador::Screen, "tirech-1-9"),
            "aviso",
        ));
        assert_eq!(anotados(), "-h 10000 -S tirech-1-9\n");

        // Encerrar acha a sessão pelo nome ou pelo `pid.nome`; a morta não casa.
        rodar(&script_encerrar(Multiplexador::Screen, "tirech-1-1"));
        assert_eq!(anotados(), "-S 4242.tirech-1-1 -X quit\n");
        rodar(&comando_encerrar(&SessaoDaAba::new(
            Multiplexador::Screen,
            "obras",
        )));
        assert_eq!(anotados(), "-S 4343.obras -X quit\n");
        rodar(&script_encerrar(Multiplexador::Screen, "4343.obras"));
        assert_eq!(anotados(), "-S 4343.obras -X quit\n");
        rodar(&script_encerrar(Multiplexador::Screen, "velha"));
        assert_eq!(anotados(), "");
        let _ = std::fs::remove_dir_all(&pasta);
    }

    /// O comando de verdade num `sh`: sem o programa no `PATH`, escreve o aviso e abre o shell.
    #[cfg(unix)]
    #[test]
    fn sem_o_programa_o_comando_avisa_e_abre_o_shell() {
        for programa in [Multiplexador::Tmux, Multiplexador::Screen] {
            let tem = ["/usr/bin", "/bin"].iter().any(|pasta| {
                std::path::Path::new(pasta)
                    .join(programa.programa())
                    .exists()
            });
            if tem {
                continue;
            }
            let c = comando(
                &SessaoDaAba::new(programa, "tirech-1-1"),
                "não há o programa: it's ok",
            );
            let saida = std::process::Command::new("sh")
                .args(["-c", &c])
                .env("PATH", "/usr/bin:/bin")
                .env("SHELL", "/bin/echo")
                .env_remove("TMUX")
                .output()
                .unwrap();
            let texto = String::from_utf8_lossy(&saida.stdout);
            // O `-l` do shell de login chega como argumento do `echo`.
            assert_eq!(texto, "não há o programa: it's ok\n-l\n", "{programa:?}");
        }
    }
}
