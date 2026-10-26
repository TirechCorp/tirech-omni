# Contributing to Tirech Omni

Thanks for your interest. The app is built with [Tomni UI](https://github.com/TirechCorp/tomni-ui),
which lives in the sibling repository and is pulled in by path (`../tomni-ui/crates/...` in the
root `Cargo.toml`). Clone both into the same folder. For writing screens, the library's guide is
`../tomni-ui/crates/cli/templates/AGENTS.md`, and the `llms.txt` there is the short API
reference.

Before a large pull request (a new page, a new file format, a new dependency), open an issue to
agree on the design. Small fixes can go straight in. A missing widget or protocol goes into the
library, following its rules, not into the app.

## Language

Code documentation, comments, error messages and test names are in Brazilian Portuguese, with
accents, and so is the source text of the interface. Library identifiers stay in English; the
app's may be in Portuguese, as they already are. Don't replace accented characters with ASCII.
The public documentation exists in Portuguese (`docs/pt-BR`) and in English (`docs/en`): if you
change one, update the other too, or ask for help in the pull request. Issues and pull requests
can be in English or Portuguese.

Screen text goes through `t!("chave", nome = valor)` (or `tk!("chave")`), with the key in the
`.ftl` files of every language (`crates/tirech-omni/i18n/`; `pt-BR` is the source). The tests in
`tests/idiomas.rs` flag missing keys, unused keys, arguments that differ between languages and
hard-coded text on screen. Widget ids and saved files are not translated.

## Before opening a pull request

All of this must pass (it's what CI runs), from the root of this repository:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check licenses
TIRECH_OMNI_CONFIG="$(mktemp -d)" XDG_DATA_HOME="$(mktemp -d)" \
  cargo run --manifest-path ../tomni-ui/Cargo.toml -p tomni-ui-cli -- lint -p tirech-omni
```

`lint` runs the app with the library's tools, in both themes. On startup, the app writes the
menu shortcut and the icons to `$XDG_DATA_HOME`; the temporary folder avoids leaving in your
home a `.desktop` file that points to the debug binary.

The Rust version is pinned in `rust-toolchain.toml`, the same as the library's (the snapshots
depend on it); the minimum is 1.89 (`cargo +1.89 check --workspace --all-targets`).

Changed the library? Run the checks from its `CONTRIBUTING.md` too.

### Optional tests, outside CI

The recipes in `crates/tirech-omni/redes/` need Docker. `demo/` starts two containers with the
same IP, without a VPN, and checks the whole idea (on ports 1081 and 1082 of your own computer):

```sh
cd crates/tirech-omni/redes/demo && docker compose up -d --build --wait
ssh -o ProxyCommand='nc -X 5 -x 127.0.0.1:1081 %h %p' demo@10.99.0.5 hostname   # cliente-a
docker compose down
```

The real-server matrices for FTP, S3, rclone and the VPNs live in the library
(`../tomni-ui/testes/`).

## Layout

| Folder | What |
| --- | --- |
| `crates/tirech-omni/src` | the app: the connection model (`modelo.rs`), storage (`armazenamento.rs`), the vault (`cofre.rs`) and the encrypted attachments (`anexos.rs`), notes (`notas*.rs`, `tela_notas*`), import (`importar/`) and export (`exportar/`), keys and agent (`chaves_cofre.rs`, `agente.rs`), the app's AI (`assistente/`, `redacao.rs`), control by assistants (`mcp/`, `tela/mcp*.rs`, `tela/controlar.rs`), assistants on the server (`ia.rs`, `tela/ia.rs`), what gets written to the system (`sistema.rs`), the screens (`tela*.rs`, `inicio.rs`), files, networks and settings |
| `crates/tirech-omni/tests` | the integration and visual tests |
| `crates/tirech-omni/tests/comum` | the test servers: SSH with SFTP, in-memory S3 and the fake `rclone rcd`, copies of the library's |
| `crates/tirech-omni/i18n` | the Project Fluent `.ftl` files, one folder per language, one file per area |
| `crates/tirech-omni/snapshots` | the references for the visual tests |
| `crates/tirech-omni/redes` | docs and Docker recipes for client networks with the same IP |
| `crates/tirech-omni-tools` | `tirech-omni-keygen`, `-agent`, `-add` and `-copy-id` on the command line |
| `empacotar/` | the Docker recipes for `empacotar-arm.sh` |

## Rules

- **Behavior:** the app saves passwords (encrypted, in `vault.json`), and that is a
  requirement. Don't cut a feature out of caution or add usage barriers (extra confirmations,
  asking for the password again, a feature turned off by default). Keeping the machine secure
  is the user's choice.
- **Visual reference:** WinUI 3, through the library.
- **Saved formats:** the app's files (`vault.json`, `connections.json`, the export fields) have
  old identifiers, such as `tirech-ssh-cofre`, that can't change: they are read from existing
  installations.
- **Snapshots:** only approve (`TOMNI_UI_UPDATE_SNAPSHOTS=1`) after opening the `.snap.new.png`
  and the `.snap.diff.png` and agreeing with the change. The references come from the
  deterministic CPU backend.
- **Reactivity:** code that reads signals outside an effect (events, focus) uses `untrack`.
- **Tests without network:** a local `russh` server, the servers in `tests/comum`, `sh` and the
  configuration folder in a temporary directory (`TIRECH_OMNI_CONFIG`). No `set_var` (it's
  `unsafe` in edition 2024): pass the path through the builder or the child process.
- No `unsafe` (the workspace lint denies it).
- **Dependencies:** only licenses on the `deny.toml` list, and in the versions the library's
  `Cargo.lock` already has, when there are any.

## Tools

To look at the app with the library's tools (the outputs go to `target/tomni-ui/`):

```sh
cargo run --manifest-path ../tomni-ui/Cargo.toml -p tomni-ui-cli -- screenshot -p tirech-omni --theme dark
cargo run --manifest-path ../tomni-ui/Cargo.toml -p tomni-ui-cli -- tree -p tirech-omni
```

## Licensing of contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in the project, as defined in the Apache-2.0 license, shall be dual licensed as MIT or
Apache-2.0, at the user's option, without any additional terms or conditions.
