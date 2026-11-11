#!/bin/sh
# Assemble le dossier distribuable du jeu à partir d'un exécutable déjà compilé.
#
# Usage : scripts/package.sh <exécutable> [dossier]      (défaut : dist/Norrblock)
#
# Contenu produit (chemins attendus par le jeu, relatifs au dossier de l'exécutable, qui est
# aussi le dossier courant quand on le lance par double-clic) :
#   norrblock(.exe)              exécutable (shaders SPIR-V, blocks.json, textures UI intégrés)
#   resource_packs/Default/      pack de textures par défaut (find_packs_root)
#   assets/                      données de référence (blocks.json, mobs_defaults.json…)
#   saves/settings.json          réglages par défaut (assets/settings_defaults.json)
#   saves/mobs_defaults.json     valeurs de référence des mobs (ne pas modifier ; comme le Java)
#   sounds/                      sons .wav personnalisés optionnels (voir sounds/LISEZMOI-SONS.md)
#   LICENSE, LISEZMOI.txt
# Les mondes sont créés dans saves/<nom>/ ; saves/mobs.json n'est écrit qu'après une
# modification dans l'écran des mobs.
set -e
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXE="$1"
DEST="${2:-$ROOT/dist/Norrblock}"
if [ -z "$EXE" ] || [ ! -f "$EXE" ]; then
  echo "usage : $0 <exécutable> [dossier]" >&2
  exit 2
fi
case "$EXE" in *.exe) NAME=norrblock.exe ;; *) NAME=norrblock ;; esac

rm -rf "$DEST"
mkdir -p "$DEST/saves" "$DEST/resource_packs" "$DEST/sounds"
cp "$EXE" "$DEST/$NAME"
chmod +x "$DEST/$NAME" 2>/dev/null || true
cp -R "$ROOT/assets" "$DEST/assets"
cp -R "$ROOT/resource_packs/Default" "$DEST/resource_packs/Default"
cp "$ROOT/assets/settings_defaults.json" "$DEST/saves/settings.json"
cp "$ROOT/assets/vanilla/mobs_defaults.json" "$DEST/saves/mobs_defaults.json"
cp "$ROOT/LICENSE" "$DEST/LICENSE"
cp "$ROOT/docs/AUDIO.md" "$DEST/sounds/LISEZMOI-SONS.md"
cp "$ROOT/scripts/LISEZMOI.txt" "$DEST/LISEZMOI.txt"


# macOS : le chargeur Vulkan est MoltenVK (Vulkan -> Metal). NB_MOLTENVK = chemin de
# libMoltenVK.dylib ; il est embarqué à côté de l'exécutable et un lanceur double-cliquable
# (Norrblock.command) le rend visible au chargeur dynamique.
if [ -n "${NB_MOLTENVK:-}" ]; then
  cp "$NB_MOLTENVK" "$DEST/libMoltenVK.dylib"
  cat > "$DEST/Norrblock.command" <<'LAUNCH'
#!/bin/sh
cd "$(dirname "$0")"
xattr -dr com.apple.quarantine . 2>/dev/null || true
export DYLD_FALLBACK_LIBRARY_PATH="$PWD:$DYLD_FALLBACK_LIBRARY_PATH"
exec ./norrblock "$@"
LAUNCH
  chmod +x "$DEST/Norrblock.command"
fi

# Contrôle : tout ce que le jeu lit à l'exécution est présent.
for f in "$NAME" resource_packs/Default/pack.mcmeta \
         resource_packs/Default/assets/minecraft/textures/block saves/settings.json; do
  [ -e "$DEST/$f" ] || { echo "manquant dans le paquet : $f" >&2; exit 1; }
done
echo "paquet prêt : $DEST"
