# Portas do servidor (M58).

portas-aviso-sem-leitura = Não consegui ler as portas deste servidor: nem o ss, nem o SFTP, nem o netstat responderam. "Encaminhar destino…" continua funcionando.
portas-aviso-sem-docker = Sem acesso ao Docker neste usuário: os contêineres não aparecem. Entre no grupo docker ou use o podman.
portas-sem-ssh = A sessão não tem conexão SSH aberta.

# Painel
portas-painel = Portas
portas-menu-da-aba = Portas do servidor
portas-titulo = Portas de { $conexao }
portas-atualizar = Atualizar
portas-filtrar = Filtrar
portas-sem-aba = Abra uma aba SSH para ver as portas do servidor.
portas-vazio = Nenhuma porta escutando.
portas-no-servidor = No servidor
portas-conteineres = Contêineres
portas-mostrar-todas = Mostrar todas
portas-so-no-servidor = só no servidor
portas-atualizado = atualizado há { $segundos } s · por { $fonte }
portas-nome-web = Web
portas-nome-web-titulo = Web — { $titulo }
portas-nome-desconhecido = Serviço desconhecido
portas-no-conteiner = contêiner { $nome }
portas-ctr-publicada = porta { $dentro }, publicada na { $fora }
portas-ctr-interna = porta { $dentro }, só na rede do contêiner

# Ações
portas-encaminhar-destino = Encaminhar destino…
portas-mais = Mais ações
portas-nova = { $n ->
    [one] Porta nova em { $conexao }: { $portas }
    *[other] Portas novas em { $conexao }: { $portas }
    }
portas-nova-ver = Ver portas
portas-principal-abrir = Abrir
portas-principal-encaminhar = Encaminhar
portas-principal-local = localhost:{ $porta }
portas-abrir-navegador = Abrir no navegador
portas-abrir-app = Abrir { $app }
portas-encaminhar = Encaminhar
portas-encaminhar-para = Encaminhar para a porta…
portas-copiar-endereco = Copiar endereço
portas-copiar-url = Copiar URL
portas-copiar-como = Copiar como { $esquema }://
portas-fixar = Fixar nesta conexão
portas-desafixar = Desafixar
portas-parar = Parar
portas-descobrir-processo = Descobrir o processo
portas-encaminhadas = Encaminhadas
portas-enc-escutando = escutando
portas-enc-subindo = abrindo…
portas-enc-conexoes = { $n ->
    [one] 1 conexão aberta
    *[other] { $n } conexões abertas
    }
portas-enc-certificado = certificado não confere ({ $motivo })
portas-encaminhada = localhost:{ $local } leva a { $destino }:{ $porta }.
portas-copiado = Copiado: { $texto }
portas-local-ocupada = A porta { $porta } deste computador já está em uso.
portas-sem-porta-livre = Não há porta livre neste computador.
portas-demorou = O túnel demorou demais para abrir.
portas-parou = O túnel parou antes de abrir.

# Diálogo "Encaminhar destino…"
portas-dialogo-titulo = Encaminhar destino
portas-dialogo-host = Host (visto do servidor)
portas-dialogo-porta = Porta do destino
portas-dialogo-local = Porta deste computador (0 escolhe sozinha)
portas-dialogo-caminho = Caminho no navegador
portas-dialogo-opcoes-web = Opções da web
portas-dialogo-site = Nome do site
portas-dialogo-https = O servidor fala HTTPS
portas-nome-do-site = Nome do site…
portas-dialogo-abrir = Abrir no navegador depois de encaminhar
portas-dialogo-fixar = Fixar nesta conexão
portas-dialogo-encaminhar = Encaminhar
portas-dialogo-mesmo = Encaminhar mesmo assim
portas-dialogo-testando = Testando o destino a partir do servidor…
portas-dialogo-invalido = Informe um host e uma porta de 1 a 65535.
portas-dialogo-destino-fechado = O servidor não alcançou o destino: { $erro }
