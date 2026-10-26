# Puertos del servidor (M58).

portas-aviso-sem-leitura = No se pudieron leer los puertos de este servidor: no respondieron ni ss, ni SFTP, ni netstat. "Reenviar destino…" sigue funcionando.
portas-aviso-sem-docker = Sin acceso a Docker con este usuario: los contenedores no aparecen. Únete al grupo docker o usa podman.
portas-sem-ssh = La sesión no tiene una conexión SSH abierta.

# Panel
portas-painel = Puertos
portas-menu-da-aba = Puertos del servidor
portas-titulo = Puertos de { $conexao }
portas-atualizar = Actualizar
portas-filtrar = Filtrar
portas-sem-aba = Abre una pestaña SSH para ver los puertos del servidor.
portas-vazio = Ningún puerto escuchando.
portas-no-servidor = En el servidor
portas-conteineres = Contenedores
portas-mostrar-todas = Mostrar todos
portas-so-no-servidor = solo en el servidor
portas-atualizado = actualizado hace { $segundos } s · mediante { $fonte }
portas-nome-web = Web
portas-nome-web-titulo = Web — { $titulo }
portas-nome-desconhecido = Servicio desconocido
portas-no-conteiner = contenedor { $nome }
portas-ctr-publicada = puerto { $dentro }, publicado en el { $fora }
portas-ctr-interna = puerto { $dentro }, solo en la red del contenedor

# Acciones
portas-encaminhar-destino = Reenviar destino…
portas-mais = Más acciones
portas-nova = { $n ->
    [one] Puerto nuevo en { $conexao }: { $portas }
    *[other] Puertos nuevos en { $conexao }: { $portas }
    }
portas-nova-ver = Ver puertos
portas-principal-abrir = Abrir
portas-principal-encaminhar = Reenviar
portas-principal-local = localhost:{ $porta }
portas-abrir-navegador = Abrir en el navegador
portas-abrir-app = Abrir { $app }
portas-encaminhar = Reenviar
portas-encaminhar-para = Reenviar al puerto…
portas-copiar-endereco = Copiar dirección
portas-copiar-url = Copiar URL
portas-copiar-como = Copiar como { $esquema }://
portas-fixar = Anclar a esta conexión
portas-desafixar = Desanclar
portas-parar = Detener
portas-descobrir-processo = Averiguar el proceso
portas-encaminhadas = Reenviados
portas-enc-escutando = escuchando
portas-enc-subindo = abriendo…
portas-enc-conexoes = { $n ->
    [one] 1 conexión abierta
    *[other] { $n } conexiones abiertas
    }
portas-enc-certificado = el certificado no coincide ({ $motivo })
portas-encaminhada = localhost:{ $local } lleva a { $destino }:{ $porta }.
portas-copiado = Copiado: { $texto }
portas-local-ocupada = El puerto { $porta } de este equipo ya está en uso.
portas-sem-porta-livre = No hay ningún puerto libre en este equipo.
portas-demorou = El túnel tardó demasiado en abrirse.
portas-parou = El túnel se detuvo antes de abrirse.

# Diálogo "Reenviar destino…"
portas-dialogo-titulo = Reenviar destino
portas-dialogo-host = Host (visto desde el servidor)
portas-dialogo-porta = Puerto de destino
portas-dialogo-local = Puerto de este equipo (0 elige uno solo)
portas-dialogo-caminho = Ruta en el navegador
portas-dialogo-opcoes-web = Opciones web
portas-dialogo-site = Nombre del sitio
portas-dialogo-https = El servidor usa HTTPS
portas-nome-do-site = Nombre del sitio…
portas-dialogo-abrir = Abrir en el navegador después de reenviar
portas-dialogo-fixar = Anclar a esta conexión
portas-dialogo-encaminhar = Reenviar
portas-dialogo-mesmo = Reenviar de todos modos
portas-dialogo-testando = Probando el destino desde el servidor…
portas-dialogo-invalido = Indica un host y un puerto de 1 a 65535.
portas-dialogo-destino-fechado = El servidor no pudo llegar al destino: { $erro }
