# Tirech Omni

**Português** · [English](../en/README.md)

Gerenciador de conexões SSH, shell local, Telnet e serial, com terminais em abas e em grade,
cofre de senhas, chaves e agente SSH, arquivos remotos e VPNs embutidas que rodam sem
administrador. Funciona no Linux, no Windows e no macOS, com o visual do Windows 11, e pode ser
operado por assistentes de IA como o Claude Code, o Codex e o OpenCode.

<p align="center">
  <img src="../capturas/terminais-escuro.png" alt="A árvore de conexões e um terminal, no tema escuro" width="820">
</p>

## Recursos

- **Conexões** em árvore com pastas, herança de propriedades, busca (Ctrl+E), fixadas e
  recentes. SSH com salto, proxy, agente, X11, túneis `-L`, `-R` e `-D` e keepalive; Telnet,
  serial e o shell local.
- **Terminais** em abas e em grade (1, 2 × 2, 3 × 3), em várias janelas, com envio do que se
  digita para vários terminais, sessão persistente no tmux ou no screen, log e comandos ao
  conectar.
- **Cofre de senhas** cifrado, com gerador, senha mestra opcional, notas em Markdown com anexos
  e cópia para outra máquina num arquivo só. Importa e exporta o formato do Bitwarden e lê o
  CSV do KeePassXC.
- **Chaves e agente SSH**: gerar, importar e converter chaves, o agente do próprio app para o
  `ssh` do sistema e, no Windows, o Pageant para o PuTTY, o WinSCP e o FileZilla.
- **Arquivos** em dois painéis: SFTP, FTP e FTPS, S3 e compatíveis e as demais nuvens pelo
  `rclone`, com fila de transferências e edição de arquivos remotos.
- **Redes de clientes**: OpenVPN, WireGuard, Tailscale, Nebula e ZeroTier embutidos, por pasta
  ou por conexão. Só o tráfego do app passa pela VPN, sem placa virtual e sem administrador, e
  dois clientes com o mesmo IP convivem.
- **Assistentes de IA**: um assistente do seu computador opera o app pelo MCP (abre conexões,
  lê a tela, digita, roda comandos), com permissões por conexão, aprovação e auditoria. O
  "Controlar com" abre o Claude Code, o Codex ou o OpenCode numa aba ligada a uma sessão. O app
  também instala e abre esses assistentes no servidor e tem uma IA própria que sugere comandos.
- **Importar** do `~/.ssh/config`, do WinSSHTerm, do PuTTY e do KiTTY, do WinSCP, do FileZilla,
  do mRemoteNG e do MobaXterm, com pré-visualização.
- **Portas do servidor**: o que escuta no servidor, com um clique para encaminhar a porta ou
  abrir a página no navegador pelo canal SSH.
- Interface em **português, inglês e espanhol**.
- Versão **portable** para o Windows, pacotes para **ARM**, bandeja do sistema e instalação no
  computador sem administrador.

<p align="center">
  <img src="../capturas/grade-com-ia-claro.png" alt="Grade com quatro sessões e um assistente de IA" width="400">
  <img src="../capturas/cofre-claro.png" alt="O cofre de senhas" width="400">
</p>
<p align="center">
  <img src="../capturas/redes-claro.png" alt="A página Redes com uma rede WireGuard" width="400">
</p>

O [manual](manual.md) descreve cada recurso, as teclas, as configurações e
onde o app grava cada arquivo.

## Baixar

Os pacotes ficam nas [Releases](https://github.com/TirechCorp/tirech-omni/releases):

| Pacote | Para |
| --- | --- |
| `tirech-omni-portable-windows.zip` | Windows 10 e 11 x64, portable: descompacte e rode o `tirech-omni.exe` |
| `tirech-omni-portable-windows-arm64.zip` | Windows 11 ARM, portable |
| `tirech-omni-linux-x64.tar.gz` | Linux x64 (glibc 2.35 ou mais nova: Ubuntu 22.04, Debian 12 e afins) |
| `tirech-omni-linux-arm64.tar.gz` | Linux ARM de 64 bits (Raspberry Pi 4 e 5 com sistema de 64 bits e afins) |
| `tirech-omni-linux-armv7.tar.gz` | Linux ARM de 32 bits (armhf) |

Cada pacote traz o app, as ferramentas de linha de comando (`tirech-omni-keygen`,
`tirech-omni-agent`, `tirech-omni-add` e `tirech-omni-copy-id`) e o
`THIRD-PARTY-NOTICES.txt`. O `SHA256SUMS.txt` da release confere os downloads
(`sha256sum -c SHA256SUMS.txt --ignore-missing`). Para o macOS, por enquanto, compile a partir
do código.

## Compilar

O app depende do [Tomni UI](https://github.com/TirechCorp/tomni-ui) por caminho
(`../tomni-ui`): clone os dois lado a lado.

```sh
git clone https://github.com/TirechCorp/tomni-ui
git clone https://github.com/TirechCorp/tirech-omni
cd tirech-omni
cargo run --release -p tirech-omni
```

A versão do Rust está fixada em `rust-toolchain.toml` (pede o `rustup`); a mínima suportada é
a 1.89.

O workspace tem dois pacotes:

| Pacote | O quê |
| --- | --- |
| `tirech-omni` | o app |
| `tirech-omni-tools` | `tirech-omni-keygen`, `tirech-omni-agent`, `tirech-omni-add` e `tirech-omni-copy-id`: o `ssh-keygen`, o `ssh-agent`, o `ssh-add` e o `ssh-copy-id` em linha de comando, com as chaves do app |

### Pacotes

| Script | Gera |
| --- | --- |
| `empacotar-portable.sh` | o zip portable do Windows, a partir de `cargo build --release --target x86_64-pc-windows-gnu -p tirech-omni -p tirech-omni-tools` |
| `empacotar-arm.sh` | os pacotes do Linux ARM de 64 e 32 bits e o portable do Windows ARM, compilados pelo Docker num computador x86_64 |

## Onde o app grava

Na pasta de configuração do sistema (`~/.config/tirech-omni` no Linux), na pasta que a variável
`TIRECH_OMNI_CONFIG` apontar ou, na versão portable, na pasta `tirech-omnidata` ao lado do
executável. A lista dos arquivos está no
[manual](manual.md#onde-ficam-os-arquivos).

## Segurança

O app salva as senhas, como os gerenciadores de conexões costumam fazer, e as guarda cifradas:

- O cofre (`vault.json`) é cifrado com AES-256-GCM. Sem senha mestra, a chave fica no
  `vault.key`, ao lado, e o cofre abre sozinho; com a senha mestra, a chave sai dela pelo
  Argon2id e não fica gravada. No Linux e no macOS, o cofre é gravado com permissão 0600.
- O texto que vai para uma IA passa antes por uma redação de segredos.
- O MCP fala por um socket Unix só do usuário (ou um pipe com dono, no Windows), e a ponte
  SOCKS5 das VPNs escuta só em `127.0.0.1`, com porta e credencial aleatórias.

Proteger a máquina e decidir pela senha mestra fica com quem usa: o app não cria barreiras como
bloqueio automático ou confirmações extras. Para relatar uma falha, veja o
[`SECURITY.md`](SECURITY.md).

## Contribuir

Leia o [`CONTRIBUTING.md`](CONTRIBUTING.md) (a bateria do CI, a estrutura e as regras) e o
[código de conduta](CODE_OF_CONDUCT.md). Widgets e protocolos novos costumam ir para o
[Tomni UI](https://github.com/TirechCorp/tomni-ui), não para o app. O que está planejado fica em [`ROADMAP.md`](ROADMAP.md).

## Licença

MIT ou Apache-2.0, à sua escolha ([`LICENSE-MIT`](../../LICENSE-MIT),
[`LICENSE-APACHE`](../../LICENSE-APACHE)). As dependências ficam restritas a licenças permissivas
(`cargo deny check licenses`).
