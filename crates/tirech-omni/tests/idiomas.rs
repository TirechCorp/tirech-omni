//! A interface em outros idiomas (M43): as chaves dos `.ftl`, os formatos, a troca de idioma e a
//! varredura dos textos que ainda não passam pelo `t!`.

mod comum;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tirech_omni::arquivos::formato;
use tirech_omni::configuracoes::Idioma;
use tirech_omni::configuracoes::catalogo::{CATALOGO, Tipo};
use tirech_omni::i18n;
use tirech_omni::recentes;
use tirech_omni::reinicio;
use tomni_ui::prelude::Key;
use tomni_ui::theme::Theme;
use tomni_ui_test::TestApp;

use comum::{Montagem, Pasta, fixar_idioma, montar, montar_pura};

// ---------------------------------------------------------------------------------------------
// Os `.ftl`: leitura simples, bloco a bloco (o app não depende do `regex` nem do `fluent-syntax`)
// ---------------------------------------------------------------------------------------------

/// Para cada chave de um idioma (`mensagem` ou `mensagem.atributo`), as `$variaveis` do texto.
fn chaves_do_idioma(idioma: Idioma) -> BTreeMap<String, BTreeSet<String>> {
    let mut chaves: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (arquivo, texto) in i18n::fontes(idioma) {
        let mut atual: Option<String> = None;
        let mut mensagem = String::new();
        for (n, linha) in texto.lines().enumerate() {
            let aparada = linha.trim_start();
            if aparada.is_empty() || aparada.starts_with('#') {
                continue;
            }
            let recuada = linha.starts_with(' ') || linha.starts_with('\t');
            if !recuada {
                let Some((id, resto)) = linha.split_once('=') else {
                    panic!("{arquivo}.ftl:{}: linha solta: {linha}", n + 1);
                };
                mensagem = id.trim().to_owned();
                atual = Some(mensagem.clone());
                let antes = chaves.insert(mensagem.clone(), variaveis(resto));
                assert!(antes.is_none(), "{arquivo}.ftl: {mensagem} repetida");
            } else if let Some(atributo) = atributo_da_linha(aparada) {
                let chave = format!("{mensagem}.{}", atributo.0);
                atual = Some(chave.clone());
                let antes = chaves.insert(chave.clone(), variaveis(atributo.1));
                assert!(antes.is_none(), "{arquivo}.ftl: {chave} repetida");
            } else if let Some(chave) = &atual {
                chaves
                    .get_mut(chave)
                    .expect("a chave atual existe")
                    .extend(variaveis(aparada));
            }
        }
    }
    chaves
}

/// `.nome = texto` → `("nome", " texto")`.
fn atributo_da_linha(linha: &str) -> Option<(&str, &str)> {
    let resto = linha.strip_prefix('.')?;
    let (nome, texto) = resto.split_once('=')?;
    let nome = nome.trim();
    let valido = !nome.is_empty()
        && nome
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valido.then_some((nome, texto))
}

/// Os `$nome` de um trecho de `.ftl`.
fn variaveis(trecho: &str) -> BTreeSet<String> {
    let mut achadas = BTreeSet::new();
    let mut resto = trecho;
    while let Some(i) = resto.find('$') {
        resto = &resto[i + 1..];
        let fim = resto
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(resto.len());
        if fim > 0 {
            achadas.insert(resto[..fim].to_owned());
        }
    }
    achadas
}

/// Os idiomas além do pt-BR, que é a fonte.
const TRADUCOES: [Idioma; 2] = [Idioma::EnUs, Idioma::EsEs];

#[test]
fn os_idiomas_tem_as_mesmas_chaves_e_atributos() {
    let pt: BTreeSet<_> = chaves_do_idioma(Idioma::PtBr).into_keys().collect();
    for idioma in TRADUCOES {
        let outro: BTreeSet<_> = chaves_do_idioma(idioma).into_keys().collect();
        let so_no_pt: Vec<_> = pt.difference(&outro).collect();
        let so_no_outro: Vec<_> = outro.difference(&pt).collect();
        assert!(
            so_no_pt.is_empty() && so_no_outro.is_empty(),
            "só no pt-BR: {so_no_pt:?}\nsó no {idioma:?}: {so_no_outro:?}"
        );
    }
}

#[test]
fn os_argumentos_batem_entre_os_idiomas() {
    let pt = chaves_do_idioma(Idioma::PtBr);
    for idioma in TRADUCOES {
        let outro = chaves_do_idioma(idioma);
        let diferentes: Vec<_> = pt
            .iter()
            .filter(|(chave, vars)| outro.get(*chave).is_some_and(|outras| outras != *vars))
            .map(|(chave, vars)| format!("{chave}: pt-BR {vars:?}, {idioma:?} {:?}", outro[chave]))
            .collect();
        assert!(diferentes.is_empty(), "{}", diferentes.join("\n"));
    }
}

// ---------------------------------------------------------------------------------------------
// Os fontes: as chaves usadas e os textos fixos
// ---------------------------------------------------------------------------------------------

/// Os `.rs` de `src/`, com o caminho relativo a ele (`tela/arquivos.rs`).
fn fontes() -> Vec<(String, String)> {
    fn andar(pasta: &Path, raiz: &Path, saida: &mut Vec<(String, String)>) {
        let mut entradas: Vec<PathBuf> = std::fs::read_dir(pasta)
            .expect("lê a pasta")
            .map(|e| e.expect("entrada").path())
            .collect();
        entradas.sort();
        for caminho in entradas {
            if caminho.is_dir() {
                andar(&caminho, raiz, saida);
            } else if caminho.extension().is_some_and(|e| e == "rs") {
                let relativo = caminho
                    .strip_prefix(raiz)
                    .expect("dentro de src")
                    .to_string_lossy()
                    .replace('\\', "/");
                let texto = std::fs::read_to_string(&caminho).expect("lê o fonte");
                saida.push((relativo, sem_testes(&texto)));
            }
        }
    }
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut saida = Vec::new();
    andar(&raiz, &raiz, &mut saida);
    saida
}

/// O fonte sem os itens `#[cfg(test)]` (que começam na coluna 0) e sem os comentários de linha
/// (`//`, `///`, `//!`). As linhas apagadas ficam vazias, para os números de linha continuarem os
/// do arquivo.
fn sem_testes(texto: &str) -> String {
    let linhas: Vec<&str> = texto.lines().collect();
    let mut saida = String::new();
    let mut i = 0;
    while i < linhas.len() {
        let linha = linhas[i];
        if linha.trim_end() == "#[cfg(test)]" {
            // Pula o item: até o `}` da coluna 0, ou até a linha que termina em `;`.
            saida.push('\n');
            i += 1;
            while i < linhas.len() {
                let l = linhas[i];
                i += 1;
                saida.push('\n');
                if l == "}" || (l.ends_with(';') && !l.starts_with(' ')) {
                    break;
                }
            }
            continue;
        }
        if !linha.trim_start().starts_with("//") {
            saida.push_str(linha);
        }
        saida.push('\n');
        i += 1;
    }
    saida
}

/// O literal de texto que começa em `resto` (depois de espaços), ou `None`. Devolve o texto sem
/// aspas e o que sobra depois dele.
fn literal(resto: &str) -> Option<(&str, &str)> {
    let resto = resto.trim_start().strip_prefix('"')?;
    let mut anterior_barra = false;
    for (i, c) in resto.char_indices() {
        match c {
            '\\' => anterior_barra = !anterior_barra,
            '"' if !anterior_barra => return Some((&resto[..i], &resto[i + 1..])),
            _ => anterior_barra = false,
        }
    }
    None
}

/// As chaves de `t!("…")` e `tk!("…")` num fonte.
fn chaves_usadas(texto: &str) -> BTreeSet<String> {
    let mut achadas = BTreeSet::new();
    for marca in ["t!(", "tk!("] {
        let mut de = 0;
        while let Some(i) = texto[de..].find(marca) {
            let inicio = de + i;
            de = inicio + marca.len();
            // `format!(` também termina em `t!(`: o caractere antes tem de ser de fora de uma palavra.
            let antes = texto[..inicio].chars().next_back();
            if antes.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            if let Some((chave, _)) = literal(&texto[de..]) {
                achadas.insert(chave.to_owned());
            }
        }
    }
    achadas
}

/// Todas as chaves que o app usa: as dos fontes, as do catálogo de Configurações e as `lib-*`.
fn todas_as_usadas() -> BTreeSet<String> {
    let mut usadas = BTreeSet::new();
    for (_, texto) in fontes() {
        usadas.extend(chaves_usadas(&texto));
    }
    for d in CATALOGO.iter() {
        let id = d.id();
        usadas.insert(format!("{id}.descricao"));
        usadas.insert(format!("{id}.palavras"));
        usadas.insert(id);
        // O resumo de cada seção da sanfona; o rodapé (`config-secao-fim`) não é sanfona.
        if d.secao != "config-secao-fim" {
            usadas.insert(format!("{}.resumo", d.secao));
        }
    }
    usadas.insert("config-titulo-atalhos.resumo".to_owned());
    usadas
}

/// Se a chave é de uma das `lib-*`: a ponte para a lib as lê pela tabela de `strings_da_lib`.
fn e_da_lib(chave: &str) -> bool {
    chave.starts_with("lib-")
}

#[test]
fn toda_chave_usada_existe() {
    let idiomas = [Idioma::PtBr, Idioma::EnUs, Idioma::EsEs].map(chaves_do_idioma);
    let faltam: Vec<_> = todas_as_usadas()
        .into_iter()
        .filter(|c| idiomas.iter().any(|chaves| !chaves.contains_key(c)))
        .collect();
    assert!(faltam.is_empty(), "chaves sem texto: {faltam:?}");
}

#[test]
fn nao_sobra_chave_sem_uso() {
    let usadas = todas_as_usadas();
    let mensagens_usadas: BTreeSet<&str> = usadas
        .iter()
        .map(|c| c.split_once('.').map_or(c.as_str(), |(m, _)| m))
        .collect();
    let sobras: Vec<_> = chaves_do_idioma(Idioma::PtBr)
        .into_keys()
        .map(|c| c.split_once('.').map_or(c.clone(), |(m, _)| m.to_owned()))
        // Um termo (`-app`) não é chamado por `t!`: as mensagens o citam por `{ -app }`.
        .filter(|m| !m.starts_with('-') && !e_da_lib(m) && !mensagens_usadas.contains(m.as_str()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert!(sobras.is_empty(), "chaves sem uso: {sobras:?}");
}

#[test]
fn o_catalogo_tem_titulo_descricao_e_palavras_em_todos_os_idiomas() {
    for idioma in [Idioma::PtBr, Idioma::EnUs, Idioma::EsEs] {
        fixar_idioma(idioma);
        for d in CATALOGO.iter().filter(|d| d.tipo != Tipo::Grupo) {
            for texto in [d.titulo(), d.descricao(), d.palavras()] {
                assert!(
                    !texto.starts_with('[') && !texto.is_empty(),
                    "{} ({idioma:?}): {texto}",
                    d.chave
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Formatos
// ---------------------------------------------------------------------------------------------

#[test]
fn numeros_e_tamanhos_seguem_o_idioma() {
    fixar_idioma(Idioma::PtBr);
    assert_eq!(i18n::decimal(1.26, 1), "1,3");
    assert_eq!(formato::tamanho(1_258_291), "1,2 MB");
    assert_eq!(formato::tamanho(512), "512 B");
    fixar_idioma(Idioma::EnUs);
    assert_eq!(i18n::decimal(1.26, 1), "1.3");
    assert_eq!(formato::tamanho(1_258_291), "1.2 MB");
    assert_eq!(formato::tamanho(512), "512 B");
    fixar_idioma(Idioma::EsEs);
    assert_eq!(i18n::decimal(1.26, 1), "1,3");
    assert_eq!(formato::tamanho(1_258_291), "1,2 MB");
}

#[test]
fn datas_seguem_o_idioma() {
    // 2026-03-05 14:07:00 UTC.
    let momento = 1_772_719_620;
    fixar_idioma(Idioma::PtBr);
    assert_eq!(recentes::data_e_hora(momento), "05/03/2026 14:07");
    assert_eq!(recentes::data(momento), "05/03/2026");
    fixar_idioma(Idioma::EnUs);
    assert_eq!(recentes::data_e_hora(momento), "03/05/2026 14:07");
    assert_eq!(recentes::data(momento), "03/05/2026");
    fixar_idioma(Idioma::EsEs);
    assert_eq!(recentes::data_e_hora(momento), "05/03/2026 14:07");
    assert_eq!(recentes::data(momento), "05/03/2026");
}

#[test]
fn o_tempo_dos_recentes_segue_o_idioma() {
    const MIN: u64 = 60;
    const DIA: u64 = 86_400;
    let agora = 10 * DIA;
    let casos = |ultimo: u64| recentes::quando(ultimo, agora);
    fixar_idioma(Idioma::PtBr);
    assert_eq!(casos(agora - 10), "agora mesmo");
    assert_eq!(casos(agora - MIN), "há 1 min");
    assert_eq!(casos(agora - 5 * MIN), "há 5 min");
    assert_eq!(casos(agora - DIA - 60), "ontem");
    assert_eq!(casos(agora - 4 * DIA), "há 4 dias");
    fixar_idioma(Idioma::EnUs);
    assert_eq!(casos(agora - 10), "just now");
    assert_eq!(casos(agora - MIN), "1 min ago");
    assert_eq!(casos(agora - 5 * MIN), "5 min ago");
    assert_eq!(casos(agora - DIA - 60), "yesterday");
    assert_eq!(casos(agora - 4 * DIA), "4 days ago");
}

// ---------------------------------------------------------------------------------------------
// Idioma e reinício
// ---------------------------------------------------------------------------------------------

fn existe(app: &mut TestApp, id: &str) -> bool {
    app.ui().find(id).is_some()
}

fn aparece(app: &mut TestApp, id: &str) -> bool {
    existe(app, id) && app.visible(id)
}

/// Abre o grupo (o `SettingsExpander`) que guarda o controle `id`, se ele ainda não aparece.
fn alcancar(app: &mut TestApp, id: &str) {
    for grupo in CATALOGO.iter().filter(|d| d.tipo == Tipo::Grupo) {
        if aparece(app, id) {
            return;
        }
        let cabecalho = format!("{}-cabecalho", grupo.id());
        if existe(app, &cabecalho) {
            app.click(&cabecalho);
        }
    }
    assert!(aparece(app, id), "{id:?} não aparece na página");
}

#[test]
fn trocar_o_idioma_mostra_o_aviso_e_reinicia() {
    let pasta = Pasta::nova("idioma-troca");
    let mut m = montar_pura(&pasta, Theme::Light);
    m.app.click("inicio-ferramenta-configuracoes");
    m.app.click("config-secao-geral");
    alcancar(&mut m.app, "config-geral-idioma-valor");
    assert!(!m.app.visible("config-geral-idioma-aviso"));

    m.app.select("config-geral-idioma-valor", "English");
    assert!(m.app.visible("config-geral-idioma-aviso"));
    assert_eq!(
        m.app.text("config-geral-idioma-aviso"),
        "O idioma novo vale depois de reiniciar o Tirech Omni."
    );
    let gravado =
        std::fs::read_to_string(pasta.armazenamento().arquivo_configuracoes()).unwrap_or_default();
    assert!(gravado.contains("\"idioma\": \"en_us\""), "{gravado}");
    // A interface continua no idioma de antes até reiniciar.
    assert_eq!(
        m.app.label("config-geral-idioma-reiniciar"),
        "Reiniciar agora"
    );

    // Voltar ao idioma que valia faz o aviso sumir.
    m.app
        .select("config-geral-idioma-valor", "Usar o idioma do sistema");
    assert!(!m.app.visible("config-geral-idioma-aviso"));

    // Reiniciar agora pede o fechamento pelo caminho normal e o reinício.
    m.app.select("config-geral-idioma-valor", "English");
    assert!(!reinicio::pedido());
    m.app.click("config-geral-idioma-reiniciar");
    assert!(reinicio::pedido());
}

#[test]
fn com_o_ingles_fixado_a_casca_sai_em_ingles() {
    fixar_idioma(Idioma::EnUs);
    let pasta = Pasta::nova("idioma-en-casca");
    let mut m = montar(&pasta, Theme::Light);
    assert_eq!(m.app.label("menus-file"), "File");
    assert_eq!(m.app.label("menus-tools"), "Tools");
    assert_eq!(m.app.label("menus-help"), "Help");
    // Os ids dos menus vêm do título: em inglês são menus-file, menus-tools e menus-help.
    // A dica do ✕ da aba vem da lib.
    m.app.click("menus-tools").click("abrir-shell");
    m.app.frame();
    let arvore = m.app.tree();
    let fechar = descendentes(&arvore)
        .into_iter()
        .find(|n| n.kind == "tab_button" && n.label.as_deref() == Some("Close tab"));
    assert!(fechar.is_some(), "sem o botão \"Close tab\"");
}

#[test]
fn os_testes_sem_fixar_continuam_em_pt_br() {
    let pasta = Pasta::nova("idioma-pt-casca");
    let m = montar_pura(&pasta, Theme::Light);
    assert_eq!(m.app.label("menus-arquivo"), "Arquivo");
    assert_eq!(m.app.label("menus-ajuda"), "Ajuda");
}

fn descendentes(no: &tomni_ui::Node) -> Vec<&tomni_ui::Node> {
    let mut todos = vec![no];
    for filho in &no.children {
        todos.extend(descendentes(filho));
    }
    todos
}

// ---------------------------------------------------------------------------------------------
// Pseudoidioma: toda a tela passa pelo t!
// ---------------------------------------------------------------------------------------------

/// Os textos de tela de uma árvore que **não** vieram do `t!` (sem a marca `⟦ ⟧` do
/// pseudoidioma): `(id do nó mais próximo, tipo, texto)`.
fn sem_marca(arvore: &tomni_ui::Node) -> Vec<(String, &'static str, String)> {
    fn andar(no: &tomni_ui::Node, id_pai: &str, saida: &mut Vec<(String, &'static str, String)>) {
        // O que o terminal mostra não é da interface.
        if no.kind == "terminal" {
            return;
        }
        let id = no.id.clone().unwrap_or_else(|| id_pai.to_owned());
        if no.visible {
            // O valor da navegação e da árvore é a página ou a pasta escolhida (um id), não texto.
            let campos: Vec<&Option<String>> = if matches!(no.kind, "navigation_view" | "tree_view")
            {
                vec![&no.title, &no.placeholder]
            } else {
                vec![&no.text, &no.label, &no.title, &no.placeholder]
            };
            for texto in campos.into_iter().flatten() {
                if !texto.contains('⟦') && e_texto_de_tela(texto) && !e_dado_do_teste(texto) {
                    saida.push((id.clone(), no.kind, texto.clone()));
                }
            }
        }
        for filho in &no.children {
            andar(filho, &id, saida);
        }
    }
    let mut saida = Vec::new();
    andar(arvore, "", &mut saida);
    saida.sort();
    saida.dedup();
    saida
}

/// Os dados que os testes semeiam (nomes de conexões, hosts, caminhos): não são da interface.
fn e_dado_do_teste(texto: &str) -> bool {
    const DADOS: &[&str] = &[
        "Produção",
        "web-01",
        "db-01",
        "Roteador",
        "Shell",
        "web-01.exemplo",
        "10.0.0.20",
        "192.168.0.1",
        "deploy",
        "dba",
        "tirech-omni",
        // A marca (o nome e as partes do selo da barra de título) não se traduz.
        "Tirech Omni",
        "Tirech",
        "Omni",
        "SSH",
        "Roteador (192.168.0.1)",
        "/chaves/web",
        "us-east-1",
        "xterm-256color",
        "Amazon S3",
        // Comandos que o app propõe por padrão: são do sistema, não da interface.
        "sh -s",
        "bash",
        "virt-manager --no-fork",
        // A pasta padrão do servidor para o que se cola (M45): um caminho, não texto.
        "/tmp/tirech-colar",
        // O nome padrão da chave nova no cofre é o nome do arquivo do tipo: não se traduz.
        "id_ed25519",
    ];
    DADOS.contains(&texto.trim())
        || texto.contains("web-01")
        || texto.contains("db-01")
        || texto.contains("10.0.0.20")
        // Caminhos da pasta temporária, o socket do agente e o comentário da chave gerada.
        || texto.contains("/tmp/tirech-omni-")
        || texto.starts_with("SSH_AUTH_SOCK=")
        || texto.starts_with("tirech@")
}

#[test]
fn toda_a_tela_passa_pelo_t() {
    i18n::fixar_pseudo();
    let pasta = Pasta::nova("idioma-pseudo");
    pasta.semear(&comum::exemplo());
    let mut m = montar(&pasta, Theme::Light);
    // Cada achado aparece uma vez, na primeira tela em que surge.
    let mut achados: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut conferir = |m: &Montagem, tela: &str| {
        for (id, tipo, texto) in sem_marca(&m.app.tree()) {
            achados
                .entry((id.clone(), texto.clone()))
                .or_insert_with(|| format!("{tela}: {id} ({tipo}): {texto}"));
        }
    };

    // Terminais: a árvore com uma conexão escolhida, os quatro menus e o menu da árvore.
    m.app.expand("arvore", "1");
    m.app.click("arvore-2");
    conferir(&m, "terminais");
    // Os ids dos menus vêm do título; no pseudoidioma não são os de sempre, então vêm da árvore.
    let menus: Vec<String> = descendentes(&m.app.tree())
        .into_iter()
        .filter(|n| n.kind == "menu_bar_item" && n.visible)
        .filter_map(|n| n.id.clone())
        .collect();
    assert!(menus.len() >= 5, "menus da barra: {menus:?}");
    for id in menus {
        m.app.click(&id);
        conferir(&m, &format!("menu {id}"));
        m.app.key(Key::Escape);
    }
    m.app.right_click("arvore-2");
    conferir(&m, "menu da árvore");
    m.app.key(Key::Escape);

    // As outras páginas.
    for pagina in [
        "inicio",
        "arquivos",
        "cofre",
        "chaves",
        "agente",
        "redes",
        "ia",
        "notas",
        "configuracoes",
    ] {
        m.app.click(&format!("navegacao-{pagina}"));
        conferir(&m, pagina);
    }

    assert!(
        achados.is_empty(),
        "texto que não passa pelo t! ({}):\n{}",
        achados.len(),
        achados.values().cloned().collect::<Vec<_>>().join("\n")
    );
}

// ---------------------------------------------------------------------------------------------
// Varredura: nenhum texto fixo na interface
// ---------------------------------------------------------------------------------------------

/// Os arquivos que ainda não passaram pela extração do M43. **Precisa ficar vazia no fim do
/// marco.** Um arquivo sai da lista quando a varredura não acha mais nada nele (o teste cobra).
const PENDENTES: &[&str] = &[];

/// Nomes próprios que valem nos dois idiomas.
const NOMES_PROPRIOS: &[&str] = &[
    "Tirech Omni",
    "SSH",
    "SFTP",
    "FTP",
    "FTPS",
    "S3",
    "rclone",
    "Telnet",
    "Passphrase",
    "JSON",
    "Markdown",
    "gitk",
    "db.interno:5432",
    "/srv/app",
    "/phpmyadmin/",
    "loja.exemplo.com.br",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
];

/// Chamadas cujo primeiro argumento é um texto de tela.
const CHAMADAS: &[&str] = &[
    "Text::new(",
    "Button::new(",
    "ContentDialog::new(",
    "SettingsCard::new(",
    ".message(",
    ".title(",
    ".label(",
    ".tooltip(",
    ".placeholder(",
    ".header(",
    ".description(",
    ".primary(",
    ".secondary(",
    ".close(",
    ".menu(",
    ".filter(",
    ".feed(",
];

/// Chamadas `(id, texto)`: o texto de tela é o segundo literal.
const CHAMADAS_COM_ID: &[&str] = &[".item(", ".toggle(", ".submenu(", ".tab("];

/// Tira do começo `format!(`, `&format!(`, `&` e `String::from(`, para chegar ao literal.
fn sem_envolturas(mut resto: &str) -> &str {
    loop {
        let r = resto.trim_start();
        let seguinte = ["&format!(", "format!(", "String::from(", "&"]
            .iter()
            .find_map(|p| r.strip_prefix(p));
        match seguinte {
            Some(s) => resto = s,
            None => return r,
        }
    }
}

/// Se o literal é um texto de tela que deveria passar pelo `t!`.
fn e_texto_de_tela(literal: &str) -> bool {
    // O que está entre chaves é argumento do `format!`, não texto; `\r`, `\n` e `\t` também não.
    let literal = literal
        .replace("\\r", "")
        .replace("\\n", "")
        .replace("\\t", "");
    let mut fora = String::new();
    let mut dentro = false;
    for c in literal.chars() {
        match c {
            '{' => dentro = true,
            '}' => dentro = false,
            _ if !dentro => fora.push(c),
            _ => {}
        }
    }
    let letras: String = fora.chars().filter(|c| c.is_alphabetic()).collect();
    if letras.is_empty() {
        return false;
    }
    // Unidades e siglas curtas ("B", "KB", "OK").
    if letras.chars().count() <= 2 && letras.chars().all(char::is_uppercase) {
        return false;
    }
    !NOMES_PROPRIOS.contains(&literal.trim())
}

/// Os textos fixos de um fonte: `(linha, texto)`.
fn textos_fixos(texto: &str) -> Vec<(usize, String)> {
    let mut achados = Vec::new();
    let linha_de = |posicao: usize| texto[..posicao].matches('\n').count() + 1;
    for chamada in CHAMADAS {
        let mut de = 0;
        while let Some(i) = texto[de..].find(chamada) {
            let inicio = de + i;
            de = inicio + chamada.len();
            if let Some((lit, _)) = literal(sem_envolturas(&texto[de..]))
                && e_texto_de_tela(lit)
            {
                achados.push((linha_de(inicio), lit.to_owned()));
            }
        }
    }
    for chamada in CHAMADAS_COM_ID {
        let mut de = 0;
        while let Some(i) = texto[de..].find(chamada) {
            let inicio = de + i;
            de = inicio + chamada.len();
            let Some((_, depois)) = literal(&texto[de..]) else {
                continue;
            };
            let Some(depois) = depois.trim_start().strip_prefix(',') else {
                continue;
            };
            if let Some((lit, _)) = literal(sem_envolturas(depois))
                && e_texto_de_tela(lit)
            {
                achados.push((linha_de(inicio), lit.to_owned()));
            }
        }
    }
    achados.sort();
    achados.dedup();
    achados
}

#[test]
fn nenhum_texto_fixo_na_interface() {
    let mut achados = Vec::new();
    let mut pendentes_com_texto = BTreeSet::new();
    for (arquivo, texto) in fontes() {
        let fixos = textos_fixos(&texto);
        if PENDENTES.contains(&arquivo.as_str()) {
            if !fixos.is_empty() {
                pendentes_com_texto.insert(arquivo);
            }
            continue;
        }
        for (linha, texto) in fixos {
            achados.push(format!("{arquivo}:{linha}: {texto}"));
        }
    }
    assert!(
        achados.is_empty(),
        "texto fixo na interface ({}): troque por t!(\"…\") e ponha a chave nos .ftl\n{}",
        achados.len(),
        achados.join("\n")
    );
    let limpos: Vec<_> = PENDENTES
        .iter()
        .filter(|p| !pendentes_com_texto.contains(**p))
        .collect();
    assert!(
        limpos.is_empty(),
        "já sem texto fixo, tire de PENDENTES: {limpos:?}"
    );
}
