//! A lista de bloqueio (M51): os comandos que parecem perigosos e que pedem a aprovação da
//! pessoa mesmo numa conexão em "Livre" (e mesmo depois de "Aprovar tudo desta sessão"). É
//! puro: texto entra, motivo sai.
//!
//! **Não é uma garantia.** A lista pega os comandos perigosos mais comuns. Um comando escrito
//! para escapar dela escapa. Para uma conexão que não pode errar, use "Aprovar cada ação" ou
//! "Só ler".
//!
//! Antes de casar, o comando é *normalizado*: dividido em comandos simples (por `;`, `&&`, `||`,
//! `|`, `&`, quebra de linha, parênteses e pelo que há dentro de `$( … )`, crases, `<( … )` e
//! `>( … )`), com as aspas e a contrabarra tiradas (sem expandir variáveis), sem os prefixos que
//! só repassam o comando (`sudo`, `env`, `nohup`, `FOO=1`…), com o texto de `bash -c "…"`,
//! `eval`, `su -c` e `ssh host "…"` normalizado de novo, e sem o caminho do programa
//! (`/usr/bin/rm` vira `rm`).

use regex::Regex;

use crate::t;

/// Por que um comando parece perigoso.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motivo {
    /// `rm -rf`.
    RmRf,
    /// Apagar a raiz, a pasta pessoal ou a pasta atual.
    Raiz,
    /// Escrever direto num disco.
    Disco,
    /// Desligar ou reiniciar.
    Desliga,
    /// Mudar as permissões do sistema inteiro.
    Permissoes,
    /// Baixar e rodar um programa.
    BaixaERoda,
    /// Uma bomba de processos.
    Fork,
    /// Derrubar o firewall.
    Firewall,
    /// Mexer em contas ou em quem pode entrar.
    Contas,
    /// Apagar o histórico.
    Historico,
    /// Matar todos os processos.
    MataTudo,
    /// Um comando da lista da pessoa.
    Extra,
}

impl Motivo {
    /// O motivo em português (ou no idioma da interface), para o aviso do diálogo.
    pub fn texto(self) -> String {
        match self {
            Self::RmRf => t!("mcp-bloqueio-rm-rf"),
            Self::Raiz => t!("mcp-bloqueio-raiz"),
            Self::Disco => t!("mcp-bloqueio-disco"),
            Self::Desliga => t!("mcp-bloqueio-desliga"),
            Self::Permissoes => t!("mcp-bloqueio-permissoes"),
            Self::BaixaERoda => t!("mcp-bloqueio-baixa-e-roda"),
            Self::Fork => t!("mcp-bloqueio-fork"),
            Self::Firewall => t!("mcp-bloqueio-firewall"),
            Self::Contas => t!("mcp-bloqueio-contas"),
            Self::Historico => t!("mcp-bloqueio-historico"),
            Self::MataTudo => t!("mcp-bloqueio-mata-tudo"),
            Self::Extra => t!("mcp-bloqueio-extra"),
        }
    }
}

/// Por que um comando parece perigoso, ou `None`.
///
/// `extras` são as regras da pessoa: o nome de um programa (casado depois da normalização) ou,
/// se começar com `re:`, uma expressão regular casada no comando inteiro, sem normalizar. Uma
/// expressão inválida é ignorada (veja [`extras_invalidos`]).
///
/// ```
/// use tirech_omni::mcp::bloqueio::perigoso;
///
/// assert!(perigoso("sudo rm -rf /var/www", &[]).is_some());
/// assert!(perigoso("ls -la", &[]).is_none());
/// assert!(perigoso("docker compose down", &["docker".into()]).is_some());
/// ```
pub fn perigoso(comando: &str, extras: &[String]) -> Option<String> {
    motivo(comando, extras).map(Motivo::texto)
}

/// Como [`perigoso`], mas devolve o [`Motivo`].
pub fn motivo(comando: &str, extras: &[String]) -> Option<Motivo> {
    if bomba_de_processos(comando) {
        return Some(Motivo::Fork);
    }
    let coleta = coletar(comando);
    for cmd in &coleta.cmds {
        if let Some(m) = regra_do_comando(cmd) {
            return Some(m);
        }
    }
    if baixa_e_roda_pelo_pipe(&coleta.cmds) || coleta.baixa_e_roda {
        return Some(Motivo::BaixaERoda);
    }
    extra(comando, &coleta.cmds, extras)
}

/// Os comandos de um texto **digitado** no terminal, que só rodam quando a linha termina em
/// `\r` ou `\n`: casa cada linha completa e ignora a última se ficou sem fim de linha.
pub fn perigoso_digitado(texto: &str, extras: &[String]) -> Option<String> {
    texto
        .split_inclusive(['\n', '\r'])
        .filter(|linha| linha.ends_with(['\n', '\r']))
        .find_map(|linha| perigoso(linha, extras))
}

/// As regras `re:` da lista da pessoa que não são uma expressão regular válida (para a página de
/// configurações avisar).
pub fn extras_invalidos(extras: &[String]) -> Vec<String> {
    extras
        .iter()
        .filter(|e| {
            e.trim()
                .strip_prefix("re:")
                .is_some_and(|r| Regex::new(r).is_err())
        })
        .cloned()
        .collect()
}

/// O primeiro extra que casa.
fn extra(comando: &str, cmds: &[Cmd], extras: &[String]) -> Option<Motivo> {
    for e in extras {
        let e = e.trim();
        if e.is_empty() {
            continue;
        }
        if let Some(r) = e.strip_prefix("re:") {
            if Regex::new(r).is_ok_and(|r| r.is_match(comando)) {
                return Some(Motivo::Extra);
            }
        } else if cmds.iter().any(|c| c.prog == nome_do_programa(e)) {
            return Some(Motivo::Extra);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------------
// A normalização
// ---------------------------------------------------------------------------------------------

/// Um comando simples como saiu da leitura, ainda com os prefixos.
#[derive(Debug, Default)]
struct Cru {
    palavras: Vec<String>,
    /// Os redirecionamentos de saída: o operador (`>`, `>>`, `>|`) e o alvo.
    redirs: Vec<(String, String)>,
    /// O texto de dentro de `$( … )`, de crases e de `<( … )`.
    subs: Vec<String>,
    /// Se a saída dele vai por um pipe para o próximo.
    pipe: bool,
}

/// Um comando simples normalizado.
#[derive(Debug)]
struct Cmd {
    prog: String,
    args: Vec<String>,
    redirs: Vec<(String, String)>,
    /// Se a saída dele vai por um pipe para o próximo da lista.
    pipe: bool,
}

#[derive(Debug, Default)]
struct Coleta {
    cmds: Vec<Cmd>,
    /// Um interpretador que recebe o resultado de um `curl` ou `wget` por substituição
    /// (`bash <(curl …)`, `sh -c "$(curl …)"`).
    baixa_e_roda: bool,
}

/// O que ainda falta normalizar depois de um comando.
enum Depois {
    /// Um texto de shell (`bash -c`, `eval`, `$( … )`); `via_shell` se foi dado a um interpretador.
    Texto(String, bool),
    /// Um comando já em palavras (o de um `find -exec`).
    Palavras(Vec<String>),
}

/// Separa o texto em comandos simples. Não expande variáveis nem interpreta o resto da sintaxe.
fn lexar(texto: &str) -> Vec<Cru> {
    let mut l = Leitor {
        cs: texto.chars().collect(),
        i: 0,
    };
    let mut cmds = Vec::new();
    let mut atual = Cru::default();
    let fecha = |cmds: &mut Vec<Cru>, atual: &mut Cru, pipe: bool| {
        if !atual.palavras.is_empty() || !atual.subs.is_empty() || !atual.redirs.is_empty() {
            let mut pronto = std::mem::take(atual);
            pronto.pipe = pipe;
            cmds.push(pronto);
        }
    };
    while let Some(c) = l.ver(0) {
        match c {
            ' ' | '\t' | '\r' => l.i += 1,
            '\n' | ';' | '(' | ')' => {
                fecha(&mut cmds, &mut atual, false);
                l.i += 1;
            }
            '&' if l.ver(1) == Some('>') => l.redirecionar(&mut atual),
            '&' => {
                fecha(&mut cmds, &mut atual, false);
                l.i += if l.ver(1) == Some('&') { 2 } else { 1 };
            }
            '|' if l.ver(1) == Some('|') => {
                fecha(&mut cmds, &mut atual, false);
                l.i += 2;
            }
            '|' => {
                l.i += if l.ver(1) == Some('&') { 2 } else { 1 };
                fecha(&mut cmds, &mut atual, true);
            }
            '#' => {
                while l.ver(0).is_some_and(|c| c != '\n') {
                    l.i += 1;
                }
            }
            '<' | '>' if l.ver(1) == Some('(') => {
                l.sub_parenteses(1, &mut atual.subs);
                atual.palavras.push("/dev/fd/63".into());
            }
            '<' | '>' => l.redirecionar(&mut atual),
            _ => match l.palavra(&mut atual.subs) {
                Some(w) => atual.palavras.push(w),
                None => l.i += 1,
            },
        }
    }
    fecha(&mut cmds, &mut atual, false);
    cmds
}

struct Leitor {
    cs: Vec<char>,
    i: usize,
}

impl Leitor {
    fn ver(&self, d: usize) -> Option<char> {
        self.cs.get(self.i + d).copied()
    }

    /// O índice do `)` que fecha o `(` em `abre` (ou o fim do texto, se não fecha).
    fn fecha(&self, abre: usize) -> usize {
        let n = self.cs.len();
        let (mut j, mut prof) = (abre, 0usize);
        while j < n {
            match self.cs[j] {
                '\\' => j += 1,
                '\'' => {
                    j += 1;
                    while j < n && self.cs[j] != '\'' {
                        j += 1;
                    }
                }
                '"' => {
                    j += 1;
                    while j < n && self.cs[j] != '"' {
                        if self.cs[j] == '\\' {
                            j += 1;
                        }
                        j += 1;
                    }
                }
                '(' => prof += 1,
                ')' => {
                    prof = prof.saturating_sub(1);
                    if prof == 0 {
                        return j;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        n
    }

    /// Guarda o texto do `( … )` que abre em `i + deslocamento` e passa depois dele.
    fn sub_parenteses(&mut self, deslocamento: usize, subs: &mut Vec<String>) {
        let abre = self.i + deslocamento;
        let fim = self.fecha(abre).min(self.cs.len());
        subs.push(self.cs[(abre + 1).min(fim)..fim].iter().collect());
        self.i = (fim + 1).min(self.cs.len());
    }

    /// Guarda o texto entre crases e passa depois delas.
    fn sub_crase(&mut self, subs: &mut Vec<String>) {
        self.i += 1;
        let mut texto = String::new();
        while let Some(c) = self.ver(0) {
            self.i += 1;
            match c {
                '`' => break,
                '\\' if matches!(self.ver(0), Some('`' | '\\')) => {
                    texto.extend(self.ver(0));
                    self.i += 1;
                }
                c => texto.push(c),
            }
        }
        subs.push(texto);
    }

    /// O resto de uma palavra entre aspas duplas (o `"` de abrir já foi lido).
    fn aspas_duplas(&mut self, palavra: &mut String, subs: &mut Vec<String>) {
        while let Some(c) = self.ver(0) {
            match c {
                '"' => {
                    self.i += 1;
                    return;
                }
                '\\' => match self.ver(1) {
                    Some('\n') => self.i += 2,
                    Some(x @ ('"' | '\\' | '$' | '`')) => {
                        palavra.push(x);
                        self.i += 2;
                    }
                    _ => {
                        palavra.push('\\');
                        self.i += 1;
                    }
                },
                '$' if self.ver(1) == Some('(') && self.ver(2) != Some('(') => {
                    self.sub_parenteses(1, subs);
                }
                '`' => self.sub_crase(subs),
                c => {
                    palavra.push(c);
                    self.i += 1;
                }
            }
        }
    }

    /// Uma palavra: vai até um espaço ou um operador fora das aspas. `None` se não leu nada.
    fn palavra(&mut self, subs: &mut Vec<String>) -> Option<String> {
        let mut s = String::new();
        let mut algum = false;
        while let Some(c) = self.ver(0) {
            match c {
                ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | '(' | ')' | '<' | '>' => break,
                '\\' => match self.ver(1) {
                    Some('\n') => self.i += 2,
                    Some(x) => {
                        s.push(x);
                        algum = true;
                        self.i += 2;
                    }
                    None => self.i += 1,
                },
                '\'' => {
                    self.i += 1;
                    algum = true;
                    while let Some(x) = self.ver(0) {
                        self.i += 1;
                        if x == '\'' {
                            break;
                        }
                        s.push(x);
                    }
                }
                '"' => {
                    self.i += 1;
                    algum = true;
                    self.aspas_duplas(&mut s, subs);
                }
                '$' if self.ver(1) == Some('(') && self.ver(2) == Some('(') => {
                    // `$(( … ))`: conta de aritmética, fica como texto.
                    let fim = self.fecha(self.i + 1).min(self.cs.len().saturating_sub(1));
                    s.extend(&self.cs[self.i..=fim]);
                    algum = true;
                    self.i = fim + 1;
                }
                '$' if self.ver(1) == Some('(') => {
                    self.sub_parenteses(1, subs);
                    algum = true;
                }
                '`' => {
                    self.sub_crase(subs);
                    algum = true;
                }
                c => {
                    s.push(c);
                    algum = true;
                    self.i += 1;
                }
            }
        }
        algum.then_some(s)
    }

    /// Lê um redirecionamento (`>`, `>>`, `>|`, `>&`, `&>`, `<`, `<<`…) e o alvo dele.
    fn redirecionar(&mut self, atual: &mut Cru) {
        // O número do descritor colado ao operador (`2>`) não é uma palavra do comando.
        let colado = self.i > 0 && !self.cs[self.i - 1].is_whitespace();
        if colado
            && atual
                .palavras
                .last()
                .is_some_and(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_digit()))
        {
            atual.palavras.pop();
        }
        let mut op = String::new();
        if self.ver(0) == Some('&') {
            self.i += 1;
        }
        op.extend(self.ver(0));
        self.i += 1;
        while op.len() < 3
            && self
                .ver(0)
                .is_some_and(|c| matches!(c, '>' | '<' | '|' | '&'))
        {
            op.extend(self.ver(0));
            self.i += 1;
        }
        while self.ver(0).is_some_and(|c| c == ' ' || c == '\t') {
            self.i += 1;
        }
        let alvo = self.palavra(&mut atual.subs).unwrap_or_default();
        if op.starts_with('>') && !op.ends_with('&') {
            atual.redirs.push((op, alvo));
        }
    }
}

/// As palavras que abrem um bloco do shell e não são o comando.
const RESERVADAS: [&str; 11] = [
    "{", "}", "!", "if", "then", "else", "elif", "do", "while", "until", "done",
];

/// Os shells que aceitam `-c` e que rodam o que recebem.
const SHELLS: [&str; 7] = ["sh", "bash", "zsh", "dash", "ksh", "fish", "ash"];

/// `NOME=valor` (ou `NOME+=valor`).
fn eh_atribuicao(w: &str) -> bool {
    let Some(p) = w.find('=') else {
        return false;
    };
    let nome = w[..p].strip_suffix('+').unwrap_or(&w[..p]);
    nome.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && nome.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// O nome do programa sem o caminho (`/usr/bin/rm` → `rm`).
fn nome_do_programa(w: &str) -> String {
    let w = w.trim_start_matches('\\');
    w.rsplit('/').next().unwrap_or(w).to_string()
}

/// Tira as opções que vêm logo depois de um prefixo (`sudo -u root -E`). `curtas` são as letras
/// que levam valor (colado ou no argumento seguinte) e `longas` as opções `--nome valor`.
fn pular_opcoes(p: &mut Vec<String>, curtas: &str, longas: &[&str]) {
    while let Some(w) = p.first().cloned() {
        if w == "--" {
            p.remove(0);
            break;
        }
        if !w.starts_with('-') || w.len() == 1 {
            break;
        }
        p.remove(0);
        if let Some(l) = w.strip_prefix("--") {
            if !l.contains('=') && longas.contains(&l) && !p.is_empty() {
                p.remove(0);
            }
            continue;
        }
        let letras: Vec<char> = w[1..].chars().collect();
        for (k, ch) in letras.iter().enumerate() {
            if curtas.contains(*ch) {
                if k + 1 == letras.len() && !p.is_empty() {
                    p.remove(0);
                }
                break;
            }
        }
    }
}

/// O texto depois do `-c` de um interpretador (`bash -lc "…"`, `sh -c '…'`).
fn texto_do_c(args: &[String]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            return None;
        }
        if a.starts_with("--") {
            i += if matches!(a.as_str(), "--rcfile" | "--init-file") {
                2
            } else {
                1
            };
            continue;
        }
        if a.len() > 1 && (a.starts_with('-') || a.starts_with('+')) {
            if a.starts_with('-') && a[1..].contains('c') {
                return args.get(i + 1).cloned();
            }
            i += if a.ends_with(['o', 'O']) { 2 } else { 1 };
            continue;
        }
        return None;
    }
    None
}

/// Tira os prefixos que só repassam o comando e junta o que precisa ser normalizado de novo.
/// Devolve as palavras do comando de verdade (a primeira é o programa, sem caminho).
fn descascar(mut p: Vec<String>) -> (Vec<String>, Vec<Depois>) {
    let mut depois = Vec::new();
    loop {
        while p.first().is_some_and(|w| RESERVADAS.contains(&w.as_str())) {
            p.remove(0);
        }
        let Some(primeiro) = p.first() else {
            break;
        };
        if eh_atribuicao(primeiro) {
            p.remove(0);
            continue;
        }
        let prog = nome_do_programa(primeiro);
        p[0] = prog.clone();
        match prog.as_str() {
            "sudo" => {
                p.remove(0);
                pular_opcoes(
                    &mut p,
                    "ughCprtTUD",
                    &[
                        "user",
                        "group",
                        "host",
                        "prompt",
                        "role",
                        "type",
                        "other-user",
                        "chdir",
                        "close-from",
                        "command-timeout",
                    ],
                );
            }
            "doas" => {
                p.remove(0);
                pular_opcoes(&mut p, "uC", &[]);
            }
            "env" => {
                p.remove(0);
                pular_opcoes(&mut p, "uCS", &["unset", "chdir", "split-string"]);
            }
            "command" | "builtin" | "nohup" | "setsid" | "busybox" => {
                p.remove(0);
                pular_opcoes(&mut p, "", &[]);
            }
            "exec" => {
                p.remove(0);
                pular_opcoes(&mut p, "a", &[]);
            }
            "nice" => {
                p.remove(0);
                pular_opcoes(&mut p, "n", &["adjustment"]);
            }
            "ionice" => {
                p.remove(0);
                pular_opcoes(&mut p, "cnpP", &["class", "classdata", "pid", "pgid"]);
            }
            "time" => {
                p.remove(0);
                pular_opcoes(&mut p, "fo", &["format", "output"]);
            }
            "timeout" => {
                p.remove(0);
                pular_opcoes(&mut p, "sk", &["signal", "kill-after"]);
                if !p.is_empty() {
                    p.remove(0);
                }
            }
            "stdbuf" => {
                p.remove(0);
                pular_opcoes(&mut p, "ioe", &["input", "output", "error"]);
            }
            "xargs" => {
                p.remove(0);
                pular_opcoes(
                    &mut p,
                    "InPLdEsa",
                    &[
                        "replace",
                        "max-args",
                        "max-procs",
                        "max-lines",
                        "delimiter",
                        "eof",
                        "max-chars",
                        "arg-file",
                    ],
                );
            }
            "eval" => {
                depois.push(Depois::Texto(p[1..].join(" "), true));
                break;
            }
            p0 if SHELLS.contains(&p0) => {
                if let Some(t) = texto_do_c(&p[1..]) {
                    depois.push(Depois::Texto(t, true));
                }
                break;
            }
            "su" => {
                let mut i = 1;
                while i < p.len() {
                    let a = &p[i];
                    if a == "--command"
                        || (a.starts_with('-') && !a.starts_with("--") && a[1..].contains('c'))
                    {
                        if let Some(t) = p.get(i + 1) {
                            depois.push(Depois::Texto(t.clone(), true));
                        }
                        break;
                    }
                    i += 1;
                }
                break;
            }
            "ssh" => {
                let mut resto: Vec<String> = p[1..].to_vec();
                pular_opcoes(&mut resto, "bcDEeFIiJLlmOopQRSWw", &[]);
                if !resto.is_empty() {
                    resto.remove(0);
                }
                if !resto.is_empty() {
                    depois.push(Depois::Texto(resto.join(" "), true));
                }
                break;
            }
            "find" => {
                let mut i = 1;
                while i < p.len() {
                    if matches!(p[i].as_str(), "-exec" | "-execdir" | "-ok" | "-okdir") {
                        let fim = p[i + 1..]
                            .iter()
                            .position(|w| w == ";" || w == "+")
                            .map_or(p.len(), |k| i + 1 + k);
                        depois.push(Depois::Palavras(p[i + 1..fim].to_vec()));
                        i = fim;
                    }
                    i += 1;
                }
                break;
            }
            _ => break,
        }
    }
    (p, depois)
}

/// Normaliza um texto e junta os comandos simples, de todos os níveis, em `col`.
fn expandir(texto: &str, col: &mut Coleta, via_shell: bool) {
    let mut locais = Vec::new();
    let mut fila = Vec::new();
    for cru in lexar(texto) {
        if cru.palavras.is_empty() && via_shell && sub_baixa(&cru.subs) {
            col.baixa_e_roda = true;
        }
        fila.extend(cru.subs.iter().map(|t| Depois::Texto(t.clone(), false)));
        tratar(
            cru.palavras,
            cru.redirs,
            &cru.subs,
            cru.pipe,
            &mut locais,
            &mut fila,
            col,
        );
    }
    col.cmds.extend(locais);
    drenar(fila, col);
}

/// Descasca as palavras de um comando e guarda o resultado.
fn tratar(
    palavras: Vec<String>,
    redirs: Vec<(String, String)>,
    subs: &[String],
    pipe: bool,
    locais: &mut Vec<Cmd>,
    fila: &mut Vec<Depois>,
    col: &mut Coleta,
) {
    let (palavras, mais) = descascar(palavras);
    fila.extend(mais);
    let Some((prog, args)) = palavras.split_first() else {
        return;
    };
    let interpreta =
        SHELLS.contains(&prog.as_str()) || matches!(prog.as_str(), "source" | "." | "eval");
    if interpreta && sub_baixa(subs) {
        col.baixa_e_roda = true;
    }
    locais.push(Cmd {
        prog: prog.clone(),
        args: args.to_vec(),
        redirs,
        pipe,
    });
}

/// Normaliza o que ficou para depois (sem limite de profundidade: cada volta consome texto).
fn drenar(fila: Vec<Depois>, col: &mut Coleta) {
    for item in fila {
        match item {
            Depois::Texto(t, via_shell) => expandir(&t, col, via_shell),
            Depois::Palavras(p) => {
                let mut locais = Vec::new();
                let mut fila = Vec::new();
                tratar(p, Vec::new(), &[], false, &mut locais, &mut fila, col);
                col.cmds.extend(locais);
                drenar(fila, col);
            }
        }
    }
}

fn coletar(texto: &str) -> Coleta {
    let mut col = Coleta::default();
    expandir(texto, &mut col, false);
    col
}

/// Se algum dos textos de substituição começa baixando alguma coisa.
fn sub_baixa(subs: &[String]) -> bool {
    subs.iter().any(|t| {
        coletar(t)
            .cmds
            .first()
            .is_some_and(|c| matches!(c.prog.as_str(), "curl" | "wget"))
    })
}

// ---------------------------------------------------------------------------------------------
// As regras
// ---------------------------------------------------------------------------------------------

/// As letras das opções curtas (`-rf` → `r`, `f`), até o `--`.
fn letras(args: &[String]) -> Vec<char> {
    let mut out = Vec::new();
    for a in args {
        if a == "--" {
            break;
        }
        if a.starts_with('-') && !a.starts_with("--") {
            out.extend(a[1..].chars());
        }
    }
    out
}

/// Se há a opção longa `--nome` (com ou sem `=valor`).
fn longa(args: &[String], nome: &str) -> bool {
    args.iter().any(|a| {
        a.strip_prefix("--")
            .is_some_and(|r| r == nome || r.starts_with(&format!("{nome}=")))
    })
}

/// Os argumentos que não são opção (e tudo o que vem depois do `--`).
fn alvos(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut so_alvos = false;
    for a in args {
        if so_alvos || !a.starts_with('-') {
            out.push(a.as_str());
        } else if a == "--" {
            so_alvos = true;
        }
    }
    out
}

/// `/`, `/*`, `~`, `$HOME`, `.`, `..` e `*` (e as formas com `/` no fim ou `/*`).
fn raiz_ou_pessoal(w: &str) -> bool {
    let t = w.trim_end_matches('/');
    let t = if w.starts_with('/') && t.is_empty() {
        ""
    } else {
        t
    };
    let base = t.strip_suffix("/*").unwrap_or(t);
    matches!(base, "" | "~" | "$HOME" | "${HOME}" | "." | "..")
        && (!base.is_empty() || w.starts_with('/'))
        || matches!(t, "*" | ".*")
}

/// `/` e as pastas do sistema que a regra do `chmod -R` protege.
fn pasta_do_sistema(w: &str) -> bool {
    let t = w.trim_end_matches('/');
    let base = t.strip_suffix("/*").unwrap_or(t);
    (w.starts_with('/') && base.is_empty())
        || matches!(base, "/etc" | "/usr" | "/var" | "/home" | "/boot")
}

/// Um caminho de disco (`/dev/sda`, `/dev/nvme0n1`…).
fn disco(w: &str) -> bool {
    const DISCOS: [&str; 11] = [
        "sd", "hd", "vd", "xvd", "nvme", "mmcblk", "md", "dm-", "loop", "mapper/", "disk/",
    ];
    w.strip_prefix("/dev/")
        .is_some_and(|r| DISCOS.iter().any(|d| r.starts_with(d)))
}

/// Um arquivo que decide quem pode entrar.
fn sensivel(w: &str) -> bool {
    w.ends_with(".ssh/authorized_keys")
        || w.ends_with(".ssh/authorized_keys2")
        || w == "/etc/sudoers"
        || w.starts_with("/etc/sudoers.d/")
        || w == "/etc/shadow"
        || w == "/etc/passwd"
}

fn regra_do_comando(c: &Cmd) -> Option<Motivo> {
    let prog = c.prog.as_str();
    let args = &c.args;
    let letras = letras(args);
    let alvos = alvos(args);
    let tem = |l: char| letras.contains(&l);

    // Redirecionar a saída para um disco ou para um arquivo de contas.
    for (_, alvo) in &c.redirs {
        if disco(alvo) {
            return Some(Motivo::Disco);
        }
        if sensivel(alvo) {
            return Some(Motivo::Contas);
        }
    }

    match prog {
        "rm" | "rmdir" | "shred" | "unlink" => {
            if prog == "rm"
                && (tem('r') || tem('R') || longa(args, "recursive"))
                && (tem('f') || longa(args, "force"))
            {
                return Some(Motivo::RmRf);
            }
            if alvos.iter().any(|a| raiz_ou_pessoal(a)) {
                return Some(Motivo::Raiz);
            }
            if prog == "shred" && alvos.iter().any(|a| a.ends_with("history")) {
                return Some(Motivo::Historico);
            }
            None
        }
        "find" => {
            let apaga = args.iter().any(|a| a == "-delete");
            let exec_rm = c.args.windows(2).any(|w| {
                matches!(w[0].as_str(), "-exec" | "-execdir" | "-ok" | "-okdir")
                    && nome_do_programa(&w[1]) == "rm"
            });
            let inicio: Vec<&str> = args
                .iter()
                .take_while(|a| !a.starts_with('-') && a.as_str() != "(" && a.as_str() != "!")
                .map(String::as_str)
                .collect();
            let so_o_delete = args.iter().filter(|a| a.starts_with('-')).count() == 1;
            let longe = |a: &&str| raiz_ou_pessoal(a) && (!matches!(*a, "." | "..") || so_o_delete);
            ((apaga || exec_rm) && inicio.iter().any(longe)).then_some(Motivo::Raiz)
        }
        p if p.starts_with("mkfs") => Some(Motivo::Disco),
        "mkswap" | "wipefs" | "sgdisk" | "gdisk" | "cfdisk" => Some(Motivo::Disco),
        "fdisk" | "sfdisk" | "parted" => {
            let so_lista = args.iter().any(|a| a == "-l" || a == "--list");
            (!so_lista).then_some(Motivo::Disco)
        }
        "dd" => args
            .iter()
            .any(|a| a.strip_prefix("of=").is_some_and(disco))
            .then_some(Motivo::Disco),
        "shutdown" | "reboot" | "halt" | "poweroff" => Some(Motivo::Desliga),
        "init" | "telinit" => args
            .iter()
            .any(|a| a == "0" || a == "6")
            .then_some(Motivo::Desliga),
        "systemctl" => args
            .iter()
            .any(|a| matches!(a.as_str(), "poweroff" | "reboot" | "halt" | "kexec"))
            .then_some(Motivo::Desliga),
        "chmod" | "chown" | "chgrp" => ((tem('R') || longa(args, "recursive"))
            && alvos.iter().any(|a| pasta_do_sistema(a)))
        .then_some(Motivo::Permissoes),
        "iptables" | "ip6tables" | "iptables-nft" | "iptables-legacy" => {
            (tem('F') || longa(args, "flush")).then_some(Motivo::Firewall)
        }
        "nft" => args
            .iter()
            .any(|a| a == "flush")
            .then_some(Motivo::Firewall),
        "ufw" => args
            .iter()
            .any(|a| a == "disable" || a == "reset")
            .then_some(Motivo::Firewall),
        "passwd" | "usermod" | "userdel" | "chpasswd" => Some(Motivo::Contas),
        "tee" => alvos.iter().any(|a| sensivel(a)).then_some(Motivo::Contas),
        "cp" | "mv" | "install" => alvos
            .last()
            .is_some_and(|a| sensivel(a))
            .then_some(Motivo::Contas),
        "sed" => ((tem('i') || longa(args, "in-place")) && alvos.iter().any(|a| sensivel(a)))
            .then_some(Motivo::Contas),
        "history" => tem('c').then_some(Motivo::Historico),
        "unset" => args
            .iter()
            .any(|a| a == "HISTFILE")
            .then_some(Motivo::Historico),
        "killall5" => Some(Motivo::MataTudo),
        "kill" => args
            .iter()
            .skip(1)
            .any(|a| a == "-1")
            .then_some(Motivo::MataTudo),
        "pkill" => {
            let sinal_9 = args
                .iter()
                .any(|a| matches!(a.as_str(), "-9" | "-KILL" | "-SIGKILL"));
            let de_root = args.windows(2).any(|w| w[0] == "-u" && w[1] == "root")
                || args.iter().any(|a| a == "-uroot");
            (sinal_9 && de_root).then_some(Motivo::MataTudo)
        }
        _ => None,
    }
}

/// Os interpretadores que rodam o que recebem pelo pipe.
fn interpretador(prog: &str) -> bool {
    SHELLS.contains(&prog) || matches!(prog, "perl" | "ruby" | "node") || prog.starts_with("python")
}

/// `curl … | sh`: um `curl` ou `wget` seguido, por pipes, de um interpretador.
fn baixa_e_roda_pelo_pipe(cmds: &[Cmd]) -> bool {
    cmds.iter().enumerate().any(|(i, c)| {
        if !matches!(c.prog.as_str(), "curl" | "wget") {
            return false;
        }
        let mut j = i;
        while cmds[j].pipe && j + 1 < cmds.len() {
            j += 1;
            if interpretador(&cmds[j].prog) {
                return true;
            }
        }
        false
    })
}

/// A bomba de processos (`:(){ :|:& };:` e as variações com outro nome): uma função que se
/// chama em segundo plano dentro de si mesma. Olha o texto sem espaços.
fn bomba_de_processos(comando: &str) -> bool {
    let junto: String = comando.chars().filter(|c| !c.is_whitespace()).collect();
    junto.match_indices("(){").any(|(i, _)| {
        let nome: String = junto[..i]
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || matches!(c, ':' | '_' | '-' | '.'))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        !nome.is_empty() && junto[i + 3..].contains(&format!("{nome}|{nome}&"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(comando: &str) -> Option<Motivo> {
        motivo(comando, &[])
    }

    #[test]
    fn pega_os_comandos_perigosos() {
        use Motivo::*;
        let casos: [(&str, Motivo); 40] = [
            ("rm -rf /", RmRf),
            ("rm -fr ~", RmRf),
            ("rm -r -f .", RmRf),
            ("rm --recursive --force x", RmRf),
            ("/bin/rm -rf /srv", RmRf),
            ("\\rm -rf x", RmRf),
            ("sudo rm -rf /", RmRf),
            ("sudo -u root -E rm -rf /", RmRf),
            ("FOO=1 BAR=2 rm -rf /tmp/x", RmRf),
            ("env -i rm -rf a", RmRf),
            ("nohup nice -n 10 timeout 5 rm -rf a", RmRf),
            ("xargs -0 -I{} rm -rf {}", RmRf),
            ("bash -c \"rm -rf /\"", RmRf),
            ("sh -c 'sudo bash -c \"rm -rf /\"'", RmRf),
            ("bash -lc 'rm -rf x'", RmRf),
            ("eval \"rm -rf x\"", RmRf),
            ("su -c 'rm -rf x' ana", RmRf),
            ("ssh web-01 'rm -rf /srv'", RmRf),
            ("echo ok; rm -rf /", RmRf),
            ("ls && (cd /; rm -rf x)", RmRf),
            ("$(rm -rf ~)", RmRf),
            ("echo \"$(rm -rf ~)\"", RmRf),
            ("find . -name '*.o' -exec rm -rf {} \\;", RmRf),
            ("rm /", Raiz),
            ("rm -r ~/", Raiz),
            ("find / -name x -delete", Raiz),
            ("true && mkfs.ext4 /dev/sdb", Disco),
            ("dd if=/dev/zero of=/dev/nvme0n1", Disco),
            ("echo x > /dev/sda", Disco),
            ("echo x 2>/dev/null >/dev/vda1", Disco),
            ("`reboot`", Desliga),
            ("systemctl reboot", Desliga),
            ("init 0", Desliga),
            ("chmod -R 777 /", Permissoes),
            ("chown -R ana:ana /etc/", Permissoes),
            ("curl -fsSL x | sh", BaixaERoda),
            ("wget -qO- x | sudo bash", BaixaERoda),
            ("curl x | tee log | python3 -", BaixaERoda),
            ("bash <(curl -s x)", BaixaERoda),
            ("sh -c \"$(curl -fsSL x)\"", BaixaERoda),
        ];
        for (comando, esperado) in casos {
            assert_eq!(m(comando), Some(esperado), "{comando}");
        }
    }

    #[test]
    fn pega_o_resto_da_lista() {
        use Motivo::*;
        let casos = [
            (":(){ :|:& };:", Fork),
            (":(){ :|: & }; :", Fork),
            ("bomba() { bomba | bomba & }; bomba", Fork),
            ("iptables -F", Firewall),
            ("nft flush ruleset", Firewall),
            ("ufw disable", Firewall),
            ("ufw reset", Firewall),
            ("passwd ana", Contas),
            ("usermod -aG sudo ana", Contas),
            ("echo k >> ~/.ssh/authorized_keys", Contas),
            (
                "echo 'x ALL=(ALL) ALL' | sudo tee -a /etc/sudoers.d/x",
                Contas,
            ),
            ("sed -i s/a/b/ /etc/sudoers", Contas),
            ("cp chave /root/.ssh/authorized_keys", Contas),
            ("history -c", Historico),
            ("unset HISTFILE", Historico),
            ("shred -u ~/.bash_history", Historico),
            ("kill -9 -1", MataTudo),
            ("killall5", MataTudo),
            ("pkill -9 -u root", MataTudo),
        ];
        for (comando, esperado) in casos {
            assert_eq!(m(comando), Some(esperado), "{comando}");
        }
    }

    #[test]
    fn nao_pega_o_que_e_comum() {
        for comando in [
            "rm arquivo.txt",
            "rm -r build",
            "rm -f a.log",
            "ls -rf",
            "echo \"rm -rf /\"",
            "echo 'sudo reboot'",
            "grep -r reboot .",
            "cat /dev/sda1.log",
            "cat /etc/passwd",
            "cp /etc/passwd /tmp/passwd.bak",
            "curl x -o arquivo",
            "curl -s x | jq .",
            "find . -name '*.log' -delete",
            "find . -name '*.o' -exec rm {} \\;",
            "fdisk -l",
            "chmod -R 755 ./site",
            "kill -9 1234",
            "kill -1 1234",
            "sed -i s/a/b/ config.txt",
            "dd if=/dev/zero of=/dev/null count=1",
            "echo $((1 + 2))",
            "# rm -rf /",
            "ls; # reboot",
            "history",
            "ssh web-01 uptime",
            "",
            "   ",
        ] {
            assert_eq!(m(comando), None, "{comando}");
        }
    }

    #[test]
    fn mostra_o_motivo_em_texto() {
        assert_eq!(
            perigoso("sudo reboot", &[]).as_deref(),
            Some("desliga ou reinicia o servidor")
        );
        assert_eq!(perigoso("ls", &[]), None);
    }

    #[test]
    fn extra_por_nome() {
        let extras = vec!["docker".to_string(), " /usr/bin/terraform ".to_string()];
        assert_eq!(motivo("docker compose down", &extras), Some(Motivo::Extra));
        assert_eq!(motivo("sudo -E docker rm x", &extras), Some(Motivo::Extra));
        assert_eq!(motivo("terraform apply", &extras), Some(Motivo::Extra));
        assert_eq!(motivo("echo docker", &extras), None);
        assert_eq!(motivo("dockerd", &extras), None);
    }

    #[test]
    fn extra_por_regex() {
        let extras = vec!["re:^kubectl\\s+delete\\b".to_string()];
        assert_eq!(motivo("kubectl delete pod x", &extras), Some(Motivo::Extra));
        // A expressão vale no comando inteiro, sem normalizar.
        assert_eq!(motivo("sudo kubectl delete pod x", &extras), None);
        assert_eq!(motivo("kubectl get pods", &extras), None);
    }

    #[test]
    fn regex_invalida_e_ignorada() {
        let extras = vec!["re:(".to_string(), "re:^ls".to_string(), "".to_string()];
        assert_eq!(motivo("echo oi", &extras), None);
        assert_eq!(motivo("ls", &extras), Some(Motivo::Extra));
        assert_eq!(extras_invalidos(&extras), vec!["re:(".to_string()]);
    }

    #[test]
    fn texto_digitado_so_vale_com_o_fim_de_linha() {
        assert_eq!(perigoso_digitado("rm -rf /", &[]), None);
        assert!(perigoso_digitado("rm -rf /\r", &[]).is_some());
        assert!(perigoso_digitado("ls\nrm -rf /\n", &[]).is_some());
        // A segunda linha ainda não foi dada ao shell.
        assert_eq!(perigoso_digitado("ls\nrm -rf /", &[]), None);
        assert_eq!(perigoso_digitado("ls\r\npwd\r\n", &[]), None);
    }

    #[test]
    fn a_normalizacao_tira_prefixos_e_aspas() {
        let col = coletar("FOO=1 sudo -u root /usr/bin/'rm' \"-rf\" a\\ b");
        assert_eq!(col.cmds.len(), 1);
        assert_eq!(col.cmds[0].prog, "rm");
        assert_eq!(col.cmds[0].args, ["-rf", "a b"]);
    }

    #[test]
    fn os_pipes_ficam_ligados() {
        let col = coletar("a | b && c");
        let pipes: Vec<_> = col.cmds.iter().map(|c| (c.prog.as_str(), c.pipe)).collect();
        assert_eq!(pipes, [("a", true), ("b", false), ("c", false)]);
    }

    #[test]
    fn aspas_abertas_e_texto_estranho_nao_travam() {
        for comando in [
            "echo 'abc",
            "echo \"abc",
            "$(",
            "`",
            "a \\",
            "(((",
            ")))",
            "<(",
            ">>",
            "|",
            "&",
        ] {
            let _ = m(comando);
        }
    }
}
