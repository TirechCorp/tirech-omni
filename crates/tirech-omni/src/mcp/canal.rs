//! O canal local entre a ponte (`tirech-omni mcp`) e o app: linhas JSON nos dois sentidos, uma
//! mensagem por linha. Este módulo tem o que os dois lados dividem: a versão, o formato da
//! abertura e da resposta a ela, e a conexão com o endereço.
//!
//! - **Abertura:** a ponte manda [`Abertura`]; o app responde [`RespostaDaAbertura`].
//! - **Pedido:** `{"id": …, "ferramenta": "…", "args": {…}}`.
//! - **Resposta:** `{"id": …, "resultado": <result do MCP>}`, ou `{"id": …, "erro": {"code":
//!   -32602, "message": "…"}}` para argumentos que não seguem o esquema.
//! - **Cancelamento:** `{"cancelar": <id>}`.
//! - **Avisos:** o hook do Claude Code (`tirech-omni mcp --avisos <lançamento>`) liga, manda
//!   [`PedidoDeAvisos`] no lugar da abertura, lê [`RespostaDosAvisos`] e sai. Um app antigo o
//!   recusa como ponte de outra versão.

#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A versão do canal. O app recusa uma ponte de outra.
pub const VERSAO: u64 = 1;

/// A conexão com o app: um socket Unix, ou o pipe com nome no Windows.
#[cfg(unix)]
pub type Fluxo = std::os::unix::net::UnixStream;

/// A conexão com o app: o pipe com nome (ver [`super::pipe`]).
#[cfg(windows)]
pub use super::pipe::Fluxo;

/// Liga no app que atende `endereco`.
#[cfg(unix)]
pub fn conectar(endereco: &Path) -> io::Result<Fluxo> {
    std::os::unix::net::UnixStream::connect(endereco)
}

/// Liga no app que atende `endereco`.
#[cfg(windows)]
pub use super::pipe::conectar;

/// A primeira linha que a ponte manda.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Abertura {
    /// A versão do canal ([`VERSAO`]).
    pub ponte: u64,
    /// O pid da ponte. No Unix é só informativo; no Windows o app o confere com o que o sistema
    /// diz do outro lado do pipe.
    pub pid: u32,
    /// O espaço de trabalho a que a ponte está presa (`--espaco`).
    pub espaco: Option<String>,
    /// O nome pedido para a sessão (`--nome`).
    pub nome: Option<String>,
    /// A chave da aba que o assistente controla (`--aba`, M63). O app faz da sessão a dona
    /// dela, libera só ela e a usa quando uma ferramenta não diz a aba.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aba: Option<String>,
    /// O lançamento do "Controlar com" que abriu o assistente (`--lancamento`): o hook dos avisos
    /// acha a sessão por ele.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lancamento: Option<String>,
}

/// O pedido do hook: os avisos pendentes da sessão do lançamento `avisos`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedidoDeAvisos {
    /// O lançamento (o mesmo `--lancamento` da ponte).
    pub avisos: String,
    /// A versão do canal ([`VERSAO`]).
    pub versao: u64,
}

/// A resposta do app ao [`PedidoDeAvisos`]: o texto para o assistente, ou nada.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RespostaDosAvisos {
    /// Os avisos, já redigidos; `None` quando não há.
    #[serde(default)]
    pub texto: Option<String>,
}

/// A resposta do app à [`Abertura`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RespostaDaAbertura {
    /// Se o app aceitou a ponte.
    pub ok: bool,
    /// O nome dado à sessão (com `ok`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessao: Option<String>,
    /// Por que não aceitou (sem `ok`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub erro: Option<String>,
}

impl RespostaDaAbertura {
    /// O app aceitou, dando `sessao` à ponte.
    pub fn aceita(sessao: String) -> Self {
        Self {
            ok: true,
            sessao: Some(sessao),
            erro: None,
        }
    }

    /// O app recusou, dizendo o motivo.
    pub fn recusada(erro: String) -> Self {
        Self {
            ok: false,
            sessao: None,
            erro: Some(erro),
        }
    }
}
