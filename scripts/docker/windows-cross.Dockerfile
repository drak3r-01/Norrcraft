# Image de cross-compilation Windows de Norrblock (tag local `nb-rust-win`) :
# toolchain Rust stable + cible x86_64-pc-windows-gnu + éditeur de liens MinGW-w64.
# Construite par scripts/build-windows.sh. Wine (64 bits) sert seulement au test `--help`
# de l'exécutable produit (aucun rendu : pas de Vulkan sous Wine dans le conteneur).
FROM rust:1
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      gcc-mingw-w64-x86-64 binutils-mingw-w64-x86-64 wine wine64 \
 && rm -rf /var/lib/apt/lists/* \
 && rustup toolchain install stable --profile minimal --component rustfmt,clippy --target x86_64-pc-windows-gnu \
 && rustup default stable
# `rust-toolchain.toml` demande la chaîne « stable » : la cible doit y être installée (et non
# dans la toolchain versionnée de l'image `rust:1`).
ENV CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
