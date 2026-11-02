#!/bin/sh
# Scénario de bout en bout de nb-app sous Docker (image `nb-rust-vk` : lavapipe, couches de
# validation, Xvfb) :
#  1. hors écran (`--headless`) avec captures PNG dans $NB_E2E_OUT (défaut : target/e2e dans
#     le volume Docker) ;
#  2. fenêtré sous Xvfb (swapchain réelle), même script, sans captures.
# Échoue si le script échoue, si la validation Vulkan n'est pas active ou si elle signale un
# message (erreur ou avertissement).
# Usage : NB_E2E_OUT=/chemin/hôte crates/nb-app/scripts/e2e.sh   (NB_E2E_RD : distance de rendu)
set -e
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
DOCKER_DIR="$ROOT/crates/nb-gfx/docker"
OUT="${NB_E2E_OUT:-}"
case "$(uname -s)" in
  MINGW*|MSYS*)
    ROOT="$(cd "$ROOT" && pwd -W)"; DOCKER_DIR="$(cd "$DOCKER_DIR" && pwd -W)"
    [ -n "$OUT" ] && mkdir -p "$OUT" && OUT="$(cd "$OUT" && pwd -W)"
    export MSYS_NO_PATHCONV=1 ;;
esac
docker image inspect nb-rust-vk >/dev/null 2>&1 || docker build -t nb-rust-vk "$DOCKER_DIR"
MOUNT_OUT=""
DEST=/w/target/e2e
if [ -n "$OUT" ]; then MOUNT_OUT="-v $OUT:/out"; DEST=/out; fi
# Distance de rendu réduite (lavapipe) via settings.json ; le reste des réglages par défaut.
exec docker run --rm -v "$ROOT:/w" -v nb-cargo:/usr/local/cargo/registry \
  -v "${NB_TARGET_VOL:-nb-target-gfx}:/w/target" $MOUNT_OUT -w /w -e NB_GFX_VALIDATION=1 \
  nb-rust-vk sh -c "
set -e
cargo build -p nb-app
for mode in headless window; do
  SAVES=/tmp/nb-e2e-\$mode; rm -rf \$SAVES; mkdir -p \$SAVES
  echo '{ \"renderDistance\": ${NB_E2E_RD:-6} }' > \$SAVES/settings.json
  LOG=/tmp/nb-e2e-\$mode.log
  if [ \$mode = headless ]; then
    rm -f $DEST/*.png; mkdir -p $DEST
    status=0
    timeout 1800 target/debug/norrblock --headless --saves \$SAVES \
      --script crates/nb-app/scripts/scenario.txt --capture-dir $DEST > \$LOG 2>&1 || status=\$?
  else
    status=0
    xvfb-run -a -s '-screen 0 1920x1080x24' timeout 1800 target/debug/norrblock --saves \$SAVES \
      --script crates/nb-app/scripts/scenario.txt > \$LOG 2>&1 || status=\$?
  fi
  cat \$LOG
  echo \"== \$mode : code \$status\"
  [ \$status -eq 0 ]
  grep -q '(validation : true)' \$LOG || { echo 'validation Vulkan inactive'; exit 1; }
  if grep -q '\[vulkan:' \$LOG; then echo 'messages de validation Vulkan'; exit 1; fi
done
ls $DEST
"
