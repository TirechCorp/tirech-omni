//! A página "Assistentes de IA": os provedores (`ia.json`) à esquerda, o formulário do escolhido à
//! direita. A chave de API não fica no arquivo: o botão "Guardar no cofre" a põe num item
//! "Chave de API — {nome}".

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::ai::{self, AiErrorKind, Event, Locality, Message, Request};
use tomni_ui::prelude::*;

use super::{Tela, poe};
use crate::agente::{acordar, ler_contador};
use crate::assistente::perfis::{Dialeto, PerfilIa, Preset, TlsIa};
use crate::busca::normalizar;
use crate::cofre::Tipo;
use crate::redes::{Rede, direta};
use crate::t;

/// O que a thread de "Testar" entrega: o nome do provedor e o resultado (a mensagem do erro).
type Testada = (String, Result<(), String>);

/// O que a thread de "Atualizar a lista" entrega: os ids dos modelos, ou a mensagem do erro.
type Modelos = Result<Vec<String>, String>;

/// Os valores do provedor escolhido, um signal por campo. O formulário edita estes signals; o app
/// os copia para o [`PerfilIa`] (e para o disco) a cada mudança.
#[derive(Clone, Copy)]
struct Campos {
    nome: Signal<String>,
    preset: Signal<String>,
    dialeto: Signal<String>,
    url: Signal<String>,
    modelo: Signal<String>,
    chave: Signal<String>,
    item: Signal<String>,
    rede: Signal<String>,
    max_tokens: Signal<String>,
    temperatura: Signal<String>,
    tls: Signal<String>,
    ca: Signal<String>,
    cabecalhos: Signal<String>,
}

/// O que os campos têm agora, lido de uma vez (com rastreamento: serve a um efeito).
struct Lidos {
    nome: String,
    preset: String,
    dialeto: String,
    url: String,
    modelo: String,
    rede: String,
    max_tokens: String,
    temperatura: String,
    tls: String,
    ca: String,
    cabecalhos: String,
}

fn tls_sistema() -> String {
    t!("ia-tls-sistema")
}
fn tls_ca() -> String {
    t!("ia-tls-ca")
}
fn tls_nao_verificar() -> String {
    t!("ia-tls-nao-verificar")
}

/// Os cabeçalhos que o texto "Nome: valor", um por linha, diz. Linha sem dois-pontos não conta.
fn cabecalhos_do_texto(texto: &str) -> Vec<(String, String)> {
    texto
        .lines()
        .filter_map(|linha| {
            let (nome, valor) = linha.split_once(':')?;
            let nome = nome.trim();
            (!nome.is_empty()).then(|| (nome.to_owned(), valor.trim().to_owned()))
        })
        .collect()
}

fn texto_dos_cabecalhos(cabecalhos: &[(String, String)]) -> String {
    cabecalhos
        .iter()
        .map(|(nome, valor)| format!("{nome}: {valor}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// O teto de tokens que o texto diz (vazio ou zero: o do provedor; texto que não é número: `atual`).
fn max_tokens_do_texto(texto: &str, atual: Option<u32>) -> Option<u32> {
    let texto = texto.trim();
    if texto.is_empty() {
        return None;
    }
    match texto.parse::<u32>() {
        Ok(0) => None,
        Ok(n) => Some(n),
        Err(_) => atual,
    }
}

/// A temperatura que o texto diz (vazio: a do provedor; texto que não é número: `atual`).
fn temperatura_do_texto(texto: &str, atual: Option<f32>) -> Option<f32> {
    let texto = texto.trim().replace(',', ".");
    if texto.is_empty() {
        return None;
    }
    texto
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .or(atual)
}

fn texto_da_temperatura(temperatura: Option<f32>) -> String {
    temperatura.map(|n| n.to_string()).unwrap_or_default()
}

impl Campos {
    fn novo() -> Self {
        Self {
            nome: signal(String::new()),
            preset: signal(Preset::Personalizado.nome()),
            dialeto: signal(Dialeto::OpenAi.nome()),
            url: signal(String::new()),
            modelo: signal(String::new()),
            chave: signal(String::new()),
            item: signal(String::new()),
            rede: signal(direta()),
            max_tokens: signal(String::new()),
            temperatura: signal(String::new()),
            tls: signal(tls_sistema()),
            ca: signal(String::new()),
            cabecalhos: signal(String::new()),
        }
    }

    fn lidos(&self) -> Lidos {
        Lidos {
            nome: self.nome.get(),
            preset: self.preset.get(),
            dialeto: self.dialeto.get(),
            url: self.url.get(),
            modelo: self.modelo.get(),
            rede: self.rede.get(),
            max_tokens: self.max_tokens.get(),
            temperatura: self.temperatura.get(),
            tls: self.tls.get(),
            ca: self.ca.get(),
            cabecalhos: self.cabecalhos.get(),
        }
    }

    /// Mostra o provedor `p` no formulário. O que a pessoa está digitando e que já vale o mesmo
    /// (um "0," na temperatura, uma linha de cabeçalho pela metade) não é reescrito.
    fn carregar(&self, p: &PerfilIa, tela: &Tela) {
        poe(self.nome, p.nome.clone());
        poe(self.preset, p.preset.nome());
        poe(self.dialeto, p.dialeto.nome());
        poe(self.url, p.url.clone());
        poe(self.modelo, p.modelo.clone());
        let rotulo_da_rede = tela.tree.with_untracked(|t| match p.rede_efetiva() {
            Rede::Perfil(id) => t.perfil(id).map_or_else(direta, |r| r.name.clone()),
            _ => direta(),
        });
        poe(self.rede, rotulo_da_rede);
        if max_tokens_do_texto(&self.max_tokens.get_untracked(), None) != p.max_tokens {
            poe(
                self.max_tokens,
                p.max_tokens.map(|n| n.to_string()).unwrap_or_default(),
            );
        }
        if temperatura_do_texto(&self.temperatura.get_untracked(), None) != p.temperatura {
            poe(self.temperatura, texto_da_temperatura(p.temperatura));
        }
        let (rotulo, caminho) = match &p.tls {
            TlsIa::Sistema => (tls_sistema(), None),
            TlsIa::CaPropria(caminho) => (tls_ca(), Some(caminho.clone())),
            TlsIa::NaoVerificar => (tls_nao_verificar(), None),
        };
        poe(self.tls, rotulo);
        if let Some(caminho) = caminho {
            poe(self.ca, caminho);
        }
        if cabecalhos_do_texto(&self.cabecalhos.get_untracked()) != p.cabecalhos {
            poe(self.cabecalhos, texto_dos_cabecalhos(&p.cabecalhos));
        }
        let titulo = p
            .credencial
            .and_then(|id| tela.cofre.agora(|c| c.item(id).map(|i| i.titulo.clone())))
            .unwrap_or_default();
        poe(self.item, titulo);
    }

    /// O provedor `base` com o que o formulário tem. Trocar o preset preenche o dialeto e a URL
    /// dele; mexer na URL de um preset o torna personalizado.
    fn aplicar(&self, base: &PerfilIa, lidos: &Lidos, tela: &Tela) -> PerfilIa {
        let mut p = base.clone();
        let preset = Preset::do_rotulo(&lidos.preset).unwrap_or(base.preset);
        if preset != base.preset {
            p.trocar_preset(preset);
        } else {
            if lidos.url != base.url {
                p.url_editada(lidos.url.clone());
            }
            if p.preset == Preset::Personalizado
                && let Some(d) = Dialeto::TODOS
                    .into_iter()
                    .find(|d| d.nome() == lidos.dialeto)
            {
                p.dialeto = d;
            }
        }
        let nome = lidos.nome.trim();
        if !nome.is_empty() {
            p.nome = nome.to_owned();
        }
        p.modelo = lidos.modelo.trim().to_owned();
        p.rede = tela.tree.with_untracked(|t| {
            if lidos.rede == direta() {
                Rede::Direta
            } else {
                t.redes
                    .iter()
                    .find(|r| r.name == lidos.rede)
                    .map_or(base.rede_efetiva(), |r| Rede::Perfil(r.id))
            }
        });
        p.max_tokens = max_tokens_do_texto(&lidos.max_tokens, base.max_tokens);
        p.temperatura = temperatura_do_texto(&lidos.temperatura, base.temperatura);
        p.tls = if lidos.tls == tls_ca() {
            TlsIa::CaPropria(lidos.ca.trim().to_owned())
        } else if lidos.tls == tls_nao_verificar() {
            TlsIa::NaoVerificar
        } else {
            TlsIa::Sistema
        };
        p.cabecalhos = cabecalhos_do_texto(&lidos.cabecalhos);
        p
    }
}

/// O texto do selo de onde fica o endereço.
pub(super) fn selo_da_localidade(url: &str) -> String {
    match ai::locality(url.trim().trim_end_matches('/')) {
        Locality::Machine => t!("ia-local-maquina"),
        Locality::Lan => t!("ia-local-rede"),
        Locality::Internet => t!("ia-local-internet"),
    }
}

/// O host da URL, para as mensagens (a URL inteira, se não der para separar).
pub(super) fn host_da_url(url: &str) -> String {
    let resto = url.split_once("://").map_or(url, |(_, r)| r);
    let autoridade = resto.split(['/', '?', '#']).next().unwrap_or(resto);
    autoridade
        .rsplit_once('@')
        .map_or(autoridade, |(_, a)| a)
        .to_owned()
}

/// Um rótulo em cima do controle.
fn linha(rotulo: impl Into<String>, controle: impl View) -> Column {
    Column::new()
        .gap(4)
        .child(Text::new(rotulo.into()).secondary())
        .child(controle)
}

impl Tela {
    /// Passa o formulário para o provedor escolhido (com o que mudou), e o `ia.json` o grava.
    fn guardar_perfil_de_ia(&self, campos: &Campos) {
        let lidos = campos.lidos();
        untrack(|| {
            let Some(id) = self
                .assistente
                .escolhido
                .get_untracked()
                .and_then(|k| k.parse::<u64>().ok())
            else {
                return;
            };
            let Some(base) = self.perfil_de_ia(id) else {
                return;
            };
            let nova = campos.aplicar(&base, &lidos, self);
            if nova == base {
                return;
            }
            self.assistente.perfis.update(|a| {
                if let Some(p) = a.perfil_mut(id) {
                    *p = nova.clone();
                }
            });
            // O preset trocou a URL e o dialeto, ou a URL o fez personalizado: o formulário acompanha.
            campos.carregar(&nova, self);
        });
    }

    /// Apaga o provedor `id`; com `com_item`, apaga também o item do cofre com a chave dele.
    fn apagar_perfil_de_ia(&self, id: u64, com_item: bool) {
        let credencial = self
            .assistente
            .perfis
            .try_update(|a| a.remover(id))
            .flatten();
        self.assistente.escolhido.set(None);
        if let (true, Some(item)) = (com_item, credencial.flatten()) {
            self.com_cofre_aberto(move |t| {
                let r = t.cofre.mudar(|c| c.remover(item));
                if let Err(erro) = r {
                    t.avisar(
                        InfoBarSeverity::Error,
                        t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
                    );
                }
            });
        }
    }
}

/// Abre a thread do "Testar": prepara a rede, manda uma pergunta curta e guarda o resultado.
fn iniciar_teste(
    tela: &Tela,
    id: u64,
    testada: &Arc<Mutex<Option<Testada>>>,
    pronto: Signal<u64>,
    testando: Signal<bool>,
) {
    let Some(perfil) = tela.perfil_de_ia(id) else {
        return;
    };
    if !perfil.usavel() {
        tela.avisar(InfoBarSeverity::Warning, t!("ia-url-invalida"));
        return;
    }
    let (mut ponto, rede) = tela.endpoint_do_perfil(&perfil);
    // O teste espera pouco: um modelo local ainda carregando responde no segundo pedido.
    ponto.response_timeout = Some(std::time::Duration::from_secs(60));
    testando.set(true);
    tela.avisar(
        InfoBarSeverity::Informational,
        t!("ia-testando", nome = perfil.nome.as_str()),
    );
    let rede = rede.map(|r| tela.resolver_rede(r));
    let testada = Arc::clone(testada);
    let nome = perfil.nome.clone();
    let dica = perfil.preset.dica_de_conexao();
    std::thread::spawn(move || {
        if let Some(rede) = &rede
            && let Err(erro) = rede.preparar(&mut |_| {})
        {
            *testada.lock().unwrap_or_else(PoisonError::into_inner) = Some((nome, Err(erro)));
            acordar(pronto);
            return;
        }
        let destino = Arc::clone(&testada);
        let pedido = Request {
            system: String::new(),
            messages: vec![Message::user(t!("ia-teste-pergunta"))],
            max_tokens: Some(16),
            temperature: None,
        };
        ai::stream(ponto, pedido, move |evento| {
            let resultado = match evento {
                Event::Done(_) => Ok(()),
                Event::Error(erro) => {
                    let mut mensagem = erro.message.clone();
                    if erro.kind == AiErrorKind::Connect
                        && let Some(dica) = &dica
                    {
                        mensagem = format!("{mensagem} {dica}");
                    }
                    Err(mensagem)
                }
                Event::Text(_) | Event::Reasoning(_) => return,
            };
            *destino.lock().unwrap_or_else(PoisonError::into_inner) =
                Some((nome.clone(), resultado));
            acordar(pronto);
        });
    });
}

/// Abre a thread do "Atualizar a lista": pede a lista de modelos ao provedor.
fn atualizar_modelos(
    tela: &Tela,
    id: u64,
    guardados: &Arc<Mutex<Option<Modelos>>>,
    pronto: Signal<u64>,
    buscando: Signal<bool>,
) {
    let Some(perfil) = tela.perfil_de_ia(id) else {
        return;
    };
    if !perfil.usavel() {
        tela.avisar(InfoBarSeverity::Warning, t!("ia-url-invalida"));
        return;
    }
    let (ponto, rede) = tela.endpoint_do_perfil(&perfil);
    let rede = rede.map(|r| tela.resolver_rede(r));
    buscando.set(true);
    let guardados = Arc::clone(guardados);
    std::thread::spawn(move || {
        let resultado = match &rede {
            Some(rede) => rede.preparar(&mut |_| {}),
            None => Ok(()),
        }
        .and_then(|()| ai::models(&ponto).map_err(|e| e.message))
        .map(|lista| lista.into_iter().map(|m| m.id).collect());
        *guardados.lock().unwrap_or_else(PoisonError::into_inner) = Some(resultado);
        acordar(pronto);
    });
}

/// A página "Assistentes de IA".
pub(super) fn pagina(tela: &Rc<Tela>) -> impl View {
    let estado = &tela.assistente;
    let (perfis, escolhido, escolher) = (estado.perfis, estado.escolhido, estado.escolher_aberto);
    let cofre = tela.cofre.clone();
    let tree = tela.tree;
    let campos = Campos::novo();
    let itens: Signal<Vec<TreeItem>> = signal(Vec::new());
    let apagar = signal(false);
    let apagar_item = signal(false);
    let testando = signal(false);
    let buscando = signal(false);
    let pronto_teste = signal(0u64);
    let pronto_modelos = signal(0u64);
    let modelos: Signal<Vec<String>> = signal(Vec::new());
    let testada: Arc<Mutex<Option<Testada>>> = Arc::default();
    let recebidos: Arc<Mutex<Option<Modelos>>> = Arc::default();

    // A lista acompanha os provedores; se o escolhido sumiu, a escolha também.
    {
        let anterior: RefCell<Vec<(u64, String, bool)>> = RefCell::default();
        effect(move || {
            let atuais: Vec<(u64, String, bool)> = perfis.with(|a| {
                a.perfis
                    .iter()
                    .map(|p| (p.id, p.nome.clone(), a.padrao == Some(p.id)))
                    .collect()
            });
            untrack(|| {
                if *anterior.borrow() != atuais {
                    itens.set(
                        atuais
                            .iter()
                            .map(|(id, nome, padrao)| {
                                let rotulo = if *padrao {
                                    t!("ia-lista-padrao", nome = nome.as_str())
                                } else {
                                    nome.clone()
                                };
                                TreeItem::leaf(id.to_string(), rotulo).icon(Symbol::Sparkle)
                            })
                            .collect(),
                    );
                }
                let existe = escolhido
                    .get_untracked()
                    .and_then(|k| k.parse::<u64>().ok())
                    .is_some_and(|id| atuais.iter().any(|(x, _, _)| *x == id));
                if !existe && escolhido.get_untracked().is_some() {
                    escolhido.set(None);
                }
                *anterior.borrow_mut() = atuais;
            });
        });
    }

    // Trocar a escolha mostra o provedor no formulário; mexer no formulário grava no provedor.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let chave = escolhido.get();
            untrack(|| {
                let perfil = chave
                    .and_then(|k| k.parse::<u64>().ok())
                    .and_then(|id| t.perfil_de_ia(id));
                campos.carregar(&perfil.unwrap_or_default(), &t);
                poe(campos.chave, String::new());
                modelos.set(Vec::new());
            });
        });
        let t = Rc::clone(tela);
        effect(move || t.guardar_perfil_de_ia(&campos));
        // O cofre mudou (outro item, outro título, desbloqueio): refaz a linha do item.
        let t = Rc::clone(tela);
        effect(move || {
            let _ = t.cofre.versao();
            untrack(|| {
                let perfil = escolhido
                    .get_untracked()
                    .and_then(|k| k.parse::<u64>().ok())
                    .and_then(|id| t.perfil_de_ia(id));
                if let Some(p) = perfil {
                    campos.carregar(&p, &t);
                }
            });
        });
    }

    // A thread do "Testar" terminou: mostra o resultado no aviso do alto.
    {
        let (t, testada) = (Rc::clone(tela), Arc::clone(&testada));
        effect(move || {
            let _ = ler_contador(pronto_teste);
            let Some((nome, resultado)) = testada
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
            else {
                return;
            };
            untrack(|| {
                testando.set(false);
                match resultado {
                    Ok(()) => t.avisar(InfoBarSeverity::Success, t!("ia-teste-ok", nome = nome)),
                    Err(erro) => t.avisar(
                        InfoBarSeverity::Error,
                        t!("ia-teste-falhou", nome = nome, erro = erro),
                    ),
                }
            });
        });
    }

    // A thread da lista de modelos terminou.
    {
        let (t, recebidos) = (Rc::clone(tela), Arc::clone(&recebidos));
        effect(move || {
            let _ = ler_contador(pronto_modelos);
            let Some(resultado) = recebidos
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
            else {
                return;
            };
            untrack(|| {
                buscando.set(false);
                match resultado {
                    Ok(lista) => {
                        if lista.is_empty() {
                            t.avisar(InfoBarSeverity::Informational, t!("ia-modelos-nenhum"));
                        }
                        modelos.set(lista);
                    }
                    Err(erro) => {
                        t.avisar(InfoBarSeverity::Warning, t!("ia-modelos-erro", erro = erro));
                    }
                }
            });
        });
    }

    let sem_escolha = move || escolhido.get().is_none();
    let id_escolhido = move || {
        escolhido
            .get_untracked()
            .and_then(|k| k.parse::<u64>().ok())
    };
    let e_padrao = move || {
        let id = escolhido.get().and_then(|k| k.parse::<u64>().ok());
        perfis.with(|a| id.is_some() && a.padrao == id)
    };

    let testar = {
        let (t, testada) = (Rc::clone(tela), Arc::clone(&testada));
        move || {
            let Some(id) = id_escolhido() else {
                return;
            };
            // Uma chave digitada e ainda não guardada vai para o cofre antes de testar.
            let chave = campos.chave.get_untracked();
            if chave.trim().is_empty() {
                iniciar_teste(&t, id, &testada, pronto_teste, testando);
            } else {
                let testada = Arc::clone(&testada);
                t.com_cofre_aberto(move |t| {
                    t.guardar_chave_do_perfil(id, chave);
                    campos.chave.set(String::new());
                    iniciar_teste(t, id, &testada, pronto_teste, testando);
                });
            }
        }
    };
    let guardar_chave = {
        let t = Rc::clone(tela);
        move || {
            let Some(id) = id_escolhido() else {
                return;
            };
            let chave = campos.chave.get_untracked();
            if chave.trim().is_empty() {
                return;
            }
            t.com_cofre_aberto(move |t| {
                t.guardar_chave_do_perfil(id, chave);
                campos.chave.set(String::new());
            });
        }
    };
    let atualizar_lista = {
        let (t, recebidos) = (Rc::clone(tela), Arc::clone(&recebidos));
        move || {
            if let Some(id) = id_escolhido() {
                atualizar_modelos(&t, id, &recebidos, pronto_modelos, buscando);
            }
        }
    };

    let botoes = Row::new()
        .gap(8)
        .child(
            Button::new(t!("comum-novo"))
                .id("ia-perfil-novo")
                .icon(Symbol::Add)
                .on_click(move || escolher.set(true)),
        )
        .child(
            Button::new(t!("ia-testar"))
                .id("ia-perfil-testar")
                .icon(Symbol::Play)
                .disabled(move || sem_escolha() || testando.get())
                .on_click(testar),
        )
        .child(
            Button::new(t!("ia-usar-como-padrao"))
                .id("ia-perfil-padrao")
                .icon(Symbol::Pin)
                .disabled(move || sem_escolha() || e_padrao())
                .on_click(move || {
                    if let Some(id) = id_escolhido() {
                        perfis.update(|a| a.padrao = Some(id));
                    }
                }),
        )
        .child(
            Button::new(t!("comum-apagar"))
                .id("ia-perfil-excluir")
                .icon(Symbol::Delete)
                .disabled(sem_escolha)
                .on_click(move || {
                    apagar_item.set(false);
                    apagar.set(true);
                }),
        );

    let arvore = TreeView::bind(itens, escolhido)
        .id("ia-perfis-arvore")
        .grow(1);
    let lista = Column::new()
        .gap(4)
        .padding(Insets::new(0.0, 8.0, 0.0, 0.0))
        .child(
            Text::new(t!("ia-lista-vazia"))
                .id("ia-perfis-vazia")
                .secondary()
                .wrap()
                .visible(move || itens.get().is_empty()),
        )
        .child(arvore);

    // O nome do modelo: o que a pessoa digita, com a lista do provedor como sugestão.
    let sugerir_modelo = move |digitado: &str| -> Vec<String> {
        let procurado = normalizar(digitado.trim());
        modelos.with_untracked(|lista| {
            lista
                .iter()
                .filter(|m| normalizar(m).contains(&procurado))
                .take(12)
                .cloned()
                .collect()
        })
    };
    // O item do cofre que já tem a chave: o título, como o `rclone.credencial` das Configurações.
    let sugerir_item = {
        let cofre = cofre.clone();
        move |digitado: &str| -> Vec<String> {
            let procurado = normalizar(digitado.trim());
            cofre.agora(|c| {
                if c.bloqueado() {
                    return Vec::new();
                }
                c.itens()
                    .iter()
                    .filter(|i| i.tipo == Tipo::Senha && normalizar(&i.titulo).contains(&procurado))
                    .map(|i| i.titulo.clone())
                    .take(8)
                    .collect()
            })
        }
    };
    let escolher_item = {
        let (t, cofre) = (Rc::clone(tela), cofre.clone());
        move |enviado: String| {
            let Some(id) = id_escolhido() else {
                return;
            };
            let enviado = enviado.trim();
            let item = if enviado.is_empty() {
                Some(None)
            } else {
                cofre
                    .agora(|c| {
                        if c.bloqueado() {
                            return None;
                        }
                        c.itens().iter().find(|i| i.titulo == enviado).map(|i| i.id)
                    })
                    .map(Some)
            };
            match item {
                Some(item) => t.assistente.perfis.update(|a| {
                    if let Some(p) = a.perfil_mut(id) {
                        p.credencial = item;
                    }
                }),
                None => t.avisar(
                    InfoBarSeverity::Warning,
                    t!("config-credencial-inexistente", titulo = enviado),
                ),
            }
        }
    };

    let estado_da_chave = {
        let cofre = cofre.clone();
        move || {
            let credencial = escolhido
                .get()
                .and_then(|k| k.parse::<u64>().ok())
                .and_then(|id| perfis.with(|a| a.perfil(id).and_then(|p| p.credencial)));
            let Some(item) = credencial else {
                return t!("ia-chave-sem");
            };
            cofre.ler(|c| {
                if c.bloqueado() {
                    t!("ia-chave-no-cofre-bloqueado")
                } else {
                    c.item(item).map_or_else(
                        || t!("ia-chave-item-sumiu"),
                        |i| t!("ia-chave-no-item", titulo = i.titulo.clone()),
                    )
                }
            })
        }
    };

    let opcoes_de_rede = move || {
        let mut opcoes = vec![direta()];
        opcoes.extend(tree.with(|t| t.redes.iter().map(|r| r.name.clone()).collect::<Vec<_>>()));
        vec![opcoes]
    };
    let e_personalizado = move || campos.preset.get() == Preset::Personalizado.nome();
    let na_internet = move || {
        ai::locality(campos.url.get().trim().trim_end_matches('/')) == Locality::Internet
            && !campos.url.get().trim().is_empty()
    };
    let com_ca = move || campos.tls.get() == tls_ca();

    let formulario = Column::new()
        .gap(12)
        .padding(Insets::new(0.0, 0.0, 12.0, 8.0))
        .child(linha(
            t!("comum-nome"),
            Edit::bind(campos.nome).id("ia-perfil-nome"),
        ))
        .child(linha(
            t!("ia-rotulo-provedor"),
            ComboBox::bind(campos.preset, Preset::TODOS.into_iter().map(Preset::nome))
                .id("ia-perfil-preset"),
        ))
        .child(
            Text::new(move || {
                Preset::do_rotulo(&campos.preset.get()).map_or_else(String::new, Preset::observacao)
            })
            .id("ia-perfil-observacao")
            .secondary()
            .wrap(),
        )
        .child(linha(
            t!("ia-rotulo-dialeto"),
            ComboBox::bind(
                campos.dialeto,
                Dialeto::TODOS.into_iter().map(Dialeto::nome),
            )
            .id("ia-perfil-dialeto")
            .disabled(move || !e_personalizado()),
        ))
        .child(linha(
            t!("ia-rotulo-url"),
            Row::new()
                .gap(8)
                .child(
                    Edit::bind(campos.url)
                        .id("ia-perfil-url")
                        .placeholder(t!("ia-url-exemplo"))
                        .grow(1),
                )
                .child(
                    Text::new(move || selo_da_localidade(&campos.url.get()))
                        .id("ia-perfil-local")
                        .secondary(),
                ),
        ))
        .child(
            InfoBar::new(InfoBarSeverity::Warning)
                .id("ia-perfil-aviso-rede")
                .message(move || t!("ia-aviso-internet", host = host_da_url(&campos.url.get())))
                .visible(na_internet),
        )
        .child(linha(
            t!("ia-rotulo-modelo"),
            Row::new()
                .gap(8)
                .child(
                    AutoSuggestBox::bind(campos.modelo, sugerir_modelo)
                        .id("ia-perfil-modelo")
                        .placeholder(t!("ia-modelo-placeholder"))
                        .grow(1),
                )
                .child(
                    Button::new(t!("ia-modelos-atualizar"))
                        .id("ia-modelos-atualizar")
                        .icon(Symbol::ArrowSync)
                        .disabled(move || sem_escolha() || buscando.get())
                        .on_click(atualizar_lista),
                ),
        ))
        .child(linha(
            t!("ia-rotulo-chave"),
            Column::new()
                .gap(4)
                .child(
                    Row::new()
                        .gap(8)
                        .child(
                            Edit::bind(campos.chave)
                                .password()
                                .id("ia-perfil-chave")
                                .placeholder(t!("ia-chave-placeholder"))
                                .grow(1),
                        )
                        .child(
                            Button::new(t!("ia-chave-guardar"))
                                .id("ia-perfil-chave-guardar")
                                .icon(Symbol::Key)
                                .disabled(move || campos.chave.get().trim().is_empty())
                                .on_click(guardar_chave),
                        ),
                )
                .child(
                    Text::new(estado_da_chave)
                        .id("ia-perfil-chave-estado")
                        .secondary()
                        .wrap(),
                ),
        ))
        .child(linha(
            t!("ia-rotulo-item"),
            AutoSuggestBox::bind(campos.item, sugerir_item)
                .id("ia-perfil-item")
                .placeholder(t!("config-credencial-nenhuma"))
                .on_submit(escolher_item),
        ))
        .child(linha(
            t!("comum-rede"),
            Column::new().child(For::each(
                opcoes_de_rede,
                |opcoes: &Vec<String>| opcoes.join("\u{1}"),
                move |opcoes: Vec<String>| ComboBox::bind(campos.rede, opcoes).id("ia-perfil-rede"),
            )),
        ))
        .child(Text::new(t!("ia-avancado")).subtitle())
        .child(linha(
            t!("ia-rotulo-max-tokens"),
            Edit::bind(campos.max_tokens)
                .id("ia-perfil-max-tokens")
                .placeholder(t!("ia-padrao-do-provedor")),
        ))
        .child(linha(
            t!("ia-rotulo-temperatura"),
            Edit::bind(campos.temperatura)
                .id("ia-perfil-temperatura")
                .placeholder(t!("ia-padrao-do-provedor")),
        ))
        .child(linha(
            t!("ia-rotulo-tls"),
            ComboBox::bind(
                campos.tls,
                vec![tls_sistema(), tls_ca(), tls_nao_verificar()],
            )
            .id("ia-perfil-tls"),
        ))
        .child(
            linha(
                t!("ia-rotulo-ca"),
                Edit::bind(campos.ca)
                    .id("ia-perfil-ca")
                    .placeholder(t!("ia-ca-exemplo")),
            )
            .visible(com_ca),
        )
        .child(linha(
            t!("ia-rotulo-cabecalhos"),
            TextEditor::bind(campos.cabecalhos)
                .id("ia-perfil-cabecalhos")
                .monospace()
                .height(72),
        ));

    let sem_perfil = Text::new(t!("ia-sem-escolha"))
        .id("ia-perfis-sem-escolha")
        .secondary()
        .wrap()
        .visible(sem_escolha);

    let direita = Column::new()
        .grow(1)
        .padding(Insets::new(0.0, 0.0, 0.0, 8.0))
        .child(
            ScrollViewer::new()
                .id("ia-perfil-rolagem")
                .grow(1)
                .child(formulario)
                .visible(move || escolhido.get().is_some()),
        )
        .child(sem_perfil);

    let dialogo = {
        let t = Rc::clone(tela);
        ContentDialog::new(
            move || {
                let nome = escolhido.get().and_then(|k| {
                    let id = k.parse().ok()?;
                    perfis.with(|a| a.perfil(id).map(|p| p.nome.clone()))
                });
                t!("comum-apagar-titulo", nome = nome.unwrap_or_default())
            },
            apagar,
        )
        .id("ia-perfil-excluir-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(t!("ia-excluir-texto")).wrap())
                .child(
                    CheckBox::bind(apagar_item, t!("ia-excluir-item-do-cofre"))
                        .id("ia-perfil-excluir-item"),
                ),
        )
        .primary(t!("comum-apagar"), move || {
            if let Some(id) = id_escolhido() {
                t.apagar_perfil_de_ia(id, apagar_item.get_untracked());
            }
        })
        .close(t!("comum-cancelar"))
    };

    let principal = Split::row(lista, direita)
        .id("ia-perfis-principal")
        .size(signal(220.0))
        .min_first(160.0)
        .min_second(360.0)
        .grow(1);

    Column::new()
        .padding(16)
        .gap(8)
        .child(Text::new(t!("ia-pagina-titulo")).title())
        .child(Text::new(t!("ia-pagina-descricao")).secondary().wrap())
        .child(botoes)
        .child(principal)
        .child(dialogo)
}
