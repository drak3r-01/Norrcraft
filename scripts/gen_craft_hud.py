#!/usr/bin/env python3
"""Génère les fonds d'écran de l'établi et de la fonderie à partir de assets/textures/hud/inventory.png.

On efface la zone « personnage + grille 2×2 » de l'écran d'inventaire puis on y recolle des cases
(découpées dans le même fichier) : établi 3×3 + résultat, fonderie matière / combustible + résultat.
Coordonnées en unités de 176×166 (×4 dans le PNG) ; elles doivent rester synchronisées avec
`crates/nb-ui/src/inventory.rs` (fonctions `cell_origin` / `output_origin`).
Usage : python3 scripts/gen_craft_hud.py   (Pillow requis)
"""
import os

from PIL import Image

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
HUD = os.path.join(ROOT, "assets/textures/hud")
K = 4  # facteur d'agrandissement du PNG par rapport aux coordonnées de l'UI
BG = (198, 198, 198, 255)

base = Image.open(os.path.join(HUD, "inventory.png")).convert("RGBA")
slot = base.crop((388, 68, 460, 140))  # une case 18×18 avec sa marge
arrow = base.crop((538, 108, 608, 170))


def clear_top(img):
    """Efface tout ce qui est au-dessus de la grille principale (y < 83 unités)."""
    for y in range(8, 330):
        for x in range(8, 696):
            img.putpixel((x, y), BG)


def put_slot(img, ox, oy):
    img.paste(slot, (ox * K - 4, oy * K - 4))


def make(name, cells, out):
    img = base.copy()
    clear_top(img)
    for ox, oy in cells:
        put_slot(img, ox, oy)
    put_slot(img, *out)
    right = max(ox for ox, _ in cells) * K + 68
    ax = (right + out[0] * K) // 2 - arrow.width // 2
    img.paste(arrow, (ax, out[1] * K - 4 + 2))
    img.save(os.path.join(HUD, name))


make("workbench.png", [(30 + 18 * c, 17 + 18 * r) for r in range(3) for c in range(3)], (124, 35))
make("forge.png", [(40, 35), (76, 35)], (124, 35))
print("ok")
