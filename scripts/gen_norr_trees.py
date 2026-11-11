#!/usr/bin/env python3
"""Textures provisoires `norr_*` des arbres Norrcraft (session 2.3) : troncs et feuillages.

Même méthode que `gen_norr_textures.py` (procédural, sans dépendance, graine fixe par nom) dont
on réutilise les outils. Les feuillages sont des niveaux de gris teintés (le mailleur les
multiplie par la teinte de feuilles du biome × 2,2, comme `oak_leaves`) ; chaque essence a sa
luminosité et sa nuance propres. Troncs : écorce verticale (côtés) et cernes (dessus/dessous).

Usage (Docker, image rust:1) :
  docker run --rm -v "$(pwd -W):/w" -w /w rust:1 python3 scripts/gen_norr_trees.py
Les PNG sont écrits dans resource_packs/Default/assets/minecraft/textures/block/.
"""
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gen_norr_textures as g  # noqa: E402

S = g.S


def bark(name, base, spread=0.2, groove=None, dashes=None, flakes=None, moss=None):
    """Écorce : colonnes de tons cohérents verticalement, sillons, tirets, plaques, mousse."""
    rng = g.rng_for(name)
    col = [rng.random() for _ in range(S)]
    img = []
    for y in range(S):
        row = []
        for x in range(S):
            k = 0.55 * col[x] + 0.45 * rng.random()
            row.append(g.shade(base, 1 - spread + 2 * spread * k))
        img.append(row)
    if groove:
        color, share = groove
        for x in range(S):
            if rng.random() < share:
                for y in range(S):
                    if rng.random() < 0.85:
                        img[y][x] = g.shade(color, 0.9 + 0.2 * rng.random())
    if dashes:
        color, n = dashes
        for _ in range(n):
            x, y, w = rng.randrange(S), rng.randrange(S), rng.randint(2, 4)
            for i in range(w):
                img[y][(x + i) % S] = color
    if flakes:
        color, share = flakes
        for y in range(S):
            for x in range(S):
                if rng.random() < share:
                    img[y][x] = g.shade(color, 0.85 + 0.3 * rng.random())
    if moss:
        color, share = moss
        for y in range(S):
            for x in range(S):
                # Mousse plus dense en bas (pied du tronc).
                if rng.random() < share * (0.4 + 0.9 * y / S):
                    img[y][x] = g.shade(color, 0.85 + 0.3 * rng.random())
    return img


def rings(name, core, bark_color, period=3):
    """Dessus de tronc : cernes concentriques et bord d'écorce."""
    rng = g.rng_for(name + "_rings")
    img = []
    c = (S - 1) / 2
    for y in range(S):
        row = []
        for x in range(S):
            d = math.hypot(x - c, y - c)
            if d > S / 2 - 1.2:
                row.append(g.shade(bark_color, 0.85 + 0.3 * rng.random()))
            else:
                k = 0.88 + 0.12 * math.sin(d * 2 * math.pi / period) + 0.04 * rng.random()
                row.append(g.shade(core, k))
        img.append(row)
    return img


def leaves(name, base, grain=0.45, spread=0.28, dark=None):
    """Feuillage : gris teinté, pixels sombres (trous d'ombre) et clairs."""
    rng = g.rng_for(name)
    img = g.paint(name, base, cell=2, grain=grain, spread=spread)
    for y in range(S):
        for x in range(S):
            r = rng.random()
            if r < 0.12:
                img[y][x] = g.shade(dark or base, 0.55)
            elif r > 0.93:
                img[y][x] = g.shade(base, 1.22)
    return img


def make_trees(t):
    # Troncs : côté et dessus.
    def trunk(name, side, top_core, top_bark):
        t["norr_" + name] = side
        t["norr_" + name + "_top"] = rings(name, top_core, top_bark)

    trunk("gran_stam", bark("gran_stam", (92, 70, 56), 0.22, groove=((56, 42, 34), 0.25),
                            flakes=((120, 88, 70), 0.06)), (176, 140, 98), (80, 60, 48))
    trunk("tall_stam", bark("tall_stam", (112, 88, 72), 0.22, groove=((70, 54, 44), 0.2),
                            flakes=((140, 110, 88), 0.1)), (206, 150, 96), (100, 76, 60))
    trunk("tall_ovre", bark("tall_ovre", (206, 124, 66), 0.16, flakes=((236, 170, 96), 0.18),
                            groove=((150, 82, 44), 0.12)), (214, 156, 98), (180, 104, 56))
    trunk("bjork_stam", bark("bjork_stam", (232, 230, 222), 0.07, dashes=((38, 36, 34), 9)),
          (224, 200, 150), (214, 212, 204))
    trunk("bok_stam", bark("bok_stam", (130, 126, 118), 0.07, flakes=((104, 102, 96), 0.05)),
          (214, 190, 146), (120, 116, 108))
    trunk("ek_stam", bark("ek_stam", (86, 66, 48), 0.25, groove=((48, 36, 26), 0.45)),
          (186, 146, 98), (72, 54, 40))
    trunk("asp_stam", bark("asp_stam", (164, 166, 142), 0.1, dashes=((60, 62, 54), 5),
                           flakes=((108, 112, 94), 0.07)), (222, 206, 160), (150, 152, 130))
    trunk("al_stam", bark("al_stam", (76, 64, 56), 0.2, groove=((44, 36, 32), 0.2),
                          flakes=((110, 94, 80), 0.07)), (190, 150, 110), (70, 58, 50))
    trunk("ronn_stam", bark("ronn_stam", (124, 106, 90), 0.1, dashes=((176, 160, 140), 6)),
          (206, 170, 124), (112, 94, 80))
    trunk("dod_stam", bark("dod_stam", (142, 140, 134), 0.16, groove=((86, 84, 80), 0.3),
                           flakes=((176, 174, 168), 0.05)), (186, 176, 158), (126, 124, 118))
    trunk("mossig_stam", bark("mossig_stam", (88, 70, 50), 0.18, groove=((52, 40, 30), 0.25),
                              moss=((70, 112, 48), 0.55)), (180, 142, 98), (70, 92, 46))
    # Feuillages (gris teintés par le biome).
    t["norr_gran_barr"] = leaves("gran_barr", (84, 98, 90), dark=(40, 54, 48))
    t["norr_tall_barr"] = leaves("tall_barr", (102, 112, 84), dark=(50, 58, 40))
    t["norr_bjork_lov"] = leaves("bjork_lov", (150, 152, 120), dark=(86, 90, 64))
    t["norr_bok_lov"] = leaves("bok_lov", (134, 138, 104), dark=(70, 76, 50))
    t["norr_ek_lov"] = leaves("ek_lov", (112, 116, 90), dark=(56, 62, 40))
    t["norr_asp_lov"] = leaves("asp_lov", (150, 150, 112), dark=(92, 92, 60))
    t["norr_al_lov"] = leaves("al_lov", (86, 98, 82), dark=(44, 54, 42))
    t["norr_ronn_lov"] = leaves("ronn_lov", (122, 124, 100), dark=(64, 66, 48))
    t["norr_salg_lov"] = leaves("salg_lov", (128, 138, 134), dark=(72, 82, 80))
    t["norr_en_barr"] = leaves("en_barr", (92, 104, 126), dark=(44, 54, 76))
    t["norr_hassel_lov"] = leaves("hassel_lov", (146, 154, 112), dark=(84, 92, 56))


def main():
    os.makedirs(g.OUT, exist_ok=True)
    tex = {}
    make_trees(tex)
    for name, img in sorted(tex.items()):
        g.write_png(os.path.join(g.OUT, name + ".png"), img)
    print(f"{len(tex)} textures écrites dans {os.path.relpath(g.OUT, g.ROOT)}")


if __name__ == "__main__":
    main()
