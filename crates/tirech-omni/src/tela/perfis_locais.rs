//! O editor dos perfis de assistente deste computador (`perfil-local-dialogo`): o nome, o
//! assistente, o executável (as instalações achadas, com a versão, ou "Outro…"), a pasta da
//! conta e os argumentos extras. A lista fica em Configurações › Assistentes deste computador.
//!
//! As instalações são procuradas numa thread ao abrir o editor e ao trocar o assistente: as
//! pastas do `PATH` do shell de login (no Windows, as `pastas_de_programas`) mais as dos
//! instaladores, e o `--version` de cada arquivo achado, com um prazo.
//!
//! O botão do `ContentDialog` fecha a caixa depois de rodar a ação: se o perfil não serve, a
//! caixa volta a abrir com o motivo (o mesmo molde do diálogo do espaço de trabalho).

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, Instant};

use tomni_ui::dialogs;
use tomni_ui::prelude::*;

use super::Tela;
use super::controlar::{EXTENSOES_DO_WINDOWS, linha_do_programa};
use crate::agente::{acordar, ler_contador};
use crate::configuracoes::perfis_locais::{PERFIS_POR_ASSISTENTE, PerfilLocal};
use crate::ferramentas::ExecutorEnviavel;
use crate::ia::{Ferramenta, ficha};
use crate::t;

/// Quanto o `--version` de cada instalação pode demorar.
const PRAZO_DA_VERSAO: Duration = Duration::from_secs(5);

/// As pastas dos instaladores, dentro da pasta pessoal, que o shell de login pode não ter no
/// `PATH`: as mesmas do `EXEC_COM_O_PATH` do "Controlar com".
const PASTAS_DOS_INSTALADORES: [&str; 7] = [
    ".local/bin",
    ".opencode/bin",
    ".npm-global/bin",
    ".bun/bin",
    ".volta/bin",
    ".asdf/shims",
    ".local/share/mise/shims",
];

/// As pastas do sistema que o `EXEC_COM_O_PATH` também põe no fim do `PATH`.
const PASTAS_DO_SISTEMA: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// Uma instalação achada: o caminho e a primeira linha do `--version`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Instalacao {
    pub caminho: String,
    pub versao: Option<String>,
}

impl Instalacao {
    /// O texto da opção na `ComboBox`.
    fn rotulo(&self) -> String {
        match &self.versao {
            Some(versao) => t!(
                "ia-perfil-exe-achado",
                caminho = self.caminho.as_str(),
                versao = versao.as_str()
            ),
            None => self.caminho.clone(),
        }
    }
}

/// O que a thread da busca devolve: o número da busca e as instalações achadas.
type SaidaDaBusca = Arc<Mutex<Option<(u64, Vec<Instalacao>)>>>;

/// O que o editor mostra e guarda enquanto está aberto.
pub(super) struct SinaisPerfil {
    /// Se a caixa está aberta.
    pub aberto: Signal<bool>,
    pub nome: Signal<String>,
    /// O nome do assistente escolhido na `ComboBox`.
    pub ferramenta: Signal<String>,
    /// O texto da opção escolhida do executável.
    pub executavel: Signal<String>,
    /// O caminho do "Outro…".
    pub outro: Signal<String>,
    pub conta: Signal<String>,
    pub argumentos: Signal<String>,
    /// Por que o perfil não serviu, ou vazio.
    pub erro: Signal<String>,
    /// Se a busca das instalações ainda roda.
    pub procurando: Signal<bool>,
    /// Muda quando as opções do executável precisam ser listadas de novo.
    pub versao: Signal<u64>,
    /// O id do perfil que se edita, ou `None` num perfil novo.
    pub editando: Cell<Option<u64>>,
    /// As instalações achadas do assistente escolhido.
    pub achadas: RefCell<Vec<Instalacao>>,
    /// O assistente da última busca (a troca na `ComboBox` começa outra).
    pub buscado: Cell<Option<Ferramenta>>,
    /// O número da última busca: o resultado de uma busca velha não vale.
    pub busca: Cell<u64>,
    /// O resultado da thread: o número da busca e as instalações.
    pub saida: SaidaDaBusca,
    /// A thread avisa por aqui que terminou.
    pub pronto: Signal<u64>,
    /// Se o botão pediu para a caixa abrir de novo (o motivo está em `erro`).
    pub reabrir: Cell<bool>,
}

impl SinaisPerfil {
    pub(super) fn novos() -> Self {
        Self {
            aberto: signal(false),
            nome: signal(String::new()),
            ferramenta: signal(String::new()),
            executavel: signal(String::new()),
            outro: signal(String::new()),
            conta: signal(String::new()),
            argumentos: signal(String::new()),
            erro: signal(String::new()),
            procurando: signal(false),
            versao: signal(0),
            editando: Cell::new(None),
            achadas: RefCell::new(Vec::new()),
            buscado: Cell::new(None),
            busca: Cell::new(0),
            saida: Arc::new(Mutex::new(None)),
            pronto: signal(0),
            reabrir: Cell::new(false),
        }
    }
}

/// Os nomes dos assistentes na `ComboBox`.
fn nomes_dos_assistentes() -> Vec<String> {
    Ferramenta::TODAS
        .iter()
        .map(|f| ficha(*f).nome.to_owned())
        .collect()
}

/// O assistente pelo nome na `ComboBox`.
fn assistente_do_nome(nome: &str) -> Option<Ferramenta> {
    Ferramenta::TODAS
        .into_iter()
        .find(|f| ficha(*f).nome == nome)
}

/// As opções do executável: "Procurar no PATH", as instalações achadas e "Outro…".
fn opcoes_do_executavel(achadas: &[Instalacao]) -> Vec<String> {
    std::iter::once(t!("ia-perfil-exe-path"))
        .chain(achadas.iter().map(Instalacao::rotulo))
        .chain(std::iter::once(t!("ia-perfil-exe-outro")))
        .collect()
}

/// A opção que mostra o `executavel` gravado: "Procurar no PATH" quando vazio, a instalação de
/// mesmo caminho, ou "Outro…".
fn opcao_do_executavel(executavel: &str, achadas: &[Instalacao]) -> String {
    if executavel.trim().is_empty() {
        return t!("ia-perfil-exe-path");
    }
    achadas
        .iter()
        .find(|i| i.caminho == executavel.trim())
        .map_or_else(|| t!("ia-perfil-exe-outro"), Instalacao::rotulo)
}

/// O `executavel` que a opção escolhida grava.
fn executavel_da_opcao(opcao: &str, outro: &str, achadas: &[Instalacao]) -> String {
    if opcao == t!("ia-perfil-exe-outro") {
        return outro.trim().to_owned();
    }
    achadas
        .iter()
        .find(|i| i.rotulo() == opcao)
        .map_or_else(String::new, |i| i.caminho.clone())
}

/// O que a thread da busca precisa.
struct Busca {
    ferramenta: Ferramenta,
    shell: String,
    windows: bool,
    pessoal: Option<PathBuf>,
    pastas_de_programas: Vec<PathBuf>,
    executor: ExecutorEnviavel,
}

impl Busca {
    /// As pastas onde procurar, na ordem: as do `PATH` do shell de login e as dos instaladores
    /// (no Windows, as `pastas_de_programas`).
    fn pastas(&self) -> Vec<PathBuf> {
        if self.windows {
            return self.pastas_de_programas.clone();
        }
        let mut pastas: Vec<PathBuf> =
            (self.executor)(&self.shell, &["-l", "-c", "printf %s \"$PATH\""])
                .map(|path| {
                    path.trim()
                        .split(':')
                        .filter(|p| !p.is_empty())
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default();
        if let Some(pessoal) = &self.pessoal {
            pastas.extend(PASTAS_DOS_INSTALADORES.iter().map(|p| pessoal.join(p)));
        }
        pastas.extend(PASTAS_DO_SISTEMA.iter().map(PathBuf::from));
        pastas
    }

    /// Procura, lê a versão de cada uma (todas ao mesmo tempo, com o prazo) e devolve.
    fn rodar(&self) -> Vec<Instalacao> {
        let arquivos = achar_arquivos(ficha(self.ferramenta).binario, &self.pastas(), self.windows);
        let pedidos: Vec<_> = arquivos
            .iter()
            .map(|arquivo| {
                let linha = linha_do_programa(
                    &arquivo.to_string_lossy(),
                    vec!["--version".to_owned()],
                    Vec::new(),
                    &self.shell,
                    self.windows,
                );
                let (enviar, receber) = mpsc::channel();
                let executor = Arc::clone(&self.executor);
                std::thread::spawn(move || {
                    let argumentos: Vec<&str> =
                        linha.argumentos.iter().map(String::as_str).collect();
                    let _ = enviar.send(executor(&linha.programa, &argumentos));
                });
                receber
            })
            .collect();
        let limite = Instant::now() + PRAZO_DA_VERSAO;
        arquivos
            .into_iter()
            .zip(pedidos)
            .map(|(arquivo, receber)| Instalacao {
                caminho: arquivo.to_string_lossy().into_owned(),
                versao: receber
                    .recv_timeout(limite.saturating_duration_since(Instant::now()))
                    .ok()
                    .and_then(Result::ok)
                    .and_then(|saida| texto_da_versao(&saida)),
            })
            .collect()
    }
}

/// Os arquivos `nome` das `pastas`, sem repetir o mesmo arquivo de verdade (o atalho e o
/// alvo dele, a pasta que aparece duas vezes no `PATH`). No Windows, com as extensões.
pub(super) fn achar_arquivos(nome: &str, pastas: &[PathBuf], windows: bool) -> Vec<PathBuf> {
    let mut vistos = HashSet::new();
    let mut achados = Vec::new();
    for pasta in pastas {
        let candidatos: Vec<PathBuf> = if windows {
            EXTENSOES_DO_WINDOWS
                .iter()
                .map(|ext| pasta.join(format!("{nome}{ext}")))
                .collect()
        } else {
            vec![pasta.join(nome)]
        };
        for candidato in candidatos {
            if !executavel(&candidato) {
                continue;
            }
            let real = std::fs::canonicalize(&candidato).unwrap_or_else(|_| candidato.clone());
            if vistos.insert(real) {
                achados.push(candidato);
            }
        }
    }
    achados
}

/// Se o arquivo existe e roda (no Unix, com a permissão de execução).
fn executavel(arquivo: &Path) -> bool {
    let Ok(dados) = std::fs::metadata(arquivo) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        dados.is_file() && dados.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        dados.is_file()
    }
}

/// A primeira linha não vazia da saída do `--version`, encurtada.
pub(super) fn texto_da_versao(saida: &str) -> Option<String> {
    let linha = saida.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(linha.chars().take(60).collect())
}

impl Tela {
    /// Abre o editor de um perfil novo, do primeiro assistente que ainda tem vaga.
    pub(super) fn novo_perfil_local(self: &Rc<Self>) {
        let ferramenta = self.config.with_untracked(|c| {
            Ferramenta::TODAS
                .into_iter()
                .find(|f| c.ia.perfis_de(*f).count() < PERFIS_POR_ASSISTENTE)
                .unwrap_or_default()
        });
        self.abrir_editor_de_perfil(None, &PerfilLocal::novo(0, ferramenta));
    }

    /// Abre o editor do perfil `id`.
    pub(super) fn editar_perfil_local(self: &Rc<Self>, id: u64) {
        let Some(perfil) = self
            .config
            .with_untracked(|c| c.ia.perfil_local(id).cloned())
        else {
            return;
        };
        self.abrir_editor_de_perfil(Some(id), &perfil);
    }

    fn abrir_editor_de_perfil(self: &Rc<Self>, editando: Option<u64>, perfil: &PerfilLocal) {
        let s = &self.controlar.perfil;
        s.editando.set(editando);
        s.nome.set(perfil.nome.clone());
        s.ferramenta.set(ficha(perfil.ferramenta()).nome.to_owned());
        s.outro.set(perfil.executavel.clone());
        s.conta.set(perfil.conta.clone());
        s.argumentos.set(perfil.argumentos.clone());
        s.erro.set(String::new());
        self.procurar_instalacoes(perfil.ferramenta());
        s.executavel
            .set(opcao_do_executavel(&perfil.executavel, &s.achadas.borrow()));
        s.aberto.set(true);
    }

    /// Começa a busca das instalações de `ferramenta` (a lista fica vazia até ela voltar).
    fn procurar_instalacoes(&self, ferramenta: Ferramenta) {
        let s = &self.controlar.perfil;
        let numero = s.busca.get() + 1;
        s.busca.set(numero);
        s.buscado.set(Some(ferramenta));
        s.achadas.borrow_mut().clear();
        s.versao.update(|v| *v += 1);
        s.procurando.set(true);
        let ambiente = self.ferramentas.ambiente();
        let busca = Busca {
            ferramenta,
            shell: ambiente
                .shell
                .clone()
                .unwrap_or_else(|| "/bin/sh".to_owned()),
            windows: ambiente.windows,
            pessoal: ambiente.pessoal.clone(),
            pastas_de_programas: ambiente.pastas_de_programas.clone(),
            executor: Arc::clone(&ambiente.executor_enviavel),
        };
        let (saida, pronto) = (Arc::clone(&s.saida), s.pronto);
        std::thread::spawn(move || {
            let achadas = busca.rodar();
            *saida.lock().unwrap_or_else(PoisonError::into_inner) = Some((numero, achadas));
            acordar(pronto);
        });
    }

    /// O resultado da busca chegou: a lista se refaz e, se o "Outro…" tem o caminho de uma
    /// instalação achada, a escolha passa para ela.
    fn receber_instalacoes(&self, numero: u64, achadas: Vec<Instalacao>) {
        let s = &self.controlar.perfil;
        if numero != s.busca.get() {
            return;
        }
        let escolha = s.executavel.get_untracked();
        if escolha == t!("ia-perfil-exe-outro") {
            let outro = s.outro.get_untracked();
            if let Some(i) = achadas.iter().find(|i| i.caminho == outro.trim()) {
                s.executavel.set(i.rotulo());
            }
        }
        *s.achadas.borrow_mut() = achadas;
        s.procurando.set(false);
        s.versao.update(|v| *v += 1);
    }

    /// O "Salvar" do editor: confere, grava na configuração e avisa.
    fn salvar_perfil_local(self: &Rc<Self>) {
        let s = &self.controlar.perfil;
        let recusar = |motivo: String| {
            s.erro.set(motivo);
            s.reabrir.set(true);
        };
        let Some(ferramenta) = assistente_do_nome(&s.ferramenta.get_untracked()) else {
            return recusar(t!("ia-perfil-erro-assistente"));
        };
        let editando = s.editando.get();
        let lotado = self.config.with_untracked(|c| {
            c.ia.perfis_de(ferramenta)
                .filter(|p| Some(p.id) != editando)
                .count()
                >= PERFIS_POR_ASSISTENTE
        });
        if lotado {
            return recusar(t!(
                "ia-perfil-erro-limite",
                n = PERFIS_POR_ASSISTENTE as i64,
                assistente = ficha(ferramenta).nome
            ));
        }
        let executavel = executavel_da_opcao(
            &s.executavel.get_untracked(),
            &s.outro.get_untracked(),
            &s.achadas.borrow(),
        );
        let id =
            editando.unwrap_or_else(|| self.config.with_untracked(|c| c.ia.proximo_perfil_local()));
        let perfil = PerfilLocal {
            id,
            nome: s.nome.get_untracked().trim().to_owned(),
            ferramenta: ferramenta.chave().to_owned(),
            executavel,
            conta: s.conta.get_untracked().trim().to_owned(),
            argumentos: s.argumentos.get_untracked().trim().to_owned(),
        };
        let nome = perfil.nome();
        self.config.update(
            |c| match c.ia.perfis_locais.iter().position(|p| p.id == id) {
                Some(i) => c.ia.perfis_locais[i] = perfil,
                None => c.ia.perfis_locais.push(perfil),
            },
        );
        s.erro.set(String::new());
        self.avisar(
            InfoBarSeverity::Success,
            t!("ia-perfil-salvo", nome = nome.as_str()),
        );
    }
}

/// Liga o resultado da busca e a troca do assistente na `ComboBox`.
pub(super) fn ligar(tela: &Rc<Tela>) {
    let s = &tela.controlar.perfil;
    let (pronto, saida) = (s.pronto, Arc::clone(&s.saida));
    {
        let tela = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(pronto);
            let recebida = saida.lock().unwrap_or_else(PoisonError::into_inner).take();
            untrack(|| {
                if let Some((numero, achadas)) = recebida {
                    tela.receber_instalacoes(numero, achadas);
                }
            });
        });
    }
    let (ferramenta, aberto) = (s.ferramenta, s.aberto);
    let tela = Rc::clone(tela);
    effect(move || {
        let nome = ferramenta.get();
        untrack(|| {
            let Some(nova) = assistente_do_nome(&nome) else {
                return;
            };
            let s = &tela.controlar.perfil;
            if !aberto.get_untracked() || s.buscado.get() == Some(nova) {
                return;
            }
            // O executável de um assistente não serve para o outro.
            s.executavel.set(t!("ia-perfil-exe-path"));
            tela.procurar_instalacoes(nova);
        });
    });
}

/// O editor do perfil (`perfil-local-dialogo`). O efeito que reabre a caixa liga uma vez, e
/// a função devolvida monta a caixa em cada janela.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl Fn() -> ContentDialog + use<> {
    let s = &tela.controlar.perfil;
    let (aberto, erro) = (s.aberto, s.erro);
    {
        let tela = Rc::clone(tela);
        effect(move || {
            if aberto.get() {
                return;
            }
            let reabrir = tela.controlar.perfil.reabrir.replace(false);
            untrack(|| {
                if reabrir {
                    aberto.set(true);
                }
            });
        });
    }
    let tela = Rc::clone(tela);
    move || {
        let s = &tela.controlar.perfil;
        let (nome, ferramenta, executavel, outro, conta, argumentos, procurando, versao) = (
            s.nome,
            s.ferramenta,
            s.executavel,
            s.outro,
            s.conta,
            s.argumentos,
            s.procurando,
            s.versao,
        );
        let opcoes = {
            let tela = Rc::clone(&tela);
            // A lista se refaz quando a busca volta.
            For::each(
                move || vec![versao.get()],
                |v| *v,
                move |_| {
                    let opcoes = opcoes_do_executavel(&tela.controlar.perfil.achadas.borrow());
                    ComboBox::bind(executavel, opcoes)
                        .id("perfil-local-executavel")
                        .width(440)
                },
            )
        };
        let escolher_arquivo = move || {
            dialogs::open_file()
                .title(t!("ia-perfil-executavel"))
                .remember("perfil-local-executavel")
                .show(move |caminho| {
                    if let Some(caminho) = caminho {
                        outro.set(caminho.display().to_string());
                    }
                });
        };
        let escolher_pasta = move || {
            dialogs::pick_folder()
                .title(t!("ia-perfil-conta"))
                .remember("perfil-local-conta")
                .show(move |caminho| {
                    if let Some(caminho) = caminho {
                        conta.set(caminho.display().to_string());
                    }
                });
        };
        let salvar = Rc::clone(&tela);
        let editando = Rc::clone(&tela);
        ContentDialog::new(
            move || {
                if editando.controlar.perfil.editando.get().is_some() && aberto.get() {
                    t!("ia-perfil-titulo-editar")
                } else {
                    t!("ia-perfil-titulo-novo")
                }
            },
            aberto,
        )
        .id("perfil-local-dialogo")
        .content(
            Column::new()
                .gap(8)
                .width(460)
                .child(Text::new(t!("ia-perfil-nome")).caption())
                .child(
                    Edit::bind(nome)
                        .id("perfil-local-nome")
                        .placeholder(t!("ia-perfil-nome-dica")),
                )
                .child(Text::new(t!("ia-perfil-assistente")).caption())
                .child(
                    ComboBox::bind(ferramenta, nomes_dos_assistentes())
                        .id("perfil-local-assistente"),
                )
                .child(Text::new(t!("ia-perfil-executavel")).caption())
                .child(opcoes)
                .child(
                    Text::new(t!("ia-perfil-procurando"))
                        .id("perfil-local-procurando")
                        .secondary()
                        .visible(move || procurando.get()),
                )
                .child(
                    Row::new()
                        .gap(8)
                        .visible(move || executavel.get() == t!("ia-perfil-exe-outro"))
                        .child(
                            Edit::bind(outro)
                                .id("perfil-local-outro")
                                .placeholder(t!("ia-perfil-outro-dica"))
                                .width(340),
                        )
                        .child(
                            Button::new(t!("config-escolher"))
                                .id("perfil-local-outro-escolher")
                                .on_click(escolher_arquivo),
                        ),
                )
                .child(Text::new(t!("ia-perfil-conta")).caption())
                .child(
                    Row::new()
                        .gap(8)
                        .child(
                            Edit::bind(conta)
                                .id("perfil-local-conta")
                                .placeholder(t!("ia-perfil-conta-dica"))
                                .width(340),
                        )
                        .child(
                            Button::new(t!("config-escolher"))
                                .id("perfil-local-conta-escolher")
                                .on_click(escolher_pasta),
                        ),
                )
                .child(
                    Text::new(t!("ia-perfil-conta-explicacao"))
                        .secondary()
                        .wrap(),
                )
                .child(Text::new(t!("ia-perfil-argumentos")).caption())
                .child(
                    Edit::bind(argumentos)
                        .id("perfil-local-argumentos")
                        .placeholder(t!("config-dica-argumentos-ia")),
                )
                .child(
                    Text::new(move || erro.get())
                        .id("perfil-local-erro")
                        .caption()
                        .wrap()
                        .visible(move || !erro.get().is_empty()),
                ),
        )
        .primary(t!("comum-salvar"), move || salvar.salvar_perfil_local())
        .close(t!("comum-cancelar"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_versao_e_a_primeira_linha() {
        assert_eq!(
            texto_da_versao("\n  2.1.317 (Claude Code)\nmais\n"),
            Some("2.1.317 (Claude Code)".to_owned())
        );
        assert_eq!(texto_da_versao("  \n"), None);
    }

    #[cfg(unix)]
    #[test]
    fn achar_nao_repete_o_mesmo_arquivo() {
        use std::os::unix::fs::PermissionsExt;
        let raiz = std::env::temp_dir().join(format!("tirech-perfis-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        let (a, b, c) = (raiz.join("a"), raiz.join("b"), raiz.join("c"));
        for pasta in [&a, &b, &c] {
            std::fs::create_dir_all(pasta).unwrap();
        }
        std::fs::write(a.join("claude"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(a.join("claude"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(a.join("claude"), b.join("claude")).unwrap();
        // Sem a permissão de execução não conta.
        std::fs::write(c.join("claude"), "").unwrap();
        let pastas = vec![b.clone(), a.clone(), c, a.clone()];
        assert_eq!(
            achar_arquivos("claude", &pastas, false),
            vec![b.join("claude")]
        );
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn a_opcao_do_executavel_vai_e_volta() {
        let achadas = vec![Instalacao {
            caminho: "/opt/oc/opencode".into(),
            versao: Some("2.2.0".into()),
        }];
        let path = opcao_do_executavel("", &achadas);
        assert_eq!(executavel_da_opcao(&path, "x", &achadas), "");
        let achada = opcao_do_executavel("/opt/oc/opencode", &achadas);
        assert_eq!(achada, achadas[0].rotulo());
        assert_eq!(
            executavel_da_opcao(&achada, "", &achadas),
            "/opt/oc/opencode"
        );
        let outro = opcao_do_executavel("/outro/opencode", &achadas);
        assert_eq!(outro, t!("ia-perfil-exe-outro"));
        assert_eq!(
            executavel_da_opcao(&outro, " /outro/opencode ", &achadas),
            "/outro/opencode"
        );
        assert_eq!(opcoes_do_executavel(&achadas).len(), 3);
    }
}
