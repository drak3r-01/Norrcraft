#!/usr/bin/env python3
"""Génère les données du mode survie nordique de Norrblock.

Écrit (sans aucune dépendance, Python 3 pur) :

* assets/items_survival.json    : objets purs (outils, armures, nourriture, matériaux…)
* assets/blocks_survival.json   : stades de culture (blé, pomme de terre, carotte)
* assets/survival.json          : propriétés (outils, nourriture, armures, dureté des blocs,
                                  butins des mobs, températures des biomes, cultures)
* assets/recipes_survival.json  : recettes
* resource_packs/Default/assets/minecraft/textures/block/item_*.png (+ stades de culture)

Usage : python3 scripts/gen_survival.py
Les textures sont des placeholders procéduraux (16×16 agrandis ×2, graine fixe).
"""
import json
import os
import random
import struct
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
ASSETS = os.path.join(ROOT, "assets")
TEX = os.path.join(ROOT, "resource_packs/Default/assets/minecraft/textures/block")
S = 16

# ------------------------------------------------------------------ PNG sans dépendance


def write_png(path, px, scale=2):
    """px : 16 lignes de 16 pixels (r, g, b, a)."""
    h = len(px) * scale
    w = len(px[0]) * scale
    raw = bytearray()
    for row in px:
        line = bytearray([0])
        for p in row:
            line += bytes(p) * scale
        raw += bytes(line) * scale
    # Le filtre 0 est ajouté à chaque ligne agrandie.
    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


class Img:
    def __init__(self):
        self.px = [[(0, 0, 0, 0)] * S for _ in range(S)]

    def put(self, x, y, c):
        if 0 <= x < S and 0 <= y < S:
            self.px[y][x] = (c[0], c[1], c[2], 255)

    def get(self, x, y):
        return self.px[y][x]

    def save(self, name):
        write_png(os.path.join(TEX, name + ".png"), self.px)


def shade(c, f):
    return tuple(max(0, min(255, int(v * f))) for v in c)


def rng_for(name):
    return random.Random(sum(ord(c) * (i + 7) for i, c in enumerate(name)))


def art(rows, pal):
    """Sprite depuis un dessin ASCII (16 lignes de 16 caractères, '.' = transparent)."""
    img = Img()
    for y, row in enumerate(rows):
        for x, ch in enumerate(row[:S]):
            if ch in pal:
                img.put(x, y, pal[ch])
    return img


def noisy(img, name, amount=0.08):
    """Grain léger sur les pixels opaques (donne de la matière aux placeholders)."""
    r = rng_for(name)
    for y in range(S):
        for x in range(S):
            c = img.px[y][x]
            if c[3]:
                f = 1 + (r.random() - 0.5) * 2 * amount
                img.px[y][x] = (*shade(c[:3], f), 255)
    return img


def blob(img, cx, cy, rx, ry, cols, name="blob"):
    r = rng_for(name)
    for y in range(S):
        for x in range(S):
            d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
            if d <= 1.0:
                k = 0 if d < 0.35 else (1 if d < 0.75 else 2)
                if r.random() < 0.18:
                    k = min(len(cols) - 1, k + 1)
                img.put(x, y, cols[min(k, len(cols) - 1)])
    return img


def outline(img, col):
    """Contour sombre autour du sprite."""
    src = [row[:] for row in img.px]
    for y in range(S):
        for x in range(S):
            if src[y][x][3] == 0:
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < S and 0 <= ny < S and src[ny][nx][3]:
                        img.put(x, y, col)
                        break
    return img


# ------------------------------------------------------------------ matériaux et paliers

TIERS = [
    # nom, couleur claire, sombre, vitesse, durabilité, dégâts épée
    ("wood", (166, 128, 78), (112, 82, 46), 2.0, 59, 3.0),
    ("stone", (150, 150, 156), (94, 94, 102), 3.5, 131, 4.0),
    ("iron", (224, 226, 232), (148, 152, 164), 5.5, 250, 5.0),
    ("diamond", (96, 232, 222), (44, 160, 168), 8.0, 1561, 6.0),
    ("aurora", (178, 132, 255), (88, 200, 238), 12.0, 2031, 8.0),
]
STICK = (150, 108, 56)
STICK_D = (104, 72, 38)

TOOL_ART = {
    "pickaxe": [
        "................",
        "....hhhhhh......",
        "...hHHHHHHhh....",
        "..hHHhhhhhHHh...",
        ".hHH.....sShHh..",
        ".hH.....sS..hH..",
        ".h.....sS...hh..",
        "......sS........",
        ".....sS.........",
        "....sS..........",
        "...sS...........",
        "..sS............",
        ".sS.............",
        "................",
        "................",
        "................",
    ],
    "axe": [
        "................",
        "......hhh.......",
        ".....hHHHh......",
        "....hHHHHHhs....",
        "....hHHHHHsS....",
        "....hHHHHsS.....",
        ".....hhhhS......",
        "......sS........",
        ".....sS.........",
        "....sS..........",
        "...sS...........",
        "..sS............",
        ".sS.............",
        "................",
        "................",
        "................",
    ],
    "shovel": [
        "..........hh....",
        ".........hHHh...",
        ".........hHHh...",
        ".........hHHh...",
        "..........hh....",
        ".........sS.....",
        "........sS......",
        ".......sS.......",
        "......sS........",
        ".....sS.........",
        "....sS..........",
        "...sS...........",
        "..sS............",
        "................",
        "................",
        "................",
    ],
    "hoe": [
        "................",
        "......hhhh......",
        ".....hHHHHh.....",
        "....hHhhhHh.....",
        "....hh..sSh.....",
        "........sS......",
        ".......sS.......",
        "......sS........",
        ".....sS.........",
        "....sS..........",
        "...sS...........",
        "..sS............",
        "................",
        "................",
        "................",
        "................",
    ],
    "sword": [
        "..............hh",
        ".............hHh",
        "............hHHh",
        "...........hHHh.",
        "..........hHHh..",
        ".........hHHh...",
        "..h.....hHHh....",
        "..hh...hHHh.....",
        "...hh.hHHh......",
        "....hhHHh.......",
        ".....hHh........",
        "...ssShh........",
        "...sS.hh........",
        "..sS............",
        ".sS.............",
        "................",
    ],
}

ARMOR_ART = {
    "head": [
        "................",
        "................",
        "....aAAAAAAa....",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "...aAAaaaaAAa...",
        "...aAa....aAa...",
        "...aaa....aaa...",
        "................",
        "................",
        "................",
        "................",
        "................",
        "................",
    ],
    "chest": [
        "................",
        "..aaa......aaa..",
        ".aAAAa....aAAAa.",
        ".aAAAAaaaaAAAAa.",
        ".aAAAAAAAAAAAAa.",
        ".aAAAAAAAAAAAAa.",
        "..aAAAAAAAAAAa..",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "....aaaaaaaa....",
        "................",
        "................",
        "................",
    ],
    "legs": [
        "................",
        "...aaaaaaaaaa...",
        "...aAAAAAAAAa...",
        "...aAAAAAAAAa...",
        "...aAAAaaAAAa...",
        "...aAAAaaAAAa...",
        "...aAAa..aAAa...",
        "...aAAa..aAAa...",
        "...aAAa..aAAa...",
        "...aAAa..aAAa...",
        "...aaaa..aaaa...",
        "................",
        "................",
        "................",
        "................",
        "................",
    ],
    "feet": [
        "................",
        "................",
        "................",
        "................",
        "................",
        "..aaaa....aaaa..",
        "..aAAa....aAAa..",
        "..aAAa....aAAa..",
        "..aAAa....aAAa..",
        ".aAAAAa..aAAAAa.",
        ".aAAAAa..aAAAAa.",
        ".aaaaaa..aaaaaa.",
        "................",
        "................",
        "................",
        "................",
    ],
}

# (clé, nom FR affiché dans les docs, pièces)
ARMOR_SETS = [
    # nom, clair, sombre, [défense tête, torse, jambes, pieds], chaleur totale, durabilité x
    ("fur", (168, 124, 80), (104, 70, 42), [1, 3, 2, 1], 0.62, 1.0),
    ("iron", (224, 226, 232), (148, 152, 164), [2, 6, 5, 2], 0.10, 2.4),
    ("diamond", (96, 232, 222), (44, 160, 168), [3, 8, 6, 3], 0.12, 6.0),
    ("aurora", (178, 132, 255), (88, 200, 238), [4, 9, 7, 4], 0.40, 8.0),
]
ARMOR_NAMES = {
    "fur": ["fur_cap", "fur_tunic", "fur_trousers", "fur_boots"],
    "iron": ["iron_helmet", "iron_chestplate", "iron_leggings", "iron_boots"],
    "diamond": ["diamond_helmet", "diamond_chestplate", "diamond_leggings", "diamond_boots"],
    "aurora": ["aurora_helmet", "aurora_chestplate", "aurora_leggings", "aurora_boots"],
}
SLOTS = ["head", "chest", "legs", "feet"]
SLOT_DUR = [55, 80, 75, 65]
SLOT_WARM = [0.18, 0.40, 0.28, 0.14]

ITEMS = []  # (nom, painter)
PROPS = {}  # nom -> dict


def item(name, painter, props=None):
    ITEMS.append((name, painter))
    if props:
        PROPS[name] = props


# ---- outils
for tname, light, dark, speed, dur, sword in TIERS:
    tier = [t[0] for t in TIERS].index(tname)
    for kind in ("pickaxe", "axe", "shovel", "hoe", "sword"):
        dmg = {"sword": sword, "axe": sword - 0.5, "pickaxe": sword - 1.5, "shovel": sword - 2.0, "hoe": 1.5}[kind]

        def painter(l=light, d=dark, k=kind):
            pal = {"H": l, "h": shade(d, 0.7), "S": STICK, "s": STICK_D}
            return noisy(art(TOOL_ART[k], pal), "tool" + k + str(l), 0.05)

        item(
            f"{tname}_{kind}",
            painter,
            {"tool": {"kind": kind, "tier": tier, "durability": dur, "speed": speed, "damage": round(dmg, 1)}},
        )

# ---- armures
for set_name, light, dark, defs, warm, durx in ARMOR_SETS:
    names = ARMOR_NAMES[set_name]
    for i, slot in enumerate(SLOTS):
        def painter(l=light, d=dark, s=slot, sn=set_name):
            pal = {"A": l, "a": shade(d, 0.7)}
            img = art(ARMOR_ART[s], pal)
            if sn == "fur":  # liseré blanc de fourrure
                for x in range(S):
                    for y in range(S):
                        if img.get(x, y)[3] and (x + y) % 5 == 0:
                            img.put(x, y, (240, 236, 224))
            return noisy(img, "armor" + s + sn, 0.06)

        item(
            names[i],
            painter,
            {
                "armor": {
                    "slot": slot,
                    "defense": defs[i],
                    "warmth": round(warm * SLOT_WARM[i], 3),
                    "durability": int(SLOT_DUR[i] * durx),
                }
            },
        )

# ---- matériaux


def p_ingot_like(c1, c2):
    def paint():
        img = Img()
        blob(img, 8, 8, 5.5, 4.0, [c1, shade(c1, 0.85), c2], "ing")
        return outline(img, shade(c2, 0.45))

    return paint


def p_raw_ore(c1, c2, speck):
    def paint():
        img = Img()
        blob(img, 8, 8, 5.2, 4.6, [c1, shade(c1, 0.85), c2], "raw" + str(c1))
        r = rng_for("rawsp" + str(c1))
        for _ in range(7):
            x, y = r.randrange(4, 12), r.randrange(5, 11)
            if img.get(x, y)[3]:
                img.put(x, y, speck)
        return outline(img, shade(c2, 0.4))

    return paint


def p_flint():
    img = Img()
    pts = [(8, 3), (7, 4), (8, 4), (6, 5), (7, 5), (8, 5), (9, 5), (5, 6), (6, 6), (7, 6), (8, 6), (9, 6), (10, 6),
           (6, 7), (7, 7), (8, 7), (9, 7), (10, 7), (11, 7), (7, 8), (8, 8), (9, 8), (10, 8), (8, 9), (9, 9)]
    for i, (x, y) in enumerate(pts):
        img.put(x, y, (58, 62, 70) if (x + y) % 3 else (86, 90, 100))
    img.put(7, 5, (140, 146, 160))
    return outline(img, (26, 28, 34))


def p_hide():
    img = Img()
    blob(img, 8, 8, 6.2, 5.2, [(176, 120, 76), (150, 98, 60), (118, 76, 44)], "hide")
    return outline(img, (72, 46, 26))


def p_pelt():
    img = Img()
    blob(img, 8, 8, 6.5, 5.6, [(226, 222, 212), (192, 186, 174), (150, 144, 134)], "pelt")
    r = rng_for("pelt")
    for _ in range(14):
        x, y = r.randrange(3, 13), r.randrange(4, 12)
        if img.get(x, y)[3]:
            img.put(x, y, (110, 104, 98))
    return outline(img, (74, 70, 66))


def p_bone():
    return art([
        "................",
        "................",
        "............ww..",
        "...........wWWw.",
        "..........wWWw..",
        ".........wWWw...",
        "........wWWw....",
        ".......wWWw.....",
        "......wWWw......",
        ".....wWWw.......",
        "..wwwWWw........",
        ".wWWwww.........",
        ".wWWw...........",
        "..ww............",
        "................",
        "................",
    ], {"W": (240, 236, 220), "w": (170, 164, 148)})


def p_feather():
    return art([
        "................",
        "..............ww",
        ".............wWw",
        "............wWWw",
        "...........wWWw.",
        "..........wWWw..",
        ".........wWWw...",
        "........wWWw....",
        ".......wWWw.....",
        "......wWWw......",
        ".....wWWw.......",
        "....wWw.........",
        "...sWw..........",
        "..ss............",
        ".s..............",
        "................",
    ], {"W": (232, 232, 240), "w": (150, 156, 172), "s": (110, 100, 90)})


def p_string():
    img = Img()
    for i in range(12):
        img.put(3 + i, 12 - i, (236, 232, 220))
        img.put(4 + i, 12 - i, (190, 184, 170))
    return img


def p_antler():
    return art([
        "................",
        "..w.......w.....",
        "..wW.....wW.....",
        "...wW...wW......",
        "...wW..wW..w....",
        "....wWwW..wW....",
        "....wWWW.wW.....",
        ".....wWWwW......",
        ".....wWWW.......",
        "......wW........",
        "......wW........",
        "......wW........",
        ".....wWW........",
        "................",
        "................",
        "................",
    ], {"W": (214, 190, 150), "w": (140, 112, 76)})


def p_tusk():
    return art([
        "................",
        "............ww..",
        "...........wWw..",
        "..........wWw...",
        ".........wWw....",
        "........wWWw....",
        ".......wWWw.....",
        ".......wWWw.....",
        "......wWWw......",
        "......wWWw......",
        ".....wWWw.......",
        ".....wWw........",
        "....wWw.........",
        "....ww..........",
        "................",
        "................",
    ], {"W": (246, 240, 224), "w": (172, 164, 140)})


def p_tallow():
    img = Img()
    blob(img, 8, 9, 4.6, 3.6, [(244, 236, 200), (224, 214, 172), (190, 178, 134)], "tallow")
    return outline(img, (120, 108, 70))


def p_flour():
    img = Img()
    blob(img, 8, 10, 5.0, 3.6, [(246, 240, 226), (226, 218, 198), (196, 186, 164)], "flour")
    for x in range(5, 11):
        img.put(x, 6, (150, 110, 70))
    for x in range(6, 10):
        img.put(x, 5, (170, 130, 86))
    return outline(img, (96, 76, 50))


def p_seeds(col):
    def paint():
        img = Img()
        r = rng_for("seeds" + str(col))
        for _ in range(9):
            x, y = r.randrange(4, 12), r.randrange(5, 12)
            img.put(x, y, col)
            img.put(x + 1, y, shade(col, 0.7))
        return img

    return paint


def p_bowl():
    img = Img()
    for y, (x0, x1) in zip(range(8, 13), [(2, 13), (3, 12), (4, 11), (5, 10), (6, 9)]):
        for x in range(x0, x1 + 1):
            img.put(x, y, (170, 126, 74) if y > 8 else (196, 150, 92))
    for x in range(2, 14):
        img.put(x, 8, (214, 170, 110))
    return outline(img, (88, 60, 30))


def p_rod():
    img = Img()
    for i in range(12):
        img.put(2 + i, 13 - i, STICK)
        img.put(3 + i, 13 - i, STICK_D)
    for y in range(3, 12):
        img.put(13, y, (236, 232, 220))
    img.put(12, 12, (190, 70, 60))
    img.put(13, 12, (230, 90, 80))
    img.put(13, 11, (230, 90, 80))
    return img


def p_fish(body, belly, tail):
    def paint():
        img = Img()
        blob(img, 7, 8, 5.2, 2.8, [body, shade(body, 0.85), belly], "fish" + str(body))
        for i, (x, y) in enumerate([(12, 8), (13, 6), (13, 7), (13, 9), (13, 10), (12, 7), (12, 9)]):
            img.put(x, y, tail)
        img.put(4, 7, (20, 20, 24))
        for y in range(7, 10):
            img.put(5, y, shade(body, 0.6))
        return outline(img, shade(body, 0.35))

    return paint


def p_meat(c1, c2, bone=True):
    def paint():
        img = Img()
        blob(img, 7, 7, 4.6, 4.0, [c1, shade(c1, 0.88), c2], "meat" + str(c1))
        if bone:
            for i in range(5):
                img.put(10 + i // 2, 10 + i // 2 + (i % 2), (240, 236, 220))
            img.put(13, 13, (240, 236, 220))
            img.put(14, 13, (220, 214, 196))
        return outline(img, shade(c2, 0.4))

    return paint


def p_berry(col, hi):
    def paint():
        img = Img()
        for cx, cy in ((6, 9), (10, 9), (8, 6)):
            blob(img, cx, cy, 2.6, 2.6, [col, shade(col, 0.82), shade(col, 0.6)], "berry" + str(cx))
            img.put(cx - 1, cy - 1, hi)
        img.put(8, 3, (52, 120, 40))
        img.put(9, 3, (52, 120, 40))
        return outline(img, shade(col, 0.3))

    return paint


def p_root(col, leaf=None):
    def paint():
        img = Img()
        for i in range(10):
            w = max(1, 4 - i // 3)
            for dx in range(-w // 2, w // 2 + 1):
                img.put(8 + dx - (i // 4), 5 + i, col if (i + dx) % 3 else shade(col, 0.82))
        if leaf:
            for p in ((7, 3), (8, 3), (9, 3), (8, 2), (6, 4), (10, 4)):
                img.put(p[0], p[1], leaf)
        return outline(img, shade(col, 0.4))

    return paint


def p_potato(col):
    def paint():
        img = Img()
        blob(img, 8, 9, 5.0, 3.8, [col, shade(col, 0.88), shade(col, 0.7)], "potato" + str(col))
        r = rng_for("eyes" + str(col))
        for _ in range(4):
            img.put(r.randrange(5, 12), r.randrange(7, 12), shade(col, 0.55))
        return outline(img, shade(col, 0.35))

    return paint


def p_bread():
    img = Img()
    blob(img, 8, 9, 6.0, 3.6, [(214, 156, 84), (188, 128, 62), (150, 96, 44)], "bread")
    for x in (5, 8, 11):
        img.put(x, 7, (240, 196, 120))
        img.put(x + 1, 8, (240, 196, 120))
    return outline(img, (88, 52, 24))


def p_dark_bread():
    img = Img()
    blob(img, 8, 9, 6.0, 3.6, [(120, 86, 56), (98, 66, 40), (72, 46, 26)], "rye_bread")
    for x in (5, 8, 11):
        img.put(x, 7, (160, 124, 86))
    return outline(img, (46, 28, 14))


def p_apple():
    img = Img()
    blob(img, 8, 9, 4.6, 4.4, [(210, 40, 40), (176, 28, 32), (130, 18, 24)], "apple")
    img.put(6, 7, (244, 150, 150))
    img.put(8, 4, (96, 64, 30))
    img.put(9, 3, (96, 64, 30))
    img.put(10, 4, (60, 150, 50))
    img.put(11, 4, (60, 150, 50))
    return outline(img, (70, 10, 14))


def p_bowl_food(inner, steam=False):
    def paint():
        img = p_bowl()
        for x in range(3, 13):
            img.put(x, 8, inner)
            img.put(x, 7, shade(inner, 1.12))
        for x in range(4, 12):
            img.put(x, 7, inner)
        if steam:
            for y in (3, 4, 5):
                img.put(6 + (y % 2), y, (230, 230, 236))
                img.put(10 - (y % 2), y - 1, (230, 230, 236))
        return img

    return paint


def p_flask(col):
    def paint():
        img = Img()
        for y in range(2, 5):
            img.put(7, y, (170, 130, 80))
            img.put(8, y, (170, 130, 80))
        img.put(6, 2, (120, 86, 50))
        img.put(9, 2, (120, 86, 50))
        blob(img, 8, 10, 4.2, 4.4, [col, shade(col, 0.85), shade(col, 0.62)], "flask")
        for y in range(5, 8):
            for x in (7, 8):
                img.put(x, y, (220, 220, 230) if y == 5 else col)
        img.put(6, 9, (255, 255, 255))
        return outline(img, shade(col, 0.28))

    return paint


def p_gem_like(c1, c2):
    def paint():
        img = Img()
        pts = [(8, 3), (7, 4), (8, 4), (9, 4), (6, 5), (7, 5), (8, 5), (9, 5), (10, 5), (5, 6), (6, 6), (7, 6),
               (8, 6), (9, 6), (10, 6), (11, 6), (6, 7), (7, 7), (8, 7), (9, 7), (10, 7), (7, 8), (8, 8), (9, 8),
               (8, 9)]
        for x, y in pts:
            img.put(x, y, c1 if (x + y) % 2 else c2)
        img.put(7, 5, (255, 255, 255))
        img.put(8, 4, (255, 255, 255))
        return outline(img, shade(c2, 0.4))

    return paint


def p_charcoal():
    img = Img()
    blob(img, 8, 8, 4.8, 3.8, [(46, 40, 38), (32, 28, 28), (22, 20, 20)], "charcoal")
    img.put(6, 6, (120, 104, 96))
    img.put(10, 8, (96, 82, 76))
    return img


def p_torch_tallow():
    img = Img()
    for y in range(6, 14):
        img.put(8, y, STICK)
        img.put(9, y, STICK_D)
    for p in ((7, 3), (8, 3), (9, 3), (8, 2), (7, 4), (8, 4), (9, 4), (8, 5), (9, 5)):
        img.put(p[0], p[1], (255, 190, 60) if p[1] > 3 else (255, 120, 30))
    img.put(8, 4, (255, 240, 170))
    return img


def p_cloth(col):
    def paint():
        img = Img()
        for y in range(4, 12):
            for x in range(3, 13):
                img.put(x, y, col if (x + y) % 2 else shade(col, 0.92))
        for x in range(3, 13):
            img.put(x, 4, shade(col, 1.15))
            img.put(x, 11, shade(col, 0.7))
        return outline(img, shade(col, 0.4))

    return paint


def p_pearl():
    img = Img()
    blob(img, 8, 8, 3.8, 3.8, [(238, 244, 255), (200, 214, 240), (150, 170, 210)], "pearl")
    img.put(7, 6, (255, 255, 255))
    return outline(img, (90, 104, 150))


def p_rune():
    img = Img()
    blob(img, 8, 8, 5.6, 5.6, [(140, 142, 150), (116, 118, 128), (84, 86, 96)], "rune")
    for p in ((8, 4), (8, 5), (8, 6), (8, 7), (8, 8), (8, 9), (8, 10), (7, 5), (9, 7), (7, 9), (9, 5)):
        img.put(p[0], p[1], (120, 230, 255))
    return outline(img, (46, 48, 58))


def p_amber():
    return p_gem_like((255, 190, 60), (214, 130, 24))()


def p_salt():
    img = Img()
    blob(img, 8, 9, 4.2, 3.2, [(248, 248, 252), (222, 224, 236), (176, 180, 204)], "salt")
    return outline(img, (110, 114, 140))


def p_pie():
    img = Img()
    blob(img, 8, 9, 6.4, 3.8, [(228, 148, 52), (204, 118, 36), (160, 82, 24)], "pie")
    for x in range(4, 13, 2):
        img.put(x, 7, (255, 210, 120))
    return outline(img, (90, 44, 14))


MAT = {
    # nom: (painter, props)
    "flint": (p_flint, None),
    "raw_iron": (p_raw_ore((205, 170, 140), (150, 110, 84), (232, 214, 190)), None),
    "raw_gold": (p_raw_ore((238, 206, 90), (190, 150, 40), (255, 244, 170)), None),
    "charcoal": (p_charcoal, {"fuel": 80}),
    "hide": (p_hide, None),
    "pelt": (p_pelt, None),
    "bone": (p_bone, None),
    "feather": (p_feather, None),
    "string": (p_string, None),
    "antler": (p_antler, None),
    "tusk": (p_tusk, None),
    "tallow": (p_tallow, {"fuel": 40}),
    "cloth": (p_cloth((226, 220, 204)), None),
    "amber": (p_amber, None),
    "bowl": (p_bowl, None),
    "fishing_rod": (p_rod, {"tool": {"kind": "rod", "tier": 0, "durability": 120, "speed": 1.0, "damage": 1.0}}),
    "flour": (p_flour, None),
    "wheat_seeds": (p_seeds((120, 150, 56)), None),
}
for n, (pt, pr) in MAT.items():
    item(n, pt, pr)

# fer / or fondus : les lingots existent déjà (ids 258/259) ; matières premières ci-dessus.

FOODS = [
    # nom, painter, faim, saturation, chaleur, santé
    ("apple", p_apple, 3, 2, 0, 0),
    ("lingonberry", p_berry((196, 30, 52), (255, 160, 170)), 2, 1, 2, 0),
    ("blueberry", p_berry((60, 70, 170), (170, 180, 255)), 2, 1, 2, 0),
    ("carrot", p_root((236, 124, 24), (60, 140, 50)), 3, 3, 0, 0),
    ("turnip", p_root((220, 196, 224), (80, 150, 60)), 2, 2, 0, 0),
    ("potato", p_potato((196, 160, 96)), 1, 1, 0, 0),
    ("baked_potato", p_potato((150, 100, 52)), 5, 6, 8, 0),
    ("rye_bread", p_dark_bread, 6, 7, 6, 0),
    ("wheat_bread", p_bread, 5, 6, 5, 0),
    ("raw_meat", p_meat((206, 84, 90), (150, 50, 60)), 2, 1, 0, 0),
    ("cooked_meat", p_meat((156, 90, 52), (104, 56, 30)), 8, 12, 12, 0),
    ("raw_poultry", p_meat((232, 150, 150), (190, 100, 110)), 2, 1, 0, 0),
    ("cooked_poultry", p_meat((210, 140, 70), (150, 90, 40)), 6, 8, 10, 0),
    ("raw_fish", p_fish((130, 150, 172), (206, 214, 226), (96, 112, 136)), 1, 0, 0, 0),
    ("cooked_fish", p_fish((196, 152, 100), (228, 196, 150), (150, 108, 60)), 5, 6, 8, 0),
    ("smoked_fish", p_fish((156, 100, 52), (190, 136, 80), (110, 70, 36)), 6, 9, 14, 0),
    ("porridge", p_bowl_food((232, 220, 188), True), 6, 8, 28, 0),
    ("fish_stew", p_bowl_food((214, 150, 84), True), 8, 11, 34, 0),
    ("mushroom_stew", p_bowl_food((190, 130, 96), True), 6, 8, 24, 0),
    ("berry_pie", p_pie, 8, 9, 10, 0),
    ("mead", p_flask((236, 170, 40)), 2, 3, 46, 0),
    ("lingon_juice", p_flask((190, 30, 56)), 3, 2, 10, 0),
]
for n, pt, hu, sa, wa, he in FOODS:
    item(n, pt, {"food": {"hunger": hu, "saturation": sa, "warmth": wa, "health": he}})

BIG_FUEL = {"coal": 80, "stick": 5}  # surcharges de combustible (informatif)

# ------------------------------------------------------------------ blocs de cultures

CROPS = {
    # nom : (stades, couleur mature, couleur tige)
    "wheat": (4, (214, 184, 70), (96, 150, 52)),
    "potato": (4, (84, 140, 52), (62, 120, 44)),
    "carrot": (4, (70, 150, 60), (60, 130, 46)),
}


def crop_img(kind, stage, stages, mature, stem):
    img = Img()
    h = 3 + stage * 4  # hauteur
    r = rng_for(f"{kind}{stage}")
    cols = [1, 3, 5, 7, 9, 11, 13] if kind == "wheat" else [2, 5, 8, 11, 14]
    for cx in cols:
        top = 15 - h + r.randrange(0, 2)
        for y in range(top, 16):
            img.put(cx, y, stem if (y + cx) % 3 else shade(stem, 0.8))
        if kind == "wheat" and stage == stages - 1:
            for y in range(top, top + 4):
                img.put(cx, y, mature)
                img.put(cx + 1, y + 1, shade(mature, 0.85))
            img.put(cx, top - 1, shade(mature, 1.15))
        elif kind != "wheat":
            if stage >= 1:
                img.put(cx - 1, top + 2, stem)
                img.put(cx + 1, top + 1, shade(stem, 1.15))
            if stage == stages - 1:
                # fleurs blanches (pomme de terre) / fanes orangées (carotte)
                img.put(cx, top, (250, 250, 240) if kind == "potato" else (236, 130, 30))
                img.put(cx + 1, top + 1, shade(stem, 0.9))
    return img


# ------------------------------------------------------------------ règles des blocs


def block_names():
    names = []
    for f in ("blocks.json", "blocks_craft.json"):
        with open(os.path.join(ASSETS, f), encoding="utf-8") as fh:
            names += [b["name"] for b in json.load(fh)]
    return names


def rules(names):
    """Dureté (secondes avec la bonne méthode), outil, palier minimal, butin."""
    R = {}

    def d(item_, mn=1, mx=1, chance=1.0):
        return {"item": item_, "min": mn, "max": mx, "chance": chance}

    for n in names:
        if n == "air":
            continue
        r = {"hardness": 1.0, "tool": "none", "tier": 0, "drops": [d(n)]}
        if n in ("water", "lava"):
            r = {"hardness": -2.0, "tool": "none", "tier": 0, "drops": []}
        elif n == "bedrock":
            r = {"hardness": -1.0, "tool": "none", "tier": 0, "drops": []}
        elif n == "grass":
            r = {"hardness": 0.9, "tool": "shovel", "tier": 0, "drops": [d("dirt")]}
        elif n in ("dirt", "podzol", "mycelium", "dirt_path", "farmland", "clay", "moss"):
            r = {"hardness": 0.9, "tool": "shovel", "tier": 0, "drops": [d("dirt" if n != "clay" and n != "moss" else n)]}
        elif n in ("sand", "red_sand", "gravel", "snow", "ash"):
            r = {"hardness": 0.8, "tool": "shovel", "tier": 0, "drops": [d(n)]}
            if n == "gravel":
                r["drops"] = [d("gravel", 1, 1, 0.8), d("flint", 1, 1, 0.35)]
            if n == "snow":
                r["hardness"] = 0.5
        elif n in ("stone", "cobblestone", "mossy_cobblestone"):
            r = {"hardness": 2.2, "tool": "pickaxe", "tier": 0, "drops": [d("cobblestone")]}
        elif n in ("basalt", "slate", "marble", "magma", "sandstone", "red_sandstone", "terracotta",
                   "terracotta_red", "terracotta_orange", "terracotta_yellow", "terracotta_white",
                   "terracotta_brown", "cut_sandstone", "chiseled_sandstone", "bone_block"):
            r = {"hardness": 2.4, "tool": "pickaxe", "tier": 0, "drops": [d(n)]}
        elif n.endswith("_bricks") or n in ("bricks", "slate_tiles", "stone_bricks"):
            r = {"hardness": 2.6, "tool": "pickaxe", "tier": 0, "drops": [d(n)]}
        elif n == "obsidian":
            r = {"hardness": 22.0, "tool": "pickaxe", "tier": 3, "drops": [d("obsidian"), d("aurora_shard", 1, 1, 0.3)]}
        elif n == "coal_ore":
            r = {"hardness": 3.0, "tool": "pickaxe", "tier": 0, "drops": [d("coal", 1, 2)]}
        elif n == "iron_ore":
            r = {"hardness": 3.2, "tool": "pickaxe", "tier": 1, "drops": [d("raw_iron")]}
        elif n == "gold_ore":
            r = {"hardness": 3.2, "tool": "pickaxe", "tier": 2, "drops": [d("raw_gold")]}
        elif n == "diamond_ore":
            r = {"hardness": 3.6, "tool": "pickaxe", "tier": 2, "drops": [d("diamond")]}
        elif n == "emerald_ore":
            r = {"hardness": 3.6, "tool": "pickaxe", "tier": 2, "drops": [d("emerald")]}
        elif n in ("ice", "packed_ice"):
            r = {"hardness": 0.9, "tool": "pickaxe", "tier": 0, "drops": []}
            if n == "packed_ice":
                r = {"hardness": 1.6, "tool": "pickaxe", "tier": 0, "drops": [d("aurora_shard", 1, 1, 0.12)]}
        elif n in ("coal_block", "iron_block", "gold_block", "diamond_block", "emerald_block", "aurora_block"):
            r = {"hardness": 5.0, "tool": "pickaxe", "tier": 1 if n == "coal_block" else 2, "drops": [d(n)]}
        elif n in ("forge", "workbench", "lantern", "sea_lantern", "prismarine_bricks"):
            tool = "axe" if n == "workbench" else "pickaxe"
            r = {"hardness": 2.4, "tool": tool, "tier": 0, "drops": [d(n)]}
        elif n == "spruce_wood":
            r = {"hardness": 2.2, "tool": "axe", "tier": 0, "drops": [d(n), d("amber", 1, 1, 0.05)]}
        elif n.endswith("_wood") or n in ("cherry_wood", "dark_oak_wood", "baobab_wood", "redwood_wood",
                                          "mushroom_stem"):
            r = {"hardness": 2.2, "tool": "axe", "tier": 0, "drops": [d(n)]}
        elif n.startswith("planks") or n in ("bookshelf", "hay_block", "pumpkin", "campfire", "bamboo"):
            r = {"hardness": 1.8, "tool": "axe", "tier": 0, "drops": [d(n)]}
            if n == "bookshelf":
                r["drops"] = [d("bookshelf")]
        elif n.endswith("_leaves"):
            drops = [d("stick", 1, 2, 0.25)]
            if n in ("apple_leaves",):
                drops.append(d("apple", 1, 1, 0.12))
            else:
                drops.append(d("apple", 1, 1, 0.02))
            r = {"hardness": 0.25, "tool": "axe", "tier": 0, "drops": drops}
        elif n == "glass":
            r = {"hardness": 0.4, "tool": "none", "tier": 0, "drops": []}
        elif n.startswith("wool_"):
            r = {"hardness": 0.9, "tool": "none", "tier": 0, "drops": [d(n)]}
        elif n in ("tall_grass", "fern", "dead_bush", "pampas_grass", "cotton_grass", "desert_sage", "cattail",
                   "reeds", "lavender", "vines"):
            r = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": []}
            if n in ("tall_grass", "fern", "pampas_grass"):
                r["drops"] = [d("wheat_seeds", 1, 2, 0.22)]
            elif n == "dead_bush":
                r["drops"] = [d("stick", 1, 2, 0.6)]
            elif n in ("reeds", "cattail"):
                r["drops"] = [d(n)]
            else:
                r["drops"] = [d(n)] if n in ("lavender",) else []
        elif n.startswith("flower_") or n in ("allium", "blue_orchid", "sunflower", "lily_of_the_valley",
                                              "hibiscus", "ember_bloom", "cotton_grass"):
            r = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d(n)]}
        elif n.startswith("mushroom_") and n != "mushroom_stem":
            r = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d(n)]}
        elif n.startswith("mushroom_cap"):
            r = {"hardness": 0.8, "tool": "axe", "tier": 0, "drops": [d("mushroom_brown" if "brown" in n else "mushroom_red", 1, 2)]}
        elif n == "torch":
            r = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d("torch")]}
        elif n == "cobweb":
            r = {"hardness": 3.0, "tool": "sword", "tier": 0, "drops": [d("string", 1, 2)]}
        elif n == "cactus":
            r = {"hardness": 0.5, "tool": "none", "tier": 0, "drops": [d("cactus")]}
        elif n == "berry_bush":
            r = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d("lingonberry", 1, 3), d("blueberry", 0, 1, 0.5)]}
        elif n == "lily_pad":
            r = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d(n)]}
        elif n.startswith("coral_"):
            r = {"hardness": 1.0, "tool": "pickaxe", "tier": 0, "drops": [d(n)]}
        elif n == "wheat":
            r = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d("wheat_bread_dough_placeholder")]}
        R[n] = r
    # cultures
    R["wheat"]["drops"] = [d("wheat_grain"), d("wheat_seeds", 1, 3, 0.9)]
    return R


# ------------------------------------------------------------------ butin des mobs


def loot_tables(mob_ids):
    def d(item_, mn=1, mx=1, chance=1.0):
        return {"item": item_, "min": mn, "max": mx, "chance": chance}

    L = {}
    big_land = {"reindeer", "elk", "bison", "camel", "giraffe", "zebra", "wild_boar", "mouflon", "mountain_goat",
                "capybara"}
    pelt_land = {"arctic_fox", "grey_wolf", "brown_bear", "polar_bear", "panther", "fennec_fox", "red_panda",
                 "hare"}
    birds = {"pheasant", "flamingo", "penguin", "ostrich"}
    fish = {"fish", "salmon", "tuna", "clownfish", "pufferfish", "piranha", "lanternfish", "manta_ray", "shark"}
    big_sea = {"orca", "dolphin", "narwhal", "sea_turtle", "tortoise"}
    for m in mob_ids:
        t = []
        if m in big_land:
            t += [d("raw_meat", 1, 3), d("hide", 1, 2, 0.8)]
            if m in ("reindeer", "elk"):
                t += [d("antler", 1, 1, 0.45)]
            if m in ("bison", "wild_boar"):
                t += [d("tusk" if m == "wild_boar" else "pelt", 1, 1, 0.4), d("tallow", 1, 2, 0.5)]
            if m == "mountain_goat" or m == "mouflon":
                t += [d("pelt", 1, 1, 0.6)]
        elif m in pelt_land:
            t += [d("pelt", 1, 2, 0.9), d("raw_meat", 1, 2 if m in ("brown_bear", "polar_bear", "grey_wolf") else 1, 0.9),
                  d("bone", 1, 1, 0.5)]
            if m in ("brown_bear", "polar_bear"):
                t += [d("tallow", 1, 3, 0.8)]
        elif m in birds:
            t += [d("raw_poultry", 1, 2), d("feather", 1, 3, 0.9)]
        elif m in fish:
            t += [d("raw_fish", 1, 2 if m in ("shark", "manta_ray", "tuna") else 1)]
            if m in ("shark", "piranha"):
                t += [d("bone", 1, 1, 0.5)]
        elif m in big_sea:
            t += [d("raw_fish", 1, 3), d("tallow", 1, 3, 0.8), d("bone", 1, 2, 0.5)]
            if m == "narwhal":
                t += [d("tusk", 1, 1, 0.8)]
        elif m in ("hedgehog", "frog", "crab", "ember_newt", "sand_lizard", "scorpion", "jellyfish",
                   "ink_squid", "mossback"):
            t += [d("raw_meat", 1, 1, 0.6)]
            if m in ("crab",):
                pass
        else:
            t += [d("raw_meat", 1, 1, 0.7), d("hide", 1, 1, 0.4)]
        L[m] = t
    return L


# ------------------------------------------------------------------ températures

BIOME_TEMP = {
    # -1 glacial … 0 doux … +1 torride
    "PLAINS": 0.0, "FOREST": -0.05, "DESERT": 0.55, "TAIGA": -0.35, "JUNGLE": 0.35, "SWAMP": 0.1,
    "SAVANNA": 0.4, "MOUNTAINS": -0.45, "OCEAN": -0.1, "DEEP_OCEAN": -0.15, "WARM_OCEAN": 0.25,
    "FROZEN_OCEAN": -0.8, "RIVER": -0.1, "BEACH": 0.05, "STONY_SHORE": -0.2, "BIRCH_FOREST": -0.05,
    "DARK_FOREST": -0.1, "FLOWER_MEADOW": 0.05, "CHERRY_GROVE": 0.0, "AUTUMN_FOREST": -0.15,
    "SNOWY_PLAINS": -0.7, "SNOWY_TAIGA": -0.75, "ICE_SPIKES": -0.95, "HEATH": -0.2, "MEGA_TAIGA": -0.35,
    "BADLANDS": 0.5, "HILLS": -0.3, "SNOWY_PEAKS": -1.0, "VOLCANO": 0.8, "MUSHROOM_FIELDS": 0.0,
}

# ------------------------------------------------------------------ recettes

RECIPES = []


def shaped(station, out, count, pattern, key):
    RECIPES.append({"station": station, "result": {"item": out, "count": count}, "pattern": pattern, "key": key})


def shapeless(station, out, count, items):
    RECIPES.append({"station": station, "result": {"item": out, "count": count}, "shapeless": items})


TAGS = {
    "tools_stick": ["stick"],
    "meats_raw": ["raw_meat", "raw_poultry"],
    "veg": ["carrot", "turnip", "potato"],
    "berries": ["lingonberry", "blueberry"],
    "fuel_all": ["coal", "charcoal", "tallow", "stick", "planks_oak", "planks_birch", "planks_spruce",
                 "planks_acacia", "planks_cherry", "planks_dark_oak", "wood", "birch_wood", "spruce_wood",
                 "jungle_wood", "acacia_wood", "cherry_wood", "dark_oak_wood"],
    "metals_head": ["planks_oak", "planks_birch", "planks_spruce", "planks_acacia", "planks_cherry",
                    "planks_dark_oak"],
    "stone_head": ["cobblestone", "stone", "basalt", "slate", "marble"],
}

HEADS = {"wood": "#metals_head", "stone": "#stone_head", "iron": "iron_ingot", "diamond": "diamond",
         "aurora": "aurora_shard"}


def tool_recipes():
    for tname, *_ in TIERS:
        h = HEADS[tname]
        shaped("workbench", f"{tname}_pickaxe", 1, ["HHH", " S ", " S "], {"H": h, "S": "stick"})
        shaped("workbench", f"{tname}_axe", 1, ["HH", "HS", " S"], {"H": h, "S": "stick"})
        shaped("workbench", f"{tname}_shovel", 1, ["H", "S", "S"], {"H": h, "S": "stick"})
        shaped("workbench", f"{tname}_hoe", 1, ["HH", " S", " S"], {"H": h, "S": "stick"})
        shaped("workbench", f"{tname}_sword", 1, ["H", "H", "S"], {"H": h, "S": "stick"})


def armor_recipes():
    mats = {"fur": "pelt", "iron": "iron_ingot", "diamond": "diamond", "aurora": "aurora_shard"}
    for set_name, *_ in ARMOR_SETS:
        m = mats[set_name]
        names = ARMOR_NAMES[set_name]
        shaped("workbench", names[0], 1, ["MMM", "M M"], {"M": m})
        shaped("workbench", names[1], 1, ["M M", "MMM", "MMM"], {"M": m})
        shaped("workbench", names[2], 1, ["MMM", "M M", "M M"], {"M": m})
        shaped("workbench", names[3], 1, ["M M", "M M"], {"M": m})


def misc_recipes():
    # base
    shaped("workbench", "bowl", 2, ["P P", " P "], {"P": "#planks"})
    shapeless("hand", "string", 1, ["hide"])
    shapeless("hand", "cloth", 1, ["string", "string", "string", "string"])
    shapeless("hand", "flour", 1, ["wheat_grain"])
    shaped("workbench", "fishing_rod", 1, ["  S", " ST", "S T"], {"S": "stick", "T": "string"})
    shapeless("hand", "wheat_bread", 1, ["flour", "flour"])
    shapeless("hand", "rye_bread", 1, ["flour", "flour", "flour"])
    shapeless("hand", "porridge", 1, ["bowl", "flour", "lingonberry"])
    shapeless("hand", "mushroom_stew", 1, ["bowl", "mushroom_brown", "mushroom_red"])
    shapeless("hand", "mushroom_stew", 1, ["bowl", "mushroom_brown", "mushroom_brown"])
    shapeless("hand", "fish_stew", 1, ["bowl", "cooked_fish", "potato"])
    shapeless("hand", "fish_stew", 1, ["bowl", "smoked_fish", "carrot"])
    shapeless("hand", "lingon_juice", 1, ["lingonberry", "lingonberry", "lingonberry"])
    shapeless("workbench", "berry_pie", 1, ["flour", "flour", "lingonberry", "blueberry", "blueberry"])
    shaped("workbench", "mead", 1, [" A ", "BFB", "BBB"], {"A": "amber", "B": "#berries", "F": "flour"})
    # lumière / chaleur
    shaped("hand", "torch", 2, ["T", "S"], {"T": "tallow", "S": "stick"})
    shaped("workbench", "campfire", 1, [" S ", "SCS", "LLL"], {"S": "stick", "C": "charcoal", "L": "#logs"})
    # combustible & transformation
    shaped("forge", "charcoal", 1, ["WF"], {"W": "#logs", "F": "#fuel_all"})
    # cuisson / fonderie
    for raw, cooked in (("raw_meat", "cooked_meat"), ("raw_poultry", "cooked_poultry"), ("raw_fish", "cooked_fish"),
                        ("potato", "baked_potato")):
        shaped("forge", cooked, 1, ["RF"], {"R": raw, "F": "#fuel_all"})
    shaped("forge", "smoked_fish", 1, ["RF", ], {"R": "cooked_fish", "F": "#fuel_all"})
    shaped("forge", "iron_ingot", 1, ["RF"], {"R": "raw_iron", "F": "#fuel_all"})
    shaped("forge", "gold_ingot", 1, ["RF"], {"R": "raw_gold", "F": "#fuel_all"})
    shaped("forge", "glass", 1, ["RF"], {"R": "sand", "F": "#fuel_all"})
    shaped("forge", "stone", 1, ["RF"], {"R": "cobblestone", "F": "#fuel_all"})
    # divers
    shaped("workbench", "hay_block", 1, ["WWW", "WWW", "WWW"], {"W": "wheat_grain"})
    shapeless("hand", "wheat_grain", 9, ["hay_block"])


tool_recipes()
armor_recipes()
misc_recipes()
# les placeholders « if False » ci-dessus retournent None : on filtre.
RECIPES[:] = [r for r in RECIPES if r]

# ------------------------------------------------------------------ extra items qui doivent exister
EXTRA_ITEMS = []  # (nom, painter, props)
# grain de blé
item("wheat_grain", lambda: noisy(art([
    "................",
    "................",
    "......g.........",
    ".....gGg..g.....",
    "....gGGg.gGg....",
    "...gGGg..gGGg...",
    "..gGGg....gGg...",
    ".sgGg.s....g....",
    ".s.g..s.........",
    "..s...s.........",
    "..s..s..........",
    "...ss...........",
    "................",
    "................",
    "................",
    "................",
], {"G": (232, 200, 90), "g": (170, 140, 56), "s": (110, 150, 56)}), "wheat_grain", 0.05), None)


def build():
    names = block_names()
    mobs = []
    for f in ("mobs_defaults.json", "mobs_nb.json"):
        with open(os.path.join(ASSETS, f), encoding="utf-8") as fh:
            data = json.load(fh)
        mobs += [m["id"] for m in data["mobs"]]
    # ----- items
    first = 300
    items_json = []
    item_names = set()
    for i, (n, painter) in enumerate(ITEMS):
        if n in item_names:
            raise SystemExit("objet en double : " + n)
        item_names.add(n)
        items_json.append({"id": first + i, "name": n, "texture": "item_" + n})
        painter().save("item_" + n)
    with open(os.path.join(ASSETS, "items_survival.json"), "w", encoding="utf-8") as fh:
        json.dump(items_json, fh, indent=1)
        fh.write("\n")
    # ----- blocs de cultures (ids 210+)
    blocks = []
    crops = {}
    bid = 210
    for kind, (stages, mature, stem) in CROPS.items():
        for s in range(stages):
            name = f"{kind}_{s}" if s < stages - 1 else (kind if kind == "wheat" else f"{kind}_{s}")
            tex = f"crop_{kind}_{s}"
            if name == "wheat":
                # le stade mûr est le bloc `wheat` d'origine (id 106) : pas de nouveau bloc.
                crop_img(kind, s, stages, mature, stem).save("wheat_stage7")
                continue
            crop_img(kind, s, stages, mature, stem).save(tex)
            blocks.append({
                "id": bid, "name": name, "textures": [tex] * 6, "transparent": True, "crossShaped": True,
                "grassTinted": False, "leavesTinted": False, "solid": False, "lightAttenuation": 0,
                "lightEmission": 0,
            })
            bid += 1
        # chaîne de croissance
        chain = [f"{kind}_{s}" for s in range(stages - 1)] + ["wheat" if kind == "wheat" else f"{kind}_{stages - 1}"]
        for a, b in zip(chain, chain[1:]):
            crops[a] = {"next": b, "grow": 100.0}
    if bid > 230:
        raise SystemExit("trop de blocs de culture")
    with open(os.path.join(ASSETS, "blocks_survival.json"), "w", encoding="utf-8") as fh:
        json.dump(blocks, fh, indent=1)
        fh.write("\n")
    # ----- règles
    all_blocks = names + [b["name"] for b in blocks]
    R = rules(all_blocks)

    def d(item_, mn=1, mx=1, chance=1.0):
        return {"item": item_, "min": mn, "max": mx, "chance": chance}

    for kind, (stages, mature, stem) in CROPS.items():
        for s in range(stages - 1):
            R[f"{kind}_{s}"] = {"hardness": 0.0, "tool": "none", "tier": 0,
                                "drops": [d("wheat_seeds" if kind == "wheat" else ("potato" if kind == "potato" else "carrot"),
                                            1, 1, 0.5 if kind == "wheat" else 0.0)]}
    R["potato_3"] = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d("potato", 2, 4), d("turnip", 0, 1, 0.1)]}
    R["carrot_3"] = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d("carrot", 2, 4)]}
    R["wheat"] = {"hardness": 0.0, "tool": "none", "tier": 0, "drops": [d("wheat_grain", 1, 2), d("wheat_seeds", 1, 3, 0.9)]}
    # les rendements sont validés : chaque objet cité existe (bloc ou objet pur)
    base_items = {i["name"] for i in json.load(open(os.path.join(ASSETS, "items.json"), encoding="utf-8"))}
    valid = set(names) | item_names | base_items | {b["name"] for b in blocks}
    for bn, r in R.items():
        for dr in r["drops"]:
            if dr["item"] not in valid:
                raise SystemExit(f"butin inconnu {dr['item']} pour {bn}")
    L = loot_tables(mobs)
    for m, t in L.items():
        for dr in t:
            if dr["item"] not in valid:
                raise SystemExit(f"butin inconnu {dr['item']} pour {m}")
    props = {"items": PROPS, "blocks": R, "mobs": L, "crops": crops, "biomes": BIOME_TEMP}
    for n in PROPS:
        if n not in item_names:
            raise SystemExit("propriétés d'un objet inconnu : " + n)
    with open(os.path.join(ASSETS, "survival.json"), "w", encoding="utf-8") as fh:
        json.dump(props, fh, indent=1, sort_keys=True)
        fh.write("\n")
    with open(os.path.join(ASSETS, "recipes_survival.json"), "w", encoding="utf-8") as fh:
        json.dump({"tags": TAGS, "recipes": RECIPES}, fh, indent=1)
        fh.write("\n")
    print(f"{len(items_json)} objets, {len(blocks)} blocs, {len(R)} règles de blocs, {len(L)} mobs, {len(RECIPES)} recettes")


if __name__ == "__main__":
    build()
