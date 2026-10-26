//! Os endereços que o app monta para uma porta encaminhada (M58).

use super::local::apelido_do_site;

/// O endereço que o navegador abre: `http://localhost:3307/` ou, com "Nome do site",
/// `http://loja-exemplo-com-br.localhost:3307/caminho`. É sempre `http`: quando o servidor
/// fala TLS, quem fala TLS com ele é o relé.
#[must_use]
pub fn url_local(porta: u16, caminho: &str, host_http: &str) -> String {
    let caminho = if caminho.starts_with('/') {
        caminho.to_owned()
    } else {
        format!("/{caminho}")
    };
    if host_http.trim().is_empty() {
        format!("http://localhost:{porta}{caminho}")
    } else {
        format!(
            "http://{}.localhost:{porta}{caminho}",
            apelido_do_site(host_http)
        )
    }
}

/// O endereço de quem se conecta com o cliente do banco ou do que for: `127.0.0.1:3307`.
#[must_use]
pub fn endereco_local(porta: u16) -> String {
    format!("127.0.0.1:{porta}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_sem_nome_do_site_usa_o_localhost() {
        assert_eq!(url_local(3307, "/", ""), "http://localhost:3307/");
        assert_eq!(url_local(8080, "", ""), "http://localhost:8080/");
        assert_eq!(
            url_local(8080, "phpmyadmin/", "  "),
            "http://localhost:8080/phpmyadmin/"
        );
    }

    #[test]
    fn a_url_com_nome_do_site_usa_o_apelido() {
        assert_eq!(
            url_local(8080, "/loja", "loja.exemplo.com.br"),
            "http://loja-exemplo-com-br.localhost:8080/loja"
        );
    }

    #[test]
    fn o_endereco_local_e_o_loopback_com_a_porta() {
        assert_eq!(endereco_local(5433), "127.0.0.1:5433");
    }
}
