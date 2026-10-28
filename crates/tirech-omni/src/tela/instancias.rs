//! O canal entre as cópias do app abertas na mesma pasta de dados ([`crate::instancias`]), na
//! tela. A principal atende os pedidos das secundárias entre um quadro e outro: confere, grava se
//! o arquivo ainda é o que a secundária leu e relê o documento. A secundária relê o que a
//! principal gravou diferente do que ela tinha e avisa os conflitos e as falhas.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use tomni_ui::files::Live;
use tomni_ui::prelude::*;
use tomni_ui::terminal::Proxy;

use super::Tela;
use crate::assistente::perfis::ArquivoIa;
use crate::configuracoes::Configuracoes;
use crate::instancias::{
    self, ChamadoDeRede, Documento, Emprestimo, Evento, Eventos, Fila, FilaDeRedes, Pedido,
    PedidoDaSecundaria, Resposta, ServidorDasInstancias,
};
use crate::redes::{PerfilRede, ProxyRede, RedeResolvida, TipoProxy};
use crate::t;

pub(super) struct Estado {
    /// O canal, na principal.
    servidor: RefCell<Option<ServidorDasInstancias>>,
    /// Os pedidos das secundárias, à espera da interface.
    fila: Fila,
    /// O que voltou da principal, na secundária.
    eventos: Eventos,
    /// Os pedidos de rede das secundárias, na principal.
    redes: FilaDeRedes,
    /// As redes emprestadas às secundárias agora, pelo número do empréstimo, na principal: contam
    /// como em uso, e o motor não para enquanto a secundária usa.
    pub(super) emprestadas: Signal<BTreeMap<u64, PerfilRede>>,
    /// As redes que a principal empresta a esta cópia, pelo perfil, na secundária (o último
    /// empréstimo de cada uma; as abas seguram os delas).
    emprestimos: RefCell<HashMap<u64, Arc<Emprestimo>>>,
    /// Os perfis que esta secundária liga ela mesma, porque não havia principal quando pediu.
    locais: RefCell<HashSet<u64>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            servidor: RefCell::new(None),
            fila: Live::new(Vec::new()),
            eventos: Live::new(Vec::new()),
            redes: Live::new(Vec::new()),
            emprestadas: signal(BTreeMap::new()),
            emprestimos: RefCell::default(),
            locais: RefCell::default(),
        }
    }
}

impl Tela {
    /// Sobe o canal (na principal) ou passa a ouvir o que volta dele (na secundária).
    pub(super) fn ligar_instancias(self: &Rc<Self>) {
        if self.so_leitura {
            self.seguir_a_principal();
        } else {
            self.atender_as_secundarias();
        }
    }

    fn atender_as_secundarias(self: &Rc<Self>) {
        let endereco = instancias::endereco_da_pasta(self.armazenamento.pasta());
        match ServidorDasInstancias::iniciar(
            &endereco,
            self.instancias.fila.clone(),
            self.instancias.redes.clone(),
        ) {
            Ok(servidor) => *self.instancias.servidor.borrow_mut() = Some(servidor),
            Err(erro) => self.avisar(
                InfoBarSeverity::Warning,
                t!("instancias-sem-canal", erro = erro),
            ),
        }
        let (tela, fila) = (Rc::clone(self), self.instancias.fila.clone());
        effect(move || {
            // Só rastreia a versão: `tirar` pega o mutex de novo, e dentro do `with` travaria.
            fila.with(|_| ());
            let pedidos = instancias::tirar(&fila);
            untrack(|| {
                for pedido in pedidos {
                    tela.atender_a_secundaria(pedido);
                }
            });
        });
        let (tela, redes) = (Rc::clone(self), self.instancias.redes.clone());
        effect(move || {
            redes.with(|_| ());
            let chamados = instancias::tirar(&redes);
            untrack(|| {
                for chamado in chamados {
                    match chamado {
                        ChamadoDeRede::Pedir {
                            emprestimo,
                            rede,
                            responder,
                        } => {
                            let _ = responder.send(tela.emprestar_a_secundaria(emprestimo, rede));
                        }
                        ChamadoDeRede::Soltar(emprestimo) => {
                            if tela
                                .instancias
                                .emprestadas
                                .with_untracked(|m| m.contains_key(&emprestimo))
                            {
                                tela.instancias.emprestadas.update(|m| {
                                    m.remove(&emprestimo);
                                });
                            }
                        }
                    }
                }
            });
        });
    }

    /// Liga (ou acha ligado) o motor do perfil `id` para uma secundária e conta o uso até ela
    /// soltar. O motor esperar conectar fica com a thread da ligação.
    fn emprestar_a_secundaria(&self, emprestimo: u64, id: u64) -> Result<RedeResolvida, String> {
        let perfil = self
            .tree
            .with_untracked(|t| t.perfil(id).cloned())
            .filter(PerfilRede::e_embutida)
            .ok_or_else(|| t!("instancias-rede-sumiu"))?;
        let rede = self.resolver_rede(perfil.clone());
        if let Some(erro) = rede.erro_do_motor() {
            return Err(erro.to_owned());
        }
        self.instancias.emprestadas.update(|m| {
            m.insert(emprestimo, perfil);
        });
        Ok(rede)
    }

    /// Na secundária, pede à principal a VPN embutida da `rede` e sai pela ponte dela. `false`
    /// se a rede é desta cópia: não há principal atendendo (ou não havia, quando ela ligou o
    /// motor aqui), e a tela liga o motor como sempre.
    pub(super) fn emprestar_da_principal(&self, rede: &mut RedeResolvida) -> bool {
        let id = rede.perfil.id;
        if !self.so_leitura
            || !rede.perfil.e_embutida()
            || self.instancias.locais.borrow().contains(&id)
        {
            return false;
        }
        let endereco = instancias::endereco_da_pasta(self.armazenamento.pasta());
        match Emprestimo::pedir(&endereco, id) {
            Err(_) => {
                self.instancias.locais.borrow_mut().insert(id);
                false
            }
            Ok(Ok(emprestimo)) => {
                if let Proxy::Socks5 {
                    host,
                    port,
                    user,
                    password,
                } = emprestimo.proxy.clone()
                {
                    rede.perfil.proxy = Some(ProxyRede {
                        tipo: TipoProxy::Socks5,
                        host,
                        port,
                        credencial: None,
                    });
                    rede.proxy_usuario = user;
                    rede.proxy_senha = password;
                }
                self.instancias
                    .emprestimos
                    .borrow_mut()
                    .insert(id, Arc::clone(&emprestimo));
                rede.emprestimo = Some(emprestimo);
                true
            }
            Ok(Err(erro)) => {
                // Como um motor que não subiu: a conexão nunca sai direto.
                rede.perfil.proxy = Some(ProxyRede {
                    port: 0,
                    ..ProxyRede::novo()
                });
                rede.proxy_usuario = None;
                rede.proxy_senha = None;
                rede.motor_erro = Some(erro);
                true
            }
        }
    }

    /// O perfil `id` ficou sem uso nesta cópia: solta o empréstimo dele (a principal pode parar o
    /// motor) e, da próxima vez, pede à principal de novo.
    pub(super) fn soltar_rede_emprestada(&self, id: u64) {
        self.instancias.emprestimos.borrow_mut().remove(&id);
        self.instancias.locais.borrow_mut().remove(&id);
    }

    fn atender_a_secundaria(&self, pedido: PedidoDaSecundaria) {
        let resposta = self.gravar_da_secundaria(&pedido.pedido);
        pedido.responder(resposta);
    }

    /// Confere e grava o pedido de uma secundária; gravado, relê o documento.
    fn gravar_da_secundaria(&self, pedido: &Pedido) -> Resposta {
        let recusado = |erro| Resposta::Recusado { erro };
        let Some(documento) = Documento::do_nome(&pedido.documento) else {
            return recusado(t!(
                "instancias-documento-desconhecido",
                documento = pedido.documento.clone()
            ));
        };
        let pasta = if documento.na_pasta_do_cofre() {
            self.cofre.agora(|c| c.pasta_do_cofre().to_owned())
        } else {
            self.armazenamento.pasta().to_owned()
        };
        if !instancias::mesma_pasta(&pasta, &pedido.pasta) {
            return recusado(t!(
                "instancias-outra-pasta",
                pasta = pedido.pasta.display().to_string()
            ));
        }
        let conteudo = pedido.conteudo.as_bytes();
        let valido = match documento {
            Documento::Cofre => self.cofre.agora(|c| c.aceita_de_outra_copia(conteudo)),
            _ => instancias::validar(documento, conteudo),
        };
        if let Err(erro) = valido {
            return recusado(t!("instancias-conteudo-invalido", erro = erro));
        }
        let resposta = instancias::gravar_se_ainda_for(
            &pasta.join(documento.nome()),
            &pedido.esperado,
            conteudo,
            documento == Documento::Cofre,
        );
        if resposta == Resposta::Gravado {
            self.reler_documento(documento);
        }
        resposta
    }

    /// Relê da pasta o documento que a outra cópia gravou.
    fn reler_documento(&self, documento: Documento) {
        match documento {
            Documento::Conexoes => {
                // A vigia relê só o que mudou desde a última leitura: esquecida, ela relê já.
                *self.visto_conexoes.borrow_mut() = crate::armazenamento::Visto::default();
                self.conferir_conexoes();
            }
            Documento::Configuracoes => {
                let (config, avisos) = Configuracoes::carregar(&self.armazenamento);
                if self.config.with_untracked(|c| *c != config) {
                    self.config.set(config);
                }
                if !avisos.is_empty() {
                    self.avisar(InfoBarSeverity::Warning, avisos.join(" "));
                }
            }
            Documento::Ia => {
                let (ia, aviso) = ArquivoIa::carregar(&self.armazenamento);
                if self.assistente.perfis.with_untracked(|a| *a != ia) {
                    self.assistente.perfis.set(ia);
                }
                if let Some(aviso) = aviso {
                    self.avisar(InfoBarSeverity::Warning, aviso);
                }
            }
            Documento::Cofre => self.conferir_cofre(),
        }
    }

    fn seguir_a_principal(self: &Rc<Self>) {
        let dados = self.armazenamento.pasta().to_owned();
        instancias::seguir_tambem(&dados, &self.cofre.agora(|c| c.pasta_do_cofre().to_owned()));
        instancias::ouvir(&dados, self.instancias.eventos.clone());
        let (tela, eventos) = (Rc::clone(self), self.instancias.eventos.clone());
        effect(move || {
            eventos.with(|_| ());
            let lista = instancias::tirar(&eventos);
            untrack(|| {
                for evento in lista {
                    tela.receber_da_principal(evento);
                }
            });
        });
    }

    fn receber_da_principal(&self, evento: Evento) {
        match evento {
            Evento::Gravado { documento, adotar } => self.adotar(documento, adotar),
            Evento::Conflitos(caminhos) => self.avisar(
                InfoBarSeverity::Warning,
                t!("instancias-conflito", caminhos = caminhos.join(", ")),
            ),
            Evento::Falhou(erro) => self.avisar(
                InfoBarSeverity::Warning,
                t!("instancias-falhou", erro = erro),
            ),
            Evento::SemPrincipal(texto) => self.avisar(InfoBarSeverity::Warning, texto),
        }
    }

    /// A principal gravou `documento`. Com `adotar`, o que ficou no disco não é o que esta cópia
    /// tem: relê, se não há outra mudança dela a caminho.
    fn adotar(&self, documento: Documento, adotar: bool) {
        match documento {
            // A vigia das conexões já relê o que mudou no disco.
            Documento::Conexoes => self.conferir_conexoes(),
            Documento::Cofre => self.conferir_cofre(),
            Documento::Configuracoes | Documento::Ia => {
                let arquivo = self.armazenamento.pasta().join(documento.nome());
                if adotar && !instancias::pendente(&arquivo) {
                    self.reler_documento(documento);
                }
            }
        }
    }
}
