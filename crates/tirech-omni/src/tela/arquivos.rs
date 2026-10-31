//! A página Arquivos na casca: abrir abas (com a senha do cofre ou o diálogo), os recentes de
//! pastas, o terminal da sessão, o fechamento com transferências e o estado no `estado.json`.
//! As views ficam em `crate::arquivos`.

use std::path::Path;
use std::rc::Rc;

use tomni_ui::files::{Conflict, Endpoint, Opener as ArquivosOpener, TransferState};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{Lugar, Pedido, Recusa, Tela, icone_do_cartao, rclone};
use crate::abrir::{Senhas, pede_senha_na_recusa, precisa_de_senha};
use crate::arquivos::editar::{self, Edicao};
use crate::arquivos::s3::AcoesS3;
use crate::arquivos::{
    self as vistas, Aba, ConexaoDeArquivos, Contexto, Entrada, Lado, fila, pasta_inicial,
};
use crate::configuracoes::EnviarEditado;
use crate::espacos::ArquivoSalvo;
use crate::modelo::{Connection, FtpSeguranca, Kind};
use crate::paineis::PaineisDaPagina;
use crate::recentes::Alvo as AlvoRecente;
use crate::t;

impl Tela {
    /// Abre os arquivos da conexão `id` numa aba nova e vai para a página.
    pub(super) fn abrir_arquivos(&self, id: u64) {
        self.abrir_arquivos_em(id, None, None, true);
    }

    /// Abre os arquivos da conexão `id`; `remoto` e `local` são as pastas em que os painéis
    /// abrem. Com `mostrar`, a tela vai para a página Arquivos (a restauração do estado não vai).
    pub(super) fn abrir_arquivos_em(
        &self,
        id: u64,
        remoto: Option<String>,
        local: Option<String>,
        mostrar: bool,
    ) {
        let Some(c) = self.tree.with_untracked(|t| t.conexao_para_abrir(id)) else {
            return;
        };
        let senhas = self.senhas_do_cofre(&c);
        if precisa_de_senha(&c) && senhas.conexao.is_none() {
            self.iniciais_arquivos
                .borrow_mut()
                .push((id, remoto, local));
            self.pendentes.update(|p| {
                p.push(Pedido {
                    id,
                    lugar: Lugar::default(),
                    recusa: Recusa::Nao,
                    registrar: mostrar,
                    arquivos: true,
                    programa: None,
                });
            });
        } else {
            self.abrir_arquivos_agora(&c, senhas, remoto, local, mostrar);
        }
    }

    /// A senha do diálogo chegou para uma abertura de arquivos.
    pub(super) fn concluir_senha_de_arquivos(&self, c: &Connection, senhas: Senhas, mostrar: bool) {
        let (remoto, local) = {
            let mut iniciais = self.iniciais_arquivos.borrow_mut();
            match iniciais.iter().position(|(id, _, _)| *id == c.id) {
                Some(n) => {
                    let (_, remoto, local) = iniciais.remove(n);
                    (remoto, local)
                }
                None => (None, None),
            }
        };
        self.abrir_arquivos_agora(c, senhas, remoto, local, mostrar);
    }

    /// Pede o abridor ao `Opener` e cria a aba. Uma autenticação recusada volta ao diálogo.
    fn abrir_arquivos_agora(
        &self,
        c: &Connection,
        senhas: Senhas,
        remoto: Option<String>,
        local: Option<String>,
        mostrar: bool,
    ) {
        if c.kind == Kind::Rclone {
            // O `rcd` sobe numa thread; a aba nasce quando ele está pronto.
            self.abrir_remoto(
                c.name.clone(),
                Some(c.id),
                c.rclone.clone(),
                remoto,
                local,
                mostrar,
            );
            return;
        }
        match self
            .opener
            .arquivos(&self.conexao_efetiva(c), senhas.clone())
        {
            Ok(servidor) => {
                let mut aba =
                    self.nova_aba(c.name.clone(), Some(c.id), None, remoto, local, servidor);
                if c.kind == Kind::S3
                    && let Ok(config) = self.opener.s3_config(&self.conexao_efetiva(c), senhas)
                {
                    aba.s3 = Some(AcoesS3::nova(config));
                }
                aba.sem_tls
                    .set(c.kind == Kind::Ftp && c.ftp_security == FtpSeguranca::Nenhuma);
                self.entrar_na_aba(aba, mostrar);
            }
            Err(motivo) => {
                if crate::abrir::e_recusa_de_autenticacao(&motivo) && pede_senha_na_recusa(c) {
                    self.iniciais_arquivos
                        .borrow_mut()
                        .push((c.id, remoto, local));
                    self.pendentes.update(|p| {
                        p.push(Pedido {
                            id: c.id,
                            lugar: Lugar::default(),
                            recusa: self.recusa_de(c.id, &motivo),
                            registrar: mostrar,
                            arquivos: true,
                            programa: None,
                        });
                    });
                } else {
                    self.avisar(
                        InfoBarSeverity::Error,
                        t!(
                            "arquivos-nao-foi-possivel-abrir-de",
                            nome = c.name.as_str(),
                            motivo = motivo.to_string()
                        ),
                    );
                }
            }
        }
    }

    /// Uma aba nova, com a chave `a<n>`.
    pub(super) fn nova_aba(
        &self,
        titulo: String,
        conexao: Option<u64>,
        sessao: Option<Session>,
        remoto: Option<String>,
        local: Option<String>,
        servidor: ArquivosOpener,
    ) -> Aba {
        let n = self.proxima_aba.get() + 1;
        self.proxima_aba.set(n);
        let config = self.config.with_untracked(|c| c.arquivos.clone());
        // A pasta explícita vence a da conexão, que vence a das Configurações (só no disco).
        let (pasta_local, pasta_remota, ativar) = conexao
            .and_then(|id| {
                self.tree.with_untracked(|t| {
                    t.connection(id).map(|c| {
                        (
                            c.pasta_local.clone(),
                            c.pasta_remota.clone(),
                            Some((c.comparar, c.sincronizar)),
                        )
                    })
                })
            })
            .unwrap_or_default();
        let lembrada = (local.is_some(), remoto.is_some());
        let inicial = pasta_inicial(remoto, &pasta_remota, "");
        let local_inicial = pasta_inicial(local, &pasta_local, &config.pasta_local);
        let mut aba = Aba::nova(
            format!("a{n}"),
            titulo,
            conexao,
            sessao,
            inicial,
            &self.opener.disco(),
            servidor,
            &config,
        );
        aba.local_inicial = local_inicial;
        aba.pasta_lembrada = lembrada;
        // A conexão decide se a comparação e a navegação sincronizada ligam ao abrir; "Padrão"
        // segue as Configurações.
        if let Some((comparar, sincronizar)) = ativar {
            aba.comparar.set(comparar.resolver(config.comparar));
            aba.sincronizar
                .set(sincronizar.resolver(config.sincronizar));
        }
        aba
    }

    /// Põe a aba na página, escolhe e, com `mostrar`, vai para a página Arquivos.
    pub(super) fn entrar_na_aba(&self, aba: Aba, mostrar: bool) {
        let chave = aba.chave.clone();
        self.marcar_rede_da_aba(&chave, aba.conexao, aba.sessao);
        self.abas_arquivos.update(|a| a.push(aba));
        self.aba_arquivos.set(chave);
        if mostrar {
            self.ir_para("arquivos");
        }
    }

    /// Abre os arquivos pela conexão SSH da sessão (sem autenticar de novo). Se a sessão já tem
    /// uma aba, só escolhe ela.
    pub(super) fn abrir_arquivos_da_sessao(&self, sessao: Session) {
        let existente = self
            .abas_arquivos
            .with_untracked(|a| a.iter().find(|a| a.sessao == Some(sessao)).cloned());
        if let Some(aba) = existente {
            self.aba_arquivos.set(aba.chave);
            self.ir_para("arquivos");
            return;
        }
        let servidor = match self.opener.arquivos_da_sessao(sessao) {
            Ok(servidor) => servidor,
            Err(motivo) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!(
                        "arquivos-nao-foi-possivel-abrir-da-sessao",
                        motivo = motivo.to_string()
                    ),
                );
                return;
            }
        };
        let conexao = sessao
            .key_str()
            .split('-')
            .next()
            .and_then(|id| id.parse::<u64>().ok());
        let nome = conexao
            .and_then(|id| {
                self.tree
                    .with_untracked(|t| t.connection(id).map(|c| c.name.clone()))
            })
            .unwrap_or_else(|| sessao.title());
        let aba = self.nova_aba(
            t!("arquivos-nome-da-sessao", nome = nome.as_str()),
            conexao,
            Some(sessao),
            None,
            None,
            servidor,
        );
        self.entrar_na_aba(aba, true);
    }

    /// O botão de fechar a aba: pergunta antes se ela tem transferências em andamento.
    pub(super) fn pedir_fechar_aba(&self, chave: &str) {
        let Some(aba) = self
            .abas_arquivos
            .with_untracked(|a| a.iter().find(|a| a.chave == chave).cloned())
        else {
            return;
        };
        let ativas = aba.ativas(&self.transferencias);
        if ativas == 0 {
            self.fechar_aba(chave);
        } else {
            *self.fechando_aba.borrow_mut() = Some(chave.to_owned());
            self.fechando_quantas.set(ativas);
            self.fechando_aberto.set(true);
        }
    }

    /// Cancela o que a aba pediu, encerra as conexões dela e a tira da página.
    pub(super) fn fechar_aba(&self, chave: &str) {
        let Some(aba) = self
            .abas_arquivos
            .with_untracked(|a| a.iter().find(|a| a.chave == chave).cloned())
        else {
            return;
        };
        for id in aba.ids.get_untracked() {
            self.transferencias.cancel(id);
        }
        aba.fechar();
        self.esquecer_edicoes_da_aba(chave);
        let escolhida = self.aba_arquivos.get_untracked() == chave;
        let vizinha = self.abas_arquivos.with_untracked(|a| {
            let n = a.iter().position(|a| a.chave == chave).unwrap_or(0);
            let restantes: Vec<&Aba> = a.iter().filter(|a| a.chave != chave).collect();
            restantes
                .get(n.min(restantes.len().saturating_sub(1)))
                .map(|a| a.chave.clone())
        });
        self.abas_arquivos
            .update(|a| a.retain(|a| a.chave != chave));
        if escolhida {
            self.aba_arquivos.set(vizinha.unwrap_or_default());
        }
    }

    /// Reconecta a aba que falhou. Uma senha recusada fecha a aba e volta ao diálogo de senha.
    fn refazer_aba(&self, aba: &Aba) {
        let recusada = aba.remoto.remote.state().with_untracked(|e| {
            matches!(e, tomni_ui::files::RemoteState::Failed(m) if crate::abrir::e_recusa_de_autenticacao(m))
        });
        if recusada
            && aba.sessao.is_none()
            && let Some(id) = aba.conexao
            && let Some(c) = self.tree.with_untracked(|t| t.conexao_para_abrir(id))
            && pede_senha_na_recusa(&c)
        {
            let remoto = aba.remoto.caminho.get_untracked();
            let local = aba.local.caminho.get_untracked();
            self.fechar_aba(&aba.chave);
            self.iniciais_arquivos.borrow_mut().push((
                id,
                (!remoto.is_empty()).then_some(remoto),
                (!local.is_empty()).then_some(local),
            ));
            self.pendentes.update(|p| {
                p.push(Pedido {
                    id,
                    lugar: Lugar::default(),
                    recusa: Recusa::Conexao,
                    registrar: false,
                    arquivos: true,
                    programa: None,
                });
            });
            return;
        }
        aba.remoto.remote.reconnect();
        aba.remoto.atualizar();
    }

    /// "Abrir terminal aqui": manda o `cd` para a sessão e mostra os Terminais.
    fn terminal_aqui(&self, sessao: Session, caminho: &str) {
        let seguro = caminho.replace('\'', r"'\''");
        sessao.send(format!("cd '{seguro}'\r").as_bytes());
        self.ir_para("terminais");
        for janela in self.janelas.todas_agora() {
            let tem = janela
                .layout
                .with_untracked(|l| l.sessions().contains(&sessao));
            if tem {
                janela.layout.update(|l| l.select(sessao));
                break;
            }
        }
    }

    /// A pasta remota da aba mudou: guarda nos recentes.
    fn lembrar_pasta(&self, aba: &Aba, caminho: &str) {
        let Some(conexao) = aba.conexao else { return };
        if caminho.is_empty() {
            return;
        }
        let titulo = format!(
            "{}: {}",
            aba.titulo,
            tomni_ui::files::path::file_name(caminho)
        );
        self.recentes.update(|r| {
            r.registrar(
                AlvoRecente::Local {
                    conexao,
                    caminho: caminho.to_owned(),
                },
                &titulo,
                caminho,
            );
        });
    }

    /// Duplo clique num arquivo: o local abre direto no editor; o remoto é baixado para a pasta
    /// temporária e, quando chega, abre (a [`Tela::acompanhar_edicoes`] vigia o resto).
    fn abrir_arquivo(&self, aba: &Aba, lado: Lado, nome: &str) {
        match lado {
            Lado::Local => self.abrir_no_editor(Path::new(&aba.local.caminho_de(nome))),
            Lado::Remoto => self.editar_remoto(aba, nome),
        }
    }

    /// Manda o `arquivo` (que já está no disco) para o editor externo.
    pub(super) fn abrir_no_editor(&self, arquivo: &Path) {
        let editor = self.config.with_untracked(|c| c.arquivos.editor.clone());
        if let Err(erro) = self.opener.abrir_externo(arquivo, &editor) {
            self.avisar(InfoBarSeverity::Error, erro);
        }
    }

    /// Baixa o arquivo remoto `nome` da aba e o abre no editor quando a cópia chegar.
    fn editar_remoto(&self, aba: &Aba, nome: &str) {
        let remoto = aba.remoto.caminho_de(nome);
        let ja_aberto = self
            .edicoes
            .borrow()
            .iter()
            .find(|e| e.aba.chave == aba.chave && e.remoto == remoto)
            .map(|e| (e.baixando.is_some(), e.local.clone()));
        match ja_aberto {
            // Ainda baixando: o duplo clique a mais não faz nada.
            Some((true, _)) => return,
            // Já está no editor: só o traz de volta, sem baixar por cima do que se digitou.
            Some((false, local)) => return self.abrir_no_editor(&local),
            None => {}
        }
        let local = editar::pasta_temporaria(
            &self.ferramentas.ambiente().pasta_temporaria(),
            aba.conexao,
            &remoto,
        );
        if let Some(pasta) = local.parent()
            && let Err(erro) = std::fs::create_dir_all(pasta)
        {
            self.avisar(
                InfoBarSeverity::Error,
                t!(
                    "arquivos-nao-consegui-criar",
                    arquivo = pasta.display().to_string(),
                    erro = erro.to_string()
                ),
            );
            return;
        }
        let baixando = self.transferencias.add(
            (aba.remoto.endpoint.clone(), remoto.clone()),
            (Endpoint::Local, editar::caminho_em_texto(&local)),
            Conflict::Overwrite,
        );
        aba.ids.update(|ids| ids.push(baixando));
        let id = self.proxima_edicao.get() + 1;
        self.proxima_edicao.set(id);
        self.edicoes.borrow_mut().push(Edicao {
            id,
            aba: aba.clone(),
            remoto,
            local,
            baixando: Some(baixando),
            sempre: self.config.with_untracked(|c| c.arquivos.enviar_editado)
                == EnviarEditado::Sempre,
        });
    }

    /// Liga a tela ao que acontece com as edições: abre no editor a cópia que acabou de chegar
    /// e pergunta (ou envia) quando o vigia vê o arquivo mudar.
    pub(super) fn acompanhar_edicoes(self: &Rc<Self>) {
        {
            let (tela, itens) = (Rc::clone(self), self.transferencias.items());
            effect(move || {
                let itens = itens.get();
                untrack(|| tela.baixadas(&itens));
            });
        }
        let (tela, alterados) = (Rc::clone(self), self.vigia.alterados());
        effect(move || {
            // Esvaziar a lista avisa o próprio efeito: só mexe quando há o que tirar.
            if alterados.with(Vec::is_empty) {
                return;
            }
            untrack(|| {
                let mut ids = Vec::new();
                alterados.update(|lista| ids = std::mem::take(lista));
                for id in ids {
                    tela.edicao_mudou(id);
                }
            });
        });
    }

    /// Olha as cópias que estavam sendo baixadas: a que terminou vai para o editor e passa a ser
    /// vigiada; a que não deu certo é esquecida (a fila já mostra o motivo).
    fn baixadas(&self, itens: &[tomni_ui::files::TransferInfo]) {
        let mut prontas = Vec::new();
        self.edicoes.borrow_mut().retain_mut(|e| {
            let Some(baixando) = e.baixando else {
                return true;
            };
            let Some(info) = itens.iter().find(|i| i.id == baixando) else {
                return true;
            };
            match info.state {
                TransferState::Done => {
                    e.baixando = None;
                    prontas.push((e.id, e.local.clone()));
                    true
                }
                TransferState::Failed(_) | TransferState::Skipped | TransferState::Cancelled => {
                    false
                }
                _ => true,
            }
        });
        for (id, local) in prontas {
            self.vigia
                .observar(id, local.clone(), editar::data_de(&local));
            self.abrir_no_editor(&local);
        }
    }

    /// O arquivo da edição `id` mudou no disco: envia, se a pessoa já disse "sempre", ou pergunta.
    fn edicao_mudou(&self, id: u64) {
        let sempre = self
            .edicoes
            .borrow()
            .iter()
            .find(|e| e.id == id && e.baixando.is_none())
            .map(|e| e.sempre);
        match sempre {
            Some(true) => self.enviar_edicao(id),
            Some(false) => self.edicao_pergunta.update(|p| {
                if !p.contains(&id) {
                    p.push(id);
                }
            }),
            None => {}
        }
    }

    /// Manda a cópia local da edição `id` de volta ao servidor, pela fila e por cima do arquivo.
    fn enviar_edicao(&self, id: u64) {
        let Some((aba, remoto, local)) = self
            .edicoes
            .borrow()
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.aba.clone(), e.remoto.clone(), e.local.clone()))
        else {
            return;
        };
        let tamanho = std::fs::metadata(&local).ok().map(|m| m.len());
        let enviando = self.transferencias.add_sized(
            (Endpoint::Local, editar::caminho_em_texto(&local)),
            (aba.remoto.endpoint.clone(), remoto),
            Conflict::Overwrite,
            tamanho,
        );
        aba.ids.update(|ids| ids.push(enviando));
    }

    /// A resposta à pergunta "Enviar?" da frente da fila; `sempre` passa a enviar sem perguntar.
    fn responder_edicao(&self, enviar: bool, sempre: bool) {
        let Some(id) = self.edicao_pergunta.with_untracked(|p| p.first().copied()) else {
            return;
        };
        self.edicao_pergunta.update(|p| {
            p.remove(0);
        });
        if sempre && let Some(e) = self.edicoes.borrow_mut().iter_mut().find(|e| e.id == id) {
            e.sempre = true;
        }
        if enviar {
            self.enviar_edicao(id);
        }
    }

    /// O nome do arquivo da pergunta "Enviar?" que está na frente.
    fn nome_da_pergunta(&self) -> String {
        let Some(id) = self.edicao_pergunta.with(|p| p.first().copied()) else {
            return String::new();
        };
        self.edicoes
            .borrow()
            .iter()
            .find(|e| e.id == id)
            .map(|e| t!("arquivos-foi-alterado-enviar", nome = e.nome()))
            .unwrap_or_default()
    }

    /// Esquece as edições de uma aba que fechou.
    fn esquecer_edicoes_da_aba(&self, chave: &str) {
        let esquecidas: Vec<u64> = self
            .edicoes
            .borrow()
            .iter()
            .filter(|e| e.aba.chave == chave)
            .map(|e| e.id)
            .collect();
        for id in &esquecidas {
            self.vigia.esquecer(*id);
        }
        self.edicoes.borrow_mut().retain(|e| e.aba.chave != chave);
        self.edicao_pergunta
            .update(|p| p.retain(|id| !esquecidas.contains(id)));
    }

    /// As abas e as pastas delas, para o `estado.json`. As que nasceram de uma sessão voltam
    /// como a conexão própria.
    pub(super) fn arquivos_salvos(&self) -> Vec<ArquivoSalvo> {
        self.abas_arquivos.with_untracked(|abas| {
            abas.iter()
                .filter_map(|a| {
                    Some(ArquivoSalvo {
                        conexao: a.conexao?,
                        local: a.local.caminho.get_untracked(),
                        remoto: a.remoto.caminho.get_untracked(),
                    })
                })
                .collect()
        })
    }

    /// Reabre as abas do `estado.json`, sem ir para a página.
    pub(super) fn restaurar_arquivos(&self, abas: &[ArquivoSalvo]) {
        for salva in abas {
            let pasta = |p: &str| (!p.is_empty()).then(|| p.to_owned());
            self.abrir_arquivos_em(
                salva.conexao,
                pasta(&salva.remoto),
                pasta(&salva.local),
                false,
            );
        }
    }
}

/// O contexto das views: o que elas pedem à tela.
pub(super) fn contexto(tela: &Rc<Tela>) -> Rc<Contexto> {
    let (t_abrir, t_refazer, t_terminal, t_lembrar, t_ssh, t_espaco) = (
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
        Rc::clone(tela),
    );
    Rc::new(Contexto {
        transferencias: tela.transferencias.clone(),
        abrir_arquivo: Box::new(move |aba, lado, entrada| {
            t_abrir.abrir_arquivo(aba, lado, &entrada.name);
        }),
        refazer: Box::new(move |aba| t_refazer.refazer_aba(aba)),
        terminal_aqui: Box::new(move |sessao, caminho| t_terminal.terminal_aqui(sessao, caminho)),
        lembrar: Box::new(move |aba, caminho| t_lembrar.lembrar_pasta(aba, caminho)),
        config: tela.config,
        aba_ssh: Box::new(move |aba| {
            aba.conexao.is_some_and(|id| {
                t_ssh
                    .tree
                    .with_untracked(|t| t.connection(id).is_some_and(|c| c.kind == Kind::Ssh))
            })
        }),
        espaco_de_ia: Box::new(move |aba, pasta| t_espaco.novo_espaco_ia(aba.conexao, pasta)),
    })
}

/// O detalhe do cartão de uma conexão: o servidor e o tipo (`tirech@host:22 · SFTP`).
fn detalhe_do_cartao(c: &Connection) -> String {
    let servidor = Tela::detalhe_de(c);
    if servidor == c.kind.nome() {
        servidor
    } else {
        format!("{servidor} · {}", c.kind.nome())
    }
}

/// A página Arquivos.
pub(super) fn pagina(tela: &Rc<Tela>, ctx: &Rc<Contexto>, paineis: &PaineisDaPagina) -> Column {
    let conexoes = {
        let t = Rc::clone(tela);
        Rc::new(move || {
            t.tree.with(|arvore| {
                arvore
                    .connections()
                    .into_iter()
                    .filter(|c| c.kind.tem_arquivos())
                    .map(|c| ConexaoDeArquivos {
                        id: c.id,
                        nome: c.name.clone(),
                        detalhe: detalhe_do_cartao(c),
                        icone: icone_do_cartao(c.kind),
                    })
                    .collect()
            })
        })
    };
    let recentes = {
        let t = Rc::clone(tela);
        Rc::new(move || {
            let lista = t.recentes.with(|r| r.lista().to_vec());
            lista
                .into_iter()
                .filter(|r| match &r.alvo {
                    AlvoRecente::Local { conexao, .. } => {
                        t.tree.with(|arvore| arvore.connection(*conexao).is_some())
                    }
                    _ => false,
                })
                .map(|r| (r.alvo, r.titulo, r.detalhe))
                .collect()
        })
    };
    let nova = {
        let t = Rc::clone(tela);
        Rc::new(move |id: u64| t.abrir_arquivos(id))
    };
    let abrir_recente = {
        let t = Rc::clone(tela);
        Rc::new(move |alvo: &AlvoRecente| t.abrir_recente(alvo))
    };
    let fechar = {
        let t = Rc::clone(tela);
        Rc::new(move |chave: String| t.pedir_fechar_aba(&chave))
    };
    let pergunta = {
        let (t_nome, t_enviar, t_sempre, t_fechar) = (
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
        );
        let aberta = tela.edicao_pergunta;
        Row::new()
            .gap(4)
            .padding(Insets::new(8.0, 8.0, 8.0, 0.0))
            .visible(move || aberta.with(|p| !p.is_empty()))
            .child(
                InfoBar::new(InfoBarSeverity::Informational)
                    .id("edicao-aviso")
                    .grow(1)
                    .message(move || t_nome.nome_da_pergunta())
                    .on_close(move || t_fechar.responder_edicao(false, false)),
            )
            .child(
                Button::new(t!("arquivos-enviar"))
                    .id("edicao-enviar")
                    .on_click(move || t_enviar.responder_edicao(true, false)),
            )
            .child(
                Button::new(t!("arquivos-sempre-enviar-este-arquivo"))
                    .id("edicao-sempre")
                    .on_click(move || t_sempre.responder_edicao(true, true)),
            )
    };
    let nova_conexao = {
        let t = Rc::clone(tela);
        Rc::new(move || {
            t.nova_conexao();
            t.ir_para("terminais");
        })
    };
    let remotos = {
        let t = Rc::clone(tela);
        Rc::new(move || t.abrir_remotos_do_rclone())
    };
    let aberta = tela.rclone.aberta;
    Column::new()
        .grow(1)
        .child(
            Column::new()
                .grow(1)
                .visible(move || !aberta.get())
                .child(pergunta)
                .child(rclone::aviso_de_instalacao(
                    tela,
                    "arquivos-rclone-instalar",
                ))
                .child(vistas::pagina(Entrada {
                    abas: tela.abas_arquivos,
                    escolhida: tela.aba_arquivos,
                    ctx: Rc::clone(ctx),
                    conexoes,
                    recentes,
                    nova,
                    abrir_recente,
                    fechar,
                    nova_conexao,
                    remotos,
                    paineis: paineis.clone(),
                })),
        )
        .child(rclone::subpagina(tela).visible(move || aberta.get()))
}

/// Os diálogos da página, que moram ao lado da navegação: o de conflito e o de fechar a aba.
pub(super) fn dialogos(tela: &Rc<Tela>, ctx: &Rc<Contexto>) -> impl View + use<> {
    let t = Rc::clone(tela);
    Column::new()
        .child(fila::dialogo_de_conflito(ctx))
        .child(vistas::dialogo_de_fechar(
            tela.fechando_aberto,
            tela.fechando_quantas,
            move || {
                let chave = t.fechando_aba.borrow_mut().take();
                if let Some(chave) = chave {
                    t.fechar_aba(&chave);
                }
            },
        ))
}
