//! "Abrir em" (M36): escolher o painel de uma conexão e ver o número dos painéis enquanto a
//! pessoa escolhe. Este módulo guarda o que não depende da tela: os ids dos itens do submenu e a
//! leitura do item em realce, que diz qual janela identificar e qual painel destacar. Quem monta
//! o submenu e abre as abas é a `Tela`.

/// Quantos itens "Painel N" o submenu reserva. É o teto de painéis que o menu alcança (a grade
/// 3 × 3 tem 9); numa janela com mais painéis, os de número maior ficam sem item.
pub const PAINEIS: usize = 16;

/// O que o submenu abre: uma conexão ou todas as de uma pasta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlvoAbrir {
    /// A conexão de id `id`.
    Conexao(u64),
    /// As conexões de terminal da pasta de id `id`, em qualquer profundidade.
    Pasta(u64),
}

/// Para onde a abertura vai.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Destino {
    /// O painel de índice `n` (a partir de 0) da janela de destino.
    Painel(usize),
    /// Um painel novo, à direita do ativo, na janela de destino.
    Direita,
    /// Uma janela nova.
    NovaJanela,
    /// A janela de índice `n` da lista de janelas, no painel livre dela.
    Janela(usize),
}

/// Qual janela mostra os crachás.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Onde {
    /// A janela de destino (a última ativa).
    Destino,
    /// A janela de índice `n` da lista de janelas.
    Janela(usize),
}

/// O que identificar: a janela e o painel em destaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identificacao {
    /// A janela que mostra os crachás.
    pub onde: Onde,
    /// O painel destacado (índice a partir de 0), se o item em realce aponta para um.
    pub painel: Option<usize>,
}

/// O que um item do submenu abre, pelo sufixo do id depois de `-abrir-em`.
pub fn destino_do_item(sufixo: &str) -> Option<Destino> {
    match sufixo {
        "-direita" => Some(Destino::Direita),
        "-nova" => Some(Destino::NovaJanela),
        _ => {
            if let Some(k) = sufixo.strip_prefix("-painel-") {
                return k.parse().ok().map(Destino::Painel);
            }
            sufixo
                .strip_prefix("-janela-")?
                .parse()
                .ok()
                .map(Destino::Janela)
        }
    }
}

/// O que identificar para o item de menu em realce (`None`: nada).
///
/// - o item "Abrir em" (o pai), "Novo painel à direita" e "Nova janela": os painéis da janela de
///   destino, sem destaque;
/// - "Painel N": os da janela de destino, com o painel `N - 1` em destaque;
/// - "Janela N" do submenu "Outra janela": os painéis daquela janela, sem destaque;
/// - qualquer outro id (os itens que não abrem: "Renomear", "Apagar"…) ou `None`: nada.
///
/// O id vale com o prefixo que for (`menu-3-abrir-em-painel-2` na árvore,
/// `inicio-recente-0-menu-abrir-em-painel-2` no Início): só o que vem depois do último
/// `-abrir-em` conta, para a leitura não depender do menu de onde veio.
pub fn identificacao(realce: Option<&str>) -> Option<Identificacao> {
    let id = realce?;
    let sufixo = &id[id.rfind("-abrir-em")? + "-abrir-em".len()..];
    if sufixo.is_empty() {
        return Some(Identificacao {
            onde: Onde::Destino,
            painel: None,
        });
    }
    match destino_do_item(sufixo)? {
        Destino::Painel(k) => Some(Identificacao {
            onde: Onde::Destino,
            painel: Some(k),
        }),
        Destino::Direita | Destino::NovaJanela => Some(Identificacao {
            onde: Onde::Destino,
            painel: None,
        }),
        Destino::Janela(j) => Some(Identificacao {
            onde: Onde::Janela(j),
            painel: None,
        }),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn destino() -> Option<Identificacao> {
        Some(Identificacao {
            onde: Onde::Destino,
            painel: None,
        })
    }

    #[test]
    fn sem_realce_ou_fora_do_submenu_nao_identifica_nada() {
        assert_eq!(identificacao(None), None);
        assert_eq!(identificacao(Some("menu-3-apagar")), None);
        assert_eq!(identificacao(Some("menu-3-abrir")), None);
        assert_eq!(identificacao(Some("")), None);
    }

    #[test]
    fn o_item_pai_identifica_a_janela_de_destino_sem_destaque() {
        assert_eq!(identificacao(Some("menu-3-abrir-em")), destino());
    }

    #[test]
    fn novo_painel_e_nova_janela_identificam_a_janela_de_destino() {
        assert_eq!(identificacao(Some("menu-3-abrir-em-direita")), destino());
        assert_eq!(identificacao(Some("menu-3-abrir-em-nova")), destino());
    }

    #[test]
    fn o_painel_k_vem_destacado() {
        assert_eq!(
            identificacao(Some("menu-3-abrir-em-painel-0")),
            Some(Identificacao {
                onde: Onde::Destino,
                painel: Some(0)
            })
        );
        assert_eq!(
            identificacao(Some("menu-3-abrir-em-painel-15")),
            Some(Identificacao {
                onde: Onde::Destino,
                painel: Some(15)
            })
        );
    }

    #[test]
    fn a_janela_j_do_submenu_identifica_aquela_janela() {
        assert_eq!(
            identificacao(Some("menu-3-abrir-em-janela-2")),
            Some(Identificacao {
                onde: Onde::Janela(2),
                painel: None
            })
        );
    }

    #[test]
    fn o_item_que_abre_o_submenu_das_outras_janelas_e_o_de_numero_invalido_nao_identificam() {
        assert_eq!(identificacao(Some("menu-3-abrir-em-outra")), None);
        assert_eq!(identificacao(Some("menu-3-abrir-em-painel-x")), None);
        assert_eq!(identificacao(Some("menu-3-abrir-em-janela-")), None);
    }

    #[test]
    fn o_prefixo_do_menu_nao_importa() {
        let esperado = Some(Identificacao {
            onde: Onde::Destino,
            painel: Some(2),
        });
        assert_eq!(
            identificacao(Some("inicio-recente-0-menu-abrir-em-painel-2")),
            esperado
        );
        assert_eq!(
            identificacao(Some("menu-7-todas-abrir-em-painel-2")),
            esperado
        );
        assert_eq!(
            identificacao(Some("inicio-fixada-1-menu-abrir-em")),
            destino()
        );
    }

    #[test]
    fn os_destinos_dos_itens() {
        assert_eq!(destino_do_item("-painel-4"), Some(Destino::Painel(4)));
        assert_eq!(destino_do_item("-direita"), Some(Destino::Direita));
        assert_eq!(destino_do_item("-nova"), Some(Destino::NovaJanela));
        assert_eq!(destino_do_item("-janela-1"), Some(Destino::Janela(1)));
        assert_eq!(destino_do_item("-outra"), None);
        assert_eq!(destino_do_item(""), None);
    }
}
