#!/bin/sh
# Cada contêiner tem as próprias chaves de host e o mesmo IP, 10.99.0.5, posto no lo.
set -e
ssh-keygen -A
ip addr add 10.99.0.5/32 dev lo
/usr/sbin/sshd
exec microsocks -i 0.0.0.0 -p 1080
