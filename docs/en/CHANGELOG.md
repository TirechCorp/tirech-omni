# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [SemVer](https://semver.org/).

## [Unreleased]

### Added

- Links: a connection of type Link opens an address in the browser. With a network profile, it
  opens in a browser for that profile only, which goes out through the client's network (proxy
  or embedded VPN) via a local HTTP proxy; without one, in the default browser. "Links…" in the
  Tools menu lists them all.
- A folder chooses the credential mode of the SSH and SFTP connections inside it (for example,
  all through the agent, without asking for the password); a new connection starts at "Inherit".
- In Settings › General, "AI assistants" and "Control by assistants (MCP)" turn each area off:
  its menus, rows, sections and shortcuts disappear, and what was configured is kept.
- A connection and a folder choose the mouse in the terminal: Copy and paste (selecting copies, the right
  button copies or pastes) or Menu; "Inherit" follows the folder and, with no folder choosing,
  Settings › Terminal.
- A CPU meter in the title bar (squares from light yellow to dark red) and the Performance page,
  opened by clicking it or through View ▸ Performance: the app's CPU, memory and network over the
  last minute and the "Per terminal" table, with the weight of each tab and of the local shell's
  program. The meter turns off in Settings › General.
- Automatic reconnection doesn't give up: when the attempts run out, the tab waits and starts
  another round, until the connection is back ("Trying again in 5 min"). The time is in
  Settings › Connections › Retry after (5 min by default; 0 turns it off), and a connection and a
  folder choose another one, or Off; "Inherit" follows the folder and, with no folder choosing,
  Settings.
- With more than one window open, the main one is marked: the title "Tirech Omni — Main
  window", the subtitle "Main window" in the title bar and "Window 1 (main)" in the menus that
  list the windows.
- "Control with ▸ OpenCode", next to Claude Code and Codex, and the OpenCode tab in "How to
  connect". Each MCP server gets its own time limit (OpenCode's `timeout`, Codex's
  `tool_timeout_sec`), and in Codex and OpenCode the tab's request arrives through the MCP
  server's instructions (`tirech-omni mcp --instrucoes`).
- The `travamentos.log`, in the settings folder: when the interface is stuck for more than 2 s,
  which stage it was in, the state of the thread and of the system, and the threads that used
  the most CPU; for each heavy minute, the time per stage.

### Changed

- The read-only copy sends to the owner what changes in the connections, the settings, the AI
  providers and the vault, and the owner saves it. If the owner changed the same file at the
  same time, both changes are merged; the same field changed on both sides keeps the owner's,
  with a warning.

- New windows have the Connections and Properties panels, pinned as in the main window. The
  selection, the filter and the open folders are the same in every window; each window's View
  menu shows and hides its own panels, the layout comes back on the next run, and deleting,
  editing tunnels and commands, running and the server sessions ask in the window that was
  clicked.
- A second copy of the app opened with the same folder is read-only, as in WinSSHTerm: a bar
  says so, it doesn't write the files on its own, and the Restart button opens it again as the
  owner once the first one closes.
- Connections changed by another program (another copy of the app, an editor, sync) are reread
  on their own.
- A second copy of the app no longer brings up its own built-in VPN: it asks the owner for the
  network through the channel between copies and goes out through the owner's engine bridge,
  which stays up while the second copy uses it. With no owner running, the second copy brings the
  engine up itself.

### Fixed

- A program that dumps output nonstop in a terminal (a `yes`, a `cat` of a huge file) no longer
  hangs the app or fills the memory: each terminal's queue is capped at 1 MiB, the connection
  waits for the queue to drain, Ctrl+C and the other channels keep getting through, and the app
  wakes once per batch instead of once per chunk.
- The tray icon shows on Cinnamon and MATE, which ignored the image and showed an empty slot.
- The vault and the settings no longer get corrupted by two copies of the app writing at the
  same time: each write uses its own temporary file, an unreadable file is kept as `.bak`
  without overwriting an earlier copy, a file from a newer version is not overwritten, and the
  fields this version doesn't know stay in the file.
- The `copias/` folder keeps one copy per day of the connections and the vault, for the last 7
  days.
- With many keys in the agent, the SSH connection was dropped before reaching the right key
  ("the agent refused to use the key"): the connection's key goes first.
- A profile's built-in VPN dropped now and then when the server accepts only one connection per
  user: an engine still used by tabs was forgotten when stopped and another came up with the same
  identity, and the two knocked each other off. Now the leftover engine is reused or closed, and
  the profile's lock is only released when it actually stops.

## [0.1.0] - 2026-10-26

First public release.

- SSH, Telnet, serial and local shell connections, in a tree, with terminals in tabs, in grids
  and across multiple windows.
- Encrypted password vault, notes with attachments, SSH keys and agent.
- Remote files (SFTP, FTP and FTPS, S3 and rclone).
- Client networks with built-in OpenVPN, WireGuard, Tailscale, Nebula and ZeroTier.
- Control of the app by AI assistants (MCP and "Control with"), assistants on the server and the
  app's own AI.
- Import from OpenSSH, WinSSHTerm, PuTTY, KiTTY, WinSCP, FileZilla, mRemoteNG and MobaXterm.
- Interface in Portuguese, English and Spanish; portable Windows version and ARM packages.

[0.1.0]: https://github.com/TirechCorp/tirech-omni/releases/tag/v0.1.0
