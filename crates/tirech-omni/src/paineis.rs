//! Os painéis encaixáveis de cada página (M54): o que o `estado.json` guarda do `DockLayout` e as
//! conversões de ida e volta. Os tamanhos vão em pixels inteiros e os pesos em milésimos, para o
//! arquivo ser estável e para os tipos serem comparáveis.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tomni_ui::prelude::*;
use tomni_ui::{DockArea, DockPaneState, WindowGeometry};

use crate::espacos::Espaco;

/// A página Terminais no `Espaco.paineis`.
pub const TERMINAIS: &str = "terminais";
/// A página Notas.
pub const NOTAS: &str = "notas";
/// A página Arquivos.
pub const ARQUIVOS: &str = "arquivos";

/// O que uma página com painéis guarda: o layout (o app grava e devolve) e o controlador que os
/// menus e os atalhos usam.
#[derive(Clone)]
pub struct PaineisDaPagina {
    /// O estado dos painéis, que o `DockHost` mantém em dia.
    pub layout: Signal<DockLayout>,
    /// Mostra, esconde e move os painéis.
    pub controle: DockController,
}

impl PaineisDaPagina {
    /// Os painéis de uma página que começam em `layout` (vazio: o padrão dos painéis).
    pub fn novo(layout: DockLayout) -> Self {
        Self {
            layout: signal(layout),
            controle: DockController::new(),
        }
    }

    /// O estado de agora no formato do arquivo, ou `None` enquanto o layout é o vazio (a página
    /// nunca foi montada e ninguém mexeu).
    pub fn salvos(&self) -> Option<PaineisSalvos> {
        self.layout
            .with_untracked(|layout| (*layout != DockLayout::default()).then(|| de_layout(layout)))
    }
}

/// O maior tamanho de área ou de painel que o arquivo aceita; acima disso vale o padrão.
const TAMANHO_MAXIMO: u32 = 10_000;
/// O maior peso que o arquivo aceita, em milésimos (1000 = 1,0).
const PESO_MAXIMO: u32 = 1_000_000;

/// Como o painel aparece, no arquivo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModoSalvo {
    /// Ocupa espaço ao lado do conteúdo.
    #[default]
    Fixado,
    /// Um botão na faixa da borda.
    Auto,
    /// Numa janela própria.
    Flutuando,
    /// Escondido.
    Fechado,
}

/// O lado do painel, no arquivo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LadoSalvo {
    /// À esquerda do conteúdo.
    #[default]
    Esquerda,
    /// À direita do conteúdo.
    Direita,
    /// Embaixo do conteúdo.
    Embaixo,
}

/// A posição e o tamanho da janela de um painel flutuante.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeometriaSalva {
    /// A posição na tela (o Wayland não informa).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posicao: Option<(i32, i32)>,
    /// A largura e a altura do conteúdo.
    #[serde(default)]
    pub tamanho: (u32, u32),
}

/// Uma área fixada: o tamanho e a ordem dos painéis do lado.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaSalva {
    /// A largura (ou a altura, embaixo), em pixels. Zero: o padrão do primeiro painel.
    #[serde(default)]
    pub tamanho: u32,
    /// As chaves dos painéis do lado, na ordem.
    #[serde(default)]
    pub ordem: Vec<String>,
}

/// O estado de um painel, no arquivo.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PainelSalvo {
    /// Como o painel aparece agora.
    #[serde(default)]
    pub modo: ModoSalvo,
    /// Em que lado ele está.
    #[serde(default)]
    pub lado: LadoSalvo,
    /// A parte da área fixada que o painel ocupa, em milésimos.
    #[serde(default)]
    pub peso: u32,
    /// A largura (ou a altura) do painel aberto por cima, em pixels.
    #[serde(default)]
    pub por_cima: u32,
    /// Para onde o painel volta ao reabrir ou ao sair da janela flutuante.
    #[serde(default)]
    pub encaixado: ModoSalvo,
    /// A janela flutuante, quando já flutuou alguma vez.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flutuante: Option<GeometriaSalva>,
}

/// Os painéis de uma página.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaineisSalvos {
    /// A área fixada à esquerda.
    #[serde(default)]
    pub esquerda: AreaSalva,
    /// A área fixada à direita.
    #[serde(default)]
    pub direita: AreaSalva,
    /// A área fixada embaixo.
    #[serde(default)]
    pub embaixo: AreaSalva,
    /// O estado de cada painel, pela chave.
    #[serde(default)]
    pub paineis: BTreeMap<String, PainelSalvo>,
    /// O painel maximizado.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximizado: Option<String>,
}

fn modo_do_dock(modo: DockMode) -> ModoSalvo {
    match modo {
        DockMode::Pinned => ModoSalvo::Fixado,
        DockMode::AutoHide => ModoSalvo::Auto,
        DockMode::Floating => ModoSalvo::Flutuando,
        DockMode::Closed => ModoSalvo::Fechado,
    }
}

fn modo_salvo(modo: ModoSalvo) -> DockMode {
    match modo {
        ModoSalvo::Fixado => DockMode::Pinned,
        ModoSalvo::Auto => DockMode::AutoHide,
        ModoSalvo::Flutuando => DockMode::Floating,
        ModoSalvo::Fechado => DockMode::Closed,
    }
}

/// O modo em que um painel volta a ficar: só o fixado e o auto-ocultar valem.
fn modo_encaixado(modo: ModoSalvo) -> DockMode {
    match modo_salvo(modo) {
        DockMode::AutoHide => DockMode::AutoHide,
        _ => DockMode::Pinned,
    }
}

fn lado_do_dock(lado: DockSide) -> LadoSalvo {
    match lado {
        DockSide::Left => LadoSalvo::Esquerda,
        DockSide::Right => LadoSalvo::Direita,
        DockSide::Bottom => LadoSalvo::Embaixo,
    }
}

fn lado_salvo(lado: LadoSalvo) -> DockSide {
    match lado {
        LadoSalvo::Esquerda => DockSide::Left,
        LadoSalvo::Direita => DockSide::Right,
        LadoSalvo::Embaixo => DockSide::Bottom,
    }
}

fn pixels(valor: f64) -> u32 {
    if valor.is_finite() {
        valor.round().clamp(0.0, f64::from(TAMANHO_MAXIMO)) as u32
    } else {
        0
    }
}

/// Um tamanho do arquivo em pixels, ou `0.0` (o padrão do painel) se for absurdo.
fn tamanho_de(valor: u32) -> f64 {
    if (1..=TAMANHO_MAXIMO).contains(&valor) {
        f64::from(valor)
    } else {
        0.0
    }
}

fn area_do_dock(area: &DockArea) -> AreaSalva {
    AreaSalva {
        tamanho: pixels(area.size),
        ordem: area.order.clone(),
    }
}

fn area_salva(area: &AreaSalva) -> DockArea {
    DockArea {
        size: tamanho_de(area.tamanho),
        order: area.ordem.clone(),
    }
}

/// A geometria da janela flutuante de volta ao dock, sem posições e tamanhos absurdos (um
/// monitor que saiu), pelo mesmo critério das janelas salvas.
fn geometria_salva(salva: GeometriaSalva) -> Option<WindowGeometry> {
    let posicao = salva
        .posicao
        .filter(|&(x, y)| x.abs() <= 20_000 && y.abs() <= 20_000);
    let (largura, altura) = salva.tamanho;
    if !(200..=10_000).contains(&largura) || !(150..=10_000).contains(&altura) {
        return None;
    }
    Some(WindowGeometry {
        position: posicao,
        size: salva.tamanho,
    })
}

/// O `DockLayout` no formato do arquivo.
pub fn de_layout(layout: &DockLayout) -> PaineisSalvos {
    PaineisSalvos {
        esquerda: area_do_dock(&layout.left),
        direita: area_do_dock(&layout.right),
        embaixo: area_do_dock(&layout.bottom),
        paineis: layout
            .panes
            .iter()
            .map(|(chave, estado)| {
                let salvo = PainelSalvo {
                    modo: modo_do_dock(estado.mode),
                    lado: lado_do_dock(estado.side),
                    peso: (estado.weight * 1000.0)
                        .round()
                        .clamp(0.0, f64::from(PESO_MAXIMO)) as u32,
                    por_cima: pixels(estado.overlay_size),
                    encaixado: modo_do_dock(estado.docked),
                    flutuante: estado.floating.map(|g| GeometriaSalva {
                        posicao: g.position,
                        tamanho: g.size,
                    }),
                };
                (chave.clone(), salvo)
            })
            .collect(),
        maximizado: layout.maximized.clone(),
    }
}

/// O formato do arquivo de volta ao `DockLayout`. Valores absurdos (tamanho zero ou enorme, peso
/// zero, geometria fora da faixa) viram o padrão: o `DockHost` preenche o que ficou em zero.
pub fn para_layout(salvos: &PaineisSalvos) -> DockLayout {
    let mut layout = DockLayout {
        left: area_salva(&salvos.esquerda),
        right: area_salva(&salvos.direita),
        bottom: area_salva(&salvos.embaixo),
        panes: BTreeMap::new(),
        maximized: salvos.maximizado.clone(),
    };
    for (chave, salvo) in &salvos.paineis {
        let peso = if (1..=PESO_MAXIMO).contains(&salvo.peso) {
            f64::from(salvo.peso) / 1000.0
        } else {
            1.0
        };
        layout.panes.insert(
            chave.clone(),
            DockPaneState {
                mode: modo_salvo(salvo.modo),
                side: lado_salvo(salvo.lado),
                weight: peso,
                overlay_size: tamanho_de(salvo.por_cima),
                floating: salvo.flutuante.and_then(geometria_salva),
                docked: modo_encaixado(salvo.encaixado),
            },
        );
    }
    layout
}

/// O layout inicial da página `pagina`: o que o `estado.json` guardou, ou o vazio (o `DockHost`
/// usa o padrão dos painéis). `lateral_antiga` é a largura que as Notas gravavam antes do M54:
/// sem painéis guardados, ela vira o tamanho da área esquerda.
pub fn layout_inicial(
    estado: Option<&Espaco>,
    pagina: &str,
    lateral_antiga: Option<u32>,
) -> DockLayout {
    if let Some(salvos) = estado.and_then(|e| e.paineis.get(pagina)) {
        return para_layout(salvos);
    }
    let mut layout = DockLayout::default();
    if let Some(largura) = lateral_antiga {
        layout.left.size = tamanho_de(largura);
    }
    layout
}

#[cfg(test)]
mod testes {
    use super::*;

    fn exemplo() -> DockLayout {
        let mut layout = DockLayout {
            left: DockArea {
                size: 340.0,
                order: vec!["conexoes".into(), "propriedades".into()],
            },
            bottom: DockArea {
                size: 220.0,
                order: vec!["fila".into()],
            },
            ..DockLayout::default()
        };
        layout.panes.insert(
            "conexoes".into(),
            DockPaneState {
                mode: DockMode::AutoHide,
                side: DockSide::Left,
                weight: 1.5,
                overlay_size: 300.0,
                floating: None,
                docked: DockMode::AutoHide,
            },
        );
        layout.panes.insert(
            "propriedades".into(),
            DockPaneState {
                mode: DockMode::Floating,
                side: DockSide::Right,
                weight: 1.0,
                overlay_size: 320.0,
                floating: Some(WindowGeometry {
                    position: Some((40, 60)),
                    size: (500, 400),
                }),
                docked: DockMode::Pinned,
            },
        );
        layout.maximized = Some("fila".into());
        layout
    }

    #[test]
    fn ida_e_volta_do_layout_mantem_tudo() {
        let layout = exemplo();
        assert_eq!(para_layout(&de_layout(&layout)), layout);
    }

    #[test]
    fn o_layout_vazio_vai_e_volta_vazio() {
        let vazio = DockLayout::default();
        assert_eq!(para_layout(&de_layout(&vazio)), vazio);
    }

    #[test]
    fn o_json_usa_nomes_em_portugues() {
        let json = serde_json::to_value(de_layout(&exemplo())).unwrap();
        assert_eq!(json["esquerda"]["tamanho"], 340);
        assert_eq!(json["paineis"]["conexoes"]["modo"], "auto");
        assert_eq!(json["paineis"]["propriedades"]["modo"], "flutuando");
        assert_eq!(json["paineis"]["propriedades"]["lado"], "direita");
        assert_eq!(json["paineis"]["conexoes"]["peso"], 1500);
        assert_eq!(json["maximizado"], "fila");
    }

    #[test]
    fn valores_absurdos_viram_o_padrao() {
        let salvos = PaineisSalvos {
            esquerda: AreaSalva {
                tamanho: 4_000_000_000,
                ordem: vec!["a".into()],
            },
            paineis: BTreeMap::from([(
                "a".to_owned(),
                PainelSalvo {
                    peso: 0,
                    por_cima: 99_999,
                    flutuante: Some(GeometriaSalva {
                        posicao: Some((900_000, 0)),
                        tamanho: (3, 4),
                    }),
                    ..PainelSalvo::default()
                },
            )]),
            ..PaineisSalvos::default()
        };
        let layout = para_layout(&salvos);
        assert_eq!(layout.left.size, 0.0);
        let a = layout.pane("a").unwrap();
        assert_eq!(a.weight, 1.0);
        assert_eq!(a.overlay_size, 0.0);
        assert_eq!(a.floating, None);
    }

    #[test]
    fn a_posicao_absurda_cai_mas_o_tamanho_bom_fica() {
        let salva = GeometriaSalva {
            posicao: Some((-900_000, 10)),
            tamanho: (600, 400),
        };
        assert_eq!(
            geometria_salva(salva),
            Some(WindowGeometry {
                position: None,
                size: (600, 400),
            })
        );
    }

    #[test]
    fn um_arquivo_sem_os_campos_novos_abre_com_o_padrao() {
        let salvos: PaineisSalvos = serde_json::from_str("{}").unwrap();
        assert_eq!(para_layout(&salvos), DockLayout::default());
        let painel: PainelSalvo = serde_json::from_str("{\"modo\":\"auto\"}").unwrap();
        assert_eq!(painel.modo, ModoSalvo::Auto);
        assert_eq!(painel.lado, LadoSalvo::Esquerda);
    }

    #[test]
    fn a_lateral_antiga_das_notas_vira_a_area_esquerda() {
        let layout = layout_inicial(None, NOTAS, Some(280));
        assert_eq!(layout.left.size, 280.0);
        let gravado = Espaco {
            paineis: BTreeMap::from([(NOTAS.to_owned(), de_layout(&exemplo()))]),
            ..Espaco::default()
        };
        assert_eq!(
            layout_inicial(Some(&gravado), NOTAS, Some(280)).left.size,
            340.0,
            "o que o M54 gravou vale mais que a lateral antiga"
        );
    }
}
