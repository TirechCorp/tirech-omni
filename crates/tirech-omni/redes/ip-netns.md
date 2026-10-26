# Sem Docker: `ip netns`

Um *network namespace* do Linux faz o papel do contêiner: tem a sua tabela de rotas, e a VPN
do cliente roda dentro dele. É o esqueleto; o que muda de cliente para cliente é só o comando
da VPN. Exige `root` e o pacote `iproute2`. Troque `clientea` pelo nome do cliente, e a
`10.200.1.0/24` por uma faixa livre no seu computador (uma por cliente).

## Montar

```sh
sudo ip netns add clientea
sudo ip link add vethA0 type veth peer name vethA1
sudo ip link set vethA1 netns clientea
sudo ip addr add 10.200.1.1/24 dev vethA0 && sudo ip link set vethA0 up
sudo ip netns exec clientea ip addr add 10.200.1.2/24 dev vethA1
sudo ip netns exec clientea ip link set vethA1 up
sudo ip netns exec clientea ip link set lo up
sudo ip netns exec clientea ip route add default via 10.200.1.1   # a saída, até a VPN subir
```

## Subir a VPN dentro dele

O comando da VPN roda com o `ip netns exec`. Os mesmos pacotes das receitas, sem o contêiner:

```sh
sudo ip netns exec clientea wg-quick up ./wg0.conf                       # WireGuard
sudo ip netns exec clientea openvpn --config cliente.ovpn --daemon      # OpenVPN
sudo ip netns exec clientea openconnect --protocol=anyconnect -b vpn.cliente-a.exemplo
sudo ip netns exec clientea openfortivpn -c config &                     # FortiClient
```

## O SOCKS5

O proxy também roda dentro do namespace, e o app o alcança pelo par `veth`, no endereço
`10.200.1.2`, não em `127.0.0.1`:

```sh
sudo ip netns exec clientea microsocks -i 10.200.1.2 -p 1080 &
```

No perfil, o proxy fica em `10.200.1.2`, porta `1080`. Quem alcança esse IP é só o seu
computador (a faixa é a do par `veth`), então o proxy não fica aberto para a rede local.

## Subir e descer pelo perfil

Ponha a sequência acima num script (`rede-cliente-a.sh subir` e `rede-cliente-a.sh descer`) e
use o script nos dois comandos do perfil. O comando que roda como `root` precisa de `sudo` sem
senha para esse script (uma linha em `/etc/sudoers.d/`).

## Desfazer

```sh
sudo pkill -f "microsocks -i 10.200.1.2"
sudo ip netns delete clientea     # apaga o veth junto
```
