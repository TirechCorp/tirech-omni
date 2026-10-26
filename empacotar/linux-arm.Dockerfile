# Compila o Tirech Omni para Linux ARM (aarch64 e armv7) num x86_64, pelo empacotar-arm.sh.
# O Ubuntu 22.04 dá a glibc 2.35: o binário roda nele e nos sistemas mais novos (Debian 12,
# Raspberry Pi OS bookworm, Fedora). O app só liga na libc; X11 e Wayland vêm por dlopen.
FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        gcc libc6-dev \
        gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
        gcc-arm-linux-gnueabihf libc6-dev-armhf-cross \
        curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# A versão do Rust vem do rust-toolchain.toml (o script passa por --build-arg).
ARG RUST
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
        --default-toolchain "$RUST" --component rustfmt,clippy \
        --target aarch64-unknown-linux-gnu,armv7-unknown-linux-gnueabihf \
    && chmod -R a+rwX /opt/rustup /opt/cargo
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABIHF_LINKER=arm-linux-gnueabihf-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
    CC_armv7_unknown_linux_gnueabihf=arm-linux-gnueabihf-gcc
