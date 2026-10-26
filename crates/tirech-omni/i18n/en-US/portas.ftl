# Server ports (M58).

portas-aviso-sem-leitura = I could not read this server's ports: neither ss, SFTP nor netstat answered. "Forward destination…" still works.
portas-aviso-sem-docker = No access to Docker for this user: the containers are not listed. Join the docker group or use podman.
portas-sem-ssh = The session has no open SSH connection.

# Panel
portas-painel = Ports
portas-menu-da-aba = Server ports
portas-titulo = Ports of { $conexao }
portas-atualizar = Refresh
portas-filtrar = Filter
portas-sem-aba = Open an SSH tab to see the server's ports.
portas-vazio = No ports listening.
portas-no-servidor = On the server
portas-conteineres = Containers
portas-mostrar-todas = Show all
portas-so-no-servidor = server only
portas-atualizado = updated { $segundos } s ago · by { $fonte }
portas-nome-web = Web
portas-nome-web-titulo = Web — { $titulo }
portas-nome-desconhecido = Unknown service
portas-no-conteiner = container { $nome }
portas-ctr-publicada = port { $dentro }, published on { $fora }
portas-ctr-interna = port { $dentro }, container network only

# Actions
portas-encaminhar-destino = Forward destination…
portas-mais = More actions
portas-nova = { $n ->
    [one] New port on { $conexao }: { $portas }
    *[other] New ports on { $conexao }: { $portas }
    }
portas-nova-ver = View ports
portas-principal-abrir = Open
portas-principal-encaminhar = Forward
portas-principal-local = localhost:{ $porta }
portas-abrir-navegador = Open in browser
portas-abrir-app = Open { $app }
portas-encaminhar = Forward
portas-encaminhar-para = Forward to port…
portas-copiar-endereco = Copy address
portas-copiar-url = Copy URL
portas-copiar-como = Copy as { $esquema }://
portas-fixar = Pin to this connection
portas-desafixar = Unpin
portas-parar = Stop
portas-descobrir-processo = Find the process
portas-encaminhadas = Forwarded
portas-enc-escutando = listening
portas-enc-subindo = opening…
portas-enc-conexoes = { $n ->
    [one] 1 open connection
    *[other] { $n } open connections
    }
portas-enc-certificado = certificate does not match ({ $motivo })
portas-encaminhada = localhost:{ $local } reaches { $destino }:{ $porta }.
portas-copiado = Copied: { $texto }
portas-local-ocupada = Port { $porta } on this computer is already in use.
portas-sem-porta-livre = There is no free port on this computer.
portas-demorou = The tunnel took too long to open.
portas-parou = The tunnel stopped before opening.

# "Forward destination…" dialog
portas-dialogo-titulo = Forward destination
portas-dialogo-host = Host (as seen from the server)
portas-dialogo-porta = Destination port
portas-dialogo-local = Port on this computer (0 picks one)
portas-dialogo-caminho = Path in the browser
portas-dialogo-opcoes-web = Web options
portas-dialogo-site = Site name
portas-dialogo-https = The server speaks HTTPS
portas-nome-do-site = Site name…
portas-dialogo-abrir = Open in the browser after forwarding
portas-dialogo-fixar = Pin to this connection
portas-dialogo-encaminhar = Forward
portas-dialogo-mesmo = Forward anyway
portas-dialogo-testando = Testing the destination from the server…
portas-dialogo-invalido = Enter a host and a port from 1 to 65535.
portas-dialogo-destino-fechado = The server could not reach the destination: { $erro }
