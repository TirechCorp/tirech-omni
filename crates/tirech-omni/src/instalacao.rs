//! Instalar no computador (M65): copiar o app para uma pasta fixa da pessoa e pôr o atalho na
//! área de trabalho, no menu, na barra de tarefas (ou no painel, ou no Dock), o início com o
//! sistema e a entrada de "Aplicativos instalados" do Windows. E tirar tudo isso.
//!
//! O módulo é feito para rodar fora da thread da interface: o [`Contexto`] reúne só o que as
//! operações precisam (caminhos, o sistema, um executor de programas que é `Send`) e é montado a
//! partir do [`Ambiente`]. Tudo o que muda o disco recebe o contexto, e os testes dão o seu, com
//! pastas temporárias e um executor que só anota a linha de comando.

use crate::ferramentas::{Ambiente, ExecutorEnviavel};
use crate::lnk::{self, Atalho};
use crate::marca::{ID_DO_APP, NOME_DO_APP};
use crate::sistema::{self, AtalhoDoMenu, EstadoDoInicio, LugarDoInicio};
use crate::t;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A chave do registro (`HKCU\...`) de "Aplicativos instalados" do Windows.
pub const CHAVE_UNINSTALL: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\TirechOmni";

/// O argumento com que o Windows (e o menu) pede para desinstalar.
pub const ARGUMENTO_DESINSTALAR: &str = "--desinstalar";

const CHAVE_PASTAS_DO_USUARIO: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders";
const CHAVE_PASTAS_DO_SHELL: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders";

/// Os programas que a cópia leva: o app e as ferramentas de linha de comando do lado dele.
const EXECUTAVEIS: [&str; 5] = [
    "tirech-omni",
    "tirech-omni-keygen",
    "tirech-omni-agent",
    "tirech-omni-add",
    "tirech-omni-copy-id",
];

/// O applet de lista de janelas do Cinnamon, onde ficam os fixados.
const APPLET_DO_CINNAMON: &str = "grouped-window-list@cinnamon.org";

/// Os itens do diálogo "Instalar no computador", na ordem em que se mostram e se aplicam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemDeInstalacao {
    /// A cópia do app na pasta fixa da pessoa.
    Copia,
    /// O atalho na área de trabalho.
    AreaDeTrabalho,
    /// O atalho no menu Iniciar (Windows) ou de aplicativos (Linux).
    Menu,
    /// Fixado na barra de tarefas (Windows), no painel (Linux) ou no Dock (macOS).
    Barra,
    /// Abrir com o sistema.
    Inicio,
    /// A entrada de "Aplicativos instalados" (só no Windows).
    Aplicativos,
}

impl ItemDeInstalacao {
    /// Todos, na ordem de instalar (a de desinstalar é a contrária).
    pub const TODOS: [Self; 6] = [
        Self::Copia,
        Self::AreaDeTrabalho,
        Self::Menu,
        Self::Barra,
        Self::Inicio,
        Self::Aplicativos,
    ];
}

/// Por que um item vale no sistema mas não dá agora.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motivo {
    /// O ambiente não tem a pasta da área de trabalho.
    SemAreaDeTrabalho,
    /// O item mexe na cópia instalada, e ela não existe.
    PrecisaDaCopia,
    /// O painel deste ambiente não deixa o app fixar sozinho.
    PainelNaoSuportado,
    /// O app não roda de dentro de um `.app` (macOS).
    SemBundle,
}

/// Como um item está no disco agora.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EstadoDoItem {
    /// O item não existe neste sistema.
    NaoSeAplica,
    /// Existe no sistema, mas não dá agora.
    Indisponivel(Motivo),
    /// Não está feito.
    Ausente,
    /// Está feito e aponta para o executável certo.
    Presente,
    /// Está feito, mas com outro executável (ou com outra versão da cópia).
    Desatualizado,
}

impl EstadoDoItem {
    /// Se o item está no disco (certo ou desatualizado).
    pub fn instalado(self) -> bool {
        matches!(self, Self::Presente | Self::Desatualizado)
    }
}

/// O passo que só a pessoa pode dar, e que o resultado mostra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassoManual {
    /// Fixar na barra de tarefas do Windows pelo menu Iniciar. `depois_de_abrir_a_copia` diz
    /// que o app está rodando de fora da cópia: o fixado só aponta para a cópia se for feito
    /// depois de abri-la.
    FixarNaBarraDoWindows { depois_de_abrir_a_copia: bool },
    /// Desafixar da barra de tarefas do Windows.
    DesafixarDaBarraDoWindows,
    /// Arrastar o ícone para fora do Dock (o app não reescreve o Dock inteiro).
    TirarDoDock,
}

/// O que as operações precisam do ambiente. É `Clone` e `Send`: vai para a thread de trabalho.
#[derive(Clone)]
pub struct Contexto {
    /// Se o sistema é o Windows.
    pub windows: bool,
    /// Se o sistema é o macOS.
    pub macos: bool,
    /// O executável que está rodando.
    pub exe: Option<PathBuf>,
    /// A pasta pessoal.
    pub pessoal: Option<PathBuf>,
    /// O `%LOCALAPPDATA%`.
    pub local_app_data: Option<PathBuf>,
    /// O `%APPDATA%`.
    pub app_data: Option<PathBuf>,
    /// A pasta da área de trabalho.
    pub area_de_trabalho: Option<PathBuf>,
    /// A pasta de atalhos do menu Iniciar.
    pub menu_iniciar: Option<PathBuf>,
    /// O `XDG_CONFIG_HOME`.
    pub xdg_config: Option<PathBuf>,
    /// O `XDG_DATA_HOME`.
    pub xdg_data: Option<PathBuf>,
    /// O `XDG_CURRENT_DESKTOP`.
    pub desktop_atual: String,
    /// A chave do registro do início com o sistema.
    pub chave_run: String,
    /// A chave do registro de "Aplicativos instalados".
    pub chave_uninstall: String,
    /// Só para os testes visuais: se não está vazia, vale no lugar do disco.
    pub estados_fixos: Vec<(ItemDeInstalacao, EstadoDoItem)>,
    /// Roda `gio`, `gsettings`, `gdbus`, `defaults`, `killall`.
    pub executor: ExecutorEnviavel,
}

impl Contexto {
    /// O contexto do ambiente `a`.
    pub fn de(a: &Ambiente) -> Self {
        Self {
            windows: a.windows,
            macos: a.macos,
            exe: a.exe.clone(),
            pessoal: a.pessoal.clone(),
            local_app_data: a.local_app_data.clone(),
            app_data: a.app_data.clone(),
            area_de_trabalho: a.area_de_trabalho.clone(),
            menu_iniciar: a.menu_iniciar.clone(),
            xdg_config: a.xdg_config.clone(),
            xdg_data: a.xdg_data.clone(),
            desktop_atual: a.desktop_atual.clone(),
            chave_run: a.chave_run.clone(),
            chave_uninstall: a.chave_uninstall.clone(),
            estados_fixos: a.instalacao_fixa.clone(),
            executor: a.executor_enviavel.clone(),
        }
    }

    fn linux(&self) -> bool {
        !self.windows && !self.macos
    }

    /// O `.app` de onde o executável de agora roda (macOS).
    fn bundle_atual(&self) -> Option<PathBuf> {
        bundle_de(self.exe.as_deref()?)
    }

    /// A pasta da cópia: `%LOCALAPPDATA%\Programs\Tirech Omni`, `~/.local/bin` ou
    /// `~/Applications/Tirech Omni.app`.
    pub fn pasta_instalada(&self) -> Option<PathBuf> {
        if self.windows {
            let local = self.local_app_data.clone().or_else(|| {
                self.pessoal
                    .as_ref()
                    .map(|p| p.join("AppData").join("Local"))
            })?;
            return Some(local.join("Programs").join(NOME_DO_APP));
        }
        let pessoal = self.pessoal.as_ref()?;
        if self.macos {
            return Some(
                pessoal
                    .join("Applications")
                    .join(format!("{NOME_DO_APP}.app")),
            );
        }
        Some(pessoal.join(".local").join("bin"))
    }

    /// O executável da cópia (existindo ou não).
    pub fn exe_instalado(&self) -> Option<PathBuf> {
        let pasta = self.pasta_instalada()?;
        if self.windows {
            return Some(pasta.join(format!("{ID_DO_APP}.exe")));
        }
        if self.macos {
            let nome = self
                .exe
                .as_deref()
                .and_then(Path::file_name)
                .map_or_else(|| ID_DO_APP.into(), ToOwned::to_owned);
            return Some(pasta.join("Contents").join("MacOS").join(nome));
        }
        Some(pasta.join(ID_DO_APP))
    }

    /// O executável que os atalhos e o início com o sistema abrem: o da cópia, se ela existe;
    /// senão, o que está rodando.
    pub fn exe_alvo(&self) -> Option<PathBuf> {
        self.exe_instalado()
            .filter(|e| e.is_file())
            .or_else(|| self.exe.clone())
    }

    /// Se o app roda da própria cópia.
    pub fn executando_da_copia(&self) -> bool {
        match (&self.exe, self.exe_instalado()) {
            (Some(atual), Some(copia)) => mesmo_arquivo(atual, &copia),
            _ => false,
        }
    }

    /// Tira a cópia na desinstalação, mesmo com o app rodando dela. No Windows o executável em
    /// uso não se apaga: devolve a pasta, que quem chama apaga depois que o app sair. Nos outros
    /// sistemas, apagar um arquivo aberto vale, e o resultado é `None`.
    pub fn desinstalar_copia(&self) -> io::Result<Option<PathBuf>> {
        if self.executando_da_copia() && self.windows {
            return Ok(self.pasta_instalada());
        }
        self.remover_copia().map(|()| None)
    }

    /// O `.app` que os atalhos do macOS apontam: a cópia, se existe; senão, o de agora.
    fn bundle_alvo(&self) -> Option<PathBuf> {
        let copia = self
            .pasta_instalada()
            .filter(|_| self.exe_instalado().is_some_and(|e| e.is_file()));
        copia.or_else(|| self.bundle_atual())
    }

    fn atalho_do_menu(&self) -> Option<AtalhoDoMenu> {
        AtalhoDoMenu::do_sistema(
            self.windows,
            self.macos,
            self.xdg_data.as_deref(),
            self.pessoal.as_deref(),
        )
    }

    fn lugar_do_inicio(&self) -> Option<LugarDoInicio> {
        LugarDoInicio::do_sistema(
            self.windows,
            self.macos,
            self.xdg_config.as_deref(),
            self.pessoal.as_deref(),
            &self.chave_run,
        )
    }

    /// A pasta de configuração do Linux (`XDG_CONFIG_HOME` ou `~/.config`).
    fn pasta_de_config(&self) -> Option<PathBuf> {
        self.xdg_config
            .clone()
            .filter(|p| p.is_absolute())
            .or_else(|| self.pessoal.as_ref().map(|p| p.join(".config")))
    }

    fn rodar(&self, programa: &str, argumentos: &[&str]) -> io::Result<String> {
        (self.executor)(programa, argumentos)
    }

    /// O arquivo do atalho da área de trabalho.
    fn atalho_da_area(&self) -> Option<PathBuf> {
        let area = self.area_de_trabalho.as_ref()?;
        Some(if self.windows {
            area.join(format!("{NOME_DO_APP}.lnk"))
        } else if self.macos {
            area.join(format!("{NOME_DO_APP}.app"))
        } else {
            area.join(id_desktop())
        })
    }

    /// O `.lnk` do menu Iniciar.
    fn atalho_do_iniciar(&self) -> Option<PathBuf> {
        Some(
            self.menu_iniciar
                .as_ref()?
                .join(format!("{NOME_DO_APP}.lnk")),
        )
    }

    /// A pasta onde o Windows guarda os atalhos fixados na barra de tarefas.
    fn pasta_dos_fixados(&self) -> Option<PathBuf> {
        Some(
            self.app_data
                .as_ref()?
                .join("Microsoft")
                .join("Internet Explorer")
                .join("Quick Launch")
                .join("User Pinned")
                .join("TaskBar"),
        )
    }

    /// O arquivo (ou a pasta) que o item usa, para o diálogo mostrar o caminho. `None` quando o
    /// item não é um arquivo (o registro, os fixados).
    pub fn lugar(&self, item: ItemDeInstalacao) -> Option<PathBuf> {
        match item {
            ItemDeInstalacao::Copia => self.pasta_instalada(),
            ItemDeInstalacao::AreaDeTrabalho => self.atalho_da_area(),
            ItemDeInstalacao::Menu => {
                if self.windows {
                    self.atalho_do_iniciar()
                } else {
                    self.atalho_do_menu().map(|a| a.desktop())
                }
            }
            ItemDeInstalacao::Inicio => match self.lugar_do_inicio()? {
                LugarDoInicio::Desktop(arquivo) | LugarDoInicio::LaunchAgent(arquivo) => {
                    Some(arquivo)
                }
                LugarDoInicio::Registro(_) => None,
            },
            ItemDeInstalacao::Barra | ItemDeInstalacao::Aplicativos => None,
        }
    }

    /// O caminho como o diálogo o mostra: com `\` no Windows e `~` no resto.
    pub fn mostrar(&self, caminho: &Path) -> String {
        if self.windows {
            caminho.display().to_string().replace('/', "\\")
        } else {
            crate::armazenamento::com_til_em(caminho, self.pessoal.as_deref())
        }
    }

    // ------------------------------------------------------------------ estado

    /// Como o item está no disco agora.
    pub fn estado(&self, item: ItemDeInstalacao) -> EstadoDoItem {
        if !self.estados_fixos.is_empty() {
            return self
                .estados_fixos
                .iter()
                .find(|(i, _)| *i == item)
                .map_or(EstadoDoItem::NaoSeAplica, |(_, e)| *e);
        }
        match item {
            ItemDeInstalacao::Copia => self.estado_da_copia(),
            ItemDeInstalacao::AreaDeTrabalho => self.estado_da_area(),
            ItemDeInstalacao::Menu => self.estado_do_menu(),
            ItemDeInstalacao::Barra => self.estado_da_barra(),
            ItemDeInstalacao::Inicio => self.estado_do_inicio(),
            ItemDeInstalacao::Aplicativos => self.estado_dos_aplicativos(),
        }
    }

    /// O estado de todos os itens, na ordem de [`ItemDeInstalacao::TODOS`].
    pub fn situacao(&self) -> Vec<(ItemDeInstalacao, EstadoDoItem)> {
        ItemDeInstalacao::TODOS
            .into_iter()
            .map(|i| (i, self.estado(i)))
            .collect()
    }

    /// Se algum item está no disco (é o que mostra o botão "Desinstalar…").
    pub fn algo_instalado(&self) -> bool {
        ItemDeInstalacao::TODOS
            .into_iter()
            .any(|i| self.estado(i).instalado())
    }

    fn estado_da_copia(&self) -> EstadoDoItem {
        let (Some(atual), Some(instalado)) = (self.exe.as_deref(), self.exe_instalado()) else {
            return EstadoDoItem::NaoSeAplica;
        };
        if self.executando_da_copia() {
            return EstadoDoItem::Presente;
        }
        // Instalado por pacote (`/usr`, `/opt`): o app já está no lugar. No macOS, só de dentro
        // de um `.app`.
        if self.linux() && (atual.starts_with("/usr") || atual.starts_with("/opt")) {
            return EstadoDoItem::NaoSeAplica;
        }
        if self.macos && self.bundle_atual().is_none() {
            return EstadoDoItem::NaoSeAplica;
        }
        if !instalado.is_file() {
            return EstadoDoItem::Ausente;
        }
        if mesmo_conteudo(atual, &instalado) {
            EstadoDoItem::Presente
        } else {
            EstadoDoItem::Desatualizado
        }
    }

    fn estado_da_area(&self) -> EstadoDoItem {
        let Some(area) = self.area_de_trabalho.as_deref() else {
            return EstadoDoItem::Indisponivel(Motivo::SemAreaDeTrabalho);
        };
        if !self.windows && !area.is_dir() {
            return EstadoDoItem::Indisponivel(Motivo::SemAreaDeTrabalho);
        }
        if self.macos && self.bundle_alvo().is_none() {
            return EstadoDoItem::Indisponivel(Motivo::SemBundle);
        }
        let (Some(arquivo), Some(exe)) = (self.atalho_da_area(), self.exe_alvo()) else {
            return EstadoDoItem::NaoSeAplica;
        };
        if self.windows {
            estado_do_lnk(&arquivo, &exe)
        } else if self.macos {
            match fs::read_link(&arquivo) {
                Ok(alvo) if Some(&alvo) == self.bundle_alvo().as_ref() => EstadoDoItem::Presente,
                Ok(_) => EstadoDoItem::Desatualizado,
                Err(_) => EstadoDoItem::Ausente,
            }
        } else {
            estado_do_texto(&arquivo, &entrada_do_menu_do_app(&exe))
        }
    }

    fn estado_do_menu(&self) -> EstadoDoItem {
        let Some(exe) = self.exe_alvo() else {
            return EstadoDoItem::NaoSeAplica;
        };
        if self.windows {
            return match self.atalho_do_iniciar() {
                Some(arquivo) => estado_do_lnk(&arquivo, &exe),
                None => EstadoDoItem::NaoSeAplica,
            };
        }
        if self.macos {
            // A cópia em `~/Applications` já é o item do menu.
            return EstadoDoItem::NaoSeAplica;
        }
        match self.atalho_do_menu() {
            Some(atalho) => estado_do_texto(&atalho.desktop(), &entrada_do_menu_do_app(&exe)),
            None => EstadoDoItem::NaoSeAplica,
        }
    }

    fn estado_da_barra(&self) -> EstadoDoItem {
        let Some(exe) = self.exe_alvo() else {
            return EstadoDoItem::NaoSeAplica;
        };
        if self.windows {
            let presente = self.pasta_dos_fixados().is_some_and(|pasta| {
                lnks_da_pasta(&pasta)
                    .iter()
                    .any(|(_, atalho)| mesmo_caminho_de_texto(&atalho.alvo, &exe))
            });
            return if presente {
                EstadoDoItem::Presente
            } else {
                EstadoDoItem::Ausente
            };
        }
        if self.macos {
            if self.bundle_alvo().is_none() {
                return EstadoDoItem::Indisponivel(Motivo::SemBundle);
            }
            let presente = self
                .rodar("defaults", &["read", "com.apple.dock", "persistent-apps"])
                .is_ok_and(|saida| dock_tem(&saida));
            return if presente {
                EstadoDoItem::Presente
            } else {
                EstadoDoItem::Ausente
            };
        }
        let Some(painel) = painel_do(&self.desktop_atual) else {
            return EstadoDoItem::Indisponivel(Motivo::PainelNaoSuportado);
        };
        match painel {
            Painel::Cinnamon => {
                let arquivos = self.fixados_do_cinnamon();
                if arquivos.is_empty() {
                    return EstadoDoItem::Indisponivel(Motivo::PainelNaoSuportado);
                }
                let todos = arquivos
                    .iter()
                    .all(|a| fs::read_to_string(a).is_ok_and(|texto| json_do_cinnamon_tem(&texto)));
                if todos {
                    EstadoDoItem::Presente
                } else {
                    EstadoDoItem::Ausente
                }
            }
            Painel::Gnome => {
                let lista = self
                    .rodar("gsettings", &["get", "org.gnome.shell", "favorite-apps"])
                    .map(|s| ler_lista_do_gsettings(&s))
                    .unwrap_or_default();
                if lista.contains(&id_desktop()) {
                    EstadoDoItem::Presente
                } else {
                    EstadoDoItem::Ausente
                }
            }
            Painel::Kde => {
                let tem = self
                    .pasta_de_config()
                    .map(|c| c.join("plasma-org.kde.plasma.desktop-appletsrc"))
                    .and_then(|a| fs::read_to_string(a).ok())
                    .is_some_and(|texto| kde_tem(&texto));
                if tem {
                    EstadoDoItem::Presente
                } else {
                    EstadoDoItem::Ausente
                }
            }
        }
    }

    fn estado_do_inicio(&self) -> EstadoDoItem {
        let (Some(lugar), Some(exe)) = (self.lugar_do_inicio(), self.exe_alvo()) else {
            return EstadoDoItem::NaoSeAplica;
        };
        match lugar.estado(&exe) {
            EstadoDoInicio::Desligado => EstadoDoItem::Ausente,
            EstadoDoInicio::Ligado => EstadoDoItem::Presente,
            EstadoDoInicio::OutroExecutavel => EstadoDoItem::Desatualizado,
        }
    }

    fn estado_dos_aplicativos(&self) -> EstadoDoItem {
        if !self.windows {
            return EstadoDoItem::NaoSeAplica;
        }
        let Some(exe) = self.exe_alvo() else {
            return EstadoDoItem::NaoSeAplica;
        };
        match sistema::ler_run_de(&self.chave_uninstall, "UninstallString") {
            Some(linha) if linha == linha_de_desinstalar(&exe) => EstadoDoItem::Presente,
            Some(_) => EstadoDoItem::Desatualizado,
            None if self.exe_instalado().is_some_and(|e| e.is_file()) => EstadoDoItem::Ausente,
            None => EstadoDoItem::Indisponivel(Motivo::PrecisaDaCopia),
        }
    }

    // ----------------------------------------------------------------- aplicar

    /// Liga (`querer`) ou desliga um item. Devolve o passo que só a pessoa pode dar, se houver.
    pub fn aplicar(&self, item: ItemDeInstalacao, querer: bool) -> io::Result<Option<PassoManual>> {
        match item {
            ItemDeInstalacao::Copia => self.aplicar_copia(querer).map(|()| None),
            ItemDeInstalacao::AreaDeTrabalho => self.aplicar_area(querer).map(|()| None),
            ItemDeInstalacao::Menu => self.aplicar_menu(querer).map(|()| None),
            ItemDeInstalacao::Barra => self.aplicar_barra(querer),
            ItemDeInstalacao::Inicio => self.aplicar_inicio(querer).map(|()| None),
            ItemDeInstalacao::Aplicativos => self.aplicar_aplicativos(querer).map(|()| None),
        }
    }

    /// Aplica os pedidos, cada um à parte (a falha de um não impede os outros). Para instalar a
    /// ordem é a de [`ItemDeInstalacao::TODOS`] (a cópia primeiro, para os outros apontarem
    /// para ela); para tirar, a contrária (a cópia por último). Só vão os itens cujo desejo
    /// difere do que já está no disco, ou que estão desatualizados.
    pub fn aplicar_pedidos(
        &self,
        pedidos: &[(ItemDeInstalacao, bool)],
    ) -> Vec<(ItemDeInstalacao, io::Result<Option<PassoManual>>)> {
        let mut resultados = Vec::new();
        let querido = |i: ItemDeInstalacao| pedidos.iter().find(|(p, _)| *p == i).map(|(_, q)| *q);
        let mut ordem: Vec<ItemDeInstalacao> = ItemDeInstalacao::TODOS.to_vec();
        for item in ordem.drain(..) {
            if let Some(true) = querido(item) {
                let estado = self.estado(item);
                if estado == EstadoDoItem::Presente && item != ItemDeInstalacao::Barra {
                    continue;
                }
                resultados.push((item, self.aplicar(item, true)));
            }
        }
        for item in ItemDeInstalacao::TODOS.into_iter().rev() {
            if let Some(false) = querido(item) {
                if !self.estado(item).instalado() {
                    continue;
                }
                resultados.push((item, self.aplicar(item, false)));
            }
        }
        resultados
    }

    fn aplicar_copia(&self, querer: bool) -> io::Result<()> {
        if self.executando_da_copia() {
            return if querer {
                Ok(())
            } else {
                Err(io::Error::other(t!("instalacao-erro-em-uso")))
            };
        }
        if !querer {
            return self.remover_copia();
        }
        let (Some(origem), Some(pasta)) = (self.exe.as_deref(), self.pasta_instalada()) else {
            return Err(io::Error::other(t!("instalacao-erro-sem-pasta")));
        };
        if self.macos {
            let Some(bundle) = self.bundle_atual() else {
                return Err(io::Error::other(t!("instalacao-erro-sem-bundle")));
            };
            return copiar_bundle(&bundle, &pasta);
        }
        copiar_executaveis(origem, &pasta, self.windows)
    }

    fn remover_copia(&self) -> io::Result<()> {
        let Some(pasta) = self.pasta_instalada() else {
            return Ok(());
        };
        if self.macos {
            return apagar_pasta(&pasta);
        }
        for nome in EXECUTAVEIS {
            let nome = if self.windows {
                format!("{nome}.exe")
            } else {
                nome.to_owned()
            };
            for sufixo in ["", ".novo", ".antigo"] {
                apagar_arquivo(&pasta.join(format!("{nome}{sufixo}")))?;
            }
        }
        if self.windows {
            // A pasta é só nossa: some se ficou vazia.
            let _ = fs::remove_dir(&pasta);
        }
        Ok(())
    }

    fn aplicar_area(&self, querer: bool) -> io::Result<()> {
        let Some(arquivo) = self.atalho_da_area() else {
            return if querer {
                Err(io::Error::other(t!("instalacao-erro-sem-area")))
            } else {
                Ok(())
            };
        };
        if !querer {
            return apagar_arquivo(&arquivo);
        }
        let Some(exe) = self.exe_alvo() else {
            return Err(io::Error::other(t!("instalacao-erro-sem-pasta")));
        };
        if self.windows {
            return gravar_lnk(&arquivo, &exe);
        }
        if self.macos {
            let Some(bundle) = self.bundle_alvo() else {
                return Err(io::Error::other(t!("instalacao-erro-sem-bundle")));
            };
            return ligar_symlink(&bundle, &arquivo);
        }
        if let Some(atalho) = self.atalho_do_menu() {
            atalho.instalar_icones()?;
        }
        if let Some(pasta) = arquivo.parent() {
            fs::create_dir_all(pasta)?;
        }
        sistema::gravar_atomico(&arquivo, entrada_do_menu_do_app(&exe).as_bytes(), false)?;
        marcar_como_executavel(&arquivo)?;
        // O Nemo e o Nautilus só abrem o `.desktop` sem perguntar quando ele é "confiável". Se o
        // `gio` não existir, a pessoa confirma uma vez ("Confiar e abrir").
        let _ = self.rodar(
            "gio",
            &[
                "set",
                &arquivo.display().to_string(),
                "metadata::trusted",
                "true",
            ],
        );
        Ok(())
    }

    fn aplicar_menu(&self, querer: bool) -> io::Result<()> {
        if self.windows {
            let Some(arquivo) = self.atalho_do_iniciar() else {
                return Ok(());
            };
            if !querer {
                return apagar_arquivo(&arquivo);
            }
            let exe = self
                .exe_alvo()
                .ok_or_else(|| io::Error::other(t!("instalacao-erro-sem-pasta")))?;
            return gravar_lnk(&arquivo, &exe);
        }
        let Some(atalho) = self.atalho_do_menu() else {
            return Ok(());
        };
        if !querer {
            return atalho.remover();
        }
        let exe = self
            .exe_alvo()
            .ok_or_else(|| io::Error::other(t!("instalacao-erro-sem-pasta")))?;
        atalho.instalar(&exe)
    }

    fn aplicar_barra(&self, querer: bool) -> io::Result<Option<PassoManual>> {
        if self.windows {
            if querer {
                // O que se fixa é o atalho do menu Iniciar.
                self.aplicar_menu(true)?;
                let de_fora = !self.executando_da_copia()
                    && self.exe_instalado().is_some_and(|e| e.is_file());
                return Ok(Some(PassoManual::FixarNaBarraDoWindows {
                    depois_de_abrir_a_copia: de_fora,
                }));
            }
            return Ok((self.estado_da_barra() == EstadoDoItem::Presente)
                .then_some(PassoManual::DesafixarDaBarraDoWindows));
        }
        if self.macos {
            return self.aplicar_dock(querer);
        }
        let Some(painel) = painel_do(&self.desktop_atual) else {
            return Err(io::Error::other(t!("instalacao-erro-painel")));
        };
        if querer {
            // O fixado é o id do `.desktop` do menu.
            self.aplicar_menu(true)?;
        }
        match painel {
            Painel::Cinnamon => self.fixar_no_cinnamon(querer)?,
            Painel::Gnome => self.fixar_no_gnome(querer)?,
            Painel::Kde => self.fixar_no_kde(querer)?,
        }
        Ok(None)
    }

    fn aplicar_dock(&self, querer: bool) -> io::Result<Option<PassoManual>> {
        if !querer {
            return Ok((self.estado_da_barra() == EstadoDoItem::Presente)
                .then_some(PassoManual::TirarDoDock));
        }
        let Some(bundle) = self.bundle_alvo() else {
            return Err(io::Error::other(t!("instalacao-erro-sem-bundle")));
        };
        if self.estado_da_barra() == EstadoDoItem::Presente {
            return Ok(None);
        }
        self.rodar(
            "defaults",
            &[
                "write",
                "com.apple.dock",
                "persistent-apps",
                "-array-add",
                &tile_do_dock(&bundle),
            ],
        )?;
        self.rodar("killall", &["Dock"])?;
        Ok(None)
    }

    /// Os `<id>.json` das instâncias do applet de janelas do Cinnamon que existem em disco.
    fn fixados_do_cinnamon(&self) -> Vec<PathBuf> {
        let Some(config) = self.pasta_de_config() else {
            return Vec::new();
        };
        let ativos = self
            .rodar("gsettings", &["get", "org.cinnamon", "enabled-applets"])
            .unwrap_or_default();
        let pasta = config
            .join("cinnamon")
            .join("spices")
            .join(APPLET_DO_CINNAMON);
        instancias_do_cinnamon(&ativos)
            .into_iter()
            .map(|id| pasta.join(format!("{id}.json")))
            .filter(|a| a.is_file())
            .collect()
    }

    fn fixar_no_cinnamon(&self, querer: bool) -> io::Result<()> {
        let arquivos = self.fixados_do_cinnamon();
        if arquivos.is_empty() {
            return Err(io::Error::other(t!("instalacao-erro-painel")));
        }
        for arquivo in arquivos {
            let texto = fs::read_to_string(&arquivo)?;
            let novo = json_do_cinnamon(&texto, &id_desktop(), querer)
                .map_err(|e| io::Error::other(t!("instalacao-erro-json", erro = e)))?;
            if let Some(novo) = novo {
                sistema::gravar_atomico(&arquivo, novo.as_bytes(), true)?;
            }
        }
        Ok(())
    }

    fn fixar_no_gnome(&self, querer: bool) -> io::Result<()> {
        let atual = self.rodar("gsettings", &["get", "org.gnome.shell", "favorite-apps"])?;
        let mut lista = ler_lista_do_gsettings(&atual);
        let id = id_desktop();
        let tem = lista.contains(&id);
        if querer == tem {
            return Ok(());
        }
        if querer {
            lista.push(id);
        } else {
            lista.retain(|i| *i != id);
        }
        self.rodar(
            "gsettings",
            &[
                "set",
                "org.gnome.shell",
                "favorite-apps",
                &escrever_lista_do_gsettings(&lista),
            ],
        )
        .map(|_| ())
    }

    fn fixar_no_kde(&self, querer: bool) -> io::Result<()> {
        self.rodar(
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                "org.kde.PlasmaShell",
                "--object-path",
                "/PlasmaShell",
                "--method",
                "org.kde.PlasmaShell.evaluateScript",
                &script_do_kde(querer),
            ],
        )
        .map(|_| ())
    }

    fn aplicar_inicio(&self, querer: bool) -> io::Result<()> {
        let Some(lugar) = self.lugar_do_inicio() else {
            return Ok(());
        };
        if !querer {
            return lugar.desligar();
        }
        let exe = self
            .exe_alvo()
            .ok_or_else(|| io::Error::other(t!("instalacao-erro-sem-pasta")))?;
        lugar.ligar(&exe)
    }

    fn aplicar_aplicativos(&self, querer: bool) -> io::Result<()> {
        if !self.windows {
            return Ok(());
        }
        if !querer {
            return apagar_chave(&self.chave_uninstall);
        }
        let (Some(exe), Some(pasta)) = (self.exe_instalado(), self.pasta_instalada()) else {
            return Err(io::Error::other(t!("instalacao-erro-sem-copia")));
        };
        if !exe.is_file() {
            return Err(io::Error::other(t!("instalacao-erro-sem-copia")));
        }
        let valores = valores_da_desinstalacao(
            &exe,
            &pasta,
            env!("CARGO_PKG_VERSION"),
            tamanho_em_kib(&pasta),
        );
        gravar_chave(&self.chave_uninstall, &valores)
    }

    // ------------------------------------------------------------ os apontamentos

    /// O executável para onde o item já feito aponta (`None` se o item não existe ou não se lê).
    /// A atualização ao abrir (fatia 5) o compara com os arquivos que existem.
    pub fn apontamento(&self, item: ItemDeInstalacao) -> Option<PathBuf> {
        match item {
            ItemDeInstalacao::AreaDeTrabalho => {
                let arquivo = self.atalho_da_area()?;
                if self.windows {
                    let atalho = lnk::ler(&fs::read(&arquivo).ok()?)?;
                    Some(PathBuf::from(atalho.alvo))
                } else if self.macos {
                    let bundle = fs::read_link(&arquivo).ok()?;
                    let nome = self.exe.as_deref()?.file_name()?.to_owned();
                    Some(bundle.join("Contents").join("MacOS").join(nome))
                } else {
                    exec_do_desktop_de(&fs::read_to_string(&arquivo).ok()?)
                }
            }
            ItemDeInstalacao::Barra if self.windows => self
                .pasta_dos_fixados()
                .and_then(|pasta| {
                    lnks_da_pasta(&pasta)
                        .into_iter()
                        .find(|(arquivo, _)| nome_do_fixado(arquivo))
                })
                .map(|(_, atalho)| PathBuf::from(atalho.alvo)),
            _ => None,
        }
    }

    /// Aponta o atalho da área de trabalho e o fixado da barra (Windows) para o executável alvo,
    /// quando eles existem **e** apontam para um executável que já não existe (o app foi
    /// movido). O que a pessoa apagou à mão não volta. Devolve o que foi repontado.
    pub fn repontar_o_que_sumiu(&self) -> Vec<ItemDeInstalacao> {
        let Some(exe) = self.exe_alvo() else {
            return Vec::new();
        };
        let mut feitos = Vec::new();
        if let Some(velho) = self.apontamento(ItemDeInstalacao::AreaDeTrabalho)
            && !velho.exists()
            && self.aplicar_area(true).is_ok()
        {
            feitos.push(ItemDeInstalacao::AreaDeTrabalho);
        }
        if self.windows
            && let Some(pasta) = self.pasta_dos_fixados()
        {
            for (arquivo, atalho) in lnks_da_pasta(&pasta) {
                if nome_do_fixado(&arquivo)
                    && !Path::new(&atalho.alvo).exists()
                    && gravar_lnk(&arquivo, &exe).is_ok()
                {
                    feitos.push(ItemDeInstalacao::Barra);
                }
            }
        }
        feitos
    }
}

// ---------------------------------------------------------------------- pastas

/// Troca os `%NOME%` de `texto` pelo valor que `busca` der; o que `busca` não conhece fica como
/// está (é como o Windows expande os `REG_EXPAND_SZ`).
pub fn expandir_variaveis(texto: &str, busca: impl Fn(&str) -> Option<String>) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut resto = texto;
    while let Some(inicio) = resto.find('%') {
        saida.push_str(&resto[..inicio]);
        let depois = &resto[inicio + 1..];
        match depois.find('%') {
            Some(fim) if fim > 0 => {
                let nome = &depois[..fim];
                match busca(nome) {
                    Some(valor) => saida.push_str(&valor),
                    None => {
                        saida.push('%');
                        saida.push_str(nome);
                        saida.push('%');
                    }
                }
                resto = &depois[fim + 1..];
            }
            // `%%` ou um `%` sem par: fica como está.
            _ => {
                saida.push('%');
                resto = depois;
            }
        }
    }
    saida.push_str(resto);
    saida
}

/// A pasta da área de trabalho que o `user-dirs.dirs` do Linux diz (`XDG_DESKTOP_DIR`), com o
/// `$HOME` trocado pela pasta pessoal. Sem a linha, ou com um caminho que não é absoluto,
/// `~/Desktop`.
pub fn pasta_da_area_de_trabalho(texto: &str, pessoal: &Path) -> PathBuf {
    let padrao = pessoal.join("Desktop");
    let valor = texto
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| l.strip_prefix("XDG_DESKTOP_DIR="));
    let Some(valor) = valor else {
        return padrao;
    };
    let valor = valor.trim().trim_matches('"');
    let home = pessoal.display().to_string();
    let caminho = valor.replace("${HOME}", &home).replace("$HOME", &home);
    let caminho = PathBuf::from(caminho);
    if caminho.is_absolute() {
        caminho
    } else {
        padrao
    }
}

/// A pasta da área de trabalho do sistema (a do registro no Windows, a do `user-dirs.dirs` no
/// Linux, `~/Desktop` no macOS). `None` sem pasta pessoal.
pub fn area_de_trabalho_do_sistema(
    windows: bool,
    macos: bool,
    pessoal: Option<&Path>,
    xdg_config: Option<&Path>,
) -> Option<PathBuf> {
    if windows {
        return pasta_do_shell("Desktop", pessoal, &["Desktop"]);
    }
    let pessoal = pessoal?;
    if macos {
        return Some(pessoal.join("Desktop"));
    }
    let config = xdg_config
        .filter(|p| p.is_absolute())
        .map_or_else(|| pessoal.join(".config"), Path::to_owned);
    let texto = fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default();
    Some(pasta_da_area_de_trabalho(&texto, pessoal))
}

/// A pasta de atalhos do menu Iniciar (`Programs`), só no Windows.
pub fn menu_iniciar_do_sistema(windows: bool, pessoal: Option<&Path>) -> Option<PathBuf> {
    if !windows {
        return None;
    }
    pasta_do_shell(
        "Programs",
        pessoal,
        &[
            "AppData",
            "Roaming",
            "Microsoft",
            "Windows",
            "Start Menu",
            "Programs",
        ],
    )
}

/// Uma pasta do shell do Windows: o valor do registro em `User Shell Folders` (com os `%VAR%`
/// expandidos), senão o de `Shell Folders` (já expandido), senão `pessoal` + `padrao`.
fn pasta_do_shell(nome: &str, pessoal: Option<&Path>, padrao: &[&str]) -> Option<PathBuf> {
    let expandir = |texto: &str| expandir_variaveis(texto, |v| std::env::var(v).ok());
    if let Some(valor) = sistema::ler_run_de(CHAVE_PASTAS_DO_USUARIO, nome)
        .filter(|v| !v.is_empty())
        .map(|v| expandir(&v))
    {
        return Some(PathBuf::from(valor));
    }
    if let Some(valor) = sistema::ler_run_de(CHAVE_PASTAS_DO_SHELL, nome).filter(|v| !v.is_empty())
    {
        return Some(PathBuf::from(valor));
    }
    let mut pasta = pessoal?.to_owned();
    pasta.extend(padrao);
    Some(pasta)
}

/// A pasta da cópia instalada.
pub fn pasta_instalada(a: &Ambiente) -> Option<PathBuf> {
    Contexto::de(a).pasta_instalada()
}

/// O executável que os atalhos e o início com o sistema abrem: o da cópia instalada, se o
/// arquivo existe; senão, o que está rodando.
pub fn exe_alvo(a: &Ambiente) -> Option<PathBuf> {
    Contexto::de(a).exe_alvo()
}

/// O `.app` que contém o executável (`.../X.app/Contents/MacOS/x`).
fn bundle_de(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    let tem_forma = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension().is_some_and(|e| e == "app");
    tem_forma.then(|| app.to_owned())
}

// ---------------------------------------------------------------------- arquivos

fn id_desktop() -> String {
    format!("{ID_DO_APP}.desktop")
}

fn entrada_do_menu_do_app(exe: &Path) -> String {
    sistema::entrada_do_menu(exe)
}

/// `nome` + `sufixo`, ao lado de `arquivo`.
fn com_sufixo(arquivo: &Path, sufixo: &str) -> PathBuf {
    let mut nome = arquivo.file_name().unwrap_or_default().to_owned();
    nome.push(sufixo);
    arquivo.with_file_name(nome)
}

/// Se os dois caminhos são o mesmo arquivo.
fn mesmo_arquivo(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// Se `texto` (o destino de um `.lnk`) é o caminho `caminho`, sem olhar `/`, `\` nem maiúsculas.
fn mesmo_caminho_de_texto(texto: &str, caminho: &Path) -> bool {
    let normal = |t: &str| t.replace('/', "\\").to_lowercase();
    normal(texto) == normal(&caminho.display().to_string())
}

/// Se os dois arquivos têm o mesmo conteúdo: o tamanho e a data de modificação iguais bastam
/// (a cópia do app copia a data); com a data diferente, vale o SHA-256.
fn mesmo_conteudo(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    if let (Ok(da), Ok(db)) = (ma.modified(), mb.modified())
        && da == db
    {
        return true;
    }
    match (sha256_do_arquivo(a), sha256_do_arquivo(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn sha256_do_arquivo(arquivo: &Path) -> io::Result<[u8; 32]> {
    use io::Read;
    let mut f = fs::File::open(arquivo)?;
    let mut hash = Sha256::new();
    let mut bloco = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut bloco)?;
        if n == 0 {
            break;
        }
        hash.update(&bloco[..n]);
    }
    Ok(hash.finalize().into())
}

/// Presente se o arquivo existe com `esperado`, desatualizado se existe com outro texto.
fn estado_do_texto(arquivo: &Path, esperado: &str) -> EstadoDoItem {
    match fs::read_to_string(arquivo) {
        Ok(texto) if texto == esperado => EstadoDoItem::Presente,
        Ok(_) => EstadoDoItem::Desatualizado,
        Err(erro) if erro.kind() == io::ErrorKind::NotFound => EstadoDoItem::Ausente,
        Err(_) => EstadoDoItem::Desatualizado,
    }
}

/// Presente se o `.lnk` existe e aponta para `exe`, desatualizado se aponta para outro.
fn estado_do_lnk(arquivo: &Path, exe: &Path) -> EstadoDoItem {
    match fs::read(arquivo) {
        Ok(bytes) => match lnk::ler(&bytes) {
            Some(atalho) if mesmo_caminho_de_texto(&atalho.alvo, exe) => EstadoDoItem::Presente,
            _ => EstadoDoItem::Desatualizado,
        },
        Err(_) => EstadoDoItem::Ausente,
    }
}

/// O `.lnk` do app para `exe`.
fn atalho_do_app(exe: &Path) -> Atalho {
    let pasta = exe
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    Atalho {
        alvo: exe.display().to_string(),
        argumentos: String::new(),
        pasta,
        icone: exe.display().to_string(),
        nome: NOME_DO_APP.to_owned(),
    }
}

fn gravar_lnk(destino: &Path, exe: &Path) -> io::Result<()> {
    if let Some(pasta) = destino.parent() {
        fs::create_dir_all(pasta)?;
    }
    sistema::gravar_atomico(destino, &lnk::gerar(&atalho_do_app(exe)), false)
}

/// Os `.lnk` de uma pasta e o que cada um guarda (os que não se leem ficam de fora).
fn lnks_da_pasta(pasta: &Path) -> Vec<(PathBuf, Atalho)> {
    let Ok(entradas) = fs::read_dir(pasta) else {
        return Vec::new();
    };
    entradas
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")))
        .filter_map(|p| {
            let atalho = lnk::ler(&fs::read(&p).ok()?)?;
            Some((p, atalho))
        })
        .collect()
}

/// Se o fixado da barra é o do Tirech Omni (pelo nome do arquivo que o Windows gravou).
fn nome_do_fixado(arquivo: &Path) -> bool {
    arquivo
        .file_stem()
        .is_some_and(|n| n.to_string_lossy().to_lowercase().contains("tirech omni"))
}

/// O caminho do `Exec` de um `.desktop` que o app escreveu (entre aspas, ver `exec_citado`).
fn exec_do_desktop_de(texto: &str) -> Option<PathBuf> {
    let linha = texto.lines().find_map(|l| l.strip_prefix("Exec="))?;
    let resto = linha.strip_prefix('"')?;
    let mut caminho = String::new();
    let mut chars = resto.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(PathBuf::from(caminho)),
            '\\' => {
                if let Some(proximo) = chars.next() {
                    caminho.push(proximo);
                }
            }
            '%' => {
                // `%%` é um `%`.
                if let Some(proximo) = chars.next() {
                    caminho.push(proximo);
                }
            }
            outro => caminho.push(outro),
        }
    }
    None
}

fn apagar_arquivo(arquivo: &Path) -> io::Result<()> {
    match fs::remove_file(arquivo) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn apagar_pasta(pasta: &Path) -> io::Result<()> {
    match fs::remove_dir_all(pasta) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(unix)]
fn marcar_como_executavel(arquivo: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(arquivo, fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn marcar_como_executavel(_arquivo: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn ligar_symlink(alvo: &Path, link: &Path) -> io::Result<()> {
    apagar_arquivo(link)?;
    if let Some(pasta) = link.parent() {
        fs::create_dir_all(pasta)?;
    }
    std::os::unix::fs::symlink(alvo, link)
}

#[cfg(not(unix))]
fn ligar_symlink(_alvo: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        t!("instalacao-erro-sem-bundle"),
    ))
}

/// Copia o executável de `origem` e os irmãos (`tirech-omni-keygen`…) que estão ao lado dele
/// para `pasta`. Cada arquivo vai para um `.novo` e é trocado pelo nome final; no Windows, o que
/// está em uso não se sobrescreve, mas se renomeia: o antigo vira `.antigo`, que a próxima
/// instalação apaga.
fn copiar_executaveis(origem: &Path, pasta: &Path, windows: bool) -> io::Result<()> {
    let Some(origem_pasta) = origem.parent() else {
        return Err(io::Error::other(t!("instalacao-erro-sem-pasta")));
    };
    fs::create_dir_all(pasta)?;
    if windows {
        for nome in EXECUTAVEIS {
            let _ = fs::remove_file(pasta.join(format!("{nome}.exe.antigo")));
        }
    }
    let principal = origem.file_name().unwrap_or_default().to_owned();
    for nome in EXECUTAVEIS {
        let arquivo = if windows {
            format!("{nome}.exe")
        } else {
            nome.to_owned()
        };
        // O executável de agora vai com o nome padrão, ainda que o arquivo se chame de outro
        // jeito (um `tirech-omni-1.2.exe` baixado).
        let fonte = if nome == ID_DO_APP {
            origem.to_owned()
        } else {
            origem_pasta.join(&arquivo)
        };
        if !fonte.is_file() {
            if nome == ID_DO_APP {
                return Err(io::Error::other(t!("instalacao-erro-sem-pasta")));
            }
            continue;
        }
        let _ = &principal;
        copiar_um(&fonte, &pasta.join(&arquivo))?;
    }
    Ok(())
}

fn copiar_um(origem: &Path, destino: &Path) -> io::Result<()> {
    let novo = com_sufixo(destino, ".novo");
    let _ = fs::remove_file(&novo);
    fs::copy(origem, &novo)?;
    if let Ok(data) = fs::metadata(origem).and_then(|m| m.modified()) {
        let _ = fs::OpenOptions::new()
            .write(true)
            .open(&novo)
            .and_then(|f| f.set_modified(data));
    }
    marcar_como_executavel(&novo)?;
    let antigo = com_sufixo(destino, ".antigo");
    let trocado = if cfg!(windows) && destino.exists() {
        let _ = fs::remove_file(&antigo);
        fs::rename(destino, &antigo).is_ok()
    } else {
        false
    };
    match fs::rename(&novo, destino) {
        Ok(()) => Ok(()),
        Err(erro) => {
            if trocado {
                let _ = fs::rename(&antigo, destino);
            }
            let _ = fs::remove_file(&novo);
            Err(erro)
        }
    }
}

/// Copia o bundle inteiro (macOS) para `destino`, trocando o antigo.
fn copiar_bundle(origem: &Path, destino: &Path) -> io::Result<()> {
    if let Some(pasta) = destino.parent() {
        fs::create_dir_all(pasta)?;
    }
    let novo = com_sufixo(destino, ".novo");
    let antigo = com_sufixo(destino, ".antigo");
    apagar_pasta(&novo)?;
    apagar_pasta(&antigo)?;
    copiar_pasta(origem, &novo)?;
    if destino.exists() {
        fs::rename(destino, &antigo)?;
    }
    if let Err(erro) = fs::rename(&novo, destino) {
        let _ = fs::rename(&antigo, destino);
        return Err(erro);
    }
    apagar_pasta(&antigo)
}

fn copiar_pasta(origem: &Path, destino: &Path) -> io::Result<()> {
    fs::create_dir_all(destino)?;
    for entrada in fs::read_dir(origem)? {
        let entrada = entrada?;
        let tipo = entrada.file_type()?;
        let alvo = destino.join(entrada.file_name());
        if tipo.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entrada.path())?, &alvo)?;
        } else if tipo.is_dir() {
            copiar_pasta(&entrada.path(), &alvo)?;
        } else {
            fs::copy(entrada.path(), &alvo)?;
        }
    }
    Ok(())
}

/// O tamanho dos arquivos da pasta, em KiB (para o `EstimatedSize`).
fn tamanho_em_kib(pasta: &Path) -> u32 {
    fn soma(pasta: &Path) -> u64 {
        fs::read_dir(pasta)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|e| match e.metadata() {
                Ok(m) if m.is_dir() => soma(&e.path()),
                Ok(m) => m.len(),
                Err(_) => 0,
            })
            .sum()
    }
    u32::try_from(soma(pasta) / 1024).unwrap_or(u32::MAX)
}

// ------------------------------------------------------------- "Aplicativos instalados"

/// Um valor de uma chave do registro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Valor {
    /// `REG_SZ`.
    Texto(String),
    /// `REG_DWORD`.
    Numero(u32),
}

/// A linha do `UninstallString`: o executável entre aspas e o argumento.
pub fn linha_de_desinstalar(exe: &Path) -> String {
    format!("\"{}\" {ARGUMENTO_DESINSTALAR}", exe.display())
}

/// Os valores da chave `Uninstall\TirechOmni`, para o Windows listar o app em Configurações ›
/// Aplicativos. `exe` é o da cópia, `pasta` a pasta dela e `tamanho_kib` o espaço que ocupa.
pub fn valores_da_desinstalacao(
    exe: &Path,
    pasta: &Path,
    versao: &str,
    tamanho_kib: u32,
) -> Vec<(&'static str, Valor)> {
    vec![
        ("DisplayName", Valor::Texto(NOME_DO_APP.to_owned())),
        (
            "DisplayIcon",
            Valor::Texto(format!("\"{}\",0", exe.display())),
        ),
        ("DisplayVersion", Valor::Texto(versao.to_owned())),
        ("Publisher", Valor::Texto("Tirech".to_owned())),
        ("InstallLocation", Valor::Texto(pasta.display().to_string())),
        ("UninstallString", Valor::Texto(linha_de_desinstalar(exe))),
        ("NoModify", Valor::Numero(1)),
        ("NoRepair", Valor::Numero(1)),
        ("EstimatedSize", Valor::Numero(tamanho_kib)),
    ]
}

#[cfg(windows)]
fn gravar_chave(chave: &str, valores: &[(&'static str, Valor)]) -> io::Result<()> {
    let chave = windows_registry::CURRENT_USER
        .create(chave)
        .map_err(io::Error::other)?;
    for (nome, valor) in valores {
        match valor {
            Valor::Texto(texto) => chave.set_string(nome, texto),
            Valor::Numero(numero) => chave.set_u32(nome, *numero),
        }
        .map_err(io::Error::other)?;
    }
    Ok(())
}

#[cfg(windows)]
fn apagar_chave(chave: &str) -> io::Result<()> {
    if windows_registry::CURRENT_USER.open(chave).is_err() {
        return Ok(());
    }
    windows_registry::CURRENT_USER
        .remove_tree(chave)
        .map_err(io::Error::other)
}

#[cfg(not(windows))]
fn gravar_chave(_chave: &str, _valores: &[(&'static str, Valor)]) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        t!("sistema-registro-so-windows"),
    ))
}

#[cfg(not(windows))]
fn apagar_chave(_chave: &str) -> io::Result<()> {
    Ok(())
}

// ------------------------------------------------------------------- o painel

/// O painel que o app sabe mexer no Linux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Painel {
    Cinnamon,
    Gnome,
    Kde,
}

/// O painel do `XDG_CURRENT_DESKTOP` (lista separada por `:`, sem diferenciar maiúsculas).
pub fn painel_do(desktop_atual: &str) -> Option<Painel> {
    let partes: Vec<String> = desktop_atual
        .split(':')
        .map(|p| p.trim().to_lowercase())
        .collect();
    let tem = |nome: &str| partes.iter().any(|p| p == nome);
    if tem("x-cinnamon") || tem("cinnamon") {
        Some(Painel::Cinnamon)
    } else if tem("kde") {
        Some(Painel::Kde)
    } else if tem("gnome") {
        Some(Painel::Gnome)
    } else {
        None
    }
}

/// Lê uma lista de textos do `gsettings` (`['a.desktop', 'b.desktop']`, `@as []`, `[]`).
pub fn ler_lista_do_gsettings(texto: &str) -> Vec<String> {
    let texto = texto.trim();
    let texto = texto.strip_prefix("@as").unwrap_or(texto).trim();
    let mut itens = Vec::new();
    let mut chars = texto.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\'' && c != '"' {
            continue;
        }
        let mut item = String::new();
        while let Some(d) = chars.next() {
            if d == '\\' {
                if let Some(e) = chars.next() {
                    item.push(e);
                }
            } else if d == c {
                break;
            } else {
                item.push(d);
            }
        }
        itens.push(item);
    }
    itens
}

/// Escreve uma lista de textos como o `gsettings set` a lê: `['a', 'b']`.
pub fn escrever_lista_do_gsettings(itens: &[String]) -> String {
    let partes: Vec<String> = itens
        .iter()
        .map(|i| format!("'{}'", i.replace('\\', "\\\\").replace('\'', "\\'")))
        .collect();
    format!("[{}]", partes.join(", "))
}

/// Os ids das instâncias do applet de janelas do Cinnamon no `enabled-applets`, cujos itens são
/// `painel:lado:ordem:uuid:id`.
pub fn instancias_do_cinnamon(enabled_applets: &str) -> Vec<String> {
    ler_lista_do_gsettings(enabled_applets)
        .iter()
        .filter_map(|item| {
            let partes: Vec<&str> = item.split(':').collect();
            (partes.len() >= 5 && partes[3] == APPLET_DO_CINNAMON).then(|| partes[4].to_owned())
        })
        .collect()
}

/// Se o JSON de uma instância do Cinnamon tem o app em `pinned-apps`.
pub fn json_do_cinnamon_tem(texto: &str) -> bool {
    let Ok(valor) = serde_json::from_str::<serde_json::Value>(texto) else {
        return false;
    };
    valor["pinned-apps"]["value"]
        .as_array()
        .is_some_and(|l| l.iter().any(|i| i.as_str() == Some(&id_desktop())))
}

/// O JSON de uma instância do Cinnamon com `id` posto (`querer`) ou tirado de `pinned-apps`,
/// mexendo só nessa chave. `Ok(None)` quando já está como se quer; o erro é o texto da falha.
pub fn json_do_cinnamon(texto: &str, id: &str, querer: bool) -> Result<Option<String>, String> {
    let mut valor: serde_json::Value = serde_json::from_str(texto).map_err(|e| e.to_string())?;
    let Some(lista) = valor
        .get_mut("pinned-apps")
        .and_then(|p| p.get_mut("value"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Err("pinned-apps".to_owned());
    };
    let posicao = lista.iter().position(|i| i.as_str() == Some(id));
    match (querer, posicao) {
        (true, Some(_)) | (false, None) => return Ok(None),
        (true, None) => lista.push(serde_json::Value::String(id.to_owned())),
        (false, Some(p)) => {
            lista.remove(p);
        }
    }
    let mut saida = Vec::new();
    let formato = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut serializador = serde_json::Serializer::with_formatter(&mut saida, formato);
    valor
        .serialize(&mut serializador)
        .map_err(|e| e.to_string())?;
    String::from_utf8(saida)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// O script do Plasma que põe (`querer`) ou tira o app dos lançadores de cada gerenciador de
/// tarefas dos painéis.
pub fn script_do_kde(querer: bool) -> String {
    format!(
        "var id = \"applications:{}\";\n\
         var querer = {querer};\n\
         var paineis = panels();\n\
         for (var i = 0; i < paineis.length; i++) {{\n\
         \x20 var widgets = paineis[i].widgets();\n\
         \x20 for (var j = 0; j < widgets.length; j++) {{\n\
         \x20   var w = widgets[j];\n\
         \x20   if (w.type.indexOf(\"taskmanager\") === -1) continue;\n\
         \x20   w.currentConfigGroup = [\"General\"];\n\
         \x20   var lista = String(w.readConfig(\"launchers\", \"\")).split(\",\")\
         .filter(function (x) {{ return x !== \"\"; }});\n\
         \x20   var pos = lista.indexOf(id);\n\
         \x20   if (querer && pos === -1) lista.push(id);\n\
         \x20   if (!querer && pos !== -1) lista.splice(pos, 1);\n\
         \x20   w.writeConfig(\"launchers\", lista.join(\",\"));\n\
         \x20   w.reloadConfig();\n\
         \x20 }}\n\
         }}\n",
        id_desktop()
    )
}

/// Se o `plasma-org.kde.plasma.desktop-appletsrc` tem o app nos `launchers` de algum widget.
pub fn kde_tem(texto: &str) -> bool {
    let id = id_desktop();
    texto
        .lines()
        .filter(|l| l.trim_start().starts_with("launchers="))
        .any(|l| l.split(',').any(|i| i.trim_end().ends_with(&id)))
}

// ------------------------------------------------------------------------ o Dock

/// A URL `file://` de um caminho, com os bytes fora do conjunto seguro em `%XX`.
pub fn url_de_arquivo(caminho: &Path) -> String {
    let mut url = String::from("file://");
    for b in caminho.display().to_string().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                url.push(b as char);
            }
            outro => url.push_str(&format!("%{outro:02X}")),
        }
    }
    if !url.ends_with('/') {
        url.push('/');
    }
    url
}

/// O item do `persistent-apps` do Dock para o `.app` em `bundle`.
pub fn tile_do_dock(bundle: &Path) -> String {
    format!(
        "<dict><key>tile-data</key><dict><key>file-data</key><dict>\
         <key>_CFURLString</key><string>{}</string>\
         <key>_CFURLStringType</key><integer>15</integer></dict></dict></dict>",
        url_de_arquivo(bundle)
    )
}

/// Se a saída do `defaults read com.apple.dock persistent-apps` tem o app.
pub fn dock_tem(saida: &str) -> bool {
    saida.contains(&format!("{NOME_DO_APP}.app")) || saida.contains("Tirech%20Omni.app")
}

/// Se o app foi aberto com `--desinstalar` (o menu de "Aplicativos instalados" do Windows).
pub fn pedido_de_desinstalar() -> bool {
    std::env::args().skip(1).any(|a| a == ARGUMENTO_DESINSTALAR)
}

/// Se a pasta de dados pode ser apagada inteira: não é a raiz, nem a pasta pessoal, nem uma
/// pasta que contém a pessoal. A pasta de dados é escolha da pessoa (pode estar num pendrive ou
/// numa pasta sincronizada), e apagar a errada não tem volta.
pub fn pasta_apagavel(pasta: &Path, pessoal: Option<&Path>) -> bool {
    if !pasta.is_absolute() || pasta.components().count() < 3 {
        return false;
    }
    match pessoal.filter(|p| !p.as_os_str().is_empty()) {
        Some(pessoal) => !pessoal.starts_with(pasta),
        None => true,
    }
}

/// A linha de comando do `cmd.exe /c` que espera o app sair e apaga as pastas. O `ping` do começo
/// dá tempo ao app de gravar o estado ao fechar. Depois, cada pasta é apagada em até 60 tentativas,
/// uma por segundo: no Windows o `rmdir` falha enquanto o executável em uso não sair, e a pasta
/// seguinte (a dos dados) só é tocada depois que a anterior (a da cópia) sumiu.
pub fn linha_de_remocao_do_windows(pastas: &[PathBuf]) -> String {
    let mut linha = String::from("ping -n 4 127.0.0.1 >nul");
    for pasta in pastas {
        let texto = pasta.display().to_string().replace('/', "\\");
        linha.push_str(&format!(
            " & for /l %i in (1,1,60) do @if exist \"{texto}\" (rmdir /s /q \"{texto}\" 2>nul & ping -n 2 127.0.0.1 >nul)"
        ));
    }
    linha
}

/// O script do `sh -c` que espera o app sair e apaga as pastas (que vão como argumentos).
pub const SCRIPT_DE_REMOCAO: &str = "sleep 3; rm -rf -- \"$@\"";

/// Apaga as `pastas` depois que este app sair: um processo solto espera uns segundos e apaga.
/// O app ainda grava o estado ao fechar, e no Windows não dá para apagar o executável em uso.
pub fn remover_depois(windows: bool, pastas: &[PathBuf]) -> io::Result<()> {
    if pastas.is_empty() {
        return Ok(());
    }
    let mut comando;
    if windows {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            comando = std::process::Command::new("cmd.exe");
            comando.raw_arg(format!("/c {}", linha_de_remocao_do_windows(pastas)));
        }
        #[cfg(not(windows))]
        {
            return Err(io::Error::from(io::ErrorKind::Unsupported));
        }
    } else {
        comando = std::process::Command::new("sh");
        comando.arg("-c").arg(SCRIPT_DE_REMOCAO).arg("sh");
        comando.args(pastas);
    }
    // Fora das pastas que vai apagar: no Windows a pasta onde um processo está não se apaga, e
    // este herdaria a do app (o atalho abre o app dentro da pasta da cópia).
    comando
        .current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    sistema::sem_janela(&mut comando).spawn().map(|_| ())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::armazenamento::testes::PastaTemporaria;
    use std::sync::{Arc, Mutex};

    type Chamadas = Arc<Mutex<Vec<String>>>;

    /// O contexto de teste: tudo dentro da pasta temporária, e um executor que anota a linha de
    /// comando e responde com o que `respostas` disser para o programa (ou vazio).
    fn contexto(pasta: &Path, respostas: Vec<(&'static str, String)>) -> (Contexto, Chamadas) {
        let chamadas: Chamadas = Arc::default();
        let anotadas = chamadas.clone();
        let executor: ExecutorEnviavel = Arc::new(move |programa, argumentos| {
            anotadas
                .lock()
                .unwrap()
                .push(format!("{programa} {}", argumentos.join(" ")));
            let chave = format!("{programa} {}", argumentos.join(" "));
            Ok(respostas
                .iter()
                .find(|(prefixo, _)| chave.starts_with(prefixo))
                .map(|(_, r)| r.clone())
                .unwrap_or_default())
        });
        let casa = pasta.join("casa");
        let origem = pasta.join("build");
        fs::create_dir_all(&origem).unwrap();
        fs::create_dir_all(casa.join("Desktop")).unwrap();
        fs::write(origem.join("tirech-omni"), b"binario-1").unwrap();
        fs::write(origem.join("tirech-omni-keygen"), b"keygen").unwrap();
        fs::write(origem.join("tirech-omni.d"), b"nao copia").unwrap();
        let contexto = Contexto {
            windows: false,
            macos: false,
            exe: Some(origem.join("tirech-omni")),
            pessoal: Some(casa.clone()),
            local_app_data: None,
            app_data: None,
            area_de_trabalho: Some(casa.join("Desktop")),
            menu_iniciar: None,
            xdg_config: Some(casa.join(".config")),
            xdg_data: Some(casa.join(".local/share")),
            desktop_atual: "X-Cinnamon".to_owned(),
            chave_run: "Software\\Teste\\Run".to_owned(),
            chave_uninstall: "Software\\Teste\\Uninstall".to_owned(),
            estados_fixos: Vec::new(),
            executor,
        };
        (contexto, chamadas)
    }

    #[test]
    fn expande_as_variaveis_conhecidas_e_deixa_as_outras() {
        let busca = |nome: &str| match nome {
            "USERPROFILE" => Some(r"C:\Users\ana".to_owned()),
            _ => None,
        };
        assert_eq!(
            expandir_variaveis(r"%USERPROFILE%\Desktop", busca),
            r"C:\Users\ana\Desktop"
        );
        assert_eq!(
            expandir_variaveis(r"%DESCONHECIDA%\x", busca),
            r"%DESCONHECIDA%\x"
        );
        assert_eq!(expandir_variaveis("100%", busca), "100%");
        assert_eq!(expandir_variaveis("a%%b", busca), "a%%b");
        assert_eq!(expandir_variaveis("sem nada", busca), "sem nada");
        assert_eq!(
            expandir_variaveis(r"%USERPROFILE%\OneDrive\%USERPROFILE%", busca),
            r"C:\Users\ana\OneDrive\C:\Users\ana"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_area_de_trabalho_vem_do_user_dirs() {
        let pessoal = Path::new("/home/ana");
        let texto = "# comentario\nXDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n\
                     XDG_DESKTOP_DIR=\"$HOME/Área de trabalho\"\n";
        assert_eq!(
            pasta_da_area_de_trabalho(texto, pessoal),
            PathBuf::from("/home/ana/Área de trabalho")
        );
        assert_eq!(
            pasta_da_area_de_trabalho("XDG_DESKTOP_DIR=\"${HOME}/Mesa de trabalho\"", pessoal),
            PathBuf::from("/home/ana/Mesa de trabalho")
        );
        assert_eq!(
            pasta_da_area_de_trabalho("XDG_DESKTOP_DIR=\"/dados/mesa\"", pessoal),
            PathBuf::from("/dados/mesa")
        );
        // Sem a linha, comentada ou relativa: ~/Desktop.
        for texto in [
            "",
            "#XDG_DESKTOP_DIR=\"$HOME/x\"",
            "XDG_DESKTOP_DIR=\"mesa\"",
        ] {
            assert_eq!(
                pasta_da_area_de_trabalho(texto, pessoal),
                PathBuf::from("/home/ana/Desktop")
            );
        }
    }

    #[test]
    fn a_lista_do_gsettings_vai_e_volta() {
        assert_eq!(
            ler_lista_do_gsettings("['a.desktop', 'b.desktop']\n"),
            ["a.desktop", "b.desktop"]
        );
        assert!(ler_lista_do_gsettings("@as []").is_empty());
        assert!(ler_lista_do_gsettings("[]").is_empty());
        assert_eq!(ler_lista_do_gsettings("[\"x y.desktop\"]"), ["x y.desktop"]);
        let itens = vec!["a.desktop".to_owned(), "it's.desktop".to_owned()];
        let texto = escrever_lista_do_gsettings(&itens);
        assert_eq!(texto, "['a.desktop', 'it\\'s.desktop']");
        assert_eq!(ler_lista_do_gsettings(&texto), itens);
        assert_eq!(escrever_lista_do_gsettings(&[]), "[]");
    }

    #[test]
    fn acha_as_instancias_do_applet_de_janelas_do_cinnamon() {
        let texto = "['panel1:left:0:menu@cinnamon.org:0', \
                     'panel1:left:2:grouped-window-list@cinnamon.org:12', \
                     'panel2:left:0:grouped-window-list@cinnamon.org:15', \
                     'panel1:right:1:systray@cinnamon.org:3']";
        assert_eq!(instancias_do_cinnamon(texto), ["12", "15"]);
        assert!(instancias_do_cinnamon("@as []").is_empty());
    }

    #[test]
    fn mexe_so_na_chave_pinned_apps_do_json_do_cinnamon() {
        let texto = r#"{
    "pinned-apps": {
        "type": "generic",
        "default": ["nemo.desktop"],
        "value": ["firefox.desktop", "nemo.desktop"]
    },
    "outra": { "value": 3 }
}"#;
        let com = json_do_cinnamon(texto, "tirech-omni.desktop", true)
            .unwrap()
            .expect("muda");
        assert!(json_do_cinnamon_tem(&com));
        let valor: serde_json::Value = serde_json::from_str(&com).unwrap();
        assert_eq!(
            valor["pinned-apps"]["value"],
            serde_json::json!(["firefox.desktop", "nemo.desktop", "tirech-omni.desktop"])
        );
        assert_eq!(
            valor["pinned-apps"]["default"],
            serde_json::json!(["nemo.desktop"])
        );
        assert_eq!(valor["outra"]["value"], 3);
        // Já está: nada a fazer.
        assert_eq!(
            json_do_cinnamon(&com, "tirech-omni.desktop", true).unwrap(),
            None
        );
        // Tirar volta ao que era.
        let sem = json_do_cinnamon(&com, "tirech-omni.desktop", false)
            .unwrap()
            .expect("muda");
        assert!(!json_do_cinnamon_tem(&sem));
        assert_eq!(
            json_do_cinnamon(&sem, "tirech-omni.desktop", false).unwrap(),
            None
        );
        // Sem `pinned-apps` ou com JSON quebrado: erro.
        assert!(json_do_cinnamon("{}", "x.desktop", true).is_err());
        assert!(json_do_cinnamon("nao e json", "x.desktop", true).is_err());
    }

    #[test]
    fn o_script_do_kde_poe_e_tira() {
        let poe = script_do_kde(true);
        assert!(poe.contains("applications:tirech-omni.desktop"));
        assert!(poe.contains("var querer = true;"));
        assert!(poe.contains("taskmanager"));
        assert!(script_do_kde(false).contains("var querer = false;"));
        let rc = "[Containments][2][Applets][5][Configuration][General]\n\
                  launchers=applications:firefox.desktop,applications:tirech-omni.desktop\n";
        assert!(kde_tem(rc));
        assert!(!kde_tem("launchers=applications:firefox.desktop\n"));
        assert!(!kde_tem("favorites=applications:tirech-omni.desktop\n"));
    }

    #[test]
    fn a_url_do_dock_escapa_o_espaco_e_o_acento() {
        assert_eq!(
            url_de_arquivo(Path::new("/Users/ana/Applications/Tirech Omni.app")),
            "file:///Users/ana/Applications/Tirech%20Omni.app/"
        );
        assert_eq!(
            url_de_arquivo(Path::new("/Users/joão/x.app")),
            "file:///Users/jo%C3%A3o/x.app/"
        );
        let tile = tile_do_dock(Path::new("/Applications/Tirech Omni.app"));
        assert!(tile.contains("<string>file:///Applications/Tirech%20Omni.app/</string>"));
        assert!(tile.contains("<integer>15</integer>"));
        assert!(dock_tem("file:///Applications/Tirech%20Omni.app/"));
        assert!(!dock_tem("file:///Applications/Safari.app/"));
    }

    #[test]
    fn os_valores_da_chave_de_desinstalacao() {
        let exe = Path::new(r"C:\Users\ana\AppData\Local\Programs\Tirech Omni\tirech-omni.exe");
        let pasta = Path::new(r"C:\Users\ana\AppData\Local\Programs\Tirech Omni");
        let valores = valores_da_desinstalacao(exe, pasta, "0.1.0", 42_000);
        let pegar = |nome: &str| {
            valores
                .iter()
                .find(|(n, _)| *n == nome)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            pegar("DisplayName"),
            Some(Valor::Texto("Tirech Omni".into()))
        );
        assert_eq!(
            pegar("DisplayIcon"),
            Some(Valor::Texto(format!("\"{}\",0", exe.display())))
        );
        assert_eq!(pegar("DisplayVersion"), Some(Valor::Texto("0.1.0".into())));
        assert_eq!(pegar("Publisher"), Some(Valor::Texto("Tirech".into())));
        assert_eq!(
            pegar("InstallLocation"),
            Some(Valor::Texto(pasta.display().to_string()))
        );
        assert_eq!(
            pegar("UninstallString"),
            Some(Valor::Texto(format!("\"{}\" --desinstalar", exe.display())))
        );
        assert_eq!(pegar("NoModify"), Some(Valor::Numero(1)));
        assert_eq!(pegar("NoRepair"), Some(Valor::Numero(1)));
        assert_eq!(pegar("EstimatedSize"), Some(Valor::Numero(42_000)));
    }

    #[test]
    fn o_painel_vem_do_xdg_current_desktop() {
        assert_eq!(painel_do("X-Cinnamon"), Some(Painel::Cinnamon));
        assert_eq!(painel_do("ubuntu:GNOME"), Some(Painel::Gnome));
        assert_eq!(painel_do("KDE"), Some(Painel::Kde));
        assert_eq!(painel_do("XFCE"), None);
        assert_eq!(painel_do(""), None);
    }

    #[test]
    fn o_exe_alvo_e_a_copia_so_se_ela_existe() {
        let pasta = PastaTemporaria::nova("exe-alvo");
        let (ctx, _) = contexto(&pasta.0, Vec::new());
        assert_eq!(ctx.exe_alvo(), ctx.exe);
        let copia = ctx.exe_instalado().unwrap();
        assert_eq!(copia, pasta.0.join("casa/.local/bin/tirech-omni"));
        fs::create_dir_all(copia.parent().unwrap()).unwrap();
        fs::write(&copia, b"x").unwrap();
        assert_eq!(ctx.exe_alvo(), Some(copia));
    }

    #[test]
    fn copiar_instala_atualiza_e_remove_com_a_permissao_certa() {
        let pasta = PastaTemporaria::nova("copia");
        let (ctx, _) = contexto(&pasta.0, Vec::new());
        let copia = ctx.exe_instalado().unwrap();
        assert_eq!(ctx.estado(ItemDeInstalacao::Copia), EstadoDoItem::Ausente);

        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        assert_eq!(fs::read(&copia).unwrap(), b"binario-1");
        assert_eq!(
            fs::read(copia.with_file_name("tirech-omni-keygen")).unwrap(),
            b"keygen"
        );
        assert!(
            !copia.with_file_name("tirech-omni.d").exists(),
            "só os irmãos conhecidos"
        );
        assert!(!copia.with_file_name("tirech-omni.novo").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = fs::metadata(&copia).unwrap().permissions().mode();
            assert_eq!(modo & 0o777, 0o755);
        }
        assert_eq!(ctx.estado(ItemDeInstalacao::Copia), EstadoDoItem::Presente);

        // O app de origem muda: a cópia fica desatualizada, e aplicar a atualiza.
        fs::write(ctx.exe.as_ref().unwrap(), b"binario-2!").unwrap();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Copia),
            EstadoDoItem::Desatualizado
        );
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        assert_eq!(fs::read(&copia).unwrap(), b"binario-2!");
        assert_eq!(ctx.estado(ItemDeInstalacao::Copia), EstadoDoItem::Presente);

        // Mesmo tamanho, outro conteúdo e outra data: o SHA-256 desempata.
        fs::write(ctx.exe.as_ref().unwrap(), b"binario-3!").unwrap();
        let futuro = std::time::SystemTime::now() + std::time::Duration::from_secs(30);
        fs::OpenOptions::new()
            .write(true)
            .open(ctx.exe.as_ref().unwrap())
            .unwrap()
            .set_modified(futuro)
            .unwrap();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Copia),
            EstadoDoItem::Desatualizado
        );

        ctx.aplicar(ItemDeInstalacao::Copia, false).unwrap();
        assert!(!copia.exists());
        assert!(!copia.with_file_name("tirech-omni-keygen").exists());
        assert_eq!(ctx.estado(ItemDeInstalacao::Copia), EstadoDoItem::Ausente);
    }

    #[test]
    fn rodando_da_copia_ela_conta_como_presente_e_nao_se_apaga() {
        let pasta = PastaTemporaria::nova("da-copia");
        let (mut ctx, _) = contexto(&pasta.0, Vec::new());
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        ctx.exe = ctx.exe_instalado();
        assert!(ctx.executando_da_copia());
        assert_eq!(ctx.estado(ItemDeInstalacao::Copia), EstadoDoItem::Presente);
        assert!(ctx.aplicar(ItemDeInstalacao::Copia, true).is_ok());
        assert!(ctx.aplicar(ItemDeInstalacao::Copia, false).is_err());
        assert!(ctx.exe_instalado().unwrap().exists());
    }

    #[test]
    fn instalado_por_pacote_no_linux_nao_oferece_a_copia() {
        let pasta = PastaTemporaria::nova("pacote");
        let (mut ctx, _) = contexto(&pasta.0, Vec::new());
        ctx.exe = Some(PathBuf::from("/usr/bin/tirech-omni"));
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Copia),
            EstadoDoItem::NaoSeAplica
        );
        ctx.exe = Some(PathBuf::from("/opt/tirech-omni/tirech-omni"));
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Copia),
            EstadoDoItem::NaoSeAplica
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_area_de_trabalho_do_linux_grava_o_desktop_confiavel() {
        let pasta = PastaTemporaria::nova("area-linux");
        let (ctx, chamadas) = contexto(&pasta.0, Vec::new());
        let arquivo = pasta.0.join("casa/Desktop/tirech-omni.desktop");
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Ausente
        );
        ctx.aplicar(ItemDeInstalacao::AreaDeTrabalho, true).unwrap();
        let texto = fs::read_to_string(&arquivo).unwrap();
        assert_eq!(texto, sistema::entrada_do_menu(ctx.exe.as_ref().unwrap()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&arquivo).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        let chamadas = chamadas.lock().unwrap().clone();
        assert_eq!(
            chamadas,
            [format!(
                "gio set {} metadata::trusted true",
                arquivo.display()
            )]
        );
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Presente
        );
        // O ícone do tema também foi gravado, para o atalho da área ter ícone sem o menu.
        assert!(
            pasta
                .0
                .join("casa/.local/share/icons/hicolor/scalable/apps/tirech-omni.svg")
                .is_file()
        );
        // A cópia aparece depois: o atalho fica apontando para o executável antigo.
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Desatualizado
        );
        ctx.aplicar(ItemDeInstalacao::AreaDeTrabalho, true).unwrap();
        assert!(
            fs::read_to_string(&arquivo)
                .unwrap()
                .contains(".local/bin/tirech-omni")
        );
        ctx.aplicar(ItemDeInstalacao::AreaDeTrabalho, false)
            .unwrap();
        assert!(!arquivo.exists());
    }

    #[test]
    fn sem_pasta_da_area_de_trabalho_o_item_fica_indisponivel() {
        let pasta = PastaTemporaria::nova("sem-area");
        let (mut ctx, _) = contexto(&pasta.0, Vec::new());
        ctx.area_de_trabalho = Some(pasta.0.join("nao-existe"));
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Indisponivel(Motivo::SemAreaDeTrabalho)
        );
        ctx.area_de_trabalho = None;
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Indisponivel(Motivo::SemAreaDeTrabalho)
        );
    }

    #[cfg(unix)]
    #[test]
    fn o_menu_e_o_inicio_do_linux_apontam_para_a_copia() {
        let pasta = PastaTemporaria::nova("menu-linux");
        let (ctx, _) = contexto(&pasta.0, Vec::new());
        assert_eq!(ctx.estado(ItemDeInstalacao::Menu), EstadoDoItem::Ausente);
        ctx.aplicar(ItemDeInstalacao::Menu, true).unwrap();
        ctx.aplicar(ItemDeInstalacao::Inicio, true).unwrap();
        assert_eq!(ctx.estado(ItemDeInstalacao::Menu), EstadoDoItem::Presente);
        assert_eq!(ctx.estado(ItemDeInstalacao::Inicio), EstadoDoItem::Presente);
        // Instalar a cópia faz os dois apontarem para o executável antigo.
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Menu),
            EstadoDoItem::Desatualizado
        );
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Inicio),
            EstadoDoItem::Desatualizado
        );
        ctx.aplicar(ItemDeInstalacao::Menu, true).unwrap();
        ctx.aplicar(ItemDeInstalacao::Inicio, true).unwrap();
        let desktop = fs::read_to_string(
            pasta
                .0
                .join("casa/.local/share/applications/tirech-omni.desktop"),
        )
        .unwrap();
        assert!(desktop.contains(".local/bin/tirech-omni"));
        assert_eq!(ctx.estado(ItemDeInstalacao::Menu), EstadoDoItem::Presente);
        ctx.aplicar(ItemDeInstalacao::Menu, false).unwrap();
        ctx.aplicar(ItemDeInstalacao::Inicio, false).unwrap();
        assert_eq!(ctx.estado(ItemDeInstalacao::Menu), EstadoDoItem::Ausente);
        assert_eq!(ctx.estado(ItemDeInstalacao::Inicio), EstadoDoItem::Ausente);
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Aplicativos),
            EstadoDoItem::NaoSeAplica
        );
    }

    #[test]
    fn fixar_no_gnome_poe_e_tira_o_id_da_lista() {
        let pasta = PastaTemporaria::nova("gnome");
        let (mut ctx, chamadas) = contexto(
            &pasta.0,
            vec![(
                "gsettings get org.gnome.shell favorite-apps",
                "['firefox.desktop']\n".to_owned(),
            )],
        );
        ctx.desktop_atual = "ubuntu:GNOME".to_owned();
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Ausente);
        ctx.aplicar(ItemDeInstalacao::Barra, true).unwrap();
        let chamadas = chamadas.lock().unwrap().clone();
        assert!(chamadas.contains(
            &"gsettings set org.gnome.shell favorite-apps ['firefox.desktop', 'tirech-omni.desktop']"
                .to_owned()
        ));
        // Fixar leva o atalho do menu junto.
        assert!(
            pasta
                .0
                .join("casa/.local/share/applications/tirech-omni.desktop")
                .is_file()
        );
    }

    #[test]
    fn fixar_no_gnome_ja_fixado_nao_regrava() {
        let pasta = PastaTemporaria::nova("gnome-ja");
        let (mut ctx, chamadas) = contexto(
            &pasta.0,
            vec![(
                "gsettings get org.gnome.shell favorite-apps",
                "['tirech-omni.desktop', 'a.desktop']".to_owned(),
            )],
        );
        ctx.desktop_atual = "GNOME".to_owned();
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Presente);
        ctx.aplicar(ItemDeInstalacao::Barra, true).unwrap();
        assert!(
            !chamadas
                .lock()
                .unwrap()
                .iter()
                .any(|c| c.starts_with("gsettings set"))
        );
        ctx.aplicar(ItemDeInstalacao::Barra, false).unwrap();
        assert!(
            chamadas
                .lock()
                .unwrap()
                .contains(&"gsettings set org.gnome.shell favorite-apps ['a.desktop']".to_owned())
        );
    }

    #[test]
    fn fixar_no_cinnamon_edita_o_json_de_cada_instancia() {
        let pasta = PastaTemporaria::nova("cinnamon");
        let applets = "['panel1:left:2:grouped-window-list@cinnamon.org:12', \
                       'panel1:left:0:menu@cinnamon.org:0']"
            .to_owned();
        let (ctx, _) = contexto(
            &pasta.0,
            vec![("gsettings get org.cinnamon enabled-applets", applets)],
        );
        let spices = pasta
            .0
            .join("casa/.config/cinnamon/spices/grouped-window-list@cinnamon.org");
        fs::create_dir_all(&spices).unwrap();
        let json = spices.join("12.json");
        fs::write(
            &json,
            r#"{"pinned-apps": {"type": "generic", "value": ["nemo.desktop"]}}"#,
        )
        .unwrap();
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Ausente);
        ctx.aplicar(ItemDeInstalacao::Barra, true).unwrap();
        assert!(json_do_cinnamon_tem(&fs::read_to_string(&json).unwrap()));
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Presente);
        ctx.aplicar(ItemDeInstalacao::Barra, false).unwrap();
        assert!(!json_do_cinnamon_tem(&fs::read_to_string(&json).unwrap()));
        assert!(fs::read_to_string(&json).unwrap().contains("nemo.desktop"));
    }

    #[test]
    fn cinnamon_sem_o_applet_de_janelas_fica_indisponivel() {
        let pasta = PastaTemporaria::nova("cinnamon-sem");
        let (ctx, _) = contexto(&pasta.0, Vec::new());
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Barra),
            EstadoDoItem::Indisponivel(Motivo::PainelNaoSuportado)
        );
        assert!(ctx.aplicar(ItemDeInstalacao::Barra, true).is_err());
    }

    #[test]
    fn fixar_no_kde_manda_o_script_pelo_gdbus() {
        let pasta = PastaTemporaria::nova("kde");
        let (mut ctx, chamadas) = contexto(&pasta.0, Vec::new());
        ctx.desktop_atual = "KDE".to_owned();
        ctx.aplicar(ItemDeInstalacao::Barra, true).unwrap();
        let chamadas = chamadas.lock().unwrap().clone();
        let linha = chamadas.iter().find(|c| c.starts_with("gdbus")).unwrap();
        assert!(linha.starts_with(
            "gdbus call --session --dest org.kde.PlasmaShell --object-path /PlasmaShell \
             --method org.kde.PlasmaShell.evaluateScript "
        ));
        assert!(linha.contains("var querer = true;"));
        // O estado vem do arquivo de configuração do Plasma.
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Ausente);
        fs::create_dir_all(pasta.0.join("casa/.config")).unwrap();
        let rc = pasta
            .0
            .join("casa/.config/plasma-org.kde.plasma.desktop-appletsrc");
        fs::write(&rc, "launchers=applications:tirech-omni.desktop\n").unwrap();
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Presente);
    }

    #[test]
    fn outros_paineis_ficam_indisponiveis() {
        let pasta = PastaTemporaria::nova("xfce");
        let (mut ctx, _) = contexto(&pasta.0, Vec::new());
        ctx.desktop_atual = "XFCE".to_owned();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Barra),
            EstadoDoItem::Indisponivel(Motivo::PainelNaoSuportado)
        );
        assert!(ctx.aplicar(ItemDeInstalacao::Barra, true).is_err());
    }

    /// Um contexto "de Windows" sobre pastas temporárias: os `.lnk` são escritos de verdade.
    fn contexto_windows(pasta: &Path) -> (Contexto, Chamadas) {
        let (mut ctx, chamadas) = contexto(pasta, Vec::new());
        let casa = pasta.join("casa");
        ctx.windows = true;
        ctx.local_app_data = Some(casa.join("AppData/Local"));
        ctx.app_data = Some(casa.join("AppData/Roaming"));
        ctx.menu_iniciar = Some(casa.join("AppData/Roaming/Start Menu/Programs"));
        ctx.desktop_atual = String::new();
        (ctx, chamadas)
    }

    /// O registro de verdade (só no Windows): grava a chave `Uninstall`, confere e apaga.
    #[cfg(windows)]
    #[test]
    fn os_aplicativos_instalados_gravam_o_registro_confere_e_apagam() {
        let pasta = PastaTemporaria::nova("registro-uninstall");
        let (mut ctx, _) = contexto_windows(&pasta.0);
        ctx.chave_uninstall = r"Software\TirechOmniTeste\InstalacaoM65\Uninstall".to_owned();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Aplicativos),
            EstadoDoItem::Indisponivel(Motivo::PrecisaDaCopia)
        );
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Aplicativos),
            EstadoDoItem::Ausente
        );
        ctx.aplicar(ItemDeInstalacao::Aplicativos, true).unwrap();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Aplicativos),
            EstadoDoItem::Presente
        );
        let nome = sistema::ler_run_de(&ctx.chave_uninstall, "DisplayName");
        assert_eq!(nome.as_deref(), Some(NOME_DO_APP));
        let linha = sistema::ler_run_de(&ctx.chave_uninstall, "UninstallString").unwrap();
        assert!(linha.ends_with(ARGUMENTO_DESINSTALAR));
        ctx.aplicar(ItemDeInstalacao::Aplicativos, false).unwrap();
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Aplicativos),
            EstadoDoItem::Ausente
        );
        // Apagar o que já não existe não é erro.
        ctx.aplicar(ItemDeInstalacao::Aplicativos, false).unwrap();
        let _ = windows_registry::CURRENT_USER.remove_tree(r"Software\TirechOmniTeste");
    }

    #[test]
    fn os_atalhos_do_windows_sao_lnk_que_apontam_para_o_alvo() {
        let pasta = PastaTemporaria::nova("lnk-windows");
        let (ctx, _) = contexto_windows(&pasta.0);
        let da_area = pasta.0.join("casa/Desktop/Tirech Omni.lnk");
        let do_menu = pasta
            .0
            .join("casa/AppData/Roaming/Start Menu/Programs/Tirech Omni.lnk");
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Ausente
        );
        ctx.aplicar(ItemDeInstalacao::AreaDeTrabalho, true).unwrap();
        ctx.aplicar(ItemDeInstalacao::Menu, true).unwrap();
        for arquivo in [&da_area, &do_menu] {
            let atalho = lnk::ler(&fs::read(arquivo).unwrap()).unwrap();
            assert_eq!(atalho.alvo, ctx.exe.as_ref().unwrap().display().to_string());
            assert_eq!(atalho.nome, "Tirech Omni");
        }
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Presente
        );
        // A cópia nasce em `Programs\Tirech Omni`: os atalhos viram desatualizados.
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        assert!(
            pasta
                .0
                .join("casa/AppData/Local/Programs/Tirech Omni/tirech-omni.exe")
                .is_file()
        );
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Menu),
            EstadoDoItem::Desatualizado
        );
        ctx.aplicar(ItemDeInstalacao::Menu, true).unwrap();
        assert_eq!(ctx.estado(ItemDeInstalacao::Menu), EstadoDoItem::Presente);
        ctx.aplicar(ItemDeInstalacao::Menu, false).unwrap();
        assert!(!do_menu.exists());
        assert_eq!(
            ctx.apontamento(ItemDeInstalacao::AreaDeTrabalho),
            Some(ctx.exe.clone().unwrap())
        );
    }

    #[test]
    fn a_barra_do_windows_garante_o_menu_e_pede_o_passo_manual() {
        let pasta = PastaTemporaria::nova("barra-windows");
        let (ctx, _) = contexto_windows(&pasta.0);
        // Sem cópia: o passo é direto.
        assert_eq!(
            ctx.aplicar(ItemDeInstalacao::Barra, true).unwrap(),
            Some(PassoManual::FixarNaBarraDoWindows {
                depois_de_abrir_a_copia: false
            })
        );
        assert_eq!(ctx.estado(ItemDeInstalacao::Menu), EstadoDoItem::Presente);
        // Com a cópia feita e o app rodando de fora: abrir a cópia antes.
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        assert_eq!(
            ctx.aplicar(ItemDeInstalacao::Barra, true).unwrap(),
            Some(PassoManual::FixarNaBarraDoWindows {
                depois_de_abrir_a_copia: true
            })
        );
        // O estado vem dos fixados do Windows.
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Ausente);
        let fixados = ctx.pasta_dos_fixados().unwrap();
        fs::create_dir_all(&fixados).unwrap();
        gravar_lnk(&fixados.join("Tirech Omni.lnk"), &ctx.exe_alvo().unwrap()).unwrap();
        assert_eq!(ctx.estado(ItemDeInstalacao::Barra), EstadoDoItem::Presente);
        assert_eq!(
            ctx.aplicar(ItemDeInstalacao::Barra, false).unwrap(),
            Some(PassoManual::DesafixarDaBarraDoWindows)
        );
    }

    #[test]
    fn repontar_so_o_que_existe_e_aponta_para_um_executavel_que_sumiu() {
        let pasta = PastaTemporaria::nova("repontar");
        let (mut ctx, _) = contexto_windows(&pasta.0);
        // A pessoa apagou o atalho: o app não o recria.
        assert!(ctx.repontar_o_que_sumiu().is_empty());
        assert!(!pasta.0.join("casa/Desktop/Tirech Omni.lnk").exists());
        // O atalho aponta para um executável que existe: nada muda.
        ctx.aplicar(ItemDeInstalacao::AreaDeTrabalho, true).unwrap();
        assert!(ctx.repontar_o_que_sumiu().is_empty());
        // O app foi movido: o atalho velho aponta para o que sumiu e passa a apontar para o novo.
        let velho = ctx.exe.clone().unwrap();
        let novo = pasta.0.join("movido/tirech-omni");
        fs::create_dir_all(novo.parent().unwrap()).unwrap();
        fs::rename(&velho, &novo).unwrap();
        ctx.exe = Some(novo.clone());
        assert_eq!(
            ctx.repontar_o_que_sumiu(),
            [ItemDeInstalacao::AreaDeTrabalho]
        );
        assert_eq!(
            ctx.apontamento(ItemDeInstalacao::AreaDeTrabalho),
            Some(novo)
        );
        assert!(ctx.repontar_o_que_sumiu().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn no_macos_a_area_e_um_link_e_o_dock_vai_pelo_defaults() {
        let pasta = PastaTemporaria::nova("macos");
        let (mut ctx, chamadas) = contexto(&pasta.0, Vec::new());
        let bundle = pasta.0.join("build/Tirech Omni.app");
        let macos = bundle.join("Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        fs::write(macos.join("tirech-omni"), b"mac").unwrap();
        fs::write(bundle.join("Contents/Info.plist"), b"plist").unwrap();
        std::os::unix::fs::symlink("Contents", bundle.join("atalho")).unwrap();
        ctx.macos = true;
        ctx.exe = Some(macos.join("tirech-omni"));
        assert_eq!(
            ctx.estado(ItemDeInstalacao::Menu),
            EstadoDoItem::NaoSeAplica
        );
        assert_eq!(ctx.estado(ItemDeInstalacao::Copia), EstadoDoItem::Ausente);
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        let copia = pasta.0.join("casa/Applications/Tirech Omni.app");
        assert_eq!(
            fs::read(copia.join("Contents/MacOS/tirech-omni")).unwrap(),
            b"mac"
        );
        assert!(
            fs::symlink_metadata(copia.join("atalho"))
                .unwrap()
                .is_symlink()
        );
        assert_eq!(ctx.estado(ItemDeInstalacao::Copia), EstadoDoItem::Presente);

        ctx.aplicar(ItemDeInstalacao::AreaDeTrabalho, true).unwrap();
        assert_eq!(
            fs::read_link(pasta.0.join("casa/Desktop/Tirech Omni.app")).unwrap(),
            copia
        );
        assert_eq!(
            ctx.estado(ItemDeInstalacao::AreaDeTrabalho),
            EstadoDoItem::Presente
        );

        ctx.aplicar(ItemDeInstalacao::Barra, true).unwrap();
        let chamadas_feitas = chamadas.lock().unwrap().clone();
        let escrita = chamadas_feitas
            .iter()
            .find(|c| c.starts_with("defaults write com.apple.dock persistent-apps -array-add"))
            .expect("escreveu no Dock");
        assert!(escrita.contains(&format!("file://{}/", copia.display()).replace(' ', "%20")));
        assert!(chamadas_feitas.contains(&"killall Dock".to_owned()));
        assert_eq!(
            ctx.aplicar(ItemDeInstalacao::Barra, false).unwrap(),
            None,
            "o executor de teste devolve vazio: o app não está no Dock"
        );
        ctx.aplicar(ItemDeInstalacao::Copia, false).unwrap();
        assert!(!copia.exists());
    }

    #[test]
    fn aplicar_pedidos_faz_cada_item_a_parte_e_na_ordem() {
        let pasta = PastaTemporaria::nova("pedidos");
        let (mut ctx, _) = contexto(&pasta.0, Vec::new());
        // Sem a pasta da área de trabalho, o item falha e os outros seguem.
        ctx.area_de_trabalho = None;
        let resultados = ctx.aplicar_pedidos(&[
            (ItemDeInstalacao::Inicio, true),
            (ItemDeInstalacao::AreaDeTrabalho, true),
            (ItemDeInstalacao::Copia, true),
            (ItemDeInstalacao::Menu, true),
        ]);
        let itens: Vec<ItemDeInstalacao> = resultados.iter().map(|(i, _)| *i).collect();
        assert_eq!(
            itens,
            [
                ItemDeInstalacao::Copia,
                ItemDeInstalacao::AreaDeTrabalho,
                ItemDeInstalacao::Menu,
                ItemDeInstalacao::Inicio
            ]
        );
        assert!(resultados[0].1.is_ok());
        assert!(resultados[1].1.is_err());
        assert!(resultados[2].1.is_ok());
        assert!(resultados[3].1.is_ok());
        // Tudo apontou para a cópia, que foi feita primeiro.
        assert_eq!(ctx.estado(ItemDeInstalacao::Menu), EstadoDoItem::Presente);
        assert_eq!(ctx.estado(ItemDeInstalacao::Inicio), EstadoDoItem::Presente);
        assert!(ctx.algo_instalado());

        // Tirar: a cópia é a última.
        let resultados = ctx.aplicar_pedidos(&[
            (ItemDeInstalacao::Copia, false),
            (ItemDeInstalacao::Menu, false),
            (ItemDeInstalacao::Inicio, false),
        ]);
        let itens: Vec<ItemDeInstalacao> = resultados.iter().map(|(i, _)| *i).collect();
        assert_eq!(
            itens,
            [
                ItemDeInstalacao::Inicio,
                ItemDeInstalacao::Menu,
                ItemDeInstalacao::Copia
            ]
        );
        assert!(!ctx.algo_instalado());
    }

    #[test]
    fn o_exec_do_desktop_volta_ao_caminho() {
        let exe = Path::new("/home/ana/meu app/tirech-omni");
        let texto = sistema::entrada_do_menu(exe);
        assert_eq!(exec_do_desktop_de(&texto), Some(exe.to_owned()));
        assert_eq!(exec_do_desktop_de("[Desktop Entry]\nName=x\n"), None);
    }

    #[test]
    fn mostra_o_caminho_com_barra_invertida_no_windows_e_til_no_resto() {
        let pasta = PastaTemporaria::nova("mostrar");
        let (mut ctx, _) = contexto(&pasta.0, Vec::new());
        let casa = ctx.pessoal.clone().unwrap();
        assert_eq!(
            ctx.mostrar(&casa.join("Desktop/tirech-omni.desktop")),
            "~/Desktop/tirech-omni.desktop"
        );
        ctx.windows = true;
        assert_eq!(
            ctx.mostrar(Path::new("C:/Users/ana/Desktop/Tirech Omni.lnk")),
            r"C:\Users\ana\Desktop\Tirech Omni.lnk"
        );
    }

    #[cfg(unix)]
    #[test]
    fn so_apaga_a_pasta_de_dados_que_nao_leva_a_pessoal_junto() {
        let casa = Path::new("/home/ana");
        assert!(pasta_apagavel(
            Path::new("/home/ana/.config/tirech-omni"),
            Some(casa)
        ));
        assert!(pasta_apagavel(Path::new("/media/pen/tirech"), Some(casa)));
        assert!(!pasta_apagavel(Path::new("/"), Some(casa)));
        assert!(!pasta_apagavel(Path::new("/home"), Some(casa)));
        assert!(!pasta_apagavel(casa, Some(casa)));
        assert!(!pasta_apagavel(Path::new("relativa/x/y"), Some(casa)));
        assert!(!pasta_apagavel(Path::new("tirech"), None));
    }

    #[test]
    fn a_linha_do_cmd_espera_e_apaga_cada_pasta() {
        let linha = linha_de_remocao_do_windows(&[
            PathBuf::from("C:/Users/ana/AppData/Local/Programs/Tirech Omni"),
            PathBuf::from(r"C:\Users\ana\AppData\Roaming\tirech-omni"),
        ]);
        let tentativas = |pasta: &str| {
            format!(
                r#" & for /l %i in (1,1,60) do @if exist "{pasta}" (rmdir /s /q "{pasta}" 2>nul & ping -n 2 127.0.0.1 >nul)"#
            )
        };
        assert_eq!(
            linha,
            format!(
                "ping -n 4 127.0.0.1 >nul{}{}",
                tentativas(r"C:\Users\ana\AppData\Local\Programs\Tirech Omni"),
                tentativas(r"C:\Users\ana\AppData\Roaming\tirech-omni")
            )
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_remocao_adiada_apaga_a_pasta_depois_de_alguns_segundos() {
        let pasta = PastaTemporaria::nova("adiada");
        let alvo = pasta.0.join("dados com espaço");
        fs::create_dir_all(alvo.join("dentro")).unwrap();
        fs::write(alvo.join("dentro/x"), b"x").unwrap();
        remover_depois(false, std::slice::from_ref(&alvo)).unwrap();
        // Logo depois ainda está lá: o processo espera o app sair.
        assert!(alvo.exists());
        let limite = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while alvo.exists() && std::time::Instant::now() < limite {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(!alvo.exists());
        remover_depois(false, &[]).unwrap();
    }

    #[test]
    fn desinstalar_a_copia_em_uso_fora_do_windows_apaga_o_arquivo_aberto() {
        let pasta = PastaTemporaria::nova("em-uso");
        let (mut ctx, _) = contexto(&pasta.0, Vec::new());
        ctx.aplicar(ItemDeInstalacao::Copia, true).unwrap();
        let copia = ctx.exe_instalado().unwrap();
        assert!(copia.is_file());
        ctx.exe = Some(copia.clone());
        assert!(ctx.executando_da_copia());
        // Em uso, o aplicar comum se recusa.
        assert!(ctx.aplicar(ItemDeInstalacao::Copia, false).is_err());
        assert!(copia.is_file());
        assert_eq!(ctx.desinstalar_copia().unwrap(), None);
        assert!(!copia.exists());
    }
}
