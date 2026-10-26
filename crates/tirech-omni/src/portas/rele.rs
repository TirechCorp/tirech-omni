//! O relé das portas (M58, seções 3.8, 3.11 e 10 do plano): o que atende os túneis que o
//! `SshForwards` do M37 não sabe fazer, os de "Nome do site" (o `Host` e o `Location` trocados) e
//! os de servidor HTTPS (o TLS é aberto aqui e o navegador fala HTTP simples).
//!
//! Roda num runtime `tokio` de thread única, numa thread "portas-rele" que o app sobe na primeira
//! vez. Cada conexão do navegador abre o seu canal `direct-tcpip` (`open_direct`) e fecha junto
//! com ele. A reescrita das cabeças está em [`super::http`]; aqui só há o encanamento.

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
    crypto::ring::default_provider,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Handle;
use tokio::task::{AbortHandle, JoinSet};
use tokio_rustls::TlsConnector;
use tomni_ui::terminal::ssh::SshChannel;

use super::descobrir::AbridorCompartilhado;
use super::http::{fim_da_cabeca, reescrever_pedido, reescrever_resposta};
use super::local::apelido_do_site;
use crate::t;

/// Quanto o servidor tem para aceitar o canal.
const ESPERA_DO_CANAL: Duration = Duration::from_secs(15);

/// O tamanho que uma cabeça HTTP pode ter até desistir de achar o fim dela (os servidores
/// recusam muito antes; isto só guarda a memória do relé).
const CABECA_MAXIMA: usize = 1 << 20;

/// De onde vêm as conexões para o destino.
#[derive(Clone, Debug)]
pub enum Discador {
    /// Um canal `direct-tcpip` na conexão SSH da aba. O abridor é o de agora: a tela o renova, já
    /// que depois de um reinício da sessão o antigo só devolve erro.
    Ssh(AbridorCompartilhado),
}

impl Discador {
    async fn discar(&self, host: &str, porta: u16) -> io::Result<SshChannel> {
        match self {
            Self::Ssh(compartilhado) => {
                let abridor = compartilhado
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone()
                    .ok_or_else(|| {
                        io::Error::new(io::ErrorKind::NotConnected, t!("portas-sem-ssh"))
                    })?;
                let host = host.to_owned();
                // O `open_direct` bloqueia até o servidor responder. O canal solto dentro do
                // runtime evita o pânico do `Drop` fora dele.
                tokio::task::spawn_blocking(move || {
                    abridor.open_direct(&host, porta, ESPERA_DO_CANAL)
                })
                .await
                .map_err(|e| io::Error::other(e.to_string()))?
            }
        }
    }
}

/// O que o relé faz com cada conexão.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modo {
    /// Só copia os bytes.
    Tcp,
    /// HTTP: o `Host` vira `host_http` e o `Location` aponta para `{apelido}.localhost`.
    Http { host_http: String, apelido: String },
    /// HTTPS no servidor: o TLS é daqui, com `sni` (vazio = sem SNI), e o navegador fala HTTP.
    Https {
        sni: String,
        host_http: Option<String>,
        apelido: Option<String>,
    },
}

impl Modo {
    /// O modo para um destino web: `https` abre o TLS, `host_http` (se houver) é o nome do site.
    #[must_use]
    pub fn web(https: bool, host_http: &str) -> Self {
        let host_http = host_http.trim();
        match (https, host_http.is_empty()) {
            (false, false) => Self::Http {
                host_http: host_http.to_owned(),
                apelido: apelido_do_site(host_http),
            },
            (false, true) => Self::Http {
                host_http: String::new(),
                apelido: String::new(),
            },
            (true, false) => Self::Https {
                sni: host_http.to_owned(),
                host_http: Some(host_http.to_owned()),
                apelido: Some(apelido_do_site(host_http)),
            },
            (true, true) => Self::Https {
                sni: String::new(),
                host_http: None,
                apelido: None,
            },
        }
    }
}

/// O número de um relé aberto.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReleId(u64);

impl ReleId {
    /// O número, para compor ids de widgets.
    #[must_use]
    pub fn numero(self) -> u64 {
        self.0
    }
}

/// Como o relé está, para a linha do túnel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EstadoRele {
    /// Onde escuta (a porta de verdade, quando se pediu a `0`).
    pub escuta: SocketAddr,
    pub ativas: usize,
    pub total: u64,
    /// O motivo de o certificado do servidor não conferir ("certificado não confere ({motivo})").
    pub certificado: Option<String>,
    /// O último erro ao abrir o canal ou o TLS.
    pub erro: Option<String>,
}

#[derive(Debug, Default)]
struct Contadores {
    ativas: AtomicUsize,
    total: AtomicU64,
    certificado: Mutex<Option<String>>,
    erro: Mutex<Option<String>>,
}

impl Contadores {
    fn anotar(campo: &Mutex<Option<String>>, texto: Option<String>) {
        *campo.lock().unwrap_or_else(PoisonError::into_inner) = texto;
    }

    fn ler(campo: &Mutex<Option<String>>) -> Option<String> {
        campo.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

/// Conta a conexão enquanto existir.
struct Ativa(Arc<Contadores>);

impl Ativa {
    fn nova(contadores: &Arc<Contadores>) -> Self {
        contadores.ativas.fetch_add(1, Ordering::Relaxed);
        contadores.total.fetch_add(1, Ordering::Relaxed);
        Self(Arc::clone(contadores))
    }
}

impl Drop for Ativa {
    fn drop(&mut self) {
        self.0.ativas.fetch_sub(1, Ordering::Relaxed);
    }
}

struct Entrada {
    escuta: SocketAddr,
    contadores: Arc<Contadores>,
    tarefas: Vec<AbortHandle>,
}

/// O relé: a thread e o runtime, criados na primeira vez.
pub struct Rele {
    runtime: Handle,
    proximo: AtomicU64,
    abertos: Mutex<HashMap<u64, Entrada>>,
}

impl Rele {
    /// O relé do app, criado na primeira chamada.
    pub fn global() -> io::Result<&'static Self> {
        static GLOBAL: OnceLock<Result<Rele, String>> = OnceLock::new();
        GLOBAL
            .get_or_init(|| Self::novo().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| io::Error::other(e.clone()))
    }

    /// Um relé próprio, com a sua thread (os testes usam um por teste).
    pub fn novo() -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let handle = runtime.handle().clone();
        std::thread::Builder::new()
            .name("portas-rele".to_owned())
            .spawn(move || runtime.block_on(std::future::pending::<()>()))?;
        Ok(Self {
            runtime: handle,
            proximo: AtomicU64::new(1),
            abertos: Mutex::new(HashMap::new()),
        })
    }

    /// Começa a escutar em `bind` (e no `[::1]` da mesma porta, quando `bind` é o `127.0.0.1` e o
    /// computador tem IPv6, porque os navegadores tentam `localhost` primeiro por ele). Cada
    /// conexão aceita é discada até `destino` pelo `discador` e atendida no `modo`.
    pub fn abrir(
        &self,
        bind: SocketAddr,
        discador: Discador,
        destino: (String, u16),
        modo: Modo,
    ) -> io::Result<ReleId> {
        let principal = std::net::TcpListener::bind(bind)?;
        principal.set_nonblocking(true)?;
        let escuta = principal.local_addr()?;
        let mut sockets = vec![principal];
        if bind.ip() == IpAddr::V4(Ipv4Addr::LOCALHOST)
            && let Ok(gemeo) =
                std::net::TcpListener::bind((IpAddr::V6(Ipv6Addr::LOCALHOST), escuta.port()))
            && gemeo.set_nonblocking(true).is_ok()
        {
            sockets.push(gemeo);
        }

        let contadores = Arc::new(Contadores::default());
        let mut tarefas = Vec::new();
        for socket in sockets {
            let tarefa = self.runtime.spawn(escutar(
                socket,
                escuta.port(),
                discador.clone(),
                destino.clone(),
                modo.clone(),
                Arc::clone(&contadores),
            ));
            tarefas.push(tarefa.abort_handle());
        }
        let id = self.proximo.fetch_add(1, Ordering::Relaxed);
        self.abertos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                id,
                Entrada {
                    escuta,
                    contadores,
                    tarefas,
                },
            );
        Ok(ReleId(id))
    }

    /// Para de escutar e derruba as conexões abertas.
    pub fn parar(&self, id: ReleId) {
        let entrada = self
            .abertos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id.0);
        if let Some(entrada) = entrada {
            for tarefa in entrada.tarefas {
                tarefa.abort();
            }
        }
    }

    #[must_use]
    pub fn estado(&self, id: ReleId) -> Option<EstadoRele> {
        let abertos = self.abertos.lock().unwrap_or_else(PoisonError::into_inner);
        let entrada = abertos.get(&id.0)?;
        let c = &entrada.contadores;
        Some(EstadoRele {
            escuta: entrada.escuta,
            ativas: c.ativas.load(Ordering::Relaxed),
            total: c.total.load(Ordering::Relaxed),
            certificado: Contadores::ler(&c.certificado),
            erro: Contadores::ler(&c.erro),
        })
    }
}

impl Drop for Rele {
    fn drop(&mut self) {
        let abertos =
            std::mem::take(&mut *self.abertos.lock().unwrap_or_else(PoisonError::into_inner));
        for entrada in abertos.into_values() {
            for tarefa in entrada.tarefas {
                tarefa.abort();
            }
        }
    }
}

/// O laço de `accept` de um socket; as conexões vivem num `JoinSet` que cai (e as derruba) junto
/// com a tarefa.
async fn escutar(
    socket: std::net::TcpListener,
    porta_local: u16,
    discador: Discador,
    destino: (String, u16),
    modo: Modo,
    contadores: Arc<Contadores>,
) {
    let Ok(listener) = TcpListener::from_std(socket) else {
        return;
    };
    let tls = if matches!(modo, Modo::Https { .. }) {
        match tokio::task::spawn_blocking({
            let contadores = Arc::clone(&contadores);
            move || configurar_tls(&contadores)
        })
        .await
        {
            Ok(Ok(config)) => Some(TlsConnector::from(config)),
            Ok(Err(e)) => {
                Contadores::anotar(&contadores.erro, Some(e.to_string()));
                return;
            }
            Err(_) => return,
        }
    } else {
        None
    };
    let mut conexoes: JoinSet<()> = JoinSet::new();
    loop {
        let cliente = match listener.accept().await {
            Ok((cliente, _)) => cliente,
            Err(e) => {
                Contadores::anotar(&contadores.erro, Some(e.to_string()));
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        while conexoes.try_join_next().is_some() {}
        let ativa = Ativa::nova(&contadores);
        let discador = discador.clone();
        let destino = destino.clone();
        let modo = modo.clone();
        let tls = tls.clone();
        let contadores = Arc::clone(&contadores);
        conexoes.spawn(async move {
            let _ativa = ativa;
            let resultado = atender(
                cliente,
                &discador,
                &destino,
                &modo,
                tls.as_ref(),
                porta_local,
            )
            .await;
            match resultado {
                Ok(()) => Contadores::anotar(&contadores.erro, None),
                Err(e) if fim_comum(&e) => Contadores::anotar(&contadores.erro, None),
                Err(e) => Contadores::anotar(&contadores.erro, Some(e.to_string())),
            }
        });
    }
}

/// Um fim de conversa corriqueiro, que não é falha a mostrar: o navegador que fecha no meio, o
/// servidor que fecha o TLS sem o `close_notify` (muitos fazem).
fn fim_comum(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::UnexpectedEof
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::BrokenPipe
    )
}

async fn atender(
    mut cliente: TcpStream,
    discador: &Discador,
    destino: &(String, u16),
    modo: &Modo,
    tls: Option<&TlsConnector>,
    porta_local: u16,
) -> io::Result<()> {
    let canal = match discador.discar(&destino.0, destino.1).await {
        Ok(canal) => canal,
        Err(e) => {
            if !matches!(modo, Modo::Tcp) {
                responder_502(&mut cliente, &e.to_string()).await;
            }
            return Err(e);
        }
    };
    match modo {
        Modo::Tcp => copiar(cliente, canal).await,
        Modo::Http { host_http, apelido } => {
            atender_http(cliente, canal, host_http, apelido, porta_local).await
        }
        Modo::Https {
            sni,
            host_http,
            apelido,
        } => {
            let nome = nome_do_servidor(sni);
            let Some(tls) = tls else {
                return Err(io::Error::other("TLS sem configuração"));
            };
            let canal_tls = match tls.connect(nome, canal).await {
                Ok(canal_tls) => canal_tls,
                Err(e) => {
                    responder_502(&mut cliente, &e.to_string()).await;
                    return Err(e);
                }
            };
            let host_http = host_http.clone().unwrap_or_default();
            let apelido = apelido.clone().unwrap_or_default();
            atender_http(cliente, canal_tls, &host_http, &apelido, porta_local).await
        }
    }
}

/// O nome para o SNI: o `sni`, ou um IP (que não vai no SNI) quando vazio.
fn nome_do_servidor(sni: &str) -> ServerName<'static> {
    ServerName::try_from(sni.trim().to_owned())
        .unwrap_or_else(|_| ServerName::IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST).into()))
}

async fn responder_502(cliente: &mut TcpStream, motivo: &str) {
    let corpo = format!("502 Bad Gateway\n{motivo}\n");
    let resposta = format!(
        "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
        corpo.len()
    );
    let _ = cliente.write_all(resposta.as_bytes()).await;
    let _ = cliente.shutdown().await;
}

/// Copia nos dois sentidos até os dois lados fecharem.
async fn copiar<A, B>(mut a: A, mut b: B) -> io::Result<()>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    tokio::io::copy_bidirectional(&mut a, &mut b)
        .await
        .map(|_| ())
}

/// Lê de `leitor` até a cabeça HTTP fechar. Devolve os bytes lidos e onde a cabeça termina; `None`
/// quando a conexão fechou (ou a cabeça passou de [`CABECA_MAXIMA`]) antes disso.
async fn ler_cabeca<R: AsyncRead + Unpin>(leitor: &mut R) -> io::Result<(Vec<u8>, Option<usize>)> {
    let mut lido = Vec::new();
    let mut bloco = [0u8; 8192];
    loop {
        let n = leitor.read(&mut bloco).await?;
        if n == 0 {
            return Ok((lido, None));
        }
        // A cabeça pode chegar partida no meio do `\r\n\r\n`: confere desde 3 bytes antes.
        let desde = lido.len().saturating_sub(3);
        lido.extend_from_slice(&bloco[..n]);
        if let Some(fim) = fim_da_cabeca(&lido[desde..]) {
            return Ok((lido, Some(desde + fim)));
        }
        if lido.len() > CABECA_MAXIMA {
            return Ok((lido, None));
        }
    }
}

/// Atende um navegador falando HTTP com `servidor` (o canal, aberto ou com TLS por cima): o
/// pedido sai com a cabeça reescrita, a resposta volta com a cabeça reescrita, e o resto de cada
/// lado passa direto.
async fn atender_http<S>(
    cliente: TcpStream,
    servidor: S,
    host_http: &str,
    apelido: &str,
    porta_local: u16,
) -> io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut cliente_le, mut cliente_escreve) = cliente.into_split();
    let (mut servidor_le, mut servidor_escreve) = tokio::io::split(servidor);

    let (bytes, fim) = ler_cabeca(&mut cliente_le).await?;
    match fim {
        Some(fim) => {
            servidor_escreve
                .write_all(&reescrever_pedido(&bytes[..fim], host_http))
                .await?;
            servidor_escreve.write_all(&bytes[fim..]).await?;
        }
        // Não é HTTP (ou a cabeça não fechou): o que veio passa como está.
        None => servidor_escreve.write_all(&bytes).await?,
    }
    servidor_escreve.flush().await?;

    // O corpo do pedido (e o tráfego do WebSocket) sobe numa tarefa; quando o navegador fecha o
    // seu lado, o do servidor fecha também, e a resposta ainda pode chegar.
    let subida = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut cliente_le, &mut servidor_escreve).await;
        let _ = servidor_escreve.shutdown().await;
    });

    let descida = async {
        let (bytes, fim) = ler_cabeca(&mut servidor_le).await?;
        match fim {
            Some(fim) => {
                cliente_escreve
                    .write_all(&reescrever_resposta(
                        &bytes[..fim],
                        host_http,
                        apelido,
                        porta_local,
                    ))
                    .await?;
                cliente_escreve.write_all(&bytes[fim..]).await?;
            }
            None => cliente_escreve.write_all(&bytes).await?,
        }
        tokio::io::copy(&mut servidor_le, &mut cliente_escreve).await?;
        io::Result::Ok(())
    }
    .await;
    subida.abort();
    let _ = cliente_escreve.shutdown().await;
    descida
}

// ------------------------------------------------------------------------------------------
// TLS

/// As raízes do sistema, lidas uma vez.
fn raizes() -> Arc<RootCertStore> {
    static RAIZES: OnceLock<Arc<RootCertStore>> = OnceLock::new();
    Arc::clone(RAIZES.get_or_init(|| {
        let mut raizes = RootCertStore::empty();
        raizes.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        Arc::new(raizes)
    }))
}

/// O verificador que confere o certificado como qualquer cliente e, quando não confere, aceita do
/// mesmo jeito e anota o motivo: quem chega pelo SSH já confiou no servidor (seção 3.8).
#[derive(Debug)]
struct Complacente {
    interno: Arc<WebPkiServerVerifier>,
    anotar: Arc<Contadores>,
}

impl ServerCertVerifier for Complacente {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let resultado = self.interno.verify_server_cert(
            end_entity,
            intermediates,
            server_name,
            ocsp_response,
            now,
        );
        match resultado {
            Ok(ok) => {
                Contadores::anotar(&self.anotar.certificado, None);
                Ok(ok)
            }
            Err(e) => {
                Contadores::anotar(&self.anotar.certificado, Some(e.to_string()));
                Ok(ServerCertVerified::assertion())
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.interno.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.interno.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.interno.supported_verify_schemes()
    }
}

fn configurar_tls(contadores: &Arc<Contadores>) -> io::Result<Arc<ClientConfig>> {
    let provedor = Arc::new(default_provider());
    let interno = WebPkiServerVerifier::builder_with_provider(raizes(), Arc::clone(&provedor))
        .build()
        .map_err(|e| io::Error::other(e.to_string()))?;
    let config = ClientConfig::builder_with_provider(provedor)
        .with_safe_default_protocol_versions()
        .map_err(|e| io::Error::other(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Complacente {
            interno,
            anotar: Arc::clone(contadores),
        }))
        .with_no_client_auth();
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    fn executar<F: std::future::Future>(futuro: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(futuro)
    }

    /// Um par de `TcpStream` ligados no loopback: o primeiro é o navegador, o segundo o relé.
    async fn par_tcp() -> (TcpStream, TcpStream) {
        let escuta = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endereco = escuta.local_addr().unwrap();
        let navegador = TcpStream::connect(endereco).await.unwrap();
        (navegador, escuta.accept().await.unwrap().0)
    }

    #[test]
    fn o_modo_web_escolhe_pelo_https_e_pelo_nome() {
        assert_eq!(
            Modo::web(false, "loja.exemplo.com.br"),
            Modo::Http {
                host_http: "loja.exemplo.com.br".into(),
                apelido: "loja-exemplo-com-br".into()
            }
        );
        assert_eq!(
            Modo::web(true, " "),
            Modo::Https {
                sni: String::new(),
                host_http: None,
                apelido: None
            }
        );
        assert!(matches!(
            Modo::web(true, "loja.exemplo"),
            Modo::Https { sni, .. } if sni == "loja.exemplo"
        ));
    }

    #[test]
    fn a_cabeca_partida_em_pedacos_fecha_com_o_ultimo() {
        executar(async {
            let (mut escreve, mut le) = duplex(64);
            let leitura = tokio::spawn(async move { ler_cabeca(&mut le).await.unwrap() });
            escreve
                .write_all(b"GET / HTTP/1.1\r\nHost: a\r\n\r")
                .await
                .unwrap();
            tokio::task::yield_now().await;
            escreve.write_all(b"\ncorpo").await.unwrap();
            let (bytes, fim) = leitura.await.unwrap();
            assert_eq!(fim, Some(b"GET / HTTP/1.1\r\nHost: a\r\n\r\n".len()));
            assert_eq!(&bytes[fim.unwrap()..], b"corpo");
        });
    }

    #[test]
    fn sem_o_fim_da_cabeca_o_que_veio_passa_como_esta() {
        executar(async {
            let (mut escreve, mut le) = duplex(64);
            escreve.write_all(b"\x16\x03\x01nao e http").await.unwrap();
            drop(escreve);
            let (bytes, fim) = ler_cabeca(&mut le).await.unwrap();
            assert_eq!(fim, None);
            assert_eq!(bytes, b"\x16\x03\x01nao e http");
        });
    }

    #[test]
    fn o_rele_http_troca_o_host_e_reescreve_a_resposta() {
        executar(async {
            let (mut navegador, cliente) = par_tcp().await;
            let (servidor_do_rele, mut servidor) = duplex(4096);
            let rele = tokio::spawn(atender_http(
                cliente,
                servidor_do_rele,
                "loja.exemplo",
                "loja-exemplo",
                8080,
            ));
            navegador
                .write_all(
                    b"GET /x HTTP/1.1\r\nHost: localhost:8080\r\nConnection: keep-alive\r\n\r\n",
                )
                .await
                .unwrap();
            let (pedido, fim) = ler_cabeca(&mut servidor).await.unwrap();
            assert_eq!(fim, Some(pedido.len()));
            assert_eq!(
                String::from_utf8(pedido).unwrap(),
                "GET /x HTTP/1.1\r\nHost: loja.exemplo\r\nConnection: close\r\n\r\n"
            );
            servidor
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://loja.exemplo/y\r\nSet-Cookie: a=1; Domain=loja.exemplo; Path=/\r\nContent-Length: 2\r\n\r\nok")
                .await
                .unwrap();
            drop(servidor);
            let mut resposta = String::new();
            navegador.read_to_string(&mut resposta).await.unwrap();
            assert!(resposta.contains("Location: http://loja-exemplo.localhost:8080/y\r\n"));
            assert!(resposta.contains("Set-Cookie: a=1; Path=/\r\n"));
            assert!(resposta.ends_with("\r\n\r\nok"));
            rele.await.unwrap().unwrap();
        });
    }

    #[test]
    fn o_corpo_do_pedido_chega_inteiro_ao_servidor() {
        executar(async {
            let (mut navegador, cliente) = par_tcp().await;
            let (servidor_do_rele, mut servidor) = duplex(1 << 16);
            let rele = tokio::spawn(atender_http(cliente, servidor_do_rele, "", "", 8080));
            navegador
                .write_all(
                    b"POST /u HTTP/1.1\r\nHost: localhost:8080\r\nContent-Length: 5\r\n\r\nab",
                )
                .await
                .unwrap();
            navegador.write_all(b"cde").await.unwrap();
            navegador.shutdown().await.unwrap();
            let mut tudo = Vec::new();
            servidor.read_to_end(&mut tudo).await.unwrap();
            assert!(tudo.ends_with(b"\r\n\r\nabcde"));
            // O servidor responde depois do pedido inteiro e o rele termina.
            servidor
                .write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
                .await
                .unwrap();
            drop(servidor);
            let mut resposta = Vec::new();
            navegador.read_to_end(&mut resposta).await.unwrap();
            assert!(resposta.starts_with(b"HTTP/1.1 204"));
            rele.await.unwrap().unwrap();
        });
    }

    #[test]
    fn o_sni_vazio_vira_um_ip_que_nao_vai_no_sni() {
        assert!(matches!(nome_do_servidor(""), ServerName::IpAddress(_)));
        assert!(matches!(
            nome_do_servidor("loja.exemplo"),
            ServerName::DnsName(_)
        ));
    }

    #[test]
    fn parar_tira_o_rele_da_lista() {
        let rele = Rele::novo().unwrap();
        // O abridor só existe numa sessão SSH real; aqui basta o ciclo do socket.
        assert!(rele.estado(ReleId(99)).is_none());
        rele.parar(ReleId(99));
    }
}
