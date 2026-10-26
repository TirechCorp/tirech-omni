//! A página "Chaves SSH": gera uma chave nova e lista as de `~/.ssh`. A geração roda numa thread
//! (o RSA 4096 leva segundos); o resultado volta por um contador, como a fila das chaves do
//! servidor. A pessoa decide o que fazer com a chave: o app só impede o que o OpenSSH atual
//! recusa (DSA e RSA abaixo de 3072 bits, que `KeyType` não oferece).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::clipboard;
use tomni_ui::dialogs;
use tomni_ui::prelude::*;
use tomni_ui::terminal::ssh::keys::{
    self, KdfLevel, KeyGen, KeyType, default_comment, public_path,
};
use tomni_ui::terminal::{Loopback, Scheme, Terminal};

use crate::agente::{acordar, ler_contador};
use crate::chaves_cofre::tipo_legivel;
use crate::ferramentas::{ChaveLocal, Ferramentas, abrir_pasta, motivo, nome_do};
use crate::tela_chaves_dialogos::{
    AbrirExportar, AbrirImportar, Fonte, destinos, dialogo_apagar, dialogo_exportar,
    dialogo_importar, dialogo_lote, dialogo_remover,
};
use crate::{t, tk};

/// As proteções da passphrase: o rótulo e as rodadas do `bcrypt-pbkdf`.
const PROTECOES: [(&str, KdfLevel); 3] = [
    (tk!("chaves-protecao-padrao"), KdfLevel::Standard),
    (tk!("chaves-protecao-alta"), KdfLevel::High),
    (tk!("chaves-protecao-muito-alta"), KdfLevel::VeryHigh),
];

/// O rótulo da proteção `i` de [`PROTECOES`], no idioma de agora.
fn rotulo_da_protecao(i: usize) -> String {
    t!(PROTECOES[i].0)
}

/// O que a página pede à casca.
#[derive(Clone)]
pub struct AcoesChaves {
    /// O nome da conexão SSH escolhida nos Terminais, se há. Lê signals: dentro de uma view, a
    /// refaz quando a escolha muda.
    pub conexao: Rc<dyn Fn() -> Option<String>>,
    /// Passa o arquivo para a conexão escolhida.
    pub usar: Rc<dyn Fn(&Path)>,
    /// Passa a chave do cofre (o id do item) para a conexão escolhida: a linha "Chave do cofre".
    pub usar_cofre: Rc<dyn Fn(u64)>,
    /// Abre o diálogo de copiar a chave pública para o servidor, com esta chave escolhida, se
    /// vier.
    pub servidor: Rc<dyn Fn(Option<DeOnde>)>,
    /// Leva ao item do cofre (a página Cofre de senhas, com ele aberto).
    pub cofre_item: Rc<dyn Fn(u64)>,
    /// Abre o diálogo de desbloquear o cofre.
    pub destravar: Rc<dyn Fn()>,
    /// Pedido de fora: pôr o id de um item de chave aqui abre o "Exportar como…" dele. A página
    /// zera o signal ao atender.
    pub exportar_item: Signal<Option<u64>>,
}

/// A chave que o diálogo "Copiar a chave para o servidor" já traz escolhida.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeOnde {
    /// O arquivo da chave privada (a pública é o `.pub` ao lado).
    Arquivo(PathBuf),
    /// A chave do cofre com esta impressão digital.
    Cofre(String),
}

/// Onde a chave gerada ficou.
#[derive(Clone, Debug, PartialEq)]
enum Onde {
    /// Um arquivo OpenSSH (a pública vai ao lado, com `.pub`).
    Arquivo(PathBuf),
    /// Um item "Chave SSH" do cofre (o id e o título que ficaram).
    Cofre { id: u64, titulo: String },
}

/// A chave que acabou de ser gerada.
#[derive(Clone, Debug, PartialEq)]
struct Resultado {
    onde: Onde,
    fingerprint: String,
    randomart: String,
    publica: String,
}

/// O que a pessoa pediu ao clicar em "Gerar", para a thread e para quando ela termina.
enum Para {
    /// Grava o arquivo, com a passphrase (vazia: sem) e, se `salvar`, a guarda no cofre.
    Arquivo {
        caminho: PathBuf,
        senha: String,
        salvar: bool,
    },
    /// Guarda a chave, sem passphrase, num item do cofre com este título.
    Cofre { titulo: String },
}

/// O que a thread devolve: a chave gerada e o que fazer com ela.
struct Pronta {
    resultado: Resultado,
    privada: ssh_key::PrivateKey,
    comentario: String,
    para: Para,
}

/// A função que abre o diálogo de trocar a passphrase de uma chave.
type AbrirTroca = Rc<dyn Fn(&ChaveLocal)>;

/// O que as linhas das listas abrem.
#[derive(Clone)]
struct Abridores {
    trocar: AbrirTroca,
    importar: AbrirImportar,
    exportar: AbrirExportar,
    remover: Rc<dyn Fn(u64)>,
}

/// Uma chave do cofre como a lista a mostra.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LinhaCofre {
    id: u64,
    titulo: String,
    tipo: String,
    comentario: String,
    fingerprint: String,
    publica: String,
    certificado: bool,
    token: bool,
    confirmar: bool,
    /// O item manda a chave para o agente.
    no_agente: bool,
    /// A chave está no agente agora.
    carregada: bool,
    tem_privada: bool,
}

impl LinhaCofre {
    /// O que a lista usa para reconhecer a linha: muda com tudo que a linha mostra.
    fn chave(&self) -> String {
        format!("{self:?}")
    }
}

/// As chaves do cofre, em ordem de título.
fn linhas_do_cofre(ferr: &Ferramentas) -> Vec<LinhaCofre> {
    let itens: Vec<_> = ferr.cofre().ler(|c| {
        c.chaves_ssh()
            .into_iter()
            .filter_map(|i| i.chave.clone().map(|k| (i.id, i.titulo.clone(), k)))
            .collect()
    });
    itens
        .into_iter()
        .map(|(id, titulo, chave)| {
            let (tipo, comentario) = match keys::parse_public(&chave.publica) {
                Ok((publica, comentario)) => {
                    let (tipo, bits) = keys::kind_and_bits(&publica);
                    (tipo_legivel(&tipo, bits), comentario)
                }
                Err(_) => (String::new(), String::new()),
            };
            LinhaCofre {
                id,
                titulo,
                tipo,
                comentario,
                token: chave.e_token(),
                certificado: !chave.certificado.trim().is_empty(),
                confirmar: chave.confirmar,
                no_agente: chave.no_agente,
                carregada: ferr.agente.tem(&chave.fingerprint),
                tem_privada: !chave.privada.trim().is_empty(),
                fingerprint: chave.fingerprint,
                publica: chave.publica,
            }
        })
        .collect()
}

/// Escolhe um arquivo de chave (começando em `~/.ssh`) e chama `depois` com ele.
fn escolher_arquivo(
    ferr: &Rc<Ferramentas>,
    titulo: String,
    depois: impl Fn(PathBuf) + Clone + 'static,
) -> impl Fn() + 'static {
    let ferr = Rc::clone(ferr);
    move || {
        let pasta = ferr.pasta_ssh();
        let mut seletor = dialogs::open_file()
            .remember("chaves")
            .title(titulo.as_str());
        if pasta.is_dir() {
            seletor = seletor.directory(pasta);
        }
        let depois = depois.clone();
        seletor.show(move |caminho: Option<PathBuf>| {
            if let Some(caminho) = caminho {
                depois(caminho);
            }
        });
    }
}

/// Escolhe um ou mais arquivos de chave para importar, na pasta da última importação (ou em
/// `~/.ssh`, na primeira vez), e chama `depois` com eles.
fn escolher_arquivos(
    ferr: &Rc<Ferramentas>,
    depois: impl Fn(Vec<PathBuf>) + Clone + 'static,
) -> impl Fn() + 'static {
    let ferr = Rc::clone(ferr);
    move || {
        let pasta = ferr.pasta_ssh();
        let mut seletor = dialogs::open_files()
            .remember("chaves")
            .title(t!("chaves-importar-chaves"));
        if pasta.is_dir() {
            seletor = seletor.directory(pasta);
        }
        seletor.show_many(depois.clone());
    }
}

/// O que a thread devolve.
type Gerada = Result<Pronta, String>;

/// O nome que o item do cofre leva por padrão: o do arquivo padrão do tipo (`id_ed25519`).
fn nome_padrao_no_cofre(tipo: &str) -> String {
    tipo_de(tipo).file_name().to_owned()
}

/// Se o destino escolhido na lista é o cofre (tudo que não for o arquivo, como na importação).
fn destino_e_cofre(destino: &str) -> bool {
    destino != destinos()[1]
}

/// O quadro do randomart como o terminal o entende: limpa a tela, esconde o cursor e usa CR LF.
fn quadro(randomart: &str) -> Vec<u8> {
    let mut bytes = b"\x1b[?25l\x1b[2J\x1b[H".to_vec();
    bytes.extend_from_slice(randomart.trim_end().replace('\n', "\r\n").as_bytes());
    bytes
}

/// O tipo de chave pelo rótulo da lista.
fn tipo_de(rotulo: &str) -> KeyType {
    KeyType::ALL
        .into_iter()
        .find(|t| t.label() == rotulo)
        .unwrap_or(KeyType::Ed25519)
}

/// O rótulo da proteção de `rodadas` (`agente.rodadas_padrao`); o que não é 64 nem 100 vale o
/// padrão.
fn protecao_das_rodadas(rodadas: u32) -> String {
    match rodadas {
        64 => rotulo_da_protecao(1),
        100 => rotulo_da_protecao(2),
        _ => rotulo_da_protecao(0),
    }
}

/// As rodadas pelo rótulo da lista.
fn protecao_de(rotulo: &str) -> KdfLevel {
    PROTECOES
        .iter()
        .enumerate()
        .find(|(i, _)| rotulo_da_protecao(*i) == rotulo)
        .map_or(KdfLevel::Standard, |(_, (_, nivel))| *nivel)
}

/// A página.
pub fn pagina_chaves(ferr: Rc<Ferramentas>, acoes: AcoesChaves) -> impl View {
    let pasta = Rc::clone(&ferr);
    let caminho_padrao = move |tipo: &str| pasta.pasta_ssh().join(tipo_de(tipo).file_name());

    let padrao = ferr.cfg();
    let tipo = signal(padrao.tipo_padrao.nome().to_owned());
    let protecao = signal(protecao_das_rodadas(padrao.rodadas_padrao));
    let passphrase = signal(String::new());
    let repetida = signal(String::new());
    let salvar_no_cofre = signal(true);
    let comentario = signal(default_comment());
    let arquivo = signal(caminho_padrao(&tipo.get_untracked()).display().to_string());
    let destino = signal(destinos()[0].clone());
    let nome_no_cofre = signal(nome_padrao_no_cofre(&tipo.get_untracked()));
    let no_agente = signal(true);
    let gerando = signal(false);
    let erro = signal(String::new());
    let resultado: Signal<Option<Resultado>> = signal(None);
    let pronto = signal(0u64);
    let gerada: Arc<Mutex<Option<Gerada>>> = Arc::default();

    // O caminho e o nome no cofre acompanham o tipo enquanto a pessoa não os trocou à mão.
    {
        let ultimo = RefCell::new(arquivo.get_untracked());
        let ultimo_nome = RefCell::new(nome_no_cofre.get_untracked());
        let caminho_padrao = caminho_padrao.clone();
        effect(move || {
            let tipo_atual = tipo.get();
            let novo = caminho_padrao(&tipo_atual).display().to_string();
            let novo_nome = nome_padrao_no_cofre(&tipo_atual);
            untrack(|| {
                let atual = arquivo.get_untracked();
                if atual.is_empty() || atual == *ultimo.borrow() {
                    arquivo.set(novo.clone());
                }
                *ultimo.borrow_mut() = novo;
                let atual = nome_no_cofre.get_untracked();
                if atual.is_empty() || atual == *ultimo_nome.borrow() {
                    nome_no_cofre.set(novo_nome.clone());
                }
                *ultimo_nome.borrow_mut() = novo_nome;
            });
        });
    }

    // A thread terminou: guarda (no cofre, para esse destino), põe no agente e mostra.
    {
        let (ferr, gerada) = (Rc::clone(&ferr), Arc::clone(&gerada));
        effect(move || {
            let _ = ler_contador(pronto);
            let Some(saida) = gerada.lock().unwrap_or_else(PoisonError::into_inner).take() else {
                return;
            };
            untrack(|| {
                gerando.set(false);
                let Pronta {
                    resultado: mut nova,
                    privada,
                    comentario: comentario_da_chave,
                    para,
                } = match saida {
                    Ok(pronta) => pronta,
                    Err(mensagem) => {
                        erro.set(mensagem);
                        return;
                    }
                };
                match para {
                    Para::Arquivo {
                        caminho,
                        senha,
                        salvar,
                    } => {
                        if no_agente.get_untracked() {
                            if ferr.agente.rodando() {
                                if let Err(e) =
                                    ferr.carregar_gerada(&caminho, privada, &comentario_da_chave)
                                {
                                    erro.set(t!(
                                        "chaves-gerada-fora-do-agente",
                                        motivo = motivo(&e)
                                    ));
                                }
                            } else {
                                erro.set(t!("chaves-gerada-agente-desligado"));
                            }
                        }
                        if salvar && !senha.is_empty() {
                            ferr.guardar_no_cofre(&caminho, &senha);
                        }
                        ferr.atualizar_pasta();
                    }
                    Para::Cofre { titulo } => {
                        match ferr.guardar_gerada(&privada, &titulo, no_agente.get_untracked()) {
                            Ok((id, titulo, aviso)) => {
                                if let Some(aviso) = aviso {
                                    erro.set(aviso);
                                }
                                nova.onde = Onde::Cofre { id, titulo };
                            }
                            Err(mensagem) => {
                                erro.set(mensagem);
                                return;
                            }
                        }
                    }
                }
                passphrase.set(String::new());
                repetida.set(String::new());
                resultado.set(Some(nova));
            });
        });
    }

    let gerar = {
        let (ferr, gerada) = (Rc::clone(&ferr), Arc::clone(&gerada));
        move || {
            if gerando.get_untracked() {
                return;
            }
            let para = if destino_e_cofre(&destino.get_untracked()) {
                // Recusa antes de gerar: uma chave sem onde ficar se perderia.
                if ferr.cofre().agora(|c| c.bloqueado()) {
                    erro.set(t!("chave-cofre-bloqueado-guardar"));
                    return;
                }
                Para::Cofre {
                    titulo: nome_no_cofre.get_untracked().trim().to_owned(),
                }
            } else {
                let senha = passphrase.get_untracked();
                if senha != repetida.get_untracked() {
                    erro.set(t!("chaves-passphrases-nao-conferem"));
                    return;
                }
                let caminho = arquivo.get_untracked().trim().to_owned();
                if caminho.is_empty() {
                    erro.set(t!("agente-escolha-o-arquivo"));
                    return;
                }
                Para::Arquivo {
                    caminho: PathBuf::from(caminho),
                    senha,
                    salvar: salvar_no_cofre.get_untracked(),
                }
            };
            erro.set(String::new());
            gerando.set(true);
            let tipo = tipo_de(&tipo.get_untracked());
            let nivel = protecao_de(&protecao.get_untracked());
            let comentario = comentario.get_untracked();
            let gerada = Arc::clone(&gerada);
            std::thread::spawn(move || {
                // O cofre já cifra a chave: no destino Cofre ela nasce sem passphrase.
                let mut gerador = KeyGen::new(tipo).comment(comentario).kdf(nivel);
                if let Para::Arquivo { senha, .. } = &para
                    && !senha.is_empty()
                {
                    gerador = gerador.passphrase(senha.clone());
                }
                let saida = gerador
                    .generate()
                    .and_then(|chave| match &para {
                        Para::Arquivo { caminho, .. } => {
                            chave.write(caminho, false).map(|()| chave)
                        }
                        Para::Cofre { .. } => Ok(chave),
                    })
                    .map(|chave| {
                        let onde = match &para {
                            Para::Arquivo { caminho, .. } => Onde::Arquivo(caminho.clone()),
                            Para::Cofre { titulo } => Onde::Cofre {
                                id: 0,
                                titulo: titulo.clone(),
                            },
                        };
                        Pronta {
                            resultado: Resultado {
                                onde,
                                fingerprint: chave.fingerprint(),
                                randomart: chave.randomart(),
                                publica: chave.public_openssh(),
                            },
                            privada: chave.private_key().clone(),
                            comentario: chave.comment(),
                            para,
                        }
                    })
                    .map_err(|e| motivo(&e));
                *gerada.lock().unwrap_or_else(PoisonError::into_inner) = Some(saida);
                acordar(pronto);
            });
        }
    };

    let para_cofre = move || destino_e_cofre(&destino.get());
    let para_arquivo = move || !destino_e_cofre(&destino.get());

    let formulario = PropertyGrid::new()
        .id("gerar-chave-props")
        .height(400)
        .category(t!("chaves-nova-chave"))
        .choice(
            t!("comum-tipo"),
            move || tipo,
            KeyType::ALL.map(KeyType::label),
        )
        .default(KeyType::Ed25519.label())
        .description(t!("chaves-ed25519-e-o-padrao-do-openssh"))
        .choice(t!("chaves-destino"), move || destino, destinos())
        .default(destinos()[0].clone())
        .description(t!("chaves-destino-da-nova-chave"))
        .choice(
            t!("chaves-protecao"),
            move || protecao,
            [0, 1, 2].map(rotulo_da_protecao),
        )
        .default(rotulo_da_protecao(0))
        .description(t!("chaves-mais-rodadas-mais-lento-para-desbloquear"))
        .disabled(para_cofre)
        .password("Passphrase", move || passphrase)
        .description(t!("chaves-cifra-o-arquivo-da-chave-privada"))
        .disabled(para_cofre)
        .password(t!("chaves-repetir"), move || repetida)
        .disabled(para_cofre)
        .check(t!("comum-salvar-a-passphrase-no-cofre"), move || {
            salvar_no_cofre
        })
        .default(true)
        .description(t!("chaves-o-app-a-usa-sozinho-ao"))
        .disabled(para_cofre)
        .text(t!("chaves-comentario"), move || comentario)
        .description(t!("chaves-aparece-na-lista-do-agente-e"))
        .text(t!("chaves-nome-no-cofre"), move || nome_no_cofre)
        .description(t!("chaves-titulo-do-item-no-cofre"))
        .disabled(para_arquivo)
        .file(t!("comum-arquivo"), move || arquivo)
        .description(t!("chaves-onde-a-chave-privada-e-gravada"))
        .disabled(para_cofre)
        .check(t!("chaves-carregar-no-agente"), move || no_agente)
        .default(true)
        .description(t!("chaves-poe-a-chave-no-agente-embutido"));

    let acoes_resultado = acoes.clone();
    let bloco_resultado = Show::when(
        move || resultado.with(|r| r.is_some()),
        move || {
            let Some(r) = resultado.get_untracked() else {
                return Column::new();
            };
            let eco = Loopback::new();
            eco.handle().feed(quadro(&r.randomart));
            let publica = r.publica.clone();
            let (usar, usar_cofre, servidor) = (
                Rc::clone(&acoes_resultado.usar),
                Rc::clone(&acoes_resultado.usar_cofre),
                Rc::clone(&acoes_resultado.servidor),
            );
            let (onde, conexao) = (r.onde.clone(), Rc::clone(&acoes_resultado.conexao));
            let (onde_servidor, onde_usar) = (onde.clone(), onde.clone());
            let fingerprint_servidor = r.fingerprint.clone();
            let onde_texto = match &onde {
                Onde::Arquivo(arquivo) => t!(
                    "chaves-arquivo-gerado",
                    caminho = arquivo.display().to_string()
                ),
                Onde::Cofre { titulo, .. } => {
                    t!("chave-guardada-no-cofre", titulo = titulo.as_str())
                }
            };
            let conexao_visivel = Rc::clone(&conexao);
            Column::new()
                .gap(8)
                .child(Text::new(t!("chaves-chave-gerada")).subtitle())
                .child(Text::new(onde_texto).id("gerar-chave-resultado").wrap())
                .child(
                    Text::new(t!(
                        "chaves-impressao-digital",
                        valor = r.fingerprint.as_str()
                    ))
                    .id("gerar-chave-fingerprint")
                    .wrap(),
                )
                .child(
                    Row::new().child(
                        Terminal::new(eco)
                            .id("gerar-chave-randomart")
                            .cols_rows(19, 11)
                            .scheme(Scheme::Theme),
                    ),
                )
                .child(
                    Text::new(r.publica.clone())
                        .id("gerar-chave-publica")
                        .caption()
                        .wrap(),
                )
                .child(
                    Row::new()
                        .gap(8)
                        .child(
                            Button::new(t!("comum-copiar"))
                                .id("gerar-chave-copiar")
                                .icon(Symbol::Copy)
                                .on_click(move || clipboard::write(publica.clone())),
                        )
                        .child(
                            Button::new(t!("chaves-copiar-para-o-servidor"))
                                .id("gerar-chave-servidor")
                                .icon(Symbol::Send)
                                .on_click(move || {
                                    servidor(Some(match &onde_servidor {
                                        Onde::Arquivo(arquivo) => DeOnde::Arquivo(arquivo.clone()),
                                        Onde::Cofre { .. } => {
                                            DeOnde::Cofre(fingerprint_servidor.clone())
                                        }
                                    }))
                                }),
                        )
                        .child(
                            Button::new(move || match conexao() {
                                Some(nome) => t!("chaves-usar-na-conexao-nome", nome = nome),
                                None => t!("chaves-usar-na-conexao"),
                            })
                            .id("gerar-chave-usar")
                            .icon(Symbol::Link)
                            .visible(move || conexao_visivel().is_some())
                            .on_click(move || match &onde_usar {
                                Onde::Arquivo(arquivo) => usar(arquivo),
                                Onde::Cofre { id, .. } => usar_cofre(*id),
                            }),
                        ),
                )
        },
    );

    let ferr_lista = Rc::clone(&ferr);
    let acoes_lista = acoes.clone();
    let (trocar, view_trocar) = dialogo_trocar(&ferr);
    let (apagar_arquivos, view_apagar) = dialogo_apagar(&ferr);
    let (importar, view_importar) = dialogo_importar(&ferr, Rc::clone(&apagar_arquivos));
    let (trazer, view_trazer) = dialogo_lote(&ferr, apagar_arquivos);
    let (exportar, view_exportar) =
        dialogo_exportar(&ferr, "chaves-exportar", t!("chaves-exportar-titulo"));
    let (converter, view_converter) =
        dialogo_exportar(&ferr, "chaves-converter", t!("chaves-converter-titulo"));
    let (remover, view_remover) = dialogo_remover(&ferr);
    {
        let (pedido, exportar) = (acoes.exportar_item, Rc::clone(&exportar));
        effect(move || {
            if let Some(id) = pedido.get() {
                untrack(|| {
                    pedido.set(None);
                    exportar(Fonte::Item(id));
                });
            }
        });
    }
    let abridores = Abridores {
        trocar,
        importar: Rc::clone(&importar),
        exportar,
        remover,
    };

    let lista = For::each(
        move || {
            let _ = ferr_lista.pasta_mudou.get();
            let _ = ferr_lista.agente.versao();
            ferr_lista.listar_chaves()
        },
        ChaveLocal::chave,
        {
            let (ferr, acoes, abridores) =
                (Rc::clone(&ferr), acoes_lista.clone(), abridores.clone());
            move |c: ChaveLocal| linha_da_chave(&c, &ferr, &acoes, &abridores)
        },
    );

    let lista_cofre = {
        let (ferr_fonte, ferr, acoes, abridores) = (
            Rc::clone(&ferr),
            Rc::clone(&ferr),
            acoes.clone(),
            abridores.clone(),
        );
        For::each(
            move || {
                let _ = ferr_fonte.agente.versao();
                linhas_do_cofre(&ferr_fonte)
            },
            LinhaCofre::chave,
            move |l: LinhaCofre| linha_do_cofre(&l, &ferr, &acoes, &abridores),
        )
    };
    let cofre_bloqueado = {
        let ferr = Rc::clone(&ferr);
        move || ferr.cofre().ler(|c| c.bloqueado())
    };
    let aviso_do_cofre = {
        let (bloqueado, destravar) = (cofre_bloqueado.clone(), Rc::clone(&acoes.destravar));
        Show::when(bloqueado, move || {
            let destravar = Rc::clone(&destravar);
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Text::new(t!("chaves-o-cofre-esta-bloqueado-destrave-o"))
                        .id("chaves-cofre-bloqueado")
                        .secondary(),
                )
                .child(
                    Button::new(t!("chaves-destravar"))
                        .id("chaves-cofre-destravar")
                        .icon(Symbol::LockOpen)
                        .on_click(move || destravar()),
                )
        })
    };
    let cofre_vazio = {
        let (ferr, bloqueado) = (Rc::clone(&ferr), cofre_bloqueado);
        Show::when(
            move || !bloqueado() && ferr.cofre().ler(|c| c.chaves_ssh().is_empty()),
            || {
                Text::new(t!("chaves-nenhuma-chave-no-cofre-importe-uma"))
                    .id("chaves-cofre-vazio")
                    .secondary()
            },
        )
    };
    let vazio = {
        let ferr = Rc::clone(&ferr);
        Show::when(
            move || {
                let _ = ferr.pasta_mudou.get();
                ferr.listar_chaves().is_empty()
            },
            || {
                Text::new(t!("chaves-nenhuma-chave-na-pasta-gere-uma"))
                    .id("chaves-vazio")
                    .secondary()
            },
        )
    };

    let abrir = {
        let ferr = Rc::clone(&ferr);
        move || {
            let pasta_ssh = ferr.pasta_ssh();
            if let Err(e) = abrir_pasta(&pasta_ssh) {
                ferr.avisar_erro(t!(
                    "chaves-nao-consegui-abrir",
                    caminho = pasta_ssh.display().to_string(),
                    motivo = e.to_string()
                ));
            }
        }
    };
    let atualizar = {
        let ferr = Rc::clone(&ferr);
        move || ferr.atualizar_pasta()
    };

    let (importar_arquivo, converter_arquivo) = (
        escolher_arquivos(&ferr, {
            let (importar, trazer) = (Rc::clone(&importar), Rc::clone(&trazer));
            move |mut arquivos| match arquivos.len() {
                0 => {}
                1 => importar(arquivos.pop()),
                _ => trazer(Some(arquivos)),
            }
        }),
        escolher_arquivo(&ferr, t!("chaves-converter-uma-chave"), move |arquivo| {
            converter(Fonte::Arquivo(arquivo))
        }),
    );
    let colar = move || importar(None);

    let conteudo = Column::new()
        .grow(1000)
        .max_width(900)
        .gap(16)
        .child(Text::new(t!("chaves-chaves-ssh")).title())
        .child(
            Row::new()
                .gap(8)
                .child(
                    Button::new(t!("chaves-importar"))
                        .id("chaves-importar")
                        .icon(Symbol::ArrowDownload)
                        .on_click(importar_arquivo),
                )
                .child(
                    Button::new(t!("chaves-colar"))
                        .id("chaves-colar")
                        .icon(Symbol::ClipboardPaste)
                        .on_click(colar),
                )
                .child(
                    Button::new(t!("chaves-converter-arquivo"))
                        .id("chaves-converter")
                        .icon(Symbol::ArrowSync)
                        .on_click(converter_arquivo),
                ),
        )
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Row::new()
                        .grow(1)
                        .child(Text::new(t!("chaves-chaves-no-cofre")).subtitle()),
                )
                .child(
                    Button::new(t!("chaves-trazer-todas-de-ssh"))
                        .id("chaves-de-ssh")
                        .icon(Symbol::ArrowDownload)
                        .on_click({
                            let trazer = Rc::clone(&trazer);
                            move || trazer(None)
                        }),
                ),
        )
        .child(aviso_do_cofre)
        .child(cofre_vazio)
        .child(
            Column::new()
                .id("chaves-cofre-lista")
                .gap(8)
                .child(lista_cofre),
        )
        .child(formulario)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Button::new(t!("chaves-gerar"))
                        .id("gerar-chave-gerar")
                        .accent()
                        .icon(Symbol::Key)
                        .disabled(move || gerando.get())
                        .on_click(gerar),
                )
                .child(
                    ProgressRing::new()
                        .id("gerar-chave-andamento")
                        .width(24)
                        .height(24)
                        .visible(move || gerando.get()),
                )
                .child(
                    Text::new(move || erro.get())
                        .id("gerar-chave-erro")
                        .wrap()
                        .visible(move || !erro.get().is_empty()),
                ),
        )
        .child(bloco_resultado)
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(
                    Row::new()
                        .grow(1)
                        .child(Text::new(t!("chaves-chaves-em-ssh")).subtitle()),
                )
                .child(
                    Button::new(t!("comum-atualizar"))
                        .id("chaves-atualizar")
                        .icon(Symbol::ArrowClockwise)
                        .on_click(atualizar),
                )
                .child(
                    Button::new(t!("chaves-abrir-a-pasta"))
                        .id("chaves-abrir-pasta")
                        .icon(Symbol::FolderOpen)
                        .on_click(abrir),
                ),
        )
        .child(vazio)
        .child(Column::new().gap(8).child(lista))
        .child(view_trocar)
        .child(view_apagar)
        .child(view_importar)
        .child(view_trazer)
        .child(view_exportar)
        .child(view_converter)
        .child(view_remover);

    // Alinhado à esquerda, como as páginas de configurações do WinUI 3; o espaçador da
    // direita segura a largura máxima.
    Column::new().id("chaves-pagina").grow(1).child(
        ScrollViewer::new().id("chaves-rolagem").grow(1).child(
            Row::new()
                .padding(Insets::new(24.0, 40.0, 24.0, 40.0))
                .child(conteudo)
                .child(Column::new().grow(1)),
        ),
    )
}

/// Uma linha da lista: o que se sabe da chave e as ações.
fn linha_da_chave(
    c: &ChaveLocal,
    ferr: &Rc<Ferramentas>,
    acoes: &AcoesChaves,
    abridores: &Abridores,
) -> Card {
    let id = format!("chave-{}", c.nome);
    let mut marcas = Vec::new();
    if c.protegida {
        marcas.push(t!("chaves-marca-com-passphrase"));
    }
    if c.no_agente {
        marcas.push(t!("chaves-marca-no-agente"));
    }
    let detalhe = {
        let mut partes = vec![c.descricao()];
        if !c.comentario.is_empty() {
            partes.push(c.comentario.clone());
        }
        partes.join(" · ")
    };

    let copiar_publica = {
        let (ferr, arquivo) = (Rc::clone(ferr), c.arquivo.clone());
        move || match std::fs::read_to_string(public_path(&arquivo)) {
            Ok(texto) => clipboard::write(texto.trim().to_owned()),
            Err(e) => ferr.avisar_erro(t!(
                "chaves-nao-consegui-ler",
                caminho = public_path(&arquivo).display().to_string(),
                motivo = e.to_string()
            )),
        }
    };
    let carregar = {
        let (ferr, c) = (Rc::clone(ferr), c.clone());
        move || {
            ferr.pedir_ou_carregar(&c.arquivo, ferr.chave_nova(&c.arquivo), true);
        }
    };
    let menu = {
        let (trocar_c, c_trocar) = (Rc::clone(&abridores.trocar), c.clone());
        let (servidor, arquivo) = (Rc::clone(&acoes.servidor), c.arquivo.clone());
        let (importar, arquivo_cofre) = (Rc::clone(&abridores.importar), c.arquivo.clone());
        let (exportar, arquivo_exportar) = (Rc::clone(&abridores.exportar), c.arquivo.clone());
        MenuFlyout::new()
            .id(format!("{id}-menu"))
            .item(
                format!("{id}-trocar"),
                t!("chaves-trocar-a-passphrase"),
                move || trocar_c(&c_trocar),
            )
            .icon(Symbol::Rename)
            .item(
                format!("{id}-servidor"),
                t!("chaves-copiar-para-o-servidor"),
                move || servidor(Some(DeOnde::Arquivo(arquivo.clone()))),
            )
            .icon(Symbol::Send)
            .item(
                format!("{id}-para-cofre"),
                t!("chaves-mover-para-o-cofre"),
                move || importar(Some(arquivo_cofre.clone())),
            )
            .icon(Symbol::LockClosed)
            .item(
                format!("{id}-exportar"),
                t!("cofre-exportar-como"),
                move || exportar(Fonte::Arquivo(arquivo_exportar.clone())),
            )
            .icon(Symbol::ArrowUpload)
    };

    let mut texto = Column::new()
        .grow(1)
        .child(Text::new(c.nome.clone()).body_strong())
        .child(
            Text::new(detalhe)
                .id(format!("{id}-detalhe"))
                .caption()
                .secondary(),
        )
        .child(
            Text::new(c.fingerprint.clone())
                .id(format!("{id}-fingerprint"))
                .caption()
                .secondary()
                .wrap(),
        );
    if !marcas.is_empty() {
        texto = texto.child(
            Text::new(marcas.join(" · "))
                .id(format!("{id}-marcas"))
                .caption(),
        );
    }

    Card::new()
        .id(id.clone())
        .padding(12)
        .label(c.nome.clone())
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(texto)
                .child(
                    Button::new(t!("comum-copiar-a-publica"))
                        .id(format!("{id}-copiar"))
                        .icon(Symbol::Copy)
                        .on_click(copiar_publica),
                )
                .child(
                    Button::new(t!("chaves-carregar-no-agente"))
                        .id(format!("{id}-carregar"))
                        .icon(Symbol::Key)
                        .disabled(c.no_agente)
                        .on_click(carregar),
                )
                .child(
                    Button::icon_only(Symbol::MoreHorizontal)
                        .id(format!("{id}-mais"))
                        .label(t!("comum-mais"))
                        .flyout(menu),
                ),
        )
}

/// Uma chave do cofre: o que se sabe dela e as ações.
fn linha_do_cofre(
    l: &LinhaCofre,
    ferr: &Rc<Ferramentas>,
    acoes: &AcoesChaves,
    abridores: &Abridores,
) -> Card {
    let id = format!("chave-cofre-{}", l.id);
    let mut marcas = Vec::new();
    if l.token {
        marcas.push(t!("chaves-marca-token-fido"));
    }
    if l.carregada {
        marcas.push(t!("chaves-marca-no-agente"));
    }
    if l.confirmar {
        marcas.push(t!("chaves-marca-pede-confirmacao"));
    }
    if l.certificado {
        marcas.push(t!("chaves-marca-certificado"));
    }
    if !l.tem_privada {
        marcas.push(t!("chaves-marca-so-a-publica"));
    }
    let detalhe = [l.tipo.as_str(), l.comentario.as_str()]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");

    // O interruptor "No agente": mexer nele grava o item e acerta o agente. A primeira rodada
    // do efeito só confere o que a linha já mostra.
    let no_agente = signal(l.no_agente);
    {
        let (ferr, item) = (Rc::clone(ferr), l.id);
        effect(move || {
            let ligado = no_agente.get();
            untrack(|| {
                let guardado = ferr.cofre().agora(|c| {
                    c.item(item)
                        .and_then(|i| i.chave.as_ref())
                        .map(|k| k.no_agente)
                });
                if guardado.is_none_or(|g| g == ligado) {
                    return;
                }
                if let Err(erro) = ferr.definir_no_agente(item, ligado) {
                    ferr.avisar_erro(erro);
                    no_agente.set(guardado.unwrap_or(false));
                }
            });
        });
    }

    let copiar_publica = {
        let publica = l.publica.clone();
        move || clipboard::write(publica.trim().to_owned())
    };
    let menu = {
        let (item, titulo) = (l.id, l.titulo.clone());
        let (exportar, remover) = (
            Rc::clone(&abridores.exportar),
            Rc::clone(&abridores.remover),
        );
        let (servidor, fingerprint) = (Rc::clone(&acoes.servidor), l.fingerprint.clone());
        let (cofre_item, para_ssh_pub, para_ssh) = (
            Rc::clone(&acoes.cofre_item),
            (Rc::clone(ferr), titulo.clone()),
            (Rc::clone(ferr), titulo),
        );
        MenuFlyout::new()
            .id(format!("{id}-menu"))
            .item(
                format!("{id}-exportar"),
                t!("cofre-exportar-como"),
                move || exportar(Fonte::Item(item)),
            )
            .icon(Symbol::ArrowUpload)
            .item(
                format!("{id}-publica-ssh"),
                t!("chaves-por-a-publica-em-ssh"),
                move || match para_ssh_pub.0.por_publica_em_ssh(item) {
                    Ok(arquivo) => para_ssh_pub.0.avisar_sucesso(t!(
                        "chaves-publica-gravada-em",
                        caminho = arquivo.display().to_string()
                    )),
                    Err(erro) => para_ssh_pub.0.avisar_erro(erro),
                },
            )
            .icon(Symbol::Document)
            .item(
                format!("{id}-arquivo"),
                t!("chaves-por-a-chave-em-ssh"),
                move || match para_ssh.0.por_chave_em_ssh(item) {
                    Ok(arquivo) => para_ssh.0.avisar_sucesso(t!(
                        "chaves-gravada-sem-passphrase",
                        caminho = arquivo.display().to_string()
                    )),
                    Err(erro) => para_ssh.0.avisar_erro(erro),
                },
            )
            .icon(Symbol::Key)
            .item(
                format!("{id}-servidor"),
                t!("chaves-copiar-para-o-servidor"),
                move || servidor(Some(DeOnde::Cofre(fingerprint.clone()))),
            )
            .icon(Symbol::Send)
            .item(
                format!("{id}-cofre"),
                t!("chaves-abrir-no-cofre"),
                move || cofre_item(item),
            )
            .icon(Symbol::LockClosed)
            .item(
                format!("{id}-remover"),
                t!("chaves-remover-do-cofre-menu"),
                move || remover(item),
            )
            .icon(Symbol::Delete)
    };

    let mut texto = Column::new()
        .grow(1)
        .child(Text::new(l.titulo.clone()).body_strong())
        .child(
            Text::new(detalhe)
                .id(format!("{id}-detalhe"))
                .caption()
                .secondary()
                .visible(!l.tipo.is_empty() || !l.comentario.is_empty()),
        )
        .child(
            Text::new(l.fingerprint.clone())
                .id(format!("{id}-fingerprint"))
                .caption()
                .secondary()
                .wrap(),
        );
    if !marcas.is_empty() {
        texto = texto.child(
            Text::new(marcas.join(" · "))
                .id(format!("{id}-marcas"))
                .caption(),
        );
    }

    Card::new()
        .id(id.clone())
        .padding(12)
        .label(l.titulo.clone())
        .child(
            Row::new()
                .gap(8)
                .align(Align::Center)
                .child(texto)
                .child(
                    Button::new(t!("comum-copiar-a-publica"))
                        .id(format!("{id}-copiar"))
                        .icon(Symbol::Copy)
                        .on_click(copiar_publica),
                )
                .child(Text::new(t!("comum-no-agente")).caption())
                .child(
                    ToggleSwitch::bind(no_agente)
                        .id(format!("{id}-agente"))
                        .disabled(l.token || !l.tem_privada),
                )
                .child(
                    Button::icon_only(Symbol::MoreHorizontal)
                        .id(format!("{id}-mais"))
                        .label(t!("comum-mais"))
                        .flyout(menu),
                ),
        )
}

/// O diálogo de trocar a passphrase: devolve a função que o abre para uma chave e a view.
fn dialogo_trocar(ferr: &Rc<Ferramentas>) -> (AbrirTroca, impl View) {
    let aberto = signal(false);
    let nome = signal(String::new());
    let protegida = signal(false);
    let atual = signal(String::new());
    let nova = signal(String::new());
    let repetida = signal(String::new());
    let salvar = signal(true);
    let erro = signal(String::new());
    let alvo: Rc<RefCell<Option<ChaveLocal>>> = Rc::default();
    let refazer = Rc::new(Cell::new(false));

    // O diálogo fecha a cada botão: com erro, o efeito o abre de novo.
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
                    for campo in [atual, nova, repetida, erro] {
                        campo.set(String::new());
                    }
                });
            }
        });
    }

    let abrir: AbrirTroca = {
        let (alvo, ferr) = (Rc::clone(&alvo), Rc::clone(ferr));
        Rc::new(move |c: &ChaveLocal| {
            nome.set(c.nome.clone());
            protegida.set(c.protegida);
            let guardada = crate::agente::passphrase_do_cofre(ferr.cofre(), &c.arquivo);
            atual.set(guardada.unwrap_or_default());
            salvar.set(!ferr.cofre().agora(|x| x.bloqueado()));
            *alvo.borrow_mut() = Some(c.clone());
            aberto.set(true);
        })
    };

    let confirmar = {
        let (ferr, alvo, refazer) = (Rc::clone(ferr), Rc::clone(&alvo), Rc::clone(&refazer));
        move || {
            let Some(c) = alvo.borrow().clone() else {
                return;
            };
            let resposta = ferr.trocar_passphrase(
                &c.arquivo,
                &atual.get_untracked(),
                &nova.get_untracked(),
                &repetida.get_untracked(),
                salvar.get_untracked(),
            );
            match resposta {
                Ok(()) => {
                    erro.set(String::new());
                    ferr.avisar_sucesso(t!(
                        "chaves-passphrase-trocada",
                        nome = nome_do(&c.arquivo)
                    ));
                }
                Err(mensagem) => {
                    erro.set(mensagem);
                    refazer.set(true);
                }
            }
        }
    };

    let ferr = Rc::clone(ferr);
    let dialogo = ContentDialog::new(
        move || t!("chaves-passphrase-de", nome = nome.get()),
        aberto,
    )
    .id("chaves-trocar-dialogo")
    .content(
        Column::new()
            .gap(8)
            .child(
                Text::new(t!("chaves-deixe-a-nova-passphrase-vazia-para"))
                    .secondary()
                    .wrap(),
            )
            .child(
                Edit::bind(atual)
                    .id("chaves-trocar-atual")
                    .password()
                    .placeholder(t!("chaves-passphrase-atual"))
                    .visible(move || protegida.get()),
            )
            .child(
                Edit::bind(nova)
                    .id("chaves-trocar-nova")
                    .password()
                    .placeholder(t!("chaves-nova-passphrase")),
            )
            .child(
                Edit::bind(repetida)
                    .id("chaves-trocar-repetir")
                    .password()
                    .placeholder(t!("chaves-repetir-a-nova-passphrase")),
            )
            .child(
                CheckBox::bind(salvar, t!("chaves-salvar-a-nova-passphrase-no-cofre"))
                    .id("chaves-trocar-salvar")
                    .disabled(move || ferr.cofre().ler(|c| c.bloqueado())),
            )
            .child(
                Text::new(move || erro.get())
                    .id("chaves-trocar-erro")
                    .caption()
                    .wrap()
                    .visible(move || !erro.get().is_empty()),
            ),
    )
    .primary(t!("chaves-trocar"), confirmar)
    .close(t!("comum-cancelar"));
    (abrir, dialogo)
}
