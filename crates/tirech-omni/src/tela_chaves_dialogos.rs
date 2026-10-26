//! Os diálogos da página "Chaves SSH" que o M44 trouxe: importar (de arquivo ou colada), trazer
//! as chaves de `~/.ssh` para o cofre, exportar e converter, e as confirmações de apagar.
//!
//! Cada função devolve a função que abre o diálogo e a view dele, como o `dialogo_trocar`. O
//! `ContentDialog` fecha a cada botão: quando a ação falha, o erro fica no próprio diálogo e o
//! efeito o abre de novo (o `refazer`). Nos que têm "Salvar" e "Copiar", os botões ficam dentro
//! do conteúdo e o diálogo só fecha quando a ação dá certo.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use tomni_ui::dialogs;
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::keys::{PrivateFormat, PublicFormat};

use crate::agente::passphrase_do_cofre;
use crate::chaves_cofre::{
    Aberta, Destino, Escolhida, Lida, Parte, tipo_da_publica, titulo_no_cofre,
};
use crate::ferramentas::{ChaveLocal, Ferramentas};
use crate::t;

/// Os destinos da importação, como a lista os mostra (no idioma de agora).
pub fn destinos() -> [String; 2] {
    [t!("chaves-destino-cofre"), t!("chaves-destino-arquivo")]
}

/// As partes que o diálogo de exportar oferece (no idioma de agora).
pub fn partes() -> [String; 2] {
    [t!("chaves-parte-privada"), t!("chaves-parte-so-publica")]
}

/// Abre o diálogo de importar: com um arquivo, ou `None` para colar o texto.
pub type AbrirImportar = Rc<dyn Fn(Option<PathBuf>)>;

/// De onde vem a chave que o diálogo de exportar abre.
#[derive(Clone, Debug)]
pub enum Fonte {
    /// Um item "Chave SSH" do cofre (o id do item).
    Item(u64),
    /// Um arquivo de chave.
    Arquivo(PathBuf),
}

/// Abre o diálogo de exportar ou de converter.
pub type AbrirExportar = Rc<dyn Fn(Fonte)>;

/// Abre a confirmação de apagar arquivos de `~/.ssh`.
pub type AbrirApagar = Rc<dyn Fn(Vec<PathBuf>)>;

/// Reabre o diálogo depois de um erro e, quando ele fecha de vez, limpa o que `limpar` pedir.
fn reabrir_em_erro(aberto: Signal<bool>, refazer: &Rc<Cell<bool>>, limpar: impl Fn() + 'static) {
    let refazer = Rc::clone(refazer);
    effect(move || {
        if aberto.get() {
            return;
        }
        if refazer.replace(false) {
            aberto.set(true);
        } else {
            untrack(&limpar);
        }
    });
}

/// "Apagar N arquivos de ~/.ssh?": a pergunta que vem depois de levar chaves para o cofre.
pub fn dialogo_apagar(ferr: &Rc<Ferramentas>) -> (AbrirApagar, impl View) {
    let aberto = signal(false);
    let texto = signal(String::new());
    let alvos: Rc<RefCell<Vec<PathBuf>>> = Rc::default();

    let abrir: AbrirApagar = {
        let alvos = Rc::clone(&alvos);
        Rc::new(move |lista: Vec<PathBuf>| {
            if lista.is_empty() {
                return;
            }
            texto.set(if lista.len() == 1 {
                t!(
                    "chaves-apagar-um-arquivo",
                    arquivo = lista[0].display().to_string()
                )
            } else {
                t!("chaves-apagar-n-arquivos", n = lista.len() as i64)
            });
            *alvos.borrow_mut() = lista;
            aberto.set(true);
        })
    };

    let apagar = {
        let (ferr, alvos) = (Rc::clone(ferr), Rc::clone(&alvos));
        move || {
            let lista = std::mem::take(&mut *alvos.borrow_mut());
            let mut apagados = 0;
            for arquivo in lista {
                match ferr.apagar_chave(&arquivo) {
                    Ok(()) => apagados += 1,
                    Err(erro) => ferr.avisar_erro(erro),
                }
            }
            if apagados > 0 {
                ferr.avisar_sucesso(if apagados == 1 {
                    t!("chaves-arquivo-apagado")
                } else {
                    t!("chaves-n-arquivos-apagados", n = apagados as i64)
                });
            }
        }
    };

    let dialogo = ContentDialog::new(t!("chaves-apagar-de-ssh"), aberto)
        .id("chaves-apagar-dialogo")
        .content(
            Text::new(move || texto.get())
                .id("chaves-apagar-texto")
                .wrap(),
        )
        .primary(t!("comum-apagar"), apagar)
        .close(t!("chaves-manter"));
    (abrir, dialogo)
}

/// "Remover do cofre?": a confirmação de sempre de apagar um item.
pub fn dialogo_remover(ferr: &Rc<Ferramentas>) -> (Rc<dyn Fn(u64)>, impl View) {
    let aberto = signal(false);
    let texto = signal(String::new());
    let alvo: Rc<Cell<Option<u64>>> = Rc::default();

    let abrir: Rc<dyn Fn(u64)> = {
        let (ferr, alvo) = (Rc::clone(ferr), Rc::clone(&alvo));
        Rc::new(move |id: u64| {
            let Some(titulo) = ferr.cofre().agora(|c| c.item(id).map(|i| i.titulo.clone())) else {
                return;
            };
            texto.set(t!(
                "chaves-remover-do-cofre-aviso",
                titulo = titulo.as_str()
            ));
            alvo.set(Some(id));
            aberto.set(true);
        })
    };

    let remover = {
        let (ferr, alvo) = (Rc::clone(ferr), Rc::clone(&alvo));
        move || {
            let Some(id) = alvo.take() else {
                return;
            };
            match ferr.remover_do_cofre(id) {
                Ok(()) => ferr.avisar_sucesso(t!("chaves-removida-do-cofre")),
                Err(erro) => ferr.avisar_erro(erro),
            }
        }
    };

    let dialogo = ContentDialog::new(t!("chaves-remover-do-cofre"), aberto)
        .id("chaves-remover-dialogo")
        .content(
            Text::new(move || texto.get())
                .id("chaves-remover-texto")
                .wrap(),
        )
        .primary(t!("comum-remover"), remover)
        .close(t!("comum-cancelar"));
    (abrir, dialogo)
}

/// Importar uma chave de arquivo ou colada. `apagar` abre a confirmação de apagar o arquivo de
/// `~/.ssh` quando a pessoa pede.
pub fn dialogo_importar(ferr: &Rc<Ferramentas>, apagar: AbrirApagar) -> (AbrirImportar, impl View) {
    let aberto = signal(false);
    let colando = signal(false);
    let texto = signal(String::new());
    let formato = signal(String::new());
    let nome = signal(String::new());
    let passphrase = signal(String::new());
    let guardar = signal(true);
    let destino = signal(destinos()[0].clone());
    let apagar_depois = signal(false);
    let erro = signal(String::new());
    let protegida = signal(false);
    let do_ssh = signal(false);
    let lida: Rc<RefCell<Option<Lida>>> = Rc::default();
    let origem: Rc<RefCell<Option<PathBuf>>> = Rc::default();
    let refazer = Rc::new(Cell::new(false));

    reabrir_em_erro(aberto, &refazer, {
        let (lida, origem) = (Rc::clone(&lida), Rc::clone(&origem));
        move || {
            *lida.borrow_mut() = None;
            *origem.borrow_mut() = None;
            for campo in [texto, formato, nome, passphrase, erro] {
                campo.set(String::new());
            }
        }
    });

    // O texto colado é lido a cada mudança: o formato, o nome e o pedido de passphrase seguem.
    {
        let (lida, ultimo) = (Rc::clone(&lida), RefCell::new(String::new()));
        effect(move || {
            let colado = texto.get();
            untrack(|| {
                if !colando.get_untracked() {
                    return;
                }
                if colado.trim().is_empty() {
                    *lida.borrow_mut() = None;
                    formato.set(t!("chaves-cole-o-texto"));
                    protegida.set(false);
                    return;
                }
                match Lida::ler_texto(&colado) {
                    Ok(nova) => {
                        formato.set(nova.descricao());
                        protegida.set(nova.protegida());
                        let sugerido = nova.titulo_sugerido(None);
                        let atual = nome.get_untracked();
                        if atual.is_empty() || atual == *ultimo.borrow() {
                            nome.set(sugerido.clone());
                        }
                        *ultimo.borrow_mut() = sugerido;
                        *lida.borrow_mut() = Some(nova);
                    }
                    Err(motivo) => {
                        formato.set(motivo);
                        protegida.set(false);
                        *lida.borrow_mut() = None;
                    }
                }
            });
        });
    }

    let abrir: AbrirImportar = {
        let (ferr, lida, origem) = (Rc::clone(ferr), Rc::clone(&lida), Rc::clone(&origem));
        Rc::new(move |arquivo: Option<PathBuf>| {
            destino.set(destinos()[0].clone());
            guardar.set(true);
            apagar_depois.set(false);
            erro.set(String::new());
            passphrase.set(String::new());
            let Some(arquivo) = arquivo else {
                *lida.borrow_mut() = None;
                *origem.borrow_mut() = None;
                nome.set(String::new());
                texto.set(String::new());
                protegida.set(false);
                do_ssh.set(false);
                formato.set(t!("chaves-cole-o-texto"));
                colando.set(true);
                aberto.set(true);
                return;
            };
            let nova = match Lida::ler_arquivo(&arquivo) {
                Ok(nova) => nova,
                Err(motivo) => {
                    ferr.avisar_erro(motivo);
                    return;
                }
            };
            colando.set(false);
            formato.set(nova.descricao());
            nome.set(nova.titulo_sugerido(Some(&arquivo)));
            protegida.set(nova.protegida());
            if nova.protegida() {
                passphrase.set(passphrase_do_cofre(ferr.cofre(), &arquivo).unwrap_or_default());
            }
            do_ssh.set(arquivo.parent() == Some(ferr.pasta_ssh().as_path()));
            *lida.borrow_mut() = Some(nova);
            *origem.borrow_mut() = Some(arquivo);
            aberto.set(true);
        })
    };

    let confirmar = {
        let (ferr, lida, origem, refazer) = (
            Rc::clone(ferr),
            Rc::clone(&lida),
            Rc::clone(&origem),
            Rc::clone(&refazer),
        );
        move || {
            let Some(lida) = lida.borrow().clone() else {
                erro.set(t!("chaves-escolha-arquivo-ou-cole"));
                refazer.set(true);
                return;
            };
            let para = if destino.get_untracked() == destinos()[1] {
                Destino::Arquivo
            } else {
                Destino::Cofre
            };
            let resposta = ferr.importar(
                &lida,
                &nome.get_untracked(),
                &passphrase.get_untracked(),
                guardar.get_untracked(),
                para,
            );
            match resposta {
                Ok(feita) => {
                    ferr.avisar_sucesso(feita.mensagem);
                    let pediu = apagar_depois.get_untracked() && para == Destino::Cofre;
                    if let (true, true, Some(arquivo)) =
                        (pediu, do_ssh.get_untracked(), origem.borrow().clone())
                    {
                        apagar(vec![arquivo]);
                    }
                }
                Err(motivo) => {
                    erro.set(motivo);
                    refazer.set(true);
                }
            }
        }
    };

    let dialogo = ContentDialog::new(
        move || {
            if colando.get() {
                t!("chaves-colar-uma-chave")
            } else {
                t!("chaves-importar-uma-chave")
            }
        },
        aberto,
    )
    .id("chaves-importar-dialogo")
    .content(
        Column::new()
            .gap(8)
            .width(460)
            .child(
                Text::new(move || formato.get())
                    .id("chaves-importar-formato")
                    .secondary()
                    .wrap(),
            )
            .child(
                Edit::bind(texto)
                    .id("chaves-importar-texto")
                    .multiline()
                    .height(120)
                    .placeholder("-----BEGIN OPENSSH PRIVATE KEY-----")
                    .visible(move || colando.get()),
            )
            .child(Text::new(t!("chaves-nome-no-cofre")).caption())
            .child(
                Edit::bind(nome)
                    .id("chaves-importar-nome")
                    .placeholder(t!("chaves-nome-da-chave")),
            )
            .child(
                Edit::bind(passphrase)
                    .id("chaves-importar-passphrase")
                    .password()
                    .placeholder(t!("chaves-passphrase-da-chave"))
                    .visible(move || protegida.get()),
            )
            .child(
                CheckBox::bind(
                    guardar,
                    t!("chaves-guardar-a-passphrase-para-exportar-depois"),
                )
                .id("chaves-importar-guardar")
                .visible(move || protegida.get() && destino.get() == destinos()[0]),
            )
            .child(Text::new(t!("chaves-guardar-em")).caption())
            .child(ComboBox::bind(destino, destinos()).id("chaves-importar-destino"))
            .child(
                CheckBox::bind(apagar_depois, t!("chaves-apagar-o-arquivo-de-ssh-depois"))
                    .id("chaves-importar-apagar")
                    .visible(move || do_ssh.get() && destino.get() == destinos()[0]),
            )
            .child(
                Text::new(move || erro.get())
                    .id("chaves-importar-erro")
                    .caption()
                    .wrap()
                    .visible(move || !erro.get().is_empty()),
            ),
    )
    .primary(t!("comum-importar"), confirmar)
    .close(t!("comum-cancelar"));
    (abrir, dialogo)
}

/// Abre o diálogo de trazer várias chaves: `None` lista as de `~/.ssh`; com arquivos, mostra
/// os que a pessoa escolheu no seletor.
pub type AbrirLote = Rc<dyn Fn(Option<Vec<PathBuf>>)>;

/// Uma chave na lista de "Trazer todas" ou da importação de vários arquivos.
#[derive(Clone)]
struct Linha {
    chave: ChaveLocal,
    /// O id da linha (o nome do arquivo, com um número se outro já tem o mesmo nome).
    id: String,
    /// O tipo e o formato, como a linha mostra.
    descricao: String,
    marcada: Signal<bool>,
    passphrase: Signal<String>,
    /// O título do item do cofre que já tem esta chave.
    no_cofre: Option<String>,
}

/// A chave de um arquivo escolhido, com o que se sabe dela sem a passphrase.
fn chave_do_arquivo(ferr: &Ferramentas, arquivo: PathBuf, lida: &Lida) -> ChaveLocal {
    let fingerprint = lida.fingerprint().unwrap_or_default();
    ChaveLocal {
        nome: arquivo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        tipo: String::new(),
        bits: 0,
        comentario: lida.comentario().to_owned(),
        no_agente: !fingerprint.is_empty() && ferr.agente.tem(&fingerprint),
        fingerprint,
        protegida: lida.protegida(),
        arquivo,
    }
}

/// "Trazer todas de ~/.ssh…" e "Importar…" com vários arquivos: as chaves marcadas entram numa
/// operação só. Das de `~/.ssh` o destino é o cofre; das escolhidas, o cofre ou `~/.ssh`.
pub fn dialogo_lote(ferr: &Rc<Ferramentas>, apagar: AbrirApagar) -> (AbrirLote, impl View) {
    let aberto = signal(false);
    let de_arquivos = signal(false);
    let destino = signal(destinos()[0].clone());
    let apagar_depois = signal(false);
    let tem_de_ssh = signal(false);
    let tem_protegida = signal(false);
    let ilegiveis = signal(String::new());
    let erro = signal(String::new());
    let linhas: Rc<RefCell<Vec<Linha>>> = Rc::default();
    let a_apagar: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    let refazer = Rc::new(Cell::new(false));

    reabrir_em_erro(aberto, &refazer, {
        let (linhas, a_apagar) = (Rc::clone(&linhas), Rc::clone(&a_apagar));
        move || {
            linhas.borrow_mut().clear();
            a_apagar.borrow_mut().clear();
            erro.set(String::new());
            ilegiveis.set(String::new());
        }
    });

    let abrir: AbrirLote = {
        let (ferr, linhas, a_apagar) = (Rc::clone(ferr), Rc::clone(&linhas), Rc::clone(&a_apagar));
        Rc::new(move |arquivos: Option<Vec<PathBuf>>| {
            let mut falhas = Vec::new();
            let chaves: Vec<(ChaveLocal, String)> = match &arquivos {
                None => {
                    if ferr.cofre().agora(|c| c.bloqueado()) {
                        ferr.avisar_erro(t!("chaves-cofre-bloqueado-para-guardar"));
                        return;
                    }
                    let chaves = ferr.listar_chaves();
                    if chaves.is_empty() {
                        ferr.avisar(InfoBarSeverity::Warning, t!("chaves-sem-privadas-em-ssh"));
                        return;
                    }
                    chaves
                        .into_iter()
                        .map(|chave| {
                            let descricao = chave.descricao();
                            (chave, descricao)
                        })
                        .collect()
                }
                Some(arquivos) => arquivos
                    .iter()
                    .filter_map(|arquivo| match Lida::ler_arquivo(arquivo) {
                        Ok(lida) => Some((
                            chave_do_arquivo(&ferr, arquivo.clone(), &lida),
                            lida.descricao(),
                        )),
                        Err(motivo) => {
                            falhas.push(format!("{}: {motivo}", arquivo.display()));
                            None
                        }
                    })
                    .collect(),
            };
            if chaves.is_empty() {
                ferr.avisar_erro(falhas.join("\n"));
                return;
            }
            let pasta_ssh = ferr.pasta_ssh();
            let mut usados: Vec<String> = Vec::new();
            *linhas.borrow_mut() = chaves
                .into_iter()
                .map(|(chave, descricao)| {
                    let no_cofre = titulo_no_cofre(ferr.cofre(), &chave.fingerprint)
                        .filter(|_| !chave.fingerprint.is_empty());
                    let guardada = if chave.protegida {
                        passphrase_do_cofre(ferr.cofre(), &chave.arquivo).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let mut id = format!("chaves-de-ssh-{}", chave.nome);
                    let mut n = 1;
                    while usados.contains(&id) {
                        n += 1;
                        id = format!("chaves-de-ssh-{}-{n}", chave.nome);
                    }
                    usados.push(id.clone());
                    Linha {
                        marcada: signal(no_cofre.is_none()),
                        passphrase: signal(guardada),
                        id,
                        descricao,
                        no_cofre,
                        chave,
                    }
                })
                .collect();
            tem_de_ssh.set(
                linhas
                    .borrow()
                    .iter()
                    .any(|l| l.chave.arquivo.parent() == Some(pasta_ssh.as_path())),
            );
            tem_protegida.set(linhas.borrow().iter().any(|l| l.chave.protegida));
            de_arquivos.set(arquivos.is_some());
            destino.set(destinos()[0].clone());
            a_apagar.borrow_mut().clear();
            apagar_depois.set(false);
            ilegiveis.set(if falhas.is_empty() {
                String::new()
            } else {
                t!("chaves-arquivos-ilegiveis", lista = falhas.join("\n"))
            });
            erro.set(String::new());
            aberto.set(true);
        })
    };

    let no_cofre = move || destino.get() == destinos()[0];
    let confirmar = {
        let (ferr, linhas, a_apagar, refazer) = (
            Rc::clone(ferr),
            Rc::clone(&linhas),
            Rc::clone(&a_apagar),
            Rc::clone(&refazer),
        );
        move || {
            let todas = linhas.borrow().clone();
            let escolhidas: Vec<Escolhida> = todas
                .iter()
                .filter(|l| l.marcada.get_untracked())
                .map(|l| Escolhida {
                    chave: l.chave.clone(),
                    passphrase: l.passphrase.get_untracked(),
                })
                .collect();
            if escolhidas.is_empty() {
                erro.set(t!("chaves-marque-uma-chave"));
                refazer.set(true);
                return;
            }
            let para = if untrack(no_cofre) {
                Destino::Cofre
            } else {
                Destino::Arquivo
            };
            if para == Destino::Cofre && ferr.cofre().agora(|c| c.bloqueado()) {
                erro.set(t!("chaves-cofre-bloqueado-para-guardar"));
                refazer.set(true);
                return;
            }
            let feito = ferr.trazer(&escolhidas, para);
            // As que entraram saem da lista de marcadas: uma nova tentativa só repete as falhas.
            for linha in &todas {
                if feito.trazidas.contains(&linha.chave.arquivo) {
                    linha.marcada.set(false);
                }
            }
            // Só os arquivos de `~/.ssh` que foram para o cofre se apagam.
            if apagar_depois.get_untracked() && para == Destino::Cofre {
                let pasta_ssh = ferr.pasta_ssh();
                a_apagar.borrow_mut().extend(
                    feito
                        .trazidas
                        .iter()
                        .filter(|a| a.parent() == Some(pasta_ssh.as_path()))
                        .cloned(),
                );
            }
            if !feito.trazidas.is_empty() {
                let n = feito.trazidas.len();
                ferr.avisar_sucesso(match (para, n) {
                    (Destino::Cofre, 1) => t!("chaves-uma-guardada"),
                    (Destino::Cofre, _) => t!("chaves-n-guardadas", n = n as i64),
                    (Destino::Arquivo, _) => t!("chaves-n-gravadas-em-ssh", n = n as i64),
                });
            }
            if feito.falhas.is_empty() {
                erro.set(String::new());
                let lista = std::mem::take(&mut *a_apagar.borrow_mut());
                apagar(lista);
            } else {
                erro.set(
                    feito
                        .falhas
                        .iter()
                        .map(|(nome, motivo)| format!("{nome}: {motivo}"))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                refazer.set(true);
            }
        }
    };

    let visao = Rc::clone(&linhas);
    let dialogo = ContentDialog::new(
        move || {
            if de_arquivos.get() {
                t!("chaves-importar-chaves")
            } else {
                t!("chaves-trazer-as-chaves-de-ssh")
            }
        },
        aberto,
    )
    .id("chaves-de-ssh-dialogo")
    .content(
        Column::new()
            .gap(8)
            .width(480)
            .child(
                Text::new(t!("chaves-as-chaves-vao-para-o-cofre"))
                    .secondary()
                    .wrap()
                    .visible(move || !de_arquivos.get()),
            )
            .child(
                Text::new(move || ilegiveis.get())
                    .id("chaves-de-ssh-ilegiveis")
                    .caption()
                    .secondary()
                    .wrap()
                    .visible(move || !ilegiveis.get().is_empty()),
            )
            // As linhas mudam entre uma abertura e outra: nascem com o diálogo.
            .child(Show::when(aberto, move || {
                let mut coluna = Column::new().gap(8);
                for linha in visao.borrow().iter() {
                    let id = linha.id.clone();
                    let detalhe = match &linha.no_cofre {
                        Some(titulo) => {
                            t!(
                                "chaves-ja-esta-no-cofre-como",
                                descricao = linha.descricao.as_str(),
                                titulo = titulo.as_str()
                            )
                        }
                        None => linha.descricao.clone(),
                    };
                    let mut bloco = Column::new()
                        .gap(4)
                        .child(
                            CheckBox::bind(linha.marcada, linha.chave.nome.clone()).id(id.clone()),
                        )
                        .child(
                            Text::new(detalhe)
                                .id(format!("{id}-detalhe"))
                                .caption()
                                .secondary()
                                .wrap(),
                        );
                    if linha.chave.protegida {
                        bloco = bloco.child(
                            Edit::bind(linha.passphrase)
                                .id(format!("{id}-passphrase"))
                                .password()
                                .placeholder(t!("chaves-passphrase-da-chave")),
                        );
                    }
                    coluna = coluna.child(bloco);
                }
                ScrollViewer::new().height(320).child(coluna)
            }))
            .child(
                Text::new(t!("chaves-passphrase-vale-para-as-outras"))
                    .caption()
                    .secondary()
                    .wrap()
                    .visible(move || tem_protegida.get()),
            )
            .child(
                Text::new(t!("chaves-guardar-em"))
                    .caption()
                    .visible(move || de_arquivos.get()),
            )
            .child(
                ComboBox::bind(destino, destinos())
                    .id("chaves-de-ssh-destino")
                    .visible(move || de_arquivos.get()),
            )
            .child(
                CheckBox::bind(apagar_depois, t!("chaves-apagar-os-arquivos-de-ssh-depois"))
                    .id("chaves-de-ssh-apagar")
                    .visible(move || tem_de_ssh.get() && no_cofre()),
            )
            .child(
                Text::new(move || erro.get())
                    .id("chaves-de-ssh-erro")
                    .caption()
                    .wrap()
                    .visible(move || !erro.get().is_empty()),
            ),
    )
    .primary(
        move || {
            if de_arquivos.get() {
                t!("comum-importar")
            } else {
                t!("chaves-trazer")
            }
        },
        confirmar,
    )
    .close(t!("comum-cancelar"));
    (abrir, dialogo)
}

/// O que o diálogo de exportar tem aberto.
struct Estado {
    aberta: RefCell<Option<Rc<Aberta>>>,
    /// O arquivo de onde a chave vem (para pedir a passphrase de novo).
    arquivo: RefCell<Option<PathBuf>>,
}

fn formato_privado(rotulo: &str) -> PrivateFormat {
    PrivateFormat::ALL
        .into_iter()
        .find(|f| f.label() == rotulo)
        .unwrap_or(PrivateFormat::OpenSsh)
}

fn formato_publico(rotulo: &str) -> PublicFormat {
    PublicFormat::ALL
        .into_iter()
        .find(|f| f.label() == rotulo)
        .unwrap_or(PublicFormat::OpenSsh)
}

fn parte_de(rotulo: &str) -> Parte {
    if rotulo == partes()[1] {
        Parte::Publica
    } else {
        Parte::Privada
    }
}

/// Por que o formato não serve para a chave, se não serve.
fn aviso_do_formato(aberta: &Aberta, formato: PrivateFormat) -> Option<String> {
    let privada = aberta.privada.as_ref()?;
    if PrivateFormat::available(privada).contains(&formato) {
        return None;
    }
    let tipo = aberta.publica.algorithm();
    Some(
        if formato == PrivateFormat::PemLegacy && tipo.clone().is_ed25519() {
            t!("chaves-ed25519-sem-pem")
        } else {
            t!("chaves-formato-nao-guarda", tipo = tipo.to_string())
        },
    )
}

/// "Exportar como…" (`chaves-exportar-*`) e "Converter arquivo…" (`chaves-converter-*`): o mesmo
/// diálogo, com o `prefixo` nos ids. Abre a chave, deixa escolher a parte, o formato e a
/// passphrase, e salva num arquivo ou copia o texto.
pub fn dialogo_exportar(
    ferr: &Rc<Ferramentas>,
    prefixo: &'static str,
    titulo: String,
) -> (AbrirExportar, impl View) {
    let id = move |sufixo: &str| format!("{prefixo}-{sufixo}");
    let aberto = signal(false);
    let nome = signal(String::new());
    let descricao = signal(String::new());
    let precisa_atual = signal(false);
    let atual = signal(String::new());
    let tem_chave = signal(false);
    let tem_privada = signal(false);
    let parte = signal(partes()[0].clone());
    let formato_priv = signal(PrivateFormat::OpenSsh.label().to_owned());
    let formato_pub = signal(PublicFormat::OpenSsh.label().to_owned());
    let passphrase = signal(String::new());
    let repetida = signal(String::new());
    let erro = signal(String::new());
    let estado = Rc::new(Estado {
        aberta: RefCell::default(),
        arquivo: RefCell::default(),
    });

    // Fecha de vez: a chave aberta sai da memória e os campos voltam vazios.
    {
        let estado = Rc::clone(&estado);
        effect(move || {
            if aberto.get() {
                return;
            }
            untrack(|| {
                *estado.aberta.borrow_mut() = None;
                *estado.arquivo.borrow_mut() = None;
                tem_chave.set(false);
                tem_privada.set(false);
                for campo in [atual, passphrase, repetida, erro] {
                    campo.set(String::new());
                }
            });
        });
    }

    // Põe a chave aberta no diálogo.
    let mostrar = {
        let estado = Rc::clone(&estado);
        move |aberta: Aberta| {
            let privada = aberta.privada.is_some();
            passphrase.set(aberta.passphrase.clone());
            repetida.set(aberta.passphrase.clone());
            parte.set(partes()[usize::from(!privada)].clone());
            formato_priv.set(PrivateFormat::OpenSsh.label().to_owned());
            formato_pub.set(PublicFormat::OpenSsh.label().to_owned());
            tem_privada.set(privada);
            tem_chave.set(true);
            precisa_atual.set(false);
            *estado.aberta.borrow_mut() = Some(Rc::new(aberta));
        }
    };

    let abrir: AbrirExportar = {
        let (ferr, estado, mostrar) = (Rc::clone(ferr), Rc::clone(&estado), mostrar.clone());
        Rc::new(move |fonte: Fonte| {
            erro.set(String::new());
            atual.set(String::new());
            let (rotulo, aberta) = match &fonte {
                Fonte::Item(item) => {
                    let aberta = ferr.abrir_item(*item);
                    let titulo = ferr
                        .cofre()
                        .agora(|c| c.item(*item).map(|i| i.titulo.clone()))
                        .unwrap_or_default();
                    (t!("chaves-item-no-cofre", titulo = titulo), aberta)
                }
                Fonte::Arquivo(arquivo) => {
                    *estado.arquivo.borrow_mut() = Some(arquivo.clone());
                    (
                        arquivo.display().to_string(),
                        ferr.abrir_arquivo(arquivo, ""),
                    )
                }
            };
            match aberta {
                Ok(aberta) => {
                    nome.set(aberta.nome.clone());
                    descricao.set(format!("{rotulo} · {}", tipo_da_publica(&aberta.publica)));
                    mostrar(aberta);
                }
                Err(motivo) => {
                    // Cifrada e sem a passphrase no cofre: o diálogo pede.
                    let protegida = match &fonte {
                        Fonte::Arquivo(arquivo) => {
                            Lida::ler_arquivo(arquivo).is_ok_and(|l| l.protegida())
                        }
                        Fonte::Item(_) => false,
                    };
                    if !protegida {
                        ferr.avisar_erro(motivo);
                        return;
                    }
                    let nome_do_arquivo = match &fonte {
                        Fonte::Arquivo(arquivo) => crate::ferramentas::nome_do(arquivo),
                        Fonte::Item(_) => String::new(),
                    };
                    nome.set(nome_do_arquivo);
                    descricao.set(t!("chaves-rotulo-com-passphrase", rotulo = rotulo.as_str()));
                    tem_chave.set(false);
                    tem_privada.set(true);
                    precisa_atual.set(true);
                    *estado.aberta.borrow_mut() = None;
                }
            }
            aberto.set(true);
        })
    };

    // "Abrir a chave": a passphrase digitada.
    let abrir_a_chave = {
        let (ferr, estado, mostrar) = (Rc::clone(ferr), Rc::clone(&estado), mostrar);
        move || {
            let Some(arquivo) = estado.arquivo.borrow().clone() else {
                return;
            };
            match ferr.abrir_arquivo(&arquivo, &atual.get_untracked()) {
                Ok(aberta) => {
                    erro.set(String::new());
                    atual.set(String::new());
                    mostrar(aberta);
                }
                Err(motivo) => erro.set(motivo),
            }
        }
    };

    // O texto da exportação, pronto para salvar ou copiar.
    let texto_pronto = {
        let estado = Rc::clone(&estado);
        move || -> Result<(Rc<Aberta>, Parte, PrivateFormat, PublicFormat, String), String> {
            let Some(aberta) = estado.aberta.borrow().clone() else {
                return Err(t!("chaves-abra-a-chave-primeiro"));
            };
            let parte = parte_de(&parte.get_untracked());
            let privado = formato_privado(&formato_priv.get_untracked());
            let publico = formato_publico(&formato_pub.get_untracked());
            let texto = aberta.exportar(
                parte,
                privado,
                publico,
                &passphrase.get_untracked(),
                &repetida.get_untracked(),
            )?;
            Ok((aberta, parte, privado, publico, texto))
        }
    };

    let salvar = {
        let (ferr, texto_pronto) = (Rc::clone(ferr), texto_pronto.clone());
        move || {
            let (aberta, parte, privado, publico, texto) = match texto_pronto() {
                Ok(pronto) => pronto,
                Err(motivo) => {
                    erro.set(motivo);
                    return;
                }
            };
            erro.set(String::new());
            let sugerido = aberta.arquivo_sugerido(parte, privado, publico);
            let ferr = Rc::clone(&ferr);
            dialogs::save_file()
                .remember("chaves-exportar")
                .title(t!("chaves-salvar-a-chave"))
                .file_name(sugerido)
                .show(move |caminho: Option<PathBuf>| {
                    let Some(caminho) = caminho else {
                        return;
                    };
                    match aberta.gravar(&caminho, parte, &texto) {
                        Ok(()) => {
                            ferr.avisar_sucesso(t!(
                                "chaves-gravada-em",
                                caminho = caminho.display().to_string()
                            ));
                            ferr.atualizar_pasta();
                            aberto.set(false);
                        }
                        Err(motivo) => erro.set(motivo),
                    }
                });
        }
    };

    let copiar = {
        let ferr = Rc::clone(ferr);
        move || {
            let (_, parte, _, _, texto) = match texto_pronto() {
                Ok(pronto) => pronto,
                Err(motivo) => {
                    erro.set(motivo);
                    return;
                }
            };
            if parte == Parte::Privada {
                ferr.cofre().copiar_segredo(texto);
            } else {
                tomni_ui::clipboard::write(texto);
            }
            ferr.avisar_sucesso(t!("chaves-copiada"));
            aberto.set(false);
        }
    };

    // O aviso do formato que não serve para a chave, ou nada.
    let aviso = {
        let estado = Rc::clone(&estado);
        move || -> String {
            let (rotulo, parte) = (formato_priv.get(), parte.get());
            let _ = tem_chave.get();
            if parte_de(&parte) != Parte::Privada {
                return String::new();
            }
            estado
                .aberta
                .borrow()
                .as_ref()
                .and_then(|a| aviso_do_formato(a, formato_privado(&rotulo)))
                .unwrap_or_default()
        }
    };
    let aviso_a = aviso.clone();
    let sem_aviso = move || tem_chave.get() && aviso_a().is_empty();

    let cifra = move || {
        parte_de(&parte.get()) == Parte::Privada && formato_privado(&formato_priv.get()).encrypts()
    };

    // Os formatos da pública dependem do tipo da chave (o PKCS#1 é só do RSA): a lista nasce
    // junto com a chave aberta.
    let estado_pub = Rc::clone(&estado);

    let dialogo = ContentDialog::new(move || format!("{titulo}: {}", nome.get()), aberto)
        .id(id("dialogo"))
        .content(
            Column::new()
                .gap(8)
                .width(480)
                .child(
                    Text::new(move || descricao.get())
                        .id(id("arquivo"))
                        .secondary()
                        .wrap(),
                )
                .child(
                    Edit::bind(atual)
                        .id(id("atual"))
                        .password()
                        .placeholder(t!("chaves-passphrase-da-chave"))
                        .visible(move || precisa_atual.get()),
                )
                .child(
                    Button::new(t!("chaves-abrir-a-chave"))
                        .id(id("abrir"))
                        .visible(move || precisa_atual.get())
                        .on_click(abrir_a_chave),
                )
                .child(
                    RadioButtons::bind(parte, partes())
                        .id(id("parte"))
                        .visible(move || tem_chave.get() && tem_privada.get()),
                )
                .child(
                    Text::new(t!("chaves-formato"))
                        .caption()
                        .visible(move || tem_chave.get()),
                )
                .child(Show::when(
                    move || {
                        aberto.get() && tem_chave.get() && parte_de(&parte.get()) == Parte::Privada
                    },
                    move || {
                        ComboBox::bind(
                            formato_priv,
                            PrivateFormat::ALL.map(|f| f.label().to_owned()),
                        )
                        .id(id("formato"))
                    },
                ))
                .child(Show::when(
                    move || {
                        aberto.get() && tem_chave.get() && parte_de(&parte.get()) == Parte::Publica
                    },
                    move || {
                        let rotulos: Vec<String> = estado_pub
                            .aberta
                            .borrow()
                            .as_ref()
                            .map(|a| a.formatos_publicos())
                            .unwrap_or_default()
                            .into_iter()
                            .map(|f| f.label().to_owned())
                            .collect();
                        ComboBox::bind(formato_pub, rotulos).id(id("formato"))
                    },
                ))
                .child(
                    Edit::bind(passphrase)
                        .id(id("passphrase"))
                        .password()
                        .placeholder(t!("chaves-passphrase-do-arquivo-vazio-sem-passphrase"))
                        .visible(move || tem_chave.get() && cifra()),
                )
                .child(
                    Edit::bind(repetida)
                        .id(id("repetir"))
                        .password()
                        .placeholder(t!("chaves-repetir-a-passphrase"))
                        .visible(move || tem_chave.get() && cifra()),
                )
                .child(Text::new(aviso).id(id("aviso")).caption().wrap().visible({
                    let estado = Rc::clone(&estado);
                    move || {
                        let _ = (tem_chave.get(), formato_priv.get(), parte.get());
                        estado.aberta.borrow().as_ref().is_some_and(|a| {
                            parte_de(&parte.get()) == Parte::Privada
                                && aviso_do_formato(a, formato_privado(&formato_priv.get()))
                                    .is_some()
                        })
                    }
                }))
                .child(
                    Text::new(move || erro.get())
                        .id(id("erro"))
                        .caption()
                        .wrap()
                        .visible(move || !erro.get().is_empty()),
                )
                .child(
                    Row::new()
                        .gap(8)
                        .child(
                            Button::new(t!("comum-salvar-2"))
                                .id(id("salvar"))
                                .accent()
                                .icon(Symbol::Save)
                                .disabled({
                                    let sem_aviso = sem_aviso.clone();
                                    move || !sem_aviso()
                                })
                                .on_click(salvar),
                        )
                        .child(
                            Button::new(t!("comum-copiar"))
                                .id(id("copiar"))
                                .icon(Symbol::Copy)
                                .disabled(move || !sem_aviso())
                                .on_click(copiar),
                        ),
                ),
        )
        .close(t!("comum-fechar"));
    (abrir, dialogo)
}
