# Asistentes de IA en el servidor remoto (M46): Claude Code, Codex y OpenCode.

# Las propiedades de la conexión.
ia-remoto-categoria = Asistentes de IA
ia-remoto-pasta = Carpeta
ia-remoto-pasta-descricao = Dónde se abren los asistentes en esta conexión. Vacío: la carpeta del terminal, la carpeta remota predeterminada o la carpeta personal.
ia-remoto-instalar-por = Instalar con
ia-remoto-instalar-por-descricao = Cómo instalar los asistentes en esta conexión. Automático sigue la Configuración.
ia-remoto-argumentos-claude = Argumentos de Claude Code
ia-remoto-argumentos-codex = Argumentos de Codex
ia-remoto-argumentos-opencode = Argumentos de OpenCode
ia-remoto-argumentos-descricao = Van después del nombre del asistente, tal como los escribes. Vacío: se usan los de la Configuración.

# Los menús.
ia-remoto-menu = Asistentes de IA
ia-remoto-abrir = Abrir { $nome }
ia-remoto-instalar = Instalar { $nome }…
ia-remoto-verificando = { $nome } (comprobando…)
ia-remoto-nao-verificou = { $nome } (no se pudo comprobar)
ia-remoto-outra-pasta = Abrir en otra carpeta…
ia-remoto-entrar = Iniciar sesión
ia-remoto-entrar-claude = Claude Code (token sin navegador)
ia-remoto-entrar-codex = Codex (con código)
ia-remoto-entrar-opencode = OpenCode
ia-remoto-verificar = Volver a comprobar
ia-remoto-botao-dica = Asistentes de IA ({ $atalho })

# Avisos.
ia-remoto-fora-do-ssh = Abre una pestaña SSH para usar los asistentes de IA.
ia-remoto-sem-senha = Esta conexión pide una contraseña que no está en la bóveda. Abre primero una pestaña de ella.
ia-remoto-resposta-ilegivel = El servidor respondió algo que la aplicación no entendió.
ia-remoto-nao-verificou-erro = No se pudieron comprobar los asistentes: { $erro }
ia-remoto-porta-em-uso = El puerto 1455 está en uso aquí. Usa Asistentes de IA › Iniciar sesión › Codex (con código).

# El diálogo de instalar.
ia-remoto-instalar-titulo = Instalar { $nome } en { $conexao }
ia-remoto-instalar-texto = La aplicación ejecuta el comando de abajo en una pestaña nueva, sin sudo. La instalación queda en tu carpeta personal en el servidor.
ia-remoto-metodo-nativo = Instalador oficial (recomendado)
ia-remoto-metodo-npm = npm
ia-remoto-metodo-homebrew = Homebrew
ia-remoto-metodo-falta = { $metodo } — falta { $falta }
ia-remoto-instalar-comando = Comando
ia-remoto-instalar-copiar = Copiar
ia-remoto-instalar-abrir-depois = Abrir al terminar
ia-remoto-instalar-ok = Instalar
ia-remoto-falta-curl = El servidor no tiene curl. Elige npm o instala curl (por ejemplo, sudo apt install curl).
ia-remoto-falta-tar = El servidor no tiene tar. Elige npm o instala tar.
ia-remoto-falta-bash = El servidor no tiene bash. Elige npm o instala bash.
ia-remoto-falta-npm = El servidor no tiene npm. Elige el instalador oficial o instala Node con npm.
ia-remoto-falta-brew = El servidor no tiene Homebrew. Elige otro método.
ia-remoto-sem-node = { $nome } mediante npm necesita Node { $minimo } o posterior, y el servidor no tiene Node.
ia-remoto-falta-node-rodar = { $nome } mediante npm necesita Node { $minimo } o posterior para ejecutarse. El servidor tiene Node { $tem }.
ia-remoto-falta-node-instalar = npm pide Node { $minimo }, pero { $nome } se instala y se ejecuta con Node { $tem } (es solo una advertencia de npm).
ia-remoto-falta-npm-sem-permissao = La carpeta global de npm no es tuya; la aplicación instala en ~/.local con --prefix.
ia-remoto-falta-alpine = En Alpine, Claude Code necesita "apk add bash curl libgcc libstdc++ ripgrep" (como root).
ia-remoto-falta-arquitetura = El instalador oficial no tiene binario para { $arq }.

# Las pestañas.
ia-remoto-aba-instalar = Instalar { $nome }
ia-remoto-aba-entrar = Iniciar sesión en { $nome }

# El final de la instalación.
ia-remoto-instalado = { $nome } instalado en { $conexao } ({ $versao }).
ia-remoto-instalado-sem-versao = { $nome } instalado en { $conexao }.
ia-remoto-abrir-agora = Abrir ahora
ia-remoto-instalacao-falhou = La instalación de { $nome } no funcionó. Mira la pestaña "{ $aba }".
ia-remoto-tentar-de-novo = Intentar de nuevo

# El diálogo de otra carpeta.
ia-remoto-pasta-titulo = Abrir en otra carpeta
ia-remoto-pasta-ferramenta = Asistente
ia-remoto-pasta-nenhum = No hay ningún asistente instalado en esta conexión
ia-remoto-pasta-caminho = Carpeta en el servidor
ia-remoto-pasta-lembrar = Recordar para esta conexión
ia-remoto-pasta-abrir = Abrir
