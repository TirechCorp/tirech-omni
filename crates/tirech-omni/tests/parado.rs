//! A tela parada não pede quadro: sem prazo (animação, dica) e sem nada a repintar. É o que
//! deixa a janela em 0% de CPU quando ninguém mexe; só o pisca do cursor, a cada 530 ms, muda a
//! tela sozinho, e ele fica na plataforma.

mod comum;

use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, escolher_propriedade, exemplo, montar, montar_pura};

/// Roda alguns frames sem evento e confere que nenhum pede redesenho nem agenda prazo.
fn conferir_parado(app: &mut TestApp, onde: &str) {
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        app.ui().deadline(),
        None,
        "{onde}: a tela parada agenda um prazo ({:?})",
        app.ui().deadline_reason()
    );
    for i in 0..5 {
        assert!(
            !app.ui().frame(),
            "{onde}: o frame {i} sem evento pediu redesenho"
        );
    }
}

#[test]
fn a_tela_parada_nao_pede_quadro() {
    let pasta = Pasta::nova("parado-inicio");
    pasta.semear(&exemplo());
    pasta.pagina_inicial_padrao("inicio");
    let mut m = montar_pura(&pasta, Theme::Dark);
    conferir_parado(&mut m.app, "Início");

    let pasta = Pasta::nova("parado-terminais");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Dark);
    conferir_parado(&mut m.app, "Terminais sem sessão");

    m.app.expand("arvore", "1").click("arvore-2");
    conferir_parado(&mut m.app, "conexão escolhida");

    escolher_propriedade(&mut m.app, "Chave privada");
    conferir_parado(&mut m.app, "linha da grade escolhida");

    // Enter abre o editor: o foco fica no campo, com o cursor (que pisca na plataforma).
    m.app.key(Key::Enter);
    assert!(
        m.app.ui().text_input().is_some(),
        "o editor da Chave privada recebe o foco"
    );
    conferir_parado(&mut m.app, "editando a Chave privada");

    // A janela sem o foco do sistema também fica parada.
    m.app.ui().set_window_active(false);
    m.app.ui().set_caret_visible(false);
    m.app.frame();
    conferir_parado(&mut m.app, "janela sem foco");
}
