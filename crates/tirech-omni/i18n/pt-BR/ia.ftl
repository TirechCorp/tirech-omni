# A IA dentro do app (M52): a redação de segredos, os perfis, o comando, o chat.

redacao-do-cofre = { $n } do cofre
redacao-por-padrao = { $n } por padrão
redacao-dos-extras = { $n } dos seus extras
redacao-removi = Segredos removidos antes de sair: { $total } ({ $partes }).

ia-dialeto-openai = Compatível com OpenAI
ia-dialeto-anthropic = Anthropic (Messages)

ia-preset-ollama = Local (Ollama)
ia-preset-lmstudio = LM Studio
ia-preset-openrouter = OpenRouter
ia-preset-openai = OpenAI
ia-preset-anthropic = Anthropic
ia-preset-groq = Groq
ia-preset-personalizado = Personalizado
ia-preset-obs-local = Nada sai do computador.
ia-preset-obs-openrouter = Vários modelos com uma chave só.
ia-preset-obs-personalizado = Qualquer servidor compatível.

ia-cartao-ollama = Roda no seu computador; nada sai daqui. Precisa do Ollama instalado.
ia-cartao-lmstudio = Roda no seu computador; nada sai daqui. Precisa do LM Studio com o servidor local ligado.
ia-cartao-openrouter = Vários modelos com uma chave só.
ia-cartao-openai = Os modelos da OpenAI. Precisa de uma chave de API.
ia-cartao-anthropic = Os modelos da Anthropic. Precisa de uma chave de API.
ia-cartao-groq = Modelos abertos em servidores da Groq. Precisa de uma chave de API.
ia-cartao-personalizado = Qualquer servidor compatível, na sua rede ou na internet.

ia-dica-ollama = O Ollama está rodando? Abra o Ollama ou rode `ollama serve`.
ia-dica-lmstudio = Ligue o servidor local na aba Developer do LM Studio.
ia-o-que = de provedores de IA

# A página "Assistentes de IA" e o diálogo de escolher um provedor
ia-pagina-titulo = Assistentes de IA
ia-pagina-descricao = Os provedores de IA que o app usa para gerar comandos, explicar o que aparece no terminal e conversar. A chave de API fica no cofre, nunca no arquivo dos provedores.
ia-lista-vazia = Nenhum provedor ainda. Use Novo para escolher um.
ia-lista-padrao = { $nome } · padrão
ia-sem-escolha = Escolha um provedor na lista ou use Novo para criar um.
ia-testar = Testar
ia-usar-como-padrao = Usar como padrão
ia-rotulo-provedor = Provedor
ia-rotulo-dialeto = Formato da API
ia-rotulo-url = Endereço (até o /v1)
ia-rotulo-modelo = Modelo
ia-rotulo-chave = Chave de API
ia-rotulo-item = Usar um item do cofre
ia-rotulo-max-tokens = Limite de tokens da resposta
ia-rotulo-temperatura = Temperatura
ia-rotulo-tls = Certificado do servidor
ia-rotulo-ca = Arquivo do certificado (.pem)
ia-rotulo-cabecalhos = Cabeçalhos a mais (Nome: valor, um por linha)
ia-avancado = Opções avançadas
ia-padrao-do-provedor = O do provedor
ia-modelo-placeholder = Escreva o nome ou atualize a lista
ia-modelos-atualizar = Atualizar a lista
ia-modelos-nenhum = O provedor não mandou nenhum modelo. Escreva o nome à mão.
ia-modelos-erro = Não consegui a lista de modelos: { $erro }
ia-chave-placeholder = Cole a chave aqui
ia-chave-guardar = Guardar no cofre
ia-chave-titulo = Chave de API — { $nome }
ia-chave-guardada = Chave guardada no cofre.
ia-chave-sem = Sem chave.
ia-chave-no-item = Chave no item do cofre: { $titulo }
ia-chave-no-cofre-bloqueado = A chave está no cofre, que está bloqueado.
ia-chave-item-sumiu = O item do cofre com a chave não existe mais.
ia-local-maquina = Neste computador
ia-local-rede = Na rede local
ia-local-internet = Na internet
ia-aviso-internet = O texto do terminal vai sair deste computador para { $host }. A redação remove os segredos que ela reconhece, mas não todos.
ia-tls-sistema = Do sistema
ia-tls-ca = Certificado próprio
ia-tls-nao-verificar = Não verificar (inseguro)
ia-url-invalida = O endereço precisa começar com http:// ou https://.
ia-testando = Testando { $nome }…
ia-teste-pergunta = Responda só: ok
ia-teste-ok = { $nome } respondeu.
ia-teste-falhou = { $nome } não respondeu: { $erro }
ia-excluir-texto = O provedor sai da lista. O item do cofre com a chave dele continua lá, a menos que você marque a caixa abaixo.
ia-excluir-item-do-cofre = Apagar também o item do cofre com a chave
ia-escolher-titulo = Escolher um provedor de IA
ia-escolher-texto = Escolha um para começar. Dá para mudar tudo depois, em Assistentes de IA.
ia-url-exemplo = https://servidor/v1
ia-ca-exemplo = /caminho/ca.pem

# A linha "Assistente de IA" na ficha da conexão
ia-conexao-categoria = Assistente de IA
ia-conexao-linha = Provedor de IA
ia-conexao-descricao = O provedor que responde nesta conexão. Padrão usa o escolhido na página Assistentes de IA.
ia-conexao-padrao = Padrão ({ $nome })
ia-conexao-padrao-vazio = Padrão (nenhum)

# O que vai sair e o aviso de internet (M52)
ia-internet-titulo = Enviar para a internet?
ia-internet-texto = O texto do terminal vai sair deste computador para { $host }. A redação remove os segredos que ela reconhece, mas não todos. Continuar?
ia-sai-titulo = Enviar para { $nome }?
ia-sai-destino = { $url } · { $onde }
ia-sai-instrucoes = Instruções para o modelo
ia-sai-cortei = Cortei { $n } linhas antigas para caber em { $max } caracteres.
ia-sai-cofre-bloqueado = O cofre está bloqueado: só os padrões comuns foram procurados.
ia-sai-nao-perguntar-aba = Não perguntar de novo nesta aba
ia-sai-nao-perguntar-mais = Não perguntar mais
ia-sai-enviar = Enviar

# Gerar comando (M52, fatia a)
ia-comando-exemplo = Descreva o que quer fazer, por exemplo: achar os arquivos maiores que 1 GB nesta pasta
ia-comando-com-tela = Mandar as últimas { $n } linhas do terminal
ia-comando-com-selecao = { $n ->
    [one] Com o texto selecionado (1 linha)
    *[other] Com o texto selecionado ({ $n } linhas)
    }
ia-comando-tirar-selecao = Não mandar o texto selecionado
ia-comando-gerando = Gerando…
ia-comando-vazio = A resposta veio sem nenhum comando. Tente de novo ou descreva o pedido de outro jeito.
ia-comando-perigo = Este comando apaga ou sobrescreve dados. Leia antes de inserir.
ia-comando-inserir = Inserir
ia-comando-inserir-uma-linha = Inserir em uma linha
ia-comando-de-novo = De novo
ia-comando-sem-terminal = Abra um terminal para gerar um comando.
ia-comando-explicar = Explicar no chat

# O chat ao lado do terminal.
ia-chat-painel = Chat com a IA
ia-chat-titulo = Chat: { $aba }
ia-chat-titulo-sem-aba = Chat com a IA
ia-chat-vazio = Pergunte sobre o que está no terminal. A conversa fica nesta aba, e a IA só sugere: nada roda sem você.
ia-chat-pergunte = Pergunte alguma coisa…
ia-chat-enviar = Enviar
ia-chat-nova = Nova conversa
ia-chat-copiar = Copiar a última resposta
ia-chat-gerando = Pensando na resposta…
ia-chat-inserir = Inserir no terminal
ia-chat-ver-enviado = Ver o que foi enviado
ia-chat-ver-titulo = O que foi enviado
ia-chat-cortou = Cortei { $n } mensagens antigas da conversa para caber no limite.
ia-chat-resposta-vazia = A resposta veio vazia. Tente de novo.
ia-chat-aba-fechada = A aba desta conversa já foi fechada.
ia-chat-uma-linha = O texto tinha várias linhas e este terminal não aceita colar em bloco: inseri tudo em uma linha.
ia-chat-sem-selecao = Selecione um trecho do terminal para explicar.
ia-chat-contexto = Contexto: { $itens }
ia-chat-ctx-selecao = Seleção
ia-chat-ctx-linhas = Últimas { $n } linhas
ia-chat-ctx-erro = Último comando e saída
ia-chat-ctx-sistema = Sistema
ia-chat-ctx-nome-selecao = seleção
ia-chat-ctx-nome-linhas = últimas { $n } linhas
ia-chat-ctx-nome-erro = último comando e saída
ia-chat-ctx-nome-sistema = sistema
ia-chat-pedido-selecao = Explique a seleção.
ia-chat-pedido-erro = Explique o erro.
ia-chat-pedido-comando = Explique o comando: { $comando }

# O menu do terminal.
terminal-ia-explicar-selecao = Explicar a seleção
terminal-ia-explicar-erro = Explicar o erro
terminal-ia-gerar = Gerar comando…
terminal-ia-perguntar = Perguntar à IA…
terminal-ia-chat = Abrir o chat
