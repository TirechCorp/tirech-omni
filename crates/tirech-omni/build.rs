//! No Windows, embute o ícone e a versão no `tirech-omni.exe` (M49): é o que o Explorador, a
//! barra de tarefas e o Alt+Tab mostram. Nos outros sistemas não faz nada.
//!
//! Se o compilador de recursos não for achado, a compilação segue sem o ícone e avisa: o app
//! abre igual, só o arquivo `.exe` fica com o ícone padrão.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icone.ico");
    // `cfg!(windows)` seria o sistema de quem compila; o que vale é o do executável.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let versao = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut recursos = winresource::WindowsResource::new();
    recursos
        .set_icon("assets/icone.ico")
        .set("FileDescription", "Tirech Omni")
        .set("ProductName", "Tirech Omni")
        .set("ProductVersion", &versao)
        .set("FileVersion", &versao)
        .set("OriginalFilename", "tirech-omni.exe");
    if let Err(erro) = recursos.compile() {
        println!("cargo:warning=ícone do .exe não embutido: {erro}");
    }
}
