//! A página Início: o destaque com as ações principais, as conexões fixadas e recentes e os
//! atalhos para as ferramentas. Só desenha: quem abre, fixa e remove são as [`AcoesInicio`],
//! que a tela liga aos dados.

use std::rc::Rc;

use tomni_ui::prelude::*;

use crate::marca::NOME_DO_APP;
use crate::recentes::Alvo;
use crate::t;

/// Um cartão de recente, já com o texto pronto.
#[derive(Clone, Debug, PartialEq)]
pub struct Cartao {
    /// A posição entre os fixados ou entre os não fixados (o `n` do id `inicio-recente-{n}`).
    pub indice: usize,
    /// O que o cartão abre.
    pub alvo: Alvo,
    /// O ícone do tipo.
    pub icone: Symbol,
    /// O nome.
    pub titulo: String,
    /// `usuario@host:porta`, a pasta, etc.
    pub detalhe: String,
    /// Há quanto tempo foi o último uso ("há 5 min").
    pub quando: String,
    /// Se está fixado no Início.
    pub fixado: bool,
}

/// Fixar (`true`) ou desafixar um alvo.
type Fixar = Rc<dyn Fn(&Alvo, bool)>;

/// Acrescenta ao menu de um cartão o submenu "Abrir em": recebe o menu, o alvo e o prefixo dos
/// ids (o do menu) e devolve o menu com o item (ou como veio, se o alvo não abre num terminal).
type AbrirEm = Rc<dyn Fn(MenuFlyout, &Alvo, &str) -> MenuFlyout>;

/// O que os botões e os menus do Início fazem.
#[derive(Clone)]
pub struct AcoesInicio {
    /// Nova conexão.
    pub nova: Rc<dyn Fn()>,
    /// Importar do `~/.ssh/config`.
    pub importar: Rc<dyn Fn()>,
    /// Abrir um shell local.
    pub shell: Rc<dyn Fn()>,
    /// Abrir o alvo do cartão e ir para os Terminais.
    pub abrir: Rc<dyn Fn(&Alvo)>,
    /// O submenu "Abrir em", depois do "Abrir".
    pub abrir_em: AbrirEm,
    /// Fixar (`true`) ou desafixar o alvo.
    pub fixar: Fixar,
    /// Escolher o alvo na árvore, nos Terminais.
    pub mostrar: Rc<dyn Fn(&Alvo)>,
    /// Tirar o alvo dos recentes.
    pub remover: Rc<dyn Fn(&Alvo)>,
}

/// Largura de leitura da página, como a da Gallery.
const LARGURA_MAXIMA: f64 = 1200.0;

/// Altura do cartão: a grade fica regular.
const ALTURA_DO_CARTAO: f64 = 72.0;

/// O fundo do destaque, na diagonal: o ciano-azul da Gallery no claro e o azul-petróleo no
/// escuro.
fn fundo_do_destaque() -> Brush {
    let cor = |claro: u32, escuro: u32| ThemeColor::themed(Color::hex(claro), Color::hex(escuro));
    Brush::linear(
        (0.0, 0.0),
        (1.0, 1.0),
        [
            (0.0, cor(0xD4F1F9, 0x0E3B4C)),
            (0.5, cor(0xA9DDF3, 0x123A63)),
            (1.0, cor(0x8CC4EE, 0x1C2A4A)),
        ],
    )
}

/// O quadrado do ícone do cartão: a cor accent a 15% por trás de um ícone accent.
fn selo(icone: Symbol) -> Border {
    let fundo = ThemeColor::themed(
        Color::rgba(0x00, 0x5F, 0xB8, 38),
        Color::rgba(0x60, 0xCD, 0xFF, 38),
    );
    Border::new()
        .width(40)
        .height(40)
        .corner_radius(4)
        .background(fundo)
        .child(
            Column::new()
                .grow(1)
                .align(Align::Center)
                .justify(Justify::Center)
                .child(
                    Icon::new(icone)
                        .size(20)
                        .color(Token::AccentFillColorDefault)
                        .decorative(),
                ),
        )
}

/// O corpo de um cartão: o selo do ícone, o título e o detalhe (este com o id `{id}-detalhe`).
/// A página Arquivos usa o mesmo cartão.
pub(crate) fn cartao_base(id: String, icone: Symbol, titulo: &str, detalhe: &str) -> Card {
    let detalhe = Text::new(detalhe)
        .id(format!("{id}-detalhe"))
        .caption()
        .secondary()
        .max_lines(2);
    Card::new()
        .id(id)
        .padding(12)
        .height(ALTURA_DO_CARTAO)
        .label(titulo)
        .child(
            Row::new()
                .gap(16)
                .align(Align::Center)
                .child(selo(icone))
                .child(
                    Column::new()
                        .grow(1)
                        .child(Text::new(titulo).body_strong())
                        .child(detalhe),
                ),
        )
}

/// Um cartão de ferramenta: leva para a página `pagina` da navegação ao ser clicado. Tem o id
/// `inicio-ferramenta-{pagina}`.
pub fn cartao_ferramenta(
    icone: Symbol,
    titulo: &str,
    descricao: &str,
    pagina: &'static str,
    destino: Signal<String>,
) -> Card {
    cartao_base(
        format!("inicio-ferramenta-{pagina}"),
        icone,
        titulo,
        descricao,
    )
    .on_click(move || destino.set(pagina.to_owned()))
}

/// O cartão de um recente, com o menu de contexto.
fn cartao_recente(c: &Cartao, acoes: &AcoesInicio) -> Card {
    let id = if c.fixado {
        format!("inicio-fixada-{}", c.indice)
    } else {
        format!("inicio-recente-{}", c.indice)
    };
    let detalhe = format!("{} · {}", c.detalhe, c.quando);
    let item = |nome: &str| format!("{id}-menu-{nome}");

    let abrir = {
        let (alvo, acoes) = (c.alvo.clone(), acoes.clone());
        move || (acoes.abrir)(&alvo)
    };
    let mut menu = MenuFlyout::new().id(format!("{id}-menu")).item(
        item("abrir"),
        t!("comum-abrir"),
        abrir.clone(),
    );
    menu = (acoes.abrir_em)(menu.icon(Symbol::Open), &c.alvo, &format!("{id}-menu"));
    let fixar = {
        let (alvo, acoes, fixado) = (c.alvo.clone(), acoes.clone(), c.fixado);
        move || (acoes.fixar)(&alvo, !fixado)
    };
    menu = if c.fixado {
        menu.item(item("fixar"), t!("inicio-desafixar"), fixar)
            .icon(Symbol::PinOff)
    } else {
        menu.item(item("fixar"), t!("inicio-fixar-no-inicio"), fixar)
            .icon(Symbol::Pin)
    };
    if matches!(c.alvo, Alvo::Conexao { .. }) {
        let (alvo, acoes) = (c.alvo.clone(), acoes.clone());
        menu = menu
            .item(item("arvore"), t!("inicio-mostrar-na-arvore"), move || {
                (acoes.mostrar)(&alvo)
            })
            .icon(Symbol::FolderOpen);
    }
    let (alvo, remover) = (c.alvo.clone(), Rc::clone(&acoes.remover));
    menu = menu
        .separator()
        .item(
            item("remover"),
            t!("inicio-remover-dos-recentes"),
            move || remover(&alvo),
        )
        .icon(Symbol::Delete);

    cartao_base(id, c.icone, &c.titulo, &detalhe)
        .on_click(abrir)
        .context_flyout(menu)
}

/// A chave de um cartão na lista: tudo o que aparece nele, para o cartão ser refeito quando
/// algo muda.
fn chave(c: &Cartao) -> String {
    format!(
        "{:?}|{}|{}|{}|{}|{}",
        c.alvo, c.indice, c.titulo, c.detalhe, c.quando, c.fixado
    )
}

/// A grade de cartões dos recentes (os fixados ou os não fixados).
fn grade(cartoes: Rc<dyn Fn() -> Vec<Cartao>>, acoes: AcoesInicio, fixados: bool) -> UniformGrid {
    let lista = move || -> Vec<Cartao> {
        cartoes()
            .into_iter()
            .filter(|c| c.fixado == fixados)
            .collect()
    };
    UniformGrid::new()
        .min_column_width(320)
        .gap(8)
        .child(For::each(lista, chave, move |c: Cartao| {
            cartao_recente(&c, &acoes)
        }))
}

/// A faixa de destaque da Gallery: o rótulo pequeno, o nome, a descrição, a coluna de botões
/// e o ícone grande. O Início e a página Arquivos (sem abas) usam.
pub(crate) fn faixa(
    rotulo: Text,
    titulo: &str,
    descricao: &str,
    botoes: Column,
    icone: Symbol,
) -> Border {
    Border::new()
        .background(fundo_do_destaque())
        .corner_radius(8)
        .height(220)
        .padding(40)
        .child(
            Row::new()
                .gap(32)
                .align(Align::Center)
                .child(
                    Column::new()
                        .grow(1)
                        .gap(4)
                        .child(rotulo.caption())
                        .child(Text::new(titulo).title_large())
                        .child(Text::new(descricao).body_large().wrap()),
                )
                .child(botoes.width(240).gap(4))
                .child(
                    Icon::new(icone)
                        .size(160)
                        .color(Token::TextFillColorTertiary)
                        .decorative(),
                ),
        )
}

/// O destaque do alto: versão, nome, descrição, as três ações e o ícone grande.
fn destaque(acoes: &AcoesInicio) -> Border {
    let botao = |id: &str, texto: &str, icone: Symbol, acao: &Rc<dyn Fn()>| {
        let acao = Rc::clone(acao);
        Button::new(texto)
            .id(id)
            .icon(icone)
            .on_click(move || acao())
    };
    faixa(
        Text::new(t!("inicio-versao", versao = env!("CARGO_PKG_VERSION"))).id("inicio-versao"),
        NOME_DO_APP,
        &t!("inicio-destaque-descricao"),
        Column::new()
            .child(
                botao(
                    "inicio-novo",
                    &t!("comum-nova-conexao"),
                    Symbol::Add,
                    &acoes.nova,
                )
                .accent(),
            )
            .child(botao(
                "inicio-importar",
                &t!("inicio-importar-ssh-config"),
                Symbol::ArrowDownload,
                &acoes.importar,
            ))
            .child(botao(
                "inicio-shell",
                &t!("inicio-abrir-shell-local"),
                Symbol::WindowConsole,
                &acoes.shell,
            )),
        Symbol::WindowConsole,
    )
}

/// Título de uma seção.
pub(crate) fn secao(titulo: impl Into<String>) -> Text {
    Text::new(titulo.into()).subtitle()
}

/// A página Início. `cartoes` devolve os recentes de agora (lê os signals, então a página se
/// refaz quando eles mudam); `ferramentas` são os cartões da última seção.
pub fn pagina_inicio(
    cartoes: impl Fn() -> Vec<Cartao> + 'static,
    acoes: AcoesInicio,
    ferramentas: Vec<Card>,
) -> impl View {
    let cartoes: Rc<dyn Fn() -> Vec<Cartao>> = Rc::new(cartoes);

    let ha_fixadas = {
        let cartoes = Rc::clone(&cartoes);
        move || cartoes().iter().any(|c| c.fixado)
    };
    let ha_recentes = {
        let cartoes = Rc::clone(&cartoes);
        move || cartoes().iter().any(|c| !c.fixado)
    };

    let fixadas = {
        let (cartoes, acoes) = (Rc::clone(&cartoes), acoes.clone());
        Show::when(ha_fixadas, move || {
            Column::new()
                .gap(8)
                .child(secao(t!("inicio-fixadas")))
                .child(grade(Rc::clone(&cartoes), acoes.clone(), true))
        })
    };

    let recentes = {
        let (cartoes, acoes_grade) = (Rc::clone(&cartoes), acoes.clone());
        let nova = Rc::clone(&acoes.nova);
        Show::when(ha_recentes, move || {
            grade(Rc::clone(&cartoes), acoes_grade.clone(), false)
        })
        .otherwise(move || {
            let nova = Rc::clone(&nova);
            Column::new()
                .gap(4)
                .align(Align::Start)
                .child(
                    Text::new(t!("inicio-nenhuma-conexao-aberta-ainda"))
                        .id("inicio-sem-recentes")
                        .secondary(),
                )
                .child(
                    Button::new(t!("comum-nova-conexao"))
                        .id("inicio-recentes-novo")
                        .hyperlink()
                        .on_click(move || nova()),
                )
        })
    };

    let grade_ferramentas = ferramentas
        .into_iter()
        .fold(UniformGrid::new().min_column_width(320).gap(8), |g, c| {
            g.child(c)
        });

    let conteudo = Column::new()
        .gap(24)
        .child(destaque(&acoes))
        .child(fixadas)
        .child(
            Column::new()
                .gap(8)
                .child(secao(t!("inicio-conexoes-recentes")))
                .child(recentes),
        )
        .child(
            Column::new()
                .gap(8)
                .child(secao(t!("inicio-ferramentas")))
                .child(grade_ferramentas),
        );

    pagina_centrada("inicio-pagina", "inicio-rolagem", conteudo)
}

/// A página no formato da Gallery: `conteudo` (com a largura de leitura) centrado entre dois
/// espaçadores, numa rolagem.
pub(crate) fn pagina_centrada(id: &str, rolagem: &str, conteudo: Column) -> Column {
    Column::new().id(id).grow(1).child(
        ScrollViewer::new().id(rolagem).grow(1).child(
            Row::new()
                .padding(Insets::new(24.0, 40.0, 24.0, 40.0))
                .child(Column::new().grow(1))
                .child(conteudo.grow(1000).max_width(LARGURA_MAXIMA))
                .child(Column::new().grow(1)),
        ),
    )
}
