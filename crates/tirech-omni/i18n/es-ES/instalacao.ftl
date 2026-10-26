# Instalar en el equipo (M65): errores de las operaciones de instalación.
instalacao-erro-em-uso = La aplicación se está ejecutando desde la copia instalada; solo se puede eliminar desinstalándola.
instalacao-erro-sem-copia = Este elemento necesita la copia instalada. Marca también "Copiar al equipo".
instalacao-erro-sem-pasta = No se encontró la carpeta de instalación ni el ejecutable de la aplicación.
instalacao-erro-sem-area = Este entorno no tiene carpeta de escritorio.
instalacao-erro-painel = Este entorno no permite que la aplicación se ancle sola al panel.
instalacao-erro-sem-bundle = La aplicación no se está ejecutando desde dentro de un paquete .app.
instalacao-erro-json = No se pudo editar la configuración del panel: { $erro }

# El diálogo "Instalar en el equipo" (M65).
instalar-titulo = Instalar en el equipo
instalar-titulo-desinstalar = Desinstalar del equipo
instalar-introducao = Elige qué deja { -app } en este equipo. Cada elemento se aplica por separado: si uno falla, los demás siguen.
instalar-introducao-desinstalar = Lo que esté desmarcado se quita de este equipo.
instalar-aplicar = Aplicar
instalar-aplicando = Aplicando…
instalar-desinstalar = Desinstalar…
instalar-apagar-dados = Eliminar también mis datos (conexiones, bóveda, notas, registros)
instalar-apagar-dados-pasta = Carpeta de datos: { $pasta }
instalar-confirmar-titulo = ¿Eliminar tus datos?
instalar-confirmar-texto = Las conexiones, la bóveda, las notas y los registros de { $pasta } se eliminarán y no se podrán recuperar. La aplicación se cierra a continuación.
instalar-confirmar-apagar = Eliminar todo y desinstalar

# Los elementos.
instalar-item-copia = Copiar al equipo
instalar-item-area = Acceso directo en el escritorio
instalar-item-menu-windows = Acceso directo en el menú Inicio
instalar-item-menu-linux = Acceso directo en el menú de aplicaciones
instalar-item-barra-windows = Anclar a la barra de tareas
instalar-item-barra-linux = Anclar al panel
instalar-item-barra-macos = Mantener en el Dock
instalar-inicio-na-bandeja = Quedarse solo en la bandeja, sin abrir la ventana
instalar-item-inicio = Abrir al iniciar el sistema
instalar-item-aplicativos = Aparecer en Aplicaciones instaladas

# Lo que cada elemento muestra debajo del nombre, cuando no es una ruta.
instalar-desc-registro = Registro de Windows: { $chave }
instalar-desc-barra-windows = Windows solo permite que lo ancle la propia persona; la aplicación muestra el paso.
instalar-desc-barra-painel = Panel de { $painel }
instalar-desc-barra-dock = Dock de macOS
instalar-desc-copia-em-uso = Esta es la copia que está abierta ahora.

# Por qué un elemento no está disponible ahora.
instalar-motivo-sem-area = Este entorno no tiene carpeta de escritorio.
instalar-motivo-precisa-da-copia = Necesita la copia en el equipo.
instalar-motivo-painel = Este entorno no permite que la aplicación se ancle sola; arrastra { -app } del menú al panel.
instalar-motivo-sem-bundle = Solo funciona con la aplicación abierta desde dentro del paquete .app.
instalar-motivo-barra-precisa-do-menu = Anclar necesita el acceso directo en el menú.

# El resultado.
instalar-resultado-titulo-ok = Instalación aplicada
instalar-resultado-titulo-aviso = Algunos elementos no se aplicaron
instalar-resultado-ok = Se aplicó lo que elegiste.
instalar-resultado-nada = Nada que hacer: el equipo ya está como pediste.
instalar-resultado-falha = { $item }: { $erro }
instalar-passo-fixar-windows = Para anclar a la barra de tareas: abre el menú Inicio, busca { -app }, haz clic con el botón derecho y elige Anclar a la barra de tareas.
instalar-passo-fixar-windows-depois = Para anclar a la barra de tareas: abre la versión instalada, haz clic con el botón derecho en su icono de la barra y elige Anclar a la barra de tareas.
instalar-passo-desafixar-windows = Para quitarla de la barra de tareas: haz clic con el botón derecho en el icono y elige Desanclar de la barra de tareas.
instalar-passo-dock = Para quitarla del Dock: arrastra el icono de { -app } fuera de él.
instalar-abrir-copia = Abrir la versión instalada
instalar-erro-abrir-copia = No se pudo abrir la versión instalada: { $erro }
instalar-erro-desinstalar = No se pudo programar la eliminación de las carpetas: { $erro }

# El resumen de la página de Configuración.
instalar-resumo-instalado = Instalado en { $pasta }
instalar-resumo-parcial = Instalado en parte
instalar-resumo-nao = No instalado
