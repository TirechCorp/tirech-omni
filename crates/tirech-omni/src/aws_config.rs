//! Importa os perfis do `~/.aws`: o `credentials` (as chaves) e o `config` (a região e o
//! `endpoint_url`). Cada perfil com chaves vira uma conexão S3.

use crate::modelo::{Connection, Kind, S3Opcoes, S3Provedor};
use crate::t;

/// Um perfil do `~/.aws`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Perfil {
    /// O nome (`default`, `trabalho`…).
    pub nome: String,
    /// O Access Key ID.
    pub access_key: String,
    /// O Secret Access Key.
    pub secret_key: String,
    /// O token de sessão, se as chaves são temporárias.
    pub token: Option<String>,
    /// A região.
    pub region: Option<String>,
    /// O endereço do serviço, quando não é o da Amazon.
    pub endpoint: Option<String>,
}

/// O que a importação achou.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Importacao {
    /// Os perfis com chaves, na ordem em que aparecem.
    pub perfis: Vec<Perfil>,
    /// O que ficou de fora e por quê (para a tela mostrar).
    pub avisos: Vec<String>,
}

/// Lê o texto do `credentials` e do `config` (este pode ser vazio) e junta por perfil.
pub fn parse(credentials: &str, config: &str) -> Importacao {
    let mut perfis: Vec<Perfil> = Vec::new();
    let indice = |nome: &str, perfis: &mut Vec<Perfil>| -> usize {
        match perfis.iter().position(|p| p.nome == nome) {
            Some(i) => i,
            None => {
                perfis.push(Perfil {
                    nome: nome.to_owned(),
                    ..Perfil::default()
                });
                perfis.len() - 1
            }
        }
    };

    for (arquivo_config, texto) in [(false, credentials), (true, config)] {
        // `None`: fora de um perfil (uma seção que não se importa, como `[sso-session x]`).
        let mut atual: Option<usize> = None;
        for linha in texto.lines() {
            let linha = linha.trim();
            if linha.is_empty() || linha.starts_with('#') || linha.starts_with(';') {
                continue;
            }
            if let Some(secao) = linha.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let secao = secao.trim();
                let nome = if arquivo_config {
                    match secao.strip_prefix("profile") {
                        Some(resto) if resto.starts_with(char::is_whitespace) => Some(resto.trim()),
                        _ if secao == "default" => Some("default"),
                        _ => None,
                    }
                } else {
                    Some(secao)
                };
                atual = nome
                    .filter(|n| !n.is_empty())
                    .map(|n| indice(n, &mut perfis));
                continue;
            }
            let (Some(i), Some((chave, valor))) = (atual, linha.split_once('=')) else {
                continue;
            };
            let (chave, valor) = (chave.trim().to_ascii_lowercase(), valor.trim());
            if valor.is_empty() {
                continue;
            }
            let perfil = &mut perfis[i];
            match chave.as_str() {
                "aws_access_key_id" => perfil.access_key = valor.to_owned(),
                "aws_secret_access_key" => perfil.secret_key = valor.to_owned(),
                "aws_session_token" => perfil.token = Some(valor.to_owned()),
                "region" => perfil.region = Some(valor.to_owned()),
                "endpoint_url" => perfil.endpoint = Some(valor.trim_end_matches('/').to_owned()),
                _ => {}
            }
        }
    }

    let mut saida = Importacao::default();
    for perfil in perfis {
        if perfil.access_key.is_empty() || perfil.secret_key.is_empty() {
            saida
                .avisos
                .push(t!("aws-perfil-sem-chaves", perfil = perfil.nome.as_str()));
        } else {
            saida.perfis.push(perfil);
        }
    }
    saida
}

impl Perfil {
    /// A conexão S3 do perfil: o provedor sai do `endpoint_url` quando é de um conhecido.
    pub fn conexao(&self) -> Connection {
        let mut c = Connection::new(self.nome.clone(), Kind::S3);
        c.user = self.access_key.clone();
        let provedor = match self.endpoint.as_deref() {
            None => S3Provedor::Amazon,
            Some(endereco) => provedor_de(endereco),
        };
        let mut s3 = S3Opcoes::do_provedor(provedor);
        if let Some(regiao) = &self.region {
            s3.trocar_regiao(regiao);
        }
        if let Some(endereco) = &self.endpoint {
            s3.endpoint = endereco.clone();
        }
        c.s3 = s3;
        c
    }
}

/// O provedor que serve o `endereco`; `Outro` se não for de nenhum conhecido.
fn provedor_de(endereco: &str) -> S3Provedor {
    let endereco = endereco.to_ascii_lowercase();
    if endereco.contains("amazonaws.com") {
        S3Provedor::Amazon
    } else if endereco.contains("r2.cloudflarestorage.com") {
        S3Provedor::CloudflareR2
    } else if endereco.contains("wasabisys.com") {
        S3Provedor::Wasabi
    } else if endereco.contains("backblazeb2.com") {
        S3Provedor::BackblazeB2
    } else if endereco.contains("digitaloceanspaces.com") {
        S3Provedor::DigitalOcean
    } else if endereco.contains(":9000") {
        S3Provedor::Minio
    } else {
        S3Provedor::Outro
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const CREDENCIAIS: &str = "\
# o perfil de sempre
[default]
aws_access_key_id = AKIADEFAULT
aws_secret_access_key = segredo-um

[minio]
aws_access_key_id=minioadmin
aws_secret_access_key=minioadmin
aws_session_token = tok

[sem-chaves]
region = us-west-2
";

    const CONFIG: &str = "\
[default]
region = sa-east-1

[profile minio]
region = us-east-1
endpoint_url = http://127.0.0.1:19000/

[sso-session x]
sso_region = us-east-1
";

    #[test]
    fn junta_o_credentials_e_o_config_por_perfil() {
        let i = parse(CREDENCIAIS, CONFIG);
        assert_eq!(i.perfis.len(), 2);
        assert_eq!(i.perfis[0].nome, "default");
        assert_eq!(i.perfis[0].access_key, "AKIADEFAULT");
        assert_eq!(i.perfis[0].region.as_deref(), Some("sa-east-1"));
        assert_eq!(i.perfis[0].endpoint, None);
        assert_eq!(i.perfis[1].nome, "minio");
        assert_eq!(i.perfis[1].token.as_deref(), Some("tok"));
        assert_eq!(
            i.perfis[1].endpoint.as_deref(),
            Some("http://127.0.0.1:19000")
        );
    }

    #[test]
    fn perfil_sem_chaves_vira_aviso() {
        let i = parse(CREDENCIAIS, CONFIG);
        assert_eq!(i.avisos.len(), 1);
        assert!(i.avisos[0].contains("sem-chaves"));
    }

    #[test]
    fn a_conexao_leva_o_endereco_e_a_regiao() {
        let i = parse(CREDENCIAIS, CONFIG);
        let amazon = i.perfis[0].conexao();
        assert_eq!(amazon.kind, Kind::S3);
        assert_eq!(amazon.user, "AKIADEFAULT");
        assert_eq!(amazon.s3.provider, S3Provedor::Amazon);
        assert_eq!(amazon.s3.region, "sa-east-1");
        assert_eq!(amazon.s3.endpoint, "https://s3.sa-east-1.amazonaws.com");
        let minio = i.perfis[1].conexao();
        assert_eq!(minio.s3.endpoint, "http://127.0.0.1:19000");
        assert_eq!(minio.s3.region, "us-east-1");
        assert_ne!(minio.s3.provider, S3Provedor::Amazon);
    }

    #[test]
    fn arquivo_vazio_nao_acha_nada() {
        assert_eq!(parse("", ""), Importacao::default());
    }
}
