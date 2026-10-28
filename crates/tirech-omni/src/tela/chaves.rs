//! As chaves SSH na casca: os diálogos que valem em qualquer página (a passphrase de uma chave
//! salva, a confirmação de uso do agente e "Copiar a chave para o servidor") e o que as páginas
//! "Chaves SSH" e "Agente" pedem à tela (usar uma chave numa conexão).

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use ssh_key::PublicKey;
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::CopyIdOutcome;
use tomni_ui::terminal::ssh::keys;

use super::Tela;
use crate::agente::{acordar, ler_contador};
use crate::modelo::Kind;
use crate::t;
use crate::tela_chaves::{AcoesChaves, DeOnde};

/// O que a pessoa escolheu copiar: a chave pública e, se veio de um par em arquivo, a privada.
#[derive(Clone)]
struct OpcaoDeChave {
    rotulo: String,
    publica: PublicKey,
    privada: Option<PathBuf>,
}

/// O que a thread do `ssh-copy-id` devolve, com a conexão e a chave de origem.
struct Instalada {
    conexao: u64,
    nome: String,
    privada: Option<PathBuf>,
    resultado: Result<CopyIdOutcome, String>,
}

/// O arquivo da chave privada que corresponde a um `.pub`.
fn privada_de(publica: &Path) -> PathBuf {
    publica.with_extension("")
}

/// As chaves públicas que dá para copiar: os `.pub` da pasta e, depois, as do agente que não
/// têm `.pub` ali. Os rótulos não se repetem.
fn opcoes_de_chave(tela: &Tela) -> Vec<OpcaoDeChave> {
    let mut opcoes: Vec<OpcaoDeChave> = Vec::new();
    let mut arquivos: Vec<PathBuf> = fs::read_dir(tela.ferramentas.pasta_ssh())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "pub"))
        .collect();
    arquivos.sort();
    for arquivo in arquivos {
        let Ok(texto) = fs::read_to_string(&arquivo) else {
            continue;
        };
        let Ok(publica) = PublicKey::from_openssh(texto.trim()) else {
            continue;
        };
        let nome = arquivo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        opcoes.push(OpcaoDeChave {
            rotulo: format!("{nome} ({})", publica.algorithm()),
            publica,
            privada: Some(privada_de(&arquivo)),
        });
    }
    // As chaves do cofre (M44) vêm antes das do agente: uma que está nos dois aparece uma vez.
    let do_cofre: Vec<(String, String)> = tela.ferramentas.cofre().agora(|c| {
        c.chaves_ssh()
            .into_iter()
            .filter_map(|i| {
                i.chave
                    .as_ref()
                    .map(|k| (i.titulo.clone(), k.publica.clone()))
            })
            .collect()
    });
    for (titulo, linha) in do_cofre {
        let Ok((publica, _)) = keys::parse_public(&linha) else {
            continue;
        };
        let fingerprint = keys::fingerprint(&publica);
        if opcoes
            .iter()
            .any(|o| keys::fingerprint(&o.publica) == fingerprint)
        {
            continue;
        }
        opcoes.push(OpcaoDeChave {
            rotulo: t!("chaves-opcao-cofre", titulo = titulo.as_str()),
            publica,
            privada: None,
        });
    }
    for chave in tela.ferramentas.agente.chaves() {
        let fingerprint = keys::fingerprint(&chave.public);
        if opcoes
            .iter()
            .any(|o| keys::fingerprint(&o.publica) == fingerprint)
        {
            continue;
        }
        let nome = if chave.comment.is_empty() {
            fingerprint.clone()
        } else {
            chave.comment.clone()
        };
        opcoes.push(OpcaoDeChave {
            rotulo: t!("chaves-opcao-agente", nome = nome.as_str()),
            publica: chave.public,
            privada: None,
        });
    }
    // Rótulos iguais viram únicos com a impressão digital.
    for i in 0..opcoes.len() {
        let repetido = opcoes
            .iter()
            .enumerate()
            .any(|(j, o)| j != i && o.rotulo == opcoes[i].rotulo);
        if repetido {
            let fingerprint = keys::fingerprint(&opcoes[i].publica);
            opcoes[i].rotulo = format!("{} {fingerprint}", opcoes[i].rotulo);
        }
    }
    opcoes
}

impl Tela {
    /// O nome da conexão SSH escolhida na árvore, se há. Lê signals.
    pub(super) fn nome_da_conexao_ssh(&self) -> Option<String> {
        let id: u64 = self.selecionado.get().and_then(|c| c.parse().ok())?;
        self.tree.with(|t| {
            t.connection(id)
                .filter(|c| c.kind == Kind::Ssh)
                .map(|c| c.name.clone())
        })
    }

    /// Passa o arquivo da chave privada para a conexão SSH escolhida.
    fn usar_chave(&self, arquivo: &Path) {
        let Some(conexao) = self.conexao_selecionada().filter(|c| c.kind == Kind::Ssh) else {
            self.avisar(InfoBarSeverity::Warning, t!("chaves-escolha-conexao-ssh"));
            return;
        };
        self.campos.chave.set(arquivo.display().to_string());
        self.avisar(
            InfoBarSeverity::Success,
            t!(
                "chaves-conexao-agora-usa",
                conexao = conexao.name.as_str(),
                chave = arquivo.display().to_string()
            ),
        );
    }

    /// Passa a chave do cofre (o item `id`) para a conexão SSH escolhida, na linha "Chave do
    /// cofre". A chave de arquivo sai, porque a conexão usa uma só.
    fn usar_chave_do_cofre(&self, id: u64) {
        let Some(conexao) = self.conexao_selecionada().filter(|c| c.kind == Kind::Ssh) else {
            self.avisar(InfoBarSeverity::Warning, t!("chaves-escolha-conexao-ssh"));
            return;
        };
        let Some(item) = self.cofre.agora(|c| c.item(id).cloned()) else {
            self.avisar(InfoBarSeverity::Error, t!("chave-nao-esta-mais-no-cofre"));
            return;
        };
        self.campos.chave.set(String::new());
        self.campos.chave_cofre.set(super::rotulo(&item));
        self.avisar(
            InfoBarSeverity::Success,
            t!(
                "chaves-conexao-agora-usa",
                conexao = conexao.name.as_str(),
                chave = item.titulo.as_str()
            ),
        );
    }

    /// Grava `arquivo` como a chave da conexão `id`, escolhida ou não.
    fn definir_chave_da_conexao(&self, id: u64, arquivo: &Path) {
        let mudou = self.tree.try_update(|t| {
            t.connection_mut(id)
                .map(|c| c.key_file = Some(arquivo.to_path_buf()))
                .is_some()
        });
        if mudou != Some(true) {
            return;
        }
        self.atualizar();
        if self.id_selecionado() == Some(id) {
            self.carregar_propriedades();
        }
    }

    /// O que as páginas "Chaves SSH" e "Agente" pedem à casca.
    pub(super) fn acoes_de_chaves(
        self: &Rc<Self>,
        escolher_item: Signal<Option<u64>>,
        exportar_item: Signal<Option<u64>>,
    ) -> AcoesChaves {
        let (conexao, usar, servidor) = (Rc::clone(self), Rc::clone(self), Rc::clone(self));
        let usar_cofre = Rc::clone(self);
        let (cofre_item, destravar) = (Rc::clone(self), Rc::clone(self));
        AcoesChaves {
            exportar_item,
            cofre_item: Rc::new(move |id: u64| {
                escolher_item.set(Some(id));
                cofre_item.pagina.set("cofre".to_owned());
            }),
            destravar: Rc::new(move || destravar.desbloquear.set(true)),
            conexao: Rc::new(move || conexao.nome_da_conexao_ssh()),
            usar: Rc::new(move |arquivo: &Path| usar.usar_chave(arquivo)),
            usar_cofre: Rc::new(move |id: u64| usar_cofre.usar_chave_do_cofre(id)),
            servidor: Rc::new(move |origem: Option<DeOnde>| servidor.abrir_copiar_chave(origem)),
        }
    }

    /// Abre o diálogo "Copiar a chave para o servidor", com a chave `origem` já escolhida (a
    /// pública é a do `.pub` ao lado ou a do item do cofre).
    pub(super) fn abrir_copiar_chave(&self, origem: Option<DeOnde>) {
        let abrir = self.copiar_chave.borrow().clone();
        if let Some(abrir) = abrir {
            abrir(origem);
        }
    }
}

/// Os diálogos de chaves da casca. Também deixa em `tela.copiar_chave` a função que abre o de
/// copiar, para os menus.
pub(super) fn dialogos_de_chaves(tela: &Rc<Tela>) -> impl View {
    // Quando o cofre abre, as chaves salvas que esperavam a passphrase dele são carregadas.
    {
        let (t, feito) = (Rc::clone(tela), Cell::new(false));
        effect(move || {
            if t.desbloquear.get() || t.refazer_desbloqueio.get() {
                return;
            }
            untrack(|| {
                if !feito.replace(true) {
                    t.ferramentas.carregar_adiadas();
                }
            });
        });
    }
    Column::new()
        .child(dialogo_passphrase(tela))
        .child(dialogo_uso(tela))
        .child(dialogo_copiar(tela))
}

/// A passphrase de uma chave salva no `agente.json`, uma por vez, na ordem da fila.
fn dialogo_passphrase(tela: &Rc<Tela>) -> impl View {
    let ferr = Rc::clone(&tela.ferramentas);
    let aberto = signal(false);
    let passphrase = signal(String::new());
    let salvar = signal(true);
    let errou = signal(false);
    let (confirmou, refazer, foi_aberto) = (
        Rc::new(Cell::new(false)),
        Rc::new(Cell::new(false)),
        Cell::new(false),
    );

    {
        let (ferr, desbloquear) = (Rc::clone(&ferr), tela.desbloquear);
        let (confirmou, refazer) = (Rc::clone(&confirmou), Rc::clone(&refazer));
        effect(move || {
            let esta_aberto = aberto.get();
            let fila_vazia = ferr.cargas.with(|c| c.is_empty());
            let bloqueado = desbloquear.get();
            untrack(|| {
                if esta_aberto {
                    foi_aberto.set(true);
                    return;
                }
                if refazer.replace(false) {
                    aberto.set(true);
                    return;
                }
                passphrase.set(String::new());
                if foi_aberto.replace(false) && !confirmou.replace(false) {
                    // Fechou sem confirmar: a chave fica de fora até a próxima execução.
                    ferr.cancelar_carga();
                    return;
                }
                if !fila_vazia && !bloqueado {
                    errou.set(false);
                    aberto.set(true);
                }
            });
        });
    }

    let arquivo = {
        let ferr = Rc::clone(&ferr);
        move || {
            ferr.cargas.with(|c| {
                c.first()
                    .map(|carga| carga.arquivo.display().to_string())
                    .unwrap_or_default()
            })
        }
    };
    let nome = {
        let arquivo = arquivo.clone();
        move || crate::ferramentas::nome_do(Path::new(&arquivo()))
    };

    let confirmar = {
        let (ferr, confirmou, refazer) =
            (Rc::clone(&ferr), Rc::clone(&confirmou), Rc::clone(&refazer));
        move || {
            confirmou.set(true);
            let certa = ferr.concluir_carga(&passphrase.get_untracked(), salvar.get_untracked());
            errou.set(!certa);
            refazer.set(!certa);
            if !certa {
                // Errou: o campo volta vazio para digitar de novo.
                passphrase.set(String::new());
            }
        }
    };

    ContentDialog::new(move || t!("chaves-passphrase-de", nome = nome()), aberto)
        .id("agente-passphrase")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(arquivo)
                        .id("agente-passphrase-arquivo")
                        .secondary()
                        .wrap(),
                )
                .child(
                    Edit::bind(passphrase)
                        .id("agente-passphrase-campo")
                        .password()
                        .placeholder("Passphrase"),
                )
                .child(
                    CheckBox::bind(salvar, t!("comum-salvar-a-passphrase-no-cofre"))
                        .id("agente-passphrase-salvar"),
                )
                .child(
                    Text::new(t!("chave-passphrase-incorreta"))
                        .id("agente-passphrase-erro")
                        .caption()
                        .visible(move || errou.get()),
                ),
        )
        .primary(t!("chave-carregar"), confirmar)
        .close(t!("comum-agora-nao"))
}

/// "Permitir que um programa use a chave?": o agente espera até 60 s por esta resposta.
fn dialogo_uso(tela: &Rc<Tela>) -> impl View {
    let agente = Rc::clone(&tela.ferramentas.agente);
    let aberto = signal(false);
    let lembrar = signal(false);
    let (respondeu, foi_aberto) = (Rc::new(Cell::new(false)), Cell::new(false));

    {
        let (agente, respondeu) = (Rc::clone(&agente), Rc::clone(&respondeu));
        let t = Rc::clone(tela);
        effect(move || {
            let esta_aberto = aberto.get();
            let fila = agente.pedidos();
            untrack(|| {
                if esta_aberto {
                    foi_aberto.set(true);
                    return;
                }
                if foi_aberto.replace(false) && !respondeu.replace(false) {
                    // Fechou sem escolher: nega.
                    if let Some(pedido) = fila.first() {
                        agente.responder(pedido, false, false);
                    }
                    return;
                }
                if !fila.is_empty() {
                    lembrar.set(false);
                    aberto.set(true);
                    // O pedido pode chegar com o app escondido na bandeja ou atrás de outra janela.
                    t.mostrar_para_pergunta();
                    return;
                }
                t.esconder_depois_da_pergunta();
            });
        });
    }

    let texto = {
        let agente = Rc::clone(&agente);
        move |f: fn(&crate::agente::PedidoDeUso) -> String| {
            let agente = Rc::clone(&agente);
            move || agente.pedidos().first().map(|p| f(p)).unwrap_or_default()
        }
    };

    let permitir = {
        let (agente, respondeu) = (Rc::clone(&agente), Rc::clone(&respondeu));
        move || {
            respondeu.set(true);
            if let Some(pedido) = agente.pedidos_agora().first() {
                agente.responder(pedido, true, lembrar.get_untracked());
            }
        }
    };
    let negar = {
        let (agente, respondeu) = (Rc::clone(&agente), Rc::clone(&respondeu));
        move || {
            respondeu.set(true);
            if let Some(pedido) = agente.pedidos_agora().first() {
                agente.responder(pedido, false, false);
            }
        }
    };

    ContentDialog::new(t!("chave-usar-a-chave"), aberto)
        .id("agente-uso-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(texto(|p| p.programa.clone()))
                        .id("agente-uso-programa")
                        .body_strong()
                        .wrap(),
                )
                .child(
                    Text::new(t!("chave-quer-assinar-com-uma-chave-do"))
                        .secondary()
                        .wrap(),
                )
                .child(
                    Text::new(texto(|p| p.comentario.clone()))
                        .id("agente-uso-chave")
                        .body_strong()
                        .wrap(),
                )
                .child(
                    Text::new(texto(|p| {
                        p.destino
                            .as_ref()
                            .map(|d| t!("chaves-uso-para-destino", destino = d.as_str()))
                            .unwrap_or_default()
                    }))
                    .id("agente-uso-destino")
                    .secondary()
                    .wrap()
                    .visible({
                        let agente = Rc::clone(&agente);
                        move || {
                            agente
                                .pedidos()
                                .first()
                                .is_some_and(|p| p.destino.is_some())
                        }
                    }),
                )
                .child(
                    Text::new(texto(|p| p.fingerprint.clone()))
                        .id("agente-uso-fingerprint")
                        .caption()
                        .secondary()
                        .wrap(),
                )
                .child(
                    CheckBox::bind(lembrar, t!("chave-nao-perguntar-de-novo-para-este"))
                        .id("agente-uso-lembrar"),
                ),
        )
        .primary(t!("chave-permitir"), permitir)
        .secondary(t!("chave-negar"), negar)
        .close(t!("comum-fechar"))
}

/// "Copiar a chave para o servidor": escolhe a conexão SSH e a chave pública e roda o
/// `ssh-copy-id` numa thread. Deixa a função que abre o diálogo em `tela.copiar_chave`.
fn dialogo_copiar(tela: &Rc<Tela>) -> impl View {
    let aberto = signal(false);
    let rotulo_chave = signal(String::new());
    let rotulo_conexao = signal(String::new());
    let forcar = signal(false);
    let senha = signal(String::new());
    let opcoes_chave: Rc<RefCell<Vec<OpcaoDeChave>>> = Rc::default();
    let opcoes_conexao: Rc<RefCell<Vec<(String, u64)>>> = Rc::default();
    let pronto = signal(0u64);
    let saida: Arc<Mutex<Option<Instalada>>> = Arc::default();
    let usar_aberto = signal(false);
    let a_usar: Rc<RefCell<Option<(u64, String, PathBuf)>>> = Rc::default();
    let nome_a_usar = signal(String::new());

    // A senha do servidor acompanha a conexão escolhida: a do cofre, se há.
    {
        let (t, opcoes) = (Rc::clone(tela), Rc::clone(&opcoes_conexao));
        effect(move || {
            let escolhida = rotulo_conexao.get();
            untrack(|| {
                if !aberto.get_untracked() {
                    return;
                }
                let id = opcoes
                    .borrow()
                    .iter()
                    .find(|(r, _)| *r == escolhida)
                    .map(|(_, id)| *id);
                let conexao =
                    id.and_then(|id| t.tree.with_untracked(|tr| tr.conexao_para_abrir(id)));
                senha.set(
                    conexao
                        .and_then(|c| t.senhas_do_cofre(&c).conexao)
                        .unwrap_or_default(),
                );
            });
        });
    }

    let abrir: Rc<dyn Fn(Option<DeOnde>)> = {
        let (t, chaves, conexoes) = (
            Rc::clone(tela),
            Rc::clone(&opcoes_chave),
            Rc::clone(&opcoes_conexao),
        );
        Rc::new(move |origem: Option<DeOnde>| {
            let lista_de_chaves = opcoes_de_chave(&t);
            let lista_de_conexoes: Vec<(String, u64)> = t.tree.with_untracked(|tr| {
                let ssh: Vec<_> = tr
                    .connections()
                    .into_iter()
                    .filter(|c| c.kind == Kind::Ssh)
                    .collect();
                ssh.iter()
                    .map(|c| {
                        let repetido = ssh.iter().filter(|o| o.name == c.name).count() > 1;
                        let rotulo = if repetido {
                            format!("{} ({})", c.name, c.id)
                        } else {
                            c.name.clone()
                        };
                        (rotulo, c.id)
                    })
                    .collect()
            });
            if lista_de_conexoes.is_empty() {
                t.avisar(
                    InfoBarSeverity::Warning,
                    t!("chaves-crie-conexao-ssh-antes"),
                );
                return;
            }
            if lista_de_chaves.is_empty() {
                t.avisar(
                    InfoBarSeverity::Warning,
                    t!("chaves-sem-publica-para-copiar"),
                );
                return;
            }
            let chave_inicial = origem
                .as_ref()
                .and_then(|origem| {
                    lista_de_chaves.iter().find(|o| match origem {
                        DeOnde::Arquivo(p) => o.privada.as_ref() == Some(p),
                        DeOnde::Cofre(fingerprint) => keys::fingerprint(&o.publica) == *fingerprint,
                    })
                })
                .unwrap_or(&lista_de_chaves[0])
                .rotulo
                .clone();
            let escolhida = t.id_selecionado();
            let conexao_inicial = lista_de_conexoes
                .iter()
                .find(|(_, id)| Some(*id) == escolhida)
                .unwrap_or(&lista_de_conexoes[0])
                .0
                .clone();
            *chaves.borrow_mut() = lista_de_chaves;
            *conexoes.borrow_mut() = lista_de_conexoes;
            forcar.set(false);
            rotulo_chave.set(chave_inicial);
            // A senha vem do efeito acima; ele só age com o diálogo aberto.
            aberto.set(true);
            rotulo_conexao.set(conexao_inicial);
        })
    };
    *tela.copiar_chave.borrow_mut() = Some(Rc::clone(&abrir));

    let instalar = {
        let (t, chaves, conexoes, saida) = (
            Rc::clone(tela),
            Rc::clone(&opcoes_chave),
            Rc::clone(&opcoes_conexao),
            Arc::clone(&saida),
        );
        move || {
            let chave = chaves
                .borrow()
                .iter()
                .find(|o| o.rotulo == rotulo_chave.get_untracked())
                .cloned();
            let id = conexoes
                .borrow()
                .iter()
                .find(|(r, _)| *r == rotulo_conexao.get_untracked())
                .map(|(_, id)| *id);
            let (Some(chave), Some(conexao)) = (
                chave,
                id.and_then(|id| t.tree.with_untracked(|tr| tr.conexao_para_abrir(id))),
            ) else {
                return;
            };
            let mut senhas = t.senhas_do_cofre(&conexao);
            let digitada = senha.get_untracked();
            if !digitada.is_empty() {
                senhas.conexao = Some(digitada);
            }
            let efetiva = t.conexao_efetiva(&conexao);
            let instalador = t.opener.instalador(&efetiva, senhas);
            t.avisar(
                InfoBarSeverity::Informational,
                t!("chaves-instalando-em", conexao = conexao.name.as_str()),
            );
            let (forcada, saida) = (forcar.get_untracked(), Arc::clone(&saida));
            std::thread::spawn(move || {
                let resultado = instalador(chave.publica, forcada);
                *saida.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instalada {
                    conexao: conexao.id,
                    nome: conexao.name,
                    privada: chave.privada,
                    resultado,
                });
                acordar(pronto);
            });
        }
    };

    // A thread terminou: avisa e, se a chave é de um par em arquivo e a conexão ainda não tem
    // chave, oferece usá-la.
    {
        let (t, saida, a_usar) = (Rc::clone(tela), Arc::clone(&saida), Rc::clone(&a_usar));
        effect(move || {
            let _ = ler_contador(pronto);
            let Some(feita) = saida.lock().unwrap_or_else(PoisonError::into_inner).take() else {
                return;
            };
            untrack(|| {
                match &feita.resultado {
                    Ok(
                        CopyIdOutcome::Installed { path } | CopyIdOutcome::WouldInstall { path },
                    ) => {
                        t.avisar(
                            InfoBarSeverity::Success,
                            t!("chaves-instalada-em", caminho = path.as_str()),
                        );
                    }
                    Ok(CopyIdOutcome::AlreadyThere) => {
                        t.avisar(InfoBarSeverity::Success, t!("chaves-ja-estava-no-servidor"));
                    }
                    Err(erro) => {
                        t.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "chaves-nao-consegui-instalar",
                                conexao = feita.nome.as_str(),
                                motivo = erro.to_string()
                            ),
                        );
                        return;
                    }
                }
                let sem_chave = t.tree.with_untracked(|tr| {
                    tr.connection(feita.conexao).map(|c| c.key_file.is_none())
                }) == Some(true);
                if let (true, Some(privada)) = (sem_chave, feita.privada) {
                    nome_a_usar.set(feita.nome.clone());
                    *a_usar.borrow_mut() = Some((feita.conexao, feita.nome, privada));
                    usar_aberto.set(true);
                }
            });
        });
    }

    let (chaves_visao, conexoes_visao) = (Rc::clone(&opcoes_chave), Rc::clone(&opcoes_conexao));
    let dialogo = ContentDialog::new(t!("chave-copiar-a-chave-para-o-servidor"), aberto)
        .id("copiar-chave-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(t!("chave-conexao")).caption())
                // As listas mudam entre uma abertura e outra: os seletores nascem com o diálogo.
                .child(Show::when(aberto, move || {
                    let nomes: Vec<String> = conexoes_visao
                        .borrow()
                        .iter()
                        .map(|(r, _)| r.clone())
                        .collect();
                    ComboBox::bind(rotulo_conexao, nomes).id("copiar-chave-conexao")
                }))
                .child(Text::new(t!("chave-chave-publica")).caption())
                .child(Show::when(aberto, move || {
                    let nomes: Vec<String> = chaves_visao
                        .borrow()
                        .iter()
                        .map(|o| o.rotulo.clone())
                        .collect();
                    ComboBox::bind(rotulo_chave, nomes).id("copiar-chave-chave")
                }))
                .child(Text::new(t!("chave-senha-do-servidor")).caption())
                .child(
                    Edit::bind(senha)
                        .id("copiar-chave-senha")
                        .password()
                        .placeholder(t!("chave-senha-do-servidor")),
                )
                .child(
                    CheckBox::bind(forcar, t!("chave-forcar-instalar-mesmo-que-ja-esteja"))
                        .id("copiar-chave-forcar"),
                ),
        )
        .primary(t!("chave-instalar"), instalar)
        .close(t!("comum-cancelar"));

    let usar_sim = {
        let (t, a_usar) = (Rc::clone(tela), Rc::clone(&a_usar));
        move || {
            if let Some((id, _, privada)) = a_usar.borrow_mut().take() {
                t.definir_chave_da_conexao(id, &privada);
                t.avisar(
                    InfoBarSeverity::Success,
                    t!(
                        "chaves-conexao-agora-usa-a-chave",
                        chave = privada.display().to_string()
                    ),
                );
            }
        }
    };
    let dialogo_usar = ContentDialog::new(t!("chave-usar-esta-chave-na-conexao"), usar_aberto)
        .id("copiar-chave-usar")
        .content(
            Text::new(move || {
                t!(
                    "chaves-conexao-sem-arquivo-de-chave",
                    conexao = nome_a_usar.get()
                )
            })
            .id("copiar-chave-usar-texto")
            .wrap(),
        )
        .primary(t!("chave-usar-a-chave-2"), usar_sim)
        .close(t!("comum-agora-nao"));

    Column::new().child(dialogo).child(dialogo_usar)
}
