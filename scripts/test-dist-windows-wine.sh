#!/bin/sh
# Vérifie le paquet Windows dist/Norrblock/ (produit par scripts/build-windows.sh) en lançant
# le vrai norrblock.exe sous Wine, dans Docker (image `nb-rust-win`) : Vulkan passe par
# winevulkan vers lavapipe (rendu logiciel Mesa), la fenêtre par Xvfb.
#  1. scénario complet hors écran depuis le dossier du paquet, captures PNG dans $NB_E2E_OUT
#     (défaut dist/.wine-e2e) ;
#  2. même scénario en fenêtre réelle (swapchain Win32 de Wine) sous Xvfb.
# Pas de couches de validation (exécutable release, couches Windows absentes de Wine).
# Usage : NB_E2E_OUT=/chemin/hôte scripts/test-dist-windows-wine.sh   (NB_E2E_RD : distance)
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${NB_E2E_OUT:-$ROOT/dist/.wine-e2e}"
mkdir -p "$OUT"
DIST="$ROOT/dist/Norrblock"
SCEN="$ROOT/crates/nb-app/scripts"
[ -f "$DIST/norrblock.exe" ] || { echo "paquet absent : lancer scripts/build-windows.sh" >&2; exit 1; }
case "$(uname -s)" in
  MINGW*|MSYS*)
    DIST="$(cd "$DIST" && pwd -W)"; SCEN="$(cd "$SCEN" && pwd -W)"; OUT="$(cd "$OUT" && pwd -W)"
    export MSYS_NO_PATHCONV=1 ;;
esac
docker image inspect nb-rust-win >/dev/null 2>&1 \
  || docker build -t nb-rust-win -f "$ROOT/scripts/docker/windows-cross.Dockerfile" "$ROOT/scripts/docker"
exec docker run --rm -v "$DIST:/d:ro" -v "$SCEN:/s:ro" -v "$OUT:/out" nb-rust-win sh -c "
set -e
apt-get update >/dev/null && DEBIAN_FRONTEND=noninteractive apt-get install -y \
  --no-install-recommends mesa-vulkan-drivers libvulkan1 xvfb xauth >/dev/null
export WINEDEBUG=-all XDG_RUNTIME_DIR=/tmp
wineboot -i >/dev/null 2>&1 || true
rm -f /out/*.png
for mode in headless window; do
  rm -rf /tmp/N && cp -R /d /tmp/N && cd /tmp/N
  sed -i 's/\"renderDistance\": 12/\"renderDistance\": ${NB_E2E_RD:-6}/' saves/settings.json
  status=0
  if [ \$mode = headless ]; then
    timeout 2400 wine norrblock.exe --headless --script 'Z:\\s\\scenario.txt' \
      --capture-dir 'Z:\\out' > /tmp/log 2>&1 || status=\$?
  else
    xvfb-run -a -s '-screen 0 1920x1080x24' timeout 2400 wine norrblock.exe \
      --script 'Z:\\s\\scenario.txt' > /tmp/log 2>&1 || status=\$?
  fi
  tr -d '\r' < /tmp/log | grep -v '^capture ' || true
  echo \"== \$mode : code \$status\"
  [ \$status -eq 0 ]
  tr -d '\r' < /tmp/log | grep -q '^packs de ressources : resource_packs\$'
  tr -d '\r' < /tmp/log | grep -q 'échecs de script : 0'
  ls saves
done
ls /out
"
