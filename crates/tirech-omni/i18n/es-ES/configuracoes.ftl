# Configuración: el catálogo de la página (M35). Cada tarjeta tiene la clave `config-<ruta en el JSON con . → ->`,
# que es también el id de la tarjeta en pantalla: el título, la `.descricao` (descripción) y las
# `.palavras` (palabras de búsqueda). Las secciones vienen de `config-secao-*`.

## Secciones

config-secao-geral = General
    .resumo = Tema, idioma, página de inicio, sesiones y recientes

config-secao-terminal = Terminal
    .resumo = Fuente, colores, cursor, copiar y pegar, duplicar pestaña, shell local y logs

config-secao-conexoes = Conexiones
    .resumo = Valores predeterminados de las conexiones nuevas, known_hosts, keepalive y reconectar

config-secao-scripts = Scripts
    .resumo = Intérprete, ejecución en paralelo y carpeta de los scripts

config-secao-cofre = Bóveda de contraseñas
    .resumo = Protección, bloqueo, dónde está la bóveda y generador de contraseñas

config-secao-notas = Notas
    .resumo = Modo de edición, fuente y ajuste de línea

config-secao-agente = Claves SSH y agente
    .resumo = El agente, las claves nuevas y el uso por el sistema

config-secao-arquivos = Archivos
    .resumo = Transferencias, conflictos, editor, comparar y sincronizar

config-secao-ftp = FTP
    .resumo = Seguridad, modo, TLS y codificación

config-secao-s3 = S3
    .resumo = Región, estilo de las direcciones, partes, clase y vínculos

config-secao-rclone = rclone
    .resumo = El programa, el archivo de configuración y la contraseña

config-secao-redes = Redes
    .resumo = Perfiles de red, espera y cuándo desconectar

config-secao-x11 = X11 y túneles
    .resumo = X11 predeterminado, display, virt-manager y túneles

config-secao-portas = Puertos del servidor
    .resumo = Actualización, sondeo, avisos y vínculos por el túnel

config-secao-janelas = Ventanas y espacios
    .resumo = Carpeta de los espacios, abrir al iniciar y desacoplar pestañas

config-secao-ia = Asistentes de IA
    .resumo = Perfiles, lo que sale del equipo y el contexto del terminal

config-secao-ia-servidor = Asistentes en el servidor
    .resumo = Claude Code, Codex y OpenCode en el servidor

config-secao-ia-local = Asistentes en este equipo
    .resumo = Claude Code, Codex y OpenCode en este equipo

config-secao-mcp = Control por asistentes (MCP)
    .resumo = Activar, permisos, espacio de trabajo y auditoría

config-secao-fim = Fin de la página

## Tarjetas

# General

config-geral-tema = Tema
    .descricao = Claro, oscuro o el del sistema.
    .palavras = apariencia colores oscuro claro dark

config-geral-idioma = Idioma
    .descricao = El idioma de la interfaz. Se aplica después de reiniciar { -app }.
    .palavras = idioma lengua language ingles english portugues espanol traduccion

config-geral-pagina_inicial = Página al iniciar
    .descricao = Automática: los Terminales si había pestañas abiertas la última vez; si no, el Inicio.
    .palavras = abrir comienzo inicio

config-geral-restaurar_sessoes = Volver a abrir las pestañas y ventanas al iniciar
    .descricao = Desactivado, la aplicación empieza vacía (lo que estaba abierto sigue guardado).
    .palavras = sesiones restaurar pestanas

config-geral-confirmar_fechar = Preguntar antes de cerrar con sesiones abiertas
    .descricao = Desactivado, cerrar la ventana cierra directamente.
    .palavras = salir confirmar cerrar

config-geral-ao_fechar = Al cerrar la ventana
    .descricao = Preguntar, dejar la aplicación en la bandeja (el agente sigue) o cerrar. Requiere bandeja en el sistema.
    .palavras = salir bandeja ocultar cerrar minimizar

config-geral-iniciar_com_sistema = Iniciar con el sistema
    .descricao = Abre la aplicación al iniciar sesión en el sistema, para que el agente de claves ya esté en marcha.
    .palavras = autostart inicio arranque login bandeja boot

config-geral-iniciar_na_bandeja = Al iniciar con el sistema, quedarse solo en la bandeja
    .descricao = Desactivado, la ventana se abre. Activado, solo aparece el icono de la bandeja. Requiere bandeja en el sistema.
    .palavras = autostart inicio arranque oculto minimizado bandeja ventana

config-geral-atalho_no_menu = Acceso directo en el menú de aplicaciones
    .descricao = Muestra { -app } en el menú y en el dock del sistema, con su propio icono.
    .palavras = desktop dock lanzador icono launcher menu aplicaciones

config-geral-recentes_max = Recientes guardados
    .descricao = Cuántas conexiones guarda la lista de recientes, sin contar las ancladas.
    .palavras = historial ultimas

config-acao-limpar_recentes = Borrar recientes
    .descricao = Borra la lista de recientes y mantiene las ancladas.
    .palavras = borrar historial

config-acao-instalacao = Instalar en el equipo
    .descricao = Copia la aplicación a una carpeta fija y crea los accesos directos, el inicio con el sistema y la entrada en Aplicaciones instaladas.
    .palavras = instalar instalacion desinstalar copiar acceso directo menu inicio escritorio barra de tareas dock aplicaciones

config-acao-pasta_dados = Carpeta de datos
    .descricao = Dónde se guardan las conexiones, la bóveda y los espacios.
    .palavras = almacenamiento usb pendrive sincronizar cambiar dropbox

config-acao-sobre = Acerca de { -app }
    .descricao = Versión, licencia y qué es la aplicación.
    .palavras = version licencia

# Terminal

config-terminal-tamanho_fonte = Tamaño de fuente
    .descricao = El tamaño de la letra en todos los terminales abiertos.
    .palavras = fuente letra zoom texto

config-terminal-esquema = Combinación de colores
    .descricao = Los colores del terminal: Campbell y otras combinaciones.
    .palavras = colores paleta esquema campbell tango vintage one half tema

config-terminal-cursor = Forma del cursor
    .descricao = Barra, bloque o subrayado.
    .palavras = cursor bloque barra subrayado

config-terminal-cursor_piscando = Cursor parpadeante
    .descricao = Desactivado, el cursor queda fijo aunque el programa pida que parpadee.
    .palavras = cursor parpadear parpadeo blink

config-terminal-historico = Líneas de historial
    .descricao = Cuántas líneas guarda el terminal para desplazarse. Se aplica a las sesiones nuevas.
    .palavras = scrollback desplazamiento buffer

config-terminal-copiar_ao_selecionar = Copiar al seleccionar
    .descricao = Al soltar el mouse, el texto seleccionado va al portapapeles.
    .palavras = copiar seleccion clipboard portapapeles

config-terminal-avisar_colar = Avisar al pegar varias líneas
    .descricao = Pregunta antes de pegar un texto con más de una línea.
    .palavras = pegar paste confirmar

config-terminal-colar_arquivos = Al pegar una imagen o un archivo en una sesión SSH
    .descricao = Qué hacer cuando el portapapeles tiene una imagen o archivos, o cuando arrastras archivos al terminal.
    .palavras = pegar paste imagen archivo captura print claude codex opencode sftp tmp

config-terminal-pasta_colar = Carpeta en el servidor para lo que se pega
    .descricao = Dónde quedan en el servidor los archivos pegados. ~ es la carpeta personal. Si no se puede usar, la aplicación usa ~/.cache/tirech-colar.
    .palavras = pegar carpeta tmp temporal

config-terminal-ctrl_v_arquivos = Ctrl+V pega imágenes y archivos
    .descricao = Sin texto en el portapapeles, Ctrl+V envía la imagen o los archivos y pega la ruta. Con texto, sigue yendo al programa.
    .palavras = ctrl+v pegar imagen

config-terminal-limpar_colados_dias = Borrar del servidor lo que se pegó hace más de
    .descricao = En días. 0 nunca borra. La limpieza ocurre en el primer envío de cada sesión.
    .palavras = limpiar borrar temporal dias

config-terminal-colar_tar_min_arquivos = Enviar la carpeta en un solo paquete a partir de
    .descricao = En archivos. Una carpeta con al menos esa cantidad de archivos (o con 10 o más, todos pequeños) va en un solo paquete tar, mucho más rápido. El paquete lo crea la propia aplicación; el servidor solo necesita tener tar para abrirlo, y sin él la carpeta va por SFTP. 0 envía siempre por SFTP.
    .palavras = carpeta tar paquete muchos archivos rapido

config-terminal-fechar_painel_vazio = Cerrar el panel cuando salga la última pestaña
    .descricao = El panel que se queda sin pestañas desaparece solo, si hay otro panel.
    .palavras = panel cuadrante vacio cerrar

config-terminal-duplicar_na_pasta = Duplicar en la carpeta actual
    .descricao = La pestaña duplicada se abre en la carpeta en la que está la original. En SSH, la aplicación escribe un cd cuando aparece el prompt.
    .palavras = duplicar pestana carpeta directorio cd clonar copiar

config-terminal-links_com_ctrl = Abrir vínculos con Ctrl+clic
    .descricao = Ctrl+clic abre la dirección en este equipo; con Ctrl presionado, los vínculos aparecen subrayados. Desactivado, el menú del clic derecho sigue teniendo Copiar vínculo.
    .palavras = link vinculo enlace url direccion hipervinculo navegador ctrl clic

config-terminal-botao_direito = Botón derecho
    .descricao = Copia (con selección) y pega (sin ella), como en PuTTY, y Ctrl+clic derecho abre el menú; o siempre abre el menú.
    .palavras = mouse raton menu pegar

config-terminal-campainha = Campana
    .descricao = Qué ocurre cuando el programa hace sonar la campana.
    .palavras = bell campana sonido parpadear aviso

config-terminal-shell = Shell local
    .descricao = El programa que ejecuta "Abrir shell local".
    .palavras = bash zsh powershell cmd

config-terminal-shell-programa = Programa
    .descricao = Vacío usa $SHELL (o cmd.exe).
    .palavras = shell bash zsh ejecutable

config-terminal-shell-argumentos = Argumentos
    .descricao = Separados por espacios.
    .palavras = shell parametros opciones

config-terminal-shell-pasta = Carpeta inicial
    .descricao = Dónde empieza el shell local; acepta ~. Una conexión local puede tener la suya.
    .palavras = shell directorio cwd cd inicio

config-terminal-logs = Logs de las sesiones
    .descricao = Graba en un archivo lo que aparece en el terminal.
    .palavras = grabar registro log

config-terminal-logs-pasta = Carpeta de los logs
    .descricao = Vacía usa la carpeta logs dentro de la carpeta de datos.
    .palavras = log grabar directorio

config-terminal-logs-todas_as_sessoes = Grabar todas las sesiones
    .descricao = Cada sesión nueva graba un archivo con el nombre de la conexión y la hora.
    .palavras = log registro automatico

config-terminal-logs-cifrar = Cifrar los logs
    .descricao = Los logs nuevos se guardan cifrados (.log.cifrado) y solo se abren con la bóveda desbloqueada. La clave de los logs se guarda en la bóveda.
    .palavras = log cifrar encriptar proteger secreto

config-terminal-logs-selar_secs = Sellar el log cifrado cada (segundos)
    .descricao = Cada cuánto tiempo el texto reciente va al disco. Si la aplicación se cierra inesperadamente, se pierde como máximo eso.
    .palavras = log cifrado disco grabar intervalo

config-terminal-logs-visualizar_mib = Tamaño del log en la página Logs (MiB)
    .descricao = Cuánto del final del log muestra la página de una vez. 0 muestra todo, y un log grande tarda en abrirse.
    .palavras = log visualizar limite memoria leer

# Conexiones

config-conexoes-usuario_padrao = Usuario predeterminado
    .descricao = Rellena las conexiones nuevas. Vacío usa el usuario del sistema.
    .palavras = login user usuario nombre

config-conexoes-porta_padrao = Puerto SSH predeterminado
    .descricao = El puerto de las conexiones nuevas.
    .palavras = 22 ssh puerto

config-conexoes-term_padrao = TERM predeterminado
    .descricao = El tipo de terminal que anuncian las conexiones nuevas: xterm-256color, xterm, vt100, linux.
    .palavras = xterm vt100 terminfo

config-conexoes-keepalive_padrao_secs = Keepalive predeterminado
    .descricao = Segundos entre las señales que mantienen viva la conexión (0 lo desactiva). Conexiones nuevas.
    .palavras = mantener viva timeout caida

config-conexoes-known_hosts = Archivo known_hosts
    .descricao = Dónde se guardan las claves de los servidores conocidos. Vacío usa ~/.ssh/known_hosts.
    .palavras = hosts servidores fingerprint huella

config-conexoes-chave_nova = Servidor desconocido
    .descricao = Qué hacer con la clave de un servidor que known_hosts todavía no tiene. Una clave cambiada se sigue rechazando.
    .palavras = host key confiar aceptar preguntar

config-conexoes-reconectar = Al caerse la conexión
    .descricao = Qué hace la aplicación cuando la conexión se cae (no cuando el shell termina con exit).
    .palavras = reconectar caida caer volver conectar

config-conexoes-reconectar-modo = Qué hacer
    .descricao = No hacer nada, ofrecer reconectar o reconectar automáticamente.
    .palavras = reconectar automatico

config-conexoes-reconectar-tentativas = Intentos
    .descricao = Cuántas veces reconectar automáticamente (de 1 a 20).
    .palavras = reconectar veces reintentos

config-conexoes-reconectar-espera_secs = Espera entre los intentos
    .descricao = Segundos antes de cada intento (de 1 a 300).
    .palavras = reconectar intervalo tiempo

config-conexoes-abrir_em = Abrir conexión en
    .descricao = Dónde abre el doble clic en el árbol, en el Inicio y en la búsqueda.
    .palavras = ventana panel pestana

config-acao-importar_ssh_config = Importar de ~/.ssh/config
    .descricao = Trae los hosts del archivo de configuración de OpenSSH.
    .palavras = openssh hosts importar

config-acao-importar_aws = Importar de ~/.aws
    .descricao = Trae los perfiles de AWS como conexiones S3.
    .palavras = amazon credenciales importar s3

config-conexoes-confirmar_fechar_todas = Preguntar antes de cerrar todas las pestañas
    .descricao = Al usar Herramientas > Cerrar todas las pestañas con sesiones abiertas, pide confirmación.
    .palavras = cerrar pestanas todas confirmar preguntar

config-conexoes-tmux_ao_fechar = Al cerrar una pestaña con tmux o screen
    .descricao = Qué hacer con la sesión de tmux o de screen en el servidor cuando cierras una pestaña que se ejecuta en ella. Cerrar la aplicación nunca la termina: las pestañas vuelven a las mismas sesiones.
    .palavras = tmux screen sesion persistente cerrar pestana terminar mantener

config-conexoes-comandos_quieto_ms = Pantalla inactiva después de (ms)
    .descricao = Cuánto tiempo sin salida cuenta como pantalla inactiva, en los comandos al conectar (de 50 a 60000).
    .palavras = comandos conectar sudo pasos inactivo esperar

config-conexoes-comandos_max_secs = Tiempo máximo de cada paso (s)
    .descricao = El tiempo máximo que esperan los pasos nuevos de los comandos al conectar (de 1 a 86400).
    .palavras = comandos conectar sudo pasos tiempo limite

config-conexoes-importar_senhas = Importar las contraseñas de forma predeterminada
    .descricao = Al importar de otro programa, deja marcada la casilla que guarda las contraseñas en la bóveda.
    .palavras = importar contrasena boveda winsshterm filezilla winscp

# Scripts

config-scripts-interpretador = Intérprete
    .descricao = El programa que lee el archivo del script por la entrada (sh -s, bash -s). Una línea escrita se ejecuta tal cual.
    .palavras = script shell bash sh comando

config-scripts-paralelo = Servidores a la vez
    .descricao = Cuántos servidores ejecutan el script juntos (de 1 a 64); los demás esperan su turno.
    .palavras = script paralelo simultaneo threads veces

config-scripts-pasta = Carpeta de los scripts
    .descricao = Vacía usa la carpeta scripts dentro de la carpeta de datos.
    .palavras = script directorio archivos sh

# Bóveda de contraseñas

config-acao-protecao_cofre = Protección de la bóveda
    .descricao = Cómo se cifra la bóveda: clave guardada en el archivo o contraseña maestra.
    .palavras = contrasena maestra cifrar cifrado seguridad

config-cofre-limpar_area_transferencia_secs = Borrar el portapapeles
    .descricao = Después de copiar una contraseña, la borra del portapapeles si todavía está allí.
    .palavras = clipboard portapapeles copiar contrasena borrar

config-cofre-bloquear_apos_min = Bloquear por inactividad
    .descricao = Bloquea la bóveda después de un tiempo sin usar una contraseña. Requiere contraseña maestra.
    .palavras = bloquear inactividad timeout contrasena maestra

config-acao-cofre_local = Ubicación de la bóveda
    .descricao = La carpeta de vault.json y vault.key. Vacía usa la carpeta de datos; en una carpeta sincronizada (Syncthing, Dropbox, OneDrive), la bóveda va también a los otros equipos.
    .palavras = carpeta sincronizar syncthing dropbox onedrive vault directorio

config-acao-cofre_levar = Llevar la bóveda a otro equipo…
    .descricao = Guarda un archivo .tirech-vault con la bóveda, para abrirlo en otro equipo.
    .palavras = exportar copiar tirech-vault backup copia mover

config-acao-cofre_abrir_outro = Abrir la bóveda de otro equipo…
    .descricao = Combina un .tirech-vault (o un vault.json) con la bóveda de aquí, o lo usa en su lugar.
    .palavras = importar juntar tirech-vault sincronizar combinar fusionar

config-cofre-gerador = Generador de contraseñas
    .descricao = Lo que usa el botón Generar de la bóveda.
    .palavras = generar aleatoria contrasena

config-cofre-gerador-tamanho = Longitud
    .descricao = De 8 a 128 caracteres.
    .palavras = generador longitud

config-cofre-gerador-maiusculas = Letras mayúsculas
    .descricao = Entran de la A a la Z.
    .palavras = generador

config-cofre-gerador-minusculas = Letras minúsculas
    .descricao = Entran de la a a la z.
    .palavras = generador

config-cofre-gerador-digitos = Dígitos
    .descricao = Entran del 0 al 9.
    .palavras = generador numeros

config-cofre-gerador-simbolos = Símbolos
    .descricao = Entran !, #, $ y similares.
    .palavras = generador especiales

config-cofre-gerador-sem_ambiguos = Sin caracteres ambiguos
    .descricao = Quita 0, O, 1, l e I, que se confunden.
    .palavras = generador parecidos

config-acao-csv_cofre = Importar y exportar CSV
    .descricao = Abre la página de la bóveda, donde están importar y exportar.
    .palavras = hoja de calculo importar exportar backup copia

# Notas

config-notas-modo = Modo al abrir
    .descricao = Cómo abre una nota la página Notas: solo el texto, el texto con la vista previa al lado o solo la vista previa.
    .palavras = markdown editar dividir leer vista previa visualizar

config-notas-fonte_mono = Editor con fuente monoespaciada
    .descricao = Escribe el Markdown con una fuente en la que todas las letras tienen el mismo ancho.
    .palavras = markdown fuente monoespaciada codigo

config-notas-quebrar_linhas = Ajustar las líneas largas
    .descricao = Continúa la línea en la de abajo en lugar de crear una barra de desplazamiento horizontal.
    .palavras = markdown ajuste desplazamiento ancho

# Claves SSH y agente

config-agente-ligado = Agente integrado
    .descricao = El agente de la aplicación guarda las claves abiertas para las conexiones y para otros programas.
    .palavras = ssh-agent claves socket

config-agente-socket = Dirección del socket
    .descricao = Vacío usa la dirección predeterminada del sistema (la canalización openssh-ssh-agent en Windows). Se aplica al reiniciar el agente.
    .palavras = ssh_auth_sock ruta

config-agente-exportar_no_shell = Usar en el shell local
    .descricao = Pone SSH_AUTH_SOCK en los shells locales que se abran después.
    .palavras = ssh_auth_sock entorno

config-agente-carregar_ao_iniciar = Cargar las claves al iniciar
    .descricao = Abre las claves de la lista del agente al abrir la aplicación.
    .palavras = claves automatico

config-agente-vida_padrao_secs = Tiempo de vida predeterminado
    .descricao = Cuánto tiempo se queda una clave nueva en el agente. Se aplica a las que se agreguen después.
    .palavras = validez caducar expirar lifetime

config-agente-confirmar_padrao = Confirmar cada uso
    .descricao = Pide confirmación en cada uso de las claves que se agreguen después.
    .palavras = confirm permiso

config-agente-encaminhar_padrao = Reenviar el agente de forma predeterminada
    .descricao = Las conexiones SSH usan el agente de la aplicación en el servidor (agent forwarding).
    .palavras = forward reenvio -A

config-agente-pasta_chaves = Carpeta de las claves
    .descricao = Dónde graba Generar clave y lo que lista la página Claves. Vacía usa ~/.ssh.
    .palavras = ssh directorio id_ed25519

config-agente-chaves_novas = Tipo y protección de las claves nuevas
    .descricao = Lo que el cuadro de diálogo Generar clave trae elegido.
    .palavras = generar ed25519 rsa ecdsa

config-agente-tipo_padrao = Tipo
    .descricao = Ed25519, ECDSA o RSA.
    .palavras = ed25519 rsa ecdsa algoritmo

config-agente-rodadas_padrao = Protección de la frase de contraseña
    .descricao = Cuántas rondas de bcrypt-pbkdf: más es más lento de romper y de abrir.
    .palavras = bcrypt kdf passphrase frase

config-agente-sistema_ssh_config = Usar en el ssh del sistema (~/.ssh/config)
    .descricao = Escribe un bloque al principio de ~/.ssh/config: el terminal, git, codex y VS Code pasan a usar las claves de la aplicación mientras esté abierta.
    .palavras = ssh identityagent git codex claude sistema

config-agente-sistema_shells = También en los shells (bash, zsh, fish)
    .descricao = Pone SSH_AUTH_SOCK en .bashrc, en .zshrc y en fish, para los programas que no leen ~/.ssh/config. Se aplica a los shells que se abran después.
    .palavras = ssh_auth_sock bashrc zshrc fish entorno

config-agente-sistema_sessao = También en la sesión gráfica
    .descricao = Informa a la sesión del usuario (systemctl --user o launchctl) de SSH_AUTH_SOCK. Se aplica a los programas que se abran después.
    .palavras = ssh_auth_sock systemd launchctl entorno

config-agente-sistema_git = Hacer que Git use el OpenSSH de Windows
    .descricao = El ssh que viene con Git no habla con el agente de Windows. Esto apunta core.sshCommand de Git al ssh.exe de Windows.
    .palavras = git sshcommand openssh windows

config-agente-tirar_ao_bloquear = Al bloquear la bóveda, quitar sus claves del agente
    .descricao = Las claves de archivo y las que otro programa puso en el agente se quedan.
    .palavras = bloquear boveda claves

config-agente-pageant = Responder como Pageant (PuTTY, WinSCP, FileZilla)
    .descricao = Esos programas buscan Pageant y pasan a usar las claves de la aplicación. Si el Pageant de PuTTY ya está abierto, sigue respondiendo él.
    .palavras = putty winscp filezilla kitty windows

config-acao-gerenciar_chaves = Administrar claves y agente
    .descricao = Abre la página del agente.
    .palavras = claves agente

# Archivos

config-arquivos-paralelas = Transferencias a la vez
    .descricao = De 1 a 8. Las que están en curso terminan.
    .palavras = cola simultaneas paralelo

config-arquivos-conflito = Cuando el archivo ya existe
    .descricao = Qué hacer en las próximas transferencias.
    .palavras = sobrescribir reanudar omitir saltar renombrar conflicto

config-arquivos-mostrar_ocultos = Mostrar archivos ocultos
    .descricao = El estado inicial del botón en los paneles que se abran después.
    .palavras = dotfiles ocultos

config-arquivos-editor = Editor externo
    .descricao = El programa de Editar. Vacío usa el del sistema.
    .palavras = editar vim code notepad

config-arquivos-enviar_editado = Enviar automáticamente el archivo editado
    .descricao = Preguntar o enviarlo de vuelta en cuanto el archivo cambie.
    .palavras = editar guardar upload subir

config-arquivos-preservar_data = Mantener la fecha de modificación
    .descricao = En las próximas transferencias.
    .palavras = mtime timestamp fecha

config-arquivos-preservar_permissoes = Mantener los permisos
    .descricao = En las próximas transferencias.
    .palavras = chmod modo permisos

config-arquivos-pasta_local = Carpeta local inicial
    .descricao = El panel local de las pestañas nuevas. Vacía usa la carpeta personal.
    .palavras = home directorio

config-arquivos-confirmar_apagar = Preguntar antes de eliminar
    .descricao = Desactivado, Supr elimina sin el cuadro de diálogo.
    .palavras = eliminar borrar quitar confirmar

config-arquivos-keepalive_secs = Keepalive del explorador
    .descricao = Segundos entre las señales de la conexión del explorador de archivos (0 lo desactiva). Conexiones que se abran después.
    .palavras = sftp mantener viva

config-arquivos-comparar = Comparar carpetas al abrir
    .descricao = Las pestañas nuevas ya se abren comparando el panel local con el remoto.
    .palavras = diferencia diff sincronizar nuevo amarillo

config-arquivos-comparar_modo = Criterio de la comparación
    .descricao = Por el tamaño o por la fecha de modificación. Se aplica a las pestañas nuevas.
    .palavras = diferencia diff tamano fecha mas nuevo

config-arquivos-comparar_tolerancia_min = Tolerancia de fecha (minutos)
    .descricao = Una diferencia de hasta tantos minutos cuenta como la misma fecha. FTP lista la fecha solo hasta el minuto.
    .palavras = diferencia diff ftp mtime reloj

config-arquivos-ocultar_identicos = Ocultar archivos idénticos al comparar
    .descricao = Solo aparece lo que difiere. Se aplica a las pestañas nuevas.
    .palavras = diferencia diff ocultar iguales

config-arquivos-sincronizar = Navegación sincronizada al abrir
    .descricao = Las pestañas nuevas ya se abren con la navegación sincronizada: entrar o subir en un panel lleva al otro a la misma carpeta. La base es el par de carpetas en el que se abre la pestaña.
    .palavras = sincronizar navegacion reflejar espejo link

# FTP

config-ftp-seguranca = Seguridad
    .descricao = Cómo protegen el tráfico las conexiones FTP nuevas.
    .palavras = ftps tls ssl cifrado

config-ftp-modo = Modo de la conexión de datos
    .descricao = Pasivo o activo. Con un perfil de red con proxy, la aplicación fuerza el pasivo.
    .palavras = pasv port pasivo activo

config-ftp-tls = Versión de TLS
    .descricao = La versión de TLS de las conexiones FTPS nuevas.
    .palavras = ftps tls 1.2 tls 1.3

config-ftp-codificacao = Codificación de los nombres
    .descricao = Cómo leer los nombres de los archivos del servidor.
    .palavras = utf-8 latin-1 acentos charset

# S3

config-s3-regiao = Región predeterminada
    .descricao = De las conexiones S3 nuevas con el proveedor "otro".
    .palavras = region us-east-1 aws

config-s3-estilo = Estilo de dirección predeterminado
    .descricao = Ruta o host virtual, en las conexiones S3 nuevas con el proveedor "otro".
    .palavras = path style ruta virtual host endpoint

config-s3-parte_mib = Tamaño de la parte
    .descricao = El tamaño de cada parte del envío por partes (multipart), de 5 a 512 MiB.
    .palavras = multipart upload mib

config-s3-classe = Clase de almacenamiento predeterminada
    .descricao = STANDARD, STANDARD_IA, INTELLIGENT_TIERING, GLACIER_IR. Vacía usa la del bucket.
    .palavras = storage class clase almacenamiento glacier standard

config-s3-validade_link = Validez predeterminada del vínculo
    .descricao = El primer elemento del menú Copiar vínculo prefirmado.
    .palavras = presigned prefirmado compartir url caduca

# rclone

config-rclone-binario = Programa rclone
    .descricao = Vacío lo busca en el PATH. Se aplica a las conexiones que se abran después.
    .palavras = binario ejecutable ruta

config-rclone-arquivo_config = Archivo de configuración
    .descricao = Vacío usa el predeterminado de rclone.
    .palavras = rclone.conf remotos

config-rclone-credencial = Contraseña de la configuración cifrada
    .descricao = El elemento de la bóveda con la contraseña del archivo de configuración. La contraseña no sale de la bóveda.
    .palavras = config password boveda cifrada

config-acao-testar_rclone = Versión
    .descricao = Busca rclone y muestra la versión.
    .palavras = probar version instalado

# Redes

config-acao-perfis_de_rede = Perfiles de red
    .descricao = Una red por cliente: proxy, salto y cómo levantar la VPN.
    .palavras = proxy vpn salto socks

config-redes-espera_padrao_secs = Espera predeterminada del proxy
    .descricao = Segundos que un perfil nuevo espera a que la red se levante (de 1 a 600).
    .palavras = timeout perfil vpn

config-redes-descer_ao_fechar = Desconectar las redes al cerrar
    .descricao = Desactivado, cerrar la aplicación no ejecuta el comando de desconexión de los perfiles en uso.
    .palavras = vpn desconectar apagar cerrar

# X11 y túneles

config-x11-padrao = X11 predeterminado
    .descricao = El modo de las conexiones SSH que no eligen X11. De confianza es ssh -Y; que no es de confianza, ssh -X.
    .palavras = x11 display ventanas grafico xauth virt-manager

config-x11-display = Servidor X de este equipo (DISPLAY)
    .descricao = Vacío usa la variable DISPLAY. En Windows, VcXsrv o Xming suelen ser localhost:0; en macOS, XQuartz.
    .palavras = x11 display xming vcxsrv xquartz xwayland

config-x11-virt_manager = Comando del virt-manager remoto
    .descricao = Lo que el menú Abrir virt-manager ejecuta en el servidor.
    .palavras = virt-manager libvirt kvm maquinas virtuales

config-tuneis-outras_maquinas = Permitir otros equipos en los túneles nuevos
    .descricao = Los túneles -L y -D nuevos escuchan en todas las interfaces, no solo en 127.0.0.1. Cada túnel puede cambiarlo.
    .palavras = tunel port forwarding reenvio socks gatewayports red escuchar

# Puertos del servidor (M58)

config-portas-intervalo_secs = Actualizar los puertos cada (segundos)
    .descricao = Con el panel Puertos abierto, la lista se actualiza en ese intervalo. 0 actualiza solo con el botón.
    .palavras = puertos servidor actualizar intervalo listen

config-portas-sondar = Ver qué responde en los puertos nuevos
    .descricao = Abre una conexión breve por SSH en cada puerto nuevo para encontrar el título de la página y aplicaciones como phpMyAdmin.
    .palavras = puertos sondeo titulo phpmyadmin http

config-portas-sonda_ms = Tiempo del sondeo (milisegundos)
    .descricao = El tiempo total que el sondeo espera por un puerto. Mínimo 100.
    .palavras = puertos sondeo espera timeout

config-portas-usar_ferramentas = Usar ss, netstat, docker y podman del servidor
    .descricao = Cuando existen, traen el programa de cada puerto y los contenedores. Desactivado, solo SFTP lee los puertos.
    .palavras = puertos ss netstat docker podman contenedores

config-portas-avisar_novas = Avisar cuando aparece un puerto nuevo
    .descricao = Un aviso en las sesiones SSH con el botón para abrir el panel. Funciona con el panel cerrado.
    .palavras = puertos aviso nuevo notificar

config-portas-links_pelo_tunel = Abrir vínculos locales por el túnel
    .descricao = Un localhost:3000 en el terminal de una pestaña SSH abre el servidor de la conexión, no este equipo.
    .palavras = puertos vinculos links terminal localhost tunel navegador

config-portas-mostrar_sistema = Mostrar los puertos del sistema
    .descricao = Incluye en la lista el propio SSH, el DNS local y otros puertos del sistema.
    .palavras = puertos sistema ssh dns

# Ventanas y espacios

config-janelas-pasta_espacos = Carpeta de los espacios de trabajo
    .descricao = Vacía usa la carpeta espacos dentro de la carpeta de datos.
    .palavras = workspace espacio directorio layouts

config-janelas-abrir_espaco = Abrir un espacio guardado de una ventana en
    .descricao = Dónde abre el doble clic en un espacio de origen Ventana o Todas.
    .palavras = workspace espacio ventana

config-janelas-tear_off = Soltar una pestaña fuera de la ventana abre otra
    .descricao = Desactivado, soltar fuera no hace nada.
    .palavras = desacoplar arrastrar pestana ventana

config-acao-perfis_de_ia = Proveedores de IA
    .descricao = Dónde están los modelos: en tu equipo, en la red local o en internet.
    .palavras = ia modelo ollama openai anthropic openrouter groq clave api lm studio

config-ia-redigir = Quitar contraseñas y claves de lo que va a la IA
    .descricao = Reemplaza por [secreto eliminado] lo que la aplicación reconoce como contraseña o clave antes de enviar.
    .palavras = ia redaccion secreto privacidad enmascarar

config-ia-redigir_extras = Otros textos que quitar
    .descricao = Uno por línea. Empieza la línea con re: para usar una expresión regular.
    .palavras = ia redaccion regex expresion regular secreto privacidad

config-ia-mostrar_no_menu = Mostrar los elementos de IA en el menú del terminal
    .descricao = Explicar la selección, explicar el último error y generar un comando.
    .palavras = ia menu contexto boton derecho

config-ia-mostrar_o_que_sai = Mostrar lo que va a salir antes del primer envío de cada pestaña
    .descricao = Abre el texto exacto, ya sin los secretos, para que lo revises antes de enviar.
    .palavras = ia privacidad confirmar enviar

config-ia-linhas_contexto = Líneas del terminal en el contexto
    .descricao = Cuántas de las últimas líneas visibles del terminal van junto con la solicitud (de 1 a 100 000).
    .palavras = ia contexto historial

config-ia-max_caracteres = Tamaño máximo del contexto
    .descricao = En caracteres. Si se supera, la aplicación corta las líneas más antiguas. 0 no limita.
    .palavras = ia contexto limite modelo local

config-ia-comando_shell = Shell que suponer
    .descricao = Se usa cuando la aplicación no consigue saber cuál es el shell del servidor.
    .palavras = ia bash zsh fish powershell

config-ia-espera_resposta = Esperar la respuesta durante
    .descricao = En segundos. 0 espera sin límite, lo que sirve para modelos locales lentos.
    .palavras = ia timeout tiempo limite

config-ia-mcp_ligado = Dejar que los asistentes controlen la aplicación
    .descricao = Claude Code y Codex pasan a ver y usar las conexiones que permitas, por MCP. Tirech Omni tiene que estar abierto.
    .palavras = mcp asistente ia ai claude codex controlar activar

config-ia-bloqueio_extras = Comandos que siempre piden aprobación
    .descricao = Uno por línea: el nombre de un programa o, con re: al principio, una expresión regular. Se suman a la lista de bloqueo de la aplicación, incluso con la conexión en Libre.
    .palavras = mcp bloqueo permiso aprobar comando peligroso regex

config-ia-auditoria = Grabar el registro de lo que hacen los asistentes
    .descricao = Un archivo por día en la carpeta de datos, sin contraseñas. Los comandos y textos pasan por la misma limpieza de secretos que lo que va a la IA.
    .palavras = mcp auditoria registro log historial secreto

config-ia-marca_secs = Marcar la pestaña después de la última acción durante
    .descricao = En segundos. Cuánto tiempo muestra la pestaña que un asistente la tocó.
    .palavras = mcp marca pestana tiempo ia

config-ia-linhas_por_leitura = Líneas por lectura de archivo
    .descricao = Cuántas líneas recibe el asistente cuando lee un archivo del espacio de trabajo sin pedir un fragmento (de 1 a 100 000).
    .palavras = mcp leer archivo lineas espacio

config-ia-caracteres_por_resposta = Tamaño máximo de una respuesta
    .descricao = En caracteres. Si se supera, la aplicación la corta y dice cómo continuar. 0 no limita.
    .palavras = mcp respuesta limite tamano caracteres

config-ia-comandos_na_aba = Ejecutar los comandos de los asistentes en la pestaña
    .descricao = Aunque el asistente pida ejecutarlo aparte, el comando aparece en la pestaña, como si lo escribieras tú. Si la pestaña está ocupada (un programa abierto u otro comando en ejecución), se ejecuta aparte, sin aparecer.
    .palavras = mcp ejecutar comando pestana visible separado terminal

config-ia-finais_de_prompt = Finales de prompt adicionales
    .descricao = Uno por línea. La aplicación reconoce # $ % y > como final del prompt; lo que pongas aquí también cuenta, para saber cuándo terminó un comando.
    .palavras = mcp prompt terminal ejecutar fin comando

config-ia-pasta_troca = Carpeta para intercambiar archivos con el asistente
    .descricao = Donde caen los archivos que el asistente descarga y desde donde suben los que envía. Vacío es Downloads/Tirech Omni.
    .palavras = mcp archivos intercambio descargar enviar carpeta

config-ia-preferido = Asistente del atajo
    .descricao = Lo que Ctrl+Alt+A abre, instala o comprueba en la pestaña SSH en la que estás.
    .palavras = ia asistente claude codex opencode atajo preferido predeterminado

config-ia-abrir_em = Abrir el asistente en
    .descricao = En una pestaña nueva, en un panel libre de la cuadrícula o escribiendo el comando en la pestaña actual (solo en el shell, fuera de programas de pantalla completa).
    .palavras = ia asistente abrir pestana panel ventana donde

config-ia-shell_ao_sair = Quedarse en un shell cuando el asistente termine
    .descricao = La pestaña sigue abierta, en la misma carpeta, en lugar de cerrarse.
    .palavras = ia asistente salir terminar cerrar shell pestana

config-ia-tunel_codex = Abrir el puerto 1455 en el inicio de sesión de Codex
    .descricao = Permite que funcione el inicio de sesión por el navegador de este equipo. Si el puerto está en uso, usa Iniciar sesión > Codex (con código).
    .palavras = ia codex login tunel puerto 1455 navegador

config-ia-verificar_ao_abrir = Comprobar los asistentes al abrir una pestaña SSH
    .descricao = Ejecuta un comando breve en el servidor para saber qué está instalado. El resultado vale durante 10 minutos.
    .palavras = ia asistente comprobar verificar sondear instalado servidor

config-ia-mostrar_botao = Mostrar el botón de los asistentes en la barra
    .descricao = El menú también está en Herramientas, en el menú de la pestaña y en el menú de la conexión.
    .palavras = ia asistente boton barra encabezado

config-ia-mostrar_claude = Mostrar Claude Code en los menús
    .descricao = Desactivado, desaparece de los menús y de lo que abre el atajo.
    .palavras = ia claude code anthropic menu

config-ia-mostrar_codex = Mostrar Codex en los menús
    .descricao = Desactivado, desaparece de los menús y de lo que abre el atajo.
    .palavras = ia codex openai menu

config-ia-mostrar_opencode = Mostrar OpenCode en los menús
    .descricao = Desactivado, desaparece de los menús y de lo que abre el atajo.
    .palavras = ia opencode menu

config-ia-metodo_claude = Instalar Claude Code mediante
    .descricao = Automático usa el instalador oficial y, si el servidor no lo permite, npm.
    .palavras = ia claude instalar metodo npm homebrew nativo

config-ia-metodo_codex = Instalar Codex mediante
    .descricao = Automático usa el instalador oficial y, si el servidor no lo permite, npm.
    .palavras = ia codex instalar metodo npm homebrew nativo

config-ia-metodo_opencode = Instalar OpenCode mediante
    .descricao = Automático usa el instalador oficial y, si el servidor no lo permite, npm.
    .palavras = ia opencode instalar metodo npm homebrew nativo

config-ia-argumentos_claude = Argumentos de Claude Code
    .descricao = Van después del comando, tal como los escribas. La conexión puede tener los suyos.
    .palavras = ia claude argumentos opciones parametros

config-ia-argumentos_codex = Argumentos de Codex
    .descricao = Van después del comando, tal como los escribas. La conexión puede tener los suyos.
    .palavras = ia codex argumentos opciones parametros

config-ia-argumentos_opencode = Argumentos de OpenCode
    .descricao = Van después del comando, tal como los escribas. La conexión puede tener los suyos.
    .palavras = ia opencode argumentos opciones parametros

config-ia-local_pasta = Carpeta de trabajo de los asistentes
    .descricao = Dónde Controlar con abre Claude Code, Codex y OpenCode de este equipo, y dónde queda lo que crean. Vacía usa una carpeta de la aplicación, lejos de tus archivos.
    .palavras = ia claude codex opencode carpeta trabajo directorio basura limpiar controlar

config-acao-ia_local_abrir_pasta = Abrir la carpeta de trabajo
    .descricao = Muestra en el explorador de archivos lo que dejaron los asistentes.
    .palavras = ia claude codex carpeta trabajo abrir

config-acao-ia_local_limpar = Limpiar la carpeta de trabajo…
    .descricao = Borra todo lo que hay dentro. Los asistentes abiertos siguen.
    .palavras = ia claude codex carpeta trabajo limpiar borrar basura

config-ia-local_claude_caminho = Ruta de Claude Code
    .descricao = El programa de este equipo. Vacío lo busca en las carpetas del sistema y de tu usuario.
    .palavras = ia claude ruta ejecutable programa equipo local

config-ia-local_claude_argumentos = Argumentos de Claude Code
    .descricao = Van después del comando cuando usas Controlar con Claude Code. Aquí van las opciones que quieras.
    .palavras = ia claude argumentos opciones parametros equipo local controlar

config-ia-local_codex_caminho = Ruta de Codex
    .descricao = El programa de este equipo. Vacío lo busca en las carpetas del sistema y de tu usuario.
    .palavras = ia codex ruta ejecutable programa equipo local

config-ia-local_codex_argumentos = Argumentos de Codex
    .descricao = Van después del comando cuando usas Controlar con Codex. Aquí van las opciones que quieras.
    .palavras = ia codex argumentos opciones parametros equipo local controlar

config-ia-local_opencode_caminho = Ruta de OpenCode
    .descricao = El programa de este equipo. Vacío lo busca en las carpetas del sistema y de tu usuario.
    .palavras = ia opencode ruta ejecutable programa equipo local

config-ia-local_opencode_argumentos = Argumentos de OpenCode
    .descricao = Van después del comando cuando usas Controlar con OpenCode. Aquí van las opciones que quieras.
    .palavras = ia opencode argumentos opciones parametros equipo local controlar

config-ia-conversas_lembrar = Recordar las conversaciones de cada pestaña
    .descricao = Guarda el id de las conversaciones de Claude Code, Codex y OpenCode con cada pestaña, para continuar después. Desactivado, Controlar con siempre empieza una conversación nueva.
    .palavras = ia claude codex opencode conversacion sesion resume continuar diario historial recordar

config-ia-conversas_reabrir = Volver a abrir los asistentes al abrir la aplicación
    .descricao = Al abrir la aplicación o un espacio, los asistentes que estaban controlando las pestañas vuelven, continuando la misma conversación.
    .palavras = ia claude codex opencode conversacion reabrir restaurar continuar espacio

config-ia-conversas_ao_controlar = Cuando la pestaña ya tuvo una conversación
    .descricao = Qué hace Controlar con cuando el asistente ya conversó sobre esa pestaña: preguntar, continuar la más reciente o empezar otra.
    .palavras = ia claude codex opencode conversacion continuar nueva preguntar controlar

config-acao-mcp_como = Cómo conectar un asistente…
    .descricao = Los comandos para Claude Code y Codex, listos para copiar.
    .palavras = mcp claude codex conectar configurar puente

config-acao-mcp_registro = Ver el registro de hoy
    .descricao = Abre el archivo del día con lo que hicieron los asistentes.
    .palavras = mcp auditoria registro log

config-acao-mcp_pasta_registro = Abrir la carpeta del registro
    .descricao = La carpeta con un archivo por día.
    .palavras = mcp auditoria registro log carpeta

config-acao-mcp_espaco_novo = Nuevo espacio de trabajo…
    .descricao = Una carpeta de un servidor que el asistente lee y edita directamente, sin abrir un terminal.
    .palavras = mcp espacio trabajo carpeta servidor ssh sftp

# Fin de la página

config-acao-exportar = Exportar configuración
    .descricao = Graba un archivo con la configuración, sin contraseñas.
    .palavras = backup copia guardar json

config-acao-importar = Importar configuración
    .descricao = Lee un archivo exportado y reemplaza la configuración.
    .palavras = restaurar cargar json

config-acao-restaurar_tudo = Restablecer toda la configuración
    .descricao = Vuelve todo a los valores predeterminados y mantiene la carpeta de datos.
    .palavras = predeterminado restablecer resetear limpiar

## Opciones de las elecciones (`escolha!`): `opcao-<escolha>-<variante>`

# Tema
opcao-tema-sistema = Usar el tema del sistema
opcao-tema-claro = Claro
opcao-tema-escuro = Oscuro

# Esquema
opcao-esquema-campbell = Campbell
opcao-esquema-tema = Seguir el tema de la aplicación
opcao-esquema-campbell-powershell = Campbell Powershell
opcao-esquema-vintage = Vintage
opcao-esquema-one-half-dark = One Half Dark
opcao-esquema-one-half-light = One Half Light
opcao-esquema-tango-dark = Tango Dark
opcao-esquema-tango-light = Tango Light

# Cursor
opcao-cursor-barra = Barra
opcao-cursor-bloco = Bloque
opcao-cursor-sublinhado = Subrayado

# BotaoDireito
opcao-botao-direito-menu = Abrir el menú
opcao-botao-direito-copiar-colar = Copiar y pegar

# ColarArquivos
opcao-colar-arquivos-enviar = Enviar y pegar la ruta
opcao-colar-arquivos-caminho-local = Pegar solo la ruta local
opcao-colar-arquivos-perguntar = Preguntar

# Campainha
opcao-campainha-piscar = Hacer parpadear la pantalla
opcao-campainha-nenhuma = Ninguna

# TmuxAoFechar
opcao-tmux-ao-fechar-perguntar = Preguntar
opcao-tmux-ao-fechar-encerrar = Terminar la sesión
opcao-tmux-ao-fechar-manter = Mantener en el servidor

# ConversaAoControlar
opcao-conversa-ao-controlar-perguntar = Preguntar
opcao-conversa-ao-controlar-continuar = Continuar la más reciente
opcao-conversa-ao-controlar-nova = Empezar una nueva

# ChaveNova
opcao-chave-nova-perguntar = Preguntar
opcao-chave-nova-aceitar = Aceptar y guardar

# X11Padrao
opcao-x11-padrao-desligado = Desactivado
opcao-x11-padrao-nao-confiavel = No es de confianza
opcao-x11-padrao-confiavel = De confianza

# ModoReconectar
opcao-modo-reconectar-oferecer = Ofrecer reconectar
opcao-modo-reconectar-nada = No hacer nada
opcao-modo-reconectar-sozinho = Reconectar automáticamente

# AbrirEm
opcao-abrir-em-painel-ativo = Panel activo de la última ventana activa
opcao-abrir-em-nova-janela = Nueva ventana

# TipoChave
opcao-tipo-chave-ed25519 = Ed25519
opcao-tipo-chave-ecdsa256 = ECDSA P-256
opcao-tipo-chave-ecdsa384 = ECDSA P-384
opcao-tipo-chave-ecdsa521 = ECDSA P-521
opcao-tipo-chave-rsa3072 = RSA 3072
opcao-tipo-chave-rsa4096 = RSA 4096

# ConflitoCfg
opcao-conflito-cfg-perguntar = Preguntar
opcao-conflito-cfg-sobrescrever = Sobrescribir
opcao-conflito-cfg-retomar = Reanudar
opcao-conflito-cfg-pular = Omitir
opcao-conflito-cfg-renomear = Cambiar el nombre

# EnviarEditado
opcao-enviar-editado-perguntar = Preguntar
opcao-enviar-editado-sempre = Siempre

# ValidadeLink
opcao-validade-link-uma-hora = 1 hora
opcao-validade-link-um-dia = 1 día
opcao-validade-link-sete-dias = 7 días

# AbrirEspaco
opcao-abrir-espaco-nova-janela = Nueva ventana
opcao-abrir-espaco-janela-ativa = Ventana activa

# AoFechar
opcao-ao-fechar-perguntar = Preguntar
opcao-ao-fechar-bandeja = Dejar en la bandeja
opcao-ao-fechar-fechar = Cerrar la aplicación

# ModoNotas
opcao-modo-notas-editar = Editar
opcao-modo-notas-dividir = Dividir
opcao-modo-notas-ler = Leer

# ModoComparar
opcao-modo-comparar-tamanho = Tamaño
opcao-modo-comparar-data = Fecha de modificación

# PaginaInicial
opcao-pagina-inicial-auto = Automática
opcao-pagina-inicial-inicio = Inicio
opcao-pagina-inicial-terminais = Terminales

# Idioma: los nombres de los idiomas aparecen en su propia lengua, en todos los archivos
opcao-idioma-sistema = Usar el idioma del sistema
opcao-idioma-pt-br = Português (Brasil)
opcao-idioma-en-us = English
opcao-idioma-es-es = Español

## Página

config-titulo = Configuración
config-buscar = Buscar en la configuración
config-expandir-tudo = Expandir todo
config-recolher-tudo = Contraer todo
config-busca-vazia = No se encontró ninguna configuración para "{ $busca }".
config-avisos-leitura = Algunas opciones volvieron a los valores predeterminados
config-desfazer = Deshacer
config-desfeito = Deshecho.
config-nada-mudou = No cambió nada: ya estaba así.
config-restaurar-secao = Restablecer los valores predeterminados de esta sección
config-secao-restaurada = { $secao }: se restablecieron los valores predeterminados.
config-tudo-restaurado = Toda la configuración volvió a los valores predeterminados.
config-escolher = Elegir…
config-limpar-pasta-titulo = ¿Limpiar la carpeta de trabajo?
config-limpar-pasta-texto = { $n ->
    [one] ¿Borrar 1 elemento de { $pasta }? No se puede deshacer.
    *[other] ¿Borrar { $n } elementos de { $pasta }? No se puede deshacer.
    }
config-limpar-pasta-apagar = Borrar
config-limpar-pasta-vazia = La carpeta de trabajo ya está vacía.
config-limpar-pasta-feito = { $n ->
    [one] 1 elemento borrado de la carpeta de trabajo.
    *[other] { $n } elementos borrados de la carpeta de trabajo.
    }
config-limpar-pasta-erro = { $n ->
    [one] 1 elemento borrado, pero otros no se pudieron borrar: { $erro }
    *[other] { $n } elementos borrados, pero otros no se pudieron borrar: { $erro }
    }
config-limpar-pasta-recusada = La carpeta de trabajo es tu carpeta personal o la raíz del disco: la aplicación no la vacía. Elige una carpeta solo para los asistentes.
config-abrir-pasta-erro = No se pudo abrir { $pasta }: { $erro }
config-bandeja-indisponivel = Este sistema no muestra iconos en la bandeja (en GNOME, instala la extensión AppIndicator).
config-erro-abrir = No se pudo abrir { $caminho }: { $erro }
config-erro-gravar = No se pudo guardar { $caminho }: { $erro }
config-erro-ler = No se pudo leer { $caminho }: { $erro }
config-exportar-titulo = Exportar la configuración
config-exportado = Configuración exportada a { $caminho }.
config-importar-titulo = Importar la configuración
config-importado = Configuración importada de { $caminho }.

# El idioma: el cambio se aplica al reiniciar.
config-geral-idioma-aviso = El nuevo idioma se aplica después de reiniciar { -app }.
config-geral-idioma-reiniciar = Reiniciar ahora

# La carpeta de datos.
config-pasta-origem-variavel = { $pasta } (definida por { $variavel }; para cambiarla, quita la variable).
config-pasta-origem-portatil = { $pasta } (versión portable: la carpeta tirech-omnidata junto al programa).
config-pasta-portatil-senha = La bóveda de esta carpeta no tiene contraseña maestra
    .descricao = Quien copie la carpeta tirech-omnidata abre las contraseñas guardadas. Para cifrar la bóveda con una contraseña tuya, usa Bóveda > Contraseña maestra…
config-pasta-origem-escolhida = { $pasta } (elegida por ti).
config-pasta-origem-padrao = { $pasta } (la predeterminada).
config-pasta-titulo = Carpeta de datos
config-pasta-abrir = Abrir la carpeta
config-pasta-mudar = Cambiar…
    .descricao = Copia los datos a una carpeta vacía, o usa una que ya tenga datos.
config-pasta-padrao = Volver a la predeterminada
config-pasta-reiniciar = Reiniciar ahora
    .descricao = La nueva carpeta, { $pasta }, se aplica después de reiniciar.
config-pasta-copiou = Los datos se copiaron a { $pasta }. Reinicia para usarla.
config-pasta-apontou = La aplicación va a usar los datos que ya están en { $pasta }. Reinicia para usarla.
config-pasta-erro-mudar = No se pudo cambiar la carpeta de datos: { $erro }.
config-pasta-voltou = La carpeta de datos vuelve a ser { $pasta }. Reinicia para usarla.
config-pasta-erro-padrao = No se pudo volver a la predeterminada: { $erro }.

# La ubicación de la bóveda.
config-cofre-local-titulo = Ubicación de la bóveda
config-cofre-local-pasta-dados = { $pasta } (la carpeta de datos).
config-cofre-local-pasta = { $pasta }.
config-cofre-local-abrir = Abrir la carpeta
config-cofre-local-mudar = Cambiar…
    .descricao = Lleva la bóveda a otra carpeta, o usa la que ya esté allí.
config-cofre-local-padrao = Volver a la carpeta de datos

# La prueba de rclone y la contraseña de su configuración.
config-rclone-nao-achei = No se encontró rclone. Instálalo o elige el programa arriba.
config-rclone-terminou = { $programa } terminó con { $status }.
config-rclone-erro-rodar = No se pudo ejecutar { $programa }: { $erro }
config-credencial-nenhuma = Ningún elemento de la bóveda
config-credencial-inexistente = No hay un elemento "{ $titulo }" en la bóveda (o la bóveda está bloqueada).

# Atajos.
config-titulo-atalhos = Atajos
    .resumo = Teclas de la aplicación y de los paneles
config-atalhos-palavras = atajo atajos teclado tecla
config-atalhos-app = { -app }
    .descricao = Nueva conexión, diseños, bóveda, búsqueda y las demás acciones de la aplicación.
config-atalhos-paineis = Paneles del terminal
    .descricao = Dividir, mover el foco, cambiar el tamaño, cerrar y ampliar los paneles.
config-restaurar-atalhos = Restablecer todos los atajos
config-atalho-restaurar = Restablecer
config-atalho-sem-padrao = Sin atajo de fábrica.
config-atalho-padrao = Predeterminado: { $atalho }.
config-atalho-tambem = También lo usa { $outra }.

# Sugerencias de los campos (lo que se aplica cuando el campo está vacío).
config-dica-shell = $SHELL
config-dica-argumentos-shell = -l
config-dica-pasta-shell = la carpeta personal
config-dica-logs = logs en la carpeta de datos
config-dica-pasta-colar = /tmp/tirech-colar
config-dica-usuario = el usuario del sistema
config-dica-term = xterm-256color
config-dica-known-hosts = ~/.ssh/known_hosts
config-dica-interpretador = sh -s
config-dica-scripts = scripts en la carpeta de datos
config-dica-socket = una dirección de la aplicación
config-dica-pasta-chaves = ~/.ssh
config-dica-editor = el programa del sistema
config-dica-pasta-local = la carpeta personal
config-dica-regiao = us-east-1
config-dica-classe = la del bucket
config-dica-rclone = el rclone del PATH
config-dica-rclone-config = el predeterminado de rclone
config-dica-display = la variable DISPLAY
config-dica-virt-manager = virt-manager --no-fork
config-dica-espacos = espacos en la carpeta de datos
config-dica-shell-ia = bash
config-dica-redigir-extras = uno por línea; re:… para regex
config-dica-bloqueio-mcp = uno por línea; re:… para regex
config-dica-finais-prompt = uno por línea
config-dica-argumentos-ia = por ejemplo --model sonnet
config-dica-caminho-ia-local = buscar automáticamente
config-dica-pasta-troca = Downloads/Tirech Omni
config-dica-atalho = Ctrl+Shift+N
atalho-nova-conexao = Nueva conexión
atalho-novo-grupo = Nuevo grupo
atalho-shell-local = Abrir shell local
atalho-layout-1 = Diseño 1
atalho-layout-2x2 = Diseño 2 × 2
atalho-layout-3x3 = Diseño 3 × 3
atalho-identificar-paineis = Identificar los paneles
atalho-painel-conexoes = Mostrar u ocultar las conexiones
atalho-painel-propriedades = Mostrar u ocultar las propiedades
atalho-painel-notas = Mostrar u ocultar la lista de notas
atalho-painel-fila = Mostrar u ocultar las transferencias
atalho-nova-janela = Nueva ventana
atalho-multi-entrada = Envío a varios
atalho-cluster = Modo clúster
atalho-cluster-ia = Clúster con IA
atalho-scripts = Ejecutar script
atalho-reconectar-caidas = Reconectar las pestañas caídas
atalho-fechar-caidas = Cerrar las pestañas caídas
atalho-fechar-todas = Cerrar todas las pestañas
atalho-cofre = Abrir la bóveda
atalho-notas = Abrir las notas
atalho-ia-comando = Generar comando con IA
atalho-ia-chat = Abrir o cerrar el chat con la IA
atalho-ia-abrir = Abrir el asistente de IA en el servidor
atalho-busca = Buscar conexiones y elementos de la bóveda
atalho-configuracoes = Abrir la configuración
atalho-aumentar-fonte = Aumentar la fuente del terminal
atalho-diminuir-fonte = Reducir la fuente del terminal
atalho-fonte-padrao = Tamaño predeterminado de la fuente del terminal
atalho-dividir-abaixo = Dividir el panel hacia abajo
atalho-dividir-direita = Dividir el panel hacia la derecha
atalho-duplicar-sessao = Duplicar la sesión en un panel nuevo
atalho-focar-esquerda = Enfocar el panel de la izquierda
atalho-focar-direita = Enfocar el panel de la derecha
atalho-focar-acima = Enfocar el panel de arriba
atalho-focar-abaixo = Enfocar el panel de abajo
atalho-redimensionar-esquerda = Mover la división a la izquierda
atalho-redimensionar-direita = Mover la división a la derecha
atalho-redimensionar-acima = Mover la división hacia arriba
atalho-redimensionar-abaixo = Mover la división hacia abajo
atalho-fechar-painel = Cerrar el panel
atalho-zoom-painel = Alternar el zoom del panel
config-logs-existentes = { $n ->
    [one] La carpeta de los logs tiene 1 log de texto, grabado antes de que los logs se cifraran.
    *[other] La carpeta de los logs tiene { $n } logs de texto, grabados antes de que los logs se cifraran.
    }
config-logs-existentes-cifrar = { $n ->
    [one] Cifrar el log de texto que ya existe
    *[other] Cifrar los { $n } logs de texto que ya existen
    }
config-ia-extra-invalido = { $texto }: { $erro }

# AssistentePreferido
opcao-assistente-claude = Claude Code
opcao-assistente-codex = Codex
opcao-assistente-opencode = OpenCode

# AbrirIa
opcao-abrir-ia-aba-nova = En una pestaña nueva
opcao-abrir-ia-painel-livre = En un panel libre
opcao-abrir-ia-nesta-aba = En esta pestaña

# MetodoIa
opcao-metodo-ia-automatico = Automático
opcao-metodo-ia-nativo = Instalador oficial
opcao-metodo-ia-npm = npm
opcao-metodo-ia-homebrew = Homebrew
