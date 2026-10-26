# Install on this computer (M65): errors from the install operations.
instalacao-erro-em-uso = The app is running from the installed copy; it can only be removed by the uninstall.
instalacao-erro-sem-copia = This item needs the installed copy. Also check "Copy to this computer".
instalacao-erro-sem-pasta = Could not find the install folder or the app executable.
instalacao-erro-sem-area = This environment has no desktop folder.
instalacao-erro-painel = This environment does not let the app pin itself to the panel.
instalacao-erro-sem-bundle = The app is not running from inside an .app bundle.
instalacao-erro-json = Could not edit the panel configuration: { $erro }

# The "Install on this computer" dialog (M65).
instalar-titulo = Install on this computer
instalar-titulo-desinstalar = Uninstall from this computer
instalar-introducao = Choose what { -app } leaves on this computer. Each item is applied on its own: if one fails, the others go on.
instalar-introducao-desinstalar = Anything left unchecked is removed from this computer.
instalar-aplicar = Apply
instalar-aplicando = Applying…
instalar-desinstalar = Uninstall…
instalar-apagar-dados = Also delete my data (connections, vault, notes, logs)
instalar-apagar-dados-pasta = Data folder: { $pasta }
instalar-confirmar-titulo = Delete your data?
instalar-confirmar-texto = The connections, vault, notes, and logs in { $pasta } will be deleted for good. The app closes afterwards.
instalar-confirmar-apagar = Delete everything and uninstall

# The items.
instalar-item-copia = Copy to this computer
instalar-item-area = Desktop shortcut
instalar-item-menu-windows = Start menu shortcut
instalar-item-menu-linux = Applications menu shortcut
instalar-item-barra-windows = Pin to the taskbar
instalar-item-barra-linux = Pin to the panel
instalar-item-barra-macos = Keep in the Dock
instalar-inicio-na-bandeja = Stay in the tray only, without opening the window
instalar-item-inicio = Open with the system
instalar-item-aplicativos = Show in Installed apps

# What each item shows under its name, when it is not a path.
instalar-desc-registro = Windows registry: { $chave }
instalar-desc-barra-windows = Windows only lets the person pin it; the app shows the step.
instalar-desc-barra-painel = { $painel } panel
instalar-desc-barra-dock = The macOS Dock
instalar-desc-copia-em-uso = This is the copy that is open right now.

# Why an item is not available right now.
instalar-motivo-sem-area = This environment has no desktop folder.
instalar-motivo-precisa-da-copia = Needs the copy on this computer.
instalar-motivo-painel = This environment does not let the app pin itself; drag { -app } from the menu to the panel.
instalar-motivo-sem-bundle = Only works with the app open from inside the .app bundle.
instalar-motivo-barra-precisa-do-menu = Pinning needs the menu shortcut.

# The result.
instalar-resultado-titulo-ok = Installation applied
instalar-resultado-titulo-aviso = Some items were not applied
instalar-resultado-ok = What you chose was applied.
instalar-resultado-nada = Nothing to do: the computer is already as you asked.
instalar-resultado-falha = { $item }: { $erro }
instalar-passo-fixar-windows = To pin to the taskbar: open the Start menu, find { -app }, right-click it, and choose Pin to taskbar.
instalar-passo-fixar-windows-depois = To pin to the taskbar: open the installed version, right-click its icon on the taskbar, and choose Pin to taskbar.
instalar-passo-desafixar-windows = To remove it from the taskbar: right-click the icon and choose Unpin from taskbar.
instalar-passo-dock = To remove it from the Dock: drag the { -app } icon out of it.
instalar-abrir-copia = Open the installed version
instalar-erro-abrir-copia = Could not open the installed version: { $erro }
instalar-erro-desinstalar = Could not schedule the removal of the folders: { $erro }

# The summary on the Settings page.
instalar-resumo-instalado = Installed in { $pasta }
instalar-resumo-parcial = Partly installed
instalar-resumo-nao = Not installed
