# Os logs de sessão cifrados (M53): o formato e a leitura.

log-sem-aleatorio = sem aleatório: { $erro }
log-nao-cifrou = não consegui cifrar o log
log-chave-invalida = a chave pública dos logs cifrados é inválida
log-nao-e-do-app = o arquivo não é um log cifrado do app
log-alterado-no-meio = o log cifrado está alterado ou estragado
log-escrita-parou = a gravação do log cifrado parou depois de um erro de disco

log-erro-nao-e = O arquivo não é um log cifrado do app.
log-erro-versao-nova = Este log foi gravado por uma versão mais nova do app.
log-erro-chave-ausente = A chave deste log não está no cofre (id { $id }). Ela pode estar no cofre de outra máquina: junte os cofres em Cofre > Juntar.
log-erro-alterado = O log foi alterado ou está estragado a partir daqui (segmento { $segmento }, registro { $registro }). O texto anterior está abaixo.
log-erro-io = { $erro }
log-erro-cancelado = Cancelado.

log-chave-titulo = Chave dos logs cifrados
log-chave-notas = Abre os logs cifrados do { -app }. Se apagar este item, os logs cifrados com ele não abrem mais.

log-sem-chave-publica = falta a chave pública dos logs cifrados; destrave o cofre

log-filtro-cifrado = Log cifrado
log-destrave-para-cifrar = Para cifrar os logs, destrave o cofre.
log-nao-criou-a-chave = Não consegui criar a chave dos logs cifrados: { $erro }. Os logs voltaram a ser gravados em texto.

# A página Logs.
logs-titulo = Logs
logs-descricao = Os logs das sessões do terminal, em texto e cifrados. Os cifrados abrem aqui com a chave do cofre.
logs-conexao = Conexão
logs-modificado = Modificado
logs-gravando = Gravando
logs-vazio = Nenhum log ainda. Ligue os logs em Configurações > Terminal.
logs-escolha-um = Escolha um log na lista para ler.
logs-varios = { $n } logs escolhidos.
logs-destrave = Destrave o cofre para abrir este log
logs-destravar = Destravar
logs-quebrar-linhas = Quebrar linhas
logs-copiar-tudo = Copiar tudo
logs-buscar = Buscar no log
logs-buscar-dica = Buscar (Ctrl+F)
logs-diferenciar = Diferenciar maiúsculas
logs-mostrar-tudo = Mostrar tudo
logs-mostrando-o-fim = Mostrando o fim do log ({ $mostrado } de { $total }).
logs-sem-fim = Este log terminou sem fechar: o app pode ter fechado de repente. O texto até ali está abaixo.
logs-cortado = O fim do log estava incompleto ({ $bytes } bytes) e ficou de fora.
logs-lendo = Lendo { $nome }…
logs-leitura-cancelada = A leitura foi cancelada.
logs-atualizar-dica = Ler a pasta de novo
logs-exportar = Exportar como texto…
logs-exportar-titulo = Exportar o log como texto
logs-exportar-pasta-titulo = Exportar os logs como texto, em qual pasta?
logs-exportado-em = Log exportado em { $arquivo }.
logs-exportados-em = { $n ->
    [one] 1 log exportado em { $pasta }.
    *[other] { $n } logs exportados em { $pasta }.
    }
logs-exportados-com-avisos = { $n ->
    [one] 1 log exportado, mas com avisos: { $avisos }
    *[other] { $n } logs exportados, mas com avisos: { $avisos }
    }
logs-nao-exportou = Não consegui exportar { $nome }: { $erro }
logs-abrir-pasta = Abrir a pasta
logs-nao-abriu-a-pasta = Não consegui abrir { $caminho }: { $motivo }
logs-apagar-titulo = Apagar os logs?
logs-apagar-texto = { $n ->
    [one] Apagar 1 log?
    *[other] Apagar { $n } logs?
    }
logs-apagar-aviso = Os logs apagados não podem ser recuperados pelo app.
logs-apagar-gravando = { $n ->
    [one] 1 está sendo gravado e fica de fora.
    *[other] { $n } estão sendo gravados e ficam de fora.
    }
logs-apagados = { $n ->
    [one] 1 log apagado.
    *[other] { $n } logs apagados.
    }
logs-nao-apagou = Não consegui apagar { $nome }: { $erro }
logs-so-gravando = Os logs escolhidos estão sendo gravados e não podem ser apagados agora.
logs-abrir-da-aba = Abrir o log desta sessão
logs-sem-pasta-de-logs = Não consegui criar a pasta dos logs: { $erro }
logs-cifragem = Cifrar ou decifrar
logs-cifrar = Cifrar os escolhidos…
logs-decifrar = Decifrar os escolhidos…
logs-cifrar-todos = Cifrar todos os de texto…
logs-converter = Converter
logs-converter-titulo = Cifrar ou decifrar os logs
logs-converter-cifrar-texto = { $n ->
    [one] Cifrar 1 log de texto ({ $tamanho })?
    *[other] Cifrar { $n } logs de texto ({ $tamanho })?
    }
logs-converter-decifrar-texto = { $n ->
    [one] Decifrar 1 log ({ $tamanho })? O texto fica no disco, sem proteção.
    *[other] Decifrar { $n } logs ({ $tamanho })? O texto fica no disco, sem proteção.
    }
logs-converter-apagar-originais = Apagar os originais (de texto)
logs-converter-apagar-cifrados = Apagar os cifrados
logs-converter-nada-cifrar = Não há log de texto para cifrar aqui.
logs-converter-nada-decifrar = Não há log cifrado para decifrar entre os escolhidos.
logs-converter-so-gravando = Os logs estão sendo gravados e não podem ser convertidos agora.
logs-convertendo = Convertendo os logs…
logs-cifrados-n = { $n ->
    [one] 1 log cifrado.
    *[other] { $n } logs cifrados.
    }
logs-decifrados-n = { $n ->
    [one] 1 log decifrado.
    *[other] { $n } logs decifrados.
    }
logs-convertidos-gravando = { $n ->
    [one] 1 ficou de fora por estar sendo gravado.
    *[other] { $n } ficaram de fora por estarem sendo gravados.
    }
logs-convertidos-erros = { $n ->
    [one] 1 com erro: { $detalhes }
    *[other] { $n } com erro: { $detalhes }
    }
logs-convertidos-cancelado = Cancelado: os que faltavam ficaram como estavam.
