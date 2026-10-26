# Receita: WireGuard

> Para WireGuard, use o perfil **WireGuard** da página Redes; o contêiner não é necessário. Veja
> "Rede WireGuard embutida" no [manual](../../../../docs/pt-BR/manual.md). Esta receita serve só para quem prefere
> deixar a VPN num contêiner, como as outras desta pasta.

Ponha o `wg0.conf` do cliente nesta pasta e suba com `docker compose up -d --build --wait`. No
`AllowedIPs`, só a faixa do cliente (não `0.0.0.0/0`, senão toda a saída do contêiner vai pela
VPN). O contêiner precisa de `NET_ADMIN` e de `net.ipv4.conf.all.src_valid_mark=1`, e o kernel do
host com o módulo do WireGuard (todo ≥ 5.6 tem).
