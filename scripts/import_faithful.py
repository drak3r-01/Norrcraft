#!/usr/bin/env python3
"""Importe dans resource_packs/Default les textures de blocs du pack Faithful 32x.

Usage : python3 scripts/import_faithful.py chemin/vers/Faithful-32x.zip [--dry-run]

* Ne remplace que les textures réellement utilisées par assets/vanilla/blocks.json, et seulement si
  l'image du zip a la largeur de tuile attendue (32 px) : les textures de secours générées par
  scripts/gen_textures.py restent pour les blocs que Faithful n'a pas (lavande, cendre…).
* Écrit resource_packs/Default/CREDITS-Faithful.txt : la licence de Faithful
  (https://faithfulpack.net/license) exige un crédit clair avec un lien vers leur site.
  À conserver dans toute distribution, et à citer dans l'écran de crédits du jeu.
* Faithful est un redessin des textures de Minecraft : leurs droits de base restent à Mojang.
  La licence de Faithful ne couvre que leur propre travail.
"""
import io
import json
import os
import sys
import zipfile

from PIL import Image

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
BLOCKS = os.path.join(ROOT, "assets/vanilla/blocks.json")
OUT = os.path.join(ROOT, "resource_packs/Default/assets/minecraft/textures/block")
CREDITS = os.path.join(ROOT, "resource_packs/Default/CREDITS-Faithful.txt")
PREFIX = "assets/minecraft/textures/block/"


def used_textures():
    names = set()
    for b in json.load(open(BLOCKS)) + json.load(open(os.path.join(ROOT, "assets/common/blocks.json"))):
        names.update(t for t in b.get("textures", []) if t)
    return names


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    dry = "--dry-run" in sys.argv
    zf = zipfile.ZipFile(sys.argv[1])
    members = {n[len(PREFIX):-4]: n for n in zf.namelist() if n.startswith(PREFIX) and n.endswith(".png")}
    done, skipped, missing = [], [], []
    for name in sorted(used_textures()):
        if name not in members:
            missing.append(name)
            continue
        data = zf.read(members[name])
        im = Image.open(io.BytesIO(data))
        if im.width != 32:
            skipped.append(f"{name} ({im.width}x{im.height})")
            continue
        if not dry:
            with open(os.path.join(OUT, name + ".png"), "wb") as f:
                f.write(data)
        done.append(name)
    lic = next((n for n in zf.namelist() if n.lower().rsplit("/", 1)[-1] in ("license", "license.txt", "license.md")), None)
    if not dry:
        with open(CREDITS, "w", encoding="utf-8") as f:
            f.write(
                "Textures de blocs : Faithful 32x — https://faithfulpack.net\n"
                "Licence : https://faithfulpack.net/license (crédit obligatoire, lien vers leur site).\n"
                "Faithful est un redessin des textures de Minecraft ; les droits de base restent à Mojang.\n\n"
                "Fichiers importés par scripts/import_faithful.py :\n  " + "\n  ".join(done) + "\n"
            )
            if lic:
                f.write("\n--- Licence incluse dans le zip ---\n" + zf.read(lic).decode("utf-8", "replace"))
    print(f"{len(done)} textures {'à importer' if dry else 'importées'}, {len(skipped)} ignorées (largeur ≠ 32), {len(missing)} absentes du zip")
    for label, items in (("ignorées", skipped), ("absentes (texture de secours conservée)", missing)):
        if items:
            print(f"{label} : " + ", ".join(items))


if __name__ == "__main__":
    main()
