//! A aba Sensores da página Desempenho: as temperaturas da máquina escolhida por grupo (CPU,
//! memória, disco, placa-mãe, vídeo e outros), com o gráfico do último minuto do sensor
//! principal de cada grupo e a lista dos sensores do grupo escolhido, com a cor dos limites do
//! chip. O que não tem sensor não aparece: o grupo vazio, a ventoinha que nunca girou e, na
//! página, a aba inteira quando este computador não tem sensores e não há sessão SSH. As
//! leituras estão em [`crate::sensores`].

use std::collections::BTreeSet;
use std::rc::Rc;

use tomni_ui::prelude::*;

use super::{Curva, PONTOS, Processos, SENSORES, Tela, avisos, barra, numero, paineis, teto};
use crate::sensores::{Grupo, Nivel, Nome, Sensores};
use crate::t;

/// Uma temperatura em graus inteiros: "86 °C".
fn graus(celsius: f64) -> String {
    format!("{celsius:.0} °C")
}

/// Uma temperatura em milésimos de grau, como o kernel dá.
pub(in crate::tela) fn graus_mili(mili: i32) -> String {
    graus(f64::from(mili) / 1000.0)
}

/// Põe a página na aba Sensores deste computador (o clique no aviso de calor).
pub(in crate::tela) fn mostrar_sensores_daqui(p: Processos) {
    p.maquina.set(t!("processos-este-computador"));
    p.aba.set(SENSORES.to_owned());
}

/// O nome de um grupo na tela.
fn nome_do_grupo(grupo: Grupo) -> String {
    match grupo {
        Grupo::Cpu => t!("sensores-grupo-cpu"),
        Grupo::Memoria => t!("sensores-grupo-memoria"),
        Grupo::Disco => t!("sensores-grupo-disco"),
        Grupo::PlacaMae => t!("sensores-grupo-placa-mae"),
        Grupo::Video => t!("sensores-grupo-video"),
        Grupo::Outros => t!("sensores-grupo-outros"),
    }
}

/// O título do gráfico grande de um grupo.
fn titulo_do_grupo(grupo: Grupo) -> String {
    match grupo {
        Grupo::Cpu => t!("sensores-titulo-cpu"),
        Grupo::Memoria => t!("sensores-titulo-memoria"),
        Grupo::Disco => t!("sensores-titulo-disco"),
        Grupo::PlacaMae => t!("sensores-titulo-placa-mae"),
        Grupo::Video => t!("sensores-titulo-video"),
        Grupo::Outros => t!("sensores-titulo-outros"),
    }
}

/// A cor da curva de um grupo.
fn cor_do_grupo(grupo: Grupo) -> Token {
    match grupo {
        Grupo::Cpu => Token::AccentFillColorDefault,
        Grupo::Memoria => Token::SystemFillColorCaution,
        Grupo::Disco => Token::SystemFillColorSuccess,
        Grupo::PlacaMae => Token::SystemFillColorAttention,
        Grupo::Video => Token::SystemFillColorCritical,
        Grupo::Outros => Token::TextFillColorSecondary,
    }
}

/// O nome de um sensor na tela.
fn nome_do_sensor(nome: &Nome) -> String {
    match nome {
        Nome::Pacote(None) => t!("sensores-pacote"),
        Nome::Pacote(Some(n)) => t!("sensores-pacote-n", n = *n),
        Nome::Nucleo(n) => t!("sensores-nucleo", n = *n),
        Nome::Pente(n) => t!("sensores-pente", n = *n),
        Nome::PlacaMae => t!("sensores-placa-mae"),
        Nome::Soquete => t!("sensores-soquete"),
        Nome::ZonaAcpi => t!("sensores-zona-acpi"),
        Nome::Bateria => t!("sensores-bateria"),
        Nome::Texto(texto) => texto.clone(),
    }
}

/// As linhas da lista: os sensores de `grupo` e, na placa-mãe, as ventoinhas que já giraram.
fn linhas(sensores: &Sensores, grupo: Grupo, vistas: &BTreeSet<(String, u32)>) -> Vec<DataGridRow> {
    let limite = |valor: Option<i32>| match valor {
        Some(mili) => (graus_mili(mili), CellValue::Number(f64::from(mili))),
        None => ("—".to_owned(), CellValue::Number(-1.0)),
    };
    let mut linhas: Vec<DataGridRow> = sensores
        .do_grupo(grupo)
        .enumerate()
        .map(|(n, sensor)| {
            let nome = nome_do_sensor(&sensor.nome);
            let (alto, alto_valor) = limite(sensor.alto);
            let (critico, critico_valor) = limite(sensor.critico);
            let linha = DataGridRow::new(format!("{}-{n}", sensor.chip))
                .cell(nome.clone(), CellValue::Text(nome))
                .cell(
                    graus_mili(sensor.mili),
                    CellValue::Number(f64::from(sensor.mili)),
                )
                .cell(alto, alto_valor)
                .cell(critico, critico_valor)
                .cell(sensor.chip.clone(), CellValue::Text(sensor.chip.clone()));
            match sensor.nivel() {
                Nivel::Critico => linha.tone(RowTone::Critical),
                Nivel::Alto => linha.tone(RowTone::Caution),
                Nivel::Normal => linha,
            }
        })
        .collect();
    if grupo == Grupo::PlacaMae {
        let giraram = sensores
            .ventoinhas
            .iter()
            .filter(|v| vistas.contains(&(v.chip.clone(), v.numero)));
        for ventoinha in giraram {
            let nome = ventoinha
                .rotulo
                .clone()
                .unwrap_or_else(|| t!("sensores-ventoinha", n = ventoinha.numero));
            let (traco, sem_valor) = limite(None);
            linhas.push(
                DataGridRow::new(format!("{}-ventoinha-{}", ventoinha.chip, ventoinha.numero))
                    .cell(nome.clone(), CellValue::Text(nome))
                    .cell(
                        t!("sensores-rpm", rpm = ventoinha.rpm),
                        CellValue::Number(f64::from(ventoinha.rpm)),
                    )
                    .cell(traco.clone(), sem_valor.clone())
                    .cell(traco, sem_valor)
                    .cell(
                        ventoinha.chip.clone(),
                        CellValue::Text(ventoinha.chip.clone()),
                    ),
            );
        }
    }
    linhas
}

/// O efeito da lista: as linhas do grupo escolhido a cada leitura. Se o grupo escolhido sumiu
/// (outra máquina), a escolha volta ao primeiro.
pub(super) fn ligar(p: Processos) {
    effect(move || {
        let escolhido = p.grafico_dos_sensores.get();
        let vistas = p.ventoinhas_vistas.get();
        let novas = p.quadro.with(|quadro| {
            let leitura = quadro.as_ref()?;
            let grupos = leitura.sensores.grupos();
            match grupos.get(escolhido) {
                Some(&grupo) => Some(Ok(linhas(&leitura.sensores, grupo, &vistas))),
                None if escolhido > 0 => Some(Err(())),
                None => None,
            }
        });
        match novas {
            Some(Ok(novas)) => p.linhas_dos_sensores.set(novas),
            Some(Err(())) => p.grafico_dos_sensores.set(0),
            None => p.linhas_dos_sensores.set(Vec::new()),
        }
    });
}

/// Os grupos da leitura de agora; nenhum antes da primeira.
fn grupos_de_agora(p: Processos) -> Vec<Grupo> {
    p.quadro.with(|quadro| {
        quadro
            .as_ref()
            .map(|l| l.sensores.grupos())
            .unwrap_or_default()
    })
}

/// Os painéis dos `grupos`: um item por grupo, o gráfico do escolhido e, embaixo, a
/// temperatura de agora, a maior do minuto e os limites do chip.
fn painel(p: Processos, grupos: Vec<Grupo>) -> impl View + use<> {
    let agora = move |grupo: Grupo| {
        move || {
            p.quadro.with(|quadro| {
                quadro
                    .as_ref()
                    .and_then(|l| l.sensores.principal(grupo))
                    .map_or_else(|| "—".to_owned(), |s| graus_mili(s.mili))
            })
        }
    };
    let curvas = grupos
        .iter()
        .map(|&grupo| {
            let serie = p.temperaturas[grupo as usize];
            Curva {
                id: grupo.chave(),
                nome: nome_do_grupo(grupo),
                titulo: titulo_do_grupo(grupo),
                cor: cor_do_grupo(grupo),
                serie,
                pontos: PONTOS,
                max: Rc::new(teto(serie, 80.0)),
                valor: Rc::new(agora(grupo)),
                detalhe: None,
            }
        })
        .collect();
    // Cada item da coluna da esquerda tem uns 62 px (com o espaço): com mais de três grupos, a
    // altura cresce para nenhum ficar cortado.
    let altura = (grupos.len() as f64 * 62.0).max(200.0);
    let grupos = Rc::new(grupos);
    let escolhido = move || {
        let grupos = Rc::clone(&grupos);
        move || grupos.get(p.grafico_dos_sensores.get()).copied()
    };
    let do_principal = |f: fn(&crate::sensores::Sensor) -> Option<i32>| {
        let escolhido = escolhido();
        move || {
            let Some(grupo) = escolhido() else {
                return "—".to_owned();
            };
            p.quadro.with(|quadro| {
                quadro
                    .as_ref()
                    .and_then(|l| l.sensores.principal(grupo))
                    .and_then(f)
                    .map_or_else(|| "—".to_owned(), graus_mili)
            })
        }
    };
    let maior = {
        let escolhido = escolhido();
        move || {
            escolhido()
                .and_then(|grupo| {
                    p.temperaturas[grupo as usize]
                        .with(|serie| serie.iter().copied().reduce(f64::max))
                })
                .map_or_else(|| "—".to_owned(), graus)
        }
    };
    let numeros = Row::new()
        .wrap()
        .gap(16)
        .child(numero(
            "desempenho-sensores-agora",
            t!("sensores-agora"),
            do_principal(|s| Some(s.mili)),
        ))
        .child(numero(
            "desempenho-sensores-maior",
            t!("sensores-maior"),
            maior,
        ))
        .child(numero(
            "desempenho-sensores-alto",
            t!("sensores-alto"),
            do_principal(|s| s.alto),
        ))
        .child(numero(
            "desempenho-sensores-critico",
            t!("sensores-critico"),
            do_principal(|s| s.critico),
        ));
    paineis(
        "desempenho-sensores",
        curvas,
        p.grafico_dos_sensores,
        altura,
        numeros,
    )
}

/// A aba Sensores.
pub(in crate::tela) fn aba_sensores(tela: &Rc<Tela>, p: Processos) -> impl View + use<> {
    let cpu = move || {
        p.quadro.with(|quadro| {
            quadro
                .as_ref()
                .and_then(|l| l.sensores.cpu.clone())
                .unwrap_or_default()
        })
    };
    let tem = move || !grupos_de_agora(p).is_empty();
    let vazio = move || {
        if p.quadro.with(Option::is_none) {
            t!("processos-lendo")
        } else {
            t!("sensores-nenhum")
        }
    };
    // Os painéis nascem de novo só quando os grupos mudam (outra máquina).
    let graficos = For::each(
        move || {
            let grupos = grupos_de_agora(p);
            if grupos.is_empty() {
                Vec::new()
            } else {
                vec![grupos]
            }
        },
        |grupos: &Vec<Grupo>| {
            grupos
                .iter()
                .map(|g| g.chave())
                .collect::<Vec<_>>()
                .join(",")
        },
        move |grupos: Vec<Grupo>| painel(p, grupos),
    );
    let lista = DataGrid::bind(p.linhas_dos_sensores, p.escolhidos_dos_sensores)
        .id("desempenho-sensores-lista")
        .grow(1)
        .columns([
            DataGridColumn::new("nome", t!("sensores-sensor")).width(ColumnWidth::Star(1.2)),
            DataGridColumn::new("valor", t!("sensores-valor"))
                .width(ColumnWidth::Pixels(110.0))
                .align_end(),
            DataGridColumn::new("alto", t!("sensores-alto"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("critico", t!("sensores-critico"))
                .width(ColumnWidth::Pixels(90.0))
                .align_end(),
            DataGridColumn::new("chip", t!("sensores-chip")).width(ColumnWidth::Pixels(130.0)),
        ])
        .sort(p.ordem_dos_sensores);
    Column::new()
        .grow(1)
        .gap(8)
        .padding(8)
        .child(barra(tela, p, SENSORES, p.escolhidos_dos_sensores))
        .child(
            Text::new(cpu)
                .id("desempenho-sensores-processador")
                .subtitle()
                .visible(move || !cpu().is_empty()),
        )
        .child(avisos(p, SENSORES))
        .child(
            Text::new(vazio)
                .id("desempenho-sensores-vazio")
                .visible(move || !tem()),
        )
        .child(
            Column::new()
                .grow(1)
                .gap(8)
                .visible(tem)
                .child(graficos)
                .child(lista),
        )
}
