//! As chaves SSH no cofre e fora dele: importar (de arquivo ou colada, em qualquer formato),
//! exportar, converter e trazer as de `~/.ssh`. Aqui não há widget: a página "Chaves SSH" chama
//! estas funções, e os testes também.
//!
//! O cofre guarda a privada em OpenSSH **sem passphrase** (ele já cifra tudo); a passphrase
//! original, se a chave tinha, fica na `senha` do item para o "Exportar como…" propô-la.

use std::fs;
use std::path::{Path, PathBuf};

use ssh_key::{Certificate, PrivateKey, PublicKey};
use tomni_ui::terminal::ssh::keys::{
    self, KdfLevel, KeyError, KeyFile, KeyFormat, KeyInfo, PrivateFormat, PublicFormat,
};

use crate::agente::{ChaveSalva, passphrase_do_cofre};
use crate::cofre::{ChaveSsh, CofreCompartilhado, Item, Tipo};
use crate::ferramentas::{ChaveLocal, Ferramentas, TAMANHO_MAXIMO_DE_CHAVE, motivo};
use crate::t;

/// O que se leu de um arquivo ou de um texto colado.
#[derive(Clone, Debug)]
pub enum Lida {
    /// Uma chave privada, em qualquer formato que o app leia.
    Privada {
        /// O formato, o tipo, o comentário e se há passphrase.
        info: KeyInfo,
        /// O texto da chave.
        texto: String,
        /// O `-cert.pub` que estava ao lado do arquivo (vazio se não havia).
        certificado: String,
    },
    /// Só a parte pública (a chave de outra pessoa, para o `authorized_keys`).
    Publica {
        /// A chave.
        publica: PublicKey,
        /// O comentário.
        comentario: String,
    },
    /// Um certificado OpenSSH (`…-cert-v01@openssh.com`).
    Certificado {
        /// A linha do certificado.
        texto: String,
        /// O fingerprint da chave que ele certifica.
        fingerprint: String,
        /// O comentário.
        comentario: String,
    },
}

impl Lida {
    /// Lê o que foi colado ou o conteúdo de um arquivo.
    pub fn ler_texto(texto: &str) -> Result<Self, String> {
        let texto = texto.trim();
        if texto.is_empty() {
            return Err(t!("chave-nada-para-ler"));
        }
        if e_certificado(texto) {
            let (fingerprint, comentario) = ler_certificado(texto)?;
            return Ok(Self::Certificado {
                texto: format!("{texto}\n"),
                fingerprint,
                comentario,
            });
        }
        match KeyFile::inspect_text(texto) {
            Ok(info) => Ok(Self::Privada {
                info,
                texto: format!("{texto}\n"),
                certificado: String::new(),
            }),
            Err(KeyError::OnlyPublic) => {
                let (publica, comentario) = keys::parse_public(texto).map_err(|e| motivo(&e))?;
                Ok(Self::Publica {
                    publica,
                    comentario,
                })
            }
            Err(erro) => Err(motivo(&erro)),
        }
    }

    /// Lê um arquivo (até 64 KiB). De uma privada, procura o `<nome>-cert.pub` ao lado.
    pub fn ler_arquivo(arquivo: &Path) -> Result<Self, String> {
        let texto = ler_texto_do_arquivo(arquivo)?;
        let mut lida = Self::ler_texto(&texto)?;
        if let Self::Privada {
            info, certificado, ..
        } = &mut lida
        {
            info.path = arquivo.to_owned();
            if let Some(fingerprint) = info.fingerprint() {
                *certificado = certificado_ao_lado(arquivo, &fingerprint).unwrap_or_default();
            }
        }
        Ok(lida)
    }

    /// Se há passphrase a pedir.
    pub fn protegida(&self) -> bool {
        matches!(self, Self::Privada { info, .. } if info.encrypted)
    }

    /// O comentário que a chave traz (vazio se não traz).
    pub fn comentario(&self) -> &str {
        match self {
            Self::Privada { info, .. } => &info.comment,
            Self::Publica { comentario, .. } | Self::Certificado { comentario, .. } => comentario,
        }
    }

    /// O fingerprint, quando se sabe sem a passphrase.
    pub fn fingerprint(&self) -> Option<String> {
        match self {
            Self::Privada { info, .. } => info.fingerprint(),
            Self::Publica { publica, .. } => Some(keys::fingerprint(publica)),
            Self::Certificado { fingerprint, .. } => Some(fingerprint.clone()),
        }
    }

    /// O título que o diálogo propõe: o comentário ou, sem ele, o nome do arquivo.
    pub fn titulo_sugerido(&self, arquivo: Option<&Path>) -> String {
        let comentario = self.comentario().trim();
        if !comentario.is_empty() {
            return comentario.to_owned();
        }
        arquivo
            .and_then(|a| a.file_stem())
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "chave".to_owned())
    }

    /// O que o diálogo mostra em "Formato": "PuTTY (.ppk) versão 3, Ed25519, com passphrase".
    pub fn descricao(&self) -> String {
        match self {
            Self::Privada { info, .. } => {
                let formato = match info.format {
                    KeyFormat::OpenSsh => "OpenSSH".to_owned(),
                    KeyFormat::Pem => t!("chave-formato-pem"),
                    KeyFormat::Pkcs8 => "PKCS#8".to_owned(),
                    KeyFormat::PpkV2 => t!("chave-formato-ppk", versao = 2),
                    KeyFormat::PpkV3 => t!("chave-formato-ppk", versao = 3),
                };
                let tipo = tipo_legivel(&info.kind, info.bits);
                let senha = if info.encrypted {
                    t!("chave-com-passphrase")
                } else {
                    t!("chave-sem-passphrase")
                };
                if tipo.is_empty() {
                    format!("{formato}, {senha}")
                } else {
                    format!("{formato}, {tipo}, {senha}")
                }
            }
            Self::Publica { publica, .. } => {
                let (tipo, bits) = keys::kind_and_bits(publica);
                t!("chave-publica-tipo", tipo = tipo_legivel(&tipo, bits))
            }
            Self::Certificado { .. } => t!("chave-certificado-openssh"),
        }
    }
}

/// "Ed25519", "RSA 3072 bits" ou, sem o tipo, vazio.
pub fn tipo_legivel(tipo: &str, bits: u32) -> String {
    match (tipo, bits) {
        ("", _) => String::new(),
        ("ED25519", _) => "Ed25519".to_owned(),
        (tipo, 0) => tipo.to_owned(),
        (tipo, bits) => t!("chave-tipo-bits", tipo = tipo, bits = i64::from(bits)),
    }
}

fn e_certificado(texto: &str) -> bool {
    texto
        .lines()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| l.split_whitespace().next())
        .is_some_and(|algoritmo| algoritmo.contains("-cert-v01@openssh.com"))
}

/// O fingerprint da chave certificada e o comentário.
fn ler_certificado(texto: &str) -> Result<(String, String), String> {
    let linha = texto
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
        .trim();
    let certificado = Certificate::from_openssh(linha)
        .map_err(|e| t!("chave-certificado-nao-pode-ser-lido", erro = e.to_string()))?;
    let publica = PublicKey::from(certificado.public_key().clone());
    Ok((
        keys::fingerprint(&publica),
        certificado.comment().to_owned(),
    ))
}

fn ler_texto_do_arquivo(arquivo: &Path) -> Result<String, String> {
    let metadados = fs::metadata(arquivo).map_err(|e| {
        t!(
            "comum-nao-consegui-ler",
            arquivo = arquivo.display().to_string(),
            erro = e.to_string()
        )
    })?;
    if metadados.len() > TAMANHO_MAXIMO_DE_CHAVE {
        return Err(t!("chave-arquivo-grande-demais"));
    }
    let bytes = fs::read(arquivo).map_err(|e| {
        t!(
            "comum-nao-consegui-ler",
            arquivo = arquivo.display().to_string(),
            erro = e.to_string()
        )
    })?;
    String::from_utf8(bytes).map_err(|_| t!("chave-arquivo-nao-e-texto"))
}

/// O `<arquivo>-cert.pub` (ou o do nome sem extensão) se ele certifica a chave `fingerprint`.
fn certificado_ao_lado(arquivo: &Path, fingerprint: &str) -> Option<String> {
    let mut candidatos = Vec::new();
    let mut nome = arquivo.as_os_str().to_owned();
    nome.push("-cert.pub");
    candidatos.push(PathBuf::from(nome));
    if arquivo.extension().is_some() {
        let mut nome = arquivo.with_extension("").into_os_string();
        nome.push("-cert.pub");
        candidatos.push(PathBuf::from(nome));
    }
    candidatos.into_iter().find_map(|c| {
        let texto = ler_texto_do_arquivo(&c).ok()?;
        let (certifica, _) = ler_certificado(texto.trim()).ok()?;
        (certifica == fingerprint).then(|| format!("{}\n", texto.trim()))
    })
}

/// Para onde a chave importada vai.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destino {
    /// O cofre (recomendado).
    Cofre,
    /// Um arquivo OpenSSH em `~/.ssh`.
    Arquivo,
}

/// O resultado de uma importação.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Importada {
    /// O item do cofre (`None` quando foi para um arquivo).
    pub item: Option<u64>,
    /// O arquivo gravado (`None` quando foi para o cofre).
    pub arquivo: Option<PathBuf>,
    /// O que dizer à pessoa.
    pub mensagem: String,
}

/// O título do item do cofre que já tem essa chave.
pub fn titulo_no_cofre(cofre: &CofreCompartilhado, fingerprint: &str) -> Option<String> {
    cofre.agora(|c| {
        c.chave_ssh_por_fingerprint(fingerprint)
            .map(|i| i.titulo.clone())
    })
}

/// "Ed25519" ou "RSA 3072 bits", do tipo da chave pública.
pub fn tipo_da_publica(publica: &PublicKey) -> String {
    let (tipo, bits) = keys::kind_and_bits(publica);
    tipo_legivel(&tipo, bits)
}

/// Troca o que não serve num nome de arquivo por `_`.
pub fn nome_de_arquivo(titulo: &str) -> String {
    let nome: String = titulo
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let nome = nome.trim_matches(['.', '_']);
    if nome.is_empty() {
        "chave".to_owned()
    } else {
        nome.to_owned()
    }
}

fn cofre_travado(cofre: &CofreCompartilhado) -> Result<(), String> {
    if cofre.agora(|c| c.bloqueado()) {
        Err(t!("chave-cofre-bloqueado-guardar"))
    } else {
        Ok(())
    }
}

/// Guarda a chave no cofre como um item "Chave SSH". Devolve o id do item.
///
/// Uma privada cuja pública já está no cofre é recusada; uma pública guardada antes ganha a
/// privada. Um certificado sozinho liga-se ao item da mesma chave.
pub fn importar_para_cofre(
    cofre: &CofreCompartilhado,
    lida: &Lida,
    titulo: &str,
    passphrase: &str,
    guardar_passphrase: bool,
) -> Result<u64, String> {
    cofre_travado(cofre)?;
    let titulo = titulo.trim();
    let titulo_padrao = Tipo::ChaveSsh.nome();
    let titulo = if titulo.is_empty() {
        titulo_padrao.as_str()
    } else {
        titulo
    };
    match lida {
        Lida::Privada {
            info,
            texto,
            certificado,
        } => {
            let aberta = KeyFile::from_text(texto, (!passphrase.is_empty()).then_some(passphrase))
                .map_err(|e| motivo(&e))?;
            let mut chave = ChaveSsh::de_chave(aberta.private_key())?;
            if let Ok((certifica, _)) = ler_certificado(certificado.trim())
                && certifica == chave.fingerprint
            {
                chave.certificado = certificado.trim().to_owned();
            }
            let senha = if guardar_passphrase && info.encrypted {
                passphrase.to_owned()
            } else {
                String::new()
            };
            let existente = cofre.agora(|c| {
                c.chave_ssh_por_fingerprint(&chave.fingerprint)
                    .map(|i| (i.id, i.titulo.clone(), i.chave.clone().unwrap_or_default()))
            });
            match existente {
                Some((_, titulo, atual)) if !atual.privada.trim().is_empty() => {
                    Err(t!("chave-ja-esta-no-cofre", titulo = titulo.as_str()))
                }
                // A pública de antes ganha a privada, mantendo o título e a política.
                Some((id, _, atual)) => {
                    let Some(mut item) = cofre.agora(|c| c.item(id).cloned()) else {
                        return Err(t!("chave-item-sumiu"));
                    };
                    item.chave = Some(ChaveSsh {
                        no_agente: atual.no_agente,
                        confirmar: atual.confirmar,
                        vida_secs: atual.vida_secs,
                        ..chave
                    });
                    if !senha.is_empty() {
                        item.senha = senha;
                    }
                    cofre.mudar(|c| c.salvar(item)).map_err(erro_de_gravacao)
                }
                None => cofre
                    .mudar(|c| {
                        c.salvar(Item {
                            tipo: Tipo::ChaveSsh,
                            titulo: titulo.to_owned(),
                            senha,
                            chave: Some(chave),
                            ..Item::default()
                        })
                    })
                    .map_err(erro_de_gravacao),
            }
        }
        Lida::Publica {
            publica,
            comentario,
        } => {
            let fingerprint = keys::fingerprint(publica);
            if let Some(existente) = titulo_no_cofre(cofre, &fingerprint) {
                return Err(t!("chave-ja-esta-no-cofre", titulo = existente.as_str()));
            }
            let chave = ChaveSsh {
                publica: keys::export_public(publica, comentario, PublicFormat::OpenSsh)
                    .map_err(|e| motivo(&e))?
                    .trim()
                    .to_owned(),
                fingerprint,
                no_agente: false,
                ..ChaveSsh::default()
            };
            cofre
                .mudar(|c| {
                    c.salvar(Item {
                        tipo: Tipo::ChaveSsh,
                        titulo: titulo.to_owned(),
                        chave: Some(chave),
                        ..Item::default()
                    })
                })
                .map_err(erro_de_gravacao)
        }
        Lida::Certificado {
            texto, fingerprint, ..
        } => {
            let Some(id) = cofre.agora(|c| c.chave_ssh_por_fingerprint(fingerprint).map(|i| i.id))
            else {
                return Err(t!("chave-certificado-sem-privada"));
            };
            let Some(mut item) = cofre.agora(|c| c.item(id).cloned()) else {
                return Err(t!("chave-item-sumiu"));
            };
            if let Some(chave) = item.chave.as_mut() {
                chave.certificado = texto.trim().to_owned();
            }
            cofre.mudar(|c| c.salvar(item)).map_err(erro_de_gravacao)
        }
    }
}

fn erro_de_gravacao(erro: std::io::Error) -> String {
    t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string())
}

/// Grava a chave como arquivo OpenSSH em `pasta` (com o `.pub` e, se houver, o `-cert.pub`).
/// Mantém a passphrase da chave; nunca sobrescreve. Devolve o arquivo.
pub fn importar_para_arquivo(
    pasta: &Path,
    lida: &Lida,
    nome: &str,
    passphrase: &str,
) -> Result<PathBuf, String> {
    let destino = pasta.join(nome_de_arquivo(nome));
    match lida {
        Lida::Privada {
            texto, certificado, ..
        } => {
            let aberta = KeyFile::from_text(texto, (!passphrase.is_empty()).then_some(passphrase))
                .map_err(|e| motivo(&e))?;
            let chave = aberta.private_key();
            let privada = keys::export_private(
                chave,
                PrivateFormat::OpenSsh,
                (!passphrase.is_empty() && aberta.info().encrypted).then_some(passphrase),
                KdfLevel::Standard,
            )
            .map_err(|e| motivo(&e))?;
            gravar_novo(&destino, &privada, true)?;
            let publica = format!("{}\n", keys::public_openssh(chave.public_key()));
            gravar_sobrescrevendo(&keys::public_path(&destino), &publica, false)?;
            if !certificado.trim().is_empty() {
                gravar_sobrescrevendo(&caminho_do_certificado(&destino), certificado, false)?;
            }
        }
        Lida::Publica {
            publica,
            comentario,
        } => {
            let linha = keys::export_public(publica, comentario, PublicFormat::OpenSsh)
                .map_err(|e| motivo(&e))?;
            let destino = keys::public_path(&destino);
            gravar_novo(&destino, &format!("{}\n", linha.trim()), false)?;
            return Ok(destino);
        }
        Lida::Certificado { texto, .. } => {
            let destino = caminho_do_certificado(&destino);
            gravar_novo(&destino, texto, false)?;
            return Ok(destino);
        }
    }
    Ok(destino)
}

fn caminho_do_certificado(privada: &Path) -> PathBuf {
    let mut nome = privada.as_os_str().to_owned();
    nome.push("-cert.pub");
    PathBuf::from(nome)
}

fn gravar_novo(arquivo: &Path, texto: &str, privado: bool) -> Result<(), String> {
    keys::write_key_text(arquivo, texto, privado, false).map_err(|erro| match erro {
        KeyError::Exists(existente) => {
            t!(
                "chave-ja-existe-arquivo",
                arquivo = existente.display().to_string()
            )
        }
        outro => motivo(&outro),
    })
}

fn gravar_sobrescrevendo(arquivo: &Path, texto: &str, privado: bool) -> Result<(), String> {
    keys::write_key_text(arquivo, texto, privado, true).map_err(|e| motivo(&e))
}

/// Que parte da chave exportar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parte {
    /// A chave privada.
    Privada,
    /// Só a pública.
    Publica,
}

/// Uma chave pronta para exportar ou converter: a privada aberta (se há) e o que a acompanha.
pub struct Aberta {
    /// O nome com que o arquivo é sugerido.
    pub nome: String,
    /// A privada, sem passphrase. `None`: só se tem a pública.
    pub privada: Option<PrivateKey>,
    /// A pública.
    pub publica: PublicKey,
    /// O comentário.
    pub comentario: String,
    /// A passphrase original, que o diálogo propõe ao exportar.
    pub passphrase: String,
    /// O certificado (vazio se não há).
    pub certificado: String,
}

impl Aberta {
    /// Abre o texto de uma chave (privada ou pública). `atual` é a passphrase dela.
    pub fn do_texto(nome: &str, texto: &str, atual: &str) -> Result<Self, String> {
        match KeyFile::from_text(texto, (!atual.is_empty()).then_some(atual)) {
            Ok(arquivo) => {
                let privada = arquivo.private_key().clone();
                Ok(Self {
                    nome: nome.to_owned(),
                    publica: privada.public_key().clone(),
                    comentario: privada.comment().to_string(),
                    privada: Some(privada),
                    passphrase: if arquivo.info().encrypted {
                        atual.to_owned()
                    } else {
                        String::new()
                    },
                    certificado: String::new(),
                })
            }
            Err(KeyError::OnlyPublic) => {
                let (publica, comentario) = keys::parse_public(texto).map_err(|e| motivo(&e))?;
                Ok(Self {
                    nome: nome.to_owned(),
                    privada: None,
                    publica,
                    comentario,
                    passphrase: String::new(),
                    certificado: String::new(),
                })
            }
            Err(erro) => Err(motivo(&erro)),
        }
    }

    /// Abre o arquivo de uma chave.
    pub fn do_arquivo(arquivo: &Path, atual: &str) -> Result<Self, String> {
        let texto = ler_texto_do_arquivo(arquivo)?;
        let nome = arquivo
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "chave".to_owned());
        let mut aberta = Self::do_texto(&nome, &texto, atual)?;
        aberta.certificado =
            certificado_ao_lado(arquivo, &keys::fingerprint(&aberta.publica)).unwrap_or_default();
        Ok(aberta)
    }

    /// Abre um item "Chave SSH" do cofre.
    pub fn do_item(item: &Item) -> Result<Self, String> {
        let Some(chave) = item.chave.as_ref() else {
            return Err(t!("chave-item-nao-e-chave"));
        };
        let nome = nome_de_arquivo(&item.titulo);
        let mut aberta = if chave.privada.trim().is_empty() {
            let (publica, comentario) =
                keys::parse_public(&chave.publica).map_err(|e| motivo(&e))?;
            Self {
                nome,
                privada: None,
                publica,
                comentario,
                passphrase: String::new(),
                certificado: String::new(),
            }
        } else {
            Self::do_texto(&nome, &chave.privada, "")?
        };
        aberta.passphrase = item.senha.clone();
        aberta.certificado = chave.certificado.clone();
        Ok(aberta)
    }

    /// Os formatos em que a privada pode sair (vazio se só se tem a pública).
    pub fn formatos_privados(&self) -> Vec<PrivateFormat> {
        self.privada
            .as_ref()
            .map(PrivateFormat::available)
            .unwrap_or_default()
    }

    /// Os formatos em que a pública pode sair.
    pub fn formatos_publicos(&self) -> Vec<PublicFormat> {
        PublicFormat::available(&self.publica)
    }

    /// O texto da parte pedida no formato pedido. `passphrase` e `repetida` valem para a
    /// privada; vazia, sai sem passphrase (e não se confirma nada).
    pub fn exportar(
        &self,
        parte: Parte,
        privado: PrivateFormat,
        publico: PublicFormat,
        passphrase: &str,
        repetida: &str,
    ) -> Result<String, String> {
        match parte {
            Parte::Privada => {
                let Some(chave) = self.privada.as_ref() else {
                    return Err(t!("chave-so-parte-publica"));
                };
                if passphrase != repetida {
                    return Err(t!("chave-passphrases-nao-conferem"));
                }
                keys::export_private(chave, privado, Some(passphrase), KdfLevel::Standard)
                    .map(|t| t.to_string())
                    .map_err(|e| motivo(&e))
            }
            Parte::Publica => keys::export_public(&self.publica, &self.comentario, publico)
                .map_err(|e| motivo(&e)),
        }
    }

    /// O nome que o "Salvar" sugere: `<nome><extensão do formato>`.
    pub fn arquivo_sugerido(
        &self,
        parte: Parte,
        privado: PrivateFormat,
        publico: PublicFormat,
    ) -> String {
        match parte {
            Parte::Privada => format!("{}{}", self.nome, privado.extension()),
            Parte::Publica => format!("{}{}", self.nome, publico.extension()),
        }
    }

    /// Grava o texto exportado em `destino`. Com a privada, grava também o `-cert.pub` se o
    /// certificado existe. O diálogo do sistema já perguntou sobre sobrescrever.
    pub fn gravar(&self, destino: &Path, parte: Parte, texto: &str) -> Result<(), String> {
        gravar_sobrescrevendo(destino, texto, parte == Parte::Privada)?;
        if parte == Parte::Privada && !self.certificado.trim().is_empty() {
            let base = if destino.extension().is_some() {
                destino.with_extension("")
            } else {
                destino.to_owned()
            };
            gravar_sobrescrevendo(
                &caminho_do_certificado(&base),
                &format!("{}\n", self.certificado.trim()),
                false,
            )?;
        }
        Ok(())
    }
}

/// Uma chave que a pessoa marcou para trazer, de `~/.ssh` ou de arquivos escolhidos.
#[derive(Clone, Debug)]
pub struct Escolhida {
    /// A chave.
    pub chave: ChaveLocal,
    /// A passphrase digitada (vazia: tenta a do cofre e as digitadas nas outras chaves).
    pub passphrase: String,
}

/// O que "Trazer todas de ~/.ssh" ou a importação de vários arquivos fez.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trazidas {
    /// Os arquivos que agora estão no cofre.
    pub trazidas: Vec<PathBuf>,
    /// Os que falharam, com o motivo.
    pub falhas: Vec<(String, String)>,
}

impl Ferramentas {
    /// Importa o que foi lido, para o cofre ou para um arquivo em `~/.ssh`. Uma chave nova do
    /// cofre entra no agente se `no_agente` (o padrão) e o agente roda.
    pub fn importar(
        &self,
        lida: &Lida,
        titulo: &str,
        passphrase: &str,
        guardar_passphrase: bool,
        destino: Destino,
    ) -> Result<Importada, String> {
        match destino {
            Destino::Cofre => {
                let id = importar_para_cofre(
                    self.cofre(),
                    lida,
                    titulo,
                    passphrase,
                    guardar_passphrase,
                )?;
                if let Err(aviso) = self.sincronizar_com_o_agente(id) {
                    self.avisar_erro(aviso);
                }
                Ok(Importada {
                    item: Some(id),
                    arquivo: None,
                    mensagem: t!("chave-guardada-no-cofre", titulo = titulo.trim()),
                })
            }
            Destino::Arquivo => {
                let arquivo = importar_para_arquivo(&self.pasta_ssh(), lida, titulo, passphrase)?;
                self.atualizar_pasta();
                Ok(Importada {
                    item: None,
                    mensagem: t!("chave-gravada-em", arquivo = arquivo.display().to_string()),
                    arquivo: Some(arquivo),
                })
            }
        }
    }

    /// Guarda no cofre uma chave que o app acabou de gerar, pelo mesmo caminho da importação
    /// ([`importar_para_cofre`]): o cofre guarda a privada sem passphrase, então a chave gerada
    /// para ele nasce sem ela. Com `no_agente`, a chave entra no agente com a origem do cofre
    /// (como as que o cofre carrega ao destravar); sem ele, o item fica com "No agente" desligado.
    ///
    /// Devolve o id do item, o título que ficou e, se a chave devia entrar no agente e não
    /// entrou, o aviso para a pessoa.
    pub fn guardar_gerada(
        &self,
        privada: &PrivateKey,
        titulo: &str,
        no_agente: bool,
    ) -> Result<(u64, String, Option<String>), String> {
        let texto = keys::export_private(privada, PrivateFormat::OpenSsh, None, KdfLevel::Standard)
            .map_err(|e| motivo(&e))?;
        let lida = Lida::ler_texto(&texto)?;
        let id = importar_para_cofre(self.cofre(), &lida, titulo, "", false)?;
        let titulo = self
            .cofre()
            .agora(|c| c.item(id).map(|i| i.titulo.clone()))
            .unwrap_or_default();
        let mut aviso = None;
        if no_agente {
            if !self.agente.rodando() {
                aviso = Some(t!("chaves-gerada-agente-desligado"));
            } else {
                // O mesmo caminho de quando o cofre destrava: põe no agente com a origem do cofre.
                self.sincronizar_cofre();
                if !self.agente.tem(&keys::fingerprint(privada.public_key())) {
                    aviso = Some(t!(
                        "chaves-gerada-fora-do-agente",
                        motivo = t!("chaves-gerada-agente-recusou")
                    ));
                }
            }
        } else {
            self.definir_no_agente(id, false)?;
        }
        Ok((id, titulo, aviso))
    }

    /// Abre um arquivo de chave para exportar ou converter, com a passphrase `atual` ou, sem
    /// ela, a que o cofre guardou.
    pub fn abrir_arquivo(&self, arquivo: &Path, atual: &str) -> Result<Aberta, String> {
        let do_cofre;
        let atual = if atual.is_empty() {
            do_cofre = passphrase_do_cofre(self.cofre(), arquivo).unwrap_or_default();
            do_cofre.as_str()
        } else {
            atual
        };
        Aberta::do_arquivo(arquivo, atual)
    }

    /// Abre a chave do item do cofre.
    pub fn abrir_item(&self, id: u64) -> Result<Aberta, String> {
        let item = self
            .cofre()
            .agora(|c| c.item(id).cloned())
            .ok_or_else(|| t!("chave-nao-esta-mais-no-cofre"))?;
        Aberta::do_item(&item)
    }

    /// Traz as chaves escolhidas para o `destino`, numa operação só: as que falham ficam em
    /// `falhas`, e as outras entram. Uma protegida sem passphrase digitada tenta a que o cofre
    /// guardou e as digitadas nas outras (a mesma passphrase costuma servir a várias chaves).
    /// Não apaga arquivo nenhum ([`Ferramentas::apagar_chave`]).
    pub fn trazer(&self, escolhidas: &[Escolhida], destino: Destino) -> Trazidas {
        let mut digitadas: Vec<&str> = Vec::new();
        for escolhida in escolhidas {
            let passphrase = escolhida.passphrase.as_str();
            if !passphrase.is_empty() && !digitadas.contains(&passphrase) {
                digitadas.push(passphrase);
            }
        }
        let mut resultado = Trazidas::default();
        for Escolhida { chave, passphrase } in escolhidas {
            match self.trazer_uma(chave, passphrase, &digitadas, destino) {
                Ok(()) => resultado.trazidas.push(chave.arquivo.clone()),
                Err(erro) => resultado.falhas.push((chave.nome.clone(), erro)),
            }
        }
        resultado
    }

    fn trazer_uma(
        &self,
        chave: &ChaveLocal,
        passphrase: &str,
        digitadas: &[&str],
        destino: Destino,
    ) -> Result<(), String> {
        let lida = Lida::ler_arquivo(&chave.arquivo)?;
        let do_cofre;
        let passphrase = match &lida {
            Lida::Privada { info, texto, .. } if info.encrypted && passphrase.is_empty() => {
                do_cofre = passphrase_do_cofre(self.cofre(), &chave.arquivo).unwrap_or_default();
                // A primeira que abre a chave; sem nenhuma, a importação diz o que faltou.
                std::iter::once(do_cofre.as_str())
                    .chain(digitadas.iter().copied())
                    .filter(|p| !p.is_empty())
                    .find(|p| KeyFile::from_text(texto, Some(p)).is_ok())
                    .unwrap_or_default()
            }
            _ => passphrase,
        };
        // No cofre o título é o comentário; em `~/.ssh` o arquivo leva o nome que já tinha.
        let titulo = if destino == Destino::Arquivo || lida.comentario().trim().is_empty() {
            chave.nome.clone()
        } else {
            lida.titulo_sugerido(Some(&chave.arquivo))
        };
        self.importar(&lida, &titulo, passphrase, true, destino)
            .map(|_| ())
    }

    /// Apaga o arquivo da chave, o `.pub` e o `-cert.pub` ao lado, e a passphrase do cofre que
    /// só servia a esse arquivo.
    pub fn apagar_chave(&self, arquivo: &Path) -> Result<(), String> {
        self.esquecer_passphrase(arquivo);
        for alvo in [
            arquivo.to_owned(),
            keys::public_path(arquivo),
            caminho_do_certificado(arquivo),
        ] {
            if alvo.exists() {
                fs::remove_file(&alvo).map_err(|e| {
                    t!(
                        "chave-nao-consegui-apagar-arquivo",
                        arquivo = alvo.display().to_string(),
                        erro = e.to_string()
                    )
                })?;
            }
        }
        self.atualizar_pasta();
        Ok(())
    }

    /// Grava a pública do item em `~/.ssh/<título>.pub`.
    pub fn por_publica_em_ssh(&self, id: u64) -> Result<PathBuf, String> {
        let aberta = self.abrir_item(id)?;
        let destino = self.pasta_ssh().join(format!("{}.pub", aberta.nome));
        let linha = keys::export_public(&aberta.publica, &aberta.comentario, PublicFormat::OpenSsh)
            .map_err(|e| motivo(&e))?;
        gravar_novo(&destino, &format!("{}\n", linha.trim()), false)?;
        self.atualizar_pasta();
        Ok(destino)
    }

    /// Grava a privada do item em `~/.ssh/<título>` (OpenSSH, sem passphrase) com o `.pub` e o
    /// `-cert.pub`.
    pub fn por_chave_em_ssh(&self, id: u64) -> Result<PathBuf, String> {
        let aberta = self.abrir_item(id)?;
        let Some(privada) = aberta.privada.as_ref() else {
            return Err(t!("chave-so-parte-publica"));
        };
        let destino = self.pasta_ssh().join(&aberta.nome);
        let texto = keys::export_private(privada, PrivateFormat::OpenSsh, None, KdfLevel::Standard)
            .map_err(|e| motivo(&e))?;
        gravar_novo(&destino, &texto, true)?;
        let publica = format!("{}\n", keys::public_openssh(&aberta.publica));
        gravar_sobrescrevendo(&keys::public_path(&destino), &publica, false)?;
        if !aberta.certificado.trim().is_empty() {
            gravar_sobrescrevendo(
                &caminho_do_certificado(&destino),
                &format!("{}\n", aberta.certificado.trim()),
                false,
            )?;
        }
        self.atualizar_pasta();
        Ok(destino)
    }

    /// Se a chave do item está no agente.
    pub fn item_no_agente(&self, id: u64) -> bool {
        let fingerprint = self.cofre().agora(|c| {
            c.item(id)
                .and_then(|i| i.chave.as_ref())
                .map(|k| k.fingerprint.clone())
        });
        fingerprint.is_some_and(|f| self.agente.tem(&f))
    }

    /// Põe no agente a chave do item, se ele manda (`no_agente`) e o agente roda; tira, se não.
    /// A chave de token (FIDO) nunca entra: o `ssh` do sistema usa o token direto.
    pub fn sincronizar_com_o_agente(&self, id: u64) -> Result<(), String> {
        let Some(item) = self.cofre().agora(|c| c.item(id).cloned()) else {
            return Ok(());
        };
        let Some(chave) = item.chave.as_ref() else {
            return Ok(());
        };
        let presente = self
            .agente
            .chaves()
            .into_iter()
            .find(|k| keys::fingerprint(&k.public) == chave.fingerprint);
        if let Some(presente) = presente.as_ref() {
            // Põe de novo, com a política de agora.
            self.agente.remover(presente);
        }
        if !chave.no_agente || chave.e_token() || chave.privada.trim().is_empty() {
            return Ok(());
        }
        if !self.agente.rodando() {
            return Ok(());
        }
        let Some(privada) = chave.chave_privada() else {
            return Err(t!(
                "chave-nao-consegui-abrir-a-chave",
                titulo = item.titulo.as_str()
            ));
        };
        self.agente
            .guardar(
                privada,
                &item.titulo,
                &ChaveSalva {
                    confirmar: chave.confirmar,
                    vida_secs: chave.vida_secs,
                    ..ChaveSalva::default()
                },
            )
            .map_err(|e| {
                t!(
                    "chave-nao-consegui-por-no-agente",
                    titulo = item.titulo.as_str(),
                    erro = e.to_string()
                )
            })
    }

    /// Liga ou desliga "No agente" da chave do item e acerta o agente.
    pub fn definir_no_agente(&self, id: u64, ligado: bool) -> Result<(), String> {
        let Some(mut item) = self.cofre().agora(|c| c.item(id).cloned()) else {
            return Err(t!("chave-nao-esta-mais-no-cofre"));
        };
        if let Some(chave) = item.chave.as_mut() {
            if ligado && chave.e_token() {
                return Err(t!("chave-de-token"));
            }
            if ligado && chave.privada.trim().is_empty() {
                return Err(t!("chave-so-parte-publica"));
            }
            chave.no_agente = ligado;
        }
        self.cofre()
            .mudar(|c| c.salvar(item))
            .map_err(erro_de_gravacao)?;
        self.sincronizar_com_o_agente(id)
    }

    /// Tira a chave do agente e apaga o item do cofre.
    pub fn remover_do_cofre(&self, id: u64) -> Result<(), String> {
        if let Some(chave) = self
            .cofre()
            .agora(|c| c.item(id).and_then(|i| i.chave.clone()))
            && let Some(no_agente) = self
                .agente
                .chaves()
                .into_iter()
                .find(|k| keys::fingerprint(&k.public) == chave.fingerprint)
        {
            self.agente.remover(&no_agente);
        }
        self.cofre()
            .mudar(|c| c.remover(id))
            .map_err(erro_de_gravacao)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::cofre::Cofre;
    use keys::{KeyGen, KeyType};

    fn cofre_novo(nome: &str) -> (PastaTemporaria, CofreCompartilhado) {
        let pasta = PastaTemporaria::nova(nome);
        let (cofre, _) = Cofre::abrir(&pasta.0);
        (pasta, CofreCompartilhado::new(cofre))
    }

    fn ed25519() -> keys::GeneratedKey {
        KeyGen::new(KeyType::Ed25519)
            .comment("ana@notebook")
            .generate()
            .unwrap()
    }

    /// Um certificado de usuário da chave, assinado por uma CA nova (a linha `-cert.pub`).
    fn certificar(chave: &keys::GeneratedKey) -> String {
        let mut construtor = ssh_key::certificate::Builder::new_with_random_nonce(
            &mut rand::rng(),
            chave.public_key().clone(),
            0,
            u64::MAX,
        )
        .unwrap();
        construtor.serial(1).unwrap();
        construtor.key_id("ana").unwrap();
        construtor.valid_principal("ana").unwrap();
        construtor
            .sign(ed25519().private_key())
            .unwrap()
            .to_openssh()
            .unwrap()
    }

    fn ppk_v3(chave: &keys::GeneratedKey, passphrase: &str) -> String {
        keys::export_private(
            chave.private_key(),
            PrivateFormat::PpkV3,
            Some(passphrase),
            KdfLevel::Standard,
        )
        .unwrap()
        .to_string()
    }

    #[test]
    fn descreve_o_formato_detectado() {
        let chave = ed25519();
        let lida = Lida::ler_texto(&ppk_v3(&chave, "frase")).unwrap();
        assert_eq!(
            lida.descricao(),
            "PuTTY (.ppk) versão 3, Ed25519, com passphrase"
        );
        assert!(lida.protegida());
        assert_eq!(lida.comentario(), "ana@notebook");
        let aberta = Lida::ler_texto(&chave.private_openssh()).unwrap();
        assert_eq!(aberta.descricao(), "OpenSSH, Ed25519, sem passphrase");
        assert_eq!(aberta.fingerprint(), Some(chave.fingerprint()));
    }

    #[test]
    fn uma_privada_cifrada_entra_no_cofre_sem_passphrase() {
        let (_pasta, cofre) = cofre_novo("cc-importar");
        let chave = ed25519();
        let lida = Lida::ler_texto(&ppk_v3(&chave, "frase")).unwrap();
        assert_eq!(
            importar_para_cofre(&cofre, &lida, "github", "errada", true),
            Err("Passphrase incorreta.".to_owned())
        );
        let id = importar_para_cofre(&cofre, &lida, "github", "frase", true).unwrap();
        let item = cofre.agora(|c| c.item(id).cloned()).unwrap();
        assert_eq!(item.titulo, "github");
        assert_eq!(item.senha, "frase");
        let guardada = item.chave.unwrap();
        assert_eq!(guardada.fingerprint, chave.fingerprint());
        // A privada do cofre abre sem passphrase.
        assert!(KeyFile::from_text(&guardada.privada, None).is_ok());
        // A mesma chave não entra duas vezes.
        let de_novo = importar_para_cofre(&cofre, &lida, "outra", "frase", true);
        assert_eq!(
            de_novo,
            Err("Esta chave já está no cofre, como «github».".to_owned())
        );
    }

    #[test]
    fn sem_guardar_a_passphrase_a_senha_do_item_fica_vazia() {
        let (_pasta, cofre) = cofre_novo("cc-sem-senha");
        let chave = ed25519();
        let lida = Lida::ler_texto(&ppk_v3(&chave, "frase")).unwrap();
        let id = importar_para_cofre(&cofre, &lida, "gh", "frase", false).unwrap();
        assert_eq!(cofre.agora(|c| c.item(id).unwrap().senha.clone()), "");
    }

    #[test]
    fn a_publica_vira_item_sem_privada_e_depois_ganha_a_privada() {
        let (_pasta, cofre) = cofre_novo("cc-publica");
        let chave = ed25519();
        let publica = Lida::ler_texto(&chave.public_openssh()).unwrap();
        assert!(matches!(publica, Lida::Publica { .. }));
        let id = importar_para_cofre(&cofre, &publica, "colega", "", false).unwrap();
        let item = cofre.agora(|c| c.item(id).cloned()).unwrap();
        let guardada = item.chave.unwrap();
        assert!(guardada.privada.is_empty());
        assert!(!guardada.no_agente);
        let privada = Lida::ler_texto(&chave.private_openssh()).unwrap();
        let mesmo = importar_para_cofre(&cofre, &privada, "novo", "", false).unwrap();
        assert_eq!(mesmo, id);
        let item = cofre.agora(|c| c.item(id).cloned()).unwrap();
        assert_eq!(item.titulo, "colega");
        assert!(!item.chave.unwrap().privada.is_empty());
    }

    #[test]
    fn o_cofre_bloqueado_recusa_a_importacao() {
        let pasta = PastaTemporaria::nova("cc-bloqueado");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.definir_senha_mestra(Some("mestra")).unwrap();
        cofre.bloquear();
        let cofre = CofreCompartilhado::new(cofre);
        let lida = Lida::ler_texto(&ed25519().private_openssh()).unwrap();
        assert!(
            importar_para_cofre(&cofre, &lida, "x", "", false)
                .unwrap_err()
                .contains("bloqueado")
        );
    }

    #[test]
    fn exporta_para_ppk_v2_e_abre_de_volta() {
        let chave = ed25519();
        let item = {
            let (_p, cofre) = cofre_novo("cc-exportar");
            let lida = Lida::ler_texto(&chave.private_openssh()).unwrap();
            let id = importar_para_cofre(&cofre, &lida, "meu servidor", "", false).unwrap();
            cofre.agora(|c| c.item(id).cloned()).unwrap()
        };
        let aberta = Aberta::do_item(&item).unwrap();
        assert_eq!(aberta.nome, "meu_servidor");
        assert!(aberta.formatos_privados().contains(&PrivateFormat::PpkV2));
        assert!(
            !aberta
                .formatos_privados()
                .contains(&PrivateFormat::PemLegacy)
        );
        let texto = aberta
            .exportar(
                Parte::Privada,
                PrivateFormat::PpkV2,
                PublicFormat::OpenSsh,
                "nova",
                "nova",
            )
            .unwrap();
        assert!(texto.starts_with("PuTTY-User-Key-File-2"));
        let volta = Aberta::do_texto("x", &texto, "nova").unwrap();
        assert_eq!(keys::fingerprint(&volta.publica), chave.fingerprint());
        assert_eq!(
            aberta.arquivo_sugerido(Parte::Privada, PrivateFormat::PpkV2, PublicFormat::OpenSsh),
            "meu_servidor.ppk"
        );
        // Sem passphrase, sem confirmação.
        assert!(
            aberta
                .exportar(
                    Parte::Privada,
                    PrivateFormat::OpenSsh,
                    PublicFormat::OpenSsh,
                    "",
                    ""
                )
                .is_ok()
        );
        assert_eq!(
            aberta.exportar(
                Parte::Privada,
                PrivateFormat::OpenSsh,
                PublicFormat::OpenSsh,
                "a",
                "b"
            ),
            Err(t!("chave-passphrases-nao-conferem"))
        );
    }

    #[test]
    fn exportar_a_publica_da_chave_so_publica() {
        let (_p, cofre) = cofre_novo("cc-so-publica");
        let chave = ed25519();
        let lida = Lida::ler_texto(&chave.public_openssh()).unwrap();
        let id = importar_para_cofre(&cofre, &lida, "colega", "", false).unwrap();
        let item = cofre.agora(|c| c.item(id).cloned()).unwrap();
        let aberta = Aberta::do_item(&item).unwrap();
        assert!(aberta.formatos_privados().is_empty());
        assert!(
            aberta
                .exportar(
                    Parte::Privada,
                    PrivateFormat::OpenSsh,
                    PublicFormat::OpenSsh,
                    "",
                    ""
                )
                .is_err()
        );
        let rfc = aberta
            .exportar(
                Parte::Publica,
                PrivateFormat::OpenSsh,
                PublicFormat::Rfc4716,
                "",
                "",
            )
            .unwrap();
        assert!(rfc.starts_with("---- BEGIN SSH2 PUBLIC KEY ----"));
    }

    #[test]
    fn importar_para_arquivo_nao_sobrescreve_e_grava_o_pub() {
        let pasta = PastaTemporaria::nova("cc-arquivo");
        let chave = ed25519();
        let lida = Lida::ler_texto(&ppk_v3(&chave, "frase")).unwrap();
        let destino = importar_para_arquivo(&pasta.0, &lida, "minha chave", "frase").unwrap();
        assert_eq!(destino, pasta.0.join("minha_chave"));
        let info = KeyFile::inspect(&destino).unwrap();
        assert_eq!(info.format, KeyFormat::OpenSsh);
        assert!(info.encrypted, "a passphrase original é mantida");
        assert!(pasta.0.join("minha_chave.pub").exists());
        let de_novo = importar_para_arquivo(&pasta.0, &lida, "minha chave", "frase");
        assert!(de_novo.unwrap_err().starts_with("Já existe"));
    }

    #[test]
    fn o_certificado_ao_lado_acompanha_a_privada() {
        let pasta = PastaTemporaria::nova("cc-cert");
        let chave = ed25519();
        let arquivo = pasta.0.join("id_cert");
        keys::write_key_text(&arquivo, &chave.private_openssh(), true, false).unwrap();
        let linha = certificar(&chave);
        fs::write(pasta.0.join("id_cert-cert.pub"), format!("{linha}\n")).unwrap();
        let lida = Lida::ler_arquivo(&arquivo).unwrap();
        let Lida::Privada { certificado, .. } = &lida else {
            panic!("devia ser uma privada");
        };
        assert!(certificado.contains("-cert-v01@openssh.com"));
        let (_p, cofre) = cofre_novo("cc-cert-cofre");
        let id = importar_para_cofre(&cofre, &lida, "com certificado", "", false).unwrap();
        let guardada = cofre.agora(|c| c.item(id).unwrap().chave.clone().unwrap());
        assert!(guardada.certificado.contains("-cert-v01@openssh.com"));
        // Exportar grava o -cert.pub junto.
        let aberta = Aberta::do_item(&cofre.agora(|c| c.item(id).cloned()).unwrap()).unwrap();
        let saida = pasta.0.join("saida");
        aberta.gravar(&saida, Parte::Privada, "texto").unwrap();
        assert!(pasta.0.join("saida-cert.pub").exists());
    }

    #[test]
    fn um_certificado_sozinho_liga_ao_item_da_mesma_chave_ou_avisa() {
        let (_p, cofre) = cofre_novo("cc-cert-so");
        let chave = ed25519();
        let linha = certificar(&chave);
        let cert = Lida::ler_texto(&linha).unwrap();
        assert_eq!(cert.descricao(), "Certificado OpenSSH");
        assert!(
            importar_para_cofre(&cofre, &cert, "x", "", false)
                .unwrap_err()
                .contains("Importe primeiro a privada")
        );
        let privada = Lida::ler_texto(&chave.private_openssh()).unwrap();
        let id = importar_para_cofre(&cofre, &privada, "chave", "", false).unwrap();
        assert_eq!(importar_para_cofre(&cofre, &cert, "x", "", false), Ok(id));
        assert!(
            cofre
                .agora(|c| c.item(id).unwrap().chave.clone().unwrap())
                .certificado
                .contains("-cert-v01@openssh.com")
        );
    }

    #[test]
    fn texto_que_nao_e_chave_e_recusado_com_motivo() {
        assert!(Lida::ler_texto("   ").is_err());
        assert!(
            Lida::ler_texto("isto não é uma chave")
                .unwrap_err()
                .contains("não parece")
        );
    }

    #[test]
    fn nomes_de_arquivo_seguros() {
        assert_eq!(nome_de_arquivo("meu servidor/prod"), "meu_servidor_prod");
        assert_eq!(nome_de_arquivo("..."), "chave");
        assert_eq!(nome_de_arquivo("ação-é"), "ação-é");
    }
}
