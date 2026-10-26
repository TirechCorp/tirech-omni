# Security

## Reporting a vulnerability

Don't open a public issue. Use GitHub's private reporting: in the repository's **Security** tab,
**Report a vulnerability** (or
[this link](https://github.com/TirechCorp/tirech-omni/security/advisories/new)). Issues in the
library (terminal, SSH, files, VPNs) can go straight to
[Tomni UI](https://github.com/TirechCorp/tomni-ui/security/advisories/new).

Send whatever you can: the version or commit, the operating system, how to reproduce it and the
impact you see. Don't send real passwords, keys or configuration files. We reply within 7 days
and agree with you on a disclosure date after the fix. Reporters get credit in the advisory, if
they want it.

## Supported versions

The project is still in the 0.x series: fixes ship only in the latest version on `main`.

## The app's model

Tirech Omni saves passwords, as connection managers usually do. That is a product decision, not
a vulnerability. What it promises:

- **Vault**: `vault.json` is encrypted with AES-256-GCM. Without a master password (the
  default), the key is stored in `vault.key`, in the same folder, and protects against anyone
  who reads only `vault.json` (a copy, a backup, a sync tool). With a master password, the key
  is derived from it with Argon2id, is not stored, and `vault.json` alone can't be opened
  without the password. Each note attachment is encrypted with AES-256-GCM and its own key,
  kept in the vault; encrypted logs use a public key whose private key is kept in the vault.
- **Anything running as your user** on the computer can read what the app reads. The app
  doesn't try to defend itself against a malicious program running as the same user.
- **Local ports and channels**:
  - MCP uses a 0600 Unix socket in a 0700 folder, or an owned Windows pipe;
  - the SSH agent uses a socket owned by the user (or the Pageant pipe and window, on Windows);
  - each VPN's SOCKS5 bridge listens only on `127.0.0.1`, on a random port, and requires a
    random 128-bit username and password;
  - `-L` tunnels and forwarded ports open on `127.0.0.1` and are open to any local program, as
    with OpenSSH's `ssh`.
- **AI**: text sent to a model goes through secret redaction, which has limits described in
  the manual ("What redaction catches and what it doesn't"). An assistant connected over MCP
  can only do what the connection's permissions allow, and every request is recorded in the
  audit log.

Of particular interest: reading the vault or the key without the master password when one is
set, access by another user or another machine to any of the channels above, an assistant
going beyond the connection's permissions, and secrets leaking into logs, to the AI or into
unencrypted files.
