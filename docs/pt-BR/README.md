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
  digita para vários terminais, sessão persistente no tmux ou no screen, log, comandos ao
  conectar e autocompletar de comandos (Ctrl+Espaço).
- **Cofre de senhas** cifrado, com gerador, senha mestra opcional, notas em Markdown com anexos
  e cópia para outra máquina num arquivo só. Importa e exporta o formato do Bitwarden e lê o
  CSV do KeePassXC.
- **Chaves e agente SSH**: gerar, importar e converter chaves, o agente do próprio app para o
  `ssh` do sistema e, no Windows, o Pageant para o PuTTY, o WinSCP e o FileZilla.
- **Arquivos** em dois painéis: SFTP, FTP e FTPS, S3 e compatíveis e as demais nuvens pelo
  `rclone`, com fila de transferências e edição de arquivos remotos.
- **Redes de clientes**: OpenVPN, WireGuard, Tailscale, Nebula e ZeroTier embutidos, por pasta
  ou por conexão. Só o tráfego do app passa pela VPN, sem placa virtual e sem administrador, e
  dois clientes com o mesmo IP convivem; a Nebula passa pelos relays da rede atrás de CGNAT.
  Os links da árvore abrem num navegador só do cliente, que sai pela rede dele, e o shell local
  também sai por ela.
- **Assistentes de IA**: um assistente do seu computador opera o app pelo MCP (abre conexões,
  lê a tela, digita, roda comandos), com permissões por conexão, aprovação e auditoria. O
  "Controlar com" abre o Claude Code, o Codex ou o OpenCode numa aba ligada a uma sessão. O app
  também instala e abre esses assistentes no servidor e tem uma IA própria que sugere comandos.
- **Importar** do `~/.ssh/config`, do WinSSHTerm, do PuTTY e do KiTTY, do WinSCP, do FileZilla,
  do mRemoteNG e do MobaXterm, com pré-visualização.
- **Portas do servidor**: o que escuta no servidor, com um clique para encaminhar a porta ou
  abrir a página no navegador pelo canal SSH.
- **Desempenho**: um medidor de CPU na barra de título e uma página como a do Gerenciador de
  Tarefas, com o peso de cada terminal, e os processos e o disco deste computador ou do
  servidor SSH, com filtro, árvore e encerrar.
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
| [`tirech-omni-portable-windows.zip`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-portable-windows.zip) | Windows 10 e 11 x64, portable: descompacte e rode o `tirech-omni.exe` |
| [`tirech-omni-portable-windows-arm64.zip`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-portable-windows-arm64.zip) | Windows 11 ARM, portable |
| [`tirech-omni-linux-x64.tar.gz`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-x64.tar.gz) | Linux x64 (glibc 2.35 ou mais nova: Ubuntu 22.04, Debian 12 e afins) |
| [`tirech-omni-linux-arm64.tar.gz`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-arm64.tar.gz) | Linux ARM de 64 bits (Raspberry Pi 4 e 5 com sistema de 64 bits e afins) |
| [`tirech-omni-linux-armv7.tar.gz`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-armv7.tar.gz) | Linux ARM de 32 bits (armhf) |
| [`tirech-omni-macos.dmg`](https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-macos.dmg) | macOS 11 ou mais novo, Apple Silicon e Intel (universal) |

Cada pacote traz o app, as ferramentas de linha de comando (`tirech-omni-keygen`,
`tirech-omni-agent`, `tirech-omni-add` e `tirech-omni-copy-id`) e o
`THIRD-PARTY-NOTICES.txt`; no macOS, as ferramentas ficam dentro do `.app`, em
`Contents/MacOS`. O `SHA256SUMS.txt` da release confere os downloads.

## Instalar

### Windows

1. Baixe o `tirech-omni-portable-windows.zip` (ou o `-arm64`, no Windows ARM) e descompacte.
2. Abra o `tirech-omni.exe`, dentro da pasta `Tirech Omni`. O app não é assinado: se o
   SmartScreen avisar, clique em **Mais informações ▸ Executar assim mesmo**.

Assim ele roda **portable**: tudo o que grava fica na pasta `tirech-omnidata`, ao lado do
`.exe`, e nada vai para o computador. Dá para levar a pasta num pendrive.

Para **instalar**, apague (ou tire de lá) a pasta `tirech-omnidata`, abra o `tirech-omni.exe` e
use Arquivo ▸ **Instalar no computador…**. O app se copia para
`%LOCALAPPDATA%\Programs\Tirech Omni`, com as ferramentas de linha de comando, e cria os atalhos
que você marcar (área de trabalho, menu Iniciar, abrir com o sistema, aparecer em Aplicativos
instalados). Não pede administrador. Os dados ficam em `%APPDATA%\tirech-omni`.

### Linux

```sh
# troque x64 por arm64 ou armv7, conforme a máquina
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-linux-x64.tar.gz
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/SHA256SUMS.txt
sha256sum -c SHA256SUMS.txt --ignore-missing
tar -xzf tirech-omni-linux-x64.tar.gz
./tirech-omni-linux-x64/tirech-omni
```

Com o app aberto, Arquivo ▸ **Instalar no computador…** copia os binários para `~/.local/bin`
e cria o atalho no menu de aplicativos, na área de trabalho, na barra (no Cinnamon, no GNOME e
no KDE) e o início com o sistema, conforme o que você marcar. Sem `sudo`. Para instalar à mão:

```sh
install -m 755 tirech-omni-linux-x64/tirech-omni* ~/.local/bin/
```

O `~/.local/bin` precisa estar no `PATH` para as ferramentas de linha de comando
(`tirech-omni-keygen` e as outras); as distribuições comuns já o põem lá. Os dados ficam em
`~/.config/tirech-omni`. No GNOME, o ícone da bandeja pede a extensão AppIndicator.

### macOS

1. Baixe o `tirech-omni-macos.dmg`, abra e arraste o **Tirech Omni** para **Aplicativos**.
2. Abra o app. Ele não é assinado com uma conta de desenvolvedor da Apple, e na primeira vez o
   macOS avisa que não pôde verificá-lo: feche o aviso, vá em **Ajustes do Sistema ▸
   Privacidade e Segurança**, role até o fim e clique em **Abrir Mesmo Assim**. Das próximas
   vezes, ele abre direto.

Pelo Terminal, sem o aviso (o `curl` não marca o arquivo como baixado da internet):

```sh
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/tirech-omni-macos.dmg
curl -LO https://github.com/TirechCorp/tirech-omni/releases/latest/download/SHA256SUMS.txt
shasum -a 256 -c SHA256SUMS.txt --ignore-missing
hdiutil attach tirech-omni-macos.dmg
cp -R "/Volumes/Tirech Omni/Tirech Omni.app" /Applications/
hdiutil detach "/Volumes/Tirech Omni"
```

Arquivo ▸ **Instalar no computador…** copia o app para `~/Applications` e cria o atalho na área
de trabalho, no Dock e o início com o sistema, conforme o que você marcar. As ferramentas de
linha de comando ficam dentro do `.app`; para usá-las no Terminal:

```sh
echo 'export PATH="/Applications/Tirech Omni.app/Contents/MacOS:$PATH"' >> ~/.zprofile
```

Os dados ficam em `~/Library/Application Support/tirech-omni`.

### Atualizar e desinstalar

- **Atualizar:** baixe o pacote novo, abra o app dele e use Arquivo ▸ **Instalar no
  computador…** de novo: a cópia instalada, se for diferente, é substituída. Na versão portable,
  troque os `.exe` e mantenha a pasta `tirech-omnidata`. No macOS, arraste o app novo por cima do
  antigo em Aplicativos. Os dados e as conexões ficam.
- **Desinstalar:** Arquivo ▸ **Instalar no computador…** ▸ **Desinstalar…** remove a cópia e os
  atalhos (no Windows, também pelo botão Desinstalar em Aplicativos instalados). Os dados só
  saem se você marcar "Apagar também os meus dados". A versão portable sai apagando a pasta.

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
| `empacotar-mac.sh` | o `.dmg` do macOS, universal (Apple Silicon e Intel), compilado num Mac com as ferramentas de linha de comando da Apple; assina ad-hoc, ou com o Developer ID de `IDENTIDADE` (e notariza com o perfil de `PERFIL_NOTARIZACAO`) |

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
