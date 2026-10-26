//! O catálogo das portas conhecidas e dos apps que a sonda procura (M58). Só dados: os testes
//! conferem que nenhuma porta se repete.

/// O tipo do que escuta numa porta conhecida.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tipo {
    Web,
    WebHttps,
    Banco,
    Fila,
    Painel,
    Sistema,
    Outro,
}

/// Uma porta do catálogo. `http` diz se a sonda pode mandar um `GET` (os bancos não levam, para
/// não sujar o log deles com "invalid startup packet").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemPorta {
    pub porta: u16,
    pub nome: &'static str,
    pub tipo: Tipo,
    pub http: bool,
}

const fn item(porta: u16, nome: &'static str, tipo: Tipo, http: bool) -> ItemPorta {
    ItemPorta {
        porta,
        nome,
        tipo,
        http,
    }
}

/// As portas conhecidas.
pub const PORTAS: &[ItemPorta] = &[
    item(80, "Web", Tipo::Web, true),
    item(8080, "Web", Tipo::Web, true),
    item(8000, "Web", Tipo::Web, true),
    item(8008, "Web", Tipo::Web, true),
    item(8081, "Web", Tipo::Web, true),
    item(8888, "Web", Tipo::Web, true),
    item(3000, "Web", Tipo::Web, true),
    item(4200, "Web", Tipo::Web, true),
    item(5000, "Web", Tipo::Web, true),
    item(5173, "Web", Tipo::Web, true),
    item(443, "Web (HTTPS)", Tipo::WebHttps, true),
    item(8443, "Web (HTTPS)", Tipo::WebHttps, true),
    item(3306, "MySQL/MariaDB", Tipo::Banco, false),
    item(5432, "PostgreSQL", Tipo::Banco, false),
    item(6379, "Redis", Tipo::Banco, false),
    item(27017, "MongoDB", Tipo::Banco, false),
    item(11211, "Memcached", Tipo::Banco, false),
    item(1433, "SQL Server", Tipo::Banco, false),
    item(1521, "Oracle", Tipo::Banco, false),
    item(9200, "Elasticsearch", Tipo::Banco, true),
    item(5672, "RabbitMQ", Tipo::Fila, false),
    item(15672, "RabbitMQ (painel)", Tipo::Painel, true),
    item(9000, "PHP-FPM ou web", Tipo::Outro, false),
    item(9090, "Painel web", Tipo::Painel, true),
    item(9443, "Painel web (HTTPS)", Tipo::WebHttps, true),
    item(10000, "Painel web (HTTPS)", Tipo::WebHttps, true),
    item(5050, "pgAdmin", Tipo::Painel, true),
    item(5601, "Kibana", Tipo::Painel, true),
    item(2375, "API do Docker (sem TLS)", Tipo::Sistema, false),
    item(5900, "VNC", Tipo::Outro, false),
    item(22, "SSH", Tipo::Sistema, false),
    item(25, "SMTP", Tipo::Sistema, false),
    item(53, "DNS", Tipo::Sistema, false),
    item(111, "rpcbind", Tipo::Sistema, false),
    item(631, "Impressão (IPP)", Tipo::Sistema, false),
];

/// A porta `n` do catálogo, se houver.
#[must_use]
pub fn porta(n: u16) -> Option<&'static ItemPorta> {
    PORTAS.iter().find(|item| item.porta == n)
}

/// Se a sonda pode mandar um `GET` à porta: as que não estão no catálogo valem, as marcadas
/// "não HTTP" não.
#[must_use]
pub fn aceita_get(n: u16) -> bool {
    porta(n).is_none_or(|item| item.http)
}

/// Um app web que costuma morar num caminho do servidor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct App {
    pub nome: &'static str,
    pub caminhos: &'static [&'static str],
    /// O pedaço do `<title>` que o identifica (sem diferenciar maiúsculas).
    pub titulo: &'static str,
}

/// Os apps que a sonda procura nas portas web.
pub const APPS: &[App] = &[
    App {
        nome: "phpMyAdmin",
        caminhos: &["/phpmyadmin/", "/phpMyAdmin/"],
        titulo: "phpmyadmin",
    },
    App {
        nome: "Adminer",
        caminhos: &["/adminer.php", "/adminer/"],
        titulo: "adminer",
    },
    App {
        nome: "pgAdmin",
        caminhos: &["/pgadmin4/"],
        titulo: "pgadmin",
    },
    App {
        nome: "Roundcube",
        caminhos: &["/webmail/", "/roundcube/"],
        titulo: "roundcube",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn nenhuma_porta_se_repete() {
        let mut vistas = HashSet::new();
        for item in PORTAS {
            assert!(vistas.insert(item.porta), "porta repetida: {}", item.porta);
        }
    }

    #[test]
    fn todo_app_tem_caminho_e_titulo() {
        for app in APPS {
            assert!(!app.caminhos.is_empty(), "{}", app.nome);
            assert!(!app.titulo.is_empty(), "{}", app.nome);
            assert_eq!(app.titulo, app.titulo.to_lowercase(), "{}", app.nome);
            for caminho in app.caminhos {
                assert!(caminho.starts_with('/'), "{caminho}");
            }
        }
    }

    #[test]
    fn os_bancos_nao_recebem_get_e_o_desconhecido_recebe() {
        for banco in [3306, 5432, 6379, 27017, 11211, 1433, 1521, 9000, 22] {
            assert!(!aceita_get(banco), "{banco}");
        }
        for web in [80, 8080, 3000, 443, 9200, 15672] {
            assert!(aceita_get(web), "{web}");
        }
        assert!(aceita_get(4567), "fora do catálogo vale tentar");
    }

    #[test]
    fn a_porta_do_catalogo_traz_nome_e_tipo() {
        let mysql = porta(3306).unwrap();
        assert_eq!(mysql.nome, "MySQL/MariaDB");
        assert_eq!(mysql.tipo, Tipo::Banco);
        assert_eq!(porta(10000).unwrap().tipo, Tipo::WebHttps);
        assert!(porta(4567).is_none());
    }
}
