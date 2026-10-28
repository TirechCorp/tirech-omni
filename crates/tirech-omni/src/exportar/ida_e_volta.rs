//! Exportar e ler de volta com os importadores: o que sai tem de voltar igual (ou com o aviso
//! de que não coube).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tomni_ui::terminal::ssh::keys::{self, KdfLevel, KeyGen, KeyType, PrivateFormat};

use super::chaves;
use super::{Alvo, Opcoes, Resultado, Semente, exportar};
use crate::armazenamento::testes::PastaTemporaria;
use crate::cofre::{ChaveSsh, Cofre, Item, Tipo};
use crate::importar::{Entrada, Formato, Importacao, Trazida};
use crate::modelo::{
    Ativar, ComandosAoConectar, Connection, Jump, Kind, ModoComandos, ModoCredencial, ModoX11,
    MouseDoTerminal, Node, OrigemDaSenha, Passo, Retentar, SessaoPersistente, TipoTunel, Tree,
    Tunel,
};
use crate::redes::{PerfilRede, ProxyRede, Rede, TipoProxy};

struct Amostra {
    arvore: Tree,
    cofre: Cofre,
    _pasta: PastaTemporaria,
}

fn cofre_com(pasta: &Path, itens: Vec<Item>) -> (Cofre, Vec<u64>) {
    let (mut cofre, _) = Cofre::abrir(pasta);
    let ids = itens
        .into_iter()
        .map(|item| cofre.salvar(item).unwrap())
        .collect();
    (cofre, ids)
}

fn login(titulo: &str, usuario: &str, senha: &str) -> Item {
    Item {
        tipo: Tipo::Ssh,
        titulo: titulo.to_owned(),
        usuario: usuario.to_owned(),
        senha: senha.to_owned(),
        ..Item::default()
    }
}

fn ssh(nome: &str, host: &str) -> Connection {
    let mut c = Connection::new(nome, Kind::Ssh);
    c.host = host.to_owned();
    c
}

/// Uma árvore com pastas, uma pasta vazia e conexões de vários tipos; o cofre tem a senha de
/// duas delas e uma nota solta, que nenhuma conexão usa.
fn amostra(nome: &str) -> Amostra {
    let pasta = PastaTemporaria::nova(nome);
    let (cofre, ids) = cofre_com(
        &pasta.0,
        vec![
            login("acme", "root", "s3nha-do-acme"),
            login("beta", "", "s3nha-do-beta"),
            Item {
                tipo: Tipo::Nota,
                titulo: "notas do acme".to_owned(),
                notas: "banco & site".to_owned(),
                ..Item::default()
            },
            login("ftp", "ftpuser", "s3nha-do-ftp"),
        ],
    );
    let mut arvore = Tree::default();
    let clientes = arvore.add_group(None, "Clientes");
    let acme = arvore.add_group(Some(clientes), "ACME");
    arvore.add_group(None, "Vazia");

    let mut c = ssh("ACME prod", "10.0.0.9");
    c.port = 2222;
    c.user = "root".to_owned();
    c.key_file = Some(PathBuf::from("/home/ana/.ssh/id_acme"));
    c.x11 = ModoX11::Confiavel;
    c.credencial = Some(ids[0]);
    c.descricao = "banco & site".to_owned();
    let mut tunel = Tunel::novo(TipoTunel::Local, false);
    tunel.porta = 8080;
    tunel.destino_host = "localhost".to_owned();
    tunel.destino_porta = 80;
    c.tuneis.push(tunel);
    arvore.add_connection(Some(acme), c);

    let mut c = ssh("Beta", "beta.exemplo.com");
    c.credencial = Some(ids[1]);
    c.jump = Some(Jump {
        host: "gw.exemplo.com".to_owned(),
        port: 2200,
        user: "jumper".to_owned(),
        credencial: None,
    });
    arvore.add_connection(Some(clientes), c);

    let mut c = Connection::new("Arquivos", Kind::Ftp);
    c.host = "ftp.exemplo.com".to_owned();
    c.user = String::new();
    c.credencial = Some(ids[3]);
    arvore.add_connection(None, c);

    let mut c = Connection::new("Sftp solto", Kind::Sftp);
    c.host = "sftp.exemplo.com".to_owned();
    c.user = "ana".to_owned();
    c.pasta_local = "/home/ana/meus projetos".to_owned();
    c.pasta_remota = "/var/www/meu site".to_owned();
    c.comparar = Ativar::Sim;
    c.sincronizar = Ativar::Nao;
    arvore.add_connection(None, c);

    let proxy = ProxyRede {
        tipo: TipoProxy::Socks5,
        host: "127.0.0.1".to_owned(),
        port: 1081,
        credencial: None,
    };
    let id_rede = arvore.add_perfil(PerfilRede {
        proxy: Some(proxy),
        ..PerfilRede::new("Cliente A")
    });
    let mut c = ssh("Pelo proxy", "10.9.9.9");
    c.rede = Rede::Perfil(id_rede);
    c.credencial_modo = ModoCredencial::AgenteOuSenha;
    arvore.add_connection(None, c);

    Amostra {
        arvore,
        cofre,
        _pasta: pasta,
    }
}

fn fazer(alvo: Alvo, a: &Amostra, senhas: bool) -> Resultado {
    exportar(
        alvo,
        &a.arvore,
        Some(&a.cofre),
        Opcoes {
            senhas,
            chaves: true,
        },
        &HashMap::new(),
        Path::new("/tmp/destino.txt"),
        &mut Semente(42),
    )
}

fn ler(formato: &dyn Formato, texto: &str) -> Importacao {
    formato.analisar(texto, None).unwrap()
}

fn achar<'a>(i: &'a Importacao, nome: &str) -> &'a Trazida {
    i.conexoes()
        .find(|t| t.conexao.name == nome)
        .unwrap_or_else(|| {
            panic!(
                "falta {nome}: {:?}",
                i.conexoes().map(|t| &t.conexao.name).collect::<Vec<_>>()
            )
        })
}

fn pastas(i: &Importacao) -> Vec<Vec<String>> {
    i.entradas
        .iter()
        .filter_map(|e| match e {
            Entrada::Pasta(p) => Some(p.caminho.clone()),
            Entrada::Conexao(_) => None,
        })
        .collect()
}

#[test]
fn putty_volta_com_pasta_chave_x11_tunel_e_proxy() {
    let a = amostra("pu");
    let r = fazer(Alvo::Putty, &a, true);
    // Só SSH entra no `.reg`.
    assert_eq!(r.exportadas, 3, "{:?}", r.avisos);
    let i = ler(&crate::importar::putty::Putty, &r.texto);
    let acme = achar(&i, "ACME prod");
    assert_eq!(acme.pasta, ["Clientes", "ACME"]);
    assert_eq!(acme.conexao.host, "10.0.0.9");
    assert_eq!(acme.conexao.port, 2222);
    assert_eq!(acme.conexao.user, "root");
    assert_eq!(
        acme.conexao.key_file,
        Some(PathBuf::from("/home/ana/.ssh/id_acme"))
    );
    assert_eq!(acme.conexao.x11, ModoX11::Confiavel);
    assert_eq!(acme.conexao.tuneis.len(), 1);
    assert_eq!(acme.conexao.tuneis[0].porta, 8080);
    assert_eq!(acme.senha, None, "o PuTTY não guarda senha");
    let proxy = achar(&i, "Pelo proxy");
    assert_eq!(
        proxy
            .perfil
            .as_ref()
            .unwrap()
            .perfil
            .proxy
            .as_ref()
            .unwrap()
            .port,
        1081
    );
    assert!(!r.texto.contains("s3nha"));
}

#[test]
fn winscp_volta_com_senhas_pastas_vazias_e_salto() {
    let a = amostra("wi");
    let r = fazer(Alvo::WinScp, &a, true);
    assert_eq!(r.exportadas, 5, "{:?}", r.avisos);
    let i = ler(&crate::importar::winscp::WinScp, &r.texto);
    let acme = achar(&i, "ACME prod");
    assert_eq!(acme.pasta, ["Clientes", "ACME"]);
    assert_eq!(acme.senha.as_deref(), Some("s3nha-do-acme"));
    assert_eq!(acme.conexao.descricao, "banco & site");
    let beta = achar(&i, "Beta");
    assert_eq!(beta.senha.as_deref(), Some("s3nha-do-beta"));
    let jump = beta.conexao.jump.as_ref().unwrap();
    assert_eq!((jump.host.as_str(), jump.port), ("gw.exemplo.com", 2200));
    let ftp = achar(&i, "Arquivos");
    assert_eq!(ftp.conexao.kind, Kind::Ftp);
    assert_eq!(ftp.conexao.user, "ftpuser");
    assert_eq!(ftp.senha.as_deref(), Some("s3nha-do-ftp"));
    assert!(pastas(&i).contains(&vec!["Vazia".to_owned()]));
    // Sem senhas, nada delas no texto.
    let sem = fazer(Alvo::WinScp, &a, false);
    assert!(!sem.texto.contains("Password="), "{}", sem.texto);
    assert_eq!(
        ler(&crate::importar::winscp::WinScp, &sem.texto).senhas(),
        0
    );
}

#[test]
fn winscp_com_a_mesma_semente_sai_igual() {
    let a = amostra("wi2");
    assert_eq!(
        fazer(Alvo::WinScp, &a, true).texto,
        fazer(Alvo::WinScp, &a, true).texto
    );
}

#[test]
fn filezilla_volta_com_pasta_senha_e_so_o_que_ele_conhece() {
    let a = amostra("fz");
    let r = fazer(Alvo::FileZilla, &a, true);
    // FTP e SFTP; o SSH não entra.
    assert_eq!(r.exportadas, 2, "{:?}", r.avisos);
    assert!(r.avisos.iter().any(|v| v.contains("SSH")), "{:?}", r.avisos);
    let i = ler(&crate::importar::filezilla::FileZilla, &r.texto);
    let ftp = achar(&i, "Arquivos");
    assert_eq!(ftp.conexao.kind, Kind::Ftp);
    assert_eq!(ftp.senha.as_deref(), Some("s3nha-do-ftp"));
    assert_eq!(achar(&i, "Sftp solto").conexao.kind, Kind::Sftp);
}

#[test]
fn filezilla_volta_com_as_pastas_padrao_e_so_o_ligado() {
    let a = amostra("fz-pastas");
    let r = fazer(Alvo::FileZilla, &a, false);
    assert!(
        r.texto
            .contains("<RemoteDir>1 0 3 var 3 www 8 meu site</RemoteDir>"),
        "{}",
        r.texto
    );
    assert!(
        r.texto
            .contains("<DirectoryComparison>1</DirectoryComparison>")
    );
    // "Desligado" não existe no FileZilla: sai sem o campo.
    assert!(!r.texto.contains("SyncBrowsing"), "{}", r.texto);
    let i = ler(&crate::importar::filezilla::FileZilla, &r.texto);
    let sftp = &achar(&i, "Sftp solto").conexao;
    assert_eq!(sftp.pasta_local, "/home/ana/meus projetos");
    assert_eq!(sftp.pasta_remota, "/var/www/meu site");
    assert_eq!(sftp.comparar, Ativar::Sim);
    assert_eq!(sftp.sincronizar, Ativar::Padrao);
    assert_eq!(achar(&i, "Arquivos").conexao.pasta_local, "");
}

#[test]
fn winscp_volta_com_as_pastas_padrao() {
    let a = amostra("wi-pastas");
    let r = fazer(Alvo::WinScp, &a, false);
    assert!(
        r.texto.contains("RemoteDirectory=/var/www/meu%20site"),
        "{}",
        r.texto
    );
    let i = ler(&crate::importar::winscp::WinScp, &r.texto);
    let sftp = &achar(&i, "Sftp solto").conexao;
    assert_eq!(sftp.pasta_local, "/home/ana/meus projetos");
    assert_eq!(sftp.pasta_remota, "/var/www/meu site");
    // O WinSCP não tem o liga-desliga: o que não cabe volta como "Padrão".
    assert_eq!(sftp.comparar, Ativar::Padrao);
}

#[test]
fn s3_no_winscp_nao_leva_a_pasta_remota_no_lugar_do_bucket() {
    let mut a = com_s3();
    let id = a.arvore.connections()[0].id;
    a.arvore.connection_mut(id).unwrap().pasta_remota = "/outra".to_owned();
    let r = fazer(Alvo::WinScp, &a, false);
    let i = ler(&crate::importar::winscp::WinScp, &r.texto);
    let t = achar(&i, "Backups");
    assert_eq!(t.conexao.s3.bucket, "meu-bucket");
    assert_eq!(t.conexao.pasta_remota, "");
}

#[test]
fn winsshterm_volta_com_pastas_descricao_x11_e_proxy_e_avisa_do_salto() {
    let a = amostra("ws");
    let r = fazer(Alvo::WinSshTerm, &a, true);
    assert_eq!(r.exportadas, 3, "{:?}", r.avisos);
    assert!(r.texto.contains("Password=\"\""), "{}", r.texto);
    assert!(!r.texto.contains("s3nha"));
    assert!(
        r.avisos.iter().any(|v| v.contains("salto")),
        "{:?}",
        r.avisos
    );
    let i = ler(&crate::importar::winsshterm::WinSshTerm, &r.texto);
    let acme = achar(&i, "ACME prod");
    assert_eq!(acme.pasta, ["Clientes", "ACME"]);
    assert_eq!(acme.conexao.x11, ModoX11::Confiavel);
    assert_eq!(acme.conexao.descricao, "banco & site");
    assert!(achar(&i, "Pelo proxy").perfil.is_some());
    assert!(pastas(&i).contains(&vec!["Vazia".to_owned()]));
}

#[test]
fn mremoteng_volta_com_a_senha_mestra_padrao() {
    let a = amostra("mr");
    let r = fazer(Alvo::MRemoteNg, &a, true);
    assert_eq!(r.exportadas, 3, "{:?}", r.avisos);
    assert!(!r.texto.contains("s3nha"));
    let i = ler(&crate::importar::mremoteng::MRemoteNg, &r.texto);
    assert!(!i.pede_senha_mestra);
    assert_eq!(
        achar(&i, "ACME prod").senha.as_deref(),
        Some("s3nha-do-acme")
    );
    assert_eq!(achar(&i, "ACME prod").pasta, ["Clientes", "ACME"]);
    assert_eq!(achar(&i, "Beta").senha.as_deref(), Some("s3nha-do-beta"));
    // Sem senhas, nada cifrado.
    let sem = fazer(Alvo::MRemoteNg, &a, false);
    assert_eq!(
        ler(&crate::importar::mremoteng::MRemoteNg, &sem.texto).senhas(),
        0
    );
}

#[test]
fn mobaxterm_volta_com_pastas_chave_e_so_ssh() {
    let a = amostra("mo");
    let r = fazer(Alvo::MobaXterm, &a, true);
    assert_eq!(r.exportadas, 3, "{:?}", r.avisos);
    let i = ler(&crate::importar::mobaxterm::MobaXterm, &r.texto);
    let acme = achar(&i, "ACME prod");
    assert_eq!(acme.pasta, ["Clientes", "ACME"]);
    assert_eq!(acme.conexao.port, 2222);
    assert_eq!(acme.conexao.user, "root");
    assert_eq!(
        acme.conexao.key_file,
        Some(PathBuf::from("/home/ana/.ssh/id_acme"))
    );
}

#[test]
fn ssh_config_escreve_host_porta_usuario_chave_e_salto() {
    let a = amostra("sc");
    let r = fazer(Alvo::SshConfig, &a, true);
    assert_eq!(r.exportadas, 3, "{:?}", r.avisos);
    let t = &r.texto;
    assert!(t.contains("Host ACME-prod"), "{t}");
    assert!(t.contains("HostName 10.0.0.9"), "{t}");
    assert!(t.contains("Port 2222"), "{t}");
    assert!(t.contains("User root"), "{t}");
    assert!(t.contains("IdentityFile /home/ana/.ssh/id_acme"), "{t}");
    assert!(t.contains("ProxyJump jumper@gw.exemplo.com:2200"), "{t}");
    assert!(t.contains("LocalForward 8080 localhost:80"), "{t}");
    assert!(!t.contains("s3nha"));
    assert!(
        r.avisos.iter().any(|v| v.contains("proxy")),
        "{:?}",
        r.avisos
    );
}

#[test]
fn nativo_sem_senhas_solta_as_ligacoes_e_volta_igual() {
    let a = amostra("na");
    let r = fazer(Alvo::Nativo, &a, false);
    assert!(!r.texto.contains("s3nha"));
    assert!(!r.texto.contains("\"cofre\""));
    let i = ler(&crate::importar::nativo::Nativo, &r.texto);
    assert!(i.cofre.is_none());
    assert_eq!(i.total(), 5);
    let acme = achar(&i, "ACME prod");
    assert_eq!(acme.pasta, ["Clientes", "ACME"]);
    assert_eq!(acme.conexao.credencial, None);
    assert_eq!(acme.conexao.descricao, "banco & site");
    assert_eq!(acme.conexao.tuneis, a.arvore.connections()[0].tuneis);
    assert!(achar(&i, "Pelo proxy").perfil.is_some());
    assert_eq!(
        achar(&i, "Pelo proxy").conexao.credencial_modo,
        ModoCredencial::AgenteOuSenha,
        "o modo da credencial não aponta para o cofre: vai junto"
    );
    assert!(pastas(&i).contains(&vec!["Vazia".to_owned()]));
}

#[test]
fn nativo_com_senhas_leva_o_cofre_dos_itens_usados() {
    let a = amostra("nb");
    let r = fazer(Alvo::Nativo, &a, true);
    let i = ler(&crate::importar::nativo::Nativo, &r.texto);
    let lote = i.cofre.as_ref().expect("o bloco cofre");
    let titulos: Vec<&str> = lote.itens.iter().map(|i| i.titulo.as_str()).collect();
    assert!(
        titulos.contains(&"acme") && titulos.contains(&"beta"),
        "{titulos:?}"
    );
    assert!(
        titulos.contains(&"ftp") && !titulos.contains(&"notas do acme"),
        "só os itens usados: {titulos:?}"
    );
    assert_eq!(
        achar(&i, "ACME prod").conexao.credencial,
        Some(a.cofre.itens()[0].id)
    );
}

/// Um cofre com uma chave Ed25519 (a privada em OpenSSH, sem passphrase) e uma conexão que a usa.
fn com_chave(nome: &str) -> (Amostra, String) {
    let pasta = PastaTemporaria::nova(nome);
    let gerada = KeyGen::new(KeyType::Ed25519)
        .comment("ana@notebook")
        .generate()
        .unwrap();
    let privada = keys::export_private(
        gerada.private_key(),
        PrivateFormat::OpenSsh,
        None,
        KdfLevel::Standard,
    )
    .unwrap()
    .to_string();
    let chave = Item {
        tipo: Tipo::ChaveSsh,
        titulo: "Chave do ACME".to_owned(),
        chave: Some(ChaveSsh {
            privada,
            publica: gerada.public_key().to_openssh().unwrap(),
            fingerprint: keys::fingerprint(gerada.public_key()),
            ..ChaveSsh::default()
        }),
        ..Item::default()
    };
    let (cofre, ids) = cofre_com(&pasta.0, vec![chave]);
    let uid = cofre.item(ids[0]).unwrap().uid.clone();
    let mut arvore = Tree::default();
    let mut c = ssh("ACME", "10.0.0.9");
    c.chave_do_cofre = Some(uid.clone());
    arvore.add_connection(None, c);
    (
        Amostra {
            arvore,
            cofre,
            _pasta: pasta,
        },
        uid,
    )
}

#[test]
fn chaves_do_cofre_saem_na_pasta_ao_lado_e_a_conexao_aponta_para_elas() {
    let (a, uid) = com_chave("ch");
    let saida = PastaTemporaria::nova("ch-saida");
    let destino = saida.0.join("config.reg");
    let gravadas = chaves::gravar(&a.arvore, Some(&a.cofre), Alvo::Putty, &destino);
    let arquivo = gravadas
        .arquivos
        .get(&uid)
        .expect("a chave gravada")
        .clone();
    assert_eq!(
        arquivo,
        saida.0.join("config-chaves").join("Chave_do_ACME.ppk")
    );
    assert!(
        std::fs::read_to_string(&arquivo)
            .unwrap()
            .starts_with("PuTTY-User-Key-File-3")
    );
    assert_eq!(gravadas.pasta.as_deref(), arquivo.parent());
    assert!(gravadas.avisos.iter().any(|v| v.contains("sem passphrase")));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modo = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo(arquivo.parent().unwrap()), 0o700);
        assert_eq!(modo(&arquivo), 0o600);
    }
    let r = exportar(
        Alvo::Putty,
        &a.arvore,
        Some(&a.cofre),
        Opcoes::default(),
        &gravadas.arquivos,
        &destino,
        &mut Semente(1),
    );
    let i = ler(&crate::importar::putty::Putty, &r.texto);
    assert_eq!(
        achar(&i, "ACME").conexao.key_file.as_deref(),
        Some(arquivo.as_path())
    );
    assert!(
        r.avisos.iter().all(|v| !v.contains("agente")),
        "{:?}",
        r.avisos
    );
}

#[test]
fn chaves_do_cofre_sem_a_caixa_ficam_sem_arquivo_e_avisam_do_agente() {
    let (a, _) = com_chave("ch2");
    let r = exportar(
        Alvo::SshConfig,
        &a.arvore,
        Some(&a.cofre),
        Opcoes {
            senhas: false,
            chaves: false,
        },
        &HashMap::new(),
        Path::new("/tmp/config"),
        &mut Semente(1),
    );
    assert!(!r.texto.contains("IdentityFile"), "{}", r.texto);
    assert!(
        r.avisos.iter().any(|v| v.contains("agente")),
        "{:?}",
        r.avisos
    );
}

#[test]
fn duas_chaves_com_o_mesmo_titulo_ganham_nomes_diferentes() {
    let mut usados = std::collections::HashSet::new();
    assert_eq!(
        chaves::nome_livre("chave", ".ppk", &mut usados),
        "chave.ppk"
    );
    assert_eq!(
        chaves::nome_livre("chave", ".ppk", &mut usados),
        "chave-2.ppk"
    );
    assert_eq!(
        chaves::nome_livre("Chave", ".ppk", &mut usados),
        "Chave-3.ppk"
    );
}

#[test]
fn sem_cofre_as_chaves_nao_saem_e_o_aviso_diz_porque() {
    let (a, _) = com_chave("ch3");
    let g = chaves::gravar(&a.arvore, None, Alvo::SshConfig, Path::new("/tmp/x/config"));
    assert!(g.arquivos.is_empty() && g.pasta.is_none());
    assert!(g.avisos.iter().any(|v| v.contains("bloqueado")));
    // O mRemoteNG e o nativo não têm campo de chave: nada a gravar.
    let g = chaves::gravar(
        &a.arvore,
        Some(&a.cofre),
        Alvo::MRemoteNg,
        Path::new("/tmp/x/c"),
    );
    assert_eq!(g, chaves::Gravadas::default());
}

#[test]
fn nativo_com_senhas_religa_no_cofre_de_outra_maquina() {
    let a = amostra("nc");
    let r = fazer(Alvo::Nativo, &a, true);
    let mut i = ler(&crate::importar::nativo::Nativo, &r.texto);
    let outra = PastaTemporaria::nova("nc-outra");
    let (mut destino, _) = Cofre::abrir(&outra.0);
    let lote = i.cofre.take().unwrap();
    let mapa = destino.importar(lote, None).unwrap();
    i.religar(&mapa);
    let acme = achar(&i, "ACME prod");
    let credencial = destino.item(acme.conexao.credencial.unwrap()).unwrap();
    assert_eq!(credencial.senha, "s3nha-do-acme");
    assert_eq!(acme.conexao.descricao, "banco & site");
    let ftp = achar(&i, "Arquivos");
    assert_eq!(
        destino.item(ftp.conexao.credencial.unwrap()).unwrap().senha,
        "s3nha-do-ftp"
    );
}

/// Os passos de uma conexão e de uma pasta, com a senha de um item do cofre.
fn com_comandos(a: &mut Amostra) -> (u64, u64) {
    let beta = a
        .arvore
        .connections()
        .into_iter()
        .find(|c| c.name == "Beta");
    let item = beta.and_then(|c| c.credencial).unwrap();
    let proprios = |passos: Vec<Passo>| ComandosAoConectar {
        modo: ModoComandos::Proprios,
        passos,
        reconectar: false,
    };
    fn cada_pasta(nos: &mut [Node], f: &mut dyn FnMut(&mut crate::modelo::Group)) {
        for no in nos {
            if let Node::Group(g) = no {
                f(g);
                cada_pasta(&mut g.children, f);
            }
        }
    }
    cada_pasta(&mut a.arvore.nodes, &mut |g| {
        if g.name == "ACME" {
            g.persistente = SessaoPersistente::Tmux;
            g.mouse = MouseDoTerminal::CopiarColar;
            g.retentar = Retentar::Minutos(15);
            g.credencial_modo = ModoCredencial::Agente;
            g.comandos = proprios(vec![
                Passo::comando("sudo su -", 20),
                Passo::senha(OrigemDaSenha::Cofre { id: item }, 20),
            ]);
        }
    });
    let id = a
        .arvore
        .connections()
        .into_iter()
        .find(|c| c.name == "ACME prod")
        .unwrap()
        .id;
    let acme = a.arvore.connection_mut(id).unwrap();
    acme.persistente = SessaoPersistente::Nenhuma;
    acme.mouse = MouseDoTerminal::Menu;
    acme.retentar = Retentar::Desligado;
    acme.comandos = proprios(vec![
        Passo::comando("cd /srv", 5),
        Passo::senha(OrigemDaSenha::Conexao, 5),
    ]);
    (id, item)
}

#[test]
fn nativo_leva_os_comandos_e_religa_a_senha_do_cofre() {
    let mut a = amostra("nc-cmd");
    let (_, item) = com_comandos(&mut a);
    let r = fazer(Alvo::Nativo, &a, true);
    let mut i = ler(&crate::importar::nativo::Nativo, &r.texto);
    let outra = PastaTemporaria::nova("nc-cmd-outra");
    let (mut destino, _) = Cofre::abrir(&outra.0);
    let lote = i.cofre.take().unwrap();
    let mapa = destino.importar(lote, None).unwrap();
    i.religar(&mapa);
    let novo = mapa.itens[&item];
    let pasta = i
        .entradas
        .iter()
        .find_map(|e| match e {
            Entrada::Pasta(p) if p.caminho.last().is_some_and(|n| n == "ACME") => Some(p),
            _ => None,
        })
        .unwrap();
    assert_eq!(pasta.comandos.modo, ModoComandos::Proprios);
    assert_eq!(pasta.comandos.passos.len(), 2);
    assert_eq!(pasta.comandos.passos[0].max_secs, 20);
    assert_eq!(
        pasta.comandos.passos[1],
        Passo::senha(OrigemDaSenha::Cofre { id: novo }, 20)
    );
    assert_eq!(pasta.persistente, SessaoPersistente::Tmux);
    assert_eq!(pasta.mouse, MouseDoTerminal::CopiarColar);
    assert_eq!(pasta.retentar, Retentar::Minutos(15));
    assert_eq!(pasta.credencial_modo, ModoCredencial::Agente);
    let acme = achar(&i, "ACME prod");
    assert_eq!(acme.conexao.persistente, SessaoPersistente::Nenhuma);
    assert_eq!(acme.conexao.mouse, MouseDoTerminal::Menu);
    assert_eq!(acme.conexao.retentar, Retentar::Desligado);
    assert!(!acme.conexao.comandos.reconectar);
    assert_eq!(
        acme.conexao.comandos.passos[0],
        Passo::comando("cd /srv", 5)
    );
    assert_eq!(
        acme.conexao.comandos.passos[1],
        Passo::senha(OrigemDaSenha::Conexao, 5)
    );
}

#[test]
fn nativo_sem_senhas_leva_os_comandos_e_a_senha_vira_a_da_conexao() {
    let mut a = amostra("nc-cmd-sem");
    com_comandos(&mut a);
    let r = fazer(Alvo::Nativo, &a, false);
    assert!(!r.texto.contains("\"cofre\":"), "{}", r.texto);
    let i = ler(&crate::importar::nativo::Nativo, &r.texto);
    let pasta = i
        .entradas
        .iter()
        .find_map(|e| match e {
            Entrada::Pasta(p) if p.caminho.last().is_some_and(|n| n == "ACME") => Some(p),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        pasta.comandos.passos[1],
        Passo::senha(OrigemDaSenha::Conexao, 20)
    );
    assert_eq!(pasta.comandos.passos[0], Passo::comando("sudo su -", 20));
}

fn com_s3() -> Amostra {
    let pasta = PastaTemporaria::nova("s3");
    let (cofre, ids) = cofre_com(
        &pasta.0,
        vec![Item {
            tipo: Tipo::S3,
            titulo: "conta".to_owned(),
            usuario: "AKIAEXEMPLO".to_owned(),
            senha: "segredo-do-s3".to_owned(),
            campos: vec![crate::cofre::Campo {
                nome: crate::cofre::CAMPO_TOKEN.to_owned(),
                valor: "token-de-sessao".to_owned(),
                ..crate::cofre::Campo::default()
            }],
            ..Item::default()
        }],
    );
    let mut arvore = Tree::default();
    let mut c = Connection::new("Backups", Kind::S3);
    c.s3 = crate::importar::opcoes_s3("s3.eu-west-1.amazonaws.com", 443, true, "meu-bucket");
    c.credencial = Some(ids[0]);
    arvore.add_connection(None, c);
    Amostra {
        arvore,
        cofre,
        _pasta: pasta,
    }
}

#[test]
fn s3_volta_pelo_winscp_com_bucket_regiao_chave_e_token() {
    let a = com_s3();
    let r = fazer(Alvo::WinScp, &a, true);
    assert!(!r.texto.contains("segredo-do-s3"));
    let i = ler(&crate::importar::winscp::WinScp, &r.texto);
    let t = achar(&i, "Backups");
    assert_eq!(t.conexao.kind, Kind::S3);
    assert_eq!(t.conexao.s3.bucket, "meu-bucket");
    assert_eq!(t.conexao.s3.region, "eu-west-1");
    assert_eq!(t.conexao.user, "AKIAEXEMPLO");
    assert_eq!(t.senha.as_deref(), Some("segredo-do-s3"));
    assert_eq!(t.token_s3.as_deref(), Some("token-de-sessao"));
}

#[test]
fn s3_no_filezilla_perde_o_bucket_e_avisa() {
    let a = com_s3();
    let r = fazer(Alvo::FileZilla, &a, true);
    assert_eq!(r.exportadas, 1);
    assert!(
        r.avisos.iter().any(|v| v.contains("bucket")),
        "{:?}",
        r.avisos
    );
    let i = ler(&crate::importar::filezilla::FileZilla, &r.texto);
    assert_eq!(achar(&i, "Backups").conexao.kind, Kind::S3);
}

#[test]
fn winscp_avisa_da_senha_maior_que_255_bytes() {
    let pasta = PastaTemporaria::nova("longa");
    let (cofre, ids) = cofre_com(&pasta.0, vec![login("longa", "ana", &"x".repeat(300))]);
    let mut arvore = Tree::default();
    let mut c = ssh("Longa", "h.exemplo.com");
    c.credencial = Some(ids[0]);
    arvore.add_connection(None, c);
    let a = Amostra {
        arvore,
        cofre,
        _pasta: pasta,
    };
    let r = fazer(Alvo::WinScp, &a, true);
    assert!(r.avisos.iter().any(|v| v.contains("255")), "{:?}", r.avisos);
    assert_eq!(ler(&crate::importar::winscp::WinScp, &r.texto).senhas(), 0);
}
