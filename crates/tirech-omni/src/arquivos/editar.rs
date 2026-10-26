//! Editar um arquivo remoto no editor externo: baixa para a pasta temporária, abre e vigia a
//! data do arquivo; quando muda, a tela pergunta se envia de volta.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use tomni_ui::files::Live;

use super::Aba;

/// De quanto em quanto tempo o vigia olha a data dos arquivos.
const INTERVALO_DO_VIGIA: Duration = Duration::from_secs(1);

/// Um arquivo remoto aberto no editor.
pub struct Edicao {
    /// O número da edição (o que o vigia devolve).
    pub id: u64,
    /// A aba de onde o arquivo veio, para enviar de volta pela mesma conexão.
    pub aba: Aba,
    /// O caminho no servidor.
    pub remoto: String,
    /// A cópia na pasta temporária.
    pub local: PathBuf,
    /// A transferência que baixa o arquivo, enquanto não terminou.
    pub baixando: Option<u64>,
    /// Se as mudanças seguem para o servidor sem perguntar.
    pub sempre: bool,
}

impl Edicao {
    /// O nome do arquivo, para as mensagens.
    pub fn nome(&self) -> &str {
        tomni_ui::files::path::file_name(&self.remoto)
    }
}

/// Onde fica a cópia de `remoto` da conexão `conexao`: `<temporária>/tirech-omni/<conexão>/<caminho>`,
/// com a `temporaria` do [`Ambiente`](crate::ferramentas::Ambiente::pasta_temporaria). Cada
/// servidor tem a sua pasta, e o caminho remoto inteiro se mantém para dois arquivos de mesmo
/// nome não se misturarem.
pub fn pasta_temporaria(temporaria: &Path, conexao: Option<u64>, remoto: &str) -> PathBuf {
    let dono = conexao.map_or_else(|| "sessao".to_owned(), |id| id.to_string());
    let mut caminho = temporaria.join("tirech-omni").join(dono);
    for parte in remoto
        .split('/')
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
    {
        caminho.push(parte);
    }
    caminho
}

/// O caminho em texto, com `/`, como o `Local` do `tomni-ui-files` espera.
pub fn caminho_em_texto(caminho: &Path) -> String {
    caminho.to_string_lossy().replace('\\', "/")
}

/// Um arquivo que o vigia olha.
struct Alvo {
    id: u64,
    arquivo: PathBuf,
    visto: Option<SystemTime>,
}

/// A thread que olha, a cada segundo, a data dos arquivos abertos no editor (sem crate de
/// vigia). Nasce no primeiro arquivo e acaba quando o `Vigia` é descartado.
pub struct Vigia {
    alvos: Arc<Mutex<Vec<Alvo>>>,
    alterados: Live<Vec<u64>>,
    parar: Arc<AtomicBool>,
    rodando: std::cell::Cell<bool>,
}

impl Vigia {
    /// Um vigia sem arquivos e sem thread.
    pub fn novo() -> Self {
        Self {
            alvos: Arc::default(),
            alterados: Live::new(Vec::new()),
            parar: Arc::default(),
            rodando: std::cell::Cell::new(false),
        }
    }

    /// Os números das edições cujo arquivo mudou desde a última vez; quem lê esvazia a lista
    /// com [`Live::set`].
    pub fn alterados(&self) -> Live<Vec<u64>> {
        self.alterados.clone()
    }

    /// Passa a vigiar `arquivo`, que agora tem a data `visto`.
    pub fn observar(&self, id: u64, arquivo: PathBuf, visto: Option<SystemTime>) {
        self.alvos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Alvo { id, arquivo, visto });
        if !self.rodando.replace(true) {
            self.iniciar();
        }
    }

    /// Para de vigiar a edição `id`.
    pub fn esquecer(&self, id: u64) {
        self.alvos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|a| a.id != id);
    }

    fn iniciar(&self) {
        let (alvos, alterados, parar) = (
            Arc::clone(&self.alvos),
            self.alterados.clone(),
            Arc::clone(&self.parar),
        );
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(INTERVALO_DO_VIGIA);
                if parar.load(Ordering::Relaxed) {
                    return;
                }
                let mudaram: Vec<u64> = alvos
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .iter_mut()
                    .filter_map(|alvo| {
                        let agora = data_de(&alvo.arquivo);
                        (agora != alvo.visto).then(|| {
                            alvo.visto = agora;
                            alvo.id
                        })
                    })
                    .collect();
                if !mudaram.is_empty() {
                    alterados.update(|lista| lista.extend(mudaram));
                }
            }
        });
    }
}

impl Drop for Vigia {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::Relaxed);
    }
}

/// A data da última modificação do arquivo, se ele existe.
pub fn data_de(arquivo: &Path) -> Option<SystemTime> {
    std::fs::metadata(arquivo).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_pasta_temporaria_guarda_o_caminho_inteiro_e_ignora_o_que_sobe() {
        let temporaria = Path::new("/media/ana/tirech-omnidata/temp");
        let a = pasta_temporaria(temporaria, Some(3), "/etc/nginx/../nginx.conf");
        let base = temporaria.join("tirech-omni").join("3");
        assert_eq!(a, base.join("etc").join("nginx").join("nginx.conf"));
        let b = pasta_temporaria(temporaria, None, "/x.txt");
        assert_eq!(
            b,
            temporaria.join("tirech-omni").join("sessao").join("x.txt")
        );
    }

    #[test]
    fn o_caminho_em_texto_usa_barra() {
        assert_eq!(caminho_em_texto(Path::new("a/b.txt")), "a/b.txt");
    }
}
