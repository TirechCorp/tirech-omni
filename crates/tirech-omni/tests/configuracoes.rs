//! A página Configurações do M35: um teste de efeito por configuração do catálogo, a busca, a
//! exportação e a importação, e os cards de ação.
//!
//! Cada `efeito_*` muda o controle da página como uma pessoa e confere o que ficou no
//! `configuracoes.json`. O que a configuração faz no app (o tema, o terminal, o cofre...) tem
//! os testes dos marcos de cada área.
#![allow(non_snake_case)]

mod comum;

use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::configuracoes::catalogo::{self, Tipo, id_do_card, id_do_valor};
use tirech_omni::configuracoes::{
    AbrirAbaAuto, AbrirAbaIa, AbrirEm, AbrirEspaco, AbrirIa, AoFechar, AssistentePreferido,
    BotaoDireito, Campainha, ChaveNova, ColarArquivos, Configuracoes, ConflitoCfg,
    ConversaAoControlar, Cursor, EnviarEditado, Esquema, Idioma, MetodoIa, ModoComparar, ModoNotas,
    ModoReconectar, PaginaInicial, Tema, TipoChave, TmuxAoFechar, ValidadeLink, X11Padrao,
};
use tirech_omni::modelo::{FtpCodificacao, FtpModo, FtpSeguranca, FtpTls, S3Estilo};
use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, item_ssh, montar_com_ambiente, montar_pura};

/// A tela na página Configurações, numa pasta nova.
fn abrir(nome: &str) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(&format!("config-pagina-{nome}"));
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    expandir_tudo(&mut m.app);
    (pasta, m)
}

/// O que está gravado no `configuracoes.json`.
fn gravada(pasta: &Pasta) -> Configuracoes {
    Configuracoes::carregar(&Armazenamento::new(pasta.caminho())).0
}

/// O texto do `configuracoes.json`.
fn arquivo(pasta: &Pasta) -> String {
    std::fs::read_to_string(pasta.armazenamento().arquivo_configuracoes()).unwrap_or_default()
}

fn existe(app: &mut TestApp, id: &str) -> bool {
    app.ui().find(id).is_some()
}

/// Se o controle `id` está na tela (existe e não está escondido).
fn aparece(app: &mut TestApp, id: &str) -> bool {
    existe(app, id) && app.visible(id)
}

/// Abre todas as seções da sanfona, se alguma estiver recolhida.
fn expandir_tudo(app: &mut TestApp) {
    if app.label("config-expandir-tudo") == "Expandir tudo" {
        app.click("config-expandir-tudo");
    }
}

/// Abre as seções e os grupos (os `SettingsExpander`) até o controle `id` aparecer.
fn alcancar(app: &mut TestApp, id: &str) {
    if !aparece(app, id) {
        expandir_tudo(app);
    }
    for grupo in catalogo::CATALOGO.iter().filter(|d| d.tipo == Tipo::Grupo) {
        if aparece(app, id) {
            return;
        }
        let cabecalho = format!("{}-cabecalho", grupo.id());
        if existe(app, &cabecalho) {
            app.click(&cabecalho);
        }
    }
    assert!(aparece(app, id), "{id:?} não aparece na página");
}

/// Troca o texto de um `Edit`.
fn digitar(app: &mut TestApp, id: &str, texto: &str) {
    app.click(id).shortcut('a').type_text(texto);
}

macro_rules! efeito_bool {
    ($nome:ident, $chave:literal, $($p:ident).+) => {
        #[test]
        fn $nome() {
            let (pasta, mut m) = abrir(stringify!($nome));
            let id = id_do_valor($chave);
            alcancar(&mut m.app, &id);
            let antes = m.app.checked(&id);
            m.app.click(&id);
            assert_eq!(m.app.checked(&id), !antes);
            assert_eq!(gravada(&pasta).$($p).+, !antes, "{}", arquivo(&pasta));
            assert!(m.app.file_dialogs().is_empty());
        }
    };
}

macro_rules! efeito_numero {
    ($nome:ident, $chave:literal, $valor:expr, $($p:ident).+) => {
        #[test]
        fn $nome() {
            let (pasta, mut m) = abrir(stringify!($nome));
            let id = id_do_valor($chave);
            alcancar(&mut m.app, &id);
            m.app.set_value(&id, $valor);
            assert_eq!(m.app.value(&id), $valor);
            assert_eq!(gravada(&pasta).$($p).+ as f64, $valor, "{}", arquivo(&pasta));
        }
    };
}

macro_rules! efeito_escolha {
    ($nome:ident, $chave:literal, $todas:expr, $($p:ident).+) => {
        #[test]
        fn $nome() {
            let (pasta, mut m) = abrir(stringify!($nome));
            let id = id_do_valor($chave);
            alcancar(&mut m.app, &id);
            let atual = m.app.text(&id);
            let outra = $todas
                .iter()
                .copied()
                .map(|o| o.nome())
                .find(|n| *n != atual)
                .expect("há mais de uma opção");
            m.app.select(&id, &outra);
            assert_eq!(m.app.text(&id), outra);
            assert_eq!(gravada(&pasta).$($p).+.nome(), outra, "{}", arquivo(&pasta));
        }
    };
}

macro_rules! efeito_texto {
    ($nome:ident, $chave:literal, $texto:expr, $($p:ident).+) => {
        #[test]
        fn $nome() {
            let (pasta, mut m) = abrir(stringify!($nome));
            let id = id_do_valor($chave);
            alcancar(&mut m.app, &id);
            digitar(&mut m.app, &id, $texto);
            assert_eq!(m.app.text(&id), $texto);
            assert_eq!(gravada(&pasta).$($p).+, $texto, "{}", arquivo(&pasta));
        }
    };
}

// Os nomes `efeito_<categoria>__<campo>` seguem a chave do catálogo.

efeito_bool!(
    efeito_geral__restaurar_sessoes,
    "geral.restaurar_sessoes",
    geral.restaurar_sessoes
);
efeito_bool!(
    efeito_geral__iniciar_com_sistema,
    "geral.iniciar_com_sistema",
    geral.iniciar_com_sistema
);

efeito_bool!(
    efeito_geral__iniciar_na_bandeja,
    "geral.iniciar_na_bandeja",
    geral.iniciar_na_bandeja
);

efeito_bool!(
    efeito_geral__atalho_no_menu,
    "geral.atalho_no_menu",
    geral.atalho_no_menu
);

efeito_bool!(
    efeito_geral__medidor_cpu,
    "geral.medidor_cpu",
    geral.medidor_cpu
);

efeito_bool!(
    efeito_geral__confirmar_fechar,
    "geral.confirmar_fechar",
    geral.confirmar_fechar
);
/// Desliga em Geral o interruptor `chave` e confere que as seções `somem` saem da página, que
/// as `ficam` continuam e que tudo volta ao ligar de novo. Devolve a pasta, para o que foi gravado.
fn desligar_area(nome: &str, chave: &str, somem: &[&str], ficam: &[&str]) -> Pasta {
    let (pasta, mut m) = abrir(nome);
    let id = id_do_valor(chave);
    alcancar(&mut m.app, &id);
    for s in somem.iter().chain(ficam) {
        assert!(aparece(&mut m.app, &format!("config-secao-{s}")), "{s}");
    }
    m.app.click(&id);
    assert!(!m.app.checked(&id));
    for s in somem {
        assert!(
            !aparece(&mut m.app, &format!("config-secao-{s}")),
            "{s} some"
        );
    }
    for s in ficam {
        assert!(
            aparece(&mut m.app, &format!("config-secao-{s}")),
            "{s} fica"
        );
    }
    m.app.click(&id);
    for s in somem {
        assert!(
            aparece(&mut m.app, &format!("config-secao-{s}")),
            "{s} volta"
        );
    }
    m.app.click(&id);
    pasta
}

#[test]
fn efeito_geral__assistente_ia() {
    let pasta = desligar_area(
        "geral-assistente-ia",
        "geral.assistente_ia",
        &["ia", "ia_servidor"],
        &["ia_local", "mcp"],
    );
    let c = gravada(&pasta);
    assert!(!c.geral.assistente_ia, "{}", arquivo(&pasta));
    assert!(c.geral.mcp);
}

#[test]
fn efeito_geral__mcp() {
    let pasta = desligar_area(
        "geral-mcp",
        "geral.mcp",
        &["ia_local", "mcp"],
        &["ia", "ia_servidor"],
    );
    let c = gravada(&pasta);
    assert!(!c.geral.mcp, "{}", arquivo(&pasta));
    assert!(c.geral.assistente_ia);
}

efeito_bool!(
    efeito_terminal__links_com_ctrl,
    "terminal.links_com_ctrl",
    terminal.links_com_ctrl
);
efeito_bool!(
    efeito_terminal__copiar_ao_selecionar,
    "terminal.copiar_ao_selecionar",
    terminal.copiar_ao_selecionar
);
efeito_bool!(
    efeito_conexoes__importar_senhas,
    "conexoes.importar_senhas",
    conexoes.importar_senhas
);
efeito_bool!(
    efeito_conexoes__confirmar_fechar_todas,
    "conexoes.confirmar_fechar_todas",
    conexoes.confirmar_fechar_todas
);
efeito_bool!(
    efeito_terminal__cursor_piscando,
    "terminal.cursor_piscando",
    terminal.cursor_piscando
);
efeito_bool!(
    efeito_terminal__fechar_painel_vazio,
    "terminal.fechar_painel_vazio",
    terminal.fechar_painel_vazio
);
efeito_bool!(
    efeito_terminal__avisar_colar,
    "terminal.avisar_colar",
    terminal.avisar_colar
);
efeito_bool!(
    efeito_terminal__ctrl_v_arquivos,
    "terminal.ctrl_v_arquivos",
    terminal.ctrl_v_arquivos
);
efeito_bool!(
    efeito_terminal__duplicar_na_pasta,
    "terminal.duplicar_na_pasta",
    terminal.duplicar_na_pasta
);
efeito_texto!(
    efeito_terminal__pasta_colar,
    "terminal.pasta_colar",
    "~/colados",
    terminal.pasta_colar
);
efeito_numero!(
    efeito_terminal__limpar_colados_dias,
    "terminal.limpar_colados_dias",
    30.0,
    terminal.limpar_colados_dias
);
efeito_numero!(
    efeito_terminal__colar_tar_min_arquivos,
    "terminal.colar_tar_min_arquivos",
    200.0,
    terminal.colar_tar_min_arquivos
);
efeito_escolha!(
    efeito_terminal__colar_arquivos,
    "terminal.colar_arquivos",
    ColarArquivos::TODAS,
    terminal.colar_arquivos
);
efeito_bool!(
    efeito_terminal__logs__todas_as_sessoes,
    "terminal.logs.todas_as_sessoes",
    terminal.logs.todas_as_sessoes
);
efeito_bool!(
    efeito_terminal__logs__cifrar,
    "terminal.logs.cifrar",
    terminal.logs.cifrar
);
efeito_numero!(
    efeito_terminal__logs__selar_secs,
    "terminal.logs.selar_secs",
    30.0,
    terminal.logs.selar_secs
);
efeito_numero!(
    efeito_terminal__logs__visualizar_mib,
    "terminal.logs.visualizar_mib",
    64.0,
    terminal.logs.visualizar_mib
);
efeito_bool!(
    efeito_cofre__gerador__maiusculas,
    "cofre.gerador.maiusculas",
    cofre.gerador.maiusculas
);
efeito_bool!(
    efeito_cofre__gerador__minusculas,
    "cofre.gerador.minusculas",
    cofre.gerador.minusculas
);
efeito_bool!(
    efeito_cofre__gerador__digitos,
    "cofre.gerador.digitos",
    cofre.gerador.digitos
);
efeito_bool!(
    efeito_cofre__gerador__simbolos,
    "cofre.gerador.simbolos",
    cofre.gerador.simbolos
);
efeito_bool!(
    efeito_cofre__gerador__sem_ambiguos,
    "cofre.gerador.sem_ambiguos",
    cofre.gerador.sem_ambiguos
);
efeito_bool!(efeito_agente__ligado, "agente.ligado", agente.ligado);
efeito_bool!(
    efeito_agente__sistema_ssh_config,
    "agente.sistema_ssh_config",
    agente.sistema_ssh_config
);
efeito_bool!(
    efeito_agente__sistema_shells,
    "agente.sistema_shells",
    agente.sistema_shells
);
efeito_bool!(
    efeito_agente__sistema_sessao,
    "agente.sistema_sessao",
    agente.sistema_sessao
);
efeito_bool!(
    efeito_agente__tirar_ao_bloquear,
    "agente.tirar_ao_bloquear",
    agente.tirar_ao_bloquear
);
// Só existem no Windows: nos outros sistemas o catálogo os esconde.
#[cfg(windows)]
efeito_bool!(
    efeito_agente__sistema_git,
    "agente.sistema_git",
    agente.sistema_git
);
#[cfg(windows)]
efeito_bool!(efeito_agente__pageant, "agente.pageant", agente.pageant);
efeito_bool!(
    efeito_agente__exportar_no_shell,
    "agente.exportar_no_shell",
    agente.exportar_no_shell
);
efeito_bool!(
    efeito_agente__carregar_ao_iniciar,
    "agente.carregar_ao_iniciar",
    agente.carregar_ao_iniciar
);
efeito_bool!(
    efeito_agente__confirmar_padrao,
    "agente.confirmar_padrao",
    agente.confirmar_padrao
);
efeito_bool!(
    efeito_agente__encaminhar_padrao,
    "agente.encaminhar_padrao",
    agente.encaminhar_padrao
);
efeito_bool!(
    efeito_arquivos__mostrar_ocultos,
    "arquivos.mostrar_ocultos",
    arquivos.mostrar_ocultos
);
efeito_bool!(
    efeito_arquivos__preservar_data,
    "arquivos.preservar_data",
    arquivos.preservar_data
);
efeito_bool!(
    efeito_arquivos__preservar_permissoes,
    "arquivos.preservar_permissoes",
    arquivos.preservar_permissoes
);
efeito_bool!(
    efeito_arquivos__comparar,
    "arquivos.comparar",
    arquivos.comparar
);
efeito_bool!(
    efeito_arquivos__sincronizar,
    "arquivos.sincronizar",
    arquivos.sincronizar
);
efeito_bool!(
    efeito_arquivos__ocultar_identicos,
    "arquivos.ocultar_identicos",
    arquivos.ocultar_identicos
);
efeito_bool!(
    efeito_arquivos__confirmar_apagar,
    "arquivos.confirmar_apagar",
    arquivos.confirmar_apagar
);
efeito_bool!(
    efeito_redes__descer_ao_fechar,
    "redes.descer_ao_fechar",
    redes.descer_ao_fechar
);
efeito_texto!(
    efeito_redes__navegador,
    "redes.navegador",
    "/usr/bin/firefox",
    redes.navegador
);
efeito_escolha!(
    efeito_x11__padrao,
    "x11.padrao",
    X11Padrao::TODAS,
    x11.padrao
);
efeito_texto!(
    efeito_x11__display,
    "x11.display",
    "localhost:0",
    x11.display
);
efeito_texto!(
    efeito_x11__virt_manager,
    "x11.virt_manager",
    "virt-manager --no-fork --debug",
    x11.virt_manager
);
efeito_bool!(
    efeito_tuneis__outras_maquinas,
    "tuneis.outras_maquinas",
    tuneis.outras_maquinas
);
efeito_bool!(efeito_portas__sondar, "portas.sondar", portas.sondar);
efeito_bool!(
    efeito_portas__usar_ferramentas,
    "portas.usar_ferramentas",
    portas.usar_ferramentas
);
efeito_bool!(
    efeito_portas__avisar_novas,
    "portas.avisar_novas",
    portas.avisar_novas
);
efeito_bool!(
    efeito_portas__links_pelo_tunel,
    "portas.links_pelo_tunel",
    portas.links_pelo_tunel
);
efeito_bool!(
    efeito_portas__mostrar_sistema,
    "portas.mostrar_sistema",
    portas.mostrar_sistema
);
efeito_bool!(
    efeito_janelas__tear_off,
    "janelas.tear_off",
    janelas.tear_off
);

efeito_numero!(
    efeito_geral__recentes_max,
    "geral.recentes_max",
    12.0,
    geral.recentes_max
);
efeito_numero!(
    efeito_terminal__tamanho_fonte,
    "terminal.tamanho_fonte",
    20.0,
    terminal.tamanho_fonte
);
efeito_numero!(
    efeito_terminal__historico,
    "terminal.historico",
    5000.0,
    terminal.historico
);
efeito_numero!(
    efeito_conexoes__porta_padrao,
    "conexoes.porta_padrao",
    2222.0,
    conexoes.porta_padrao
);
efeito_numero!(
    efeito_conexoes__keepalive_padrao_secs,
    "conexoes.keepalive_padrao_secs",
    45.0,
    conexoes.keepalive_padrao_secs
);
efeito_numero!(
    efeito_conexoes__comandos_quieto_ms,
    "conexoes.comandos_quieto_ms",
    900.0,
    conexoes.comandos_quieto_ms
);
efeito_numero!(
    efeito_conexoes__comandos_max_secs,
    "conexoes.comandos_max_secs",
    30.0,
    conexoes.comandos_max_secs
);
efeito_numero!(
    efeito_conexoes__reconectar__tentativas,
    "conexoes.reconectar.tentativas",
    5.0,
    conexoes.reconectar.tentativas
);
efeito_numero!(
    efeito_conexoes__reconectar__espera_secs,
    "conexoes.reconectar.espera_secs",
    10.0,
    conexoes.reconectar.espera_secs
);
efeito_numero!(
    efeito_conexoes__reconectar__retentar_mins,
    "conexoes.reconectar.retentar_mins",
    15.0,
    conexoes.reconectar.retentar_mins
);
efeito_numero!(
    efeito_conexoes__ocioso_mins,
    "conexoes.ocioso_mins",
    10.0,
    conexoes.ocioso_mins
);
efeito_numero!(
    efeito_cofre__limpar_area_transferencia_secs,
    "cofre.limpar_area_transferencia_secs",
    30.0,
    cofre.limpar_area_transferencia_secs
);
efeito_numero!(
    efeito_cofre__bloquear_apos_min,
    "cofre.bloquear_apos_min",
    15.0,
    cofre.bloquear_apos_min
);
efeito_numero!(
    efeito_cofre__gerador__tamanho,
    "cofre.gerador.tamanho",
    40.0,
    cofre.gerador.tamanho
);
efeito_numero!(
    efeito_agente__vida_padrao_secs,
    "agente.vida_padrao_secs",
    3600.0,
    agente.vida_padrao_secs
);
efeito_numero!(
    efeito_agente__rodadas_padrao,
    "agente.rodadas_padrao",
    200.0,
    agente.rodadas_padrao
);
efeito_numero!(
    efeito_arquivos__paralelas,
    "arquivos.paralelas",
    4.0,
    arquivos.paralelas
);
efeito_numero!(
    efeito_arquivos__comparar_tolerancia_min,
    "arquivos.comparar_tolerancia_min",
    5.0,
    arquivos.comparar_tolerancia_min
);
efeito_numero!(
    efeito_arquivos__keepalive_secs,
    "arquivos.keepalive_secs",
    60.0,
    arquivos.keepalive_secs
);
efeito_numero!(efeito_s3__parte_mib, "s3.parte_mib", 64.0, s3.parte_mib);
efeito_numero!(
    efeito_portas__intervalo_secs,
    "portas.intervalo_secs",
    12.0,
    portas.intervalo_secs
);
efeito_numero!(
    efeito_portas__sonda_ms,
    "portas.sonda_ms",
    2500.0,
    portas.sonda_ms
);
efeito_numero!(
    efeito_redes__espera_padrao_secs,
    "redes.espera_padrao_secs",
    90.0,
    redes.espera_padrao_secs
);

efeito_escolha!(
    efeito_notas__modo,
    "notas.modo",
    ModoNotas::TODAS,
    notas.modo
);
efeito_bool!(
    efeito_notas__fonte_mono,
    "notas.fonte_mono",
    notas.fonte_mono
);
efeito_bool!(
    efeito_notas__quebrar_linhas,
    "notas.quebrar_linhas",
    notas.quebrar_linhas
);
efeito_escolha!(efeito_geral__tema, "geral.tema", Tema::TODAS, geral.tema);
efeito_escolha!(
    efeito_geral__idioma,
    "geral.idioma",
    Idioma::TODAS,
    geral.idioma
);
efeito_escolha!(
    efeito_geral__ao_fechar,
    "geral.ao_fechar",
    AoFechar::TODAS,
    geral.ao_fechar
);
efeito_escolha!(
    efeito_geral__pagina_inicial,
    "geral.pagina_inicial",
    PaginaInicial::TODAS,
    geral.pagina_inicial
);
efeito_escolha!(
    efeito_terminal__esquema,
    "terminal.esquema",
    Esquema::TODAS,
    terminal.esquema
);
efeito_escolha!(
    efeito_terminal__cursor,
    "terminal.cursor",
    Cursor::TODAS,
    terminal.cursor
);
efeito_escolha!(
    efeito_terminal__botao_direito,
    "terminal.botao_direito",
    BotaoDireito::TODAS,
    terminal.botao_direito
);
efeito_escolha!(
    efeito_terminal__campainha,
    "terminal.campainha",
    Campainha::TODAS,
    terminal.campainha
);
efeito_escolha!(
    efeito_conexoes__chave_nova,
    "conexoes.chave_nova",
    ChaveNova::TODAS,
    conexoes.chave_nova
);
efeito_escolha!(
    efeito_conexoes__reconectar__modo,
    "conexoes.reconectar.modo",
    ModoReconectar::TODAS,
    conexoes.reconectar.modo
);
efeito_escolha!(
    efeito_conexoes__abrir_em,
    "conexoes.abrir_em",
    AbrirEm::TODAS,
    conexoes.abrir_em
);
efeito_escolha!(
    efeito_conexoes__tmux_ao_fechar,
    "conexoes.tmux_ao_fechar",
    TmuxAoFechar::TODAS,
    conexoes.tmux_ao_fechar
);
efeito_escolha!(
    efeito_agente__tipo_padrao,
    "agente.tipo_padrao",
    TipoChave::TODAS,
    agente.tipo_padrao
);
efeito_escolha!(
    efeito_arquivos__conflito,
    "arquivos.conflito",
    ConflitoCfg::TODAS,
    arquivos.conflito
);
efeito_escolha!(
    efeito_arquivos__comparar_modo,
    "arquivos.comparar_modo",
    ModoComparar::TODAS,
    arquivos.comparar_modo
);
efeito_escolha!(
    efeito_arquivos__enviar_editado,
    "arquivos.enviar_editado",
    EnviarEditado::TODAS,
    arquivos.enviar_editado
);
efeito_escolha!(
    efeito_ftp__seguranca,
    "ftp.seguranca",
    FtpSeguranca::TODAS,
    ftp.seguranca
);
efeito_escolha!(efeito_ftp__modo, "ftp.modo", FtpModo::TODOS, ftp.modo);
efeito_escolha!(efeito_ftp__tls, "ftp.tls", FtpTls::TODAS, ftp.tls);
efeito_escolha!(
    efeito_ftp__codificacao,
    "ftp.codificacao",
    FtpCodificacao::TODAS,
    ftp.codificacao
);
efeito_escolha!(efeito_s3__estilo, "s3.estilo", S3Estilo::TODOS, s3.estilo);
efeito_escolha!(
    efeito_s3__validade_link,
    "s3.validade_link",
    ValidadeLink::TODAS,
    s3.validade_link
);
efeito_escolha!(
    efeito_janelas__abrir_espaco,
    "janelas.abrir_espaco",
    AbrirEspaco::TODAS,
    janelas.abrir_espaco
);

efeito_texto!(
    efeito_terminal__shell__programa,
    "terminal.shell.programa",
    "/bin/zsh",
    terminal.shell.programa
);
efeito_texto!(
    efeito_terminal__shell__argumentos,
    "terminal.shell.argumentos",
    "-l -i",
    terminal.shell.argumentos
);
efeito_texto!(
    efeito_terminal__shell__pasta,
    "terminal.shell.pasta",
    "/tmp/shell-teste",
    terminal.shell.pasta
);
efeito_texto!(
    efeito_terminal__logs__pasta,
    "terminal.logs.pasta",
    "/tmp/logs-teste",
    terminal.logs.pasta
);
efeito_texto!(
    efeito_conexoes__usuario_padrao,
    "conexoes.usuario_padrao",
    "deploy",
    conexoes.usuario_padrao
);
efeito_texto!(
    efeito_conexoes__term_padrao,
    "conexoes.term_padrao",
    "tmux-256color",
    conexoes.term_padrao
);
efeito_texto!(
    efeito_conexoes__known_hosts,
    "conexoes.known_hosts",
    "/tmp/known-hosts-teste",
    conexoes.known_hosts
);
efeito_texto!(
    efeito_scripts__interpretador,
    "scripts.interpretador",
    "bash -s",
    scripts.interpretador
);
efeito_numero!(
    efeito_scripts__paralelo,
    "scripts.paralelo",
    16.0,
    scripts.paralelo
);
efeito_texto!(
    efeito_scripts__pasta,
    "scripts.pasta",
    "/tmp/scripts-teste",
    scripts.pasta
);
efeito_texto!(
    efeito_agente__socket,
    "agente.socket",
    "/tmp/agente-teste.sock",
    agente.socket
);
efeito_texto!(
    efeito_agente__pasta_chaves,
    "agente.pasta_chaves",
    "/tmp/chaves-teste",
    agente.pasta_chaves
);
efeito_texto!(
    efeito_arquivos__editor,
    "arquivos.editor",
    "/usr/bin/nano",
    arquivos.editor
);
efeito_texto!(
    efeito_arquivos__pasta_local,
    "arquivos.pasta_local",
    "/tmp/local-teste",
    arquivos.pasta_local
);
efeito_texto!(efeito_s3__regiao, "s3.regiao", "sa-east-1", s3.regiao);
efeito_texto!(efeito_s3__classe, "s3.classe", "STANDARD_IA", s3.classe);
efeito_texto!(
    efeito_rclone__binario,
    "rclone.binario",
    "/usr/bin/rclone",
    rclone.binario
);
efeito_texto!(
    efeito_rclone__arquivo_config,
    "rclone.arquivo_config",
    "/tmp/rclone-teste.conf",
    rclone.arquivo_config
);
efeito_texto!(
    efeito_janelas__pasta_espacos,
    "janelas.pasta_espacos",
    "/tmp/espacos-teste",
    janelas.pasta_espacos
);

/// A senha do rclone é um item do cofre: o arquivo guarda só o id dele.
#[test]
fn efeito_rclone__credencial() {
    let pasta = Pasta::nova("config-pagina-efeito-rclone-credencial");
    let ids = pasta.semear_cofre(vec![item_ssh("Config do rclone", "root", "segredo-123")]);
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    let id = id_do_valor("rclone.credencial");
    alcancar(&mut m.app, &id);
    digitar(&mut m.app, &id, "Config do rclone");
    m.app.key(tomni_ui::Key::Enter);
    assert_eq!(gravada(&pasta).rclone.credencial, Some(ids[0]));
    assert!(
        !arquivo(&pasta).contains("segredo-123"),
        "{}",
        arquivo(&pasta)
    );

    m.app
        .click(&id)
        .shortcut('a')
        .key(tomni_ui::Key::Backspace)
        .key(tomni_ui::Key::Enter);
    assert_eq!(gravada(&pasta).rclone.credencial, None);
}

#[test]
fn todas_as_configuracoes_tem_teste_de_efeito() {
    let fonte = include_str!("configuracoes.rs");
    let mut faltam = Vec::new();
    for d in catalogo::CATALOGO {
        let esperado = match d.tipo {
            Tipo::Grupo => continue,
            Tipo::Valor => format!("efeito_{}", d.chave.replace('.', "__")),
            Tipo::Acao => format!("acao_{}", d.chave.trim_start_matches("acao.")),
        };
        // Os macros `efeito_*!` entram numa linha só (`(nome,`) ou com o nome numa linha própria.
        let teste = fonte.contains(&format!("fn {esperado}()"))
            || fonte.contains(&format!("({esperado},"))
            || fonte.lines().any(|l| l.trim() == format!("{esperado},"));
        if !teste {
            faltam.push(esperado);
        }
    }
    assert!(faltam.is_empty(), "sem teste: {faltam:?}");
}

// --- Os cards de ação ---

// Os assistentes de IA (M52).

efeito_bool!(efeito_ia__redigir, "ia.redigir", ia.redigir);
efeito_bool!(
    efeito_ia__mostrar_no_menu,
    "ia.mostrar_no_menu",
    ia.mostrar_no_menu
);
efeito_bool!(
    efeito_ia__mostrar_o_que_sai,
    "ia.mostrar_o_que_sai",
    ia.mostrar_o_que_sai
);
efeito_numero!(
    efeito_ia__linhas_contexto,
    "ia.linhas_contexto",
    500.0,
    ia.linhas_contexto
);
efeito_numero!(
    efeito_ia__max_caracteres,
    "ia.max_caracteres",
    50_000.0,
    ia.max_caracteres
);
efeito_numero!(
    efeito_ia__espera_resposta,
    "ia.espera_resposta",
    90.0,
    ia.espera_resposta
);
efeito_texto!(
    efeito_ia__comando_shell,
    "ia.comando_shell",
    "zsh",
    ia.comando_shell
);

#[test]
fn efeito_ia__redigir_extras() {
    let (pasta, mut m) = abrir("efeito-ia-redigir-extras");
    let id = id_do_valor("ia.redigir_extras");
    alcancar(&mut m.app, &id);
    assert!(!aparece(&mut m.app, "config-ia-redigir_extras-erro"));
    digitar(&mut m.app, &id, "segredo-da-casa\nre:token-[0-9]+");
    assert_eq!(
        gravada(&pasta).ia.redigir_extras,
        ["segredo-da-casa", "re:token-[0-9]+"],
        "{}",
        arquivo(&pasta)
    );
    assert!(!aparece(&mut m.app, "config-ia-redigir_extras-erro"));
}

#[test]
fn efeito_ia__redigir_extras_com_regex_invalida_avisa() {
    let (pasta, mut m) = abrir("efeito-ia-redigir-extras-invalida");
    let id = id_do_valor("ia.redigir_extras");
    alcancar(&mut m.app, &id);
    digitar(&mut m.app, &id, "re:(");
    assert!(aparece(&mut m.app, "config-ia-redigir_extras-erro"));
    // O texto fica gravado; quem avisa é a tela, e o redator ignora o que não compila.
    assert_eq!(gravada(&pasta).ia.redigir_extras, ["re:("]);
}

// O controle por assistentes (M51).

#[test]
fn efeito_ia__mcp_ligado() {
    let (pasta, mut m) = abrir("efeito-ia-mcp-ligado");
    let id = id_do_valor("ia.mcp_ligado");
    alcancar(&mut m.app, &id);
    // A primeira ativação mostra o aviso e ainda não liga.
    m.app.click(&id);
    assert!(aparece(&mut m.app, "mcp-aviso"));
    assert!(!m.app.checked(&id));
    assert!(!gravada(&pasta).ia.mcp_ligado);
    // "Ligar" no aviso liga e lembra que a pessoa viu.
    m.app.click("mcp-aviso-primary");
    assert!(m.app.checked(&id));
    let c = gravada(&pasta);
    assert!(c.ia.mcp_ligado && c.ia.mcp_avisado, "{}", arquivo(&pasta));
    // Desligar e ligar de novo não repete o aviso.
    m.app.click(&id);
    assert!(!gravada(&pasta).ia.mcp_ligado);
    m.app.click(&id);
    assert!(m.app.checked(&id));
    assert!(gravada(&pasta).ia.mcp_ligado);
    assert!(!aparece(&mut m.app, "mcp-aviso"));
}

efeito_escolha!(
    efeito_ia__mcp_abrir,
    "ia.mcp_abrir",
    AbrirAbaIa::TODAS,
    ia.mcp_abrir
);
efeito_escolha!(
    efeito_ia__mcp_abrir_auto,
    "ia.mcp_abrir_auto",
    AbrirAbaAuto::TODAS,
    ia.mcp_abrir_auto
);

#[test]
fn efeito_ia__mcp_ligado_cancelando_o_aviso_nao_liga() {
    let (pasta, mut m) = abrir("efeito-ia-mcp-ligado-cancela");
    let id = id_do_valor("ia.mcp_ligado");
    alcancar(&mut m.app, &id);
    m.app.click(&id);
    m.app.click("mcp-aviso-close");
    assert!(!m.app.checked(&id));
    let c = gravada(&pasta);
    assert!(!c.ia.mcp_ligado && !c.ia.mcp_avisado);
}

#[test]
fn a_linha_de_estado_do_controle_acompanha_o_interruptor() {
    let (_pasta, mut m) = abrir("mcp-estado");
    assert!(aparece(&mut m.app, "mcp-estado"));
    assert_eq!(m.app.text("mcp-estado"), "Desligado.");
    let id = id_do_valor("ia.mcp_ligado");
    m.app.click(&id);
    m.app.click("mcp-aviso-primary");
    assert_eq!(m.app.text("mcp-estado"), "Esperando assistentes.");
}

efeito_bool!(efeito_ia__auditoria, "ia.auditoria", ia.auditoria);
efeito_bool!(
    efeito_ia__comandos_na_aba,
    "ia.comandos_na_aba",
    ia.comandos_na_aba
);
efeito_numero!(efeito_ia__marca_secs, "ia.marca_secs", 45.0, ia.marca_secs);
efeito_numero!(
    efeito_ia__linhas_por_leitura,
    "ia.linhas_por_leitura",
    500.0,
    ia.linhas_por_leitura
);
efeito_numero!(
    efeito_ia__caracteres_por_resposta,
    "ia.caracteres_por_resposta",
    20_000.0,
    ia.caracteres_por_resposta
);
efeito_texto!(
    efeito_ia__pasta_troca,
    "ia.pasta_troca",
    "/tmp/troca",
    ia.pasta_troca
);

// Os assistentes de IA no servidor (M46).

efeito_escolha!(
    efeito_ia__preferido,
    "ia.preferido",
    AssistentePreferido::TODAS,
    ia.preferido
);
efeito_escolha!(
    efeito_ia__abrir_em,
    "ia.abrir_em",
    AbrirIa::TODAS,
    ia.abrir_em
);

// O diário das conversas dos assistentes deste computador (M63).

efeito_bool!(
    efeito_ia__conversas_lembrar,
    "ia.conversas_lembrar",
    ia.conversas_lembrar
);
efeito_bool!(
    efeito_ia__conversas_reabrir,
    "ia.conversas_reabrir",
    ia.conversas_reabrir
);
efeito_escolha!(
    efeito_ia__conversas_ao_controlar,
    "ia.conversas_ao_controlar",
    ConversaAoControlar::TODAS,
    ia.conversas_ao_controlar
);
efeito_bool!(
    efeito_ia__shell_ao_sair,
    "ia.shell_ao_sair",
    ia.shell_ao_sair
);
efeito_bool!(efeito_ia__tunel_codex, "ia.tunel_codex", ia.tunel_codex);
efeito_bool!(
    efeito_ia__verificar_ao_abrir,
    "ia.verificar_ao_abrir",
    ia.verificar_ao_abrir
);
efeito_bool!(
    efeito_ia__mostrar_botao,
    "ia.mostrar_botao",
    ia.mostrar_botao
);
efeito_bool!(
    efeito_ia__mostrar_claude,
    "ia.mostrar_claude",
    ia.mostrar_claude
);
efeito_bool!(
    efeito_ia__mostrar_codex,
    "ia.mostrar_codex",
    ia.mostrar_codex
);
efeito_bool!(
    efeito_ia__mostrar_opencode,
    "ia.mostrar_opencode",
    ia.mostrar_opencode
);
efeito_escolha!(
    efeito_ia__metodo_claude,
    "ia.metodo_claude",
    MetodoIa::TODAS,
    ia.metodo_claude
);
efeito_escolha!(
    efeito_ia__metodo_codex,
    "ia.metodo_codex",
    MetodoIa::TODAS,
    ia.metodo_codex
);
efeito_escolha!(
    efeito_ia__metodo_opencode,
    "ia.metodo_opencode",
    MetodoIa::TODAS,
    ia.metodo_opencode
);
efeito_texto!(
    efeito_ia__argumentos_claude,
    "ia.argumentos_claude",
    "--model sonnet",
    ia.argumentos_claude
);
efeito_texto!(
    efeito_ia__argumentos_codex,
    "ia.argumentos_codex",
    "--model o3",
    ia.argumentos_codex
);
efeito_texto!(
    efeito_ia__argumentos_opencode,
    "ia.argumentos_opencode",
    "--continue",
    ia.argumentos_opencode
);

// Os assistentes deste computador (M63).
efeito_texto!(
    efeito_ia__local_pasta,
    "ia.local_pasta",
    "/srv/assistentes",
    ia.local_pasta
);
efeito_texto!(
    efeito_ia__local_claude_caminho,
    "ia.local_claude_caminho",
    "/opt/claude/bin/claude",
    ia.local_claude_caminho
);
efeito_texto!(
    efeito_ia__local_claude_argumentos,
    "ia.local_claude_argumentos",
    "--model sonnet",
    ia.local_claude_argumentos
);
efeito_texto!(
    efeito_ia__local_codex_caminho,
    "ia.local_codex_caminho",
    "/opt/codex/bin/codex",
    ia.local_codex_caminho
);
efeito_texto!(
    efeito_ia__local_codex_argumentos,
    "ia.local_codex_argumentos",
    "--model o3",
    ia.local_codex_argumentos
);
efeito_texto!(
    efeito_ia__local_opencode_caminho,
    "ia.local_opencode_caminho",
    "/opt/opencode/bin/opencode",
    ia.local_opencode_caminho
);
efeito_texto!(
    efeito_ia__local_opencode_argumentos,
    "ia.local_opencode_argumentos",
    "--continue",
    ia.local_opencode_argumentos
);

#[test]
fn efeito_ia__bloqueio_extras() {
    let (pasta, mut m) = abrir("efeito-ia-bloqueio-extras");
    let id = id_do_valor("ia.bloqueio_extras");
    alcancar(&mut m.app, &id);
    digitar(&mut m.app, &id, "docker\nre:^kubectl\\s+delete");
    assert_eq!(
        gravada(&pasta).ia.bloqueio_extras,
        ["docker", "re:^kubectl\\s+delete"],
        "{}",
        arquivo(&pasta)
    );
}

#[test]
fn efeito_ia__finais_de_prompt() {
    let (pasta, mut m) = abrir("efeito-ia-finais-de-prompt");
    let id = id_do_valor("ia.finais_de_prompt");
    alcancar(&mut m.app, &id);
    digitar(&mut m.app, &id, "❯\n»");
    assert_eq!(
        gravada(&pasta).ia.finais_de_prompt,
        ["❯", "»"],
        "{}",
        arquivo(&pasta)
    );
}

#[test]
fn restaurar_a_secao_do_mcp_mantem_os_espacos_e_a_ia() {
    let (pasta, mut m) = abrir("restaurar-mcp");
    let id = id_do_valor("ia.marca_secs");
    alcancar(&mut m.app, &id);
    m.app.set_value(&id, 5.0);
    assert_eq!(gravada(&pasta).ia.marca_secs, 5);
    let redigir = id_do_valor("ia.redigir");
    alcancar(&mut m.app, &redigir);
    m.app.click(&redigir);
    assert!(!gravada(&pasta).ia.redigir);
    m.app.click("config-restaurar-mcp");
    let c = gravada(&pasta);
    assert_eq!(c.ia.marca_secs, 30, "{}", arquivo(&pasta));
    assert!(!c.ia.redigir, "a seção da IA não volta junto");
}

#[test]
fn acao_mcp_como() {
    let (_pasta, mut m) = abrir("acao-mcp-como");
    m.app.click(&id_do_card("acao.mcp_como"));
    assert!(aparece(&mut m.app, "mcp-como-dialogo"));
}

#[test]
fn acao_mcp_espaco_novo() {
    let (_pasta, mut m) = abrir("acao-mcp-espaco-novo");
    m.app.click(&id_do_card("acao.mcp_espaco_novo"));
    assert!(aparece(&mut m.app, "mcp-espaco-dialogo"));
}

#[test]
fn acao_mcp_registro() {
    let (_pasta, mut m) = abrir("acao-mcp-registro");
    // Sem registro de hoje, o app diz isso em vez de abrir um editor.
    m.app.click(&id_do_card("acao.mcp_registro"));
    assert!(aparece(&mut m.app, "config-mensagem"));
    assert!(!aparece(&mut m.app, "config-erro"));
}

#[test]
fn acao_mcp_pasta_registro() {
    let (_pasta, mut m) = abrir("acao-mcp-pasta-registro");
    // Abrir o gerenciador de arquivos do sistema não roda no teste: confere só o card.
    assert!(aparece(&mut m.app, &id_do_card("acao.mcp_pasta_registro")));
}

#[test]
fn acao_ia_local_abrir_pasta() {
    let (_pasta, mut m) = abrir("acao-ia-local-abrir-pasta");
    // Abrir o gerenciador de arquivos do sistema não roda no teste: confere só o card.
    assert!(aparece(
        &mut m.app,
        &id_do_card("acao.ia_local_abrir_pasta")
    ));
}

#[test]
fn acao_ia_local_limpar() {
    let (pasta, mut m) = abrir("acao-ia-local-limpar");
    // Vazia, a pasta de trabalho padrão fica na pasta de cache da pasta pessoal do teste.
    m.app.click(&id_do_card("acao.ia_local_limpar"));
    assert!(aparece(&mut m.app, "config-mensagem"));
    assert!(!aparece(&mut m.app, "config-limpar-pasta"));

    let trabalho = pasta.0.join(".cache/tirech-omni/assistentes");
    std::fs::create_dir_all(trabalho.join("projeto/src")).unwrap();
    std::fs::write(trabalho.join("projeto/src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(trabalho.join("rascunho.txt"), "x").unwrap();
    m.app.click(&id_do_card("acao.ia_local_limpar"));
    assert!(aparece(&mut m.app, "config-limpar-pasta"));
    let texto = m.app.text("config-limpar-pasta-texto");
    assert!(texto.contains("2 itens"), "{texto}");
    assert!(texto.contains(&*trabalho.to_string_lossy()), "{texto}");
    m.app.click("config-limpar-pasta-primary");
    assert!(trabalho.is_dir(), "a pasta fica, vazia");
    assert_eq!(std::fs::read_dir(&trabalho).unwrap().count(), 0);
    assert!(m.app.text("config-mensagem").contains("2 itens"));
}

#[test]
fn limpar_a_pasta_pessoal_e_recusado() {
    let pasta = Pasta::nova("config-pagina-limpar-pessoal");
    pasta.configurar("ia", "local_pasta", serde_json::json!("~"));
    std::fs::create_dir_all(pasta.0.join("documentos")).unwrap();
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    expandir_tudo(&mut m.app);
    m.app.click(&id_do_card("acao.ia_local_limpar"));
    assert!(aparece(&mut m.app, "config-erro"));
    assert!(!aparece(&mut m.app, "config-limpar-pasta"));
    assert!(pasta.0.join("documentos").is_dir());
}

#[test]
fn acao_limpar_recentes() {
    let (_pasta, mut m) = abrir("acao-limpar");
    m.app.click(&id_do_card("acao.limpar_recentes"));
    assert!(!aparece(&mut m.app, "config-erro"));
}

#[test]
fn acao_pasta_dados() {
    let (_pasta, mut m) = abrir("acao-pasta");
    let id = id_do_card("acao.pasta_dados");
    assert!(aparece(&mut m.app, &id));
    // Nada é mudado sem escolher uma pasta: o seletor cancelado deixa tudo como está.
    m.app.click(&format!("{id}-cabecalho"));
    assert!(aparece(&mut m.app, "config-pasta-mudar"));
    assert!(aparece(&mut m.app, "config-pasta-abrir"));
    assert!(!aparece(&mut m.app, "config-pasta-reiniciar"));
}

#[test]
fn acao_sobre() {
    let (_pasta, mut m) = abrir("acao-sobre");
    m.app.click(&id_do_card("acao.sobre"));
    assert!(m.app.checked("navegacao-sobre"));
}

#[test]
fn acao_instalacao() {
    let (_pasta, mut m) = abrir("acao-instalacao");
    let id = id_do_card("acao.instalacao");
    assert!(aparece(&mut m.app, &id));
    assert!(!m.app.is_open("instalar-dialogo"));
    m.app.click(&id);
    assert!(m.app.is_open("instalar-dialogo"));
}

#[test]
fn acao_importar_ssh_config() {
    let (_pasta, mut m) = abrir("acao-ssh-config");
    m.app.click(&id_do_card("acao.importar_ssh_config"));
    assert_eq!(m.app.file_dialogs().len(), 1);
    m.app.file_dialog_cancel();
}

#[test]
fn acao_importar_aws() {
    let (_pasta, mut m) = abrir("acao-aws");
    m.app.click(&id_do_card("acao.importar_aws"));
    assert_eq!(m.app.file_dialogs().len(), 1);
    m.app.file_dialog_cancel();
}

#[test]
fn acao_protecao_cofre() {
    let (_pasta, mut m) = abrir("acao-protecao");
    m.app.click(&id_do_card("acao.protecao_cofre"));
    assert!(m.app.checked("navegacao-cofre"));
}

#[test]
fn acao_cofre_local() {
    let (_pasta, mut m) = abrir("acao-cofre-local");
    let id = id_do_card("acao.cofre_local");
    assert!(aparece(&mut m.app, &id));
    m.app.click(&format!("{id}-cabecalho"));
    assert!(aparece(&mut m.app, "config-cofre-local-mudar"));
    assert!(aparece(&mut m.app, "config-cofre-local-abrir"));
    // Na pasta de dados, não há para onde voltar.
    assert!(!m.app.enabled("config-cofre-local-padrao"));
    // Sem escolher uma pasta, nada muda.
    m.app.click("config-cofre-local-mudar");
    assert_eq!(m.app.file_dialogs().len(), 1);
    m.app.file_dialog_cancel();
    assert!(!m.app.is_open("cofre-local-dialogo"));
}

#[test]
fn acao_cofre_levar() {
    let (_pasta, mut m) = abrir("acao-cofre-levar");
    m.app.click(&id_do_card("acao.cofre_levar"));
    assert!(m.app.is_open("cofre-levar-dialogo"));
    m.app.click("cofre-levar-dialogo-close");
    assert!(!m.app.is_open("cofre-levar-dialogo"));
    assert!(m.app.file_dialogs().is_empty());
}

#[test]
fn acao_cofre_abrir_outro() {
    let (_pasta, mut m) = abrir("acao-cofre-abrir");
    m.app.click(&id_do_card("acao.cofre_abrir_outro"));
    assert_eq!(m.app.file_dialogs().len(), 1);
    m.app.file_dialog_cancel();
    assert!(!m.app.is_open("cofre-abrir-dialogo"));
}

#[test]
fn acao_csv_cofre() {
    let (_pasta, mut m) = abrir("acao-csv");
    m.app.click(&id_do_card("acao.csv_cofre"));
    assert!(m.app.checked("navegacao-cofre"));
}

#[test]
fn acao_gerenciar_chaves() {
    let (_pasta, mut m) = abrir("acao-chaves");
    m.app.click(&id_do_card("acao.gerenciar_chaves"));
    assert!(m.app.checked("navegacao-chaves"));
}

#[test]
fn acao_testar_rclone() {
    let (_pasta, mut m) = abrir("acao-rclone");
    m.app.click(&id_do_card("acao.testar_rclone"));
    // Ou acha o rclone e mostra a versão, ou diz que não achou.
    let mensagem = aparece(&mut m.app, "config-mensagem");
    let erro = aparece(&mut m.app, "config-erro");
    assert!(mensagem ^ erro, "mensagem: {mensagem}, erro: {erro}");
}

#[test]
fn acao_perfis_de_rede() {
    let (_pasta, mut m) = abrir("acao-redes");
    m.app.click(&id_do_card("acao.perfis_de_rede"));
    assert!(m.app.checked("navegacao-redes"));
}

#[test]
fn acao_perfis_de_ia() {
    let (_pasta, mut m) = abrir("acao-ia");
    m.app.click(&id_do_card("acao.perfis_de_ia"));
    assert!(m.app.checked("navegacao-ia"));
}

#[test]
fn acao_exportar() {
    let pasta = Pasta::nova("config-pagina-acao-exportar");
    let ids = pasta.semear_cofre(vec![item_ssh("Senha do rclone", "root", "segredo-123")]);
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    std::fs::write(
        pasta.armazenamento().arquivo_configuracoes(),
        format!(
            r#"{{"geral": {{"pagina_inicial": "inicio"}}, "terminal": {{"tamanho_fonte": 24}},
                "rclone": {{"credencial": {}}}, "agente": {{"ligado": false, "socket": "/tmp/s.sock"}}}}"#,
            ids[0]
        ),
    )
    .unwrap();
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    let destino = pasta.caminho().join("exportado.json");
    m.app.click(&id_do_card("acao.exportar"));
    assert_eq!(m.app.file_dialogs().len(), 1);
    m.app.file_dialog_answer(Some(destino.clone()));

    let texto = std::fs::read_to_string(&destino).unwrap();
    let json: serde_json::Value = serde_json::from_str(&texto).unwrap();
    assert_eq!(json["terminal"]["tamanho_fonte"], 24);
    assert!(json["rclone"].get("credencial").is_none(), "{texto}");
    assert!(json["agente"].get("chaves").is_none(), "{texto}");
    assert!(json["agente"].get("socket").is_none(), "{texto}");
    assert!(!texto.contains("segredo-123"), "{texto}");
    assert!(aparece(&mut m.app, "config-mensagem"));
}

#[test]
fn acao_importar() {
    let (pasta, mut m) = abrir("acao-importar");
    let origem = pasta.caminho().join("trazido.json");
    std::fs::write(
        &origem,
        r#"{"terminal": {"tamanho_fonte": 30}, "geral": {"recentes_max": 9}}"#,
    )
    .unwrap();
    m.app.click(&id_do_card("acao.importar"));
    m.app.file_dialog_answer(Some(origem));
    let fonte = id_do_valor("terminal.tamanho_fonte");
    alcancar(&mut m.app, &fonte);
    assert_eq!(m.app.value(&fonte), 30.0);
    assert_eq!(gravada(&pasta).terminal.tamanho_fonte, 30);
    assert_eq!(gravada(&pasta).geral.recentes_max, 9);

    // O "Desfazer" volta ao que era.
    m.app.click("config-desfazer");
    assert_eq!(gravada(&pasta).terminal.tamanho_fonte, 16);
    assert_eq!(m.app.value(&fonte), 16.0);
}

#[test]
fn importar_um_arquivo_ruim_mostra_o_erro_e_nao_muda_nada() {
    let (pasta, mut m) = abrir("importar-ruim");
    let origem = pasta.caminho().join("ruim.json");
    std::fs::write(&origem, "isto não é json").unwrap();
    m.app.click(&id_do_card("acao.importar"));
    m.app.file_dialog_answer(Some(origem));
    assert!(aparece(&mut m.app, "config-erro"));
    assert!(m.app.text("config-erro").contains("não é um JSON válido"));
    let padrao = Configuracoes::default();
    assert_eq!(gravada(&pasta).terminal, padrao.terminal);
    assert_eq!(
        gravada(&pasta).geral.recentes_max,
        padrao.geral.recentes_max
    );
}

#[test]
fn acao_restaurar_tudo() {
    let (pasta, mut m) = abrir("acao-restaurar-tudo");
    let fonte = id_do_valor("terminal.tamanho_fonte");
    alcancar(&mut m.app, &fonte);
    m.app.set_value(&fonte, 30.0);
    assert_eq!(gravada(&pasta).terminal.tamanho_fonte, 30);

    m.app.click(&id_do_card("acao.restaurar_tudo"));
    assert_eq!(m.app.value(&fonte), 16.0);
    assert_eq!(gravada(&pasta).terminal.tamanho_fonte, 16);

    m.app.click("config-desfazer");
    assert_eq!(m.app.value(&fonte), 30.0);
    assert_eq!(gravada(&pasta).terminal.tamanho_fonte, 30);
}

#[test]
fn restaurar_uma_secao_so_mexe_nela() {
    let (pasta, mut m) = abrir("restaurar-secao");
    let fonte = id_do_valor("terminal.tamanho_fonte");
    let recentes = id_do_valor("geral.recentes_max");
    alcancar(&mut m.app, &fonte);
    m.app.set_value(&fonte, 30.0);
    m.app.set_value(&recentes, 12.0);
    m.app.click("config-restaurar-terminal");
    assert_eq!(gravada(&pasta).terminal.tamanho_fonte, 16);
    assert_eq!(gravada(&pasta).geral.recentes_max, 12);
    assert!(aparece(&mut m.app, "config-desfazer"));
}

#[test]
fn a_secao_scripts_tem_os_padroes_e_o_restaurar_volta_a_eles() {
    let (pasta, mut m) = abrir("scripts-padroes");
    let interpretador = id_do_valor("scripts.interpretador");
    let paralelo = id_do_valor("scripts.paralelo");
    alcancar(&mut m.app, &paralelo);
    assert_eq!(m.app.value(&paralelo), 8.0);
    assert_eq!(m.app.text(&interpretador), "sh -s");
    m.app.set_value(&paralelo, 20.0);
    digitar(&mut m.app, &interpretador, "bash -s");
    m.app.key(tomni_ui::Key::Enter);
    assert_eq!(gravada(&pasta).scripts.paralelo, 20);
    assert_eq!(gravada(&pasta).scripts.interpretador, "bash -s");
    m.app.click("config-restaurar-scripts");
    assert_eq!(gravada(&pasta).scripts.paralelo, 8);
    assert_eq!(gravada(&pasta).scripts.interpretador, "sh -s");
    assert!(aparece(&mut m.app, "config-desfazer"));
}

#[test]
fn a_busca_acha_os_scripts() {
    let (_pasta, mut m) = abrir("busca-scripts");
    m.app.type_into("config-busca", "interpretador");
    assert!(aparece(&mut m.app, &id_do_card("scripts.interpretador")));
    assert!(aparece(&mut m.app, "config-secao-scripts"));
    assert!(!aparece(&mut m.app, &id_do_card("geral.tema")));
}

#[test]
fn mexer_nas_configuracoes_nao_abre_dialogo() {
    let (_pasta, mut m) = abrir("sem-dialogo");
    let fonte = id_do_valor("terminal.tamanho_fonte");
    alcancar(&mut m.app, &fonte);
    m.app.set_value(&fonte, 22.0);
    m.app.click("config-restaurar-terminal");
    assert!(m.app.file_dialogs().is_empty());
}

// --- A busca ---

#[test]
fn a_busca_deixa_so_o_que_casa() {
    let (_pasta, mut m) = abrir("busca");
    m.app.type_into("config-busca", "fonte");
    assert!(aparece(&mut m.app, &id_do_card("terminal.tamanho_fonte")));
    assert!(!aparece(&mut m.app, &id_do_card("geral.tema")));
    assert!(!aparece(&mut m.app, "config-secao-geral"));
    assert!(aparece(&mut m.app, "config-secao-terminal"));
    assert!(!aparece(&mut m.app, "config-busca-vazia"));
}

#[test]
fn a_busca_acha_ao_colar_por_imagem_e_por_claude() {
    for termo in ["imagem", "claude"] {
        let (_pasta, mut m) = abrir(&format!("busca-colar-{termo}"));
        m.app.type_into("config-busca", termo);
        assert!(
            aparece(&mut m.app, &id_do_card("terminal.colar_arquivos")),
            "\"{termo}\" não achou Ao colar"
        );
        assert!(!aparece(&mut m.app, &id_do_card("geral.tema")));
    }
}

#[test]
fn a_busca_ignora_acentos_e_caixa() {
    let (_pasta, mut m) = abrir("busca-acento");
    m.app.type_into("config-busca", "PAGINA INICIAR");
    assert!(aparece(&mut m.app, &id_do_card("geral.pagina_inicial")));
    assert!(!aparece(&mut m.app, &id_do_card("geral.tema")));
}

#[test]
fn a_busca_abre_o_grupo_que_tem_o_que_casa() {
    let (_pasta, mut m) = abrir("busca-grupo");
    m.app.type_into("config-busca", "executavel");
    assert!(aparece(&mut m.app, &id_do_card("terminal.shell")));
    assert!(aparece(&mut m.app, &id_do_valor("terminal.shell.programa")));
    assert!(!aparece(&mut m.app, &id_do_card("terminal.cursor")));
}

// --- A sanfona ---

/// Abre a página sem expandir nada, como a pessoa a vê.
fn abrir_recolhida(nome: &str) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(&format!("config-pagina-{nome}"));
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    (pasta, m)
}

#[test]
fn as_secoes_comecam_recolhidas_com_o_resumo() {
    let (_pasta, mut m) = abrir_recolhida("sanfona-recolhida");
    for secao in ["geral", "terminal", "cofre", "mcp", "atalhos"] {
        let id = format!("config-secao-{secao}");
        assert!(aparece(&mut m.app, &id), "{id} não aparece");
        assert!(!m.app.is_open(&id), "{id} começou aberta");
    }
    assert!(!aparece(&mut m.app, &id_do_card("geral.tema")));
    assert!(!aparece(&mut m.app, "config-restaurar-geral"));
    assert!(existe(&mut m.app, "config-secao-geral-conteudo"));
    m.app.click("config-secao-geral");
    assert!(m.app.is_open("config-secao-geral"));
    assert!(aparece(&mut m.app, &id_do_card("geral.tema")));
    assert!(!m.app.is_open("config-secao-terminal"));
}

#[test]
fn a_busca_fica_fora_da_rolagem() {
    let (_pasta, mut m) = abrir_recolhida("sanfona-busca-fixa");
    m.app.click("config-expandir-tudo");
    let antes = m.app.rect("config-busca");
    m.app.scroll("config-rolagem", 2000.0);
    assert_eq!(m.app.rect("config-busca"), antes);
    assert!(aparece(&mut m.app, "config-busca"));
}

#[test]
fn a_busca_abre_as_secoes_que_tem_o_que_casa() {
    let (_pasta, mut m) = abrir_recolhida("sanfona-busca");
    m.app.type_into("config-busca", "fonte");
    assert!(m.app.is_open("config-secao-terminal"));
    assert!(aparece(&mut m.app, &id_do_card("terminal.tamanho_fonte")));
    assert!(!m.app.is_open("config-secao-geral"));
    // Os atalhos também.
    m.app
        .click("config-busca")
        .shortcut('a')
        .type_text("painel");
    assert!(m.app.is_open("config-secao-atalhos"));
}

#[test]
fn limpar_a_busca_volta_as_secoes_a_como_estavam() {
    let (_pasta, mut m) = abrir_recolhida("sanfona-limpar");
    m.app.click("config-secao-geral");
    m.app.type_into("config-busca", "fonte");
    assert!(m.app.is_open("config-secao-terminal"));
    m.app
        .click("config-busca")
        .shortcut('a')
        .key(tomni_ui::Key::Backspace);
    assert!(m.app.is_open("config-secao-geral"));
    assert!(!m.app.is_open("config-secao-terminal"));
    assert!(aparece(&mut m.app, "config-secao-mcp"));
}

#[test]
fn expandir_tudo_e_recolher_tudo() {
    let (_pasta, mut m) = abrir_recolhida("sanfona-tudo");
    assert_eq!(m.app.label("config-expandir-tudo"), "Expandir tudo");
    m.app.click("config-expandir-tudo");
    for secao in ["geral", "terminal", "ia_servidor", "mcp", "atalhos"] {
        assert!(m.app.is_open(&format!("config-secao-{secao}")), "{secao}");
    }
    assert_eq!(m.app.label("config-expandir-tudo"), "Recolher tudo");
    m.app.click("config-expandir-tudo");
    assert!(!m.app.is_open("config-secao-geral"));
    assert!(!m.app.is_open("config-secao-atalhos"));
    assert_eq!(m.app.label("config-expandir-tudo"), "Expandir tudo");
    // Com uma só aberta, o link ainda expande.
    m.app.click("config-secao-geral");
    assert_eq!(m.app.label("config-expandir-tudo"), "Expandir tudo");
}

#[test]
fn sem_resultado_a_busca_diz_o_que_procurou() {
    let (_pasta, mut m) = abrir("busca-vazia");
    m.app.type_into("config-busca", "xyz");
    assert!(aparece(&mut m.app, "config-busca-vazia"));
    assert_eq!(
        m.app.text("config-busca-vazia"),
        "Nenhuma configuração encontrada para \"xyz\"."
    );
    assert!(!aparece(&mut m.app, "config-secao-geral"));
}

#[test]
fn a_busca_global_leva_a_configuracao() {
    let (_pasta, mut m) = abrir("busca-global");
    m.app.click("navegacao-inicio");
    m.app.type_into("busca", "campainha");
    m.app.key(tomni_ui::Key::Enter);
    assert!(
        m.app.checked("navegacao-configuracoes") || aparece(&mut m.app, "configuracoes-pagina")
    );
    assert!(aparece(&mut m.app, &id_do_card("terminal.campainha")));
    assert!(!aparece(&mut m.app, &id_do_card("terminal.cursor")));
}

// --- O arquivo ---

#[test]
fn o_que_o_arquivo_traz_aparece_na_pagina_e_o_aviso_da_leitura_tambem() {
    let pasta = Pasta::nova("config-pagina-arquivo");
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    std::fs::write(
        pasta.armazenamento().arquivo_configuracoes(),
        r#"{"geral": {"pagina_inicial": "inicio"}, "terminal": {"tamanho_fonte": 99}}"#,
    )
    .unwrap();
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    assert!(aparece(&mut m.app, "config-avisos-leitura"));
    let fonte = id_do_valor("terminal.tamanho_fonte");
    alcancar(&mut m.app, &fonte);
    assert_eq!(m.app.value(&fonte), 48.0);
}

#[test]
fn a_pagina_passa_no_lint_nos_dois_temas() {
    for tema in [Theme::Light, Theme::Dark] {
        let pasta = Pasta::nova("config-pagina-lint");
        let mut m = montar_pura(&pasta, tema);
        m.app.click("inicio-ferramenta-configuracoes");
        m.app.assert_lint();
    }
}

/// O atalho do menu (`geral.atalho_no_menu`, M49): ligado na abertura grava o `.desktop` e os
/// ícones; desligar apaga; ligar de novo grava. Só o Linux mostra o interruptor.
#[cfg(target_os = "linux")]
#[test]
fn o_atalho_no_menu_acompanha_o_interruptor() {
    let pasta = Pasta::nova("config-atalho-no-menu");
    let dados = pasta.caminho().join("dados");
    let destino = dados.clone();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, move |a| {
        a.xdg_data = Some(destino);
    });
    let desktop = dados.join("applications/tirech-omni.desktop");
    let icone = dados.join("icons/hicolor/scalable/apps/tirech-omni.svg");
    assert!(
        desktop.is_file(),
        "ligado por padrão, a abertura grava o atalho"
    );
    assert!(icone.is_file());
    let texto = std::fs::read_to_string(&desktop).unwrap();
    assert!(
        texto.contains("Exec=\"/opt/tirech-omni/tirech-omni\"\n"),
        "{texto}"
    );

    m.app.click("navegacao-configuracoes");
    let id = id_do_valor("geral.atalho_no_menu");
    alcancar(&mut m.app, &id);
    assert!(m.app.checked(&id));
    m.app.click(&id);
    assert!(
        !desktop.exists() && !icone.exists(),
        "desligar apaga o que o app gravou"
    );
    assert!(!gravada(&pasta).geral.atalho_no_menu);

    m.app.click(&id);
    assert!(desktop.is_file() && icone.is_file());
    assert!(gravada(&pasta).geral.atalho_no_menu);
}

/// A tela na página Configurações no modo portable (M66), com ou sem senha mestra.
fn abrir_portatil(nome: &str, senha_mestra: bool) -> (Pasta, Montagem) {
    let pasta = Pasta::nova(&format!("config-portatil-{nome}"));
    if senha_mestra {
        pasta.cofre_com_senha_mestra("mestra");
    }
    let caminho = pasta.caminho().to_owned();
    let (mut m, _) = montar_com_ambiente(&pasta, Theme::Light, |a| a.portatil = Some(caminho));
    if senha_mestra {
        m.app.click("cofre-desbloquear-close");
    }
    // O menu Arquivo não oferece instalar a partir do portable.
    m.app.click("menus-arquivo");
    assert!(!aparece(&mut m.app, "instalar-no-computador"));
    m.app.key(Key::Escape);
    m.app.click("navegacao-configuracoes");
    expandir_tudo(&mut m.app);
    (pasta, m)
}

#[test]
fn no_portable_somem_os_cards_que_escrevem_no_sistema() {
    let (_pasta, mut m) = abrir_portatil("cards", false);
    for chave in [
        "geral.iniciar_com_sistema",
        "geral.atalho_no_menu",
        "acao.instalacao",
        "agente.sistema_ssh_config",
        "agente.sistema_shells",
        "agente.sistema_sessao",
        "agente.sistema_git",
    ] {
        assert!(!aparece(&mut m.app, &id_do_card(chave)), "{chave} aparece");
    }
    // O resto da página continua.
    assert!(aparece(&mut m.app, &id_do_card("acao.pasta_dados")));
    assert!(aparece(&mut m.app, &id_do_card("acao.sobre")));
}

#[test]
fn no_portable_a_pasta_fica_fixa_e_sugere_a_senha_mestra() {
    let (_pasta, mut m) = abrir_portatil("pasta", false);
    let id = id_do_card("acao.pasta_dados");
    m.app.click(&format!("{id}-cabecalho"));
    assert!(!aparece(&mut m.app, "config-pasta-mudar"));
    assert!(aparece(&mut m.app, "config-pasta-abrir"));
    assert!(aparece(&mut m.app, "config-pasta-portatil-senha"));
}

#[test]
fn no_portable_com_senha_mestra_nao_ha_sugestao() {
    let (_pasta, mut m) = abrir_portatil("com-senha", true);
    let id = id_do_card("acao.pasta_dados");
    m.app.click(&format!("{id}-cabecalho"));
    assert!(!aparece(&mut m.app, "config-pasta-portatil-senha"));
}
