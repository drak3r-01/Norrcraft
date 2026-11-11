#!/usr/bin/env python3
"""Textures provisoires `norr_*` des blocs de terrain Norrcraft (session 2.1).

Procédurales, sans dépendance (PNG écrit à la main avec zlib) : dessin 16×16 agrandi ×2 en
32×32, graine fixe par nom, résultat reproductible. Les textures teintées par le biome
(`norr_gras_top`, `norr_mossa_top`) sont en niveaux de gris clairs : le mailleur les multiplie
par la teinte d'herbe du biome (× 2,2), comme `grass_block_top`.

Usage (Docker, image rust:1) :
  docker run --rm -v "$(pwd -W):/w" -w /w rust:1 python3 scripts/gen_norr_textures.py
Les PNG sont écrits dans resource_packs/Default/assets/minecraft/textures/block/.
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


def value_noise(rng, cell):
    """Bruit de valeurs périodique S×S à cellules de `cell` pixels, dans [0, 1]."""
    n = max(1, S // cell)
    g = [[rng.random() for _ in range(n)] for _ in range(n)]
    out = [[0.0] * S for _ in range(S)]
    for y in range(S):
        for x in range(S):
            fx, fy = x / cell, y / cell
            x0, y0 = int(fx) % n, int(fy) % n
            x1, y1 = (x0 + 1) % n, (y0 + 1) % n
            tx, ty = fx - int(fx), fy - int(fy)
            tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
            a = g[y0][x0] * (1 - tx) + g[y0][x1] * tx
            b = g[y1][x0] * (1 - tx) + g[y1][x1] * tx
            out[y][x] = a * (1 - ty) + b * ty
    return out


def field(rng, cell=4, grain=0.35):
    """Bruit de valeurs + grain blanc, normalisé dans [0, 1]."""
    v = value_noise(rng, cell)
    f = [[(1 - grain) * v[y][x] + grain * rng.random() for x in range(S)] for y in range(S)]
    lo = min(min(r) for r in f)
    hi = max(max(r) for r in f)
    return [[(f[y][x] - lo) / (hi - lo + 1e-9) for x in range(S)] for y in range(S)]


def shade(c, k):
    return tuple(max(0, min(255, int(v * k))) for v in c)


def palette(c, n=5, spread=0.18):
    return [shade(c, 1 - spread + 2 * spread * i / (n - 1)) for i in range(n)]


def paint(name, base, cell=4, grain=0.35, spread=0.18, specks=None):
    """Image S×S : tons de `base` choisis par bruit, puis taches `(couleur, part)`."""
    rng = rng_for(name)
    f = field(rng, cell, grain)
    tones = palette(base, spread=spread)
    img = [[tones[min(len(tones) - 1, int(f[y][x] * len(tones)))] for x in range(S)] for y in range(S)]
    for color, share in specks or []:
        for y in range(S):
            for x in range(S):
                if rng.random() < share:
                    img[y][x] = shade(color, 0.9 + 0.2 * rng.random())
    return img


def bands(img, rng, colors, period=4, wobble=1):
    """Litage horizontal ondulé (gneiss)."""
    offs = [rng.randint(-wobble, wobble) for _ in range(S)]
    for y in range(S):
        for x in range(S):
            k = ((y + offs[x]) // period) % len(colors)
            if colors[k] is not None:
                img[y][x] = shade(colors[k], 0.92 + 0.16 * rng.random())
    return img


def cracks(img, rng, color, n=3):
    """Fissures : marches aléatoires sombres."""
    for _ in range(n):
        x, y = rng.randrange(S), rng.randrange(S)
        for _ in range(rng.randint(5, 10)):
            img[y % S][x % S] = color
            x += rng.choice((-1, 0, 1))
            y += rng.choice((0, 1))
    return img


def side_with_top(name, body, top, depth_fn):
    """Côté d'un bloc à peau : le haut prend la peau sur une profondeur irrégulière."""
    rng = rng_for(name + "_edge")
    img = [row[:] for row in body]
    for x in range(S):
        d = depth_fn(rng)
        for y in range(d):
            img[y][x] = top[y][x]
    return img


def write_png(path, img):
    """PNG RGBA 32×32 (chaque pixel 16×16 doublé)."""
    w = h = 2 * S
    raw = bytearray()
    for y in range(h):
        raw.append(0)
        for x in range(w):
            r, g, b = img[y // 2][x // 2]
            raw += bytes((r, g, b, 255))

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def make_all():
    t = {}
    # Roches.
    t["norr_rodgranit"] = paint("rodgranit", (176, 104, 92), cell=2, grain=0.6,
                                specks=[((60, 50, 50), 0.10), ((225, 190, 175), 0.08)])
    g = paint("gnejs", (120, 120, 124), cell=2, grain=0.5)
    t["norr_gnejs"] = bands(g, rng_for("gnejs_b"), [(150, 148, 146), None, (82, 82, 88), None], 3)
    t["norr_kalksten"] = cracks(paint("kalksten", (206, 200, 178), cell=4, grain=0.3, spread=0.1),
                                rng_for("kalk_c"), (150, 144, 126), 2)
    t["norr_djupberg"] = paint("djupberg", (58, 60, 66), cell=2, grain=0.5,
                               specks=[((95, 98, 104), 0.05)])
    t["norr_sprucket_berg"] = cracks(paint("sprucket", (132, 130, 128), cell=4, grain=0.5),
                                     rng_for("spr_c"), (60, 58, 58), 5)
    # Sols.
    t["norr_svartsand"] = paint("svartsand", (52, 50, 54), cell=1, grain=0.8,
                                specks=[((110, 106, 100), 0.05)])
    t["norr_blalera"] = paint("blalera", (112, 124, 136), cell=8, grain=0.2, spread=0.08)
    jord = paint("jord", (112, 80, 52), cell=2, grain=0.6)
    t["norr_jord"] = jord
    t["norr_aska"] = paint("aska", (74, 72, 70), cell=2, grain=0.6,
                           specks=[((30, 28, 28), 0.10), ((150, 146, 140), 0.05)])
    t["norr_rodmull"] = paint("rodmull", (140, 70, 44), cell=2, grain=0.6,
                              specks=[((90, 40, 30), 0.08)])
    t["norr_slagg"] = paint("slagg", (60, 56, 64), cell=2, grain=0.5,
                            specks=[((40, 90, 80), 0.08), ((120, 70, 40), 0.06)])
    t["norr_sno"] = paint("sno", (236, 240, 246), cell=4, grain=0.2, spread=0.04)
    t["norr_havsis"] = cracks(paint("havsis", (176, 208, 228), cell=8, grain=0.15, spread=0.06),
                              rng_for("havsis_c"), (225, 240, 250), 3)
    # Peaux (gris clair pour les teintées).
    gras_top = paint("gras_top", (122, 122, 122), cell=1, grain=0.7, spread=0.22)
    t["norr_gras_top"] = gras_top
    gras_edge = paint("gras_edge", (96, 140, 60), cell=1, grain=0.7)
    t["norr_gras_side"] = side_with_top("gras", jord, gras_edge, lambda r: r.randint(2, 4))
    mossa_top = paint("mossa_top", (104, 104, 104), cell=2, grain=0.5, spread=0.25)
    t["norr_mossa_top"] = mossa_top
    moran = paint("moran_body", (120, 98, 72), cell=2, grain=0.6)
    mossa_edge = paint("mossa_edge", (78, 112, 52), cell=2, grain=0.5)
    t["norr_mossa_side"] = side_with_top("mossa", moran, mossa_edge, lambda r: r.randint(3, 5))
    barr = paint("barrmatta", (120, 86, 56), cell=1, grain=0.8,
                 specks=[((90, 62, 40), 0.15), ((150, 112, 72), 0.08)])
    t["norr_barrmatta"] = barr
    t["norr_barrmatta_side"] = side_with_top("barrmatta", moran, barr, lambda r: r.randint(1, 3))
    lov = paint("lovforna", (136, 90, 44), cell=2, grain=0.7,
                specks=[((170, 120, 50), 0.12), ((90, 60, 30), 0.12)])
    t["norr_lovforna"] = lov
    t["norr_lovforna_side"] = side_with_top("lovforna", jord, lov, lambda r: r.randint(1, 3))
    lav = paint("lavhed", (150, 160, 128), cell=2, grain=0.6,
                specks=[((70, 92, 52), 0.16), ((206, 212, 192), 0.12)])
    t["norr_lavhed"] = lav
    t["norr_lavhed_side"] = side_with_top("lavhed", moran, lav, lambda r: r.randint(2, 4))
    vit = paint("vitmossa", (150, 156, 80), cell=2, grain=0.6,
                specks=[((170, 70, 60), 0.10), ((190, 180, 100), 0.08)])
    t["norr_vitmossa"] = vit
    torv = paint("torv_body", (78, 56, 38), cell=2, grain=0.5)
    t["norr_vitmossa_side"] = side_with_top("vitmossa", torv, vit, lambda r: r.randint(3, 5))
    make_subsoil(t)
    return t


def clusters(img, rng, color, n, size=2):
    """Veinules : `n` petits amas de `color` (minerai dans la roche hôte)."""
    for _ in range(n):
        x, y = rng.randrange(S), rng.randrange(S)
        for _ in range(rng.randint(2, 2 + size)):
            img[y % S][x % S] = shade(color, 0.88 + 0.24 * rng.random())
            x += rng.choice((-1, 0, 1))
            y += rng.choice((-1, 0, 1))
    return img


def make_subsoil(t):
    """Roches de province et minerais du sous-sol (session 2.2)."""
    # Roches.
    sand = paint("sandsten", (196, 142, 100), cell=2, grain=0.45)
    t["norr_sandsten"] = bands(sand, rng_for("sandsten_b"),
                               [(206, 156, 108), None, (176, 120, 84), None, None], 2)
    alun = paint("alunskiffer", (50, 52, 58), cell=2, grain=0.5, spread=0.12)
    t["norr_alunskiffer"] = bands(alun, rng_for("alun_b"), [None, (38, 38, 44), None, (74, 76, 84)], 2, 0)
    t["norr_gronsten"] = paint("gronsten", (68, 102, 84), cell=2, grain=0.55,
                               specks=[((40, 66, 54), 0.12), ((150, 170, 140), 0.05)])
    t["norr_porfyr"] = paint("porfyr", (130, 62, 72), cell=2, grain=0.55,
                             specks=[((226, 204, 190), 0.12), ((86, 38, 50), 0.08)])
    t["norr_diabas"] = paint("diabas", (30, 30, 36), cell=2, grain=0.4, spread=0.14,
                             specks=[((62, 64, 72), 0.04)])
    t["norr_krita"] = cracks(paint("krita", (238, 236, 226), cell=4, grain=0.2, spread=0.05),
                             rng_for("krita_c"), (206, 204, 190), 2)
    t["norr_taljsten"] = paint("taljsten", (126, 140, 128), cell=4, grain=0.25, spread=0.1,
                               specks=[((170, 186, 170), 0.06)])
    t["norr_kvarts"] = paint("kvarts", (232, 230, 236), cell=4, grain=0.2, spread=0.06,
                             specks=[((255, 255, 255), 0.10), ((200, 196, 208), 0.08)])
    t["norr_svavel"] = paint("svavel", (222, 202, 60), cell=2, grain=0.5, spread=0.14,
                             specks=[((248, 236, 120), 0.10), ((170, 150, 30), 0.08)])
    # Minerais : roche hôte + veinules de minerai.
    host = lambda n: paint(n + "_host", (122, 122, 126), cell=2, grain=0.5)
    gravel = lambda n: paint(n + "_host", (132, 124, 116), cell=1, grain=0.7)

    def ore(name, base, color, n, size=2):
        t["norr_" + name] = clusters(base(name), rng_for(name + "_ore"), color, n, size)

    ore("jarnmalm", host, (96, 54, 44), 9)
    ore("rik_jarnmalm", host, (70, 36, 32), 15, 3)
    ore("kopparmalm", host, (200, 120, 60), 8)
    clusters(t["norr_kopparmalm"], rng_for("kopparmalm_g"), (70, 168, 140), 5)
    ore("silvermalm", host, (226, 230, 236), 8)
    ore("koboltmalm", host, (60, 90, 190), 8)
    ore("volframmalm", lambda n: paint(n + "_host", (74, 70, 70), cell=2, grain=0.5),
        (190, 112, 50), 9)
    ore("myrmalm", lambda n: paint(n + "_host", (96, 70, 44), cell=2, grain=0.55),
        (176, 98, 40), 12, 3)
    ore("sjomalm", lambda n: paint(n + "_host", (84, 78, 62), cell=1, grain=0.65),
        (140, 96, 48), 11, 3)
    ore("guldkvarts", lambda n: paint(n + "_host", (232, 230, 236), cell=4, grain=0.2, spread=0.06),
        (240, 196, 50), 7)
    ore("guldgrus", gravel, (244, 200, 56), 6, 1)
    t["norr_flinta"] = cracks(paint("flinta", (48, 50, 58), cell=4, grain=0.35, spread=0.1,
                                    specks=[((96, 98, 104), 0.06)]),
                              rng_for("flinta_c"), (150, 150, 150), 2)


def main():
    os.makedirs(OUT, exist_ok=True)
    tex = make_all()
    for name, img in sorted(tex.items()):
        write_png(os.path.join(OUT, name + ".png"), img)
    print(f"{len(tex)} textures écrites dans {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
    main()
