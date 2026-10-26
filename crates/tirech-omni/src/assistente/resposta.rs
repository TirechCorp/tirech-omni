//! O que fazer com a resposta da IA (M52): separar o comando da explicação e avisar dos comandos
//! perigosos. Puro: a tela só mostra o que sai daqui. **Nada é executado**: o comando só pode ser
//! inserido na linha do terminal (sem Enter), por [`crate::texto_limpo::para_inserir`].

use std::sync::LazyLock;

use regex::Regex;

/// Um jeito de apagar ou estragar dados que merece um aviso: o nome (para o teste e para quem
/// ler o código) e a expressão.
pub struct Perigoso {
    /// O que o padrão pega.
    pub nome: &'static str,
    /// A expressão regular, sem diferenciar maiúsculas de minúsculas.
    pub padrao: &'static str,
}

/// Os comandos que casam com o aviso (seção 8.2 do plano).
pub const PERIGOSOS: &[Perigoso] = &[
    Perigoso {
        nome: "rm recursivo na raiz, no ~ ou em *",
        padrao: concat!(
            r"\brm\s+(?:-\S+\s+)*?(?:-\S*r\S*|--recursive)\s+(?:-\S+\s+)*(?:--\s+)?",
            r"(?:/\*?|~/?\*?|\*|\$HOME/?\*?|/(?:etc|usr|var|bin|sbin|lib|lib64|boot|home|root|opt)/?\*?)",
            r"(?:\s|$|[;&|])"
        ),
    },
    Perigoso {
        nome: "mkfs",
        padrao: r"\bmkfs(?:\.\w+)?\b",
    },
    Perigoso {
        nome: "dd escrevendo num dispositivo",
        padrao: r"\bdd\b[^;&|\n]*\bof=/dev/",
    },
    Perigoso {
        nome: "redirecionar para um disco",
        padrao: r">\s*/dev/(?:sd|hd|vd|xvd|nvme)",
    },
    Perigoso {
        nome: "chmod recursivo na raiz",
        padrao: concat!(
            r"\bchmod\s+(?:-\S+\s+)*(?:-\S*R\S*|--recursive)\s+(?:-\S+\s+)*\S+\s+",
            r"(?:/\*?|/(?:etc|usr|var|bin|sbin|lib|lib64|boot|home|root|opt)/?\*?)(?:\s|$|[;&|])"
        ),
    },
    Perigoso {
        nome: "chown recursivo na raiz",
        padrao: concat!(
            r"\bchown\s+(?:-\S+\s+)*(?:-\S*R\S*|--recursive)\s+(?:-\S+\s+)*\S+\s+",
            r"(?:/\*?|/(?:etc|usr|var|bin|sbin|lib|lib64|boot|home|root|opt)/?\*?)(?:\s|$|[;&|])"
        ),
    },
    Perigoso {
        nome: "fork bomb",
        padrao: r":\(\)\s*\{",
    },
    Perigoso {
        nome: "desligar ou reiniciar",
        padrao: r"\b(?:shutdown|reboot|halt|poweroff)\b",
    },
    Perigoso {
        nome: "DROP TABLE ou DROP DATABASE",
        padrao: r"\bdrop\s+(?:table|database)\b",
    },
    Perigoso {
        nome: "git push --force",
        padrao: r"\bgit\s+push\b[^;&|\n]*(?:--force(?:\s|$)|\s-f(?:\s|$))",
    },
    Perigoso {
        nome: "git reset --hard",
        padrao: r"\bgit\s+reset\b[^;&|\n]*--hard\b",
    },
];

/// As expressões de [`PERIGOSOS`], compiladas uma vez.
static COMPILADOS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    PERIGOSOS
        .iter()
        .map(|p| {
            let regex = Regex::new(&format!("(?i){}", p.padrao))
                .unwrap_or_else(|erro| panic!("padrão perigoso inválido ({}): {erro}", p.nome));
            (p.nome, regex)
        })
        .collect()
});

/// O nome do primeiro padrão perigoso que o comando casa, ou `None`. O aviso não bloqueia nada.
pub fn perigoso(comando: &str) -> Option<&'static str> {
    COMPILADOS
        .iter()
        .find(|(_, regex)| regex.is_match(comando))
        .map(|(nome, _)| *nome)
}

/// Uma cerca de bloco de código aberta: o caractere (`` ` `` ou `~`) e quantos.
fn abre_cerca(linha: &str) -> Option<(char, usize)> {
    let aparada = linha.trim_start_matches(' ');
    if linha.len() - aparada.len() > 3 {
        return None;
    }
    let caractere = aparada.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let quantos = aparada.chars().take_while(|c| *c == caractere).count();
    // Uma cerca de crase não tem crase na linha de abertura (seria código em linha).
    let resto = &aparada[quantos..];
    (quantos >= 3 && !(caractere == '`' && resto.contains('`'))).then_some((caractere, quantos))
}

/// Se a linha fecha a cerca `(caractere, quantos)`.
fn fecha_cerca(linha: &str, (caractere, quantos): (char, usize)) -> bool {
    let aparada = linha.trim();
    let n = aparada.chars().take_while(|c| *c == caractere).count();
    n >= quantos && n == aparada.chars().count()
}

/// Tira o prompt colado no começo (`$ `, `# `, `> `, `PS> `).
fn sem_prompt(linha: &str) -> &str {
    for prefixo in ["PS> ", "$ ", "# ", "> "] {
        if let Some(resto) = linha.strip_prefix(prefixo) {
            return resto;
        }
    }
    linha
}

/// Tira o prompt de todas as linhas, se **todas** as não vazias o têm (assim um `# comentário` no
/// meio de um script não é confundido com prompt).
fn sem_prompts(comando: &str) -> String {
    let linhas: Vec<&str> = comando.lines().collect();
    let todas = linhas
        .iter()
        .filter(|l| !l.trim().is_empty())
        .all(|l| sem_prompt(l) != *l);
    if todas {
        linhas
            .iter()
            .map(|l| sem_prompt(l))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        comando.to_owned()
    }
}

/// Separa a resposta do modo "gerar comando" em comando e explicação (seção 8.3 do plano):
///
/// 1. o primeiro bloco cercado (```` ``` ```` ou `~~~`), de qualquer linguagem, é o comando, e o
///    texto fora dele é a explicação; um bloco ainda aberto (a resposta está chegando) vale até
///    onde chegou;
/// 2. sem bloco, a primeira linha não vazia, sem os `` ` `` em volta, é o comando e o resto é a
///    explicação (modelos pequenos ignoram o formato);
/// 3. um prompt colado no começo sai.
///
/// O comando é `None` enquanto não há nada que sirva.
pub fn comando_e_explicacao(resposta: &str) -> (Option<String>, String) {
    let linhas: Vec<&str> = resposta.lines().collect();
    let mut antes: Vec<&str> = Vec::new();
    let mut comando: Vec<&str> = Vec::new();
    let mut depois: Vec<&str> = Vec::new();
    let mut estado = 0u8; // 0 = antes do bloco, 1 = dentro, 2 = depois
    let mut cerca = ('`', 3);
    let mut achou_bloco = false;
    for linha in &linhas {
        match estado {
            0 => match abre_cerca(linha) {
                Some(c) => {
                    cerca = c;
                    estado = 1;
                    achou_bloco = true;
                }
                None => antes.push(linha),
            },
            1 => {
                if fecha_cerca(linha, cerca) {
                    estado = 2;
                } else {
                    comando.push(linha);
                }
            }
            _ => depois.push(linha),
        }
    }
    let (comando, explicacao) = if achou_bloco {
        let fora: Vec<&str> = antes.into_iter().chain(depois).collect();
        (comando.join("\n"), fora.join("\n"))
    } else {
        let mut resto = linhas.iter().copied().skip_while(|l| l.trim().is_empty());
        let primeira = resto.next().unwrap_or("").trim();
        let sem_crases = primeira
            .strip_prefix('`')
            .and_then(|p| p.strip_suffix('`'))
            .unwrap_or(primeira);
        (
            sem_crases.trim().to_owned(),
            resto.collect::<Vec<_>>().join("\n"),
        )
    };
    let comando = sem_prompts(comando.trim_matches(['\n', '\r']).trim_end());
    let comando = comando.trim().to_owned();
    (
        (!comando.is_empty()).then_some(comando),
        explicacao.trim().to_owned(),
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn bloco_com_linguagem_e_frase() {
        let resposta =
            "```bash\nfind . -type f -size +1G\n```\n\nLista os arquivos com mais de 1 GB.";
        let (comando, explicacao) = comando_e_explicacao(resposta);
        assert_eq!(comando.as_deref(), Some("find . -type f -size +1G"));
        assert_eq!(explicacao, "Lista os arquivos com mais de 1 GB.");
    }

    #[test]
    fn frase_antes_e_depois_do_bloco() {
        let (comando, explicacao) =
            comando_e_explicacao("Use isto:\n~~~sh\nls -la\n~~~\nMostra tudo.");
        assert_eq!(comando.as_deref(), Some("ls -la"));
        assert_eq!(explicacao, "Use isto:\nMostra tudo.");
    }

    #[test]
    fn so_o_primeiro_bloco_vale() {
        let (comando, explicacao) = comando_e_explicacao("```\nuno\n```\ntexto\n```\ndos\n```");
        assert_eq!(comando.as_deref(), Some("uno"));
        assert!(explicacao.contains("dos"));
    }

    #[test]
    fn bloco_ainda_aberto_vale_ate_onde_chegou() {
        let (comando, explicacao) = comando_e_explicacao("```sh\nss -tl");
        assert_eq!(comando.as_deref(), Some("ss -tl"));
        assert_eq!(explicacao, "");
        // Só a cerca de abertura: ainda não há comando.
        assert_eq!(comando_e_explicacao("```sh\n").0, None);
    }

    #[test]
    fn cerca_mais_longa_aceita_crases_dentro() {
        let (comando, _) = comando_e_explicacao("````\necho ```oi```\n````");
        assert_eq!(comando.as_deref(), Some("echo ```oi```"));
    }

    #[test]
    fn sem_bloco_a_primeira_linha_e_o_comando() {
        let (comando, explicacao) = comando_e_explicacao("\n`ls -la`\nLista tudo.\nCom detalhes.");
        assert_eq!(comando.as_deref(), Some("ls -la"));
        assert_eq!(explicacao, "Lista tudo.\nCom detalhes.");
        assert_eq!(comando_e_explicacao("df -h").0.as_deref(), Some("df -h"));
    }

    #[test]
    fn resposta_vazia_nao_tem_comando() {
        assert_eq!(comando_e_explicacao(""), (None, String::new()));
        assert_eq!(comando_e_explicacao("  \n\n"), (None, String::new()));
    }

    #[test]
    fn prompt_colado_no_comeco_sai() {
        for (entrada, esperado) in [
            ("```\n$ ls -la\n```", "ls -la"),
            ("```\n# systemctl status ssh\n```", "systemctl status ssh"),
            ("```\n> dir\n```", "dir"),
            ("```\nPS> Get-Process\n```", "Get-Process"),
            ("$ uptime", "uptime"),
        ] {
            assert_eq!(comando_e_explicacao(entrada).0.as_deref(), Some(esperado));
        }
    }

    #[test]
    fn comentario_num_script_nao_e_prompt() {
        let (comando, _) = comando_e_explicacao("```\n# lista\nls\n```");
        assert_eq!(comando.as_deref(), Some("# lista\nls"));
    }

    #[test]
    fn varias_linhas_com_prompt_em_todas() {
        let (comando, _) = comando_e_explicacao("```\n$ cd /tmp\n$ ls\n```");
        assert_eq!(comando.as_deref(), Some("cd /tmp\nls"));
    }

    #[test]
    fn cada_perigoso_e_pego() {
        let casos = [
            "rm -rf /",
            "rm -rf /*",
            "sudo rm -rf --no-preserve-root /",
            "rm -rf ~",
            "rm -rf ~/",
            "rm -r *",
            "rm -fr $HOME",
            "rm -r -f /etc",
            "rm --recursive --force /",
            "mkfs.ext4 /dev/sdb1",
            "mkfs /dev/sdb",
            "dd if=/dev/zero of=/dev/sda bs=1M",
            "echo x > /dev/sda",
            "cat img > /dev/nvme0n1",
            "chmod -R 777 /",
            "chmod -R 755 /etc",
            "chown -R ana:ana /",
            ":(){ :|:& };:",
            "shutdown -h now",
            "sudo reboot",
            "systemctl poweroff",
            "halt",
            "psql -c 'DROP TABLE usuarios'",
            "mysql -e \"drop database app\"",
            "git push --force origin main",
            "git push origin main -f",
            "git reset --hard HEAD~3",
        ];
        for comando in casos {
            assert!(perigoso(comando).is_some(), "devia avisar: {comando}");
        }
    }

    #[test]
    fn cada_padrao_da_lista_tem_um_caso() {
        // Se alguém acrescentar um padrão sem teste, este conjunto acusa.
        let casos: &[(&str, &str)] = &[
            ("rm recursivo na raiz, no ~ ou em *", "rm -rf /"),
            ("mkfs", "mkfs.xfs /dev/sdc"),
            ("dd escrevendo num dispositivo", "dd if=a of=/dev/sdb"),
            ("redirecionar para um disco", "cat x > /dev/sda"),
            ("chmod recursivo na raiz", "chmod -R 000 /"),
            ("chown recursivo na raiz", "chown -R root /"),
            ("fork bomb", ":(){ :|:& };:"),
            ("desligar ou reiniciar", "reboot"),
            ("DROP TABLE ou DROP DATABASE", "DROP DATABASE x"),
            ("git push --force", "git push --force"),
            ("git reset --hard", "git reset --hard"),
        ];
        assert_eq!(casos.len(), PERIGOSOS.len());
        for p in PERIGOSOS {
            let (_, comando) = casos
                .iter()
                .find(|(nome, _)| *nome == p.nome)
                .unwrap_or_else(|| panic!("sem caso para {}", p.nome));
            assert_eq!(perigoso(comando), Some(p.nome), "{comando}");
        }
    }

    #[test]
    fn falsos_positivos_obvios_nao_avisam() {
        let casos = [
            "rm -r ./build",
            "rm -rf build/",
            "rm -rf /tmp/cache",
            "rm arquivo.txt",
            "rm -f /var/log/app.log",
            "ls -la /",
            "chmod -R 755 /var/www/site",
            "chown -R www-data /srv/app",
            "echo oi > /tmp/x",
            "dd if=/dev/zero of=teste.img bs=1M count=10",
            "git push origin main",
            "git push --force-with-lease",
            "git reset HEAD~1",
            "git reset --soft HEAD~1",
            "select * from tabela",
            "mkdir formatos",
            "ls /dev/sda",
        ];
        for comando in casos {
            assert_eq!(perigoso(comando), None, "não devia avisar: {comando}");
        }
    }
}
