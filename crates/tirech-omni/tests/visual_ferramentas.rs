//! A aparência das ferramentas para vários servidores (M42) nos dois temas: snapshots e lint.

mod comum;

use std::time::{Duration, Instant};

use tirech_omni::abrir::FakeOpener;
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::ExecOutput;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar, montar_com};

/// Quanto o ponteiro espera sobre um item para o submenu abrir.
const ATRASO_DO_SUBMENU: Duration = Duration::from_millis(450);

fn nome_do_tema(tema: Theme) -> &'static str {
    match tema {
        Theme::Dark => "escuro",
        _ => "claro",
    }
}

/// A tela com a árvore de exemplo e o grupo "Produção" aberto (ids 2 e 3: `web-01` e `db-01`;
/// 4: o Telnet).
fn tela(nome: &str, tema: Theme) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, tema);
    m.app.expand("arvore", "1");
    (pasta, m)
}

/// Abre `web-01`, `db-01` e o Telnet, nesta ordem, no painel ativo.
fn abrir_tres(m: &mut Montagem) {
    m.app
        .double_click("arvore-2")
        .double_click("arvore-3")
        .double_click("arvore-4");
    m.app.frame();
}

fn encerrar(m: &Montagem) {
    for (sessao, _) in m.opener.sessoes.borrow().iter() {
        sessao.dispose();
    }
}

/// Confere o lint e o snapshot `{nome}-{tema}`.
fn conferir(app: &mut TestApp, nome: &str, tema: Theme) {
    // Deixa as animações (a dos diálogos, a das dicas) terminarem.
    app.wait(Duration::from_millis(400));
    app.assert_lint();
    app.assert_snapshot(&format!("{nome}-{}", nome_do_tema(tema)));
}

fn chaves(app: &TestApp, painel: usize) -> Vec<String> {
    app.panes("paineis").panes[painel]
        .tabs
        .iter()
        .map(|(chave, _)| chave.clone())
        .collect()
}

fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

fn ferramentas_menu(tema: Theme) {
    let pasta = Pasta::nova(&format!("visual-ferramentas-menu-{}", nome_do_tema(tema)));
    pasta.semear(&exemplo());
    let opener = FakeOpener::new();
    *opener.distros_wsl.lock().unwrap() = vec!["Ubuntu".into(), "Debian".into()];
    let (app, fila) = montar_com(&pasta, tema, opener.clone());
    let mut m = Montagem { app, opener, fila };
    m.app.frame();
    // A thread que lê a lista acorda a tela quando termina.
    m.app.wait(Duration::from_millis(300));
    m.app.click("menus-ferramentas");
    m.app.hover("wsl");
    m.app.wait(ATRASO_DO_SUBMENU);
    conferir(&mut m.app, "ferramentas-menu", tema);
}

fn abas_caidas(tema: Theme) {
    let (_pasta, mut m) = tela(&format!("visual-abas-caidas-{}", nome_do_tema(tema)), tema);
    abrir_tres(&mut m);
    m.opener.derrubar(2, "tempo esgotado");
    m.opener.derrubar(3, "conexão recusada");
    m.app.frame();
    conferir(&mut m.app, "abas-caidas", tema);
    encerrar(&m);
}

fn aba_dica(tema: Theme) {
    let (_pasta, mut m) = tela(&format!("visual-aba-dica-{}", nome_do_tema(tema)), tema);
    abrir_tres(&mut m);
    m.opener.derrubar(2, "tempo esgotado");
    m.app.frame();
    let chave = chaves(&m.app, 0)[0].clone();
    m.app.hover(&format!("paineis-painel-0-{chave}"));
    m.app.wait(Duration::from_millis(1200));
    conferir(&mut m.app, "aba-dica", tema);
    encerrar(&m);
}

fn painel_vazio(tema: Theme) {
    let (_pasta, mut m) = tela(&format!("visual-painel-vazio-{}", nome_do_tema(tema)), tema);
    m.app.click("menus-exibir").click("layout-2x2");
    m.app.double_click("arvore-2");
    m.app.frame();
    // Fecha painéis vazios até sobrarem dois: um com a aba e um vazio.
    while m.app.panes("paineis").panes.len() > 2 {
        let vazio = m
            .app
            .panes("paineis")
            .panes
            .iter()
            .position(|p| p.tabs.is_empty())
            .unwrap();
        m.app.click(&format!("paineis-painel-{vazio}-fechar"));
        m.app.frame();
    }
    conferir(&mut m.app, "painel-vazio", tema);
    encerrar(&m);
}

fn cursor_aba_caida(tema: Theme) {
    let (_pasta, mut m) = tela(
        &format!("visual-cursor-aba-caida-{}", nome_do_tema(tema)),
        tema,
    );
    m.app.double_click("arvore-2");
    m.app.frame();
    let chave = chaves(&m.app, 0)[0].clone();
    m.app.click(&format!("paineis-terminal-{chave}"));
    m.opener.derrubar(2, "tempo esgotado");
    m.app.frame();
    conferir(&mut m.app, "cursor-aba-caida", tema);
    encerrar(&m);
}

fn envio_barra(tema: Theme) {
    let (_pasta, mut m) = tela(&format!("visual-envio-barra-{}", nome_do_tema(tema)), tema);
    abrir_tres(&mut m);
    let chaves = chaves(&m.app, 0);
    m.app.shortcut(Shortcut::ctrl_shift('i'));
    m.app.frame();
    m.app.click("multi-abas");
    m.app
        .grid_select("multi-abas-grade", &[&chaves[0], &chaves[2]]);
    m.app.click("multi-abas-dialogo-close");
    m.app
        .type_into("multi-texto", "sudo systemctl restart nginx");
    conferir(&mut m.app, "envio-barra", tema);
    encerrar(&m);
}

fn envio_abas_dialogo(tema: Theme) {
    let (_pasta, mut m) = tela(
        &format!("visual-envio-abas-dialogo-{}", nome_do_tema(tema)),
        tema,
    );
    abrir_tres(&mut m);
    m.opener.derrubar(3, "conexão recusada");
    m.app.shortcut(Shortcut::ctrl_shift('i'));
    m.app.frame();
    m.app.click("multi-abas");
    conferir(&mut m.app, "envio-abas-dialogo", tema);
    encerrar(&m);
}

fn cluster_dialogo(tema: Theme) {
    let (_pasta, mut m) = tela(
        &format!("visual-cluster-dialogo-{}", nome_do_tema(tema)),
        tema,
    );
    m.app
        .click("menus-ferramentas")
        .click("cluster-modo")
        .click("cluster-menu");
    m.app.frame();
    m.app.grid_select("cluster-grade", &["2", "3", "4"]);
    conferir(&mut m.app, "cluster-dialogo", tema);
}

fn cluster_janela(tema: Theme) {
    let (_pasta, mut m) = tela(
        &format!("visual-cluster-janela-{}", nome_do_tema(tema)),
        tema,
    );
    m.app.context_menu("arvore-1");
    m.app.click("menu-1-cluster");
    m.app.frame();
    m.app.click("cluster-abrir");
    m.app.frame();
    let [_, nova] = m.app.windows()[..] else {
        panic!("esperava duas janelas");
    };
    m.app.switch_to(nova);
    conferir(&mut m.app, "cluster-janela", tema);
    encerrar(&m);
}

fn scripts_pagina(tema: Theme) {
    let (_pasta, mut m) = tela(&format!("visual-scripts-{}", nome_do_tema(tema)), tema);
    m.opener.respostas_exec.lock().unwrap().insert(
        3,
        Ok(ExecOutput {
            status: Some(2),
            stdout: b"\x1b[32mverificando\x1b[0m\n".to_vec(),
            stderr: b"\x1b[31mdisco cheio em /var\x1b[0m\n".to_vec(),
        }),
    );
    m.app.click("menus-ferramentas").click("scripts-menu");
    m.app.frame();
    m.app.grid_select("scripts-servidores", &["2", "3"]);
    m.app.type_into("scripts-comando", "df -h /var");
    m.app.click("scripts-rodar");
    esperar(&mut m.app, "o fim da rodada", |a| {
        a.enabled("scripts-rodar")
    });
    m.app.grid_select("scripts-resultados", &["3"]);
    conferir(&mut m.app, "scripts-pagina", tema);
}

fn fechar_todas_dialogo(tema: Theme) {
    let (_pasta, mut m) = tela(&format!("visual-fechar-todas-{}", nome_do_tema(tema)), tema);
    abrir_tres(&mut m);
    m.app.click("menus-ferramentas").click("fechar-todas");
    conferir(&mut m.app, "fechar-todas-dialogo", tema);
    encerrar(&m);
}

/// Um teste por cena e por tema: assim cada snapshot sem referência (ou que mudou) deixa o seu
/// `.snap.new.png` para ser aberto antes de aprovar.
macro_rules! cenas {
    ($($cena:ident: $claro:ident, $escuro:ident;)*) => {
        $(
            #[test]
            fn $claro() {
                $cena(Theme::Light);
            }

            #[test]
            fn $escuro() {
                $cena(Theme::Dark);
            }
        )*
    };
}

cenas! {
    ferramentas_menu: ferramentas_menu_claro, ferramentas_menu_escuro;
    abas_caidas: abas_caidas_claro, abas_caidas_escuro;
    aba_dica: aba_dica_claro, aba_dica_escuro;
    painel_vazio: painel_vazio_claro, painel_vazio_escuro;
    cursor_aba_caida: cursor_aba_caida_claro, cursor_aba_caida_escuro;
    envio_barra: envio_barra_claro, envio_barra_escuro;
    envio_abas_dialogo: envio_abas_dialogo_claro, envio_abas_dialogo_escuro;
    cluster_dialogo: cluster_dialogo_claro, cluster_dialogo_escuro;
    cluster_janela: cluster_janela_claro, cluster_janela_escuro;
    scripts_pagina: scripts_pagina_claro, scripts_pagina_escuro;
    fechar_todas_dialogo: fechar_todas_dialogo_claro, fechar_todas_dialogo_escuro;
}
