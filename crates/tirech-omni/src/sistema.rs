//! O que o app escreve fora da própria pasta para o resto do sistema usar o agente (M44): o
//! bloco do `~/.ssh/config`, o `SSH_AUTH_SOCK` dos shells e a gravação segura desses arquivos.
//!
//! As funções de texto são puras: recebem o conteúdo do arquivo e devolvem o novo, sem tocar no
//! que a pessoa escreveu (byte a byte) e sem trocar o fim de linha que o arquivo usa.

use crate::icone;
use crate::marca::{ID_DO_APP, NOME_DO_APP};
use crate::t;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// A primeira linha do bloco do `~/.ssh/config`.
pub const INICIO_CONFIG: &str =
    "# >>> tirech-omni: agente (gerado pelo Tirech Omni; \"Desligar\" na página Agente remove) >>>";

/// A primeira linha do bloco dos shells.
pub const INICIO_SHELL: &str = "# >>> tirech-omni: agente >>>";

/// O começo de qualquer um dos dois: é por ele que o bloco é achado.
const PREFIXO_INICIO: &str = "# >>> tirech-omni: agente";

/// A última linha dos blocos.
const FIM: &str = "# <<< tirech-omni: agente <<<";

/// Os mesmos marcadores de antes do M55 (o app se chamava Tirech SSH). Continuam reconhecidos: o
/// bloco antigo é trocado pelo novo ao ligar e sai ao desligar, sem sobrar duas cópias.
const PREFIXO_INICIO_ANTIGO: &str = "# >>> tirech-ssh: agente";
const FIM_ANTIGO: &str = "# <<< tirech-ssh: agente <<<";

/// Se `linha` abre um bloco do app (com o nome de agora ou o antigo).
fn abre_bloco(linha: &str) -> bool {
    linha.starts_with(PREFIXO_INICIO) || linha.starts_with(PREFIXO_INICIO_ANTIGO)
}

/// Se `linha` fecha um bloco do app (com o nome de agora ou o antigo).
fn fecha_bloco(linha: &str) -> bool {
    let linha = linha.trim_end();
    linha == FIM || linha == FIM_ANTIGO
}

/// O bloco do `~/.ssh/config`: enquanto o socket existir, o `ssh` usa o agente do app; sem ele
/// (app fechado), a configuração segue como era. O `Match all` do fim devolve ao resto do arquivo o
/// escopo de antes: sem ele, o que vem depois (opções soltas no topo do `config`) ficaria dentro
/// do `Match exec` e só valeria com o app aberto.
pub fn bloco_ssh_config(socket: &Path) -> String {
    let socket = socket.display().to_string();
    let teste = format!("test -S {}", aspas_sh(&socket));
    format!(
        "{INICIO_CONFIG}\nMatch exec \"{}\"\n    IdentityAgent \"{}\"\nMatch all\n{FIM}",
        escapar_config(&teste),
        escapar_config(&socket),
    )
}

/// O bloco do `.bashrc` e do `.zshrc`.
pub fn bloco_shell_posix(socket: &Path) -> String {
    let socket = aspas_sh(&socket.display().to_string());
    format!("{INICIO_SHELL}\n[ -S {socket} ] && export SSH_AUTH_SOCK={socket}\n{FIM}")
}

/// O bloco do fish (`~/.config/fish/conf.d/tirech-omni.fish`).
pub fn bloco_shell_fish(socket: &Path) -> String {
    let socket = aspas_fish(&socket.display().to_string());
    format!("{INICIO_SHELL}\ntest -S {socket}; and set -gx SSH_AUTH_SOCK {socket}\n{FIM}")
}

/// Entre aspas simples para o `sh`.
fn aspas_sh(texto: &str) -> String {
    format!("'{}'", texto.replace('\'', "'\\''"))
}

/// Entre aspas simples para o fish (lá `\'` e `\\` valem dentro das aspas).
fn aspas_fish(texto: &str) -> String {
    format!("'{}'", texto.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// O texto de um argumento entre aspas duplas do `ssh_config`.
fn escapar_config(texto: &str) -> String {
    texto
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

/// Onde o bloco entra quando o arquivo ainda não o tem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lugar {
    /// No começo (o `~/.ssh/config`: vale o primeiro valor que o `ssh` acha).
    Topo,
    /// No fim (os arquivos dos shells).
    Fim,
}

/// O fim de linha que o arquivo usa: `\r\n` se é o que ele tem, senão `\n`.
fn fim_de_linha(texto: &str) -> &'static str {
    if texto.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Onde o bloco está no texto: o começo da primeira linha dele e o fim da última (com o fim de
/// linha dela). Um começo sem fim vale como bloco de uma linha só.
fn achar_bloco(texto: &str) -> Option<(usize, usize)> {
    let mut posicao = 0;
    let mut inicio = None;
    for linha in texto.split_inclusive('\n') {
        let fim_da_linha = posicao + linha.len();
        let limpa = linha.trim_end_matches(['\r', '\n']);
        match inicio {
            None if abre_bloco(limpa) => {
                inicio = Some(posicao);
                // Um começo sem fim, seguido de outro começo ou do fim do arquivo, é só ele.
                if !texto[fim_da_linha..].lines().any(fecha_bloco) {
                    return Some((posicao, fim_da_linha));
                }
            }
            Some(inicio) if fecha_bloco(limpa) => return Some((inicio, fim_da_linha)),
            _ => {}
        }
        posicao = fim_da_linha;
    }
    None
}

/// O bloco com os fins de linha do arquivo.
fn com_eol(bloco: &str, eol: &str) -> String {
    if eol == "\n" {
        bloco.to_owned()
    } else {
        bloco.replace('\n', eol)
    }
}

/// Põe o `bloco` no `texto`: troca o que já está lá ou o acrescenta em `lugar`. O resto do
/// arquivo não muda, e o fim de linha é o do arquivo.
pub fn com_bloco_em(texto: &str, bloco: &str, lugar: Lugar) -> String {
    let eol = fim_de_linha(texto);
    let bloco = com_eol(bloco, eol);
    if let Some((inicio, fim)) = achar_bloco(texto) {
        // Mantém o fim de linha que fechava o bloco antigo (ou nenhum, no fim do arquivo).
        let terminador = &texto[inicio..fim];
        let cauda = &terminador[terminador.trim_end_matches(['\r', '\n']).len()..];
        return format!("{}{bloco}{cauda}{}", &texto[..inicio], &texto[fim..]);
    }
    if texto.is_empty() {
        return format!("{bloco}{eol}");
    }
    match lugar {
        Lugar::Topo => format!("{bloco}{eol}{eol}{texto}"),
        Lugar::Fim => {
            let separador = if texto.ends_with('\n') { "" } else { eol };
            format!("{texto}{separador}{eol}{bloco}{eol}")
        }
    }
}

/// [`com_bloco_em`] no topo: é o que o `~/.ssh/config` pede.
pub fn com_bloco(texto: &str, bloco: &str) -> String {
    com_bloco_em(texto, bloco, Lugar::Topo)
}

/// Tira o bloco do `texto`, com a linha em branco que o separava do resto. Sem bloco, devolve o
/// texto igual.
pub fn sem_bloco(texto: &str) -> String {
    let Some((mut inicio, mut fim)) = achar_bloco(texto) else {
        return texto.to_owned();
    };
    let linha_vazia = |s: &str| s == "\n" || s == "\r\n";
    if inicio == 0 {
        // No topo: a linha em branco que separava vem depois.
        let resto = &texto[fim..];
        let proxima = resto.split_inclusive('\n').next().unwrap_or("");
        if linha_vazia(proxima) {
            fim += proxima.len();
        }
    } else if fim == texto.len() || texto[fim..].trim().is_empty() {
        // No fim: a que separava vem antes.
        let antes = &texto[..inicio];
        let anterior = antes
            .rfind('\n')
            .map_or(antes, |i| &antes[..i])
            .rfind('\n')
            .map_or(0, |i| i + 1);
        let ultima = &antes[anterior..];
        if linha_vazia(ultima) && anterior > 0 {
            inicio = anterior;
        }
    }
    format!("{}{}", &texto[..inicio], &texto[fim..])
}

/// O socket que o bloco do `texto` usa, se há bloco: o `IdentityAgent` do `~/.ssh/config` ou o
/// socket do `test -S` dos shells. Um bloco que não dá para ler devolve o caminho vazio.
pub fn tem_bloco(texto: &str) -> Option<PathBuf> {
    let (inicio, fim) = achar_bloco(texto)?;
    let bloco = &texto[inicio..fim];
    for linha in bloco.lines() {
        let linha = linha.trim();
        if let Some(resto) = linha.strip_prefix("IdentityAgent")
            && let Some(socket) = entre_aspas_duplas(resto.trim())
        {
            return Some(PathBuf::from(socket));
        }
        if let Some(depois) = linha
            .find("-S ")
            .map(|i| &linha[i + 3..])
            .filter(|_| linha.contains("SSH_AUTH_SOCK"))
            && let Some(socket) = entre_aspas_simples(depois.trim_start())
        {
            return Some(PathBuf::from(socket));
        }
    }
    Some(PathBuf::new())
}

/// O conteúdo de `"..."` no começo de `texto`, sem os escapes do `ssh_config`.
fn entre_aspas_duplas(texto: &str) -> Option<String> {
    let mut chars = texto.strip_prefix('"')?.chars();
    let mut saida = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(saida),
            '\\' => saida.push(chars.next()?),
            '%' => {
                // `%%` é um `%`.
                saida.push('%');
                if chars.clone().next() == Some('%') {
                    chars.next();
                }
            }
            c => saida.push(c),
        }
    }
    None
}

/// O conteúdo de `'...'` no começo de `texto` (com `'\''` e `\'` do fish).
fn entre_aspas_simples(texto: &str) -> Option<String> {
    let mut chars = texto.strip_prefix('\'')?.chars().peekable();
    let mut saida = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                // `'\''` volta a abrir as aspas; senão acabou.
                let mut resto = chars.clone();
                if resto.next() == Some('\\')
                    && resto.next() == Some('\'')
                    && resto.next() == Some('\'')
                {
                    saida.push('\'');
                    chars = resto;
                } else {
                    return Some(saida);
                }
            }
            '\\' if matches!(chars.peek(), Some('\'' | '\\')) => saida.push(chars.next()?),
            c => saida.push(c),
        }
    }
    None
}

/// Põe (`Some`) ou tira (`None`) o `bloco` do arquivo.
///
/// - Se o arquivo é um link simbólico (dotfiles), escreve no destino.
/// - `criar` deixa criar o arquivo (e a pasta, 0700) quando ele não existe; sem isso um arquivo
///   que não existe continua sem existir.
/// - `backup`, quando dado, recebe uma cópia do arquivo antes da escrita.
/// - A escrita é atômica; o arquivo novo é 0600 e o que já existia mantém a permissão.
///
/// Devolve se o arquivo mudou.
pub fn aplicar_bloco(
    arquivo: &Path,
    bloco: Option<&str>,
    lugar: Lugar,
    criar: bool,
    backup: Option<&Path>,
) -> io::Result<bool> {
    let destino = fs::canonicalize(arquivo).unwrap_or_else(|_| arquivo.to_owned());
    let existe = destino.is_file();
    let antigo = if existe {
        let bytes = fs::read(&destino)?;
        String::from_utf8(bytes).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                t!(
                    "sistema-nao-esta-em-utf8",
                    arquivo = destino.display().to_string()
                ),
            )
        })?
    } else {
        String::new()
    };
    if !existe && !(criar && bloco.is_some()) {
        return Ok(false);
    }
    let novo = match bloco {
        Some(bloco) => com_bloco_em(&antigo, bloco, lugar),
        None => sem_bloco(&antigo),
    };
    if existe && novo == antigo {
        return Ok(false);
    }
    if let Some(pai) = destino.parent().filter(|p| !p.as_os_str().is_empty())
        && !pai.exists()
    {
        criar_pasta_privada(pai)?;
    }
    if existe && let Some(backup) = backup {
        fs::copy(&destino, backup)?;
    }
    gravar_atomico(&destino, novo.as_bytes(), existe)?;
    Ok(true)
}

/// Cria a pasta (e as de cima) com 0700 no Unix.
fn criar_pasta_privada(pasta: &Path) -> io::Result<()> {
    let mut construtor = fs::DirBuilder::new();
    construtor.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        construtor.mode(0o700);
    }
    construtor.create(pasta)
}

/// Escreve `bytes` num arquivo ao lado e o põe no lugar de `destino`. `manter` conserva a
/// permissão do arquivo que já existe; sem ele, o novo é 0600 (Unix).
pub(crate) fn gravar_atomico(destino: &Path, bytes: &[u8], manter: bool) -> io::Result<()> {
    let nome = destino
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = destino.with_file_name(format!(".{nome}.tirech-tmp"));
    let _ = fs::remove_file(&tmp);
    let resultado = (|| {
        let mut opcoes = fs::OpenOptions::new();
        opcoes.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opcoes.mode(0o600);
        }
        let mut arquivo = opcoes.open(&tmp)?;
        arquivo.write_all(bytes)?;
        arquivo.sync_all()?;
        if manter {
            fs::set_permissions(&tmp, fs::metadata(destino)?.permissions())?;
        }
        fs::rename(&tmp, destino)
    })();
    if resultado.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    resultado
}

/// O `texto` com o bloco de antes do M55 (os marcadores `tirech-ssh`) trocado pelo de agora: os
/// marcadores, o nome do app e o socket padrão (`.../tirech-ssh/agent.sock`,
/// `tirech-ssh-agent.sock`). Um socket que a pessoa escolheu fica como está. `None` se o texto não
/// tem bloco com o nome antigo.
pub(crate) fn com_o_nome_novo(texto: &str) -> Option<String> {
    let (inicio, fim) = achar_bloco(texto)?;
    let bloco = &texto[inicio..fim];
    if !bloco.starts_with(PREFIXO_INICIO_ANTIGO) {
        return None;
    }
    let novo = bloco
        .replace(PREFIXO_INICIO_ANTIGO, PREFIXO_INICIO)
        .replace(FIM_ANTIGO, FIM)
        .replace(
            "gerado pelo Tirech SSH",
            &format!("gerado pelo {NOME_DO_APP}"),
        )
        .replace("/tirech-ssh/agent.sock", "/tirech-omni/agent.sock")
        .replace("tirech-ssh-agent.sock", "tirech-omni-agent.sock");
    Some(format!("{}{novo}{}", &texto[..inicio], &texto[fim..]))
}

/// Troca, em `arquivo`, o bloco com o nome antigo pelo de agora ([`com_o_nome_novo`]). O que a
/// pessoa escreveu fora do bloco não muda. `backup`, se dado e ainda inexistente, recebe uma
/// cópia antes da escrita. Devolve se o arquivo mudou (o ausente, o ilegível e o sem bloco antigo
/// ficam como estão).
pub(crate) fn trocar_o_nome_do_bloco(arquivo: &Path, backup: Option<&Path>) -> io::Result<bool> {
    let destino = fs::canonicalize(arquivo).unwrap_or_else(|_| arquivo.to_owned());
    let Ok(bytes) = fs::read(&destino) else {
        return Ok(false);
    };
    let Ok(texto) = String::from_utf8(bytes) else {
        return Ok(false);
    };
    let Some(novo) = com_o_nome_novo(&texto) else {
        return Ok(false);
    };
    if let Some(backup) = backup.filter(|b| !b.exists()) {
        fs::copy(&destino, backup)?;
    }
    gravar_atomico(&destino, novo.as_bytes(), true)?;
    Ok(true)
}

/// Passa o arquivo do fish de `antigo` para `novo`, com o bloco no nome de agora, e apaga o
/// antigo. Se o novo já existe, só apaga o antigo. Devolve se havia o antigo.
pub(crate) fn mover_o_arquivo_do_fish(antigo: &Path, novo: &Path) -> io::Result<bool> {
    let Ok(texto) = fs::read_to_string(antigo) else {
        return Ok(false);
    };
    if !novo.exists() {
        let conteudo = com_o_nome_novo(&texto).unwrap_or(texto);
        if let Some(pasta) = novo.parent() {
            fs::create_dir_all(pasta)?;
        }
        gravar_atomico(novo, conteudo.as_bytes(), false)?;
    }
    fs::remove_file(antigo)?;
    Ok(true)
}

/// O socket do bloco que `arquivo` tem, se tem (o arquivo ausente ou ilegível não tem).
pub fn bloco_do_arquivo(arquivo: &Path) -> Option<PathBuf> {
    tem_bloco(&fs::read_to_string(arquivo).ok()?)
}

// --- Processos de console sem janela (M65) ---

/// Faz o processo de console abrir sem janela. O release do Windows é do subsistema gráfico
/// (sem console próprio): sem isto, cada `rclone`, `cmd` ou `powershell` que o app roda abre uma
/// janela de console que pisca na tela. No Windows põe `CREATE_NO_WINDOW`; nos outros sistemas não
/// faz nada. Devolve o próprio `Command`, para encadear. É o `tomni_ui::process::no_window` da lib.
///
/// Exceções: o `reinicio`, que abre o próprio app (de interface), e o que já usa
/// `DETACHED_PROCESS`.
pub fn sem_janela(comando: &mut std::process::Command) -> &mut std::process::Command {
    tomni_ui::process::no_window(comando)
}

// --- Iniciar com o sistema ---

/// O argumento da entrada de início com o sistema. Com `geral.iniciar_na_bandeja`, o app abre
/// escondido na bandeja; sem, abre a janela. A entrada não muda quando a opção muda.
pub const ARGUMENTO_BANDEJA: &str = "--bandeja";

/// Quanto o app espera, aberto com `--bandeja`, para saber se o sistema tem bandeja: o
/// `tray::available()` só vale depois que o laço da janela começa.
pub const ESPERA_DA_BANDEJA: std::time::Duration = std::time::Duration::from_secs(3);

/// O identificador do LaunchAgent do macOS (o nome do `.plist`, sem a extensão).
const ID_DO_LAUNCH_AGENT: &str = "com.tirech.omni";

/// A chave do registro do Windows onde ficam os programas que abrem com o login.
pub const CHAVE_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Se o app foi aberto com `--bandeja` (o início com o sistema).
pub fn aberto_na_bandeja() -> bool {
    std::env::args().skip(1).any(|a| a == ARGUMENTO_BANDEJA)
}

/// Se a entrada de início do sistema existe e se aponta para o executável de agora.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstadoDoInicio {
    /// Não há entrada.
    Desligado,
    /// Há entrada, com este executável.
    Ligado,
    /// Há entrada, mas para outro executável (o app foi movido): a tela a regrava ao abrir.
    OutroExecutavel,
}

/// O `Exec` do `.desktop` de início com o sistema: o caminho de [`exec_citado`] e o argumento
/// que abre o app escondido na bandeja.
fn exec_do_desktop(exe: &Path) -> String {
    format!("{} {ARGUMENTO_BANDEJA}", exec_citado(exe))
}

/// O caminho de um `Exec` de `.desktop`, entre aspas, com `"`, `` ` ``, `$` e `\` escapados como
/// manda a especificação (a barra invertida vale quatro, porque o escape de texto vem antes do
/// das aspas) e `%` dobrado (os códigos de campo).
fn exec_citado(exe: &Path) -> String {
    let mut saida = String::from("\"");
    for c in exe.display().to_string().chars() {
        match c {
            '"' | '`' | '$' => {
                saida.push('\\');
                saida.push(c);
            }
            '\\' => saida.push_str(r"\\\\"),
            '%' => saida.push_str("%%"),
            outro => saida.push(outro),
        }
    }
    saida.push('"');
    saida
}

/// O `~/.config/autostart/tirech-omni.desktop` do Linux.
pub fn entrada_desktop(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={NOME_DO_APP}\nExec={}\n\
         Icon={ID_DO_APP}\nStartupWMClass={ID_DO_APP}\nX-GNOME-Autostart-enabled=true\n",
        exec_do_desktop(exe)
    )
}

/// O `tirech-omni.desktop` do menu de aplicativos do Linux. Abre o app com a janela à vista (sem
/// o `--bandeja` do início com o sistema); o `StartupWMClass` é o `app_id` da janela, e é por ele
/// que o dock casa a janela com este atalho e usa o ícone dele.
pub fn entrada_do_menu(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={NOME_DO_APP}\n\
         Comment=Terminal SSH, arquivos e túneis\nExec={}\nIcon={ID_DO_APP}\nTerminal=false\n\
         Categories=Network;RemoteAccess;System;TerminalEmulator;\n\
         StartupWMClass={ID_DO_APP}\nStartupNotify=true\n",
        exec_citado(exe)
    )
}

/// O atalho do app no menu e no dock do Linux (M49): o `.desktop` em `applications/` e o ícone
/// em cada tamanho do tema `hicolor`, tudo na pasta de dados da pessoa. O app não chama
/// `update-desktop-database` nem atualiza o cache de ícones. O ambiente percebe o `.desktop`
/// novo sozinho; os ícones, não: o tema só é relido quando a data da pasta `hicolor` muda, e
/// gravar em `24x24/apps/` não muda essa data. Por isso, quando grava ou apaga um ícone, o app
/// atualiza a data da pasta (é o que o `xdg-icon-resource forceupdate` faz).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtalhoDoMenu {
    /// A pasta `applications` onde fica o `.desktop`.
    applications: PathBuf,
    /// A pasta `icons/hicolor`, com uma subpasta por tamanho.
    icones: PathBuf,
}

impl AtalhoDoMenu {
    /// O atalho do usuário. `xdg_data` é o `XDG_DATA_HOME` (só vale se for absoluto; senão,
    /// `~/.local/share`). Fora do Linux não há atalho.
    pub fn do_sistema(
        windows: bool,
        macos: bool,
        xdg_data: Option<&Path>,
        pessoal: Option<&Path>,
    ) -> Option<Self> {
        if windows || macos {
            return None;
        }
        let dados = match xdg_data.filter(|p| p.is_absolute()) {
            Some(pasta) => pasta.to_owned(),
            None => pessoal?.join(".local/share"),
        };
        Some(Self {
            applications: dados.join("applications"),
            icones: dados.join("icons/hicolor"),
        })
    }

    /// O `.desktop` de `id`.
    fn desktop_de(&self, id: &str) -> PathBuf {
        self.applications.join(format!("{id}.desktop"))
    }

    /// O `.desktop`.
    pub(crate) fn desktop(&self) -> PathBuf {
        self.desktop_de(ID_DO_APP)
    }

    /// Os caminhos dos ícones de `id`: cada tamanho do `hicolor` e o SVG.
    fn caminhos_dos_icones(&self, id: &str) -> Vec<PathBuf> {
        let mut todos: Vec<PathBuf> = icone::TAMANHOS
            .iter()
            .map(|&lado| self.icones.join(format!("{lado}x{lado}/apps/{id}.png")))
            .collect();
        todos.push(self.icones.join(format!("scalable/apps/{id}.svg")));
        todos
    }

    /// O ícone de cada tamanho do `hicolor` e o SVG, com o conteúdo.
    fn icones(&self) -> Vec<(PathBuf, &'static [u8])> {
        let mut todos: Vec<(PathBuf, &'static [u8])> = icone::TAMANHOS
            .iter()
            .zip(self.caminhos_dos_icones(ID_DO_APP))
            .filter_map(|(&lado, caminho)| Some((caminho, icone::png_gravado(lado)?)))
            .collect();
        todos.push((
            self.icones.join(format!("scalable/apps/{ID_DO_APP}.svg")),
            icone::GRANDE,
        ));
        todos
    }

    /// Se há `.desktop` de `id` (o do app de antes do M55, por exemplo).
    pub fn existe_de(&self, id: &str) -> bool {
        self.desktop_de(id).is_file()
    }

    /// Apaga o `.desktop` e os ícones de `id`: é como o M55 tira os do nome antigo.
    pub fn remover_de(&self, id: &str) -> io::Result<()> {
        let mut icone_apagado = false;
        let icones = self.caminhos_dos_icones(id);
        let caminhos = std::iter::once(self.desktop_de(id)).chain(icones.iter().cloned());
        for arquivo in caminhos {
            match fs::remove_file(&arquivo) {
                Err(erro) if erro.kind() != io::ErrorKind::NotFound => return Err(erro),
                Ok(()) if icones.contains(&arquivo) => icone_apagado = true,
                _ => {}
            }
        }
        if icone_apagado {
            self.avisar_o_tema();
        }
        Ok(())
    }

    /// Atualiza a data da pasta `hicolor`, para o ambiente reler o tema e achar (ou esquecer) o
    /// ícone sem reiniciar. Se não der, o ícone aparece no próximo início da sessão: não é erro
    /// que impeça o atalho.
    fn avisar_o_tema(&self) {
        let _ = fs::File::open(&self.icones)
            .and_then(|pasta| pasta.set_modified(std::time::SystemTime::now()));
    }

    /// Os arquivos do atalho com o conteúdo que ele deve ter: o `.desktop` de `exe` e os ícones.
    fn arquivos(&self, exe: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut todos = vec![(self.desktop(), entrada_do_menu(exe).into_bytes())];
        todos.extend(self.icones().into_iter().map(|(c, b)| (c, b.to_vec())));
        todos
    }

    /// Grava o atalho que abre `exe`. Só toca no arquivo que mudou: o que já tem o conteúdo
    /// certo fica como está, e o app não gera eventos de arquivo a cada início.
    pub fn instalar(&self, exe: &Path) -> io::Result<()> {
        let desktop = self.desktop();
        let mut icone_gravado = false;
        for (arquivo, bytes) in self.arquivos(exe) {
            if fs::read(&arquivo).is_ok_and(|atual| atual == bytes) {
                continue;
            }
            if let Some(pasta) = arquivo.parent() {
                fs::create_dir_all(pasta)?;
            }
            gravar_atomico(&arquivo, &bytes, false)?;
            icone_gravado |= arquivo != desktop;
        }
        if icone_gravado {
            self.avisar_o_tema();
        }
        Ok(())
    }

    /// Grava só os ícones (o `.desktop` da área de trabalho (M65) usa `Icon=tirech-omni`, e o
    /// tema precisa achá-lo mesmo sem o atalho no menu).
    pub(crate) fn instalar_icones(&self) -> io::Result<()> {
        let mut gravado = false;
        for (arquivo, bytes) in self.icones() {
            if fs::read(&arquivo).is_ok_and(|atual| atual == bytes) {
                continue;
            }
            if let Some(pasta) = arquivo.parent() {
                fs::create_dir_all(pasta)?;
            }
            gravar_atomico(&arquivo, bytes, false)?;
            gravado = true;
        }
        if gravado {
            self.avisar_o_tema();
        }
        Ok(())
    }

    /// Apaga o `.desktop` e os ícones que o app gravou. As pastas, mesmo vazias, ficam.
    pub fn remover(&self) -> io::Result<()> {
        self.remover_de(ID_DO_APP)
    }
}

/// Escapa o texto de um elemento de XML.
fn escapar_xml(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    for c in texto.chars() {
        match c {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '>' => saida.push_str("&gt;"),
            '"' => saida.push_str("&quot;"),
            '\'' => saida.push_str("&apos;"),
            outro => saida.push(outro),
        }
    }
    saida
}

/// O `~/Library/LaunchAgents/com.tirech.omni.plist` do macOS.
pub fn plist_launch_agent(exe: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \t<key>Label</key>\n\t<string>{ID_DO_LAUNCH_AGENT}</string>\n\
         \t<key>ProgramArguments</key>\n\t<array>\n\
         \t\t<string>{}</string>\n\t\t<string>{ARGUMENTO_BANDEJA}</string>\n\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n</dict>\n</plist>\n",
        escapar_xml(&exe.display().to_string())
    )
}

/// O valor do `HKCU\...\Run` do Windows.
pub fn linha_run(exe: &Path) -> String {
    format!("\"{}\" {ARGUMENTO_BANDEJA}", exe.display())
}

/// Onde fica a entrada de início de cada sistema, a partir da pasta pessoal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LugarDoInicio {
    /// O `.desktop` do Linux.
    Desktop(PathBuf),
    /// O `.plist` do macOS.
    LaunchAgent(PathBuf),
    /// O valor do registro do Windows, na chave (`HKCU\<chave>`).
    Registro(String),
}

impl LugarDoInicio {
    /// O lugar do sistema. `xdg_config` é o `XDG_CONFIG_HOME` (só vale se for absoluto).
    pub fn do_sistema(
        windows: bool,
        macos: bool,
        xdg_config: Option<&Path>,
        pessoal: Option<&Path>,
        chave_run: &str,
    ) -> Option<Self> {
        if windows {
            return Some(Self::Registro(chave_run.to_owned()));
        }
        let pessoal = pessoal?;
        if macos {
            return Some(Self::LaunchAgent(
                pessoal
                    .join("Library/LaunchAgents")
                    .join(format!("{ID_DO_LAUNCH_AGENT}.plist")),
            ));
        }
        let config = xdg_config
            .filter(|p| p.is_absolute())
            .map_or_else(|| pessoal.join(".config"), Path::to_owned);
        Some(Self::Desktop(config.join("autostart/tirech-omni.desktop")))
    }

    /// O que a entrada leva para abrir `exe`.
    fn conteudo(&self, exe: &Path) -> String {
        match self {
            Self::Desktop(_) => entrada_desktop(exe),
            Self::LaunchAgent(_) => plist_launch_agent(exe),
            Self::Registro(_) => linha_run(exe),
        }
    }

    /// O que a entrada tem agora, se existe.
    fn atual(&self) -> Option<String> {
        match self {
            Self::Desktop(arquivo) | Self::LaunchAgent(arquivo) => fs::read_to_string(arquivo).ok(),
            Self::Registro(chave) => ler_run(chave),
        }
    }

    /// Se a entrada existe e se é a de `exe`.
    pub fn estado(&self, exe: &Path) -> EstadoDoInicio {
        match self.atual() {
            None => EstadoDoInicio::Desligado,
            Some(texto) if texto == self.conteudo(exe) => EstadoDoInicio::Ligado,
            Some(_) => EstadoDoInicio::OutroExecutavel,
        }
    }

    /// Grava a entrada que abre `exe` com o sistema.
    pub fn ligar(&self, exe: &Path) -> io::Result<()> {
        let conteudo = self.conteudo(exe);
        match self {
            Self::Desktop(arquivo) | Self::LaunchAgent(arquivo) => {
                if let Some(pasta) = arquivo.parent() {
                    fs::create_dir_all(pasta)?;
                }
                gravar_atomico(arquivo, conteudo.as_bytes(), false)
            }
            Self::Registro(chave) => gravar_run(chave, &conteudo),
        }
    }

    /// Apaga a entrada (o que não existe já está apagado).
    pub fn desligar(&self) -> io::Result<()> {
        match self {
            Self::Desktop(arquivo) | Self::LaunchAgent(arquivo) => match fs::remove_file(arquivo) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            },
            Self::Registro(chave) => apagar_run(chave),
        }
    }
}

/// O valor de nome `nome` na chave `chave` do `HKCU`, se existe.
#[cfg(windows)]
pub(crate) fn ler_run_de(chave: &str, nome: &str) -> Option<String> {
    windows_registry::CURRENT_USER
        .open(chave)
        .ok()?
        .get_string(nome)
        .ok()
}

/// Grava o valor `nome` na chave `chave` do `HKCU`.
#[cfg(windows)]
pub(crate) fn gravar_run_de(chave: &str, nome: &str, linha: &str) -> io::Result<()> {
    let chave = windows_registry::CURRENT_USER
        .create(chave)
        .map_err(io::Error::other)?;
    chave.set_string(nome, linha).map_err(io::Error::other)
}

/// Apaga o valor `nome` da chave `chave` do `HKCU` (o que não existe já está apagado).
#[cfg(windows)]
pub(crate) fn apagar_run_de(chave: &str, nome: &str) -> io::Result<()> {
    // Abrir só para leitura (`open`) nega o `remove_value`: pede a escrita também.
    let Ok(chave) = windows_registry::CURRENT_USER
        .options()
        .read()
        .write()
        .open(chave)
    else {
        return Ok(());
    };
    if chave.get_string(nome).is_err() {
        return Ok(());
    }
    chave.remove_value(nome).map_err(io::Error::other)
}

#[cfg(not(windows))]
pub(crate) fn ler_run_de(_chave: &str, _nome: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub(crate) fn gravar_run_de(_chave: &str, _nome: &str, _linha: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        t!("sistema-registro-so-windows"),
    ))
}

#[cfg(not(windows))]
pub(crate) fn apagar_run_de(_chave: &str, _nome: &str) -> io::Result<()> {
    Ok(())
}

fn ler_run(chave: &str) -> Option<String> {
    ler_run_de(chave, NOME_DO_APP)
}

fn gravar_run(chave: &str, linha: &str) -> io::Result<()> {
    gravar_run_de(chave, NOME_DO_APP, linha)
}

fn apagar_run(chave: &str) -> io::Result<()> {
    apagar_run_de(chave, NOME_DO_APP)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;

    fn socket() -> PathBuf {
        PathBuf::from("/run/user/1000/tirech-omni/agent.sock")
    }

    /// No Windows, só confere que o processo de console ainda roda com a flag; a ausência da
    /// janela se confere na VM (roteiro do M65). Nos outros sistemas, que o comando não muda.
    #[test]
    fn sem_janela_devolve_o_comando_e_o_processo_roda() {
        #[cfg(windows)]
        let mut comando = {
            let mut c = std::process::Command::new("cmd.exe");
            c.args(["/C", "echo ok"]);
            c
        };
        #[cfg(not(windows))]
        let mut comando = {
            let mut c = std::process::Command::new("sh");
            c.args(["-c", "echo ok"]);
            c
        };
        let saida = sem_janela(&mut comando).output().expect("roda");
        assert!(saida.status.success());
        assert!(String::from_utf8_lossy(&saida.stdout).starts_with("ok"));
    }

    #[test]
    fn o_bloco_do_ssh_config_tem_o_match_e_o_identity_agent() {
        let bloco = bloco_ssh_config(&socket());
        assert_eq!(
            bloco,
            "# >>> tirech-omni: agente (gerado pelo Tirech Omni; \"Desligar\" na página Agente \
             remove) >>>\n\
             Match exec \"test -S '/run/user/1000/tirech-omni/agent.sock'\"\n    \
             IdentityAgent \"/run/user/1000/tirech-omni/agent.sock\"\n\
             Match all\n\
             # <<< tirech-omni: agente <<<"
        );
    }

    #[test]
    fn em_arquivo_vazio_o_bloco_e_tudo() {
        let bloco = bloco_ssh_config(&socket());
        let texto = com_bloco("", &bloco);
        assert_eq!(texto, format!("{bloco}\n"));
        assert_eq!(sem_bloco(&texto), "");
    }

    #[test]
    fn o_bloco_vai_antes_do_host_e_sai_sem_deixar_rastro() {
        let original = "Host exemplo\n    HostName 10.0.0.1\n    User ana\n";
        let bloco = bloco_ssh_config(&socket());
        let texto = com_bloco(original, &bloco);
        assert!(texto.starts_with("# >>> tirech-omni: agente"), "{texto}");
        assert!(texto.ends_with(original), "{texto}");
        assert_eq!(sem_bloco(&texto), original);
    }

    #[test]
    fn o_bloco_velho_e_trocado_sem_mexer_no_resto() {
        let velho = bloco_ssh_config(Path::new("/tmp/velho.sock"));
        let original = "# antes\nHost a\n  User x\n\n# depois, sem fim de linha";
        let texto = format!("{velho}\n\n{original}");
        let novo = bloco_ssh_config(&socket());
        let trocado = com_bloco(&texto, &novo);
        assert_eq!(trocado, format!("{novo}\n\n{original}"));
        assert_eq!(tem_bloco(&trocado), Some(socket()));
        assert_eq!(sem_bloco(&trocado), original);
    }

    /// O bloco do `~/.ssh/config` como o app o gravava antes do M55.
    fn bloco_do_nome_antigo() -> String {
        bloco_ssh_config(&socket())
            .replace("# >>> tirech-omni: agente", PREFIXO_INICIO_ANTIGO)
            .replace(FIM, FIM_ANTIGO)
    }

    #[test]
    fn o_bloco_com_o_nome_antigo_ainda_e_reconhecido() {
        let original = "Host a\n  User x\n";
        let texto = format!("{}\n\n{original}", bloco_do_nome_antigo());
        assert_eq!(tem_bloco(&texto), Some(socket()));
        assert_eq!(sem_bloco(&texto), original);
    }

    #[test]
    fn ligar_troca_o_bloco_do_nome_antigo_sem_deixar_duas_copias() {
        let original = "Host a\n  User x\n";
        let texto = format!("{}\n\n{original}", bloco_do_nome_antigo());
        let novo = bloco_ssh_config(&socket());
        let trocado = com_bloco(&texto, &novo);
        assert_eq!(trocado, format!("{novo}\n\n{original}"));
        assert!(!trocado.contains("tirech-ssh"), "{trocado}");
    }

    #[test]
    fn a_transformacao_do_nome_so_mexe_no_bloco_antigo() {
        let original = "Host a\n  User x\n";
        let antigo = format!("{original}\n{}\n", bloco_do_nome_antigo());
        let novo = com_o_nome_novo(&antigo).unwrap();
        assert!(novo.starts_with(original));
        assert_eq!(tem_bloco(&novo), Some(socket()));
        assert!(!novo.contains("tirech-ssh"), "{novo}");
        // O socket padrão de antes passa para o de agora; um escolhido pela pessoa fica.
        let padrao_antigo =
            bloco_do_nome_antigo().replace("/tirech-omni/agent.sock", "/tirech-ssh/agent.sock");
        let migrado = com_o_nome_novo(&padrao_antigo).unwrap();
        assert_eq!(tem_bloco(&migrado), Some(socket()));
        // Sem bloco antigo (já migrado ou sem bloco), nada a fazer.
        assert_eq!(com_o_nome_novo(&novo), None);
        assert_eq!(com_o_nome_novo(original), None);
    }

    #[test]
    fn o_fim_de_linha_do_arquivo_e_mantido() {
        let original = "Host a\r\n  User x\r\n";
        let bloco = bloco_ssh_config(&socket());
        let texto = com_bloco(original, &bloco);
        assert!(!texto.replace("\r\n", "").contains('\n'), "{texto:?}");
        assert!(texto.ends_with(original));
        assert_eq!(sem_bloco(&texto), original);
        // Trocar o bloco também fica em CRLF.
        let outro = bloco_ssh_config(Path::new("/tmp/outro.sock"));
        let trocado = com_bloco(&texto, &outro);
        assert!(!trocado.replace("\r\n", "").contains('\n'), "{trocado:?}");
        assert_eq!(tem_bloco(&trocado), Some(PathBuf::from("/tmp/outro.sock")));
    }

    #[test]
    fn sem_bloco_nao_mexe_em_arquivo_sem_bloco() {
        for original in ["", "Host a\n", "sem fim", "\n\nHost b\r\n"] {
            assert_eq!(sem_bloco(original), original);
        }
        assert_eq!(tem_bloco("Host a\n"), None);
    }

    #[test]
    fn caminho_com_aspas_e_lido_de_volta() {
        let estranho = PathBuf::from("/tmp/o'meu \"agente\"/a.sock");
        let texto = com_bloco("", &bloco_ssh_config(&estranho));
        assert_eq!(tem_bloco(&texto), Some(estranho.clone()));
        let sh = bloco_shell_posix(&estranho);
        assert_eq!(tem_bloco(&sh), Some(estranho.clone()));
        let fish = bloco_shell_fish(&estranho);
        assert_eq!(tem_bloco(&fish), Some(estranho));
    }

    #[test]
    fn o_bloco_dos_shells_entra_no_fim_e_sai_igual() {
        let bloco = bloco_shell_posix(&socket());
        assert_eq!(
            bloco,
            "# >>> tirech-omni: agente >>>\n\
             [ -S '/run/user/1000/tirech-omni/agent.sock' ] && export \
             SSH_AUTH_SOCK='/run/user/1000/tirech-omni/agent.sock'\n\
             # <<< tirech-omni: agente <<<"
        );
        for original in [
            "export PATH=$HOME/bin:$PATH\nalias l='ls -l'\n",
            "alias l='ls -l'\n\n",
            "",
        ] {
            let texto = com_bloco_em(original, &bloco, Lugar::Fim);
            assert!(texto.starts_with(original), "{texto:?}");
            assert!(texto.ends_with("agente <<<\n"), "{texto:?}");
            assert_eq!(tem_bloco(&texto), Some(socket()));
            assert_eq!(sem_bloco(&texto), original, "{texto:?}");
            // Pôr de novo troca, não duplica.
            assert_eq!(com_bloco_em(&texto, &bloco, Lugar::Fim), texto);
        }
    }

    #[test]
    fn o_bloco_do_fish_usa_test_e_set() {
        let bloco = bloco_shell_fish(&socket());
        assert!(bloco.contains(
            "test -S '/run/user/1000/tirech-omni/agent.sock'; and set -gx SSH_AUTH_SOCK \
             '/run/user/1000/tirech-omni/agent.sock'"
        ));
        let texto = com_bloco_em("", &bloco, Lugar::Fim);
        assert_eq!(sem_bloco(&texto), "");
    }

    #[test]
    fn arquivo_sem_fim_de_linha_ganha_um_antes_do_bloco_no_fim() {
        let bloco = bloco_shell_posix(&socket());
        let texto = com_bloco_em("alias a=b", &bloco, Lugar::Fim);
        assert!(texto.starts_with("alias a=b\n\n# >>>"), "{texto:?}");
    }

    #[test]
    fn gravar_cria_a_pasta_o_arquivo_e_faz_backup_do_que_existia() {
        let pasta = PastaTemporaria::nova("sistema-gravar");
        let ssh = pasta.0.join(".ssh");
        let config = ssh.join("config");
        let bloco = bloco_ssh_config(&socket());

        // Sem `~/.ssh` nem config: cria os dois.
        assert!(aplicar_bloco(&config, Some(&bloco), Lugar::Topo, true, None).unwrap());
        assert_eq!(bloco_do_arquivo(&config), Some(socket()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(modo(&ssh), 0o700);
            assert_eq!(modo(&config), 0o600);
        }
        // Igual: nada a fazer.
        assert!(!aplicar_bloco(&config, Some(&bloco), Lugar::Topo, true, None).unwrap());

        // Tirar volta ao arquivo vazio.
        assert!(aplicar_bloco(&config, None, Lugar::Topo, true, None).unwrap());
        assert_eq!(fs::read_to_string(&config).unwrap(), "");

        // Com texto da pessoa e o backup pedido.
        fs::write(&config, "Host a\n  User x\n").unwrap();
        let backup = ssh.join("config.tirech-bak");
        assert!(aplicar_bloco(&config, Some(&bloco), Lugar::Topo, true, Some(&backup)).unwrap());
        assert_eq!(fs::read_to_string(&backup).unwrap(), "Host a\n  User x\n");
        assert!(aplicar_bloco(&config, None, Lugar::Topo, true, None).unwrap());
        assert_eq!(fs::read_to_string(&config).unwrap(), "Host a\n  User x\n");
    }

    #[test]
    fn arquivo_que_nao_existe_so_e_criado_se_pedido() {
        let pasta = PastaTemporaria::nova("sistema-nao-cria");
        fs::create_dir_all(&pasta.0).unwrap();
        let rc = pasta.0.join(".zshrc");
        let bloco = bloco_shell_posix(&socket());
        assert!(!aplicar_bloco(&rc, Some(&bloco), Lugar::Fim, false, None).unwrap());
        assert!(!rc.exists());
        assert!(!aplicar_bloco(&rc, None, Lugar::Fim, false, None).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn link_simbolico_escreve_no_destino_e_mantem_a_permissao() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let pasta = PastaTemporaria::nova("sistema-link");
        let dotfiles = pasta.0.join("dotfiles");
        fs::create_dir_all(&dotfiles).unwrap();
        let real = dotfiles.join("bashrc");
        fs::write(&real, "alias a=b\n").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o644)).unwrap();
        let link = pasta.0.join(".bashrc");
        symlink(&real, &link).unwrap();

        let bloco = bloco_shell_posix(&socket());
        assert!(aplicar_bloco(&link, Some(&bloco), Lugar::Fim, false, None).unwrap());
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&real).unwrap().contains("tirech-omni"));
        assert_eq!(
            fs::metadata(&real).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn arquivo_fora_de_utf8_nao_e_tocado() {
        let pasta = PastaTemporaria::nova("sistema-utf8");
        fs::create_dir_all(&pasta.0).unwrap();
        let rc = pasta.0.join(".bashrc");
        fs::write(&rc, [b'a', 0xff, b'\n']).unwrap();
        let bloco = bloco_shell_posix(&socket());
        let erro = aplicar_bloco(&rc, Some(&bloco), Lugar::Fim, false, None).unwrap_err();
        assert_eq!(erro.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(&rc).unwrap(), [b'a', 0xff, b'\n']);
    }

    #[test]
    fn o_exec_do_desktop_escapa_espaco_aspas_cifrao_e_barra() {
        let simples = entrada_desktop(Path::new("/opt/tirech ssh/tirech-omni"));
        assert!(simples.starts_with("[Desktop Entry]\nType=Application\nName=Tirech Omni\n"));
        assert!(simples.contains("Exec=\"/opt/tirech ssh/tirech-omni\" --bandeja\n"));
        assert!(simples.contains("Icon=tirech-omni\nStartupWMClass=tirech-omni\n"));
        assert!(simples.ends_with("X-GNOME-Autostart-enabled=true\n"));

        let esquisito = entrada_desktop(Path::new("/a \"b\"/$c/`d`/e\\f/100%"));
        assert!(
            esquisito.contains(r#"Exec="/a \"b\"/\$c/\`d\`/e\\\\f/100%%" --bandeja"#),
            "{esquisito}"
        );
    }

    #[test]
    fn a_entrada_do_menu_abre_o_app_com_a_janela_e_o_icone() {
        let entrada = entrada_do_menu(Path::new("/opt/tirech ssh/tirech-omni"));
        assert_eq!(
            entrada,
            "[Desktop Entry]\nType=Application\nName=Tirech Omni\n\
             Comment=Terminal SSH, arquivos e túneis\n\
             Exec=\"/opt/tirech ssh/tirech-omni\"\nIcon=tirech-omni\nTerminal=false\n\
             Categories=Network;RemoteAccess;System;TerminalEmulator;\n\
             StartupWMClass=tirech-omni\nStartupNotify=true\n"
        );
        // Sem o `--bandeja`: quem clica no menu quer ver a janela.
        assert!(!entrada.contains(ARGUMENTO_BANDEJA));
        let esquisito = entrada_do_menu(Path::new("/a \"b\"/$c/`d`/e\\f/100%"));
        assert!(
            esquisito.contains("Exec=\"/a \\\"b\\\"/\\$c/\\`d\\`/e\\\\\\\\f/100%%\"\n"),
            "{esquisito}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn o_atalho_do_menu_fica_na_pasta_de_dados() {
        let casa = Path::new("/home/ana");
        let atalho = |windows, macos, xdg: Option<&str>| {
            AtalhoDoMenu::do_sistema(windows, macos, xdg.map(Path::new), Some(casa))
        };
        let em = |dados: &str| AtalhoDoMenu {
            applications: Path::new(dados).join("applications"),
            icones: Path::new(dados).join("icons/hicolor"),
        };
        assert_eq!(
            atalho(false, false, None),
            Some(em("/home/ana/.local/share"))
        );
        assert_eq!(
            atalho(false, false, Some("/home/ana/.dados")),
            Some(em("/home/ana/.dados"))
        );
        // `XDG_DATA_HOME` relativo ou vazio não vale.
        assert_eq!(
            atalho(false, false, Some("relativo")),
            Some(em("/home/ana/.local/share"))
        );
        assert_eq!(
            atalho(false, false, Some("")),
            Some(em("/home/ana/.local/share"))
        );
        assert_eq!(atalho(true, false, None), None);
        assert_eq!(atalho(false, true, None), None);
        assert_eq!(AtalhoDoMenu::do_sistema(false, false, None, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn instalar_o_atalho_grava_uma_vez_regrava_so_o_desktop_e_remover_apaga() {
        let pasta = PastaTemporaria::nova("sistema-atalho");
        let atalho = AtalhoDoMenu::do_sistema(false, false, Some(&pasta.0), None).unwrap();
        let exe = Path::new("/opt/tirech/tirech-omni");
        let desktop = pasta.0.join("applications/tirech-omni.desktop");
        let svg = pasta.0.join("icons/hicolor/scalable/apps/tirech-omni.svg");
        let pngs: Vec<_> = icone::TAMANHOS
            .iter()
            .map(|n| {
                pasta
                    .0
                    .join(format!("icons/hicolor/{n}x{n}/apps/tirech-omni.png"))
            })
            .collect();
        let todos: Vec<_> = [desktop.clone(), svg.clone()]
            .into_iter()
            .chain(pngs.clone())
            .collect();
        let datas = |todos: &[PathBuf]| -> Vec<std::time::SystemTime> {
            todos
                .iter()
                .map(|f| fs::metadata(f).unwrap().modified().unwrap())
                .collect()
        };

        // A pasta do tema já existe, com outros ícones, e com uma data antiga: gravar o ícone
        // tem de mudar essa data, senão o ambiente não relê o tema.
        let tema = pasta.0.join("icons/hicolor");
        let antiga = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let envelhecer_o_tema = || {
            fs::File::open(&tema).unwrap().set_modified(antiga).unwrap();
        };
        let data_do_tema = || fs::metadata(&tema).unwrap().modified().unwrap();
        for icone in pngs.iter().chain([&svg]) {
            fs::create_dir_all(icone.parent().unwrap()).unwrap();
        }
        envelhecer_o_tema();

        atalho.instalar(exe).unwrap();
        assert_eq!(fs::read_to_string(&desktop).unwrap(), entrada_do_menu(exe));
        assert_eq!(fs::read(&svg).unwrap(), icone::GRANDE);
        for (png, &lado) in pngs.iter().zip(icone::TAMANHOS) {
            assert_eq!(fs::read(png).unwrap(), icone::png(lado), "{lado} px");
        }
        assert!(data_do_tema() > antiga, "gravar os ícones avisa o tema");

        // A segunda vez não toca em nada, nem na pasta do tema.
        let antes = datas(&todos);
        envelhecer_o_tema();
        std::thread::sleep(std::time::Duration::from_millis(30));
        atalho.instalar(exe).unwrap();
        assert_eq!(datas(&todos), antes);
        assert_eq!(data_do_tema(), antiga);

        // O app foi movido: só o `.desktop` é regravado, e o tema fica como está.
        std::thread::sleep(std::time::Duration::from_millis(30));
        let novo = Path::new("/opt/novo/tirech-omni");
        atalho.instalar(novo).unwrap();
        assert_eq!(fs::read_to_string(&desktop).unwrap(), entrada_do_menu(novo));
        let depois = datas(&todos);
        assert_ne!(depois[0], antes[0]);
        assert_eq!(depois[1..], antes[1..]);
        assert_eq!(data_do_tema(), antiga);

        atalho.remover().unwrap();
        assert!(todos.iter().all(|f| !f.exists()));
        assert!(data_do_tema() > antiga, "apagar os ícones avisa o tema");
        // Apagar o que já não existe não é erro.
        atalho.remover().unwrap();
    }

    #[test]
    fn o_plist_do_launch_agent_escapa_o_xml() {
        let plist = plist_launch_agent(Path::new("/Apps/Tirech & <Cia>/tirech-omni"));
        assert!(plist.contains("<string>com.tirech.omni</string>"));
        assert!(plist.contains("<string>/Apps/Tirech &amp; &lt;Cia&gt;/tirech-omni</string>"));
        assert!(plist.contains("<string>--bandeja</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n\t<true/>"));
    }

    #[test]
    fn a_linha_do_run_leva_o_caminho_entre_aspas() {
        assert_eq!(
            linha_run(Path::new(r"C:\Program Files\Tirech Omni\tirech-omni.exe")),
            r#""C:\Program Files\Tirech Omni\tirech-omni.exe" --bandeja"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn o_lugar_da_entrada_de_cada_sistema() {
        let casa = Path::new("/home/ana");
        let xdg = Path::new("/home/ana/.cfg");
        let lugar = |windows, macos, xdg| {
            LugarDoInicio::do_sistema(windows, macos, xdg, Some(casa), CHAVE_RUN).unwrap()
        };
        assert_eq!(
            lugar(false, false, None),
            LugarDoInicio::Desktop("/home/ana/.config/autostart/tirech-omni.desktop".into())
        );
        assert_eq!(
            lugar(false, false, Some(xdg)),
            LugarDoInicio::Desktop("/home/ana/.cfg/autostart/tirech-omni.desktop".into())
        );
        assert_eq!(
            lugar(false, false, Some(Path::new("relativo"))),
            LugarDoInicio::Desktop("/home/ana/.config/autostart/tirech-omni.desktop".into())
        );
        assert_eq!(
            lugar(false, true, Some(xdg)),
            LugarDoInicio::LaunchAgent(
                "/home/ana/Library/LaunchAgents/com.tirech.omni.plist".into()
            )
        );
        assert_eq!(
            lugar(true, false, None),
            LugarDoInicio::Registro(CHAVE_RUN.to_owned())
        );
        assert_eq!(
            LugarDoInicio::do_sistema(false, false, None, None, CHAVE_RUN),
            None
        );
    }

    #[test]
    fn ligar_e_desligar_a_entrada_numa_pasta_pessoal_de_teste() {
        let pasta = PastaTemporaria::nova("sistema-inicio");
        for macos in [false, true] {
            let lugar =
                LugarDoInicio::do_sistema(false, macos, None, Some(&pasta.0), CHAVE_RUN).unwrap();
            let exe = Path::new("/opt/tirech/tirech-omni");
            assert_eq!(lugar.estado(exe), EstadoDoInicio::Desligado);
            lugar.ligar(exe).unwrap();
            assert_eq!(lugar.estado(exe), EstadoDoInicio::Ligado);
            // O app foi movido: a entrada ainda existe, mas aponta para o lugar velho.
            let novo = Path::new("/opt/novo/tirech-omni");
            assert_eq!(lugar.estado(novo), EstadoDoInicio::OutroExecutavel);
            lugar.ligar(novo).unwrap();
            assert_eq!(lugar.estado(novo), EstadoDoInicio::Ligado);
            lugar.desligar().unwrap();
            assert_eq!(lugar.estado(novo), EstadoDoInicio::Desligado);
            // Apagar o que já não existe não é erro.
            lugar.desligar().unwrap();
        }
    }

    #[cfg(windows)]
    #[test]
    fn o_valor_do_registro_grava_e_apaga_numa_chave_de_teste() {
        let chave = r"Software\TirechOmniTeste\Run";
        let lugar = LugarDoInicio::Registro(chave.to_owned());
        let exe = Path::new(r"C:\Tirech Omni\tirech-omni.exe");
        lugar.ligar(exe).unwrap();
        assert_eq!(lugar.estado(exe), EstadoDoInicio::Ligado);
        lugar.desligar().unwrap();
        assert_eq!(lugar.estado(exe), EstadoDoInicio::Desligado);
        let _ = windows_registry::CURRENT_USER.remove_tree(r"Software\TirechOmniTeste");
    }
}
