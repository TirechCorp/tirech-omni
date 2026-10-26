#!/bin/sh
# Monta o zip da versão portable do Windows: a pasta "Tirech Omni" com os cinco
# executáveis e a pasta tirech-omnidata ao lado deles. Com essa pasta ao lado do .exe, o app
# grava tudo nela e não escreve no sistema.
#
# Uso: ./empacotar-portable.sh [saída.zip]
# Os .exe vêm de $CARGO_TARGET_DIR/$TRIPLO/release (padrão: ./target e x86_64-pc-windows-gnu),
# do build
#   cargo build --release --target x86_64-pc-windows-gnu -p tirech-omni -p tirech-omni-tools
# O Windows ARM (TRIPLO=aarch64-pc-windows-gnullvm) vem do empacotar-arm.sh, que deixa a
# libunwind.dll ao lado dos .exe: ela vai junto no zip.
# Os avisos de terceiros vêm de $AVISOS (padrão: target/THIRD-PARTY-NOTICES.txt, do
#   cargo about generate about.hbs -o target/THIRD-PARTY-NOTICES.txt
# ) e entram no zip quando existem.
set -eu

raiz=$(cd "$(dirname "$0")" && pwd)
alvo=${CARGO_TARGET_DIR:-$raiz/target}/${TRIPLO:-x86_64-pc-windows-gnu}/release
saida=${1:-$raiz/target/tirech-omni-portable-windows.zip}
avisos=${AVISOS:-$raiz/target/THIRD-PARTY-NOTICES.txt}
case $saida in /*) ;; *) saida=$(pwd)/$saida ;; esac

trabalho=$(mktemp -d)
trap 'rm -rf "$trabalho"' EXIT
pacote="$trabalho/Tirech Omni"
mkdir -p "$pacote/tirech-omnidata"

for nome in tirech-omni tirech-omni-add tirech-omni-agent tirech-omni-copy-id tirech-omni-keygen; do
    if [ ! -f "$alvo/$nome.exe" ]; then
        echo "Falta $alvo/$nome.exe: rode o build do Windows antes." >&2
        exit 1
    fi
    cp "$alvo/$nome.exe" "$pacote/"
done
if [ -f "$alvo/libunwind.dll" ]; then
    cp "$alvo/libunwind.dll" "$pacote/"
fi
if [ -f "$avisos" ]; then
    cp "$avisos" "$pacote/THIRD-PARTY-NOTICES.txt"
fi

# O zip não guarda pasta vazia: o LEIA-ME mantém a tirech-omnidata no pacote.
printf '%s\r\n' \
    'Tirech Omni portable' \
    '' \
    'Esta pasta guarda tudo o que o Tirech Omni grava: conexões, cofre, chaves, configurações e' \
    'logs. Enquanto ela estiver ao lado do tirech-omni.exe, o app não grava nada no computador.' \
    '' \
    'Quem tiver esta pasta abre o cofre. Para protegê-lo, crie uma senha mestra em' \
    'Configurações > Cofre de senhas.' \
    '' \
    'Sem esta pasta, o app usa a pasta de dados do usuário (%APPDATA%\tirech-omni).' \
    '' \
    '---' \
    '' \
    'This folder holds everything Tirech Omni saves: connections, vault, keys, settings and' \
    'logs. While it sits next to tirech-omni.exe, the app writes nothing to the computer.' \
    '' \
    'Whoever has this folder opens the vault. To protect it, set a master password in' \
    'Settings > Password vault.' \
    '' \
    'Without this folder, the app uses the user data folder (%APPDATA%\tirech-omni).' \
    > "$pacote/tirech-omnidata/LEIA-ME.txt"

mkdir -p "$(dirname "$saida")"
rm -f "$saida"
(cd "$trabalho" && zip -qr "$saida" "Tirech Omni")
echo "$saida"
