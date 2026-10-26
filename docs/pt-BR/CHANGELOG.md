# Mudanças

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e as versões, o
[SemVer](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Mudado

- As janelas novas têm os painéis Conexões e Propriedades, fixados como na principal. A seleção,
  o filtro e as pastas abertas são os mesmos em todas as janelas; o Exibir de cada uma mostra e
  esconde os painéis dela, o arranjo volta na próxima execução, e apagar, editar túneis e
  comandos, executar e as sessões do servidor perguntam na janela em que se clicou.
- Uma segunda cópia do app aberta com a mesma pasta é só de leitura, como no WinSSHTerm: uma
  barra avisa, nada da configuração nem do cofre é gravado por ela, e o botão Reiniciar a abre
  de novo como dona quando a primeira fecha.
- As conexões mudadas por outro programa (outra cópia do app, um editor, a sincronização) são
  relidas sozinhas.

### Corrigido

- O cofre e a configuração não se corrompem mais com duas cópias do app gravando ao mesmo tempo:
  cada gravação usa um arquivo temporário próprio, um arquivo ilegível é guardado como `.bak`
  sem pisar numa cópia de antes, um arquivo de uma versão mais nova não é sobrescrito, e os
  campos que esta versão não conhece continuam no arquivo.
- A pasta `copias/` guarda uma cópia por dia das conexões e do cofre, dos últimos 7 dias.
- Com muitas chaves no agente, a conexão SSH era derrubada antes de chegar à chave certa ("o
  agente recusou usar a chave"): a chave da conexão vai primeiro.

## [0.1.0] - 2026-10-26

Primeira versão pública.

- Conexões SSH, Telnet, serial e shell local, em árvore, com terminais em abas, em grade e em
  várias janelas.
- Cofre de senhas cifrado, notas com anexos, chaves e agente SSH.
- Arquivos remotos (SFTP, FTP e FTPS, S3 e rclone).
- Redes de clientes com OpenVPN, WireGuard, Tailscale, Nebula e ZeroTier embutidos.
- Controle do app por assistentes de IA (MCP e "Controlar com"), assistentes no servidor e a IA
  do próprio app.
- Importação do OpenSSH, WinSSHTerm, PuTTY, KiTTY, WinSCP, FileZilla, mRemoteNG e MobaXterm.
- Interface em português, inglês e espanhol; versão portable do Windows e pacotes para ARM.

[0.1.0]: https://github.com/TirechCorp/tirech-omni/releases/tag/v0.1.0
