//! O app como servidor MCP (M51): um assistente de código (Claude Code, Codex) abre abas, roda
//! comandos e lê arquivos nas conexões que a pessoa liberou.
//!
//! - [`permissao`]: o que cada conexão libera e o que cada ferramenta exige;
//! - [`bloqueio`]: a lista de comandos perigosos, que pedem aprovação mesmo em "Livre";
//! - [`caminho`], [`glob`] e [`gitignore`]: o confinamento à raiz de um espaço de trabalho e o
//!   casamento de nomes de arquivo;
//! - [`espaco`], [`espaco_fs`] e [`espaco_cmd`]: os espaços de trabalho (`ler`, `editar`,
//!   `escrever`, `buscar`, `glob` e `bash` numa pasta do servidor);
//! - [`clientes`]: os comandos e as configurações que ligam o Claude Code e o Codex ao app;
//! - [`ferramentas`]: a lista e os esquemas das ferramentas;
//! - [`prompts`]: os prompts (os comandos com barra do Claude Code), que a ponte responde sozinha;
//! - [`ponte`]: o subcomando `tirech-omni mcp`, que o assistente roda e que repassa as linhas
//!   para o app;
//! - [`canal`] e [`servidor`]: o canal local entre a ponte e o app, e o servidor dentro do app;
//! - `pipe` (só no Windows): o pipe com nome e dono que faz o papel do socket Unix.

pub mod alvo;
pub mod auditoria;
pub mod bloqueio;
pub mod caminho;
pub mod canal;
pub mod clientes;
pub mod espaco;
pub mod espaco_cmd;
pub mod espaco_fs;
pub mod espera;
pub mod ferramentas;
pub mod gitignore;
pub mod glob;
pub mod operacao;
pub mod permissao;
#[cfg(windows)]
pub mod pipe;
pub mod ponte;
pub mod prompts;
pub mod saidas;
pub mod servidor;
pub mod sessoes;
pub mod teclas;

use std::path::{Path, PathBuf};

/// O endereço onde o app atende a ponte, no molde de [`crate::agente::endereco_em`]:
///
/// - Windows: o pipe `\\.\pipe\tirech-omni-mcp-<usuário>`;
/// - Linux com `$XDG_RUNTIME_DIR` absoluto: `<xdg>/tirech-omni/mcp.sock` (sem a variável, a
///   pasta `/run/user/<uid>`, se existe: o Codex roda a ponte só com algumas variáveis);
/// - Linux sem ele e macOS: `mcp.sock` na pasta de configuração do app, que já é do usuário.
///
/// Com a pasta `isolada` (a variável, o modo portable ou a pasta de um teste), o endereço é só
/// dela: no Windows, o pipe ganha `-<resumo da pasta>` no fim; nos outros sistemas, o socket fica
/// na pasta.
pub fn endereco_em(
    windows: bool,
    macos: bool,
    xdg_runtime_dir: Option<&Path>,
    pasta_do_app: &Path,
    usuario: &str,
    isolada: bool,
) -> PathBuf {
    if windows {
        return PathBuf::from(if isolada {
            format!(
                r"\\.\pipe\tirech-omni-mcp-{usuario}-{}",
                crate::agente::resumo_do_caminho(pasta_do_app)
            )
        } else {
            format!(r"\\.\pipe\tirech-omni-mcp-{usuario}")
        });
    }
    match xdg_runtime_dir {
        Some(pasta) if !isolada && !macos && pasta.is_absolute() => {
            pasta.join("tirech-omni").join("mcp.sock")
        }
        _ => pasta_do_app.join("mcp.sock"),
    }
}

/// O endereço para a pasta de dados `pasta`. Fora da pasta padrão (trocada por variável, a do
/// modo portable, ou dada por um teste), o endereço é só dela: dois apps não disputam o endereço
/// da pessoa. Um caminho comprido demais para um socket Unix vira o curto de
/// [`crate::agente::endereco_que_cabe_aqui`].
pub fn endereco_da_pasta(pasta: &Path) -> PathBuf {
    let usuario = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let xdg = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(pasta_de_execucao_do_usuario);
    let isolada = crate::armazenamento::variavel_da_pasta().is_some()
        || crate::armazenamento::modo_portatil()
        || pasta != crate::armazenamento::pasta();
    let desejado = endereco_em(
        cfg!(windows),
        cfg!(target_os = "macos"),
        xdg.as_deref(),
        pasta,
        &usuario,
        isolada,
    );
    if cfg!(windows) {
        desejado
    } else {
        crate::agente::endereco_que_cabe_aqui(desejado)
    }
}

/// A pasta que o `XDG_RUNTIME_DIR` apontaria (`/run/user/<uid>`), para quando a variável não
/// chega: o Codex roda os servidores MCP só com `HOME`, `PATH`, `USER` e poucas outras. O uid é
/// o dono do `/proc/self`, este processo.
#[cfg(target_os = "linux")]
pub(crate) fn pasta_de_execucao_do_usuario() -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self").ok()?.uid();
    let pasta = PathBuf::from(format!("/run/user/{uid}"));
    pasta.is_dir().then_some(pasta)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn pasta_de_execucao_do_usuario() -> Option<PathBuf> {
    None
}

/// O endereço deste processo: o da pasta de dados em uso.
pub fn endereco_padrao() -> PathBuf {
    endereco_da_pasta(&crate::armazenamento::pasta())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_endereco_de_cada_sistema() {
        let xdg = Path::new("/run/user/1000");
        let app = Path::new("/home/ana/.config/tirech-omni");
        assert_eq!(
            endereco_em(false, false, Some(xdg), app, "ana", false),
            Path::new("/run/user/1000/tirech-omni/mcp.sock")
        );
        // Sem XDG, ou com um relativo, vai para a pasta do app.
        assert_eq!(
            endereco_em(false, false, None, app, "ana", false),
            app.join("mcp.sock")
        );
        assert_eq!(
            endereco_em(false, false, Some(Path::new("relativo")), app, "ana", false),
            app.join("mcp.sock")
        );
        // O macOS não usa o XDG.
        assert_eq!(
            endereco_em(false, true, Some(xdg), app, "ana", false),
            app.join("mcp.sock")
        );
        assert_eq!(
            endereco_em(true, false, Some(xdg), app, "ana", false),
            Path::new(r"\\.\pipe\tirech-omni-mcp-ana")
        );
    }

    #[test]
    fn a_pasta_isolada_tem_endereco_proprio() {
        let xdg = Path::new("/run/user/1000");
        let pendrive = Path::new(r"E:\Tirech Omni\tirech-omnidata");
        let outra = Path::new(r"D:\Tirech Omni\tirech-omnidata");
        let pipe = endereco_em(true, false, None, pendrive, "ana", true);
        let texto = pipe.to_string_lossy().into_owned();
        assert!(
            texto.starts_with(r"\\.\pipe\tirech-omni-mcp-ana-"),
            "{texto}"
        );
        assert_eq!(texto.len(), r"\\.\pipe\tirech-omni-mcp-ana-".len() + 16);
        assert_ne!(pipe, endereco_em(true, false, None, outra, "ana", true));
        // Fora do Windows, o socket fica na pasta mesmo com o XDG.
        let app = Path::new("/media/ana/pendrive/tirech-omnidata");
        assert_eq!(
            endereco_em(false, false, Some(xdg), app, "ana", true),
            app.join("mcp.sock")
        );
    }
}
