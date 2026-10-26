//! Os caminhos que o assistente pede (M51): juntar com a raiz do espaço de trabalho, tirar `.` e
//! `..` e conferir que nada sai da pasta, nem por um link simbólico.
//!
//! Os caminhos são de servidor (separador `/`). A conferência é em duas partes: [`normalizar`]
//! resolve o texto sem tocar no disco; [`confinar`] pergunta ao servidor para onde o caminho de
//! verdade vai (`Fs::real_path`, o `realpath` do SFTP) e confere de novo.

use std::io::ErrorKind;

use tomni_ui::files::Fs;

use crate::t;

/// Os componentes de um caminho, sem os vazios e sem os `.`.
fn componentes(caminho: &str) -> Vec<&str> {
    caminho
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".")
        .collect()
}

fn juntar(componentes: &[&str]) -> String {
    format!("/{}", componentes.join("/"))
}

/// Se `caminho` está em `raiz` ou é a própria `raiz`, comparando por componentes (`/srv/app2`
/// não está em `/srv/app`). Um `..` no caminho dá `false`: normalize antes.
pub fn dentro(raiz: &str, caminho: &str) -> bool {
    let raiz = componentes(raiz);
    let caminho = componentes(caminho);
    !caminho.contains(&"..") && caminho.starts_with(&raiz)
}

/// Junta `caminho` à `raiz` (se for relativo), tira `.` e `..` e devolve o caminho absoluto, ou
/// o erro, em texto para o assistente, se sai da pasta do espaço.
///
/// ```
/// use tirech_omni::mcp::caminho::normalizar;
///
/// assert_eq!(normalizar("/srv/app", "src/../lib/a.rs").unwrap(), "/srv/app/lib/a.rs");
/// assert!(normalizar("/srv/app", "../outro").is_err());
/// ```
pub fn normalizar(raiz: &str, caminho: &str) -> Result<String, String> {
    let mut saida: Vec<&str> = if caminho.starts_with('/') {
        Vec::new()
    } else {
        componentes(raiz)
    };
    for c in componentes(caminho) {
        if c == ".." {
            saida.pop();
        } else {
            saida.push(c);
        }
    }
    let resultado = juntar(&saida);
    if dentro(raiz, &resultado) {
        Ok(resultado)
    } else {
        Err(t!("mcp-caminho-sai", caminho = caminho.to_string()))
    }
}

/// O caminho de um arquivo dentro do espaço, como o assistente o vê: relativo à `raiz` (`.` para
/// a própria raiz).
pub fn relativo(raiz: &str, caminho: &str) -> String {
    let n = componentes(raiz).len();
    let resto = componentes(caminho).split_off(n.min(componentes(caminho).len()));
    if resto.is_empty() {
        ".".to_string()
    } else {
        resto.join("/")
    }
}

/// Confere, no servidor, que `caminho` (já [normalizado](normalizar)) continua dentro de
/// `raiz_real` (o `real_path` da raiz) depois de resolver os links simbólicos. Devolve o
/// caminho resolvido.
///
/// Com `novo`, o arquivo pode não existir ainda: então se resolve a pasta dele e se junta o nome.
/// Um arquivo que existe, mesmo sendo um link, é conferido pelo destino.
pub fn confinar(
    fs: &mut dyn Fs,
    raiz_real: &str,
    caminho: &str,
    novo: bool,
) -> Result<String, String> {
    let resolvido = match fs.real_path(caminho) {
        Ok(r) => r,
        Err(e) if novo && e.kind() == ErrorKind::NotFound => {
            let (pai, nome) = caminho.rsplit_once('/').unwrap_or(("", caminho));
            let pai = if pai.is_empty() { "/" } else { pai };
            let pai_real = fs.real_path(pai).map_err(|e| {
                if e.kind() == ErrorKind::NotFound {
                    t!("mcp-caminho-sem-pai", pasta = pai.to_string())
                } else {
                    t!(
                        "mcp-caminho-erro",
                        caminho = caminho.to_string(),
                        erro = e.to_string()
                    )
                }
            })?;
            format!("{}/{nome}", pai_real.trim_end_matches('/'))
        }
        Err(e) => {
            return Err(t!(
                "mcp-caminho-erro",
                caminho = caminho.to_string(),
                erro = e.to_string()
            ));
        }
    };
    if dentro(raiz_real, &resolvido) {
        Ok(resolvido)
    } else {
        Err(t!(
            "mcp-caminho-sai-link",
            caminho = caminho.to_string(),
            resolvido = resolvido
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomni_ui::files::MemFs;

    #[test]
    fn junta_resolve_e_recusa() {
        assert_eq!(normalizar("/srv/app", "a/b").unwrap(), "/srv/app/a/b");
        assert_eq!(normalizar("/srv/app", "./a//b/.").unwrap(), "/srv/app/a/b");
        assert_eq!(normalizar("/srv/app", "a/../b").unwrap(), "/srv/app/b");
        assert_eq!(normalizar("/srv/app", ".").unwrap(), "/srv/app");
        assert_eq!(normalizar("/srv/app", "").unwrap(), "/srv/app");
        assert_eq!(normalizar("/srv/app", "/srv/app/x").unwrap(), "/srv/app/x");
        assert_eq!(normalizar("/", "/etc/hosts").unwrap(), "/etc/hosts");
        for ruim in [
            "..",
            "../x",
            "a/../../x",
            "/etc/passwd",
            "/srv/app2/x",
            "/srv",
        ] {
            assert!(normalizar("/srv/app", ruim).is_err(), "{ruim}");
        }
    }

    #[test]
    fn dentro_compara_componentes() {
        assert!(dentro("/srv/app", "/srv/app"));
        assert!(dentro("/srv/app/", "/srv/app/a/b"));
        assert!(dentro("/", "/qualquer/coisa"));
        assert!(!dentro("/srv/app", "/srv/app2"));
        assert!(!dentro("/srv/app", "/srv"));
        assert!(!dentro("/srv/app", "/srv/app/../x"));
    }

    #[test]
    fn caminho_relativo_volta_relativo() {
        assert_eq!(relativo("/srv/app", "/srv/app/src/a.rs"), "src/a.rs");
        assert_eq!(relativo("/srv/app/", "/srv/app"), ".");
        assert_eq!(relativo("/", "/etc/hosts"), "etc/hosts");
    }

    fn servidor() -> MemFs {
        let fs = MemFs::new();
        fs.put("/srv/app/src/a.rs", b"1")
            .put("/srv/segredo.txt", b"2")
            .dir("/srv/app/vazia");
        fs.link("/srv/app/para-fora", "/srv")
            .link("/srv/app/atalho", "src")
            .link("/srv/app/src/segredo", "../../segredo.txt");
        fs
    }

    #[test]
    fn link_para_fora_e_recusado() {
        let mut fs = servidor();
        let erro =
            confinar(&mut fs, "/srv/app", "/srv/app/para-fora/segredo.txt", false).unwrap_err();
        assert!(erro.contains("/srv/segredo.txt"), "{erro}");
        // O link de um arquivo para fora também.
        assert!(confinar(&mut fs, "/srv/app", "/srv/app/src/segredo", false).is_err());
    }

    #[test]
    fn link_dentro_vale() {
        let mut fs = servidor();
        assert_eq!(
            confinar(&mut fs, "/srv/app", "/srv/app/atalho/a.rs", false).unwrap(),
            "/srv/app/src/a.rs"
        );
    }

    #[test]
    fn arquivo_novo_confere_o_pai() {
        let mut fs = servidor();
        assert_eq!(
            confinar(&mut fs, "/srv/app", "/srv/app/vazia/novo.txt", true).unwrap(),
            "/srv/app/vazia/novo.txt"
        );
        // O pai é um link para fora: o arquivo novo cairia fora.
        assert!(confinar(&mut fs, "/srv/app", "/srv/app/para-fora/novo.txt", true).is_err());
        // Um arquivo que já existe é conferido por inteiro, mesmo com `novo`.
        assert!(confinar(&mut fs, "/srv/app", "/srv/app/src/segredo", true).is_err());
    }

    #[test]
    fn pai_inexistente_da_erro() {
        let mut fs = servidor();
        let erro = confinar(&mut fs, "/srv/app", "/srv/app/nada/novo.txt", true).unwrap_err();
        assert!(erro.contains("/srv/app/nada"), "{erro}");
        // Sem `novo`, o que não existe também dá erro.
        assert!(confinar(&mut fs, "/srv/app", "/srv/app/nada.txt", false).is_err());
    }
}
