//! `tirech-omni-keygen`: gera, inspeciona e troca a passphrase de chaves SSH.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ssh_key::PublicKey;
use tomni_ui_terminal::ssh::keys::{
    self, KdfLevel, KeyError, KeyFile, KeyGen, KeyType, default_comment,
};

use crate::getopt::{Parsed, getopt};
use crate::{display_path, prompt, usage_error};

const PROGRAM: &str = "tirech-omni-keygen";

const HELP: &str = "\
tirech-omni-keygen: gera e inspeciona chaves SSH (um subconjunto do ssh-keygen)

Uso:
  tirech-omni-keygen [-t ed25519|ecdsa|rsa] [-b bits] [-a rodadas] [-C comentário]
                    [-f arquivo] [-N passphrase] [-q]      gera um par de chaves
  tirech-omni-keygen -p -f arquivo [-P antiga] [-N nova] [-a rodadas] [-q]
                                                           troca a passphrase
  tirech-omni-keygen -y -f arquivo [-P passphrase]          imprime a chave pública
  tirech-omni-keygen -l [-v] -f arquivo                     imprime a impressão digital

O tipo padrão é ed25519. O RSA tem 3072 ou 4096 bits e o ECDSA 256, 384 ou 521; DSA e RSA
menor não são gerados. Sem -f e sem -N, o programa pergunta no terminal.
";

/// Roda o programa com `args` (sem o nome dele) e devolve o código de saída.
pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let parsed = match getopt(args, "t:b:a:C:f:N:P:pylvq") {
        Ok(parsed) => parsed,
        Err(message) => return usage_error(PROGRAM, &message),
    };
    if let Some(extra) = parsed.rest.first() {
        return usage_error(PROGRAM, &format!("argumento inesperado: {extra}"));
    }
    let result = if parsed.has('p') {
        change_passphrase(&parsed)
    } else if parsed.has('y') {
        print_public(&parsed)
    } else if parsed.has('l') {
        print_fingerprint(&parsed)
    } else {
        generate(&parsed)
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{PROGRAM}: {message}");
            ExitCode::FAILURE
        }
    }
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

/// O arquivo de `-f`, ou o que se responde à pergunta (padrão: `default`).
fn key_path(parsed: &Parsed, question: &str, default: &Path) -> Result<PathBuf, String> {
    if let Some(path) = parsed.value('f') {
        return Ok(PathBuf::from(path));
    }
    let answer =
        prompt::line(&format!("{question} ({}): ", display_path(default))).map_err(io_error)?;
    let answer = answer.trim();
    Ok(if answer.is_empty() {
        default.to_owned()
    } else {
        PathBuf::from(answer)
    })
}

fn kdf(parsed: &Parsed) -> Result<KdfLevel, String> {
    match parsed.value('a') {
        None => Ok(KdfLevel::Standard),
        Some(text) => match text.parse::<u32>() {
            Ok(rounds) if rounds >= 1 => Ok(KdfLevel::Custom(rounds)),
            _ => Err(format!("número de rodadas inválido: {text}")),
        },
    }
}

fn generate(parsed: &Parsed) -> Result<(), String> {
    let bits = parsed
        .value('b')
        .map(|text| {
            text.parse::<u32>()
                .map_err(|_| format!("tamanho inválido: {text}"))
        })
        .transpose()?;
    let kind = KeyType::parse(parsed.value('t').unwrap_or("ed25519"), bits)?;
    let quiet = parsed.has('q');
    let kdf = kdf(parsed)?;
    let default = crate::ssh_dir().join(kind.file_name());
    if !quiet {
        eprintln!("Gerando o par de chaves {}.", kind.label());
    }
    let path = key_path(parsed, "Arquivo onde salvar a chave", &default)?;

    let mut overwrite = false;
    for existing in [path.clone(), keys::public_path(&path)] {
        if existing.exists() {
            if parsed.has('f') && parsed.has('N') {
                // Sem ninguém para perguntar, como no OpenSSH: não sobrescreve.
                return Err(KeyError::Exists(existing).to_string());
            }
            let question = format!("{} já existe.\nSobrescrever (s/n)? ", existing.display());
            if !prompt::confirm(&question).map_err(io_error)? {
                return Err("nada foi gravado".to_owned());
            }
            overwrite = true;
            break;
        }
    }

    let passphrase = match parsed.value('N') {
        Some(passphrase) => passphrase.to_owned(),
        None => ask_new_passphrase()?,
    };
    let comment = parsed
        .value('C')
        .map_or_else(default_comment, str::to_owned);

    let mut generator = KeyGen::new(kind).comment(comment).kdf(kdf);
    if !passphrase.is_empty() {
        generator = generator.passphrase(passphrase);
    }
    let key = generator.generate().map_err(|e| e.to_string())?;
    key.write(&path, overwrite).map_err(|e| e.to_string())?;
    if !quiet {
        eprintln!("Sua chave privada foi salva em {}.", display_path(&path));
        eprintln!(
            "Sua chave pública foi salva em {}.",
            display_path(&keys::public_path(&path))
        );
        eprintln!("A impressão digital da chave é:");
        eprintln!("{} {}", key.fingerprint(), key.comment());
        eprintln!("A imagem randomart da chave é:");
        eprintln!("{}", key.randomart());
    }
    Ok(())
}

/// Pede a passphrase nova duas vezes (vazia é sem passphrase).
fn ask_new_passphrase() -> Result<String, String> {
    let first = prompt::secret("Passphrase (vazia para nenhuma): ").map_err(io_error)?;
    let second = prompt::secret("Repita a passphrase: ").map_err(io_error)?;
    if first != second {
        return Err("as passphrases não são iguais".to_owned());
    }
    Ok(first)
}

/// Abre a chave de `path`, pedindo a passphrase se ela é cifrada e `-P` não a deu.
fn open_key(path: &Path, given: Option<&str>) -> Result<KeyFile, String> {
    let info = KeyFile::inspect(path).map_err(|e| e.to_string())?;
    let passphrase = match given {
        Some(passphrase) => Some(passphrase.to_owned()),
        None if info.encrypted => Some(
            prompt::secret(&format!("Passphrase de {}: ", display_path(path))).map_err(io_error)?,
        ),
        None => None,
    };
    KeyFile::open(path, passphrase.as_deref()).map_err(|e| e.to_string())
}

fn change_passphrase(parsed: &Parsed) -> Result<(), String> {
    let default = crate::ssh_dir().join(KeyType::Ed25519.file_name());
    let path = key_path(parsed, "Arquivo da chave", &default)?;
    let kdf = kdf(parsed)?;
    let file = open_key(&path, parsed.value('P'))?;
    let new = match parsed.value('N') {
        Some(passphrase) => passphrase.to_owned(),
        None => ask_new_passphrase()?,
    };
    let new = (!new.is_empty()).then_some(new);
    let written = file
        .rewrite(&path, new.as_deref(), kdf)
        .map_err(|e| e.to_string())?;
    if !parsed.has('q') {
        eprintln!(
            "Sua identificação foi salva em {} com a nova passphrase.",
            display_path(&written)
        );
    }
    Ok(())
}

fn print_public(parsed: &Parsed) -> Result<(), String> {
    let default = crate::ssh_dir().join(KeyType::Ed25519.file_name());
    let path = key_path(parsed, "Arquivo da chave", &default)?;
    // A pública costuma estar no arquivo mesmo com passphrase: só se pede se faltar.
    let info = KeyFile::inspect(&path).map_err(|e| e.to_string())?;
    let public = match info.public {
        Some(public) => public,
        None => open_key(&path, parsed.value('P'))?
            .private_key()
            .public_key()
            .clone(),
    };
    println!("{}", with_comment(public, &info.comment));
    Ok(())
}

/// A linha `.pub` de `public`, com `comment` se a chave não tiver o dela.
fn with_comment(mut public: PublicKey, comment: &str) -> String {
    if public.comment().is_empty() {
        public.set_comment(comment);
    }
    keys::public_openssh(&public)
}

fn print_fingerprint(parsed: &Parsed) -> Result<(), String> {
    let default = crate::ssh_dir().join(KeyType::Ed25519.file_name());
    let path = key_path(parsed, "Arquivo da chave", &default)?;
    let (public, comment) = if path.extension().is_some_and(|ext| ext == "pub") {
        let text = std::fs::read_to_string(&path).map_err(io_error)?;
        let public = PublicKey::from_openssh(text.trim())
            .map_err(|e| KeyError::Invalid(e.to_string()).to_string())?;
        let comment = public.comment().to_string();
        (public, comment)
    } else {
        let info = KeyFile::inspect(&path).map_err(|e| e.to_string())?;
        let public = info.public.ok_or_else(|| {
            "a chave pública não está no arquivo; aponte para o .pub com -f".to_owned()
        })?;
        (public, info.comment)
    };
    let (kind, bits) = keys::kind_and_bits(&public);
    println!("{bits} {} {comment} ({kind})", keys::fingerprint(&public));
    if parsed.has('v') {
        println!("{}", keys::randomart(&public));
    }
    Ok(())
}
