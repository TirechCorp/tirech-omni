//! O JSON do Tirech Omni: o cofre inteiro (ou parte dele) com as pastas, em claro, e a escolha do
//! formato de um arquivo importado (JSON do Tirech Omni, JSON do Bitwarden, ou CSV).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::anexos::{self, ErroAnexo, Progresso};
use crate::bitwarden;
use crate::cofre::{Anexo, Cofre, Lote};
use crate::csv;
use crate::notas;
use crate::t;

/// O valor do campo `formato` do documento.
///
/// Identificador de formato: não muda com o nome do app (está gravado nos arquivos de quem usa).
pub const FORMATO: &str = "tirech-ssh-cofre";
/// A versão do documento que este app escreve e lê.
pub const VERSAO: u32 = 1;

/// O documento: o cabeçalho e o mesmo `Lote` do cofre (pastas e itens).
#[derive(Debug, Serialize, Deserialize)]
pub struct Documento {
    /// Sempre [`FORMATO`].
    pub formato: String,
    /// A versão do documento.
    pub versao: u32,
    /// As pastas e os itens.
    #[serde(flatten)]
    pub lote: Lote,
}

impl Documento {
    /// O cofre inteiro.
    pub fn tudo(cofre: &Cofre) -> Self {
        Self {
            formato: FORMATO.to_owned(),
            versao: VERSAO,
            lote: Lote {
                pastas: cofre.pastas().to_vec(),
                itens: cofre.itens().to_vec(),
            },
        }
    }

    /// Só os itens `ids` e as pastas acima deles.
    pub fn parcial(cofre: &Cofre, ids: &[u64]) -> Self {
        let itens: Vec<_> = cofre
            .itens()
            .iter()
            .filter(|i| ids.contains(&i.id))
            .cloned()
            .collect();
        let mut usadas: HashSet<u64> = HashSet::new();
        for item in &itens {
            let mut atual = item.pasta;
            // O limite protege de um ciclo que o cofre não deixaria existir.
            for _ in 0..=cofre.pastas().len() {
                let Some(id) = atual else { break };
                if !usadas.insert(id) {
                    break;
                }
                atual = cofre.pasta(id).and_then(|p| p.pai);
            }
        }
        Self {
            formato: FORMATO.to_owned(),
            versao: VERSAO,
            lote: Lote {
                pastas: cofre
                    .pastas()
                    .iter()
                    .filter(|p| usadas.contains(&p.id))
                    .cloned()
                    .collect(),
                itens,
            },
        }
    }
}

/// Um arquivo de anexo que a exportação grava, em claro, ao lado do JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArquivoDeAnexo {
    /// A ficha do anexo no cofre (com a chave, que o JSON não leva).
    pub ficha: Anexo,
    /// O caminho no JSON e ao lado dele: `<nome>-attachments/<id>/<nome do anexo>`, com `/`.
    pub relativo: String,
}

/// O JSON do cofre e os arquivos que vão junto (M47): o JSON nunca leva os bytes de um anexo nem a
/// chave dele, só a ficha e o `arquivo`, o caminho relativo ao JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exportacao {
    /// O texto do JSON.
    pub texto: String,
    /// Os arquivos a gravar ao lado dele (sem repetir o mesmo `relativo`).
    pub arquivos: Vec<ArquivoDeAnexo>,
    /// Quantas fichas ficaram de fora porque o arquivo não está nesta máquina.
    pub ausentes: usize,
}

impl Exportacao {
    /// Os bytes que [`gravar_arquivos`] vai escrever.
    pub fn bytes(&self) -> u64 {
        self.arquivos.iter().map(|a| a.ficha.tamanho).sum()
    }
}

/// O nome da pasta dos anexos de um JSON `<nome>.json`: `<nome>-attachments`.
pub fn pasta_dos_anexos(json: &Path) -> String {
    let base = json
        .file_stem()
        .map_or_else(|| "cofre".to_owned(), |n| n.to_string_lossy().into_owned());
    format!("{base}-attachments")
}

impl Documento {
    /// O documento como JSON, com as fichas dos anexos no formato de troca: `nome`, `tamanho`,
    /// `tipo`, `sha256`, `criado` e `arquivo` (`<pasta>/<id>/<nome>`), sem `id` nem `chave`.
    /// Fichas cujo arquivo não está em `vault` ficam de fora e entram na contagem. É o que o
    /// bloco `cofre` do "tudo" (M38) também usa.
    pub fn com_anexos(&self, pasta: &str, vault: &Path) -> Exportacao {
        let mut valor = serde_json::to_value(self).unwrap_or(Value::Null);
        let mut arquivos: Vec<ArquivoDeAnexo> = Vec::new();
        let mut vistos: HashSet<String> = HashSet::new();
        let mut ausentes = 0;
        if let Some(itens) = valor.get_mut("itens").and_then(Value::as_array_mut) {
            for (json, item) in itens.iter_mut().zip(&self.lote.itens) {
                if item.anexos.is_empty() {
                    continue;
                }
                let mut fichas = Vec::new();
                for ficha in &item.anexos {
                    if !anexos::existe(vault, &ficha.id) {
                        ausentes += 1;
                        continue;
                    }
                    let relativo = format!(
                        "{pasta}/{}/{}",
                        ficha.id,
                        notas::nome_de_arquivo(&ficha.nome)
                    );
                    fichas.push(serde_json::json!({
                        "nome": ficha.nome,
                        "tamanho": ficha.tamanho,
                        "tipo": ficha.tipo,
                        "sha256": ficha.sha256,
                        "criado": ficha.criado,
                        "arquivo": relativo,
                    }));
                    if vistos.insert(relativo.clone()) {
                        arquivos.push(ArquivoDeAnexo {
                            ficha: ficha.clone(),
                            relativo,
                        });
                    }
                }
                if let Some(objeto) = json.as_object_mut() {
                    if fichas.is_empty() {
                        objeto.remove("anexos");
                    } else {
                        objeto.insert("anexos".to_owned(), Value::Array(fichas));
                    }
                }
            }
        }
        Exportacao {
            texto: serde_json::to_string_pretty(&valor).unwrap_or_default(),
            arquivos,
            ausentes,
        }
    }
}

/// O cofre inteiro como JSON, mais os arquivos dos anexos; `json` é o arquivo que vai receber o
/// texto (o nome dele dá o da pasta dos anexos).
pub fn exportar(cofre: &Cofre, json: &Path) -> Exportacao {
    let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
    Documento::tudo(cofre).com_anexos(&pasta_dos_anexos(json), &vault)
}

/// Grava os arquivos de `exportacao` em claro ao lado do `json`, em fluxo, com o espaço conferido
/// antes. Cancelar ou falhar apaga o que foi gravado. Devolve quantos arquivos foram gravados.
/// Um arquivo que sumiu ou está corrompido é pulado e não conta.
pub fn gravar_arquivos(
    arquivos: &[ArquivoDeAnexo],
    vault: &Path,
    json: &Path,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<usize, ErroAnexo> {
    let pasta = json.parent().filter(|p| !p.as_os_str().is_empty());
    let pasta = pasta.unwrap_or_else(|| Path::new("."));
    let total: u64 = arquivos.iter().map(|a| a.ficha.tamanho).sum();
    anexos::conferir_espaco_em(pasta, total)?;
    let mut criados: Vec<PathBuf> = Vec::new();
    let mut feito = 0;
    let mut gravados = 0;
    let resultado = (|| -> Result<(), ErroAnexo> {
        for arquivo in arquivos {
            if cancelado.load(Ordering::Relaxed) {
                return Err(ErroAnexo::Cancelado);
            }
            let destino = pasta.join(&arquivo.relativo);
            if let Some(dir) = destino.parent() {
                let novas: Vec<PathBuf> = dir
                    .ancestors()
                    .take_while(|a| *a != pasta && !a.exists())
                    .map(Path::to_path_buf)
                    .collect();
                fs::create_dir_all(dir)?;
                criados.extend(novas.into_iter().rev());
            }
            let base = feito;
            let parcial = |p: Progresso| {
                progresso(Progresso {
                    feito: base + p.feito,
                    total,
                })
            };
            match anexos::ler_para_arquivo(vault, &arquivo.ficha, &destino, &parcial, cancelado) {
                Ok(()) => {
                    criados.push(destino);
                    gravados += 1;
                }
                Err(ErroAnexo::Ausente | ErroAnexo::Corrompido) => {}
                Err(e) => return Err(e),
            }
            feito += arquivo.ficha.tamanho;
            progresso(Progresso { feito, total });
        }
        Ok(())
    })();
    if let Err(erro) = resultado {
        for caminho in criados.iter().rev() {
            if caminho.is_dir() {
                let _ = fs::remove_dir(caminho);
            } else {
                let _ = fs::remove_file(caminho);
            }
        }
        return Err(erro);
    }
    Ok(gravados)
}

/// O documento dentro do valor: ele mesmo, ou o campo `cofre` (o JSON "tudo" do M38).
fn documento_de(valor: &Value) -> Option<&Value> {
    let eh_cofre = |v: &Value| v.get("formato").and_then(Value::as_str) == Some(FORMATO);
    if eh_cofre(valor) {
        Some(valor)
    } else {
        valor.get("cofre").filter(|v| eh_cofre(v))
    }
}

/// Nas fichas de anexo do JSON, o `arquivo` (o caminho relativo ao JSON) passa para o `id` e a
/// `chave` sai: o que vem de fora nunca decide o nome nem a chave de um arquivo do `vault/`.
/// [`trazer_anexos`] troca esse `id` pelo de um arquivo novo.
fn preparar_anexos(documento: &mut Value) {
    let Some(itens) = documento.get_mut("itens").and_then(Value::as_array_mut) else {
        return;
    };
    for item in itens {
        let Some(fichas) = item.get_mut("anexos").and_then(Value::as_array_mut) else {
            continue;
        };
        for ficha in fichas {
            let Some(objeto) = ficha.as_object_mut() else {
                continue;
            };
            let arquivo = objeto.remove("arquivo").unwrap_or(Value::Null);
            objeto.remove("chave");
            objeto.insert(
                "id".to_owned(),
                if arquivo.is_string() {
                    arquivo
                } else {
                    Value::String(String::new())
                },
            );
        }
    }
}

/// Lê o documento: aceita o objeto sozinho ou um objeto com um campo `cofre` que o contém.
pub fn ler(texto: &str) -> Result<Lote, String> {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    let valor: Value = serde_json::from_str(texto)
        .map_err(|e| t!("cofre-json-nao-consegui-ler-tirech", erro = e.to_string()))?;
    let Some(documento) = documento_de(&valor) else {
        return Err(t!("cofre-json-nao-e-cofre"));
    };
    let mut documento = documento.clone();
    preparar_anexos(&mut documento);
    let documento: Documento = serde_json::from_value(documento)
        .map_err(|e| t!("cofre-json-nao-consegui-ler-tirech", erro = e.to_string()))?;
    if documento.versao > VERSAO {
        return Err(t!("cofre-json-versao-nova"));
    }
    Ok(documento.lote)
}

/// O que [`trazer_anexos`] fez.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trazidos {
    /// Quantas fichas ganharam um arquivo cifrado em `vault/`.
    pub trazidos: usize,
    /// Quantas fichas ficaram de fora: o arquivo não está ao lado do JSON, não é o que a ficha
    /// diz (o SHA-256 não bate) ou o caminho não é seguro.
    pub nao_encontrados: usize,
}

/// O caminho de uma ficha do JSON dentro da pasta do JSON, se ele fica lá dentro (só nomes
/// normais: nada de `..`, de raiz ou de letra de unidade).
fn dentro_da_pasta(pasta: &Path, relativo: &str) -> Option<PathBuf> {
    let caminho = Path::new(relativo);
    let seguro = !relativo.is_empty()
        && caminho
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    seguro.then(|| pasta.join(caminho))
}

/// Cifra para `vault/` os arquivos que as fichas do `lote` citam (o `id` de cada ficha vem do
/// [`ler`] como o caminho relativo ao JSON, que mora em `pasta`), com id e chave novos, e troca
/// `id` e `chave` pelos do arquivo novo. Fichas sem arquivo, ou com o arquivo diferente do que o
/// JSON diz (SHA-256), saem do item e entram na contagem. Cancelar ou falhar apaga o que entrou.
pub fn trazer_anexos(
    lote: &mut Lote,
    pasta: &Path,
    vault: &Path,
    progresso: &dyn Fn(Progresso),
    cancelado: &AtomicBool,
) -> Result<Trazidos, ErroAnexo> {
    let total: u64 = lote
        .itens
        .iter()
        .flat_map(|i| &i.anexos)
        .filter_map(|a| dentro_da_pasta(pasta, &a.id))
        .filter_map(|c| fs::metadata(c).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    let mut resumo = Trazidos::default();
    // O arquivo já cifrado de cada caminho: fichas do mesmo arquivo dividem o mesmo `id`.
    let mut feitos: HashMap<String, Option<Anexo>> = HashMap::new();
    let mut novos: Vec<String> = Vec::new();
    let mut feito = 0;
    let resultado = (|| -> Result<(), ErroAnexo> {
        for item in &mut lote.itens {
            let fichas = std::mem::take(&mut item.anexos);
            for ficha in fichas {
                if cancelado.load(Ordering::Relaxed) {
                    return Err(ErroAnexo::Cancelado);
                }
                let relativo = ficha.id.clone();
                if !feitos.contains_key(&relativo) {
                    let base = feito;
                    let parcial = |p: Progresso| {
                        progresso(Progresso {
                            feito: base + p.feito,
                            total,
                        });
                    };
                    let nova = match dentro_da_pasta(pasta, &relativo) {
                        Some(origem) if origem.is_file() => {
                            let id = anexos::novo_id();
                            let nome =
                                (!ficha.nome.trim().is_empty()).then_some(ficha.nome.as_str());
                            let r = notas::anexar_arquivo_com_id(
                                vault, &id, &origem, nome, &parcial, cancelado,
                            );
                            match r {
                                Ok(nova) => {
                                    novos.push(id);
                                    feito += nova.tamanho;
                                    if ficha.sha256.is_empty() || ficha.sha256 == nova.sha256 {
                                        Some(nova)
                                    } else {
                                        let _ = fs::remove_file(anexos::caminho(vault, &nova.id));
                                        None
                                    }
                                }
                                Err(ErroAnexo::Io(e))
                                    if e.kind() == std::io::ErrorKind::NotFound =>
                                {
                                    None
                                }
                                Err(e) => return Err(e),
                            }
                        }
                        _ => None,
                    };
                    feitos.insert(relativo.clone(), nova);
                }
                match &feitos[&relativo] {
                    Some(nova) => {
                        item.anexos.push(Anexo {
                            id: nova.id.clone(),
                            chave: nova.chave.clone(),
                            sha256: nova.sha256.clone(),
                            tamanho: nova.tamanho,
                            nome: if ficha.nome.trim().is_empty() {
                                nova.nome.clone()
                            } else {
                                ficha.nome
                            },
                            tipo: if ficha.tipo.is_empty() {
                                nova.tipo.clone()
                            } else {
                                ficha.tipo
                            },
                            criado: if ficha.criado == 0 {
                                nova.criado
                            } else {
                                ficha.criado
                            },
                        });
                        resumo.trazidos += 1;
                    }
                    None => resumo.nao_encontrados += 1,
                }
            }
        }
        Ok(())
    })();
    if let Err(erro) = resultado {
        descartar(vault, &novos);
        return Err(erro);
    }
    Ok(resumo)
}

/// Apaga de `vault/` os arquivos cifrados `ids` (a importação que falhou depois de trazê-los).
pub fn descartar(vault: &Path, ids: &[String]) {
    for id in ids {
        let _ = fs::remove_file(anexos::caminho(vault, id));
    }
}

/// Os `id`s dos anexos que as fichas do `lote` têm agora (depois de [`trazer_anexos`]).
pub fn ids_dos_anexos(lote: &Lote) -> Vec<String> {
    let mut ids: Vec<String> = lote
        .itens
        .iter()
        .flat_map(|i| i.anexos.iter().map(|a| a.id.clone()))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// Descobre qual formato é o arquivo e lê: o lote, o nome do formato (para o aviso) e os avisos.
pub fn importar_arquivo(texto: &str) -> Result<(Lote, String, Vec<String>), String> {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    if texto.trim_start().starts_with('{') {
        let valor: Value = serde_json::from_str(texto)
            .map_err(|e| t!("cofre-json-nao-consegui-ler", erro = e.to_string()))?;
        if documento_de(&valor).is_some() {
            return ler(texto).map(|lote| (lote, t!("formato-json-tirech"), Vec::new()));
        }
        if valor.get("items").is_some() || valor.get("encrypted").is_some() {
            return bitwarden::importar(texto)
                .map(|(lote, avisos)| (lote, t!("formato-json-bitwarden"), avisos));
        }
        return Err(t!("cofre-json-formato-desconhecido"));
    }
    csv::importar_com_formato(texto).map(|(lote, nome)| (lote, nome, Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::cofre::{Campo, Item, Tipo};

    /// Um cofre com Clientes › ACME (um SSH), Banco (uma senha) e uma nota na raiz.
    fn cofre_de_exemplo(pasta: &PastaTemporaria) -> (Cofre, [u64; 3]) {
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let acme = cofre.garantir_caminho(None, &["Clientes", "ACME"]);
        let banco = cofre.garantir_caminho(None, &["Banco"]);
        let web = cofre
            .salvar(Item {
                tipo: Tipo::Ssh,
                titulo: "web-01".into(),
                pasta: acme,
                usuario: "root".into(),
                senha: "pw".into(),
                url: "web-01.acme:22".into(),
                campos: vec![Campo {
                    nome: "região".into(),
                    valor: "sa".into(),
                    oculto: true,
                }],
                ..Item::default()
            })
            .unwrap();
        let conta = cofre
            .salvar(Item {
                titulo: "conta".into(),
                pasta: banco,
                ..Item::default()
            })
            .unwrap();
        let nota = cofre
            .salvar(Item {
                tipo: Tipo::Nota,
                titulo: "lembrete".into(),
                notas: "texto".into(),
                ..Item::default()
            })
            .unwrap();
        (cofre, [web, conta, nota])
    }

    #[test]
    fn ida_e_volta_devolve_pastas_e_itens() {
        let pasta = PastaTemporaria::nova("json-ida");
        let (cofre, _) = cofre_de_exemplo(&pasta);
        let texto = exportar(&cofre, Path::new("cofre.json")).texto;
        assert!(texto.contains("\"formato\": \"tirech-ssh-cofre\""));
        let lote = ler(&texto).unwrap();
        assert_eq!(lote.pastas, cofre.pastas());
        assert_eq!(lote.itens, cofre.itens());
    }

    #[test]
    fn parcial_leva_as_pastas_de_cima_e_so_elas() {
        let pasta = PastaTemporaria::nova("json-parcial");
        let (cofre, [web, _, nota]) = cofre_de_exemplo(&pasta);
        let doc = Documento::parcial(&cofre, &[web, nota]);
        let nomes: Vec<&str> = doc.lote.pastas.iter().map(|p| p.nome.as_str()).collect();
        assert_eq!(nomes, ["Clientes", "ACME"]);
        assert_eq!(doc.lote.itens.len(), 2);
        let sem = Documento::parcial(&cofre, &[nota]);
        assert!(sem.lote.pastas.is_empty());
    }

    #[test]
    fn aceita_o_documento_dentro_do_campo_cofre() {
        let pasta = PastaTemporaria::nova("json-campo");
        let (cofre, _) = cofre_de_exemplo(&pasta);
        let dentro = serde_json::json!({
            "conexoes": [],
            "cofre": serde_json::to_value(Documento::tudo(&cofre)).unwrap(),
        });
        let lote = ler(&dentro.to_string()).unwrap();
        assert_eq!(lote.itens.len(), 3);
        assert_eq!(lote.pastas.len(), 3);
    }

    #[test]
    fn recusa_outro_formato_e_versao_mais_nova() {
        assert_eq!(
            ler(r#"{"formato":"outro","versao":1}"#).unwrap_err(),
            "Esse JSON não é um cofre do Tirech Omni."
        );
        assert_eq!(
            ler("[]").unwrap_err(),
            "Esse JSON não é um cofre do Tirech Omni."
        );
        assert_eq!(
            ler(r#"{"formato":"tirech-ssh-cofre","versao":2,"pastas":[],"itens":[]}"#).unwrap_err(),
            "Esse cofre foi exportado por uma versão mais nova do Tirech Omni."
        );
        assert!(ler("{ não é json").is_err());
    }

    #[test]
    fn importar_arquivo_acha_cada_formato() {
        let pasta = PastaTemporaria::nova("json-formatos");
        let (cofre, _) = cofre_de_exemplo(&pasta);

        let (lote, nome, avisos) =
            importar_arquivo(&exportar(&cofre, Path::new("cofre.json")).texto).unwrap();
        assert_eq!(
            (nome.as_str(), lote.itens.len(), avisos.len()),
            ("JSON do Tirech Omni", 3, 0)
        );

        let (lote, nome, _) = importar_arquivo(&bitwarden::exportar(&cofre)).unwrap();
        assert_eq!((nome.as_str(), lote.itens.len()), ("JSON do Bitwarden", 3));

        let csv_bw = "folder,type,name,login_username\nA,login,um,ana\n";
        let (lote, nome, _) = importar_arquivo(csv_bw).unwrap();
        assert_eq!((nome.as_str(), lote.itens.len()), ("CSV do Bitwarden", 1));

        let csv_kp =
            "\"Group\",\"Title\",\"Username\",\"Password\"\n\"Root/A\",\"x\",\"u\",\"p\"\n";
        let (lote, nome, _) = importar_arquivo(csv_kp).unwrap();
        assert_eq!((nome.as_str(), lote.itens.len()), ("CSV do KeePassXC", 1));
    }

    #[test]
    fn importar_arquivo_ignora_o_bom_e_recusa_json_desconhecido() {
        let com_bom = "\u{feff}{\"encrypted\":false,\"folders\":[],\"items\":[]}";
        assert_eq!(importar_arquivo(com_bom).unwrap().1, "JSON do Bitwarden");
        let com_bom_csv = "\u{feff}folder,type,name,login_username\n,login,um,ana\n";
        assert_eq!(importar_arquivo(com_bom_csv).unwrap().1, "CSV do Bitwarden");
        assert_eq!(
            importar_arquivo(r#"{"qualquer":"coisa"}"#).unwrap_err(),
            "Formato de JSON desconhecido: esperava o do Tirech Omni ou o do Bitwarden."
        );
    }

    #[test]
    fn json_cifrado_do_bitwarden_vira_erro() {
        let erro = importar_arquivo(r#"{"encrypted":true,"data":"x"}"#).unwrap_err();
        assert!(erro.contains("está cifrado"), "{erro}");
    }

    fn nao() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// Anexa `bytes` à nota `nota` e devolve a ficha.
    fn anexar(cofre: &mut Cofre, nota: u64, nome: &str, bytes: &[u8]) -> Anexo {
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        let ficha =
            notas::anexar_bytes(&vault, &anexos::novo_id(), nome, bytes, &|_| {}, &nao()).unwrap();
        cofre.anexar(nota, ficha.clone()).unwrap();
        ficha
    }

    /// O cofre de exemplo com dois anexos na nota `lembrete`, e a pasta onde o JSON vai.
    fn cofre_com_anexos(nome: &str) -> (Cofre, PastaTemporaria, u64, PathBuf) {
        let pasta = PastaTemporaria::nova(nome);
        let (mut cofre, [_, _, nota]) = cofre_de_exemplo(&pasta);
        anexar(&mut cofre, nota, "chave.pem", b"-----BEGIN-----abacaxi");
        anexar(&mut cofre, nota, "foto.png", b"PNG-bytes");
        let saida = pasta.0.join("saida");
        fs::create_dir_all(&saida).unwrap();
        (cofre, pasta, nota, saida)
    }

    #[test]
    fn a_exportacao_leva_a_ficha_sem_id_nem_chave_e_o_arquivo_relativo() {
        let (cofre, _pasta, _, _) = cofre_com_anexos("json-anexos-ficha");
        let ex = exportar(&cofre, Path::new("/tmp/meu.json"));
        assert_eq!(ex.ausentes, 0);
        assert_eq!(ex.arquivos.len(), 2);
        let valor: Value = serde_json::from_str(&ex.texto).unwrap();
        let nota = valor["itens"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["titulo"] == "lembrete")
            .unwrap();
        let fichas = nota["anexos"].as_array().unwrap();
        assert_eq!(fichas.len(), 2);
        let id = &ex.arquivos[0].ficha.id;
        assert_eq!(fichas[0]["nome"], "chave.pem");
        assert_eq!(fichas[0]["tamanho"], 22);
        assert_eq!(
            fichas[0]["arquivo"],
            format!("meu-attachments/{id}/chave.pem")
        );
        assert!(fichas[0].get("id").is_none() && fichas[0].get("chave").is_none());
        assert!(fichas[0]["sha256"].as_str().is_some_and(|h| h.len() == 64));
        // Nem os bytes nem a chave aparecem no texto (o id só aparece dentro do caminho `arquivo`).
        for arquivo in &ex.arquivos {
            assert!(!ex.texto.contains(&arquivo.ficha.chave));
        }
        assert!(!ex.texto.contains("abacaxi"));
    }

    #[test]
    fn ida_e_volta_cifra_os_arquivos_de_novo_com_id_e_chave_novos() {
        let (cofre, pasta, nota, saida) = cofre_com_anexos("json-anexos-ida");
        let json = saida.join("cofre.json");
        let ex = exportar(&cofre, &json);
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        let gravados = gravar_arquivos(&ex.arquivos, &vault, &json, &|_| {}, &nao()).unwrap();
        assert_eq!(gravados, 2);
        let id = &ex.arquivos[0].ficha.id;
        assert_eq!(
            fs::read(saida.join(format!("cofre-attachments/{id}/chave.pem"))).unwrap(),
            b"-----BEGIN-----abacaxi"
        );
        fs::write(&json, &ex.texto).unwrap();

        // Num cofre vazio de outra pasta.
        let destino = PastaTemporaria::nova("json-anexos-volta");
        let (mut outro, _) = Cofre::abrir(&destino.0);
        let (mut lote, _, _) = importar_arquivo(&fs::read_to_string(&json).unwrap()).unwrap();
        let vault_novo = outro.pasta_do_cofre().join(anexos::PASTA);
        let r = trazer_anexos(&mut lote, &saida, &vault_novo, &|_| {}, &nao()).unwrap();
        assert_eq!(
            r,
            Trazidos {
                trazidos: 2,
                nao_encontrados: 0
            }
        );
        outro.importar(lote, None).unwrap();

        let nota_nova = outro
            .itens()
            .iter()
            .find(|i| i.titulo == "lembrete")
            .unwrap();
        let antigas = &cofre.item(nota).unwrap().anexos;
        assert_eq!(nota_nova.anexos.len(), 2);
        for (nova, antiga) in nota_nova.anexos.iter().zip(antigas) {
            assert_ne!(nova.id, antiga.id, "id novo");
            assert_ne!(nova.chave, antiga.chave, "chave nova");
            assert_eq!(
                (&nova.nome, nova.tamanho, &nova.sha256),
                (&antiga.nome, antiga.tamanho, &antiga.sha256)
            );
            assert_eq!(nova.criado, antiga.criado);
        }
        let lido = anexos::ler_para_memoria(&vault_novo, &nota_nova.anexos[0], &nao()).unwrap();
        assert_eq!(lido, b"-----BEGIN-----abacaxi");
        assert!(!pasta.0.as_os_str().is_empty());
    }

    #[test]
    fn dois_anexos_de_mesmo_nome_nao_colidem_ao_lado_do_json() {
        let pasta = PastaTemporaria::nova("json-anexos-mesmo-nome");
        let (mut cofre, [_, _, nota]) = cofre_de_exemplo(&pasta);
        anexar(&mut cofre, nota, "LEIA", b"primeiro");
        anexar(&mut cofre, nota, "LEIA", b"segundo");
        let json = pasta.0.join("saida.json");
        let ex = exportar(&cofre, &json);
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        gravar_arquivos(&ex.arquivos, &vault, &json, &|_| {}, &nao()).unwrap();
        let caminhos: Vec<_> = ex.arquivos.iter().map(|a| a.relativo.clone()).collect();
        assert_eq!(caminhos.len(), 2);
        assert_ne!(caminhos[0], caminhos[1]);
        let conteudos: Vec<_> = caminhos
            .iter()
            .map(|c| fs::read(pasta.0.join(c)).unwrap())
            .collect();
        assert_eq!(conteudos, [b"primeiro".to_vec(), b"segundo".to_vec()]);
    }

    #[test]
    fn anexo_sem_arquivo_no_cofre_fica_fora_da_exportacao_e_e_contado() {
        let (cofre, _pasta, nota, _) = cofre_com_anexos("json-anexos-ausente");
        let ficha = cofre.item(nota).unwrap().anexos[0].clone();
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        fs::remove_file(anexos::caminho(&vault, &ficha.id)).unwrap();
        let ex = exportar(&cofre, Path::new("cofre.json"));
        assert_eq!((ex.arquivos.len(), ex.ausentes), (1, 1));
        let valor: Value = serde_json::from_str(&ex.texto).unwrap();
        let nota = valor["itens"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["titulo"] == "lembrete")
            .unwrap();
        assert_eq!(nota["anexos"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn importar_sem_a_pasta_traz_os_itens_sem_os_anexos_e_conta_os_que_faltam() {
        let (cofre, _pasta, _, saida) = cofre_com_anexos("json-anexos-faltam");
        let ex = exportar(&cofre, &saida.join("cofre.json"));
        let (mut lote, _, _) = importar_arquivo(&ex.texto).unwrap();
        let destino = PastaTemporaria::nova("json-anexos-faltam-destino");
        let vault = destino.0.join(anexos::PASTA);
        let r = trazer_anexos(&mut lote, &saida, &vault, &|_| {}, &nao()).unwrap();
        assert_eq!(
            r,
            Trazidos {
                trazidos: 0,
                nao_encontrados: 2
            }
        );
        assert!(lote.itens.iter().all(|i| i.anexos.is_empty()));
        assert_eq!(lote.itens.len(), 3, "os itens entram mesmo assim");
        assert!(!vault.exists() || fs::read_dir(&vault).unwrap().next().is_none());
    }

    #[test]
    fn importar_nao_confia_no_id_nem_na_chave_do_json_e_confere_o_sha256() {
        let (cofre, _pasta, _, saida) = cofre_com_anexos("json-anexos-sha");
        let json = saida.join("cofre.json");
        let ex = exportar(&cofre, &json);
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        gravar_arquivos(&ex.arquivos, &vault, &json, &|_| {}, &nao()).unwrap();
        // Mexe num arquivo (o SHA-256 deixa de bater), põe um caminho para fora da pasta e um
        // `id` e uma `chave` que não deviam valer.
        let id = &ex.arquivos[0].ficha.id;
        fs::write(
            saida.join(format!("cofre-attachments/{id}/chave.pem")),
            b"trocado",
        )
        .unwrap();
        let mut valor: Value = serde_json::from_str(&ex.texto).unwrap();
        let itens = valor["itens"].as_array_mut().unwrap();
        let nota = itens
            .iter_mut()
            .find(|i| i["titulo"] == "lembrete")
            .unwrap();
        let fichas = nota["anexos"].as_array_mut().unwrap();
        fichas[1]["arquivo"] = "../fora.png".into();
        fichas[1]["id"] = "00000000000000000000000000000000".into();
        fichas[1]["chave"] = "AAAA".into();
        let (mut lote, _, _) = importar_arquivo(&valor.to_string()).unwrap();
        // O que vem de fora nunca vira id nem chave do cofre.
        let fichas = &lote
            .itens
            .iter()
            .find(|i| i.titulo == "lembrete")
            .unwrap()
            .anexos;
        assert!(fichas[0].id.starts_with("cofre-attachments/"));
        assert_eq!(fichas[1].id, "../fora.png");
        assert!(fichas.iter().all(|f| f.chave.is_empty()));
        fs::write(saida.join("../fora.png"), b"PNG-bytes").unwrap();
        let destino = PastaTemporaria::nova("json-anexos-sha-destino");
        let vault_novo = destino.0.join(anexos::PASTA);
        let r = trazer_anexos(&mut lote, &saida, &vault_novo, &|_| {}, &nao()).unwrap();
        assert_eq!(
            r,
            Trazidos {
                trazidos: 0,
                nao_encontrados: 2
            }
        );
        assert!(
            fs::read_dir(&vault_novo).map_or(true, |mut d| d.next().is_none()),
            "o arquivo que não bateu não fica no vault"
        );
    }

    #[test]
    fn cancelar_a_gravacao_dos_arquivos_nao_deixa_nada_ao_lado_do_json() {
        let (cofre, _pasta, _, saida) = cofre_com_anexos("json-anexos-cancela");
        let json = saida.join("cofre.json");
        let ex = exportar(&cofre, &json);
        let vault = cofre.pasta_do_cofre().join(anexos::PASTA);
        let cancelado = AtomicBool::new(true);
        let erro = gravar_arquivos(&ex.arquivos, &vault, &json, &|_| {}, &cancelado).unwrap_err();
        assert!(matches!(erro, ErroAnexo::Cancelado));
        assert_eq!(fs::read_dir(&saida).unwrap().count(), 0);
        // Cancelar no meio: o primeiro arquivo já foi gravado e é apagado.
        let depois = AtomicBool::new(false);
        let erro = gravar_arquivos(
            &ex.arquivos,
            &vault,
            &json,
            &|p| {
                if p.feito > 0 {
                    depois.store(true, Ordering::Relaxed);
                }
            },
            &depois,
        )
        .unwrap_err();
        assert!(matches!(erro, ErroAnexo::Cancelado));
        assert_eq!(fs::read_dir(&saida).unwrap().count(), 0, "tudo desfeito");
    }

    #[test]
    fn o_json_do_bitwarden_exporta_um_item_com_anexos_sem_erro_e_sem_eles() {
        let (cofre, _pasta, _, _) = cofre_com_anexos("json-anexos-bitwarden");
        let texto = bitwarden::exportar(&cofre);
        assert!(!texto.contains("chave.pem"));
        assert!(!texto.contains("abacaxi"));
        let (lote, nome, _) = importar_arquivo(&texto).unwrap();
        assert_eq!((nome.as_str(), lote.itens.len()), ("JSON do Bitwarden", 3));
        assert!(lote.itens.iter().all(|i| i.anexos.is_empty()));
    }

    fn identidade_de_teste() -> crate::cofre::IdentidadeNebula {
        crate::nebula::identidade_de_pem(
            include_str!("../tests/dados/nebula/ca-25519.crt"),
            include_str!("../tests/dados/nebula/no-v2.crt"),
            include_str!("../tests/dados/nebula/no-v2.key"),
        )
        .expect("a fixture vale")
    }

    #[test]
    fn a_identidade_nebula_vai_e_volta_no_json_do_cofre() {
        let pasta = PastaTemporaria::nova("json-nebula");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre
            .salvar(Item {
                tipo: Tipo::Nebula,
                titulo: "Cliente A".into(),
                nebula: Some(identidade_de_teste()),
                ..Item::default()
            })
            .unwrap();
        let texto = exportar(&cofre, Path::new("cofre.json")).texto;
        let lote = ler(&texto).unwrap();
        assert_eq!(lote.itens, cofre.itens());
        assert_eq!(lote.itens[0].nebula, Some(identidade_de_teste()));
    }

    #[test]
    fn a_identidade_openvpn_vai_e_volta_no_json_do_cofre() {
        let agora = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_900_000_000);
        let identidade = crate::openvpn::importar(
            include_str!("../tests/dados/openvpn/tudo-embutido.ovpn"),
            None,
            agora,
        )
        .expect("a fixture vale")
        .identidade;
        let pasta = PastaTemporaria::nova("json-openvpn");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre
            .salvar(Item {
                tipo: Tipo::OpenVpn,
                titulo: "Cliente B".into(),
                usuario: "ana".into(),
                openvpn: Some(identidade.clone()),
                ..Item::default()
            })
            .unwrap();
        let texto = exportar(&cofre, Path::new("cofre.json")).texto;
        let lote = ler(&texto).unwrap();
        assert_eq!(lote.itens, cofre.itens());
        assert_eq!(lote.itens[0].openvpn, Some(identidade));
    }
}
