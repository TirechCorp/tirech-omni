//! Como a página Arquivos escreve tamanhos, datas e permissões.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tomni_ui::files::{EntryKind, Eta};

use crate::i18n;
use crate::recentes::data_e_hora;
use crate::t;

/// O tamanho em bytes como `512 B`, `1,2 KB` ou `3,4 MB`: unidades de 1024, uma casa e o
/// separador decimal do idioma (vírgula no pt-BR, ponto no en-US).
pub fn tamanho(bytes: u64) -> String {
    const UNIDADES: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut valor = bytes as f64;
    let mut unidade = 0;
    while valor >= 1024.0 && unidade < UNIDADES.len() - 1 {
        valor /= 1024.0;
        unidade += 1;
    }
    format!("{} {}", i18n::decimal(valor, 1), UNIDADES[unidade])
}

/// A data como `dd/mm/aaaa hh:mm` (`mm/dd/aaaa` no en-US), em UTC (o app não conhece o fuso do
/// sistema); vazio se o protocolo não disse.
pub fn data(momento: Option<SystemTime>) -> String {
    let Some(momento) = momento else {
        return String::new();
    };
    let Ok(desde_1970) = momento.duration_since(UNIX_EPOCH) else {
        return String::new();
    };
    data_e_hora(desde_1970.as_secs())
}

/// As nove letras das permissões (`rwxr-xr-x`), com `s`/`S` e `t`/`T` nos bits especiais.
pub fn permissoes(modo: u32) -> String {
    let mut texto = String::with_capacity(9);
    for (i, (r, w, x)) in [(8, 7, 6), (5, 4, 3), (2, 1, 0)].into_iter().enumerate() {
        let especial = match i {
            0 => modo & 0o4000 != 0,
            1 => modo & 0o2000 != 0,
            _ => modo & 0o1000 != 0,
        };
        let executa = modo & (1 << x) != 0;
        texto.push(if modo & (1 << r) != 0 { 'r' } else { '-' });
        texto.push(if modo & (1 << w) != 0 { 'w' } else { '-' });
        texto.push(match (especial, executa, i) {
            (true, true, 2) => 't',
            (true, false, 2) => 'T',
            (true, true, _) => 's',
            (true, false, _) => 'S',
            (false, true, _) => 'x',
            (false, false, _) => '-',
        });
    }
    texto
}

/// O modo em octal (`755`; `4755` com bits especiais).
pub fn octal(modo: u32) -> String {
    let modo = modo & 0o7777;
    if modo > 0o777 {
        format!("{modo:04o}")
    } else {
        format!("{modo:03o}")
    }
}

/// Lê um modo em octal (de três ou quatro dígitos).
pub fn de_octal(texto: &str) -> Option<u32> {
    let texto = texto.trim();
    if !(3..=4).contains(&texto.len()) {
        return None;
    }
    u32::from_str_radix(texto, 8).ok().filter(|m| *m <= 0o7777)
}

/// A letra do tipo (`d`, `l`, `-`) que vai antes das permissões na lista.
pub fn letra_do_tipo(tipo: EntryKind) -> char {
    match tipo {
        EntryKind::Dir => 'd',
        EntryKind::Symlink => 'l',
        EntryKind::File => '-',
        EntryKind::Other => '?',
    }
}

/// A velocidade como `1,2 MB/s`; vazia se parou.
pub fn velocidade(bytes_por_segundo: f64) -> String {
    if bytes_por_segundo < 1.0 {
        return String::new();
    }
    format!("{}/s", tamanho(bytes_por_segundo as u64))
}

/// A unidade de um tempo restante.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unidade {
    Segundos,
    Minutos,
    Horas,
    Dias,
}

impl Unidade {
    /// Quantos segundos tem uma unidade.
    fn segundos(self) -> f64 {
        match self {
            Unidade::Segundos => 1.0,
            Unidade::Minutos => 60.0,
            Unidade::Horas => 3_600.0,
            Unidade::Dias => 86_400.0,
        }
    }

    /// O passo do arredondamento, na própria unidade: 5 s, e 1 das demais.
    fn passo(self) -> f64 {
        if self == Unidade::Segundos { 5.0 } else { 1.0 }
    }

    /// Como a unidade se escreve junto de `n`.
    fn texto(self, n: u64) -> String {
        match self {
            Unidade::Segundos => "s".to_owned(),
            Unidade::Minutos => "min".to_owned(),
            Unidade::Horas => "h".to_owned(),
            Unidade::Dias => t!("formato-unidade-dias", n = n as i64),
        }
    }
}

/// Um tempo restante já arredondado: o que a tela mostra. `de == ate` lê-se "cerca de"; os dois
/// em zero, "alguns segundos". Os números estão na `unidade` (em segundos, de 5 em 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Faixa {
    pub de: u64,
    pub ate: u64,
    pub unidade: Unidade,
}

impl Faixa {
    /// Os limites em segundos, para a histerese.
    fn limites(&self) -> (f64, f64) {
        let por_unidade = self.unidade.segundos();
        if self.de == 0 && self.ate == 0 {
            // "Alguns segundos" vai até os 10 s do corte.
            return (0.0, 10.0);
        }
        (self.de as f64 * por_unidade, self.ate as f64 * por_unidade)
    }
}

/// Quanto o fim da faixa pode passar do começo (em razão) para ainda se dizer "cerca de".
const RAZAO_DE_CERCA: f64 = 1.5;

/// Arredonda o tempo restante para o que se mostra: a unidade sai do fim da faixa (`high`), o
/// começo arredonda ao passo mais próximo (nunca abaixo de um passo) e o fim para cima. Faixa
/// estreita vira "cerca de" o provável.
pub fn faixa(eta: &Eta) -> Faixa {
    let (lo, mi, hi) = (
        eta.low.as_secs_f64(),
        eta.likely.as_secs_f64(),
        eta.high.as_secs_f64(),
    );
    if hi < 10.0 {
        return Faixa {
            de: 0,
            ate: 0,
            unidade: Unidade::Segundos,
        };
    }
    let unidade = match hi {
        h if h < 60.0 => Unidade::Segundos,
        h if h < 3_600.0 => Unidade::Minutos,
        h if h < 86_400.0 => Unidade::Horas,
        _ => Unidade::Dias,
    };
    let passo = unidade.passo() * unidade.segundos();
    let em_passos = |segundos: f64| segundos / passo;
    let visivel = |passos: f64| (passos * unidade.passo()).round() as u64;
    let de_passos = em_passos(lo).round().max(1.0);
    let ate_passos = em_passos(hi).ceil().max(de_passos);
    let estreita = ate_passos <= de_passos || hi <= lo * RAZAO_DE_CERCA;
    if estreita {
        let cerca = visivel(em_passos(mi).round().max(1.0));
        return Faixa {
            de: cerca,
            ate: cerca,
            unidade,
        };
    }
    Faixa {
        de: visivel(de_passos),
        ate: visivel(ate_passos),
        unidade,
    }
}

/// A faixa sem o verbo: "3–5 min", "cerca de 40 s", "alguns segundos".
pub fn corpo_da_faixa(f: &Faixa) -> String {
    if f.de == 0 && f.ate == 0 {
        t!("formato-faixa-alguns-segundos")
    } else if f.de == f.ate {
        t!(
            "formato-faixa-cerca",
            n = f.de as i64,
            unidade = f.unidade.texto(f.de)
        )
    } else {
        t!(
            "formato-faixa-intervalo",
            de = f.de as i64,
            ate = f.ate as i64,
            unidade = f.unidade.texto(f.ate)
        )
    }
}

/// A faixa como frase: "Faltam 3–5 min", "Falta cerca de 1 min", "Faltam alguns segundos".
pub fn texto_da_faixa(f: &Faixa) -> String {
    let corpo = corpo_da_faixa(f);
    if f.de == f.ate && f.de == 1 {
        t!("formato-falta", corpo = corpo)
    } else {
        t!("formato-faltam", corpo = corpo)
    }
}

/// Uma duração curta e exata: "12 s", "3 min", "2 h", "1 dia".
pub fn duracao_curta(d: Duration) -> String {
    let segundos = d.as_secs();
    match segundos {
        0..=59 => format!("{segundos} s"),
        60..=3_599 => format!("{} min", segundos / 60),
        3_600..=86_399 => format!("{} h", segundos / 3_600),
        _ => {
            let dias = segundos / 86_400;
            format!("{dias} {}", Unidade::Dias.texto(dias))
        }
    }
}

/// Quanto tempo uma faixa nova precisa se manter antes de trocar a que está na tela.
const ESPERA_DA_TROCA: Duration = Duration::from_secs(3);

/// A faixa que está na tela e a candidata a trocá-la: o texto do tempo restante não pode tremer
/// a cada leitura.
#[derive(Clone, Debug, Default)]
pub struct Estavel {
    /// O que se mostra, com os limites em segundos.
    mostrada: Option<(Faixa, f64, f64)>,
    /// A faixa nova que está esperando, e desde quando.
    candidata: Option<(Faixa, Duration)>,
}

/// A faixa a mostrar agora. Uma faixa nova só entra na tela se não havia nada, se o provável
/// saiu claramente da faixa mostrada (15% para fora), ou se ela se manteve por 3 s (`agora` é o
/// relógio da fila, o `at` do resumo).
pub fn estabilizar(e: &mut Estavel, eta: &Eta, agora: Duration) -> Faixa {
    let nova = faixa(eta);
    let provavel = eta.likely.as_secs_f64();
    let trocar = match &e.mostrada {
        None => true,
        Some((_, de_s, ate_s)) if provavel < de_s * 0.85 || provavel > ate_s * 1.15 => true,
        Some((mostrada, _, _)) if *mostrada == nova => {
            e.candidata = None;
            false
        }
        Some(_) => match &e.candidata {
            Some((candidata, desde)) if *candidata == nova => {
                agora.saturating_sub(*desde) >= ESPERA_DA_TROCA
            }
            _ => {
                e.candidata = Some((nova.clone(), agora));
                false
            }
        },
    };
    if trocar {
        let (de_s, ate_s) = nova.limites();
        e.mostrada = Some((nova.clone(), de_s, ate_s));
        e.candidata = None;
        return nova;
    }
    e.mostrada
        .as_ref()
        .map(|(f, _, _)| f.clone())
        .unwrap_or(nova)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn tamanho_usa_unidades_de_1024_e_a_virgula() {
        assert_eq!(tamanho(0), "0 B");
        assert_eq!(tamanho(1023), "1023 B");
        assert_eq!(tamanho(1024), "1,0 KB");
        assert_eq!(tamanho(1_258_291), "1,2 MB");
        assert_eq!(tamanho(5 * 1024 * 1024 * 1024), "5,0 GB");
    }

    #[test]
    fn data_vem_em_utc_no_formato_brasileiro() {
        let momento = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(data(Some(momento)), "14/11/2023 22:13");
        assert_eq!(data(None), "");
    }

    #[test]
    fn permissoes_escrevem_as_nove_letras() {
        assert_eq!(permissoes(0o755), "rwxr-xr-x");
        assert_eq!(permissoes(0o644), "rw-r--r--");
        assert_eq!(permissoes(0o4755), "rwsr-xr-x");
        assert_eq!(permissoes(0o1777), "rwxrwxrwt");
        assert_eq!(permissoes(0o1666), "rw-rw-rwT");
    }

    #[test]
    fn octal_vai_e_volta() {
        assert_eq!(octal(0o755), "755");
        assert_eq!(octal(0o4755), "4755");
        assert_eq!(octal(0o7), "007");
        assert_eq!(de_octal("644"), Some(0o644));
        assert_eq!(de_octal(" 1777 "), Some(0o1777));
        assert_eq!(de_octal("888"), None);
        assert_eq!(de_octal("64"), None);
    }

    #[test]
    fn velocidade_some_quando_parou() {
        assert_eq!(velocidade(0.0), "");
        assert_eq!(velocidade(2.0 * 1024.0 * 1024.0), "2,0 MB/s");
    }

    fn eta(baixo: f64, provavel: f64, alto: f64) -> Eta {
        Eta {
            low: Duration::from_secs_f64(baixo),
            likely: Duration::from_secs_f64(provavel),
            high: Duration::from_secs_f64(alto),
        }
    }

    fn frase(baixo: f64, provavel: f64, alto: f64) -> String {
        texto_da_faixa(&faixa(&eta(baixo, provavel, alto)))
    }

    #[test]
    fn a_faixa_arredonda_para_o_que_se_le() {
        assert_eq!(frase(3.0, 5.0, 8.0), "Faltam alguns segundos");
        assert_eq!(frase(35.0, 40.0, 44.0), "Faltam cerca de 40 s");
        assert_eq!(frase(170.0, 230.0, 290.0), "Faltam 3–5 min");
        assert_eq!(frase(55.0, 70.0, 100.0), "Faltam 1–2 min");
        assert_eq!(frase(50.0, 60.0, 70.0), "Falta cerca de 1 min");
        assert_eq!(frase(3_000.0, 4_000.0, 9_000.0), "Faltam 1–3 h");
        let dia = 86_400.0;
        assert_eq!(frase(2.0 * dia, 3.0 * dia, 4.0 * dia), "Faltam 2–4 dias");
    }

    #[test]
    fn a_faixa_em_segundos_anda_de_cinco_em_cinco() {
        let f = faixa(&eta(12.0, 25.0, 52.0));
        assert_eq!(f.unidade, Unidade::Segundos);
        assert_eq!((f.de, f.ate), (10, 55));
        assert_eq!(corpo_da_faixa(&f), "10–55 s");
    }

    #[test]
    fn o_corpo_da_faixa_nao_tem_o_verbo() {
        assert_eq!(corpo_da_faixa(&faixa(&eta(170.0, 230.0, 290.0))), "3–5 min");
        assert_eq!(
            corpo_da_faixa(&faixa(&eta(35.0, 40.0, 44.0))),
            "cerca de 40 s"
        );
        assert_eq!(
            corpo_da_faixa(&faixa(&eta(2.0, 4.0, 6.0))),
            "alguns segundos"
        );
        let dia = 86_400.0;
        assert_eq!(
            corpo_da_faixa(&faixa(&eta(dia, dia, dia))),
            "cerca de 1 dia"
        );
    }

    #[test]
    fn a_faixa_nunca_tem_o_fim_antes_do_comeco() {
        for baixo in [1.0, 9.0, 40.0, 61.0, 590.0, 5_000.0, 200_000.0] {
            for fator in [1.0_f64, 1.2, 1.6, 3.0, 9.0] {
                let f = faixa(&eta(baixo, baixo * fator.sqrt(), baixo * fator));
                assert!(f.de <= f.ate, "{f:?}");
                if f.ate > 0 {
                    assert!(f.de >= 1, "{f:?}");
                }
            }
        }
    }

    #[test]
    fn a_duracao_curta_e_exata() {
        assert_eq!(duracao_curta(Duration::from_secs(12)), "12 s");
        assert_eq!(duracao_curta(Duration::from_secs(185)), "3 min");
        assert_eq!(duracao_curta(Duration::from_secs(7_300)), "2 h");
        assert_eq!(duracao_curta(Duration::from_secs(86_400)), "1 dia");
    }

    #[test]
    fn oscilar_cinco_por_cento_nao_troca_o_texto() {
        let mut e = Estavel::default();
        let primeira = estabilizar(&mut e, &eta(180.0, 240.0, 300.0), Duration::ZERO);
        assert_eq!(texto_da_faixa(&primeira), "Faltam 3–5 min");
        for passo in 1..=80_u32 {
            let fator = if passo % 2 == 0 { 1.05 } else { 0.95 };
            let agora = Duration::from_millis(250 * u64::from(passo));
            let f = estabilizar(
                &mut e,
                &eta(180.0 * fator, 240.0 * fator, 300.0 * fator),
                agora,
            );
            assert_eq!(f, primeira, "no passo {passo}");
        }
    }

    #[test]
    fn cair_para_um_minuto_troca_na_hora() {
        let mut e = Estavel::default();
        estabilizar(&mut e, &eta(180.0, 240.0, 300.0), Duration::ZERO);
        let f = estabilizar(&mut e, &eta(50.0, 60.0, 70.0), Duration::from_millis(250));
        assert_eq!(texto_da_faixa(&f), "Falta cerca de 1 min");
    }

    #[test]
    fn uma_faixa_nova_dentro_da_margem_so_troca_depois_de_tres_segundos() {
        let mut e = Estavel::default();
        let antes = estabilizar(&mut e, &eta(180.0, 240.0, 300.0), Duration::ZERO);
        // O provável continua dentro da faixa mostrada, mas a faixa arredondada é outra (3–6).
        let nova = eta(180.0, 250.0, 330.0);
        assert_ne!(faixa(&nova), antes);
        let mut agora = Duration::from_secs(10);
        assert_eq!(estabilizar(&mut e, &nova, agora), antes);
        agora += Duration::from_millis(2_900);
        assert_eq!(estabilizar(&mut e, &nova, agora), antes);
        agora += Duration::from_millis(200);
        assert_eq!(estabilizar(&mut e, &nova, agora), faixa(&nova));
    }

    #[test]
    fn a_candidata_recomeca_quando_a_faixa_nova_muda() {
        let mut e = Estavel::default();
        let antes = estabilizar(&mut e, &eta(180.0, 240.0, 300.0), Duration::ZERO);
        let (a, b) = (eta(180.0, 250.0, 330.0), eta(180.0, 250.0, 390.0));
        assert_ne!(faixa(&a), faixa(&b));
        for (i, e_) in [&a, &b, &a, &b, &a].into_iter().enumerate() {
            let agora = Duration::from_secs(10 + 2 * i as u64);
            assert_eq!(estabilizar(&mut e, e_, agora), antes, "leitura {i}");
        }
    }
}
