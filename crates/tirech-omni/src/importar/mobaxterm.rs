//! O MobaXterm: o `.mobaconf` / `MobaXterm.ini` e o `.mxtsessions` exportado. É um INI com
//! `[Bookmarks]`, `[Bookmarks_1]`…; cada seção tem `SubRep` (a pasta, com `\`) e uma linha por
//! sessão: `Nome=#ícone#tipo%servidor%porta%usuário%…%chave(campo 14)%…#cores…`.
//! Lê SSH, Telnet, FTP e SFTP. As senhas ficam no registro do Windows, cifradas: não vêm.

use std::path::{Path, PathBuf};

use super::{
    Formato, Ignorado, Importacao, Secao, Trazida, avisar_ppk, ler_ini, nao_suportadas,
    nome_da_conexao, porta_ou,
};
use crate::modelo::{Connection, Kind};
use crate::t;

/// O formato MobaXterm.
pub struct MobaXterm;

impl Formato for MobaXterm {
    fn id(&self) -> &'static str {
        "mobaxterm"
    }

    fn nome(&self) -> &'static str {
        "MobaXterm"
    }

    fn arquivo_padrao(&self) -> &'static str {
        "MobaXterm.ini"
    }

    fn extensoes(&self) -> &'static [&'static str] {
        &["mobaconf", "mxtsessions", "ini"]
    }

    fn locais_provaveis(&self, home: &Path) -> Vec<PathBuf> {
        vec![
            home.join("Documents/MobaXterm/MobaXterm.ini"),
            home.join("AppData/Roaming/MobaXterm/MobaXterm.ini"),
        ]
    }

    fn analisar(&self, texto: &str, _senha_mestra: Option<&str>) -> Result<Importacao, String> {
        analisar(texto)
    }
}

/// As pastas que o MobaXterm usa para sessões de outros programas (ficam no registro).
const PASTAS_DE_OUTROS: [&str; 3] = ["PuTTY sessions", "SCRT sessions", "SuperPuTTY sessions"];

/// O nome do tipo de sessão que não é suportado.
fn nome_do_tipo(codigo: u32) -> String {
    match codigo {
        2 => "Rsh".to_owned(),
        3 => "XDMCP".to_owned(),
        4 => "RDP".to_owned(),
        5 => "VNC".to_owned(),
        8 => "serial".to_owned(),
        9 => t!("importar-moba-tipo-arquivo"),
        10 => "shell".to_owned(),
        11 => t!("importar-moba-tipo-navegador"),
        12 => "Mosh".to_owned(),
        13 => "AWS S3".to_owned(),
        14 => "WSL".to_owned(),
        n => t!("importar-de-codigo", n = i64::from(n)),
    }
}

fn analisar(texto: &str) -> Result<Importacao, String> {
    let secoes = ler_ini(texto);
    let favoritas: Vec<&Secao> = secoes
        .iter()
        .filter(|s| s.nome.to_ascii_lowercase().starts_with("bookmarks"))
        .collect();
    if favoritas.is_empty() {
        return Err(t!("importar-moba-sem-sessoes"));
    }
    let mut importacao = Importacao::default();
    let mut ignoradas: Vec<(String, usize)> = Vec::new();
    for secao in favoritas {
        let pasta: Vec<String> = secao
            .texto_ou_vazio("SubRep")
            .split(['\\', '/'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
        if pasta
            .first()
            .is_some_and(|p| PASTAS_DE_OUTROS.iter().any(|o| o.eq_ignore_ascii_case(p)))
        {
            continue;
        }
        for fim in 1..=pasta.len() {
            importacao.pasta(pasta[..fim].to_vec());
        }
        for (nome, valor) in &secao.linhas {
            if nome.eq_ignore_ascii_case("SubRep") || nome.eq_ignore_ascii_case("ImgNum") {
                continue;
            }
            let Some(sessao) = ler_sessao(valor) else {
                continue;
            };
            let tipo = match sessao.tipo {
                0 => Kind::Ssh,
                1 => Kind::Telnet,
                6 => Kind::Ftp,
                7 => Kind::Sftp,
                outro => {
                    let nome = nome_do_tipo(outro);
                    match ignoradas.iter_mut().find(|(n, _)| *n == nome) {
                        Some((_, quantas)) => *quantas += 1,
                        None => ignoradas.push((nome, 1)),
                    }
                    continue;
                }
            };
            if sessao.host.is_empty() {
                continue;
            }
            let padrao = match tipo {
                Kind::Telnet => 23,
                Kind::Ftp => 21,
                _ => 22,
            };
            let mut c = Connection::new(nome_da_conexao(nome, &sessao.host), tipo);
            c.port = porta_ou(&sessao.porta, padrao);
            c.host = sessao.host;
            c.user = sessao.usuario;
            if !sessao.chave.is_empty() && matches!(tipo, Kind::Ssh | Kind::Sftp) {
                avisar_ppk(&mut importacao, &sessao.chave);
                c.key_file = Some(PathBuf::from(sessao.chave));
            }
            importacao.conexao(Trazida::nova(c, pasta.clone()));
        }
    }
    for (nome, quantas) in ignoradas {
        importacao.avisar(nao_suportadas(quantas, Ignorado::Sessao, &nome));
    }
    if importacao.total() > 0 {
        importacao.avisar(t!("importar-moba-senhas-no-registro"));
    }
    Ok(importacao)
}

/// Os campos de uma sessão que o importador usa.
struct Sessao {
    tipo: u32,
    host: String,
    porta: String,
    usuario: String,
    chave: String,
}

/// Lê `#109#0%servidor%22%usuário%…#cores…`. `None` se não parece uma sessão.
fn ler_sessao(valor: &str) -> Option<Sessao> {
    let resto = valor.trim().strip_prefix('#')?;
    let (_icone, resto) = resto.split_once('#')?;
    let campos: Vec<&str> = resto.split('#').next()?.split('%').collect();
    let campo = |i: usize| campos.get(i).copied().unwrap_or_default().trim().to_owned();
    Some(Sessao {
        tipo: campos.first()?.trim().parse().ok()?,
        host: campo(1),
        porta: campo(2),
        usuario: campo(3),
        chave: campo(14),
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::importar::Entrada;

    const AMOSTRA: &str = "[Bookmarks]\nSubRep=\nImgNum=42\n\
        Servidor raiz=#109#0%raiz.exemplo.com%22%root%%-1%-1%%%22%%0%0%0%%%-1%0%0%0%%1080%%0%0%1#MobaXterm#0#\n\
        \n[Bookmarks_1]\nSubRep=Clientes\\ACME\nImgNum=41\n\
        Web=#109#0%web.acme.com%2222%ana%%-1%-1%%%22%%0%0%0%C:\\chaves\\acme.ppk%%-1%0%0%0%%1080%%0%0%1#MobaXterm#0#\n\
        Roteador=#91#1%10.0.0.1%23%admin%%-1#MobaXterm#0#\n\
        Desktop=#91#4%pc.acme.com%3389%ana%%-1#MobaXterm#0#\n\
        Arquivos=#130#7%ftp.acme.com%22%ana%%-1#MobaXterm#0#\n\
        \n[Bookmarks_2]\nSubRep=PuTTY sessions\nImgNum=41\nAntiga=#98#0%velho.exemplo.com%22%x%%-1#a#\n\
        \n[Bookmarks_3]\nSubRep=Vazia\nImgNum=41\n";

    #[test]
    fn le_pastas_tipos_chave_e_avisos() {
        let i = MobaXterm.analisar(AMOSTRA, None).unwrap();
        assert_eq!(i.total(), 4);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.name, "Servidor raiz");
        assert!(c[0].pasta.is_empty());
        assert_eq!(c[1].conexao.name, "Web");
        assert_eq!(c[1].pasta, ["Clientes", "ACME"]);
        assert_eq!(c[1].conexao.port, 2222);
        assert_eq!(c[1].conexao.user, "ana");
        assert_eq!(
            c[1].conexao.key_file,
            Some(PathBuf::from("C:\\chaves\\acme.ppk"))
        );
        assert_eq!(c[2].conexao.kind, Kind::Telnet);
        assert_eq!(c[3].conexao.kind, Kind::Sftp);
        assert!(i.avisos.iter().any(|a| a.contains("1 sessão RDP")));
        assert!(i.avisos.iter().any(|a| a.contains("registro do Windows")));
        assert!(i.conexoes().all(|t| t.conexao.host != "velho.exemplo.com"));
        let pastas: Vec<Vec<String>> = i
            .entradas
            .iter()
            .filter_map(|e| match e {
                Entrada::Pasta(p) => Some(p.caminho.clone()),
                _ => None,
            })
            .collect();
        assert!(pastas.contains(&vec!["Clientes".to_owned()]));
        assert!(pastas.contains(&vec!["Vazia".to_owned()]));
    }

    #[test]
    fn arquivo_sem_favoritos_vira_erro() {
        assert!(MobaXterm.analisar("[Misc]\nA=1\n", None).is_err());
    }
}
