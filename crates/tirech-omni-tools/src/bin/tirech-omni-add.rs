//! O programa `tirech-omni-add`; o código está em `tirech_omni_tools::add`.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    tirech_omni_tools::add::run(&args)
}
