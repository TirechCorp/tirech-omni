# Tirech Omni: manual

[Português](../pt-BR/manual.md) · **English**

SSH connection manager built with Tomni UI: a tree of connections in groups, the properties of
the selected connection, and terminals in tabs and in a grid (1, 2 × 2 and 3 × 3), in as many
windows as you like, with what you type sent to several terminals at once.

```sh
cargo run -p tirech-omni
```

## Navigation

The pane on the left leads to the **Home**, **Terminals**, **Password vault**, **SSH keys**,
**Agent**, **Networks**, **About** and **Settings** pages. In Terminals the pane shows only the icons, so the terminal area gets the
full width. Tabs stay open when you switch pages.

- **Home**: a hero area with the actions (New connection, Import from `~/.ssh/config`, Open local
  shell) and, below it, the **Pinned** connections, the **Recent connections** and the **Tools** (Password
  vault, SSH keys, Agent, Networks and Settings). Clicking a card opens the connection and shows Terminals; the
  right-click menu has Open, Open in ▸ (the pane, see "Opening in a pane"), Pin to Start (or
  Unpin), Show in the tree and Remove from recents.
- **Search** (title bar, Ctrl+E): finds connections by name, host or user, and vault items
  (with the vault unlocked) by title, ignoring case and accents. Choosing a
  connection opens it in Terminals; choosing a vault item shows the Vault with it selected.
- **Settings** (Ctrl+,): everything the app lets you change, on a single page (see "Settings").
- **Theme** (button next to the window controls): Use the system theme, Light or Dark, the same
  choice as Settings ▸ General ▸ Theme; changing one updates the other.

Recents are added when you open a connection (from the tree, Home or search); reopening the
tabs from the previous state does not count. Deleting a connection removes its recent entry.

## Windows and workspaces

- **Multiple windows**: Window ▸ New window (Ctrl+Alt+N) opens another window with just the terminals
  (title bar, the same menu bar as the main window, and the panes). The tree, the properties,
  the vault and the other pages stay in the main window. In the other windows, session actions
  (Reconnect, Stop logging, Close tab…) act on the window the menu belongs to; the ones that only exist in the
  main window (New connection, Import, Export, Vault, Notes, Keyboard shortcuts, About) bring the main window
  to the front, even from the tray, and act there; the pane items in the View menu are disabled. Double-clicking a connection opens the tab in the last
  window you interacted with. Dragging a tab to another window (or outside all of them, which
  opens a new window) and the tab menu's "Move to window" carry the **same session**: nothing
  reconnects and the history is kept. The Window menu lists the open windows and focuses them.
- **Connection status in the tree**: a badge on the corner of the icon shows the status of each connection
  with an open session: solid green (connected), amber ring (connecting), red with "!" (dropped) and
  gray ring (closed, with the tab still open); no session, no badge. With several sessions the
  most severe wins, and a **collapsed** folder shows the most severe one inside it (expanded, each child shows its
  own). The row tooltip has the status and the reason for the drop ("Dropped: timed out"). The
  right-click menu gains **Reconnect** (only when some session dropped; on a folder, it reopens the dropped ones
  inside) and **Go to the tab** (only on a connection with a session: selects the tab and focuses its
  window). The tab shows the ring while connecting and the red warning when it dropped.
- **Closing a window** with sessions asks whether the tabs move to the main window or close. Closing
  the main window with other windows open asks whether to close everything.
- **Workspaces**: what a window (or all of them) has open becomes a file in
  `espacos/`, with the grid, the connections in each pane and, if the system reports them, the position and
  size of the windows. Window ▸ "Save this window as a workspace…", "Save all windows as a
  workspace…" and, in the tab menu, "Save the pane as a workspace…". The file holds only **references**
  to the connections: no passwords. If a connection was deleted, the workspace opens the others and says
  which are missing. The window of an **AI cluster** saved as a workspace also keeps the assistant
  and its pane (`cluster_ia`, without the assistant's tab among the tabs): opening the workspace reopens
  the machines and launches the assistant again, continuing its conversation.
- Workspaces appear in the tree, in the fixed **Workspaces** group (always the last one), with
  folders. Double-click opens: a pane workspace goes into the active pane, window and all-windows
  workspaces each open in a new window. The right-click menu has Open, Open in a new
  window, Open in the active pane, Rename and Delete; folders have New folder, Rename and Delete;
  dragging moves the workspace to another folder. Opening a workspace adds it to the Home recents.

## What it does

- **Connections**: SSH (with key, agent and jump server), SFTP, FTP and FTPS, S3, rclone remotes, local shell and Telnet, in groups. New
  connection and new group (Ctrl+Shift+N and Ctrl+Shift+G), rename, duplicate (in the connection menu:
  the "(copy)" goes right below, with everything from the original; a vault password that only the
  original used gets its own item, and one shared by several connections stays shared), move up,
  move down, delete (with confirmation) and filter by name, user or host. The properties show only what applies to
  the connection type (the local shell has no host or port, S3 has no host, and so
  on); what depends on another field is disabled, and nothing is erased: switching the type and
  back restores what was filled in. The default **Port** follows the type (22, 23, 21, or 990
  for implicit FTPS) and is only bold when it is a different one.
- **AI**: the app talks to the model you choose (local Ollama and LM Studio, OpenRouter,
  OpenAI, Anthropic, Groq or another compatible one), registered in Settings › AI assistants.
  **Generate a command** (Ctrl+Shift+Space) turns a description into a command and puts it on the terminal
  line **without Enter**; **Explain the selection** and **Explain the error** (right-click
  menu) and the **chat** (Ctrl+Shift+A, pane on the right, one conversation per tab) answer in
  Markdown, and the button on each code block inserts it into the terminal, also without Enter. The AI never
  runs anything. Before the first send from each tab, the **What will be sent** dialog shows the exact
  text and the destination; vault passwords and common secret patterns are removed (on by
  default, but it does not catch everything), the text goes out without control sequences, and a provider outside the
  computer and the local network asks for confirmation once per profile.
- **AI assistants on the server**: on an SSH connection, **Tools › AI assistants** (also
  in the tab menu, in the connection menu in the tree and on the header button) installs and opens **Claude
  Code**, **Codex** and **OpenCode** on the remote machine, with one click. The app finds out on its own,
  without typing in the terminal, what is installed; what is missing is installed in a visible tab, without
  `sudo`. **Ctrl+Alt+A** opens the preferred assistant. Details in the "AI assistants on the
  server" section.
- **Pasting images and files into the remote terminal**: in an SSH tab, **Ctrl+V** with a
  screenshot on the clipboard, **Ctrl+Shift+V** (or Shift+Insert, the right button) with copied
  files, or dragging files and folders onto the terminal uploads everything over the tab's own connection
  and pastes the path on the server, quoted when needed, so Claude Code, Codex and
  OpenCode can attach the image. The upload goes over SFTP, or via `cat` on a server without SFTP, to
  `/tmp/tirech-colar/<AAAAMMDD-HHMMSS>-<nome>` (folder 0700; the image becomes
  `colado-AAAAMMDD-HHMMSS.png`; without permission, the app uses `~/.cache/tirech-colar` and tells you). A
  folder with many files goes in a single `tar` bundle, built by the app; without `tar` on the server,
  it goes over SFTP with several files in flight. A bar shows the progress, with **Cancel**
  (what arrived is deleted and nothing is pasted). The local shell and Telnet paste the path on this
  machine. In Settings ▸ Terminal: **When pasting an image or file in an SSH session** (Upload
  and paste the path, Paste only the local path or Ask), the folder on the server, Ctrl+V, the
  cleanup of what is older than N days (7; runs on the first upload of each session) and from how
  many files a folder goes as a single bundle. In the connection properties, **Session** category:
  **Compression** (`ssh -C`, also read from `Compression` in `~/.ssh/config`) and, under
  **Paste into the terminal**, **On paste**, **Folder on the server** and **Server shell** (for the
  quoting: Automatic, POSIX, PowerShell or cmd).
  In a tab that an assistant controls via MCP, the app uploads the same way, but does not type the
  path into the shell (it would get in the way of the assistant's next command): a notice says what was
  delivered, and the next MCP response about the tab (and `listar_abas`, in `arquivos_soltos`)
  tells the assistant. What goes as a path on this machine (in Claude Code's own tab, for
  example) becomes a valid source for `enviar_arquivo`, besides the exchange folder.
- **Downloading the file selected in the terminal**: in an SSH tab, select the name or path of a
  file on the server (something `ls` printed, for example) and **Ctrl+right-click**, as in
  WinSSHTerm; the terminal menu also has **Download selected file…** when there is a selection. The
  app asks where to save it (the dialog remembers the last folder) and copies it over the session's own
  SFTP, with a progress bar and **Cancel** (whatever started being written is deleted). An
  absolute path is used as is; `~/…` goes to the home folder; a bare name goes to the folder the
  shell is in (the same probe as "Open the current folder"; without it, `OSC 7`; with nothing, the home folder).
  Surrounding quotes and the `\ ` from `ls` are removed. A folder or a file that does not exist gives a
  warning. With nothing selected, or in a tab without files (local shell, Telnet, serial),
  Ctrl+right-click opens the menu, as always.
- **Persistent session (tmux or screen)**: the **Persistent session** row in the properties of
  an SSH connection (and of a folder, which passes the choice on to those inside it; "Inherit" says where it comes from)
  turns on `tmux` or `screen`. With tmux, the tab runs `tmux new-session -A -s tirech-<conexão>-<n>` instead of the shell, one session
  per tab: when the network drops, the session stays alive on the server, with whatever was running in it, and the
  reconnection returns to the same session. Commands on connect run only when the session is created, not on
  every return. Closing the app saves each tab's session name in `estado.json` (and a
  workspace does too), and the restored tab goes back to it. Duplicating the tab or opening a new tab of the
  same connection creates another session (`tirech-<conexão>-2`…). Without `tmux` on the server, the tab warns and
  opens the regular shell. It does not apply to "Tunnels only" or to a server marked as PowerShell or cmd.
  Mouse support is turned on only for that session (`set-option mouse on`, without touching `~/.tmux.conf`): the
  wheel scrolls tmux's history, which draws the screen; tmux's own shortcut is Ctrl+B [. To
  select text with the terminal's mouse, hold Shift.
  **Closing the tab** asks what to do with the session on the server: **End the session** (runs
  `tmux kill-session` on a new connection, with the same authentication, because the tab's connection may have
  dropped) or **Keep on the server**; Esc means keep, and **Remember my choice** saves the answer in
  `conexoes.tmux_ao_fechar` (Ask, End or Keep, also on the Settings › Connections
  page). "Close all tabs" and "Close the dropped tabs" ask only once for all of them.
  Closing the app or a window, and the MCP `fechar_aba`, keep the sessions.
  With **screen**, the tab joins the session `tirech-<conexão>-<n>` if it exists (`screen -x`,
  which shares the session with whoever is already in it) or creates one (`screen -h 10000 -S tirech-…`, with 10
  thousand lines of history); ending it runs `screen -S <sessão> -X quit`. The rest works the same: the
  reconnection, the restore, duplication and the question on close. The duplicate's folder probe reads the folder
  of the shell running inside the session (through `/proc`, on Linux).
  **Sessions on the server…**, in the SSH connection menu in the tree and in the tab menu, lists the tmux
  and screen sessions alive on the server, opened by the app or by hand, with how many terminals are
  attached to each one. **Open** attaches a new tab to the session (even if the connection does not use a persistent
  session), or goes to the tab that is already in it; **End** ends the session on the server,
  without asking, and the tab here that was in it no longer asks when closing. The list runs over the
  connection of an open tab of that connection, if there is one, and otherwise over a new connection with the
  vault password.
- **Import and export**: File ▸ **Import ▸** brings in connections from WinSSHTerm, FileZilla,
  WinSCP, PuTTY/KiTTY, mRemoteNG, MobaXterm and the app's own JSON; **Export ▸** sends them
  to those same programs and to `~/.ssh/config` ("Import and export" section, below). The
  `~/.ssh/config` import reads `Host`, `HostName`, `Port`, `User`, `IdentityFile`, `ProxyJump`,
  `ForwardX11`, `ForwardX11Trusted`, `LocalForward`, `RemoteForward`, `DynamicForward` and
  `ExitOnForwardFailure` and puts everything in a new group, reporting what it ignored. `~/.aws` does the
  same with the S3 profiles (see below).
- **Description**: the **Description** row in the properties holds free text about the connection,
  like WinSSHTerm's. It shows up in the tree row's tooltip, is matched by the filter and round-trips through
  import and export (`Descr` in WinSSHTerm and mRemoteNG, `Note` in WinSCP and `Comments`
  in FileZilla).
- **Tabs**: double-click (or Enter) on a connection opens a tab; View changes the grid; Tools
  reconnects, duplicates and records the session log. The selected tab of each pane has the background and
  text of the terminal's color scheme, and the one in the active pane gets a small bar in the accent
  color underneath.
- **Opening in a pane**: the connection's right-click menu (in the tree and on the Home card)
  has **Open in ▸** with Pane 1 through the last pane of the active window, **New pane to the right**,
  **New window** and, with more than one window, **Another window ▸**. While the submenu is open,
  each pane shows its number in the center and the one under the pointer (or with focus) is
  highlighted; keys 1 to 9 pick the pane. On a folder, **Open all** opens all of its terminal
  connections (file-only ones, such as FTP and S3, are left out), **Open all in ▸**
  puts them all in the chosen pane and **Open all controlled by AI ▸** (Claude Code, Codex or
  OpenCode, whichever are shown in Settings) opens the AI cluster directly, without the dialog. View ▸ **Identify the panes** (Ctrl+Alt+I, configurable) shows the
  numbers for 2 seconds.
- **Server key**: on the first connection, a dialog shows the fingerprint and asks you to
  trust it. A key that changed is rejected, with a warning.
- **Password vault**: Ctrl+Shift+K (or Tools ▸ Password vault) opens the vault page,
  with items in folders and subfolders; the tabs stay open in Terminals.
- **Markdown notes**: Ctrl+Shift+J (or Tools ▸ Notes) opens the Notes page: the vault's
  notes in folders, an editor with highlighting and the preview beside it or in its place (see
  "Notes" below). Each note is a vault item, encrypted with it.
- **Picks up where you left off**: the windows, the grid and the open connections come back on the next run.
- **Client networks**: Tools ▸ Networks stores network profiles (a SOCKS5 or HTTP proxy on
  `127.0.0.1`, an embedded Nebula network, a jump server and the commands to bring the
  VPN up and down), and the **Network** row in the properties ties a group or a connection to a profile (see below).
- **Commands on connect**: the **Commands** row in the properties of an SSH, Telnet
  or Local connection (and of a folder) opens an editor of steps the app types after the session opens:
  `sudo su -`, "Send the password", `cd /srv/app`. Each step waits for the screen to settle, for some text, or for nothing,
  with a timeout; it runs on open, on duplicate, on reconnect and from the tab menu. Folders
  pass the steps on (Inherit, Own, None). The default timeouts are in Settings ›
  Connections.
- **Files** (SFTP, FTP and S3): the Files page shows, side by side, the files on this machine and those on the
  server, one tab per connection (SSH, which also works for SFTP, SFTP, FTP or S3). Each pane has the
  Up, Home, Refresh and New folder buttons, the path field (with suggestions), the filter and
  "Show hidden files"; the table sorts by name, size, date and permissions, with multiple
  selection, rename (F2), delete (Delete, with confirmation), permissions (Properties) and a
  context menu. Dragging from one pane to the other uploads or downloads; dragging from the system's file
  manager uploads (on Wayland the system does not deliver the drag: use the menu). The queue
  at the bottom shows the progress and speed of each transfer, with pause, cancel, retry
  and clear. When the destination already exists, a dialog offers overwrite, resume, skip or
  rename (`nome (1).ext`), with "Do the same for the next ones". A tab from an open terminal
  uses its SSH connection, without asking for the password again; "Open terminal here" sends the `cd` to the
  session. With no tab open, the page follows the WinUI 3 Gallery, like Home: the hero strip
  with **New connection** and **rclone remotes**, and below it the cards of the connections that open
  files (name, `usuário@host:porta` and the type) and of the recent folders. With tabs, the two buttons
  sit on the bar, next to **New tab**.
- **Compare folders**: the **Compare folders** button on the tab's bar colors the rows of both
  panes, by name (case-sensitive): yellow is what exists only on that side,
  green is the newer one (**date** criterion) and red is the one with a different size (**size**
  criterion). The **Comparison options** button, next to it, opens the menu that picks the criterion
  and turns on **Hide identical**, which hides the uncolored rows and brings them back when turned off.
  The date tolerance and what new tabs open with already turned on are in Settings ▸ Files. Folder
  against folder only compares whether it exists; folder against file is "different size" under the size
  criterion and identical under the date criterion. For dates, a difference up to the tolerance (1 minute by
  default, because of FTP, which lists only down to the minute) counts as the same, and a side with no date
  leaves the pair identical. The comparison follows navigation and compares only the open folder, without
  descending into subfolders or looking at the contents.
- **Synchronized browsing**: the **Synchronized browsing** button stores the two folders you
  are in (the base). Entering, going up, Home or typing a path in one pane takes the other to the
  equivalent path from its own base, with no echo back. If the destination is outside the base, or the
  equivalent folder does not exist on the other side, synchronization turns off with a warning and the other
  pane stays where it was. Turning it on again stores a new base.
- **Default folders**: each connection has a default **local folder** (with a picker) and a default **remote folder**,
  and "Compare folders" and "Synchronized browsing" on open (Default, On or
  Off; Default follows Settings). The folder each pane opens in comes, in this
  order, from: the recent folder or the saved state, the connection's folder, the local folder in
  Settings (local pane only) and the home folder. A folder that does not exist falls back to the home folder, with a
  warning.
- **FTP and FTPS**: the **FTP** connection type opens directly on the Files page (it has no terminal).
  In the properties, **Security** has four modes: "FTP over explicit TLS" (the
  default, port 21; rejects a server that does not accept TLS), "TLS, if available" (explicit TLS that
  continues in the clear if the server does not offer it, with a warning on the tab), "Implicit FTPS" (port 990)
  and "Plain FTP (no TLS)". Changing the security switches the port between 21 and 990 if it was at the
  default. **Mode** is Passive (the default) or Active; "FTP advanced" holds the TLS version
  (Automatic, which drops from 1.3 to 1.2 on its own, with a warning, if the server does not resume the session),
  the filename encoding, the server's time zone and the active-mode port range. An empty user
  (or `anonymous`) logs in anonymously; otherwise, the password comes from the vault (Credential) or is asked for and
  saved, as with SSH. A certificate the system does not accept (self-signed, expired) opens the
  **Untrusted certificate** dialog, with the reason, the subject, the issuer, the validity and the
  SHA-256 fingerprint; "Trust and continue" stores the fingerprint in `certificados.json` (key
  `host:porta`) and the connection does not ask again as long as the certificate stays the same. A tab without
  TLS shows an open padlock. Transfers resume from where they stopped.
- **S3**: the **S3** connection type opens on the Files page, like FTP (it has no terminal), with
  Amazon and compatible services. In the properties, the **S3** category has the **Provider**
  (Amazon S3, MinIO, Cloudflare R2, Wasabi, Backblaze B2, DigitalOcean Spaces and "Ceph RGW /
  other"): choosing one fills in the **Address**, the **Region** and the **URL style** (Path or
  Virtual host), and all three stay editable; changing the region updates the address along with it as long as it
  is the provider's. On Cloudflare R2, replace `{conta}` with the account id in the address. An empty
  **Bucket** makes the root list the account's buckets, each one becoming a folder; with a bucket, the
  root is the bucket's. "S3 advanced" has the storage class for uploads and how the
  certificate is checked (by the system, by a custom CA or not at all, for a lab
  MinIO). Folders are prefixes: New folder creates the `pasta/` marker and deleting a folder
  deletes everything in it. Large uploads go in parts and resume from where they stopped.
  There are no permissions, symbolic links or owner, and the server does not allow changing the date: the file's
  date goes along with the upload.

  Context menu of a file in the bucket: **Copy presigned link** (valid for 1 hour, 1 day or
  7 days; the link goes to the clipboard and a bar says until when it is valid),
  **Storage class** (changes the object's class) and **Object properties**
  (size, class, type, ETag, date and metadata). **Incomplete uploads…**, on the tab's bar, lists
  the multipart uploads that did not finish in the current folder (which the provider charges for) and aborts one or
  all of them. These actions open their own connection, in a thread, and the bar shows the error if they fail.
- **rclone (all the other clouds)**: the Files page has the **rclone remotes** subpage, which
  lists the remotes in `rclone.conf` (Google Drive, OneDrive, Dropbox, Box, Azure Blob, Google
  Cloud Storage, WebDAV, SMB, Mega… more than 70 in all) and opens any of them in the same screen as the
  others. Buttons: **Open**, **New**, **Edit**, **Delete**, **Add to the tree** (creates an
  rclone connection in the chosen group, which also opens from the tree and goes into the recents),
  **Refresh** and **Options** (the path of the binary and of `rclone.conf`). **New** builds the
  form with the options rclone itself describes for the provider (the basic ones, and the
  advanced ones through the "Show advanced options" box), and those that only apply to one `provider`
  disappear when another is chosen. Saving opens the "rclone needs to know" dialog for each question
  in the configuration; with OAuth (Drive, OneDrive, Dropbox…), "Yes" opens the browser and shows
  "Finish signing in in the browser…" until it is done, and "No" asks for the code from
  `rclone authorize` (for a machine without a browser). Cancelling midway deletes the unfinished
  remote. The rclone connection in the tree has the **Remote** (`gdrive`) and **Path**
  (`Backup/2026`) rows. Copies between the disk and the cloud, and between two clouds, are done by
  rclone itself, with progress and cancellation. Files have a public link when the
  provider supports it.
- **Editing a remote file**: opening a file on the server (double-click, Enter or "Open" in the
  menu) downloads it to a temporary folder and opens it in the system's program. When the file changes, the
  page asks "Upload it?" (or "Always upload this file"). The temporary folder goes away when
  the tab closes.

## Tools for multiple servers

The **Tools** menu gathers what helps people who look after many servers. Each tool has
a menu item and, where it makes sense, a shortcut that can be changed in Settings › Shortcuts.

- **Send to several** (Ctrl+Shift+I): opens a bar at the bottom of the window with the text, "Enter"
  and the "Keys" button (Ctrl+C, Ctrl+D, Ctrl+Z, Tab, Esc and Enter). The "N tabs…" button picks the
  tabs that receive it; a dropped tab is left out, and the history of what was already sent becomes
  suggestions. Next to None, Visible and All, the **Selected** mode sends what you type
  in a terminal to the marked tabs ("Receive the multi-send" in the tab menu, or the bar's dialog); the
  marked ones show the broadcast icon.
- **Cluster mode** (context menu of a group or a connection › "Open in cluster mode…",
  or Tools › Cluster mode › "Cluster…"): a dialog with the terminal connections, already checked according to the
  group or connection in the tree. "Open" creates a new window with the connections in a grid (up to
  4 × 4; anything beyond that becomes tabs) and sending set to **All**: what you type in one terminal goes to
  all of them.
- **AI cluster** (context menu › "Open in AI cluster…", or Tools › Cluster
  mode › "AI cluster…"; on a folder, "Open all controlled by AI ▸" skips the dialog and
  opens all of its terminal connections with the chosen assistant): the same dialog, plus the choice of **assistant** (Claude Code,
  Codex or OpenCode, whichever are shown in Settings). "Open" creates a window with the
  machines side by side and, in the next pane, **a single** assistant on this computer that controls
  all of them (see "AI cluster" in the section on control by assistants). There is no send-to-all: what
  you type goes only to the active tab. It does not depend on a workspace; to keep it,
  save the window as a workspace.
- **Scripts** (context menu › "Run script…", Tools › "Run script…" or the Home
  card): the Scripts page runs a file from the scripts folder, or a typed line, on
  several SSH connections at the same time, over the `exec` channel and without opening tabs. The folder is
  `scripts` in the app's data folder (or the one in `scripts.pasta`); "New…" creates the file and
  opens it in the system editor. The file is fed to the interpreter's input (`scripts.interpretador`,
  `sh -s` by default; change it to `bash -s`, for example); the typed line goes as is. Per
  server, the grid shows the status, the exit code, the duration and the first line of the
  output; selecting the row puts the full output, with colors, in the page's terminal, and there are
  "Copy the output" and "Save the results…". `scripts.paralelo` limits how many run at the
  same time (8 by default); "Stop" prevents the ones still waiting. For connections without a password
  in the vault, the page asks for a shared password (optional, with "Save in the vault").
- **WSL** (Tools › WSL, on Windows): lists the distributions (`wsl.exe -l -q`), opens each
  one in a tab (like the local shell, without adding it to the recents) and, with "Save as connections",
  saves them in the "WSL" group, without duplicates. Each one is a **Local** connection with program
  `wsl.exe` and arguments `-d <distribuição> --cd ~`. Every Local connection has the
  **Program** and **Arguments** rows in its properties: left empty, it opens the usual shell.
- **Dropped tabs**: the tab of a closed session shows the warning icon, and its terminal gets
  a fixed, hollow cursor. "Reconnect the dropped tabs" (Ctrl+Shift+R), "Close the dropped
  tabs" and "Close all tabs" are in the Tools menu (all windows) and in the tab
  menu (its own window). "Close all tabs" only asks when there is a live session, with "Don't ask
  again" (`conexoes.confirmar_fechar_todas`).
- **Tab tooltip**: resting the pointer on a tab shows the title, the connection, the destination (type,
  user, host and port), the jump, the network and the status ("Connected", "Dropped: <reason>" or
  "Closed").
- **Duplicate tab** (tab menu, Tools menu and Alt+Shift+D): the copy opens in the folder
  the original is in now. In the local shell, the process starts in it directly (Linux: through `/proc`; macOS and
  Windows: through OSC 7; on WSL it goes in `--cd`). Over SSH, a probe of up to 2 s in an `exec` session on the
  same connection finds the shell's folder without configuring anything on the server, with OSC 7 as a fallback, and
  the app types ` cd '<pasta>'` after the prompt and the commands on connect. With no folder found, the
  copy opens as before. The copy also inherits the network and the font zoom. In a tab with a
  persistent session, the probe asks tmux for the folder (`#{pane_current_path}`), or `/proc` for
  screen, and the copy gets another session. With a `tmux` or `screen` started by hand, the foreground program is their
  client, and the folder is the one it was started from.
  `terminal.duplicar_na_pasta` (on) turns this off.
- **Cursor and empty pane**: `terminal.cursor_piscando` (on) takes precedence over what the
  program requests; a pane that loses its last tab shows "Close pane", and
  `terminal.fechar_painel_vazio` closes it automatically.
- **Reveal the tab's connection in the tree** (View menu, off by default, saved in
  `conexoes.mostrar_aba_na_arvore`): selecting a tab, in any window, selects its
  connection in the tree, expands the folders around it and scrolls to the row, without taking focus from the terminal,
  like WinSSHTerm's "sync the tree". Clicking the tab that is already active also reveals the
  connection again.

| Tool | Where it is |
|---|---|
| Send to several | Tools › Send to several…, Ctrl+Shift+I, "Selected" mode in View |
| Cluster mode | Tools › Cluster mode › Cluster…, context menu of groups and connections |
| AI cluster | Tools › Cluster mode › AI cluster…, context menu of groups and connections |
| Scripts | Tools › Run script…, context menu, Home card |
| WSL | Tools › WSL |
| Dropped tabs | Tools and tab menu, Ctrl+Shift+R to reconnect |
| Tab tooltip | pointer over the tab |
| Duplicate tab | tab menu, Tools and Alt+Shift+D |
| Cursor and empty pane | Settings › Terminal |
| Key conversion | SSH keys page |

## Passwords

The app stores passwords. An SSH connection without a key asks for the password in a dialog with
"Save the password" (checked by default): the password goes into the vault and the next connection opens without a
dialog. If the server rejects the password, it is asked for again and the item is updated. The connection
(and its jump) points to a vault item through the "Credential" row in the properties.

On Telnet the "Credential" row and **Password** also exist: the password is sent by the "Send the
password" step of the commands on connect, with the "Connection password" source. That source is only offered in the commands editor
when the connection type has a Credential (on a folder, always).

With no vault item, the "Credential" row of an SSH or SFTP connection chooses when the password is
asked for (the vault item, when there is one, wins):

- **Ask when connecting** (the default): the password dialog comes before connecting.
- **SSH agent:** never asks. Logs in through the agent and the keys; if the server
  rejects them, the error stays on the tab, with no dialog.
- **SSH agent, then password:** connects directly through the agent and the keys; if the server
  rejects them, the password dialog opens ("The server accepted neither the SSH agent nor the keys.") and the
  connection is retried with the typed password.

The jump server does not have these modes: its password is never asked for before connecting. It
tries the agent and the keys (and the vault item, if any), and a rejection only opens the jump's
password dialog when the connection is watched (with a vault secret or in "SSH agent, then
password" mode).

The vault (Tools ▸ Password vault, Ctrl+Shift+K) also stores any other password:
items of type Password, SSH, SFTP, FTP, S3, rclone, Key passphrase or Note, with user, URL,
notes and extra fields. The page has three panes, like File Explorer: the folders on the
left (the root is "Vault"), the list of the open folder in the middle and the item's details on the right.

- **Folders and subfolders:** New folder (Ctrl+Shift+N), rename (F2), drag from the list onto a
  folder in the list or the tree, drag folders within the tree, "Move to…" and delete. Deleting
  a folder takes its subfolders and items with it, and the dialog counts what goes. A folder cannot go
  inside itself or where there is already a sibling folder with the same name. Double-click (or Enter)
  opens the folder, or copies the item's password; Backspace goes up one level.
- **Filter:** searches the open folder and its subfolders, by title, user, address, notes and
  folder name, ignoring accents and case, and shows the path of each result.
- **Import** (into the open folder, merging folders with the same name): Tirech Omni JSON, Bitwarden
  JSON (the unencrypted one), Bitwarden CSV and KeePassXC CSV. Nested folders
  become subfolders.
- **Export:** Tirech Omni JSON (complete) and Bitwarden JSON, always with the passwords.
- Password generator, copy user and password, master password (set, change, remove) and
  lock. The global search in the title bar finds the items, shows the folder and opens the item.

### Notes

The Notes page (Ctrl+Shift+J) shows the vault's notes (items of type Note) in the folder
tree, with the Markdown editor and the preview (Edit, Split or Read). It saves automatically,
has a formatting bar, search (Ctrl+F, F3) and replace (Ctrl+H), and uses the vault's master password and
lock. A ` ```segredo ` block is masked in the preview; "Copy" and Ctrl+C
clear the clipboard after the Vault's timeout. Notes hold secrets and loose
text (API keys, system logins) and are not tied to connections: a connection has the
**Description** row in its properties. An old file, from when a note was tied to a connection, moves its
text to the description when the vault is opened, and the note leaves the vault (one with attachments stays).

- **Attachments:** pasting an image (Ctrl+V), dragging files or "Attach file…" attaches the file
  to the note, with no size limit. Each attachment is encrypted in 1 MiB blocks (AES-256-GCM, with its own
  key) in the `vault/` folder next to `vault.json`; the note keeps the `anexo:<id>` link, and the
  image appears in the preview. Deleting the attachment deletes the encrypted file.
- **Markdown:** headings, emphasis, lists and tasks, quotes, tables, code with highlighting, links
  (`https:` opens in the browser) and images.
- **Import and export:** `.md` per note. Tirech Omni JSON exports the attachments in the clear in a
  `<nome sem .json>-attachments` folder next to the file (the JSON is written after the
  files); on import, the app checks the SHA-256 and encrypts them again with new ids and keys. Bitwarden
  JSON does not carry attachments.

`vault.json` is encrypted with AES-256-GCM. Without a master password (the default) the key lives in the
`vault.key` file in the configuration folder and the vault opens on its own; with the optional master password (Argon2id),
the app asks for it at startup. Protecting the passwords is the user's responsibility: tirech-omni
encrypts what it writes and offers the master password, but does not put up barriers (no automatic locking,
no extra confirmations, no requirement for a master password).

### Taking the vault to another machine

Nothing in the vault depends on the machine: copying its folder (`vault.json`, `vault.key` and the
`vault/` folder with the note attachments) to
another computer and opening the app there works, with the same passwords and keys. With a master password,
`vault.json` alone is enough (the password opens it). For a single file:

- **Take the vault to another machine…** (vault ▸ More, or Settings ▸ Password vault)
  saves a `.tirech-vault` (and, if there are attachments, the `<nome>.vault/` folder next to it; the copy
  shows its progress and can be cancelled). Without a password for the file, the key goes inside it (whoever has the
  file opens the vault); with a password of its own, it is asked for on open. A vault with a master password
  takes the file protected by it and must be unlocked.
- **Open vault from another machine…** accepts the `.tirech-vault` or a `vault.json` (with
  `vault.key` next to it, if it has no master password), with the `<nome>.vault/` attachments folder, if
  there is one; without it, the app counts how many attachments did not come along. **Merge with this vault** brings in what the other one
  has that is new, without losing anything here; **Use instead of this one** replaces the vault and keeps the current one
  alongside as `vault.json.bak-<data e hora>` (and `vault.key`).
- **Vault location** (Settings ▸ Password vault) separates the vault from the data folder: you can
  keep it in a synced folder (Syncthing, Dropbox, OneDrive…). When you change it, the app moves the
  vault there or uses whatever is already in the new folder; "Back to the data folder" undoes it.
- **Two machines, one synced folder:** each item and each folder has a `uid`, and deleting leaves
  a tombstone. Before writing, the app merges what changed on disk; it checks the file every 5
  seconds and on gaining focus, and tells you when something came in from another machine. If the sync
  creates a conflicted copy (`vault.sync-conflict-….json` from Syncthing, `vault (conflicted
  copy …).json` from Dropbox…), the app offers to merge it and archives the copy.
- `vault.json` is written with 0600 permissions on Linux and macOS.

## Import and export

File ▸ **Import ▸** and File ▸ **Export ▸** (ids `importar-<formato>` and `exportar-<alvo>`).

### Import

Choose the program, choose the file (the picker already opens where it usually lives) and the app shows
a **preview**: the tree that will come in, the count ("3 connections in 2 groups, 3
passwords") and warnings about what was left out. There you change the **destination group** (by default,
"Imported from X"), decide whether the **passwords** go into the vault and choose what to do with connections
that already exist (same name, type, host, port and user): **skip**, **replace** or **keep
both**. Nothing is written before "Import".

| Program | File | What comes in |
|---|---|---|
| WinSSHTerm | `connections.xml` (`%APPDATA%\WinSSHTerm`) | folders, host, port, user, key, X11. **No passwords**: WinSSHTerm encrypts them in the `key` file, and the import warns about it |
| FileZilla | `sitemanager.xml` (`~/.config/filezilla`, `%APPDATA%\FileZilla`) | folders, FTP/FTPS/SFTP/S3, passive mode, encoding, key, default local and remote folder, "Compare folders" and "Synchronized browsing" turned on; the password in `base64`. A password with `encoding="crypt"` (with a FileZilla master password) does not come in, with a warning |
| WinSCP | `WinSCP.ini` | folders, SFTP/SCP (SCP becomes SFTP, with a warning)/FTP/S3, key, default local and remote folder, X11, tunnels, proxy, jump; the password is recovered |
| PuTTY / KiTTY | `.reg` exported from `regedit` (UTF-16 too) or the `Sessions` folder of portable KiTTY (`~/.putty/sessions`) | SSH and Telnet, key, X11, tunnels, proxy; a name with `/` becomes a folder. **No passwords**: PuTTY does not store them |
| mRemoteNG | `confCons.xml` | folders, SSH and Telnet, passwords (the default master password is `mR3m`; if the file uses another one, the app asks). `FullFileEncryption="True"` is refused with a warning |
| MobaXterm | `MobaXterm.ini` | folders and SSH sessions. **No passwords**: MobaXterm keeps them in the Windows registry |
| Tirech Omni | JSON exported by the app itself | everything (reimports the tree as is) |
| OpenSSH | `~/.ssh/config` | see "What it does" |
| AWS | `~/.aws` | see "S3: where the credentials come from" |

The last two keep the old, direct flow, with no preview. Private key, proxy (becomes a
network profile) and jump come in when the source program has them. In Settings ▸
Connections, **Import passwords by default** makes the passwords box start checked. With the vault
locked the box is disabled and passwords do not come in (the preview says so). A
connection with a protocol the app does not open (WebDAV, RDP, VNC…) is left out, with a warning.

### Export

Choose the program; the dialog has **Include the passwords** and **Include the vault keys**, with no
extra confirmation. Formats that do not store passwords (`~/.ssh/config`, PuTTY, WinSSHTerm,
MobaXterm) disable the first box, and so does a locked vault.

| Target | File | Passwords | Keys |
|---|---|---|---|
| Tirech Omni (JSON) | `tirech-omni-conexoes.json` | yes (the `cofre` block) | inside the `cofre` block, with the passwords |
| `~/.ssh/config` | `config` | no | OpenSSH |
| PuTTY / KiTTY | `tirech-omni.reg` | no | `.ppk` |
| FileZilla | `sitemanager.xml` | yes | `.ppk` (SFTP only) |
| WinSCP | `WinSCP.ini` | yes | `.ppk` |
| WinSSHTerm | `connections.xml` | no | `.ppk` |
| mRemoteNG | `confCons.xml` (default master password `mR3m`) | yes | no |
| MobaXterm | `tirech-omni.mxtsessions` | no | OpenSSH |

The vault keys the connections use go out to a folder next to the file, `<nome sem extensão>-chaves/`
(PPK or OpenSSH, depending on the program), **without a passphrase**, and the exported file points to
them. Delete the folder after moving the keys. Tirech Omni JSON also carries the note
attachments, in the `<nome sem extensão>-attachments/` folder. Connections of a type the program does not know
(for example, S3 in PuTTY) are left out, and the final dialog counts what went out and what did not.

How to open the file in the target program (check the menu name in the version you use):

- **PuTTY / KiTTY:** double-click the `.reg` (Windows asks whether it may merge it into the registry).
- **FileZilla:** File ▸ Import… and choose `sitemanager.xml`.
- **WinSCP:** Tools ▸ Import/Restore Configuration…, or `WinSCP.exe /ini=<arquivo>`.
- **WinSSHTerm:** close the program, back up the `connections.xml` in its configuration
  folder and replace it with the exported one.
- **mRemoteNG:** File ▸ Import ▸ Import from File… (the passwords open with `mR3m`).
- **MobaXterm:** right-click "User sessions" ▸ Import sessions from file.
- **OpenSSH:** copy the contents into your `~/.ssh/config`.

### What it does not do

Remmina, SecureCRT, Royal TS, Xshell, SuperPuTTY, MTPuTTY, PuTTY Connection Manager and Tabby; reading
the Windows registry directly (export a `.reg`); picking items in the preview (everything comes in, and
you delete what you do not want); mRemoteNG's `FullFileEncryption`; WinSSHTerm's `.ppk` key.
The jump does not go out to FileZilla, WinSSHTerm, PuTTY, MobaXterm and mRemoteNG, which do not have the concept per connection; the
export warnings say when that happens. The note attachments in Tirech Omni JSON come in
all at once, with no progress dialog.

## Keys and agent

tirech-omni generates keys, stores them in the vault and has its own agent, with no external program.
What you do with your keys is up to whoever uses the app: it doesn't insist or
block (an empty passphrase, a key without confirmation and a password saved in the vault are all allowed). The
only things missing from key generation are DSA and RSA smaller than 3072 bits.

### One key, one place

The private key lives **in the vault** (an "SSH key" item, with no passphrase of its own, with the
original file kept for exporting). Connections, the agent and `~/.ssh/config` use it by
reference: there is no copy to maintain. The **SSH keys** page has, at the top, Generate, **Import…**
(files in any format; pick several at once and they come in a list, with a shared
destination and the typed passphrase tried on the other keys; the picker reopens in the last folder used),
**Paste…**, **Convert file…** (changes the format without storing
anything) and **Import all from ~/.ssh…** (imports the keys in that folder; deleting the files there afterwards is
optional and asks for confirmation). Each key has **Export as…** and the usual actions (Copy the
public key, Load into the agent, Copy to the server…). The passphrase of a key in a file is
found by its fingerprint, not by its path.

- **Generating a key:** the **New key** form has a **Destination** row, with the same pair of
  options as importing. **Vault (recommended)**, the default, writes no file at all: the key
  is created without a passphrase (the vault already encrypts it) and becomes an "SSH key" item with the **Name in the vault**
  you give it (initially, the default file name for the type, such as `id_ed25519`); protection,
  passphrase and file are disabled, and a locked vault refuses before generating.
  "Load into the agent" puts it in the agent with the vault as its origin, and the result (fingerprint,
  randomart and public key) offers Copy, Copy to the server and "Use in connection", which links the
  chosen connection to the item's "Vault key". **File in ~/.ssh (OpenSSH)** is the previous behavior:
  it writes `id_ed25519` and `id_ed25519.pub`, with a passphrase if you give one (and stores it in the vault
  if you leave that checked).
- **Private key formats:** OpenSSH, PKCS#8 (encrypted and unencrypted), traditional PEM (PKCS#1 and SEC1;
  written only unencrypted and not for Ed25519) and PuTTY `.ppk` v2 and v3.
- **Public key formats:** OpenSSH line, RFC 4716, SPKI (PEM) and PKCS#1 (PEM).
- **FIDO (`sk-`)** keys are stored and exported in OpenSSH format, but don't go into the agent. Certificates
  (`-cert.pub`) stay with the key and go into the agent along with it.
- Exporting the private key without a passphrase doesn't ask for confirmation.

### The agent

- **Built-in agent**, compatible with OpenSSH's `ssh-agent`. It loads the keys marked "In
  the agent" when the vault opens (and removes them on lock, if that option is on). The page
  lists the keys, with the origin of each one (vault, file or "Added by another program")
  and its lifetime ("expires in 5 min"), and has Add file…, Remove, Remove all, Lock and
  Unlock. File keys added through the page come back on startup (`agente.json`).
- **Stable address:** `$XDG_RUNTIME_DIR/tirech-omni/agent.sock` on Linux,
  `~/.ssh/tirech-omni-agent.sock` on macOS (and on Linux without `XDG_RUNTIME_DIR`) and the pipe
  `\\.\pipe\openssh-ssh-agent` on Windows.
- **Confirm use** of a key (a per-key option, off by default): the dialog shows
  who is asking ("codex ← git ← ssh") and can remember the answer for 15 minutes. Without the option, the
  agent answers without asking.

### Using the app's keys in the system ssh

The terminal, `git`, VS Code, Codex and Claude Code use the vault keys **without exporting
anything**, as long as the app is open:

- **Linux and macOS:** the **Use in the system ssh** button (Agent page, or Settings ▸
  SSH keys and agent) writes this block at the **top** of `~/.ssh/config` (with a backup
  next to it) and **Turn off** removes it. With the app closed, the socket disappears, the `Match exec` doesn't match and
  `ssh` goes back to what it was, with no error:

  ```
  # >>> tirech-omni: agente (gerado pelo Tirech Omni; "Desligar" na página Agente remove) >>>
  Match exec "test -S '/run/user/1000/tirech-omni/agent.sock'"
      IdentityAgent "/run/user/1000/tirech-omni/agent.sock"
  Match all
  # <<< tirech-omni: agente <<<
  ```

  "Also in shells" puts `SSH_AUTH_SOCK` in `.bashrc`, `.zshrc` and fish; "Also in the
  graphical session" notifies `systemctl --user` or `launchctl`.
- **Windows:** the agent uses the OpenSSH pipe, and the system's `ssh.exe` finds it with no configuration.
  If the "OpenSSH Authentication Agent" service already holds it, the page explains and **Copy the
  command** copies the command that stops the service. Git for Windows ships its own `ssh` that doesn't talk
  to that pipe: the "Make Git use Windows OpenSSH" switch points
  `core.sshCommand` at the system's `ssh.exe`. In **WSL** nothing is done: use Windows'
  `ssh.exe` from inside it.

### PuTTY, WinSCP and FileZilla (Pageant, Windows only)

With "Answer as Pageant" on (the default), the agent also speaks as Pageant, through the pipe
`\\.\pipe\pageant.<user>.<hash>` and through the hidden "Pageant" window: PuTTY, plink, WinSCP,
FileZilla, KiTTY and TortoiseGit use the vault keys without exporting a `.ppk`. If PuTTY's
`pageant.exe` is already open, the app warns you and doesn't compete: close Pageant and use
**Try again**.

### Tray

Closing the main window asks "Close Tirech Omni?": **Keep in the tray** (the agent,
Pageant and the sessions keep running), **Close** or Cancel, with "Don't ask again"; the choice
lives in Settings ▸ General ▸ "When the window is closed". The tray icon has "Open Tirech
Omni", "Lock the vault", "Agent" (turn on and off, with the number of keys) and "Exit" (which doesn't
ask). An agent confirmation request brings the window back and hides it afterwards. On
GNOME you need the AppIndicator extension; with no system tray there is no icon, closing quits the
app, and it tells you once. "Start with the system, minimized to the tray" opens the app hidden at
login (`~/.config/autostart/tirech-omni.desktop` on Linux, a LaunchAgent on macOS, the `Run` key
on Windows; the argument is `--bandeja`).

### Install on this computer

File ▸ **Install on this computer…** (and the card in Settings ▸ General) opens a dialog in
which you check what the app leaves on this computer. Each item is applied separately: if one fails, the
others go ahead, and the result appears in a notification bar.

- **Copy to this computer**: `%LOCALAPPDATA%\Programs\Tirech Omni` on Windows (with the
  `tirech-omni-*` tools), `~/.local/bin` on Linux and `~/Applications` on macOS. The copy
  becomes the target of everything below; an outdated copy (size, date and SHA-256
  different from the running app) is replaced.
- **Desktop shortcut** and the menu shortcut (**Start menu shortcut**, **Applications menu shortcut**).
- **Pin to the taskbar**: on Cinnamon, GNOME and KDE the app does it by itself; on Windows and in the Dock the
  system only lets the user pin, and the app shows the step.
- **Open with the system** (the same `Run` key as the tray, with `--bandeja`) and, on Windows,
  **Show in Installed apps** (`HKCU\...\Uninstall\TirechOmni`, whose
  Uninstall button opens the app with `--desinstalar`).

The dialog shows what is already installed, and the **Uninstall…** button turns into the same dialog with
everything unchecked: whatever stays unchecked is removed. The "Also delete my data" box starts
unchecked and is the only one that asks for confirmation, because it can't be undone. On startup, the app repairs what
has moved (the shortcuts point at the installed copy when it exists). What the user
asked for is stored in `instalacao` in `configuracoes.json` (it isn't included when exporting settings).

On Windows the app doesn't open a console window, not even for child processes (`sistema::sem_janela`),
and a window that doesn't fit on the monitor is shrunk and centered by the lib.

### Portable version

The portable package (`tirech-omni-portable-windows.zip`, built by `empacotar-portable.sh` at the root
of the repository) contains the `Tirech Omni` folder with the five executables and a
`tirech-omnidata` folder next to them. **The folder is the marker:** with `tirech-omnidata` next to the
executable, the app writes everything into it and doesn't touch `%APPDATA%` or `~/.config`. This works on
any system; without the folder, nothing changes. The order for the data folder is: the
`TIRECH_OMNI_CONFIG` variable, the portable folder, `pasta.txt`, the default folder.

In portable mode:

- nothing is written to the system: no migration from the old name, no re-pointing shortcuts, no menu
  shortcut and no start with the system. "Open with the system", "Applications menu shortcut",
  "Install on this computer…", "Change the data folder" and the agent's system integrations
  disappear from the screen (the Agent page shows the agent's address for anyone who wants to point `SSH_AUTH_SOCK` at it by hand);
- `known_hosts` lives in `tirech-omnidata/known_hosts`, the assistants' working folder in
  `assistentes`, the MCP exchange folder in `troca` and the copies used for editing remote files
  in `temp`. A path chosen in Settings keeps working as written;
- the agent and MCP use addresses specific to the folder (on Windows,
  `\\.\pipe\tirech-omni-agent-<h>` and `\\.\pipe\tirech-omni-mcp-<user>-<h>`, with `h` being the
  start of the SHA-256 of the path), so they don't collide with an installed copy open at the same time;
- only one copy opens per folder: the second one finds `trava.lock` held and says the app is already
  open with that folder;
- the data folder card says the app is in portable mode and, without a master password, suggests
  creating one: whoever has the folder can open the vault.


### ARM packages

`empacotar-arm.sh`, at the root of the repository, builds on an x86_64 computer through Docker (the
recipes are in `empacotar/`) and leaves in `target/` (or in `$SAIDA`):

| Package | For |
|---|---|
| `tirech-omni-linux-arm64.tar.gz` | 64-bit ARM Linux (Raspberry Pi 3/4/5 with a 64-bit OS, ARM servers) |
| `tirech-omni-linux-armv7.tar.gz` | 32-bit ARM Linux (Raspberry Pi 2/3/4 with a 32-bit OS) |
| `tirech-omni-portable-windows-arm64.zip` | Windows 11 ARM, in the same format as the portable version, with `libunwind.dll` next to the `.exe` files |

The Linux ones require glibc 2.35 or newer (the container is Ubuntu 22.04). `./empacotar-arm.sh
linux-arm64` builds a single target. The build goes into `target/docker`, separate from the computer's own build.

### Copying the key to the server

**Copy to the server…** (Tools, or from the keys) is the equivalent of `ssh-copy-id`.
It picks the SSH connection and the public key, uses the password from the vault (or the one typed in the dialog),
installs into `authorized_keys` without duplicating ("Force" installs anyway) and, if the connection doesn't
have a key yet, offers to start using the copied one. On Windows servers it uses the
administrators' file.

The same key features exist on the command line in the `crates/tirech-omni-tools` package
(`tirech-omni-keygen`, `tirech-omni-agent`, `tirech-omni-add` and `tirech-omni-copy-id`).

## AI assistants on the server

Claude Code, Codex and OpenCode run **on the remote machine**, where the code and the tools are.
This is different from "Assistants controlling the app" (an assistant on your computer controls the
app) and from the AI inside the app (the app talks to a model): here the app only installs and opens the three on the server, in regular terminal tabs.

### The submenu

**Tools › AI assistants** only appears when the active tab (or the connection clicked in the tree)
is SSH. Each tool has an item whose text tells its state: **Open Claude Code** (installed),
**Install Claude Code…** (missing), **Claude Code (checking…)** and **Claude Code (could not
check)**. Further down: **Open in another folder…**, **Sign in** (each tool's login, only for the
ones that are installed) and **Check again**. The sparkle button in the header and **Ctrl+Alt+A**
(the preferred assistant, in Settings) do the same.

Detection runs through `exec` (a script sent over the stdin of `sh -s`, without typing anything in your tab)
and caches the result for 10 minutes per connection: system, architecture, the login shell's `PATH`,
`node`, `npm`, `curl`, `tar`, and each tool's path, version and login. If the server is
Windows (`sh` doesn't exist, or the connection's **Server shell** is PowerShell or Cmd), the same probe
runs in PowerShell, and so do the open and install commands.

### Installing

The dialog shows the method (the official installer, npm or Homebrew; automatic picks the best one
for that server), the **exact command** that will run and what is missing (no `curl`, no `npm`,
old Node, an architecture with no binary). The warnings don't block: you pick the method and go on. The
command runs in a new tab, without `sudo`; if `npm` can't write to the global folder, the app uses
`--prefix ~/.local`. At the end the app checks again and, with **Open when it finishes** on, opens
it by itself; with it off, it shows the notice with **Open now**.

| Tool | Official installer (Unix) | Windows | Checked |
|---|---|---|---|
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` | `irm https://claude.ai/install.ps1 \| iex` | 2026-10-19 |
| Codex | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh` | `irm https://chatgpt.com/codex/install.ps1 \| iex` | 2026-10-19 |
| OpenCode | `curl -fsSL https://opencode.ai/install \| bash` | npm only | 2026-10-19 |

npm (`npm install -g @anthropic-ai/claude-code`, `@openai/codex`, `opencode-ai`) and Homebrew
(`brew install --cask claude-code`, `--cask codex`, `anomalyco/tap/opencode`) are the alternatives.
The catalog, with links to the documentation, lives in `src/ia.rs`: when the commands change, that's
where to update them (and the date).

### Opening

In a new tab (the default), in a free pane of the grid or **in this tab** (it types the command into the terminal
that is already open). The folder comes, in this order, from the one the terminal reports (OSC 7, below), from the
connection's **Folder** (the **AI assistants** category in Properties, which **Open in another
folder…** also saves, with the "Remember for this connection" box), from the connection's default remote folder
or from home. A folder that doesn't exist on the server falls back to home. **Open in another folder…**
asks for the tool and the folder. With **Stay in a shell when the assistant ends** (on), the tab
stays in a shell when the tool exits.

For the app to know which folder the terminal is in, the shell has to report it through OSC 7. In bash:

```sh
PROMPT_COMMAND='printf "\033]7;file://%s%s\033\\" "$HOSTNAME" "$PWD"'${PROMPT_COMMAND:+";$PROMPT_COMMAND"}
```

Without that report, the app uses the remembered folder or the default remote folder.

### Signing in without a browser on the server

| Tool | How |
|---|---|
| Claude Code | `claude` shows the URL: **Ctrl+click** opens it in your computer's browser; the browser shows a code, which you paste into the terminal (Ctrl+Shift+V). **Sign in** runs `claude setup-token` (a one-year token, for `CLAUDE_CODE_OAUTH_TOKEN`). |
| Codex | Without a login, the tab opens with a tunnel for port **1455** (`ia.tunel_codex`, on): the URL opens with Ctrl+click and the callback to `localhost:1455` goes through the tunnel. If the port is already in use here, the app warns you and opens without the tunnel. **Sign in** runs `codex login --device-auth` (the ChatGPT account has to allow it). |
| OpenCode | **Sign in** runs `opencode auth login`; in the `opencode` screen, `/connect`. |

A program on the server can also put text on your clipboard through OSC 52 (`claude`'s `c`
copies the URL), always write-only: the terminal never reads the clipboard.

### Settings

**Settings › Assistants on the server**: the Ctrl+Alt+A assistant, where to open, staying
in a shell on exit, the Codex tunnel, checking when an SSH tab opens, the header button, which
tools to show, the installation method and each tool's arguments (`--model sonnet`, for
example). Per connection (the **AI assistants** category in Properties): folder, method and
arguments, which override the global ones.

### What it doesn't do yet

- It doesn't update or uninstall the tools, doesn't install Node or system packages, and never
  uses `sudo`.
- SSH connections only (not Telnet, serial, local terminal, FTP, S3 or rclone).
- On macOS, Claude Code's login lives in the Keychain: the app can't tell whether it's logged in (and doesn't block
  anything because of that).
- The automated tests use a fake server; actually installing and opening on Debian,
  Alpine, macOS and Windows is the manual test plan recorded in the plan.

## Assistants controlling the app (MCP)

Coding assistants that run on your computer (Claude Code, Codex and any MCP
client) can operate the app: list and open connections, read a tab's screen, type, send keys,
wait for some text, run commands, work with files and turn on tunnels. The app opens the connection with the
saved password (the assistant never sees the password) and you keep seeing everything in the tabs. This is
different from the AI inside the app: there the app talks to a model and the AI only suggests; here an outside
assistant commands the app, and you decide how much it can do, connection by connection.

### What it is

The app's executable has a subcommand, `tirech-omni mcp`, which is the **bridge**: the assistant runs it
as an MCP server over `stdio`, and it forwards each call to the open app through a local channel (a
Unix socket with 0600 in a 0700 folder, or a Windows pipe `\\.\pipe\tirech-omni-mcp-<user>`,
which only your user can open). The bridge decides nothing: every permission, approval and redaction happens
inside the app. With the app closed, or with control turned off, the bridge says Tirech Omni isn't
open and tries again on the next call.

### How to turn it on

1. **Settings › AI assistants › Let assistants control the app**, or the
   **Tools › AI assistants › Turn on control by assistants** item. Control comes
   **off**; the first time, it shows a notice explaining what the assistant will be able to do.
   When off, the app doesn't even open the socket.
2. Choose the permission for connections and folders (below). Everything starts as **Off**: an
   assistant sees no connection until you allow it.
3. Open **How to connect an assistant…** (a button right there, or the Tools menu) and copy the command for
   your client, with the path of the running executable:

   ```sh
   claude mcp add tirech-omni -- /caminho/para/tirech-omni mcp
   codex mcp add tirech-omni -- /caminho/para/tirech-omni mcp
   ```

   The dialog also includes Claude Code's `.mcp.json` and the snippet for Codex's `config.toml`. The
   commands were checked with Claude Code 2.1.308 and Codex 0.160.0.
4. In the client, `/mcp` shows the connected server and its tools.

The bridge accepts `--nome <name>` (the session name, which appears on the tab and in the dialog) and
`--espaco <name>` (ties the assistant to a workspace, below).

### What each permission does

The permission lives in the connection and folder form (the "Control by assistants (MCP)" category) and
applies to the connections inside the folder, like the network: **Inherit from folder** (the default; at the root it means
Off), or one of these:

| Permission | The assistant can |
|---|---|
| **Off** | nothing: the connection doesn't show up for it |
| **Read only** | see the connection, tabs, screen, files and tunnels; it doesn't type, run, write or open tabs |
| **Ask first** | everything, but each action that changes something waits for your OK in the app's dialog |
| **Free** | everything without asking, **except** what the block list catches |

The approval dialog shows the exact command (control characters appear as `<ESC>`,
`<BEL>`…, nothing is hidden) and the connection. You can approve just that action, check "Allow the
rest of this session on this tab" or refuse (with a reason, which goes to the assistant). The block
list ignores that checkbox: the dangerous command asks again.

The **block list** catches the most common dangerous commands (`rm -rf` and relatives, `mkfs`, `dd`
on a disk, `shutdown`, `curl … | sh`, touching accounts and `authorized_keys`, erasing history,
taking down the firewall…) after stripping `sudo`, `env`, assignments and `bash -c`, and **always asks,
even in Free**. You add your own in "Commands that always ask for approval" (a program or
`re:` with a regular expression). **It is not a guarantee**: a command written to get past
it gets past it. For a connection that can't afford mistakes, use "Ask first" or "Read only".

### Tools

| Tool | What it does | Requires |
|---|---|---|
| `listar_conexoes`, `listar_abas`, `listar_paineis` | what exists and what is open | view |
| `abrir_conexao`, `fechar_aba`, `reconectar_aba`, `organizar_paineis` | opens a tab (in a pane of your choice), closes, reconnects the same dropped tab, arranges the grid | change |
| `ler_tela`, `esperar`, `ler_saida` | reads the screen, waits for some text or silence, fetches the rest of a truncated response | view |
| `digitar`, `teclas` | types (with or without bracketed paste) and sends `Enter`, `Ctrl+C`, `F2`… | change |
| `executar` | runs a command in the tab, in view (waits for the prompt), or separately (with the exit code); "separately" goes to the tab when it's idle at the prompt, unless "Run the assistants' commands in the tab" is off, and the response says where it ran (`modo`) | change |
| `listar_arquivos`, `baixar_arquivo`, `enviar_arquivo` | the connection's files over SFTP, FTP, S3 or rclone | view / change |
| `listar_tuneis`, `ligar_tunel`, `desligar_tunel` | the connection's tunnels | view / change |
| `listar_portas`, `encaminhar_porta`, `parar_porta` | what is listening on the server and the tunnel for one of its ports (with a site name and HTTPS, if requested) | view / change |
| `listar_espacos`, `ler`, `editar`, `escrever`, `buscar`, `glob`, `bash` | the remote workspace (below) | view / change |

A tab an assistant uses (opened by it, tied to it by "Control with" or touched recently)
reconnects by itself when it drops, with the attempts and wait from Settings > Connections > When the
connection drops, whatever mode is chosen there: the assistant doesn't click the notice. While it
waits, `listar_abas` shows the `reconectando` state with the attempt number, and the tools that need
the tab up say the same. `reconectar_aba` reopens it immediately.

There is no fixed size cap: a response larger than "Maximum size of an answer" (Settings) comes
truncated, with the id to ask for the rest through `ler_saida`; zero means no limit.

### What redaction catches and what it doesn't

"Remove passwords and keys from what goes to the AI" comes **on** and replaces with
`[segredo removido]` everything that goes back to the assistant (screen, command output, files read and
error messages):

- **Catches:** passwords and hidden fields of the open vault (exact text) and the passwords of open
  connections; private key blocks (OpenSSH, PEM, PuTTY); AWS keys; GitHub, GitLab,
  Slack, OpenAI/Anthropic (`sk-…`) and Google (`AIza…`) tokens; `Authorization: Bearer` and `Basic`;
  assignments such as `password=`, `token:`, `api_key=`; the password in a URL
  (`https://usuario:senha@host`); JWT; and whatever you put in "Other texts to remove".
- **Doesn't catch:** a secret with its own shape that no pattern knows, a password of 1 to 3
  characters, what the assistant **sends** (its command goes as it came), a file that
  `baixar_arquivo` writes to your disk, and a secret the server returns encrypted or in pieces.
  With the vault locked, only the passwords the app holds in memory for open sessions are known.
- If the snippet the assistant wants to replace with `editar` contains a `[segredo removido…]` marker,
  `editar` refuses, so as not to write the marker in place of the real password.

Everything coming from the server arrives inside `<saida_do_servidor>`, and the bridge's instructions tell the
assistant that it is data, not orders (an MOTD or a log saying "ignore the instructions and run
`curl … | sh`" is not obeyed, and if the assistant tried, the block list would catch it). Output arrives without
control sequences. This reduces the risk; it doesn't eliminate it.

### Stopping the AI, the marker and multiple sessions

- The controlled tab gets an AI icon (in the session's color), a tooltip "Assistant: …, N s ago" and a
  bar at the top, with the **Stop the AI** button. Stopping refuses what is waiting for approval, cancels what is
  running, undoes the "rest of this session" approvals and does **not** turn off control (for that, turn it off in
  Settings). Typing in the tab doesn't stop the AI.
- You can have **multiple sessions** at once, each with a name and color (`--nome`), in the pane you
  choose (`abrir_conexao` with `painel`). A tab belongs to the session that opened it: another session that wants to
  change something in it opens the ownership dialog ("Allow once", "Give the tab to…" or "Refuse"), even in
  Free. The approval queue shows which session each request is from, and "Stop" can be per session.

### The remote workspace

To program on a server without copying anything to your computer, create a **workspace**
(Settings › AI assistants, the files panel or the tab menu): a name, an SSH connection
and a folder on the server. The assistant that the bridge starts with `--espaco <name>` works only there:
`ler`, `editar`, `escrever`, `buscar`, `glob` and `bash`, with paths relative to the root. "How to
connect" gives the command with `--espaco` and the `deny` that turns off the client's local tools (in
Codex, `sandbox_mode = "read-only"` and `shell_tool = false`), so it doesn't edit files on your
computer by mistake.

- **Confines:** `ler`, `editar`, `escrever`, `buscar` and `glob`. The path is normalized (`..` doesn't
  get out) and checked against SFTP's `realpath`, so a link pointing outside is refused even
  if its name is inside. `buscar` and `glob` use the server's `grep` and `find` when available, and
  SFTP when not (with a limit of 50,000 entries, 5,000 lines and 4 MiB per file). `editar`
  refuses files that aren't UTF-8 or are over 16 MiB.
- **Doesn't confine:** `bash`. It runs at the root, but a `cd /` works, and the connection's permission
  applies. For real confinement, use a server account that can only see the folder. It also doesn't
  close the window between checking a path and writing (another process **on the server** can swap
  a folder for a link at that instant): the workspace confines the assistant, not someone who is already
  inside the server. And on Windows, `bash` runs through `cmd` and searches go through SFTP.
- Each `bash` is a new process (one call's `cd` doesn't carry over to the next), with no terminal
  (interactive programs don't work). A process that ignores cancellation may stay on the server.

### Controlling a tab with Claude Code, Codex or OpenCode

One click does it all: **Control with ▸ Claude Code / Codex / OpenCode**, in the terminal's right-click
menu, in the tab menu and in Tools › AI assistants (which controls the current tab).

- If control by assistants is off, the click turns it on (with the usual notice,
  the first time) and a bar says so, with the **Turn off** button.
- The app opens, to the right of the tab, a local shell tab running the assistant **on this
  computer**, already connected to the app through MCP (`tirech-omni mcp --nome … --aba <chave>`) and tied
  to that tab: it becomes its owner, gets **Free** only on it while it's running and uses that
  tab when it doesn't name another (`listar_abas` marks it with `alvo: true`). This also works for the standalone local
  shell and for a connection with the Off permission, without touching the saved permission. The
  block list still asks for approval.
- MCP goes through each assistant's command line (`--mcp-config` in Claude Code, `-c
  mcp_servers…` in Codex, `OPENCODE_CONFIG_CONTENT` in OpenCode): **the assistant's configuration
  isn't changed** (nothing is written to `~/.claude.json`, `~/.codex/config.toml`
  or `opencode.json`). In Claude Code the server is called `tirech-omni-aba`, so it doesn't
  collide with one the user already added by hand.
- Clicking again, on the same tab and with the same assistant still open, just brings its tab to the
  front. The submenu doesn't appear on the assistant's own tab.
- **The conversation log**: the app keeps the id of each conversation an assistant had with a
  tab (by its connection; the standalone local shell has its own), in `conversas.json`. Claude Code
  opens with `--session-id` (the app picks the id) and continues with `--resume`; Codex reports the id of the
  new conversation through `notify` (the app calls the `notify` you already had) and continues with `codex
  resume <id>`; in OpenCode the app finds the session through `opencode session list` and continues with
  `--session`. "Control with" on a tab that has already talked to that assistant asks
  **Continue the conversation?**: up to 5 conversations, the most recent one selected, each with its first
  message and date, plus **New conversation** and **Remember my choice**. With no previous conversation, it opens
  directly. Conversations the assistant deleted (Claude Code deletes those older than 30 days) drop off
  the list.
- Closing the app with assistants open and opening it again brings the assistants back, each to the
  right of its tab, **continuing the conversation**, without asking; opening a workspace
  does too. In Settings › AI assistants › **Assistants on this computer** you can turn off the log
  (`conversas_lembrar`), the return of the assistants (`conversas_reabrir`) and choose what
  "Control with" does on a tab that already had a conversation (`conversas_ao_controlar`: Ask, Continue
  the latest or Start a new one).
- **Give this tab to ▸** (in the same three places) ties more tabs to an assistant that is already
  open: it lists the connected sessions by name, and the click makes the chosen one the tab's owner (without
  asking) and gives it **Free** only there, as with "Control with" (also on the standalone local shell
  and on an Off connection; the block list still asks for approval). Its default tab doesn't
  change, and `listar_abas` marks the given tab with `dada: true`. The permission goes away when the assistant
  exits. The submenu doesn't appear without a connected session or on an assistant's tab, and doesn't list whoever already
  has the tab.
- In Claude Code, the app's notices (the given tab, files dropped on the tab) arrive along with the
  next message you send, without waiting for the assistant to call a tool: the app launches
  Claude with `--settings` pointing to a `UserPromptSubmit` hook (`tirech-omni mcp --avisos
  <code>`), in a `lancar-<n>-avisos.json` file next to `lancar-<n>.json`. Your
  Claude Code settings don't change.
- The app's server also offers three slash commands in Claude Code (MCP prompts):
  `/mcp__tirech-omni-aba__abas` (the open tabs and which ones it controls),
  `/mcp__tirech-omni-aba__conectar web1 web2` (opens the connections, one per pane, and arranges the
  grid) and `/mcp__tirech-omni-aba__rodar-em-todas df -h` (runs the command in each tab it
  controls and compares the outputs). With the server added by hand, the prefix is the name you gave
  it (`/mcp__tirech-omni__abas`).
- When the assistant exits, the permission and the ownership go away, and the tab is left ended (**Reconnect**
  opens it again). Closing the target tab doesn't close the assistant.
- If the assistant isn't installed, a bar says so, with **How to install**, and the new tab
  closes by itself. In Settings › AI assistants › **Assistants on this computer** you can
  set the path of each one's executable and its extra arguments (for example Claude Code's
  `--dangerously-skip-permissions`; the app doesn't suggest any). The path belongs to
  that computer and isn't included when exporting settings.
- The assistant starts in the **assistants' working folder**, which you choose in the same section.
  When empty, it's an app folder in the system cache (`~/.cache/tirech-omni/assistentes` on Linux,
  `~/Library/Caches/tirech-omni/assistentes` on macOS, `%LOCALAPPDATA%\Tirech Omni\assistentes`
  on Windows), created on startup. What the assistant leaves there (drafts, scripts, clones) doesn't
  go away by itself: **Open the working folder** shows what's there and **Clean the working
  folder…** deletes everything inside it, after saying how many items and where. The app refuses to
  clean the home folder and the root.
- On Linux and macOS the assistant runs through the login shell (`$SHELL -l -c exec …`), to
  find your terminal's `PATH`, plus the folders where installers put the program
  (`~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `~/.volta/bin`, the
  asdf and mise shims, `/opt/homebrew/bin` and `/usr/local/bin`), at the end: the OpenCode
  installer writes `PATH` into `~/.bashrc`, which the non-interactive login shell doesn't read up to there.
  On Windows, the app looks in `PATH` and also in
  `%USERPROFILE%\.local\bin` and `%APPDATA%\npm`, and a `.cmd` runs through `cmd /c`.

### AI cluster

Tools › Cluster mode › **AI cluster…** (or "Open in AI cluster…" in the right-click menu
of a group or a connection) opens the chosen machines in a window, side by side, and
an assistant on this computer in a pane beside them, to control all of them at once.
In a folder's right-click menu, **Open all controlled by AI ▸** Claude Code, Codex or
OpenCode does the same with all the terminal connections in the folder, skipping the dialog (with
control off, the control notice comes first, as in the dialog).

- The assistant is launched as with "Control with" (same command line, same working
  folder, same notices), but the bridge goes with `--cluster` and without `--aba`: it has no default
  tab, and the instructions say which machines there are and that each terminal tool takes the
  `aba` argument.
- Each machine tab that opens in the window is **given** to it (like "Give this tab to"):
  it becomes its own, with **Free** only there, and the notice reaches Claude Code along with the next message.
  The tabs that were already in the window before (when only one window fits) and the assistant's own tab
  are not included.
- With control by assistants off, it goes through the turn-on notice first, and the cluster opens
  after you accept.
- It's separate from workspaces: it's only kept if you save the window as a workspace.

### Audit

Each request becomes a line in `mcp/AAAA-MM-DD.jsonl` in the data folder (0600 on Unix): when,
bridge, session, tool, connection, tab, arguments (with the password and the sent content redacted),
decision, reason, result and duration. **See today's log** (Settings) opens the file; "Write a log of what the
assistants do" turns it off.

### What it doesn't do yet

- It doesn't confine the workspace's `bash` (above) or protect against someone already inside the server.
- There's no tool for background processes: use `nohup … > log 2>&1 &`.
- The mirror folder (a local, synchronized copy of the workspace) hasn't been built.
- "Control with" on Windows and macOS hasn't been checked on a real machine.
- The bridge on Windows has only been checked by compiling; the pipe test runs in the Windows CI, and the
  manual test plan with a real Windows, real Claude Code and real Codex is left to whoever has them.

## Where the files are

In the system's configuration folder (`~/.config/tirech-omni` on Linux), or in the folder the
`TIRECH_OMNI_CONFIG` variable points to:

- `connections.json`: connections and groups, written immediately and atomically. If it is
  corrupted, it is moved to `connections.json.bak` and the app warns you.
- `estado.json`: all windows, with the grid and the connections open in each pane, and the tmux
  session of tabs with a persistent session (the workspace format; the single-window format of the first versions is still read). It also stores the layout of each page's
  pinned panels (Terminals, Notes and Files: mode, side, sizes, order and the
  floating panels' window). With two copies of the app open on the same folder, only the first one restores
  and writes this file: the second opens without the tabs (they are already open in the first) and starts
  writing when the first one closes.
- `conversas.json`: the "Control with" assistants' conversation log, per tab (the
  connection id; `0` is the standalone local shell): the assistant, the conversation id in it, when it was opened and
  the first message. Up to 20 per tab and assistant. Deleting it only makes the app forget the conversations
  (they remain in the assistants).
- `espacos/`: the workspaces, one `<name>.json` per workspace, in subfolders if there are
  folders. It's a regular folder: you can copy it, version it and edit it through the file manager
  (Refresh, in the tree's group, rereads it).
- `pastas.json`: the last folder used in each file picker (importing keys, the vault, the
  networks…), so the picker opens there next time. Deleting it only makes the pickers go back to the
  default folders.
- `recentes.json`: the most recently opened connections and the ones pinned on Home. If corrupted, it is moved to
  `recentes.json.bak` and the app warns you.
- `configuracoes.json`: all the settings from the Settings page, with no secrets at all. A
  bad field goes back to its default by itself (with a notice on the page); fields the app doesn't know are
  kept when writing. If corrupted, it is moved to `configuracoes.json.bak` and the app warns you.
- `pasta.txt` (in the system's configuration folder, not the data folder): the path of the data folder,
  when it was changed through the Settings page.
- `trava.lock`: the lock of the first copy opened with the folder. In portable mode, it prevents the
  second copy; outside it, it says who owns `estado.json`.
- `agente.json`: whether the built-in agent starts with the app, the default lifetime and the file
  keys it loads on startup (only the file path, the confirmation and the lifetime; never the
  passphrase). Vault keys aren't stored here: they live in `vault.json` itself. The agent's
  socket (`agent.sock`) lives in `$XDG_RUNTIME_DIR/tirech-omni/` (or `tirech-omni-agent.sock` in
  `~/.ssh`) when the app uses the default configuration folder; with `TIRECH_OMNI_CONFIG`, it lives in that
  folder.
- `redes/<id>/trava.lock`: the lock of a built-in network profile (OpenVPN, WireGuard, Tailscale,
  Nebula and ZeroTier) while its engine is running. Two open copies of the app don't bring up the
  same network together (with the same identity, one would knock the other off the server): the second one says
  it's already up in the other.
- `redes/<id>/known_hosts`: the server keys for each network profile (the same IP has a
  different key in each network). The profiles themselves live in `connections.json`.
- `ia.json`: the AI providers (name, URL, dialect, model, network, TLS and headers) and the
  default provider. **The API key isn't stored here**: it lives in a vault item ("API key — …")
  and the profile stores only its id. If corrupted, it is moved to `ia.json.bak` and the app warns you.
- `mcp/lancar-<n>.json`, `mcp/lancar-<n>-avisos.json` and `mcp/conversa-<n>.json`: the files for
  each "Control with" launch (the bridge, the Claude Code notices hook and the id of Codex's
  new conversation, which `notify` writes). They stay while the app is open (
  Reconnect reuses them) and are deleted on the next startup.
- `mcp/AAAA-MM-DD.jsonl`: the MCP audit log, one line per request from an
  assistant: when, bridge, session, tool, connection, tab, arguments (with the password and the
  sent content redacted), decision (allowed, approved, refused…), reason, result and
  duration. Written with 0600 on Unix; `ia.auditoria` turns it off.
- `mcp.sock` (Unix): the app's channel to the `tirech-omni mcp` bridge, only while `ia.mcp_ligado`
  is on. On Linux it lives in `$XDG_RUNTIME_DIR/tirech-omni/`; without that variable and on macOS,
  in `/run/user/<uid>/tirech-omni/` when that folder exists, and otherwise (and on macOS)
  in the configuration folder, with 0700 on the folder and 0600 on the socket. "Control with" and the AI
  cluster pass the address to the bridge (`--canal`), because Codex runs MCP servers without
  `XDG_RUNTIME_DIR` or `TIRECH_OMNI_CONFIG`. MCP fields of `ia` in `configuracoes.json`:
  `mcp_ligado`, `bloqueio_extras` (programs or `re:` expressions that always ask for approval),
  `auditoria`, `marca_secs` (how long the tab stays marked, from 1 to 3600), `caracteres_por_resposta`
  (0 means no limit), `finais_de_prompt` (in addition to `#`, `$`, `%` and `>`) and `pasta_troca` (empty means
  `Downloads/Tirech Omni`), plus the conversation log ones, `conversas_lembrar`, `conversas_reabrir`
  and `conversas_ao_controlar` (`perguntar`, `continuar` or `nova`). Each connection's permission is the `mcp` field in `connections.json`
  (`desligada`, `so_ler`, `aprovar` or `livre`; without it, it inherits from the folder).
- `certificados.json`: the fingerprints of the FTPS certificates the user trusted, by
  `host:port` (next to `known_hosts`).
- `vault.json` and `vault.key`: the password and key vault (version 3: folders and items with
  `uid`, tombstones for removals and the SSH keys) and the key that opens it (gone when there's a master
  password). `cofre.json` and `chave` from older versions are renamed on first open, and
  version 1 and 2 vaults move to version 3 on the first write. Written with 0600 on Unix. If
  `vault.json` is corrupted, it is moved to `vault.json.bak` and the app warns you. "Use instead
  of this one" leaves the previous vault as `vault.json.bak-<date and time>` (and `vault.key`).
  The vault folder can be different from the data folder ("Vault location", in `configuracoes.json`).
- `vault/`: note attachments, one encrypted file per attachment (name = id), next to `vault.json`.
  Those left without a record (an interrupted save) are deleted after 24 hours.
  When switching vaults, the old `vault/` becomes `vault.bak/` (or `vault.bak-<date and time>/`).
- `quedas.log`: the panic and the error that closed the app, with date, system and stack trace;
  it starts over above 512 KiB. A crash of the main thread also opens a notice with its path.
- `desenho.txt`: the startup guard (`<degrau> ok` or `<degrau> abrindo`). If the app crashed before
  showing the window, the next startup renders in a simpler way (the frame on the CPU; on
  Windows, also DirectX 12 and OpenGL). Deleting the file goes back to normal rendering.
- `vault-orphans.json`: in the data folder, the files in `vault/` without a record and the time the
  collector first saw them (the 24-hour margin gives sync time to bring the
  record from another machine).
- `*.tirech-vault`: the file from "Take the vault to another computer…", written wherever the user
  chooses.
- `logs-cifrados.pub`: the public key for encrypted logs, created the first time
  "Encrypt logs" is turned on. Only the public key lives here: the private one is the "Encrypted logs key" item
  in the vault, so writing a log doesn't need the vault open, and reading one does.
- `*.log.cifrado`: session logs written with "Encrypt logs" on, in the logs folder
  (the same as the plain-text `.log` files). Segments with a `TRCHLOGS` header and authenticated records
  (AES-256-GCM); the Logs page reads, searches, exports as text and converts from one format to the
  other.
- `~/.ssh/config`, `.bashrc`, `.zshrc` and `~/.config/fish/conf.d/tirech-omni.fish`: they only receive the
  agent block if the user turns on "Use in the system ssh" (and "Also in shells"). The block is
  marked by `# >>> tirech-omni: agente` and removed entirely when turned off.

## Settings

The **Settings** page (Ctrl+,) follows the Windows 11 Settings pattern: cards in a single
scroll, with the sections General, Terminal, Connections, Password vault, SSH keys and agent, Files,
FTP, S3, rclone, Networks, X11 and tunnels (default X11, display, virt-manager command and "Allow other
machines" on new tunnels), Windows and workspaces and Shortcuts. Each change takes effect immediately and is written to
`configuracoes.json`; whatever needs an app restart (the data folder) has the "Restart now" button.

- **Search**: the field at the top of the page filters the cards by name, description and similar
  terms (typing "font" shows only the font size). The title bar search (Ctrl+E)
  also finds settings and takes you to the card.
- **Restore**: each section has "Restore this section's defaults", and the end of the page has one for
  everything. After restoring, the **Undo** button goes back to what it was. Nothing asks for confirmation
  or the master password.
- **Shortcuts**: the app's shortcuts are editable (type `Ctrl+Shift+T`, for example); an empty
  field goes back to the default, and a duplicated shortcut warns you immediately.
- **Export and import**: writes and reads the settings as JSON. The file doesn't carry passwords
  or vault items (the rclone credential id is left out). Importing shows what was
  applied and what was ignored, and "Undo" reverts it.
- **Data folder**: the card shows where the app keeps everything; you can open it, change it (the app copies the
  data there, or only points to it if the new one already has a `connections.json`, and starts using the folder
  on the next startup) or go back to the default.
- **Password protection**: "Clear the clipboard" and "Lock when idle" exist and
  start **off**. Protecting passwords is a choice of whoever uses the app, and the app keeps
  saving passwords. Automatic locking only works with a master password and counts
  from the last use of a password.

What the page doesn't do yet: sync between machines, watch the file for
changes made outside the app (restart after editing the JSON by hand), choose the font family,
edit the scheme colors, capture a shortcut by pressing the keys, and accept a server
key that has changed.

## Language

The interface comes in **Português (Brasil)**, **English** and **Español**. In Settings › General ›
Language, "Use the system language" is the default (`pt*` opens in Portuguese, `es*` in Spanish and
any other language in English). The
change takes effect on restart: the card shows the notice and the **Restart now** button, which closes through the
normal path and reopens the app with the tabs restored. Sizes, numbers, dates and the "N days ago"
in recents follow the language; the Settings search finds by the terms of the current language and by
the Portuguese ones.

Not translated: what the user writes (connection names, folders, vault items), the files
written to disk (they store keys, never screen text), ids, the error messages from the lib and the system
(the app translates only the sentence around them) and the command-line tools (`tirech-omni-tools`).

The texts live in `crates/tirech-omni/i18n/<language>/*.ftl` (Project Fluent), one file per
area. In code, `t!("chave", nome = valor)`.

**Adding a language:**

1. Copy `crates/tirech-omni/i18n/en-US/` to `i18n/<tag>/` (`es-ES`) and translate it.
2. Add the variant to `Idioma`, with `tk!("opcao-idioma-<tag>")` and the autonym in the `.ftl` files of all
   languages (the key is `opcao-idioma-<tag>`).
3. Add the folder to the `recursos!` macro, to the `fontes`, `etiqueta` and `pacote` lists, and the case
   in `resolver` (the `es` prefix becomes `EsEs`).
4. Run `cargo test -p tirech-omni --test idiomas`: the tests point out what's missing.


## X11, virt-manager and tunnels

Two things that `ssh` does and the app builds into the connection itself: **showing the windows of
the server's graphical programs here** (X11) and **forwarding ports** (tunnels). Everything works
with a jump server and with the connection's network profile.

**On the connection** (Properties, **Forwarding** category, SSH only):

- **Forward X11**: Default, Off, Untrusted (`ssh -X`) or Trusted (`ssh -Y`). Default follows
  Settings ▸ X11 and tunnels (which starts as Trusted).
- **Tunnels**: the "…" button opens the editor. Types: Local (`-L`), Remote (`-R`, port 0 lets the
  server choose), Dynamic (SOCKS5, `-D`) and Server Unix socket. Each tunnel has "Enabled"
  (starts with the connection) and "Allow other machines" (listens on `0.0.0.0` instead of `127.0.0.1`).
- **Tunnels only (no shell)**: the connection only holds the tunnels; the tab gets the suffix "· tunnels".
- **Stop if a tunnel fails**: like `ExitOnForwardFailure`. Without it, a busy port becomes a warning
  and the connection goes on without the tunnel.

**Active tunnels**: the tab menu (right-click the tab) has **Active tunnels…**: the list with
the state, where each tunnel listens, the open connections and the bytes in each direction; you can stop,
start again and add tunnels on the spot. If a tunnel doesn't come up, the app warns you and points to that list.

**Graphical programs** (connection menu in the tree):

- **Open virt-manager**: runs `virt-manager --no-fork` on the server, over X11, without opening a tab. The
  window shows up here; the app's notice goes away when the program ends, and closing the notice closes the program.
  The command can be changed in Settings ▸ X11 and tunnels.
- **Open local virt-manager**: only shows up if `virt-manager` is installed on this machine. The app
  finds the server's libvirt socket (`/var/run/libvirt/libvirt-sock` or `virtqemud-sock`), brings up
  a tunnel to it and runs `virt-manager -c qemu+unix:///system?socket=…` here.
- **Run remote graphical app…**: asks for a command (`gitk`, `wireshark`…); with "Show the
  output in a tab" the program runs in a terminal tab, so you can see the text it writes. Recent
  commands go into the dialog's list and onto Home.
- A graphical program uses Trusted X11 when the connection has X11 set to Off, and ignores the connection's
  "Tunnels only" (the program needs the session's own shell).
- Known errors come with a plain explanation: program not installed on the server, "cannot open
  display" (missing `X11Forwarding yes` or `xauth`) and lack of permission on libvirt.

**What the server needs** (`/etc/ssh/sshd_config`):

```
X11Forwarding yes               # and the xauth package installed
AllowTcpForwarding yes          # -L, -R and -D (the default is yes)
AllowStreamLocalForwarding yes  # the Unix socket tunnel (the default is yes)
GatewayPorts clientspecified    # only for "Allow other machines" on a remote tunnel (-R)
```

If `sshd` answers "X11 forwarding request failed" and its log says `Failed to allocate
internet-domain X11 display socket`, the server can't open the display port (common
with IPv6 disabled): put `AddressFamily inet` in `sshd_config`.

**This machine's X server**:

- **Linux**: the session's X11 or XWayland (`DISPLAY` is already set). In Settings ▸ X11 and
  tunnels you can choose another display.
- **Windows**: install VcXsrv or Xming, start it with access control disabled and use
  display `localhost:0` (that's what the app assumes when there's no `DISPLAY`).
- **macOS**: XQuartz; the app finds its socket on its own.

**Trusted vs. untrusted.** Trusted (`-Y`) gives the remote program the real cookie, with
full access to the X server (keyboard, screen and other windows); it's what virt-manager and most
programs ask for. Untrusted (`-X`) creates a cookie restricted by the SECURITY extension
(`xauth generate`, valid for one hour since last use) and breaks many programs; if the
local X server doesn't have the extension (the XWayland of several distributions), the app uses
trusted and says so in the terminal. The remote server always gets a fake cookie: the real one
only reaches the local X server.

**Remote or local virt-manager.** The remote one runs there and draws here over X11: simple, but
slow on a bad network. The local one runs here and talks to the server's libvirt through the tunnel: the
interface is snappier. In both cases the connection's user must be in the server's `libvirt` group
(`sudo usermod -aG libvirt usuario`, then log out and back in); without that the notice
is "permission denied" or "authentication unavailable".

**Virtual machine console (SPICE and VNC).** Local virt-manager opens the console over
`ssh`, but you can open just a VM's console with a tunnel and `remote-viewer`
(`virt-viewer` package):

```sh
ssh servidor virsh domdisplay minha-vm        # spice://127.0.0.1:5900
# Local tunnel: port 5900 here → 127.0.0.1:5900 on the server
remote-viewer spice://127.0.0.1:5900
```

**Browsing through the server.** A **Dynamic** tunnel on port 1080 is a real SOCKS5
proxy: point the browser to `127.0.0.1:1080` (SOCKS v5, with name resolution through the
proxy) and pages go out through the server. `CONNECT` only, no authentication.

**Remote Docker.** A **Unix socket** tunnel from `/var/run/docker.sock` to a port here
(or a file) lets the local `docker` talk to the server's, without exposing the daemon on the network:

```sh
docker -H tcp://127.0.0.1:2375 ps       # tunnel: 2375 here → /var/run/docker.sock
```

Not supported yet: `-R` to a Unix socket, SOCKS4 and a password on `-D`, an X server built into the app,
and a built-in SPICE/VNC viewer.

## Server ports

In an SSH tab, **Server ports** (tab menu) opens the right-hand pane with what is listening on the
server: the reach (a port that only listens on loopback shows as "server only"), the
process, the user, the port name and, for web pages, the title and the known app (phpMyAdmin,
Adminer, pgAdmin). The list comes from `ss -ltnpeH`; without it, from `/proc/net/tcp` and `/proc/net/tcp6`
read over SFTP, and from `netstat -an` where there's no `/proc`. Docker and Podman containers show up
through their IP.

- **Forward** opens the same port on `127.0.0.1` (or +8000 below 1024, or the next
  free one); **Open in browser** forwards and opens the URL with the app's path; **Pin to this connection** saves the
  tunnel in the connection, and **Stop** closes it. "Forward destination…" reaches any `host:port` as seen
  from the server.
- **Site name**, in the port menu, routes the tunnel through an app relay that rewrites `Host`,
  rewrites `Location` and cookies and, for an HTTPS server, handles TLS (the server's certificate
  is not checked): the browser opens `http://{site}.localhost:{porta}` without a warning. Pinned tunnels
  with a site name or HTTPS come up on their own when the session opens.
- An `http://localhost:3000` printed in the tab opens through the tunnel on Ctrl+click.
- In Settings › Server ports: list refresh interval, web probe, use of `ss`/`docker`, new-port
  notice, links through the tunnel and system ports.
- The assistant (MCP) has `listar_portas` (view), `encaminhar_porta` and `parar_porta` (change).

## Client networks with the same IP

Two clients with the same range (`10.0.0.0/24` on both) don't fit in the same routing table.
The app solves this with a **network profile** per client: the profile points to a SOCKS5 proxy on
`127.0.0.1` (a container with the client's VPN and `microsocks`), plus the commands to bring it up and
down. A group or a connection picks the profile in the **Network** row; whatever is inside the group
inherits it (`Inherit (Client A)`), and `Direct` breaks the inheritance. The profile applies to SSH, SFTP, Telnet,
FTP, S3 and rclone (with the caveat that rclone 1.60's `sftp` and `ftp` ignore the proxy); the
local shell and serial don't use a network. The tab gets the suffix ` · Client A`, and the bring-down
command runs when the profile's last tab closes and when the app closes. Each profile has its own
`known_hosts`.

The app does **not** bring up the VPN by itself, nor does it manage containers: the recipes (WireGuard,
OpenVPN, OpenConnect, openfortivpn, `ip netns`, VM) and a local demo without a VPN are in
[`redes/LEIAME.md`](../../crates/tirech-omni/redes/LEIAME.md) (in Portuguese).

### Embedded Nebula network

If the client uses a [Nebula](https://github.com/slackhq/nebula) network, you don't need a container:
the **Embedded Nebula** profile joins the network inside the app itself, **for its own traffic only**.
It doesn't create a virtual network adapter, doesn't touch the computer's routes or DNS and doesn't ask for
administrator rights. The protocol is implemented in the lib (no `nebula` program, no Go and no
system calls out), and the Nebula network's `10.42.0.7` only exists for the connections the profile
serves; your other programs can't see that network.

**How to use it**

1. In Tools ▸ Networks, **New** and, in the **Type** row, choose **Embedded Nebula**.
2. **Import config.yml…** and pick the `config.yml` that the network administrators gave you. The
   files it references (`ca.crt`, `host.crt` and `host.key`) must be next to it, as
   in Nebula. The vault must be unlocked.
3. Link the profile to a group or a connection in the **Network** row, like any profile. Use the
   Nebula network IP as the connection's host (`10.42.0.1`): the computer's DNS doesn't know the
   Nebula names.
4. Connect. The network **starts on first use** and **stops when the profile's last tab closes** (and
   when the app closes, notifying the peers). The page's **State** row shows "Off",
   "Connecting…", "Connected as 10.42.0.20 (1 lighthouses, 2 tunnels)", "Trying again: …" or
   "Failed: …". **Test** starts the network, waits for the connection and reports the IP.

It applies to SSH, SFTP, Telnet, FTP, S3 and rclone (with the caveat about rclone
1.60's `sftp` and `ftp`), through the same paths as the other profiles. Two Nebula profiles for different networks, even
with the same range, work at the same time, each with its own engine. The tab gets the suffix
` · Client N` and its tooltip says "Network: Client N (Nebula, 10.42.0.20)".

**Where the secrets live.** The import separates the config from the identity: the CA, the certificate and the
key go to a **Nebula** item in the vault (encrypted like the other items; in the item's panel
you see the address, the groups, the validity and the fingerprint, can show the private key and replace the
certificate when the administrators issue a new one). What stays in the profile, in `conexoes.json`, is only
the config without `pki.ca`, `pki.cert` and `pki.key`: no PEM at all. Importing the same certificate again
doesn't duplicate the item. A Nebula item is exported and imported with the rest of the vault (the app's `.json`
vault, Bitwarden as a secure note with its own fields); CSV carries no secrets and skips this
type. The **Config** row opens the stored `config.yml` (lighthouses, `static_host_map`,
`punchy`, `tun.unsafe_routes`…); a PEM pasted there is rejected.

**Warnings and errors.** The import refuses, stating the reason: certificate expired or not yet
valid, a key in place of the certificate, a lighthouse `config.yml` (`am_lighthouse:
true`), `ca`/`cert`/`key` written inline in the YAML instead of as files, an identity that doesn't
match the CA. These become warnings (and the import goes on): `am_relay`, `firewall.inbound`, `sshd`, `stats`,
`logging` and other keys that only make sense in the Nebula service, and anything the lib finds odd in the
config. If the network doesn't connect, the notice says whether it was the locked vault, the identity that disappeared from the
vault, a lighthouse that doesn't answer or a `cipher` different from the network's. The network is **fail-closed**:
if Nebula doesn't come up, the connection does **not** go straight out over the internet; it fails.

**What it doesn't do.** TCP only and IPv4 only; it doesn't accept incoming connections (the app only opens them; SSH's `-R` keeps
working, because it runs inside the SSH connection); it isn't a lighthouse or relay; it doesn't sign
certificates; it doesn't read keys from a token (PKCS#11); the P-256 curve and **relays** aren't supported yet (a network where
both sides are behind symmetric NAT needs a relay and gives "no direct path").
Nothing of the Nebula network is exposed to other programs: there's no port, address or credential to
copy.

### Embedded OpenVPN network

If the client hands you an `.ovpn`, you don't need a container or the `openvpn` program either: the
**Embedded OpenVPN** profile joins the VPN inside the app itself, **for its own traffic only**. It doesn't
create a virtual network adapter (TUN), doesn't touch the computer's routes or DNS and doesn't ask for
administrator rights. The protocol is implemented in the lib, written from the public description of the
protocol (OpenVPN is a registered trademark of OpenVPN Inc.; the app is not affiliated with it), and the IP the
server assigns only exists for the connections the profile serves.

**How to use it**

1. In Tools ▸ Networks, **New** and, in the **Type** row, choose **Embedded OpenVPN** (or use
   **Import .ovpn…**, which creates the profile right away). The vault must be unlocked.
2. **Import .ovpn…** and pick the file. If it references other files (`ca ca.crt`, `cert`,
   `key`, `tls-auth`…), they must be in the same folder; files outside it are rejected.
   If the `.ovpn` has `auth-user-pass`, the app asks for the username and password (the password is optional:
   without it, the app asks on every connection).
3. Link the profile to a group or a connection in the **Network** row, like any profile. Use the
   VPN IP as the connection's host: the computer's DNS doesn't know the names inside it (the DNS
   the server pushes only applies to the profile's connections).
4. Connect. The VPN **starts on first use** and **stops when the profile's last tab closes** (and
   when the app closes, notifying the server). The **VPN state** row shows "Off",
   "Connecting to …", "Connected as 10.8.0.6 (…, AES-256-GCM)", "Retrying: …",
   "Waiting for your answer: …" or "Failed: …". **Test** starts the VPN, waits for the connection and reports the IP.

It applies to SSH, SFTP, Telnet, FTP, S3 and rclone, through the same paths as the other profiles. Two
OpenVPN profiles for different networks, even with the same range, work at the same time, each one
with its own engine. The tab gets the suffix ` · Client V` and its tooltip says "Network: Client V
(OpenVPN, 10.8.0.6)". A profile has only one embedded network: Nebula **or** OpenVPN.

**Verification codes and challenges.** If the server asks for a code (OTP, `static-challenge` or
dynamic challenge), the app opens a dialog with the server's question. Answering sends the code;
closing without answering cancels the connection. On the password prompt, the **Save the password in the vault**
checkbox stores the answer in the identity item.

**Where the secrets live.** The import separates the `.ovpn` from the identity: the CA, the certificate, the
private key and the TLS key (`tls-auth`, `tls-crypt` or `tls-crypt-v2`) go to an
**OpenVPN** item in the vault (encrypted like the other items; the item's panel shows the CN, the server, the
validity and the fingerprint, can show the private key and replaces the certificate when the server
issues a new one). The `auth-user-pass` username and password go in the User and Password fields of the same
item. What stays in the profile, in `conexoes.json`, is only the `.ovpn` without those blocks: no PEM and
no password, and the editor of the **.ovpn profile** row rejects any text that contains one. Importing the same
certificate again doesn't duplicate the item. The item is exported and imported with the rest of the vault
(the app's `.json` vault; Bitwarden as a login with its own fields); CSV carries no secrets and skips
this type.

**Routes.** By default, what the server pushes (`push`) applies. Uncheck **Use the server's
routes** (that's `route-nopull`) to apply only the routes written in the `.ovpn`. Since only the app's
traffic goes through the VPN, the routes decide what goes through the VPN and what goes out directly.

**Warnings and errors.** The import refuses, stating the reason: certificate expired or not yet
valid, a key that doesn't match the certificate, a CA that didn't sign the certificate, `dev
tap`, `secret` (static key), encrypted private key, PKCS#12, CBC cipher without negotiation and an `.ovpn`
without `remote`. These become warnings (and the import goes on): `crl-verify`, `http-proxy`/`socks-proxy`, a certificate close
to expiring, and the options the app read and ignored (scripts and plugins **never run**; the list shows up
on one line of the notice, "Ignored options (n): …"). The VPN is **fail-closed**: if it doesn't connect (vault
locked, identity that disappeared from the vault, server that doesn't answer, password rejected), the connection
does **not** go straight out over the internet; it fails, and the notice says why.

**What it doesn't do.** TCP only and IPv4 only inside the VPN; UDP and TCP for transport; AES-GCM and
ChaCha20-Poly1305 (no CBC); it doesn't accept incoming connections (SSH's `-R` keeps working, because it runs
inside the SSH connection); it doesn't read PKCS#12, encrypted keys or tokens (PKCS#11); it doesn't open the login in the
browser (`OPEN_URL`: the state shows "Waiting for confirmation" and the page isn't opened). Nothing of the VPN is exposed to other programs: the
Networks page has no port, address or credential to copy, and the only listening socket is the
internal bridge, on `127.0.0.1`.

### Embedded WireGuard network

If the client hands you a WireGuard `.conf`, the **WireGuard** profile joins the network inside the
app itself, **for its own traffic only**: no virtual network adapter, no changes to the computer's routes or
DNS, no administrator rights and no `wg` program. It coexists with any other VPN
running on the computer. The protocol is implemented in the lib (Noise, cookie, transport, replay and
timers), with no Go and no helper program.

**How to use it**

1. In Tools ▸ Networks, **New** and, in the **Type** row, choose **WireGuard**. The vault must
   be unlocked.
2. **Import .conf…** and pick the file. The `PrivateKey` and the `PresharedKey` entries go to a
   **WireGuard** item in the vault; the profile keeps only the `.conf` without them, and the editor of the **.conf
   profile** row rejects any text that contains one. The page shows the node's addresses, the public
   key (for the other side to put in its `[Peer]`), the servers (`Endpoint`) and the allowed
   routes (`AllowedIPs`).
3. Link the profile to a group or a connection in the **Network** row and use the IP inside the network
   as the host. The network **starts on first use** and **stops when the profile's last tab
   closes**. The **Tunnel state** row shows "Off", "Connecting…", "Connected as
   10.8.0.2 (1 of 1 peers with a session)", "Trying again: …" or "Failed: …", and the peers
   list shows each one's session and traffic. **Test** starts the network and reports the IP.

Only what `AllowedIPs` covers goes through the tunnel; a destination outside it fails right away, saying
so (the app's traffic doesn't go out directly by mistake). It applies to SSH, SFTP, Telnet, FTP, S3 and
rclone. Two WireGuard profiles for different networks, even with the same range, work at the same
time, each with its own engine. The vault item is exported and imported with the rest (the app's
`.json` vault; Bitwarden as a login with its own fields, the private key hidden); CSV carries no
secrets and skips this type. The tab gets the network's suffix and the tooltip says "Network: Client W
(WireGuard, 10.8.0.2)".

**What it doesn't do.** TCP only and IPv4 only inside the tunnel; the `.conf`'s `DNS =` line doesn't apply yet
(use the destination IP, or a name the computer's DNS already knows); it isn't a server and doesn't accept
connections (SSH's `-R` keeps working, because it runs inside the SSH connection). If the network doesn't
connect (vault locked, identity that disappeared from the vault, server that doesn't answer), the connection
does **not** go straight out over the internet: it fails, and the notice says why. Nothing of the network is exposed to
other programs: the page has no port, address or credential to copy, and the only
listening socket is the internal bridge, on `127.0.0.1`.

### Embedded Tailscale network

The **Tailscale** profile joins a tailnet (the official Tailscale one or a **Headscale** one)
as a machine of the app itself, **for its own traffic only**, with no virtual network adapter, no
administrator rights and **no dependency on Tailscale being installed** on the computer (the two coexist: the app
is another machine on the tailnet, with its own name and IP). The client is implemented in the lib (ts2021
control, map, DERP, disco, STUN and MagicDNS), with no Go and no helper program. The machine starts
in `ShieldsUp`: on the tailnet, the app only goes out, it never accepts incoming connections.

**How to use it**

1. In Tools ▸ Networks, **New** and, in the **Type** row, choose **Tailscale**. The vault must
   be unlocked (the machine is kept in it). Leave **Server** empty for official Tailscale;
   for a Headscale, type the address (`https://…`) and, if its certificate comes from a private
   CA, paste that CA into **Server CA**. **Machine name** is how it appears on the tailnet
   (empty uses the computer's name).
2. Sign in one of these ways: **Sign in** (the app opens the login address in the browser, once;
   **Open again** opens it again) or **Use an auth key…** (paste the
   `tskey-auth-…` the tailnet generated; it's stored in the vault item and signs in without a browser). If the
   administrator requires approval, the state shows "Waiting for the machine to be approved".
3. Link the profile to a group or a connection in the **Network** row. The network **starts on first use**
   and **stops when the profile's last tab closes**. The **Tailnet state** row shows
   "Connected as 100.64.0.7 (empresa.ts.net, 3 machines online)", "Needs sign-in",
   "Waiting for the machine to be approved", "The machine key expired; sign in again",
   "Trying again: …" or "Failed: …". **Test** starts the network and reports the IP.

Inside the tailnet, use the **short name** (`servidor`), the full MagicDNS name or the
`100.x.y.z` IP as the host: MagicDNS only applies to the profile's connections. **Tailnet machines**
lists the ones the control server shows (name, IP, OS, online or offline, and whether the path is
direct or through DERP); **New connection…** creates a connection already linked to this network, and the ones that already
exist show as "Already added". The list is only a suggestion: it doesn't stop you from typing another
name. **Use subnets** (on by default) reaches the networks that other machines advertise;
**Ephemeral machine** makes the control server forget the machine when it goes away.

**Where the secrets live.** The **Tailscale** item in the vault holds the machine: the keys
(machine, node and disco), the login, the tailnet, the addresses, the key's validity and the auth
key, if any. The profile, in `conexoes.json`, keeps only the server, the name, the options
and the CA. Reopening the app **doesn't ask you to sign in again**: the state is reused. If the vault is
locked when the machine signs in (or saving fails), the app holds the new state in memory and
saves it as soon as the vault is unlocked; if the app closes before that, the next connection registers
a new machine. **Forget this machine** clears the state from the vault (the next use signs in as a
new machine; delete the old one in the tailnet's admin panel). The item is exported and imported with the
rest of the vault (the app's `.json` vault; Bitwarden as a login with its own fields, the keys
hidden); CSV carries no secrets and skips this type.

**Direct path or DERP.** The app discovers its public address via STUN and exchanges disco
messages with the other machine; if UDP gets through, it talks directly, and if it doesn't (or stops getting through)
it falls back to DERP, the tailnet relay, with no action from you. The list shows the path next to
"online". If a connection doesn't respond, check: (1) the tailnet state; (2) whether the destination
machine is "online"; (3) the tailnet ACLs, which may block the port (the app only says the
machine didn't respond); (4) "no UDP: DERP only" is normal on networks that block UDP, and is just
slower.

**What it doesn't do.** It isn't an exit node, doesn't advertise subnets or services, has no
Taildrop, Funnel, Serve or Tailscale SSH as a server, doesn't accept incoming connections (SSH's `-R`
keeps working, because it runs inside the SSH connection), doesn't support Tailnet Lock, and only carries TCP
over IPv4. Split DNS and the tailnet's name servers don't apply yet (only MagicDNS). If the
network doesn't connect (vault locked, identity that disappeared, control server that doesn't respond, not
signed in), the connection does **not** go straight out over the internet: it fails, and the notice says why. Nothing of the
network is exposed to other programs: the page has no port, address or credential to
copy, and the only listening socket is the internal bridge, on `127.0.0.1`.

### Embedded ZeroTier network

The **ZeroTier** profile joins a ZeroTier network (one on **ZeroTier Central** or on a self-hosted
controller) as a member that is the app itself, **for its own traffic only**: no virtual network adapter,
no changes to the system's routes or DNS, no administrator rights and **no dependency on ZeroTier being
installed** on the computer (the two coexist: the app is another member, with its own identity, address and
UDP port). The protocol is implemented in the lib in Rust (identity, ciphers, root, NAT,
network configuration, rules and ARP), with no C++ and no helper program.

**How to use it**

1. In Tools ▸ Networks, **New** and, in the **Type** row, choose **ZeroTier**. The vault must
   be unlocked (the identity is kept in it). In **Network ID**, type the network's 16 characters
   (`8056c2e21c000001`).
2. Click **Connect**. The first time, the app creates the identity (it takes a few seconds; the
   screen stays frozen during that time) and saves the **ZeroTier** item in the vault. If you already have an
   identity, **Import identity.secret…** uses its file instead of creating a new one.
3. A network with manual authorization leaves the state at **Waiting for authorization**, and the notice shows
   the **Member address** (10 digits). **Copy address** copies it; whoever administers the network
   ticks **Auth** for that member in ZeroTier Central (or in the controller). No restart needed:
   the app asks again on its own and moves to **Connected as 10.147.17.5 (network name)**.
4. Link the profile to a group or a connection in the **Network** row. The network **starts on first use**
   and **stops when the profile's last tab closes**; a new profile waits up to 120 s, enough time
   for someone to authorize it. **Test** starts the network, waits and reports the IP.

Inside the network, use the managed IP (`10.147.17.x`) or, with **Use the network DNS**, the names in the
domain the network advertises (DNS goes over TCP, and the network's server must accept TCP; other
names go to the computer's DNS, as usual).

**Options** (the ones from ZeroTier's `local.conf`, for this profile only): **Allow public IPs**
(`allowGlobal`), **Allow the default route** (`allowDefault`: all of the app's traffic goes through the
network), **Use the network DNS** (`allowDNS`), **Fixed IP** (`10.0.0.5/24`, for networks that don't
hand out addresses; empty uses the controller's) and **Moons**, one `id/seed` per moon,
separated by spaces, to orbit extra roots. **Roots and peers** shows the roots and the peers with
traffic (address, "direct" or "through the root" and the latency), for diagnostics.

**Where the secrets live.** The **ZeroTier** item in the vault holds the identity (the
`identity.secret`) and the node state (planet, moons, the last signed configuration and the
paths). The profile, in `conexoes.json`, keeps only the network ID, the options and the moons. Reopening
the app **doesn't ask for authorization again**: the same member comes back, and the last state gets it started
even with the controller offline. **Forget the state** deletes only the state and **keeps the
identity** (the member stays authorized). The item has **Export identity.secret…** (a file
readable only by its owner) and is exported and imported with the rest of the vault (the app's `.json` vault;
Bitwarden as a login with its own fields, the identity hidden); CSV carries no secrets and skips
this type. **One identity serves one network at a time:** duplicating the profile shares the item, and
the second one to start gets "identity in use"; for another network, create another item.

**Direct path or through the root.** The app talks to the ZeroTier roots (the public planet is
built in and updates itself) and, when UDP gets through, opens a direct path to the other
member; otherwise, the conversation continues through the root, more slowly. **Without UDP the network doesn't work in this
version** (the TCP relay doesn't exist yet). If a connection doesn't respond, check: (1) **Network
state**; (2) whether the member is authorized; (3) the network rules, which may block the port (the app
only says the machine didn't respond); (4) the computer's firewall, which may block UDP.

**What it doesn't do.** It doesn't attach the network to the computer (no other program sees its IPs), doesn't
accept incoming connections (SSH's `-R` keeps working, because it runs inside the SSH connection), isn't a
controller, root, moon, bridge or relay, and only carries TCP over IPv4: IPv6-only networks, tags,
capabilities, SSO login and the TCP relay are left for later. If the network doesn't connect (vault
locked, identity that disappeared, network that doesn't exist, authorization pending), the connection does **not** go
straight out over the internet: it fails, and the notice says why. Nothing of the network is exposed to other programs:
the page has no port or credential to copy, and the app's only sockets are ZeroTier's own UDP
socket and the internal bridge, on `127.0.0.1`.

## S3: where the credentials come from

The connection's **user** is the Access Key ID. The secret key (the "Password" row) and the session
token for temporary AWS keys ("Session token") live in the vault, in the credential item
(S3 type, with the token in a hidden field): they never go in plain text into `connections.json`. With no
credential, the secret key is asked for when opening, in the same dialog as SSH passwords, and can be
saved. With the vault locked, both rows are disabled.

File ▸ **Import from `~/.aws`** reads `credentials` (the keys) and the `config` next to
it (the region and `endpoint_url`) and creates one S3 connection per profile in an "Imported from
~/.aws" group; the provider is derived from `endpoint_url` when it's a known one (`:9000` becomes MinIO). The
keys and the token go to the open vault; with the vault locked, the connections are created without the
secret key, which will be asked for on connect, and the import warns you. A profile with no keys (an `sso` one, for example) is left out, with a warning.

## S3: what it doesn't do

Customer-provided key encryption (SSE-C), bucket ACLs and policies, versioning, lifecycle,
creating or deleting buckets (use the provider's console), the AWS credential chain
(`sso`, `credential_process`, profiles with `role_arn`) and `gzip`. The presigned link is download-only
(`GET`).

## rclone: installing and configuring

The app **doesn't ship** rclone: it talks to the binary (MIT, separate process) through its remote
control API (`rclone rcd`). Install it from <https://rclone.org/install/> (or through your
package manager). The **minimum version is 1.60**. The app looks for the binary at the path in
**rclone options ▸ rclone path** and, if that's empty, in `PATH`; without it, or with an old version, the
subpage and the Files page show a notice with the install address, and the rest of the app
works normally. The notice only appears after the first attempt to open rclone, never
at app startup.

- **One `rcd` per proxy.** The app starts `rclone rcd` on `127.0.0.1` (a free port), with a random user and
  password that only the app knows, when something needs it, and shuts it down on close. There's one
  process per proxy (`rcd` is launched with `HTTP_PROXY` and `HTTPS_PROXY`, and `NO_PROXY` leaves `127.0.0.1` out); today only
  the "no proxy" one is used, and the per-proxy map is waiting for per-connection proxies.
- **Configuration.** `rclone.conf` is the usual one (the one from `rclone config`), unless
  **Options ▸ Configuration file** points to another. Remotes created here also work in the
  terminal.
- **Encrypted configuration.** If `rclone.conf` has a password, a dialog asks for it and can store the
  password in the vault, in the "rclone configuration password" item; subsequent times it comes from there,
  with no dialog. With the vault locked it's asked for every time.
- **Remote passwords.** The passwords and secrets the form receives go to `rclone`
  (which obscures them in `rclone.conf`, as always) and, with the "Store the passwords in the vault" box
  checked, also to an `rclone: {nome}` item in the vault ("Connections" folder, one hidden field per
  password). They never go to `connections.json` or `rclone.json`. When editing, a blank
  password keeps the current one.

## rclone: what it doesn't do

Mounting remotes, syncing and scheduling, installing or updating rclone, and helping with remotes
that wrap others (`crypt`, `union`, `alias`: they open like any remote, but the app
doesn't explain their questions). The per-connection proxy and the binary options on the binary's
settings page are also on the Settings page (rclone section) and under the subpage's **Options** button.
`rcd` only listens on `127.0.0.1`. If the app crashes without closing (a `kill -9`, for example), its
`rclone rcd` process is left orphaned: end it from the task manager.

## Transfer queue

Above the queue table sits the summary of the whole batch (the copies queued in sequence, until
the queue empties), like the Windows 11 Explorer copy dialog:

- a bar for the **bytes** and, on the line above, the percentage, "128 of 340 files" and "1.4 GB of
  3.4 GB" (sizes in base 1024, as in the rest of the app);
- on the right, the batch **speed** ("12.4 MB/s") and the **time remaining**;
- in the table, each in-progress row has the **Remaining** column, with the time for that item.

**How to read the range.** The time remaining is a range, not a number: "3–5 min left" means
the copy should finish within that interval (10% to 90% likelihood). When throughput is steady, the
range is narrow ("about 40 s left"); when it fluctuates, it gets wide. The estimate comes from what the
queue measured **for that connection and that direction** (upload or download): a fixed cost per file
plus the remaining bytes over the throughput. That's why many small files take longer than the
bytes suggest. The range adjusts with every measurement, but only changes the text when the difference is worth
it, so it doesn't keep flickering.

**What shows up at each moment:**

- "**Counting files…**" and the indeterminate bar, while there are still folders to list: the total
  is only known later, and the app doesn't guess a percentage;
- "**Calculating…**", in the first seconds, until there's enough measurement (about 2 s of throughput
  and 3 files, in the case of files without bytes);
- "**Paused**" with the yellow bar; "**Waiting for an answer**" when the queue asks what to do
  about a conflict; "**Stalled for N s**" when nothing moves and it's neither of those;
- "**Done · N files**" with the bar full, or "**Done with N failures**" with the bar
  red.

**What doesn't count:** paused time, waits for an answer and files that failed
(a failure doesn't become per-file cost). After a pause, the speed goes back to the previous pace
instead of plummeting.

**What it doesn't do:** the app doesn't keep what the queue learned between runs (each run starts at
"Calculating…"), doesn't draw a speed graph and has no way to hide the summary. A
copy that finishes before the next one is queued opens a new batch, with its own summary.
While the server doesn't report a file's size (a download for editing, for example),
it counts as a file, with no bytes.

## Files: what isn't configurable yet

Parallel transfers, date and permissions, conflicts, hidden files, keepalive and
the editor are on the Settings page (Files section). These stay fixed: server dates, which
show in UTC, and FTP without ASCII type and without extensions.

## Tests

```sh
cargo test -p tirech-omni
TIRECH_OMNI_CONFIG="$(mktemp -d)" cargo run -p tomni-ui-cli -- lint -p tirech-omni
```

The key tests use the pairs in `tests/dados/` (one without a passphrase and one with the passphrase
`teste`) and, when they exist on the system, the real `ssh-add` and `ssh-keygen` to prove
agent compatibility. The SSH Keys and Agent page snapshots use folders in `/tmp`.

The `redes/` recipes (containers and VPNs) aren't part of `cargo test` or CI: the network
tests use fake proxies and servers and `sh` commands.

The lint checks both of the app's screens, the main window's (`main`) and a secondary window's
(`janela_terminais`), in both themes.

The tests use `Loopback` sessions in place of connections (the `FakeOpener`), a temporary folder
per test, a `MemFs` in place of the server and the disk on the Files page (which is why dragging from the
local pane to the remote one shows as "Copy" and not "Upload") and, in `tests/real.rs`, a real `sh` and
a real SFTP server (local `russh-sftp`, Unix only). FTP is tested with `MemFs` and
`FakeOpener`; the client itself has the real matrix in `tomni-ui/testes/ftp/` (Docker). S3 uses an
in-memory S3 server (`tomni-ui/crates/files/tests/comum/s3d.rs`, copied into `tests/comum`, included by the tests in
`tests/s3.rs`); the client also has an optional test against a real MinIO in
`tomni-ui/testes/s3/` (Docker, port 19000). rclone uses a hand-written fake `rcd`
(`tomni-ui/crates/files/tests/comum/rcd_falso.rs`, copied into `tests/comum`, included by `tests/rclone.rs` and `tests/visual_rclone.rs`),
which speaks the `rclone rcd` protocol and the questions of `falso` and `nuvem`; the client has
optional tests with real rclone (`TOMNI_UI_RCLONE=rclone`, see `tomni-ui`'s `CONTRIBUTING.md`). The lint needs the empty folder so it doesn't
read the connections of whoever runs it.
