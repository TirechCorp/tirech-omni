//! O JSON do Bitwarden (Ferramentas ▸ Exportar cofre ▸ `.json`, sem cifrar): importa e exporta.
//! A exportação é em claro, com as senhas: é o que a pessoa pediu.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use tomni_ui::terminal::ssh::keys::KeyFile;

use crate::cofre::{
    CAMPO_NEBULA_CA, CAMPO_NEBULA_CERTIFICADO, CAMPO_NEBULA_CHAVE, CAMPO_OPENVPN_CA,
    CAMPO_OPENVPN_CERTIFICADO, CAMPO_OPENVPN_CHAVE, CAMPO_OPENVPN_CHAVE_TLS, CAMPO_OPENVPN_DIRECAO,
    CAMPO_OPENVPN_MODO_TLS, CAMPO_OPENVPN_SERVIDOR, CAMPO_TAILSCALE_CHAVE_AUTENTICACAO,
    CAMPO_TAILSCALE_CHAVE_MAQUINA, CAMPO_TAILSCALE_CHAVE_NO, CAMPO_TAILSCALE_ENDERECOS,
    CAMPO_TAILSCALE_LOGIN, CAMPO_TAILSCALE_NOME, CAMPO_TAILSCALE_SERVIDOR, CAMPO_TAILSCALE_TAILNET,
    CAMPO_TAILSCALE_VENCE_EM, CAMPO_TIPO, CAMPO_WIREGUARD_CHAVE, CAMPO_WIREGUARD_ENDERECOS,
    CAMPO_WIREGUARD_PSK, CAMPO_WIREGUARD_PUBLICA, CAMPO_ZEROTIER_ENDERECO,
    CAMPO_ZEROTIER_ENDERECOS, CAMPO_ZEROTIER_ESTADO, CAMPO_ZEROTIER_IDENTIDADE,
    CAMPO_ZEROTIER_NOME_REDE, Campo, ChaveSsh, Cofre, IdentidadeOpenVpn, IdentidadeTailscale,
    IdentidadeWireGuard, IdentidadeZeroTier, Item, Lote, Tipo,
};
use crate::nebula;
use crate::openvpn;
use crate::t;
use crate::tailscale;
use crate::wireguard;
use crate::zerotier;

/// Um `null` vira o valor padrão (o `#[serde(default)]` só cobre o campo que falta).
fn ou_padrao<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Arquivo {
    encrypted: Option<bool>,
    #[serde(deserialize_with = "ou_padrao")]
    folders: Vec<PastaBw>,
    #[serde(deserialize_with = "ou_padrao")]
    items: Vec<ItemBw>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PastaBw {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ItemBw {
    folder_id: Option<String>,
    #[serde(rename = "type")]
    tipo: i64,
    name: Option<String>,
    notes: Option<String>,
    #[serde(deserialize_with = "ou_padrao")]
    fields: Vec<CampoBw>,
    login: Option<LoginBw>,
    card: Option<CartaoBw>,
    identity: Option<IdentidadeBw>,
    ssh_key: Option<ChaveSshBw>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CampoBw {
    name: Option<String>,
    value: Option<String>,
    #[serde(rename = "type")]
    tipo: i64,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct LoginBw {
    #[serde(deserialize_with = "ou_padrao")]
    uris: Vec<UriBw>,
    username: Option<String>,
    password: Option<String>,
    totp: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct UriBw {
    uri: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct CartaoBw {
    cardholder_name: Option<String>,
    brand: Option<String>,
    number: Option<String>,
    exp_month: Option<String>,
    exp_year: Option<String>,
    code: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct IdentidadeBw {
    title: Option<String>,
    first_name: Option<String>,
    middle_name: Option<String>,
    last_name: Option<String>,
    address1: Option<String>,
    address2: Option<String>,
    address3: Option<String>,
    city: Option<String>,
    state: Option<String>,
    postal_code: Option<String>,
    country: Option<String>,
    company: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    ssn: Option<String>,
    username: Option<String>,
    passport_number: Option<String>,
    license_number: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ChaveSshBw {
    private_key: Option<String>,
    public_key: Option<String>,
    key_fingerprint: Option<String>,
}

/// Os itens de uma lista separada por vírgulas, sem os vazios.
fn lista_de(texto: Option<&str>) -> Vec<String> {
    texto
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Soma o campo `nome` à lista, se o valor veio preenchido.
fn campo(campos: &mut Vec<Campo>, nome: &str, valor: Option<&String>, oculto: bool) {
    if let Some(valor) = valor.filter(|v| !v.is_empty()) {
        campos.push(Campo {
            nome: nome.to_owned(),
            valor: valor.clone(),
            oculto,
        });
    }
}

/// Importa o JSON do Bitwarden. Devolve o lote e os avisos (o que ficou de fora).
pub fn importar(texto: &str) -> Result<(Lote, Vec<String>), String> {
    let arquivo: Arquivo = serde_json::from_str(texto.strip_prefix('\u{feff}').unwrap_or(texto))
        .map_err(|e| t!("bitwarden-nao-consegui-ler", erro = e.to_string()))?;
    if arquivo.encrypted == Some(true) {
        return Err(t!("bitwarden-cifrado"));
    }
    let mut lote = Lote::default();
    let mut pastas: HashMap<String, u64> = HashMap::new();
    for pasta in &arquivo.folders {
        if let (Some(id), Some(nome)) = (&pasta.id, &pasta.name)
            && let Some(no_lote) = lote.caminho(nome)
        {
            pastas.insert(id.clone(), no_lote);
        }
    }
    let mut desconhecidos = 0usize;
    let mut vinculados = false;
    for bw in arquivo.items {
        let mut item = Item {
            titulo: bw.name.unwrap_or_default(),
            notas: bw.notes.unwrap_or_default(),
            pasta: bw.folder_id.and_then(|id| pastas.get(&id).copied()),
            ..Item::default()
        };
        match bw.tipo {
            1 => {
                item.tipo = Tipo::Senha;
                if let Some(login) = bw.login {
                    item.usuario = login.username.unwrap_or_default();
                    item.senha = login.password.unwrap_or_default();
                    let mut uris = login.uris.into_iter().filter_map(|u| u.uri);
                    item.url = uris.next().unwrap_or_default();
                    for (i, uri) in uris.enumerate() {
                        item.campos.push(Campo {
                            nome: t!("bitwarden-url", n = (i + 2) as i64),
                            valor: uri,
                            oculto: false,
                        });
                    }
                    campo(&mut item.campos, "TOTP", login.totp.as_ref(), true);
                }
            }
            2 => item.tipo = Tipo::Nota,
            3 => {
                item.tipo = Tipo::Nota;
                if let Some(c) = bw.card {
                    campo(
                        &mut item.campos,
                        "Titular",
                        c.cardholder_name.as_ref(),
                        false,
                    );
                    campo(
                        &mut item.campos,
                        &t!("bitwarden-bandeira"),
                        c.brand.as_ref(),
                        false,
                    );
                    campo(
                        &mut item.campos,
                        &t!("bitwarden-numero"),
                        c.number.as_ref(),
                        true,
                    );
                    let mes = c.exp_month.filter(|m| !m.is_empty());
                    let ano = c.exp_year.filter(|a| !a.is_empty());
                    let validade = match (mes, ano) {
                        (Some(m), Some(a)) => Some(format!("{m:0>2}/{a}")),
                        (Some(m), None) => Some(m),
                        (None, Some(a)) => Some(a),
                        (None, None) => None,
                    };
                    campo(
                        &mut item.campos,
                        &t!("bitwarden-validade"),
                        validade.as_ref(),
                        false,
                    );
                    campo(
                        &mut item.campos,
                        &t!("bitwarden-codigo"),
                        c.code.as_ref(),
                        true,
                    );
                }
            }
            4 => {
                item.tipo = Tipo::Nota;
                if let Some(i) = bw.identity {
                    item.usuario = i.username.unwrap_or_default();
                    let c = &mut item.campos;
                    campo(c, &t!("bitwarden-tratamento"), i.title.as_ref(), false);
                    campo(c, &t!("bitwarden-nome"), i.first_name.as_ref(), false);
                    campo(
                        c,
                        &t!("bitwarden-nome-do-meio"),
                        i.middle_name.as_ref(),
                        false,
                    );
                    campo(c, &t!("bitwarden-sobrenome"), i.last_name.as_ref(), false);
                    campo(c, &t!("bitwarden-empresa"), i.company.as_ref(), false);
                    campo(c, &t!("bitwarden-email"), i.email.as_ref(), false);
                    campo(c, &t!("bitwarden-telefone"), i.phone.as_ref(), false);
                    campo(c, &t!("bitwarden-endereco"), i.address1.as_ref(), false);
                    campo(c, &t!("bitwarden-endereco-2"), i.address2.as_ref(), false);
                    campo(c, &t!("bitwarden-endereco-3"), i.address3.as_ref(), false);
                    campo(c, &t!("bitwarden-cidade"), i.city.as_ref(), false);
                    campo(c, &t!("bitwarden-estado"), i.state.as_ref(), false);
                    campo(c, &t!("bitwarden-cep"), i.postal_code.as_ref(), false);
                    campo(c, &t!("bitwarden-pais"), i.country.as_ref(), false);
                    campo(c, &t!("bitwarden-ssn"), i.ssn.as_ref(), false);
                    campo(
                        c,
                        &t!("bitwarden-passaporte"),
                        i.passport_number.as_ref(),
                        false,
                    );
                    campo(
                        c,
                        &t!("bitwarden-carteira"),
                        i.license_number.as_ref(),
                        false,
                    );
                }
            }
            5 => {
                // Não é `Tipo::Chave`: esse é a passphrase de uma chave em arquivo. A que o app
                // lê (OpenSSH, PKCS#8, PEM) vira `Tipo::ChaveSsh`; o resto fica como nota.
                item.tipo = Tipo::Nota;
                if let Some(k) = bw.ssh_key {
                    let aberta = k
                        .private_key
                        .as_deref()
                        .and_then(|texto| KeyFile::from_text(texto, None).ok())
                        .and_then(|arquivo| ChaveSsh::de_chave(arquivo.private_key()).ok());
                    if let Some(chave) = aberta {
                        item.tipo = Tipo::ChaveSsh;
                        item.chave = Some(chave);
                    } else {
                        campo(
                            &mut item.campos,
                            &t!("bitwarden-chave-privada"),
                            k.private_key.as_ref(),
                            true,
                        );
                        campo(
                            &mut item.campos,
                            &t!("bitwarden-chave-publica"),
                            k.public_key.as_ref(),
                            false,
                        );
                        campo(
                            &mut item.campos,
                            &t!("bitwarden-impressao-digital"),
                            k.key_fingerprint.as_ref(),
                            false,
                        );
                    }
                }
            }
            _ => {
                desconhecidos += 1;
                continue;
            }
        }
        // A identidade Nebula (CA, certificado e chave) que o app pôs em campos da nota.
        let mut nebula: [Option<String>; 3] = Default::default();
        // A identidade OpenVPN: CA, certificado, chave, chave TLS, modo, direção e servidor.
        let mut vpn: [Option<String>; 7] = Default::default();
        // A identidade WireGuard: chave privada, chave pública, endereços e as chaves pré-compartilhadas.
        let mut wg: [Option<String>; 3] = Default::default();
        let mut wg_psks: Vec<(String, String)> = Vec::new();
        // A identidade Tailscale: servidor, chaves, validade, chave de autenticação, login, tailnet,
        // nome e endereços.
        let mut ts: [Option<String>; 9] = Default::default();
        // A identidade ZeroTier: identity.secret, estado, endereço, nome da rede e endereços.
        let mut zt: [Option<String>; 5] = Default::default();
        for extra in bw.fields {
            let nome = extra.name.unwrap_or_default();
            let valor = extra.value.unwrap_or_default();
            match extra.tipo {
                3 => vinculados = true,
                _ if nome == CAMPO_TIPO => {
                    if let Some(tipo) = Tipo::do_id(valor.trim())
                        && (tipo != Tipo::ChaveSsh || item.chave.is_some())
                        && !matches!(
                            tipo,
                            Tipo::Nebula
                                | Tipo::OpenVpn
                                | Tipo::WireGuard
                                | Tipo::Tailscale
                                | Tipo::ZeroTier
                        )
                    {
                        item.tipo = tipo;
                    }
                }
                _ if nome == CAMPO_NEBULA_CA => nebula[0] = Some(valor),
                _ if nome == CAMPO_NEBULA_CERTIFICADO => nebula[1] = Some(valor),
                _ if nome == CAMPO_NEBULA_CHAVE => nebula[2] = Some(valor),
                _ if nome == CAMPO_OPENVPN_CA => vpn[0] = Some(valor),
                _ if nome == CAMPO_OPENVPN_CERTIFICADO => vpn[1] = Some(valor),
                _ if nome == CAMPO_OPENVPN_CHAVE => vpn[2] = Some(valor),
                _ if nome == CAMPO_OPENVPN_CHAVE_TLS => vpn[3] = Some(valor),
                _ if nome == CAMPO_OPENVPN_MODO_TLS => vpn[4] = Some(valor),
                _ if nome == CAMPO_OPENVPN_DIRECAO => vpn[5] = Some(valor),
                _ if nome == CAMPO_OPENVPN_SERVIDOR => vpn[6] = Some(valor),
                _ if nome == CAMPO_WIREGUARD_CHAVE => wg[0] = Some(valor),
                _ if nome == CAMPO_WIREGUARD_PUBLICA => wg[1] = Some(valor),
                _ if nome == CAMPO_WIREGUARD_ENDERECOS => wg[2] = Some(valor),
                _ if nome.starts_with(CAMPO_WIREGUARD_PSK) => {
                    wg_psks.push((nome[CAMPO_WIREGUARD_PSK.len()..].to_owned(), valor));
                }
                _ if nome == CAMPO_TAILSCALE_SERVIDOR => ts[0] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_CHAVE_MAQUINA => ts[1] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_CHAVE_NO => ts[2] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_VENCE_EM => ts[3] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_CHAVE_AUTENTICACAO => ts[4] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_LOGIN => ts[5] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_TAILNET => ts[6] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_NOME => ts[7] = Some(valor),
                _ if nome == CAMPO_TAILSCALE_ENDERECOS => ts[8] = Some(valor),
                _ if nome == CAMPO_ZEROTIER_IDENTIDADE => zt[0] = Some(valor),
                _ if nome == CAMPO_ZEROTIER_ESTADO => zt[1] = Some(valor),
                _ if nome == CAMPO_ZEROTIER_ENDERECO => zt[2] = Some(valor),
                _ if nome == CAMPO_ZEROTIER_NOME_REDE => zt[3] = Some(valor),
                _ if nome == CAMPO_ZEROTIER_ENDERECOS => zt[4] = Some(valor),
                2 => item.campos.push(Campo {
                    nome,
                    valor: (valor == "true").to_string(),
                    oculto: false,
                }),
                tipo => item.campos.push(Campo {
                    nome,
                    valor,
                    oculto: tipo == 1,
                }),
            }
        }
        if let [Some(ca), Some(certificado), Some(chave)] = nebula {
            match nebula::identidade_de_pem(&ca, &certificado, &chave) {
                Ok(identidade) => {
                    item.tipo = Tipo::Nebula;
                    item.nebula = Some(identidade);
                }
                // Não formou uma identidade: o que veio fica na nota, como campos.
                Err(_) => {
                    for (nome, valor, oculto) in [
                        (CAMPO_NEBULA_CA, ca, false),
                        (CAMPO_NEBULA_CERTIFICADO, certificado, false),
                        (CAMPO_NEBULA_CHAVE, chave, true),
                    ] {
                        item.campos.push(Campo {
                            nome: nome.to_owned(),
                            valor,
                            oculto,
                        });
                    }
                }
            }
        }
        if let [Some(ca), Some(certificado), Some(chave), ..] = &vpn {
            let mut identidade = IdentidadeOpenVpn {
                ca: ca.clone(),
                certificado: certificado.clone(),
                chave: chave.clone(),
                chave_tls: vpn[3].clone().unwrap_or_default(),
                modo_tls: vpn[4].clone().unwrap_or_default(),
                key_direction: vpn[5].as_deref().and_then(|d| d.trim().parse().ok()),
                servidor: vpn[6].clone().unwrap_or_default(),
                ..IdentidadeOpenVpn::default()
            };
            match openvpn::completar(&mut identidade) {
                Ok(()) => {
                    item.tipo = Tipo::OpenVpn;
                    item.openvpn = Some(identidade);
                }
                // Não formou uma identidade: o que veio fica na nota, como campos.
                Err(_) => {
                    let nomes = [
                        (CAMPO_OPENVPN_CA, false),
                        (CAMPO_OPENVPN_CERTIFICADO, false),
                        (CAMPO_OPENVPN_CHAVE, true),
                        (CAMPO_OPENVPN_CHAVE_TLS, true),
                        (CAMPO_OPENVPN_MODO_TLS, false),
                        (CAMPO_OPENVPN_DIRECAO, false),
                        (CAMPO_OPENVPN_SERVIDOR, false),
                    ];
                    for ((nome, oculto), valor) in nomes.into_iter().zip(vpn) {
                        if let Some(valor) = valor {
                            item.campos.push(Campo {
                                nome: nome.to_owned(),
                                valor,
                                oculto,
                            });
                        }
                    }
                }
            }
        }
        if let [Some(chave), publica, enderecos] = &wg {
            let mut identidade = IdentidadeWireGuard {
                chave_privada: chave.clone(),
                chave_publica: publica.clone().unwrap_or_default(),
                psks: wg_psks.clone(),
                enderecos: lista_de(enderecos.as_deref()),
            };
            match wireguard::completar(&mut identidade) {
                Ok(()) => {
                    item.tipo = Tipo::WireGuard;
                    item.wireguard = Some(identidade);
                }
                // Não formou uma identidade: o que veio fica na nota, como campos.
                Err(_) => {
                    let nomes = [
                        (CAMPO_WIREGUARD_CHAVE, true),
                        (CAMPO_WIREGUARD_PUBLICA, false),
                        (CAMPO_WIREGUARD_ENDERECOS, false),
                    ];
                    for ((nome, oculto), valor) in nomes.into_iter().zip(wg) {
                        if let Some(valor) = valor {
                            item.campos.push(Campo {
                                nome: nome.to_owned(),
                                valor,
                                oculto,
                            });
                        }
                    }
                    for (par, valor) in wg_psks {
                        item.campos.push(Campo {
                            nome: format!("{CAMPO_WIREGUARD_PSK}{par}"),
                            valor,
                            oculto: true,
                        });
                    }
                }
            }
        }
        if ts.iter().any(Option::is_some) {
            let texto = |i: usize| ts[i].clone().unwrap_or_default();
            let identidade = IdentidadeTailscale {
                servidor: texto(0),
                chave_maquina: texto(1),
                chave_no: texto(2),
                vence_em: ts[3]
                    .as_deref()
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0),
                chave_autenticacao: texto(4),
                login: texto(5),
                tailnet: texto(6),
                nome: texto(7),
                enderecos: lista_de(ts[8].as_deref()),
            };
            match tailscale::completar(&identidade) {
                Ok(()) => {
                    item.tipo = Tipo::Tailscale;
                    item.tailscale = Some(identidade);
                }
                // Não formou uma identidade: o que veio fica na nota, como campos.
                Err(_) => {
                    let nomes = [
                        (CAMPO_TAILSCALE_SERVIDOR, false),
                        (CAMPO_TAILSCALE_CHAVE_MAQUINA, true),
                        (CAMPO_TAILSCALE_CHAVE_NO, true),
                        (CAMPO_TAILSCALE_VENCE_EM, false),
                        (CAMPO_TAILSCALE_CHAVE_AUTENTICACAO, true),
                        (CAMPO_TAILSCALE_LOGIN, false),
                        (CAMPO_TAILSCALE_TAILNET, false),
                        (CAMPO_TAILSCALE_NOME, false),
                        (CAMPO_TAILSCALE_ENDERECOS, false),
                    ];
                    for ((nome, oculto), valor) in nomes.into_iter().zip(ts) {
                        if let Some(valor) = valor {
                            item.campos.push(Campo {
                                nome: nome.to_owned(),
                                valor,
                                oculto,
                            });
                        }
                    }
                }
            }
        }
        if zt.iter().any(Option::is_some) {
            let texto = |i: usize| zt[i].clone().unwrap_or_default();
            let mut identidade = IdentidadeZeroTier {
                identidade: texto(0),
                estado: texto(1),
                endereco: texto(2),
                nome_rede: texto(3),
                enderecos: lista_de(zt[4].as_deref()),
            };
            match zerotier::completar(&mut identidade) {
                Ok(()) => {
                    item.tipo = Tipo::ZeroTier;
                    item.zerotier = Some(identidade);
                }
                // Não formou uma identidade: o que veio fica na nota, como campos.
                Err(_) => {
                    let nomes = [
                        (CAMPO_ZEROTIER_IDENTIDADE, true),
                        (CAMPO_ZEROTIER_ESTADO, false),
                        (CAMPO_ZEROTIER_ENDERECO, false),
                        (CAMPO_ZEROTIER_NOME_REDE, false),
                        (CAMPO_ZEROTIER_ENDERECOS, false),
                    ];
                    for ((nome, oculto), valor) in nomes.into_iter().zip(zt) {
                        if let Some(valor) = valor {
                            item.campos.push(Campo {
                                nome: nome.to_owned(),
                                valor,
                                oculto,
                            });
                        }
                    }
                }
            }
        }
        lote.itens.push(item);
    }
    let mut avisos = Vec::new();
    match desconhecidos {
        0 => {}
        n => avisos.push(t!("bitwarden-desconhecidos", n = n as i64)),
    }
    if vinculados {
        avisos.push(t!("bitwarden-vinculados"));
    }
    Ok((lote, avisos))
}

/// O UUID (só no formato) de uma pasta.
fn uuid_da_pasta(id: u64) -> String {
    format!("00000000-0000-4000-8000-{id:012x}")
}

/// O UUID (só no formato) de um item.
fn uuid_do_item(id: u64) -> String {
    format!("00000000-0000-4000-9000-{id:012x}")
}

/// Exporta o cofre inteiro no formato do Bitwarden, em claro e com as senhas. As pastas viram o
/// caminho completo (é assim que o Bitwarden aninha) e o tipo do item vai no campo
/// `tirech-ssh.tipo`, para a ida e volta não perdê-lo.
pub fn exportar(cofre: &Cofre) -> String {
    let pastas: Vec<Value> = cofre
        .pastas()
        .iter()
        .map(|p| json!({ "id": uuid_da_pasta(p.id), "name": cofre.caminho(Some(p.id)) }))
        .collect();
    let itens: Vec<Value> = cofre
        .itens()
        .iter()
        .map(|item| {
            let mut campos = vec![json!({
                "name": CAMPO_TIPO,
                "value": item.tipo.id(),
                "type": 0,
                "linkedId": null,
            })];
            if let (Tipo::Nebula, Some(n)) = (item.tipo, &item.nebula) {
                for (nome, valor, oculto) in [
                    (CAMPO_NEBULA_CA, &n.ca, 0),
                    (CAMPO_NEBULA_CERTIFICADO, &n.certificado, 0),
                    (CAMPO_NEBULA_CHAVE, &n.chave, 1),
                ] {
                    campos.push(json!({
                        "name": nome,
                        "value": valor,
                        "type": oculto,
                        "linkedId": null,
                    }));
                }
            }
            if let (Tipo::OpenVpn, Some(o)) = (item.tipo, &item.openvpn) {
                let direcao = o.key_direction.map(|d| d.to_string()).unwrap_or_default();
                for (nome, valor, oculto) in [
                    (CAMPO_OPENVPN_CA, &o.ca, 0),
                    (CAMPO_OPENVPN_CERTIFICADO, &o.certificado, 0),
                    (CAMPO_OPENVPN_CHAVE, &o.chave, 1),
                    (CAMPO_OPENVPN_CHAVE_TLS, &o.chave_tls, 1),
                    (CAMPO_OPENVPN_MODO_TLS, &o.modo_tls, 0),
                    (CAMPO_OPENVPN_DIRECAO, &direcao, 0),
                    (CAMPO_OPENVPN_SERVIDOR, &o.servidor, 0),
                ] {
                    if valor.is_empty() {
                        continue;
                    }
                    campos.push(json!({
                        "name": nome,
                        "value": valor,
                        "type": oculto,
                        "linkedId": null,
                    }));
                }
            }
            if let (Tipo::WireGuard, Some(w)) = (item.tipo, &item.wireguard) {
                let enderecos = w.enderecos.join(", ");
                let mut linhas = vec![
                    (CAMPO_WIREGUARD_CHAVE.to_owned(), &w.chave_privada, 1),
                    (CAMPO_WIREGUARD_PUBLICA.to_owned(), &w.chave_publica, 0),
                    (CAMPO_WIREGUARD_ENDERECOS.to_owned(), &enderecos, 0),
                ];
                for (par, psk) in &w.psks {
                    linhas.push((format!("{CAMPO_WIREGUARD_PSK}{par}"), psk, 1));
                }
                for (nome, valor, oculto) in linhas {
                    if valor.is_empty() {
                        continue;
                    }
                    campos.push(json!({
                        "name": nome,
                        "value": valor,
                        "type": oculto,
                        "linkedId": null,
                    }));
                }
            }
            if let (Tipo::Tailscale, Some(t)) = (item.tipo, &item.tailscale) {
                let vence_em = if t.vence_em > 0 {
                    t.vence_em.to_string()
                } else {
                    String::new()
                };
                let enderecos = t.enderecos.join(", ");
                for (nome, valor, oculto) in [
                    (CAMPO_TAILSCALE_SERVIDOR, &t.servidor, 0),
                    (CAMPO_TAILSCALE_CHAVE_MAQUINA, &t.chave_maquina, 1),
                    (CAMPO_TAILSCALE_CHAVE_NO, &t.chave_no, 1),
                    (CAMPO_TAILSCALE_VENCE_EM, &vence_em, 0),
                    (CAMPO_TAILSCALE_CHAVE_AUTENTICACAO, &t.chave_autenticacao, 1),
                    (CAMPO_TAILSCALE_LOGIN, &t.login, 0),
                    (CAMPO_TAILSCALE_TAILNET, &t.tailnet, 0),
                    (CAMPO_TAILSCALE_NOME, &t.nome, 0),
                    (CAMPO_TAILSCALE_ENDERECOS, &enderecos, 0),
                ] {
                    if valor.is_empty() {
                        continue;
                    }
                    campos.push(json!({
                        "name": nome,
                        "value": valor,
                        "type": oculto,
                        "linkedId": null,
                    }));
                }
            }
            if let (Tipo::ZeroTier, Some(z)) = (item.tipo, &item.zerotier) {
                let enderecos = z.enderecos.join(", ");
                for (nome, valor, oculto) in [
                    (CAMPO_ZEROTIER_IDENTIDADE, &z.identidade, 1),
                    (CAMPO_ZEROTIER_ESTADO, &z.estado, 0),
                    (CAMPO_ZEROTIER_ENDERECO, &z.endereco, 0),
                    (CAMPO_ZEROTIER_NOME_REDE, &z.nome_rede, 0),
                    (CAMPO_ZEROTIER_ENDERECOS, &enderecos, 0),
                ] {
                    if valor.is_empty() {
                        continue;
                    }
                    campos.push(json!({
                        "name": nome,
                        "value": valor,
                        "type": oculto,
                        "linkedId": null,
                    }));
                }
            }
            campos.extend(item.campos.iter().map(|c| {
                json!({
                    "name": c.nome,
                    "value": c.valor,
                    "type": i32::from(c.oculto),
                    "linkedId": null,
                })
            }));
            let mut json = json!({
                "id": uuid_do_item(item.id),
                "organizationId": null,
                "folderId": item.pasta.map(uuid_da_pasta),
                "reprompt": 0,
                "name": item.titulo,
                "notes": (!item.notas.is_empty()).then_some(&item.notas),
                "favorite": false,
                "fields": campos,
                "collectionIds": null,
            });
            if let (Tipo::ChaveSsh, Some(chave)) = (item.tipo, &item.chave) {
                json["type"] = json!(5);
                json["sshKey"] = json!({
                    "privateKey": chave.privada,
                    "publicKey": chave.publica,
                    "keyFingerprint": chave.fingerprint,
                });
            } else if matches!(item.tipo, Tipo::Nota | Tipo::Nebula) {
                json["type"] = json!(2);
                json["secureNote"] = json!({ "type": 0 });
            } else {
                let uris: Vec<Value> = if item.url.is_empty() {
                    Vec::new()
                } else {
                    vec![json!({ "match": null, "uri": item.url })]
                };
                json["type"] = json!(1);
                json["login"] = json!({
                    "uris": uris,
                    "username": item.usuario,
                    "password": item.senha,
                    "totp": null,
                });
            }
            json
        })
        .collect();
    let documento = json!({ "encrypted": false, "folders": pastas, "items": itens });
    serde_json::to_string_pretty(&documento).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    fn caminho(lote: &Lote, pasta: Option<u64>) -> String {
        let mut nomes = Vec::new();
        let mut atual = pasta;
        while let Some(p) = atual.and_then(|id| lote.pastas.iter().find(|p| p.id == id)) {
            nomes.push(p.nome.as_str());
            atual = p.pai;
        }
        nomes.reverse();
        nomes.join("/")
    }

    fn um_item(json: &str) -> Item {
        let texto = format!(r#"{{"encrypted":false,"folders":[],"items":[{json}]}}"#);
        let (lote, avisos) = importar(&texto).unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        assert_eq!(lote.itens.len(), 1);
        lote.itens.into_iter().next().unwrap()
    }

    fn valor<'a>(item: &'a Item, nome: &str) -> &'a Campo {
        item.campos
            .iter()
            .find(|c| c.nome == nome)
            .unwrap_or_else(|| panic!("falta o campo {nome}"))
    }

    #[test]
    fn login_com_varias_urls_totp_e_campos() {
        let item = um_item(
            r#"{"type":1,"name":"Painel","notes":"anotação","favorite":true,"reprompt":1,
                "fields":[{"name":"região","value":"sa-east-1","type":0},
                          {"name":"pin","value":"1234","type":1},
                          {"name":"ativo","value":"true","type":2}],
                "login":{"uris":[{"match":null,"uri":"https://a"},{"uri":"https://b"},
                                 {"uri":"https://c"}],
                         "username":"ana","password":"pw","totp":"JBSWY3DP"}}"#,
        );
        assert_eq!(item.tipo, Tipo::Senha);
        assert_eq!(
            (
                item.titulo.as_str(),
                item.usuario.as_str(),
                item.senha.as_str()
            ),
            ("Painel", "ana", "pw")
        );
        assert_eq!(
            (item.url.as_str(), item.notas.as_str()),
            ("https://a", "anotação")
        );
        assert_eq!(valor(&item, "URL 2").valor, "https://b");
        assert_eq!(valor(&item, "URL 3").valor, "https://c");
        assert!(valor(&item, "TOTP").oculto);
        assert!(!valor(&item, "região").oculto);
        assert!(valor(&item, "pin").oculto);
        assert_eq!(valor(&item, "ativo").valor, "true");
        assert!(!valor(&item, "ativo").oculto);
    }

    #[test]
    fn nota_segura_e_campos_nulos() {
        let item = um_item(
            r#"{"type":2,"name":"Lembrete","notes":null,"fields":null,"folderId":null,
                "secureNote":{"type":0},"login":null}"#,
        );
        assert_eq!(
            (item.tipo, item.titulo.as_str(), item.notas.as_str()),
            (Tipo::Nota, "Lembrete", "")
        );
        assert!(item.campos.is_empty());
        assert_eq!(item.pasta, None);
    }

    #[test]
    fn cartao_vira_nota_com_campos_e_validade() {
        let item = um_item(
            r#"{"type":3,"name":"Cartão","card":{"cardholderName":"Ana","brand":"Visa",
                "number":"4111","expMonth":"3","expYear":"2030","code":"123"}}"#,
        );
        assert_eq!(item.tipo, Tipo::Nota);
        assert_eq!(valor(&item, "Titular").valor, "Ana");
        assert_eq!(valor(&item, "Bandeira").valor, "Visa");
        assert!(valor(&item, "Número").oculto);
        assert_eq!(valor(&item, "Validade").valor, "03/2030");
        assert!(valor(&item, "Código").oculto);
    }

    #[test]
    fn identidade_vira_nota_e_so_traz_o_preenchido() {
        let item = um_item(
            r#"{"type":4,"name":"Eu","identity":{"username":"ana","firstName":"Ana",
                "lastName":"Silva","address1":"Rua A","city":"São Paulo","postalCode":"01000-000",
                "country":"BR","email":"a@b.c","phone":"","licenseNumber":"123"}}"#,
        );
        assert_eq!((item.tipo, item.usuario.as_str()), (Tipo::Nota, "ana"));
        assert_eq!(valor(&item, "Nome").valor, "Ana");
        assert_eq!(valor(&item, "Sobrenome").valor, "Silva");
        assert_eq!(valor(&item, "Endereço").valor, "Rua A");
        assert_eq!(valor(&item, "CEP").valor, "01000-000");
        assert_eq!(valor(&item, "Carteira de motorista").valor, "123");
        assert!(item.campos.iter().all(|c| c.nome != "Telefone"));
    }

    #[test]
    fn chave_ssh_vira_nota_e_nao_passphrase() {
        let item = um_item(
            r#"{"type":5,"name":"Minha chave","sshKey":{"privateKey":"-----BEGIN-----",
                "publicKey":"ssh-ed25519 AAAA","keyFingerprint":"SHA256:abc"}}"#,
        );
        assert_eq!(item.tipo, Tipo::Nota);
        assert!(valor(&item, "Chave privada").oculto);
        assert!(!valor(&item, "Chave pública").oculto);
        assert_eq!(valor(&item, "Impressão digital").valor, "SHA256:abc");
    }

    #[test]
    fn chave_ssh_do_bitwarden_vira_chave_ssh_do_cofre_e_volta() {
        use tomni_ui::terminal::ssh::keys::{KeyGen, KeyType};
        let gerada = KeyGen::new(KeyType::Ed25519)
            .comment("ana@teste")
            .generate()
            .unwrap();
        let privada = gerada.private_openssh().to_string();
        let texto = serde_json::json!({
            "encrypted": false, "folders": [],
            "items": [{
                "type": 5, "name": "github",
                "sshKey": {
                    "privateKey": privada,
                    "publicKey": gerada.public_openssh(),
                    "keyFingerprint": gerada.fingerprint(),
                },
            }],
        })
        .to_string();
        let (lote, avisos) = importar(&texto).unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        let item = &lote.itens[0];
        assert_eq!(item.tipo, Tipo::ChaveSsh);
        let chave = item.chave.as_ref().unwrap();
        assert_eq!(chave.fingerprint, gerada.fingerprint());
        assert!(chave.privada.contains("BEGIN OPENSSH PRIVATE KEY"));
        assert!(chave.no_agente);
        assert!(item.campos.is_empty());

        // De volta: sai como item SSH (type 5) do Bitwarden, e o tipo do Tirech não estraga.
        let pasta = PastaTemporaria::nova("bw-chave-ssh");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre.salvar(item.clone()).unwrap();
        let exportado: Value = serde_json::from_str(&exportar(&cofre)).unwrap();
        assert_eq!(exportado["items"][0]["type"], 5);
        assert_eq!(
            exportado["items"][0]["sshKey"]["keyFingerprint"],
            gerada.fingerprint()
        );
        let (volta, _) = importar(&exportado.to_string()).unwrap();
        assert_eq!(volta.itens[0].tipo, Tipo::ChaveSsh);
        assert_eq!(volta.itens[0].chave, item.chave);
    }

    #[test]
    fn campo_de_tipo_nao_cria_chave_ssh_sem_chave() {
        let item = um_item(
            r#"{"type":2,"name":"x","fields":[{"name":"tirech-ssh.tipo","value":"ChaveSsh","type":0}]}"#,
        );
        assert_eq!(item.tipo, Tipo::Nota);
    }

    #[test]
    fn tipo_desconhecido_e_campo_vinculado_dao_aviso() {
        let texto = r#"{"folders":[],"items":[
            {"type":9,"name":"Futuro"},{"type":9,"name":"Outro"},
            {"type":1,"name":"Login","fields":[{"name":"a","type":3,"linkedId":100},
                                               {"name":"b","type":3,"linkedId":101}],
             "login":{"username":"u"}}]}"#;
        let (lote, avisos) = importar(texto).unwrap();
        assert_eq!(lote.itens.len(), 1);
        assert!(lote.itens[0].campos.is_empty());
        assert_eq!(
            avisos,
            [
                "2 itens de tipo desconhecido ficaram de fora.",
                "Campos vinculados do Bitwarden foram ignorados."
            ]
        );
    }

    #[test]
    fn cifrado_e_recusado() {
        let erro = importar(r#"{"encrypted":true,"items":[]}"#).unwrap_err();
        assert_eq!(
            erro,
            "Esse JSON do Bitwarden está cifrado. Exporte de novo no formato .json (sem a opção cifrada)."
        );
    }

    #[test]
    fn pastas_aninham_pelo_nome_e_item_acha_a_sua() {
        let texto = r#"{"folders":[{"id":"f1","name":"Clientes/ACME"},{"id":"f2","name":"Banco"}],
            "items":[{"type":2,"name":"a","folderId":"f1"},
                     {"type":2,"name":"b","folderId":"f2"},
                     {"type":2,"name":"c","folderId":"nao-existe"},
                     {"type":2,"name":"d"}]}"#;
        let (lote, _) = importar(texto).unwrap();
        assert_eq!(lote.pastas.len(), 3);
        assert_eq!(caminho(&lote, lote.itens[0].pasta), "Clientes/ACME");
        assert_eq!(caminho(&lote, lote.itens[1].pasta), "Banco");
        assert_eq!(lote.itens[2].pasta, None);
        assert_eq!(lote.itens[3].pasta, None);
    }

    #[test]
    fn o_campo_de_tipo_troca_o_tipo_e_nao_vira_campo() {
        let item = um_item(
            r#"{"type":1,"name":"web","fields":[{"name":"tirech-ssh.tipo","value":"Ssh","type":0}],
                "login":{"username":"u"}}"#,
        );
        assert_eq!(item.tipo, Tipo::Ssh);
        assert!(item.campos.is_empty());
    }

    #[test]
    fn json_invalido_e_erro() {
        assert!(importar("{ não é json").is_err());
    }

    #[test]
    fn exportar_e_importar_devolvem_o_mesmo_cofre() {
        let pasta = PastaTemporaria::nova("bitwarden");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        let acme = cofre.garantir_caminho(None, &["Clientes", "ACME"]);
        cofre
            .salvar(Item {
                tipo: Tipo::Ssh,
                titulo: "web-01".into(),
                pasta: acme,
                usuario: "root".into(),
                senha: "se\"gredo".into(),
                url: "web-01.acme:22".into(),
                notas: "linha 1\nlinha 2".into(),
                campos: vec![
                    Campo {
                        nome: "região".into(),
                        valor: "sa-east-1".into(),
                        oculto: false,
                    },
                    Campo {
                        nome: "token".into(),
                        valor: "t".into(),
                        oculto: true,
                    },
                ],
                ..Item::default()
            })
            .unwrap();
        cofre
            .salvar(Item {
                tipo: Tipo::Nota,
                titulo: "lembrete".into(),
                notas: "texto".into(),
                ..Item::default()
            })
            .unwrap();
        let texto = exportar(&cofre);
        let valor: Value = serde_json::from_str(&texto).unwrap();
        assert_eq!(valor["encrypted"], false);
        assert_eq!(valor["folders"][1]["name"], "Clientes/ACME");
        let itens = valor["items"].as_array().unwrap();
        assert_eq!(itens[0]["type"], 1);
        assert_eq!(itens[0]["id"].as_str().unwrap().len(), 36);
        assert_eq!(itens[0]["folderId"].as_str().unwrap().len(), 36);
        assert_eq!(itens[1]["type"], 2);
        assert!(itens[1]["login"].is_null());
        assert!(itens[0]["notes"].is_string());

        let (lote, avisos) = importar(&texto).unwrap();
        assert!(avisos.is_empty());
        let web = &lote.itens[0];
        assert_eq!(web.tipo, Tipo::Ssh);
        assert_eq!(caminho(&lote, web.pasta), "Clientes/ACME");
        assert_eq!(
            (web.usuario.as_str(), web.senha.as_str()),
            ("root", "se\"gredo")
        );
        assert_eq!(web.campos, cofre.itens()[0].campos);
        assert_eq!(web.notas, "linha 1\nlinha 2");
        assert_eq!(lote.itens[1].tipo, Tipo::Nota);
        assert_eq!(lote.itens[1].pasta, None);
    }

    fn identidade_de_teste() -> crate::cofre::IdentidadeNebula {
        crate::nebula::identidade_de_pem(
            include_str!("../tests/dados/nebula/ca-25519.crt"),
            include_str!("../tests/dados/nebula/no-v2.crt"),
            include_str!("../tests/dados/nebula/no-v2.key"),
        )
        .expect("a fixture vale")
    }

    #[test]
    fn a_identidade_nebula_vai_e_volta_em_campos_da_nota() {
        let identidade = identidade_de_teste();
        let pasta = PastaTemporaria::nova("bw-nebula");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre
            .salvar(Item {
                tipo: Tipo::Nebula,
                titulo: "Cliente A".into(),
                nebula: Some(identidade.clone()),
                ..Item::default()
            })
            .unwrap();
        let exportado: Value = serde_json::from_str(&exportar(&cofre)).unwrap();
        assert_eq!(exportado["items"][0]["type"], 2);
        let campos = exportado["items"][0]["fields"].as_array().unwrap();
        let chave = campos
            .iter()
            .find(|c| c["name"] == CAMPO_NEBULA_CHAVE)
            .unwrap();
        assert_eq!(chave["type"], 1, "a chave privada sai oculta");

        let (volta, avisos) = importar(&exportado.to_string()).unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        let item = &volta.itens[0];
        assert_eq!(item.tipo, Tipo::Nebula);
        assert_eq!(item.nebula.as_ref(), Some(&identidade));
        assert!(item.campos.is_empty(), "os campos viraram a identidade");
    }

    #[test]
    fn campos_nebula_que_nao_formam_identidade_ficam_na_nota() {
        let item = um_item(
            r#"{"type":2,"name":"x","fields":[
                {"name":"tirech-ssh.tipo","value":"Nebula","type":0},
                {"name":"tirech-ssh.nebula.ca","value":"lixo","type":0},
                {"name":"tirech-ssh.nebula.certificado","value":"lixo","type":0},
                {"name":"tirech-ssh.nebula.chave","value":"lixo","type":1}]}"#,
        );
        assert_eq!(item.tipo, Tipo::Nota);
        assert!(item.nebula.is_none());
        assert!(valor(&item, CAMPO_NEBULA_CHAVE).oculto);
    }

    fn identidade_openvpn_de_teste() -> IdentidadeOpenVpn {
        let agora = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_900_000_000);
        crate::openvpn::importar(
            include_str!("../tests/dados/openvpn/tudo-embutido.ovpn"),
            None,
            agora,
        )
        .expect("a fixture vale")
        .identidade
    }

    #[test]
    fn a_identidade_openvpn_vai_e_volta_em_campos_com_o_login() {
        let identidade = identidade_openvpn_de_teste();
        let pasta = PastaTemporaria::nova("bw-openvpn");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre
            .salvar(Item {
                tipo: Tipo::OpenVpn,
                titulo: "Cliente B".into(),
                usuario: "ana".into(),
                senha: "segredo".into(),
                openvpn: Some(identidade.clone()),
                ..Item::default()
            })
            .unwrap();
        let exportado: Value = serde_json::from_str(&exportar(&cofre)).unwrap();
        assert_eq!(
            exportado["items"][0]["type"], 1,
            "o usuário e a senha ficam no login"
        );
        let campos = exportado["items"][0]["fields"].as_array().unwrap();
        for oculto in [CAMPO_OPENVPN_CHAVE, CAMPO_OPENVPN_CHAVE_TLS] {
            let campo = campos.iter().find(|c| c["name"] == oculto).unwrap();
            assert_eq!(campo["type"], 1, "{oculto} sai oculto");
        }

        let (volta, avisos) = importar(&exportado.to_string()).unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        let item = &volta.itens[0];
        assert_eq!(item.tipo, Tipo::OpenVpn);
        assert_eq!(item.openvpn.as_ref(), Some(&identidade));
        assert_eq!(
            (item.usuario.as_str(), item.senha.as_str()),
            ("ana", "segredo")
        );
        assert!(item.campos.is_empty(), "os campos viraram a identidade");
    }

    #[test]
    fn campos_openvpn_que_nao_formam_identidade_ficam_nos_campos() {
        let item = um_item(
            r#"{"type":1,"name":"x","login":{},"fields":[
                {"name":"tirech-ssh.tipo","value":"OpenVPN","type":0},
                {"name":"tirech-ssh.openvpn.ca","value":"lixo","type":0},
                {"name":"tirech-ssh.openvpn.certificado","value":"lixo","type":0},
                {"name":"tirech-ssh.openvpn.chave","value":"lixo","type":1}]}"#,
        );
        assert_ne!(item.tipo, Tipo::OpenVpn);
        assert!(item.openvpn.is_none());
        assert!(valor(&item, CAMPO_OPENVPN_CHAVE).oculto);
    }

    #[test]
    fn a_identidade_wireguard_vai_e_volta_em_campos_com_o_login() {
        let identidade =
            crate::wireguard::importar(include_str!("../tests/dados/wireguard/cliente.conf"))
                .expect("a fixture vale")
                .identidade;
        assert!(!identidade.psks.is_empty(), "a fixture tem PresharedKey");
        let pasta = PastaTemporaria::nova("bw-wireguard");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre
            .salvar(Item {
                tipo: Tipo::WireGuard,
                titulo: "Cliente W".into(),
                usuario: "ana".into(),
                senha: "segredo".into(),
                wireguard: Some(identidade.clone()),
                ..Item::default()
            })
            .unwrap();
        let exportado: Value = serde_json::from_str(&exportar(&cofre)).unwrap();
        let campos = exportado["items"][0]["fields"].as_array().unwrap();
        let chave = campos
            .iter()
            .find(|c| c["name"] == CAMPO_WIREGUARD_CHAVE)
            .unwrap();
        assert_eq!(chave["type"], 1, "a chave privada sai oculta");
        assert!(
            campos
                .iter()
                .filter(|c| c["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with(CAMPO_WIREGUARD_PSK)))
                .all(|c| c["type"] == 1),
            "as chaves pré-compartilhadas saem ocultas"
        );

        let (volta, avisos) = importar(&exportado.to_string()).unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        let item = &volta.itens[0];
        assert_eq!(item.tipo, Tipo::WireGuard);
        assert_eq!(item.wireguard.as_ref(), Some(&identidade));
        assert_eq!(
            (item.usuario.as_str(), item.senha.as_str()),
            ("ana", "segredo")
        );
        assert!(item.campos.is_empty(), "os campos viraram a identidade");
    }

    #[test]
    fn campos_wireguard_que_nao_formam_identidade_ficam_nos_campos() {
        let item = um_item(
            r#"{"type":1,"name":"x","login":{},"fields":[
                {"name":"tirech-ssh.tipo","value":"WireGuard","type":0},
                {"name":"tirech-ssh.wireguard.chave","value":"lixo","type":1},
                {"name":"tirech-ssh.wireguard.enderecos","value":"10.8.0.2/24","type":0}]}"#,
        );
        assert_ne!(item.tipo, Tipo::WireGuard);
        assert!(item.wireguard.is_none());
        assert!(valor(&item, CAMPO_WIREGUARD_CHAVE).oculto);
    }

    fn identidade_tailscale_de_teste() -> IdentidadeTailscale {
        IdentidadeTailscale {
            servidor: "https://headscale.exemplo.test".into(),
            vence_em: 1_900_000_000,
            chave_autenticacao: "tskey-auth-teste".into(),
            login: "ana@exemplo.test".into(),
            tailnet: "empresa.exemplo.test".into(),
            nome: "tirech".into(),
            enderecos: vec!["100.64.0.7".into(), "fd7a:115c:a1e0::7".into()],
            ..IdentidadeTailscale::default()
        }
    }

    #[test]
    fn a_identidade_tailscale_vai_e_volta_em_campos_com_o_login() {
        let identidade = identidade_tailscale_de_teste();
        let pasta = PastaTemporaria::nova("bw-tailscale");
        let (mut cofre, _) = Cofre::abrir(&pasta.0);
        cofre
            .salvar(Item {
                tipo: Tipo::Tailscale,
                titulo: "Cliente T".into(),
                usuario: "ana".into(),
                senha: "segredo".into(),
                tailscale: Some(identidade.clone()),
                ..Item::default()
            })
            .unwrap();
        let exportado: Value = serde_json::from_str(&exportar(&cofre)).unwrap();
        let campos = exportado["items"][0]["fields"].as_array().unwrap();
        let chave = campos
            .iter()
            .find(|c| c["name"] == CAMPO_TAILSCALE_CHAVE_AUTENTICACAO)
            .unwrap();
        assert_eq!(chave["type"], 1, "a chave de autenticação sai oculta");

        let (volta, avisos) = importar(&exportado.to_string()).unwrap();
        assert!(avisos.is_empty(), "{avisos:?}");
        let item = &volta.itens[0];
        assert_eq!(item.tipo, Tipo::Tailscale);
        assert_eq!(item.tailscale.as_ref(), Some(&identidade));
        assert_eq!(
            (item.usuario.as_str(), item.senha.as_str()),
            ("ana", "segredo")
        );
        assert!(item.campos.is_empty(), "os campos viraram a identidade");
    }

    #[test]
    fn campos_tailscale_que_nao_formam_identidade_ficam_nos_campos() {
        let item = um_item(
            r#"{"type":1,"name":"x","login":{},"fields":[
                {"name":"tirech-ssh.tipo","value":"Tailscale","type":0},
                {"name":"tirech-ssh.tailscale.chave-no","value":"lixo","type":1},
                {"name":"tirech-ssh.tailscale.chave-autenticacao","value":"tskey-auth-x","type":1}]}"#,
        );
        assert_ne!(item.tipo, Tipo::Tailscale);
        assert!(item.tailscale.is_none());
        assert!(valor(&item, CAMPO_TAILSCALE_CHAVE_NO).oculto);
        assert!(valor(&item, CAMPO_TAILSCALE_CHAVE_AUTENTICACAO).oculto);
    }
}
