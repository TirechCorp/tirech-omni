# Comandos ao conectar (M50).

comandos-herdar-de =
    Herdar ({ $nome }: { $passos ->
        [0] nenhum passo
        [1] 1 passo
       *[other] { $passos } passos
    })
comandos-herdar-nenhum = Herdar (nenhum)
comandos-herdar-resumo = Herdar ({ $nome }: { $resumo })
comandos-nenhum = Nenhum
comandos-resumo-senha = senha
comandos-passo-senha = Enviar a senha
comandos-aviso-parou = Os comandos ao conectar de { $aba } pararam no passo { $n } ({ $passo }): { $motivo }.
comandos-motivo-tela = a tela não parou em { $s } s
comandos-motivo-texto = o texto "{ $texto }" não apareceu em { $s } s
comandos-motivo-senha = o pedido de senha não apareceu em { $s } s
comandos-motivo-sem-senha = não há senha para enviar (a credencial está vazia ou o cofre continua trancado)
comandos-rodar-de-novo = Rodar os comandos ao conectar de novo
comandos-cancelar = Cancelar os comandos ao conectar
comandos-categoria = Comandos ao conectar
comandos-linha = Comandos
comandos-descricao-da-linha = Comandos que o app digita depois que a sessão abre, como sudo su - e a senha. Rodam de novo ao reconectar, se você quiser.
comandos-descricao-da-pasta = Comandos que as conexões da pasta rodam depois que a sessão abre, a não ser que tenham os seus.
comandos-editor-titulo = Comandos ao conectar — { $nome }
comandos-modo-titulo = Comandos
comandos-modo-proprios = Próprios
comandos-modo-nenhum = Nenhum
comandos-reconectar = Rodar de novo ao reconectar
comandos-adicionar = Adicionar comando
comandos-adicionar-senha = Adicionar senha
comandos-root-su = Virar root (sudo su -)
comandos-root-i = Virar root (sudo -i)
comandos-copiar-da-pasta = Copiar os da pasta
comandos-placeholder-comando = Comando, por exemplo sudo su -
comandos-placeholder-esperado = Texto, ou alternativas com |
comandos-espera-tela = Quando a tela parar
comandos-espera-texto = Quando aparecer o texto…
comandos-espera-logo = Logo
comandos-enviar-a-senha = Enviar a senha
comandos-senha-da-conexao = Senha da conexão
comandos-senha-item-removido = Item removido [{ $id }]
comandos-esperar-ate = Esperar até (s)
comandos-subir = Subir
comandos-descer = Descer
