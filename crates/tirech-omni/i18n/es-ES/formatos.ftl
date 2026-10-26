# Formatos que cambian con el idioma: separador decimal, fechas y el "hace cuánto" de Inicio.
# Reloj de 24 horas en todos los idiomas.

formatos-decimal = ,

# $dia, $mes, $ano son números ya con ceros a la izquierda (texto).
formatos-data = { $dia }/{ $mes }/{ $ano }
formatos-data-hora = { $dia }/{ $mes }/{ $ano } { $hora }:{ $minuto }

inicio-quando-agora = ahora mismo
inicio-quando-min = hace { $n } min
inicio-quando-horas = hace { $n } h
inicio-quando-ontem = ayer
inicio-quando-dias = hace { $n } días
formato-csv-bitwarden = CSV de Bitwarden
formato-csv-keepassxc = CSV de KeePassXC
formato-json-tirech = JSON de { -app }
formato-json-bitwarden = JSON de Bitwarden
formato-unidade-dias = { $n ->
        [1] día
       *[other] días
    }
formato-faixa-alguns-segundos = unos segundos
formato-faixa-cerca = unos { $n } { $unidade }
formato-faixa-intervalo = { $de }–{ $ate } { $unidade }
formato-falta = Queda { $corpo }
formato-faltam = Quedan { $corpo }
