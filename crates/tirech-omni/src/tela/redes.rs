//! Os perfis de rede na casca: o perfil de cada aba aberta, o sufixo ` · Cliente A` no nome da
//! aba e o comando de descer a rede quando o último uso acaba. A página Redes fica em
//! `crate::tela_redes`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::clipboard;
use tomni_ui::dialogs;
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use super::{
    Tela, como_porta, id_do_rotulo, opcoes_de_credencial, perguntar, poe, rotulo,
    rotulo_da_credencial,
};
use crate::agente::{acordar, ler_contador};
use crate::cofre::{
    CofreCompartilhado, IdentidadeNebula, IdentidadeOpenVpn, IdentidadeTailscale,
    IdentidadeZeroTier, Item, Tipo,
};
use crate::modelo::{Connection, Jump, Kind, Node, Tree};
use crate::nebula;
use crate::openvpn;
use crate::recentes;
use crate::redes::{
    self, CORES, MotorRede, NebulaRede, OpenVpnRede, PerfilRede, ProxyRede, Rede, TailscaleRede,
    TipoProxy, WireGuardRede, ZeroTierRede, herdar, nome_livre, opcoes_de_rede, rede_do_rotulo,
    rede_efetiva, rotulo_da_rede,
};
use crate::t;
use crate::tailscale;
use crate::wireguard;
use crate::zerotier;

/// As opções da linha "Rede" do nó escolhido na árvore (só `Herdar` se não há nó).
pub(super) fn opcoes_da_selecao(
    tree: Signal<Tree>,
    selecionado: Signal<Option<String>>,
) -> Vec<String> {
    let id = selecionado.get().and_then(|k| k.parse::<u64>().ok());
    tree.with(|t| match id {
        Some(id) if t.find(id).is_some() => opcoes_de_rede(t, id),
        _ => vec![herdar()],
    })
}

impl Tela {
    /// O perfil de rede por onde a conexão `id` sai (`None`: sai direto, ou é o shell local).
    pub(super) fn perfil_da_conexao(&self, id: u64) -> Option<PerfilRede> {
        self.tree.with_untracked(|t| {
            let local = t.connection(id).is_none_or(|c| c.kind == Kind::Local);
            if local {
                None
            } else {
                rede_efetiva(t, id).cloned()
            }
        })
    }

    /// A rede que o texto da linha "Rede" diz para o nó `id`, que tinha `atual`. O texto que é o
    /// rótulo de `atual` não muda nada (um perfil que sumiu continua como estava até a pessoa
    /// escolher outra coisa).
    pub(super) fn rede_do_texto(&self, id: u64, atual: Rede, texto: &str) -> Rede {
        self.tree.with_untracked(|t| {
            if texto == rotulo_da_rede(t, id, atual) {
                atual
            } else {
                rede_do_rotulo(t, texto).unwrap_or(atual)
            }
        })
    }

    /// Mostra na linha "Rede" a rede do nó escolhido (conexão ou grupo).
    pub(super) fn sincronizar_rede(&self) {
        let Some(id) = self.id_selecionado() else {
            return;
        };
        let texto = self.tree.with_untracked(|t| {
            let rede = match t.find(id)? {
                Node::Connection(c) => c.rede,
                Node::Group(g) => g.rede,
            };
            Some(rotulo_da_rede(t, id, rede))
        });
        if let Some(texto) = texto {
            poe(self.campos.rede, texto);
        }
    }

    /// Passa a linha "Rede" para o grupo escolhido, grava e refaz os ícones da árvore. Lê o
    /// signal com rastreamento: serve a um efeito.
    pub(super) fn guardar_rede_do_grupo(&self) {
        let texto = self.campos.rede.get();
        untrack(|| {
            let Some(id) = self.id_selecionado() else {
                return;
            };
            let atual = self.tree.with_untracked(|t| match t.find(id) {
                Some(Node::Group(g)) => Some(g.rede),
                _ => None,
            });
            let Some(atual) = atual else {
                return;
            };
            let nova = self.rede_do_texto(id, atual, &texto);
            if nova == atual {
                return;
            }
            self.tree.update(|t| {
                if let Some(Node::Group(g)) = t.find_mut(id) {
                    g.rede = nova;
                }
            });
            self.atualizar();
        });
    }

    /// Guarda o perfil da sessão e põe o nome do perfil no fim do nome da aba.
    pub(super) fn marcar_rede_da_sessao(&self, sessao: Session, nome: &str, perfil: PerfilRede) {
        sessao.name(format!("{nome} · {}", perfil.name));
        self.rede_das_sessoes.borrow_mut().insert(sessao, perfil);
    }

    /// A sessão `nova`, copiada de `origem`, sai pela mesma rede (o nome da aba já vem junto).
    pub(super) fn herdar_rede_da_sessao(&self, origem: Option<Session>, nova: Session) {
        let mut mapa = self.rede_das_sessoes.borrow_mut();
        if let Some(perfil) = origem.and_then(|o| mapa.get(&o).cloned()) {
            mapa.insert(nova, perfil);
        }
    }

    /// Guarda o perfil de rede da aba de arquivos. Uma aba que usa a sessão de um terminal vai
    /// pelo canal dela e já conta pela sessão.
    pub(super) fn marcar_rede_da_aba(
        &self,
        chave: &str,
        conexao: Option<u64>,
        sessao: Option<Session>,
    ) {
        if sessao.is_some() {
            return;
        }
        if let Some(perfil) = conexao.and_then(|id| self.perfil_da_conexao(id)) {
            self.rede_das_abas
                .borrow_mut()
                .insert(chave.to_owned(), perfil);
        }
    }

    /// Os perfis das sessões e das abas de arquivos abertas agora (com rastreamento: serve a um
    /// efeito).
    fn perfis_em_uso(&self) -> Vec<PerfilRede> {
        let mut perfis = Vec::new();
        {
            let mapa = self.rede_das_sessoes.borrow();
            for janela in self.janelas.todas() {
                let sessoes = janela.layout.try_with(|l| l.sessions()).unwrap_or_default();
                perfis.extend(sessoes.iter().filter_map(|s| mapa.get(s).cloned()));
            }
        }
        let mapa = self.rede_das_abas.borrow();
        perfis.extend(
            self.abas_arquivos
                .get()
                .iter()
                .filter_map(|a| mapa.get(&a.chave).cloned()),
        );
        // As redes que a principal empresta às secundárias.
        self.instancias
            .emprestadas
            .with(|m| perfis.extend(m.values().cloned()));
        perfis
    }

    /// Roda o comando de descer de cada perfil que ficou sem sessão e sem aba de arquivos.
    pub(super) fn vigiar_redes(self: &Rc<Self>) {
        self.motores.ao_mudar(self.nebula_mudou);
        self.motores_vpn.ao_mudar(self.nebula_mudou);
        self.motores_wireguard.ao_mudar(self.nebula_mudou);
        self.motores_tailscale.ao_mudar(self.nebula_mudou);
        self.motores_zerotier.ao_mudar(self.nebula_mudou);
        // Duas cópias do app com o mesmo perfil ligado derrubam uma à outra no servidor.
        let pasta = self.armazenamento.pasta();
        self.motores.na_pasta(pasta);
        self.motores_vpn.na_pasta(pasta);
        self.motores_wireguard.na_pasta(pasta);
        self.motores_tailscale.na_pasta(pasta);
        self.motores_zerotier.na_pasta(pasta);
        let t = Rc::clone(self);
        effect(move || {
            // O nó Tailscale muda de estado sozinho (chaves, login, endereços): a tela grava no cofre.
            let _ = ler_contador(t.nebula_mudou);
            untrack(|| {
                t.gravar_nos_tailscale();
                t.gravar_nos_zerotier();
            });
        });
        let t = Rc::clone(self);
        effect(move || {
            let perfis = t.perfis_em_uso();
            for perfil in t.usos.atualizar(&perfis) {
                redes::descer(&perfil);
                t.soltar_rede_emprestada(perfil.id);
                if perfil.nebula.is_some() {
                    t.motores.parar(perfil.id);
                }
                if perfil.openvpn.is_some() {
                    t.motores_vpn.parar(perfil.id);
                }
                if perfil.wireguard.is_some() {
                    t.motores_wireguard.parar(perfil.id);
                }
                if perfil.tailscale.is_some() {
                    t.motores_tailscale.parar(perfil.id);
                }
                if perfil.zerotier.is_some() {
                    t.motores_zerotier.parar(perfil.id);
                }
            }
        });
    }
}

// --- A página Redes ---

/// A opção do tipo de proxy que deixa o perfil sem proxy.
fn rotulo_sem_proxy() -> String {
    t!("comum-nenhum")
}
/// A opção da credencial que não liga o proxy ou o salto a nenhum item do cofre.
fn sem_credencial() -> String {
    t!("comum-nenhuma")
}
/// A opção do tipo da rede que sobe um proxy de fora (SOCKS5 ou HTTP).
fn rotulo_tipo_proxy() -> String {
    t!("redes-tipo-proxy")
}
/// A opção do tipo da rede que liga a Nebula embutida (M48).
fn rotulo_tipo_nebula() -> String {
    t!("redes-tipo-nebula")
}
/// A opção do tipo da rede que liga a OpenVPN embutida (M56).
fn rotulo_tipo_openvpn() -> String {
    t!("redes-tipo-openvpn")
}

/// A opção do tipo da rede que liga o WireGuard embutido (M57).
fn rotulo_tipo_wireguard() -> String {
    t!("redes-tipo-wireguard")
}
/// A opção do tipo da rede que liga a Tailscale embutida (M57).
fn rotulo_tipo_tailscale() -> String {
    t!("redes-tipo-tailscale")
}
/// A opção do tipo da rede que liga o ZeroTier embutido (M61).
fn rotulo_tipo_zerotier() -> String {
    t!("redes-tipo-zerotier")
}

/// O tipo do perfil que o menu do "Novo" cria: assim a seção da rede (o ID do ZeroTier, o login
/// da Tailscale) já aparece, sem procurar a linha "Tipo" no topo das propriedades.
#[derive(Clone, Copy)]
enum TipoNovo {
    Proxy,
    Nebula,
    OpenVpn,
    WireGuard,
    Tailscale,
    ZeroTier,
}

/// Um item do menu do "Novo": o tipo, o id do item e o rótulo.
type ItemDoNovo = (TipoNovo, &'static str, fn() -> String);

impl TipoNovo {
    /// Os itens do menu, na ordem da linha "Tipo".
    const TODOS: [ItemDoNovo; 6] = [
        (Self::Proxy, "rede-nova-proxy", rotulo_tipo_proxy),
        (Self::Nebula, "rede-nova-nebula", rotulo_tipo_nebula),
        (Self::OpenVpn, "rede-nova-openvpn", rotulo_tipo_openvpn),
        (
            Self::WireGuard,
            "rede-nova-wireguard",
            rotulo_tipo_wireguard,
        ),
        (
            Self::Tailscale,
            "rede-nova-tailscale",
            rotulo_tipo_tailscale,
        ),
        (Self::ZeroTier, "rede-nova-zerotier", rotulo_tipo_zerotier),
    ];
}

/// As opções de uma linha "Identidade": nenhuma e os itens do cofre do `tipo` que guardam a
/// identidade, por título. Um item criado à mão no cofre com o tipo, sem ela, não sobe a rede.
fn opcoes_de_identidade_do_tipo(cofre: &CofreCompartilhado, tipo: Tipo) -> Vec<String> {
    let mut itens: Vec<String> = cofre.ler(|c| {
        c.itens()
            .iter()
            .filter(|i| i.tipo == tipo && i.tem_identidade_de_rede())
            .map(rotulo)
            .collect()
    });
    itens.sort_by_key(|r| r.to_lowercase());
    itens.insert(0, sem_credencial());
    itens
}

/// O item `id` do cofre, se ele existe, é do `tipo` e guarda a identidade.
fn item_do_tipo(cofre: &CofreCompartilhado, id: Option<u64>, tipo: Tipo) -> Option<Item> {
    cofre
        .agora(|c| c.item(id?).cloned())
        .filter(|i| i.tipo == tipo && i.tem_identidade_de_rede())
}

/// O texto de uma linha "Identidade" para o item `id` (nenhuma, se ele não existe ou é de outro tipo).
fn rotulo_do_item_do_tipo(cofre: &CofreCompartilhado, id: Option<u64>, tipo: Tipo) -> String {
    item_do_tipo(cofre, id, tipo).map_or_else(sem_credencial, |item| rotulo(&item))
}

/// Uma máquina da tailnet na lista da página Redes.
#[derive(Clone, PartialEq)]
struct LinhaMaquina {
    nome: String,
    descricao: String,
    host: Option<String>,
    /// Já existe uma conexão que sai por este perfil e aponta para este endereço.
    cadastrada: bool,
}

impl LinhaMaquina {
    fn chave(&self) -> String {
        format!(
            "{}|{}|{}",
            self.nome,
            self.host.as_deref().unwrap_or(""),
            self.cadastrada
        )
    }
}

/// A linha de uma máquina: o resumo e o botão que cria uma conexão SSH para ela.
fn linha_da_maquina(
    maquina: LinhaMaquina,
    tela: &Rc<Tela>,
    escolhido: Signal<Option<String>>,
) -> Row {
    let id = format!("rede-maquina-{}", maquina.nome);
    let (t, nome, host) = (Rc::clone(tela), maquina.nome.clone(), maquina.host.clone());
    Row::new()
        .gap(8)
        .align(Align::Center)
        .child(
            Text::new(maquina.descricao.clone())
                .id(format!("{id}-texto"))
                .grow(1),
        )
        .child(
            Text::new(t!("redes-ja-cadastrada"))
                .id(format!("{id}-cadastrada"))
                .secondary()
                .visible(maquina.cadastrada),
        )
        .child(
            Button::new(t!("redes-nova-conexao"))
                .id(format!("{id}-conectar"))
                .icon(Symbol::Add)
                .visible(!maquina.cadastrada)
                .disabled(maquina.host.is_none())
                .on_click(move || {
                    let perfil = escolhido
                        .get_untracked()
                        .and_then(|k| k.parse::<u64>().ok());
                    if let (Some(perfil), Some(host)) = (perfil, host.clone()) {
                        t.nova_conexao_na_maquina(perfil, nome.clone(), host);
                    }
                }),
        )
}

/// A identidade OpenVPN do item `id`, se ele existe e é desse tipo.
fn identidade_vpn_do_cofre(
    cofre: &CofreCompartilhado,
    id: Option<u64>,
) -> Option<(Item, IdentidadeOpenVpn)> {
    let item = cofre.agora(|c| c.item(id?).cloned())?;
    let identidade = item.openvpn.clone()?;
    Some((item, identidade))
}

/// O texto da linha "Identidade" da OpenVPN para o item `id` (nenhuma, se ele não existe).
fn rotulo_da_identidade_vpn(cofre: &CofreCompartilhado, id: Option<u64>) -> String {
    identidade_vpn_do_cofre(cofre, id).map_or_else(sem_credencial, |(item, _)| rotulo(&item))
}

/// A identidade Nebula do item `id`, se ele existe e é desse tipo.
fn identidade_do_cofre(
    cofre: &CofreCompartilhado,
    id: Option<u64>,
) -> Option<(Item, IdentidadeNebula)> {
    let item = cofre.agora(|c| c.item(id?).cloned())?;
    let identidade = item.nebula.clone()?;
    Some((item, identidade))
}

/// O texto da linha "Identidade" para o item `id` (nenhuma, se ele não existe).
fn rotulo_da_identidade(cofre: &CofreCompartilhado, id: Option<u64>) -> String {
    identidade_do_cofre(cofre, id).map_or_else(sem_credencial, |(item, _)| rotulo(&item))
}

/// Os segundos desde 1970, agora.
fn agora_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Troca o valor do signal só se for diferente.
fn poe_se(signal: Signal<f64>, valor: f64) {
    if signal.get_untracked() != valor {
        signal.set(valor);
    }
}

/// As opções da linha "Credencial" do perfil: nenhuma e os itens do cofre.
fn opcoes_de_credencial_do_perfil(cofre: &CofreCompartilhado) -> Vec<String> {
    let mut opcoes = opcoes_de_credencial(cofre);
    if let Some(primeira) = opcoes.first_mut() {
        *primeira = sem_credencial();
    }
    opcoes
}

fn rotulo_da_credencial_do_perfil(cofre: &CofreCompartilhado, id: Option<u64>) -> String {
    let texto = rotulo_da_credencial(cofre, id);
    if texto == perguntar() {
        sem_credencial()
    } else {
        texto
    }
}

/// Os valores do perfil escolhido, um signal por propriedade (o mesmo desenho dos `Campos` da
/// conexão).
#[derive(Clone, Copy)]
struct CamposRede {
    nome: Signal<String>,
    cor: Signal<String>,
    proxy: Signal<String>,
    host: Signal<String>,
    porta: Signal<f64>,
    credencial: Signal<String>,
    salto_host: Signal<String>,
    salto_porta: Signal<f64>,
    salto_usuario: Signal<String>,
    salto_credencial: Signal<String>,
    subir: Signal<String>,
    descer: Signal<String>,
    espera: Signal<f64>,
    /// "Proxy" ou "Nebula": o que o perfil liga.
    tipo: Signal<String>,
    /// O item do cofre com a identidade Nebula (o texto da linha).
    identidade: Signal<String>,
    /// O `config.yml` do perfil Nebula, sem os segredos. Fica guardado mesmo com o tipo Proxy.
    config: Signal<String>,
    /// O nome do arquivo de onde o config veio.
    origem: Signal<String>,
    // O que só se lê: o resumo da identidade, os lighthouses e o estado do motor.
    endereco: Signal<String>,
    grupos: Signal<String>,
    validade: Signal<String>,
    lighthouses: Signal<String>,
    estado: Signal<String>,
    // A OpenVPN (M56): os mesmos papéis, cada um com o seu signal.
    vpn_identidade: Signal<String>,
    vpn_config: Signal<String>,
    vpn_origem: Signal<String>,
    /// Usar as rotas que o servidor empurra (desligado, o perfil guarda o `route-nopull`).
    vpn_usar_rotas: Signal<bool>,
    vpn_servidor: Signal<String>,
    vpn_validade: Signal<String>,
    vpn_rotas: Signal<String>,
    vpn_estado: Signal<String>,
    // O WireGuard (M57).
    wg_identidade: Signal<String>,
    wg_config: Signal<String>,
    wg_origem: Signal<String>,
    wg_chave_publica: Signal<String>,
    wg_enderecos: Signal<String>,
    wg_servidor: Signal<String>,
    wg_rotas: Signal<String>,
    wg_estado: Signal<String>,
    wg_pares: Signal<String>,
    // A Tailscale (M57).
    ts_identidade: Signal<String>,
    /// A URL do servidor de controle; vazio é a Tailscale oficial.
    ts_servidor: Signal<String>,
    ts_nome: Signal<String>,
    ts_sub_redes: Signal<bool>,
    ts_efemero: Signal<bool>,
    /// As CAs extras do servidor em PEM.
    ts_ca: Signal<String>,
    ts_login: Signal<String>,
    ts_tailnet: Signal<String>,
    ts_enderecos: Signal<String>,
    ts_validade: Signal<String>,
    ts_chave: Signal<String>,
    ts_estado: Signal<String>,
    // O ZeroTier (M61).
    zt_identidade: Signal<String>,
    /// O id da rede (16 hexadecimais).
    zt_rede: Signal<String>,
    zt_globais: Signal<bool>,
    zt_padrao: Signal<bool>,
    zt_dns: Signal<bool>,
    zt_ip_fixo: Signal<String>,
    /// As moons, `id/semente` separadas por espaço.
    zt_moons: Signal<String>,
    zt_endereco: Signal<String>,
    zt_nome_rede: Signal<String>,
    zt_enderecos: Signal<String>,
    zt_estado: Signal<String>,
    zt_detalhes: Signal<String>,
}

impl CamposRede {
    fn novo() -> Self {
        let proxy = ProxyRede::novo();
        Self {
            nome: signal(String::new()),
            cor: signal(CORES[0].to_owned()),
            proxy: signal(rotulo_sem_proxy()),
            host: signal(proxy.host),
            porta: signal(f64::from(proxy.port)),
            credencial: signal(sem_credencial()),
            salto_host: signal(String::new()),
            salto_porta: signal(22.0),
            salto_usuario: signal(String::new()),
            salto_credencial: signal(sem_credencial()),
            subir: signal(String::new()),
            descer: signal(String::new()),
            espera: signal(30.0),
            tipo: signal(rotulo_tipo_proxy()),
            identidade: signal(sem_credencial()),
            config: signal(String::new()),
            origem: signal(String::new()),
            endereco: signal(String::new()),
            grupos: signal(String::new()),
            validade: signal(String::new()),
            lighthouses: signal(String::new()),
            estado: signal(String::new()),
            vpn_identidade: signal(sem_credencial()),
            vpn_config: signal(String::new()),
            vpn_origem: signal(String::new()),
            vpn_usar_rotas: signal(true),
            vpn_servidor: signal(String::new()),
            vpn_validade: signal(String::new()),
            vpn_rotas: signal(String::new()),
            vpn_estado: signal(String::new()),
            wg_identidade: signal(sem_credencial()),
            wg_config: signal(String::new()),
            wg_origem: signal(String::new()),
            wg_chave_publica: signal(String::new()),
            wg_enderecos: signal(String::new()),
            wg_servidor: signal(String::new()),
            wg_rotas: signal(String::new()),
            wg_estado: signal(String::new()),
            wg_pares: signal(String::new()),
            ts_identidade: signal(sem_credencial()),
            ts_servidor: signal(String::new()),
            ts_nome: signal(String::new()),
            ts_sub_redes: signal(true),
            ts_efemero: signal(false),
            ts_ca: signal(String::new()),
            ts_login: signal(String::new()),
            ts_tailnet: signal(String::new()),
            ts_enderecos: signal(String::new()),
            ts_validade: signal(String::new()),
            ts_chave: signal(String::new()),
            ts_estado: signal(String::new()),
            zt_identidade: signal(sem_credencial()),
            zt_rede: signal(String::new()),
            zt_globais: signal(false),
            zt_padrao: signal(false),
            zt_dns: signal(false),
            zt_ip_fixo: signal(String::new()),
            zt_moons: signal(String::new()),
            zt_endereco: signal(String::new()),
            zt_nome_rede: signal(String::new()),
            zt_enderecos: signal(String::new()),
            zt_estado: signal(String::new()),
            zt_detalhes: signal(String::new()),
        }
    }

    /// Mostra o perfil `p` nas propriedades.
    fn carregar(&self, p: &PerfilRede, cofre: &CofreCompartilhado) {
        let proxy = p.proxy.clone().unwrap_or_else(ProxyRede::novo);
        let salto = p.salto.clone();
        poe(self.nome, p.name.clone());
        poe(self.cor, p.nome_da_cor().to_owned());
        poe(
            self.proxy,
            p.proxy
                .as_ref()
                .map_or_else(rotulo_sem_proxy, |x| x.tipo.nome().to_owned()),
        );
        poe(self.host, proxy.host);
        poe_se(self.porta, f64::from(proxy.port));
        poe(
            self.credencial,
            rotulo_da_credencial_do_perfil(cofre, proxy.credencial),
        );
        poe(
            self.salto_host,
            salto.as_ref().map(|j| j.host.clone()).unwrap_or_default(),
        );
        poe_se(
            self.salto_porta,
            salto.as_ref().map_or(22.0, |j| f64::from(j.port)),
        );
        poe(
            self.salto_usuario,
            salto.as_ref().map(|j| j.user.clone()).unwrap_or_default(),
        );
        poe(
            self.salto_credencial,
            rotulo_da_credencial_do_perfil(cofre, salto.as_ref().and_then(|j| j.credencial)),
        );
        poe(self.subir, p.subir.clone().unwrap_or_default());
        poe(self.descer, p.descer.clone().unwrap_or_default());
        poe_se(self.espera, f64::from(p.espera_secs));
        let nebula = p.nebula.clone().unwrap_or_default();
        poe(
            self.tipo,
            if p.nebula.is_some() {
                rotulo_tipo_nebula()
            } else if p.openvpn.is_some() {
                rotulo_tipo_openvpn()
            } else if p.wireguard.is_some() {
                rotulo_tipo_wireguard()
            } else if p.tailscale.is_some() {
                rotulo_tipo_tailscale()
            } else if p.zerotier.is_some() {
                rotulo_tipo_zerotier()
            } else {
                rotulo_tipo_proxy()
            },
        );
        poe(
            self.identidade,
            rotulo_da_identidade(cofre, nebula.identidade),
        );
        poe(self.config, nebula.config);
        poe(self.origem, nebula.origem);
        let vpn = p.openvpn.clone().unwrap_or_default();
        poe(
            self.vpn_identidade,
            rotulo_da_identidade_vpn(cofre, vpn.identidade),
        );
        poe(self.vpn_config, vpn.config);
        poe(self.vpn_origem, vpn.origem);
        if self.vpn_usar_rotas.get_untracked() == vpn.ignorar_rotas {
            self.vpn_usar_rotas.set(!vpn.ignorar_rotas);
        }
        let wg = p.wireguard.clone().unwrap_or_default();
        poe(
            self.wg_identidade,
            rotulo_do_item_do_tipo(cofre, wg.identidade, Tipo::WireGuard),
        );
        poe(self.wg_config, wg.config);
        poe(self.wg_origem, wg.origem);
        let ts = p.tailscale.clone().unwrap_or_default();
        poe(
            self.ts_identidade,
            rotulo_do_item_do_tipo(cofre, ts.identidade, Tipo::Tailscale),
        );
        poe(self.ts_servidor, ts.servidor);
        poe(self.ts_nome, ts.nome_maquina);
        poe(self.ts_ca, ts.ca);
        if self.ts_sub_redes.get_untracked() != ts.usar_sub_redes {
            self.ts_sub_redes.set(ts.usar_sub_redes);
        }
        if self.ts_efemero.get_untracked() != ts.efemero {
            self.ts_efemero.set(ts.efemero);
        }
        let zt = p.zerotier.clone().unwrap_or_default();
        poe(
            self.zt_identidade,
            rotulo_do_item_do_tipo(cofre, zt.identidade, Tipo::ZeroTier),
        );
        poe(self.zt_rede, zt.rede);
        poe(self.zt_ip_fixo, zt.ip_fixo);
        poe(self.zt_moons, zerotier::moons_para_texto(&zt.moons));
        for (campo, valor) in [
            (self.zt_globais, zt.permitir_globais),
            (self.zt_padrao, zt.permitir_padrao),
            (self.zt_dns, zt.permitir_dns),
        ] {
            if campo.get_untracked() != valor {
                campo.set(valor);
            }
        }
    }

    /// Se o perfil é uma rede Nebula (lê o signal com rastreamento).
    fn e_nebula(&self) -> bool {
        self.tipo.get() == rotulo_tipo_nebula()
    }

    /// Se o perfil é uma rede OpenVPN (lê o signal com rastreamento).
    fn e_openvpn(&self) -> bool {
        self.tipo.get() == rotulo_tipo_openvpn()
    }

    /// Se o perfil é uma rede WireGuard (lê o signal com rastreamento).
    fn e_wireguard(&self) -> bool {
        self.tipo.get() == rotulo_tipo_wireguard()
    }

    /// Se o perfil é uma rede Tailscale (lê o signal com rastreamento).
    fn e_tailscale(&self) -> bool {
        self.tipo.get() == rotulo_tipo_tailscale()
    }

    /// Se o perfil é uma rede ZeroTier (lê o signal com rastreamento).
    fn e_zerotier(&self) -> bool {
        self.tipo.get() == rotulo_tipo_zerotier()
    }

    /// O perfil `base` com o que está nas propriedades. Lê os signals com rastreamento. O nome
    /// vem como foi digitado (sem as pontas); quem grava trata o nome repetido.
    fn aplicar(&self, base: &PerfilRede, cofre: &CofreCompartilhado) -> PerfilRede {
        let mut p = base.clone();
        let nome = self.nome.get();
        if !nome.trim().is_empty() {
            p.name = nome.trim().to_owned();
        }
        let cor = self.cor.get();
        p.cor = CORES
            .iter()
            .position(|c| *c == cor)
            .map_or(base.cor, |i| i as u8);
        let tipo = self.proxy.get();
        let (host, porta) = (self.host.get(), como_porta(self.porta.get()));
        let credencial = self.credencial.get();
        let credencial_do_salto = self.salto_credencial.get();
        // Com o cofre bloqueado a lista só tem "Nenhuma": a ligação que já existe fica.
        let bloqueado = cofre.agora(|c| c.bloqueado());
        let credencial = if bloqueado {
            base.proxy.as_ref().and_then(|x| x.credencial)
        } else {
            id_do_rotulo(&credencial)
        };
        let credencial_do_salto = if bloqueado {
            base.salto.as_ref().and_then(|j| j.credencial)
        } else {
            id_do_rotulo(&credencial_do_salto)
        };
        let identidade = self.identidade.get();
        let identidade = if bloqueado {
            base.nebula.as_ref().and_then(|n| n.identidade)
        } else {
            id_do_rotulo(&identidade)
        };
        let identidade_vpn = self.vpn_identidade.get();
        let identidade_vpn = if bloqueado {
            base.openvpn.as_ref().and_then(|n| n.identidade)
        } else {
            id_do_rotulo(&identidade_vpn)
        };
        let identidade_wg = self.wg_identidade.get();
        let identidade_wg = if bloqueado {
            base.wireguard.as_ref().and_then(|n| n.identidade)
        } else {
            id_do_rotulo(&identidade_wg)
        };
        let identidade_ts = self.ts_identidade.get();
        let identidade_ts = if bloqueado {
            base.tailscale.as_ref().and_then(|n| n.identidade)
        } else {
            id_do_rotulo(&identidade_ts)
        };
        let identidade_zt = self.zt_identidade.get();
        let identidade_zt = if bloqueado {
            base.zerotier.as_ref().and_then(|n| n.identidade)
        } else {
            id_do_rotulo(&identidade_zt)
        };
        p.zerotier = self.e_zerotier().then(|| ZeroTierRede {
            rede: zerotier::rede_normalizada(&self.zt_rede.get()),
            identidade: identidade_zt,
            permitir_globais: self.zt_globais.get(),
            permitir_padrao: self.zt_padrao.get(),
            permitir_dns: self.zt_dns.get(),
            ip_fixo: self.zt_ip_fixo.get().trim().to_owned(),
            moons: zerotier::moons_de_texto(&self.zt_moons.get()),
        });
        p.wireguard = self.e_wireguard().then(|| WireGuardRede {
            config: self.wg_config.get(),
            identidade: identidade_wg,
            origem: self.wg_origem.get(),
        });
        p.tailscale = self.e_tailscale().then(|| TailscaleRede {
            servidor: self.ts_servidor.get().trim().to_owned(),
            nome_maquina: self.ts_nome.get().trim().to_owned(),
            identidade: identidade_ts,
            usar_sub_redes: self.ts_sub_redes.get(),
            efemero: self.ts_efemero.get(),
            ca: self.ts_ca.get(),
        });
        p.openvpn = self.e_openvpn().then(|| OpenVpnRede {
            config: self.vpn_config.get(),
            identidade: identidade_vpn,
            origem: self.vpn_origem.get(),
            ignorar_rotas: !self.vpn_usar_rotas.get(),
        });
        p.nebula = self.e_nebula().then(|| NebulaRede {
            config: self.config.get(),
            identidade,
            origem: self.origem.get(),
        });
        p.proxy = [TipoProxy::Socks5, TipoProxy::Http]
            .into_iter()
            .find(|t| t.nome() == tipo)
            .map(|tipo| ProxyRede {
                tipo,
                host: host.trim().to_owned(),
                port: porta,
                credencial,
            });
        let salto = self.salto_host.get().trim().to_owned();
        let (salto_porta, salto_usuario) = (
            como_porta(self.salto_porta.get()),
            self.salto_usuario.get().trim().to_owned(),
        );
        p.salto = (!salto.is_empty()).then_some(Jump {
            host: salto,
            port: salto_porta,
            user: salto_usuario,
            credencial: credencial_do_salto,
        });
        let comando = |texto: String| {
            let texto = texto.trim().to_owned();
            (!texto.is_empty()).then_some(texto)
        };
        p.subir = comando(self.subir.get());
        p.descer = comando(self.descer.get());
        p.espera_secs = self.espera.get().clamp(1.0, 3600.0).round() as u32;
        p
    }
}

/// O diálogo de usuário e senha que a importação do `.ovpn` abre quando o arquivo pede
/// `auth-user-pass` e não traz o usuário.
#[derive(Clone, Copy)]
struct PedidoDeCredenciais {
    aberto: Signal<bool>,
    /// O item `Tipo::OpenVpn` do cofre que recebe o usuário e a senha.
    item: Signal<Option<u64>>,
    usuario: Signal<String>,
    senha: Signal<String>,
}

impl PedidoDeCredenciais {
    fn novo() -> Self {
        Self {
            aberto: signal(false),
            item: signal(None),
            usuario: signal(String::new()),
            senha: signal(String::new()),
        }
    }
}

/// O que a thread do "Testar" devolve: o perfil, o nome dele e o resultado (a mensagem de
/// sucesso já pronta, ou o erro).
type Testada = (u64, String, Result<String, String>);

impl Tela {
    /// Passa as propriedades da página Redes para o perfil escolhido, grava e atualiza a lista.
    fn guardar_perfil(&self, campos: &CamposRede, escolhido: Signal<Option<String>>) {
        let id = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok());
        let base = id.and_then(|id| self.tree.with_untracked(|t| t.perfil(id).cloned()));
        let Some(base) = base else {
            // Lê os signals mesmo assim, para o efeito continuar dependendo deles.
            let _ = campos.aplicar(&PerfilRede::new(""), &self.cofre);
            return;
        };
        let mut nova = campos.aplicar(&base, &self.cofre);
        let digitado = nova.name.clone();
        nova.name = self
            .tree
            .with_untracked(|t| nome_livre(t, &digitado, Some(base.id)));
        // O nome que a pessoa digitou só volta à caixa quando mudou além das pontas.
        if nova.name != digitado {
            poe(campos.nome, nova.name.clone());
        }
        // Ligar um proxy novo preenche o endereço padrão, se a caixa estava vazia.
        if let Some(proxy) = nova.proxy.as_mut()
            && proxy.host.is_empty()
        {
            proxy.host = ProxyRede::novo().host;
            poe(campos.host, proxy.host.clone());
        }
        // Entrar pelo navegador leva mais que os 30 s do padrão: a Tailscale nova espera 120 s.
        if nova.tailscale.is_some() && base.tailscale.is_none() && nova.espera_secs < 120 {
            nova.espera_secs = 120;
            poe_se(campos.espera, 120.0);
        }
        // O ZeroTier espera a autorização de quem administra a rede, e isso também leva mais que 30 s.
        if nova.zerotier.is_some()
            && base.zerotier.is_none()
            && nova.espera_secs < zerotier::ESPERA_PADRAO_SECS
        {
            nova.espera_secs = zerotier::ESPERA_PADRAO_SECS;
            poe_se(campos.espera, f64::from(zerotier::ESPERA_PADRAO_SECS));
        }
        if nova == base {
            return;
        }
        self.tree.update(|t| {
            if let Some(p) = t.perfil_mut(base.id) {
                *p = nova.clone();
            }
        });
        self.atualizar();
        self.sincronizar_rede();
    }

    /// Cria um perfil do `tipo` e o escolhe. Começa como a troca da linha "Tipo" deixaria: o
    /// SOCKS5 em `127.0.0.1:1081`, a rede embutida vazia e a espera de
    /// `redes.espera_padrao_secs` (ou a mais longa do Tailscale e do ZeroTier, que esperam o
    /// login ou a autorização).
    fn novo_perfil(&self, escolhido: Signal<Option<String>>, tipo: TipoNovo) {
        let espera = self.config.with_untracked(|c| c.redes.espera_padrao_secs);
        let id = self.tree.try_update(|t| {
            let mut p = PerfilRede::new(nome_livre(t, &t!("redes-nova-rede"), None));
            p.proxy = Some(ProxyRede::novo());
            p.cor = (t.redes.len() % CORES.len()) as u8;
            p.espera_secs = espera;
            match tipo {
                TipoNovo::Proxy => {}
                TipoNovo::Nebula => p.nebula = Some(NebulaRede::default()),
                TipoNovo::OpenVpn => p.openvpn = Some(OpenVpnRede::default()),
                TipoNovo::WireGuard => p.wireguard = Some(WireGuardRede::default()),
                TipoNovo::Tailscale => {
                    p.tailscale = Some(TailscaleRede::default());
                    p.espera_secs = p.espera_secs.max(120);
                }
                TipoNovo::ZeroTier => {
                    p.zerotier = Some(ZeroTierRede::default());
                    p.espera_secs = p.espera_secs.max(zerotier::ESPERA_PADRAO_SECS);
                }
            }
            t.add_perfil(p)
        });
        self.atualizar();
        escolhido.set(id.map(|id| id.to_string()));
    }

    /// Copia o perfil escolhido e escolhe a cópia.
    fn duplicar_perfil_escolhido(&self, escolhido: Signal<Option<String>>) {
        let Some(id) = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok())
        else {
            return;
        };
        let nova = self
            .tree
            .try_update(|t| {
                let nova = t.duplicar_perfil(id)?;
                let nome = t.perfil(nova).map(|p| p.name.clone()).unwrap_or_default();
                let livre = nome_livre(t, &nome, Some(nova));
                if let Some(p) = t.perfil_mut(nova) {
                    p.name = livre;
                }
                Some(nova)
            })
            .flatten();
        self.atualizar();
        if let Some(nova) = nova {
            escolhido.set(Some(nova.to_string()));
        }
    }

    /// Grava no cofre o estado novo de cada nó Tailscale (chaves, login, endereços). Com o cofre
    /// trancado, ou se a gravação falha, o estado fica esperando em memória para a próxima vez.
    pub(super) fn gravar_nos_tailscale(&self) {
        let pendentes = self.motores_tailscale.perfis_pendentes();
        if pendentes.is_empty() || self.cofre.agora(|c| c.bloqueado()) {
            return;
        }
        for perfil in pendentes {
            let Some(no) = self.motores_tailscale.tomar_pendente(perfil) else {
                continue;
            };
            let item = self.tree.with_untracked(|t| {
                t.perfil(perfil)
                    .and_then(|p| p.tailscale.as_ref())
                    .and_then(|ts| ts.identidade)
            });
            // Perfil apagado ou sem identidade: o estado não tem para onde ir.
            let Some(id) = item else {
                continue;
            };
            let gravado = self.cofre.mudar(|c| {
                let Some(mut item) = c.item(id).cloned() else {
                    return Ok(());
                };
                let Some(identidade) = item.tailscale.as_mut() else {
                    return Ok(());
                };
                crate::tailscale::gravar_no(identidade, &no);
                c.salvar(item).map(|_| ())
            });
            if gravado.is_err() {
                self.motores_tailscale.devolver(perfil, no);
            }
        }
    }

    /// Grava no cofre o estado novo de cada nó ZeroTier (a identidade que o motor gerou, a
    /// configuração da rede, os caminhos). Com o cofre trancado, ou se a gravação falha, o estado
    /// fica esperando em memória para a próxima vez.
    pub(super) fn gravar_nos_zerotier(&self) {
        let pendentes = self.motores_zerotier.perfis_pendentes();
        if pendentes.is_empty() || self.cofre.agora(|c| c.bloqueado()) {
            return;
        }
        for perfil in pendentes {
            let Some(no) = self.motores_zerotier.tomar_pendente(perfil) else {
                continue;
            };
            let item = self.tree.with_untracked(|t| {
                t.perfil(perfil)
                    .and_then(|p| p.zerotier.as_ref())
                    .and_then(|zt| zt.identidade)
            });
            // Perfil apagado ou sem identidade: o estado não tem para onde ir.
            let Some(id) = item else {
                continue;
            };
            let gravado = self.cofre.mudar(|c| {
                let Some(mut item) = c.item(id).cloned() else {
                    return Ok(());
                };
                let Some(identidade) = item.zerotier.as_mut() else {
                    return Ok(());
                };
                crate::zerotier::gravar_no(identidade, &no);
                c.salvar(item).map(|_| ())
            });
            if gravado.is_err() {
                self.motores_zerotier.devolver(perfil, no);
            }
        }
    }

    /// Muda a parte Nebula do perfil escolhido (que passa a ser Nebula), grava na árvore e mostra
    /// o resultado nas propriedades. Vale na hora, sem esperar os efeitos das propriedades.
    fn gravar_nebula(
        &self,
        campos: &CamposRede,
        escolhido: Signal<Option<String>>,
        mudar: impl FnOnce(&mut PerfilRede),
    ) {
        let Some(id) = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok())
        else {
            return;
        };
        let Some(mut perfil) = self.tree.with_untracked(|t| t.perfil(id).cloned()) else {
            return;
        };
        // O que está nas propriedades e ainda não foi para a árvore também entra.
        untrack(|| {
            perfil = campos.aplicar(&perfil, &self.cofre);
        });
        mudar(&mut perfil);
        perfil.name = self
            .tree
            .with_untracked(|t| nome_livre(t, &perfil.name, Some(id)));
        self.tree.update(|t| {
            if let Some(p) = t.perfil_mut(id) {
                *p = perfil.clone();
            }
        });
        self.atualizar();
        self.sincronizar_rede();
        campos.carregar(&perfil, &self.cofre);
    }

    /// "Importar config.yml…": lê o arquivo, guarda a identidade no cofre (ou reaproveita a que
    /// já está lá) e põe o resto no perfil, que passa a ser Nebula.
    fn importar_config(self: &Rc<Self>, campos: CamposRede, escolhido: Signal<Option<String>>) {
        if self.cofre.agora(|c| c.bloqueado()) {
            self.avisar(InfoBarSeverity::Warning, t!("redes-nebula-destrave"));
            return;
        }
        let t = Rc::clone(self);
        dialogs::open_file()
            .remember("redes")
            .title(t!("redes-importar-config"))
            .filter(t!("redes-config-yml"), &["yml", "yaml"])
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let texto = match std::fs::read_to_string(&caminho) {
                    Ok(texto) => texto,
                    Err(erro) => {
                        t.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "comum-nao-consegui-ler",
                                arquivo = caminho.display().to_string(),
                                erro = erro.to_string()
                            ),
                        );
                        return;
                    }
                };
                let importacao =
                    match nebula::importar(&texto, caminho.parent(), agora_secs() as i64) {
                        Ok(importacao) => importacao,
                        Err(erro) => {
                            t.avisar(InfoBarSeverity::Error, erro);
                            return;
                        }
                    };
                let impressao = importacao.identidade.impressao.clone();
                let existente = t.cofre.agora(|c| {
                    c.itens()
                        .iter()
                        .find(|i| {
                            i.nebula
                                .as_ref()
                                .is_some_and(|n| !impressao.is_empty() && n.impressao == impressao)
                        })
                        .map(|i| i.id)
                });
                let item = match existente {
                    Some(id) => id,
                    None => {
                        let novo = Item {
                            tipo: Tipo::Nebula,
                            titulo: t!(
                                "redes-titulo-da-identidade",
                                nome = importacao.identidade.nome.as_str()
                            ),
                            nebula: Some(importacao.identidade.clone()),
                            ..Item::default()
                        };
                        match t.cofre.mudar(|c| c.salvar(novo)) {
                            Ok(id) => id,
                            Err(erro) => {
                                t.avisar(
                                    InfoBarSeverity::Error,
                                    t!(
                                        "comum-nao-consegui-gravar-o-cofre",
                                        erro = erro.to_string()
                                    ),
                                );
                                return;
                            }
                        }
                    }
                };
                let origem = caminho
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let nome_da_rede = importacao.identidade.nome.clone();
                let yaml = importacao.yaml;
                t.gravar_nebula(&campos, escolhido, |perfil| {
                    perfil.nebula = Some(NebulaRede {
                        config: yaml,
                        identidade: Some(item),
                        origem,
                    });
                    // Um perfil que ainda tem o nome de fábrica passa a ter o do certificado.
                    if perfil.name.is_empty() || perfil.name.starts_with(&t!("redes-nova-rede")) {
                        perfil.name = nome_da_rede;
                    }
                });
                if importacao.avisos.is_empty() {
                    t.avisar(InfoBarSeverity::Success, t!("redes-config-importado"));
                } else {
                    t.avisar(InfoBarSeverity::Warning, importacao.avisos.join("\n"));
                }
            });
    }

    /// "Importar .ovpn…": lê o arquivo, guarda a identidade no cofre (ou reaproveita a que já está
    /// lá) e põe o resto no perfil, que passa a ser OpenVPN. Se o arquivo pede usuário e senha e
    /// não os traz, abre o diálogo para a pessoa preencher.
    fn importar_ovpn(
        self: &Rc<Self>,
        campos: CamposRede,
        escolhido: Signal<Option<String>>,
        pedido: PedidoDeCredenciais,
    ) {
        if self.cofre.agora(|c| c.bloqueado()) {
            self.avisar(InfoBarSeverity::Warning, t!("redes-openvpn-destrave"));
            return;
        }
        let t = Rc::clone(self);
        dialogs::open_file()
            .remember("redes")
            .title(t!("redes-importar-ovpn"))
            .filter(t!("redes-config-ovpn"), &["ovpn", "conf"])
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let texto = match std::fs::read_to_string(&caminho) {
                    Ok(texto) => texto,
                    Err(erro) => {
                        t.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "comum-nao-consegui-ler",
                                arquivo = caminho.display().to_string(),
                                erro = erro.to_string()
                            ),
                        );
                        return;
                    }
                };
                let importacao =
                    match openvpn::importar(&texto, caminho.parent(), std::time::SystemTime::now())
                    {
                        Ok(importacao) => importacao,
                        Err(erro) => {
                            t.avisar(InfoBarSeverity::Error, erro);
                            return;
                        }
                    };
                let origem = caminho
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let nome_da_rede = if importacao.identidade.cn.is_empty() {
                    caminho
                        .file_stem()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                } else {
                    importacao.identidade.cn.clone()
                };
                let (impressao, servidor) = (
                    importacao.identidade.impressao.clone(),
                    importacao.identidade.servidor.clone(),
                );
                let existente = t.cofre.agora(|c| {
                    c.itens()
                        .iter()
                        .find(|i| {
                            i.openvpn.as_ref().is_some_and(|o| {
                                !impressao.is_empty()
                                    && o.impressao == impressao
                                    && o.servidor == servidor
                            })
                        })
                        .map(|i| (i.id, i.usuario.clone()))
                });
                let (item, usuario) = match existente {
                    Some(achado) => achado,
                    None => {
                        let novo = Item {
                            tipo: Tipo::OpenVpn,
                            titulo: t!(
                                "redes-titulo-da-identidade-openvpn",
                                nome = nome_da_rede.as_str()
                            ),
                            usuario: importacao.usuario.clone(),
                            senha: importacao.senha.clone(),
                            openvpn: Some(importacao.identidade.clone()),
                            ..Item::default()
                        };
                        match t.cofre.mudar(|c| c.salvar(novo)) {
                            Ok(id) => (id, importacao.usuario.clone()),
                            Err(erro) => {
                                t.avisar(
                                    InfoBarSeverity::Error,
                                    t!(
                                        "comum-nao-consegui-gravar-o-cofre",
                                        erro = erro.to_string()
                                    ),
                                );
                                return;
                            }
                        }
                    }
                };
                let ovpn = importacao.ovpn.clone();
                t.gravar_nebula(&campos, escolhido, |perfil| {
                    perfil.nebula = None;
                    perfil.openvpn = Some(OpenVpnRede {
                        config: ovpn,
                        identidade: Some(item),
                        origem,
                        ignorar_rotas: false,
                    });
                    // Um perfil que ainda tem o nome de fábrica passa a ter o do certificado.
                    if perfil.name.is_empty() || perfil.name.starts_with(&t!("redes-nova-rede")) {
                        perfil.name = nome_da_rede;
                    }
                });
                let mut avisos = importacao.avisos.clone();
                if !importacao.ignoradas.is_empty() {
                    avisos.push(t!(
                        "redes-openvpn-ignoradas",
                        quantas = importacao.ignoradas.len(),
                        lista = importacao.ignoradas.join("; ")
                    ));
                }
                if avisos.is_empty() {
                    t.avisar(InfoBarSeverity::Success, t!("redes-ovpn-importado"));
                } else {
                    t.avisar(InfoBarSeverity::Warning, avisos.join("\n"));
                }
                if importacao.pede_credenciais && usuario.is_empty() {
                    pedido.item.set(Some(item));
                    pedido.usuario.set(String::new());
                    pedido.senha.set(String::new());
                    pedido.aberto.set(true);
                }
            });
    }

    /// "Importar .conf…": lê o arquivo, guarda a chave privada no cofre (ou reaproveita o item da
    /// mesma chave) e põe o resto no perfil, que passa a ser WireGuard.
    fn importar_wireguard(self: &Rc<Self>, campos: CamposRede, escolhido: Signal<Option<String>>) {
        if self.cofre.agora(|c| c.bloqueado()) {
            self.avisar(InfoBarSeverity::Warning, t!("redes-wireguard-destrave"));
            return;
        }
        let t = Rc::clone(self);
        dialogs::open_file()
            .remember("redes")
            .title(t!("redes-importar-wireguard"))
            .filter(t!("redes-config-wireguard"), &["conf"])
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let texto = match std::fs::read_to_string(&caminho) {
                    Ok(texto) => texto,
                    Err(erro) => {
                        t.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "comum-nao-consegui-ler",
                                arquivo = caminho.display().to_string(),
                                erro = erro.to_string()
                            ),
                        );
                        return;
                    }
                };
                let importacao = match wireguard::importar(&texto) {
                    Ok(importacao) => importacao,
                    Err(erro) => {
                        t.avisar(InfoBarSeverity::Error, erro);
                        return;
                    }
                };
                let origem = caminho
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let nome_da_rede = caminho
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let publica = importacao.identidade.chave_publica.clone();
                let existente = t.cofre.agora(|c| {
                    c.itens()
                        .iter()
                        .find(|i| {
                            i.wireguard
                                .as_ref()
                                .is_some_and(|w| !publica.is_empty() && w.chave_publica == publica)
                        })
                        .cloned()
                });
                // A mesma chave num item que já existe: o item fica e leva as chaves
                // pré-compartilhadas e os endereços do arquivo novo.
                let novo = match existente {
                    Some(mut item) => {
                        item.wireguard = Some(importacao.identidade.clone());
                        item
                    }
                    None => Item {
                        tipo: Tipo::WireGuard,
                        titulo: t!(
                            "redes-titulo-da-identidade-wireguard",
                            nome = nome_da_rede.as_str()
                        ),
                        wireguard: Some(importacao.identidade.clone()),
                        ..Item::default()
                    },
                };
                let item = match t.cofre.mudar(|c| c.salvar(novo)) {
                    Ok(id) => id,
                    Err(erro) => {
                        t.avisar(
                            InfoBarSeverity::Error,
                            t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
                        );
                        return;
                    }
                };
                let conf = importacao.conf.clone();
                t.gravar_nebula(&campos, escolhido, |perfil| {
                    perfil.nebula = None;
                    perfil.openvpn = None;
                    perfil.tailscale = None;
                    perfil.zerotier = None;
                    perfil.wireguard = Some(WireGuardRede {
                        config: conf,
                        identidade: Some(item),
                        origem,
                    });
                    // Um perfil que ainda tem o nome de fábrica passa a ter o do arquivo.
                    if perfil.name.is_empty() || perfil.name.starts_with(&t!("redes-nova-rede")) {
                        perfil.name = nome_da_rede;
                    }
                });
                if importacao.avisos.is_empty() {
                    t.avisar(InfoBarSeverity::Success, t!("redes-wireguard-importado"));
                } else {
                    t.avisar(InfoBarSeverity::Warning, importacao.avisos.join("\n"));
                }
            });
    }

    /// O item Tailscale do perfil escolhido. Se o perfil ainda não tem, cria um item vazio no cofre
    /// e liga o perfil a ele (que passa a ser Tailscale). `None`: o cofre está trancado ou não
    /// gravou.
    fn identidade_tailscale_do_perfil(
        &self,
        campos: &CamposRede,
        escolhido: Signal<Option<String>>,
    ) -> Option<u64> {
        let id = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok())?;
        let base = self.tree.with_untracked(|t| t.perfil(id).cloned())?;
        // O que está nas propriedades e ainda não foi para a árvore também conta.
        let atual = untrack(|| campos.aplicar(&base, &self.cofre));
        let existente = atual.tailscale.as_ref().and_then(|ts| ts.identidade);
        if item_do_tipo(&self.cofre, existente, Tipo::Tailscale).is_some() {
            return existente;
        }
        if self.cofre.agora(|c| c.bloqueado()) {
            self.avisar(InfoBarSeverity::Warning, t!("redes-tailscale-destrave"));
            return None;
        }
        let novo = Item {
            tipo: Tipo::Tailscale,
            titulo: t!(
                "redes-titulo-da-identidade-tailscale",
                nome = atual.name.as_str()
            ),
            tailscale: Some(IdentidadeTailscale::default()),
            ..Item::default()
        };
        let item = match self.cofre.mudar(|c| c.salvar(novo)) {
            Ok(id) => id,
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
                );
                return None;
            }
        };
        self.gravar_nebula(campos, escolhido, |perfil| {
            let tinha = perfil.tailscale.is_some();
            perfil.nebula = None;
            perfil.openvpn = None;
            perfil.wireguard = None;
            perfil.zerotier = None;
            let atual = perfil.tailscale.take().unwrap_or_default();
            perfil.tailscale = Some(TailscaleRede {
                identidade: Some(item),
                ..atual
            });
            if !tinha && perfil.espera_secs < 120 {
                perfil.espera_secs = 120;
            }
        });
        Some(item)
    }

    /// Guarda no item Tailscale do perfil a chave de autenticação (`tskey-auth-…`) que a pessoa
    /// colou: o próximo "Entrar" a usa em vez do navegador.
    fn guardar_chave_autenticacao(
        &self,
        campos: &CamposRede,
        escolhido: Signal<Option<String>>,
        chave: &str,
    ) {
        let Some(item) = self.identidade_tailscale_do_perfil(campos, escolhido) else {
            return;
        };
        let chave = chave.trim().to_owned();
        let resultado = self.cofre.mudar(|c| {
            let Some(mut item) = c.item(item).cloned() else {
                return Ok(0);
            };
            if let Some(identidade) = item.tailscale.as_mut() {
                identidade.chave_autenticacao = chave;
            }
            c.salvar(item)
        });
        match resultado {
            Ok(_) => self.avisar(InfoBarSeverity::Success, t!("redes-chave-guardada")),
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            ),
        }
    }

    /// "Esquecer esta máquina": desliga o nó do perfil e apaga as chaves e o login do item, para o
    /// próximo uso entrar como uma máquina nova.
    fn esquecer_maquina_tailscale(&self, escolhido: Signal<Option<String>>) {
        let Some(perfil) = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok())
        else {
            return;
        };
        let item = self.tree.with_untracked(|t| {
            t.perfil(perfil)
                .and_then(|p| p.tailscale.as_ref())
                .and_then(|ts| ts.identidade)
        });
        let Some(item) = item else {
            return;
        };
        self.motores_tailscale.parar(perfil);
        // O estado que o nó deixou esperando seria gravado por cima do esquecimento.
        let _ = self.motores_tailscale.tomar_pendente(perfil);
        let resultado = self.cofre.mudar(|c| {
            let Some(mut item) = c.item(item).cloned() else {
                return Ok(0);
            };
            if let Some(identidade) = item.tailscale.as_mut() {
                tailscale::esquecer_maquina(identidade);
            }
            c.salvar(item)
        });
        match resultado {
            Ok(_) => self.avisar(InfoBarSeverity::Success, t!("tailscale-maquina-esquecida")),
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            ),
        }
    }

    /// O item ZeroTier do perfil escolhido. Se o perfil ainda não tem, cria um item vazio no cofre
    /// (o motor gera a identidade na primeira conexão) e liga o perfil a ele (que passa a ser
    /// ZeroTier). `None`: o cofre está trancado ou não gravou.
    fn identidade_zerotier_do_perfil(
        &self,
        campos: &CamposRede,
        escolhido: Signal<Option<String>>,
    ) -> Option<u64> {
        let id = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok())?;
        let base = self.tree.with_untracked(|t| t.perfil(id).cloned())?;
        // O que está nas propriedades e ainda não foi para a árvore também conta.
        let atual = untrack(|| campos.aplicar(&base, &self.cofre));
        let existente = atual.zerotier.as_ref().and_then(|zt| zt.identidade);
        if item_do_tipo(&self.cofre, existente, Tipo::ZeroTier).is_some() {
            return existente;
        }
        let novo = Item {
            tipo: Tipo::ZeroTier,
            titulo: t!(
                "redes-titulo-da-identidade-zerotier",
                nome = atual.name.as_str()
            ),
            zerotier: Some(IdentidadeZeroTier::default()),
            ..Item::default()
        };
        self.ligar_identidade_zerotier(campos, escolhido, novo)
    }

    /// Grava `novo` no cofre e liga o perfil escolhido a ele, como ZeroTier.
    fn ligar_identidade_zerotier(
        &self,
        campos: &CamposRede,
        escolhido: Signal<Option<String>>,
        novo: Item,
    ) -> Option<u64> {
        if self.cofre.agora(|c| c.bloqueado()) {
            self.avisar(InfoBarSeverity::Warning, t!("redes-zerotier-destrave"));
            return None;
        }
        let item = match self.cofre.mudar(|c| c.salvar(novo)) {
            Ok(id) => id,
            Err(erro) => {
                self.avisar(
                    InfoBarSeverity::Error,
                    t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
                );
                return None;
            }
        };
        self.gravar_nebula(campos, escolhido, |perfil| {
            let tinha = perfil.zerotier.is_some();
            perfil.nebula = None;
            perfil.openvpn = None;
            perfil.wireguard = None;
            perfil.tailscale = None;
            let atual = perfil.zerotier.take().unwrap_or_default();
            perfil.zerotier = Some(ZeroTierRede {
                identidade: Some(item),
                ..atual
            });
            if !tinha && perfil.espera_secs < zerotier::ESPERA_PADRAO_SECS {
                perfil.espera_secs = zerotier::ESPERA_PADRAO_SECS;
            }
        });
        Some(item)
    }

    /// "Importar identity.secret…": lê o arquivo de um membro que já existe (para manter o
    /// endereço que o administrador autorizou), guarda num item do cofre e liga o perfil a ele.
    fn importar_identidade_zerotier(
        self: &Rc<Self>,
        campos: CamposRede,
        escolhido: Signal<Option<String>>,
    ) {
        let t = Rc::clone(self);
        dialogs::open_file()
            .remember("redes")
            .title(t!("redes-zerotier-importar"))
            .filter(t!("redes-zerotier-arquivo"), &["secret"])
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let texto = match std::fs::read_to_string(&caminho) {
                    Ok(texto) => texto,
                    Err(erro) => {
                        t.avisar(
                            InfoBarSeverity::Error,
                            t!(
                                "comum-nao-consegui-ler",
                                arquivo = caminho.display().to_string(),
                                erro = erro.to_string()
                            ),
                        );
                        return;
                    }
                };
                let identidade = match zerotier::importar_identidade(&texto) {
                    Ok(identidade) => identidade,
                    Err(erro) => {
                        t.avisar(InfoBarSeverity::Error, erro);
                        return;
                    }
                };
                let endereco = identidade.endereco.clone();
                let nome = t
                    .tree
                    .with_untracked(|tr| {
                        escolhido
                            .get_untracked()
                            .and_then(|k| k.parse::<u64>().ok())
                            .and_then(|id| tr.perfil(id).map(|p| p.name.clone()))
                    })
                    .unwrap_or_default();
                // A mesma identidade num item que já existe: o item fica e leva o arquivo novo.
                let existente = t.cofre.agora(|c| {
                    c.itens()
                        .iter()
                        .find(|i| i.zerotier.as_ref().is_some_and(|z| z.endereco == endereco))
                        .cloned()
                });
                let novo = match existente {
                    Some(mut item) => {
                        item.zerotier = Some(identidade);
                        item
                    }
                    None => Item {
                        tipo: Tipo::ZeroTier,
                        titulo: t!("redes-titulo-da-identidade-zerotier", nome = nome.as_str()),
                        zerotier: Some(identidade),
                        ..Item::default()
                    },
                };
                if t.ligar_identidade_zerotier(&campos, escolhido, novo)
                    .is_some()
                {
                    t.avisar(
                        InfoBarSeverity::Success,
                        t!("redes-zerotier-identidade-importada", endereco = endereco),
                    );
                }
            });
    }

    /// "Esquecer o estado": desliga o nó do perfil e apaga a configuração e os caminhos do item
    /// (a identidade fica), para o próximo uso começar do planet embutido.
    fn esquecer_estado_zerotier(&self, escolhido: Signal<Option<String>>) {
        let Some(perfil) = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok())
        else {
            return;
        };
        let item = self.tree.with_untracked(|t| {
            t.perfil(perfil)
                .and_then(|p| p.zerotier.as_ref())
                .and_then(|zt| zt.identidade)
        });
        let Some(item) = item else {
            return;
        };
        self.motores_zerotier.parar(perfil);
        // O estado que o nó deixou esperando seria gravado por cima do esquecimento.
        let _ = self.motores_zerotier.tomar_pendente(perfil);
        let resultado = self.cofre.mudar(|c| {
            let Some(mut item) = c.item(item).cloned() else {
                return Ok(0);
            };
            if let Some(identidade) = item.zerotier.as_mut() {
                zerotier::esquecer_estado(identidade);
            }
            c.salvar(item)
        });
        match resultado {
            Ok(_) => self.avisar(InfoBarSeverity::Success, t!("zerotier-estado-esquecido")),
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            ),
        }
    }

    /// Cria uma conexão SSH para a máquina `nome` da tailnet (no endereço `host`), saindo pelo
    /// perfil `perfil`.
    fn nova_conexao_na_maquina(&self, perfil: u64, nome: String, host: String) {
        self.tree.update(|t| {
            let mut conexao = Connection::new(nome.as_str(), Kind::Ssh);
            conexao.host = host;
            conexao.rede = Rede::Perfil(perfil);
            t.add_connection(None, conexao);
        });
        self.atualizar();
        self.avisar(
            InfoBarSeverity::Success,
            t!("redes-conexao-criada", nome = nome.as_str()),
        );
    }

    /// Grava o usuário e a senha do diálogo no item OpenVPN do cofre.
    fn guardar_credenciais_ovpn(&self, pedido: PedidoDeCredenciais) {
        let Some(id) = pedido.item.get_untracked() else {
            return;
        };
        let (usuario, senha) = (
            pedido.usuario.get_untracked().trim().to_owned(),
            pedido.senha.get_untracked(),
        );
        pedido.senha.set(String::new());
        let resultado = self.cofre.mudar(|c| {
            let Some(mut item) = c.item(id).cloned() else {
                return Ok(0);
            };
            item.usuario = usuario;
            item.senha = senha;
            c.salvar(item)
        });
        if let Err(erro) = resultado {
            self.avisar(
                InfoBarSeverity::Error,
                t!("comum-nao-consegui-gravar-o-cofre", erro = erro.to_string()),
            );
        }
    }

    /// Apaga o perfil escolhido; quem apontava para ele passa a herdar.
    fn apagar_perfil_escolhido(&self, escolhido: Signal<Option<String>>) {
        let Some(id) = escolhido
            .get_untracked()
            .and_then(|chave| chave.parse::<u64>().ok())
        else {
            return;
        };
        self.tree.update(|t| {
            t.apagar_perfil(id);
        });
        escolhido.set(None);
        self.atualizar();
        self.sincronizar_rede();
    }
}

/// A página Redes, em Ferramentas: a lista dos perfis, as propriedades do escolhido e os
/// botões Novo, Duplicar, Apagar e Testar.
pub(super) fn pagina(tela: &Rc<Tela>) -> impl View + use<> {
    let tree = tela.tree;
    let cofre = tela.cofre.clone();
    let campos = CamposRede::novo();
    let escolhido: Signal<Option<String>> = signal(None);
    let itens: Signal<Vec<TreeItem>> = signal(Vec::new());
    let apagar = signal(false);
    let testando = signal(false);
    let pronto = signal(0u64);
    let testada: Arc<Mutex<Option<Testada>>> = Arc::default();
    // O editor do config.yml e o texto que não passou na conferência (volta na próxima abertura).
    let editando = signal(false);
    let texto_do_config = signal(String::new());
    let rascunho: Rc<RefCell<Option<String>>> = Rc::default();
    let resumo_do_config = signal(String::new());
    // O mesmo para o .ovpn, e o diálogo de usuário e senha da importação.
    let editando_ovpn = signal(false);
    let texto_do_ovpn = signal(String::new());
    let rascunho_ovpn: Rc<RefCell<Option<String>>> = Rc::default();
    let resumo_do_ovpn = signal(String::new());
    let pedido = PedidoDeCredenciais::novo();
    // O .conf do WireGuard e a CA do servidor Tailscale: o editor, o texto que não passou na
    // conferência (volta na próxima abertura) e o resumo na linha.
    let editando_wg = signal(false);
    let texto_do_wg = signal(String::new());
    let rascunho_wg: Rc<RefCell<Option<String>>> = Rc::default();
    let resumo_do_wg = signal(String::new());
    let editando_ca = signal(false);
    let texto_da_ca = signal(String::new());
    let rascunho_ca: Rc<RefCell<Option<String>>> = Rc::default();
    let resumo_da_ca = signal(String::new());
    // A chave de autenticação da Tailscale, o endereço de login e as máquinas da tailnet.
    let pedindo_chave = signal(false);
    let texto_da_chave = signal(String::new());
    let login_url = signal(String::new());
    let aguarda_aprovacao = signal(false);
    let maquinas: Signal<Vec<LinhaMaquina>> = signal(Vec::new());
    // O ZeroTier: o endereço que o administrador precisa autorizar (vazio: nada a autorizar).
    let a_autorizar = signal(String::new());
    effect(move || {
        let origem = campos.vpn_origem.get();
        poe(
            resumo_do_ovpn,
            if origem.is_empty() {
                t!("redes-sem-config")
            } else {
                origem
            },
        );
    });
    effect(move || {
        let origem = campos.origem.get();
        poe(
            resumo_do_config,
            if origem.is_empty() {
                t!("redes-sem-config")
            } else {
                origem
            },
        );
    });

    // A lista acompanha os perfis; se o escolhido sumiu, a escolha também.
    {
        let anterior: RefCell<Vec<(u64, String)>> = RefCell::default();
        effect(move || {
            let atuais: Vec<(u64, String)> =
                tree.with(|t| t.redes.iter().map(|p| (p.id, p.name.clone())).collect());
            untrack(|| {
                if *anterior.borrow() != atuais {
                    itens.set(
                        atuais
                            .iter()
                            .map(|(id, nome)| {
                                TreeItem::leaf(id.to_string(), nome).icon(Symbol::Globe)
                            })
                            .collect(),
                    );
                }
                let existe = escolhido
                    .get_untracked()
                    .and_then(|chave| chave.parse::<u64>().ok())
                    .is_some_and(|id| atuais.iter().any(|(x, _)| *x == id));
                if !existe && escolhido.get_untracked().is_some() {
                    escolhido.set(None);
                }
                *anterior.borrow_mut() = atuais;
            });
        });
    }

    // Trocar a escolha mostra o perfil nas propriedades; mexer nas propriedades grava no perfil.
    // Cada efeito lê os signals do outro sem rastrear, para um não disparar o outro em círculo.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let chave = escolhido.get();
            untrack(|| {
                let perfil = chave
                    .and_then(|k| k.parse::<u64>().ok())
                    .and_then(|id| t.tree.with_untracked(|tr| tr.perfil(id).cloned()));
                campos.carregar(&perfil.unwrap_or_else(|| PerfilRede::new("")), &t.cofre);
            });
        });
        let t = Rc::clone(tela);
        effect(move || t.guardar_perfil(&campos, escolhido));
        // O cofre mudou (outro item, outro título, desbloqueio): refaz as linhas de credencial.
        let t = Rc::clone(tela);
        effect(move || {
            let _ = t.cofre.versao();
            untrack(|| {
                let perfil = escolhido
                    .get_untracked()
                    .and_then(|k| k.parse::<u64>().ok())
                    .and_then(|id| t.tree.with_untracked(|tr| tr.perfil(id).cloned()));
                if let Some(p) = perfil {
                    campos.carregar(&p, &t.cofre);
                }
            });
        });
    }

    // Os lighthouses do config, o resumo da identidade e o estado do motor, só para ler.
    {
        effect(move || {
            let config = campos.config.get();
            poe(campos.lighthouses, nebula::lighthouses(&config).join("; "));
        });
        let t = Rc::clone(tela);
        effect(move || {
            let rotulo = campos.identidade.get();
            let _ = t.cofre.versao();
            untrack(|| {
                let identidade = identidade_do_cofre(&t.cofre, id_do_rotulo(&rotulo));
                let (endereco, grupos, validade) =
                    identidade.map_or_else(Default::default, |(_, i)| {
                        let validade = if i.vence_em == 0 {
                            String::new()
                        } else {
                            recentes::data(i.vence_em)
                        };
                        (i.endereco, i.grupos.join(", "), validade)
                    });
                poe(campos.endereco, endereco);
                poe(campos.grupos, grupos);
                poe(campos.validade, validade);
            });
        });
        let t = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(t.nebula_mudou);
            let chave = escolhido.get();
            let _ = campos.tipo.get();
            untrack(|| {
                let estado = chave
                    .and_then(|k| k.parse::<u64>().ok())
                    .and_then(|id| t.motores.estado(id));
                poe(campos.estado, nebula::descrever_estado(estado.as_ref()));
            });
        });
    }

    // O servidor, as rotas e a validade da OpenVPN, e o estado do motor, só para ler.
    {
        effect(move || {
            let config = campos.vpn_config.get();
            poe(campos.vpn_servidor, openvpn::servidores(&config).join("; "));
            poe(campos.vpn_rotas, openvpn::rotas(&config).join("; "));
        });
        let t = Rc::clone(tela);
        effect(move || {
            let rotulo = campos.vpn_identidade.get();
            let _ = t.cofre.versao();
            untrack(|| {
                let validade = identidade_vpn_do_cofre(&t.cofre, id_do_rotulo(&rotulo))
                    .map(|(_, i)| i.vence_em)
                    .filter(|vence| *vence != 0)
                    .map_or_else(String::new, recentes::data);
                poe(campos.vpn_validade, validade);
            });
        });
        let t = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(t.nebula_mudou);
            let chave = escolhido.get();
            let _ = campos.tipo.get();
            untrack(|| {
                let estado = chave
                    .and_then(|k| k.parse::<u64>().ok())
                    .and_then(|id| t.motores_vpn.estado(id));
                poe(
                    campos.vpn_estado,
                    openvpn::descrever_estado(estado.as_ref()),
                );
            });
        });
    }

    // O WireGuard: servidores, rotas, chave pública, estado e pares, só para ler.
    {
        effect(move || {
            let config = campos.wg_config.get();
            poe(
                campos.wg_servidor,
                wireguard::servidores(&config).join("; "),
            );
            poe(campos.wg_rotas, wireguard::rotas(&config).join("; "));
        });
        effect(move || {
            let origem = campos.wg_origem.get();
            poe(
                resumo_do_wg,
                if origem.is_empty() {
                    t!("redes-sem-config")
                } else {
                    origem
                },
            );
        });
        let t = Rc::clone(tela);
        effect(move || {
            let rotulo = campos.wg_identidade.get();
            let _ = t.cofre.versao();
            untrack(|| {
                let identidade = item_do_tipo(&t.cofre, id_do_rotulo(&rotulo), Tipo::WireGuard)
                    .and_then(|i| i.wireguard);
                poe(
                    campos.wg_chave_publica,
                    identidade
                        .as_ref()
                        .map(|i| i.chave_publica.clone())
                        .unwrap_or_default(),
                );
                poe(
                    campos.wg_enderecos,
                    identidade
                        .map(|i| i.enderecos.join(", "))
                        .unwrap_or_default(),
                );
            });
        });
        let t = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(t.nebula_mudou);
            let chave = escolhido.get();
            let _ = campos.tipo.get();
            untrack(|| {
                let id = chave.and_then(|k| k.parse::<u64>().ok());
                let estado = id.and_then(|id| t.motores_wireguard.estado(id));
                poe(
                    campos.wg_estado,
                    wireguard::descrever_estado(estado.as_ref()),
                );
                let pares = id
                    .map(|id| t.motores_wireguard.pares(id))
                    .unwrap_or_default();
                poe(
                    campos.wg_pares,
                    pares
                        .iter()
                        .map(wireguard::descrever_par)
                        .collect::<Vec<_>>()
                        .join("; "),
                );
            });
        });
    }

    // A Tailscale: a CA, o que o item do cofre sabe da máquina, o estado do nó, o endereço de
    // login e a lista das máquinas da tailnet, só para ler.
    {
        effect(move || {
            let ca = campos.ts_ca.get();
            poe(
                resumo_da_ca,
                if ca.trim().is_empty() {
                    t!("comum-nenhuma")
                } else {
                    t!("redes-ca-definida")
                },
            );
        });
        let t = Rc::clone(tela);
        effect(move || {
            let rotulo = campos.ts_identidade.get();
            let _ = t.cofre.versao();
            untrack(|| {
                let identidade = item_do_tipo(&t.cofre, id_do_rotulo(&rotulo), Tipo::Tailscale)
                    .and_then(|i| i.tailscale);
                let (login, tailnet, enderecos, validade, chave) =
                    identidade.map_or_else(Default::default, |i| {
                        let validade = if i.vence_em == 0 {
                            String::new()
                        } else {
                            recentes::data(i.vence_em)
                        };
                        let chave = if i.chave_autenticacao.is_empty() {
                            t!("comum-nenhuma")
                        } else {
                            t!("redes-chave-definida")
                        };
                        (i.login, i.tailnet, i.enderecos.join(", "), validade, chave)
                    });
                poe(campos.ts_login, login);
                poe(campos.ts_tailnet, tailnet);
                poe(campos.ts_enderecos, enderecos);
                poe(campos.ts_validade, validade);
                poe(campos.ts_chave, chave);
            });
        });
        let t = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(t.nebula_mudou);
            let chave = escolhido.get();
            let _ = campos.tipo.get();
            let id = chave.and_then(|k| k.parse::<u64>().ok());
            // As conexões que já saem por este perfil (e para onde).
            let cadastrados: Vec<String> = id.map_or_else(Vec::new, |id| {
                tree.with(|tr| {
                    tr.connections()
                        .iter()
                        .filter(|c| c.rede == Rede::Perfil(id))
                        .map(|c| c.host.clone())
                        .collect()
                })
            });
            untrack(|| {
                let estado = id.and_then(|id| t.motores_tailscale.estado(id));
                poe(
                    campos.ts_estado,
                    tailscale::descrever_estado(estado.as_ref()),
                );
                poe(
                    login_url,
                    tailscale::url_de_login(estado.as_ref()).unwrap_or_default(),
                );
                let aprovacao = matches!(estado, Some(tomni_ui_tailscale::State::NeedsApproval));
                if aguarda_aprovacao.get_untracked() != aprovacao {
                    aguarda_aprovacao.set(aprovacao);
                }
                let linhas: Vec<LinhaMaquina> = id
                    .map(|id| t.motores_tailscale.maquinas(id))
                    .unwrap_or_default()
                    .iter()
                    .map(|m| {
                        let host = tailscale::host_da_maquina(m);
                        LinhaMaquina {
                            nome: tailscale::nome_da_maquina(m),
                            descricao: tailscale::descrever_maquina(m),
                            cadastrada: host.as_ref().is_some_and(|h| cadastrados.contains(h)),
                            host,
                        }
                    })
                    .collect();
                if maquinas.get_untracked() != linhas {
                    maquinas.set(linhas);
                }
            });
        });
    }

    // O ZeroTier: o que o item do cofre sabe do membro e o estado do nó, só para ler.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let rotulo = campos.zt_identidade.get();
            let _ = t.cofre.versao();
            untrack(|| {
                let identidade = item_do_tipo(&t.cofre, id_do_rotulo(&rotulo), Tipo::ZeroTier)
                    .and_then(|i| i.zerotier);
                let (endereco, nome, enderecos) = identidade.map_or_else(Default::default, |i| {
                    (i.endereco, i.nome_rede, i.enderecos.join(", "))
                });
                poe(campos.zt_endereco, endereco);
                poe(campos.zt_nome_rede, nome);
                poe(campos.zt_enderecos, enderecos);
            });
        });
        let t = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(t.nebula_mudou);
            let chave = escolhido.get();
            let _ = campos.tipo.get();
            let id = chave.and_then(|k| k.parse::<u64>().ok());
            untrack(|| {
                let estado = id.and_then(|id| t.motores_zerotier.estado(id));
                poe(
                    campos.zt_estado,
                    zerotier::descrever_estado(estado.as_ref()),
                );
                poe(
                    a_autorizar,
                    zerotier::endereco_a_autorizar(estado.as_ref()).unwrap_or_default(),
                );
                let detalhes = id
                    .map(|id| t.motores_zerotier.pares(id))
                    .unwrap_or_default()
                    .iter()
                    .map(zerotier::descrever_par)
                    .collect::<Vec<_>>()
                    .join("; ");
                poe(campos.zt_detalhes, detalhes);
            });
        });
    }

    // A thread do "Testar" terminou: mostra o resultado no aviso do alto.
    {
        let (t, testada) = (Rc::clone(tela), Arc::clone(&testada));
        effect(move || {
            let _ = ler_contador(pronto);
            let Some((id, _nome, resultado)) = testada
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
            else {
                return;
            };
            untrack(|| {
                testando.set(false);
                // O motor que o teste ligou desliga se nenhuma aba usa o perfil.
                let em_uso = t.usos.em_uso().iter().any(|p| p.id == id);
                if !em_uso {
                    if t.motores.ligado(id) {
                        t.motores.parar(id);
                    }
                    if t.motores_vpn.ligado(id) {
                        t.motores_vpn.parar(id);
                    }
                    if t.motores_wireguard.ligado(id) {
                        t.motores_wireguard.parar(id);
                    }
                    if t.motores_tailscale.ligado(id) {
                        t.motores_tailscale.parar(id);
                    }
                    if t.motores_zerotier.ligado(id) {
                        t.motores_zerotier.parar(id);
                    }
                }
                match resultado {
                    Ok(mensagem) => t.avisar(InfoBarSeverity::Success, mensagem),
                    Err(erro) => t.avisar(InfoBarSeverity::Error, erro),
                }
            });
        });
    }

    let sem_escolha = move || escolhido.get().is_none();
    let e_nebula = move || campos.tipo.get() == rotulo_tipo_nebula();
    let e_openvpn = move || campos.tipo.get() == rotulo_tipo_openvpn();
    let e_wireguard = move || campos.tipo.get() == rotulo_tipo_wireguard();
    let e_tailscale = move || campos.tipo.get() == rotulo_tipo_tailscale();
    let e_zerotier = move || campos.tipo.get() == rotulo_tipo_zerotier();
    let embutida =
        move || e_nebula() || e_openvpn() || e_wireguard() || e_tailscale() || e_zerotier();
    let sem_proxy = move || embutida() || campos.proxy.get() == rotulo_sem_proxy();
    let sem_salto = move || campos.salto_host.get().trim().is_empty();
    let travado = {
        let cofre = cofre.clone();
        move || cofre.ler(|c| c.bloqueado())
    };

    let testar = {
        let (t, testada) = (Rc::clone(tela), Arc::clone(&testada));
        move || {
            let Some(perfil) = escolhido
                .get_untracked()
                .and_then(|k| k.parse::<u64>().ok())
                .and_then(|id| t.tree.with_untracked(|tr| tr.perfil(id).cloned()))
            else {
                return;
            };
            if perfil.proxy.is_none() && !perfil.e_embutida() {
                t.avisar(
                    InfoBarSeverity::Informational,
                    t!("redes-sem-proxy", nome = perfil.name.as_str()),
                );
                return;
            }
            testando.set(true);
            t.avisar(
                InfoBarSeverity::Informational,
                t!("redes-testando", nome = perfil.name.as_str()),
            );
            let testada = Arc::clone(&testada);
            if perfil.nebula.is_some() {
                // Liga o motor aqui (rápido) e espera a conexão na thread.
                let (id, nome) = (perfil.id, perfil.name.clone());
                let rede = t.resolver_rede(perfil);
                std::thread::spawn(move || {
                    let resultado = rede.preparar(&mut |_| {}).map(|()| {
                        let ip = rede
                            .nebula
                            .as_ref()
                            .and_then(|m| m.estado().address())
                            .map(|a| a.to_string())
                            .unwrap_or_default();
                        t!("redes-nebula-conectou", nome = nome.as_str(), ip = ip)
                    });
                    *testada.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some((id, nome, resultado));
                    acordar(pronto);
                });
                return;
            }
            if perfil.wireguard.is_some() || perfil.tailscale.is_some() || perfil.zerotier.is_some()
            {
                let (id, nome) = (perfil.id, perfil.name.clone());
                let rede = t.resolver_rede(perfil);
                std::thread::spawn(move || {
                    let resultado = rede.preparar(&mut |_| {}).map(|()| {
                        let ip = match rede.motor.as_ref() {
                            Some(MotorRede::WireGuard(m)) => match m.estado() {
                                tomni_ui_wireguard::State::Connected { address, .. } => {
                                    address.to_string()
                                }
                                _ => String::new(),
                            },
                            Some(MotorRede::Tailscale(m)) => match m.estado() {
                                tomni_ui_tailscale::State::Connected { address, .. } => {
                                    address.to_string()
                                }
                                _ => String::new(),
                            },
                            Some(MotorRede::ZeroTier(m)) => match m.estado() {
                                tomni_ui_zerotier::State::Connected { address, .. } => {
                                    address.to_string()
                                }
                                _ => String::new(),
                            },
                            None => String::new(),
                        };
                        t!("redes-nebula-conectou", nome = nome.as_str(), ip = ip)
                    });
                    *testada.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some((id, nome, resultado));
                    acordar(pronto);
                });
                return;
            }
            if perfil.openvpn.is_some() {
                let (id, nome) = (perfil.id, perfil.name.clone());
                let rede = t.resolver_rede(perfil);
                std::thread::spawn(move || {
                    let resultado = rede.preparar(&mut |_| {}).map(|()| {
                        let ip = match rede.openvpn.as_ref().map(|m| m.estado()) {
                            Some(tomni_ui_openvpn::State::Connected { address, .. }) => {
                                address.to_string()
                            }
                            _ => String::new(),
                        };
                        t!("redes-nebula-conectou", nome = nome.as_str(), ip = ip)
                    });
                    *testada.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some((id, nome, resultado));
                    acordar(pronto);
                });
                return;
            }
            std::thread::spawn(move || {
                let resultado = redes::preparar(&perfil, &mut |_| {})
                    .map(|()| t!("redes-responde", nome = perfil.name.as_str()));
                *testada.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some((perfil.id, perfil.name, resultado));
                acordar(pronto);
            });
        }
    };

    // O "Testar" e o "Entrar" da Tailscale são o mesmo gesto: ligar o nó e esperar a conexão.
    let testar = Rc::new(testar);
    let botoes = Row::new()
        .gap(8)
        .child(
            Button::new(t!("comum-novo"))
                .id("rede-nova")
                .icon(Symbol::Add)
                .flyout(TipoNovo::TODOS.iter().fold(
                    MenuFlyout::new().id("rede-nova-menu"),
                    |menu, &(tipo, id, rotulo)| {
                        let t = Rc::clone(tela);
                        menu.item(id, rotulo(), move || t.novo_perfil(escolhido, tipo))
                    },
                )),
        )
        .child(
            Button::new(t!("comum-duplicar"))
                .id("rede-duplicar")
                .icon(Symbol::Copy)
                .disabled(sem_escolha)
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.duplicar_perfil_escolhido(escolhido)
                }),
        )
        .child(
            Button::new(t!("comum-apagar"))
                .id("rede-apagar")
                .icon(Symbol::Delete)
                .disabled(sem_escolha)
                .on_click(move || apagar.set(true)),
        )
        .child(
            Button::new(t!("redes-testar"))
                .id("rede-testar")
                .icon(Symbol::Play)
                .disabled(move || sem_escolha() || testando.get())
                .on_click({
                    let testar = Rc::clone(&testar);
                    move || testar()
                }),
        )
        .child(
            Button::new(t!("redes-importar-config"))
                .id("rede-importar-nebula")
                .icon(Symbol::ArrowUpload)
                .disabled(sem_escolha)
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.importar_config(campos, escolhido)
                }),
        )
        .child(
            Button::new(t!("redes-importar-ovpn"))
                .id("rede-importar-openvpn")
                .icon(Symbol::ArrowUpload)
                .disabled(sem_escolha)
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.importar_ovpn(campos, escolhido, pedido)
                }),
        )
        .child(
            Button::new(t!("redes-importar-wireguard"))
                .id("rede-importar-wireguard")
                .icon(Symbol::ArrowUpload)
                .disabled(sem_escolha)
                .on_click({
                    let t = Rc::clone(tela);
                    move || t.importar_wireguard(campos, escolhido)
                }),
        );

    let arvore = TreeView::bind(itens, escolhido).id("redes-arvore").grow(1);
    let vazia = move || itens.get().is_empty();
    let lista = Column::new()
        .gap(4)
        .padding(Insets::new(0.0, 8.0, 0.0, 0.0))
        .child(
            Text::new(t!("redes-nenhuma-rede-use-novo-para-criar"))
                .id("redes-vazia")
                .secondary()
                .wrap()
                .visible(vazia),
        )
        .child(arvore);

    let propriedades = PropertyGrid::new()
        .id("redes-props")
        .grow(1)
        .category(t!("comum-rede"))
        .text(t!("comum-nome"), move || campos.nome)
        .description(t!("redes-o-nome-do-perfil-nas-listas"))
        .choice(t!("redes-cor"), move || campos.cor, CORES)
        .default(CORES[0])
        .description(t!("redes-a-cor-do-perfil"))
        .choice(
            t!("comum-tipo"),
            move || campos.tipo,
            vec![
                rotulo_tipo_proxy(),
                rotulo_tipo_nebula(),
                rotulo_tipo_openvpn(),
                rotulo_tipo_wireguard(),
                rotulo_tipo_tailscale(),
                rotulo_tipo_zerotier(),
            ],
        )
        .default(rotulo_tipo_proxy())
        .description(t!("redes-por-onde-a-rede-sai"))
        .category(t!("redes-proxy"))
        .hidden(embutida)
        .choice(
            t!("redes-proxy"),
            move || campos.proxy,
            vec![
                rotulo_sem_proxy(),
                TipoProxy::Socks5.nome().to_owned(),
                TipoProxy::Http.nome().to_owned(),
            ],
        )
        .default(rotulo_sem_proxy())
        .description(t!("redes-por-onde-as-conexoes-saem-socks5"))
        .text(t!("comum-host"), move || campos.host)
        .disabled(sem_proxy)
        .default("127.0.0.1")
        .description(t!("redes-o-endereco-do-proxy-quase-sempre"))
        .number(t!("comum-porta"), move || campos.porta, 1.0..=65535.0)
        .disabled(sem_proxy)
        .default(1081)
        .description(t!("redes-a-porta-do-proxy"))
        .choice_from(t!("comum-credencial"), move || campos.credencial, {
            let cofre = cofre.clone();
            move || opcoes_de_credencial_do_perfil(&cofre)
        })
        .disabled({
            let travado = travado.clone();
            move || sem_proxy() || travado()
        })
        .default(sem_credencial())
        .description(t!("redes-o-item-do-cofre-com-o"))
        .category(t!("redes-nebula"))
        .hidden(move || !e_nebula())
        .choice_from(t!("redes-identidade"), move || campos.identidade, {
            let cofre = cofre.clone();
            move || opcoes_de_identidade_do_tipo(&cofre, Tipo::Nebula)
        })
        .disabled(travado.clone())
        .default(sem_credencial())
        .description(t!("redes-o-item-nebula-do-cofre"))
        .text(t!("comum-endereco"), move || campos.endereco)
        .read_only()
        .description(t!("redes-o-endereco-do-no"))
        .text(t!("redes-grupos"), move || campos.grupos)
        .read_only()
        .description(t!("redes-os-grupos-do-certificado"))
        .text(t!("redes-valido-ate"), move || campos.validade)
        .read_only()
        .description(t!("redes-ate-quando-o-certificado-vale"))
        .button(t!("redes-config"), resumo_do_config, {
            let rascunho = Rc::clone(&rascunho);
            move || {
                texto_do_config.set(
                    rascunho
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| campos.config.get_untracked()),
                );
                editando.set(true);
            }
        })
        .description(t!("redes-o-config-sem-os-segredos"))
        .text(t!("redes-lighthouses"), move || campos.lighthouses)
        .read_only()
        .description(t!("redes-os-lighthouses-do-config"))
        .text(t!("redes-estado"), move || campos.estado)
        .read_only()
        .description(t!("redes-como-esta-a-rede-nebula"))
        .category(t!("redes-openvpn"))
        .hidden(move || !e_openvpn())
        .choice_from(
            t!("redes-identidade-openvpn"),
            move || campos.vpn_identidade,
            {
                let cofre = cofre.clone();
                move || opcoes_de_identidade_do_tipo(&cofre, Tipo::OpenVpn)
            },
        )
        .disabled(travado.clone())
        .default(sem_credencial())
        .description(t!("redes-o-item-openvpn-do-cofre"))
        .button(t!("redes-perfil-ovpn"), resumo_do_ovpn, {
            let rascunho = Rc::clone(&rascunho_ovpn);
            move || {
                texto_do_ovpn.set(
                    rascunho
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| campos.vpn_config.get_untracked()),
                );
                editando_ovpn.set(true);
            }
        })
        .description(t!("redes-o-ovpn-sem-os-segredos"))
        .text(t!("openvpn-servidor"), move || campos.vpn_servidor)
        .read_only()
        .description(t!("redes-os-servidores-do-ovpn"))
        .text(t!("redes-certificado-vale-ate"), move || {
            campos.vpn_validade
        })
        .read_only()
        .description(t!("redes-ate-quando-o-certificado-vale"))
        .check(t!("redes-usar-as-rotas-do-servidor"), move || {
            // A caixa diz "usar"; o perfil guarda o contrário (`route-nopull`).
            campos.vpn_usar_rotas
        })
        .description(t!("redes-as-rotas-que-o-servidor-empurra"))
        .text(t!("redes-rotas"), move || campos.vpn_rotas)
        .read_only()
        .description(t!("redes-as-rotas-fixas-do-ovpn"))
        .text(t!("redes-estado-da-vpn"), move || campos.vpn_estado)
        .read_only()
        .description(t!("redes-como-esta-a-rede-openvpn"))
        .category(t!("redes-wireguard"))
        .hidden(move || !e_wireguard())
        .choice_from(
            t!("redes-identidade-wireguard"),
            move || campos.wg_identidade,
            {
                let cofre = cofre.clone();
                move || opcoes_de_identidade_do_tipo(&cofre, Tipo::WireGuard)
            },
        )
        .disabled(travado.clone())
        .default(sem_credencial())
        .description(t!("redes-o-item-wireguard-do-cofre"))
        .text(t!("wireguard-chave-publica"), move || {
            campos.wg_chave_publica
        })
        .read_only()
        .description(t!("redes-a-chave-publica-do-no"))
        .text(t!("wireguard-enderecos"), move || campos.wg_enderecos)
        .read_only()
        .description(t!("redes-o-endereco-do-no-wireguard"))
        .button(t!("redes-perfil-wireguard"), resumo_do_wg, {
            let rascunho = Rc::clone(&rascunho_wg);
            move || {
                texto_do_wg.set(
                    rascunho
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| campos.wg_config.get_untracked()),
                );
                editando_wg.set(true);
            }
        })
        .description(t!("redes-o-conf-sem-os-segredos"))
        .text(t!("redes-servidores"), move || campos.wg_servidor)
        .read_only()
        .description(t!("redes-os-servidores-do-conf"))
        .text(t!("redes-rotas-permitidas"), move || campos.wg_rotas)
        .read_only()
        .description(t!("redes-as-rotas-do-conf"))
        .text(t!("redes-pares"), move || campos.wg_pares)
        .read_only()
        .description(t!("redes-os-pares-do-tunel"))
        .text(t!("redes-estado-do-tunel"), move || campos.wg_estado)
        .read_only()
        .description(t!("redes-como-esta-a-rede-wireguard"))
        .category(t!("redes-tailscale"))
        .hidden(move || !e_tailscale())
        .choice_from(
            t!("redes-identidade-tailscale"),
            move || campos.ts_identidade,
            {
                let cofre = cofre.clone();
                move || opcoes_de_identidade_do_tipo(&cofre, Tipo::Tailscale)
            },
        )
        .disabled(travado.clone())
        .default(sem_credencial())
        .description(t!("redes-o-item-tailscale-do-cofre"))
        .text(t!("redes-servidor-de-controle"), move || campos.ts_servidor)
        .description(t!("redes-o-servidor-de-controle"))
        .text(t!("redes-nome-da-maquina"), move || campos.ts_nome)
        .description(t!("redes-o-nome-na-tailnet"))
        .check(t!("redes-usar-as-sub-redes"), move || campos.ts_sub_redes)
        .description(t!("redes-as-sub-redes-das-outras-maquinas"))
        .check(t!("redes-maquina-efemera"), move || campos.ts_efemero)
        .description(t!("redes-a-maquina-some-quando-sai"))
        .button(t!("redes-ca-do-servidor"), resumo_da_ca, {
            let rascunho = Rc::clone(&rascunho_ca);
            move || {
                texto_da_ca.set(
                    rascunho
                        .borrow_mut()
                        .take()
                        .unwrap_or_else(|| campos.ts_ca.get_untracked()),
                );
                editando_ca.set(true);
            }
        })
        .description(t!("redes-a-ca-do-servidor-de-controle"))
        .text(t!("tailscale-login"), move || campos.ts_login)
        .read_only()
        .description(t!("redes-a-conta-que-entrou"))
        .text(t!("tailscale-tailnet"), move || campos.ts_tailnet)
        .read_only()
        .description(t!("redes-o-dominio-da-tailnet"))
        .text(t!("redes-enderecos-na-tailnet"), move || {
            campos.ts_enderecos
        })
        .read_only()
        .description(t!("redes-o-endereco-do-no-tailscale"))
        .text(t!("redes-chave-vale-ate"), move || campos.ts_validade)
        .read_only()
        .description(t!("redes-ate-quando-a-chave-da-maquina-vale"))
        .text(t!("tailscale-chave-autenticacao"), move || campos.ts_chave)
        .read_only()
        .description(t!("redes-a-chave-de-autenticacao-no-cofre"))
        .text(t!("redes-estado-da-tailnet"), move || campos.ts_estado)
        .read_only()
        .description(t!("redes-como-esta-a-rede-tailscale"))
        .category(t!("redes-zerotier"))
        .hidden(move || !e_zerotier())
        .choice_from(
            t!("redes-identidade-zerotier"),
            move || campos.zt_identidade,
            {
                let cofre = cofre.clone();
                move || opcoes_de_identidade_do_tipo(&cofre, Tipo::ZeroTier)
            },
        )
        .disabled(travado.clone())
        .default(sem_credencial())
        .description(t!("redes-o-item-zerotier-do-cofre"))
        .text(t!("redes-zerotier-id"), move || campos.zt_rede)
        .description(t!("redes-zerotier-id-descricao"))
        .text(t!("redes-zerotier-ip-fixo"), move || campos.zt_ip_fixo)
        .description(t!("redes-zerotier-ip-fixo-descricao"))
        .check(t!("redes-zerotier-globais"), move || campos.zt_globais)
        .description(t!("redes-zerotier-globais-descricao"))
        .check(t!("redes-zerotier-padrao"), move || campos.zt_padrao)
        .description(t!("redes-zerotier-padrao-descricao"))
        .check(t!("redes-zerotier-dns"), move || campos.zt_dns)
        .description(t!("redes-zerotier-dns-descricao"))
        .text(t!("redes-zerotier-moons"), move || campos.zt_moons)
        .description(t!("redes-zerotier-moons-descricao"))
        .text(t!("redes-zerotier-endereco"), move || campos.zt_endereco)
        .read_only()
        .description(t!("redes-zerotier-endereco-descricao"))
        .text(t!("redes-zerotier-nome-rede"), move || campos.zt_nome_rede)
        .read_only()
        .description(t!("redes-zerotier-nome-rede-descricao"))
        .text(t!("redes-zerotier-enderecos"), move || campos.zt_enderecos)
        .read_only()
        .description(t!("redes-zerotier-enderecos-descricao"))
        .text(t!("redes-zerotier-detalhes"), move || campos.zt_detalhes)
        .read_only()
        .description(t!("redes-zerotier-detalhes-descricao"))
        .text(t!("redes-zerotier-estado"), move || campos.zt_estado)
        .read_only()
        .description(t!("redes-zerotier-estado-descricao"))
        .category(t!("redes-salto"))
        .text(t!("redes-host-do-salto"), move || campos.salto_host)
        .description(t!("redes-um-servidor-de-salto-que-vem"))
        .number(
            t!("redes-porta-do-salto"),
            move || campos.salto_porta,
            1.0..=65535.0,
        )
        .disabled(sem_salto)
        .default(22)
        .description(t!("comum-a-porta-do-servidor-de-salto"))
        .text(t!("redes-usuario-do-salto"), move || campos.salto_usuario)
        .disabled(sem_salto)
        .description(t!("redes-o-usuario-no-servidor-de-salto"))
        .choice_from(
            t!("redes-credencial-do-salto"),
            move || campos.salto_credencial,
            {
                let cofre = cofre.clone();
                move || opcoes_de_credencial_do_perfil(&cofre)
            },
        )
        .disabled(move || sem_salto() || travado())
        .default(sem_credencial())
        .description(t!("comum-o-item-do-cofre-com-o"))
        .category(t!("redes-subir-e-descer"))
        .text(t!("redes-comando-para-subir"), move || campos.subir)
        .hidden(embutida)
        .description(t!("redes-roda-quando-o-proxy-nao-responde"))
        .text(t!("redes-comando-para-descer"), move || campos.descer)
        .hidden(embutida)
        .description(t!("redes-roda-quando-a-ultima-aba-que"))
        .number(
            t!("redes-esperar-ate-s"),
            move || campos.espera,
            1.0..=3600.0,
        )
        .default(30)
        .description(t!("redes-quantos-segundos-esperar-o-proxy-responder"))
        .visible(move || escolhido.get().is_some());

    let sem_perfil = Text::new(t!("redes-escolha-uma-rede-na-lista-para"))
        .id("redes-sem-escolha")
        .secondary()
        .wrap()
        .visible(sem_escolha);

    let painel_tailscale = {
        let (t, entrar) = (Rc::clone(tela), Rc::clone(&testar));
        let (t_abrir, t_esquecer) = (Rc::clone(tela), Rc::clone(tela));
        let t_lista = Rc::clone(tela);
        Column::new()
            .id("redes-tailscale-painel")
            .gap(8)
            .visible(move || escolhido.get().is_some() && e_tailscale())
            .child(
                Row::new()
                    .gap(8)
                    .child(
                        Button::new(t!("redes-entrar"))
                            .id("rede-tailscale-entrar")
                            .accent()
                            .disabled(move || testando.get())
                            .on_click(move || {
                                // Sem item no cofre, o "Entrar" cria o da máquina.
                                if t.identidade_tailscale_do_perfil(&campos, escolhido)
                                    .is_some()
                                {
                                    entrar();
                                }
                            }),
                    )
                    .child(
                        Button::new(t!("redes-usar-chave-de-autenticacao"))
                            .id("rede-tailscale-chave")
                            .on_click(move || {
                                texto_da_chave.set(String::new());
                                pedindo_chave.set(true);
                            }),
                    )
                    .child(
                        Button::new(t!("tailscale-esquecer"))
                            .id("rede-tailscale-esquecer")
                            .on_click(move || t_esquecer.esquecer_maquina_tailscale(escolhido)),
                    ),
            )
            .child(
                Row::new()
                    .gap(8)
                    .align(Align::Center)
                    .visible(move || !login_url.get().is_empty())
                    .child(
                        InfoBar::new(InfoBarSeverity::Informational)
                            .id("rede-tailscale-login")
                            .message(move || t!("redes-login-url", url = login_url.get()))
                            .grow(1),
                    )
                    .child(
                        Button::new(t!("comum-copiar"))
                            .id("rede-tailscale-copiar")
                            .icon(Symbol::Copy)
                            .on_click(move || clipboard::write(login_url.get_untracked())),
                    )
                    .child(
                        Button::new(t!("redes-abrir-de-novo"))
                            .id("rede-tailscale-abrir")
                            .on_click(move || {
                                t_abrir
                                    .motores_tailscale
                                    .abrir_login(&login_url.get_untracked());
                            }),
                    ),
            )
            .child(
                InfoBar::new(InfoBarSeverity::Warning)
                    .id("rede-tailscale-aprovacao")
                    .message(t!("redes-aprovacao-pendente"))
                    .visible(move || aguarda_aprovacao.get()),
            )
            .child(Text::new(t!("redes-maquinas-da-tailnet")).id("rede-tailscale-maquinas-titulo"))
            .child(
                Text::new(t!("redes-nenhuma-maquina"))
                    .id("rede-tailscale-sem-maquinas")
                    .secondary()
                    .wrap()
                    .visible(move || maquinas.get().is_empty()),
            )
            .child(
                ScrollViewer::new()
                    .id("rede-tailscale-maquinas")
                    .height(140)
                    .child(For::each(
                        maquinas,
                        LinhaMaquina::chave,
                        move |m: LinhaMaquina| linha_da_maquina(m, &t_lista, escolhido),
                    )),
            )
    };

    let painel_zerotier = {
        let (t_conectar, t_importar, t_esquecer) =
            (Rc::clone(tela), Rc::clone(tela), Rc::clone(tela));
        let conectar = Rc::clone(&testar);
        Column::new()
            .id("redes-zerotier-painel")
            .gap(8)
            .visible(move || escolhido.get().is_some() && e_zerotier())
            .child(
                Row::new()
                    .gap(8)
                    .child(
                        Button::new(t!("redes-zerotier-conectar"))
                            .id("rede-zerotier-conectar")
                            .accent()
                            .disabled(move || testando.get())
                            .on_click(move || {
                                // Sem item no cofre, o "Conectar" cria o da identidade.
                                if t_conectar
                                    .identidade_zerotier_do_perfil(&campos, escolhido)
                                    .is_some()
                                {
                                    conectar();
                                }
                            }),
                    )
                    .child(
                        Button::new(t!("redes-zerotier-importar"))
                            .id("rede-zerotier-importar")
                            .icon(Symbol::ArrowUpload)
                            .on_click(move || {
                                t_importar.importar_identidade_zerotier(campos, escolhido)
                            }),
                    )
                    .child(
                        Button::new(t!("redes-zerotier-copiar-endereco"))
                            .id("rede-zerotier-copiar")
                            .icon(Symbol::Copy)
                            .disabled(move || campos.zt_endereco.get().is_empty())
                            .on_click(move || clipboard::write(campos.zt_endereco.get_untracked())),
                    )
                    .child(
                        Button::new(t!("redes-zerotier-esquecer"))
                            .id("rede-zerotier-esquecer")
                            .on_click(move || t_esquecer.esquecer_estado_zerotier(escolhido)),
                    ),
            )
            .child(
                Row::new()
                    .gap(8)
                    .align(Align::Center)
                    .visible(move || !a_autorizar.get().is_empty())
                    .child(
                        InfoBar::new(InfoBarSeverity::Warning)
                            .id("rede-zerotier-autorizacao")
                            .message(move || {
                                t!(
                                    "redes-zerotier-autorizar",
                                    endereco = a_autorizar.get(),
                                    rede = zerotier::rede_normalizada(&campos.zt_rede.get())
                                )
                            })
                            .grow(1),
                    ),
            )
    };

    let direita = Column::new()
        .grow(1)
        .padding(Insets::new(0.0, 0.0, 0.0, 8.0))
        .child(propriedades)
        .child(painel_tailscale)
        .child(painel_zerotier)
        .child(sem_perfil);

    let dialogo = {
        let t = Rc::clone(tela);
        ContentDialog::new(
            move || {
                let nome = escolhido.get().and_then(|k| {
                    let id = k.parse().ok()?;
                    tree.with(|t| t.perfil(id).map(|p| p.name.clone()))
                });
                t!("redes-apagar-a-rede", nome = nome.unwrap_or_default())
            },
            apagar,
        )
        .id("rede-apagar-dialogo")
        .content(Text::new(t!("redes-os-grupos-e-as-conexoes-que")).wrap())
        .primary(t!("comum-apagar"), move || {
            t.apagar_perfil_escolhido(escolhido)
        })
        .close(t!("comum-cancelar"))
    };

    let editor = {
        let t = Rc::clone(tela);
        let rascunho = Rc::clone(&rascunho);
        ContentDialog::new(t!("redes-editar-config"), editando)
            .id("rede-config-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(t!("redes-config-dica")).secondary().wrap())
                    .child(
                        Edit::bind(texto_do_config)
                            .id("rede-config-texto")
                            .multiline()
                            .height(320)
                            .width(560),
                    ),
            )
            .primary(t!("comum-salvar"), move || {
                let texto = texto_do_config.get_untracked();
                match nebula::conferir_yaml(&texto) {
                    Ok(avisos) => {
                        t.gravar_nebula(&campos, escolhido, |perfil| {
                            let atual = perfil.nebula.take().unwrap_or_default();
                            perfil.nebula = Some(NebulaRede {
                                config: texto,
                                ..atual
                            });
                        });
                        if avisos.is_empty() {
                            t.avisar(InfoBarSeverity::Success, t!("redes-config-salvo"));
                        } else {
                            t.avisar(InfoBarSeverity::Warning, avisos.join("\n"));
                        }
                    }
                    Err(erro) => {
                        *rascunho.borrow_mut() = Some(texto);
                        t.avisar(InfoBarSeverity::Error, erro);
                    }
                }
            })
            .close(t!("comum-cancelar"))
    };

    let editor_ovpn = {
        let t = Rc::clone(tela);
        let rascunho = Rc::clone(&rascunho_ovpn);
        ContentDialog::new(t!("redes-editar-ovpn"), editando_ovpn)
            .id("rede-ovpn-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(t!("redes-ovpn-dica")).secondary().wrap())
                    .child(
                        Edit::bind(texto_do_ovpn)
                            .id("rede-ovpn-texto")
                            .multiline()
                            .height(320)
                            .width(560),
                    ),
            )
            .primary(t!("comum-salvar"), move || {
                let texto = texto_do_ovpn.get_untracked();
                match openvpn::conferir_ovpn(&texto) {
                    Ok(avisos) => {
                        t.gravar_nebula(&campos, escolhido, |perfil| {
                            let atual = perfil.openvpn.take().unwrap_or_default();
                            perfil.openvpn = Some(OpenVpnRede {
                                config: texto,
                                ..atual
                            });
                        });
                        if avisos.is_empty() {
                            t.avisar(InfoBarSeverity::Success, t!("redes-config-salvo"));
                        } else {
                            t.avisar(InfoBarSeverity::Warning, avisos.join("\n"));
                        }
                    }
                    Err(erro) => {
                        *rascunho.borrow_mut() = Some(texto);
                        t.avisar(InfoBarSeverity::Error, erro);
                    }
                }
            })
            .close(t!("comum-cancelar"))
    };

    let credenciais = {
        let t = Rc::clone(tela);
        ContentDialog::new(t!("redes-usuario-e-senha-da-vpn"), pedido.aberto)
            .id("rede-ovpn-credenciais")
            .content(
                Column::new()
                    .gap(8)
                    .child(
                        Text::new(t!("redes-usuario-e-senha-dica"))
                            .secondary()
                            .wrap(),
                    )
                    .child(
                        Edit::bind(pedido.usuario)
                            .id("rede-ovpn-usuario")
                            .placeholder(t!("comum-usuario")),
                    )
                    .child(
                        Edit::bind(pedido.senha)
                            .id("rede-ovpn-senha")
                            .password()
                            .placeholder(t!("comum-senha")),
                    ),
            )
            .primary(t!("comum-salvar"), move || {
                t.guardar_credenciais_ovpn(pedido);
            })
            .close(t!("redes-perguntar-depois"))
    };

    let editor_wg = {
        let t = Rc::clone(tela);
        let rascunho = Rc::clone(&rascunho_wg);
        ContentDialog::new(t!("redes-editar-wireguard"), editando_wg)
            .id("rede-wireguard-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(t!("redes-wireguard-dica")).secondary().wrap())
                    .child(
                        Edit::bind(texto_do_wg)
                            .id("rede-wireguard-texto")
                            .multiline()
                            .height(320)
                            .width(560),
                    ),
            )
            .primary(t!("comum-salvar"), move || {
                let texto = texto_do_wg.get_untracked();
                match wireguard::conferir_conf(&texto) {
                    Ok(()) => {
                        t.gravar_nebula(&campos, escolhido, |perfil| {
                            let atual = perfil.wireguard.take().unwrap_or_default();
                            perfil.wireguard = Some(WireGuardRede {
                                config: texto,
                                ..atual
                            });
                        });
                        t.avisar(InfoBarSeverity::Success, t!("redes-config-salvo"));
                    }
                    Err(erro) => {
                        *rascunho.borrow_mut() = Some(texto);
                        t.avisar(InfoBarSeverity::Error, erro);
                    }
                }
            })
            .close(t!("comum-cancelar"))
    };

    let editor_ca = {
        let t = Rc::clone(tela);
        let rascunho = Rc::clone(&rascunho_ca);
        ContentDialog::new(t!("redes-editar-ca"), editando_ca)
            .id("rede-ca-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(t!("redes-ca-dica")).secondary().wrap())
                    .child(
                        Edit::bind(texto_da_ca)
                            .id("rede-ca-texto")
                            .multiline()
                            .height(240)
                            .width(560),
                    ),
            )
            .primary(t!("comum-salvar"), move || {
                let texto = texto_da_ca.get_untracked().trim().to_owned();
                if !texto.is_empty() && !texto.contains("-----BEGIN CERTIFICATE-----") {
                    *rascunho.borrow_mut() = Some(texto);
                    t.avisar(InfoBarSeverity::Error, t!("redes-ca-invalida"));
                    return;
                }
                t.gravar_nebula(&campos, escolhido, |perfil| {
                    let atual = perfil.tailscale.take().unwrap_or_default();
                    perfil.tailscale = Some(TailscaleRede { ca: texto, ..atual });
                });
                t.avisar(InfoBarSeverity::Success, t!("redes-config-salvo"));
            })
            .close(t!("comum-cancelar"))
    };

    let dialogo_chave = {
        let t = Rc::clone(tela);
        ContentDialog::new(t!("redes-chave-de-autenticacao"), pedindo_chave)
            .id("rede-tailscale-chave-dialogo")
            .content(
                Column::new()
                    .gap(8)
                    .child(Text::new(t!("redes-chave-dica")).secondary().wrap())
                    .child(
                        Edit::bind(texto_da_chave)
                            .id("rede-tailscale-chave-texto")
                            .password()
                            .placeholder(t!("tailscale-chave-autenticacao-dica")),
                    ),
            )
            .primary(t!("comum-salvar"), move || {
                let chave = texto_da_chave.get_untracked();
                texto_da_chave.set(String::new());
                t.guardar_chave_autenticacao(&campos, escolhido, &chave);
            })
            .close(t!("comum-cancelar"))
    };

    let principal = Split::row(lista, direita)
        .id("redes-principal")
        .size(signal(200.0))
        .min_first(140.0)
        .min_second(320.0)
        .grow(1);

    Column::new()
        .padding(16)
        .gap(8)
        .child(Text::new(t!("redes-redes")).title())
        .child(
            Text::new(t!("redes-uma-rede-por-cliente-cada-perfil"))
                .secondary()
                .wrap(),
        )
        .child(botoes)
        .child(principal)
        .child(dialogo)
        .child(editor)
        .child(editor_ovpn)
        .child(editor_wg)
        .child(editor_ca)
        .child(dialogo_chave)
        .child(credenciais)
}
