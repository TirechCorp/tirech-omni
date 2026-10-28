//! Os textos de "Como ligar um assistente" (M51): o comando e a configuração que a pessoa põe
//! no Claude Code e no Codex para eles abrirem a ponte `tirech-omni mcp`. São funções puras: o
//! caminho do executável entra, o texto sai.
//!
//! Sintaxes conferidas em 21/10/2026 com o Claude Code 2.1.308 (`claude mcp add [options]
//! <name> <commandOrUrl> [args...]`, escopos `local`, `user` e `project`, `.mcp.json` com
//! `mcpServers`) e o Codex 0.160.0 (`codex mcp add <NAME> -- <COMMAND>...`, `[mcp_servers.<nome>]`
//! no `~/.codex/config.toml`). As ferramentas locais que o Claude Code nega são as de
//! `permissions.deny` do `.claude/settings.json` (`Bash`, `Read`, `Edit`, `Write`, `Glob`,
//! `Grep`, `NotebookEdit`); as que o Codex desliga são `shell_tool` e `unified_exec` em
//! `[features]` e o `sandbox_mode = "read-only"`.

use std::path::Path;

use serde_json::json;

/// O nome do servidor MCP, sem espaço de trabalho.
pub const NOME: &str = "tirech-omni";

/// O nome do servidor que o "Controlar com" (M63) põe na linha de comando do Claude Code. Fica
/// diferente de [`NOME`] porque a documentação do Claude Code não diz se o `--mcp-config` vence
/// um servidor de mesmo nome do escopo da pessoa; com dois nomes, ela pode ter os dois ligados
/// sem um trocar o outro.
pub const NOME_DA_ABA: &str = "tirech-omni-aba";

/// Onde o Claude Code guarda a configuração do servidor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escopo {
    /// Só para esta pasta e só para você (o padrão do `claude mcp add`).
    Local,
    /// Em todas as pastas (`--scope user`).
    Usuario,
    /// No `.mcp.json` da pasta, para quem mais usar o projeto (`--scope project`).
    Projeto,
}

impl Escopo {
    fn opcao(self) -> Option<&'static str> {
        match self {
            Self::Local => None,
            Self::Usuario => Some("--scope user"),
            Self::Projeto => Some("--scope project"),
        }
    }
}

/// O nome do servidor para um espaço de trabalho: `tirech-omni-<nome>`, com o nome reduzido a
/// letras minúsculas, números e `-` (cabe como chave de TOML sem aspas).
pub fn nome_do_servidor(espaco: Option<&str>) -> String {
    let Some(espaco) = espaco else {
        return NOME.to_string();
    };
    let mut nome = String::new();
    for c in espaco.chars().flat_map(char::to_lowercase) {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if c != '-' || !nome.ends_with('-') {
            nome.push(c);
        }
    }
    let nome = nome.trim_matches('-');
    if nome.is_empty() {
        NOME.to_string()
    } else {
        format!("{NOME}-{nome}")
    }
}

/// Põe aspas duplas num argumento de linha de comando que tem espaço ou algo que o shell
/// interpretaria.
fn aspas(texto: &str) -> String {
    let precisa = texto.is_empty()
        || texto
            .chars()
            .any(|c| c.is_whitespace() || "\"'$&;()<>|*?[]{}`!#~".contains(c));
    if precisa {
        format!("\"{}\"", texto.replace('"', "\\\""))
    } else {
        texto.to_string()
    }
}

/// Os argumentos da ponte: `mcp` e, num espaço de trabalho, `--espaco <nome>`.
fn args_da_ponte(espaco: Option<&str>) -> Vec<String> {
    let mut args = vec!["mcp".to_string()];
    if let Some(espaco) = espaco {
        args.push("--espaco".into());
        args.push(espaco.into());
    }
    args
}

/// O comando que a pessoa roda no terminal para o Claude Code.
///
/// ```
/// use tirech_omni::mcp::clientes::{comando_claude, Escopo};
///
/// assert_eq!(
///     comando_claude("/usr/bin/tirech-omni", Escopo::Usuario, None),
///     "claude mcp add --scope user tirech-omni -- /usr/bin/tirech-omni mcp"
/// );
/// ```
pub fn comando_claude(exe: &str, escopo: Escopo, espaco: Option<&str>) -> String {
    let mut partes = vec!["claude mcp add".to_string()];
    partes.extend(escopo.opcao().map(str::to_string));
    partes.push(nome_do_servidor(espaco));
    partes.push("--".into());
    partes.push(aspas(exe));
    partes.extend(args_da_ponte(espaco).iter().map(|a| aspas(a)));
    partes.join(" ")
}

/// O `.mcp.json` do Claude Code, com as barras do Windows escapadas.
pub fn json_claude(exe: &str, espaco: Option<&str>) -> String {
    let servidor = json!({
        "mcpServers": {
            nome_do_servidor(espaco): {
                "command": exe,
                "args": args_da_ponte(espaco),
            }
        }
    });
    serde_json::to_string_pretty(&servidor).unwrap_or_default()
}

/// O comando que a pessoa roda no terminal para o Codex.
pub fn comando_codex(exe: &str, espaco: Option<&str>) -> String {
    let mut partes = vec![
        "codex mcp add".to_string(),
        nome_do_servidor(espaco),
        "--".into(),
        aspas(exe),
    ];
    partes.extend(args_da_ponte(espaco).iter().map(|a| aspas(a)));
    partes.join(" ")
}

/// Uma string básica do TOML.
fn toml_texto(texto: &str) -> String {
    let mut s = String::from("\"");
    for c in texto.chars() {
        match c {
            '\\' => s.push_str("\\\\"),
            '"' => s.push_str("\\\""),
            '\n' => s.push_str("\\n"),
            '\t' => s.push_str("\\t"),
            c => s.push(c),
        }
    }
    s.push('"');
    s
}

/// O trecho do `~/.codex/config.toml`, com o prazo das ferramentas em segundos
/// ([`PRAZO_DAS_FERRAMENTAS`]): o padrão do Codex corta um `executar` longo.
pub fn toml_codex(exe: &str, espaco: Option<&str>) -> String {
    let args: Vec<String> = args_da_ponte(espaco)
        .iter()
        .map(|a| toml_texto(a))
        .collect();
    format!(
        "[mcp_servers.{}]\ncommand = {}\nargs = [{}]\ntool_timeout_sec = {PRAZO_DAS_FERRAMENTAS}\n",
        nome_do_servidor(espaco),
        toml_texto(exe),
        args.join(", ")
    )
}

/// O `opencode.json` do OpenCode (o do projeto ou o `~/.config/opencode/opencode.json`): o
/// servidor local com o comando inteiro numa lista e o `timeout` das ferramentas em
/// milissegundos ([`PRAZO_DAS_FERRAMENTAS`]), porque o prazo padrão corta um `executar` longo.
pub fn json_opencode(exe: &str, espaco: Option<&str>) -> String {
    let mut comando = vec![exe.to_owned()];
    comando.extend(args_da_ponte(espaco));
    let config = json!({
        "$schema": "https://opencode.ai/config.json",
        "mcp": {
            nome_do_servidor(espaco): {
                "type": "local",
                "command": comando,
                "enabled": true,
                "timeout": PRAZO_DAS_FERRAMENTAS * 1000,
            }
        }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

/// Os argumentos da ponte presa a uma aba: `mcp --nome <nome> --aba <chave>` (M63).
pub fn args_da_ponte_na_aba(nome: &str, aba: &str) -> Vec<String> {
    ["mcp", "--nome", nome, "--aba", aba]
        .map(str::to_owned)
        .to_vec()
}

/// O JSON que o `claude --mcp-config <arquivo>` lê: o servidor [`NOME_DA_ABA`] com a `ponte`
/// (os argumentos de [`args_da_ponte_na_aba`]).
pub fn json_claude_na_aba(exe: &str, ponte: &[String]) -> String {
    let servidor = json!({
        "mcpServers": { NOME_DA_ABA: { "command": exe, "args": ponte } }
    });
    serde_json::to_string_pretty(&servidor).unwrap_or_default()
}

/// Os settings do `claude --settings <arquivo>` com o hook dos avisos: a cada mensagem da
/// pessoa (`UserPromptSubmit`), `tirech-omni mcp --avisos <lancamento>` entrega o que a sessão
/// ainda não sabe (arquivos soltos, abas dadas), sem esperar a próxima ferramenta. O Claude Code
/// soma estes settings aos da pessoa. O comando roda pelo shell do Claude Code: o `sh` no Linux e
/// no macOS (aspas simples), o Git Bash ou o `cmd` no Windows (aspas duplas servem aos dois).
pub fn settings_claude_avisos(exe: &str, lancamento: &str, windows: bool) -> String {
    let exe = if windows {
        format!("\"{exe}\"")
    } else {
        crate::ia::aspas_sh(exe)
    };
    let settings = json!({
        "hooks": { "UserPromptSubmit": [ { "hooks": [ {
            "type": "command",
            "command": format!("{exe} mcp --avisos {}", crate::ia::aspas_sh(lancamento)),
        } ] } ] }
    });
    serde_json::to_string_pretty(&settings).unwrap_or_default()
}

/// Os argumentos do `claude` que ligam o app presos a uma aba (M63): o arquivo do
/// [`json_claude_na_aba`] e o dos [`settings_claude_avisos`], gravados pelo chamador, o nome da
/// sessão do Claude Code e as `instrucoes`, somadas ao prompt de sistema, que dizem que a aba é o
/// alvo e o Bash dele é este computador. Os JSON vão por arquivo e não na linha de comando porque
/// no Windows as aspas deles se perdem; as instruções não levam aspas.
pub fn lancar_claude(arquivo: &Path, avisos: &Path, nome: &str, instrucoes: &str) -> Vec<String> {
    vec![
        "--mcp-config".into(),
        arquivo.to_string_lossy().into_owned(),
        "--settings".into(),
        avisos.to_string_lossy().into_owned(),
        "-n".into(),
        nome.into(),
        "--append-system-prompt".into(),
        instrucoes.into(),
    ]
}

/// Quanto o assistente espera uma ferramenta do app, em segundos: 24 h. O `executar` espera o
/// comando acabar, e um comando longo (um backup, uma compilação) passaria do prazo padrão do
/// Codex (60 s) e o do OpenCode (o do SDK do MCP), que dariam a ferramenta por perdida.
pub const PRAZO_DAS_FERRAMENTAS: u64 = 24 * 60 * 60;

/// Os argumentos do `codex` que ligam o app presos a uma aba (M63): três `-c` que sobrepõem
/// `mcp_servers.tirech-omni` só nesta execução, sem tocar no `~/.codex/config.toml`: o
/// comando, os argumentos e o prazo das ferramentas ([`PRAZO_DAS_FERRAMENTAS`]).
pub fn lancar_codex(exe: &str, ponte: &[String]) -> Vec<String> {
    let args: Vec<String> = ponte.iter().map(|a| toml_texto(a)).collect();
    vec![
        "-c".into(),
        format!("mcp_servers.{NOME}.command={}", toml_texto(exe)),
        "-c".into(),
        format!("mcp_servers.{NOME}.args=[{}]", args.join(", ")),
        "-c".into(),
        format!("mcp_servers.{NOME}.tool_timeout_sec={PRAZO_DAS_FERRAMENTAS}"),
    ]
}

/// Os argumentos do `claude` para a conversa `id`: `--resume` continua uma que já existe,
/// `--session-id` começa uma nova com esse id (um UUID), para o diário saber qual é.
pub fn conversa_claude(id: &str, continuar: bool) -> Vec<String> {
    let opcao = if continuar {
        "--resume"
    } else {
        "--session-id"
    };
    vec![opcao.into(), id.into()]
}

/// O `-c notify=…` do `codex` que avisa o id da conversa nova: o Codex chama
/// `<exe> mcp --conversa <arquivo>` depois de cada resposta, com o JSON no fim, e a ponte chama
/// o `notify` que a pessoa tinha (`original`), que este `-c` sobrepõe.
pub fn notify_codex(exe: &str, arquivo: &Path, original: &[String]) -> Vec<String> {
    let mut comando = vec![
        exe.to_owned(),
        "mcp".into(),
        "--conversa".into(),
        arquivo.to_string_lossy().into_owned(),
    ];
    if !original.is_empty() {
        comando.push("--".into());
        comando.extend(original.iter().cloned());
    }
    let itens: Vec<String> = comando.iter().map(|a| toml_texto(a)).collect();
    vec!["-c".into(), format!("notify=[{}]", itens.join(", "))]
}

/// O que o `opencode` precisa para ligar o app presos a uma aba (M63): sem argumentos, e a
/// variável `OPENCODE_CONFIG_CONTENT` com o JSON do servidor local, que vence a configuração
/// da pessoa e não grava nada. Formato da documentação do OpenCode (`mcp`, `type: "local"`,
/// `command` como lista, `enabled`, e `timeout` em milissegundos, o prazo das ferramentas
/// deste servidor: [`PRAZO_DAS_FERRAMENTAS`]).
pub fn lancar_opencode(exe: &str, ponte: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    let mut comando = vec![exe.to_owned()];
    comando.extend(ponte.iter().cloned());
    let config = json!({
        "mcp": { NOME: {
            "type": "local",
            "command": comando,
            "enabled": true,
            "timeout": PRAZO_DAS_FERRAMENTAS * 1000,
        } }
    });
    (
        Vec::new(),
        vec![("OPENCODE_CONFIG_CONTENT".into(), config.to_string())],
    )
}

/// As ferramentas locais do Claude Code que o `permissions.deny` nega, para o assistente usar só
/// a pasta do servidor.
const FERRAMENTAS_LOCAIS_DO_CLAUDE: [&str; 7] = [
    "Bash",
    "Read",
    "Edit",
    "Write",
    "Glob",
    "Grep",
    "NotebookEdit",
];

/// O `.claude/settings.json` que nega as ferramentas locais do Claude Code.
pub fn deny_claude() -> String {
    let deny = json!({ "permissions": { "deny": FERRAMENTAS_LOCAIS_DO_CLAUDE } });
    serde_json::to_string_pretty(&deny).unwrap_or_default()
}

/// O trecho do `~/.codex/config.toml` que desliga o shell e a escrita locais do Codex.
pub fn config_codex_so_servidor() -> String {
    "sandbox_mode = \"read-only\"\n\n[features]\nshell_tool = false\nunified_exec = false\n"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comandos_simples() {
        assert_eq!(
            comando_claude("/usr/bin/tirech-omni", Escopo::Local, None),
            "claude mcp add tirech-omni -- /usr/bin/tirech-omni mcp"
        );
        assert_eq!(
            comando_claude("/usr/bin/tirech-omni", Escopo::Usuario, None),
            "claude mcp add --scope user tirech-omni -- /usr/bin/tirech-omni mcp"
        );
        assert_eq!(
            comando_claude("/usr/bin/tirech-omni", Escopo::Projeto, None),
            "claude mcp add --scope project tirech-omni -- /usr/bin/tirech-omni mcp"
        );
        assert_eq!(
            comando_codex("/usr/bin/tirech-omni", None),
            "codex mcp add tirech-omni -- /usr/bin/tirech-omni mcp"
        );
    }

    #[test]
    fn comando_claude_com_espaco_no_caminho() {
        assert_eq!(
            comando_claude(
                r"C:\Program Files\Tirech Omni\tirech-omni.exe",
                Escopo::Local,
                None
            ),
            r#"claude mcp add tirech-omni -- "C:\Program Files\Tirech Omni\tirech-omni.exe" mcp"#
        );
        assert_eq!(
            comando_codex("/home/ana/Meus Apps/tirech-omni", None),
            r#"codex mcp add tirech-omni -- "/home/ana/Meus Apps/tirech-omni" mcp"#
        );
    }

    #[test]
    fn json_claude_windows_escapa_barras() {
        let json = json_claude(r"C:\Program Files\Tirech\tirech-omni.exe", None);
        assert!(
            json.contains(r"C:\\Program Files\\Tirech\\tirech-omni.exe"),
            "{json}"
        );
        // E o que o Claude Code lê de volta é o caminho de verdade.
        let lido: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            lido["mcpServers"]["tirech-omni"]["command"],
            r"C:\Program Files\Tirech\tirech-omni.exe"
        );
        assert_eq!(lido["mcpServers"]["tirech-omni"]["args"], json!(["mcp"]));
    }

    #[test]
    fn toml_codex_com_aspas() {
        let toml = toml_codex(r#"C:\Users\Ana\tirech "omni".exe"#, None);
        assert_eq!(
            toml,
            "[mcp_servers.tirech-omni]\ncommand = \"C:\\\\Users\\\\Ana\\\\tirech \\\"omni\\\".exe\"\nargs = [\"mcp\"]\ntool_timeout_sec = 86400\n"
        );
    }

    #[test]
    fn json_opencode_com_o_prazo_e_o_espaco() {
        let json = json_opencode(r"C:\Tirech\tirech-omni.exe", Some("web"));
        let lido: serde_json::Value = serde_json::from_str(&json).unwrap();
        let servidor = &lido["mcp"]["tirech-omni-web"];
        assert_eq!(servidor["type"], "local");
        assert_eq!(servidor["enabled"], true);
        assert_eq!(servidor["timeout"], 86_400_000);
        assert_eq!(
            servidor["command"],
            json!([r"C:\Tirech\tirech-omni.exe", "mcp", "--espaco", "web"])
        );
        assert_eq!(lido["$schema"], "https://opencode.ai/config.json");
    }

    #[test]
    fn com_espaco_muda_o_nome_do_servidor() {
        assert_eq!(nome_do_servidor(None), "tirech-omni");
        assert_eq!(
            nome_do_servidor(Some("Produção Web")),
            "tirech-omni-produ-o-web"
        );
        assert_eq!(nome_do_servidor(Some("  ")), "tirech-omni");
        assert_eq!(
            comando_claude("/x/tirech-omni", Escopo::Local, Some("web")),
            "claude mcp add tirech-omni-web -- /x/tirech-omni mcp --espaco web"
        );
        assert_eq!(
            comando_codex("/x/tirech-omni", Some("Meu Site")),
            "codex mcp add tirech-omni-meu-site -- /x/tirech-omni mcp --espaco \"Meu Site\""
        );
        let toml = toml_codex("/x/tirech-omni", Some("web"));
        assert!(toml.starts_with("[mcp_servers.tirech-omni-web]\n"));
        assert!(toml.contains("args = [\"mcp\", \"--espaco\", \"web\"]"));
        let json: serde_json::Value =
            serde_json::from_str(&json_claude("/x", Some("web"))).unwrap();
        assert_eq!(
            json["mcpServers"]["tirech-omni-web"]["args"],
            json!(["mcp", "--espaco", "web"])
        );
    }

    #[test]
    fn deny_do_claude_nega_as_ferramentas_locais() {
        let json: serde_json::Value = serde_json::from_str(&deny_claude()).unwrap();
        assert_eq!(
            json["permissions"]["deny"],
            json!([
                "Bash",
                "Read",
                "Edit",
                "Write",
                "Glob",
                "Grep",
                "NotebookEdit"
            ])
        );
    }

    #[test]
    fn config_do_codex_desliga_o_shell_e_a_escrita() {
        let toml = config_codex_so_servidor();
        assert!(toml.contains("sandbox_mode = \"read-only\""), "{toml}");
        assert!(toml.contains("[features]"), "{toml}");
        assert!(toml.contains("shell_tool = false"), "{toml}");
        assert!(toml.contains("unified_exec = false"), "{toml}");
    }

    #[test]
    fn ponte_na_aba() {
        assert_eq!(
            args_da_ponte_na_aba("Claude Code · api", "7-1"),
            ["mcp", "--nome", "Claude Code · api", "--aba", "7-1"]
        );
    }

    #[test]
    fn lancar_o_claude_por_arquivo() {
        let ponte = args_da_ponte_na_aba("Claude Code", "0-1");
        let json: serde_json::Value = serde_json::from_str(&json_claude_na_aba(
            r"C:\Program Files\Tirech\tirech-omni.exe",
            &ponte,
        ))
        .unwrap();
        let servidor = &json["mcpServers"]["tirech-omni-aba"];
        assert_eq!(
            servidor["command"],
            r"C:\Program Files\Tirech\tirech-omni.exe"
        );
        assert_eq!(
            servidor["args"],
            json!(["mcp", "--nome", "Claude Code", "--aba", "0-1"])
        );
        assert_eq!(
            lancar_claude(
                Path::new("/home/ana/Meus Dados/lancar-1.json"),
                Path::new("/home/ana/Meus Dados/lancar-1-avisos.json"),
                "Claude Code · api",
                "Use a aba api."
            ),
            [
                "--mcp-config",
                "/home/ana/Meus Dados/lancar-1.json",
                "--settings",
                "/home/ana/Meus Dados/lancar-1-avisos.json",
                "-n",
                "Claude Code · api",
                "--append-system-prompt",
                "Use a aba api."
            ]
        );
    }

    #[test]
    fn o_hook_dos_avisos_chama_a_ponte_com_o_lancamento() {
        let comando = |exe, windows| {
            let json: serde_json::Value =
                serde_json::from_str(&settings_claude_avisos(exe, "4242-3", windows)).unwrap();
            let hook = &json["hooks"]["UserPromptSubmit"][0]["hooks"][0];
            assert_eq!(hook["type"], "command");
            hook["command"].as_str().unwrap().to_owned()
        };
        assert_eq!(
            comando("/opt/Meu App/tirech-omni", false),
            "'/opt/Meu App/tirech-omni' mcp --avisos '4242-3'"
        );
        assert_eq!(
            comando(r"C:\Program Files\Tirech\tirech-omni.exe", true),
            r#""C:\Program Files\Tirech\tirech-omni.exe" mcp --avisos '4242-3'"#
        );
    }

    #[test]
    fn a_conversa_do_claude_comeca_ou_continua() {
        assert_eq!(conversa_claude("abc", false), ["--session-id", "abc"]);
        assert_eq!(conversa_claude("abc", true), ["--resume", "abc"]);
    }

    #[test]
    fn o_notify_do_codex_chama_a_ponte_e_o_da_pessoa() {
        assert_eq!(
            notify_codex(
                "/usr/bin/tirech-omni",
                Path::new("/c/mcp/conversa-1.json"),
                &[]
            ),
            [
                "-c",
                "notify=[\"/usr/bin/tirech-omni\", \"mcp\", \"--conversa\", \"/c/mcp/conversa-1.json\"]"
            ]
        );
        assert_eq!(
            notify_codex("t", Path::new("a.json"), &["notificar".into(), "-q".into()])[1],
            "notify=[\"t\", \"mcp\", \"--conversa\", \"a.json\", \"--\", \"notificar\", \"-q\"]"
        );
    }

    #[test]
    fn lancar_o_codex_com_tres_c() {
        let ponte = args_da_ponte_na_aba("Codex", "3-2");
        assert_eq!(
            lancar_codex("/usr/bin/tirech-omni", &ponte),
            [
                "-c",
                "mcp_servers.tirech-omni.command=\"/usr/bin/tirech-omni\"",
                "-c",
                "mcp_servers.tirech-omni.args=[\"mcp\", \"--nome\", \"Codex\", \"--aba\", \"3-2\"]",
                "-c",
                "mcp_servers.tirech-omni.tool_timeout_sec=86400"
            ]
        );
        // Com barras e espaço, o TOML escapa e o valor volta igual.
        let args = lancar_codex(r"C:\Program Files\t.exe", &ponte);
        assert_eq!(
            args[1],
            r#"mcp_servers.tirech-omni.command="C:\\Program Files\\t.exe""#
        );
    }

    #[test]
    fn lancar_o_opencode_pela_variavel() {
        let ponte = args_da_ponte_na_aba("OpenCode", "5-1");
        let (args, ambiente) = lancar_opencode("/x/tirech-omni", &ponte);
        assert!(args.is_empty());
        assert_eq!(ambiente.len(), 1);
        assert_eq!(ambiente[0].0, "OPENCODE_CONFIG_CONTENT");
        let json: serde_json::Value = serde_json::from_str(&ambiente[0].1).unwrap();
        let servidor = &json["mcp"]["tirech-omni"];
        assert_eq!(servidor["type"], "local");
        assert_eq!(servidor["enabled"], true);
        // 24 h em milissegundos: o `executar` de um comando longo não cai no prazo padrão.
        assert_eq!(servidor["timeout"], 86_400_000);
        assert_eq!(
            servidor["command"],
            json!([
                "/x/tirech-omni",
                "mcp",
                "--nome",
                "OpenCode",
                "--aba",
                "5-1"
            ])
        );
    }
}
