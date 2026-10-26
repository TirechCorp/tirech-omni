//! A navegação sincronizada: entrar, subir ou digitar um caminho num painel leva o outro à
//! mesma pasta, contada a partir de um par de pastas base.
//!
//! A parte pura são [`relativo`] e [`juntar`]. [`ligar`] liga os efeitos da aba: eles guardam a
//! base, espelham as navegações do usuário e desligam a sincronização, com aviso, quando o
//! espelho não cabe. A navegação espelhada leva a marca [`Origem::Espelho`] e por isso não
//! espelha de volta.

use tomni_ui::files::path;
use tomni_ui::prelude::*;

use super::{Aba, Lado, Painel};
use crate::t;

/// De onde uma navegação veio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origem {
    /// O usuário (clique, Enter, Acima, Backspace, caminho digitado, Início): espelha.
    #[default]
    Usuario,
    /// A sincronização, a pedido do outro painel: não espelha de volta.
    Espelho,
}

/// O caminho de `destino` relativo a `base` (os dois absolutos, com `/` ou `\`); `None` se
/// `destino` fica fora de `base`. A fronteira é a barra, não o prefixo do texto.
///
/// ```text
/// relativo("/a/b", "/a/b/c/d") == Some("c/d")
/// relativo("/a/b", "/a/b")     == Some("")
/// relativo("/a/b", "/a/bc")    == None
/// ```
pub fn relativo(base: &str, destino: &str) -> Option<String> {
    let base = path::normalize(base);
    let destino = path::normalize(destino);
    let base = base.trim_end_matches('/');
    if destino.trim_end_matches('/') == base {
        return Some(String::new());
    }
    destino
        .strip_prefix(base)
        .and_then(|resto| resto.strip_prefix('/'))
        .map(str::to_owned)
}

/// `base` + `rel`, normalizado (sem `//`, sem `/.`). Um `rel` vazio devolve a própria base.
pub fn juntar(base: &str, rel: &str) -> String {
    path::join(base, rel.trim_start_matches(['/', '\\']))
}

/// O que a aba diz quando a sincronização desliga.
fn desligada(motivo: String) -> String {
    t!("arquivos-sincronia-desligada", motivo = motivo)
}

/// O aviso de que a pasta espelhada não existe no lado.
fn nao_existe_no_lado(lado: Lado, pasta: &str) -> String {
    match lado {
        Lado::Local => t!("arquivos-sincronia-nao-existe-no-disco", pasta = pasta),
        Lado::Remoto => t!("arquivos-sincronia-nao-existe-no-servidor", pasta = pasta),
    }
}

/// Liga a sincronização da aba. Chame dentro da view da aba (os efeitos morrem com ela).
/// `aviso` é a `InfoBar` da aba.
pub fn ligar(aba: &Aba, aviso: Signal<Option<String>>) {
    let (sincronizar, base) = (aba.sincronizar, aba.base);
    let (local, remoto) = (aba.local.clone(), aba.remoto.clone());

    // A base: o par de pastas de quando a sincronização liga. Ao abrir ligada, espera os dois
    // painéis listarem a primeira pasta. Desligada, não há base.
    {
        let (local, remoto) = (local.clone(), remoto.clone());
        effect(move || {
            let ligada = sincronizar.get();
            let prontos =
                local.caminho.with(|c| !c.is_empty()) && remoto.caminho.with(|c| !c.is_empty());
            untrack(|| {
                if !ligada {
                    if base.with_untracked(Option::is_some) {
                        base.set(None);
                    }
                } else if prontos && base.with_untracked(Option::is_none) {
                    base.set(Some((
                        local.caminho.get_untracked(),
                        remoto.caminho.get_untracked(),
                    )));
                }
            });
        });
    }

    // A navegação do usuário num painel vai para o outro.
    for (lado, painel, outro) in [
        (Lado::Local, local.clone(), remoto.clone()),
        (Lado::Remoto, remoto.clone(), local.clone()),
    ] {
        let navegou = painel.navegou;
        effect(move || {
            let (n, destino) = navegou.get();
            if n == 0 {
                return;
            }
            untrack(|| espelhar(lado, &destino, &outro, sincronizar, base, aviso));
        });
    }

    // O espelho que não deu certo: a sincronização desliga e o outro painel fica onde estava.
    for painel in [local, remoto] {
        let (lado, falhou) = (painel.lado, painel.espelho_falhou);
        effect(move || {
            let Some(pasta) = falhou.get() else {
                return;
            };
            untrack(|| {
                falhou.set(None);
                sincronizar.set(false);
                aviso.set(Some(desligada(nao_existe_no_lado(lado, &pasta))));
            });
        });
    }
}

/// Uma navegação do usuário chegou em `destino`, do lado `lado`: leva o `outro` painel junto.
fn espelhar(
    lado: Lado,
    destino: &str,
    outro: &Painel,
    sincronizar: Signal<bool>,
    base: Signal<Option<(String, String)>>,
    aviso: Signal<Option<String>>,
) {
    if !sincronizar.get_untracked() {
        return;
    }
    let Some((base_local, base_remota)) = base.get_untracked() else {
        return;
    };
    let (minha, da_outra) = match lado {
        Lado::Local => (base_local, base_remota),
        Lado::Remoto => (base_remota, base_local),
    };
    match relativo(&minha, destino) {
        Some(rel) => outro.espelhar(&juntar(&da_outra, &rel)),
        None => {
            sincronizar.set(false);
            aviso.set(Some(desligada(t!(
                "arquivos-sincronia-fora-da-base",
                destino = destino,
                minha = minha.as_str()
            ))));
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn relativo_de_um_filho_e_de_um_neto() {
        assert_eq!(relativo("/a/b", "/a/b/c").as_deref(), Some("c"));
        assert_eq!(relativo("/a/b", "/a/b/c/d").as_deref(), Some("c/d"));
    }

    #[test]
    fn relativo_da_propria_base_e_vazio() {
        assert_eq!(relativo("/a/b", "/a/b").as_deref(), Some(""));
        assert_eq!(relativo("/a/b/", "/a/b").as_deref(), Some(""));
        assert_eq!(relativo("/a/b", "/a/b/").as_deref(), Some(""));
    }

    #[test]
    fn relativo_de_fora_e_none() {
        assert_eq!(relativo("/a/b", "/a"), None);
        assert_eq!(relativo("/a/b", "/x/y"), None);
        assert_eq!(relativo("/a/b", "/"), None);
    }

    #[test]
    fn a_fronteira_e_a_barra_e_nao_o_prefixo_do_texto() {
        assert_eq!(relativo("/a/b", "/a/bc"), None);
        assert_eq!(relativo("/a/b", "/a/bc/d"), None);
    }

    #[test]
    fn relativo_com_a_raiz_como_base() {
        assert_eq!(relativo("/", "/a/b").as_deref(), Some("a/b"));
        assert_eq!(relativo("/", "/").as_deref(), Some(""));
    }

    #[test]
    fn relativo_normaliza_antes_de_comparar() {
        assert_eq!(relativo("/a//b/", "/a/./b/c/../d").as_deref(), Some("d"));
    }

    #[test]
    fn relativo_com_barra_invertida_do_windows() {
        assert_eq!(
            relativo("C:\\site", "C:\\site\\css\\temas").as_deref(),
            Some("css/temas")
        );
        assert_eq!(relativo("C:/site", "D:/site"), None);
    }

    #[test]
    fn juntar_com_rel_vazio_devolve_a_base() {
        assert_eq!(juntar("/srv/www", ""), "/srv/www");
        assert_eq!(juntar("/srv/www/", ""), "/srv/www");
    }

    #[test]
    fn juntar_arruma_barra_dupla_e_ponto() {
        assert_eq!(juntar("/srv/www", "css//temas"), "/srv/www/css/temas");
        assert_eq!(juntar("/srv/www", "./css/."), "/srv/www/css");
    }

    #[test]
    fn juntar_nao_deixa_o_rel_virar_absoluto() {
        assert_eq!(juntar("/srv/www", "/css"), "/srv/www/css");
    }

    #[test]
    fn juntar_desfaz_o_relativo() {
        let rel = relativo("/home/ana/site", "/home/ana/site/css/a").unwrap();
        assert_eq!(juntar("/srv/www", &rel), "/srv/www/css/a");
    }

    #[test]
    fn juntar_com_a_raiz() {
        assert_eq!(juntar("/", "a/b"), "/a/b");
    }
}
