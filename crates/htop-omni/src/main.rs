// O release do Windows é do subsistema gráfico: sem janela de console ao lado, como o Omni.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! O `htop-omni`: as abas Processos, Disco, Rede e Sensores da página Desempenho do Tirech
//! Omni, num programa que roda sem ele. Só lê este computador. Da pasta de configuração do Omni
//! (`TIRECH_OMNI_CONFIG` ou `~/.config/tirech-omni`), lê o idioma e o tema, só de leitura: não
//! pega a trava da pasta e não grava nada, e pode ficar aberto junto com o Omni.

use tirech_omni::armazenamento::{self, Armazenamento};
use tirech_omni::configuracoes::{Configuracoes, Tema};
use tirech_omni::saude_discos;
use tirech_omni::tela::htop;
use tirech_omni::{i18n, icone};
use tomni_ui::WindowIcon;
use tomni_ui::theme::Theme;

fn main() -> Result<(), tomni_ui::platform::Error> {
    let pasta = armazenamento::pasta();
    armazenamento::somente_leitura_em(&pasta);
    let armazenamento = Armazenamento::new(pasta);
    let (configuradas, _) = Configuracoes::carregar(&armazenamento);
    let sudo_provado = saude_discos::sudo_provado_neste_computador(&armazenamento);
    i18n::iniciar(
        configuradas.geral.idioma,
        sys_locale::get_locale().as_deref(),
    );
    let diagnostico = configuradas.geral.diagnostico_discos;
    let app = tomni_ui::App::new(move || {
        i18n::entregar_a_lib();
        htop::tela(diagnostico, sudo_provado)
    })
    .size(1100, 720)
    .icon(WindowIcon::new(icone::GRANDE).with_small(icone::PEQUENO))
    .app_id(htop::NOME);
    let app = match configuradas.geral.tema {
        Tema::Sistema => app,
        Tema::Claro => app.theme(Theme::Light),
        Tema::Escuro => app.theme(Theme::Dark),
    };
    app.run()
}
