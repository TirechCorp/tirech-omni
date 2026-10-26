# Como contribuir com o Tirech Omni

Obrigado pelo interesse. O app é feito com o [Tomni UI](https://github.com/TirechCorp/tomni-ui),
que fica no repositório ao lado e entra por caminho (`../tomni-ui/crates/...` no `Cargo.toml`
da raiz). Clone os dois na mesma pasta. Para escrever telas, o guia da lib é o
`../tomni-ui/crates/cli/templates/AGENTS.md`, e o `llms.txt` de lá é a referência curta da API.

Antes de um pull request grande (uma página nova, um formato de arquivo novo, uma dependência
nova), abra uma issue para combinar o desenho. Correções pequenas podem ir direto. Um widget
ou um protocolo que falta vai para a lib, com as regras dela, e não para o app.

## Idioma

Documentação, comentários, mensagens de erro, nomes de testes e textos da interface em
português do Brasil, com acentos. Identificadores da lib ficam em inglês; os do app podem ser
em português, como já são. Não troque acentos por ASCII. A documentação pública existe em
português (`docs/pt-BR`) e em inglês (`docs/en`): quem muda uma atualiza a outra, ou pede ajuda
no pull request. Issues e pull requests podem ser em português ou em inglês.

Texto de tela vai por `t!("chave", nome = valor)` (ou `tk!("chave")`), com a chave nos `.ftl`
de todos os idiomas (`crates/tirech-omni/i18n/`; o `pt-BR` é a fonte). Os testes de
`tests/idiomas.rs` acusam chave faltando, sem uso, com argumentos diferentes entre idiomas e
texto fixo na tela. Ids de widgets e arquivos gravados não se traduzem.

## Antes de abrir o pull request

Tudo isto precisa passar (é o que o CI roda), a partir da raiz deste repositório:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check licenses
TIRECH_OMNI_CONFIG="$(mktemp -d)" XDG_DATA_HOME="$(mktemp -d)" \
  cargo run --manifest-path ../tomni-ui/Cargo.toml -p tomni-ui-cli -- lint -p tirech-omni
```

O `lint` roda o app com as ferramentas da lib, nos dois temas. O app grava o atalho do menu e
os ícones em `$XDG_DATA_HOME` ao iniciar; a pasta temporária evita deixar no seu home um
`.desktop` que aponta para o binário de debug.

A versão do Rust está fixada em `rust-toolchain.toml`, a mesma da lib (os snapshots dependem
dela); a mínima é a 1.89 (`cargo +1.89 check --workspace --all-targets`).

Mexeu na lib? Rode a bateria do `CONTRIBUTING.md` de lá também.

### Testes opcionais, fora do CI

As receitas de `crates/tirech-omni/redes/` precisam do Docker. O `demo/` sobe dois contêineres
com o mesmo IP, sem VPN, e confere a ideia toda (nas portas 1081 e 1082 do próprio computador):

```sh
cd crates/tirech-omni/redes/demo && docker compose up -d --build --wait
ssh -o ProxyCommand='nc -X 5 -x 127.0.0.1:1081 %h %p' demo@10.99.0.5 hostname   # cliente-a
docker compose down
```

As matrizes reais do FTP, do S3, do rclone e das VPNs ficam na lib (`../tomni-ui/testes/`).

## Estrutura

| Pasta | O quê |
| --- | --- |
| `crates/tirech-omni/src` | o app: o modelo das conexões (`modelo.rs`), a gravação (`armazenamento.rs`), o cofre (`cofre.rs`) e os anexos cifrados (`anexos.rs`), as notas (`notas*.rs`, `tela_notas*`), importar (`importar/`) e exportar (`exportar/`), chaves e agente (`chaves_cofre.rs`, `agente.rs`), a IA do app (`assistente/`, `redacao.rs`), o controle por assistentes (`mcp/`, `tela/mcp*.rs`, `tela/controlar.rs`), os assistentes no servidor (`ia.rs`, `tela/ia.rs`), o que se escreve no sistema (`sistema.rs`), as telas (`tela*.rs`, `inicio.rs`), arquivos, redes e configurações |
| `crates/tirech-omni/tests` | os testes de integração e os visuais |
| `crates/tirech-omni/tests/comum` | os servidores de teste: SSH com SFTP, S3 em memória e o `rclone rcd` falso, cópias dos da lib |
| `crates/tirech-omni/i18n` | os `.ftl` do Project Fluent, uma pasta por idioma, um arquivo por área |
| `crates/tirech-omni/snapshots` | as referências dos testes visuais |
| `crates/tirech-omni/redes` | doc e receitas Docker das redes de clientes com o mesmo IP |
| `crates/tirech-omni-tools` | `tirech-omni-keygen`, `-agent`, `-add` e `-copy-id` em linha de comando |
| `empacotar/` | as receitas Docker do `empacotar-arm.sh` |

## Regras

- **Comportamento:** o app salva as senhas (cifradas, no `vault.json`), e isso é um requisito.
  Não corte um recurso por cautela nem crie barreiras de uso (confirmações a mais, senha pedida
  de novo, recurso desligado por padrão). Manter a máquina segura é escolha de quem usa.
- **Referência do visual:** o WinUI 3, pela lib.
- **Formatos gravados:** os arquivos do app (`vault.json`, `connections.json`, os campos de
  exportação) têm identificadores antigos, como `tirech-ssh-cofre`, que não podem mudar: são
  lidos de instalações existentes.
- **Snapshots:** só aprove (`TOMNI_UI_UPDATE_SNAPSHOTS=1`) depois de abrir o `.snap.new.png` e
  o `.snap.diff.png` e concordar com a mudança. As referências são da CPU determinística.
- **Reatividade:** código que lê signals fora de um efeito (eventos, foco) usa `untrack`.
- **Testes sem rede:** um servidor `russh` local, os servidores de `tests/comum`, o `sh` e a
  pasta de configuração num diretório temporário (`TIRECH_OMNI_CONFIG`). Nada de `set_var`
  (é `unsafe` na edition 2024): passe o caminho pelo builder ou pelo processo filho.
- Sem `unsafe` (o lint do workspace nega).
- **Dependências:** só licenças da lista do `deny.toml`, e nas versões que o `Cargo.lock` da
  lib já tem, quando houver.

## Ferramentas

Para olhar o app com as ferramentas da lib (as saídas vão para `target/tomni-ui/`):

```sh
cargo run --manifest-path ../tomni-ui/Cargo.toml -p tomni-ui-cli -- screenshot -p tirech-omni --theme dark
cargo run --manifest-path ../tomni-ui/Cargo.toml -p tomni-ui-cli -- tree -p tirech-omni
```

## Licença das contribuições

Salvo indicação em contrário, toda contribuição enviada para inclusão no projeto, como
definido na licença Apache-2.0, é licenciada como MIT ou Apache-2.0, à escolha de quem usa, sem
termos ou condições adicionais.
