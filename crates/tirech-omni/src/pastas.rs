//! A última pasta de cada seletor de arquivo, no `pastas.json`: o `remember` dos diálogos da lib
//! guarda aqui, e o "Importar…" das chaves (e os outros seletores) abre onde a pessoa estava da
//! última vez, mesmo depois de fechar o app. É só uma conveniência: falhar ao ler ou gravar não
//! avisa ninguém, e uma pasta que sumiu volta à pasta padrão do seletor.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tomni_ui::dialogs::FolderMemory;

use crate::armazenamento::Armazenamento;

/// As pastas por nome do seletor (`chaves`, `cofre`, `redes`…).
pub struct Pastas {
    armazenamento: Armazenamento,
    pastas: RefCell<BTreeMap<String, PathBuf>>,
}

impl Pastas {
    /// Lê o `pastas.json` (ausente ou inválido: nenhuma pasta).
    pub fn carregar(armazenamento: &Armazenamento) -> Self {
        let pastas = armazenamento
            .ler(&armazenamento.arquivo_pastas())
            .unwrap_or_default();
        Self {
            armazenamento: armazenamento.clone(),
            pastas: RefCell::new(pastas),
        }
    }
}

impl FolderMemory for Pastas {
    fn folder(&self, key: &str) -> Option<PathBuf> {
        self.pastas.borrow().get(key).cloned()
    }

    fn set_folder(&self, key: &str, folder: &Path) {
        let mudou = self
            .pastas
            .borrow_mut()
            .insert(key.to_owned(), folder.to_owned())
            .is_none_or(|antes| antes != folder);
        if mudou {
            let _ = self
                .armazenamento
                .gravar(&self.armazenamento.arquivo_pastas(), &*self.pastas.borrow());
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    #[test]
    fn a_pasta_guardada_volta_na_proxima_execucao() {
        let pasta = PastaTemporaria::nova("pastas");
        let armazenamento = Armazenamento::new(&pasta.0);
        Pastas::carregar(&armazenamento).set_folder("chaves", Path::new("/srv/chaves"));
        let de_novo = Pastas::carregar(&armazenamento);
        assert_eq!(de_novo.folder("chaves"), Some(PathBuf::from("/srv/chaves")));
        assert_eq!(de_novo.folder("cofre"), None);
    }
}
