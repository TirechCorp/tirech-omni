//! A troca de nome do M55 (Tirech SSH → Tirech Omni) para quem já usava o app: na primeira
//! abertura, leva para o nome novo o que o app deixou no computador. Roda no começo do `main`,
//! antes de a pasta de dados ser resolvida e de o agente ligar.
//!
//! Cada passo é idempotente (rodar de novo não faz nada) e independente: um que falha entra na
//! lista de erros, que o `main` registra, e o app abre do mesmo jeito. Os passos recebem os
//! lugares por parâmetro ([`Lugares`]), e os testes usam pastas temporárias.
//!
//! Os identificadores gravados nos arquivos (o cofre, os logs, as exportações) **não** mudam e,
//! por isso, o cofre e as senhas vão junto com a pasta sem nenhum passo próprio.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::sistema::{AtalhoDoMenu, LugarDoInicio};

/// O nome da pasta de dados, do atalho e dos arquivos de antes do M55.
const ID_ANTIGO: &str = "tirech-ssh";

/// O nome de antes do M55 como o Windows o guardava no `Run`.
#[cfg(windows)]
const NOME_ANTIGO: &str = "Tirech SSH";

/// O id do `LaunchAgent` do macOS de antes do M55.
const LAUNCH_AGENT_ANTIGO: &str = "com.tirech.ssh";

/// Os lugares que a migração toca. No app de verdade vêm do sistema ([`Lugares::do_sistema`]).
#[derive(Clone, Debug)]
pub struct Lugares {
    /// A pasta de configuração do sistema, onde moram as pastas `tirech-ssh` e `tirech-omni`.
    pub configuracao: Option<PathBuf>,
    /// A pasta pessoal (`~`).
    pub pessoal: Option<PathBuf>,
    /// A pasta `~/.ssh`.
    pub pasta_ssh: PathBuf,
    /// O `XDG_CONFIG_HOME`, onde fica a pasta `autostart` do Linux.
    pub xdg_config: Option<PathBuf>,
    /// O `XDG_DATA_HOME`, onde ficam o `.desktop` e os ícones do menu do Linux.
    pub xdg_data: Option<PathBuf>,
    /// O executável de agora, que as entradas regravadas abrem.
    pub exe: Option<PathBuf>,
    /// A chave do registro do início com o sistema no Windows.
    pub chave_run: String,
    /// Se o sistema é o Windows.
    pub windows: bool,
    /// Se o sistema é o macOS.
    pub macos: bool,
}

impl Lugares {
    /// Os lugares de verdade do computador.
    pub fn do_sistema() -> Self {
        Self {
            configuracao: crate::armazenamento::pasta_de_configuracao(),
            pessoal: crate::armazenamento::pasta_pessoal(),
            pasta_ssh: crate::ferramentas::pasta_ssh_padrao(),
            xdg_config: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            xdg_data: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            exe: std::env::current_exe().ok(),
            chave_run: crate::sistema::CHAVE_RUN.to_owned(),
            windows: cfg!(windows),
            macos: cfg!(target_os = "macos"),
        }
    }
}

/// Um passo da migração: lê os lugares e leva o que achar para o nome novo.
type Passo = fn(&Lugares) -> io::Result<()>;

/// Migra o que o app deixou com o nome antigo. Devolve o motivo de cada passo que falhou (vazio
/// quando deu tudo certo ou não havia nada a migrar).
///
/// Quem chama não roda isto com a pasta de dados trocada por variável de ambiente: os testes, o
/// `lint` e as capturas de tela não mexem nos arquivos da pessoa que os roda.
pub fn migrar(lugares: &Lugares) -> Vec<String> {
    let passos: [(&str, Passo); 4] = [
        ("pasta de dados", pasta_de_dados),
        ("início com o sistema", inicio_com_o_sistema),
        ("atalho do menu", atalho_do_menu),
        ("agente no sistema", agente_no_sistema),
    ];
    passos
        .into_iter()
        .filter_map(|(nome, passo)| {
            passo(lugares)
                .err()
                .map(|erro| format!("migração do nome ({nome}): {erro}"))
        })
        .collect()
}

/// Passo 1: a pasta `tirech-ssh` vira `tirech-omni`. Só se a nova não existe: com as duas, vale a
/// nova e nada se mexe. Se o `rename` falha (arquivo aberto no Windows por uma versão antiga ainda
/// aberta), copia a árvore e deixa a antiga como está.
fn pasta_de_dados(lugares: &Lugares) -> io::Result<()> {
    let Some(base) = lugares.configuracao.as_deref() else {
        return Ok(());
    };
    let antiga = base.join(ID_ANTIGO);
    let nova = base.join(crate::marca::ID_DO_APP);
    if nova.exists() || !antiga.is_dir() {
        return Ok(());
    }
    if fs::rename(&antiga, &nova).is_ok() {
        return Ok(());
    }
    // A cópia parcial de uma tentativa que falhou não pode valer como "a pasta nova".
    if let Err(erro) = copiar_arvore(&antiga, &nova) {
        let _ = fs::remove_dir_all(&nova);
        return Err(erro);
    }
    Ok(())
}

/// Copia `de` para `para`, com as subpastas.
fn copiar_arvore(de: &Path, para: &Path) -> io::Result<()> {
    fs::create_dir_all(para)?;
    for entrada in fs::read_dir(de)? {
        let entrada = entrada?;
        let destino = para.join(entrada.file_name());
        if entrada.file_type()?.is_dir() {
            copiar_arvore(&entrada.path(), &destino)?;
        } else {
            fs::copy(entrada.path(), destino)?;
        }
    }
    Ok(())
}

/// Passo 2: a entrada de início com o sistema, só se a de nome antigo existe. Grava a nova com o
/// executável de agora e apaga a antiga.
fn inicio_com_o_sistema(lugares: &Lugares) -> io::Result<()> {
    let Some(exe) = lugares.exe.as_deref() else {
        return Ok(());
    };
    let Some(novo) = LugarDoInicio::do_sistema(
        lugares.windows,
        lugares.macos,
        lugares.xdg_config.as_deref(),
        lugares.pessoal.as_deref(),
        &lugares.chave_run,
    ) else {
        return Ok(());
    };
    match &novo {
        LugarDoInicio::Desktop(arquivo) => {
            let antigo = arquivo.with_file_name(format!("{ID_ANTIGO}.desktop"));
            trocar_o_arquivo_de_inicio(&novo, &antigo, exe)
        }
        LugarDoInicio::LaunchAgent(arquivo) => {
            let antigo = arquivo.with_file_name(format!("{LAUNCH_AGENT_ANTIGO}.plist"));
            trocar_o_arquivo_de_inicio(&novo, &antigo, exe)
        }
        LugarDoInicio::Registro(chave) => trocar_o_valor_do_registro(&novo, chave, exe),
    }
}

/// Liga `novo` e apaga o arquivo `antigo` (o que não existe fica como está). Se a pessoa já tem a
/// entrada nova, só tira a antiga.
fn trocar_o_arquivo_de_inicio(novo: &LugarDoInicio, antigo: &Path, exe: &Path) -> io::Result<()> {
    if !antigo.is_file() {
        return Ok(());
    }
    if novo.estado(exe) == crate::sistema::EstadoDoInicio::Desligado {
        novo.ligar(exe)?;
    }
    fs::remove_file(antigo)
}

/// O mesmo para o valor do registro do Windows.
#[cfg(windows)]
fn trocar_o_valor_do_registro(novo: &LugarDoInicio, chave: &str, exe: &Path) -> io::Result<()> {
    use crate::sistema::{apagar_run_de, ler_run_de};
    if ler_run_de(chave, NOME_ANTIGO).is_none() {
        return Ok(());
    }
    if novo.estado(exe) == crate::sistema::EstadoDoInicio::Desligado {
        novo.ligar(exe)?;
    }
    apagar_run_de(chave, NOME_ANTIGO)
}

#[cfg(not(windows))]
fn trocar_o_valor_do_registro(_novo: &LugarDoInicio, _chave: &str, _exe: &Path) -> io::Result<()> {
    Ok(())
}

/// Passo 3: o atalho do menu e os ícones do Linux, só se o `.desktop` antigo existe. Grava os
/// novos e apaga os antigos (quem fixou o atalho antigo no painel precisa fixar de novo).
fn atalho_do_menu(lugares: &Lugares) -> io::Result<()> {
    let Some(exe) = lugares.exe.as_deref() else {
        return Ok(());
    };
    let Some(atalho) = AtalhoDoMenu::do_sistema(
        lugares.windows,
        lugares.macos,
        lugares.xdg_data.as_deref(),
        lugares.pessoal.as_deref(),
    ) else {
        return Ok(());
    };
    if !atalho.existe_de(ID_ANTIGO) {
        return Ok(());
    }
    atalho.instalar(exe)?;
    atalho.remover_de(ID_ANTIGO)
}

/// Passo 4: os blocos do agente com o nome antigo, no `~/.ssh/config`, nos arquivos dos shells e
/// no arquivo do fish. A pessoa pode ter ligado o agente no sistema numa versão antiga; o bloco
/// ganha os marcadores novos e o socket padrão novo, e o que ela escreveu fora dele não muda.
fn agente_no_sistema(lugares: &Lugares) -> io::Result<()> {
    use crate::sistema::{mover_o_arquivo_do_fish, trocar_o_nome_do_bloco};
    let mut primeiro_erro = Ok(());
    let mut guardar = |resultado: io::Result<bool>| {
        if let Err(erro) = resultado
            && primeiro_erro.is_ok()
        {
            primeiro_erro = Err(erro);
        }
    };
    guardar(trocar_o_nome_do_bloco(
        &lugares.pasta_ssh.join("config"),
        Some(&backup_de(&lugares.pasta_ssh.join("config"))),
    ));
    if let Some(casa) = lugares.pessoal.as_deref() {
        for nome in crate::ferramentas::ARQUIVOS_POSIX {
            let arquivo = casa.join(nome);
            guardar(trocar_o_nome_do_bloco(&arquivo, Some(&backup_de(&arquivo))));
        }
        let conf_d = casa.join(".config/fish/conf.d");
        guardar(mover_o_arquivo_do_fish(
            &conf_d.join(format!("{ID_ANTIGO}.fish")),
            &conf_d.join(format!("{}.fish", crate::marca::ID_DO_APP)),
        ));
    }
    primeiro_erro
}

/// O arquivo do backup de `arquivo`, o mesmo que o app faz ao gravar o bloco.
fn backup_de(arquivo: &Path) -> PathBuf {
    let mut nome = arquivo.as_os_str().to_owned();
    nome.push(".tirech-bak");
    PathBuf::from(nome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use crate::cofre::Cofre;
    use crate::sistema::{bloco_shell_posix, bloco_ssh_config};

    /// Uma casa de mentira: tudo o que a migração toca fica dentro da pasta do teste.
    fn lugares(raiz: &Path) -> Lugares {
        Lugares {
            configuracao: Some(raiz.join(".config")),
            pessoal: Some(raiz.to_owned()),
            pasta_ssh: raiz.join(".ssh"),
            xdg_config: None,
            xdg_data: None,
            exe: Some(PathBuf::from("/opt/tirech-omni/tirech-omni")),
            chave_run: crate::sistema::CHAVE_RUN.to_owned(),
            windows: false,
            macos: false,
        }
    }

    fn escrever(arquivo: &Path, texto: &str) {
        fs::create_dir_all(arquivo.parent().unwrap()).unwrap();
        fs::write(arquivo, texto).unwrap();
    }

    /// O bloco como o app o gravava antes do M55.
    fn bloco_antigo(bloco: &str) -> String {
        bloco
            .replace("tirech-omni", "tirech-ssh")
            .replace("Tirech Omni", "Tirech SSH")
    }

    #[test]
    fn a_pasta_antiga_vira_a_nova() {
        let casa = PastaTemporaria::nova("migrar-pasta");
        let l = lugares(&casa.0);
        let antiga = casa.0.join(".config/tirech-ssh");
        escrever(&antiga.join("conexoes.json"), "{\"dados\":1}");
        escrever(&antiga.join("anexos/a.bin"), "anexo");

        assert!(migrar(&l).is_empty());
        let nova = casa.0.join(".config/tirech-omni");
        assert!(!antiga.exists());
        assert_eq!(
            fs::read_to_string(nova.join("conexoes.json")).unwrap(),
            "{\"dados\":1}"
        );
        assert_eq!(
            fs::read_to_string(nova.join("anexos/a.bin")).unwrap(),
            "anexo"
        );
    }

    #[test]
    fn com_as_duas_usa_a_nova_e_nao_mexe() {
        let casa = PastaTemporaria::nova("migrar-duas");
        let l = lugares(&casa.0);
        escrever(&casa.0.join(".config/tirech-ssh/conexoes.json"), "antiga");
        escrever(&casa.0.join(".config/tirech-omni/conexoes.json"), "nova");

        assert!(migrar(&l).is_empty());
        assert_eq!(
            fs::read_to_string(casa.0.join(".config/tirech-omni/conexoes.json")).unwrap(),
            "nova"
        );
        assert_eq!(
            fs::read_to_string(casa.0.join(".config/tirech-ssh/conexoes.json")).unwrap(),
            "antiga"
        );
    }

    #[test]
    fn sem_a_antiga_nao_faz_nada() {
        let casa = PastaTemporaria::nova("migrar-nada");
        let l = lugares(&casa.0);
        fs::create_dir_all(&casa.0).unwrap();

        assert!(migrar(&l).is_empty());
        assert!(!casa.0.join(".config").exists());
        assert!(!casa.0.join(".ssh").exists());
    }

    #[test]
    fn rodar_duas_vezes_nao_muda_nada() {
        let casa = PastaTemporaria::nova("migrar-duas-vezes");
        let l = lugares(&casa.0);
        escrever(&casa.0.join(".config/tirech-ssh/conexoes.json"), "dados");
        escrever(
            &casa.0.join(".bashrc"),
            &format!(
                "alias a=b\n{}\n",
                bloco_antigo(&bloco_shell_posix(Path::new(
                    "/run/user/1/tirech-omni/agent.sock"
                )))
            ),
        );
        escrever(
            &casa.0.join(".config/autostart/tirech-ssh.desktop"),
            "[Desktop Entry]\n",
        );

        assert!(migrar(&l).is_empty());
        let depois = (
            fs::read_to_string(casa.0.join(".bashrc")).unwrap(),
            fs::read_to_string(casa.0.join(".config/tirech-omni/conexoes.json")).unwrap(),
            fs::read_to_string(casa.0.join(".config/autostart/tirech-omni.desktop")).unwrap(),
        );
        assert!(migrar(&l).is_empty());
        let de_novo = (
            fs::read_to_string(casa.0.join(".bashrc")).unwrap(),
            fs::read_to_string(casa.0.join(".config/tirech-omni/conexoes.json")).unwrap(),
            fs::read_to_string(casa.0.join(".config/autostart/tirech-omni.desktop")).unwrap(),
        );
        assert_eq!(depois, de_novo);
    }

    #[test]
    fn o_pasta_txt_vai_junto() {
        let casa = PastaTemporaria::nova("migrar-pasta-txt");
        let l = lugares(&casa.0);
        escrever(
            &casa.0.join(".config/tirech-ssh/pasta.txt"),
            "/mnt/pendrive/dados\n",
        );

        assert!(migrar(&l).is_empty());
        assert_eq!(
            fs::read_to_string(casa.0.join(".config/tirech-omni/pasta.txt")).unwrap(),
            "/mnt/pendrive/dados\n"
        );
        assert_eq!(
            crate::armazenamento::resolver_pasta(None, None, &casa.0.join(".config/tirech-omni")).0,
            PathBuf::from("/mnt/pendrive/dados")
        );
    }

    #[test]
    fn a_copia_serve_quando_o_rename_nao_da() {
        let casa = PastaTemporaria::nova("migrar-copia");
        let antiga = casa.0.join("antiga");
        let nova = casa.0.join("nova");
        escrever(&antiga.join("a.json"), "a");
        escrever(&antiga.join("sub/fundo/b.bin"), "b");

        copiar_arvore(&antiga, &nova).unwrap();
        assert_eq!(fs::read_to_string(nova.join("a.json")).unwrap(), "a");
        assert_eq!(
            fs::read_to_string(nova.join("sub/fundo/b.bin")).unwrap(),
            "b"
        );
        assert!(antiga.join("a.json").exists());
    }

    #[test]
    fn o_autostart_antigo_vira_o_novo_com_o_executavel_de_agora() {
        let casa = PastaTemporaria::nova("migrar-autostart");
        let l = lugares(&casa.0);
        let antigo = casa.0.join(".config/autostart/tirech-ssh.desktop");
        escrever(
            &antigo,
            "[Desktop Entry]\nExec=/velho/tirech-ssh --bandeja\n",
        );

        assert!(migrar(&l).is_empty());
        assert!(!antigo.exists());
        let novo =
            fs::read_to_string(casa.0.join(".config/autostart/tirech-omni.desktop")).unwrap();
        assert!(novo.contains("/opt/tirech-omni/tirech-omni"), "{novo}");
        assert!(!novo.contains("/velho/"), "{novo}");
    }

    #[test]
    fn o_launch_agent_antigo_vira_o_novo() {
        let casa = PastaTemporaria::nova("migrar-launch-agent");
        let mut l = lugares(&casa.0);
        l.macos = true;
        let pasta = casa.0.join("Library/LaunchAgents");
        escrever(&pasta.join("com.tirech.ssh.plist"), "<plist/>");

        assert!(migrar(&l).is_empty());
        assert!(!pasta.join("com.tirech.ssh.plist").exists());
        let novo = fs::read_to_string(pasta.join("com.tirech.omni.plist")).unwrap();
        assert!(novo.contains("<string>com.tirech.omni</string>"), "{novo}");
        assert!(novo.contains("/opt/tirech-omni/tirech-omni"), "{novo}");
    }

    #[test]
    fn sem_o_autostart_antigo_nao_liga_o_inicio() {
        let casa = PastaTemporaria::nova("migrar-sem-autostart");
        let l = lugares(&casa.0);
        fs::create_dir_all(&casa.0).unwrap();

        assert!(migrar(&l).is_empty());
        assert!(!casa.0.join(".config/autostart").exists());
    }

    #[test]
    fn o_atalho_antigo_do_menu_vira_o_novo() {
        let casa = PastaTemporaria::nova("migrar-menu");
        let l = lugares(&casa.0);
        let dados = casa.0.join(".local/share");
        escrever(
            &dados.join("applications/tirech-ssh.desktop"),
            "[Desktop Entry]\n",
        );
        escrever(
            &dados.join("icons/hicolor/48x48/apps/tirech-ssh.png"),
            "png",
        );
        escrever(
            &dados.join("icons/hicolor/scalable/apps/tirech-ssh.svg"),
            "svg",
        );

        assert!(migrar(&l).is_empty());
        assert!(!dados.join("applications/tirech-ssh.desktop").exists());
        assert!(
            !dados
                .join("icons/hicolor/48x48/apps/tirech-ssh.png")
                .exists()
        );
        assert!(
            !dados
                .join("icons/hicolor/scalable/apps/tirech-ssh.svg")
                .exists()
        );
        let desktop = fs::read_to_string(dados.join("applications/tirech-omni.desktop")).unwrap();
        assert!(desktop.contains("Icon=tirech-omni"), "{desktop}");
        assert!(desktop.contains("StartupWMClass=tirech-omni"), "{desktop}");
        assert!(
            dados
                .join("icons/hicolor/scalable/apps/tirech-omni.svg")
                .is_file()
        );
    }

    #[test]
    fn o_bloco_antigo_do_ssh_config_vira_o_novo_com_o_socket_novo() {
        let casa = PastaTemporaria::nova("migrar-ssh-config");
        let l = lugares(&casa.0);
        let config = casa.0.join(".ssh/config");
        let socket_antigo = Path::new("/run/user/1000/tirech-ssh/agent.sock");
        let antes = format!(
            "Host a\n    User ana\n\n{}\n\nHost b\n    User bia\n",
            bloco_antigo(&bloco_ssh_config(Path::new(
                "/run/user/1000/tirech-omni/agent.sock"
            )))
        );
        assert!(antes.contains(&socket_antigo.display().to_string()));
        escrever(&config, &antes);

        assert!(migrar(&l).is_empty());
        let depois = fs::read_to_string(&config).unwrap();
        assert!(depois.contains("# >>> tirech-omni: agente"), "{depois}");
        assert!(depois.contains("# <<< tirech-omni: agente <<<"), "{depois}");
        assert!(
            depois.contains("/run/user/1000/tirech-omni/agent.sock"),
            "{depois}"
        );
        assert!(!depois.contains("tirech-ssh"), "{depois}");
        // O que a pessoa escreveu fora do bloco fica como estava, e o backup guarda o original.
        assert!(depois.starts_with("Host a\n    User ana\n\n"));
        assert!(depois.ends_with("\n\nHost b\n    User bia\n"));
        assert_eq!(fs::read_to_string(backup_de(&config)).unwrap(), antes);
    }

    #[test]
    fn o_socket_escolhido_pela_pessoa_fica_como_esta() {
        let casa = PastaTemporaria::nova("migrar-socket-proprio");
        let l = lugares(&casa.0);
        let config = casa.0.join(".ssh/config");
        escrever(
            &config,
            &bloco_antigo(&bloco_ssh_config(Path::new("/srv/meu/agente.sock"))),
        );

        assert!(migrar(&l).is_empty());
        let depois = fs::read_to_string(&config).unwrap();
        assert!(depois.contains("/srv/meu/agente.sock"), "{depois}");
        assert!(depois.contains("# >>> tirech-omni: agente"), "{depois}");
    }

    #[test]
    fn o_bloco_antigo_dos_shells_e_do_fish_vira_o_novo() {
        let casa = PastaTemporaria::nova("migrar-shells");
        let l = lugares(&casa.0);
        let socket = Path::new("/run/user/1000/tirech-omni/agent.sock");
        let bloco = bloco_antigo(&bloco_shell_posix(socket));
        escrever(&casa.0.join(".bashrc"), &format!("export A=1\n{bloco}\n"));
        escrever(&casa.0.join(".zshrc"), "# sem bloco\n");
        let fish_antigo = casa.0.join(".config/fish/conf.d/tirech-ssh.fish");
        escrever(&fish_antigo, &format!("{bloco}\n"));

        assert!(migrar(&l).is_empty());
        let bash = fs::read_to_string(casa.0.join(".bashrc")).unwrap();
        assert!(
            bash.starts_with("export A=1\n# >>> tirech-omni: agente"),
            "{bash}"
        );
        assert!(!bash.contains("tirech-ssh"), "{bash}");
        assert_eq!(
            fs::read_to_string(casa.0.join(".zshrc")).unwrap(),
            "# sem bloco\n"
        );
        assert!(!casa.0.join(".zshrc.tirech-bak").exists());
        assert!(!fish_antigo.exists());
        let fish = fs::read_to_string(casa.0.join(".config/fish/conf.d/tirech-omni.fish")).unwrap();
        assert!(fish.contains("# >>> tirech-omni: agente"), "{fish}");
        assert!(!fish.contains("tirech-ssh"), "{fish}");
    }

    #[test]
    fn o_cofre_continua_abrindo_depois_da_migracao() {
        let casa = PastaTemporaria::nova("migrar-cofre");
        let l = lugares(&casa.0);
        let antiga = casa.0.join(".config/tirech-ssh");
        let (mut cofre, _) = Cofre::abrir(&antiga);
        cofre.definir_senha_mestra(Some("a mestra")).unwrap();

        assert!(migrar(&l).is_empty());
        let (mut reaberto, aviso) = Cofre::abrir(&casa.0.join(".config/tirech-omni"));
        assert_eq!(aviso, None);
        assert!(reaberto.bloqueado() && reaberto.tem_senha_mestra());
        assert!(!reaberto.desbloquear("outra"));
        assert!(reaberto.desbloquear("a mestra"));
    }

    #[test]
    fn os_identificadores_de_formato_nao_mudam() {
        // Gravados nos arquivos de quem usa: mudar um quebraria o cofre, os logs ou as exportações.
        assert_eq!(crate::cofre::VERIFICADOR, b"tirech-ssh");
        assert_eq!(crate::cofre::CAMPO_TIPO, "tirech-ssh.tipo");
        assert_eq!(crate::cofre::CAMPO_LOGS, "tirech-ssh.logs");
        assert_eq!(crate::cofre_json::FORMATO, "tirech-ssh-cofre");
        assert_eq!(crate::log_cifrado::INFO, b"tirech-ssh log 1");
        assert_eq!(crate::tela::cofre_portatil::EXTENSAO, "tirech-vault");
    }
}
