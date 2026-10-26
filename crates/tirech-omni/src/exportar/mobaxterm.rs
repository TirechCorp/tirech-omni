//! O `.mxtsessions` do MobaXterm: um INI com uma seção `[Bookmarks…]` por pasta (`SubRep` com as
//! pastas separadas por `\`) e uma linha por sessão SSH. As senhas ficam no registro do Windows,
//! cifradas, e não saem.

use super::{ItemPlano, NomesUnicos, Plano, Resultado};
use crate::modelo::Kind;
use crate::t;

/// Uma seção do arquivo: a pasta (`SubRep`) e as sessões que moram direto nela.
fn secao(numero: usize, pasta: &[String], itens: &[&ItemPlano], saida: &mut String) {
    if numero == 0 {
        saida.push_str("[Bookmarks]\n");
    } else {
        saida.push_str(&format!("\n[Bookmarks_{numero}]\n"));
    }
    let limpo = |t: &str| t.replace(['\\', '/'], "-");
    let caminho: Vec<String> = pasta.iter().map(|p| limpo(p)).collect();
    saida.push_str(&format!("SubRep={}\nImgNum=41\n", caminho.join("\\")));
    let mut nomes = NomesUnicos::default();
    for item in itens {
        let c = &item.conexao;
        let nome = nomes.unico(&c.name.replace(['=', '\r', '\n'], "-"));
        let chave = c
            .key_file
            .as_deref()
            .map(|k| k.to_string_lossy().replace(['%', '#'], "_"))
            .unwrap_or_default();
        // Os campos entre `%` que o MobaXterm grava numa sessão SSH; o 14 é a chave privada.
        saida.push_str(&format!(
            "{nome}=#109#0%{}%{}%{}%%-1%-1%%%22%%0%0%0%{chave}%%-1%0%0%0%%1080%%0%0%1#MobaXterm#0#\n",
            c.host.replace(['%', '#'], "_"),
            c.port,
            c.user.replace(['%', '#'], "_"),
        ));
    }
}

/// Escreve o arquivo.
pub fn exportar(plano: &Plano, itens: &[&ItemPlano]) -> Resultado {
    let mut resultado = Resultado::default();
    let mut saida = String::new();
    // A raiz primeiro e depois cada pasta (também as vazias), com as sessões que são dela.
    let mut caminhos: Vec<&[String]> = vec![&[]];
    caminhos.extend(plano.pastas.iter().map(Vec::as_slice));
    for (numero, caminho) in caminhos.into_iter().enumerate() {
        let daqui: Vec<&ItemPlano> = itens
            .iter()
            .copied()
            .filter(|i| i.conexao.kind == Kind::Ssh && i.pasta == caminho)
            .collect();
        secao(numero, caminho, &daqui, &mut saida);
    }
    resultado.texto = saida;
    let com_salto = itens.iter().filter(|i| !i.saltos.is_empty()).count();
    if com_salto > 0 {
        resultado.avisar(t!("exportar-moba-salto", n = com_salto as i64));
    }
    let com_tunel = itens
        .iter()
        .filter(|i| !i.conexao.tuneis.is_empty() || i.proxy.is_some())
        .count();
    if com_tunel > 0 {
        resultado.avisar(t!("exportar-moba-tunel", n = com_tunel as i64));
    }
    resultado
}
