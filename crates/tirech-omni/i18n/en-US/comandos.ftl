# Commands on connect (M50).

comandos-herdar-de =
    Inherit ({ $nome }: { $passos ->
        [0] no steps
        [1] 1 step
       *[other] { $passos } steps
    })
comandos-herdar-nenhum = Inherit (none)
comandos-herdar-resumo = Inherit ({ $nome }: { $resumo })
comandos-nenhum = None
comandos-resumo-senha = password
comandos-passo-senha = Send the password
comandos-aviso-parou = The commands on connect of { $aba } stopped at step { $n } ({ $passo }): { $motivo }.
comandos-motivo-tela = the screen did not settle in { $s } s
comandos-motivo-texto = the text "{ $texto }" did not appear in { $s } s
comandos-motivo-senha = the password prompt did not appear in { $s } s
comandos-motivo-sem-senha = there is no password to send (the credential is empty or the vault is still locked)
comandos-rodar-de-novo = Run the commands on connect again
comandos-cancelar = Cancel the commands on connect
comandos-categoria = Commands on connect
comandos-linha = Commands
comandos-descricao-da-linha = Commands the app types after the session opens, such as sudo su - and the password. They run again on reconnect, if you want.
comandos-descricao-da-pasta = Commands the connections in the folder run after the session opens, unless they have their own.
comandos-editor-titulo = Commands on connect — { $nome }
comandos-modo-titulo = Commands
comandos-modo-proprios = Own
comandos-modo-nenhum = None
comandos-reconectar = Run again on reconnect
comandos-adicionar = Add command
comandos-adicionar-senha = Add password
comandos-root-su = Become root (sudo su -)
comandos-root-i = Become root (sudo -i)
comandos-copiar-da-pasta = Copy the folder's
comandos-placeholder-comando = Command, for example sudo su -
comandos-placeholder-esperado = Text, or alternatives with |
comandos-espera-tela = When the screen settles
comandos-espera-texto = When the text appears…
comandos-espera-logo = Right away
comandos-enviar-a-senha = Send the password
comandos-senha-da-conexao = Connection password
comandos-senha-item-removido = Removed item [{ $id }]
comandos-esperar-ate = Wait up to (s)
comandos-subir = Move up
comandos-descer = Move down
