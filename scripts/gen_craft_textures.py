#!/usr/bin/env python3
"""Textures provisoires du système de craft : blocs exclusifs et objets.

Écrit dans resource_packs/Default/assets/minecraft/textures/block/ (16×16 agrandi ×2, graine
fixe → résultat reproductible). Les objets purs vivent dans le même dossier que les blocs (leur
icône est une couche de l'atlas) et s'appellent `item_<nom>.png`.
Les blocs portent les noms du pack Faithful 32x (`crafting_table_*`, `blast_furnace_*`,
`coal_block`…) : le pack téléchargé écrase simplement ces placeholders. `marble_bricks`,
`aurora_block` et les `item_*` n'existent pas dans Faithful : à fournir (voir docs/CRAFT.md).
Usage : python3 scripts/gen_craft_textures.py   (Pillow + numpy requis)
"""
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import numpy as np  # noqa: E402

import gen_textures as g  # noqa: E402
from gen_textures import S, palette_img, put, rng_for, save, shade, solid, sprite, tones  # noqa: E402


def planks_base(name):
    rng = rng_for(name)
    img = palette_img(rng, tones((158, 124, 74), 4, 0.1), 4, 0.05, 0.3)
    for y in (3, 7, 11, 15):  # joints horizontaux des planches
        img[y, :, :3] = (96, 72, 40)
    return img


def border(img, color, w=1):
    for i in range(w):
        img[i, :, :3] = color
        img[S - 1 - i, :, :3] = color
        img[:, i, :3] = color
        img[:, S - 1 - i, :3] = color
    return img


def bricks(name, colors, mortar):
    rng = rng_for(name)
    img = palette_img(rng, colors, 2, 0.05, 0.4)
    for y in range(0, S, 4):
        img[y, :, :3] = mortar
        off = 0 if (y // 4) % 2 == 0 else 4
        for x in range(off, S, 8):
            img[y : y + 4, x, :3] = mortar
    return img


def block_sheet(base_color, shine, name):
    """Bloc de minerai/métal : cadre sombre, centre clair, reflet en diagonale."""
    rng = rng_for(name)
    img = palette_img(rng, tones(base_color, 4, 0.08), 3, 0.0, 0.3)
    border(img, shade(base_color, 0.62))
    border_in = shade(base_color, 1.18)
    for i in range(1, S - 1):
        img[1, i, :3] = border_in
        img[i, 1, :3] = border_in
    for k in range(3, 11):
        put(img, [(k, 13 - k + 3)], shine)
    return img


def make():
    # -- Établi ------------------------------------------------------------------------------
    top = planks_base("workbench_top")
    border(top, (84, 58, 30), 2)
    for x in (4, 7, 10):  # grille de découpe
        top[3:13, x, :3] = (70, 48, 26)
    for y in (4, 7, 10):
        top[y, 3:13, :3] = (70, 48, 26)
    save("crafting_table_top", top)
    side = planks_base("workbench_side")
    border(side, (84, 58, 30), 1)
    side[2:5, :, :3] = (110, 78, 44)  # plateau
    put(side, [(x, 8) for x in range(3, 8)], (180, 180, 190))  # scie
    put(side, [(x, 9) for x in range(3, 8)], (130, 130, 140))
    put(side, [(10, y) for y in range(7, 12)], (120, 90, 50))  # marteau
    put(side, [(9, 7), (10, 7), (11, 7)], (150, 150, 160))
    save("crafting_table_side", side)
    front = side.copy()
    put(front, [(x, 12) for x in range(3, 13)], (70, 48, 26))
    save("crafting_table_front", front)

    # -- Fonderie ----------------------------------------------------------------------------
    rng = rng_for("forge_side")
    fs = palette_img(rng, [(70, 70, 76), (62, 62, 68), (78, 78, 84), (56, 56, 62)], 3, 0.1, 0.4)
    border(fs, (40, 40, 46))
    for y in (5, 10):
        fs[y, 1:-1, :3] = (46, 46, 52)
    save("blast_furnace_side", fs)
    ft = fs.copy()
    ft[5:11, 5:11, :3] = (28, 26, 30)  # cheminée
    border(ft, (96, 96, 104), 1)
    save("blast_furnace_top", ft)
    ff = fs.copy()
    ff[4:13, 3:13, :3] = (22, 20, 24)  # gueule
    for x in range(4, 12):
        for y in range(8, 12):
            t = (x * 7 + y * 3) % 5
            ff[y, x, :3] = [(255, 150, 30), (255, 190, 60), (230, 90, 20), (255, 120, 24), (255, 220, 100)][t]
    border(ff, (40, 40, 46))
    save("blast_furnace_front", ff)

    # -- Pierres taillées --------------------------------------------------------------------
    save("marble_bricks", bricks("marble_bricks", [(232, 232, 236), (222, 222, 228), (240, 240, 242)], (156, 158, 168)))
    save("deepslate_tiles", bricks("deepslate_tiles", [(66, 70, 82), (58, 62, 74), (74, 78, 90)], (36, 38, 46)))

    # -- Blocs de stockage -------------------------------------------------------------------
    save("coal_block", block_sheet((38, 38, 42), (110, 110, 120), "coal_block"))
    save("iron_block", block_sheet((214, 214, 220), (255, 255, 255), "iron_block"))
    save("diamond_block", block_sheet((74, 220, 210), (200, 255, 250), "diamond_block"))
    save("emerald_block", block_sheet((46, 190, 92), (160, 255, 190), "emerald_block"))

    # -- Aurore : bloc lumineux violet → cyan ---------------------------------------------------
    rng = rng_for("aurora_block")
    img = np.zeros((S, S, 4), dtype=np.uint8)
    for y in range(S):
        for x in range(S):
            t = (np.sin(x * 0.55 + y * 0.35) + np.sin(y * 0.8 - x * 0.2) + 2) / 4
            c = (int(120 + 90 * t), int(70 + 170 * (1 - t) * 0.6 + 60 * t), int(220 - 30 * t))
            img[y, x] = (*c, 255)
    for _ in range(14):
        put(img, [(int(rng.integers(0, S)), int(rng.integers(0, S)))], (255, 255, 255))
    border(img, (90, 60, 170))
    save("aurora_block", img)

    # -- Objets purs (sprites sur fond transparent) --------------------------------------------
    def item(name, painter):
        img = sprite()
        painter(img)
        save("item_" + name, img)

    def stick(img):
        for i in range(11):
            put(img, [(3 + i, 12 - i)], (140, 100, 52))
            put(img, [(4 + i, 12 - i)], (104, 72, 36))

    def coal(img):
        for dx, dy in [(x, y) for x in range(-4, 5) for y in range(-3, 4) if x * x + y * y * 1.6 < 17]:
            put(img, [(8 + dx, 8 + dy)], (30, 30, 34) if (dx + dy) % 3 else (52, 52, 58))
        put(img, [(6, 6), (7, 6)], (110, 110, 122))

    def ingot(light, dark):
        def paint(img):
            for y in range(6, 12):
                for x in range(3, 14):
                    shear = x - (y - 6)
                    if 3 <= shear <= 11:
                        put(img, [(x, y)], light if y < 9 else dark)
            put(img, [(x, 6) for x in range(4, 12)], tuple(min(255, c + 40) for c in light))
        return paint

    def gem(c1, c2):
        def paint(img):
            pts = [(8, 3), (7, 4), (8, 4), (9, 4), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (5, 6), (6, 6),
                   (7, 6), (8, 6), (9, 6), (10, 6), (11, 6), (6, 7), (7, 7), (8, 7), (9, 7), (10, 7),
                   (7, 8), (8, 8), (9, 8), (8, 9)]
            for i, p in enumerate(pts):
                put(img, [p], c1 if (p[0] + p[1]) % 2 else c2)
            put(img, [(7, 5), (8, 4)], (255, 255, 255))
        return paint

    def brick_item(img):
        for y in range(6, 11):
            for x in range(3, 13):
                put(img, [(x, y)], (176, 82, 56) if y < 9 else (136, 58, 40))
        put(img, [(x, 6) for x in range(3, 13)], (206, 112, 82))

    def shard(img):
        for k, (x, y) in enumerate([(8, 2), (8, 3), (7, 4), (8, 4), (9, 4), (7, 5), (8, 5), (9, 5), (6, 6),
                                    (7, 6), (8, 6), (9, 6), (10, 6), (7, 7), (8, 7), (9, 7), (8, 8), (8, 9),
                                    (8, 10), (8, 11)]):
            put(img, [(x, y)], (150, 110, 255) if k % 2 else (90, 220, 240))
        put(img, [(8, 3), (7, 5)], (255, 255, 255))

    item("stick", stick)
    item("coal", coal)
    item("iron_ingot", ingot((222, 222, 228), (170, 170, 180)))
    item("gold_ingot", ingot((250, 214, 70), (200, 156, 30)))
    item("diamond", gem((90, 240, 230), (50, 190, 200)))
    item("emerald", gem((60, 220, 110), (30, 160, 80)))
    item("brick", brick_item)
    item("aurora_shard", shard)


if __name__ == "__main__":
    make()
    print("ok", file=sys.stderr)
