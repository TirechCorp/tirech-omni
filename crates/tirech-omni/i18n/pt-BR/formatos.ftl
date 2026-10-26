# Formatos que mudam com o idioma: separador decimal, datas e o "há quanto tempo" do Início.
# Relógio de 24 horas nos dois idiomas.

formatos-decimal = ,

# $dia, $mes, $ano são números já com zeros à esquerda (texto).
formatos-data = { $dia }/{ $mes }/{ $ano }
formatos-data-hora = { $dia }/{ $mes }/{ $ano } { $hora }:{ $minuto }

inicio-quando-agora = agora mesmo
inicio-quando-min = há { $n } min
inicio-quando-horas = há { $n } h
inicio-quando-ontem = ontem
inicio-quando-dias = há { $n } dias
formato-csv-bitwarden = CSV do Bitwarden
formato-csv-keepassxc = CSV do KeePassXC
formato-json-tirech = JSON do { -app }
formato-json-bitwarden = JSON do Bitwarden
formato-unidade-dias = { $n ->
        [1] dia
       *[other] dias
    }
formato-faixa-alguns-segundos = alguns segundos
formato-faixa-cerca = cerca de { $n } { $unidade }
formato-faixa-intervalo = { $de }–{ $ate } { $unidade }
formato-falta = Falta { $corpo }
formato-faltam = Faltam { $corpo }
