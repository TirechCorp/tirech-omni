//! O que foi aberto por último, para a página Início: conexões (e, nos marcos seguintes,
//! espaços de trabalho e locais de arquivos). Fica no `recentes.json`, gravado a cada mudança
//! (a lista é pequena).

use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::armazenamento::Armazenamento;
use crate::t;

/// O que um recente abre.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Alvo {
    /// Uma conexão da árvore.
    Conexao {
        /// O id da conexão.
        id: u64,
    },
    /// Um espaço de trabalho (M28): o caminho relativo à pasta dos espaços.
    Espaco {
        /// O caminho.
        caminho: String,
    },
    /// Um lugar num navegador de arquivos (M30 em diante): a conexão e a pasta remota.
    Local {
        /// O id da conexão.
        conexao: u64,
        /// A pasta.
        caminho: String,
    },
    /// Um aplicativo gráfico remoto (M37): a conexão e o comando.
    Aplicativo {
        /// O id da conexão.
        conexao: u64,
        /// O comando que roda no servidor.
        comando: String,
    },
}

/// Um item da lista de recentes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recente {
    /// O que abre.
    pub alvo: Alvo,
    /// O nome na hora do uso.
    pub titulo: String,
    /// `user@host:porta`, a pasta, etc.
    pub detalhe: String,
    /// Segundos desde 1970 (UTC) do último uso.
    pub ultimo: u64,
    /// Quantas vezes foi usado.
    pub vezes: u32,
    /// Se a pessoa fixou o item no Início.
    #[serde(default)]
    pub fixado: bool,
}

/// A lista de recentes e o arquivo dela.
#[derive(Clone, Debug)]
pub struct Recentes {
    armazenamento: Armazenamento,
    itens: Vec<Recente>,
    limite: usize,
}

/// O limite de itens (fora os fixados) enquanto ninguém chamar [`Recentes::definir_limite`].
const LIMITE_PADRAO: usize = 30;

impl Recentes {
    /// Lê o `recentes.json`. Arquivo ausente dá a lista vazia; corrompido vai para `.bak` com o
    /// aviso.
    pub fn carregar(armazenamento: &Armazenamento) -> (Self, Option<String>) {
        let (itens, aviso): (Vec<Recente>, _) =
            armazenamento.carregar_json(&armazenamento.arquivo_recentes(), &t!("recentes-o-que"));
        let recentes = Self {
            armazenamento: armazenamento.clone(),
            itens,
            limite: LIMITE_PADRAO,
        };
        (recentes, aviso)
    }

    /// Quantos itens guardar, sem contar os fixados (`Configuracoes::limite_de_recentes`).
    /// Corta o que passar.
    pub fn definir_limite(&mut self, limite: usize) {
        self.limite = limite;
        if self.cortar() {
            self.gravar();
        }
    }

    /// Registra um uso: acha o item pelo `alvo`, atualiza `titulo`, `detalhe` e `ultimo`, soma
    /// `vezes` e o põe na frente; senão, insere.
    pub fn registrar(&mut self, alvo: Alvo, titulo: &str, detalhe: &str) {
        let ultimo = agora();
        let mut item = match self.itens.iter().position(|r| r.alvo == alvo) {
            Some(n) => self.itens.remove(n),
            None => Recente {
                alvo,
                titulo: String::new(),
                detalhe: String::new(),
                ultimo,
                vezes: 0,
                fixado: false,
            },
        };
        titulo.clone_into(&mut item.titulo);
        detalhe.clone_into(&mut item.detalhe);
        item.ultimo = ultimo;
        item.vezes = item.vezes.saturating_add(1);
        self.itens.insert(0, item);
        self.cortar();
        self.gravar();
    }

    /// Fixa ou desafixa um item.
    pub fn fixar(&mut self, alvo: &Alvo, fixado: bool) {
        if let Some(item) = self.itens.iter_mut().find(|r| &r.alvo == alvo)
            && item.fixado != fixado
        {
            item.fixado = fixado;
            // Desafixar pode fazer a lista passar do limite.
            self.cortar();
            self.gravar();
        }
    }

    /// Tira um item da lista.
    pub fn remover(&mut self, alvo: &Alvo) {
        let antes = self.itens.len();
        self.itens.retain(|r| &r.alvo != alvo);
        if self.itens.len() != antes {
            self.gravar();
        }
    }

    /// Tira o que `existe` recusa (as conexões apagadas da árvore).
    pub fn manter_se(&mut self, existe: impl Fn(&Alvo) -> bool) {
        let antes = self.itens.len();
        self.itens.retain(|r| existe(&r.alvo));
        if self.itens.len() != antes {
            self.gravar();
        }
    }

    /// Um espaço ou uma pasta de espaços mudou de caminho (`de` para `para`): os recentes de
    /// espaços que estavam nele passam a apontar para o caminho novo, mantendo o que foi
    /// fixado e o contador de usos. O título do espaço renomeado acompanha o nome novo.
    pub fn reapontar_espacos(&mut self, de: &str, para: &str) {
        let mut mudou = false;
        for item in &mut self.itens {
            let Alvo::Espaco { caminho } = &mut item.alvo else {
                continue;
            };
            let novo = if caminho == de {
                Some(para.to_owned())
            } else {
                caminho
                    .strip_prefix(&format!("{de}/"))
                    .map(|resto| format!("{para}/{resto}"))
            };
            if let Some(novo) = novo {
                if *caminho == de {
                    novo.rsplit_once('/')
                        .map_or(novo.as_str(), |(_, nome)| nome)
                        .clone_into(&mut item.titulo);
                }
                *caminho = novo;
                mudou = true;
            }
        }
        if mudou {
            self.gravar();
        }
    }

    /// Esvazia a lista, mantendo os fixados.
    pub fn limpar(&mut self) {
        self.itens.retain(|r| r.fixado);
        self.gravar();
    }

    /// Os itens, o mais recente primeiro (fixados e não fixados juntos).
    pub fn lista(&self) -> &[Recente] {
        &self.itens
    }

    /// O item de um alvo.
    pub fn item(&self, alvo: &Alvo) -> Option<&Recente> {
        self.itens.iter().find(|r| &r.alvo == alvo)
    }

    /// Corta os não fixados mais antigos que passarem do limite. Devolve se cortou algo.
    fn cortar(&mut self) -> bool {
        let mut vistos = 0;
        let antes = self.itens.len();
        let limite = self.limite;
        self.itens.retain(|r| {
            if r.fixado {
                return true;
            }
            vistos += 1;
            vistos <= limite
        });
        self.itens.len() != antes
    }

    fn gravar(&self) {
        // Uma falha ao gravar (disco cheio, pasta sem permissão) não pode derrubar o uso: a
        // lista continua certa em memória.
        let _ = self
            .armazenamento
            .gravar(&self.armazenamento.arquivo_recentes(), &self.itens);
    }
}

thread_local! {
    static RELOGIO: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Os segundos desde 1970 (UTC), ou o valor de [`fixar_relogio`] na thread.
pub fn agora() -> u64 {
    RELOGIO.with(Cell::get).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    })
}

/// Fixa o relógio da thread, para os testes (`None` volta ao relógio do sistema).
#[doc(hidden)]
pub fn fixar_relogio(segundos: Option<u64>) {
    RELOGIO.with(|r| r.set(segundos));
}

/// Há quanto tempo foi o `ultimo` uso, para o cartão: "agora mesmo", "há 5 min", "há 3 h",
/// "ontem", "há 4 dias" ou, depois de uma semana, a data (`dd/mm/aaaa` no pt-BR, `mm/dd/aaaa` no
/// en-US). Os textos vêm de `formatos.ftl`.
pub fn quando(ultimo: u64, agora: u64) -> String {
    const MIN: u64 = 60;
    const HORA: u64 = 60 * MIN;
    const DIA: u64 = 24 * HORA;
    let passou = agora.saturating_sub(ultimo);
    if passou < MIN {
        t!("inicio-quando-agora")
    } else if passou < HORA {
        t!("inicio-quando-min", n = passou / MIN)
    } else if passou < DIA {
        t!("inicio-quando-horas", n = passou / HORA)
    } else if passou < 2 * DIA {
        t!("inicio-quando-ontem")
    } else if passou < 7 * DIA {
        t!("inicio-quando-dias", n = passou / DIA)
    } else {
        data(ultimo)
    }
}

/// Os segundos desde 1970 como data (UTC): `dd/mm/aaaa` no pt-BR e `mm/dd/aaaa` no en-US.
pub fn data(segundos: u64) -> String {
    let (ano, mes, dia) = data_civil(segundos / 86_400);
    t!(
        "formatos-data",
        dia = format!("{dia:02}"),
        mes = format!("{mes:02}"),
        ano = format!("{ano:04}")
    )
}

/// Os segundos desde 1970 como data e hora (UTC, relógio de 24 horas).
pub fn data_e_hora(segundos: u64) -> String {
    let (ano, mes, dia) = data_civil(segundos / 86_400);
    let resto = segundos % 86_400;
    t!(
        "formatos-data-hora",
        dia = format!("{dia:02}"),
        mes = format!("{mes:02}"),
        ano = format!("{ano:04}"),
        hora = format!("{:02}", resto / 3_600),
        minuto = format!("{:02}", resto % 3_600 / 60)
    )
}

/// Dia, mês e ano (UTC) de um número de dias desde 1970-01-01 (algoritmo de Howard Hinnant).
pub(crate) fn data_civil(dias: u64) -> (u64, u64, u64) {
    let z = dias + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + u64::from(mes <= 2);
    (ano, mes, dia)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    fn conexao(id: u64) -> Alvo {
        Alvo::Conexao { id }
    }

    fn recentes(nome: &str) -> (Recentes, PastaTemporaria) {
        let pasta = PastaTemporaria::nova(nome);
        let (recentes, aviso) = Recentes::carregar(&Armazenamento::new(&pasta.0));
        assert_eq!(aviso, None);
        (recentes, pasta)
    }

    fn ids(r: &Recentes) -> Vec<u64> {
        r.lista()
            .iter()
            .map(|r| match r.alvo {
                Alvo::Conexao { id } => id,
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn reapontar_acompanha_o_espaco_e_a_pasta() {
        fixar_relogio(Some(1000));
        let (mut r, _p) = recentes("rec-reapontar");
        let espaco = |c: &str| Alvo::Espaco {
            caminho: c.to_owned(),
        };
        r.registrar(espaco("diário/manhã"), "manhã", "2 abas");
        r.registrar(espaco("diário/tarde"), "tarde", "1 aba");
        r.registrar(espaco("outro"), "outro", "1 aba");
        r.fixar(&espaco("diário/manhã"), true);

        r.reapontar_espacos("diário/manhã", "diário/cedo");
        assert!(r.item(&espaco("diário/manhã")).is_none());
        let cedo = r.item(&espaco("diário/cedo")).unwrap();
        assert_eq!(cedo.titulo, "cedo");
        assert!(cedo.fixado, "o que estava fixado continua fixado");

        r.reapontar_espacos("diário", "rotina");
        assert!(r.item(&espaco("rotina/cedo")).is_some());
        assert!(r.item(&espaco("rotina/tarde")).is_some());
        assert_eq!(r.item(&espaco("rotina/tarde")).unwrap().titulo, "tarde");
        assert!(r.item(&espaco("outro")).is_some(), "o resto fica como está");
    }

    #[test]
    fn registrar_soma_e_poe_na_frente() {
        fixar_relogio(Some(1000));
        let (mut r, _p) = recentes("rec-soma");
        r.registrar(conexao(1), "web", "ana@web:22");
        r.registrar(conexao(2), "db", "ana@db:22");
        fixar_relogio(Some(2000));
        r.registrar(conexao(1), "web-01", "ana@web:2222");
        assert_eq!(ids(&r), [1, 2]);
        let primeiro = &r.lista()[0];
        assert_eq!(primeiro.titulo, "web-01", "o nome na hora do uso");
        assert_eq!(primeiro.detalhe, "ana@web:2222");
        assert_eq!(primeiro.vezes, 2);
        assert_eq!(primeiro.ultimo, 2000);
        assert_eq!(r.lista()[1].ultimo, 1000);
        fixar_relogio(None);
    }

    #[test]
    fn o_limite_nao_conta_os_fixados() {
        let (mut r, _p) = recentes("rec-limite");
        r.definir_limite(2);
        r.registrar(conexao(1), "a", "");
        r.fixar(&conexao(1), true);
        for id in 2..=5 {
            r.registrar(conexao(id), "x", "");
        }
        // O fixado fica, mais os 2 mais recentes.
        assert_eq!(ids(&r), [5, 4, 1]);
        // Desafixar o devolve à regra: 4 e 5 já ocupam as duas vagas.
        r.fixar(&conexao(1), false);
        assert_eq!(ids(&r), [5, 4]);
    }

    #[test]
    fn limpar_mantem_os_fixados() {
        let (mut r, _p) = recentes("rec-limpar");
        r.registrar(conexao(1), "a", "");
        r.registrar(conexao(2), "b", "");
        r.fixar(&conexao(2), true);
        r.limpar();
        assert_eq!(ids(&r), [2]);
    }

    #[test]
    fn remover_e_manter_se() {
        let (mut r, _p) = recentes("rec-remover");
        for id in 1..=3 {
            r.registrar(conexao(id), "x", "");
        }
        r.remover(&conexao(2));
        assert_eq!(ids(&r), [3, 1]);
        r.manter_se(|a| *a == conexao(1));
        assert_eq!(ids(&r), [1]);
    }

    #[test]
    fn cada_mudanca_vai_para_o_arquivo() {
        let pasta = PastaTemporaria::nova("rec-arquivo");
        let armazenamento = Armazenamento::new(&pasta.0);
        let (mut r, _) = Recentes::carregar(&armazenamento);
        r.registrar(conexao(7), "web", "ana@web:22");
        r.registrar(
            Alvo::Local {
                conexao: 7,
                caminho: "/var/log".into(),
            },
            "log",
            "/var/log",
        );
        r.fixar(&conexao(7), true);
        let (volta, aviso) = Recentes::carregar(&armazenamento);
        assert_eq!(aviso, None);
        assert_eq!(volta.lista(), r.lista());
        assert!(volta.item(&conexao(7)).unwrap().fixado);
        let arquivos: Vec<_> = fs::read_dir(&pasta.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(arquivos, ["recentes.json"], "sem o .tmp");
    }

    #[test]
    fn arquivo_corrompido_vai_para_o_bak_com_aviso() {
        let pasta = PastaTemporaria::nova("rec-corrompido");
        let armazenamento = Armazenamento::new(&pasta.0);
        fs::create_dir_all(&pasta.0).unwrap();
        fs::write(armazenamento.arquivo_recentes(), "[ não é json").unwrap();
        let (r, aviso) = Recentes::carregar(&armazenamento);
        assert!(r.lista().is_empty());
        let aviso = aviso.expect("deve avisar");
        assert!(aviso.contains("recentes.json.bak"), "{aviso}");
        assert!(pasta.0.join("recentes.json.bak").exists());
    }

    #[test]
    fn o_relogio_fixado_vale_so_na_thread() {
        fixar_relogio(Some(42));
        assert_eq!(agora(), 42);
        fixar_relogio(None);
        assert!(agora() > 1_700_000_000);
    }

    #[test]
    fn quando_em_cada_faixa() {
        let agora = 10 * 86_400;
        let antes = |s: u64| quando(agora - s, agora);
        assert_eq!(antes(0), "agora mesmo");
        assert_eq!(antes(59), "agora mesmo");
        assert_eq!(antes(60), "há 1 min");
        assert_eq!(antes(5 * 60), "há 5 min");
        assert_eq!(antes(3599), "há 59 min");
        assert_eq!(antes(3600), "há 1 h");
        assert_eq!(antes(86_399), "há 23 h");
        assert_eq!(antes(86_400), "ontem");
        assert_eq!(antes(2 * 86_400 - 1), "ontem");
        assert_eq!(antes(2 * 86_400), "há 2 dias");
        assert_eq!(antes(6 * 86_400 + 5), "há 6 dias");
        // 1970-01-01 + 3 dias, a 7 dias de "agora" (dia 10): a data.
        assert_eq!(antes(7 * 86_400), "04/01/1970");
        // O futuro (relógio mudado) não quebra.
        assert_eq!(quando(agora + 100, agora), "agora mesmo");
    }

    #[test]
    fn a_data_civil_acerta_anos_bissextos() {
        assert_eq!(data_civil(0), (1970, 1, 1));
        // 2024-02-29 = 19782 dias desde 1970.
        assert_eq!(data_civil(19_782), (2024, 2, 29));
        // 2026-10-18 = 20_744 dias.
        assert_eq!(data_civil(20_744), (2026, 10, 18));
    }
}
