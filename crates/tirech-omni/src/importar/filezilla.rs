//! O `sitemanager.xml` do FileZilla: `<Servers>` com `Folder`s (o nome é o texto do próprio nó)
//! e `Server`s. Lê FTP, FTPS, SFTP e S3; os outros protocolos viram aviso.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::xml::{self, Elemento};
use super::{
    Entrada, Formato, Ignorado, Importacao, PastaTrazida, Trazida, avisar_ppk, nao_suportadas,
    nome_da_conexao, opcoes_s3, porta_ou,
};
use crate::modelo::{Ativar, Connection, FtpCodificacao, FtpModo, FtpSeguranca, FtpTls, Kind};
use crate::t;

/// O formato FileZilla.
pub struct FileZilla;

impl Formato for FileZilla {
    fn id(&self) -> &'static str {
        "filezilla"
    }

    fn nome(&self) -> &'static str {
        "FileZilla"
    }

    fn arquivo_padrao(&self) -> &'static str {
        "sitemanager.xml"
    }

    fn extensoes(&self) -> &'static [&'static str] {
        &["xml"]
    }

    fn locais_provaveis(&self, home: &Path) -> Vec<PathBuf> {
        vec![
            home.join(".config/filezilla/sitemanager.xml"),
            home.join("AppData/Roaming/FileZilla/sitemanager.xml"),
        ]
    }

    fn analisar(&self, texto: &str, _senha_mestra: Option<&str>) -> Result<Importacao, String> {
        analisar(texto)
    }
}

/// O nome que o FileZilla dá a cada número de protocolo que o Tirech Omni não abre.
fn nome_do_protocolo(numero: i64) -> String {
    match numero {
        2 | 5 => "WebDAV".to_owned(),
        8 => "Storj".to_owned(),
        n => t!("importar-de-codigo", n = n),
    }
}

/// Um `RemoteDir` do FileZilla (o `CServerPath` serializado): o tipo do servidor, o prefixo
/// (`0` quando não há) e cada trecho do caminho com o tamanho na frente, por exemplo
/// `1 0 3 var 3 www` → `/var/www`. Só os tipos Unix (`0` e `1`) viram caminho; VMS, DOS e os
/// outros ficam de fora.
fn caminho_remoto(texto: &str) -> Option<String> {
    fn palavra(texto: &str) -> (&str, &str) {
        texto.split_once(' ').unwrap_or((texto, ""))
    }
    let (tipo, resto) = palavra(texto.trim());
    if !matches!(tipo, "0" | "1") {
        return None;
    }
    let (prefixo, mut resto) = palavra(resto);
    if prefixo != "0" {
        return None;
    }
    let mut trechos = Vec::new();
    while !resto.is_empty() {
        let (tamanho, depois) = palavra(resto);
        let tamanho: usize = tamanho.parse().ok()?;
        let fim = depois
            .char_indices()
            .nth(tamanho)
            .map_or(depois.len(), |(i, _)| i);
        if depois[..fim].chars().count() != tamanho {
            return None;
        }
        trechos.push(&depois[..fim]);
        resto = depois[fim..].strip_prefix(' ').unwrap_or(&depois[fim..]);
    }
    Some(format!("/{}", trechos.join("/")))
}

/// `SyncBrowsing` e `DirectoryComparison`: `1` liga; `0` ou ausente segue as Configurações
/// (o FileZilla escreve `0` em todo servidor, e isso não é uma escolha da pessoa).
fn ativar(texto: &str) -> Ativar {
    if texto.trim() == "1" {
        Ativar::Sim
    } else {
        Ativar::Padrao
    }
}

fn analisar(texto: &str) -> Result<Importacao, String> {
    let raiz = xml::ler(texto)?;
    if raiz.nome != "FileZilla3" {
        return Err(t!("importar-filezilla-nao-e-xml"));
    }
    let mut importacao = Importacao::default();
    let mut ignorados: BTreeMap<i64, usize> = BTreeMap::new();
    let mut cifradas = 0usize;
    if let Some(servidores) = raiz.filho("Servers") {
        andar(
            servidores,
            &mut Vec::new(),
            &mut importacao,
            &mut ignorados,
            &mut cifradas,
        );
    }
    for (numero, quantos) in ignorados {
        importacao.avisar(nao_suportadas(
            quantos,
            Ignorado::Conexao,
            &t!("importar-com-protocolo", nome = nome_do_protocolo(numero)),
        ));
    }
    if cifradas > 0 {
        importacao.avisar(t!("importar-filezilla-cifradas", n = cifradas as i64));
    }
    Ok(importacao)
}

fn andar(
    pai: &Elemento,
    caminho: &mut Vec<String>,
    importacao: &mut Importacao,
    ignorados: &mut BTreeMap<i64, usize>,
    cifradas: &mut usize,
) {
    for no in &pai.filhos {
        match no.nome.as_str() {
            "Folder" => {
                // O nome é o texto do nó; versões novas também o põem num filho `Name`.
                let nome = if no.texto.is_empty() {
                    no.texto_do_filho("Name").to_owned()
                } else {
                    no.texto.clone()
                };
                caminho.push(nome);
                importacao.entradas.push(Entrada::Pasta(PastaTrazida {
                    caminho: caminho.clone(),
                    ..PastaTrazida::default()
                }));
                andar(no, caminho, importacao, ignorados, cifradas);
                caminho.pop();
            }
            "Server" => {
                if let Some(trazida) =
                    servidor(no, caminho.clone(), importacao, ignorados, cifradas)
                {
                    importacao.conexao(trazida);
                }
            }
            _ => {}
        }
    }
}

fn servidor(
    no: &Elemento,
    pasta: Vec<String>,
    importacao: &mut Importacao,
    ignorados: &mut BTreeMap<i64, usize>,
    cifradas: &mut usize,
) -> Option<Trazida> {
    let protocolo: i64 = no.texto_do_filho("Protocol").trim().parse().unwrap_or(0);
    let host = no.texto_do_filho("Host").trim().to_owned();
    let nome = nome_da_conexao(no.texto_do_filho("Name"), &host);
    let porta_texto = no.texto_do_filho("Port");
    let mut c = match protocolo {
        0 | 3 | 4 | 6 => {
            let mut c = Connection::new(nome, Kind::Ftp);
            c.ftp_security = match protocolo {
                3 => FtpSeguranca::Implicita,
                4 => FtpSeguranca::Explicita,
                6 => FtpSeguranca::Nenhuma,
                _ => FtpSeguranca::ExplicitaSeDisponivel,
            };
            c.ftp_tls = FtpTls::Auto;
            c.ftp_mode = match no.texto_do_filho("PasvMode") {
                "MODE_ACTIVE" => FtpModo::Ativo,
                _ => FtpModo::Passivo,
            };
            c.ftp_encoding = match no.texto_do_filho("EncodingType") {
                "UTF-8" => FtpCodificacao::Utf8,
                "Custom" => {
                    let outra = no.texto_do_filho("CustomEncoding").to_ascii_lowercase();
                    if matches!(outra.as_str(), "iso-8859-1" | "latin1" | "latin-1") {
                        FtpCodificacao::Latin1
                    } else {
                        FtpCodificacao::Auto
                    }
                }
                _ => FtpCodificacao::Auto,
            };
            c.ftp_timezone_min = no
                .texto_do_filho("TimezoneOffset")
                .trim()
                .parse()
                .unwrap_or(0);
            c.port = porta_ou(porta_texto, c.ftp_security.porta_padrao());
            c
        }
        1 => {
            let mut c = Connection::new(nome, Kind::Sftp);
            c.port = porta_ou(porta_texto, 22);
            c
        }
        7 => {
            let mut c = Connection::new(nome, Kind::S3);
            let porta = porta_ou(porta_texto, 443);
            c.s3 = opcoes_s3(&host, porta, true, "");
            c.port = 0;
            c
        }
        outro => {
            *ignorados.entry(outro).or_default() += 1;
            return None;
        }
    };
    if protocolo != 7 {
        c.host = host;
    }
    c.pasta_local = no.texto_do_filho("LocalDir").trim().to_owned();
    if protocolo != 7 {
        c.pasta_remota = caminho_remoto(no.texto_do_filho("RemoteDir")).unwrap_or_default();
    }
    c.sincronizar = ativar(no.texto_do_filho("SyncBrowsing"));
    c.comparar = ativar(no.texto_do_filho("DirectoryComparison"));
    let tipo_de_login: i64 = no.texto_do_filho("Logontype").trim().parse().unwrap_or(1);
    c.user = if tipo_de_login == 0 {
        "anonymous".to_owned()
    } else {
        no.texto_do_filho("User").trim().to_owned()
    };
    let chave = no.texto_do_filho("Keyfile").trim();
    if !chave.is_empty() {
        avisar_ppk(importacao, chave);
        c.key_file = Some(PathBuf::from(chave));
    }
    let mut trazida = Trazida::nova(c, pasta);
    if let Some(pass) = no.filho("Pass") {
        match pass.attr("encoding") {
            Some("base64") => {
                if let Ok(bytes) = STANDARD.decode(pass.texto.trim()) {
                    trazida.senha = Some(String::from_utf8_lossy(&bytes).into_owned());
                }
            }
            Some(_) => *cifradas += 1,
            None if !pass.texto.is_empty() => trazida.senha = Some(pass.texto.clone()),
            None => {}
        }
    }
    let comentario = no.texto_do_filho("Comments");
    if !comentario.trim().is_empty() {
        trazida.conexao.descricao = comentario.to_owned();
    }
    Some(trazida)
}

#[cfg(test)]
mod testes {
    use super::*;

    const AMOSTRA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FileZilla3 version="3.66.5" platform="*nix">
 <Servers>
  <Folder expanded="1">Clientes
   <Folder expanded="0">ACME
    <Server>
     <Host>ftp.acme.com</Host><Port>21</Port><Protocol>0</Protocol><Type>0</Type>
     <User>acme</User><Pass encoding="base64">c2VucmE=</Pass>
     <Logontype>1</Logontype><PasvMode>MODE_ACTIVE</PasvMode>
     <EncodingType>Custom</EncodingType><CustomEncoding>ISO-8859-1</CustomEncoding>
     <TimezoneOffset>-60</TimezoneOffset>
     <LocalDir>/home/ana/acme</LocalDir><RemoteDir>1 0 3 var 8 meu site</RemoteDir>
     <SyncBrowsing>1</SyncBrowsing><DirectoryComparison>0</DirectoryComparison>
     <Comments>servidor velho</Comments>
     <Name>ACME FTP</Name>
    </Server>
   </Folder>
   <Server>
    <Host>sftp.exemplo.com</Host><Port>2200</Port><Protocol>1</Protocol>
    <User>ana</User><Logontype>5</Logontype><Keyfile>/home/ana/.ssh/id.ppk</Keyfile>
    <DirectoryComparison>1</DirectoryComparison>
    <Name>Beta SFTP</Name>
   </Server>
   <Server>
    <Host>dav.exemplo.com</Host><Port>443</Port><Protocol>5</Protocol><User>u</User><Name>DAV</Name>
   </Server>
  </Folder>
  <Server>
   <Host>ftp.publico.org</Host><Port></Port><Protocol>3</Protocol><Logontype>0</Logontype>
   <Pass encoding="crypt">AAAA</Pass><Name></Name>
  </Server>
  <Server>
   <Host>s3.eu-west-1.amazonaws.com</Host><Protocol>7</Protocol><User>AKIAXYZ</User>
   <Pass encoding="base64">c2VnLXMz</Pass><Name>Bucket</Name>
  </Server>
 </Servers>
</FileZilla3>"#;

    #[test]
    fn le_pastas_protocolos_senha_e_avisos() {
        let i = FileZilla.analisar(AMOSTRA, None).unwrap();
        assert_eq!(i.total(), 4);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.name, "ACME FTP");
        assert_eq!(c[0].pasta, ["Clientes", "ACME"]);
        assert_eq!(c[0].conexao.kind, Kind::Ftp);
        assert_eq!(
            c[0].conexao.ftp_security,
            FtpSeguranca::ExplicitaSeDisponivel
        );
        assert_eq!(c[0].conexao.ftp_mode, FtpModo::Ativo);
        assert_eq!(c[0].conexao.ftp_encoding, FtpCodificacao::Latin1);
        assert_eq!(c[0].conexao.ftp_timezone_min, -60);
        assert_eq!(c[0].senha.as_deref(), Some("senra"));
        assert_eq!(c[0].conexao.descricao, "servidor velho");
        assert_eq!(c[1].conexao.kind, Kind::Sftp);
        assert_eq!(c[1].conexao.port, 2200);
        assert_eq!(c[1].pasta, ["Clientes"]);
        assert_eq!(
            c[1].conexao.key_file,
            Some(PathBuf::from("/home/ana/.ssh/id.ppk"))
        );
        assert_eq!(c[2].conexao.name, "ftp.publico.org");
        assert_eq!(c[2].conexao.ftp_security, FtpSeguranca::Implicita);
        assert_eq!(c[2].conexao.port, 990);
        assert_eq!(c[2].conexao.user, "anonymous");
        assert!(c[2].senha.is_none());
        assert!(c[2].pasta.is_empty());
        assert_eq!(c[3].conexao.kind, Kind::S3);
        assert_eq!(
            c[3].conexao.s3.endpoint,
            "https://s3.eu-west-1.amazonaws.com"
        );
        assert_eq!(c[3].conexao.s3.region, "eu-west-1");
        assert_eq!(c[3].conexao.user, "AKIAXYZ");
        assert_eq!(c[3].senha.as_deref(), Some("seg-s3"));
        assert!(
            i.avisos
                .iter()
                .any(|a| a.contains("1 conexão com protocolo WebDAV"))
        );
        assert!(i.avisos.iter().any(|a| a.contains("chave-mestra")));
        assert!(i.avisos.iter().any(|a| a.contains(".ppk")));
    }

    #[test]
    fn le_as_pastas_padrao_e_os_dois_liga_desliga() {
        let i = FileZilla.analisar(AMOSTRA, None).unwrap();
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.pasta_local, "/home/ana/acme");
        assert_eq!(c[0].conexao.pasta_remota, "/var/meu site");
        assert_eq!(c[0].conexao.sincronizar, Ativar::Sim);
        // O `0` que o FileZilla escreve em todo servidor não vira "Desligado".
        assert_eq!(c[0].conexao.comparar, Ativar::Padrao);
        assert_eq!(c[1].conexao.comparar, Ativar::Sim);
        assert_eq!(c[1].conexao.sincronizar, Ativar::Padrao);
        assert_eq!(c[1].conexao.pasta_local, "");
        assert_eq!(c[1].conexao.pasta_remota, "");
    }

    #[test]
    fn o_caminho_remoto_segue_o_formato_do_filezilla() {
        // Os três primeiros são de um `sitemanager.xml` de verdade.
        assert_eq!(caminho_remoto("1 0 3 opt").as_deref(), Some("/opt"));
        assert_eq!(
            caminho_remoto("1 0 3 opt 6 docker 12 sistemaocesp").as_deref(),
            Some("/opt/docker/sistemaocesp")
        );
        assert_eq!(
            caminho_remoto("1 0 21 johannc35.sg-host.com 11 public_html 10 wp-content 7 plugins")
                .as_deref(),
            Some("/johannc35.sg-host.com/public_html/wp-content/plugins")
        );
        assert_eq!(caminho_remoto("1 0").as_deref(), Some("/"));
        assert_eq!(caminho_remoto("0 0 4 html").as_deref(), Some("/html"));
        assert_eq!(
            caminho_remoto("1 0 11 São João pé").as_deref(),
            Some("/São João pé")
        );
        // Outros tipos de servidor, prefixo, tamanho errado e lixo não viram caminho.
        assert_eq!(caminho_remoto("3 0 3 DSK"), None);
        assert_eq!(caminho_remoto("1 1 4 root 3 var"), None);
        assert_eq!(caminho_remoto("1 0 9 var"), None);
        assert_eq!(caminho_remoto("1 0 x var"), None);
        assert_eq!(caminho_remoto(""), None);
    }

    #[test]
    fn xml_de_outro_programa_vira_erro() {
        assert!(FileZilla.analisar("<WinSSHTerm/>", None).is_err());
    }
}
