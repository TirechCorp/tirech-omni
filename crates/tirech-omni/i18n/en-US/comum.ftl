# Texts used in more than one place.

# The app name: every text that mentions it uses this term, so renaming is one line.
-app = Tirech Omni

comum-a-porta-do-servidor-de-salto = The jump server port.
comum-abrir = Open
comum-adicionar = Add
comum-agora-nao = Not now
comum-anexar-arquivo = Attach file
comum-apagar = Delete
comum-atualizar = Refresh
comum-cancelar = Cancel
comum-conectar = Connect
comum-continuar = Resume
comum-copiar = Copy
comum-copiar-a-publica = Copy the public key
comum-criar = Create
comum-desbloquear = Unlock
comum-duplicar = Duplicate
comum-editar = Edit
comum-enviar-para = Send to
comum-fechar = Close
comum-filtrar = Filter
comum-importar = Import
comum-mais = More
comum-mover = Move
comum-no-agente = In the agent
comum-nova-conexao = New connection
comum-nova-pasta = New folder
comum-novo = New
comum-o-cofre-esta-bloqueado = The vault is locked
comum-o-item-do-cofre-com-o = The vault item with the jump server user name and password.
comum-parar = Stop
comum-pasta = Folder
comum-remotos-do-rclone = rclone remotes
comum-remover = Remove
comum-salvar = Save
comum-salvar-2 = Save…
comum-substituir = Replace
comum-tentar-de-novo = Try again
comum-texto = Text
comum-voltar = Back
comum-arquivo = File
comum-comparar-pastas = Compare folders
comum-credencial = Credential
comum-credencial-agente = SSH agent
comum-credencial-agente-ou-senha = SSH agent, then password
comum-credencial-herdar = Inherit
comum-credencial-herdar-de = Inherit ({ $nome })
comum-credencial-descricao-pasta = How the SSH and SFTP connections in this folder that did not choose otherwise and have no vault item sign in. SSH agent: they use the agent (and the key, if any) without asking. SSH agent, then password: they try the agent and, if the server refuses, ask for the password.
comum-endereco = Address
comum-estado = Status
comum-grupo = Group
comum-host = Host
comum-mostrar = Show
comum-nao-perguntar-de-novo = Don't ask again
comum-nome = Name
comum-notas = Notes
comum-pedir-confirmacao-a-cada-uso = Ask for confirmation on every use
comum-porta = Port
comum-rede = Network
comum-remoto = Remote
comum-salvar-a-passphrase-no-cofre = Save the passphrase in the vault
comum-senha = Password
comum-tamanho = Size
comum-tipo = Type
comum-usuario = User
comum-renomear = Rename
comum-cofre = Vault
comum-titulo = Title
comum-ftp-seguranca-explicita = FTP over explicit TLS
comum-ftp-seguranca-se-disponivel = TLS, if available
comum-ftp-seguranca-implicita = Implicit FTPS
comum-ftp-seguranca-nenhuma = Plain FTP (no TLS)
comum-ftp-modo-passivo = Passive
comum-ftp-modo-ativo = Active
comum-ftp-tls-auto = Automatic
comum-ftp-tls-12 = TLS 1.2 only
comum-ftp-tls-13 = TLS 1.3 only
comum-ftp-codificacao-auto = Automatic
comum-s3-estilo-caminho = Path
comum-s3-estilo-virtual = Virtual host
comum-s3-certificado-verificar = Verify (system authorities)
comum-s3-certificado-ca-propria = Verify with a custom CA
comum-s3-certificado-nao-verificar = Don't verify
comum-tipo-shell-local = Local shell
comum-tipo-link = Link
comum-x11-padrao = Default
comum-x11-desligado = Off
comum-x11-nao-confiavel = Untrusted
comum-x11-confiavel = Trusted
comum-tunel-remoto-nome = Remote (-R)
comum-tunel-dinamico-nome = Dynamic (SOCKS5, -D)
comum-tunel-socket-nome = Server Unix socket
comum-tunel-remoto-curto = Remote
comum-ativar-padrao = Default
comum-ativar-ligado = On
comum-ativar-desligado = Off
comum-tuneis-resumo = { $n ->
    [0] None
    [1] 1 tunnel
    *[other] { $n } tunnels
    }
comum-nenhum = None
comum-nenhuma = None
comum-nao-consegui-gravar = Couldn't write { $arquivo }: { $erro }
comum-conexoes = Connections
comum-n-itens = { $n ->
    [1] 1 item
    *[other] { $n } items
    }
comum-n-pastas = { $n ->
    [1] 1 folder
    *[other] { $n } folders
    }
comum-n-subpastas = { $n ->
    [1] 1 subfolder
    *[other] { $n } subfolders
    }
comum-n-anexos = { $n ->
    [1] 1 attachment
    *[other] { $n } attachments
    }
comum-n-palavras = { $n ->
    [1] 1 word
    *[other] { $n } words
    }
comum-juntar-dois = { $ate } and { $ultimo }
comum-juntar-fim = { $ate }, and { $ultimo }
comum-apagar-titulo = Delete { $nome }?
comum-sem-titulo = (untitled)
comum-nao-consegui-gravar-o-cofre = Couldn't write the password vault: { $erro }
comum-nao-consegui-ler = Couldn't read { $arquivo }: { $erro }
comum-n-chaves-ssh = { $n ->
    [1] 1 SSH key
    *[other] { $n } SSH keys
    }
comum-nao-consegui-salvar = Couldn't save { $arquivo }: { $erro }
comum-sim = Yes
comum-nao = No
comum-vazio-entre-parenteses = (empty)
comum-s3-provedor-outro = Ceph RGW / other
comum-titulo-copia = { $titulo } (copy)
sistema-nao-esta-em-utf8 = { $arquivo } isn't UTF-8
sistema-registro-so-windows = the registry only exists on Windows
s3-modelo-amazon = https://s3.{ "{" }region{ "}" }.amazonaws.com
s3-modelo-r2 = https://{ "{" }account{ "}" }.r2.cloudflarestorage.com
s3-modelo-wasabi = https://s3.{ "{" }region{ "}" }.wasabisys.com
s3-modelo-b2 = https://s3.{ "{" }region{ "}" }.backblazeb2.com
s3-modelo-digitalocean = https://{ "{" }region{ "}" }.digitaloceanspaces.com
comum-parte-de-total = { $parte } of { $total }
comum-tunel-local-nome = Local (-L)
comum-colar-padrao = Default
comum-shell-remoto-automatico = Automatic
comum-shell-remoto-posix = sh, bash, zsh (POSIX)
comum-shell-remoto-powershell = PowerShell
comum-shell-remoto-cmd = cmd
