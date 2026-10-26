//! O JSON do próprio Tirech Omni: a `Tree` (o mesmo formato do `connections.json`) e, quando a
//! exportação levou as senhas, um bloco `cofre` com os itens ligados às conexões (o `Documento`
//! do `cofre_json`). Os ids de item do cofre que a árvore guarda (`credencial`, `nota`…) só
//! valem junto com esse bloco: sem ele, as ligações saem, para não apontarem para o item errado
//! de outro cofre. Depois de gravar o lote, a tela chama [`Importacao::religar`].

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Entrada, Formato, Importacao, PastaTrazida, PerfilTrazido, Trazida};
use crate::cofre_json;
use crate::modelo::{Connection, Node, Tree};
use crate::redes::{PerfilRede, Rede};
use crate::t;

/// O formato nativo.
pub struct Nativo;

impl Formato for Nativo {
    fn id(&self) -> &'static str {
        "nativo"
    }

    fn nome(&self) -> &'static str {
        "Tirech Omni (JSON)"
    }

    fn arquivo_padrao(&self) -> &'static str {
        "tirech-omni-conexoes.json"
    }

    fn extensoes(&self) -> &'static [&'static str] {
        &["json"]
    }

    fn locais_provaveis(&self, _home: &Path) -> Vec<PathBuf> {
        Vec::new()
    }

    fn analisar(&self, texto: &str, _senha_mestra: Option<&str>) -> Result<Importacao, String> {
        analisar(texto)
    }
}

fn analisar(texto: &str) -> Result<Importacao, String> {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    let valor: Value = serde_json::from_str(texto)
        .map_err(|e| t!("importar-nativo-json", erro = e.to_string()))?;
    if !valor.get("nodes").is_some_and(Value::is_array) {
        return Err(
            if valor.get("formato").and_then(Value::as_str) == Some(cofre_json::FORMATO) {
                t!("importar-nativo-e-cofre")
            } else {
                t!("importar-nativo-nao-e-exportacao")
            },
        );
    }
    let arvore: Tree = serde_json::from_value(valor.clone())
        .map_err(|e| t!("importar-nativo-conexoes", erro = e.to_string()))?;
    if arvore.version > crate::modelo::VERSAO {
        return Err(t!("importar-nativo-versao-nova"));
    }
    let mut importacao = Importacao::default();
    if valor.get("cofre").is_some() {
        importacao.cofre = Some(cofre_json::ler(texto)?);
    }
    let com_cofre = importacao.cofre.is_some();
    andar(
        &arvore,
        &arvore.nodes,
        &mut Vec::new(),
        com_cofre,
        &mut importacao,
    );
    Ok(importacao)
}

/// Um perfil da árvore, solto dela: sem id, com as credenciais do cofre só se o cofre veio.
fn perfil_trazido(arvore: &Tree, id: u64, com_cofre: bool) -> Option<PerfilTrazido> {
    let mut perfil: PerfilRede = arvore.redes.iter().find(|p| p.id == id)?.clone();
    perfil.id = 0;
    if !com_cofre {
        if let Some(proxy) = perfil.proxy.as_mut() {
            proxy.credencial = None;
        }
        if let Some(salto) = perfil.salto.as_mut() {
            salto.credencial = None;
        }
        if let Some(nebula) = perfil.nebula.as_mut() {
            nebula.identidade = None;
        }
        if let Some(vpn) = perfil.openvpn.as_mut() {
            vpn.identidade = None;
        }
        if let Some(wg) = perfil.wireguard.as_mut() {
            wg.identidade = None;
        }
        if let Some(ts) = perfil.tailscale.as_mut() {
            ts.identidade = None;
        }
        if let Some(zt) = perfil.zerotier.as_mut() {
            zt.identidade = None;
        }
    }
    Some(PerfilTrazido {
        perfil,
        usuario: String::new(),
        senha: None,
    })
}

/// Sem o bloco `cofre`, os ids de itens dele não valem.
fn soltar_do_cofre(c: &mut Connection) {
    c.credencial = None;
    c.nota = None;
    c.chave_do_cofre = None;
    if let Some(salto) = c.jump.as_mut() {
        salto.credencial = None;
    }
}

fn andar(
    arvore: &Tree,
    nos: &[Node],
    caminho: &mut Vec<String>,
    com_cofre: bool,
    importacao: &mut Importacao,
) {
    for no in nos {
        match no {
            Node::Group(g) => {
                caminho.push(g.name.clone());
                let (rede, perfil) = match g.rede {
                    Rede::Perfil(id) => match perfil_trazido(arvore, id, com_cofre) {
                        Some(p) => (Rede::Herdar, Some(p)),
                        None => (Rede::Herdar, None),
                    },
                    outra => (outra, None),
                };
                importacao.entradas.push(Entrada::Pasta(PastaTrazida {
                    caminho: caminho.clone(),
                    rede,
                    perfil,
                    comandos: g.comandos.clone(),
                    persistente: g.persistente,
                }));
                andar(arvore, &g.children, caminho, com_cofre, importacao);
                caminho.pop();
            }
            Node::Connection(c) => {
                let mut c = c.clone();
                c.id = 0;
                if !com_cofre {
                    soltar_do_cofre(&mut c);
                }
                let mut perfil = None;
                if let Rede::Perfil(id) = c.rede {
                    c.rede = Rede::Herdar;
                    perfil = perfil_trazido(arvore, id, com_cofre);
                }
                let mut trazida = Trazida::nova(c, caminho.clone());
                trazida.perfil = perfil;
                importacao.conexao(trazida);
            }
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::cofre::{Item, Lote, Tipo};
    use crate::modelo::Kind;
    use crate::redes::{ProxyRede, TipoProxy};

    /// Uma árvore com um grupo de rede própria, uma conexão ligada ao cofre e outra no perfil.
    fn arvore() -> Tree {
        let mut t = Tree::default();
        let mut perfil = PerfilRede::new("Cliente A");
        perfil.proxy = Some(ProxyRede {
            tipo: TipoProxy::Socks5,
            host: "10.0.0.9".into(),
            port: 1080,
            credencial: Some(7),
        });
        perfil.id = t.novo_id();
        let id_perfil = perfil.id;
        t.redes.push(perfil);
        let grupo = t.add_group(None, "Clientes");
        let mut c = Connection::new("Web", Kind::Ssh);
        c.host = "web.exemplo.com".into();
        c.credencial = Some(7);
        c.nota = Some(8);
        t.add_connection(Some(grupo), c);
        let mut d = Connection::new("Banco", Kind::Ssh);
        d.rede = Rede::Perfil(id_perfil);
        t.add_connection(None, d);
        t
    }

    fn documento() -> Value {
        let lote = Lote {
            pastas: Vec::new(),
            itens: vec![Item {
                id: 7,
                tipo: Tipo::Ssh,
                titulo: "Web".into(),
                ..Item::default()
            }],
        };
        serde_json::to_value(cofre_json::Documento {
            formato: cofre_json::FORMATO.to_owned(),
            versao: cofre_json::VERSAO,
            lote,
        })
        .unwrap()
    }

    #[test]
    fn so_a_arvore_solta_as_ligacoes_do_cofre() {
        let texto = serde_json::to_string(&arvore()).unwrap();
        let i = Nativo.analisar(&texto, None).unwrap();
        assert!(i.cofre.is_none());
        assert_eq!(i.total(), 2);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].pasta, ["Clientes"]);
        assert_eq!(c[0].conexao.credencial, None);
        assert_eq!(c[0].conexao.nota, None);
        assert_eq!(c[0].conexao.id, 0);
        assert!(c[1].pasta.is_empty());
        let perfil = c[1].perfil.as_ref().unwrap();
        assert_eq!(perfil.perfil.name, "Cliente A");
        assert_eq!(perfil.perfil.id, 0);
        assert_eq!(perfil.perfil.proxy.as_ref().unwrap().credencial, None);
        assert_eq!(c[1].conexao.rede, Rede::Herdar);
    }

    #[test]
    fn o_provedor_de_ia_da_conexao_vai_junto() {
        let mut t = arvore();
        let id = t.connections()[0].id;
        t.connection_mut(id).unwrap().perfil_ia = Some(3);
        let texto = serde_json::to_string(&t).unwrap();
        let i = Nativo.analisar(&texto, None).unwrap();
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.perfil_ia, Some(3));
        assert_eq!(c[1].conexao.perfil_ia, None);
    }

    #[test]
    fn com_o_bloco_cofre_mantem_os_ids_para_religar() {
        let mut valor = serde_json::to_value(arvore()).unwrap();
        valor["cofre"] = documento();
        let i = Nativo.analisar(&valor.to_string(), None).unwrap();
        assert_eq!(i.cofre.as_ref().unwrap().itens.len(), 1);
        let c: Vec<_> = i.conexoes().collect();
        assert_eq!(c[0].conexao.credencial, Some(7));
        assert_eq!(c[0].conexao.nota, Some(8));
        assert_eq!(
            c[1].perfil
                .as_ref()
                .unwrap()
                .perfil
                .proxy
                .as_ref()
                .unwrap()
                .credencial,
            Some(7)
        );
        assert_eq!(i.senhas(), 1);
    }

    #[test]
    fn outros_json_viram_erro() {
        assert!(
            Nativo
                .analisar("{}", None)
                .unwrap_err()
                .contains("não é uma exportação")
        );
        assert!(Nativo.analisar("[1,2", None).is_err());
        let cofre = documento().to_string();
        assert!(
            Nativo
                .analisar(&cofre, None)
                .unwrap_err()
                .contains("página Cofre")
        );
    }

    #[test]
    fn a_identidade_nebula_do_perfil_so_vale_com_o_cofre_junto() {
        let mut t = arvore();
        let id = t.redes[0].id;
        t.redes[0].proxy = None;
        t.redes[0].nebula = Some(crate::redes::NebulaRede {
            config: "cipher: aes\n".into(),
            identidade: Some(7),
            origem: String::new(),
        });
        let mut d = Connection::new("Rede", Kind::Ssh);
        d.rede = Rede::Perfil(id);
        t.add_connection(None, d);
        let sem_cofre = Nativo
            .analisar(&serde_json::to_string(&t).unwrap(), None)
            .unwrap();
        let perfil = sem_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.nebula.as_ref().unwrap().identidade, None);

        let mut valor = serde_json::to_value(&t).unwrap();
        valor["cofre"] = documento();
        let mut com_cofre = Nativo.analisar(&valor.to_string(), None).unwrap();
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.nebula.as_ref().unwrap().identidade, Some(7));
        let mut mapa = crate::cofre::Mapa::default();
        mapa.itens.insert(7, 70);
        com_cofre.religar(&mapa);
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.nebula.as_ref().unwrap().identidade, Some(70));
    }

    #[test]
    fn a_identidade_openvpn_do_perfil_so_vale_com_o_cofre_junto() {
        let mut t = arvore();
        let id = t.redes[0].id;
        t.redes[0].proxy = None;
        t.redes[0].openvpn = Some(crate::redes::OpenVpnRede {
            config: "client\n".into(),
            identidade: Some(7),
            ..Default::default()
        });
        let mut d = Connection::new("Rede", Kind::Ssh);
        d.rede = Rede::Perfil(id);
        t.add_connection(None, d);
        let sem_cofre = Nativo
            .analisar(&serde_json::to_string(&t).unwrap(), None)
            .unwrap();
        let perfil = sem_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.openvpn.as_ref().unwrap().identidade, None);

        let mut valor = serde_json::to_value(&t).unwrap();
        valor["cofre"] = documento();
        let mut com_cofre = Nativo.analisar(&valor.to_string(), None).unwrap();
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.openvpn.as_ref().unwrap().identidade, Some(7));
        let mut mapa = crate::cofre::Mapa::default();
        mapa.itens.insert(7, 70);
        com_cofre.religar(&mapa);
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.openvpn.as_ref().unwrap().identidade, Some(70));
    }

    #[test]
    fn a_identidade_wireguard_do_perfil_so_vale_com_o_cofre_junto() {
        let mut t = arvore();
        let id = t.redes[0].id;
        t.redes[0].proxy = None;
        t.redes[0].wireguard = Some(crate::redes::WireGuardRede {
            config: "[Interface]\n".into(),
            identidade: Some(7),
            ..Default::default()
        });
        let mut d = Connection::new("Rede", Kind::Ssh);
        d.rede = Rede::Perfil(id);
        t.add_connection(None, d);
        let sem_cofre = Nativo
            .analisar(&serde_json::to_string(&t).unwrap(), None)
            .unwrap();
        let perfil = sem_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.wireguard.as_ref().unwrap().identidade, None);

        let mut valor = serde_json::to_value(&t).unwrap();
        valor["cofre"] = documento();
        let mut com_cofre = Nativo.analisar(&valor.to_string(), None).unwrap();
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(
            perfil.perfil.wireguard.as_ref().unwrap().identidade,
            Some(7)
        );
        let mut mapa = crate::cofre::Mapa::default();
        mapa.itens.insert(7, 70);
        com_cofre.religar(&mapa);
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(
            perfil.perfil.wireguard.as_ref().unwrap().identidade,
            Some(70)
        );
    }

    #[test]
    fn a_identidade_tailscale_do_perfil_so_vale_com_o_cofre_junto() {
        let mut t = arvore();
        let id = t.redes[0].id;
        t.redes[0].proxy = None;
        t.redes[0].tailscale = Some(crate::redes::TailscaleRede {
            identidade: Some(7),
            ..Default::default()
        });
        let mut d = Connection::new("Rede", Kind::Ssh);
        d.rede = Rede::Perfil(id);
        t.add_connection(None, d);
        let sem_cofre = Nativo
            .analisar(&serde_json::to_string(&t).unwrap(), None)
            .unwrap();
        let perfil = sem_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(perfil.perfil.tailscale.as_ref().unwrap().identidade, None);

        let mut valor = serde_json::to_value(&t).unwrap();
        valor["cofre"] = documento();
        let mut com_cofre = Nativo.analisar(&valor.to_string(), None).unwrap();
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(
            perfil.perfil.tailscale.as_ref().unwrap().identidade,
            Some(7)
        );
        let mut mapa = crate::cofre::Mapa::default();
        mapa.itens.insert(7, 70);
        com_cofre.religar(&mapa);
        let perfil = com_cofre
            .conexoes()
            .find_map(|c| c.perfil.as_ref())
            .unwrap();
        assert_eq!(
            perfil.perfil.tailscale.as_ref().unwrap().identidade,
            Some(70)
        );
    }
}
