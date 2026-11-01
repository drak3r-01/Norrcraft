#!/bin/sh
# Lance cargo dans un conteneur Docker (aucune toolchain Rust requise sur la machine).
# Usage : scripts/cargo-docker.sh test --workspace
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
case "$(uname -s)" in MINGW*|MSYS*) ROOT="$(cd "$ROOT" && pwd -W)"; export MSYS_NO_PATHCONV=1 ;; esac
exec docker run --rm \
  -v "$ROOT:/w" -v nb-cargo:/usr/local/cargo/registry -v nb-target:/w/target -w /w \
  rust:1 sh -c 'rustup component add rustfmt clippy >/dev/null 2>&1; exec cargo "$@"' sh "$@"
