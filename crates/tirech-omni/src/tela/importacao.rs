//! Importar de outros programas e exportar para eles (M38): os diálogos, a pré-visualização
//! e a gravação. O que cada formato lê e escreve está em [`crate::importar`] e
//! [`crate::exportar`] (lógica pura); aqui ficam o IO, o cofre e a tela.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;

use tomni_ui::dialogs;
use tomni_ui::prelude::*;

use super::{Tela, icone_do_tipo};
use crate::cofre::gravar_arquivo_privado;
use crate::cofre_json;
use crate::exportar::{self, Alvo};
use crate::importar::aplicar::{self, Duplicadas};
use crate::importar::{self, Entrada, Importacao};
use crate::modelo::Kind;
use crate::t;

/// As escolhas do que fazer com uma conexão que já existe.
fn duplicadas() -> [String; 3] {
    [
        t!("importacao-duplicada-pular"),
        t!("importacao-duplicada-substituir"),
        t!("importacao-duplicada-manter"),
    ]
}

fn duplicadas_de(texto: &str) -> Duplicadas {
    match duplicadas().iter().position(|d| d == texto) {
        Some(1) => Duplicadas::Substituir,
        Some(2) => Duplicadas::ManterAsDuas,
        _ => Duplicadas::Pular,
    }
}

/// O estado dos diálogos de importar e de exportar, guardado na [`Tela`].
pub(super) struct Estado {
    /// O formato que está sendo importado (`winscp`).
    formato: RefCell<String>,
    /// O arquivo que está sendo importado.
    arquivo: RefCell<PathBuf>,
    /// O texto dele, para analisar de novo com a senha-mestra.
    texto: RefCell<String>,
    /// A análise que ficou sem as senhas (o mRemoteNG pediu a senha-mestra): serve para "Sem senhas".
    sem_decifrar: RefCell<Option<Importacao>>,
    /// O que a pré-visualização mostra e a pessoa ainda pode importar.
    pendente: RefCell<Option<Importacao>>,
    mestra_aberta: Signal<bool>,
    mestra_senha: Signal<String>,
    mestra_errou: Signal<bool>,
    refazer_mestra: Rc<Cell<bool>>,
    previa_aberta: Signal<bool>,
    previa_nome: Signal<String>,
    previa_resumo: Signal<String>,
    previa_avisos: Signal<String>,
    previa_itens: Signal<Vec<TreeItem>>,
    previa_abertos: Signal<BTreeSet<String>>,
    previa_escolhido: Signal<Option<String>>,
    previa_destino: Signal<String>,
    previa_senhas: Signal<bool>,
    previa_tem_senhas: Signal<bool>,
    previa_duplicadas: Signal<String>,
    exportar_aberto: Signal<bool>,
    exportar_alvo: Cell<Alvo>,
    exportar_nome: Signal<String>,
    exportar_senhas: Signal<bool>,
    exportar_chaves: Signal<bool>,
    /// Se o formato guarda senhas.
    exportar_guarda_senhas: Signal<bool>,
    /// Se o formato leva chaves.
    exportar_leva_chaves: Signal<bool>,
    /// Se alguma conexão exportada usa uma chave do cofre.
    exportar_usa_chaves: Signal<bool>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            formato: RefCell::default(),
            arquivo: RefCell::default(),
            texto: RefCell::default(),
            sem_decifrar: RefCell::default(),
            pendente: RefCell::default(),
            mestra_aberta: signal(false),
            mestra_senha: signal(String::new()),
            mestra_errou: signal(false),
            refazer_mestra: Rc::default(),
            previa_aberta: signal(false),
            previa_nome: signal(String::new()),
            previa_resumo: signal(String::new()),
            previa_avisos: signal(String::new()),
            previa_itens: signal(Vec::new()),
            previa_abertos: signal(BTreeSet::new()),
            previa_escolhido: signal(None),
            previa_destino: signal(String::new()),
            previa_senhas: signal(false),
            previa_tem_senhas: signal(false),
            previa_duplicadas: signal(duplicadas()[0].clone()),
            exportar_aberto: signal(false),
            exportar_alvo: Cell::new(Alvo::Nativo),
            exportar_nome: signal(String::new()),
            exportar_senhas: signal(false),
            exportar_chaves: signal(true),
            exportar_guarda_senhas: signal(false),
            exportar_leva_chaves: signal(false),
            exportar_usa_chaves: signal(false),
        }
    }
}

/// Uma pasta ou conexão da pré-visualização.
struct No {
    nome: String,
    /// `None` é uma pasta.
    tipo: Option<Kind>,
    filhos: Vec<No>,
}

/// Desce por `caminho` criando as pastas que faltam; devolve os filhos da última.
fn descer<'a>(nos: &'a mut Vec<No>, caminho: &[String]) -> &'a mut Vec<No> {
    let Some((nome, resto)) = caminho.split_first() else {
        return nos;
    };
    let posicao = nos
        .iter()
        .position(|n| n.tipo.is_none() && &n.nome == nome)
        .unwrap_or_else(|| {
            nos.push(No {
                nome: nome.clone(),
                tipo: None,
                filhos: Vec::new(),
            });
            nos.len() - 1
        });
    descer(&mut nos[posicao].filhos, resto)
}

/// As linhas da pré-visualização (as pastas todas abertas) e as chaves das pastas.
fn itens_da_previa(importacao: &Importacao) -> (Vec<TreeItem>, BTreeSet<String>) {
    let mut raiz = Vec::new();
    for entrada in &importacao.entradas {
        match entrada {
            Entrada::Pasta(p) => {
                descer(&mut raiz, &p.caminho);
            }
            Entrada::Conexao(t) => descer(&mut raiz, &t.pasta).push(No {
                nome: t.conexao.label(),
                tipo: Some(t.conexao.kind),
                filhos: Vec::new(),
            }),
        }
    }
    fn converter(
        nos: Vec<No>,
        contador: &mut usize,
        abertas: &mut BTreeSet<String>,
    ) -> Vec<TreeItem> {
        nos.into_iter()
            .map(|no| {
                *contador += 1;
                let chave = format!("previa-{contador}");
                match no.tipo {
                    Some(tipo) => TreeItem::leaf(chave, no.nome).icon(icone_do_tipo(tipo)),
                    None => {
                        abertas.insert(chave.clone());
                        let filhos = converter(no.filhos, contador, abertas);
                        TreeItem::folder(chave, no.nome)
                            .icon(Symbol::Folder)
                            .children(filhos)
                    }
                }
            })
            .collect()
    }
    let mut abertas = BTreeSet::new();
    let itens = converter(raiz, &mut 0, &mut abertas);
    (itens, abertas)
}

/// "3 conexões em 2 grupos" (sem os grupos, quando não há).
fn resumo_da_previa(importacao: &Importacao) -> String {
    let (conexoes, grupos) = (importacao.total(), importacao.grupos());
    let mut texto = t!("importacao-previa-conexoes", n = conexoes as i64);
    if grupos > 0 {
        texto.push(' ');
        texto.push_str(&t!("importacao-previa-grupos", n = grupos as i64));
    }
    let senhas = importacao.senhas();
    if senhas > 0 {
        texto.push_str(&t!("importacao-previa-senhas", n = senhas as i64));
    }
    texto
}

impl Tela {
    // --- Importar ---

    /// "Importar ▸ <programa>": pede o arquivo do formato `id`, perto de onde ele costuma ficar.
    pub(super) fn importar_de(self: &Rc<Self>, id: &str) {
        let Some(formato) = importar::formato(id) else {
            return;
        };
        let tela = Rc::clone(self);
        let id = id.to_owned();
        let home = crate::armazenamento::pasta_pessoal();
        let provavel = home.as_deref().and_then(|h| {
            formato
                .locais_provaveis(h)
                .into_iter()
                .find(|caminho| caminho.is_file())
        });
        let mut dialogo = dialogs::open_file()
            .remember("importacao")
            .title(t!("importacao-importar-do", nome = formato.nome()));
        if !formato.extensoes().is_empty() {
            dialogo = dialogo.filter(formato.nome(), formato.extensoes());
        }
        let nome = match provavel {
            Some(caminho) => {
                if let Some(pasta) = caminho.parent() {
                    dialogo = dialogo.directory(pasta);
                }
                caminho.file_name().map_or_else(
                    || formato.arquivo_padrao().to_owned(),
                    |n| n.to_string_lossy().into_owned(),
                )
            }
            None => {
                if let Some(home) = home {
                    dialogo = dialogo.directory(home);
                }
                formato.arquivo_padrao().to_owned()
            }
        };
        dialogo.file_name(nome).show(move |caminho| {
            if let Some(caminho) = caminho {
                tela.ler_para_importar(&id, &caminho);
            }
        });
    }

    /// Lê o arquivo e analisa: o resto é [`Tela::analisar_importacao`].
    fn ler_para_importar(&self, id: &str, caminho: &Path) {
        let bytes = match std::fs::read(caminho) {
            Ok(bytes) => bytes,
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "comum-nao-consegui-ler",
                        arquivo = caminho.display().to_string(),
                        erro = erro.to_string()
                    ),
                );
                return;
            }
        };
        let e = &self.importacao;
        *e.formato.borrow_mut() = id.to_owned();
        *e.arquivo.borrow_mut() = caminho.to_owned();
        *e.texto.borrow_mut() = importar::texto_do_arquivo(&bytes);
        e.mestra_senha.set(String::new());
        e.mestra_errou.set(false);
        self.analisar_importacao(None);
    }

    /// Analisa o texto guardado (com a senha-mestra, se houver) e abre o diálogo que cabe: o da
    /// senha-mestra do mRemoteNG ou a pré-visualização.
    fn analisar_importacao(&self, senha_mestra: Option<&str>) {
        let e = &self.importacao;
        let Some(formato) = importar::formato(&e.formato.borrow()) else {
            return;
        };
        let arquivo = e.arquivo.borrow().clone();
        e.refazer_mestra.set(false);
        let analise = formato.analisar(&e.texto.borrow(), senha_mestra);
        let importacao = match analise {
            Ok(importacao) => importacao,
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "importacao-nao-consegui-importar",
                        arquivo = arquivo.display().to_string(),
                        erro = erro.to_string()
                    ),
                );
                return;
            }
        };
        if importacao.pede_senha_mestra {
            e.mestra_errou.set(senha_mestra.is_some());
            e.mestra_senha.set(String::new());
            *e.sem_decifrar.borrow_mut() = Some(importacao);
            // Digitada pelo diálogo: ele ainda está aberto e a reabre ao fechar.
            e.refazer_mestra.set(senha_mestra.is_some());
            if senha_mestra.is_none() {
                e.mestra_aberta.set(true);
            }
            return;
        }
        if importacao.total() == 0 && importacao.grupos() == 0 {
            let mut mensagem = t!(
                "importacao-nenhuma-conexao-em",
                arquivo = arquivo.display().to_string()
            );
            for aviso in &importacao.avisos {
                mensagem.push(' ');
                mensagem.push_str(aviso);
            }
            self.avisar(InfoBarSeverity::Warning, mensagem);
            return;
        }
        self.mostrar_previa(formato.nome(), importacao);
    }

    /// "Sem senhas" do diálogo da senha-mestra: segue com o que o arquivo mostra sem decifrar.
    fn importar_sem_decifrar(&self) {
        let e = &self.importacao;
        let Some(mut importacao) = e.sem_decifrar.borrow_mut().take() else {
            return;
        };
        importacao.pede_senha_mestra = false;
        if let Some(formato) = importar::formato(&e.formato.borrow()) {
            self.mostrar_previa(formato.nome(), importacao);
        }
    }

    /// Enche e abre a pré-visualização.
    fn mostrar_previa(&self, nome: &str, importacao: Importacao) {
        let e = &self.importacao;
        let (itens, abertas) = itens_da_previa(&importacao);
        let com_senhas = importacao.senhas() > 0;
        let por_padrao = self.config.with_untracked(|c| c.conexoes.importar_senhas);
        e.previa_nome.set(nome.to_owned());
        e.previa_resumo.set(resumo_da_previa(&importacao));
        e.previa_avisos.set(importacao.avisos.join("\n"));
        e.previa_itens.set(itens);
        e.previa_abertos.set(abertas);
        e.previa_escolhido.set(None);
        e.previa_destino
            .set(t!("importacao-importado-de", nome = nome));
        e.previa_tem_senhas.set(com_senhas);
        e.previa_senhas
            .set(com_senhas && por_padrao && !self.cofre.agora(|c| c.bloqueado()));
        e.previa_duplicadas.set(duplicadas()[0].clone());
        *e.pendente.borrow_mut() = Some(importacao);
        e.previa_aberta.set(true);
    }

    /// "Importar" da pré-visualização: traz os anexos do JSON nativo, grava na árvore e no
    /// cofre e abre o grupo.
    fn confirmar_importacao(&self) {
        let e = &self.importacao;
        let Some(mut importacao) = e.pendente.borrow_mut().take() else {
            return;
        };
        let aberto = !self.cofre.agora(|c| c.bloqueado());
        let opcoes = aplicar::Opcoes {
            destino: e.previa_destino.get_untracked(),
            senhas: e.previa_senhas.get_untracked()
                && e.previa_tem_senhas.get_untracked()
                && aberto,
            duplicadas: duplicadas_de(&e.previa_duplicadas.get_untracked()),
            perfis_de_ia: self
                .assistente
                .perfis
                .with_untracked(|a| a.perfis.iter().map(|p| p.id).collect()),
        };
        let mut avisos_do_arquivo = Vec::new();
        if opcoes.senhas
            && let Some(lote) = importacao.cofre.as_mut()
        {
            let pasta = e
                .arquivo
                .borrow()
                .parent()
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
            let vault = self.vault();
            match cofre_json::trazer_anexos(lote, &pasta, &vault, &|_| {}, &AtomicBool::new(false))
            {
                Ok(t) if t.nao_encontrados > 0 => avisos_do_arquivo.push(t!(
                    "importacao-anexos-nao-encontrados",
                    n = t.nao_encontrados as i64
                )),
                Ok(_) => {}
                Err(erro) => {
                    self.avisar(
                        InfoBarSeverity::Error,
                        t!(
                            "importacao-nao-consegui-trazer-anexos",
                            erro = erro.to_string()
                        ),
                    );
                    return;
                }
            }
        }
        let Some(aplicado) = self.tree.try_update(|t| {
            self.cofre
                .mudar(|c| aplicar::aplicar(t, (!c.bloqueado()).then_some(c), importacao, &opcoes))
        }) else {
            return;
        };
        self.abre_grupo(aplicado.grupo);
        self.atualizar();
        let mut mensagem = aplicado.resumo();
        if aplicado.senhas > 0 {
            mensagem.push(' ');
            mensagem.push_str(&t!(
                "importacao-itens-para-o-cofre",
                n = aplicado.senhas as i64
            ));
        }
        let avisos = aplicado.avisos.iter().chain(&avisos_do_arquivo);
        let mut com_aviso = false;
        for aviso in avisos {
            com_aviso = true;
            mensagem.push(' ');
            mensagem.push_str(aviso);
        }
        let severidade = if com_aviso {
            InfoBarSeverity::Warning
        } else {
            InfoBarSeverity::Success
        };
        self.avisar(severidade, mensagem);
        self.pagina.set("terminais".to_owned());
    }

    // --- Exportar ---

    /// "Exportar ▸ <programa>": abre o diálogo das caixas.
    pub(super) fn exportar_para(&self, alvo: Alvo) {
        let e = &self.importacao;
        let aberto = !self.cofre.agora(|c| c.bloqueado());
        let usa_chaves = self
            .tree
            .with_untracked(|t| !exportar::chaves::usadas(t, alvo).is_empty());
        e.exportar_alvo.set(alvo);
        e.exportar_nome.set(alvo.nome().to_owned());
        e.exportar_guarda_senhas.set(alvo.guarda_senhas());
        e.exportar_leva_chaves
            .set(alvo == Alvo::Nativo || alvo.formato_de_chave().is_some());
        e.exportar_usa_chaves.set(usa_chaves);
        e.exportar_senhas.set(alvo.guarda_senhas() && aberto);
        e.exportar_chaves
            .set(usa_chaves && (alvo == Alvo::Nativo || alvo.formato_de_chave().is_some()));
        e.exportar_aberto.set(true);
    }

    /// "Exportar…" do diálogo: pede onde salvar e grava.
    fn salvar_exportacao(self: &Rc<Self>) {
        let alvo = self.importacao.exportar_alvo.get();
        let senhas = self.importacao.exportar_senhas.get_untracked();
        // Só vale a caixa das chaves quando ela está habilitada (o JSON as leva com as senhas).
        let e = &self.importacao;
        let chaves = e.exportar_chaves.get_untracked()
            && e.exportar_leva_chaves.get_untracked()
            && e.exportar_usa_chaves.get_untracked()
            && (alvo != Alvo::Nativo || senhas);
        let tela = Rc::clone(self);
        let mut dialogo = dialogs::save_file()
            .remember("exportacao")
            .title(t!("importacao-exportar-para", nome = alvo.nome()))
            .file_name(alvo.arquivo_padrao());
        if !alvo.extensoes().is_empty() {
            dialogo = dialogo.filter(alvo.nome(), alvo.extensoes());
        }
        dialogo.show(move |caminho| {
            if let Some(caminho) = caminho {
                tela.exportar_em(alvo, &caminho, senhas, chaves);
            }
        });
    }

    /// Exporta a árvore para `caminho`: grava as chaves do cofre (se pedido), o arquivo e os
    /// anexos das notas, e conta o que saiu.
    pub(super) fn exportar_em(&self, alvo: Alvo, caminho: &Path, senhas: bool, chaves: bool) {
        let arvore = self.tree.get_untracked();
        let opcoes = exportar::Opcoes {
            senhas: senhas && alvo.guarda_senhas(),
            chaves,
        };
        let (gravadas, resultado) = self.cofre.agora(|c| {
            let cofre = (!c.bloqueado()).then_some(c);
            let gravadas = if chaves {
                exportar::chaves::gravar(&arvore, cofre, alvo, caminho)
            } else {
                exportar::chaves::Gravadas::default()
            };
            let resultado = exportar::exportar(
                alvo,
                &arvore,
                cofre,
                opcoes,
                &gravadas.arquivos,
                caminho,
                &mut exportar::Sistema,
            );
            (gravadas, resultado)
        });
        let bytes = exportar::bytes_do_arquivo(alvo, &resultado.texto);
        if let Err(erro) = gravar_arquivo_privado(caminho, &bytes) {
            self.avisar(
                InfoBarSeverity::Error,
                t!(
                    "comum-nao-consegui-salvar",
                    arquivo = caminho.display().to_string(),
                    erro = erro.to_string()
                ),
            );
            return;
        }
        let mut avisos = resultado.avisos.clone();
        avisos.extend(gravadas.avisos);
        if !resultado.anexos.is_empty() {
            let vault = self.vault();
            if let Err(erro) = cofre_json::gravar_arquivos(
                &resultado.anexos,
                &vault,
                caminho,
                &|_| {},
                &AtomicBool::new(false),
            ) {
                avisos.push(t!(
                    "importacao-nao-consegui-gravar-anexos",
                    erro = erro.to_string()
                ));
            }
        }
        let mut mensagem = t!("importacao-salvei", arquivo = caminho.display().to_string());
        mensagem.push(' ');
        mensagem.push_str(
            &exportar::Resultado {
                avisos: Vec::new(),
                ..resultado.clone()
            }
            .resumo(),
        );
        for aviso in &avisos {
            mensagem.push(' ');
            mensagem.push_str(aviso);
        }
        let severidade = if avisos.is_empty() {
            InfoBarSeverity::Success
        } else {
            InfoBarSeverity::Warning
        };
        self.avisar(severidade, mensagem);
    }
}

/// O submenu "Importar ▸": um item por programa, e depois o `~/.ssh/config` e o `~/.aws`, que
/// seguem direto, sem pré-visualização.
pub(super) fn menu_importar(tela: &Rc<Tela>) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id("importar-menu");
    for formato in importar::formatos() {
        let t = Rc::clone(tela);
        let id = formato.id();
        menu = menu.item(
            format!("importar-{id}"),
            format!("{}…", formato.nome()),
            move || t.importar_de(id),
        );
    }
    let (ssh, aws) = (Rc::clone(tela), Rc::clone(tela));
    menu.separator()
        .item("importar", t!("importacao-ssh-config"), move || {
            ssh.importar()
        })
        .item("importar-aws", t!("importacao-aws"), move || {
            aws.importar_aws()
        })
}

/// O submenu "Exportar ▸": o JSON do Tirech Omni primeiro e depois cada programa.
pub(super) fn menu_exportar(tela: &Rc<Tela>) -> MenuFlyout {
    let mut menu = MenuFlyout::new().id("exportar-menu");
    for (i, alvo) in Alvo::TODOS.into_iter().enumerate() {
        let t = Rc::clone(tela);
        if i == 1 {
            menu = menu.separator();
        }
        menu = menu.item(
            format!("exportar-{}", alvo.id()),
            format!("{}…", alvo.nome()),
            move || t.exportar_para(alvo),
        );
    }
    menu
}

/// Os diálogos de importar e de exportar.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View + use<> {
    let e = &tela.importacao;

    // Solta o que a pré-visualização guardava quando ela fecha sem importar.
    {
        let t = Rc::clone(tela);
        let aberta = e.previa_aberta;
        effect(move || {
            if !aberta.get() {
                untrack(|| t.importacao.pendente.borrow_mut().take());
            }
        });
    }

    Column::new()
        .child(dialogo_mestra(tela))
        .child(dialogo_previa(tela))
        .child(dialogo_exportar(tela))
}

fn dialogo_mestra(tela: &Rc<Tela>) -> impl View + use<> {
    let e = &tela.importacao;
    let (aberto, senha, errou) = (e.mestra_aberta, e.mestra_senha, e.mestra_errou);
    let refazer = Rc::clone(&e.refazer_mestra);
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
                    senha.set(String::new());
                    errou.set(false);
                });
            }
        });
    }
    let t = Rc::clone(tela);
    ContentDialog::new(t!("importacao-senha-mestra-do-mremoteng"), aberto)
        .id("importar-mestra-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(t!("importacao-esse-arquivo-usa-uma-senha-mestra"))
                        .secondary()
                        .wrap(),
                )
                .child(
                    Edit::bind(senha)
                        .id("importar-mestra-senha")
                        .password()
                        .placeholder(t!("importacao-senha-mestra")),
                )
                .child(
                    Text::new(t!("importacao-a-senha-mestra-informada-nao-abre"))
                        .id("importar-mestra-erro")
                        .caption()
                        .visible(move || errou.get()),
                ),
        )
        .primary(t!("comum-continuar"), move || {
            let digitada = senha.get_untracked();
            t.analisar_importacao(Some(&digitada));
        })
        .secondary(t!("importacao-sem-senhas"), {
            let t = Rc::clone(tela);
            move || t.importar_sem_decifrar()
        })
        .close(t!("comum-cancelar"))
}

fn dialogo_previa(tela: &Rc<Tela>) -> impl View + use<> {
    let e = &tela.importacao;
    let aberto = e.previa_aberta;
    let (nome, resumo, avisos) = (e.previa_nome, e.previa_resumo, e.previa_avisos);
    let (itens, abertos, escolhido) = (e.previa_itens, e.previa_abertos, e.previa_escolhido);
    let (destino, senhas, tem_senhas, duplicadas) = (
        e.previa_destino,
        e.previa_senhas,
        e.previa_tem_senhas,
        e.previa_duplicadas,
    );
    let bloqueado = {
        let t = Rc::clone(tela);
        move || t.cofre.ler(|c| c.bloqueado())
    };
    let t = Rc::clone(tela);
    ContentDialog::new(t!("importacao-importar-conexoes"), aberto)
        .id("importar-dialogo")
        .content(
            Column::new()
                .gap(8)
                .width(480)
                .child(
                    Text::new(move || {
                        t!(
                            "importacao-resumo-do-programa",
                            nome = nome.get(),
                            resumo = resumo.get()
                        )
                    })
                    .id("importar-resumo")
                    .wrap(),
                )
                .child(
                    TreeView::bind(itens, escolhido)
                        .id("importar-arvore")
                        .expanded(abertos)
                        .height(200),
                )
                .child(
                    Text::new(move || avisos.get())
                        .id("importar-avisos")
                        .caption()
                        .wrap()
                        .visible(move || !avisos.get().is_empty()),
                )
                .child(Text::new(t!("importacao-grupo-de-destino")).caption())
                .child(
                    Edit::bind(destino)
                        .id("importar-destino")
                        .placeholder(t!("importacao-vazio-na-raiz-da-arvore")),
                )
                .child(
                    CheckBox::bind(senhas, t!("importacao-importar-as-senhas-no-cofre"))
                        .id("importar-senhas")
                        .disabled(bloqueado.clone())
                        .visible(move || tem_senhas.get()),
                )
                .child(
                    Text::new(t!("importacao-desbloqueie-o-cofre-para-guardar-as"))
                        .id("importar-cofre-bloqueado")
                        .caption()
                        .visible(move || tem_senhas.get() && bloqueado()),
                )
                .child(Text::new(t!("importacao-se-a-conexao-ja-existe")).caption())
                .child(
                    RadioButtons::bind(duplicadas, self::duplicadas()).id("importar-duplicadas"),
                ),
        )
        .primary(t!("comum-importar"), move || t.confirmar_importacao())
        .close(t!("comum-cancelar"))
}

fn dialogo_exportar(tela: &Rc<Tela>) -> impl View + use<> {
    let e = &tela.importacao;
    let aberto = e.exportar_aberto;
    let (nome, senhas, chaves) = (e.exportar_nome, e.exportar_senhas, e.exportar_chaves);
    let (guarda, leva, usa) = (
        e.exportar_guarda_senhas,
        e.exportar_leva_chaves,
        e.exportar_usa_chaves,
    );
    let bloqueado = {
        let t = Rc::clone(tela);
        move || t.cofre.ler(|c| c.bloqueado())
    };
    // O JSON do Tirech Omni só leva as chaves junto com o cofre (as senhas).
    let nativo = {
        let t = Rc::clone(tela);
        move || t.importacao.exportar_alvo.get() == Alvo::Nativo
    };
    let sem_senhas = {
        let bloqueado = bloqueado.clone();
        move || !guarda.get() || bloqueado()
    };
    let sem_chaves = {
        let nativo = nativo.clone();
        move || !leva.get() || !usa.get() || (nativo() && !senhas.get())
    };
    let t = Rc::clone(tela);
    ContentDialog::new(t!("importacao-exportar-conexoes"), aberto)
        .id("exportar-dialogo")
        .content(
            Column::new()
                .gap(8)
                .width(480)
                .child(
                    Text::new(move || t!("importacao-exportar-para-o-formato", nome = nome.get()))
                        .id("exportar-formato")
                        .wrap(),
                )
                .child(
                    CheckBox::bind(senhas, t!("importacao-incluir-as-senhas"))
                        .id("exportar-senhas")
                        .disabled(sem_senhas),
                )
                .child(
                    Text::new(t!("importacao-este-formato-nao-guarda-senhas"))
                        .id("exportar-sem-senhas")
                        .caption()
                        .visible(move || !guarda.get()),
                )
                .child(
                    Text::new(t!("importacao-desbloqueie-o-cofre-para-levar-as"))
                        .id("exportar-cofre-bloqueado")
                        .caption()
                        .visible({
                            let bloqueado = bloqueado.clone();
                            move || guarda.get() && bloqueado()
                        }),
                )
                .child(
                    CheckBox::bind(chaves, t!("importacao-incluir-as-chaves-do-cofre"))
                        .id("exportar-chaves")
                        .disabled(sem_chaves),
                )
                .child(
                    Text::new(t!("importacao-este-formato-nao-leva-chaves"))
                        .id("exportar-sem-chaves")
                        .caption()
                        .visible(move || !leva.get()),
                )
                .child(
                    Text::new(t!("importacao-nenhuma-conexao-usa-uma-chave-do"))
                        .id("exportar-nenhuma-chave")
                        .caption()
                        .visible(move || leva.get() && !usa.get()),
                )
                .child(
                    Text::new(t!("importacao-as-chaves-saem-sem-passphrase-numa"))
                        .caption()
                        .wrap()
                        .visible(move || leva.get() && usa.get() && chaves.get()),
                ),
        )
        .primary(t!("importacao-exportar"), move || t.salvar_exportacao())
        .close(t!("comum-cancelar"))
}
