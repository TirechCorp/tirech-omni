//! O `connections.xml` do WinSSHTerm: `Node`s `Container` (pasta) e `Connection`. As senhas dele
//! ficam cifradas com o arquivo `key` do programa, então o arquivo sai sem elas.

use super::plano::{NoPlano, PastaPlano};
use super::{ItemPlano, Plano, Resultado, escapar_xml};
use crate::modelo::ModoX11;
use crate::redes::TipoProxy;
use crate::t;

fn conexao(item: &ItemPlano, recuo: &str, saida: &mut String) {
    let c = &item.conexao;
    let x11 = match c.x11 {
        ModoX11::Padrao => "",
        ModoX11::Desligado => "don't forward",
        ModoX11::NaoConfiavel | ModoX11::Confiavel => "forward",
    };
    let chave = c
        .key_file
        .as_deref()
        .map(|k| k.to_string_lossy().into_owned())
        .unwrap_or_default();
    saida.push_str(&format!(
        "{recuo}<Node Name=\"{}\" Type=\"Connection\" Descr=\"{}\" Username=\"{}\" Password=\"\" \
         PrivateKey=\"{}\" Hostname=\"{}\" Port=\"{}\" Certificate=\"\" LaunchToolInt=\"\" \
         sX11=\"{x11}\" cfProt=\"sftp\" ",
        escapar_xml(&c.name),
        escapar_xml(&c.descricao),
        escapar_xml(&c.user),
        escapar_xml(&chave),
        escapar_xml(&c.host),
        c.port,
    ));
    match &item.proxy {
        Some(proxy) => saida.push_str(&format!(
            "pSshProxy=\"enabled\" pType=\"{}\" pHost=\"{}\" pPort=\"{}\" pUser=\"{}\" />\n",
            if proxy.tipo == TipoProxy::Socks5 {
                "SOCKS5"
            } else {
                "HTTP"
            },
            escapar_xml(&proxy.host),
            proxy.port,
            escapar_xml(&proxy.usuario),
        )),
        None => saida
            .push_str("pSshProxy=\"disabled\" pType=\"\" pHost=\"\" pPort=\"\" pUser=\"\" />\n"),
    }
}

fn pasta(p: &PastaPlano, recuo: &str, saida: &mut String) {
    saida.push_str(&format!(
        "{recuo}<Node Name=\"{}\" Type=\"Container\" Expanded=\"False\">\n",
        escapar_xml(&p.nome)
    ));
    nos(&p.filhos, &format!("{recuo}  "), saida);
    saida.push_str(&format!("{recuo}</Node>\n"));
}

fn nos(lista: &[NoPlano], recuo: &str, saida: &mut String) {
    for no in lista {
        match no {
            NoPlano::Pasta(p) => pasta(p, recuo, saida),
            NoPlano::Item(item) => conexao(item, recuo, saida),
        }
    }
}

/// Escreve o arquivo.
pub fn exportar(plano: &Plano, itens: &[&ItemPlano]) -> Resultado {
    let mut resultado = Resultado::default();
    let mut saida =
        String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<WinSSHTerm Version=\"1\">\n");
    nos(&plano.aninhar(itens), "  ", &mut saida);
    saida.push_str("</WinSSHTerm>\n");
    resultado.texto = saida;
    let com_salto = itens.iter().filter(|i| !i.saltos.is_empty()).count();
    if com_salto > 0 {
        resultado.avisar(t!("exportar-winsshterm-salto", n = com_salto as i64));
    }
    let com_tunel = itens
        .iter()
        .filter(|i| !i.conexao.tuneis.is_empty())
        .count();
    if com_tunel > 0 {
        resultado.avisar(t!("exportar-winsshterm-tunel", n = com_tunel as i64));
    }
    resultado
}
