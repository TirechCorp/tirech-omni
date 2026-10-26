//! Os textos da interface em mais de um idioma (M43), pelo Project Fluent.
//!
//! Os `.ftl` ficam em `crates/tirech-omni/i18n/<idioma>/` e entram no binário. O pt-BR é a fonte e
//! a reserva: uma chave que falta no idioma cai nele, e se faltar também lá o texto sai como
//! `[chave]`, bem visível. Os testes impedem que isso chegue a quem usa.
//!
//! O idioma é **um por processo**: o `main.rs` chama [`iniciar`] antes de montar a tela, e todas as
//! threads (redes, SFTP, editor) montam texto no idioma certo sem receber nada. Os testes usam
//! [`fixar`], que vale só na thread do teste, então testes paralelos não se misturam, e quem não
//! fixa vê pt-BR.
//!
//! ```ignore
//! let nome = t!("arquivos-nova-conexao");
//! let aviso = t!("arquivos-selecionados", n = 3);
//! let dica = t!("inicio-ferramenta-configuracoes.dica");
//! ```

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

use crate::configuracoes::Idioma;

/// Os textos de um idioma, por arquivo.
type Fontes = &'static [(&'static str, &'static str)];

/// Uma coleção de textos de um idioma.
type Pacote = FluentBundle<FluentResource>;

/// Declara os arquivos `.ftl`: para cada um, o texto de cada idioma entra no binário. **Esquecer
/// um arquivo num idioma é erro de compilação**, porque a lista é uma só.
macro_rules! recursos {
    ($($nome:ident),+ $(,)?) => {
        /// Os arquivos do pt-BR.
        const PT_BR: Fontes = &[
            $( (stringify!($nome), include_str!(concat!("../i18n/pt-BR/", stringify!($nome), ".ftl"))) ),+
        ];
        /// Os arquivos do en-US.
        const EN_US: Fontes = &[
            $( (stringify!($nome), include_str!(concat!("../i18n/en-US/", stringify!($nome), ".ftl"))) ),+
        ];
        /// Os arquivos do espanhol.
        const ES_ES: Fontes = &[
            $( (stringify!($nome), include_str!(concat!("../i18n/es-ES/", stringify!($nome), ".ftl"))) ),+
        ];
    };
}

recursos!(
    geral,
    configuracoes,
    formatos,
    lib,
    comum,
    casca,
    terminais,
    arquivos,
    cofre,
    chaves,
    redes,
    notas,
    erros,
    logs,
    comandos,
    ia,
    mcp,
    nebula,
    openvpn,
    wireguard,
    tailscale,
    zerotier,
    portas,
    assistentes,
    instalacao,
);

/// Os `.ftl` de um idioma já resolvido (`PtBr`, `EnUs` ou `EsEs`).
pub fn fontes(idioma: Idioma) -> Fontes {
    match idioma {
        Idioma::EnUs => EN_US,
        Idioma::EsEs => ES_ES,
        _ => PT_BR,
    }
}

/// A marca de cada idioma no Fluent (para o plural e os números).
fn etiqueta(idioma: Idioma) -> LanguageIdentifier {
    match idioma {
        Idioma::EnUs => "en-US",
        Idioma::EsEs => "es-ES",
        _ => "pt-BR",
    }
    .parse()
    .expect("etiqueta de idioma válida")
}

/// Monta o pacote de um idioma a partir de `.ftl` quaisquer. Os erros vão para o `stderr` (uma
/// vez, ao montar): o pacote sai com o que deu para ler.
pub fn montar(idioma: Idioma, fontes: &[(&str, &str)]) -> Pacote {
    let mut pacote = FluentBundle::new_concurrent(vec![etiqueta(idioma)]);
    // Sem isso o Fluent cerca cada argumento com U+2068/U+2069, que aparecem como quadradinhos
    // e quebram a medida do texto.
    pacote.set_use_isolating(false);
    for (nome, texto) in fontes {
        let recurso = FluentResource::try_new((*texto).to_owned()).unwrap_or_else(|(r, erros)| {
            eprintln!("i18n: erros em {nome}.ftl ({idioma:?}): {erros:?}");
            r
        });
        if let Err(erros) = pacote.add_resource(recurso) {
            eprintln!("i18n: mensagens repetidas em {nome}.ftl ({idioma:?}): {erros:?}");
        }
    }
    pacote
}

/// Os pacotes de cada idioma, montados uma vez por processo.
fn pacote(idioma: Idioma) -> Arc<Pacote> {
    static PT: OnceLock<Arc<Pacote>> = OnceLock::new();
    static EN: OnceLock<Arc<Pacote>> = OnceLock::new();
    static ES: OnceLock<Arc<Pacote>> = OnceLock::new();
    let (celula, qual) = match idioma {
        Idioma::EnUs => (&EN, Idioma::EnUs),
        Idioma::EsEs => (&ES, Idioma::EsEs),
        _ => (&PT, Idioma::PtBr),
    };
    Arc::clone(celula.get_or_init(|| Arc::new(montar(qual, fontes(qual)))))
}

/// Os textos de um idioma, com o pt-BR de reserva.
pub struct Traducoes {
    /// O idioma (nunca `Sistema`).
    idioma: Idioma,
    /// O que estava configurado quando os textos foram montados (pode ser `Sistema`).
    configurado: Idioma,
    /// Faz o texto sair como `⟦texto⟧`, para o teste achar o que não passa pelo `t!`.
    pseudo: bool,
    /// O pacote do idioma.
    principal: Arc<Pacote>,
    /// O pacote do pt-BR.
    reserva: Arc<Pacote>,
}

impl Traducoes {
    /// Os textos de um idioma do processo (`Sistema` vira o pt-BR).
    fn de(configurado: Idioma) -> Self {
        let idioma = resolver(configurado, None);
        Self {
            idioma,
            configurado,
            pseudo: false,
            principal: pacote(idioma),
            reserva: pacote(Idioma::PtBr),
        }
    }

    /// Os textos de um pacote qualquer, para os testes das regras de reserva.
    #[doc(hidden)]
    pub fn de_pacotes(idioma: Idioma, principal: Pacote, reserva: Pacote) -> Self {
        Self {
            idioma,
            configurado: idioma,
            pseudo: false,
            principal: Arc::new(principal),
            reserva: Arc::new(reserva),
        }
    }

    /// O texto da `chave` (`mensagem` ou `mensagem.atributo`).
    pub fn texto(&self, chave: &str, args: Option<&FluentArgs>) -> String {
        let achado = self.cru(chave, args);
        if self.pseudo {
            format!("⟦{achado}⟧")
        } else {
            achado
        }
    }

    /// O texto da `chave` sem a marca do pseudoidioma (para o que não é texto de tela).
    fn cru(&self, chave: &str, args: Option<&FluentArgs>) -> String {
        formatar(&self.principal, chave, args)
            .or_else(|| formatar(&self.reserva, chave, args))
            .unwrap_or_else(|| format!("[{chave}]"))
    }

    /// O idioma destes textos.
    pub fn idioma(&self) -> Idioma {
        self.idioma
    }
}

/// O texto de `chave` num pacote, ou `None` se a mensagem ou o atributo não existem.
fn formatar(pacote: &Pacote, chave: &str, args: Option<&FluentArgs>) -> Option<String> {
    let (mensagem, atributo) = chave
        .split_once('.')
        .map_or((chave, None), |(m, a)| (m, Some(a)));
    let mensagem = pacote.get_message(mensagem)?;
    let padrao = match atributo {
        Some(a) => mensagem.get_attribute(a)?.value(),
        None => mensagem.value()?,
    };
    let mut erros = Vec::new();
    let texto = pacote.format_pattern(padrao, args, &mut erros);
    if !erros.is_empty() {
        avisar_erro(chave, &format!("{erros:?}"));
    }
    Some(texto.into_owned())
}

/// Escreve no `stderr` um erro de formatação, uma vez por chave.
fn avisar_erro(chave: &str, erro: &str) {
    static JA_DITOS: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let mut ditos = JA_DITOS.lock().unwrap_or_else(PoisonError::into_inner);
    if ditos.insert(chave.to_owned()) {
        eprintln!("i18n: erro ao formatar {chave}: {erro}");
    }
}

/// Qual idioma uma escolha dá, dado o idioma do sistema (`pt-BR`, `en_US.UTF-8`, `C`…).
///
/// - `pt*` dá pt-BR; `en*` dá en-US; `es*` dá o espanhol;
/// - qualquer outro idioma conhecido do sistema dá en-US, o mais entendido;
/// - sem idioma (`None`) dá pt-BR, que é o caso dos testes.
pub fn resolver(escolha: Idioma, sistema: Option<&str>) -> Idioma {
    match escolha {
        Idioma::PtBr => Idioma::PtBr,
        Idioma::EnUs => Idioma::EnUs,
        Idioma::EsEs => Idioma::EsEs,
        Idioma::Sistema => {
            let Some(sistema) = sistema.map(str::trim).filter(|s| !s.is_empty()) else {
                return Idioma::PtBr;
            };
            let minusculas = sistema.to_ascii_lowercase();
            // "C" e "POSIX" são o idioma de quem não configurou nada.
            if minusculas.starts_with("pt") || matches!(minusculas.as_str(), "c" | "posix") {
                Idioma::PtBr
            } else if minusculas.starts_with("es") {
                Idioma::EsEs
            } else {
                Idioma::EnUs
            }
        }
    }
}

static PROCESSO: OnceLock<Arc<Traducoes>> = OnceLock::new();

thread_local! {
    /// O idioma fixado na thread, só para os testes.
    static FIXADO: RefCell<Option<Arc<Traducoes>>> = const { RefCell::new(None) };
}

/// Escolhe o idioma do processo. Chame uma vez, no `main`, antes de montar a tela: `configurado`
/// é o `geral.idioma` e `sistema` o idioma do sistema (`sys_locale::get_locale()`). Também
/// entrega à lib os textos dela (`set_strings`), nesta thread, que é a da interface.
pub fn iniciar(configurado: Idioma, sistema: Option<&str>) {
    let mut traducoes = Traducoes::de(resolver(configurado, sistema));
    traducoes.configurado = configurado;
    let traducoes = Arc::new(traducoes);
    tomni_ui::set_strings(strings_da_lib(&traducoes));
    // Já iniciado (um segundo `iniciar`): vale o primeiro.
    let _ = PROCESSO.set(traducoes);
}

/// Entrega à lib os textos dela **nesta thread**. A thread da interface pode não ser a do `main`:
/// quem monta a tela chama isto antes de criar os widgets.
pub fn entregar_a_lib() {
    tomni_ui::set_strings(strings_da_lib(&atuais()));
}

/// Fixa o idioma **desta thread**, para os testes (`Sistema` vira pt-BR). Também troca os textos
/// da lib nesta thread.
#[doc(hidden)]
pub fn fixar(idioma: Idioma) {
    let traducoes = Arc::new(Traducoes::de(idioma));
    tomni_ui::set_strings(strings_da_lib(&traducoes));
    FIXADO.with(|f| *f.borrow_mut() = Some(traducoes));
}

/// Como [`fixar`], mas todo texto sai `⟦texto⟧` (o pt-BR entre marcas), para o teste achar o que
/// ainda não passa pelo `t!`.
#[doc(hidden)]
pub fn fixar_pseudo() {
    let mut traducoes = Traducoes::de(Idioma::PtBr);
    traducoes.pseudo = true;
    let traducoes = Arc::new(traducoes);
    tomni_ui::set_strings(strings_da_lib(&traducoes));
    FIXADO.with(|f| *f.borrow_mut() = Some(traducoes));
}

/// Desfaz o [`fixar`] desta thread (volta ao do processo, ou ao pt-BR).
#[doc(hidden)]
pub fn desfixar() {
    FIXADO.with(|f| *f.borrow_mut() = None);
    tomni_ui::set_strings(strings_da_lib(&atuais()));
}

/// Os textos valendo agora: os fixados na thread, os do processo ou os do pt-BR.
fn atuais() -> Arc<Traducoes> {
    if let Some(fixados) = FIXADO.with(|f| f.borrow().clone()) {
        return fixados;
    }
    if let Some(processo) = PROCESSO.get() {
        return Arc::clone(processo);
    }
    static PADRAO: OnceLock<Arc<Traducoes>> = OnceLock::new();
    Arc::clone(PADRAO.get_or_init(|| Arc::new(Traducoes::de(Idioma::Sistema))))
}

/// O idioma valendo agora (`PtBr`, `EnUs` ou `EsEs`).
pub fn atual() -> Idioma {
    atuais().idioma()
}

/// O `geral.idioma` que valia quando os textos foram montados: a página Configurações compara
/// com o de agora para avisar que a troca pede reiniciar.
pub fn configurado() -> Idioma {
    atuais().configurado
}

/// O texto da `chave` no idioma atual.
pub fn texto(chave: &str, args: Option<&FluentArgs>) -> String {
    atuais().texto(chave, args)
}

/// O texto da `chave` num idioma qualquer (a busca de Configurações junta o atual e o pt-BR).
pub fn texto_em(idioma: Idioma, chave: &str, args: Option<&FluentArgs>) -> String {
    let atuais = atuais();
    if atuais.idioma() == resolver(idioma, None) && !atuais.pseudo {
        return atuais.texto(chave, args);
    }
    Traducoes::de(idioma).texto(chave, args)
}

/// Um número com `casas` casas decimais e o separador do idioma (`1,2` no pt-BR e `1.2` no en-US).
/// Não use o `NUMBER()` do Fluent para isso: ele não bate com os formatos que o app sempre teve.
pub fn decimal(valor: f64, casas: usize) -> String {
    let base = format!("{valor:.casas$}");
    let separador = atuais().cru(crate::tk!("formatos-decimal"), None);
    base.replace('.', &separador)
}

/// Os textos da lib no idioma de `t`, para o `set_strings`.
fn strings_da_lib(t: &Traducoes) -> tomni_ui::Strings {
    let mut s = tomni_ui::Strings::default();
    let tx = |chave: &str| t.texto(chave, None);
    s.tab_close = tx("lib-tab-close").into();
    s.tab_add = tx("lib-tab-add").into();
    s.tab_scroll_left = tx("lib-tab-scroll-left").into();
    s.tab_scroll_right = tx("lib-tab-scroll-right").into();
    s.dock_pin = tx("lib-dock-pin").into();
    s.dock_unpin = tx("lib-dock-unpin").into();
    s.dock_float = tx("lib-dock-float").into();
    s.dock_dock = tx("lib-dock-dock").into();
    s.dock_close = tx("lib-dock-close").into();
    s.dock_maximize = tx("lib-dock-maximize").into();
    s.dock_restore = tx("lib-dock-restore").into();
    s.dock_options = tx("lib-dock-options").into();
    s.dock_side_left = tx("lib-dock-side-left").into();
    s.dock_side_right = tx("lib-dock-side-right").into();
    s.dock_side_bottom = tx("lib-dock-side-bottom").into();
    s.back = tx("lib-back").into();
    s.navigation_close = tx("lib-navigation-close").into();
    s.navigation_open = tx("lib-navigation-open").into();
    s.navigation_search = tx("lib-navigation-search").into();
    s.navigation_settings = tx("lib-navigation-settings").into();
    s.minimize = tx("lib-minimize").into();
    s.maximize = tx("lib-maximize").into();
    s.restore = tx("lib-restore").into();
    s.close = tx("lib-close").into();
    s.show_password = tx("lib-show-password").into();
    s.increase = tx("lib-increase").into();
    s.decrease = tx("lib-decrease").into();
    s.clear = tx("lib-clear").into();
    s.query = tx("lib-query").into();
    s.toggle_on = tx("lib-toggle-on").into();
    s.toggle_off = tx("lib-toggle-off").into();
    s.sort_alphabetical = tx("lib-sort-alphabetical").into();
    s.paste_title = tx("lib-paste-title").into();
    s.paste_warning = tx("lib-paste-warning").into();
    // A lib troca `{count}`, `{n}` e `{reason}` pelo valor; o Fluent só entrega o texto cru.
    s.paste_warning_many = com_marca(t, "lib-paste-warning-many", "count").into();
    s.paste_anyway = tx("lib-paste-anyway").into();
    s.cancel = tx("lib-cancel").into();
    s.pane_empty = tx("lib-pane-empty").into();
    s.pane_close = tx("lib-pane-close").into();
    s.pane_name = com_marca(t, "lib-pane-name", "n").into();
    s.pane_move_to = tx("lib-pane-move-to").into();
    s.pane_new_right = tx("lib-pane-new-right").into();
    s.pane_new_below = tx("lib-pane-new-below").into();
    s.pane_restart = tx("lib-pane-restart").into();
    s.pane_close_tab = tx("lib-pane-close-tab").into();
    s.connection_closed = tx("lib-connection-closed").into();
    s.connection_closed_reason = com_marca(t, "lib-connection-closed-reason", "reason").into();
    s.page_failed = tx("lib-page-failed").into();
    s.try_again = tx("lib-try-again").into();
    s.tab_progress_working = tx("lib-tab-progress-working").into();
    s.tab_progress_paused = tx("lib-tab-progress-paused").into();
    s.tab_progress_error = tx("lib-tab-progress-error").into();
    s.tab_attention = tx("lib-tab-attention").into();
    s.tab_connecting = tx("lib-tab-connecting").into();
    s.tree_status_success = tx("lib-tree-status-success").into();
    s.tree_status_caution = tx("lib-tree-status-caution").into();
    s.tree_status_critical = tx("lib-tree-status-critical").into();
    s.tree_status_neutral = tx("lib-tree-status-neutral").into();
    s.terminal_copy = tx("lib-terminal-copy").into();
    s.terminal_paste = tx("lib-terminal-paste").into();
    s.terminal_link_open = tx("lib-terminal-link-open").into();
    s.terminal_link_copy = tx("lib-terminal-link-copy").into();
    s.terminal_link_hint = com_marca(t, "lib-terminal-link-hint", "action").into();
    s.terminal_link_no_open = tx("lib-terminal-link-no-open").into();
    s
}

/// Um texto da lib que leva um valor: o argumento sai como `{nome}`, a marca que a lib troca.
fn com_marca(t: &Traducoes, chave: &str, nome: &'static str) -> String {
    let mut args = FluentArgs::new();
    args.set(nome, format!("{{{nome}}}"));
    t.texto(chave, Some(&args))
}

/// O texto de uma chave no idioma atual: `t!("chave")` ou `t!("chave", n = 3, nome = x.as_str())`.
/// Cada valor é um número, um `&str` ou uma `String` (um `&String` precisa de `.as_str()`).
#[macro_export]
macro_rules! t {
    ($chave:expr $(,)?) => {
        $crate::i18n::texto($chave, ::core::option::Option::None)
    };
    ($chave:expr, $($nome:ident = $valor:expr),+ $(,)?) => {{
        let mut args = ::fluent_bundle::FluentArgs::new();
        $( args.set(stringify!($nome), $valor); )+
        $crate::i18n::texto($chave, ::core::option::Option::Some(&args))
    }};
}

/// A própria chave, **sem traduzir**: marca chaves guardadas em tabelas estáticas (a `escolha!`,
/// o catálogo) para o teste de chaves achá-las pela varredura. Quem usa chama `t!` na hora de
/// mostrar.
#[macro_export]
macro_rules! tk {
    ($chave:literal) => {
        $chave
    };
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um pacote pequeno, de `.ftl` escrito no teste.
    fn pacote_de(idioma: Idioma, ftl: &str) -> Pacote {
        montar(idioma, &[("teste", ftl)])
    }

    #[test]
    fn o_idioma_do_sistema_resolve() {
        let r = |s| resolver(Idioma::Sistema, s);
        assert_eq!(r(Some("pt-BR")), Idioma::PtBr);
        assert_eq!(r(Some("pt_PT")), Idioma::PtBr);
        assert_eq!(r(Some("en-GB")), Idioma::EnUs);
        assert_eq!(r(Some("en_US.UTF-8")), Idioma::EnUs);
        assert_eq!(r(Some("es-ES")), Idioma::EsEs);
        assert_eq!(r(Some("es_MX.UTF-8")), Idioma::EsEs);
        assert_eq!(r(Some("de-DE")), Idioma::EnUs);
        assert_eq!(r(Some("C")), Idioma::PtBr);
        assert_eq!(r(Some("")), Idioma::PtBr);
        assert_eq!(r(None), Idioma::PtBr);
        // Uma escolha explícita ignora o sistema.
        assert_eq!(resolver(Idioma::EnUs, Some("pt-BR")), Idioma::EnUs);
        assert_eq!(resolver(Idioma::PtBr, Some("en-US")), Idioma::PtBr);
        assert_eq!(resolver(Idioma::EsEs, Some("pt-BR")), Idioma::EsEs);
    }

    #[test]
    fn chave_que_falta_cai_no_pt_br_e_depois_na_propria_chave() {
        let en = pacote_de(Idioma::EnUs, "so-no-ingles = Only here\n");
        let pt = pacote_de(
            Idioma::PtBr,
            "so-no-ingles = Só aqui\nso-no-pt = Só no pt-BR\n    .dica = A dica\n",
        );
        let t = Traducoes::de_pacotes(Idioma::EnUs, en, pt);
        assert_eq!(t.texto("so-no-ingles", None), "Only here");
        assert_eq!(t.texto("so-no-pt", None), "Só no pt-BR");
        assert_eq!(t.texto("so-no-pt.dica", None), "A dica");
        assert_eq!(t.texto("nao-existe", None), "[nao-existe]");
        assert_eq!(t.texto("nao-existe.dica", None), "[nao-existe.dica]");
    }

    #[test]
    fn os_argumentos_entram_sem_marcas_de_isolamento() {
        let pt = pacote_de(Idioma::PtBr, "ola = Olá, { $nome }! São { $n } itens.\n");
        let t = Traducoes::de_pacotes(Idioma::PtBr, pt, pacote_de(Idioma::PtBr, ""));
        let mut args = FluentArgs::new();
        args.set("nome", "Ana");
        args.set("n", 3);
        assert_eq!(t.texto("ola", Some(&args)), "Olá, Ana! São 3 itens.");
    }

    #[test]
    fn o_zero_do_pt_br_tem_variante_propria() {
        let ftl =
            "sel = { $n ->\n    [0] Nenhum item\n    [one] 1 item\n   *[other] { $n } itens\n}\n";
        let t = Traducoes::de_pacotes(
            Idioma::PtBr,
            pacote_de(Idioma::PtBr, ftl),
            pacote_de(Idioma::PtBr, ""),
        );
        let com = |n: i32| {
            let mut args = FluentArgs::new();
            args.set("n", n);
            t.texto("sel", Some(&args))
        };
        assert_eq!(com(0), "Nenhum item");
        assert_eq!(com(1), "1 item");
        assert_eq!(com(2), "2 itens");
    }

    #[test]
    fn os_ftl_de_todos_os_idiomas_carregam_sem_erro() {
        for idioma in [Idioma::PtBr, Idioma::EnUs] {
            let mut pacote = FluentBundle::new_concurrent(vec![etiqueta(idioma)]);
            for (nome, texto) in fontes(idioma) {
                let recurso = FluentResource::try_new((*texto).to_owned())
                    .unwrap_or_else(|(_, erros)| panic!("{nome}.ftl ({idioma:?}): {erros:?}"));
                if let Err(erros) = pacote.add_resource(recurso) {
                    panic!("{nome}.ftl ({idioma:?}): {erros:?}");
                }
            }
        }
    }

    #[test]
    fn o_pseudoidioma_cerca_o_texto() {
        fixar_pseudo();
        assert_eq!(texto("formatos-decimal", None), "⟦,⟧");
        // Um número não é texto de tela: sai sem a marca.
        assert_eq!(decimal(1.26, 1), "1,3");
        desfixar();
        assert_eq!(texto("formatos-decimal", None), ",");
    }
}
