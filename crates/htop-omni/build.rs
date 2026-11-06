//! No Windows, embute o ícone e a versão no `htop-omni.exe` (o mesmo ícone do Tirech Omni). Nos
//! outros sistemas não faz nada.
//!
//! Se o compilador de recursos não for achado, a compilação segue sem o ícone e avisa.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../tirech-omni/assets/icone.ico");
    // `cfg!(windows)` seria o sistema de quem compila; o que vale é o do executável.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let versao = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut recursos = winresource::WindowsResource::new();
    recursos
        .set_icon("../tirech-omni/assets/icone.ico")
        .set("FileDescription", "htop-omni")
        .set("ProductName", "htop-omni")
        .set("ProductVersion", &versao)
        .set("FileVersion", &versao)
        .set("OriginalFilename", "htop-omni.exe");
    if let Err(erro) = recursos.compile() {
        println!("cargo:warning=ícone do .exe não embutido: {erro}");
    }
}
