//! O `.reg` do PuTTY (o KiTTY lê o mesmo, pela chave `SimonTatham`). Não leva senha de
//! conexão; a pasta vira o prefixo do nome da sessão (`Clientes/ACME`).

use super::{ItemPlano, NomesUnicos, Plano, Resultado};
use crate::importar::codificar_percentual;
use crate::modelo::{Kind, ModoX11, TipoTunel, Tunel};
use crate::redes::TipoProxy;
use crate::t;

const CHAVE: &str = r"HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions";

/// Um texto de valor do `.reg`: `\` e `"` com a barra na frente.
fn texto(valor: &str) -> String {
    let mut saida = String::with_capacity(valor.len() + 2);
    saida.push('"');
    for c in valor.chars() {
        if matches!(c, '\\' | '"') {
            saida.push('\\');
        }
        saida.push(c);
    }
    saida.push('"');
    saida
}

fn linha_texto(nome: &str, valor: &str) -> String {
    format!("\"{nome}\"={}\n", texto(valor))
}

fn linha_numero(nome: &str, valor: u32) -> String {
    format!("\"{nome}\"=dword:{valor:08x}\n")
}

/// O item do `PortForwardings`, ou `None` para o que o PuTTY não tem (socket Unix).
fn tunel(t: &Tunel) -> Option<String> {
    if t.problema().is_some() {
        return None;
    }
    match t.tipo {
        TipoTunel::Local => Some(format!("L{}={}", t.porta, t.destino())),
        TipoTunel::Remoto => Some(format!("R{}={}", t.porta, t.destino())),
        TipoTunel::Dinamico => Some(format!("D{}", t.porta)),
        TipoTunel::SocketUnix => None,
    }
}

/// O nome da sessão: as pastas e o nome, com `/` entre eles e `%XX` no que o registro não
/// gosta.
pub(super) fn nome_da_sessao(item: &ItemPlano) -> String {
    let limpo = |t: &str| t.replace(['/', '\\'], "-");
    let mut partes: Vec<String> = item.pasta.iter().map(|p| limpo(p)).collect();
    partes.push(limpo(&item.conexao.name));
    partes.join("/")
}

/// Escreve o arquivo.
pub fn exportar(_plano: &Plano, itens: &[&ItemPlano]) -> Resultado {
    let mut resultado = Resultado::default();
    let mut nomes = NomesUnicos::default();
    let mut texto_reg = String::from("Windows Registry Editor Version 5.00\n");
    let mut com_salto = 0usize;
    let mut sem_tunel = 0usize;
    for item in itens {
        let c = &item.conexao;
        let nome = nomes.unico(&nome_da_sessao(item));
        // A barra entre a pasta e o nome sai como `%2F`; o importador a desfaz e divide ali, o que
        // é o que o KiTTY faz.
        texto_reg.push_str(&format!(
            "\n[{CHAVE}\\{}]\n",
            codificar_percentual(&nome, "-_.")
        ));
        texto_reg.push_str(&linha_texto("HostName", &c.host));
        texto_reg.push_str(&linha_texto(
            "Protocol",
            if c.kind == Kind::Telnet {
                "telnet"
            } else {
                "ssh"
            },
        ));
        texto_reg.push_str(&linha_numero("PortNumber", u32::from(c.port)));
        if !c.user.is_empty() {
            texto_reg.push_str(&linha_texto("UserName", &c.user));
        }
        if c.term != "xterm-256color" && !c.term.is_empty() {
            texto_reg.push_str(&linha_texto("TerminalType", &c.term));
        }
        if c.kind == Kind::Ssh {
            if let Some(chave) = c.key_file.as_deref() {
                texto_reg.push_str(&linha_texto("PublicKeyFile", &chave.to_string_lossy()));
            }
            match c.x11 {
                ModoX11::Padrao => {}
                ModoX11::Desligado => texto_reg.push_str(&linha_numero("X11Forward", 0)),
                ModoX11::NaoConfiavel | ModoX11::Confiavel => {
                    texto_reg.push_str(&linha_numero("X11Forward", 1));
                }
            }
            if c.keepalive_secs != 30 {
                texto_reg.push_str(&linha_numero("PingIntervalSecs", c.keepalive_secs));
            }
            let mut lista = Vec::new();
            for t in &c.tuneis {
                match tunel(t) {
                    Some(item) => lista.push(item),
                    None => sem_tunel += 1,
                }
            }
            if !lista.is_empty() {
                texto_reg.push_str(&linha_texto("PortForwardings", &lista.join(",")));
                let aberto = |remoto: bool| {
                    c.tuneis
                        .iter()
                        .any(|t| t.outras_maquinas && (t.tipo == TipoTunel::Remoto) == remoto)
                };
                if aberto(false) {
                    texto_reg.push_str(&linha_numero("LocalPortAcceptAll", 1));
                }
                if aberto(true) {
                    texto_reg.push_str(&linha_numero("RemotePortAcceptAll", 1));
                }
            }
        }
        if let Some(proxy) = &item.proxy {
            texto_reg.push_str(&linha_numero(
                "ProxyMethod",
                if proxy.tipo == TipoProxy::Socks5 {
                    2
                } else {
                    3
                },
            ));
            texto_reg.push_str(&linha_texto("ProxyHost", &proxy.host));
            texto_reg.push_str(&linha_numero("ProxyPort", u32::from(proxy.port)));
            if !proxy.usuario.is_empty() {
                texto_reg.push_str(&linha_texto("ProxyUsername", &proxy.usuario));
            }
        }
        if !item.saltos.is_empty() {
            com_salto += 1;
        }
    }
    if com_salto > 0 {
        resultado.avisar(t!("exportar-putty-salto", n = com_salto as i64));
    }
    if sem_tunel > 0 {
        resultado.avisar(t!("exportar-putty-tunel", n = sem_tunel as i64));
    }
    resultado.texto = texto_reg;
    resultado
}
