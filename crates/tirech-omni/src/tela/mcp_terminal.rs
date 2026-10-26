//! As ferramentas do MCP que mexem em abas e terminais (M51, seção 7.1): abrir e fechar,
//! listar, digitar, apertar teclas, ler a tela, esperar e executar. A decisão de permissão
//! e a aprovação já aconteceram quando o código daqui roda.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tomni_ui::prelude::*;
use tomni_ui::terminal::{PaneDirection, Session};

use super::mcp::{Plano, Saida, Sonda, Trabalho};
use super::{Janela, Lugar, Tela};
use crate::mcp::espera::{self, Espera, Passo, SILENCIO_DO_PROMPT, linha_de_prompt};
use crate::mcp::permissao::{PermissaoIa, permissao_efetiva};
use crate::mcp::teclas::bytes_das_teclas;
use crate::t;
use crate::texto_limpo::{para_inserir, sem_controle};

/// Quanto tempo uma aba nova espera pelo primeiro texto do servidor.
const PRAZO_DA_ABERTURA: Duration = Duration::from_secs(15);

/// Quanto tempo sem texto novo vale por "a conexão terminou de chegar".
const SILENCIO_DA_ABERTURA: Duration = Duration::from_millis(500);

/// Quanto tempo uma abertura espera a pessoa digitar a senha.
const PRAZO_DA_SENHA: Duration = Duration::from_secs(300);

/// O número que o assistente deu para uma janela ou um painel (a partir de 1).
pub(super) fn numero(valor: &Value) -> Option<usize> {
    match valor {
        Value::Number(n) => n.as_u64().and_then(|n| usize::try_from(n).ok()),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|&n| n >= 1)
}

fn texto_de(valor: &Value) -> Option<&str> {
    valor.as_str().filter(|s| !s.is_empty())
}

impl Tela {
    /// A sessão do plano, ou o erro de "diga qual aba".
    fn sessao_do(plano: &Plano) -> Result<Session, String> {
        plano.sessao.ok_or_else(|| t!("mcp-diga-a-aba"))
    }

    fn finais_de_prompt(&self) -> Vec<String> {
        self.config
            .with_untracked(|c| c.ia.finais_de_prompt.clone())
    }

    pub(super) fn erro_mcp(self: &Rc<Self>, t: &Rc<Trabalho>, texto: String) {
        self.terminar_mcp(t, Saida::Erro(texto));
    }

    /// Onde a aba nova entra, pelos argumentos `janela` e `painel`.
    pub(super) fn lugar_do_assistente(self: &Rc<Self>, args: &Value) -> Result<Lugar, String> {
        let nova = |tela: &Rc<Self>| {
            tela.abrir_janela((1, 1), None)
                .ok_or_else(|| t!("casca-so-uma-janela"))
        };
        let mut janela: Janela = match &args["janela"] {
            Value::Null => self.janela_ativa(),
            Value::String(s) if s == "atual" => self.janela_ativa(),
            Value::String(s) if s == "nova" => nova(self)?,
            outro => self.janela_numerada(outro)?,
        };
        match &args["painel"] {
            Value::Null => Ok(Lugar {
                janela: Some(janela.handle),
                painel: None,
            }),
            Value::String(s) if s == "nova_janela" => {
                janela = nova(self)?;
                Ok(Lugar {
                    janela: Some(janela.handle),
                    painel: None,
                })
            }
            Value::String(s) if s == "direita" => {
                let mut novo = 0;
                janela.layout.update(|l| {
                    let ativo = l.active();
                    novo = l.split(ativo, PaneDirection::Right);
                });
                Ok(Lugar::painel(janela, novo))
            }
            outro => {
                let n = numero(outro).ok_or_else(|| t!("mcp-painel-invalido"))?;
                let total = janela.layout.with_untracked(|l| l.pane_count());
                if n > total {
                    let numero = self.numero_da_janela(janela);
                    return Err(if total == 1 {
                        t!("mcp-painel-nao-existe-um", janela = numero.to_string())
                    } else {
                        t!(
                            "mcp-painel-nao-existe",
                            janela = numero.to_string(),
                            total = total.to_string()
                        )
                    });
                }
                Ok(Lugar::painel(janela, n - 1))
            }
        }
    }

    /// `abrir_conexao`: abre a aba (pedindo a senha à pessoa, se não estiver salva), espera o
    /// servidor responder e devolve a aba com o que apareceu na tela.
    pub(super) fn mcp_abrir_conexao(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let Some(conexao) = plano.conexao else {
            return self.erro_mcp(&t, t!("mcp-conexao-invalida"));
        };
        let lugar = match self.lugar_do_assistente(&t.pedido.args) {
            Ok(lugar) => lugar,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        if conexao.kind.so_arquivos() {
            self.abrir_com(conexao.id, lugar, true);
            let texto = t!("mcp-abriu-arquivos", conexao = conexao.name.as_str());
            return self.terminar_mcp(&t, Saida::App(texto));
        }
        let antes: HashSet<String> = self
            .sessoes_das_janelas(None)
            .iter()
            .map(|(_, s)| s.key_str())
            .collect();
        self.abrir_com(conexao.id, lugar, true);
        let (id, prefixo) = (conexao.id, format!("{}-", conexao.id));
        let inicio = Instant::now();
        // A sessão achada, o total de bytes na última olhada, quando mudou e desde quando espera.
        let mut achada: Option<(Session, u64, Instant, Instant)> = None;
        let trabalho = Rc::clone(&t);
        self.passo_mcp(
            t,
            Box::new(move |tela| {
                let agora = Instant::now();
                if achada.is_none() {
                    let nova = tela
                        .sessoes_das_janelas(None)
                        .into_iter()
                        .map(|(_, s)| s)
                        .find(|s| {
                            let chave = s.key_str();
                            chave.starts_with(&prefixo) && !antes.contains(&chave)
                        });
                    match nova {
                        Some(sessao) => {
                            let chave = sessao.key_str();
                            tela.marcar_aba_mcp(&trabalho, &chave);
                            tela.dar_aba(&chave, trabalho.pedido.ligacao);
                            achada = Some((sessao, sessao.received(), agora, agora));
                        }
                        None => {
                            let esperando_senha = tela
                                .pendentes
                                .with_untracked(|p| p.iter().any(|p| p.id == id));
                            if esperando_senha && inicio.elapsed() < PRAZO_DA_SENHA {
                                return Sonda::Depois(Duration::from_millis(200));
                            }
                            return Sonda::Fim(Saida::Erro(t!("mcp-abrir-sem-aba")));
                        }
                    }
                }
                let Some((sessao, ultimo, mudou, desde)) = &mut achada else {
                    return Sonda::Depois(Duration::from_millis(100));
                };
                let recebidos = sessao.received();
                if recebidos != *ultimo {
                    *ultimo = recebidos;
                    *mudou = agora;
                }
                let caiu = sessao.closed();
                let quieta = recebidos > 0 && agora.duration_since(*mudou) >= SILENCIO_DA_ABERTURA;
                let demorou = agora.duration_since(*desde) >= PRAZO_DA_ABERTURA;
                if !(caiu || quieta || demorou) {
                    return Sonda::Depois(Duration::from_millis(100));
                }
                let estado = if caiu {
                    "caida"
                } else if quieta {
                    "conectada"
                } else {
                    "conectando"
                };
                let cabecalho = json!({
                    "aba": sessao.key_str(),
                    "conexao": id,
                    "estado": estado,
                })
                .to_string();
                Sonda::Fim(Saida::servidor("tela", sessao.text(), Some(cabecalho)))
            }),
        );
    }

    /// `listar_abas`: as abas abertas das conexões liberadas, com o estado de cada uma.
    pub(super) fn mcp_listar_abas(self: &Rc<Self>, t: Rc<Trabalho>) {
        let filtro = match t.pedido.args.get("conexao").filter(|v| !v.is_null()) {
            Some(valor) => match self
                .tree
                .with_untracked(|tr| crate::mcp::alvo::achar_conexao(tr, valor).map(|c| c.id))
            {
                Ok(id) => Some(id),
                Err(erro) => return self.erro_mcp(&t, erro),
            },
            None => None,
        };
        let mut abas = Vec::new();
        for (janela, sessao) in self.sessoes_das_janelas(None) {
            let chave = sessao.key_str();
            let Some(id) = crate::espacos::id_da_chave(&chave) else {
                continue;
            };
            if filtro.is_some_and(|f| f != id) {
                continue;
            }
            let alvo = self.aba_da_sessao(t.pedido.ligacao).as_deref() == Some(chave.as_str());
            let dada = !alvo && self.liberada_pelo_clique(t.pedido.ligacao, &chave);
            let liberada = alvo
                || dada
                || self
                    .tree
                    .with_untracked(|tr| permissao_efetiva(tr, id) != PermissaoIa::Desligada);
            if !liberada {
                continue;
            }
            let reconexao = self.reconexao_marcada(sessao);
            let (estado, motivo) = if !sessao.closed() {
                ("conectada", None)
            } else {
                match sessao.close_reason() {
                    Some(motivo) if reconexao.is_some() => ("reconectando", Some(motivo)),
                    Some(motivo) => ("caida", Some(motivo)),
                    None => ("encerrada", None),
                }
            };
            let (painel, ativa) = janela.layout.with_untracked(|l| {
                (
                    l.pane_of(sessao).map(|p| p + 1),
                    l.pane(l.active()).selected == Some(sessao),
                )
            });
            let dona = self.mcp.donos.borrow().get(&chave).copied();
            let nome_do_dono = dona.and_then(|d| self.sessao_ia(d)).map(|s| s.nome);
            let mut aba = json!({
                "aba": chave,
                "conexao": id,
                "titulo": sessao.title(),
                "estado": estado,
                "janela": self.janelas.numero_agora(janela.handle),
                "painel": painel,
                "ativa": ativa,
                "ia": self.aba_marcada(&chave),
                "aberta_pelo_assistente": dona.is_some(),
                "dono": nome_do_dono,
                "minha": dona == Some(t.pedido.ligacao),
                "alvo": alvo,
            });
            if dada {
                aba["dada"] = json!(true);
            }
            if let Some((n, max)) = reconexao.filter(|_| estado == "reconectando") {
                aba["tentativa"] = json!(n);
                aba["tentativas"] = json!(max);
            }
            if let Some(motivo) = motivo {
                aba["motivo"] = Value::String(motivo);
            }
            if let Some(soltos) = self.soltos_da_aba(&chave) {
                aba["arquivos_soltos"] = soltos;
            }
            abas.push(aba);
        }
        let texto = serde_json::to_string_pretty(&abas).unwrap_or_else(|_| "[]".to_owned());
        self.terminar_mcp(&t, Saida::servidor("abas", texto, None));
    }

    /// `fechar_aba`.
    pub(super) fn mcp_fechar_aba(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let sessao = match Self::sessao_do(&plano) {
            Ok(s) => s,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let chave = sessao.key_str();
        let Some((janela, _)) = self.sessao_por_chave(&chave) else {
            return self.erro_mcp(&t, t!("mcp-aba-nao-existe", aba = chave.as_str()));
        };
        self.mcp.donos.borrow_mut().remove(&chave);
        for janela in self.tirar_sessoes(vec![(janela, sessao)], false) {
            self.fechar_se_vazia(janela);
        }
        self.terminar_mcp(&t, Saida::App(t!("mcp-aba-fechada", aba = chave.as_str())));
    }

    /// `reconectar_aba`: reabre a mesma aba caída, com o histórico, e espera o servidor
    /// responder, como o `abrir_conexao`. A aba que está de pé fica como está.
    pub(super) fn mcp_reconectar_aba(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let sessao = match Self::sessao_do(&plano) {
            Ok(s) => s,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let chave = sessao.key_str();
        if !sessao.closed() {
            return self.terminar_mcp(
                &t,
                Saida::App(t!("mcp-aba-ja-conectada", aba = chave.as_str())),
            );
        }
        // A reconexão sozinha que estava marcada vira esta.
        self.reconectando.borrow_mut().remove(&sessao);
        if !sessao.restart() {
            return self.erro_mcp(&t, t!("mcp-aba-nao-reabre", aba = chave.as_str()));
        }
        let id = crate::espacos::id_da_chave(&chave);
        let inicio = Instant::now();
        let (mut ultimo, mut mudou) = (sessao.received(), inicio);
        let base = ultimo;
        self.passo_mcp(
            t,
            Box::new(move |tela| {
                let agora = Instant::now();
                let recebidos = sessao.received();
                if recebidos != ultimo {
                    ultimo = recebidos;
                    mudou = agora;
                }
                let caiu = sessao.closed();
                let quieta =
                    recebidos > base && agora.duration_since(mudou) >= SILENCIO_DA_ABERTURA;
                let demorou = agora.duration_since(inicio) >= PRAZO_DA_ABERTURA;
                let esperando_senha = id.is_some_and(|id| {
                    tela.pendentes
                        .with_untracked(|p| p.iter().any(|p| p.id == id))
                });
                if esperando_senha && agora.duration_since(inicio) < PRAZO_DA_SENHA {
                    return Sonda::Depois(Duration::from_millis(200));
                }
                if !(caiu || quieta || demorou) {
                    return Sonda::Depois(Duration::from_millis(100));
                }
                let mut cabecalho = json!({
                    "aba": sessao.key_str(),
                    "estado": if caiu {
                        "caida"
                    } else if quieta {
                        "conectada"
                    } else {
                        "conectando"
                    },
                });
                if let Some(motivo) = sessao.close_reason() {
                    cabecalho["motivo"] = Value::String(motivo);
                }
                Sonda::Fim(Saida::servidor(
                    "tela",
                    sessao.text(),
                    Some(cabecalho.to_string()),
                ))
            }),
        );
    }

    /// `digitar`: manda o texto como se a pessoa o digitasse; `colar` o cola de uma vez.
    pub(super) fn mcp_digitar(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let sessao = match Self::sessao_do(&plano) {
            Ok(s) => s,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let Some(texto) = t.pedido.args["texto"].as_str() else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "texto"));
        };
        if sessao.closed() {
            return self.erro_mcp(&t, self.erro_aba_caida(sessao));
        }
        if t.pedido.args["colar"].as_bool().unwrap_or(false) {
            sessao.paste(texto);
        } else {
            let bytes = texto.replace("\r\n", "\r").replace('\n', "\r");
            sessao.send(bytes.as_bytes());
        }
        let n = texto.chars().count();
        self.terminar_mcp(&t, Saida::App(t!("mcp-digitado", n = n.to_string())));
    }

    /// `teclas`: aperta as teclas na ordem, com os modos que a tela tem agora.
    pub(super) fn mcp_teclas(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let sessao = match Self::sessao_do(&plano) {
            Ok(s) => s,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let nomes: Vec<String> = t.pedido.args["teclas"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if nomes.is_empty() {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "teclas"));
        }
        if sessao.closed() {
            return self.erro_mcp(&t, self.erro_aba_caida(sessao));
        }
        let modos = sessao.emulator(|e| e.modes()).unwrap_or_default();
        match bytes_das_teclas(&nomes, &modos) {
            Ok(bytes) => {
                sessao.send(&bytes);
                let n = nomes.len();
                self.terminar_mcp(&t, Saida::App(t!("mcp-teclas-enviadas", n = n.to_string())));
            }
            Err(erro) => self.erro_mcp(&t, erro),
        }
    }

    /// `ler_tela`: o texto da tela, com `historico` linhas de rolagem acima se pedirem.
    pub(super) fn mcp_ler_tela(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let sessao = match Self::sessao_do(&plano) {
            Ok(s) => s,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let args = &t.pedido.args;
        let historico = args["historico"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(0);
        let (texto, cursor) = sessao
            .emulator(|e| {
                let mut linhas: Vec<String> = Vec::new();
                if historico > 0 {
                    let (primeira, ultima) = e.line_bounds();
                    let da_tela = (ultima + 1).saturating_sub(e.rows() as u64).max(primeira);
                    let desde = da_tela.saturating_sub(historico as u64).max(primeira);
                    for linha in desde..da_tela {
                        linhas.push(
                            e.line_at(linha)
                                .map(|r| r.text(0, e.cols()))
                                .unwrap_or_default(),
                        );
                    }
                }
                linhas.push(e.screen_text());
                (linhas.join("\n"), e.cursor())
            })
            .unwrap_or_default();
        let cabecalho = args["cursor"].as_bool().unwrap_or(false).then(|| {
            t!(
                "mcp-cursor",
                linha = (cursor.0 + 1).to_string(),
                coluna = (cursor.1 + 1).to_string()
            )
        });
        self.terminar_mcp(&t, Saida::servidor("tela", texto, cabecalho));
    }

    /// `esperar`: até o texto aparecer, a aba ficar em silêncio ou o prazo acabar.
    pub(super) fn mcp_esperar(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let sessao = match Self::sessao_do(&plano) {
            Ok(s) => s,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let args = &t.pedido.args;
        let texto = texto_de(&args["texto"]).map(str::to_owned);
        let regex = texto_de(&args["regex"]);
        if texto.is_some() && regex.is_some() {
            return self.erro_mcp(&t, t!("mcp-texto-ou-regex"));
        }
        let regex = match regex.map(espera::compilar).transpose() {
            Ok(r) => r,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let secs = |chave: &str| args[chave].as_u64().map(Duration::from_secs);
        let pedido = espera::Pedido {
            texto,
            regex,
            silencio: secs("silencio_secs"),
            prompt: None,
            prazo: secs("prazo_secs"),
        };
        self.esperar_na_aba(t, sessao, pedido, None);
    }

    /// Espera na aba `sessao` o que `pedido` diz. Com `comando`, a saída perde o eco dele e o
    /// prompt de volta.
    fn esperar_na_aba(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        sessao: Session,
        pedido: espera::Pedido,
        comando: Option<String>,
    ) {
        let finais = self.finais_de_prompt();
        let tap = sessao.tap();
        let mut espera = Espera::nova(pedido, Instant::now(), sessao.received());
        self.passo_mcp(
            t,
            Box::new(move |_| {
                let novo = tap.take();
                let linha = Self::linha_do_cursor(sessao);
                let passo = espera.avancar(
                    Instant::now(),
                    &novo,
                    sessao.received(),
                    &linha,
                    sessao.closed(),
                );
                match passo {
                    Passo::Esperar(depois) => Sonda::Depois(depois),
                    Passo::Fim(motivo) => {
                        let texto = match &comando {
                            Some(c) => espera::saida_do_comando(espera.saida(), c, &finais),
                            None => espera.saida().to_owned(),
                        };
                        let cabecalho = json!({ "motivo": motivo.nome() }).to_string();
                        Sonda::Fim(Saida::servidor("saida", texto, Some(cabecalho)))
                    }
                }
            }),
        );
    }

    /// `executar`: digita o comando na aba (modo `aba`) ou o roda à parte (modo `separado`).
    pub(super) fn mcp_executar(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let sessao = match Self::sessao_do(&plano) {
            Ok(s) => s,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let args = &t.pedido.args;
        let Some(comando) = texto_de(&args["comando"]).map(str::to_owned) else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "comando"));
        };
        let prazo = args["prazo_secs"].as_u64().map(Duration::from_secs);
        if sessao.closed() {
            return self.erro_mcp(&t, self.erro_aba_caida(sessao));
        }
        // Com `ia.comandos_na_aba`, o "separado" vai para a aba, à vista, quando ela está livre.
        let na_aba = self.config.with_untracked(|c| c.ia.comandos_na_aba);
        match args["modo"].as_str().unwrap_or("aba") {
            "separado" if na_aba && self.aba_livre_para(sessao, &comando) => {
                self.executar_na_aba(t, sessao, comando, prazo)
            }
            "aba" => self.executar_na_aba(t, sessao, comando, prazo),
            "separado" => self.executar_separado(t, sessao, comando, prazo),
            outro => self.erro_mcp(&t, t!("mcp-modo-invalido", modo = outro)),
        }
    }

    /// Se a aba está no prompt, fora da tela alternada, e o `comando` cabe nela de uma vez.
    fn aba_livre_para(&self, sessao: Session, comando: &str) -> bool {
        let (alternada, bracketed) = sessao
            .emulator(|e| (e.alt_screen(), e.modes().bracketed_paste))
            .unwrap_or_default();
        !alternada
            && linha_de_prompt(&Self::linha_do_cursor(sessao), &self.finais_de_prompt())
            && para_inserir(comando, bracketed).is_some()
    }

    fn executar_na_aba(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        sessao: Session,
        comando: String,
        prazo: Option<Duration>,
    ) {
        let finais = self.finais_de_prompt();
        let (alternada, bracketed) = sessao
            .emulator(|e| (e.alt_screen(), e.modes().bracketed_paste))
            .unwrap_or_default();
        let linha = Self::linha_do_cursor(sessao);
        if alternada || !linha_de_prompt(&linha, &finais) {
            return self.erro_mcp(&t, t!("mcp-aba-ocupada"));
        }
        let Some(mut bytes) = para_inserir(&comando, bracketed) else {
            return self.erro_mcp(&t, t!("mcp-comando-varias-linhas"));
        };
        let pedido = espera::Pedido {
            texto: None,
            regex: None,
            silencio: None,
            prompt: Some(finais),
            prazo,
        };
        // A espera começa antes do envio: o eco do comando já conta como saída.
        bytes.push(b'\r');
        let tap = sessao.tap();
        let recebidos = sessao.received();
        sessao.send(&bytes);
        self.esperar_comando(t, sessao, pedido, comando, tap, recebidos);
    }

    fn esperar_comando(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        sessao: Session,
        pedido: espera::Pedido,
        comando: String,
        tap: tomni_ui::terminal::OutputTap,
        recebidos: u64,
    ) {
        let finais = pedido.prompt.clone().unwrap_or_default();
        let mut espera = Espera::nova(pedido, Instant::now(), recebidos);
        // O prompt só vale depois que a saída para por um instante.
        let _ = SILENCIO_DO_PROMPT;
        self.passo_mcp(
            t,
            Box::new(move |_| {
                let novo = tap.take();
                let linha = Self::linha_do_cursor(sessao);
                let passo = espera.avancar(
                    Instant::now(),
                    &novo,
                    sessao.received(),
                    &linha,
                    sessao.closed(),
                );
                match passo {
                    Passo::Esperar(depois) => Sonda::Depois(depois),
                    Passo::Fim(motivo) => {
                        let texto = espera::saida_do_comando(espera.saida(), &comando, &finais);
                        let cabecalho =
                            json!({ "motivo": motivo.nome(), "codigo": null, "modo": "aba" })
                                .to_string();
                        Sonda::Fim(Saida::servidor("comando", texto, Some(cabecalho)))
                    }
                }
            }),
        );
    }

    fn executar_separado(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        sessao: Session,
        comando: String,
        prazo: Option<Duration>,
    ) {
        let Some(canais) = sessao.ssh_channels() else {
            return self.erro_mcp(&t, t!("mcp-so-ssh"));
        };
        // Cancelar o pedido fecha o canal, e o servidor mata o comando.
        self.em_thread_cancelavel(
            t,
            prazo,
            move |cancelado| {
                canais
                    .exec_cancelable(
                        &comando,
                        &mut std::io::empty(),
                        &mut |_| {},
                        Duration::from_secs(30),
                        &cancelado,
                    )
                    .map_err(|erro| erro.to_string())
            },
            |_, resultado| {
                Some(match resultado {
                    Ok(saida) => {
                        let mut texto = saida.stdout_text();
                        let erros = saida.stderr_text();
                        if !erros.is_empty() {
                            texto.push_str(&format!("\n{}\n{erros}", t!("mcp-saida-de-erro")));
                        }
                        let cabecalho =
                            json!({ "codigo": saida.status, "modo": "separado" }).to_string();
                        Saida::servidor("comando", texto, Some(cabecalho))
                    }
                    Err(erro) => Saida::Erro(sem_controle(&erro)),
                })
            },
        );
    }
}
