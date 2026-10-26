//! Scripts em vários servidores (M42): a pasta de scripts, os arquivos, o estado de cada
//! servidor e a rodada que roda tudo em paralelo pelo canal `exec` do SSH. A tela está em
//! `tela/scripts.rs`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use tomni_ui::prelude::Signal;

use crate::abrir::Executor;
use crate::agente::acordar;
use crate::configuracoes::Configuracoes;
use crate::t;

/// Um arquivo da pasta de scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    /// O nome do arquivo.
    pub nome: String,
    /// O caminho completo.
    pub caminho: PathBuf,
}

/// A pasta dos scripts: `scripts.pasta`, ou `scripts` dentro da pasta de dados quando vazia.
/// Um `~` no começo é a pasta pessoal.
pub fn pasta_de_scripts(config: &Configuracoes, dados: &Path) -> PathBuf {
    match config.scripts.pasta.trim() {
        "" => dados.join("scripts"),
        pasta => crate::armazenamento::expandir_til(pasta),
    }
}

/// Se o nome é de um arquivo que não é script: oculto ou cópia de editor.
fn e_lixo(nome: &str) -> bool {
    nome.starts_with('.')
        || nome.ends_with('~')
        || nome.ends_with(".bak")
        || nome.ends_with(".swp")
        || nome.ends_with(".swo")
}

/// Os arquivos de `pasta` (sem subpastas), por nome, sem os ocultos nem as cópias dos editores.
/// A pasta é criada se não existir.
pub fn listar(pasta: &Path) -> Vec<Script> {
    let _ = fs::create_dir_all(pasta);
    let Ok(entradas) = fs::read_dir(pasta) else {
        return Vec::new();
    };
    let mut scripts: Vec<Script> = entradas
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let nome = e.file_name().to_string_lossy().into_owned();
            (!e_lixo(&nome)).then(|| Script {
                nome,
                caminho: e.path(),
            })
        })
        .collect();
    scripts.sort_by_key(|s| s.nome.to_lowercase());
    scripts
}

/// Cria `nome.sh` em `pasta` com um cabeçalho, sem sobrescrever: com o nome já usado, põe
/// ` (2)`, ` (3)`… Devolve o caminho criado.
pub fn novo_script(pasta: &Path, nome: &str) -> io::Result<PathBuf> {
    let nome = nome.trim();
    if nome.is_empty() || nome.contains(['/', '\\']) || nome == "." || nome == ".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            t!("scripts-nome-invalido"),
        ));
    }
    fs::create_dir_all(pasta)?;
    let base = nome.strip_suffix(".sh").unwrap_or(nome);
    for n in 1u32.. {
        let arquivo = if n == 1 {
            format!("{base}.sh")
        } else {
            format!("{base} ({n}).sh")
        };
        let caminho = pasta.join(arquivo);
        let aberto = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&caminho);
        match aberto {
            Ok(mut f) => {
                io::Write::write_all(
                    &mut f,
                    format!("#!/bin/sh\n# {}\n\n", t!("scripts-modelo-comentario")).as_bytes(),
                )?;
                return Ok(caminho);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    unreachable!("o laço só sai pelo return")
}

/// Em que pé está um servidor da rodada.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Estado {
    /// Ainda não começou (espera uma vaga).
    Esperando,
    /// Rodando.
    Rodando,
    /// Saiu com o código 0.
    Ok,
    /// Saiu com outro código (ou sem código).
    Falhou(Option<u32>),
    /// Falhou antes de rodar: a conexão, a senha, ou a rodada foi parada.
    Erro(String),
}

impl Estado {
    /// Se já terminou, de um jeito ou de outro.
    pub fn terminou(&self) -> bool {
        !matches!(self, Self::Esperando | Self::Rodando)
    }

    /// O texto da coluna "Estado".
    pub fn texto(&self) -> String {
        match self {
            Self::Esperando => t!("scripts-estado-esperando"),
            Self::Rodando => t!("scripts-estado-rodando"),
            Self::Ok => t!("scripts-estado-concluido"),
            Self::Falhou(_) => t!("scripts-estado-falhou"),
            Self::Erro(_) => t!("scripts-estado-erro"),
        }
    }
}

/// O resultado de um servidor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resultado {
    /// O id da conexão.
    pub id: u64,
    /// O nome da conexão.
    pub nome: String,
    /// Em que pé está.
    pub estado: Estado,
    /// Quanto levou (preenchido ao terminar).
    pub duracao: Option<Duration>,
    /// O stdout seguido do stderr, como o terminal mostraria; num erro, a mensagem.
    pub saida: Vec<u8>,
}

impl Resultado {
    /// O código de saída, se houve.
    pub fn codigo(&self) -> Option<u32> {
        match self.estado {
            Estado::Ok => Some(0),
            Estado::Falhou(codigo) => codigo,
            _ => None,
        }
    }
}

/// A primeira linha com texto da saída, sem os códigos de escape ANSI, em até 120 caracteres.
pub fn primeira_linha(saida: &[u8]) -> String {
    let texto = String::from_utf8_lossy(saida);
    let limpo = sem_ansi(&texto);
    let linha = limpo
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if linha.chars().count() > 120 {
        let mut curta: String = linha.chars().take(119).collect();
        curta.push('…');
        curta
    } else {
        linha.to_owned()
    }
}

/// Tira as sequências de escape ANSI (CSI e OSC) e os controles que não são quebra de linha.
fn sem_ansi(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut chars = texto.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.peek() {
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            },
            '\n' | '\t' => saida.push(c),
            c if c.is_control() => {}
            c => saida.push(c),
        }
    }
    saida
}

/// "1,2 s" ou "250 ms".
pub fn duracao_curta(d: Duration) -> String {
    if d < Duration::from_secs(1) {
        format!("{} ms", d.as_millis())
    } else {
        format!("{} s", crate::i18n::decimal(d.as_secs_f64(), 1))
    }
}

/// O resumo da rodada: "3 de 5 terminaram — 2 ok, 1 falhou".
pub fn resumo(resultados: &[Resultado]) -> String {
    let total = resultados.len();
    let terminaram = resultados.iter().filter(|r| r.estado.terminou()).count();
    let ok = resultados.iter().filter(|r| r.estado == Estado::Ok).count();
    let falhas = terminaram - ok;
    let mut texto = if terminaram == total && total > 0 {
        t!("scripts-resumo-todos", n = total as i64)
    } else {
        t!(
            "scripts-resumo-parcial",
            terminaram = terminaram as i64,
            total = total as i64
        )
    };
    if terminaram > 0 {
        texto.push_str(" — ");
        texto.push_str(&t!("scripts-resumo-ok", n = ok as i64));
        if falhas > 0 {
            texto.push_str(", ");
            texto.push_str(&t!("scripts-resumo-falhas", n = falhas as i64));
        }
    }
    texto
}

/// O arquivo de resultados: um bloco `=== servidor (código) ===` por servidor.
pub fn texto_dos_resultados(resultados: &[Resultado]) -> String {
    let mut texto = String::new();
    for r in resultados {
        let codigo = match &r.estado {
            Estado::Ok => t!("scripts-resultado-codigo", c = 0),
            Estado::Falhou(Some(c)) => t!("scripts-resultado-codigo", c = i64::from(*c)),
            Estado::Falhou(None) => t!("scripts-sem-codigo"),
            Estado::Erro(_) => t!("scripts-resultado-erro"),
            Estado::Rodando => t!("scripts-resultado-rodando"),
            Estado::Esperando => t!("scripts-resultado-esperando"),
        };
        texto.push_str(&format!("=== {} ({codigo}) ===\n", r.nome));
        let saida = sem_ansi(&String::from_utf8_lossy(&r.saida));
        texto.push_str(&saida);
        if !saida.ends_with('\n') {
            texto.push('\n');
        }
        texto.push('\n');
    }
    texto
}

/// O que a rodada roda: um arquivo de script (que vai pela entrada do interpretador) ou uma
/// linha digitada (que é o próprio comando).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tarefa {
    /// O conteúdo do arquivo, lido por `interpretador`.
    Arquivo {
        /// O comando do interpretador (`sh -s`).
        interpretador: String,
        /// O conteúdo do arquivo.
        conteudo: Vec<u8>,
    },
    /// Uma linha digitada.
    Linha(String),
}

impl Tarefa {
    /// O comando do `exec` e a entrada dele.
    pub fn comando_e_entrada(&self) -> (String, Vec<u8>) {
        match self {
            Self::Arquivo {
                interpretador,
                conteudo,
            } => {
                let interpretador = interpretador.trim();
                let comando = if interpretador.is_empty() {
                    "sh -s"
                } else {
                    interpretador
                };
                (comando.to_owned(), conteudo.clone())
            }
            Self::Linha(linha) => (linha.trim().to_owned(), Vec::new()),
        }
    }
}

/// Uma rodada de um script em vários servidores.
pub struct Rodada {
    resultados: Arc<Mutex<Vec<Resultado>>>,
    parar: Arc<AtomicBool>,
}

impl Rodada {
    /// Começa a rodar `comando` (com `entrada`) em cada `(id, nome, executor)`, com no máximo
    /// `paralelo` ao mesmo tempo. Volta na hora: uma thread coordenadora faz o resto e acorda
    /// `pronto` a cada servidor que muda de estado.
    pub fn iniciar(
        alvos: Vec<(u64, String, Executor)>,
        comando: String,
        entrada: Vec<u8>,
        paralelo: usize,
        pronto: Signal<u64>,
    ) -> Self {
        let resultados = Arc::new(Mutex::new(
            alvos
                .iter()
                .map(|(id, nome, _)| Resultado {
                    id: *id,
                    nome: nome.clone(),
                    estado: Estado::Esperando,
                    duracao: None,
                    saida: Vec::new(),
                })
                .collect::<Vec<_>>(),
        ));
        let parar = Arc::new(AtomicBool::new(false));
        let rodada = Self {
            resultados: Arc::clone(&resultados),
            parar: Arc::clone(&parar),
        };
        if alvos.is_empty() {
            return rodada;
        }
        let paralelo = paralelo.max(1);
        std::thread::spawn(move || {
            let (terminou, terminados) = mpsc::channel::<()>();
            let mut rodando = 0usize;
            for (indice, (_, _, executor)) in alvos.into_iter().enumerate() {
                while rodando >= paralelo {
                    let _ = terminados.recv();
                    rodando -= 1;
                }
                if parar.load(Ordering::SeqCst) {
                    marcar(&resultados, indice, |r| {
                        r.estado = Estado::Erro("parado".to_owned());
                        r.saida = b"parado".to_vec();
                    });
                    acordar(pronto);
                    continue;
                }
                marcar(&resultados, indice, |r| r.estado = Estado::Rodando);
                acordar(pronto);
                rodando += 1;
                let (resultados, terminou) = (Arc::clone(&resultados), terminou.clone());
                let (comando, entrada) = (comando.clone(), entrada.clone());
                std::thread::spawn(move || {
                    let inicio = Instant::now();
                    let retorno = executor(&comando, &entrada);
                    let duracao = inicio.elapsed();
                    marcar(&resultados, indice, |r| {
                        r.duracao = Some(duracao);
                        match retorno {
                            Ok(saida) => {
                                r.estado = if saida.status == Some(0) {
                                    Estado::Ok
                                } else {
                                    Estado::Falhou(saida.status)
                                };
                                let mut juntas = saida.stdout;
                                juntas.extend_from_slice(&saida.stderr);
                                r.saida = juntas;
                            }
                            Err(erro) => {
                                r.saida = erro.clone().into_bytes();
                                r.estado = Estado::Erro(erro);
                            }
                        }
                    });
                    acordar(pronto);
                    let _ = terminou.send(());
                });
            }
        });
        rodada
    }

    /// Uma cópia dos resultados de agora.
    pub fn resultados(&self) -> Vec<Resultado> {
        self.resultados
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Se todos os servidores já terminaram.
    pub fn terminou(&self) -> bool {
        self.resultados
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .all(|r| r.estado.terminou())
    }

    /// Para de começar servidores novos: os que esperam viram `Erro("parado")`; os que já rodam
    /// terminam (o `exec` não se interrompe).
    pub fn parar(&self) {
        self.parar.store(true, Ordering::SeqCst);
    }
}

fn marcar(resultados: &Mutex<Vec<Resultado>>, indice: usize, f: impl FnOnce(&mut Resultado)) {
    let mut trava = resultados.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(r) = trava.get_mut(indice) {
        f(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomni_ui::prelude::*;
    use tomni_ui::terminal::ssh::ExecOutput;

    fn pasta_temporaria(nome: &str) -> PathBuf {
        let pasta =
            std::env::temp_dir().join(format!("tirech-scripts-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        pasta
    }

    #[test]
    fn a_pasta_padrao_fica_nos_dados_e_a_configurada_vale() {
        let mut config = Configuracoes::default();
        assert_eq!(
            pasta_de_scripts(&config, Path::new("/dados")),
            Path::new("/dados/scripts")
        );
        config.scripts.pasta = "/meus/scripts".to_owned();
        assert_eq!(
            pasta_de_scripts(&config, Path::new("/dados")),
            Path::new("/meus/scripts")
        );
    }

    #[test]
    fn listar_cria_a_pasta_ordena_e_pula_ocultos_copias_e_subpastas() {
        let pasta = pasta_temporaria("listar");
        assert!(listar(&pasta).is_empty());
        assert!(pasta.is_dir());
        for nome in ["b.sh", "A.sh", ".oculto", "c.sh~", "d.bak", ".e.swp"] {
            fs::write(pasta.join(nome), "").unwrap();
        }
        fs::create_dir(pasta.join("sub")).unwrap();
        let nomes: Vec<_> = listar(&pasta).into_iter().map(|s| s.nome).collect();
        assert_eq!(nomes, ["A.sh", "b.sh"]);
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn novo_script_grava_o_cabecalho_e_nao_sobrescreve() {
        let pasta = pasta_temporaria("novo");
        let a = novo_script(&pasta, "limpar").unwrap();
        assert_eq!(a.file_name().unwrap(), "limpar.sh");
        assert_eq!(
            fs::read_to_string(&a).unwrap(),
            "#!/bin/sh\n# Roda em cada servidor escolhido.\n\n"
        );
        fs::write(&a, "meu").unwrap();
        let b = novo_script(&pasta, "limpar").unwrap();
        let c = novo_script(&pasta, "limpar.sh").unwrap();
        assert_eq!(b.file_name().unwrap(), "limpar (2).sh");
        assert_eq!(c.file_name().unwrap(), "limpar (3).sh");
        assert_eq!(fs::read_to_string(&a).unwrap(), "meu");
        assert!(novo_script(&pasta, "  ").is_err());
        assert!(novo_script(&pasta, "../fora").is_err());
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn a_primeira_linha_pula_vazias_tira_ansi_e_corta_em_120() {
        assert_eq!(primeira_linha(b""), "");
        assert_eq!(primeira_linha(b"\n  \nola\nmundo"), "ola");
        assert_eq!(
            primeira_linha(b"\x1b[1;32mverde\x1b[0m e mais"),
            "verde e mais"
        );
        assert_eq!(primeira_linha(b"\x1b]0;titulo\x07texto"), "texto");
        let longa = "x".repeat(300);
        let curta = primeira_linha(longa.as_bytes());
        assert_eq!(curta.chars().count(), 120);
        assert!(curta.ends_with('…'));
    }

    #[test]
    fn a_duracao_vira_texto_curto() {
        assert_eq!(duracao_curta(Duration::from_millis(250)), "250 ms");
        assert_eq!(duracao_curta(Duration::from_millis(1200)), "1,2 s");
    }

    fn resultado(estado: Estado) -> Resultado {
        Resultado {
            id: 1,
            nome: "s".to_owned(),
            estado,
            duracao: None,
            saida: Vec::new(),
        }
    }

    #[test]
    fn o_resumo_conta_os_que_terminaram_os_ok_e_as_falhas() {
        let r = [
            resultado(Estado::Ok),
            resultado(Estado::Ok),
            resultado(Estado::Falhou(Some(2))),
            resultado(Estado::Rodando),
            resultado(Estado::Esperando),
        ];
        assert_eq!(resumo(&r), "3 de 5 terminaram — 2 ok, 1 falhou");
        let todos = [resultado(Estado::Ok), resultado(Estado::Erro("x".into()))];
        assert_eq!(
            resumo(&todos),
            "Os 2 servidores terminaram — 1 ok, 1 falhou"
        );
        assert_eq!(
            resumo(&[resultado(Estado::Ok)]),
            "1 servidor terminou — 1 ok"
        );
        assert_eq!(resumo(&[resultado(Estado::Esperando)]), "0 de 1 terminaram");
    }

    #[test]
    fn a_tarefa_de_arquivo_vai_pelo_interpretador_e_a_linha_e_o_comando() {
        let t = Tarefa::Arquivo {
            interpretador: " bash -s ".to_owned(),
            conteudo: b"echo oi\n".to_vec(),
        };
        assert_eq!(
            t.comando_e_entrada(),
            ("bash -s".to_owned(), b"echo oi\n".to_vec())
        );
        let vazio = Tarefa::Arquivo {
            interpretador: "  ".to_owned(),
            conteudo: Vec::new(),
        };
        assert_eq!(vazio.comando_e_entrada().0, "sh -s");
        let l = Tarefa::Linha(" uptime ".to_owned());
        assert_eq!(l.comando_e_entrada(), ("uptime".to_owned(), Vec::new()));
    }

    #[test]
    fn o_texto_dos_resultados_tem_um_bloco_por_servidor() {
        let mut ok = resultado(Estado::Ok);
        ok.nome = "web-01".to_owned();
        ok.saida = b"\x1b[32mtudo bem\x1b[0m".to_vec();
        let mut ruim = resultado(Estado::Falhou(Some(3)));
        ruim.nome = "db-01".to_owned();
        ruim.saida = b"deu ruim\n".to_vec();
        assert_eq!(
            texto_dos_resultados(&[ok, ruim]),
            "=== web-01 (código 0) ===\ntudo bem\n\n=== db-01 (código 3) ===\ndeu ruim\n\n"
        );
    }

    fn executor(
        status: Option<u32>,
        saida: &'static str,
        atraso: Duration,
        simultaneos: Option<Arc<(Mutex<usize>, Mutex<usize>)>>,
    ) -> Executor {
        Box::new(move |_, _| {
            if let Some(s) = &simultaneos {
                let agora = {
                    let mut n = s.0.lock().unwrap();
                    *n += 1;
                    *n
                };
                let mut maximo = s.1.lock().unwrap();
                *maximo = (*maximo).max(agora);
            }
            std::thread::sleep(atraso);
            if let Some(s) = &simultaneos {
                *s.0.lock().unwrap() -= 1;
            }
            Ok(ExecOutput {
                status,
                stdout: saida.as_bytes().to_vec(),
                stderr: b"!".to_vec(),
            })
        })
    }

    fn esperar(rodada: &Rodada) {
        for _ in 0..500 {
            if rodada.terminou() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("a rodada não terminou");
    }

    #[test]
    fn a_rodada_junta_a_saida_marca_o_estado_e_respeita_o_paralelo() {
        let sinal = signal(0u64);
        let simultaneos = Arc::new((Mutex::new(0usize), Mutex::new(0usize)));
        let alvos: Vec<(u64, String, Executor)> = (1..=6)
            .map(|i| {
                let status = if i == 3 { Some(7) } else { Some(0) };
                (
                    i,
                    format!("s{i}"),
                    executor(
                        status,
                        "saida\n",
                        Duration::from_millis(30),
                        Some(Arc::clone(&simultaneos)),
                    ),
                )
            })
            .collect();
        let rodada = Rodada::iniciar(alvos, "x".into(), Vec::new(), 2, sinal);
        esperar(&rodada);
        let r = rodada.resultados();
        assert_eq!(r.len(), 6);
        assert_eq!(r[0].estado, Estado::Ok);
        assert_eq!(r[2].estado, Estado::Falhou(Some(7)));
        assert_eq!(r[0].saida, b"saida\n!");
        assert!(r.iter().all(|x| x.duracao.is_some()));
        assert!(*simultaneos.1.lock().unwrap() <= 2, "passou do paralelo");
    }

    #[test]
    fn parar_deixa_os_que_esperam_como_erro_e_os_que_rodam_terminam() {
        let sinal = signal(0u64);
        let alvos: Vec<(u64, String, Executor)> = (1..=4)
            .map(|i| {
                (
                    i,
                    format!("s{i}"),
                    executor(Some(0), "ok", Duration::from_millis(150), None),
                )
            })
            .collect();
        let rodada = Rodada::iniciar(alvos, "x".into(), Vec::new(), 1, sinal);
        std::thread::sleep(Duration::from_millis(40));
        rodada.parar();
        esperar(&rodada);
        let r = rodada.resultados();
        assert_eq!(r[0].estado, Estado::Ok);
        assert!(
            r[1..]
                .iter()
                .all(|x| x.estado == Estado::Erro("parado".to_owned()))
        );
    }

    #[test]
    fn um_erro_do_executor_vira_estado_de_erro_com_a_mensagem() {
        let sinal = signal(0u64);
        let falha: Executor = Box::new(|_, _| Err("senha recusada".to_owned()));
        let rodada = Rodada::iniciar(
            vec![(1, "s".into(), falha)],
            "x".into(),
            Vec::new(),
            4,
            sinal,
        );
        esperar(&rodada);
        let r = rodada.resultados();
        assert_eq!(r[0].estado, Estado::Erro("senha recusada".to_owned()));
        assert_eq!(r[0].saida, b"senha recusada");
    }
}
