//! O rclone no app (M33): onde está o binário, os processos `rcd` (um por proxy) e as opções
//! que a pessoa pode trocar. A tela e o `Opener` falam com o [`Rcds`]; quem sobe o processo é
//! uma fábrica, que os testes trocam pelo `rcd` falso.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tomni_ui::files::rclone::{Rcd, RcdConfig, Rclone, RcloneError, find_rclone};
use tomni_ui::files::{Fs, Opener as ArquivosOpener};

use crate::configuracoes::RcloneCfg;

/// O que a pessoa escolhe sobre o rclone (`rclone.binario` e `rclone.arquivo_config`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpcoesRclone {
    /// O caminho do binário; vazio procura `rclone` no `PATH`.
    pub binario: String,
    /// O arquivo de configuração (`rclone.conf`); vazio usa o padrão do rclone.
    pub config: String,
}

impl OpcoesRclone {
    /// As opções de `config`.
    pub fn da_config(config: &RcloneCfg) -> Self {
        Self {
            binario: config.binario.trim().to_owned(),
            config: config.arquivo_config.trim().to_owned(),
        }
    }

    /// O binário escolhido, se há um.
    pub fn binario(&self) -> Option<PathBuf> {
        let texto = self.binario.trim();
        (!texto.is_empty()).then(|| PathBuf::from(texto))
    }

    /// O arquivo de configuração escolhido, se há um.
    pub fn config(&self) -> Option<PathBuf> {
        let texto = self.config.trim();
        (!texto.is_empty()).then(|| PathBuf::from(texto))
    }
}

/// Como subir um `rcd`: as opções, o proxy e a senha da configuração cifrada.
pub type Fabrica = Arc<
    dyn Fn(&OpcoesRclone, Option<&str>, Option<&str>) -> Result<Arc<Rcd>, RcloneError>
        + Send
        + Sync,
>;

/// A fábrica de verdade: procura o binário e sobe o `rclone rcd`.
pub fn fabrica_real() -> Fabrica {
    Arc::new(|opcoes, proxy, senha| {
        let binario = find_rclone(opcoes.binario().as_deref()).ok_or(RcloneError::NotFound)?;
        let mut config = RcdConfig::new(binario);
        config.config_file = opcoes.config();
        config.config_password = senha.map(str::to_owned);
        config.proxy = proxy.map(str::to_owned);
        Rcd::spawn(config)
    })
}

/// Os processos `rcd` do app: um por proxy, subidos quando alguém precisa (subir leva até
/// 10 s, então quem chama está numa thread e nunca na da tela).
pub struct Rcds {
    fabrica: Fabrica,
    opcoes: Mutex<OpcoesRclone>,
    senha: Mutex<Option<String>>,
    ativos: Mutex<HashMap<Option<String>, Arc<Rcd>>>,
}

impl Rcds {
    /// Os `rcd` de verdade, com as `opcoes`.
    pub fn real(opcoes: OpcoesRclone) -> Arc<Rcds> {
        Self::com_fabrica(opcoes, fabrica_real())
    }

    /// Com uma `fabrica` própria (os testes).
    pub fn com_fabrica(opcoes: OpcoesRclone, fabrica: Fabrica) -> Arc<Rcds> {
        Arc::new(Rcds {
            fabrica,
            opcoes: Mutex::new(opcoes),
            senha: Mutex::new(None),
            ativos: Mutex::new(HashMap::new()),
        })
    }

    fn trava<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// As opções de agora.
    pub fn opcoes(&self) -> OpcoesRclone {
        Self::trava(&self.opcoes).clone()
    }

    /// Troca as opções e derruba os processos (o próximo pedido sobe outro com elas).
    pub fn definir_opcoes(&self, opcoes: OpcoesRclone) {
        *Self::trava(&self.opcoes) = opcoes;
        Self::trava(&self.ativos).clear();
    }

    /// Guarda a senha da configuração cifrada (do cofre ou do diálogo) para os próximos
    /// processos.
    pub fn definir_senha(&self, senha: Option<String>) {
        *Self::trava(&self.senha) = senha;
        Self::trava(&self.ativos).clear();
    }

    /// `true` se já há uma senha de configuração guardada.
    pub fn tem_senha(&self) -> bool {
        Self::trava(&self.senha).is_some()
    }

    /// O `rcd` do `proxy` (`None`: sem proxy), subindo um se ainda não há. Bloqueia.
    pub fn obter(&self, proxy: Option<&str>) -> Result<Arc<Rcd>, RcloneError> {
        let chave = proxy.map(str::to_owned);
        if let Some(rcd) = Self::trava(&self.ativos).get(&chave) {
            return Ok(rcd.clone());
        }
        let opcoes = self.opcoes();
        let senha = Self::trava(&self.senha).clone();
        let rcd = (self.fabrica)(&opcoes, proxy, senha.as_deref())?;
        Self::trava(&self.ativos)
            .entry(chave)
            .or_insert_with(|| rcd.clone());
        Ok(rcd)
    }

    /// Quantos processos estão de pé.
    pub fn ativos(&self) -> usize {
        Self::trava(&self.ativos).len()
    }

    /// Encerra todos os processos (ao fechar o app).
    pub fn encerrar(&self) {
        for (_, rcd) in Self::trava(&self.ativos).drain() {
            rcd.shutdown();
        }
    }
}

impl Drop for Rcds {
    fn drop(&mut self) {
        self.encerrar();
    }
}

/// Quem abre o remoto `spec` (`gdrive:` ou `gdrive:Backup`) pelo `rcd` de `rcds`. Cada chamada
/// pode subir o processo (até 10 s), e a página Arquivos chama de threads dela. `proxy` é o do
/// perfil de rede da conexão (M34): cada proxy tem o seu `rcd`.
pub fn abridor(rcds: Arc<Rcds>, spec: String, proxy: Option<String>) -> ArquivosOpener {
    Arc::new(move || {
        let rcd = rcds.obter(proxy.as_deref()).map_err(std::io::Error::from)?;
        Ok(Box::new(Rclone::new(rcd, &spec)) as Box<dyn Fs>)
    })
}

/// O que a tela faz com cada erro do rclone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Falha {
    /// O binário não existe: o `InfoBar` de instalação.
    Ausente(String),
    /// A versão é antiga demais.
    Antigo(String),
    /// A configuração é cifrada: a tela pede a senha.
    SenhaDaConfig(String),
    /// Qualquer outra coisa: o texto vai num aviso.
    Outra(String),
}

impl Falha {
    /// Classifica o erro.
    pub fn de(erro: &RcloneError) -> Falha {
        let texto = erro.to_string();
        match erro {
            RcloneError::NotFound => Falha::Ausente(texto),
            RcloneError::TooOld(_) => Falha::Antigo(texto),
            RcloneError::ConfigPassword => Falha::SenhaDaConfig(texto),
            _ => Falha::Outra(texto),
        }
    }

    /// O texto do erro.
    pub fn texto(&self) -> &str {
        match self {
            Falha::Ausente(t) | Falha::Antigo(t) | Falha::SenhaDaConfig(t) | Falha::Outra(t) => t,
        }
    }

    /// `true` se o `InfoBar` de "instale o rclone" cobre o caso (binário ausente ou antigo).
    pub fn de_instalacao(&self) -> bool {
        matches!(self, Falha::Ausente(_) | Falha::Antigo(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn opcoes_vazias_nao_escolhem_nada() {
        let o = OpcoesRclone::default();
        assert!(o.binario().is_none() && o.config().is_none());
        let o = OpcoesRclone {
            binario: " /usr/bin/rclone ".into(),
            config: String::new(),
        };
        assert_eq!(o.binario(), Some(PathBuf::from("/usr/bin/rclone")));
    }

    #[test]
    fn um_processo_por_proxy_e_reaproveitado() {
        let subidas = Arc::new(AtomicUsize::new(0));
        let contagem = subidas.clone();
        let rcds = Rcds::com_fabrica(
            OpcoesRclone::default(),
            Arc::new(move |_, _, _| {
                contagem.fetch_add(1, Ordering::SeqCst);
                Ok(Rcd::attach("http://127.0.0.1:1", "u", "s"))
            }),
        );
        rcds.obter(None).unwrap();
        rcds.obter(None).unwrap();
        rcds.obter(Some("socks5://127.0.0.1:1081")).unwrap();
        assert_eq!(subidas.load(Ordering::SeqCst), 2);
        assert_eq!(rcds.ativos(), 2);
        rcds.definir_senha(Some("x".into()));
        assert_eq!(rcds.ativos(), 0);
    }

    #[test]
    fn a_falha_separa_instalacao_de_resto() {
        assert!(Falha::de(&RcloneError::NotFound).de_instalacao());
        assert!(Falha::de(&RcloneError::TooOld((1, 50, 0))).de_instalacao());
        assert!(!Falha::de(&RcloneError::ConfigPassword).de_instalacao());
        assert!(matches!(
            Falha::de(&RcloneError::ConfigPassword),
            Falha::SenhaDaConfig(_)
        ));
    }
}
