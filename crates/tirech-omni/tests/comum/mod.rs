//! O que os testes de tela têm em comum: uma pasta temporária, a árvore de exemplo e a
//! montagem do `TestApp` com o `FakeOpener`.

#![allow(dead_code)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use tirech_omni::abrir::{FakeOpener, FilaChaves, Opener};
use tirech_omni::armazenamento::Armazenamento;
use tirech_omni::cofre::{Cofre, Item, Tipo, cifrar, partes_do_caminho};
use tirech_omni::configuracoes::Idioma;
use tirech_omni::ferramentas::{Ambiente, Ferramentas};
use tirech_omni::i18n;
use tirech_omni::modelo::{Connection, Jump, Kind, Tree};
use tirech_omni::tela::{tela_com_ferramentas, tela_com_ssh};
use tomni_ui::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

/// Uma pasta temporária só deste teste, apagada ao sair de escopo.
pub struct Pasta(pub PathBuf);

impl Pasta {
    pub fn nova(nome: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let pasta = std::env::temp_dir().join(format!(
            "tirech-omni-tela-{}-{}-{nome}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&pasta);
        Self(pasta)
    }

    pub fn caminho(&self) -> &Path {
        &self.0
    }

    /// A pasta das chaves SSH do teste (`ssh/` dentro da pasta temporária), no lugar de `~/.ssh`.
    pub fn ssh(&self) -> PathBuf {
        self.0.join("ssh")
    }

    pub fn armazenamento(&self) -> Armazenamento {
        Armazenamento::new(&self.0)
    }

    /// O `connections.json` lido de volta.
    pub fn conexoes(&self) -> Tree {
        self.armazenamento().carregar().0
    }

    /// O texto do `connections.json`.
    pub fn texto_conexoes(&self) -> String {
        std::fs::read_to_string(self.armazenamento().arquivo_conexoes()).unwrap_or_default()
    }

    /// Grava `itens` no cofre (sem senha mestra) e devolve os ids, na ordem.
    pub fn semear_cofre(&self, itens: Vec<Item>) -> Vec<u64> {
        let (mut cofre, _) = Cofre::abrir(&self.0);
        itens
            .into_iter()
            .map(|i| cofre.salvar(i).expect("gravar o cofre do teste"))
            .collect()
    }

    /// Grava `itens` no cofre, cada um na pasta do caminho que acompanha (`"Clientes/ACME"`, ou
    /// `""` para a raiz), criando as pastas que faltam. Devolve os ids dos itens, na ordem.
    pub fn semear_cofre_em(&self, itens: Vec<(&str, Item)>) -> Vec<u64> {
        let (mut cofre, _) = Cofre::abrir(&self.0);
        itens
            .into_iter()
            .map(|(caminho, mut item)| {
                item.pasta = cofre.garantir_caminho(None, &partes_do_caminho(caminho));
                cofre.salvar(item).expect("gravar o cofre do teste")
            })
            .collect()
    }

    /// O id da pasta de `caminho` (`"Clientes/ACME"`) no cofre gravado.
    pub fn id_da_pasta(&self, caminho: &str) -> u64 {
        let (mut cofre, _) = Cofre::abrir(&self.0);
        cofre
            .garantir_caminho(None, &partes_do_caminho(caminho))
            .unwrap_or_else(|| panic!("a pasta {caminho} não existe"))
    }

    /// Grava na pasta um cofre no formato 1 (o dos nomes `cofre.json` e `chave`, com a pasta
    /// como texto em cada item), sem senha mestra: `itens` são `(pasta, título, senha)`.
    pub fn semear_cofre_v1(&self, itens: &[(&str, &str, &str)]) {
        std::fs::create_dir_all(&self.0).expect("criar a pasta do teste");
        let chave = [7u8; 32];
        let itens: Vec<serde_json::Value> = itens
            .iter()
            .enumerate()
            .map(|(n, (pasta, titulo, senha))| {
                serde_json::json!({
                    "id": n + 1, "tipo": "Senha", "titulo": titulo, "pasta": pasta,
                    "usuario": "", "senha": senha, "url": "", "notas": "", "campos": [],
                    "alterado": 0,
                })
            })
            .collect();
        let claro = serde_json::json!({ "proximo_id": itens.len() + 1, "itens": itens });
        let segredo = cifrar(&chave, claro.to_string().as_bytes()).expect("cifrar o cofre v1");
        let arquivo = serde_json::json!({
            "versao": 1, "protecao": { "tipo": "chave" }, "itens": segredo,
        });
        std::fs::write(self.0.join("cofre.json"), arquivo.to_string()).unwrap();
        std::fs::write(self.0.join("chave"), chave).unwrap();
    }

    /// Põe a senha mestra no cofre que já está na pasta.
    pub fn cofre_com_senha_mestra(&self, senha: &str) {
        let (mut cofre, _) = Cofre::abrir(&self.0);
        cofre
            .definir_senha_mestra(Some(senha))
            .expect("definir a senha mestra do teste");
    }

    /// O texto do `vault.json`.
    pub fn texto_cofre(&self) -> String {
        std::fs::read_to_string(self.0.join("vault.json")).unwrap_or_default()
    }

    /// O cofre lido de volta do disco (sem senha mestra).
    pub fn cofre(&self) -> Cofre {
        Cofre::abrir(&self.0).0
    }

    /// Põe `categoria.campo = valor` no `configuracoes.json`, se o arquivo ainda não tem a
    /// categoria (o app grava todas de uma vez, então uma categoria presente é de quem já rodou
    /// ou de outro padrão do teste).
    fn padrao(&self, categoria: &str, campo: &str, valor: serde_json::Value) {
        let arquivo = self.armazenamento().arquivo_configuracoes();
        let mut raiz: serde_json::Map<String, serde_json::Value> =
            std::fs::read_to_string(&arquivo)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
        if raiz.contains_key(categoria) {
            return;
        }
        raiz.insert(categoria.to_owned(), serde_json::json!({ campo: valor }));
        std::fs::create_dir_all(&self.0).expect("criar a pasta do teste");
        std::fs::write(arquivo, serde_json::to_string(&raiz).unwrap())
            .expect("gravar as configurações do teste");
    }

    /// Põe `categoria.campo = valor` no `configuracoes.json`, mantendo o que a categoria já tem.
    #[allow(dead_code)]
    pub fn configurar(&self, categoria: &str, campo: &str, valor: serde_json::Value) {
        let arquivo = self.armazenamento().arquivo_configuracoes();
        let mut raiz: serde_json::Map<String, serde_json::Value> =
            std::fs::read_to_string(&arquivo)
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
        let entrada = raiz
            .entry(categoria.to_owned())
            .or_insert_with(|| serde_json::json!({}));
        entrada[campo] = valor;
        std::fs::create_dir_all(&self.0).expect("criar a pasta do teste");
        std::fs::write(arquivo, serde_json::to_string(&raiz).unwrap())
            .expect("gravar as configurações do teste");
    }

    /// Grava `geral.pagina_inicial` no `configuracoes.json`, se a pasta ainda não tem o geral.
    pub fn pagina_inicial_padrao(&self, pagina: &str) {
        self.padrao("geral", "pagina_inicial", pagina.into());
    }

    /// Grava `agente.ligado` no `configuracoes.json`, se a pasta ainda não tem o agente. Os
    /// testes sobem a tela várias vezes na mesma pasta e o agente leva um instante para
    /// soltar o socket: por padrão ele fica desligado, e quem o testa liga.
    pub fn agente_padrao(&self, ligado: bool) {
        self.padrao("agente", "ligado", ligado.into());
    }

    /// Grava `tree` como as conexões que o app vai encontrar ao abrir.
    pub fn semear(&self, tree: &Tree) {
        self.armazenamento()
            .salvar(tree)
            .expect("gravar as conexões do teste");
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Os ids são: 1 Produção, 2 web-01, 3 db-01 (com salto), 4 Roteador (Telnet), 5 Shell (local).
/// Os SSH têm chave, para abrirem sem pedir a senha.
pub fn exemplo() -> Tree {
    let mut tree = Tree::default();
    let producao = tree.add_group(None, "Produção");
    let mut web = Connection::new("web-01", Kind::Ssh);
    web.host = "web-01.exemplo".into();
    web.user = "deploy".into();
    web.key_file = Some("/chaves/web".into());
    tree.add_connection(Some(producao), web);
    let mut db = Connection::new("db-01", Kind::Ssh);
    db.host = "10.0.0.20".into();
    db.user = "dba".into();
    db.key_file = Some("/chaves/db".into());
    db.jump = Some(Jump {
        host: "bastiao.exemplo".into(),
        port: 2200,
        user: "ana".into(),
        credencial: None,
    });
    tree.add_connection(Some(producao), db);
    let mut roteador = Connection::new("Roteador", Kind::Telnet);
    roteador.host = "192.168.0.1".into();
    tree.add_connection(None, roteador);
    tree.add_connection(None, Connection::new("Shell", Kind::Local));
    tree
}

/// Um item SSH do cofre.
pub fn item_ssh(titulo: &str, usuario: &str, senha: &str) -> Item {
    Item {
        tipo: Tipo::Ssh,
        titulo: titulo.into(),
        usuario: usuario.into(),
        senha: senha.into(),
        ..Item::default()
    }
}

/// Um item de outro tipo, só com o título.
pub fn item_do_tipo(tipo: Tipo, titulo: &str) -> Item {
    Item {
        tipo,
        titulo: titulo.into(),
        ..Item::default()
    }
}

/// Um SSH sem chave (id 1), que pede a senha ao abrir.
pub fn so_com_senha() -> Tree {
    let mut tree = Tree::default();
    let mut c = Connection::new("sem-chave", Kind::Ssh);
    c.host = "sem-chave.exemplo".into();
    c.user = "ana".into();
    tree.add_connection(None, c);
    tree
}

/// Fixa o idioma **desta thread** (o teste), antes de montar a tela. Não use o
/// `configuracoes.json` para isso: a configuração só vale pelo `main.rs`.
pub fn fixar_idioma(idioma: Idioma) {
    i18n::fixar(idioma);
}

/// A tela montada, o abridor falso e a fila de pedidos de chave do servidor.
pub struct Montagem {
    pub app: TestApp,
    pub opener: FakeOpener,
    pub fila: FilaChaves,
}

/// A tela já na página Terminais (a que os testes do M24 e do M25 usam): sem uma
/// `configuracoes.json` na pasta, grava uma com `pagina_inicial` = "terminais".
pub fn montar(pasta: &Pasta, tema: Theme) -> Montagem {
    pasta.pagina_inicial_padrao("terminais");
    montar_pura(pasta, tema)
}

/// A tela como o app abre: a página ao iniciar vem do estado e das configurações da pasta.
pub fn montar_pura(pasta: &Pasta, tema: Theme) -> Montagem {
    let opener = FakeOpener::new();
    let (app, fila) = montar_com_pura(pasta, tema, opener.clone());
    Montagem { app, opener, fila }
}

/// A tela com um abridor qualquer, na página Terminais; devolve também a fila de pedidos de
/// chave.
pub fn montar_com(
    pasta: &Pasta,
    tema: Theme,
    opener: impl Opener + 'static,
) -> (TestApp, FilaChaves) {
    pasta.pagina_inicial_padrao("terminais");
    montar_com_pura(pasta, tema, opener)
}

/// Como [`montar_com`], sem mexer nas configurações da pasta.
pub fn montar_com_pura(
    pasta: &Pasta,
    tema: Theme,
    opener: impl Opener + 'static,
) -> (TestApp, FilaChaves) {
    pasta.agente_padrao(false);
    let capturada: Rc<RefCell<Option<FilaChaves>>> = Rc::default();
    let captura = Rc::clone(&capturada);
    let caminho = pasta.0.clone();
    let ssh = pasta.ssh();
    let app = TestApp::new(move || {
        tela_com_ssh(caminho, ssh, move |fila| {
            *captura.borrow_mut() = Some(fila);
            Rc::new(opener)
        })
    })
    .size(1280, 720)
    .theme(tema);
    let fila = capturada
        .borrow_mut()
        .take()
        .expect("a tela chama a fábrica do abridor");
    (app, fila)
}

/// A tela com o ambiente ajustado por `ajustar` (Windows forçado, executor que anota os comandos)
/// e as [`Ferramentas`] dela, para mexer no agente. Na página Terminais, como [`montar`].
pub fn montar_com_ambiente(
    pasta: &Pasta,
    tema: Theme,
    ajustar: impl FnOnce(&mut Ambiente),
) -> (Montagem, Rc<Ferramentas>) {
    pasta.pagina_inicial_padrao("terminais");
    pasta.agente_padrao(false);
    let opener = FakeOpener::new();
    let aberto = opener.clone();
    let capturada: Rc<RefCell<Option<FilaChaves>>> = Rc::default();
    let captura = Rc::clone(&capturada);
    let ferramentas: Rc<RefCell<Option<Rc<Ferramentas>>>> = Rc::default();
    let entrega = Rc::clone(&ferramentas);
    let caminho = pasta.0.clone();
    let mut ambiente = Ambiente::de_teste(&caminho, pasta.ssh());
    ajustar(&mut ambiente);
    let app = TestApp::new(move || {
        tela_com_ferramentas(
            caminho,
            ambiente,
            move |fila| {
                *captura.borrow_mut() = Some(fila);
                Rc::new(aberto)
            },
            move |f| *entrega.borrow_mut() = Some(f),
        )
    })
    .size(1280, 720)
    .theme(tema);
    let fila = capturada
        .borrow_mut()
        .take()
        .expect("a tela chama a fábrica do abridor");
    let ferramentas = ferramentas
        .borrow_mut()
        .take()
        .expect("a tela entrega as ferramentas");
    (Montagem { app, opener, fila }, ferramentas)
}

/// Um item de chave SSH do cofre com uma Ed25519 nova; devolve o item e o fingerprint.
pub fn item_chave_ssh(titulo: &str, no_agente: bool) -> (Item, String) {
    use tirech_omni::cofre::ChaveSsh;
    use tomni_ui::terminal::ssh::keys::{KeyFile, KeyGen, KeyType};
    let gerada = KeyGen::new(KeyType::Ed25519)
        .comment(titulo)
        .generate()
        .expect("gerar a chave do teste");
    let aberta =
        KeyFile::from_text(gerada.private_openssh().as_str(), None).expect("abrir a chave");
    let mut chave = ChaveSsh::de_chave(aberta.private_key()).expect("montar a chave");
    chave.no_agente = no_agente;
    let fingerprint = chave.fingerprint.clone();
    (
        Item {
            tipo: Tipo::ChaveSsh,
            titulo: titulo.to_owned(),
            chave: Some(chave),
            ..Item::default()
        },
        fingerprint,
    )
}

pub fn valor(app: &TestApp, nome: &str) -> String {
    app.properties("props")
        .into_iter()
        .find(|p| p.name == nome)
        .unwrap_or_else(|| panic!("sem a propriedade {nome}"))
        .value
}

/// Escolhe a linha como uma pessoa: clica no nome da primeira (o centro da linha é o divisor
/// das colunas) e desce com ↓ até a pedida.
pub fn escolher_propriedade(app: &mut TestApp, nome: &str) {
    escolher_propriedade_em(app, "props", nome);
}

/// Como [`escolher_propriedade`], numa grade qualquer (`grade` é o id dela).
pub fn escolher_propriedade_em(app: &mut TestApp, grade: &str, nome: &str) {
    if app
        .focused()
        .is_some_and(|f| f.starts_with(&format!("{grade}-")))
    {
        app.key(Key::Home);
    } else {
        // A grade pode estar rolada (num painel baixo a linha "Nome" sai de vista): `hover` rola
        // até ela antes de ler a posição.
        app.hover(&format!("{grade}-Nome"));
        let r = app.rect(&format!("{grade}-Nome"));
        app.click_at(
            tomni_ui::kurbo::Point::new(r.x0 + 24.0, (r.y0 + r.y1) / 2.0),
            1,
        );
    }
    for _ in 0..80 {
        let escolhida = app
            .properties(grade)
            .into_iter()
            .find(|p| p.selected)
            .map(|p| p.name);
        if escolhida.as_deref() == Some(nome) {
            return;
        }
        app.key(Key::Down);
    }
    let linhas: Vec<_> = app
        .properties(grade)
        .into_iter()
        .map(|p| (p.name, p.selected))
        .collect();
    panic!("a propriedade {nome} não foi escolhida: {linhas:?}");
}

/// Troca o valor de uma propriedade de texto ou número: Enter abre o editor, Enter confirma.
pub fn editar_propriedade(app: &mut TestApp, nome: &str, texto: &str) {
    editar_propriedade_em(app, "props", nome, texto);
}

/// Como [`editar_propriedade`], numa grade qualquer.
pub fn editar_propriedade_em(app: &mut TestApp, grade: &str, nome: &str, texto: &str) {
    escolher_propriedade_em(app, grade, nome);
    app.key(Key::Enter);
    let atual = app.text(&format!("{grade}-{nome}-valor"));
    app.key(Key::End);
    for _ in 0..atual.chars().count() {
        app.key(Key::Backspace);
    }
    app.type_text(texto).key(Key::Enter);
}

/// Um Windows de mentira para a instalação (M65): caminhos de `C:\Users\ana` e o estado de
/// cada item dado, em vez de ler o disco. Os testes visuais não dependem da máquina.
pub fn ambiente_windows_falso(
    a: &mut Ambiente,
    estados: Vec<(
        tirech_omni::instalacao::ItemDeInstalacao,
        tirech_omni::instalacao::EstadoDoItem,
    )>,
) {
    a.windows = true;
    a.pessoal = Some(PathBuf::from("C:/Users/ana"));
    a.local_app_data = Some(PathBuf::from("C:/Users/ana/AppData/Local"));
    a.app_data = Some(PathBuf::from("C:/Users/ana/AppData/Roaming"));
    a.area_de_trabalho = Some(PathBuf::from("C:/Users/ana/Desktop"));
    a.menu_iniciar = Some(PathBuf::from(
        "C:/Users/ana/AppData/Roaming/Microsoft/Windows/Start Menu/Programs",
    ));
    a.exe = Some(PathBuf::from("C:/Users/ana/Downloads/tirech-omni.exe"));
    a.chave_uninstall = tirech_omni::instalacao::CHAVE_UNINSTALL.to_owned();
    a.instalacao_fixa = estados;
}
