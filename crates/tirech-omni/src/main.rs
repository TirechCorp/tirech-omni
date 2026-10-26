// O release do Windows é do subsistema gráfico: sem janela de console ao lado do app (M65). O
// build de debug mantém o console, que é onde se leem os `eprintln!` no desenvolvimento. A ponte
// `tirech-omni mcp` segue funcionando: o Windows entrega os pipes do stdin e do stdout pelo
// `STARTUPINFO`, e o `std::io::stdin()` os lê normalmente.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! O `tirech-omni` de verdade: lê as conexões da pasta de configuração (`TIRECH_OMNI_CONFIG` ou
//! `~/.config/tirech-omni`) e abre as sessões pelo SSH do sistema de arquivos do usuário
//! (`~/.ssh/known_hosts`, agente e chaves).

use std::rc::Rc;

use tirech_omni::abrir::RealOpener;
use tirech_omni::armazenamento::{self, Armazenamento};
use tirech_omni::configuracoes::Configuracoes;
use tirech_omni::ferramentas::Ambiente;
use tirech_omni::marca::ID_DO_APP;
use tirech_omni::{i18n, icone, migrar_nome, reinicio, sistema, t, tela};
use tomni_ui::WindowIcon;
use tomni_ui::platform::CrashReport;

fn main() -> Result<(), tomni_ui::platform::Error> {
    // `tirech-omni mcp`: a ponte que o Claude Code e o Codex rodam. Não abre o app nem a janela.
    if std::env::args().nth(1).as_deref() == Some("mcp") {
        // A ponte só lê a configuração: quem grava é o app.
        let pasta = armazenamento::pasta();
        armazenamento::somente_leitura_em(&pasta);
        let (configuradas, _) = Configuracoes::carregar(&Armazenamento::new(pasta));
        i18n::iniciar(
            configuradas.geral.idioma,
            sys_locale::get_locale().as_deref(),
        );
        std::process::exit(tirech_omni::mcp::ponte::rodar(std::env::args().skip(2)));
    }
    let portatil = armazenamento::modo_portatil();
    // Quem já usava o app com o nome antigo (Tirech SSH) leva os dados e as entradas do sistema
    // para o nome novo. Com a pasta trocada por variável (testes, `lint`) ou no modo portable,
    // nada é tocado.
    if armazenamento::variavel_da_pasta().is_none() && !portatil {
        for erro in migrar_nome::migrar(&migrar_nome::Lugares::do_sistema()) {
            eprintln!("{erro}");
        }
    }
    let pasta = armazenamento::pasta();
    // A trava vem antes de ler a configuração, que pode gravar (uma migração, um arquivo ilegível
    // que vai para o `.bak`). A primeira cópia aberta fica com ela e é a dona da configuração; as
    // outras abrem só de leitura, como no WinSSHTerm: sem as abas da primeira (que já estão
    // abertas nela) e sem gravar arquivo nenhum da pasta de dados, do cofre e dos espaços. O que
    // se muda nelas fica na memória e se perde ao fechar. A trava solta quando o processo acaba,
    // mesmo se ele cair. Aberto pelo "Reiniciar agora", o app espera a trava do anterior soltar.
    let tentativas = if portatil || reinicio::reiniciado() {
        15
    } else {
        1
    };
    let (trava, dono_do_estado) = match travar_tentando(&pasta, tentativas) {
        Ok(trava) => (trava, true),
        Err(()) => {
            armazenamento::somente_leitura_em(&pasta);
            (None, false)
        }
    };
    // O idioma vale por processo: a troca em Configurações pede reiniciar.
    let (configuradas, _) = Configuracoes::carregar(&Armazenamento::new(pasta.clone()));
    i18n::iniciar(
        configuradas.geral.idioma,
        sys_locale::get_locale().as_deref(),
    );
    // No modo portable (num pendrive, por exemplo), uma cópia do app por pasta.
    if portatil && !dono_do_estado {
        tomni_ui::platform::alert(
            &t!("portatil-ja-aberto-titulo"),
            &t!("portatil-ja-aberto", pasta = pasta.display().to_string()),
        );
        return Ok(());
    }
    // No Windows o app não tem console: o pânico e o erro que fecham a janela vão para o
    // `quedas.log` e para um aviso, em vez de sumirem.
    let quedas = pasta.join("quedas.log");
    let arquivo = quedas.display().to_string();
    let relatorio = CrashReport {
        title: t!("queda-titulo"),
        message: t!("queda-mensagem", arquivo = arquivo.as_str()),
        log: quedas,
    };
    relatorio.clone().install();
    let guarda = pasta.join("desenho.txt");
    let resultado = tomni_ui::App::new(move || {
        i18n::entregar_a_lib();
        // No modo portable, o `known_hosts` padrão fica na pasta, e não no `~/.ssh`.
        let known_hosts = portatil.then(|| pasta.join("known_hosts"));
        let mut ambiente = Ambiente::do_sistema(&pasta);
        ambiente.dono_do_estado = dono_do_estado;
        tela::tela_com_ambiente(pasta.clone(), ambiente, move |fila| {
            Rc::new(RealOpener::new(fila, known_hosts))
        })
    })
    .view(tela::janela_terminais)
    .size(1280, 720)
    // O ícone da janela e o `app_id`, que o ambiente casa com o atalho do menu (`StartupWMClass`).
    .icon(WindowIcon::new(icone::GRANDE).with_small(icone::PEQUENO))
    .app_id(ID_DO_APP)
    // Aberto pelo início com o sistema (`--bandeja`) e com `geral.iniciar_na_bandeja`: a janela
    // nasce escondida. A tela a mostra de novo se, passada a espera, o sistema não tiver bandeja.
    .start_hidden(sistema::aberto_na_bandeja() && configuradas.geral.iniciar_na_bandeja)
    // Se a abertura anterior caiu antes do primeiro quadro (um driver de vídeo), esta desenha
    // de um jeito mais simples; o degrau que funcionou fica no `desenho.txt`.
    .render_guard(guarda)
    .run();
    if let Err(erro) = &resultado {
        relatorio.failure(erro);
    }
    // "Reiniciar agora" (idioma, pasta de dados): sobe outro app, que espera este acabar. A
    // trava sai antes, para o novo não a achar ocupada.
    drop(trava);
    if reinicio::pedido() {
        let _ = reinicio::subir_de_novo();
    }
    resultado
}

/// Trava a pasta de dados, com `tentativas` tentativas de 200 ms: no "Reiniciar agora" do
/// Windows, o app novo sobe antes de o antigo acabar. `Err` se continuou ocupada. Um erro de disco
/// (pasta só de leitura) não impede de abrir.
fn travar_tentando(pasta: &std::path::Path, tentativas: u32) -> Result<Option<std::fs::File>, ()> {
    for tentativa in 0..tentativas {
        match armazenamento::travar(pasta) {
            Ok(armazenamento::Trava::Travada(arquivo)) => return Ok(Some(arquivo)),
            Ok(armazenamento::Trava::Ocupada) => {
                if tentativa + 1 < tentativas {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            }
            Err(erro) => {
                eprintln!("{erro}");
                return Ok(None);
            }
        }
    }
    Err(())
}
