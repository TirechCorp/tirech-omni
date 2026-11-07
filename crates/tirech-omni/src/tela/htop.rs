//! O `htop-omni`: as abas Processos, Disco, Rede e Sensores da página Desempenho numa janela
//! própria, sem o resto do Omni. Só lê este computador (sem sessões SSH, não há máquina para
//! escolher) e não grava nada: o idioma e o tema vêm da configuração do Omni, lida só de
//! leitura pelo `main` do `htop-omni`. A aba "Este app" fica de fora: ela mede o próprio Omni.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use tomni_ui::prelude::*;

use super::processos::{self, DISCO, Fonte, FonteDaSaude, PROCESSOS, REDE, SENSORES};
use crate::processos::{FonteDeProcessos, Maquina};
use crate::saude_discos::Diagnostico;
use crate::sensores::Sensores;
use crate::{icone, t};

/// O nome do programa: o do executável, o da barra de título e o `app_id` da janela.
pub const NOME: &str = "htop-omni";

/// A janela com as leituras de verdade deste computador.
///
/// `diagnostico` é a opção "Diagnóstico dos discos" das Configurações do Omni: desligada, a aba
/// Disco não lê a saúde dos discos. `sudo_provado` diz que o `sudo` já funcionou neste computador
/// (é o `saude_discos.json` do Omni): sem isso, só o botão "Verificar de novo" o chama.
pub fn tela(diagnostico: bool, sudo_provado: bool) -> impl View {
    tela_com(
        Rc::new(crate::processos::fonte_do_sistema),
        crate::processos::RITMO,
        Arc::new(crate::sensores::deste_computador),
        Arc::new(crate::saude_discos::deste_computador),
        diagnostico,
        sudo_provado,
    )
}

/// A janela com a fonte dos processos, o ritmo do leitor, os sensores e o diagnóstico dos discos
/// dados (os testes passam a [`FonteDeTeste`](crate::processos::FonteDeTeste)).
pub fn tela_com(
    processos: Rc<dyn Fn(Maquina) -> Box<dyn FonteDeProcessos>>,
    ritmo: Duration,
    sensores: Arc<dyn Fn() -> Sensores + Send + Sync>,
    saude_local: Arc<dyn Fn(bool) -> Diagnostico + Send + Sync>,
    diagnostico: bool,
    sudo_provado: bool,
) -> impl View {
    let saude = FonteDaSaude::so_deste_computador(saude_local, diagnostico, sudo_provado);
    let fonte = Fonte::deste_computador(processos, ritmo, sensores, saude);
    // A janela aberta é a página: o leitor corre o tempo todo.
    let p = processos::ligar(&fonte, || true);
    p.aba.set(PROCESSOS.to_owned());
    Form::new(NOME)
        .density(Density::Compact)
        .title_bar(TitleBar::new(NOME).icon_image(icone::PEQUENO))
        .child(
            TabView::bind(p.aba)
                .id("desempenho-secoes")
                .tab_width(TabWidth::SizeToContent)
                .selected_indicator(true)
                .grow(1)
                .tab(
                    PROCESSOS,
                    t!("processos-aba"),
                    processos::aba_processos(&fonte, p),
                )
                .tab(
                    DISCO,
                    t!("processos-aba-disco"),
                    processos::aba_disco(&fonte, p),
                )
                .tab(
                    REDE,
                    t!("processos-aba-rede"),
                    processos::aba_rede(&fonte, p),
                )
                .tab(
                    SENSORES,
                    t!("processos-aba-sensores"),
                    processos::aba_sensores(&fonte, p),
                )
                // Sem sensores aqui, a aba não tem o que mostrar.
                .tab_visible(move |chave| chave != SENSORES || p.sensores_aqui),
        )
}
