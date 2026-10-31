# Mudanças

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e as versões, o
[SemVer](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado

- As abas Processos e Disco na página Desempenho, para este computador ou para o servidor de uma
  conexão SSH aberta (Linux): todos os processos com a CPU, a memória, o estado, as threads e o
  comando, ou a leitura e a escrita no disco de cada um; filtro, ordem por coluna, a árvore de
  processos e os botões Encerrar e Forçar encerramento, também no menu do botão direito. Em
  cima, os gráficos do último minuto, como na aba Este app: a CPU e a memória da máquina, ou a
  leitura e a escrita no disco.
- Pacote para o macOS: o `tirech-omni-macos.dmg`, universal (Apple Silicon e Intel), com o
  `Tirech Omni.app` e as ferramentas de linha de comando dentro dele, gerado pelo
  `empacotar-mac.sh` num Mac.
- Autocompletar no terminal (Ctrl+Espaço): uma lista com os comandos que cabem na aba e de onde
  vem cada um. No shell local, os comandos prontos para os túneis abertos, já com a porta de
  verdade (o `xfreerdp` para um túnel que leva à 3389, o `curl` pelo SOCKS5 de um túnel dinâmico
  e outros pela porta de destino), e o histórico do bash, do zsh e do fish; em toda aba, os
  comandos já usados na conexão, os da barra de envio e os comandos ao conectar. Enter põe o
  comando na linha do terminal, sem executar.
- A rede Nebula passa pelos relays da rede quando o punch não passa, como atrás de CGNAT (o comum
  nas operadoras brasileiras) ou de NAT simétrico: o servidor anuncia os relays dele em
  `relay.relays`, e o app pede o relay e passa por ele, sem nada para ligar.
- O shell local sai pela rede do perfil (herdado da pasta ou escolhido na linha Rede): abre com
  `ALL_PROXY`, `HTTP_PROXY` e `HTTPS_PROXY` apontando para o proxy local do perfil, e aceita
  túneis locais e dinâmicos que levam pela rede do cliente (por exemplo, `127.0.0.1:13389` →
  `10.0.0.5:3389` para o `xfreerdp`). A primeira linha do terminal diz por qual perfil o shell sai.
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
  último minuto e a tabela "Por terminal", com o peso de cada aba e do programa do shell local,
  no Linux, no Windows e no macOS. O medidor desliga em Configurações › Geral.
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
- A conexão e a pasta escolhem se um assistente pode abrir abas novas pelo MCP: Permitir,
  Perguntar ou Bloquear, com "Herdar" e o padrão em Configurações › Controle por assistentes
  (MCP). Ao aprovar uma aba nova, dá para deixar a sessão abrir as próximas sem perguntar, pelo
  tempo de "Abrir sem perguntar depois de aprovar" (1 hora por padrão).
- Fechar a aba sem uso: a aba de terminal que passa um tempo sem nada digitado, colado ou mandado
  por um assistente fecha sozinha (a sessão do tmux ou do screen fica no servidor). O tempo fica
  em Configurações › Conexões › Fechar a aba sem uso após (desligado por padrão), e a conexão e
  a pasta escolhem outro, ou Desligado, com "Herdar".
- O aviso "Arquivo baixado" tem o botão "Abrir a pasta", que abre a pasta onde o arquivo foi
  gravado.
- A pasta escolhe a chave do cofre e o arquivo da chave privada das conexões SSH e SFTP de dentro
  que não têm chave própria; na conexão, a linha "Chave do cofre" mostra "Herdar (pasta)".
- A navegação completa em todas as janelas: cada janela aberta com Janela > Nova janela (ou com
  uma aba arrastada para fora) tem a barra lateral, a busca e o medidor, e mostra a sua própria
  página. O que se abre numa janela (Cofre, Sobre, Configurações…) abre nela. Arquivos, Logs e
  Scripts ficam numa janela por vez, a última que os usou; as outras mostram um aviso.

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

- A senha não entrava nos servidores que só aceitam o `keyboard-interactive` (o FreeBSD e o
  pfSense de fábrica, o OpenSSH com `PasswordAuthentication no`): agora entra. Um código de dois
  fatores pedido depois da senha aparece no erro.
- No macOS, o app pede a permissão da Rede Local com uma explicação, e o "servidor
  inalcançável" diz onde liberá-la (Ajustes do Sistema > Privacidade e Segurança > Rede Local).
- Uma barra de andamento que ocupa a linha inteira (o `pv` num pipe com o `7z`, por exemplo) não
  desce mais uma linha a cada volta: o Backspace na coluna 0 de uma linha que continua a de cima
  volta para o fim da de cima, como nos outros terminais.
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
- Com mais de uma janela, as confirmações e os pedidos (senha, chave do servidor, desbloquear o
  cofre, salvar espaço, cluster, Links, instalar, importar e os outros) abriam na janela
  principal, que vinha para a frente. Agora abrem na janela em que foram pedidos.

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
