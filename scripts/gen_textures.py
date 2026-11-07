#!/usr/bin/env python3
"""Génère les textures procédurales des blocs ajoutés par le générateur « Norrblock ».

Dessin 16×16 agrandi ×2 en 32×32 (même taille que le pack par défaut), graine fixe : le résultat
est reproductible. Usage : python3 scripts/gen_textures.py  (Pillow + numpy requis).
Les PNG sont écrits dans resource_packs/Default/assets/minecraft/textures/block/.
"""
import os
import sys

import numpy as np
from PIL import Image

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "resource_packs/Default/assets/minecraft/textures/block")
S = 16


def rng_for(name):
    return np.random.default_rng(abs(hash_str(name)) % (2**32))


def hash_str(s):
    h = 1469598103934665603
    for c in s.encode():
        h = ((h ^ c) * 1099511628211) % (2**64)
    return h


def vnoise(rng, cell):
    """Bruit de valeurs 16×16 à cellules de `cell` pixels (interpolation bilinéaire, périodique)."""
    n = max(1, S // cell)
    g = rng.random((n + 1, n + 1))
    g[n, :] = g[0, :]
    g[:, n] = g[:, 0]
    xs = np.arange(S) / cell
    x0 = np.floor(xs).astype(int) % n
    x1 = (x0 + 1) % (n + 1) if n + 1 > 1 else x0
    fx = xs - np.floor(xs)
    out = np.zeros((S, S))
    for y in range(S):
        for x in range(S):
            a = g[y // cell % n, x0[x]]
            b = g[y // cell % n, x0[x] + 1]
            c = g[(y // cell % n) + 1, x0[x]]
            d = g[(y // cell % n) + 1, x0[x] + 1]
            fy = (y / cell) - np.floor(y / cell)
            out[y, x] = (a * (1 - fx[x]) + b * fx[x]) * (1 - fy) + (c * (1 - fx[x]) + d * fx[x]) * fy
    return out


def palette_img(rng, colors, cell=2, speck=0.0, mix=0.5):
    """Image 16×16 : couleurs de `colors` choisies par bruit de valeurs + grains."""
    nz = 0.6 * vnoise(rng, cell) + 0.4 * rng.random((S, S)) * mix + 0.4 * (1 - mix) * vnoise(rng, 1)
    nz = (nz - nz.min()) / (nz.max() - nz.min() + 1e-9)
    idx = np.minimum((nz * len(colors)).astype(int), len(colors) - 1)
    img = np.zeros((S, S, 4), dtype=np.uint8)
    for y in range(S):
        for x in range(S):
            img[y, x, :3] = colors[idx[y, x]]
            img[y, x, 3] = 255
    if speck > 0:
        m = rng.random((S, S)) < speck
        img[m, :3] = np.clip(img[m, :3].astype(int) + rng.integers(-28, 28, (m.sum(), 1)), 0, 255)
    return img


def shade(c, f):
    return tuple(int(max(0, min(255, v * f))) for v in c)


def tones(c, n=5, spread=0.16):
    return [shade(c, 1 - spread + 2 * spread * i / (n - 1)) for i in range(n)]


# Noms de fichiers identiques à ceux du pack Faithful 32x : le pack téléchargé, quand il est
# présent, écrase simplement ces textures de secours.
RENAME = {
    "terracotta_red": "red_terracotta", "terracotta_orange": "orange_terracotta",
    "terracotta_yellow": "yellow_terracotta", "terracotta_white": "white_terracotta",
    "terracotta_brown": "brown_terracotta", "marble": "calcite", "slate": "deepslate",
    "mushroom_cap_red": "red_mushroom_block", "mushroom_cap_brown": "brown_mushroom_block",
    "coral_pink": "brain_coral_block", "coral_purple": "bubble_coral_block",
    "coral_yellow": "horn_coral_block", "coral_blue": "tube_coral_block",
    "moss": "moss_block", "daisy": "oxeye_daisy", "reeds": "sugar_cane",
}


def save(name, img):
    name = RENAME.get(name, name)
    im = Image.fromarray(img, "RGBA").resize((S * 2, S * 2), Image.NEAREST)
    im.save(os.path.join(OUT, name + ".png"))


def solid(name, color, cell=2, spread=0.12, speck=0.08, n=5, mix=0.5):
    save(name, palette_img(rng_for(name), tones(color, n, spread), cell, speck, mix))


def streaks(img, rng, vertical=True, strength=0.18, density=0.5):
    """Ombre/éclaircit des colonnes (ou lignes) entières : écorce, basalte…"""
    out = img.copy().astype(float)
    for i in range(S):
        f = 1 + (rng.random() - 0.5) * 2 * strength if rng.random() < density else 1.0
        if vertical:
            out[:, i, :3] *= f
        else:
            out[i, :, :3] *= f
    return np.clip(out, 0, 255).astype(np.uint8)


def ring_top(name, outer, inner, bark):
    """Coupe de tronc : écorce sur le pourtour puis anneaux concentriques."""
    rng = rng_for(name)
    img = np.zeros((S, S, 4), dtype=np.uint8)
    for y in range(S):
        for x in range(S):
            d = max(abs(x - 7.5), abs(y - 7.5))
            if d > 6.5:
                c = bark
            else:
                k = int(d) % 3
                c = [inner, outer, shade(outer, 0.88)][k]
            c = shade(c, 1 + (rng.random() - 0.5) * 0.1)
            img[y, x] = (*c, 255)
    save(name, img)


def side_strip(name, dirt_img, top_colors, depth_fn, rng):
    img = dirt_img.copy()
    for x in range(S):
        d = depth_fn(x)
        for y in range(d):
            c = top_colors[int(rng.integers(0, len(top_colors)))]
            img[y, x] = (*c, 255)
    save(name, img)


def load_dirt():
    im = Image.open(os.path.join(OUT, "dirt.png")).convert("RGBA").resize((S, S), Image.NEAREST)
    return np.array(im)


# ---------------------------------------------------------------------------------------------
def make_all():
    # -- Sols --------------------------------------------------------------------------------
    rng = rng_for("gravel")
    gimg = palette_img(rng, [(96, 92, 90), (118, 113, 110), (136, 131, 128), (152, 147, 143), (110, 100, 92)], 3, 0.0, 0.7)
    save("gravel", gimg)
    solid("red_sand", (190, 100, 44), 2, 0.1, 0.14, 4, 0.7)
    solid("ash", (122, 120, 124), 2, 0.1, 0.2, 5, 0.8)
    solid("moss", (84, 112, 42), 3, 0.18, 0.1, 5)

    # -- Terracotta -------------------------------------------------------------------------
    for n, c in {
        "terracotta": (152, 94, 67),
        "terracotta_red": (143, 61, 46),
        "terracotta_orange": (161, 83, 37),
        "terracotta_yellow": (186, 133, 35),
        "terracotta_white": (209, 178, 161),
        "terracotta_brown": (77, 51, 36),
    }.items():
        rng = rng_for(n)
        img = palette_img(rng, tones(c, 4, 0.05), 4, 0.04, 0.3)
        save(n, streaks(img, rng, vertical=False, strength=0.04, density=0.4))

    # -- Glace -------------------------------------------------------------------------------
    rng = rng_for("ice")
    img = palette_img(rng, [(150, 190, 250), (166, 202, 252), (182, 214, 254), (140, 180, 245)], 4, 0.0, 0.2)
    for i in range(S):  # reflets en diagonale
        for k in (3, 10):
            x, y = (i + k) % S, i
            img[y, x, :3] = np.clip(img[y, x, :3].astype(int) + 28, 0, 255)
    save("ice", img)
    rng = rng_for("packed_ice")
    img = palette_img(rng, [(120, 165, 238), (132, 176, 245), (110, 154, 230), (144, 186, 250)], 3, 0.0, 0.4)
    for _ in range(5):
        x, y = int(rng.integers(0, S)), int(rng.integers(0, S))
        for k in range(int(rng.integers(3, 7))):
            img[(y + k) % S, (x + k // 2) % S, :3] = (86, 128, 205)
    save("packed_ice", img)

    # -- Sols de forêt / champignons ---------------------------------------------------------
    dirt = load_dirt()
    rng = rng_for("podzol")
    podzol_cols = [(88, 60, 22), (101, 70, 28), (74, 50, 18), (112, 80, 34)]
    save("podzol_top", palette_img(rng, podzol_cols, 2, 0.1, 0.7))
    depth = [int(rng.integers(2, 5)) for _ in range(S)]
    side_strip("podzol_side", dirt, podzol_cols, lambda x: depth[x], rng)
    rng = rng_for("mycelium")
    myc_cols = [(112, 96, 112), (124, 106, 124), (100, 86, 102), (138, 120, 136)]
    save("mycelium_top", palette_img(rng, myc_cols, 2, 0.15, 0.7))
    depth = [int(rng.integers(2, 5)) for _ in range(S)]
    side_strip("mycelium_side", dirt, myc_cols, lambda x: depth[x], rng)

    # -- Roches ------------------------------------------------------------------------------
    rng = rng_for("basalt_side")
    img = palette_img(rng, [(66, 68, 74), (58, 60, 66), (76, 78, 84), (50, 52, 58)], 4, 0.05, 0.3)
    save("basalt_side", streaks(img, rng, True, 0.25, 0.7))
    ring_top("basalt_top", (70, 72, 78), (52, 54, 60), (60, 62, 68))
    rng = rng_for("marble")
    img = palette_img(rng, [(228, 228, 232), (238, 238, 240), (216, 216, 222), (246, 246, 248)], 4, 0.0, 0.3)
    vein = vnoise(rng, 4)
    for y in range(S):
        for x in range(S):
            if abs(vein[y, x] - 0.5) < 0.035:
                img[y, x, :3] = (150, 152, 162)
    save("marble", img)
    rng = rng_for("slate")
    img = palette_img(rng, [(78, 83, 94), (70, 74, 86), (88, 93, 104), (62, 66, 78)], 3, 0.06, 0.4)
    save("slate", streaks(img, rng, False, 0.12, 0.6))

    rng = rng_for("magma")
    img = palette_img(rng, [(54, 20, 16), (66, 26, 18), (44, 16, 14)], 3, 0.0, 0.5)
    crack = vnoise(rng, 4)
    for y in range(S):
        for x in range(S):
            d = abs(crack[y, x] - 0.5)
            if d < 0.05:
                img[y, x, :3] = (255, 150, 30)
            elif d < 0.09:
                img[y, x, :3] = (204, 70, 22)
    save("magma", img)

    # -- Bois --------------------------------------------------------------------------------
    for name, bark, inner, outer, rim in (
        ("cherry_log", (106, 66, 68), (232, 176, 172), (214, 150, 148), (84, 52, 54)),
        ("dark_oak_log", (52, 38, 24), (96, 70, 38), (80, 58, 32), (40, 29, 18)),
    ):
        rng = rng_for(name)
        img = palette_img(rng, tones(bark, 5, 0.18), 4, 0.1, 0.4)
        img = streaks(img, rng, True, 0.2, 0.8)
        save(name, img)
        ring_top(name + "_top", outer, inner, rim)

    # -- Champignons géants -----------------------------------------------------------------
    rng = rng_for("mushroom_stem")
    img = palette_img(rng, [(218, 208, 192), (206, 196, 180), (226, 218, 204)], 4, 0.04, 0.3)
    save("mushroom_stem", streaks(img, rng, True, 0.08, 0.6))
    rng = rng_for("mushroom_cap_red")
    img = palette_img(rng, [(196, 40, 36), (184, 34, 32), (208, 52, 44)], 4, 0.04, 0.3)
    for _ in range(5):
        x, y = int(rng.integers(1, S - 2)), int(rng.integers(1, S - 2))
        img[y : y + 2, x : x + 2, :3] = (240, 232, 226)
    save("mushroom_cap_red", img)
    rng = rng_for("mushroom_cap_brown")
    img = palette_img(rng, [(146, 108, 74), (134, 98, 66), (158, 120, 84)], 4, 0.05, 0.3)
    for _ in range(4):
        x, y = int(rng.integers(1, S - 3)), int(rng.integers(1, S - 3))
        img[y : y + 2, x : x + 3, :3] = (176, 140, 104)
    save("mushroom_cap_brown", img)

    # -- Coraux --------------------------------------------------------------------------------
    for n, c in {
        "coral_pink": (226, 104, 164),
        "coral_purple": (150, 64, 200),
        "coral_blue": (54, 96, 222),
        "coral_yellow": (232, 204, 52),
    }.items():
        rng = rng_for(n)
        img = palette_img(rng, tones(c, 4, 0.14), 2, 0.0, 0.2)
        cells = vnoise(rng, 3)
        for y in range(S):
            for x in range(S):
                if abs(cells[y, x] - 0.5) < 0.05:
                    img[y, x, :3] = shade(c, 0.62)
        save(n, img)


# -- Sprites en croix (fond transparent) -----------------------------------------------------
def sprite():
    return np.zeros((S, S, 4), dtype=np.uint8)


def put(img, pts, color):
    for x, y in pts:
        if 0 <= x < S and 0 <= y < S:
            img[y, x] = (*color, 255)


GREEN = (58, 122, 40)
DGREEN = (40, 92, 30)


def stem(img, x=7, top=8, leaves=True):
    put(img, [(x, y) for y in range(top, 16)], GREEN)
    put(img, [(x + 1, y) for y in range(top + 3, 16)], DGREEN)
    if leaves:
        put(img, [(x - 2, 12), (x - 1, 11), (x + 3, 11), (x + 2, 10), (x - 1, 12), (x + 2, 11)], GREEN)


def blossom(img, cx, cy, petal, center, r=2):
    for dx in range(-r, r + 1):
        for dy in range(-r, r + 1):
            if abs(dx) + abs(dy) <= r + (1 if r > 1 else 0) - (1 if abs(dx) == abs(dy) == r else 0):
                put(img, [(cx + dx, cy + dy)], petal)
    put(img, [(cx, cy)], center)


def make_sprites():
    img = sprite()
    stem(img, 7, 7)
    blossom(img, 7, 5, (54, 104, 226), (240, 226, 120))
    put(img, [(7, 3), (4, 5), (10, 5)], (84, 132, 246))
    save("cornflower", img)

    img = sprite()
    stem(img, 7, 7, leaves=False)
    put(img, [(5, 12), (4, 11), (9, 11), (10, 10)], GREEN)
    put(img, [(6, 3), (7, 2), (8, 3), (5, 4), (9, 4), (5, 5), (9, 5), (6, 6), (7, 6), (8, 6), (6, 4), (7, 4), (8, 4), (6, 5), (7, 5), (8, 5)], (236, 118, 170))
    put(img, [(7, 3), (7, 4)], (252, 170, 204))
    save("pink_tulip", img)

    img = sprite()
    stem(img, 7, 6)
    for dx, dy in ((0, -2), (0, 2), (-2, 0), (2, 0), (-1, -1), (1, -1), (-1, 1), (1, 1)):
        put(img, [(7 + dx, 4 + dy)], (246, 246, 240))
    put(img, [(7, 4), (6, 4), (8, 4), (7, 3), (7, 5)], (250, 210, 40))
    save("daisy", img)

    img = sprite()
    stem(img, 7, 7, leaves=False)
    put(img, [(6, 12), (5, 11), (9, 10), (10, 9)], GREEN)
    put(img, [(6, 3), (8, 3), (6, 4), (7, 4), (8, 4), (6, 5), (7, 5), (8, 5), (6, 6), (7, 6), (8, 6), (7, 2)], (240, 130, 36))
    put(img, [(7, 4), (7, 5)], (255, 190, 80))
    save("orange_tulip", img)

    img = sprite()
    put(img, [(x, y) for x in (5, 8, 11) for y in range(9, 16)], GREEN)
    for x, top in ((5, 3), (8, 1), (11, 4)):
        put(img, [(x, y) for y in range(top, 10)], (150, 90, 210))
        put(img, [(x - 1, y) for y in range(top + 1, 9, 2)], (178, 120, 232))
        put(img, [(x + 1, y) for y in range(top + 2, 10, 2)], (126, 70, 186))
    save("lavender", img)

    img = sprite()
    for x, top in ((3, 4), (7, 1), (11, 5)):
        for y in range(top, 16):
            put(img, [(x, y), (x + 1, y)], (116, 168, 58) if (y - top) % 4 else (84, 130, 40))
        put(img, [(x - 1, top + 3), (x + 2, top + 5)], (96, 148, 50))
    save("reeds", img)


if __name__ == "__main__":
    make_all()
    make_sprites()
    print("ok", file=sys.stderr)
