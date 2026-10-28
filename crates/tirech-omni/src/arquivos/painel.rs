//! Um painel de arquivos: uma conexão (a [`Remote`]), a pasta aberta e o `DataGrid`.
//!
//! O [`Painel`] só guarda signals e a `Remote`, que são `Send + Sync` e cabem num
//! `Signal<Vec<Aba>>`. Os pedidos vão para a thread da conexão e a resposta volta por
//! [`Reply`]; os efeitos de [`Painel::ligar`] aplicam a resposta quando ela chega. Quem sabe da
//! fila, do outro painel e dos diálogos é a view (em `mod.rs`), que passa isso aos botões.

use std::collections::BTreeSet;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use tomni_ui::clipboard;
use tomni_ui::files::{Endpoint, Entry, EntryKind, Remote, RemoteState, Reply, path};
use tomni_ui::prelude::*;

use super::comparar::{Marca, Realce};
use super::s3::{self, AcoesS3};
use super::sincronia::Origem;
use super::{Aba, Contexto, formato};
use crate::configuracoes::ValidadeLink;
use crate::t;

/// De que lado da aba o painel fica.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lado {
    /// O disco desta máquina (à esquerda).
    Local,
    /// O servidor da aba (à direita).
    Remoto,
}

impl Lado {
    /// O pedaço do id dos widgets.
    pub fn chave(self) -> &'static str {
        match self {
            Lado::Local => "local",
            Lado::Remoto => "remoto",
        }
    }
}

/// O que fazer com a seleção depois que uma pasta carrega.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Depois {
    /// Manter o que continua na pasta.
    #[default]
    Nada,
    /// Escolher esta entrada (a pasta que acabou de ser criada, por exemplo).
    Selecionar(String),
}

/// A pasta que foi pedida e ainda não chegou.
#[derive(Clone)]
struct Listagem {
    resposta: Reply<(String, Vec<Entry>)>,
    depois: Depois,
    /// A pasta de [`Painel::abrir`], para dizer se ela não existia e o painel caiu no início.
    pedida: Option<String>,
    /// Quem pediu: o usuário ou a navegação sincronizada.
    origem: Origem,
    /// A pasta pedida (vazia quando é o início), para o aviso de quando o espelho falha.
    destino: String,
}

/// Um pedido de mudança (criar, renomear, mudar permissão) que ainda não voltou.
#[derive(Clone)]
struct Acao {
    resposta: Reply<()>,
    depois: Depois,
}

/// Um painel: o lado, a conexão, a pasta aberta e o que o `DataGrid` mostra.
#[derive(Clone)]
pub struct Painel {
    /// O id do `DataGrid` (`a1-local`, `a1-remoto`).
    pub id: String,
    /// O lado.
    pub lado: Lado,
    /// A conexão do painel.
    pub remote: Remote,
    /// A ponta que a fila usa para falar com este painel.
    pub endpoint: Endpoint,
    /// A pasta aberta (vazia até a primeira listagem chegar).
    pub caminho: Signal<String>,
    /// O texto do campo de caminho.
    pub campo: Signal<String>,
    /// As entradas da pasta, como o servidor mandou.
    pub entradas: Signal<Vec<Entry>>,
    /// As linhas do `DataGrid`.
    pub linhas: Signal<Vec<DataGridRow>>,
    /// As linhas escolhidas.
    pub selecao: Signal<BTreeSet<String>>,
    /// A ordem do `DataGrid`.
    pub ordem: Signal<Option<(String, SortDirection)>>,
    /// O filtro.
    pub filtro: Signal<String>,
    /// A linha que está sendo renomeada.
    pub renomeando: Signal<Option<String>>,
    /// Mostra os arquivos que começam com ponto.
    pub ocultos: Signal<bool>,
    /// O último erro do painel (a `InfoBar` mostra).
    pub erro: Signal<Option<String>>,
    /// Uma pasta está sendo carregada.
    pub carregando: Signal<bool>,
    /// A pasta que o [`Painel::abrir`] pediu e não existia: o painel abriu o início no lugar.
    pub caiu_de: Signal<Option<String>>,
    /// A comparação com o outro painel: as cores das linhas e o que fica escondido (`None`:
    /// desligada).
    pub realce: Signal<Option<Arc<Realce>>>,
    /// Cada vez que o usuário muda de pasta neste painel: o número (que sobe a cada mudança,
    /// para o efeito que escuta disparar mesmo com o mesmo destino) e a pasta nova. A
    /// navegação sincronizada escuta isto; `0` é "ainda não navegou".
    pub navegou: Signal<(u64, String)>,
    /// A pasta que a navegação sincronizada pediu a este painel e não abriu.
    pub espelho_falhou: Signal<Option<String>>,
    listagem: Signal<Option<Listagem>>,
    acoes: Signal<Vec<Acao>>,
}

impl Painel {
    /// Um painel sem pasta; [`Painel::abrir`] pede a primeira.
    pub fn novo(
        id: impl Into<String>,
        lado: Lado,
        remote: Remote,
        endpoint: Endpoint,
        ocultos: bool,
    ) -> Self {
        Self {
            id: id.into(),
            lado,
            remote,
            endpoint,
            caminho: signal(String::new()),
            campo: signal(String::new()),
            entradas: signal(Vec::new()),
            linhas: signal(Vec::new()),
            selecao: signal(BTreeSet::new()),
            ordem: signal(Some(("nome".to_owned(), SortDirection::Ascending))),
            filtro: signal(String::new()),
            renomeando: signal(None),
            ocultos: signal(ocultos),
            erro: signal(None),
            carregando: signal(false),
            caiu_de: signal(None),
            realce: signal(None),
            navegou: signal((0, String::new())),
            espelho_falhou: signal(None),
            listagem: signal(None),
            acoes: signal(Vec::new()),
        }
    }

    /// Liga os efeitos do painel. Chame dentro da view da aba: os efeitos morrem com ela.
    pub fn ligar(&self) {
        let p = self.clone();
        effect(move || {
            let Some(listagem) = p.listagem.get() else {
                return;
            };
            let Some(resposta) = listagem.resposta.get() else {
                return;
            };
            untrack(|| {
                p.listagem.set(None);
                p.carregando.set(false);
                match resposta {
                    Ok((destino, entradas)) => {
                        if let Some(pedida) = listagem.pedida.filter(|pedida| *pedida != destino) {
                            p.caiu_de.set(Some(pedida));
                        }
                        p.chegou(destino, entradas, listagem.depois, listagem.origem);
                    }
                    Err(_) if listagem.origem == Origem::Espelho => {
                        // Quem avisa é a aba, que desliga a sincronização.
                        p.campo.set(p.caminho.get_untracked());
                        p.espelho_falhou.set(Some(listagem.destino));
                    }
                    Err(mensagem) => {
                        p.erro.set(Some(mensagem));
                        p.campo.set(p.caminho.get_untracked());
                    }
                }
            });
        });

        let p = self.clone();
        effect(move || {
            let realce = p.realce.get();
            let linhas = linhas_de(
                &p.entradas.get(),
                &p.caminho.get(),
                p.ocultos.get(),
                realce.as_deref(),
            );
            // O que a comparação esconde sai da seleção, para um Apagar ou Copiar depois não
            // pegar o que não se vê.
            if let Some(realce) = &realce
                && !realce.ocultar.is_empty()
                && p.selecao
                    .with_untracked(|s| s.iter().any(|n| realce.ocultar.contains(n)))
            {
                p.selecao
                    .update(|s| s.retain(|n| !realce.ocultar.contains(n)));
            }
            p.linhas.set(linhas);
        });

        let p = self.clone();
        effect(move || {
            let acoes = p.acoes.get();
            let prontas: Vec<(Acao, Result<(), String>)> = acoes
                .iter()
                .filter_map(|a| a.resposta.get().map(|r| (a.clone(), r)))
                .collect();
            if prontas.is_empty() {
                return;
            }
            untrack(|| {
                p.acoes
                    .update(|v| v.retain(|a| a.resposta.get_untracked().is_none()));
                for (acao, resposta) in prontas {
                    match resposta {
                        Ok(()) => p.recarregar(acao.depois),
                        Err(mensagem) => p.erro.set(Some(mensagem)),
                    }
                }
            });
        });
    }

    /// A pasta chegou: troca as entradas e acerta a seleção. Uma mudança de pasta pedida pelo
    /// usuário avisa a navegação sincronizada por [`Painel::navegou`].
    fn chegou(&self, destino: String, entradas: Vec<Entry>, depois: Depois, origem: Origem) {
        let mudou = self.caminho.get_untracked() != destino;
        self.erro.set(None);
        self.caminho.set(destino.clone());
        self.campo.set(destino);
        match depois {
            Depois::Selecionar(nome) => self.selecao.set(BTreeSet::from([nome])),
            Depois::Nada if mudou => self.selecao.set(BTreeSet::new()),
            Depois::Nada => {
                self.selecao
                    .update(|s| s.retain(|n| entradas.iter().any(|e| &e.name == n)));
            }
        }
        if mudou {
            self.filtro.set(String::new());
        }
        self.entradas.set(entradas);
        if mudou && origem == Origem::Usuario {
            let n = self.navegou.with_untracked(|(n, _)| n + 1);
            self.navegou.set((n, self.caminho.get_untracked()));
        }
    }

    /// Abre a primeira pasta: `inicial` se der, senão a pasta inicial da conexão (e então
    /// [`Painel::caiu_de`] guarda a que não abriu).
    pub fn abrir(&self, inicial: Option<String>) {
        self.carregando.set(true);
        let pedida = inicial.clone();
        let resposta = self.remote.run(move |fs| {
            if let Some(pasta) = &inicial
                && let Ok(lista) = fs.list(pasta)
            {
                return Ok((pasta.clone(), lista));
            }
            let casa = fs.home()?;
            let lista = fs.list(&casa)?;
            Ok((casa, lista))
        });
        self.listagem.set(Some(Listagem {
            resposta,
            depois: Depois::Nada,
            pedida,
            origem: Origem::Usuario,
            destino: String::new(),
        }));
    }

    /// Vai para a pasta inicial da conexão.
    pub fn inicio(&self) {
        self.abrir(None);
    }

    /// Vai para `destino`. Um erro (a pasta não existe, sem permissão) fica em [`Painel::erro`].
    pub fn ir(&self, destino: &str) {
        self.pedir(destino, Depois::Nada);
    }

    /// Vai para `destino` a pedido da navegação sincronizada: a chegada não espelha de volta, e
    /// se a pasta não abrir o erro vai para [`Painel::espelho_falhou`], não para [`Painel::erro`].
    pub fn espelhar(&self, destino: &str) {
        self.pedir_como(destino, Depois::Nada, Origem::Espelho);
    }

    fn pedir(&self, destino: &str, depois: Depois) {
        self.pedir_como(destino, depois, Origem::Usuario);
    }

    fn pedir_como(&self, destino: &str, depois: Depois, origem: Origem) {
        let texto = destino.trim();
        if texto == "~" || texto.is_empty() {
            self.inicio();
            return;
        }
        let destino = path::normalize(texto);
        self.carregando.set(true);
        let pasta = destino.clone();
        let resposta = self.remote.run(move |fs| {
            let lista = fs.list(&destino)?;
            Ok((destino.clone(), lista))
        });
        self.listagem.set(Some(Listagem {
            resposta,
            depois,
            pedida: None,
            origem,
            destino: pasta,
        }));
    }

    /// Lê a pasta de novo.
    pub fn atualizar(&self) {
        self.recarregar(Depois::Nada);
    }

    fn recarregar(&self, depois: Depois) {
        let caminho = self.caminho.get_untracked();
        if caminho.is_empty() {
            self.inicio();
        } else {
            self.pedir(&caminho, depois);
        }
    }

    /// Sobe uma pasta.
    pub fn acima(&self) {
        let caminho = self.caminho.get_untracked();
        let pai = path::parent(&caminho);
        if pai != caminho {
            self.ir(&pai);
        }
    }

    /// O caminho de `nome` dentro da pasta aberta.
    pub fn caminho_de(&self, nome: &str) -> String {
        path::join(&self.caminho.get_untracked(), nome)
    }

    /// A entrada da pasta aberta com esse nome.
    pub fn entrada(&self, nome: &str) -> Option<Entry> {
        self.entradas
            .with_untracked(|e| e.iter().find(|x| x.name == nome).cloned())
    }

    /// Os nomes escolhidos (sem o `..`), na ordem do alfabeto.
    pub fn escolhidos(&self) -> Vec<String> {
        self.selecao
            .with_untracked(|s| s.iter().filter(|n| *n != "..").cloned().collect())
    }

    /// As entradas escolhidas.
    pub fn entradas_escolhidas(&self) -> Vec<Entry> {
        self.escolhidos()
            .iter()
            .filter_map(|n| self.entrada(n))
            .collect()
    }

    /// Acompanha uma mudança que já foi pedida: o erro vira o aviso do painel e o sucesso lê a
    /// pasta de novo.
    pub fn fazer(&self, resposta: Reply<()>, depois: Depois) {
        self.acoes.update(|v| v.push(Acao { resposta, depois }));
    }

    /// Cria a pasta `nome` na pasta aberta e a escolhe.
    pub fn nova_pasta(&self, nome: &str) {
        let nome = nome.trim().to_owned();
        if nome_invalido(&nome) {
            self.erro.set(Some(t!(
                "arquivos-nome-de-pasta-invalido",
                nome = nome.as_str()
            )));
            return;
        }
        let resposta = self.remote.mkdir(&self.caminho_de(&nome));
        self.fazer(resposta, Depois::Selecionar(nome));
    }

    /// Troca o nome de `antigo` para `novo` (sem sobrescrever).
    pub fn renomear(&self, antigo: &str, novo: &str) {
        self.renomeando.set(None);
        let novo = novo.trim().to_owned();
        if novo == antigo {
            return;
        }
        if nome_invalido(&novo) {
            self.erro
                .set(Some(t!("arquivos-nome-invalido", nome = novo.as_str())));
            return;
        }
        let resposta = self
            .remote
            .rename(&self.caminho_de(antigo), &self.caminho_de(&novo), false);
        self.fazer(resposta, Depois::Selecionar(novo));
    }

    /// Move `nome` para dentro da pasta `destino` (um caminho inteiro).
    pub fn mover(&self, nome: &str, destino: &str) {
        let de = self.caminho_de(nome);
        let para = path::join(destino, nome);
        if de == para || destino == de {
            return;
        }
        let resposta = self.remote.rename(&de, &para, false);
        self.fazer(resposta, Depois::Nada);
    }

    /// Muda as permissões de `nome`; com `recursivo`, as das pastas que há dentro também.
    pub fn permissoes(&self, nome: &str, modo: u32, recursivo: bool) {
        let caminho = self.caminho_de(nome);
        let resposta = if recursivo {
            self.remote
                .run(move |fs| permissoes_recursivas(fs, &caminho, modo))
        } else {
            self.remote.set_permissions(&caminho, modo)
        };
        self.fazer(resposta, Depois::Nada);
    }

    /// Entra na pasta `nome`; num link, descobre antes se ele aponta para uma pasta.
    pub fn entrar(&self, nome: &str) {
        if nome == ".." {
            self.acima();
            return;
        }
        let Some(entrada) = self.entrada(nome) else {
            return;
        };
        match entrada.kind {
            EntryKind::Dir => self.ir(&self.caminho_de(nome)),
            EntryKind::Symlink => {
                let alvo = self.caminho_de(nome);
                let nao_e_pasta = t!("arquivos-nao-e-pasta", alvo = alvo.as_str());
                self.carregando.set(true);
                let resposta = self.remote.run(move |fs| {
                    if !fs.stat(&alvo)?.is_dir() {
                        return Err(io::Error::new(
                            io::ErrorKind::NotADirectory,
                            nao_e_pasta.clone(),
                        ));
                    }
                    let lista = fs.list(&alvo)?;
                    Ok((alvo.clone(), lista))
                });
                self.listagem.set(Some(Listagem {
                    resposta,
                    depois: Depois::Nada,
                    pedida: None,
                    origem: Origem::Usuario,
                    destino: self.caminho_de(nome),
                }));
            }
            _ => {}
        }
    }
}

/// Um nome que não serve para criar ou renomear.
fn nome_invalido(nome: &str) -> bool {
    nome.is_empty() || nome == "." || nome == ".." || nome.contains('/') || nome.contains('\0')
}

/// Aplica `modo` a `caminho` e às pastas dentro dele (links não são seguidos).
fn permissoes_recursivas(
    fs: &mut dyn tomni_ui::files::Fs,
    caminho: &str,
    modo: u32,
) -> std::io::Result<()> {
    fs.set_permissions(caminho, modo)?;
    let Ok(lista) = fs.list(caminho) else {
        return Ok(());
    };
    for entrada in lista.into_iter().filter(Entry::is_dir) {
        permissoes_recursivas(fs, &path::join(caminho, &entrada.name), modo)?;
    }
    Ok(())
}

/// As linhas do `DataGrid` de uma pasta: o `..` (fora da raiz), as pastas e os arquivos. Com
/// `realce`, cada linha marcada ganha a cor da comparação e as idênticas, se for o caso, somem;
/// o `..` nunca muda.
pub fn linhas_de(
    entradas: &[Entry],
    caminho: &str,
    ocultos: bool,
    realce: Option<&Realce>,
) -> Vec<DataGridRow> {
    let mut linhas = Vec::with_capacity(entradas.len() + 1);
    if !caminho.is_empty() && path::parent(caminho) != caminho {
        linhas.push(
            DataGridRow::new("..")
                .cell("..", CellValue::Text("..".to_owned()))
                .cell("", CellValue::Number(0.0))
                .cell("", CellValue::Time(None))
                .cell("", CellValue::Text(String::new()))
                .icon(Symbol::FolderOpen)
                .rank(0)
                .drop_target(),
        );
    }
    for e in entradas {
        if !ocultos && e.name.starts_with('.') {
            continue;
        }
        if realce.is_some_and(|r| r.ocultar.contains(&e.name)) {
            continue;
        }
        let marca = realce.and_then(|r| r.marcas.get(&e.name)).copied();
        linhas.push(linha_de(e, marca));
    }
    linhas
}

fn linha_de(e: &Entry, marca: Option<Marca>) -> DataGridRow {
    let pasta = e.is_dir();
    let tamanho = if pasta {
        String::new()
    } else {
        formato::tamanho(e.size)
    };
    let modo = e
        .permissions
        .map(|m| {
            format!(
                "{}{}",
                formato::letra_do_tipo(e.kind),
                formato::permissoes(m)
            )
        })
        .unwrap_or_default();
    let mut linha = DataGridRow::new(e.name.clone())
        .cell(e.name.clone(), CellValue::Text(e.name.clone()))
        .cell(
            tamanho,
            CellValue::Number(if pasta { 0.0 } else { e.size as f64 }),
        )
        .cell(formato::data(e.modified), CellValue::Time(e.modified))
        .cell(modo.clone(), CellValue::Text(modo))
        .icon(match e.kind {
            EntryKind::Dir => Symbol::Folder,
            EntryKind::Symlink => Symbol::Link,
            _ => Symbol::Document,
        })
        .rank(if pasta { 1 } else { 2 });
    if pasta {
        linha = linha.drop_target();
    }
    if let Some(marca) = marca {
        linha = linha.tone(marca.tom());
    }
    linha
}

/// Enfileira a cópia de `nome`, do painel `de` para a pasta `pasta` do painel `para`, e anota
/// o número da transferência na aba (para o aviso de fechar e para atualizar os painéis).
pub fn enfileirar(
    ctx: &Contexto,
    ids: Signal<Vec<u64>>,
    de: &Painel,
    nome: &str,
    para: &Painel,
    pasta: &str,
) {
    // O tamanho que a listagem já deu põe a estimativa na tela desde a primeira leitura; numa
    // pasta ou num link a fila conta sozinha.
    let tamanho = de.entradas.with_untracked(|v| {
        v.iter()
            .find(|e| e.name == nome && e.kind == EntryKind::File)
            .map(|e| e.size)
    });
    let id = ctx.transferencias.add_sized(
        (de.endpoint.clone(), de.caminho_de(nome)),
        (para.endpoint.clone(), path::join(pasta, nome)),
        ctx.conflito(),
        tamanho,
    );
    ids.update(|v| v.push(id));
}

/// Os signals dos diálogos de um painel.
#[derive(Clone, Copy)]
struct Dialogos {
    nova_aberta: Signal<bool>,
    nova_nome: Signal<String>,
    apagar_aberto: Signal<bool>,
    apagar_nomes: Signal<Vec<String>>,
    prop_aberta: Signal<bool>,
    prop_nome: Signal<String>,
    prop_octal: Signal<String>,
    prop_bits: [Signal<bool>; 9],
    prop_especial: Signal<u32>,
    prop_recursivo: Signal<bool>,
    prop_dono: Signal<String>,
}

impl Dialogos {
    fn novos() -> Self {
        Self {
            nova_aberta: signal(false),
            nova_nome: signal(String::new()),
            apagar_aberto: signal(false),
            apagar_nomes: signal(Vec::new()),
            prop_aberta: signal(false),
            prop_nome: signal(String::new()),
            prop_octal: signal(String::new()),
            prop_bits: std::array::from_fn(|_| signal(false)),
            prop_especial: signal(0),
            prop_recursivo: signal(false),
            prop_dono: signal(String::new()),
        }
    }
}

/// O texto de "Apagar N itens?".
fn titulo_de_apagar(n: usize) -> String {
    t!("arquivos-apagar-titulo", n = n as i64)
}

/// A view de um painel: a barra, o aviso de erro, o `DataGrid` e os diálogos.
pub fn vista(aba: &Aba, lado: Lado, ctx: &Rc<Contexto>) -> Column {
    let (p, outro) = match lado {
        Lado::Local => (aba.local.clone(), aba.remoto.clone()),
        Lado::Remoto => (aba.remoto.clone(), aba.local.clone()),
    };
    let id = p.id.clone();
    let ids = aba.ids;
    let d = Dialogos::novos();
    let com_sessao = lado == Lado::Remoto && aba.sessao.is_some();
    let grupo = format!("arq-{}", aba.chave);

    // Os bits rwx e o campo octal andam juntos; cada efeito só escreve se o valor mudou.
    effect(move || {
        let mut modo = d.prop_especial.get();
        for (i, bit) in d.prop_bits.iter().enumerate() {
            if bit.get() {
                modo |= 1 << (8 - i);
            }
        }
        let texto = formato::octal(modo);
        untrack(|| {
            if d.prop_octal.get_untracked() != texto {
                d.prop_octal.set(texto);
            }
        });
    });
    effect(move || {
        let Some(modo) = formato::de_octal(&d.prop_octal.get()) else {
            return;
        };
        untrack(|| {
            for (i, bit) in d.prop_bits.iter().enumerate() {
                let quer = modo & (1 << (8 - i)) != 0;
                if bit.get_untracked() != quer {
                    bit.set(quer);
                }
            }
            if d.prop_especial.get_untracked() != modo & 0o7000 {
                d.prop_especial.set(modo & 0o7000);
            }
        });
    });

    // Enfileira o apagar de cada nome (o diálogo de confirmação e o `confirmar_apagar`
    // desligado chegam aqui).
    let apagar_agora: Rc<dyn Fn(Vec<String>)> = {
        let (p, ctx) = (p.clone(), Rc::clone(ctx));
        Rc::new(move |nomes| {
            for nome in nomes {
                let id = ctx
                    .transferencias
                    .delete(p.endpoint.clone(), &p.caminho_de(&nome));
                ids.update(|v| v.push(id));
            }
            p.selecao.set(BTreeSet::new());
        })
    };
    let pedir_apagar: Rc<dyn Fn(Vec<String>)> = {
        let (ctx, apagar_agora) = (Rc::clone(ctx), Rc::clone(&apagar_agora));
        Rc::new(move |nomes| {
            let nomes: Vec<String> = nomes.into_iter().filter(|n| n != "..").collect();
            if nomes.is_empty() {
                return;
            }
            if !ctx.config.with_untracked(|c| c.arquivos.confirmar_apagar) {
                apagar_agora(nomes);
                return;
            }
            d.apagar_nomes.set(nomes);
            d.apagar_aberto.set(true);
        })
    };
    let abrir_propriedades: Rc<dyn Fn(&str)> = {
        let p = p.clone();
        Rc::new(move |nome| {
            let Some(e) = p.entrada(nome) else { return };
            let modo = e.permissions.unwrap_or(0);
            d.prop_nome.set(nome.to_owned());
            d.prop_especial.set(modo & 0o7000);
            for (i, bit) in d.prop_bits.iter().enumerate() {
                bit.set(modo & (1 << (8 - i)) != 0);
            }
            d.prop_octal.set(formato::octal(modo));
            d.prop_recursivo.set(false);
            d.prop_dono.set(t!(
                "painel-dono-e-grupo",
                dono = e.owner.as_deref().unwrap_or("—"),
                grupo = e.group.as_deref().unwrap_or("—")
            ));
            d.prop_aberta.set(true);
        })
    };
    let transferir: Rc<dyn Fn()> = {
        let (p, outro, ctx) = (p.clone(), outro.clone(), Rc::clone(ctx));
        Rc::new(move || {
            let pasta = outro.caminho.get_untracked();
            if pasta.is_empty() {
                return;
            }
            for nome in p.escolhidos() {
                enfileirar(&ctx, ids, &p, &nome, &outro, &pasta);
            }
        })
    };
    let invocar: Rc<dyn Fn(&str)> = {
        let (p, aba, ctx) = (p.clone(), aba.clone(), Rc::clone(ctx));
        Rc::new(move |nome| match p.entrada(nome) {
            Some(e) if !e.is_dir() && e.kind != EntryKind::Symlink => {
                (ctx.abrir_arquivo)(&aba, lado, &e);
            }
            _ => p.entrar(nome),
        })
    };

    let barra = Row::new()
        .gap(2)
        .child(
            Button::icon_only(Symbol::ChevronUp)
                .id(format!("{id}-acima"))
                .label(t!("painel-acima"))
                .on_click({
                    let p = p.clone();
                    move || p.acima()
                }),
        )
        .child(
            Button::icon_only(Symbol::Home)
                .id(format!("{id}-inicio"))
                .label(t!("painel-inicio"))
                .on_click({
                    let p = p.clone();
                    move || p.inicio()
                }),
        )
        .child(
            Button::icon_only(Symbol::ArrowSync)
                .id(format!("{id}-atualizar"))
                .label(t!("comum-atualizar"))
                .on_click({
                    let p = p.clone();
                    move || p.atualizar()
                }),
        )
        .child(
            Button::icon_only(Symbol::FolderAdd)
                .id(format!("{id}-nova-pasta"))
                .label(t!("comum-nova-pasta"))
                .on_click(move || {
                    d.nova_nome.set(String::new());
                    d.nova_aberta.set(true);
                }),
        )
        .child(
            AutoSuggestBox::bind(p.campo, |_| Vec::new())
                .id(format!("{id}-caminho"))
                .placeholder(t!("painel-caminho"))
                .query_icon(Symbol::ArrowRight)
                .grow(1)
                .on_submit({
                    let p = p.clone();
                    move |texto: String| p.ir(&texto)
                }),
        )
        .child(
            Edit::bind(p.filtro)
                .id(format!("{id}-filtro"))
                .placeholder(t!("comum-filtrar"))
                .width(130),
        )
        .child(
            ToggleButton::icon_only(p.ocultos, Symbol::Eye)
                .id(format!("{id}-ocultos"))
                .label(t!("painel-mostrar-ocultos")),
        );

    let aviso_de_erro = {
        let erro = p.erro;
        InfoBar::new(InfoBarSeverity::Error)
            .id(format!("{id}-erro"))
            .message(move || erro.get().unwrap_or_default())
            .visible(move || erro.get().is_some())
            .on_close(move || erro.set(None))
    };
    let reconectar = {
        let estado = p.remote.state();
        let (p, aba, ctx) = (p.clone(), aba.clone(), Rc::clone(ctx));
        let falhou = {
            let estado = estado.clone();
            move || matches!(estado.get(), RemoteState::Failed(_))
        };
        Button::new(t!("comum-tentar-de-novo"))
            .id(format!("{id}-reconectar"))
            .icon(Symbol::ArrowSync)
            .visible(falhou)
            .on_click(move || {
                if lado == Lado::Remoto && aba.conexao.is_some() {
                    (ctx.refazer)(&aba);
                } else {
                    p.remote.reconnect();
                    p.recarregar(Depois::Nada);
                }
            })
    };

    let grade = {
        let (p1, p2, p3, p4, p5) = (p.clone(), p.clone(), p.clone(), p.clone(), p.clone());
        let (outro_drop, ctx_drop, ctx_files) = (outro.clone(), Rc::clone(ctx), Rc::clone(ctx));
        let (id_menu, ctx_menu, aba_menu) = (id.clone(), Rc::clone(ctx), aba.clone());
        let (pedir_tecla, pedir_menu) = (Rc::clone(&pedir_apagar), Rc::clone(&pedir_apagar));
        let (transferir_tecla, transferir_menu) = (Rc::clone(&transferir), Rc::clone(&transferir));
        let (invocar_grade, invocar_menu) = (Rc::clone(&invocar), Rc::clone(&invocar));
        let propriedades_menu = Rc::clone(&abrir_propriedades);
        let id_drop = id.clone();
        DataGrid::bind(p.linhas, p.selecao)
            .id(id.clone())
            .grow(1)
            .columns([
                DataGridColumn::new("nome", t!("comum-nome")).width(ColumnWidth::Star(1.0)),
                DataGridColumn::new("tamanho", t!("comum-tamanho"))
                    .width(ColumnWidth::Pixels(90.0))
                    .align_end(),
                DataGridColumn::new("modificado", t!("painel-modificado"))
                    .width(ColumnWidth::Pixels(130.0)),
                DataGridColumn::new("permissoes", t!("painel-permissoes"))
                    .width(ColumnWidth::Pixels(100.0))
                    .not_sortable(),
            ])
            .sort(p.ordem)
            .filter(p.filtro)
            .renaming(p.renomeando)
            .empty_text(t!("painel-pasta-vazia"))
            .drag_group(grupo)
            .on_invoke(move |chave| invocar_grade(&chave))
            .on_rename(move |chave, novo| p1.renomear(&chave, &novo))
            .on_delete(move |chaves| pedir_tecla(chaves))
            .on_key(move |tecla, _| match tecla {
                Key::F(5) => {
                    transferir_tecla();
                    true
                }
                Key::F(7) => {
                    d.nova_nome.set(String::new());
                    d.nova_aberta.set(true);
                    true
                }
                Key::Backspace => {
                    p2.acima();
                    true
                }
                _ => false,
            })
            .can_drop(move |soltar| soltar.source_grid != id_drop || soltar.target.is_some())
            .on_drop(move |soltar| {
                let destino = match &soltar.target {
                    Some(alvo) if alvo == ".." => path::parent(&p3.caminho.get_untracked()),
                    Some(alvo) => p3.caminho_de(alvo),
                    None => p3.caminho.get_untracked(),
                };
                if soltar.source_grid == p3.id {
                    for nome in soltar.keys.iter().filter(|n| *n != "..") {
                        p3.mover(nome, &destino);
                    }
                } else if soltar.source_grid == outro_drop.id {
                    for nome in soltar.keys.iter().filter(|n| *n != "..") {
                        enfileirar(&ctx_drop, ids, &outro_drop, nome, &p3, &destino);
                    }
                }
            })
            .on_files_dropped(move |caminhos, alvo| {
                if lado != Lado::Remoto {
                    return;
                }
                let destino = match alvo {
                    Some(alvo) if alvo == ".." => path::parent(&p4.caminho.get_untracked()),
                    Some(alvo) => p4.caminho_de(&alvo),
                    None => p4.caminho.get_untracked(),
                };
                for caminho in caminhos {
                    let texto = caminho.to_string_lossy().into_owned();
                    let nome = path::file_name(&texto).to_owned();
                    let tamanho = std::fs::metadata(&texto)
                        .ok()
                        .filter(|m| m.is_file())
                        .map(|m| m.len());
                    let ids_novo = ctx_files.transferencias.add_sized(
                        (Endpoint::Local, texto),
                        (p4.endpoint.clone(), path::join(&destino, &nome)),
                        ctx_files.conflito(),
                        tamanho,
                    );
                    ids.update(|v| v.push(ids_novo));
                }
            })
            .item_context_flyout(move |chave| {
                menu_da_linha(
                    &id_menu,
                    chave,
                    lado,
                    com_sessao,
                    &p5,
                    &aba_menu,
                    &ctx_menu,
                    &invocar_menu,
                    &transferir_menu,
                    &pedir_menu,
                    &propriedades_menu,
                )
            })
    };

    let dialogo_nova_pasta = {
        let p = p.clone();
        ContentDialog::new(t!("comum-nova-pasta"), d.nova_aberta)
            .id(format!("{id}-nova-pasta-dialogo"))
            .content(
                Edit::bind(d.nova_nome)
                    .id(format!("{id}-nova-pasta-nome"))
                    .placeholder(t!("painel-nome-da-pasta")),
            )
            .primary(t!("comum-criar"), move || {
                p.nova_pasta(&d.nova_nome.get_untracked())
            })
            .close(t!("comum-cancelar"))
    };

    let dialogo_apagar = {
        let (apagar_agora, id_d) = (Rc::clone(&apagar_agora), id.clone());
        ContentDialog::new(
            move || titulo_de_apagar(d.apagar_nomes.with(Vec::len)),
            d.apagar_aberto,
        )
        .id(format!("{id_d}-apagar-dialogo"))
        .content(
            Text::new(move || {
                let nomes = d.apagar_nomes.get();
                let mut linhas: Vec<String> = nomes.iter().take(5).cloned().collect();
                if nomes.len() > 5 {
                    linhas.push(t!("arquivos-e-mais", n = (nomes.len() - 5) as i64));
                }
                linhas.join("\n")
            })
            .id(format!("{id_d}-apagar-lista"))
            .wrap(),
        )
        .primary(t!("comum-apagar"), move || {
            apagar_agora(d.apagar_nomes.get_untracked())
        })
        .close(t!("comum-cancelar"))
    };

    let dialogo_propriedades = {
        let (p, id_d) = (p.clone(), id.clone());
        let caixa = move |i: usize, rotulo: &'static str| {
            CheckBox::bind(d.prop_bits[i], rotulo).id(format!("{id_d}-prop-bit-{i}"))
        };
        let linha_de_bits = move |titulo: &'static str, primeiro: usize| {
            Row::new()
                .gap(12)
                .child(Text::new(titulo).width(60))
                .child(caixa(primeiro, "Ler"))
                .child(caixa(primeiro + 1, "Escrever"))
                .child(caixa(primeiro + 2, "Executar"))
        };
        let id_c = id.clone();
        ContentDialog::new(
            move || t!("arquivos-propriedades-de", nome = d.prop_nome.get()),
            d.prop_aberta,
        )
        .id(format!("{id}-prop-dialogo"))
        .content(
            Column::new()
                .gap(8)
                .child(
                    Text::new(move || d.prop_dono.get())
                        .id(format!("{id_c}-prop-dono"))
                        .secondary(),
                )
                .child(linha_de_bits("Dono", 0))
                .child(linha_de_bits("Grupo", 3))
                .child(linha_de_bits("Outros", 6))
                .child(
                    Edit::bind(d.prop_octal)
                        .id(format!("{id_c}-prop-octal"))
                        .placeholder(t!("painel-octal-755"))
                        .width(100),
                )
                .child(
                    CheckBox::bind(d.prop_recursivo, t!("painel-aplicar-as-subpastas"))
                        .id(format!("{id_c}-prop-recursivo")),
                ),
        )
        .primary(t!("painel-aplicar"), move || {
            let Some(modo) = formato::de_octal(&d.prop_octal.get_untracked()) else {
                p.erro.set(Some(t!("arquivos-octal-invalido")));
                return;
            };
            p.permissoes(
                &d.prop_nome.get_untracked(),
                modo,
                d.prop_recursivo.get_untracked(),
            );
        })
        .close(t!("comum-cancelar"))
    };

    Column::new()
        .grow(1)
        .gap(4)
        .child(barra)
        .child(aviso_de_erro)
        .child(reconectar)
        .child(grade)
        .child(dialogo_nova_pasta)
        .child(dialogo_apagar)
        .child(dialogo_propriedades)
}

/// O menu de contexto de uma linha. Os ids levam a chave da linha, senão repetiriam entre
/// linhas. As ações valem para a seleção na hora em que são acionadas.
#[allow(clippy::too_many_arguments)]
fn menu_da_linha(
    id: &str,
    chave: &str,
    lado: Lado,
    com_sessao: bool,
    p: &Painel,
    aba: &Aba,
    ctx: &Rc<Contexto>,
    invocar: &Rc<dyn Fn(&str)>,
    transferir: &Rc<dyn Fn()>,
    pedir_apagar: &Rc<dyn Fn(Vec<String>)>,
    propriedades: &Rc<dyn Fn(&str)>,
) -> MenuFlyout {
    let prefixo = format!("{id}-{chave}");
    let abrir = {
        let (invocar, chave) = (Rc::clone(invocar), chave.to_owned());
        move || invocar(&chave)
    };
    let menu = MenuFlyout::new()
        .id(format!("{prefixo}-menu"))
        .item(format!("{prefixo}-abrir"), t!("comum-abrir"), abrir)
        .icon(Symbol::FolderOpen);
    if chave == ".." {
        return menu;
    }
    let transferir = Rc::clone(transferir);
    let renomear = {
        let (p, chave) = (p.clone(), chave.to_owned());
        move || p.renomeando.set(Some(chave.clone()))
    };
    let apagar = {
        let (p, pedir) = (p.clone(), Rc::clone(pedir_apagar));
        move || pedir(p.escolhidos())
    };
    let abrir_propriedades = {
        let (propriedades, chave) = (Rc::clone(propriedades), chave.to_owned());
        move || propriedades(&chave)
    };
    let copiar = {
        let (p, chave) = (p.clone(), chave.to_owned());
        move || clipboard::write(p.caminho_de(&chave))
    };
    let terminal = {
        let (p, aba, ctx) = (p.clone(), aba.clone(), Rc::clone(ctx));
        move || {
            if let Some(sessao) = aba.sessao {
                (ctx.terminal_aqui)(sessao, &p.caminho.get_untracked());
            }
        }
    };
    let permissoes = p.remote.caps().is_none_or(|c| c.permissions);
    let mut menu = menu
        .item(
            format!("{prefixo}-transferir"),
            if lado == Lado::Remoto {
                "Baixar"
            } else {
                "Enviar"
            },
            move || transferir(),
        )
        .icon(if lado == Lado::Remoto {
            Symbol::ArrowDownload
        } else {
            Symbol::ArrowUpload
        })
        .separator()
        .item(
            format!("{prefixo}-renomear"),
            t!("comum-renomear"),
            renomear,
        )
        .icon(Symbol::Rename)
        .item(format!("{prefixo}-apagar"), t!("comum-apagar"), apagar)
        .icon(Symbol::Delete)
        .separator();
    if permissoes {
        menu = menu
            .item(
                format!("{prefixo}-propriedades"),
                t!("arquivos-propriedades"),
                abrir_propriedades,
            )
            .icon(Symbol::Settings);
    }
    menu = menu
        .item(
            format!("{prefixo}-copiar-caminho"),
            t!("arquivos-copiar-caminho"),
            copiar,
        )
        .icon(Symbol::Copy);
    if lado == Lado::Remoto
        && let Some(acoes) = &aba.s3
        && p.entrada(chave).is_some_and(|e| e.kind == EntryKind::File)
    {
        let padrao = ctx.config.with_untracked(|c| c.s3.validade_link);
        menu = menu_s3(menu, &prefixo, acoes, p.caminho_de(chave), padrao);
    }
    // O link público só aparece se o serviço faz (o `features()` do remoto).
    if lado == Lado::Remoto
        && let Some(acoes) = &aba.rclone
        && acoes.link_publico.get_untracked()
        && p.entrada(chave).is_some_and(|e| e.kind == EntryKind::File)
    {
        let (acoes, caminho) = (acoes.clone(), p.caminho_de(chave));
        menu = menu
            .item(
                format!("{prefixo}-link-publico"),
                t!("arquivos-copiar-link-publico"),
                move || acoes.copiar_link(&caminho),
            )
            .icon(Symbol::Link);
    }
    if com_sessao {
        menu = menu
            .item(
                format!("{prefixo}-terminal"),
                t!("arquivos-abrir-terminal-aqui"),
                terminal,
            )
            .icon(Symbol::WindowConsole);
    }
    if lado == Lado::Remoto
        && ctx.config.with_untracked(|c| c.geral.mcp)
        && (ctx.aba_ssh)(aba)
        && p.entrada(chave).is_some_and(|e| e.kind == EntryKind::Dir)
    {
        let (p, aba, ctx, chave) = (p.clone(), aba.clone(), Rc::clone(ctx), chave.to_owned());
        menu = menu
            .item(
                format!("{prefixo}-mcp-espaco"),
                t!("mcp-espaco-do-painel"),
                move || (ctx.espaco_de_ia)(&aba, &p.caminho_de(&chave)),
            )
            .icon(Symbol::Sparkle);
    }
    menu
}

/// Os itens que só o S3 tem no menu de um arquivo: link pré-assinado, classe de armazenamento
/// e propriedades do objeto.
fn menu_s3(
    menu: MenuFlyout,
    prefixo: &str,
    acoes: &AcoesS3,
    caminho: String,
    padrao: ValidadeLink,
) -> MenuFlyout {
    let mut links = MenuFlyout::new().id(format!("{prefixo}-link-menu"));
    for (i, (rotulo, validade)) in s3::validades().into_iter().enumerate() {
        let (acoes, caminho) = (acoes.clone(), caminho.clone());
        links = links.item(format!("{prefixo}-link-{i}"), rotulo, move || {
            acoes.copiar_link(&caminho, validade)
        });
    }
    let mut classes = MenuFlyout::new().id(format!("{prefixo}-classe-menu"));
    for classe in s3::classes() {
        let (acoes, caminho) = (acoes.clone(), caminho.clone());
        classes = classes.item(format!("{prefixo}-classe-{classe}"), classe, move || {
            acoes.trocar_classe(&caminho, classe)
        });
    }
    let link_padrao = {
        let (acoes, caminho) = (acoes.clone(), caminho.clone());
        move || acoes.copiar_link(&caminho, Duration::from_secs(padrao.segundos()))
    };
    let objeto = {
        let acoes = acoes.clone();
        move || acoes.propriedades(&caminho)
    };
    menu.item(
        format!("{prefixo}-link-padrao"),
        t!("arquivos-copiar-link-padrao", validade = padrao.nome()),
        link_padrao,
    )
    .icon(Symbol::Link)
    .submenu(
        format!("{prefixo}-link"),
        t!("arquivos-copiar-link-outra-validade"),
        links,
    )
    .icon(Symbol::Link)
    .submenu(
        format!("{prefixo}-classe"),
        t!("casca-classe-de-armazenamento"),
        classes,
    )
    .icon(Symbol::Cloud)
    .item(
        format!("{prefixo}-objeto"),
        t!("arquivos-propriedades-do-objeto"),
        objeto,
    )
    .icon(Symbol::Info)
    .separator()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pasta(nome: &str) -> Entry {
        Entry::new(nome, EntryKind::Dir)
    }

    fn arquivo(nome: &str, tamanho: u64) -> Entry {
        let mut e = Entry::new(nome, EntryKind::File);
        e.size = tamanho;
        e
    }

    #[test]
    fn a_raiz_nao_tem_linha_de_subir() {
        let linhas = linhas_de(&[pasta("etc")], "/", false, None);
        assert_eq!(linhas.len(), 1);
        assert_eq!(linhas[0].key, "etc");
    }

    #[test]
    fn fora_da_raiz_o_primeiro_e_o_dois_pontos() {
        let linhas = linhas_de(&[arquivo("a.txt", 1)], "/home/ana", false, None);
        assert_eq!(linhas[0].key, "..");
        assert_eq!(linhas[1].key, "a.txt");
    }

    #[test]
    fn os_ocultos_so_aparecem_se_pedidos() {
        let entradas = [arquivo(".bashrc", 3), arquivo("leia.md", 5)];
        assert_eq!(linhas_de(&entradas, "/", false, None).len(), 1);
        assert_eq!(linhas_de(&entradas, "/", true, None).len(), 2);
    }

    #[test]
    fn pasta_vem_com_alvo_de_soltar_e_arquivo_com_tamanho() {
        let linhas = linhas_de(&[pasta("docs"), arquivo("x.bin", 1536)], "/", false, None);
        assert!(linhas[0].drop_target);
        assert!(!linhas[1].drop_target);
        assert_eq!(linhas[1].cells[1].0, "1,5 KB");
    }

    #[test]
    fn o_realce_pinta_esconde_e_poupa_o_dois_pontos() {
        let entradas = [arquivo("novo", 1), arquivo("igual", 2), arquivo("dif", 3)];
        let realce = Realce {
            marcas: [
                ("novo".to_owned(), Marca::SoAqui),
                ("dif".to_owned(), Marca::TamanhoDiferente),
                // Um `..` marcado por engano não pega: ele não vem das entradas.
                ("..".to_owned(), Marca::SoAqui),
            ]
            .into(),
            ocultar: ["igual".to_owned(), "..".to_owned()].into(),
        };
        let linhas = linhas_de(&entradas, "/home/ana", false, Some(&realce));
        let chaves: Vec<&str> = linhas.iter().map(|l| l.key.as_str()).collect();
        assert_eq!(chaves, ["..", "novo", "dif"]);
        assert_eq!(linhas[0].tone, None);
        assert_eq!(linhas[1].tone, Some(RowTone::Caution));
        assert_eq!(linhas[2].tone, Some(RowTone::Critical));
    }

    #[test]
    fn nomes_que_nao_servem() {
        for nome in ["", ".", "..", "a/b"] {
            assert!(nome_invalido(nome), "{nome:?}");
        }
        assert!(!nome_invalido("relatório final.txt"));
    }
}
