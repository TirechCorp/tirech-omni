# Tirech Omni: manual

Gerenciador de conexões SSH feito com o Tomni UI: uma árvore de conexões em grupos, as
propriedades da conexão escolhida, e os terminais em abas e em grade (1, 2 × 2 e 3 × 3), em
quantas janelas se quiser, com envio do que se digita para vários terminais.

```sh
cargo run -p tirech-omni
```

## Navegação

O painel à esquerda leva às páginas **Início**, **Terminais**, **Cofre de senhas**, **Chaves SSH**,
**Agente**, **Redes**, **Sobre** e **Configurações**. Em Terminais o painel fica só com os ícones, para a área dos terminais ter a
largura toda. As abas continuam abertas quando se muda de página.

- **Início**: um destaque com as ações (Nova conexão, Importar do `~/.ssh/config`, Abrir shell
  local) e, abaixo, as conexões **Fixadas**, as **Conexões recentes** e as **Ferramentas** (Cofre
  de senhas, Chaves SSH, Agente, Redes e Configurações). Clicar num cartão abre a conexão e mostra Terminais; o menu do
  botão direito tem Abrir, Abrir em ▸ (o painel, ver "Abrir em um painel"), Fixar no Início (ou
  Desafixar), Mostrar na árvore e Remover dos recentes.
- **Busca** (barra de título, Ctrl+E): acha conexões pelo nome, host ou usuário e itens do cofre
  (com o cofre desbloqueado) pelo título, sem diferenciar maiúsculas e acentos. Escolher uma
  conexão a abre em Terminais; escolher um item do cofre mostra o Cofre com ele selecionado.
- **Configurações** (Ctrl+,): tudo o que o app deixa mudar, numa página só (ver "Configurações").
- **Tema** (botão ao lado dos controles da janela): Usar o tema do sistema, Claro ou Escuro, a mesma
  escolha de Configurações ▸ Geral ▸ Tema; trocar por um lado atualiza o outro.

Os recentes entram quando se abre uma conexão (pela árvore, pelo Início ou pela busca); reabrir as
abas do estado anterior não conta. Apagar uma conexão tira o recente dela.

## Janelas e espaços de trabalho

- **Várias janelas**: Janela ▸ Nova janela (Ctrl+Alt+N) abre outra janela com os terminais e
  os painéis **Conexões** e **Propriedades**, fixados como na principal (barra de título com a busca, a
  navegação e a mesma barra de menus da principal). A seleção, o filtro e as pastas abertas da árvore
  são os mesmos em todas as janelas; o Exibir de cada janela mostra e esconde os painéis dela, e o
  arranjo de cada uma volta na próxima execução. Apagar, editar túneis e comandos, executar e as
  sessões do servidor perguntam na janela da árvore em que a pessoa mexeu. Cada janela mostra a
  sua própria página, e o que se abre nela (Cofre, Notas, Desempenho, Sobre, Configurações, o
  script e os arquivos do menu da árvore) abre ali. Arquivos, Logs e Scripts ficam numa janela por
  vez, a última que os usou; as outras mostram um aviso, e a página volta quando a pessoa usa a
  janela. As ações de sessão (Reconectar, Parar o log, Fechar a aba…) agem na janela do menu; as
  que abrem um diálogo da principal (Nova conexão, Novo grupo, Atalhos, Links, Instalar no
  computador, o cluster e copiar a chave) trazem a principal para a frente, até da bandeja, e agem
  lá; no Exibir das outras janelas, só o item do chat fica desabilitado. Um clique duplo
  numa conexão abre a aba na última janela em que a pessoa mexeu. Arrastar uma aba para outra janela (ou para fora de todas, que
  abre uma janela nova) e o menu da aba "Mover para a janela" levam a **mesma sessão**: nada
  reconecta e o histórico continua. O menu Janela lista as janelas abertas e dá o foco.
  Com mais de uma janela, a principal se marca: o título vira "Tirech Omni — Janela principal",
  a barra de título mostra "Janela principal" e os menus a chamam de "Janela 1 (principal)".
- **Estado das conexões na árvore**: um selo no canto do ícone mostra o estado de cada conexão
  com sessão aberta: verde cheio (conectada), anel âmbar (conectando), vermelho com "!" (caiu) e
  anel cinza (encerrada, com a aba ainda aberta); sem sessão, sem selo. Com várias sessões vale a
  mais grave, e a pasta **recolhida** mostra a mais grave de dentro (aberta, cada filho mostra o
  seu). A dica da linha tem o estado e o motivo da queda ("Caiu: tempo esgotado"). O menu do
  botão direito ganha **Reconectar** (só quando alguma sessão caiu; na pasta, reabre as caídas de
  dentro) e **Ir para a aba** (só em conexão com sessão: seleciona a aba e dá o foco à janela
  dela). A aba mostra o anel enquanto conecta e o aviso vermelho quando caiu.
- **Fechar uma janela** com sessões pergunta se as abas vão para a principal ou se fecham. Fechar
  a principal com outras janelas abertas pergunta se fecha tudo.
- **Espaços de trabalho**: o que uma janela (ou todas) tem aberto vira um arquivo em
  `espacos/`, com a grade, as conexões de cada painel e, se o sistema informa, a posição e o
  tamanho das janelas. Janela ▸ "Salvar esta janela como espaço…", "Salvar todas as janelas como
  espaço…" e, no menu da aba, "Salvar o painel como espaço…". O arquivo só tem **referências**
  às conexões: nenhuma senha. Se uma conexão foi apagada, o espaço abre as outras e avisa
  quais faltam. A janela de um **cluster com IA** salva como espaço guarda também o assistente
  e o painel dele (`cluster_ia`, sem a aba do assistente entre as abas): abrir o espaço reabre
  as máquinas e lança o assistente de novo, continuando a conversa dele.
- Os espaços aparecem na árvore, no grupo fixo **Espaços de trabalho** (sempre o último), com
  pastas. Clique duplo abre: um espaço de painel entra no painel ativo, os de janela e de todas
  as janelas abrem numa janela nova cada. O menu do botão direito tem Abrir, Abrir numa nova
  janela, Abrir no painel ativo, Renomear e Apagar; as pastas, Nova pasta, Renomear e Apagar;
  arrastar leva o espaço para outra pasta. Abrir um espaço entra nos recentes do Início.

## O que faz

- **Conexões**: SSH (com chave, agente e jump server), SFTP, FTP e FTPS, S3, os remotos do rclone, shell local e Telnet, em grupos. Nova
  conexão e novo grupo (Ctrl+Shift+N e Ctrl+Shift+G), renomear, duplicar (no menu da conexão:
  a cópia "(cópia)" entra logo abaixo, com tudo da original; a senha do cofre que só a
  original usava ganha um item próprio, e a de várias conexões continua compartilhada), subir,
  descer, apagar (com confirmação) e filtrar pelo nome, usuário ou host. As propriedades mostram só o que vale para
  o tipo da conexão (o shell local não tem host nem porta, o S3 não tem host, e assim por
  diante); o que depende de outro campo fica desabilitado, e nada se apaga: trocar o tipo e
  voltar devolve o que estava preenchido. A **Porta** padrão segue o tipo (22, 23, 21 ou 990
  no FTPS implícito) e só fica em negrito quando é outra.
- **IA**: o app fala com o modelo que a pessoa escolher (Ollama e LM Studio locais, OpenRouter,
  OpenAI, Anthropic, Groq ou outro compatível), cadastrado em Configurações › Assistentes de IA.
  **Gerar comando** (Ctrl+Shift+Space) transforma uma descrição em um comando e o põe na linha
  do terminal **sem Enter**; **Explicar a seleção** e **Explicar o erro** (menu do clique
  direito) e o **chat** (Ctrl+Shift+A, painel à direita, uma conversa por aba) respondem em
  Markdown, e o botão de cada bloco de código insere no terminal, também sem Enter. A IA nunca
  executa nada. Antes do primeiro envio de cada aba, o diálogo **O que vai sair** mostra o texto
  exato e o destino; as senhas do cofre e os padrões comuns de segredo são removidos (ligado por
  padrão, mas não pega tudo), o texto sai sem sequências de controle, e um provedor fora do
  computador e da rede local pede confirmação uma vez por perfil.
- **Assistentes de IA no servidor**: numa conexão SSH, **Ferramentas › Assistentes de IA** (também
  no menu da aba, no menu da conexão na árvore e no botão do cabeçalho) instala e abre o **Claude
  Code**, o **Codex** e o **OpenCode** na máquina remota, com um clique. O app descobre sozinho,
  sem digitar no terminal, o que está instalado; o que falta é instalado numa aba à vista, sem
  `sudo`. **Ctrl+Alt+A** abre o assistente preferido. Detalhes na seção "Assistentes de IA no
  servidor".
- **Colar imagem e arquivos no terminal remoto**: numa aba SSH, **Ctrl+V** com uma captura de
  tela na área de transferência, **Ctrl+Shift+V** (ou Shift+Insert, o botão direito) com arquivos
  copiados, ou arrastar arquivos e pastas para o terminal envia tudo pela mesma conexão da aba
  e cola o caminho no servidor, entre aspas quando preciso, para o Claude Code, o Codex e o
  OpenCode anexarem a imagem. O envio é pelo SFTP, ou por `cat` num servidor sem SFTP, para
  `/tmp/tirech-colar/<AAAAMMDD-HHMMSS>-<nome>` (pasta 0700; a imagem vira
  `colado-AAAAMMDD-HHMMSS.png`; sem permissão, o app usa `~/.cache/tirech-colar` e avisa). Uma
  pasta com muitos arquivos vai num pacote `tar` só, montado pelo app; sem `tar` no servidor,
  ela vai pelo SFTP com vários arquivos em voo. Uma barra mostra o andamento, com **Cancelar**
  (o que chegou é apagado e nada é colado). O shell local e o Telnet colam o caminho desta
  máquina. Em Configurações ▸ Terminal: **Ao colar imagem ou arquivo numa sessão SSH** (Enviar
  e colar o caminho, Colar só o caminho local ou Perguntar), a pasta no servidor, o Ctrl+V, a
  limpeza do que tem mais de N dias (7; roda no primeiro envio de cada sessão) e a partir de
  quantos arquivos a pasta vai num pacote só. Nas propriedades da conexão, categoria **Sessão**:
  **Comprimir a conexão** (o `ssh -C`, lido também do `Compression` do `~/.ssh/config`) e, em
  **Colar no terminal**, **Ao colar**, **Pasta no servidor** e **Shell do servidor** (para as
  aspas: Automático, POSIX, PowerShell ou cmd).
  Numa aba que um assistente controla pelo MCP, o app envia do mesmo jeito, mas não escreve o
  caminho no shell (atrapalharia o próximo comando do assistente): um aviso diz o que foi
  entregue, e a próxima resposta do MCP sobre a aba (e o `listar_abas`, em `arquivos_soltos`)
  conta ao assistente. O que vai como caminho desta máquina (na aba do próprio Claude Code, por
  exemplo) passa a valer como origem do `enviar_arquivo`, além da pasta de troca.
- **Baixar o arquivo marcado no terminal**: numa aba SSH, marque o nome ou o caminho de um
  arquivo do servidor (o que o `ls` mostrou, por exemplo) e dê **Ctrl+clique direito**, como no
  WinSSHTerm; o menu do terminal também tem **Baixar o arquivo marcado…** quando há marca. O
  app pergunta onde gravar (o diálogo lembra a última pasta) e copia pelo SFTP da própria
  sessão, com uma barra de andamento e **Cancelar** (o que começou a ser gravado é apagado). Um
  caminho absoluto vale como está; `~/…` vai para a home; um nome solto vai para a pasta em que
  o shell está (a mesma sonda do "Abrir a pasta atual"; sem ela, o `OSC 7`; sem nada, a home).
  Aspas em volta e o `\ ` do `ls` são tirados. Uma pasta ou um arquivo que não existe dá um
  aviso. O aviso de que terminou tem **Abrir a pasta**, que abre a pasta onde o arquivo foi
  gravado. Sem nada marcado, ou numa aba sem arquivos (shell local, Telnet, serial), o
  Ctrl+clique direito abre o menu, como sempre.
- **Sessão persistente (tmux ou screen)**: a linha **Sessão persistente** das propriedades de
  uma conexão SSH (e de uma pasta, que passa a escolha às de dentro; "Herdar" diz de onde vem)
  liga o `tmux` ou o `screen`. Com o tmux, a aba roda `tmux new-session -A -s tirech-<conexão>-<n>` no lugar do shell, uma sessão
  por aba: quando a rede cai, a sessão continua viva no servidor, com o que rodava nela, e a
  reconexão volta à mesma sessão. Os comandos ao conectar rodam só quando a sessão nasce, não a
  cada volta. Fechar o app guarda o nome da sessão de cada aba no `estado.json` (e um espaço de
  trabalho também), e a aba restaurada volta a ela. Duplicar a aba ou abrir uma aba nova da
  mesma conexão cria outra sessão (`tirech-<conexão>-2`…). Sem `tmux` no servidor, a aba avisa e
  abre o shell comum. Não vale em "Só túneis" nem em servidor marcado como PowerShell ou cmd.
  O mouse fica ligado só naquela sessão (`set-option mouse on`, sem tocar no `~/.tmux.conf`): a
  roda rola o histórico do tmux, que desenha a tela; o atalho do próprio tmux é Ctrl+B [. Para
  marcar texto com o mouse do terminal, segure Shift.
  **Fechar a aba** pergunta o que fazer com a sessão no servidor: **Encerrar a sessão** (roda
  `tmux kill-session` numa conexão nova, com a mesma autenticação, porque a da aba pode ter
  caído) ou **Manter no servidor**; Esc é manter, e **Lembrar a escolha** grava a resposta em
  `conexoes.tmux_ao_fechar` (Perguntar, Encerrar ou Manter, também na página Configurações ›
  Conexões). "Fechar todas as abas" e "Fechar as caídas" perguntam uma vez só para todas.
  Fechar o app ou uma janela, e o `fechar_aba` do MCP, mantêm as sessões.
  Com o **screen**, a aba entra na sessão `tirech-<conexão>-<n>` se ela existe (`screen -x`,
  que divide a sessão com quem já está nela) ou cria uma (`screen -h 10000 -S tirech-…`, com 10
  mil linhas de histórico); encerrar roda `screen -S <sessão> -X quit`. O resto vale igual: a
  reconexão, a restauração, a cópia e a pergunta ao fechar. A sonda da pasta da cópia lê a pasta
  do shell que roda dentro da sessão (pelo `/proc`, no Linux).
  **Sessões no servidor…**, no menu da conexão SSH na árvore e no menu da aba, lista as sessões
  do tmux e do screen vivas no servidor, abertas pelo app ou à mão, com quantos terminais estão
  ligados em cada uma. **Abrir** liga uma aba nova à sessão (mesmo que a conexão não use sessão
  persistente), ou vai para a aba que já está nela; **Encerrar** encerra a sessão no servidor,
  sem pergunta, e a aba daqui que estava nela deixa de perguntar ao fechar. A lista roda pela
  conexão de uma aba aberta da conexão, se há uma, e senão por uma conexão nova com a senha do
  cofre.
- **Mouse no terminal (copiar e colar ou menu)**: a linha **Mouse no terminal** das
  propriedades de uma conexão SSH, shell local ou Telnet (e de uma pasta, que passa a escolha às
  de dentro; "Herdar" diz de qual pasta vem) escolhe **Copiar e colar**: selecionar copia e o
  botão direito copia (com seleção) ou cola (sem), e o Ctrl+clique direito abre o menu; ou
  **Menu**: o botão direito abre o menu e selecionar não copia. Sem escolha na conexão nem nas pastas de cima,
  valem as linhas Copiar ao selecionar e Botão direito de Configurações › Terminal. A aba aberta
  muda junto quando a conexão ou a pasta mudam.
- **Desempenho (CPU, memória e o peso de cada terminal)**: na barra de título, ao lado do botão
  do tema, um medidor de quadradinhos, do amarelo-claro ao vermelho-escuro, mostra quanto da CPU
  do computador o app inteiro usa agora (a dica diz o número). O interruptor **Medidor de CPU na
  barra de título**, em Configurações › Geral, o esconde. O clique nele, ou Exibir ▸
  **Desempenho**, abre a página Desempenho, como a do Gerenciador de Tarefas: à esquerda CPU,
  Memória e Rede com a miniatura de cada uma; à direita o gráfico do último minuto do escolhido,
  com a utilização, a memória, as abas abertas e o tempo aberto embaixo; e a tabela **Por
  terminal**, uma linha por aba de todas as janelas, com a conexão, a CPU que o app gasta com ela
  (desenhar o terminal mais as threads da conexão), os dados por segundo, a CPU do programa que
  roda num shell local e o histórico, com as células pintadas conforme o peso. A linha
  **Interface** é o resto do app. A tabela ordena por qualquer coluna, e o clique duplo numa
  linha vai para a aba. Com o medidor desligado e a página fechada, nada é medido.
  A página tem três abas: **Este app** (o que está acima), **Processos** e **Disco**. As duas
  novas têm os gráficos do mesmo jeito: em Processos, a CPU e a memória da máquina no último
  minuto, com a utilização, a memória, os processos e as threads embaixo; em Disco, a leitura e
  a escrita, com quantos processos usam o disco agora. Embaixo dos gráficos, em
  Processos, todos os processos da máquina escolhida no seletor (**Este computador** ou o
  servidor de uma das conexões SSH abertas, se for Linux) com o PID, o usuário, o estado, a CPU
  (em % da máquina), a memória, as threads e o comando, e um resumo com o total e a memória
  usada. Em Disco, quanto cada um lê e escreve por segundo e o total desde que começou (a
  caixa **Só os que usam o disco** esconde os parados). As duas filtram pelo
  nome, usuário, comando ou PID, ordenam por qualquer coluna e se atualizam a cada 2 segundos
  enquanto estão na tela; Processos também mostra a **Árvore** (cada um embaixo do pai). Os
  botões **Encerrar** e **Forçar encerramento**, e o menu do botão direito, agem nos
  selecionados (no Linux e no macOS, `kill` com TERM ou KILL; no Windows, `taskkill`, com `/F`
  para forçar; no servidor, `kill` pela conexão).
- **Tentar de novo após (reconexão que não desiste)**: quando a aba reconecta sozinha (Configurações
  › Conexões › Ao cair a conexão em **Reconectar sozinho**, e sempre na aba de um assistente)
  e as tentativas acabam, ela espera e começa outra rodada, até a conexão voltar; a aba avisa
  "Tento de novo em 5 min". O tempo vem de **Tentar de novo após (minutos)**, em Configurações ›
  Conexões (5 por padrão; 0 desliga), e a linha **Tentar de novo após** das propriedades de uma
  conexão SSH ou Telnet, ou de uma pasta, escolhe outro: Herdar, **Desligado**, 1, 2, 5, 10, 15
  ou 30 min, ou 1 h. "Herdar" diz de qual pasta vem.
- **Fechar a aba sem uso**: a aba de terminal que passa um tempo sem nada digitado, colado ou
  mandado por um assistente fecha sozinha, com um aviso ("A aba web fechou depois de 10 min sem
  uso"). O que chega do servidor não conta como uso, e a sessão persistente (tmux ou screen)
  continua no servidor. O tempo vem de **Fechar a aba sem uso após (minutos)**, em
  Configurações › Conexões (0, desligado, por padrão), e a linha **Fechar sem uso após**
  das propriedades de uma conexão SSH, Telnet ou do shell local, ou de uma pasta, escolhe outro:
  Herdar, **Desligado**, 5, 10, 15 ou 30 min, ou 1, 2, 4, 8 ou 12 h.
- **Importar e exportar**: Arquivo ▸ **Importar ▸** traz as conexões do WinSSHTerm, do FileZilla, do
  WinSCP, do PuTTY/KiTTY, do mRemoteNG, do MobaXterm e do JSON do próprio app; **Exportar ▸** leva
  para esses mesmos programas e para o `~/.ssh/config` (seção "Importar e exportar", abaixo). O
  `~/.ssh/config` lê `Host`, `HostName`, `Port`, `User`, `IdentityFile`, `ProxyJump`,
  `ForwardX11`, `ForwardX11Trusted`, `LocalForward`, `RemoteForward`, `DynamicForward` e
  `ExitOnForwardFailure` e põe tudo num grupo novo, avisando o que ignorou. O `~/.aws` faz o
  mesmo com os perfis do S3 (ver abaixo).
- **Descrição**: a linha **Descrição** das propriedades guarda um texto livre sobre a conexão,
  como a do WinSSHTerm. Ela aparece na dica da linha da árvore, entra no filtro e vai e volta na
  importação e na exportação (`Descr` do WinSSHTerm e do mRemoteNG, `Note` do WinSCP e `Comments`
  do FileZilla).
- **Abas**: clique duplo (ou Enter) numa conexão abre uma aba; Exibir muda a grade; Ferramentas
  reconecta, duplica e grava o log da sessão. A aba escolhida de cada painel tem o fundo e o
  texto do esquema de cores do terminal, e a do painel ativo ganha uma barrinha na cor de
  destaque embaixo.
- **Abrir em um painel**: o menu do botão direito da conexão (na árvore e no cartão do Início)
  tem **Abrir em ▸** com Painel 1 até o último painel da janela ativa, **Novo painel à direita**,
  **Nova janela** e, com mais de uma janela, **Outra janela ▸**. Enquanto o submenu está aberto,
  cada painel mostra o número dele no centro e o que está sob o ponteiro (ou o foco) fica em
  destaque; as teclas 1 a 9 escolhem o painel. Numa pasta, **Abrir todas** abre todas as
  conexões de terminal dela (as de só arquivos, como FTP e S3, ficam de fora), **Abrir todas em ▸**
  põe todas no painel escolhido e **Abrir todas controladas por IA ▸** (Claude Code, Codex ou
  OpenCode, os que estão à mostra nas Configurações) abre o cluster com IA direto, sem o diálogo. Exibir ▸ **Identificar os painéis** (Ctrl+Alt+I, configurável) mostra os
  números por 2 segundos.
- **Chave do servidor**: na primeira conexão, um diálogo mostra a impressão digital e pede para
  confiar. Uma chave que mudou é recusada, com o aviso.
- **Cofre de senhas**: Ctrl+Shift+K (ou Ferramentas ▸ Cofre de senhas) abre a página do cofre,
  com os itens em pastas e subpastas; as abas continuam abertas em Terminais.
- **Notas em Markdown**: Ctrl+Shift+J (ou Ferramentas ▸ Notas) abre a página Notas: as notas do
  cofre em pastas, um editor com realce e a pré-visualização ao lado ou no lugar dele (ver
  "Notas" abaixo). Cada nota é um item do cofre, cifrada com ele.
- **Reabre onde parou**: as janelas, a grade e as conexões abertas voltam na execução seguinte.
- **Redes de clientes**: Ferramentas ▸ Redes guarda perfis de rede (um proxy SOCKS5 ou HTTP em
  `127.0.0.1`, uma rede Nebula embutida, um servidor de salto e os comandos para subir e descer a
  VPN), e a linha **Rede** das propriedades liga um grupo ou uma conexão a um perfil (ver abaixo).
- **Comandos ao conectar**: a linha **Comandos** das propriedades de uma conexão SSH, Telnet
  ou Local (e de uma pasta) abre um editor de passos que o app digita depois que a sessão abre:
  `sudo su -`, "Enviar a senha", `cd /srv/app`. Cada passo espera a tela parar, um texto ou nada,
  com tempo máximo; roda ao abrir, ao duplicar, ao reconectar e pelo menu da aba. As pastas
  passam os passos adiante (Herdar, Próprios, Nenhum). Os tempos padrão estão em Configurações ›
  Conexões.
- **Arquivos** (SFTP, FTP e S3): a página Arquivos mostra, lado a lado, os arquivos desta máquina e os do
  servidor, uma aba por conexão (SSH, que também serve para o SFTP, SFTP, FTP ou S3). Cada painel tem os botões
  Acima, Início, Atualizar e Nova pasta, o campo de caminho (com sugestões), o filtro e
  "Mostrar ocultos"; a tabela ordena por nome, tamanho, data e permissões, com seleção
  múltipla, renomear (F2), apagar (Delete, com confirmação), permissões (Propriedades) e
  menu de contexto. Arrastar de um painel para o outro envia ou baixa; arrastar do gerenciador
  de arquivos do sistema envia (no Wayland o sistema não entrega o arrastar: use o menu). A fila
  embaixo mostra o andamento e a velocidade de cada transferência, com pausar, cancelar, tentar de
  novo e limpar. Quando o destino já existe, um diálogo oferece sobrescrever, retomar, pular ou
  renomear (`nome (1).ext`), com "Fazer o mesmo nos próximos". Uma aba de um terminal aberto
  usa a conexão SSH dele, sem pedir a senha de novo; "Abrir terminal aqui" manda o `cd` para a
  sessão. Sem aba aberta, a página segue a Gallery do WinUI 3, como o Início: a faixa de destaque
  com **Nova conexão** e **Remotos do rclone**, e embaixo os cartões das conexões que abrem
  arquivos (nome, `usuário@host:porta` e o tipo) e das pastas recentes. Com abas, os dois botões
  ficam na barra, ao lado de **Nova aba**.
- **Comparar pastas**: o botão **Comparar pastas** da barra da aba pinta as linhas dos dois
  painéis, pelo nome (maiúsculas e minúsculas contam): amarelo é o que só existe naquele lado,
  verde é o mais novo (critério **data**) e vermelho é o de tamanho diferente (critério
  **tamanho**). O botão **Opções da comparação**, ao lado, abre o menu que escolhe o critério
  e liga **Ocultar idênticos**, que esconde as linhas sem cor e as traz de volta ao desligar.
  A tolerância de data e o que as abas novas já abrem ligado ficam em Configurações ▸ Arquivos. Pasta
  contra pasta só compara se existe; pasta contra arquivo é "tamanho diferente" no critério
  tamanho e idêntico no critério data. Na data, uma diferença de até a tolerância (1 minuto por
  padrão, por causa do FTP, que lista só até o minuto) conta como a mesma, e um lado sem data
  deixa o par idêntico. A comparação acompanha a navegação e compara só a pasta aberta, sem
  descer nas subpastas nem olhar o conteúdo.
- **Navegação sincronizada**: o botão **Navegação sincronizada** guarda as duas pastas em que
  está (a base). Entrar, subir, Início ou digitar um caminho num painel leva o outro ao caminho
  equivalente a partir da base dele, sem eco de volta. Se o destino fica fora da base, ou a
  pasta equivalente não existe no outro lado, a sincronização desliga com um aviso e o outro
  painel fica onde estava. Ligar de novo guarda uma base nova.
- **Pastas padrão**: cada conexão tem uma **pasta local** (com seletor) e uma **pasta remota**
  padrão, e "Comparar pastas" e "Navegação sincronizada" ao abrir (Padrão, Ligado ou
  Desligado; Padrão segue as Configurações). A pasta em que cada painel abre vem, nesta
  ordem, de: a pasta recente ou o estado salvo, a pasta da conexão, a pasta local das
  Configurações (só o painel local) e o início. Uma pasta que não existe cai no início, com
  aviso.
- **FTP e FTPS**: o tipo de conexão **FTP** abre direto na página Arquivos (não tem terminal).
  Nas propriedades, a **Segurança** tem quatro modos: "FTP sobre TLS explícito" (o
  padrão, porta 21; recusa o servidor que não aceita TLS), "TLS, se disponível" (o explícito que
  segue em claro se o servidor não oferecer, com um aviso na aba), "FTPS implícito" (porta 990)
  e "FTP simples (sem TLS)". Trocar a segurança troca a porta entre 21 e 990 se ela estava no
  padrão. O **Modo** é Passivo (o padrão) ou Ativo; em "FTP avançado" ficam a versão do TLS
  (Automática, que passa do 1.3 ao 1.2 sozinha, com um aviso, se o servidor não retomar a sessão),
  a codificação dos nomes, o fuso do servidor e a faixa de portas do modo ativo. O usuário vazio
  (ou `anonymous`) entra como anônimo; senão, a senha vem do cofre (Credencial) ou é pedida e
  salva, como no SSH. Um certificado que o sistema não aceita (autoassinado, vencido) abre o
  diálogo **Certificado não confiável**, com o motivo, o assunto, o emissor, a validade e a
  impressão SHA-256; "Confiar e continuar" guarda a impressão em `certificados.json` (chave
  `host:porta`) e a conexão não pergunta de novo enquanto o certificado for o mesmo. Uma aba sem
  TLS mostra um cadeado aberto. As transferências retomam do ponto em que pararam.
- **S3**: o tipo de conexão **S3** abre na página Arquivos, como o FTP (não tem terminal), com a
  Amazon e os serviços compatíveis. Nas propriedades, a categoria **S3** tem o **Provedor**
  (Amazon S3, MinIO, Cloudflare R2, Wasabi, Backblaze B2, DigitalOcean Spaces e "Ceph RGW /
  outro"): escolher um preenche o **Endereço**, a **Região** e o **Estilo da URL** (Caminho ou
  Virtual host), e os três seguem editáveis; trocar a região leva o endereço junto enquanto ele
  for o do provedor. No Cloudflare R2, troque `{conta}` pelo id da conta no endereço. O
  **Bucket** vazio faz a raiz listar os buckets da conta, e cada um vira uma pasta; com ele, a
  raiz é a do bucket. "S3 avançado" tem a classe de armazenamento dos envios e como o
  certificado é conferido (pelo sistema, por uma CA própria ou sem conferir, para um MinIO de
  laboratório). Pastas são prefixos: Nova pasta cria o marcador `pasta/` e apagar uma pasta
  apaga tudo que há nela. Os envios grandes vão em partes e retomam do ponto em que pararam.
  Não há permissões, links simbólicos nem dono, e o servidor não deixa mudar a data: a data do
  arquivo vai junto no envio.

  Menu de contexto de um arquivo do bucket: **Copiar link pré-assinado** (vale 1 hora, 1 dia ou
  7 dias; o link vai para a área de transferência e uma barra diz até quando vale),
  **Classe de armazenamento** (muda a classe do objeto) e **Propriedades do objeto**
  (tamanho, classe, tipo, ETag, data e metadados). **Envios incompletos…**, na barra da aba, lista
  os envios em partes que não terminaram na pasta atual (que o provedor cobra) e aborta um ou
  todos. Essas ações abrem uma conexão própria, numa thread, e a barra mostra o erro se falharem.
- **rclone (as demais nuvens)**: a página Arquivos tem a subpágina **Remotos do rclone**, que
  lista os remotos do `rclone.conf` (Google Drive, OneDrive, Dropbox, Box, Azure Blob, Google
  Cloud Storage, WebDAV, SMB, Mega… são mais de 70) e abre qualquer um na mesma tela dos
  outros. Botões: **Abrir**, **Novo**, **Editar**, **Apagar**, **Pôr na árvore** (cria uma
  conexão do tipo rclone no grupo escolhido, que também abre pela árvore e entra nos recentes),
  **Atualizar** e **Opções** (o caminho do binário e do `rclone.conf`). O **Novo** monta o
  formulário com as opções que o próprio rclone descreve para o provedor (as básicas, e as
  avançadas pela caixa "Mostrar as opções avançadas"), e as que só valem para um `provider`
  somem quando outro é escolhido. Salvar abre o diálogo "O rclone quer saber" a cada pergunta
  da configuração; no OAuth (Drive, OneDrive, Dropbox…), "Sim" abre o navegador e mostra
  "Conclua o login no navegador…" até terminar, e "Não" pede o código do
  `rclone authorize` (para uma máquina sem navegador). Cancelar no meio apaga o remoto
  inacabado. A conexão rclone da árvore tem as linhas **Remoto** (`gdrive`) e **Caminho**
  (`Backup/2026`). As cópias entre o disco e a nuvem, e entre duas nuvens, são feitas pelo
  próprio rclone, com progresso e cancelamento. Os arquivos têm o link público quando o
  provedor faz.
- **Editar um arquivo remoto**: abrir um arquivo do servidor (clique duplo, Enter ou "Abrir" no
  menu) baixa para uma pasta temporária e abre no programa do sistema. Quando o arquivo muda, a
  página pergunta "Enviar?" (ou "Sempre enviar este arquivo"). A pasta temporária some ao
  fechar a aba.

## Ferramentas para vários servidores

O menu **Ferramentas** reúne o que ajuda quem cuida de vários servidores. Cada ferramenta tem
um item no menu e, quando faz sentido, um atalho que se troca em Configurações › Atalhos.

- **Envio para vários** (Ctrl+Shift+I): abre uma barra no pé da janela com o texto, o "Enter"
  e o botão "Teclas" (Ctrl+C, Ctrl+D, Ctrl+Z, Tab, Esc e Enter). O botão "N abas…" escolhe as
  abas que recebem; uma aba caída fica de fora, e o histórico do que já foi enviado vira
  sugestão. Ao lado de Nenhum, Visíveis e Todos, o modo **Escolhidas** leva o que se digita
  num terminal às abas marcadas ("Receber o envio para vários" no menu da aba, ou o diálogo da barra); as
  marcadas mostram o ícone de transmissão.
- **Modo cluster** (menu de contexto de um grupo ou de uma conexão › "Abrir em modo cluster…",
  ou Ferramentas › Modo cluster › "Cluster…"): um diálogo com as conexões de terminal, já marcadas pelo
  grupo ou pela conexão da árvore. "Abrir" cria uma janela nova com as conexões em grade (até
  4 × 4; o que passar vira abas) e o envio em **Todos**: o que se digita num terminal vai para
  todos.
- **Cluster com IA** (menu de contexto › "Abrir em cluster com IA…", ou Ferramentas › Modo
  cluster › "Cluster com IA…"; numa pasta, "Abrir todas controladas por IA ▸" pula o diálogo e
  abre todas as conexões de terminal dela com o assistente escolhido): o mesmo diálogo, com a escolha do **assistente** (Claude Code,
  Codex ou OpenCode, os que estão à mostra nas Configurações). "Abrir" cria uma janela com as
  máquinas lado a lado e, no painel seguinte, **um só** assistente deste computador que controla
  todas (ver "Cluster com IA" na seção do controle por assistentes). Não tem envio para todos: o
  que se digita vai só para a aba ativa. Não depende de espaço de trabalho; para guardar,
  salve a janela como espaço.
- **Scripts** (menu de contexto › "Rodar script…", Ferramentas › "Rodar script…" ou o cartão
  do Início): a página Scripts roda um arquivo da pasta de scripts, ou uma linha digitada, em
  várias conexões SSH ao mesmo tempo, pelo canal `exec` e sem abrir abas. A pasta é
  `scripts` na pasta de dados do app (ou a de `scripts.pasta`); "Novo…" cria o arquivo e o
  abre no editor do sistema. O arquivo vai pela entrada do interpretador (`scripts.interpretador`,
  `sh -s` por padrão; troque por `bash -s`, por exemplo); a linha digitada vai como está. Por
  servidor, a grade mostra o estado, o código de saída, a duração e a primeira linha da
  saída; escolher a linha põe a saída inteira, com as cores, no terminal da página, e há
  "Copiar a saída" e "Salvar os resultados…". `scripts.paralelo` limita quantos rodam ao
  mesmo tempo (8 por padrão); "Parar" impede os que ainda esperam. Para as conexões sem senha
  no cofre, a página pede uma senha comum (opcional, com "Salvar no cofre").
- **WSL** (Ferramentas › WSL, no Windows): lista as distribuições (`wsl.exe -l -q`), abre cada
  uma numa aba (como o shell local, sem entrar nos recentes) e, em "Salvar como conexões",
  as grava no grupo "WSL", sem repetir. Cada uma é uma conexão **Local** com programa
  `wsl.exe` e argumentos `-d <distribuição> --cd ~`. Toda conexão Local tem as linhas
  **Programa** e **Argumentos** nas propriedades: vazias, abre o shell de sempre.
- **Abas caídas**: a aba de uma sessão encerrada mostra o ícone de aviso, e o terminal dela fica
  com o cursor fixo e vazado. "Reconectar as abas caídas" (Ctrl+Shift+R), "Fechar as abas
  caídas" e "Fechar todas as abas" estão no menu Ferramentas (todas as janelas) e no menu da
  aba (a janela dela). "Fechar todas" só pergunta quando há sessão viva, com "Não perguntar de
  novo" (`conexoes.confirmar_fechar_todas`).
- **Dica da aba**: parar o ponteiro sobre uma aba mostra o título, a conexão, o destino (tipo,
  usuário, host e porta), o salto, a rede e o estado ("Conectada", "Caiu: <motivo>" ou
  "Encerrada").
- **Duplicar aba** (menu da aba, menu Ferramentas e Alt+Shift+D): a cópia abre na pasta em que
  a original está agora. No shell local, o processo já nasce nela (Linux: pelo `/proc`; macOS e
  Windows: pelo OSC 7; no WSL vai no `--cd`). No SSH, uma sonda de até 2 s numa sessão `exec` da
  mesma conexão acha a pasta do shell sem configurar nada no servidor, com o OSC 7 como reserva, e
  o app digita ` cd '<pasta>'` depois do prompt e dos comandos ao conectar. Sem pasta descoberta, a
  cópia abre como antes. A cópia herda também a rede e o zoom da fonte. Na aba com a sessão
  persistente, a sonda pergunta a pasta ao tmux (`#{pane_current_path}`), ou ao `/proc` no
  screen, e a cópia ganha outra sessão. Com um `tmux` ou um `screen` aberto à mão, o programa em primeiro plano é o
  cliente deles, e a pasta é a de onde ele foi aberto.
  `terminal.duplicar_na_pasta` (ligada) desliga.
- **Cursor e painel vazio**: `terminal.cursor_piscando` (ligada) vale mais que o pedido do
  programa; o painel que perde a última aba mostra "Fechar o painel", e
  `terminal.fechar_painel_vazio` o fecha sozinho.
- **Mostrar a conexão da aba na árvore** (menu Exibir, desligado por padrão, grava em
  `conexoes.mostrar_aba_na_arvore`): escolher uma aba, em qualquer janela, escolhe a conexão
  dela na árvore, abre as pastas em volta e rola até a linha, sem tirar o foco do terminal,
  como o "sincronizar a árvore" do WinSSHTerm. Clicar na aba que já é a ativa também mostra a
  conexão de novo.

| Ferramenta | Onde está |
|---|---|
| Envio para vários | Ferramentas › Envio para vários…, Ctrl+Shift+I, modo "Escolhidas" em Exibir |
| Modo cluster | Ferramentas › Modo cluster › Cluster…, menu de contexto de grupos e conexões |
| Cluster com IA | Ferramentas › Modo cluster › Cluster com IA…, menu de contexto de grupos e conexões |
| Scripts | Ferramentas › Rodar script…, menu de contexto, cartão do Início |
| WSL | Ferramentas › WSL |
| Abas caídas | Ferramentas e menu da aba, Ctrl+Shift+R para reconectar |
| Dica da aba | ponteiro sobre a aba |
| Duplicar aba | menu da aba, Ferramentas e Alt+Shift+D |
| Cursor e painel vazio | Configurações › Terminal |
| Conversão de chaves | página Chaves SSH |

## Senhas

O app guarda senhas. Uma conexão SSH sem chave pede a senha num diálogo com
"Salvar a senha" (marcada por padrão): a senha vai para o cofre e a conexão seguinte abre sem
diálogo. Se o servidor recusar a senha, ela é pedida de novo e o item é atualizado. A conexão
(e o salto dela) aponta para um item do cofre pela linha "Credencial" das propriedades.

No Telnet a linha "Credencial" e a **Senha** também existem: a senha vai pelo passo "Enviar a
senha" dos comandos ao conectar, com a origem "Senha da conexão". Essa origem só é oferecida no editor dos comandos
quando o tipo da conexão tem Credencial (numa pasta, sempre).

Sem item do cofre, a linha "Credencial" de uma conexão SSH ou SFTP escolhe quando a senha é
pedida (o item do cofre, quando há, vence):

- **Herdar** (o padrão): vale o modo da pasta mais próxima que escolheu um ("Herdar (Cliente)"
  diz qual); sem nenhuma, é o "Perguntar ao conectar".
- **Perguntar ao conectar:** o diálogo da senha vem antes de conectar.
- **Agente SSH:** nunca pergunta. Entra pelo agente e pelas chaves; se o servidor recusar, o
  erro fica na aba, sem diálogo.
- **Agente SSH, senão a senha:** conecta direto pelo agente e pelas chaves; se o servidor
  recusar, o diálogo da senha abre ("O servidor não aceitou o agente SSH nem as chaves.") e a
  conexão é refeita com a senha digitada.

A pasta tem a mesma linha "Credencial" nas propriedades dela, ao lado de Rede, Assistentes
(MCP), Sessão persistente e Comandos: escolher **Agente SSH** numa pasta faz todas as conexões
SSH e SFTP de dentro (e das subpastas) que deixaram "Herdar" entrarem pelo agente, sem pedir a
senha. Uma conexão que escolheu outro modo, ou que aponta para um item do cofre, segue a dela.

A pasta também tem as linhas **Chave do cofre** e **Chave privada** (o arquivo da chave): as
conexões SSH e SFTP de dentro (e das subpastas) que não têm chave própria usam a da pasta mais
próxima que escolheu uma. Na conexão, a linha "Chave do cofre" mostra "Herdar (Cliente)" quando
uma pasta dá a chave; a chave da própria conexão, do cofre ou em arquivo, sempre vence. Somada a
"Agente SSH" na Credencial, a pasta faz as conexões de dentro entrarem pela chave sem pedir senha.

O salto (jump server) não tem esses modos: a senha dele nunca é pedida antes de conectar. Ele
tenta o agente e as chaves (e o item do cofre, se houver), e uma recusa só abre o diálogo da
senha do salto quando a conexão é vigiada (com segredo do cofre ou no modo "Agente SSH, senão a
senha").

O cofre (Ferramentas ▸ Cofre de senhas, Ctrl+Shift+K) guarda também qualquer outra senha:
itens de tipo Senha, SSH, SFTP, FTP, S3, rclone, Passphrase de chave ou Nota, com usuário, URL,
notas e campos extras. A página tem três painéis, como o Explorador de Arquivos: as pastas à
esquerda (a raiz é "Cofre"), a lista da pasta aberta no meio e os detalhes do item à direita.

- **Pastas e subpastas:** Nova pasta (Ctrl+Shift+N), renomear (F2), arrastar da lista para uma
  pasta da lista ou da árvore, arrastar pastas dentro da árvore, "Mover para…" e apagar. Apagar
  uma pasta leva as subpastas e os itens, e o diálogo conta o que sai. Uma pasta não vai para
  dentro dela mesma nem para onde já há uma pasta irmã de mesmo nome. Clique duplo (ou Enter)
  abre a pasta, ou copia a senha do item; Backspace sobe um nível.
- **Filtro:** procura na pasta aberta e nas subpastas, pelo título, usuário, endereço, notas e
  nome da pasta, sem diferenciar acentos nem maiúsculas, e mostra o caminho de cada resultado.
- **Importar** (na pasta aberta, juntando pastas de mesmo nome): o JSON do Tirech Omni, o JSON
  do Bitwarden (o não cifrado), o CSV do Bitwarden e o CSV do KeePassXC. As pastas aninhadas
  viram subpastas.
- **Exportar:** o JSON do Tirech Omni (completo) e o JSON do Bitwarden, sempre com as senhas.
- Gerador de senhas, copiar usuário e senha, senha mestra (definir, trocar, remover) e
  bloquear. A busca global da barra de título acha os itens, mostra a pasta e abre o item.

### Notas

A página Notas (Ctrl+Shift+J) mostra as notas do cofre (itens de tipo Nota) na árvore das
pastas, com o editor de Markdown e a pré-visualização (Editar, Dividir ou Ler). Salva sozinha,
tem barra de formatação, busca (Ctrl+F, F3) e substituição (Ctrl+H) e usa a senha mestra e o
bloqueio do cofre. Um bloco ` ```segredo ` fica mascarado na pré-visualização; "Copiar" e Ctrl+C
limpam a área de transferência depois do tempo do Cofre. As notas guardam segredos e textos
soltos (chaves de API, acessos a sistemas) e não se ligam às conexões: a conexão tem a linha
**Descrição** das propriedades. Um arquivo antigo, de quando a nota se ligava à conexão, passa o
texto dela para a descrição ao abrir o cofre, e a nota sai do cofre (a que tem anexos fica).

- **Anexos:** colar uma imagem (Ctrl+V), arrastar arquivos ou "Anexar arquivo…" gruda o arquivo
  na nota, sem limite de tamanho. Cada anexo é cifrado em blocos de 1 MiB (AES-256-GCM, com chave
  própria) na pasta `vault/` ao lado do `vault.json`; a nota guarda o link `anexo:<id>`, e a
  imagem aparece na pré-visualização. Apagar o anexo apaga o arquivo cifrado.
- **Markdown:** títulos, ênfase, listas e tarefas, citações, tabelas, código com realce, links
  (`https:` abre no navegador) e imagens.
- **Importar e exportar:** `.md` por nota. O JSON do Tirech Omni exporta os anexos em claro numa
  pasta `<nome sem .json>-attachments` ao lado do arquivo (o JSON é gravado depois dos
  arquivos); ao importar, o app confere o SHA-256 e cifra de novo com ids e chaves novos. O JSON
  do Bitwarden não leva anexos.

O `vault.json` é cifrado com AES-256-GCM. Sem senha mestra (o padrão) a chave fica no arquivo
`vault.key` da pasta de configuração e o cofre abre sozinho; com senha mestra opcional (Argon2id),
o app a pede ao iniciar. Proteger as senhas é responsabilidade de quem usa o app: o tirech-omni
cifra o que grava e oferece a senha mestra, mas não cria barreiras (sem bloqueio automático,
sem confirmações extras, sem exigir a senha mestra).

### Levar o cofre para outra máquina

Nada no cofre depende da máquina: copiar a pasta dele (o `vault.json`, o `vault.key` e a pasta
`vault/` dos anexos das notas) para
outro computador e abrir o app lá funciona, com as mesmas senhas e chaves. Com senha mestra, o
`vault.json` sozinho basta (a senha o abre). Para um arquivo só:

- **Levar o cofre para outra máquina…** (cofre ▸ Mais, ou Configurações ▸ Cofre de senhas)
  salva um `.tirech-vault` (e, se há anexos, a pasta `<nome>.vault/` ao lado dele; a cópia
  mostra o andamento e dá para cancelar). Sem senha para o arquivo, a chave vai dentro dele (quem tiver o
  arquivo abre o cofre); com uma senha só dele, ela é pedida ao abrir. Um cofre com senha mestra
  leva o arquivo protegido por ela e precisa estar desbloqueado.
- **Abrir cofre de outra máquina…** aceita o `.tirech-vault` ou um `vault.json` (com o
  `vault.key` ao lado, se não tiver senha mestra), com a pasta `<nome>.vault/` dos anexos, se
  houver; sem ela, o app conta quantos anexos não vieram junto. **Juntar com este cofre** traz o que o outro
  tem de novo, sem perder nada daqui; **Usar no lugar deste** troca o cofre e guarda o de agora
  ao lado como `vault.json.bak-<data e hora>` (e o `vault.key`).
- **Local do cofre** (Configurações ▸ Cofre de senhas) separa o cofre da pasta de dados: dá
  para guardá-lo numa pasta sincronizada (Syncthing, Dropbox, OneDrive…). Ao mudar, o app leva o
  cofre para lá ou usa o que já houver na pasta nova; "Voltar à pasta de dados" desfaz.
- **Duas máquinas, uma pasta sincronizada:** cada item e cada pasta têm um `uid`, e apagar deixa
  uma lápide. Antes de gravar, o app junta o que mudou no disco; confere o arquivo a cada 5
  segundos e ao ganhar foco, e avisa quando algo entrou de outra máquina. Se a sincronização
  criar uma cópia em conflito (`vault.sync-conflict-….json` do Syncthing, `vault (conflicted
  copy …).json` do Dropbox…), o app oferece juntá-la e arquiva a cópia.
- O `vault.json` é gravado com permissão 0600 no Linux e no macOS.

## Importar e exportar

Arquivo ▸ **Importar ▸** e Arquivo ▸ **Exportar ▸** (ids `importar-<formato>` e `exportar-<alvo>`).

### Importar

Escolha o programa, escolha o arquivo (o seletor já abre onde ele costuma ficar) e o app mostra
uma **pré-visualização**: a árvore que vai entrar, a contagem ("3 conexões em 2 grupos, 3
senhas") e os avisos do que ficou de fora. Ali você muda o **grupo de destino** (por padrão,
"Importado de X"), decide se as **senhas** vão para o cofre e escolhe o que fazer com as conexões
que já existem (mesmo nome, tipo, host, porta e usuário): **pular**, **substituir** ou **manter as
duas**. Nada é gravado antes de "Importar".

| Programa | Arquivo | O que vem |
|---|---|---|
| WinSSHTerm | `connections.xml` (`%APPDATA%\WinSSHTerm`) | pastas, host, porta, usuário, chave, X11. **Sem senha**: o WinSSHTerm as cifra no arquivo `key`, e a importação avisa |
| FileZilla | `sitemanager.xml` (`~/.config/filezilla`, `%APPDATA%\FileZilla`) | pastas, FTP/FTPS/SFTP/S3, modo passivo, codificação, chave, pasta local e remota padrão, "Comparar pastas" e "Navegação sincronizada" ligados; a senha em `base64`. A senha `encoding="crypt"` (com senha-mestra do FileZilla) não vem, com aviso |
| WinSCP | `WinSCP.ini` | pastas, SFTP/SCP (SCP vira SFTP, com aviso)/FTP/S3, chave, pasta local e remota padrão, X11, túneis, proxy, salto; a senha é recuperada |
| PuTTY / KiTTY | `.reg` exportado do `regedit` (UTF-16 também) ou a pasta `Sessions` do KiTTY portátil (`~/.putty/sessions`) | SSH e Telnet, chave, X11, túneis, proxy; um nome com `/` vira pasta. **Sem senha**: o PuTTY não guarda |
| mRemoteNG | `confCons.xml` | pastas, SSH e Telnet, senhas (a senha-mestra padrão é `mR3m`; se o arquivo usa outra, o app pergunta). `FullFileEncryption="True"` é recusado com aviso |
| MobaXterm | `MobaXterm.ini` | pastas e sessões SSH. **Sem senha**: o MobaXterm as guarda no registro do Windows |
| Tirech Omni | JSON exportado pelo próprio app | tudo (reimporta a árvore igual) |
| OpenSSH | `~/.ssh/config` | ver "O que faz" |
| AWS | `~/.aws` | ver "S3: de onde vêm as credenciais" |

Os dois últimos mantêm o fluxo antigo, direto, sem pré-visualização. Chave privada, proxy (vira um
perfil de rede) e salto vêm quando o programa de origem tem. Em Configurações ▸
Conexões, **Importar as senhas por padrão** liga a caixa das senhas já marcada. Com o cofre
bloqueado a caixa fica desabilitada e as senhas não entram (a pré-visualização diz isso). Uma
conexão com protocolo que o app não abre (WebDAV, RDP, VNC…) fica de fora, com o aviso.

### Exportar

Escolha o programa; o diálogo traz **Incluir as senhas** e **Incluir as chaves do cofre**, sem
confirmação extra. Os formatos que não guardam senha (`~/.ssh/config`, PuTTY, WinSSHTerm,
MobaXterm) deixam a primeira caixa desabilitada, e o cofre bloqueado também.

| Alvo | Arquivo | Senhas | Chaves |
|---|---|---|---|
| Tirech Omni (JSON) | `tirech-omni-conexoes.json` | sim (o bloco `cofre`) | dentro do bloco `cofre`, com as senhas |
| `~/.ssh/config` | `config` | não | OpenSSH |
| PuTTY / KiTTY | `tirech-omni.reg` | não | `.ppk` |
| FileZilla | `sitemanager.xml` | sim | `.ppk` (só SFTP) |
| WinSCP | `WinSCP.ini` | sim | `.ppk` |
| WinSSHTerm | `connections.xml` | não | `.ppk` |
| mRemoteNG | `confCons.xml` (senha-mestra padrão `mR3m`) | sim | não |
| MobaXterm | `tirech-omni.mxtsessions` | não | OpenSSH |

As chaves do cofre que as conexões usam saem numa pasta ao lado do arquivo, `<nome sem extensão>-chaves/`
(PPK ou OpenSSH, conforme o programa), **sem passphrase**, e o arquivo exportado aponta para
elas. Apague a pasta depois de levar as chaves. O JSON do Tirech Omni também leva os anexos das
notas, na pasta `<nome sem extensão>-attachments/`. Conexões de um tipo que o programa não conhece
(por exemplo, S3 no PuTTY) ficam de fora, e o diálogo final conta o que saiu e o que não saiu.

Como abrir o arquivo no programa de destino (confira o nome do menu na versão que você usa):

- **PuTTY / KiTTY:** duplo clique no `.reg` (o Windows pergunta se pode juntar ao registro).
- **FileZilla:** Arquivo ▸ Importar… e escolha o `sitemanager.xml`.
- **WinSCP:** Ferramentas ▸ Importar/Restaurar configuração…, ou `WinSCP.exe /ini=<arquivo>`.
- **WinSSHTerm:** feche o programa, faça um backup do `connections.xml` da pasta de configuração
  dele e troque pelo exportado.
- **mRemoteNG:** Arquivo ▸ Importar ▸ Importar de arquivo… (as senhas abrem com `mR3m`).
- **MobaXterm:** botão direito em "User sessions" ▸ Importar sessões de um arquivo.
- **OpenSSH:** copie o conteúdo para o seu `~/.ssh/config`.

### O que não faz

Remmina, SecureCRT, Royal TS, Xshell, SuperPuTTY, MTPuTTY, PuTTY Connection Manager e Tabby; ler o
registro do Windows direto (exporte um `.reg`); escolher itens da pré-visualização (entra tudo, e
você apaga o que não quiser); o `FullFileEncryption` do mRemoteNG; a chave `.ppk` do WinSSHTerm.
O salto (jump) não sai para o FileZilla, o WinSSHTerm, o PuTTY, o MobaXterm e o mRemoteNG, que não têm o conceito por conexão; os
avisos da exportação dizem quando isso acontece. Os anexos das notas do JSON do Tirech Omni entram
de uma vez, sem diálogo de andamento.

## Chaves e agente

O tirech-omni gera chaves, guarda-as no cofre e tem um agente próprio, sem programa externo
O que se faz com as chaves é escolha de quem usa o app: ele não insiste nem
bloqueia (passphrase vazia, chave sem confirmação e senha salva no cofre são permitidas). Na
geração só não há DSA nem RSA menor que 3072 bits.

### Uma chave, um lugar

A chave privada fica **no cofre** (item "Chave SSH", sem passphrase própria, com o arquivo
original guardado para exportar). As conexões, o agente e o `~/.ssh/config` a usam por
referência: não há cópia a manter. A página **Chaves SSH** tem, em cima, Gerar, **Importar…**
(arquivos de qualquer formato; escolha vários de uma vez e eles vêm numa lista, com o destino
comum e a passphrase digitada tentada nas outras chaves; o seletor reabre na última pasta usada),
**Colar…**, **Converter arquivo…** (troca o formato sem guardar
nada) e **Trazer todas de ~/.ssh…** (importa as chaves da pasta; apagar os arquivos de lá depois é
opcional e pede confirmação). Cada chave tem **Exportar como…** e as ações de sempre (Copiar a
pública, Carregar no agente, Copiar para o servidor…). A passphrase de uma chave em arquivo é
achada pela impressão digital, não pelo caminho.

- **Gerar uma chave:** o formulário **Nova chave** tem a linha **Destino**, com o mesmo par de
  opções da importação. **Cofre (recomendado)**, o padrão, não grava arquivo nenhum: a chave
  nasce sem passphrase (o cofre já a cifra) e vira um item "Chave SSH" com o **Nome no cofre**
  que você der (de início, o do arquivo padrão do tipo, como `id_ed25519`); a proteção, a
  passphrase e o arquivo ficam desabilitados, e o cofre bloqueado recusa antes de gerar.
  "Carregar no agente" a põe no agente com a origem do cofre, e o resultado (impressão digital,
  randomart e pública) oferece Copiar, Copiar para o servidor e "Usar na conexão", que liga a
  conexão escolhida à "Chave do cofre" do item. **Arquivo em ~/.ssh (OpenSSH)** é o de antes:
  grava `id_ed25519` e `id_ed25519.pub`, com passphrase se você der uma (e a guarda no cofre
  se você deixar marcado).
- **Formatos das privadas:** OpenSSH, PKCS#8 (com e sem cifra), PEM tradicional (PKCS#1 e SEC1;
  gravado só sem cifra e não para Ed25519) e PuTTY `.ppk` v2 e v3.
- **Formatos das públicas:** linha OpenSSH, RFC 4716, SPKI (PEM) e PKCS#1 (PEM).
- **FIDO (`sk-`)** é guardada e exportada em OpenSSH, mas não entra no agente. Os certificados
  (`-cert.pub`) ficam com a chave e entram no agente junto.
- Exportar a chave privada sem passphrase não pede confirmação.

### O agente

- **Agente embutido**, compatível com o `ssh-agent` do OpenSSH. Carrega as chaves marcadas "No
  agente" quando o cofre abre (e as tira ao bloquear, se a opção estiver ligada). A página
  lista as chaves, com a origem de cada uma (cofre, arquivo ou "Adicionada por outro programa")
  e a validade ("expira em 5 min"), e tem Adicionar arquivo…, Remover, Remover todas, Travar e
  Destravar. As chaves de arquivo adicionadas pela página voltam ao iniciar (`agente.json`).
- **Endereço estável:** `$XDG_RUNTIME_DIR/tirech-omni/agent.sock` no Linux,
  `~/.ssh/tirech-omni-agent.sock` no macOS (e no Linux sem `XDG_RUNTIME_DIR`) e o pipe
  `\\.\pipe\openssh-ssh-agent` no Windows.
- **Confirmar o uso** de uma chave (opção por chave, desligada por padrão): o diálogo mostra
  quem pede ("codex ← git ← ssh") e pode lembrar a resposta por 15 minutos. Sem a opção, o
  agente atende sem perguntar.

### Usar as chaves do app no ssh do sistema

O terminal, o `git`, o VS Code, o Codex e o Claude Code usam as chaves do cofre **sem exportar
nada**, enquanto o app estiver aberto:

- **Linux e macOS:** o botão **Usar no ssh do sistema** (página Agente, ou Configurações ▸
  Chaves SSH e agente) escreve este bloco no **topo** do `~/.ssh/config` (com cópia de segurança
  ao lado) e **Desligar** o remove. Com o app fechado, o socket some, o `Match exec` não casa e
  o `ssh` volta ao que era, sem erro:

  ```
  # >>> tirech-omni: agente (gerado pelo Tirech Omni; "Desligar" na página Agente remove) >>>
  Match exec "test -S '/run/user/1000/tirech-omni/agent.sock'"
      IdentityAgent "/run/user/1000/tirech-omni/agent.sock"
  Match all
  # <<< tirech-omni: agente <<<
  ```

  "Também nos shells" põe o `SSH_AUTH_SOCK` no `.bashrc`, no `.zshrc` e no fish; "Também na
  sessão gráfica" avisa o `systemctl --user` ou o `launchctl`.
- **Windows:** o agente usa o pipe do OpenSSH, e o `ssh.exe` do sistema o acha sem configurar.
  Se o serviço "OpenSSH Authentication Agent" já o ocupa, a página explica e **Copiar o
  comando** copia o que parar o serviço. O Git for Windows traz um `ssh` próprio que não fala
  com esse pipe: o interruptor "Fazer o Git usar o OpenSSH do Windows" aponta o
  `core.sshCommand` para o `ssh.exe` do sistema. No **WSL** nada é feito: use o `ssh.exe` do
  Windows de dentro dele.

### PuTTY, WinSCP e FileZilla (Pageant, só no Windows)

Com "Atender como o Pageant" ligado (o padrão), o agente também fala como o Pageant, pelo pipe
`\\.\pipe\pageant.<usuário>.<hash>` e pela janela oculta "Pageant": o PuTTY, o plink, o WinSCP,
o FileZilla, o KiTTY e o TortoiseGit usam as chaves do cofre sem exportar `.ppk`. Se o
`pageant.exe` do PuTTY já estiver aberto, o app avisa e não disputa: feche o Pageant e use
**Tentar de novo**.

### Bandeja

Fechar a janela principal pergunta "Fechar o Tirech Omni?": **Deixar na bandeja** (o agente, o
Pageant e as sessões continuam), **Fechar** ou Cancelar, com "Não perguntar de novo"; a escolha
fica em Configurações ▸ Geral ▸ "Ao fechar a janela". O ícone da bandeja tem "Abrir o Tirech
SSH", "Travar o cofre", "Agente" (ligar e desligar, com o número de chaves) e "Sair" (que não
pergunta). Um pedido de confirmação do agente traz a janela de volta e a esconde depois. No
GNOME é preciso a extensão AppIndicator (no Cinnamon e no MATE o ícone vai como um PNG em
`$XDG_RUNTIME_DIR/tomni-ui/`, que é o que o painel deles lê); sem bandeja no sistema não há ícone, fechar encerra o
app e ele avisa uma vez. "Iniciar com o sistema, minimizado na bandeja" abre o app escondido no
login (`~/.config/autostart/tirech-omni.desktop` no Linux, um LaunchAgent no macOS, a chave `Run`
no Windows; o argumento é `--bandeja`).

### Instalar no computador

Arquivo ▸ **Instalar no computador…** (e o cartão em Configurações ▸ Geral) abre um diálogo em
que se marca o que o app deixa neste computador. Cada item é aplicado à parte: se um falha, os
outros seguem, e o resultado aparece numa barra de aviso.

- **Copiar para o computador**: `%LOCALAPPDATA%\Programs\Tirech Omni` no Windows (com as
  ferramentas `tirech-omni-*`), `~/.local/bin` no Linux e `~/Applications` no macOS. A cópia
  vira o destino de tudo o que vem abaixo; uma cópia desatualizada (tamanho, data e SHA-256
  diferentes do app aberto) é substituída.
- **Atalho na área de trabalho** e **Atalho no menu** (menu Iniciar, menu de aplicativos).
- **Fixar na barra**: o Cinnamon, o GNOME e o KDE o app faz sozinho; no Windows e no Dock o
  sistema só deixa a própria pessoa fixar, e o app mostra o passo.
- **Abrir com o sistema** (a mesma chave `Run` da bandeja, com `--bandeja`) e, no Windows,
  **Aparecer em Aplicativos instalados** (`HKCU\...\Uninstall\TirechOmni`, cujo botão
  Desinstalar abre o app com `--desinstalar`).

O diálogo mostra o que já está instalado e o botão **Desinstalar…** vira o mesmo diálogo com
tudo desmarcado: o que fica desmarcado é removido. A caixa "Apagar também os meus dados" nasce
desmarcada e é a única que pede confirmação, porque não tem volta. Ao abrir, o app repara o que
mudou de lugar (os atalhos apontam para a cópia instalada quando ela existe). O que a pessoa
pediu fica em `instalacao` no `configuracoes.json` (não vai na exportação das configurações).

No Windows o app não abre janela de console nem para os processos filhos (`sistema::sem_janela`),
e a janela que não cabe no monitor é reduzida e centralizada pela lib.

### Versão portable

O pacote portable (`tirech-omni-portable-windows.zip`, gerado por `empacotar-portable.sh` na raiz
do repositório) traz a pasta `Tirech Omni` com os cinco executáveis e uma pasta
`tirech-omnidata` ao lado deles. **O marcador é a pasta:** com `tirech-omnidata` ao lado do
executável, o app grava tudo nela e não toca em `%APPDATA%` nem em `~/.config`. Vale em
qualquer sistema; sem a pasta, nada muda. A ordem da pasta de dados é: a variável
`TIRECH_OMNI_CONFIG`, a pasta portable, o `pasta.txt`, a pasta padrão.

No modo portable:

- nada é escrito no sistema: sem migração do nome antigo, sem repontar atalhos, sem atalho no
  menu e sem início com o sistema. Somem da tela "Abrir com o sistema", "Atalho no menu",
  "Instalar no computador…", "Mudar a pasta de dados" e as integrações do agente com o sistema
  (a página Agente mostra o endereço do agente para quem quiser apontar o `SSH_AUTH_SOCK` à mão);
- o `known_hosts` fica em `tirech-omnidata/known_hosts`, a pasta de trabalho dos assistentes em
  `assistentes`, a pasta de troca do MCP em `troca` e as cópias da edição de arquivos remotos
  em `temp`. Um caminho escolhido nas Configurações continua valendo como foi escrito;
- o agente e o MCP usam endereços próprios da pasta (no Windows,
  `\\.\pipe\tirech-omni-agent-<h>` e `\\.\pipe\tirech-omni-mcp-<usuário>-<h>`, com `h` o
  começo do SHA-256 do caminho), para não colidir com uma cópia instalada aberta ao mesmo tempo;
- só uma cópia abre por pasta: a segunda encontra o `trava.lock` preso e avisa que o app já está
  aberto com essa pasta;
- o card da pasta de dados diz que o app está no modo portable e, sem senha mestra, sugere
  criar uma: quem tiver a pasta abre o cofre.


### Pacotes para ARM

O `empacotar-arm.sh`, na raiz do repositório, compila num computador x86_64 pelo Docker (as
receitas ficam em `empacotar/`) e deixa em `target/` (ou em `$SAIDA`):

| Pacote | Para |
|---|---|
| `tirech-omni-linux-arm64.tar.gz` | Linux ARM 64 bits (Raspberry Pi 3/4/5 com sistema de 64 bits, servidores ARM) |
| `tirech-omni-linux-armv7.tar.gz` | Linux ARM 32 bits (Raspberry Pi 2/3/4 com sistema de 32 bits) |
| `tirech-omni-portable-windows-arm64.zip` | Windows 11 ARM, no mesmo formato do portable, com a `libunwind.dll` ao lado dos `.exe` |

Os do Linux pedem glibc 2.35 ou mais nova (o contêiner é o Ubuntu 22.04). `./empacotar-arm.sh
linux-arm64` faz um alvo só. O build fica em `target/docker`, separado do build do computador.

### Pacote para macOS

O `empacotar-mac.sh`, na raiz do repositório, roda num Mac com as ferramentas de linha de comando
da Apple (`xcode-select --install`) e o `rustup`. Compila o `aarch64-apple-darwin` e o
`x86_64-apple-darwin`, junta cada executável num universal com o `lipo` e deixa
`target/tirech-omni-macos.dmg` (ou o caminho que receber), com:

- o `Tirech Omni.app`: os cinco executáveis em `Contents/MacOS` (a instalação copia o app e as
  ferramentas juntos), o ícone `assets/icone.icns`, gerado dos mesmos desenhos do `.ico`, e o
  `Info.plist` com o identificador `com.tirech.omni`, o mesmo do LaunchAgent do início com o
  sistema;
- um atalho para `/Applications` e o `THIRD-PARTY-NOTICES.txt`.

Sem `IDENTIDADE`, a assinatura é ad-hoc: o Apple Silicon roda, mas o Gatekeeper não reconhece
quem assinou e, na primeira vez, pede o **Abrir Mesmo Assim** em Ajustes do Sistema ▸
Privacidade e Segurança. Com `IDENTIDADE` (o nome de um certificado "Developer ID Application" no
chaveiro), assina com o runtime endurecido; com `PERFIL_NOTARIZACAO` também (o perfil gravado por
`xcrun notarytool store-credentials`), manda o `.dmg` para a Apple notarizar e grampeia o
resultado, e o app abre sem aviso. O pacote pede o macOS 11 ou mais novo.

### Copiar a chave para o servidor

**Copiar para o servidor…** (Ferramentas, ou nas chaves) é o equivalente do `ssh-copy-id`.
Escolhe a conexão SSH e a chave pública, usa a senha do cofre (ou a digitada no diálogo),
instala no `authorized_keys` sem duplicar ("Forçar" instala mesmo assim) e, se a conexão ainda
não tem chave, oferece passar a usar a copiada. Em servidores Windows usa o arquivo dos
administradores.

Os mesmos recursos de chave existem na linha de comando no pacote `crates/tirech-omni-tools`
(`tirech-omni-keygen`, `tirech-omni-agent`, `tirech-omni-add` e `tirech-omni-copy-id`).

## Assistentes de IA no servidor

Claude Code, Codex e OpenCode rodam **na máquina remota**, onde estão o código e as ferramentas.
Isto é diferente de "Assistentes controlando o app" (um assistente do seu computador controla o
app) e da IA dentro do app (o app fala com um modelo): aqui o app só instala e abre os três no servidor, em abas comuns de terminal.

### O submenu

**Ferramentas › Assistentes de IA** só aparece quando a aba ativa (ou a conexão clicada na árvore)
é SSH. Cada ferramenta tem um item cujo texto diz o estado: **Abrir Claude Code** (instalado),
**Instalar Claude Code…** (falta), **Claude Code (verificando…)** e **Claude Code (não deu para
verificar)**. Mais abaixo: **Abrir em outra pasta…**, **Entrar** (o login de cada uma, só para as
que estão instaladas) e **Verificar de novo**. O botão de brilho no cabeçalho e o **Ctrl+Alt+A**
(o assistente preferido, em Configurações) fazem o mesmo.

A detecção roda por `exec` (um script mandado pelo stdin do `sh -s`, sem digitar nada na sua aba)
e guarda o resultado por 10 minutos por conexão: sistema, arquitetura, `PATH` do shell de login,
`node`, `npm`, `curl`, `tar`, e o caminho, a versão e o login de cada ferramenta. Se o servidor for
Windows (o `sh` não existe, ou o **Shell do servidor** da conexão é PowerShell ou Cmd), a mesma sondagem
roda em PowerShell e os comandos de abrir e instalar também.

### Instalar

O diálogo mostra o método (o instalador oficial, npm ou Homebrew; o automático escolhe o melhor
para aquele servidor), o **comando exato** que vai rodar e o que falta (sem `curl`, sem `npm`,
Node antigo, arquitetura sem binário). Os avisos não impedem: você escolhe o método e segue. O
comando roda numa aba nova, sem `sudo`; se o `npm` não pode gravar na pasta global, o app usa
`--prefix ~/.local`. No fim o app verifica de novo e, com **Abrir quando terminar** ligado, abre
sozinho; desligado, mostra o aviso com **Abrir agora**.

| Ferramenta | Instalador oficial (Unix) | Windows | Conferido |
|---|---|---|---|
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` | `irm https://claude.ai/install.ps1 \| iex` | 19/10/2026 |
| Codex | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh` | `irm https://chatgpt.com/codex/install.ps1 \| iex` | 19/10/2026 |
| OpenCode | `curl -fsSL https://opencode.ai/install \| bash` | só npm | 19/10/2026 |

O npm (`npm install -g @anthropic-ai/claude-code`, `@openai/codex`, `opencode-ai`) e o Homebrew
(`brew install --cask claude-code`, `--cask codex`, `anomalyco/tap/opencode`) são as alternativas.
O catálogo, com os links da documentação, fica em `src/ia.rs`: quando os comandos mudarem, é lá
que se atualiza (e a data).

### Abrir

Numa aba nova (padrão), num painel livre da grade ou **nesta aba** (digita o comando no terminal
que já está aberto). A pasta vem, nesta ordem, da que o terminal informa (OSC 7, abaixo), da
**Pasta** da conexão (a categoria **Assistentes de IA** nas Propriedades, que **Abrir em outra
pasta…** também grava, com a caixa "Lembrar para esta conexão"), da pasta remota padrão da
conexão ou do home. Uma pasta que não existe no servidor cai no home. **Abrir em outra pasta…**
pergunta a ferramenta e a pasta. Com **Ficar num shell ao sair do assistente** (ligado), a aba
continua num shell quando a ferramenta termina.

Para o app saber a pasta em que o terminal está, o shell precisa avisá-la pelo OSC 7. No bash:

```sh
PROMPT_COMMAND='printf "\033]7;file://%s%s\033\\" "$HOSTNAME" "$PWD"'${PROMPT_COMMAND:+";$PROMPT_COMMAND"}
```

Sem o aviso, o app usa a pasta lembrada ou a remota padrão.

### Entrar sem navegador no servidor

| Ferramenta | Como |
|---|---|
| Claude Code | O `claude` mostra a URL: **Ctrl+clique** abre no navegador do seu computador; o navegador mostra um código, que você cola no terminal (Ctrl+Shift+V). **Entrar** roda `claude setup-token` (um token de um ano, para `CLAUDE_CODE_OAUTH_TOKEN`). |
| Codex | Sem login, a aba abre com um túnel da porta **1455** (`ia.tunel_codex`, ligado): a URL abre com Ctrl+clique e o retorno em `localhost:1455` atravessa o túnel. Se a porta já estiver em uso aqui, o app avisa e abre sem o túnel. **Entrar** roda `codex login --device-auth` (a conta do ChatGPT precisa permitir). |
| OpenCode | **Entrar** roda `opencode auth login`; na tela do `opencode`, `/connect`. |

Um programa no servidor também pode pôr texto na sua área de transferência pelo OSC 52 (o `c` do
`claude` copia a URL), sempre só escrita: o terminal nunca lê a área de transferência.

### Configurações

**Configurações › Assistentes no servidor**: o assistente do Ctrl+Alt+A, onde abrir, ficar
num shell ao sair, o túnel do Codex, verificar ao abrir uma aba SSH, o botão do cabeçalho, quais
ferramentas mostrar, o método de instalação e os argumentos de cada uma (`--model sonnet`, por
exemplo). Por conexão (categoria **Assistentes de IA** nas Propriedades): pasta, método e
argumentos, que substituem os globais.

### O que ainda não faz

- Não atualiza nem desinstala as ferramentas, não instala o Node nem pacotes do sistema, e nunca
  usa `sudo`.
- Só para conexões SSH (não Telnet, serial, terminal local, FTP, S3 nem rclone).
- Num macOS, o login do Claude Code fica no Keychain: o app não sabe se está logado (e não bloqueia
  nada por isso).
- Os testes automáticos usam um servidor falso; instalar e abrir de verdade, num Debian, num
  Alpine, num macOS e num Windows, é o roteiro manual que o plano registra.

## Assistentes controlando o app (MCP)

Assistentes de código que rodam no seu computador (o Claude Code, o Codex e qualquer cliente
MCP) podem operar o app: listar e abrir conexões, ler a tela de uma aba, digitar, mandar teclas,
esperar um texto, rodar comandos, mexer em arquivos e ligar túneis. O app abre a conexão com a
senha salva (o assistente nunca vê a senha) e a pessoa continua vendo tudo nas abas. Isso é
diferente da IA dentro do app: lá o app fala com um modelo e a IA só sugere; aqui um assistente de
fora comanda o app, e quem decide o quanto ele pode é você, conexão por conexão.

### O que é

O executável do app tem um subcomando, `tirech-omni mcp`, que é a **ponte**: o assistente a roda
como um servidor MCP por `stdio`, e ela repassa cada chamada ao app aberto por um canal local (um
socket Unix com 0600 numa pasta 0700, ou um pipe `\\.\pipe\tirech-omni-mcp-<usuário>` do Windows,
que só o seu usuário abre). A ponte não decide nada: toda permissão, aprovação e redação acontece
dentro do app. Com o app fechado, ou com o controle desligado, a ponte avisa que o Tirech Omni não
está aberto e tenta de novo na chamada seguinte.

### Como ligar

1. **Configurações › Assistentes de IA › Deixar assistentes controlarem o app**, ou o item
   **Ferramentas › Assistentes de IA › Ligar o controle por assistentes**. O controle vem
   **desligado**; a primeira vez mostra um aviso que explica o que o assistente poderá fazer.
   Desligado, o app nem abre o socket.
2. Escolha a permissão das conexões e das pastas (abaixo). Nasce tudo em **Desligada**: um
   assistente não vê nenhuma conexão enquanto você não liberar.
3. Abra **Como ligar um assistente…** (botão ali mesmo, ou o menu Ferramentas) e copie o comando do
   seu cliente, com o caminho do executável que está rodando:

   ```sh
   claude mcp add tirech-omni -- /caminho/para/tirech-omni mcp
   codex mcp add tirech-omni -- /caminho/para/tirech-omni mcp
   ```

   O diálogo também traz o `.mcp.json` do Claude Code e o trecho do `config.toml` do Codex. Os
   comandos foram conferidos com o Claude Code 2.1.308 e o Codex 0.160.0.
4. No cliente, `/mcp` mostra o servidor conectado e as ferramentas.

A ponte aceita `--nome <nome>` (o nome da sessão, que aparece na aba e no diálogo) e
`--espaco <nome>` (prende o assistente a um espaço de trabalho, abaixo).

### O que cada permissão faz

A permissão fica no formulário da conexão e da pasta (categoria "Controle por assistentes (MCP)") e
vale para as conexões de dentro da pasta, como a rede: **Herdar da pasta** (o padrão; na raiz vale
Desligada), ou uma destas:

| Permissão | O assistente pode |
|---|---|
| **Desligada** | nada: a conexão não aparece para ele |
| **Só ler** | ver a conexão, as abas, a tela, os arquivos e os túneis; não digita, não roda, não grava, não abre aba |
| **Aprovar** | tudo, mas cada ação que muda algo espera o seu OK no diálogo do app |
| **Livre** | tudo sem perguntar, **menos** o que a lista de bloqueio pega |

O diálogo de aprovação mostra o comando exato (os caracteres de controle aparecem como `<ESC>`,
`<BEL>`…, nada fica escondido) e a conexão. Dá para aprovar só aquela ação, marcar "Permitir o
resto desta sessão nesta aba" ou recusar (com um motivo, que vai para o assistente). A lista de
bloqueio ignora a marca: o comando perigoso pergunta de novo.

**Abrir abas novas**, na mesma categoria, decide se o assistente pode abrir uma aba nova da
conexão: o `abrir_conexao`, o `ligar_tunel` sem aba aberta e o espaço de trabalho sem a aba
"(IA)" de pé. **Permitir** (o padrão) segue a permissão; **Perguntar** pede o seu OK a cada aba
nova, mesmo em Livre; **Bloquear** recusa, e o assistente usa só as abas que você abrir.
Reconectar uma aba que caiu não conta. **Herdar** segue a pasta e, sem pasta que escolha,
Configurações › Controle por assistentes (MCP) › Abrir abas novas. Quando a aba nova pergunta, o
diálogo oferece "Abrir as abas novas desta sessão sem perguntar, por 1 hora": o tempo vem de
"Abrir sem perguntar depois de aprovar" (Desligado, 15 ou 30 minutos, 1, 4, 8, 12 ou 24 horas,
ou Sempre abrir, que vale até a sessão acabar), e "Parar a IA" desfaz.

A **lista de bloqueio** pega os comandos perigosos mais comuns (`rm -rf` e parentes, `mkfs`, `dd`
em disco, `shutdown`, `curl … | sh`, mexer em contas e no `authorized_keys`, apagar o histórico,
derrubar o firewall…) depois de tirar `sudo`, `env`, atribuições e `bash -c`, e **sempre pergunta,
mesmo em Livre**. Você acrescenta os seus em "Comandos que sempre pedem aprovação" (um programa ou
`re:` com uma expressão regular). **Ela não é uma garantia**: um comando escrito para escapar
dela escapa. Para uma conexão que não pode errar, use "Aprovar" ou "Só ler".

### Ferramentas

| Ferramenta | O que faz | Exige |
|---|---|---|
| `listar_conexoes`, `listar_abas`, `listar_paineis` | o que existe e o que está aberto | ver |
| `abrir_conexao`, `fechar_aba`, `reconectar_aba`, `organizar_paineis` | abre uma aba (num painel à escolha), fecha, reconecta a mesma aba caída, ajusta a grade | mudar |
| `ler_tela`, `esperar`, `ler_saida` | lê a tela, espera por um texto ou silêncio, pega o resto de uma resposta cortada | ver |
| `digitar`, `teclas` | digita (com ou sem colar entre colchetes) e manda `Enter`, `Ctrl+C`, `F2`… | mudar |
| `executar` | roda um comando na aba, à vista (espera o prompt), ou à parte (com o código de saída); o "à parte" vai para a aba quando ela está livre no prompt, salvo se "Rodar os comandos dos assistentes na aba" estiver desligado, e a resposta diz onde rodou (`modo`) | mudar |
| `listar_arquivos`, `baixar_arquivo`, `enviar_arquivo` | arquivos da conexão pelo SFTP, FTP, S3 ou rclone | ver / mudar |
| `listar_tuneis`, `ligar_tunel`, `desligar_tunel` | túneis da conexão | ver / mudar |
| `listar_portas`, `encaminhar_porta`, `parar_porta` | o que escuta no servidor e o túnel de uma porta dele (com nome de site e HTTPS, se pedir) | ver / mudar |
| `listar_espacos`, `ler`, `editar`, `escrever`, `buscar`, `glob`, `bash` | o espaço de trabalho remoto (abaixo) | ver / mudar |

Uma aba que um assistente usa (aberta por ele, presa pelo "Controlar com" ou mexida há pouco)
reconecta sozinha quando cai, com as tentativas e a espera de Configurações > Conexões > Ao cair
a conexão, qualquer que seja o modo escolhido ali: o assistente não clica no aviso. Enquanto
espera, `listar_abas` mostra o estado `reconectando` com a tentativa, e as ferramentas que pedem
a aba de pé dizem o mesmo. `reconectar_aba` reabre na hora.

Não há teto fixo de tamanho: uma resposta maior que "Tamanho máximo de uma resposta" (Configurações) vem
cortada, com o id para pedir o resto por `ler_saida`; zero não limita.

### O que a redação pega e o que não pega

"Tirar senhas e chaves do que vai para a IA" vem **ligado** e troca por
`[segredo removido]` tudo o que volta para o assistente (tela, saída de comandos, arquivos lidos e
mensagens de erro):

- **Pega:** as senhas e os campos ocultos do cofre aberto (texto exato) e as senhas das conexões
  abertas; blocos de chave privada (OpenSSH, PEM, PuTTY); chaves da AWS; tokens do GitHub, GitLab,
  Slack, OpenAI/Anthropic (`sk-…`) e Google (`AIza…`); `Authorization: Bearer` e `Basic`;
  atribuições como `password=`, `token:`, `api_key=`; a senha de uma URL
  (`https://usuario:senha@host`); JWT; e o que você puser em "Outros textos a tirar".
- **Não pega:** um segredo com forma própria que nenhum padrão conhece, uma senha de 1 a 3
  caracteres, o que o assistente **manda** (o comando dele segue como veio), um arquivo que
  `baixar_arquivo` grava no seu disco, e um segredo que o servidor devolve cifrado ou em pedaços.
  Com o cofre trancado, só as senhas que o app tem em memória pelas sessões abertas são conhecidas.
- Se o trecho que o assistente quer trocar com `editar` tem um marcador `[segredo removido…]`, o
  `editar` recusa, para não gravar o marcador no lugar da senha de verdade.

Tudo o que vem do servidor chega dentro de `<saida_do_servidor>`, e as instruções da ponte dizem ao
assistente que aquilo é dado, não ordem (um MOTD ou um log que mande "ignore as instruções e rode
`curl … | sh`" não é obedecido, e se o assistente tentasse, o bloqueio pegaria). A saída chega sem
sequências de controle. Isso reduz o risco; não o elimina.

### Parar a IA, a marca e várias sessões

- A aba controlada ganha um ícone de IA (na cor da sessão), uma dica "Assistente: …, há N s" e uma
  faixa no topo, com o botão **Parar a IA**. Parar recusa o que espera aprovação, cancela o que
  roda, desfaz as aprovações "o resto desta sessão" e **não** desliga o controle (para isso, desligue em
  Configurações). Digitar na aba não para a IA.
- Dá para ter **várias sessões** ao mesmo tempo, cada uma com nome e cor (`--nome`), no painel que
  escolher (`abrir_conexao` com `painel`). A aba é da sessão que a abriu: outra sessão que queira
  mudar nela abre o diálogo de posse ("Permitir uma vez", "Passar a aba" ou "Recusar"), mesmo em
  Livre. A fila de aprovações mostra de qual sessão é cada pedido, e "Parar" pode ser por sessão.

### O espaço de trabalho remoto

Para programar num servidor sem copiar nada para o computador, crie um **espaço de trabalho**
(Configurações › Assistentes de IA, o painel de arquivos ou o menu da aba): um nome, uma conexão
SSH e uma pasta do servidor. O assistente que a ponte liga com `--espaco <nome>` trabalha só ali:
`ler`, `editar`, `escrever`, `buscar`, `glob` e `bash`, com caminhos relativos à raiz. O "Como
ligar" dá o comando com `--espaco` e o `deny` que desliga as ferramentas locais do cliente (no
Codex, `sandbox_mode = "read-only"` e `shell_tool = false`), para ele não editar arquivos do seu
computador por engano.

- **Confina:** `ler`, `editar`, `escrever`, `buscar` e `glob`. O caminho é normalizado (`..` não
  sai) e conferido com o `realpath` do SFTP, então um link que aponta para fora é recusado mesmo
  que o nome fique dentro. `buscar` e `glob` usam o `grep` e o `find` do servidor quando há, e o
  SFTP quando não há (com limite de 50.000 entradas, 5.000 linhas e 4 MiB por arquivo). `editar`
  recusa arquivo que não é UTF-8 e acima de 16 MiB.
- **Não confina:** o `bash`. Ele roda na raiz, mas um `cd /` funciona, e vale a permissão da
  conexão. Para um confinamento de verdade, use uma conta do servidor que só enxergue a pasta. Também não
  fecha a janela entre conferir um caminho e gravar (outro processo **no servidor** pode trocar
  uma pasta por um link nesse instante): o espaço confina o assistente, não alguém que já está
  dentro do servidor. E, no Windows, o `bash` roda por `cmd` e as buscas vão pelo SFTP.
- Cada `bash` é um processo novo (o `cd` de uma chamada não vale para a próxima), sem terminal
  (programas interativos não funcionam). Um processo que ignora o cancelamento pode ficar no servidor.

### Controlar uma aba com o Claude Code, o Codex ou o OpenCode

Um clique faz tudo: **Controlar com ▸ Claude Code / Codex / OpenCode**, no clique direito do
terminal, no menu da aba e em Ferramentas › Assistentes de IA (que controla a aba atual).

- Se o controle por assistentes estiver desligado, o clique o liga (com o mesmo aviso de
  sempre, na primeira vez) e uma barra avisa, com o botão **Desligar**.
- O app abre, à direita da aba, uma aba de shell local rodando o assistente **deste
  computador**, já ligado ao app por MCP (`tirech-omni mcp --nome … --aba <chave>`) e preso
  àquela aba: ele vira o dono dela, ganha **Livre** só nela enquanto estiver ligado e usa essa
  aba quando não diz outra (o `listar_abas` a marca com `alvo: true`). Vale também para o shell
  local avulso e para uma conexão com a permissão Desligada, sem mexer na permissão salva. A
  lista de bloqueio continua pedindo aprovação.
- O MCP vai pela linha de comando de cada assistente (`--mcp-config` no Claude Code, `-c
  mcp_servers…` no Codex, `OPENCODE_CONFIG_CONTENT` no OpenCode): **a configuração do
  assistente não é alterada** (nada é gravado no `~/.claude.json`, no `~/.codex/config.toml`
  nem no `opencode.json`). No Claude Code o servidor se chama `tirech-omni-aba`, para não
  colidir com o que a pessoa já ligou à mão.
- Clicar de novo, na mesma aba e com o mesmo assistente ainda aberto, só traz a aba dele para a
  frente. O submenu não aparece na aba do próprio assistente.
- **O diário das conversas**: o app guarda o id de cada conversa que um assistente teve com uma
  aba (pela conexão dela; o shell local avulso tem o seu), no `conversas.json`. O Claude Code
  abre com `--session-id` (o app escolhe o id) e continua com `--resume`; o Codex avisa o id da
  conversa nova pelo `notify` (o app chama o `notify` que você já tinha) e continua com `codex
  resume <id>`; no OpenCode o app acha a sessão pelo `opencode session list` e continua com
  `--session`. O "Controlar com" numa aba que já conversou com aquele assistente pergunta
  **Continuar a conversa?**: até 5 conversas, a mais recente marcada, cada uma com a primeira
  mensagem e a data, mais **Nova conversa** e **Lembrar a escolha**. Sem conversa anterior, abre
  direto. As conversas que o assistente apagou (o Claude Code apaga as de mais de 30 dias) saem
  da lista.
- Fechar o app com assistentes abertos e abrir de novo traz os assistentes de volta, cada um à
  direita da sua aba, **continuando a conversa**, sem perguntar; abrir um espaço de trabalho
  também. Em Configurações › Assistentes de IA › **Assistentes neste computador** dá para desligar o diário
  (`conversas_lembrar`), a volta dos assistentes (`conversas_reabrir`) e escolher o que o
  "Controlar com" faz numa aba que já conversou (`conversas_ao_controlar`: Perguntar, Continuar
  a mais recente ou Nova conversa).
- **Dar esta aba também a ▸** (nos mesmos três lugares) prende mais abas a um assistente que já
  está aberto: lista as sessões ligadas pelo nome, e o clique faz da escolhida a dona da aba (sem
  perguntar) e dá a ela o **Livre** só ali, como no "Controlar com" (também no shell local avulso
  e numa conexão Desligada; a lista de bloqueio continua pedindo aprovação). A aba padrão dela não
  muda, e o `listar_abas` marca a aba dada com `dada: true`. A liberação some quando o assistente
  sai. O submenu não aparece sem sessão ligada nem na aba de um assistente, e não lista quem já
  tem a aba.
- No Claude Code, os avisos do app (a aba dada, os arquivos soltos na aba) chegam junto da
  próxima mensagem que você manda, sem esperar o assistente chamar uma ferramenta: o app lança o
  Claude com `--settings` apontando para um hook `UserPromptSubmit` (`tirech-omni mcp --avisos
  <código>`), num arquivo `lancar-<pid>-<n>-avisos.json` ao lado do `lancar-<pid>-<n>.json`. As suas
  configurações do Claude Code não mudam.
- O servidor do app também oferece três comandos com barra no Claude Code (prompts do MCP):
  `/mcp__tirech-omni-aba__abas` (as abas abertas e quais ele controla),
  `/mcp__tirech-omni-aba__conectar web1 web2` (abre as conexões, uma por painel, e arruma a
  grade) e `/mcp__tirech-omni-aba__rodar-em-todas df -h` (roda o comando em cada aba que ele
  controla e compara as saídas). Com o servidor ligado à mão, o prefixo é o nome que você deu a
  ele (`/mcp__tirech-omni__abas`).
- Quando o assistente sai, a liberação e o dono somem, e a aba fica encerrada (o **Reconectar**
  abre de novo). Fechar a aba alvo não fecha o assistente.
- Se o assistente não está instalado, uma barra diz isso, com **Como instalar**, e a aba nova
  fecha sozinha. Em Configurações › Assistentes de IA › **Assistentes neste computador** dá para
  indicar o caminho do executável e os argumentos extras de cada um (por exemplo o
  `--dangerously-skip-permissions` do Claude Code; o app não sugere nenhum). O caminho é
  daquele computador e não vai na exportação das configurações.
- O assistente começa na **pasta de trabalho dos assistentes**, que se escolhe na mesma seção.
  Vazia, é uma pasta do app no cache do sistema (`~/.cache/tirech-omni/assistentes` no Linux,
  `~/Library/Caches/tirech-omni/assistentes` no macOS, `%LOCALAPPDATA%\Tirech Omni\assistentes`
  no Windows), criada ao abrir. O que o assistente deixa ali (rascunhos, scripts, clones) não
  some sozinho: **Abrir a pasta de trabalho** mostra o que tem e **Limpar a pasta de
  trabalho…** apaga tudo o que está dentro, depois de dizer quantos itens e onde. O app recusa
  limpar a pasta pessoal e a raiz.
- No Linux e no macOS o assistente roda pelo shell de login (`$SHELL -l -c exec …`), para
  achar o `PATH` do seu terminal, mais as pastas onde os instaladores põem o programa
  (`~/.local/bin`, `~/.opencode/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `~/.volta/bin`, os
  shims do asdf e do mise, `/opt/homebrew/bin` e `/usr/local/bin`), no fim: o instalador do
  OpenCode escreve o `PATH` no `~/.bashrc`, que o shell de login não interativo não lê até lá.
  No Windows, o app procura no `PATH` e também em
  `%USERPROFILE%\.local\bin` e `%APPDATA%\npm`, e um `.cmd` roda por `cmd /c`.

### Cluster com IA

Ferramentas › Modo cluster › **Cluster com IA…** (ou "Abrir em cluster com IA…" no clique
direito de um grupo ou de uma conexão) abre as máquinas escolhidas numa janela, lado a lado, e
um assistente deste computador num painel ao lado, para controlar todas de uma vez.
No clique direito de uma pasta, **Abrir todas controladas por IA ▸** Claude Code, Codex ou
OpenCode faz o mesmo com todas as conexões de terminal da pasta, sem passar pelo diálogo (com o
controle desligado, o aviso do controle vem antes, como no diálogo).

- O assistente é lançado como no "Controlar com" (mesma linha de comando, mesma pasta de
  trabalho, mesmos avisos), mas a ponte vai com `--cluster` e sem `--aba`: ele não tem aba
  padrão, e as instruções dizem quais são as máquinas e que cada ferramenta de terminal leva o
  argumento `aba`.
- Cada aba de máquina que abre na janela é **dada** a ele (como o "Dar esta aba também a"):
  vira dele, com o **Livre** só ali, e o aviso chega no Claude Code junto da próxima mensagem.
  As abas que já estavam na janela antes (quando só cabe uma janela) e a do próprio assistente
  não entram.
- Com o controle por assistentes desligado, passa antes pelo aviso de ligar, e o cluster abre
  depois do aceite.
- É separado dos espaços de trabalho: só fica guardado se você salvar a janela como espaço.

### Auditoria

Cada pedido vira uma linha em `mcp/AAAA-MM-DD.jsonl` na pasta de dados (0600 no Unix): quando,
ponte, sessão, ferramenta, conexão, aba, argumentos (com a senha e o conteúdo enviado redigidos),
decisão, motivo, resultado e duração. **Ver o registro de hoje** (Configurações) abre o arquivo; "Gravar o registro do que os
assistentes fazem" desliga.

### O que ainda não faz

- Não confina o `bash` do espaço (acima) nem protege de quem já está dentro do servidor.
- Não tem ferramenta de processos em segundo plano: use `nohup … > log 2>&1 &`.
- A pasta espelho (uma cópia local do espaço de trabalho, sincronizada) não foi feita.
- O "Controlar com" no Windows e no macOS não foi conferido numa máquina de verdade.
- A ponte no Windows foi conferida só por compilação; o teste do pipe roda no CI do Windows e o
  roteiro manual com um Windows real, o Claude Code e o Codex reais fica para quem os tem.

## Onde ficam os arquivos

Na pasta de configuração do sistema (`~/.config/tirech-omni` no Linux), ou na pasta que a
variável `TIRECH_OMNI_CONFIG` apontar:

- `connections.json`: as conexões e os grupos, gravado na hora, de forma atômica (cada gravação
  num `.tmp` próprio, que só substitui o arquivo inteiro). Se estiver corrompido, vai para
  `connections.json.bak` (ou `.bak-<data e hora>`, sem pisar numa cópia de antes) e o app
  avisa. Campos que esta versão não conhece (de uma versão mais nova) continuam no arquivo ao
  gravar; um arquivo com `version` maior que a desta versão é lido no que se entende e não é
  sobrescrito. Quando outro programa (outra cópia do app, um editor, a sincronização) muda o
  arquivo, o app relê sozinho em poucos segundos; se a mudança de fora chega entre a leitura e
  uma gravação daqui, a versão de fora fica guardada como `connections.json.conflito-<data e
  hora>` e o app avisa.
- `estado.json`: todas as janelas, com a grade e as conexões abertas em cada painel, e a sessão
  do tmux das abas com a sessão persistente (o formato de um espaço; o de uma janela só, das primeiras versões, ainda é lido). Guarda também o layout dos painéis
  com alfinete de cada página (Terminais, Notas e Arquivos: modo, lado, tamanhos, ordem e a
  janela dos flutuantes). Com duas cópias do app abertas na mesma pasta, só a primeira restaura
  e grava este arquivo: a segunda abre sem as abas (elas já estão abertas na primeira) e é só de
  leitura (ver `trava.lock`).
- `conversas.json`: o diário das conversas dos assistentes do "Controlar com", por aba (o id da
  conexão; `0` é o shell local avulso): o assistente, o id da conversa nele, quando foi aberta e
  a primeira mensagem. Até 20 por aba e assistente. Apagar só faz o app esquecer as conversas
  (elas continuam nos assistentes).
- `espacos/`: os espaços de trabalho, um `<nome>.json` por espaço, em subpastas se houver
  pastas. É uma pasta comum: dá para copiar, versionar e mexer pelo gerenciador de arquivos
  (Atualizar, no grupo da árvore, relê).
- `pastas.json`: a última pasta usada em cada seletor de arquivo (importar chaves, o cofre, as
  redes…), para o seletor abrir nela da próxima vez. Apagar só faz os seletores voltarem às
  pastas padrão.
- `recentes.json`: as conexões abertas por último e as fixadas no Início. Corrompido, vai para
  `recentes.json.bak` e o app avisa.
- `configuracoes.json`: todas as configurações da página Configurações, sem segredo nenhum. Um
  campo ruim volta ao padrão sozinho (com um aviso na página); campos que o app não conhece são
  mantidos ao gravar. Corrompido, vai para `configuracoes.json.bak` e o app avisa.
- `pasta.txt` (na pasta de configuração do sistema, não na de dados): o caminho da pasta de dados,
  quando ela foi mudada pela página Configurações.
- `trava.lock`: a trava da primeira cópia aberta com a pasta, a dona da configuração. No modo
  portable, ela impede a segunda cópia. Fora dele, a segunda abre **só de leitura**, como no
  WinSSHTerm: uma barra no topo avisa, e nada da pasta de dados nem do cofre é gravado por ela
  (estado, espaços, anexos, recentes, a cópia do dia, a limpeza dos arquivos do "Controlar
  com"). O que se muda nela nas conexões, nas configurações, nos provedores de IA e no cofre vai
  para a dona pelo canal entre as cópias (`instancias.sock`, abaixo), que grava; o resto fica na
  memória e se perde ao fechar. O que a dona grava aparece sozinho na segunda. Quando a dona
  fecha, a barra diz que dá para gravar depois de reiniciar, e o botão Reiniciar da barra abre
  esta cópia de novo, agora como dona. A ponte `tirech-omni mcp` também não grava nada da pasta.
- `instancias.sock` (Unix): o canal entre as cópias do app na mesma pasta, sempre no ar na
  dona (independe do MCP). No Linux fica em `$XDG_RUNTIME_DIR/tirech-omni/instancias-<resumo
  da pasta>.sock`; no macOS, sem essa variável, com `TIRECH_OMNI_CONFIG`, no modo portable ou
  com a pasta de dados mudada, na própria pasta. No Windows é o pipe
  `\\.\pipe\tirech-omni-instancias-<usuário>-<resumo>`. A cópia só de leitura manda por ele o
  `connections.json`, o `configuracoes.json`, o `ia.json` e o `vault.json` (já cifrado) inteiros
  e o resumo SHA-256 do que leu; a dona confere (o tipo do arquivo, a mesma pasta, o JSON; no
  cofre, a mesma senha mestra ou chave), grava só se o arquivo ainda é o lido e relê. Se mudou, a
  secundária junta a mudança dela com a do disco (pela base, o que ela leu por último; nos dois
  lados o mesmo campo vira conflito, com aviso, e fica o do disco) e manda de novo, até cinco
  vezes. Dois itens novos com o mesmo id: o da secundária ganha outro. Pelo mesmo canal, a
  secundária pede a VPN embutida de um perfil: a dona liga o motor (ou usa o que está ligado) e
  responde com a ponte SOCKS5 dele (`127.0.0.1`, porta e credencial dela), os avisos de subida e
  "pronta"; a ligação fica aberta enquanto a secundária usa a rede, e a dona conta esse uso.
- `agente.json`: se o agente embutido liga com o app, o tempo de vida padrão e as chaves de
  arquivo que ele carrega ao iniciar (só o caminho do arquivo, a confirmação e a vida; nunca a
  passphrase). As chaves do cofre não entram aqui: ficam no próprio `vault.json`. O socket do
  agente (`agent.sock`) fica em `$XDG_RUNTIME_DIR/tirech-omni/` (ou o `tirech-omni-agent.sock` em
  `~/.ssh`) quando o app usa a pasta de configuração padrão; com `TIRECH_OMNI_CONFIG`, fica na
  pasta.
- `redes/<id>/trava.lock`: a trava de um perfil de rede embutida (OpenVPN, WireGuard, Tailscale,
  Nebula e ZeroTier) enquanto o motor dele está ligado, até ele parar de fato (um motor que as
  abas ainda seguram fica com a trava). Duas cópias do app abertas não ligam a mesma rede juntas
  (com a mesma identidade, uma derrubaria a outra no servidor): a cópia só de leitura pede a rede
  à dona pelo `instancias.sock` e sai pela ponte do motor dela, que fica ligado enquanto a
  secundária usa. Sem a dona no ar, a secundária liga o motor ela mesma; se uma dona aberta
  depois achar a trava presa por ela, avisa que a rede já está ligada na outra cópia.
- `redes/<id>/known_hosts`: as chaves de servidor de cada perfil de rede (o mesmo IP tem chave
  diferente em cada rede). Os perfis em si ficam no `connections.json`.
- `navegador/<id>/`: o perfil do navegador separado de cada perfil de rede (a pasta que o
  Chromium recebe em `--user-data-dir` ou o Firefox em `--profile`, com o `user.js` do proxy):
  histórico, cookies e logins das páginas do cliente ficam aqui, longe do navegador de sempre.
  Apagar só faz o próximo link começar num navegador limpo.
- `navegador/portas.json`: a porta do proxy local de cada perfil de rede, para o navegador
  achar o proxy na mesma porta depois de reiniciar o app.
- `ia.json`: os provedores de IA (nome, URL, dialeto, modelo, rede, TLS e cabeçalhos) e o
  provedor padrão. **A chave da API não entra aqui**: fica num item do cofre ("Chave de API — …")
  e o perfil guarda só o id dele. Corrompido, vai para `ia.json.bak` e o app avisa.
- `mcp/lancar-<pid>-<n>.json`, `mcp/lancar-<pid>-<n>-avisos.json` e `mcp/conversa-<pid>-<n>.json`: os arquivos de
  cada lançamento do "Controlar com" (a ponte, o hook dos avisos do Claude Code e o id da
  conversa nova do Codex, que o `notify` grava). Ficam enquanto o app está aberto (o
  Reconectar reusa) e são apagados na próxima abertura.
- `mcp/AAAA-MM-DD.jsonl`: o registro de auditoria do MCP, uma linha por pedido de um
  assistente: quando, ponte, sessão, ferramenta, conexão, aba, argumentos (com a senha e o
  conteúdo enviado redigidos), decisão (permitida, aprovada, recusada…), motivo, resultado e
  duração. Gravado com 0600 no Unix; `ia.auditoria` desliga.
- `mcp.sock` (Unix): o canal do app com a ponte `tirech-omni mcp`, só enquanto `ia.mcp_ligado`
  está ligado. No Linux fica em `$XDG_RUNTIME_DIR/tirech-omni/`; sem essa variável e no macOS,
  com `/run/user/<uid>/tirech-omni/` quando a pasta existe, e senão (e no macOS)
  na pasta de configuração, com 0700 na pasta e 0600 no socket. O "Controlar com" e o cluster
  com IA passam o endereço à ponte (`--canal`), porque o Codex roda os servidores MCP sem o
  `XDG_RUNTIME_DIR` nem o `TIRECH_OMNI_CONFIG`. Campos de `ia` do MCP no `configuracoes.json`:
  `mcp_ligado`, `bloqueio_extras` (programas ou `re:` expressões que sempre pedem aprovação),
  `auditoria`, `marca_secs` (quanto tempo a aba fica marcada, de 1 a 3600), `caracteres_por_resposta`
  (0 não limita), `finais_de_prompt` (além de `#`, `$`, `%` e `>`) e `pasta_troca` (vazia é
  `Downloads/Tirech Omni`), e os do diário das conversas, `conversas_lembrar`, `conversas_reabrir`
  e `conversas_ao_controlar` (`perguntar`, `continuar` ou `nova`). A permissão de cada conexão é o campo `mcp` do `connections.json`
  (`desligada`, `so_ler`, `aprovar` ou `livre`; sem ele, herda da pasta).
- `certificados.json`: as impressões dos certificados FTPS que a pessoa confiou, por
  `host:porta` (ao lado do `known_hosts`).
- `vault.json` e `vault.key`: o cofre de senhas e de chaves (versão 3: pastas e itens com
  `uid`, lápides das remoções e as chaves SSH) e a chave que o abre (some quando há senha
  mestra). O `cofre.json` e o `chave` das versões antigas são renomeados na primeira abertura, e
  os cofres das versões 1 e 2 passam para a 3 na primeira gravação. Gravado com 0600 no Unix. Se
  o `vault.json` estiver corrompido, vai para `vault.json.bak` (ou `.bak-<data e hora>`, nunca
  por cima de uma cópia de antes, com o `vault.key` junto) e o app avisa; um `vault.json` de uma
  versão mais nova do app abre trancado, sem ser sobrescrito. "Usar no lugar
  deste" deixa o cofre anterior como `vault.json.bak-<data e hora>` (e o `vault.key`).
  A pasta do cofre pode ser outra que a de dados ("Local do cofre", em `configuracoes.json`).
- `copias/`: uma cópia por dia do `connections.json`, do `vault.json` e do `vault.key`
  (`<aaaammdd>-<nome>`, o arquivo como estava antes da primeira mudança do dia), só dos últimos 7
  dias. Um arquivo que se perde ou estraga volta com, no máximo, um dia de mudanças a menos:
  feche o app e copie a cópia por cima. A pasta é só do dono no Unix.
- `vault/`: os anexos das notas, um arquivo cifrado por anexo (nome = id), ao lado do `vault.json`.
  Os que ficam sem ficha (um salvar interrompido) são apagados depois de 24 horas.
  Ao trocar de cofre, o `vault/` antigo vira `vault.bak/` (ou `vault.bak-<data e hora>/`).
- `quedas.log`: o pânico e o erro que fecharam o app, com data, sistema e o rastro da pilha;
  recomeça acima de 512 KiB. A queda da thread principal também abre um aviso com o caminho dele.
- `desenho.txt`: a guarda da abertura (`<degrau> ok` ou `<degrau> abrindo`). Se o app caiu antes
  de mostrar a janela, a próxima abertura desenha de um jeito mais simples (o quadro na CPU; no
  Windows, também o DirectX 12 e o OpenGL). Apagar o arquivo volta ao desenho normal.
- `travamentos.log`: quando a interface fica parada mais de 2 s, em que etapa ela estava (um
  evento, o quadro, o layout, os terminais, a textura ou a apresentação da janela), o estado da
  thread da interface, a carga, a memória e o swap e as threads do app que mais gastaram CPU;
  repete a cada 10 s e diz quanto durou. A cada minuto em que a interface usou mais de 30% de
  uma CPU, ou uma etapa passou de meio segundo, o tempo por etapa. Também o aviso da janela que
  pinta sem parar. Recomeça acima de 1 MiB. O estado do sistema só aparece no Linux.
- `vault-orphans.json`: na pasta de dados, os arquivos de `vault/` sem ficha e a hora em que a
  coleta os viu pela primeira vez (a margem de 24 horas dá tempo de a sincronização trazer a
  ficha de outra máquina).
- `*.tirech-vault`: o arquivo de "Levar o cofre para outra máquina…", gravado onde a pessoa
  escolher.
- `logs-cifrados.pub`: a chave pública dos logs cifrados, criada na primeira vez que
  "Cifrar os logs" é ligada. Só a pública fica aqui: a privada é o item "Chave dos logs cifrados"
  do cofre, então gravar um log não precisa do cofre aberto e ler precisa.
- `*.log.cifrado`: os logs de sessão gravados com "Cifrar os logs" ligada, na pasta de logs
  (a mesma dos `.log` em texto). Segmentos com cabeçalho `TRCHLOGS` e registros autenticados
  (AES-256-GCM); a página Logs lê, busca, exporta como texto e converte de um formato para o
  outro.
- `~/.ssh/config`, `.bashrc`, `.zshrc` e `~/.config/fish/conf.d/tirech-omni.fish`: só recebem o
  bloco do agente se a pessoa ligar "Usar no ssh do sistema" (e "Também nos shells"). O bloco é
  marcado por `# >>> tirech-omni: agente` e removido por inteiro ao desligar.

## Configurações

A página **Configurações** (Ctrl+,) segue o padrão das Configurações do Windows 11: cartões numa
rolagem só, com as seções Geral, Terminal, Conexões, Cofre de senhas, Chaves e agente, Arquivos,
FTP, S3, rclone, Redes, X11 e túneis (padrão do X11, display, comando do virt-manager e "Permitir outras
máquinas" nos túneis novos), Janelas e espaços e Atalhos. Cada mudança vale na hora e é gravada no
`configuracoes.json`; o que precisa reiniciar o app (a pasta de dados) tem o botão "Reiniciar agora".

- **Busca**: o campo no topo da página filtra os cartões pelo nome, pela descrição e por termos
  parecidos (digitar "fonte" mostra só o tamanho da fonte). A busca da barra de título (Ctrl+E)
  também acha configurações e leva ao cartão.
- **Restaurar**: cada seção tem "Restaurar os padrões desta seção", e o fim da página tem o de
  tudo. Depois de restaurar, o botão **Desfazer** volta ao que estava. Nada pede confirmação
  nem a senha mestra.
- **Atalhos**: os atalhos do app são editáveis (digite `Ctrl+Shift+T`, por exemplo); um campo
  vazio volta ao padrão, e um atalho repetido avisa na hora.
- **Exportar e importar**: grava e lê as configurações em um JSON. O arquivo não leva senha
  nem item do cofre (o id da credencial do rclone fica de fora). Importar mostra o que foi
  aplicado e o que foi ignorado, e "Desfazer" volta.
- **Pasta de dados**: o cartão mostra onde o app guarda tudo; dá para abrir, mudar (o app copia os
  dados para lá, ou só aponta se a nova já tem um `connections.json`, e passa a usar a pasta
  no próximo início) ou voltar à padrão.
- **Proteção das senhas**: "Limpar a área de transferência" e "Bloquear sem uso" existem e
  nascem **desligadas**. Proteger as senhas é uma escolha de quem usa o app, e o app continua
  salvando senhas. O bloqueio automático só vale com senha mestra e conta
  desde o último uso de uma senha.
- **Desligar a IA ou o MCP**: em Geral, "Assistentes de IA" e "Controle por assistentes (MCP)"
  nascem ligados. Desligado, some tudo da área: seções, menus, linhas das Propriedades, o botão,
  o painel do chat e os atalhos (e, no MCP, o servidor). O que estava configurado fica guardado
  e volta ao ligar de novo.

O que a página ainda não faz: sincronizar entre máquinas, vigiar o arquivo por
mudanças feitas fora do app (reinicie depois de editar o JSON à mão), escolher a família da fonte,
editar as cores dos esquemas, capturar o atalho pressionando as teclas e aceitar uma chave de
servidor que mudou.

## Idioma

A interface vem em **Português (Brasil)**, **English** e **Español**. Em Configurações › Geral ›
Idioma, "Usar o idioma do sistema" é o padrão (`pt*` abre em português, `es*` em espanhol e
qualquer outro idioma em inglês). A
troca vale ao reiniciar: o cartão mostra o aviso e o botão **Reiniciar agora**, que fecha pelo
caminho normal e reabre o app com as abas restauradas. Tamanhos, números, datas e o "há N dias"
dos recentes seguem o idioma; a busca de Configurações acha pelos termos do idioma atual e pelos
do português.

Não se traduzem: o que quem usa escreve (nomes de conexões, pastas, itens do cofre), os arquivos
gravados (guardam chaves, nunca texto de tela), os ids, as mensagens de erro da lib e do sistema
(o app traduz só a frase em volta) e as ferramentas de linha de comando (`tirech-omni-tools`).

Os textos moram em `crates/tirech-omni/i18n/<idioma>/*.ftl` (Project Fluent), um arquivo por
área. No código, `t!("chave", nome = valor)`.

**Acrescentar um idioma:**

1. Copie `crates/tirech-omni/i18n/en-US/` para `i18n/<tag>/` (`es-ES`) e traduza.
2. Ponha a variante em `Idioma`, com `tk!("opcao-idioma-<tag>")` e o autônimo nos `.ftl` de todos
   os idiomas (a chave é `opcao-idioma-<tag>`).
3. Ponha a pasta na macro `recursos!`, nas listas de `fontes`, `etiqueta` e `pacote`, e o caso
   em `resolver` (o prefixo `es` vira `EsEs`).
4. Rode `cargo test -p tirech-omni --test idiomas`: os testes acusam o que falta.

## X11, virt-manager e túneis

Duas coisas que o `ssh` faz e o app põe na própria conexão: **abrir aqui as janelas de programas
gráficos do servidor** (X11) e **encaminhar portas** (túneis). Tudo vale com jump server e com o
perfil de rede da conexão.

**Na conexão** (Propriedades, categoria **Encaminhamento**, só no SSH):

- **Encaminhar X11**: Padrão, Desligado, Não confiável (`ssh -X`) ou Confiável (`ssh -Y`). O
  Padrão segue Configurações ▸ X11 e túneis (nasce Confiável).
- **Túneis**: o botão "…" abre o editor. Tipos: Local (`-L`), Remoto (`-R`, a porta 0 deixa o
  servidor escolher), Dinâmico (SOCKS5, `-D`) e Socket Unix do servidor. Cada túnel tem "Ligado"
  (sobe com a conexão) e "Permitir outras máquinas" (escuta em `0.0.0.0` em vez de `127.0.0.1`).
- **Só túneis (sem shell)**: a conexão só segura os túneis; a aba ganha o sufixo "· túneis".
- **Parar se túnel falhar**: como o `ExitOnForwardFailure`. Sem ele, uma porta ocupada vira um aviso
  e a conexão segue sem o túnel.

**Túneis ativos**: o menu da aba (botão direito na aba) tem **Túneis ativos…**: a lista com
o estado, onde escuta, as conexões abertas e os bytes de cada direção; dá para parar, ligar de
novo e adicionar túneis na hora. Se um túnel não sobe, o app avisa e aponta para essa lista.

**Programas gráficos** (menu da conexão na árvore):

- **Abrir virt-manager**: roda `virt-manager --no-fork` no servidor, por X11, sem abrir aba. A
  janela aparece aqui; o aviso do app some quando o programa acaba, e fechar o aviso fecha o programa.
  O comando se muda em Configurações ▸ X11 e túneis.
- **Abrir virt-manager local**: só aparece se o `virt-manager` está instalado nesta máquina. O app
  acha o socket do libvirt do servidor (`/var/run/libvirt/libvirt-sock` ou `virtqemud-sock`), sobe
  um túnel até ele e roda `virt-manager -c qemu+unix:///system?socket=…` aqui.
- **Executar aplicativo gráfico remoto…**: pede um comando (`gitk`, `wireshark`…); com "Mostrar a
  saída numa aba" o programa roda numa aba de terminal, para ver o texto que ele escreve. Os
  comandos recentes entram na lista do diálogo e no Início.
- Um programa gráfico usa o X11 Confiável quando a conexão está com X11 Desligado, e ignora o "Só
  túneis" da conexão (o programa precisa do shell da própria sessão).
- Os erros conhecidos vêm em português: programa não instalado no servidor, "cannot open
  display" (falta `X11Forwarding yes` ou `xauth`) e falta de permissão no libvirt.

**O que o servidor precisa** (`/etc/ssh/sshd_config`):

```
X11Forwarding yes               # e o pacote xauth instalado
AllowTcpForwarding yes          # -L, -R e -D (o padrão é yes)
AllowStreamLocalForwarding yes  # o túnel de socket Unix (o padrão é yes)
GatewayPorts clientspecified    # só para "Permitir outras máquinas" num túnel remoto (-R)
```

Se o `sshd` responde "X11 forwarding request failed" e o log dele diz `Failed to allocate
internet-domain X11 display socket`, o servidor não consegue abrir a porta do display (comum
com o IPv6 desligado): ponha `AddressFamily inet` no `sshd_config`.

**O servidor X desta máquina**:

- **Linux**: o X11 ou o XWayland da sessão (o `DISPLAY` já vem pronto). Em Configurações ▸ X11 e
  túneis dá para escolher outro display.
- **Windows**: instale o VcXsrv ou o Xming, abra com o controle de acesso desligado e use o
  display `localhost:0` (é o que o app assume sem `DISPLAY`).
- **macOS**: o XQuartz; o app acha o socket dele sozinho.

**Confiável × não confiável.** O confiável (`-Y`) dá ao programa remoto o cookie de verdade, com
acesso total ao servidor X (teclado, tela e outras janelas); é o que o virt-manager e a maioria
dos programas pedem. O não confiável (`-X`) cria um cookie limitado pela extensão SECURITY
(`xauth generate`, válido por uma hora desde o último uso) e quebra vários programas; se o
servidor X daqui não tem a extensão (o XWayland de várias distribuições), o app usa o
confiável e avisa no terminal. O servidor remoto sempre recebe um cookie falso: o de verdade
só chega ao servidor X daqui.

**virt-manager remoto ou local.** O remoto roda lá e desenha aqui pelo X11: simples, mas
lento em rede ruim. O local roda aqui e conversa com o libvirt do servidor pelo túnel: a
interface é mais ágil. Nos dois casos o usuário da conexão precisa estar no grupo `libvirt` do
servidor (`sudo usermod -aG libvirt usuario`, depois sair e entrar de novo); sem isso o aviso
é "permission denied" ou "authentication unavailable".

**Console de uma máquina virtual (SPICE e VNC).** O virt-manager local abre o console pelo
`ssh`, mas dá para abrir só o console de uma VM com um túnel e o `remote-viewer`
(pacote `virt-viewer`):

```sh
ssh servidor virsh domdisplay minha-vm        # spice://127.0.0.1:5900
# túnel Local: porta 5900 aqui → 127.0.0.1:5900 no servidor
remote-viewer spice://127.0.0.1:5900
```

**Navegador pelo servidor.** Um túnel **Dinâmico** na porta 1080 é um proxy SOCKS5 de
verdade: aponte o navegador para `127.0.0.1:1080` (SOCKS v5, com a resolução de nomes pelo
proxy) e as páginas saem pelo servidor. Só `CONNECT`, sem autenticação.

**Docker remoto.** Um túnel de **Socket Unix** de `/var/run/docker.sock` para uma porta daqui
(ou um arquivo) deixa o `docker` local falar com o do servidor, sem expor o daemon na rede:

```sh
docker -H tcp://127.0.0.1:2375 ps       # túnel: 2375 aqui → /var/run/docker.sock
```

O que ainda não faz: `-R` para socket Unix, SOCKS4 e senha no `-D`, servidor X embutido no app
e visualizador SPICE/VNC próprio.

## Portas do servidor

Numa aba SSH, **Portas do servidor** (menu da aba) abre o painel à direita com o que escuta no
servidor: o alcance (uma porta que só escuta no loopback aparece como "só no servidor"), o
processo, o usuário, o nome da porta e, nas páginas web, o título e o app conhecido (phpMyAdmin,
Adminer, pgAdmin). A lista vem do `ss -ltnpeH`; sem ele, de `/proc/net/tcp` e `/proc/net/tcp6`
lidos pelo SFTP, e do `netstat -an` onde não há `/proc`. Os contêineres Docker e Podman entram
pelo IP deles.

- **Encaminhar** abre a mesma porta em `127.0.0.1` (ou +8000 abaixo de 1024, ou a próxima
  livre); **Abrir no navegador** encaminha e abre a URL com o caminho do app; **Fixar** grava o
  túnel na conexão, e **Parar** o fecha. "Encaminhar destino…" leva a qualquer `host:porta` visto
  do servidor.
- **Nome do site**, no menu da porta, faz o túnel passar por um relé do app que troca o `Host`,
  reescreve o `Location` e os cookies e, num servidor HTTPS, faz o TLS (o certificado do servidor
  não é conferido): o navegador abre `http://{site}.localhost:{porta}` sem aviso. Os túneis
  fixados com nome de site ou HTTPS sobem sozinhos quando a sessão abre.
- Um `http://localhost:3000` impresso na aba abre pelo túnel no Ctrl+clique.
- Em Configurações › Portas: intervalo da lista, sonda web, uso do `ss`/`docker`, aviso de porta
  nova, links pelo túnel e portas de sistema.
- O assistente (MCP) tem `listar_portas` (ver), `encaminhar_porta` e `parar_porta` (mudar).

## Redes de clientes com o mesmo IP

Dois clientes com a mesma faixa (`10.0.0.0/24` nos dois) não cabem na mesma tabela de rotas.
O app resolve com um **perfil de rede** por cliente: o perfil aponta para um proxy SOCKS5 em
`127.0.0.1` (um contêiner com a VPN do cliente e o `microsocks`), e os comandos de subir e
descer. Um grupo ou uma conexão escolhe o perfil na linha **Rede**; o que está dentro do grupo
herda (`Herdar (Cliente A)`), `Direta` corta a herança. O perfil vale para SSH, SFTP, Telnet,
FTP, S3 e rclone (com a ressalva do `sftp` e do `ftp` do rclone 1.60, que ignoram o proxy); o
shell local também (veja abaixo), e a serial não usa rede. A aba ganha o sufixo ` · Cliente A`, e o comando de
descer roda quando a última aba do perfil fecha e quando o app fecha. Cada perfil tem o seu
`known_hosts`.

O app **não** sobe a VPN por conta própria nem gerencia contêineres: as receitas (WireGuard,
OpenVPN, OpenConnect, openfortivpn, `ip netns`, VM) e uma demonstração local sem VPN estão em
[`redes/LEIAME.md`](../../crates/tirech-omni/redes/LEIAME.md).

### Links pela rede do cliente

Uma conexão do tipo **Link** guarda um endereço (`https://intranet.cliente/`; sem `http://`, o
app põe) e fica na árvore com um globo, como as outras. O duplo clique (ou a busca, o Início e
os recentes) abre o link sem criar aba:

- **Sem perfil de rede** (Direta, ou nenhuma pasta com rede): no navegador padrão do sistema.
- **Com perfil de rede** (herdado da pasta, como nas outras conexões): num navegador separado
  daquele perfil, que sai pela rede do cliente. O app sobe um proxy HTTP só em `127.0.0.1`,
  um por perfil, que leva cada pedido pelo proxy do perfil ou pela VPN embutida (OpenVPN,
  WireGuard, Tailscale, Nebula, ZeroTier), ligando-a se for preciso. O navegador abre com um
  perfil próprio (`navegador/<id>/`), então os cookies e logins de um cliente não se misturam
  com os de outro nem com os do navegador de sempre, e o resto da máquina não passa pela rede
  do cliente.

O navegador vem de Configurações › Redes › **Navegador dos links**: vazio procura o Chrome, o
Chromium, o Brave, o Edge, o Vivaldi e o Firefox, nessa ordem; um nome do `PATH` ou um caminho
escolhe um. Os da família do Chromium recebem o proxy por `--proxy-server`; o Firefox (e o
LibreWolf, o Waterfox, o Floorp, o Zen) por um `user.js` no perfil, com o DNS também pelo
proxy. **Links…** no menu Ferramentas lista todos os links da árvore, com filtro, endereço e
rede, para abrir sem procurar.

Ainda não vale para um perfil só com salto SSH (sem proxy nem VPN embutida): o link avisa que o
perfil não tem proxy.

### Shell local pela rede do cliente

Um **Shell local** dentro de uma pasta com perfil de rede (ou com o perfil escolhido na linha
**Rede** da própria conexão) também sai pela rede do cliente, para os programas que sabem usar
proxy. A VPN embutida roda dentro do app, sem placa virtual, então não há rota nova no
computador: o caminho é o mesmo proxy local dos links (um por perfil, em `127.0.0.1`, que agora
fala HTTP e SOCKS5 na mesma porta), e o shell abre com estas variáveis no ambiente:

- `ALL_PROXY` e `all_proxy`: `socks5h://127.0.0.1:<porta>` (o nome também é resolvido pela rede
  do cliente);
- `HTTP_PROXY`, `HTTPS_PROXY`, `http_proxy` e `https_proxy`: `http://127.0.0.1:<porta>`;
- `NO_PROXY` e `no_proxy`: `localhost,127.0.0.1,::1`.

O `curl`, o `wget`, o `git`, o `pip` e o `npm` passam a sair pela rede do cliente; a primeira
linha do terminal diz por qual perfil e lembra que **Rede › Direta** na conexão desliga a
herança.

Para o que ignora proxy (o `xfreerdp`, um cliente de banco, o `ssh` puro), use os **Túneis** da
conexão, que no shell local aceitam dois tipos:

- **Local**: a porta daqui leva a um destino fixo, visto pela rede da conexão. Um túnel
  `13389 → 10.0.0.5:3389` deixa `xfreerdp /v:127.0.0.1:13389` abrir a área de trabalho
  do `10.0.0.5` do cliente, mesmo que outro cliente tenha o mesmo IP.
- **Dinâmico**: um proxy SOCKS5 e HTTP numa porta escolhida, para os programas em que dá para
  configurar o proxy à mão.

Os túneis sobem com a aba e descem com ela; a linha `Túnel 127.0.0.1:13389 → 10.0.0.5:3389`
no começo do terminal mostra a porta de verdade (com **Usar a próxima porta livre**, ela pode ser outra). Sem
perfil de rede, os túneis do shell local levam direto ao destino.

Um perfil só com salto SSH não vale para o shell local (o shell não passa por salto): o
terminal avisa, o shell sai direto e os túneis não sobem.

**Autocompletar (Ctrl+Espaço).** Sobre o terminal ativo abre uma lista de comandos que cabem
na aba, com a coluna **Origem** dizendo de onde veio cada um. Num shell local vêm primeiro os
comandos prontos para os túneis que a aba enxerga (os dela e os `-L`/`-D` das abas SSH abertas),
já com a porta de verdade: `xfreerdp /v:127.0.0.1:13389 /dynamic-resolution /u:` para um túnel
que leva à 3389 (o `mstsc` no Windows e o `rdp://` no macOS), `vncviewer` para a 5900, `ssh`,
`mysql`, `psql`, `redis-cli`, `mongosh`, `sqlcmd` e `curl` conforme a porta de destino, `nc`
no resto; para um túnel dinâmico, o `curl --proxy socks5h://...` e o `export ALL_PROXY`. Depois vêm os comandos já usados nesta
conexão, os da barra de envio, os passos dos **Comandos ao conectar** e, no shell local, o histórico do
bash, do zsh e do fish desta máquina. O campo de cima filtra por palavras; ↓ desce para a
lista e Enter (ou o clique duplo) **põe o comando na linha do terminal, sem executar**, para
completar o usuário ou ajustar antes. O Tab segue sendo do shell, e o atalho muda em
Configurações › Atalhos.

### Rede Nebula embutida

Se o cliente usa uma rede [Nebula](https://github.com/slackhq/nebula), não precisa de contêiner:
o perfil **Nebula embutida** entra na rede dentro do próprio app, **só para o tráfego dele**.
Não cria placa de rede virtual, não mexe nas rotas nem no DNS do computador e não pede
administrador. O protocolo vem implementado na lib (sem o programa `nebula`, sem Go e sem
chamar nada do sistema), e o `10.42.0.7` da rede Nebula só existe para as conexões que o perfil
atende; outro programa seu não enxerga essa rede.

**Como usar**

1. Em Ferramentas ▸ Redes, **Novo** e, na linha **Tipo**, escolha **Nebula embutida**.
2. **Importar config.yml…** e escolha o `config.yml` que a administração da rede entregou. Os
   arquivos que ele cita (`ca.crt`, `host.crt` e `host.key`) precisam estar ao lado dele, como
   no Nebula. O cofre precisa estar destravado.
3. Ligue o perfil a um grupo ou a uma conexão na linha **Rede**, como qualquer perfil. Use o IP
   da rede Nebula como host da conexão (`10.42.0.1`): o DNS do computador não conhece os nomes
   da Nebula.
4. Conecte. A rede **liga no primeiro uso** e **desliga quando a última aba do perfil fecha** (e
   quando o app fecha, avisando os pares). A linha **Estado** da página mostra "Desligada",
   "Conectando…", "Conectada como 10.42.0.20 (1 lighthouses, 2 túneis)", "Tentando de novo: …" ou
   "Falhou: …". **Testar** liga a rede, espera a conexão e diz o IP.

Vale para SSH, SFTP, Telnet, FTP, S3 e rclone (com a ressalva do `sftp` e do `ftp` do rclone
1.60), pelos mesmos caminhos dos outros perfis. Dois perfis Nebula de redes diferentes, mesmo
com a mesma faixa, funcionam ao mesmo tempo, cada um com o seu motor. A aba ganha o sufixo
` · Cliente N` e a dica dela diz "Rede: Cliente N (Nebula, 10.42.0.20)".

**Onde ficam os segredos.** A importação separa o config da identidade: a CA, o certificado e a
chave vão para um item **Nebula** do cofre (cifrados como os outros itens, e no painel do item
você vê o endereço, os grupos, a validade e a impressão digital, mostra a chave privada e troca o
certificado quando a administração emite outro). O que fica no perfil, em `conexoes.json`, é só
o config sem `pki.ca`, `pki.cert` e `pki.key`: nenhum PEM. Importar de novo o mesmo certificado
não duplica o item. Um item Nebula é exportado e importado com o resto do cofre (cofre `.json`
do app, Bitwarden como nota segura com campos próprios); o CSV não leva segredo e pula esse
tipo. A linha **Config** abre o `config.yml` guardado (lighthouses, `static_host_map`,
`punchy`, `tun.unsafe_routes`…); um PEM colado ali é recusado.

**Avisos e erros.** A importação recusa, dizendo o motivo: certificado vencido ou ainda não
válido, uma chave no lugar do certificado, um `config.yml` de lighthouse (`am_lighthouse:
true`), `ca`/`cert`/`key` escritos no próprio YAML em vez de arquivos, uma identidade que não
fecha com a CA. Vira aviso (e segue): `am_relay`, `firewall.inbound`, `sshd`, `stats`,
`logging` e outras chaves que só fazem sentido no serviço do Nebula, e o que a lib estranha no
config. Se a rede não conecta, o aviso diz se foi o cofre trancado, a identidade que sumiu do
cofre, um lighthouse que não responde ou uma `cipher` diferente da rede. A rede é **fail-closed**:
se a Nebula não sobe, a conexão **não** sai direto pela internet; ela falha.

**Relays (CGNAT).** Quando o computador e o servidor estão os dois atrás de CGNAT (o comum nas
operadoras brasileiras) ou de NAT simétrico, o punch não passa e a conexão vai por um **relay**
da rede: um nó com `relay.am_relay: true` (em geral o próprio lighthouse). Não há o que ligar no
app: o servidor lista os relays dele em `relay.relays` no `config.yml` dele, o lighthouse conta
isso ao app, e o app pede o relay e passa por ele, com o túnel ainda cifrado de ponta a ponta
(o relay só repassa). O `relay.use_relays: false` no config do perfil desliga isso. Se o app
também precisa ser alcançado por relays (outro nó abrindo conexão até ele), o `relay.relays` do
config do perfil vale do mesmo jeito.

**O que não faz.** Só TCP e só IPv4; não recebe conexões (o app só abre; o `-R` do SSH segue
funcionando, porque vai dentro da conexão SSH); não é lighthouse nem relay (usa os relays da
rede, não serve de relay para os outros); não assina certificados; não lê chave em token
(PKCS#11); curva P-256 ainda não.
Nada da rede Nebula é exposto a outros programas: não há porta, endereço ou credencial para
copiar.

### Rede OpenVPN embutida

Se o cliente entrega um `.ovpn`, também não precisa de contêiner nem do programa `openvpn`: o
perfil **OpenVPN embutida** entra na VPN dentro do próprio app, **só para o tráfego dele**. Não
cria placa de rede virtual (TUN), não mexe nas rotas nem no DNS do computador e não pede
administrador. O protocolo vem implementado na lib, escrito a partir da descrição pública do
protocolo (OpenVPN é marca registrada da OpenVPN Inc.; o app não é afiliado a ela), e o IP que o
servidor entrega só existe para as conexões que o perfil atende.

**Como usar**

1. Em Ferramentas ▸ Redes, **Novo** e, na linha **Tipo**, escolha **OpenVPN embutida** (ou use
   **Importar .ovpn…**, que já cria o perfil). O cofre precisa estar destravado.
2. **Importar .ovpn…** e escolha o arquivo. Se ele cita outros arquivos (`ca ca.crt`, `cert`,
   `key`, `tls-auth`…), eles precisam estar na mesma pasta; arquivos fora dela são recusados.
   Se o `.ovpn` tem `auth-user-pass`, o app pergunta o usuário e a senha (a senha é opcional:
   sem ela, o app pergunta a cada conexão).
3. Ligue o perfil a um grupo ou a uma conexão na linha **Rede**, como qualquer perfil. Use o IP
   da VPN como host da conexão: o DNS do computador não conhece os nomes de dentro dela (o DNS
   que o servidor empurra vale só para as conexões do perfil).
4. Conecte. A VPN **liga no primeiro uso** e **desliga quando a última aba do perfil fecha** (e
   quando o app fecha, avisando o servidor). A linha **Estado da VPN** mostra "Desligada",
   "Conectando a …", "Conectada como 10.8.0.6 (…, AES-256-GCM)", "Tentando de novo: …",
   "Aguardando a sua resposta: …" ou "Falhou: …". **Testar** liga a VPN, espera a conexão e diz o IP.

Vale para SSH, SFTP, Telnet, FTP, S3 e rclone, pelos mesmos caminhos dos outros perfis. Dois
perfis OpenVPN de redes diferentes, mesmo com a mesma faixa, funcionam ao mesmo tempo, cada um
com o seu motor. A aba ganha o sufixo ` · Cliente V` e a dica dela diz "Rede: Cliente V
(OpenVPN, 10.8.0.6)". Um perfil tem uma rede embutida só: Nebula **ou** OpenVPN.

**Código de verificação e desafios.** Se o servidor pede um código (OTP, `static-challenge` ou
desafio dinâmico), o app abre um diálogo com a pergunta do servidor. Responder envia o código;
fechar sem responder cancela a conexão. Na pergunta da senha, a caixa **Salvar a senha no cofre**
guarda a resposta no item da identidade.

**Onde ficam os segredos.** A importação separa o `.ovpn` da identidade: a CA, o certificado, a
chave privada e a chave TLS (`tls-auth`, `tls-crypt` ou `tls-crypt-v2`) vão para um item
**OpenVPN** do cofre (cifrados como os outros itens; o painel do item mostra o CN, o servidor, a
validade e a impressão digital, mostra a chave privada e troca o certificado quando o servidor
emite outro). O usuário e a senha do `auth-user-pass` ficam nos campos Usuário e Senha do mesmo
item. O que fica no perfil, em `conexoes.json`, é só o `.ovpn` sem esses blocos: nenhum PEM e
nenhuma senha, e o editor da linha **Perfil .ovpn** recusa um texto que traga algum. Importar de
novo o mesmo certificado não duplica o item. O item é exportado e importado com o resto do cofre
(cofre `.json` do app; Bitwarden como login com campos próprios); o CSV não leva segredo e pula
esse tipo.

**Rotas.** Por padrão vale o que o servidor empurra (`push`). Desmarque **Usar as rotas do
servidor** (é o `route-nopull`) para valer só as rotas escritas no `.ovpn`. Como só o tráfego do
app passa pela VPN, as rotas decidem o que vai pela VPN e o que sai direto.

**Avisos e erros.** A importação recusa, dizendo o motivo: certificado vencido ou ainda não
válido, uma chave que não fecha com o certificado, uma CA que não assinou o certificado, `dev
tap`, `secret` (chave estática), chave privada cifrada, PKCS#12, cifra CBC sem negociação e `.ovpn`
sem `remote`. Vira aviso (e segue): `crl-verify`, `http-proxy`/`socks-proxy`, certificado perto
de vencer, e as opções que o app leu e ignorou (scripts e plugins **nunca rodam**; a lista sai
numa linha do aviso, "Opções ignoradas (n): …"). A VPN é **fail-closed**: se não conecta (cofre
trancado, identidade que sumiu do cofre, servidor que não responde, senha recusada), a conexão
**não** sai direto pela internet; ela falha, e o aviso diz o motivo.

**O que não faz.** Só TCP e só IPv4 dentro da VPN; UDP e TCP no transporte; AES-GCM e
ChaCha20-Poly1305 (sem CBC); não recebe conexões (o `-R` do SSH segue funcionando, porque vai
dentro da conexão SSH); não lê PKCS#12, chave cifrada nem token (PKCS#11); não abre o login pelo
navegador (`OPEN_URL`: o estado mostra "Aguardando confirmação" e a página não é aberta). Nada da VPN é exposto a outros programas: a
página Redes não tem porta, endereço nem credencial para copiar, e o único socket em escuta é a
ponte interna, em `127.0.0.1`.

### Rede WireGuard embutida

Se o cliente entrega um `.conf` do WireGuard, o perfil **WireGuard** entra na rede dentro do
próprio app, **só para o tráfego dele**: sem placa de rede virtual, sem mexer nas rotas nem no
DNS do computador, sem administrador e sem o programa `wg`. Convive com qualquer outra VPN
ligada no computador. O protocolo é implementado na lib (Noise, cookie, transporte, replay e
temporizadores), sem Go e sem programa auxiliar.

**Como usar**

1. Em Ferramentas ▸ Redes, **Novo** e, na linha **Tipo**, escolha **WireGuard**. O cofre precisa
   estar destravado.
2. **Importar .conf…** e escolha o arquivo. A `PrivateKey` e as `PresharedKey` vão para um item
   **WireGuard** do cofre; o perfil guarda só o `.conf` sem elas, e o editor da linha **Perfil
   .conf** recusa um texto que traga alguma. A página mostra os endereços do nó, a chave
   pública (para o outro lado pôr no `[Peer]`), os servidores (`Endpoint`) e as rotas
   permitidas (`AllowedIPs`).
3. Ligue o perfil a um grupo ou a uma conexão na linha **Rede** e use o IP de dentro da rede
   como host. A rede **liga no primeiro uso** e **desliga quando a última aba do perfil
   fecha**. A linha **Estado do túnel** mostra "Desligada", "Conectando…", "Conectada como
   10.8.0.2 (1 de 1 pares com sessão)", "Tentando de novo: …" ou "Falhou: …", e a lista de
   pares mostra a sessão e o tráfego de cada um. **Testar** liga a rede e diz o IP.

Só o que o `AllowedIPs` cobre vai pelo túnel; um destino fora dele falha na hora, dizendo
isso (o tráfego do app não sai direto por engano). Vale para SSH, SFTP, Telnet, FTP, S3 e
rclone. Dois perfis WireGuard de redes diferentes, mesmo com a mesma faixa, funcionam ao mesmo
tempo, cada um com o seu motor. O item do cofre é exportado e importado com o resto (cofre
`.json` do app; Bitwarden como login com campos próprios, a chave privada oculta); o CSV não
leva segredo e pula esse tipo. A aba ganha o sufixo com a rede e a dica diz "Rede: Cliente W
(WireGuard, 10.8.0.2)".

**O que não faz.** Só TCP e só IPv4 dentro do túnel; a linha `DNS =` do `.conf` ainda não vale
(use o IP do destino, ou o nome que o DNS do computador já conhece); não é servidor nem recebe
conexões (o `-R` do SSH segue funcionando, porque vai dentro da conexão SSH). Se a rede não
conecta (cofre trancado, identidade que sumiu do cofre, servidor que não responde), a conexão
**não** sai direto pela internet: falha, e o aviso diz o motivo. Nada da rede é exposto a
outros programas: a página não tem porta, endereço nem credencial para copiar, e o único
socket em escuta é a ponte interna, em `127.0.0.1`.

### Rede Tailscale embutida

O perfil **Tailscale** entra numa tailnet (a do Tailscale oficial ou a de um **Headscale**)
como uma máquina do próprio app, **só para o tráfego dele**, sem placa de rede virtual, sem
administrador e **sem depender do Tailscale instalado** no computador (os dois convivem: o app
é outra máquina da tailnet, com o nome e o IP dele). O cliente é implementado na lib (controle
ts2021, mapa, DERP, disco, STUN e MagicDNS), sem Go e sem programa auxiliar. A máquina nasce
em `ShieldsUp`: para a tailnet, o app só sai, nunca recebe.

**Como usar**

1. Em Ferramentas ▸ Redes, **Novo** e, na linha **Tipo**, escolha **Tailscale**. O cofre precisa
   estar destravado (a máquina fica nele). Deixe o **Servidor** vazio para o Tailscale oficial;
   para um Headscale, escreva o endereço (`https://…`) e, se o certificado dele é de uma CA
   própria, cole-a em **CA do servidor**. **Nome da máquina** é como ela aparece na tailnet
   (vazio usa o nome do computador).
2. Entre de um destes jeitos: **Entrar** (o app abre o endereço de login no navegador, uma vez;
   **Abrir de novo** abre outra vez) ou **Usar uma chave de autenticação…** (cole a
   `tskey-auth-…` que a tailnet gerou; ela fica no item do cofre e entra sem navegador). Se o
   administrador exige aprovação, o estado mostra "Esperando a aprovação da máquina".
3. Ligue o perfil a um grupo ou a uma conexão na linha **Rede**. A rede **liga no primeiro uso**
   e **desliga quando a última aba do perfil fecha**. A linha **Estado da tailnet** mostra
   "Conectada como 100.64.0.7 (empresa.ts.net, 3 máquinas no ar)", "Precisa entrar",
   "Esperando a aprovação da máquina", "A chave da máquina venceu; entre de novo",
   "Tentando de novo: …" ou "Falhou: …". **Testar** liga a rede e diz o IP.

Dentro da tailnet, use o **nome curto** (`servidor`), o nome completo do MagicDNS ou o IP
`100.x.y.z` como host: o MagicDNS vale só para as conexões do perfil. **Máquinas da tailnet**
lista as que o controle mostra (nome, IP, sistema, no ar ou fora do ar, e se o caminho é
direto ou pelo DERP); **Nova conexão…** cria uma conexão já ligada a esta rede, e as que já
existem aparecem como "Já cadastrada". A lista é só sugestão: ela não impede digitar outro
nome. **Usar as sub-redes** (ligado por padrão) alcança as redes que outras máquinas anunciam;
**Máquina efêmera** faz o controle esquecer a máquina quando ela some.

**Onde ficam os segredos.** O item **Tailscale** do cofre guarda a máquina: as chaves
(máquina, nó e disco), o login, a tailnet, os endereços, a validade da chave e a chave de
autenticação, se houver. O perfil, em `conexoes.json`, guarda só o servidor, o nome, as opções
e a CA. Reabrir o app **não pede login de novo**: o estado é reaproveitado. Se o cofre está
trancado quando a máquina entra (ou a gravação falha), o app segura o estado novo na memória e
o grava assim que o cofre destrava; se o app fecha antes disso, a próxima conexão registra
uma máquina nova. **Esquecer esta máquina** limpa o estado do cofre (o próximo uso entra como
máquina nova; apague a antiga no painel da tailnet). O item é exportado e importado com o
resto do cofre (cofre `.json` do app; Bitwarden como login com campos próprios, as chaves
ocultas); o CSV não leva segredo e pula esse tipo.

**Caminho direto ou DERP.** O app descobre o endereço público por STUN e troca mensagens de
disco com a outra máquina; se o UDP passa, fala direto, e se não passa (ou deixa de passar)
cai para o DERP, o relé da tailnet, sem a pessoa fazer nada. A lista mostra o caminho ao lado
de "no ar". Se uma conexão não responde, veja: (1) o estado da tailnet; (2) se a máquina de
destino está "no ar"; (3) as ACLs da tailnet, que podem proibir a porta (o app só diz que a
máquina não respondeu); (4) "sem UDP: só pelo DERP" é normal em redes que bloqueiam UDP, e só
fica mais lento.

**O que não faz.** Não é nó de saída (exit node), não anuncia sub-redes nem serviços, não tem
Taildrop, Funnel, Serve nem Tailscale SSH como servidor, não recebe conexões (o `-R` do SSH
segue funcionando, porque vai dentro da conexão SSH), não suporta Tailnet Lock, e só leva TCP
em IPv4. O split DNS e os servidores de nomes da tailnet ainda não valem (só o MagicDNS). Se a
rede não conecta (cofre trancado, identidade que sumiu, controle que não responde, falta
entrar), a conexão **não** sai direto pela internet: falha, e o aviso diz o motivo. Nada da
rede é exposto a outros programas: a página não tem porta, endereço nem credencial para
copiar, e o único socket em escuta é a ponte interna, em `127.0.0.1`.

### Rede ZeroTier embutida

O perfil **ZeroTier** entra numa rede ZeroTier (a do **ZeroTier Central** ou a de um controlador
próprio) como um membro do próprio app, **só para o tráfego dele**: sem placa de rede virtual,
sem mexer nas rotas nem no DNS do sistema, sem administrador e **sem depender do ZeroTier
instalado** no computador (os dois convivem: o app é outro membro, com identidade, endereço e
porta UDP só dele). O protocolo é implementado na lib em Rust (identidade, cifras, raiz, NAT,
configuração da rede, regras e ARP), sem C++ e sem programa auxiliar.

**Como usar**

1. Em Ferramentas ▸ Redes, **Novo** e, na linha **Tipo**, escolha **ZeroTier**. O cofre precisa
   estar destravado (a identidade fica nele). Em **ID da rede**, escreva os 16 caracteres da
   rede (`8056c2e21c000001`).
2. Clique em **Conectar**. Na primeira vez o app cria a identidade (leva alguns segundos; a
   tela fica parada nesse tempo) e grava o item **ZeroTier** no cofre. Se você já tem uma
   identidade, **Importar identity.secret…** usa o arquivo dela no lugar de criar uma.
3. Uma rede com autorização manual deixa o estado em **Aguardando autorização**, e o aviso traz
   o **endereço do membro** (10 dígitos). **Copiar endereço** copia; quem administra a rede
   marca **Auth** nesse membro no ZeroTier Central (ou no controlador). Não precisa reiniciar:
   o app pergunta de novo sozinho e passa a **Conectada como 10.147.17.5 (nome da rede)**.
4. Ligue o perfil a um grupo ou a uma conexão na linha **Rede**. A rede **liga no primeiro uso**
   e **desliga quando a última aba do perfil fecha**; o perfil novo espera até 120 s, o tempo
   de alguém autorizar. **Testar** liga a rede, espera e diz o IP.

Dentro da rede, use o IP gerenciado (`10.147.17.x`) ou, com **Usar o DNS da rede**, os nomes do
domínio que a rede anuncia (o DNS vai por TCP, e o servidor da rede precisa aceitar TCP; os
outros nomes seguem para o DNS do computador, como sempre).

**Opções** (as do `local.conf` do ZeroTier, só para este perfil): **Permitir IPs públicos**
(`allowGlobal`), **Permitir a rota padrão** (`allowDefault`: o tráfego do app todo passa pela
rede), **Usar o DNS da rede** (`allowDNS`), **IP fixo** (`10.0.0.5/24`, para as redes que não
distribuem endereço; vazio usa o do controlador) e **Moons**, uma `id/semente` por moon,
separadas por espaço, para orbitar raízes extras. **Raiz e pares** mostra os raiz e os pares com
tráfego (endereço, "direto" ou "pelo raiz" e a latência), para diagnóstico.

**Onde ficam os segredos.** O item **ZeroTier** do cofre guarda a identidade (o
`identity.secret`) e o estado do nó (planet, moons, a última configuração assinada e os
caminhos). O perfil, em `conexoes.json`, guarda só o ID da rede, as opções e as moons. Reabrir
o app **não pede autorização de novo**: o mesmo membro volta, e o último estado dá a partida
mesmo com o controlador fora do ar. **Esquecer o estado** apaga só o estado e **mantém a
identidade** (o membro continua autorizado). O item tem **Exportar identity.secret…** (arquivo
com permissão só do dono) e é exportado e importado com o resto do cofre (cofre `.json` do app;
Bitwarden como login com campos próprios, a identidade oculta); o CSV não leva segredo e pula
esse tipo. **Uma identidade serve a uma rede por vez:** duplicar o perfil compartilha o item, e
o segundo a ligar recebe "identidade em uso"; para outra rede, crie outro item.

**Caminho direto ou pelo raiz.** O app fala com os raiz do ZeroTier (o planet público vem
embutido e se atualiza sozinho) e, quando o UDP passa, abre um caminho direto com o outro
membro; senão, a conversa continua pelo raiz, mais lenta. **Sem UDP a rede não funciona nesta
versão** (o relé por TCP ainda não existe). Se uma conexão não responde, veja: (1) **Estado da
rede**; (2) se o membro está autorizado; (3) as regras da rede, que podem proibir a porta (o app
só diz que a máquina não respondeu); (4) o firewall do computador, que pode bloquear o UDP.

**O que não faz.** Não liga a rede ao computador (nenhum outro programa enxerga os IPs dela), não
recebe conexões (o `-R` do SSH segue funcionando, porque vai dentro da conexão SSH), não é
controlador, raiz, moon, ponte nem relé, e só leva TCP em IPv4: redes só IPv6, tags,
capabilities, login por SSO e o relé por TCP ficam para depois. Se a rede não conecta (cofre
trancado, identidade que sumiu, rede que não existe, falta autorizar), a conexão **não** sai
direto pela internet: falha, e o aviso diz o motivo. Nada da rede é exposto a outros programas:
a página não tem porta nem credencial para copiar, e os únicos sockets do app são o UDP do
próprio ZeroTier e a ponte interna, em `127.0.0.1`.

## S3: de onde vêm as credenciais

O **usuário** da conexão é o Access Key ID. A chave secreta (a linha "Senha") e o token de
sessão das chaves temporárias da AWS ("Token de sessão") ficam no cofre, no item da credencial
(tipo S3, com o token num campo oculto): nunca vão em claro para o `connections.json`. Sem
credencial, a chave secreta é pedida ao abrir, no mesmo diálogo das senhas do SSH, e pode ser
salva. Com o cofre bloqueado, as duas linhas ficam desabilitadas.

Arquivo ▸ **Importar do `~/.aws`** lê o `credentials` (as chaves) e o `config` que fica ao lado
dele (a região e o `endpoint_url`) e cria uma conexão S3 por perfil num grupo "Importadas do
~/.aws"; o provedor sai do `endpoint_url` quando é de um conhecido (`:9000` vira MinIO). As
chaves e o token vão para o cofre aberto; com o cofre bloqueado as conexões são criadas sem a
chave secreta, que será pedida ao conectar, e a importação avisa. Um perfil sem chaves (um `sso`, por exemplo) fica de fora, com o aviso.

## S3: o que não faz

Criptografia com chave do cliente (SSE-C), ACLs e políticas do bucket, versionamento, ciclo de
vida, criar ou apagar buckets (use o console do provedor), a cadeia de credenciais da AWS
(`sso`, `credential_process`, perfis com `role_arn`) e o `gzip`. O link pré-assinado é só de
download (`GET`).

## rclone: instalar e configurar

O app **não traz** o rclone: ele conversa com o binário (MIT, processo separado) pela API de
controle remoto (`rclone rcd`). Instale pelo endereço <https://rclone.org/install/> (ou pelo
gerenciador de pacotes). A **versão mínima é a 1.60**. O app procura o binário no caminho de
**Opções do rclone ▸ Caminho do rclone** e, se estiver vazio, no `PATH`; sem ele, ou com uma versão antiga, a
subpágina e a página Arquivos mostram um aviso com o endereço de instalação e o resto do app
funciona normalmente. O aviso só aparece depois da primeira tentativa de abrir o rclone, nunca
na partida do app.

- **Um `rcd` por proxy.** O app sobe o `rclone rcd` em `127.0.0.1` (porta livre), com usuário e
  senha aleatórios que só o app conhece, quando alguém precisa, e o encerra ao fechar. Há um
  processo por proxy (o `rcd` é lançado com `HTTP_PROXY` e `HTTPS_PROXY`, e o `NO_PROXY` deixa o `127.0.0.1` de fora); hoje só o
  do "sem proxy" é usado, e o mapa por proxy espera o proxy por conexão.
- **Configuração.** O `rclone.conf` é o de sempre (o do `rclone config`), a menos que
  **Opções ▸ Arquivo de configuração** aponte outro. Os remotos criados aqui valem também no
  terminal.
- **Configuração cifrada.** Se o `rclone.conf` tem senha, um diálogo pede e pode guardar a
  senha no cofre, no item "Senha da configuração do rclone"; as vezes seguintes vêm de lá,
  sem diálogo. Com o cofre bloqueado ela é pedida sempre.
- **Senhas dos remotos.** As senhas e os segredos que o formulário recebe vão para o `rclone`
  (que as ofusca no `rclone.conf`, como sempre) e, com a caixa "Guardar as senhas no cofre"
  marcada, também para um item `rclone: {nome}` do cofre (pasta "Conexões", um campo oculto por
  senha). Nunca vão para o `connections.json` nem para o `rclone.json`. Ao editar, uma senha
  em branco mantém a atual.

## rclone: o que não faz

Montar remotos, sincronizar e agendar, instalar ou atualizar o rclone, e ajudar nos remotos
que embrulham outros (`crypt`, `union`, `alias`: eles abrem como qualquer remoto, mas o app
não explica as perguntas deles). O proxy por conexão e as opções do binário na página de
configurações do binário também estão na página Configurações (seção rclone) e no botão **Opções** da subpágina.
O `rcd` só escuta em `127.0.0.1`. Se o app cair sem fechar (um `kill -9`, por exemplo), o
processo `rclone rcd` dele fica órfão: encerre-o pelo gerenciador de tarefas.

## Fila de transferências

Acima da tabela da fila fica o resumo da leva inteira (as cópias enfileiradas em sequência, até
a fila esvaziar), como o diálogo de cópia do Explorador do Windows 11:

- uma barra pelos **bytes** e, na linha de cima, o percentual, "128 de 340 arquivos" e "1,4 GB de
  3,4 GB" (tamanhos em base 1024, como no resto do app);
- à direita, a **velocidade** da leva ("12,4 MB/s") e o **tempo restante**;
- na tabela, cada linha em andamento tem a coluna **Restante**, com o tempo daquele item.

**Como ler a faixa.** O tempo restante é uma faixa, não um número: "Faltam 3–5 min" quer dizer
que a cópia deve acabar nesse intervalo (de 10% a 90% de chance). Quando a vazão é estável, a
faixa é estreita ("Faltam cerca de 40 s"); quando oscila, fica larga. O cálculo vem do que a
fila mediu **daquela conexão e daquela direção** (envio ou download): um custo fixo por arquivo
mais os bytes que faltam pela vazão. Por isso muitos arquivos pequenos demoram mais do que os
bytes sugerem. A faixa se ajusta a cada medida, mas só muda o texto quando a diferença vale a
pena, para não ficar piscando.

**O que aparece em cada momento:**

- "**Contando arquivos…**" e a barra indeterminada, enquanto ainda há pastas por listar: o total
  só é conhecido depois, e o app não chuta um percentual;
- "**Calculando…**", nos primeiros segundos, até haver medida suficiente (cerca de 2 s de vazão
  e 3 arquivos, no caso de arquivos sem bytes);
- "**Pausada**" com a barra amarela; "**Esperando resposta**" quando a fila pergunta o que fazer
  com um conflito; "**Parada há N s**" quando nada anda e não é nenhuma das duas;
- "**Concluído · N arquivos**" com a barra cheia, ou "**Concluído com N falhas**" com a barra
  vermelha.

**O que não entra na conta:** o tempo pausado, as esperas por resposta e os arquivos que falharam
(uma falha não vira custo de arquivo). Depois de uma pausa, a velocidade volta ao ritmo de antes
em vez de despencar.

**O que não faz:** o app não guarda entre execuções o que a fila aprendeu (cada vez começa em
"Calculando…"), não desenha o gráfico de velocidade e não tem como esconder o resumo. Uma
cópia que termina antes de a próxima ser enfileirada abre uma leva nova, com resumo próprio.
Enquanto o servidor não informa o tamanho de um arquivo (o download para edição, por exemplo),
ele conta como arquivo, sem bytes.

## Arquivos: o que ainda não é configurável

As transferências em paralelo, a data e as permissões, o conflito, os arquivos ocultos, o keepalive e
o editor são da página Configurações (seção Arquivos). Seguem fixos: as datas do servidor, que
aparecem em UTC, e o FTP sem tipo ASCII e sem extensões.

## Testes

```sh
cargo test -p tirech-omni
TIRECH_OMNI_CONFIG="$(mktemp -d)" cargo run -p tomni-ui-cli -- lint -p tirech-omni
```

Os testes de chaves usam os pares de `tests/dados/` (um sem passphrase e um com a passphrase
`teste`) e, quando existem no sistema, o `ssh-add` e o `ssh-keygen` de verdade para provar a
compatibilidade do agente. Os snapshots das páginas Chaves SSH e Agente usam pastas em `/tmp`.

As receitas de `redes/` (contêineres e VPNs) não entram no `cargo test` nem no CI: os testes
das redes usam proxies e servidores falsos e comandos do `sh`.

O lint confere as duas telas do app, a da janela principal (`main`) e a de uma janela secundária
(`janela_terminais`), nos dois temas.

Os testes usam sessões `Loopback` no lugar das conexões (o `FakeOpener`), uma pasta temporária
por teste, um `MemFs` no lugar do servidor e do disco da página Arquivos (por isso arrastar do
painel local para o remoto aparece como "Cópia" e não "Envio") e, em `tests/real.rs`, um `sh` e
um servidor SFTP de verdade (`russh-sftp` local, só em Unix). O FTP é testado com o `MemFs` e
o `FakeOpener`; o cliente em si tem a matriz real em `tomni-ui/testes/ftp/` (Docker). O S3 usa um
servidor S3 em memória (`tomni-ui/crates/files/tests/comum/s3d.rs`, copiado em `tests/comum`, incluído pelos testes em
`tests/s3.rs`); o cliente tem ainda um teste opcional contra um MinIO de verdade em
`tomni-ui/testes/s3/` (Docker, porta 19000). O rclone usa um `rcd` falso escrito à mão
(`tomni-ui/crates/files/tests/comum/rcd_falso.rs`, copiado em `tests/comum`, incluído por `tests/rclone.rs` e `tests/visual_rclone.rs`),
que fala o protocolo do `rclone rcd` e as perguntas do `falso` e do `nuvem`; o cliente tem
testes opcionais com o rclone de verdade (`TOMNI_UI_RCLONE=rclone`, ver o `CONTRIBUTING.md` do
`tomni-ui`). O lint precisa da pasta vazia para não
ler as conexões de quem o roda.
