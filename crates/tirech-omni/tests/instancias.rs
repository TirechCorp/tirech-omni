//! O canal entre as cópias do app na mesma pasta de dados: a secundária manda o que muda para a
//! principal, que grava se o arquivo ainda é o lido. Uma tela por processo é a principal ou a
//! secundária de uma pasta (o registro das pastas só de leitura é do processo): o outro lado é
//! feito à mão, com as funções do canal.

mod comum;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tomni_ui::files::Live;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use tirech_omni::armazenamento::{self, Trava};
use tirech_omni::instancias::{
    self, ChamadoDeRede, Emprestimo, Fila, FilaDeRedes, Pedido, Resposta, ServidorDasInstancias,
    VERSAO, resumo,
};
use tirech_omni::modelo::Tree;
use tirech_omni::redes::{PerfilRede, ProxyRede, RedeResolvida};

use comum::{Pasta, exemplo, montar, montar_com_ambiente};

/// O texto do widget `id`, ou `None` se ele (ainda) não existe.
fn texto(app: &TestApp, id: &str) -> Option<String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.text(id))).ok()
}

/// Espera, girando a tela, até `pronto`.
fn esperar(app: &mut TestApp, o_que: &str, mut pronto: impl FnMut(&TestApp) -> bool) {
    let limite = Instant::now() + Duration::from_secs(15);
    while !pronto(app) {
        assert!(Instant::now() < limite, "não chegou: {o_que}");
        app.wait(Duration::from_millis(20));
    }
}

/// Manda `pedido` como uma secundária faria, girando a tela da principal enquanto ela responde.
fn mandar(app: &mut TestApp, pasta: &Pasta, pedido: Pedido) -> Resposta {
    let endereco = instancias::endereco_da_pasta(pasta.caminho());
    let ida = thread::spawn(move || instancias::mandar(&endereco, &pedido));
    let limite = Instant::now() + Duration::from_secs(15);
    while !ida.is_finished() {
        assert!(Instant::now() < limite, "a principal não respondeu");
        app.wait(Duration::from_millis(20));
    }
    ida.join().unwrap().unwrap()
}

fn pedido(pasta: &Pasta, documento: &str, lido: Option<&[u8]>, conteudo: String) -> Pedido {
    Pedido {
        instancias: VERSAO,
        documento: documento.to_owned(),
        pasta: pasta.caminho().to_owned(),
        esperado: resumo(lido),
        conteudo,
    }
}

#[test]
fn a_principal_grava_o_que_a_secundaria_manda_se_o_arquivo_ainda_e_o_lido() {
    let pasta = Pasta::nova("instancias-principal");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    let arquivo = pasta.armazenamento().arquivo_conexoes();
    let lido = std::fs::read(&arquivo).unwrap();
    let mut arvore: Tree = serde_json::from_slice(&lido).unwrap();
    arvore.add_group(None, "Da secundária");
    let novo = serde_json::to_string_pretty(&arvore).unwrap();

    let resposta = mandar(
        &mut m.app,
        &pasta,
        pedido(&pasta, "connections.json", Some(&lido), novo.clone()),
    );
    assert_eq!(resposta, Resposta::Gravado);
    assert_eq!(std::fs::read_to_string(&arquivo).unwrap(), novo);
    esperar(&mut m.app, "a árvore relida", |app| {
        texto(app, "arvore-6").as_deref() == Some("Da secundária")
    });

    // O mesmo pedido de novo: o arquivo já não é o que a secundária leu.
    let resposta = mandar(
        &mut m.app,
        &pasta,
        pedido(&pasta, "connections.json", Some(&lido), "{}".into()),
    );
    assert_eq!(resposta, Resposta::Mudou);
    assert_eq!(std::fs::read_to_string(&arquivo).unwrap(), novo);

    // O que não vai pelo canal não se grava.
    let agora = std::fs::read(&arquivo).unwrap();
    for pedido in [
        pedido(&pasta, "recentes.json", None, "{}".into()),
        pedido(
            &pasta,
            "connections.json",
            Some(&agora),
            "não é JSON".into(),
        ),
        Pedido {
            pasta: pasta.caminho().join("outra"),
            ..pedido(&pasta, "connections.json", Some(&agora), novo.clone())
        },
    ] {
        let resposta = mandar(&mut m.app, &pasta, pedido.clone());
        assert!(
            matches!(resposta, Resposta::Recusado { .. }),
            "{pedido:?}: {resposta:?}"
        );
    }
    assert_eq!(std::fs::read(&arquivo).unwrap(), agora);

    // As configurações: a principal grava e relê (e a memória dela, gravada de novo ao mudar,
    // mantém a mudança).
    let configuracoes = pasta.armazenamento().arquivo_configuracoes();
    let lido = std::fs::read(&configuracoes).ok();
    let mut valor: serde_json::Value = lido
        .as_deref()
        .and_then(|b| serde_json::from_slice(b).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    valor["geral"]["recentes_max"] = serde_json::json!(7);
    let resposta = mandar(
        &mut m.app,
        &pasta,
        pedido(
            &pasta,
            "configuracoes.json",
            lido.as_deref(),
            valor.to_string(),
        ),
    );
    assert_eq!(resposta, Resposta::Gravado);
    m.app.frame();
    let relido: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&configuracoes).unwrap()).unwrap();
    assert_eq!(relido["geral"]["recentes_max"], 7, "{relido}");
}

#[test]
fn a_secundaria_manda_a_mudanca_e_junta_com_a_da_principal() {
    let pasta = Pasta::nova("instancias-secundaria");
    pasta.semear(&exemplo());
    let arquivo = pasta.armazenamento().arquivo_conexoes();
    // Outra cópia segura a trava da pasta: esta abre como secundária.
    let primeira = armazenamento::travar(pasta.caminho()).unwrap();
    assert!(matches!(primeira, Trava::Travada(_)));
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| {
        a.dono_do_estado = false;
    });

    // A principal falsa: no primeiro pedido, ela mesma acabou de gravar um grupo (com o id que a
    // secundária também vai usar), e o arquivo já não é o lido.
    let fila: Fila = Live::detached(Vec::new());
    let endereco = instancias::endereco_da_pasta(pasta.caminho());
    let servidor =
        ServidorDasInstancias::iniciar(&endereco, fila.clone(), Live::detached(Vec::new()))
            .unwrap();
    let pedidos = Arc::new(AtomicUsize::new(0));
    let parar = Arc::new(AtomicBool::new(false));
    let principal = {
        let (arquivo, pedidos, parar) = (arquivo.clone(), Arc::clone(&pedidos), Arc::clone(&parar));
        thread::spawn(move || {
            while !parar.load(Ordering::SeqCst) {
                for p in instancias::tirar(&fila) {
                    if pedidos.fetch_add(1, Ordering::SeqCst) == 0 {
                        let mut arvore: Tree =
                            serde_json::from_slice(&std::fs::read(&arquivo).unwrap()).unwrap();
                        arvore.add_group(None, "Da principal");
                        std::fs::write(&arquivo, serde_json::to_vec_pretty(&arvore).unwrap())
                            .unwrap();
                    }
                    // O `gravar_se_ainda_for` da principal, por fora do registro das pastas só de
                    // leitura deste processo (que é o da secundária).
                    let agora = std::fs::read(&arquivo).ok();
                    let resposta = if resumo(agora.as_deref()) == p.pedido.esperado {
                        std::fs::write(&arquivo, &p.pedido.conteudo).unwrap();
                        Resposta::Gravado
                    } else {
                        Resposta::Mudou
                    };
                    p.responder(resposta);
                }
                thread::sleep(Duration::from_millis(10));
            }
        })
    };

    m.app.click("nova-conexao");
    m.app.frame();

    let limite = Instant::now() + Duration::from_secs(15);
    let arvore = loop {
        assert!(
            Instant::now() < limite,
            "a mudança não chegou ao arquivo ({} pedidos): {:?}",
            pedidos.load(Ordering::SeqCst),
            texto(&m.app, "aviso-atencao"),
        );
        let arvore = pasta.conexoes();
        if arvore
            .connections()
            .iter()
            .any(|c| c.name.starts_with("Nova conexão"))
        {
            break arvore;
        }
        m.app.wait(Duration::from_millis(20));
    };
    let texto_do_arquivo = pasta.texto_conexoes();
    assert!(
        texto_do_arquivo.contains("Da principal"),
        "a mudança da principal se perdeu: {texto_do_arquivo}"
    );
    assert_eq!(
        pedidos.load(Ordering::SeqCst),
        2,
        "mudou, juntou e mandou de novo"
    );
    // Os dois lados deram o id 6: o da secundária ganhou outro.
    let nova = arvore
        .connections()
        .into_iter()
        .find(|c| c.name.starts_with("Nova conexão"))
        .unwrap()
        .id;
    assert_eq!(nova, 7);
    assert!(arvore.next_id > 7);

    // A secundária relê o que ficou no disco: o grupo da principal aparece nela.
    esperar(&mut m.app, "o grupo da principal na secundária", |app| {
        texto(app, "arvore-6").as_deref() == Some("Da principal")
    });
    assert!(instancias::esperar_pendentes(Duration::from_secs(5)));

    parar.store(true, Ordering::SeqCst);
    principal.join().unwrap();
    drop(servidor);
    drop(primeira);
}

/// A secundária não liga a VPN embutida dela: pede à principal, recebe a ponte, espera o motor de
/// lá ficar pronto (com os avisos) e, ao soltar, a principal esquece o uso. Sem principal, a
/// secundária fica sabendo que liga o motor ela mesma.
#[test]
fn a_principal_empresta_a_rede_e_esquece_quando_a_secundaria_solta() {
    let pasta = Pasta::nova("instancias-rede");
    let endereco = instancias::endereco_da_pasta(pasta.caminho());
    assert!(
        Emprestimo::pedir(&endereco, 7).is_err(),
        "sem principal, a secundária liga o próprio motor"
    );

    // A ponte "do motor" da principal: uma porta que só abre depois do pedido.
    let porta = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let redes: FilaDeRedes = Live::detached(Vec::new());
    let servidor =
        ServidorDasInstancias::iniciar(&endereco, Live::detached(Vec::new()), redes.clone())
            .unwrap();
    let (soltos_tx, soltos) = std::sync::mpsc::channel();
    let parar = Arc::new(AtomicBool::new(false));
    let principal = {
        let parar = Arc::clone(&parar);
        thread::spawn(move || {
            let mut ponte = None;
            while !parar.load(Ordering::SeqCst) {
                for chamado in instancias::tirar(&redes) {
                    match chamado {
                        ChamadoDeRede::Pedir {
                            emprestimo: _,
                            rede,
                            responder,
                        } => {
                            let resposta = if rede == 7 {
                                let mut perfil = PerfilRede::new("Cliente A");
                                perfil.id = 7;
                                perfil.proxy = Some(ProxyRede {
                                    port: porta,
                                    ..ProxyRede::novo()
                                });
                                perfil.subir = Some("true".into());
                                perfil.espera_secs = 10;
                                let mut rede = RedeResolvida::sem_segredos(perfil);
                                rede.proxy_usuario = Some("ponte".into());
                                rede.proxy_senha = Some("da-ponte".into());
                                Ok(rede)
                            } else {
                                Err("o motor não subiu".to_owned())
                            };
                            let _ = responder.send(resposta);
                            // A ponte abre um pouco depois, como um motor subindo.
                            if rede == 7 {
                                thread::sleep(Duration::from_millis(300));
                                ponte = Some(
                                    std::net::TcpListener::bind(("127.0.0.1", porta)).unwrap(),
                                );
                            }
                        }
                        ChamadoDeRede::Soltar(emprestimo) => {
                            let _ = soltos_tx.send(emprestimo);
                        }
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
            drop(ponte);
        })
    };

    let emprestimo = Emprestimo::pedir(&endereco, 7)
        .expect("a principal atende")
        .expect("a principal empresta");
    match &emprestimo.proxy {
        tomni_ui::terminal::Proxy::Socks5 {
            host,
            port,
            user,
            password,
        } => {
            assert_eq!((host.as_str(), *port), ("127.0.0.1", porta));
            assert_eq!(user.as_deref(), Some("ponte"));
            assert_eq!(password.as_deref(), Some("da-ponte"));
        }
        outro => panic!("a ponte é SOCKS5: {outro:?}"),
    }
    let mut avisos = Vec::new();
    emprestimo
        .esperar(&mut |a| avisos.push(a.to_owned()))
        .expect("o motor da principal ficou pronto");
    assert_eq!(
        avisos.len(),
        2,
        "subindo e pronta, vindos da principal: {avisos:?}"
    );
    assert!(emprestimo.valendo());
    assert!(
        soltos.try_recv().is_err(),
        "enquanto a secundária usa, a principal conta o uso"
    );

    drop(emprestimo);
    soltos
        .recv_timeout(Duration::from_secs(5))
        .expect("solto o empréstimo, a principal esquece o uso");

    let recusado = Emprestimo::pedir(&endereco, 8).expect("a principal atende");
    assert_eq!(recusado.err().as_deref(), Some("o motor não subiu"));

    parar.store(true, Ordering::SeqCst);
    principal.join().unwrap();
    drop(servidor);
}

/// A tela da principal atende o pedido de rede: um perfil que não existe ou que não é uma VPN
/// embutida ela não empresta; o motor que não sobe volta com o motivo dele.
#[test]
fn a_tela_da_principal_recusa_a_rede_que_nao_pode_emprestar() {
    let pasta = Pasta::nova("instancias-rede-tela");
    let mut arvore = exemplo();
    let proxy = arvore.add_perfil(PerfilRede::new("Só proxy"));
    let mut vpn = PerfilRede::new("VPN do cliente");
    vpn.proxy = None;
    vpn.openvpn = Some(tirech_omni::redes::OpenVpnRede::default());
    let vpn = arvore.add_perfil(vpn);
    pasta.semear(&arvore);
    let mut m = montar(&pasta, Theme::Light);

    let mut pedir = |rede: u64| {
        let endereco = instancias::endereco_da_pasta(pasta.caminho());
        let ida = thread::spawn(move || Emprestimo::pedir(&endereco, rede));
        let limite = Instant::now() + Duration::from_secs(15);
        while !ida.is_finished() {
            assert!(Instant::now() < limite, "a principal não respondeu");
            m.app.wait(Duration::from_millis(20));
        }
        ida.join()
            .unwrap()
            .expect("a principal atende")
            .expect_err("a principal não empresta")
    };
    let sumiu = pedir(9999);
    assert_eq!(pedir(proxy), sumiu, "o perfil de proxy não se empresta");
    let erro = pedir(vpn);
    assert_ne!(erro, sumiu);
    assert!(erro.contains("VPN do cliente"), "o motivo do motor: {erro}");
}
