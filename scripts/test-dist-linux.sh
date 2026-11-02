#!/bin/sh
# Vérifie le paquet distribuable avec l'exécutable Linux équivalent, sous Docker (image
# `nb-rust-vk` : lavapipe, couches de validation, Xvfb) :
#  1. compile `norrblock` (profil release) et assemble /tmp/Norrblock avec scripts/package.sh
#     (hors du dépôt : rien ne peut être trouvé par erreur dans l'arborescence des sources) ;
#  2. depuis le dossier du paquet : scénario complet hors écran (captures dans $NB_E2E_OUT,
#     défaut target/dist-e2e dans le volume) puis en fenêtre réelle sous Xvfb ;
#  3. depuis un autre dossier courant : les packs doivent être trouvés à côté de l'exécutable.
# Échoue si un scénario échoue, si les packs ne sont pas trouvés, si la validation Vulkan est
# inactive ou signale un message.
# Usage : NB_E2E_OUT=/chemin/hôte scripts/test-dist-linux.sh   (NB_E2E_RD : distance de rendu)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
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
DEST=/w/target/dist-e2e
if [ -n "$OUT" ]; then MOUNT_OUT="-v $OUT:/out"; DEST=/out; fi
exec docker run --rm -v "$ROOT:/w" -v nb-cargo:/usr/local/cargo/registry \
  -v "${NB_TARGET_VOL:-nb-target-gfx}:/w/target" $MOUNT_OUT -w /w -e NB_GFX_VALIDATION=1 \
  nb-rust-vk sh -c "
set -e
cargo build --locked --release -p nb-app
sh scripts/package.sh target/release/norrblock /tmp/Norrblock
ls -R /tmp/Norrblock | head -20
# Distance de rendu réduite (lavapipe) ; le reste des réglages par défaut du paquet.
sed -i 's/\"renderDistance\": 12/\"renderDistance\": ${NB_E2E_RD:-6}/' /tmp/Norrblock/saves/settings.json
grep -q '\"renderDistance\": ${NB_E2E_RD:-6}' /tmp/Norrblock/saves/settings.json
check() {
  cat \$2
  echo \"== \$1 : code \$3\"
  [ \$3 -eq 0 ]
  grep -q '(validation : true)' \$2 || { echo 'validation Vulkan inactive'; exit 1; }
  if grep -q '\[vulkan:' \$2; then echo 'messages de validation Vulkan'; exit 1; fi
  grep -q \"^packs de ressources : \$4\\\$\" \$2 || { echo \"packs non trouvés en \$4\"; exit 1; }
}
cp -R /tmp/Norrblock/saves /tmp/saves-initial
cd /tmp/Norrblock
rm -f $DEST/*.png; mkdir -p $DEST
status=0
timeout 1800 ./norrblock --headless --script /w/crates/nb-app/scripts/scenario.txt \
  --capture-dir $DEST > /tmp/dist-headless.log 2>&1 || status=\$?
check headless /tmp/dist-headless.log \$status resource_packs
ls saves
rm -rf saves && cp -R /tmp/saves-initial saves
status=0
xvfb-run -a -s '-screen 0 1920x1080x24' timeout 1800 ./norrblock \
  --script /w/crates/nb-app/scripts/scenario.txt > /tmp/dist-window.log 2>&1 || status=\$?
check fenêtre /tmp/dist-window.log \$status resource_packs
mkdir -p /tmp/ailleurs && cd /tmp/ailleurs
status=0
timeout 600 /tmp/Norrblock/norrblock --headless --frames 30 > /tmp/dist-ailleurs.log 2>&1 || status=\$?
check 'autre dossier courant' /tmp/dist-ailleurs.log \$status /tmp/Norrblock/resource_packs
ls $DEST
"
