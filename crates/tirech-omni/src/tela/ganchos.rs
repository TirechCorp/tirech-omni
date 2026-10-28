//! Onde as configurações chegam ao resto do app: um efeito grava o `configuracoes.json` a cada
//! mudança e os outros aplicam o que vale na hora (o tema, o limite de recentes). O que cada
//! categoria muda fica junto de quem usa; aqui ficam os efeitos que não têm outro dono.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;
use tomni_ui::theme::Theme;
use tomni_ui::{LauncherEntry, LauncherState};

use super::Tela;
use crate::configuracoes::{ModoReconectar, Tema};
use crate::dock;
use crate::espacos::id_da_chave;
use crate::t;

impl Tela {
    /// Liga os efeitos das configurações. Chamado uma vez, ao montar a tela.
    pub(super) fn ligar_configuracoes(self: &Rc<Self>) {
        self.gravar_ao_mudar();
        self.aplicar_tema();
        self.aplicar_recentes();
        self.aplicar_no_abridor();
        self.aplicar_transferencias();
        self.aplicar_reconexao();
        self.aplicar_fechar_painel_vazio();
        self.aplicar_mostrar_aba_na_arvore();
        self.aplicar_agente();
        self.aplicar_sistema();
        self.aplicar_inicio_com_sistema();
        self.aplicar_atalho_no_menu();
        self.aplicar_dock();
        self.aplicar_cofre_no_agente();
        self.repontar_instalacao();
    }

    /// Ao abrir (M65): o atalho da área de trabalho e o fixado da barra que apontam para um
    /// executável que sumiu passam a apontar para o app de agora (ou para a cópia instalada). O
    /// que a pessoa apagou não volta. Numa thread, porque lê e grava arquivos; sem resultado na
    /// tela, é um conserto silencioso. Com a pasta de dados trocada por variável, não mexe em nada.
    fn repontar_instalacao(&self) {
        let ambiente = self.ferramentas.ambiente();
        if !ambiente.repontar_ao_abrir {
            return;
        }
        let ctx = crate::instalacao::Contexto::de(ambiente);
        std::thread::spawn(move || {
            let _ = ctx.repontar_o_que_sumiu();
        });
    }

    /// `geral.atalho_no_menu`: grava ou apaga o `.desktop` e os ícones do menu do Linux. Na
    /// abertura só grava o que falta ou mudou (o app pode ter mudado de lugar); desligar depois
    /// de ligado apaga.
    fn aplicar_atalho_no_menu(&self) {
        let config = self.config;
        let ferramentas = Rc::clone(&self.ferramentas);
        let anterior: Cell<Option<bool>> = Cell::new(None);
        effect(move || {
            let ligado = config.with(|c| c.geral.atalho_no_menu);
            untrack(|| {
                let antes = anterior.replace(Some(ligado));
                ferramentas.aplicar_atalho_no_menu(ligado, antes.is_some());
            });
        });
    }

    /// O progresso e o número no ícone do dock: a soma de todas as sessões de todas as janelas.
    /// Só vale com o atalho no menu ligado, porque o dock casa o ícone pelo `.desktop`.
    fn aplicar_dock(&self) {
        let (config, janelas) = (self.config, self.janelas.clone());
        let entrada = LauncherEntry::new(dock::DESKTOP_ID);
        effect(move || {
            let ligado = config.with(|c| c.geral.atalho_no_menu);
            let sessoes: Vec<Session> = janelas
                .todas()
                .iter()
                .flat_map(|j| j.layout.with(|l| l.sessions()))
                .collect();
            let estado = if ligado {
                dock::estado(&sessoes)
            } else {
                LauncherState::default()
            };
            untrack(|| entrada.update(estado));
        });
    }

    /// `geral.iniciar_com_sistema`: grava ou apaga a entrada que abre o app com o sistema. Na
    /// abertura só regrava a entrada que ficou apontando para outro executável.
    fn aplicar_inicio_com_sistema(&self) {
        let config = self.config;
        let ferramentas = Rc::clone(&self.ferramentas);
        let anterior: Cell<Option<bool>> = Cell::new(None);
        effect(move || {
            let ligado = config.with(|c| c.geral.iniciar_com_sistema);
            untrack(|| {
                let antes = anterior.replace(Some(ligado));
                ferramentas.aplicar_inicio_com_sistema(ligado, antes.is_some());
            });
        });
    }

    /// `terminal.fechar_painel_vazio`: o painel que perde a última aba some sozinho. Vale em
    /// todas as janelas, e o efeito refaz quando uma janela abre.
    fn aplicar_fechar_painel_vazio(&self) {
        let (config, janelas) = (self.config, self.janelas.clone());
        effect(move || {
            let ligado = config.with(|c| c.terminal.fechar_painel_vazio);
            for janela in janelas.todas() {
                // O signal das configurações avisa a cada mudança: só mexe no que difere.
                if janela.layout.with_untracked(|l| l.closes_emptied()) != ligado {
                    janela.layout.update(|l| l.close_emptied(ligado));
                }
            }
        });
    }

    /// `conexoes.mostrar_aba_na_arvore` (menu Exibir): ao escolher uma aba, em qualquer janela
    /// (o clique na aba, a troca de painel ativo), a conexão dela fica escolhida na árvore, com
    /// os grupos abertos e a linha à vista, como o "sincronizar a árvore" do WinSSHTerm. Ligar a
    /// opção já mostra a aba ativa da principal. Só a troca de aba conta: escolher outra linha
    /// na árvore depois não volta atrás.
    fn aplicar_mostrar_aba_na_arvore(&self) {
        let (config, janelas) = (self.config, self.janelas.clone());
        let (tree, abertos, selecionado) = (self.tree, self.abertos, self.selecionado);
        // A aba ativa de cada janela na última volta, e se a opção estava ligada.
        let anteriores: RefCell<HashMap<WindowHandle, Option<Session>>> = RefCell::default();
        let estava_ligado = Cell::new(None::<bool>);
        effect(move || {
            let ligado = config.with(|c| c.conexoes.mostrar_aba_na_arvore);
            let ativas: Vec<(WindowHandle, Option<Session>)> = janelas
                .todas()
                .iter()
                .map(|j| {
                    let aba = j.layout.with(|l| l.pane(l.active()).selected);
                    (j.handle, aba)
                })
                .collect();
            let mut antes = anteriores.borrow_mut();
            let primeira_volta = estava_ligado.get().is_none();
            let acabou_de_ligar = estava_ligado.replace(Some(ligado)) == Some(false) && ligado;
            let mut escolhida = None;
            if acabou_de_ligar {
                escolhida = ativas.first().and_then(|(_, aba)| *aba);
            } else if ligado && !primeira_volta {
                for (janela, aba) in &ativas {
                    if aba.is_some() && antes.get(janela) != Some(aba) {
                        escolhida = *aba;
                    }
                }
            }
            *antes = ativas.into_iter().collect();
            drop(antes);
            if let Some(aba) = escolhida
                && let Some(id) = id_da_chave(&aba.key_str())
            {
                untrack(|| super::escolher_na_arvore(tree, abertos, selecionado, id));
            }
        });
    }

    /// `conexoes.reconectar`: o que fazer quando uma conexão cai (o `close_reason` da sessão;
    /// um shell que acabou com `exit` não tem motivo e fica de fora). A aba que um assistente
    /// está usando reconecta sozinha em qualquer modo: o assistente não tem como responder ao
    /// aviso, e sem a aba o trabalho dele para. Acabadas as tentativas, a aba espera o tempo da
    /// linha "Tentar de novo após" da conexão (ou da pasta, ou `retentar_mins`) e começa outra
    /// rodada, até voltar; desligado, para.
    fn aplicar_reconexao(&self) {
        let (config, janelas, aviso, alarmes, reconectando, eu) = (
            self.config,
            self.janelas.clone(),
            self.aviso,
            self.alarmes.clone(),
            Rc::clone(&self.reconectando),
            self.eu.clone(),
        );
        // As sessões que já foram tratadas nesta queda, e as tentativas seguidas de cada uma.
        let tratadas: Rc<RefCell<HashSet<Session>>> = Rc::default();
        let tentativas: Rc<RefCell<HashMap<Session, (u32, Instant)>>> = Rc::default();
        // As sessões que esperam a rodada nova. Reabrir a aba nesse meio-tempo (à mão) tira
        // daqui, e o alarme da pausa não faz nada.
        let em_pausa: Rc<RefCell<HashSet<Session>>> = Rc::default();
        effect(move || {
            let cfg = config.with(|c| c.conexoes.reconectar.clone());
            let sessoes: Vec<Session> = janelas
                .todas()
                .iter()
                .flat_map(|j| j.layout.with(|l| l.sessions()))
                .collect();
            for sessao in sessoes {
                let Some(motivo) = sessao.close_reason() else {
                    // Reaberta (à mão ou pelo alarme): a próxima queda volta a ser tratada.
                    if !reconectando.borrow().contains_key(&sessao) {
                        tratadas.borrow_mut().remove(&sessao);
                    }
                    em_pausa.borrow_mut().remove(&sessao);
                    continue;
                };
                if !tratadas.borrow_mut().insert(sessao) {
                    continue;
                }
                untrack(|| {
                    let do_assistente = eu
                        .upgrade()
                        .is_some_and(|tela| tela.aba_com_assistente(&sessao.key_str()));
                    let modo = if do_assistente {
                        ModoReconectar::Sozinho
                    } else {
                        cfg.modo
                    };
                    match modo {
                        ModoReconectar::Nada => {}
                        ModoReconectar::Oferecer => aviso.set(Some((
                            InfoBarSeverity::Warning,
                            t!("gancho-a-conexao-caiu", motivo = motivo.as_str()),
                        ))),
                        ModoReconectar::Sozinho => {
                            let espera = Duration::from_secs(u64::from(cfg.espera_secs));
                            let n = {
                                let mut todas = tentativas.borrow_mut();
                                let (n, ultima) =
                                    todas.entry(sessao).or_insert((0, Instant::now()));
                                if ultima.elapsed() > (espera * 10).max(Duration::from_secs(60)) {
                                    *n = 0;
                                }
                                *n += 1;
                                *ultima = Instant::now();
                                *n
                            };
                            if n > cfg.tentativas {
                                tentativas.borrow_mut().remove(&sessao);
                                let pausa = eu.upgrade().and_then(|tela| {
                                    tela.pausa_da_reconexao(sessao, cfg.retentar_mins)
                                });
                                let Some(pausa) = pausa else {
                                    sessao.feed(
                                        format!(
                                            "\r\n[{}]\r\n",
                                            t!("gancho-reconexao-falhou", motivo = motivo.as_str())
                                        )
                                        .as_bytes(),
                                    );
                                    return;
                                };
                                let minutos = u32::try_from(pausa.as_secs() / 60).unwrap_or(0);
                                sessao.feed(
                                    format!(
                                        "\r\n[{}]\r\n",
                                        t!(
                                            "gancho-reconexao-pausa",
                                            motivo = motivo.as_str(),
                                            tempo = crate::retentar::tempo(minutos)
                                        )
                                    )
                                    .as_bytes(),
                                );
                                em_pausa.borrow_mut().insert(sessao);
                                let (tratadas, em_pausa, janelas) =
                                    (Rc::clone(&tratadas), Rc::clone(&em_pausa), janelas.clone());
                                alarmes.depois(pausa, move || {
                                    // Reaberta à mão, ou a aba fechou: nada a fazer.
                                    let aberta = janelas.todas().iter().any(|j| {
                                        j.layout.with_untracked(|l| l.sessions().contains(&sessao))
                                    });
                                    if !em_pausa.borrow_mut().remove(&sessao) || !aberta {
                                        return;
                                    }
                                    tratadas.borrow_mut().remove(&sessao);
                                    if sessao.closed() {
                                        sessao.restart();
                                    }
                                });
                                return;
                            }
                            sessao.feed(
                                format!(
                                    "\r\n[{}]\r\n",
                                    t!(
                                        "gancho-reconexao-tentando",
                                        motivo = motivo.as_str(),
                                        n = n,
                                        max = cfg.tentativas,
                                        espera = cfg.espera_secs
                                    )
                                )
                                .as_bytes(),
                            );
                            reconectando
                                .borrow_mut()
                                .insert(sessao, (n, cfg.tentativas));
                            let tratadas = Rc::clone(&tratadas);
                            let reconectando = Rc::clone(&reconectando);
                            alarmes.depois(espera, move || {
                                reconectando.borrow_mut().remove(&sessao);
                                tratadas.borrow_mut().remove(&sessao);
                                // Quem reconectou à mão nesse meio-tempo já resolveu.
                                if sessao.closed() {
                                    sessao.restart();
                                }
                            });
                        }
                    }
                });
            }
        });
    }

    /// `agente.ligado`, `socket` e `vida_padrao_secs`: ligar, desligar ou trocar o socket do
    /// agente embutido e a validade das chaves novas. A primeira rodada só anota o estado; quem
    /// sobe o agente na abertura é `Ferramentas::iniciar`.
    fn aplicar_agente(&self) {
        let config = self.config;
        let ferramentas = Rc::clone(&self.ferramentas);
        let anterior: RefCell<Option<(String, u64)>> = RefCell::new(None);
        effect(move || {
            let (ligado, socket, vida) = config.with(|c| {
                (
                    c.agente.ligado,
                    c.agente.socket.trim().to_owned(),
                    c.agente.vida_padrao_secs,
                )
            });
            untrack(|| {
                let antes = anterior.replace(Some((socket.clone(), vida)));
                let Some((socket_antes, vida_antes)) = antes else {
                    return;
                };
                if ligado != ferramentas.agente.rodando() {
                    if ligado {
                        ferramentas.iniciar();
                    } else {
                        ferramentas.desligar_agente();
                    }
                } else if ligado && socket != socket_antes {
                    ferramentas.iniciar();
                }
                if vida != vida_antes {
                    ferramentas.agente.vida_padrao(vida);
                }
            });
        });
    }

    /// `agente.sistema_*` e `agente.pageant`: o que o app escreve para o resto do sistema usar o
    /// agente (o bloco do `~/.ssh/config`, os shells, a sessão gráfica, o Git) e o Pageant.
    /// A primeira rodada só anota o estado: quem aplica na abertura é `Ferramentas::iniciar`.
    fn aplicar_sistema(&self) {
        let config = self.config;
        let ferramentas = Rc::clone(&self.ferramentas);
        let anterior: RefCell<Option<bool>> = RefCell::new(None);
        effect(move || {
            let (_, _, _, _, _, pageant) = config.with(|c| {
                (
                    c.agente.sistema_ssh_config,
                    c.agente.sistema_shells,
                    c.agente.sistema_sessao,
                    c.agente.sistema_git,
                    c.agente.socket.clone(),
                    c.agente.pageant,
                )
            });
            untrack(|| {
                let antes = anterior.replace(Some(pageant));
                if antes.is_none() {
                    return;
                }
                ferramentas.aplicar_sistema();
                if antes != Some(pageant) && ferramentas.ambiente().windows {
                    ferramentas.agente.definir_pageant(pageant);
                }
            });
        });
    }

    /// As chaves do cofre marcadas "No agente": entram no agente ao destravar o cofre, saem ao
    /// bloqueá-lo (com `agente.tirar_ao_bloquear`) e acompanham as mudanças dos itens.
    fn aplicar_cofre_no_agente(&self) {
        let (config, cofre) = (self.config, self.cofre.clone());
        let ferramentas = Rc::clone(&self.ferramentas);
        effect(move || {
            let _ = cofre.versao();
            let _ = config.with(|c| (c.agente.tirar_ao_bloquear, c.conexoes.known_hosts.clone()));
            untrack(|| ferramentas.sincronizar_cofre());
        });
    }

    /// `arquivos.paralelas`, `preservar_data` e `preservar_permissoes`: valem para as
    /// transferências que ainda não começaram.
    fn aplicar_transferencias(&self) {
        let config = self.config;
        let fila = self.transferencias.clone();
        effect(move || {
            let (paralelas, data, permissoes) = config.with(|c| {
                (
                    c.arquivos.paralelas as usize,
                    c.arquivos.preservar_data,
                    c.arquivos.preservar_permissoes,
                )
            });
            untrack(|| {
                fila.set_parallel(paralelas);
                fila.set_preserve_date(data);
                fila.set_preserve_permissions(permissoes);
            });
        });
    }

    /// Entrega as configurações ao abridor (shell local, `known_hosts`, logs, chave nova): uma
    /// vez já e a cada mudança. Vale da próxima sessão em diante.
    fn aplicar_no_abridor(&self) {
        let config = self.config;
        let opener = Rc::clone(&self.opener);
        let pasta = self.armazenamento.pasta().to_path_buf();
        effect(move || {
            let atual = config.get();
            untrack(|| opener.configurar(&atual, &pasta));
        });
    }

    /// Grava o `configuracoes.json` a cada mudança do signal, menos na primeira execução (o
    /// que acabou de ser lido não precisa voltar ao disco). Se falhar, avisa na barra da casca.
    fn gravar_ao_mudar(&self) {
        let config = self.config;
        let armazenamento = self.armazenamento.clone();
        let aviso = self.aviso;
        let primeira = Cell::new(true);
        effect(move || {
            let atual = config.get();
            if primeira.replace(false) {
                return;
            }
            untrack(|| {
                if let Err(erro) = atual.salvar(&armazenamento) {
                    aviso.set(Some((
                        InfoBarSeverity::Error,
                        t!(
                            "comum-nao-consegui-gravar",
                            arquivo = armazenamento.arquivo_configuracoes().display().to_string(),
                            erro = erro.to_string()
                        ),
                    )));
                }
            });
        });
    }

    /// `geral.tema`: fixa o tema de todas as janelas. "Sistema" na primeira execução não faz
    /// nada, para não sobrepor o tema que o `App` pediu.
    fn aplicar_tema(&self) {
        let tema = memo({
            let config = self.config;
            move || config.with(|c| c.geral.tema)
        });
        let primeira = Cell::new(true);
        effect(move || {
            let tema = tema.get();
            if primeira.replace(false) && tema == Tema::Sistema {
                return;
            }
            untrack(|| {
                tomni_ui::set_theme(match tema {
                    Tema::Sistema => None,
                    Tema::Claro => Some(Theme::Light),
                    Tema::Escuro => Some(Theme::Dark),
                });
            });
        });
    }

    /// `geral.recentes_max`: o limite da lista de recentes.
    fn aplicar_recentes(&self) {
        let config = self.config;
        let recentes = self.recentes;
        effect(move || {
            let limite = config.with(|c| c.limite_de_recentes());
            untrack(|| recentes.update(|r| r.definir_limite(limite)));
        });
    }
}
