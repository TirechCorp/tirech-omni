# La IA dentro de la aplicación (M52): la eliminación de secretos, los perfiles, el comando, el chat.

redacao-do-cofre = { $n } de la bóveda
redacao-por-padrao = { $n } por patrón
redacao-dos-extras = { $n } de tus extras
redacao-removi = Secretos eliminados antes de enviar: { $total } ({ $partes }).

ia-dialeto-openai = Compatible con OpenAI
ia-dialeto-anthropic = Anthropic (Messages)

ia-preset-ollama = Local (Ollama)
ia-preset-lmstudio = LM Studio
ia-preset-openrouter = OpenRouter
ia-preset-openai = OpenAI
ia-preset-anthropic = Anthropic
ia-preset-groq = Groq
ia-preset-personalizado = Personalizado
ia-preset-obs-local = Nada sale del equipo.
ia-preset-obs-openrouter = Varios modelos con una sola clave.
ia-preset-obs-personalizado = Cualquier servidor compatible.

ia-cartao-ollama = Se ejecuta en tu equipo; nada sale de aquí. Necesita Ollama instalado.
ia-cartao-lmstudio = Se ejecuta en tu equipo; nada sale de aquí. Necesita LM Studio con el servidor local activado.
ia-cartao-openrouter = Varios modelos con una sola clave.
ia-cartao-openai = Los modelos de OpenAI. Necesita una clave de API.
ia-cartao-anthropic = Los modelos de Anthropic. Necesita una clave de API.
ia-cartao-groq = Modelos abiertos en servidores de Groq. Necesita una clave de API.
ia-cartao-personalizado = Cualquier servidor compatible, en tu red o en internet.

ia-dica-ollama = ¿Se está ejecutando Ollama? Abre Ollama o ejecuta `ollama serve`.
ia-dica-lmstudio = Activa el servidor local en la pestaña Developer de LM Studio.
ia-o-que = de proveedores de IA

# La página "Asistentes de IA" y el cuadro de diálogo para elegir un proveedor
ia-pagina-titulo = Asistentes de IA
ia-pagina-descricao = Los proveedores de IA que usa la aplicación para generar comandos, explicar lo que aparece en el terminal y conversar. La clave de API queda en la bóveda, nunca en el archivo de los proveedores.
ia-lista-vazia = Todavía no hay ningún proveedor. Usa Nuevo para elegir uno.
ia-lista-padrao = { $nome } · predeterminado
ia-sem-escolha = Elige un proveedor de la lista o usa Nuevo para crear uno.
ia-testar = Probar
ia-usar-como-padrao = Usar como predeterminado
ia-rotulo-provedor = Proveedor
ia-rotulo-dialeto = Formato de la API
ia-rotulo-url = Dirección (hasta el /v1)
ia-rotulo-modelo = Modelo
ia-rotulo-chave = Clave de API
ia-rotulo-item = Usar un elemento de la bóveda
ia-rotulo-max-tokens = Límite de tokens de la respuesta
ia-rotulo-temperatura = Temperatura
ia-rotulo-tls = Certificado del servidor
ia-rotulo-ca = Archivo del certificado (.pem)
ia-rotulo-cabecalhos = Encabezados adicionales (Nombre: valor, uno por línea)
ia-avancado = Opciones avanzadas
ia-padrao-do-provedor = El del proveedor
ia-modelo-placeholder = Escribe el nombre o actualiza la lista
ia-modelos-atualizar = Actualizar la lista
ia-modelos-nenhum = El proveedor no envió ningún modelo. Escribe el nombre a mano.
ia-modelos-erro = No se pudo obtener la lista de modelos: { $erro }
ia-chave-placeholder = Pega la clave aquí
ia-chave-guardar = Guardar en la bóveda
ia-chave-titulo = Clave de API — { $nome }
ia-chave-guardada = Clave guardada en la bóveda.
ia-chave-sem = Sin clave.
ia-chave-no-item = Clave en el elemento de la bóveda: { $titulo }
ia-chave-no-cofre-bloqueado = La clave está en la bóveda, que está bloqueada.
ia-chave-item-sumiu = El elemento de la bóveda con la clave ya no existe.
ia-local-maquina = En este equipo
ia-local-rede = En la red local
ia-local-internet = En internet
ia-aviso-internet = El texto del terminal saldrá de este equipo hacia { $host }. La eliminación de secretos quita los que reconoce, pero no todos.
ia-tls-sistema = Del sistema
ia-tls-ca = Certificado propio
ia-tls-nao-verificar = No verificar (no seguro)
ia-url-invalida = La dirección debe empezar por http:// o https://.
ia-testando = Probando { $nome }…
ia-teste-pergunta = Responde solo: ok
ia-teste-ok = { $nome } respondió.
ia-teste-falhou = { $nome } no respondió: { $erro }
ia-excluir-texto = El proveedor sale de la lista. El elemento de la bóveda con su clave sigue allí, a menos que marques la casilla de abajo.
ia-excluir-item-do-cofre = Eliminar también el elemento de la bóveda con la clave
ia-escolher-titulo = Elegir un proveedor de IA
ia-escolher-texto = Elige uno para empezar. Puedes cambiarlo todo después, en Asistentes de IA.
ia-url-exemplo = https://servidor/v1
ia-ca-exemplo = /ruta/ca.pem

# La fila "Asistente de IA" en la ficha de la conexión
ia-conexao-categoria = Asistente de IA
ia-conexao-linha = Proveedor de IA
ia-conexao-descricao = El proveedor que responde en esta conexión. Predeterminado usa el elegido en la página Asistentes de IA.
ia-conexao-padrao = Predeterminado ({ $nome })
ia-conexao-padrao-vazio = Predeterminado (ninguno)

# Lo que se enviará y el aviso de internet (M52)
ia-internet-titulo = ¿Enviar a internet?
ia-internet-texto = El texto del terminal saldrá de este equipo hacia { $host }. La eliminación de secretos quita los que reconoce, pero no todos. ¿Continuar?
ia-sai-titulo = ¿Enviar a { $nome }?
ia-sai-destino = { $url } · { $onde }
ia-sai-instrucoes = Instrucciones para el modelo
ia-sai-cortei = Se quitaron { $n } líneas antiguas para que quepa en { $max } caracteres.
ia-sai-cofre-bloqueado = La bóveda está bloqueada: solo se buscaron los patrones comunes.
ia-sai-nao-perguntar-aba = No volver a preguntar en esta pestaña
ia-sai-nao-perguntar-mais = No preguntar más
ia-sai-enviar = Enviar

# Generar comando (M52, parte a)
ia-comando-exemplo = Describe lo que quieres hacer, por ejemplo: encontrar los archivos de más de 1 GB en esta carpeta
ia-comando-com-tela = Enviar las últimas { $n } líneas del terminal
ia-comando-com-selecao = { $n ->
    [one] Con el texto seleccionado (1 línea)
    *[other] Con el texto seleccionado ({ $n } líneas)
    }
ia-comando-tirar-selecao = No enviar el texto seleccionado
ia-comando-gerando = Generando…
ia-comando-vazio = La respuesta no trajo ningún comando. Vuelve a intentarlo o describe la petición de otra forma.
ia-comando-perigo = Este comando elimina o sobrescribe datos. Léelo antes de insertarlo.
ia-comando-inserir = Insertar
ia-comando-inserir-uma-linha = Insertar en una línea
ia-comando-de-novo = Otra vez
ia-comando-sem-terminal = Abre un terminal para generar un comando.
ia-comando-explicar = Explicar en el chat

# El chat junto al terminal.
ia-chat-painel = Chat con la IA
ia-chat-titulo = Chat: { $aba }
ia-chat-titulo-sem-aba = Chat con la IA
ia-chat-vazio = Pregunta sobre lo que hay en el terminal. La conversación queda en esta pestaña, y la IA solo sugiere: nada se ejecuta sin ti.
ia-chat-pergunte = Pregunta algo…
ia-chat-enviar = Enviar
ia-chat-nova = Nueva conversación
ia-chat-copiar = Copiar la última respuesta
ia-chat-gerando = Pensando la respuesta…
ia-chat-inserir = Insertar en el terminal
ia-chat-ver-enviado = Ver lo que se envió
ia-chat-ver-titulo = Lo que se envió
ia-chat-cortou = Se quitaron { $n } mensajes antiguos de la conversación para que quepa en el límite.
ia-chat-resposta-vazia = La respuesta llegó vacía. Vuelve a intentarlo.
ia-chat-aba-fechada = La pestaña de esta conversación ya se cerró.
ia-chat-uma-linha = El texto tenía varias líneas y este terminal no acepta pegar en bloque: se insertó todo en una línea.
ia-chat-sem-selecao = Selecciona un fragmento del terminal para explicarlo.
ia-chat-contexto = Contexto: { $itens }
ia-chat-ctx-selecao = Selección
ia-chat-ctx-linhas = Últimas { $n } líneas
ia-chat-ctx-erro = Último comando y salida
ia-chat-ctx-sistema = Sistema
ia-chat-ctx-nome-selecao = selección
ia-chat-ctx-nome-linhas = últimas { $n } líneas
ia-chat-ctx-nome-erro = último comando y salida
ia-chat-ctx-nome-sistema = sistema
ia-chat-pedido-selecao = Explica la selección.
ia-chat-pedido-erro = Explica el error.
ia-chat-pedido-comando = Explica el comando: { $comando }

# El menú del terminal.
terminal-ia-explicar-selecao = Explicar la selección
terminal-ia-explicar-erro = Explicar el error
terminal-ia-gerar = Generar comando…
terminal-ia-perguntar = Preguntar a la IA…
terminal-ia-chat = Abrir el chat
