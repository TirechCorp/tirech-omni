//! A barra de menus, igual em todas as janelas (M62): a principal e as abertas por "Nova janela",
//! "Abrir em ›", "Mover para uma janela nova" e o modo cluster têm Arquivo, Exibir, Janela,
//! Ferramentas e Ajuda, com os mesmos ids, ícones, atalhos e teclas de acesso.
//!
//! As ações de sessão agem na janela do menu, as páginas (Cofre, Notas, Desempenho, Sobre)
//! abrem na navegação dela, e o Exibir mostra e esconde os painéis da página que ela mostra. As
//! que abrem algo que só existe na principal (os diálogos fora da árvore) trazem a principal para
//! a frente, também da bandeja, e agem lá ([`Tela::na_principal`]).

use std::rc::Rc;

use tomni_ui::dialogs;
use tomni_ui::prelude::*;
use tomni_ui::terminal::LogMode;

use super::duplicar::Destino;
use super::{
    Lugar, PaineisDaJanela, Tela, acao, com_ia, com_mcp, ia, importacao, janelas, mcp_sessoes, wsl,
};
use crate::abrir::{abrir_log_cifrado, com_sufixo_cifrado};
use crate::atalhos;
use crate::janelas::Janela;
use crate::paineis;
use crate::t;
use crate::tela_configuracoes::ligar;

impl Tela {
    /// Leva a janela principal para a frente (mostrando o app se ele está na bandeja) e roda `f`
    /// com a tela. Para nova conexão e novo grupo, que escolhem o item na árvore da principal. Na
    /// própria principal só dá o foco.
    pub(super) fn na_principal(self: &Rc<Self>, f: impl FnOnce(&Rc<Self>)) {
        if self.escondido.get_untracked() {
            self.mostrar();
        } else {
            self.janelas.principal().handle.focus();
        }
        f(self);
    }

    /// Roda `f` com o app à mostra: se ele está na bandeja, a principal volta antes. Para os
    /// diálogos gerais, que abrem na última janela ativa ([`Tela::dialogos_na_janela`]).
    pub(super) fn a_mostra(self: &Rc<Self>, f: impl FnOnce(&Rc<Self>)) {
        if self.escondido.get_untracked() {
            self.mostrar();
        }
        f(self);
    }
}

/// A barra de menus da janela `janela`. `paineis` são os controladores dos painéis das páginas
/// dela (os da principal ou os da secundária); o chat e as portas só existem na principal.
pub(super) fn barra_de_menus(
    tela: &Rc<Tela>,
    janela: Janela,
    paineis: &PaineisDaJanela,
) -> MenuBar {
    let principal = janela.handle == tela.janelas.principal().handle;
    MenuBar::new()
        .id("menus")
        .menu(t!("casca-arquivo"), menu_arquivo(tela))
        .access_key('A')
        .menu(
            t!("casca-exibir"),
            menu_exibir(tela, janela, paineis, principal),
        )
        .access_key('E')
        .menu(t!("casca-janela"), menu_janela(tela, janela))
        .access_key('N')
        .menu(t!("casca-ferramentas"), menu_ferramentas(tela, janela))
        .access_key('F')
        .menu(t!("casca-ajuda"), menu_ajuda(tela, janela))
        .access_key('J')
}

fn menu_arquivo(tela: &Rc<Tela>) -> MenuFlyout {
    let menu = MenuFlyout::new()
        .id("arquivo")
        .item(
            "nova-conexao-menu",
            t!("casca-nova-conexao"),
            acao(tela, |t| t.na_principal(|t| t.nova_conexao())),
        )
        .icon(Symbol::Add)
        .shortcut(atalhos::atalho(tela.config, "nova_conexao"))
        .item(
            "novo-grupo-menu",
            t!("casca-novo-grupo"),
            acao(tela, |t| t.na_principal(|t| t.novo_grupo())),
        )
        .icon(Symbol::FolderAdd)
        .shortcut(atalhos::atalho(tela.config, "novo_grupo"))
        .separator()
        .submenu(
            "importar-menu",
            t!("casca-importar"),
            importacao::menu_importar(tela),
        )
        .icon(Symbol::FolderOpen)
        .submenu(
            "exportar-menu",
            t!("casca-exportar"),
            importacao::menu_exportar(tela),
        )
        .icon(Symbol::Save)
        .item(
            "remotos-rclone-menu",
            t!("casca-remotos-do-rclone"),
            acao(tela, |t| t.abrir_remotos_do_rclone()),
        )
        .icon(Symbol::Cloud)
        .separator();
    // No modo portable (M66) o app não se instala: quem quiser instalar usa a cópia sem a pasta
    // `tirech-omnidata`.
    let menu = if tela.ferramentas.ambiente().portatil.is_some() {
        menu
    } else {
        menu.item(
            "instalar-no-computador",
            t!("casca-instalar-no-computador"),
            acao(tela, |t| t.a_mostra(|t| t.abrir_instalar(false))),
        )
        .icon(Symbol::Apps)
        .separator()
    };
    menu.item("sair", t!("casca-sair"), acao(tela, |t| t.sair()))
        .icon(Symbol::ArrowExit)
}

fn menu_ferramentas(tela: &Rc<Tela>, janela: Janela) -> MenuFlyout {
    MenuFlyout::new()
        .id("ferramentas")
        .item(
            "abrir-shell",
            t!("casca-abrir-shell-local"),
            acao(tela, move |t| t.abrir_shell_local(t.lugar_novo_em(janela))),
        )
        .icon(Symbol::WindowConsole)
        .shortcut(atalhos::atalho(tela.config, "shell_local"))
        .submenu("wsl", t!("casca-wsl"), wsl::submenu(tela))
        .icon(Symbol::WindowConsole)
        .item_visible({
            let lista = tela.wsl.lista;
            move || lista.with(|l| !l.is_empty())
        })
        .item(
            "abrir-sessao",
            t!("casca-abrir-a-conexao-escolhida"),
            acao(tela, move |t| {
                if let Some(id) = t.id_selecionado() {
                    t.abrir(id, t.lugar_novo_em(janela));
                }
            }),
        )
        .icon(Symbol::Open)
        .separator()
        .item(
            "reconectar",
            t!("casca-reconectar-a-sessao"),
            acao(tela, move |t| {
                if let Some(s) = t.sessao_ativa_de(janela) {
                    s.restart();
                }
            }),
        )
        .icon(Symbol::ArrowClockwise)
        .item(
            "duplicar",
            t!("casca-duplicar-a-sessao"),
            acao(tela, move |t| {
                if let Some(origem) = t.sessao_ativa_de(janela) {
                    t.duplicar(origem, Destino::Lugar(Lugar::na_janela(janela)));
                }
            }),
        )
        .icon(Symbol::Copy)
        .separator()
        .item(
            "multi-entrada-menu",
            t!("casca-envio-para-varios"),
            acao(tela, move |t| t.abrir_barra_de_envio(janela)),
        )
        .icon(Symbol::Broadcast)
        .shortcut(atalhos::atalho(tela.config, "multi_entrada"))
        .submenu("cluster-modo", t!("casca-modo-cluster"), menu_cluster(tela))
        .icon(Symbol::Grid)
        .item(
            "scripts-menu",
            t!("casca-rodar-script"),
            acao(tela, |t| t.abrir_scripts_da_selecao()),
        )
        .icon(Symbol::Code)
        .shortcut(atalhos::atalho(tela.config, "scripts"))
        .item(
            "links-menu",
            t!("links-menu"),
            acao(tela, |t| t.a_mostra(|t| t.abrir_lancador_de_links())),
        )
        .icon(Symbol::Globe)
        .shortcut(atalhos::atalho(tela.config, "links"))
        .submenu(
            "ia",
            t!("ia-remoto-menu"),
            ia::submenu_ia(tela, "ia", ia::AlvoIa::Janela(janela)),
        )
        .icon(Symbol::Sparkle)
        .item_visible({
            let t = Rc::clone(tela);
            let ligada = com_ia(tela.config);
            move || ligada() && t.aba_ssh(ia::AlvoIa::Janela(janela))
        })
        .separator()
        .item(
            "copiar-chave-menu",
            t!("casca-copiar-a-chave-para-o-servidor"),
            acao(tela, |t| t.a_mostra(|t| t.abrir_copiar_chave(None))),
        )
        .icon(Symbol::Send)
        .separator()
        // O Ctrl+Shift+K fica na casca (`tela_com`): vale também com esta página escondida.
        .item(
            "abrir-cofre",
            t!("casca-cofre-de-senhas"),
            acao(tela, move |_| janela.pagina.set("cofre".to_owned())),
        )
        .icon(Symbol::Key)
        // O Ctrl+Shift+J também fica na casca.
        .item(
            "abrir-notas",
            t!("casca-notas"),
            acao(tela, move |_| janela.pagina.set("notas".to_owned())),
        )
        .icon(Symbol::Notepad)
        .separator()
        .item(
            "reconectar-caidas",
            t!("casca-reconectar-as-abas-caidas"),
            acao(tela, |t| t.reconectar_caidas(None)),
        )
        .icon(Symbol::ArrowSync)
        .shortcut(atalhos::atalho(tela.config, "reconectar_caidas"))
        .item(
            "fechar-caidas",
            t!("casca-fechar-as-abas-caidas"),
            acao(tela, |t| t.fechar_caidas(None)),
        )
        .icon(Symbol::DismissCircle)
        .shortcut(atalhos::atalho(tela.config, "fechar_caidas"))
        .item(
            "fechar-todas",
            t!("casca-fechar-todas-as-abas"),
            acao(tela, |t| t.fechar_todas(None)),
        )
        .icon(Symbol::Dismiss)
        .shortcut(atalhos::atalho(tela.config, "fechar_todas"))
        .separator()
        .item(
            "gravar-log",
            t!("casca-salvar-o-log-da-sessao"),
            acao(tela, move |t| {
                let Some(s) = t.sessao_ativa_de(janela) else {
                    return;
                };
                let cifrar = t.config.with_untracked(|c| c.terminal.logs.cifrar);
                let pasta_de_dados = t.armazenamento.pasta().to_owned();
                let dialogo = dialogs::save_file()
                    .remember("log-da-sessao")
                    .title(t!("casca-log-da-sessao"));
                let dialogo = if cifrar {
                    dialogo.filter(t!("log-filtro-cifrado"), &["cifrado"])
                } else {
                    dialogo.filter(t!("comum-texto"), &["log", "txt"])
                };
                dialogo.show(move |caminho| {
                    let Some(caminho) = caminho else {
                        return;
                    };
                    if cifrar {
                        let caminho = crate::logs::caminho_livre(&com_sufixo_cifrado(&caminho));
                        match abrir_log_cifrado(&s, &caminho, &pasta_de_dados) {
                            Ok(()) => crate::logs::registrar_ativo(&s.key_str(), &caminho),
                            Err(erro) => s.feed(
                                format!(
                                    "{}\r\n",
                                    t!(
                                        "abrir-log-falhou",
                                        caminho = caminho.display().to_string(),
                                        erro = erro.to_string()
                                    )
                                )
                                .as_bytes(),
                            ),
                        }
                    } else if s.log_to(&caminho, LogMode::Text).is_ok() {
                        crate::logs::registrar_ativo(&s.key_str(), &caminho);
                    }
                });
            }),
        )
        .icon(Symbol::DocumentText)
        .item(
            "parar-log",
            t!("casca-parar-o-log"),
            acao(tela, move |t| {
                if let Some(s) = t.sessao_ativa_de(janela) {
                    s.stop_log();
                    crate::logs::esquecer_ativo(&s.key_str());
                }
            }),
        )
        .icon(Symbol::Stop)
        .separator_visible(com_mcp(tela.config))
        .submenu(
            "mcp-assistentes",
            t!("mcp-assistentes-de-ia"),
            mcp_sessoes::menu_assistentes(tela),
        )
        .icon(Symbol::Sparkle)
        .item_visible(com_mcp(tela.config))
}

fn menu_ajuda(tela: &Rc<Tela>, janela: Janela) -> MenuFlyout {
    MenuFlyout::new()
        .id("ajuda")
        .item(
            "atalhos",
            t!("casca-atalhos-de-teclado"),
            acao(tela, |t| t.a_mostra(|t| t.atalhos_aberto.set(true))),
        )
        .icon(Symbol::Keyboard)
        .item(
            "sobre",
            t!("casca-sobre"),
            acao(tela, move |_| janela.pagina.set("sobre".to_owned())),
        )
        .icon(Symbol::Info)
}

/// O menu Exibir de uma janela. Os painéis das páginas existem em todas as janelas, cada uma com
/// os seus (`paineis`), e os itens seguem a página que a janela mostra; o chat e as portas, só na
/// principal: nas outras janelas os itens deles aparecem desabilitados.
fn menu_exibir(
    tela: &Rc<Tela>,
    janela: Janela,
    paineis: &PaineisDaJanela,
    principal: bool,
) -> MenuFlyout {
    let envio = janela.envio;
    let mut menu = MenuFlyout::new()
        .id("exibir")
        .item(
            "layout-1",
            t!("casca-layout-1"),
            acao(tela, move |t| t.aplicar_grade(janela, 1, 1)),
        )
        .icon(Symbol::Window)
        .shortcut(atalhos::atalho(tela.config, "layout_1"))
        .item(
            "layout-2x2",
            t!("casca-layout-2-2"),
            acao(tela, move |t| t.aplicar_grade(janela, 2, 2)),
        )
        .icon(Symbol::Grid)
        .shortcut(atalhos::atalho(tela.config, "layout_2x2"))
        .item(
            "layout-3x3",
            t!("casca-layout-3-3"),
            acao(tela, move |t| t.aplicar_grade(janela, 3, 3)),
        )
        .icon(Symbol::Apps)
        .shortcut(atalhos::atalho(tela.config, "layout_3x3"))
        .item(
            "identificar",
            t!("casca-identificar-os-paineis"),
            acao(tela, |t| t.identificar_paineis()),
        )
        .icon(Symbol::Eye)
        .shortcut(atalhos::atalho(tela.config, "identificar_paineis"))
        .item(
            "desempenho",
            t!("casca-desempenho"),
            acao(tela, move |_| {
                janela.pagina.set(super::desempenho::PAGINA.to_owned());
            }),
        )
        .icon(Symbol::DataArea)
        .separator()
        .submenu(
            "envio",
            t!("casca-enviar-o-que-se-digita-para"),
            envio.submenu(),
        );
    // Os itens seguem a página da janela; o chat e as portas aparecem, desabilitados, fora da
    // principal (o menu é igual em todas).
    let pagina = janela.pagina;
    let em = move |nome: &'static str| move || pagina.get() == nome;
    let em_alguma = move || {
        pagina
            .with(|p| [paineis::TERMINAIS, paineis::NOTAS, paineis::ARQUIVOS].contains(&p.as_str()))
    };
    menu = menu
        .separator_visible(em_alguma)
        .toggle(
            "painel-conexoes",
            t!("casca-painel-conexoes"),
            paineis.terminais.shown("conexoes"),
        )
        .icon(Symbol::Server)
        .shortcut(atalhos::atalho(tela.config, "painel_conexoes"))
        .item_visible(em(paineis::TERMINAIS))
        .toggle(
            "painel-propriedades",
            t!("casca-painel-propriedades"),
            paineis.terminais.shown("propriedades"),
        )
        .icon(Symbol::Info)
        .shortcut(atalhos::atalho(tela.config, "painel_propriedades"))
        .item_visible(em(paineis::TERMINAIS))
        .toggle(
            "painel-chat",
            t!("casca-painel-chat"),
            tela.paineis_terminais.controle.shown("chat"),
        )
        .icon(Symbol::Chat)
        .shortcut(atalhos::atalho(tela.config, "ia_chat"))
        .item_visible({
            let (terminais, ligada) = (em(paineis::TERMINAIS), com_ia(tela.config));
            move || terminais() && ligada()
        })
        .item_enabled(principal)
        .toggle(
            "painel-portas",
            t!("casca-painel-portas"),
            tela.paineis_terminais.controle.shown("portas"),
        )
        .icon(Symbol::PlugConnected)
        .item_visible(em(paineis::TERMINAIS))
        .item_enabled(principal)
        .toggle(
            "mostrar-aba-na-arvore",
            t!("casca-mostrar-aba-na-arvore"),
            ligar(
                tela.config,
                |c| c.conexoes.mostrar_aba_na_arvore,
                |c, v| c.conexoes.mostrar_aba_na_arvore = v,
            ),
        )
        .icon(Symbol::Location)
        .item_visible(em(paineis::TERMINAIS))
        .toggle(
            "painel-notas-lista",
            t!("casca-painel-notas"),
            paineis.notas.controle.shown("lista"),
        )
        .icon(Symbol::Notepad)
        .shortcut(atalhos::atalho(tela.config, "painel_notas"))
        .item_visible(em(paineis::NOTAS))
        .toggle(
            "painel-fila",
            t!("casca-painel-fila"),
            paineis.arquivos.controle.shown("fila"),
        )
        .icon(Symbol::ArrowSync)
        .shortcut(atalhos::atalho(tela.config, "painel_fila"))
        .item_visible(em(paineis::ARQUIVOS));
    menu
}

/// O menu Janela: abre outra janela, salva as janelas como espaço de trabalho e dá o foco às
/// que existem. `janela` é a janela dona do menu (a que "Salvar esta janela" guarda).
fn menu_janela(tela: &Rc<Tela>, janela: Janela) -> MenuFlyout {
    let mut menu = MenuFlyout::new()
        .id("janela")
        .item(
            "nova-janela",
            t!("casca-nova-janela"),
            acao(tela, |t| t.nova_janela()),
        )
        .icon(Symbol::Window)
        .shortcut(atalhos::atalho(tela.config, "nova_janela"))
        .item(
            "salvar-janela-espaco",
            t!("casca-salvar-esta-janela-como-espaco"),
            acao(tela, move |t| t.salvar_janela_como_espaco(janela)),
        )
        .icon(Symbol::Save)
        .item(
            "salvar-todas-espaco",
            t!("casca-salvar-todas-as-janelas-como-espaco"),
            acao(tela, |t| t.salvar_todas_como_espaco()),
        )
        .icon(Symbol::Save)
        .separator();
    // Itens reservados: o menu não muda de forma, só mostra os das janelas que existem.
    for n in 0..janelas::NO_MENU {
        let janelas = tela.janelas.clone();
        let visivel = tela.janelas.clone();
        menu = menu
            .item(
                format!("janela-foco-{}", n + 1),
                janelas::no_menu(n),
                move || {
                    if let Some(janela) = janelas.todas_agora().get(n) {
                        janela.handle.focus();
                    }
                },
            )
            .item_visible(move || visivel.quantas() > n);
    }
    menu
}

/// O submenu "Modo cluster" de Ferramentas: o cluster de sempre, com o envio para todas, e o
/// cluster com IA, em que um só assistente do computador controla as máquinas escolhidas.
fn menu_cluster(tela: &Rc<Tela>) -> MenuFlyout {
    MenuFlyout::new()
        .id("cluster-modo-menu")
        .item(
            "cluster-menu",
            t!("casca-cluster-normal"),
            acao(tela, |t| {
                t.a_mostra(|t| t.abrir_dialogo_do_cluster_da_selecao(false))
            }),
        )
        .icon(Symbol::Grid)
        .shortcut(atalhos::atalho(tela.config, "cluster"))
        .item(
            "cluster-ia-menu",
            t!("casca-cluster-com-ia"),
            acao(tela, |t| {
                t.a_mostra(|t| t.abrir_dialogo_do_cluster_da_selecao(true))
            }),
        )
        .icon(Symbol::Sparkle)
        .shortcut(atalhos::atalho(tela.config, "cluster_ia"))
        .item_visible(com_mcp(tela.config))
}
