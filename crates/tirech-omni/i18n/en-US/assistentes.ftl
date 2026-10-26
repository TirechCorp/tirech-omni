# AI assistants on the remote server (M46): Claude Code, Codex and OpenCode.

# The connection properties.
ia-remoto-categoria = AI assistants
ia-remoto-pasta = Folder
ia-remoto-pasta-descricao = Where the assistants open on this connection. Empty: the terminal's folder, the default remote folder or the home folder.
ia-remoto-instalar-por = Install with
ia-remoto-instalar-por-descricao = How to install the assistants on this connection. Automatic follows the Settings.
ia-remoto-argumentos-claude = Claude Code arguments
ia-remoto-argumentos-codex = Codex arguments
ia-remoto-argumentos-opencode = OpenCode arguments
ia-remoto-argumentos-descricao = They go after the assistant's name, as you type them. Empty: the ones in Settings apply.

# The menus.
ia-remoto-menu = AI assistants
ia-remoto-abrir = Open { $nome }
ia-remoto-instalar = Install { $nome }…
ia-remoto-verificando = { $nome } (checking…)
ia-remoto-nao-verificou = { $nome } (could not check)
ia-remoto-outra-pasta = Open in another folder…
ia-remoto-entrar = Sign in
ia-remoto-entrar-claude = Claude Code (token without a browser)
ia-remoto-entrar-codex = Codex (with a code)
ia-remoto-entrar-opencode = OpenCode
ia-remoto-verificar = Check again
ia-remoto-botao-dica = AI assistants ({ $atalho })

# Notices.
ia-remoto-fora-do-ssh = Open an SSH tab to use the AI assistants.
ia-remoto-sem-senha = This connection asks for a password that is not in the vault. Open a tab of it first.
ia-remoto-resposta-ilegivel = The server answered something the app did not understand.
ia-remoto-nao-verificou-erro = Could not check the assistants: { $erro }
ia-remoto-porta-em-uso = Port 1455 is in use here. Use AI assistants › Sign in › Codex (with a code).

# The install dialog.
ia-remoto-instalar-titulo = Install { $nome } on { $conexao }
ia-remoto-instalar-texto = The app runs the command below in a new tab, without sudo. The installation goes in your home folder on the server.
ia-remoto-metodo-nativo = Official installer (recommended)
ia-remoto-metodo-npm = npm
ia-remoto-metodo-homebrew = Homebrew
ia-remoto-metodo-falta = { $metodo } — { $falta } is missing
ia-remoto-instalar-comando = Command
ia-remoto-instalar-copiar = Copy
ia-remoto-instalar-abrir-depois = Open when it finishes
ia-remoto-instalar-ok = Install
ia-remoto-falta-curl = The server has no curl. Choose npm or install curl (for example, sudo apt install curl).
ia-remoto-falta-tar = The server has no tar. Choose npm or install tar.
ia-remoto-falta-bash = The server has no bash. Choose npm or install bash.
ia-remoto-falta-npm = The server has no npm. Choose the official installer or install Node with npm.
ia-remoto-falta-brew = The server has no Homebrew. Choose another method.
ia-remoto-sem-node = { $nome } through npm needs Node { $minimo } or newer, and the server has no Node.
ia-remoto-falta-node-rodar = { $nome } through npm needs Node { $minimo } or newer to run. The server has Node { $tem }.
ia-remoto-falta-node-instalar = npm asks for Node { $minimo }, but { $nome } installs and runs with Node { $tem } (it is only an npm warning).
ia-remoto-falta-npm-sem-permissao = The global npm folder is not yours; the app installs in ~/.local with --prefix.
ia-remoto-falta-alpine = On Alpine, Claude Code needs "apk add bash curl libgcc libstdc++ ripgrep" (as root).
ia-remoto-falta-arquitetura = The official installer has no binary for { $arq }.

# The tabs.
ia-remoto-aba-instalar = Install { $nome }
ia-remoto-aba-entrar = Sign in to { $nome }

# The end of the installation.
ia-remoto-instalado = { $nome } installed on { $conexao } ({ $versao }).
ia-remoto-instalado-sem-versao = { $nome } installed on { $conexao }.
ia-remoto-abrir-agora = Open now
ia-remoto-instalacao-falhou = The { $nome } installation did not work. See the "{ $aba }" tab.
ia-remoto-tentar-de-novo = Try again

# The other folder dialog.
ia-remoto-pasta-titulo = Open in another folder
ia-remoto-pasta-ferramenta = Assistant
ia-remoto-pasta-nenhum = No assistant installed on this connection
ia-remoto-pasta-caminho = Folder on the server
ia-remoto-pasta-lembrar = Remember for this connection
ia-remoto-pasta-abrir = Open
