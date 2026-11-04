//! Os perfis dos assistentes deste computador (`ia.perfis_locais`): cada um é um assistente
//! (Claude Code, Codex ou OpenCode) com o seu executável, a sua conta e os seus argumentos. O
//! "Controlar com ▸" mostra um item por perfil, e o diário de conversas guarda o perfil.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ia::{Ferramenta, ficha};

/// Quantos perfis de cada assistente cabem nos menus (as vagas fixas do "Controlar com ▸").
pub const PERFIS_POR_ASSISTENTE: usize = 8;

/// Um perfil de assistente deste computador.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PerfilLocal {
    /// O id, estável: vai para o diário, o `estado.json` e os espaços de trabalho.
    pub id: u64,
    /// O nome nos menus; vazio é o nome do assistente.
    pub nome: String,
    /// O assistente (`claude`, `codex` ou `opencode`).
    pub ferramenta: String,
    /// O executável; vazio procura no `PATH`.
    pub executavel: String,
    /// A pasta da conta; vazia é a conta de sempre do assistente.
    pub conta: String,
    /// Os argumentos extras, numa linha.
    pub argumentos: String,
}

impl Default for PerfilLocal {
    fn default() -> Self {
        Self::novo(0, Ferramenta::Claude)
    }
}

impl PerfilLocal {
    /// Um perfil vazio do assistente.
    pub fn novo(id: u64, ferramenta: Ferramenta) -> Self {
        Self {
            id,
            nome: String::new(),
            ferramenta: ferramenta.chave().to_owned(),
            executavel: String::new(),
            conta: String::new(),
            argumentos: String::new(),
        }
    }

    /// O assistente do perfil (o Claude Code, se a chave não é conhecida).
    pub fn ferramenta(&self) -> Ferramenta {
        Ferramenta::da_chave(&self.ferramenta).unwrap_or_default()
    }

    /// O nome nos menus e nos títulos.
    pub fn nome(&self) -> String {
        match self.nome.trim() {
            "" => ficha(self.ferramenta()).nome.to_owned(),
            nome => nome.to_owned(),
        }
    }

    /// A pasta da conta, com o `~` trocado pela pasta pessoal; `None` é a conta de sempre.
    pub fn pasta_da_conta(&self, pessoal: Option<&Path>) -> Option<PathBuf> {
        expandir(self.conta.trim(), pessoal)
    }

    /// O ambiente que põe o assistente na conta do perfil (vazio na conta de sempre).
    pub fn ambiente_da_conta(&self, pessoal: Option<&Path>) -> Vec<(String, String)> {
        let Some(pasta) = self.pasta_da_conta(pessoal) else {
            return Vec::new();
        };
        let texto = |p: &Path| p.display().to_string();
        match self.ferramenta() {
            Ferramenta::Claude => vec![("CLAUDE_CONFIG_DIR".into(), texto(&pasta))],
            Ferramenta::Codex => vec![("CODEX_HOME".into(), texto(&pasta))],
            Ferramenta::OpenCode => vec![
                ("XDG_DATA_HOME".into(), texto(&pasta.join("data"))),
                ("XDG_STATE_HOME".into(), texto(&pasta.join("state"))),
            ],
        }
    }

    /// As pastas que o lançamento cria antes de abrir o assistente.
    pub fn pastas_a_criar(&self, pessoal: Option<&Path>) -> Vec<PathBuf> {
        self.ambiente_da_conta(pessoal)
            .into_iter()
            .map(|(_, pasta)| PathBuf::from(pasta))
            .collect()
    }
}

/// Um caminho das Configurações: vazio é `None`, e o `~` no começo é a pasta pessoal.
pub fn expandir(texto: &str, pessoal: Option<&Path>) -> Option<PathBuf> {
    if texto.is_empty() {
        return None;
    }
    if let Some(pessoal) = pessoal {
        if texto == "~" {
            return Some(pessoal.to_path_buf());
        }
        if let Some(resto) = texto
            .strip_prefix("~/")
            .or_else(|| texto.strip_prefix("~\\"))
        {
            return Some(pessoal.join(resto));
        }
    }
    Some(PathBuf::from(texto))
}

/// Os perfis de quem nunca mexeu: um por assistente, ids 1, 2 e 3.
pub fn perfis_padrao() -> Vec<PerfilLocal> {
    Ferramenta::TODAS
        .into_iter()
        .zip(1..)
        .map(|(f, id)| PerfilLocal::novo(id, f))
        .collect()
}

/// Tira os perfis de assistente desconhecido e dá um id novo a quem não tem ou repete outro.
/// Devolve se mudou algo.
pub fn arrumar(perfis: &mut Vec<PerfilLocal>) -> bool {
    let antes = perfis.clone();
    perfis.retain(|p| Ferramenta::da_chave(&p.ferramenta).is_some());
    let mut vistos = std::collections::HashSet::new();
    let mut proximo = perfis.iter().map(|p| p.id).max().unwrap_or(0) + 1;
    for perfil in perfis.iter_mut() {
        if perfil.id == 0 || !vistos.insert(perfil.id) {
            perfil.id = proximo;
            vistos.insert(proximo);
            proximo += 1;
        }
    }
    *perfis != antes
}

/// Os caminhos e argumentos do M63, um par de campos por assistente, que viraram os perfis.
const CHAVES_ANTIGAS: [(Ferramenta, &str, &str); 3] = [
    (
        Ferramenta::Claude,
        "local_claude_caminho",
        "local_claude_argumentos",
    ),
    (
        Ferramenta::Codex,
        "local_codex_caminho",
        "local_codex_argumentos",
    ),
    (
        Ferramenta::OpenCode,
        "local_opencode_caminho",
        "local_opencode_argumentos",
    ),
];

/// Um `ia` sem `perfis_locais` ganha os três perfis padrão com o caminho e os argumentos de
/// antes, e perde as chaves antigas. Devolve se mudou algo.
pub fn migrar(raiz: &mut Map<String, Value>) -> bool {
    let Some(Value::Object(ia)) = raiz.get_mut("ia") else {
        return false;
    };
    let tinha_antigas = CHAVES_ANTIGAS
        .iter()
        .any(|(_, caminho, args)| ia.contains_key(*caminho) || ia.contains_key(*args));
    if !tinha_antigas {
        return false;
    }
    let mut perfis = perfis_padrao();
    for (perfil, (f, caminho, args)) in perfis.iter_mut().zip(CHAVES_ANTIGAS) {
        debug_assert_eq!(perfil.ferramenta(), f);
        let texto = |v: Option<Value>| match v {
            Some(Value::String(s)) => s,
            _ => String::new(),
        };
        perfil.executavel = texto(ia.remove(caminho));
        perfil.argumentos = texto(ia.remove(args));
    }
    if !ia.contains_key("perfis_locais") {
        ia.insert(
            "perfis_locais".into(),
            serde_json::to_value(perfis).unwrap_or(Value::Null),
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn os_padroes_sao_um_por_assistente() {
        let p = perfis_padrao();
        assert_eq!(
            p.iter().map(|p| (p.id, p.ferramenta())).collect::<Vec<_>>(),
            vec![
                (1, Ferramenta::Claude),
                (2, Ferramenta::Codex),
                (3, Ferramenta::OpenCode)
            ]
        );
        assert_eq!(p[0].nome(), "Claude Code");
    }

    #[test]
    fn a_migracao_leva_o_caminho_e_os_argumentos() {
        let mut raiz = json!({"ia": {
            "local_claude_caminho": "/opt/claude",
            "local_claude_argumentos": "--model opus",
            "local_codex_caminho": "",
            "local_codex_argumentos": "",
            "local_opencode_caminho": "~/.opencode/bin/opencode",
            "local_opencode_argumentos": "",
            "redigir": false
        }})
        .as_object()
        .cloned()
        .unwrap();
        assert!(migrar(&mut raiz));
        let ia = raiz["ia"].as_object().unwrap();
        assert!(!ia.keys().any(|k| k.starts_with("local_")), "{ia:?}");
        assert_eq!(ia["redigir"], json!(false));
        let perfis: Vec<PerfilLocal> = serde_json::from_value(ia["perfis_locais"].clone()).unwrap();
        assert_eq!(perfis.len(), 3);
        assert_eq!(perfis[0].executavel, "/opt/claude");
        assert_eq!(perfis[0].argumentos, "--model opus");
        assert_eq!(perfis[2].executavel, "~/.opencode/bin/opencode");
        // Uma segunda vez não faz nada.
        assert!(!migrar(&mut raiz));
    }

    #[test]
    fn arrumar_tira_o_desconhecido_e_os_ids_repetidos() {
        let mut perfis = perfis_padrao();
        perfis.push(PerfilLocal {
            id: 2,
            ..PerfilLocal::novo(0, Ferramenta::Claude)
        });
        perfis.push(PerfilLocal {
            ferramenta: "gemini".into(),
            ..PerfilLocal::novo(9, Ferramenta::Claude)
        });
        assert!(arrumar(&mut perfis));
        assert_eq!(
            perfis.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert!(!arrumar(&mut perfis));
    }

    #[test]
    fn a_conta_vira_o_ambiente_de_cada_assistente() {
        let pessoal = Path::new("/home/ana");
        let mut p = PerfilLocal::novo(4, Ferramenta::Claude);
        assert!(p.ambiente_da_conta(Some(pessoal)).is_empty());
        p.conta = "~/contas/b".into();
        assert_eq!(
            p.ambiente_da_conta(Some(pessoal)),
            vec![("CLAUDE_CONFIG_DIR".into(), "/home/ana/contas/b".into())]
        );
        p.ferramenta = "codex".into();
        assert_eq!(p.ambiente_da_conta(Some(pessoal))[0].0, "CODEX_HOME");
        p.ferramenta = "opencode".into();
        assert_eq!(
            p.ambiente_da_conta(Some(pessoal)),
            vec![
                ("XDG_DATA_HOME".into(), "/home/ana/contas/b/data".into()),
                ("XDG_STATE_HOME".into(), "/home/ana/contas/b/state".into())
            ]
        );
    }
}
