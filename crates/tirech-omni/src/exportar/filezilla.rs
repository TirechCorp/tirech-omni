//! O `sitemanager.xml` do FileZilla: as conexões de arquivo (FTP, FTPS, SFTP e S3), com as pastas
//! do plano. O FileZilla não abre SSH com terminal.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::plano::{NoPlano, PastaPlano};
use super::{ItemPlano, Opcoes, Plano, Resultado, escapar_xml};
use crate::modelo::{Ativar, FtpCodificacao, FtpModo, FtpSeguranca, Kind};
use crate::t;

/// O número do protocolo no arquivo do FileZilla.
fn protocolo(item: &ItemPlano) -> u8 {
    let c = &item.conexao;
    match c.kind {
        Kind::Sftp => 1,
        Kind::S3 => 7,
        _ => match c.ftp_security {
            FtpSeguranca::ExplicitaSeDisponivel => 0,
            FtpSeguranca::Implicita => 3,
            FtpSeguranca::Explicita => 4,
            FtpSeguranca::Nenhuma => 6,
        },
    }
}

/// O `RemoteDir` do FileZilla: o tipo Unix (`1`), sem prefixo (`0`) e cada trecho com o tamanho
/// na frente, por exemplo `/var/www` → `1 0 3 var 3 www`.
fn caminho_remoto(caminho: &str) -> String {
    let mut saida = String::from("1 0");
    for trecho in caminho.split('/').filter(|t| !t.is_empty()) {
        saida.push_str(&format!(" {} {trecho}", trecho.chars().count()));
    }
    saida
}

/// `https://s3.eu-west-1.amazonaws.com:9000` vira `(s3.eu-west-1.amazonaws.com, 9000)`.
fn endereco_s3(endpoint: &str) -> (String, u16) {
    let (https, resto) = match endpoint.split_once("://") {
        Some((esquema, resto)) => (!esquema.eq_ignore_ascii_case("http"), resto),
        None => (true, endpoint),
    };
    let resto = resto.trim_end_matches('/');
    let padrao = if https { 443 } else { 80 };
    match resto.rsplit_once(':') {
        Some((host, porta)) if !host.contains(']') || host.ends_with(']') => (
            host.to_owned(),
            porta.parse().ok().filter(|p| *p > 0).unwrap_or(padrao),
        ),
        _ => (resto.to_owned(), padrao),
    }
}

fn servidor(item: &ItemPlano, opcoes: Opcoes, saida: &mut String, nivel: usize) {
    let c = &item.conexao;
    let recuo = " ".repeat(nivel);
    let (host, porta) = if c.kind == Kind::S3 {
        endereco_s3(&c.s3.endpoint)
    } else {
        (c.host.clone(), c.port)
    };
    let anonimo = c.user.eq_ignore_ascii_case("anonymous");
    let senha = item.senha.as_deref().filter(|_| opcoes.senhas);
    // 0 anônimo, 1 normal, 2 pede a senha, 5 arquivo de chave.
    let tipo_de_login = if anonimo {
        0
    } else if c.key_file.is_some() && c.kind == Kind::Sftp {
        5
    } else if senha.is_some() {
        1
    } else {
        2
    };
    saida.push_str(&format!("{recuo}<Server>\n"));
    let campo = |saida: &mut String, nome: &str, valor: &str| {
        saida.push_str(&format!(
            "{recuo} <{nome}>{}</{nome}>\n",
            escapar_xml(valor)
        ));
    };
    campo(saida, "Host", &host);
    campo(saida, "Port", &porta.to_string());
    campo(saida, "Protocol", &protocolo(item).to_string());
    campo(saida, "Type", "0");
    campo(saida, "User", if anonimo { "" } else { &c.user });
    campo(saida, "Logontype", &tipo_de_login.to_string());
    if let Some(senha) = senha.filter(|_| !anonimo) {
        saida.push_str(&format!(
            "{recuo} <Pass encoding=\"base64\">{}</Pass>\n",
            STANDARD.encode(senha)
        ));
    }
    if c.kind == Kind::Sftp
        && let Some(chave) = c.key_file.as_deref()
    {
        campo(saida, "Keyfile", &chave.to_string_lossy());
    }
    if c.kind == Kind::Ftp {
        campo(
            saida,
            "PasvMode",
            if c.ftp_mode == FtpModo::Ativo {
                "MODE_ACTIVE"
            } else {
                "MODE_PASSIVE"
            },
        );
        match c.ftp_encoding {
            FtpCodificacao::Auto => campo(saida, "EncodingType", "Auto"),
            FtpCodificacao::Utf8 => campo(saida, "EncodingType", "UTF-8"),
            FtpCodificacao::Latin1 => {
                campo(saida, "EncodingType", "Custom");
                campo(saida, "CustomEncoding", "ISO-8859-1");
            }
        }
        campo(saida, "TimezoneOffset", &c.ftp_timezone_min.to_string());
    }
    if !c.pasta_local.is_empty() {
        campo(saida, "LocalDir", &c.pasta_local);
    }
    if c.kind != Kind::S3 && !c.pasta_remota.is_empty() {
        campo(saida, "RemoteDir", &caminho_remoto(&c.pasta_remota));
    }
    // O FileZilla só conhece ligado ou desligado: "Padrão" e "Desligado" saem sem o campo.
    if c.sincronizar == Ativar::Sim {
        campo(saida, "SyncBrowsing", "1");
    }
    if c.comparar == Ativar::Sim {
        campo(saida, "DirectoryComparison", "1");
    }
    if !c.descricao.trim().is_empty() {
        campo(saida, "Comments", &c.descricao);
    }
    campo(saida, "Name", &c.name);
    saida.push_str(&format!("{recuo}</Server>\n"));
}

fn pasta(p: &PastaPlano, opcoes: Opcoes, saida: &mut String, nivel: usize) {
    let recuo = " ".repeat(nivel);
    saida.push_str(&format!(
        "{recuo}<Folder expanded=\"0\">{}\n",
        escapar_xml(&p.nome)
    ));
    nos(&p.filhos, opcoes, saida, nivel + 1);
    saida.push_str(&format!("{recuo}</Folder>\n"));
}

fn nos(lista: &[NoPlano], opcoes: Opcoes, saida: &mut String, nivel: usize) {
    for no in lista {
        match no {
            NoPlano::Pasta(p) => pasta(p, opcoes, saida, nivel),
            NoPlano::Item(item) => servidor(item, opcoes, saida, nivel),
        }
    }
}

/// Escreve o arquivo.
pub fn exportar(plano: &Plano, itens: &[&ItemPlano], opcoes: Opcoes) -> Resultado {
    let mut resultado = Resultado::default();
    let mut saida = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<FileZilla3 version=\"3.66.5\" platform=\"*nix\">\n <Servers>\n",
    );
    nos(&plano.aninhar(itens), opcoes, &mut saida, 2);
    saida.push_str(" </Servers>\n</FileZilla3>\n");
    let com_bucket = itens
        .iter()
        .filter(|i| i.conexao.kind == Kind::S3 && !i.conexao.s3.bucket.is_empty())
        .count();
    if com_bucket > 0 {
        resultado.avisar(t!("exportar-filezilla-bucket", n = com_bucket as i64));
    }
    let com_salto = itens.iter().filter(|i| !i.saltos.is_empty()).count();
    if com_salto > 0 {
        resultado.avisar(t!("exportar-filezilla-salto", n = com_salto as i64));
    }
    resultado.texto = saida;
    resultado
}
