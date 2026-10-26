//! A redação (M52, comum com o M51): troca os segredos do texto por marcadores **antes** de ele
//! sair do computador. É puro, sem widgets nem threads.
//!
//! Três fontes de segredo:
//!
//! - os **segredos do cofre** (as senhas, os campos ocultos e as frases-senha das chaves), que
//!   casam literalmente e viram `[segredo removido]`; os de 1 a 3 caracteres são ignorados
//!   (trocariam letras soltas da saída);
//! - os **padrões comuns** (chaves privadas, tokens de serviços, cabeçalhos de autorização,
//!   atribuições `senha=…`, senhas em URL e JWT), que viram `[segredo removido: {tipo}]`;
//! - os **extras** que a pessoa escreveu em `ia.redigir_extras`: texto literal, ou uma expressão
//!   regular com o prefixo `re:`.
//!
//! O [`Redator`] é montado a cada envio (os segredos do cofre mudam) e descartado em seguida; os
//! segredos ficam em [`Zeroizing`].

use std::ops::Range;
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};
use tomni_ui::ai::Zeroizing;

use crate::t;

/// O marcador dos segredos literais (do cofre e dos extras).
pub const MARCADOR: &str = "[segredo removido]";

/// Os segredos do cofre com até este tamanho (em caracteres) não são procurados.
const TAMANHO_MINIMO: usize = 4;

/// O prefixo de um extra que é expressão regular.
const PREFIXO_REGEX: &str = "re:";

/// Um padrão comum: o que ele acha e como se chama no marcador.
struct Padrao {
    /// O tipo que vai no marcador (`[segredo removido: chave privada]`).
    tipo: &'static str,
    /// A expressão. Se tiver o grupo `valor`, só ele é trocado (o resto fica, como o nome da
    /// variável em `senha=…`).
    regex: Regex,
}

/// Monta um padrão. As expressões são fixas: um erro aqui é erro de programação, e o teste
/// `os_padroes_compilam` o pega.
fn padrao(tipo: &'static str, expressao: &str) -> Padrao {
    let regex = RegexBuilder::new(expressao)
        .size_limit(16 << 20)
        .build()
        .unwrap_or_else(|erro| panic!("padrão de redação inválido ({tipo}): {erro}"));
    Padrao { tipo, regex }
}

/// Os padrões, na ordem em que valem (os mais específicos primeiro).
static PADROES: LazyLock<Vec<Padrao>> = LazyLock::new(|| {
    vec![
        padrao(
            "chave privada",
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----.*?-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----",
        ),
        // Um bloco cortado (o histórico começou no meio): vale até o fim do texto.
        padrao(
            "chave privada",
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----.*",
        ),
        padrao(
            "chave PuTTY",
            r"(?s)PuTTY-User-Key-File-\d+:.*?Private-MAC:\s*[0-9a-fA-F]+",
        ),
        padrao("chave da AWS", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        padrao(
            "chave secreta da AWS",
            r"(?i)\baws_secret_access_key\s*[=:]\s*(?P<valor>[^\s]+)",
        ),
        padrao("token do GitHub", r"\bgh[pousr]_[A-Za-z0-9]{20,}"),
        padrao("token do GitHub", r"\bgithub_pat_[A-Za-z0-9_]{20,}"),
        padrao("token do GitLab", r"\bglpat-[A-Za-z0-9_\-]{20,}"),
        padrao("token do Slack", r"\bxox[baprs]-[A-Za-z0-9\-]{10,}"),
        padrao("chave de API", r"\bsk-(?:ant-)?[A-Za-z0-9_\-]{20,}"),
        padrao("chave do Google", r"\bAIza[0-9A-Za-z_\-]{35}"),
        padrao(
            "JWT",
            r"\beyJ[A-Za-z0-9_\-]{5,}\.eyJ[A-Za-z0-9_\-]{5,}\.[A-Za-z0-9_\-]{5,}",
        ),
        padrao(
            "autorização",
            r"(?i)\b(?:Bearer|Basic)[ \t]+(?P<valor>[A-Za-z0-9._~+/=\-]{8,})",
        ),
        padrao(
            "senha em URL",
            r"[a-zA-Z][a-zA-Z0-9+.\-]*://[^\s/:@]+:(?P<valor>[^\s/@]+)@",
        ),
        padrao(
            "senha",
            r#"(?i)--(?:password|passwd|senha|token|api-key)[ \t]+(?P<valor>\[segredo removido[^\]\n]*\]|"[^"\n]*"|'[^'\n]*'|[^\s"'-][^\s"']*)"#,
        ),
        padrao(
            "senha",
            r#"(?i)\b(?:password|passwd|senha|secret|token|api[_-]?key)\b[ \t]*[=:][ \t]*(?P<valor>\[segredo removido[^\]\n]*\]|"[^"\n]*"|'[^'\n]*'|[^\s"']+)"#,
        ),
    ]
});

/// Quantos segredos saíram de um texto, por origem.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contagem {
    /// Os literais do cofre.
    pub cofre: usize,
    /// Os padrões comuns (tokens, chaves, atribuições…).
    pub padroes: usize,
    /// Os extras de `ia.redigir_extras`.
    pub extras: usize,
}

impl Contagem {
    /// O total.
    pub fn total(&self) -> usize {
        self.cofre + self.padroes + self.extras
    }

    /// Soma outra contagem.
    pub fn somar(&mut self, outra: Contagem) {
        self.cofre += outra.cofre;
        self.padroes += outra.padroes;
        self.extras += outra.extras;
    }

    /// A frase do "O que vai sair": "Removi 3 segredos: 2 do cofre, 1 por padrão." Vazio quando
    /// nada saiu.
    pub fn frase(&self) -> String {
        if self.total() == 0 {
            return String::new();
        }
        let mut partes = Vec::new();
        if self.cofre > 0 {
            partes.push(t!("redacao-do-cofre", n = self.cofre.to_string()));
        }
        if self.padroes > 0 {
            partes.push(t!("redacao-por-padrao", n = self.padroes.to_string()));
        }
        if self.extras > 0 {
            partes.push(t!("redacao-dos-extras", n = self.extras.to_string()));
        }
        t!(
            "redacao-removi",
            total = self.total().to_string(),
            partes = partes.join(", ")
        )
    }
}

/// Um extra que não serve (a expressão regular tem erro), para a página de Configurações mostrar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtraInvalido {
    /// O texto como está na lista (com o `re:`).
    pub texto: String,
    /// O motivo.
    pub erro: String,
}

/// O que a redação procura num texto.
pub struct Redator {
    /// Os segredos do cofre, do maior para o menor.
    segredos: Vec<Zeroizing<String>>,
    /// Os extras literais.
    extras_literais: Vec<String>,
    /// Os extras que são expressão regular.
    extras_regex: Vec<Regex>,
    /// Os extras que não serviram.
    invalidos: Vec<ExtraInvalido>,
    /// Se está desligado (`ia.redigir` em `false`): o texto passa como está.
    desligado: bool,
}

/// O `Debug` não mostra os segredos: só quantos são.
impl std::fmt::Debug for Redator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redator")
            .field("segredos", &self.segredos.len())
            .field(
                "extras",
                &(self.extras_literais.len() + self.extras_regex.len()),
            )
            .field("desligado", &self.desligado)
            .finish()
    }
}

impl Redator {
    /// Monta o redator com os segredos do cofre e a lista de extras (`ia.redigir_extras`: um por
    /// item; o prefixo `re:` marca expressão regular; os itens vazios são ignorados).
    pub fn novo(segredos: Vec<Zeroizing<String>>, extras: &[String]) -> Self {
        let mut segredos: Vec<Zeroizing<String>> = segredos
            .into_iter()
            .filter(|s| s.chars().count() >= TAMANHO_MINIMO)
            .collect();
        segredos.sort_by(|a, b| {
            b.len()
                .cmp(&a.len())
                .then_with(|| a.as_str().cmp(b.as_str()))
        });
        segredos.dedup_by(|a, b| a.as_str() == b.as_str());

        let mut extras_literais = Vec::new();
        let mut extras_regex = Vec::new();
        let mut invalidos = Vec::new();
        for extra in extras {
            let extra = extra.trim();
            if extra.is_empty() {
                continue;
            }
            match extra.strip_prefix(PREFIXO_REGEX) {
                Some(expressao) => match RegexBuilder::new(expressao.trim())
                    .size_limit(16 << 20)
                    .build()
                {
                    Ok(regex) => extras_regex.push(regex),
                    Err(erro) => invalidos.push(ExtraInvalido {
                        texto: extra.to_owned(),
                        erro: primeira_linha(&erro.to_string()),
                    }),
                },
                None => extras_literais.push(extra.to_owned()),
            }
        }
        extras_literais.sort_by_key(|e| std::cmp::Reverse(e.len()));
        Self {
            segredos,
            extras_literais,
            extras_regex,
            invalidos,
            desligado: false,
        }
    }

    /// Um redator que devolve o texto como está (`ia.redigir` desligado).
    pub fn desligado() -> Self {
        Self {
            desligado: true,
            ..Self::so_padroes()
        }
    }

    /// Um redator sem segredo do cofre e sem extras: só os padrões comuns.
    pub fn so_padroes() -> Self {
        Self::novo(Vec::new(), &[])
    }

    /// Os extras que não serviram (regex com erro).
    pub fn invalidos(&self) -> &[ExtraInvalido] {
        &self.invalidos
    }

    /// O texto sem os segredos.
    pub fn aplicar(&self, texto: &str) -> String {
        self.aplicar_contando(texto).0
    }

    /// O texto sem os segredos e quantos saíram de cada origem.
    pub fn aplicar_contando(&self, texto: &str) -> (String, Contagem) {
        let mut contagem = Contagem::default();
        if self.desligado {
            return (texto.to_owned(), contagem);
        }

        // 1. Os literais do cofre, de uma vez sobre o texto original: o marcador de um não é
        //    procurado pelo seguinte.
        let faixas = faixas_literais(texto, self.segredos.iter().map(|s| s.as_str()));
        contagem.cofre = faixas.len();
        let mut atual = trocar_faixas(texto, &faixas, MARCADOR);

        // 2. Os padrões comuns.
        for padrao in PADROES.iter() {
            let mut achados = 0usize;
            let tem_valor = padrao.regex.capture_names().flatten().any(|n| n == "valor");
            let trocado = padrao
                .regex
                .replace_all(&atual, |caps: &regex::Captures<'_>| {
                    let todo = caps.get(0).map_or("", |m| m.as_str());
                    let marcador = format!("[segredo removido: {}]", padrao.tipo);
                    match caps.name("valor") {
                        Some(valor) if tem_valor => {
                            if valor.as_str().starts_with("[segredo removido") {
                                return todo.to_owned();
                            }
                            achados += 1;
                            let inicio = valor.start() - caps.get(0).map_or(0, |m| m.start());
                            let fim = valor.end() - caps.get(0).map_or(0, |m| m.start());
                            format!("{}{}{}", &todo[..inicio], marcador, &todo[fim..])
                        }
                        _ => {
                            achados += 1;
                            marcador
                        }
                    }
                })
                .into_owned();
            if achados > 0 {
                contagem.padroes += achados;
                atual = trocado;
            }
        }

        // 3. Os extras.
        let faixas = faixas_literais(&atual, self.extras_literais.iter().map(String::as_str));
        if !faixas.is_empty() {
            contagem.extras += faixas.len();
            atual = trocar_faixas(&atual, &faixas, MARCADOR);
        }
        for regex in &self.extras_regex {
            let mut achados = 0usize;
            let trocado = regex
                .replace_all(&atual, |caps: &regex::Captures<'_>| {
                    // Um casamento vazio não esconde nada.
                    if caps.get(0).is_none_or(|m| m.as_str().is_empty()) {
                        return String::new();
                    }
                    achados += 1;
                    MARCADOR.to_owned()
                })
                .into_owned();
            if achados > 0 {
                contagem.extras += achados;
                atual = trocado;
            }
        }
        (atual, contagem)
    }
}

/// A primeira linha de uma mensagem de erro de regex (a que diz o motivo).
fn primeira_linha(erro: &str) -> String {
    erro.lines()
        .rev()
        .find_map(|linha| linha.strip_prefix("error: "))
        .unwrap_or_else(|| erro.lines().next().unwrap_or_default())
        .to_owned()
}

/// Onde cada literal aparece em `texto`, sem sobreposição: o que começa antes vence, e, no mesmo
/// ponto, o maior (os literais vêm do maior para o menor).
fn faixas_literais<'a>(texto: &str, literais: impl Iterator<Item = &'a str>) -> Vec<Range<usize>> {
    let mut faixas: Vec<Range<usize>> = Vec::new();
    for literal in literais {
        if literal.is_empty() {
            continue;
        }
        faixas.extend(
            texto
                .match_indices(literal)
                .map(|(inicio, achado)| inicio..inicio + achado.len()),
        );
    }
    faixas.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let mut unidas: Vec<Range<usize>> = Vec::with_capacity(faixas.len());
    for faixa in faixas {
        match unidas.last_mut() {
            Some(ultima) if faixa.start < ultima.end => {
                // Sobrepõe: o segredo que passa do fim do anterior estende a faixa, para nenhum
                // pedaço dele ficar à mostra.
                ultima.end = ultima.end.max(faixa.end);
            }
            _ => unidas.push(faixa),
        }
    }
    unidas
}

/// `texto` com cada faixa trocada pelo marcador.
fn trocar_faixas(texto: &str, faixas: &[Range<usize>], marcador: &str) -> String {
    if faixas.is_empty() {
        return texto.to_owned();
    }
    let mut saida = String::with_capacity(texto.len());
    let mut fim_anterior = 0;
    for faixa in faixas {
        saida.push_str(&texto[fim_anterior..faixa.start]);
        saida.push_str(marcador);
        fim_anterior = faixa.end;
    }
    saida.push_str(&texto[fim_anterior..]);
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    fn so_padroes(texto: &str) -> String {
        Redator::so_padroes().aplicar(texto)
    }

    fn segredo(texto: &str) -> Zeroizing<String> {
        Zeroizing::new(texto.to_owned())
    }

    #[test]
    fn os_padroes_compilam() {
        assert!(PADROES.len() >= 15);
    }

    #[test]
    fn chave_privada_openssh() {
        let texto = "antes\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk=\nAAAA\n-----END OPENSSH PRIVATE KEY-----\ndepois";
        assert_eq!(
            so_padroes(texto),
            "antes\n[segredo removido: chave privada]\ndepois"
        );
        let rsa = "-----BEGIN RSA PRIVATE KEY-----\nxx\n-----END RSA PRIVATE KEY-----";
        assert_eq!(so_padroes(rsa), "[segredo removido: chave privada]");
    }

    #[test]
    fn chave_privada_cortada_vai_ate_o_fim() {
        let texto = "ls\n-----BEGIN PRIVATE KEY-----\nMIIE\nAAAA";
        assert_eq!(so_padroes(texto), "ls\n[segredo removido: chave privada]");
    }

    #[test]
    fn chave_putty() {
        let texto = "PuTTY-User-Key-File-3: ssh-ed25519\nEncryption: none\nPrivate-Lines: 1\nAAAA\nPrivate-MAC: 0123abcd\nfim";
        assert_eq!(so_padroes(texto), "[segredo removido: chave PuTTY]\nfim");
    }

    #[test]
    fn chave_da_aws() {
        assert_eq!(
            so_padroes("id AKIAIOSFODNN7EXAMPLE ok"),
            "id [segredo removido: chave da AWS] ok"
        );
        assert_eq!(
            so_padroes("ASIAIOSFODNN7EXAMPLE"),
            "[segredo removido: chave da AWS]"
        );
        assert_eq!(
            so_padroes("aws_secret_access_key = wJalrXUtnFEMI/K7MDENG"),
            "aws_secret_access_key = [segredo removido: chave secreta da AWS]"
        );
    }

    #[test]
    fn tokens_de_servicos() {
        let casos = [
            (
                "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
                "token do GitHub",
            ),
            (
                "github_pat_11ABCDEFG0abcdefghijkl_mnopqrstuvwxyz",
                "token do GitHub",
            ),
            ("glpat-abcdefghijklmnopqrstu", "token do GitLab"),
            ("xoxb-1234567890-abcdefghij", "token do Slack"),
            ("sk-abcdefghijklmnopqrstuvwxyz", "chave de API"),
            ("sk-ant-api03-abcdefghijklmnopqrstuvwxyz", "chave de API"),
            ("AIzaSyA1234567890abcdefghijklmnopqrstuv", "chave do Google"),
        ];
        for (entrada, tipo) in casos {
            assert_eq!(
                so_padroes(&format!("x {entrada} y")),
                format!("x [segredo removido: {tipo}] y"),
                "{entrada}"
            );
        }
    }

    #[test]
    fn cabecalho_bearer_e_basic() {
        assert_eq!(
            so_padroes("Authorization: Bearer abc123def456ghi"),
            "Authorization: Bearer [segredo removido: autorização]"
        );
        assert_eq!(
            so_padroes("curl -H 'Authorization: Basic dXNlcjpwYXNz'"),
            "curl -H 'Authorization: Basic [segredo removido: autorização]'"
        );
    }

    #[test]
    fn atribuicoes_de_senha() {
        assert_eq!(
            so_padroes("export PASSWORD=Abc123xyz"),
            "export PASSWORD=[segredo removido: senha]"
        );
        assert_eq!(
            so_padroes("senha: \"minha senha\" fim"),
            "senha: [segredo removido: senha] fim"
        );
        assert_eq!(
            so_padroes("API_KEY='k3y'"),
            "API_KEY=[segredo removido: senha]"
        );
        assert_eq!(
            so_padroes("db --password hunter22 --host x"),
            "db --password [segredo removido: senha] --host x"
        );
        assert_eq!(
            so_padroes("token=abc passwd=def"),
            "token=[segredo removido: senha] passwd=[segredo removido: senha]"
        );
    }

    #[test]
    fn prompt_de_senha_nao_leva_a_linha_de_baixo() {
        assert_eq!(so_padroes("Password:\nlinha dois"), "Password:\nlinha dois");
        assert_eq!(so_padroes("token_type=bearer"), "token_type=bearer");
    }

    #[test]
    fn senha_numa_url() {
        assert_eq!(
            so_padroes("git clone https://ana:s3gredo@github.com/x/y"),
            "git clone https://ana:[segredo removido: senha em URL]@github.com/x/y"
        );
        assert_eq!(
            so_padroes("https://github.com:443/x"),
            "https://github.com:443/x"
        );
    }

    #[test]
    fn jwt() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.abcDEF123_-xyz";
        assert_eq!(
            so_padroes(&format!("t {jwt} f")),
            "t [segredo removido: JWT] f"
        );
    }

    #[test]
    fn texto_comum_passa_inteiro() {
        let texto = "root@web:~# ls -la\ntotal 8\ndrwxr-xr-x 2 root root 4096 jan 1 coração";
        assert_eq!(so_padroes(texto), texto);
        let (_, contagem) = Redator::so_padroes().aplicar_contando(texto);
        assert_eq!(contagem.total(), 0);
        assert_eq!(contagem.frase(), "");
    }

    #[test]
    fn segredo_literal_do_cofre() {
        let redator = Redator::novo(vec![segredo("Zq9!pLmN")], &[]);
        let (saida, contagem) = redator.aplicar_contando("a Zq9!pLmN b Zq9!pLmN");
        assert_eq!(saida, "a [segredo removido] b [segredo removido]");
        assert_eq!(contagem.cofre, 2);
    }

    #[test]
    fn senha_do_cofre_literal_com_ponto_e_asterisco() {
        // Sem regex: o `.` e o `*` da senha não viram curinga.
        let redator = Redator::novo(vec![segredo("a.b*c+d")], &[]);
        assert_eq!(
            redator.aplicar("a.b*c+d axbbbc+d a.b*c+d"),
            "[segredo removido] axbbbc+d [segredo removido]"
        );
    }

    #[test]
    fn desligada_devolve_igual() {
        let texto = "password=hunter22 AKIAABCDEFGHIJKLMNOP";
        let (saida, contagem) = Redator::desligado().aplicar_contando(texto);
        assert_eq!(saida, texto);
        assert_eq!(contagem.total(), 0);
    }

    #[test]
    fn redator_nao_aparece_no_debug() {
        let redator = Redator::novo(vec![segredo("Zq9!pLmN-segredo")], &["cliente-x".into()]);
        let debug = format!("{redator:?}");
        assert!(!debug.contains("Zq9"), "{debug}");
        assert!(!debug.contains("cliente-x"), "{debug}");
    }

    #[test]
    fn segredos_curtos_sao_ignorados() {
        let redator = Redator::novo(vec![segredo("abc"), segredo("a"), segredo("")], &[]);
        assert_eq!(redator.aplicar("abc a b"), "abc a b");
    }

    #[test]
    fn segredo_dentro_do_marcador_nao_quebra_o_texto() {
        // O segredo "removido" não pode comer o marcador do outro.
        let redator = Redator::novo(vec![segredo("Zq9!pLmN"), segredo("removido")], &[]);
        assert_eq!(
            redator.aplicar("Zq9!pLmN e removido"),
            "[segredo removido] e [segredo removido]"
        );
    }

    #[test]
    fn segredos_que_se_sobrepoem_saem_inteiros() {
        let redator = Redator::novo(vec![segredo("abcdef"), segredo("defghi")], &[]);
        assert_eq!(redator.aplicar("x abcdefghi y"), "x [segredo removido] y");
        let redator = Redator::novo(vec![segredo("senha-grande"), segredo("senha")], &[]);
        assert_eq!(redator.aplicar("senha-grande"), "[segredo removido]");
    }

    #[test]
    fn extras_literais_e_regex() {
        let extras = vec![
            "cliente-secreto".to_owned(),
            "re:ID-[0-9]{4}".to_owned(),
            "".to_owned(),
            "  ".to_owned(),
        ];
        let redator = Redator::novo(Vec::new(), &extras);
        let (saida, contagem) = redator.aplicar_contando("cliente-secreto ID-1234 e ID-12");
        assert_eq!(saida, "[segredo removido] [segredo removido] e ID-12");
        assert_eq!(contagem.extras, 2);
        assert!(redator.invalidos().is_empty());
    }

    #[test]
    fn extra_literal_curto_vale() {
        // O que a pessoa escreve de propósito vale, mesmo curto.
        let redator = Redator::novo(Vec::new(), &["xy".to_owned()]);
        assert_eq!(redator.aplicar("axyb"), "a[segredo removido]b");
    }

    #[test]
    fn regex_invalida_e_ignorada_e_acusada() {
        let redator = Redator::novo(Vec::new(), &["re:([".to_owned(), "ok-ok".to_owned()]);
        assert_eq!(redator.invalidos().len(), 1);
        assert_eq!(redator.invalidos()[0].texto, "re:([");
        assert!(!redator.invalidos()[0].erro.is_empty());
        assert_eq!(redator.aplicar("ok-ok"), "[segredo removido]");
    }

    #[test]
    fn regex_que_casa_vazio_nao_esconde_nada() {
        let redator = Redator::novo(Vec::new(), &["re:x*".to_owned()]);
        assert_eq!(redator.aplicar("abc"), "abc");
    }

    #[test]
    fn contagem_soma_as_origens() {
        let redator = Redator::novo(vec![segredo("Zq9!pLmN")], &["meu-token".to_owned()]);
        let (saida, contagem) =
            redator.aplicar_contando("Zq9!pLmN meu-token ghp_abcdefghijklmnopqrstuvwxyz0123456789");
        assert_eq!(
            saida,
            "[segredo removido] [segredo removido] [segredo removido: token do GitHub]"
        );
        assert_eq!(
            contagem,
            Contagem {
                cofre: 1,
                padroes: 1,
                extras: 1
            }
        );
        assert_eq!(contagem.total(), 3);
        assert!(contagem.frase().contains('3'));
    }

    #[test]
    fn valor_ja_removido_nao_conta_duas_vezes() {
        let redator = Redator::novo(vec![segredo("Zq9!pLmN")], &[]);
        let (saida, contagem) = redator.aplicar_contando("password=Zq9!pLmN");
        assert_eq!(saida, "password=[segredo removido]");
        assert_eq!(contagem.total(), 1);
    }
}
