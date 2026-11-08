#!/bin/sh
# Aperçu hors écran du mode survie (Docker : lavapipe) : captures PNG dans $NB_E2E_OUT.
# Usage : NB_E2E_OUT=apercu-survie sh scripts/preview-survival.sh
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
OUT="${NB_E2E_OUT:-apercu-survie}"
case "$(uname -s)" in
  MINGW*|MSYS*) ROOT="$(cd "$ROOT" && pwd -W)"; mkdir -p "$OUT"; OUT="$(cd "$OUT" && pwd -W)"; export MSYS_NO_PATHCONV=1 ;;
esac
docker image inspect nb-rust-vk >/dev/null 2>&1 || docker build -t nb-rust-vk "$ROOT/crates/nb-gfx/docker"
exec docker run --rm -v "$ROOT:/w" -v nb-cargo:/usr/local/cargo/registry \
  -v "${NB_TARGET_VOL:-nb-target-survival-gfx}:/w/target" -v "$OUT:/out" -w /w \
  nb-rust-vk sh -c "
set -e
cargo build -p nb-app
SAVES=/tmp/nb-preview; rm -rf \$SAVES; mkdir -p \$SAVES
echo '{ \"renderDistance\": 6 }' > \$SAVES/settings.json
timeout 1500 target/debug/norrblock --headless --saves \$SAVES --world Survie \
  --script crates/nb-app/scripts/survival.txt --capture-dir /out
"
