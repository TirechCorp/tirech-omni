# Error messages and texts from small areas.
abrir-log-falhou = Couldn't open the log { $caminho }: { $erro }
abrir-conectado = Connected to { $nome }.
anexo-io = Couldn't read or write the attachment: { $erro }.
anexo-sem-espaco = There isn't enough disk space: the attachment needs { $precisa } and { $livre } is free.
anexo-corrompido = The attachment is corrupted or incomplete.
anexo-ausente = This attachment's file isn't on this machine yet.
anexo-cancelado = Canceled.
anexo-sem-aleatorio = no random source: { $erro }
anexo-invalido = invalid attachment
anexo-ja-existe = the attachment already exists
anexo-nao-cifrou = couldn't encrypt the attachment
anexo-blocos-demais = the attachment has too many blocks
armazenamento-caminho-absoluto = the folder path must be absolute
armazenamento-pasta-nao-vazia = the folder isn't empty and has no { -app } data
armazenamento-corrompido-copia = The { $o_que } file was corrupted; I kept a copy at { $copia }.
armazenamento-corrompido-sem-copia = The { $o_que } file was corrupted and I couldn't keep a copy: { $erro }.
armazenamento-conexoes-corrompido-copia = The connections file was corrupted; I kept a copy at { $copia }.
armazenamento-conexoes-corrompido-sem-copia = The connections file was corrupted and I couldn't keep a copy: { $erro }.
armazenamento-somente-leitura = another copy of { -app } is open and owns the settings: this one only reads, and saves nothing
armazenamento-conexoes-versao-mais-nova = The connections file was saved by a newer version of { -app }. It won't be changed here: whatever you change in the connections lasts only until you close.
armazenamento-conexoes-conflito = Another program changed the connections file at the same time; its version was kept at { $copia }.
armazenamento-so-leitura-titulo = Read-only
armazenamento-so-leitura-aviso = Another copy of { -app } is open and owns the settings. This one saves nothing: whatever you change here is lost when you close it, and the connections the other one saves show up on their own.
armazenamento-so-leitura-dona-fechou = The other copy of { -app } has closed. Restart this one to be able to save.
armazenamento-so-leitura-reiniciar = Restart
recentes-o-que = recents
agente-embutido-nao-iniciou = Couldn't start the built-in agent ({ $erro }). The app uses the system agent.
agente-endereco-invalido = The agent address must be a full path: { $endereco }
agente-pasta-curta-erro = Couldn't prepare the folder { $pasta } for the agent socket: { $erro }
agente-pasta-curta-insegura = The folder { $pasta } already exists and doesn't belong to your user alone; the agent socket won't be created in it.
agente-embutido-parado = the built-in agent isn't running
agente-um-programa-pageant = A program (through the Pageant window)
agente-um-programa = A program
agente-pid = (pid { $pid })
espacos-faltante = { $n ->
        [1] 1 connection in this space no longer exists: { $lista }
       *[other] { $n } connections in this space no longer exist: { $lista }
    }
espacos-nome-invalido = The name can't be empty or contain / \ : * ? " < > |
espacos-ja-existe = An item with that name already exists in this folder.
espacos-nao-vazia = The folder isn't empty.
espacos-nao-existe = The space or folder no longer exists.
espacos-dentro-de-si-mesma = A folder can't go inside itself.
espacos-invalido = The space file is unusable: { $motivo }
csv-aspas-sem-fechar = Unclosed quote on line { $linha }
csv-formato-desconhecido = Unknown CSV format: expected Bitwarden's or KeePassXC's
cofre-json-nao-consegui-ler-tirech = Couldn't read the { -app } JSON: { $erro }.
cofre-json-nao-consegui-ler = Couldn't read the JSON: { $erro }.
cofre-json-nao-e-cofre = That JSON isn't a { -app } vault.
cofre-json-versao-nova = That vault was exported by a newer version of { -app }.
cofre-json-formato-desconhecido = Unknown JSON format: expected { -app }'s or Bitwarden's.
bitwarden-bandeira = Brand
bitwarden-numero = Number
bitwarden-validade = Expiry
bitwarden-codigo = Code
bitwarden-tratamento = Title
bitwarden-nome = First name
bitwarden-nome-do-meio = Middle name
bitwarden-sobrenome = Last name
bitwarden-empresa = Company
bitwarden-email = Email
bitwarden-telefone = Phone
bitwarden-endereco = Address
bitwarden-endereco-2 = Address 2
bitwarden-endereco-3 = Address 3
bitwarden-cidade = City
bitwarden-estado = State
bitwarden-cep = Postal code
bitwarden-pais = Country
bitwarden-ssn = SSN
bitwarden-passaporte = Passport
bitwarden-carteira = Driver's license
bitwarden-chave-privada = Private key
bitwarden-chave-publica = Public key
bitwarden-impressao-digital = Fingerprint
bitwarden-url = URL { $n }
bitwarden-nao-consegui-ler = Couldn't read the Bitwarden JSON: { $erro }.
bitwarden-cifrado = That Bitwarden JSON is encrypted. Export again in the .json format (without the encrypted option).
bitwarden-desconhecidos = { $n ->
        [1] 1 item of an unknown type was left out.
       *[other] { $n } items of an unknown type were left out.
    }
bitwarden-vinculados = Bitwarden linked fields were ignored.
ssh-config-proxyjump = ProxyJump with { $n } hops: only the last one was kept ({ $ultimo }).
ssh-config-proxycommand = ProxyCommand isn't supported and was ignored.
ssh-config-socks-remoto = { $diretiva } { $valor }: remote SOCKS (without a destination) isn't supported and was ignored.
ssh-config-nao-entendi = { $diretiva } { $valor }: not understood, so it was ignored.
ssh-config-socket-de-origem = { $diretiva } { $valor }: a local socket as the source isn't supported and was ignored.
ssh-config-nao-entendi-porta = { $diretiva } { $valor }: the port wasn't understood, so it was ignored.
ssh-config-todas-as-interfaces = { $diretiva } { $origem } …: the address becomes "all interfaces".
ssh-config-r-para-socket = { $diretiva } { $valor }: -R to a socket isn't supported and was ignored.
ssh-config-destino-incompleto = { $diretiva } { $valor }: the destination needs a host and a port, so it was ignored.
ssh-config-problema = { $diretiva } { $valor }: { $problema } It was ignored.
ssh-config-aviso-da-conexao = { $nome }: { $aviso }
config-fora-da-faixa = { $nome } is out of range ({ $min } to { $max }); adjusted to { $preso }.
config-atalho-invalido = shortcuts.{ $id }: couldn't understand "{ $ruim }"; went back to the default.
config-json-invalido = The file isn't valid JSON: { $erro }.
config-json-nao-e-objeto = The file isn't a JSON settings object.
config-ilegivel-copia = The settings were unreadable and went back to the defaults; the copy is at { $copia }.
config-ilegivel-sem-copia = The settings were unreadable and went back to the defaults; I couldn't keep a copy: { $erro }.
config-invalido-padrao = { $nome } is invalid; went back to the default ({ $valor_padrao }).
aws-perfil-sem-chaves = The { $perfil } profile was left out: it has no access keys.
atalho-terminal-deixa-de-receber = The terminal stops receiving { $atalho }.
importar-nao-suportada-conexao = { $n ->
        [1] 1 connection ({ $complemento }) isn't supported and was ignored.
       *[other] { $n } connections ({ $complemento }) aren't supported and were ignored.
    }
importar-nao-suportada-sessao = { $n ->
        [1] 1 session ({ $complemento }) isn't supported and was ignored.
       *[other] { $n } sessions ({ $complemento }) aren't supported and were ignored.
    }
importar-com-protocolo = protocol { $nome }
importar-de-codigo = code { $n }
importar-proxy-importado = Imported proxy ({ $host }:{ $porta })
importar-ppk = There are keys in the PuTTY format (.ppk). If the agent doesn't accept them, convert them with `puttygen` or import the key into the vault.
importar-proxy-sem-suporte = Some sessions use a SOCKS4, Telnet or command proxy; only SOCKS5 and HTTP are imported.
importar-importei = { $n ->
        [1] I imported 1 connection
       *[other] I imported { $n } connections
    }
importar-trocadas = { $n ->
        [1] 1 replaced
       *[other] { $n } replaced
    }
importar-puladas = { $n ->
        [1] 1 skipped
       *[other] { $n } skipped
    }
importar-cofre-bloqueado-senhas = { $n ->
        [1] The vault is locked: 1 password was left out and will be asked for when connecting.
       *[other] The vault is locked: { $n } passwords were left out and will be asked for when connecting.
    }
cripto-winscp-formato = The WinSCP password is in a format I don't recognize.
cripto-mremoteng-sem-base64 = The mRemoteNG password isn't base64.
cripto-mremoteng-curta = The mRemoteNG password is too short.
cripto-mremoteng-rodadas = mRemoteNG asks for a number of rounds outside the acceptable range.
cripto-nonce-torto = Malformed nonce.
cripto-mremoteng-senha-mestra = The mRemoteNG master password doesn't open this file.
cripto-mremoteng-nao-cifrou = I couldn't encrypt the password for mRemoteNG.
cripto-mremoteng-curta-ou-torta = The mRemoteNG password is too short or malformed.
cripto-chave-ou-iv = Malformed key or IV.
importar-filezilla-nao-e-xml = That XML isn't a FileZilla sitemanager.xml.
importar-filezilla-cifradas = { $n ->
        [1] 1 FileZilla password is encrypted with its master key and wasn't imported.
       *[other] { $n } FileZilla passwords are encrypted with its master key and weren't imported.
    }
importar-moba-tipo-arquivo = file
importar-moba-tipo-navegador = browser
importar-moba-sem-sessoes = That file has no MobaXterm sessions ([Bookmarks…] sections).
importar-moba-senhas-no-registro = MobaXterm keeps passwords encrypted in the Windows registry; the connections come without a password.
importar-mremoteng-modo = The { $nome } mode isn't supported.
importar-mremoteng-parece-cifrado = The file looks fully encrypted (mRemoteNG's FullFileEncryption). Export again without that option.
importar-mremoteng-nao-e-xml = That XML isn't an mRemoteNG confCons.xml.
importar-mremoteng-cifrado = The file is fully encrypted (mRemoteNG's FullFileEncryption). Export again without that option.
importar-mremoteng-modo-sem-suporte = The passwords are encrypted in mRemoteNG's { $nome } mode, which isn't supported; the connections come without a password.
importar-mremoteng-senha-recusada = The master password you gave doesn't open the file.
importar-mremoteng-ilegiveis = { $n ->
        [1] 1 mRemoteNG password couldn't be decrypted.
       *[other] { $n } mRemoteNG passwords couldn't be decrypted.
    }
importar-putty-reg-sem-sessoes = That .reg has no PuTTY or KiTTY sessions.
importar-putty-pasta-sem-sessoes = That folder has no PuTTY or KiTTY sessions.
importar-winscp-sem-sessoes = That file has no WinSCP sessions ([Sessions\…] sections).
importar-winscp-workspaces = WinSCP workspaces aren't imported (only sessions).
importar-winscp-scp = { $n ->
        [1] 1 session used SCP and became SFTP (the server needs the sftp subsystem).
       *[other] { $n } sessions used SCP and became SFTP (the server needs the sftp subsystem).
    }
importar-winscp-ilegiveis = { $n ->
        [1] 1 WinSCP password couldn't be recovered (file with a WinSCP master password, or from another session).
       *[other] { $n } WinSCP passwords couldn't be recovered (file with a WinSCP master password, or from another session).
    }
importar-nativo-json = I couldn't read the { -app } JSON: { $erro }.
importar-nativo-e-cofre = That JSON is a { -app } vault, with no connections. Import it from the Vault page.
importar-nativo-nao-e-exportacao = That JSON isn't a { -app } connections export.
importar-nativo-conexoes = I couldn't read the connections in the JSON: { $erro }.
importar-nativo-versao-nova = This export is from a newer version of { -app }.
importar-winsshterm-nao-e-xml = That XML isn't a WinSSHTerm connections.xml.
importar-winsshterm-senhas = WinSSHTerm passwords are encrypted in its `key` file and weren't imported; type them in { -app }.
importar-winsshterm-proxy = The { $tipo } proxy type isn't supported (only SOCKS5 and HTTP) and wasn't imported.
importar-xml-atributo = Malformed attribute in the XML: { $erro }.
importar-xml-ler = I couldn't read the XML: { $erro }.
importar-xml-texto = Malformed text in the XML: { $erro }.
importar-xml-aninhado = The XML has too many nested elements.
importar-xml-fecha-sem-abrir = The XML closes an element that wasn't opened.
importar-xml-termina-aberto = The XML ends with an open element.
importar-xml-vazio = The file has no XML element.
exportar-resumo = { $n ->
        [1] 1 connection exported.
       *[other] { $n } connections exported.
    }
exportar-chave-do-cofre = { $n ->
        [1] 1 connection uses a vault key; with { -app } open, the agent (M44) offers it to the other program.
       *[other] { $n } connections use vault keys; with { -app } open, the agent (M44) offers them to the other program.
    }
exportar-fora-do-formato = { $n ->
        [1] 1 { $tipo } connection doesn't fit the { $programa } format and was left out.
       *[other] { $n } { $tipo } connections don't fit the { $programa } format and were left out.
    }
exportar-chaves-cofre-bloqueado = The vault is locked: its keys weren't saved and the connections that use them were left without a key file.
exportar-chaves-gravadas = The vault keys were saved, without a passphrase, in { $pasta }; keep the folder as safe as you would keep the key.
exportar-chave-sumiu = A connection uses a key that is no longer in the vault.
exportar-chave-erro = The key “{ $titulo }”: { $erro }
exportar-chave-so-publica = The key “{ $titulo }” only has the public part and wasn't saved.
exportar-chave-sem-formato = The key “{ $titulo }” doesn't exist in the { $formato } format and wasn't saved.
exportar-nao-criou-pasta = I couldn't create { $pasta }: { $erro }
exportar-filezilla-bucket = { $n ->
        [1] 1 S3 connection loses its bucket, which FileZilla asks for in the remote folder: open it when connecting.
       *[other] { $n } S3 connections lose their bucket, which FileZilla asks for in the remote folder: open it when connecting.
    }
exportar-filezilla-salto = { $n ->
        [1] 1 connection has a jump server, which FileZilla doesn't have.
       *[other] { $n } connections have a jump server, which FileZilla doesn't have.
    }
exportar-moba-salto = { $n ->
        [1] 1 connection has a jump server, which `.mxtsessions` doesn't carry.
       *[other] { $n } connections have a jump server, which `.mxtsessions` doesn't carry.
    }
exportar-moba-tunel = { $n ->
        [1] 1 connection has tunnels or a proxy, which `.mxtsessions` doesn't carry.
       *[other] { $n } connections have tunnels or a proxy, which `.mxtsessions` doesn't carry.
    }
exportar-mremoteng-falhas = { $n ->
        [1] 1 password couldn't be encrypted for mRemoteNG and was left empty.
       *[other] { $n } passwords couldn't be encrypted for mRemoteNG and were left empty.
    }
exportar-mremoteng-chave = mRemoteNG has no key field: it uses the one from the PuTTY session of the same name. Also export the PuTTY `.reg` (Export ▸ PuTTY) and import it into PuTTY.
exportar-mremoteng-rede = { $n ->
        [1] 1 connection has a proxy or jump server, which mRemoteNG doesn't store per connection.
       *[other] { $n } connections have a proxy or jump server, which mRemoteNG doesn't store per connection.
    }
exportar-nativo-cofre-bloqueado = The vault is locked: the passwords weren't included.
exportar-nativo-anexos = { $n ->
        [1] 1 note attachment isn't on this machine and was left out.
       *[other] { $n } note attachments aren't on this machine and were left out.
    }
exportar-putty-salto = { $n ->
        [1] 1 connection has a jump server, which PuTTY doesn't store in the session.
       *[other] { $n } connections have a jump server, which PuTTY doesn't store in the session.
    }
exportar-putty-tunel = { $n ->
        [1] 1 tunnel to a Unix socket doesn't fit in PuTTY and was left out.
       *[other] { $n } tunnels to a Unix socket don't fit in PuTTY and were left out.
    }
exportar-ssh-config-proxy = { $n ->
        [1] 1 connection goes out through a network proxy, which this ssh_config export doesn't carry.
       *[other] { $n } connections go out through a network proxy, which this ssh_config export doesn't carry.
    }
exportar-ssh-config-tunel = { $n ->
        [1] 1 tunnel didn't fit in OpenSSH directives and was left out.
       *[other] { $n } tunnels didn't fit in OpenSSH directives and were left out.
    }
exportar-winscp-grandes = { $n ->
        [1] 1 password goes over 255 bytes (user, server and password) and doesn't fit in WinSCP: it was left empty.
       *[other] { $n } passwords go over 255 bytes (user, server and password) and don't fit in WinSCP: they were left empty.
    }
exportar-winscp-saltos = { $n ->
        [1] 1 connection has more than one jump; WinSCP carries only one (the closest to the server).
       *[other] { $n } connections have more than one jump; WinSCP carries only one (the closest to the server).
    }
exportar-winsshterm-salto = { $n ->
        [1] 1 connection has a jump server, which WinSSHTerm doesn't store per connection.
       *[other] { $n } connections have a jump server, which WinSSHTerm doesn't store per connection.
    }
exportar-winsshterm-tunel = { $n ->
        [1] 1 connection has tunnels, which WinSSHTerm doesn't have.
       *[other] { $n } connections have tunnels, which WinSSHTerm doesn't have.
    }
queda-titulo = { -app } stopped
queda-mensagem = { -app } ran into an error and had to close. The details are in:

    { $arquivo }

    If it closed right after opening, open it again: next time it draws the window in a simpler way that works with more graphics cards.
portatil-ja-aberto-titulo = { -app } is already open
portatil-ja-aberto = Another { -app } window already uses the folder { $pasta }. Use that window; two at the same time would overwrite each other's changes.
