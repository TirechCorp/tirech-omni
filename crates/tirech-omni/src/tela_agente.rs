//! A página "Agente": as chaves que o agente embutido guarda, com o que dá para fazer com elas
//! (adicionar um arquivo, remover, travar) e o `SSH_AUTH_SOCK` para os outros programas.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use tomni_ui::clipboard;
use tomni_ui::dialogs;
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::agent::{AgentKey, OPENSSH_PIPE};
use tomni_ui::terminal::ssh::keys;

use crate::agente::{ChaveSalva, OrigemDaChave};
use crate::configuracoes::AgenteCfg;
use crate::ferramentas::{Ferramentas, motivo, pasta_ssh_padrao};
use crate::t;

/// O que a lista mostra de uma chave do agente.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LinhaDoAgente {
    fingerprint: String,
    tipo: String,
    comentario: String,
    validade: String,
    confirma: bool,
    /// De onde veio: "Cofre · github", "Arquivo · ~/.ssh/id_rsa" ou "Adicionada por outro programa".
    origem: String,
}

impl LinhaDoAgente {
    fn de(chave: &AgentKey, origem: String) -> Self {
        Self {
            fingerprint: keys::fingerprint(&chave.public),
            tipo: chave.public.algorithm().to_string(),
            comentario: chave.comment.clone(),
            validade: validade(chave.expires),
            confirma: chave.confirm,
            origem,
        }
    }

    /// A chave da linha no `For`: tudo que a linha mostra.
    fn chave(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.fingerprint, self.comentario, self.validade, self.confirma, self.origem
        )
    }

    /// Um sufixo para os ids: a impressão digital só com letras e dígitos.
    fn sufixo(&self) -> String {
        self.fingerprint
            .trim_start_matches("SHA256:")
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(8)
            .collect()
    }
}

/// "expira em 5 min", "expira em 2 h", "não expira". Relativo ao agora: o app não tem fuso.
fn validade(expira: Option<SystemTime>) -> String {
    let Some(fim) = expira else {
        return t!("agente-nao-expira");
    };
    let falta = fim
        .duration_since(SystemTime::now())
        .map_or(0, |d| d.as_secs());
    match falta {
        0 => t!("agente-expira-agora"),
        1..=59 => t!("agente-expira-menos-de-1-min"),
        60..=5399 => t!("agente-expira-em-min", n = falta.div_ceil(60)),
        _ => t!("agente-expira-em-h", n = falta.div_ceil(3600)),
    }
}

/// A página.
pub fn pagina_agente(ferr: Rc<Ferramentas>) -> impl View {
    let ligado = signal(ferr.cfg().ligado);
    let minutos = signal((ferr.cfg().vida_padrao_secs / 60) as f64);

    // A mesma página pode estar em outra janela: as caixas seguem a configuração.
    {
        let config = ferr.config;
        effect(move || {
            let (quer, vida) =
                config.with(|c| (c.agente.ligado, (c.agente.vida_padrao_secs / 60) as f64));
            untrack(|| {
                if ligado.get_untracked() != quer {
                    ligado.set(quer);
                }
                if minutos.get_untracked() != vida {
                    minutos.set(vida);
                }
            });
        });
    }

    // As caixas escrevem na configuração; quem já está certo não grava de novo.
    {
        let ferr = Rc::clone(&ferr);
        effect(move || {
            let quer = ligado.get();
            untrack(|| ferr.definir_ligado(quer));
        });
    }
    {
        let ferr = Rc::clone(&ferr);
        effect(move || {
            let m = minutos.get().max(0.0) as u64;
            untrack(|| {
                if ferr.cfg().vida_padrao_secs != m * 60 {
                    ferr.definir_vida_padrao(m * 60);
                }
            });
        });
    }

    let (adicionar, dialogo_adicionar) = dialogo_adicionar(&ferr, minutos);
    let (travar, dialogo_travar) = dialogo_travar(&ferr);

    let ferr_erro = Rc::clone(&ferr);
    let ferr_socket = Rc::clone(&ferr);
    let ferr_encurtado = Rc::clone(&ferr);
    let ferr_copiar = Rc::clone(&ferr);
    let ferr_lista = Rc::clone(&ferr);
    let ferr_vazio = Rc::clone(&ferr);
    let ferr_todas = Rc::clone(&ferr);
    let ferr_todas_ativo = Rc::clone(&ferr);
    let ferr_travado = Rc::clone(&ferr);
    let ferr_travado_texto = Rc::clone(&ferr);
    let ferr_adicionar_ativo = Rc::clone(&ferr);

    let lista = For::each(
        move || {
            let _ = ferr_lista.agente.versao();
            ferr_lista
                .agente
                .chaves()
                .iter()
                .map(|k| LinhaDoAgente::de(k, texto_da_origem(&ferr_lista, k)))
                .collect::<Vec<_>>()
        },
        LinhaDoAgente::chave,
        {
            let ferr = Rc::clone(&ferr);
            move |l: LinhaDoAgente| linha_do_agente(&l, &ferr)
        },
    );

    let conteudo = Column::new()
        .grow(1000)
        .max_width(900)
        .gap(16)
        .child(Text::new(t!("agente-agente-ssh")).title())
        .child(
            Text::new(t!("agente-o-agente-embutido-guarda-as-chaves"))
                .secondary()
                .wrap(),
        )
        .child(CheckBox::bind(ligado, t!("agente-ligar-o-agente-embutido")).id("agente-ligado"))
        .child(secao_sistema(&ferr))
        .child(
            Text::new(move || {
                let _ = ferr_erro.agente.versao();
                ferr_erro.agente.erro().unwrap_or_default()
            })
            .id("agente-erro")
            .wrap()
            .visible({
                let ferr = Rc::clone(&ferr);
                move || {
                    let _ = ferr.agente.versao();
                    ferr.agente.erro().is_some()
                }
            }),
        )
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(Text::new(t!("agente-tempo-de-vida-padrao-minutos-0")))
                .child(
                    NumberBox::bind(minutos)
                        .id("agente-vida")
                        .min(0.0)
                        .max(10_080.0),
                ),
        )
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Button::new(t!("agente-adicionar-arquivo"))
                        .id("agente-adicionar")
                        .icon(Symbol::Add)
                        .disabled(move || !ferr_adicionar_ativo.agente.rodando())
                        .on_click(move || adicionar()),
                )
                .child(
                    Button::new(t!("agente-remover-todas"))
                        .id("agente-remover-todas")
                        .icon(Symbol::Delete)
                        .disabled(move || {
                            let _ = ferr_todas_ativo.agente.versao();
                            ferr_todas_ativo.agente.chaves().is_empty()
                        })
                        .on_click(move || ferr_todas.remover_todas()),
                )
                .child(
                    Button::new(move || {
                        let _ = ferr_travado_texto.agente.versao();
                        if ferr_travado_texto.agente.travado() {
                            t!("agente-destravar")
                        } else {
                            t!("agente-travar")
                        }
                    })
                    .id("agente-travar")
                    .icon(Symbol::LockClosed)
                    .disabled({
                        let ferr = Rc::clone(&ferr);
                        move || !ferr.agente.rodando()
                    })
                    .on_click(move || travar(ferr_travado.agente.travado())),
                ),
        )
        .child({
            let mensagem = move || {
                let _ = ferr_vazio.agente.versao();
                if !ferr_vazio.agente.rodando() {
                    t!("agente-vazio-desligado")
                } else if ferr_vazio.agente.travado() {
                    t!("agente-vazio-travado")
                } else if ferr_vazio.agente.chaves().is_empty() {
                    t!("agente-vazio-sem-chaves")
                } else {
                    String::new()
                }
            };
            let escondida = mensagem.clone();
            Text::new(mensagem)
                .id("agente-vazio")
                .secondary()
                .wrap()
                .visible(move || !escondida().is_empty())
        })
        .child(Column::new().gap(8).child(lista))
        .child(
            Column::new()
                .gap(4)
                .child(Text::new(t!("agente-para-outros-programas")).subtitle())
                .child(
                    Text::new(move || format!("SSH_AUTH_SOCK={}", ferr_socket.socket().display()))
                        .id("agente-socket")
                        .caption()
                        .wrap(),
                )
                .child(
                    Text::new(t!("agente-endereco-encurtado"))
                        .id("agente-endereco-encurtado")
                        .caption()
                        .secondary()
                        .wrap()
                        .visible(move || ferr_encurtado.socket_encurtado()),
                )
                .child(
                    Row::new().child(
                        Button::new(t!("agente-copiar-export-ssh-auth-sock"))
                            .id("agente-copiar-export")
                            .icon(Symbol::Copy)
                            .on_click(move || {
                                clipboard::write(format!(
                                    "export SSH_AUTH_SOCK={}",
                                    ferr_copiar.socket().display()
                                ));
                            }),
                    ),
                ),
        )
        .child(dialogo_adicionar)
        .child(dialogo_travar);

    // Alinhado à esquerda, como as páginas de configurações do WinUI 3; o espaçador da
    // direita segura a largura máxima.
    Column::new().id("agente-pagina").grow(1).child(
        ScrollViewer::new().id("agente-rolagem").grow(1).child(
            Row::new()
                .padding(Insets::new(24.0, 40.0, 24.0, 40.0))
                .child(conteudo)
                .child(Column::new().grow(1)),
        ),
    )
}

/// "Cofre · github", "Arquivo · ~/.ssh/id_rsa" ou "Adicionada por outro programa".
fn texto_da_origem(ferr: &Ferramentas, chave: &AgentKey) -> String {
    match ferr.agente.origem_de(chave) {
        OrigemDaChave::Cofre(uid) => {
            let titulo = ferr.cofre().agora(|c| {
                c.item_por_uid(&uid)
                    .or_else(|| {
                        uid.strip_prefix('#')
                            .and_then(|id| id.parse().ok())
                            .and_then(|id| c.item(id))
                    })
                    .map(|i| i.titulo.clone())
            });
            match titulo {
                Some(t) if !t.is_empty() => t!("agente-origem-cofre", titulo = t.as_str()),
                _ => t!("agente-origem-cofre-sem-titulo"),
            }
        }
        OrigemDaChave::Arquivo(arquivo) => {
            let pessoal = ferr.ambiente().pessoal.clone();
            let texto = match pessoal
                .as_deref()
                .and_then(|p| arquivo.strip_prefix(p).ok())
            {
                Some(resto) => format!("~/{}", resto.display()),
                None => arquivo.display().to_string(),
            };
            t!("agente-origem-arquivo", arquivo = texto.as_str())
        }
        OrigemDaChave::Externa => t!("agente-origem-externa"),
    }
}

/// Liga um interruptor a um campo de `agente.*` da configuração.
fn ligar_cfg(
    ferr: &Rc<Ferramentas>,
    ler: fn(&AgenteCfg) -> bool,
    gravar: fn(&mut AgenteCfg, bool),
) -> Signal<bool> {
    let config = ferr.config;
    let valor = signal(config.with_untracked(|c| ler(&c.agente)));
    effect(move || {
        let novo = valor.get();
        untrack(|| {
            if config.with_untracked(|c| ler(&c.agente) != novo) {
                config.update(|c| gravar(&mut c.agente, novo));
            }
        });
    });
    effect(move || {
        let atual = config.with(|c| ler(&c.agente));
        untrack(|| {
            if valor.get_untracked() != atual {
                valor.set(atual);
            }
        });
    });
    valor
}

/// Um card de configuração com um interruptor, como os da página Configurações.
fn cartao(titulo: String, id: &'static str, valor: Signal<bool>) -> SettingsCard {
    SettingsCard::new(titulo).content(ToggleSwitch::bind(valor).id(id))
}

/// "Programas do sistema": faz o `ssh`, o git e o PuTTY de fora do app usarem as chaves dele.
fn secao_sistema(ferr: &Rc<Ferramentas>) -> impl View + use<> {
    let windows = ferr.ambiente().windows;
    let estado = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            ferr.config.with(|_| ());
            if ferr.sistema_usa_o_app() {
                t!("agente-sistema-usa")
            } else {
                t!("agente-sistema-nao-usa")
            }
        }
    };
    let outro = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            ferr.agente.outro_app()
        }
    };
    // No portable o app não escreve fora da pasta: diz o endereço e deixa o resto com quem usa.
    let portatil = ferr.ambiente().portatil.is_some();
    let endereco = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            t!(
                "agente-sistema-portatil",
                socket = ferr.socket().display().to_string()
            )
        }
    };
    let (ferr_windows, ferr_unix) = (Rc::clone(ferr), Rc::clone(ferr));
    let corpo = Show::when(windows, move || secao_windows(&ferr_windows))
        .otherwise(move || secao_unix(&ferr_unix));
    Column::new()
        .id("agente-sistema")
        .gap(8)
        .child(Text::new(t!("agente-programas-do-sistema")).subtitle())
        .child(
            Text::new(endereco)
                .id("agente-sistema-portatil")
                .secondary()
                .wrap()
                .visible(portatil),
        )
        .child(
            Text::new(estado)
                .id("agente-sistema-estado")
                .secondary()
                .wrap()
                .visible(!portatil),
        )
        .child(
            Text::new(t!("agente-outro-app-aberto-ja-atende"))
                .id("agente-sistema-outro")
                .secondary()
                .wrap()
                .visible(move || !portatil && outro()),
        )
        .child(Column::new().visible(!portatil).child(corpo))
        .child(secao_pageant(ferr))
}

/// Linux e macOS: o bloco do `~/.ssh/config`, os shells e a sessão gráfica.
fn secao_unix(ferr: &Rc<Ferramentas>) -> impl View + use<> {
    let ssh_config = ligar_cfg(
        ferr,
        |c| c.sistema_ssh_config,
        |c, v| c.sistema_ssh_config = v,
    );
    let shells = ligar_cfg(ferr, |c| c.sistema_shells, |c, v| c.sistema_shells = v);
    let sessao = ligar_cfg(ferr, |c| c.sistema_sessao, |c, v| c.sistema_sessao = v);
    let (bloco_aberto, dialogo) = dialogo_do_bloco(ferr);
    let fechado = signal(false);
    let primeira_vez = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            !fechado.get()
                && !ferr.config.with(|c| c.agente.sistema_ssh_config)
                && !ferr.sistema_usa_o_app()
        }
    };
    Column::new()
        .gap(8)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .visible(primeira_vez)
                .child(
                    InfoBar::new(InfoBarSeverity::Informational)
                        .id("agente-sistema-aviso")
                        .message(t!("agente-o-ssh-do-sistema-terminal-git"))
                        .grow(1)
                        .on_close(move || fechado.set(true)),
                )
                .child(
                    Button::new(t!("agente-usar-no-ssh-do-sistema"))
                        .id("agente-sistema-usar")
                        .accent()
                        .on_click(move || ssh_config.set(true)),
                ),
        )
        .child(cartao(
            t!("agente-cartao-ssh-config"),
            "agente-sistema-ssh-config",
            ssh_config,
        ))
        .child(cartao(
            t!("agente-cartao-shells"),
            "agente-sistema-shells",
            shells,
        ))
        .child(cartao(
            t!("agente-cartao-sessao"),
            "agente-sistema-sessao",
            sessao,
        ))
        .child(
            Row::new().child(
                Button::new(t!("agente-ver-o-que-e-escrito"))
                    .id("agente-sistema-ver-bloco")
                    .icon(Symbol::Document)
                    .on_click(move || bloco_aberto.set(true)),
            ),
        )
        .child(dialogo)
}

/// O diálogo com o texto que as opções escrevem, para conferir antes de ligar.
fn dialogo_do_bloco(ferr: &Rc<Ferramentas>) -> (Signal<bool>, impl View + use<>) {
    let aberto = signal(false);
    let texto = signal(String::new());
    {
        let ferr = Rc::clone(ferr);
        effect(move || {
            if aberto.get() {
                let novo = untrack(|| ferr.texto_do_bloco());
                texto.set(novo);
            }
        });
    }
    let dialogo = ContentDialog::new(t!("agente-o-que-e-escrito"), aberto)
        .id("agente-sistema-bloco-dialogo")
        .content(
            Edit::bind(texto)
                .id("agente-sistema-bloco")
                .multiline()
                .read_only()
                .height(260),
        )
        .close(t!("comum-fechar"));
    (aberto, dialogo)
}

/// Windows: o endereço do pipe, o serviço do OpenSSH e o git.
fn secao_windows(ferr: &Rc<Ferramentas>) -> impl View + use<> {
    let git = ligar_cfg(ferr, |c| c.sistema_git, |c, v| c.sistema_git = v);
    let sessao = ligar_cfg(ferr, |c| c.sistema_sessao, |c, v| c.sistema_sessao = v);
    let tentar = {
        let ferr = Rc::clone(ferr);
        move || {
            ferr.desligar_agente();
            ferr.ligar_agente();
        }
    };
    let copiar = || {
        clipboard::write("Stop-Service ssh-agent; Set-Service ssh-agent -StartupType Disabled");
    };
    let servico = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            ferr.agente.servico_ocupa()
        }
    };
    let pipe = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            let socket = ferr.socket();
            let padrao = socket.as_os_str() == OPENSSH_PIPE;
            t!(
                "agente-endereco",
                socket = socket.display().to_string(),
                padrao = if padrao {
                    // O Fluent tira o espaço do começo de um valor: ele vem daqui.
                    format!(" {}", t!("agente-endereco-padrao"))
                } else {
                    String::new()
                }
            )
        }
    };
    let git_existe = {
        let ferr = Rc::clone(ferr);
        let existe = (ferr.ambiente().executor)("git", &["--version"]).is_ok();
        move || existe
    };
    let git_ativo = git_existe;
    Column::new()
        .gap(8)
        .child(Text::new(pipe).id("agente-sistema-pipe").caption().wrap())
        .child(
            Column::new()
                .gap(8)
                .visible(servico)
                .child(
                    InfoBar::new(InfoBarSeverity::Warning)
                        .id("agente-sistema-servico")
                        .message(t!("agente-o-agente-do-windows-servico-ssh")),
                )
                .child(
                    Row::new()
                        .gap(8)
                        .child(
                            Button::new(t!("agente-copiar-o-comando"))
                                .id("agente-sistema-copiar-comando")
                                .icon(Symbol::Copy)
                                .on_click(copiar),
                        )
                        .child(
                            Button::new(t!("comum-tentar-de-novo"))
                                .id("agente-sistema-tentar")
                                .on_click(tentar),
                        ),
                )
                .child(cartao(
                    t!("agente-cartao-sessao-windows"),
                    "agente-sistema-sessao",
                    sessao,
                )),
        )
        .child(
            SettingsCard::new(t!("agente-usar-o-ssh-do-windows-no")).content(
                ToggleSwitch::bind(git)
                    .id("agente-sistema-git")
                    .disabled(move || !git_ativo()),
            ),
        )
        .child(
            Text::new(t!("agente-o-git-nao-foi-encontrado-neste"))
                .id("agente-sistema-git-aviso")
                .caption()
                .secondary()
                .visible(move || !git_existe()),
        )
}

/// O Pageant: o PuTTY, o WinSCP e o FileZilla usam as chaves do app. Só aparece no Windows.
fn secao_pageant(ferr: &Rc<Ferramentas>) -> impl View + use<> {
    let windows = ferr.ambiente().windows;
    let ligado = ligar_cfg(ferr, |c| c.pageant, |c, v| c.pageant = v);
    let estado = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            if ferr.agente.pageant_ligado() {
                t!("agente-pageant-usa")
            } else {
                t!("agente-pageant-desligado")
            }
        }
    };
    let ocupado = {
        let ferr = Rc::clone(ferr);
        move || {
            let _ = ferr.agente.versao();
            ferr.agente.pageant_ocupado()
        }
    };
    let tentar = {
        let ferr = Rc::clone(ferr);
        move || {
            if ferr.agente.definir_pageant(ligado.get_untracked()) {
                ferr.agente.marcar_pageant_ocupado(false);
            }
        }
    };
    Column::new()
        .id("agente-pageant-secao")
        .gap(8)
        .visible(move || windows)
        .child(cartao(
            t!("agente-cartao-pageant"),
            "agente-pageant",
            ligado,
        ))
        .child(
            Text::new(estado)
                .id("agente-pageant-estado")
                .secondary()
                .wrap(),
        )
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .visible(ocupado)
                .child(
                    InfoBar::new(InfoBarSeverity::Warning)
                        .id("agente-pageant-ocupado")
                        .message(t!("agente-o-pageant-do-putty-ja-esta"))
                        .grow(1),
                )
                .child(
                    Button::new(t!("comum-tentar-de-novo"))
                        .id("agente-pageant-tentar")
                        .on_click(tentar),
                ),
        )
}

/// Uma linha da lista do agente.
fn linha_do_agente(l: &LinhaDoAgente, ferr: &Rc<Ferramentas>) -> Card {
    let id = format!("agente-chave-{}", l.sufixo());
    let remover = {
        let (ferr, fingerprint) = (Rc::clone(ferr), l.fingerprint.clone());
        move || {
            if let Some(chave) = ferr
                .agente
                .chaves()
                .into_iter()
                .find(|k| keys::fingerprint(&k.public) == fingerprint)
            {
                ferr.remover(&chave);
            }
        }
    };
    let mut detalhe = vec![l.tipo.clone(), l.validade.clone()];
    if l.confirma {
        detalhe.push(t!("agente-pede-confirmacao"));
    }
    Card::new()
        .id(id.clone())
        .padding(12)
        .label(if l.comentario.is_empty() {
            l.fingerprint.clone()
        } else {
            l.comentario.clone()
        })
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Column::new()
                        .grow(1)
                        .child(
                            Text::new(if l.comentario.is_empty() {
                                t!("agente-sem-comentario")
                            } else {
                                l.comentario.clone()
                            })
                            .id(format!("{id}-comentario"))
                            .body_strong(),
                        )
                        .child(
                            Text::new(l.fingerprint.clone())
                                .id(format!("{id}-fingerprint"))
                                .caption()
                                .secondary()
                                .wrap(),
                        )
                        .child(
                            Text::new(detalhe.join(" · "))
                                .id(format!("{id}-detalhe"))
                                .caption(),
                        )
                        .child(
                            Text::new(l.origem.clone())
                                .id(format!("{id}-origem"))
                                .caption()
                                .secondary()
                                .wrap(),
                        ),
                )
                .child(
                    Button::new(t!("comum-remover"))
                        .id(format!("{id}-remover"))
                        .icon(Symbol::Delete)
                        .on_click(remover),
                ),
        )
}

/// O diálogo "Adicionar arquivo…": devolve o que o abre e a view.
fn dialogo_adicionar(
    ferr: &Rc<Ferramentas>,
    vida_padrao: Signal<f64>,
) -> (Rc<dyn Fn()>, impl View) {
    let aberto = signal(false);
    let arquivo = signal(String::new());
    let passphrase = signal(String::new());
    let salvar = signal(false);
    let confirmar = signal(false);
    let minutos = signal(0.0_f64);
    let erro = signal(String::new());
    let refazer = Rc::new(Cell::new(false));

    {
        let refazer = Rc::clone(&refazer);
        effect(move || {
            if aberto.get() {
                return;
            }
            if refazer.replace(false) {
                aberto.set(true);
            } else {
                untrack(|| {
                    for campo in [passphrase, erro] {
                        campo.set(String::new());
                    }
                });
            }
        });
    }

    let abrir_ferr = Rc::clone(ferr);
    let abrir: Rc<dyn Fn()> = Rc::new(move || {
        arquivo.set(String::new());
        confirmar.set(abrir_ferr.cfg().confirmar_padrao);
        salvar.set(false);
        minutos.set(vida_padrao.get_untracked());
        erro.set(String::new());
        aberto.set(true);
    });

    let escolher = {
        let ferr = Rc::clone(ferr);
        move || {
            let pasta = ferr.pasta_ssh();
            let pasta: PathBuf = if pasta.is_dir() {
                pasta.clone()
            } else {
                pasta_ssh_padrao()
            };
            dialogs::open_file()
                .remember("chaves")
                .title(t!("agente-escolher-a-chave-privada"))
                .directory(pasta)
                .show(move |caminho| {
                    if let Some(caminho) = caminho {
                        arquivo.set(caminho.display().to_string());
                    }
                });
        }
    };

    let ok = {
        let (ferr, refazer) = (Rc::clone(ferr), Rc::clone(&refazer));
        move || {
            let caminho = PathBuf::from(arquivo.get_untracked().trim());
            if caminho.as_os_str().is_empty() {
                erro.set(t!("agente-escolha-o-arquivo"));
                refazer.set(true);
                return;
            }
            let mut chave = ChaveSalva::new(&caminho);
            chave.confirmar = confirmar.get_untracked();
            chave.vida_secs = (minutos.get_untracked().max(0.0) as u64) * 60;
            let senha = passphrase.get_untracked();
            match ferr.carregar_com(&caminho, chave, &senha, salvar.get_untracked()) {
                Ok(()) => erro.set(String::new()),
                Err(e) => {
                    erro.set(motivo(&e));
                    refazer.set(true);
                }
            }
        }
    };

    let dialogo = ContentDialog::new(t!("agente-adicionar-chave-ao-agente"), aberto)
        .id("agente-adicionar-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Row::new()
                        .gap(8)
                        .child(
                            Edit::bind(arquivo)
                                .id("agente-adicionar-arquivo")
                                .grow(1)
                                .placeholder(t!("agente-arquivo-da-chave-privada")),
                        )
                        .child(
                            Button::new("…")
                                .id("agente-adicionar-escolher")
                                .label(t!("agente-escolher-o-arquivo"))
                                .on_click(escolher),
                        ),
                )
                .child(
                    Edit::bind(passphrase)
                        .id("agente-adicionar-passphrase")
                        .password()
                        .placeholder(t!("agente-passphrase-se-a-chave-tiver")),
                )
                .child(
                    CheckBox::bind(salvar, t!("comum-salvar-a-passphrase-no-cofre"))
                        .id("agente-adicionar-salvar"),
                )
                .child(
                    CheckBox::bind(confirmar, t!("comum-pedir-confirmacao-a-cada-uso"))
                        .id("agente-adicionar-confirmar"),
                )
                .child(
                    Row::new()
                        .gap(8)
                        .align(Align::Center)
                        .child(Text::new(t!("agente-tempo-de-vida-minutos-0-sem")))
                        .child(
                            NumberBox::bind(minutos)
                                .id("agente-adicionar-vida")
                                .min(0.0)
                                .max(10_080.0),
                        ),
                )
                .child(
                    Text::new(move || erro.get())
                        .id("agente-adicionar-erro")
                        .caption()
                        .wrap()
                        .visible(move || !erro.get().is_empty()),
                ),
        )
        .primary(t!("comum-adicionar"), ok)
        .close(t!("comum-cancelar"));
    (abrir, dialogo)
}

/// O diálogo de travar e destravar: devolve a função que o abre (com `true` se o agente está
/// travado e a ação é destravar) e a view.
fn dialogo_travar(ferr: &Rc<Ferramentas>) -> (Rc<dyn Fn(bool)>, impl View) {
    let aberto = signal(false);
    let destravando = signal(false);
    let passphrase = signal(String::new());
    let erro = signal(String::new());
    let refazer = Rc::new(Cell::new(false));

    {
        let refazer = Rc::clone(&refazer);
        effect(move || {
            if aberto.get() {
                return;
            }
            if refazer.replace(false) {
                aberto.set(true);
            } else {
                untrack(|| {
                    passphrase.set(String::new());
                    erro.set(String::new());
                });
            }
        });
    }

    let abrir: Rc<dyn Fn(bool)> = Rc::new(move |travado| {
        destravando.set(travado);
        passphrase.set(String::new());
        erro.set(String::new());
        aberto.set(true);
    });

    let ok = {
        let (ferr, refazer) = (Rc::clone(ferr), Rc::clone(&refazer));
        move || {
            let senha = passphrase.get_untracked();
            if destravando.get_untracked() {
                if !ferr.agente.destravar(&senha) {
                    erro.set(t!("agente-passphrase-nao-confere"));
                    refazer.set(true);
                }
            } else if senha.is_empty() {
                erro.set(t!("agente-escolha-passphrase"));
                refazer.set(true);
            } else if !ferr.agente.travar(&senha) {
                erro.set(t!("agente-nao-travou"));
                refazer.set(true);
            }
        }
    };

    let dialogo = ContentDialog::new(
        move || {
            if destravando.get() {
                t!("agente-destravar-o-agente")
            } else {
                t!("agente-travar-o-agente")
            }
        },
        aberto,
    )
    .id("agente-travar-dialogo")
    .content(
        Column::new()
            .gap(8)
            .child(
                Text::new(move || {
                    if destravando.get() {
                        t!("agente-digite-passphrase")
                    } else {
                        t!("agente-travado-explica")
                    }
                })
                .secondary()
                .wrap(),
            )
            .child(
                Edit::bind(passphrase)
                    .id("agente-travar-passphrase")
                    .password()
                    .placeholder("Passphrase"),
            )
            .child(
                Text::new(move || erro.get())
                    .id("agente-travar-erro")
                    .caption()
                    .wrap()
                    .visible(move || !erro.get().is_empty()),
            ),
    )
    .primary(
        move || {
            if destravando.get() {
                t!("agente-botao-destravar")
            } else {
                t!("agente-botao-travar")
            }
        },
        ok,
    )
    .close(t!("comum-cancelar"));
    (abrir, dialogo)
}
