# Comandos al conectar (M50).

comandos-herdar-de =
    Heredar ({ $nome }: { $passos ->
        [0] ningún paso
        [1] 1 paso
       *[other] { $passos } pasos
    })
comandos-herdar-nenhum = Heredar (ninguno)
comandos-herdar-resumo = Heredar ({ $nome }: { $resumo })
comandos-nenhum = Ninguno
comandos-resumo-senha = contraseña
comandos-passo-senha = Enviar la contraseña
comandos-aviso-parou = Los comandos al conectar de { $aba } se detuvieron en el paso { $n } ({ $passo }): { $motivo }.
comandos-motivo-tela = la pantalla no se estabilizó en { $s } s
comandos-motivo-texto = el texto "{ $texto }" no apareció en { $s } s
comandos-motivo-senha = la solicitud de contraseña no apareció en { $s } s
comandos-motivo-sem-senha = no hay contraseña que enviar (la credencial está vacía o la bóveda sigue bloqueada)
comandos-rodar-de-novo = Volver a ejecutar los comandos al conectar
comandos-cancelar = Cancelar los comandos al conectar
comandos-categoria = Comandos al conectar
comandos-linha = Comandos
comandos-descricao-da-linha = Comandos que la aplicación escribe después de que se abre la sesión, como sudo su - y la contraseña. Se vuelven a ejecutar al reconectar, si quieres.
comandos-descricao-da-pasta = Comandos que ejecutan las conexiones de la carpeta después de que se abre la sesión, salvo que tengan los suyos.
comandos-editor-titulo = Comandos al conectar — { $nome }
comandos-modo-titulo = Comandos
comandos-modo-proprios = Propios
comandos-modo-nenhum = Ninguno
comandos-reconectar = Volver a ejecutar al reconectar
comandos-adicionar = Agregar comando
comandos-adicionar-senha = Agregar contraseña
comandos-root-su = Convertirse en root (sudo su -)
comandos-root-i = Convertirse en root (sudo -i)
comandos-copiar-da-pasta = Copiar los de la carpeta
comandos-placeholder-comando = Comando, por ejemplo sudo su -
comandos-placeholder-esperado = Texto, o alternativas con |
comandos-espera-tela = Cuando la pantalla se estabilice
comandos-espera-texto = Cuando aparezca el texto…
comandos-espera-logo = Enseguida
comandos-enviar-a-senha = Enviar la contraseña
comandos-senha-da-conexao = Contraseña de la conexión
comandos-senha-item-removido = Elemento eliminado [{ $id }]
comandos-esperar-ate = Esperar hasta (s)
comandos-subir = Subir
comandos-descer = Bajar
