#!/bin/sh
# Compila e empacota o Tirech Omni para ARM num computador x86_64, pelo Docker (as receitas
# ficam em empacotar/). Precisa do ../tomni-ui ao lado, como o build normal.
#
# Uso: ./empacotar-arm.sh [linux-arm64] [linux-armv7] [windows-arm64]   (sem nada: os três)
# Os pacotes vão para $SAIDA (padrão: ./target):
#   tirech-omni-linux-arm64.tar.gz, tirech-omni-linux-armv7.tar.gz (os cinco executáveis e,
#   se existir, o target/THIRD-PARTY-NOTICES.txt do `cargo about`)
#   tirech-omni-portable-windows-arm64.zip (o portable, pelo empacotar-portable.sh)
# O build fica em target/docker (os do contêiner não se misturam com os do computador), e os
# downloads do Cargo em target/docker/cargo-home, para a próxima vez.
set -eu

raiz=$(cd "$(dirname "$0")" && pwd)
mae=$(dirname "$raiz")
saida=${SAIDA:-$raiz/target}
case $saida in /*) ;; *) saida=$(pwd)/$saida ;; esac
build=$raiz/target/docker
avisos=${AVISOS:-$raiz/target/THIRD-PARTY-NOTICES.txt}
rust=$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$raiz/rust-toolchain.toml")
nomes="tirech-omni tirech-omni-add tirech-omni-agent tirech-omni-copy-id tirech-omni-keygen"

if [ ! -d "$mae/tomni-ui" ]; then
    echo "Falta $mae/tomni-ui: clone a lib ao lado deste repositório." >&2
    exit 1
fi
alvos=${*:-linux-arm64 linux-armv7 windows-arm64}
for alvo in $alvos; do
    case $alvo in
        linux-arm64 | linux-armv7 | windows-arm64) ;;
        *) echo "Alvo desconhecido: $alvo (linux-arm64, linux-armv7, windows-arm64)" >&2; exit 1 ;;
    esac
done
mkdir -p "$saida" "$build/cargo-home"

# imagem <receita>: monta (ou reaproveita) a imagem tirech-omni-<receita>-<rust>.
imagem() {
    tag=tirech-omni-$1:$rust
    docker build -q -t "$tag" --build-arg RUST="$rust" \
        -f "$raiz/empacotar/$1.Dockerfile" "$raiz/empacotar" >/dev/null
    echo "$tag"
}

# compilar <imagem> <triplo>: o app e as ferramentas, em release, com o usuário de fora (os
# arquivos gerados não ficam do root).
compilar() {
    docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp \
        -v "$mae:$mae" -w "$raiz" \
        -e CARGO_TARGET_DIR="$build" -e CARGO_HOME="$build/cargo-home" \
        "$1" cargo build --release --locked --target "$2" -p tirech-omni -p tirech-omni-tools
}

# tarball <triplo> <nome>: a pasta <nome> com os cinco executáveis, num .tar.gz.
tarball() {
    trabalho=$(mktemp -d)
    mkdir "$trabalho/$2"
    for nome in $nomes; do
        cp "$build/$1/release/$nome" "$trabalho/$2/"
    done
    if [ -f "$avisos" ]; then
        cp "$avisos" "$trabalho/$2/THIRD-PARTY-NOTICES.txt"
    fi
    tar -C "$trabalho" -czf "$saida/$2.tar.gz" "$2"
    rm -rf "$trabalho"
    echo "$saida/$2.tar.gz"
}

for alvo in $alvos; do
    echo "== $alvo"
    case $alvo in
        linux-arm64)
            compilar "$(imagem linux-arm)" aarch64-unknown-linux-gnu
            tarball aarch64-unknown-linux-gnu tirech-omni-linux-arm64
            ;;
        linux-armv7)
            compilar "$(imagem linux-arm)" armv7-unknown-linux-gnueabihf
            tarball armv7-unknown-linux-gnueabihf tirech-omni-linux-armv7
            ;;
        windows-arm64)
            tag=$(imagem windows-arm)
            compilar "$tag" aarch64-pc-windows-gnullvm
            docker run --rm --user "$(id -u):$(id -g)" -v "$build:$build" "$tag" \
                cp /opt/llvm-mingw/aarch64-w64-mingw32/bin/libunwind.dll \
                "$build/aarch64-pc-windows-gnullvm/release/"
            AVISOS=$avisos CARGO_TARGET_DIR=$build TRIPLO=aarch64-pc-windows-gnullvm \
                sh "$raiz/empacotar-portable.sh" "$saida/tirech-omni-portable-windows-arm64.zip"
            ;;
    esac
done
