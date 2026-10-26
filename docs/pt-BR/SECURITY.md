# Segurança

## Como relatar uma falha

Não abra issue pública. Use o relato privado do GitHub: na aba **Security** do repositório,
**Report a vulnerability** (ou
[este link](https://github.com/TirechCorp/tirech-omni/security/advisories/new)). Falhas na
lib (terminal, SSH, arquivos, VPNs) podem ir direto para o
[Tomni UI](https://github.com/TirechCorp/tomni-ui/security/advisories/new).

Mande o que for possível: a versão ou o commit, o sistema, como reproduzir e o impacto que
você vê. Não mande senhas, chaves nem arquivos de configuração reais. Respondemos em até 7 dias
e combinamos com você a data da divulgação depois da correção. Quem relatar recebe o crédito
no aviso, se quiser.

## Versões cobertas

O projeto ainda está na série 0.x: as correções saem só na versão mais recente da `main`.

## O modelo do app

O Tirech Omni salva as senhas, como os gerenciadores de conexões costumam fazer. Isso é uma
escolha do produto, não uma falha. O que ele promete:

- **Cofre**: o `vault.json` é cifrado com AES-256-GCM. Sem senha mestra (o padrão), a chave
  fica no `vault.key`, na mesma pasta, e protege contra quem lê só o `vault.json` (uma cópia,
  um backup, um sincronizador). Com a senha mestra, a chave sai dela pelo Argon2id, não fica
  gravada, e o `vault.json` sozinho não abre sem a senha. Cada anexo das notas é cifrado
  com AES-256-GCM e uma chave própria, guardada no cofre; os logs cifrados usam uma chave
  pública cuja privada fica no cofre.
- **Quem roda com o seu usuário** no computador pode ler o que o app lê. O app não tenta se
  defender de um programa malicioso rodando com o mesmo usuário.
- **Portas e canais locais**:
  - o MCP usa um socket Unix 0600 numa pasta 0700, ou um pipe do Windows com dono;
  - o agente SSH usa um socket só do usuário (ou o pipe e a janela do Pageant, no Windows);
  - a ponte SOCKS5 de cada VPN escuta só em `127.0.0.1`, em porta aleatória, e exige usuário e
    senha aleatórios de 128 bits;
  - os túneis `-L` e as portas encaminhadas abrem em `127.0.0.1` e ficam abertos a qualquer
    programa local, como no `ssh` do OpenSSH.
- **IA**: o texto que vai para um modelo passa por uma redação de segredos, que tem limites
  descritos no manual ("O que a redação pega e o que não pega"). Um assistente ligado pelo MCP
  só faz o que as permissões da conexão deixam, e cada pedido fica na auditoria.

Interessam em especial: ler o cofre ou a chave sem a senha mestra quando ela está definida,
acesso de outro usuário ou de outra máquina a qualquer canal acima, um assistente passando das
permissões da conexão e segredos que vazam para logs, para a IA ou para arquivos sem cifra.
