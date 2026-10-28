# Mudanças

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e as versões, o
[SemVer](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado

- Links: uma conexão do tipo Link abre um endereço no navegador. Com um perfil de rede, abre num
  navegador só daquele perfil, que sai pela rede do cliente (proxy ou VPN embutida) por um proxy
  HTTP local; sem perfil, no navegador padrão. "Links…" no menu Ferramentas lista todos.
- A pasta escolhe o modo da credencial das conexões SSH e SFTP de dentro (por exemplo, todas pelo
  agente, sem pedir a senha); a conexão nova começa em "Herdar".
- Em Configurações › Geral, "Assistentes de IA" e "Controle por assistentes (MCP)" desligam cada
  área: os menus, as linhas, as seções e os atalhos dela somem, e o que estava configurado fica.
- A conexão e a pasta escolhem o mouse no terminal: Copiar e colar (selecionar copia, o botão direito
  copia ou cola) ou Menu; "Herdar" segue a pasta e, sem pasta que escolha, Configurações › Terminal.
- Medidor de CPU na barra de título (quadradinhos do amarelo-claro ao vermelho-escuro) e a página
  Desempenho, aberta pelo clique nele ou por Exibir ▸ Desempenho: CPU, memória e rede do app no
  último minuto e a tabela "Por terminal", com o peso de cada aba e do programa do shell local. O
  medidor desliga em Configurações › Geral.
- A reconexão automática não desiste: acabadas as tentativas, a aba espera e começa outra rodada,
  até a conexão voltar ("Tento de novo em 5 min"). O tempo fica em Configurações › Conexões ›
  Tentar de novo após (5 min por padrão; 0 desliga), e a conexão e a pasta escolhem outro, ou
  Desligado; "Herdar" segue a pasta e, sem pasta que escolha, as Configurações.
- Com mais de uma janela aberta, a principal se marca: o título "Tirech Omni — Janela
  principal", o subtítulo "Janela principal" na barra de título e "Janela 1 (principal)" nos
  menus que listam as janelas.
- "Controlar com ▸ OpenCode", ao lado do Claude Code e do Codex, e a aba OpenCode em "Como
  ligar". Cada servidor MCP ganha um prazo próprio (o `timeout` do OpenCode, o
  `tool_timeout_sec` do Codex), e no Codex e no OpenCode o pedido da aba chega pelas
  instruções do servidor MCP (`tirech-omni mcp --instrucoes`).
- O `travamentos.log`, na pasta de configuração: quando a interface fica parada mais de 2 s,
  em que etapa estava, o estado da thread e do sistema e as threads que mais gastaram CPU; a
  cada minuto pesado, o tempo por etapa.

### Mudado

- A cópia só de leitura manda para a dona o que muda nas conexões, nas configurações, nos
  provedores de IA e no cofre, e a dona grava. Se a dona mudou o mesmo arquivo ao mesmo tempo, as
  duas mudanças se juntam; o mesmo campo dos dois lados fica com o da dona, com aviso.

- As janelas novas têm os painéis Conexões e Propriedades, fixados como na principal. A seleção,
  o filtro e as pastas abertas são os mesmos em todas as janelas; o Exibir de cada uma mostra e
  esconde os painéis dela, o arranjo volta na próxima execução, e apagar, editar túneis e
  comandos, executar e as sessões do servidor perguntam na janela em que se clicou.
- Uma segunda cópia do app aberta com a mesma pasta é só de leitura, como no WinSSHTerm: uma
  barra avisa, ela não grava os arquivos por conta própria, e o botão Reiniciar a abre de novo
  como dona quando a primeira fecha.
- As conexões mudadas por outro programa (outra cópia do app, um editor, a sincronização) são
  relidas sozinhas.
- Uma segunda cópia do app não liga mais a sua própria VPN embutida: ela pede a rede à dona pelo
  canal entre as cópias e sai pela ponte do motor da dona, que fica ligado enquanto a segunda usa.
  Sem a dona no ar, a segunda liga o motor ela mesma.

### Corrigido

- Um programa que despeja saída sem parar num terminal (um `yes`, um `cat` de um arquivo
  enorme) não pendura mais o app nem enche a memória: a fila de cada terminal tem limite de
  1 MiB, a conexão espera a fila esvaziar, o Ctrl+C e os outros canais continuam passando, e o
  app acorda uma vez por lote em vez de uma vez por pedaço.
- O ícone da bandeja aparece no Cinnamon e no MATE, que ignoravam a imagem e mostravam uma vaga
  vazia.
- O cofre e a configuração não se corrompem mais com duas cópias do app gravando ao mesmo tempo:
  cada gravação usa um arquivo temporário próprio, um arquivo ilegível é guardado como `.bak`
  sem pisar numa cópia de antes, um arquivo de uma versão mais nova não é sobrescrito, e os
  campos que esta versão não conhece continuam no arquivo.
- A pasta `copias/` guarda uma cópia por dia das conexões e do cofre, dos últimos 7 dias.
- Com muitas chaves no agente, a conexão SSH era derrubada antes de chegar à chave certa ("o
  agente recusou usar a chave"): a chave da conexão vai primeiro.
- A VPN embutida de um perfil caía de tempos em tempos quando o servidor só aceita uma conexão
  por usuário: um motor que ainda tinha abas usando era esquecido ao parar e outro subia com a
  mesma identidade, e os dois se derrubavam. Agora o motor que sobra é reaproveitado ou fechado,
  e a trava do perfil só solta quando ele para de fato.

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
