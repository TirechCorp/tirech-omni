//! O `~/.ssh/config` do OpenSSH: um `Host` por conexão SSH. Não guarda senha.

use super::{ItemPlano, NomesUnicos, Plano, Resultado, salto_em_texto};
use crate::modelo::{ModoX11, TipoTunel, Tunel};
use crate::t;

/// O `Host` sem espaços nem curingas (`*`, `?`, `!` e `#` viram `-`, como os espaços).
fn nome_do_host(nome: &str, host: &str) -> String {
    let limpo: String = nome
        .trim()
        .chars()
        .map(|c| {
            if c.is_whitespace() || matches!(c, '*' | '?' | '!' | '#' | '"' | '\'' | ',') {
                '-'
            } else {
                c
            }
        })
        .collect();
    if limpo.trim_matches('-').is_empty() {
        host.to_owned()
    } else {
        limpo
    }
}

/// Um valor que tem espaço vai entre aspas.
fn valor(texto: &str) -> String {
    if texto.contains(char::is_whitespace) {
        format!("\"{texto}\"")
    } else {
        texto.to_owned()
    }
}

/// `[*:]porta`: o `*` abre para as outras máquinas.
fn origem(tunel: &Tunel) -> String {
    if tunel.outras_maquinas {
        format!("*:{}", tunel.porta)
    } else {
        tunel.porta.to_string()
    }
}

/// A linha de um túnel, ou `None` quando ele não tem como virar uma diretiva do OpenSSH.
fn tunel(t: &Tunel) -> Option<String> {
    if t.problema().is_some() {
        return None;
    }
    match t.tipo {
        TipoTunel::Local => Some(format!("LocalForward {} {}", origem(t), t.destino())),
        TipoTunel::Remoto => Some(format!("RemoteForward {} {}", origem(t), t.destino())),
        TipoTunel::Dinamico => Some(format!("DynamicForward {}", origem(t))),
        TipoTunel::SocketUnix => {
            let local = t.socket_local.trim();
            let de = if !local.is_empty() {
                valor(local)
            } else if t.porta > 0 {
                origem(t)
            } else {
                // Um socket "automático" não tem nome para o OpenSSH.
                return None;
            };
            Some(format!(
                "LocalForward {de} {}",
                valor(t.caminho_remoto.trim())
            ))
        }
    }
}

/// Escreve o arquivo.
pub fn exportar(_plano: &Plano, itens: &[&ItemPlano]) -> Resultado {
    let mut resultado = Resultado::default();
    let mut nomes = NomesUnicos::default();
    let mut texto = String::from("# Exportado pelo Tirech Omni\n");
    let mut com_proxy = 0usize;
    let mut sem_tunel = 0usize;
    for item in itens {
        let c = &item.conexao;
        let host = nomes.unico(&nome_do_host(&c.name, &c.host));
        texto.push_str(&format!("\nHost {host}\n    HostName {}\n", c.host));
        if !c.user.is_empty() {
            texto.push_str(&format!("    User {}\n", c.user));
        }
        if c.port != 22 && c.port != 0 {
            texto.push_str(&format!("    Port {}\n", c.port));
        }
        if let Some(chave) = c.key_file.as_deref() {
            texto.push_str(&format!(
                "    IdentityFile {}\n",
                valor(&chave.to_string_lossy())
            ));
        }
        if !item.saltos.is_empty() {
            let lista: Vec<String> = item
                .saltos
                .iter()
                .map(|s| salto_em_texto(&s.salto.host, s.salto.port, &s.salto.user))
                .collect();
            texto.push_str(&format!("    ProxyJump {}\n", lista.join(",")));
        }
        match c.x11 {
            ModoX11::Padrao => {}
            ModoX11::Desligado => texto.push_str("    ForwardX11 no\n"),
            ModoX11::NaoConfiavel => texto.push_str("    ForwardX11 yes\n"),
            ModoX11::Confiavel => {
                texto.push_str("    ForwardX11 yes\n    ForwardX11Trusted yes\n");
            }
        }
        for t in &c.tuneis {
            match tunel(t) {
                Some(linha) => texto.push_str(&format!("    {linha}\n")),
                None => sem_tunel += 1,
            }
        }
        if c.parar_se_tunel_falhar && !c.tuneis.is_empty() {
            texto.push_str("    ExitOnForwardFailure yes\n");
        }
        if item.proxy.is_some() {
            com_proxy += 1;
        }
    }
    if com_proxy > 0 {
        resultado.avisar(t!("exportar-ssh-config-proxy", n = com_proxy as i64));
    }
    if sem_tunel > 0 {
        resultado.avisar(t!("exportar-ssh-config-tunel", n = sem_tunel as i64));
    }
    resultado.texto = texto;
    resultado
}
