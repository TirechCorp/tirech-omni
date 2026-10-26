//! O formulário de um remoto do rclone (M33): Novo e Editar, gerados das opções do provedor, as
//! perguntas da configuração (inclusive o login pelo navegador), as senhas no cofre e a senha da
//! configuração cifrada do rclone.
//!
//! Tudo que fala com o rclone roda numa thread (ver [`super::rclone`]). O formulário não sabe o
//! que cada provedor tem: ele lê as opções do `config/providers` e monta a grade de novo quando
//! o tipo, a caixa das avançadas ou o valor da opção `provider` mudam.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tomni_ui::files::rclone::Rcd;
use tomni_ui::files::rclone::config::{ConfigStep, Provider, ProviderOption, applies};
use tomni_ui::prelude::*;

use super::Tela;
use super::rclone::{Alvo, Chegada, listar};
use crate::cofre::{Campo as CampoDoCofre, Item, Tipo};
use crate::rclone::Falha;
use crate::t;

/// O título do item do cofre com a senha da configuração cifrada do rclone.
const ITEM_SENHA_DA_CONFIG: &str = "Senha da configuração do rclone";

/// O que refazer depois que a senha da configuração cifrada chegou.
// Vive só até a pessoa digitar a senha: pôr o `Alvo` numa `Box` mudaria cada uso sem ganho que
// se note.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub(super) enum Retomar {
    /// Nada: a pessoa repete a operação.
    Nada,
    /// Ler a lista de remotos.
    Listar,
    /// Abrir o remoto na página Arquivos.
    Abrir(Alvo),
    /// Ler os provedores do formulário (e os valores do remoto, se for editar).
    Provedores(Option<String>),
}

/// O que o formulário guarda de uma opção do provedor.
enum Valor {
    Texto(Signal<String>),
    Bool(Signal<bool>),
}

/// Uma opção do provedor e o signal que a grade edita.
struct CampoForm {
    opcao: ProviderOption,
    valor: Valor,
}

/// A pergunta do rclone que o diálogo mostra.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pergunta {
    pub estado: String,
    pub opcao: ProviderOption,
    pub erro: String,
}

/// O que se grava no cofre quando a criação (ou a edição) termina.
#[derive(Clone, Debug)]
struct Pendente {
    nome: String,
    senhas: Vec<(String, String)>,
}

/// O estado do formulário e dos diálogos dele.
pub(super) struct Formulario {
    /// O formulário está na frente da lista.
    pub aberto: Signal<bool>,
    /// O nome do remoto que está sendo editado (`None`: um novo).
    pub editando: Signal<Option<String>>,
    /// O nome do remoto novo.
    pub nome: Signal<String>,
    /// O tipo escolhido, pelo texto da lista.
    pub tipo: Signal<String>,
    /// Mostra as opções avançadas.
    pub avancadas: Signal<bool>,
    /// Guarda as senhas no cofre.
    pub cofre: Signal<bool>,
    /// O erro que o formulário mostra.
    pub erro: Signal<Option<String>>,
    /// Os provedores estão sendo lidos.
    pub carregando: Signal<bool>,
    /// Uma criação ou edição está em andamento.
    pub trabalhando: Signal<bool>,
    /// Os provedores do rclone, pelo texto da lista (ordem alfabética).
    pub provedores: Signal<Vec<(String, Provider)>>,
    /// Sobe a cada vez que as opções são refeitas: a grade nasce de novo.
    pub versao: Signal<u64>,
    campos: RefCell<Vec<CampoForm>>,
    escopos: RefCell<VecDeque<Scope>>,
    assinatura: RefCell<Option<(String, bool, String)>>,
    tipo_construido: RefCell<String>,
    iniciais: RefCell<BTreeMap<String, String>>,
    /// A pergunta aberta.
    pub pergunta: Signal<Option<Pergunta>>,
    /// O diálogo da pergunta está aberto.
    pub pergunta_aberta: Signal<bool>,
    /// O que a pessoa respondeu (o texto, ou o rótulo do exemplo escolhido).
    pub resposta: Signal<String>,
    respondida: Cell<bool>,
    criando: Cell<bool>,
    /// O diálogo "Conclua o login no navegador" está aberto.
    pub login_aberto: Signal<bool>,
    aguardando: Cell<bool>,
    cancelar_login: RefCell<Option<Arc<AtomicBool>>>,
    pendente: RefCell<Option<Pendente>>,
    /// O diálogo da senha da configuração cifrada.
    pub senha_aberta: Signal<bool>,
    pub senha_texto: Signal<String>,
    pub senha_guardar: Signal<bool>,
    pub senha_erro: Signal<Option<String>>,
    retomar: RefCell<Retomar>,
}

impl Formulario {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            editando: signal(None),
            nome: signal(String::new()),
            tipo: signal(String::new()),
            avancadas: signal(false),
            cofre: signal(true),
            erro: signal(None),
            carregando: signal(false),
            trabalhando: signal(false),
            provedores: signal(Vec::new()),
            versao: signal(0),
            campos: RefCell::default(),
            escopos: RefCell::default(),
            assinatura: RefCell::default(),
            tipo_construido: RefCell::default(),
            iniciais: RefCell::default(),
            pergunta: signal(None),
            pergunta_aberta: signal(false),
            resposta: signal(String::new()),
            respondida: Cell::new(false),
            criando: Cell::new(false),
            login_aberto: signal(false),
            aguardando: Cell::new(false),
            cancelar_login: RefCell::default(),
            pendente: RefCell::default(),
            senha_aberta: signal(false),
            senha_texto: signal(String::new()),
            senha_guardar: signal(true),
            senha_erro: signal(None),
            retomar: RefCell::new(Retomar::Nada),
        }
    }

    /// Esquece o que o formulário anterior deixou.
    fn zerar(&self) {
        self.editando.set(None);
        self.nome.set(String::new());
        self.tipo.set(String::new());
        self.avancadas.set(false);
        self.cofre.set(true);
        self.erro.set(None);
        self.trabalhando.set(false);
        self.campos.borrow_mut().clear();
        *self.assinatura.borrow_mut() = None;
        self.tipo_construido.borrow_mut().clear();
        self.iniciais.borrow_mut().clear();
        self.versao.update(|v| *v += 1);
    }

    /// O valor da opção `provider` do formulário agora (vazio se o provedor não tem). Com
    /// `rastrear`, quem chama passa a depender do signal dela.
    fn provider_atual(&self, rastrear: bool) -> String {
        self.campos
            .borrow()
            .iter()
            .find(|c| c.opcao.name == "provider")
            .map(|c| valor_do_campo(c, "", rastrear))
            .unwrap_or_default()
    }

    /// O provedor escolhido na lista.
    fn provedor_escolhido(&self) -> Option<Provider> {
        let tipo = self.tipo.get_untracked();
        self.provedores
            .with_untracked(|v| v.iter().find(|(r, _)| *r == tipo).map(|(_, p)| p.clone()))
    }
}

// ---------------------------------------------------------------------------
// As opções do provedor na grade
// ---------------------------------------------------------------------------

/// A primeira linha não vazia do `Help`.
fn primeira_linha(ajuda: &str) -> String {
    ajuda
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned()
}

/// O texto que a lista mostra para um exemplo.
fn rotulo_do_exemplo(valor: &str, ajuda: &str) -> String {
    match valor {
        "true" => return t!("comum-sim"),
        "false" => return t!("comum-nao"),
        _ => {}
    }
    let linha = primeira_linha(ajuda);
    if valor.is_empty() {
        return if linha.is_empty() {
            t!("comum-vazio-entre-parenteses")
        } else {
            linha
        };
    }
    if linha.is_empty() || linha == valor {
        valor.to_owned()
    } else {
        format!("{valor} — {linha}")
    }
}

/// Os exemplos da opção que valem para o `provider`, como (rótulo, valor).
fn exemplos(opcao: &ProviderOption, provider: &str) -> Vec<(String, String)> {
    let lista: Vec<(String, String)> = opcao
        .examples
        .iter()
        .filter(|e| applies(&e.provider, provider))
        .map(|e| (rotulo_do_exemplo(&e.value, &e.help), e.value.clone()))
        .collect();
    if lista.is_empty() && opcao.kind == "bool" {
        return vec![
            (t!("comum-sim"), "true".to_owned()),
            (t!("comum-nao"), "false".to_owned()),
        ];
    }
    lista
}

/// A opção só aceita um dos exemplos.
fn e_escolha(opcao: &ProviderOption, provider: &str) -> bool {
    opcao.kind != "bool" && opcao.exclusive && !exemplos(opcao, provider).is_empty()
}

/// O valor do rclone que o texto da tela quer dizer.
fn valor_do_texto(opcao: &ProviderOption, provider: &str, texto: &str) -> String {
    exemplos(opcao, provider)
        .into_iter()
        .find(|(rotulo, _)| rotulo == texto)
        .map(|(_, valor)| valor)
        .unwrap_or_else(|| texto.trim().to_owned())
}

/// O texto da tela para o valor do rclone.
fn texto_do_valor(opcao: &ProviderOption, provider: &str, valor: &str) -> String {
    exemplos(opcao, provider)
        .into_iter()
        .find(|(_, v)| v == valor)
        .map(|(rotulo, _)| rotulo)
        .unwrap_or_else(|| valor.to_owned())
}

/// O valor do campo, do jeito que vai para o rclone.
fn valor_do_campo(campo: &CampoForm, provider: &str, rastrear: bool) -> String {
    match &campo.valor {
        Valor::Bool(s) => {
            let ligado = if rastrear { s.get() } else { s.get_untracked() };
            ligado.to_string()
        }
        Valor::Texto(s) => {
            let texto = if rastrear { s.get() } else { s.get_untracked() };
            if e_escolha(&campo.opcao, provider) {
                valor_do_texto(&campo.opcao, provider, &texto)
            } else {
                texto.trim().to_owned()
            }
        }
    }
}

/// O nome da linha da grade.
fn nome_da_linha(opcao: &ProviderOption) -> String {
    if opcao.required {
        t!("rclone-form-obrigatorio", nome = opcao.name.as_str())
    } else {
        opcao.name.clone()
    }
}

/// A descrição da linha: a primeira linha do `Help` (e os exemplos, quando a opção os aceita
/// mas não exige).
fn descricao(opcao: &ProviderOption, provider: &str, editando: bool) -> String {
    let mut texto = primeira_linha(&opcao.help);
    if !opcao.exclusive && opcao.kind != "bool" {
        let valores: Vec<String> = exemplos(opcao, provider)
            .into_iter()
            .map(|(_, v)| v)
            .filter(|v| !v.is_empty())
            .take(5)
            .collect();
        if !valores.is_empty() {
            texto.push(' ');
            texto.push_str(&t!("rclone-form-exemplos", lista = valores.join(", ")));
        }
    }
    if opcao.is_password && editando {
        texto.push(' ');
        texto.push_str(&t!("rclone-form-deixe-em-branco"));
    }
    texto
}

/// O valor inicial de uma opção, em texto da tela.
fn inicial_do_texto(
    opcao: &ProviderOption,
    provider: &str,
    preservado: Option<&String>,
    do_remoto: Option<&String>,
) -> String {
    if let Some(texto) = preservado {
        return texto.clone();
    }
    if opcao.is_password {
        return String::new();
    }
    let valor = do_remoto.unwrap_or(&opcao.default);
    texto_do_valor(opcao, provider, valor)
}

/// `true` se a opção entra no formulário com o `provider` e a caixa das avançadas de agora.
fn aparece(opcao: &ProviderOption, provider: &str, avancadas: bool) -> bool {
    !opcao.hidden() && (avancadas || !opcao.advanced) && opcao.applies_to(provider)
}

fn adicionar(
    grade: PropertyGrid,
    campo: &CampoForm,
    provider: &str,
    editando: bool,
) -> PropertyGrid {
    let nome = nome_da_linha(&campo.opcao);
    let grade = match &campo.valor {
        Valor::Bool(s) => {
            let s = *s;
            grade.check(nome, move || s)
        }
        Valor::Texto(s) => {
            let s = *s;
            if e_escolha(&campo.opcao, provider) {
                let rotulos: Vec<String> = exemplos(&campo.opcao, provider)
                    .into_iter()
                    .map(|(r, _)| r)
                    .collect();
                grade.choice(nome, move || s, rotulos)
            } else if campo.opcao.is_password {
                grade.password(nome, move || s)
            } else {
                grade.text(nome, move || s)
            }
        }
    };
    grade.description(descricao(&campo.opcao, provider, editando))
}

// ---------------------------------------------------------------------------
// A tela
// ---------------------------------------------------------------------------

/// O texto do tipo na lista: a descrição do provedor, com o nome quando duas se repetem.
fn rotulos_dos_tipos(provedores: Vec<Provider>) -> Vec<(String, Provider)> {
    let mut lista: Vec<(String, Provider)> = provedores
        .iter()
        .map(|p| {
            let repetida = provedores
                .iter()
                .filter(|o| o.description == p.description)
                .count()
                > 1;
            let rotulo = match (p.description.is_empty(), repetida) {
                (true, _) => p.name.clone(),
                (false, true) => format!("{} ({})", p.description, p.name),
                (false, false) => p.description.clone(),
            };
            (rotulo, p.clone())
        })
        .collect();
    lista.sort_by_key(|(r, _)| r.to_lowercase());
    lista
}

/// O nome é aceito pelo rclone (e pela tela): letras, números, `-`, `_`, `.` e espaço no meio.
fn nome_valido(nome: &str) -> bool {
    !nome.is_empty()
        && nome == nome.trim()
        && nome
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
}

impl Tela {
    /// Liga os efeitos do formulário: refazer as opções e fechar as perguntas.
    pub(super) fn ligar_formulario(self: &Rc<Self>) {
        let tela = Rc::clone(self);
        effect(move || tela.refazer_opcoes());

        let tela = Rc::clone(self);
        effect(move || {
            if tela.rclone.form.pergunta_aberta.get() {
                return;
            }
            untrack(|| {
                let f = &tela.rclone.form;
                if f.pergunta.with_untracked(Option::is_some) && !f.respondida.get() {
                    tela.cancelar_pergunta();
                }
            });
        });

        let tela = Rc::clone(self);
        effect(move || {
            if tela.rclone.form.login_aberto.get() {
                return;
            }
            untrack(|| {
                let f = &tela.rclone.form;
                if f.aguardando.replace(false)
                    && let Some(cancelar) = f.cancelar_login.borrow().as_ref()
                {
                    cancelar.store(true, Ordering::SeqCst);
                }
            });
        });
    }

    /// Monta de novo as opções quando o tipo, a caixa das avançadas ou o valor da opção
    /// `provider` mudam.
    fn refazer_opcoes(&self) {
        let f = &self.rclone.form;
        let tipo = f.tipo.get();
        let avancadas = f.avancadas.get();
        let provedores = f.provedores.get();
        let provider = f.provider_atual(true);
        let Some((_, provedor)) = provedores.iter().find(|(r, _)| *r == tipo) else {
            if !f.campos.borrow().is_empty() {
                f.campos.borrow_mut().clear();
                *f.assinatura.borrow_mut() = None;
                f.versao.update(|v| *v += 1);
            }
            return;
        };
        let assinatura = (provedor.name.clone(), avancadas, provider.clone());
        if f.assinatura.borrow().as_ref() == Some(&assinatura) {
            return;
        }
        untrack(|| {
            let mesmo_tipo = *f.tipo_construido.borrow() == tipo;
            let preservados: BTreeMap<String, String> = if mesmo_tipo {
                f.campos
                    .borrow()
                    .iter()
                    .map(|c| {
                        let texto = match &c.valor {
                            Valor::Bool(s) => s.get_untracked().to_string(),
                            Valor::Texto(s) => s.get_untracked(),
                        };
                        (c.opcao.name.clone(), texto)
                    })
                    .collect()
            } else {
                BTreeMap::new()
            };
            let iniciais = f.iniciais.borrow().clone();
            let escopo = Scope::detached();
            let campos: Vec<CampoForm> = escopo.run(|| {
                provedor
                    .options
                    .iter()
                    .filter(|o| aparece(o, &provider, avancadas))
                    .map(|o| {
                        let preservado = preservados.get(&o.name);
                        let do_remoto = iniciais.get(&o.name);
                        let valor = if o.kind == "bool" {
                            let texto = preservado
                                .or(do_remoto)
                                .map(String::as_str)
                                .unwrap_or(o.default.as_str());
                            Valor::Bool(signal(texto == "true"))
                        } else {
                            Valor::Texto(signal(inicial_do_texto(
                                o, &provider, preservado, do_remoto,
                            )))
                        };
                        CampoForm {
                            opcao: o.clone(),
                            valor,
                        }
                    })
                    .collect()
            });
            let mut escopos = f.escopos.borrow_mut();
            escopos.push_back(escopo);
            while escopos.len() > 2 {
                if let Some(velho) = escopos.pop_front() {
                    velho.clear();
                }
            }
            *f.campos.borrow_mut() = campos;
            *f.assinatura.borrow_mut() = Some(assinatura);
            *f.tipo_construido.borrow_mut() = tipo;
            f.versao.update(|v| *v += 1);
        });
        // Passa a depender do signal novo da opção `provider`.
        let _ = f.provider_atual(true);
    }

    /// "Novo": o formulário vazio.
    pub(super) fn novo_remoto_rclone(&self) {
        let f = &self.rclone.form;
        f.zerar();
        f.aberto.set(true);
        self.carregar_provedores(None);
    }

    /// "Editar": o formulário com os valores do remoto escolhido.
    pub(super) fn editar_remoto_rclone(&self) {
        let Some(nome) = self.remoto_escolhido() else {
            return;
        };
        let f = &self.rclone.form;
        f.zerar();
        f.nome.set(nome.clone());
        f.editando.set(Some(nome.clone()));
        f.aberto.set(true);
        self.carregar_provedores(Some(nome));
    }

    /// Lê os provedores (e, ao editar, os valores do remoto).
    pub(super) fn carregar_provedores(&self, editando: Option<String>) {
        self.rclone.form.carregando.set(true);
        self.em_thread_rclone(move |rcds| {
            let resultado = rcds
                .obter(None)
                .and_then(|rcd| {
                    let provedores = rcd.config_providers()?;
                    let valores = match &editando {
                        Some(nome) => rcd.config_get(nome)?,
                        None => BTreeMap::new(),
                    };
                    Ok((provedores, valores))
                })
                .map_err(|e| Falha::de(&e));
            Chegada::Provedores(editando, resultado)
        });
    }

    /// Os provedores chegaram.
    pub(super) fn aplicar_provedores(
        &self,
        editando: Option<String>,
        resultado: Result<(Vec<Provider>, BTreeMap<String, String>), Falha>,
    ) {
        let f = &self.rclone.form;
        f.carregando.set(false);
        // Se a pessoa já fechou ou trocou o formulário, a resposta velha não vale.
        if !f.aberto.get_untracked() || f.editando.get_untracked() != editando {
            return;
        }
        match resultado {
            Ok((provedores, mut valores)) => {
                let tipo = valores.remove("type");
                *f.iniciais.borrow_mut() = valores;
                let lista = rotulos_dos_tipos(provedores);
                if let Some(tipo) = tipo {
                    match lista.iter().find(|(_, p)| p.name == tipo) {
                        Some((rotulo, _)) => f.tipo.set(rotulo.clone()),
                        None => f.erro.set(Some(t!(
                            "rclone-form-tipo-desconhecido",
                            tipo = tipo.as_str()
                        ))),
                    }
                }
                f.provedores.set(lista);
            }
            Err(falha) => {
                if matches!(falha, Falha::SenhaDaConfig(_)) {
                    self.falha_do_rclone(falha, Retomar::Provedores(editando));
                } else {
                    let texto = falha.texto().to_owned();
                    self.falha_do_rclone(falha, Retomar::Nada);
                    f.erro.set(Some(texto));
                }
            }
        }
    }

    /// "Salvar": confere o formulário e manda criar (ou atualizar) o remoto.
    pub(super) fn salvar_remoto(&self) {
        let f = &self.rclone.form;
        let editando = f.editando.get_untracked();
        let Some(provedor) = f.provedor_escolhido() else {
            f.erro.set(Some(t!("rclone-form-escolha-o-tipo")));
            return;
        };
        let nome = f.nome.get_untracked().trim().to_owned();
        if editando.is_none() {
            if !nome_valido(&nome) {
                f.erro.set(Some(t!("rclone-form-nome-invalido")));
                return;
            }
            if self
                .rclone
                .remotos
                .with_untracked(|r| r.iter().any(|x| x.name == nome))
            {
                f.erro
                    .set(Some(t!("rclone-form-ja-existe", nome = nome.as_str())));
                return;
            }
        }
        let provider = f.provider_atual(false);
        let originais = f.iniciais.borrow().clone();
        let mut parametros: Vec<(String, String)> = Vec::new();
        let mut senhas: Vec<(String, String)> = Vec::new();
        let mut faltando: Vec<String> = Vec::new();
        for campo in f.campos.borrow().iter() {
            let o = &campo.opcao;
            let valor = valor_do_campo(campo, &provider, false);
            if o.is_password && !valor.is_empty() {
                senhas.push((o.name.clone(), valor.clone()));
            }
            let antes = originais.get(&o.name);
            if o.required
                && valor.is_empty()
                && o.default.is_empty()
                && !(editando.is_some() && (o.is_password || antes.is_some_and(|a| !a.is_empty())))
            {
                faltando.push(o.name.clone());
                continue;
            }
            let enviar = if editando.is_some() {
                if o.is_password {
                    !valor.is_empty()
                } else {
                    valor != *antes.unwrap_or(&o.default)
                }
            } else {
                valor != o.default && !(valor.is_empty() && o.default.is_empty())
            };
            if enviar {
                parametros.push((o.name.clone(), valor));
            }
        }
        if !faltando.is_empty() {
            f.erro.set(Some(t!(
                "rclone-form-preencha",
                lista = faltando.join(", ")
            )));
            return;
        }
        f.erro.set(None);
        let guardar = f.cofre.get_untracked();
        let criando = editando.is_none();
        *f.pendente.borrow_mut() = Some(Pendente {
            nome: nome.clone(),
            senhas: if guardar { senhas } else { Vec::new() },
        });
        if !criando && parametros.is_empty() {
            self.concluir_remoto(&nome, false);
            return;
        }
        f.trabalhando.set(true);
        f.criando.set(criando);
        let tipo = provedor.name.clone();
        self.passo_em_thread(nome.clone(), criando, false, None, move |rcd| {
            if criando {
                rcd.config_create(&nome, &tipo, &parametros)
            } else {
                rcd.config_update(&nome, &parametros)
            }
        });
    }

    /// Chama o rclone numa thread e devolve o passo seguinte da configuração. Se a chamada
    /// falhar depois da criação (`limpar`) ou for cancelada, o remoto inacabado é apagado.
    fn passo_em_thread(
        &self,
        nome: String,
        criando: bool,
        limpar: bool,
        cancelar: Option<Arc<AtomicBool>>,
        chamada: impl FnOnce(&Rcd) -> Result<ConfigStep, tomni_ui::files::rclone::RcloneError>
        + Send
        + 'static,
    ) {
        self.em_thread_rclone(move |rcds| {
            let rcd = match rcds.obter(None) {
                Ok(rcd) => rcd,
                Err(e) => {
                    return Chegada::Passo {
                        nome,
                        criando,
                        cancelado: false,
                        resultado: Err(Falha::de(&e)),
                    };
                }
            };
            let resultado = chamada(&rcd);
            let cancelado = cancelar.is_some_and(|c| c.load(Ordering::SeqCst));
            if criando && (cancelado || (limpar && resultado.is_err())) {
                let _ = rcd.config_delete(&nome);
            }
            Chegada::Passo {
                nome,
                criando,
                cancelado,
                resultado: resultado.map_err(|e| Falha::de(&e)),
            }
        });
    }

    /// O rclone respondeu a um passo da configuração.
    pub(super) fn aplicar_passo(
        &self,
        nome: String,
        criando: bool,
        cancelado: bool,
        resultado: Result<ConfigStep, Falha>,
    ) {
        let f = &self.rclone.form;
        f.aguardando.set(false);
        f.login_aberto.set(false);
        *f.cancelar_login.borrow_mut() = None;
        if cancelado {
            f.trabalhando.set(false);
            f.pergunta.set(None);
            self.listar_remotos_com(Some(t!("rclone-form-login-cancelado")));
            return;
        }
        match resultado {
            Ok(ConfigStep::Done) => {
                f.pergunta.set(None);
                self.concluir_remoto(&nome, criando);
            }
            Ok(ConfigStep::Question {
                state,
                option,
                error,
            }) => {
                f.trabalhando.set(false);
                let provider = "";
                f.resposta.set(inicial_da_pergunta(&option, provider));
                f.respondida.set(false);
                f.criando.set(criando);
                f.pergunta.set(Some(Pergunta {
                    estado: state,
                    opcao: option,
                    erro: error,
                }));
                f.pergunta_aberta.set(true);
            }
            Err(falha) => {
                f.trabalhando.set(false);
                f.pergunta.set(None);
                if matches!(falha, Falha::SenhaDaConfig(_)) {
                    self.falha_do_rclone(falha, Retomar::Nada);
                } else {
                    let texto = falha.texto().to_owned();
                    self.falha_do_rclone(falha, Retomar::Nada);
                    f.erro.set(Some(texto));
                }
            }
        }
    }

    /// "Continuar" no diálogo da pergunta: manda a resposta ao rclone.
    pub(super) fn responder_pergunta(&self) {
        let f = &self.rclone.form;
        f.respondida.set(true);
        let Some(pergunta) = f.pergunta.get_untracked() else {
            return;
        };
        let nome = match self.nome_do_formulario() {
            Some(nome) => nome,
            None => return,
        };
        let texto = f.resposta.get_untracked();
        let valor = valor_do_texto(&pergunta.opcao, "", &texto);
        let login = pergunta.opcao.name == "config_is_local" && valor == "true";
        let cancelar = login.then(|| Arc::new(AtomicBool::new(false)));
        if let Some(c) = &cancelar {
            *f.cancelar_login.borrow_mut() = Some(Arc::clone(c));
            f.aguardando.set(true);
            f.login_aberto.set(true);
        }
        f.trabalhando.set(true);
        let criando = f.criando.get();
        let estado = pergunta.estado;
        self.passo_em_thread(nome.clone(), criando, true, cancelar, move |rcd| {
            rcd.config_answer(&nome, &estado, &valor)
        });
    }

    /// O nome do remoto que o formulário cria ou edita.
    fn nome_do_formulario(&self) -> Option<String> {
        let f = &self.rclone.form;
        f.pendente
            .borrow()
            .as_ref()
            .map(|p| p.nome.clone())
            .or_else(|| f.editando.get_untracked())
    }

    /// A pessoa fechou o diálogo da pergunta sem responder: apaga o remoto inacabado.
    fn cancelar_pergunta(&self) {
        let f = &self.rclone.form;
        f.pergunta.set(None);
        f.trabalhando.set(false);
        if !f.criando.get() {
            return;
        }
        let Some(nome) = self.nome_do_formulario() else {
            return;
        };
        self.em_thread_rclone(move |rcds| {
            let apagado = rcds
                .obter(None)
                .and_then(|rcd| rcd.config_delete(&nome))
                .map_err(|e| Falha::de(&e));
            match apagado {
                Ok(()) => Chegada::Remotos(listar(&rcds), Some(t!("rclone-form-config-cancelada"))),
                Err(falha) => Chegada::Remotos(Err(falha), None),
            }
        });
    }

    /// O remoto está pronto: guarda as senhas no cofre, fecha o formulário e lê a lista.
    fn concluir_remoto(&self, nome: &str, criando: bool) {
        let f = &self.rclone.form;
        f.trabalhando.set(false);
        let pendente = f.pendente.borrow_mut().take();
        let mut texto = if criando {
            t!("rclone-form-remoto-criado", nome = nome)
        } else {
            t!("rclone-form-remoto-atualizado", nome = nome)
        };
        if let Some(p) = pendente
            && !p.senhas.is_empty()
        {
            match self.guardar_senhas_do_remoto(nome, &p.senhas) {
                Ok(()) => texto.push_str(&t!("rclone-form-senhas-no-cofre")),
                Err(motivo) => texto.push_str(&t!(
                    "rclone-form-senhas-fora-do-cofre",
                    motivo = motivo.as_str()
                )),
            }
        }
        f.aberto.set(false);
        self.listar_remotos_com(Some(texto));
    }

    /// Cria ou atualiza o item `rclone: {nome}` do cofre, um campo por senha.
    fn guardar_senhas_do_remoto(
        &self,
        nome: &str,
        senhas: &[(String, String)],
    ) -> Result<(), String> {
        if self.cofre.agora(|c| c.bloqueado()) {
            return Err(t!("comum-o-cofre-esta-bloqueado"));
        }
        let titulo = format!("rclone: {nome}");
        let existente = self.cofre.agora(|c| {
            c.itens()
                .iter()
                .find(|i| i.tipo == Tipo::Rclone && i.titulo == titulo)
                .cloned()
        });
        let mut item = existente.unwrap_or_else(|| Item {
            tipo: Tipo::Rclone,
            titulo,
            ..Item::default()
        });
        for (opcao, valor) in senhas {
            match item.campos.iter_mut().find(|c| c.nome == *opcao) {
                Some(campo) => campo.valor.clone_from(valor),
                None => item.campos.push(CampoDoCofre {
                    nome: opcao.clone(),
                    valor: valor.clone(),
                    oculto: true,
                }),
            }
        }
        self.cofre
            .mudar(|c| {
                if item.id == 0 {
                    item.pasta = c.garantir_caminho(None, &[t!("comum-conexoes").as_str()]);
                }
                c.salvar(item)
            })
            .map(drop)
            .map_err(|e| e.to_string())
    }

    // -----------------------------------------------------------------------
    // A senha da configuração cifrada
    // -----------------------------------------------------------------------

    /// A configuração do rclone é cifrada: usa a senha do cofre ou pede ao usuário.
    pub(super) fn pedir_senha_da_config(&self, retomar: Retomar) {
        let Some(rcds) = self.opener.rcds() else {
            return;
        };
        let f = &self.rclone.form;
        let errada = rcds.tem_senha();
        if !errada && let Some(senha) = self.senha_da_config_do_cofre() {
            rcds.definir_senha(Some(senha));
            self.retomar_rclone(retomar);
            return;
        }
        // A senha que a pessoa digitou (ou o cofre deu) não serviu.
        rcds.definir_senha(None);
        *f.retomar.borrow_mut() = retomar;
        f.senha_texto.set(String::new());
        f.senha_erro
            .set(errada.then(|| t!("rclone-form-senha-da-config-errada")));
        f.senha_aberta.set(true);
    }

    /// O item do cofre com a senha da configuração: o de `rclone.credencial` ou, sem ele, o
    /// que o app guardou com o título de sempre.
    fn senha_da_config_do_cofre(&self) -> Option<String> {
        let escolhido = self.config.with_untracked(|c| c.rclone.credencial);
        self.cofre
            .agora(|c| {
                escolhido
                    .and_then(|id| c.item(id))
                    .or_else(|| {
                        c.itens()
                            .iter()
                            .find(|i| i.tipo == Tipo::Rclone && i.titulo == ITEM_SENHA_DA_CONFIG)
                    })
                    .map(|i| i.senha.clone())
            })
            .filter(|s| !s.is_empty())
    }

    /// "Continuar" no diálogo da senha da configuração.
    pub(super) fn confirmar_senha_da_config(&self) {
        let f = &self.rclone.form;
        let senha = f.senha_texto.get_untracked();
        let Some(rcds) = self.opener.rcds() else {
            return;
        };
        if senha.is_empty() {
            f.senha_erro
                .set(Some(t!("rclone-form-digite-a-senha-da-config")));
            f.senha_aberta.set(true);
            return;
        }
        rcds.definir_senha(Some(senha.clone()));
        if f.senha_guardar.get_untracked() {
            let guardado = if self.cofre.agora(|c| c.bloqueado()) {
                Err(t!("comum-o-cofre-esta-bloqueado"))
            } else {
                let escolhido = self.config.with_untracked(|c| c.rclone.credencial);
                let existente = self.cofre.agora(|c| {
                    escolhido
                        .and_then(|id| c.item(id))
                        .or_else(|| {
                            c.itens().iter().find(|i| {
                                i.tipo == Tipo::Rclone && i.titulo == ITEM_SENHA_DA_CONFIG
                            })
                        })
                        .cloned()
                });
                let mut item = existente.unwrap_or_else(|| Item {
                    tipo: Tipo::Rclone,
                    titulo: ITEM_SENHA_DA_CONFIG.to_owned(),
                    ..Item::default()
                });
                item.senha = senha;
                self.cofre
                    .mudar(|c| {
                        if item.id == 0 {
                            item.pasta = c.garantir_caminho(None, &[t!("comum-conexoes").as_str()]);
                        }
                        c.salvar(item)
                    })
                    .map(|id| {
                        self.config.update(|c| c.rclone.credencial = Some(id));
                    })
                    .map_err(|e| e.to_string())
            };
            if let Err(motivo) = guardado {
                self.avisar(
                    InfoBarSeverity::Warning,
                    t!("rclone-form-senha-fora-do-cofre", motivo = motivo.as_str()),
                );
            }
        }
        let retomar = std::mem::replace(&mut *f.retomar.borrow_mut(), Retomar::Nada);
        self.retomar_rclone(retomar);
    }

    fn retomar_rclone(&self, retomar: Retomar) {
        match retomar {
            Retomar::Nada => self
                .rclone
                .aviso
                .set(Some((false, t!("rclone-form-senha-aceita")))),
            Retomar::Listar => self.listar_remotos(),
            Retomar::Abrir(alvo) => self.abrir_alvo(alvo),
            Retomar::Provedores(editando) => self.carregar_provedores(editando),
        }
    }
}

/// A resposta que o diálogo já mostra quando a pergunta chega.
fn inicial_da_pergunta(opcao: &ProviderOption, provider: &str) -> String {
    let lista = exemplos(opcao, provider);
    if !opcao.default.is_empty() {
        return texto_do_valor(opcao, provider, &opcao.default);
    }
    if opcao.kind == "bool" || (opcao.exclusive && !lista.is_empty()) {
        return lista.first().map(|(r, _)| r.clone()).unwrap_or_default();
    }
    String::new()
}

// ---------------------------------------------------------------------------
// As views
// ---------------------------------------------------------------------------

/// A página do formulário.
pub(super) fn pagina(tela: &Rc<Tela>) -> Column {
    let f = &tela.rclone.form;
    let (aberto, editando, erro, carregando, trabalhando) =
        (f.aberto, f.editando, f.erro, f.carregando, f.trabalhando);
    let grade = {
        let t = Rc::clone(tela);
        For::each(move || vec![t.rclone.form.versao.get()], |v| *v, {
            let t = Rc::clone(tela);
            move |_| t.montar_grade()
        })
    };
    let (t_salvar, t_cancelar) = (Rc::clone(tela), Rc::clone(tela));
    Column::new()
        .id("rclone-form")
        .grow(1)
        .gap(8)
        .padding(Insets::new(8.0, 8.0, 8.0, 8.0))
        .child(
            Row::new()
                .gap(8)
                .child(
                    Button::new(t!("comum-voltar"))
                        .id("rclone-form-voltar")
                        .icon(Symbol::ArrowLeft)
                        .on_click(move || t_cancelar.rclone.form.aberto.set(false)),
                )
                .child(
                    Text::new(move || {
                        if editando.with(Option::is_some) {
                            t!("rclone-form-editar-o-remoto")
                        } else {
                            t!("rclone-form-novo-remoto")
                        }
                    })
                    .title(),
                ),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("rclone-form-erro")
                .message(move || erro.get().unwrap_or_default())
                .visible(move || erro.with(Option::is_some))
                .on_close(move || erro.set(None)),
        )
        .child(
            Text::new(t!("rclone-lendo-os-tipos-de-remoto-do"))
                .id("rclone-form-carregando")
                .secondary()
                .visible(move || carregando.get()),
        )
        .child(grade)
        .child(
            CheckBox::bind(f.avancadas, t!("rclone-mostrar-as-opcoes-avancadas"))
                .id("rclone-form-avancadas"),
        )
        .child(
            CheckBox::bind(f.cofre, t!("rclone-guardar-as-senhas-no-cofre"))
                .id("rclone-form-cofre"),
        )
        .child(
            Row::new()
                .gap(8)
                .child(
                    Button::new(t!("comum-salvar"))
                        .id("rclone-form-salvar")
                        .accent()
                        .icon(Symbol::Save)
                        .disabled(move || carregando.get() || trabalhando.get())
                        .on_click(move || t_salvar.salvar_remoto()),
                )
                .child(
                    ProgressRing::new()
                        .id("rclone-form-andamento")
                        .width(24)
                        .height(24)
                        .visible(move || trabalhando.get()),
                ),
        )
        .visible(move || aberto.get())
}

impl Tela {
    /// A grade do formulário com as opções de agora.
    fn montar_grade(&self) -> PropertyGrid {
        let f = &self.rclone.form;
        let (nome, tipo, editando, provedores) = (f.nome, f.tipo, f.editando, f.provedores);
        let provider = f.provider_atual(false);
        let editando_agora = editando.with_untracked(Option::is_some);
        let mut grade = PropertyGrid::new()
            .id("rclone-form-grade")
            .grow(1)
            .category(t!("comum-remoto"))
            .text(t!("comum-nome"), move || nome)
            .disabled(move || editando.with(Option::is_some))
            .description(t!("rclone-como-o-remoto-se-chama-no"))
            .choice_from(
                t!("comum-tipo"),
                move || tipo,
                move || provedores.with(|v| v.iter().map(|(r, _)| r.clone()).collect()),
            )
            .disabled(move || editando.with(Option::is_some))
            .description(t!("rclone-o-servico-de-nuvem-ou-protocolo"));
        let campos = f.campos.borrow();
        let basicos: Vec<&CampoForm> = campos.iter().filter(|c| !c.opcao.advanced).collect();
        let avancados: Vec<&CampoForm> = campos.iter().filter(|c| c.opcao.advanced).collect();
        if !basicos.is_empty() {
            grade = grade.category(t!("rclone-opcoes"));
            for campo in basicos {
                grade = adicionar(grade, campo, &provider, editando_agora);
            }
        }
        if !avancados.is_empty() {
            grade = grade.category(t!("rclone-avancadas"));
            for campo in avancados {
                grade = adicionar(grade, campo, &provider, editando_agora);
            }
        }
        grade
    }
}

/// O controle da resposta de uma pergunta: a lista dos exemplos, a senha ou o texto.
fn controle_da_pergunta(tela: &Rc<Tela>, pergunta: &Pergunta) -> Column {
    let resposta = tela.rclone.form.resposta;
    let opcao = &pergunta.opcao;
    let lista = exemplos(opcao, "");
    if !lista.is_empty() {
        let rotulos: Vec<String> = lista.into_iter().map(|(r, _)| r).collect();
        Column::new().child(ComboBox::bind(resposta, rotulos).id("rclone-pergunta-escolha"))
    } else if opcao.is_password {
        Column::new().child(Edit::bind(resposta).password().id("rclone-pergunta-texto"))
    } else {
        Column::new().child(Edit::bind(resposta).id("rclone-pergunta-texto"))
    }
}

/// Os diálogos do formulário: a pergunta, o login no navegador e a senha da configuração.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View {
    let f = &tela.rclone.form;
    let pergunta = f.pergunta;
    let conteudo = {
        let t = Rc::clone(tela);
        Column::new()
            .gap(8)
            .child(
                Text::new(move || {
                    pergunta.with(|p| {
                        p.as_ref()
                            .map(|p| p.opcao.help.trim().to_owned())
                            .unwrap_or_default()
                    })
                })
                .id("rclone-pergunta-ajuda")
                .wrap(),
            )
            .child(
                Text::new(t!("rclone-sim-abre-o-navegador-nesta-maquina"))
                    .id("rclone-pergunta-login")
                    .secondary()
                    .wrap()
                    .visible(move || {
                        pergunta.with(|p| {
                            p.as_ref()
                                .is_some_and(|p| p.opcao.name == "config_is_local")
                        })
                    }),
            )
            .child(
                Text::new(move || {
                    pergunta.with(|p| p.as_ref().map(|p| p.erro.clone()).unwrap_or_default())
                })
                .id("rclone-pergunta-erro")
                .wrap()
                .visible(move || pergunta.with(|p| p.as_ref().is_some_and(|p| !p.erro.is_empty()))),
            )
            .child(For::each(
                move || pergunta.get().into_iter().collect::<Vec<_>>(),
                |p: &Pergunta| p.estado.clone(),
                move |p: Pergunta| controle_da_pergunta(&t, &p),
            ))
    };
    let (t_continuar,) = (Rc::clone(tela),);
    let dialogo_pergunta = ContentDialog::new(t!("rclone-o-rclone-quer-saber"), f.pergunta_aberta)
        .id("rclone-pergunta-dialogo")
        .content(conteudo)
        .primary(t!("comum-continuar"), move || {
            t_continuar.responder_pergunta()
        })
        .close(t!("comum-cancelar"));

    let login = ContentDialog::new(t!("rclone-conclua-o-login-no-navegador"), f.login_aberto)
        .id("rclone-login-dialogo")
        .content(
            Row::new()
                .gap(12)
                .child(
                    ProgressRing::new()
                        .id("rclone-login-andamento")
                        .width(32)
                        .height(32),
                )
                .child(Text::new(t!("rclone-o-rclone-abriu-o-navegador-para")).wrap()),
        )
        .close(t!("comum-cancelar"));

    let t_senha = Rc::clone(tela);
    let (senha_texto, senha_guardar, senha_erro) = (f.senha_texto, f.senha_guardar, f.senha_erro);
    let senha = ContentDialog::new(t!("rclone-senha-da-configuracao-do-rclone"), f.senha_aberta)
        .id("rclone-senha-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(t!("rclone-a-configuracao-do-rclone-rclone-conf")).wrap())
                .child(Edit::bind(senha_texto).password().id("rclone-senha-texto"))
                .child(
                    Text::new(move || senha_erro.get().unwrap_or_default())
                        .id("rclone-senha-erro")
                        .wrap()
                        .visible(move || senha_erro.with(Option::is_some)),
                )
                .child(
                    CheckBox::bind(senha_guardar, t!("rclone-guardar-no-cofre"))
                        .id("rclone-senha-guardar"),
                ),
        )
        .primary(t!("comum-continuar"), move || {
            t_senha.confirmar_senha_da_config()
        })
        .close(t!("comum-cancelar"));

    Column::new()
        .child(dialogo_pergunta)
        .child(login)
        .child(senha)
}
