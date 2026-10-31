//! A IA na casca (M52): os provedores (`ia.json`), a chave no cofre, o endpoint de cada pedido e o
//! diálogo "Escolher um provedor de IA". A página dos provedores fica em `tela/ia_perfis.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use tomni_ui::ai::{Endpoint, Locality, Zeroizing};
use tomni_ui::prelude::*;

use super::{Tela, poe};
use crate::assistente::perfis::{ArquivoIa, PerfilIa, Preset};
use crate::cofre::Tipo;
use crate::redes::{PerfilRede, Rede, RedeResolvida};
use crate::t;

/// O que a tela guarda do assistente.
pub(super) struct EstadoAssistente {
    /// O conteúdo do `ia.json`.
    pub perfis: Signal<ArquivoIa>,
    /// O provedor escolhido na página (o id, em texto, como o `TreeView` pede).
    pub escolhido: Signal<Option<String>>,
    /// Se o diálogo "Escolher um provedor de IA" está aberto.
    pub escolher_aberto: Signal<bool>,
    /// O envio ao modelo: contexto, confirmações e pedidos em andamento.
    pub motor: super::ia_envio::Motor,
    /// O Flyout "Gerar comando com IA".
    pub comando: super::ia_comando::EstadoComando,
    /// O chat ao lado do terminal.
    pub chat: super::ia_chat::EstadoChat,
}

impl EstadoAssistente {
    pub(super) fn novo(arquivo: ArquivoIa) -> Self {
        Self {
            perfis: signal(arquivo),
            escolhido: signal(None),
            escolher_aberto: signal(false),
            motor: super::ia_envio::Motor::novo(),
            comando: super::ia_comando::EstadoComando::novo(),
            chat: super::ia_chat::EstadoChat::novo(),
        }
    }
}

/// O título do item do cofre que guarda a chave do provedor `nome`.
pub(super) fn titulo_da_chave(nome: &str) -> String {
    t!("ia-chave-titulo", nome = nome)
}

impl Tela {
    /// Faz o `ia.json` acompanhar os provedores: cada mudança é gravada, e uma falha avisa. A
    /// linha "Assistente de IA" da conexão acompanha os nomes.
    pub(super) fn ligar_assistente(self: &Rc<Self>) {
        self.ligar_motor();
        self.ligar_comando();
        self.ligar_chat();
        let (t, perfis) = (Rc::clone(self), self.assistente.perfis);
        effect(move || {
            let _ = perfis.get();
            untrack(|| t.sincronizar_perfil_ia());
        });
        let (tela, perfis) = (Rc::clone(self), self.assistente.perfis);
        let gravado = RefCell::new(perfis.get_untracked());
        effect(move || {
            let atual = perfis.get();
            untrack(|| {
                if *gravado.borrow() == atual {
                    return;
                }
                if let Err(erro) = atual.salvar(&tela.armazenamento) {
                    tela.avisar(
                        InfoBarSeverity::Error,
                        t!(
                            "comum-nao-consegui-gravar",
                            arquivo = tela.armazenamento.arquivo_ia().display().to_string(),
                            erro = erro.to_string()
                        ),
                    );
                }
                *gravado.borrow_mut() = atual;
            });
        });
    }

    /// O provedor de id `id`.
    pub(super) fn perfil_de_ia(&self, id: u64) -> Option<PerfilIa> {
        self.assistente
            .perfis
            .with_untracked(|a| a.perfil(id).cloned())
    }

    /// A chave do provedor, lida do cofre (`None`: sem chave, ou o cofre está bloqueado).
    pub(super) fn chave_do_perfil(&self, perfil: &PerfilIa) -> Option<Zeroizing<String>> {
        let id = perfil.credencial?;
        self.cofre
            .agora(|c| c.item(id).map(|i| Zeroizing::new(i.senha.clone())))
            .filter(|chave| !chave.is_empty())
    }

    /// O perfil de rede por onde o provedor sai (`None`: direto).
    pub(super) fn rede_do_perfil_de_ia(&self, perfil: &PerfilIa) -> Option<PerfilRede> {
        match perfil.rede_efetiva() {
            Rede::Perfil(id) => self.tree.with_untracked(|t| t.perfil(id).cloned()),
            _ => None,
        }
    }

    /// O que a lib recebe para falar com o provedor: a chave do cofre e o proxy da rede. Roda na
    /// thread da interface, antes de abrir a thread do pedido. Devolve também o perfil de rede, que
    /// a thread prepara (sobe o túnel) antes do pedido.
    pub(super) fn endpoint_do_perfil(&self, perfil: &PerfilIa) -> (Endpoint, Option<PerfilRede>) {
        let rede = self.rede_do_perfil_de_ia(perfil);
        let proxy = rede
            .clone()
            .and_then(|r| RedeResolvida::proxy(&self.resolver_rede(r)));
        let mut ponto = perfil.endpoint(self.chave_do_perfil(perfil), proxy);
        let espera = self.config.with_untracked(|c| c.ia.espera_resposta);
        ponto.response_timeout =
            (espera > 0).then(|| std::time::Duration::from_secs(u64::from(espera)));
        (ponto, rede)
    }

    /// Guarda `chave` no cofre como o item de chave do provedor `id` e liga o provedor a ele. Se o
    /// provedor já usa um item "Chave de API — {nome}", troca a senha dele; um item que a pessoa
    /// escolheu à mão não é tocado: a chave vai para um item novo.
    pub(super) fn guardar_chave_do_perfil(&self, id: u64, chave: String) {
        let Some(perfil) = self.perfil_de_ia(id) else {
            return;
        };
        let titulo = titulo_da_chave(&perfil.nome);
        let atual = perfil.credencial;
        let resultado = self.cofre.mudar(|c| {
            let mut item = atual
                .and_then(|i| c.item(i).cloned())
                .filter(|i| i.tipo == Tipo::Senha && i.titulo == titulo)
                .unwrap_or_default();
            item.tipo = Tipo::Senha;
            item.titulo = titulo.clone();
            item.url = perfil.url_limpa();
            item.senha = chave;
            c.salvar(item)
        });
        match resultado {
            Ok(item) => {
                self.assistente.perfis.update(|a| {
                    if let Some(p) = a.perfil_mut(id) {
                        p.credencial = Some(item);
                    }
                });
                self.avisar(InfoBarSeverity::Success, t!("ia-chave-guardada"));
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            ),
        }
    }

    /// Cria um provedor a partir do preset, escolhe e leva à página dos provedores.
    pub(super) fn novo_perfil_de_ia(&self, preset: Preset) -> u64 {
        let id = self.assistente.perfis.try_update(|a| {
            let mut perfil = PerfilIa::do_preset(0, preset);
            perfil.nome = a.nome_livre(&perfil.nome);
            a.adicionar(perfil)
        });
        let id = id.unwrap_or_default();
        self.abrir_perfis_de_ia(Some(id));
        id
    }

    /// Leva à página dos provedores, com o de id `id` escolhido.
    pub(super) fn abrir_perfis_de_ia(&self, id: Option<u64>) {
        if let Some(id) = id {
            self.assistente.escolhido.set(Some(id.to_string()));
        }
        self.ir_para("ia");
    }

    /// O primeiro uso: sem nenhum provedor, abre o diálogo para escolher um. Devolve se havia
    /// provedor (quem chama segue com o gatilho só se houver).
    pub(super) fn exigir_perfil_de_ia(&self) -> bool {
        let ha = self
            .assistente
            .perfis
            .with_untracked(|a| !a.perfis.is_empty());
        if !ha {
            self.assistente.escolher_aberto.set(true);
        }
        ha
    }
}

/// O texto da linha "Assistente de IA" de uma conexão que escolheu `escolhido`: o nome do
/// provedor, ou `Padrão (…)` quando usa o padrão (ou o que escolheu já não existe).
fn rotulo_do_perfil(arquivo: &ArquivoIa, escolhido: Option<u64>) -> String {
    match escolhido.and_then(|id| arquivo.perfil(id)) {
        Some(perfil) => perfil.nome.clone(),
        None => match arquivo.padrao_efetivo() {
            Some(padrao) => t!("ia-conexao-padrao", nome = padrao.nome.clone()),
            None => t!("ia-conexao-padrao-vazio"),
        },
    }
}

/// As opções da linha "Assistente de IA": o padrão e depois cada provedor.
pub(super) fn opcoes_de_perfil_ia(perfis: Signal<ArquivoIa>) -> Vec<String> {
    perfis.with(|a| {
        std::iter::once(rotulo_do_perfil(a, None))
            .chain(a.perfis.iter().map(|p| p.nome.clone()))
            .collect()
    })
}

impl Tela {
    /// O provedor que o texto da linha "Assistente de IA" diz, para uma conexão que tinha `atual`.
    /// O texto que é o rótulo de `atual` não muda nada.
    pub(super) fn perfil_ia_do_texto(&self, atual: Option<u64>, texto: &str) -> Option<u64> {
        self.assistente.perfis.with_untracked(|a| {
            if texto == rotulo_do_perfil(a, atual) {
                atual
            } else if texto == rotulo_do_perfil(a, None) {
                None
            } else {
                a.perfis
                    .iter()
                    .find(|p| p.nome == texto)
                    .map_or(atual, |p| Some(p.id))
            }
        })
    }

    /// Mostra na linha "Assistente de IA" o provedor da conexão escolhida.
    pub(super) fn sincronizar_perfil_ia(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let atual = self
            .tree
            .with_untracked(|t| t.connection(id).map(|c| c.perfil_ia));
        let Some(atual) = atual else {
            return;
        };
        let texto = self
            .assistente
            .perfis
            .with_untracked(|a| rotulo_do_perfil(a, atual));
        poe(self.campos.perfil_ia, texto);
    }
}

/// O símbolo do cartão de um preset: uma máquina para os locais, a nuvem para os da internet.
fn simbolo(preset: Preset) -> Symbol {
    match PerfilIa::do_preset(0, preset).localidade() {
        _ if preset == Preset::Personalizado => Symbol::Settings,
        Locality::Internet => Symbol::Cloud,
        _ => Symbol::Server,
    }
}

/// O diálogo "Escolher um provedor de IA": um cartão por preset, o local primeiro.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl View {
    let aberto = tela.assistente.escolher_aberto;
    let mut cartoes = Column::new().gap(4).width(460);
    for preset in Preset::TODOS {
        let t = Rc::clone(tela);
        cartoes = cartoes.child(
            SettingsCard::new(preset.nome())
                .id(format!("ia-escolher-{}", preset.chave()))
                .description(preset.descricao_do_cartao())
                .icon(simbolo(preset))
                .on_click(move || {
                    aberto.set(false);
                    t.novo_perfil_de_ia(preset);
                }),
        );
    }
    let escolher = ContentDialog::new(t!("ia-escolher-titulo"), aberto)
        .id("ia-escolher")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(t!("ia-escolher-texto")).secondary().wrap())
                .child(cartoes),
        )
        .close(t!("comum-cancelar"));
    Column::new()
        .child(escolher)
        .child(super::ia_envio::dialogos(tela))
        .child(super::ia_chat::dialogos(tela))
}
