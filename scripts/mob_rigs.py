#!/usr/bin/env python3
"""Rigs et animations des mobs (utilisé par `gen_mobs.py`, écrit `assets/mob_rigs.json`).

Chaque archétype de `gen_mobs.py` étiquette ses boîtes avec un os (`B(..., bone="head")`) et décrit
sa structure dans un `Boxes` (genre + dimensions). Ici :

* `build_rig(boxes, parents, recipe, ...)` : os, pivots et rattachement des boîtes ;
* une *recette* par genre (`recipe_quad`, `recipe_biped`, `recipe_fish`…) qui écrit les clips
  adaptés à la morphologie du mob : amplitude et durée des cycles de marche calculées pour que
  les pieds ne patinent pas (une boucle = 4·longueur de patte·sin(amplitude) blocs parcourus),
  poses de broutage qui atteignent vraiment le sol, regard, réactions aux dégâts, etc.

Conventions (repère du modèle : museau vers +Z, +Y en haut, +X = gauche du mob) :
rotation X positive = vers le bas/l'arrière (un membre pendant part vers −Z, le museau plonge) ;
rotation Y positive = vers +X ; rotation Z positive = le côté +X monte. Unités : degrés et pixels
(16 px = 1 bloc). `_l` = côté −X, `_r` = côté +X (étiquettes seulement).
"""
import math

TAU = 2 * math.pi
CTX = {"bones": set()}


def r3(v):
    return round(float(v), 3)


# ------------------------------------------------------------------------------------------------
# Courbes et clips
# ------------------------------------------------------------------------------------------------
def W(a, n=1, p=0.0, o=0.0, abs_=False, a2=0.0, n2=2, p2=0.0):
    """Onde `o + a·sin(2π(n·u + p)) [+ a2·sin(2π(n2·u + p2))]`."""
    d = {"a": r3(a)}
    if n != 1:
        d["n"] = n
    if p:
        d["p"] = round(p % 1.0, 4)
    if o:
        d["o"] = r3(o)
    if abs_:
        d["abs"] = True
    if a2:
        d["a2"] = r3(a2)
        if n2 != 2:
            d["n2"] = n2
        if p2:
            d["p2"] = round(p2 % 1.0, 4)
    return d


def K(fn, n=16):
    """Clés d'une fonction périodique `fn(u)` échantillonnée sur [0, 1[ (clip en boucle)."""
    return [[round(i / n, 4), r3(fn(i / n))] for i in range(n)]


def KO(fn, n=20):
    """Clés d'une fonction sur [0, 1] (clip joué une fois)."""
    return [[round(i / n, 4), r3(fn(i / n))] for i in range(n + 1)]


def smooth(x):
    x = max(0.0, min(1.0, x))
    return x * x * (3 - 2 * x)


def bump(t, a, b, c, d):
    """0 avant a, monte jusqu'à 1 en b, reste 1 jusqu'à c, redescend à 0 en d."""
    if t < a or t > d:
        return 0.0
    if t < b:
        return smooth((t - a) / max(b - a, 1e-6))
    if t <= c:
        return 1.0
    return smooth(1 - (t - c) / max(d - c, 1e-6))


def C(bone, **props):
    """Canal d'un os ; `None` si l'os n'existe pas dans ce rig (le clip l'ignore)."""
    if bone not in CTX["bones"]:
        return None
    ch = {"bone": bone}
    for k, v in props.items():
        if v is None:
            continue
        ch[k] = r3(v) if isinstance(v, (int, float)) else v
    return ch if len(ch) > 1 else None


def clip(duration, chans, loop=True, additive=False, fade=None):
    d = {"duration": r3(duration), "loop": loop}
    if additive:
        d["additive"] = True
    if fade is not None:
        d["fade"] = fade
    d["channels"] = [c for c in chans if c]
    return d


def side(name):
    """`('leg_l', -1)`, `('leg_r', +1)` pour `name = 'leg'`."""
    return [(name + "_l", -1), (name + "_r", 1)]


def cycle(leg_blocks, speed, a0=30.0, amax=55.0, dmin=0.45, dmax=3.4):
    """Durée de boucle et amplitude (degrés) d'une marche sans patinage à `speed` blocs/s :
    une boucle parcourt `4·jambe·sin(A)` blocs, donc `speed·durée`."""
    leg = max(leg_blocks, 0.12)
    stride = 4 * leg * math.sin(math.radians(a0))
    dur = max(dmin, min(dmax, stride / max(speed, 0.05)))
    s = speed * dur / (4 * leg)
    s = max(math.sin(math.radians(8)), min(math.sin(math.radians(amax)), s))
    return dur, math.degrees(math.asin(s))


# ------------------------------------------------------------------------------------------------
# Construction du rig
# ------------------------------------------------------------------------------------------------
class Boxes(list):
    """Liste de boîtes + genre et dimensions du modèle (pour choisir la recette)."""

    def __init__(self, items=(), kind=None, parents=None, **meta):
        super().__init__(items)
        self.kind = kind
        self.parents = parents or {}
        self.meta = meta

    def __add__(self, other):
        return Boxes(list(self) + list(other), self.kind, self.parents, **self.meta)


def _aabb(boxes):
    x0 = min(b[0] for b in boxes)
    x1 = max(b[0] + b[3] for b in boxes)
    y0 = min(b[1] for b in boxes)
    y1 = max(b[1] + b[4] for b in boxes)
    z0 = min(b[2] for b in boxes)
    z1 = max(b[2] + b[5] for b in boxes)
    return x0, x1, y0, y1, z0, z1


def auto_pivot(name, boxes):
    x0, x1, y0, y1, z0, z1 = _aabb(boxes)
    cx, cy, cz = (x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2
    if name.startswith("leg") or name.startswith("claw"):
        return (cx, y1, cz)
    return (cx, cy, cz)


def build_rig(boxes, recipe, spd, tune=None):
    """Rig complet : os (ancêtres inclus), pivots, rattachement des boîtes, clips.

    `boxes` : `Boxes` dont chaque boîte porte `.bone` (et éventuellement `.pv`). `spd` :
    `{"walk": blocs/s, "run": blocs/s}`.
    """
    tune = tune or {}
    parents = dict(boxes.parents)
    parents.setdefault("root", None)
    used = []
    for b in boxes:
        name = getattr(b, "bone", None) or "body"
        if name not in used:
            used.append(name)
    names = set(used) | {"root"}
    for n in list(names):
        p = parents.get(n)
        while p is not None:
            names.add(p)
            p = parents.get(p)

    def depth(n):
        d = 0
        while parents.get(n) is not None:
            n = parents[n]
            d += 1
        return d

    order = sorted(names, key=lambda n: (depth(n), n != "root", n))
    by_bone = {}
    pivots = {}
    for b in boxes:
        name = getattr(b, "bone", None) or "body"
        by_bone.setdefault(name, []).append(b)
        pv = getattr(b, "pv", None)
        if pv is not None and name not in pivots:
            pivots[name] = pv
    bones = []
    for n in order:
        if n == "root":
            pv = (0.0, 0.0, 0.0)
        elif n in pivots:
            pv = pivots[n]
        elif n in by_bone:
            pv = auto_pivot(n, by_bone[n])
        else:
            # Os purement structurel (sans boîte) : au pivot de son premier enfant.
            kids = [k for k in order if parents.get(k) == n]
            pv = pivots.get(kids[0]) if kids and kids[0] in pivots else (0.0, 0.0, 0.0)
        bone = {"name": n, "pivot": [r3(v) for v in pv]}
        if parents.get(n) is not None:
            bone["parent"] = parents[n]
        bones.append(bone)
    index = {n: i for i, n in enumerate(order)}
    CTX["bones"] = set(order)
    rig = {"bones": bones, "boxes": [index[getattr(b, "bone", None) or "body"] for b in boxes]}
    rig.update(recipe(boxes.meta, spd, tune))
    return rig


def finish(clips, gaits=(), swim=(), idle=None, idles=(), actions=None, air=None, look=None,
           drivers=(), idle_gap=(4.0, 10.0)):
    """Assemble la partie « animation » d'un rig (retire les références vers des clips absents)."""
    clips = {k: v for k, v in clips.items() if v and v["channels"]}
    out = {"clips": clips}
    g = [{"clip": c, "speed": r3(s)} for c, s in gaits if c in clips]
    if g:
        out["gaits"] = g
    s = [{"clip": c, "speed": r3(v)} for c, v in swim if c in clips]
    if s:
        out["swim"] = s
    if idle in clips:
        out["idle"] = idle
    iv = [{"clip": c, "weight": r3(w)} for c, w in idles if c in clips]
    if iv:
        out["idles"] = iv
        out["idle_gap"] = [r3(idle_gap[0]), r3(idle_gap[1])]
    ac = {k: v for k, v in (actions or {}).items() if v in clips}
    if ac:
        out["actions"] = ac
    if air in clips:
        out["air"] = air
    if look:
        bones = [{"bone": b, "weight": r3(w)} for b, w in look["bones"] if b in CTX["bones"]]
        if bones:
            out["look"] = {"bones": bones, "yaw": look["yaw"], "pitch": look["pitch"], "range": look.get("range", 9.0)}
    dr = [d for d in drivers if d and d["bone"] in CTX["bones"]]
    if dr:
        out["drivers"] = dr
    return out


def drv(bone, prop, src, gain, mx=1e6, smooth=0.15):
    d = {"bone": bone, "prop": prop, "src": src, "gain": r3(gain)}
    if mx < 1e5:
        d["max"] = r3(mx)
    if smooth != 0.15:
        d["smooth"] = r3(smooth)
    return d


# ------------------------------------------------------------------------------------------------
# Quadrupèdes
# ------------------------------------------------------------------------------------------------
LEG_PHASE = {
    "walk4": {"leg_fl": 0.0, "leg_br": 0.25, "leg_fr": 0.5, "leg_bl": 0.75},
    "trot": {"leg_fl": 0.0, "leg_br": 0.0, "leg_fr": 0.5, "leg_bl": 0.5},
    "pace": {"leg_fl": 0.0, "leg_bl": 0.0, "leg_fr": 0.5, "leg_br": 0.5},
    "bound": {"leg_fl": 0.0, "leg_fr": 0.08, "leg_bl": 0.5, "leg_br": 0.58},
    "hop": {"leg_fl": 0.0, "leg_fr": 0.0, "leg_bl": 0.4, "leg_br": 0.4},
}


def leg_chans(phases, amp, lift=0.16, stretch=True, front_mult=1.0, back_mult=1.0):
    """Pattes : balancement `A·sin` et étirement vertical qui garde le pied au sol en appui
    (`1/cos θ`) puis le soulève pendant le retour (`lift` × longueur)."""
    out = []
    for bone, p in phases.items():
        a = amp * (front_mult if bone.startswith("leg_f") else back_mult)
        ar = math.radians(a)

        def sy(u, p=p, ar=ar):
            phi = TAU * (u + p)
            th = ar * math.sin(phi)
            return 1 / math.cos(th) - lift * max(0.0, -math.cos(phi))

        out.append(C(bone, rx=W(a, 1, p), sy=K(sy) if stretch else None))
    return out


def recipe_quad(m, sp, t):
    legh = m["legh"]
    leg = legh / 16
    style_w = t.get("walk_style", "walk4")
    style_r = t.get("run_style", "trot")
    has_neck = m.get("neck", False)
    heavy = t.get("heavy", 1.0)
    walk_s, run_s = sp["walk"], sp["run"]
    big = leg > 0.5
    dur_w, a_w = cycle(leg, walk_s, a0=t.get("a_walk", 28.0))
    dur_r, a_r = cycle(leg, run_s, a0=t.get("a_run", 40.0), amax=62.0, dmin=0.34)
    hop = style_r in ("hop", "bound")
    clips = {}

    # -- Marche ---------------------------------------------------------------------------------
    bob = 0.5 * min(2.0, legh / 8) * heavy
    clips["walk"] = clip(dur_w, leg_chans(LEG_PHASE[style_w], a_w, stretch=style_w != "hop") + [
        C("root", ty=W(0.6, 2, 0.25, abs_=False, o=0.0) if style_w == "hop" else None),
        C("body", ty=W(bob / 2, 2, 0.25, o=-bob / 2), rz=W(2.0 * heavy, 1, 0.0), ry=W(1.6 * heavy, 1, 0.25),
          rx=W(1.0, 2, 0.0)),
        C("neck", rx=W(3.0, 2, 0.15), ry=W(1.5, 1, 0.5)),
        C("head", rx=W(2.5, 2, 0.35), ry=W(1.0, 1, 0.5)),
        C("tail", ry=W(9 * (1.0 if m.get("tail_len", 6) > 4 else 1.4), 1, 0.3), rx=W(3, 2, 0.1)),
        C("ear_l", rx=W(4, 2, 0.4), rz=W(3, 1, 0.1)), C("ear_r", rx=W(4, 2, 0.4), rz=W(-3, 1, 0.1)),
    ])

    # -- Course ---------------------------------------------------------------------------------
    if hop:
        phases = LEG_PHASE["bound"] if style_r == "bound" else LEG_PHASE["hop"]
        hop_h = t.get("hop_h", max(2.0, legh * 0.8))
        legs = leg_chans(phases, a_r * (1.0 if style_r == "bound" else 1.15), stretch=False,
                         front_mult=0.8, back_mult=1.15)
        run_chans = legs + [
            C("root", ty=W(hop_h, 1, 0.0, abs_=True) if style_r == "hop" else W(hop_h * 0.5, 1, 0.15, o=hop_h * 0.5)),
            C("body", rx=W(9.0, 1, 0.1)),
            C("neck", rx=W(5.0, 1, 0.6)), C("head", rx=W(4.0, 1, 0.65)),
            C("tail", rx=W(8, 1, 0.3, o=-12)),
            C("ear_l", rx=W(5, 1, 0.4, o=-8)), C("ear_r", rx=W(5, 1, 0.4, o=-8)),
        ]
    else:
        run_chans = leg_chans(LEG_PHASE[style_r], a_r, lift=0.2) + [
            C("root", ty=W(0.5, 2, 0.5, abs_=False)),
            C("body", ty=W(bob, 2, 0.25, o=-bob), rx=W(3.5, 2, 0.5), rz=W(2.5 * heavy, 1, 0.0)),
            C("neck", rx=W(5.0, 2, 0.2, o=-6)), C("head", rx=W(4.0, 2, 0.3, o=-2)),
            C("tail", rx=W(6, 2, 0.1, o=-18), ry=W(7, 1, 0.3)),
            C("ear_l", rx=W(3, 2, 0.4, o=-12)), C("ear_r", rx=W(3, 2, 0.4, o=-12)),
        ]
    clips["run"] = clip(dur_r, run_chans)

    # -- Repos ------------------------------------------------------------------------------------
    clips["idle"] = clip(3.6, [
        C("body", sy=W(0.014, 1, 0.0, o=1.0), ty=W(0.12, 1, 0.25)),
        C("neck", rx=W(1.6, 1, 0.2), ry=W(2.0, 1, 0.55)),
        C("head", rx=W(1.4, 1, 0.45), ry=W(2.5, 1, 0.8)),
        C("tail", ry=W(5.5, 1, 0.0), rx=W(1.5, 2, 0.3)),
        C("ear_l", rx=W(1.5, 2, 0.1)), C("ear_r", rx=W(1.5, 2, 0.6)),
    ])

    # -- Variantes d'idle -------------------------------------------------------------------------
    nk_pv = m.get("neck_pv")  # (y, z) du pivot du cou
    hc = m.get("head_c", (legh + m["H"], m["L"] / 2))  # centre de la tête (y, z)
    hh = m.get("head_h", 5.0)
    if has_neck and nk_pv:
        dy, dz = hc[0] - nk_pv[0], hc[1] - nk_pv[1]
        r = math.hypot(dy, dz)
        phi0 = math.atan2(dy, dz)
        target = max(-0.98, min(0.98, (hh * 0.5 - nk_pv[0]) / r))
        neck_down = max(25.0, min(88.0, math.degrees(phi0 - math.asin(target))))
        head_down = 28.0
    else:
        neck_down, head_down = 0.0, 42.0
    idles = []
    if t.get("graze", True):
        gd = 5.0

        def e(u):
            return bump(u, 0.04, 0.2, 0.82, 0.97)

        def chew(u):
            return math.sin(TAU * 9 * u) * e(u)

        def paw(u):
            return bump(u, 0.45, 0.52, 0.55, 0.62) + bump(u, 0.66, 0.72, 0.74, 0.8)

        clips["graze"] = clip(gd, [
            C("neck", rx=KO(lambda u: neck_down * e(u))),
            C("head", rx=KO(lambda u: head_down * e(u) + 2.2 * chew(u))),
            C("body", rx=KO(lambda u: (0 if has_neck else 5.0) * e(u))),
            C("leg_fl", rx=KO(lambda u: -22 * paw(u) * e(u)) if t.get("paw", big) else None),
            C("tail", ry=W(9, 3, 0.1)),
            C("ear_l", rx=KO(lambda u: -6 * e(u) + 3 * chew(u))), C("ear_r", rx=KO(lambda u: -6 * e(u) - 3 * chew(u))),
        ], loop=False, fade=[0.4, 0.5])
        idles.append(("graze", 3.0))
    if t.get("sniff", not t.get("graze", True)):
        clips["sniff"] = clip(3.2, [
            C("head", rx=KO(lambda u: head_down * 0.8 * bump(u, 0.05, 0.2, 0.8, 0.95) + 4 * math.sin(TAU * 7 * u) * bump(u, 0.05, 0.2, 0.8, 0.95))),
            C("neck", rx=KO(lambda u: neck_down * 0.4 * bump(u, 0.05, 0.2, 0.8, 0.95))),
            C("body", rx=KO(lambda u: 3.0 * bump(u, 0.05, 0.2, 0.8, 0.95))),
            C("tail", ry=W(8, 2, 0.0)),
        ], loop=False, fade=[0.3, 0.4])
        idles.append(("sniff", 2.5))
    clips["look"] = clip(4.2, [
        C("neck", ry=KO(lambda u: 38 * math.sin(TAU * u) * bump(u, 0.02, 0.12, 0.88, 0.98)) if has_neck else None),
        C("head", ry=KO(lambda u: (22 if has_neck else 48) * math.sin(TAU * u) * bump(u, 0.02, 0.12, 0.88, 0.98)),
          rx=KO(lambda u: -5 * bump(u, 0.1, 0.25, 0.75, 0.9))),
        C("ear_l", rx=KO(lambda u: 8 * bump(u, 0.1, 0.2, 0.8, 0.9))), C("ear_r", rx=KO(lambda u: 8 * bump(u, 0.1, 0.2, 0.8, 0.9))),
    ], loop=False, fade=[0.4, 0.5])
    idles.append(("look", 2.0))
    clips["tail_flick"] = clip(1.1, [
        C("tail", ry=KO(lambda u: 30 * math.sin(TAU * 3 * u) * bump(u, 0, 0.1, 0.6, 1.0)), rx=KO(lambda u: -10 * bump(u, 0, 0.1, 0.5, 1.0))),
    ], loop=False, additive=True, fade=[0.1, 0.25])
    idles.append(("tail_flick", 1.5))
    clips["ear_twitch"] = clip(0.9, [
        C("ear_l", rx=KO(lambda u: -22 * bump(u, 0, 0.12, 0.3, 0.6)), rz=KO(lambda u: 12 * math.sin(TAU * 4 * u) * bump(u, 0, 0.1, 0.5, 0.9))),
        C("ear_r", rx=KO(lambda u: -14 * bump(u, 0.3, 0.45, 0.6, 0.9))),
        C("head", rz=KO(lambda u: 3 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.5, 0.9))),
    ], loop=False, additive=True, fade=[0.08, 0.2])
    idles.append(("ear_twitch", 1.5))
    if t.get("howl"):
        clips["howl"] = clip(4.0, [
            C("neck", rx=KO(lambda u: -30 * bump(u, 0.05, 0.25, 0.8, 0.95)) if has_neck else None),
            C("head", rx=KO(lambda u: -42 * bump(u, 0.05, 0.25, 0.8, 0.95) + 2 * math.sin(TAU * 6 * u) * bump(u, 0.25, 0.3, 0.75, 0.8))),
            C("body", rx=KO(lambda u: -6 * bump(u, 0.05, 0.25, 0.8, 0.95))),
            C("tail", rx=KO(lambda u: -18 * bump(u, 0.05, 0.25, 0.8, 0.95))),
        ], loop=False, fade=[0.4, 0.5])
        idles.append(("howl", 1.2))

    # -- Actions ----------------------------------------------------------------------------------
    clips["hurt"] = clip(0.5, [
        C("body", rx=KO(lambda u: -9 * bump(u, 0, 0.12, 0.2, 1.0)), ry=KO(lambda u: 7 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.4, 1.0))),
        C("neck", rx=KO(lambda u: -14 * bump(u, 0, 0.1, 0.2, 1.0))),
        C("head", rx=KO(lambda u: -16 * bump(u, 0, 0.1, 0.2, 1.0))),
        C("tail", rx=KO(lambda u: -28 * bump(u, 0, 0.1, 0.3, 1.0))),
        C("ear_l", rx=KO(lambda u: -25 * bump(u, 0, 0.1, 0.3, 1.0))), C("ear_r", rx=KO(lambda u: -25 * bump(u, 0, 0.1, 0.3, 1.0))),
        C("leg_fl", rx=KO(lambda u: -10 * bump(u, 0, 0.1, 0.2, 1.0))), C("leg_fr", rx=KO(lambda u: -10 * bump(u, 0, 0.1, 0.2, 1.0))),
    ], loop=False, additive=True, fade=[0.05, 0.25])
    if t.get("attack"):
        rear = t.get("rear", False)
        reach = t.get("reach", 3.0)
        if rear:
            def tilt(u):
                return -30 * bump(u, 0.05, 0.3, 0.45, 0.8)

            cy = m["legh"] + m["H"] / 2          # centre du corps (pivot du tangage)
            zf = m["L"] / 2 - 2.0                # hanche avant, devant le centre
            yf = m["legh"] - cy

            def hip(u, axis):
                """Déplacement de la hanche avant quand le corps pivote de `tilt(u)` autour de son centre."""
                th = math.radians(tilt(u))
                dy = yf * math.cos(th) - zf * math.sin(th) - yf
                dz = yf * math.sin(th) + zf * math.cos(th) - zf - 1.0 * bump(u, 0.05, 0.3, 0.45, 0.8) * -1
                return dy if axis == "y" else dz

            clips["attack"] = clip(1.0, [
                C("body", rx=KO(tilt), tz=KO(lambda u: -1.0 * bump(u, 0.05, 0.3, 0.45, 0.8))),
                C("leg_fl", rx=KO(lambda u: tilt(u) - 85 * bump(u, 0.1, 0.32, 0.4, 0.62) + 25 * bump(u, 0.4, 0.5, 0.55, 0.7)),
                  ty=KO(lambda u: hip(u, "y")), tz=KO(lambda u: hip(u, "z"))),
                C("leg_fr", rx=KO(lambda u: tilt(u) - 85 * bump(u, 0.14, 0.36, 0.5, 0.7)),
                  ty=KO(lambda u: hip(u, "y")), tz=KO(lambda u: hip(u, "z"))),
                C("neck", rx=KO(lambda u: 18 * bump(u, 0.38, 0.5, 0.58, 0.8))),
                C("head", rx=KO(lambda u: -10 * bump(u, 0.1, 0.3, 0.38, 0.5) + 32 * bump(u, 0.4, 0.5, 0.58, 0.8))),
                C("tail", rx=KO(lambda u: -20 * bump(u, 0.1, 0.3, 0.6, 0.9))),
            ], loop=False, fade=[0.08, 0.2])
        else:
            clips["attack"] = clip(0.7, [
                C("body", tz=KO(lambda u: -(reach * 0.3) * bump(u, 0, 0.25, 0.3, 0.4) + reach * bump(u, 0.3, 0.42, 0.5, 0.9)),
                  rx=KO(lambda u: 4 * bump(u, 0.3, 0.45, 0.5, 0.9))),
                C("neck", rx=KO(lambda u: -12 * bump(u, 0.05, 0.25, 0.3, 0.4) + 16 * bump(u, 0.35, 0.45, 0.52, 0.85))),
                C("head", rx=KO(lambda u: -22 * bump(u, 0.05, 0.25, 0.3, 0.4) + 34 * bump(u, 0.35, 0.45, 0.52, 0.85))),
                C("leg_fl", rx=KO(lambda u: -35 * bump(u, 0.3, 0.45, 0.5, 0.85))),
                C("leg_fr", rx=KO(lambda u: -35 * bump(u, 0.32, 0.46, 0.52, 0.85))),
                C("ear_l", rx=KO(lambda u: -30 * bump(u, 0.05, 0.2, 0.6, 0.95))), C("ear_r", rx=KO(lambda u: -30 * bump(u, 0.05, 0.2, 0.6, 0.95))),
                C("tail", rx=KO(lambda u: 8 * bump(u, 0.1, 0.4, 0.6, 0.95))),
            ], loop=False, fade=[0.08, 0.2])
    # -- En l'air -----------------------------------------------------------------------------------
    clips["air"] = clip(1.0, [
        C("leg_fl", rx=-34), C("leg_fr", rx=-30), C("leg_bl", rx=36), C("leg_br", rx=32),
        C("tail", rx=-14), C("ear_l", rx=-14), C("ear_r", rx=-14),
    ])
    look = {"bones": [("neck", 0.45), ("head", 0.55)] if has_neck else [("head", 1.0)], "yaw": 52, "pitch": 28, "range": 9.0}
    drivers = [
        drv("body", "rz", "turn", -5.0, 9.0),
        drv("head", "ry", "turn", 5.0, 16.0, 0.12),
        drv("tail", "ry", "turn", -9.0, 28.0, 0.2),
    ]
    return finish(
        clips,
        gaits=[("walk", walk_s), ("run", run_s)], idle="idle", idles=idles,
        actions={"hurt": "hurt", "attack": "attack"}, air="air", look=look, drivers=drivers,
        idle_gap=t.get("idle_gap", (3.5, 9.0)),
    )
