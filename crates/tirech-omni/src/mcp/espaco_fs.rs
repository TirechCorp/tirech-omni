//! As operações de arquivo do espaço de trabalho (M51, seção 8) sobre o SFTP: `ler`, `editar`,
//! `escrever` e as reservas de `buscar` e `glob` para quando o servidor não roda `grep` nem
//! `find`. Tudo passa por um [`Fs`], então os testes usam o `MemFs`.
//!
//! Todo caminho que o assistente manda é [normalizado](normalizar) e [confinado](confinar) à raiz
//! real do espaço antes de qualquer acesso. A gravação é por arquivo temporário e `rename`, e o
//! SFTP não troca um arquivo que existe (sem `posix-rename@openssh.com` no `russh-sftp`): o
//! original vai para um arquivo de reserva, o novo entra no lugar e a reserva é apagada. Se o
//! `rename` falhar no meio, o original volta.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::time::UNIX_EPOCH;

use regex::Regex;
use tomni_ui::files::{Entry, EntryKind, Fs};

use super::caminho::{confinar, normalizar, relativo};
use super::espaco::{
    Leitura, Lido, aviso_crlf, binario_por_nul, contexto_da_resposta, inicio_e_linhas, ler_trecho,
    resposta_da_leitura, trocar,
};
use super::espaco_cmd::{ArquivoListado, PedidoDeBusca, SaidaDaBusca};
use super::gitignore::{Gitignore, Gitignores};
use super::glob;
use crate::arquivos::formato::tamanho;
use crate::t;

/// O maior arquivo que `editar` aceita ler inteiro.
const LIMITE_EDITAVEL: u64 = 16 * 1024 * 1024;

/// O maior arquivo que a busca pelo SFTP abre.
const LIMITE_DA_BUSCA: u64 = 4 * 1024 * 1024;

/// O maior `.gitignore` que se lê.
const LIMITE_DO_GITIGNORE: u64 = 256 * 1024;

/// Quantas entradas o percurso pelo SFTP olha antes de desistir.
const LIMITE_DE_ENTRADAS: usize = 50_000;

/// Quantas linhas de achado a busca pelo SFTP junta antes de parar.
const LIMITE_DE_LINHAS: usize = 5_000;

/// Quantos `lstat` o `glob` faz para saber a data de arquivos que o `git` listou sem data.
const LIMITE_DE_DATAS: usize = 3_000;

/// Uma sessão de arquivos de um espaço: o `Fs`, a raiz e o que o assistente já leu.
pub struct Remoto {
    fs: Box<dyn Fs>,
    raiz: String,
    raiz_real: String,
    lidos: HashMap<String, Lido>,
}

fn segundos(entrada: &Entry) -> Option<u64> {
    entrada
        .modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

fn erro_de(caminho: &str, e: &io::Error) -> String {
    if e.kind() == io::ErrorKind::NotFound {
        t!("mcp-espaco-nao-existe", caminho = caminho.to_string())
    } else {
        t!(
            "mcp-espaco-erro",
            caminho = caminho.to_string(),
            erro = e.to_string()
        )
    }
}

/// Quantas linhas um texto tem (a última sem `\n` conta).
pub fn contar_linhas(texto: &str) -> usize {
    texto.matches('\n').count() + usize::from(!texto.is_empty() && !texto.ends_with('\n'))
}

/// Se `rel` (relativo à raiz) casa com `padrao` visto a partir de `caminho` (relativo à raiz,
/// `.` para a própria raiz).
pub fn casa_no_caminho(padrao: &str, caminho: &str, rel: &str) -> bool {
    let resto = if caminho == "." || caminho.is_empty() {
        rel
    } else {
        match rel.strip_prefix(caminho).and_then(|r| r.strip_prefix('/')) {
            Some(r) => r,
            None => return false,
        }
    };
    glob::casa(padrao, resto)
}

/// Do mais recente ao mais antigo; sem data vai para o fim, e empata pelo caminho.
pub fn ordenar_por_data(lista: &mut [ArquivoListado]) {
    lista.sort_by(|a, b| {
        b.mtime
            .unwrap_or(0.0)
            .total_cmp(&a.mtime.unwrap_or(0.0))
            .then_with(|| a.caminho.cmp(&b.caminho))
    });
}

/// O texto de um `glob`: um caminho por linha, ou o aviso de que nada casa.
pub fn texto_do_glob(lista: &[ArquivoListado], padrao: &str) -> String {
    if lista.is_empty() {
        return t!("mcp-espaco-glob-nenhum", padrao = padrao.to_string());
    }
    lista
        .iter()
        .map(|a| a.caminho.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// As linhas de um arquivo que a busca mostra: os achados (`caminho:linha:texto`) e o contexto
/// em volta (`caminho-linha-texto`), com `--` entre grupos que não se tocam, como o `grep`.
pub fn linhas_do_achado(
    rel: &str,
    linhas: &[&str],
    acertos: &[usize],
    contexto: usize,
) -> Vec<String> {
    let mut mostrar = vec![false; linhas.len()];
    for &i in acertos {
        let de = i.saturating_sub(contexto);
        let ate = (i + contexto).min(linhas.len().saturating_sub(1));
        for marca in &mut mostrar[de..=ate] {
            *marca = true;
        }
    }
    let mut saida = Vec::new();
    let mut anterior: Option<usize> = None;
    for (j, linha) in linhas.iter().enumerate() {
        if !mostrar[j] {
            continue;
        }
        if contexto > 0 && anterior.is_some_and(|a| j > a + 1) {
            saida.push("--".to_owned());
        }
        let sep = if acertos.contains(&j) { ':' } else { '-' };
        saida.push(format!("{rel}{sep}{}{sep}{linha}", j + 1));
        anterior = Some(j);
    }
    saida
}

fn nome_temporario(real: &str, sufixo: &str) -> String {
    let (pasta, nome) = real.rsplit_once('/').unwrap_or(("", real));
    let mut bytes = [0u8; 4];
    // Sem sorteio o nome é o do contador do relógio; só precisa não bater com outro arquivo.
    if getrandom::fill(&mut bytes).is_err() {
        bytes = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
            .to_le_bytes();
    }
    let aleatorio: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{pasta}/.{nome}.tirech-{aleatorio}{sufixo}")
}

impl Remoto {
    /// Abre o espaço sobre `fs`: resolve a raiz e confere que é uma pasta.
    pub fn abrir(mut fs: Box<dyn Fs>, raiz: &str) -> Result<Self, String> {
        let ausente = || t!("mcp-espaco-raiz-ausente", raiz = raiz.to_string());
        let raiz_real = fs.real_path(raiz).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                ausente()
            } else {
                erro_de(raiz, &e)
            }
        })?;
        match fs.stat(&raiz_real) {
            Ok(entrada) if entrada.is_dir() => {}
            Ok(_) => return Err(ausente()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ausente()),
            Err(e) => return Err(erro_de(raiz, &e)),
        }
        Ok(Self {
            fs,
            raiz: raiz.to_owned(),
            raiz_real,
            lidos: HashMap::new(),
        })
    }

    /// A raiz com os links resolvidos.
    pub fn raiz_real(&self) -> &str {
        &self.raiz_real
    }

    /// Esquece que o assistente leu `caminho` (e todos os outros): depois de uma troca feita
    /// por fora, como um `bash`.
    pub fn esquecer_leituras(&mut self) {
        self.lidos.clear();
    }

    fn registrar(&mut self, real: &str) {
        if let Ok(entrada) = self.fs.stat(real) {
            self.lidos.insert(
                real.to_owned(),
                Lido {
                    mtime: segundos(&entrada),
                    tamanho: entrada.size,
                },
            );
        }
    }

    /// O caminho absoluto no servidor e o resolvido, de um caminho que existe.
    fn resolver(&mut self, caminho: &str) -> Result<(String, String, Entry), String> {
        let alvo = normalizar(&self.raiz, caminho)?;
        let entrada = self.fs.stat(&alvo).map_err(|e| erro_de(caminho, &e))?;
        let real = confinar(&mut *self.fs, &self.raiz_real, &alvo, false)?;
        Ok((alvo, real, entrada))
    }

    /// Confere que `caminho` é uma pasta (ou arquivo) do espaço e devolve o caminho relativo à
    /// raiz que o `grep` e o `find` usam (`.` para a raiz).
    pub fn caminho_relativo(&mut self, caminho: &str) -> Result<String, String> {
        let (alvo, _, _) = self.resolver(caminho)?;
        Ok(relativo(&self.raiz, &alvo))
    }

    /// `ler`: o trecho numerado de um arquivo.
    pub fn ler(
        &mut self,
        caminho: &str,
        inicio: Option<u64>,
        linhas: Option<u64>,
        padrao: usize,
    ) -> Result<String, String> {
        let (_, real, entrada) = self.resolver(caminho)?;
        if entrada.is_dir() {
            return Err(t!("mcp-espaco-e-pasta"));
        }
        let (inicio, linhas) = inicio_e_linhas(inicio, linhas, padrao);
        let mut arquivo = self
            .fs
            .open_read(&real, 0)
            .map_err(|e| erro_de(caminho, &e))?;
        let leitura = ler_trecho(&mut arquivo, inicio, linhas).map_err(|e| erro_de(caminho, &e))?;
        let _ = arquivo.finish();
        if leitura != Leitura::Binario {
            self.lidos.insert(
                real,
                Lido {
                    mtime: segundos(&entrada),
                    tamanho: entrada.size,
                },
            );
        }
        Ok(resposta_da_leitura(leitura, entrada.size, inicio))
    }

    fn conferir(&self, real: &str, entrada: &Entry, mensagem: String) -> Result<(), String> {
        let lido = self.lidos.get(real).ok_or(mensagem)?;
        if lido.tamanho != entrada.size || lido.mtime != segundos(entrada) {
            return Err(t!("mcp-espaco-mudou"));
        }
        Ok(())
    }

    fn ler_tudo(&mut self, real: &str, caminho: &str, limite: u64) -> Result<Vec<u8>, String> {
        let mut arquivo = self
            .fs
            .open_read(real, 0)
            .map_err(|e| erro_de(caminho, &e))?;
        let mut bytes = Vec::new();
        (&mut arquivo)
            .take(limite + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| erro_de(caminho, &e))?;
        let _ = arquivo.finish();
        if bytes.len() as u64 > limite {
            return Err(t!("mcp-espaco-grande", tamanho = tamanho(limite + 1)));
        }
        Ok(bytes)
    }

    /// `editar`: troca `antigo` por `novo` num arquivo que o assistente leu.
    pub fn editar(
        &mut self,
        caminho: &str,
        antigo: &str,
        novo: &str,
        todas: bool,
    ) -> Result<String, String> {
        let (_, real, entrada) = self.resolver(caminho)?;
        if entrada.is_dir() {
            return Err(t!("mcp-espaco-e-pasta"));
        }
        self.conferir(&real, &entrada, t!("mcp-espaco-ler-antes"))?;
        if entrada.size > LIMITE_EDITAVEL {
            return Err(t!("mcp-espaco-grande", tamanho = tamanho(entrada.size)));
        }
        let bytes = self.ler_tudo(&real, caminho, LIMITE_EDITAVEL)?;
        if binario_por_nul(&bytes) {
            return Err(t!("mcp-espaco-binario", tamanho = tamanho(entrada.size)));
        }
        let conteudo = String::from_utf8(bytes).map_err(|_| t!("mcp-espaco-nao-utf8-edicao"))?;
        let troca = trocar(&conteudo, antigo, novo, todas)?;
        self.gravar(&real, troca.conteudo.as_bytes(), entrada.permissions)?;
        self.registrar(&real);
        let mut resposta = contexto_da_resposta(&troca, novo);
        if let Some(aviso) = aviso_crlf(&conteudo, novo) {
            resposta.push('\n');
            resposta.push_str(&aviso);
        }
        Ok(resposta)
    }

    /// `escrever`: grava o arquivo inteiro. Um que existe precisa ter sido lido e continuar
    /// igual; um novo ganha as pastas que faltam (dentro da raiz).
    pub fn escrever(&mut self, caminho: &str, conteudo: &str) -> Result<String, String> {
        let alvo = normalizar(&self.raiz, caminho)?;
        let real = match self.fs.stat(&alvo) {
            Ok(entrada) => {
                if entrada.is_dir() {
                    return Err(t!(
                        "mcp-espaco-escrever-em-pasta",
                        caminho = caminho.to_string()
                    ));
                }
                let real = confinar(&mut *self.fs, &self.raiz_real, &alvo, false)?;
                self.conferir(&real, &entrada, t!("mcp-espaco-ler-antes-escrever"))?;
                self.gravar(&real, conteudo.as_bytes(), entrada.permissions)?;
                real
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let pai = alvo.rsplit_once('/').map_or("/", |(pai, _)| pai);
                self.criar_pastas(pai)?;
                let real = confinar(&mut *self.fs, &self.raiz_real, &alvo, true)?;
                self.gravar(&real, conteudo.as_bytes(), Some(0o644))?;
                real
            }
            Err(e) => return Err(erro_de(caminho, &e)),
        };
        self.registrar(&real);
        Ok(t!(
            "mcp-espaco-gravado",
            bytes = conteudo.len(),
            linhas = contar_linhas(conteudo)
        ))
    }

    /// Cria as pastas de `pai` (absoluto, dentro da raiz) que ainda não existem, conferindo
    /// cada uma que já existe contra os links para fora.
    fn criar_pastas(&mut self, pai: &str) -> Result<(), String> {
        let rel = relativo(&self.raiz, pai);
        if rel == "." {
            return Ok(());
        }
        let mut atual = self.raiz.trim_end_matches('/').to_owned();
        for parte in rel.split('/') {
            atual = format!("{atual}/{parte}");
            match self.fs.stat(&atual) {
                Ok(e) if e.is_dir() => {
                    confinar(&mut *self.fs, &self.raiz_real, &atual, false)?;
                }
                Ok(_) => return Err(t!("mcp-espaco-nao-e-pasta", caminho = atual)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    self.fs.mkdir(&atual).map_err(|e| erro_de(&atual, &e))?;
                }
                Err(e) => return Err(erro_de(&atual, &e)),
            }
        }
        Ok(())
    }

    /// Grava `bytes` em `real` por um arquivo temporário vizinho e um `rename`, mantendo o modo.
    fn gravar(&mut self, real: &str, bytes: &[u8], modo: Option<u32>) -> Result<(), String> {
        let temporario = nome_temporario(real, "");
        let escrito = (|| -> io::Result<()> {
            let mut saida = self.fs.open_write(&temporario, 0)?;
            saida.write_all(bytes)?;
            saida.finish()
        })();
        if let Err(e) = escrito {
            let _ = self.fs.remove_file(&temporario);
            return Err(erro_de(real, &e));
        }
        if let Some(modo) = modo {
            // Um servidor que não aceita mudar o modo deixa o do `umask`; não é motivo para falhar.
            let _ = self.fs.set_permissions(&temporario, modo);
        }
        let trocado = self.trocar_arquivo(&temporario, real);
        if trocado.is_err() {
            let _ = self.fs.remove_file(&temporario);
        }
        trocado.map_err(|e| erro_de(real, &e))
    }

    /// Põe `temporario` no lugar de `real`; o original só some depois que o novo entrou.
    fn trocar_arquivo(&mut self, temporario: &str, real: &str) -> io::Result<()> {
        if self.fs.lstat(real).is_err() {
            return self.fs.rename(temporario, real, false);
        }
        let reserva = nome_temporario(real, ".bak");
        self.fs.rename(real, &reserva, false)?;
        if let Err(e) = self.fs.rename(temporario, real, false) {
            let _ = self.fs.rename(&reserva, real, false);
            return Err(e);
        }
        let _ = self.fs.remove_file(&reserva);
        Ok(())
    }

    /// Percorre as pastas a partir de `real` (que tem `rel` de caminho relativo à raiz) sem
    /// seguir links de pasta e respeitando os `.gitignore`. Chama `visita(self, real, rel,
    /// entrada)` em cada arquivo, link ou outra coisa que não é pasta; `false` para o percurso.
    /// Devolve `true` se parou no limite de entradas.
    fn percorrer(
        &mut self,
        real: &str,
        rel: &str,
        visita: &mut dyn FnMut(&mut Self, &str, &str, &Entry) -> bool,
    ) -> bool {
        let mut ignorados = Gitignores::default();
        // Os `.gitignore` das pastas de cima valem aqui dentro.
        let mut base_real = self.raiz_real.clone();
        let mut base_rel = String::new();
        let partes: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        for parte in &partes[..partes.len().saturating_sub(1)] {
            ignorados = self.com_gitignore(ignorados, &base_real, &base_rel);
            base_real = format!("{}/{parte}", base_real.trim_end_matches('/'));
            base_rel = if base_rel.is_empty() {
                (*parte).to_owned()
            } else {
                format!("{base_rel}/{parte}")
            };
        }
        let mut pilha = vec![(real.to_owned(), rel.trim_matches('/').to_owned(), ignorados)];
        let mut vistas = 0usize;
        while let Some((pasta_real, pasta_rel, ignorados)) = pilha.pop() {
            let Ok(mut entradas) = self.fs.list(&pasta_real) else {
                continue;
            };
            entradas.sort_by(|a, b| a.name.cmp(&b.name));
            let ignorados = if entradas
                .iter()
                .any(|e| e.name == ".gitignore" && e.is_file())
            {
                self.com_gitignore(ignorados, &pasta_real, &pasta_rel)
            } else {
                ignorados
            };
            let mut subpastas = Vec::new();
            for entrada in &entradas {
                if entrada.name == ".git" {
                    continue;
                }
                vistas += 1;
                if vistas > LIMITE_DE_ENTRADAS {
                    return true;
                }
                let filho_real = format!("{}/{}", pasta_real.trim_end_matches('/'), entrada.name);
                let filho_rel = if pasta_rel.is_empty() {
                    entrada.name.clone()
                } else {
                    format!("{pasta_rel}/{}", entrada.name)
                };
                match entrada.kind {
                    EntryKind::Dir => {
                        if !ignorados.ignora(&filho_rel, true) {
                            subpastas.push((filho_real, filho_rel, ignorados.clone()));
                        }
                    }
                    _ => {
                        if !ignorados.ignora(&filho_rel, false)
                            && !visita(self, &filho_real, &filho_rel, entrada)
                        {
                            return false;
                        }
                    }
                }
            }
            pilha.extend(subpastas.into_iter().rev());
        }
        false
    }

    /// Acrescenta o `.gitignore` da pasta `real` (se houver) às regras.
    fn com_gitignore(&mut self, ignorados: Gitignores, real: &str, rel: &str) -> Gitignores {
        let caminho = format!("{}/.gitignore", real.trim_end_matches('/'));
        let Ok(entrada) = self.fs.stat(&caminho) else {
            return ignorados;
        };
        if !entrada.is_file() || entrada.size > LIMITE_DO_GITIGNORE {
            return ignorados;
        }
        match self.ler_tudo(&caminho, &caminho, LIMITE_DO_GITIGNORE) {
            Ok(bytes) => ignorados.com(rel, Gitignore::novo(&String::from_utf8_lossy(&bytes))),
            Err(_) => ignorados,
        }
    }

    /// `glob` pelo SFTP: os arquivos e links de `caminho` (relativo à raiz) que casam com
    /// `padrao`, com a data, sem ordenar.
    pub fn glob_pelo_sftp(
        &mut self,
        padrao: &str,
        caminho: &str,
    ) -> Result<Vec<ArquivoListado>, String> {
        let (alvo, real, entrada) = self.resolver(caminho)?;
        let base = relativo(&self.raiz, &alvo);
        let mut achados = Vec::new();
        if !entrada.is_dir() {
            return Ok(achados);
        }
        let rel = if base == "." { "" } else { base.as_str() };
        self.percorrer(&real, rel, &mut |_, _, rel, e| {
            if matches!(e.kind, EntryKind::File | EntryKind::Symlink)
                && casa_no_caminho(padrao, &base, rel)
            {
                achados.push(ArquivoListado {
                    caminho: rel.to_owned(),
                    mtime: e
                        .modified
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64()),
                });
            }
            true
        });
        Ok(achados)
    }

    /// Dá a data (pelo `lstat`) aos arquivos da lista que não a têm e tira os que sumiram
    /// (o `git ls-files -c` lista os apagados que ainda não foram para o commit).
    pub fn completar_datas(&mut self, lista: &mut Vec<ArquivoListado>) {
        let mut feitos = 0;
        lista.retain_mut(|a| {
            if a.mtime.is_some() || feitos >= LIMITE_DE_DATAS {
                return true;
            }
            feitos += 1;
            let caminho = format!("{}/{}", self.raiz_real.trim_end_matches('/'), a.caminho);
            match self.fs.lstat(&caminho) {
                Ok(e) => {
                    a.mtime = e
                        .modified
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64());
                    true
                }
                Err(e) => e.kind() != io::ErrorKind::NotFound,
            }
        });
    }

    /// `buscar` pelo SFTP: a reserva de quando o servidor não roda `grep`.
    pub fn buscar_pelo_sftp(&mut self, pedido: &PedidoDeBusca) -> Result<String, String> {
        let mut padrao = if pedido.regex {
            pedido.padrao.clone()
        } else {
            regex::escape(&pedido.padrao)
        };
        if !pedido.maiusculas {
            padrao = format!("(?i){padrao}");
        }
        let re =
            Regex::new(&padrao).map_err(|e| t!("mcp-espaco-busca-regex", erro = e.to_string()))?;
        let (alvo, real, entrada) = self.resolver(&pedido.caminho)?;
        let base = relativo(&self.raiz, &alvo);
        let mut saida: Vec<String> = vec![t!("mcp-espaco-busca-sftp")];
        let mut cortada = false;
        let mut procurar = |remoto: &mut Self, real: &str, rel: &str, tamanho: u64| -> bool {
            if let Some(g) = &pedido.glob {
                let nome = rel.rsplit('/').next().unwrap_or(rel);
                let alvo = if g.contains('/') { rel } else { nome };
                if !glob::casa(g, alvo) {
                    return true;
                }
            }
            if tamanho > LIMITE_DA_BUSCA {
                return true;
            }
            let Ok(bytes) = remoto.ler_tudo(real, rel, LIMITE_DA_BUSCA) else {
                return true;
            };
            if binario_por_nul(&bytes) {
                return true;
            }
            let texto = String::from_utf8_lossy(&bytes);
            let linhas: Vec<&str> = texto.lines().collect();
            let acertos: Vec<usize> = linhas
                .iter()
                .enumerate()
                .filter(|(_, l)| re.is_match(l))
                .map(|(i, _)| i)
                .collect();
            if acertos.is_empty() {
                return true;
            }
            match pedido.saida {
                SaidaDaBusca::Arquivos => saida.push(rel.to_owned()),
                SaidaDaBusca::Contagem => saida.push(format!("{rel}:{}", acertos.len())),
                SaidaDaBusca::Linhas => {
                    saida.extend(linhas_do_achado(rel, &linhas, &acertos, pedido.contexto));
                }
            }
            if saida.len() > LIMITE_DE_LINHAS {
                cortada = true;
                return false;
            }
            true
        };
        if entrada.is_dir() {
            let rel = if base == "." { "" } else { base.as_str() };
            let limite = self.percorrer(&real, rel, &mut |r, real, rel, e| {
                // O `grep -r` não segue links; um arquivo comum por vez.
                e.kind != EntryKind::File || procurar(r, real, rel, e.size)
            });
            cortada |= limite;
        } else {
            procurar(self, &real, &base, entrada.size);
        }
        if saida.len() == 1 {
            saida.push(t!("mcp-espaco-busca-nada"));
        }
        if cortada {
            saida.push(t!("mcp-espaco-busca-corte"));
        }
        Ok(saida.join("\n"))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use tomni_ui::files::MemFs;

    fn espaco() -> (MemFs, Remoto) {
        let fs = MemFs::new();
        fs.dir("/srv")
            .dir("/srv/app")
            .dir("/srv/app/src")
            .dir("/srv/fora");
        fs.put("/srv/app/a.txt", "um\ndois\ntrês\n");
        fs.put("/srv/app/src/main.rs", "fn main() {}\n// TODO: tudo\n");
        fs.put("/srv/app/src/lib.rs", "pub fn lib() {}\n");
        fs.put("/srv/app/.gitignore", "alvo/\n*.log\n");
        fs.put("/srv/app/x.log", "TODO: log\n");
        fs.dir("/srv/app/alvo")
            .put("/srv/app/alvo/saida.rs", "// TODO: alvo\n");
        fs.put("/srv/fora/segredo.txt", "TODO: não deve sair");
        let remoto = Remoto::abrir(Box::new(fs.connect()), "/srv/app").unwrap();
        (fs, remoto)
    }

    fn pedido(padrao: &str) -> PedidoDeBusca {
        PedidoDeBusca {
            padrao: padrao.to_owned(),
            regex: false,
            caminho: ".".to_owned(),
            glob: None,
            maiusculas: true,
            contexto: 0,
            saida: SaidaDaBusca::Linhas,
        }
    }

    #[test]
    fn raiz_inexistente() {
        let fs = MemFs::new();
        let e = Remoto::abrir(Box::new(fs.connect()), "/nao/existe")
            .err()
            .unwrap();
        assert_eq!(e, t!("mcp-espaco-raiz-ausente", raiz = "/nao/existe"));
        fs.put("/arquivo", "x");
        assert!(Remoto::abrir(Box::new(fs.connect()), "/arquivo").is_err());
    }

    #[test]
    fn ler_com_inicio_e_linhas() {
        let (_, mut r) = espaco();
        let todo = r.ler("a.txt", None, None, 2000).unwrap();
        assert_eq!(todo, "     1\tum\n     2\tdois\n     3\ttrês");
        let parte = r.ler("a.txt", Some(2), Some(1), 2000).unwrap();
        assert!(parte.starts_with("     2\tdois"), "{parte}");
        assert!(parte.contains("inicio=3"), "{parte}");
        assert_eq!(
            r.ler("src", None, None, 10).unwrap_err(),
            t!("mcp-espaco-e-pasta")
        );
        assert_eq!(
            r.ler("nada.txt", None, None, 10).unwrap_err(),
            t!("mcp-espaco-nao-existe", caminho = "nada.txt")
        );
    }

    #[test]
    fn ler_binario_recusa() {
        let (fs, mut r) = espaco();
        fs.put("/srv/app/b.bin", b"PK\0\0abc");
        let texto = r.ler("b.bin", None, None, 10).unwrap();
        assert_eq!(texto, t!("mcp-espaco-binario", tamanho = tamanho(7)));
        assert_eq!(
            r.editar("b.bin", "a", "b", false).unwrap_err(),
            t!("mcp-espaco-ler-antes")
        );
    }

    #[test]
    fn fora_da_raiz_e_recusado() {
        let (fs, mut r) = espaco();
        assert!(r.ler("../fora/segredo.txt", None, None, 10).is_err());
        assert!(r.ler("/srv/fora/segredo.txt", None, None, 10).is_err());
        fs.link("/srv/app/atalho.txt", "/srv/fora/segredo.txt");
        let e = r.ler("atalho.txt", None, None, 10).unwrap_err();
        assert!(e.contains("atalho.txt"), "{e}");
        assert!(r.escrever("atalho.txt", "x").is_err());
        assert_eq!(
            fs.contents("/srv/fora/segredo.txt").unwrap(),
            b"TODO: n\xc3\xa3o deve sair"
        );
    }

    #[test]
    fn editar_sem_ler_recusa() {
        let (_, mut r) = espaco();
        assert_eq!(
            r.editar("a.txt", "dois", "2", false).unwrap_err(),
            t!("mcp-espaco-ler-antes")
        );
    }

    #[test]
    fn editar_troca_e_preserva_o_modo() {
        let (fs, mut r) = espaco();
        fs.put("/srv/app/run.sh", "#!/bin/sh\necho oi\n");
        Fs::set_permissions(&mut fs.connect(), "/srv/app/run.sh", 0o755).unwrap();
        r.ler("run.sh", None, None, 10).unwrap();
        let resposta = r.editar("run.sh", "echo oi", "echo tchau", false).unwrap();
        assert!(
            resposta.starts_with(&t!("mcp-espaco-editado", n = 1)),
            "{resposta}"
        );
        assert!(resposta.contains("     2\techo tchau"), "{resposta}");
        assert_eq!(
            fs.contents("/srv/app/run.sh").unwrap(),
            b"#!/bin/sh\necho tchau\n"
        );
        let modo = Fs::stat(&mut fs.connect(), "/srv/app/run.sh")
            .unwrap()
            .permissions;
        assert_eq!(modo, Some(0o755));
        // Não sobra temporário nem reserva.
        assert!(
            fs.paths().iter().all(|p| !p.contains(".tirech-")),
            "{:?}",
            fs.paths()
        );
        // Outra edição seguida funciona sem reler: o app atualizou o que sabe.
        r.editar("run.sh", "tchau", "até", false).unwrap();
    }

    #[test]
    fn editar_depois_de_mudanca_no_servidor_recusa() {
        let (fs, mut r) = espaco();
        r.ler("a.txt", None, None, 10).unwrap();
        fs.put("/srv/app/a.txt", "outro conteúdo, maior\n");
        assert_eq!(
            r.editar("a.txt", "dois", "2", false).unwrap_err(),
            t!("mcp-espaco-mudou")
        );
        assert_eq!(
            r.escrever("a.txt", "x").unwrap_err(),
            t!("mcp-espaco-mudou")
        );
    }

    #[test]
    fn editar_recusa_erros_do_texto() {
        let (fs, mut r) = espaco();
        r.ler("a.txt", None, None, 10).unwrap();
        assert!(r.editar("a.txt", "zzz", "y", false).is_err());
        fs.put("/srv/app/i.txt", b"ol\xe1\n");
        r.ler("i.txt", None, None, 10).unwrap();
        assert_eq!(
            r.editar("i.txt", "ol", "oi", false).unwrap_err(),
            t!("mcp-espaco-nao-utf8-edicao")
        );
    }

    #[test]
    fn escrever_cria_as_pastas_dentro_da_raiz() {
        let (fs, mut r) = espaco();
        let resposta = r.escrever("novo/fundo/arq.txt", "a\nb").unwrap();
        assert_eq!(resposta, t!("mcp-espaco-gravado", bytes = 3, linhas = 2));
        assert_eq!(fs.contents("/srv/app/novo/fundo/arq.txt").unwrap(), b"a\nb");
        let modo = Fs::stat(&mut fs.connect(), "/srv/app/novo/fundo/arq.txt")
            .unwrap()
            .permissions;
        assert_eq!(modo, Some(0o644));
        assert!(r.escrever("../fora/novo.txt", "x").is_err());
        assert!(!fs.exists("/srv/fora/novo.txt"));
    }

    #[test]
    fn escrever_por_cima_exige_leitura() {
        let (fs, mut r) = espaco();
        assert_eq!(
            r.escrever("a.txt", "x").unwrap_err(),
            t!("mcp-espaco-ler-antes-escrever")
        );
        r.ler("a.txt", None, None, 10).unwrap();
        r.escrever("a.txt", "novo\n").unwrap();
        assert_eq!(fs.contents("/srv/app/a.txt").unwrap(), b"novo\n");
        assert!(r.escrever("src", "x").is_err());
    }

    #[test]
    fn buscar_pelo_sftp_acha_e_respeita_o_gitignore() {
        let (_, mut r) = espaco();
        let texto = r.buscar_pelo_sftp(&pedido("TODO")).unwrap();
        assert!(texto.starts_with(&t!("mcp-espaco-busca-sftp")), "{texto}");
        assert!(texto.contains("src/main.rs:2:// TODO: tudo"), "{texto}");
        assert!(
            !texto.contains("x.log") && !texto.contains("alvo"),
            "{texto}"
        );
        let mut p = pedido("todo");
        p.maiusculas = false;
        p.saida = SaidaDaBusca::Arquivos;
        p.caminho = "src".into();
        let texto = r.buscar_pelo_sftp(&p).unwrap();
        assert!(texto.ends_with("src/main.rs"), "{texto}");
        let mut p = pedido("fn \\w+\\(\\)");
        p.regex = true;
        p.saida = SaidaDaBusca::Contagem;
        let texto = r.buscar_pelo_sftp(&p).unwrap();
        assert!(
            texto.contains("src/lib.rs:1") && texto.contains("src/main.rs:1"),
            "{texto}"
        );
        p.padrao = "(".into();
        assert!(r.buscar_pelo_sftp(&p).is_err());
        let nada = r.buscar_pelo_sftp(&pedido("zzzz")).unwrap();
        assert!(nada.ends_with(&t!("mcp-espaco-busca-nada")), "{nada}");
    }

    #[test]
    fn buscar_com_glob_e_contexto() {
        let (_, mut r) = espaco();
        let mut p = pedido("TODO");
        p.glob = Some("*.rs".into());
        p.contexto = 1;
        let texto = r.buscar_pelo_sftp(&p).unwrap();
        assert!(texto.contains("src/main.rs-1-fn main() {}"), "{texto}");
        assert!(texto.contains("src/main.rs:2:// TODO: tudo"), "{texto}");
        p.glob = Some("src/**/*.rs".into());
        assert!(r.buscar_pelo_sftp(&p).unwrap().contains("main.rs"));
        p.glob = Some("*.md".into());
        assert!(
            r.buscar_pelo_sftp(&p)
                .unwrap()
                .ends_with(&t!("mcp-espaco-busca-nada"))
        );
    }

    #[test]
    fn glob_pelo_sftp() {
        let (_, mut r) = espaco();
        let mut l = r.glob_pelo_sftp("**/*.rs", ".").unwrap();
        ordenar_por_data(&mut l);
        let nomes: Vec<&str> = l.iter().map(|a| a.caminho.as_str()).collect();
        assert_eq!(nomes, ["src/lib.rs", "src/main.rs"]);
        let l = r.glob_pelo_sftp("*.rs", "src").unwrap();
        assert_eq!(l.len(), 2);
        let l = r.glob_pelo_sftp("*.txt", ".").unwrap();
        assert_eq!(l[0].caminho, "a.txt");
        assert_eq!(
            texto_do_glob(&[], "*.zip"),
            t!("mcp-espaco-glob-nenhum", padrao = "*.zip")
        );
    }

    #[test]
    fn datas_completadas_e_apagados_somem() {
        let (fs, mut r) = espaco();
        let mut lista = vec![
            ArquivoListado {
                caminho: "a.txt".into(),
                mtime: None,
            },
            ArquivoListado {
                caminho: "apagado.txt".into(),
                mtime: None,
            },
            ArquivoListado {
                caminho: "src/lib.rs".into(),
                mtime: Some(5.0),
            },
        ];
        r.completar_datas(&mut lista);
        assert_eq!(lista.len(), 2);
        assert!(lista[0].mtime.is_some());
        let _ = fs;
    }

    #[test]
    fn achados_com_contexto() {
        let linhas = ["a", "b", "c", "d", "e", "f"];
        assert_eq!(
            linhas_do_achado("x", &linhas, &[0, 5], 1),
            ["x:1:a", "x-2-b", "--", "x-5-e", "x:6:f"]
        );
        assert_eq!(linhas_do_achado("x", &linhas, &[2], 0), ["x:3:c"]);
    }

    #[test]
    fn auxiliares() {
        assert_eq!(contar_linhas(""), 0);
        assert_eq!(contar_linhas("a"), 1);
        assert_eq!(contar_linhas("a\n"), 1);
        assert_eq!(contar_linhas("a\nb"), 2);
        assert!(casa_no_caminho("*.rs", "src", "src/a.rs"));
        assert!(!casa_no_caminho("*.rs", "src", "lib/a.rs"));
        assert!(casa_no_caminho("**/*.rs", ".", "src/a.rs"));
    }
}
