//! Assistentes de IA no servidor (M46): o catálogo dos instaladores, a sondagem e os comandos.
//!
//! **Como atualizar o catálogo:** abra os links `doc` de cada [`Ficha`], confira os comandos,
//! as pastas e as versões mínimas do Node, troque o que mudou e a data [`PESQUISADO_EM`]. Os
//! testes só conferem a forma (todo método tem comando, link e pasta), não a rede.
//!
//! Tudo aqui é puro: nada abre conexão nem roda programa. A tela (`tela/ia.rs`) manda o
//! [`SCRIPT_SONDAGEM`] ao servidor, entrega a saída a [`Sondagem::ler`] e monta os comandos
//! com as funções deste módulo.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tomni_ui::terminal::{ShellQuote, shell_quote};

/// Quando os comandos do catálogo foram conferidos nas páginas oficiais.
pub const PESQUISADO_EM: &str = "2026-10-19";

/// As marcas que cercam a saída da sondagem, para tolerar o lixo que um `.profile` imprime.
pub const MARCA_INICIO: &str = "tirech-ia-inicio";
pub const MARCA_FIM: &str = "tirech-ia-fim";

/// Quanto tempo a sondagem de uma conexão vale antes de a tela perguntar de novo.
pub const VALIDADE_IA: std::time::Duration = std::time::Duration::from_secs(600);

/// Os assistentes que o app sabe instalar e abrir.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Ferramenta {
    #[default]
    Claude,
    Codex,
    OpenCode,
}

impl Ferramenta {
    pub const TODAS: [Ferramenta; 3] =
        [Ferramenta::Claude, Ferramenta::Codex, Ferramenta::OpenCode];

    /// O nome que vai nos ids e nas configurações (`claude`, `codex`, `opencode`).
    pub fn chave(self) -> &'static str {
        match self {
            Ferramenta::Claude => "claude",
            Ferramenta::Codex => "codex",
            Ferramenta::OpenCode => "opencode",
        }
    }

    /// O inverso de [`Ferramenta::chave`].
    pub fn da_chave(chave: &str) -> Option<Ferramenta> {
        Self::TODAS.into_iter().find(|f| f.chave() == chave)
    }
}

/// Como instalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metodo {
    /// O instalador do próprio fabricante (o recomendado).
    Nativo,
    Npm,
    Homebrew,
}

impl Metodo {
    pub const TODOS: [Metodo; 3] = [Metodo::Nativo, Metodo::Npm, Metodo::Homebrew];

    pub fn chave(self) -> &'static str {
        match self {
            Metodo::Nativo => "nativo",
            Metodo::Npm => "npm",
            Metodo::Homebrew => "homebrew",
        }
    }

    pub fn da_chave(chave: &str) -> Option<Metodo> {
        Self::TODOS.into_iter().find(|m| m.chave() == chave)
    }
}

/// O que um método de instalação pede do servidor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requisito {
    Curl,
    Tar,
    Bash,
    Npm,
    /// O Node na versão `minimo` ou mais nova; `em_execucao` é se o assistente precisa dele
    /// também para rodar (o Codex pelo npm), e não só para instalar.
    Node {
        minimo: u32,
        em_execucao: bool,
    },
    Brew,
}

/// Um jeito de instalar uma ferramenta.
#[derive(Clone, Copy, Debug)]
pub struct Instalacao {
    pub metodo: Metodo,
    pub unix: &'static str,
    pub windows: Option<&'static str>,
    pub requisitos: &'static [Requisito],
    /// Onde o binário fica, relativo ao home (`.local/bin`); vazio: a pasta do npm ou do brew.
    pub pasta: &'static str,
    pub doc: &'static str,
}

/// Tudo o que o app sabe de uma ferramenta.
#[derive(Clone, Copy, Debug)]
pub struct Ficha {
    pub ferramenta: Ferramenta,
    pub nome: &'static str,
    pub binario: &'static str,
    /// A primeira é a recomendada.
    pub instalacoes: &'static [Instalacao],
    /// O arquivo da credencial, relativo ao home.
    pub credencial: &'static str,
    /// O login sem navegador no servidor.
    pub entrar: &'static str,
    pub doc_login: &'static str,
}

impl Ficha {
    pub fn instalacao(&self, metodo: Metodo) -> Option<&'static Instalacao> {
        self.instalacoes.iter().find(|i| i.metodo == metodo)
    }
}

pub const CATALOGO: &[Ficha] = &[
    Ficha {
        ferramenta: Ferramenta::Claude,
        nome: "Claude Code",
        binario: "claude",
        instalacoes: &[
            Instalacao {
                metodo: Metodo::Nativo,
                unix: "curl -fsSL https://claude.ai/install.sh | bash",
                windows: Some("irm https://claude.ai/install.ps1 | iex"),
                requisitos: &[Requisito::Curl, Requisito::Bash],
                pasta: ".local/bin",
                doc: "https://code.claude.com/docs/en/setup",
            },
            Instalacao {
                metodo: Metodo::Npm,
                unix: "npm install -g @anthropic-ai/claude-code",
                windows: Some("npm install -g @anthropic-ai/claude-code"),
                requisitos: &[
                    Requisito::Npm,
                    Requisito::Node {
                        minimo: 22,
                        em_execucao: false,
                    },
                ],
                pasta: "",
                doc: "https://code.claude.com/docs/en/setup",
            },
            Instalacao {
                metodo: Metodo::Homebrew,
                unix: "brew install --cask claude-code",
                windows: None,
                requisitos: &[Requisito::Brew],
                pasta: "",
                doc: "https://code.claude.com/docs/en/setup",
            },
        ],
        credencial: ".claude/.credentials.json",
        entrar: "claude setup-token",
        doc_login: "https://code.claude.com/docs/en/authentication",
    },
    Ficha {
        ferramenta: Ferramenta::Codex,
        nome: "Codex",
        binario: "codex",
        instalacoes: &[
            Instalacao {
                metodo: Metodo::Nativo,
                unix: "curl -fsSL https://chatgpt.com/codex/install.sh | sh",
                windows: Some(
                    "powershell -ExecutionPolicy ByPass -c \"irm https://chatgpt.com/codex/install.ps1 | iex\"",
                ),
                requisitos: &[Requisito::Curl, Requisito::Tar],
                pasta: ".local/bin",
                doc: "https://github.com/openai/codex",
            },
            Instalacao {
                metodo: Metodo::Npm,
                unix: "npm install -g @openai/codex",
                windows: Some("npm install -g @openai/codex"),
                requisitos: &[
                    Requisito::Npm,
                    Requisito::Node {
                        minimo: 16,
                        em_execucao: true,
                    },
                ],
                pasta: "",
                doc: "https://github.com/openai/codex",
            },
            Instalacao {
                metodo: Metodo::Homebrew,
                unix: "brew install --cask codex",
                windows: None,
                requisitos: &[Requisito::Brew],
                pasta: "",
                doc: "https://github.com/openai/codex",
            },
        ],
        credencial: ".codex/auth.json",
        entrar: "codex login --device-auth",
        doc_login: "https://learn.chatgpt.com/docs/auth",
    },
    Ficha {
        ferramenta: Ferramenta::OpenCode,
        nome: "OpenCode",
        binario: "opencode",
        instalacoes: &[
            Instalacao {
                metodo: Metodo::Nativo,
                unix: "curl -fsSL https://opencode.ai/install | bash",
                windows: None,
                requisitos: &[Requisito::Curl, Requisito::Tar, Requisito::Bash],
                pasta: ".opencode/bin",
                doc: "https://opencode.ai/docs/",
            },
            Instalacao {
                metodo: Metodo::Npm,
                unix: "npm install -g opencode-ai",
                windows: Some("npm install -g opencode-ai"),
                requisitos: &[Requisito::Npm],
                pasta: "",
                doc: "https://opencode.ai/docs/",
            },
            Instalacao {
                metodo: Metodo::Homebrew,
                unix: "brew install anomalyco/tap/opencode",
                windows: None,
                requisitos: &[Requisito::Brew],
                pasta: "",
                doc: "https://opencode.ai/docs/",
            },
        ],
        credencial: ".local/share/opencode/auth.json",
        entrar: "opencode auth login",
        doc_login: "https://opencode.ai/docs/cli/",
    },
];

/// A ficha de `ferramenta`.
pub fn ficha(ferramenta: Ferramenta) -> &'static Ficha {
    CATALOGO
        .iter()
        .find(|f| f.ferramenta == ferramenta)
        .expect("o catálogo tem as três ferramentas")
}

/// O script POSIX que a sondagem manda pelo stdin de `sh -s`. A saída é `chave=valor` entre
/// as marcas; o `path` do fim é o que o app usa depois para instalar e abrir, para funcionar
/// mesmo que o rc do shell só monte o `PATH` no modo interativo.
pub const SCRIPT_SONDAGEM: &str = r#"echo tirech-ia-inicio
echo "uname=$(uname -sm 2>/dev/null)"
echo "home=$HOME"
echo "shell=${SHELL:-}"
if [ -f /etc/alpine-release ] || ldd --version 2>&1 | grep -qi musl; then echo musl=1; fi
t=; command -v timeout >/dev/null 2>&1 && t="timeout 5"
lp=$($t "${SHELL:-sh}" -lc 'printf %s "$PATH"' </dev/null 2>/dev/null)
echo "path_login=$lp"
ex="$HOME/.local/bin:$HOME/.opencode/bin:$HOME/.npm-global/bin:$HOME/.bun/bin:$HOME/.volta/bin"
ex="$ex:$HOME/.asdf/shims:$HOME/.local/share/mise/shims:/opt/homebrew/bin:/usr/local/bin"
for d in "$HOME"/.nvm/versions/node/*/bin; do [ -d "$d" ] && ex="$ex:$d"; done
PATH="$lp:$PATH:$ex"; export PATH
np=$(npm prefix -g 2>/dev/null)
if [ -n "$np" ]; then PATH="$PATH:$np/bin"; echo "npm_prefixo=$np"; [ -w "$np" ] && echo npm_gravavel=1; fi
for p in node npm curl wget tar bash brew rg; do echo "tem_$p=$(command -v "$p" 2>/dev/null)"; done
echo "node=$(node --version 2>/dev/null)"
for p in claude codex opencode; do
  c=$(command -v "$p" 2>/dev/null); echo "$p=$c"
  [ -n "$c" ] && echo "${p}_versao=$($t "$c" --version </dev/null 2>/dev/null | head -n 1)"
done
[ -f "$HOME/.claude/.credentials.json" ] && echo claude_logado=1
[ -f "${CODEX_HOME:-$HOME/.codex}/auth.json" ] && echo codex_logado=1
[ -f "${XDG_DATA_HOME:-$HOME/.local/share}/opencode/auth.json" ] && echo opencode_logado=1
echo "path=$PATH"
echo tirech-ia-fim
"#;

/// O comando do `exec` da sondagem num servidor Windows: o script vai pelo stdin, e
/// `-NonInteractive` impede que um prompt prenda a conexão.
pub const COMANDO_SONDAGEM_WINDOWS: &str = "powershell -NoProfile -NonInteractive -Command -";

/// O mesmo da [`SCRIPT_SONDAGEM`], em PowerShell, com as mesmas chaves mais `windows=1`. Cada
/// instrução fica numa linha só: no `-Command -` o PowerShell lê o stdin linha a linha. O
/// `'' |` antes dos programas impede que eles leiam o resto do script como entrada.
pub const SCRIPT_SONDAGEM_WINDOWS: &str = r#"$ErrorActionPreference = 'SilentlyContinue'
'tirech-ia-inicio'
'windows=1'
"uname=Windows $env:PROCESSOR_ARCHITECTURE"
"home=$HOME"
'shell=powershell'
$ex = @("$HOME\.local\bin", "$HOME\.opencode\bin", "$env:APPDATA\npm", "$HOME\.bun\bin", "$env:LOCALAPPDATA\Volta\bin", "$env:ProgramFiles\nodejs")
$env:PATH = $env:PATH + ';' + ($ex -join ';')
$np = (& { '' | & npm prefix -g 2>$null } | Select-Object -First 1)
if ($np) { "npm_prefixo=$np" }
function Tirech-Versao($c) { if ($c) { '' | & $c --version 2>$null | Select-Object -First 1 } }
foreach ($p in 'node','npm','curl','wget','tar','bash','brew','rg') { $c = (Get-Command $p -CommandType Application | Select-Object -First 1).Source; "tem_$p=$c" }
"node=" + (Tirech-Versao (Get-Command node -CommandType Application | Select-Object -First 1).Source)
foreach ($p in 'claude','codex','opencode') { $c = (Get-Command $p -CommandType Application | Select-Object -First 1).Source; "$p=$c"; if ($c) { "${p}_versao=" + (Tirech-Versao $c) } }
if (Test-Path "$HOME\.claude\.credentials.json") { 'claude_logado=1' }
$ch = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { "$HOME\.codex" }
if (Test-Path "$ch\auth.json") { 'codex_logado=1' }
$xd = if ($env:XDG_DATA_HOME) { $env:XDG_DATA_HOME } else { "$HOME\.local\share" }
if (Test-Path "$xd\opencode\auth.json") { 'opencode_logado=1' }
"path=$env:PATH"
'tirech-ia-fim'
"#;

/// Se o `stderr` do `sh -s` é o de um servidor Windows sem `sh`: o `cmd.exe` diz "is not
/// recognized as an internal or external command" e o PowerShell, "The term 'sh' is not
/// recognized" (ou o equivalente em português: "não é reconhecido").
pub fn parece_windows(stderr: &str) -> bool {
    let minusculo = stderr.to_lowercase();
    minusculo.contains("not recognized") || minusculo.contains("não é reconhecido")
}

/// O que a sondagem achou de uma ferramenta.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Achado {
    pub caminho: Option<String>,
    pub versao: Option<String>,
    pub logado: bool,
}

/// O que a sondagem descobriu do servidor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sondagem {
    pub so: String,
    pub arq: String,
    pub musl: bool,
    pub windows: bool,
    pub home: String,
    pub shell: String,
    /// O `PATH` completo da sondagem (login, extras e o do npm).
    pub path: String,
    pub node: Option<(u32, u32, u32)>,
    pub npm: bool,
    pub npm_prefixo: String,
    pub npm_gravavel: bool,
    pub curl: bool,
    pub wget: bool,
    pub tar: bool,
    pub bash: bool,
    pub brew: bool,
    pub rg: bool,
    pub achados: BTreeMap<Ferramenta, Achado>,
}

impl Sondagem {
    /// Lê a saída do script. `None` sem as duas marcas. Linhas fora das marcas, chaves
    /// desconhecidas e linhas sem `=` são ignoradas.
    pub fn ler(saida: &str) -> Option<Sondagem> {
        let mut dentro = false;
        let mut fechou = false;
        let mut s = Sondagem::default();
        for linha in saida.lines() {
            let linha = linha.trim_end_matches('\r');
            if !dentro {
                dentro = linha.trim() == MARCA_INICIO;
                continue;
            }
            if linha.trim() == MARCA_FIM {
                fechou = true;
                break;
            }
            let Some((chave, valor)) = linha.split_once('=') else {
                continue;
            };
            s.campo(chave.trim(), valor.trim());
        }
        fechou.then_some(s)
    }

    fn campo(&mut self, chave: &str, valor: &str) {
        let tem = !valor.is_empty();
        match chave {
            "uname" => {
                let mut partes = valor.split_whitespace();
                self.so = partes.next().unwrap_or_default().to_owned();
                self.arq = partes.next().unwrap_or_default().to_owned();
            }
            "home" => self.home = valor.to_owned(),
            "shell" => self.shell = valor.to_owned(),
            "musl" => self.musl = tem,
            "windows" => self.windows = tem,
            "path" => self.path = valor.to_owned(),
            "npm_prefixo" => self.npm_prefixo = valor.to_owned(),
            "npm_gravavel" => self.npm_gravavel = tem,
            "tem_npm" => self.npm = tem,
            "tem_curl" => self.curl = tem,
            "tem_wget" => self.wget = tem,
            "tem_tar" => self.tar = tem,
            "tem_bash" => self.bash = tem,
            "tem_brew" => self.brew = tem,
            "tem_rg" => self.rg = tem,
            "node" => self.node = versao_numerica(valor),
            "claude" | "codex" | "opencode" => {
                if let Some(f) = Ferramenta::da_chave(chave) {
                    self.achado(f).caminho = tem.then(|| valor.to_owned());
                }
            }
            "claude_versao" | "codex_versao" | "opencode_versao" => {
                if let Some(f) = Ferramenta::da_chave(chave.trim_end_matches("_versao")) {
                    self.achado(f).versao = versao(valor);
                }
            }
            "claude_logado" | "codex_logado" | "opencode_logado" => {
                if let Some(f) = Ferramenta::da_chave(chave.trim_end_matches("_logado")) {
                    self.achado(f).logado = tem;
                }
            }
            _ => {}
        }
    }

    fn achado(&mut self, f: Ferramenta) -> &mut Achado {
        self.achados.entry(f).or_default()
    }

    /// Se a ferramenta foi achada no `PATH` do servidor.
    pub fn instalada(&self, f: Ferramenta) -> bool {
        self.achados.get(&f).is_some_and(|a| a.caminho.is_some())
    }

    pub fn versao_de(&self, f: Ferramenta) -> Option<&str> {
        self.achados.get(&f)?.versao.as_deref()
    }

    pub fn logado(&self, f: Ferramenta) -> bool {
        self.achados.get(&f).is_some_and(|a| a.logado)
    }

    /// Se o instalador oficial tem binário para a arquitetura do servidor (vazia: não se sabe,
    /// e vale).
    pub fn arquitetura_suportada(&self) -> bool {
        matches!(
            self.arq.as_str(),
            "" | "x86_64" | "amd64" | "AMD64" | "x64" | "aarch64" | "arm64" | "ARM64"
        )
    }
}

/// A primeira sequência `N.N` ou `N.N.N` do texto: `"2.1.211 (Claude Code)"` → `"2.1.211"`,
/// `"codex-cli 0.163.0"` → `"0.163.0"`, `"v22.11.0"` → `"22.11.0"`.
pub fn versao(texto: &str) -> Option<String> {
    let b = texto.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let inicio = i;
        let mut fim = digitos_ate(b, i);
        let mut pontos = 0;
        while pontos < 2
            && b.get(fim) == Some(&b'.')
            && b.get(fim + 1).is_some_and(u8::is_ascii_digit)
        {
            fim = digitos_ate(b, fim + 1);
            pontos += 1;
        }
        if pontos > 0 {
            return Some(texto[inicio..fim].to_owned());
        }
        i = fim;
    }
    None
}

fn digitos_ate(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    i
}

/// A versão como três números (`"v22.11.0"` → `(22, 11, 0)`; sem o último, zero).
pub fn versao_numerica(texto: &str) -> Option<(u32, u32, u32)> {
    let v = versao(texto)?;
    let mut n = v.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    Some((n.next()?, n.next().unwrap_or(0), n.next().unwrap_or(0)))
}

/// O que falta (ou merece aviso) para instalar por um método.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Falta {
    Curl,
    Tar,
    Bash,
    Npm,
    /// O Node não existe (`tem` vazio) ou é velho demais.
    Node {
        tem: Option<u32>,
        minimo: u32,
        em_execucao: bool,
    },
    Brew,
    /// O instalador oficial não tem binário para esta arquitetura.
    Arquitetura(String),
    /// O prefixo global do npm não é gravável: o comando usa `--prefix "$HOME/.local"`.
    NpmSemPermissao,
    /// Alpine (musl): o Claude Code pede pacotes a mais.
    Alpine,
}

impl Falta {
    /// Se a falta impede o método de funcionar (as outras são só avisos).
    pub fn impede(&self) -> bool {
        match self {
            Falta::Curl | Falta::Tar | Falta::Bash | Falta::Npm | Falta::Brew => true,
            Falta::Node { em_execucao, .. } => *em_execucao,
            Falta::Arquitetura(_) | Falta::NpmSemPermissao | Falta::Alpine => false,
        }
    }
}

/// As faltas de instalar `ficha` por `metodo` num servidor como `s`, na ordem do catálogo.
pub fn faltas(ficha: &Ficha, metodo: Metodo, s: &Sondagem) -> Vec<Falta> {
    let mut faltas = Vec::new();
    if let Some(inst) = ficha.instalacao(metodo) {
        for r in inst.requisitos {
            let falta = match *r {
                Requisito::Curl => (!s.curl).then_some(Falta::Curl),
                Requisito::Tar => (!s.tar).then_some(Falta::Tar),
                Requisito::Bash => (!s.bash).then_some(Falta::Bash),
                Requisito::Npm => (!s.npm).then_some(Falta::Npm),
                Requisito::Brew => (!s.brew).then_some(Falta::Brew),
                Requisito::Node {
                    minimo,
                    em_execucao,
                } => {
                    let tem = s.node.map(|(maior, ..)| maior);
                    // Sem npm o aviso do npm já basta: o Node faltando não acrescenta nada.
                    let velho = tem.is_none_or(|t| t < minimo);
                    (velho && (s.npm || tem.is_some())).then_some(Falta::Node {
                        tem,
                        minimo,
                        em_execucao,
                    })
                }
            };
            faltas.extend(falta);
        }
    }
    if metodo == Metodo::Nativo && !s.arquitetura_suportada() {
        faltas.push(Falta::Arquitetura(s.arq.clone()));
    }
    if metodo == Metodo::Npm && s.npm && !s.npm_gravavel && !s.windows && !s.npm_prefixo.is_empty()
    {
        faltas.push(Falta::NpmSemPermissao);
    }
    if ficha.ferramenta == Ferramenta::Claude && s.musl {
        faltas.push(Falta::Alpine);
    }
    faltas
}

/// O método que o app escolhe sozinho: o nativo se o que ele pede existe; senão o npm (e, no
/// Codex, com o Node que ele exige); senão o Homebrew; senão o nativo (o diálogo mostra o que
/// falta).
pub fn metodo_automatico(ficha: &Ficha, s: &Sondagem) -> Metodo {
    [Metodo::Nativo, Metodo::Npm, Metodo::Homebrew]
        .into_iter()
        .filter(|m| {
            ficha
                .instalacao(*m)
                .is_some_and(|i| i.windows.is_some() || !s.windows)
        })
        .find(|m| !faltas(ficha, *m, s).iter().any(Falta::impede))
        .unwrap_or(Metodo::Nativo)
}

/// Os métodos que existem para o servidor (no Windows, só os que têm comando para ele).
pub fn metodos_do_servidor(ficha: &Ficha, s: &Sondagem) -> Vec<Metodo> {
    ficha
        .instalacoes
        .iter()
        .filter(|i| i.windows.is_some() || !s.windows)
        .map(|i| i.metodo)
        .collect()
}

/// O comando que instala `ficha` por `metodo` (o texto que o diálogo mostra). No npm, se o
/// prefixo global não é gravável, ele instala em `~/.local`, onde a sondagem já procura.
pub fn comando_instalar(ficha: &Ficha, metodo: Metodo, s: &Sondagem) -> String {
    let Some(inst) = ficha.instalacao(metodo) else {
        return String::new();
    };
    let comando = if s.windows {
        inst.windows.unwrap_or(inst.unix)
    } else {
        inst.unix
    };
    if metodo == Metodo::Npm && !s.windows && s.npm && !s.npm_gravavel && !s.npm_prefixo.is_empty()
    {
        return comando.replacen(
            "npm install -g ",
            "npm install -g --prefix \"$HOME/.local\" ",
            1,
        );
    }
    comando.to_owned()
}

/// `'…'` com o `'` do texto como `'\''`: um argumento só para o `sh`.
pub fn aspas_sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Como [`aspas_sh`], para o texto que o **shell de login** do servidor lê (pode ser o fish).
/// Fora de `'…'`, o fish e o `sh` concordam com `\'` e `\\`; dentro, o fish trata `\` como
/// escape. Então o `'` vira `'\''` e a `\` vira `'\\'`.
pub fn aspas_externas(s: &str) -> String {
    let mut fora = String::with_capacity(s.len() + 2);
    fora.push('\'');
    for c in s.chars() {
        match c {
            '\'' => fora.push_str(r"'\''"),
            '\\' => fora.push_str(r"'\\'"),
            c => fora.push(c),
        }
    }
    fora.push('\'');
    fora
}

/// O comando que abre `ficha` numa sessão nova do servidor. `pasta` é onde começar (`None`:
/// o home); `argumentos` são os da configuração, que entram como a pessoa escreveu;
/// `shell_ao_sair` deixa um shell de login na pasta quando a ferramenta termina.
pub fn comando_abrir(
    ficha: &Ficha,
    s: &Sondagem,
    pasta: Option<&str>,
    argumentos: &str,
    shell_ao_sair: bool,
) -> String {
    let caminho = s
        .achados
        .get(&ficha.ferramenta)
        .and_then(|a| a.caminho.as_deref())
        .unwrap_or(ficha.binario);
    let argumentos = argumentos.trim();
    if s.windows {
        return comando_abrir_windows(caminho, pasta, argumentos, shell_ao_sair);
    }
    let mut script = String::new();
    if !s.path.is_empty() {
        script.push_str(&format!("export PATH={}; ", aspas_sh(&s.path)));
    }
    if ficha.ferramenta == Ferramenta::Claude && s.musl && s.rg {
        script.push_str("export USE_BUILTIN_RIPGREP=0; ");
    }
    match pasta.filter(|p| !p.is_empty()) {
        Some(p) => script.push_str(&format!("cd {} 2>/dev/null || cd; ", aspas_sh(p))),
        None => script.push_str("cd; "),
    }
    script.push_str(&aspas_sh(caminho));
    if !argumentos.is_empty() {
        script.push(' ');
        script.push_str(argumentos);
    }
    script.push(';');
    if shell_ao_sair {
        script.push_str(r#" exec "${SHELL:-sh}" -l"#);
    }
    format!("exec sh -c {}", aspas_externas(&script))
}

/// A linha que se digita numa aba que já tem um shell (o modo "nesta aba"): a de
/// [`comando_abrir`] sem o `exec`, que fecharia a aba, e sem o shell de login no fim.
pub fn comando_digitar(
    ficha: &Ficha,
    s: &Sondagem,
    pasta: Option<&str>,
    argumentos: &str,
) -> String {
    let linha = comando_abrir(ficha, s, pasta, argumentos, false);
    match linha.strip_prefix("exec ") {
        Some(sem_exec) => sem_exec.to_owned(),
        None => linha,
    }
}

fn comando_abrir_windows(
    caminho: &str,
    pasta: Option<&str>,
    argumentos: &str,
    shell_ao_sair: bool,
) -> String {
    let mut interno = String::new();
    if let Some(p) = pasta.filter(|p| !p.is_empty()) {
        interno.push_str(&format!(
            "Set-Location -LiteralPath {}; ",
            shell_quote(p, ShellQuote::PowerShell)
        ));
    }
    interno.push_str(&format!(
        "& {}",
        shell_quote(caminho, ShellQuote::PowerShell)
    ));
    if !argumentos.is_empty() {
        interno.push(' ');
        interno.push_str(argumentos);
    }
    let sair = if shell_ao_sair { "-NoExit " } else { "" };
    format!(
        "powershell -NoLogo {sair}-Command \"{}\"",
        interno.replace('"', "\\\"")
    )
}

/// O comando da aba de instalação: instala e deixa a saída à mostra, com o resultado no fim.
pub fn comando_instalar_aba(ficha: &Ficha, metodo: Metodo, s: &Sondagem) -> String {
    let instalar = comando_instalar(ficha, metodo, s);
    if s.windows {
        return format!(
            "powershell -NoLogo -NoExit -Command \"{}\"",
            instalar.replace('"', "\\\"")
        );
    }
    let mut script = String::new();
    if !s.path.is_empty() {
        script.push_str(&format!("export PATH={}; ", aspas_sh(&s.path)));
    }
    script.push_str(&instalar);
    script.push_str(
        r#"; s=$?; echo; if [ $s -eq 0 ]; then echo "tirech: a instalação terminou."; else echo "tirech: a instalação falhou (código $s)."; fi; exit $s"#,
    );
    format!("exec sh -c {}", aspas_externas(&script))
}

/// O comando de login sem navegador no servidor (`claude setup-token`, `codex login
/// --device-auth`, `opencode auth login`).
pub fn comando_entrar(ficha: &Ficha) -> &'static str {
    ficha.entrar
}

/// O comando da aba de login: roda [`comando_entrar`] com o binário que a sondagem achou e,
/// com `shell_ao_sair`, deixa um shell de login aberto depois (o token fica à mostra).
pub fn comando_entrar_aba(ficha: &Ficha, s: &Sondagem, shell_ao_sair: bool) -> String {
    let caminho = s
        .achados
        .get(&ficha.ferramenta)
        .and_then(|a| a.caminho.as_deref())
        .unwrap_or(ficha.binario);
    let resto = ficha
        .entrar
        .strip_prefix(ficha.binario)
        .unwrap_or(ficha.entrar);
    if s.windows {
        let sair = if shell_ao_sair { "-NoExit " } else { "" };
        let interno = format!("& {}{resto}", shell_quote(caminho, ShellQuote::PowerShell));
        return format!(
            "powershell -NoLogo {sair}-Command \"{}\"",
            interno.replace('"', "\\\"")
        );
    }
    let mut script = String::new();
    if !s.path.is_empty() {
        script.push_str(&format!("export PATH={}; ", aspas_sh(&s.path)));
    }
    script.push_str(&format!("cd; {}{resto};", aspas_sh(caminho)));
    if shell_ao_sair {
        script.push_str(r#" exec "${SHELL:-sh}" -l"#);
    }
    format!("exec sh -c {}", aspas_externas(&script))
}

#[cfg(test)]
mod testes {
    use super::*;

    const SAIDA: &str = "Bem-vindo ao servidor\r\n\
        tirech-ia-inicio\r\n\
        uname=Linux x86_64\r\n\
        home=/home/ana\r\n\
        shell=/bin/bash\r\n\
        chave_nova=1\r\n\
        linha sem igual\r\n\
        tem_curl=/usr/bin/curl\r\n\
        tem_tar=/usr/bin/tar\r\n\
        tem_bash=/usr/bin/bash\r\n\
        tem_npm=/usr/bin/npm\r\n\
        tem_wget=\r\n\
        npm_prefixo=/usr\r\n\
        node=v22.11.0\r\n\
        claude=/home/ana/.local/bin/claude\r\n\
        claude_versao=2.1.211 (Claude Code)\r\n\
        codex=/usr/bin/codex\r\n\
        codex_versao=codex-cli 0.163.0\r\n\
        opencode=\r\n\
        claude_logado=1\r\n\
        path=/home/ana/.local/bin:/usr/bin\r\n\
        tirech-ia-fim\r\n\
        mais lixo=do logout\r\n";

    fn servidor() -> Sondagem {
        Sondagem {
            so: "Linux".into(),
            arq: "x86_64".into(),
            home: "/home/ana".into(),
            path: "/usr/bin:/home/ana/.local/bin".into(),
            curl: true,
            tar: true,
            bash: true,
            node: Some((22, 11, 0)),
            npm: true,
            npm_prefixo: "/usr".into(),
            npm_gravavel: true,
            ..Sondagem::default()
        }
    }

    #[test]
    fn o_catalogo_tem_a_forma_certa() {
        assert_eq!(CATALOGO.len(), 3);
        for f in CATALOGO {
            assert!(!f.nome.is_empty() && !f.binario.is_empty(), "{}", f.nome);
            assert!(!f.entrar.is_empty() && f.doc_login.starts_with("https://"));
            assert!(f.credencial.starts_with('.'), "{}", f.credencial);
            assert_eq!(f.instalacoes[0].metodo, Metodo::Nativo, "{}", f.nome);
            for i in f.instalacoes {
                assert!(!i.unix.is_empty(), "{} {:?}", f.nome, i.metodo);
                assert!(i.doc.starts_with("https://"), "{} {:?}", f.nome, i.metodo);
                assert!(!i.requisitos.is_empty());
            }
            assert_eq!(ficha(f.ferramenta).nome, f.nome);
        }
        let partes: Vec<_> = PESQUISADO_EM.split('-').collect();
        assert_eq!(partes.len(), 3);
        assert!(partes[0].len() == 4 && partes[1].len() == 2 && partes[2].len() == 2);
        assert!(partes.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())));
    }

    #[test]
    fn as_chaves_vão_e_voltam() {
        for f in Ferramenta::TODAS {
            assert_eq!(Ferramenta::da_chave(f.chave()), Some(f));
        }
        for m in Metodo::TODOS {
            assert_eq!(Metodo::da_chave(m.chave()), Some(m));
        }
        assert_eq!(Ferramenta::da_chave("gemini"), None);
    }

    #[test]
    fn o_parser_le_a_saida_com_lixo_e_crlf() {
        let s = Sondagem::ler(SAIDA).expect("tem as duas marcas");
        assert_eq!((s.so.as_str(), s.arq.as_str()), ("Linux", "x86_64"));
        assert_eq!(
            (s.home.as_str(), s.shell.as_str()),
            ("/home/ana", "/bin/bash")
        );
        assert!(s.curl && s.tar && s.bash && s.npm && !s.wget && !s.brew);
        assert_eq!(s.node, Some((22, 11, 0)));
        assert_eq!(s.npm_prefixo, "/usr");
        assert_eq!(s.path, "/home/ana/.local/bin:/usr/bin");
        assert!(s.instalada(Ferramenta::Claude) && s.instalada(Ferramenta::Codex));
        assert!(!s.instalada(Ferramenta::OpenCode));
        assert_eq!(s.versao_de(Ferramenta::Claude), Some("2.1.211"));
        assert_eq!(s.versao_de(Ferramenta::Codex), Some("0.163.0"));
        assert!(s.logado(Ferramenta::Claude) && !s.logado(Ferramenta::Codex));
    }

    #[test]
    fn sem_as_duas_marcas_nao_ha_sondagem() {
        assert_eq!(Sondagem::ler(""), None);
        assert_eq!(Sondagem::ler("tirech-ia-inicio\nhome=/x\n"), None);
        assert_eq!(Sondagem::ler("home=/x\ntirech-ia-fim\n"), None);
    }

    #[test]
    fn as_versoes() {
        assert_eq!(versao("2.1.211 (Claude Code)").as_deref(), Some("2.1.211"));
        assert_eq!(versao("codex-cli 0.163.0").as_deref(), Some("0.163.0"));
        assert_eq!(versao("1.19.3").as_deref(), Some("1.19.3"));
        assert_eq!(versao("v22.11").as_deref(), Some("22.11"));
        assert_eq!(versao("sem número 7 aqui"), None);
        assert_eq!(versao(""), None);
        assert_eq!(versao_numerica("v22.11.0"), Some((22, 11, 0)));
        assert_eq!(versao_numerica("v18.3"), Some((18, 3, 0)));
        assert_eq!(versao_numerica("nada"), None);
    }

    #[test]
    fn o_script_tem_as_marcas_e_as_chaves_que_o_parser_le() {
        assert!(SCRIPT_SONDAGEM.starts_with(&format!("echo {MARCA_INICIO}")));
        assert!(
            SCRIPT_SONDAGEM
                .trim_end()
                .ends_with(&format!("echo {MARCA_FIM}"))
        );
        for chave in [
            "uname=",
            "home=",
            "path=",
            "npm_prefixo=",
            "npm_gravavel=",
            "node=",
        ] {
            assert!(SCRIPT_SONDAGEM.contains(chave), "{chave}");
        }
    }

    #[test]
    fn o_script_do_windows_tem_as_marcas_e_as_chaves_que_o_parser_le() {
        assert!(
            SCRIPT_SONDAGEM_WINDOWS
                .lines()
                .nth(1)
                .is_some_and(|l| l == format!("'{MARCA_INICIO}'"))
        );
        assert!(
            SCRIPT_SONDAGEM_WINDOWS
                .trim_end()
                .ends_with(&format!("'{MARCA_FIM}'"))
        );
        for chave in [
            "windows=1",
            "uname=",
            "home=",
            "path=",
            "npm_prefixo=",
            "tem_$p=",
            "node=",
            "${p}_versao=",
            "claude_logado=",
            "codex_logado=",
            "opencode_logado=",
        ] {
            assert!(SCRIPT_SONDAGEM_WINDOWS.contains(chave), "{chave}");
        }
        // Linha única por instrução: nada de bloco aberto no fim da linha.
        for linha in SCRIPT_SONDAGEM_WINDOWS.lines() {
            assert!(!linha.trim_end().ends_with('{'), "{linha}");
        }
    }

    #[test]
    fn a_saida_do_windows_vira_sondagem() {
        let saida = "tirech-ia-inicio\r\nwindows=1\r\nuname=Windows AMD64\r\n\
            home=C:\\Users\\ana\r\nshell=powershell\r\nnpm_prefixo=C:\\Users\\ana\\AppData\\Roaming\\npm\r\n\
            tem_npm=C:\\Program Files\\nodejs\\npm.cmd\r\ntem_curl=C:\\Windows\\System32\\curl.exe\r\n\
            node=v22.11.0\r\nclaude=C:\\Users\\ana\\.local\\bin\\claude.exe\r\nclaude_versao=2.1.211 (Claude Code)\r\n\
            codex=\r\nclaude_logado=1\r\ntirech-ia-fim\r\n";
        let s = Sondagem::ler(saida).expect("com as duas marcas");
        assert!(s.windows);
        assert_eq!((s.so.as_str(), s.arq.as_str()), ("Windows", "AMD64"));
        assert!(s.arquitetura_suportada());
        assert!(s.npm && s.curl && !s.wget);
        assert_eq!(s.node, Some((22, 11, 0)));
        assert!(s.instalada(Ferramenta::Claude) && !s.instalada(Ferramenta::Codex));
        assert_eq!(s.versao_de(Ferramenta::Claude), Some("2.1.211"));
        assert!(s.logado(Ferramenta::Claude));
    }

    #[test]
    fn reconhece_o_erro_de_um_servidor_windows() {
        assert!(parece_windows(
            "'sh' is not recognized as an internal or external command,\r\noperable program or batch file."
        ));
        assert!(parece_windows(
            "sh : The term 'sh' is not recognized as the name of a cmdlet"
        ));
        assert!(parece_windows(
            "'sh' não é reconhecido como um comando interno"
        ));
        assert!(!parece_windows("sh: 1: cannot open /dev/null"));
        assert!(!parece_windows(""));
    }

    #[test]
    fn o_metodo_automatico() {
        let claude = ficha(Ferramenta::Claude);
        let mut s = servidor();
        assert_eq!(metodo_automatico(claude, &s), Metodo::Nativo);
        s.curl = false;
        assert_eq!(metodo_automatico(claude, &s), Metodo::Npm);
        // Sem curl nem npm, mas com brew.
        s.npm = false;
        s.brew = true;
        assert_eq!(metodo_automatico(claude, &s), Metodo::Homebrew);
        // Sem nada: o nativo, e o diálogo mostra o que falta.
        s.brew = false;
        assert_eq!(metodo_automatico(claude, &s), Metodo::Nativo);
        assert!(faltas(claude, Metodo::Nativo, &s).contains(&Falta::Curl));
    }

    #[test]
    fn o_codex_pelo_npm_precisa_do_node_para_rodar() {
        let codex = ficha(Ferramenta::Codex);
        let mut s = servidor();
        s.curl = false;
        s.node = Some((14, 0, 0));
        let f = faltas(codex, Metodo::Npm, &s);
        assert_eq!(
            f,
            [Falta::Node {
                tem: Some(14),
                minimo: 16,
                em_execucao: true
            }]
        );
        assert!(f[0].impede());
        // Nenhum método serve: volta ao nativo.
        assert_eq!(metodo_automatico(codex, &s), Metodo::Nativo);
    }

    #[test]
    fn o_node_velho_pelo_npm_do_claude_so_avisa() {
        let claude = ficha(Ferramenta::Claude);
        let mut s = servidor();
        s.node = Some((18, 0, 0));
        let f = faltas(claude, Metodo::Npm, &s);
        assert_eq!(
            f,
            [Falta::Node {
                tem: Some(18),
                minimo: 22,
                em_execucao: false
            }]
        );
        assert!(!f[0].impede());
    }

    #[test]
    fn npm_sem_permissao_instala_em_local() {
        let claude = ficha(Ferramenta::Claude);
        let mut s = servidor();
        s.npm_gravavel = false;
        assert!(faltas(claude, Metodo::Npm, &s).contains(&Falta::NpmSemPermissao));
        assert_eq!(
            comando_instalar(claude, Metodo::Npm, &s),
            r#"npm install -g --prefix "$HOME/.local" @anthropic-ai/claude-code"#
        );
        s.npm_gravavel = true;
        assert_eq!(
            comando_instalar(claude, Metodo::Npm, &s),
            "npm install -g @anthropic-ai/claude-code"
        );
    }

    #[test]
    fn alpine_e_arquitetura_estranha() {
        let claude = ficha(Ferramenta::Claude);
        let mut s = servidor();
        s.musl = true;
        s.arq = "armv7l".into();
        let f = faltas(claude, Metodo::Nativo, &s);
        assert!(f.contains(&Falta::Alpine));
        assert!(f.contains(&Falta::Arquitetura("armv7l".into())));
        assert!(f.iter().all(|f| !f.impede()));
        // O Codex não é afetado pelo Alpine.
        assert!(!faltas(ficha(Ferramenta::Codex), Metodo::Nativo, &s).contains(&Falta::Alpine));
    }

    #[test]
    fn as_aspas_externas() {
        assert_eq!(aspas_sh("it's"), r"'it'\''s'");
        assert_eq!(aspas_externas("a b"), "'a b'");
        assert_eq!(aspas_externas(r"it's"), r"'it'\''s'");
        assert_eq!(aspas_externas(r"a\b"), r"'a'\\'b'");
        assert_eq!(aspas_externas("$HOME é ç"), "'$HOME é ç'");
    }

    /// O que o `sh` entende de `comando` (só `printf`, que não faz nada além de imprimir).
    fn lido_pelo_sh(argumento: &str) -> Option<String> {
        let saida = std::process::Command::new("sh")
            .args(["-c", &format!("printf %s {argumento}")])
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&saida.stdout).into_owned())
    }

    #[test]
    fn as_aspas_dao_o_mesmo_texto_de_volta_no_sh() {
        for texto in [
            "a b",
            "it's",
            r"a\b",
            r"\'\'",
            "$HOME `x` é ç",
            "a;b|c&d",
            "",
        ] {
            for aspas in [aspas_sh as fn(&str) -> String, aspas_externas] {
                if let Some(lido) = lido_pelo_sh(&aspas(texto)) {
                    assert_eq!(lido, texto);
                }
            }
        }
    }

    fn sintaxe_ok(comando: &str) {
        // Só confere a sintaxe (`sh -n`), sem rodar nada.
        let Ok(saida) = std::process::Command::new("sh")
            .args(["-n", "-c", comando])
            .output()
        else {
            return;
        };
        assert!(
            saida.status.success(),
            "{comando}\n{}",
            String::from_utf8_lossy(&saida.stderr)
        );
    }

    #[test]
    fn comando_abrir_com_pasta_dificil() {
        let claude = ficha(Ferramenta::Claude);
        let mut s = servidor();
        s.achados.insert(
            Ferramenta::Claude,
            Achado {
                caminho: Some("/home/ana/.local/bin/claude".into()),
                ..Achado::default()
            },
        );
        let sem_shell = comando_abrir(claude, &s, Some("/srv/meu app/it's"), "--model opus", false);
        assert!(sem_shell.starts_with("exec sh -c '"), "{sem_shell}");
        assert!(sem_shell.contains("--model opus;"), "{sem_shell}");
        assert!(!sem_shell.contains("-l"), "{sem_shell}");
        sintaxe_ok(&sem_shell);
        let com_shell = comando_abrir(claude, &s, Some("/srv/meu app/it's"), "--model opus", true);
        assert!(
            com_shell.contains(r#"exec "${SHELL:-sh}" -l"#),
            "{com_shell}"
        );
        sintaxe_ok(&com_shell);
        // Sem pasta, só o `cd`; sem argumentos, nada depois do binário.
        let home = comando_abrir(claude, &s, None, "  ", false);
        assert!(home.contains("cd; "), "{home}");
        assert!(!home.contains("--"), "{home}");
        sintaxe_ok(&home);
    }

    #[test]
    fn o_claude_no_alpine_usa_o_ripgrep_do_sistema() {
        let claude = ficha(Ferramenta::Claude);
        let mut s = servidor();
        s.musl = true;
        assert!(!comando_abrir(claude, &s, None, "", false).contains("USE_BUILTIN_RIPGREP"));
        s.rg = true;
        assert!(comando_abrir(claude, &s, None, "", false).contains("USE_BUILTIN_RIPGREP=0"));
        assert!(!comando_abrir(ficha(Ferramenta::Codex), &s, None, "", false).contains("RIPGREP"));
    }

    #[test]
    fn o_comando_da_aba_de_instalacao() {
        let claude = ficha(Ferramenta::Claude);
        let s = servidor();
        let c = comando_instalar_aba(claude, Metodo::Nativo, &s);
        assert!(c.starts_with("exec sh -c '"), "{c}");
        assert!(
            c.contains("curl -fsSL https://claude.ai/install.sh | bash"),
            "{c}"
        );
        assert!(c.contains("exit $s"), "{c}");
        sintaxe_ok(&c);
    }

    #[test]
    fn no_windows_vira_powershell() {
        let codex = ficha(Ferramenta::Codex);
        let mut s = servidor();
        s.windows = true;
        s.achados.insert(
            Ferramenta::Codex,
            Achado {
                caminho: Some(r"C:\Users\ana\codex.exe".into()),
                ..Achado::default()
            },
        );
        let c = comando_abrir(codex, &s, Some(r"C:\Meus Projetos"), "--oss", true);
        assert!(
            c.starts_with("powershell -NoLogo -NoExit -Command \""),
            "{c}"
        );
        assert!(c.contains("Set-Location -LiteralPath"), "{c}");
        assert!(c.contains("--oss"), "{c}");
        let sem_shell = comando_abrir(codex, &s, None, "", false);
        assert!(!sem_shell.contains("-NoExit"), "{sem_shell}");
        // O OpenCode não tem instalador nativo para o Windows.
        let opencode = ficha(Ferramenta::OpenCode);
        assert_eq!(metodos_do_servidor(opencode, &s), [Metodo::Npm]);
        assert_eq!(metodo_automatico(opencode, &s), Metodo::Npm);
    }

    #[test]
    fn o_login_sem_navegador() {
        assert_eq!(
            comando_entrar(ficha(Ferramenta::Claude)),
            "claude setup-token"
        );
        assert_eq!(
            comando_entrar(ficha(Ferramenta::Codex)),
            "codex login --device-auth"
        );
        assert_eq!(
            comando_entrar(ficha(Ferramenta::OpenCode)),
            "opencode auth login"
        );
    }
}
