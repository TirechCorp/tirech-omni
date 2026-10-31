//! As abas Processos, Disco e Rede da página Desempenho: os processos deste computador ou do
//! servidor de uma conexão SSH aberta, com a CPU, a memória e o disco de cada um, a árvore pelos
//! pais e os botões de encerrar; a CPU por núcleo com o tempo de kernel; as operações por
//! segundo de cada disco; e o tráfego de cada interface de rede. As leituras estão em [`crate::processos`].
//!
//! O leitor só corre com a página aberta numa das abas novas; trocar de máquina troca o leitor.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use super::desempenho::{Curva, Detalhe, numero, paineis, teto};
use crate::agente::{acordar, ler_contador};
use crate::desempenho as conta;
use crate::espacos::id_da_chave;
use crate::portas::descobrir::AbridorCompartilhado;
use crate::processos::{Evento, Leitor, Leitura, Linha, Maquina, PONTOS, UsoDoDisco, UsoDoNucleo};
use crate::t;

/// A aba do que o próprio app gasta (a de antes das abas novas).
pub(super) const APP: &str = "app";
/// A aba dos processos.
pub(super) const PROCESSOS: &str = "processos";
/// A aba do disco por processo.
pub(super) const DISCO: &str = "disco";
/// A aba da rede por interface.
pub(super) const REDE: &str = "rede";

/// O estado das abas novas.
#[derive(Clone, Copy)]
pub(super) struct Processos {
    /// A aba escolhida na página: [`APP`], [`PROCESSOS`], [`DISCO`] ou [`REDE`].
    pub(super) aba: Signal<String>,
    /// O rótulo da máquina escolhida no seletor.
    maquina: Signal<String>,
    /// O texto do filtro, o mesmo nas duas abas.
    filtro: Signal<String>,
    /// A tabela de processos como árvore, pelos pais.
    arvore: Signal<bool>,
    /// Só os processos que leram ou escreveram no disco agora.
    so_disco: Signal<bool>,
    /// A leitura mais nova.
    quadro: Signal<Option<Leitura>>,
    /// O erro da leitura (o servidor sem `/proc`, a conexão caída).
    erro: Signal<Option<String>>,
    /// O erro de encerrar.
    aviso: Signal<Option<String>>,
    /// Sobe a cada entrega do leitor.
    pronto: Signal<u64>,
    linhas: Signal<Vec<DataGridRow>>,
    escolhidas: Signal<BTreeSet<String>>,
    ordem: Signal<Option<(String, SortDirection)>>,
    linhas_do_disco: Signal<Vec<DataGridRow>>,
    escolhidas_do_disco: Signal<BTreeSet<String>>,
    ordem_do_disco: Signal<Option<(String, SortDirection)>>,
    /// A tabela dos discos, em cima da dos processos na aba Disco, e a altura dela.
    linhas_dos_discos: Signal<Vec<DataGridRow>>,
    escolhidos_dos_discos: Signal<BTreeSet<String>>,
    ordem_dos_discos: Signal<Option<(String, SortDirection)>>,
    altura_dos_discos: Signal<f64>,
    linhas_da_rede: Signal<Vec<DataGridRow>>,
    escolhidas_da_rede: Signal<BTreeSet<String>>,
    ordem_da_rede: Signal<Option<(String, SortDirection)>>,
    /// As séries dos gráficos, uma leitura por ponto: a CPU da máquina e a parte do kernel (%),
    /// a memória em uso (bytes), o disco e a rede (bytes por segundo) e as operações do disco
    /// (por segundo). Recomeçam quando a máquina muda.
    cpu: Signal<Vec<f64>>,
    cpu_sistema: Signal<Vec<f64>>,
    memoria: Signal<Vec<f64>>,
    leitura: Signal<Vec<f64>>,
    escrita: Signal<Vec<f64>>,
    operacoes: Signal<Vec<f64>>,
    recebido: Signal<Vec<f64>>,
    enviado: Signal<Vec<f64>>,
    /// As séries de cada núcleo: o uso e a parte do kernel (%).
    nucleos: Signal<Vec<Vec<f64>>>,
    nucleos_sistema: Signal<Vec<Vec<f64>>>,
    /// O tráfego de cada interface (o recebido mais o enviado, por segundo), pelo nome.
    trafego: Signal<HashMap<String, Vec<f64>>>,
    /// As operações de cada disco (as leituras mais as escritas, por segundo), pelo nome.
    iops: Signal<HashMap<String, Vec<f64>>>,
    /// O gráfico da CPU mostra um gráfico por núcleo.
    por_nucleo: Signal<bool>,
    /// O gráfico da CPU mostra a linha do tempo de kernel.
    kernel: Signal<bool>,
    /// O gráfico grande de cada aba: o índice da curva.
    grafico: Signal<usize>,
    grafico_do_disco: Signal<usize>,
    grafico_da_rede: Signal<usize>,
}

/// O leitor que corre e, numa máquina remota, a sessão e o abridor dela (renovado a cada
/// entrega: depois de uma reconexão o abridor antigo só dá erro).
pub(super) struct Corrente {
    leitor: Leitor,
    remota: Option<(Session, AbridorCompartilhado)>,
}

impl Corrente {
    /// Sobe o leitor de `sessao` (`None`: este computador); cada entrega sobe `pronto`.
    pub(super) fn nova(tela: &Tela, sessao: Option<Session>, pronto: Signal<u64>) -> Self {
        let ambiente = tela.ferramentas.ambiente();
        let (alvo, remota) = match sessao {
            Some(sessao) => {
                let abridor: AbridorCompartilhado = Arc::new(Mutex::new(sessao.ssh_channels()));
                (
                    Maquina::Remota(Arc::clone(&abridor)),
                    Some((sessao, abridor)),
                )
            }
            None => (Maquina::Local, None),
        };
        let leitor = Leitor::novo(
            (ambiente.processos)(alvo),
            ambiente.ritmo_dos_processos,
            move || acordar(pronto),
        );
        Self { leitor, remota }
    }

    /// Tira da fila o que o leitor entregou, depois de renovar o abridor da sessão.
    pub(super) fn recolher(&self) -> Vec<Evento> {
        if let Some((sessao, abridor)) = &self.remota {
            *abridor.lock().unwrap_or_else(PoisonError::into_inner) = sessao.ssh_channels();
        }
        self.leitor.recolher()
    }
}

/// A ordem padrão de cada tabela.
fn ordem_padrao(aba: &str) -> Option<(String, SortDirection)> {
    let coluna = match aba {
        DISCO => "escrita",
        REDE => "recebendo",
        _ => "cpu",
    };
    Some((coluna.to_owned(), SortDirection::Descending))
}

/// As máquinas do seletor: este computador e as sessões SSH conectadas, pelo nome da conexão
/// (ou o título da aba). Nomes repetidos ganham " (2)", " (3)"…
pub(super) fn maquinas(tela: &Tela) -> Vec<(String, Option<Session>)> {
    let mut lista = vec![(t!("processos-este-computador"), None)];
    let mut vistos: HashMap<String, usize> = HashMap::new();
    for janela in tela.janelas.todas() {
        let sessoes = janela.layout.try_with(|l| l.sessions()).unwrap_or_default();
        for sessao in sessoes {
            if sessao.ssh_channels().is_none() {
                continue;
            }
            let nome = id_da_chave(&sessao.key_str())
                .and_then(|id| {
                    tela.tree
                        .with_untracked(|a| a.connection(id).map(|c| c.name.clone()))
                })
                .unwrap_or_else(|| sessao.title());
            let n = vistos.entry(nome.clone()).or_insert(0);
            *n += 1;
            let rotulo = if *n == 1 {
                nome
            } else {
                format!("{nome} ({n})")
            };
            lista.push((rotulo, Some(sessao)));
        }
    }
    lista
}

/// O estado e os efeitos: o leitor da máquina escolhida enquanto `pagina_aberta()` e uma das
/// abas novas está à mostra, e as linhas das tabelas a cada leitura.
pub(super) fn ligar(tela: &Rc<Tela>, pagina_aberta: impl Fn() -> bool + 'static) -> Processos {
    let p = Processos {
        aba: signal(APP.to_owned()),
        maquina: signal(t!("processos-este-computador")),
        filtro: signal(String::new()),
        arvore: signal(false),
        so_disco: signal(false),
        quadro: signal(None),
        erro: signal(None),
        aviso: signal(None),
        pronto: signal(0),
        linhas: signal(Vec::new()),
        escolhidas: signal(BTreeSet::new()),
        ordem: signal(ordem_padrao(PROCESSOS)),
        linhas_do_disco: signal(Vec::new()),
        escolhidas_do_disco: signal(BTreeSet::new()),
        ordem_do_disco: signal(ordem_padrao(DISCO)),
        linhas_dos_discos: signal(Vec::new()),
        escolhidos_dos_discos: signal(BTreeSet::new()),
        ordem_dos_discos: signal(None),
        altura_dos_discos: signal(104.0),
        linhas_da_rede: signal(Vec::new()),
        escolhidas_da_rede: signal(BTreeSet::new()),
        ordem_da_rede: signal(ordem_padrao(REDE)),
        cpu: signal(Vec::new()),
        cpu_sistema: signal(Vec::new()),
        memoria: signal(Vec::new()),
        leitura: signal(Vec::new()),
        escrita: signal(Vec::new()),
        operacoes: signal(Vec::new()),
        recebido: signal(Vec::new()),
        enviado: signal(Vec::new()),
        nucleos: signal(Vec::new()),
        nucleos_sistema: signal(Vec::new()),
        trafego: signal(HashMap::new()),
        iops: signal(HashMap::new()),
        por_nucleo: signal(false),
        kernel: signal(true),
        grafico: signal(0),
        grafico_do_disco: signal(0),
        grafico_da_rede: signal(0),
    };
    // O leitor: nasce quando a aba aparece, troca com a máquina e para quando some.
    let t = Rc::clone(tela);
    let antes: RefCell<Option<(bool, String)>> = RefCell::new(None);
    effect(move || {
        let ativo = pagina_aberta() && p.aba.get() != APP;
        let maquina = p.maquina.get();
        let agora = Some((ativo, maquina.clone()));
        if *antes.borrow() == agora {
            return;
        }
        *antes.borrow_mut() = agora;
        untrack(|| {
            t.processos_correndo.borrow_mut().take();
            p.quadro.set(None);
            p.erro.set(None);
            p.aviso.set(None);
            for serie in [
                p.cpu,
                p.cpu_sistema,
                p.memoria,
                p.leitura,
                p.escrita,
                p.operacoes,
                p.recebido,
                p.enviado,
            ] {
                serie.set(Vec::new());
            }
            p.nucleos.set(Vec::new());
            p.nucleos_sistema.set(Vec::new());
            p.trafego.set(HashMap::new());
            p.iops.set(HashMap::new());
            if !ativo {
                return;
            }
            let sessao = maquinas(&t)
                .into_iter()
                .find(|(rotulo, _)| *rotulo == maquina)
                .and_then(|(_, s)| s);
            *t.processos_correndo.borrow_mut() = Some(Corrente::nova(&t, sessao, p.pronto));
        });
    });

    // O que o leitor entregou.
    let t = Rc::clone(tela);
    effect(move || {
        ler_contador(p.pronto);
        untrack(|| {
            let eventos = {
                let correndo = t.processos_correndo.borrow();
                let Some(c) = correndo.as_ref() else {
                    return;
                };
                c.recolher()
            };
            for evento in eventos {
                match evento {
                    Evento::Leitura(Ok(leitura)) => {
                        p.erro.set(None);
                        p.empurrar(&leitura);
                        p.quadro.set(Some(leitura));
                    }
                    Evento::Leitura(Err(erro)) => p.erro.set(Some(erro)),
                    Evento::Encerrado(Ok(())) => p.aviso.set(None),
                    Evento::Encerrado(Err(erro)) => p.aviso.set(Some(erro)),
                }
            }
        });
    });

    // A sessão escolhida fechou: volta para este computador.
    let t = Rc::clone(tela);
    effect(move || {
        p.pronto.get();
        let lista = maquinas(&t);
        let atual = p.maquina.get();
        if !lista.iter().any(|(rotulo, _)| *rotulo == atual) {
            p.maquina.set(lista[0].0.clone());
        }
    });

    // A árvore tem a ordem dela: ligar a árvore tira a ordem, e escolher uma coluna desliga a
    // árvore.
    effect(move || {
        if p.arvore.get() {
            p.ordem.set(None);
        } else if p.ordem.get_untracked().is_none() {
            p.ordem.set(ordem_padrao(PROCESSOS));
        }
    });
    effect(move || {
        if p.ordem.with(Option::is_some) && p.arvore.get_untracked() {
            p.arvore.set(false);
        }
    });

    // As linhas.
    effect(move || {
        let filtro = p.filtro.get().trim().to_lowercase();
        let (arvore, so_disco) = (p.arvore.get(), p.so_disco.get());
        p.quadro.with(|quadro| {
            let Some(leitura) = quadro else {
                p.linhas.set(Vec::new());
                p.linhas_do_disco.set(Vec::new());
                p.linhas_dos_discos.set(Vec::new());
                p.linhas_da_rede.set(Vec::new());
                return;
            };
            p.linhas.set(linhas_dos_processos(leitura, &filtro, arvore));
            p.linhas_do_disco.set(linhas_do_disco(
                leitura,
                &filtro,
                so_disco,
                OPERACOES_POR_PROCESSO,
            ));
            // As séries já têm o ponto desta leitura: o leitor empurra antes de mostrar.
            let linhas = p
                .iops
                .with_untracked(|iops| linhas_dos_discos(&leitura.discos, &filtro, iops));
            p.linhas_dos_discos.set(linhas);
            let linhas = p
                .trafego
                .with_untracked(|trafego| linhas_da_rede(leitura, &filtro, trafego));
            p.linhas_da_rede.set(linhas);
        });
    });
    p
}

impl Processos {
    /// Põe a leitura nas séries dos gráficos. A primeira foto não tem taxas e fica de fora.
    fn empurrar(&self, leitura: &Leitura) {
        let tem_taxa = leitura
            .linhas
            .iter()
            .any(|l| l.cpu.is_some() || l.leitura.is_some())
            || !leitura.nucleos_uso.is_empty()
            || leitura.interfaces.iter().any(|i| i.recebendo.is_some())
            || leitura.discos.iter().any(|d| d.leituras.is_some());
        if !tem_taxa {
            return;
        }
        let (lido, escrito) = leitura.disco();
        let (recebido, enviado) = leitura.rede();
        for (serie, valor) in [
            (self.cpu, leitura.cpu()),
            (self.memoria, leitura.memoria_em_uso() as f64),
            (self.leitura, lido),
            (self.escrita, escrito),
            (self.recebido, recebido),
            (self.enviado, enviado),
        ] {
            serie.update(|s| por_no_fim(s, valor));
        }
        if let Some(sistema) = leitura.cpu_sistema() {
            self.cpu_sistema.update(|s| por_no_fim(s, sistema));
        }
        if let Some((leituras, escritas)) = leitura.operacoes() {
            self.operacoes
                .update(|s| por_no_fim(s, leituras + escritas));
        }
        let quantos = leitura.nucleos_uso.len();
        for (series, valor) in [
            (
                self.nucleos,
                (|n: &UsoDoNucleo| n.total) as fn(&UsoDoNucleo) -> f64,
            ),
            (self.nucleos_sistema, |n: &UsoDoNucleo| n.sistema),
        ] {
            series.update(|series| {
                // Os núcleos mudaram (outra máquina, um núcleo desligado): recomeça.
                if series.len() != quantos {
                    *series = vec![Vec::new(); quantos];
                }
                for (serie, uso) in series.iter_mut().zip(&leitura.nucleos_uso) {
                    por_no_fim(serie, valor(uso));
                }
            });
        }
        self.trafego.update(|trafego| {
            trafego.retain(|nome, _| leitura.interfaces.iter().any(|i| i.nome == *nome));
            for interface in &leitura.interfaces {
                let valor = interface.recebendo.unwrap_or(0.0) + interface.enviando.unwrap_or(0.0);
                por_no_fim(trafego.entry(interface.nome.clone()).or_default(), valor);
            }
        });
        self.iops.update(|iops| {
            iops.retain(|nome, _| leitura.discos.iter().any(|d| d.nome == *nome));
            for disco in &leitura.discos {
                let valor = disco.leituras.unwrap_or(0.0) + disco.escritas.unwrap_or(0.0);
                por_no_fim(iops.entry(disco.nome.clone()).or_default(), valor);
            }
        });
    }
}

/// Põe `valor` no fim da série, que guarda os [`PONTOS`] mais novos.
fn por_no_fim(serie: &mut Vec<f64>, valor: f64) {
    serie.push(valor);
    if serie.len() > PONTOS {
        serie.drain(..serie.len() - PONTOS);
    }
}

/// Se a linha passa no filtro: o nome, a linha de comando, o usuário ou o pid.
fn passa(linha: &Linha, filtro: &str) -> bool {
    let p = &linha.processo;
    filtro.is_empty()
        || p.nome.to_lowercase().contains(filtro)
        || p.comando.to_lowercase().contains(filtro)
        || p.usuario.to_lowercase().contains(filtro)
        || p.pid.to_string() == filtro
}

/// As linhas na ordem da árvore, cada uma com o prefixo do desenho ("├─ ", "│  └─ "…). Com
/// filtro, ficam as que passam e os pais delas.
fn em_arvore<'a>(linhas: &'a [Linha], filtro: &str) -> Vec<(String, &'a Linha)> {
    let por_pid: HashMap<u32, &Linha> = linhas.iter().map(|l| (l.processo.pid, l)).collect();
    let mut ficam: HashSet<u32> = HashSet::new();
    for linha in linhas.iter().filter(|l| passa(l, filtro)) {
        let mut pid = linha.processo.pid;
        // Sobe pelos pais (com teto, contra um ciclo de pids reaproveitados).
        for _ in 0..256 {
            if !ficam.insert(pid) {
                break;
            }
            match por_pid.get(&pid) {
                Some(l) if l.processo.pai != pid && por_pid.contains_key(&l.processo.pai) => {
                    pid = l.processo.pai;
                }
                _ => break,
            }
        }
    }
    let mut filhos: HashMap<u32, Vec<&Linha>> = HashMap::new();
    let mut raizes = Vec::new();
    for linha in linhas.iter().filter(|l| ficam.contains(&l.processo.pid)) {
        let pai = linha.processo.pai;
        if pai != linha.processo.pid && ficam.contains(&pai) {
            filhos.entry(pai).or_default().push(linha);
        } else {
            raizes.push(linha);
        }
    }
    for lista in filhos.values_mut() {
        lista.sort_by_key(|l| l.processo.pid);
    }
    raizes.sort_by_key(|l| l.processo.pid);
    let mut saida = Vec::with_capacity(ficam.len());
    let mut vistos = HashSet::new();
    // A pilha guarda a linha, o recuo dos níveis de cima e se é a última entre os irmãos.
    let mut pilha: Vec<(&Linha, String, Option<bool>)> = raizes
        .into_iter()
        .rev()
        .map(|l| (l, String::new(), None))
        .collect();
    while let Some((linha, recuo, ultima)) = pilha.pop() {
        if !vistos.insert(linha.processo.pid) {
            continue;
        }
        let (prefixo, recuo_dos_filhos) = match ultima {
            None => (String::new(), String::new()),
            Some(true) => (format!("{recuo}└─ "), format!("{recuo}   ")),
            Some(false) => (format!("{recuo}├─ "), format!("{recuo}│  ")),
        };
        saida.push((prefixo, linha));
        if let Some(lista) = filhos.get(&linha.processo.pid) {
            let n = lista.len();
            for (i, filho) in lista.iter().enumerate().rev() {
                pilha.push((filho, recuo_dos_filhos.clone(), Some(i + 1 == n)));
            }
        }
    }
    saida
}

/// O calor de uma CPU: um núcleo inteiro é o máximo.
fn calor_da_cpu(cpu: f64, nucleos: usize) -> f64 {
    (cpu * nucleos.max(1) as f64 / 100.0).clamp(0.0, 1.0)
}

/// Uma célula de número que pode faltar: "—" e −1 para ir ao fim da ordem.
fn ou_traco(valor: Option<f64>, texto: impl Fn(f64) -> String) -> (String, CellValue) {
    match valor {
        Some(v) => (texto(v), CellValue::Number(v)),
        None => ("—".to_owned(), CellValue::Number(-1.0)),
    }
}

/// As linhas da tabela de processos.
fn linhas_dos_processos(leitura: &Leitura, filtro: &str, arvore: bool) -> Vec<DataGridRow> {
    let ordenadas: Vec<(String, &Linha)> = if arvore {
        em_arvore(&leitura.linhas, filtro)
    } else {
        leitura
            .linhas
            .iter()
            .filter(|l| passa(l, filtro))
            .map(|l| (String::new(), l))
            .collect()
    };
    let total = leitura.memoria_total;
    ordenadas
        .into_iter()
        .map(|(prefixo, linha)| {
            let p = &linha.processo;
            let (cpu, cpu_valor) = ou_traco(linha.cpu, conta::cpu);
            let (memoria, memoria_valor) =
                ou_traco(p.memoria.map(|m| m as f64), |m| conta::memoria(m as u64));
            let (threads, threads_valor) =
                ou_traco(p.threads.map(f64::from), |n| format!("{n:.0}"));
            let estado = p.estado.map_or_else(|| "—".to_owned(), |e| e.texto());
            DataGridRow::new(p.pid.to_string())
                .cell(
                    format!("{prefixo}{}", p.nome),
                    CellValue::Text(p.nome.clone()),
                )
                .cell(p.pid.to_string(), CellValue::Number(f64::from(p.pid)))
                .cell(p.usuario.clone(), CellValue::Text(p.usuario.clone()))
                .cell(estado.clone(), CellValue::Text(estado))
                .cell(cpu, cpu_valor)
                .heat(linha.cpu.map_or(0.0, |c| calor_da_cpu(c, leitura.nucleos)))
                .cell(memoria, memoria_valor)
                .heat(p.memoria.zip(total).map_or(0.0, |(m, t)| {
                    (m as f64 / t.max(1) as f64 * 4.0).clamp(0.0, 1.0)
                }))
                .cell(threads, threads_valor)
                .cell(p.comando.clone(), CellValue::Text(p.comando.clone()))
        })
        .collect()
}

/// Se a fonte local conta as operações de cada processo (só o Windows conta): a tabela de
/// processos da aba Disco ganha as colunas das leituras e das escritas por segundo.
const OPERACOES_POR_PROCESSO: bool = cfg!(windows);

/// Operações por segundo, sem casas ("120/s").
fn por_segundo(valor: f64) -> String {
    t!("processos-por-segundo", n = format!("{valor:.0}"))
}

/// As linhas da tabela de processos da aba Disco; com `operacoes`, também as leituras e as
/// escritas por segundo de cada um.
fn linhas_do_disco(
    leitura: &Leitura,
    filtro: &str,
    so_disco: bool,
    operacoes: bool,
) -> Vec<DataGridRow> {
    // 1 MB/s já é o calor máximo, como na rede da aba Este app.
    let calor = |v: Option<f64>| v.map_or(0.0, |v| (v / (1024.0 * 1024.0)).clamp(0.0, 1.0));
    leitura
        .linhas
        .iter()
        .filter(|l| passa(l, filtro))
        .filter(|l| !so_disco || l.leitura.unwrap_or(0.0) + l.escrita.unwrap_or(0.0) > 0.0)
        .map(|linha| {
            let p = &linha.processo;
            let (leitura, leitura_valor) = ou_traco(linha.leitura, conta::bytes_por_segundo);
            let (escrita, escrita_valor) = ou_traco(linha.escrita, conta::bytes_por_segundo);
            let (lido, lido_valor) =
                ou_traco(p.disco.map(|d| d.0 as f64), |b| conta::memoria(b as u64));
            let (escrito, escrito_valor) =
                ou_traco(p.disco.map(|d| d.1 as f64), |b| conta::memoria(b as u64));
            let mut fileira = DataGridRow::new(p.pid.to_string())
                .cell(p.nome.clone(), CellValue::Text(p.nome.clone()))
                .cell(p.pid.to_string(), CellValue::Number(f64::from(p.pid)))
                .cell(p.usuario.clone(), CellValue::Text(p.usuario.clone()))
                .cell(leitura, leitura_valor)
                .heat(calor(linha.leitura))
                .cell(escrita, escrita_valor)
                .heat(calor(linha.escrita));
            if operacoes {
                let (leituras, leituras_valor) =
                    ou_traco(linha.operacoes.map(|o| o.0), por_segundo);
                let (escritas, escritas_valor) =
                    ou_traco(linha.operacoes.map(|o| o.1), por_segundo);
                fileira = fileira
                    .cell(leituras, leituras_valor)
                    .cell(escritas, escritas_valor);
            }
            fileira
                .cell(lido, lido_valor)
                .cell(escrito, escrito_valor)
                .cell(p.comando.clone(), CellValue::Text(p.comando.clone()))
        })
        .collect()
}

/// As linhas da tabela dos discos: um por disco, com as operações e os bytes por segundo e o
/// histórico das operações.
fn linhas_dos_discos(
    discos: &[UsoDoDisco],
    filtro: &str,
    iops: &HashMap<String, Vec<f64>>,
) -> Vec<DataGridRow> {
    // 1 MB/s e 100 operações por segundo já são o calor máximo.
    let calor = |v: Option<f64>| v.map_or(0.0, |v| (v / (1024.0 * 1024.0)).clamp(0.0, 1.0));
    let calor_ops = |v: Option<f64>| v.map_or(0.0, |v| (v / 100.0).clamp(0.0, 1.0));
    discos
        .iter()
        .filter(|d| filtro.is_empty() || d.nome.to_lowercase().contains(filtro))
        .map(|d| {
            let (leituras, leituras_valor) = ou_traco(d.leituras, por_segundo);
            let (escritas, escritas_valor) = ou_traco(d.escritas, por_segundo);
            let (lendo, lendo_valor) = ou_traco(d.lendo, conta::bytes_por_segundo);
            let (escrevendo, escrevendo_valor) = ou_traco(d.escrevendo, conta::bytes_por_segundo);
            let tipo = if d.fisico {
                t!("processos-disco-fisico")
            } else {
                t!("processos-disco-virtual")
            };
            let serie = iops.get(&d.nome).cloned().unwrap_or_default();
            let teto = serie.iter().copied().fold(10.0, f64::max);
            DataGridRow::new(d.nome.clone())
                .cell(d.nome.clone(), CellValue::Text(d.nome.clone()))
                .cell(tipo.clone(), CellValue::Text(tipo))
                .cell(leituras, leituras_valor)
                .heat(calor_ops(d.leituras))
                .cell(escritas, escritas_valor)
                .heat(calor_ops(d.escritas))
                .cell(lendo, lendo_valor)
                .heat(calor(d.lendo))
                .cell(escrevendo, escrevendo_valor)
                .heat(calor(d.escrevendo))
                .cell(
                    t!("desempenho-historico-de", aba = d.nome.as_str()),
                    CellValue::Sparkline(serie.into_iter().map(|v| v / teto).collect()),
                )
        })
        .collect()
}

/// As linhas da aba Rede: uma por interface, com o histórico do tráfego dela.
fn linhas_da_rede(
    leitura: &Leitura,
    filtro: &str,
    trafego: &HashMap<String, Vec<f64>>,
) -> Vec<DataGridRow> {
    let calor = |v: Option<f64>| v.map_or(0.0, |v| (v / (1024.0 * 1024.0)).clamp(0.0, 1.0));
    leitura
        .interfaces
        .iter()
        .filter(|i| filtro.is_empty() || i.nome.to_lowercase().contains(filtro))
        .map(|i| {
            let (recebendo, recebendo_valor) = ou_traco(i.recebendo, conta::bytes_por_segundo);
            let (enviando, enviando_valor) = ou_traco(i.enviando, conta::bytes_por_segundo);
            let tipo = if i.fisica {
                t!("processos-rede-fisica")
            } else {
                t!("processos-rede-virtual")
            };
            let serie = trafego.get(&i.nome).cloned().unwrap_or_default();
            let teto = serie.iter().copied().fold(4096.0, f64::max);
            DataGridRow::new(i.nome.clone())
                .cell(i.nome.clone(), CellValue::Text(i.nome.clone()))
                .cell(tipo.clone(), CellValue::Text(tipo))
                .cell(recebendo, recebendo_valor)
                .heat(calor(i.recebendo))
                .cell(enviando, enviando_valor)
                .heat(calor(i.enviando))
                .cell(
                    conta::memoria(i.recebidos),
                    CellValue::Number(i.recebidos as f64),
                )
                .cell(
                    conta::memoria(i.enviados),
                    CellValue::Number(i.enviados as f64),
                )
                .cell(
                    t!("desempenho-historico-de", aba = i.nome.as_str()),
                    CellValue::Sparkline(serie.into_iter().map(|v| v / teto).collect()),
                )
        })
        .collect()
}

impl Tela {
    /// Encerra os processos `chaves` (os pids) da máquina da aba; `forcar`, na marra.
    fn encerrar_processos(&self, chaves: &[String], forcar: bool) {
        let pids: Vec<u32> = chaves.iter().filter_map(|c| c.parse().ok()).collect();
        if pids.is_empty() {
            return;
        }
        if let Some(c) = self.processos_correndo.borrow().as_ref() {
            c.leitor.encerrar(pids, forcar);
        }
    }
}

/// A barra de cima de uma aba: a máquina, o filtro, a opção da aba e os botões de encerrar (que
/// descem para a linha de baixo quando a janela é estreita). Na Rede, só a máquina e o filtro.
fn barra(
    tela: &Rc<Tela>,
    p: Processos,
    aba: &'static str,
    escolhidas: Signal<BTreeSet<String>>,
) -> impl View + use<> {
    let t = Rc::clone(tela);
    let rotulos = move || {
        p.pronto.get();
        vec![
            maquinas(&t)
                .into_iter()
                .map(|(rotulo, _)| rotulo)
                .collect::<Vec<_>>(),
        ]
    };
    let seletor = For::each(
        rotulos,
        |opcoes: &Vec<String>| opcoes.join("\u{1}"),
        move |opcoes: Vec<String>| {
            ComboBox::bind(p.maquina, opcoes)
                .id(format!("desempenho-{aba}-maquina"))
                .width(200)
        },
    );
    let linha = Row::new()
        .wrap()
        .gap(8)
        .align(Align::Center)
        .child(Column::new().child(seletor))
        .child(
            Edit::bind(p.filtro)
                .id(format!("desempenho-{aba}-filtro"))
                .placeholder(if aba == REDE {
                    t!("processos-filtrar-interfaces")
                } else {
                    t!("processos-filtrar")
                })
                .width(200),
        );
    // A rede não tem processos para encerrar.
    if aba == REDE {
        return linha;
    }
    let opcao = if aba == PROCESSOS {
        CheckBox::bind(p.arvore, t!("processos-arvore")).id("desempenho-processos-arvore")
    } else {
        CheckBox::bind(p.so_disco, t!("processos-so-disco")).id("desempenho-disco-so-ativos")
    };
    let nenhuma = move || escolhidas.with(BTreeSet::is_empty);
    let (t1, t2) = (Rc::clone(tela), Rc::clone(tela));
    linha.child(opcao).child(Border::new().grow(1)).child(
        Row::new()
            .gap(8)
            .child(
                Button::new(t!("processos-encerrar"))
                    .id(format!("desempenho-{aba}-encerrar"))
                    .icon(Symbol::Stop)
                    .disabled(nenhuma)
                    .on_click(move || {
                        let chaves: Vec<String> = escolhidas.get_untracked().into_iter().collect();
                        t1.encerrar_processos(&chaves, false);
                    }),
            )
            .child(
                Button::new(t!("processos-forcar"))
                    .id(format!("desempenho-{aba}-forcar"))
                    .disabled(nenhuma)
                    .on_click(move || {
                        let chaves: Vec<String> = escolhidas.get_untracked().into_iter().collect();
                        t2.encerrar_processos(&chaves, true);
                    }),
            ),
    )
}

/// Um número da leitura de agora, ou "—" antes da primeira.
fn agora(p: Processos, f: impl Fn(&Leitura) -> String + 'static) -> impl Fn() -> String + 'static {
    move || {
        p.quadro
            .with(|q| q.as_ref().map_or_else(|| "—".to_owned(), &f))
    }
}

/// Os gráficos da aba Processos: a CPU (com o tempo de kernel e, pelo menu, um gráfico por
/// núcleo) e a memória da máquina no último minuto. Os números embaixo são os da curva escolhida.
fn graficos_dos_processos(p: Processos) -> impl View + use<> {
    let teto_memoria = move || {
        p.quadro
            .with(|q| q.as_ref().and_then(|l| l.memoria_total))
            .map_or_else(|| teto(p.memoria, 64.0 * 1024.0 * 1024.0)(), |t| t as f64)
    };
    let cpu = agora(p, |l| conta::cpu(l.cpu()));
    let memoria = agora(p, |l| conta::memoria(l.memoria_em_uso()));
    let curvas = vec![
        Curva {
            id: "cpu",
            nome: t!("desempenho-cpu"),
            titulo: t!("processos-cpu-titulo"),
            cor: Token::AccentFillColorDefault,
            serie: p.cpu,
            pontos: PONTOS,
            max: Rc::new(|| 100.0),
            valor: Rc::new(cpu),
            detalhe: Some(detalhe_da_cpu(p)),
        },
        Curva {
            id: "memoria",
            nome: t!("desempenho-memoria"),
            titulo: t!("desempenho-memoria-titulo"),
            cor: Token::SystemFillColorCaution,
            serie: p.memoria,
            pontos: PONTOS,
            max: Rc::new(teto_memoria),
            valor: Rc::new(memoria),
            detalhe: None,
        },
    ];
    let numeros_da_cpu = Row::new()
        .wrap()
        .gap(16)
        .visible(move || p.grafico.get() == 0)
        .child(numero(
            "desempenho-processos-utilizacao",
            t!("desempenho-utilizacao"),
            agora(p, |l| conta::cpu(l.cpu())),
        ))
        .child(numero(
            "desempenho-processos-kernel",
            t!("processos-kernel"),
            agora(p, |l| {
                l.cpu_sistema().map_or_else(|| "—".to_owned(), conta::cpu)
            }),
        ))
        .child(numero(
            "desempenho-processos-quantos",
            t!("processos-quantos"),
            agora(p, |l| l.linhas.len().to_string()),
        ))
        .child(numero(
            "desempenho-processos-threads",
            t!("processos-threads"),
            agora(p, |l| {
                l.linhas
                    .iter()
                    .filter_map(|l| l.processo.threads)
                    .map(u64::from)
                    .sum::<u64>()
                    .to_string()
            }),
        ));
    let numeros_da_memoria = Row::new()
        .wrap()
        .gap(16)
        .visible(move || p.grafico.get() == 1)
        .child(numero(
            "desempenho-processos-memoria-numero",
            t!("processos-em-uso"),
            agora(p, |l| conta::memoria(l.memoria_em_uso())),
        ))
        .child(numero(
            "desempenho-processos-disponivel",
            t!("desempenho-disponivel"),
            agora(p, |l| {
                l.memoria_disponivel
                    .map_or_else(|| "—".to_owned(), conta::memoria)
            }),
        ))
        .child(numero(
            "desempenho-processos-memoria-total",
            t!("desempenho-memoria-total"),
            agora(p, |l| {
                l.memoria_total
                    .map_or_else(|| "—".to_owned(), conta::memoria)
            }),
        ));
    let numeros = Column::new()
        .child(numeros_da_cpu)
        .child(numeros_da_memoria);
    paineis("desempenho-processos", curvas, p.grafico, 220.0, numeros)
}

/// O que o gráfico da CPU tem a mais: a linha do kernel (com o "Mostrar tempos de kernel"
/// ligado), o menu do clique direito e a grade com um gráfico por núcleo lógico.
/// Acima de quantos núcleos a grade vira o mapa de calor.
const NUCLEOS_EM_GRAFICOS: usize = 32;

/// Quantas colunas os gráficos de `total` núcleos ocupam numa área umas 4 vezes mais larga
/// que alta: 8 numa linha, 12 em 2 linhas de 6, 32 em 3 de 11.
fn colunas_dos_nucleos(total: usize) -> usize {
    let linhas = ((total as f64 / 4.0).sqrt().round() as usize).clamp(1, total.max(1));
    total.div_ceil(linhas).max(1)
}

fn detalhe_da_cpu(p: Processos) -> Detalhe {
    let menu = move |onde: &str| {
        MenuFlyout::new()
            .submenu(
                format!("desempenho-processos-{onde}-mudar"),
                t!("processos-mudar-grafico"),
                MenuFlyout::new()
                    .radio(
                        format!("desempenho-processos-{onde}-geral"),
                        t!("processos-utilizacao-geral"),
                        p.por_nucleo,
                        false,
                    )
                    .radio(
                        format!("desempenho-processos-{onde}-nucleos"),
                        t!("processos-processadores-logicos"),
                        p.por_nucleo,
                        true,
                    ),
            )
            .toggle(
                format!("desempenho-processos-{onde}-kernel"),
                t!("processos-mostrar-kernel"),
                p.kernel,
            )
    };
    // Um gráfico por núcleo, dividindo a área do gráfico grande como no Gerenciador de
    // Tarefas: as linhas e os gráficos partem de zero (`height(0)`, `width(0)`) e crescem até
    // ocupar a área, sem empurrar os números de baixo. Com mais de `NUCLEOS_EM_GRAFICOS`, os
    // gráficos ficariam pequenos demais para ler, e fica o mapa de calor, um ladrilho por
    // núcleo, como o Gerenciador faz nas máquinas com centenas de processadores lógicos. A
    // grade é refeita só quando a contagem de núcleos muda.
    let nucleo = move |n: usize| {
        AreaChart::new(move || p.nucleos.with(|s| s.get(n).cloned().unwrap_or_default()))
            .id(format!("desempenho-processos-nucleo-{n}"))
            .max(100.0)
            .capacity(PONTOS)
            .color(Token::AccentFillColorDefault)
            .secondary(move || {
                if p.kernel.get() {
                    p.nucleos_sistema
                        .with(|s| s.get(n).cloned().unwrap_or_default())
                } else {
                    Vec::new()
                }
            })
            .width(0)
            .grow(1)
    };
    let grade = For::each(
        move || vec![p.nucleos.with(Vec::len)],
        |total: &usize| *total,
        move |total: usize| {
            let mut linhas = Column::new().gap(4).height(0).grow(1);
            if total > NUCLEOS_EM_GRAFICOS {
                return linhas.child(
                    HeatMap::new(move || {
                        p.nucleos.with(|s| {
                            s.iter()
                                .map(|h| h.last().copied().unwrap_or_default())
                                .collect()
                        })
                    })
                    .id("desempenho-processos-nucleos-mapa")
                    .cell_tooltip(|n, uso| {
                        t!("processos-nucleo-dica", n = n, uso = conta::cpu(uso))
                    })
                    .height(0)
                    .grow(1),
                );
            }
            let colunas = colunas_dos_nucleos(total);
            for inicio in (0..total).step_by(colunas) {
                let mut linha = Row::new().gap(4).height(0).grow(1);
                for n in inicio..inicio + colunas {
                    linha = if n < total {
                        linha.child(nucleo(n))
                    } else {
                        // A última linha incompleta guarda o lugar dos que faltam.
                        linha.child(Column::new().width(0).grow(1))
                    };
                }
                linhas = linhas.child(linha);
            }
            linhas
        },
    );
    Detalhe {
        secundaria: Rc::new(move || {
            if p.kernel.get() {
                p.cpu_sistema.get()
            } else {
                Vec::new()
            }
        }),
        menu: Rc::new(menu),
        // Sem os tempos dos núcleos (a fonte não deu), fica o gráfico geral.
        grade_a_mostra: Rc::new(move || p.por_nucleo.get() && p.nucleos.with(|s| !s.is_empty())),
        grade: Column::new()
            .id("desempenho-processos-nucleos")
            .child(grade),
    }
}

/// Os gráficos da aba Disco: a leitura, a escrita e as operações por segundo da máquina no
/// último minuto, somadas dos discos físicos (ou de todos, se não houver nenhum físico).
fn graficos_do_disco(p: Processos) -> impl View + use<> {
    let lido = agora(p, |l| conta::bytes_por_segundo(l.disco().0));
    let escrito = agora(p, |l| conta::bytes_por_segundo(l.disco().1));
    let operacoes = |f: fn((f64, f64)) -> f64| {
        agora(p, move |l| {
            l.operacoes()
                .map_or_else(|| "—".to_owned(), |o| por_segundo(f(o)))
        })
    };
    let curvas = vec![
        Curva {
            id: "leitura",
            nome: t!("processos-disco-leitura"),
            titulo: t!("processos-leitura-titulo"),
            cor: Token::SystemFillColorSuccess,
            serie: p.leitura,
            pontos: PONTOS,
            max: Rc::new(teto(p.leitura, 4096.0)),
            valor: Rc::new(lido),
            detalhe: None,
        },
        Curva {
            id: "escrita",
            nome: t!("processos-disco-escrita"),
            titulo: t!("processos-escrita-titulo"),
            cor: Token::AccentFillColorDefault,
            serie: p.escrita,
            pontos: PONTOS,
            max: Rc::new(teto(p.escrita, 4096.0)),
            valor: Rc::new(escrito),
            detalhe: None,
        },
        Curva {
            id: "operacoes",
            nome: t!("processos-disco-operacoes"),
            titulo: t!("processos-disco-operacoes-titulo"),
            cor: Token::SystemFillColorCaution,
            serie: p.operacoes,
            pontos: PONTOS,
            max: Rc::new(teto(p.operacoes, 10.0)),
            valor: Rc::new(operacoes(|(l, e)| l + e)),
            detalhe: None,
        },
    ];
    let usando = move |id: &'static str| {
        numero(
            id,
            t!("processos-usando-disco"),
            agora(p, |l| {
                l.linhas
                    .iter()
                    .filter(|l| l.leitura.unwrap_or(0.0) + l.escrita.unwrap_or(0.0) > 0.0)
                    .count()
                    .to_string()
            }),
        )
    };
    let da_curva = move |indice: usize| {
        Row::new()
            .wrap()
            .gap(16)
            .visible(move || p.grafico_do_disco.get() == indice)
    };
    let numeros_da_leitura = da_curva(0)
        .child(numero(
            "desempenho-disco-leitura-numero",
            t!("processos-disco-leitura"),
            agora(p, |l| conta::bytes_por_segundo(l.disco().0)),
        ))
        .child(numero(
            "desempenho-disco-leituras-numero",
            t!("processos-disco-leituras"),
            operacoes(|(l, _)| l),
        ))
        .child(usando("desempenho-disco-usando"));
    let numeros_da_escrita = da_curva(1)
        .child(numero(
            "desempenho-disco-escrita-numero",
            t!("processos-disco-escrita"),
            agora(p, |l| conta::bytes_por_segundo(l.disco().1)),
        ))
        .child(numero(
            "desempenho-disco-escritas-numero",
            t!("processos-disco-escritas"),
            operacoes(|(_, e)| e),
        ))
        .child(usando("desempenho-disco-usando-escrita"));
    let numeros_das_operacoes = da_curva(2)
        .child(numero(
            "desempenho-disco-leituras-operacoes",
            t!("processos-disco-leituras"),
            operacoes(|(l, _)| l),
        ))
        .child(numero(
            "desempenho-disco-escritas-operacoes",
            t!("processos-disco-escritas"),
            operacoes(|(_, e)| e),
        ))
        .child(numero(
            "desempenho-disco-quantos-discos",
            t!("processos-disco-discos"),
            agora(p, |l| l.discos_do_total().count().to_string()),
        ));
    let numeros = Column::new()
        .child(numeros_da_leitura)
        .child(numeros_da_escrita)
        .child(numeros_das_operacoes);
    paineis(
        "desempenho-disco",
        curvas,
        p.grafico_do_disco,
        200.0,
        numeros,
    )
}

/// Os gráficos da aba Rede: o recebido e o enviado pela máquina no último minuto, somados das
/// placas físicas (ou de todas, se não houver nenhuma física).
fn graficos_da_rede(p: Processos) -> impl View + use<> {
    let recebendo = agora(p, |l| conta::bytes_por_segundo(l.rede().0));
    let enviando = agora(p, |l| conta::bytes_por_segundo(l.rede().1));
    let curvas = vec![
        Curva {
            id: "recebido",
            nome: t!("processos-rede-recebido"),
            titulo: t!("processos-rede-recebido-titulo"),
            cor: Token::SystemFillColorSuccess,
            serie: p.recebido,
            pontos: PONTOS,
            max: Rc::new(teto(p.recebido, 4096.0)),
            valor: Rc::new(recebendo),
            detalhe: None,
        },
        Curva {
            id: "enviado",
            nome: t!("processos-rede-enviado"),
            titulo: t!("processos-rede-enviado-titulo"),
            cor: Token::AccentFillColorDefault,
            serie: p.enviado,
            pontos: PONTOS,
            max: Rc::new(teto(p.enviado, 4096.0)),
            valor: Rc::new(enviando),
            detalhe: None,
        },
    ];
    let interfaces = move |id: &'static str| {
        numero(
            id,
            t!("processos-rede-interfaces"),
            agora(p, |l| l.interfaces.len().to_string()),
        )
    };
    let numeros_do_recebido = Row::new()
        .wrap()
        .gap(16)
        .visible(move || p.grafico_da_rede.get() == 0)
        .child(numero(
            "desempenho-rede-recebendo",
            t!("processos-rede-recebendo"),
            agora(p, |l| conta::bytes_por_segundo(l.rede().0)),
        ))
        .child(numero(
            "desempenho-rede-total-recebido",
            t!("processos-rede-total-recebido"),
            agora(p, |l| conta::memoria(l.rede_total().0)),
        ))
        .child(interfaces("desempenho-rede-interfaces-recebido"));
    let numeros_do_enviado = Row::new()
        .wrap()
        .gap(16)
        .visible(move || p.grafico_da_rede.get() == 1)
        .child(numero(
            "desempenho-rede-enviando",
            t!("processos-rede-enviando"),
            agora(p, |l| conta::bytes_por_segundo(l.rede().1)),
        ))
        .child(numero(
            "desempenho-rede-total-enviado",
            t!("processos-rede-total-enviado"),
            agora(p, |l| conta::memoria(l.rede_total().1)),
        ))
        .child(interfaces("desempenho-rede-interfaces-enviado"));
    paineis(
        "desempenho-rede",
        curvas,
        p.grafico_da_rede,
        200.0,
        Column::new()
            .child(numeros_do_recebido)
            .child(numeros_do_enviado),
    )
}

/// O resumo da máquina embaixo da barra.
fn resumo(p: Processos, aba: &'static str) -> impl View {
    Text::new(move || {
        p.quadro.with(|q| {
            let Some(l) = q else {
                return t!("processos-lendo");
            };
            if aba == REDE {
                let (recebendo, enviando) = l.rede();
                let (recebendo, enviando) = (
                    conta::bytes_por_segundo(recebendo),
                    conta::bytes_por_segundo(enviando),
                );
                return t!(
                    "processos-resumo-rede",
                    n = l.interfaces.len(),
                    recebendo = recebendo.as_str(),
                    enviando = enviando.as_str()
                );
            }
            if aba == DISCO {
                let (leitura, escrita) = l.disco();
                let (leitura, escrita) = (
                    conta::bytes_por_segundo(leitura),
                    conta::bytes_por_segundo(escrita),
                );
                return t!(
                    "processos-resumo-disco",
                    leitura = leitura.as_str(),
                    escrita = escrita.as_str()
                );
            }
            let (cpu, usada) = (conta::cpu(l.cpu()), conta::memoria(l.memoria_em_uso()));
            match l.memoria_total {
                Some(total) => {
                    let total = conta::memoria(total);
                    t!(
                        "processos-resumo",
                        n = l.linhas.len(),
                        cpu = cpu.as_str(),
                        usada = usada.as_str(),
                        total = total.as_str()
                    )
                }
                None => t!(
                    "processos-resumo-sem-total",
                    n = l.linhas.len(),
                    cpu = cpu.as_str(),
                    usada = usada.as_str()
                ),
            }
        })
    })
    .id(format!("desempenho-{aba}-resumo"))
    .caption()
    .secondary()
}

/// Os avisos de erro: o da leitura e o de encerrar.
fn avisos(p: Processos, aba: &'static str) -> impl View {
    Column::new()
        .gap(4)
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id(format!("desempenho-{aba}-erro"))
                .title(t!("processos-erro-ao-ler"))
                .message(move || p.erro.get().unwrap_or_default())
                .visible(move || p.erro.with(Option::is_some)),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id(format!("desempenho-{aba}-aviso"))
                .title(t!("processos-erro-ao-encerrar"))
                .message(move || p.aviso.get().unwrap_or_default())
                .on_close(move || p.aviso.set(None))
                .visible(move || p.aviso.with(Option::is_some)),
        )
}

/// O menu de contexto de uma linha: encerrar a linha (ou as escolhidas, se ela está entre
/// elas).
fn menu(
    tela: &Rc<Tela>,
    aba: &'static str,
    escolhidas: Signal<BTreeSet<String>>,
) -> impl Fn(&str) -> MenuFlyout + 'static {
    let tela = Rc::clone(tela);
    move |chave: &str| {
        // O menu nasce com a linha: a seleção se lê na hora do clique.
        let alvo = move |chave: &str| -> Vec<String> {
            escolhidas.with_untracked(|e| {
                if e.contains(chave) {
                    e.iter().cloned().collect()
                } else {
                    vec![chave.to_owned()]
                }
            })
        };
        let (t1, t2) = (Rc::clone(&tela), Rc::clone(&tela));
        let (c1, c2) = (chave.to_owned(), chave.to_owned());
        MenuFlyout::new()
            .item(
                format!("desempenho-{aba}-menu-{chave}-encerrar"),
                t!("processos-encerrar"),
                move || t1.encerrar_processos(&alvo(&c1), false),
            )
            .item(
                format!("desempenho-{aba}-menu-{chave}-forcar"),
                t!("processos-forcar"),
                move || t2.encerrar_processos(&alvo(&c2), true),
            )
    }
}

/// A aba Processos.
pub(super) fn aba_processos(tela: &Rc<Tela>, p: Processos) -> impl View + use<> {
    let apagar = Rc::clone(tela);
    let tabela = DataGrid::bind(p.linhas, p.escolhidas)
        .id("desempenho-processos-tabela")
        .grow(1)
        .columns([
            DataGridColumn::new("nome", t!("processos-nome")).width(ColumnWidth::Star(1.2)),
            DataGridColumn::new("pid", t!("processos-pid"))
                .width(ColumnWidth::Pixels(70.0))
                .align_end(),
            DataGridColumn::new("usuario", t!("processos-usuario"))
                .width(ColumnWidth::Pixels(100.0)),
            DataGridColumn::new("estado", t!("processos-estado")).width(ColumnWidth::Pixels(100.0)),
            DataGridColumn::new("cpu", t!("desempenho-cpu"))
                .width(ColumnWidth::Pixels(70.0))
                .align_end(),
            DataGridColumn::new("memoria", t!("desempenho-memoria"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("threads", t!("processos-threads"))
                .width(ColumnWidth::Pixels(80.0))
                .align_end(),
            DataGridColumn::new("comando", t!("processos-comando")).width(ColumnWidth::Star(2.0)),
        ])
        .sort(p.ordem)
        .empty_text(t!("processos-nenhum"))
        .item_context_flyout(menu(tela, PROCESSOS, p.escolhidas))
        .on_delete(move |chaves| apagar.encerrar_processos(&chaves, false));
    Column::new()
        .grow(1)
        .gap(8)
        .padding(8)
        .child(barra(tela, p, PROCESSOS, p.escolhidas))
        .child(graficos_dos_processos(p))
        .child(resumo(p, PROCESSOS))
        .child(avisos(p, PROCESSOS))
        .child(tabela)
}

/// A aba Disco.
pub(super) fn aba_disco(tela: &Rc<Tela>, p: Processos) -> impl View + use<> {
    let apagar = Rc::clone(tela);
    let operacoes = OPERACOES_POR_PROCESSO.then(|| {
        [
            DataGridColumn::new("leituras", t!("processos-disco-leituras"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("escritas", t!("processos-disco-escritas"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
        ]
    });
    let colunas = [
        DataGridColumn::new("nome", t!("processos-nome")).width(ColumnWidth::Star(1.2)),
        DataGridColumn::new("pid", t!("processos-pid"))
            .width(ColumnWidth::Pixels(70.0))
            .align_end(),
        DataGridColumn::new("usuario", t!("processos-usuario")).width(ColumnWidth::Pixels(100.0)),
        DataGridColumn::new("leitura", t!("processos-leitura"))
            .width(ColumnWidth::Pixels(100.0))
            .align_end(),
        DataGridColumn::new("escrita", t!("processos-escrita"))
            .width(ColumnWidth::Pixels(100.0))
            .align_end(),
    ]
    .into_iter()
    .chain(operacoes.into_iter().flatten())
    .chain([
        DataGridColumn::new("lido", t!("processos-total-lido"))
            .width(ColumnWidth::Pixels(100.0))
            .align_end(),
        DataGridColumn::new("escrito", t!("processos-total-escrito"))
            .width(ColumnWidth::Pixels(100.0))
            .align_end(),
        DataGridColumn::new("comando", t!("processos-comando")).width(ColumnWidth::Star(2.0)),
    ]);
    let tabela = DataGrid::bind(p.linhas_do_disco, p.escolhidas_do_disco)
        .id("desempenho-disco-tabela")
        .grow(1)
        .columns(colunas)
        .sort(p.ordem_do_disco)
        .empty_text(t!("processos-nenhum"))
        .item_context_flyout(menu(tela, DISCO, p.escolhidas_do_disco))
        .on_delete(move |chaves| apagar.encerrar_processos(&chaves, false));
    let discos = DataGrid::bind(p.linhas_dos_discos, p.escolhidos_dos_discos)
        .id("desempenho-disco-discos")
        .grow(1)
        .columns([
            DataGridColumn::new("nome", t!("processos-disco-disco")).width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("tipo", t!("processos-disco-tipo"))
                .width(ColumnWidth::Pixels(90.0)),
            DataGridColumn::new("leituras", t!("processos-disco-leituras"))
                .width(ColumnWidth::Pixels(100.0))
                .align_end(),
            DataGridColumn::new("escritas", t!("processos-disco-escritas"))
                .width(ColumnWidth::Pixels(100.0))
                .align_end(),
            DataGridColumn::new("lendo", t!("processos-disco-lendo"))
                .width(ColumnWidth::Pixels(120.0))
                .align_end(),
            DataGridColumn::new("escrevendo", t!("processos-disco-escrevendo"))
                .width(ColumnWidth::Pixels(120.0))
                .align_end(),
            DataGridColumn::new("historico", t!("desempenho-historico"))
                .width(ColumnWidth::Pixels(140.0)),
        ])
        .sort(p.ordem_dos_discos)
        .empty_text(t!("processos-disco-nenhum"));
    let tabelas = Split::column(discos, tabela)
        .id("desempenho-disco-divisao")
        .size(p.altura_dos_discos)
        .min_first(56.0)
        .min_second(48.0)
        .grow(1);
    Column::new()
        .grow(1)
        .gap(8)
        .padding(8)
        .child(barra(tela, p, DISCO, p.escolhidas_do_disco))
        .child(graficos_do_disco(p))
        .child(resumo(p, DISCO))
        .child(avisos(p, DISCO))
        .child(tabelas)
}

/// A aba Rede.
pub(super) fn aba_rede(tela: &Rc<Tela>, p: Processos) -> impl View + use<> {
    let tabela = DataGrid::bind(p.linhas_da_rede, p.escolhidas_da_rede)
        .id("desempenho-rede-tabela")
        .grow(1)
        .columns([
            DataGridColumn::new("nome", t!("processos-rede-interface"))
                .width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("tipo", t!("processos-rede-tipo")).width(ColumnWidth::Pixels(90.0)),
            DataGridColumn::new("recebendo", t!("processos-rede-recebendo"))
                .width(ColumnWidth::Pixels(130.0))
                .align_end(),
            DataGridColumn::new("enviando", t!("processos-rede-enviando"))
                .width(ColumnWidth::Pixels(130.0))
                .align_end(),
            DataGridColumn::new("recebidos", t!("processos-rede-total-recebido"))
                .width(ColumnWidth::Pixels(120.0))
                .align_end(),
            DataGridColumn::new("enviados", t!("processos-rede-total-enviado"))
                .width(ColumnWidth::Pixels(120.0))
                .align_end(),
            DataGridColumn::new("historico", t!("desempenho-historico"))
                .width(ColumnWidth::Pixels(140.0)),
        ])
        .sort(p.ordem_da_rede)
        .empty_text(t!("processos-rede-nenhuma"));
    Column::new()
        .grow(1)
        .gap(8)
        .padding(8)
        .child(barra(tela, p, REDE, p.escolhidas_da_rede))
        .child(graficos_da_rede(p))
        .child(resumo(p, REDE))
        .child(avisos(p, REDE))
        .child(tabela)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::processos::Processo;

    fn linha(pid: u32, pai: u32, nome: &str) -> Linha {
        Linha {
            processo: Processo {
                pid,
                pai,
                nome: nome.to_owned(),
                ..Processo::default()
            },
            cpu: None,
            leitura: None,
            escrita: None,
            operacoes: None,
        }
    }

    /// Os gráficos dos núcleos ficam numa área mais larga que alta, sem coluna sobrando.
    #[test]
    fn as_colunas_dos_nucleos_seguem_a_area() {
        let colunas: Vec<usize> = [1, 4, 8, 12, 16, 24, 32]
            .into_iter()
            .map(colunas_dos_nucleos)
            .collect();
        assert_eq!(colunas, [1, 4, 8, 6, 8, 12, 11]);
    }

    /// A tabela de processos da aba Disco tem as operações por segundo onde a fonte conta
    /// (o Windows) e não tem nos outros sistemas.
    #[test]
    fn as_operacoes_por_processo_so_onde_a_fonte_conta() {
        let mut rsync = linha(900, 1, "rsync");
        rsync.leitura = Some(1024.0);
        rsync.escrita = Some(2048.0);
        rsync.operacoes = Some((4.0, 12.4));
        let leitura = Leitura {
            linhas: vec![rsync],
            ..Leitura::default()
        };
        let textos = |operacoes| -> Vec<String> {
            linhas_do_disco(&leitura, "", false, operacoes)[0]
                .cells
                .iter()
                .map(|c| c.0.clone())
                .collect()
        };
        let com = textos(true);
        assert_eq!(com[5..7], ["4/s", "12/s"]);
        assert_eq!(com.len(), textos(false).len() + 2);
    }

    fn desenho(linhas: &[Linha], filtro: &str) -> Vec<String> {
        em_arvore(linhas, filtro)
            .into_iter()
            .map(|(prefixo, l)| format!("{prefixo}{}", l.processo.nome))
            .collect()
    }

    #[test]
    fn a_arvore_desenha_os_ramos() {
        let linhas = vec![
            linha(1, 0, "init"),
            linha(30, 1, "cron"),
            linha(10, 1, "sshd"),
            linha(11, 10, "bash"),
            linha(12, 11, "vim"),
            linha(13, 10, "bash2"),
        ];
        assert_eq!(
            desenho(&linhas, ""),
            vec![
                "init",
                "├─ sshd",
                "│  ├─ bash",
                "│  │  └─ vim",
                "│  └─ bash2",
                "└─ cron",
            ]
        );
    }

    #[test]
    fn com_filtro_a_arvore_guarda_os_pais() {
        let linhas = vec![
            linha(1, 0, "init"),
            linha(30, 1, "cron"),
            linha(10, 1, "sshd"),
            linha(12, 10, "vim"),
        ];
        assert_eq!(
            desenho(&linhas, "vim"),
            vec!["init", "└─ sshd", "   └─ vim"]
        );
    }

    #[test]
    fn um_pai_que_e_o_proprio_processo_nao_trava() {
        let linhas = vec![linha(0, 0, "ocioso"), linha(4, 0, "system")];
        assert_eq!(desenho(&linhas, ""), vec!["ocioso", "└─ system"]);
    }
}
