//! A página Scripts (M42): escolhe os servidores SSH e um script (um arquivo da pasta de scripts
//! ou uma linha digitada), roda em todos pelo canal `exec`, sem abrir abas, e mostra o
//! resultado de cada um. As funções puras (arquivos, estados, a rodada) estão em
//! [`crate::scripts`].

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use tomni_ui::prelude::*;
use tomni_ui::terminal::{Loopback, Scheme, Session, Terminal};
use tomni_ui::{clipboard, dialogs};

use super::{Alvo, Tela, icone_do_cartao, senha_da_credencial, terminal_configurado};
use crate::abrir::precisa_de_senha;
use crate::agente::ler_contador;
use crate::cluster;
use crate::envio::{lembrar, sugestoes};
use crate::modelo::Kind;
use crate::scripts::{
    Estado as EstadoDoServidor, Resultado, Rodada, Script, Tarefa, duracao_curta, listar,
    novo_script, pasta_de_scripts, primeira_linha, resumo, texto_dos_resultados,
};
use crate::t;

/// A chave da linha "(comando de uma linha)" da lista de scripts.
const LINHA: &str = "linha";

/// O que a página Scripts guarda enquanto o app está aberto.
pub(super) struct Estado {
    /// O filtro da lista de servidores.
    filtro: Signal<String>,
    /// Os ids (como texto) dos servidores escolhidos.
    escolhidos: Signal<BTreeSet<String>>,
    /// As linhas dos servidores (SSH) que o filtro deixa passar.
    servidores: Signal<Vec<DataGridRow>>,
    /// Os arquivos da pasta de scripts.
    arquivos: Signal<Vec<Script>>,
    /// As linhas da lista de scripts: a "linha" e os arquivos.
    lista: Signal<Vec<DataGridRow>>,
    /// O script escolhido (a chave do arquivo, ou [`LINHA`]).
    script: Signal<BTreeSet<String>>,
    /// A linha digitada.
    comando: Signal<String>,
    /// As linhas já rodadas nesta sessão do app, a mais recente primeiro.
    historico: Signal<Vec<String>>,
    /// As primeiras linhas do arquivo escolhido.
    previa: Signal<String>,
    /// A senha digitada para os servidores sem senha no cofre.
    senha: Signal<String>,
    /// Gravar essa senha nos servidores que ficaram sem.
    salvar_senha: Signal<bool>,
    /// O diálogo "Novo script" está na tela.
    novo_aberto: Signal<bool>,
    /// O nome do script novo.
    novo_nome: Signal<String>,
    /// A rodada em andamento (ou a última).
    rodada: RefCell<Option<Rodada>>,
    /// Há servidores ainda rodando.
    rodando: Signal<bool>,
    /// O contador que a rodada sobe a cada servidor que muda de estado.
    pronto: Signal<u64>,
    /// As linhas dos resultados.
    resultados: Signal<Vec<DataGridRow>>,
    /// O resultado escolhido (a chave é o id da conexão).
    resultado: Signal<BTreeSet<String>>,
    /// O que a rodada tinha na última leitura.
    ultimos: RefCell<Vec<Resultado>>,
    /// O texto de "3 de 5 terminaram — 2 ok, 1 falhou".
    resumo: Signal<String>,
    /// Onde a saída completa do servidor escolhido aparece.
    saida: Session,
    /// O que o terminal da saída mostra agora: o id e a saída.
    exibida: RefCell<Option<(u64, Vec<u8>)>>,
}

impl Estado {
    pub(super) fn novo() -> Self {
        Self {
            filtro: signal(String::new()),
            escolhidos: signal(BTreeSet::new()),
            servidores: signal(Vec::new()),
            arquivos: signal(Vec::new()),
            lista: signal(Vec::new()),
            script: signal(BTreeSet::from([LINHA.to_owned()])),
            comando: signal(String::new()),
            historico: signal(Vec::new()),
            previa: signal(String::new()),
            senha: signal(String::new()),
            salvar_senha: signal(false),
            novo_aberto: signal(false),
            novo_nome: signal(String::new()),
            rodada: RefCell::new(None),
            rodando: signal(false),
            pronto: signal(0),
            resultados: signal(Vec::new()),
            resultado: signal(BTreeSet::new()),
            ultimos: RefCell::new(Vec::new()),
            resumo: signal(String::new()),
            saida: Session::new(Loopback::new()),
            exibida: RefCell::new(None),
        }
    }
}

/// Deixa uma seleção de uma linha só: quando uma segunda entra, a nova fica e a velha sai.
fn selecao_unica(selecao: Signal<BTreeSet<String>>) {
    let anterior: RefCell<BTreeSet<String>> = RefCell::default();
    effect(move || {
        let agora = selecao.get();
        untrack(|| {
            if agora.len() > 1 {
                let nova = agora
                    .iter()
                    .find(|k| !anterior.borrow().contains(*k))
                    .or_else(|| agora.iter().next())
                    .cloned();
                let unica: BTreeSet<String> = nova.into_iter().collect();
                *anterior.borrow_mut() = unica.clone();
                selecao.set(unica);
            } else {
                *anterior.borrow_mut() = agora;
            }
        });
    });
}

/// O ícone do estado de um servidor, se há.
fn icone_do_estado(estado: &EstadoDoServidor) -> Option<Symbol> {
    match estado {
        EstadoDoServidor::Esperando => None,
        EstadoDoServidor::Rodando => Some(Symbol::Play),
        EstadoDoServidor::Ok => Some(Symbol::Checkmark),
        EstadoDoServidor::Falhou(_) | EstadoDoServidor::Erro(_) => Some(Symbol::ErrorCircle),
    }
}

/// A linha da grade de resultados de um servidor.
fn linha_do_resultado(r: &Resultado) -> DataGridRow {
    let codigo = r.codigo().map(|c| c.to_string()).unwrap_or_default();
    let duracao = r.duracao.map(duracao_curta).unwrap_or_default();
    let mut linha = DataGridRow::new(r.id.to_string())
        .cell(r.nome.clone(), CellValue::Text(r.nome.clone()))
        .cell(r.estado.texto(), CellValue::Text(r.estado.texto()))
        .cell(
            codigo,
            CellValue::Number(r.codigo().map_or(-1.0, f64::from)),
        )
        .cell(
            duracao,
            CellValue::Number(r.duracao.map_or(-1.0, |d| d.as_secs_f64())),
        );
    let saida = primeira_linha(&r.saida);
    linha = linha.cell(saida.clone(), CellValue::Text(saida));
    match icone_do_estado(&r.estado) {
        Some(icone) => linha.icon(icone),
        None => linha,
    }
}

/// O texto da saída para o terminal: `\n` vira `\r\n`, que o terminal não faz sozinho.
fn para_o_terminal(saida: &[u8]) -> Vec<u8> {
    let mut texto = Vec::with_capacity(saida.len() + 16);
    let mut anterior = 0u8;
    for &b in saida {
        if b == b'\n' && anterior != b'\r' {
            texto.push(b'\r');
        }
        texto.push(b);
        anterior = b;
    }
    texto
}

impl Tela {
    /// As conexões SSH que o item `id` da árvore sugere para o script: as do grupo, ou a própria
    /// conexão. Vazio se não for SSH.
    pub(super) fn previas_de_scripts(&self, id: u64) -> Vec<u64> {
        self.previas_do_cluster(id)
            .into_iter()
            .filter(|id| {
                self.tree
                    .with_untracked(|t| t.connection(*id).is_some_and(|c| c.kind == Kind::Ssh))
            })
            .collect()
    }

    /// Vai para a página Scripts com `previas` escolhidas.
    pub(super) fn abrir_scripts(&self, previas: &[u64]) {
        self.scripts.filtro.set(String::new());
        self.scripts
            .escolhidos
            .set(previas.iter().map(u64::to_string).collect());
        self.pagina.set("scripts".to_owned());
    }

    /// "Rodar script…" do menu Ferramentas: a seleção da árvore, se houver, vem escolhida.
    pub(super) fn abrir_scripts_da_selecao(&self) {
        let previas = self
            .id_selecionado()
            .map(|id| self.previas_de_scripts(id))
            .unwrap_or_default();
        self.abrir_scripts(&previas);
    }

    /// A pasta dos scripts de agora.
    fn pasta_dos_scripts(&self) -> std::path::PathBuf {
        self.config
            .with_untracked(|c| pasta_de_scripts(c, self.armazenamento.pasta()))
    }

    /// Lê de novo a pasta dos scripts.
    fn atualizar_scripts(&self) {
        let pasta = self.pasta_dos_scripts();
        self.scripts.arquivos.set(listar(&pasta));
    }

    /// O arquivo escolhido na lista de scripts (`None` na linha digitada, ou sem escolha).
    fn script_escolhido(&self) -> Option<Script> {
        let chave = self
            .scripts
            .script
            .with_untracked(|s| s.iter().next().cloned())?;
        self.scripts
            .arquivos
            .with_untracked(|a| a.iter().find(|s| s.nome == chave).cloned())
    }

    /// Se a escolha é a linha digitada.
    fn script_e_a_linha(&self) -> bool {
        self.scripts.script.with_untracked(|s| s.contains(LINHA))
    }

    /// Cria o script `nome`, abre no editor e atualiza a lista.
    fn criar_script(&self, nome: &str) {
        let pasta = self.pasta_dos_scripts();
        match novo_script(&pasta, nome) {
            Ok(caminho) => {
                self.atualizar_scripts();
                if let Some(chave) = caminho
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                {
                    self.scripts.script.set(BTreeSet::from([chave]));
                }
                self.abrir_no_editor(&caminho);
            }
            Err(erro) => self.avisar(
                InfoBarSeverity::Error,
                t!("scripts-nao-consegui-criar", erro = erro.to_string()),
            ),
        }
    }

    /// Os servidores escolhidos, na ordem da árvore.
    fn servidores_escolhidos(&self) -> Vec<u64> {
        let escolhidos = self.scripts.escolhidos.get_untracked();
        self.tree.with_untracked(|t| {
            cluster::linhas(t)
                .into_iter()
                .filter(|l| l.kind == Kind::Ssh && escolhidos.contains(&l.id.to_string()))
                .map(|l| l.id)
                .collect()
        })
    }

    /// Se algum dos servidores escolhidos precisa de senha e não tem uma no cofre.
    fn algum_sem_senha(&self) -> bool {
        let escolhidos = self.scripts.escolhidos.get();
        self.tree.with(|t| {
            t.connections().into_iter().any(|c| {
                escolhidos.contains(&c.id.to_string())
                    && c.kind == Kind::Ssh
                    && precisa_de_senha(c)
                    && senha_da_credencial(&self.cofre, c.credencial).is_empty()
            })
        })
    }

    /// "Rodar": monta o executor de cada servidor e começa a rodada.
    fn rodar_scripts(self: &Rc<Self>) {
        let e = &self.scripts;
        if e.rodando.get_untracked() {
            return;
        }
        let ids = self.servidores_escolhidos();
        if ids.is_empty() {
            return;
        }
        let tarefa = if self.script_e_a_linha() {
            let linha = e.comando.get_untracked();
            if linha.trim().is_empty() {
                self.avisar(InfoBarSeverity::Warning, t!("scripts-digite-o-comando"));
                return;
            }
            e.historico.update(|h| lembrar(h, linha.trim()));
            Tarefa::Linha(linha)
        } else if let Some(script) = self.script_escolhido() {
            match std::fs::read(&script.caminho) {
                Ok(conteudo) => Tarefa::Arquivo {
                    interpretador: self
                        .config
                        .with_untracked(|c| c.scripts.interpretador.clone()),
                    conteudo,
                },
                Err(erro) => {
                    self.avisar(
                        InfoBarSeverity::Error,
                        t!(
                            "comum-nao-consegui-ler",
                            arquivo = script.nome.as_str(),
                            erro = erro.to_string()
                        ),
                    );
                    return;
                }
            }
        } else {
            return;
        };
        let (comando, entrada) = tarefa.comando_e_entrada();
        let digitada = e.senha.get_untracked();
        let guardar = e.salvar_senha.get_untracked() && !digitada.is_empty();
        let mut alvos = Vec::new();
        let mut sem_senha = Vec::new();
        for id in ids {
            let Some(c) = self.tree.with_untracked(|t| t.connection(id).cloned()) else {
                continue;
            };
            let mut senhas = self.senhas_do_cofre(&c);
            if senhas.conexao.is_none() && precisa_de_senha(&c) && !digitada.is_empty() {
                senhas.conexao = Some(digitada.clone());
                sem_senha.push(id);
            }
            let efetiva = self.conexao_efetiva(&c);
            alvos.push((id, c.name.clone(), self.opener.executor(&efetiva, senhas)));
        }
        if guardar && !self.cofre.agora(|x| x.bloqueado()) {
            for id in sem_senha {
                self.guardar_senha(id, Alvo::Conexao, &digitada);
            }
        }
        let paralelo = self.config.with_untracked(|c| c.scripts.paralelo) as usize;
        let rodada = Rodada::iniciar(alvos, comando, entrada, paralelo, e.pronto);
        *e.ultimos.borrow_mut() = rodada.resultados();
        *e.rodada.borrow_mut() = Some(rodada);
        *e.exibida.borrow_mut() = None;
        e.resultado.set(BTreeSet::new());
        e.rodando.set(true);
        self.copiar_resultados();
    }

    /// "Parar": os servidores que ainda esperam não começam.
    fn parar_scripts(&self) {
        if let Some(rodada) = self.scripts.rodada.borrow().as_ref() {
            rodada.parar();
        }
    }

    /// Lê os resultados da rodada, atualiza as linhas, o resumo e a saída; avisa quando acaba.
    fn copiar_resultados(&self) {
        let e = &self.scripts;
        let (agora, terminou) = match e.rodada.borrow().as_ref() {
            Some(rodada) => (rodada.resultados(), rodada.terminou()),
            None => return,
        };
        e.resultados
            .set(agora.iter().map(linha_do_resultado).collect());
        e.resumo.set(resumo(&agora));
        *e.ultimos.borrow_mut() = agora;
        self.mostrar_saida();
        if terminou && e.rodando.get_untracked() {
            e.rodando.set(false);
            let ultimos = e.ultimos.borrow();
            let falhas = ultimos
                .iter()
                .filter(|r| r.estado != EstadoDoServidor::Ok)
                .count();
            let texto = format!("{}.", resumo(&ultimos));
            drop(ultimos);
            self.avisar(
                if falhas == 0 {
                    InfoBarSeverity::Success
                } else {
                    InfoBarSeverity::Warning
                },
                texto,
            );
        }
    }

    /// Mostra no terminal a saída do resultado escolhido (limpa se não há).
    fn mostrar_saida(&self) {
        let e = &self.scripts;
        let id = e
            .resultado
            .with_untracked(|r| r.iter().next().and_then(|k| k.parse::<u64>().ok()));
        let saida = id.and_then(|id| {
            e.ultimos
                .borrow()
                .iter()
                .find(|r| r.id == id)
                .map(|r| (id, r.saida.clone()))
        });
        if *e.exibida.borrow() == saida {
            return;
        }
        e.saida.feed(b"\x1b[2J\x1b[3J\x1b[H");
        if let Some((_, texto)) = &saida {
            e.saida.feed(&para_o_terminal(texto));
        }
        *e.exibida.borrow_mut() = saida;
    }

    /// A saída do resultado escolhido, ou a de todos se nenhum está escolhido.
    fn saida_para_copiar(&self) -> String {
        let e = &self.scripts;
        let id = e
            .resultado
            .with_untracked(|r| r.iter().next().and_then(|k| k.parse::<u64>().ok()));
        let ultimos = e.ultimos.borrow();
        match id {
            Some(id) => ultimos
                .iter()
                .filter(|r| r.id == id)
                .map(|r| String::from_utf8_lossy(&r.saida).into_owned())
                .collect(),
            None => texto_dos_resultados(&ultimos),
        }
    }
}

/// A página Scripts, em Ferramentas.
pub(super) fn pagina(tela: &Rc<Tela>) -> impl View + use<> {
    let e = &tela.scripts;
    let (filtro, escolhidos, servidores) = (e.filtro, e.escolhidos, e.servidores);
    let (arquivos, lista, script) = (e.arquivos, e.lista, e.script);
    let (comando, historico, previa) = (e.comando, e.historico, e.previa);
    let (senha, salvar_senha) = (e.senha, e.salvar_senha);
    let (rodando, pronto, resultados, resultado, resumo_sinal) =
        (e.rodando, e.pronto, e.resultados, e.resultado, e.resumo);

    selecao_unica(script);
    selecao_unica(resultado);

    // A lista de arquivos nasce com a página.
    tela.atualizar_scripts();

    // Os servidores acompanham a árvore e o filtro.
    {
        let arvore = tela.tree;
        effect(move || {
            let busca = filtro.get();
            let novas = arvore.with(|t| {
                cluster::linhas(t)
                    .into_iter()
                    .filter(|l| l.kind == Kind::Ssh && l.combina(&busca))
                    .map(|l| {
                        DataGridRow::new(l.id.to_string())
                            .cell(l.nome.clone(), CellValue::Text(l.nome))
                            .cell(l.grupo.clone(), CellValue::Text(l.grupo))
                            .icon(icone_do_cartao(l.kind))
                    })
                    .collect::<Vec<_>>()
            });
            servidores.set(novas);
        });
    }

    // A lista de scripts: a linha digitada e os arquivos.
    effect(move || {
        let mut linhas = vec![
            DataGridRow::new(LINHA)
                .cell(
                    t!("scripts-comando-de-uma-linha"),
                    CellValue::Text(String::new()),
                )
                .icon(Symbol::Code),
        ];
        arquivos.with(|a| {
            linhas.extend(a.iter().map(|s| {
                DataGridRow::new(s.nome.clone())
                    .cell(s.nome.clone(), CellValue::Text(s.nome.clone()))
                    .icon(Symbol::Document)
            }));
        });
        lista.set(linhas);
    });

    // A prévia do arquivo escolhido.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let _ = script.get();
            let _ = arquivos.get();
            untrack(|| {
                let texto = t
                    .script_escolhido()
                    .and_then(|s| std::fs::read_to_string(&s.caminho).ok())
                    .map(|conteudo| {
                        let mut linhas: Vec<&str> = conteudo.lines().take(13).collect();
                        let mais = linhas.len() > 12;
                        linhas.truncate(12);
                        let mut texto = linhas.join("\n");
                        if mais {
                            texto.push_str("\n…");
                        }
                        texto
                    })
                    .unwrap_or_default();
                previa.set(texto);
            });
        });
    }

    // A rodada acorda a tela a cada servidor que muda de estado.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let _ = ler_contador(pronto);
            untrack(|| t.copiar_resultados());
        });
    }
    // Trocar a linha escolhida troca a saída.
    {
        let t = Rc::clone(tela);
        effect(move || {
            let _ = resultado.get();
            untrack(|| t.mostrar_saida());
        });
    }

    let filtro_campo = Edit::bind(filtro)
        .id("scripts-filtro")
        .placeholder(t!("comum-filtrar"));
    let grade_servidores = DataGrid::bind(servidores, escolhidos)
        .id("scripts-servidores")
        .grow(3)
        .columns([
            DataGridColumn::new("nome", t!("comum-nome")).width(ColumnWidth::Star(1.0)),
            DataGridColumn::new("grupo", t!("comum-grupo")).width(ColumnWidth::Star(0.8)),
        ])
        .empty_text(t!("scripts-nenhuma-conexao-ssh"));
    let grade_scripts = DataGrid::bind(lista, script)
        .id("scripts-lista")
        .grow(2)
        .columns([DataGridColumn::new("nome", t!("scripts-script")).width(ColumnWidth::Star(1.0))]);

    let botoes_de_arquivo = {
        let (novo, editar, pasta, atualizar) = (
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
            Rc::clone(tela),
        );
        Row::new()
            .gap(8)
            .child(
                Button::new(t!("scripts-novo"))
                    .id("scripts-novo")
                    .icon(Symbol::Add)
                    .on_click(move || {
                        novo.scripts.novo_nome.set(String::new());
                        novo.scripts.novo_aberto.set(true);
                    }),
            )
            .child(
                Button::new(t!("comum-editar"))
                    .id("scripts-editar")
                    .icon(Symbol::Edit)
                    .disabled({
                        let t = Rc::clone(tela);
                        move || {
                            let _ = script.get();
                            t.script_escolhido().is_none()
                        }
                    })
                    .on_click(move || {
                        if let Some(s) = editar.script_escolhido() {
                            editar.abrir_no_editor(&s.caminho);
                        }
                    }),
            )
            .child(
                Button::new(t!("comum-pasta"))
                    .id("scripts-abrir-pasta")
                    .icon(Symbol::FolderOpen)
                    .on_click(move || {
                        let destino = pasta.pasta_dos_scripts();
                        let _ = std::fs::create_dir_all(&destino);
                        if let Err(erro) = crate::ferramentas::abrir_pasta(&destino) {
                            pasta.avisar(
                                InfoBarSeverity::Error,
                                t!(
                                    "chaves-nao-consegui-abrir",
                                    caminho = destino.display().to_string(),
                                    motivo = erro.to_string()
                                ),
                            );
                        }
                    }),
            )
            .child(
                Button::new("")
                    .id("scripts-atualizar")
                    .icon(Symbol::ArrowSync)
                    .tooltip(t!("scripts-ler-a-pasta-de-novo"))
                    .on_click(move || atualizar.atualizar_scripts()),
            )
    };

    let esquerda = Column::new()
        .width(360)
        .gap(8)
        .child(Text::new(t!("scripts-servidores")).subtitle())
        .child(filtro_campo)
        .child(grade_servidores)
        .child(Text::new(t!("scripts-script")).subtitle())
        .child(grade_scripts)
        .child(botoes_de_arquivo);

    let e_a_linha = move || script.with(|s| s.contains(LINHA));
    let campo_da_linha = Show::when(e_a_linha, move || {
        AutoSuggestBox::bind(comando, move |digitado| {
            historico.with_untracked(|h| sugestoes(h, digitado))
        })
        .id("scripts-comando")
        .placeholder(t!("scripts-comando-para-os-servidores-escolhidos"))
    });
    let previa_do_arquivo = Show::when(
        move || !script.with(|s| s.contains(LINHA)) && !script.with(BTreeSet::is_empty),
        move || {
            Text::new(move || previa.get())
                .id("scripts-previa")
                .caption()
                .wrap()
        },
    );
    let campo_da_senha = {
        let t = Rc::clone(tela);
        Show::when(
            move || t.algum_sem_senha(),
            move || {
                Column::new()
                    .gap(4)
                    .child(
                        Edit::bind(senha)
                            .id("scripts-senha")
                            .password()
                            .placeholder(t!("scripts-senha-para-os-servidores-sem-senha")),
                    )
                    .child(
                        CheckBox::bind(salvar_senha, t!("scripts-salvar-no-cofre"))
                            .id("scripts-salvar-senha"),
                    )
            },
        )
    };

    let rodar = {
        let t = Rc::clone(tela);
        let pode = {
            let t = Rc::clone(tela);
            move || {
                let _ = escolhidos.get();
                let sem_servidor = escolhidos.with(BTreeSet::is_empty)
                    || t.tree.with(|arv| {
                        cluster::linhas(arv).iter().all(|l| {
                            l.kind != Kind::Ssh
                                || !escolhidos.with(|e| e.contains(&l.id.to_string()))
                        })
                    });
                sem_servidor || rodando.get() || script.with(BTreeSet::is_empty)
            }
        };
        Button::new(t!("scripts-rodar"))
            .id("scripts-rodar")
            .icon(Symbol::Play)
            .accent()
            .disabled(pode)
            .on_click(move || t.rodar_scripts())
    };
    let parar = {
        let t = Rc::clone(tela);
        Button::new(t!("comum-parar"))
            .id("scripts-parar")
            .icon(Symbol::Stop)
            .visible(move || rodando.get())
            .on_click(move || t.parar_scripts())
    };

    let grade_resultados = DataGrid::bind(resultados, resultado)
        .id("scripts-resultados")
        .grow(2)
        .columns([
            DataGridColumn::new("servidor", t!("scripts-servidor"))
                .width(ColumnWidth::Pixels(110.0)),
            DataGridColumn::new("estado", t!("comum-estado")).width(ColumnWidth::Pixels(96.0)),
            DataGridColumn::new("codigo", t!("scripts-codigo")).width(ColumnWidth::Pixels(72.0)),
            DataGridColumn::new("duracao", t!("scripts-duracao")).width(ColumnWidth::Pixels(80.0)),
            DataGridColumn::new("saida", t!("scripts-saida")).width(ColumnWidth::Star(1.0)),
        ])
        .empty_text(t!("scripts-escolha-os-servidores-e-o-script"));

    let terminal = {
        let t = Rc::clone(tela);
        terminal_configurado(t.config, None)(
            Terminal::session(t.scripts.saida)
                .id("scripts-saida")
                .grow(3)
                .scheme(Scheme::Theme),
        )
    };

    let acoes_de_saida = {
        let (copiar, salvar) = (Rc::clone(tela), Rc::clone(tela));
        Row::new()
            .gap(8)
            .child(
                Button::new(t!("scripts-copiar-a-saida"))
                    .id("scripts-copiar-saida")
                    .icon(Symbol::Copy)
                    .on_click(move || clipboard::write(copiar.saida_para_copiar())),
            )
            .child(
                Button::new(t!("scripts-salvar-os-resultados"))
                    .id("scripts-salvar-saida")
                    .icon(Symbol::Save)
                    .on_click(move || {
                        let t = Rc::clone(&salvar);
                        dialogs::save_file()
                            .remember("scripts-resultados")
                            .title(t!("scripts-salvar-os-resultados-2"))
                            .filter(t!("comum-texto"), &["txt"])
                            .file_name("resultados.txt")
                            .show(move |caminho| {
                                let Some(caminho) = caminho else {
                                    return;
                                };
                                let texto = texto_dos_resultados(&t.scripts.ultimos.borrow());
                                match std::fs::write(&caminho, texto) {
                                    Ok(()) => t.avisar(
                                        InfoBarSeverity::Success,
                                        t!(
                                            "scripts-resultados-gravados-em",
                                            arquivo = caminho.display().to_string()
                                        ),
                                    ),
                                    Err(erro) => t.avisar(
                                        InfoBarSeverity::Error,
                                        t!(
                                            "comum-nao-consegui-gravar",
                                            arquivo = caminho.display().to_string(),
                                            erro = erro.to_string()
                                        ),
                                    ),
                                }
                            });
                    }),
            )
    };

    let direita = Column::new()
        .grow(1)
        .gap(8)
        .child(campo_da_linha)
        .child(previa_do_arquivo)
        .child(campo_da_senha)
        .child(Row::new().gap(8).child(rodar).child(parar))
        .child(Text::new(move || resumo_sinal.get()).id("scripts-resumo"))
        .child(grade_resultados)
        .child(terminal)
        .child(acoes_de_saida);

    Column::new()
        .id("scripts-pagina")
        .grow(1)
        .gap(12)
        .padding(16)
        .child(
            Text::new(t!("scripts-scripts"))
                .title()
                .id("scripts-titulo"),
        )
        .child(
            Text::new(t!("scripts-rode-um-script-em-varios-servidores"))
                .caption()
                .wrap(),
        )
        .child(Row::new().grow(1).gap(16).child(esquerda).child(direita))
}

/// O diálogo "Novo script" (pede só o nome).
pub(super) fn dialogo_novo(tela: &Rc<Tela>) -> impl View + use<> {
    let (aberto, nome) = (tela.scripts.novo_aberto, tela.scripts.novo_nome);
    let t = Rc::clone(tela);
    ContentDialog::new(t!("scripts-novo-script"), aberto)
        .id("scripts-novo-dialogo")
        .content(
            Column::new()
                .gap(8)
                .child(Text::new(t!("scripts-o-arquivo-nasce-na-pasta-dos")).wrap())
                .child(
                    Edit::bind(nome)
                        .id("scripts-novo-nome")
                        .placeholder(t!("scripts-nome-do-script")),
                ),
        )
        .primary(t!("comum-criar"), move || {
            let nome = nome.get_untracked();
            if !nome.trim().is_empty() {
                t.criar_script(&nome);
            }
        })
        .close(t!("comum-cancelar"))
}
