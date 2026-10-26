//! Importar conexões de outros gerenciadores (M38): WinSSHTerm, FileZilla, WinSCP, PuTTY e
//! KiTTY, mRemoteNG, MobaXterm e o JSON do próprio Tirech Omni.
//!
//! Tudo aqui é **puro**: cada formato recebe o texto do arquivo e devolve uma [`Importacao`],
//! sem ler disco, sem tocar no cofre e sem signal. A tela faz o IO, pergunta a senha-mestra
//! quando precisa, grava no cofre e na árvore. Os arquivos vêm de fora (dado não confiável):
//! nenhum parser entra em pânico com um arquivo torto, devolve `Err` ou um aviso.

pub mod aplicar;
pub mod cripto;
pub mod filezilla;
pub mod mobaxterm;
pub mod mremoteng;
pub mod nativo;
pub mod putty;
pub mod winscp;
pub mod winsshterm;
pub mod xml;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::cofre::{Lote, Mapa};
use crate::modelo::{ComandosAoConectar, Connection, S3Opcoes, S3Provedor, SessaoPersistente};
use crate::redes::{PerfilRede, ProxyRede, Rede, TipoProxy};
use crate::t;

/// Um perfil de rede (M34) que a importação cria, com o usuário e a senha do proxy à parte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerfilTrazido {
    /// O perfil, com `id = 0` (a árvore atribui ao inserir).
    pub perfil: PerfilRede,
    /// O usuário do proxy; vai para o cofre junto com a senha.
    pub usuario: String,
    /// A senha do proxy, quando o formato a guarda.
    pub senha: Option<String>,
}

/// Uma pasta que o arquivo tem, na ordem em que aparece: também as vazias.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PastaTrazida {
    /// O caminho a partir do grupo de destino (`["Clientes", "ACME"]`).
    pub caminho: Vec<String>,
    /// `Direta` ou `Herdar` (o perfil, se houver, vai em [`PastaTrazida::perfil`]).
    pub rede: Rede,
    /// O perfil de rede do grupo, só no JSON nativo.
    pub perfil: Option<PerfilTrazido>,
    /// Os comandos ao conectar da pasta, só no JSON nativo.
    pub comandos: ComandosAoConectar,
    /// A sessão persistente (tmux) da pasta, só no JSON nativo.
    pub persistente: SessaoPersistente,
}

/// Uma conexão trazida de fora, com as senhas recuperadas (quando deu) à parte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trazida {
    /// A conexão, com `id = 0`.
    pub conexao: Connection,
    /// O caminho de pastas a partir do grupo de destino.
    pub pasta: Vec<String>,
    /// A senha da conexão; vai para o cofre se a pessoa mandar.
    pub senha: Option<String>,
    /// A senha do salto.
    pub senha_salto: Option<String>,
    /// O token de sessão do S3.
    pub token_s3: Option<String>,
    /// O perfil de rede (proxy) da conexão; vale no lugar de `conexao.rede`.
    pub perfil: Option<PerfilTrazido>,
}

impl Trazida {
    /// Uma conexão sem senha e sem rede própria.
    pub fn nova(conexao: Connection, pasta: Vec<String>) -> Self {
        Self {
            conexao,
            pasta,
            senha: None,
            senha_salto: None,
            token_s3: None,
            perfil: None,
        }
    }
}

/// Um item do arquivo, na ordem em que aparece.
// Uma importação tem dezenas de itens: pôr a pasta numa `Box` só mudaria cada `Entrada::Pasta(..)`
// dos parsers e dos testes, sem ganho que se note.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entrada {
    /// Uma pasta.
    Pasta(PastaTrazida),
    /// Uma conexão.
    Conexao(Box<Trazida>),
}

/// O resultado de um parser: o que trazer e o que avisar.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Importacao {
    /// As pastas e as conexões, na ordem do arquivo.
    pub entradas: Vec<Entrada>,
    /// O que não coube ou não deu para recuperar ("3 conexões RDP não são suportadas…").
    pub avisos: Vec<String>,
    /// mRemoteNG com a senha-mestra diferente da padrão: a tela pergunta e analisa de novo.
    pub pede_senha_mestra: bool,
    /// O cofre que veio junto (só no JSON nativo): itens ligados às conexões pelos ids dele.
    pub cofre: Option<Lote>,
}

impl Importacao {
    /// Acrescenta uma conexão.
    pub fn conexao(&mut self, trazida: Trazida) {
        self.entradas.push(Entrada::Conexao(Box::new(trazida)));
    }

    /// Acrescenta uma pasta sem rede própria.
    pub fn pasta(&mut self, caminho: Vec<String>) {
        self.entradas.push(Entrada::Pasta(PastaTrazida {
            caminho,
            ..PastaTrazida::default()
        }));
    }

    /// Acrescenta um aviso, sem repetir o mesmo texto.
    pub fn avisar(&mut self, texto: impl Into<String>) {
        let texto = texto.into();
        if !self.avisos.contains(&texto) {
            self.avisos.push(texto);
        }
    }

    /// As conexões, na ordem.
    pub fn conexoes(&self) -> impl Iterator<Item = &Trazida> {
        self.entradas.iter().filter_map(|e| match e {
            Entrada::Conexao(t) => Some(t.as_ref()),
            Entrada::Pasta(_) => None,
        })
    }

    /// As conexões, para mudar.
    pub fn conexoes_mut(&mut self) -> impl Iterator<Item = &mut Trazida> {
        self.entradas.iter_mut().filter_map(|e| match e {
            Entrada::Conexao(t) => Some(t.as_mut()),
            Entrada::Pasta(_) => None,
        })
    }

    /// Quantas conexões.
    pub fn total(&self) -> usize {
        self.conexoes().count()
    }

    /// Quantos grupos o arquivo tem (cada caminho conta uma vez, com os de cima).
    pub fn grupos(&self) -> usize {
        let mut vistos: HashSet<Vec<String>> = HashSet::new();
        let mut caminhos: Vec<&Vec<String>> = Vec::new();
        for entrada in &self.entradas {
            match entrada {
                Entrada::Pasta(p) => caminhos.push(&p.caminho),
                Entrada::Conexao(t) => caminhos.push(&t.pasta),
            }
        }
        for caminho in caminhos {
            for n in 1..=caminho.len() {
                vistos.insert(caminho[..n].to_vec());
            }
        }
        vistos.len()
    }

    /// Quantas senhas (da conexão, do salto, do S3 e dos proxies) o arquivo trouxe.
    pub fn senhas(&self) -> usize {
        let das_conexoes: usize = self
            .conexoes()
            .map(|t| {
                usize::from(t.senha.is_some())
                    + usize::from(t.senha_salto.is_some())
                    + usize::from(t.token_s3.is_some())
                    + usize::from(t.perfil.as_ref().is_some_and(|p| p.senha.is_some()))
            })
            .sum();
        let das_pastas = self
            .entradas
            .iter()
            .filter(|e| {
                matches!(e, Entrada::Pasta(p) if p.perfil.as_ref().is_some_and(|x| x.senha.is_some()))
            })
            .count();
        das_conexoes + das_pastas + self.cofre.as_ref().map_or(0, |c| c.itens.len())
    }

    /// Troca os ids de itens do cofre que vieram no arquivo (`credencial`, `jump.credencial`,
    /// `nota`, o proxy dos perfis e a identidade de rede (Nebula, OpenVPN, WireGuard ou Tailscale) deles) pelos que o cofre deu ao lote. Um id que não está no mapa
    /// vale como `None`.
    pub fn religar(&mut self, mapa: &Mapa) {
        let troca = |id: &mut Option<u64>| {
            *id = id.and_then(|antigo| mapa.itens.get(&antigo).copied());
        };
        let perfil = |p: &mut PerfilTrazido| {
            if let Some(proxy) = p.perfil.proxy.as_mut() {
                troca(&mut proxy.credencial);
            }
            if let Some(salto) = p.perfil.salto.as_mut() {
                troca(&mut salto.credencial);
            }
            if let Some(nebula) = p.perfil.nebula.as_mut() {
                troca(&mut nebula.identidade);
            }
            if let Some(vpn) = p.perfil.openvpn.as_mut() {
                troca(&mut vpn.identidade);
            }
            if let Some(wg) = p.perfil.wireguard.as_mut() {
                troca(&mut wg.identidade);
            }
            if let Some(ts) = p.perfil.tailscale.as_mut() {
                troca(&mut ts.identidade);
            }
            if let Some(zt) = p.perfil.zerotier.as_mut() {
                troca(&mut zt.identidade);
            }
        };
        for entrada in &mut self.entradas {
            match entrada {
                Entrada::Pasta(p) => {
                    if let Some(x) = p.perfil.as_mut() {
                        perfil(x);
                    }
                    p.comandos.religar_senhas(|id| mapa.itens.get(&id).copied());
                }
                Entrada::Conexao(t) => {
                    troca(&mut t.conexao.credencial);
                    troca(&mut t.conexao.nota);
                    t.conexao
                        .comandos
                        .religar_senhas(|id| mapa.itens.get(&id).copied());
                    if let Some(j) = t.conexao.jump.as_mut() {
                        troca(&mut j.credencial);
                    }
                    if let Some(x) = t.perfil.as_mut() {
                        perfil(x);
                    }
                }
            }
        }
    }
}

/// Um formato de origem. Cada um implementa [`Formato::analisar`].
pub trait Formato {
    /// O identificador, para os ids da tela (`winscp`).
    fn id(&self) -> &'static str;
    /// O nome na tela ("WinSCP").
    fn nome(&self) -> &'static str;
    /// Os nomes de arquivo comuns (`WinSCP.ini`), para a dica e o filtro do diálogo.
    fn arquivo_padrao(&self) -> &'static str;
    /// As extensões do filtro do diálogo, sem o ponto.
    fn extensoes(&self) -> &'static [&'static str];
    /// Onde o arquivo costuma ficar, a partir da pasta pessoal, para pré-preencher o diálogo.
    fn locais_provaveis(&self, home: &Path) -> Vec<PathBuf>;
    /// Lê o texto (e a senha-mestra, se um `analisar` anterior pediu) e devolve a árvore.
    fn analisar(&self, texto: &str, senha_mestra: Option<&str>) -> Result<Importacao, String>;
}

/// Os formatos de entrada, na ordem do menu "Importar ▸".
pub fn formatos() -> Vec<Box<dyn Formato>> {
    vec![
        Box::new(winsshterm::WinSshTerm),
        Box::new(filezilla::FileZilla),
        Box::new(winscp::WinScp),
        Box::new(putty::Putty),
        Box::new(mremoteng::MRemoteNg),
        Box::new(mobaxterm::MobaXterm),
        Box::new(nativo::Nativo),
    ]
}

/// O formato de `id` dado.
pub fn formato(id: &str) -> Option<Box<dyn Formato>> {
    formatos().into_iter().find(|f| f.id() == id)
}

/// O texto de um arquivo: UTF-8 (com ou sem BOM) ou UTF-16 com BOM (o `regedit` exporta em
/// UTF-16 LE). Byte inválido vira `�`; nada falha.
pub fn texto_do_arquivo(bytes: &[u8]) -> String {
    let utf16 = |dados: &[u8], le: bool| {
        let unidades: Vec<u16> = dados
            .chunks_exact(2)
            .map(|p| {
                if le {
                    u16::from_le_bytes([p[0], p[1]])
                } else {
                    u16::from_be_bytes([p[0], p[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&unidades)
    };
    match bytes {
        [0xFF, 0xFE, resto @ ..] => utf16(resto, true),
        [0xFE, 0xFF, resto @ ..] => utf16(resto, false),
        [0xEF, 0xBB, 0xBF, resto @ ..] => String::from_utf8_lossy(resto).into_owned(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Desfaz o `%XX` (os bytes viram UTF-8; um `%` sem dois dígitos hexa fica como está).
pub fn decodificar_percentual(texto: &str) -> String {
    let bytes = texto.as_bytes();
    let mut saida: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let par = (digito_hexa(bytes[i + 1]), digito_hexa(bytes[i + 2]));
            if let (Some(a), Some(b)) = par {
                saida.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        saida.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

fn digito_hexa(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Escreve `%XX` em cada byte que não é letra ou dígito ASCII nem está em `manter`.
pub fn codificar_percentual(texto: &str, manter: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for &b in texto.as_bytes() {
        if b.is_ascii_alphanumeric() || (b.is_ascii() && manter.as_bytes().contains(&b)) {
            saida.push(char::from(b));
        } else {
            saida.push_str(&format!("%{b:02X}"));
        }
    }
    saida
}

/// Uma seção de INI: o nome e as linhas `chave=valor`, na ordem.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Secao {
    /// O nome entre colchetes (vazio para o que vem antes da primeira seção).
    pub nome: String,
    /// As linhas `chave=valor`.
    pub linhas: Vec<(String, String)>,
}

impl Secao {
    /// O valor de `chave` (sem diferenciar maiúsculas).
    pub fn texto(&self, chave: &str) -> Option<&str> {
        self.linhas
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(chave))
            .map(|(_, v)| v.as_str())
    }

    /// O valor de `chave`, ou vazio.
    pub fn texto_ou_vazio(&self, chave: &str) -> &str {
        self.texto(chave).unwrap_or_default()
    }

    /// O valor de `chave` como inteiro.
    pub fn numero(&self, chave: &str) -> Option<i64> {
        self.texto(chave).and_then(|v| v.trim().parse().ok())
    }
}

/// Lê um INI: seções `[nome]`, linhas `chave=valor`, comentários com `;`. Aceita BOM.
pub fn ler_ini(texto: &str) -> Vec<Secao> {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    let mut secoes: Vec<Secao> = vec![Secao::default()];
    for linha in texto.lines() {
        let linha = linha.trim_end_matches('\r');
        let aparada = linha.trim();
        if aparada.is_empty() || aparada.starts_with(';') {
            continue;
        }
        if let Some(resto) = aparada.strip_prefix('[')
            && let Some(nome) = resto.strip_suffix(']')
        {
            secoes.push(Secao {
                nome: nome.to_owned(),
                linhas: Vec::new(),
            });
            continue;
        }
        if let Some((chave, valor)) = linha.split_once('=')
            && let Some(atual) = secoes.last_mut()
        {
            atual
                .linhas
                .push((chave.trim().to_owned(), valor.to_owned()));
        }
    }
    secoes.retain(|s| !s.nome.is_empty() || !s.linhas.is_empty());
    secoes
}

/// O valor de uma chave do registro num `.reg`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValorReg {
    /// `"nome"="texto"`.
    Texto(String),
    /// `"nome"=dword:0000000a`.
    Dword(u32),
    /// Binário e outros: o importador não usa.
    Outro,
}

/// Uma chave do registro (`[HKEY_…\Sessions\NOME]`) e os valores dela.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChaveReg {
    /// O caminho entre colchetes.
    pub caminho: String,
    /// Os valores, na ordem.
    pub valores: Vec<(String, ValorReg)>,
}

impl ChaveReg {
    /// O valor de texto `nome`.
    pub fn texto(&self, nome: &str) -> Option<&str> {
        self.valores.iter().find_map(|(n, v)| match v {
            ValorReg::Texto(t) if n == nome => Some(t.as_str()),
            _ => None,
        })
    }

    /// O valor `nome` como número (um `dword`, ou um texto com dígitos).
    pub fn numero(&self, nome: &str) -> Option<u32> {
        self.valores.iter().find_map(|(n, v)| match v {
            ValorReg::Dword(d) if n == nome => Some(*d),
            ValorReg::Texto(t) if n == nome => t.trim().parse().ok(),
            _ => None,
        })
    }
}

/// Desfaz as barras de um texto entre aspas do `.reg` (`\\` e `\"`).
fn desfazer_aspas(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut chars = texto.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(outro) => saida.push(outro),
                None => saida.push('\\'),
            }
        } else {
            saida.push(c);
        }
    }
    saida
}

/// Lê o texto entre aspas que começa em `texto` (que começa com `"`) e devolve o conteúdo (sem
/// as aspas, já sem as barras) e o que sobra depois da aspa final.
fn texto_entre_aspas(texto: &str) -> Option<(String, &str)> {
    let resto = texto.strip_prefix('"')?;
    let mut anterior_barra = false;
    for (i, c) in resto.char_indices() {
        if c == '"' && !anterior_barra {
            return Some((desfazer_aspas(&resto[..i]), &resto[i + 1..]));
        }
        anterior_barra = c == '\\' && !anterior_barra;
    }
    None
}

/// Lê um `.reg`: o cabeçalho (`Windows Registry Editor Version 5.00` ou `REGEDIT4`), as chaves
/// entre colchetes e os valores de texto e `dword`. Linhas de continuação (`hex:…,\`) são
/// juntas e ignoradas.
pub fn ler_reg(texto: &str) -> Vec<ChaveReg> {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    let mut chaves: Vec<ChaveReg> = Vec::new();
    let mut linhas = texto.lines();
    while let Some(bruta) = linhas.next() {
        let mut linha = bruta.trim().to_owned();
        // Valor binário em várias linhas: termina em `\` e continua na seguinte.
        while linha.ends_with('\\') && linha.contains("hex") && !linha.starts_with('"') {
            match linhas.next() {
                Some(proxima) => {
                    linha.pop();
                    linha.push_str(proxima.trim());
                }
                None => break,
            }
        }
        if let Some(resto) = linha.strip_prefix('[') {
            if let Some(caminho) = resto.strip_suffix(']') {
                chaves.push(ChaveReg {
                    caminho: caminho.to_owned(),
                    valores: Vec::new(),
                });
            }
            continue;
        }
        let Some(atual) = chaves.last_mut() else {
            continue;
        };
        let Some((nome, resto)) = texto_entre_aspas(&linha) else {
            continue;
        };
        let Some(valor) = resto.trim_start().strip_prefix('=') else {
            continue;
        };
        let valor = valor.trim();
        let valor = if valor.starts_with('"') {
            match texto_entre_aspas(valor) {
                Some((t, _)) => ValorReg::Texto(t),
                None => ValorReg::Outro,
            }
        } else if let Some(hexa) = valor.strip_prefix("dword:") {
            match u32::from_str_radix(hexa.trim(), 16) {
                Ok(n) => ValorReg::Dword(n),
                Err(_) => ValorReg::Outro,
            }
        } else {
            ValorReg::Outro
        };
        atual.valores.push((nome, valor));
    }
    chaves
}

/// O nome da conexão: o `nome` do arquivo, ou o `host` quando ele vem vazio.
pub fn nome_da_conexao(nome: &str, host: &str) -> String {
    let nome = nome.trim();
    if nome.is_empty() { host.trim() } else { nome }.to_owned()
}

/// O aviso de itens com protocolo não suportado, no singular ou no plural:
/// `1 conexão com protocolo X não é suportada e foi ignorada.`
pub fn nao_suportadas(quantas: usize, tipo: Ignorado, complemento: &str) -> String {
    let n = quantas as i64;
    match tipo {
        Ignorado::Conexao => t!(
            "importar-nao-suportada-conexao",
            n = n,
            complemento = complemento
        ),
        Ignorado::Sessao => t!(
            "importar-nao-suportada-sessao",
            n = n,
            complemento = complemento
        ),
    }
}

/// O que o aviso de [`nao_suportadas`] conta: conexões (FileZilla, mRemoteNG) ou sessões (PuTTY,
/// WinSCP, MobaXterm).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ignorado {
    Conexao,
    Sessao,
}

/// O perfil de rede de um proxy SOCKS5 ou HTTP: `Proxy importado (host:porta)`. A tela junta os
/// que forem iguais.
pub fn perfil_de_proxy(
    tipo: TipoProxy,
    host: &str,
    porta: u16,
    usuario: &str,
    senha: Option<String>,
) -> PerfilTrazido {
    let mut perfil = PerfilRede::new(t!(
        "importar-proxy-importado",
        host = host,
        porta = porta.to_string()
    ));
    perfil.proxy = Some(ProxyRede {
        tipo,
        host: host.to_owned(),
        port: porta,
        credencial: None,
    });
    PerfilTrazido {
        perfil,
        usuario: usuario.to_owned(),
        senha,
    }
}

/// As opções de um S3 a partir do endereço do serviço (`s3.eu-west-1.amazonaws.com`, sem o
/// esquema, ou com ele): acha o provedor e a região pelo nome do servidor e usa HTTPS, a não ser
/// que `https` seja falso ou o texto já traga `http://`.
pub fn opcoes_s3(endereco: &str, porta: u16, https: bool, bucket: &str) -> S3Opcoes {
    let endereco = endereco.trim();
    let (esquema, servidor) = match endereco.split_once("://") {
        Some((esquema, resto)) => (esquema.to_ascii_lowercase(), resto),
        None => (if https { "https" } else { "http" }.to_owned(), endereco),
    };
    let servidor = servidor.trim_end_matches('/');
    let minusculo = servidor.to_ascii_lowercase();
    let provider = if minusculo.ends_with("amazonaws.com") {
        S3Provedor::Amazon
    } else if minusculo.ends_with("wasabisys.com") {
        S3Provedor::Wasabi
    } else if minusculo.ends_with("backblazeb2.com") {
        S3Provedor::BackblazeB2
    } else if minusculo.ends_with("digitaloceanspaces.com") {
        S3Provedor::DigitalOcean
    } else if minusculo.ends_with("r2.cloudflarestorage.com") {
        S3Provedor::CloudflareR2
    } else {
        S3Provedor::Outro
    };
    let mut opcoes = S3Opcoes::do_provedor(provider);
    // A região vem do nome (`s3.<região>.amazonaws.com`), quando o provedor a põe lá.
    let partes: Vec<&str> = minusculo.split('.').collect();
    if matches!(
        provider,
        S3Provedor::Amazon | S3Provedor::Wasabi | S3Provedor::BackblazeB2
    ) && partes.len() >= 4
        && partes[0] == "s3"
    {
        opcoes.region = partes[1].to_owned();
    }
    let padrao = if esquema == "http" { 80 } else { 443 };
    opcoes.endpoint = if porta == 0 || porta == padrao {
        format!("{esquema}://{servidor}")
    } else {
        format!("{esquema}://{servidor}:{porta}")
    };
    opcoes.bucket = bucket.trim_matches('/').to_owned();
    opcoes
}

/// Avisa, uma vez só, que há chave `.ppk` (formato do PuTTY).
pub fn avisar_ppk(importacao: &mut Importacao, arquivo: &str) {
    if arquivo.to_ascii_lowercase().ends_with(".ppk") {
        importacao.avisar(t!("importar-ppk"));
    }
}

/// A porta de um texto, ou `padrao` se estiver vazio, torto ou fora de `1..=65535`.
pub fn porta_ou(texto: &str, padrao: u16) -> u16 {
    match texto.trim().parse::<u32>() {
        Ok(n @ 1..=65535) => n as u16,
        _ => padrao,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn os_formatos_tem_ids_unicos_e_se_acham_pelo_id() {
        let todos = formatos();
        assert_eq!(todos.len(), 7);
        let mut ids: Vec<&str> = todos.iter().map(|f| f.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 7);
        for f in &todos {
            assert_eq!(formato(f.id()).unwrap().nome(), f.nome());
        }
        assert!(formato("remmina").is_none());
    }

    #[test]
    fn percentual_ida_e_volta() {
        assert_eq!(decodificar_percentual("ACME%20prod%2Fa"), "ACME prod/a");
        assert_eq!(decodificar_percentual("caf%C3%A9"), "café");
        assert_eq!(decodificar_percentual("100%"), "100%");
        assert_eq!(decodificar_percentual("%zz%4"), "%zz%4");
        let original = "Clientes/ACME prod é 100%";
        assert_eq!(
            decodificar_percentual(&codificar_percentual(original, "/")),
            original
        );
    }

    #[test]
    fn ini_com_secoes_e_comentarios() {
        let ini = ler_ini(
            "\u{feff}; c\r\n[Sessions\\A%20B]\r\nHostName=h\r\nPortNumber=2222\r\n\r\n[Outra]\r\nx = y z\r\n",
        );
        assert_eq!(ini.len(), 2);
        assert_eq!(ini[0].nome, "Sessions\\A%20B");
        assert_eq!(ini[0].texto("hostname"), Some("h"));
        assert_eq!(ini[0].numero("PortNumber"), Some(2222));
        assert_eq!(ini[1].texto("x"), Some(" y z"));
    }

    #[test]
    fn reg_com_texto_dword_e_continuacao() {
        let reg = "Windows Registry Editor Version 5.00\r\n\r\n\
            [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\a%20b]\r\n\
            \"HostName\"=\"h\\\\x \\\"q\\\"\"\r\n\
            \"PortNumber\"=dword:0000000a\r\n\
            \"Bin\"=hex:01,02,\\\r\n  03,04\r\n\
            \"Protocol\"=\"ssh\"\r\n";
        let chaves = ler_reg(reg);
        assert_eq!(chaves.len(), 1);
        let c = &chaves[0];
        assert!(c.caminho.ends_with("Sessions\\a%20b"));
        assert_eq!(c.texto("HostName"), Some("h\\x \"q\""));
        assert_eq!(c.numero("PortNumber"), Some(10));
        assert_eq!(c.texto("Protocol"), Some("ssh"));
    }

    #[test]
    fn arquivo_em_utf16_e_em_utf8_com_bom() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "Olá".encode_utf16() {
            bytes.extend(u.to_le_bytes());
        }
        assert_eq!(texto_do_arquivo(&bytes), "Olá");
        assert_eq!(texto_do_arquivo(b"\xEF\xBB\xBFoi"), "oi");
        assert_eq!(texto_do_arquivo(b"ab\xFFc"), "ab\u{FFFD}c");
    }

    #[test]
    fn porta_fora_da_faixa_vale_o_padrao() {
        assert_eq!(porta_ou("2222", 22), 2222);
        assert_eq!(porta_ou("", 22), 22);
        assert_eq!(porta_ou("0", 22), 22);
        assert_eq!(porta_ou("70000", 22), 22);
    }
}
