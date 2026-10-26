# Assistentes de IA no servidor remoto (M46): Claude Code, Codex e OpenCode.

# As propriedades da conexão.
ia-remoto-categoria = Assistentes de IA
ia-remoto-pasta = Pasta
ia-remoto-pasta-descricao = Onde os assistentes abrem nesta conexão. Vazio: a pasta do terminal, a pasta remota padrão ou a pasta pessoal.
ia-remoto-instalar-por = Instalar por
ia-remoto-instalar-por-descricao = Como instalar os assistentes nesta conexão. Automático segue as Configurações.
ia-remoto-argumentos-claude = Argumentos do Claude Code
ia-remoto-argumentos-codex = Argumentos do Codex
ia-remoto-argumentos-opencode = Argumentos do OpenCode
ia-remoto-argumentos-descricao = Vão depois do nome do assistente, como você escreve. Vazio: valem os das Configurações.

# Os menus.
ia-remoto-menu = Assistentes de IA
ia-remoto-abrir = Abrir { $nome }
ia-remoto-instalar = Instalar { $nome }…
ia-remoto-verificando = { $nome } (verificando…)
ia-remoto-nao-verificou = { $nome } (não deu para verificar)
ia-remoto-outra-pasta = Abrir em outra pasta…
ia-remoto-entrar = Entrar
ia-remoto-entrar-claude = Claude Code (token sem navegador)
ia-remoto-entrar-codex = Codex (com código)
ia-remoto-entrar-opencode = OpenCode
ia-remoto-verificar = Verificar de novo
ia-remoto-botao-dica = Assistentes de IA ({ $atalho })

# Avisos.
ia-remoto-fora-do-ssh = Abra uma aba SSH para usar os assistentes de IA.
ia-remoto-sem-senha = Esta conexão pede uma senha que não está no cofre. Abra uma aba dela primeiro.
ia-remoto-resposta-ilegivel = O servidor respondeu algo que o app não entendeu.
ia-remoto-nao-verificou-erro = Não deu para verificar os assistentes: { $erro }
ia-remoto-porta-em-uso = A porta 1455 está em uso aqui. Use Assistentes de IA › Entrar › Codex (com código).

# O diálogo de instalar.
ia-remoto-instalar-titulo = Instalar { $nome } em { $conexao }
ia-remoto-instalar-texto = O app roda o comando abaixo numa aba nova, sem sudo. A instalação fica na sua pasta pessoal no servidor.
ia-remoto-metodo-nativo = Instalador oficial (recomendado)
ia-remoto-metodo-npm = npm
ia-remoto-metodo-homebrew = Homebrew
ia-remoto-metodo-falta = { $metodo } — falta o { $falta }
ia-remoto-instalar-comando = Comando
ia-remoto-instalar-copiar = Copiar
ia-remoto-instalar-abrir-depois = Abrir quando terminar
ia-remoto-instalar-ok = Instalar
ia-remoto-falta-curl = O servidor não tem o curl. Escolha o npm ou instale o curl (por exemplo, sudo apt install curl).
ia-remoto-falta-tar = O servidor não tem o tar. Escolha o npm ou instale o tar.
ia-remoto-falta-bash = O servidor não tem o bash. Escolha o npm ou instale o bash.
ia-remoto-falta-npm = O servidor não tem o npm. Escolha o instalador oficial ou instale o Node com o npm.
ia-remoto-falta-brew = O servidor não tem o Homebrew. Escolha outro método.
ia-remoto-sem-node = O { $nome } pelo npm precisa do Node { $minimo } ou mais novo, e o servidor não tem o Node.
ia-remoto-falta-node-rodar = O { $nome } pelo npm precisa do Node { $minimo } ou mais novo para rodar. O servidor tem o Node { $tem }.
ia-remoto-falta-node-instalar = O npm pede o Node { $minimo }, mas o { $nome } instala e roda com o Node { $tem } (é só um aviso do npm).
ia-remoto-falta-npm-sem-permissao = A pasta global do npm não é sua; o app instala em ~/.local com --prefix.
ia-remoto-falta-alpine = No Alpine, o Claude Code precisa de "apk add bash curl libgcc libstdc++ ripgrep" (com root).
ia-remoto-falta-arquitetura = O instalador oficial não tem binário para { $arq }.

# As abas.
ia-remoto-aba-instalar = Instalar { $nome }
ia-remoto-aba-entrar = Entrar no { $nome }

# O fim da instalação.
ia-remoto-instalado = { $nome } instalado em { $conexao } ({ $versao }).
ia-remoto-instalado-sem-versao = { $nome } instalado em { $conexao }.
ia-remoto-abrir-agora = Abrir agora
ia-remoto-instalacao-falhou = A instalação do { $nome } não deu certo. Veja a aba "{ $aba }".
ia-remoto-tentar-de-novo = Tentar de novo

# O diálogo de outra pasta.
ia-remoto-pasta-titulo = Abrir em outra pasta
ia-remoto-pasta-ferramenta = Assistente
ia-remoto-pasta-nenhum = Nenhum assistente instalado nesta conexão
ia-remoto-pasta-caminho = Pasta no servidor
ia-remoto-pasta-lembrar = Lembrar para esta conexão
ia-remoto-pasta-abrir = Abrir
