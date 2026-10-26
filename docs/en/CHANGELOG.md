# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [SemVer](https://semver.org/).

## [Unreleased]

### Changed

- New windows have the Connections and Properties panels, pinned as in the main window. The
  selection, the filter and the open folders are the same in every window; each window's View
  menu shows and hides its own panels, the layout comes back on the next run, and deleting,
  editing tunnels and commands, running and the server sessions ask in the window that was
  clicked.
- A second copy of the app opened with the same folder is read-only, as in WinSSHTerm: a bar
  says so, it saves nothing of the settings or the vault, and the Restart button opens it again
  as the owner once the first one closes.
- Connections changed by another program (another copy of the app, an editor, sync) are reread
  on their own.

### Fixed

- The vault and the settings no longer get corrupted by two copies of the app writing at the
  same time: each write uses its own temporary file, an unreadable file is kept as `.bak`
  without overwriting an earlier copy, a file from a newer version is not overwritten, and the
  fields this version doesn't know stay in the file.
- The `copias/` folder keeps one copy per day of the connections and the vault, for the last 7
  days.
- With many keys in the agent, the SSH connection was dropped before reaching the right key
  ("the agent refused to use the key"): the connection's key goes first.

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
