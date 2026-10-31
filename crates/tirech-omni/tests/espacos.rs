//! Espaços de trabalho (M28): salvar um painel, uma janela ou todas as janelas, ver os espaços
//! na árvore, abrir de volta (no painel ativo, numa janela nova ou uma janela por janela
//! salva) e organizar em pastas.

mod comum;

use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::espacos::{Espaco, Espacos, Origem};
use tomni_ui::Modifiers;
use tomni_ui::prelude::{Key, Shortcut};
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, exemplo, montar};

/// Os títulos das abas de cada painel da janela atual.
fn abas(app: &TestApp) -> Vec<Vec<String>> {
    app.panes("paineis")
        .panes
        .iter()
        .map(|p| p.tabs.iter().map(|(_, titulo)| titulo.clone()).collect())
        .collect()
}

/// Fecha as sessões que o teste deixou no registro desta thread.
fn encerrar(m: &Montagem) {
    for janela in m.app.windows() {
        let _ = janela;
    }
    for n in 1..=40 {
        for id in 1..=5 {
            if let Some(s) = Session::find(&format!("{id}-{n}")) {
                s.dispose();
            }
        }
    }
}

/// Os espaços da pasta de teste, lidos do disco.
fn espacos(pasta: &Pasta) -> Espacos {
    Espacos::new(pasta.caminho())
}

/// A chave da linha de um espaço na árvore.
fn linha(caminho: &str) -> String {
    format!("arvore-espaco:item:{caminho}")
}

/// Os rótulos dos filhos diretos da raiz dos espaços.
fn dentro_da_raiz(app: &TestApp) -> Vec<String> {
    let linhas = app.tree_items("arvore");
    let Some(inicio) = linhas.iter().position(|l| l.key == "espaco:raiz") else {
        return Vec::new();
    };
    linhas[inicio + 1..]
        .iter()
        .take_while(|l| l.depth > linhas[inicio].depth)
        .filter(|l| l.depth == linhas[inicio].depth + 1)
        .map(|l| l.label.clone())
        .collect()
}

/// Abre `web-01` e `db-01` no painel único da janela atual.
fn abrir_duas(app: &mut TestApp) {
    app.expand("arvore", "1");
    app.double_click("arvore-2").double_click("arvore-3");
}

/// Troca o texto do campo `id` por `texto` (o `type_into` acrescenta ao que já está lá).
fn escrever(app: &mut TestApp, id: &str, texto: &str) {
    app.click(id);
    app.shortcut(Shortcut::new('A', Modifiers::CTRL));
    app.key(Key::Delete);
    app.type_text(texto);
}

/// Salva pelo menu `item` do menu Janela, com o nome `nome` e a pasta padrão.
fn salvar_pelo_menu(app: &mut TestApp, item: &str, nome: &str) {
    app.click("menus-janela").click(item);
    assert!(app.is_open("salvar-espaco-dialogo"), "o diálogo não abriu");
    escrever(app, "espaco-nome", nome);
    app.click("salvar-espaco-dialogo-primary");
}

/// Abre o espaço `caminho` pelo item `como` do menu de contexto da linha dele.
fn abrir_pelo_menu(app: &mut TestApp, caminho: &str, como: &str) {
    app.expand("arvore", "espaco:raiz");
    app.context_menu(&linha(caminho));
    app.click(&format!("menu-espaco:item:{caminho}-{como}"));
    app.frame();
}

#[test]
fn salvar_a_janela_grava_o_arquivo_e_poe_na_arvore() {
    let pasta = Pasta::nova("espacos-salvar-janela");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);

    salvar_pelo_menu(&mut m.app, "salvar-janela-espaco", "diário");
    assert!(!m.app.is_open("salvar-espaco-dialogo"));
    assert!(m.app.visible("aviso-sucesso"));

    let arquivo = pasta.caminho().join("espacos").join("diário.json");
    assert!(arquivo.is_file(), "o arquivo não foi gravado");
    let salvo = espacos(&pasta).ler("diário").unwrap();
    assert_eq!(salvo.origem, Origem::Janela);
    assert_eq!(salvo.janelas.len(), 1);
    let nomes: Vec<&str> = salvo.janelas[0].paineis[0]
        .iter()
        .map(|i| i.nome.as_str())
        .collect();
    assert_eq!(nomes, ["web-01", "db-01"]);
    // Só referências: nenhuma senha vai para o arquivo.
    let texto = std::fs::read_to_string(&arquivo).unwrap();
    assert!(!texto.contains("senha"), "o arquivo de espaço leva senha");

    assert_eq!(dentro_da_raiz(&m.app), ["diário"]);
    encerrar(&m);
}

#[test]
fn a_raiz_dos_espacos_e_sempre_o_ultimo_grupo() {
    let pasta = Pasta::nova("espacos-raiz-ultima");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);
    salvar_pelo_menu(&mut m.app, "salvar-janela-espaco", "primeiro");

    let linhas = m.app.tree_items("arvore");
    let raiz = linhas.iter().position(|l| l.key == "espaco:raiz").unwrap();
    assert!(
        linhas[raiz + 1..]
            .iter()
            .all(|l| l.depth > linhas[raiz].depth),
        "algo vem depois da raiz dos espaços"
    );
    assert_eq!(linhas[raiz].depth, 0);
    assert_eq!(linhas[raiz].label, "Espaços de trabalho");
    // Um grupo novo de conexões fica antes da raiz.
    let raizes: Vec<_> = linhas.iter().filter(|l| l.depth == 0).collect();
    assert_eq!(raizes.last().unwrap().key, "espaco:raiz");
    encerrar(&m);
}

#[test]
fn salvar_o_painel_leva_so_as_abas_dele() {
    let pasta = Pasta::nova("espacos-salvar-painel");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-exibir").click("layout-2x2");
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    m.app.double_click("arvore-3");

    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("salvar-painel-espaco");
    assert!(m.app.is_open("salvar-espaco-dialogo"));
    assert!(
        m.app.text("espaco-nome").contains("web-01")
            || m.app.text("espaco-resumo").contains("1 aba"),
        "o resumo não fala de uma aba só: {}",
        m.app.text("espaco-resumo")
    );
    escrever(&mut m.app, "espaco-nome", "só-o-web");
    m.app.click("salvar-espaco-dialogo-primary");

    let salvo = espacos(&pasta).ler("só-o-web").unwrap();
    assert_eq!(salvo.origem, Origem::Painel);
    assert_eq!(salvo.total_de_abas(), 1);
    encerrar(&m);
}

#[test]
fn salvar_todas_guarda_uma_entrada_por_janela() {
    let pasta = Pasta::nova("espacos-salvar-todas");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let outra = m.app.windows()[1];
    m.app.set_active_window(outra);
    m.app.double_click("arvore-4");
    m.app.frame();
    let principal = m.app.windows()[0];
    m.app.set_active_window(principal);
    m.app.switch_to(principal);

    salvar_pelo_menu(&mut m.app, "salvar-todas-espaco", "tudo");
    let salvo = espacos(&pasta).ler("tudo").unwrap();
    assert_eq!(salvo.origem, Origem::Todas);
    assert_eq!(salvo.janelas.len(), 2);
    assert_eq!(salvo.total_de_abas(), 3);
    encerrar(&m);
}

#[test]
fn sem_abas_nao_ha_o_que_salvar() {
    let pasta = Pasta::nova("espacos-sem-abas");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("menus-janela").click("salvar-janela-espaco");
    assert!(!m.app.is_open("salvar-espaco-dialogo"));
    assert!(m.app.visible("aviso-atencao"));
    assert!(espacos(&pasta).listar().is_empty());
}

#[test]
fn nome_invalido_reabre_o_dialogo_com_o_erro() {
    let pasta = Pasta::nova("espacos-nome-invalido");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);
    m.app.click("menus-janela").click("salvar-janela-espaco");
    escrever(&mut m.app, "espaco-nome", "a/b");
    m.app.click("salvar-espaco-dialogo-primary");
    m.app.frame();
    assert!(m.app.is_open("salvar-espaco-dialogo"), "o diálogo fechou");
    assert!(m.app.visible("espaco-erro"));
    assert!(espacos(&pasta).listar().is_empty());
    encerrar(&m);
}

#[test]
fn salvar_com_nome_existente_pede_para_substituir() {
    let pasta = Pasta::nova("espacos-substituir");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);
    salvar_pelo_menu(&mut m.app, "salvar-janela-espaco", "meu");
    assert_eq!(espacos(&pasta).ler("meu").unwrap().total_de_abas(), 2);

    // Fecha uma aba e salva de novo com o mesmo nome.
    let chave = m.app.panes("paineis").panes[0].tabs[1].0.clone();
    m.app.close_tab("paineis-painel-0", &chave);
    m.app.frame();
    salvar_pelo_menu(&mut m.app, "salvar-janela-espaco", "meu");
    m.app.frame();
    assert!(m.app.is_open("substituir-espaco-dialogo"));
    // Cancelar deixa o arquivo como estava.
    m.app.click("substituir-espaco-dialogo-close");
    assert_eq!(espacos(&pasta).ler("meu").unwrap().total_de_abas(), 2);

    salvar_pelo_menu(&mut m.app, "salvar-janela-espaco", "meu");
    m.app.frame();
    m.app.click("substituir-espaco-dialogo-primary");
    assert_eq!(espacos(&pasta).ler("meu").unwrap().total_de_abas(), 1);
    encerrar(&m);
}

#[test]
fn salvar_numa_pasta_nova_cria_a_pasta() {
    let pasta = Pasta::nova("espacos-pasta-nova");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);
    m.app.click("menus-janela").click("salvar-janela-espaco");
    escrever(&mut m.app, "espaco-nome", "rotina");
    m.app.select("espaco-pasta", "Nova pasta…");
    escrever(&mut m.app, "espaco-pasta-nova", "Produção");
    m.app.click("salvar-espaco-dialogo-primary");

    assert!(
        pasta
            .caminho()
            .join("espacos")
            .join("Produção")
            .join("rotina.json")
            .is_file()
    );
    m.app.expand("arvore", "espaco:raiz");
    assert!(
        m.app
            .tree_items("arvore")
            .iter()
            .any(|l| l.key == "espaco:item:Produção/rotina")
    );
    encerrar(&m);
}

/// Salva a janela com `web-01` e `db-01` como `nome` e fecha as abas (e a janela, se houver
/// mais de uma), para o teste abrir o espaço do zero.
fn salvar_e_limpar(m: &mut Montagem, nome: &str) {
    abrir_duas(&mut m.app);
    salvar_pelo_menu(&mut m.app, "salvar-janela-espaco", nome);
    let chaves: Vec<String> = m.app.panes("paineis").panes[0]
        .tabs
        .iter()
        .map(|(c, _)| c.clone())
        .collect();
    for chave in chaves {
        m.app.close_tab("paineis-painel-0", &chave);
    }
    m.app.frame();
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);
}

#[test]
fn abrir_espaco_de_janela_abre_numa_janela_nova() {
    let pasta = Pasta::nova("espacos-abrir-janela");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    salvar_e_limpar(&mut m, "diário");

    abrir_pelo_menu(&mut m.app, "diário", "abrir");
    assert_eq!(m.app.windows().len(), 2, "devia abrir uma janela nova");
    assert_eq!(
        abas(&m.app),
        [Vec::<String>::new()],
        "a principal ficou vazia"
    );
    m.app.switch_to(m.app.windows()[1]);
    assert_eq!(abas(&m.app), [["web-01", "db-01"]]);
    encerrar(&m);
}

#[test]
fn abrir_espaco_de_painel_usa_o_painel_ativo() {
    let pasta = Pasta::nova("espacos-abrir-painel");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.double_click("arvore-2");
    let chave = m.app.panes("paineis").panes[0].tabs[0].0.clone();
    m.app.context_menu(&format!("paineis-painel-0-{chave}"));
    m.app.click("salvar-painel-espaco");
    escrever(&mut m.app, "espaco-nome", "um");
    m.app.click("salvar-espaco-dialogo-primary");

    abrir_pelo_menu(&mut m.app, "um", "abrir");
    assert_eq!(m.app.windows().len(), 1, "não devia abrir janela");
    assert_eq!(abas(&m.app), [["web-01", "web-01"]]);
    encerrar(&m);
}

#[test]
fn abrir_numa_nova_janela_e_no_painel_ativo_pelo_menu() {
    let pasta = Pasta::nova("espacos-abrir-como");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    salvar_e_limpar(&mut m, "dupla");

    abrir_pelo_menu(&mut m.app, "dupla", "painel-ativo");
    assert_eq!(m.app.windows().len(), 1);
    assert_eq!(abas(&m.app), [["web-01", "db-01"]]);

    abrir_pelo_menu(&mut m.app, "dupla", "nova-janela");
    assert_eq!(m.app.windows().len(), 2);
    m.app.switch_to(m.app.windows()[1]);
    assert_eq!(abas(&m.app), [["web-01", "db-01"]]);
    encerrar(&m);
}

#[test]
fn abrir_todas_reabre_uma_janela_por_janela_salva() {
    let pasta = Pasta::nova("espacos-abrir-todas");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);
    m.app.click("menus-janela").click("nova-janela");
    m.app.frame();
    let outra = m.app.windows()[1];
    m.app.set_active_window(outra);
    m.app.double_click("arvore-4");
    m.app.frame();
    let principal = m.app.windows()[0];
    m.app.set_active_window(principal);
    m.app.switch_to(principal);
    salvar_pelo_menu(&mut m.app, "salvar-todas-espaco", "tudo");

    // Fecha a secundária e as abas da principal.
    m.app.switch_to(outra);
    assert!(!m.app.close_window(outra));
    m.app.click("fechar-janela-dialogo-primary");
    m.app.frame();
    let chaves: Vec<String> = m.app.panes("paineis").panes[0]
        .tabs
        .iter()
        .map(|(c, _)| c.clone())
        .collect();
    for chave in chaves {
        m.app.close_tab("paineis-painel-0", &chave);
    }
    m.app.frame();
    assert_eq!(m.app.windows().len(), 1);

    abrir_pelo_menu(&mut m.app, "tudo", "abrir");
    assert_eq!(m.app.windows().len(), 3, "uma janela por janela salva");
    m.app.switch_to(m.app.windows()[1]);
    assert_eq!(abas(&m.app), [["web-01", "db-01"]]);
    m.app.switch_to(m.app.windows()[2]);
    assert_eq!(abas(&m.app), [["Roteador"]]);
    encerrar(&m);
}

#[test]
fn abrir_com_conexao_apagada_avisa_quais_faltam() {
    let pasta = Pasta::nova("espacos-faltando");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    salvar_e_limpar(&mut m, "dupla");

    // Troca as conexões que o espaço aponta por duas que não existem mais.
    let armazenamento = Armazenamento::new(pasta.caminho());
    let _ = armazenamento;
    let esp = espacos(&pasta);
    let mut salvo: Espaco = esp.ler("dupla").unwrap();
    for item in salvo.janelas[0].paineis[0].iter_mut() {
        item.id = Some(999);
        item.host = "sumiu.example".into();
        item.user = "ninguém".into();
    }
    esp.gravar("", "dupla", &salvo, true).unwrap();

    abrir_pelo_menu(&mut m.app, "dupla", "painel-ativo");
    assert!(m.app.visible("aviso-atencao"), "sem o aviso de faltantes");
    assert_eq!(abas(&m.app), [Vec::<String>::new()]);
    encerrar(&m);
}

#[test]
fn renomear_um_espaco_muda_o_arquivo() {
    let pasta = Pasta::nova("espacos-renomear");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    salvar_e_limpar(&mut m, "velho");

    m.app.expand("arvore", "espaco:raiz");
    m.app.context_menu(&linha("velho"));
    m.app.click("menu-espaco:item:velho-renomear");
    m.app.key(Key::Home);
    for _ in 0.."velho".chars().count() {
        m.app.key(Key::Delete);
    }
    m.app.type_text("novo").key(Key::Enter);
    m.app.frame();

    let esp = espacos(&pasta);
    assert!(!esp.existe("", "velho"));
    assert!(esp.existe("", "novo"));
    assert_eq!(dentro_da_raiz(&m.app), ["novo"]);
    encerrar(&m);
}

#[test]
fn apagar_um_espaco_pede_confirmacao() {
    let pasta = Pasta::nova("espacos-apagar");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    salvar_e_limpar(&mut m, "descartável");

    m.app.expand("arvore", "espaco:raiz");
    m.app.context_menu(&linha("descartável"));
    m.app.click("menu-espaco:item:descartável-apagar");
    assert!(m.app.is_open("apagar-espaco-dialogo"));
    m.app.click("apagar-espaco-dialogo-close");
    assert!(espacos(&pasta).existe("", "descartável"));

    m.app.context_menu(&linha("descartável"));
    m.app.click("menu-espaco:item:descartável-apagar");
    m.app.click("apagar-espaco-dialogo-primary");
    m.app.frame();
    assert!(!espacos(&pasta).existe("", "descartável"));
    assert!(dentro_da_raiz(&m.app).is_empty());
    encerrar(&m);
}

#[test]
fn arrastar_para_outra_pasta_move_o_arquivo() {
    let pasta = Pasta::nova("espacos-mover");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    abrir_duas(&mut m.app);
    espacos(&pasta).criar_pasta("", "Rotinas").unwrap();
    salvar_pelo_menu(&mut m.app, "salvar-janela-espaco", "arrastado");
    m.app.click("menus-janela").click("salvar-janela-espaco");
    m.app.click("salvar-espaco-dialogo-close");
    m.app.expand("arvore", "espaco:raiz");
    m.app.context_menu("arvore-espaco:raiz");
    m.app.click("menu-espaco:raiz-atualizar");
    m.app.expand("arvore", "espaco:raiz");

    m.app
        .drag(linha("arrastado").as_str(), "arvore-espaco:pasta:Rotinas");
    m.app.frame();
    let esp = espacos(&pasta);
    assert!(!esp.existe("", "arrastado"));
    assert!(esp.existe("Rotinas", "arrastado"));
    encerrar(&m);
}

#[test]
fn a_raiz_dos_espacos_nao_se_mexe() {
    let pasta = Pasta::nova("espacos-raiz-protegida");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("arvore-espaco:raiz");
    for botao in ["subir", "descer", "apagar"] {
        if m.app.visible(botao) {
            assert!(!m.app.enabled(botao), "{botao} está ligado na raiz");
        }
    }
    m.app.context_menu("arvore-espaco:raiz");
    assert!(m.app.visible("menu-espaco:raiz-nova-pasta"));
    assert!(m.app.visible("menu-espaco:raiz-atualizar"));
}

#[test]
fn conexoes_nao_vao_para_a_raiz_dos_espacos() {
    let pasta = Pasta::nova("espacos-sem-conexoes");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    m.app.expand("arvore", "1");
    m.app.drag("arvore-4", "arvore-espaco:raiz");
    m.app.frame();
    let tree = pasta.conexoes();
    assert!(tree.find(4).is_some(), "a conexão sumiu");
    assert!(
        m.app
            .tree_items("arvore")
            .iter()
            .filter(|l| l.key == "4")
            .all(|l| l.depth == 0),
        "a conexão foi parar dentro da raiz dos espaços"
    );
}

/// Os recentes gravados na pasta de teste.
fn recentes(pasta: &Pasta) -> String {
    std::fs::read_to_string(pasta.armazenamento().arquivo_recentes()).unwrap_or_default()
}

#[test]
fn abrir_um_espaco_vai_para_os_recentes_e_salvar_nao() {
    let pasta = Pasta::nova("espacos-recentes");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    salvar_e_limpar(&mut m, "diário");
    assert!(!recentes(&pasta).contains("diário"), "salvar registrou");

    abrir_pelo_menu(&mut m.app, "diário", "painel-ativo");
    let texto = recentes(&pasta);
    assert!(texto.contains("diário"), "abrir não registrou: {texto}");
    assert!(texto.contains("espaco") || texto.contains("Espaco"));
    encerrar(&m);
}

#[test]
fn renomear_o_espaco_reaponta_o_recente() {
    let pasta = Pasta::nova("espacos-recente-renomeado");
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    salvar_e_limpar(&mut m, "velho");
    abrir_pelo_menu(&mut m.app, "velho", "painel-ativo");

    m.app.context_menu(&linha("velho"));
    m.app.click("menu-espaco:item:velho-renomear");
    m.app.key(Key::Home);
    for _ in 0.."velho".chars().count() {
        m.app.key(Key::Delete);
    }
    m.app.type_text("novo").key(Key::Enter);
    m.app.frame();
    let texto = recentes(&pasta);
    assert!(texto.contains("novo"), "o recente não acompanhou: {texto}");
    assert!(!texto.contains("velho"), "sobrou o caminho antigo: {texto}");
    encerrar(&m);
}
