//! As abas Processos e Disco da página Desempenho. Com a fonte de teste (três processos que
//! gastam sempre o mesmo: `init`, o `sshd` e o `rsync` que ele roda), a lista, o resumo, o
//! filtro, a árvore, a ordem e os pedidos de encerrar; no Linux, também contra o servidor SSH de
//! teste, que roda o script de leitura no `sh` deste computador e encerra um processo de verdade.

mod comum;
#[cfg(target_os = "linux")]
#[path = "comum/servidor.rs"]
mod servidor;

use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tirech_omni::modelo::{Connection, Kind, Tree};
use tirech_omni::processos::{FonteDeTeste, Maquina, PONTOS, foto_de_exemplo};
use tomni_ui::kurbo::Point;
use tomni_ui::theme::Theme;
use tomni_ui::{Key, Modifiers};
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, montar_com_ambiente};

/// Espera de verdade (o leitor anda numa thread, no relógio do computador): dorme um pouco e
/// desenha um quadro, até `pronto` ou 10 s.
fn esperar(app: &mut TestApp, o_que: &str, pronto: impl Fn(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    while !pronto(app) {
        assert!(Instant::now() < limite, "esperando {o_que}");
        thread::sleep(Duration::from_millis(20));
        app.frame();
    }
}

/// As chaves das linhas da tabela `id`.
fn chaves(app: &TestApp, id: &str) -> Vec<String> {
    app.grid(id).rows.iter().map(|l| l.key.clone()).collect()
}

/// As células da linha `chave` da tabela `id`.
fn celulas(app: &TestApp, id: &str, chave: &str) -> Vec<String> {
    app.grid(id)
        .rows
        .into_iter()
        .find(|l| l.key == chave)
        .unwrap_or_else(|| panic!("a linha {chave}"))
        .cells
}

/// Troca o texto do filtro da aba Processos por `texto`.
fn filtrar(app: &mut TestApp, texto: &str) {
    app.click("desempenho-processos-filtro")
        .key_with(Key::Char('a'), Modifiers::COMMAND)
        .type_text(texto);
}

/// Duas conexões SSH na raiz: 1 `web` e 2 `db`.
fn arvore() -> Tree {
    let mut tree = Tree::default();
    for (nome, host) in [("web", "web.exemplo"), ("db", "db.exemplo")] {
        let mut c = Connection::new(nome, Kind::Ssh);
        c.host = host.into();
        c.user = "ana".into();
        c.key_file = Some("/chaves/ana".into());
        tree.add_connection(None, c);
    }
    tree
}

const TABELA: &str = "desempenho-processos-tabela";
const DISCO: &str = "desempenho-disco-tabela";

/// A tela com a fonte de teste, a página Desempenho aberta na aba Processos e a primeira leitura
/// já na tabela. Devolve também as máquinas que a fábrica recebeu.
struct Cena {
    _pasta: Pasta,
    m: Montagem,
    fonte: FonteDeTeste,
    maquinas: Arc<Mutex<Vec<bool>>>,
}

impl Cena {
    fn nova(nome: &str, tema: Theme) -> Self {
        Self::com_fonte(nome, tema, FonteDeTeste::exemplo())
    }

    /// Para as snapshots: a fonte entrega as fotos que enchem os gráficos (a primeira não tem
    /// taxas) e depois para, e a cena espera a última chegar. Assim os totais do disco e as
    /// curvas saem sempre iguais.
    fn para_snapshot(nome: &str, tema: Theme) -> Self {
        let ultima = PONTOS as u64;
        let parou = Arc::new(AtomicBool::new(false));
        let p = Arc::clone(&parou);
        let fonte = FonteDeTeste::nova(move |n| {
            if n > ultima {
                // O leitor não é esperado ao ser descartado: esta thread só dorme até o fim.
                p.store(true, Ordering::SeqCst);
                thread::sleep(Duration::from_secs(60));
            }
            Ok(foto_de_exemplo(n.min(ultima)))
        });
        let mut c = Self::com_fonte(nome, tema, fonte);
        esperar(&mut c.m.app, "a última foto", |_| {
            parou.load(Ordering::SeqCst)
        });
        c.m.app.frame();
        c
    }

    fn com_fonte(nome: &str, tema: Theme, fonte: FonteDeTeste) -> Self {
        let pasta = Pasta::nova(nome);
        pasta.semear(&arvore());
        let maquinas: Arc<Mutex<Vec<bool>>> = Arc::default();
        let (f, q) = (fonte.clone(), Arc::clone(&maquinas));
        let (mut m, _) = montar_com_ambiente(&pasta, tema, move |a| {
            a.processos = Rc::new(move |maquina| {
                q.lock()
                    .unwrap()
                    .push(matches!(maquina, Maquina::Remota(_)));
                Box::new(f.clone())
            });
        });
        m.app.click("barra-de-titulo-cpu");
        m.app.click("desempenho-secoes-processos");
        esperar(&mut m.app, "a primeira leitura", |a| {
            a.grid(TABELA).rows.len() == 3
        });
        Self {
            _pasta: pasta,
            m,
            fonte,
            maquinas,
        }
    }

    /// Clica no cabeçalho da coluna `coluna` da tabela `id`.
    fn ordenar_por(&mut self, id: &str, coluna: &str) {
        let grade = self.m.app.grid(id);
        let cabecalho = self.m.app.rect(&format!("{id}-cabecalho"));
        let mut x = cabecalho.x0;
        for c in &grade.columns {
            if c.key == coluna {
                x += c.width / 2.0;
                break;
            }
            x += c.width;
        }
        self.m
            .app
            .click_at(Point::new(x, (cabecalho.y0 + cabecalho.y1) / 2.0), 1);
    }
}

#[test]
fn a_aba_processos_lista_com_a_cpu_da_maquina_e_o_resumo() {
    let mut c = Cena::nova("processos-lista", Theme::Light);
    // Pela CPU, da maior para a menor; a CPU é da máquina inteira (4 núcleos).
    assert_eq!(chaves(&c.m.app, TABELA), ["900", "400", "1"]);
    assert_eq!(
        celulas(&c.m.app, TABELA, "900"),
        [
            "rsync",
            "900",
            "ana",
            "Executando",
            "12%",
            "120 MB",
            "3",
            "rsync -a /dados /copia"
        ]
    );
    assert_eq!(
        celulas(&c.m.app, TABELA, "400")[2..6],
        ["root", "Dormindo", "1,2%", "24 MB"]
    );
    assert_eq!(
        c.m.app.text("desempenho-processos-resumo"),
        "3 processos · CPU 14% · Memória 2,0 GB de 8,0 GB"
    );
    // Este computador, sem nenhuma conexão aberta.
    assert_eq!(*c.maquinas.lock().unwrap(), [false]);
    c.m.app.assert_lint();
}

#[test]
fn a_aba_disco_mostra_as_taxas_e_so_os_que_usam() {
    let mut c = Cena::nova("processos-disco", Theme::Light);
    c.m.app.click("desempenho-secoes-disco");
    esperar(&mut c.m.app, "a tabela do disco", |a| {
        a.grid(DISCO).rows.len() == 3
    });
    assert_eq!(chaves(&c.m.app, DISCO)[0], "900");
    assert_eq!(
        celulas(&c.m.app, DISCO, "900")[3..5],
        ["512,0 KB/s", "2,0 MB/s"]
    );
    assert_eq!(
        c.m.app.text("desempenho-disco-resumo"),
        "Leitura 512,0 KB/s · Escrita 2,0 MB/s"
    );
    c.m.app.click("desempenho-disco-so-ativos");
    assert_eq!(chaves(&c.m.app, DISCO), ["900"]);
}

#[test]
fn o_filtro_procura_no_nome_no_usuario_no_comando_e_no_pid() {
    let mut c = Cena::nova("processos-filtro", Theme::Light);
    filtrar(&mut c.m.app, "ANA");
    assert_eq!(chaves(&c.m.app, TABELA), ["900"]);
    filtrar(&mut c.m.app, "/usr/sbin");
    assert_eq!(chaves(&c.m.app, TABELA), ["400"]);
    filtrar(&mut c.m.app, "1");
    assert_eq!(chaves(&c.m.app, TABELA), ["1"]);
}

#[test]
fn a_arvore_mostra_os_pais_e_a_ordem_por_coluna_a_desliga() {
    let mut c = Cena::nova("processos-arvore", Theme::Light);
    c.m.app.click("desempenho-processos-arvore");
    assert_eq!(chaves(&c.m.app, TABELA), ["1", "400", "900"]);
    let nomes: Vec<String> =
        c.m.app
            .grid(TABELA)
            .rows
            .iter()
            .map(|l| l.cells[0].clone())
            .collect();
    assert_eq!(nomes, ["init", "└─ sshd", "   └─ rsync"]);
    assert_eq!(c.m.app.grid(TABELA).sort, None);
    // Com filtro, os pais de quem passa ficam.
    filtrar(&mut c.m.app, "rsync");
    assert_eq!(chaves(&c.m.app, TABELA), ["1", "400", "900"]);
    // Ordenar por uma coluna sai da árvore.
    c.ordenar_por(TABELA, "memoria");
    assert!(!c.m.app.checked("desempenho-processos-arvore"));
    assert_eq!(chaves(&c.m.app, TABELA), ["900"]);
    assert!(c.m.app.grid(TABELA).sort.is_some());
    // E desligar a árvore à mão volta para a ordem da CPU.
    c.m.app.click("desempenho-processos-arvore");
    c.m.app.click("desempenho-processos-arvore");
    assert_eq!(
        c.m.app
            .grid(TABELA)
            .sort
            .map(|(coluna, _)| coluna)
            .as_deref(),
        Some("cpu")
    );
}

#[test]
fn encerrar_e_forcar_pedem_pela_selecao_e_pelo_menu() {
    let mut c = Cena::nova("processos-encerrar", Theme::Light);
    assert!(!c.m.app.enabled("desempenho-processos-encerrar"));
    c.m.app.click(&format!("{TABELA}-900"));
    c.m.app
        .click_with(&format!("{TABELA}-400"), Modifiers::COMMAND);
    assert!(c.m.app.enabled("desempenho-processos-encerrar"));
    c.m.app.click("desempenho-processos-encerrar");
    esperar(&mut c.m.app, "os dois pedidos", |_| {
        c.fonte.pedidos().len() == 2
    });
    let mut pedidos = c.fonte.pedidos();
    pedidos.sort_unstable();
    assert_eq!(pedidos, [(400, false), (900, false)]);
    c.m.app.click("desempenho-processos-forcar");
    esperar(&mut c.m.app, "os forçados", |_| {
        c.fonte.pedidos().len() == 4
    });
    assert!(c.fonte.pedidos()[2..].iter().all(|(_, forcar)| *forcar));
    // O menu de uma linha fora da seleção age só nela.
    c.m.app.context_menu(&format!("{TABELA}-1"));
    c.m.app.click("desempenho-processos-menu-1-forcar");
    esperar(&mut c.m.app, "o do menu", |_| c.fonte.pedidos().len() == 5);
    assert_eq!(c.fonte.pedidos()[4], (1, true));
    // O Delete encerra a seleção, sem forçar.
    c.m.app.click(&format!("{TABELA}-900"));
    c.m.app.key(Key::Delete);
    esperar(&mut c.m.app, "o do Delete", |_| {
        c.fonte.pedidos().len() == 6
    });
    assert_eq!(c.fonte.pedidos()[5], (900, false));
}

#[test]
fn o_erro_da_leitura_aparece_na_aba() {
    let pasta = Pasta::nova("processos-erro");
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| {
        a.processos = Rc::new(|_| {
            Box::new(FonteDeTeste::nova(|n| {
                if n < 2 {
                    Ok(foto_de_exemplo(n))
                } else {
                    Err("o servidor não respondeu".to_owned())
                }
            }))
        });
    });
    m.app.click("barra-de-titulo-cpu");
    m.app.click("desempenho-secoes-processos");
    esperar(&mut m.app, "o erro", |a| {
        a.visible("desempenho-processos-erro")
    });
    assert!(
        m.app
            .text("desempenho-processos-erro")
            .contains("o servidor não respondeu")
    );
    // A leitura que já chegou continua na tabela.
    assert_eq!(m.app.grid(TABELA).rows.len(), 3);
}

#[test]
fn o_leitor_so_corre_com_uma_das_abas_novas_a_mostra() {
    let mut c = Cena::nova("processos-parado", Theme::Light);
    c.m.app.click("desempenho-secoes-app");
    assert!(!c.m.app.visible(TABELA));
    c.m.app.click("desempenho-secoes-disco");
    esperar(&mut c.m.app, "a tabela do disco", |a| {
        a.grid(DISCO).rows.len() == 3
    });
    // Saiu da aba app e voltou: um leitor novo, ainda neste computador.
    assert_eq!(*c.maquinas.lock().unwrap(), [false, false]);
    // Ir de Disco para Processos não troca o leitor.
    c.m.app.click("desempenho-secoes-processos");
    assert_eq!(c.maquinas.lock().unwrap().len(), 2);
}

#[test]
fn o_seletor_lista_as_conexoes_ssh_abertas() {
    let mut c = Cena::nova("processos-maquinas", Theme::Light);
    // A conexão falsa não tem os canais do SSH: o seletor só tem este computador.
    c.m.app.click("navegacao-terminais");
    c.m.app.double_click("arvore-1");
    c.m.app.click("menus-exibir").click("desempenho");
    esperar(&mut c.m.app, "a tabela", |a| a.grid(TABELA).rows.len() == 3);
    c.m.app.click("desempenho-processos-maquina");
    assert!(
        c.m.app
            .visible("desempenho-processos-maquina-este-computador")
    );
    assert!(
        c.m.app
            .tree()
            .find("desempenho-processos-maquina-web")
            .is_none()
    );
}

#[test]
fn os_graficos_mostram_a_maquina_e_o_disco() {
    let mut c = Cena::nova("processos-graficos", Theme::Light);
    esperar(&mut c.m.app, "a segunda leitura com taxas", |a| {
        a.text("desempenho-processos-utilizacao") != "—"
    });
    assert_eq!(c.m.app.text("desempenho-processos-quantos"), "3");
    assert_eq!(c.m.app.text("desempenho-processos-threads"), "5");
    assert!(c.m.app.visible("desempenho-processos-grafico-cpu"));
    assert!(!c.m.app.visible("desempenho-processos-grafico-memoria"));
    c.m.app.click("desempenho-processos-memoria");
    assert!(c.m.app.visible("desempenho-processos-grafico-memoria"));
    assert!(!c.m.app.visible("desempenho-processos-grafico-cpu"));

    c.m.app.click("desempenho-secoes-disco");
    esperar(&mut c.m.app, "o disco", |a| {
        a.text("desempenho-disco-escrita-numero") == "2,0 MB/s"
    });
    assert_eq!(
        c.m.app.text("desempenho-disco-leitura-numero"),
        "512,0 KB/s"
    );
    assert_eq!(c.m.app.text("desempenho-disco-usando"), "1");
    // O ponto mais novo da série da escrita, em bytes por segundo.
    assert!(
        (c.m.app.value("desempenho-disco-grafico-escrita") - 2.0 * 1024.0 * 1024.0).abs() < 1.0
    );
}

#[test]
fn snapshot_da_aba_processos_claro() {
    let mut c = Cena::para_snapshot("processos-snapshot-claro", Theme::Light);
    c.m.app.click(&format!("{TABELA}-900"));
    c.m.app.assert_snapshot("desempenho-processos-claro");
}

#[test]
fn snapshot_da_aba_processos_escuro() {
    let mut c = Cena::para_snapshot("processos-snapshot-escuro", Theme::Dark);
    c.m.app.click("desempenho-processos-arvore");
    c.m.app.click("desempenho-processos-memoria");
    c.m.app.assert_snapshot("desempenho-processos-escuro");
}

#[test]
fn snapshot_da_aba_disco_escuro() {
    let mut c = Cena::para_snapshot("processos-snapshot-disco", Theme::Dark);
    c.m.app.click("desempenho-secoes-disco");
    esperar(&mut c.m.app, "a tabela do disco", |a| {
        a.grid(DISCO).rows.len() == 3
    });
    // A barra não alarga a página: o último botão fica dentro dela.
    assert!(
        c.m.app.rect("desempenho-disco-forcar").x1 <= c.m.app.rect("desempenho-pagina").x1,
        "a barra do disco passou da página"
    );
    c.m.app.click("desempenho-disco-escrita");
    c.m.app.assert_snapshot("desempenho-disco-escuro");
}

/// Contra o servidor SSH de teste: o script de leitura roda no `sh` deste computador, então a
/// lista tem os processos daqui, e o `kill` encerra um de verdade.
#[cfg(target_os = "linux")]
mod remoto {
    use std::process::Command;

    use tirech_omni::abrir::{Opener, Senhas, chave_da_sessao};
    use tirech_omni::ferramentas::Ambiente;
    use tirech_omni::processos::fonte_do_sistema;
    use tirech_omni::tela::tela_com_ferramentas;
    use tomni_ui::terminal::Session;

    use super::servidor::Servidor;
    use super::*;

    struct AbreReal(Rc<Servidor>);

    impl Opener for AbreReal {
        fn open(&self, conexao: &Connection, _senhas: Senhas, n: u32) -> Session {
            Session::new(self.0.builder().connect())
                .key(chave_da_sessao(conexao, n))
                .name(conexao.name.clone())
        }
    }

    #[test]
    fn le_e_encerra_os_processos_do_servidor_da_conexao() {
        let pasta = Pasta::nova("processos-remoto");
        let servidor = Rc::new(Servidor::novo(&pasta.caminho().join("servidor")));
        let mut tree = Tree::default();
        let mut c = Connection::new("servidor", Kind::Ssh);
        c.host = "127.0.0.1".into();
        c.port = servidor.porta;
        c.user = "ana".into();
        c.key_file = Some("/chaves/ana".into());
        tree.add_connection(None, c);
        pasta.semear(&tree);
        pasta.pagina_inicial_padrao("terminais");
        pasta.agente_padrao(false);
        let mut ambiente = Ambiente::de_teste(pasta.caminho(), pasta.ssh());
        ambiente.processos = Rc::new(fonte_do_sistema);
        ambiente.ritmo_dos_processos = Duration::from_millis(300);
        let caminho = pasta.caminho().to_path_buf();
        let s = Rc::clone(&servidor);
        let mut app = TestApp::new(move || {
            tela_com_ferramentas(caminho, ambiente, move |_| Rc::new(AbreReal(s)), |_| {})
        })
        .size(1280, 720)
        .theme(Theme::Light);
        app.double_click("arvore-1");
        esperar(&mut app, "a sessão SSH", |_| {
            Session::find("1-1").is_some_and(|s| s.ssh_channels().is_some())
        });
        let mut filho = Command::new("sleep").arg("30").spawn().expect("o sleep");
        let pid = filho.id().to_string();
        app.click("menus-exibir").click("desempenho");
        app.click("desempenho-secoes-processos");
        app.select("desempenho-processos-maquina", "servidor");
        app.type_into("desempenho-processos-filtro", &pid);
        esperar(&mut app, "o sleep na lista", |a| {
            chaves(a, TABELA) == [pid.clone()]
        });
        assert_eq!(celulas(&app, TABELA, &pid)[0], "sleep");
        assert_eq!(celulas(&app, TABELA, &pid)[7], "sleep 30");
        app.click(&format!("{TABELA}-{pid}"));
        app.click("desempenho-processos-encerrar");
        let status = filho.wait().expect("o fim do sleep");
        assert!(!status.success());
        esperar(&mut app, "o sleep fora da lista", |a| {
            chaves(a, TABELA).is_empty()
        });
        assert!(
            servidor
                .comandos()
                .iter()
                .any(|c| c == &format!("kill -s TERM {pid}")),
            "{:?}",
            servidor.comandos()
        );
    }
}
