# Redes de clientes com o mesmo IP

> Para Nebula, OpenVPN, WireGuard ou Tailscale, use o perfil embutido; o contêiner não é
> necessário. Veja "Rede Nebula embutida", "Rede OpenVPN embutida", "Rede WireGuard embutida" e
> "Rede Tailscale embutida" no [manual](../../../docs/pt-BR/manual.md).

Você atende dois clientes, e as redes dos dois usam a mesma faixa: o `10.0.0.5` do cliente A
não é o `10.0.0.5` do cliente B. Uma máquina só não consegue estar nas duas VPNs ao mesmo
tempo, porque a tabela de rotas tem uma rota só para `10.0.0.0/24`.

## A solução recomendada

**Um contêiner por cliente.** Cada contêiner (Docker ou Podman) tem a própria pilha de rede,
então cada um tem a sua tabela de rotas. Dentro dele rodam a VPN do cliente e um proxy SOCKS5,
publicado só em `127.0.0.1` do seu computador. O `tirech-omni` escolhe o proxy pelo **perfil de
rede** da conexão: o `10.0.0.5` do grupo "Cliente A" sai pelo contêiner do A, e o do grupo
"Cliente B" pelo contêiner do B.

```
tirech-omni ──SOCKS5──> 127.0.0.1:1081 ─┐  contêiner rede-cliente-a ── VPN A ──> 10.0.0.5 (do A)
           ──SOCKS5──> 127.0.0.1:1082 ─┘  contêiner rede-cliente-b ── VPN B ──> 10.0.0.5 (do B)
```

O app não fala protocolo de VPN e não gerencia contêineres: ele só roda dois comandos que
você escolhe (um para subir, outro para descer) e conecta pelo proxy.

| Situação | O que usar |
|---|---|
| A VPN do cliente roda no Linux (WireGuard, OpenVPN, OpenConnect, FortiClient) | Contêiner, as receitas desta pasta |
| Sem Docker, mas com Linux | `ip netns`: [`ip-netns.md`](ip-netns.md) |
| A VPN só roda no Windows (cliente proprietário) | Uma VM: [`vm.md`](vm.md) |
| Você controla o concentrador da VPN | NAT para outra faixa lá, e não precisa de nada disto |
| Flatpak | Não serve: o sandbox não deixa subir contêineres nem namespaces |

## Passo a passo

1. **Suba a receita do cliente.** Copie a pasta da VPN dele (`wireguard/`, `openvpn/`,
   `openconnect/` ou `openfortivpn/`) para um lugar seu, ponha o arquivo de configuração
   dentro dela (veja o cabeçalho do `compose.yml`) e teste:

   ```sh
   docker compose up -d --wait      # espera o SOCKS5 responder, ou falha
   docker compose down
   ```

   Para o segundo cliente, copie a pasta outra vez e troque o `container_name` (por exemplo
   `rede-cliente-b`) e a porta publicada (`127.0.0.1:1082:1080`).

2. **Crie o perfil no app.** Em Ferramentas, **Redes**, **Novo**:

   | Propriedade | Valor |
   |---|---|
   | Nome | `Cliente A` |
   | Proxy | `SOCKS5`, host `127.0.0.1`, porta `1081` |
   | Comando para subir | `docker compose -f /caminho/cliente-a/compose.yml up -d --wait` |
   | Comando para descer | `docker compose -f /caminho/cliente-a/compose.yml down` |

   O botão **Testar** sobe a rede e confere o proxy.

3. **Ligue o grupo do cliente ao perfil.** Escolha o grupo na árvore e, na linha **Rede** das
   propriedades, o perfil `Cliente A`. Todas as conexões do grupo herdam; uma conexão que
   precise sair de outro jeito escolhe a própria rede (`Direta` ou outro perfil).

A partir daí, abrir uma conexão do grupo sobe a rede (se ainda não estiver de pé), mostra
"Subindo a rede Cliente A…" no terminal e conecta. A aba ganha o sufixo ` · Cliente A`. Quando
a última aba que usa o perfil fecha, e quando o app fecha, o comando de descer roda.

## Como o app usa o perfil

- **SSH, SFTP e Telnet:** a conexão sai pelo proxy; o servidor de salto do perfil, se houver,
  vem antes do salto da própria conexão (o Telnet não usa salto).
- **FTP e FTPS:** o controle e os dados saem pelo proxy, e por isso só o modo passivo vale (o
  modo ativo fica desabilitado enquanto a conexão tem proxy).
- **S3:** o cliente HTTP usa o proxy, e o nome do servidor é resolvido do lado de lá.
- **rclone:** o app sobe um `rclone rcd` por perfil, com o proxy em `HTTP_PROXY`,
  `HTTPS_PROXY` e `ALL_PROXY`. O rclone 1.60 não tem opção de proxy para os remotos `sftp` e
  `ftp`, e esses ignoram o perfil (a descrição da linha Rede avisa).
- **Shell local e serial:** o perfil não vale; a linha Rede fica desabilitada.
- **`known_hosts`:** cada perfil tem o seu, em `redes/<id>/known_hosts` na pasta de dados do
  app. O mesmo `10.0.0.5` tem chave de host diferente em cada rede, e com um arquivo só a
  segunda conexão seria recusada como "chave mudou".

## Segurança

- Publique o SOCKS5 **sempre** com `127.0.0.1:` na frente (`127.0.0.1:1081:1080`). Sem isso, o
  proxy fica aberto para a rede local e qualquer um dela entra na VPN do cliente.
- As credenciais da VPN (`wg0.conf`, `.ovpn`, a senha do OpenConnect) ficam nos arquivos que
  você monta no contêiner, por sua conta: cuide das permissões (`chmod 600`) e não as ponha
  em repositório.
- A senha do proxy, quando ele tiver uma, vai para o cofre do app, na credencial do perfil.

## As receitas

Cada pasta tem um `compose.yml`, um `Dockerfile` e um `entrypoint.sh` que sobe a VPN, espera
o túnel e só então abre o SOCKS5 (o `microsocks`, em `0.0.0.0:1080` **dentro** do contêiner).
Como o contêiner só abre a porta depois da VPN, o `up -d --wait` (com o `healthcheck`)
termina quando a rede está pronta. O `restart` é `"no"`: quem sobe e desce é o app.

| Pasta | VPN | O que o contêiner pede |
|---|---|---|
| [`wireguard/`](wireguard) | WireGuard | `NET_ADMIN` e `net.ipv4.conf.all.src_valid_mark=1`; o kernel do host com o módulo (todo ≥ 5.6 tem) |
| [`openvpn/`](openvpn) | OpenVPN | `NET_ADMIN` e `/dev/net/tun` |
| [`openconnect/`](openconnect) | AnyConnect, GlobalProtect, Fortinet, Pulse, Juniper NC, F5, Array | `NET_ADMIN` e `/dev/net/tun`; o protocolo vem de `VPN_PROTOCOL` |
| [`openfortivpn/`](openfortivpn) | FortiClient VPN (SSL) | `NET_ADMIN` e `/dev/ppp` |

O `microsocks` e o `openfortivpn` não estão nos pacotes principais do Alpine: os `Dockerfile`
os instalam do repositório `edge/testing`. Se você prefere um proxy com usuário e senha, o
`microsocks` aceita `-u usuário -P senha`; ponha a credencial no cofre e escolha-a na linha
**Credencial** do perfil.

Podman: `podman compose` (ou `podman-compose`) lê os mesmos arquivos. Sem o Docker, troque
`docker compose` por `podman compose` nos comandos do perfil.

## Ver funcionando sem VPN: o `demo/`

[`demo/`](demo) reproduz o problema sem nenhuma VPN: dois contêineres, os dois com o IP
`10.99.0.5` e um `sshd` (usuário `demo`, senha `demo`), publicados em `127.0.0.1:1081` e
`127.0.0.1:1082`.

```sh
cd demo
docker compose up -d --build --wait
```

No app, crie os perfis `Cliente A` (SOCKS5, `127.0.0.1`, `1081`) e `Cliente B` (`1082`), dois
grupos ligados a eles e, em cada um, uma conexão SSH para `10.99.0.5` (usuário `demo`, senha
`demo`). Cada uma chega ao seu `sshd`: `hostname` mostra `cliente-a` e `cliente-b`. Sem o
Docker no app, o mesmo vale na linha de comando:

```sh
ssh -o ProxyCommand='nc -X 5 -x 127.0.0.1:1081 %h %p' demo@10.99.0.5 hostname   # cliente-a
ssh -o ProxyCommand='nc -X 5 -x 127.0.0.1:1082 %h %p' demo@10.99.0.5 hostname   # cliente-b
```

Para desfazer: `docker compose down`.

Nada desta pasta entra no `cargo test` nem no CI: os testes do app usam servidores falsos e
não precisam de Docker nem de VPN.
