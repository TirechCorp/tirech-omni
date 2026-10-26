# Los registros de sesión cifrados (M53): el formato y la lectura.

log-sem-aleatorio = sin fuente aleatoria: { $erro }
log-nao-cifrou = no se pudo cifrar el registro
log-chave-invalida = la clave pública de los registros cifrados no es válida
log-nao-e-do-app = el archivo no es un registro cifrado de la aplicación
log-alterado-no-meio = el registro cifrado está alterado o dañado
log-escrita-parou = la escritura del registro cifrado se detuvo después de un error de disco

log-erro-nao-e = El archivo no es un registro cifrado de la aplicación.
log-erro-versao-nova = Este registro lo escribió una versión más reciente de la aplicación.
log-erro-chave-ausente = La clave de este registro no está en la bóveda (id { $id }). Puede estar en la bóveda de otro equipo: combina las bóvedas en Bóveda > Combinar.
log-erro-alterado = El registro se alteró o está dañado a partir de aquí (segmento { $segmento }, entrada { $registro }). El texto anterior aparece abajo.
log-erro-io = { $erro }
log-erro-cancelado = Cancelado.

log-chave-titulo = Clave de los registros cifrados
log-chave-notas = Abre los registros cifrados de { -app }. Si eliminas este elemento, los registros cifrados con él ya no se abrirán.

log-sem-chave-publica = falta la clave pública de los registros cifrados; desbloquea la bóveda

log-filtro-cifrado = Registro cifrado
log-destrave-para-cifrar = Para cifrar los registros, desbloquea la bóveda.
log-nao-criou-a-chave = No se pudo crear la clave de los registros cifrados: { $erro }. Los registros vuelven a guardarse como texto.

# La página Registros.
logs-titulo = Registros
logs-descricao = Los registros de las sesiones del terminal, en texto y cifrados. Los cifrados se abren aquí con la clave de la bóveda.
logs-conexao = Conexión
logs-modificado = Modificado
logs-gravando = Grabando
logs-vazio = Aún no hay registros. Activa los registros en Configuración > Terminal.
logs-escolha-um = Elige un registro de la lista para leerlo.
logs-varios = { $n } registros elegidos.
logs-destrave = Desbloquea la bóveda para abrir este registro
logs-destravar = Desbloquear
logs-quebrar-linhas = Ajustar líneas
logs-copiar-tudo = Copiar todo
logs-buscar = Buscar en el registro
logs-buscar-dica = Buscar (Ctrl+F)
logs-diferenciar = Distinguir mayúsculas y minúsculas
logs-mostrar-tudo = Mostrar todo
logs-mostrando-o-fim = Mostrando el final del registro ({ $mostrado } de { $total }).
logs-sem-fim = Este registro terminó sin cerrarse: puede que la aplicación se cerrara de repente. El texto hasta ese punto aparece abajo.
logs-cortado = El final del registro estaba incompleto ({ $bytes } bytes) y se dejó fuera.
logs-lendo = Leyendo { $nome }…
logs-leitura-cancelada = Se canceló la lectura.
logs-atualizar-dica = Volver a leer la carpeta
logs-exportar = Exportar como texto…
logs-exportar-titulo = Exportar el registro como texto
logs-exportar-pasta-titulo = Exportar los registros como texto, ¿en qué carpeta?
logs-exportado-em = Registro exportado a { $arquivo }.
logs-exportados-em = { $n ->
    [one] 1 registro exportado a { $pasta }.
    *[other] { $n } registros exportados a { $pasta }.
    }
logs-exportados-com-avisos = { $n ->
    [one] 1 registro exportado, pero con advertencias: { $avisos }
    *[other] { $n } registros exportados, pero con advertencias: { $avisos }
    }
logs-nao-exportou = No se pudo exportar { $nome }: { $erro }
logs-abrir-pasta = Abrir la carpeta
logs-nao-abriu-a-pasta = No se pudo abrir { $caminho }: { $motivo }
logs-apagar-titulo = ¿Eliminar los registros?
logs-apagar-texto = { $n ->
    [one] ¿Eliminar 1 registro?
    *[other] ¿Eliminar { $n } registros?
    }
logs-apagar-aviso = La aplicación no puede recuperar los registros eliminados.
logs-apagar-gravando = { $n ->
    [one] 1 se está grabando y queda fuera.
    *[other] { $n } se están grabando y quedan fuera.
    }
logs-apagados = { $n ->
    [one] 1 registro eliminado.
    *[other] { $n } registros eliminados.
    }
logs-nao-apagou = No se pudo eliminar { $nome }: { $erro }
logs-so-gravando = Los registros elegidos se están grabando y no se pueden eliminar ahora.
logs-abrir-da-aba = Abrir el registro de esta sesión
logs-sem-pasta-de-logs = No se pudo crear la carpeta de los registros: { $erro }
logs-cifragem = Cifrar o descifrar
logs-cifrar = Cifrar los elegidos…
logs-decifrar = Descifrar los elegidos…
logs-cifrar-todos = Cifrar todos los de texto…
logs-converter = Convertir
logs-converter-titulo = Cifrar o descifrar los registros
logs-converter-cifrar-texto = { $n ->
    [one] ¿Cifrar 1 registro de texto ({ $tamanho })?
    *[other] ¿Cifrar { $n } registros de texto ({ $tamanho })?
    }
logs-converter-decifrar-texto = { $n ->
    [one] ¿Descifrar 1 registro ({ $tamanho })? El texto queda en el disco, sin protección.
    *[other] ¿Descifrar { $n } registros ({ $tamanho })? El texto queda en el disco, sin protección.
    }
logs-converter-apagar-originais = Eliminar los originales (de texto)
logs-converter-apagar-cifrados = Eliminar los cifrados
logs-converter-nada-cifrar = Aquí no hay ningún registro de texto que cifrar.
logs-converter-nada-decifrar = Entre los elegidos no hay ningún registro cifrado que descifrar.
logs-converter-so-gravando = Los registros se están grabando y no se pueden convertir ahora.
logs-convertendo = Convirtiendo los registros…
logs-cifrados-n = { $n ->
    [one] 1 registro cifrado.
    *[other] { $n } registros cifrados.
    }
logs-decifrados-n = { $n ->
    [one] 1 registro descifrado.
    *[other] { $n } registros descifrados.
    }
logs-convertidos-gravando = { $n ->
    [one] 1 quedó fuera por estar grabándose.
    *[other] { $n } quedaron fuera por estar grabándose.
    }
logs-convertidos-erros = { $n ->
    [one] 1 con error: { $detalhes }
    *[other] { $n } con error: { $detalhes }
    }
logs-convertidos-cancelado = Cancelado: los que faltaban quedaron como estaban.
