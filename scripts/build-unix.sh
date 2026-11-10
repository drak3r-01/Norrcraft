#!/bin/sh
# Compile Norrblock nativement (Linux ou macOS, x86_64 ou arm64) et assemble dist/Norrblock/.
# Prérequis : Rust (https://rustup.rs). macOS : `brew install molten-vk` pour embarquer MoltenVK.
#
# Usage : scripts/build-unix.sh
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo build --locked --release -p nb-app
if [ "$(uname -s)" = Darwin ] && [ -z "${NB_MOLTENVK:-}" ] && command -v brew >/dev/null 2>&1; then
  P="$(brew --prefix molten-vk 2>/dev/null || true)/lib/libMoltenVK.dylib"
  [ -f "$P" ] && NB_MOLTENVK="$P" && export NB_MOLTENVK
fi
sh scripts/package.sh target/release/norrblock dist/Norrblock
