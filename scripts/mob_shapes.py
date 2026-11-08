#!/usr/bin/env python3
"""Archétypes de géométrie des animaux (utilisés par `gen_mobs.py`).

Chaque boîte est étiquetée avec l'os qui la porte (`B(..., bone="head")`, pivot facultatif `pv`) et
chaque fonction renvoie un `Boxes` qui porte le genre du modèle, la hiérarchie des os et les
dimensions nécessaires aux recettes d'animation (`mob_recipes.py`). Les unités sont celles de la
géométrie : pixels MC, 16 px = 1 bloc, museau vers +Z.
"""
from mob_rigs import Boxes


def rgb(h):
    h = h.lstrip("#")
    if len(h) == 3:
        h = "".join(c * 2 for c in h)
    return tuple(int(h[i : i + 2], 16) / 255.0 for i in (0, 2, 4))


class Box(list):
    """Ligne de géométrie `[x, y, z, w, h, d, r, g, b, a]` + os porteur et pivot."""

    bone = None
    pv = None


def B(x, y, z, w, h, d, col, a=1.0, bone=None, pv=None):
    r, g, b = rgb(col) if isinstance(col, str) else col
    box = Box([round(x, 3), round(y, 3), round(z, 3), round(w, 3), round(h, 3), round(d, 3), round(r, 3), round(g, 3), round(b, 3), a])
    box.bone = bone
    box.pv = pv
    return box


def shade(col, f):
    r, g, b = rgb(col)
    return "#%02x%02x%02x" % tuple(int(max(0, min(1, v * f)) * 255) for v in (r, g, b))


def LR(sx):
    return "l" if sx < 0 else "r"


# ------------------------------------------------------------------------------------------------
# Quadrupèdes
# ------------------------------------------------------------------------------------------------
def quad(L, W, H, legh, body, leg=None, belly=None, legw=None, hoof=None, head=(6, 6, 7), head_col=None,
         neck=None, snout=None, ears=None, horns=None, tail=None, hump=None, mane=None, stripes=None,
         spots=None, eye="#101010", head_up=0.0, front_legh=None, antler=None, nose=None):
    """Quadrupède de profil, tête vers +Z."""
    leg = leg or shade(body, 0.8)
    legw = legw or max(2, W * 0.28)
    head_col = head_col or body
    out = []
    # Pattes (devant/arrière, gauche/droite) ; os `leg_{f|b}{l|r}`, pivot en haut de la patte.
    for sx in (-1, 1):
        for sz in (-1, 1):
            x = sx * (W / 2 - legw / 2) - legw / 2
            z = (L / 2 - legw - 0.3) if sz > 0 else (-L / 2 + 0.3)
            bone = ("leg_f" if sz > 0 else "leg_b") + LR(sx)
            pv = (x + legw / 2, legh, z + legw / 2)
            if hoof is None:
                out.append(B(x, 0, z, legw, legh, legw, leg, bone=bone, pv=pv))
            else:
                out.append(B(x, 1.2, z, legw, legh - 1.2, legw, leg, bone=bone, pv=pv))
                out.append(B(x, 0, z, legw, 1.2, legw, hoof, bone=bone, pv=pv))
    # Corps + ventre.
    out.append(B(-W / 2, legh, -L / 2, W, H, L, body, bone="body"))
    if belly:
        out.append(B(-W / 2 + 0.2, legh - 0.2, -L / 2 + 1, W - 0.4, H * 0.35, L - 2, belly, bone="body"))
    if stripes:
        col, n = stripes
        for i in range(n):
            z = -L / 2 + (i + 0.5) * L / n
            out.append(B(-W / 2 - 0.15, legh + 0.4, z - 0.6, W + 0.3, H - 0.8, 1.2, col, bone="body"))
    if spots:
        col, pts = spots
        for (sx_, sy_, sz_, sw) in pts:
            out.append(B(-W / 2 - 0.15 + sx_ * (W + 0.3 - sw), legh + sy_, -L / 2 + sz_, sw, sw, sw, col, bone="body"))
    if hump:
        hw, hh, hd, hz, col = hump
        out.append(B(-hw / 2, legh + H, hz - hd / 2, hw, hh, hd, col, bone="body"))
    if mane:
        mw, mh, col = mane
        out.append(B(-mw / 2, legh + H, -L / 2 + 2, mw, mh, L * 0.55, col, bone="body"))
    # Cou + tête.
    hw, hh, hd = head
    top = legh + H
    parents = {"body": "root", "tail": "body", "ear_l": "head", "ear_r": "head"}
    for n in ("leg_fl", "leg_fr", "leg_bl", "leg_br"):
        parents[n] = "root"
    meta = dict(L=L, W=W, H=H, legh=legh, neck=bool(neck))
    if neck:
        nw, nh, nd = neck
        npv = (0, top - nh * 0.3 + 1.0, L / 2 - nd / 2)
        out.append(B(-nw / 2, top - nh * 0.3, L / 2 - nd, nw, nh, nd, body, bone="neck", pv=npv))
        hy = top - nh * 0.3 + nh - hh * 0.8 + head_up
        hz = L / 2 - nd + nd * 0.5
        hpv = (0, hy + hh * 0.4, hz + hd * 0.2)
        parents["neck"] = "body"
        parents["head"] = "neck"
        meta["neck_pv"] = (npv[1], npv[2])
    else:
        hy = top - hh + head_up
        hz = L / 2 - 0.5
        hpv = (0, hy + hh * 0.5, hz + 0.5)
        parents["head"] = "body"
    out.append(B(-hw / 2, hy, hz, hw, hh, hd, head_col, bone="head", pv=hpv))
    meta["head_c"] = (hy + hh / 2, hz + hd / 2)
    meta["head_h"] = hh
    if snout:
        sw, sh, sd, col = snout
        out.append(B(-sw / 2, hy + 0.5, hz + hd, sw, sh, sd, col, bone="head"))
        if nose:
            out.append(B(-nose[0] / 2, hy + 0.5 + sh - nose[1], hz + hd + sd, nose[0], nose[1], 0.8, nose[2], bone="head"))
    if eye:
        for sx in (-1, 1):
            out.append(B(sx * (hw / 2) - (0.6 if sx > 0 else 0) + (-0.2 if sx > 0 else -0.2), hy + hh * 0.55, hz + hd - 0.3, 0.9, 0.9, 0.5, eye, bone="head"))
    if ears:
        ew, eh, ed, col, spread = ears
        for sx in (-1, 1):
            out.append(B(sx * spread - ew / 2, hy + hh - 0.2, hz + 0.5, ew, eh, ed, col, bone="ear_" + LR(sx),
                         pv=(sx * spread, hy + hh, hz + 0.5 + ed / 2)))
    if horns:
        hw2, hh2, hd2, col, spread, out_w = horns
        for sx in (-1, 1):
            out.append(B(sx * spread - hw2 / 2, hy + hh, hz + 1, hw2, hh2, hd2, col, bone="head"))
            out.append(B(sx * (spread + out_w) - hw2 / 2, hy + hh + hh2 - 0.5, hz + 1, hw2, hh2 * 0.6, hd2, col, bone="head"))
    if antler:
        aw, ah, col = antler
        for sx in (-1, 1):
            out.append(B(sx * 3 - 0.6, hy + hh, hz + 1.5, 1.2, ah, 1.2, col, bone="head"))
            out.append(B(sx * 3 + sx * 1.5 - aw / 2, hy + hh + ah - 1, hz + 1.5, aw, 1.2, 1.2, col, bone="head"))
            out.append(B(sx * (3 + aw) - 0.6, hy + hh + ah - 1, hz + 1.5, 1.2, ah * 0.7, 1.2, col, bone="head"))
            out.append(B(sx * (3 + aw) - 0.6 - sx * 0.0, hy + hh + ah * 1.5, hz + 1.5, 1.2, 1.2, aw * 0.6, col, bone="head"))
    if tail:
        tw, th, td, col, up = tail
        out.append(B(-tw / 2, legh + H - th - up, -L / 2 - td, tw, th, td, col, bone="tail",
                     pv=(0, legh + H - up - th * 0.3, -L / 2 + 0.2)))
        meta["tail_len"] = td
    return Boxes(out, "quad", parents, **meta)


# ------------------------------------------------------------------------------------------------
# Oiseaux marcheurs
# ------------------------------------------------------------------------------------------------
def biped(H, W, body, belly, leg, head, head_col, beak, legh, neck_h=0, tail=None, wing=None, eye="#101010",
          leg_w=1.2, crest=None, neck_col=None, foot=None, L=None):
    """Oiseau marcheur debout (jambes fines, bec vers +Z)."""
    L = L or W * 1.7
    out = []
    for sx in (-1, 1):
        x = sx * W * 0.25 - leg_w / 2
        bone = "leg_" + LR(sx)
        pv = (x + leg_w / 2, legh, -1 + leg_w / 2)
        out.append(B(x, 0, -1, leg_w, legh, leg_w, leg, bone=bone, pv=pv))
        if foot:
            out.append(B(x - 0.6, 0, -1 + 0.5, leg_w + 1.2, 0.6, 3, foot, bone=bone, pv=pv))
    out.append(B(-W / 2, legh, -L / 2, W, H, L, body, bone="body"))
    out.append(B(-W / 2 + 0.3, legh, -L / 2 + 1, W - 0.6, H * 0.45, L - 2, belly, bone="body"))
    parents = {"body": "root", "tail": "body", "wing_l": "body", "wing_r": "body", "leg_l": "root", "leg_r": "root"}
    if wing:
        ww, wh, wd, col = wing
        for sx in (-1, 1):
            out.append(B(sx * (W / 2) - (0 if sx > 0 else 0.7), legh + H - wh - 0.5, -wd / 2, 0.7, wh, wd, col, bone="wing_" + LR(sx),
                         pv=(sx * W / 2, legh + H - 0.5, 0)))
    if tail:
        tw, th, td, col = tail
        out.append(B(-tw / 2, legh + H * 0.5, -L / 2 - td + 0.5, tw, th, td, col, bone="tail",
                     pv=(0, legh + H * 0.5 + th * 0.5, -L / 2 + 0.5)))
    top = legh + H
    hw, hh, hd = head
    nc = neck_col or body
    if neck_h:
        out.append(B(-1, top - 1, L / 2 - 2.2, 2, neck_h + 1, 2, nc, bone="neck", pv=(0, top - 0.5, L / 2 - 1.2)))
        top = top + neck_h - 1
        parents["neck"] = "body"
        parents["head"] = "neck"
    else:
        parents["head"] = "body"
    hz = (L / 2 - 1.5 if neck_h else L / 2 - 1)
    hpv = (0, top - 0.2 + hh * 0.3, hz + hd * 0.4)
    out.append(B(-hw / 2, top - 0.2, hz, hw, hh, hd, head_col, bone="head", pv=hpv))
    bw, bh, bd, bc = beak
    out.append(B(-bw / 2, top - 0.2 + hh * 0.35, hz + hd, bw, bh, bd, bc, bone="head"))
    for sx in (-1, 1):
        out.append(B(sx * hw / 2 - (0.9 if sx > 0 else 0) - 0.0, top + hh * 0.5, hz + hd - 0.8, 0.9, 0.9, 0.5, eye, bone="head"))
    if crest:
        cw, ch, cd, col = crest
        out.append(B(-cw / 2, top + hh - 0.2, hz + 0.2, cw, ch, cd, col, bone="head"))
    return Boxes(out, "biped", parents, H=H, W=W, L=L, legh=legh, neck_h=neck_h, wing=bool(wing))


# ------------------------------------------------------------------------------------------------
# Poissons et cétacés
# ------------------------------------------------------------------------------------------------
def fish(L, H, W, body, belly=None, fin=None, tail=None, eye="#101010", stripes=None, snout=None, dorsal=True,
         alpha=1.0, spike=None, tusk=None, head_taper=True, vertical=False):
    """Poisson / cétacé de profil, museau vers +Z (`vertical` : ondulation haut-bas des cétacés)."""
    out = []
    out.append(B(-W / 2, 0, -L / 2, W, H, L, body, alpha, bone="body"))
    if belly:
        out.append(B(-W / 2 + 0.1, 0, -L / 2 + L * 0.1, W - 0.2, H * 0.35, L * 0.8, belly, alpha, bone="body"))
    hpv = (0, H * 0.5, L / 2 - 0.5)
    if head_taper:
        out.append(B(-W * 0.4, H * 0.1, L / 2, W * 0.8, H * 0.8, L * 0.16, body, alpha, bone="head", pv=hpv))
    if snout:
        sw, sh, sd, col = snout
        out.append(B(-sw / 2, H * 0.25, L / 2 + L * 0.16, sw, sh, sd, col, alpha, bone="head", pv=hpv))
    if tusk:
        out.append(B(-0.5, H * 0.35, L / 2 + L * 0.16, 1, 1, tusk, "#f0ecd8", bone="head", pv=hpv))
    fin = fin or shade(body, 0.8)
    if dorsal:
        out.append(B(-0.5, H, -L * 0.12, 1, max(1.5, H * 0.45), L * 0.28, fin, alpha, bone="dorsal", pv=(0, H, L * 0.02)))
    for sx in (-1, 1):
        out.append(B(sx * (W / 2 + 0.2) - (0.9 if sx > 0 else 0.0), H * 0.1, L * 0.1, 0.9, max(1.0, H * 0.18), L * 0.18, fin, alpha,
                     bone="fin_" + LR(sx), pv=(sx * W / 2, H * 0.19, L * 0.19)))
    t = tail or fin
    out.append(B(-0.5, H * 0.1, -L / 2 - L * 0.2, 1, H * 0.9, L * 0.22, t, alpha, bone="tail", pv=(0, H * 0.5, -L / 2 + L * 0.04)))
    out.append(B(-0.5, H * 0.45 - H * 0.5, -L / 2 - L * 0.1, 1, H * 1.5, L * 0.12, t, alpha, bone="fluke", pv=(0, H * 0.5, -L / 2 - L * 0.0)))
    if stripes:
        col, n = stripes
        for i in range(n):
            z = -L / 2 + (i + 1.2) * L / (n + 2)
            out.append(B(-W / 2 - 0.1, 0.5, z, W + 0.2, H - 1, max(0.8, L * 0.06), col, alpha, bone="body"))
    for sx in (-1, 1):
        out.append(B(sx * (W / 2 + 0.05) - (0.5 if sx > 0 else 0.0), H * 0.55, L / 2 + L * 0.02, 0.5, max(0.8, H * 0.18), max(0.8, H * 0.18), eye,
                     bone="head", pv=hpv))
    if spike:
        for dx, dy, dz in ((0, H, 0), (0, -0.5, 0), (-W / 2 - 0.5, H / 2, 0), (W / 2, H / 2, 0), (0, H / 2, L / 3), (0, H / 2, -L / 3)):
            out.append(B(dx - 0.4, dy, dz - 0.4, 0.8, 0.8, 0.8, spike, bone="body"))
    parents = {"body": "root", "head": "body", "dorsal": "body", "fin_l": "body", "fin_r": "body", "tail": "body", "fluke": "tail"}
    return Boxes(out, "fish", parents, L=L, H=H, W=W, vertical=vertical)


# ------------------------------------------------------------------------------------------------
# Tortues
# ------------------------------------------------------------------------------------------------
def turtle(L, W, H, shell, shell2, skin, legh=2, flipper=False, moss=None, head_len=4):
    out = []
    out.append(B(-W / 2, legh, -L / 2, W, H, L, shell, bone="body"))
    out.append(B(-W / 2 + 1, legh + H, -L / 2 + 1, W - 2, 1.2, L - 2, shell2, bone="body"))
    out.append(B(-W / 2 + 0.2, legh - 0.5, -L / 2 + 0.5, W - 0.4, 1.2, L - 1, skin, bone="body"))
    if moss:
        for i in range(5):
            out.append(B(-W / 2 + 1 + (i * 3) % max(1, int(W - 3)), legh + H + 1.2, -L / 2 + 1 + i * (L - 4) / 5, 2, 1.6, 2, moss, bone="body"))
    for sx in (-1, 1):
        for sz in (-1, 1):
            bone = ("leg_f" if sz > 0 else "leg_b") + LR(sx)
            if flipper:
                x = W / 2 if sx > 0 else -W / 2 - 3.2
                z = sz * L * 0.28 - 1.5
                d = 3 if sz > 0 else 2
                out.append(B(x, legh, z, 3.2, 0.9, d, skin, bone=bone, pv=(sx * W / 2, legh + 0.45, z + d / 2)))
            else:
                x = sx * (W / 2 - 1.2) - 1.0
                z = sz * (L / 2 - 2) - 1.0
                out.append(B(x, 0, z, 2, legh + 0.5, 2, skin, bone=bone, pv=(x + 1, legh + 0.5, z + 1)))
    hpv = (0, legh + 2.0, L / 2 - 0.5)
    out.append(B(-1.6, legh + 0.6, L / 2 - 0.5, 3.2, 3, head_len, skin, bone="head", pv=hpv))
    for sx in (-1, 1):
        out.append(B(sx * 1.6 - (0.6 if sx > 0 else 0), legh + 2.4, L / 2 + head_len - 0.8, 0.6, 0.6, 0.5, "#101010", bone="head", pv=hpv))
    out.append(B(-0.8, legh, -L / 2 - 1.5, 1.6, 1.2, 1.8, skin, bone="tail", pv=(0, legh + 0.6, -L / 2 + 0.3)))
    parents = {"body": "root", "head": "body", "tail": "body"}
    for n in ("leg_fl", "leg_fr", "leg_bl", "leg_br"):
        parents[n] = "root" if not flipper else "body"
    return Boxes(out, "turtle", parents, L=L, W=W, H=H, legh=legh, flipper=flipper)


# ------------------------------------------------------------------------------------------------
# Méduse, raie, calmar
# ------------------------------------------------------------------------------------------------
def jelly(R, tent, col, core, alpha=0.75, n=5):
    out = [B(-R, tent, -R, 2 * R, R * 0.9, 2 * R, col, alpha, bone="bell"),
           B(-R + 1, tent + R * 0.9, -R + 1, 2 * R - 2, R * 0.35, 2 * R - 2, col, alpha, bone="bell"),
           B(-R * 0.45, tent + R * 0.2, -R * 0.45, R * 0.9, R * 0.6, R * 0.9, core, 0.9, bone="bell")]
    pts = [(-R * 0.7, -R * 0.7), (R * 0.7 - 1, -R * 0.7), (-R * 0.7, R * 0.7 - 1), (R * 0.7 - 1, R * 0.7 - 1), (-0.5, -0.5)]
    parents = {"bell": "root"}
    for i, (x, z) in enumerate(pts[:n]):
        out.append(B(x, 0, z, 1, tent, 1, col, alpha, bone=f"tent{i}", pv=(x + 0.5, tent, z + 0.5)))
        parents[f"tent{i}"] = "bell"
    return Boxes(out, "jelly", parents, n=min(n, len(pts)), tent=tent)


def ray(W, L, col, belly, tail_len, eye="#101010"):
    cw = W * 0.36
    pw = W / 2 - cw / 2
    out = [B(-cw / 2, 1, -L / 2, cw, 1.6, L, col, bone="body"),
           B(-cw / 2 + 0.6, 0.6, -L / 2 + 1, cw - 1.2, 0.6, L - 2, belly, bone="body"),
           B(-W * 0.14, 2.6, -L * 0.15, W * 0.28, 1.2, L * 0.5, shade(col, 0.85), bone="body"),
           B(-0.5, 1.2, -L / 2 - tail_len, 1, 1, tail_len, shade(col, 0.7), bone="tail", pv=(0, 1.7, -L / 2 + 0.5))]
    parents = {"body": "root", "tail": "body", "wing_l": "body", "wing_r": "body", "tip_l": "wing_l", "tip_r": "wing_r"}
    for sx in (-1, 1):
        x0 = cw / 2 if sx > 0 else -cw / 2 - pw
        z0 = -L * 0.3
        out.append(B(x0, 1.2, z0, pw, 1.1, L * 0.55, col, bone="wing_" + LR(sx), pv=(sx * cw / 2, 1.75, z0 + L * 0.27)))
        tx = W / 2 if sx > 0 else -W / 2 - 2
        out.append(B(tx, 1.2, -L * 0.1, 2, 1.1, L * 0.35, col, bone="tip_" + LR(sx), pv=(sx * W / 2, 1.75, -L * 0.1 + L * 0.17)))
    for sx in (-1, 1):
        out.append(B(sx * 2.5 - (0.7 if sx > 0 else 0), 2.0, L / 2 - 1.2, 0.8, 0.8, 0.5, eye, bone="body"))
    return Boxes(out, "ray", parents, W=W, L=L)


def squid(R, col, fin, tent, eye="#f0e8d0"):
    out = [B(-R * 0.6, R, -R * 1.4, R * 1.2, R * 1.2, R * 2.2, col, bone="mantle"),
           B(-R * 1.1, R * 1.4, -R * 1.2, R * 2.2, 0.8, R * 0.9, fin, bone="fin", pv=(0, R * 1.4 + 0.4, -R * 0.75)),
           B(-R * 0.55, R * 0.2, R * 0.2, R * 1.1, R * 1.0, R * 0.9, col, bone="head", pv=(0, R * 0.7, R * 0.3))]
    parents = {"mantle": "root", "fin": "mantle", "head": "root"}
    for i in range(6):
        x = -R * 0.5 + i * R * 0.2
        d = tent * (0.8 + 0.1 * (i % 3))
        out.append(B(x, 0, R * 0.9, 0.9, R * 0.9, d, col, bone=f"tent{i}", pv=(x + 0.45, R * 0.45, R * 0.9)))
        parents[f"tent{i}"] = "head"
    for sx in (-1, 1):
        out.append(B(sx * R * 0.6 - (0.8 if sx > 0 else 0), R * 0.4, R * 0.5, 0.8, 1.6, 1.6, eye, bone="head", pv=(0, R * 0.7, R * 0.3)))
    return Boxes(out, "squid", parents, R=R, n=6)


# ------------------------------------------------------------------------------------------------
# Arthropodes, reptiles, amphibiens
# ------------------------------------------------------------------------------------------------
def crab(W, L, shell, leg, claw):
    out = [B(-W / 2, 2, -L / 2, W, 2.4, L, shell, bone="body"), B(-W / 2 + 0.6, 4.4, -L / 2 + 0.6, W - 1.2, 0.8, L - 1.2, shade(shell, 1.15), bone="body")]
    parents = {"body": "root", "claw_l": "body", "claw_r": "body"}
    for sx in (-1, 1):
        for i in range(3):
            x = sx * (W / 2 + 0.6) - 0.5
            z = -L / 2 + 0.8 + i * (L - 2) / 3
            out.append(B(x, 0, z, 1, 3, 1, leg, bone=f"leg_{LR(sx)}{i}", pv=(x + 0.5, 3, z + 0.5)))
            parents[f"leg_{LR(sx)}{i}"] = "body"
        x = sx * (W / 2 + 1.5) - 1.2
        out.append(B(x, 2, L / 2 - 0.3, 2.4, 2.4, 3, claw, bone="claw_" + LR(sx), pv=(x + 1.2, 3.2, L / 2 - 0.3 + 0.4)))
        out.append(B(sx * 1.6 - 0.4, 4.6, L / 2 - 0.5, 0.8, 1.6, 0.8, "#101010", bone="body"))
    return Boxes(out, "crab", parents, W=W, L=L)


def arthropod(W, L, body, leg, tail_col, claw):
    """Scorpion : corps plat, pinces, queue recourbée au-dessus."""
    out = [B(-W / 2, 1.2, -L / 2, W, 2.2, L, body, bone="body"), B(-W * 0.35, 1.8, L / 2 - 1, W * 0.7, 1.8, 3, shade(body, 1.1), bone="body")]
    parents = {"body": "root", "claw_l": "body", "claw_r": "body", "tail1": "body", "tail2": "tail1", "tail3": "tail2", "sting": "tail3"}
    for sx in (-1, 1):
        for i in range(4):
            x = sx * (W / 2 + 0.7) - 0.4
            z = -L / 2 + 1 + i * (L - 3) / 4
            out.append(B(x, 0, z, 0.8, 1.6, 0.8, leg, bone=f"leg_{LR(sx)}{i}", pv=(x + 0.4, 1.6, z + 0.4)))
            parents[f"leg_{LR(sx)}{i}"] = "body"
        x = sx * (W / 2 + 1.2) - 1.0
        out.append(B(x, 1.4, L / 2 + 1, 2, 1.6, 3.2, claw, bone="claw_" + LR(sx), pv=(x + 1.0, 2.2, L / 2 + 1.3)))
        out.append(B(sx * 1.2 - 0.3, 3.2, L / 2 + 0.5, 0.6, 0.6, 0.6, "#c01010", bone="body"))
    out += [B(-0.8, 2.0, -L / 2 - 2.5, 1.6, 1.6, 3, tail_col, bone="tail1", pv=(0, 2.8, -L / 2 + 0.5)),
            B(-0.8, 3.4, -L / 2 - 2.5, 1.6, 3, 1.6, tail_col, bone="tail2", pv=(0, 4.2, -L / 2 - 1.7)),
            B(-0.8, 6.0, -L / 2 - 1.5, 1.6, 1.6, 3, tail_col, bone="tail3", pv=(0, 6.8, -L / 2 - 1.0)),
            B(-0.4, 5.4, -L / 2 + 1.1, 0.8, 0.8, 1.4, "#c8c040", bone="sting", pv=(0, 5.8, -L / 2 + 1.1))]
    return Boxes(out, "arthropod", parents, W=W, L=L)


def lizard(W, L, body, belly, spots):
    out = [B(-W / 2, 1.2, -L / 2, W, 1.6, L, body, bone="body"), B(-W / 2 + 0.3, 1.0, -L / 2 + 0.5, W - 0.6, 0.6, L - 1, belly, bone="body"),
           B(-W * 0.4, 1.4, L / 2, W * 0.8, 1.4, 2.4, body, bone="head", pv=(0, 2.1, L / 2 - 0.2)),
           B(-W * 0.3, 1.6, -L / 2 - 4.5, W * 0.6, 1.0, 4.5, shade(body, 0.9), bone="tail1", pv=(0, 2.1, -L / 2 + 0.2)),
           B(-0.4, 1.8, -L / 2 - 8, 0.8, 0.7, 3.5, shade(body, 0.8), bone="tail2", pv=(0, 2.15, -L / 2 - 4.5))]
    parents = {"body": "root", "head": "body", "tail1": "body", "tail2": "tail1"}
    for sx in (-1, 1):
        for sz in (-1, 1):
            bone = ("leg_f" if sz > 0 else "leg_b") + LR(sx)
            x, z = sx * (W / 2 + 0.4) - 0.5, sz * L * 0.3 - 0.5
            out.append(B(x, 0, z, 1, 1.6, 1, shade(body, 0.8), bone=bone, pv=(x + 0.5, 1.6, z + 0.5)))
            parents[bone] = "root"
        out.append(B(sx * 0.9 - 0.3, 2.6, L / 2 + 1.5, 0.6, 0.6, 0.5, "#101010", bone="head"))
    for i in range(3):
        out.append(B(-0.5, 2.8, -L / 2 + 1.5 + i * 2.2, 1, 0.5, 1, spots, bone="body"))
    return Boxes(out, "lizard", parents, W=W, L=L, legh=1.6)


def frog(W, L, body, belly, spot):
    out = [B(-W / 2, 1.5, -L / 2, W, 3, L, body, bone="body"), B(-W / 2 + 0.4, 1.2, -L / 2 + 0.6, W - 0.8, 1, L - 1.2, belly, bone="body")]
    parents = {"body": "root", "head": "body"}
    for sx in (-1, 1):
        x = sx * (W / 2 + 0.2) - (1.4 if sx < 0 else 0)
        z = -L / 2 - 0.2
        out.append(B(x, 0, z, 1.4, 2.6, 3.4, shade(body, 0.85), bone="leg_b" + LR(sx), pv=(x + 0.7, 2.6, z + 1.7)))
        x = sx * (W / 2 - 0.6) - (1.0 if sx < 0 else 0)
        z = L / 2 - 1.2
        out.append(B(x, 0, z, 1, 1.8, 1.8, shade(body, 0.85), bone="leg_f" + LR(sx), pv=(x + 0.5, 1.8, z + 0.9)))
        for n in ("leg_b", "leg_f"):
            parents[n + LR(sx)] = "root"
        out.append(B(sx * 1.8 - 0.6, 4.5, L / 2 - 1.6, 1.2, 1.4, 1.2, "#e8d040", bone="head", pv=(0, 4.0, L / 2 - 1.5)))
        out.append(B(sx * 1.8 - 0.2, 4.9, L / 2 - 0.6, 0.5, 0.6, 0.4, "#101010", bone="head", pv=(0, 4.0, L / 2 - 1.5)))
    for dx in (-1.5, 0.5):
        out.append(B(dx, 4.5, -1, 1.2, 0.5, 1.2, spot, bone="body"))
    return Boxes(out, "frog", parents, W=W, L=L)


def hedgehog(L, H, quills, face, belly):
    out = [B(-H * 0.55, 1, -L / 2, H * 1.1, H, L, quills, bone="body"), B(-H * 0.45, 0.6, -L / 2 + 1, H * 0.9, 1, L - 2, belly, bone="body"),
           B(-H * 0.4, 1, L / 2, H * 0.8, H * 0.8, 3.2, face, bone="head", pv=(0, 1.8, L / 2 - 0.2)),
           B(-0.6, 1.4, L / 2 + 3.2, 1.2, 1.0, 0.9, "#202020", bone="head", pv=(0, 1.8, L / 2 - 0.2)),
           B(-H * 0.55, H + 1, -L / 2 + 1, H * 1.1, 1.2, L * 0.6, shade(quills, 0.8), bone="body")]
    parents = {"body": "root", "head": "body"}
    for sx in (-1, 1):
        for sz in (-1, 1):
            bone = ("leg_f" if sz > 0 else "leg_b") + LR(sx)
            x, z = sx * H * 0.35 - 0.5, sz * L * 0.3 - 0.5
            out.append(B(x, 0, z, 1, 1.2, 1, face, bone=bone, pv=(x + 0.5, 1.2, z + 0.5)))
            parents[bone] = "root"
        out.append(B(sx * 1.0 - 0.3, 2.3, L / 2 + 2.6, 0.6, 0.6, 0.5, "#101010", bone="head", pv=(0, 1.8, L / 2 - 0.2)))
    return Boxes(out, "hedgehog", parents, L=L, H=H)
