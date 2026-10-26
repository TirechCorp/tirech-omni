//! O `WinSCP.ini`: uma seção `[Sessions\Pasta/Nome]` por conexão, com o nome em percentual. As
//! senhas saem embaralhadas do jeito do WinSCP (a chave é `usuário + servidor`) com "incluir
//! senhas".

use super::{Aleatorio, ItemPlano, NomesUnicos, Opcoes, Plano, Resultado};
use crate::importar::codificar_percentual;
use crate::importar::cripto::winscp_embaralhar;
use crate::modelo::{FtpSeguranca, Kind};
use crate::redes::TipoProxy;
use crate::t;

const MANTER: &str = "-_.";

fn percentual(texto: &str) -> String {
    codificar_percentual(texto, MANTER)
}

/// `https://s3.eu-west-1.amazonaws.com:9000` vira `(s3.eu-west-1.amazonaws.com, 9000, true)`.
fn endereco_s3(endpoint: &str) -> (String, u16, bool) {
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
            https,
        ),
        _ => (resto.to_owned(), padrao, https),
    }
}

/// Quem embaralha: a fonte dos bytes, as senhas grandes demais e a seção atual.
struct Cifra<'a> {
    aleatorio: &'a mut dyn Aleatorio,
    grandes: usize,
}

impl Cifra<'_> {
    /// A linha `campo=senha embaralhada`, ou nada se a senha não cabe (255 bytes).
    fn senha(&mut self, secao: &mut String, campo: &str, chave: &str, senha: &str) {
        let mut sorteio = || self.aleatorio.byte();
        match winscp_embaralhar(chave, senha, &mut sorteio) {
            Some(texto) => secao.push_str(&format!("{campo}={texto}\n")),
            None => self.grandes += 1,
        }
    }
}

fn sessao(item: &ItemPlano, nome: &str, opcoes: Opcoes, cifra: &mut Cifra) -> (String, bool) {
    let c = &item.conexao;
    let mut s = format!("[Sessions\\{nome}]\n");
    let (host, porta, https) = if c.kind == Kind::S3 {
        endereco_s3(&c.s3.endpoint)
    } else {
        (c.host.clone(), c.port, true)
    };
    s.push_str(&format!("HostName={host}\n"));
    s.push_str(&format!("PortNumber={porta}\n"));
    if !c.user.is_empty() {
        s.push_str(&format!("UserName={}\n", percentual(&c.user)));
    }
    s.push_str(&format!(
        "FSProtocol={}\n",
        match c.kind {
            Kind::Ftp => 5,
            Kind::S3 => 7,
            _ => 2,
        }
    ));
    match c.kind {
        Kind::Ftp => {
            let ftps = match c.ftp_security {
                FtpSeguranca::Implicita => 1,
                FtpSeguranca::Explicita | FtpSeguranca::ExplicitaSeDisponivel => 3,
                FtpSeguranca::Nenhuma => 0,
            };
            if ftps != 0 {
                s.push_str(&format!("Ftps={ftps}\n"));
            }
        }
        Kind::S3 => {
            s.push_str(&format!("Ftps={}\n", u8::from(https)));
            if !c.s3.region.is_empty() {
                s.push_str(&format!("S3DefaultRegion={}\n", c.s3.region));
            }
            if !c.s3.bucket.is_empty() {
                s.push_str(&format!("RemoteDirectory=/{}\n", percentual(&c.s3.bucket)));
            }
        }
        _ => {}
    }
    if !c.pasta_local.is_empty() {
        s.push_str(&format!(
            "LocalDirectory={}\n",
            codificar_percentual(&c.pasta_local, "-_./")
        ));
    }
    // No S3 a pasta remota do WinSCP é o bucket (escrito acima).
    if c.kind != Kind::S3 && !c.pasta_remota.is_empty() {
        s.push_str(&format!(
            "RemoteDirectory={}\n",
            codificar_percentual(&c.pasta_remota, "-_./")
        ));
    }
    if let Some(chave) = c.key_file.as_deref()
        && matches!(c.kind, Kind::Ssh | Kind::Sftp)
    {
        s.push_str(&format!(
            "PublicKeyFile={}\n",
            percentual(&chave.to_string_lossy())
        ));
    }
    if opcoes.senhas {
        if let Some(senha) = &item.senha {
            cifra.senha(&mut s, "Password", &format!("{}{host}", c.user), senha);
        }
        if let Some(token) = &item.token_s3 {
            s.push_str(&format!("S3SessionToken={token}\n"));
        }
    }
    if let Some(proxy) = &item.proxy {
        s.push_str(&format!(
            "ProxyMethod={}\nProxyHost={}\nProxyPort={}\n",
            if proxy.tipo == TipoProxy::Socks5 {
                2
            } else {
                3
            },
            proxy.host,
            proxy.port
        ));
        if !proxy.usuario.is_empty() {
            s.push_str(&format!("ProxyUsername={}\n", percentual(&proxy.usuario)));
        }
        if let Some(senha) = &proxy.senha {
            let chave = format!("{}{}", proxy.usuario, proxy.host);
            cifra.senha(&mut s, "ProxyPassword", &chave, senha);
        }
    }
    let varios_saltos = item.saltos.len() > 1;
    if let Some(salto) = item.ultimo_salto() {
        let j = &salto.salto;
        s.push_str(&format!(
            "Tunnel=1\nTunnelHostName={}\nTunnelPortNumber={}\n",
            j.host,
            if j.port == 0 { 22 } else { j.port }
        ));
        if !j.user.is_empty() {
            s.push_str(&format!("TunnelUserName={}\n", percentual(&j.user)));
        }
        if let Some(senha) = &salto.senha {
            cifra.senha(
                &mut s,
                "TunnelPassword",
                &format!("{}{}", j.user, j.host),
                senha,
            );
        }
    }
    if !item.conexao.descricao.trim().is_empty() {
        s.push_str(&format!("Note={}\n", percentual(&item.conexao.descricao)));
    }
    (s, varios_saltos)
}

/// Escreve o arquivo.
pub fn exportar(
    plano: &Plano,
    itens: &[&ItemPlano],
    opcoes: Opcoes,
    aleatorio: &mut dyn Aleatorio,
) -> Resultado {
    let mut resultado = Resultado::default();
    let mut nomes = NomesUnicos::default();
    let mut cifra = Cifra {
        aleatorio,
        grandes: 0,
    };
    let mut texto = String::new();
    let mut varios_saltos = 0usize;
    for item in itens {
        let mut partes: Vec<String> = item.pasta.iter().map(|p| percentual(p)).collect();
        partes.push(percentual(&item.conexao.name));
        let nome = nomes.unico(&partes.join("/"));
        let (secao, varios) = sessao(item, &nome, opcoes, &mut cifra);
        varios_saltos += usize::from(varios);
        texto.push_str(&secao);
        texto.push('\n');
    }
    // Uma pasta que não tem conexão (nem outra pasta) dentro vira `[Sessions\Pasta/]`.
    for pasta in &plano.pastas {
        let ocupada = plano
            .pastas
            .iter()
            .any(|outra| outra.len() > pasta.len() && outra.starts_with(pasta))
            || itens.iter().any(|i| i.pasta.starts_with(pasta));
        if !ocupada {
            let caminho: Vec<String> = pasta.iter().map(|p| percentual(p)).collect();
            texto.push_str(&format!("[Sessions\\{}/]\n\n", caminho.join("/")));
        }
    }
    if cifra.grandes > 0 {
        resultado.avisar(t!("exportar-winscp-grandes", n = cifra.grandes as i64));
    }
    if varios_saltos > 0 {
        resultado.avisar(t!("exportar-winscp-saltos", n = varios_saltos as i64));
    }
    resultado.texto = texto;
    resultado
}
