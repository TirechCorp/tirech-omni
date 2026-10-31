//! As ferramentas do MCP que trabalham numa pasta de um servidor (M51, seção 8): `listar_espacos`,
//! `ler`, `editar`, `escrever`, `buscar`, `glob` e `bash`. Cada espaço de trabalho tem uma
//! [`SessaoEspaco`], criada no primeiro pedido: um canal SFTP aberto pela conexão SSH de uma aba
//! da conexão do espaço (a aba da pessoa, se há; senão uma que o app abre e marca como da IA).
//! A decisão de permissão e a aprovação já aconteceram quando o código daqui roda.

use std::collections::HashSet;
use std::io::empty;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::Tela;
use super::mcp::{Plano, Saida, Sonda, Trabalho};
use crate::mcp::espaco::EspacoIa;
use crate::mcp::espaco_cmd::{
    PedidoDeBusca, RespostaDaBusca, SaidaDaBusca, comando_de_busca, comando_de_lista,
    comando_do_bash, interpretar_busca, interpretar_lista,
};
use crate::mcp::espaco_fs::{Remoto, casa_no_caminho, ordenar_por_data, texto_do_glob};
use crate::mcp::permissao::{PermissaoIa, permissao_efetiva};
use crate::modelo::{Connection, Kind};
use crate::t;
use crate::texto_limpo::sem_controle;

/// Quanto tempo a abertura do SFTP e da raiz do espaço pode levar.
const PRAZO_DA_ABERTURA_DO_SFTP: Duration = Duration::from_secs(60);

/// Quanto tempo uma aba nova espera o servidor aceitar a conexão.
const PRAZO_DA_ABA: Duration = Duration::from_secs(15);

/// Quanto tempo uma abertura espera a pessoa digitar a senha.
const PRAZO_DA_SENHA: Duration = Duration::from_secs(300);

/// Quanto tempo o servidor tem para aceitar um canal `exec`.
const PRAZO_DO_CANAL: Duration = Duration::from_secs(30);

/// O mais que o diálogo mostra do conteúdo de um `escrever`.
const LIMITE_DO_DETALHE: usize = 20_000;

/// A sessão de arquivos de um espaço de trabalho.
pub(super) struct SessaoEspaco {
    /// A conexão e a raiz com que foi aberta: se a pessoa muda o espaço, abre-se de novo.
    conexao: u64,
    raiz: String,
    /// A aba cuja conexão SSH leva o canal SFTP.
    aba: Session,
    /// Os arquivos, compartilhados com a thread que trabalha neles.
    remoto: Arc<Mutex<Remoto>>,
}

/// O que roda quando o espaço está pronto.
type Fazer = Box<dyn FnOnce(&Rc<Tela>, Rc<Trabalho>, Pronto)>;

/// O que roda quando a aba do espaço está pronta.
type Seguir = Box<dyn FnOnce(&Rc<Tela>, Rc<Trabalho>, Session)>;

/// O que um executor recebe quando o espaço está pronto.
struct Pronto {
    remoto: Arc<Mutex<Remoto>>,
    aba: Session,
    espaco: EspacoIa,
}

fn travar(remoto: &Mutex<Remoto>) -> MutexGuard<'_, Remoto> {
    remoto.lock().unwrap_or_else(PoisonError::into_inner)
}

fn texto_do(args: &Value, chave: &str) -> Option<String> {
    args[chave].as_str().map(str::to_owned)
}

impl Tela {
    /// O espaço de trabalho que o pedido aponta (`--espaco` da ponte ou o argumento `espaco`) e
    /// a conexão dele. Um espaço de conexão desligada para assistentes não existe, para quem
    /// pergunta.
    pub(super) fn espaco_do_pedido(
        &self,
        pedido: &crate::mcp::servidor::Pedido,
    ) -> Result<(EspacoIa, Connection), String> {
        let nome = pedido.args["espaco"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| pedido.espaco.clone())
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .ok_or_else(|| t!("mcp-falta-argumento", nome = "espaco"))?;
        let procurado = nome.to_lowercase();
        let espaco = self
            .config
            .with_untracked(|c| {
                c.ia.espacos
                    .iter()
                    .find(|e| e.nome.to_lowercase() == procurado)
                    .cloned()
            })
            .ok_or_else(|| t!("mcp-espaco-inexistente", nome = nome.as_str()))?;
        let conexao = self.tree.with_untracked(|tr| {
            let liberada = permissao_efetiva(tr, espaco.conexao) != PermissaoIa::Desligada;
            liberada
                .then(|| tr.conexao_para_abrir(espaco.conexao))
                .flatten()
        });
        let Some(conexao) = conexao else {
            return Err(t!("mcp-espaco-inexistente", nome = espaco.nome.as_str()));
        };
        if conexao.kind != Kind::Ssh {
            return Err(t!("mcp-espaco-sem-conexao", nome = espaco.nome.as_str()));
        }
        Ok((espaco, conexao))
    }

    /// O que o diálogo de aprovação mostra de um `editar` ou `escrever`.
    pub(super) fn detalhe_da_edicao(ferramenta: &str, args: &Value) -> String {
        let caminho = args["caminho"].as_str().unwrap_or_default();
        if ferramenta == "editar" {
            return t!(
                "mcp-detalhe-editar",
                caminho = caminho,
                antigo = args["antigo"].as_str().unwrap_or_default(),
                novo = args["novo"].as_str().unwrap_or_default()
            );
        }
        let conteudo = args["conteudo"].as_str().unwrap_or_default();
        let linhas = conteudo.matches('\n').count()
            + usize::from(!conteudo.is_empty() && !conteudo.ends_with('\n'));
        let mostrado: String = conteudo.chars().take(LIMITE_DO_DETALHE).collect();
        t!(
            "mcp-detalhe-escrever",
            caminho = caminho,
            linhas = linhas.to_string(),
            conteudo = mostrado
        )
    }

    /// `listar_espacos`: os espaços cuja conexão está liberada para assistentes.
    pub(super) fn mcp_listar_espacos(self: &Rc<Self>, t: Rc<Trabalho>) {
        let espacos = self.config.with_untracked(|c| c.ia.espacos.clone());
        let lista: Vec<Value> = self.tree.with_untracked(|tr| {
            espacos
                .iter()
                .filter(|e| permissao_efetiva(tr, e.conexao) != PermissaoIa::Desligada)
                .filter_map(|e| {
                    let c = tr.connection(e.conexao)?;
                    (c.kind == Kind::Ssh).then(|| {
                        json!({
                            "nome": e.nome,
                            "conexao": c.name,
                            "host": c.host,
                            "raiz": e.raiz,
                        })
                    })
                })
                .collect()
        });
        let texto = serde_json::to_string_pretty(&lista).unwrap_or_else(|_| "[]".to_owned());
        self.terminar_mcp(&t, Saida::servidor("espacos", texto, None));
    }

    /// Acha ou cria a sessão do espaço do pedido e entrega a `fazer`.
    fn com_o_espaco(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        plano: &Plano,
        fazer: impl FnOnce(&Rc<Tela>, Rc<Trabalho>, Pronto) + 'static,
    ) {
        let (espaco, conexao) = match self.espaco_do_pedido(&t.pedido) {
            Ok(par) => par,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        if plano.conexao.as_ref().is_some_and(|c| c.id != conexao.id) {
            return self.erro_mcp(&t, t!("mcp-conexao-invalida"));
        }
        // Sessões de abas que fecharam ou caíram não servem mais.
        self.mcp
            .espacos
            .borrow_mut()
            .retain(|_, s| !s.aba.closed() && self.sessao_por_chave(&s.aba.key_str()).is_some());
        let existente = self.mcp.espacos.borrow().get(&espaco.nome).and_then(|s| {
            (s.conexao == espaco.conexao && s.raiz == espaco.raiz)
                .then(|| (Arc::clone(&s.remoto), s.aba))
        });
        if let Some((remoto, aba)) = existente {
            return fazer(
                self,
                t,
                Pronto {
                    remoto,
                    aba,
                    espaco,
                },
            );
        }
        self.mcp.espacos.borrow_mut().remove(&espaco.nome);
        match self.aba_ssh_da_conexao(conexao.id) {
            Some(aba) => self.abrir_o_remoto(t, aba, espaco, Box::new(fazer)),
            None => {
                if let Err(erro) = self.pode_abrir_aba(conexao.id, plano.abre_aba) {
                    return self.erro_mcp(&t, erro);
                }
                self.abrir_a_aba_do_espaco(
                    t,
                    conexao,
                    Box::new(move |tela, t, aba| {
                        tela.abrir_o_remoto(t, aba, espaco, Box::new(fazer))
                    }),
                );
            }
        }
    }

    /// Uma aba SSH de pé da conexão `id`, que o espaço pode usar sem abrir outra.
    fn aba_ssh_da_conexao(&self, id: u64) -> Option<Session> {
        self.aba_da_conexao(id)
            .map(|(_, s)| s)
            .filter(|s| !s.closed() && s.ssh_channels().is_some())
    }

    /// Se usar o espaço `espaco` vai abrir uma aba nova da conexão `conexao`: não há a sessão do
    /// espaço de pé nem uma aba SSH da conexão. Não mexe em nada (é para decidir antes).
    pub(super) fn espaco_abre_aba(&self, espaco: &EspacoIa, conexao: u64) -> bool {
        let de_pe = self
            .mcp
            .espacos
            .borrow()
            .get(&espaco.nome)
            .is_some_and(|s| {
                s.conexao == espaco.conexao
                    && s.raiz == espaco.raiz
                    && !s.aba.closed()
                    && self.sessao_por_chave(&s.aba.key_str()).is_some()
            });
        !de_pe && self.aba_ssh_da_conexao(conexao).is_none()
    }

    /// Abre o SFTP pela conexão da aba e confere a raiz, numa thread.
    fn abrir_o_remoto(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        aba: Session,
        espaco: EspacoIa,
        fazer: Fazer,
    ) {
        let abrir = match self.opener.arquivos_da_sessao(aba) {
            Ok(abrir) => abrir,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let raiz = espaco.raiz.clone();
        let trabalho = Rc::clone(&t);
        self.em_thread_cancelavel(
            t,
            Some(PRAZO_DA_ABERTURA_DO_SFTP),
            move |_| {
                let fs = abrir().map_err(|e| sem_controle(&e.to_string()))?;
                Remoto::abrir(fs, &raiz)
            },
            move |tela, resultado| match resultado {
                Ok(remoto) => {
                    let remoto = Arc::new(Mutex::new(remoto));
                    tela.mcp.espacos.borrow_mut().insert(
                        espaco.nome.clone(),
                        SessaoEspaco {
                            conexao: espaco.conexao,
                            raiz: espaco.raiz.clone(),
                            aba,
                            remoto: Arc::clone(&remoto),
                        },
                    );
                    fazer(
                        tela,
                        trabalho,
                        Pronto {
                            remoto,
                            aba,
                            espaco,
                        },
                    );
                    None
                }
                Err(erro) => Some(Saida::Erro(erro)),
            },
        );
    }

    /// Abre uma aba da conexão do espaço, como o `abrir_conexao` (pedindo a senha à pessoa, se
    /// não estiver salva), com o título "{conexão} (IA)", e espera o servidor aceitar.
    fn abrir_a_aba_do_espaco(
        self: &Rc<Self>,
        t: Rc<Trabalho>,
        conexao: Connection,
        seguir: Seguir,
    ) {
        let lugar = match self.lugar_do_assistente(&json!({})) {
            Ok(lugar) => lugar,
            Err(erro) => return self.erro_mcp(&t, erro),
        };
        let antes: HashSet<String> = self
            .sessoes_das_janelas(None)
            .iter()
            .map(|(_, s)| s.key_str())
            .collect();
        self.abrir_com(conexao.id, lugar, true);
        let (id, prefixo) = (conexao.id, format!("{}-", conexao.id));
        let inicio = Instant::now();
        // A aba achada e quando começou a esperar por ela.
        let mut achada: Option<(Session, Instant)> = None;
        let mut seguir = Some(seguir);
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
                            sessao.name(t!("mcp-aba-do-espaco", nome = conexao.name.as_str()));
                            tela.marcar_aba_mcp(&trabalho, &chave);
                            tela.dar_aba(&chave, trabalho.pedido.ligacao);
                            achada = Some((sessao, agora));
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
                let Some((sessao, desde)) = achada else {
                    return Sonda::Depois(Duration::from_millis(100));
                };
                if sessao.closed() {
                    let motivo = sessao.close_reason().unwrap_or_else(|| t!("mcp-aba-caida"));
                    return Sonda::Fim(Saida::Erro(sem_controle(&motivo)));
                }
                // O primeiro texto do servidor diz que a conexão está de pé.
                let de_pe = sessao.received() > 0;
                if !de_pe && agora.duration_since(desde) < PRAZO_DA_ABA {
                    return Sonda::Depois(Duration::from_millis(100));
                }
                if let Some(seguir) = seguir.take() {
                    seguir(tela, Rc::clone(&trabalho), sessao);
                }
                Sonda::Seguiu
            }),
        );
    }

    /// `ler`: um trecho numerado de um arquivo do espaço.
    pub(super) fn mcp_ler(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(caminho) = texto_do(args, "caminho") else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "caminho"));
        };
        let (inicio, linhas) = (args["inicio"].as_u64(), args["linhas"].as_u64());
        let padrao = self
            .config
            .with_untracked(|c| c.ia.linhas_por_leitura as usize);
        self.com_o_espaco(t, &plano, move |tela, t, pronto| {
            tela.em_thread(
                t,
                Some(Duration::from_secs(120)),
                move || travar(&pronto.remoto).ler(&caminho, inicio, linhas, padrao),
                |_, resultado| match resultado {
                    Ok(texto) => Saida::servidor("arquivo", texto, None),
                    Err(erro) => Saida::Erro(erro),
                },
            );
        });
    }

    /// `editar`: troca um trecho de um arquivo que o assistente leu.
    pub(super) fn mcp_editar(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(caminho) = texto_do(args, "caminho") else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "caminho"));
        };
        let Some(antigo) = texto_do(args, "antigo") else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "antigo"));
        };
        let Some(novo) = texto_do(args, "novo") else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "novo"));
        };
        let todas = args["todas"].as_bool().unwrap_or(false);
        self.com_o_espaco(t, &plano, move |tela, t, pronto| {
            tela.em_thread(
                t,
                Some(Duration::from_secs(120)),
                move || travar(&pronto.remoto).editar(&caminho, &antigo, &novo, todas),
                |_, resultado| match resultado {
                    Ok(texto) => Saida::App(texto),
                    Err(erro) => Saida::Erro(erro),
                },
            );
        });
    }

    /// `escrever`: grava um arquivo inteiro.
    pub(super) fn mcp_escrever(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(caminho) = texto_do(args, "caminho") else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "caminho"));
        };
        let Some(conteudo) = texto_do(args, "conteudo") else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "conteudo"));
        };
        self.com_o_espaco(t, &plano, move |tela, t, pronto| {
            tela.em_thread(
                t,
                Some(Duration::from_secs(120)),
                move || travar(&pronto.remoto).escrever(&caminho, &conteudo),
                |_, resultado| match resultado {
                    Ok(texto) => Saida::App(texto),
                    Err(erro) => Saida::Erro(erro),
                },
            );
        });
    }

    /// `buscar`: pelo `git grep` ou `grep` do servidor; sem eles (ou com um `glob` de caminho),
    /// pelo SFTP.
    pub(super) fn mcp_buscar(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(padrao) = texto_do(args, "padrao").filter(|p| !p.is_empty()) else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "padrao"));
        };
        let pedido = PedidoDeBusca {
            padrao,
            regex: args["modo"].as_str() == Some("regex"),
            caminho: texto_do(args, "caminho")
                .filter(|c| !c.is_empty())
                .unwrap_or_else(|| ".".to_owned()),
            glob: texto_do(args, "glob").filter(|g| !g.is_empty()),
            maiusculas: args["maiusculas"].as_bool().unwrap_or(true),
            contexto: args["contexto"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0)
                .min(50),
            saida: SaidaDaBusca::do_argumento(args["saida"].as_str()),
        };
        self.com_o_espaco(t, &plano, move |tela, t, pronto| {
            let Some(canais) = pronto.aba.ssh_channels() else {
                return tela.erro_mcp(&t, t!("mcp-so-ssh"));
            };
            let raiz = pronto.espaco.raiz.clone();
            tela.em_thread_cancelavel(
                t,
                Some(Duration::from_secs(300)),
                move |cancelado| {
                    let mut pedido = pedido;
                    pedido.caminho = travar(&pronto.remoto).caminho_relativo(&pedido.caminho)?;
                    // O `--include` do grep só olha o nome do arquivo: um glob com `/` vai
                    // direto para a busca pelo SFTP.
                    let glob_de_caminho = pedido.glob.as_deref().is_some_and(|g| g.contains('/'));
                    if !glob_de_caminho {
                        let comando = comando_de_busca(&raiz, &pedido);
                        let rodou = canais.exec_cancelable(
                            &comando,
                            &mut empty(),
                            &mut |_| {},
                            PRAZO_DO_CANAL,
                            &cancelado,
                        );
                        if let Ok(saida) = rodou {
                            match interpretar_busca(
                                saida.status,
                                &saida.stdout_text(),
                                &saida.stderr_text(),
                                pedido.saida,
                            ) {
                                RespostaDaBusca::Pronta(texto) => return Ok(texto),
                                RespostaDaBusca::Erro(erro) => return Err(erro),
                                RespostaDaBusca::Reserva => {}
                            }
                        }
                    }
                    travar(&pronto.remoto).buscar_pelo_sftp(&pedido)
                },
                |_, resultado| {
                    Some(match resultado {
                        Ok(texto) => Saida::servidor("busca", texto, None),
                        Err(erro) => Saida::Erro(erro),
                    })
                },
            );
        });
    }

    /// `glob`: os arquivos que casam com o padrão, os mais recentes primeiro. Pelo `git
    /// ls-files` ou `find` do servidor; sem eles, pelo SFTP.
    pub(super) fn mcp_glob(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(padrao) = texto_do(args, "padrao").filter(|p| !p.is_empty()) else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "padrao"));
        };
        let caminho = texto_do(args, "caminho")
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| ".".to_owned());
        self.com_o_espaco(t, &plano, move |tela, t, pronto| {
            let canais = pronto.aba.ssh_channels();
            let raiz = pronto.espaco.raiz.clone();
            tela.em_thread_cancelavel(
                t,
                Some(Duration::from_secs(300)),
                move |cancelado| {
                    let base = travar(&pronto.remoto).caminho_relativo(&caminho)?;
                    let listada = canais.and_then(|canais| {
                        let comando = comando_de_lista(&raiz, &base);
                        let saida = canais
                            .exec_cancelable(
                                &comando,
                                &mut empty(),
                                &mut |_| {},
                                PRAZO_DO_CANAL,
                                &cancelado,
                            )
                            .ok()?;
                        interpretar_lista(saida.status, &saida.stdout_text())
                    });
                    let mut lista = match listada {
                        Some(mut lista) => {
                            lista.retain(|a| casa_no_caminho(&padrao, &base, &a.caminho));
                            travar(&pronto.remoto).completar_datas(&mut lista);
                            lista
                        }
                        None => travar(&pronto.remoto).glob_pelo_sftp(&padrao, &caminho)?,
                    };
                    ordenar_por_data(&mut lista);
                    Ok::<_, String>(texto_do_glob(&lista, &padrao))
                },
                |_, resultado| {
                    Some(match resultado {
                        Ok(texto) => Saida::servidor("glob", texto, None),
                        Err(erro) => Saida::Erro(erro),
                    })
                },
            );
        });
    }

    /// `bash`: roda o comando na raiz do espaço, num canal `exec` (sem terminal). Cancelar o
    /// pedido fecha o canal.
    pub(super) fn mcp_bash(self: &Rc<Self>, t: Rc<Trabalho>, plano: Plano) {
        let args = &t.pedido.args;
        let Some(comando) = texto_do(args, "comando").filter(|c| !c.is_empty()) else {
            return self.erro_mcp(&t, t!("mcp-falta-argumento", nome = "comando"));
        };
        let entrada = texto_do(args, "entrada").unwrap_or_default();
        let prazo = args["prazo_secs"].as_u64().map(Duration::from_secs);
        self.com_o_espaco(t, &plano, move |tela, t, pronto| {
            let Some(canais) = pronto.aba.ssh_channels() else {
                return tela.erro_mcp(&t, t!("mcp-so-ssh"));
            };
            let completo = comando_do_bash(&pronto.espaco.raiz, &comando);
            tela.em_thread_cancelavel(
                t,
                prazo,
                move |cancelado: Arc<AtomicBool>| {
                    let saida = canais.exec_cancelable(
                        &completo,
                        &mut entrada.as_bytes(),
                        &mut |_| {},
                        PRAZO_DO_CANAL,
                        &cancelado,
                    );
                    // O comando pode ter mexido nos arquivos: o que se leu antes não vale.
                    travar(&pronto.remoto).esquecer_leituras();
                    saida.map_err(|erro| sem_controle(&erro.to_string()))
                },
                |_, resultado| {
                    Some(match resultado {
                        Ok(saida) => {
                            let mut texto = saida.stdout_text();
                            let erros = saida.stderr_text();
                            if !erros.is_empty() {
                                texto.push_str(&format!("\n{}\n{erros}", t!("mcp-saida-de-erro")));
                            }
                            let cabecalho = json!({ "codigo": saida.status }).to_string();
                            Saida::servidor("bash", texto, Some(cabecalho))
                        }
                        Err(erro) => Saida::Erro(erro),
                    })
                },
            );
        });
    }
}
