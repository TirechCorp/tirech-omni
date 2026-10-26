//! Exportar as conexões para outros gerenciadores (M38): o JSON do próprio Tirech Omni, o
//! `~/.ssh/config`, o `.reg` do PuTTY, o FileZilla, o WinSCP, o WinSSHTerm, o mRemoteNG e o
//! MobaXterm.
//!
//! Tudo aqui é **puro**, como em [`crate::importar`]: cada formato recebe o [`Plano`] (a árvore
//! com as senhas e as chaves já resolvidas) e devolve o texto do arquivo, sem tocar no disco. A
//! tela escolhe o destino, grava as chaves do cofre ([`chaves`]) e escreve o arquivo. Os bytes
//! aleatórios (sal, nonce, enchimento) vêm de um [`Aleatorio`] dado, e os testes o fixam.

pub mod chaves;
pub mod filezilla;
pub mod mobaxterm;
pub mod mremoteng;
pub mod nativo;
pub mod plano;
pub mod putty;
pub mod ssh_config;
pub mod winscp;
pub mod winsshterm;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use tomni_ui::terminal::ssh::keys::PrivateFormat;

use crate::cofre::Cofre;
use crate::cofre_json::ArquivoDeAnexo;
use crate::modelo::{Kind, Tree};
use crate::t;

pub use plano::{ItemPlano, Plano, ProxyPlano, SaltoPlano};

/// O formato de saída.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alvo {
    /// O JSON do próprio Tirech Omni: a árvore inteira e, com as senhas, o cofre ligado a ela.
    Nativo,
    /// O `~/.ssh/config` do OpenSSH.
    SshConfig,
    /// O `.reg` do PuTTY (e do KiTTY).
    Putty,
    /// O `sitemanager.xml` do FileZilla.
    FileZilla,
    /// O `WinSCP.ini`.
    WinScp,
    /// O `connections.xml` do WinSSHTerm.
    WinSshTerm,
    /// O `confCons.xml` do mRemoteNG.
    MRemoteNg,
    /// O `.mxtsessions` do MobaXterm.
    MobaXterm,
}

impl Alvo {
    /// Todos, na ordem do menu (o nativo, o mais completo, primeiro).
    pub const TODOS: [Alvo; 8] = [
        Alvo::Nativo,
        Alvo::SshConfig,
        Alvo::Putty,
        Alvo::FileZilla,
        Alvo::WinScp,
        Alvo::WinSshTerm,
        Alvo::MRemoteNg,
        Alvo::MobaXterm,
    ];

    /// O identificador, para os ids da tela (`winscp`).
    pub fn id(self) -> &'static str {
        match self {
            Alvo::Nativo => "nativo",
            Alvo::SshConfig => "ssh-config",
            Alvo::Putty => "putty",
            Alvo::FileZilla => "filezilla",
            Alvo::WinScp => "winscp",
            Alvo::WinSshTerm => "winsshterm",
            Alvo::MRemoteNg => "mremoteng",
            Alvo::MobaXterm => "mobaxterm",
        }
    }

    /// O nome na tela.
    pub fn nome(self) -> &'static str {
        match self {
            Alvo::Nativo => "Tirech Omni (JSON)",
            Alvo::SshConfig => "OpenSSH (~/.ssh/config)",
            Alvo::Putty => "PuTTY / KiTTY",
            Alvo::FileZilla => "FileZilla",
            Alvo::WinScp => "WinSCP",
            Alvo::WinSshTerm => "WinSSHTerm",
            Alvo::MRemoteNg => "mRemoteNG",
            Alvo::MobaXterm => "MobaXterm",
        }
    }

    /// O nome do programa nas mensagens ("não cabem no formato do WinSSHTerm").
    fn do_programa(self) -> &'static str {
        match self {
            Alvo::Nativo => "Tirech Omni",
            Alvo::SshConfig => "OpenSSH",
            Alvo::Putty => "PuTTY",
            Alvo::FileZilla => "FileZilla",
            Alvo::WinScp => "WinSCP",
            Alvo::WinSshTerm => "WinSSHTerm",
            Alvo::MRemoteNg => "mRemoteNG",
            Alvo::MobaXterm => "MobaXterm",
        }
    }

    /// O nome de arquivo que o diálogo sugere.
    pub fn arquivo_padrao(self) -> &'static str {
        match self {
            Alvo::Nativo => "tirech-omni-conexoes.json",
            Alvo::SshConfig => "config",
            Alvo::Putty => "tirech-omni.reg",
            Alvo::FileZilla => "sitemanager.xml",
            Alvo::WinScp => "WinSCP.ini",
            Alvo::WinSshTerm => "connections.xml",
            Alvo::MRemoteNg => "confCons.xml",
            Alvo::MobaXterm => "tirech-omni.mxtsessions",
        }
    }

    /// As extensões do filtro do diálogo, sem o ponto (vazio: qualquer arquivo).
    pub fn extensoes(self) -> &'static [&'static str] {
        match self {
            Alvo::Nativo => &["json"],
            Alvo::SshConfig => &[],
            Alvo::Putty => &["reg"],
            Alvo::FileZilla | Alvo::WinSshTerm | Alvo::MRemoteNg => &["xml"],
            Alvo::WinScp => &["ini"],
            Alvo::MobaXterm => &["mxtsessions"],
        }
    }

    /// Se o formato leva as senhas (a caixa "incluir senhas" só vale nesses).
    pub fn guarda_senhas(self) -> bool {
        matches!(
            self,
            Alvo::Nativo | Alvo::FileZilla | Alvo::WinScp | Alvo::MRemoteNg
        )
    }

    /// O formato em que a chave do cofre sai para esse programa; `None` quando o formato não
    /// tem campo de chave (o nativo leva as chaves dentro do bloco `cofre`).
    pub fn formato_de_chave(self) -> Option<PrivateFormat> {
        match self {
            Alvo::SshConfig | Alvo::MobaXterm => Some(PrivateFormat::OpenSsh),
            Alvo::Putty | Alvo::WinScp | Alvo::FileZilla | Alvo::WinSshTerm => {
                Some(PrivateFormat::PpkV3)
            }
            Alvo::Nativo | Alvo::MRemoteNg => None,
        }
    }

    /// Se o programa conhece o tipo de conexão.
    pub fn aceita(self, tipo: Kind) -> bool {
        match self {
            Alvo::Nativo => true,
            Alvo::SshConfig | Alvo::WinSshTerm | Alvo::MobaXterm => tipo == Kind::Ssh,
            Alvo::Putty | Alvo::MRemoteNg => matches!(tipo, Kind::Ssh | Kind::Telnet),
            Alvo::FileZilla => matches!(tipo, Kind::Sftp | Kind::Ftp | Kind::S3),
            Alvo::WinScp => matches!(tipo, Kind::Ssh | Kind::Sftp | Kind::Ftp | Kind::S3),
        }
    }
}

/// O que a pessoa marcou no diálogo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opcoes {
    /// Levar as senhas (só vale nos formatos que as guardam).
    pub senhas: bool,
    /// Levar as chaves do cofre que as conexões usam, numa pasta ao lado do arquivo.
    pub chaves: bool,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            senhas: false,
            chaves: true,
        }
    }
}

/// A fonte dos bytes aleatórios (sal, nonce, enchimento e GUIDs).
pub trait Aleatorio {
    /// Enche `destino` de bytes aleatórios.
    fn preencher(&mut self, destino: &mut [u8]);

    /// Um byte só.
    fn byte(&mut self) -> u8 {
        let mut b = [0u8; 1];
        self.preencher(&mut b);
        b[0]
    }
}

/// O gerador do sistema, o mesmo que o cofre usa para os nonces.
pub struct Sistema;

impl Aleatorio for Sistema {
    fn preencher(&mut self, destino: &mut [u8]) {
        getrandom::fill(destino).expect("o sistema precisa de uma fonte de números aleatórios");
    }
}

/// Um gerador de semente fixa, para os testes (xorshift: nada de segurança aqui).
pub struct Semente(pub u64);

impl Aleatorio for Semente {
    fn preencher(&mut self, destino: &mut [u8]) {
        for byte in destino {
            let mut x = self.0.max(1);
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            *byte = (x >> 24) as u8;
        }
    }
}

/// O que uma exportação produziu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resultado {
    /// O texto do arquivo.
    pub texto: String,
    /// Quantas conexões entraram no arquivo.
    pub exportadas: usize,
    /// O que ficou de fora ou mudou, uma frase por item, para o resumo.
    pub avisos: Vec<String>,
    /// Os arquivos de anexo que acompanham o JSON nativo (vazio nos outros).
    pub anexos: Vec<ArquivoDeAnexo>,
}

impl Resultado {
    /// Acrescenta um aviso, sem repetir o mesmo texto.
    pub fn avisar(&mut self, texto: impl Into<String>) {
        let texto = texto.into();
        if !self.avisos.contains(&texto) {
            self.avisos.push(texto);
        }
    }

    /// O resumo que a tela mostra: quantas conexões e cada aviso numa linha.
    pub fn resumo(&self) -> String {
        let mut linhas = vec![t!("exportar-resumo", n = self.exportadas as i64)];
        linhas.extend(self.avisos.iter().cloned());
        linhas.join("\n")
    }
}

/// Os bytes que vão para o arquivo do `alvo`: UTF-8, menos o `.reg` do PuTTY, que o `regedit`
/// só lê em UTF-16 LE com BOM e fim de linha `\r\n`.
pub fn bytes_do_arquivo(alvo: Alvo, texto: &str) -> Vec<u8> {
    if alvo != Alvo::Putty {
        return texto.as_bytes().to_vec();
    }
    let mut bytes = vec![0xFF, 0xFE];
    for unidade in texto
        .replace("\r\n", "\n")
        .replace('\n', "\r\n")
        .encode_utf16()
    {
        bytes.extend_from_slice(&unidade.to_le_bytes());
    }
    bytes
}

/// Monta o [`Plano`] e escreve o arquivo do formato. `chaves` leva o caminho de cada chave do
/// cofre já gravada (uid → arquivo; vazio quando a caixa está desligada). `json` é o arquivo que
/// vai receber o texto: o nome dele dá o da pasta dos anexos do formato nativo.
pub fn exportar(
    alvo: Alvo,
    arvore: &Tree,
    cofre: Option<&Cofre>,
    opcoes: Opcoes,
    chaves: &HashMap<String, std::path::PathBuf>,
    destino: &Path,
    aleatorio: &mut dyn Aleatorio,
) -> Resultado {
    // O formato que não guarda senha nunca as leva, mesmo que a caixa chegue ligada.
    let opcoes = Opcoes {
        senhas: opcoes.senhas && alvo.guarda_senhas(),
        ..opcoes
    };
    if alvo == Alvo::Nativo {
        return nativo::exportar(arvore, cofre, opcoes, destino);
    }
    let plano = Plano::montar(arvore, cofre, opcoes, chaves);
    let (aceitos, mut avisos) = separar(alvo, &plano);
    let mut resultado = match alvo {
        Alvo::Nativo => unreachable!("o nativo saiu logo acima"),
        Alvo::SshConfig => ssh_config::exportar(&plano, &aceitos),
        Alvo::Putty => putty::exportar(&plano, &aceitos),
        Alvo::FileZilla => filezilla::exportar(&plano, &aceitos, opcoes),
        Alvo::WinScp => winscp::exportar(&plano, &aceitos, opcoes, aleatorio),
        Alvo::WinSshTerm => winsshterm::exportar(&plano, &aceitos),
        Alvo::MRemoteNg => mremoteng::exportar(&plano, &aceitos, opcoes, aleatorio),
        Alvo::MobaXterm => mobaxterm::exportar(&plano, &aceitos),
    };
    resultado.exportadas = aceitos.len();
    avisos.append(&mut resultado.avisos);
    resultado.avisos = avisos;
    if alvo.formato_de_chave().is_some() {
        let sem_arquivo = aceitos
            .iter()
            .filter(|i| i.usa_chave_do_cofre && i.conexao.key_file.is_none())
            .count();
        match sem_arquivo {
            0 => {}
            n => resultado.avisar(t!("exportar-chave-do-cofre", n = n as i64)),
        }
    }
    resultado
}

/// Separa as conexões que o formato leva das que ficam de fora, com um aviso por tipo
/// ("3 conexões FTP não cabem no formato do WinSSHTerm e ficaram de fora").
fn separar(alvo: Alvo, plano: &Plano) -> (Vec<&ItemPlano>, Vec<String>) {
    let mut aceitos = Vec::new();
    let mut fora: Vec<(Kind, usize)> = Vec::new();
    for item in &plano.itens {
        if alvo.aceita(item.conexao.kind) {
            aceitos.push(item);
        } else {
            match fora.iter_mut().find(|(k, _)| *k == item.conexao.kind) {
                Some((_, quantas)) => *quantas += 1,
                None => fora.push((item.conexao.kind, 1)),
            }
        }
    }
    let avisos = fora
        .into_iter()
        .map(|(tipo, quantas)| {
            t!(
                "exportar-fora-do-formato",
                n = quantas as i64,
                tipo = tipo.nome(),
                programa = alvo.do_programa()
            )
        })
        .collect();
    (aceitos, avisos)
}

// ------------------------------------------------------------ ajudas dos formatos

/// Escapa o texto para um atributo ou um elemento XML.
pub fn escapar_xml(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '>' => saida.push_str("&gt;"),
            '"' => saida.push_str("&quot;"),
            '\'' => saida.push_str("&apos;"),
            // O XML 1.0 não aceita os caracteres de controle (menos tab, LF e CR).
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => saida.push(c),
        }
    }
    saida
}

/// Entrega nomes que não se repetem (sem diferenciar maiúsculas): `ACME`, `ACME (2)`, `ACME (3)`…
#[derive(Default)]
pub struct NomesUnicos {
    usados: HashSet<String>,
}

impl NomesUnicos {
    /// O `base`, ou `base (n)` se ele já saiu.
    pub fn unico(&mut self, base: &str) -> String {
        let mut candidato = base.to_owned();
        let mut n = 1;
        while !self.usados.insert(candidato.to_lowercase()) {
            n += 1;
            candidato = format!("{base} ({n})");
        }
        candidato
    }
}

/// `usuario@host:porta` de um salto, com a porta só quando não é a 22 e o IPv6 entre colchetes.
pub fn salto_em_texto(host: &str, porta: u16, usuario: &str) -> String {
    let servidor = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    let mut texto = if usuario.is_empty() {
        servidor
    } else {
        format!("{usuario}@{servidor}")
    };
    if porta != 22 && porta != 0 {
        texto.push_str(&format!(":{porta}"));
    }
    texto
}

#[cfg(test)]
mod ida_e_volta;

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_alvos_tem_ids_unicos() {
        let ids: HashSet<&str> = Alvo::TODOS.iter().map(|a| a.id()).collect();
        assert_eq!(ids.len(), Alvo::TODOS.len());
    }

    #[test]
    fn o_reg_sai_em_utf16_com_bom_e_crlf_e_o_importador_le_de_volta() {
        let texto = "Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Software]\n\"Nome\"=\"ação\"\n";
        let bytes = bytes_do_arquivo(Alvo::Putty, texto);
        assert_eq!(&bytes[..2], [0xFF, 0xFE]);
        let lido = crate::importar::texto_do_arquivo(&bytes);
        assert_eq!(lido, texto.replace('\n', "\r\n"));
        assert_eq!(bytes_do_arquivo(Alvo::WinScp, texto), texto.as_bytes());
    }

    #[test]
    fn so_o_que_guarda_senha_aceita_a_caixa() {
        assert!(Alvo::WinScp.guarda_senhas());
        assert!(Alvo::MRemoteNg.guarda_senhas());
        assert!(!Alvo::Putty.guarda_senhas());
        assert!(!Alvo::SshConfig.guarda_senhas());
        assert!(!Alvo::WinSshTerm.guarda_senhas());
        assert!(!Alvo::MobaXterm.guarda_senhas());
    }

    #[test]
    fn nomes_unicos_ganham_sufixo_sem_diferenciar_maiusculas() {
        let mut nomes = NomesUnicos::default();
        assert_eq!(nomes.unico("Web"), "Web");
        assert_eq!(nomes.unico("web"), "web (2)");
        assert_eq!(nomes.unico("WEB"), "WEB (3)");
        assert_eq!(nomes.unico("Outro"), "Outro");
    }

    #[test]
    fn o_xml_sai_escapado() {
        assert_eq!(
            escapar_xml("a & <b> \"c\" 'd'\u{1}"),
            "a &amp; &lt;b&gt; &quot;c&quot; &apos;d&apos;"
        );
    }

    #[test]
    fn o_salto_em_texto() {
        assert_eq!(salto_em_texto("gw", 22, "ana"), "ana@gw");
        assert_eq!(salto_em_texto("gw", 2200, ""), "gw:2200");
        assert_eq!(salto_em_texto("::1", 22, "ana"), "ana@[::1]");
    }

    #[test]
    fn a_semente_repete_a_sequencia() {
        let mut a = Semente(7);
        let mut b = Semente(7);
        let (mut x, mut y) = ([0u8; 16], [0u8; 16]);
        a.preencher(&mut x);
        b.preencher(&mut y);
        assert_eq!(x, y);
        assert_ne!(x, [0u8; 16]);
    }
}
