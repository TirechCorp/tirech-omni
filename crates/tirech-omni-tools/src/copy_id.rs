//! `tirech-omni-copy-id`: instala uma chave pública no `authorized_keys` de um servidor.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ssh_key::PublicKey;
use tomni_ui_terminal::ssh::keys::{self, KeyFile};
use tomni_ui_terminal::ssh::{CopyIdOptions, CopyIdOutcome, KnownHost, Ssh, SshBuilder, copy_id};

use crate::getopt::getopt;
use crate::{DEFAULT_KEYS, display_path, prompt, usage_error};

const PROGRAM: &str = "tirech-omni-copy-id";

/// A ordem em que o OpenSSH procura a chave pública quando não se diz qual.
const PUBLIC_ORDER: [&str; 3] = ["id_rsa.pub", "id_ecdsa.pub", "id_ed25519.pub"];

const HELP: &str = "\
tirech-omni-copy-id: instala uma chave pública no authorized_keys do servidor

Uso:
  tirech-omni-copy-id [-i arquivo.pub] [-p porta] [-J salto[,salto...]] [-f] [-n]
                     [-o IdentityFile=arquivo] [-o StrictHostKeyChecking=ask|accept-new|no|yes]
                     [-o UserKnownHostsFile=arquivo] [usuário@]servidor

Sem -i, usa a primeira .pub de ~/.ssh (id_rsa, id_ecdsa, id_ed25519). A autenticação tenta o
agente (SSH_AUTH_SOCK), as chaves de ~/.ssh (ou as de IdentityFile) e por fim a senha, pedida
no terminal. -f instala mesmo que a chave já esteja lá; -n só mostra o que seria feito. Um
salto é [usuário@]servidor[:porta]. Funciona em servidores Unix e no OpenSSH do Windows.
";

/// Como a chave de um servidor desconhecido é tratada (`StrictHostKeyChecking`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Strict {
    /// Pergunta (o padrão).
    Ask,
    /// Aceita e grava a chave nova (`accept-new` e `no`).
    Accept,
    /// Recusa a chave nova (`yes`).
    Refuse,
}

/// Um servidor: `[usuário@]host[:porta]`.
struct Target {
    user: String,
    host: String,
    port: Option<u16>,
}

fn parse_target(text: &str) -> Result<Target, String> {
    let (user, rest) = match text.split_once('@') {
        Some((user, rest)) if !user.is_empty() => (user.to_owned(), rest),
        Some(_) => return Err(format!("servidor inválido: {text}")),
        None => (default_user(), text),
    };
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => (
            host,
            Some(
                port.parse::<u16>()
                    .map_err(|_| format!("porta inválida: {port}"))?,
            ),
        ),
        _ => (rest, None),
    };
    if host.is_empty() {
        return Err(format!("servidor inválido: {text}"));
    }
    Ok(Target {
        user,
        host: host.trim_matches(['[', ']']).to_owned(),
        port,
    })
}

fn default_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// Roda o programa com `args` (sem o nome dele) e devolve o código de saída.
pub fn run(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let parsed = match getopt(args, "i:p:J:o:fn") {
        Ok(parsed) => parsed,
        Err(message) => return usage_error(PROGRAM, &message),
    };
    let [server] = parsed.rest.as_slice() else {
        return usage_error(PROGRAM, "informe o servidor: [usuário@]servidor");
    };
    let setup = (|| {
        let mut target = parse_target(server)?;
        if let Some(port) = parsed.value('p') {
            target.port = Some(
                port.parse::<u16>()
                    .map_err(|_| format!("porta inválida: {port}"))?,
            );
        }
        let jumps = parsed
            .value('J')
            .map(|list| {
                list.split(',')
                    .map(parse_target)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        let options = Options::from(&parsed.values('o'))?;
        Ok::<_, String>((target, jumps, options))
    })();
    let (target, jumps, options) = match setup {
        Ok(setup) => setup,
        Err(message) => return usage_error(PROGRAM, &message),
    };

    let copy = CopyIdOptions {
        force: parsed.has('f'),
        dry_run: parsed.has('n'),
    };
    match install(
        parsed.value('i').map(Path::new),
        &target,
        &jumps,
        &options,
        copy,
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{PROGRAM}: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Os `-o chave=valor` que o programa entende.
struct Options {
    identities: Vec<PathBuf>,
    strict: Strict,
    known_hosts: Option<PathBuf>,
}

impl Options {
    fn from(values: &[&str]) -> Result<Self, String> {
        let mut options = Options {
            identities: Vec::new(),
            strict: Strict::Ask,
            known_hosts: None,
        };
        for value in values {
            let (name, setting) = value
                .split_once('=')
                .or_else(|| value.split_once(' '))
                .ok_or_else(|| format!("opção -o inválida: {value}"))?;
            let setting = setting.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "identityfile" => options.identities.push(expand(setting)),
                "userknownhostsfile" => options.known_hosts = Some(expand(setting)),
                "stricthostkeychecking" => {
                    options.strict = match setting.to_ascii_lowercase().as_str() {
                        "ask" => Strict::Ask,
                        "accept-new" | "no" | "off" => Strict::Accept,
                        "yes" | "on" => Strict::Refuse,
                        other => return Err(format!("StrictHostKeyChecking inválido: {other}")),
                    };
                }
                other => return Err(format!("a opção -o {other} não é suportada")),
            }
        }
        Ok(options)
    }
}

/// `~/…` vira o caminho dentro da pasta pessoal.
fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => std::env::home_dir().unwrap_or_default().join(rest),
        None => PathBuf::from(path),
    }
}

/// A chave pública a instalar: a de `-i` (ou a `.pub` ao lado de uma privada) ou a primeira
/// `.pub` padrão de `~/.ssh`.
fn read_public(given: Option<&Path>) -> Result<(PublicKey, PathBuf), String> {
    let path = match given {
        Some(path) if path.extension().is_some_and(|ext| ext == "pub") => path.to_owned(),
        Some(path) => keys::public_path(path),
        None => {
            let dir = crate::ssh_dir();
            PUBLIC_ORDER
                .iter()
                .map(|name| dir.join(name))
                .find(|path| path.exists())
                .ok_or("não há nenhuma chave pública em ~/.ssh; gere uma ou use -i")?
        }
    };
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", display_path(&path)))?;
    let public = PublicKey::from_openssh(text.trim())
        .map_err(|_| format!("{} não é uma chave pública", display_path(&path)))?;
    Ok((public, path))
}

/// Um servidor da cadeia, sem a senha: agente e chaves, e a conferência da chave do servidor.
fn builder(
    target: &Target,
    options: &Options,
    identities: &[(PathBuf, Option<String>)],
) -> SshBuilder {
    let mut builder =
        Ssh::builder(&target.host, target.port.unwrap_or(22), &target.user).agent_auth();
    for (path, passphrase) in identities {
        builder = builder.key_file(path, passphrase.as_deref());
    }
    if let Some(known_hosts) = &options.known_hosts {
        builder = builder.known_hosts(known_hosts);
    }
    let strict = options.strict;
    builder.check_host_key(move |key, known| match known {
        KnownHost::Known => true,
        KnownHost::Changed { line } => {
            eprintln!(
                "{PROGRAM}: ATENÇÃO: a chave de {}:{} mudou (linha {line} do known_hosts). \
Pode ser um ataque; se o servidor foi reinstalado, apague a linha e tente de novo.",
                key.host(),
                key.port()
            );
            false
        }
        KnownHost::Unknown => {
            let accept = match strict {
                Strict::Accept => true,
                Strict::Refuse => false,
                Strict::Ask => {
                    eprintln!(
                        "A autenticidade de {}:{} não pode ser confirmada.\n{} {}",
                        key.host(),
                        key.port(),
                        key.algorithm(),
                        key.fingerprint()
                    );
                    prompt::confirm("Continuar conectando (sim/não)? ").unwrap_or(false)
                }
            };
            accept && key.remember().is_ok()
        }
    })
}

/// As chaves de arquivo para autenticar: as de `-o IdentityFile` ou as padrão de `~/.ssh`,
/// as que não pedem passphrase (as outras vêm do agente, ou depois de a senha falhar).
fn identities(options: &Options) -> (Vec<(PathBuf, Option<String>)>, Vec<PathBuf>) {
    let candidates: Vec<PathBuf> = if options.identities.is_empty() {
        let dir = crate::ssh_dir();
        DEFAULT_KEYS.iter().map(|name| dir.join(name)).collect()
    } else {
        options.identities.clone()
    };
    let (mut open, mut encrypted) = (Vec::new(), Vec::new());
    for path in candidates.into_iter().filter(|path| path.exists()) {
        match KeyFile::inspect(&path) {
            Ok(info) if info.encrypted => encrypted.push(path),
            Ok(_) => open.push((path, None)),
            Err(_) => {}
        }
    }
    (open, encrypted)
}

fn chain(
    target: &Target,
    jumps: &[Target],
    options: &Options,
    identities: &[(PathBuf, Option<String>)],
    password: Option<&str>,
) -> SshBuilder {
    let mut via: Option<SshBuilder> = None;
    for jump in jumps {
        let mut hop = builder(jump, options, identities);
        if let Some(previous) = via.take() {
            hop = hop.jump(previous);
        }
        via = Some(hop);
    }
    let mut destination = builder(target, options, identities);
    if let Some(password) = password {
        destination = destination.password(password);
    }
    if let Some(via) = via {
        destination = destination.jump(via);
    }
    destination
}

fn install(
    given: Option<&Path>,
    target: &Target,
    jumps: &[Target],
    options: &Options,
    copy: CopyIdOptions,
) -> Result<(), String> {
    let (public, public_path) = read_public(given)?;
    eprintln!(
        "Chave a instalar: {} ({})",
        display_path(&public_path),
        keys::fingerprint(&public)
    );

    let (mut open_keys, encrypted) = identities(options);
    let first = copy_id(
        &chain(target, jumps, options, &open_keys, None),
        &public,
        copy,
    );
    let outcome = match first {
        // A autenticação sem a senha não bastou: pede as passphrases e a senha, como o `ssh`.
        Err(message) if message.contains("autenticação recusada") => {
            for path in encrypted {
                let question =
                    format!("Passphrase de {} (Enter para pular): ", display_path(&path));
                let passphrase = prompt::secret(&question).map_err(|e| e.to_string())?;
                if !passphrase.is_empty() {
                    open_keys.push((path, Some(passphrase)));
                }
            }
            let question = format!("Senha de {}@{}: ", target.user, target.host);
            let password = prompt::secret(&question).map_err(|e| e.to_string())?;
            copy_id(
                &chain(target, jumps, options, &open_keys, Some(&password)),
                &public,
                copy,
            )?
        }
        other => other?,
    };
    report(&outcome, target, &public);
    Ok(())
}

fn report(outcome: &CopyIdOutcome, target: &Target, public: &PublicKey) {
    match outcome {
        CopyIdOutcome::Installed { path } => {
            eprintln!("Número de chaves adicionadas: 1 (em {path})");
            let port = target.port.map_or(String::new(), |p| format!(" -p {p}"));
            eprintln!(
                "\nAgora tente entrar na máquina com: \"ssh{port} {}@{}\"\n\
e confira se só a(s) chave(s) esperada(s) foram adicionadas.",
                target.user, target.host
            );
        }
        CopyIdOutcome::AlreadyThere => {
            eprintln!("Todas as chaves já estão instaladas no servidor; nada foi feito.");
        }
        CopyIdOutcome::WouldInstall { path } => {
            eprintln!(
                "Seria instalada a chave {} em {path} (modo -n: nada foi mudado).",
                keys::fingerprint(public)
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servidor_com_usuario_e_porta() {
        let alvo = parse_target("ana@exemplo.com:2222").unwrap();
        assert_eq!(
            (alvo.user.as_str(), alvo.host.as_str(), alvo.port),
            ("ana", "exemplo.com", Some(2222))
        );
        let alvo = parse_target("ana@exemplo.com").unwrap();
        assert_eq!(alvo.port, None);
        let alvo = parse_target("ana@::1").unwrap();
        assert_eq!((alvo.host.as_str(), alvo.port), ("::1", None));
        assert!(parse_target("ana@").is_err());
        assert!(parse_target("@host").is_err());
        assert!(parse_target("ana@host:porta").is_err());
    }

    #[test]
    fn opcoes_o() {
        let opcoes =
            Options::from(&["IdentityFile=/tmp/chave", "StrictHostKeyChecking=no"]).unwrap();
        assert_eq!(opcoes.identities, [PathBuf::from("/tmp/chave")]);
        assert!(opcoes.strict == Strict::Accept);
        assert!(Options::from(&["Compression=yes"]).is_err());
        assert!(Options::from(&["semvalor"]).is_err());
    }
}
