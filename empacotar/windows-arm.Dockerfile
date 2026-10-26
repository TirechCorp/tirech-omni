# Compila o Tirech Omni para Windows ARM (aarch64-pc-windows-gnullvm) num x86_64, pelo
# empacotar-arm.sh. O llvm-mingw traz o clang, o lld, o windres do ARM (o ícone e a versão do
# .exe) e a libunwind.dll, que o .exe pede e vai no zip ao lado dele.
FROM mstorsjo/llvm-mingw:20251104
# A versão do Rust vem do rust-toolchain.toml (o script passa por --build-arg).
ARG RUST
ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
        --default-toolchain "$RUST" --component rustfmt,clippy \
        --target aarch64-pc-windows-gnullvm \
    && chmod -R a+rwX /opt/rustup /opt/cargo
# O winresource chamaria o llvm-windres, que gera recurso x64; o WINDRES aponta o do ARM.
ENV CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER=aarch64-w64-mingw32-clang \
    CC_aarch64_pc_windows_gnullvm=aarch64-w64-mingw32-clang \
    AR_aarch64_pc_windows_gnullvm=llvm-ar \
    WINDRES=aarch64-w64-mingw32-windres \
    AR=llvm-ar
