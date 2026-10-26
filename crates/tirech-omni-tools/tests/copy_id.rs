//! O `tirech-omni-copy-id` contra um servidor russh local, que roda os comandos num `sh` de
//! verdade (sem rede de fora nem OpenSSH).

mod comum;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use comum::{Casa, gerar, rodar, texto};
use russh::keys::{PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId};

const COPY_ID: &str = env!("CARGO_BIN_EXE_tirech-omni-copy-id");

/// Aceita a chave `cliente` para "ana" e roda os comandos `exec` num `sh` com `HOME` em `home`.
struct Servidor {
    cliente: PublicKey,
    home: PathBuf,
    comandos: std::collections::HashMap<ChannelId, (String, Vec<u8>)>,
}

impl server::Handler for Servidor {
    type Error = russh::Error;

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(
            if user == "ana" && key.key_data() == self.cliente.key_data() {
                Auth::Accept
            } else {
                Auth::reject()
            },
        )
    }

    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        self.comandos.insert(
            channel,
            (String::from_utf8_lossy(data).into_owned(), Vec::new()),
        );
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some((_, entrada)) = self.comandos.get_mut(&channel) {
            entrada.extend_from_slice(data);
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some((comando, entrada)) = self.comandos.remove(&channel) else {
            return Ok(());
        };
        let mut filho = std::process::Command::new("sh")
            .args(["-c", &comando])
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = filho.stdin.take().unwrap();
        let _ = stdin.write_all(&entrada);
        drop(stdin);
        let saida = filho.wait_with_output().unwrap();
        if !saida.stdout.is_empty() {
            session.data(channel, saida.stdout)?;
        }
        if !saida.stderr.is_empty() {
            session.extended_data(channel, 1, saida.stderr)?;
        }
        session.exit_status_request(channel, saida.status.code().unwrap_or(255) as u32)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}

/// O servidor no ar: a porta e a pasta pessoal dele.
struct Alvo {
    porta: u16,
    home: PathBuf,
}

fn subir_servidor(casa: &Casa, cliente: &Path, hospedeira: &Path) -> Alvo {
    let home = casa.pasta.join("servidor");
    std::fs::create_dir_all(&home).unwrap();
    let cliente = PublicKey::from_openssh(&std::fs::read_to_string(cliente).unwrap()).unwrap();
    let chave = PrivateKey::from_openssh(std::fs::read_to_string(hospedeira).unwrap()).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = listener.local_addr().unwrap().port();
    let pasta = home.clone();
    thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let handler = Servidor {
                cliente: cliente.clone(),
                home: pasta.clone(),
                comandos: Default::default(),
            };
            let chave = chave.clone();
            thread::spawn(move || {
                stream.set_nonblocking(true).unwrap();
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime.block_on(async move {
                    let config = Arc::new(server::Config {
                        keys: vec![chave],
                        auth_rejection_time: Duration::ZERO,
                        auth_rejection_time_initial: Some(Duration::ZERO),
                        ..Default::default()
                    });
                    let stream = tokio::net::TcpStream::from_std(stream).unwrap();
                    if let Ok(sessao) = server::run_stream(config, stream, handler).await {
                        let _ = sessao.await;
                    }
                });
            });
        }
    });
    Alvo { porta, home }
}

/// Monta o cenário: a chave de login do cliente, a chave a instalar e o servidor.
struct Cenario {
    casa: Casa,
    alvo: Alvo,
    login: PathBuf,
    instalar: PathBuf,
}

impl Cenario {
    fn novo(nome: &str) -> Self {
        let casa = Casa::nova(nome);
        let login = casa.pasta.join("login");
        let instalar = casa.pasta.join("nova");
        let hospedeira = casa.pasta.join("hospedeira");
        gerar(&casa, &login, "ana@login");
        gerar(&casa, &instalar, "ana@nova");
        gerar(&casa, &hospedeira, "servidor");
        let alvo = subir_servidor(&casa, &login.with_extension("pub"), &hospedeira);
        Self {
            casa,
            alvo,
            login,
            instalar,
        }
    }

    fn rodar(&self, extras: &[&str]) -> std::process::Output {
        let mut comando = self.casa.comando(COPY_ID);
        comando
            .arg("-i")
            .arg(self.instalar.with_extension("pub"))
            .arg("-p")
            .arg(self.alvo.porta.to_string())
            .arg("-o")
            .arg(format!("IdentityFile={}", self.login.display()))
            .arg("-o")
            .arg(format!(
                "UserKnownHostsFile={}",
                self.casa.pasta.join("known_hosts").display()
            ))
            .args(["-o", "StrictHostKeyChecking=accept-new"])
            .args(extras)
            .arg("ana@127.0.0.1");
        rodar(comando)
    }

    fn authorized_keys(&self) -> String {
        std::fs::read_to_string(self.alvo.home.join(".ssh/authorized_keys")).unwrap_or_default()
    }
}

#[test]
fn instala_a_chave_e_nao_repete() {
    let cenario = Cenario::novo("copy-id-instala");
    let publica = std::fs::read_to_string(cenario.instalar.with_extension("pub")).unwrap();
    let corpo = publica.split_whitespace().nth(1).unwrap().to_owned();

    let saida = cenario.rodar(&[]);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let erro = texto(&saida.stderr);
    assert!(erro.contains("Número de chaves adicionadas: 1"), "{erro}");
    assert!(erro.contains("ssh -p"), "{erro}");
    let arquivo = cenario.authorized_keys();
    assert_eq!(arquivo.matches(&corpo).count(), 1, "{arquivo}");

    // De novo: já está lá, nada muda.
    let saida = cenario.rodar(&[]);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    assert!(texto(&saida.stderr).contains("já estão instaladas"));
    assert_eq!(cenario.authorized_keys().matches(&corpo).count(), 1);

    // Com -f, instala mesmo assim.
    let saida = cenario.rodar(&["-f"]);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    assert_eq!(cenario.authorized_keys().matches(&corpo).count(), 2);
}

#[test]
fn com_n_nao_muda_nada() {
    let cenario = Cenario::novo("copy-id-n");
    let saida = cenario.rodar(&["-n"]);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    assert!(texto(&saida.stderr).contains("modo -n"));
    assert_eq!(cenario.authorized_keys(), "");
}

#[test]
fn recusa_servidor_desconhecido_com_strict_yes() {
    let cenario = Cenario::novo("copy-id-strict");
    let saida = cenario.rodar(&["-o", "StrictHostKeyChecking=yes"]);
    assert!(!saida.status.success());
    assert_eq!(cenario.authorized_keys(), "");
}

#[test]
fn pergunta_a_chave_do_servidor_e_aceita_com_sim() {
    let cenario = Cenario::novo("copy-id-pergunta");
    let mut comando = cenario.casa.comando(COPY_ID);
    comando
        .arg("-i")
        .arg(cenario.instalar.with_extension("pub"))
        .arg("-p")
        .arg(cenario.alvo.porta.to_string())
        .arg("-o")
        .arg(format!("IdentityFile={}", cenario.login.display()))
        .arg("-o")
        .arg(format!(
            "UserKnownHostsFile={}",
            cenario.casa.pasta.join("known_hosts").display()
        ))
        .arg("ana@127.0.0.1")
        .stdin(Stdio::piped());
    let mut filho = comando.spawn().unwrap();
    filho.stdin.take().unwrap().write_all(b"sim\n").unwrap();
    let saida = filho.wait_with_output().unwrap();
    let erro = texto(&saida.stderr);
    assert!(saida.status.success(), "{erro}");
    assert!(erro.contains("não pode ser confirmada"), "{erro}");
    assert!(erro.contains("SHA256:"), "{erro}");
    assert!(!cenario.authorized_keys().is_empty());
}

#[test]
fn sem_o_servidor_sai_com_2() {
    let casa = Casa::nova("copy-id-uso");
    let mut comando = casa.comando(COPY_ID);
    comando.arg("-i").arg("x.pub");
    assert_eq!(rodar(comando).status.code(), Some(2));
}
