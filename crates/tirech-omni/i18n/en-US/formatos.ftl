# Formats that change with the language: decimal separator, dates and the Home "time ago".
# 24-hour clock in both languages.

formatos-decimal = .

# $dia, $mes, $ano are numbers already padded with zeros (text).
formatos-data = { $mes }/{ $dia }/{ $ano }
formatos-data-hora = { $mes }/{ $dia }/{ $ano } { $hora }:{ $minuto }

inicio-quando-agora = just now
inicio-quando-min = { $n } min ago
inicio-quando-horas = { $n } h ago
inicio-quando-ontem = yesterday
inicio-quando-dias = { $n } days ago
formato-csv-bitwarden = Bitwarden CSV
formato-csv-keepassxc = KeePassXC CSV
formato-json-tirech = { -app } JSON
formato-json-bitwarden = Bitwarden JSON
formato-unidade-dias = { $n ->
        [1] day
       *[other] days
    }
formato-faixa-alguns-segundos = a few seconds
formato-faixa-cerca = about { $n } { $unidade }
formato-faixa-intervalo = { $de }–{ $ate } { $unidade }
formato-falta = { $corpo } left
formato-faltam = { $corpo } left
