//! O medidor de CPU da barra de título e a página Desempenho, como a do Gerenciador de Tarefas:
//! à esquerda a CPU, a memória e a rede com a série de cada uma; à direita o gráfico grande da
//! escolhida, com os números embaixo; e a tabela "Por terminal", com o peso de cada aba. As
//! contas estão em [`crate::desempenho`].
//!
//! As amostras só correm com o medidor ligado (`geral.medidor_cpu`) ou com a página aberta: uma
//! thread bate a cada segundo e para quando nenhum dos dois pede.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tomni_ui::files::Live;
use tomni_ui::prelude::*;

use super::Tela;
use crate::desempenho::{self as conta, Amostra, Amostrador, PONTOS, RITMO};
use crate::espacos::id_da_chave;
use crate::t;

/// O id da página na navegação.
pub(super) const PAGINA: &str = "desempenho";

/// A chave da linha "Interface" da tabela (o que não é de nenhuma aba).
const INTERFACE: &str = "interface";

/// Os três gráficos da coluna da esquerda.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Grafico {
    Cpu,
    Memoria,
    Rede,
}

impl Grafico {
    fn id(self) -> &'static str {
        match self {
            Grafico::Cpu => "cpu",
            Grafico::Memoria => "memoria",
            Grafico::Rede => "rede",
        }
    }

    fn cor(self) -> Token {
        match self {
            Grafico::Cpu => Token::AccentFillColorDefault,
            Grafico::Memoria => Token::SystemFillColorCaution,
            Grafico::Rede => Token::SystemFillColorSuccess,
        }
    }
}

/// O que a página guarda enquanto o app está aberto.
#[derive(Clone, Copy)]
pub(super) struct Desempenho {
    /// A CPU do app, em % da máquina, do ponto mais antigo ao mais novo.
    cpu: Signal<Vec<f64>>,
    /// A memória, em MB.
    memoria: Signal<Vec<f64>>,
    /// Os dados de todas as abas, em KB/s.
    rede: Signal<Vec<f64>>,
    /// A amostra mais nova.
    ultima: Signal<Option<Amostra>>,
    /// Quanto o app está aberto (o relógio da fonte).
    aberto: Signal<Duration>,
    /// As linhas da tabela "Por terminal".
    linhas: Signal<Vec<DataGridRow>>,
    /// A linha escolhida na tabela.
    escolhidas: Signal<BTreeSet<String>>,
    /// A ordem da tabela: a CPU, da maior para a menor.
    ordem: Signal<Option<(String, SortDirection)>>,
    /// O gráfico grande.
    grafico: Signal<Grafico>,
}

/// Se a amostragem deve correr: o medidor ligado ou a página aberta.
fn ativo(tela: &Tela) -> impl Fn() -> bool + Copy + 'static {
    let (config, pagina) = (tela.config, tela.pagina);
    move || config.with(|c| c.geral.medidor_cpu) || pagina.with(|p| p == PAGINA)
}

/// Põe `valor` no fim da série, que guarda os [`PONTOS`] mais novos.
fn empurrar(serie: &mut Vec<f64>, valor: f64) {
    serie.push(valor);
    if serie.len() > PONTOS {
        serie.drain(..serie.len() - PONTOS);
    }
}

/// O estado e os efeitos: a batida (a thread de verdade ou a do teste) e a amostra a cada uma.
pub(super) fn ligar(tela: &Rc<Tela>) -> Desempenho {
    let d = Desempenho {
        cpu: signal(Vec::new()),
        memoria: signal(Vec::new()),
        rede: signal(Vec::new()),
        ultima: signal(None),
        aberto: signal(Duration::ZERO),
        linhas: signal(Vec::new()),
        escolhidas: signal(BTreeSet::new()),
        ordem: signal(Some(("cpu".to_owned(), SortDirection::Descending))),
        grafico: signal(Grafico::Cpu),
    };
    let medicao = tela.ferramentas.ambiente().medicao.clone();
    let ativo = ativo(tela);
    let batida = match medicao.batida {
        Some(batida) => batida,
        None => batida_do_sistema(ativo),
    };
    let amostrador = Rc::new(RefCell::new(Amostrador::novo()));
    let historicos: Rc<RefCell<HashMap<String, Vec<f64>>>> = Rc::default();
    let t = Rc::clone(tela);
    effect(move || {
        batida.get();
        if !ativo() {
            // Parado, a próxima leitura recomeça do zero (sem um intervalo de minutos).
            *amostrador.borrow_mut() = Amostrador::novo();
            return;
        }
        untrack(|| {
            let fonte = &medicao.fonte;
            let sessoes = t.sessoes_das_janelas(None);
            let leituras = sessoes
                .iter()
                .map(|(_, s)| (s.key_str(), fonte.aba(*s)))
                .collect();
            let amostra = amostrador.borrow_mut().amostrar(
                fonte.agora(),
                fonte.nucleos(),
                fonte.processo(),
                leituras,
            );
            d.aberto.set(fonte.agora());
            let Some(amostra) = amostra else {
                return;
            };
            d.cpu.update(|s| empurrar(s, amostra.cpu));
            if let Some(bytes) = amostra.memoria {
                d.memoria
                    .update(|s| empurrar(s, bytes as f64 / (1024.0 * 1024.0)));
            }
            d.rede.update(|s| empurrar(s, amostra.dados / 1024.0));
            let linhas = {
                let mut historicos = historicos.borrow_mut();
                historicos.retain(|chave, _| {
                    chave == INTERFACE || amostra.abas.iter().any(|a| &a.chave == chave)
                });
                for aba in &amostra.abas {
                    empurrar(historicos.entry(aba.chave.clone()).or_default(), aba.cpu);
                }
                empurrar(
                    historicos.entry(INTERFACE.to_owned()).or_default(),
                    amostra.interface,
                );
                linhas_da_tabela(&t, &amostra, &historicos, fonte.nucleos())
            };
            d.linhas.set(linhas);
            d.ultima.set(Some(amostra));
        });
    });
    d
}

/// A batida de verdade: uma thread que sobe o contador a cada [`RITMO`] enquanto `ativo()`.
/// Quando para de ser pedida a thread acaba; quando volta, outra nasce.
fn batida_do_sistema(ativo: impl Fn() -> bool + 'static) -> Live<u64> {
    let batida = Live::new(0_u64);
    let pedida = Arc::new(AtomicBool::new(false));
    let rodando = Arc::new(AtomicBool::new(false));
    {
        let batida = batida.clone();
        effect(move || {
            let agora = ativo();
            pedida.store(agora, Ordering::SeqCst);
            if !agora || rodando.swap(true, Ordering::SeqCst) {
                return;
            }
            let (pedida, rodando, batida) =
                (Arc::clone(&pedida), Arc::clone(&rodando), batida.clone());
            std::thread::Builder::new()
                .name("desempenho".to_owned())
                .spawn(move || {
                    loop {
                        while pedida.load(Ordering::SeqCst) {
                            std::thread::sleep(RITMO);
                            batida.update(|n| *n += 1);
                        }
                        rodando.store(false, Ordering::SeqCst);
                        // Pedida de novo entre o fim do laço e a linha de cima: esta continua.
                        if !pedida.load(Ordering::SeqCst) || rodando.swap(true, Ordering::SeqCst) {
                            break;
                        }
                    }
                })
                .ok();
        });
    }
    batida
}

/// As linhas da tabela: uma por aba e a "Interface".
fn linhas_da_tabela(
    tela: &Tela,
    amostra: &Amostra,
    historicos: &HashMap<String, Vec<f64>>,
    nucleos: usize,
) -> Vec<DataGridRow> {
    let titulos: HashMap<String, String> = tela
        .sessoes_das_janelas(None)
        .into_iter()
        .map(|(_, s)| (s.key_str(), s.title()))
        .collect();
    // Uma aba que usa um núcleo inteiro fica no calor máximo.
    let por_nucleo = 100.0 / nucleos.max(1) as f64;
    let calor = move |cpu: f64| (cpu / por_nucleo).clamp(0.0, 1.0);
    let serie = |chave: &str| {
        let valores = historicos.get(chave).cloned().unwrap_or_default();
        let teto = valores.iter().copied().fold(por_nucleo / 4.0, f64::max);
        valores.into_iter().map(|v| v / teto).collect::<Vec<_>>()
    };
    let mut linhas: Vec<DataGridRow> = amostra
        .abas
        .iter()
        .map(|aba| {
            let titulo = titulos.get(&aba.chave).cloned().unwrap_or_default();
            let conexao = id_da_chave(&aba.chave)
                .and_then(|id| {
                    tela.tree
                        .with_untracked(|a| a.connection(id).map(|c| c.name.clone()))
                })
                .unwrap_or_else(|| t!("desempenho-sem-conexao"));
            let programa = aba.programa.map_or_else(|| "—".to_owned(), conta::cpu);
            DataGridRow::new(aba.chave.clone())
                .cell(titulo.clone(), CellValue::Text(titulo))
                .cell(conexao.clone(), CellValue::Text(conexao))
                .cell(conta::cpu(aba.cpu), CellValue::Number(aba.cpu))
                .heat(calor(aba.cpu))
                .cell(
                    conta::bytes_por_segundo(aba.dados),
                    CellValue::Number(aba.dados),
                )
                .heat((aba.dados / (1024.0 * 1024.0)).clamp(0.0, 1.0))
                .cell(programa, CellValue::Number(aba.programa.unwrap_or(-1.0)))
                .heat(aba.programa.map_or(0.0, calor))
                .cell(
                    t!(
                        "desempenho-historico-de",
                        aba = titulos.get(&aba.chave).map_or("", String::as_str)
                    ),
                    CellValue::Sparkline(serie(&aba.chave)),
                )
        })
        .collect();
    let interface = t!("desempenho-interface");
    linhas.push(
        DataGridRow::new(INTERFACE)
            .icon(Symbol::Window)
            .cell(interface.clone(), CellValue::Text(interface.clone()))
            .cell(
                t!("desempenho-o-app"),
                CellValue::Text(t!("desempenho-o-app")),
            )
            .cell(
                conta::cpu(amostra.interface),
                CellValue::Number(amostra.interface),
            )
            .heat(calor(amostra.interface))
            .cell("—", CellValue::Number(-1.0))
            .cell("—", CellValue::Number(-1.0))
            .cell(
                t!("desempenho-historico-de", aba = interface.as_str()),
                CellValue::Sparkline(serie(INTERFACE)),
            ),
    );
    linhas
}

/// O medidor da barra de título: os quadradinhos da CPU do app, com a CPU em % na dica. O clique
/// abre a página Desempenho. Some com `geral.medidor_cpu` desligado.
pub(super) fn medidor(tela: &Rc<Tela>, d: Desempenho) -> impl View + use<> {
    let config = tela.config;
    let atual = move || d.cpu.with(|s| s.last().copied().unwrap_or(0.0));
    let abrir = Rc::clone(tela);
    LevelMeter::new(move || atual() / 100.0)
        .id("barra-de-titulo-cpu")
        .tooltip(move || {
            let cpu = conta::cpu(atual());
            t!("desempenho-medidor-dica", cpu = cpu.as_str())
        })
        .visible(move || config.with(|c| c.geral.medidor_cpu))
        .on_click(move || abrir.na_principal(|t| t.pagina.set(PAGINA.to_owned())))
}

/// Um item da coluna da esquerda: a série pequena, o nome e o número de agora. O clique põe o
/// gráfico no lado direito; o escolhido tem a barrinha de destaque, como na navegação.
fn item(
    d: Desempenho,
    grafico: Grafico,
    titulo: String,
    serie: Signal<Vec<f64>>,
    max: impl Fn() -> f64 + 'static,
    valor: impl Fn() -> String + 'static,
) -> impl View {
    let escolhido = move || d.grafico.get() == grafico;
    Card::new()
        .id(format!("desempenho-{}", grafico.id()))
        .label(titulo.clone())
        .padding(8)
        .on_click(move || d.grafico.set(grafico))
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Border::new()
                        .width(3)
                        .height(16)
                        .corner_radius(2)
                        .background(Token::AccentFillColorDefault)
                        .visible(escolhido),
                )
                .child(
                    Sparkline::new(serie)
                        .id(format!("desempenho-{}-serie", grafico.id()))
                        .max(max)
                        .color(grafico.cor()),
                )
                .child(
                    Column::new()
                        .grow(1)
                        .child(Text::new(titulo).body_strong())
                        .child(
                            Text::new(valor)
                                .id(format!("desempenho-{}-valor", grafico.id()))
                                .caption()
                                .secondary(),
                        ),
                ),
        )
}

/// O teto da escala da memória e da rede: o maior ponto com folga, para a curva não colar no
/// alto. Nunca zero.
fn teto(serie: Signal<Vec<f64>>, minimo: f64) -> impl Fn() -> f64 + Copy + 'static {
    move || serie.with(|s| s.iter().copied().fold(minimo, f64::max) * 1.25)
}

/// Um número embaixo do gráfico grande: o rótulo pequeno em cima e o valor.
fn numero(id: &str, rotulo: String, valor: impl Fn() -> String + 'static) -> impl View {
    Column::new()
        .width(140)
        .child(Text::new(rotulo).caption().secondary())
        .child(Text::new(valor).id(id.to_owned()).subtitle())
}

/// O gráfico grande de `g`, que só aparece quando é o escolhido.
fn grafico(
    d: Desempenho,
    g: Grafico,
    serie: Signal<Vec<f64>>,
    max: impl Fn() -> f64 + 'static,
) -> impl View {
    AreaChart::new(serie)
        .id(format!("desempenho-grafico-{}", g.id()))
        .max(max)
        .color(g.cor())
        .grow(1)
        .visible(move || d.grafico.get() == g)
}

/// "1:02:03" ou "2:03".
fn duracao(tempo: Duration) -> String {
    let s = tempo.as_secs();
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// A página Desempenho.
pub(super) fn pagina(tela: &Rc<Tela>, d: Desempenho) -> impl View + use<> {
    let ultima = d.ultima;
    let cpu_agora = move || ultima.with(|u| u.as_ref().map_or(0.0, |a| a.cpu));
    let memoria_agora = move || {
        ultima.with(|u| {
            u.as_ref()
                .and_then(|a| a.memoria)
                .map_or_else(|| "—".to_owned(), conta::memoria)
        })
    };
    let rede_agora =
        move || ultima.with(|u| conta::bytes_por_segundo(u.as_ref().map_or(0.0, |a| a.dados)));
    let (teto_memoria, teto_rede) = (teto(d.memoria, 64.0), teto(d.rede, 4.0));

    let esquerda = Column::new()
        .gap(4)
        .width(220)
        .child(item(
            d,
            Grafico::Cpu,
            t!("desempenho-cpu"),
            d.cpu,
            || 100.0,
            move || conta::cpu(cpu_agora()),
        ))
        .child(item(
            d,
            Grafico::Memoria,
            t!("desempenho-memoria"),
            d.memoria,
            teto_memoria,
            memoria_agora,
        ))
        .child(item(
            d,
            Grafico::Rede,
            t!("desempenho-rede"),
            d.rede,
            teto_rede,
            rede_agora,
        ));

    let titulo_do_grafico = move || match d.grafico.get() {
        Grafico::Cpu => t!("desempenho-cpu-titulo"),
        Grafico::Memoria => t!("desempenho-memoria-titulo"),
        Grafico::Rede => t!("desempenho-rede-titulo"),
    };
    let abas = {
        let t = Rc::clone(tela);
        move || {
            t.janelas
                .todas()
                .iter()
                .map(|j| j.layout.try_with(|l| l.sessions().len()).unwrap_or(0))
                .sum::<usize>()
                .to_string()
        }
    };
    let direita = Column::new()
        .grow(1)
        .gap(8)
        .child(
            Row::new()
                .align(Align::End)
                .child(Text::new(titulo_do_grafico).subtitle().grow(1))
                .child(
                    Text::new(t!("desempenho-60-segundos"))
                        .caption()
                        .secondary(),
                ),
        )
        .child(
            Row::new()
                .grow(1)
                .child(grafico(d, Grafico::Cpu, d.cpu, || 100.0))
                .child(grafico(d, Grafico::Memoria, d.memoria, teto_memoria))
                .child(grafico(d, Grafico::Rede, d.rede, teto_rede)),
        )
        .child(
            Row::new()
                .gap(16)
                .child(numero(
                    "desempenho-utilizacao",
                    t!("desempenho-utilizacao"),
                    move || conta::cpu(cpu_agora()),
                ))
                .child(numero(
                    "desempenho-memoria-numero",
                    t!("desempenho-memoria"),
                    memoria_agora,
                ))
                .child(numero(
                    "desempenho-abas",
                    t!("desempenho-abas-abertas"),
                    abas,
                ))
                .child(numero(
                    "desempenho-aberto",
                    t!("desempenho-tempo-aberto"),
                    move || duracao(d.aberto.get()),
                )),
        );

    let ir = Rc::clone(tela);
    let tabela = DataGrid::bind(d.linhas, d.escolhidas)
        .id("desempenho-terminais")
        .grow(1)
        .columns([
            DataGridColumn::new("aba", t!("desempenho-aba")).width(ColumnWidth::Star(1.2)),
            DataGridColumn::new("conexao", t!("desempenho-conexao")).width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("cpu", t!("desempenho-cpu"))
                .width(ColumnWidth::Pixels(80.0))
                .align_end(),
            DataGridColumn::new("dados", t!("desempenho-dados"))
                .width(ColumnWidth::Pixels(100.0))
                .align_end(),
            DataGridColumn::new("programa", t!("desempenho-programa"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("historico", t!("desempenho-historico"))
                .width(ColumnWidth::Pixels(120.0))
                .not_sortable(),
        ])
        .sort(d.ordem)
        .empty_text(t!("desempenho-sem-abas"))
        .on_invoke(move |chave| ir.ir_para_a_aba_da_chave(&chave));

    Column::new()
        .id("desempenho-pagina")
        .grow(1)
        .gap(12)
        .padding(16)
        .child(
            Text::new(t!("desempenho-titulo"))
                .title()
                .id("desempenho-titulo"),
        )
        .child(Text::new(t!("desempenho-descricao")).caption().wrap())
        .child(
            Row::new()
                .gap(16)
                .height(260)
                .child(esquerda)
                .child(direita),
        )
        .child(Text::new(t!("desempenho-por-terminal")).subtitle())
        .child(tabela)
}

impl Tela {
    /// O clique duplo numa linha da tabela: a aba de chave `chave` fica escolhida, a janela dela
    /// ganha o foco e, na principal, a página volta aos terminais.
    fn ir_para_a_aba_da_chave(&self, chave: &str) {
        let Some((janela, sessao)) = self
            .sessoes_das_janelas(None)
            .into_iter()
            .find(|(_, s)| s.key_str() == chave)
        else {
            return;
        };
        janela.layout.update(|l| l.select(sessao));
        if janela.layout == self.janelas.principal().layout {
            self.pagina.set(super::paineis::TERMINAIS.to_owned());
        }
        janela.handle.focus();
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_serie_guarda_so_os_pontos_mais_novos() {
        let mut s = Vec::new();
        for i in 0..PONTOS + 5 {
            empurrar(&mut s, i as f64);
        }
        assert_eq!(s.len(), PONTOS);
        assert_eq!(s[0], 5.0);
    }

    #[test]
    fn a_duracao_aberta() {
        assert_eq!(duracao(Duration::from_secs(5)), "0:05");
        assert_eq!(duracao(Duration::from_secs(3_723)), "1:02:03");
    }
}
