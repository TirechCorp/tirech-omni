# Tirech Omni

[Português](../pt-BR/README.md) · **English**

A connection manager for SSH, local shell, Telnet and serial, with tabbed and grid terminals, a
password vault, SSH keys and agent, remote files and built-in VPNs that run without admin
rights. It works on Linux, Windows and macOS, with the Windows 11 look, and can be operated by
AI assistants such as Claude Code, Codex and OpenCode.

<p align="center">
  <img src="../capturas/terminais-escuro.png" alt="The connection tree and a terminal, in the dark theme" width="820">
</p>

## Features

- **Connections** in a tree with folders, property inheritance, search (Ctrl+E), pinned and
  recent items. SSH with jump host, proxy, agent, X11, `-L`, `-R` and `-D` tunnels and
  keepalive; Telnet, serial and the local shell.
- **Terminals** in tabs and grids (1, 2 × 2, 3 × 3), across multiple windows, with typing sent
  to several terminals at once, persistent sessions in tmux or screen, logging, commands on
  connect and command autocomplete (Ctrl+Space).
- **Encrypted password vault**, with a generator, an optional master password, Markdown notes
  with attachments and copying to another machine in a single file. Imports and exports the
  Bitwarden format and reads KeePassXC CSV.
- **SSH keys and agent**: generate, import and convert keys, the app's own agent for the
  system's `ssh` and, on Windows, Pageant for PuTTY, WinSCP and FileZilla.
- **Files** in two panes: SFTP, FTP and FTPS, S3 and compatibles, and other clouds through
  `rclone`, with a transfer queue and editing of remote files.
- **Client networks**: built-in OpenVPN, WireGuard, Tailscale, Nebula and ZeroTier, per folder
  or per connection. Only the app's traffic goes through the VPN, with no virtual network
  adapter and no admin rights, and two clients with the same IP can coexist; Nebula goes through the network's relays behind
  CGNAT. Links in the tree open in a browser for that client only, going out through its
  network, and the local shell goes out through it too.
- **AI assistants**: an assistant on your computer operates the app over MCP (opens
  connections, reads the screen, types, runs commands), with per-connection permissions,
  approval and auditing. "Control with" opens Claude Code, Codex or OpenCode in a tab tied to a
  session. The app also installs and opens these assistants on the server, and has its own AI
  that suggests commands.
- **Import** from `~/.ssh/config`, WinSSHTerm, PuTTY and KiTTY, WinSCP, FileZilla, mRemoteNG and
  MobaXterm, with a preview.
- **Server ports**: what is listening on the server, with one click to forward the port or open
  the page in the browser through the SSH channel.
- **Performance**: a CPU meter in the title bar and a page like the Task Manager's, with the
  weight of each terminal, and the processes and disk of this computer or of the SSH server,
  with filter, tree and end process.
- Interface in **Portuguese, English and Spanish**.
- **Portable** version for Windows, **ARM** packages, system tray and per-user installation
  without admin rights.

<p align="center">
  <img src="../capturas/grade-com-ia-claro.png" alt="A grid with four sessions and an AI assistant" width="400">
  <img src="../capturas/cofre-claro.png" alt="The password vault" width="400">
</p>
<p align="center">
  <img src="../capturas/redes-claro.png" alt="The Networks page with a WireGuard network" width="400">
</p>

The [manual](manual.md) describes each feature, the keys, the settings and
where the app stores each file.

## Download

The packages are on the [Releases](https://github.com/TirechCorp/tirech-omni/releases) page:

| Package | For |
| --- | --- |
| [`tirech-omni-portable-windows.zip`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-portable-windows.zip) | Windows 10 and 11 x64, portable: unzip it and run `tirech-omni.exe` |
| [`tirech-omni-portable-windows-arm64.zip`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-portable-windows-arm64.zip) | Windows 11 ARM, portable |
| [`tirech-omni-linux-x64.tar.gz`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-x64.tar.gz) | Linux x64 (glibc 2.35 or newer: Ubuntu 22.04, Debian 12 and similar) |
| [`tirech-omni-linux-arm64.tar.gz`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-arm64.tar.gz) | 64-bit Linux ARM (Raspberry Pi 4 and 5 with a 64-bit OS, and similar) |
| [`tirech-omni-linux-armv7.tar.gz`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-armv7.tar.gz) | 32-bit Linux ARM (armhf) |
| [`tirech-omni-macos.dmg`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-macos.dmg) | macOS 11 or newer, Apple Silicon and Intel (universal) |

Each package includes the app, the command-line tools (`tirech-omni-keygen`,
`tirech-omni-agent`, `tirech-omni-add` and `tirech-omni-copy-id`) and
`THIRD-PARTY-NOTICES.txt`; on macOS, the tools live inside the `.app`, in `Contents/MacOS`.
The release's `SHA256SUMS.txt` verifies the downloads.

## Install

### Windows

1. Download `tirech-omni-portable-windows.zip` (or the `-arm64` one, on Windows ARM) and unzip it.
2. Open `tirech-omni.exe`, inside the `Tirech Omni` folder. The app is not signed: if
   SmartScreen warns you, click **More info ▸ Run anyway**.

This runs it **portable**: everything it saves goes to the `tirech-omnidata` folder next to the
`.exe`, and nothing is written to the computer. You can carry the folder on a USB stick.

To **install** it, delete (or move away) the `tirech-omnidata` folder, open `tirech-omni.exe`
and use File ▸ **Install on this computer…**. The app copies itself to
`%LOCALAPPDATA%\Programs\Tirech Omni`, with the command-line tools, and creates the shortcuts
you check (desktop, Start menu, start with the system, show in Installed apps). It does not need
admin rights. Your data goes to `%APPDATA%\tirech-omni`.

### Linux

```sh
# replace x64 with arm64 or armv7, to match the machine
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-x64.tar.gz
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/SHA256SUMS.txt
sha256sum -c SHA256SUMS.txt --ignore-missing
tar -xzf tirech-omni-linux-x64.tar.gz
./tirech-omni-linux-x64/tirech-omni
```

With the app open, File ▸ **Install on this computer…** copies the binaries to `~/.local/bin`
and creates the applications menu shortcut, the desktop shortcut, the panel pin (on Cinnamon,
GNOME and KDE) and the start with the system, as you check them. No `sudo`. To install by hand:

```sh
install -m 755 tirech-omni-linux-x64/tirech-omni* ~/.local/bin/
```

`~/.local/bin` must be on the `PATH` for the command-line tools (`tirech-omni-keygen` and the
others); common distributions already put it there. Your data goes to `~/.config/tirech-omni`.
On GNOME, the tray icon needs the AppIndicator extension.

### macOS

1. Download `tirech-omni-macos.dmg`, open it and drag **Tirech Omni** to **Applications**.
2. Open the app. It is not signed with an Apple developer account, so the first time macOS warns
   that it could not verify it: close the warning, go to **System Settings ▸ Privacy &
   Security**, scroll to the bottom and click **Open Anyway**. From then on, it opens directly.

From the Terminal, without the warning (`curl` does not mark the file as downloaded from the
internet):

```sh
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-macos.dmg
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/SHA256SUMS.txt
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
hdiutil attach tirech-omni-macos.dmg
cp -R "/Volumes/Tirech Omni/Tirech Omni.app" /Applications/
hdiutil detach "/Volumes/Tirech Omni"
```

File ▸ **Install on this computer…** copies the app to `~/Applications` and creates the
shortcuts you check: desktop, Dock and start with the system. The command-line tools live inside
the `.app`; to use them in the Terminal:

```sh
echo 'export PATH="/Applications/Tirech Omni.app/Contents/MacOS:$PATH"' >> ~/.zprofile
```

Your data goes to `~/Library/Application Support/tirech-omni`.

### Update and uninstall

- **Update:** download the new package, open its app and use File ▸ **Install on this
  computer…** again: the installed copy, if it differs, is replaced. For the portable version,
  replace the `.exe` files and keep the `tirech-omnidata` folder. On macOS, drag the new app over
  the old one in Applications. Your data and connections stay.
- **Uninstall:** File ▸ **Install on this computer…** ▸ **Uninstall…** removes the copy and the
  shortcuts (on Windows, the Uninstall button in Installed apps does it too). Your data is only
  removed if you check the box to delete it. The portable version goes away with its folder.

## Building

The app depends on [Tomni UI](https://github.com/TirechCorp/tomni-ui) by path
(`../tomni-ui`): clone both side by side.

```sh
git clone https://github.com/TirechCorp/tomni-ui
git clone https://github.com/TirechCorp/tirech-omni
cd tirech-omni
cargo run --release -p tirech-omni
```

The Rust version is pinned in `rust-toolchain.toml` (requires `rustup`); the minimum supported
version is 1.89.

The workspace has two packages:

| Package | What |
| --- | --- |
| `tirech-omni` | the app |
| `tirech-omni-tools` | `tirech-omni-keygen`, `tirech-omni-agent`, `tirech-omni-add` and `tirech-omni-copy-id`: `ssh-keygen`, `ssh-agent`, `ssh-add` and `ssh-copy-id` on the command line, with the app's keys |

### Packages

| Script | Produces |
| --- | --- |
| `empacotar-portable.sh` | the Windows portable zip, from `cargo build --release --target x86_64-pc-windows-gnu -p tirech-omni -p tirech-omni-tools` |
| `empacotar-arm.sh` | the 64- and 32-bit Linux ARM packages and the Windows ARM portable, built through Docker on an x86_64 computer |
| `empacotar-mac.sh` | the macOS `.dmg`, universal (Apple Silicon and Intel), built on a Mac with Apple's command-line tools; signs ad hoc, or with the Developer ID in `IDENTIDADE` (and notarizes with the profile in `PERFIL_NOTARIZACAO`) |

## Where the app stores its data

In the system configuration folder (`~/.config/tirech-omni` on Linux), in the folder pointed to
by the `TIRECH_OMNI_CONFIG` variable or, in the portable version, in the `tirech-omnidata`
folder next to the executable. The list of files is in the
[manual](manual.md#where-the-files-are).

## Security

The app saves passwords, as connection managers usually do, and keeps them encrypted:

- The vault (`vault.json`) is encrypted with AES-256-GCM. Without a master password, the key is
  stored in `vault.key`, next to it, and the vault opens on its own; with a master password, the
  key is derived from it with Argon2id and is not stored. On Linux and macOS, the vault is
  written with 0600 permissions.
- Text sent to an AI first goes through secret redaction.
- MCP talks over a Unix socket owned by the user (or an owned pipe, on Windows), and the VPNs'
  SOCKS5 bridge listens only on `127.0.0.1`, with a random port and random credentials.

Protecting the machine and deciding on a master password is up to the user: the app doesn't
add barriers such as automatic locking or extra confirmations. To report a vulnerability, see
[`SECURITY.md`](SECURITY.md).

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md) (the CI checks, the layout and the rules) and the
[code of conduct](CODE_OF_CONDUCT.md). New widgets and protocols usually belong in
[Tomni UI](https://github.com/TirechCorp/tomni-ui), not in the app. What is planned is in [`ROADMAP.md`](ROADMAP.md).

## License

MIT or Apache-2.0, at your option ([`LICENSE-MIT`](../../LICENSE-MIT),
[`LICENSE-APACHE`](../../LICENSE-APACHE)). Dependencies are restricted to permissive licenses
(`cargo deny check licenses`).
