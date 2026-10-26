//! A página Configurações (M35), no padrão das Configurações do Windows 11: seções com um card
//! por opção, busca no alto e "Restaurar" por seção. Cada mudança vale na hora (a tela grava o
//! `configuracoes.json` e aplica o efeito).
//!
//! Os títulos, as descrições e as palavras da busca vêm de [`catalogo`]; aqui ficam só os
//! controles. Cada controle liga um campo de [`Configuracoes`] por `ligar`: um signal local
//! que escreve na configuração quando muda e relê dela quando ela muda por outro caminho
//! (restaurar, importar, desfazer).

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tomni_ui::dialogs;
use tomni_ui::files::rclone::find_rclone;
use tomni_ui::prelude::*;

use crate::armazenamento::{self, Armazenamento, MudancaDePasta, OrigemDaPasta};
use crate::atalhos::{self, Grupo};
use crate::busca::normalizar;
use crate::cofre::{Cofre, CofreCompartilhado};
use crate::configuracoes::catalogo::{self, Tipo, id_do_valor};
use crate::configuracoes::{
    AbrirEm, AbrirEspaco, AbrirIa, AoFechar, AssistentePreferido, BotaoDireito, Campainha,
    ChaveNova, ColarArquivos, Configuracoes, ConflitoCfg, ConversaAoControlar, Cursor,
    EnviarEditado, Esquema, Idioma, MetodoIa, ModoComparar, ModoNotas, ModoReconectar,
    PaginaInicial, RECENTES_MAX, RECENTES_MIN, Tema, TipoChave, TmuxAoFechar, ValidadeLink,
    X11Padrao,
};
use crate::i18n;
use crate::modelo::{FtpCodificacao, FtpModo, FtpSeguranca, FtpTls, S3Estilo};
use crate::recentes::Recentes;
use crate::reinicio;
use crate::{t, tk};

/// A largura máxima do conteúdo, como nas Configurações do Windows.
const LARGURA_MAXIMA: f64 = 1000.0;

/// Uma pasta a partir do valor de um campo de configuração.
pub type PastaPorValor = Rc<dyn Fn(&str) -> Option<PathBuf>>;

/// O que a página precisa da tela.
#[derive(Clone)]
pub struct Contexto {
    /// As configurações, lidas e escritas pela página.
    pub config: Signal<Configuracoes>,
    /// Os recentes (o "Limpar recentes").
    pub recentes: Signal<Recentes>,
    /// A página da navegação: os cards que levam a outras páginas.
    pub pagina: Signal<String>,
    /// O texto da busca da página; a busca global da casca também escreve nele.
    pub busca: Signal<String>,
    /// Os avisos da leitura do `configuracoes.json` (o que voltou ao padrão).
    pub avisos_da_leitura: Signal<Vec<String>>,
    /// Onde estão os dados.
    pub armazenamento: Armazenamento,
    /// A pasta padrão do sistema, onde fica o `pasta.txt`.
    pub pasta_padrao: PathBuf,
    /// De onde veio a pasta de dados atual.
    pub origem_da_pasta: OrigemDaPasta,
    /// O cofre (a lista de itens do card da senha do rclone).
    pub cofre: CofreCompartilhado,
    /// Abre o diálogo "Importar do ~/.ssh/config".
    pub importar_ssh_config: Rc<dyn Fn()>,
    /// Abre o diálogo "Importar do ~/.aws".
    pub importar_aws: Rc<dyn Fn()>,
    /// "Levar o cofre para outra máquina…".
    pub levar_cofre: Rc<dyn Fn()>,
    /// "Abrir cofre de outra máquina…".
    pub abrir_cofre_de_fora: Rc<dyn Fn()>,
    /// Quantos logs de texto a pasta dos logs tem (a InfoBar "Cifrar os N logs de texto que já
    /// existem").
    pub logs_a_cifrar: Signal<usize>,
    /// "Cifrar todos os de texto…": abre o diálogo de confirmar da página Logs.
    pub cifrar_logs_existentes: Rc<dyn Fn()>,
    /// Pergunta como mudar o local do cofre para a pasta dada (`None` é a pasta de dados).
    pub mudar_local_do_cofre: Rc<dyn Fn(Option<PathBuf>)>,
    /// A árvore de conexões (o nome da conexão de cada espaço de trabalho).
    pub arvore: Signal<crate::modelo::Tree>,
    /// A linha de estado do controle por assistentes (lê signals).
    pub mcp_estado: Rc<dyn Fn() -> String>,
    /// Mostra o aviso de ligar o controle pela primeira vez.
    pub mcp_aviso: Rc<dyn Fn()>,
    /// Abre o "Como ligar um assistente", com o espaço dado escolhido quando há.
    pub mcp_como: Rc<dyn Fn(Option<String>)>,
    /// Abre o diálogo de um espaço de trabalho novo.
    pub espaco_novo: Rc<dyn Fn()>,
    /// Abre o diálogo para editar o espaço de trabalho de nome dado.
    pub espaco_editar: Rc<dyn Fn(String)>,
    /// Abre o diálogo "Instalar no computador" (M65).
    pub abrir_instalacao: Rc<dyn Fn()>,
    /// A linha de resumo da instalação ("Instalado em …" ou "Não instalado"; lê signals).
    pub resumo_da_instalacao: Rc<dyn Fn() -> String>,
    /// A pasta de trabalho dos assistentes para um valor de `ia.local_pasta` (vazio é a padrão).
    pub pasta_dos_assistentes: PastaPorValor,
    /// A pasta pessoal, que o "Limpar a pasta de trabalho" não esvazia.
    pub pasta_pessoal: Option<PathBuf>,
}

/// O estado da página: o contexto mais o que só ela usa.
#[derive(Clone)]
struct Pagina {
    ctx: Rc<Contexto>,
    config: Signal<Configuracoes>,
    busca: Signal<String>,
    /// As chaves do catálogo que a busca deixa ver.
    achados: Memo<Vec<&'static str>>,
    /// Os ids de atalhos que a busca deixa ver.
    atalhos_achados: Memo<Vec<&'static str>>,
    /// O que a última ação fez, ou vazio.
    mensagem: Signal<String>,
    /// O que deu errado, ou vazio.
    erro: Signal<String>,
    /// A configuração de antes e a de depois da última troca em bloco (restaurar, importar):
    /// o "Desfazer" vale enquanto nada mais mudou.
    desfazer: Signal<Option<(Configuracoes, Configuracoes)>>,
    /// A pasta de dados que vale depois de reiniciar, se a pessoa a mudou.
    reiniciar: Signal<Option<String>>,
    /// O aberto de cada seção da sanfona, para o "Expandir tudo".
    secoes: Rc<RefCell<Vec<Signal<bool>>>>,
    /// O diálogo "Limpar a pasta de trabalho?" aberto.
    limpar_aberto: Signal<bool>,
    /// A pasta que o diálogo vai esvaziar e quantos itens ela tem.
    limpar_alvo: Signal<Option<(PathBuf, usize)>>,
}

/// Liga um campo da configuração a um signal. `ler` tira o valor da configuração e `gravar`
/// o devolve (prendendo na faixa, se for o caso).
pub(crate) fn ligar<T>(
    config: Signal<Configuracoes>,
    ler: fn(&Configuracoes) -> T,
    gravar: fn(&mut Configuracoes, T),
) -> Signal<T>
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    let valor = signal(config.with_untracked(ler));
    effect(move || {
        let novo = valor.get();
        untrack(|| {
            if config.with_untracked(|c| ler(c) != novo) {
                config.update(|c| gravar(c, novo));
            }
        });
    });
    effect(move || {
        let atual = config.with(ler);
        untrack(|| {
            if valor.get_untracked() != atual {
                valor.set(atual);
            }
        });
    });
    valor
}

/// Um campo copiado (`bool`, `String`).
macro_rules! campo {
    ($pagina:expr, $($p:ident).+) => {
        ligar($pagina.config, |c| c.$($p).+.clone(), |c, v| c.$($p).+ = v)
    };
}

/// Um campo numérico, no `NumberBox` (que trabalha com `f64`), preso entre `$min` e `$max`.
macro_rules! numero_de {
    ($pagina:expr, $min:expr, $max:expr, $($p:ident).+) => {
        ligar(
            $pagina.config,
            |c| c.$($p).+ as f64,
            |c, v| {
                if v.is_finite() {
                    c.$($p).+ = v.round().clamp($min as f64, $max as f64) as _;
                }
            },
        )
    };
}

/// Um campo de escolha, na `ComboBox` (que trabalha com o nome na tela).
macro_rules! opcao_de {
    ($pagina:expr, $todas:expr, $($p:ident).+) => {
        ligar(
            $pagina.config,
            |c| c.$($p).+.nome().to_owned(),
            |c, v| {
                if let Some(o) = $todas.iter().copied().find(|o| o.nome() == v) {
                    c.$($p).+ = o;
                }
            },
        )
    };
}

/// Os nomes na tela de uma lista de opções.
fn nomes<T: Copy, S: Into<String>>(todas: &[T], nome: fn(T) -> S) -> Vec<String> {
    todas.iter().map(|o| nome(*o).into()).collect()
}

/// O que o botão "Escolher…" de um caminho abre.
#[derive(Clone, Copy)]
enum Escolher {
    Pasta,
    Arquivo,
}

impl Pagina {
    /// Se a busca deixa ver `chave`.
    fn achou(&self, chave: &'static str) -> impl Fn() -> bool + 'static {
        let achados = self.achados;
        move || achados.with(|a| a.contains(&chave))
    }

    /// Se a busca deixa ver alguma das `chaves`.
    fn achou_alguma(&self, chaves: &'static [&'static str]) -> impl Fn() -> bool + Copy + 'static {
        let achados = self.achados;
        move || achados.with(|a| chaves.iter().any(|c| a.contains(c)))
    }

    /// O card de `chave`, com o título, a descrição e o id do catálogo.
    fn card(&self, chave: &'static str) -> SettingsCard {
        let d =
            catalogo::definicao(chave).unwrap_or_else(|| panic!("{chave} não está no catálogo"));
        SettingsCard::new(d.titulo())
            .description(d.descricao())
            .id(d.id())
            .visible(self.achou(chave))
    }

    fn interruptor(&self, chave: &'static str, valor: Signal<bool>) -> SettingsCard {
        self.card(chave)
            .content(ToggleSwitch::bind(valor).id(id_do_valor(chave)))
    }

    fn numero(&self, chave: &'static str, valor: Signal<f64>, min: f64, max: f64) -> SettingsCard {
        self.card(chave).content(
            NumberBox::bind(valor)
                .id(id_do_valor(chave))
                .min(min)
                .max(max)
                .width(160),
        )
    }

    fn escolha(
        &self,
        chave: &'static str,
        valor: Signal<String>,
        opcoes: Vec<String>,
    ) -> SettingsCard {
        self.card(chave).content(
            ComboBox::bind(valor, opcoes)
                .id(id_do_valor(chave))
                .width(260),
        )
    }

    fn texto(
        &self,
        chave: &'static str,
        valor: Signal<String>,
        dica: &'static str,
    ) -> SettingsCard {
        self.card(chave).content(
            Edit::bind(valor)
                .id(id_do_valor(chave))
                .placeholder(t!(dica))
                .width(260),
        )
    }

    /// Um caminho: o campo e o botão que abre o seletor do sistema.
    fn caminho(
        &self,
        chave: &'static str,
        valor: Signal<String>,
        dica: &'static str,
        o_que: Escolher,
    ) -> SettingsCard {
        self.caminho_com_dica(chave, valor, t!(dica), o_que)
    }

    /// Como [`Pagina::caminho`], com a dica já pronta (um caminho do sistema, por exemplo).
    fn caminho_com_dica(
        &self,
        chave: &'static str,
        valor: Signal<String>,
        dica: String,
        o_que: Escolher,
    ) -> SettingsCard {
        let titulo = catalogo::definicao(chave).map_or_else(String::new, |d| d.titulo());
        let escolher = move || {
            let dialogo = match o_que {
                Escolher::Pasta => dialogs::pick_folder(),
                Escolher::Arquivo => dialogs::open_file(),
            };
            dialogo.title(&titulo).remember(chave).show(move |caminho| {
                if let Some(caminho) = caminho {
                    valor.set(caminho.display().to_string());
                }
            });
        };
        self.card(chave).content(
            Row::new()
                .gap(8)
                .child(
                    Edit::bind(valor)
                        .id(id_do_valor(chave))
                        .placeholder(dica)
                        .width(260),
                )
                .child(
                    Button::new(t!("config-escolher"))
                        .id(format!("{}-escolher", id_do_valor(chave)))
                        .on_click(escolher),
                ),
        )
    }

    /// Um card que faz algo ao clicar.
    fn acao(&self, chave: &'static str, icone: Symbol, f: impl FnMut() + 'static) -> SettingsCard {
        self.card(chave).icon(icone).on_click(f)
    }

    /// Um card que leva a outra página.
    fn levar(&self, chave: &'static str, icone: Symbol, destino: &'static str) -> SettingsCard {
        let pagina = self.ctx.pagina;
        self.acao(chave, icone, move || pagina.set(destino.to_owned()))
    }

    /// Um grupo de cards (o `SettingsExpander` sem valor próprio). Abre sozinho quando a busca
    /// acha algum filho.
    fn grupo(&self, chave: &'static str, filhos: &'static [&'static str]) -> SettingsExpander {
        let d =
            catalogo::definicao(chave).unwrap_or_else(|| panic!("{chave} não está no catálogo"));
        let aberto = signal(false);
        let (achados, busca) = (self.achados, self.busca);
        effect(move || {
            let achou = achados.with(|a| filhos.iter().any(|f| a.contains(f)));
            let buscando = !busca.with(|b| b.trim().is_empty());
            if achou && buscando {
                aberto.set(true);
            }
        });
        SettingsExpander::new(d.titulo(), aberto)
            .description(d.descricao())
            .id(d.id())
            .visible(self.achou_alguma(filhos))
    }

    fn informar(&self, texto: impl Into<String>) {
        self.erro.set(String::new());
        self.mensagem.set(texto.into());
    }

    fn falhar(&self, texto: impl Into<String>) {
        self.mensagem.set(String::new());
        self.erro.set(texto.into());
    }

    /// Troca a configuração toda e guarda a de antes para o "Desfazer".
    fn trocar(&self, nova: Configuracoes, texto: impl Into<String>) {
        let antes = self.config.get_untracked();
        if nova == antes {
            self.informar(t!("config-nada-mudou"));
            return;
        }
        self.config.set(nova.clone());
        self.desfazer.set(Some((antes, nova)));
        self.informar(texto);
    }

    /// Volta uma categoria ao padrão.
    fn restaurar(&self, categoria: &str, nome: &str) {
        let mut nova = self.config.get_untracked();
        nova.restaurar_categoria(categoria);
        self.trocar(nova, t!("config-secao-restaurada", secao = nome));
    }

    fn restaurar_tudo(&self) {
        let mut nova = self.config.get_untracked();
        nova.restaurar_tudo();
        self.trocar(nova, t!("config-tudo-restaurado"));
    }

    fn exportar(&self) {
        let pagina = self.clone();
        dialogs::save_file()
            .remember("configuracoes-arquivo")
            .title(t!("config-exportar-titulo"))
            .filter("JSON", &["json"])
            .file_name("tirech-omni-configuracoes.json")
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let texto = pagina.config.with_untracked(Configuracoes::exportar);
                match std::fs::write(&caminho, texto) {
                    Ok(()) => pagina.informar(t!(
                        "config-exportado",
                        caminho = caminho.display().to_string()
                    )),
                    Err(erro) => pagina.falhar(t!(
                        "config-erro-gravar",
                        caminho = caminho.display().to_string(),
                        erro = erro.to_string()
                    )),
                }
            });
    }

    fn importar(&self) {
        let pagina = self.clone();
        dialogs::open_file()
            .remember("configuracoes-arquivo")
            .title(t!("config-importar-titulo"))
            .filter("JSON", &["json"])
            .show(move |caminho| {
                let Some(caminho) = caminho else {
                    return;
                };
                let texto = match std::fs::read_to_string(&caminho) {
                    Ok(texto) => texto,
                    Err(erro) => {
                        pagina.falhar(t!(
                            "config-erro-ler",
                            caminho = caminho.display().to_string(),
                            erro = erro.to_string()
                        ));
                        return;
                    }
                };
                let atual = pagina.config.get_untracked();
                match atual.importar(&texto) {
                    Ok((nova, avisos)) => {
                        let mut aviso =
                            t!("config-importado", caminho = caminho.display().to_string());
                        if !avisos.is_empty() {
                            aviso.push(' ');
                            aviso.push_str(&avisos.join(" "));
                        }
                        pagina.trocar(nova, aviso);
                    }
                    Err(erro) => pagina.falhar(erro),
                }
            });
    }
}

/// A página Configurações.
pub fn pagina_configuracoes(ctx: Contexto) -> impl View {
    let config = ctx.config;
    let busca = ctx.busca;
    let portatil = ctx.origem_da_pasta == OrigemDaPasta::Portatil;
    let achados = memo(move || {
        catalogo::buscar_em(&busca.get(), portatil)
            .iter()
            .map(|d| d.chave)
            .collect::<Vec<_>>()
    });
    let atalhos_achados = memo(move || {
        let termos = catalogo::termos(&busca.get());
        atalhos::CATALOGO
            .iter()
            .filter(|a| {
                let palavras = [Idioma::PtBr, Idioma::EnUs, Idioma::EsEs]
                    .map(|i| i18n::texto_em(i, tk!("config-atalhos-palavras"), None))
                    .join(" ");
                let nomes = [Idioma::PtBr, Idioma::EnUs, Idioma::EsEs]
                    .map(|i| a.nome_em(i))
                    .join(" ");
                let texto = normalizar(&format!("{palavras} {nomes} {}", a.id));
                catalogo::casa(&termos, &texto)
            })
            .map(|a| a.id)
            .collect::<Vec<_>>()
    });
    let p = Pagina {
        ctx: Rc::new(ctx),
        config,
        busca,
        achados,
        atalhos_achados,
        mensagem: signal(String::new()),
        erro: signal(String::new()),
        desfazer: signal(None),
        reiniciar: signal(None),
        secoes: Rc::default(),
        limpar_aberto: signal(false),
        limpar_alvo: signal(None),
    };

    let conteudo = Column::new()
        .grow(1000)
        .max_width(LARGURA_MAXIMA)
        .gap(8)
        .child(avisos(&p))
        .child(vazio(&p))
        .child(secao_geral(&p))
        .child(secao_terminal(&p))
        .child(secao_conexoes(&p))
        .child(secao_scripts(&p))
        .child(secao_cofre(&p))
        .child(secao_notas(&p))
        .child(secao_agente(&p))
        .child(secao_arquivos(&p))
        .child(secao_ftp(&p))
        .child(secao_s3(&p))
        .child(secao_rclone(&p))
        .child(secao_redes(&p))
        .child(secao_x11(&p))
        .child(secao_portas(&p))
        .child(secao_janelas(&p))
        .child(secao_ia(&p))
        .child(secao_ia_servidor(&p))
        .child(secao_ia_local(&p))
        .child(secao_mcp(&p))
        .child(secao_atalhos(&p))
        .child(rodape(&p))
        .child(dialogo_limpar_pasta(&p));

    // O título e a busca ficam fixos no alto; só as seções rolam.
    let topo = Column::new()
        .grow(1000)
        .max_width(LARGURA_MAXIMA)
        .gap(16)
        .child(
            Row::new()
                .gap(16)
                .align(Align::Center)
                .child(
                    Text::new(t!("config-titulo"))
                        .title()
                        .id("config-titulo")
                        .grow(1),
                )
                .child(expandir_tudo(&p)),
        )
        .child(Row::new().child(campo_de_busca(&p)));
    Column::new()
        .id("configuracoes-pagina")
        .grow(1)
        .child(
            Row::new()
                .padding(Insets::new(28.0, 40.0, 16.0, 40.0))
                .child(Column::new().grow(1))
                .child(topo)
                .child(Column::new().grow(1)),
        )
        .child(
            ScrollViewer::new().id("config-rolagem").grow(1).child(
                Row::new()
                    .padding(Insets::new(0.0, 40.0, 24.0, 40.0))
                    .child(Column::new().grow(1))
                    .child(conteudo)
                    .child(Column::new().grow(1)),
            ),
        )
}

/// "Expandir tudo", ou "Recolher tudo" quando todas as seções estão abertas.
fn expandir_tudo(p: &Pagina) -> Button {
    let secoes = p.secoes.clone();
    let todas_abertas = move || secoes.borrow().iter().all(|s| s.get());
    let secoes = p.secoes.clone();
    Button::new(move || {
        if todas_abertas() {
            t!("config-recolher-tudo")
        } else {
            t!("config-expandir-tudo")
        }
    })
    .id("config-expandir-tudo")
    .hyperlink()
    .on_click(move || {
        let secoes = secoes.borrow();
        let abrir = !secoes.iter().all(|s| s.get_untracked());
        for aberto in secoes.iter() {
            aberto.set(abrir);
        }
    })
}

/// O campo de busca da página: filtra os cards e sugere os títulos.
fn campo_de_busca(p: &Pagina) -> AutoSuggestBox {
    AutoSuggestBox::bind(p.busca, |procurado: &str| {
        if procurado.trim().is_empty() {
            return Vec::new();
        }
        let mut titulos: Vec<String> = Vec::new();
        for d in catalogo::buscar(procurado) {
            if titulos.len() >= 8 {
                break;
            }
            let titulo = d.titulo();
            if !titulos.contains(&titulo) {
                titulos.push(titulo);
            }
        }
        titulos
    })
    .id("config-busca")
    .placeholder(t!("config-buscar"))
    .query_icon(Symbol::Search)
    .width(360.0)
}

/// As barras de aviso do alto: o que a leitura do arquivo corrigiu, o que a última ação fez
/// (com o "Desfazer") e o que deu errado.
fn avisos(p: &Pagina) -> impl View {
    let (mensagem, erro, desfazer, config) = (p.mensagem, p.erro, p.desfazer, p.config);
    let leitura = p.ctx.avisos_da_leitura;
    let pode_desfazer = move || {
        desfazer.with(|d| {
            d.as_ref()
                .is_some_and(|(_, depois)| config.with(|c| c == depois))
        })
    };
    Column::new()
        .gap(8)
        .child(
            InfoBar::new(InfoBarSeverity::Warning)
                .id("config-avisos-leitura")
                .title(t!("config-avisos-leitura"))
                .message(move || leitura.with(|a| a.join(" ")))
                .visible(move || leitura.with(|a| !a.is_empty()))
                .on_close(move || leitura.set(Vec::new())),
        )
        .child(
            Row::new()
                .gap(8)
                .visible(move || !mensagem.with(String::is_empty))
                .child(
                    InfoBar::new(InfoBarSeverity::Success)
                        .id("config-mensagem")
                        .message(move || mensagem.get())
                        .grow(1)
                        .on_close(move || mensagem.set(String::new())),
                )
                .child(
                    Button::new(t!("config-desfazer"))
                        .id("config-desfazer")
                        .visible(pode_desfazer)
                        .on_click(move || {
                            if let Some((antes, _)) = desfazer.get_untracked() {
                                config.set(antes);
                            }
                            desfazer.set(None);
                            mensagem.set(t!("config-desfeito"));
                        }),
                ),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id("config-erro")
                .message(move || erro.get())
                .visible(move || !erro.with(String::is_empty))
                .on_close(move || erro.set(String::new())),
        )
}

/// "Nenhuma configuração encontrada para …", quando a busca não acha nada.
fn vazio(p: &Pagina) -> Text {
    let (achados, atalhos_achados, busca) = (p.achados, p.atalhos_achados, p.busca);
    let sem_nada = move || achados.with(Vec::is_empty) && atalhos_achados.with(Vec::is_empty);
    Text::new(move || {
        let procurado = busca.get();
        t!("config-busca-vazia", busca = procurado.trim())
    })
    .id("config-busca-vazia")
    .secondary()
    .visible(sem_nada)
}

/// Uma seção da sanfona: começa recolhida, com o título e o resumo (o atributo `.resumo` da
/// chave `titulo`). Enquanto há busca, fica aberta se acha algo nela e some se não acha nada;
/// ao limpar a busca, volta a como estava antes dela. O id é `config-secao-{categoria}`.
fn sanfona(
    p: &Pagina,
    titulo: &'static str,
    categoria: &'static str,
    achou: impl Fn() -> bool + Copy + 'static,
    corpo: Column,
) -> Expander {
    let aberto = signal(false);
    p.secoes.borrow_mut().push(aberto);
    let busca = p.busca;
    // Como estava antes de a busca começar.
    let antes = Cell::new(None);
    effect(move || {
        let achou = achou();
        if busca.with(|b| b.trim().is_empty()) {
            if let Some(estava) = antes.take() {
                aberto.set(estava);
            }
        } else {
            if antes.get().is_none() {
                antes.set(Some(aberto.get_untracked()));
            }
            aberto.set(achou);
        }
    });
    Expander::new(t!(titulo), aberto)
        .description(t!(&format!("{titulo}.resumo")))
        .id(format!("config-secao-{categoria}"))
        .visible(achou)
        .content(corpo)
}

/// Uma seção: os cards e o "Restaurar os padrões desta seção", numa sanfona.
/// `titulo` é a chave do título (`config-secao-…`), a mesma que o catálogo guarda em `secao`.
fn secao(p: &Pagina, titulo: &'static str, categoria: &'static str, cards: Column) -> Expander {
    let chaves: &'static [&'static str] = Box::leak(
        catalogo::CATALOGO
            .iter()
            .filter(|d| d.secao == titulo && d.tipo != Tipo::Grupo)
            .map(|d| d.chave)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    let pagina = p.clone();
    let corpo = Column::new().gap(4).child(cards).child(
        Row::new().child(
            Button::new(t!("config-restaurar-secao"))
                .id(format!("config-restaurar-{categoria}"))
                .hyperlink()
                .on_click(move || pagina.restaurar(categoria, &t!(titulo))),
        ),
    );
    sanfona(p, titulo, categoria, p.achou_alguma(chaves), corpo)
}

fn lista() -> Column {
    Column::new().gap(4)
}

fn secao_geral(p: &Pagina) -> Expander {
    let pagina_inicial = ligar(
        p.config,
        |c| c.geral.pagina_inicial.nome().to_owned(),
        |c, v| {
            if let Some(o) = PaginaInicial::do_nome(&v) {
                c.geral.pagina_inicial = o;
            }
        },
    );
    let limpar = p.ctx.recentes;
    let sobre = p.ctx.pagina;
    let sem_bandeja = || !tomni_ui::tray::available();
    secao(
        p,
        tk!("config-secao-geral"),
        "geral",
        lista()
            .child(p.escolha(
                "geral.tema",
                opcao_de!(p, Tema::TODAS, geral.tema),
                Tema::nomes(),
            ))
            .child(p.escolha(
                "geral.idioma",
                opcao_de!(p, Idioma::TODAS, geral.idioma),
                Idioma::nomes(),
            ))
            .child(aviso_do_idioma(p))
            .child(p.escolha(
                "geral.pagina_inicial",
                pagina_inicial,
                nomes(&PaginaInicial::TODAS, PaginaInicial::nome),
            ))
            .child(p.interruptor(
                "geral.restaurar_sessoes",
                campo!(p, geral.restaurar_sessoes),
            ))
            .child(p.interruptor("geral.confirmar_fechar", campo!(p, geral.confirmar_fechar)))
            .child(
                p.card("geral.ao_fechar")
                    .description(descricao_da_bandeja("geral.ao_fechar"))
                    .disabled(sem_bandeja)
                    .content(
                        ComboBox::bind(
                            opcao_de!(p, AoFechar::TODAS, geral.ao_fechar),
                            AoFechar::nomes(),
                        )
                        .id(id_do_valor("geral.ao_fechar"))
                        .disabled(sem_bandeja)
                        .width(260),
                    ),
            )
            .child(p.interruptor(
                "geral.iniciar_com_sistema",
                campo!(p, geral.iniciar_com_sistema),
            ))
            .child(
                p.card("geral.iniciar_na_bandeja")
                    .description(descricao_da_bandeja("geral.iniciar_na_bandeja"))
                    .disabled(sem_bandeja)
                    .content(
                        ToggleSwitch::bind(campo!(p, geral.iniciar_na_bandeja))
                            .id(id_do_valor("geral.iniciar_na_bandeja"))
                            .disabled(sem_bandeja),
                    ),
            )
            .child(p.interruptor("geral.atalho_no_menu", campo!(p, geral.atalho_no_menu)))
            .child(p.numero(
                "geral.recentes_max",
                numero_de!(p, RECENTES_MIN, RECENTES_MAX, geral.recentes_max),
                f64::from(RECENTES_MIN),
                f64::from(RECENTES_MAX),
            ))
            .child(p.acao("acao.limpar_recentes", Symbol::Delete, move || {
                limpar.update(Recentes::limpar)
            }))
            .child(cartao_instalacao(p))
            .child(cartao_pasta(p))
            .child(p.acao("acao.sobre", Symbol::Info, move || {
                sobre.set("sobre".to_owned())
            })),
    )
}

/// O aviso de que o idioma novo só vale ao reiniciar, com o botão que reinicia. Aparece quando o
/// idioma escolhido não é o que o app abriu.
fn aviso_do_idioma(p: &Pagina) -> impl View {
    let config = p.config;
    let a_reiniciar = move || config.with(|c| c.geral.idioma != i18n::configurado());
    Row::new()
        .gap(8)
        .id("config-geral-idioma-aviso-linha")
        .visible(a_reiniciar)
        .child(
            InfoBar::new(InfoBarSeverity::Informational)
                .id("config-geral-idioma-aviso")
                .message(t!("config-geral-idioma-aviso"))
                .grow(1),
        )
        .child(
            Button::new(t!("config-geral-idioma-reiniciar"))
                .id("config-geral-idioma-reiniciar")
                .on_click(reinicio::pedir),
        )
}

/// A descrição de um card que depende da bandeja: sem ela, explica por que está desligado.
fn descricao_da_bandeja(chave: &'static str) -> impl Fn() -> String + 'static {
    let d = catalogo::definicao(chave).expect("catálogo");
    move || {
        if tomni_ui::tray::available() {
            d.descricao()
        } else {
            format!("{} {}", d.descricao(), t!("config-bandeja-indisponivel"))
        }
    }
}

/// A instalação no computador: o resumo de como está e o botão que abre o diálogo.
fn cartao_instalacao(p: &Pagina) -> SettingsCard {
    let d = catalogo::definicao("acao.instalacao").expect("catálogo");
    let abrir = Rc::clone(&p.ctx.abrir_instalacao);
    let resumo = Rc::clone(&p.ctx.resumo_da_instalacao);
    p.acao("acao.instalacao", Symbol::Apps, move || abrir())
        .description(move || {
            let estado = resumo();
            if estado.is_empty() {
                d.descricao()
            } else {
                format!("{} {estado}", d.descricao())
            }
        })
}

/// A pasta de dados: onde está, abrir, mudar e voltar ao padrão (vale ao reiniciar).
fn cartao_pasta(p: &Pagina) -> SettingsExpander {
    let d = catalogo::definicao("acao.pasta_dados").expect("catálogo");
    let aberto = signal(false);
    let atual = p.ctx.armazenamento.pasta().to_path_buf();
    let origem = p.ctx.origem_da_pasta;
    let fixa = origem == OrigemDaPasta::Variavel;
    let portatil = origem == OrigemDaPasta::Portatil;
    let (busca, achados) = (p.busca, p.achados);
    effect(move || {
        let buscando = !busca.with(|b| b.trim().is_empty());
        if buscando && achados.with(|a| a.contains(&"acao.pasta_dados")) {
            aberto.set(true);
        }
    });
    let pasta_atual = atual.display().to_string();
    let descricao = match origem {
        OrigemDaPasta::Variavel => t!(
            "config-pasta-origem-variavel",
            pasta = pasta_atual,
            variavel = armazenamento::ENV
        ),
        OrigemDaPasta::Portatil => t!("config-pasta-origem-portatil", pasta = pasta_atual),
        OrigemDaPasta::PastaTxt => t!("config-pasta-origem-escolhida", pasta = pasta_atual),
        OrigemDaPasta::Padrao => t!("config-pasta-origem-padrao", pasta = pasta_atual),
    };
    let reiniciar = p.reiniciar;
    let sem_senha_mestra = {
        let cofre = p.ctx.cofre.clone();
        move || portatil && !cofre.ler(Cofre::tem_senha_mestra)
    };

    let abrir = {
        let (pagina, atual) = (p.clone(), atual.clone());
        move || {
            if let Err(erro) = crate::ferramentas::abrir_pasta(&atual) {
                pagina.falhar(t!(
                    "config-erro-abrir",
                    caminho = atual.display().to_string(),
                    erro = erro.to_string()
                ));
            }
        }
    };
    let mudar = {
        let (pagina, atual, padrao) = (p.clone(), atual.clone(), p.ctx.pasta_padrao.clone());
        move || {
            let (pagina, atual, padrao) = (pagina.clone(), atual.clone(), padrao.clone());
            dialogs::pick_folder()
                .remember("configuracoes-pasta")
                .title(t!("config-pasta-titulo"))
                .show(move |destino| {
                    let Some(destino) = destino else {
                        return;
                    };
                    match armazenamento::mudar_pasta(&atual, &destino, &padrao) {
                        Ok(mudanca) => {
                            pagina.reiniciar.set(Some(destino.display().to_string()));
                            let pasta = destino.display().to_string();
                            pagina.informar(match mudanca {
                                MudancaDePasta::Copiou => t!("config-pasta-copiou", pasta = pasta),
                                MudancaDePasta::Apontou => {
                                    t!("config-pasta-apontou", pasta = pasta)
                                }
                            });
                        }
                        Err(erro) => {
                            pagina.falhar(t!("config-pasta-erro-mudar", erro = erro.to_string()))
                        }
                    }
                });
        }
    };
    let padrao = {
        let (pagina, padrao) = (p.clone(), p.ctx.pasta_padrao.clone());
        move || match armazenamento::voltar_ao_padrao(&padrao) {
            Ok(()) => {
                pagina.reiniciar.set(Some(padrao.display().to_string()));
                pagina.informar(t!(
                    "config-pasta-voltou",
                    pasta = padrao.display().to_string()
                ));
            }
            Err(erro) => pagina.falhar(t!("config-pasta-erro-padrao", erro = erro.to_string())),
        }
    };

    SettingsExpander::new(d.titulo(), aberto)
        .description(descricao)
        .icon(Symbol::Folder)
        .id(d.id())
        .visible(p.achou("acao.pasta_dados"))
        .item(
            SettingsCard::new(t!("config-pasta-abrir"))
                .id("config-pasta-abrir")
                .icon(Symbol::FolderOpen)
                .on_click(abrir),
        )
        .item(
            SettingsCard::new(t!("config-pasta-portatil-senha"))
                .id("config-pasta-portatil-senha")
                .description(t!("config-pasta-portatil-senha.descricao"))
                .icon(Symbol::Shield)
                .visible(sem_senha_mestra),
        )
        .item(
            SettingsCard::new(t!("config-pasta-mudar"))
                .id("config-pasta-mudar")
                .description(t!("config-pasta-mudar.descricao"))
                .icon(Symbol::Folder)
                .disabled(fixa)
                .visible(!portatil)
                .on_click(mudar),
        )
        .item(
            SettingsCard::new(t!("config-pasta-padrao"))
                .id("config-pasta-padrao")
                .icon(Symbol::ArrowUndo)
                .disabled(fixa || origem == OrigemDaPasta::Padrao)
                .visible(!portatil)
                .on_click(padrao),
        )
        .item(
            SettingsCard::new(t!("config-pasta-reiniciar"))
                .id("config-pasta-reiniciar")
                .description(move || {
                    reiniciar
                        .get()
                        .map(|pasta| t!("config-pasta-reiniciar.descricao", pasta = pasta))
                        .unwrap_or_default()
                })
                .icon(Symbol::ArrowSync)
                .visible(move || reiniciar.with(Option::is_some))
                .on_click(reinicio::pedir),
        )
}

fn secao_terminal(p: &Pagina) -> Expander {
    const SHELL: &[&str] = &[
        "terminal.shell.programa",
        "terminal.shell.argumentos",
        "terminal.shell.pasta",
    ];
    const LOGS: &[&str] = &[
        "terminal.logs.pasta",
        "terminal.logs.todas_as_sessoes",
        "terminal.logs.cifrar",
        "terminal.logs.selar_secs",
        "terminal.logs.visualizar_mib",
    ];
    secao(
        p,
        tk!("config-secao-terminal"),
        "terminal",
        lista()
            .child(p.numero(
                "terminal.tamanho_fonte",
                numero_de!(p, 8, 48, terminal.tamanho_fonte),
                8.0,
                48.0,
            ))
            .child(p.escolha(
                "terminal.esquema",
                opcao_de!(p, Esquema::TODAS, terminal.esquema),
                Esquema::nomes(),
            ))
            .child(p.escolha(
                "terminal.cursor",
                opcao_de!(p, Cursor::TODAS, terminal.cursor),
                Cursor::nomes(),
            ))
            .child(p.interruptor(
                "terminal.cursor_piscando",
                campo!(p, terminal.cursor_piscando),
            ))
            .child(p.numero(
                "terminal.historico",
                numero_de!(p, 0, 100_000, terminal.historico),
                0.0,
                100_000.0,
            ))
            .child(p.interruptor(
                "terminal.copiar_ao_selecionar",
                campo!(p, terminal.copiar_ao_selecionar),
            ))
            .child(p.interruptor("terminal.avisar_colar", campo!(p, terminal.avisar_colar)))
            .child(p.escolha(
                "terminal.colar_arquivos",
                opcao_de!(p, ColarArquivos::TODAS, terminal.colar_arquivos),
                ColarArquivos::nomes(),
            ))
            .child(p.texto(
                "terminal.pasta_colar",
                campo!(p, terminal.pasta_colar),
                tk!("config-dica-pasta-colar"),
            ))
            .child(p.interruptor(
                "terminal.ctrl_v_arquivos",
                campo!(p, terminal.ctrl_v_arquivos),
            ))
            .child(p.numero(
                "terminal.limpar_colados_dias",
                numero_de!(p, 0, 365, terminal.limpar_colados_dias),
                0.0,
                365.0,
            ))
            .child(p.numero(
                "terminal.colar_tar_min_arquivos",
                numero_de!(p, 0, 100_000, terminal.colar_tar_min_arquivos),
                0.0,
                100_000.0,
            ))
            .child(p.escolha(
                "terminal.botao_direito",
                opcao_de!(p, BotaoDireito::TODAS, terminal.botao_direito),
                BotaoDireito::nomes(),
            ))
            .child(p.escolha(
                "terminal.campainha",
                opcao_de!(p, Campainha::TODAS, terminal.campainha),
                Campainha::nomes(),
            ))
            .child(p.interruptor(
                "terminal.fechar_painel_vazio",
                campo!(p, terminal.fechar_painel_vazio),
            ))
            .child(p.interruptor(
                "terminal.duplicar_na_pasta",
                campo!(p, terminal.duplicar_na_pasta),
            ))
            .child(p.interruptor(
                "terminal.links_com_ctrl",
                campo!(p, terminal.links_com_ctrl),
            ))
            .child(
                p.grupo("terminal.shell", SHELL)
                    .item(p.texto(
                        "terminal.shell.programa",
                        campo!(p, terminal.shell.programa),
                        tk!("config-dica-shell"),
                    ))
                    .item(p.texto(
                        "terminal.shell.argumentos",
                        campo!(p, terminal.shell.argumentos),
                        tk!("config-dica-argumentos-shell"),
                    ))
                    .item(p.caminho(
                        "terminal.shell.pasta",
                        campo!(p, terminal.shell.pasta),
                        tk!("config-dica-pasta-shell"),
                        Escolher::Pasta,
                    )),
            )
            .child(
                p.grupo("terminal.logs", LOGS)
                    .item(p.caminho(
                        "terminal.logs.pasta",
                        campo!(p, terminal.logs.pasta),
                        tk!("config-dica-logs"),
                        Escolher::Pasta,
                    ))
                    .item(p.interruptor(
                        "terminal.logs.todas_as_sessoes",
                        campo!(p, terminal.logs.todas_as_sessoes),
                    ))
                    .item(p.interruptor("terminal.logs.cifrar", campo!(p, terminal.logs.cifrar)))
                    .item(p.numero(
                        "terminal.logs.selar_secs",
                        numero_de!(p, 1, 3600, terminal.logs.selar_secs),
                        1.0,
                        3600.0,
                    ))
                    .item(p.numero(
                        "terminal.logs.visualizar_mib",
                        numero_de!(p, 0, 1024, terminal.logs.visualizar_mib),
                        0.0,
                        1024.0,
                    )),
            )
            .child(aviso_dos_logs_de_texto(p)),
    )
}

/// Com os logs cifrados ligados, oferece cifrar os de texto que a pasta já tinha.
fn aviso_dos_logs_de_texto(p: &Pagina) -> impl View {
    let (config, a_cifrar) = (p.config, p.ctx.logs_a_cifrar);
    let cifrar = Rc::clone(&p.ctx.cifrar_logs_existentes);
    let visivel = move || config.with(|c| c.terminal.logs.cifrar) && a_cifrar.get() > 0;
    Row::new()
        .gap(8)
        .id("config-terminal-logs-existentes-linha")
        .visible(visivel)
        .child(
            InfoBar::new(InfoBarSeverity::Informational)
                .id("config-terminal-logs-existentes")
                .message(move || t!("config-logs-existentes", n = a_cifrar.get() as i64))
                .grow(1),
        )
        .child(
            Button::new(move || t!("config-logs-existentes-cifrar", n = a_cifrar.get() as i64))
                .id("config-terminal-logs-existentes-cifrar")
                .on_click(move || cifrar()),
        )
}

fn secao_conexoes(p: &Pagina) -> Expander {
    const RECONECTAR: &[&str] = &[
        "conexoes.reconectar.modo",
        "conexoes.reconectar.tentativas",
        "conexoes.reconectar.espera_secs",
    ];
    let (importar_ssh, importar_aws) = (
        Rc::clone(&p.ctx.importar_ssh_config),
        Rc::clone(&p.ctx.importar_aws),
    );
    secao(
        p,
        tk!("config-secao-conexoes"),
        "conexoes",
        lista()
            .child(p.texto(
                "conexoes.usuario_padrao",
                campo!(p, conexoes.usuario_padrao),
                tk!("config-dica-usuario"),
            ))
            .child(p.numero(
                "conexoes.porta_padrao",
                numero_de!(p, 1, 65_535, conexoes.porta_padrao),
                1.0,
                65_535.0,
            ))
            .child(p.texto(
                "conexoes.term_padrao",
                campo!(p, conexoes.term_padrao),
                tk!("config-dica-term"),
            ))
            .child(p.numero(
                "conexoes.keepalive_padrao_secs",
                numero_de!(p, 0, 3600, conexoes.keepalive_padrao_secs),
                0.0,
                3600.0,
            ))
            .child(p.caminho(
                "conexoes.known_hosts",
                campo!(p, conexoes.known_hosts),
                tk!("config-dica-known-hosts"),
                Escolher::Arquivo,
            ))
            .child(p.escolha(
                "conexoes.chave_nova",
                opcao_de!(p, ChaveNova::TODAS, conexoes.chave_nova),
                ChaveNova::nomes(),
            ))
            .child(
                p.grupo("conexoes.reconectar", RECONECTAR)
                    .item(p.escolha(
                        "conexoes.reconectar.modo",
                        opcao_de!(p, ModoReconectar::TODAS, conexoes.reconectar.modo),
                        ModoReconectar::nomes(),
                    ))
                    .item(p.numero(
                        "conexoes.reconectar.tentativas",
                        numero_de!(p, 1, 20, conexoes.reconectar.tentativas),
                        1.0,
                        20.0,
                    ))
                    .item(p.numero(
                        "conexoes.reconectar.espera_secs",
                        numero_de!(p, 1, 300, conexoes.reconectar.espera_secs),
                        1.0,
                        300.0,
                    )),
            )
            .child(p.escolha(
                "conexoes.abrir_em",
                opcao_de!(p, AbrirEm::TODAS, conexoes.abrir_em),
                AbrirEm::nomes(),
            ))
            .child(p.interruptor(
                "conexoes.importar_senhas",
                campo!(p, conexoes.importar_senhas),
            ))
            .child(p.interruptor(
                "conexoes.confirmar_fechar_todas",
                campo!(p, conexoes.confirmar_fechar_todas),
            ))
            .child(p.escolha(
                "conexoes.tmux_ao_fechar",
                opcao_de!(p, TmuxAoFechar::TODAS, conexoes.tmux_ao_fechar),
                TmuxAoFechar::nomes(),
            ))
            .child(p.numero(
                "conexoes.comandos_quieto_ms",
                numero_de!(p, 50, 60_000, conexoes.comandos_quieto_ms),
                50.0,
                60_000.0,
            ))
            .child(p.numero(
                "conexoes.comandos_max_secs",
                numero_de!(p, 1, 86_400, conexoes.comandos_max_secs),
                1.0,
                86_400.0,
            ))
            .child(p.acao(
                "acao.importar_ssh_config",
                Symbol::ArrowDownload,
                move || importar_ssh(),
            ))
            .child(p.acao("acao.importar_aws", Symbol::Cloud, move || importar_aws())),
    )
}

fn secao_scripts(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-scripts"),
        "scripts",
        lista()
            .child(p.texto(
                "scripts.interpretador",
                campo!(p, scripts.interpretador),
                tk!("config-dica-interpretador"),
            ))
            .child(p.numero(
                "scripts.paralelo",
                numero_de!(p, 1, 64, scripts.paralelo),
                1.0,
                64.0,
            ))
            .child(p.caminho(
                "scripts.pasta",
                campo!(p, scripts.pasta),
                tk!("config-dica-scripts"),
                Escolher::Pasta,
            )),
    )
}

/// O local do cofre: onde está, abrir a pasta, mudar (o diálogo da tela pergunta se leva o
/// cofre ou usa o que já está lá) e voltar à pasta de dados.
fn cartao_local_do_cofre(p: &Pagina) -> SettingsExpander {
    let d = catalogo::definicao("acao.cofre_local").expect("catálogo");
    let aberto = signal(false);
    let (busca, achados) = (p.busca, p.achados);
    effect(move || {
        let buscando = !busca.with(|b| b.trim().is_empty());
        if buscando && achados.with(|a| a.contains(&"acao.cofre_local")) {
            aberto.set(true);
        }
    });
    let (cofre, config) = (p.ctx.cofre.clone(), p.config);
    let descricao = {
        let cofre = cofre.clone();
        let dados = p.ctx.armazenamento.pasta().display().to_string();
        move || {
            let pasta = cofre.ler(|c| c.pasta_do_cofre().to_path_buf());
            let pasta = pasta.display().to_string();
            if pasta == dados {
                t!("config-cofre-local-pasta-dados", pasta = pasta)
            } else {
                t!("config-cofre-local-pasta", pasta = pasta)
            }
        }
    };
    let abrir = {
        let (pagina, cofre) = (p.clone(), cofre.clone());
        move || {
            let pasta = cofre.agora(|c| c.pasta_do_cofre().to_path_buf());
            if let Err(erro) = crate::ferramentas::abrir_pasta(&pasta) {
                pagina.falhar(t!(
                    "config-erro-abrir",
                    caminho = pasta.display().to_string(),
                    erro = erro.to_string()
                ));
            }
        }
    };
    let mudar = {
        let mudar_local = Rc::clone(&p.ctx.mudar_local_do_cofre);
        move || {
            let mudar_local = Rc::clone(&mudar_local);
            dialogs::pick_folder()
                .remember("cofre-local")
                .title(t!("config-cofre-local-titulo"))
                .show(move |destino| {
                    if let Some(destino) = destino {
                        mudar_local(Some(destino));
                    }
                });
        }
    };
    let padrao = {
        let mudar_local = Rc::clone(&p.ctx.mudar_local_do_cofre);
        move || mudar_local(None)
    };

    SettingsExpander::new(d.titulo(), aberto)
        .description(descricao)
        .icon(Symbol::Folder)
        .id(d.id())
        .visible(p.achou("acao.cofre_local"))
        .item(
            SettingsCard::new(t!("config-cofre-local-abrir"))
                .id("config-cofre-local-abrir")
                .icon(Symbol::FolderOpen)
                .on_click(abrir),
        )
        .item(
            SettingsCard::new(t!("config-cofre-local-mudar"))
                .id("config-cofre-local-mudar")
                .description(t!("config-cofre-local-mudar.descricao"))
                .icon(Symbol::Folder)
                .on_click(mudar),
        )
        .item(
            SettingsCard::new(t!("config-cofre-local-padrao"))
                .id("config-cofre-local-padrao")
                .icon(Symbol::ArrowUndo)
                .disabled(move || config.with(|c| c.cofre.local.trim().is_empty()))
                .on_click(padrao),
        )
}

fn secao_cofre(p: &Pagina) -> Expander {
    const GERADOR: &[&str] = &[
        "cofre.gerador.tamanho",
        "cofre.gerador.maiusculas",
        "cofre.gerador.minusculas",
        "cofre.gerador.digitos",
        "cofre.gerador.simbolos",
        "cofre.gerador.sem_ambiguos",
    ];
    secao(
        p,
        tk!("config-secao-cofre"),
        "cofre",
        lista()
            .child(p.levar("acao.protecao_cofre", Symbol::Shield, "cofre"))
            .child(cartao_local_do_cofre(p))
            .child({
                let levar = Rc::clone(&p.ctx.levar_cofre);
                p.acao("acao.cofre_levar", Symbol::ArrowUpload, move || levar())
            })
            .child({
                let abrir = Rc::clone(&p.ctx.abrir_cofre_de_fora);
                p.acao("acao.cofre_abrir_outro", Symbol::ArrowDownload, move || {
                    abrir()
                })
            })
            .child(p.numero(
                "cofre.limpar_area_transferencia_secs",
                numero_de!(p, 0, 3600, cofre.limpar_area_transferencia_secs),
                0.0,
                3600.0,
            ))
            .child(p.numero(
                "cofre.bloquear_apos_min",
                numero_de!(p, 0, 1440, cofre.bloquear_apos_min),
                0.0,
                1440.0,
            ))
            .child(
                p.grupo("cofre.gerador", GERADOR)
                    .item(p.numero(
                        "cofre.gerador.tamanho",
                        numero_de!(p, 8, 128, cofre.gerador.tamanho),
                        8.0,
                        128.0,
                    ))
                    .item(p.interruptor(
                        "cofre.gerador.maiusculas",
                        campo!(p, cofre.gerador.maiusculas),
                    ))
                    .item(p.interruptor(
                        "cofre.gerador.minusculas",
                        campo!(p, cofre.gerador.minusculas),
                    ))
                    .item(p.interruptor("cofre.gerador.digitos", campo!(p, cofre.gerador.digitos)))
                    .item(
                        p.interruptor("cofre.gerador.simbolos", campo!(p, cofre.gerador.simbolos)),
                    )
                    .item(p.interruptor(
                        "cofre.gerador.sem_ambiguos",
                        campo!(p, cofre.gerador.sem_ambiguos),
                    )),
            )
            .child(p.levar("acao.csv_cofre", Symbol::Document, "cofre")),
    )
}

fn secao_notas(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-notas"),
        "notas",
        lista()
            .child(p.escolha(
                "notas.modo",
                opcao_de!(p, ModoNotas::TODAS, notas.modo),
                ModoNotas::nomes(),
            ))
            .child(p.interruptor("notas.fonte_mono", campo!(p, notas.fonte_mono)))
            .child(p.interruptor("notas.quebrar_linhas", campo!(p, notas.quebrar_linhas))),
    )
}

fn secao_agente(p: &Pagina) -> Expander {
    const NOVAS: &[&str] = &["agente.tipo_padrao", "agente.rodadas_padrao"];
    secao(
        p,
        tk!("config-secao-agente"),
        "agente",
        lista()
            .child(p.interruptor("agente.ligado", campo!(p, agente.ligado)))
            .child(p.texto(
                "agente.socket",
                campo!(p, agente.socket),
                tk!("config-dica-socket"),
            ))
            .child(p.interruptor(
                "agente.exportar_no_shell",
                campo!(p, agente.exportar_no_shell),
            ))
            .child(p.interruptor(
                "agente.carregar_ao_iniciar",
                campo!(p, agente.carregar_ao_iniciar),
            ))
            .child(p.numero(
                "agente.vida_padrao_secs",
                numero_de!(p, 0, 31_536_000, agente.vida_padrao_secs),
                0.0,
                31_536_000.0,
            ))
            .child(p.interruptor(
                "agente.confirmar_padrao",
                campo!(p, agente.confirmar_padrao),
            ))
            .child(p.interruptor(
                "agente.encaminhar_padrao",
                campo!(p, agente.encaminhar_padrao),
            ))
            .child(p.caminho(
                "agente.pasta_chaves",
                campo!(p, agente.pasta_chaves),
                tk!("config-dica-pasta-chaves"),
                Escolher::Pasta,
            ))
            .child(
                p.grupo("agente.chaves_novas", NOVAS)
                    .item(p.escolha(
                        "agente.tipo_padrao",
                        opcao_de!(p, TipoChave::TODAS, agente.tipo_padrao),
                        TipoChave::nomes(),
                    ))
                    .item(p.numero(
                        "agente.rodadas_padrao",
                        numero_de!(p, 16, 1000, agente.rodadas_padrao),
                        16.0,
                        1000.0,
                    )),
            )
            .child(p.interruptor(
                "agente.sistema_ssh_config",
                campo!(p, agente.sistema_ssh_config),
            ))
            .child(p.interruptor("agente.sistema_shells", campo!(p, agente.sistema_shells)))
            .child(p.interruptor("agente.sistema_sessao", campo!(p, agente.sistema_sessao)))
            .child(p.interruptor("agente.sistema_git", campo!(p, agente.sistema_git)))
            .child(p.interruptor(
                "agente.tirar_ao_bloquear",
                campo!(p, agente.tirar_ao_bloquear),
            ))
            .child(p.interruptor("agente.pageant", campo!(p, agente.pageant)))
            .child(p.levar("acao.gerenciar_chaves", Symbol::Key, "chaves")),
    )
}

fn secao_arquivos(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-arquivos"),
        "arquivos",
        lista()
            .child(p.numero(
                "arquivos.paralelas",
                numero_de!(p, 1, 8, arquivos.paralelas),
                1.0,
                8.0,
            ))
            .child(p.escolha(
                "arquivos.conflito",
                opcao_de!(p, ConflitoCfg::TODAS, arquivos.conflito),
                ConflitoCfg::nomes(),
            ))
            .child(p.interruptor(
                "arquivos.mostrar_ocultos",
                campo!(p, arquivos.mostrar_ocultos),
            ))
            .child(p.caminho(
                "arquivos.editor",
                campo!(p, arquivos.editor),
                tk!("config-dica-editor"),
                Escolher::Arquivo,
            ))
            .child(p.escolha(
                "arquivos.enviar_editado",
                opcao_de!(p, EnviarEditado::TODAS, arquivos.enviar_editado),
                EnviarEditado::nomes(),
            ))
            .child(p.interruptor(
                "arquivos.preservar_data",
                campo!(p, arquivos.preservar_data),
            ))
            .child(p.interruptor(
                "arquivos.preservar_permissoes",
                campo!(p, arquivos.preservar_permissoes),
            ))
            .child(p.caminho(
                "arquivos.pasta_local",
                campo!(p, arquivos.pasta_local),
                tk!("config-dica-pasta-local"),
                Escolher::Pasta,
            ))
            .child(p.interruptor(
                "arquivos.confirmar_apagar",
                campo!(p, arquivos.confirmar_apagar),
            ))
            .child(p.numero(
                "arquivos.keepalive_secs",
                numero_de!(p, 0, 3600, arquivos.keepalive_secs),
                0.0,
                3600.0,
            ))
            .child(p.interruptor("arquivos.comparar", campo!(p, arquivos.comparar)))
            .child(p.escolha(
                "arquivos.comparar_modo",
                opcao_de!(p, ModoComparar::TODAS, arquivos.comparar_modo),
                ModoComparar::nomes(),
            ))
            .child(p.numero(
                "arquivos.comparar_tolerancia_min",
                numero_de!(p, 0, 1440, arquivos.comparar_tolerancia_min),
                0.0,
                1440.0,
            ))
            .child(p.interruptor(
                "arquivos.ocultar_identicos",
                campo!(p, arquivos.ocultar_identicos),
            ))
            .child(p.interruptor("arquivos.sincronizar", campo!(p, arquivos.sincronizar))),
    )
}

fn secao_ftp(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-ftp"),
        "ftp",
        lista()
            .child(p.escolha(
                "ftp.seguranca",
                opcao_de!(p, FtpSeguranca::TODAS, ftp.seguranca),
                nomes(&FtpSeguranca::TODAS, FtpSeguranca::nome),
            ))
            .child(p.escolha(
                "ftp.modo",
                opcao_de!(p, FtpModo::TODOS, ftp.modo),
                nomes(&FtpModo::TODOS, FtpModo::nome),
            ))
            .child(p.escolha(
                "ftp.tls",
                opcao_de!(p, FtpTls::TODAS, ftp.tls),
                nomes(&FtpTls::TODAS, FtpTls::nome),
            ))
            .child(p.escolha(
                "ftp.codificacao",
                opcao_de!(p, FtpCodificacao::TODAS, ftp.codificacao),
                nomes(&FtpCodificacao::TODAS, FtpCodificacao::nome),
            )),
    )
}

fn secao_s3(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-s3"),
        "s3",
        lista()
            .child(p.texto("s3.regiao", campo!(p, s3.regiao), tk!("config-dica-regiao")))
            .child(p.escolha(
                "s3.estilo",
                opcao_de!(p, S3Estilo::TODOS, s3.estilo),
                nomes(&S3Estilo::TODOS, S3Estilo::nome),
            ))
            .child(p.numero(
                "s3.parte_mib",
                numero_de!(p, 5, 512, s3.parte_mib),
                5.0,
                512.0,
            ))
            .child(p.texto("s3.classe", campo!(p, s3.classe), tk!("config-dica-classe")))
            .child(p.escolha(
                "s3.validade_link",
                opcao_de!(p, ValidadeLink::TODAS, s3.validade_link),
                ValidadeLink::nomes(),
            )),
    )
}

fn secao_rclone(p: &Pagina) -> Expander {
    let config = p.config;
    let pagina = p.clone();
    secao(
        p,
        tk!("config-secao-rclone"),
        "rclone",
        lista()
            .child(p.caminho(
                "rclone.binario",
                campo!(p, rclone.binario),
                tk!("config-dica-rclone"),
                Escolher::Arquivo,
            ))
            .child(p.caminho(
                "rclone.arquivo_config",
                campo!(p, rclone.arquivo_config),
                tk!("config-dica-rclone-config"),
                Escolher::Arquivo,
            ))
            .child(cartao_credencial(p))
            .child(p.acao("acao.testar_rclone", Symbol::Beaker, move || {
                let binario = config.with_untracked(|c| c.rclone.binario.clone());
                pagina.testar_rclone(&binario);
            })),
    )
}

impl Pagina {
    /// Procura o rclone e mostra a versão.
    fn testar_rclone(&self, binario: &str) {
        let configurado = (!binario.trim().is_empty()).then(|| Path::new(binario.trim()));
        let Some(caminho) = find_rclone(configurado) else {
            self.falhar(t!("config-rclone-nao-achei"));
            return;
        };
        let mut comando = std::process::Command::new(&caminho);
        match crate::sistema::sem_janela(&mut comando)
            .arg("version")
            .output()
        {
            Ok(saida) if saida.status.success() => {
                let texto = String::from_utf8_lossy(&saida.stdout);
                let versao = texto.lines().next().unwrap_or("rclone");
                self.informar(format!("{versao} ({})", caminho.display()));
            }
            Ok(saida) => self.falhar(t!(
                "config-rclone-terminou",
                programa = caminho.display().to_string(),
                status = saida.status.to_string()
            )),
            Err(erro) => self.falhar(t!(
                "config-rclone-erro-rodar",
                programa = caminho.display().to_string(),
                erro = erro.to_string()
            )),
        }
    }
}

/// A senha da configuração cifrada do rclone: o título de um item do cofre (a senha em si
/// nunca sai de lá).
fn cartao_credencial(p: &Pagina) -> SettingsCard {
    let (config, cofre) = (p.config, p.ctx.cofre.clone());
    let titulo_de = {
        let cofre = cofre.clone();
        move |id: Option<u64>| -> String {
            let Some(id) = id else {
                return String::new();
            };
            cofre.agora(|c| {
                if c.bloqueado() {
                    return String::new();
                }
                c.itens()
                    .iter()
                    .find(|i| i.id == id)
                    .map(|i| i.titulo.clone())
                    .unwrap_or_default()
            })
        }
    };
    let texto = signal(titulo_de(config.with_untracked(|c| c.rclone.credencial)));
    let credencial = memo(move || config.with(|c| c.rclone.credencial));
    {
        let titulo_de = titulo_de.clone();
        effect(move || {
            let novo = titulo_de(credencial.get());
            untrack(|| {
                if texto.get_untracked() != novo {
                    texto.set(novo);
                }
            });
        });
    }
    let sugerir = {
        let cofre = cofre.clone();
        move |procurado: &str| -> Vec<String> {
            let procurado = normalizar(procurado.trim());
            cofre.agora(|c| {
                if c.bloqueado() {
                    return Vec::new();
                }
                c.itens()
                    .iter()
                    .filter(|i| normalizar(&i.titulo).contains(&procurado))
                    .map(|i| i.titulo.clone())
                    .take(8)
                    .collect()
            })
        }
    };
    let pagina = p.clone();
    p.card("rclone.credencial").content(
        AutoSuggestBox::bind(texto, sugerir)
            .id(id_do_valor("rclone.credencial"))
            .placeholder(t!("config-credencial-nenhuma"))
            .width(260)
            .on_submit(move |enviado: String| {
                let enviado = enviado.trim();
                if enviado.is_empty() {
                    config.update(|c| c.rclone.credencial = None);
                    return;
                }
                let id = cofre.agora(|c| {
                    if c.bloqueado() {
                        return None;
                    }
                    c.itens().iter().find(|i| i.titulo == enviado).map(|i| i.id)
                });
                match id {
                    Some(id) => config.update(|c| c.rclone.credencial = Some(id)),
                    None => pagina.falhar(t!("config-credencial-inexistente", titulo = enviado)),
                }
            }),
    )
}

fn secao_redes(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-redes"),
        "redes",
        lista()
            .child(p.levar("acao.perfis_de_rede", Symbol::Globe, "redes"))
            .child(p.numero(
                "redes.espera_padrao_secs",
                numero_de!(p, 1, 600, redes.espera_padrao_secs),
                1.0,
                600.0,
            ))
            .child(p.interruptor("redes.descer_ao_fechar", campo!(p, redes.descer_ao_fechar))),
    )
}

fn secao_x11(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-x11"),
        "x11",
        lista()
            .child(p.escolha(
                "x11.padrao",
                opcao_de!(p, X11Padrao::TODAS, x11.padrao),
                X11Padrao::nomes(),
            ))
            .child(p.texto(
                "x11.display",
                campo!(p, x11.display),
                tk!("config-dica-display"),
            ))
            .child(p.texto(
                "x11.virt_manager",
                campo!(p, x11.virt_manager),
                tk!("config-dica-virt-manager"),
            ))
            .child(p.interruptor("tuneis.outras_maquinas", campo!(p, tuneis.outras_maquinas))),
    )
}

fn secao_portas(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-portas"),
        "portas",
        lista()
            .child(p.numero(
                "portas.intervalo_secs",
                numero_de!(p, 0, u32::MAX, portas.intervalo_secs),
                0.0,
                f64::from(u32::MAX),
            ))
            .child(p.interruptor("portas.sondar", campo!(p, portas.sondar)))
            .child(p.numero(
                "portas.sonda_ms",
                numero_de!(p, 100, u32::MAX, portas.sonda_ms),
                100.0,
                f64::from(u32::MAX),
            ))
            .child(p.interruptor(
                "portas.usar_ferramentas",
                campo!(p, portas.usar_ferramentas),
            ))
            .child(p.interruptor("portas.avisar_novas", campo!(p, portas.avisar_novas)))
            .child(p.interruptor(
                "portas.links_pelo_tunel",
                campo!(p, portas.links_pelo_tunel),
            ))
            .child(p.interruptor("portas.mostrar_sistema", campo!(p, portas.mostrar_sistema))),
    )
}

fn secao_janelas(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-janelas"),
        "janelas",
        lista()
            .child(p.caminho(
                "janelas.pasta_espacos",
                campo!(p, janelas.pasta_espacos),
                tk!("config-dica-espacos"),
                Escolher::Pasta,
            ))
            .child(p.escolha(
                "janelas.abrir_espaco",
                opcao_de!(p, AbrirEspaco::TODAS, janelas.abrir_espaco),
                AbrirEspaco::nomes(),
            ))
            .child(p.interruptor("janelas.tear_off", campo!(p, janelas.tear_off))),
    )
}

/// Os assistentes de IA (M52): o botão dos provedores e as preferências do que vai e do que
/// volta. Os provedores em si moram no `ia.json`, na página própria.
fn secao_ia(p: &Pagina) -> Expander {
    let extras = ligar(
        p.config,
        |c| c.ia.redigir_extras.join("\n"),
        |c, v| {
            c.ia.redigir_extras = if v.is_empty() {
                Vec::new()
            } else {
                v.split('\n').map(str::to_owned).collect()
            };
        },
    );
    let invalidos = {
        let config = p.config;
        move || {
            config.with(|c| {
                crate::redacao::Redator::novo(Vec::new(), &c.ia.redigir_extras)
                    .invalidos()
                    .iter()
                    .map(|i| {
                        t!(
                            "config-ia-extra-invalido",
                            texto = i.texto.as_str(),
                            erro = i.erro.as_str()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }
    };
    let tem_invalido = { move || !invalidos().is_empty() };
    secao(
        p,
        tk!("config-secao-ia"),
        "ia",
        lista()
            .child(p.levar("acao.perfis_de_ia", Symbol::Sparkle, "ia"))
            .child(p.interruptor("ia.redigir", campo!(p, ia.redigir)))
            .child(
                p.card("ia.redigir_extras").content(
                    TextEditor::bind(extras)
                        .id(id_do_valor("ia.redigir_extras"))
                        .placeholder(t!("config-dica-redigir-extras"))
                        .monospace()
                        .height(88)
                        .width(320),
                ),
            )
            .child(
                Text::new(invalidos)
                    .id("config-ia-redigir_extras-erro")
                    .secondary()
                    .wrap()
                    .visible(tem_invalido),
            )
            .child(p.interruptor("ia.mostrar_no_menu", campo!(p, ia.mostrar_no_menu)))
            .child(p.interruptor("ia.mostrar_o_que_sai", campo!(p, ia.mostrar_o_que_sai)))
            .child(p.numero(
                "ia.linhas_contexto",
                numero_de!(p, 1, 100_000, ia.linhas_contexto),
                1.0,
                100_000.0,
            ))
            .child(p.numero(
                "ia.max_caracteres",
                numero_de!(p, 0, 100_000_000, ia.max_caracteres),
                0.0,
                100_000_000.0,
            ))
            .child(p.texto(
                "ia.comando_shell",
                campo!(p, ia.comando_shell),
                tk!("config-dica-shell-ia"),
            ))
            .child(p.numero(
                "ia.espera_resposta",
                numero_de!(p, 0, 3600, ia.espera_resposta),
                0.0,
                3600.0,
            )),
    )
}

/// Os assistentes de IA no servidor (M46): o que o atalho abre, onde abre, quais aparecem nos
/// menus, como instalar e os argumentos de cada um.
fn secao_ia_servidor(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-ia-servidor"),
        "ia_servidor",
        lista()
            .child(p.escolha(
                "ia.preferido",
                opcao_de!(p, AssistentePreferido::TODAS, ia.preferido),
                AssistentePreferido::nomes(),
            ))
            .child(p.escolha(
                "ia.abrir_em",
                opcao_de!(p, AbrirIa::TODAS, ia.abrir_em),
                AbrirIa::nomes(),
            ))
            .child(p.interruptor("ia.shell_ao_sair", campo!(p, ia.shell_ao_sair)))
            .child(p.interruptor("ia.tunel_codex", campo!(p, ia.tunel_codex)))
            .child(p.interruptor("ia.verificar_ao_abrir", campo!(p, ia.verificar_ao_abrir)))
            .child(p.interruptor("ia.mostrar_botao", campo!(p, ia.mostrar_botao)))
            .child(p.interruptor("ia.mostrar_claude", campo!(p, ia.mostrar_claude)))
            .child(p.interruptor("ia.mostrar_codex", campo!(p, ia.mostrar_codex)))
            .child(p.interruptor("ia.mostrar_opencode", campo!(p, ia.mostrar_opencode)))
            .child(p.escolha(
                "ia.metodo_claude",
                opcao_de!(p, MetodoIa::TODAS, ia.metodo_claude),
                MetodoIa::nomes(),
            ))
            .child(p.escolha(
                "ia.metodo_codex",
                opcao_de!(p, MetodoIa::TODAS, ia.metodo_codex),
                MetodoIa::nomes(),
            ))
            .child(p.escolha(
                "ia.metodo_opencode",
                opcao_de!(p, MetodoIa::TODAS, ia.metodo_opencode),
                MetodoIa::nomes(),
            ))
            .child(p.texto(
                "ia.argumentos_claude",
                campo!(p, ia.argumentos_claude),
                tk!("config-dica-argumentos-ia"),
            ))
            .child(p.texto(
                "ia.argumentos_codex",
                campo!(p, ia.argumentos_codex),
                tk!("config-dica-argumentos-ia"),
            ))
            .child(p.texto(
                "ia.argumentos_opencode",
                campo!(p, ia.argumentos_opencode),
                tk!("config-dica-argumentos-ia"),
            )),
    )
}

/// Os assistentes deste computador (M63): o caminho e os argumentos extras de cada um, usados
/// pelo "Controlar com" do menu do terminal.
fn secao_ia_local(p: &Pagina) -> Expander {
    secao(
        p,
        tk!("config-secao-ia-local"),
        "ia_local",
        lista()
            .child(
                p.caminho_com_dica(
                    "ia.local_pasta",
                    campo!(p, ia.local_pasta),
                    (p.ctx.pasta_dos_assistentes)("")
                        .map(|pasta| pasta.display().to_string())
                        .unwrap_or_default(),
                    Escolher::Pasta,
                ),
            )
            .child(p.acao("acao.ia_local_abrir_pasta", Symbol::Folder, {
                let p = p.clone();
                move || p.abrir_pasta_dos_assistentes()
            }))
            .child(p.acao("acao.ia_local_limpar", Symbol::Delete, {
                let p = p.clone();
                move || p.pedir_para_limpar_a_pasta()
            }))
            .child(p.caminho(
                "ia.local_claude_caminho",
                campo!(p, ia.local_claude_caminho),
                tk!("config-dica-caminho-ia-local"),
                Escolher::Arquivo,
            ))
            .child(p.texto(
                "ia.local_claude_argumentos",
                campo!(p, ia.local_claude_argumentos),
                tk!("config-dica-argumentos-ia"),
            ))
            .child(p.caminho(
                "ia.local_codex_caminho",
                campo!(p, ia.local_codex_caminho),
                tk!("config-dica-caminho-ia-local"),
                Escolher::Arquivo,
            ))
            .child(p.texto(
                "ia.local_codex_argumentos",
                campo!(p, ia.local_codex_argumentos),
                tk!("config-dica-argumentos-ia"),
            ))
            .child(p.caminho(
                "ia.local_opencode_caminho",
                campo!(p, ia.local_opencode_caminho),
                tk!("config-dica-caminho-ia-local"),
                Escolher::Arquivo,
            ))
            .child(p.texto(
                "ia.local_opencode_argumentos",
                campo!(p, ia.local_opencode_argumentos),
                tk!("config-dica-argumentos-ia"),
            ))
            .child(p.interruptor("ia.conversas_lembrar", campo!(p, ia.conversas_lembrar)))
            .child(p.interruptor("ia.conversas_reabrir", campo!(p, ia.conversas_reabrir)))
            .child(p.escolha(
                "ia.conversas_ao_controlar",
                opcao_de!(p, ConversaAoControlar::TODAS, ia.conversas_ao_controlar),
                ConversaAoControlar::nomes(),
            )),
    )
}

/// O "Limpar a pasta de trabalho?": mostra a pasta e quantos itens saem.
fn dialogo_limpar_pasta(p: &Pagina) -> impl View + use<> {
    let alvo = p.limpar_alvo;
    let texto = move || {
        alvo.with(|a| {
            a.as_ref().map_or_else(String::new, |(pasta, n)| {
                t!(
                    "config-limpar-pasta-texto",
                    n = *n as i64,
                    pasta = pasta.display().to_string()
                )
            })
        })
    };
    let apagar = p.clone();
    ContentDialog::new(t!("config-limpar-pasta-titulo"), p.limpar_aberto)
        .id("config-limpar-pasta")
        .content(Text::new(texto).id("config-limpar-pasta-texto").wrap())
        .primary(t!("config-limpar-pasta-apagar"), move || {
            apagar.limpar_a_pasta()
        })
        .close(t!("comum-cancelar"))
}

/// O controle por assistentes (M51): o interruptor com o aviso da primeira vez, o "Como ligar",
/// os espaços de trabalho, o registro e o que vale para as ferramentas.
fn secao_mcp(p: &Pagina) -> Expander {
    let ligado = signal(p.config.with_untracked(|c| c.ia.mcp_ligado));
    {
        // A primeira ativação não liga: mostra o aviso, e o "Ligar" do aviso é que liga.
        let (config, aviso) = (p.config, Rc::clone(&p.ctx.mcp_aviso));
        effect(move || {
            let novo = ligado.get();
            untrack(|| {
                let (atual, avisado) =
                    config.with_untracked(|c| (c.ia.mcp_ligado, c.ia.mcp_avisado));
                if novo == atual {
                    return;
                }
                if novo && !avisado {
                    ligado.set(false);
                    aviso();
                    return;
                }
                config.update(|c| c.ia.mcp_ligado = novo);
            });
        });
        let config = p.config;
        effect(move || {
            let atual = config.with(|c| c.ia.mcp_ligado);
            untrack(|| {
                if ligado.get_untracked() != atual {
                    ligado.set(atual);
                }
            });
        });
    }
    let bloqueio = ligar(
        p.config,
        |c| c.ia.bloqueio_extras.join("\n"),
        |c, v| c.ia.bloqueio_extras = em_linhas(&v),
    );
    let finais = ligar(
        p.config,
        |c| c.ia.finais_de_prompt.join("\n"),
        |c, v| c.ia.finais_de_prompt = em_linhas(&v),
    );
    let estado = {
        let ctx = Rc::clone(&p.ctx);
        move || (ctx.mcp_estado)()
    };
    let registro = {
        let p = p.clone();
        move || p.abrir_registro_do_dia()
    };
    let pasta_do_registro = {
        let p = p.clone();
        move || p.abrir_pasta_do_registro()
    };
    let como = {
        let ctx = Rc::clone(&p.ctx);
        move || (ctx.mcp_como)(None)
    };
    let novo = {
        let ctx = Rc::clone(&p.ctx);
        move || (ctx.espaco_novo)()
    };
    secao(
        p,
        tk!("config-secao-mcp"),
        "mcp",
        lista()
            .child(
                p.card("ia.mcp_ligado")
                    .content(ToggleSwitch::bind(ligado).id(id_do_valor("ia.mcp_ligado"))),
            )
            .child(
                Text::new(estado)
                    .id("mcp-estado")
                    .secondary()
                    .wrap()
                    .visible(p.achou("ia.mcp_ligado")),
            )
            .child(p.acao("acao.mcp_como", Symbol::Sparkle, como))
            .child(p.acao("acao.mcp_espaco_novo", Symbol::Add, novo))
            .child(lista_de_espacos(p))
            .child(
                p.card("ia.bloqueio_extras").content(
                    TextEditor::bind(bloqueio)
                        .id(id_do_valor("ia.bloqueio_extras"))
                        .placeholder(t!("config-dica-bloqueio-mcp"))
                        .monospace()
                        .height(88)
                        .width(320),
                ),
            )
            .child(p.interruptor("ia.auditoria", campo!(p, ia.auditoria)))
            .child(p.acao("acao.mcp_registro", Symbol::Document, registro))
            .child(p.acao("acao.mcp_pasta_registro", Symbol::Folder, pasta_do_registro))
            .child(p.numero(
                "ia.marca_secs",
                numero_de!(p, 0, 3600, ia.marca_secs),
                0.0,
                3600.0,
            ))
            .child(p.numero(
                "ia.linhas_por_leitura",
                numero_de!(p, 1, 100_000, ia.linhas_por_leitura),
                1.0,
                100_000.0,
            ))
            .child(p.numero(
                "ia.caracteres_por_resposta",
                numero_de!(p, 0, 100_000_000, ia.caracteres_por_resposta),
                0.0,
                100_000_000.0,
            ))
            .child(p.interruptor("ia.comandos_na_aba", campo!(p, ia.comandos_na_aba)))
            .child(
                p.card("ia.finais_de_prompt").content(
                    TextEditor::bind(finais)
                        .id(id_do_valor("ia.finais_de_prompt"))
                        .placeholder(t!("config-dica-finais-prompt"))
                        .monospace()
                        .height(88)
                        .width(320),
                ),
            )
            .child(p.caminho(
                "ia.pasta_troca",
                campo!(p, ia.pasta_troca),
                tk!("config-dica-pasta-troca"),
                Escolher::Pasta,
            )),
    )
}

/// O texto de um editor de várias linhas como lista (vazio é lista vazia).
fn em_linhas(texto: &str) -> Vec<String> {
    if texto.is_empty() {
        Vec::new()
    } else {
        texto.split('\n').map(str::to_owned).collect()
    }
}

/// A lista dos espaços de trabalho: um cartão por espaço, com "Editar…", "Como ligar…" e "Apagar".
fn lista_de_espacos(p: &Pagina) -> Column {
    let config = p.config;
    let pagina = p.clone();
    Column::new()
        .id("mcp-espacos")
        .gap(4)
        .visible(p.achou_alguma(&["acao.mcp_espaco_novo"]))
        .child(
            Text::new(t!("mcp-espacos-vazio"))
                .id("mcp-espacos-vazio")
                .secondary()
                .visible(move || config.with(|c| c.ia.espacos.is_empty())),
        )
        .child(For::each(
            move || config.with(|c| c.ia.espacos.clone()),
            |e| format!("{}|{}|{}", e.nome, e.conexao, e.raiz),
            move |e| cartao_do_espaco(&pagina, &e),
        ))
}

/// O cartão de um espaço de trabalho.
fn cartao_do_espaco(p: &Pagina, espaco: &crate::mcp::espaco::EspacoIa) -> SettingsCard {
    let conexao = p.ctx.arvore.with_untracked(|t| {
        t.connection(espaco.conexao)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| t!("mcp-espaco-conexao-apagada"))
    });
    let nome = espaco.nome.clone();
    let editar = {
        let (ctx, nome) = (Rc::clone(&p.ctx), nome.clone());
        move || (ctx.espaco_editar)(nome.clone())
    };
    let como = {
        let (ctx, nome) = (Rc::clone(&p.ctx), nome.clone());
        move || (ctx.mcp_como)(Some(nome.clone()))
    };
    let apagar = {
        let (p, nome) = (p.clone(), nome.clone());
        move || {
            let mut nova = p.config.get_untracked();
            nova.ia.espacos.retain(|e| e.nome != nome);
            p.trocar(nova, t!("mcp-espaco-apagado", nome = nome.as_str()));
        }
    };
    SettingsCard::new(t!(
        "mcp-espaco-item",
        nome = espaco.nome.as_str(),
        conexao = conexao.as_str(),
        raiz = espaco.raiz.as_str()
    ))
    .id(format!("mcp-espaco-{nome}"))
    .content(
        Row::new()
            .gap(8)
            .child(
                Button::new(t!("mcp-espaco-como"))
                    .id(format!("mcp-espaco-como-{nome}"))
                    .on_click(como),
            )
            .child(
                Button::new(t!("mcp-espaco-editar"))
                    .id(format!("mcp-espaco-editar-{nome}"))
                    .on_click(editar),
            )
            .child(
                Button::new(t!("comum-apagar"))
                    .id(format!("mcp-espaco-apagar-{nome}"))
                    .on_click(apagar),
            ),
    )
}

impl Pagina {
    /// Abre o registro do assistente de hoje no editor.
    fn abrir_registro_do_dia(&self) {
        let pasta = self.ctx.armazenamento.pasta().to_path_buf();
        let hoje = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
        let arquivo = crate::mcp::auditoria::arquivo_do_dia(&pasta, &hoje);
        if !arquivo.exists() {
            self.informar(t!("mcp-registro-vazio"));
            return;
        }
        let editor = self.config.with_untracked(|c| c.arquivos.editor.clone());
        match crate::ferramentas::abrir_no_editor(&arquivo, &editor) {
            Ok(()) => self.erro.set(String::new()),
            Err(e) => self.falhar(t!("mcp-registro-erro", erro = e.to_string())),
        }
    }

    /// A pasta de trabalho dos assistentes que vale agora.
    fn pasta_dos_assistentes(&self) -> Option<PathBuf> {
        let escolhida = self.config.with_untracked(|c| c.ia.local_pasta.clone());
        (self.ctx.pasta_dos_assistentes)(&escolhida)
    }

    /// Abre a pasta de trabalho dos assistentes no gerenciador de arquivos, criando-a se falta.
    fn abrir_pasta_dos_assistentes(&self) {
        let Some(pasta) = self.pasta_dos_assistentes() else {
            return;
        };
        let aberta =
            std::fs::create_dir_all(&pasta).and_then(|()| crate::ferramentas::abrir_pasta(&pasta));
        if let Err(e) = aberta {
            self.falhar(t!(
                "config-abrir-pasta-erro",
                pasta = pasta.display().to_string(),
                erro = e.to_string()
            ));
        }
    }

    /// O "Limpar a pasta de trabalho…": confere a pasta e abre o diálogo com o que vai sair.
    fn pedir_para_limpar_a_pasta(&self) {
        let Some(pasta) = self.pasta_dos_assistentes() else {
            return;
        };
        if !crate::ferramentas::pode_esvaziar(&pasta, self.ctx.pasta_pessoal.as_deref()) {
            return self.falhar(t!("config-limpar-pasta-recusada"));
        }
        let n = crate::ferramentas::itens_da_pasta(&pasta);
        if n == 0 {
            return self.informar(t!("config-limpar-pasta-vazia"));
        }
        self.limpar_alvo.set(Some((pasta, n)));
        self.limpar_aberto.set(true);
    }

    /// O "Apagar" do diálogo: esvazia a pasta e diz quantos itens saíram.
    fn limpar_a_pasta(&self) {
        let Some((pasta, _)) = self.limpar_alvo.get_untracked() else {
            return;
        };
        match crate::ferramentas::esvaziar_pasta(&pasta) {
            (n, None) => self.informar(t!("config-limpar-pasta-feito", n = n as i64)),
            (n, Some(e)) => self.falhar(t!(
                "config-limpar-pasta-erro",
                n = n as i64,
                erro = e.to_string()
            )),
        }
    }

    /// Abre a pasta do registro no gerenciador de arquivos.
    fn abrir_pasta_do_registro(&self) {
        let pasta = crate::mcp::auditoria::pasta_do_registro(self.ctx.armazenamento.pasta());
        let aberta =
            std::fs::create_dir_all(&pasta).and_then(|()| crate::ferramentas::abrir_pasta(&pasta));
        if let Err(e) = aberta {
            self.falhar(t!("mcp-registro-erro", erro = e.to_string()));
        }
    }
}

/// Os atalhos: um campo por ação, que lê "Ctrl+Shift+N" (vários separados por vírgula).
fn secao_atalhos(p: &Pagina) -> Expander {
    let grupo = |id: &'static str, de: Grupo| {
        let ids: &'static [&'static str] = Box::leak(
            atalhos::CATALOGO
                .iter()
                .filter(|a| a.grupo == de)
                .map(|a| a.id)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let aberto = signal(false);
        let (achados, busca) = (p.atalhos_achados, p.busca);
        effect(move || {
            let achou = achados.with(|a| ids.iter().any(|i| a.contains(i)));
            if achou && !busca.with(|b| b.trim().is_empty()) {
                aberto.set(true);
            }
        });
        let mut expansor = SettingsExpander::new(t!(id), aberto)
            .description(t!(&format!("{id}.descricao")))
            .id(id)
            .visible(move || achados.with(|a| ids.iter().any(|i| a.contains(i))));
        for acao in atalhos::CATALOGO.iter().filter(|a| a.grupo == de) {
            expansor = expansor.item(cartao_atalho(p, acao));
        }
        expansor
    };
    let pagina = p.clone();
    let tem_atalho = p.atalhos_achados;
    let corpo = Column::new()
        .gap(4)
        .child(grupo(tk!("config-atalhos-app"), Grupo::TirechOmni))
        .child(grupo(tk!("config-atalhos-paineis"), Grupo::Paineis))
        .child(
            Row::new().child(
                Button::new(t!("config-restaurar-atalhos"))
                    .id("config-restaurar-atalhos")
                    .hyperlink()
                    .on_click(move || pagina.restaurar("atalhos", &t!("config-titulo-atalhos"))),
            ),
        );
    sanfona(
        p,
        tk!("config-titulo-atalhos"),
        "atalhos",
        move || tem_atalho.with(|a| !a.is_empty()),
        corpo,
    )
}

/// O card de uma ação: o campo com as teclas e o "Restaurar".
fn cartao_atalho(p: &Pagina, acao: &'static atalhos::Acao) -> SettingsCard {
    let (config, id) = (p.config, acao.id);
    let padrao = acao.padrao();
    let padrao_texto = atalhos::texto(&padrao);
    let texto = signal(config.with_untracked(|c| atalhos::texto(&atalhos::atalhos_de(c, id))));
    let erro = signal(String::new());

    // O que se digita vira atalho quando é legível; vazio volta ao padrão.
    {
        let padrao = padrao.clone();
        effect(move || {
            let digitado = texto.get();
            untrack(|| match atalhos::ler(&digitado) {
                Ok(lidos) => {
                    erro.set(String::new());
                    let novos = if lidos.is_empty() {
                        padrao.clone()
                    } else {
                        lidos
                    };
                    let atuais = config.with_untracked(|c| atalhos::atalhos_de(c, id));
                    if atuais != novos {
                        config.update(|c| atalhos::definir(c, id, &novos));
                    }
                }
                Err(motivo) => erro.set(motivo),
            });
        });
    }
    // O que muda na configuração (restaurar, importar, desfazer) volta ao campo.
    effect(move || {
        let atuais = config.with(|c| atalhos::atalhos_de(c, id));
        untrack(|| {
            let digitado = texto.get_untracked();
            if digitado.trim().is_empty() {
                return;
            }
            let igual = atalhos::ler(&digitado).is_ok_and(|lidos| lidos == atuais);
            if !igual {
                texto.set(atalhos::texto(&atuais));
            }
        });
    });

    let descricao = move || {
        let erro = erro.get();
        if !erro.is_empty() {
            return erro;
        }
        let mut descricao = if padrao_texto.is_empty() {
            t!("config-atalho-sem-padrao")
        } else {
            t!("config-atalho-padrao", atalho = padrao_texto.as_str())
        };
        config.with(|c| {
            if let Some(outra) = atalhos::tambem_usado_por(c, id) {
                descricao.push(' ');
                descricao.push_str(&t!("config-atalho-tambem", outra = outra));
            }
            if let Some(aviso) = atalhos::tira_do_terminal(&atalhos::atalhos_de(c, id)) {
                descricao.push(' ');
                descricao.push_str(&aviso);
            }
        });
        descricao
    };
    let achados = p.atalhos_achados;
    SettingsCard::new(acao.nome())
        .id(format!("config-atalho-{id}"))
        .description(descricao)
        .visible(move || achados.with(|a| a.contains(&id)))
        .content(
            Row::new()
                .gap(8)
                .child(
                    Edit::bind(texto)
                        .id(format!("config-atalho-{id}-valor"))
                        .placeholder(t!("config-dica-atalho"))
                        .width(240),
                )
                .child(
                    Button::new(t!("config-atalho-restaurar"))
                        .id(format!("config-atalho-{id}-restaurar"))
                        .on_click(move || {
                            config.update(|c| atalhos::definir(c, id, &[]));
                        }),
                ),
        )
}

/// O fim da página: exportar, importar e restaurar tudo.
fn rodape(p: &Pagina) -> Column {
    let (exportar, importar, restaurar) = (p.clone(), p.clone(), p.clone());
    let (achados, busca) = (p.achados, p.busca);
    let visivel = move |chave: &'static str| move || achados.with(|a| a.contains(&chave));
    let _ = busca;
    Column::new()
        .id("config-rodape")
        .gap(4)
        .child(
            p.card("acao.exportar")
                .icon(Symbol::ArrowUpload)
                .visible(visivel("acao.exportar"))
                .on_click(move || exportar.exportar()),
        )
        .child(
            p.card("acao.importar")
                .icon(Symbol::ArrowDownload)
                .visible(visivel("acao.importar"))
                .on_click(move || importar.importar()),
        )
        .child(
            p.card("acao.restaurar_tudo")
                .icon(Symbol::ArrowUndo)
                .visible(visivel("acao.restaurar_tudo"))
                .on_click(move || restaurar.restaurar_tudo()),
        )
}
