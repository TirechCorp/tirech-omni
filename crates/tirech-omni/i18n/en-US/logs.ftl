# Encrypted session logs (M53): the format and reading.

log-sem-aleatorio = no random source: { $erro }
log-nao-cifrou = couldn't encrypt the log
log-chave-invalida = the public key for encrypted logs is invalid
log-nao-e-do-app = the file isn't an encrypted log from the app
log-alterado-no-meio = the encrypted log is altered or damaged
log-escrita-parou = writing the encrypted log stopped after a disk error

log-erro-nao-e = The file isn't an encrypted log from the app.
log-erro-versao-nova = This log was written by a newer version of the app.
log-erro-chave-ausente = This log's key isn't in the vault (id { $id }). It may be in another computer's vault: merge the vaults in Vault > Merge.
log-erro-alterado = The log was altered or is damaged from this point on (segment { $segmento }, record { $registro }). The text before it is shown below.
log-erro-io = { $erro }
log-erro-cancelado = Canceled.

log-chave-titulo = Encrypted logs key
log-chave-notas = Opens the encrypted logs of { -app }. If you delete this item, the logs encrypted with it will no longer open.

log-sem-chave-publica = the public key for encrypted logs is missing; unlock the vault

log-filtro-cifrado = Encrypted log
log-destrave-para-cifrar = To encrypt the logs, unlock the vault.
log-nao-criou-a-chave = Could not create the encrypted logs key: { $erro }. Logs went back to plain text.

# The Logs page.
logs-titulo = Logs
logs-descricao = The terminal session logs, in text and encrypted. The encrypted ones open here with the vault key.
logs-conexao = Connection
logs-modificado = Modified
logs-gravando = Recording
logs-vazio = No logs yet. Turn the logs on in Settings > Terminal.
logs-escolha-um = Choose a log in the list to read it.
logs-varios = { $n } logs chosen.
logs-destrave = Unlock the vault to open this log
logs-destravar = Unlock
logs-quebrar-linhas = Wrap lines
logs-copiar-tudo = Copy all
logs-buscar = Search the log
logs-buscar-dica = Search (Ctrl+F)
logs-diferenciar = Match case
logs-mostrar-tudo = Show all
logs-mostrando-o-fim = Showing the end of the log ({ $mostrado } of { $total }).
logs-sem-fim = This log ended without closing: the app may have closed suddenly. The text up to that point is below.
logs-cortado = The end of the log was incomplete ({ $bytes } bytes) and was left out.
logs-lendo = Reading { $nome }…
logs-leitura-cancelada = The reading was canceled.
logs-atualizar-dica = Read the folder again
logs-exportar = Export as text…
logs-exportar-titulo = Export the log as text
logs-exportar-pasta-titulo = Export the logs as text, to which folder?
logs-exportado-em = Log exported to { $arquivo }.
logs-exportados-em = { $n ->
    [one] 1 log exported to { $pasta }.
    *[other] { $n } logs exported to { $pasta }.
    }
logs-exportados-com-avisos = { $n ->
    [one] 1 log exported, but with warnings: { $avisos }
    *[other] { $n } logs exported, but with warnings: { $avisos }
    }
logs-nao-exportou = Couldn't export { $nome }: { $erro }
logs-abrir-pasta = Open the folder
logs-nao-abriu-a-pasta = Couldn't open { $caminho }: { $motivo }
logs-apagar-titulo = Delete the logs?
logs-apagar-texto = { $n ->
    [one] Delete 1 log?
    *[other] Delete { $n } logs?
    }
logs-apagar-aviso = Deleted logs can't be recovered by the app.
logs-apagar-gravando = { $n ->
    [one] 1 is being recorded and stays out.
    *[other] { $n } are being recorded and stay out.
    }
logs-apagados = { $n ->
    [one] 1 log deleted.
    *[other] { $n } logs deleted.
    }
logs-nao-apagou = Couldn't delete { $nome }: { $erro }
logs-so-gravando = The chosen logs are being recorded and can't be deleted right now.
logs-abrir-da-aba = Open this session's log
logs-sem-pasta-de-logs = Couldn't create the logs folder: { $erro }
logs-cifragem = Encrypt or decrypt
logs-cifrar = Encrypt the chosen logs…
logs-decifrar = Decrypt the chosen logs…
logs-cifrar-todos = Encrypt all the text ones…
logs-converter = Convert
logs-converter-titulo = Encrypt or decrypt the logs
logs-converter-cifrar-texto = { $n ->
    [one] Encrypt 1 text log ({ $tamanho })?
    *[other] Encrypt { $n } text logs ({ $tamanho })?
    }
logs-converter-decifrar-texto = { $n ->
    [one] Decrypt 1 log ({ $tamanho })? The text stays on disk, unprotected.
    *[other] Decrypt { $n } logs ({ $tamanho })? The text stays on disk, unprotected.
    }
logs-converter-apagar-originais = Delete the originals (text)
logs-converter-apagar-cifrados = Delete the encrypted ones
logs-converter-nada-cifrar = There is no text log to encrypt here.
logs-converter-nada-decifrar = None of the chosen logs is encrypted.
logs-converter-so-gravando = The logs are being recorded and can't be converted right now.
logs-convertendo = Converting the logs…
logs-cifrados-n = { $n ->
    [one] 1 log encrypted.
    *[other] { $n } logs encrypted.
    }
logs-decifrados-n = { $n ->
    [one] 1 log decrypted.
    *[other] { $n } logs decrypted.
    }
logs-convertidos-gravando = { $n ->
    [one] 1 was left out because it is being recorded.
    *[other] { $n } were left out because they are being recorded.
    }
logs-convertidos-erros = { $n ->
    [one] 1 failed: { $detalhes }
    *[other] { $n } failed: { $detalhes }
    }
logs-convertidos-cancelado = Canceled: the remaining ones were left as they were.
