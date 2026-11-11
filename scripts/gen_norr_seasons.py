#!/usr/bin/env python3
"""Textures provisoires `norr_*` de la surcouche saisonnière Norrcraft (session 3.2).

Cubes de glace (`nyis` neuve et sombre, `is` portante et bleutée, `tjockis` épaisse et
enneigée) et deux sprites en croix : `kvistar` (ramilles nues d'un feuillu en hiver) et `ris`
(rameau feuillu sans fruits, gris clair teinté par l'herbe du biome). Dessin 16×16 agrandi ×2
en 32×32, graine fixe par nom, sans dépendance.

Usage (Docker, image rust:1) :
  docker run --rm -v "$(pwd -W):/w" -w /w rust:1 python3 scripts/gen_norr_seasons.py
"""
import os
import random
import struct
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "resource_packs/Default/assets/minecraft/textures/block")
S = 16


def rng_for(name):
    h = 1469598103934665603
    for c in name.encode():
        h = ((h ^ c) * 1099511628211) % (2**64)
    return random.Random(h)


def shade(c, k):
    return tuple(max(0, min(255, int(v * k))) for v in c)


def mix(a, b, t):
    return tuple(int(x + (y - x) * t) for x, y in zip(a, b))


def ice_block(name, base, deep, crack, snow=0.0, cracks=0):
    """Cube de glace : dégradé de bruit entre `base` et `deep`, fissures, poudre de neige."""
    rng = rng_for(name)
    px = [[None] * S for _ in range(S)]
    for y in range(S):
        for x in range(S):
            t = rng.random() * 0.6 + 0.4 * (((x // 4) + (y // 4)) % 2)
            c = mix(base, deep, t)
            if snow and rng.random() < snow:
                c = mix(c, (244, 248, 252), 0.8)
            px[y][x] = c + (255,)
    for _ in range(cracks):
        x, y = rng.randrange(S), rng.randrange(S)
        for _ in range(rng.randint(4, 8)):
            if 0 <= x < S and 0 <= y < S:
                px[y][x] = crack + (255,)
            x += rng.choice((-1, 0, 1))
            y += rng.choice((0, 1))
    return px


def sprite():
    return [[None] * S for _ in range(S)]


def put(px, x, y, c):
    if 0 <= x < S and 0 <= y < S:
        px[y][x] = c + (255,)


def line(px, x0, y0, x1, y1, c):
    n = max(abs(x1 - x0), abs(y1 - y0), 1)
    for i in range(n + 1):
        put(px, round(x0 + (x1 - x0) * i / n), round(y0 + (y1 - y0) * i / n), c)


def twigs():
    """Ramilles nues : un tronc grêle d'où partent des branches fines."""
    rng = rng_for("kvistar")
    px = sprite()
    brown, dark = (96, 78, 60), (66, 52, 40)
    line(px, 8, 15, 8, 8, dark)
    for _ in range(6):
        y = rng.randint(6, 13)
        dx = rng.choice((-1, 1))
        line(px, 8, y, 8 + dx * rng.randint(3, 6), y - rng.randint(2, 5), brown)
    for _ in range(4):
        x, y = rng.randint(3, 13), rng.randint(1, 8)
        put(px, x, y, shade(brown, 0.9))
    return px


def sprig():
    """Rameau feuillu : gris clair, multiplié par la teinte d'herbe du biome."""
    rng = rng_for("ris")
    px = sprite()
    stem, leaf = (150, 150, 150), (205, 205, 205)
    line(px, 8, 15, 8, 6, stem)
    for _ in range(5):
        y = rng.randint(7, 13)
        dx = rng.choice((-1, 1))
        line(px, 8, y, 8 + dx * 3, y - 2, stem)
    for _ in range(22):
        put(px, rng.randint(4, 12), rng.randint(3, 12), shade(leaf, 0.85 + 0.3 * rng.random()))
    return px


def write_png(path, img):
    w = h = 2 * S
    raw = bytearray()
    for y in range(h):
        raw.append(0)
        for x in range(w):
            p = img[y // 2][x // 2]
            raw += bytes(p if p else (0, 0, 0, 0))

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def main():
    os.makedirs(OUT, exist_ok=True)
    textures = {
        "nyis": ice_block("nyis", (46, 66, 92), (24, 36, 58), (140, 170, 200), 0.0, 1),
        "is": ice_block("is", (150, 190, 226), (104, 150, 200), (232, 244, 252), 0.0, 4),
        "tjockis": ice_block("tjockis", (214, 228, 240), (176, 204, 228), (120, 160, 200), 0.35, 3),
        "kvistar": twigs(),
        "ris": sprig(),
    }
    for n, img in textures.items():
        write_png(os.path.join(OUT, "norr_" + n + ".png"), img)
    print(f"{len(textures)} textures écrites dans {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
