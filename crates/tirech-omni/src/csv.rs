//! CSV à mão (RFC 4180) e os dois formatos que o cofre importa: o do Bitwarden e o do KeePassXC.
//! Só importa: a exportação do cofre é em JSON (`cofre_json` e `bitwarden`).

use crate::cofre::{CAMPO_TIPO, Campo, Item, Lote, Tipo};
use crate::t;

/// Lê um CSV em linhas de campos. Ignora o BOM do começo e as linhas em branco; aspas sem
/// fechar é erro, com a linha onde elas abriram.
pub fn ler(texto: &str) -> Result<Vec<Vec<String>>, String> {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    let mut linhas = Vec::new();
    let mut linha: Vec<String> = Vec::new();
    let mut campo = String::new();
    // Se o campo em andamento abriu aspas (um `""` vazio ainda é um campo).
    let mut entre_aspas = false;
    let mut teve_aspas = false;
    let (mut numero, mut abriu_na_linha) = (1usize, 1usize);
    let mut chars = texto.chars().peekable();

    while let Some(c) = chars.next() {
        if entre_aspas {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    campo.push('"');
                }
                '"' => entre_aspas = false,
                _ => {
                    if c == '\n' {
                        numero += 1;
                    }
                    campo.push(c);
                }
            }
            continue;
        }
        match c {
            '"' if campo.is_empty() && !teve_aspas => {
                entre_aspas = true;
                teve_aspas = true;
                abriu_na_linha = numero;
            }
            ',' => {
                linha.push(std::mem::take(&mut campo));
                teve_aspas = false;
            }
            '\r' => {}
            '\n' => {
                numero += 1;
                if !(linha.is_empty() && campo.is_empty() && !teve_aspas) {
                    linha.push(std::mem::take(&mut campo));
                    linhas.push(std::mem::take(&mut linha));
                }
                teve_aspas = false;
            }
            _ => campo.push(c),
        }
    }
    if entre_aspas {
        return Err(t!("csv-aspas-sem-fechar", linha = abriu_na_linha as i64));
    }
    if !(linha.is_empty() && campo.is_empty() && !teve_aspas) {
        linha.push(campo);
        linhas.push(linha);
    }
    Ok(linhas)
}

/// O índice da coluna `nome` no cabeçalho (sem diferenciar maiúsculas).
fn coluna(cabecalho: &[String], nome: &str) -> Option<usize> {
    cabecalho
        .iter()
        .position(|c| c.trim().eq_ignore_ascii_case(nome))
}

/// O texto da coluna `nome` na linha (vazio se a coluna ou o campo não existem).
fn campo_de<'a>(cabecalho: &[String], linha: &'a [String], nome: &str) -> &'a str {
    coluna(cabecalho, nome)
        .and_then(|i| linha.get(i))
        .map_or("", String::as_str)
}

/// Importa o CSV do Bitwarden ou o do KeePassXC (descobre pelo cabeçalho). Os itens chegam com
/// `id` 0 e a pasta entre as do lote.
pub fn importar(texto: &str) -> Result<Lote, String> {
    importar_com_formato(texto).map(|(lote, _)| lote)
}

/// Como [`importar`], e diz qual formato era: "CSV do Bitwarden" ou "CSV do KeePassXC".
pub fn importar_com_formato(texto: &str) -> Result<(Lote, String), String> {
    let linhas = ler(texto)?;
    let Some((cabecalho, dados)) = linhas.split_first() else {
        return Err(erro_formato());
    };
    let tem = |nome| coluna(cabecalho, nome).is_some();
    type Leitura = fn(&mut Lote, &[String], &[String]) -> Item;
    let (leitura, nome): (Leitura, String) = if tem("login_username") && tem("name") {
        (item_do_bitwarden, t!("formato-csv-bitwarden"))
    } else if tem("Title") && tem("Password") {
        (item_do_keepassxc, t!("formato-csv-keepassxc"))
    } else {
        return Err(erro_formato());
    };
    let mut lote = Lote::default();
    for linha in dados {
        let item = leitura(&mut lote, cabecalho, linha);
        lote.itens.push(item);
    }
    Ok((lote, nome))
}

fn erro_formato() -> String {
    t!("csv-formato-desconhecido")
}

fn item_do_bitwarden(lote: &mut Lote, cabecalho: &[String], linha: &[String]) -> Item {
    let texto = |nome| campo_de(cabecalho, linha, nome).to_owned();
    let mut item = Item {
        tipo: if texto("type") == "note" {
            Tipo::Nota
        } else {
            Tipo::Senha
        },
        titulo: texto("name"),
        pasta: lote.caminho(&texto("folder")),
        usuario: texto("login_username"),
        senha: texto("login_password"),
        url: texto("login_uri"),
        notas: texto("notes"),
        ..Item::default()
    };
    for linha in campo_de(cabecalho, linha, "fields")
        .lines()
        .filter(|l| !l.trim().is_empty())
    {
        let (nome, valor) = linha.split_once(": ").unwrap_or((linha, ""));
        if nome == CAMPO_TIPO {
            // As chaves SSH e as identidades de rede precisam do segredo dentro, que o CSV não leva.
            if let Some(tipo) = Tipo::do_id(valor.trim()).filter(|t| {
                !matches!(
                    t,
                    Tipo::ChaveSsh
                        | Tipo::Nebula
                        | Tipo::OpenVpn
                        | Tipo::WireGuard
                        | Tipo::Tailscale
                        | Tipo::ZeroTier
                )
            }) {
                item.tipo = tipo;
            }
        } else {
            item.campos.push(Campo {
                nome: nome.to_owned(),
                valor: valor.to_owned(),
                oculto: false,
            });
        }
    }
    item
}

fn item_do_keepassxc(lote: &mut Lote, cabecalho: &[String], linha: &[String]) -> Item {
    let texto = |nome| campo_de(cabecalho, linha, nome).to_owned();
    let grupo = texto("Group");
    let pasta = match grupo.strip_prefix("Root") {
        Some("") => "",
        Some(resto) if resto.starts_with('/') => &resto[1..],
        _ => &grupo,
    };
    Item {
        tipo: Tipo::Senha,
        titulo: texto("Title"),
        pasta: lote.caminho(pasta),
        usuario: texto("Username"),
        senha: texto("Password"),
        url: texto("URL"),
        notas: texto("Notes"),
        ..Item::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(texto: &str) -> Vec<Vec<String>> {
        ler(texto).unwrap()
    }

    fn linha(campos: &[&str]) -> Vec<String> {
        campos.iter().map(|c| (*c).to_owned()).collect()
    }

    #[test]
    fn campos_simples_com_lf_e_crlf() {
        assert_eq!(l("a,b\nc,d\n"), [linha(&["a", "b"]), linha(&["c", "d"])]);
        assert_eq!(l("a,b\r\nc,d"), [linha(&["a", "b"]), linha(&["c", "d"])]);
    }

    #[test]
    fn aspas_virgula_aspas_dobradas_e_quebra_dentro_do_campo() {
        let linhas = l("\"a,b\",\"diz \"\"oi\"\"\",\"duas\nlinhas\"\r\nx,y,z\r\n");
        assert_eq!(linhas[0], linha(&["a,b", "diz \"oi\"", "duas\nlinhas"]));
        assert_eq!(linhas[1], linha(&["x", "y", "z"]));
    }

    #[test]
    fn bom_linhas_em_branco_e_campos_vazios() {
        assert_eq!(
            l("\u{feff}a,,c\n\n\n\"\",x\n"),
            [linha(&["a", "", "c"]), linha(&["", "x"])]
        );
        assert!(l("").is_empty());
    }

    #[test]
    fn aspas_sem_fechar_e_erro_com_a_linha() {
        let erro = ler("a,b\nc,\"sem fim\nmais").unwrap_err();
        assert_eq!(erro, "Aspas sem fechar na linha 2");
    }

    /// O nome da pasta de um item do lote.
    fn caminho(lote: &Lote, indice: usize) -> String {
        let mut nomes = Vec::new();
        let mut atual = lote.itens[indice].pasta;
        while let Some(pasta) = atual.and_then(|id| lote.pastas.iter().find(|p| p.id == id)) {
            nomes.push(pasta.nome.as_str());
            atual = pasta.pai;
        }
        nomes.reverse();
        nomes.join("/")
    }

    #[test]
    fn bitwarden_de_fora_com_o_campo_de_tipo_e_subpastas() {
        let texto = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,\
                     login_password,login_totp\n\
                     Clientes/ACME,,login,\"web, 01\",\"linha 1\nlinha 2\",\"tirech-ssh.tipo: Ssh\nregião: sa-east-1\",0,web.exemplo:22,ana,\"se\"\"gredo\",\n\
                     ,,note,lembrete,texto,\"tirech-ssh.tipo: Nota\",0,,,,\n";
        let (lote, nome) = importar_com_formato(texto).unwrap();
        assert_eq!(nome, "CSV do Bitwarden");
        assert_eq!(lote.itens.len(), 2);
        assert_eq!(lote.pastas.len(), 2);
        let web = &lote.itens[0];
        assert_eq!((web.tipo, web.titulo.as_str()), (Tipo::Ssh, "web, 01"));
        assert_eq!(
            (web.senha.as_str(), web.notas.as_str()),
            ("se\"gredo", "linha 1\nlinha 2")
        );
        assert_eq!(web.campos.len(), 1);
        assert_eq!(web.campos[0].nome, "região");
        assert_eq!(caminho(&lote, 0), "Clientes/ACME");
        assert_eq!(lote.itens[1].pasta, None);
    }

    #[test]
    fn bitwarden_de_fora_sem_o_campo_de_tipo() {
        let texto = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,\
                     login_password,login_totp\n\
                     Banco,1,login,Conta,,\"cartao: 1234\",0,https://banco,ana,pw,\n\
                     ,,note,Nota solta,texto,,0,,,,\n";
        let lote = importar(texto).unwrap();
        let itens = &lote.itens;
        assert_eq!(itens.len(), 2);
        assert_eq!(
            (
                itens[0].tipo,
                caminho(&lote, 0).as_str(),
                itens[0].url.as_str()
            ),
            (Tipo::Senha, "Banco", "https://banco")
        );
        assert_eq!(itens[0].campos[0].nome, "cartao");
        assert_eq!(itens[0].campos[0].valor, "1234");
        assert_eq!(
            (itens[1].tipo, itens[1].notas.as_str()),
            (Tipo::Nota, "texto")
        );
    }

    #[test]
    fn pastas_iguais_do_csv_se_juntam_no_lote() {
        let texto =
            "folder,type,name,login_username\nA/B,login,um,\nA,login,dois,\na/b,login,tres,\n";
        let lote = importar(texto).unwrap();
        assert_eq!(lote.pastas.len(), 2);
        assert_eq!(lote.itens[0].pasta, lote.itens[2].pasta);
        assert_ne!(lote.itens[0].pasta, lote.itens[1].pasta);
    }

    #[test]
    fn keepassxc_tira_o_root_e_ignora_colunas_extras() {
        let texto = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"Extra\"\n\
                     \"Root/Clientes\",\"Site\",\"ana\",\"pw\",\"https://x\",\"nota\",\"lixo\"\n\
                     \"Root\",\"Raiz\",\"\",\"p2\",\"\",\"\",\"\"\n\
                     \"Outro\",\"Curto\"\n\
                     \"Root/Banco/Contas\",\"Conta\",\"\",\"p3\",\"\",\"\",\"\"\n";
        let (lote, nome) = importar_com_formato(texto).unwrap();
        assert_eq!(nome, "CSV do KeePassXC");
        let itens = &lote.itens;
        assert_eq!(itens.len(), 4);
        assert_eq!(caminho(&lote, 0), "Clientes");
        assert_eq!(
            (
                itens[0].titulo.as_str(),
                itens[0].usuario.as_str(),
                itens[0].senha.as_str()
            ),
            ("Site", "ana", "pw")
        );
        assert_eq!(itens[1].pasta, None);
        // Linha com menos colunas: o resto fica vazio.
        assert_eq!(
            (caminho(&lote, 2).as_str(), itens[2].senha.as_str()),
            ("Outro", "")
        );
        assert_eq!(caminho(&lote, 3), "Banco/Contas");
        assert!(itens.iter().all(|i| i.tipo == Tipo::Senha));
    }

    #[test]
    fn cabecalho_desconhecido_e_erro() {
        let erro = importar("a,b,c\n1,2,3\n").unwrap_err();
        assert_eq!(
            erro,
            "Formato de CSV desconhecido: esperava o do Bitwarden ou o do KeePassXC"
        );
        assert!(importar("").is_err());
    }

    #[test]
    fn o_tipo_nebula_no_csv_nao_cria_identidade_vazia() {
        let texto =
            "folder,type,name,login_username,fields\n,login,rede,,\"tirech-ssh.tipo: Nebula\"\n";
        let lote = importar(texto).unwrap();
        assert_eq!(lote.itens[0].tipo, Tipo::Senha);
        assert!(lote.itens[0].nebula.is_none());
    }

    #[test]
    fn o_tipo_openvpn_no_csv_nao_cria_identidade_vazia() {
        let csv =
            "folder,type,name,login_username,fields\n,login,rede,,\"tirech-ssh.tipo: OpenVPN\"\n";
        let lote = importar(csv).unwrap();
        assert_eq!(lote.itens[0].tipo, Tipo::Senha);
        assert!(lote.itens[0].openvpn.is_none());
    }

    #[test]
    fn o_tipo_wireguard_no_csv_nao_cria_identidade_vazia() {
        let csv =
            "folder,type,name,login_username,fields\n,login,rede,,\"tirech-ssh.tipo: WireGuard\"\n";
        let lote = importar(csv).unwrap();
        assert_eq!(lote.itens[0].tipo, Tipo::Senha);
        assert!(lote.itens[0].wireguard.is_none());
    }

    #[test]
    fn o_tipo_tailscale_no_csv_nao_cria_identidade_vazia() {
        let csv =
            "folder,type,name,login_username,fields\n,login,rede,,\"tirech-ssh.tipo: Tailscale\"\n";
        let lote = importar(csv).unwrap();
        assert_eq!(lote.itens[0].tipo, Tipo::Senha);
        assert!(lote.itens[0].tailscale.is_none());
    }
}
