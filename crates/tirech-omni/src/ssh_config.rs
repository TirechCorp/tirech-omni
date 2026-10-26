//! Importa o `~/.ssh/config` do OpenSSH: cada `Host` sem curinga vira uma conexão SSH.

use std::path::{Path, PathBuf};

use crate::armazenamento::expandir_til_em;
use crate::cofre::Cofre;
use crate::modelo::{Connection, Jump, Kind, ModoX11, TipoTunel, Tunel, dividir_destino};
use crate::t;

/// O que a importação achou.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Importacao {
    /// As conexões, na ordem do arquivo (os ids ainda são 0: a árvore os distribui).
    pub conexoes: Vec<Connection>,
    /// O que não deu para importar do jeito que o OpenSSH entende (para a tela mostrar).
    pub avisos: Vec<String>,
}

/// Lê o texto de um `config`, expandindo o `~` do `IdentityFile` com a pasta do usuário.
pub fn parse(text: &str) -> Importacao {
    parse_com_home(text, crate::armazenamento::pasta_pessoal().as_deref())
}

/// Como [`parse`], com a pasta do usuário dada (os testes não dependem do `HOME`).
pub fn parse_com_home(text: &str, home: Option<&Path>) -> Importacao {
    let mut saida = Importacao::default();
    // O bloco `Host` em andamento: os nomes e as opções que ele acumulou.
    let mut bloco: Option<Bloco> = None;

    for linha in text.lines() {
        let Some((chave, valor)) = chave_e_valor(linha) else {
            continue;
        };
        match chave.to_ascii_lowercase().as_str() {
            "host" => {
                fecha(bloco.take(), &mut saida);
                bloco = Some(Bloco::novo(&valor));
            }
            // `Match` abre um bloco que não se importa; as opções dele não valem para o `Host`
            // de antes.
            "match" => {
                fecha(bloco.take(), &mut saida);
            }
            _ => {
                if let Some(bloco) = bloco.as_mut() {
                    bloco.opcao(&chave.to_ascii_lowercase(), &valor, home);
                }
            }
        }
    }
    fecha(bloco.take(), &mut saida);
    saida
}

/// Separa `Chave valor`, `Chave=valor` e `Chave = valor`, ignorando comentários e linhas em
/// branco. As aspas em volta do valor saem.
fn chave_e_valor(linha: &str) -> Option<(String, String)> {
    let linha = linha.trim();
    if linha.is_empty() || linha.starts_with('#') {
        return None;
    }
    let fim = linha.find(|c: char| c.is_whitespace() || c == '=')?;
    let chave = &linha[..fim];
    let valor = linha[fim..].trim_start_matches(|c: char| c.is_whitespace() || c == '=');
    let valor = valor.trim();
    let valor = valor
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(valor);
    if valor.is_empty() {
        return None;
    }
    Some((chave.to_owned(), valor.to_owned()))
}

#[derive(Default)]
struct Bloco {
    nomes: Vec<String>,
    host_name: Option<String>,
    port: Option<u16>,
    user: Option<String>,
    identity: Option<PathBuf>,
    jump: Option<Jump>,
    forward_x11: Option<bool>,
    x11_confiavel: Option<bool>,
    tuneis: Vec<Tunel>,
    parar_se_tunel_falhar: Option<bool>,
    comprimir: Option<bool>,
    avisos: Vec<String>,
}

/// `yes`/`true` e `no`/`false`, sem diferenciar maiúsculas.
fn sim_ou_nao(valor: &str) -> Option<bool> {
    match valor.to_ascii_lowercase().as_str() {
        "yes" | "true" => Some(true),
        "no" | "false" => Some(false),
        _ => None,
    }
}

/// A origem de um túnel, `[bind:]porta`, com o IPv6 entre colchetes: o endereço (se houver) e
/// a porta.
fn origem_do_tunel(texto: &str) -> Option<(Option<String>, u16)> {
    if let Some(dentro) = texto.strip_prefix('[') {
        let (bind, depois) = dentro.split_once(']')?;
        let porta = depois.strip_prefix(':')?.parse().ok()?;
        return Some((Some(bind.to_owned()), porta));
    }
    match texto.rsplit_once(':') {
        Some((bind, porta)) => Some((Some(bind.to_owned()), porta.parse().ok()?)),
        None => Some((None, texto.parse().ok()?)),
    }
}

impl Bloco {
    fn novo(padroes: &str) -> Self {
        let nomes = padroes
            .split_whitespace()
            .filter(|p| !p.contains(['*', '?', '!']))
            .map(str::to_owned)
            .collect();
        Self {
            nomes,
            ..Self::default()
        }
    }

    /// Como no OpenSSH, vale o primeiro valor dado a cada opção.
    fn opcao(&mut self, chave: &str, valor: &str, home: Option<&Path>) {
        match chave {
            "hostname" => {
                self.host_name.get_or_insert_with(|| valor.to_owned());
            }
            "port" => {
                if self.port.is_none() {
                    self.port = valor.parse().ok();
                }
            }
            "user" => {
                self.user.get_or_insert_with(|| valor.to_owned());
            }
            "identityfile" => {
                if self.identity.is_none() {
                    self.identity = Some(expandir_til_em(valor, home));
                }
            }
            "proxyjump" => {
                if self.jump.is_none() && !valor.eq_ignore_ascii_case("none") {
                    let saltos: Vec<&str> = valor
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect();
                    if saltos.len() > 1 {
                        self.avisos.push(t!(
                            "ssh-config-proxyjump",
                            n = saltos.len() as i64,
                            ultimo = saltos[saltos.len() - 1].to_string()
                        ));
                    }
                    self.jump = saltos.last().map(|s| salto(s));
                }
            }
            "forwardx11" => {
                if self.forward_x11.is_none() {
                    self.forward_x11 = sim_ou_nao(valor);
                }
            }
            "forwardx11trusted" => {
                if self.x11_confiavel.is_none() {
                    self.x11_confiavel = sim_ou_nao(valor);
                }
            }
            "compression" => {
                if self.comprimir.is_none() {
                    self.comprimir = sim_ou_nao(valor);
                }
            }
            "exitonforwardfailure" => {
                if self.parar_se_tunel_falhar.is_none() {
                    self.parar_se_tunel_falhar = sim_ou_nao(valor);
                }
            }
            "localforward" => self.encaminhamento("LocalForward", TipoTunel::Local, valor),
            "remoteforward" => self.encaminhamento("RemoteForward", TipoTunel::Remoto, valor),
            "dynamicforward" => self.encaminhamento("DynamicForward", TipoTunel::Dinamico, valor),
            "proxycommand" => self.avisos.push(t!("ssh-config-proxycommand")),
            _ => {}
        }
    }
}

impl Bloco {
    /// Uma linha `LocalForward`, `RemoteForward` ou `DynamicForward`. As diretivas se acumulam.
    fn encaminhamento(&mut self, diretiva: &str, tipo: TipoTunel, valor: &str) {
        let partes: Vec<&str> = valor.split_whitespace().collect();
        let (origem, destino) = match (tipo, partes.as_slice()) {
            (TipoTunel::Dinamico, [origem]) => (*origem, None),
            (TipoTunel::Remoto, [_]) => {
                self.avisos.push(t!(
                    "ssh-config-socks-remoto",
                    diretiva = diretiva,
                    valor = valor
                ));
                return;
            }
            (TipoTunel::Local | TipoTunel::Remoto, [origem, destino]) => (*origem, Some(*destino)),
            _ => {
                self.avisos.push(t!(
                    "ssh-config-nao-entendi",
                    diretiva = diretiva,
                    valor = valor
                ));
                return;
            }
        };
        if origem.starts_with('/') {
            self.avisos.push(t!(
                "ssh-config-socket-de-origem",
                diretiva = diretiva,
                valor = valor
            ));
            return;
        }
        let Some((bind, porta)) = origem_do_tunel(origem) else {
            self.avisos.push(t!(
                "ssh-config-nao-entendi-porta",
                diretiva = diretiva,
                valor = valor
            ));
            return;
        };
        let mut tunel = Tunel::novo(tipo, false);
        tunel.porta = porta;
        match bind.as_deref() {
            None | Some("" | "localhost" | "127.0.0.1" | "::1") => {}
            Some("*" | "0.0.0.0" | "::") => tunel.outras_maquinas = true,
            Some(_) => {
                tunel.outras_maquinas = true;
                self.avisos.push(t!(
                    "ssh-config-todas-as-interfaces",
                    diretiva = diretiva,
                    origem = origem
                ));
            }
        }
        if let Some(destino) = destino {
            if destino.starts_with('/') {
                if tipo == TipoTunel::Remoto {
                    self.avisos.push(t!(
                        "ssh-config-r-para-socket",
                        diretiva = diretiva,
                        valor = valor
                    ));
                    return;
                }
                tunel.tipo = TipoTunel::SocketUnix;
                destino.clone_into(&mut tunel.caminho_remoto);
            } else if let Some((host, porta)) = dividir_destino(destino) {
                tunel.destino_host = host;
                tunel.destino_porta = porta;
            } else {
                self.avisos.push(t!(
                    "ssh-config-destino-incompleto",
                    diretiva = diretiva,
                    valor = valor
                ));
                return;
            }
        }
        if let Some(problema) = tunel.problema() {
            self.avisos.push(t!(
                "ssh-config-problema",
                diretiva = diretiva,
                valor = valor,
                problema = problema
            ));
            return;
        }
        self.tuneis.push(tunel);
    }
}

fn fecha(bloco: Option<Bloco>, saida: &mut Importacao) {
    let Some(bloco) = bloco else {
        return;
    };
    for nome in &bloco.nomes {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.host = bloco.host_name.clone().unwrap_or_else(|| nome.clone());
        if let Some(port) = bloco.port {
            c.port = port;
        }
        c.user = bloco.user.clone().unwrap_or_default();
        c.key_file = bloco.identity.clone();
        c.jump = bloco.jump.clone().map(|mut j| {
            if j.user.is_empty() {
                j.user = c.user.clone();
            }
            j
        });
        c.x11 = match bloco.forward_x11 {
            Some(false) => ModoX11::Desligado,
            Some(true) if bloco.x11_confiavel == Some(true) => ModoX11::Confiavel,
            // O padrão do OpenSSH com `ForwardX11 yes` é o `-X`.
            Some(true) => ModoX11::NaoConfiavel,
            None => ModoX11::Padrao,
        };
        c.tuneis.clone_from(&bloco.tuneis);
        c.parar_se_tunel_falhar = bloco.parar_se_tunel_falhar == Some(true);
        c.comprimir = bloco.comprimir == Some(true);
        saida.conexoes.push(c);
        for aviso in &bloco.avisos {
            saida.avisos.push(t!(
                "ssh-config-aviso-da-conexao",
                nome = nome.as_str(),
                aviso = aviso.as_str()
            ));
        }
    }
}

/// `usuario@host:porta`, com o usuário e a porta opcionais e o IPv6 entre colchetes.
fn salto(texto: &str) -> Jump {
    let (user, resto) = match texto.split_once('@') {
        Some((user, resto)) => (user, resto),
        None => ("", texto),
    };
    let (host, port) = if let Some(dentro) = resto.strip_prefix('[') {
        match dentro.split_once(']') {
            Some((host, depois)) => (host, depois.strip_prefix(':').and_then(|p| p.parse().ok())),
            None => (dentro, None),
        }
    } else {
        match resto.rsplit_once(':') {
            Some((host, porta)) if porta.parse::<u16>().is_ok() => (host, porta.parse().ok()),
            _ => (resto, None),
        }
    };
    Jump {
        host: host.to_owned(),
        port: port.unwrap_or(22),
        user: user.to_owned(),
        credencial: None,
    }
}

/// Um `IdentityFile` cuja pública é a de uma chave do cofre passa a usar a do cofre: a conexão
/// ganha o `uid` dela e perde o caminho do arquivo.
pub fn ligar_ao_cofre(conexoes: &mut [Connection], cofre: &Cofre) {
    for c in conexoes {
        let uid = c
            .key_file
            .as_deref()
            .and_then(crate::agente::fingerprint_do_arquivo)
            .and_then(|fingerprint| cofre.chave_ssh_por_fingerprint(&fingerprint))
            .map(|item| item.uid.clone());
        if uid.is_some() {
            c.chave_do_cofre = uid;
            c.key_file = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXEMPLO: &str = "\
# minhas máquinas
Host *
    ServerAliveInterval 30

Host web-01 web-02
    HostName web.exemplo
    User ana
    Port 2222
    IdentityFile ~/.ssh/id_web
    IdentityFile ~/.ssh/outra

Host interno
  HostName=10.0.0.5
  ProxyJump ana@bastiao.exemplo:2200

Host db-*
    User dba

Include config.d/*

Host duplo
    HostName duplo.exemplo
    ProxyJump a@um, b@dois:23

Match host nuvem
    User ignorado

Host tunel
    ProxyCommand ssh -W %h:%p bastiao
";

    fn importa() -> Importacao {
        parse_com_home(EXEMPLO, Some(Path::new("/home/ana")))
    }

    fn por_nome<'a>(i: &'a Importacao, nome: &str) -> &'a Connection {
        i.conexoes
            .iter()
            .find(|c| c.name == nome)
            .unwrap_or_else(|| panic!("sem a conexão {nome}"))
    }

    #[test]
    fn curingas_e_match_nao_viram_conexao() {
        let nomes: Vec<String> = importa()
            .conexoes
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(nomes, ["web-01", "web-02", "interno", "duplo", "tunel"]);
    }

    #[test]
    fn host_com_dois_nomes_gera_duas_conexoes_iguais() {
        let i = importa();
        let (a, b) = (por_nome(&i, "web-01"), por_nome(&i, "web-02"));
        for c in [a, b] {
            assert_eq!(c.host, "web.exemplo");
            assert_eq!(c.port, 2222);
            assert_eq!(c.user, "ana");
            assert_eq!(c.kind, Kind::Ssh);
        }
    }

    #[test]
    fn identity_file_expande_o_til_e_vale_o_primeiro() {
        let i = importa();
        assert_eq!(
            por_nome(&i, "web-01").key_file.as_deref(),
            Some(Path::new("/home/ana/.ssh/id_web"))
        );
        assert!(
            !por_nome(&i, "web-01")
                .key_file
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .contains('~')
        );
    }

    #[test]
    fn identity_file_de_uma_chave_do_cofre_liga_ao_cofre() {
        use crate::armazenamento::testes::PastaTemporaria;
        use crate::cofre::{ChaveSsh, Item, Tipo};
        use tomni_ui::terminal::ssh::keys::{KeyFile, KeyGen, KeyType};

        let pasta = PastaTemporaria::nova("ssh-config-cofre");
        let gerada = KeyGen::new(KeyType::Ed25519).generate().unwrap();
        let arquivo = pasta.0.join("ssh/id_github");
        gerada.write(&arquivo, false).unwrap();
        let outra = KeyGen::new(KeyType::Ed25519).generate().unwrap();
        let arquivo_fora = pasta.0.join("ssh/id_fora");
        outra.write(&arquivo_fora, false).unwrap();

        let (mut cofre, _) = Cofre::abrir(&pasta.0.join("cofre"));
        let aberta = KeyFile::open(&arquivo, None).unwrap();
        cofre
            .salvar(Item {
                tipo: Tipo::ChaveSsh,
                titulo: "github".to_owned(),
                chave: Some(ChaveSsh::de_chave(aberta.private_key()).unwrap()),
                ..Item::default()
            })
            .unwrap();
        let uid = cofre.chaves_ssh()[0].uid.clone();

        let mut com = Connection::new("com", Kind::Ssh);
        com.key_file = Some(arquivo);
        let mut sem = Connection::new("sem", Kind::Ssh);
        sem.key_file = Some(arquivo_fora.clone());
        let mut conexoes = [com, sem];
        ligar_ao_cofre(&mut conexoes, &cofre);
        assert_eq!(conexoes[0].chave_do_cofre.as_deref(), Some(uid.as_str()));
        assert_eq!(conexoes[0].key_file, None);
        assert_eq!(conexoes[1].chave_do_cofre, None);
        assert_eq!(conexoes[1].key_file, Some(arquivo_fora));
    }

    #[test]
    fn sem_hostname_o_host_e_o_proprio_nome_e_a_porta_e_22() {
        let i = parse_com_home("Host solto\n", None);
        let c = &i.conexoes[0];
        assert_eq!((c.host.as_str(), c.port), ("solto", 22));
        assert_eq!(c.user, "");
    }

    #[test]
    fn hostname_com_igual_e_proxyjump_com_usuario_e_porta() {
        let i = importa();
        let c = por_nome(&i, "interno");
        assert_eq!(c.host, "10.0.0.5");
        assert_eq!(
            c.jump,
            Some(Jump {
                host: "bastiao.exemplo".into(),
                port: 2200,
                user: "ana".into(),
                credencial: None
            })
        );
    }

    #[test]
    fn varios_saltos_ficam_so_com_o_ultimo_e_avisam() {
        let i = importa();
        let c = por_nome(&i, "duplo");
        assert_eq!(c.jump.as_ref().unwrap().host, "dois");
        assert_eq!(c.jump.as_ref().unwrap().port, 23);
        assert!(
            i.avisos
                .iter()
                .any(|a| a.starts_with("duplo:") && a.contains("2 saltos"))
        );
    }

    #[test]
    fn proxy_command_e_ignorado_com_aviso() {
        let i = importa();
        assert!(por_nome(&i, "tunel").jump.is_none());
        assert!(
            i.avisos
                .iter()
                .any(|a| a.starts_with("tunel:") && a.contains("ProxyCommand"))
        );
    }

    #[test]
    fn salto_sem_usuario_usa_o_da_conexao() {
        let i = parse_com_home("Host x\n  User ana\n  ProxyJump bastiao\n", None);
        assert_eq!(
            i.conexoes[0].jump,
            Some(Jump {
                host: "bastiao".into(),
                port: 22,
                user: "ana".into(),
                credencial: None
            })
        );
    }

    #[test]
    fn salto_ipv6_entre_colchetes() {
        let j = salto("root@[::1]:2022");
        assert_eq!(
            (j.host.as_str(), j.port, j.user.as_str()),
            ("::1", 2022, "root")
        );
    }

    #[test]
    fn texto_vazio_nao_importa_nada() {
        assert_eq!(parse_com_home("", None), Importacao::default());
        assert_eq!(
            parse_com_home("# só comentário\n\n", None),
            Importacao::default()
        );
    }

    #[test]
    fn forwardx11_vira_o_modo() {
        let i = parse_com_home(
            "Host a\n ForwardX11 no\nHost b\n ForwardX11 yes\nHost c\n ForwardX11 yes\n ForwardX11Trusted yes\nHost d\n ForwardX11Trusted yes\n ForwardX11 yes\nHost e\n HostName e.exemplo\n",
            None,
        );
        let modo = |n: &str| i.conexoes.iter().find(|c| c.name == n).map(|c| c.x11);
        assert_eq!(modo("a"), Some(ModoX11::Desligado));
        assert_eq!(modo("b"), Some(ModoX11::NaoConfiavel));
        assert_eq!(modo("c"), Some(ModoX11::Confiavel));
        assert_eq!(modo("d"), Some(ModoX11::Confiavel));
        assert_eq!(modo("e"), Some(ModoX11::Padrao));
    }

    #[test]
    fn compression_liga_a_compressao() {
        let i = parse_com_home(
            "Host a\n Compression yes\nHost b\n Compression no\nHost c\n Compression yes\n Compression no\nHost d\n Compression delayed\nHost e\n HostName e.exemplo\n",
            None,
        );
        let comprime = |n: &str| i.conexoes.iter().find(|c| c.name == n).map(|c| c.comprimir);
        assert_eq!(comprime("a"), Some(true));
        assert_eq!(comprime("b"), Some(false));
        // O primeiro valor vale.
        assert_eq!(comprime("c"), Some(true));
        // Um valor que o OpenSSH não aceita é ignorado.
        assert_eq!(comprime("d"), Some(false));
        assert_eq!(comprime("e"), Some(false));
    }

    #[test]
    fn os_tres_forwards_viram_tuneis() {
        let i = parse_com_home(
            "Host a\n LocalForward 8080 db:5432\n LocalForward 127.0.0.1:9090 [::1]:80\n RemoteForward 9000 localhost:3000\n DynamicForward 1080\n LocalForward 5433 /var/run/libvirt/libvirt-sock\n ExitOnForwardFailure yes\n",
            None,
        );
        let c = &i.conexoes[0];
        assert!(i.avisos.is_empty(), "{:?}", i.avisos);
        assert!(c.parar_se_tunel_falhar);
        assert_eq!(c.tuneis.len(), 5);
        assert_eq!(c.tuneis[0].tipo, TipoTunel::Local);
        assert_eq!(
            (c.tuneis[0].porta, c.tuneis[0].destino()),
            (8080, "db:5432".to_owned())
        );
        assert_eq!(c.tuneis[1].destino(), "[::1]:80");
        assert!(!c.tuneis[1].outras_maquinas);
        assert_eq!(c.tuneis[2].tipo, TipoTunel::Remoto);
        assert_eq!(c.tuneis[3].tipo, TipoTunel::Dinamico);
        assert_eq!(c.tuneis[3].porta, 1080);
        assert_eq!(c.tuneis[4].tipo, TipoTunel::SocketUnix);
        assert_eq!(c.tuneis[4].caminho_remoto, "/var/run/libvirt/libvirt-sock");
        assert_eq!(c.tuneis[4].porta, 5433);
    }

    #[test]
    fn o_endereco_de_escuta_decide_as_outras_maquinas() {
        let i = parse_com_home(
            "Host a\n LocalForward *:1 x:1\n LocalForward 0.0.0.0:2 x:2\n LocalForward localhost:3 x:3\n LocalForward 192.168.0.5:4 x:4\n LocalForward [::1]:5 x:5\n",
            None,
        );
        let abertos: Vec<bool> = i.conexoes[0]
            .tuneis
            .iter()
            .map(|t| t.outras_maquinas)
            .collect();
        assert_eq!(abertos, [true, true, false, true, false]);
        assert_eq!(
            i.avisos,
            ["a: LocalForward 192.168.0.5:4 …: o endereço vira \"todas as interfaces\"."]
        );
    }

    #[test]
    fn forwards_que_nao_cabem_avisam_e_somem() {
        let i = parse_com_home(
            "Host a\n LocalForward /tmp/s.sock x:1\n RemoteForward 9000\n RemoteForward 9001 /tmp/r.sock\n LocalForward 0 x:1\n LocalForward 8080 semporta\n ClearAllForwardings yes\n ForwardX11Timeout 5\n GatewayPorts yes\n",
            None,
        );
        assert!(i.conexoes[0].tuneis.is_empty());
        assert_eq!(i.avisos.len(), 5, "{:?}", i.avisos);
    }
}
