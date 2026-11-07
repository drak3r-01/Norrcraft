#!/usr/bin/env python3
"""Textures procédurales (provisoires) des blocs de construction et de la flore ajoutés en phase 9.

Mêmes conventions que gen_textures.py : dessin 16×16 agrandi ×2, graine fixe, noms de fichiers
identiques à ceux de Minecraft / Faithful 32x quand ils existent (le pack Faithful importé les
remplacera). Un fichier déjà présent n'est jamais écrasé sauf avec --force (ex. textures Faithful).
Usage : python3 scripts/gen_textures_nature.py [--force]   (Pillow + numpy requis).
"""
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_textures as g  # noqa: E402
from gen_textures import S, palette_img, rng_for, save, shade, streaks, tones, vnoise, put  # noqa: E402

FORCE = "--force" in sys.argv
_orig_save = g.save


def save_new(name, img):
    name = g.RENAME.get(name, name)
    path = os.path.join(g.OUT, name + ".png")
    if os.path.exists(path) and not FORCE:
        return
    _orig_save(name, img)


def rgb(img, mask, color):
    img[mask, :3] = color
    img[mask, 3] = 255


def planks(name, color):
    rng = rng_for(name)
    img = palette_img(rng, tones(color, 4, 0.07), 4, 0.06, 0.4)
    img = streaks(img, rng, False, 0.05, 0.3)
    for y in (3, 7, 11, 15):  # joints entre planches
        img[y, :, :3] = shade(color, 0.72)
    for y0, xs in ((0, (5, 12)), (4, (2, 9)), (8, (6, 13)), (12, (3, 10))):
        for x in xs:
            for y in range(y0, y0 + 3):
                img[y, x, :3] = shade(color, 0.82)
    save_new(name, img)


def brick_pattern(name, base, mortar, rows=4, cols=2, var=0.08, moss=None, cracks=False):
    rng = rng_for(name)
    img = np.zeros((S, S, 4), dtype=np.uint8)
    rh = S // rows
    for r in range(rows):
        off = (r % 2) * (S // cols // 2)
        for y in range(r * rh, (r + 1) * rh):
            for x in range(S):
                xx = (x + off) % S
                brick = xx // (S // cols)
                f = 1 + (hash((r, brick)) % 7 - 3) * var / 3
                joint = y % rh == rh - 1 or xx % (S // cols) == (S // cols) - 1
                col = mortar if joint else shade(base, f + (rng.random() - 0.5) * var)
                img[y, x] = (*col, 255)
    if moss is not None:
        m = vnoise(rng, 4)
        sel = m > 0.62
        for y in range(S):
            for x in range(S):
                if sel[y, x] and rng.random() < 0.85:
                    img[y, x, :3] = shade(moss, 0.85 + rng.random() * 0.3)
    save_new(name, img)


def cobble(name, base, moss=None):
    rng = rng_for(name)
    cells = vnoise(rng, 4)
    img = palette_img(rng, tones(base, 5, 0.22), 4, 0.05, 0.4)
    for y in range(S):
        for x in range(S):
            v = cells[y, x]
            if abs(v - 0.5) < 0.04:
                img[y, x, :3] = shade(base, 0.55)
    if moss is not None:
        m = vnoise(rng_for(name + "m"), 4)
        for y in range(S):
            for x in range(S):
                if m[y, x] > 0.58:
                    img[y, x, :3] = shade(moss, 0.8 + rng.random() * 0.3)
    save_new(name, img)


def wool(name, color):
    rng = rng_for(name)
    img = palette_img(rng, tones(color, 4, 0.06), 1, 0.12, 0.8)
    save_new(name, img)


def leaves(name, base_gray, fruit=None, hole=0.2):
    rng = rng_for(name)
    img = palette_img(rng, tones(base_gray, 5, 0.25), 2, 0.1, 0.6)
    holes = rng.random((S, S)) < hole
    img[holes, 3] = 0
    if fruit is not None:
        for _ in range(7):
            x, y = int(rng.integers(1, S - 1)), int(rng.integers(1, S - 1))
            img[y, x] = (*fruit, 255)
            img[y, x + 1] = (*shade(fruit, 0.8), 255)
    save_new(name, img)


def log_side(name, color):
    rng = rng_for(name)
    img = palette_img(rng, tones(color, 5, 0.18), 4, 0.1, 0.4)
    save_new(name, streaks(img, rng, True, 0.2, 0.8))


def log_top(name, outer, inner, rim):
    # ring_top écrit via save() : on le redirige temporairement vers save_new.
    g.save = save_new
    g.ring_top(name, outer, inner, rim)
    g.save = _orig_save


def blocks():
    for n, c in (
        ("oak_planks", (162, 130, 78)), ("spruce_planks", (114, 84, 48)), ("birch_planks", (192, 175, 121)),
        ("acacia_planks", (168, 90, 50)), ("dark_oak_planks", (66, 43, 20)), ("cherry_planks", (226, 178, 172)),
    ):
        planks(n, c)
    cobble("cobblestone", (122, 122, 122))
    cobble("mossy_cobblestone", (118, 122, 116), (80, 120, 44))
    brick_pattern("stone_bricks", (124, 124, 124), (92, 92, 92), 4, 2, 0.1)
    brick_pattern("mossy_stone_bricks", (120, 124, 118), (92, 96, 88), 4, 2, 0.1, moss=(76, 118, 44))
    brick_pattern("bricks", (150, 82, 66), (176, 168, 158), 4, 2, 0.14)
    brick_pattern("polished_blackstone_bricks", (54, 48, 58), (30, 26, 32), 4, 2, 0.1)
    brick_pattern("prismarine_bricks", (98, 164, 150), (66, 120, 112), 4, 2, 0.12)
    brick_pattern("cut_sandstone", (222, 208, 160), (198, 182, 134), 2, 1, 0.05)
    rng = rng_for("chiseled_sandstone")
    img = palette_img(rng, tones((222, 208, 160), 4, 0.05), 3, 0.04, 0.4)
    for i in range(3, 13):
        img[3, i, :3] = img[12, i, :3] = (180, 164, 118)
        img[i, 3, :3] = img[i, 12, :3] = (180, 164, 118)
    img[7:9, 7:9, :3] = (150, 134, 96)
    save_new("chiseled_sandstone", img)

    # Paille, étagère, verre, lumières
    rng = rng_for("hay_block_side")
    img = palette_img(rng, tones((196, 164, 40), 5, 0.14), 2, 0.1, 0.5)
    img = streaks(img, rng, True, 0.15, 0.9)
    img[[3, 4, 11, 12], :, :3] = (150, 40, 30)  # sangles
    save_new("hay_block_side", img)
    rng = rng_for("hay_block_top")
    save_new("hay_block_top", palette_img(rng, tones((205, 175, 52), 5, 0.14), 2, 0.1, 0.6))
    rng = rng_for("bookshelf")
    img = palette_img(rng, tones((162, 130, 78), 4, 0.07), 4, 0.04, 0.4)
    cols = [(150, 40, 36), (40, 80, 150), (60, 120, 60), (170, 120, 40), (110, 60, 140)]
    for y0 in (1, 9):
        for x in range(0, S):
            c = cols[(x // 2 + y0) % len(cols)]
            for y in range(y0, y0 + 6):
                img[y, x, :3] = shade(c, 0.85 + 0.3 * rng.random()) if x % 3 else shade(c, 0.6)
    img[7:9, :, :3] = (110, 84, 46)
    img[[0, 15], :, :3] = (110, 84, 46)
    save_new("bookshelf", img)
    img = np.zeros((S, S, 4), dtype=np.uint8)
    img[:, :, :3] = (196, 226, 234)
    img[:, :, 3] = 52
    img[[0, 15], :, 3] = 255
    img[:, [0, 15], 3] = 255
    img[[0, 15], :, :3] = img[:, [0, 15], :3] = (176, 214, 224)
    for i in range(3, 8):  # reflet
        img[i + 1, i - 1, :3] = (255, 255, 255)
        img[i + 1, i - 1, 3] = 190
    save_new("glass", img)
    rng = rng_for("glowstone")
    img = palette_img(rng, [(236, 188, 90), (250, 214, 120), (214, 160, 64), (255, 232, 160)], 3, 0.0, 0.6)
    save_new("glowstone", img)
    rng = rng_for("sea_lantern")
    img = palette_img(rng, [(196, 232, 226), (214, 244, 238), (170, 214, 208)], 4, 0.0, 0.3)
    img[[0, 7, 8, 15], :, :3] = (110, 150, 160)
    img[:, [0, 7, 8, 15], :3] = (110, 150, 160)
    save_new("sea_lantern", img)

    for n, c in (("white_wool", (232, 232, 228)), ("red_wool", (160, 40, 36)),
                 ("blue_wool", (46, 56, 160)), ("yellow_wool", (230, 190, 40))):
        wool(n, c)

    # Os, obsidienne, or, citrouille
    rng = rng_for("bone_block_side")
    img = palette_img(rng, tones((226, 222, 200), 4, 0.07), 3, 0.05, 0.4)
    img = streaks(img, rng, True, 0.08, 0.5)
    img[[0, 15], :, :3] = (190, 184, 160)
    save_new("bone_block_side", img)
    rng = rng_for("bone_block_top")
    img = palette_img(rng, tones((226, 222, 200), 4, 0.07), 3, 0.05, 0.4)
    for r in (3, 5):
        for i in range(S):
            for j in range(S):
                if abs(max(abs(i - 7.5), abs(j - 7.5)) - r) < 0.5:
                    img[j, i, :3] = (190, 184, 160)
    save_new("bone_block_top", img)
    rng = rng_for("obsidian")
    img = palette_img(rng, [(22, 12, 38), (30, 16, 50), (16, 8, 28), (44, 24, 72)], 3, 0.1, 0.5)
    save_new("obsidian", img)
    rng = rng_for("gold_block")
    img = palette_img(rng, [(246, 208, 62), (252, 224, 90), (228, 180, 40), (255, 238, 140)], 3, 0.0, 0.4)
    img[[0, 15], :, :3] = img[:, [0, 15], :3] = (200, 150, 30)
    save_new("gold_block", img)
    rng = rng_for("pumpkin_side")
    img = palette_img(rng, tones((214, 120, 20), 4, 0.1), 2, 0.04, 0.3)
    img = streaks(img, rng, True, 0.0, 0.0)
    for x in (0, 4, 8, 12):
        img[:, x, :3] = shade((214, 120, 20), 0.78)
    save_new("pumpkin_side", img)
    rng = rng_for("pumpkin_top")
    img = palette_img(rng, tones((200, 112, 18), 4, 0.1), 2, 0.04, 0.3)
    img[6:10, 6:10, :3] = (90, 120, 40)
    save_new("pumpkin_top", img)

    # Feu de camp, chemins, champs
    rng = rng_for("campfire_log_lit")
    img = palette_img(rng, tones((90, 60, 30), 4, 0.2), 3, 0.1, 0.4)
    f = vnoise(rng, 3)
    for y in range(S):
        for x in range(S):
            if f[y, x] > 0.55:
                img[y, x, :3] = (255, int(150 + 80 * (f[y, x] - 0.55) / 0.45), 30)
    save_new("campfire_log_lit", img)
    rng = rng_for("dirt_path_top")
    save_new("dirt_path_top", palette_img(rng, tones((150, 118, 70), 5, 0.1), 2, 0.1, 0.6))
    dirt = g.load_dirt()
    rng = rng_for("dirt_path_side")
    side = dirt.copy()
    top = palette_img(rng, tones((150, 118, 70), 5, 0.1), 2, 0.1, 0.6)
    side[:3, :, :] = top[:3, :, :]
    save_new("dirt_path_side", side)
    rng = rng_for("farmland_moist")
    img = palette_img(rng, tones((84, 50, 28), 4, 0.1), 2, 0.05, 0.5)
    for x in (2, 6, 10, 14):
        img[:, x, :3] = shade((84, 50, 28), 0.7)
    save_new("farmland_moist", img)

    # Bois et feuillages
    log_side("stripped_acacia_log", (150, 112, 90))
    log_top("stripped_acacia_log_top", (162, 120, 96), (140, 100, 78), (130, 96, 74))
    log_side("mangrove_log", (104, 46, 40))
    log_top("mangrove_log_top", (124, 56, 48), (96, 40, 34), (84, 36, 32))
    leaves("apple_leaves", (74, 150, 52), fruit=(214, 40, 32), hole=0.16)
    leaves("mangrove_leaves", (112, 112, 112), hole=0.22)


# -- Sprites -------------------------------------------------------------------------------------
def sp():
    return np.zeros((S, S, 4), dtype=np.uint8)


def col(img, x, y0, y1, c):
    put(img, [(x, y) for y in range(y0, y1)], c)


def sprites():
    img = sp()
    col(img, 7, 6, 15, (110, 80, 44))
    put(img, [(7, 4), (7, 5), (6, 5), (8, 5)], (255, 190, 50))
    put(img, [(7, 3)], (255, 240, 150))
    save_new("torch", img)

    img = sp()
    for i in range(1, 15):
        put(img, [(i, i), (15 - i, i)], (226, 226, 226))
    for x in (3, 8, 12):
        put(img, [(x, y) for y in range(2, 14)], (210, 210, 210))
    save_new("cobweb", img)

    img = sp()
    rng = rng_for("vine")
    for x in range(1, 15):
        if rng.random() < 0.7:
            for y in range(0, int(rng.integers(5, 16))):
                put(img, [(x, y)], (40 + int(rng.integers(0, 30)), 110 + int(rng.integers(0, 30)), 36))
    save_new("vine", img)

    img = sp()
    for x in (3, 6, 9, 12):
        col(img, x, 4, 16, (190, 160, 60))
        put(img, [(x - 1, 3), (x, 2), (x + 1, 3), (x - 1, 5), (x + 1, 5)], (226, 196, 80))
    save_new("wheat_stage7", img)

    img = sp()
    g.stem(img, 7, 6)
    for dx, dy in ((-2, 0), (-1, -1), (0, -2), (1, -1), (2, 0), (-1, 1), (0, 2), (1, 1), (0, 0), (-1, 0), (1, 0), (0, -1), (0, 1)):
        put(img, [(7 + dx, 4 + dy)], (156, 84, 196))
    put(img, [(7, 4)], (200, 150, 230))
    save_new("allium", img)

    img = sp()
    g.stem(img, 7, 7)
    g.blossom(img, 7, 5, (52, 150, 230), (230, 240, 255))
    save_new("blue_orchid", img)

    img = sp()
    col(img, 7, 6, 16, (62, 120, 40))
    put(img, [(5, 12), (4, 11), (9, 11), (10, 10)], (62, 120, 40))
    for dx, dy in ((0, -3), (0, 3), (-3, 0), (3, 0), (-2, -2), (2, -2), (-2, 2), (2, 2), (-1, -3), (1, -3), (-3, 1), (3, 1)):
        put(img, [(7 + dx, 4 + dy)], (252, 206, 30))
    for dx in range(-2, 3):
        for dy in range(-2, 3):
            if abs(dx) + abs(dy) <= 3:
                put(img, [(7 + dx, 4 + dy)], (120, 76, 30) if abs(dx) + abs(dy) < 3 else (252, 206, 30))
    save_new("sunflower_front", img)

    img = sp()
    g.stem(img, 7, 8, leaves=False)
    put(img, [(5, 12), (4, 11), (3, 10), (9, 11), (10, 10), (11, 9)], (46, 110, 56))
    for x, y in ((6, 6), (7, 5), (8, 6), (7, 7), (6, 8), (8, 8), (5, 7), (9, 7)):
        put(img, [(x, y)], (240, 240, 250))
    save_new("lily_of_the_valley", img)

    img = sp()
    for x in (4, 7, 10):
        col(img, x, 7, 16, (120, 150, 90))
        put(img, [(x - 1, 4), (x, 3), (x + 1, 4), (x, 4), (x - 1, 5), (x + 1, 5), (x, 5), (x, 6)], (250, 250, 250))
    save_new("cotton_grass", img)

    img = sp()
    for x, top in ((3, 4), (6, 1), (9, 3), (12, 5)):
        col(img, x, top + 4, 16, (190, 176, 110))
        put(img, [(x - 1, top + 3), (x + 1, top + 3), (x, top + 2), (x - 1, top + 1), (x + 1, top + 1), (x, top), (x, top + 1), (x, top + 3)], (236, 226, 190))
    save_new("pampas_grass", img)

    img = sp()
    g.stem(img, 7, 8, leaves=True)
    for dx, dy in ((-2, 0), (-1, -1), (0, -2), (1, -1), (2, 0), (-1, 1), (0, 2), (1, 1), (-1, 0), (1, 0), (0, -1), (0, 1)):
        put(img, [(7 + dx, 5 + dy)], (240, 70, 110))
    put(img, [(7, 5), (7, 4)], (255, 220, 80))
    save_new("hibiscus", img)

    img = sp()
    col(img, 7, 8, 16, (70, 40, 34))
    put(img, [(5, 12), (9, 11), (4, 11), (10, 10)], (70, 40, 34))
    for dx, dy in ((-1, 0), (0, -1), (1, 0), (0, 1), (0, 0), (-2, 0), (2, 0), (0, -2)):
        put(img, [(7 + dx, 6 + dy)], (255, 120, 24))
    put(img, [(7, 6)], (255, 230, 100))
    save_new("ember_bloom", img)

    img = sp()
    for x in (4, 8, 11):
        for y in range(0, 16):
            put(img, [(x, y)], (92, 150, 44) if (y % 5) else (60, 108, 32))
    put(img, [(5, 3), (6, 2), (9, 7), (10, 6), (12, 9), (13, 8)], (110, 170, 60))
    save_new("bamboo_stalk", img)

    img = sp()
    for dx in range(-5, 6):
        for dy in range(-4, 5):
            if dx * dx / 28 + dy * dy / 18 < 1:
                put(img, [(8 + dx, 10 + dy)], (40, 104, 40) if (dx + dy) % 3 else (56, 128, 52))
    for x, y in ((4, 8), (8, 7), (11, 10), (6, 12), (9, 12)):
        put(img, [(x, y)], (190, 40, 70))
    save_new("sweet_berry_bush_stage3", img)

    img = sp()
    for x, top in ((4, 5), (8, 1), (11, 6)):
        col(img, x, top, 16, (96, 140, 52))
        put(img, [(x, y) for y in range(top, top + 4)], (110, 70, 40))
        put(img, [(x + 1, y) for y in range(top, top + 4)], (130, 86, 50))
        put(img, [(x - 1, top + 6), (x + 1, top + 9)], (96, 140, 52))
    save_new("cattail", img)

    img = sp()
    for x, top in ((3, 8), (6, 5), (9, 7), (12, 9)):
        col(img, x, top, 16, (150, 160, 110))
        put(img, [(x - 1, top + 1), (x + 1, top + 2), (x - 1, top + 4)], (170, 178, 128))
    put(img, [(6, 4), (6, 3), (9, 6), (9, 5)], (200, 170, 220))
    save_new("desert_sage", img)


if __name__ == "__main__":
    blocks()
    sprites()
    print("ok", file=sys.stderr)
