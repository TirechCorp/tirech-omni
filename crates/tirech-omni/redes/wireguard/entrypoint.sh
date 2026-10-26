#!/bin/sh
# Sobe o túnel e só então abre o SOCKS5: a porta só responde com a rede de pé.
set -e
wg-quick up wg0
exec microsocks -i 0.0.0.0 -p 1080
