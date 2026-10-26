//! A porta do computador onde o túnel escuta, e o nome que o relé dá a cada site (M58).

use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, TcpListener};
use std::sync::OnceLock;

/// A porta daqui para um túnel: a `preferida` se estiver livre; se for menor que 1024 e estiver
/// ocupada ou sem permissão, `preferida + 8000` (80 → 8080); daí para cima, a primeira livre;
/// depois de 65535, de 1024 para cima. `None` só se nenhuma porta estiver livre.
pub fn escolher_porta_local(preferida: u16, livre: impl Fn(u16) -> bool) -> Option<u16> {
    if preferida != 0 && livre(preferida) {
        return Some(preferida);
    }
    let mut inicio = preferida;
    if (1..1024).contains(&preferida) {
        let alternativa = preferida + 8000;
        if livre(alternativa) {
            return Some(alternativa);
        }
        inicio = alternativa;
    }
    let piso = inicio.max(1023);
    (piso + 1..=u16::MAX)
        .chain(1024..=piso)
        .find(|&porta| livre(porta))
}

/// Se a porta está livre no loopback: o `bind` em `127.0.0.1` e, quando o IPv6 existe, também em
/// `[::1]`, porque os navegadores tentam `localhost` primeiro pelo `::1` e um programa local ali
/// roubaria o endereço.
#[must_use]
pub fn porta_livre(porta: u16) -> bool {
    if TcpListener::bind(("127.0.0.1", porta)).is_err() {
        return false;
    }
    !tem_ipv6() || ipv6_livre(porta)
}

fn ipv6_livre(porta: u16) -> bool {
    let endereco = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, porta, 0, 0));
    TcpListener::bind(endereco).is_ok()
}

/// Se este computador abre `[::1]`, conferido uma vez.
fn tem_ipv6() -> bool {
    static TEM: OnceLock<bool> = OnceLock::new();
    *TEM.get_or_init(|| ipv6_livre(0))
}

/// `loja.exemplo.com.br` → `loja-exemplo-com-br`: o nome do host em `*.localhost`.
#[must_use]
pub fn apelido_do_site(host: &str) -> String {
    let mut apelido = String::new();
    for c in host.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            apelido.push(c);
        } else if !apelido.ends_with('-') {
            apelido.push('-');
        }
    }
    let apelido = apelido.trim_matches('-');
    if apelido.is_empty() {
        "site".to_owned()
    } else {
        apelido.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn livre_exceto(ocupadas: &[u16]) -> impl Fn(u16) -> bool {
        let ocupadas: HashSet<u16> = ocupadas.iter().copied().collect();
        move |p| !ocupadas.contains(&p)
    }

    #[test]
    fn a_preferida_livre_vence() {
        assert_eq!(escolher_porta_local(3307, livre_exceto(&[])), Some(3307));
    }

    #[test]
    fn a_preferida_ocupada_vai_para_a_proxima_livre() {
        assert_eq!(
            escolher_porta_local(3307, livre_exceto(&[3307, 3308])),
            Some(3309)
        );
    }

    #[test]
    fn a_menor_que_1024_sem_permissao_soma_8000() {
        let sem_permissao = |p: u16| p >= 1024;
        assert_eq!(escolher_porta_local(80, sem_permissao), Some(8080));
        assert_eq!(escolher_porta_local(443, sem_permissao), Some(8443));
        assert_eq!(
            escolher_porta_local(80, |p| p >= 1024 && p != 8080),
            Some(8081)
        );
    }

    #[test]
    fn depois_de_65535_volta_para_1024() {
        let so_perto_do_fim_ocupado = |p: u16| p < 65530;
        assert_eq!(
            escolher_porta_local(65531, so_perto_do_fim_ocupado),
            Some(1024)
        );
        let livre = |p: u16| p == 2000;
        assert_eq!(escolher_porta_local(65531, livre), Some(2000));
    }

    #[test]
    fn tudo_ocupado_nao_devolve_nada() {
        assert_eq!(escolher_porta_local(3000, |_| false), None);
        assert_eq!(escolher_porta_local(80, |_| false), None);
    }

    #[test]
    fn o_ipv6_ocupado_conta_como_ocupado() {
        // Uma porta que o `127.0.0.1` ainda aceita, mas o `[::1]` não.
        let Ok(escuta) = TcpListener::bind("[::1]:0") else {
            return; // sem IPv6 neste computador
        };
        let porta = escuta.local_addr().unwrap().port();
        if TcpListener::bind(("127.0.0.1", porta)).is_ok() {
            assert!(!porta_livre(porta), "o `::1` ocupado precisa contar");
        }
        drop(escuta);
    }

    #[test]
    fn uma_porta_solta_esta_livre() {
        let escuta = TcpListener::bind("127.0.0.1:0").unwrap();
        let porta = escuta.local_addr().unwrap().port();
        assert!(!porta_livre(porta));
        drop(escuta);
        assert!(porta_livre(porta));
    }

    #[test]
    fn o_apelido_troca_pontos_por_hifens() {
        assert_eq!(
            apelido_do_site("loja.exemplo.com.br"),
            "loja-exemplo-com-br"
        );
        assert_eq!(apelido_do_site("  WWW.Exemplo.COM "), "www-exemplo-com");
        assert_eq!(apelido_do_site("a..b_c"), "a-b-c");
        assert_eq!(apelido_do_site("..."), "site");
    }
}
