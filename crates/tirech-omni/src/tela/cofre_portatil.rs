//! O cofre entre máquinas (M44): levar o cofre num arquivo só, abrir o de outra máquina
//! (juntando ou no lugar do daqui), mudar o local do cofre, e a conferência periódica do
//! `vault.json` e das cópias em conflito que a sincronização deixa na pasta.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tomni_ui::dialogs;
use tomni_ui::prelude::*;

use super::Tela;
use crate::andamento::Andamento;
use crate::anexos::{self, Copia, ErroAnexo};
use crate::cofre::{Cofre, Externo, gravar_arquivo_privado};
use crate::t;

/// A extensão do arquivo que leva o cofre.
///
/// Identificador de formato: não muda com o nome do app (está gravado nos arquivos de quem usa).
pub(crate) const EXTENSAO: &str = "tirech-vault";

/// O estado dos diálogos do cofre entre máquinas, guardado na [`Tela`].
pub(super) struct Estado {
    levar_aberto: Signal<bool>,
    levar_senha: Signal<String>,
    senha_aberta: Signal<bool>,
    senha: Signal<String>,
    senha_errou: Signal<bool>,
    refazer_senha: Rc<Cell<bool>>,
    abrir_aberto: Signal<bool>,
    substituir_aberto: Signal<bool>,
    /// O cofre de fora que os diálogos de abrir tratam.
    externo: Rc<RefCell<Option<Externo>>>,
    /// O nome do arquivo de fora, para os textos.
    arquivo: Signal<String>,
    /// "N itens, M chaves SSH, alterado em …".
    resumo: Signal<String>,
    /// A cópia em conflito que está sendo juntada (vira `.juntado` depois).
    em_conflito: RefCell<Option<PathBuf>>,
    local_aberto: Signal<bool>,
    local_texto: Signal<String>,
    local_com_cofre: Signal<bool>,
    local_destino: RefCell<PathBuf>,
    conflitos: Signal<Vec<PathBuf>>,
    ultimo_erro: RefCell<String>,
    /// O caminho do arquivo de fora que os diálogos de abrir tratam.
    caminho_externo: RefCell<PathBuf>,
    /// Se o cofre tem anexos: o diálogo de levar avisa da pasta `<nome>.vault`.
    levar_com_anexos: Signal<bool>,
    copia: Andamento<Copia>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            levar_aberto: signal(false),
            levar_senha: signal(String::new()),
            senha_aberta: signal(false),
            senha: signal(String::new()),
            senha_errou: signal(false),
            refazer_senha: Rc::new(Cell::new(false)),
            abrir_aberto: signal(false),
            substituir_aberto: signal(false),
            externo: Rc::default(),
            arquivo: signal(String::new()),
            resumo: signal(String::new()),
            em_conflito: RefCell::new(None),
            local_aberto: signal(false),
            local_texto: signal(String::new()),
            local_com_cofre: signal(false),
            local_destino: RefCell::new(PathBuf::new()),
            conflitos: signal(Vec::new()),
            ultimo_erro: RefCell::default(),
            caminho_externo: RefCell::new(PathBuf::new()),
            levar_com_anexos: signal(false),
            copia: Andamento::nova(),
        }
    }
}

/// A frase das notas alteradas nas duas máquinas (vazia sem conflito): a versão que perdeu ficou
/// como uma nota nova.
fn frase_de_conflitos(conflitos: usize) -> String {
    match conflitos {
        0 => String::new(),
        n => format!(" {}", t!("cofre-conflitos-notas", n = n as i64)),
    }
}

/// A pasta dos anexos que fica ao lado de um `.tirech-vault`: `<nome>.vault`. Se o arquivo já
/// tem esse nome, acrescenta `.vault` ao nome inteiro.
fn pasta_dos_anexos_ao_lado(arquivo: &Path) -> PathBuf {
    let pasta = arquivo.with_extension("vault");
    if pasta == arquivo {
        let mut nome = arquivo.as_os_str().to_owned();
        nome.push(".vault");
        PathBuf::from(nome)
    } else {
        pasta
    }
}

/// Onde estão os anexos do cofre de fora: `<nome>.vault` ao lado de um `.tirech-vault`, ou
/// `vault/` ao lado de um `vault.json` (ou de uma cópia dele).
fn pasta_dos_anexos_de_fora(arquivo: &Path) -> PathBuf {
    if arquivo.extension().is_some_and(|e| e == EXTENSAO) {
        pasta_dos_anexos_ao_lado(arquivo)
    } else {
        arquivo
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(anexos::PASTA)
    }
}

/// Os anexos que o cofre de fora cita e que não vieram: o que dizer e onde procurar (vazio se
/// nenhum faltou).
fn frase_de_anexos(faltam: usize, pasta: &Path) -> String {
    let nome = pasta
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    match faltam {
        0 => String::new(),
        n => format!(
            " {}",
            t!("cofre-anexos-nao-vieram", n = n as i64, nome = nome)
        ),
    }
}

/// O texto de uma junção feita: o que entrou, mudou e saiu.
fn texto_da_juncao(r: crate::cofre::Resumo) -> String {
    if r.mudou() {
        t!(
            "cofre-juncao-feita",
            novos = r.novos as i64,
            atualizados = r.atualizados as i64,
            removidos = r.removidos as i64,
            conflitos = frase_de_conflitos(r.conflitos)
        )
    } else {
        t!("cofre-juncao-nada")
    }
}

impl Tela {
    // --- Levar ---

    /// "Levar o cofre para outra máquina…": abre o diálogo, que pede uma senha para o arquivo se
    /// o cofre não tem senha mestra.
    pub(super) fn levar_cofre(&self) {
        if self.cofre.agora(|c| c.bloqueado()) {
            self.avisar(InfoBarSeverity::Warning, t!("cofre-desbloqueie-para-levar"));
            return;
        }
        let vault = self.vault();
        self.portatil
            .levar_com_anexos
            .set(anexos::tem_anexos(&vault));
        self.portatil.levar_senha.set(String::new());
        self.portatil.levar_aberto.set(true);
    }

    /// A pasta `vault/` do cofre de agora.
    pub(super) fn vault(&self) -> PathBuf {
        self.cofre.agora(|c| c.pasta_do_cofre().join(anexos::PASTA))
    }

    /// Copia os arquivos de `origem` que faltam em `destino` numa thread, com o diálogo de
    /// andamento, e roda `ao_fim` na thread da interface. Sem anexos para copiar (ou com as duas
    /// pastas sendo a mesma), roda `ao_fim` na hora.
    fn copiar_anexos(
        &self,
        texto: impl Into<String>,
        origem: PathBuf,
        destino: PathBuf,
        ao_fim: impl FnOnce(Result<Copia, ErroAnexo>) + 'static,
    ) {
        let mesma = origem == destino
            || matches!(
                (origem.canonicalize(), destino.canonicalize()),
                (Ok(a), Ok(b)) if a == b
            );
        if mesma || !anexos::tem_anexos(&origem) {
            ao_fim(Ok(Copia::default()));
            return;
        }
        self.cofre.marcar_uso();
        self.portatil.copia.iniciar(
            texto,
            move |progresso, cancelado| {
                anexos::copiar_pasta(&origem, &destino, progresso, cancelado)
            },
            ao_fim,
        );
    }

    /// Cifra o cofre e pede onde salvar o `.tirech-vault`.
    fn salvar_cofre_portatil(self: &Rc<Self>) {
        let senha = self.portatil.levar_senha.get_untracked();
        self.portatil.levar_senha.set(String::new());
        let senha = (!senha.is_empty()).then_some(senha);
        let bytes = match self.cofre.agora(|c| c.levar(senha.as_deref())) {
            Ok(bytes) => bytes,
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!("cofre-nao-consegui-preparar", erro = erro.to_string()),
                );
                return;
            }
        };
        let tela = Rc::clone(self);
        dialogs::save_file()
            .remember("cofre-portatil")
            .title(t!("portatil-levar-o-cofre-para-outra-maquina"))
            .filter(t!("portatil-cofre-do-app"), &[EXTENSAO])
            .file_name(format!("cofre.{EXTENSAO}"))
            .show(move |caminho| {
                let Some(mut caminho) = caminho else {
                    return;
                };
                if caminho.extension().is_none() {
                    caminho.set_extension(EXTENSAO);
                }
                if let Err(erro) = gravar_arquivo_privado(&caminho, &bytes) {
                    tela.avisar(
                        InfoBarSeverity::Error,
                        t!(
                            "comum-nao-consegui-salvar",
                            arquivo = caminho.display().to_string(),
                            erro = erro.to_string()
                        ),
                    );
                    return;
                }
                let pasta = pasta_dos_anexos_ao_lado(&caminho);
                let vault = tela.vault();
                let avisar = Rc::clone(&tela);
                tela.copiar_anexos(
                    t!("cofre-copiando-anexos-andamento"),
                    vault,
                    pasta.clone(),
                    move |r| match r {
                        Ok(copia) => avisar.avisar(
                            InfoBarSeverity::Success,
                            t!(
                                "cofre-salvo-em",
                                caminho = caminho.display().to_string(),
                                anexos = if copia.copiados + copia.pulados == 0 {
                                    String::new()
                                } else {
                                    format!(
                                        " {}",
                                        t!(
                                            "cofre-anexos-na-pasta",
                                            pasta = pasta.display().to_string()
                                        )
                                    )
                                },
                                menu = t!("cofre-abrir-cofre-de-outra-maquina")
                            ),
                        ),
                        Err(ErroAnexo::Cancelado) => avisar.avisar(
                            InfoBarSeverity::Warning,
                            t!(
                                "cofre-salvo-copia-cancelada",
                                caminho = caminho.display().to_string()
                            ),
                        ),
                        Err(erro) => avisar.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "cofre-salvo-copia-falhou",
                                caminho = caminho.display().to_string(),
                                erro = erro.to_string()
                            ),
                        ),
                    },
                );
            });
    }

    // --- Abrir ---

    /// "Abrir cofre de outra máquina…": pede o arquivo.
    pub(super) fn abrir_cofre_de_fora(self: &Rc<Self>) {
        let tela = Rc::clone(self);
        dialogs::open_file()
            .remember("cofre-portatil")
            .title(t!("portatil-abrir-cofre-de-outra-maquina"))
            .filter(t!("portatil-cofre-do-app"), &[EXTENSAO, "json"])
            .show(move |caminho| {
                if let Some(caminho) = caminho {
                    tela.abrir_externo(&caminho, false);
                }
            });
    }

    /// Lê o cofre de `caminho` e abre o diálogo que cabe: o da senha dele, se pedir uma, ou o
    /// das duas escolhas. `conflito` marca uma cópia que a sincronização deixou na pasta.
    fn abrir_externo(&self, caminho: &Path, conflito: bool) {
        let externo = match self.cofre.agora(|c| c.abrir_copia(caminho)) {
            Ok(externo) => externo,
            Err(erro) => {
                self.avisar(InfoBarSeverity::Error, erro);
                return;
            }
        };
        let p = &self.portatil;
        p.arquivo.set(
            caminho
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        );
        *p.em_conflito.borrow_mut() = conflito.then(|| caminho.to_owned());
        *p.caminho_externo.borrow_mut() = caminho.to_owned();
        let pede = externo.pede_senha();
        *p.externo.borrow_mut() = Some(externo);
        if pede {
            p.senha.set(String::new());
            p.senha_errou.set(false);
            p.senha_aberta.set(true);
        } else {
            self.mostrar_externo();
        }
    }

    /// O diálogo das duas escolhas, com o resumo do cofre de fora.
    fn mostrar_externo(&self) {
        let p = &self.portatil;
        let Some(resumo) = p.externo.borrow().as_ref().and_then(Externo::resumo) else {
            return;
        };
        let alterado = if resumo.alterado == 0 {
            String::new()
        } else {
            t!(
                "cofre-alterado-em",
                quando = crate::recentes::data_e_hora(resumo.alterado)
            )
        };
        p.resumo.set(t!(
            "cofre-resumo-externo",
            itens = t!("comum-n-itens", n = resumo.itens as i64),
            chaves = t!("comum-n-chaves-ssh", n = resumo.chaves_ssh as i64),
            alterado = alterado
        ));
        p.abrir_aberto.set(true);
    }

    /// Destrava o cofre de fora com a senha dele. Com a senha errada o diálogo volta.
    fn destravar_externo(&self) {
        let p = &self.portatil;
        let senha = p.senha.get_untracked();
        p.senha.set(String::new());
        let certa = p
            .externo
            .borrow_mut()
            .as_mut()
            .is_some_and(|e| e.desbloquear(&senha));
        p.senha_errou.set(!certa);
        p.refazer_senha.set(!certa);
        if certa {
            self.mostrar_externo();
        }
    }

    /// Traz para o `vault/` daqui os anexos do cofre de fora que ainda não existem aqui (a pasta
    /// `<nome>.vault` ao lado do `.tirech-vault`, ou o `vault/` ao lado do `vault.json`) e roda
    /// `seguir` com a pasta de onde vieram e quantos anexos o cofre de fora cita sem que o
    /// arquivo exista aqui. Cancelar a cópia não segue.
    fn trazer_anexos(self: &Rc<Self>, seguir: impl FnOnce(&Rc<Tela>, &Path, usize) + 'static) {
        let p = &self.portatil;
        let origem = pasta_dos_anexos_de_fora(&p.caminho_externo.borrow());
        let ids = p
            .externo
            .borrow()
            .as_ref()
            .map(Externo::anexos_referenciados)
            .unwrap_or_default();
        let destino = self.vault();
        let tela = Rc::clone(self);
        self.copiar_anexos(
            t!("cofre-copiando-anexos-andamento"),
            origem.clone(),
            destino.clone(),
            move |r| match r {
                Ok(_) => {
                    let faltam = anexos::faltam(&destino, &ids);
                    seguir(&tela, &origem, faltam);
                }
                Err(ErroAnexo::Cancelado) => {
                    tela.avisar(
                        InfoBarSeverity::Informational,
                        t!("cofre-cancelado-nao-mudou"),
                    );
                }
                Err(erro) => tela.avisar(
                    InfoBarSeverity::Error,
                    t!("cofre-nao-consegui-copiar-anexos", erro = erro.to_string()),
                ),
            },
        );
    }

    /// "Juntar com este cofre": traz o que o de fora tem de novo, sem perder nada daqui.
    fn juntar_externo(self: &Rc<Self>) {
        self.trazer_anexos(|tela, pasta, faltam| tela.juntar_externo_agora(pasta, faltam));
    }

    fn juntar_externo_agora(&self, pasta: &Path, faltam: usize) {
        let p = &self.portatil;
        let resultado = {
            let externo = p.externo.borrow();
            let Some(externo) = externo.as_ref() else {
                return;
            };
            self.cofre.mudar(|c| c.juntar_externo(externo))
        };
        match resultado {
            Ok(resumo) => {
                let mut texto = texto_da_juncao(resumo);
                if let Some(copia) = p.em_conflito.borrow_mut().take() {
                    match Cofre::arquivar_copia(&copia) {
                        Ok(depois) => {
                            texto.push(' ');
                            texto.push_str(&t!(
                                "cofre-copia-em-conflito-ficou",
                                caminho = depois.display().to_string()
                            ));
                        }
                        Err(erro) => {
                            texto.push(' ');
                            texto.push_str(&t!(
                                "cofre-nao-guardei-copia-em-conflito",
                                erro = erro.to_string()
                            ));
                        }
                    }
                    self.conferir_cofre();
                }
                texto.push_str(&frase_de_anexos(faltam, pasta));
                self.avisar(
                    if faltam > 0 {
                        InfoBarSeverity::Warning
                    } else {
                        InfoBarSeverity::Success
                    },
                    texto,
                );
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                if self.cofre.agora(|c| c.bloqueado()) {
                    t!("cofre-desbloqueie-para-juntar")
                } else {
                    t!("cofre-nao-consegui-juntar", erro = erro.to_string())
                },
            ),
        }
    }

    /// "Usar no lugar deste": o cofre de agora vira cópia (`.bak-<data>`) e o de fora passa a
    /// ser o cofre do app.
    fn substituir_pelo_externo(self: &Rc<Self>) {
        self.trazer_anexos(|tela, pasta, faltam| tela.substituir_pelo_externo_agora(pasta, faltam));
    }

    fn substituir_pelo_externo_agora(&self, pasta: &Path, faltam: usize) {
        let p = &self.portatil;
        let resultado = {
            let externo = p.externo.borrow();
            let Some(externo) = externo.as_ref() else {
                return;
            };
            self.cofre.mudar(|c| c.substituir_por(externo))
        };
        *p.em_conflito.borrow_mut() = None;
        match resultado {
            Ok(copia) => {
                let guardado = copia.map_or_else(String::new, |c| {
                    format!(
                        " {}",
                        t!(
                            "cofre-o-de-antes-ficou-em",
                            caminho = c.display().to_string()
                        )
                    )
                });
                self.avisar(
                    if faltam > 0 {
                        InfoBarSeverity::Warning
                    } else {
                        InfoBarSeverity::Success
                    },
                    t!(
                        "cofre-passou-a-ser-deste-app",
                        guardado = guardado,
                        anexos = frase_de_anexos(faltam, pasta)
                    ),
                );
                self.desbloquear.set(self.cofre.agora(|c| c.bloqueado()));
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("cofre-nao-consegui-trocar", erro = erro.to_string()),
            ),
        }
    }

    // --- O local do cofre ---

    /// Mudar o local do cofre para `destino` (`None` é a pasta de dados, o padrão). Abre o
    /// diálogo que pergunta se leva o cofre ou usa o que já está lá.
    pub(super) fn pedir_local_do_cofre(&self, destino: Option<PathBuf>) {
        let destino = destino.unwrap_or_else(|| self.armazenamento.pasta().to_owned());
        let atual = self.cofre.agora(|c| c.pasta_do_cofre().to_owned());
        if destino == atual {
            self.avisar(
                InfoBarSeverity::Informational,
                t!("cofre-ja-esta-nessa-pasta"),
            );
            return;
        }
        let p = &self.portatil;
        p.local_com_cofre.set(destino.join("vault.json").exists());
        p.local_texto.set(destino.display().to_string());
        *p.local_destino.borrow_mut() = destino;
        p.local_aberto.set(true);
    }

    /// Grava o novo local na configuração (vazio é a pasta de dados).
    fn gravar_local_do_cofre(&self, destino: &Path) {
        let valor = if destino == self.armazenamento.pasta() {
            String::new()
        } else {
            crate::armazenamento::com_til(destino)
        };
        self.config.update(|c| c.cofre.local = valor);
    }

    /// "Levar o cofre para lá": copia os anexos (em segundo plano), depois o `vault.json` e o
    /// `vault.key`, e passa a usar a pasta. O `vault/` de agora vira `vault.bak/`.
    fn levar_cofre_para_o_local(self: &Rc<Self>) {
        let destino = self.portatil.local_destino.borrow().clone();
        let tela = Rc::clone(self);
        let alvo = destino.join(anexos::PASTA);
        self.copiar_anexos(
            t!("cofre-copiando-anexos-novo-local"),
            self.vault(),
            alvo,
            move |r| match r {
                Ok(_) => tela.concluir_levar_para_o_local(&destino),
                Err(ErroAnexo::Cancelado) => tela.avisar(
                    InfoBarSeverity::Informational,
                    t!("cofre-cancelado-continua"),
                ),
                Err(erro) => tela.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "cofre-nao-consegui-copiar-anexos-para",
                        destino = destino.display().to_string(),
                        erro = erro.to_string()
                    ),
                ),
            },
        );
    }

    fn concluir_levar_para_o_local(&self, destino: &Path) {
        match self.cofre.mudar(|c| c.levar_para(destino)) {
            Ok(()) => {
                self.gravar_local_do_cofre(destino);
                self.avisar(
                    InfoBarSeverity::Success,
                    t!(
                        "cofre-agora-fica-em",
                        destino = destino.display().to_string()
                    ),
                );
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!(
                    "cofre-nao-consegui-levar",
                    destino = destino.display().to_string(),
                    erro = erro.to_string()
                ),
            ),
        }
    }

    /// "Usar o cofre que já está lá" (ou começar um novo, se não há).
    fn usar_cofre_do_local(&self) {
        let destino = self.portatil.local_destino.borrow().clone();
        let (novo, aviso) = Cofre::abrir(&destino);
        self.cofre.mudar(|c| *c = novo);
        self.gravar_local_do_cofre(&destino);
        if self.cofre.agora(|c| c.bloqueado()) {
            self.desbloquear.set(true);
        }
        match aviso {
            Some(aviso) => self.avisar(InfoBarSeverity::Warning, aviso),
            None => self.avisar(
                InfoBarSeverity::Success,
                t!(
                    "cofre-agora-fica-em",
                    destino = destino.display().to_string()
                ),
            ),
        }
    }

    // --- A conferência do disco ---

    /// Liga a conferência do `vault.json` e das cópias em conflito: de tempos em tempos
    /// (`Ambiente.vigia_do_cofre`) e quando a janela ganha o foco.
    pub(super) fn vigiar_cofre(self: &Rc<Self>) {
        self.agendar_vigia_do_cofre();
        let t = Rc::clone(self);
        effect(move || {
            let _ = t.ativa.get();
            untrack(|| t.conferir_cofre());
        });
        // Uma nota alterada nas duas máquinas, achada no meio de um salvamento: a cópia já foi
        // criada, e o aviso sai quando a gravação sobe a versão do cofre.
        let t = Rc::clone(self);
        effect(move || {
            let _ = t.cofre.versao();
            untrack(|| {
                let conflitos = t.cofre.tomar_conflitos();
                if conflitos > 0 {
                    t.avisar(
                        InfoBarSeverity::Warning,
                        t!(
                            "cofre-mudou-em-outra-maquina",
                            conflitos = frase_de_conflitos(conflitos)
                        ),
                    );
                }
            });
        });
    }

    fn agendar_vigia_do_cofre(self: &Rc<Self>) {
        let espera = self.ferramentas.ambiente().vigia_do_cofre;
        let t = Rc::clone(self);
        self.alarmes.depois(espera, move || {
            t.conferir_cofre();
            t.agendar_vigia_do_cofre();
        });
    }

    /// Junta o que outra máquina gravou no `vault.json` e atualiza a lista das cópias em
    /// conflito.
    fn conferir_cofre(&self) {
        let (bloqueado, mudou) = self.cofre.agora(|c| (c.bloqueado(), c.mudou_no_disco()));
        if mudou && !bloqueado {
            match self.cofre.mudar(|c| c.atualizar_do_disco()) {
                Ok(Some(resumo)) if resumo.mudou() => {
                    self.portatil.ultimo_erro.borrow_mut().clear();
                    self.avisar(
                        if resumo.conflitos > 0 {
                            InfoBarSeverity::Warning
                        } else {
                            InfoBarSeverity::Informational
                        },
                        t!(
                            "cofre-mudou-e-atualizado",
                            novos = resumo.novos as i64,
                            atualizados = resumo.atualizados as i64,
                            removidos = resumo.removidos as i64,
                            conflitos = frase_de_conflitos(resumo.conflitos)
                        ),
                    );
                }
                Ok(_) => self.portatil.ultimo_erro.borrow_mut().clear(),
                Err(erro) => {
                    let texto = t!(
                        "cofre-nao-consegui-atualizar-pelo-disco",
                        erro = erro.to_string()
                    );
                    if *self.portatil.ultimo_erro.borrow() != texto {
                        self.avisar(InfoBarSeverity::Warning, texto.clone());
                        *self.portatil.ultimo_erro.borrow_mut() = texto;
                    }
                }
            }
        }
        let conflitos = self.cofre.agora(|c| c.copias_em_conflito());
        if self.portatil.conflitos.get_untracked() != conflitos {
            self.portatil.conflitos.set(conflitos);
        }
    }

    /// "Juntar" do aviso de cópia em conflito: abre a primeira como um cofre de fora.
    fn juntar_copia_em_conflito(&self) {
        let primeira = self.portatil.conflitos.get_untracked().into_iter().next();
        if let Some(caminho) = primeira {
            self.abrir_externo(&caminho, true);
        }
    }
}

/// Os diálogos do cofre entre máquinas: levar, a senha do cofre de fora, as duas escolhas, a
/// confirmação de usar no lugar e o do local.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View + use<> {
    let p = &tela.portatil;

    // Esvazia o cofre de fora (que já foi decifrado) quando nenhum diálogo o usa mais.
    {
        let externo = Rc::clone(&p.externo);
        let refazer = Rc::clone(&p.refazer_senha);
        let (senha, abrir, substituir) = (p.senha_aberta, p.abrir_aberto, p.substituir_aberto);
        let copiando = p.copia.aberto();
        effect(move || {
            let aberto = senha.get() | abrir.get() | substituir.get() | copiando.get();
            if !aberto && !refazer.get() {
                untrack(|| externo.borrow_mut().take());
            }
        });
    }

    Column::new()
        .child(dialogo_levar(tela))
        .child(dialogo_senha_do_externo(tela))
        .child(dialogo_abrir(tela))
        .child(dialogo_substituir(tela))
        .child(dialogo_local(tela))
        .child(
            tela.portatil
                .copia
                .dialogo(t!("cofre-copiando-os-anexos"), "cofre-copia"),
        )
}

fn dialogo_levar(tela: &Rc<Tela>) -> impl View + use<> {
    let p = &tela.portatil;
    let (aberto, senha, com_anexos) = (p.levar_aberto, p.levar_senha, p.levar_com_anexos);
    let tem_mestra = {
        let t = Rc::clone(tela);
        move || t.cofre.ler(|c| c.tem_senha_mestra())
    };
    let sem_mestra = {
        let tem = tem_mestra.clone();
        move || !tem()
    };
    let sem_senha = {
        let sem = sem_mestra.clone();
        move || sem() && senha.get().is_empty()
    };
    let salvar = Rc::clone(tela);
    ContentDialog::new(t!("portatil-levar-o-cofre-para-outra-maquina"), aberto)
        .id("cofre-levar-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(t!("portatil-o-arquivo-leva-o-cofre-inteiro"))
                        .secondary()
                        .wrap(),
                )
                .child(
                    Text::new(t!("portatil-o-arquivo-abre-com-a-senha"))
                        .id("cofre-levar-com-mestra")
                        .caption()
                        .visible(tem_mestra),
                )
                .child(
                    Edit::bind(senha)
                        .id("cofre-levar-senha")
                        .password()
                        .placeholder(t!("portatil-senha-para-o-arquivo-opcional"))
                        .visible(sem_mestra),
                )
                .child(
                    Text::new(t!("portatil-leve-tambem-a-pasta-nome-vault"))
                        .id("cofre-levar-anexos")
                        .caption()
                        .visible(move || com_anexos.get()),
                )
                .child(
                    Text::new(t!("portatil-sem-senha-quem-tiver-o-arquivo"))
                        .id("cofre-levar-aviso")
                        .caption()
                        .visible(sem_senha),
                ),
        )
        .primary(t!("comum-salvar-2"), move || salvar.salvar_cofre_portatil())
        .close(t!("comum-cancelar"))
}

fn dialogo_senha_do_externo(tela: &Rc<Tela>) -> impl View + use<> {
    let p = &tela.portatil;
    let (aberto, senha, errou) = (p.senha_aberta, p.senha, p.senha_errou);
    let refazer = Rc::clone(&p.refazer_senha);
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
    let arquivo = p.arquivo;
    let t = Rc::clone(tela);
    ContentDialog::new(t!("portatil-senha-do-cofre-de-fora"), aberto)
        .id("cofre-abrir-senha-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || t!("cofre-senha-propria", arquivo = arquivo.get()))
                        .secondary()
                        .wrap(),
                )
                .child(
                    Edit::bind(senha)
                        .id("cofre-abrir-senha")
                        .password()
                        .placeholder(t!("portatil-senha-mestra-do-cofre-de-fora")),
                )
                .child(
                    Text::new(t!("portatil-senha-incorreta"))
                        .id("cofre-abrir-senha-erro")
                        .caption()
                        .visible(move || errou.get()),
                ),
        )
        .primary(t!("comum-abrir"), move || t.destravar_externo())
        .close(t!("comum-cancelar"))
}

fn dialogo_abrir(tela: &Rc<Tela>) -> impl View + use<> {
    let p = &tela.portatil;
    let (aberto, substituir, arquivo, resumo) =
        (p.abrir_aberto, p.substituir_aberto, p.arquivo, p.resumo);
    let juntar = Rc::clone(tela);
    ContentDialog::new(t!("portatil-abrir-cofre-de-outra-maquina"), aberto)
        .id("cofre-abrir-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || arquivo.get())
                        .id("cofre-abrir-arquivo")
                        .wrap(),
                )
                .child(
                    Text::new(move || resumo.get())
                        .id("cofre-abrir-resumo")
                        .secondary()
                        .wrap(),
                )
                .child(
                    Text::new(t!("portatil-juntar-traz-o-que-o-outro"))
                        .caption()
                        .wrap(),
                ),
        )
        .primary(t!("portatil-juntar-com-este-cofre"), move || {
            juntar.juntar_externo()
        })
        .secondary(t!("portatil-usar-no-lugar-deste"), move || {
            substituir.set(true)
        })
        .close(t!("comum-cancelar"))
}

fn dialogo_substituir(tela: &Rc<Tela>) -> impl View + use<> {
    let aberto = tela.portatil.substituir_aberto;
    let t = Rc::clone(tela);
    ContentDialog::new(t!("portatil-usar-no-lugar-deste-cofre"), aberto)
        .id("cofre-abrir-substituir-dialogo")
        .content(
            Text::new(t!("portatil-o-cofre-de-agora-vai-para"))
                .id("cofre-abrir-substituir-texto")
                .wrap(),
        )
        .primary(t!("portatil-usar-no-lugar-deste"), move || {
            t.substituir_pelo_externo()
        })
        .close(t!("comum-cancelar"))
}

fn dialogo_local(tela: &Rc<Tela>) -> impl View + use<> {
    let p = &tela.portatil;
    let (aberto, texto, com_cofre) = (p.local_aberto, p.local_texto, p.local_com_cofre);
    let (levar, usar) = (Rc::clone(tela), Rc::clone(tela));
    ContentDialog::new(t!("portatil-mudar-o-local-do-cofre"), aberto)
        .id("cofre-local-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || texto.get())
                        .id("cofre-local-pasta")
                        .wrap(),
                )
                .child(
                    Text::new(move || {
                        if com_cofre.get() {
                            t!("cofre-ja-ha-cofre-nessa-pasta")
                        } else {
                            t!("cofre-nao-ha-cofre-nessa-pasta")
                        }
                    })
                    .id("cofre-local-aviso")
                    .secondary()
                    .wrap(),
                ),
        )
        .primary(t!("portatil-levar-o-cofre-para-la"), move || {
            levar.levar_cofre_para_o_local()
        })
        .secondary(
            move || {
                if com_cofre.get() {
                    t!("cofre-usar-o-que-ja-esta-la")
                } else {
                    t!("cofre-comecar-novo-la")
                }
            },
            move || usar.usar_cofre_do_local(),
        )
        .close(t!("comum-cancelar"))
}

/// O aviso de cópia em conflito do cofre, com o botão que a junta.
pub(super) fn aviso_de_conflito(tela: &Rc<Tela>) -> impl View + use<> {
    let conflitos = tela.portatil.conflitos;
    let t = Rc::clone(tela);
    Row::new()
        .gap(8)
        .align(Align::Center)
        .visible(move || !conflitos.get().is_empty())
        .child(
            InfoBar::new(InfoBarSeverity::Warning)
                .id("cofre-conflito")
                .message(t!("portatil-ha-uma-copia-em-conflito-do"))
                .grow(1),
        )
        .child(
            Button::new(t!("portatil-juntar"))
                .id("cofre-conflito-juntar")
                .on_click(move || t.juntar_copia_em_conflito()),
        )
}
