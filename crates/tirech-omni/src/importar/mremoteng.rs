//! O `confCons.xml` do mRemoteNG: `Connections` com `Node`s `Container` (pastas) e `Connection`.
//! Lê SSH2 e Telnet; RDP, VNC e o resto viram aviso. As senhas vêm cifradas com a senha-mestra
//! (a padrão `mR3m`, ou a que a pessoa der): AES-256-GCM no formato atual, AES-128-CBC no antigo.

use std::path::{Path, PathBuf};

use super::cripto::{
    ITERACOES_PADRAO, PROTEGIDO_NAO, PROTEGIDO_SIM, SENHA_MESTRA_PADRAO, mremoteng_decifrar,
    mremoteng_decifrar_antigo,
};
use super::xml::{self, Elemento};
use super::{
    Entrada, Formato, Ignorado, Importacao, PastaTrazida, Trazida, nao_suportadas, nome_da_conexao,
    porta_ou,
};
use crate::modelo::{Connection, Kind};
use crate::t;

/// O formato mRemoteNG.
pub struct MRemoteNg;

impl Formato for MRemoteNg {
    fn id(&self) -> &'static str {
        "mremoteng"
    }

    fn nome(&self) -> &'static str {
        "mRemoteNG"
    }

    fn arquivo_padrao(&self) -> &'static str {
        "confCons.xml"
    }

    fn extensoes(&self) -> &'static [&'static str] {
        &["xml"]
    }

    fn locais_provaveis(&self, home: &Path) -> Vec<PathBuf> {
        vec![home.join("AppData/Roaming/mRemoteNG/confCons.xml")]
    }

    fn analisar(&self, texto: &str, senha_mestra: Option<&str>) -> Result<Importacao, String> {
        analisar(texto, senha_mestra)
    }
}

/// Como as senhas do arquivo estão cifradas.
enum Modo {
    /// `BlockCipherMode="GCM"`, com as rodadas do PBKDF2.
    Gcm(u32),
    /// Sem `BlockCipherMode` (mRemoteNG até a 1.76): CBC com MD5.
    Antigo,
    /// CBC e CCM do formato novo: não lidos.
    SemSuporte(String),
}

impl Modo {
    fn decifrar(&self, texto: &str, senha_mestra: &str) -> Result<String, String> {
        match self {
            Modo::Gcm(rodadas) => mremoteng_decifrar(texto, senha_mestra, *rodadas),
            Modo::Antigo => mremoteng_decifrar_antigo(texto, senha_mestra),
            Modo::SemSuporte(nome) => Err(t!("importar-mremoteng-modo", nome = nome.as_str())),
        }
    }
}

/// O que as pastas e conexões herdam: usuário e senha já em claro.
#[derive(Clone, Default)]
struct Credenciais {
    usuario: String,
    senha: Option<String>,
}

/// O contexto da leitura, para não passar meia dúzia de argumentos de função em função.
struct Leitura {
    modo: Modo,
    /// A senha-mestra que abriu o arquivo (ou `None`: sem senhas).
    senha_mestra: Option<String>,
    ilegiveis: usize,
    ignoradas: Vec<(String, usize)>,
}

fn verdadeiro(no: &Elemento, atributo: &str) -> bool {
    no.attr(atributo)
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
}

fn analisar(texto: &str, senha_mestra: Option<&str>) -> Result<Importacao, String> {
    let raiz = match xml::ler(texto) {
        Ok(raiz) => raiz,
        Err(erro) => {
            // Com a cifra do arquivo inteiro ligada, o `confCons.xml` é só base64.
            return Err(if texto.trim_start().starts_with('<') {
                erro
            } else {
                t!("importar-mremoteng-parece-cifrado")
            });
        }
    };
    if raiz.nome != "Connections" {
        return Err(t!("importar-mremoteng-nao-e-xml"));
    }
    if verdadeiro(&raiz, "FullFileEncryption") {
        return Err(t!("importar-mremoteng-cifrado"));
    }
    let modo = match raiz.attr("BlockCipherMode").map(str::trim) {
        None | Some("") => Modo::Antigo,
        Some(m) if m.eq_ignore_ascii_case("GCM") => Modo::Gcm(
            raiz.attr("KdfIterations")
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(ITERACOES_PADRAO),
        ),
        Some(outro) => Modo::SemSuporte(outro.to_owned()),
    };
    let mut importacao = Importacao::default();
    let tentativa = senha_mestra.unwrap_or(SENHA_MESTRA_PADRAO);
    let senha_certa = match (&modo, raiz.attr("Protected")) {
        (Modo::SemSuporte(nome), _) => {
            importacao.avisar(t!(
                "importar-mremoteng-modo-sem-suporte",
                nome = nome.as_str()
            ));
            None
        }
        (_, Some(protegido)) => match modo.decifrar(protegido, tentativa) {
            Ok(t) if matches!(modo, Modo::Gcm(_)) || t == PROTEGIDO_SIM || t == PROTEGIDO_NAO => {
                Some(tentativa.to_owned())
            }
            _ => {
                importacao.pede_senha_mestra = true;
                if senha_mestra.is_some() {
                    importacao.avisar(t!("importar-mremoteng-senha-recusada"));
                }
                None
            }
        },
        (_, None) => Some(tentativa.to_owned()),
    };
    let mut leitura = Leitura {
        modo,
        senha_mestra: senha_certa,
        ilegiveis: 0,
        ignoradas: Vec::new(),
    };
    andar(
        &raiz,
        &mut Vec::new(),
        &Credenciais::default(),
        &mut leitura,
        &mut importacao,
    );
    for (protocolo, quantas) in leitura.ignoradas {
        importacao.avisar(nao_suportadas(
            quantas,
            Ignorado::Conexao,
            &t!("importar-com-protocolo", nome = protocolo.as_str()),
        ));
    }
    if leitura.ilegiveis > 0 {
        importacao.avisar(t!(
            "importar-mremoteng-ilegiveis",
            n = leitura.ilegiveis as i64
        ));
    }
    Ok(importacao)
}

/// As credenciais de um nó: as dele, ou as do pai quando ele herda.
fn credenciais_do_no(no: &Elemento, pai: &Credenciais, leitura: &mut Leitura) -> Credenciais {
    let usuario = if verdadeiro(no, "InheritUsername") {
        pai.usuario.clone()
    } else {
        no.attr_ou_vazio("Username").trim().to_owned()
    };
    let senha = if verdadeiro(no, "InheritPassword") {
        pai.senha.clone()
    } else {
        let cifrada = no.attr_ou_vazio("Password").trim();
        match (&leitura.senha_mestra, cifrada.is_empty()) {
            (Some(mestra), false) => match leitura.modo.decifrar(cifrada, mestra) {
                Ok(senha) => Some(senha),
                Err(_) => {
                    leitura.ilegiveis += 1;
                    None
                }
            },
            _ => None,
        }
    };
    Credenciais { usuario, senha }
}

fn andar(
    pai: &Elemento,
    caminho: &mut Vec<String>,
    herdadas: &Credenciais,
    leitura: &mut Leitura,
    importacao: &mut Importacao,
) {
    for no in pai.filhos.iter().filter(|n| n.nome == "Node") {
        match no.attr_ou_vazio("Type") {
            "Container" => {
                let credenciais = credenciais_do_no(no, herdadas, leitura);
                caminho.push(nome_da_conexao(no.attr_ou_vazio("Name"), "Pasta"));
                importacao.entradas.push(Entrada::Pasta(PastaTrazida {
                    caminho: caminho.clone(),
                    ..PastaTrazida::default()
                }));
                andar(no, caminho, &credenciais, leitura, importacao);
                caminho.pop();
            }
            "Connection" => conexao(no, caminho, herdadas, leitura, importacao),
            _ => {}
        }
    }
}

fn conexao(
    no: &Elemento,
    caminho: &[String],
    herdadas: &Credenciais,
    leitura: &mut Leitura,
    importacao: &mut Importacao,
) {
    let protocolo = no.attr_ou_vazio("Protocol").trim();
    let tipo = match protocolo.to_ascii_uppercase().as_str() {
        "SSH2" => Kind::Ssh,
        "TELNET" => Kind::Telnet,
        outro => {
            let nome = if outro.is_empty() {
                "desconhecido"
            } else {
                outro
            };
            match leitura.ignoradas.iter_mut().find(|(n, _)| n == nome) {
                Some((_, quantas)) => *quantas += 1,
                None => leitura.ignoradas.push((nome.to_owned(), 1)),
            }
            return;
        }
    };
    let host = no.attr_ou_vazio("Hostname").trim().to_owned();
    if host.is_empty() {
        return;
    }
    let credenciais = credenciais_do_no(no, herdadas, leitura);
    let mut c = Connection::new(nome_da_conexao(no.attr_ou_vazio("Name"), &host), tipo);
    c.port = porta_ou(
        no.attr_ou_vazio("Port"),
        if tipo == Kind::Ssh { 22 } else { 23 },
    );
    c.host = host;
    c.user = credenciais.usuario;
    let mut trazida = Trazida::nova(c, caminho.to_vec());
    trazida.senha = credenciais.senha;
    let descricao = no.attr_ou_vazio("Descr");
    if !descricao.trim().is_empty() {
        trazida.conexao.descricao = descricao.to_owned();
    }
    importacao.conexao(trazida);
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::importar::cripto::mremoteng_cifrar;
    use aes::cipher::{BlockModeEncrypt, KeyIvInit, block_padding::Pkcs7};
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use md5::{Digest, Md5};

    fn gcm(texto: &str, mestra: &str) -> String {
        mremoteng_cifrar(texto, mestra, 1000, [1; 16], [2; 16]).unwrap()
    }

    fn antigo(texto: &str, mestra: &str) -> String {
        let iv = [5u8; 16];
        let chave = Md5::digest(mestra.as_bytes());
        let mut buffer = vec![0u8; texto.len() + 16];
        buffer[..texto.len()].copy_from_slice(texto.as_bytes());
        let cifrado = cbc::Encryptor::<aes::Aes128>::new_from_slices(&chave, &iv)
            .unwrap()
            .encrypt_padded::<Pkcs7>(&mut buffer, texto.len())
            .unwrap()
            .to_vec();
        STANDARD.encode([&iv[..], &cifrado[..]].concat())
    }

    /// O arquivo de exemplo, com as senhas cifradas por `cifrar`.
    fn arquivo(cabecalho: &str, protegido: &str, cifrar: &dyn Fn(&str) -> String) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<Connections Name="Connections" Export="False" {cabecalho} Protected="{protegido}" ConfVersion="2.6">
  <Node Name="Clientes" Type="Container" Expanded="True" Username="ana" Password="{pasta}" InheritUsername="False">
    <Node Name="Web &amp; API" Type="Connection" Descr="o site" Hostname="web.exemplo.com" Protocol="SSH2" Port="2222" InheritUsername="True" InheritPassword="True" />
    <Node Name="Banco" Type="Connection" Hostname="db.exemplo.com" Protocol="SSH2" Port="22" Username="root" Password="{propria}" />
    <Node Name="Desktop" Type="Connection" Hostname="pc.exemplo.com" Protocol="RDP" Port="3389" />
    <Node Name="Vazia" Type="Container"></Node>
  </Node>
  <Node Name="Roteador" Type="Connection" Hostname="10.0.0.1" Protocol="Telnet" Port="23" Username="admin" />
</Connections>"#,
            pasta = cifrar("senha-da-pasta"),
            propria = cifrar("senha-propria"),
        )
    }

    fn confere(i: &Importacao, com_senhas: bool) {
        assert_eq!(i.total(), 3);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.name, "Web & API");
        assert_eq!(c[0].pasta, ["Clientes"]);
        assert_eq!(c[0].conexao.port, 2222);
        assert_eq!(c[0].conexao.user, "ana");
        assert_eq!(c[0].conexao.descricao, "o site");
        assert_eq!(c[1].conexao.user, "root");
        assert_eq!(c[2].conexao.kind, Kind::Telnet);
        assert!(c[2].pasta.is_empty());
        if com_senhas {
            assert_eq!(c[0].senha.as_deref(), Some("senha-da-pasta"));
            assert_eq!(c[1].senha.as_deref(), Some("senha-propria"));
        } else {
            assert!(c.iter().all(|t| t.senha.is_none()));
        }
        assert!(
            i.avisos
                .iter()
                .any(|a| a.contains("1 conexão com protocolo RDP"))
        );
        assert!(
            i.entradas
                .iter()
                .any(|e| matches!(e, Entrada::Pasta(p) if p.caminho == ["Clientes", "Vazia"]))
        );
    }

    #[test]
    fn gcm_com_a_senha_mestra_padrao() {
        let texto = arquivo(
            r#"EncryptionEngine="AES" BlockCipherMode="GCM" KdfIterations="1000" FullFileEncryption="False""#,
            &gcm(PROTEGIDO_SIM, "mR3m"),
            &|t| gcm(t, "mR3m"),
        );
        let i = MRemoteNg.analisar(&texto, None).unwrap();
        assert!(!i.pede_senha_mestra);
        confere(&i, true);
    }

    #[test]
    fn senha_mestra_propria_pergunta_e_depois_abre() {
        let texto = arquivo(
            r#"BlockCipherMode="GCM" KdfIterations="1000""#,
            &gcm(PROTEGIDO_SIM, "outra"),
            &|t| gcm(t, "outra"),
        );
        let sem = MRemoteNg.analisar(&texto, None).unwrap();
        assert!(sem.pede_senha_mestra);
        confere(&sem, false);
        let errada = MRemoteNg.analisar(&texto, Some("errada")).unwrap();
        assert!(errada.pede_senha_mestra);
        assert!(
            errada
                .avisos
                .iter()
                .any(|a| a.contains("não abre o arquivo"))
        );
        let certa = MRemoteNg.analisar(&texto, Some("outra")).unwrap();
        assert!(!certa.pede_senha_mestra);
        confere(&certa, true);
    }

    #[test]
    fn formato_antigo_cbc() {
        let texto = arquivo("", &antigo(PROTEGIDO_NAO, "mR3m"), &|t| antigo(t, "mR3m"));
        let i = MRemoteNg.analisar(&texto, None).unwrap();
        assert!(!i.pede_senha_mestra);
        confere(&i, true);
    }

    #[test]
    fn modo_sem_suporte_e_arquivo_cifrado_por_inteiro() {
        let texto = arquivo(r#"BlockCipherMode="CCM""#, "xx", &|t| t.to_owned());
        let i = MRemoteNg.analisar(&texto, None).unwrap();
        assert!(i.avisos.iter().any(|a| a.contains("CCM")));
        confere(&i, false);
        let inteiro = r#"<Connections FullFileEncryption="True" BlockCipherMode="GCM"/>"#;
        assert!(MRemoteNg.analisar(inteiro, None).is_err());
        assert!(
            MRemoteNg
                .analisar("QUJDREVGR0g=", None)
                .unwrap_err()
                .contains("cifrado por inteiro")
        );
        assert!(MRemoteNg.analisar("<WinSSHTerm/>", None).is_err());
    }
}
