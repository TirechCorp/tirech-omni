//! Importar e exportar (M38), pela tela: o menu, a pré-visualização, as senhas no cofre, as
//! duplicatas, o diálogo da exportação e a volta do `.reg` pelo importador do PuTTY.
//!
//! As amostras são escritas à mão, no formato que cada programa grava. Para conferir com um
//! arquivo de verdade, exporte as conexões do programa (FileZilla: Arquivo ▸ Exportar; WinSCP:
//! Configurações ▸ Exportar/Backup; PuTTY: `reg export HKCU\Software\SimonTatham\PuTTY\Sessions`)
//! e use "Importar ▸" com ele. O caminho de volta (abrir o que o Tirech Omni exporta no programa
//! de destino) só se confere à mão, no Windows: está no roteiro do `docs/pt-BR/manual.md`.

mod comum;

use std::path::Path;
use std::time::Duration;

use tirech_omni::importar;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Pasta, exemplo, item_chave_ssh, item_ssh, montar, montar_pura};

const FILEZILLA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<FileZilla3 version="3.66.5" platform="*nix">
 <Servers>
  <Folder expanded="1">Clientes
   <Server>
    <Host>ftp.acme.com</Host><Port>21</Port><Protocol>0</Protocol><Type>0</Type>
    <User>acme</User><Pass encoding="base64">c2VucmE=</Pass><Logontype>1</Logontype>
    <Name>ACME FTP</Name>
   </Server>
   <Server>
    <Host>sftp.exemplo.com</Host><Port>2200</Port><Protocol>1</Protocol>
    <User>ana</User><Logontype>0</Logontype><Name>Beta SFTP</Name>
   </Server>
  </Folder>
  <Server>
   <Host>s3.eu-west-1.amazonaws.com</Host><Protocol>7</Protocol><User>AKIAXYZ</User>
   <Pass encoding="base64">c2VnLXMz</Pass><Name>Bucket</Name>
  </Server>
 </Servers>
</FileZilla3>"#;

fn abrir_submenu(app: &mut TestApp, submenu: &str) {
    app.click("menus-arquivo");
    app.hover(submenu);
    app.wait(Duration::from_millis(450));
}

/// "Arquivo ▸ Importar ▸ <programa>", respondendo o seletor com `arquivo`.
fn importar_de(app: &mut TestApp, programa: &str, arquivo: &Path) {
    abrir_submenu(app, "importar-menu");
    app.click(&format!("importar-{programa}"));
    app.file_dialog_answer(Some(arquivo.to_path_buf()));
}

/// "Arquivo ▸ Exportar ▸ <programa>": abre o diálogo das caixas.
fn abrir_exportar(app: &mut TestApp, programa: &str) {
    abrir_submenu(app, "exportar-menu");
    app.click(&format!("exportar-{programa}"));
}

fn escrever(pasta: &Pasta, nome: &str, texto: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    let caminho = pasta.caminho().join(nome);
    std::fs::write(&caminho, texto).unwrap();
    caminho
}

fn titulos_do_cofre(pasta: &Pasta) -> Vec<String> {
    let mut lista: Vec<String> = pasta
        .cofre()
        .itens()
        .iter()
        .map(|i| i.titulo.clone())
        .collect();
    lista.sort();
    lista
}

#[test]
fn o_menu_arquivo_tem_importar_e_exportar_com_um_item_por_programa() {
    let pasta = Pasta::nova("importar-menu");
    let mut m = montar(&pasta, Theme::Light);
    abrir_submenu(&mut m.app, "importar-menu");
    for id in [
        "importar-winsshterm",
        "importar-filezilla",
        "importar-winscp",
        "importar-putty",
        "importar-mremoteng",
        "importar-mobaxterm",
        "importar-nativo",
        "importar",
        "importar-aws",
    ] {
        assert!(m.app.visible(id), "falta {id} em Importar");
    }
    m.app.hover("exportar-menu");
    m.app.wait(Duration::from_millis(450));
    for id in [
        "exportar-nativo",
        "exportar-ssh-config",
        "exportar-putty",
        "exportar-filezilla",
        "exportar-winscp",
        "exportar-winsshterm",
        "exportar-mremoteng",
        "exportar-mobaxterm",
    ] {
        assert!(m.app.visible(id), "falta {id} em Exportar");
    }
}

#[test]
fn importar_do_filezilla_mostra_a_previa_e_grava_com_as_senhas_no_cofre() {
    let pasta = Pasta::nova("importar-filezilla");
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    abrir_submenu(&mut m.app, "importar-menu");
    m.app.click("importar-filezilla");
    let pedidos = m.app.file_dialogs();
    assert_eq!(pedidos.len(), 1);
    assert_eq!(pedidos[0].title, "Importar do FileZilla");
    assert_eq!(pedidos[0].file_name.as_deref(), Some("sitemanager.xml"));
    m.app.file_dialog_answer(Some(arquivo));

    assert!(m.app.is_open("importar-dialogo"));
    assert_eq!(
        m.app.text("importar-resumo"),
        "Do FileZilla: 3 conexões em 1 grupo, 2 senhas"
    );
    let linhas: Vec<String> = m
        .app
        .tree_items("importar-arvore")
        .into_iter()
        .map(|r| r.label)
        .collect();
    assert_eq!(
        linhas,
        [
            "Clientes",
            "ACME FTP (acme@ftp.acme.com)",
            "Beta SFTP (anonymous@sftp.exemplo.com)",
            "Bucket (s3.eu-west-1.amazonaws.com)"
        ]
    );
    assert!(m.app.checked("importar-senhas"), "ligada por padrão");
    assert_eq!(m.app.text("importar-destino"), "Importado de FileZilla");
    // Nada foi gravado antes de confirmar.
    assert_eq!(pasta.conexoes().connection_count(), 0);

    m.app.click("importar-dialogo-primary");
    assert!(!m.app.is_open("importar-dialogo"));
    assert!(m.app.text("aviso-sucesso").contains("Importei 3 conexões"));
    let tree = pasta.conexoes();
    assert_eq!(tree.connection_count(), 3);
    assert_eq!(
        titulos_do_cofre(&pasta),
        [
            "AKIAXYZ@https://s3.eu-west-1.amazonaws.com",
            "acme@ftp.acme.com"
        ]
    );
    let rotulos: Vec<String> = m
        .app
        .tree_items("arvore")
        .into_iter()
        .map(|r| r.label)
        .collect();
    assert_eq!(rotulos[0], "Importado de FileZilla");
}

#[test]
fn desmarcar_as_senhas_importa_so_as_conexoes() {
    let pasta = Pasta::nova("importar-sem-senhas");
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    m.app.click("importar-senhas");
    assert!(!m.app.checked("importar-senhas"));
    m.app.click("importar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 3);
    assert!(titulos_do_cofre(&pasta).is_empty());
}

#[test]
fn cancelar_a_previa_nao_muda_nada() {
    let pasta = Pasta::nova("importar-previa-cancelada");
    pasta.semear(&exemplo());
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    assert!(m.app.is_open("importar-dialogo"));
    m.app.click("importar-dialogo-close");
    assert!(!m.app.is_open("importar-dialogo"));
    assert_eq!(pasta.conexoes(), exemplo());
    assert!(titulos_do_cofre(&pasta).is_empty());
}

#[test]
fn o_nome_do_destino_vira_o_grupo_da_importacao() {
    let pasta = Pasta::nova("importar-destino");
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    // O campo vem com "Importado de FileZilla": apaga e digita o nome.
    m.app.click("importar-destino");
    m.app.key(tomni_ui::Key::End);
    for _ in 0.."Importado de FileZilla".chars().count() {
        m.app.key(tomni_ui::Key::Backspace);
    }
    m.app.type_text("Meus clientes");
    m.app.click("importar-dialogo-primary");
    let rotulos: Vec<String> = m
        .app
        .tree_items("arvore")
        .into_iter()
        .map(|r| r.label)
        .collect();
    assert_eq!(rotulos[0], "Meus clientes");
}

#[test]
fn as_duplicadas_pulam_por_padrao_e_o_outro_modo_mantem_as_duas() {
    let pasta = Pasta::nova("importar-duplicadas");
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    m.app.click("importar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 3);

    importar_de(&mut m.app, "filezilla", &arquivo);
    m.app.click("importar-dialogo-primary");
    let mensagem = m.app.text("aviso-sucesso");
    assert!(mensagem.contains("3 puladas"), "{mensagem}");
    assert_eq!(pasta.conexoes().connection_count(), 3);

    importar_de(&mut m.app, "filezilla", &arquivo);
    m.app.select("importar-duplicadas", "Manter as duas");
    m.app.click("importar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 6);
}

#[test]
fn arquivo_sem_conexoes_avisa_e_nao_abre_a_previa() {
    let pasta = Pasta::nova("importar-vazio");
    let arquivo = escrever(
        &pasta,
        "vazio.xml",
        "<FileZilla3><Servers></Servers></FileZilla3>",
    );
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    assert!(!m.app.is_open("importar-dialogo"));
    assert!(
        m.app
            .text("aviso-atencao")
            .contains("Não achei nenhuma conexão")
    );
}

#[test]
fn arquivo_que_nao_e_do_programa_mostra_o_erro() {
    let pasta = Pasta::nova("importar-errado");
    let arquivo = escrever(&pasta, "outro.xml", "isto não é XML");
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    assert!(!m.app.is_open("importar-dialogo"));
    assert!(m.app.visible("aviso-erro"));
    assert!(m.app.text("aviso-erro").contains("Não consegui importar"));
}

#[test]
fn com_o_cofre_bloqueado_as_senhas_ficam_de_fora_e_a_caixa_avisa() {
    let pasta = Pasta::nova("importar-bloqueado");
    pasta.cofre_com_senha_mestra("mestra");
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("cofre-desbloquear-close");
    importar_de(&mut m.app, "filezilla", &arquivo);
    assert!(m.app.visible("importar-senhas"));
    assert!(!m.app.enabled("importar-senhas"));
    assert!(!m.app.checked("importar-senhas"));
    assert!(m.app.visible("importar-cofre-bloqueado"));
    m.app.click("importar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 3);
}

#[test]
fn o_formato_sem_senhas_esconde_a_caixa() {
    let pasta = Pasta::nova("importar-sem-caixa");
    let arquivo = escrever(
        &pasta,
        "config.reg",
        "Windows Registry Editor Version 5.00\r\n\r\n\
         [HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions\\web]\r\n\
         \"HostName\"=\"web.exemplo\"\r\n\"Protocol\"=\"ssh\"\r\n\"PortNumber\"=dword:00000016\r\n\
         \"UserName\"=\"deploy\"\r\n",
    );
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "putty", &arquivo);
    assert!(m.app.is_open("importar-dialogo"));
    assert!(!m.app.visible("importar-senhas"));
    m.app.click("importar-dialogo-primary");
    let tree = pasta.conexoes();
    assert_eq!(tree.connection_count(), 1);
    assert_eq!(tree.connections()[0].host, "web.exemplo");
    assert_eq!(tree.connections()[0].user, "deploy");
}

#[test]
fn a_configuracao_desliga_a_caixa_das_senhas_por_padrao() {
    let pasta = Pasta::nova("importar-config-senhas");
    std::fs::create_dir_all(pasta.caminho()).unwrap();
    std::fs::write(
        pasta.caminho().join("configuracoes.json"),
        r#"{"pagina_inicial":"terminais","conexoes":{"importar_senhas":false}}"#,
    )
    .unwrap();
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar_pura(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    assert!(m.app.visible("importar-senhas"));
    assert!(!m.app.checked("importar-senhas"));
}

// --- Exportar ---

fn com_conexoes(nome: &str) -> Pasta {
    let pasta = Pasta::nova(nome);
    pasta.semear(&exemplo());
    pasta
}

#[test]
fn exportar_para_o_winscp_grava_o_ini_com_as_senhas_do_cofre() {
    let pasta = Pasta::nova("exportar-winscp");
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    m.app.click("importar-dialogo-primary");

    let destino = pasta.caminho().join("WinSCP.ini");
    abrir_exportar(&mut m.app, "winscp");
    assert!(m.app.is_open("exportar-dialogo"));
    assert!(m.app.enabled("exportar-senhas"));
    assert!(m.app.checked("exportar-senhas"));
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("exportar-dialogo-primary");
    let pedidos = m.app.file_dialogs();
    assert_eq!(
        pedidos.last().unwrap().file_name.as_deref(),
        Some("WinSCP.ini")
    );

    let texto = std::fs::read_to_string(&destino).unwrap();
    assert!(
        texto.contains("[Sessions\\Importado%20de%20FileZilla/Clientes/ACME%20FTP]"),
        "{texto}"
    );
    assert!(texto.contains("HostName=ftp.acme.com"), "{texto}");
    assert!(
        texto.contains("Password="),
        "a senha do cofre saiu embaralhada: {texto}"
    );
    assert!(!texto.contains("senra"), "a senha não sai em texto puro");
    let mensagem = m.app.text("aviso-sucesso");
    assert!(mensagem.contains("Salvei"), "{mensagem}");
}

#[test]
fn exportar_sem_marcar_as_senhas_nao_leva_senha() {
    let pasta = Pasta::nova("exportar-sem-senhas");
    let arquivo = escrever(&pasta, "sitemanager.xml", FILEZILLA);
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "filezilla", &arquivo);
    m.app.click("importar-dialogo-primary");

    let destino = pasta.caminho().join("fz.xml");
    abrir_exportar(&mut m.app, "filezilla");
    m.app.click("exportar-senhas");
    assert!(!m.app.checked("exportar-senhas"));
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("exportar-dialogo-primary");
    let texto = std::fs::read_to_string(&destino).unwrap();
    assert!(texto.contains("<Host>ftp.acme.com</Host>"), "{texto}");
    assert!(!texto.contains("<Pass"), "{texto}");
}

#[test]
fn formato_sem_senhas_desabilita_a_caixa_e_diz_por_que() {
    let pasta = com_conexoes("exportar-formato-sem-senhas");
    let mut m = montar(&pasta, Theme::Light);
    abrir_exportar(&mut m.app, "putty");
    assert!(m.app.is_open("exportar-dialogo"));
    assert!(!m.app.enabled("exportar-senhas"));
    assert!(!m.app.checked("exportar-senhas"));
    assert!(m.app.visible("exportar-sem-senhas"));
}

#[test]
fn cofre_bloqueado_desabilita_as_senhas_na_exportacao() {
    let pasta = com_conexoes("exportar-bloqueado");
    pasta.cofre_com_senha_mestra("mestra");
    let mut m = montar(&pasta, Theme::Light);
    m.app.click("cofre-desbloquear-close");
    abrir_exportar(&mut m.app, "winscp");
    assert!(!m.app.enabled("exportar-senhas"));
    assert!(m.app.visible("exportar-cofre-bloqueado"));
}

#[test]
fn sem_chave_do_cofre_a_caixa_das_chaves_fica_desabilitada() {
    let pasta = com_conexoes("exportar-sem-chaves");
    let mut m = montar(&pasta, Theme::Light);
    abrir_exportar(&mut m.app, "winscp");
    assert!(!m.app.enabled("exportar-chaves"));
    assert!(m.app.visible("exportar-nenhuma-chave"));
}

#[test]
fn o_reg_exportado_e_utf16_com_bom_e_o_importador_do_putty_le_de_volta() {
    let pasta = com_conexoes("exportar-reg");
    let mut m = montar(&pasta, Theme::Light);
    let destino = pasta.caminho().join("putty.reg");
    abrir_exportar(&mut m.app, "putty");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("exportar-dialogo-primary");

    let bytes = std::fs::read(&destino).unwrap();
    assert_eq!(
        &bytes[..2],
        [0xFF, 0xFE],
        "UTF-16 LE com BOM, como o regedit"
    );
    let texto = importar::texto_do_arquivo(&bytes);
    assert!(
        texto.starts_with("Windows Registry Editor Version 5.00"),
        "{texto}"
    );
    let formato = importar::formato("putty").unwrap();
    let lido = formato.analisar(&texto, None).unwrap();
    let ssh = exemplo()
        .connections()
        .into_iter()
        .filter(|c| {
            matches!(
                c.kind,
                tirech_omni::modelo::Kind::Ssh | tirech_omni::modelo::Kind::Telnet
            )
        })
        .count();
    assert_eq!(lido.total(), ssh);
}

#[test]
fn exportar_tudo_em_json_volta_pelo_importador_nativo() {
    let pasta = Pasta::nova("exportar-nativo");
    pasta.semear_cofre(vec![item_ssh("srv", "root", "segredo")]);
    pasta.semear(&exemplo());
    let mut m = montar(&pasta, Theme::Light);
    let destino = pasta.caminho().join("tudo.json");
    abrir_exportar(&mut m.app, "nativo");
    assert!(m.app.enabled("exportar-senhas"));
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("exportar-dialogo-primary");
    assert!(destino.is_file());
    let texto = std::fs::read_to_string(&destino).unwrap();
    let lido = importar::formato("nativo")
        .unwrap()
        .analisar(&texto, None)
        .unwrap();
    assert_eq!(lido.total(), exemplo().connection_count());
}

#[test]
fn cancelar_o_seletor_da_exportacao_nao_grava_nada() {
    let pasta = com_conexoes("exportar-cancelar");
    let mut m = montar(&pasta, Theme::Light);
    abrir_exportar(&mut m.app, "winscp");
    m.app.file_dialog_cancel();
    m.app.click("exportar-dialogo-primary");
    assert!(!pasta.caminho().join("WinSCP.ini").exists());
}

// --- mRemoteNG com senha-mestra própria ---

/// Um `confCons.xml` do mRemoteNG com a senha-mestra `mestra` e uma conexão com senha.
fn arquivo_do_mremoteng(mestra: &str) -> String {
    use tirech_omni::importar::cripto::{PROTEGIDO_SIM, mremoteng_cifrar};
    let cifrar = |t: &str| mremoteng_cifrar(t, mestra, 1000, [1; 16], [2; 16]).unwrap();
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<Connections Name="Connections" Export="False" BlockCipherMode="GCM" KdfIterations="1000" Protected="{}" ConfVersion="2.6">
  <Node Name="Banco" Type="Connection" Hostname="db.exemplo.com" Protocol="SSH2" Port="22" Username="root" Password="{}" />
</Connections>"#,
        cifrar(PROTEGIDO_SIM),
        cifrar("senha-do-banco"),
    )
}

#[test]
fn mremoteng_com_senha_mestra_propria_pergunta_errou_reabre_e_acertou_segue() {
    let pasta = Pasta::nova("importar-mremoteng-mestra");
    let arquivo = escrever(&pasta, "confCons.xml", &arquivo_do_mremoteng("outra"));
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "mremoteng", &arquivo);
    assert!(m.app.is_open("importar-mestra-dialogo"));
    assert!(!m.app.is_open("importar-dialogo"));
    assert!(!m.app.visible("importar-mestra-erro"));

    m.app.type_into("importar-mestra-senha", "errada");
    m.app.click("importar-mestra-dialogo-primary");
    assert!(m.app.is_open("importar-mestra-dialogo"), "reabre ao errar");
    assert!(m.app.visible("importar-mestra-erro"));

    m.app.type_into("importar-mestra-senha", "outra");
    m.app.click("importar-mestra-dialogo-primary");
    assert!(!m.app.is_open("importar-mestra-dialogo"));
    assert!(m.app.is_open("importar-dialogo"));
    assert!(m.app.visible("importar-senhas"));
    m.app.click("importar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 1);
    assert_eq!(titulos_do_cofre(&pasta), ["root@db.exemplo.com"]);
}

#[test]
fn mremoteng_sem_a_senha_mestra_importa_as_conexoes_sem_senha() {
    let pasta = Pasta::nova("importar-mremoteng-sem-senha");
    let arquivo = escrever(&pasta, "confCons.xml", &arquivo_do_mremoteng("outra"));
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "mremoteng", &arquivo);
    m.app.click("importar-mestra-dialogo-secondary");
    assert!(m.app.is_open("importar-dialogo"));
    assert!(!m.app.visible("importar-senhas"), "nenhuma senha veio");
    m.app.click("importar-dialogo-primary");
    assert_eq!(pasta.conexoes().connection_count(), 1);
    assert!(titulos_do_cofre(&pasta).is_empty());
}

#[test]
fn cancelar_a_senha_mestra_nao_importa_nada() {
    let pasta = Pasta::nova("importar-mremoteng-cancelar");
    let arquivo = escrever(&pasta, "confCons.xml", &arquivo_do_mremoteng("outra"));
    let mut m = montar(&pasta, Theme::Light);
    importar_de(&mut m.app, "mremoteng", &arquivo);
    m.app.click("importar-mestra-dialogo-close");
    assert!(!m.app.is_open("importar-mestra-dialogo"));
    assert!(!m.app.is_open("importar-dialogo"));
    assert_eq!(pasta.conexoes().connection_count(), 0);
}

#[test]
fn exportar_leva_a_chave_do_cofre_numa_pasta_ao_lado_do_arquivo() {
    use tirech_omni::modelo::{Connection, Kind, Tree};
    let pasta = Pasta::nova("exportar-chave-do-cofre");
    let (item, _) = item_chave_ssh("deploy", false);
    pasta.semear_cofre(vec![item]);
    let uid = pasta.cofre().itens()[0].uid.clone();
    let mut arvore = Tree::default();
    let mut web = Connection::new("web", Kind::Sftp);
    web.host = "web.exemplo".into();
    web.user = "deploy".into();
    web.chave_do_cofre = Some(uid);
    arvore.add_connection(None, web);
    pasta.semear(&arvore);
    let mut m = montar(&pasta, Theme::Light);
    let destino = pasta.caminho().join("fz.xml");
    abrir_exportar(&mut m.app, "filezilla");
    assert!(m.app.enabled("exportar-chaves"));
    assert!(m.app.checked("exportar-chaves"), "ligada por padrão");
    m.app.file_dialog_answer(Some(destino.clone()));
    m.app.click("exportar-dialogo-primary");

    let chaves = pasta.caminho().join("fz-chaves");
    let arquivos: Vec<_> = std::fs::read_dir(&chaves).unwrap().collect();
    assert_eq!(
        arquivos.len(),
        1,
        "uma chave gravada em {}",
        chaves.display()
    );
    let texto = std::fs::read_to_string(&destino).unwrap();
    assert!(texto.contains("<Keyfile>"), "{texto}");
    assert!(texto.contains("fz-chaves"), "{texto}");
    assert!(m.app.text("aviso-atencao").contains("sem passphrase"));
}
