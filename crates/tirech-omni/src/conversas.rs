//! O diário das conversas dos assistentes do "Controlar com" (M63, depois da entrega): o id de
//! cada conversa que o Claude Code, o Codex ou o OpenCode teve com uma aba, para continuar a
//! mesma conversa amanhã. Fica no `conversas.json`, por aba (pela conexão dela), com a mais
//! recente primeiro.
//!
//! Cada assistente guarda as conversas do jeito dele, e este módulo sabe ler os três:
//!
//! - **Claude Code:** o app escolhe o id (`claude --session-id <uuid>`) e continua com
//!   `claude --resume <uuid>`. A conversa fica em `~/.claude/projects/<pasta>/<uuid>.jsonl`, com
//!   a pasta de trabalho escrita com `-` no lugar de tudo que não é letra ou número.
//! - **Codex:** o id só aparece depois da primeira resposta: o `notify` do Codex chama
//!   `tirech-omni mcp --conversa <arquivo>`, que grava o `thread-id` no arquivo. Continua com
//!   `codex resume <id>`. A conversa fica em `~/.codex/sessions/AAAA/MM/DD/rollout-…-<id>.jsonl`.
//! - **OpenCode:** o id sai do `opencode session list --format json`: a sessão da pasta de
//!   trabalho criada depois do lançamento. Continua com `opencode --session <id>`.
//!
//! O Claude Code apaga as conversas depois de 30 dias (por padrão), e a pessoa pode apagar as
//! dos outros: a que sumiu sai da lista na próxima vez que ela for mostrada.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::armazenamento::Armazenamento;

/// Quantas conversas o diário guarda por aba e assistente.
const POR_ABA: usize = 20;

/// O tamanho máximo do título (a primeira mensagem), em caracteres.
const TITULO_MAX: usize = 70;

/// Uma conversa de um assistente com uma aba.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversa {
    /// O assistente (`claude`, `codex` ou `opencode`).
    pub ferramenta: String,
    /// O id da conversa no assistente.
    pub id: String,
    /// A última vez que o "Controlar com" abriu a conversa, em segundos desde 1970.
    #[serde(default)]
    pub quando: u64,
    /// A primeira mensagem, já encurtada (vazia enquanto não se sabe).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub titulo: String,
}

/// O `conversas.json`: as conversas de cada aba, pela chave de [`chave_da_aba`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diario {
    #[serde(default)]
    pub abas: BTreeMap<String, Vec<Conversa>>,
}

/// A chave de uma aba no diário: o id da conexão (0 é o shell local avulso). As abas de uma
/// mesma conexão dividem o diário, que sobrevive ao fechar e abrir o app.
pub fn chave_da_aba(conexao: u64) -> String {
    conexao.to_string()
}

impl Diario {
    fn arquivo(armazenamento: &Armazenamento) -> PathBuf {
        armazenamento.pasta().join("conversas.json")
    }

    /// O diário gravado; vazio sem arquivo.
    pub fn ler(armazenamento: &Armazenamento) -> Self {
        armazenamento
            .ler(&Self::arquivo(armazenamento))
            .unwrap_or_default()
    }

    /// Grava o diário. Falhar não avisa ninguém: o diário é uma conveniência.
    pub fn gravar(&self, armazenamento: &Armazenamento) {
        let _ = armazenamento.gravar(&Self::arquivo(armazenamento), self);
    }

    /// Anota `conversa` na aba `chave`, na ordem da hora (a mais recente na frente). Uma que já
    /// estava (mesmo assistente e id) muda de lugar com a hora nova e guarda o título que já
    /// tinha, se o novo vem vazio.
    pub fn anotar(&mut self, chave: &str, mut conversa: Conversa) {
        let lista = self.abas.entry(chave.to_owned()).or_default();
        if let Some(n) = lista
            .iter()
            .position(|c| c.ferramenta == conversa.ferramenta && c.id == conversa.id)
        {
            let antiga = lista.remove(n);
            if conversa.titulo.is_empty() {
                conversa.titulo = antiga.titulo;
            }
        }
        let lugar = lista
            .iter()
            .position(|c| c.quando <= conversa.quando)
            .unwrap_or(lista.len());
        lista.insert(lugar, conversa);
        let mut vistas = BTreeMap::<String, usize>::new();
        lista.retain(|c| {
            let n = vistas.entry(c.ferramenta.clone()).or_default();
            *n += 1;
            *n <= POR_ABA
        });
    }

    /// As conversas de `ferramenta` na aba `chave`, a mais recente primeiro.
    pub fn da_aba(&self, chave: &str, ferramenta: &str) -> Vec<Conversa> {
        self.abas
            .get(chave)
            .map(|lista| {
                lista
                    .iter()
                    .filter(|c| c.ferramenta == ferramenta)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Troca o título das conversas da aba `chave` pelo de `novas` (o mesmo assistente e id),
    /// quando o novo não é vazio.
    pub fn atualizar_titulos(&mut self, chave: &str, novas: &[Conversa]) {
        let Some(lista) = self.abas.get_mut(chave) else {
            return;
        };
        for conversa in lista {
            if let Some(nova) = novas.iter().find(|n| {
                n.ferramenta == conversa.ferramenta && n.id == conversa.id && !n.titulo.is_empty()
            }) {
                conversa.titulo.clone_from(&nova.titulo);
            }
        }
    }

    /// Tira da aba `chave` as conversas de `ferramenta` cujo id não está em `vivas`.
    pub fn esquecer_sumidas(&mut self, chave: &str, ferramenta: &str, vivas: &[String]) {
        if let Some(lista) = self.abas.get_mut(chave) {
            lista.retain(|c| c.ferramenta != ferramenta || vivas.contains(&c.id));
            if lista.is_empty() {
                self.abas.remove(chave);
            }
        }
    }

    /// Se algum id já está no diário (de qualquer aba) para `ferramenta`.
    pub fn conhece(&self, ferramenta: &str, id: &str) -> bool {
        self.abas
            .values()
            .flatten()
            .any(|c| c.ferramenta == ferramenta && c.id == id)
    }
}

/// Agora, em segundos desde 1970.
pub fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `quando` (segundos desde 1970) na hora local, no `formato` do `strftime`.
pub fn data(quando: u64, formato: &str) -> String {
    let Ok(instante) = jiff::Timestamp::from_second(i64::try_from(quando).unwrap_or(i64::MAX))
    else {
        return String::new();
    };
    instante
        .to_zoned(jiff::tz::TimeZone::system())
        .strftime(formato)
        .to_string()
}

/// Um UUID versão 4, o id que o app escolhe para a conversa do Claude Code.
pub fn uuid_novo() -> String {
    let mut b = [0u8; 16];
    if getrandom::fill(&mut b).is_err() {
        // Sem a fonte do sistema, o relógio serve: o id só precisa não repetir.
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        b = (nanos ^ u128::from(std::process::id()) << 64).to_le_bytes();
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// A primeira linha de `texto`, sem espaços nas pontas e encurtada com "…".
pub fn encurtar(texto: &str) -> String {
    let linha = texto
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if linha.chars().count() <= TITULO_MAX {
        return linha.to_owned();
    }
    let mut curto: String = linha.chars().take(TITULO_MAX - 1).collect();
    curto.truncate(curto.trim_end().len());
    curto.push('…');
    curto
}

/// Se a mensagem é da pessoa, e não o que o assistente põe no começo (`<environment_context>`,
/// `<command-name>`, as instruções do `AGENTS.md`).
fn da_pessoa(texto: &str) -> bool {
    let texto = texto.trim_start();
    !texto.is_empty() && !texto.starts_with('<') && !texto.starts_with("# AGENTS.md")
}

// --- Claude Code ---

/// A pasta das conversas do Claude Code para a pasta de trabalho `pasta`: dentro de
/// `<config>/projects`, com tudo que não é letra ou número trocado por `-`.
pub fn pasta_do_claude(config: &Path, pasta: &Path) -> PathBuf {
    let nome: String = pasta
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    config.join("projects").join(nome)
}

/// A pasta de configuração do Claude Code: `CLAUDE_CONFIG_DIR` ou `~/.claude`.
pub fn config_do_claude(pessoal: Option<&Path>) -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| pessoal.map(|p| p.join(".claude")))
}

/// A primeira mensagem da pessoa num `.jsonl` do Claude Code.
pub fn titulo_do_claude(conteudo: &str) -> Option<String> {
    conteudo.lines().find_map(|linha| {
        let v: Value = serde_json::from_str(linha).ok()?;
        if v.get("type")?.as_str()? != "user" || v.get("isSidechain") == Some(&Value::Bool(true)) {
            return None;
        }
        let conteudo = v.get("message")?.get("content")?;
        let texto = match conteudo {
            Value::String(s) => s.clone(),
            Value::Array(partes) => partes
                .iter()
                .find(|p| p.get("type").and_then(Value::as_str) == Some("text"))?
                .get("text")?
                .as_str()?
                .to_owned(),
            _ => return None,
        };
        da_pessoa(&texto).then(|| encurtar(&texto))
    })
}

/// A conversa `id` do Claude Code na pasta de trabalho `pasta`, se ainda existe: o título.
pub fn conversa_do_claude(config: &Path, pasta: &Path, id: &str) -> Option<String> {
    let arquivo = pasta_do_claude(config, pasta).join(format!("{id}.jsonl"));
    let conteudo = std::fs::read_to_string(arquivo).ok()?;
    Some(titulo_do_claude(&conteudo).unwrap_or_default())
}

// --- Codex ---

/// A pasta do Codex: `CODEX_HOME` ou `~/.codex`.
pub fn config_do_codex(pessoal: Option<&Path>) -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| pessoal.map(|p| p.join(".codex")))
}

/// O arquivo da conversa `id` do Codex, em `sessions/AAAA/MM/DD/rollout-…-<id>.jsonl`.
pub fn arquivo_do_codex(config: &Path, id: &str) -> Option<PathBuf> {
    let fim = format!("-{id}.jsonl");
    let mut pastas = vec![config.join("sessions")];
    for _ in 0..3 {
        pastas = pastas
            .iter()
            .filter_map(|p| std::fs::read_dir(p).ok())
            .flat_map(|e| e.flatten().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect();
    }
    pastas.iter().find_map(|dia| {
        std::fs::read_dir(dia).ok()?.flatten().find_map(|e| {
            let nome = e.file_name();
            let nome = nome.to_string_lossy();
            (nome.starts_with("rollout-") && nome.ends_with(&fim)).then(|| e.path())
        })
    })
}

/// A primeira mensagem da pessoa num `rollout-….jsonl` do Codex: a do evento `user_message`
/// ou, sem ele, o primeiro texto do papel `user` que não é contexto.
pub fn titulo_do_codex(conteudo: &str) -> Option<String> {
    let linhas: Vec<Value> = conteudo
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let do_evento = linhas.iter().find_map(|v| {
        let p = v.get("payload")?;
        (v.get("type")?.as_str()? == "event_msg" && p.get("type")?.as_str()? == "user_message")
            .then(|| p.get("message")?.as_str().map(str::to_owned))?
    });
    do_evento
        .or_else(|| {
            linhas.iter().find_map(|v| {
                let p = v.get("payload")?;
                if p.get("role")?.as_str()? != "user" {
                    return None;
                }
                p.get("content")?.as_array()?.iter().find_map(|parte| {
                    let texto = parte.get("text")?.as_str()?;
                    da_pessoa(texto).then(|| texto.to_owned())
                })
            })
        })
        .filter(|t| da_pessoa(t))
        .map(|t| encurtar(&t))
}

/// A conversa `id` do Codex, se ainda existe: o título.
pub fn conversa_do_codex(config: &Path, id: &str) -> Option<String> {
    let arquivo = arquivo_do_codex(config, id)?;
    let conteudo = std::fs::read_to_string(arquivo).unwrap_or_default();
    Some(titulo_do_codex(&conteudo).unwrap_or_default())
}

/// O que o `notify` do Codex deixou no arquivo do lançamento: o id e a primeira mensagem.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvisoDoCodex {
    pub id: String,
    #[serde(default)]
    pub titulo: String,
}

/// O aviso que o `notify` do Codex manda (o JSON do último argumento): o `thread-id` e a
/// primeira das `input-messages`.
pub fn aviso_do_codex(json: &str) -> Option<AvisoDoCodex> {
    let v: Value = serde_json::from_str(json).ok()?;
    let id = v.get("thread-id")?.as_str()?.trim().to_owned();
    if id.is_empty() {
        return None;
    }
    let titulo = v
        .get("input-messages")
        .and_then(Value::as_array)
        .and_then(|m| m.iter().filter_map(Value::as_str).find(|t| da_pessoa(t)))
        .map(encurtar)
        .unwrap_or_default();
    Some(AvisoDoCodex { id, titulo })
}

/// Grava o aviso do Codex em `arquivo`, se ele ainda não tem um: o primeiro diz a conversa.
pub fn gravar_aviso_do_codex(arquivo: &Path, json: &str) {
    if arquivo.exists() {
        return;
    }
    if let Some(aviso) = aviso_do_codex(json)
        && let Ok(texto) = serde_json::to_string(&aviso)
    {
        let _ = arquivo
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(arquivo, texto));
    }
}

/// O aviso gravado em `arquivo` por [`gravar_aviso_do_codex`].
pub fn ler_aviso_do_codex(arquivo: &Path) -> Option<AvisoDoCodex> {
    serde_json::from_str(&std::fs::read_to_string(arquivo).ok()?).ok()
}

/// O `notify` da pessoa no `config.toml` do Codex (a chave do começo do arquivo, antes da
/// primeira tabela), para o do app chamar depois: o `-c notify=…` do lançamento troca o dela.
/// Só entende a lista de textos numa linha, que é como a documentação escreve.
pub fn notify_da_pessoa(config_toml: &str) -> Option<Vec<String>> {
    for linha in config_toml.lines() {
        let linha = linha.trim();
        if linha.starts_with('[') {
            return None;
        }
        let Some(resto) = linha.strip_prefix("notify") else {
            continue;
        };
        let resto = resto.trim_start().strip_prefix('=')?.trim();
        let dentro = resto.strip_prefix('[')?;
        let mut itens = Vec::new();
        let mut chars = dentro.chars();
        while let Some(c) = chars.next() {
            match c {
                ']' => return (!itens.is_empty()).then_some(itens),
                '"' => {
                    let mut s = String::new();
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => break,
                            '\\' => match chars.next() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(outro) => s.push(outro),
                                None => return None,
                            },
                            c => s.push(c),
                        }
                    }
                    itens.push(s);
                }
                '\'' => itens.push(chars.by_ref().take_while(|&c| c != '\'').collect()),
                _ => {}
            }
        }
        return None;
    }
    None
}

// --- OpenCode ---

/// Uma sessão do `opencode session list --format json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessaoDoOpenCode {
    pub id: String,
    pub titulo: String,
    /// Quando foi criada, em milissegundos desde 1970.
    pub criada: u64,
    pub pasta: String,
}

/// As sessões da saída do `opencode session list --format json`.
pub fn sessoes_do_opencode(saida: &str) -> Vec<SessaoDoOpenCode> {
    let inicio = saida.find('[').unwrap_or(0);
    let Ok(Value::Array(lista)) = serde_json::from_str::<Value>(&saida[inicio..]) else {
        return Vec::new();
    };
    lista
        .iter()
        .filter_map(|s| {
            Some(SessaoDoOpenCode {
                id: s.get("id")?.as_str()?.to_owned(),
                titulo: s
                    .get("title")
                    .and_then(Value::as_str)
                    .map(encurtar)
                    .unwrap_or_default(),
                criada: s.get("created").and_then(Value::as_u64).unwrap_or(0),
                pasta: s
                    .get("directory")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .collect()
}

/// A sessão do OpenCode de um lançamento: a mais antiga da `pasta` criada desde `desde`
/// (segundos, com folga de dois) que nenhum outro lançamento pegou.
pub fn sessao_do_lancamento<'a>(
    sessoes: &'a [SessaoDoOpenCode],
    pasta: &str,
    desde: u64,
    pegas: &[String],
) -> Option<&'a SessaoDoOpenCode> {
    let desde_ms = desde.saturating_sub(2) * 1000;
    sessoes
        .iter()
        .filter(|s| s.pasta == pasta && s.criada >= desde_ms && !pegas.contains(&s.id))
        .min_by_key(|s| s.criada)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conversa(f: &str, id: &str) -> Conversa {
        Conversa {
            ferramenta: f.into(),
            id: id.into(),
            quando: 10,
            titulo: String::new(),
        }
    }

    #[test]
    fn anotar_poe_na_frente_e_nao_repete() {
        let mut d = Diario::default();
        d.anotar("4", conversa("claude", "a"));
        d.anotar("4", conversa("codex", "x"));
        d.anotar(
            "4",
            Conversa {
                titulo: "olá".into(),
                ..conversa("claude", "b")
            },
        );
        d.anotar("4", conversa("claude", "a"));
        let ids: Vec<_> = d.da_aba("4", "claude").into_iter().map(|c| c.id).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(d.da_aba("4", "codex").len(), 1);
        assert!(d.da_aba("5", "claude").is_empty());
        // O título já sabido não se perde quando a mesma conversa volta sem ele.
        d.anotar("4", conversa("claude", "b"));
        assert_eq!(d.da_aba("4", "claude")[0].titulo, "olá");
        assert!(d.conhece("claude", "b") && !d.conhece("codex", "b"));
    }

    #[test]
    fn a_ordem_e_a_da_hora_e_a_conversa_reaberta_vai_para_a_frente() {
        let mut d = Diario::default();
        let em = |id: &str, quando| Conversa {
            quando,
            ..conversa("codex", id)
        };
        d.anotar("2", em("velha", 100));
        d.anotar("2", em("nova", 300));
        // Uma que chega atrasada (resolvida depois) entra no lugar da sua hora.
        d.anotar("2", em("meio", 200));
        let ids = |d: &Diario| -> Vec<String> {
            d.da_aba("2", "codex").into_iter().map(|c| c.id).collect()
        };
        assert_eq!(ids(&d), ["nova", "meio", "velha"]);
        d.anotar("2", em("velha", 400));
        assert_eq!(ids(&d), ["velha", "nova", "meio"]);
        assert_eq!(d.da_aba("2", "codex")[0].quando, 400);
    }

    #[test]
    fn atualizar_titulos_so_troca_pelo_que_veio_preenchido() {
        let mut d = Diario::default();
        d.anotar(
            "3",
            Conversa {
                titulo: "antigo".into(),
                ..conversa("claude", "a")
            },
        );
        d.anotar("3", conversa("claude", "b"));
        d.atualizar_titulos(
            "3",
            &[
                conversa("claude", "a"),
                Conversa {
                    titulo: "novo".into(),
                    ..conversa("claude", "b")
                },
                Conversa {
                    titulo: "outro".into(),
                    ..conversa("codex", "a")
                },
            ],
        );
        let titulos: Vec<_> = d
            .da_aba("3", "claude")
            .into_iter()
            .map(|c| c.titulo)
            .collect();
        assert_eq!(titulos, ["novo", "antigo"]);
        // Aba sem diário: nada a fazer.
        d.atualizar_titulos("9", &[conversa("claude", "a")]);
        assert!(!d.abas.contains_key("9"));
    }

    #[test]
    fn a_data_sai_no_formato_pedido() {
        // Meados de junho de 2025: o ano é o mesmo em qualquer fuso.
        assert_eq!(data(1_750_000_000, "%Y"), "2025");
        assert_eq!(data(1_750_000_000, "%m"), "06");
        // Fora do que o relógio representa, nada.
        assert_eq!(data(u64::MAX, "%Y"), "");
    }

    #[test]
    fn o_diario_guarda_vinte_por_assistente() {
        let mut d = Diario::default();
        for n in 0..25 {
            d.anotar("1", conversa("claude", &n.to_string()));
        }
        d.anotar("1", conversa("codex", "x"));
        assert_eq!(d.da_aba("1", "claude").len(), POR_ABA);
        assert_eq!(d.da_aba("1", "claude")[0].id, "24");
        assert_eq!(d.da_aba("1", "codex").len(), 1);
    }

    #[test]
    fn esquecer_tira_so_as_sumidas_do_assistente() {
        let mut d = Diario::default();
        d.anotar("1", conversa("claude", "a"));
        d.anotar("1", conversa("claude", "b"));
        d.anotar("1", conversa("codex", "c"));
        d.esquecer_sumidas("1", "claude", &["a".into()]);
        assert_eq!(d.da_aba("1", "claude").len(), 1);
        assert_eq!(d.da_aba("1", "codex").len(), 1);
        d.esquecer_sumidas("1", "codex", &[]);
        d.esquecer_sumidas("1", "claude", &[]);
        assert!(d.abas.is_empty());
    }

    #[test]
    fn o_uuid_tem_a_forma_da_versao_4() {
        let id = uuid_novo();
        assert_eq!(id.len(), 36);
        let partes: Vec<_> = id.split('-').map(str::len).collect();
        assert_eq!(partes, [8, 4, 4, 4, 12]);
        assert_eq!(&id[14..15], "4");
        assert!("89ab".contains(&id[19..20]));
        assert_ne!(id, uuid_novo());
    }

    #[test]
    fn encurtar_pega_a_primeira_linha() {
        assert_eq!(encurtar("\n  oi, tudo bem?  \nmais"), "oi, tudo bem?");
        let longo = "a".repeat(100);
        let curto = encurtar(&longo);
        assert_eq!(curto.chars().count(), TITULO_MAX);
        assert!(curto.ends_with('…'));
    }

    #[test]
    fn a_pasta_do_claude_troca_o_que_nao_e_letra_ou_numero() {
        assert_eq!(
            pasta_do_claude(
                Path::new("/home/ana/.claude"),
                Path::new("/home/ana/.cache/tirech-omni/assistentes")
            ),
            Path::new("/home/ana/.claude/projects/-home-ana--cache-tirech-omni-assistentes")
        );
        assert_eq!(
            pasta_do_claude(Path::new("C:/c"), Path::new(r"C:\Users\Ana Maria\x")),
            Path::new("C:/c/projects/C--Users-Ana-Maria-x")
        );
    }

    #[test]
    fn o_titulo_do_claude_e_a_primeira_mensagem_da_pessoa() {
        let jsonl = [
            r#"{"type":"summary","summary":"x"}"#,
            r#"{"type":"user","message":{"role":"user","content":"<command-name>/model</command-name>"}}"#,
            r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"do subagente"}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"x"}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":"preparar a migração\ndo git"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"depois"}}"#,
        ]
        .join("\n");
        assert_eq!(
            titulo_do_claude(&jsonl).as_deref(),
            Some("preparar a migração")
        );
        assert_eq!(titulo_do_claude("{}"), None);
    }

    #[test]
    fn o_titulo_do_codex_vem_do_evento_ou_do_texto_da_pessoa() {
        let com_evento = [
            r#"{"type":"session_meta","payload":{"id":"x"}}"#,
            r#"{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"<environment_context>..."}]}}"#,
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"ver os logs do nginx"}}"#,
        ]
        .join("\n");
        assert_eq!(
            titulo_do_codex(&com_evento).as_deref(),
            Some("ver os logs do nginx")
        );
        let sem_evento = [
            r##"{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions"}]}}"##,
            r#"{"type":"response_item","payload":{"role":"user","content":[{"type":"input_text","text":"reiniciar o php"}]}}"#,
        ]
        .join("\n");
        assert_eq!(
            titulo_do_codex(&sem_evento).as_deref(),
            Some("reiniciar o php")
        );
    }

    #[test]
    fn o_aviso_do_codex_da_o_id_e_a_primeira_mensagem() {
        let json = r#"{"type":"agent-turn-complete","thread-id":"019a-b","turn-id":"1","cwd":"/x","input-messages":["<x>","olhar o disco"],"last-assistant-message":"ok"}"#;
        assert_eq!(
            aviso_do_codex(json),
            Some(AvisoDoCodex {
                id: "019a-b".into(),
                titulo: "olhar o disco".into()
            })
        );
        assert_eq!(aviso_do_codex(r#"{"type":"agent-turn-complete"}"#), None);
        assert_eq!(aviso_do_codex("não é json"), None);
    }

    #[test]
    fn o_primeiro_aviso_do_codex_fica() {
        let pasta = crate::armazenamento::testes::PastaTemporaria::nova("aviso-codex");
        let arquivo = pasta.0.join("mcp").join("conversa-1.json");
        gravar_aviso_do_codex(&arquivo, r#"{"thread-id":"a","input-messages":["um"]}"#);
        gravar_aviso_do_codex(&arquivo, r#"{"thread-id":"b","input-messages":["dois"]}"#);
        assert_eq!(
            ler_aviso_do_codex(&arquivo),
            Some(AvisoDoCodex {
                id: "a".into(),
                titulo: "um".into()
            })
        );
    }

    #[test]
    fn o_arquivo_do_codex_e_achado_pelo_id() {
        let pasta = crate::armazenamento::testes::PastaTemporaria::nova("codex-sessoes");
        let dia = pasta.0.join("sessions/2026/10/25");
        std::fs::create_dir_all(&dia).unwrap();
        let arquivo = dia.join("rollout-2026-10-25T10-00-00-019a-bc.jsonl");
        std::fs::write(
            &arquivo,
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"oi"}}"#,
        )
        .unwrap();
        assert_eq!(arquivo_do_codex(&pasta.0, "019a-bc"), Some(arquivo));
        assert_eq!(
            conversa_do_codex(&pasta.0, "019a-bc").as_deref(),
            Some("oi")
        );
        assert_eq!(conversa_do_codex(&pasta.0, "outro"), None);
    }

    #[test]
    fn o_notify_da_pessoa_e_lido_do_comeco_do_arquivo() {
        let toml = "model = \"o3\"\nnotify = [\"python3\", '/home/ana/notificar.py', \"a\\\"b\"]\n[mcp_servers.x]\nnotify = [\"nao\"]\n";
        assert_eq!(
            notify_da_pessoa(toml),
            Some(vec![
                "python3".into(),
                "/home/ana/notificar.py".into(),
                "a\"b".into()
            ])
        );
        assert_eq!(notify_da_pessoa("[tabela]\nnotify = [\"x\"]\n"), None);
        assert_eq!(notify_da_pessoa("notify = []\n"), None);
        assert_eq!(notify_da_pessoa("notifications = true\n"), None);
    }

    #[test]
    fn a_sessao_do_opencode_e_a_primeira_da_pasta_depois_do_lancamento() {
        let saida = r#"[
            {"id":"ses_a","title":"antiga","created":1000000,"updated":1,"directory":"/p"},
            {"id":"ses_c","title":"segunda","created":2005000,"updated":1,"directory":"/p"},
            {"id":"ses_b","title":"primeira","created":2001000,"updated":1,"directory":"/p"},
            {"id":"ses_d","title":"outra pasta","created":2000500,"updated":1,"directory":"/q"}
        ]"#;
        let sessoes = sessoes_do_opencode(saida);
        assert_eq!(sessoes.len(), 4);
        let achada = sessao_do_lancamento(&sessoes, "/p", 2000, &[]).unwrap();
        assert_eq!(
            (achada.id.as_str(), achada.titulo.as_str()),
            ("ses_b", "primeira")
        );
        let segunda = sessao_do_lancamento(&sessoes, "/p", 2000, &["ses_b".into()]).unwrap();
        assert_eq!(segunda.id, "ses_c");
        assert_eq!(sessao_do_lancamento(&sessoes, "/p", 3000, &[]), None);
        assert!(sessoes_do_opencode("nada").is_empty());
    }
}
