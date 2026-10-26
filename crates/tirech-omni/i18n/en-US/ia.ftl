# The AI inside the app (M52): secret redaction, profiles, the command, the chat.

redacao-do-cofre = { $n } from the vault
redacao-por-padrao = { $n } by pattern
redacao-dos-extras = { $n } from your extras
redacao-removi = Secrets removed before sending: { $total } ({ $partes }).

ia-dialeto-openai = OpenAI-compatible
ia-dialeto-anthropic = Anthropic (Messages)

ia-preset-ollama = Local (Ollama)
ia-preset-lmstudio = LM Studio
ia-preset-openrouter = OpenRouter
ia-preset-openai = OpenAI
ia-preset-anthropic = Anthropic
ia-preset-groq = Groq
ia-preset-personalizado = Custom
ia-preset-obs-local = Nothing leaves your computer.
ia-preset-obs-openrouter = Many models with a single key.
ia-preset-obs-personalizado = Any compatible server.

ia-cartao-ollama = Runs on your computer; nothing leaves it. Needs Ollama installed.
ia-cartao-lmstudio = Runs on your computer; nothing leaves it. Needs LM Studio with the local server on.
ia-cartao-openrouter = Many models with a single key.
ia-cartao-openai = OpenAI's models. Needs an API key.
ia-cartao-anthropic = Anthropic's models. Needs an API key.
ia-cartao-groq = Open models on Groq servers. Needs an API key.
ia-cartao-personalizado = Any compatible server, on your network or on the internet.

ia-dica-ollama = Is Ollama running? Open Ollama or run `ollama serve`.
ia-dica-lmstudio = Turn on the local server in LM Studio's Developer tab.
ia-o-que = of AI providers

# The "AI assistants" page and the dialog to choose a provider
ia-pagina-titulo = AI assistants
ia-pagina-descricao = The AI providers the app uses to generate commands, explain what shows in the terminal and chat. The API key stays in the vault, never in the providers file.
ia-lista-vazia = No providers yet. Use New to pick one.
ia-lista-padrao = { $nome } · default
ia-sem-escolha = Pick a provider from the list or use New to create one.
ia-testar = Test
ia-usar-como-padrao = Use as default
ia-rotulo-provedor = Provider
ia-rotulo-dialeto = API format
ia-rotulo-url = Address (up to /v1)
ia-rotulo-modelo = Model
ia-rotulo-chave = API key
ia-rotulo-item = Use a vault item
ia-rotulo-max-tokens = Reply token limit
ia-rotulo-temperatura = Temperature
ia-rotulo-tls = Server certificate
ia-rotulo-ca = Certificate file (.pem)
ia-rotulo-cabecalhos = Extra headers (Name: value, one per line)
ia-avancado = Advanced options
ia-padrao-do-provedor = The provider's
ia-modelo-placeholder = Type the name or refresh the list
ia-modelos-atualizar = Refresh the list
ia-modelos-nenhum = The provider sent no models. Type the name by hand.
ia-modelos-erro = Could not get the model list: { $erro }
ia-chave-placeholder = Paste the key here
ia-chave-guardar = Save to the vault
ia-chave-titulo = API key — { $nome }
ia-chave-guardada = Key saved to the vault.
ia-chave-sem = No key.
ia-chave-no-item = Key in the vault item: { $titulo }
ia-chave-no-cofre-bloqueado = The key is in the vault, which is locked.
ia-chave-item-sumiu = The vault item with the key no longer exists.
ia-local-maquina = On this computer
ia-local-rede = On the local network
ia-local-internet = On the internet
ia-aviso-internet = The terminal text will leave this computer for { $host }. Redaction removes the secrets it recognizes, but not all of them.
ia-tls-sistema = From the system
ia-tls-ca = Own certificate
ia-tls-nao-verificar = Do not verify (insecure)
ia-url-invalida = The address must start with http:// or https://.
ia-testando = Testing { $nome }…
ia-teste-pergunta = Reply only: ok
ia-teste-ok = { $nome } answered.
ia-teste-falhou = { $nome } did not answer: { $erro }
ia-excluir-texto = The provider leaves the list. The vault item with its key stays, unless you tick the box below.
ia-excluir-item-do-cofre = Also delete the vault item with the key
ia-escolher-titulo = Choose an AI provider
ia-escolher-texto = Pick one to start. You can change everything later, in AI assistants.
ia-url-exemplo = https://server/v1
ia-ca-exemplo = /path/ca.pem

# The "AI assistant" row on the connection sheet
ia-conexao-categoria = AI assistant
ia-conexao-linha = AI provider
ia-conexao-descricao = The provider that answers on this connection. Default uses the one chosen on the AI assistants page.
ia-conexao-padrao = Default ({ $nome })
ia-conexao-padrao-vazio = Default (none)

# What will be sent and the internet notice (M52)
ia-internet-titulo = Send to the internet?
ia-internet-texto = The terminal text will leave this computer for { $host }. Redaction removes the secrets it recognizes, but not all of them. Continue?
ia-sai-titulo = Send to { $nome }?
ia-sai-destino = { $url } · { $onde }
ia-sai-instrucoes = Instructions for the model
ia-sai-cortei = I cut { $n } older lines to fit in { $max } characters.
ia-sai-cofre-bloqueado = The vault is locked: only the common patterns were searched.
ia-sai-nao-perguntar-aba = Don't ask again in this tab
ia-sai-nao-perguntar-mais = Never ask again
ia-sai-enviar = Send

# Generate a command (M52, slice a)
ia-comando-exemplo = Describe what you want to do, for example: find files larger than 1 GB in this folder
ia-comando-com-tela = Send the last { $n } lines of the terminal
ia-comando-com-selecao = { $n ->
    [one] With the selected text (1 line)
    *[other] With the selected text ({ $n } lines)
    }
ia-comando-tirar-selecao = Don't send the selected text
ia-comando-gerando = Generating…
ia-comando-vazio = The answer had no command in it. Try again or describe the request another way.
ia-comando-perigo = This command deletes or overwrites data. Read it before inserting.
ia-comando-inserir = Insert
ia-comando-inserir-uma-linha = Insert on one line
ia-comando-de-novo = Again
ia-comando-sem-terminal = Open a terminal to generate a command.
ia-comando-explicar = Explain in the chat

# The chat next to the terminal.
ia-chat-painel = AI chat
ia-chat-titulo = Chat: { $aba }
ia-chat-titulo-sem-aba = AI chat
ia-chat-vazio = Ask about what is in the terminal. The conversation stays in this tab, and the AI only suggests: nothing runs without you.
ia-chat-pergunte = Ask something…
ia-chat-enviar = Send
ia-chat-nova = New conversation
ia-chat-copiar = Copy the last answer
ia-chat-gerando = Thinking about the answer…
ia-chat-inserir = Insert in the terminal
ia-chat-ver-enviado = See what was sent
ia-chat-ver-titulo = What was sent
ia-chat-cortou = I cut { $n } older messages from the conversation to fit the limit.
ia-chat-resposta-vazia = The answer came back empty. Try again.
ia-chat-aba-fechada = The tab of this conversation was already closed.
ia-chat-uma-linha = The text had several lines and this terminal does not accept block paste: I inserted it all on one line.
ia-chat-sem-selecao = Select some text in the terminal to explain it.
ia-chat-contexto = Context: { $itens }
ia-chat-ctx-selecao = Selection
ia-chat-ctx-linhas = Last { $n } lines
ia-chat-ctx-erro = Last command and output
ia-chat-ctx-sistema = System
ia-chat-ctx-nome-selecao = selection
ia-chat-ctx-nome-linhas = last { $n } lines
ia-chat-ctx-nome-erro = last command and output
ia-chat-ctx-nome-sistema = system
ia-chat-pedido-selecao = Explain the selection.
ia-chat-pedido-erro = Explain the error.
ia-chat-pedido-comando = Explain the command: { $comando }

# The terminal menu.
terminal-ia-explicar-selecao = Explain the selection
terminal-ia-explicar-erro = Explain the error
terminal-ia-gerar = Generate a command…
terminal-ia-perguntar = Ask the AI…
terminal-ia-chat = Open the chat
