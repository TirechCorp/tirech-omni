//! `tirech-omni-add`: adiciona, lista e remove as chaves de um agente.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ssh_key::PublicKey;
use tomni_ui_terminal::ssh::agent::{AgentConnection, Constraints};
use tomni_ui_terminal::ssh::keys::{self, KeyError, KeyFile};

use crate::getopt::getopt;
use crate::{DEFAULT_KEYS, display_path, prompt, tempo, usage_error};

const PROGRAM: &str = "tirech-omni-add";

/// Quantas vezes a passphrase de uma chave é pedida antes de desistir dela.
const ATTEMPTS: usize = 3;

const HELP: &str = "\
tirech-omni-add: adiciona e remove chaves de um agente (SSH_AUTH_SOCK)

Uso:
  tirech-omni-add [-t vida] [-c] [arquivo...]   adiciona (sem arquivo: ~/.ssh/id_ed25519,
                                               id_ecdsa e id_rsa, os que existirem)
  tirech-omni-add -l                            lista as chaves (tipo, impressão digital)
  tirech-omni-add -L                            lista as chaves públicas inteiras
  tirech-omni-add -d [arquivo...]               tira as chaves dos arquivos
  tirech-omni-add -D                            tira todas as chaves
  tirech-omni-add -x | -X                       trava | destrava o agente com uma senha

-t é a validade da chave (30, 10m, 1h, 1d) e -c pede confirmação a cada uso. A passphrase
da chave é pedida no terminal, sem eco. Saída: 0 ok, 1 erro (ou agente sem chaves no -l),
2 uso errado ou agente inalcançável.
";

/// Roda o programa com `args` (sem o nome dele) e devolve o código de saída.
pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let parsed = match getopt(args, "lLdDt:cxX") {
        Ok(parsed) => parsed,
        Err(message) => return usage_error(PROGRAM, &message),
    };
    let lifetime = match parsed.value('t').map(tempo::parse).transpose() {
        Ok(lifetime) => lifetime,
        Err(message) => return usage_error(PROGRAM, &message),
    };
    let mut agent = match AgentConnection::from_env() {
        Ok(agent) => agent,
        Err(error) => {
            eprintln!("{PROGRAM}: não foi possível conectar ao agente: {error}");
            return ExitCode::from(2);
        }
    };
    let result = if parsed.has('l') || parsed.has('L') {
        list(&mut agent, parsed.has('L'))
    } else if parsed.has('D') {
        remove_all(&mut agent)
    } else if parsed.has('d') {
        remove(&mut agent, &parsed.rest)
    } else if parsed.has('x') {
        lock(&mut agent)
    } else if parsed.has('X') {
        unlock(&mut agent)
    } else {
        let constraints = Constraints {
            lifetime,
            confirm: parsed.has('c'),
        };
        add(&mut agent, &parsed.rest, constraints)
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{PROGRAM}: {message}");
            ExitCode::FAILURE
        }
    }
}

fn agent_error(error: std::io::Error) -> String {
    error.to_string()
}

/// Os arquivos dados, ou os padrões de `~/.ssh` que existirem.
fn files(given: &[String], only_existing: bool) -> Vec<PathBuf> {
    if !given.is_empty() {
        return given.iter().map(PathBuf::from).collect();
    }
    let dir = crate::ssh_dir();
    DEFAULT_KEYS
        .iter()
        .map(|name| dir.join(name))
        .filter(|path| !only_existing || path.exists())
        .collect()
}

fn list(agent: &mut AgentConnection, full: bool) -> Result<ExitCode, String> {
    let keys = agent.list().map_err(agent_error)?;
    if keys.is_empty() {
        eprintln!("O agente não tem chaves.");
        return Ok(ExitCode::FAILURE);
    }
    for (mut public, comment) in keys {
        if full {
            public.set_comment(comment.as_str());
            println!("{}", keys::public_openssh(&public));
        } else {
            let (kind, bits) = keys::kind_and_bits(&public);
            println!("{bits} {} {comment} ({kind})", keys::fingerprint(&public));
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn add(
    agent: &mut AgentConnection,
    given: &[String],
    constraints: Constraints,
) -> Result<ExitCode, String> {
    let paths = files(given, true);
    if paths.is_empty() {
        return Err("não há chaves em ~/.ssh; informe o arquivo".to_owned());
    }
    let mut failed = false;
    for path in paths {
        match open(&path) {
            Ok(file) => {
                let key = file.private_key();
                let comment = match key.comment().to_string() {
                    comment if comment.is_empty() => path.display().to_string(),
                    comment => comment,
                };
                agent.add(key, &comment, constraints).map_err(agent_error)?;
                eprintln!("Identidade adicionada: {} ({comment})", display_path(&path));
                if let Some(lifetime) = constraints.lifetime {
                    eprintln!("Validade de {} segundos.", lifetime.as_secs());
                }
                if constraints.confirm {
                    eprintln!("O uso da chave precisa de confirmação.");
                }
            }
            Err(message) => {
                eprintln!("{PROGRAM}: {}: {message}", display_path(&path));
                failed = true;
            }
        }
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// Abre a chave, pedindo a passphrase (até [`ATTEMPTS`] vezes) se ela for cifrada.
fn open(path: &Path) -> Result<KeyFile, String> {
    let info = KeyFile::inspect(path).map_err(|e| e.to_string())?;
    if !info.encrypted {
        return KeyFile::open(path, None).map_err(|e| e.to_string());
    }
    for attempt in 1..=ATTEMPTS {
        let passphrase = prompt::secret(&format!("Passphrase de {}: ", display_path(path)))
            .map_err(|e| e.to_string())?;
        match KeyFile::open(path, Some(&passphrase)) {
            Ok(file) => return Ok(file),
            Err(KeyError::WrongPassphrase) if attempt < ATTEMPTS => {
                eprintln!("A passphrase não confere; tente de novo.");
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err(KeyError::WrongPassphrase.to_string())
}

fn remove_all(agent: &mut AgentConnection) -> Result<ExitCode, String> {
    agent.remove_all().map_err(agent_error)?;
    eprintln!("Todas as identidades foram removidas.");
    Ok(ExitCode::SUCCESS)
}

/// A chave pública do arquivo (`arquivo.pub`, ou a que está dentro do arquivo privado).
fn public_of(path: &Path) -> Result<(PublicKey, String), String> {
    let pub_path = keys::public_path(path);
    let candidate = if path.extension().is_some_and(|ext| ext == "pub") {
        path.to_owned()
    } else if pub_path.exists() {
        pub_path
    } else {
        let info = KeyFile::inspect(path).map_err(|e| e.to_string())?;
        let public = info
            .public
            .ok_or_else(|| "a chave pública não está no arquivo".to_owned())?;
        return Ok((public, info.comment));
    };
    let text = std::fs::read_to_string(&candidate).map_err(|e| e.to_string())?;
    let public = PublicKey::from_openssh(text.trim()).map_err(|e| e.to_string())?;
    let comment = public.comment().to_string();
    Ok((public, comment))
}

fn remove(agent: &mut AgentConnection, given: &[String]) -> Result<ExitCode, String> {
    let paths = files(given, true);
    if paths.is_empty() {
        return Err("não há chaves em ~/.ssh; informe o arquivo".to_owned());
    }
    let mut failed = false;
    for path in paths {
        let outcome = public_of(&path).and_then(|(public, comment)| {
            agent.remove(&public).map_err(agent_error)?;
            Ok(comment)
        });
        match outcome {
            Ok(comment) => eprintln!("Identidade removida: {} ({comment})", display_path(&path)),
            Err(message) => {
                eprintln!("{PROGRAM}: {}: {message}", display_path(&path));
                failed = true;
            }
        }
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn lock(agent: &mut AgentConnection) -> Result<ExitCode, String> {
    let password = prompt::secret("Senha para travar o agente: ").map_err(agent_error)?;
    let again = prompt::secret("Repita a senha: ").map_err(agent_error)?;
    if password != again {
        return Err("as senhas não são iguais".to_owned());
    }
    if password.is_empty() {
        return Err("a senha do agente não pode ser vazia".to_owned());
    }
    agent.lock(&password).map_err(agent_error)?;
    eprintln!("Agente travado.");
    Ok(ExitCode::SUCCESS)
}

fn unlock(agent: &mut AgentConnection) -> Result<ExitCode, String> {
    let password = prompt::secret("Senha para destravar o agente: ").map_err(agent_error)?;
    agent.unlock(&password).map_err(agent_error)?;
    eprintln!("Agente destravado.");
    Ok(ExitCode::SUCCESS)
}
