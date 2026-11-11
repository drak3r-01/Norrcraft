#!/usr/bin/env python3
"""Textures provisoires `norr_*` de la strate basse Norrcraft (session 2.4, 37 plantes).

Sprites 16×16 transparents (en croix), agrandis ×2 en 32×32, dessinés à la main par primitives
(tiges, touffes, corolles, chapeaux), graine fixe par nom : résultat reproductible. `hogt_gras`
et `ormbunke` sont en gris clair : le mailleur les multiplie par la teinte d'herbe du biome.

Usage (Docker, image rust:1) :
  docker run --rm -v "$(pwd -W):/w" -w /w rust:1 python3 scripts/gen_norr_flora.py
"""
import os
import random
import struct
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "resource_packs/Default/assets/minecraft/textures/block")
S = 16

GREEN = (74, 118, 52)
DGREEN = (52, 88, 40)
LGREEN = (110, 152, 70)
STEM = (86, 124, 58)
BROWN = (110, 84, 56)


def rng_for(name):
    h = 1469598103934665603
    for c in name.encode():
        h = ((h ^ c) * 1099511628211) % (2**64)
    return random.Random(h)


def shade(c, k):
    return tuple(max(0, min(255, int(v * k))) for v in c)


class Sprite:
    def __init__(self, name):
        self.px = [[None] * S for _ in range(S)]
        self.rng = rng_for(name)

    def put(self, x, y, c, jitter=0.12):
        if 0 <= x < S and 0 <= y < S:
            k = 1 - jitter + 2 * jitter * self.rng.random()
            self.px[y][x] = shade(c, k) + (255,)

    def line(self, x0, y0, x1, y1, c):
        n = max(abs(x1 - x0), abs(y1 - y0), 1)
        for i in range(n + 1):
            self.put(round(x0 + (x1 - x0) * i / n), round(y0 + (y1 - y0) * i / n), c, 0.08)

    def blob(self, cx, cy, rx, ry, c, fill=0.9):
        for y in range(S):
            for x in range(S):
                d = ((x - cx) / max(rx, 0.5)) ** 2 + ((y - cy) / max(ry, 0.5)) ** 2
                if d <= 1.0 and self.rng.random() < fill:
                    self.put(x, y, c)

    def dots(self, cx, cy, rx, ry, c, n):
        for _ in range(n):
            self.put(round(cx + self.rng.uniform(-rx, rx)), round(cy + self.rng.uniform(-ry, ry)), c, 0.05)

    def tuft(self, base=15, h=7, spread=5, c=GREEN, n=7):
        """Touffe de brins partant du sol."""
        for i in range(n):
            x = 8 + self.rng.randint(-spread // 2, spread // 2)
            tx = x + self.rng.randint(-spread // 2, spread // 2)
            self.line(x, base, tx, base - self.rng.randint(h // 2, h), c)


def flower(s, head, h=9, center=None, petal=2, x=8, leaf=GREEN):
    s.line(x, 15, x, 15 - h, STEM)
    s.line(x, 14, x - 2, 12, leaf)
    s.line(x, 13, x + 2, 11, leaf)
    top = 15 - h
    s.blob(x, top, petal, petal, head, 0.95)
    if center:
        s.put(x, top, center, 0.03)


def spike(s, head, h=11, n=8):
    s.line(8, 15, 8, 15 - h, STEM)
    s.line(8, 14, 5, 12, GREEN)
    s.line(8, 13, 11, 11, GREEN)
    for i in range(n):
        y = 15 - h + i
        s.put(8 + (i % 2) * 2 - 1, y, head)
        s.put(8 + (1 - i % 2) * 2 - 1 + 1, y, shade(head, 0.85))


def berry_bush(s, berry, h=6, leaf=GREEN, n=7):
    s.tuft(15, h, 7, leaf, 10)
    s.blob(8, 12, 5, 3, leaf, 0.8)
    s.dots(8, 11, 5, 3, berry, n)


def mushroom(s, cap, stem=(226, 218, 196), w=3, h=6, dots=None, funnel=False):
    s.line(8, 15, 8, 15 - h + 1, stem)
    s.line(9, 15, 9, 15 - h + 1, shade(stem, 0.9))
    top = 15 - h
    s.blob(8, top, w, 2 if not funnel else 3, cap, 1.0)
    if funnel:
        s.put(8, top - 1, shade(cap, 0.7), 0.03)
    if dots:
        s.dots(8, top, w - 1, 1, dots, 3)


def lichen(s, c, hang=False):
    if hang:
        for x in range(3, 14, 2):
            s.line(x, 0, x + s.rng.randint(-1, 1), s.rng.randint(8, 14), c)
    else:
        s.blob(8, 13, 6, 2, c, 0.9)
        s.blob(6, 12, 3, 2, shade(c, 1.1), 0.8)
        s.blob(11, 12, 3, 2, shade(c, 0.9), 0.8)


def make(name):
    s = Sprite(name)
    f = {
        "blabar": lambda: berry_bush(s, (52, 62, 130), 6, GREEN, 8),
        "lingon": lambda: berry_bush(s, (190, 36, 44), 5, DGREEN, 8),
        "hjortron": lambda: (s.tuft(15, 6, 6, LGREEN, 6), s.blob(8, 9, 2, 2, (238, 168, 40), 1.0),
                              s.line(8, 15, 8, 10, STEM)),
        "smultron": lambda: berry_bush(s, (214, 44, 56), 4, LGREEN, 5),
        "tranbar": lambda: berry_bush(s, (178, 30, 52), 4, DGREEN, 6),
        "nypon": lambda: (s.line(8, 15, 7, 6, BROWN), s.line(8, 12, 12, 8, BROWN), s.blob(7, 7, 4, 3, GREEN, 0.75),
                          s.blob(12, 9, 3, 2, GREEN, 0.75), s.dots(8, 8, 5, 3, (210, 40, 40), 6)),
        "havtorn": lambda: (s.line(8, 15, 8, 5, (110, 108, 96)), s.blob(8, 7, 4, 5, (130, 150, 110), 0.7),
                            s.dots(8, 7, 4, 5, (244, 150, 30), 11)),
        "ljung": lambda: (s.tuft(15, 8, 8, (84, 92, 56), 9), s.dots(8, 8, 5, 4, (178, 80, 150), 16)),
        "renlav": lambda: lichen(s, (190, 200, 176)),
        "skagglav": lambda: lichen(s, (160, 172, 120), hang=True),
        "kantarell": lambda: mushroom(s, (232, 176, 44), (236, 196, 90), 3, 5, funnel=True),
        "karljohan": lambda: mushroom(s, (128, 84, 48), (226, 214, 184), 4, 6),
        "flugsvamp": lambda: mushroom(s, (204, 36, 32), (240, 236, 224), 4, 6, (250, 250, 250)),
        "vitsippa": lambda: flower(s, (246, 246, 250), 6, (236, 200, 60), 2),
        "blasippa": lambda: flower(s, (92, 120, 214), 5, (240, 240, 250), 2),
        "midsommarblomster": lambda: (flower(s, (214, 96, 150), 7, None, 1, 5), flower(s, (250, 250, 250), 9, (240, 200, 50), 2, 10),
                                      flower(s, (90, 120, 220), 5, None, 1, 8)),
        "prastkrage": lambda: flower(s, (250, 250, 250), 8, (244, 200, 40), 3),
        "blaklocka": lambda: flower(s, (110, 110, 214), 9, None, 2),
        "rodklover": lambda: flower(s, (206, 70, 110), 6, None, 2),
        "rallarros": lambda: spike(s, (206, 70, 160), 12, 9),
        "angsull": lambda: (s.tuft(15, 8, 4, (110, 130, 80), 5), s.blob(8, 7, 2, 2, (250, 250, 250), 1.0),
                            s.line(8, 15, 8, 8, STEM)),
        "kvanne": lambda: (s.line(8, 15, 8, 5, (120, 140, 90)), s.line(8, 14, 4, 11, GREEN), s.line(8, 13, 12, 10, GREEN),
                           s.blob(8, 4, 4, 2, (226, 238, 214), 0.9)),
        "orkide": lambda: flower(s, (186, 70, 170), 8, (250, 230, 250), 2),
        "fjallsippa": lambda: flower(s, (252, 252, 252), 4, (244, 204, 44), 2),
        "isranunkel": lambda: flower(s, (244, 244, 250), 4, (230, 190, 50), 2),
        "vass": lambda: (s.line(5, 15, 5, 1, (150, 150, 80)), s.line(8, 15, 9, 0, (160, 160, 88)), s.line(11, 15, 11, 2, (140, 140, 74)),
                         s.blob(9, 1, 1, 1, (130, 100, 60), 1.0), s.line(8, 14, 3, 9, GREEN), s.line(9, 13, 14, 8, GREEN)),
        "kaveldun": lambda: (s.line(8, 15, 8, 3, (100, 140, 70)), s.blob(8, 5, 1, 3, (110, 70, 40), 1.0),
                             s.line(7, 14, 4, 6, GREEN), s.line(9, 14, 12, 6, GREEN)),
        "nackros": lambda: (s.blob(8, 12, 6, 2, (60, 130, 66), 0.95), s.blob(9, 11, 2, 1, (250, 244, 250), 1.0),
                            s.put(9, 11, (246, 200, 60), 0.02)),
        "blastang": lambda: (s.line(8, 15, 5, 3, (118, 110, 44)), s.line(8, 15, 11, 4, (130, 120, 50)),
                             s.line(8, 15, 8, 6, (104, 100, 40)), s.dots(8, 8, 4, 4, (150, 140, 60), 6)),
        "sockertang": lambda: (s.line(8, 15, 6, 1, (140, 110, 56)), s.line(9, 15, 11, 3, (126, 100, 50)),
                               s.line(7, 15, 9, 6, (150, 120, 60))),
        "hogt_gras": lambda: (s.tuft(15, 12, 9, (140, 140, 140), 13)),
        "ormbunke": lambda: [s.line(8, 15, 8 + dx, 4 + abs(dx) // 2, (125, 125, 125)) or s.dots(8 + dx // 2, 9 + abs(dx) // 3, 3, 3, (140, 140, 140), 6)
                             for dx in (-6, -3, 0, 3, 6)],
        "ramslok": lambda: (s.blob(5, 12, 3, 3, GREEN, 0.95), s.blob(11, 12, 3, 3, LGREEN, 0.95),
                            s.line(8, 15, 8, 5, STEM), s.blob(8, 4, 3, 2, (248, 248, 250), 0.9)),
        "glasort": lambda: [s.blob(x, 11 - h // 2, 1, h // 2 + 1, (96, 170, 96), 1.0) for x, h in ((5, 6), (8, 9), (11, 7))],
        "dvargbjork": lambda: (s.line(8, 15, 6, 9, BROWN), s.line(8, 14, 11, 8, BROWN), s.blob(6, 8, 3, 2, (108, 138, 62), 0.85),
                               s.blob(11, 7, 3, 2, (120, 146, 66), 0.85)),
        "lupin": lambda: spike(s, (116, 80, 200), 12, 10),
        "mjodort": lambda: (s.line(8, 15, 8, 4, STEM), s.line(8, 12, 4, 9, GREEN), s.blob(8, 4, 4, 2, (246, 240, 214), 0.85)),
    }[name]
    f()
    return s.px


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


NAMES = (
    "blabar lingon hjortron smultron tranbar nypon havtorn ljung renlav skagglav kantarell "
    "karljohan flugsvamp vitsippa blasippa midsommarblomster prastkrage blaklocka rodklover "
    "rallarros angsull kvanne orkide fjallsippa isranunkel vass kaveldun nackros blastang "
    "sockertang hogt_gras ormbunke ramslok glasort dvargbjork lupin mjodort"
).split()


def main():
    os.makedirs(OUT, exist_ok=True)
    for n in NAMES:
        write_png(os.path.join(OUT, "norr_" + n + ".png"), make(n))
    print(f"{len(NAMES)} textures écrites dans {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
