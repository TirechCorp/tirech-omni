# Mensajes de error y textos de áreas pequeñas.
abrir-log-falhou = No se pudo abrir el registro { $caminho }: { $erro }
abrir-conectado = Conectado a { $nome }.
anexo-io = No se pudo leer ni escribir el adjunto: { $erro }.
anexo-sem-espaco = No hay espacio en el disco: el adjunto necesita { $precisa } y hay { $livre } libres.
anexo-corrompido = El adjunto está dañado o incompleto.
anexo-ausente = El archivo de este adjunto todavía no está en este equipo.
anexo-cancelado = Cancelado.
anexo-sem-aleatorio = sin fuente aleatoria: { $erro }
anexo-invalido = adjunto no válido
anexo-ja-existe = el adjunto ya existe
anexo-nao-cifrou = no se pudo cifrar el adjunto
anexo-blocos-demais = el adjunto tiene demasiados bloques
armazenamento-caminho-absoluto = la ruta de la carpeta debe ser absoluta
armazenamento-pasta-nao-vazia = la carpeta no está vacía y no tiene datos de { -app }
armazenamento-corrompido-copia = El archivo { $o_que } estaba dañado; se guardó una copia en { $copia }.
armazenamento-corrompido-sem-copia = El archivo { $o_que } estaba dañado y no se pudo guardar una copia: { $erro }.
armazenamento-conexoes-corrompido-copia = El archivo de conexiones estaba dañado; se guardó una copia en { $copia }.
armazenamento-conexoes-corrompido-sem-copia = El archivo de conexiones estaba dañado y no se pudo guardar una copia: { $erro }.
armazenamento-somente-leitura = otra copia de { -app } está abierta y es la dueña de la configuración: esta solo lee y no guarda nada
armazenamento-conexoes-versao-mais-nova = El archivo de conexiones lo guardó una versión más nueva de { -app }. No se modificará aquí: lo que cambie en las conexiones dura solo hasta cerrar.
armazenamento-conexoes-conflito = Otro programa cambió el archivo de conexiones al mismo tiempo; su versión quedó en { $copia }.
armazenamento-so-leitura-titulo = Solo lectura
armazenamento-so-leitura-aviso = Otra copia de { -app } está abierta y es la dueña de la configuración. Esta no guarda nada: lo que cambie aquí se pierde al cerrar, y las conexiones que guarde la otra aparecen solas.
armazenamento-so-leitura-dona-fechou = La otra copia de { -app } se cerró. Reinicie esta para poder guardar.
armazenamento-so-leitura-reiniciar = Reiniciar
recentes-o-que = de recientes
agente-embutido-nao-iniciou = No se pudo iniciar el agente integrado ({ $erro }). La aplicación usa el agente del sistema.
agente-endereco-invalido = La dirección del agente debe ser una ruta completa: { $endereco }
agente-pasta-curta-erro = No se pudo preparar la carpeta { $pasta } para el socket del agente: { $erro }
agente-pasta-curta-insegura = La carpeta { $pasta } ya existe y no pertenece solo a tu usuario; el socket del agente no se creará en ella.
agente-embutido-parado = el agente integrado no se está ejecutando
agente-um-programa-pageant = Un programa (por la ventana de Pageant)
agente-um-programa = Un programa
agente-pid = (pid { $pid })
espacos-faltante = { $n ->
        [1] 1 conexión de este espacio ya no existe: { $lista }
       *[other] { $n } conexiones de este espacio ya no existen: { $lista }
    }
espacos-nome-invalido = El nombre no puede estar vacío ni tener / \ : * ? " < > |
espacos-ja-existe = Ya existe un elemento con ese nombre en esta carpeta.
espacos-nao-vazia = La carpeta no está vacía.
espacos-nao-existe = El espacio o la carpeta ya no existe.
espacos-dentro-de-si-mesma = Una carpeta no se puede mover dentro de sí misma.
espacos-invalido = El archivo del espacio no es válido: { $motivo }
csv-aspas-sem-fechar = Comillas sin cerrar en la línea { $linha }
csv-formato-desconhecido = Formato de CSV desconocido: se esperaba el de Bitwarden o el de KeePassXC
cofre-json-nao-consegui-ler-tirech = No se pudo leer el JSON de { -app }: { $erro }.
cofre-json-nao-consegui-ler = No se pudo leer el JSON: { $erro }.
cofre-json-nao-e-cofre = Ese JSON no es una bóveda de { -app }.
cofre-json-versao-nova = Esa bóveda se exportó con una versión más reciente de { -app }.
cofre-json-formato-desconhecido = Formato de JSON desconocido: se esperaba el de { -app } o el de Bitwarden.
bitwarden-bandeira = Marca
bitwarden-numero = Número
bitwarden-validade = Vencimiento
bitwarden-codigo = Código
bitwarden-tratamento = Tratamiento
bitwarden-nome = Nombre
bitwarden-nome-do-meio = Segundo nombre
bitwarden-sobrenome = Apellido
bitwarden-empresa = Empresa
bitwarden-email = Correo electrónico
bitwarden-telefone = Teléfono
bitwarden-endereco = Dirección
bitwarden-endereco-2 = Dirección 2
bitwarden-endereco-3 = Dirección 3
bitwarden-cidade = Ciudad
bitwarden-estado = Estado o provincia
bitwarden-cep = Código postal
bitwarden-pais = País
bitwarden-ssn = SSN
bitwarden-passaporte = Pasaporte
bitwarden-carteira = Licencia de conducir
bitwarden-chave-privada = Clave privada
bitwarden-chave-publica = Clave pública
bitwarden-impressao-digital = Huella digital
bitwarden-url = URL { $n }
bitwarden-nao-consegui-ler = No se pudo leer el JSON de Bitwarden: { $erro }.
bitwarden-cifrado = Ese JSON de Bitwarden está cifrado. Vuelve a exportarlo en formato .json (sin la opción cifrada).
bitwarden-desconhecidos = { $n ->
        [1] Se omitió 1 elemento de tipo desconocido.
       *[other] Se omitieron { $n } elementos de tipo desconocido.
    }
bitwarden-vinculados = Se omitieron los campos vinculados de Bitwarden.
ssh-config-proxyjump = ProxyJump con { $n } saltos: solo se conservó el último ({ $ultimo }).
ssh-config-proxycommand = ProxyCommand no es compatible y se omitió.
ssh-config-socks-remoto = { $diretiva } { $valor }: el SOCKS remoto (sin destino) no es compatible y se omitió.
ssh-config-nao-entendi = { $diretiva } { $valor }: no se entendió y se omitió.
ssh-config-socket-de-origem = { $diretiva } { $valor }: un socket local como origen no es compatible y se omitió.
ssh-config-nao-entendi-porta = { $diretiva } { $valor }: no se entendió el puerto y se omitió.
ssh-config-todas-as-interfaces = { $diretiva } { $origem } …: la dirección pasa a ser "todas las interfaces".
ssh-config-r-para-socket = { $diretiva } { $valor }: -R hacia un socket no es compatible y se omitió.
ssh-config-destino-incompleto = { $diretiva } { $valor }: el destino necesita servidor y puerto, y se omitió.
ssh-config-problema = { $diretiva } { $valor }: { $problema } Se omitió.
ssh-config-aviso-da-conexao = { $nome }: { $aviso }
config-fora-da-faixa = { $nome } fuera del intervalo ({ $min } a { $max }); se ajustó a { $preso }.
config-atalho-invalido = atalhos.{ $id }: no se entendió "{ $ruim }"; se restableció el valor predeterminado.
config-json-invalido = El archivo no es un JSON válido: { $erro }.
config-json-nao-e-objeto = El archivo no es un objeto JSON de configuración.
config-ilegivel-copia = La configuración era ilegible y se restableció a los valores predeterminados; la copia está en { $copia }.
config-ilegivel-sem-copia = La configuración era ilegible y se restableció a los valores predeterminados; no se pudo guardar una copia: { $erro }.
config-invalido-padrao = { $nome } no válido; se restableció el valor predeterminado ({ $valor_padrao }).
aws-perfil-sem-chaves = Se omitió el perfil { $perfil }: no tiene claves de acceso.
atalho-terminal-deixa-de-receber = El terminal deja de recibir { $atalho }.
importar-nao-suportada-conexao = { $n ->
        [1] 1 conexión { $complemento } no es compatible y se omitió.
       *[other] { $n } conexiones { $complemento } no son compatibles y se omitieron.
    }
importar-nao-suportada-sessao = { $n ->
        [1] 1 sesión { $complemento } no es compatible y se omitió.
       *[other] { $n } sesiones { $complemento } no son compatibles y se omitieron.
    }
importar-com-protocolo = con protocolo { $nome }
importar-de-codigo = de código { $n }
importar-proxy-importado = Proxy importado ({ $host }:{ $porta })
importar-ppk = Hay claves en el formato de PuTTY (.ppk). Si el agente no las acepta, conviértelas con `puttygen` o importa la clave en la bóveda.
importar-proxy-sem-suporte = Hay sesiones con proxy SOCKS4, Telnet o de comando; solo se importan SOCKS5 y HTTP.
importar-importei = { $n ->
        [1] Se importó 1 conexión
       *[other] Se importaron { $n } conexiones
    }
importar-trocadas = { $n ->
        [1] 1 reemplazada
       *[other] { $n } reemplazadas
    }
importar-puladas = { $n ->
        [1] 1 omitida
       *[other] { $n } omitidas
    }
importar-cofre-bloqueado-senhas = { $n ->
        [1] La bóveda está bloqueada: se omitió 1 contraseña, que se pedirá al conectar.
       *[other] La bóveda está bloqueada: se omitieron { $n } contraseñas, que se pedirán al conectar.
    }
cripto-winscp-formato = La contraseña de WinSCP está en un formato que no se reconoce.
cripto-mremoteng-sem-base64 = La contraseña de mRemoteNG no está en base64.
cripto-mremoteng-curta = La contraseña de mRemoteNG es demasiado corta.
cripto-mremoteng-rodadas = mRemoteNG pide un número de rondas fuera del intervalo aceptable.
cripto-nonce-torto = Nonce mal formado.
cripto-mremoteng-senha-mestra = La contraseña maestra de mRemoteNG no abre este archivo.
cripto-mremoteng-nao-cifrou = No se pudo cifrar la contraseña para mRemoteNG.
cripto-mremoteng-curta-ou-torta = La contraseña de mRemoteNG es demasiado corta o está mal formada.
cripto-chave-ou-iv = Clave o IV mal formados.
importar-filezilla-nao-e-xml = Ese XML no es un sitemanager.xml de FileZilla.
importar-filezilla-cifradas = { $n ->
        [1] 1 contraseña de FileZilla está cifrada con su clave maestra y no se importó.
       *[other] { $n } contraseñas de FileZilla están cifradas con su clave maestra y no se importaron.
    }
importar-moba-tipo-arquivo = archivo
importar-moba-tipo-navegador = navegador
importar-moba-sem-sessoes = Ese archivo no tiene sesiones de MobaXterm (secciones [Bookmarks…]).
importar-moba-senhas-no-registro = MobaXterm guarda las contraseñas en el registro de Windows, cifradas; las conexiones vienen sin contraseña.
importar-mremoteng-modo = El modo { $nome } no es compatible.
importar-mremoteng-parece-cifrado = El archivo parece cifrado por completo (FullFileEncryption de mRemoteNG). Vuelve a exportarlo sin esa opción.
importar-mremoteng-nao-e-xml = Ese XML no es un confCons.xml de mRemoteNG.
importar-mremoteng-cifrado = El archivo está cifrado por completo (FullFileEncryption de mRemoteNG). Vuelve a exportarlo sin esa opción.
importar-mremoteng-modo-sem-suporte = Las contraseñas están cifradas en el modo { $nome } de mRemoteNG, que no es compatible; las conexiones vienen sin contraseña.
importar-mremoteng-senha-recusada = La contraseña maestra indicada no abre el archivo.
importar-mremoteng-ilegiveis = { $n ->
        [1] No se pudo descifrar 1 contraseña de mRemoteNG.
       *[other] No se pudieron descifrar { $n } contraseñas de mRemoteNG.
    }
importar-putty-reg-sem-sessoes = Ese .reg no tiene sesiones de PuTTY ni de KiTTY.
importar-putty-pasta-sem-sessoes = Esa carpeta no tiene sesiones de PuTTY ni de KiTTY.
importar-winscp-sem-sessoes = Ese archivo no tiene sesiones de WinSCP (secciones [Sessions\…]).
importar-winscp-workspaces = Los espacios de trabajo de WinSCP no se importan (solo las sesiones).
importar-winscp-scp = { $n ->
        [1] 1 sesión usaba SCP y pasó a SFTP (el servidor necesita el subsistema sftp).
       *[other] { $n } sesiones usaban SCP y pasaron a SFTP (el servidor necesita el subsistema sftp).
    }
importar-winscp-ilegiveis = { $n ->
        [1] No se pudo recuperar 1 contraseña de WinSCP (archivo con contraseña maestra de WinSCP, o de otra sesión).
       *[other] No se pudieron recuperar { $n } contraseñas de WinSCP (archivo con contraseña maestra de WinSCP, o de otra sesión).
    }
importar-nativo-json = No se pudo leer el JSON de { -app }: { $erro }.
importar-nativo-e-cofre = Ese JSON es una bóveda de { -app }, sin conexiones. Impórtalo desde la página Bóveda.
importar-nativo-nao-e-exportacao = Ese JSON no es una exportación de conexiones de { -app }.
importar-nativo-conexoes = No se pudieron leer las conexiones del JSON: { $erro }.
importar-nativo-versao-nova = Esa exportación es de una versión más reciente de { -app }.
importar-winsshterm-nao-e-xml = Ese XML no es un connections.xml de WinSSHTerm.
importar-winsshterm-senhas = Las contraseñas de WinSSHTerm están cifradas en su archivo `key` y no se importaron; escríbelas en { -app }.
importar-winsshterm-proxy = El proxy de tipo { $tipo } no es compatible (solo SOCKS5 y HTTP) y no se importó.
importar-xml-atributo = Atributo mal formado en el XML: { $erro }.
importar-xml-ler = No se pudo leer el XML: { $erro }.
importar-xml-texto = Texto mal formado en el XML: { $erro }.
importar-xml-aninhado = El XML tiene demasiados elementos anidados.
importar-xml-fecha-sem-abrir = El XML cierra un elemento que no se abrió.
importar-xml-termina-aberto = El XML termina con un elemento abierto.
importar-xml-vazio = El archivo no tiene ningún elemento XML.
exportar-resumo = { $n ->
        [1] 1 conexión exportada.
       *[other] { $n } conexiones exportadas.
    }
exportar-chave-do-cofre = { $n ->
        [1] 1 conexión usa una clave de la bóveda; con { -app } abierto, el agente (M44) la ofrece al otro programa.
       *[other] { $n } conexiones usan claves de la bóveda; con { -app } abierto, el agente (M44) las ofrece al otro programa.
    }
exportar-fora-do-formato = { $n ->
        [1] 1 conexión { $tipo } no cabe en el formato de { $programa } y se omitió.
       *[other] { $n } conexiones { $tipo } no caben en el formato de { $programa } y se omitieron.
    }
exportar-chaves-cofre-bloqueado = La bóveda está bloqueada: sus claves no se guardaron y las conexiones que las usan quedaron sin archivo de clave.
exportar-chaves-gravadas = Las claves de la bóveda se guardaron, sin frase de contraseña, en { $pasta }; protege la carpeta como protegerías la clave.
exportar-chave-sumiu = Una conexión usa una clave que ya no está en la bóveda.
exportar-chave-erro = La clave «{ $titulo }»: { $erro }
exportar-chave-so-publica = La clave «{ $titulo }» solo tiene la parte pública y no se guardó.
exportar-chave-sem-formato = La clave «{ $titulo }» no existe en el formato { $formato } y no se guardó.
exportar-nao-criou-pasta = No se pudo crear { $pasta }: { $erro }
exportar-filezilla-bucket = { $n ->
        [1] 1 conexión S3 pierde el bucket, que FileZilla pide en la carpeta remota: ábrelo al conectar.
       *[other] { $n } conexiones S3 pierden el bucket, que FileZilla pide en la carpeta remota: ábrelo al conectar.
    }
exportar-filezilla-salto = { $n ->
        [1] 1 conexión tiene servidor de salto, que FileZilla no tiene.
       *[other] { $n } conexiones tienen servidor de salto, que FileZilla no tiene.
    }
exportar-moba-salto = { $n ->
        [1] 1 conexión tiene servidor de salto, que `.mxtsessions` no incluye.
       *[other] { $n } conexiones tienen servidor de salto, que `.mxtsessions` no incluye.
    }
exportar-moba-tunel = { $n ->
        [1] 1 conexión tiene túneles o proxy, que `.mxtsessions` no incluye.
       *[other] { $n } conexiones tienen túneles o proxy, que `.mxtsessions` no incluye.
    }
exportar-mremoteng-falhas = { $n ->
        [1] 1 contraseña no se cifró para mRemoteNG y quedó vacía.
       *[other] { $n } contraseñas no se cifraron para mRemoteNG y quedaron vacías.
    }
exportar-mremoteng-chave = mRemoteNG no tiene campo de clave: usa la de la sesión de PuTTY con el mismo nombre. Exporta también el `.reg` de PuTTY (Exportar ▸ PuTTY) e impórtalo en PuTTY.
exportar-mremoteng-rede = { $n ->
        [1] 1 conexión tiene proxy o servidor de salto, que mRemoteNG no guarda por conexión.
       *[other] { $n } conexiones tienen proxy o servidor de salto, que mRemoteNG no guarda por conexión.
    }
exportar-nativo-cofre-bloqueado = La bóveda está bloqueada: no se incluyeron las contraseñas.
exportar-nativo-anexos = { $n ->
        [1] 1 adjunto de nota no está en este equipo y se omitió.
       *[other] { $n } adjuntos de nota no están en este equipo y se omitieron.
    }
exportar-putty-salto = { $n ->
        [1] 1 conexión tiene servidor de salto, que PuTTY no guarda en la sesión.
       *[other] { $n } conexiones tienen servidor de salto, que PuTTY no guarda en la sesión.
    }
exportar-putty-tunel = { $n ->
        [1] 1 túnel hacia un socket Unix no cabe en PuTTY y se omitió.
       *[other] { $n } túneles hacia un socket Unix no caben en PuTTY y se omitieron.
    }
exportar-ssh-config-proxy = { $n ->
        [1] 1 conexión sale por un proxy de red, que el ssh_config de esta exportación no incluye.
       *[other] { $n } conexiones salen por un proxy de red, que el ssh_config de esta exportación no incluye.
    }
exportar-ssh-config-tunel = { $n ->
        [1] 1 túnel no cupo en directivas de OpenSSH y se omitió.
       *[other] { $n } túneles no cupieron en directivas de OpenSSH y se omitieron.
    }
exportar-winscp-grandes = { $n ->
        [1] 1 contraseña supera los 255 bytes (usuario, servidor y contraseña) y no cabe en WinSCP: quedó vacía.
       *[other] { $n } contraseñas superan los 255 bytes (usuario, servidor y contraseña) y no caben en WinSCP: quedaron vacías.
    }
exportar-winscp-saltos = { $n ->
        [1] 1 conexión tiene más de un salto; WinSCP admite solo uno (el más cercano al servidor).
       *[other] { $n } conexiones tienen más de un salto; WinSCP admite solo uno (el más cercano al servidor).
    }
exportar-winsshterm-salto = { $n ->
        [1] 1 conexión tiene servidor de salto, que WinSSHTerm no guarda por conexión.
       *[other] { $n } conexiones tienen servidor de salto, que WinSSHTerm no guarda por conexión.
    }
exportar-winsshterm-tunel = { $n ->
        [1] 1 conexión tiene túneles, que WinSSHTerm no tiene.
       *[other] { $n } conexiones tienen túneles, que WinSSHTerm no tiene.
    }
queda-titulo = { -app } se detuvo
queda-mensagem = { -app } encontró un error y tuvo que cerrarse. Los detalles están en:

    { $arquivo }

    Si se cerró nada más abrirse, vuelve a abrirlo: la próxima vez dibujará la ventana de una forma más sencilla, que funciona con más tarjetas gráficas.
portatil-ja-aberto-titulo = { -app } ya está abierto
portatil-ja-aberto = Otra ventana de { -app } ya usa la carpeta { $pasta }. Usa esa ventana; dos a la vez borrarían los cambios guardados la una de la otra.
