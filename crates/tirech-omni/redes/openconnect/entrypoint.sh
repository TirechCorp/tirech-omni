#!/bin/sh
# Sobe o túnel, espera a interface e só então abre o SOCKS5. Se a VPN cair, o contêiner sai.
openconnect --protocol="${VPN_PROTOCOL:-anyconnect}" --user="$VPN_USER" \
    --passwd-on-stdin --non-inter --interface=tun0 "$VPN_SERVER" < /run/senha &
vpn=$!
for _ in $(seq 1 60); do
    ip -o addr show tun0 2>/dev/null | grep -q inet && break
    kill -0 "$vpn" 2>/dev/null || exit 1
    sleep 1
done
ip -o addr show tun0 2>/dev/null | grep -q inet || { echo "o túnel não subiu em 60 s" >&2; exit 1; }
microsocks -i 0.0.0.0 -p 1080 &
while kill -0 "$vpn" 2>/dev/null; do sleep 2; done
exit 1
