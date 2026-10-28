# Configurações: o catálogo da página (M35). Cada card tem a chave `config-<caminho no JSON com . → ->`,
# que é também o id do card na tela: o título, a `.descricao` e as `.palavras` (a busca).
# As seções vêm de `config-secao-*`.

## Seções

config-secao-geral = Geral
    .resumo = Tema, idioma, página inicial, sessões e recentes

config-secao-terminal = Terminal
    .resumo = Fonte, cores, cursor, copiar e colar, duplicar aba, shell local e logs

config-secao-conexoes = Conexões
    .resumo = Padrões das conexões novas, known_hosts, keepalive e reconectar

config-secao-scripts = Scripts
    .resumo = Interpretador, execução em paralelo e pasta dos scripts

config-secao-cofre = Cofre de senhas
    .resumo = Proteção, bloqueio, onde o cofre fica e gerador de senhas

config-secao-notas = Notas
    .resumo = Modo de edição, fonte e quebra de linhas

config-secao-agente = Chaves SSH e agente
    .resumo = O agente, as chaves novas e o uso pelo sistema

config-secao-arquivos = Arquivos
    .resumo = Transferências, conflitos, editor, comparar e sincronizar

config-secao-ftp = FTP
    .resumo = Segurança, modo, TLS e codificação

config-secao-s3 = S3
    .resumo = Região, estilo dos endereços, partes, classe e links

config-secao-rclone = rclone
    .resumo = O programa, o arquivo de configuração e a senha

config-secao-redes = Redes
    .resumo = Perfis de rede, espera e quando derrubar

config-secao-x11 = X11 e túneis
    .resumo = X11 padrão, display, virt-manager e túneis

config-secao-portas = Portas do servidor
    .resumo = Atualização, sonda, avisos e links pelo túnel

config-secao-janelas = Janelas e espaços
    .resumo = Pasta dos espaços, abrir ao iniciar e destacar abas

config-secao-ia = Assistentes de IA
    .resumo = Perfis, o que sai do computador e o contexto do terminal

config-secao-ia-servidor = Assistentes no servidor
    .resumo = Claude Code, Codex e OpenCode no servidor

config-secao-ia-local = Assistentes neste computador
    .resumo = Claude Code, Codex e OpenCode neste computador

config-secao-mcp = Controle por assistentes (MCP)
    .resumo = Ligar, permissões, espaço de trabalho e auditoria

config-secao-fim = Fim da página

## Cards

# Geral

config-geral-tema = Tema
    .descricao = Claro, escuro ou o do sistema.
    .palavras = aparencia cores escuro claro dark

config-geral-idioma = Idioma
    .descricao = O idioma da interface. Vale depois de reiniciar o { -app }.
    .palavras = idioma lingua language ingles english portugues traducao

config-geral-pagina_inicial = Página ao iniciar
    .descricao = Automática: os Terminais se havia abas abertas na última vez; senão, o Início.
    .palavras = abrir começo inicio

config-geral-restaurar_sessoes = Reabrir as abas e janelas ao iniciar
    .descricao = Desligado, o app começa vazio (o que estava aberto continua gravado).
    .palavras = sessoes restaurar abas

config-geral-confirmar_fechar = Perguntar antes de fechar com sessões abertas
    .descricao = Desligado, fechar a janela fecha direto.
    .palavras = sair confirmar fechar

config-geral-ao_fechar = Ao fechar a janela
    .descricao = Perguntar, deixar o app na bandeja (o agente continua) ou fechar. Precisa de bandeja no sistema.
    .palavras = sair bandeja esconder fechar minimizar

config-geral-iniciar_com_sistema = Iniciar com o sistema
    .descricao = Abre o app ao entrar no sistema, para o agente de chaves já estar de pé.
    .palavras = autostart inicializacao login bandeja boot

config-geral-iniciar_na_bandeja = Ao iniciar com o sistema, ficar só na bandeja
    .descricao = Desligado, a janela abre. Ligado, só o ícone da bandeja aparece. Precisa de bandeja no sistema.
    .palavras = autostart inicializacao escondido minimizado bandeja janela

config-geral-atalho_no_menu = Atalho no menu de aplicativos
    .descricao = Mostra o { -app } no menu e no dock do sistema, com o ícone próprio.
    .palavras = desktop dock lanchador icone launcher menu aplicativos

config-geral-medidor_cpu = Medidor de CPU na barra de título
    .descricao = Mostra quanto do processador o { -app } usa. Clicar nele abre a página Desempenho, com o peso de cada terminal.
    .palavras = cpu processador desempenho uso consumo medidor grafico gerenciador de tarefas

config-geral-recentes_max = Recentes guardados
    .descricao = Quantas conexões a lista de recentes guarda, sem contar as fixadas.
    .palavras = historico ultimas

config-acao-limpar_recentes = Limpar recentes
    .descricao = Apaga a lista de recentes e mantém as fixadas.
    .palavras = apagar historico

config-geral-assistente_ia = Assistentes de IA
    .descricao = O chat com os perfis de IA e o Claude Code, o Codex e o OpenCode nos servidores. Desligado, os menus, as linhas, as seções e os atalhos deles somem; o que está gravado fica.
    .palavras = ia ai inteligencia artificial chat claude codex opencode desligar esconder

config-geral-mcp = Controle por assistentes (MCP)
    .descricao = Os assistentes controlando o app, o "Controlar com" e o cluster com IA. Desligado, somem como os da IA e o servidor MCP não sobe.
    .palavras = mcp ia ai controlar assistentes cluster desligar esconder

config-acao-instalacao = Instalar no computador
    .descricao = Copia o app para uma pasta fixa e cria os atalhos, o início com o sistema e a entrada em Aplicativos instalados.
    .palavras = instalar instalacao desinstalar copiar atalho menu iniciar area de trabalho barra de tarefas dock aplicativos

config-acao-pasta_dados = Pasta de dados
    .descricao = Onde ficam as conexões, o cofre e os espaços.
    .palavras = armazenamento pendrive sincronizar mudar dropbox

config-acao-sobre = Sobre o { -app }
    .descricao = Versão, licença e o que é o app.
    .palavras = versao licenca

# Terminal

config-terminal-tamanho_fonte = Tamanho da fonte
    .descricao = O tamanho da letra em todos os terminais abertos.
    .palavras = fonte letra zoom texto

config-terminal-esquema = Esquema de cores
    .descricao = As cores do terminal: o Campbell e outros esquemas.
    .palavras = cores paleta campbell tango vintage one half tema

config-terminal-cursor = Forma do cursor
    .descricao = Barra, bloco ou sublinhado.
    .palavras = cursor bloco barra sublinhado

config-terminal-cursor_piscando = Cursor piscando
    .descricao = Desligado, o cursor fica fixo mesmo que o programa peça para piscar.
    .palavras = cursor piscar pisca blink

config-terminal-historico = Linhas de histórico
    .descricao = Quantas linhas o terminal guarda para rolar. Vale para as sessões novas.
    .palavras = scrollback rolagem buffer

config-terminal-copiar_ao_selecionar = Copiar ao selecionar
    .descricao = Ao soltar o mouse, o texto selecionado vai para a área de transferência.
    .palavras = copiar selecao clipboard

config-terminal-avisar_colar = Avisar ao colar várias linhas
    .descricao = Pergunta antes de colar um texto com mais de uma linha.
    .palavras = colar paste confirmar

config-terminal-colar_arquivos = Ao colar imagem ou arquivo numa sessão SSH
    .descricao = O que fazer quando a área de transferência tem uma imagem ou arquivos, ou quando você arrasta arquivos para o terminal.
    .palavras = colar paste imagem arquivo captura print claude codex opencode sftp tmp

config-terminal-pasta_colar = Pasta no servidor para o que é colado
    .descricao = Onde os arquivos colados ficam no servidor. ~ é a pasta pessoal. Se não der para usar, o app usa ~/.cache/tirech-colar.
    .palavras = colar pasta tmp temporário

config-terminal-ctrl_v_arquivos = Ctrl+V cola imagem e arquivos
    .descricao = Sem texto na área de transferência, o Ctrl+V envia a imagem ou os arquivos e cola o caminho. Com texto, ele continua indo ao programa.
    .palavras = ctrl+v colar imagem

config-terminal-limpar_colados_dias = Apagar do servidor o que foi colado há mais de
    .descricao = Em dias. 0 nunca apaga. A limpeza acontece no primeiro envio de cada sessão.
    .palavras = limpar apagar temporário dias

config-terminal-colar_tar_min_arquivos = Enviar a pasta num pacote só a partir de
    .descricao = Em arquivos. Uma pasta com pelo menos esse número de arquivos (ou com 10 ou mais, todos pequenos) vai num pacote tar só, bem mais rápido. O pacote é feito pelo próprio app; o servidor só precisa ter tar para abrir, e sem ele a pasta vai pelo SFTP. 0 envia sempre pelo SFTP.
    .palavras = pasta tar pacote muitos arquivos rápido

config-terminal-fechar_painel_vazio = Fechar o painel quando a última aba sair
    .descricao = O painel que fica sem abas some sozinho, se houver outro painel.
    .palavras = painel quadrante vazio fechar

config-terminal-duplicar_na_pasta = Duplicar na pasta atual
    .descricao = A aba duplicada abre na pasta em que a original está. No SSH, o app digita um cd quando o prompt aparece.
    .palavras = duplicar aba pasta diretório cd clonar copiar

config-terminal-links_com_ctrl = Abrir links com Ctrl+clique
    .descricao = Ctrl+clique abre o endereço neste computador; com o Ctrl apertado, os links aparecem sublinhados. Desligado, o menu do clique direito continua com Copiar link.
    .palavras = link url endereço hiperlink navegador ctrl clique

config-terminal-botao_direito = Botão direito
    .descricao = Copia (com seleção) e cola (sem), e o Ctrl+clique direito abre o menu; ou sempre abre o menu.
    .palavras = mouse menu colar

config-terminal-campainha = Campainha
    .descricao = O que acontece quando o programa toca a campainha.
    .palavras = bell som piscar aviso

config-terminal-shell = Shell local
    .descricao = O programa que o "Abrir shell local" executa.
    .palavras = bash zsh powershell cmd

config-terminal-shell-programa = Programa
    .descricao = Vazio usa o $SHELL (ou o cmd.exe).
    .palavras = shell bash zsh executavel

config-terminal-shell-argumentos = Argumentos
    .descricao = Separados por espaço.
    .palavras = shell parametros opcoes

config-terminal-shell-pasta = Pasta inicial
    .descricao = Onde o shell local começa; aceita ~. Uma conexão local pode ter a sua.
    .palavras = shell diretorio cwd cd inicio

config-terminal-logs = Logs das sessões
    .descricao = Grava o que aparece no terminal num arquivo.
    .palavras = gravar registro log

config-terminal-logs-pasta = Pasta dos logs
    .descricao = Vazia usa a pasta logs dentro da pasta de dados.
    .palavras = log gravar diretorio

config-terminal-logs-todas_as_sessoes = Gravar todas as sessões
    .descricao = Cada sessão nova grava um arquivo com o nome da conexão e a hora.
    .palavras = log registro automatico

config-terminal-logs-cifrar = Cifrar os logs
    .descricao = Os logs novos saem cifrados (.log.cifrado) e só abrem com o cofre destravado. A chave dos logs fica no cofre.
    .palavras = log cifrar criptografar proteger segredo

config-terminal-logs-selar_secs = Selar o log cifrado a cada (segundos)
    .descricao = De quanto em quanto tempo o texto recente vai para o disco. Se o app cair, perde-se no máximo isso.
    .palavras = log cifrado disco gravar intervalo

config-terminal-logs-visualizar_mib = Tamanho do log na página Logs (MiB)
    .descricao = Quanto do fim do log a página mostra de uma vez. 0 mostra tudo, e um log grande demora a abrir.
    .palavras = log visualizar limite memoria ler

# Conexões

config-conexoes-usuario_padrao = Usuário padrão
    .descricao = Preenche as conexões novas. Vazio usa o usuário do sistema.
    .palavras = login user nome

config-conexoes-porta_padrao = Porta SSH padrão
    .descricao = A porta das conexões novas.
    .palavras = 22 ssh

config-conexoes-term_padrao = TERM padrão
    .descricao = O tipo de terminal que as conexões novas anunciam: xterm-256color, xterm, vt100, linux.
    .palavras = xterm vt100 terminfo

config-conexoes-keepalive_padrao_secs = Keepalive padrão
    .descricao = Segundos entre os sinais que mantêm a conexão viva (0 desliga). Conexões novas.
    .palavras = manter viva timeout cair

config-conexoes-known_hosts = Arquivo known_hosts
    .descricao = Onde ficam as chaves dos servidores conhecidos. Vazio usa ~/.ssh/known_hosts.
    .palavras = hosts servidores fingerprint impressao

config-conexoes-chave_nova = Servidor desconhecido
    .descricao = O que fazer com a chave de um servidor que o known_hosts ainda não tem. A chave mudada continua recusada.
    .palavras = host key confiar aceitar perguntar

config-conexoes-reconectar = Ao cair a conexão
    .descricao = O que o app faz quando a conexão cai (não quando o shell termina com exit).
    .palavras = reconectar queda cair religar

config-conexoes-reconectar-modo = O que fazer
    .descricao = Não fazer nada, oferecer reconectar ou reconectar sozinho.
    .palavras = reconectar automatico

config-conexoes-reconectar-tentativas = Tentativas
    .descricao = Quantas vezes reconectar sozinho (de 1 a 20).
    .palavras = reconectar vezes

config-conexoes-reconectar-espera_secs = Espera entre as tentativas
    .descricao = Segundos antes de cada tentativa (de 1 a 300).
    .palavras = reconectar intervalo tempo

config-conexoes-reconectar-retentar_mins = Tentar de novo após (minutos)
    .descricao = Quando as tentativas acabam, espera estes minutos e começa outra rodada, até a conexão voltar; 0 desliga (de 0 a 1440). Cada conexão ou pasta pode escolher outro tempo nas propriedades.
    .palavras = reconectar de novo rodada pausa minutos insistir

config-conexoes-abrir_em = Abrir conexão em
    .descricao = Onde abre o clique duplo na árvore, no Início e na busca.
    .palavras = janela painel aba

config-acao-importar_ssh_config = Importar do ~/.ssh/config
    .descricao = Traz os hosts do arquivo de configuração do OpenSSH.
    .palavras = openssh hosts importar

config-acao-importar_aws = Importar do ~/.aws
    .descricao = Traz os perfis da AWS para conexões S3.
    .palavras = amazon credenciais importar s3

config-conexoes-confirmar_fechar_todas = Perguntar antes de fechar todas as abas
    .descricao = Ao usar Ferramentas > Fechar todas as abas com sessões abertas, pede confirmação.
    .palavras = fechar abas todas confirmar perguntar

config-conexoes-tmux_ao_fechar = Ao fechar uma aba com tmux ou screen
    .descricao = O que fazer com a sessão do tmux ou do screen no servidor quando você fecha uma aba que roda nela. Fechar o app nunca encerra: as abas voltam às mesmas sessões.
    .palavras = tmux screen sessão persistente fechar aba encerrar manter

config-conexoes-comandos_quieto_ms = Tela parada depois de (ms)
    .descricao = Quanto tempo sem saída conta como tela parada, nos comandos ao conectar (de 50 a 60000).
    .palavras = comandos conectar sudo passos quieto esperar

config-conexoes-comandos_max_secs = Tempo máximo de cada passo (s)
    .descricao = O tempo máximo que os passos novos dos comandos ao conectar esperam (de 1 a 86400).
    .palavras = comandos conectar sudo passos tempo limite

config-conexoes-importar_senhas = Importar as senhas por padrão
    .descricao = Ao importar de outro programa, deixa marcada a caixa que guarda as senhas no cofre.
    .palavras = importar senha cofre winsshterm filezilla winscp

# Scripts

config-scripts-interpretador = Interpretador
    .descricao = O programa que lê o arquivo do script, na entrada (sh -s, bash -s). Uma linha digitada roda como está.
    .palavras = script shell bash sh comando

config-scripts-paralelo = Servidores ao mesmo tempo
    .descricao = Quantos servidores rodam o script juntos (de 1 a 64); os outros esperam a vez.
    .palavras = script paralelo simultaneo threads vezes

config-scripts-pasta = Pasta dos scripts
    .descricao = Vazia usa a pasta scripts dentro da pasta de dados.
    .palavras = script diretorio arquivos sh

# Cofre de senhas

config-acao-protecao_cofre = Proteção do cofre
    .descricao = Como o cofre é cifrado: chave guardada no arquivo ou senha mestra.
    .palavras = senha mestra cifrar criptografia seguranca

config-cofre-limpar_area_transferencia_secs = Limpar a área de transferência
    .descricao = Depois de copiar uma senha, apaga-a da área de transferência se ela ainda estiver lá.
    .palavras = clipboard copiar senha limpar

config-cofre-bloquear_apos_min = Bloquear sem uso
    .descricao = Bloqueia o cofre depois de um tempo sem usar uma senha. Precisa de senha mestra.
    .palavras = travar inatividade timeout senha mestra

config-acao-cofre_local = Local do cofre
    .descricao = A pasta do vault.json e do vault.key. Vazia usa a pasta de dados; numa pasta sincronizada (Syncthing, Dropbox, OneDrive), o cofre vai junto para as outras máquinas.
    .palavras = pasta sincronizar syncthing dropbox onedrive vault diretorio

config-acao-cofre_levar = Levar o cofre para outra máquina…
    .descricao = Salva um arquivo .tirech-vault com o cofre, para abrir em outro computador.
    .palavras = exportar copiar tirech-vault backup mover

config-acao-cofre_abrir_outro = Abrir cofre de outra máquina…
    .descricao = Junta um .tirech-vault (ou um vault.json) ao cofre daqui, ou usa ele no lugar.
    .palavras = importar juntar tirech-vault sincronizar mesclar

config-cofre-gerador = Gerador de senhas
    .descricao = O que o botão Gerar do cofre usa.
    .palavras = gerar aleatoria senha

config-cofre-gerador-tamanho = Tamanho
    .descricao = De 8 a 128 caracteres.
    .palavras = gerador comprimento

config-cofre-gerador-maiusculas = Letras maiúsculas
    .descricao = Entram A a Z.
    .palavras = gerador

config-cofre-gerador-minusculas = Letras minúsculas
    .descricao = Entram a a z.
    .palavras = gerador

config-cofre-gerador-digitos = Dígitos
    .descricao = Entram 0 a 9.
    .palavras = gerador numeros

config-cofre-gerador-simbolos = Símbolos
    .descricao = Entram !, #, $ e parecidos.
    .palavras = gerador especiais

config-cofre-gerador-sem_ambiguos = Sem caracteres ambíguos
    .descricao = Tira 0, O, 1, l e I, que se confundem.
    .palavras = gerador parecidos

config-acao-csv_cofre = Importar e exportar CSV
    .descricao = Abre a página do cofre, onde ficam o importar e o exportar.
    .palavras = planilha importar exportar backup

# Notas

config-notas-modo = Modo ao abrir
    .descricao = Como a página Notas abre uma nota: só o texto, o texto com a prévia ao lado ou só a prévia.
    .palavras = markdown editar dividir ler previa visualizar

config-notas-fonte_mono = Editor em fonte monoespaçada
    .descricao = Escreve o Markdown numa fonte em que todas as letras têm a mesma largura.
    .palavras = markdown fonte monoespacada codigo

config-notas-quebrar_linhas = Quebrar as linhas longas
    .descricao = Continua a linha na de baixo em vez de criar uma barra de rolagem na horizontal.
    .palavras = markdown quebra rolagem largura

# Chaves SSH e agente

config-agente-ligado = Agente embutido
    .descricao = O agente do app guarda as chaves abertas para as conexões e para outros programas.
    .palavras = ssh-agent chaves socket

config-agente-socket = Endereço do socket
    .descricao = Vazio usa o endereço padrão do sistema (o pipe openssh-ssh-agent no Windows). Vale ao religar o agente.
    .palavras = ssh_auth_sock caminho

config-agente-exportar_no_shell = Usar no shell local
    .descricao = Põe o SSH_AUTH_SOCK nos shells locais abertos depois.
    .palavras = ssh_auth_sock ambiente

config-agente-carregar_ao_iniciar = Carregar as chaves ao iniciar
    .descricao = Abre as chaves da lista do agente ao abrir o app.
    .palavras = chaves automatico

config-agente-vida_padrao_secs = Tempo de vida padrão
    .descricao = Quanto tempo uma chave nova fica no agente. Vale para as acrescentadas depois.
    .palavras = validade expirar lifetime

config-agente-confirmar_padrao = Confirmar cada uso
    .descricao = Pede confirmação a cada uso das chaves acrescentadas depois.
    .palavras = confirm permissao

config-agente-encaminhar_padrao = Encaminhar o agente por padrão
    .descricao = As conexões SSH usam o agente do app no servidor (agent forwarding).
    .palavras = forward encaminhamento -A

config-agente-pasta_chaves = Pasta das chaves
    .descricao = Onde o Gerar chave grava e o que a página Chaves lista. Vazia usa ~/.ssh.
    .palavras = ssh diretorio id_ed25519

config-agente-chaves_novas = Tipo e proteção das chaves novas
    .descricao = O que o diálogo Gerar chave já vem escolhido.
    .palavras = gerar ed25519 rsa ecdsa

config-agente-tipo_padrao = Tipo
    .descricao = Ed25519, ECDSA ou RSA.
    .palavras = ed25519 rsa ecdsa algoritmo

config-agente-rodadas_padrao = Proteção da passphrase
    .descricao = Quantas rodadas do bcrypt-pbkdf: mais é mais lento de quebrar e de abrir.
    .palavras = bcrypt kdf passphrase

config-agente-sistema_ssh_config = Usar no ssh do sistema (~/.ssh/config)
    .descricao = Escreve um bloco no topo do ~/.ssh/config: o terminal, o git, o codex e o VS Code passam a usar as chaves do app enquanto ele estiver aberto.
    .palavras = ssh identityagent git codex claude sistema

config-agente-sistema_shells = Também nos shells (bash, zsh, fish)
    .descricao = Põe o SSH_AUTH_SOCK no .bashrc, no .zshrc e no fish, para os programas que não leem o ~/.ssh/config. Vale nos shells abertos depois.
    .palavras = ssh_auth_sock bashrc zshrc fish ambiente

config-agente-sistema_sessao = Também na sessão gráfica
    .descricao = Avisa a sessão do usuário (systemctl --user ou launchctl) do SSH_AUTH_SOCK. Vale para os programas abertos depois.
    .palavras = ssh_auth_sock systemd launchctl ambiente

config-agente-sistema_git = Fazer o Git usar o OpenSSH do Windows
    .descricao = O ssh que vem com o Git não fala com o agente do Windows. Isto aponta o core.sshCommand do Git para o ssh.exe do Windows.
    .palavras = git sshcommand openssh windows

config-agente-tirar_ao_bloquear = Ao bloquear o cofre, tirar as chaves dele do agente
    .descricao = As chaves de arquivo e as que outro programa pôs no agente ficam.
    .palavras = bloquear cofre chaves

config-agente-pageant = Atender como o Pageant (PuTTY, WinSCP, FileZilla)
    .descricao = Esses programas procuram o Pageant e passam a usar as chaves do app. Se o Pageant do PuTTY já está aberto, ele continua atendendo.
    .palavras = putty winscp filezilla kitty windows

config-acao-gerenciar_chaves = Gerenciar chaves e agente
    .descricao = Abre a página do agente.
    .palavras = chaves agente

# Arquivos

config-arquivos-paralelas = Transferências ao mesmo tempo
    .descricao = De 1 a 8. As que estão em curso terminam.
    .palavras = fila simultaneas paralelo

config-arquivos-conflito = Quando o arquivo já existe
    .descricao = O que fazer nas próximas transferências.
    .palavras = sobrescrever retomar pular renomear conflito

config-arquivos-mostrar_ocultos = Mostrar arquivos ocultos
    .descricao = O estado inicial do botão nos painéis abertos depois.
    .palavras = dotfiles ocultos

config-arquivos-editor = Editor externo
    .descricao = O programa do Editar. Vazio usa o do sistema.
    .palavras = editar vim code notepad

config-arquivos-enviar_editado = Enviar sozinho o arquivo editado
    .descricao = Perguntar ou enviar de volta assim que o arquivo mudar.
    .palavras = editar salvar upload

config-arquivos-preservar_data = Manter a data de modificação
    .descricao = Nas próximas transferências.
    .palavras = mtime timestamp

config-arquivos-preservar_permissoes = Manter as permissões
    .descricao = Nas próximas transferências.
    .palavras = chmod modo

config-arquivos-pasta_local = Pasta local inicial
    .descricao = O painel local das abas novas. Vazia usa a pasta pessoal.
    .palavras = home diretorio

config-arquivos-confirmar_apagar = Perguntar antes de apagar
    .descricao = Desligado, Delete apaga sem o diálogo.
    .palavras = excluir remover confirmar

config-arquivos-keepalive_secs = Keepalive do navegador
    .descricao = Segundos entre os sinais da conexão do navegador de arquivos (0 desliga). Conexões abertas depois.
    .palavras = sftp manter viva

config-arquivos-comparar = Comparar pastas ao abrir
    .descricao = As abas novas já abrem comparando o painel local com o remoto.
    .palavras = diferenca diff sincronizar novo amarelo

config-arquivos-comparar_modo = Critério da comparação
    .descricao = Pelo tamanho ou pela data de modificação. Vale para as abas novas.
    .palavras = diferenca diff tamanho data mais novo

config-arquivos-comparar_tolerancia_min = Tolerância de data (minutos)
    .descricao = Uma diferença de até tantos minutos conta como a mesma data. O FTP lista a data só até o minuto.
    .palavras = diferenca diff ftp mtime relogio

config-arquivos-ocultar_identicos = Ocultar arquivos idênticos ao comparar
    .descricao = Só aparece o que difere. Vale para as abas novas.
    .palavras = diferenca diff esconder iguais

config-arquivos-sincronizar = Navegação sincronizada ao abrir
    .descricao = As abas novas já abrem com a navegação sincronizada: entrar ou subir num painel leva o outro à mesma pasta. A base é o par das pastas em que a aba abre.
    .palavras = sincronizar navegacao espelhar espelho link

# FTP

config-ftp-seguranca = Segurança
    .descricao = Como as conexões FTP novas protegem o tráfego.
    .palavras = ftps tls ssl criptografia

config-ftp-modo = Modo da conexão de dados
    .descricao = Passivo ou ativo. Com um perfil de rede com proxy, o app força o passivo.
    .palavras = pasv port passivo ativo

config-ftp-tls = Versão do TLS
    .descricao = A versão do TLS das conexões FTPS novas.
    .palavras = ftps tls 1.2 tls 1.3

config-ftp-codificacao = Codificação dos nomes
    .descricao = Como ler os nomes dos arquivos do servidor.
    .palavras = utf-8 latin-1 acentos charset

# S3

config-s3-regiao = Região padrão
    .descricao = Das conexões S3 novas com o provedor "outro".
    .palavras = region us-east-1 aws

config-s3-estilo = Estilo de endereço padrão
    .descricao = Caminho ou virtual host, nas conexões S3 novas com o provedor "outro".
    .palavras = path style virtual host endpoint

config-s3-parte_mib = Tamanho da parte
    .descricao = O tamanho de cada parte do envio em partes (multipart), de 5 a 512 MiB.
    .palavras = multipart upload mib

config-s3-classe = Classe de armazenamento padrão
    .descricao = STANDARD, STANDARD_IA, INTELLIGENT_TIERING, GLACIER_IR. Vazia usa a do bucket.
    .palavras = storage class glacier standard

config-s3-validade_link = Validade padrão do link
    .descricao = O primeiro item do menu Copiar link pré-assinado.
    .palavras = presigned compartilhar url expira

# rclone

config-rclone-binario = Programa rclone
    .descricao = Vazio procura no PATH. Vale para as conexões abertas depois.
    .palavras = binario executavel caminho

config-rclone-arquivo_config = Arquivo de configuração
    .descricao = Vazio usa o padrão do rclone.
    .palavras = rclone.conf remotos

config-rclone-credencial = Senha da configuração cifrada
    .descricao = O item do cofre com a senha do arquivo de configuração. A senha não sai do cofre.
    .palavras = config password cofre cifrada

config-acao-testar_rclone = Versão
    .descricao = Procura o rclone e mostra a versão.
    .palavras = testar versao instalado

# Redes

config-acao-perfis_de_rede = Perfis de rede
    .descricao = Uma rede por cliente: proxy, salto e como subir a VPN.
    .palavras = proxy vpn salto socks

config-redes-espera_padrao_secs = Espera padrão do proxy
    .descricao = Segundos que um perfil novo espera a rede subir (de 1 a 600).
    .palavras = timeout perfil vpn

config-redes-descer_ao_fechar = Descer as redes ao fechar
    .descricao = Desligado, fechar o app não roda o comando de descer dos perfis em uso.
    .palavras = vpn desligar fechar

config-redes-navegador = Navegador dos links
    .descricao = Os links com perfil de rede abrem nele, numa janela separada por perfil, que sai pela rede do cliente. Os links sem perfil abrem no navegador padrão.
    .palavras = links chrome chromium edge brave firefox browser web url

# X11 e túneis

config-x11-padrao = X11 padrão
    .descricao = O modo das conexões SSH que não escolhem o X11. Confiável é o ssh -Y; não confiável, o ssh -X.
    .palavras = x11 display janelas grafico xauth virt-manager

config-x11-display = Servidor X daqui (DISPLAY)
    .descricao = Vazio usa a variável DISPLAY. No Windows, o VcXsrv ou o Xming costumam ser localhost:0; no macOS, o XQuartz.
    .palavras = x11 display xming vcxsrv xquartz xwayland

config-x11-virt_manager = Comando do virt-manager remoto
    .descricao = O que o menu Abrir virt-manager roda no servidor.
    .palavras = virt-manager libvirt kvm maquinas virtuais

config-tuneis-outras_maquinas = Permitir outras máquinas nos túneis novos
    .descricao = Os túneis -L e -D novos escutam em todas as interfaces, não só em 127.0.0.1. Cada túnel pode mudar.
    .palavras = tunel port forwarding socks gatewayports rede escutar

# Portas do servidor (M58)

config-portas-intervalo_secs = Atualizar as portas a cada (segundos)
    .descricao = Com o painel Portas aberto, a lista se atualiza nesse intervalo. 0 atualiza só no botão.
    .palavras = portas servidor atualizar intervalo listen

config-portas-sondar = Olhar o que responde nas portas novas
    .descricao = Abre uma conexão curta pelo SSH em cada porta nova para achar o título da página e apps como o phpMyAdmin.
    .palavras = portas sonda titulo phpmyadmin http

config-portas-sonda_ms = Tempo da sonda (milissegundos)
    .descricao = O tempo total que a sonda espera por uma porta. Mínimo 100.
    .palavras = portas sonda espera timeout

config-portas-usar_ferramentas = Usar ss, netstat, docker e podman do servidor
    .descricao = Quando existem, trazem o programa de cada porta e os contêineres. Desligado, só o SFTP lê as portas.
    .palavras = portas ss netstat docker podman conteineres

config-portas-avisar_novas = Avisar quando uma porta nova aparece
    .descricao = Um aviso nas sessões SSH com o botão para abrir o painel. Funciona com o painel fechado.
    .palavras = portas aviso nova notificar

config-portas-links_pelo_tunel = Abrir links locais pelo túnel
    .descricao = Um localhost:3000 no terminal de uma aba SSH abre o servidor da conexão, não este computador.
    .palavras = portas links terminal localhost tunel navegador

config-portas-mostrar_sistema = Mostrar as portas do sistema
    .descricao = Inclui o próprio SSH, o DNS local e outras portas do sistema na lista.
    .palavras = portas sistema ssh dns

# Janelas e espaços

config-janelas-pasta_espacos = Pasta dos espaços de trabalho
    .descricao = Vazia usa a pasta espacos dentro da pasta de dados.
    .palavras = workspace diretorio layouts

config-janelas-abrir_espaco = Abrir um espaço salvo de uma janela em
    .descricao = Onde abre o clique duplo num espaço de origem Janela ou Todas.
    .palavras = workspace janela

config-janelas-tear_off = Soltar uma aba fora da janela abre outra
    .descricao = Desligado, soltar fora não faz nada.
    .palavras = destacar arrastar aba janela

config-acao-perfis_de_ia = Provedores de IA
    .descricao = Onde estão os modelos: no seu computador, na rede local ou na internet.
    .palavras = ia modelo ollama openai anthropic openrouter groq chave api lm studio

config-ia-redigir = Tirar senhas e chaves do que vai para a IA
    .descricao = Troca por [segredo removido] o que o app reconhece como senha ou chave antes de enviar.
    .palavras = ia redacao segredo privacidade mascarar

config-ia-redigir_extras = Outros textos a tirar
    .descricao = Um por linha. Comece a linha com re: para usar uma expressão regular.
    .palavras = ia redacao regex expressao regular segredo privacidade

config-ia-mostrar_no_menu = Mostrar os itens da IA no menu do terminal
    .descricao = Explicar a seleção, explicar o último erro e gerar um comando.
    .palavras = ia menu contexto botao direito

config-ia-mostrar_o_que_sai = Mostrar o que vai sair antes do primeiro envio de cada aba
    .descricao = Abre o texto exato, já sem os segredos, para você conferir antes de enviar.
    .palavras = ia privacidade confirmar enviar

config-ia-linhas_contexto = Linhas do terminal no contexto
    .descricao = Quantas das últimas linhas visíveis do terminal vão junto com o pedido (de 1 a 100 000).
    .palavras = ia contexto historico

config-ia-max_caracteres = Tamanho máximo do contexto
    .descricao = Em caracteres. Passou disso, o app corta as linhas mais antigas. 0 não limita.
    .palavras = ia contexto limite modelo local

config-ia-comando_shell = Shell a supor
    .descricao = Usado quando o app não consegue saber qual é o shell do servidor.
    .palavras = ia bash zsh fish powershell

config-ia-espera_resposta = Esperar a resposta por
    .descricao = Em segundos. 0 espera sem limite, o que serve a modelos locais lentos.
    .palavras = ia timeout tempo limite

config-ia-mcp_ligado = Deixar assistentes controlarem o app
    .descricao = O Claude Code e o Codex passam a ver e usar as conexões que você liberar, pelo MCP. O Tirech Omni precisa estar aberto.
    .palavras = mcp assistente ia ai claude codex controlar ligar

config-ia-bloqueio_extras = Comandos que sempre pedem aprovação
    .descricao = Um por linha: o nome de um programa ou, com re: no começo, uma expressão regular. Valem além da lista de bloqueio do app, mesmo com a conexão em Livre.
    .palavras = mcp bloqueio permissao aprovar comando perigoso regex

config-ia-auditoria = Gravar o registro do que os assistentes fazem
    .descricao = Um arquivo por dia na pasta de dados, sem senhas. Os comandos e textos passam pela mesma limpeza de segredos que vai para a IA.
    .palavras = mcp auditoria registro log historico segredo

config-ia-marca_secs = Marcar a aba depois da última ação por
    .descricao = Em segundos. Quanto tempo a aba mostra que um assistente mexeu nela.
    .palavras = mcp marca aba tempo ia

config-ia-linhas_por_leitura = Linhas por leitura de arquivo
    .descricao = Quantas linhas o assistente recebe quando lê um arquivo do espaço de trabalho sem pedir um trecho (de 1 a 100 000).
    .palavras = mcp ler arquivo linhas espaco

config-ia-caracteres_por_resposta = Tamanho máximo de uma resposta
    .descricao = Em caracteres. Passou disso, o app corta e diz como continuar. 0 não limita.
    .palavras = mcp resposta limite tamanho caracteres

config-ia-comandos_na_aba = Rodar os comandos dos assistentes na aba
    .descricao = Mesmo quando o assistente pede para rodar à parte, o comando aparece na aba, como se você o digitasse. Se a aba estiver ocupada (um programa aberto ou outro comando rodando), ele roda à parte, sem aparecer.
    .palavras = mcp executar comando aba visivel separado terminal

config-ia-finais_de_prompt = Finais de prompt a mais
    .descricao = Um por linha. O app reconhece # $ % e > como fim do prompt; o que você puser aqui vale também, para saber quando um comando terminou.
    .palavras = mcp prompt terminal executar fim comando

config-ia-pasta_troca = Pasta para trocar arquivos com o assistente
    .descricao = Onde caem os arquivos que o assistente baixa e de onde sobem os que ele envia. Vazio é Downloads/Tirech Omni.
    .palavras = mcp arquivos troca baixar enviar pasta

config-ia-preferido = Assistente do atalho
    .descricao = O que o Ctrl+Alt+A abre, instala ou verifica na aba SSH em que você está.
    .palavras = ia assistente claude codex opencode atalho preferido padrao

config-ia-abrir_em = Abrir o assistente em
    .descricao = Numa aba nova, num painel livre da grade ou digitando o comando na aba atual (só no shell, fora de programas de tela cheia).
    .palavras = ia assistente abrir aba painel janela onde

config-ia-shell_ao_sair = Ficar num shell quando o assistente terminar
    .descricao = A aba continua aberta, na mesma pasta, em vez de fechar.
    .palavras = ia assistente sair terminar fechar shell aba

config-ia-tunel_codex = Abrir a porta 1455 no login do Codex
    .descricao = Deixa o login pelo navegador deste computador funcionar. Se a porta estiver em uso, use Entrar > Codex (com código).
    .palavras = ia codex login tunel porta 1455 navegador

config-ia-verificar_ao_abrir = Verificar os assistentes ao abrir uma aba SSH
    .descricao = Roda um comando curto no servidor para saber o que está instalado. O resultado vale por 10 minutos.
    .palavras = ia assistente verificar sondar instalado servidor

config-ia-mostrar_botao = Mostrar o botão dos assistentes na barra
    .descricao = O menu também fica em Ferramentas, no menu da aba e no menu da conexão.
    .palavras = ia assistente botao barra cabecalho

config-ia-mostrar_claude = Mostrar o Claude Code nos menus
    .descricao = Desligado, ele some dos menus e do que o atalho abre.
    .palavras = ia claude code anthropic menu

config-ia-mostrar_codex = Mostrar o Codex nos menus
    .descricao = Desligado, ele some dos menus e do que o atalho abre.
    .palavras = ia codex openai menu

config-ia-mostrar_opencode = Mostrar o OpenCode nos menus
    .descricao = Desligado, ele some dos menus e do que o atalho abre.
    .palavras = ia opencode menu

config-ia-metodo_claude = Instalar o Claude Code por
    .descricao = Automático usa o instalador oficial e, se o servidor não der, o npm.
    .palavras = ia claude instalar metodo npm homebrew nativo

config-ia-metodo_codex = Instalar o Codex por
    .descricao = Automático usa o instalador oficial e, se o servidor não der, o npm.
    .palavras = ia codex instalar metodo npm homebrew nativo

config-ia-metodo_opencode = Instalar o OpenCode por
    .descricao = Automático usa o instalador oficial e, se o servidor não der, o npm.
    .palavras = ia opencode instalar metodo npm homebrew nativo

config-ia-argumentos_claude = Argumentos do Claude Code
    .descricao = Vão depois do comando, como você escrever. A conexão pode ter os dela.
    .palavras = ia claude argumentos opcoes parametros

config-ia-argumentos_codex = Argumentos do Codex
    .descricao = Vão depois do comando, como você escrever. A conexão pode ter os dela.
    .palavras = ia codex argumentos opcoes parametros

config-ia-argumentos_opencode = Argumentos do OpenCode
    .descricao = Vão depois do comando, como você escrever. A conexão pode ter os dela.
    .palavras = ia opencode argumentos opcoes parametros

config-ia-local_pasta = Pasta de trabalho dos assistentes
    .descricao = Onde o Controlar com abre o Claude Code, o Codex e o OpenCode deste computador, e onde fica o que eles criam. Vazia usa uma pasta do app, longe dos seus arquivos.
    .palavras = ia claude codex opencode pasta trabalho diretorio lixo limpar controlar

config-acao-ia_local_abrir_pasta = Abrir a pasta de trabalho
    .descricao = Mostra no gerenciador de arquivos o que os assistentes deixaram.
    .palavras = ia claude codex pasta trabalho abrir

config-acao-ia_local_limpar = Limpar a pasta de trabalho…
    .descricao = Apaga tudo o que está dentro dela. Os assistentes abertos continuam.
    .palavras = ia claude codex pasta trabalho limpar apagar lixo

config-ia-local_claude_caminho = Caminho do Claude Code
    .descricao = O programa deste computador. Vazio procura nas pastas do sistema e do seu usuário.
    .palavras = ia claude caminho executavel programa computador local

config-ia-local_claude_argumentos = Argumentos do Claude Code
    .descricao = Vão depois do comando quando você usa Controlar com Claude Code. É onde entram as opções que você quiser.
    .palavras = ia claude argumentos opcoes parametros computador local controlar

config-ia-local_codex_caminho = Caminho do Codex
    .descricao = O programa deste computador. Vazio procura nas pastas do sistema e do seu usuário.
    .palavras = ia codex caminho executavel programa computador local

config-ia-local_codex_argumentos = Argumentos do Codex
    .descricao = Vão depois do comando quando você usa Controlar com Codex. É onde entram as opções que você quiser.
    .palavras = ia codex argumentos opcoes parametros computador local controlar

config-ia-local_opencode_caminho = Caminho do OpenCode
    .descricao = O programa deste computador. Vazio procura nas pastas do sistema e do seu usuário.
    .palavras = ia opencode caminho executavel programa computador local

config-ia-local_opencode_argumentos = Argumentos do OpenCode
    .descricao = Vão depois do comando quando você usa Controlar com OpenCode. É onde entram as opções que você quiser.
    .palavras = ia opencode argumentos opcoes parametros computador local controlar

config-ia-conversas_lembrar = Lembrar as conversas de cada aba
    .descricao = Guarda o id das conversas do Claude Code, do Codex e do OpenCode com cada aba, para continuar depois. Desligado, o Controlar com sempre começa uma conversa nova.
    .palavras = ia claude codex opencode conversa sessao resume continuar diario historico lembrar

config-ia-conversas_reabrir = Reabrir os assistentes ao abrir o app
    .descricao = Ao abrir o app ou um espaço, os assistentes que estavam controlando as abas voltam, continuando a mesma conversa.
    .palavras = ia claude codex opencode conversa reabrir restaurar continuar espaco

config-ia-conversas_ao_controlar = Quando a aba já teve uma conversa
    .descricao = O que o Controlar com faz quando o assistente já conversou sobre aquela aba: perguntar, continuar a mais recente ou começar outra.
    .palavras = ia claude codex opencode conversa continuar nova perguntar controlar

config-acao-mcp_como = Como ligar um assistente…
    .descricao = Os comandos para o Claude Code e o Codex, prontos para copiar.
    .palavras = mcp claude codex ligar configurar ponte

config-acao-mcp_registro = Ver o registro de hoje
    .descricao = Abre o arquivo do dia com o que os assistentes fizeram.
    .palavras = mcp auditoria registro log

config-acao-mcp_pasta_registro = Abrir a pasta do registro
    .descricao = A pasta com um arquivo por dia.
    .palavras = mcp auditoria registro log pasta

config-acao-mcp_espaco_novo = Novo espaço de trabalho…
    .descricao = Uma pasta de um servidor que o assistente lê e edita direto, sem abrir terminal.
    .palavras = mcp espaco trabalho pasta servidor ssh sftp

# Fim da página

config-acao-exportar = Exportar configurações
    .descricao = Grava um arquivo com as configurações, sem senhas.
    .palavras = backup salvar json

config-acao-importar = Importar configurações
    .descricao = Lê um arquivo exportado e troca as configurações.
    .palavras = restaurar carregar json

config-acao-restaurar_tudo = Restaurar todas as configurações
    .descricao = Volta tudo ao padrão e mantém a pasta de dados.
    .palavras = padrao resetar limpar

## Opções das escolhas (`escolha!`): `opcao-<escolha>-<variante>`

# Tema
opcao-tema-sistema = Usar o tema do sistema
opcao-tema-claro = Claro
opcao-tema-escuro = Escuro

# Esquema
opcao-esquema-campbell = Campbell
opcao-esquema-tema = Seguir o tema do app
opcao-esquema-campbell-powershell = Campbell Powershell
opcao-esquema-vintage = Vintage
opcao-esquema-one-half-dark = One Half Dark
opcao-esquema-one-half-light = One Half Light
opcao-esquema-tango-dark = Tango Dark
opcao-esquema-tango-light = Tango Light

# Cursor
opcao-cursor-barra = Barra
opcao-cursor-bloco = Bloco
opcao-cursor-sublinhado = Sublinhado

# BotaoDireito
opcao-botao-direito-menu = Abrir o menu
opcao-botao-direito-copiar-colar = Copiar e colar

# ColarArquivos
opcao-colar-arquivos-enviar = Enviar e colar o caminho
opcao-colar-arquivos-caminho-local = Colar só o caminho local
opcao-colar-arquivos-perguntar = Perguntar

# Campainha
opcao-campainha-piscar = Piscar a tela
opcao-campainha-nenhuma = Nenhuma

# TmuxAoFechar
opcao-tmux-ao-fechar-perguntar = Perguntar
opcao-tmux-ao-fechar-encerrar = Encerrar a sessão
opcao-tmux-ao-fechar-manter = Manter no servidor

# ConversaAoControlar
opcao-conversa-ao-controlar-perguntar = Perguntar
opcao-conversa-ao-controlar-continuar = Continuar a mais recente
opcao-conversa-ao-controlar-nova = Começar uma nova

# ChaveNova
opcao-chave-nova-perguntar = Perguntar
opcao-chave-nova-aceitar = Aceitar e gravar

# X11Padrao
opcao-x11-padrao-desligado = Desligado
opcao-x11-padrao-nao-confiavel = Não confiável
opcao-x11-padrao-confiavel = Confiável

# ModoReconectar
opcao-modo-reconectar-oferecer = Oferecer reconectar
opcao-modo-reconectar-nada = Não fazer nada
opcao-modo-reconectar-sozinho = Reconectar sozinho

# AbrirEm
opcao-abrir-em-painel-ativo = Painel ativo da última janela ativa
opcao-abrir-em-nova-janela = Nova janela

# TipoChave
opcao-tipo-chave-ed25519 = Ed25519
opcao-tipo-chave-ecdsa256 = ECDSA P-256
opcao-tipo-chave-ecdsa384 = ECDSA P-384
opcao-tipo-chave-ecdsa521 = ECDSA P-521
opcao-tipo-chave-rsa3072 = RSA 3072
opcao-tipo-chave-rsa4096 = RSA 4096

# ConflitoCfg
opcao-conflito-cfg-perguntar = Perguntar
opcao-conflito-cfg-sobrescrever = Sobrescrever
opcao-conflito-cfg-retomar = Retomar
opcao-conflito-cfg-pular = Pular
opcao-conflito-cfg-renomear = Renomear

# EnviarEditado
opcao-enviar-editado-perguntar = Perguntar
opcao-enviar-editado-sempre = Sempre

# ValidadeLink
opcao-validade-link-uma-hora = 1 hora
opcao-validade-link-um-dia = 1 dia
opcao-validade-link-sete-dias = 7 dias

# AbrirEspaco
opcao-abrir-espaco-nova-janela = Nova janela
opcao-abrir-espaco-janela-ativa = Janela ativa

# AoFechar
opcao-ao-fechar-perguntar = Perguntar
opcao-ao-fechar-bandeja = Deixar na bandeja
opcao-ao-fechar-fechar = Fechar o app

# ModoNotas
opcao-modo-notas-editar = Editar
opcao-modo-notas-dividir = Dividir
opcao-modo-notas-ler = Ler

# ModoComparar
opcao-modo-comparar-tamanho = Tamanho
opcao-modo-comparar-data = Data de modificação

# PaginaInicial
opcao-pagina-inicial-auto = Automática
opcao-pagina-inicial-inicio = Início
opcao-pagina-inicial-terminais = Terminais

# Idioma: os nomes dos idiomas saem na própria língua, nos dois arquivos
opcao-idioma-sistema = Usar o idioma do sistema
opcao-idioma-pt-br = Português (Brasil)
opcao-idioma-en-us = English
opcao-idioma-es-es = Español

## Página

config-titulo = Configurações
config-buscar = Buscar configurações
config-expandir-tudo = Expandir tudo
config-recolher-tudo = Recolher tudo
config-busca-vazia = Nenhuma configuração encontrada para "{ $busca }".
config-avisos-leitura = Algumas configurações voltaram ao padrão
config-desfazer = Desfazer
config-desfeito = Desfeito.
config-nada-mudou = Nada mudou: já estava assim.
config-restaurar-secao = Restaurar os padrões desta seção
config-secao-restaurada = { $secao }: os padrões foram restaurados.
config-tudo-restaurado = Todas as configurações voltaram ao padrão.
config-escolher = Escolher…
config-limpar-pasta-titulo = Limpar a pasta de trabalho?
config-limpar-pasta-texto = { $n ->
    [one] Apagar 1 item de { $pasta }? Não dá para desfazer.
    *[other] Apagar { $n } itens de { $pasta }? Não dá para desfazer.
    }
config-limpar-pasta-apagar = Apagar
config-limpar-pasta-vazia = A pasta de trabalho já está vazia.
config-limpar-pasta-feito = { $n ->
    [one] 1 item apagado da pasta de trabalho.
    *[other] { $n } itens apagados da pasta de trabalho.
    }
config-limpar-pasta-erro = { $n ->
    [one] 1 item apagado, mas outros não saíram: { $erro }
    *[other] { $n } itens apagados, mas outros não saíram: { $erro }
    }
config-limpar-pasta-recusada = A pasta de trabalho é a sua pasta pessoal ou a raiz do disco: o app não a esvazia. Escolha uma pasta só para os assistentes.
config-abrir-pasta-erro = Não consegui abrir { $pasta }: { $erro }
config-bandeja-indisponivel = Este sistema não mostra ícones na bandeja (no GNOME, instale a extensão AppIndicator).
config-erro-abrir = Não consegui abrir { $caminho }: { $erro }
config-erro-gravar = Não consegui gravar { $caminho }: { $erro }
config-erro-ler = Não consegui ler { $caminho }: { $erro }
config-exportar-titulo = Exportar as configurações
config-exportado = Configurações exportadas para { $caminho }.
config-importar-titulo = Importar as configurações
config-importado = Configurações importadas de { $caminho }.

# O idioma: a troca vale ao reiniciar.
config-geral-idioma-aviso = O idioma novo vale depois de reiniciar o { -app }.
config-geral-idioma-reiniciar = Reiniciar agora

# A pasta de dados.
config-pasta-origem-variavel = { $pasta } (definida por { $variavel }; para mudar, tire a variável).
config-pasta-origem-portatil = { $pasta } (versão portable: a pasta tirech-omnidata ao lado do programa).
config-pasta-portatil-senha = O cofre desta pasta não tem senha mestra
    .descricao = Quem copiar a pasta tirech-omnidata abre as senhas guardadas. Para cifrar o cofre com uma senha sua, use Cofre > Senha mestra…
config-pasta-origem-escolhida = { $pasta } (escolhida por você).
config-pasta-origem-padrao = { $pasta } (a padrão).
config-pasta-titulo = Pasta de dados
config-pasta-abrir = Abrir a pasta
config-pasta-mudar = Mudar…
    .descricao = Copia os dados para uma pasta vazia, ou usa uma que já tenha dados.
config-pasta-padrao = Voltar ao padrão
config-pasta-reiniciar = Reiniciar agora
    .descricao = A nova pasta, { $pasta }, vale depois de reiniciar.
config-pasta-copiou = Os dados foram copiados para { $pasta }. Reinicie para usá-la.
config-pasta-apontou = O app vai usar os dados que já estão em { $pasta }. Reinicie para usá-la.
config-pasta-erro-mudar = Não consegui mudar a pasta de dados: { $erro }.
config-pasta-voltou = A pasta de dados volta a { $pasta }. Reinicie para usá-la.
config-pasta-erro-padrao = Não consegui voltar ao padrão: { $erro }.

# O local do cofre.
config-cofre-local-titulo = Local do cofre
config-cofre-local-pasta-dados = { $pasta } (a pasta de dados).
config-cofre-local-pasta = { $pasta }.
config-cofre-local-abrir = Abrir a pasta
config-cofre-local-mudar = Mudar…
    .descricao = Leva o cofre para outra pasta, ou usa o que já estiver lá.
config-cofre-local-padrao = Voltar à pasta de dados

# O teste do rclone e a senha da configuração dele.
config-rclone-nao-achei = Não encontrei o rclone. Instale-o ou escolha o programa acima.
config-rclone-terminou = { $programa } terminou com { $status }.
config-rclone-erro-rodar = Não consegui rodar { $programa }: { $erro }
config-credencial-nenhuma = Nenhum item do cofre
config-credencial-inexistente = Não há um item "{ $titulo }" no cofre (ou o cofre está bloqueado).

# Atalhos.
config-titulo-atalhos = Atalhos
    .resumo = Teclas do app e dos painéis
config-atalhos-palavras = atalho atalhos teclado tecla
config-atalhos-app = { -app }
    .descricao = Nova conexão, layouts, cofre, busca e as outras ações do app.
config-atalhos-paineis = Painéis do terminal
    .descricao = Dividir, mover o foco, redimensionar, fechar e ampliar os painéis.
config-restaurar-atalhos = Restaurar todos os atalhos
config-atalho-restaurar = Restaurar
config-atalho-sem-padrao = Sem atalho de fábrica.
config-atalho-padrao = Padrão: { $atalho }.
config-atalho-tambem = Também usado por { $outra }.

# Dicas dos campos (o que vale quando o campo está vazio).
config-dica-shell = $SHELL
config-dica-argumentos-shell = -l
config-dica-pasta-shell = a pasta pessoal
config-dica-logs = logs na pasta de dados
config-dica-pasta-colar = /tmp/tirech-colar
config-dica-usuario = o usuário do sistema
config-dica-term = xterm-256color
config-dica-known-hosts = ~/.ssh/known_hosts
config-dica-interpretador = sh -s
config-dica-scripts = scripts na pasta de dados
config-dica-socket = um endereço do app
config-dica-pasta-chaves = ~/.ssh
config-dica-editor = o programa do sistema
config-dica-pasta-local = a pasta pessoal
config-dica-regiao = us-east-1
config-dica-classe = a do bucket
config-dica-rclone = o rclone do PATH
config-dica-rclone-config = o padrão do rclone
config-dica-navegador = Chrome, Edge, Brave, Chromium ou Firefox
config-dica-display = a variável DISPLAY
config-dica-virt-manager = virt-manager --no-fork
config-dica-espacos = espacos na pasta de dados
config-dica-shell-ia = bash
config-dica-redigir-extras = um por linha; re:… para regex
config-dica-bloqueio-mcp = um por linha; re:… para regex
config-dica-finais-prompt = um por linha
config-dica-argumentos-ia = por exemplo --model sonnet
config-dica-caminho-ia-local = procurar automaticamente
config-dica-pasta-troca = Downloads/Tirech Omni
config-dica-atalho = Ctrl+Shift+N
atalho-nova-conexao = Nova conexão
atalho-novo-grupo = Novo grupo
atalho-shell-local = Abrir shell local
atalho-layout-1 = Layout 1
atalho-layout-2x2 = Layout 2 × 2
atalho-layout-3x3 = Layout 3 × 3
atalho-identificar-paineis = Identificar os painéis
atalho-painel-conexoes = Mostrar ou ocultar as conexões
atalho-painel-propriedades = Mostrar ou ocultar as propriedades
atalho-painel-notas = Mostrar ou ocultar a lista de notas
atalho-painel-fila = Mostrar ou ocultar as transferências
atalho-nova-janela = Nova janela
atalho-multi-entrada = Envio para vários
atalho-cluster = Modo cluster
atalho-cluster-ia = Cluster com IA
atalho-scripts = Rodar script
atalho-links = Abrir um link
atalho-reconectar-caidas = Reconectar as abas caídas
atalho-fechar-caidas = Fechar as abas caídas
atalho-fechar-todas = Fechar todas as abas
atalho-cofre = Abrir o cofre
atalho-notas = Abrir as notas
atalho-ia-comando = Gerar comando com IA
atalho-ia-chat = Abrir ou fechar o chat com a IA
atalho-ia-abrir = Abrir o assistente de IA no servidor
atalho-busca = Buscar conexões e itens do cofre
atalho-configuracoes = Abrir as configurações
atalho-aumentar-fonte = Aumentar a fonte do terminal
atalho-diminuir-fonte = Diminuir a fonte do terminal
atalho-fonte-padrao = Tamanho padrão da fonte do terminal
atalho-dividir-abaixo = Dividir o painel para baixo
atalho-dividir-direita = Dividir o painel para a direita
atalho-duplicar-sessao = Duplicar a sessão num painel novo
atalho-focar-esquerda = Focar o painel da esquerda
atalho-focar-direita = Focar o painel da direita
atalho-focar-acima = Focar o painel de cima
atalho-focar-abaixo = Focar o painel de baixo
atalho-redimensionar-esquerda = Mover a divisão para a esquerda
atalho-redimensionar-direita = Mover a divisão para a direita
atalho-redimensionar-acima = Mover a divisão para cima
atalho-redimensionar-abaixo = Mover a divisão para baixo
atalho-fechar-painel = Fechar o painel
atalho-zoom-painel = Alternar o zoom do painel
config-logs-existentes = { $n ->
    [one] A pasta dos logs tem 1 log de texto, gravado antes de os logs serem cifrados.
    *[other] A pasta dos logs tem { $n } logs de texto, gravados antes de os logs serem cifrados.
    }
config-logs-existentes-cifrar = { $n ->
    [one] Cifrar o log de texto que já existe
    *[other] Cifrar os { $n } logs de texto que já existem
    }
config-ia-extra-invalido = { $texto }: { $erro }

# AssistentePreferido
opcao-assistente-claude = Claude Code
opcao-assistente-codex = Codex
opcao-assistente-opencode = OpenCode

# AbrirIa
opcao-abrir-ia-aba-nova = Numa aba nova
opcao-abrir-ia-painel-livre = Num painel livre
opcao-abrir-ia-nesta-aba = Nesta aba

# MetodoIa
opcao-metodo-ia-automatico = Automático
opcao-metodo-ia-nativo = Instalador oficial
opcao-metodo-ia-npm = npm
opcao-metodo-ia-homebrew = Homebrew
