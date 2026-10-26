//! O que o dock do Linux mostra no ícone do app (M49): a barra de progresso e o número de abas
//! que pedem atenção, somados de todas as sessões de todas as janelas.

use tomni_ui::LauncherState;
use tomni_ui::terminal::Session;

/// O `.desktop` com que o dock casa o ícone.
pub const DESKTOP_ID: &str = "tirech-omni.desktop";

/// O estado do dock para `sessoes`: o progresso é a média dos valores das sessões que têm um
/// (`None` se nenhuma tem), e o número é a quantidade de abas com o ponto de atenção aceso
/// (`None` se nenhuma). O dock não pede atenção por conta própria. **Reativo**: lê o progresso e
/// os avisos de cada sessão, então serve dentro de um efeito.
pub fn estado(sessoes: &[Session]) -> LauncherState {
    let (mut soma, mut com_valor, mut atencao) = (0.0, 0_u32, 0_u64);
    for sessao in sessoes {
        if let Some(valor) = sessao.progress().percent() {
            soma += f64::from(valor) / 100.0;
            com_valor += 1;
        }
        if sessao.attention() {
            atencao += 1;
        }
    }
    LauncherState {
        progress: (com_valor > 0).then(|| soma / f64::from(com_valor)),
        count: (atencao > 0).then_some(atencao),
        urgent: false,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use tomni_ui::terminal::{Loopback, Session};

    fn sessao(entrada: &str) -> Session {
        let sessao = Session::new(Loopback::new());
        sessao.feed(entrada.as_bytes());
        sessao
    }

    #[test]
    fn sem_andamento_nem_aviso_o_dock_fica_limpo() {
        assert_eq!(estado(&[]), LauncherState::default());
        assert_eq!(estado(&[sessao("oi")]), LauncherState::default());
    }

    #[test]
    fn o_progresso_e_a_media_de_quem_tem_valor_e_o_numero_conta_os_avisos() {
        let a = sessao("\x1b]9;4;1;20\x07");
        let b = sessao("\x1b]9;4;4;60\x07\x07");
        let c = sessao("\x1b]9;4;3\x07");
        let d = sessao("\x07");
        let e = estado(&[a, b, c, d]);
        assert!((e.progress.unwrap() - 0.4).abs() < 1e-9, "{e:?}");
        assert_eq!(e.count, Some(2));
        assert!(!e.urgent);
    }

    #[test]
    fn trabalhando_sem_valor_nao_mostra_barra() {
        let e = estado(&[sessao("\x1b]9;4;3\x07")]);
        assert_eq!(e.progress, None);
    }
}
