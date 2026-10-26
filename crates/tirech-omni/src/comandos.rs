//! Os comandos ao conectar (M50): a herança por pasta, o motor que decide quando cada passo sai
//! e o autocompletar do campo do comando. É dado puro, sem widget, sem signal e sem relógio do
//! sistema: o motor recebe o que aconteceu e a hora, e diz o que fazer. A casca que liga isso
//! às sessões fica em `tela/comandos.rs`.

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use crate::modelo::{
    AcaoDoPasso, ComandosAoConectar, Connection, Espera, Group, Kind, ModoComandos, Node, Passo,
    TEXTO_DA_SENHA, Tree,
};
use crate::recentes::{Alvo, Recente};
use crate::t;

// ---------------------------------------------------------------------------------------------
// Herança
// ---------------------------------------------------------------------------------------------

/// O que um nó diz sobre os comandos: `None` passa para a pasta de cima, `Some(None)` não roda
/// nada e `Some(Some(c))` usa `c`.
fn decide(comandos: &ComandosAoConectar) -> Option<Option<&ComandosAoConectar>> {
    match comandos.modo {
        ModoComandos::Herdar => None,
        ModoComandos::Nenhum => Some(None),
        ModoComandos::Proprios => Some(Some(comandos)),
    }
}

/// Os comandos do próprio nó, se ele for uma conexão ou uma pasta.
fn proprios(tree: &Tree, no: u64) -> Option<&ComandosAoConectar> {
    match tree.find(no)? {
        Node::Connection(c) => Some(&c.comandos),
        Node::Group(g) => Some(&g.comandos),
    }
}

/// Os comandos que o nó `no` (pasta ou conexão) herdaria: sobe a partir da pasta de cima e vale
/// o primeiro [`ModoComandos::Proprios`] (com a pasta que decidiu) ou
/// [`ModoComandos::Nenhum`] (`None`). A raiz não tem comandos.
pub fn comandos_herdados(tree: &Tree, no: u64) -> Option<(&Group, &ComandosAoConectar)> {
    for pasta in tree.ancestors(no).into_iter().rev() {
        if let Some(Node::Group(g)) = tree.find(pasta)
            && let Some(decisao) = decide(&g.comandos)
        {
            return decisao.map(|c| (g, c));
        }
    }
    None
}

/// Os comandos que valem para o nó `no`: os próprios, se ele não herda; senão os da primeira
/// pasta acima que decide ([`comandos_herdados`]). Quem escolheu "Próprios" decide mesmo com a
/// lista vazia (o que equivale a "Nenhum"). O resultado vem inteiro, os passos e o "Rodar de
/// novo ao reconectar": não há mistura entre a pasta e o nó.
pub fn comandos_efetivos(tree: &Tree, no: u64) -> Option<&ComandosAoConectar> {
    if let Some(proprios) = proprios(tree, no)
        && let Some(decisao) = decide(proprios)
    {
        return decisao;
    }
    comandos_herdados(tree, no).map(|(_, c)| c)
}

/// Se o tipo de conexão abre um terminal onde os passos podem rodar.
pub fn tem_terminal(kind: Kind) -> bool {
    matches!(kind, Kind::Ssh | Kind::Telnet | Kind::Local)
}

/// O texto da opção "Herdar" do nó `no`: mostra o que ele herdaria agora ("Herdar (Cliente A: 2
/// passos)" ou "Herdar (nenhum)").
pub fn rotulo_de_herdar(tree: &Tree, no: u64) -> String {
    match comandos_herdados(tree, no) {
        Some((pasta, comandos)) => t!(
            "comandos-herdar-de",
            nome = pasta.name.clone(),
            passos = comandos.passos.len()
        ),
        None => t!("comandos-herdar-nenhum"),
    }
}

/// O texto da linha "Comandos" do formulário do nó `no` para `comandos`: o rótulo de herdar
/// com o resumo dos passos da pasta, "Nenhum", ou o resumo dos passos próprios.
pub fn rotulo_da_linha(tree: &Tree, no: u64, comandos: &ComandosAoConectar) -> String {
    match comandos.modo {
        ModoComandos::Herdar => match comandos_herdados(tree, no) {
            Some((pasta, herdados)) => t!(
                "comandos-herdar-resumo",
                nome = pasta.name.clone(),
                resumo = resumo_dos_passos(&herdados.passos)
            ),
            None => t!("comandos-herdar-nenhum"),
        },
        ModoComandos::Nenhum => t!("comandos-nenhum"),
        ModoComandos::Proprios if comandos.passos.is_empty() => t!("comandos-nenhum"),
        ModoComandos::Proprios => resumo_dos_passos(&comandos.passos),
    }
}

// ---------------------------------------------------------------------------------------------
// O texto esperado
// ---------------------------------------------------------------------------------------------

/// As alternativas de um texto esperado, em minúsculas: separadas por `|` (`\|` é a barra
/// literal e `\\` a contrabarra), sem os espaços das pontas (uma alternativa só de espaços fica
/// como está) e sem as vazias.
pub fn alternativas(texto: &str) -> Vec<String> {
    let mut partes = vec![String::new()];
    let mut letras = texto.chars().peekable();
    while let Some(letra) = letras.next() {
        let atual = partes.last_mut().expect("sempre há uma parte");
        match letra {
            '\\' if matches!(letras.peek(), Some('|' | '\\')) => {
                atual.extend(letras.next());
            }
            '|' => partes.push(String::new()),
            _ => atual.push(letra),
        }
    }
    partes
        .into_iter()
        .filter_map(|parte| {
            let limpa = if parte.trim().is_empty() {
                parte
            } else {
                parte.trim().to_owned()
            };
            (!limpa.is_empty()).then(|| limpa.to_lowercase())
        })
        .collect()
}

/// Se a linha termina num prompt de shell (`#`, `$`, `%` ou `>`, sem contar os espaços do fim).
pub fn eh_prompt(linha: &str) -> bool {
    linha.trim_end().ends_with(['#', '$', '%', '>'])
}

// ---------------------------------------------------------------------------------------------
// O motor
// ---------------------------------------------------------------------------------------------

/// O que o motor precisa saber além dos passos.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ajustes {
    /// Quanto tempo sem saída conta como "a tela parou".
    pub quieto: Duration,
}

/// Um passo com a senha já resolvida (só nos passos de senha). O `Debug` não mostra a senha.
#[derive(Clone, PartialEq, Eq)]
pub struct PassoPronto {
    /// O passo.
    pub passo: Passo,
    /// A senha a enviar, se o passo for de senha e ela existir.
    pub senha: Option<String>,
}

impl fmt::Debug for PassoPronto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PassoPronto")
            .field("passo", &self.passo)
            .field("senha", &self.senha.as_ref().map(|_| "…"))
            .finish()
    }
}

/// O que a casca viu desde a última chamada.
#[derive(Clone, Copy, Debug)]
pub struct Observacao<'a> {
    /// `Session::received()`.
    pub recebidos: u64,
    /// `OutputTap::take()`: o texto limpo que chegou desde a última observação.
    pub texto_novo: &'a str,
    /// O texto da linha do cursor.
    pub linha_do_cursor: &'a str,
    /// Se a sessão fechou.
    pub fechada: bool,
}

/// Os bytes a mandar. O `Debug` não mostra o conteúdo quando é senha.
#[derive(Clone, PartialEq, Eq)]
pub struct Envio {
    /// O comando ou a senha, já com o Enter.
    pub bytes: Vec<u8>,
    /// Se é uma senha.
    pub senha: bool,
}

impl fmt::Debug for Envio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Envio");
        if self.senha {
            d.field("bytes", &"…");
        } else {
            d.field("bytes", &String::from_utf8_lossy(&self.bytes));
        }
        d.field("senha", &self.senha).finish()
    }
}

/// Por que a sequência parou antes do fim. O `passo` conta de 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parada {
    /// O passo esperou o tempo máximo.
    TempoEsgotado {
        /// O passo.
        passo: usize,
    },
    /// O passo pediu uma senha que não existe (credencial vazia ou cofre trancado).
    SenhaIndisponivel {
        /// O passo.
        passo: usize,
    },
    /// A sessão fechou no meio.
    Fechou {
        /// O passo.
        passo: usize,
    },
    /// A pessoa cancelou.
    Cancelada {
        /// O passo.
        passo: usize,
    },
}

/// O que fazer agora.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ordem {
    /// Mande estes bytes.
    Enviar(Envio),
    /// Nada agora; chame de novo até `ate` (ou antes).
    Esperar {
        /// A hora da próxima olhada.
        ate: Instant,
    },
    /// Todos os passos rodaram.
    Terminou,
    /// A sequência parou.
    Parou(Parada),
}

/// Como a sequência começou.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inicio {
    /// A sessão acabou de abrir (ou foi duplicada).
    Abertura,
    /// A sessão reconectou.
    Reconexao,
    /// A pessoa pediu pelo menu da aba, com a sessão já aberta e parada.
    Manual,
}

/// O que o passo atual decidiu nesta olhada.
enum Decisao {
    Enviar(Envio),
    /// A senha não foi pedida: o passo some.
    Pular,
    SemSenha,
    Esperar {
        /// As alternativas do texto esperado (para aparar o acúmulo).
        alternativas: Vec<String>,
    },
}

/// A sequência de passos de uma sessão.
#[derive(Debug)]
pub struct Motor {
    passos: Vec<PassoPronto>,
    ajustes: Ajustes,
    indice: usize,
    /// Quando o passo atual começou a esperar.
    inicio_do_passo: Instant,
    /// O `recebidos` de quando o passo atual começou a esperar.
    base: u64,
    /// O último `recebidos` visto e a hora em que ele mudou.
    visto: u64,
    ultimo_byte: Instant,
    /// O texto que chegou desde que o passo atual começou a esperar, em minúsculas.
    acumulado: String,
    /// Se o passo atual só sai depois de chegar saída nova. Falso no primeiro passo de uma
    /// sequência manual e logo depois de um passo de senha pulado (nada foi enviado, então nada
    /// vai ecoar).
    exige_saida: bool,
    fim: Option<Ordem>,
}

impl Motor {
    /// Começa a sequência `passos` em `agora`, com `recebidos` bytes já vistos na sessão.
    pub fn novo(
        passos: Vec<PassoPronto>,
        ajustes: Ajustes,
        agora: Instant,
        recebidos: u64,
        inicio: Inicio,
    ) -> Self {
        Self {
            passos,
            ajustes,
            indice: 0,
            inicio_do_passo: agora,
            base: recebidos,
            visto: recebidos,
            ultimo_byte: agora,
            acumulado: String::new(),
            exige_saida: inicio != Inicio::Manual,
            fim: None,
        }
    }

    /// (o passo atual contando de 1, o total) para o estado da aba.
    pub fn passo_atual(&self) -> (usize, usize) {
        ((self.indice + 1).min(self.passos.len()), self.passos.len())
    }

    /// O passo `indice`, contando de 0.
    pub fn passo(&self, indice: usize) -> Option<&Passo> {
        self.passos.get(indice).map(|p| &p.passo)
    }

    /// Para a sequência a pedido da pessoa.
    pub fn cancelar(&mut self) -> Ordem {
        self.parar(Parada::Cancelada { passo: self.indice })
    }

    fn parar(&mut self, parada: Parada) -> Ordem {
        let ordem = Ordem::Parou(parada);
        self.fim = Some(ordem.clone());
        ordem
    }

    /// Passa ao passo seguinte, que começa a esperar em `agora`.
    fn proximo(&mut self, agora: Instant, recebidos: u64, exige_saida: bool) {
        self.indice += 1;
        self.inicio_do_passo = agora;
        self.base = recebidos;
        self.acumulado.clear();
        self.exige_saida = exige_saida;
    }

    /// Descarta do acúmulo o que já não pode fazer parte de uma alternativa partida entre dois
    /// blocos: sobra o fim, do tamanho da maior alternativa menos um.
    fn aparar(&mut self, alternativas: &[String]) {
        let maior = alternativas
            .iter()
            .map(|a| a.chars().count())
            .max()
            .unwrap_or(0);
        let guardar = maior.saturating_sub(1);
        let total = self.acumulado.chars().count();
        if total > guardar {
            let corte = self
                .acumulado
                .char_indices()
                .nth(total - guardar)
                .map_or(self.acumulado.len(), |(i, _)| i);
            self.acumulado.drain(..corte);
        }
    }

    /// O que o passo atual faz com o que se viu.
    fn decidir(&self, pronto: &PassoPronto, tela_parou: bool, linha: &str) -> Decisao {
        let enviar = |texto: &str, senha: bool| {
            let mut bytes = texto.as_bytes().to_vec();
            bytes.push(b'\r');
            Decisao::Enviar(Envio { bytes, senha })
        };
        match &pronto.passo.acao {
            AcaoDoPasso::Comando { texto } => match &pronto.passo.espera {
                Espera::Logo => enviar(texto, false),
                Espera::TelaParar => {
                    if tela_parou {
                        enviar(texto, false)
                    } else {
                        Decisao::Esperar {
                            alternativas: Vec::new(),
                        }
                    }
                }
                Espera::Texto { texto: esperado } => {
                    let alternativas = alternativas(esperado);
                    let achou = if alternativas.is_empty() {
                        tela_parou
                    } else {
                        alternativas
                            .iter()
                            .any(|a| self.acumulado.contains(a.as_str()))
                    };
                    if achou {
                        enviar(texto, false)
                    } else {
                        Decisao::Esperar { alternativas }
                    }
                }
            },
            AcaoDoPasso::Senha { texto, .. } => {
                let mut pedidos = alternativas(texto);
                if pedidos.is_empty() {
                    pedidos = alternativas(TEXTO_DA_SENHA);
                }
                let linha_minuscula = linha.to_lowercase();
                let pedida = pedidos.iter().any(|a| linha_minuscula.contains(a.as_str()));
                if pedida {
                    match &pronto.senha {
                        Some(senha) => enviar(senha, true),
                        None => Decisao::SemSenha,
                    }
                } else if tela_parou && eh_prompt(linha) {
                    Decisao::Pular
                } else {
                    Decisao::Esperar {
                        alternativas: Vec::new(),
                    }
                }
            }
        }
    }

    /// Decide o que fazer agora, depois de ver `obs` em `agora`.
    pub fn avancar(&mut self, agora: Instant, obs: &Observacao<'_>) -> Ordem {
        if let Some(fim) = &self.fim {
            return fim.clone();
        }
        if obs.recebidos != self.visto {
            self.visto = obs.recebidos;
            self.ultimo_byte = agora;
        }
        if obs.fechada {
            return self.parar(Parada::Fechou { passo: self.indice });
        }
        self.acumulado.push_str(&obs.texto_novo.to_lowercase());
        loop {
            let Some(pronto) = self.passos.get(self.indice) else {
                self.fim = Some(Ordem::Terminou);
                return Ordem::Terminou;
            };
            let max = Duration::from_secs(u64::from(pronto.passo.max_secs));
            let quieto = agora.saturating_duration_since(self.ultimo_byte) >= self.ajustes.quieto;
            let chegou = obs.recebidos > self.base || !self.exige_saida;
            let decisao = self.decidir(pronto, quieto && chegou, obs.linha_do_cursor);
            match decisao {
                Decisao::Enviar(envio) => {
                    self.proximo(agora, obs.recebidos, true);
                    return Ordem::Enviar(envio);
                }
                Decisao::Pular => {
                    self.proximo(agora, obs.recebidos, false);
                }
                Decisao::SemSenha => {
                    return self.parar(Parada::SenhaIndisponivel { passo: self.indice });
                }
                Decisao::Esperar { alternativas } => {
                    if agora.saturating_duration_since(self.inicio_do_passo) >= max {
                        return self.parar(Parada::TempoEsgotado { passo: self.indice });
                    }
                    self.aparar(&alternativas);
                    let mut ate = (self.inicio_do_passo + max).min(agora + self.ajustes.quieto / 4);
                    let fim_da_quietude = self.ultimo_byte + self.ajustes.quieto;
                    if fim_da_quietude > agora {
                        ate = ate.min(fim_da_quietude);
                    }
                    return Ordem::Esperar { ate };
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Autocompletar
// ---------------------------------------------------------------------------------------------

/// Os textos dos passos de comando de `comandos`, sem os espaços das pontas e sem os vazios.
fn textos_dos_comandos(comandos: &ComandosAoConectar) -> impl Iterator<Item = &str> {
    comandos.passos.iter().filter_map(|p| match &p.acao {
        AcaoDoPasso::Comando { texto } => Some(texto.trim()).filter(|t| !t.is_empty()),
        AcaoDoPasso::Senha { .. } => None,
    })
}

fn juntar_pastas<'a>(nos: &'a [Node], fora: &mut Vec<&'a Group>) {
    for no in nos {
        if let Node::Group(g) = no {
            fora.push(g);
            juntar_pastas(&g.children, fora);
        }
    }
}

/// Os comandos que o campo do comando sugere para `digitado`: os dos passos de todas as
/// conexões (os herdados contam), mais os das pastas que ninguém herda, mais o `historico` da
/// barra de envio (mais recente primeiro). Primeiro os que mais conexões usam, depois o uso mais
/// recente, depois a ordem alfabética; o histórico vem depois, na ordem dele. Passos de senha
/// nunca entram. Sem teto.
pub fn sugestoes(
    tree: &Tree,
    recentes: &[Recente],
    historico: &[String],
    digitado: &str,
) -> Vec<String> {
    // comando -> (conexões que usam, uso mais recente)
    let mut usados: HashMap<String, (usize, u64)> = HashMap::new();
    let ultimo_uso = |c: &Connection| {
        recentes
            .iter()
            .find(|r| r.alvo == Alvo::Conexao { id: c.id })
            .map_or(0, |r| r.ultimo)
    };
    for c in tree.connections() {
        if !tem_terminal(c.kind) {
            continue;
        }
        let Some(comandos) = comandos_efetivos(tree, c.id) else {
            continue;
        };
        let uso = ultimo_uso(c);
        let mut vistos: Vec<&str> = Vec::new();
        for texto in textos_dos_comandos(comandos) {
            if vistos.contains(&texto) {
                continue;
            }
            vistos.push(texto);
            let entrada = usados.entry(texto.to_owned()).or_default();
            entrada.0 += 1;
            entrada.1 = entrada.1.max(uso);
        }
    }
    let mut pastas = Vec::new();
    juntar_pastas(&tree.nodes, &mut pastas);
    for pasta in pastas {
        if pasta.comandos.modo == ModoComandos::Proprios {
            for texto in textos_dos_comandos(&pasta.comandos) {
                usados.entry(texto.to_owned()).or_default();
            }
        }
    }

    let filtro = digitado.to_lowercase();
    let combina = |comando: &str| {
        comando != digitado && (filtro.is_empty() || comando.to_lowercase().contains(&filtro))
    };
    let mut lista: Vec<(String, (usize, u64))> = usados
        .into_iter()
        .filter(|(comando, _)| combina(comando))
        .collect();
    lista.sort_by(|(a, (na, ua)), (b, (nb, ub))| nb.cmp(na).then(ub.cmp(ua)).then(a.cmp(b)));
    let mut saida: Vec<String> = lista.into_iter().map(|(comando, _)| comando).collect();
    for comando in historico {
        let comando = comando.trim();
        if !comando.is_empty() && combina(comando) && !saida.iter().any(|s| s == comando) {
            saida.push(comando.to_owned());
        }
    }
    saida
}

/// O resumo dos passos para a linha do formulário: os três primeiros (o comando, ou "senha"), e
/// "…" se houver mais. Vazio sem passos.
pub fn resumo_dos_passos(passos: &[Passo]) -> String {
    let mut itens: Vec<String> = passos
        .iter()
        .take(3)
        .map(|p| match &p.acao {
            AcaoDoPasso::Comando { texto } => texto.trim().to_owned(),
            AcaoDoPasso::Senha { .. } => crate::t!("comandos-resumo-senha"),
        })
        .collect();
    if passos.len() > 3 {
        itens.push("…".to_owned());
    }
    itens.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modelo::{Connection, Kind, OrigemDaSenha};

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn ajustes() -> Ajustes {
        Ajustes { quieto: MS(600) }
    }

    fn pronto(passo: Passo) -> PassoPronto {
        PassoPronto { passo, senha: None }
    }

    fn comando(texto: &str, espera: Espera) -> PassoPronto {
        pronto(Passo {
            acao: AcaoDoPasso::Comando {
                texto: texto.into(),
            },
            espera,
            max_secs: 15,
        })
    }

    fn senha(texto: &str, segredo: Option<&str>) -> PassoPronto {
        PassoPronto {
            passo: Passo {
                acao: AcaoDoPasso::Senha {
                    origem: OrigemDaSenha::Conexao,
                    texto: texto.into(),
                },
                espera: Espera::TelaParar,
                max_secs: 15,
            },
            senha: segredo.map(str::to_owned),
        }
    }

    fn obs<'a>(recebidos: u64, texto_novo: &'a str, linha: &'a str) -> Observacao<'a> {
        Observacao {
            recebidos,
            texto_novo,
            linha_do_cursor: linha,
            fechada: false,
        }
    }

    fn bytes(ordem: &Ordem) -> &[u8] {
        match ordem {
            Ordem::Enviar(e) => &e.bytes,
            outra => panic!("esperava Enviar, veio {outra:?}"),
        }
    }

    fn e_esperar(ordem: &Ordem) -> bool {
        matches!(ordem, Ordem::Esperar { .. })
    }

    #[test]
    fn tela_parada_espera_a_quietude() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando("ls", Espera::TelaParar)],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert!(e_esperar(&m.avancar(t0 + MS(100), &obs(5, "oi", ""))));
        assert!(e_esperar(&m.avancar(t0 + MS(500), &obs(5, "", ""))));
        let ordem = m.avancar(t0 + MS(700), &obs(5, "", ""));
        assert_eq!(bytes(&ordem), b"ls\r");
        assert_eq!(m.avancar(t0 + MS(800), &obs(9, "", "")), Ordem::Terminou);
    }

    #[test]
    fn tela_parada_sem_saida_nenhuma_nao_sai() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando("ls", Espera::TelaParar)],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert!(e_esperar(&m.avancar(t0 + MS(5000), &obs(0, "", ""))));
    }

    #[test]
    fn tela_parada_exige_saida_depois_do_envio() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("a", Espera::TelaParar),
                comando("b", Espera::TelaParar),
            ],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert!(e_esperar(&m.avancar(t0 + MS(100), &obs(3, "", ""))));
        assert_eq!(bytes(&m.avancar(t0 + MS(700), &obs(3, "", ""))), b"a\r");
        // Sem o eco, nem 2 s depois o passo 2 sai.
        assert!(e_esperar(&m.avancar(t0 + MS(2700), &obs(3, "", ""))));
        // O eco chega e a tela quieta.
        assert!(e_esperar(&m.avancar(t0 + MS(2800), &obs(6, "a", ""))));
        assert_eq!(bytes(&m.avancar(t0 + MS(3500), &obs(6, "", ""))), b"b\r");
    }

    #[test]
    fn manual_nao_exige_saida_no_primeiro_passo() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("a", Espera::TelaParar),
                comando("b", Espera::TelaParar),
            ],
            ajustes(),
            t0,
            40,
            Inicio::Manual,
        );
        // Já estava quieto (o último byte é o do começo da sequência).
        assert!(e_esperar(&m.avancar(t0 + MS(100), &obs(40, "", ""))));
        assert_eq!(bytes(&m.avancar(t0 + MS(600), &obs(40, "", ""))), b"a\r");
        // O segundo passo já exige saída.
        assert!(e_esperar(&m.avancar(t0 + MS(5000), &obs(40, "", ""))));
    }

    #[test]
    fn texto_esperado_com_alternativas_e_maiusculas() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando(
                "go",
                Espera::Texto {
                    texto: "Pronto|ok".into(),
                },
            )],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert!(e_esperar(&m.avancar(t0 + MS(10), &obs(1, "tudo O", ""))));
        assert_eq!(bytes(&m.avancar(t0 + MS(20), &obs(2, "K!", ""))), b"go\r");
    }

    #[test]
    fn texto_esperado_ignora_o_que_veio_antes_do_passo() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("um", Espera::Logo),
                comando("dois", Espera::Texto { texto: "ok".into() }),
            ],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        // O "ok" chega junto do envio do primeiro passo: não vale para o segundo.
        assert_eq!(bytes(&m.avancar(t0, &obs(2, "ok", ""))), b"um\r");
        assert!(e_esperar(&m.avancar(t0 + MS(10), &obs(2, "", ""))));
        assert_eq!(bytes(&m.avancar(t0 + MS(20), &obs(4, "ok", ""))), b"dois\r");
    }

    #[test]
    fn texto_esperado_vazio_espera_a_tela_parar() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando("x", Espera::Texto { texto: "||".into() })],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert!(e_esperar(&m.avancar(t0 + MS(100), &obs(1, "a", ""))));
        assert_eq!(bytes(&m.avancar(t0 + MS(800), &obs(1, "", ""))), b"x\r");
    }

    #[test]
    fn alternativas_com_barra_escapada() {
        assert_eq!(alternativas("a\\|b|c"), ["a|b", "c"]);
        assert_eq!(alternativas("a\\\\|b"), ["a\\", "b"]);
        assert_eq!(alternativas("Pronto| ok |"), ["pronto", "ok"]);
        assert_eq!(alternativas("x|   "), ["x", "   "]);
        assert!(alternativas("").is_empty());
        assert!(alternativas("||").is_empty());
    }

    #[test]
    fn logo_sai_na_hora_e_em_sequencia() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("a", Espera::Logo),
                comando("b", Espera::Logo),
                comando("c", Espera::Logo),
            ],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(bytes(&m.avancar(t0, &obs(0, "", ""))), b"a\r");
        assert_eq!(bytes(&m.avancar(t0, &obs(0, "", ""))), b"b\r");
        assert_eq!(bytes(&m.avancar(t0, &obs(0, "", ""))), b"c\r");
        assert_eq!(m.avancar(t0, &obs(0, "", "")), Ordem::Terminou);
    }

    #[test]
    fn sem_passos_termina_na_hora() {
        let t0 = Instant::now();
        let mut m = Motor::novo(Vec::new(), ajustes(), t0, 0, Inicio::Abertura);
        assert_eq!(m.avancar(t0, &obs(0, "", "")), Ordem::Terminou);
    }

    #[test]
    fn tempo_esgotado_para_no_passo_certo() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("a", Espera::Logo),
                comando(
                    "b",
                    Espera::Texto {
                        texto: "nunca".into(),
                    },
                ),
            ],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(bytes(&m.avancar(t0, &obs(0, "", ""))), b"a\r");
        assert!(e_esperar(&m.avancar(t0 + MS(14_000), &obs(1, "x", ""))));
        assert_eq!(
            m.avancar(t0 + MS(15_000), &obs(1, "", "")),
            Ordem::Parou(Parada::TempoEsgotado { passo: 1 })
        );
        // Parou, parou: depois disso nada muda.
        assert_eq!(
            m.avancar(t0 + MS(16_000), &obs(9, "nunca", "")),
            Ordem::Parou(Parada::TempoEsgotado { passo: 1 })
        );
    }

    #[test]
    fn esperar_nunca_passa_do_tempo_maximo() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando("a", Espera::Texto { texto: "x".into() })],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        let Ordem::Esperar { ate } = m.avancar(t0 + MS(14_950), &obs(0, "", "")) else {
            panic!("esperava Esperar");
        };
        assert!(ate <= t0 + MS(15_000));
        assert!(ate > t0 + MS(14_950));
    }

    #[test]
    fn senha_so_na_linha_do_cursor() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("sudo su -", Espera::Logo),
                senha("password|senha", Some("segredo")),
            ],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(bytes(&m.avancar(t0, &obs(0, "", ""))), b"sudo su -\r");
        // "Password expires" no fluxo, linha do cursor ainda no prompt: nada.
        let ordem = m.avancar(
            t0 + MS(100),
            &obs(30, "Password expires in 5 days", "ana@web:~$ "),
        );
        assert!(e_esperar(&ordem));
        let ordem = m.avancar(t0 + MS(200), &obs(60, "", "[sudo] password for ana: "));
        match ordem {
            Ordem::Enviar(e) => {
                assert!(e.senha);
                assert_eq!(e.bytes, b"segredo\r");
            }
            outra => panic!("esperava Enviar, veio {outra:?}"),
        }
    }

    #[test]
    fn senha_pulada_quando_o_prompt_volta() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("sudo su -", Espera::Logo),
                senha("password", Some("segredo")),
                comando("cd /srv", Espera::TelaParar),
            ],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(bytes(&m.avancar(t0, &obs(0, "", ""))), b"sudo su -\r");
        // O eco e o prompt de root chegam; a tela ainda não parou.
        assert!(e_esperar(
            &m.avancar(t0 + MS(100), &obs(40, "x", "root@web:~# "))
        ));
        // Parou: o passo da senha some sem enviar e o seguinte já sai (o prompt é a saída que
        // faltava).
        let ordem = m.avancar(t0 + MS(800), &obs(40, "", "root@web:~# "));
        assert_eq!(bytes(&ordem), b"cd /srv\r");
    }

    #[test]
    fn prompt_de_todo_tipo_pula_a_senha() {
        for linha in ["a# ", "a$", "a% ", "a> "] {
            let t0 = Instant::now();
            let mut m = Motor::novo(
                vec![senha("password", Some("x"))],
                ajustes(),
                t0,
                0,
                Inicio::Abertura,
            );
            assert!(e_esperar(&m.avancar(t0 + MS(10), &obs(8, "oi", linha))));
            assert_eq!(m.avancar(t0 + MS(700), &obs(8, "", linha)), Ordem::Terminou);
        }
    }

    #[test]
    fn senha_ausente_para() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![senha("password", None)],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(
            m.avancar(t0 + MS(10), &obs(5, "", "Password: ")),
            Ordem::Parou(Parada::SenhaIndisponivel { passo: 0 })
        );
    }

    #[test]
    fn senha_sem_texto_usa_o_pedido_padrao() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![senha("", Some("x"))],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(bytes(&m.avancar(t0, &obs(5, "", "Senha: "))), b"x\r");
    }

    #[test]
    fn senha_nao_aparece_no_debug() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![senha("password", Some("segredo-secreto"))],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert!(!format!("{m:?}").contains("segredo-secreto"));
        let ordem = m.avancar(t0, &obs(1, "", "Password: "));
        assert!(!format!("{ordem:?}").contains("segredo-secreto"));
        let Ordem::Enviar(envio) = ordem else {
            panic!("esperava Enviar");
        };
        assert!(!format!("{envio:?}").contains("segredo-secreto"));
        assert!(!format!("{m:?}").contains("segredo-secreto"));
    }

    #[test]
    fn sessao_fechada_para() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![
                comando("a", Espera::Logo),
                comando("b", Espera::Texto { texto: "x".into() }),
            ],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(bytes(&m.avancar(t0, &obs(0, "", ""))), b"a\r");
        let fechada = Observacao {
            fechada: true,
            ..obs(1, "", "")
        };
        assert_eq!(
            m.avancar(t0 + MS(10), &fechada),
            Ordem::Parou(Parada::Fechou { passo: 1 })
        );
    }

    #[test]
    fn cancelar_para() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando("a", Espera::TelaParar)],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(m.cancelar(), Ordem::Parou(Parada::Cancelada { passo: 0 }));
        assert_eq!(
            m.avancar(t0 + MS(5000), &obs(9, "", "")),
            Ordem::Parou(Parada::Cancelada { passo: 0 })
        );
    }

    #[test]
    fn passo_atual_conta_de_um() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando("a", Espera::Logo), comando("b", Espera::Logo)],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        assert_eq!(m.passo_atual(), (1, 2));
        m.avancar(t0, &obs(0, "", ""));
        assert_eq!(m.passo_atual(), (2, 2));
        m.avancar(t0, &obs(0, "", ""));
        m.avancar(t0, &obs(0, "", ""));
        assert_eq!(m.passo_atual(), (2, 2));
    }

    #[test]
    fn o_acumulo_nao_cresce_sem_fim() {
        let t0 = Instant::now();
        let mut m = Motor::novo(
            vec![comando(
                "a",
                Espera::Texto {
                    texto: "pronto".into(),
                },
            )],
            ajustes(),
            t0,
            0,
            Inicio::Abertura,
        );
        for i in 0..200_u64 {
            m.avancar(t0 + MS(i), &obs(i, "xxxxxxxxxxxxxxxxxxxx", ""));
        }
        assert!(m.acumulado.chars().count() <= 5 + 20);
        // A alternativa partida entre dois blocos ainda casa.
        m.avancar(t0 + MS(300), &obs(300, "pro", ""));
        assert_eq!(
            bytes(&m.avancar(t0 + MS(301), &obs(301, "nto", ""))),
            b"a\r"
        );
    }

    #[test]
    fn eh_prompt_olha_o_fim_da_linha() {
        assert!(eh_prompt("root@web:~# "));
        assert!(eh_prompt("ana@web:~$"));
        assert!(eh_prompt("PS C:\\> "));
        assert!(eh_prompt("web% "));
        assert!(!eh_prompt("[sudo] password for ana: "));
        assert!(!eh_prompt(""));
    }

    // --- Herança ---

    /// Raiz → "Clientes" → "A" → conexão `web`; mais a conexão `solta` na raiz.
    struct Cena {
        tree: Tree,
        clientes: u64,
        grupo_a: u64,
        web: u64,
        solta: u64,
    }

    fn cena() -> Cena {
        let mut tree = Tree::default();
        let clientes = tree.add_group(None, "Clientes");
        let grupo_a = tree.add_group(Some(clientes), "A");
        let web = tree.add_connection(Some(grupo_a), Connection::new("web", Kind::Ssh));
        let solta = tree.add_connection(None, Connection::new("solta", Kind::Ssh));
        Cena {
            tree,
            clientes,
            grupo_a,
            web,
            solta,
        }
    }

    fn proprios_de(passos: &[&str]) -> ComandosAoConectar {
        ComandosAoConectar {
            modo: ModoComandos::Proprios,
            passos: passos.iter().map(|c| Passo::comando(*c, 15)).collect(),
            reconectar: true,
        }
    }

    fn poe(tree: &mut Tree, no: u64, comandos: ComandosAoConectar) {
        match tree.find_mut(no) {
            Some(Node::Group(g)) => g.comandos = comandos,
            Some(Node::Connection(c)) => c.comandos = comandos,
            None => panic!("sem o nó {no}"),
        }
    }

    fn textos(c: Option<&ComandosAoConectar>) -> Vec<String> {
        c.map(|c| textos_dos_comandos(c).map(str::to_owned).collect())
            .unwrap_or_default()
    }

    #[test]
    fn raiz_nao_tem_comandos() {
        let c = cena();
        assert!(comandos_efetivos(&c.tree, c.web).is_none());
        assert!(comandos_efetivos(&c.tree, c.solta).is_none());
        assert!(comandos_efetivos(&c.tree, c.clientes).is_none());
        assert!(comandos_herdados(&c.tree, c.clientes).is_none());
    }

    #[test]
    fn herda_da_pasta_mais_proxima() {
        let mut c = cena();
        poe(&mut c.tree, c.clientes, proprios_de(&["fora"]));
        assert_eq!(textos(comandos_efetivos(&c.tree, c.web)), ["fora"]);
        poe(&mut c.tree, c.grupo_a, proprios_de(&["dentro"]));
        assert_eq!(textos(comandos_efetivos(&c.tree, c.web)), ["dentro"]);
        let (pasta, _) = comandos_herdados(&c.tree, c.web).unwrap();
        assert_eq!(pasta.name, "A");
        // A conexão com comandos próprios fica com eles, sem mistura.
        poe(&mut c.tree, c.web, proprios_de(&["meu"]));
        assert_eq!(textos(comandos_efetivos(&c.tree, c.web)), ["meu"]);
        // Fora da pasta, nada.
        assert!(comandos_efetivos(&c.tree, c.solta).is_none());
    }

    #[test]
    fn nenhum_corta_a_heranca() {
        let mut c = cena();
        poe(&mut c.tree, c.clientes, proprios_de(&["fora"]));
        poe(
            &mut c.tree,
            c.grupo_a,
            ComandosAoConectar {
                modo: ModoComandos::Nenhum,
                ..ComandosAoConectar::default()
            },
        );
        assert!(comandos_efetivos(&c.tree, c.web).is_none());
        assert!(comandos_herdados(&c.tree, c.web).is_none());
        // A própria pasta A também não herda.
        assert!(comandos_efetivos(&c.tree, c.grupo_a).is_none());
        assert_eq!(textos(comandos_efetivos(&c.tree, c.clientes)), ["fora"]);
    }

    #[test]
    fn proprios_vazio_equivale_a_nenhum() {
        let mut c = cena();
        poe(&mut c.tree, c.clientes, proprios_de(&["fora"]));
        poe(&mut c.tree, c.web, proprios_de(&[]));
        let efetivos = comandos_efetivos(&c.tree, c.web).unwrap();
        assert!(efetivos.passos.is_empty());
    }

    #[test]
    fn reconectar_vem_junto_da_pasta() {
        let mut c = cena();
        let mut pasta = proprios_de(&["x"]);
        pasta.reconectar = false;
        poe(&mut c.tree, c.clientes, pasta);
        assert!(!comandos_efetivos(&c.tree, c.web).unwrap().reconectar);
    }

    #[test]
    fn os_rotulos_mostram_o_que_se_herda() {
        let mut c = cena();
        assert_eq!(rotulo_de_herdar(&c.tree, c.web), "Herdar (nenhum)");
        poe(&mut c.tree, c.clientes, proprios_de(&["sudo su -", "ls"]));
        assert_eq!(
            rotulo_de_herdar(&c.tree, c.web),
            "Herdar (Clientes: 2 passos)"
        );
        let herdar = ComandosAoConectar::default();
        assert_eq!(
            rotulo_da_linha(&c.tree, c.web, &herdar),
            "Herdar (Clientes: sudo su -, ls)"
        );
        assert_eq!(
            rotulo_da_linha(&c.tree, c.solta, &herdar),
            "Herdar (nenhum)"
        );
        let nenhum = ComandosAoConectar {
            modo: ModoComandos::Nenhum,
            ..ComandosAoConectar::default()
        };
        assert_eq!(rotulo_da_linha(&c.tree, c.web, &nenhum), "Nenhum");
        let mut proprios = proprios_de(&["a", "b", "c", "d"]);
        proprios.passos[1] = Passo::senha(OrigemDaSenha::Conexao, 15);
        assert_eq!(rotulo_da_linha(&c.tree, c.web, &proprios), "a, senha, c, …");
        assert_eq!(rotulo_da_linha(&c.tree, c.web, &proprios_de(&[])), "Nenhum");
    }

    // --- Sugestões ---

    fn recente(id: u64, ultimo: u64) -> Recente {
        Recente {
            alvo: Alvo::Conexao { id },
            titulo: String::new(),
            detalhe: String::new(),
            ultimo,
            vezes: 1,
            fixado: false,
        }
    }

    fn com_comandos(tree: &mut Tree, nome: &str, kind: Kind, passos: &[&str]) -> u64 {
        let mut conexao = Connection::new(nome, kind);
        conexao.comandos = proprios_de(passos);
        tree.add_connection(None, conexao)
    }

    #[test]
    fn ordena_por_conexoes_depois_recencia_depois_alfabeto() {
        let mut tree = Tree::default();
        let a = com_comandos(&mut tree, "a", Kind::Ssh, &["sudo su -", "zeta", "beta"]);
        let b = com_comandos(&mut tree, "b", Kind::Telnet, &["sudo su -", "alfa", "gama"]);
        let _ = (a, b);
        let c = com_comandos(&mut tree, "c", Kind::Local, &["gama"]);
        let recentes = [recente(c, 100), recente(a, 50)];
        // "sudo su -" (2 conexões) e "gama" (2) vêm primeiro; "gama" tem o uso mais recente
        // (c, 100). Depois os de uma conexão: "zeta" e "beta" (a, 50) antes de "alfa" (nunca).
        assert_eq!(
            sugestoes(&tree, &recentes, &[], ""),
            ["gama", "sudo su -", "beta", "zeta", "alfa"]
        );
    }

    #[test]
    fn herdados_contam() {
        let mut c = cena();
        poe(&mut c.tree, c.clientes, proprios_de(&["sudo su -"]));
        let outra = com_comandos(&mut c.tree, "outra", Kind::Ssh, &["sudo su -", "uptime"]);
        let _ = outra;
        // "sudo su -" vale para web (herdado) e outra; "uptime" só para outra.
        assert_eq!(sugestoes(&c.tree, &[], &[], ""), ["sudo su -", "uptime"]);
    }

    #[test]
    fn pasta_que_ninguem_herda_tambem_sugere() {
        let mut tree = Tree::default();
        let pasta = tree.add_group(None, "Vazia");
        poe(&mut tree, pasta, proprios_de(&["top"]));
        assert_eq!(sugestoes(&tree, &[], &[], ""), ["top"]);
    }

    #[test]
    fn so_terminal_sugere() {
        let mut tree = Tree::default();
        com_comandos(&mut tree, "ftp", Kind::Ftp, &["ls"]);
        assert!(sugestoes(&tree, &[], &[], "").is_empty());
    }

    #[test]
    fn filtra_sem_diferenciar_maiusculas() {
        let mut tree = Tree::default();
        com_comandos(&mut tree, "a", Kind::Ssh, &["sudo su -", "ls", "SUDO -i"]);
        assert_eq!(sugestoes(&tree, &[], &[], "SU"), ["SUDO -i", "sudo su -"]);
        assert!(sugestoes(&tree, &[], &[], "zzz").is_empty());
    }

    #[test]
    fn nao_sugere_o_que_ja_esta_escrito() {
        let mut tree = Tree::default();
        com_comandos(&mut tree, "a", Kind::Ssh, &["sudo su -", "sudo su - root"]);
        assert_eq!(sugestoes(&tree, &[], &[], "sudo su -"), ["sudo su - root"]);
    }

    #[test]
    fn historico_entra_depois_sem_repetir() {
        let mut tree = Tree::default();
        com_comandos(&mut tree, "a", Kind::Ssh, &["ls"]);
        let historico = vec![
            "pwd".to_owned(),
            "ls".to_owned(),
            " ".to_owned(),
            "id".to_owned(),
        ];
        assert_eq!(sugestoes(&tree, &[], &historico, ""), ["ls", "pwd", "id"]);
        assert_eq!(sugestoes(&tree, &[], &historico, "p"), ["pwd"]);
    }

    #[test]
    fn senha_nunca_e_sugerida() {
        let mut tree = Tree::default();
        let mut conexao = Connection::new("a", Kind::Ssh);
        conexao.comandos = ComandosAoConectar {
            modo: ModoComandos::Proprios,
            passos: vec![
                Passo::comando("sudo su -", 15),
                Passo::senha(OrigemDaSenha::Conexao, 15),
            ],
            reconectar: true,
        };
        tree.add_connection(None, conexao);
        assert_eq!(sugestoes(&tree, &[], &[], ""), ["sudo su -"]);
    }

    #[test]
    fn comando_repetido_na_mesma_conexao_conta_uma_vez() {
        let mut tree = Tree::default();
        com_comandos(&mut tree, "a", Kind::Ssh, &["ls", "ls", "pwd"]);
        com_comandos(&mut tree, "b", Kind::Ssh, &["pwd"]);
        assert_eq!(sugestoes(&tree, &[], &[], ""), ["pwd", "ls"]);
    }
}
