//! Quais campos da grade de propriedades da conexão valem para cada tipo (M60).
//!
//! A grade mostra só o que vale para o tipo escolhido; o que vale mas depende de outro campo
//! (o Modo do FTP com proxy, as Portas do modo ativo, o Arquivo da CA, a credencial do salto
//! sem host do salto, a Credencial com o cofre bloqueado) continua visível e desabilitado, na
//! própria grade. Esconder não apaga: os valores dos campos escondidos ficam nos signals e no
//! `conexoes.json`, e voltam quando o tipo volta.
//!
//! A tabela é escrita por extenso, um braço do `match` por campo e sem `_`: quem lê este arquivo
//! vê a tabela inteira, e um tipo novo no [`Kind`] só compila depois de ganhar a coluna. Nome,
//! Tipo e Nota valem para todos os tipos e ficam fora dela. Busca, MCP e rótulo da árvore usam a
//! mesma tabela, para nunca mostrarem um campo que a grade esconde.
//!
//! Edição de várias conexões não existe hoje (a seleção múltipla da árvore só move, apaga e
//! abre). Se vier, a grade mostra só os campos que valem para todos os tipos escolhidos:
//! `Campo::TODOS.filter(|c| tipos.all(|k| c.vale(k)))`.

use crate::modelo::Kind;

/// Um campo, ou um grupo de campos que andam juntos, da grade de propriedades da conexão.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Campo {
    /// Host.
    Host,
    /// Porta.
    Porta,
    /// Usuário (no S3, o Access Key ID).
    Usuario,
    /// Credencial e Senha.
    Login,
    /// Chave do cofre e Chave privada.
    ChaveSsh,
    /// Rede.
    Rede,
    /// Assistente de IA da conexão (M52).
    Ia,
    /// Assistentes no servidor (a categoria de instalar e abrir os assistentes por SSH).
    IaNoServidor,
    /// Assistentes (MCP) (M51).
    Mcp,
    /// As categorias S3 e S3 avançado.
    S3,
    /// A categoria Rclone.
    Rclone,
    /// As categorias FTP e FTP avançado.
    Ftp,
    /// Sessão › Tipo de terminal.
    TipoDeTerminal,
    /// Sessão › Log.
    Log,
    /// Sessão › Keepalive e Comprimir a conexão.
    SessaoSsh,
    /// Sessão › Sessão persistente (tmux ou screen).
    Persistente,
    /// Sessão › Mouse no terminal (copiar e colar, ou o menu).
    Mouse,
    /// Sessão › Tentar de novo após (a rodada nova de reconexão).
    Reconectar,
    /// Sessão › Programa, Argumentos e Pasta inicial do shell local.
    Programa,
    /// A categoria Colar no terminal.
    Colar,
    /// A categoria Arquivos.
    Arquivos,
    /// A categoria Comandos ao conectar.
    Comandos,
    /// A categoria Encaminhamento.
    Encaminhamento,
    /// A categoria Jump server.
    Salto,
    /// O endereço do link.
    Url,
}

impl Campo {
    /// Todos os campos, na ordem da grade.
    pub const TODOS: [Campo; 25] = [
        Campo::Url,
        Campo::Host,
        Campo::Porta,
        Campo::Usuario,
        Campo::Login,
        Campo::ChaveSsh,
        Campo::Rede,
        Campo::Ia,
        Campo::IaNoServidor,
        Campo::Mcp,
        Campo::S3,
        Campo::Rclone,
        Campo::Ftp,
        Campo::TipoDeTerminal,
        Campo::Log,
        Campo::SessaoSsh,
        Campo::Persistente,
        Campo::Mouse,
        Campo::Reconectar,
        Campo::Programa,
        Campo::Colar,
        Campo::Arquivos,
        Campo::Comandos,
        Campo::Encaminhamento,
        Campo::Salto,
    ];

    /// Se o campo vale para o tipo: a grade mostra a linha só quando vale.
    pub fn vale(self, tipo: Kind) -> bool {
        match self {
            Campo::Host | Campo::Porta => {
                matches!(tipo, Kind::Ssh | Kind::Sftp | Kind::Ftp | Kind::Telnet)
            }
            Campo::Usuario | Campo::Login => matches!(
                tipo,
                Kind::Ssh | Kind::Sftp | Kind::Ftp | Kind::S3 | Kind::Telnet
            ),
            Campo::ChaveSsh | Campo::SessaoSsh | Campo::Salto => {
                matches!(tipo, Kind::Ssh | Kind::Sftp)
            }
            Campo::Rede => matches!(
                tipo,
                Kind::Ssh
                    | Kind::Sftp
                    | Kind::Ftp
                    | Kind::S3
                    | Kind::Rclone
                    | Kind::Telnet
                    | Kind::Link
            ),
            Campo::Ia => matches!(tipo, Kind::Ssh | Kind::Local | Kind::Telnet),
            Campo::IaNoServidor => matches!(tipo, Kind::Ssh),
            Campo::Mcp => matches!(
                tipo,
                Kind::Ssh
                    | Kind::Sftp
                    | Kind::Ftp
                    | Kind::S3
                    | Kind::Rclone
                    | Kind::Local
                    | Kind::Telnet
            ),
            Campo::S3 => matches!(tipo, Kind::S3),
            Campo::Rclone => matches!(tipo, Kind::Rclone),
            Campo::Ftp => matches!(tipo, Kind::Ftp),
            Campo::TipoDeTerminal | Campo::Persistente | Campo::Colar | Campo::Encaminhamento => {
                matches!(tipo, Kind::Ssh)
            }
            Campo::Log | Campo::Comandos | Campo::Mouse => {
                matches!(tipo, Kind::Ssh | Kind::Local | Kind::Telnet)
            }
            Campo::Reconectar => matches!(tipo, Kind::Ssh | Kind::Telnet),
            Campo::Programa => matches!(tipo, Kind::Local),
            Campo::Arquivos => matches!(
                tipo,
                Kind::Ssh | Kind::Sftp | Kind::Ftp | Kind::S3 | Kind::Rclone
            ),
            Campo::Url => matches!(tipo, Kind::Link),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::comandos::tem_terminal;

    /// A tabela por extenso, na ordem de `Kind::TODOS`: Ssh, Sftp, Ftp, S3, Rclone, Local, Telnet,
    /// Link. Mudar a tabela exige mudar este teste.
    const TABELA: [(Campo, [bool; 8]); 25] = [
        (
            Campo::Url,
            [false, false, false, false, false, false, false, true],
        ),
        (
            Campo::Host,
            [true, true, true, false, false, false, true, false],
        ),
        (
            Campo::Porta,
            [true, true, true, false, false, false, true, false],
        ),
        (
            Campo::Usuario,
            [true, true, true, true, false, false, true, false],
        ),
        (
            Campo::Login,
            [true, true, true, true, false, false, true, false],
        ),
        (
            Campo::ChaveSsh,
            [true, true, false, false, false, false, false, false],
        ),
        (
            Campo::Rede,
            [true, true, true, true, true, false, true, true],
        ),
        (
            Campo::Ia,
            [true, false, false, false, false, true, true, false],
        ),
        (
            Campo::IaNoServidor,
            [true, false, false, false, false, false, false, false],
        ),
        (
            Campo::Mcp,
            [true, true, true, true, true, true, true, false],
        ),
        (
            Campo::S3,
            [false, false, false, true, false, false, false, false],
        ),
        (
            Campo::Rclone,
            [false, false, false, false, true, false, false, false],
        ),
        (
            Campo::Ftp,
            [false, false, true, false, false, false, false, false],
        ),
        (
            Campo::TipoDeTerminal,
            [true, false, false, false, false, false, false, false],
        ),
        (
            Campo::Log,
            [true, false, false, false, false, true, true, false],
        ),
        (
            Campo::SessaoSsh,
            [true, true, false, false, false, false, false, false],
        ),
        (
            Campo::Persistente,
            [true, false, false, false, false, false, false, false],
        ),
        (
            Campo::Mouse,
            [true, false, false, false, false, true, true, false],
        ),
        (
            Campo::Reconectar,
            [true, false, false, false, false, false, true, false],
        ),
        (
            Campo::Programa,
            [false, false, false, false, false, true, false, false],
        ),
        (
            Campo::Colar,
            [true, false, false, false, false, false, false, false],
        ),
        (
            Campo::Arquivos,
            [true, true, true, true, true, false, false, false],
        ),
        (
            Campo::Comandos,
            [true, false, false, false, false, true, true, false],
        ),
        (
            Campo::Encaminhamento,
            [true, false, false, false, false, false, false, false],
        ),
        (
            Campo::Salto,
            [true, true, false, false, false, false, false, false],
        ),
    ];

    #[test]
    fn a_tabela_por_extenso_confere_com_o_vale() {
        assert_eq!(TABELA.len(), Campo::TODOS.len());
        for (campo, colunas) in TABELA {
            assert!(Campo::TODOS.contains(&campo), "{campo:?} fora do TODOS");
            for (tipo, esperado) in Kind::TODOS.into_iter().zip(colunas) {
                assert_eq!(
                    campo.vale(tipo),
                    esperado,
                    "{campo:?} no tipo {tipo:?} deveria valer {esperado}"
                );
            }
        }
    }

    #[test]
    fn a_ordem_dos_tipos_da_tabela_e_a_do_kind() {
        assert_eq!(
            Kind::TODOS,
            [
                Kind::Ssh,
                Kind::Sftp,
                Kind::Ftp,
                Kind::S3,
                Kind::Rclone,
                Kind::Local,
                Kind::Telnet,
                Kind::Link
            ]
        );
    }

    #[test]
    fn os_campos_que_coincidem_com_os_predicados_do_tipo_batem() {
        for tipo in Kind::TODOS {
            assert_eq!(Campo::Salto.vale(tipo), tipo.usa_ssh(), "{tipo:?}");
            assert_eq!(Campo::ChaveSsh.vale(tipo), tipo.usa_ssh(), "{tipo:?}");
            assert_eq!(Campo::SessaoSsh.vale(tipo), tipo.usa_ssh(), "{tipo:?}");
            assert_eq!(Campo::Arquivos.vale(tipo), tipo.tem_arquivos(), "{tipo:?}");
            assert_eq!(Campo::Comandos.vale(tipo), tem_terminal(tipo), "{tipo:?}");
            assert_eq!(Campo::Mouse.vale(tipo), tem_terminal(tipo), "{tipo:?}");
            assert_eq!(Campo::Log.vale(tipo), tem_terminal(tipo), "{tipo:?}");
            assert_eq!(Campo::Programa.vale(tipo), tipo == Kind::Local, "{tipo:?}");
        }
    }

    #[test]
    fn todo_tipo_tem_coluna_e_o_mcp_vale_para_todos_menos_o_link() {
        for tipo in Kind::TODOS {
            // O link abre no navegador, fora do alcance dos assistentes.
            assert_eq!(Campo::Mcp.vale(tipo), tipo != Kind::Link, "{tipo:?}");
            assert!(
                Campo::TODOS.iter().any(|c| c.vale(tipo)),
                "{tipo:?} sem nenhum campo"
            );
        }
    }

    #[test]
    fn o_shell_local_nao_tem_servidor_nem_porta() {
        for campo in [Campo::Host, Campo::Porta, Campo::Usuario, Campo::Login] {
            assert!(!campo.vale(Kind::Local), "{campo:?}");
        }
    }

    #[test]
    fn o_s3_tem_usuario_mas_nao_host_nem_porta() {
        assert!(Campo::Usuario.vale(Kind::S3));
        assert!(!Campo::Host.vale(Kind::S3));
        assert!(!Campo::Porta.vale(Kind::S3));
    }

    #[test]
    fn o_telnet_tem_credencial() {
        assert!(Campo::Login.vale(Kind::Telnet));
    }
}
