# Settings: the page's catalog (M35). Each card has the key `config-<JSON path with . → ->`,
# which is also the card's id on screen: the title, the `.descricao` (description) and the
# `.palavras` (extra search words). Sections come from `config-secao-*`.

## Sections

config-secao-geral = General
    .resumo = Theme, language, start page, sessions and recents

config-secao-terminal = Terminal
    .resumo = Font, colors, cursor, copy and paste, duplicate tab, local shell and logs

config-secao-conexoes = Connections
    .resumo = Defaults for new connections, known_hosts, keepalive and reconnect

config-secao-scripts = Scripts
    .resumo = Interpreter, parallel runs and scripts folder

config-secao-cofre = Password vault
    .resumo = Protection, locking, where the vault lives and password generator

config-secao-notas = Notes
    .resumo = Editing mode, font and line wrapping

config-secao-agente = SSH keys and agent
    .resumo = The agent, new keys and use by the system

config-secao-arquivos = Files
    .resumo = Transfers, conflicts, editor, compare and sync

config-secao-ftp = FTP
    .resumo = Security, mode, TLS and encoding

config-secao-s3 = S3
    .resumo = Region, address style, parts, class and links

config-secao-rclone = rclone
    .resumo = The program, the config file and the password

config-secao-redes = Networks
    .resumo = Network profiles, wait time and when to bring down

config-secao-x11 = X11 and tunnels
    .resumo = Default X11, display, virt-manager and tunnels

config-secao-portas = Server ports
    .resumo = Refresh, probe, notices and links through the tunnel

config-secao-janelas = Windows and workspaces
    .resumo = Workspaces folder, open at startup and tear off tabs

config-secao-ia = AI assistants
    .resumo = Profiles, what leaves the computer and terminal context

config-secao-ia-servidor = Assistants on the server
    .resumo = Claude Code, Codex and OpenCode on the server

config-secao-ia-local = Assistants on this computer
    .resumo = Claude Code, Codex and OpenCode on this computer

config-secao-mcp = Control by assistants (MCP)
    .resumo = Enable, permissions, workspace and audit

config-secao-fim = End of the page

## Cards

# General

config-geral-tema = Theme
    .descricao = Light, dark, or the system's.
    .palavras = appearance colors dark light

config-geral-idioma = Language
    .descricao = The language of the interface. It takes effect after you restart { -app }.
    .palavras = language idioma lingua portuguese english translation

config-geral-pagina_inicial = Startup page
    .descricao = Automatic: Terminals if tabs were open last time; otherwise, Home.
    .palavras = open start home

config-geral-restaurar_sessoes = Reopen tabs and windows at startup
    .descricao = Off, the app starts empty (what was open stays saved).
    .palavras = sessions restore tabs

config-geral-confirmar_fechar = Ask before closing with open sessions
    .descricao = Off, closing the window closes right away.
    .palavras = exit quit confirm close

config-geral-ao_fechar = When the window is closed
    .descricao = Ask, keep the app in the tray (the agent keeps running), or close. Needs a system tray.
    .palavras = exit quit tray hide close minimize

config-geral-iniciar_com_sistema = Start with the system
    .descricao = Opens the app when you sign in, so the key agent is already running.
    .palavras = autostart startup login tray boot

config-geral-iniciar_na_bandeja = When starting with the system, stay in the tray
    .descricao = Off, the window opens. On, only the tray icon appears. Needs a system tray.
    .palavras = autostart startup hidden minimized tray window

config-geral-atalho_no_menu = Shortcut in the applications menu
    .descricao = Shows { -app } in the system menu and dock, with its own icon.
    .palavras = desktop dock launcher icon menu applications

config-geral-medidor_cpu = CPU meter in the title bar
    .descricao = Shows how much of the processor { -app } uses. Clicking it opens the Performance page, with the weight of each terminal.
    .palavras = cpu processor performance usage load meter chart task manager

config-geral-recentes_max = Recents kept
    .descricao = How many connections the recents list keeps, not counting pinned ones.
    .palavras = history latest

config-acao-limpar_recentes = Clear recents
    .descricao = Clears the recents list and keeps the pinned ones.
    .palavras = delete history

config-geral-assistente_ia = AI assistants
    .descricao = The chat with the AI profiles, and Claude Code, Codex and OpenCode on the servers. When off, their menus, rows, sections and shortcuts disappear; what is saved stays.
    .palavras = ai artificial intelligence chat claude codex opencode disable hide

config-geral-mcp = Control by assistants (MCP)
    .descricao = Assistants controlling the app, "Control with" and the cluster with AI. When off, they disappear like the AI ones and the MCP server does not start.
    .palavras = mcp ai control assistants cluster disable hide

config-acao-instalacao = Install on this computer
    .descricao = Copies the app to a fixed folder and creates the shortcuts, the start with the system, and the Installed apps entry.
    .palavras = install installation uninstall copy shortcut start menu desktop taskbar dock apps

config-acao-pasta_dados = Data folder
    .descricao = Where the connections, the vault, and the workspaces are kept.
    .palavras = storage usb sync change dropbox

config-acao-sobre = About { -app }
    .descricao = Version, license, and what the app is.
    .palavras = version license

# Terminal

config-terminal-tamanho_fonte = Font size
    .descricao = The size of the text in all open terminals.
    .palavras = font text zoom size

config-terminal-esquema = Color scheme
    .descricao = The terminal colors: Campbell and other schemes.
    .palavras = colors palette campbell tango vintage one half theme

config-terminal-cursor = Cursor shape
    .descricao = Bar, block, or underline.
    .palavras = cursor block bar underline

config-terminal-cursor_piscando = Blinking cursor
    .descricao = Off, the cursor stays steady even if the program asks it to blink.
    .palavras = cursor blink

config-terminal-historico = History lines
    .descricao = How many lines the terminal keeps for scrolling. Applies to new sessions.
    .palavras = scrollback scroll buffer

config-terminal-copiar_ao_selecionar = Copy on select
    .descricao = When you release the mouse, the selected text goes to the clipboard.
    .palavras = copy selection clipboard

config-terminal-avisar_colar = Warn when pasting multiple lines
    .descricao = Asks before pasting text with more than one line.
    .palavras = paste confirm

config-terminal-colar_arquivos = When pasting an image or file in an SSH session
    .descricao = What to do when the clipboard has an image or files, or when you drag files onto the terminal.
    .palavras = paste image file screenshot print claude codex opencode sftp tmp

config-terminal-pasta_colar = Server folder for what is pasted
    .descricao = Where pasted files stay on the server. ~ is the home folder. If it can't be used, the app uses ~/.cache/tirech-colar.
    .palavras = paste folder tmp temporary

config-terminal-ctrl_v_arquivos = Ctrl+V pastes images and files
    .descricao = With no text on the clipboard, Ctrl+V uploads the image or files and pastes the path. With text, it still goes to the program.
    .palavras = ctrl+v paste image

config-terminal-limpar_colados_dias = Delete from the server what was pasted more than
    .descricao = In days. 0 never deletes. The cleanup runs on the first upload of each session.
    .palavras = clean delete temporary days

config-terminal-colar_tar_min_arquivos = Send the folder as a single package from
    .descricao = In files. A folder with at least this many files (or with 10 or more, all small) goes as a single tar package, much faster. The app builds the package itself; the server only needs tar to unpack it, and without it the folder goes over SFTP. 0 always sends over SFTP.
    .palavras = folder tar package many files fast

config-terminal-fechar_painel_vazio = Close the pane when its last tab leaves
    .descricao = A pane left without tabs goes away on its own, if there is another pane.
    .palavras = pane quadrant empty close

config-terminal-duplicar_na_pasta = Duplicate in the current folder
    .descricao = The duplicated tab opens in the folder the original is in. Over SSH, the app types a cd when the prompt appears.
    .palavras = duplicate tab folder directory cd clone copy

config-terminal-links_com_ctrl = Open links with Ctrl+click
    .descricao = Ctrl+click opens the address on this computer; while Ctrl is held, links are underlined. When off, the right-click menu still has Copy link.
    .palavras = link url hyperlink browser ctrl click

config-terminal-botao_direito = Right button
    .descricao = Copies (with a selection) and pastes (without), and Ctrl+right-click opens the menu; or always opens the menu.
    .palavras = mouse menu paste

config-terminal-campainha = Bell
    .descricao = What happens when the program rings the bell.
    .palavras = bell sound flash alert

config-terminal-shell = Local shell
    .descricao = The program that "Open local shell" runs.
    .palavras = bash zsh powershell cmd

config-terminal-shell-programa = Program
    .descricao = Empty uses $SHELL (or cmd.exe).
    .palavras = shell bash zsh executable

config-terminal-shell-argumentos = Arguments
    .descricao = Separated by spaces.
    .palavras = shell parameters options

config-terminal-shell-pasta = Starting folder
    .descricao = Where the local shell starts; accepts ~. A local connection can have its own.
    .palavras = shell directory cwd cd start

config-terminal-logs = Session logs
    .descricao = Writes what appears in the terminal to a file.
    .palavras = record log

config-terminal-logs-pasta = Logs folder
    .descricao = Empty uses the logs folder inside the data folder.
    .palavras = log record directory

config-terminal-logs-todas_as_sessoes = Log all sessions
    .descricao = Each new session writes a file named after the connection and the time.
    .palavras = log record automatic

config-terminal-logs-cifrar = Encrypt logs
    .descricao = New logs are saved encrypted (.log.cifrado) and only open with the vault unlocked. The logs key is kept in the vault.
    .palavras = log encrypt cipher protect secret

config-terminal-logs-selar_secs = Seal the encrypted log every (seconds)
    .descricao = How often recent text goes to disk. If the app crashes, at most this much is lost.
    .palavras = log encrypted disk write interval

config-terminal-logs-visualizar_mib = Log size in the Logs page (MiB)
    .descricao = How much of the end of the log the page shows at once. 0 shows everything, and a large log takes a while to open.
    .palavras = log view limit memory read

# Connections

config-conexoes-usuario_padrao = Default user
    .descricao = Fills in new connections. Empty uses the system user.
    .palavras = login user name

config-conexoes-porta_padrao = Default SSH port
    .descricao = The port of new connections.
    .palavras = 22 ssh

config-conexoes-term_padrao = Default TERM
    .descricao = The terminal type new connections announce: xterm-256color, xterm, vt100, linux.
    .palavras = xterm vt100 terminfo

config-conexoes-keepalive_padrao_secs = Default keepalive
    .descricao = Seconds between the signals that keep the connection alive (0 turns it off). New connections.
    .palavras = keep alive timeout drop

config-conexoes-known_hosts = known_hosts file
    .descricao = Where the keys of known servers are kept. Empty uses ~/.ssh/known_hosts.
    .palavras = hosts servers fingerprint

config-conexoes-chave_nova = Unknown server
    .descricao = What to do with the key of a server that known_hosts doesn't have yet. A changed key is still refused.
    .palavras = host key trust accept ask

config-conexoes-reconectar = When the connection drops
    .descricao = What the app does when the connection drops (not when the shell ends with exit).
    .palavras = reconnect drop lost retry

config-conexoes-reconectar-modo = What to do
    .descricao = Do nothing, offer to reconnect, or reconnect on its own.
    .palavras = reconnect automatic

config-conexoes-reconectar-tentativas = Attempts
    .descricao = How many times to reconnect on its own (1 to 20).
    .palavras = reconnect times retries

config-conexoes-reconectar-espera_secs = Wait between attempts
    .descricao = Seconds before each attempt (1 to 300).
    .palavras = reconnect interval time delay

config-conexoes-reconectar-retentar_mins = Retry after (minutes)
    .descricao = When the attempts run out, waits this many minutes and starts another round, until the connection is back; 0 turns it off (0 to 1440). Each connection or folder can choose another time in its properties.
    .palavras = reconnect again round pause minutes keep trying

config-conexoes-abrir_em = Open connection in
    .descricao = Where a double-click opens in the tree, on Home, and in search.
    .palavras = window pane tab

config-acao-importar_ssh_config = Import from ~/.ssh/config
    .descricao = Brings in the hosts from the OpenSSH configuration file.
    .palavras = openssh hosts import

config-acao-importar_aws = Import from ~/.aws
    .descricao = Brings in the AWS profiles as S3 connections.
    .palavras = amazon credentials import s3

config-conexoes-confirmar_fechar_todas = Ask before closing all tabs
    .descricao = When you use Tools > Close all tabs with open sessions, asks for confirmation.
    .palavras = close tabs all confirm ask

config-conexoes-tmux_ao_fechar = When closing a tab with tmux or screen
    .descricao = What to do with the tmux or screen session on the server when you close a tab that runs in it. Closing the app never ends it: the tabs come back to the same sessions.
    .palavras = tmux screen persistent session close tab kill keep

config-conexoes-comandos_quieto_ms = Screen idle after (ms)
    .descricao = How long without output counts as an idle screen, for the commands on connect (50 to 60000).
    .palavras = commands connect sudo steps idle wait

config-conexoes-comandos_max_secs = Time limit of each step (s)
    .descricao = The longest the new steps of the commands on connect wait (1 to 86400).
    .palavras = commands connect sudo steps timeout limit

config-conexoes-importar_senhas = Import passwords by default
    .descricao = When importing from another program, leaves the box that stores passwords in the vault checked.
    .palavras = import password vault winsshterm filezilla winscp

# Scripts

config-scripts-interpretador = Interpreter
    .descricao = The program that reads the script file on its input (sh -s, bash -s). A typed line runs as is.
    .palavras = script shell bash sh command

config-scripts-paralelo = Servers at the same time
    .descricao = How many servers run the script together (1 to 64); the others wait their turn.
    .palavras = script parallel simultaneous threads

config-scripts-pasta = Scripts folder
    .descricao = Empty uses the scripts folder inside the data folder.
    .palavras = script directory files sh

# Password vault

config-acao-protecao_cofre = Vault protection
    .descricao = How the vault is encrypted: a key stored in the file, or a master password.
    .palavras = master password encrypt encryption security

config-cofre-limpar_area_transferencia_secs = Clear the clipboard
    .descricao = After copying a password, clears it from the clipboard if it is still there.
    .palavras = clipboard copy password clear

config-cofre-bloquear_apos_min = Lock when idle
    .descricao = Locks the vault after a while without using a password. Needs a master password.
    .palavras = lock inactivity timeout master password

config-acao-cofre_local = Vault location
    .descricao = The folder of vault.json and vault.key. Empty uses the data folder; in a synced folder (Syncthing, Dropbox, OneDrive), the vault goes along to your other machines.
    .palavras = folder sync syncthing dropbox onedrive vault directory

config-acao-cofre_levar = Take the vault to another machine…
    .descricao = Saves a .tirech-vault file with the vault, to open on another computer.
    .palavras = export copy tirech-vault backup move

config-acao-cofre_abrir_outro = Open a vault from another machine…
    .descricao = Merges a .tirech-vault (or a vault.json) into this vault, or uses it instead.
    .palavras = import merge tirech-vault sync

config-cofre-gerador = Password generator
    .descricao = What the vault's Generate button uses.
    .palavras = generate random password

config-cofre-gerador-tamanho = Length
    .descricao = From 8 to 128 characters.
    .palavras = generator size

config-cofre-gerador-maiusculas = Uppercase letters
    .descricao = Includes A to Z.
    .palavras = generator

config-cofre-gerador-minusculas = Lowercase letters
    .descricao = Includes a to z.
    .palavras = generator

config-cofre-gerador-digitos = Digits
    .descricao = Includes 0 to 9.
    .palavras = generator numbers

config-cofre-gerador-simbolos = Symbols
    .descricao = Includes !, #, $ and similar.
    .palavras = generator special

config-cofre-gerador-sem_ambiguos = No ambiguous characters
    .descricao = Leaves out 0, O, 1, l, and I, which look alike.
    .palavras = generator similar

config-acao-csv_cofre = Import and export CSV
    .descricao = Opens the vault page, where import and export are.
    .palavras = spreadsheet import export backup

# Notes

config-notas-modo = Mode on open
    .descricao = How the Notes page opens a note: text only, text with a preview beside it, or preview only.
    .palavras = markdown edit split read preview

config-notas-fonte_mono = Monospaced editor font
    .descricao = Writes the Markdown in a font where all letters have the same width.
    .palavras = markdown monospace code font

config-notas-quebrar_linhas = Wrap long lines
    .descricao = Continues the line on the next one instead of adding a horizontal scroll bar.
    .palavras = markdown wrap scroll width

# SSH keys and agent

config-agente-ligado = Built-in agent
    .descricao = The app's agent keeps the open keys for connections and for other programs.
    .palavras = ssh-agent keys socket

config-agente-socket = Socket address
    .descricao = Empty uses the system's default address (the openssh-ssh-agent pipe on Windows). Takes effect when the agent restarts.
    .palavras = ssh_auth_sock path

config-agente-exportar_no_shell = Use in the local shell
    .descricao = Sets SSH_AUTH_SOCK in the local shells opened afterwards.
    .palavras = ssh_auth_sock environment

config-agente-carregar_ao_iniciar = Load keys at startup
    .descricao = Opens the keys on the agent's list when the app starts.
    .palavras = keys automatic

config-agente-vida_padrao_secs = Default lifetime
    .descricao = How long a new key stays in the agent. Applies to keys added afterwards.
    .palavras = expiry expire lifetime

config-agente-confirmar_padrao = Confirm each use
    .descricao = Asks for confirmation each time keys added afterwards are used.
    .palavras = confirm permission

config-agente-encaminhar_padrao = Forward the agent by default
    .descricao = SSH connections use the app's agent on the server (agent forwarding).
    .palavras = forward forwarding -A

config-agente-pasta_chaves = Keys folder
    .descricao = Where Generate key saves and what the Keys page lists. Empty uses ~/.ssh.
    .palavras = ssh directory id_ed25519

config-agente-chaves_novas = Type and protection of new keys
    .descricao = What the Generate key dialog comes with already chosen.
    .palavras = generate ed25519 rsa ecdsa

config-agente-tipo_padrao = Type
    .descricao = Ed25519, ECDSA, or RSA.
    .palavras = ed25519 rsa ecdsa algorithm

config-agente-rodadas_padrao = Passphrase protection
    .descricao = How many bcrypt-pbkdf rounds: more is slower to crack and to open.
    .palavras = bcrypt kdf passphrase

config-agente-sistema_ssh_config = Use in the system's ssh (~/.ssh/config)
    .descricao = Writes a block at the top of ~/.ssh/config: the terminal, git, codex, and VS Code use the app's keys while it is open.
    .palavras = ssh identityagent git codex claude system

config-agente-sistema_shells = Also in shells (bash, zsh, fish)
    .descricao = Sets SSH_AUTH_SOCK in .bashrc, .zshrc, and fish, for programs that don't read ~/.ssh/config. Applies to shells opened afterwards.
    .palavras = ssh_auth_sock bashrc zshrc fish environment

config-agente-sistema_sessao = Also in the graphical session
    .descricao = Tells the user session (systemctl --user or launchctl) about SSH_AUTH_SOCK. Applies to programs opened afterwards.
    .palavras = ssh_auth_sock systemd launchctl environment

config-agente-sistema_git = Make Git use Windows OpenSSH
    .descricao = The ssh that comes with Git doesn't talk to the Windows agent. This points Git's core.sshCommand at Windows' ssh.exe.
    .palavras = git sshcommand openssh windows

config-agente-tirar_ao_bloquear = When the vault locks, remove its keys from the agent
    .descricao = File keys and keys that another program added to the agent stay.
    .palavras = lock vault keys

config-agente-pageant = Answer as Pageant (PuTTY, WinSCP, FileZilla)
    .descricao = Those programs look for Pageant and start using the app's keys. If PuTTY's Pageant is already open, it keeps answering.
    .palavras = putty winscp filezilla kitty windows

config-acao-gerenciar_chaves = Manage keys and agent
    .descricao = Opens the agent page.
    .palavras = keys agent

# Files

config-arquivos-paralelas = Simultaneous transfers
    .descricao = From 1 to 8. The ones in progress finish.
    .palavras = queue simultaneous parallel

config-arquivos-conflito = When the file already exists
    .descricao = What to do in the next transfers.
    .palavras = overwrite resume skip rename conflict

config-arquivos-mostrar_ocultos = Show hidden files
    .descricao = The initial state of the button in panes opened afterwards.
    .palavras = dotfiles hidden

config-arquivos-editor = External editor
    .descricao = The program used by Edit. Empty uses the system's.
    .palavras = edit vim code notepad

config-arquivos-enviar_editado = Upload the edited file automatically
    .descricao = Ask, or upload it back as soon as the file changes.
    .palavras = edit save upload

config-arquivos-preservar_data = Keep the modification date
    .descricao = In the next transfers.
    .palavras = mtime timestamp

config-arquivos-preservar_permissoes = Keep the permissions
    .descricao = In the next transfers.
    .palavras = chmod mode

config-arquivos-pasta_local = Initial local folder
    .descricao = The local pane of new tabs. Empty uses the home folder.
    .palavras = home directory

config-arquivos-confirmar_apagar = Ask before deleting
    .descricao = Off, Delete removes without the dialog.
    .palavras = remove erase confirm

config-arquivos-keepalive_secs = Browser keepalive
    .descricao = Seconds between the signals of the file browser's connection (0 turns it off). Connections opened afterwards.
    .palavras = sftp keep alive

config-arquivos-comparar = Compare folders on open
    .descricao = New tabs open already comparing the local pane with the remote one.
    .palavras = difference diff sync new yellow

config-arquivos-comparar_modo = Comparison criterion
    .descricao = By size or by modification date. Applies to new tabs.
    .palavras = difference diff size date newer

config-arquivos-comparar_tolerancia_min = Date tolerance (minutes)
    .descricao = A difference of up to this many minutes counts as the same date. FTP lists the date only down to the minute.
    .palavras = difference diff ftp mtime clock

config-arquivos-ocultar_identicos = Hide identical files when comparing
    .descricao = Only what differs shows up. Applies to new tabs.
    .palavras = difference diff hide same

config-arquivos-sincronizar = Synchronized browsing on open
    .descricao = New tabs open with synchronized browsing already on: entering or going up in one pane takes the other to the same folder. The base is the pair of folders the tab opens in.
    .palavras = sync browsing mirror link

# FTP

config-ftp-seguranca = Security
    .descricao = How new FTP connections protect the traffic.
    .palavras = ftps tls ssl encryption

config-ftp-modo = Data connection mode
    .descricao = Passive or active. With a network profile that has a proxy, the app forces passive.
    .palavras = pasv port passive active

config-ftp-tls = TLS version
    .descricao = The TLS version of new FTPS connections.
    .palavras = ftps tls 1.2 tls 1.3

config-ftp-codificacao = File name encoding
    .descricao = How to read the file names from the server.
    .palavras = utf-8 latin-1 accents charset

# S3

config-s3-regiao = Default region
    .descricao = For new S3 connections with the "other" provider.
    .palavras = region us-east-1 aws

config-s3-estilo = Default address style
    .descricao = Path or virtual host, for new S3 connections with the "other" provider.
    .palavras = path style virtual host endpoint

config-s3-parte_mib = Part size
    .descricao = The size of each part of a multipart upload, from 5 to 512 MiB.
    .palavras = multipart upload mib

config-s3-classe = Default storage class
    .descricao = STANDARD, STANDARD_IA, INTELLIGENT_TIERING, GLACIER_IR. Empty uses the bucket's.
    .palavras = storage class glacier standard

config-s3-validade_link = Default link validity
    .descricao = The first item of the Copy presigned link menu.
    .palavras = presigned share url expires

# rclone

config-rclone-binario = rclone program
    .descricao = Empty looks in the PATH. Applies to connections opened afterwards.
    .palavras = binary executable path

config-rclone-arquivo_config = Configuration file
    .descricao = Empty uses rclone's default.
    .palavras = rclone.conf remotes

config-rclone-credencial = Encrypted configuration password
    .descricao = The vault item with the configuration file's password. The password never leaves the vault.
    .palavras = config password vault encrypted

config-acao-testar_rclone = Version
    .descricao = Looks for rclone and shows its version.
    .palavras = test version installed

# Networks

config-acao-perfis_de_rede = Network profiles
    .descricao = One network per client: proxy, jump host, and how to bring the VPN up.
    .palavras = proxy vpn jump socks

config-redes-espera_padrao_secs = Default proxy wait
    .descricao = Seconds a new profile waits for the network to come up (1 to 600).
    .palavras = timeout profile vpn

config-redes-descer_ao_fechar = Bring networks down on close
    .descricao = Off, closing the app doesn't run the profiles' down command for those in use.
    .palavras = vpn disconnect close

config-redes-navegador = Browser for links
    .descricao = Links with a network profile open in it, in a separate window per profile that goes out through the client's network. Links without a profile open in the default browser.
    .palavras = links chrome chromium edge brave firefox browser web url

# X11 and tunnels

config-x11-padrao = Default X11
    .descricao = The mode of SSH connections that don't choose X11. Trusted is ssh -Y; untrusted is ssh -X.
    .palavras = x11 display windows graphical xauth virt-manager

config-x11-display = X server from here (DISPLAY)
    .descricao = Empty uses the DISPLAY variable. On Windows, VcXsrv or Xming are usually localhost:0; on macOS, XQuartz.
    .palavras = x11 display xming vcxsrv xquartz xwayland

config-x11-virt_manager = Remote virt-manager command
    .descricao = What the Open virt-manager menu runs on the server.
    .palavras = virt-manager libvirt kvm virtual machines

config-tuneis-outras_maquinas = Allow other machines on new tunnels
    .descricao = New -L and -D tunnels listen on all interfaces, not just 127.0.0.1. Each tunnel can change this.
    .palavras = tunnel port forwarding socks gatewayports network listen

# Server ports (M58)

config-portas-intervalo_secs = Refresh the ports every (seconds)
    .descricao = With the Ports panel open, the list refreshes at this interval. 0 refreshes only on the button.
    .palavras = ports server refresh interval listen

config-portas-sondar = Look at what answers on new ports
    .descricao = Opens a short connection through SSH on each new port to find the page title and apps such as phpMyAdmin.
    .palavras = ports probe title phpmyadmin http

config-portas-sonda_ms = Probe time (milliseconds)
    .descricao = The total time the probe waits for a port. Minimum 100.
    .palavras = ports probe wait timeout

config-portas-usar_ferramentas = Use the server's ss, netstat, docker and podman
    .descricao = When present, they bring each port's program and the containers. Off, only SFTP reads the ports.
    .palavras = ports ss netstat docker podman containers

config-portas-avisar_novas = Warn when a new port shows up
    .descricao = A notice in SSH sessions with a button to open the panel. Works with the panel closed.
    .palavras = ports notice new notify

config-portas-links_pelo_tunel = Open local links through the tunnel
    .descricao = A localhost:3000 in an SSH tab's terminal opens the connection's server, not this computer.
    .palavras = ports links terminal localhost tunnel browser

config-portas-mostrar_sistema = Show system ports
    .descricao = Includes SSH itself, the local DNS and other system ports in the list.
    .palavras = ports system ssh dns

# Windows and workspaces

config-janelas-pasta_espacos = Workspaces folder
    .descricao = Empty uses the espacos folder inside the data folder.
    .palavras = workspace directory layouts

config-janelas-abrir_espaco = Open a saved window workspace in
    .descricao = Where a double-click opens on a workspace from Window or All.
    .palavras = workspace window

config-janelas-tear_off = Dropping a tab outside the window opens another
    .descricao = Off, dropping outside does nothing.
    .palavras = detach drag tab window

config-acao-perfis_de_ia = AI providers
    .descricao = Where the models are: on your computer, on your local network, or on the internet.
    .palavras = ai model ollama openai anthropic openrouter groq key api lm studio

config-ia-redigir = Remove passwords and keys from what goes to the AI
    .descricao = Replaces what the app recognizes as a password or key with [segredo removido] before sending.
    .palavras = ai redaction secret privacy mask

config-ia-redigir_extras = Other texts to remove
    .descricao = One per line. Start a line with re: to use a regular expression.
    .palavras = ai redaction regex regular expression secret privacy

config-ia-mostrar_no_menu = Show the AI items in the terminal menu
    .descricao = Explain the selection, explain the last error and generate a command.
    .palavras = ai menu context right click

config-ia-mostrar_o_que_sai = Show what will be sent before the first send of each tab
    .descricao = Opens the exact text, already without the secrets, so you can check it before sending.
    .palavras = ai privacy confirm send

config-ia-linhas_contexto = Terminal lines in the context
    .descricao = How many of the last visible terminal lines go with the request (1 to 100,000).
    .palavras = ai context history

config-ia-max_caracteres = Maximum context size
    .descricao = In characters. Past that, the app cuts the oldest lines. 0 means no limit.
    .palavras = ai context limit local model

config-ia-comando_shell = Shell to assume
    .descricao = Used when the app can't tell which shell the server runs.
    .palavras = ai bash zsh fish powershell

config-ia-espera_resposta = Wait for the answer for
    .descricao = In seconds. 0 waits without a limit, which suits slow local models.
    .palavras = ai timeout time limit

config-ia-mcp_ligado = Let assistants control the app
    .descricao = Claude Code and Codex can see and use the connections you allow, through MCP. Tirech Omni must be open.
    .palavras = mcp assistant ai claude codex control turn on

config-ia-bloqueio_extras = Commands that always ask for approval
    .descricao = One per line: a program name or, with re: at the start, a regular expression. They apply on top of the app's block list, even with the connection on Free.
    .palavras = mcp block permission approve command dangerous regex

config-ia-auditoria = Write a log of what the assistants do
    .descricao = One file per day in the data folder, without passwords. Commands and texts go through the same secret removal that goes to the AI.
    .palavras = mcp audit log history secret

config-ia-marca_secs = Mark the tab after the last action for
    .descricao = In seconds. How long the tab shows that an assistant touched it.
    .palavras = mcp mark tab time ai

config-ia-linhas_por_leitura = Lines per file read
    .descricao = How many lines the assistant gets when it reads a workspace file without asking for a range (1 to 100,000).
    .palavras = mcp read file lines workspace

config-ia-caracteres_por_resposta = Maximum size of an answer
    .descricao = In characters. Past that, the app cuts it and says how to continue. 0 does not limit.
    .palavras = mcp answer limit size characters

config-ia-comandos_na_aba = Run the assistants' commands in the tab
    .descricao = Even when the assistant asks to run it separately, the command shows up in the tab, as if you had typed it. If the tab is busy (a program open or another command running), it runs separately, out of sight.
    .palavras = mcp run command tab visible separate terminal

config-ia-finais_de_prompt = Extra prompt endings
    .descricao = One per line. The app knows # $ % and > as the end of a prompt; what you add here counts too, to tell when a command has finished.
    .palavras = mcp prompt terminal run end command

config-ia-pasta_troca = Folder to swap files with the assistant
    .descricao = Where the files the assistant downloads land and where the ones it sends come from. Empty is Downloads/Tirech Omni.
    .palavras = mcp files swap download upload folder

config-ia-preferido = Shortcut assistant
    .descricao = What Ctrl+Alt+A opens, installs or checks on the SSH tab you are in.
    .palavras = ai assistant claude codex opencode shortcut preferred default

config-ia-abrir_em = Open the assistant in
    .descricao = In a new tab, in a free pane of the grid, or by typing the command in the current tab (only at the shell, not inside full-screen programs).
    .palavras = ai assistant open tab pane window where

config-ia-shell_ao_sair = Stay in a shell when the assistant ends
    .descricao = The tab stays open, in the same folder, instead of closing.
    .palavras = ai assistant exit quit close shell tab

config-ia-tunel_codex = Open port 1455 on the Codex login
    .descricao = Lets the login through this computer's browser work. If the port is in use, use Sign in > Codex (with a code).
    .palavras = ai codex login tunnel port 1455 browser

config-ia-verificar_ao_abrir = Check the assistants when an SSH tab opens
    .descricao = Runs a short command on the server to see what is installed. The result lasts 10 minutes.
    .palavras = ai assistant check probe installed server

config-ia-mostrar_botao = Show the assistants button on the bar
    .descricao = The menu is also in Tools, in the tab menu and in the connection menu.
    .palavras = ai assistant button bar header

config-ia-mostrar_claude = Show Claude Code in the menus
    .descricao = Turned off, it leaves the menus and what the shortcut opens.
    .palavras = ai claude code anthropic menu

config-ia-mostrar_codex = Show Codex in the menus
    .descricao = Turned off, it leaves the menus and what the shortcut opens.
    .palavras = ai codex openai menu

config-ia-mostrar_opencode = Show OpenCode in the menus
    .descricao = Turned off, it leaves the menus and what the shortcut opens.
    .palavras = ai opencode menu

config-ia-metodo_claude = Install Claude Code with
    .descricao = Automatic uses the official installer and, if the server can't, npm.
    .palavras = ai claude install method npm homebrew native

config-ia-metodo_codex = Install Codex with
    .descricao = Automatic uses the official installer and, if the server can't, npm.
    .palavras = ai codex install method npm homebrew native

config-ia-metodo_opencode = Install OpenCode with
    .descricao = Automatic uses the official installer and, if the server can't, npm.
    .palavras = ai opencode install method npm homebrew native

config-ia-argumentos_claude = Claude Code arguments
    .descricao = They go after the command, as you write them. A connection can have its own.
    .palavras = ai claude arguments options parameters

config-ia-argumentos_codex = Codex arguments
    .descricao = They go after the command, as you write them. A connection can have its own.
    .palavras = ai codex arguments options parameters

config-ia-argumentos_opencode = OpenCode arguments
    .descricao = They go after the command, as you write them. A connection can have its own.
    .palavras = ai opencode arguments options parameters

config-ia-local_pasta = Assistants' working folder
    .descricao = Where Control with opens this computer's Claude Code, Codex and OpenCode, and where what they create goes. Empty uses a folder of the app, away from your files.
    .palavras = ai claude codex opencode folder working directory junk clean control

config-acao-ia_local_abrir_pasta = Open the working folder
    .descricao = Shows what the assistants left in the file manager.
    .palavras = ai claude codex folder working open

config-acao-ia_local_limpar = Clean the working folder…
    .descricao = Deletes everything inside it. Open assistants keep running.
    .palavras = ai claude codex folder working clean delete junk

config-ia-local_claude_caminho = Claude Code path
    .descricao = The program on this computer. Empty searches the system folders and yours.
    .palavras = ai claude path executable program computer local

config-ia-local_claude_argumentos = Claude Code arguments
    .descricao = They go after the command when you use Control with Claude Code. Put any options you like here.
    .palavras = ai claude arguments options parameters computer local control

config-ia-local_codex_caminho = Codex path
    .descricao = The program on this computer. Empty searches the system folders and yours.
    .palavras = ai codex path executable program computer local

config-ia-local_codex_argumentos = Codex arguments
    .descricao = They go after the command when you use Control with Codex. Put any options you like here.
    .palavras = ai codex arguments options parameters computer local control

config-ia-local_opencode_caminho = OpenCode path
    .descricao = The program on this computer. Empty searches the system folders and yours.
    .palavras = ai opencode path executable program computer local

config-ia-local_opencode_argumentos = OpenCode arguments
    .descricao = They go after the command when you use Control with OpenCode. Put any options you like here.
    .palavras = ai opencode arguments options parameters computer local control

config-ia-conversas_lembrar = Remember each tab's conversations
    .descricao = Keeps the id of the Claude Code, Codex and OpenCode conversations with each tab, to continue later. Off, Control with always starts a new conversation.
    .palavras = ai claude codex opencode conversation session resume continue journal history remember

config-ia-conversas_reabrir = Reopen the assistants when the app opens
    .descricao = When the app or a workspace opens, the assistants that were controlling the tabs come back, continuing the same conversation.
    .palavras = ai claude codex opencode conversation reopen restore continue workspace

config-ia-conversas_ao_controlar = When the tab already had a conversation
    .descricao = What Control with does when the assistant already talked about that tab: ask, continue the latest or start another.
    .palavras = ai claude codex opencode conversation continue new ask control

config-acao-mcp_como = How to connect an assistant…
    .descricao = The commands for Claude Code and Codex, ready to copy.
    .palavras = mcp claude codex connect setup bridge

config-acao-mcp_registro = See today's log
    .descricao = Opens the day's file with what the assistants did.
    .palavras = mcp audit log

config-acao-mcp_pasta_registro = Open the log folder
    .descricao = The folder with one file per day.
    .palavras = mcp audit log folder

config-acao-mcp_espaco_novo = New workspace…
    .descricao = A folder on a server that the assistant reads and edits directly, without opening a terminal.
    .palavras = mcp workspace folder server ssh sftp

# End of the page

config-acao-exportar = Export settings
    .descricao = Saves a file with the settings, without passwords.
    .palavras = backup save json

config-acao-importar = Import settings
    .descricao = Reads an exported file and replaces the settings.
    .palavras = restore load json

config-acao-restaurar_tudo = Reset all settings
    .descricao = Puts everything back to default and keeps the data folder.
    .palavras = default reset clear

## Choice options (`escolha!`): `opcao-<choice>-<variant>`

# Tema
opcao-tema-sistema = Use the system theme
opcao-tema-claro = Light
opcao-tema-escuro = Dark

# Esquema
opcao-esquema-campbell = Campbell
opcao-esquema-tema = Follow the app theme
opcao-esquema-campbell-powershell = Campbell Powershell
opcao-esquema-vintage = Vintage
opcao-esquema-one-half-dark = One Half Dark
opcao-esquema-one-half-light = One Half Light
opcao-esquema-tango-dark = Tango Dark
opcao-esquema-tango-light = Tango Light

# Cursor
opcao-cursor-barra = Bar
opcao-cursor-bloco = Block
opcao-cursor-sublinhado = Underline

# BotaoDireito
opcao-botao-direito-menu = Open the menu
opcao-botao-direito-copiar-colar = Copy and paste

# ColarArquivos
opcao-colar-arquivos-enviar = Upload and paste the path
opcao-colar-arquivos-caminho-local = Paste only the local path
opcao-colar-arquivos-perguntar = Ask

# Campainha
opcao-campainha-piscar = Flash the screen
opcao-campainha-nenhuma = None

# TmuxAoFechar
opcao-tmux-ao-fechar-perguntar = Ask
opcao-tmux-ao-fechar-encerrar = End the session
opcao-tmux-ao-fechar-manter = Keep on the server

# ConversaAoControlar
opcao-conversa-ao-controlar-perguntar = Ask
opcao-conversa-ao-controlar-continuar = Continue the latest
opcao-conversa-ao-controlar-nova = Start a new one

# ChaveNova
opcao-chave-nova-perguntar = Ask
opcao-chave-nova-aceitar = Accept and save

# X11Padrao
opcao-x11-padrao-desligado = Off
opcao-x11-padrao-nao-confiavel = Untrusted
opcao-x11-padrao-confiavel = Trusted

# ModoReconectar
opcao-modo-reconectar-oferecer = Offer to reconnect
opcao-modo-reconectar-nada = Do nothing
opcao-modo-reconectar-sozinho = Reconnect automatically

# AbrirEm
opcao-abrir-em-painel-ativo = Active pane, last used window
opcao-abrir-em-nova-janela = New window

# TipoChave
opcao-tipo-chave-ed25519 = Ed25519
opcao-tipo-chave-ecdsa256 = ECDSA P-256
opcao-tipo-chave-ecdsa384 = ECDSA P-384
opcao-tipo-chave-ecdsa521 = ECDSA P-521
opcao-tipo-chave-rsa3072 = RSA 3072
opcao-tipo-chave-rsa4096 = RSA 4096

# ConflitoCfg
opcao-conflito-cfg-perguntar = Ask
opcao-conflito-cfg-sobrescrever = Overwrite
opcao-conflito-cfg-retomar = Resume
opcao-conflito-cfg-pular = Skip
opcao-conflito-cfg-renomear = Rename

# EnviarEditado
opcao-enviar-editado-perguntar = Ask
opcao-enviar-editado-sempre = Always

# ValidadeLink
opcao-validade-link-uma-hora = 1 hour
opcao-validade-link-um-dia = 1 day
opcao-validade-link-sete-dias = 7 days

# AbrirEspaco
opcao-abrir-espaco-nova-janela = New window
opcao-abrir-espaco-janela-ativa = Active window

# AoFechar
opcao-ao-fechar-perguntar = Ask
opcao-ao-fechar-bandeja = Keep in the tray
opcao-ao-fechar-fechar = Close the app

# ModoNotas
opcao-modo-notas-editar = Edit
opcao-modo-notas-dividir = Split
opcao-modo-notas-ler = Read

# ModoComparar
opcao-modo-comparar-tamanho = Size
opcao-modo-comparar-data = Modification date

# PaginaInicial
opcao-pagina-inicial-auto = Automatic
opcao-pagina-inicial-inicio = Home
opcao-pagina-inicial-terminais = Terminals

# Language: the names of the languages are written in the language itself, in both files
opcao-idioma-sistema = Use the system language
opcao-idioma-pt-br = Português (Brasil)
opcao-idioma-en-us = English
opcao-idioma-es-es = Español

## Page

config-titulo = Settings
config-buscar = Search settings
config-expandir-tudo = Expand all
config-recolher-tudo = Collapse all
config-busca-vazia = No settings found for "{ $busca }".
config-avisos-leitura = Some settings went back to their defaults
config-desfazer = Undo
config-desfeito = Undone.
config-nada-mudou = Nothing changed: it was already like this.
config-restaurar-secao = Restore this section's defaults
config-secao-restaurada = { $secao }: defaults restored.
config-tudo-restaurado = All settings went back to their defaults.
config-escolher = Browse…
config-limpar-pasta-titulo = Clean the working folder?
config-limpar-pasta-texto = { $n ->
    [one] Delete 1 item from { $pasta }? This can't be undone.
    *[other] Delete { $n } items from { $pasta }? This can't be undone.
    }
config-limpar-pasta-apagar = Delete
config-limpar-pasta-vazia = The working folder is already empty.
config-limpar-pasta-feito = { $n ->
    [one] 1 item deleted from the working folder.
    *[other] { $n } items deleted from the working folder.
    }
config-limpar-pasta-erro = { $n ->
    [one] 1 item deleted, but others couldn't be: { $erro }
    *[other] { $n } items deleted, but others couldn't be: { $erro }
    }
config-limpar-pasta-recusada = The working folder is your home folder or the root of the disk: the app won't empty it. Choose a folder just for the assistants.
config-abrir-pasta-erro = Couldn't open { $pasta }: { $erro }
config-bandeja-indisponivel = This system doesn't show tray icons (on GNOME, install the AppIndicator extension).
config-erro-abrir = Couldn't open { $caminho }: { $erro }
config-erro-gravar = Couldn't write { $caminho }: { $erro }
config-erro-ler = Couldn't read { $caminho }: { $erro }
config-exportar-titulo = Export settings
config-exportado = Settings exported to { $caminho }.
config-importar-titulo = Import settings
config-importado = Settings imported from { $caminho }.

# The language: the change takes effect after a restart.
config-geral-idioma-aviso = The new language takes effect after you restart { -app }.
config-geral-idioma-reiniciar = Restart now

# The data folder.
config-pasta-origem-variavel = { $pasta } (set by { $variavel }; to change it, remove the variable).
config-pasta-origem-portatil = { $pasta } (portable version: the tirech-omnidata folder next to the program).
config-pasta-portatil-senha = The vault in this folder has no master password
    .descricao = Anyone who copies the tirech-omnidata folder opens the saved passwords. To encrypt the vault with a password of your own, use Vault > Master password…
config-pasta-origem-escolhida = { $pasta } (chosen by you).
config-pasta-origem-padrao = { $pasta } (the default).
config-pasta-titulo = Data folder
config-pasta-abrir = Open folder
config-pasta-mudar = Change…
    .descricao = Copies the data to an empty folder, or uses one that already has data.
config-pasta-padrao = Back to default
config-pasta-reiniciar = Restart now
    .descricao = The new folder, { $pasta }, takes effect after a restart.
config-pasta-copiou = The data was copied to { $pasta }. Restart to use it.
config-pasta-apontou = The app will use the data already in { $pasta }. Restart to use it.
config-pasta-erro-mudar = Couldn't change the data folder: { $erro }.
config-pasta-voltou = The data folder goes back to { $pasta }. Restart to use it.
config-pasta-erro-padrao = Couldn't go back to the default: { $erro }.

# The vault location.
config-cofre-local-titulo = Vault location
config-cofre-local-pasta-dados = { $pasta } (the data folder).
config-cofre-local-pasta = { $pasta }.
config-cofre-local-abrir = Open folder
config-cofre-local-mudar = Change…
    .descricao = Moves the vault to another folder, or uses the one already there.
config-cofre-local-padrao = Back to the data folder

# The rclone test and the password for its configuration.
config-rclone-nao-achei = Couldn't find rclone. Install it or choose the program above.
config-rclone-terminou = { $programa } exited with { $status }.
config-rclone-erro-rodar = Couldn't run { $programa }: { $erro }
config-credencial-nenhuma = No vault item
config-credencial-inexistente = There's no item "{ $titulo }" in the vault (or the vault is locked).

# Shortcuts.
config-titulo-atalhos = Shortcuts
    .resumo = App and pane keys
config-atalhos-palavras = shortcut shortcuts keyboard key
config-atalhos-app = { -app }
    .descricao = New connection, layouts, vault, search and the app's other actions.
config-atalhos-paineis = Terminal panes
    .descricao = Split, move focus, resize, close and zoom the panes.
config-restaurar-atalhos = Restore all shortcuts
config-atalho-restaurar = Restore
config-atalho-sem-padrao = No default shortcut.
config-atalho-padrao = Default: { $atalho }.
config-atalho-tambem = Also used by { $outra }.

# Field hints (what applies when the field is empty).
config-dica-shell = $SHELL
config-dica-argumentos-shell = -l
config-dica-pasta-shell = the home folder
config-dica-pasta-colar = /tmp/tirech-colar
config-dica-logs = logs in the data folder
config-dica-usuario = the system user
config-dica-term = xterm-256color
config-dica-known-hosts = ~/.ssh/known_hosts
config-dica-interpretador = sh -s
config-dica-scripts = scripts in the data folder
config-dica-socket = an app address
config-dica-pasta-chaves = ~/.ssh
config-dica-editor = the system program
config-dica-pasta-local = the home folder
config-dica-regiao = us-east-1
config-dica-classe = the bucket's own
config-dica-rclone = rclone from PATH
config-dica-rclone-config = rclone's default
config-dica-navegador = Chrome, Edge, Brave, Chromium or Firefox
config-dica-display = the DISPLAY variable
config-dica-virt-manager = virt-manager --no-fork
config-dica-espacos = spaces in the data folder
config-dica-shell-ia = bash
config-dica-redigir-extras = one per line; re:… for a regex
config-dica-bloqueio-mcp = one per line; re:… for a regex
config-dica-finais-prompt = one per line
config-dica-argumentos-ia = for example --model sonnet
config-dica-caminho-ia-local = search automatically
config-dica-pasta-troca = Downloads/Tirech Omni
config-dica-atalho = Ctrl+Shift+N
atalho-nova-conexao = New connection
atalho-novo-grupo = New group
atalho-shell-local = Open local shell
atalho-layout-1 = Layout 1
atalho-layout-2x2 = Layout 2 × 2
atalho-layout-3x3 = Layout 3 × 3
atalho-identificar-paineis = Identify the panes
atalho-painel-conexoes = Show or hide connections
atalho-painel-propriedades = Show or hide properties
atalho-painel-notas = Show or hide the notes list
atalho-painel-fila = Show or hide transfers
atalho-nova-janela = New window
atalho-multi-entrada = Send to many
atalho-cluster = Cluster mode
atalho-cluster-ia = AI cluster
atalho-scripts = Run script
atalho-links = Open a link
atalho-reconectar-caidas = Reconnect the dropped tabs
atalho-fechar-caidas = Close the dropped tabs
atalho-fechar-todas = Close all tabs
atalho-cofre = Open the vault
atalho-notas = Open the notes
atalho-ia-comando = Generate a command with AI
atalho-ia-chat = Open or close the AI chat
atalho-ia-abrir = Open the AI assistant on the server
atalho-busca = Search connections and vault items
atalho-configuracoes = Open settings
atalho-aumentar-fonte = Increase the terminal font
atalho-diminuir-fonte = Decrease the terminal font
atalho-fonte-padrao = Default terminal font size
atalho-dividir-abaixo = Split the pane down
atalho-dividir-direita = Split the pane to the right
atalho-duplicar-sessao = Duplicate the session in a new pane
atalho-focar-esquerda = Focus the pane on the left
atalho-focar-direita = Focus the pane on the right
atalho-focar-acima = Focus the pane above
atalho-focar-abaixo = Focus the pane below
atalho-redimensionar-esquerda = Move the divider to the left
atalho-redimensionar-direita = Move the divider to the right
atalho-redimensionar-acima = Move the divider up
atalho-redimensionar-abaixo = Move the divider down
atalho-fechar-painel = Close the pane
atalho-zoom-painel = Toggle the pane zoom
config-logs-existentes = { $n ->
    [one] The logs folder has 1 text log, recorded before the logs were encrypted.
    *[other] The logs folder has { $n } text logs, recorded before the logs were encrypted.
    }
config-logs-existentes-cifrar = { $n ->
    [one] Encrypt the text log that already exists
    *[other] Encrypt the { $n } text logs that already exist
    }
config-ia-extra-invalido = { $texto }: { $erro }

# AssistentePreferido
opcao-assistente-claude = Claude Code
opcao-assistente-codex = Codex
opcao-assistente-opencode = OpenCode

# AbrirIa
opcao-abrir-ia-aba-nova = In a new tab
opcao-abrir-ia-painel-livre = In a free pane
opcao-abrir-ia-nesta-aba = In this tab

# MetodoIa
opcao-metodo-ia-automatico = Automatic
opcao-metodo-ia-nativo = Official installer
opcao-metodo-ia-npm = npm
opcao-metodo-ia-homebrew = Homebrew
