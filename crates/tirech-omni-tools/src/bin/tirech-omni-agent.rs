//! O programa `tirech-omni-agent`; o código está em `tirech_omni_tools::agent`.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    tirech_omni_tools::agent::run(&args)
}
