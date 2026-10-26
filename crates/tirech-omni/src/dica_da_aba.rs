//! A dica que aparece ao parar o ponteiro sobre uma aba do terminal (M42): para onde a aba está
//! ligada e se a conexão está de pé. Dá para saber o servidor de cada aba, e o motivo da queda,
//! sem abrir a aba.

use tomni_ui::terminal::{Notice, Session, TermProgress};

use crate::modelo::{Connection, Kind};
use crate::t;

/// Se a conexão da aba está de pé.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EstadoDaAba {
    /// A sessão ainda abre a conexão (M62): o servidor não respondeu o shell.
    Conectando,
    /// A sessão está aberta.
    Conectada,
    /// A conexão caiu, com o motivo (`close_reason()`).
    Caiu(String),
    /// A sessão acabou sem motivo: o shell saiu.
    Encerrada,
}

impl EstadoDaAba {
    /// O estado de `sessao` agora. Lê `closed()` e `connecting()`: dentro de um efeito, muda
    /// com a queda, a reconexão e o fim da abertura.
    pub fn da_sessao(sessao: Session) -> Self {
        if sessao.closed() {
            match sessao.close_reason() {
                Some(motivo) => Self::Caiu(motivo),
                None => Self::Encerrada,
            }
        } else if sessao.connecting() {
            Self::Conectando
        } else {
            Self::Conectada
        }
    }
}

/// O que a dica de uma aba mostra.
#[derive(Clone, Debug)]
pub struct DadosDaAba<'a> {
    /// O título da aba.
    pub titulo: &'a str,
    /// A conexão do modelo; `None` quando ela foi apagada ou a chave não tem id.
    pub conexao: Option<&'a Connection>,
    /// O nome do perfil de rede por onde a aba saiu.
    pub rede: Option<&'a str>,
    /// Se a conexão está de pé.
    pub estado: EstadoDaAba,
    /// O andamento que o programa informou (`OSC 9;4`).
    pub progresso: TermProgress,
    /// O último aviso com texto, quando a aba tem o ponto de atenção aceso.
    pub aviso: Option<Notice>,
    /// O passo em que os comandos ao conectar estão (a partir de 1) e o total, enquanto rodam.
    pub comandos: Option<(usize, usize)>,
}

/// O texto da dica: uma linha por item, sem as linhas que não se aplicam.
pub fn dica_da_aba(d: &DadosDaAba) -> String {
    let mut linhas = vec![d.titulo.to_owned()];
    if let Some(c) = d.conexao {
        if !c.name.is_empty() && c.name != d.titulo {
            linhas.push(t!("dica-conexao", nome = c.name.as_str()));
        }
        if let Some(destino) = destino(c) {
            linhas.push(destino);
        }
        if let Some(salto) = c.jump.as_ref().filter(|_| c.kind == Kind::Ssh) {
            linhas.push(t!(
                "dica-salto",
                endereco = endereco(&salto.user, &salto.host, salto.port)
            ));
        }
    }
    if let Some(rede) = d.rede.filter(|r| !r.is_empty()) {
        linhas.push(t!("dica-rede", rede = rede));
    }
    if let Some(linha) = linha_do_progresso(d.progresso) {
        linhas.push(linha);
    }
    if let Some((n, total)) = d.comandos {
        linhas.push(t!("dica-comandos", n = n as i64, total = total as i64));
    }
    if let Some(aviso) = &d.aviso {
        linhas.push(t!("dica-aviso", texto = texto_do_aviso(aviso)));
    }
    linhas.push(match &d.estado {
        EstadoDaAba::Conectando => t!("dica-conectando"),
        EstadoDaAba::Conectada => t!("dica-conectada"),
        EstadoDaAba::Caiu(motivo) => t!("dica-caiu", motivo = motivo.as_str()),
        EstadoDaAba::Encerrada => t!("dica-encerrada"),
    });
    linhas.join("\n")
}

/// A linha do andamento, ou `None` sem andamento.
fn linha_do_progresso(progresso: TermProgress) -> Option<String> {
    match progresso {
        TermProgress::None => None,
        TermProgress::Normal(valor) => Some(t!("dica-progresso", valor = i64::from(valor))),
        TermProgress::Indeterminate => Some(t!("dica-trabalhando")),
        TermProgress::Paused(Some(valor)) => {
            Some(t!("dica-pausado-valor", valor = i64::from(valor)))
        }
        TermProgress::Paused(None) => Some(t!("dica-pausado")),
        TermProgress::Error(Some(valor)) => Some(t!("dica-erro-valor", valor = i64::from(valor))),
        TermProgress::Error(None) => Some(t!("dica-erro")),
    }
}

/// O aviso numa linha: "título — corpo", ou só o corpo quando o programa não deu título.
fn texto_do_aviso(aviso: &Notice) -> String {
    match aviso.title.as_deref().filter(|t| !t.is_empty()) {
        Some(titulo) => format!("{titulo} — {}", aviso.body),
        None => aviso.body.clone(),
    }
}

/// `usuário@host:porta`, sem o `usuário@` quando ele está vazio.
fn endereco(usuario: &str, host: &str, porta: u16) -> String {
    if usuario.is_empty() {
        format!("{host}:{porta}")
    } else {
        format!("{usuario}@{host}:{porta}")
    }
}

/// A linha do destino, conforme o tipo. Os tipos de arquivos não abrem aba de terminal.
fn destino(c: &Connection) -> Option<String> {
    match c.kind {
        Kind::Ssh => Some(format!("SSH · {}", endereco(&c.user, &c.host, c.port))),
        Kind::Telnet => Some(format!("Telnet · {}:{}", c.host, c.port)),
        Kind::Local if c.local_programa.trim().is_empty() => Some(t!("dica-shell-local")),
        Kind::Local => {
            let comando = format!("{} {}", c.local_programa.trim(), c.local_argumentos.trim());
            Some(t!("dica-local", programa = comando.trim_end()))
        }
        Kind::Sftp | Kind::Ftp | Kind::S3 | Kind::Rclone => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::Jump;

    fn ssh() -> Connection {
        let mut c = Connection::new("Servidor web", Kind::Ssh);
        c.host = "10.0.0.5".into();
        c.user = "deploy".into();
        c
    }

    fn dados<'a>(
        titulo: &'a str,
        c: Option<&'a Connection>,
        estado: EstadoDaAba,
    ) -> DadosDaAba<'a> {
        DadosDaAba {
            titulo,
            conexao: c,
            rede: None,
            estado,
            progresso: TermProgress::None,
            aviso: None,
            comandos: None,
        }
    }

    #[test]
    fn ssh_mostra_o_nome_o_destino_e_o_estado() {
        let c = ssh();
        let d = dados("web-01", Some(&c), EstadoDaAba::Conectada);
        assert_eq!(
            dica_da_aba(&d),
            "web-01\nConexão: Servidor web\nSSH · deploy@10.0.0.5:22\nConectada"
        );
    }

    #[test]
    fn o_nome_igual_ao_titulo_nao_se_repete() {
        let c = ssh();
        let d = dados("Servidor web", Some(&c), EstadoDaAba::Conectada);
        assert_eq!(
            dica_da_aba(&d),
            "Servidor web\nSSH · deploy@10.0.0.5:22\nConectada"
        );
    }

    #[test]
    fn sem_usuario_o_destino_nao_tem_arroba() {
        let mut c = ssh();
        c.user.clear();
        let d = dados("Servidor web", Some(&c), EstadoDaAba::Conectada);
        assert!(dica_da_aba(&d).contains("SSH · 10.0.0.5:22"));
    }

    #[test]
    fn o_salto_e_a_rede_entram_so_quando_existem() {
        let mut c = ssh();
        c.jump = Some(Jump {
            host: "bastiao".into(),
            port: 2222,
            user: "admin".into(),
            credencial: None,
        });
        let mut d = dados("Servidor web", Some(&c), EstadoDaAba::Conectada);
        d.rede = Some("VPN do escritório");
        assert_eq!(
            dica_da_aba(&d),
            "Servidor web\nSSH · deploy@10.0.0.5:22\nSalto: admin@bastiao:2222\nRede: VPN do escritório\nConectada"
        );
    }

    #[test]
    fn caiu_traz_o_motivo_e_encerrada_nao() {
        let c = ssh();
        let caiu = dados(
            "Servidor web",
            Some(&c),
            EstadoDaAba::Caiu("tempo esgotado".into()),
        );
        assert!(dica_da_aba(&caiu).ends_with("Caiu: tempo esgotado"));
        let fim = dados("Servidor web", Some(&c), EstadoDaAba::Encerrada);
        assert!(dica_da_aba(&fim).ends_with("Encerrada"));
    }

    #[test]
    fn telnet_e_local_tem_o_destino_deles() {
        let mut t = Connection::new("Roteador", Kind::Telnet);
        t.host = "192.168.0.1".into();
        let d = dados("Roteador", Some(&t), EstadoDaAba::Conectada);
        assert!(dica_da_aba(&d).contains("Telnet · 192.168.0.1:23"));

        let mut l = Connection::new("Shell local", Kind::Local);
        let d = dados("Shell local", Some(&l), EstadoDaAba::Conectada);
        assert_eq!(dica_da_aba(&d), "Shell local\nShell local\nConectada");
        l.local_programa = "wsl.exe".into();
        l.local_argumentos = "-d Ubuntu --cd ~".into();
        let d = dados("Ubuntu", Some(&l), EstadoDaAba::Conectada);
        assert!(dica_da_aba(&d).contains("Local · wsl.exe -d Ubuntu --cd ~"));
        l.local_argumentos.clear();
        let d = dados("Ubuntu", Some(&l), EstadoDaAba::Conectada);
        assert!(dica_da_aba(&d).contains("Local · wsl.exe\n"));
    }

    #[test]
    fn o_andamento_vira_uma_linha_antes_do_estado() {
        let c = ssh();
        let linha = |progresso| {
            let mut d = dados("web-01", Some(&c), EstadoDaAba::Conectada);
            d.progresso = progresso;
            dica_da_aba(&d)
        };
        assert!(linha(TermProgress::Normal(45)).ends_with("Progresso: 45%\nConectada"));
        assert!(linha(TermProgress::Indeterminate).ends_with("Trabalhando…\nConectada"));
        assert!(linha(TermProgress::Paused(Some(60))).ends_with("Pausado (60%)\nConectada"));
        assert!(linha(TermProgress::Paused(None)).ends_with("Pausado\nConectada"));
        assert!(linha(TermProgress::Error(None)).ends_with("Erro\nConectada"));
        assert!(linha(TermProgress::Error(Some(10))).ends_with("Erro (10%)\nConectada"));
        assert!(!linha(TermProgress::None).contains("Progresso"));
    }

    #[test]
    fn o_aviso_traz_o_titulo_e_o_corpo() {
        let mut d = dados("web-01", None, EstadoDaAba::Conectada);
        d.aviso = Some(Notice {
            title: Some("Build".into()),
            body: "terminou".into(),
        });
        assert_eq!(
            dica_da_aba(&d),
            "web-01\nAviso: Build — terminou\nConectada"
        );
        d.aviso = Some(Notice {
            title: None,
            body: "terminou".into(),
        });
        assert_eq!(dica_da_aba(&d), "web-01\nAviso: terminou\nConectada");
    }

    #[test]
    fn os_comandos_ao_conectar_mostram_o_passo_enquanto_rodam() {
        let mut d = dados("web-01", None, EstadoDaAba::Conectada);
        d.comandos = Some((2, 3));
        assert_eq!(
            dica_da_aba(&d),
            "web-01\nComandos ao conectar: passo 2 de 3\nConectada"
        );
    }

    #[test]
    fn sem_conexao_no_modelo_so_o_titulo_e_o_estado() {
        let d = dados("web-01", None, EstadoDaAba::Encerrada);
        assert_eq!(dica_da_aba(&d), "web-01\nEncerrada");
    }
}
