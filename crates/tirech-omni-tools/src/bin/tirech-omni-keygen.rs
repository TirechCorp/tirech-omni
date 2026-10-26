//! O programa `tirech-omni-keygen`; o código está em `tirech_omni_tools::keygen`.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    tirech_omni_tools::keygen::run(&args)
}
