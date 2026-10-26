//! Reiniciar o app: usado pela troca de idioma e pela troca da pasta de dados (Configurações).
//!
//! `pedir` marca o pedido e fecha a janela principal; quando o `App::run` volta, o `main.rs`
//! olha `pedido` e sobe o app de novo, esperando este processo acabar.

use std::sync::atomic::{AtomicBool, Ordering};

use tomni_ui::WindowHandle;

static PEDIDO: AtomicBool = AtomicBool::new(false);

/// Pede o reinício e fecha a janela principal.
pub fn pedir() {
    PEDIDO.store(true, Ordering::SeqCst);
    WindowHandle::main().close();
}

/// Se alguém pediu o reinício.
pub fn pedido() -> bool {
    PEDIDO.load(Ordering::SeqCst)
}

/// O argumento com que o app novo sobe no reinício: ele espera a trava da pasta que o anterior
/// ainda segura por um instante (no Windows, o novo sobe antes de o anterior acabar).
pub const ARGUMENTO_REINICIADO: &str = "--reiniciado";

/// Se este app foi aberto pelo reinício de outro.
pub fn reiniciado() -> bool {
    std::env::args().skip(1).any(|a| a == ARGUMENTO_REINICIADO)
}

/// Sobe outro app e deixa o atual acabar: no Unix, o novo espera este processo sair.
pub fn subir_de_novo() -> std::io::Result<()> {
    subir(&std::env::current_exe()?)
}

/// Sobe o executável `exe` (por exemplo a cópia instalada) e deixa o atual acabar: no Unix, o
/// novo espera este processo sair.
pub fn subir(exe: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::process::Command::new("sh")
            .arg("-c")
            .arg("while kill -0 \"$1\" 2>/dev/null; do sleep 0.2; done; exec \"$0\" \"$2\"")
            .arg(exe)
            .arg(std::process::id().to_string())
            .arg(ARGUMENTO_REINICIADO)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(unix))]
    {
        std::process::Command::new(exe)
            .arg(ARGUMENTO_REINICIADO)
            .spawn()
            .map(|_| ())
    }
}
