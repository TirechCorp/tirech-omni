//! O `connections.xml` do WinSSHTerm: a raiz `<WinSSHTerm>` com `Node`s `Container` (pasta) e
//! `Connection`. As senhas ficam cifradas com o arquivo `key` do programa e não dá para
//! recuperá-las daqui.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::xml::{self, Elemento};
use super::{
    Formato, Importacao, PastaTrazida, Trazida, avisar_ppk, nome_da_conexao, perfil_de_proxy,
    porta_ou,
};
use crate::modelo::{Connection, Kind, ModoX11};
use crate::redes::TipoProxy;
use crate::t;

/// O formato WinSSHTerm.
pub struct WinSshTerm;

impl Formato for WinSshTerm {
    fn id(&self) -> &'static str {
        "winsshterm"
    }

    fn nome(&self) -> &'static str {
        "WinSSHTerm"
    }

    fn arquivo_padrao(&self) -> &'static str {
        "connections.xml"
    }

    fn extensoes(&self) -> &'static [&'static str] {
        &["xml"]
    }

    fn locais_provaveis(&self, home: &Path) -> Vec<PathBuf> {
        vec![home.join("AppData/Roaming/WinSSHTerm/connections.xml")]
    }

    fn analisar(&self, texto: &str, _senha_mestra: Option<&str>) -> Result<Importacao, String> {
        analisar(texto)
    }
}

/// Nas versões antigas o nome do contêiner vinha em base64. Aceita os dois: só vale o base64 se
/// der um texto UTF-8 sem caractere de controle.
fn nome_da_pasta(bruto: &str) -> String {
    if bruto.len().is_multiple_of(4)
        && bruto.len() >= 4
        && let Ok(bytes) = STANDARD.decode(bruto)
        && let Ok(texto) = String::from_utf8(bytes)
        && !texto.is_empty()
        && !texto.chars().any(char::is_control)
    {
        return texto;
    }
    bruto.to_owned()
}

fn analisar(texto: &str) -> Result<Importacao, String> {
    let raiz = xml::ler(texto)?;
    if raiz.nome != "WinSSHTerm" {
        return Err(t!("importar-winsshterm-nao-e-xml"));
    }
    let mut importacao = Importacao::default();
    let mut senhas_cifradas = false;
    andar(
        &raiz,
        &mut Vec::new(),
        &mut importacao,
        &mut senhas_cifradas,
    );
    if senhas_cifradas {
        importacao.avisar(t!("importar-winsshterm-senhas"));
    }
    Ok(importacao)
}

fn andar(
    pai: &Elemento,
    caminho: &mut Vec<String>,
    importacao: &mut Importacao,
    senhas_cifradas: &mut bool,
) {
    for no in pai.filhos.iter().filter(|f| f.nome == "Node") {
        match no.attr_ou_vazio("Type") {
            "Container" => {
                caminho.push(nome_da_pasta(no.attr_ou_vazio("Name")));
                importacao
                    .entradas
                    .push(super::Entrada::Pasta(PastaTrazida {
                        caminho: caminho.clone(),
                        ..PastaTrazida::default()
                    }));
                andar(no, caminho, importacao, senhas_cifradas);
                caminho.pop();
            }
            "Connection" => {
                if !no.attr_ou_vazio("Password").is_empty() {
                    *senhas_cifradas = true;
                }
                let trazida = conexao(no, caminho.clone(), importacao);
                importacao.conexao(trazida);
            }
            _ => {}
        }
    }
}

fn conexao(no: &Elemento, pasta: Vec<String>, importacao: &mut Importacao) -> Trazida {
    let host = no.attr_ou_vazio("Hostname").trim().to_owned();
    let mut c = Connection::new(nome_da_conexao(no.attr_ou_vazio("Name"), &host), Kind::Ssh);
    c.port = porta_ou(no.attr_ou_vazio("Port"), 22);
    c.host = host;
    c.user = no.attr_ou_vazio("Username").trim().to_owned();
    let chave = no.attr_ou_vazio("PrivateKey").trim();
    if !chave.is_empty() {
        avisar_ppk(importacao, chave);
        c.key_file = Some(PathBuf::from(chave));
    }
    c.x11 = match no
        .attr_ou_vazio("sX11")
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "forward" | "trusted" => ModoX11::Confiavel,
        "don't forward" | "dont forward" => ModoX11::Desligado,
        _ => ModoX11::Padrao,
    };
    let mut trazida = Trazida::nova(c, pasta);
    let descricao = no.attr_ou_vazio("Descr");
    if !descricao.trim().is_empty() {
        trazida.conexao.descricao = descricao.to_owned();
    }
    if no
        .attr_ou_vazio("pSshProxy")
        .eq_ignore_ascii_case("enabled")
    {
        let tipo = match no.attr_ou_vazio("pType").to_ascii_uppercase().as_str() {
            "SOCKS5" | "SOCKS" => Some(TipoProxy::Socks5),
            "HTTP" | "HTTPS" => Some(TipoProxy::Http),
            _ => None,
        };
        let host = no.attr_ou_vazio("pHost").trim();
        match tipo {
            Some(tipo) if !host.is_empty() => {
                trazida.perfil = Some(perfil_de_proxy(
                    tipo,
                    host,
                    porta_ou(no.attr_ou_vazio("pPort"), 1080),
                    no.attr_ou_vazio("pUser"),
                    None,
                ));
            }
            _ => importacao.avisar(t!(
                "importar-winsshterm-proxy",
                tipo = no.attr_ou_vazio("pType")
            )),
        }
    }
    trazida
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::importar::Entrada;

    pub(crate) const AMOSTRA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<WinSSHTerm Version='1'>
  <Node Name='Clientes' Type='Container' Expanded='True'>
    <Node Name='ACME' Type='Container' Expanded='False'>
      <Node Name="ACME prod" Type="Connection" Descr="banco &amp; site" Username="root" Password="CIFRADO"
            PrivateKey="C:\keys\acme.ppk" Hostname="10.0.0.9" Port="2222" Certificate=""
            LaunchToolInt="" sX11="forward" cfProt="sftp" pSshProxy="disabled"
            pType="" pHost="" pPort="" pUser="" pTelnetCmd="" />
    </Node>
    <Node Name="Beta" Type="Connection" Descr="" Username="ana" Password=""
          PrivateKey="" Hostname="beta.exemplo.com" Port="22" sX11="don't forward"
          pSshProxy="enabled" pType="SOCKS5" pHost="127.0.0.1" pPort="1080" pUser="u" />
  </Node>
  <Node Name="Simples" Type="Connection" Username="" Password="" Hostname="192.168.0.2" Port="" />
</WinSSHTerm>"#;

    #[test]
    fn le_a_arvore_a_chave_o_x11_o_proxy_e_o_aviso_das_senhas() {
        let i = WinSshTerm.analisar(AMOSTRA, None).unwrap();
        assert_eq!(i.total(), 3);
        assert!(!i.pede_senha_mestra);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.name, "ACME prod");
        assert_eq!(c[0].pasta, ["Clientes", "ACME"]);
        assert_eq!(c[0].conexao.host, "10.0.0.9");
        assert_eq!(c[0].conexao.port, 2222);
        assert_eq!(c[0].conexao.user, "root");
        assert_eq!(c[0].conexao.kind, Kind::Ssh);
        assert_eq!(
            c[0].conexao.key_file,
            Some(PathBuf::from(r"C:\keys\acme.ppk"))
        );
        assert_eq!(c[0].conexao.x11, ModoX11::Confiavel);
        assert_eq!(c[0].conexao.descricao, "banco & site");
        assert_eq!(c[1].pasta, ["Clientes"]);
        assert_eq!(c[1].conexao.x11, ModoX11::Desligado);
        let proxy = c[1].perfil.as_ref().unwrap();
        assert_eq!(proxy.perfil.name, "Proxy importado (127.0.0.1:1080)");
        assert_eq!(proxy.usuario, "u");
        assert_eq!(c[2].conexao.name, "Simples");
        assert_eq!(c[2].conexao.port, 22);
        assert!(c[2].pasta.is_empty());
        assert_eq!(i.grupos(), 2);
        assert_eq!(i.avisos.len(), 2, "{:?}", i.avisos);
        assert!(i.avisos.iter().any(|a| a.contains("`key`")));
        assert!(i.avisos.iter().any(|a| a.contains(".ppk")));
    }

    #[test]
    fn mantem_a_ordem_e_as_pastas_vazias() {
        let i = WinSshTerm
            .analisar(
                "<WinSSHTerm><Node Name='Vazia' Type='Container'/><Node Name='a' Type='Connection' Hostname='h'/></WinSSHTerm>",
                None,
            )
            .unwrap();
        assert!(matches!(&i.entradas[0], Entrada::Pasta(p) if p.caminho == ["Vazia"]));
        assert!(matches!(&i.entradas[1], Entrada::Conexao(_)));
    }

    #[test]
    fn nome_de_pasta_em_base64_das_versoes_antigas() {
        assert_eq!(nome_da_pasta("Q2xpZW50ZXM="), "Clientes");
        assert_eq!(nome_da_pasta("Clientes"), "Clientes");
        assert_eq!(nome_da_pasta("Test"), "Test");
    }

    #[test]
    fn arquivo_de_outro_programa_ou_torto_vira_erro() {
        assert!(WinSshTerm.analisar("<FileZilla3/>", None).is_err());
        assert!(WinSshTerm.analisar("não é xml", None).is_err());
        assert!(WinSshTerm.analisar("<WinSSHTerm><Node", None).is_err());
    }
}
