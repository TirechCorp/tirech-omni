//! A página Logs (M53): a lista dos logs de sessão (de texto e cifrados), o leitor com busca, e as
//! ações sobre a seleção (exportar como texto, abrir a pasta, apagar). As funções puras (listar,
//! ler com cauda, exportar, apagar) estão em [`crate::logs`].

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::{Key, Mark, Modifiers, TextEditorController, clipboard, dialogs, request_focus};

use super::Tela;
use crate::abrir::pasta_dos_logs;
use crate::andamento::Andamento;
use crate::anexos::{ErroAnexo, Progresso};
use crate::arquivos::formato;
use crate::busca::normalizar;
use crate::log_cifrado::{self, AvisoLog, ErroLog, Leitura};
use crate::logs::{self, Convertidos, EntradaLog, Lido, Sentido};
use crate::t;

/// Arquivos até este tamanho abrem na hora; os maiores abrem num [`Andamento`] com Cancelar.
const LIMITE_SEM_ANDAMENTO: u64 = 4 * 1024 * 1024;

/// O que a exportação de um log trouxe: o nome e como foi a leitura.
type Exportado = (String, Result<Leitura, ErroLog>);

/// O que a página Logs guarda enquanto o app está aberto.
pub(super) struct Estado {
    /// O filtro da lista (nome e conexão).
    filtro: Signal<String>,
    /// Os logs que a pasta e as conexões têm, do mais novo ao mais velho.
    entradas: Signal<Vec<EntradaLog>>,
    /// As linhas que o filtro deixa passar.
    linhas: Signal<Vec<DataGridRow>>,
    /// Os caminhos escolhidos na lista.
    escolhidos: Signal<BTreeSet<String>>,
    /// O log que o leitor mostra.
    aberto: RefCell<Option<PathBuf>>,
    /// Se o leitor mostra o log inteiro e não só o fim.
    inteiro: Cell<bool>,
    /// O texto do leitor.
    texto: Signal<String>,
    /// O controle do editor do leitor.
    controlador: TextEditorController,
    /// Quebrar as linhas no leitor.
    quebrar: Signal<bool>,
    /// O log aberto é cifrado e o cofre está trancado.
    trancado: Signal<bool>,
    /// O aviso da leitura: a severidade mais alta e os textos.
    aviso: Signal<Option<(InfoBarSeverity, String)>>,
    /// "Mostrando o fim do log (…)", se o texto é só o fim.
    cauda: Signal<Option<String>>,
    /// A barra de busca está à mostra.
    busca_aberta: Signal<bool>,
    /// O que se procura.
    busca: Signal<String>,
    /// Diferenciar maiúsculas na busca.
    diferenciar: Signal<bool>,
    /// Onde a busca achou.
    acertos: Signal<Vec<Range<usize>>>,
    /// O acerto atual.
    atual: Signal<usize>,
    /// As marcas do editor.
    marcas: Signal<Vec<Mark>>,
    /// A última busca feita: se a barra estava aberta, o texto procurado e se diferencia.
    ultima_busca: RefCell<(bool, String, bool)>,
    /// O diálogo de apagar está na tela.
    apagar_aberto: Signal<bool>,
    /// O que o diálogo de apagar diz: a pergunta e, se há, o aviso dos que estão gravando.
    apagar_texto: Signal<(String, String)>,
    /// A leitura grande, com Cancelar.
    lendo: Andamento<Lido>,
    /// A exportação, com Cancelar.
    exportando: Andamento<Vec<Exportado>>,
    /// Quantos logs de texto (fora os que gravam) a pasta tem: o que a InfoBar das Configurações
    /// oferece para cifrar.
    pub(super) a_cifrar: Signal<usize>,
    /// O diálogo de cifrar ou decifrar está na tela.
    converter_aberto: Signal<bool>,
    /// O que ele diz: a pergunta e, se há, o aviso dos que estão gravando.
    converter_texto: Signal<(String, String)>,
    /// O sentido do pedido: `true` cifra, `false` decifra.
    converter_cifrando: Signal<bool>,
    /// A caixa "Apagar os originais" (ou "Apagar os cifrados").
    converter_apagar: Signal<bool>,
    /// Os logs que o diálogo vai converter.
    converter_caminhos: RefCell<Vec<PathBuf>>,
    /// A conversão em lote, com Cancelar.
    convertendo: Andamento<Convertidos>,
    /// A divisão entre a lista e o leitor.
    divisor: Signal<f64>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            filtro: signal(String::new()),
            entradas: signal(Vec::new()),
            linhas: signal(Vec::new()),
            escolhidos: signal(BTreeSet::new()),
            aberto: RefCell::new(None),
            inteiro: Cell::new(false),
            texto: signal(String::new()),
            controlador: TextEditorController::new(),
            quebrar: signal(false),
            trancado: signal(false),
            aviso: signal(None),
            cauda: signal(None),
            busca_aberta: signal(false),
            busca: signal(String::new()),
            diferenciar: signal(false),
            acertos: signal(Vec::new()),
            atual: signal(0),
            marcas: signal(Vec::new()),
            ultima_busca: RefCell::new((false, String::new(), false)),
            apagar_aberto: signal(false),
            apagar_texto: signal((String::new(), String::new())),
            lendo: Andamento::nova(),
            exportando: Andamento::nova(),
            a_cifrar: signal(0),
            converter_aberto: signal(false),
            converter_texto: signal((String::new(), String::new())),
            converter_cifrando: signal(true),
            converter_apagar: signal(true),
            converter_caminhos: RefCell::new(Vec::new()),
            convertendo: Andamento::nova(),
            divisor: signal(0.36),
        }
    }
}

/// A linha da lista para um log.
fn linha_do_log(e: &EntradaLog) -> DataGridRow {
    let chave = e.caminho.display().to_string();
    let segundos = e
        .modificado
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0.0, |d| d.as_secs_f64());
    let conexao = e.conexao.clone().unwrap_or_default();
    let estado = if e.gravando {
        t!("logs-gravando")
    } else {
        String::new()
    };
    DataGridRow::new(chave)
        .cell(e.nome.clone(), CellValue::Text(e.nome.clone()))
        .cell(conexao.clone(), CellValue::Text(conexao))
        .cell(formato::data(e.modificado), CellValue::Number(segundos))
        .cell(
            formato::tamanho(e.tamanho),
            CellValue::Number(e.tamanho as f64),
        )
        .cell(estado.clone(), CellValue::Text(estado))
        .icon(if e.cifrado {
            Symbol::LockClosed
        } else {
            Symbol::DocumentText
        })
}

/// A severidade mais alta das duas.
fn pior(a: InfoBarSeverity, b: InfoBarSeverity) -> InfoBarSeverity {
    let peso = |s: InfoBarSeverity| match s {
        InfoBarSeverity::Error => 3,
        InfoBarSeverity::Warning => 2,
        InfoBarSeverity::Informational => 1,
        InfoBarSeverity::Success => 0,
    };
    if peso(b) > peso(a) { b } else { a }
}

impl Tela {
    /// A pasta dos logs com a configuração de agora.
    fn pasta_dos_logs_agora(&self) -> PathBuf {
        self.config
            .with_untracked(|c| pasta_dos_logs(c, self.armazenamento.pasta()))
    }

    /// Lê a pasta dos logs e as conexões de novo e refaz a lista.
    pub(super) fn atualizar_logs(&self) {
        let pasta = self.pasta_dos_logs_agora();
        let conexoes: Vec<_> = self
            .tree
            .with_untracked(|t| t.connections().into_iter().cloned().collect());
        let lista = logs::listar(&pasta, &conexoes, &logs::ativos());
        // O que sumiu da pasta sai da seleção.
        let existentes: BTreeSet<String> = lista
            .iter()
            .map(|e| e.caminho.display().to_string())
            .collect();
        let e = &self.logs_pagina;
        let escolhidos = e.escolhidos.get_untracked();
        if escolhidos.iter().any(|c| !existentes.contains(c)) {
            e.escolhidos
                .set(escolhidos.intersection(&existentes).cloned().collect());
        }
        e.a_cifrar
            .set(lista.iter().filter(|l| !l.cifrado && !l.gravando).count());
        e.entradas.set(lista);
    }

    /// As entradas escolhidas na lista, na ordem dela.
    fn logs_escolhidos(&self) -> Vec<EntradaLog> {
        let e = &self.logs_pagina;
        let escolhidos = e.escolhidos.get_untracked();
        e.entradas.with_untracked(|todas| {
            todas
                .iter()
                .filter(|l| escolhidos.contains(&l.caminho.display().to_string()))
                .cloned()
                .collect()
        })
    }

    /// Esvazia o leitor.
    fn fechar_log(&self) {
        let e = &self.logs_pagina;
        *e.aberto.borrow_mut() = None;
        e.inteiro.set(false);
        e.texto.set(String::new());
        e.trancado.set(false);
        e.aviso.set(None);
        e.cauda.set(None);
    }

    /// A lista mudou de seleção: com um log só, ele abre; com outra quantidade, o leitor esvazia.
    fn ao_escolher_log(&self, escolhidos: &BTreeSet<String>) {
        let e = &self.logs_pagina;
        if escolhidos.len() == 1 {
            let caminho = PathBuf::from(escolhidos.iter().next().expect("um escolhido"));
            if e.aberto.borrow().as_deref() != Some(caminho.as_path()) {
                self.abrir_log(caminho, false);
            }
        } else {
            self.fechar_log();
        }
    }

    /// Seleciona `caminho` na lista e abre. Vai para a página Logs.
    pub(super) fn mostrar_log(&self, caminho: &Path) {
        self.ir_para("logs");
        self.atualizar_logs();
        let e = &self.logs_pagina;
        e.filtro.set(String::new());
        e.escolhidos
            .set(BTreeSet::from([caminho.display().to_string()]));
    }

    /// O leitor relê o log que está aberto (depois de destravar o cofre, do Atualizar).
    fn reabrir_log(&self) {
        let aberto = self.logs_pagina.aberto.borrow().clone();
        if let Some(caminho) = aberto {
            let inteiro = self.logs_pagina.inteiro.get();
            self.abrir_log(caminho, inteiro);
        }
    }

    /// Abre `caminho` no leitor; `inteiro` mostra o log todo e não só o fim.
    fn abrir_log(&self, caminho: PathBuf, inteiro: bool) {
        let e = &self.logs_pagina;
        *e.aberto.borrow_mut() = Some(caminho.clone());
        e.inteiro.set(inteiro);
        e.aviso.set(None);
        e.cauda.set(None);
        let cifrado = caminho
            .to_string_lossy()
            .ends_with(crate::abrir::SUFIXO_CIFRADO)
            || log_cifrado::e_cifrado(&caminho);
        if cifrado && self.cofre.agora(|c| c.bloqueado()) {
            e.trancado.set(true);
            e.texto.set(String::new());
            return;
        }
        e.trancado.set(false);
        let chaves = self.cofre.agora(logs::mapa_de_chaves);
        let maximo = if inteiro {
            0
        } else {
            self.config
                .with_untracked(|c| u64::from(c.terminal.logs.visualizar_mib))
                * 1024
                * 1024
        };
        let tamanho = std::fs::metadata(&caminho).map_or(0, |m| m.len());
        if tamanho <= LIMITE_SEM_ANDAMENTO {
            let lido = logs::ler_para_ver(
                &caminho,
                &chaves,
                maximo,
                &std::sync::atomic::AtomicBool::new(false),
                &|_, _| {},
            );
            self.mostrar_lido(&caminho, lido);
            return;
        }
        let nome = caminho
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        e.texto.set(String::new());
        let eu = self.eu.clone();
        let origem = caminho.clone();
        e.lendo.iniciar(
            t!("logs-lendo", nome = nome),
            move |progresso, cancelar| {
                let lido =
                    logs::ler_para_ver(&origem, &chaves, maximo, cancelar, &|feito, total| {
                        progresso(Progresso { feito, total });
                    });
                if matches!(lido.erro, Some(ErroLog::Cancelado)) {
                    Err(ErroAnexo::Cancelado)
                } else {
                    Ok(lido)
                }
            },
            move |resultado| {
                let Some(tela) = eu.upgrade() else {
                    return;
                };
                match resultado {
                    Ok(lido) => tela.mostrar_lido(&caminho, lido),
                    Err(_) => tela.logs_pagina.aviso.set(Some((
                        InfoBarSeverity::Warning,
                        t!("logs-leitura-cancelada"),
                    ))),
                }
            },
        );
    }

    /// Põe no leitor o que a leitura de `caminho` trouxe, com os avisos. Se o leitor já mostra
    /// outro log, joga fora.
    fn mostrar_lido(&self, caminho: &Path, lido: Lido) {
        let e = &self.logs_pagina;
        if e.aberto.borrow().as_deref() != Some(caminho) {
            return;
        }
        let gravando = logs::em_uso(caminho);
        let mut avisos: Vec<(InfoBarSeverity, String)> = Vec::new();
        for aviso in &lido.leitura.avisos {
            match aviso {
                AvisoLog::SemFim { .. } if !gravando => {
                    avisos.push((InfoBarSeverity::Informational, t!("logs-sem-fim")));
                }
                AvisoLog::SemFim { .. } => {}
                AvisoLog::Cortado { bytes } => avisos.push((
                    InfoBarSeverity::Warning,
                    t!("logs-cortado", bytes = *bytes as i64),
                )),
            }
        }
        if let Some(erro) = &lido.erro {
            avisos.push((InfoBarSeverity::Error, erro.to_string()));
        }
        e.aviso.set(
            avisos
                .into_iter()
                .reduce(|(sa, ta), (sb, tb)| (pior(sa, sb), format!("{ta} {tb}"))),
        );
        e.cauda.set(lido.so_o_fim.then(|| {
            t!(
                "logs-mostrando-o-fim",
                mostrado = formato::tamanho(lido.texto.len() as u64),
                total = formato::tamanho(lido.total)
            )
        }));
        // O fim é o que se quer ver num log: rola até o começo da última linha.
        let ultima_linha = lido
            .texto
            .trim_end_matches('\n')
            .rfind('\n')
            .map_or(0, |i| i + 1);
        e.texto.set(lido.texto);
        e.controlador.scroll_to(ultima_linha);
    }

    // --- Busca no log ---

    fn abrir_busca_do_log(&self) {
        if self.logs_pagina.aberto.borrow().is_none() || self.logs_pagina.trancado.get_untracked() {
            return;
        }
        self.logs_pagina.busca_aberta.set(true);
        request_focus("logs-busca");
    }

    fn fechar_busca_do_log(&self) {
        self.logs_pagina.busca_aberta.set(false);
        self.logs_pagina.controlador.focus();
    }

    /// Vai ao acerto `indice`: seleciona e rola até ele.
    fn ir_para_acerto(&self, indice: usize) {
        let e = &self.logs_pagina;
        let Some(faixa) = e.acertos.get_untracked().get(indice).cloned() else {
            return;
        };
        e.atual.set(indice);
        e.controlador.set_selection(faixa.clone());
        e.controlador.scroll_to(faixa.start);
    }

    /// Vai ao acerto seguinte (`+1`) ou anterior (`-1`), dando a volta.
    fn passar_acerto(&self, passo: isize) {
        let e = &self.logs_pagina;
        let n = e.acertos.get_untracked().len();
        if n == 0 {
            return;
        }
        let atual = e.atual.get_untracked() as isize;
        self.ir_para_acerto((atual + passo).rem_euclid(n as isize) as usize);
    }

    /// Refaz os acertos e as marcas. Quando o que se procura mudou, vai ao primeiro acerto.
    fn refazer_busca_do_log(&self) {
        let e = &self.logs_pagina;
        let aberta = e.busca_aberta.get();
        let termo = e.busca.get();
        let diferenciar = e.diferenciar.get();
        let _ = e.texto.get();
        let atual = e.atual.get();
        untrack(|| {
            if !aberta || termo.is_empty() {
                e.acertos.set(Vec::new());
                e.marcas.set(Vec::new());
                *e.ultima_busca.borrow_mut() = (aberta, termo, diferenciar);
                return;
            }
            let achados = e.controlador.find(&termo, !diferenciar);
            let nova = (aberta, termo, diferenciar);
            let mudou = *e.ultima_busca.borrow() != nova;
            *e.ultima_busca.borrow_mut() = nova;
            let indice = if achados.is_empty() {
                0
            } else if mudou {
                let cursor = e.controlador.selection().start;
                achados.iter().position(|r| r.start >= cursor).unwrap_or(0)
            } else {
                atual.min(achados.len() - 1)
            };
            e.marcas.set(
                achados
                    .iter()
                    .enumerate()
                    .map(|(i, r)| Mark {
                        range: r.clone(),
                        current: i == indice,
                    })
                    .collect(),
            );
            e.acertos.set(achados.clone());
            if mudou && !achados.is_empty() {
                e.atual.set(indice);
                let faixa = achados[indice].clone();
                e.controlador.set_selection(faixa.clone());
                e.controlador.scroll_to(faixa.start);
            } else if e.atual.get_untracked() != indice {
                e.atual.set(indice);
            }
        });
    }

    // --- Ações ---

    /// "Atualizar": sela os logs que estão sendo gravados, relê a pasta e o log aberto.
    fn atualizar_pagina_de_logs(&self) {
        self.selar_logs();
        self.atualizar_logs();
        self.reabrir_log();
    }

    /// "Abrir a pasta": a do log escolhido, ou a pasta dos logs.
    fn abrir_pasta_dos_logs(&self) {
        let destino = self
            .logs_escolhidos()
            .first()
            .and_then(|l| l.caminho.parent().map(Path::to_owned))
            .unwrap_or_else(|| self.pasta_dos_logs_agora());
        if let Err(erro) = std::fs::create_dir_all(&destino) {
            self.avisar(
                InfoBarSeverity::Error,
                t!("logs-sem-pasta-de-logs", erro = erro.to_string()),
            );
            return;
        }
        if let Err(erro) = crate::ferramentas::abrir_pasta(&destino) {
            self.avisar(
                InfoBarSeverity::Error,
                t!(
                    "logs-nao-abriu-a-pasta",
                    caminho = destino.display().to_string(),
                    motivo = erro.to_string()
                ),
            );
        }
    }

    /// "Exportar como texto…": um log pede o arquivo; vários pedem a pasta.
    fn exportar_logs(&self) {
        let escolhidos = self.logs_escolhidos();
        let Some(primeiro) = escolhidos.first() else {
            return;
        };
        let eu = self.eu.clone();
        if escolhidos.len() == 1 {
            let origem = primeiro.caminho.clone();
            dialogs::save_file()
                .remember("logs-exportar")
                .title(t!("logs-exportar-titulo"))
                .file_name(logs::nome_exportado(&primeiro.nome))
                .show(move |destino| {
                    let (Some(destino), Some(tela)) = (destino, eu.upgrade()) else {
                        return;
                    };
                    tela.exportar_logs_para(vec![(origem, destino)]);
                });
        } else {
            dialogs::pick_folder()
                .remember("logs-exportar")
                .title(t!("logs-exportar-pasta-titulo"))
                .show(move |pasta| {
                    let (Some(pasta), Some(tela)) = (pasta, eu.upgrade()) else {
                        return;
                    };
                    // Os nomes que se repetem dentro do lote também ganham `(2)`.
                    let mut pares: Vec<(PathBuf, PathBuf)> = Vec::new();
                    for e in tela.logs_escolhidos() {
                        let nome = logs::nome_exportado(&e.nome);
                        let mut destino = logs::sem_repetir(&pasta, &nome);
                        let mut n = 2;
                        while pares.iter().any(|(_, d)| *d == destino) {
                            let (radical, ext) = match nome.rfind('.') {
                                Some(i) if i > 0 => nome.split_at(i),
                                _ => (nome.as_str(), ""),
                            };
                            destino = pasta.join(format!("{radical} ({n}){ext}"));
                            n += 1;
                        }
                        pares.push((e.caminho.clone(), destino));
                    }
                    tela.exportar_logs_para(pares);
                });
        }
    }

    /// Exporta cada `(origem, destino)` num [`Andamento`] e diz como foi.
    fn exportar_logs_para(&self, pares: Vec<(PathBuf, PathBuf)>) {
        let chaves = self.cofre.agora(logs::mapa_de_chaves);
        let total: u64 = pares
            .iter()
            .map(|(o, _)| std::fs::metadata(o).map_or(0, |m| m.len()))
            .sum();
        let eu = self.eu.clone();
        let destinos: Vec<PathBuf> = pares.iter().map(|(_, d)| d.clone()).collect();
        self.logs_pagina.exportando.iniciar(
            t!("logs-exportar-titulo"),
            move |progresso, cancelar| {
                let mut feito = 0u64;
                let mut saida: Vec<Exportado> = Vec::new();
                for (origem, destino) in &pares {
                    let nome = origem
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let r = logs::exportar_texto(origem, destino, &chaves, cancelar);
                    if matches!(r, Err(ErroLog::Cancelado)) {
                        return Err(ErroAnexo::Cancelado);
                    }
                    feito += std::fs::metadata(origem).map_or(0, |m| m.len());
                    progresso(Progresso { feito, total });
                    saida.push((nome, r));
                }
                Ok(saida)
            },
            move |resultado| {
                let Some(tela) = eu.upgrade() else {
                    return;
                };
                if let Ok(saida) = resultado {
                    tela.resumir_exportacao(&saida, &destinos);
                }
            },
        );
    }

    /// Diz na tela como foi a exportação: sucesso, com avisos ou o erro.
    fn resumir_exportacao(&self, saida: &[Exportado], destinos: &[PathBuf]) {
        let mut avisos: Vec<String> = Vec::new();
        let mut feitos = 0usize;
        for (nome, r) in saida {
            match r {
                Ok(leitura) => {
                    feitos += 1;
                    for a in &leitura.avisos {
                        match a {
                            AvisoLog::SemFim { .. } => {}
                            AvisoLog::Cortado { bytes } => avisos.push(format!(
                                "{nome}: {}",
                                t!("logs-cortado", bytes = *bytes as i64)
                            )),
                        }
                    }
                }
                Err(erro) => {
                    self.avisar(
                        InfoBarSeverity::Error,
                        t!(
                            "logs-nao-exportou",
                            nome = nome.as_str(),
                            erro = erro.to_string()
                        ),
                    );
                    return;
                }
            }
        }
        if !avisos.is_empty() {
            self.avisar(
                InfoBarSeverity::Warning,
                t!(
                    "logs-exportados-com-avisos",
                    n = feitos as i64,
                    avisos = avisos.join(" ")
                ),
            );
        } else if let [unico] = destinos {
            self.avisar(
                InfoBarSeverity::Success,
                t!("logs-exportado-em", arquivo = unico.display().to_string()),
            );
        } else {
            let pasta = destinos
                .first()
                .and_then(|d| d.parent())
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            self.avisar(
                InfoBarSeverity::Success,
                t!("logs-exportados-em", n = feitos as i64, pasta = pasta),
            );
        }
    }

    /// "Apagar": pergunta antes, e deixa de fora o que está sendo gravado.
    fn pedir_para_apagar_logs(&self) {
        let escolhidos = self.logs_escolhidos();
        if escolhidos.is_empty() {
            return;
        }
        let gravando = escolhidos.iter().filter(|l| l.gravando).count();
        if gravando == escolhidos.len() {
            self.avisar(InfoBarSeverity::Warning, t!("logs-so-gravando"));
            return;
        }
        let a_apagar = escolhidos.len() - gravando;
        let aviso = if gravando > 0 {
            t!("logs-apagar-gravando", n = gravando as i64)
        } else {
            String::new()
        };
        self.logs_pagina
            .apagar_texto
            .set((t!("logs-apagar-texto", n = a_apagar as i64), aviso));
        self.logs_pagina.apagar_aberto.set(true);
    }

    /// Apaga de verdade o que a pessoa confirmou.
    fn apagar_logs(&self) {
        let caminhos: Vec<PathBuf> = self
            .logs_escolhidos()
            .into_iter()
            .map(|l| l.caminho)
            .collect();
        let r = logs::apagar(&caminhos, &logs::ativos());
        if let Some((caminho, erro)) = r.erros.first() {
            self.avisar(
                InfoBarSeverity::Error,
                t!(
                    "logs-nao-apagou",
                    nome = caminho.display().to_string(),
                    erro = erro.to_string()
                ),
            );
        } else {
            self.avisar(
                InfoBarSeverity::Success,
                t!("logs-apagados", n = r.apagados as i64),
            );
        }
        self.logs_pagina.escolhidos.set(BTreeSet::new());
        self.atualizar_logs();
    }

    /// "Cifrar" e "Decifrar": pergunta antes, só com os logs que cabem no sentido. `todos` olha a
    /// lista inteira e não só a seleção.
    pub(super) fn pedir_para_converter_logs(&self, sentido: Sentido, todos: bool) {
        let e = &self.logs_pagina;
        let candidatos = if todos {
            // A lista pode estar velha (a página Logs nem foi aberta).
            self.atualizar_logs();
            e.entradas.get_untracked()
        } else {
            self.logs_escolhidos()
        };
        let cifrando = sentido == Sentido::Cifrar;
        // Só converte o que ainda não está no formato pedido.
        let no_formato: Vec<&EntradaLog> = candidatos
            .iter()
            .filter(|l| l.cifrado != cifrando)
            .collect();
        let gravando = no_formato.iter().filter(|l| l.gravando).count();
        let a_converter: Vec<&EntradaLog> =
            no_formato.iter().copied().filter(|l| !l.gravando).collect();
        if a_converter.is_empty() {
            let texto = match (cifrando, gravando > 0) {
                (_, true) => t!("logs-converter-so-gravando"),
                (true, false) => t!("logs-converter-nada-cifrar"),
                (false, false) => t!("logs-converter-nada-decifrar"),
            };
            self.avisar(InfoBarSeverity::Warning, texto);
            return;
        }
        let n = a_converter.len() as i64;
        let tamanho = formato::tamanho(a_converter.iter().map(|l| l.tamanho).sum());
        let pergunta = if cifrando {
            t!("logs-converter-cifrar-texto", n = n, tamanho = tamanho)
        } else {
            t!("logs-converter-decifrar-texto", n = n, tamanho = tamanho)
        };
        let aviso = if gravando > 0 {
            t!("logs-apagar-gravando", n = gravando as i64)
        } else {
            String::new()
        };
        *e.converter_caminhos.borrow_mut() =
            a_converter.iter().map(|l| l.caminho.clone()).collect();
        e.converter_texto.set((pergunta, aviso));
        e.converter_cifrando.set(cifrando);
        // Cifrar apaga o texto por padrão (senão sobra o claro ao lado do cifrado); decifrar guarda
        // o cifrado.
        e.converter_apagar.set(cifrando);
        e.converter_aberto.set(true);
    }

    /// O "Converter" do diálogo: precisa do cofre aberto, para a chave dos logs.
    fn converter_logs(&self) {
        let e = &self.logs_pagina;
        let sentido = if e.converter_cifrando.get_untracked() {
            Sentido::Cifrar
        } else {
            Sentido::Decifrar
        };
        let caminhos = e.converter_caminhos.borrow().clone();
        let apagar = e.converter_apagar.get_untracked();
        self.com_cofre_aberto(move |t| t.converter_logs_agora(sentido, caminhos, apagar));
    }

    /// Converte `caminhos` num [`Andamento`] e diz como foi.
    fn converter_logs_agora(&self, sentido: Sentido, caminhos: Vec<PathBuf>, apagar: bool) {
        let publica = if sentido == Sentido::Cifrar {
            match self.cofre.mudar(logs::garantir_chave) {
                Ok(p) => Some(p.publica),
                Err(erro) => {
                    self.avisar(
                        InfoBarSeverity::Error,
                        t!("log-nao-criou-a-chave", erro = erro.to_string()),
                    );
                    return;
                }
            }
        } else {
            None
        };
        let chaves = self.cofre.agora(logs::mapa_de_chaves);
        // O registro dos que gravam é da thread da interface: a conversão leva a lista pronta.
        let ativos = logs::ativos();
        // Os bytes de antes de cada item, para a barra andar por tamanho.
        let mut antes = Vec::with_capacity(caminhos.len() + 1);
        let mut total = 0u64;
        for c in &caminhos {
            antes.push(total);
            total += std::fs::metadata(c).map_or(0, |m| m.len());
        }
        antes.push(total);
        let eu = self.eu.clone();
        self.logs_pagina.convertendo.iniciar(
            t!("logs-convertendo"),
            move |progresso, cancelar| {
                let r = logs::converter_lote(
                    &caminhos,
                    sentido,
                    publica.as_ref(),
                    &chaves,
                    apagar,
                    &ativos,
                    cancelar,
                    &|i, _, _| {
                        progresso(Progresso {
                            feito: antes[i],
                            total,
                        })
                    },
                );
                progresso(Progresso {
                    feito: total,
                    total,
                });
                Ok(r)
            },
            move |resultado| {
                let Some(tela) = eu.upgrade() else {
                    return;
                };
                if let Ok(r) = resultado {
                    tela.resumir_conversao(sentido, &r);
                }
            },
        );
    }

    /// Diz na tela como foi a conversão e refaz a lista.
    fn resumir_conversao(&self, sentido: Sentido, r: &Convertidos) {
        let n = r.convertidos as i64;
        let mut partes = vec![if sentido == Sentido::Cifrar {
            t!("logs-cifrados-n", n = n)
        } else {
            t!("logs-decifrados-n", n = n)
        }];
        if r.gravando > 0 {
            partes.push(t!("logs-convertidos-gravando", n = r.gravando as i64));
        }
        if !r.erros.is_empty() {
            let detalhes: Vec<String> = r
                .erros
                .iter()
                .map(|(c, erro)| {
                    let nome = c.file_name().map(|n| n.to_string_lossy().into_owned());
                    format!("{}: {erro}", nome.unwrap_or_default())
                })
                .collect();
            partes.push(t!(
                "logs-convertidos-erros",
                n = r.erros.len() as i64,
                detalhes = detalhes.join("; ")
            ));
        }
        for (c, aviso) in &r.avisos {
            if let AvisoLog::Cortado { bytes } = aviso {
                let nome = c.file_name().map(|n| n.to_string_lossy().into_owned());
                partes.push(format!(
                    "{}: {}",
                    nome.unwrap_or_default(),
                    t!("logs-cortado", bytes = *bytes as i64)
                ));
            }
        }
        if r.cancelado {
            partes.push(t!("logs-convertidos-cancelado"));
        }
        let severidade = if !r.erros.is_empty() {
            InfoBarSeverity::Error
        } else if r.cancelado || r.gravando > 0 || !r.avisos.is_empty() {
            InfoBarSeverity::Warning
        } else {
            InfoBarSeverity::Success
        };
        self.avisar(severidade, partes.join(" "));
        self.logs_pagina.escolhidos.set(BTreeSet::new());
        self.atualizar_logs();
        // O log que o leitor mostra pode ter saído (cifrar apagando o texto).
        let sumiu = self
            .logs_pagina
            .aberto
            .borrow()
            .as_deref()
            .is_some_and(|c| !c.exists());
        if sumiu {
            self.fechar_log();
        }
    }

    /// Limpa da memória o que foi decifrado: o texto, a busca e as marcas. O cofre vai trancar.
    pub(super) fn esquecer_log_decifrado(&self) {
        let e = &self.logs_pagina;
        let aberto = e.aberto.borrow().clone();
        let cifrado = aberto.as_deref().is_some_and(|c| {
            c.to_string_lossy().ends_with(crate::abrir::SUFIXO_CIFRADO) || log_cifrado::e_cifrado(c)
        });
        // Uma leitura em andamento não deixa nada para trás.
        (e.lendo.gancho_de_cancelar())();
        if cifrado {
            e.texto.set(String::new());
            e.acertos.set(Vec::new());
            e.marcas.set(Vec::new());
            e.aviso.set(None);
            e.cauda.set(None);
            e.busca.set(String::new());
            e.busca_aberta.set(false);
            e.trancado.set(true);
        }
    }
}

/// A página Logs, na janela que mostra a página `pagina_atual`.
pub(super) fn pagina(tela: &Rc<Tela>, pagina_atual: Signal<String>) -> impl View + use<> {
    let e = &tela.logs_pagina;
    let (filtro, entradas, linhas, escolhidos) = (e.filtro, e.entradas, e.linhas, e.escolhidos);
    let (texto, quebrar, trancado, aviso, cauda) =
        (e.texto, e.quebrar, e.trancado, e.aviso, e.cauda);
    let (busca_aberta, busca, diferenciar) = (e.busca_aberta, e.busca, e.diferenciar);
    let (acertos, atual, marcas) = (e.acertos, e.atual, e.marcas);
    let (apagar_aberto, apagar_texto) = (e.apagar_aberto, e.apagar_texto);

    // A lista se refaz ao abrir a página.
    {
        let t = Rc::clone(tela);
        effect(move || {
            if pagina_atual.get() == "logs" {
                untrack(|| t.atualizar_logs());
            }
        });
    }
    // As linhas seguem as entradas e o filtro.
    effect(move || {
        let procurado = normalizar(filtro.get().trim());
        let novas: Vec<DataGridRow> = entradas.with(|todas| {
            todas
                .iter()
                .filter(|l| {
                    procurado.is_empty()
                        || normalizar(&l.nome).contains(&procurado)
                        || l.conexao
                            .as_deref()
                            .is_some_and(|c| normalizar(c).contains(&procurado))
                })
                .map(linha_do_log)
                .collect()
        });
        linhas.set(novas);
    });
    // Escolher um log abre; escolher outra quantidade esvazia o leitor.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let sel = escolhidos.get();
            untrack(|| t.ao_escolher_log(&sel));
        });
    }
    // Destravou o cofre com um cifrado esperando: abre.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let _ = t.cofre.versao();
            untrack(|| {
                if trancado.get_untracked() && !t.cofre.agora(|c| c.bloqueado()) {
                    t.reabrir_log();
                }
            });
        });
    }
    // Vai trancar: o texto decifrado sai da memória.
    {
        let fraca = Rc::downgrade(tela);
        tela.cofre.antes_de_bloquear(move || {
            if let Some(t) = fraca.upgrade() {
                t.esquecer_log_decifrado();
            }
        });
    }
    // A busca acompanha o que se digita, o texto e a barra.
    {
        let t = Rc::clone(tela);
        effect(move || {
            t.refazer_busca_do_log();
        });
    }

    // --- A lista ---
    let filtro_campo = Edit::bind(filtro)
        .id("logs-filtro")
        .placeholder(t!("comum-filtrar"));
    let grade = DataGrid::bind(linhas, escolhidos)
        .id("logs-lista")
        .grow(1)
        .columns([
            DataGridColumn::new("nome", t!("comum-nome")).width(ColumnWidth::Star(1.4)),
            DataGridColumn::new("conexao", t!("logs-conexao")).width(ColumnWidth::Star(0.8)),
            DataGridColumn::new("modificado", t!("logs-modificado"))
                .width(ColumnWidth::Pixels(148.0)),
            DataGridColumn::new("tamanho", t!("comum-tamanho")).width(ColumnWidth::Pixels(92.0)),
            DataGridColumn::new("estado", t!("comum-estado")).width(ColumnWidth::Pixels(80.0)),
        ])
        .empty_text(t!("logs-vazio"));

    let sem_escolha = move || escolhidos.with(BTreeSet::is_empty);
    let acoes = {
        let (atualizar, exportar, pasta, apagar) = (
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
        );
        Row::new()
            .gap(8)
            .child(
                Button::icon_only(Symbol::ArrowSync)
                    .id("logs-atualizar")
                    .label(t!("comum-atualizar"))
                    .tooltip(t!("logs-atualizar-dica"))
                    .on_click(move || atualizar.atualizar_pagina_de_logs()),
            )
            .child(
                Button::new(t!("logs-exportar"))
                    .id("logs-exportar")
                    .icon(Symbol::ArrowDownload)
                    .disabled(sem_escolha)
                    .on_click(move || exportar.exportar_logs()),
            )
            .child(
                Button::icon_only(Symbol::LockClosed)
                    .id("logs-cifragem")
                    .label(t!("logs-cifragem"))
                    .tooltip(t!("logs-cifragem"))
                    .flyout(menu_de_cifragem(tela, sem_escolha)),
            )
            .child(
                Button::icon_only(Symbol::FolderOpen)
                    .id("logs-abrir-pasta")
                    .label(t!("logs-abrir-pasta"))
                    .tooltip(t!("logs-abrir-pasta"))
                    .on_click(move || pasta.abrir_pasta_dos_logs()),
            )
            .child(
                Button::new(t!("comum-apagar"))
                    .id("logs-apagar")
                    .icon(Symbol::Delete)
                    .disabled(sem_escolha)
                    .on_click(move || apagar.pedir_para_apagar_logs()),
            )
    };
    let esquerda = Column::new()
        .grow(1)
        .gap(8)
        .child(filtro_campo)
        .child(acoes)
        .child(grade);

    // --- O leitor ---
    let barra_de_busca = {
        let (anterior, proxima, fechar) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        let (a_prox, a_ant, a_fechar, a_enter, a_shift_enter) = (
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
        );
        let contagem = move || {
            let n = acertos.get().len();
            if busca.get().is_empty() {
                String::new()
            } else if n == 0 {
                t!("notas-nenhum-resultado")
            } else {
                t!(
                    "notas-resultado-n-de-m",
                    atual = atual.get() as i64 + 1,
                    total = n as i64
                )
            }
        };
        let sem_acertos = move || acertos.get().is_empty();
        Row::new()
            .id("logs-busca-barra")
            .gap(4)
            .align(Align::Center)
            .visible(move || busca_aberta.get())
            .accelerator(Shortcut::key(Key::Enter), move || a_enter.passar_acerto(1))
            .accelerator(Shortcut::new(Key::Enter, Modifiers::SHIFT), move || {
                a_shift_enter.passar_acerto(-1)
            })
            .accelerator(Shortcut::key(Key::F(3)), move || a_prox.passar_acerto(1))
            .accelerator(Shortcut::new(Key::F(3), Modifiers::SHIFT), move || {
                a_ant.passar_acerto(-1)
            })
            .accelerator(Shortcut::key(Key::Escape), move || {
                a_fechar.fechar_busca_do_log()
            })
            .child(
                Edit::bind(busca)
                    .id("logs-busca")
                    .placeholder(t!("logs-buscar"))
                    .width(220),
            )
            .child(CheckBox::bind(diferenciar, t!("logs-diferenciar")).id("logs-diferenciar"))
            .child(Text::new(contagem).id("logs-busca-contagem").secondary())
            .child(
                Button::icon_only(Symbol::ChevronUp)
                    .id("logs-busca-anterior")
                    .label(t!("notas-resultado-anterior"))
                    .tooltip(t!("notas-anterior-shift-f3"))
                    .disabled(sem_acertos)
                    .on_click(move || anterior.passar_acerto(-1)),
            )
            .child(
                Button::icon_only(Symbol::ChevronDown)
                    .id("logs-busca-proxima")
                    .label(t!("notas-proximo-resultado"))
                    .tooltip(t!("notas-proximo-f3"))
                    .disabled(sem_acertos)
                    .on_click(move || proxima.passar_acerto(1)),
            )
            .child(
                Button::icon_only(Symbol::Dismiss)
                    .id("logs-busca-fechar")
                    .label(t!("notas-fechar-a-busca"))
                    .tooltip(t!("notas-fechar-esc"))
                    .on_click(move || fechar.fechar_busca_do_log()),
            )
    };

    let ferramentas_do_leitor = {
        let (busca_botao, copiar) = (Rc::clone(tela), Rc::clone(tela));
        let aberto_e_destravado = {
            let t = Rc::clone(tela);
            move || {
                let _ = texto.get();
                trancado.get() || t.logs_pagina.aberto.borrow().is_none()
            }
        };
        Row::new()
            .gap(8)
            .align(Align::Center)
            .child(
                Button::icon_only(Symbol::Search)
                    .id("logs-buscar")
                    .label(t!("logs-buscar"))
                    .tooltip(t!("logs-buscar-dica"))
                    .disabled(aberto_e_destravado.clone())
                    .on_click(move || busca_botao.abrir_busca_do_log()),
            )
            .child(ToggleButton::bind(quebrar, t!("logs-quebrar-linhas")).id("logs-quebrar"))
            .child(
                Button::new(t!("logs-copiar-tudo"))
                    .id("logs-copiar-tudo")
                    .icon(Symbol::Copy)
                    .disabled(aberto_e_destravado)
                    .on_click(move || {
                        clipboard::write(copiar.logs_pagina.texto.get_untracked());
                    }),
            )
    };

    // O InfoBar tem uma severidade só: um por severidade, como a tela faz com os avisos dela.
    let avisos_da_leitura = Column::new()
        .gap(4)
        .child(barra_de_leitura(
            "logs-aviso-erro",
            InfoBarSeverity::Error,
            aviso,
        ))
        .child(barra_de_leitura(
            "logs-aviso-atencao",
            InfoBarSeverity::Warning,
            aviso,
        ))
        .child(barra_de_leitura(
            "logs-aviso-info",
            InfoBarSeverity::Informational,
            aviso,
        ));

    let aviso_da_cauda = {
        let tudo = Rc::clone(tela);
        Row::new()
            .gap(8)
            .align(Align::Center)
            .visible(move || cauda.get().is_some())
            .child(
                InfoBar::new(InfoBarSeverity::Informational)
                    .id("logs-cauda")
                    .message(move || cauda.get().unwrap_or_default())
                    .grow(1),
            )
            .child(
                Button::new(t!("logs-mostrar-tudo"))
                    .id("logs-mostrar-tudo")
                    .on_click(move || {
                        let aberto = tudo.logs_pagina.aberto.borrow().clone();
                        if let Some(caminho) = aberto {
                            tudo.abrir_log(caminho, true);
                        }
                    }),
            )
    };

    let editor = {
        let controlador = tela.logs_pagina.controlador.clone();
        For::each(
            move || vec![quebrar.get()],
            |k: &bool| *k,
            move |quebrar_linhas: bool| {
                TextEditor::bind(texto)
                    .id("logs-texto")
                    .read_only(true)
                    .monospace()
                    .wrap(quebrar_linhas)
                    .marks(marcas)
                    .controller(controlador.clone())
            },
        )
    };

    let painel_trancado = {
        let t = Rc::clone(tela);
        Column::new()
            .id("logs-trancado")
            .visible(move || trancado.get())
            .grow(1)
            .align(Align::Center)
            .justify(Justify::Center)
            .gap(8)
            .child(Icon::new(Symbol::LockClosed).size(24).decorative())
            .child(
                Text::new(t!("logs-destrave"))
                    .id("logs-destrave")
                    .subtitle(),
            )
            .child(
                Button::new(t!("logs-destravar"))
                    .id("logs-destravar")
                    .accent()
                    .on_click(move || t.com_cofre_aberto(Tela::reabrir_log)),
            )
    };
    let sem_log = {
        let t = Rc::clone(tela);
        Column::new()
            .id("logs-sem-log")
            .grow(1)
            .align(Align::Center)
            .justify(Justify::Center)
            .child(
                Text::new(move || {
                    let n = escolhidos.with(BTreeSet::len);
                    if n > 1 {
                        t!("logs-varios", n = n as i64)
                    } else {
                        t!("logs-escolha-um")
                    }
                })
                .id("logs-escolha-um")
                .secondary(),
            )
            .visible({
                move || {
                    let _ = t.cofre.versao();
                    escolhidos.with(BTreeSet::len) != 1
                }
            })
    };

    let leitor = {
        let t = Rc::clone(tela);
        let aberto_mostrado = move || escolhidos.with(BTreeSet::len) == 1 && !trancado.get();
        Column::new()
            .grow(1)
            .gap(8)
            .accelerator(Shortcut::new(Key::Char('f'), Modifiers::CTRL), move || {
                t.abrir_busca_do_log()
            })
            .child(sem_log)
            .child(painel_trancado)
            .child(
                Column::new()
                    .grow(1)
                    .gap(8)
                    .visible(aberto_mostrado)
                    .child(ferramentas_do_leitor)
                    .child(avisos_da_leitura)
                    .child(aviso_da_cauda)
                    .child(barra_de_busca)
                    .child(editor),
            )
    };

    let corpo = Split::column(esquerda, leitor)
        .id("logs-divisor")
        .size(e.divisor)
        .min_first(190.0)
        .min_second(220.0)
        .grow(1);

    Column::new()
        .id("logs-pagina")
        .grow(1)
        .gap(12)
        .padding(16)
        .child(Text::new(t!("logs-titulo")).title().id("logs-titulo"))
        .child(Text::new(t!("logs-descricao")).caption().wrap())
        .child(corpo)
        .child(dialogo_de_apagar(tela, apagar_aberto, apagar_texto))
        .child(dialogo_de_converter(tela))
        .child(
            tela.logs_pagina
                .lendo
                .dialogo(t!("logs-titulo"), "logs-lendo"),
        )
        .child(
            tela.logs_pagina
                .exportando
                .dialogo(t!("logs-exportar-titulo"), "logs-exportando"),
        )
        .child(
            tela.logs_pagina
                .convertendo
                .dialogo(t!("logs-converter-titulo"), "logs-convertendo"),
        )
}

/// O menu "Cifrar ou decifrar": os escolhidos num sentido ou no outro, ou todos os de texto.
fn menu_de_cifragem(
    tela: &Rc<Tela>,
    sem_escolha: impl Fn() -> bool + Clone + 'static,
) -> MenuFlyout {
    let (cifrar, decifrar, todos) = (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
    let (com_escolha_a, com_escolha_b) = (sem_escolha.clone(), sem_escolha);
    MenuFlyout::new()
        .id("logs-cifragem-menu")
        .item("logs-cifrar", t!("logs-cifrar"), move || {
            cifrar.pedir_para_converter_logs(Sentido::Cifrar, false)
        })
        .icon(Symbol::LockClosed)
        .item_visible(move || !com_escolha_a())
        .item("logs-decifrar", t!("logs-decifrar"), move || {
            decifrar.pedir_para_converter_logs(Sentido::Decifrar, false)
        })
        .icon(Symbol::LockOpen)
        .item_visible(move || !com_escolha_b())
        .item("logs-cifrar-todos", t!("logs-cifrar-todos"), move || {
            todos.pedir_para_converter_logs(Sentido::Cifrar, true)
        })
        .icon(Symbol::ShieldCheckmark)
}

/// O diálogo de confirmar a conversão, com a caixa de apagar o que fica para trás.
fn dialogo_de_converter(tela: &Rc<Tela>) -> impl View + use<> {
    let e = &tela.logs_pagina;
    let (aberto, texto, cifrando, apagar) = (
        e.converter_aberto,
        e.converter_texto,
        e.converter_cifrando,
        e.converter_apagar,
    );
    let t = Rc::clone(tela);
    ContentDialog::new(t!("logs-converter-titulo"), aberto)
        .id("logs-converter-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || texto.get().0)
                        .id("logs-converter-pergunta")
                        .wrap(),
                )
                .child(
                    Text::new(move || texto.get().1)
                        .id("logs-converter-gravando")
                        .secondary()
                        .wrap(),
                )
                .child(
                    CheckBox::bind(apagar, t!("logs-converter-apagar-originais"))
                        .id("logs-converter-apagar-originais")
                        .visible(move || cifrando.get()),
                )
                .child(
                    CheckBox::bind(apagar, t!("logs-converter-apagar-cifrados"))
                        .id("logs-converter-apagar-cifrados")
                        .visible(move || !cifrando.get()),
                ),
        )
        .primary(t!("logs-converter"), move || t.converter_logs())
        .close(t!("comum-cancelar"))
}

/// Um `InfoBar` que só aparece quando o aviso da leitura tem a `severidade`.
fn barra_de_leitura(
    id: &str,
    severidade: InfoBarSeverity,
    aviso: Signal<Option<(InfoBarSeverity, String)>>,
) -> impl View {
    InfoBar::new(severidade)
        .id(id)
        .message(move || aviso.get().map(|(_, m)| m).unwrap_or_default())
        .visible(move || aviso.get().is_some_and(|(s, _)| s == severidade))
}

/// O diálogo de confirmar o apagamento.
fn dialogo_de_apagar(
    tela: &Rc<Tela>,
    aberto: Signal<bool>,
    texto: Signal<(String, String)>,
) -> impl View + use<> {
    let t = Rc::clone(tela);
    ContentDialog::new(t!("logs-apagar-titulo"), aberto)
        .id("logs-apagar-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || texto.get().0)
                        .id("logs-apagar-pergunta")
                        .wrap(),
                )
                .child(
                    Text::new(t!("logs-apagar-aviso"))
                        .id("logs-apagar-aviso")
                        .wrap(),
                )
                .child(
                    Text::new(move || texto.get().1)
                        .id("logs-apagar-gravando")
                        .secondary()
                        .wrap(),
                ),
        )
        .primary(t!("comum-apagar"), move || t.apagar_logs())
        .close(t!("comum-cancelar"))
}
