//! A página "Arquivos": os painéis local e remoto lado a lado, uma aba por conexão, e a fila de
//! transferências embaixo. Este módulo monta as views; a ligação com a `Tela` (abrir abas,
//! senha, recentes, terminal) fica em `tela/arquivos.rs`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tomni_ui::files::ftp::NOTICE_NO_TLS;
use tomni_ui::files::{Conflict, Endpoint, Entry, Opener as ArquivosOpener, Remote, Transfers};
use tomni_ui::prelude::*;
use tomni_ui::terminal::Session;

use crate::abrir::Disco;
use crate::configuracoes::{Arquivos, Configuracoes};
use crate::inicio;
use crate::paineis::PaineisDaPagina;
use crate::recentes::Alvo as AlvoRecente;
use crate::t;
use comparar::{ModoComparar, comparar};

pub mod comparar;
pub mod editar;
pub mod fila;
pub mod formato;
pub mod painel;
pub mod rclone;
pub mod s3;
pub mod sincronia;

pub use painel::{Depois, Lado, Painel};

/// O tempo mínimo entre duas atualizações dos painéis enquanto a fila trabalha.
const INTERVALO_DE_ATUALIZACAO: Duration = Duration::from_millis(700);

thread_local! {
    /// O escopo dos signals das abas: eles vivem até a thread acabar, e não só até o diálogo
    /// ou o evento que abriu a aba ser descartado.
    static ESCOPO_DAS_ABAS: Scope = Scope::detached();
}

/// A pasta em que um painel abre: a explícita (um recente, o estado salvo, a pasta do terminal),
/// depois a da conexão, depois a global; `None` é o início do servidor ou do disco. Texto vazio
/// conta como ausente. No painel remoto, `global` é `""`.
pub fn pasta_inicial(explicita: Option<String>, da_conexao: &str, global: &str) -> Option<String> {
    explicita
        .filter(|p| !p.trim().is_empty())
        .or_else(|| Some(da_conexao.trim().to_owned()).filter(|p| !p.is_empty()))
        .or_else(|| Some(global.trim().to_owned()).filter(|p| !p.is_empty()))
}

/// Uma aba da página: os dois painéis de uma conexão.
#[derive(Clone)]
pub struct Aba {
    /// O id da aba (`a1`, `a2`…), que entra nos ids dos widgets.
    pub chave: String,
    /// O título.
    pub titulo: String,
    /// A conexão da árvore (`None` numa aba que não vem de uma conexão salva).
    pub conexao: Option<u64>,
    /// A sessão de terminal de onde a aba nasceu (o SFTP usa a conexão SSH dela).
    pub sessao: Option<Session>,
    /// A pasta em que o painel remoto abre (`None`: o início do servidor).
    pub inicial: Option<String>,
    /// A pasta em que o painel local abre (`None`: o início do disco).
    pub local_inicial: Option<String>,
    /// `true` se a pasta com que o painel abriu veio de um recente, do estado salvo ou do
    /// terminal, e não de uma pasta padrão: só muda a palavra do aviso (local, remoto).
    pub pasta_lembrada: (bool, bool),
    /// O painel da esquerda.
    pub local: Painel,
    /// O painel da direita.
    pub remoto: Painel,
    /// Os números das transferências que esta aba pediu.
    pub ids: Signal<Vec<u64>>,
    /// `true` se a conexão (FTP) não é criptografada: a aba mostra um cadeado aberto. É só
    /// informação, nada pergunta.
    pub sem_tls: Signal<bool>,
    /// As ações do S3 (link, propriedades, classe, envios incompletos), numa aba S3.
    pub s3: Option<s3::AcoesS3>,
    /// As ações do rclone (espaço usado, link público), numa aba de um remoto do rclone.
    pub rclone: Option<rclone::AcoesRclone>,
    /// A comparação dos dois painéis está ligada.
    pub comparar: Signal<bool>,
    /// O critério da comparação.
    pub modo: Signal<ModoComparar>,
    /// A diferença de data, em minutos, que ainda conta como a mesma data (vem das
    /// Configurações; a aba não tem controle para ela).
    pub tolerancia_min: Signal<u32>,
    /// Esconde os idênticos enquanto a comparação está ligada.
    pub ocultar_identicos: Signal<bool>,
    /// A navegação sincronizada está ligada.
    pub sincronizar: Signal<bool>,
    /// O par de pastas (local, remota) de onde a sincronização conta o caminho relativo. Só
    /// existe enquanto `sincronizar` está ligada e os dois painéis já listaram.
    pub base: Signal<Option<(String, String)>>,
}

impl Aba {
    /// Uma aba nova. `servidor` abre o `Fs` do painel da direita.
    #[allow(clippy::too_many_arguments)]
    pub fn nova(
        chave: String,
        titulo: String,
        conexao: Option<u64>,
        sessao: Option<Session>,
        inicial: Option<String>,
        disco: &Disco,
        servidor: ArquivosOpener,
        config: &Arquivos,
    ) -> Self {
        ESCOPO_DAS_ABAS.with(|escopo| {
            escopo.run(|| {
                Self::montar(
                    chave, titulo, conexao, sessao, inicial, disco, servidor, config,
                )
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn montar(
        chave: String,
        titulo: String,
        conexao: Option<u64>,
        sessao: Option<Session>,
        inicial: Option<String>,
        disco: &Disco,
        servidor: ArquivosOpener,
        config: &Arquivos,
    ) -> Self {
        // Zero desliga o keepalive: o tempo vira longo demais para disparar.
        let parada = match config.keepalive_secs {
            0 => Duration::from_secs(u64::from(u32::MAX)),
            s => Duration::from_secs(u64::from(s)),
        };
        let local = Painel::novo(
            format!("{chave}-local"),
            Lado::Local,
            Remote::spawn_with(format!("{chave}-local"), Arc::clone(&disco.opener), parada),
            disco.endpoint.clone(),
            config.mostrar_ocultos,
        );
        let remoto = Painel::novo(
            format!("{chave}-remoto"),
            Lado::Remoto,
            Remote::spawn_with(format!("{chave}-remoto"), Arc::clone(&servidor), parada),
            Endpoint::Remote(servidor),
            config.mostrar_ocultos,
        );
        Self {
            chave,
            titulo,
            conexao,
            sessao,
            inicial,
            local_inicial: Some(config.pasta_local.clone()).filter(|p| !p.is_empty()),
            pasta_lembrada: (false, false),
            local,
            remoto,
            ids: signal(Vec::new()),
            sem_tls: signal(false),
            s3: None,
            rclone: None,
            comparar: signal(config.comparar),
            modo: signal(config.comparar_modo),
            tolerancia_min: signal(config.comparar_tolerancia_min),
            ocultar_identicos: signal(config.ocultar_identicos),
            sincronizar: signal(config.sincronizar),
            base: signal(None),
        }
    }

    /// Quantas transferências desta aba ainda não terminaram.
    pub fn ativas(&self, transferencias: &Transfers) -> usize {
        let meus = self.ids.get_untracked();
        transferencias.items().with_untracked(|itens| {
            itens
                .iter()
                .filter(|t| meus.contains(&t.id) && !t.state.is_finished())
                .count()
        })
    }

    /// Encerra as conexões da aba.
    pub fn fechar(&self) {
        self.local.remote.close();
        self.remoto.remote.close();
    }
}

/// Abre um arquivo do painel.
type AbrirArquivo = Box<dyn Fn(&Aba, Lado, &Entry)>;
/// Manda um texto ao terminal de uma sessão.
type TerminalAqui = Box<dyn Fn(Session, &str)>;
/// Guarda a pasta de uma aba nos recentes.
type Lembrar = Box<dyn Fn(&Aba, &str)>;
/// Abre o diálogo do espaço de trabalho de um assistente com a conexão da aba e a pasta dada.
type EspacoDeIa = Box<dyn Fn(&Aba, &str)>;

/// O que a view da página usa da `Tela`. Só vive nos closures da view (tem `Rc`).
pub struct Contexto {
    /// A fila de transferências, a mesma para todas as abas.
    pub transferencias: Transfers,
    /// Abre um arquivo do painel (um duplo clique num arquivo).
    pub abrir_arquivo: AbrirArquivo,
    /// Refaz a aba que não conectou (a senha pode ter sido recusada).
    pub refazer: Box<dyn Fn(&Aba)>,
    /// Manda o `cd` para o terminal da sessão e mostra os Terminais.
    pub terminal_aqui: TerminalAqui,
    /// A pasta remota mudou: guarda nos recentes.
    pub lembrar: Lembrar,
    /// As configurações: o conflito das cópias e a confirmação do apagar.
    pub config: Signal<Configuracoes>,
    /// Se a aba é de uma conexão SSH, que pode ser espaço de trabalho de um assistente (M51).
    pub aba_ssh: Box<dyn Fn(&Aba) -> bool>,
    /// "Usar como espaço de trabalho de um assistente…": abre o diálogo do espaço com a conexão
    /// da aba e a pasta dada.
    pub espaco_de_ia: EspacoDeIa,
}

/// Uma conexão que abre arquivos, com o texto do cartão pronto.
#[derive(Clone, Debug, PartialEq)]
pub struct ConexaoDeArquivos {
    /// O id na árvore.
    pub id: u64,
    /// O nome.
    pub nome: String,
    /// `usuario@host:porta · SFTP`.
    pub detalhe: String,
    /// O ícone do tipo.
    pub icone: Symbol,
}

/// As conexões que abrem arquivos.
type Conexoes = Rc<dyn Fn() -> Vec<ConexaoDeArquivos>>;

/// O que a página precisa para se montar.
pub struct Entrada {
    /// As abas abertas.
    pub abas: Signal<Vec<Aba>>,
    /// A chave da aba escolhida.
    pub escolhida: Signal<String>,
    /// O contexto dos painéis.
    pub ctx: Rc<Contexto>,
    /// As conexões que abrem arquivos.
    pub conexoes: Conexoes,
    /// Os recentes de pastas: `(alvo, título, detalhe)`.
    pub recentes: Rc<dyn Fn() -> Vec<(AlvoRecente, String, String)>>,
    /// Abre uma aba da conexão.
    pub nova: Rc<dyn Fn(u64)>,
    /// Abre um recente.
    pub abrir_recente: Rc<dyn Fn(&AlvoRecente)>,
    /// Pede para fechar a aba (pergunta antes se há transferências).
    pub fechar: Rc<dyn Fn(String)>,
    /// Cria uma conexão e vai para os Terminais.
    pub nova_conexao: Rc<dyn Fn()>,
    /// Abre a subpágina dos remotos do rclone.
    pub remotos: Rc<dyn Fn()>,
    /// Os painéis encaixáveis da página: a fila fica num painel com alfinete (M54).
    pub paineis: PaineisDaPagina,
}

/// A página: sem aba, a lista de conexões e os recentes; com abas, as abas e a fila.
pub fn pagina(entrada: Entrada) -> Column {
    let Entrada {
        abas,
        escolhida,
        ctx,
        conexoes,
        recentes,
        nova,
        abrir_recente,
        fechar,
        nova_conexao,
        remotos,
        paineis,
    } = entrada;

    let vazia = {
        let (conexoes, recentes, nova, abrir_recente, remotos) = (
            Rc::clone(&conexoes),
            Rc::clone(&recentes),
            Rc::clone(&nova),
            Rc::clone(&abrir_recente),
            Rc::clone(&remotos),
        );
        move || {
            sem_abas(
                &conexoes,
                &recentes,
                &nova,
                &abrir_recente,
                &nova_conexao,
                &remotos,
            )
        }
    };
    let com_abas = {
        let ctx = Rc::clone(&ctx);
        move || {
            com_abas(
                abas, escolhida, &ctx, &paineis, &conexoes, &nova, &fechar, &remotos,
            )
        }
    };

    Column::new()
        .id("arquivos")
        .grow(1)
        .child(Show::when(move || !abas.with(Vec::is_empty), com_abas).otherwise(vazia))
}

/// O botão "Remotos do rclone", na faixa da página sem abas e na barra da página com abas.
fn botao_dos_remotos(remotos: &Rc<dyn Fn()>) -> Button {
    let remotos = Rc::clone(remotos);
    Button::new(t!("comum-remotos-do-rclone"))
        .id("arquivos-remotos-rclone")
        .icon(Symbol::Cloud)
        .on_click(move || remotos())
}

/// A página sem abas, no padrão da Gallery do WinUI 3, como o Início: a faixa de destaque e,
/// embaixo, as conexões que abrem arquivos e as pastas recentes em grades de cartões.
fn sem_abas(
    conexoes: &Conexoes,
    recentes: &Rc<dyn Fn() -> Vec<(AlvoRecente, String, String)>>,
    nova: &Rc<dyn Fn(u64)>,
    abrir_recente: &Rc<dyn Fn(&AlvoRecente)>,
    nova_conexao: &Rc<dyn Fn()>,
    remotos: &Rc<dyn Fn()>,
) -> Column {
    let criar = {
        let nova_conexao = Rc::clone(nova_conexao);
        Button::new(t!("comum-nova-conexao"))
            .id("arquivos-nova-conexao")
            .icon(Symbol::Add)
            .accent()
            .on_click(move || nova_conexao())
    };
    let faixa = inicio::faixa(
        Text::new(t!("arquivos-sftp-ftp-ftps-s3-rclone")).id("arquivos-protocolos"),
        &t!("arquivos-inicio-titulo"),
        &t!("arquivos-inicio-descricao"),
        Column::new().child(criar).child(botao_dos_remotos(remotos)),
        Symbol::FolderOpen,
    );

    let lista = {
        let (conexoes, nova) = (Rc::clone(conexoes), Rc::clone(nova));
        For::each(
            move || conexoes(),
            |c: &ConexaoDeArquivos| format!("{}|{}|{}", c.id, c.nome, c.detalhe),
            move |c: ConexaoDeArquivos| {
                let nova = Rc::clone(&nova);
                let id = c.id;
                inicio::cartao_base(
                    format!("arquivos-conexao-{id}"),
                    c.icone,
                    &c.nome,
                    &c.detalhe,
                )
                .on_click(move || nova(id))
            },
        )
    };
    let sem_conexoes = {
        let conexoes = Rc::clone(conexoes);
        move || conexoes().is_empty()
    };
    let recentes_lista = {
        let (recentes, abrir) = (Rc::clone(recentes), Rc::clone(abrir_recente));
        For::each(
            move || {
                recentes()
                    .into_iter()
                    .enumerate()
                    .collect::<Vec<(usize, (AlvoRecente, String, String))>>()
            },
            |(i, (_, titulo, detalhe))| format!("{i}-{titulo}-{detalhe}"),
            move |(i, (alvo, titulo, detalhe))| {
                let abrir = Rc::clone(&abrir);
                inicio::cartao_base(
                    format!("arquivos-recente-{i}"),
                    Symbol::FolderOpen,
                    &titulo,
                    &detalhe,
                )
                .on_click(move || abrir(&alvo))
            },
        )
    };
    let sem_recentes = {
        let recentes = Rc::clone(recentes);
        move || recentes().is_empty()
    };

    let grade = || UniformGrid::new().min_column_width(320).gap(8);
    let conteudo = Column::new()
        .gap(24)
        .child(faixa)
        .child(
            Column::new()
                .gap(8)
                .child(inicio::secao(t!("arquivos-conexoes")))
                .child(
                    Text::new(t!("arquivos-nenhuma-conexao-ssh-sftp-ftp-s3"))
                        .id("arquivos-sem-conexoes")
                        .secondary()
                        .wrap()
                        .visible(sem_conexoes),
                )
                .child(grade().child(lista)),
        )
        .child(
            Column::new()
                .gap(8)
                .visible(move || !sem_recentes())
                .child(inicio::secao(t!("arquivos-pastas-recentes")))
                .child(grade().child(recentes_lista)),
        );
    inicio::pagina_centrada("arquivos-inicio", "arquivos-inicio-rolagem", conteudo)
}

/// A página com abas: o botão de nova aba, as abas e, embaixo, a fila.
#[allow(clippy::too_many_arguments)]
fn com_abas(
    abas: Signal<Vec<Aba>>,
    escolhida: Signal<String>,
    ctx: &Rc<Contexto>,
    paineis: &PaineisDaPagina,
    conexoes: &Conexoes,
    nova: &Rc<dyn Fn(u64)>,
    fechar: &Rc<dyn Fn(String)>,
    remotos: &Rc<dyn Fn()>,
) -> Column {
    // O menu de "Nova aba" é montado com as conexões de agora; quando a lista muda, o botão
    // é refeito.
    let botao = {
        let (conexoes, nova) = (Rc::clone(conexoes), Rc::clone(nova));
        let assinatura = {
            let conexoes = Rc::clone(&conexoes);
            move || {
                vec![
                    conexoes()
                        .iter()
                        .map(|c| format!("{}:{}", c.id, c.nome))
                        .collect::<Vec<_>>()
                        .join("|"),
                ]
            }
        };
        For::each(
            assinatura,
            |s| s.clone(),
            move |_| {
                let mut menu = MenuFlyout::new().id("arquivos-nova-aba-menu");
                for ConexaoDeArquivos { id, nome, .. } in conexoes() {
                    let nova = Rc::clone(&nova);
                    menu = menu.item(format!("arquivos-nova-aba-{id}"), nome, move || nova(id));
                }
                Button::new(t!("arquivos-nova-aba"))
                    .id("arquivos-nova-aba")
                    .icon(Symbol::Add)
                    .flyout(menu)
            },
        )
    };

    let conteudo = {
        let ctx = Rc::clone(ctx);
        move |aba: Aba| aba_view(&aba, &ctx)
    };
    let fechar = Rc::clone(fechar);
    let abas_view = TabView::bind(escolhida)
        .id("arquivos-abas")
        .items(
            abas,
            |a: &Aba| a.chave.clone(),
            |a: &Aba| a.titulo.clone(),
            conteudo,
        )
        .tab_icon(move |chave| {
            abas.with(|abas| {
                abas.iter()
                    .find(|a| a.chave == chave && a.sem_tls.get())
                    .map(|_| Symbol::LockOpen)
            })
        })
        .on_close(move |chave| fechar(chave));

    let superior = Column::new()
        .grow(1)
        .gap(4)
        .child(
            Row::new()
                .gap(4)
                .padding(Insets::new(4.0, 4.0, 4.0, 0.0))
                .child(botao)
                .child(botao_dos_remotos(remotos)),
        )
        .child(abas_view);

    let fila = {
        let ctx = Rc::clone(ctx);
        move || fila::vista(&ctx)
    };
    Column::new().grow(1).child(
        DockHost::new(superior)
            .id("arquivos-paineis")
            .layout(paineis.layout)
            .controller(&paineis.controle)
            .min_content(240.0)
            .pane(
                DockPane::new("fila", Symbol::ArrowSync, t!("casca-painel-fila"), fila)
                    .side(DockSide::Bottom)
                    .size(220.0)
                    .min_size(120.0),
            ),
    )
}

/// O conteúdo de uma aba: os dois painéis num `Split`.
fn aba_view(aba: &Aba, ctx: &Rc<Contexto>) -> Column {
    aba.local.ligar();
    aba.remoto.ligar();
    // A página muda de janela (e a aba é montada de novo): os painéis seguem onde estavam.
    for (painel, inicial) in [
        (&aba.local, &aba.local_inicial),
        (&aba.remoto, &aba.inicial),
    ] {
        if painel.caminho.with_untracked(String::is_empty) {
            painel.abrir(inicial.clone());
        } else {
            painel.atualizar();
        }
    }

    // As pastas remotas visitadas entram nos recentes.
    {
        let (aba, ctx) = (aba.clone(), Rc::clone(ctx));
        let caminho = aba.remoto.caminho;
        effect(move || {
            let caminho = caminho.get();
            if !caminho.is_empty() {
                untrack(|| (ctx.lembrar)(&aba, &caminho));
            }
        });
    }

    // Uma pasta que não existe (a padrão ou a lembrada): o painel abriu o início, e a aba diz.
    let aviso = signal(None::<String>);
    for (painel, lembrada) in [
        (&aba.local, aba.pasta_lembrada.0),
        (&aba.remoto, aba.pasta_lembrada.1),
    ] {
        let caiu_de = painel.caiu_de;
        effect(move || {
            let Some(pasta) = caiu_de.get() else {
                return;
            };
            untrack(|| {
                caiu_de.set(None);
                aviso.set(Some(if lembrada {
                    t!(
                        "arquivos-pasta-lembrada-nao-existe",
                        pasta = pasta.to_string()
                    )
                } else {
                    t!(
                        "arquivos-pasta-padrao-nao-existe",
                        pasta = pasta.to_string()
                    )
                }));
            });
        });
    }

    // Os avisos da conexão (o FTPS que passou ao TLS 1.2, o servidor sem TLS) aparecem uma vez,
    // numa `InfoBar` da aba.
    {
        let (notice, sem_tls) = (aba.remoto.remote.notice(), aba.sem_tls);
        effect(move || {
            let Some(texto) = notice.with(Clone::clone) else {
                return;
            };
            untrack(|| {
                notice.set(None);
                if texto == NOTICE_NO_TLS {
                    sem_tls.set(true);
                }
                aviso.set(Some(texto));
            });
        });
    }

    // Os painéis mostram o que a fila fez: de tempos em tempos enquanto ela trabalha, e uma
    // última vez quando acaba.
    {
        let (aba, itens) = (aba.clone(), ctx.transferencias.items());
        let estado = Cell::new((0_usize, Instant::now() - INTERVALO_DE_ATUALIZACAO, false));
        effect(move || {
            let meus = aba.ids.get();
            let (feitas, ativas) = itens.with(|itens| {
                let minhas = itens.iter().filter(|t| meus.contains(&t.id));
                let feitas = minhas.clone().filter(|t| t.state.is_finished()).count();
                (feitas, minhas.count() - feitas)
            });
            untrack(|| {
                let (antes, ultima, pendente) = estado.get();
                let pendente = pendente || feitas != antes;
                if pendente && (ativas == 0 || ultima.elapsed() >= INTERVALO_DE_ATUALIZACAO) {
                    aba.local.atualizar();
                    aba.remoto.atualizar();
                    estado.set((feitas, Instant::now(), false));
                } else {
                    estado.set((feitas, ultima, pendente));
                }
            });
        });
    }

    ligar_comparacao(aba);
    sincronia::ligar(aba, aviso);

    let divisor = Split::row(
        painel::vista(aba, Lado::Local, ctx),
        painel::vista(aba, Lado::Remoto, ctx),
    )
    .id(format!("{}-divisor", aba.chave))
    .ratio(signal(0.5))
    .min_first(260.0)
    .min_second(260.0);
    let mut coluna = Column::new().grow(1).child(
        InfoBar::new(InfoBarSeverity::Informational)
            .id(format!("{}-aviso", aba.chave))
            .message(move || aviso.get().unwrap_or_default())
            .visible(move || aviso.with(Option::is_some))
            .on_close(move || aviso.set(None)),
    );
    if let Some(acoes) = &aba.s3 {
        coluna = coluna.child(barra_s3(aba, acoes));
    }
    if let Some(acoes) = &aba.rclone {
        coluna = coluna.child(barra_rclone(aba, acoes));
    }
    coluna.child(barra_da_aba(aba)).child(divisor)
}

/// Liga a comparação: um efeito compara as entradas dos dois painéis sempre que elas, o
/// critério, a tolerância ou "Ocultar idênticos" mudam, e entrega o realce a cada painel.
/// Desligada, ou enquanto um dos painéis ainda não listou nada, os painéis ficam sem realce.
fn ligar_comparacao(aba: &Aba) {
    let (local, remoto) = (aba.local.clone(), aba.remoto.clone());
    let (ligada, modo, tolerancia, ocultar) = (
        aba.comparar,
        aba.modo,
        aba.tolerancia_min,
        aba.ocultar_identicos,
    );
    effect(move || {
        let pronta = ligada.get()
            && local.caminho.with(|c| !c.is_empty())
            && remoto.caminho.with(|c| !c.is_empty());
        if !pronta {
            local.realce.set(None);
            remoto.realce.set(None);
            return;
        }
        let tolerancia = Duration::from_secs(u64::from(tolerancia.get()) * 60);
        let ocultar = ocultar.get();
        let resultado = local.entradas.with(|l| {
            remoto
                .entradas
                .with(|r| comparar(l, r, modo.get(), tolerancia))
        });
        local
            .realce
            .set(Some(Arc::new(resultado.realce(false, ocultar))));
        remoto
            .realce
            .set(Some(Arc::new(resultado.realce(true, ocultar))));
    });
}

/// A barra da aba: comparar as pastas (e as opções da comparação) e a navegação sincronizada.
fn barra_da_aba(aba: &Aba) -> Row {
    let chave = &aba.chave;
    let menu = MenuFlyout::new()
        .id(format!("{chave}-comparar-menu"))
        .radio(
            format!("{chave}-comparar-tamanho"),
            t!("arquivos-comparar-tamanho"),
            aba.modo,
            ModoComparar::Tamanho,
        )
        .radio(
            format!("{chave}-comparar-data"),
            t!("arquivos-comparar-data"),
            aba.modo,
            ModoComparar::Data,
        )
        .toggle(
            format!("{chave}-ocultar-identicos"),
            t!("arquivos-ocultar-identicos"),
            aba.ocultar_identicos,
        );
    Row::new()
        .id(format!("{chave}-barra"))
        .gap(4)
        .padding(Insets::new(4.0, 4.0, 0.0, 4.0))
        .child(
            ToggleButton::bind(aba.comparar, t!("comum-comparar-pastas"))
                .id(format!("{chave}-comparar"))
                .icon(Symbol::SplitHorizontal),
        )
        .child(
            ToggleButton::bind(aba.sincronizar, t!("arquivos-navegacao-sincronizada"))
                .id(format!("{chave}-sincronizar"))
                .icon(Symbol::Link),
        )
        .child(
            Button::icon_only(Symbol::Filter)
                .id(format!("{chave}-comparar-opcoes"))
                .label(t!("arquivos-opcoes-da-comparacao"))
                .tooltip(t!("arquivos-opcoes-da-comparacao"))
                .flyout(menu),
        )
}

/// O que só o rclone tem na aba: o aviso das ações e o espaço usado do remoto.
fn barra_rclone(aba: &Aba, acoes: &rclone::AcoesRclone) -> Column {
    acoes.ligar();
    let chave = aba.chave.clone();
    let aviso = acoes.aviso;
    let texto = move |erro: bool| {
        aviso.with(|a| match a {
            Some((e, t)) if *e == erro => t.clone(),
            _ => String::new(),
        })
    };
    let tem = move |erro: bool| aviso.with(|a| a.as_ref().is_some_and(|(e, _)| *e == erro));
    let espaco = acoes.espaco;
    let (ok, erro) = (acoes.clone(), acoes.clone());
    Column::new()
        .child(
            InfoBar::new(InfoBarSeverity::Success)
                .id(format!("{chave}-rclone-aviso"))
                .message(move || texto(false))
                .visible(move || tem(false))
                .on_close(move || ok.aviso.set(None)),
        )
        .child(
            InfoBar::new(InfoBarSeverity::Error)
                .id(format!("{chave}-rclone-erro"))
                .message(move || texto(true))
                .visible(move || tem(true))
                .on_close(move || erro.aviso.set(None)),
        )
        .child(
            Row::new()
                .padding(Insets::new(8.0, 0.0, 4.0, 0.0))
                .visible(move || espaco.with(|e| !e.is_empty()))
                .child(
                    Text::new(move || espaco.get())
                        .id(format!("{chave}-rclone-espaco"))
                        .secondary(),
                ),
        )
}

/// O que só o S3 tem na aba: o aviso das ações, o botão dos envios incompletos e os diálogos
/// das propriedades do objeto e dos envios incompletos.
fn barra_s3(aba: &Aba, acoes: &s3::AcoesS3) -> Column {
    acoes.ligar();
    let chave = aba.chave.clone();
    let aviso = acoes.aviso;
    let texto = move |erro: bool| {
        aviso.with(|a| match a {
            Some((e, t)) if *e == erro => t.clone(),
            _ => String::new(),
        })
    };
    let tem = move |erro: bool| aviso.with(|a| a.as_ref().is_some_and(|(e, _)| *e == erro));
    let barra = {
        let (ok, erro) = (acoes.clone(), acoes.clone());
        Column::new()
            .child(
                InfoBar::new(InfoBarSeverity::Success)
                    .id(format!("{chave}-s3-aviso"))
                    .message(move || texto(false))
                    .visible(move || tem(false))
                    .on_close(move || ok.aviso.set(None)),
            )
            .child(
                InfoBar::new(InfoBarSeverity::Error)
                    .id(format!("{chave}-s3-erro"))
                    .message(move || texto(true))
                    .visible(move || tem(true))
                    .on_close(move || erro.aviso.set(None)),
            )
    };

    let botao = {
        let (acoes, remoto) = (acoes.clone(), aba.remoto.clone());
        Button::new(t!("arquivos-envios-incompletos"))
            .id(format!("{chave}-s3-pendentes"))
            .icon(Symbol::Cloud)
            .on_click(move || acoes.listar_pendentes(&remoto.caminho.get_untracked()))
    };

    let dialogo_objeto = {
        let (linhas, titulo) = (acoes.info_linhas, acoes.info_titulo);
        let id = format!("{chave}-s3-objeto");
        ContentDialog::new(
            move || t!("arquivos-propriedades-de", nome = titulo.get()),
            acoes.info_aberta,
        )
        .id(format!("{id}-dialogo"))
        .content(
            Text::new(move || {
                linhas.with(|l| {
                    l.iter()
                        .map(|(r, v)| format!("{r}: {v}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
            .id(format!("{id}-linhas"))
            .wrap(),
        )
        .close(t!("comum-fechar"))
    };

    let dialogo_pendentes = {
        let id = format!("{chave}-s3-pendentes");
        let (lista, carregando) = (acoes.pendentes, acoes.pendentes_carregando);
        let (a_item, a_todos) = (acoes.clone(), acoes.clone());
        let itens = For::each(
            move || lista.get(),
            |e| e.upload_id.clone(),
            move |envio| {
                let a = a_item.clone();
                let texto = s3::descricao(&envio);
                let id_botao = format!("{id}-abortar-{}", envio.upload_id);
                Row::new()
                    .gap(8)
                    .child(Text::new(texto).grow(1).wrap())
                    .child(
                        Button::new(t!("arquivos-abortar"))
                            .id(id_botao)
                            .on_click(move || a.abortar(envio.clone())),
                    )
            },
        );
        let id = format!("{chave}-s3-pendentes");
        ContentDialog::new(t!("arquivos-envios-incompletos-2"), acoes.pendentes_aberto)
            .id(format!("{id}-dialogo"))
            .content(
                Column::new()
                    .gap(8)
                    .child(
                        Text::new(move || {
                            if carregando.get() {
                                t!("arquivos-procurando")
                            } else if lista.with(Vec::is_empty) {
                                t!("arquivos-nenhum-envio-incompleto")
                            } else {
                                t!("arquivos-abortar-apaga-partes")
                            }
                        })
                        .id(format!("{id}-estado"))
                        .secondary()
                        .wrap(),
                    )
                    .child(itens),
            )
            .primary(t!("arquivos-abortar-todos"), move || {
                a_todos.abortar_todos()
            })
            .close(t!("comum-fechar"))
    };

    Column::new()
        .child(barra)
        .child(
            Row::new()
                .padding(Insets::new(4.0, 0.0, 4.0, 0.0))
                .child(botao),
        )
        .child(dialogo_objeto)
        .child(dialogo_pendentes)
}

/// O diálogo "Há N transferências em andamento", ao fechar uma aba.
pub fn dialogo_de_fechar(
    aberto: Signal<bool>,
    quantas: Signal<usize>,
    cancelar_e_fechar: impl Fn() + 'static,
) -> ContentDialog {
    ContentDialog::new(
        move || t!("arquivos-fechar-titulo", n = quantas.get() as i64),
        aberto,
    )
    .id("arquivos-fechar-dialogo")
    .content(Text::new(t!("arquivos-fechar-a-aba-cancela-as-transferencias")).wrap())
    .primary(
        t!("arquivos-cancelar-transferencias-e-fechar"),
        cancelar_e_fechar,
    )
    .close(t!("comum-voltar"))
}

impl Contexto {
    /// O que fazer quando o destino de uma cópia já existe, pelas configurações.
    pub fn conflito(&self) -> Conflict {
        self.config
            .with_untracked(|c| c.arquivos.conflito.conflito())
    }
}

#[cfg(test)]
mod tests {
    use super::pasta_inicial;

    fn pasta(explicita: Option<&str>, da_conexao: &str, global: &str) -> Option<String> {
        pasta_inicial(explicita.map(str::to_owned), da_conexao, global)
    }

    #[test]
    fn a_explicita_vence_a_da_conexao_que_vence_a_global() {
        assert_eq!(pasta(Some("/a"), "/b", "/c").as_deref(), Some("/a"));
        assert_eq!(pasta(None, "/b", "/c").as_deref(), Some("/b"));
        assert_eq!(pasta(None, "", "/c").as_deref(), Some("/c"));
    }

    #[test]
    fn sem_nenhuma_o_inicio_vale() {
        assert_eq!(pasta(None, "", ""), None);
    }

    #[test]
    fn texto_vazio_ou_so_de_espacos_conta_como_ausente() {
        assert_eq!(pasta(Some(""), "/b", "").as_deref(), Some("/b"));
        assert_eq!(pasta(Some("  "), "  ", "/c").as_deref(), Some("/c"));
        assert_eq!(pasta(None, "  ", "  "), None);
    }

    #[test]
    fn a_pasta_e_aparada() {
        assert_eq!(pasta(None, " /srv/www ", "").as_deref(), Some("/srv/www"));
    }
}
