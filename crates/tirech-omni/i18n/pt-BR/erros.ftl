# Mensagens de erro e textos de áreas pequenas.
abrir-log-falhou = Não consegui abrir o log { $caminho }: { $erro }
abrir-conectado = Conectado a { $nome }.
anexo-io = Não foi possível ler ou gravar o anexo: { $erro }.
anexo-sem-espaco = Não há espaço no disco: o anexo precisa de { $precisa } e há { $livre } livres.
anexo-corrompido = O anexo está corrompido ou incompleto.
anexo-ausente = O arquivo deste anexo ainda não está nesta máquina.
anexo-cancelado = Cancelado.
anexo-sem-aleatorio = sem aleatório: { $erro }
anexo-invalido = anexo inválido
anexo-ja-existe = o anexo já existe
anexo-nao-cifrou = não consegui cifrar o anexo
anexo-blocos-demais = o anexo tem blocos demais
armazenamento-caminho-absoluto = o caminho da pasta precisa ser absoluto
armazenamento-pasta-nao-vazia = a pasta não está vazia e não tem dados do { -app }
armazenamento-corrompido-copia = O arquivo { $o_que } estava corrompido; guardei uma cópia em { $copia }.
armazenamento-corrompido-sem-copia = O arquivo { $o_que } estava corrompido e não consegui guardar uma cópia: { $erro }.
armazenamento-conexoes-corrompido-copia = O arquivo de conexões estava corrompido; guardei uma cópia em { $copia }.
armazenamento-conexoes-corrompido-sem-copia = O arquivo de conexões estava corrompido e não consegui guardar uma cópia: { $erro }.
recentes-o-que = de recentes
agente-embutido-nao-iniciou = Não foi possível iniciar o agente embutido ({ $erro }). O app usa o agente do sistema.
agente-endereco-invalido = O endereço do agente precisa ser um caminho completo: { $endereco }
agente-pasta-curta-erro = Não consegui preparar a pasta { $pasta } para o socket do agente: { $erro }
agente-pasta-curta-insegura = A pasta { $pasta } já existe e não é só do seu usuário; o socket do agente não será criado nela.
agente-embutido-parado = o agente embutido não está rodando
agente-um-programa-pageant = Um programa (pela janela do Pageant)
agente-um-programa = Um programa
agente-pid = (pid { $pid })
espacos-faltante = { $n ->
        [1] 1 conexão deste espaço não existe mais: { $lista }
       *[other] { $n } conexões deste espaço não existem mais: { $lista }
    }
espacos-nome-invalido = O nome não pode ficar vazio nem ter / \ : * ? " < > |
espacos-ja-existe = Já existe um item com esse nome nesta pasta.
espacos-nao-vazia = A pasta não está vazia.
espacos-nao-existe = O espaço ou a pasta não existe mais.
espacos-dentro-de-si-mesma = Uma pasta não pode ir para dentro dela mesma.
espacos-invalido = O arquivo do espaço não serve: { $motivo }
csv-aspas-sem-fechar = Aspas sem fechar na linha { $linha }
csv-formato-desconhecido = Formato de CSV desconhecido: esperava o do Bitwarden ou o do KeePassXC
cofre-json-nao-consegui-ler-tirech = Não consegui ler o JSON do { -app }: { $erro }.
cofre-json-nao-consegui-ler = Não consegui ler o JSON: { $erro }.
cofre-json-nao-e-cofre = Esse JSON não é um cofre do { -app }.
cofre-json-versao-nova = Esse cofre foi exportado por uma versão mais nova do { -app }.
cofre-json-formato-desconhecido = Formato de JSON desconhecido: esperava o do { -app } ou o do Bitwarden.
bitwarden-bandeira = Bandeira
bitwarden-numero = Número
bitwarden-validade = Validade
bitwarden-codigo = Código
bitwarden-tratamento = Tratamento
bitwarden-nome = Nome
bitwarden-nome-do-meio = Nome do meio
bitwarden-sobrenome = Sobrenome
bitwarden-empresa = Empresa
bitwarden-email = E-mail
bitwarden-telefone = Telefone
bitwarden-endereco = Endereço
bitwarden-endereco-2 = Endereço 2
bitwarden-endereco-3 = Endereço 3
bitwarden-cidade = Cidade
bitwarden-estado = Estado
bitwarden-cep = CEP
bitwarden-pais = País
bitwarden-ssn = SSN
bitwarden-passaporte = Passaporte
bitwarden-carteira = Carteira de motorista
bitwarden-chave-privada = Chave privada
bitwarden-chave-publica = Chave pública
bitwarden-impressao-digital = Impressão digital
bitwarden-url = URL { $n }
bitwarden-nao-consegui-ler = Não consegui ler o JSON do Bitwarden: { $erro }.
bitwarden-cifrado = Esse JSON do Bitwarden está cifrado. Exporte de novo no formato .json (sem a opção cifrada).
bitwarden-desconhecidos = { $n ->
        [1] 1 item de tipo desconhecido ficou de fora.
       *[other] { $n } itens de tipo desconhecido ficaram de fora.
    }
bitwarden-vinculados = Campos vinculados do Bitwarden foram ignorados.
ssh-config-proxyjump = ProxyJump com { $n } saltos: ficou só o último ({ $ultimo }).
ssh-config-proxycommand = ProxyCommand não é suportado e foi ignorado.
ssh-config-socks-remoto = { $diretiva } { $valor }: o SOCKS remoto (sem destino) não é suportado e foi ignorado.
ssh-config-nao-entendi = { $diretiva } { $valor }: não entendi e foi ignorado.
ssh-config-socket-de-origem = { $diretiva } { $valor }: um socket local como origem não é suportado e foi ignorado.
ssh-config-nao-entendi-porta = { $diretiva } { $valor }: não entendi a porta e foi ignorado.
ssh-config-todas-as-interfaces = { $diretiva } { $origem } …: o endereço vira "todas as interfaces".
ssh-config-r-para-socket = { $diretiva } { $valor }: o -R para um socket não é suportado e foi ignorado.
ssh-config-destino-incompleto = { $diretiva } { $valor }: o destino precisa de servidor e porta e foi ignorado.
ssh-config-problema = { $diretiva } { $valor }: { $problema } Foi ignorado.
ssh-config-aviso-da-conexao = { $nome }: { $aviso }
config-fora-da-faixa = { $nome } fora da faixa ({ $min } a { $max }); ajustado para { $preso }.
config-atalho-invalido = atalhos.{ $id }: não entendi "{ $ruim }"; voltou ao padrão.
config-json-invalido = O arquivo não é um JSON válido: { $erro }.
config-json-nao-e-objeto = O arquivo não é um objeto JSON de configurações.
config-ilegivel-copia = As configurações estavam ilegíveis e voltaram ao padrão; a cópia está em { $copia }.
config-ilegivel-sem-copia = As configurações estavam ilegíveis e voltaram ao padrão; não consegui guardar uma cópia: { $erro }.
config-invalido-padrao = { $nome } inválido; voltou ao padrão ({ $valor_padrao }).
aws-perfil-sem-chaves = O perfil { $perfil } ficou de fora: não tem chaves de acesso.
atalho-terminal-deixa-de-receber = O terminal deixa de receber { $atalho }.
importar-nao-suportada-conexao = { $n ->
        [1] 1 conexão { $complemento } não é suportada e foi ignorada.
       *[other] { $n } conexões { $complemento } não são suportadas e foram ignoradas.
    }
importar-nao-suportada-sessao = { $n ->
        [1] 1 sessão { $complemento } não é suportada e foi ignorada.
       *[other] { $n } sessões { $complemento } não são suportadas e foram ignoradas.
    }
importar-com-protocolo = com protocolo { $nome }
importar-de-codigo = de código { $n }
importar-proxy-importado = Proxy importado ({ $host }:{ $porta })
importar-ppk = Há chaves no formato do PuTTY (.ppk). Se o agente não as aceitar, converta com o `puttygen` ou importe a chave no cofre.
importar-proxy-sem-suporte = Há sessões com proxy SOCKS4, Telnet ou comando; só SOCKS5 e HTTP são importados.
importar-importei = { $n ->
        [1] Importei 1 conexão
       *[other] Importei { $n } conexões
    }
importar-trocadas = { $n ->
        [1] 1 trocada
       *[other] { $n } trocadas
    }
importar-puladas = { $n ->
        [1] 1 pulada
       *[other] { $n } puladas
    }
importar-cofre-bloqueado-senhas = { $n ->
        [1] O cofre está bloqueado: 1 senha ficou de fora e será pedida ao conectar.
       *[other] O cofre está bloqueado: { $n } senhas ficaram de fora e serão pedidas ao conectar.
    }
cripto-winscp-formato = A senha do WinSCP está num formato que não reconheço.
cripto-mremoteng-sem-base64 = A senha do mRemoteNG não está em base64.
cripto-mremoteng-curta = A senha do mRemoteNG é curta demais.
cripto-mremoteng-rodadas = O mRemoteNG pede um número de rodadas fora do aceitável.
cripto-nonce-torto = Nonce torto.
cripto-mremoteng-senha-mestra = A senha-mestra do mRemoteNG não abre este arquivo.
cripto-mremoteng-nao-cifrou = Não consegui cifrar a senha para o mRemoteNG.
cripto-mremoteng-curta-ou-torta = A senha do mRemoteNG é curta demais ou está torta.
cripto-chave-ou-iv = Chave ou IV tortos.
importar-filezilla-nao-e-xml = Esse XML não é um sitemanager.xml do FileZilla.
importar-filezilla-cifradas = { $n ->
        [1] 1 senha do FileZilla está cifrada com a chave-mestra dele e não foi importada.
       *[other] { $n } senhas do FileZilla estão cifradas com a chave-mestra dele e não foram importadas.
    }
importar-moba-tipo-arquivo = arquivo
importar-moba-tipo-navegador = navegador
importar-moba-sem-sessoes = Esse arquivo não tem sessões do MobaXterm (seções [Bookmarks…]).
importar-moba-senhas-no-registro = O MobaXterm guarda as senhas no registro do Windows, cifradas; as conexões vêm sem senha.
importar-mremoteng-modo = O modo { $nome } não é suportado.
importar-mremoteng-parece-cifrado = O arquivo parece cifrado por inteiro (FullFileEncryption do mRemoteNG). Exporte de novo, sem essa opção.
importar-mremoteng-nao-e-xml = Esse XML não é um confCons.xml do mRemoteNG.
importar-mremoteng-cifrado = O arquivo está cifrado por inteiro (FullFileEncryption do mRemoteNG). Exporte de novo, sem essa opção.
importar-mremoteng-modo-sem-suporte = As senhas estão cifradas no modo { $nome } do mRemoteNG, que não é suportado; as conexões vêm sem senha.
importar-mremoteng-senha-recusada = A senha-mestra informada não abre o arquivo.
importar-mremoteng-ilegiveis = { $n ->
        [1] 1 senha do mRemoteNG não pôde ser decifrada.
       *[other] { $n } senhas do mRemoteNG não puderam ser decifradas.
    }
importar-putty-reg-sem-sessoes = Esse .reg não tem sessões do PuTTY nem do KiTTY.
importar-putty-pasta-sem-sessoes = Essa pasta não tem sessões do PuTTY nem do KiTTY.
importar-winscp-sem-sessoes = Esse arquivo não tem sessões do WinSCP (seções [Sessions\…]).
importar-winscp-workspaces = Os workspaces do WinSCP não são importados (só as sessões).
importar-winscp-scp = { $n ->
        [1] 1 sessão usava SCP e virou SFTP (o servidor precisa ter o subsistema sftp).
       *[other] { $n } sessões usavam SCP e viraram SFTP (o servidor precisa ter o subsistema sftp).
    }
importar-winscp-ilegiveis = { $n ->
        [1] 1 senha do WinSCP não pôde ser recuperada (arquivo com senha-mestra do WinSCP, ou de outra sessão).
       *[other] { $n } senhas do WinSCP não puderam ser recuperadas (arquivo com senha-mestra do WinSCP, ou de outra sessão).
    }
importar-nativo-json = Não consegui ler o JSON do { -app }: { $erro }.
importar-nativo-e-cofre = Esse JSON é um cofre do { -app }, sem conexões. Importe-o pela página Cofre.
importar-nativo-nao-e-exportacao = Esse JSON não é uma exportação de conexões do { -app }.
importar-nativo-conexoes = Não consegui ler as conexões do JSON: { $erro }.
importar-nativo-versao-nova = Essa exportação é de uma versão mais nova do { -app }.
importar-winsshterm-nao-e-xml = Esse XML não é um connections.xml do WinSSHTerm.
importar-winsshterm-senhas = As senhas do WinSSHTerm ficam cifradas no arquivo `key` dele e não foram importadas; digite-as no { -app }.
importar-winsshterm-proxy = O proxy do tipo { $tipo } não é suportado (só SOCKS5 e HTTP) e não foi importado.
importar-xml-atributo = Atributo torto no XML: { $erro }.
importar-xml-ler = Não consegui ler o XML: { $erro }.
importar-xml-texto = Texto torto no XML: { $erro }.
importar-xml-aninhado = O XML tem elementos aninhados demais.
importar-xml-fecha-sem-abrir = O XML fecha um elemento que não abriu.
importar-xml-termina-aberto = O XML termina com um elemento aberto.
importar-xml-vazio = O arquivo não tem nenhum elemento XML.
exportar-resumo = { $n ->
        [1] 1 conexão exportada.
       *[other] { $n } conexões exportadas.
    }
exportar-chave-do-cofre = { $n ->
        [1] 1 conexão usa uma chave do cofre; com o { -app } aberto, o agente (M44) a oferece ao outro programa.
       *[other] { $n } conexões usam chaves do cofre; com o { -app } aberto, o agente (M44) as oferece ao outro programa.
    }
exportar-fora-do-formato = { $n ->
        [1] 1 conexão { $tipo } não cabe no formato do { $programa } e ficou de fora.
       *[other] { $n } conexões { $tipo } não cabem no formato do { $programa } e ficaram de fora.
    }
exportar-chaves-cofre-bloqueado = O cofre está bloqueado: as chaves dele não foram gravadas e as conexões que as usam ficaram sem arquivo de chave.
exportar-chaves-gravadas = As chaves do cofre foram gravadas, sem passphrase, em { $pasta }; guarde a pasta como guardaria a chave.
exportar-chave-sumiu = Uma conexão usa uma chave que não está mais no cofre.
exportar-chave-erro = A chave «{ $titulo }»: { $erro }
exportar-chave-so-publica = A chave «{ $titulo }» só tem a parte pública e não foi gravada.
exportar-chave-sem-formato = A chave «{ $titulo }» não existe no formato { $formato } e não foi gravada.
exportar-nao-criou-pasta = Não deu para criar { $pasta }: { $erro }
exportar-filezilla-bucket = { $n ->
        [1] 1 conexão S3 perde o bucket, que o FileZilla pede na pasta remota: abra-o ao conectar.
       *[other] { $n } conexões S3 perdem o bucket, que o FileZilla pede na pasta remota: abra-o ao conectar.
    }
exportar-filezilla-salto = { $n ->
        [1] 1 conexão tem servidor de salto, que o FileZilla não tem.
       *[other] { $n } conexões têm servidor de salto, que o FileZilla não tem.
    }
exportar-moba-salto = { $n ->
        [1] 1 conexão tem servidor de salto, que o `.mxtsessions` não leva.
       *[other] { $n } conexões têm servidor de salto, que o `.mxtsessions` não leva.
    }
exportar-moba-tunel = { $n ->
        [1] 1 conexão tem túneis ou proxy, que o `.mxtsessions` não leva.
       *[other] { $n } conexões têm túneis ou proxy, que o `.mxtsessions` não leva.
    }
exportar-mremoteng-falhas = { $n ->
        [1] 1 senha não foi cifrada para o mRemoteNG e ficou sem senha.
       *[other] { $n } senhas não foram cifradas para o mRemoteNG e ficaram sem senha.
    }
exportar-mremoteng-chave = O mRemoteNG não tem campo de chave: ele usa a da sessão do PuTTY de mesmo nome. Exporte também o `.reg` do PuTTY (Exportar ▸ PuTTY) e importe-o no PuTTY.
exportar-mremoteng-rede = { $n ->
        [1] 1 conexão tem proxy ou servidor de salto, que o mRemoteNG não guarda por conexão.
       *[other] { $n } conexões têm proxy ou servidor de salto, que o mRemoteNG não guarda por conexão.
    }
exportar-nativo-cofre-bloqueado = O cofre está bloqueado: as senhas não foram incluídas.
exportar-nativo-anexos = { $n ->
        [1] 1 anexo de nota não está nesta máquina e ficou de fora.
       *[other] { $n } anexos de nota não estão nesta máquina e ficaram de fora.
    }
exportar-putty-salto = { $n ->
        [1] 1 conexão tem servidor de salto, que o PuTTY não guarda na sessão.
       *[other] { $n } conexões têm servidor de salto, que o PuTTY não guarda na sessão.
    }
exportar-putty-tunel = { $n ->
        [1] 1 túnel para socket Unix não cabe no PuTTY e ficou de fora.
       *[other] { $n } túneis para socket Unix não cabem no PuTTY e ficaram de fora.
    }
exportar-ssh-config-proxy = { $n ->
        [1] 1 conexão sai por um proxy de rede, que o ssh_config deste export não leva.
       *[other] { $n } conexões saem por um proxy de rede, que o ssh_config deste export não leva.
    }
exportar-ssh-config-tunel = { $n ->
        [1] 1 túnel não coube em diretivas do OpenSSH e ficou de fora.
       *[other] { $n } túneis não couberam em diretivas do OpenSSH e ficaram de fora.
    }
exportar-winscp-grandes = { $n ->
        [1] 1 senha passa de 255 bytes (usuário, servidor e senha) e não cabe no WinSCP: ficou sem senha.
       *[other] { $n } senhas passam de 255 bytes (usuário, servidor e senha) e não cabem no WinSCP: ficaram sem senha.
    }
exportar-winscp-saltos = { $n ->
        [1] 1 conexão tem mais de um salto; o WinSCP leva um só (o mais perto do servidor).
       *[other] { $n } conexões têm mais de um salto; o WinSCP leva um só (o mais perto do servidor).
    }
exportar-winsshterm-salto = { $n ->
        [1] 1 conexão tem servidor de salto, que o WinSSHTerm não guarda por conexão.
       *[other] { $n } conexões têm servidor de salto, que o WinSSHTerm não guarda por conexão.
    }
exportar-winsshterm-tunel = { $n ->
        [1] 1 conexão tem túneis, que o WinSSHTerm não tem.
       *[other] { $n } conexões têm túneis, que o WinSSHTerm não tem.
    }
queda-titulo = O { -app } parou
queda-mensagem = O { -app } encontrou um erro e precisou fechar. Os detalhes ficaram em:

    { $arquivo }

    Se ele fechou logo ao abrir, abra de novo: na próxima vez ele desenha a janela de um jeito mais simples, que funciona com mais placas de vídeo.
portatil-ja-aberto-titulo = O { -app } já está aberto
portatil-ja-aberto = Outra janela do { -app } já usa a pasta { $pasta }. Use essa janela; duas ao mesmo tempo apagariam as gravações uma da outra.
