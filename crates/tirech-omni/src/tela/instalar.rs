//! "Instalar no computador" (M65): o diálogo que copia o app para uma pasta fixa e põe os atalhos,
//! o início com o sistema e a entrada de "Aplicativos instalados", e o modo desinstalar. A parte
//! que mexe no disco é de [`crate::instalacao`] e roda numa thread; aqui ficam as marcas do
//! diálogo, o resultado numa `InfoBar` e a saída do app depois de desinstalar.

use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};

use tomni_ui::prelude::*;

use super::Tela;
use crate::agente::{acordar, ler_contador};
use crate::configuracoes::Configuracoes;
use crate::instalacao::{
    Contexto, EstadoDoItem, ItemDeInstalacao as Item, Motivo, Painel, PassoManual, painel_do,
    pasta_apagavel, remover_depois,
};
use crate::{reinicio, t};

/// O que cada item fez: ficou certo, ou deu o erro, e talvez um passo para a pessoa dar.
type Resultados = Vec<(Item, io::Result<Option<PassoManual>>)>;

/// O que a thread de trabalho entrega à tela.
enum Saida {
    /// O estado de cada item no disco (a leitura ao abrir e depois de aplicar).
    Situacao(Vec<(Item, EstadoDoItem)>),
    /// O fim de um "Aplicar".
    Fim(Fim),
}

/// O fim de um "Aplicar".
struct Fim {
    /// O que foi pedido (item e se é para ficar instalado).
    pedidos: Vec<(Item, bool)>,
    resultados: Resultados,
    /// O executável da cópia que acabou de ser feita ou atualizada.
    copia_nova: Option<PathBuf>,
    /// Se o app deve fechar: a remoção das pastas foi agendada e roda depois que ele sair.
    fechar: bool,
    /// Por que a remoção das pastas não foi agendada.
    erro_da_remocao: Option<String>,
}

/// O que a `InfoBar` do resultado mostra.
#[derive(Clone)]
struct Relato {
    severidade: InfoBarSeverity,
    titulo: String,
    mensagem: String,
    /// O executável da cópia, para o botão "Abrir a versão instalada".
    abrir: Option<PathBuf>,
}

/// O estado do diálogo e da thread de trabalho.
pub(super) struct Estado {
    /// Se o diálogo está aberto.
    aberto: Signal<bool>,
    /// Se a confirmação de apagar os dados está aberta.
    confirmar: Signal<bool>,
    /// Se o diálogo está no modo desinstalar.
    desinstalar: Signal<bool>,
    /// "Apagar também os meus dados".
    apagar_dados: Signal<bool>,
    /// A marca de cada item, na ordem de [`Item::TODOS`].
    marcas: [Signal<bool>; 6],
    /// "Ficar só na bandeja", embaixo de "Abrir com o sistema" (`geral.iniciar_na_bandeja`).
    na_bandeja: Signal<bool>,
    /// O estado de cada item no disco; vazio até a primeira leitura.
    situacao: Signal<Vec<(Item, EstadoDoItem)>>,
    /// Se a leitura ainda não deu as marcas iniciais do diálogo que acabou de abrir.
    marcas_pendentes: Signal<bool>,
    /// Se há um "Aplicar" em andamento.
    ocupado: Signal<bool>,
    /// O resultado do último "Aplicar".
    relato: Signal<Option<Relato>>,
    /// Sobe quando a thread termina.
    pronto: Signal<u64>,
    /// O que a thread entregou, à espera de a tela buscar.
    saida: Arc<Mutex<Vec<Saida>>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            aberto: signal(false),
            confirmar: signal(false),
            desinstalar: signal(false),
            apagar_dados: signal(false),
            marcas: std::array::from_fn(|_| signal(false)),
            na_bandeja: signal(false),
            situacao: signal(Vec::new()),
            marcas_pendentes: signal(false),
            ocupado: signal(false),
            relato: signal(None),
            pronto: signal(0),
            saida: Arc::default(),
        }
    }

    fn marca(&self, item: Item) -> Signal<bool> {
        self.marcas[indice(item)]
    }
}

/// A posição do item em [`Item::TODOS`].
fn indice(item: Item) -> usize {
    Item::TODOS
        .iter()
        .position(|i| *i == item)
        .unwrap_or_default()
}

fn estado_em(situacao: &[(Item, EstadoDoItem)], item: Item) -> EstadoDoItem {
    situacao
        .iter()
        .find(|(i, _)| *i == item)
        .map_or(EstadoDoItem::NaoSeAplica, |(_, e)| *e)
}

/// As marcas com que o diálogo abre. Cada item vem marcado se já está no disco ou se a pessoa
/// o pediu antes (as configurações lembram); a cópia sempre vem marcada, e a barra do Windows
/// também na primeira instalação, quando nada está feito. No modo desinstalar, tudo desmarcado.
fn marcas_iniciais(
    situacao: &[(Item, EstadoDoItem)],
    config: &Configuracoes,
    desinstalar: bool,
    windows: bool,
) -> [bool; 6] {
    if desinstalar {
        return [false; 6];
    }
    let nada_instalado = !situacao.iter().any(|(_, e)| e.instalado());
    std::array::from_fn(|n| {
        let item = Item::TODOS[n];
        let estado = estado_em(situacao, item);
        let disponivel = !matches!(
            estado,
            EstadoDoItem::NaoSeAplica | EstadoDoItem::Indisponivel(_)
        );
        let pedido = match item {
            Item::Copia | Item::Aplicativos => true,
            Item::AreaDeTrabalho => config.instalacao.area_de_trabalho,
            Item::Menu => config.geral.atalho_no_menu,
            Item::Barra => config.instalacao.barra_de_tarefas || (windows && nada_instalado),
            Item::Inicio => config.geral.iniciar_com_sistema,
        };
        let precisa_da_copia = estado == EstadoDoItem::Indisponivel(Motivo::PrecisaDaCopia);
        if item == Item::Aplicativos && precisa_da_copia {
            return estado_em(situacao, Item::Copia) != EstadoDoItem::NaoSeAplica;
        }
        estado.instalado() || (disponivel && pedido)
    })
}

/// O nome do item, como o diálogo e os avisos o chamam.
fn titulo(item: Item, windows: bool, macos: bool) -> String {
    match item {
        Item::Copia => t!("instalar-item-copia"),
        Item::AreaDeTrabalho => t!("instalar-item-area"),
        Item::Menu if windows => t!("instalar-item-menu-windows"),
        Item::Menu => t!("instalar-item-menu-linux"),
        Item::Barra if windows => t!("instalar-item-barra-windows"),
        Item::Barra if macos => t!("instalar-item-barra-macos"),
        Item::Barra => t!("instalar-item-barra-linux"),
        Item::Inicio => t!("instalar-item-inicio"),
        Item::Aplicativos => t!("instalar-item-aplicativos"),
    }
}

/// A id do `CheckBox` de cada item.
fn id_do_item(item: Item) -> &'static str {
    match item {
        Item::Copia => "instalar-copia",
        Item::AreaDeTrabalho => "instalar-area",
        Item::Menu => "instalar-menu",
        Item::Barra => "instalar-barra",
        Item::Inicio => "instalar-inicio",
        Item::Aplicativos => "instalar-aplicativos",
    }
}

/// O motivo por que um item que vale no sistema não dá agora.
fn motivo(m: Motivo) -> String {
    match m {
        Motivo::SemAreaDeTrabalho => t!("instalar-motivo-sem-area"),
        Motivo::PrecisaDaCopia => t!("instalar-motivo-precisa-da-copia"),
        Motivo::PainelNaoSuportado => t!("instalar-motivo-painel"),
        Motivo::SemBundle => t!("instalar-motivo-sem-bundle"),
    }
}

/// A linha de baixo do item, quando o item não tem um motivo a mostrar: o caminho que ele usa,
/// ou o que ele é.
fn descricao_fixa(item: Item, ctx: &Contexto) -> String {
    if let Some(lugar) = ctx.lugar(item) {
        return ctx.mostrar(&lugar);
    }
    match item {
        Item::Inicio => t!(
            "instalar-desc-registro",
            chave = format!("HKCU\\{}", ctx.chave_run)
        ),
        Item::Aplicativos => t!(
            "instalar-desc-registro",
            chave = format!("HKCU\\{}", ctx.chave_uninstall)
        ),
        Item::Barra if ctx.windows => t!("instalar-desc-barra-windows"),
        Item::Barra if ctx.macos => t!("instalar-desc-barra-dock"),
        Item::Barra => {
            let painel = match painel_do(&ctx.desktop_atual) {
                Some(Painel::Cinnamon) => "Cinnamon",
                Some(Painel::Gnome) => "GNOME",
                Some(Painel::Kde) => "KDE",
                None => &ctx.desktop_atual,
            };
            t!("instalar-desc-barra-painel", painel = painel)
        }
        _ => String::new(),
    }
}

/// O texto do passo que só a pessoa dá.
fn passo(p: PassoManual) -> String {
    match p {
        PassoManual::FixarNaBarraDoWindows {
            depois_de_abrir_a_copia: false,
        } => t!("instalar-passo-fixar-windows"),
        PassoManual::FixarNaBarraDoWindows {
            depois_de_abrir_a_copia: true,
        } => t!("instalar-passo-fixar-windows-depois"),
        PassoManual::DesafixarDaBarraDoWindows => t!("instalar-passo-desafixar-windows"),
        PassoManual::TirarDoDock => t!("instalar-passo-dock"),
    }
}

/// A linha de resumo da página de Configurações.
pub(super) fn resumo(situacao: &[(Item, EstadoDoItem)], ctx: &Contexto) -> String {
    if situacao.is_empty() {
        return String::new();
    }
    let copia = estado_em(situacao, Item::Copia);
    if copia.instalado()
        && let Some(pasta) = ctx.pasta_instalada()
    {
        return t!("instalar-resumo-instalado", pasta = ctx.mostrar(&pasta));
    }
    if situacao.iter().any(|(_, e)| e.instalado()) {
        t!("instalar-resumo-parcial")
    } else {
        t!("instalar-resumo-nao")
    }
}

/// As pastas de dados que "Apagar também os meus dados" apaga: a de agora e, se for outra, a
/// padrão (onde fica o `pasta.txt`). Fora as que não são seguras de apagar.
fn pastas_de_dados(tela: &Tela, ctx: &Contexto) -> Vec<PathBuf> {
    let mut pastas = vec![tela.armazenamento.pasta().to_path_buf()];
    let padrao = crate::armazenamento::pasta_padrao();
    if !pastas.contains(&padrao) {
        pastas.push(padrao);
    }
    pastas.retain(|p| pasta_apagavel(p, ctx.pessoal.as_deref()));
    pastas
}

/// O trabalho do "Aplicar", feito numa thread.
fn trabalhar(
    ctx: &Contexto,
    mut pedidos: Vec<(Item, bool)>,
    desinstalar: bool,
    dados: &[PathBuf],
) -> Fim {
    let todos = pedidos.clone();
    let mut a_apagar: Vec<PathBuf> = Vec::new();
    let tirar_a_copia = desinstalar
        && pedidos.contains(&(Item::Copia, false))
        && ctx.estado(Item::Copia).instalado();
    if desinstalar {
        // A cópia em uso vai por último e à parte: no Windows o executável rodando não se apaga.
        pedidos.retain(|(i, _)| *i != Item::Copia);
    }
    let mut resultados = ctx.aplicar_pedidos(&pedidos);
    if tirar_a_copia {
        match ctx.desinstalar_copia() {
            Ok(Some(pasta)) => a_apagar.push(pasta),
            Ok(None) => resultados.push((Item::Copia, Ok(None))),
            Err(erro) => resultados.push((Item::Copia, Err(erro))),
        }
    }
    let falhou = resultados.iter().any(|(_, r)| r.is_err());
    let copia_nova = (!desinstalar
        && pedidos.contains(&(Item::Copia, true))
        && !ctx.executando_da_copia()
        && resultados
            .iter()
            .any(|(i, r)| *i == Item::Copia && r.is_ok()))
    .then(|| ctx.exe_instalado())
    .flatten();
    let mut fim = Fim {
        pedidos: todos,
        resultados,
        copia_nova,
        fechar: false,
        erro_da_remocao: None,
    };
    if desinstalar && !falhou {
        a_apagar.extend(dados.iter().cloned());
        if !a_apagar.is_empty() {
            match remover_depois(ctx.windows, &a_apagar) {
                Ok(()) => fim.fechar = true,
                Err(erro) => fim.erro_da_remocao = Some(erro.to_string()),
            }
        }
    }
    fim
}

impl Tela {
    /// Lê o estado dos itens numa thread (a leitura da cópia confere o conteúdo do executável).
    fn ler_instalacao(&self) {
        let ctx = Contexto::de(self.ferramentas.ambiente());
        let (saida, pronto) = (Arc::clone(&self.instalar.saida), self.instalar.pronto);
        std::thread::spawn(move || {
            let situacao = ctx.situacao();
            saida
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Saida::Situacao(situacao));
            acordar(pronto);
        });
    }

    /// Abre o diálogo "Instalar no computador" (ou, com `desinstalar`, no modo desinstalar).
    pub(super) fn abrir_instalar(&self, desinstalar: bool) {
        let e = &self.instalar;
        e.relato.set(None);
        e.apagar_dados.set(false);
        e.desinstalar.set(desinstalar);
        self.marcar_instalar();
        e.marcas_pendentes.set(true);
        self.ler_instalacao();
        e.aberto.set(true);
    }

    /// Põe as marcas iniciais, pela última leitura.
    fn marcar_instalar(&self) {
        let e = &self.instalar;
        let situacao = e.situacao.get_untracked();
        if situacao.is_empty() {
            return;
        }
        let windows = self.ferramentas.ambiente().windows;
        let marcas = self.config.with_untracked(|c| {
            marcas_iniciais(&situacao, c, e.desinstalar.get_untracked(), windows)
        });
        for (sinal, valor) in e.marcas.iter().zip(marcas) {
            sinal.set(valor);
        }
        e.na_bandeja
            .set(self.config.with_untracked(|c| c.geral.iniciar_na_bandeja));
        e.marcas_pendentes.set(false);
    }

    /// Passa para o modo desinstalar: tudo desmarcado e a caixa de apagar os dados.
    fn virar_desinstalar(&self) {
        let e = &self.instalar;
        e.desinstalar.set(true);
        e.apagar_dados.set(false);
        self.marcar_instalar();
        e.aberto.set(true);
    }

    /// O "Aplicar": faz cada item marcado (ou desmarcado) à parte, numa thread.
    fn aplicar_instalar(self: &Rc<Self>) {
        let e = &self.instalar;
        if e.ocupado.get_untracked() {
            return;
        }
        let ctx = Contexto::de(self.ferramentas.ambiente());
        let situacao = e.situacao.get_untracked();
        let copia_marcada = e.marca(Item::Copia).get_untracked();
        let pedidos: Vec<(Item, bool)> = Item::TODOS
            .into_iter()
            .filter(|item| match estado_em(&situacao, *item) {
                EstadoDoItem::NaoSeAplica => false,
                EstadoDoItem::Indisponivel(Motivo::PrecisaDaCopia) => copia_marcada,
                EstadoDoItem::Indisponivel(_) => false,
                _ => true,
            })
            .map(|item| (item, e.marca(item).get_untracked()))
            .collect();
        let desinstalar = e.desinstalar.get_untracked();
        let dados = if desinstalar && e.apagar_dados.get_untracked() {
            pastas_de_dados(self, &ctx)
        } else {
            Vec::new()
        };
        e.ocupado.set(true);
        e.relato.set(None);
        let (saida, pronto) = (Arc::clone(&e.saida), e.pronto);
        std::thread::spawn(move || {
            let fim = trabalhar(&ctx, pedidos, desinstalar, &dados);
            saida
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(Saida::Fim(fim));
            acordar(pronto);
        });
    }

    /// O que chega da thread.
    fn receber_instalar(self: &Rc<Self>, saida: Saida) {
        match saida {
            Saida::Situacao(situacao) => {
                self.instalar.situacao.set(situacao);
                if self.instalar.marcas_pendentes.get_untracked() {
                    self.marcar_instalar();
                }
            }
            Saida::Fim(fim) => self.concluir_instalar(fim),
        }
    }

    /// O fim de um "Aplicar": lembra o que a pessoa pediu, mostra o resultado e, depois de
    /// desinstalar com as pastas agendadas para apagar, fecha o app.
    fn concluir_instalar(self: &Rc<Self>, fim: Fim) {
        let e = &self.instalar;
        e.ocupado.set(false);
        let ambiente = self.ferramentas.ambiente();
        let (windows, macos) = (ambiente.windows, ambiente.macos);
        let falhou = |item: Item| fim.resultados.iter().any(|(i, r)| *i == item && r.is_err());
        if !fim.fechar {
            let na_bandeja = e.na_bandeja.get_untracked();
            // As configurações guardam o que a pessoa pediu; os efeitos delas conferem o disco.
            self.config.update(|c| {
                for (item, querer) in &fim.pedidos {
                    if falhou(*item) {
                        continue;
                    }
                    match item {
                        Item::AreaDeTrabalho => c.instalacao.area_de_trabalho = *querer,
                        Item::Barra => c.instalacao.barra_de_tarefas = *querer,
                        Item::Menu => c.geral.atalho_no_menu = *querer,
                        Item::Inicio => {
                            c.geral.iniciar_com_sistema = *querer;
                            if *querer {
                                c.geral.iniciar_na_bandeja = na_bandeja;
                            }
                        }
                        Item::Copia | Item::Aplicativos => {}
                    }
                }
            });
        }
        self.ler_instalacao();

        let mut linhas: Vec<String> = Vec::new();
        for (item, resultado) in &fim.resultados {
            if let Err(erro) = resultado {
                linhas.push(t!(
                    "instalar-resultado-falha",
                    item = titulo(*item, windows, macos),
                    erro = erro.to_string()
                ));
            }
        }
        if let Some(erro) = &fim.erro_da_remocao {
            linhas.push(t!("instalar-erro-desinstalar", erro = erro.as_str()));
        }
        let com_aviso = !linhas.is_empty();
        let mut passos: Vec<PassoManual> = Vec::new();
        for (_, resultado) in &fim.resultados {
            if let Ok(Some(p)) = resultado
                && !passos.contains(p)
            {
                passos.push(*p);
            }
        }
        linhas.extend(passos.into_iter().map(passo));
        if fim.fechar {
            self.liberado.set(true);
            WindowHandle::main().close();
            return;
        }
        let nada = fim.resultados.is_empty() && linhas.is_empty();
        let (severidade, titulo_do_aviso) = if com_aviso {
            (
                InfoBarSeverity::Warning,
                t!("instalar-resultado-titulo-aviso"),
            )
        } else if nada {
            (InfoBarSeverity::Informational, t!("instalar-titulo"))
        } else {
            (InfoBarSeverity::Success, t!("instalar-resultado-titulo-ok"))
        };
        let mensagem = if nada {
            t!("instalar-resultado-nada")
        } else if linhas.is_empty() {
            t!("instalar-resultado-ok")
        } else if com_aviso {
            linhas.join("\n")
        } else {
            format!("{}\n{}", t!("instalar-resultado-ok"), linhas.join("\n"))
        };
        e.relato.set(Some(Relato {
            severidade,
            titulo: titulo_do_aviso,
            mensagem,
            abrir: fim.copia_nova.filter(|_| !com_aviso),
        }));
    }

    /// Abre a cópia instalada e fecha este app.
    fn abrir_a_copia(&self, exe: &std::path::Path) {
        match reinicio::subir(exe) {
            Ok(()) => {
                self.liberado.set(true);
                WindowHandle::main().close();
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("instalar-erro-abrir-copia", erro = erro.to_string()),
            ),
        }
    }
}

/// Liga a thread de trabalho ao resto da tela, lê o estado ao abrir e, se o app foi aberto com
/// `--desinstalar`, já mostra o diálogo no modo desinstalar.
pub(super) fn ligar(tela: &Rc<Tela>) {
    let (pronto, saida) = (tela.instalar.pronto, Arc::clone(&tela.instalar.saida));
    let t = Rc::clone(tela);
    effect(move || {
        let _ = ler_contador(pronto);
        let recebidas: Vec<Saida> =
            std::mem::take(&mut *saida.lock().unwrap_or_else(PoisonError::into_inner));
        untrack(|| {
            for s in recebidas {
                t.receber_instalar(s);
            }
        });
    });
    tela.ler_instalacao();
    if tela.ferramentas.ambiente().desinstalar_ao_abrir {
        tela.abrir_instalar(true);
    }
}

/// A linha de resumo para a página de Configurações, atualizada a cada leitura.
pub(super) fn resumo_para_configuracoes(tela: &Rc<Tela>) -> Rc<dyn Fn() -> String> {
    let situacao = tela.instalar.situacao;
    let ambiente = tela.ferramentas.ambiente();
    let ctx = Contexto::de(ambiente);
    Rc::new(move || situacao.with(|s| resumo(s, &ctx)))
}

/// A `InfoBar` com o resultado do último "Aplicar" e, enquanto ele roda, o "Aplicando...".
pub(super) fn aviso(tela: &Rc<Tela>) -> impl View {
    let ocupado = tela.instalar.ocupado;
    // O anel que gira também mantém a tela acordando: o fim do trabalho chega de outra thread, e
    // sem um widget que anime a janela só pinta de novo quando a pessoa mexe o mouse.
    let aplicando = Row::new()
        .id("instalar-aplicando")
        .gap(8)
        .align(Align::Center)
        .padding(Insets::new(8.0, 16.0, 8.0, 16.0))
        .visible(move || ocupado.get())
        .child(
            ProgressRing::new()
                .id("instalar-andamento")
                .width(16)
                .height(16),
        )
        .child(Text::new(t!("instalar-aplicando")).id("instalar-aplicando-texto"));
    Column::new().child(aplicando).child(resultado(tela))
}

/// A `InfoBar` com o resultado do último "Aplicar".
fn resultado(tela: &Rc<Tela>) -> impl View {
    let relato = tela.instalar.relato;
    let tela = Rc::clone(tela);
    Show::when(
        move || relato.with(|r| r.is_some()),
        move || {
            let r = relato.get_untracked().expect("relato");
            let mut barra = InfoBar::new(r.severidade)
                .id("instalar-aviso")
                .title(r.titulo)
                .message(r.mensagem)
                .on_close(move || relato.set(None));
            if let Some(exe) = r.abrir {
                let tela = Rc::clone(&tela);
                barra = barra.action(t!("instalar-abrir-copia"), move || {
                    tela.abrir_a_copia(&exe);
                });
            }
            barra
        },
    )
}

/// Os diálogos: o de instalar e o de confirmar a remoção dos dados. O efeito da barra liga uma
/// vez, e a função devolvida monta os dois em cada janela.
pub(super) fn dialogos(tela: &Rc<Tela>) -> impl Fn() -> Column + use<> {
    let e = &tela.instalar;
    // A barra precisa do atalho no menu: marcá-la marca o menu e trava a marca dele.
    let (barra, menu) = (e.marca(Item::Barra), e.marca(Item::Menu));
    effect(move || {
        if barra.get() && !menu.get_untracked() {
            menu.set(true);
        }
    });

    let tela = Rc::clone(tela);
    move || {
        let tela = &tela;
        let e = &tela.instalar;
        let ctx = Contexto::de(tela.ferramentas.ambiente());
        let (windows, macos) = (ctx.windows, ctx.macos);
        let (aberto, confirmar, desinstalar, apagar) =
            (e.aberto, e.confirmar, e.desinstalar, e.apagar_dados);
        let situacao = e.situacao;
        let dados = pastas_de_dados(tela, &ctx);
        let texto_dos_dados = dados
            .iter()
            .map(|p| ctx.mostrar(p))
            .collect::<Vec<_>>()
            .join(", ");
        let em_uso = ctx.executando_da_copia();

        let mut itens = Column::new().gap(10);
        for item in Item::TODOS {
            itens = itens.child(linha_do_item(tela, &ctx, item, em_uso, (windows, macos)));
        }

        let dados_visiveis = {
            let vazio = dados.is_empty();
            move || desinstalar.get() && !vazio
        };
        let caixa_dos_dados = Column::new()
            .gap(2)
            .id("instalar-dados-bloco")
            .visible(dados_visiveis)
            .child(CheckBox::bind(apagar, t!("instalar-apagar-dados")).id("instalar-dados"))
            .child(legenda(
                Text::new(t!(
                    "instalar-apagar-dados-pasta",
                    pasta = texto_dos_dados.as_str()
                ))
                .id("instalar-dados-pasta"),
            ));

        let conteudo = Column::new()
            .gap(14)
            .width(496)
            .child(
                Text::new(move || {
                    if desinstalar.get() {
                        t!("instalar-introducao-desinstalar")
                    } else {
                        t!("instalar-introducao")
                    }
                })
                .id("instalar-introducao")
                .wrap(),
            )
            .child(itens)
            .child(caixa_dos_dados);

        let (para_aplicar, para_desinstalar) = (Rc::clone(tela), Rc::clone(tela));
        let principal = ContentDialog::new(
            move || {
                if desinstalar.get() {
                    t!("instalar-titulo-desinstalar")
                } else {
                    t!("instalar-titulo")
                }
            },
            aberto,
        )
        .id("instalar-dialogo")
        .content(conteudo)
        .primary(t!("instalar-aplicar"), move || {
            if desinstalar.get_untracked() && apagar.get_untracked() {
                confirmar.set(true);
            } else {
                para_aplicar.aplicar_instalar();
            }
        })
        .secondary(t!("instalar-desinstalar"), move || {
            para_desinstalar.virar_desinstalar();
        })
        .secondary_visible(move || {
            !desinstalar.get() && situacao.with(|s| s.iter().any(|(_, e)| e.instalado()))
        })
        .close(t!("comum-cancelar"));

        let para_confirmar = Rc::clone(tela);
        let confirmacao = ContentDialog::new(t!("instalar-confirmar-titulo"), confirmar)
            .id("instalar-confirmar")
            .content(
                Text::new(t!(
                    "instalar-confirmar-texto",
                    pasta = texto_dos_dados.as_str()
                ))
                .id("instalar-confirmar-texto")
                .wrap(),
            )
            .primary(t!("instalar-confirmar-apagar"), move || {
                para_confirmar.aplicar_instalar();
            })
            .close(t!("comum-cancelar"));

        Column::new().child(principal).child(confirmacao)
    }
}

/// A linha pequena embaixo de uma caixa, alinhada com o texto dela.
fn legenda(texto: Text) -> impl View {
    Column::new()
        .padding(Insets::new(0.0, 0.0, 0.0, 28.0))
        .child(texto.secondary().caption().wrap())
}

/// Uma linha do diálogo: a caixa do item e, embaixo, o caminho ou o motivo.
fn linha_do_item(
    tela: &Rc<Tela>,
    ctx: &Contexto,
    item: Item,
    em_uso: bool,
    (windows, macos): (bool, bool),
) -> impl View {
    let e = &tela.instalar;
    let (situacao, marca, desinstalar) = (e.situacao, e.marca(item), e.desinstalar);
    let (copia, barra) = (e.marca(Item::Copia), e.marca(Item::Barra));
    let fixa = descricao_fixa(item, ctx);
    let estado = move || estado_em(&situacao.get(), item);

    let desabilitado = move || match estado() {
        EstadoDoItem::Indisponivel(Motivo::PrecisaDaCopia) => !copia.get(),
        EstadoDoItem::Indisponivel(_) => true,
        _ => match item {
            Item::Menu => barra.get(),
            Item::Copia => em_uso && !desinstalar.get(),
            _ => false,
        },
    };
    let descricao = move || match estado() {
        EstadoDoItem::Indisponivel(Motivo::PrecisaDaCopia) if !copia.get() => {
            motivo(Motivo::PrecisaDaCopia)
        }
        EstadoDoItem::Indisponivel(m) if m != Motivo::PrecisaDaCopia => motivo(m),
        _ => match item {
            Item::Menu if barra.get() => t!("instalar-motivo-barra-precisa-do-menu"),
            Item::Copia if em_uso && !desinstalar.get() => t!("instalar-desc-copia-em-uso"),
            _ => fixa.clone(),
        },
    };
    let linha = Column::new()
        .gap(0)
        .visible(move || estado() != EstadoDoItem::NaoSeAplica)
        .child(
            CheckBox::bind(marca, titulo(item, windows, macos))
                .id(id_do_item(item))
                .disabled(desabilitado),
        )
        .child(legenda(
            Text::new(descricao).id(format!("{}-descricao", id_do_item(item))),
        ));
    if item != Item::Inicio {
        return linha;
    }
    // Aberto pelo sistema, a janela abre ou o app fica só na bandeja (precisa de bandeja).
    let sem_bandeja = move || !marca.get() || desabilitado() || !tomni_ui::tray::available();
    linha.child(
        Column::new()
            .padding(Insets::new(6.0, 0.0, 0.0, 28.0))
            .child(
                CheckBox::bind(e.na_bandeja, t!("instalar-inicio-na-bandeja"))
                    .id("instalar-inicio-bandeja")
                    .disabled(sem_bandeja),
            ),
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::configuracoes::Configuracoes;

    fn situacao(estados: &[(Item, EstadoDoItem)]) -> Vec<(Item, EstadoDoItem)> {
        estados.to_vec()
    }

    #[test]
    fn nada_instalado_marca_a_copia_e_a_barra_do_windows() {
        let s = situacao(&[
            (Item::Copia, EstadoDoItem::Ausente),
            (Item::AreaDeTrabalho, EstadoDoItem::Ausente),
            (Item::Menu, EstadoDoItem::Ausente),
            (Item::Barra, EstadoDoItem::Ausente),
            (Item::Inicio, EstadoDoItem::Ausente),
            (
                Item::Aplicativos,
                EstadoDoItem::Indisponivel(Motivo::PrecisaDaCopia),
            ),
        ]);
        let mut config = Configuracoes::default();
        config.geral.atalho_no_menu = false;
        let marcas = marcas_iniciais(&s, &config, false, true);
        // Cópia, barra (Windows, primeira instalação) e aplicativos (que vai com a cópia).
        assert_eq!(marcas, [true, false, false, true, false, true]);
        // Fora do Windows a barra não vem marcada sozinha.
        let marcas = marcas_iniciais(&s, &config, false, false);
        assert_eq!(marcas, [true, false, false, false, false, true]);
    }

    #[test]
    fn o_que_ja_esta_no_disco_ou_foi_pedido_vem_marcado() {
        let s = situacao(&[
            (Item::Copia, EstadoDoItem::Presente),
            (Item::AreaDeTrabalho, EstadoDoItem::Presente),
            (Item::Menu, EstadoDoItem::Ausente),
            (Item::Barra, EstadoDoItem::Ausente),
            (Item::Inicio, EstadoDoItem::Desatualizado),
            (
                Item::Aplicativos,
                EstadoDoItem::Indisponivel(Motivo::SemAreaDeTrabalho),
            ),
        ]);
        let mut config = Configuracoes::default();
        config.geral.atalho_no_menu = true;
        let marcas = marcas_iniciais(&s, &config, false, true);
        assert_eq!(marcas, [true, true, true, false, true, false]);
    }

    #[test]
    fn desinstalar_desmarca_tudo() {
        let s = situacao(&[(Item::Copia, EstadoDoItem::Presente)]);
        let marcas = marcas_iniciais(&s, &Configuracoes::default(), true, true);
        assert_eq!(marcas, [false; 6]);
    }

    #[test]
    fn o_resumo_diz_onde_esta_instalado_ou_que_nao_esta() {
        let pessoal = std::path::PathBuf::from("/home/ana");
        let mut ctx = Contexto::de(&crate::ferramentas::Ambiente::de_teste(
            &pessoal,
            pessoal.join(".ssh"),
        ));
        ctx.estados_fixos = Vec::new();
        assert_eq!(resumo(&[], &ctx), "");
        let nao = situacao(&[(Item::Copia, EstadoDoItem::Ausente)]);
        assert_eq!(resumo(&nao, &ctx), t!("instalar-resumo-nao"));
        let parcial = situacao(&[
            (Item::Copia, EstadoDoItem::Ausente),
            (Item::Inicio, EstadoDoItem::Presente),
        ]);
        assert_eq!(resumo(&parcial, &ctx), t!("instalar-resumo-parcial"));
    }
}
