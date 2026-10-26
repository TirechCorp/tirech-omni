//! A página Logs (M53) nos dois temas: a lista com o leitor e a busca, o cifrado com o cofre
//! trancado, o log alterado com o texto que deu para ler e a lista vazia. Snapshots e lint.

mod comum;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use tirech_omni::log_cifrado::{EscritorCifrado, ler_publica};
use tirech_omni::logs::garantir_chave;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, exemplo, montar_pura};

const TEMAS: [(&str, Theme); 2] = [("claro", Theme::Light), ("escuro", Theme::Dark)];

/// Uma pasta fixa em `/tmp` (as datas dos logs são fixas e o caminho entra na chave da lista),
/// com a chave dos logs no cofre.
fn pasta_fixa(teste: &str) -> Pasta {
    tirech_omni::recentes::fixar_relogio(Some(1_760_000_000));
    let pasta = Pasta(PathBuf::from(format!(
        "/tmp/tirech-omni-visual-logs-{teste}"
    )));
    let _ = fs::remove_dir_all(&pasta.0);
    fs::create_dir_all(pasta.0.join("logs")).unwrap();
    pasta.semear(&exemplo());
    garantir_chave(&mut pasta.cofre()).expect("criar a chave dos logs");
    pasta
}

fn datar(caminho: &Path, segundos: u64) {
    fs::File::options()
        .write(true)
        .open(caminho)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_760_000_000 - segundos))
        .unwrap();
}

fn log_de_texto(pasta: &Pasta, nome: &str, texto: &str, ha: u64) -> PathBuf {
    let caminho = pasta.caminho().join("logs").join(nome);
    fs::write(&caminho, texto).unwrap();
    datar(&caminho, ha);
    caminho
}

fn log_cifrado(pasta: &Pasta, nome: &str, texto: &str, ha: u64) -> PathBuf {
    let caminho = pasta.caminho().join("logs").join(nome);
    let publica = ler_publica(pasta.caminho()).unwrap().publica;
    {
        let mut e = EscritorCifrado::abrir(&caminho, &publica).unwrap();
        e.write_all(texto.as_bytes()).unwrap();
    }
    datar(&caminho, ha);
    caminho
}

const SESSAO: &str = "Last login: Tue Oct 14 09:12:01 2025 from 10.0.0.7\n\
    ana@web-01:~$ systemctl status nginx\n\
    * nginx.service - A high performance web server\n\
    \x20    Active: active (running) since Tue 2025-10-14 09:12:40 UTC\n\
    ana@web-01:~$ tail -n 3 /var/log/nginx/error.log\n\
    2025/10/14 09:13:02 [error] 812#812: *41 connect() failed (111: Connection refused)\n\
    2025/10/14 09:13:05 [warn] 812#812: *42 upstream server temporarily disabled\n\
    2025/10/14 09:13:09 [error] 812#812: *43 connect() failed (111: Connection refused)\n\
    ana@web-01:~$ \n";

/// A tela na página Logs, com a lista pronta.
fn abrir(pasta: &Pasta, tema: Theme) -> TestApp {
    let mut m = montar_pura(pasta, tema);
    m.app.click("navegacao-logs");
    m.app.frame();
    m.app
}

fn escolher(app: &mut TestApp, caminho: &Path) {
    app.grid_select("logs-lista", &[&caminho.display().to_string()]);
    for _ in 0..8 {
        app.wait(Duration::from_millis(20));
    }
}

#[test]
fn a_lista_e_o_log_aberto_com_a_busca() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("lista-{nome}"));
        let aberto = log_de_texto(&pasta, "Web-20251014-091200-1.log", SESSAO, 600);
        log_cifrado(
            &pasta,
            "Banco-20251013-180000-1.log.cifrado",
            "ok\n",
            90_000,
        );
        log_de_texto(&pasta, "Web-20251010-080000-1.log", "ls\n", 400_000);
        let mut app = abrir(&pasta, tema);
        escolher(&mut app, &aberto);
        app.click("logs-buscar");
        app.type_into("logs-busca", "error");
        app.frame();
        app.assert_lint();
        app.assert_snapshot(&format!("logs-lista-{nome}"));
        let _ = fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn o_cifrado_com_o_cofre_trancado() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("trancado-{nome}"));
        let cifrado = log_cifrado(&pasta, "Web-20251014-091200-1.log.cifrado", SESSAO, 600);
        pasta.cofre_com_senha_mestra("abc");
        let mut m = montar_pura(&pasta, tema);
        m.app.click("cofre-desbloquear-close");
        m.app.click("navegacao-logs");
        m.app.frame();
        escolher(&mut m.app, &cifrado);
        m.app.assert_lint();
        m.app.assert_snapshot(&format!("logs-trancado-{nome}"));
        let _ = fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn o_log_alterado_mostra_o_texto_que_deu_para_ler() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("alterado-{nome}"));
        // Mais de 64 KiB: o primeiro registro fecha antes do fim; estraga-se o segundo.
        let mut texto = String::new();
        while texto.len() < 70_000 {
            texto.push_str(SESSAO);
        }
        let caminho = log_cifrado(&pasta, "Web-20251014-091200-1.log.cifrado", &texto, 600);
        let mut bytes = fs::read(&caminho).unwrap();
        let primeiro = 49 + 5;
        let tamanho = u32::from_be_bytes(bytes[49..53].try_into().unwrap()) as usize;
        let alvo = primeiro + tamanho + 5 + 3;
        bytes[alvo] ^= 0xff;
        fs::write(&caminho, bytes).unwrap();
        datar(&caminho, 600);
        let mut app = abrir(&pasta, tema);
        escolher(&mut app, &caminho);
        assert!(app.visible("logs-aviso-erro"));
        app.assert_lint();
        app.assert_snapshot(&format!("logs-alterado-{nome}"));
        let _ = fs::remove_dir_all(pasta.caminho());
    }
}

#[test]
fn a_lista_vazia() {
    for (nome, tema) in TEMAS {
        let pasta = pasta_fixa(&format!("vazia-{nome}"));
        let mut app = abrir(&pasta, tema);
        app.assert_lint();
        app.assert_snapshot(&format!("logs-vazia-{nome}"));
        let _ = fs::remove_dir_all(pasta.caminho());
    }
}
