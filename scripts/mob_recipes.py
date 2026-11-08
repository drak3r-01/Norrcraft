#!/usr/bin/env python3
"""Recettes d'animation par genre de mob (suite de `mob_rigs.py`) : oiseaux, poissons, tortues,
méduses, raies, calmars, crabes, scorpions, lézards, grenouilles, hérissons.

Rappel : les courbes d'échelle (`s`, `sx`, `sy`, `sz`) sont des **facteurs absolus** (1 = repos),
y compris dans les clips additifs (facteur 1 + écart).
"""
import math

from mob_rigs import (C, CTX, K, KO, LEG_PHASE, TAU, W, bump, clip, cycle, drv, finish, leg_chans, recipe_quad,
                      smooth)


# ------------------------------------------------------------------------------------------------
# Oiseaux marcheurs
# ------------------------------------------------------------------------------------------------
def recipe_biped(m, sp, t):
    legh = m["legh"]
    leg = legh / 16
    waddle = t.get("style") == "waddle"
    walk_s, run_s = sp["walk"], sp["run"]
    dur_w, a_w = cycle(leg, walk_s, a0=t.get("a_walk", 26.0), dmin=0.4)
    dur_r, a_r = cycle(leg, run_s, a0=t.get("a_run", 38.0), amax=60.0, dmin=0.28)
    neck = m.get("neck_h", 0) > 0
    wing = m.get("wing", False)
    bob = 0.35 + legh / 40
    clips = {}

    def legs(a, lift):
        return leg_chans({"leg_l": 0.0, "leg_r": 0.5}, a, lift=lift)

    wing_walk = [C("wing_l", rz=W(-6 if waddle else -2, 1, 0.0, o=-5 if waddle else -1)),
                 C("wing_r", rz=W(6 if waddle else 2, 1, 0.5, o=5 if waddle else 1))]
    clips["walk"] = clip(dur_w, legs(a_w, 0.2) + [
        C("body", ty=W(bob / 2, 2, 0.25, o=-bob / 2), rz=W(9.0 if waddle else 1.5, 1, 0.0), ry=W(5.0 if waddle else 1.0, 1, 0.25),
          rx=W(1.5, 2, 0.5)),
        C("neck", rx=W(5.0, 2, 0.3), tz=W(1.6, 1, 0.0)) if neck else None,
        C("head", rx=W(3.0, 2, 0.45), tz=None if neck else W(1.4, 1, 0.0)),
        C("tail", rx=W(5, 2, 0.1), ry=W(4, 1, 0.3)),
    ] + wing_walk)
    open_w = t.get("run_wings", 22.0)
    clips["run"] = clip(dur_r, legs(a_r, 0.25) + [
        C("body", ty=W(bob, 2, 0.25, o=-bob), rx=W(3.0, 2, 0.5, o=4), rz=W(6.0 if waddle else 2.0, 1, 0.0)),
        C("neck", rx=W(7.0, 2, 0.3, o=-4), tz=W(2.0, 1, 0.0)) if neck else None,
        C("head", rx=W(3.5, 2, 0.45), tz=None if neck else W(1.8, 1, 0.0)),
        C("tail", rx=W(6, 2, 0.2, o=-14)),
        C("wing_l", rz=W(-6, 4, 0.0, o=-open_w)), C("wing_r", rz=W(6, 4, 0.0, o=open_w)),
    ])
    clips["idle"] = clip(3.4, [
        C("body", sy=W(0.016, 1, 0.0, o=1.0)),
        C("neck", rx=W(2.5, 1, 0.1), ry=W(3.0, 1, 0.4)) if neck else None,
        C("head", ry=W(4.0, 1, 0.65), rx=W(2.0, 1, 0.3)),
        C("tail", rx=W(2.5, 2, 0.2)),
        C("wing_l", rz=W(1.0, 1, 0.0, o=-1.0)), C("wing_r", rz=W(1.0, 1, 0.5, o=1.0)),
    ])
    idles = []

    def peck(u):
        v = 0.0
        for c in (0.22, 0.45, 0.68):
            v = max(v, bump(u, c - 0.08, c - 0.03, c + 0.01, c + 0.08))
        return v * bump(u, 0.0, 0.05, 0.95, 1.0)

    down = t.get("peck_down", 55.0)
    clips["peck"] = clip(2.6, [
        C("head", rx=KO(lambda u: down * peck(u) * (0.55 if neck else 1.0))),
        C("neck", rx=KO(lambda u: down * 0.7 * peck(u))) if neck else None,
        C("body", rx=KO(lambda u: 6 * peck(u))),
        C("tail", rx=KO(lambda u: -10 * peck(u))),
    ], loop=False, fade=[0.15, 0.25])
    idles.append(("peck", 2.5))
    clips["look"] = clip(3.8, [
        C("head", ry=KO(lambda u: (55 if not neck else 35) * math.sin(TAU * 1.5 * u) * bump(u, 0.02, 0.1, 0.9, 0.98))),
        C("neck", ry=KO(lambda u: 30 * math.sin(TAU * 1.5 * u) * bump(u, 0.02, 0.1, 0.9, 0.98))) if neck else None,
    ], loop=False, fade=[0.25, 0.3])
    idles.append(("look", 2.0))
    if wing:
        clips["flutter"] = clip(1.0, [
            C("wing_l", rz=KO(lambda u: -35 * math.sin(TAU * 4 * u) * bump(u, 0, 0.1, 0.7, 1.0) - 6 * bump(u, 0, 0.1, 0.7, 1))),
            C("wing_r", rz=KO(lambda u: 35 * math.sin(TAU * 4 * u) * bump(u, 0, 0.1, 0.7, 1.0) + 6 * bump(u, 0, 0.1, 0.7, 1))),
            C("body", rx=KO(lambda u: -4 * bump(u, 0, 0.2, 0.6, 1.0))),
        ], loop=False, additive=True, fade=[0.08, 0.2])
        idles.append(("flutter", 1.2))
        clips["preen"] = clip(3.4, [
            C("neck", ry=KO(lambda u: 70 * bump(u, 0.05, 0.2, 0.85, 0.97)), rx=KO(lambda u: 25 * bump(u, 0.05, 0.2, 0.85, 0.97))) if neck else None,
            C("head", ry=KO(lambda u: (130 if not neck else 70) * bump(u, 0.05, 0.2, 0.85, 0.97)),
              rx=KO(lambda u: 30 * bump(u, 0.05, 0.2, 0.85, 0.97) + 6 * math.sin(TAU * 6 * u) * bump(u, 0.2, 0.25, 0.8, 0.85))),
            C("body", rz=KO(lambda u: 4 * bump(u, 0.05, 0.2, 0.85, 0.97))),
        ], loop=False, fade=[0.3, 0.4])
        idles.append(("preen", 1.2))
    if t.get("one_leg"):
        clips["one_leg"] = clip(6.0, [
            C("leg_r", rx=KO(lambda u: -75 * bump(u, 0.04, 0.12, 0.88, 0.96)), ty=KO(lambda u: 2.5 * bump(u, 0.04, 0.12, 0.88, 0.96))),
            C("neck", rx=KO(lambda u: -22 * bump(u, 0.08, 0.2, 0.8, 0.94))) if neck else None,
            C("head", ry=KO(lambda u: 75 * bump(u, 0.2, 0.3, 0.8, 0.94)), rx=KO(lambda u: 20 * bump(u, 0.2, 0.3, 0.8, 0.94))),
            C("body", rx=KO(lambda u: -3 * bump(u, 0.04, 0.12, 0.88, 0.96))),
        ], loop=False, fade=[0.5, 0.6])
        idles.append(("one_leg", 1.5))
    clips["hurt"] = clip(0.5, [
        C("body", rx=KO(lambda u: -10 * bump(u, 0, 0.1, 0.2, 1.0))),
        C("head", rx=KO(lambda u: -16 * bump(u, 0, 0.1, 0.2, 1.0))),
        C("neck", rx=KO(lambda u: -12 * bump(u, 0, 0.1, 0.2, 1.0))) if neck else None,
        C("wing_l", rz=KO(lambda u: -45 * bump(u, 0, 0.12, 0.35, 1.0))), C("wing_r", rz=KO(lambda u: 45 * bump(u, 0, 0.12, 0.35, 1.0))),
    ], loop=False, additive=True, fade=[0.05, 0.25])
    clips["air"] = clip(0.5, [
        C("wing_l", rz=W(48, 1, 0.0, o=-10)), C("wing_r", rz=W(-48, 1, 0.0, o=10)),
        C("leg_l", rx=-22), C("leg_r", rx=-14), C("tail", rx=-12),
    ])
    look = {"bones": [("neck", 0.4), ("head", 0.6)] if neck else [("head", 1.0)], "yaw": 70, "pitch": 28, "range": 8.0}
    drivers = [drv("body", "rz", "turn", -4.0, 8.0), drv("tail", "ry", "turn", -7.0, 25.0, 0.2),
               drv("head", "ry", "turn", 5.0, 16.0, 0.12)]
    return finish(clips, gaits=[("walk", walk_s), ("run", run_s)], idle="idle", idles=idles,
                  actions={"hurt": "hurt"}, air="air", look=look, drivers=drivers, idle_gap=(3.0, 8.0))


# ------------------------------------------------------------------------------------------------
# Poissons et cétacés
# ------------------------------------------------------------------------------------------------
def recipe_fish(m, sp, t):
    L = m["L"] / 16
    vert = m.get("vertical", False)
    ax = "rx" if vert else "ry"
    walk_s, run_s = sp["walk"], sp["run"]
    beat = t.get("beat", 0.75 if not vert else 0.6)  # longueurs de corps par battement
    dur_s = max(0.4, min(2.6, beat * L / max(walk_s, 0.05)))
    dur_f = max(0.28, min(1.8, beat * L / max(run_s, 0.05) * 1.25))
    amp = t.get("amp", 15.0 if not vert else 9.0)
    stiff = t.get("stiff", 1.0)  # < 1 : corps plus raide (poisson-globe)
    clips = {}

    def wave_chans(a, n=1):
        a *= stiff
        c = [
            C("body", **{ax: W(a * 0.22, n, 0.0)}),
            C("head", **{ax: W(a * 0.18, n, 0.5)}),
            C("tail", **{ax: W(a * 1.0, n, -0.14)}),
            C("fluke", **{ax: W(a * 1.4, n, -0.26)}),
            C("dorsal", rz=W(a * 0.25, n, 0.1)) if not vert else C("dorsal", rx=W(a * 0.3, n, 0.1)),
        ]
        sp_ = 22 if not vert else 14
        c.append(C("fin_l", rz=W(-sp_, n * 2, 0.0, o=-6)))
        c.append(C("fin_r", rz=W(sp_, n * 2, 0.5, o=6)))
        return c

    clips["swim"] = clip(dur_s, wave_chans(amp))
    clips["dart"] = clip(dur_f, wave_chans(amp * 1.35))
    clips["idle"] = clip(2.8, [
        C("body", **{ax: W(2.0, 1, 0.0)}), C("tail", **{ax: W(6 * stiff, 1, -0.12)}), C("fluke", **{ax: W(9 * stiff, 1, -0.22)}),
        C("head", **{ax: W(1.5, 1, 0.5)}),
        C("fin_l", rz=W(-18, 2, 0.0, o=-8)), C("fin_r", rz=W(18, 2, 0.5, o=8)),
        C("dorsal", rz=W(3, 1, 0.1)),
        C("root", ty=W(0.5, 1, 0.25)),
    ])
    idles = []
    clips["flick"] = clip(0.9, [
        C("tail", **{ax: KO(lambda u: 30 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.6, 1.0))}),
        C("fluke", **{ax: KO(lambda u: 42 * math.sin(TAU * 2 * u + 0.8) * bump(u, 0, 0.1, 0.6, 1.0))}),
        C("body", **{ax: KO(lambda u: 8 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.6, 1.0))}),
    ], loop=False, additive=True, fade=[0.08, 0.2])
    idles.append(("flick", 1.5))
    clips["look"] = clip(3.0, [
        C("head", ry=KO(lambda u: 26 * math.sin(TAU * u) * bump(u, 0.02, 0.12, 0.88, 0.98))),
        C("body", ry=KO(lambda u: 6 * math.sin(TAU * u) * bump(u, 0.02, 0.12, 0.88, 0.98))),
    ], loop=False, fade=[0.3, 0.4])
    idles.append(("look", 1.5))
    clips["shimmy"] = clip(1.2, [
        C("body", **{ax: KO(lambda u: 10 * math.sin(TAU * 5 * u) * bump(u, 0, 0.1, 0.7, 1.0))}),
        C("tail", **{ax: KO(lambda u: 16 * math.sin(TAU * 5 * u - 0.7) * bump(u, 0, 0.1, 0.7, 1.0))}),
        C("fluke", **{ax: KO(lambda u: 22 * math.sin(TAU * 5 * u - 1.4) * bump(u, 0, 0.1, 0.7, 1.0))}),
    ], loop=False, additive=True, fade=[0.1, 0.25])
    idles.append(("shimmy", 0.8))
    puff = t.get("puff")
    clips["hurt"] = clip(0.55, [
        C("body", s=KO(lambda u: 1 + puff * bump(u, 0, 0.15, 0.5, 1.0)) if puff else None,
          **{ax: KO(lambda u: 26 * math.sin(TAU * 1.5 * u) * bump(u, 0, 0.1, 0.4, 1.0))}),
        C("tail", **{ax: KO(lambda u: -34 * math.sin(TAU * 1.5 * u) * bump(u, 0, 0.1, 0.4, 1.0))}),
        C("fluke", **{ax: KO(lambda u: -44 * math.sin(TAU * 1.5 * u) * bump(u, 0, 0.1, 0.4, 1.0))}),
        C("fin_l", rz=KO(lambda u: -35 * bump(u, 0, 0.1, 0.4, 1.0))), C("fin_r", rz=KO(lambda u: 35 * bump(u, 0, 0.1, 0.4, 1.0))),
    ], loop=False, additive=True, fade=[0.05, 0.2])
    if t.get("attack"):
        bite = t.get("bite", 3.0)
        clips["attack"] = clip(0.8, [
            C("body", tz=KO(lambda u: bite * bump(u, 0.15, 0.3, 0.45, 0.8)),
              **{("rx" if not vert else "ry"): KO(lambda u: -6 * bump(u, 0.15, 0.3, 0.5, 0.85))}),
            C("head", ry=KO(lambda u: 24 * math.sin(TAU * 3 * u) * bump(u, 0.3, 0.38, 0.62, 0.75)),
              rx=KO(lambda u: -12 * bump(u, 0.1, 0.25, 0.3, 0.4) + 10 * bump(u, 0.35, 0.45, 0.6, 0.8))),
            C("tail", **{ax: KO(lambda u: 18 * math.sin(TAU * 2 * u) * bump(u, 0.05, 0.2, 0.7, 0.95))}),
            C("fluke", **{ax: KO(lambda u: 26 * math.sin(TAU * 2 * u - 0.6) * bump(u, 0.05, 0.2, 0.7, 0.95))}),
        ], loop=False, fade=[0.08, 0.2])
    drivers = [drv("body", "rz", "turn", -5.0, 14.0, 0.2), drv("tail", "ry", "turn", -8.0, 20.0, 0.2),
               drv("body", "rx", "vspeed", -9.0, 28.0, 0.25)]
    return finish(clips, swim=[("swim", walk_s), ("dart", run_s)], idle="idle", idles=idles,
                  actions={"hurt": "hurt", "attack": "attack"}, drivers=drivers, idle_gap=(2.0, 6.0))


# ------------------------------------------------------------------------------------------------
# Tortues (terrestres et marines)
# ------------------------------------------------------------------------------------------------
def recipe_turtle(m, sp, t):
    legh = m["legh"]
    leg = max(legh, 3.0) / 16
    marine = m.get("flipper", False)
    walk_s, run_s = sp["walk"], sp["run"]
    clips = {}
    idles = []
    if marine:
        dur_s = max(1.0, min(3.0, 1.1 * m["L"] / 16 / max(walk_s, 0.05)))
        dur_f = max(0.7, min(2.0, 0.8 * m["L"] / 16 / max(run_s, 0.05)))

        def stroke(a):
            return [
                C("leg_fl", rz=W(-a, 1, 0.0, o=-6), ry=W(8, 1, 0.25)), C("leg_fr", rz=W(a, 1, 0.0, o=6), ry=W(-8, 1, 0.25)),
                C("leg_bl", rz=W(-a * 0.35, 1, 0.3, o=-3), ry=W(10, 1, 0.55)), C("leg_br", rz=W(a * 0.35, 1, 0.3, o=3), ry=W(-10, 1, 0.55)),
                C("body", rx=W(4.0, 1, 0.5), ty=W(0.5, 1, 0.25)), C("head", rx=W(4, 1, 0.8), ry=W(2, 1, 0.3)),
                C("tail", ry=W(8, 1, 0.1)),
            ]
        clips["swim"] = clip(dur_s, stroke(32))
        clips["dart"] = clip(dur_f, stroke(40))
        clips["idle"] = clip(4.0, [
            C("leg_fl", rz=W(-14, 1, 0.0, o=-6)), C("leg_fr", rz=W(14, 1, 0.0, o=6)),
            C("leg_bl", rz=W(-6, 1, 0.3, o=-3)), C("leg_br", rz=W(6, 1, 0.3, o=3)),
            C("body", rx=W(2.0, 1, 0.5), ty=W(0.5, 1, 0.25)), C("head", ry=W(3, 1, 0.5)),
        ])
        gait = dict(swim=[("swim", walk_s), ("dart", run_s)])
    else:
        dur_w, a_w = cycle(leg, walk_s, a0=24.0, dmin=0.9, dmax=3.4)
        dur_r, a_r = cycle(leg, run_s, a0=30.0, dmin=0.6)
        ph = LEG_PHASE["trot"]

        def stepping(a, bobv):
            return leg_chans(ph, a, lift=0.3) + [
                C("body", rz=W(2.5, 1, 0.0), ty=W(bobv / 2, 2, 0.25, o=-bobv / 2)),
                C("head", tz=W(1.5, 1, 0.1), rx=W(3, 1, 0.45)), C("tail", ry=W(10, 1, 0.3)),
            ]
        clips["walk"] = clip(dur_w, stepping(a_w, 0.5))
        clips["run"] = clip(dur_r, stepping(a_r, 0.8))
        clips["idle"] = clip(4.2, [
            C("body", sy=W(0.01, 1, 0.0, o=1.0)),
            C("head", tz=W(0.9, 1, 0.1), rx=W(2.0, 1, 0.5), ry=W(3.0, 1, 0.2)), C("tail", ry=W(4, 1, 0.0)),
        ])
        hide = t.get("hide", 4.0)
        clips["hide"] = clip(3.5, [
            C("head", tz=KO(lambda u: -hide * bump(u, 0.05, 0.14, 0.82, 0.95)), s=KO(lambda u: 1 - 0.3 * bump(u, 0.05, 0.14, 0.82, 0.95))),
            C("leg_fl", rx=KO(lambda u: 6 * bump(u, 0.05, 0.14, 0.82, 0.95))), C("leg_fr", rx=KO(lambda u: 6 * bump(u, 0.05, 0.14, 0.82, 0.95))),
            C("body", ty=KO(lambda u: -0.4 * bump(u, 0.05, 0.14, 0.82, 0.95))),
        ], loop=False, fade=[0.25, 0.4])
        idles.append(("hide", 1.0))
        gait = dict(gaits=[("walk", walk_s), ("run", run_s)])
    clips["look"] = clip(3.4, [
        C("head", ry=KO(lambda u: 40 * math.sin(TAU * u) * bump(u, 0.03, 0.12, 0.88, 0.97)), rx=KO(lambda u: -8 * bump(u, 0.1, 0.2, 0.8, 0.9))),
    ], loop=False, fade=[0.4, 0.5])
    idles.append(("look", 2.0))
    clips["hurt"] = clip(0.9 if not marine else 0.5, [
        C("head", tz=KO(lambda u: -3.5 * bump(u, 0, 0.1, 0.6, 1.0))),
        C("body", rx=KO(lambda u: -5 * bump(u, 0, 0.1, 0.25, 1.0))),
        C("leg_fl", rz=KO(lambda u: -30 * bump(u, 0, 0.1, 0.3, 1.0))) if marine else None,
        C("leg_fr", rz=KO(lambda u: 30 * bump(u, 0, 0.1, 0.3, 1.0))) if marine else None,
    ], loop=False, additive=True, fade=[0.05, 0.3])
    look = {"bones": [("head", 1.0)], "yaw": 60, "pitch": 25, "range": 8.0}
    return finish(clips, **gait, idle="idle", idles=idles, actions={"hurt": "hurt"}, look=look,
                  drivers=[drv("body", "rz", "turn", -3.0, 6.0), drv("tail", "ry", "turn", -8.0, 25.0, 0.2)],
                  idle_gap=(4.0, 11.0))


# ------------------------------------------------------------------------------------------------
# Méduse, raie, calmar
# ------------------------------------------------------------------------------------------------
def recipe_jelly(m, sp, t):
    n = m["n"]
    ts = [f"tent{i}" for i in range(n)]

    def contract(u):  # 1 = cloche contractée : contraction rapide, détente lente
        return bump(u, 0.0, 0.18, 0.22, 0.75)

    def pulse(depth, rise):
        chans = [
            C("bell", sy=K(lambda u: 1 - 0.2 * depth * contract(u)), sx=K(lambda u: 1 + 0.16 * depth * contract(u)),
              sz=K(lambda u: 1 + 0.16 * depth * contract(u))),
            C("root", ty=K(lambda u: rise * math.sin(math.pi * u / 0.8) ** 2 if u < 0.8 else 0.0)),
        ]
        for i, name in enumerate(ts):
            lag = 0.12 + 0.05 * i
            chans.append(C(name, sy=K(lambda u, lag=lag: 1 + 0.25 * depth * contract((u - lag) % 1.0)),
                           rx=W(10 * depth, 1, lag + 0.1 + 0.03 * i), rz=W(8 * depth, 1, lag + i * 0.37)))
        return chans

    clips = {"pulse": clip(1.7, pulse(1.0, 4.0)), "pulse_fast": clip(0.95, pulse(1.15, 5.0)), "idle": clip(3.0, pulse(0.55, 1.5))}
    clips["hurt"] = clip(0.6, [
        C("bell", sy=KO(lambda u: 1 - 0.28 * bump(u, 0, 0.12, 0.25, 1)), sx=KO(lambda u: 1 + 0.22 * bump(u, 0, 0.12, 0.25, 1)),
          sz=KO(lambda u: 1 + 0.22 * bump(u, 0, 0.12, 0.25, 1)))] +
        [C(nm, rx=KO(lambda u: 30 * bump(u, 0, 0.12, 0.3, 1)), rz=KO(lambda u, i=i: (-1) ** i * 25 * bump(u, 0, 0.12, 0.3, 1))) for i, nm in enumerate(ts)],
        loop=False, additive=True, fade=[0.05, 0.25])
    return finish(clips, swim=[("pulse", sp["walk"]), ("pulse_fast", sp["run"])], idle="idle", actions={"hurt": "hurt"},
                  drivers=[drv("root", "rz", "turn", -4.0, 12.0, 0.25)])


def recipe_ray(m, sp, t):
    L = m["L"] / 16
    dur_s = max(1.4, min(3.4, 1.3 * L / max(sp["walk"], 0.05)))
    dur_f = max(0.9, min(2.0, 0.9 * L / max(sp["run"], 0.05)))

    def flap(a):
        return [
            C("wing_l", rz=W(-a, 1, 0.0)), C("wing_r", rz=W(a, 1, 0.0)),
            C("tip_l", rz=W(-a * 0.7, 1, -0.14)), C("tip_r", rz=W(a * 0.7, 1, -0.14)),
            C("body", ty=W(0.7, 1, 0.25), rx=W(2.5, 1, 0.5)),
            C("tail", ry=W(10, 1, -0.2), rx=W(8, 1, -0.1)),
        ]
    clips = {"swim": clip(dur_s, flap(24)), "dart": clip(dur_f, flap(30))}
    clips["idle"] = clip(3.6, [
        C("wing_l", rz=W(-10, 1, 0.0)), C("wing_r", rz=W(10, 1, 0.0)), C("tip_l", rz=W(-8, 1, -0.12)), C("tip_r", rz=W(8, 1, -0.12)),
        C("body", ty=W(0.4, 1, 0.25)), C("tail", ry=W(8, 1, -0.2)),
    ])
    clips["barrel"] = clip(2.4, [
        C("body", rz=KO(lambda u: 360 * smooth(u)), ty=KO(lambda u: 1.5 * math.sin(math.pi * u))),
    ], loop=False, fade=[0.3, 0.5])
    clips["hurt"] = clip(0.6, [
        C("wing_l", rz=KO(lambda u: -35 * bump(u, 0, 0.1, 0.25, 1))), C("wing_r", rz=KO(lambda u: 35 * bump(u, 0, 0.1, 0.25, 1))),
        C("tail", ry=KO(lambda u: 40 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.5, 1))),
    ], loop=False, additive=True, fade=[0.05, 0.25])
    return finish(clips, swim=[("swim", sp["walk"]), ("dart", sp["run"])], idle="idle", idles=[("barrel", 0.5)],
                  actions={"hurt": "hurt"}, drivers=[drv("body", "rz", "turn", -7.0, 18.0, 0.2)], idle_gap=(6.0, 14.0))


def recipe_squid(m, sp, t):
    n = m["n"]
    ts = [f"tent{i}" for i in range(n)]

    def squeeze(u):
        return bump(u, 0.0, 0.16, 0.2, 0.7)

    def jet(depth, lag_s):
        chans = [
            C("mantle", sx=K(lambda u: 1 - 0.14 * depth * squeeze(u)), sz=K(lambda u: 1 - 0.14 * depth * squeeze(u)),
              sy=K(lambda u: 1 + 0.1 * depth * squeeze(u)), tz=K(lambda u: -1.2 * depth * squeeze(u))),
            C("fin", rx=W(7 * depth, 2, 0.0), rz=W(5 * depth, 2, 0.25)),
            C("head", rx=W(2.0, 1, 0.5)),
        ]
        for i, nm in enumerate(ts):
            chans.append(C(nm, rx=W(10 * depth, 1, lag_s + 0.04 * i, o=12 * depth), rz=W(6 * depth, 1, 0.2 + 0.1 * i),
                           sy=K(lambda u, i=i: 1 + 0.12 * depth * squeeze((u - 0.1 - 0.02 * i) % 1.0))))
        return chans

    clips = {"swim": clip(1.5, jet(1.0, 0.2)), "dart": clip(0.8, jet(1.25, 0.25)), "idle": clip(3.2, jet(0.4, 0.15))}
    clips["hurt"] = clip(0.5, [
        C("mantle", sx=KO(lambda u: 1 + 0.2 * bump(u, 0, 0.1, 0.25, 1)), sz=KO(lambda u: 1 + 0.2 * bump(u, 0, 0.1, 0.25, 1)))] +
        [C(nm, rx=KO(lambda u: 35 * bump(u, 0, 0.1, 0.3, 1)), rz=KO(lambda u, i=i: (-1) ** i * 30 * bump(u, 0, 0.1, 0.3, 1))) for i, nm in enumerate(ts)],
        loop=False, additive=True, fade=[0.05, 0.25])
    return finish(clips, swim=[("swim", sp["walk"]), ("dart", sp["run"])], idle="idle", actions={"hurt": "hurt"},
                  drivers=[drv("mantle", "rz", "turn", -5.0, 15.0, 0.2), drv("mantle", "rx", "vspeed", -8.0, 25.0, 0.25)])


# ------------------------------------------------------------------------------------------------
# Crabe, scorpion, lézard, grenouille, hérisson
# ------------------------------------------------------------------------------------------------
def recipe_crab(m, sp, t):
    legs = [f"leg_{s}{i}" for i in range(3) for s in "lr"]
    walk_s, run_s = sp["walk"], sp["run"]
    dur_w, a_w = cycle(0.2, walk_s, a0=26.0, dmin=0.35, dmax=1.4)
    dur_r, a_r = cycle(0.2, run_s, a0=36.0, dmin=0.22, dmax=0.9)

    def gait(a, claw_a):
        ch = []
        for i in range(3):
            for s, sg in (("l", -1), ("r", 1)):
                ph = (i * 0.33 + (0.5 if s == "r" else 0.0)) % 1.0
                ch.append(C(f"leg_{s}{i}", rx=W(a, 1, ph), rz=W(sg * 8, 1, ph + 0.25),
                            sy=K(lambda u, ph=ph: 1 - 0.18 * max(0.0, -math.cos(TAU * (u + ph))))))
        return ch + [C("body", rz=W(2.0, 1, 0.0), ty=W(0.25, 2, 0.25)),
                     C("claw_l", rx=W(claw_a, 1, 0.0, o=-14), ry=W(8, 1, 0.3)), C("claw_r", rx=W(claw_a, 1, 0.5, o=-14), ry=W(-8, 1, 0.3))]

    clips = {"walk": clip(dur_w, gait(a_w, 8)), "run": clip(dur_r, gait(a_r, 14))}
    clips["idle"] = clip(3.0, [C("body", sy=W(0.02, 1, 0.0, o=1.0)), C("claw_l", rx=W(3, 1, 0.0, o=-14)), C("claw_r", rx=W(3, 1, 0.5, o=-14))])
    clips["snap"] = clip(1.6, [
        C("claw_l", ry=KO(lambda u: -26 * math.sin(TAU * 3 * u) ** 2 * bump(u, 0.02, 0.1, 0.8, 0.95)), rx=KO(lambda u: -22 * bump(u, 0.02, 0.1, 0.8, 0.95))),
        C("claw_r", ry=KO(lambda u: 26 * math.sin(TAU * 3 * u + 1.0) ** 2 * bump(u, 0.02, 0.1, 0.8, 0.95)), rx=KO(lambda u: -22 * bump(u, 0.02, 0.1, 0.8, 0.95))),
    ], loop=False, fade=[0.2, 0.3])
    clips["wave"] = clip(2.4, [
        C("claw_r", rx=KO(lambda u: -55 * bump(u, 0.05, 0.2, 0.85, 0.97)), rz=KO(lambda u: 22 * math.sin(TAU * 4 * u) * bump(u, 0.2, 0.25, 0.8, 0.85)))
    ], loop=False, fade=[0.3, 0.4])
    clips["hurt"] = clip(0.5, [
        C("claw_l", rx=KO(lambda u: -35 * bump(u, 0, 0.1, 0.3, 1))), C("claw_r", rx=KO(lambda u: -35 * bump(u, 0, 0.1, 0.3, 1))),
        C("body", rx=KO(lambda u: -8 * bump(u, 0, 0.1, 0.2, 1))),
    ] + [C(n, sy=KO(lambda u: 1 - 0.3 * bump(u, 0, 0.1, 0.3, 1))) for n in legs], loop=False, additive=True, fade=[0.05, 0.2])
    look = {"bones": [("body", 0.35)], "yaw": 35, "pitch": 0, "range": 6.0}
    return finish(clips, gaits=[("walk", walk_s), ("run", run_s)], idle="idle", idles=[("snap", 2.0), ("wave", 1.0)],
                  actions={"hurt": "hurt"}, look=look, drivers=[drv("body", "rz", "turn", -4.0, 8.0)], idle_gap=(2.5, 7.0))


def recipe_scorpion(m, sp, t):
    walk_s, run_s = sp["walk"], sp["run"]
    dur_w, a_w = cycle(0.12, walk_s, a0=24.0, dmin=0.4, dmax=1.6)
    dur_r, a_r = cycle(0.12, run_s, a0=34.0, dmin=0.26, dmax=1.0)

    def gait(a, tail_up):
        ch = []
        for i in range(4):
            for s in "lr":
                ph = ((0.5 if (i % 2) ^ (s == "r") else 0.0) + i * 0.06) % 1.0
                ch.append(C(f"leg_{s}{i}", rx=W(a, 1, ph), sy=K(lambda u, ph=ph: 1 - 0.2 * max(0.0, -math.cos(TAU * (u + ph))))))
        return ch + [
            C("body", rz=W(2.0, 1, 0.0), ty=W(0.2, 2, 0.25), ry=W(2.0, 1, 0.25)),
            C("claw_l", rx=W(6, 1, 0.0, o=-6), ry=W(7, 1, 0.3)), C("claw_r", rx=W(6, 1, 0.5, o=-6), ry=W(-7, 1, 0.3)),
            C("tail1", ry=W(6, 1, 0.1), rx=W(3, 2, 0.1, o=tail_up)), C("tail2", ry=W(8, 1, 0.0), rx=W(4, 2, 0.2)),
            C("tail3", ry=W(10, 1, -0.1), rx=W(4, 2, 0.3)), C("sting", rx=W(4, 2, 0.4)),
        ]
    clips = {"walk": clip(dur_w, gait(a_w, 0)), "run": clip(dur_r, gait(a_r, -6))}
    clips["idle"] = clip(3.4, [
        C("body", sy=W(0.015, 1, 0.0, o=1.0)),
        C("tail1", ry=W(4, 1, 0.0), rx=W(2, 1, 0.2)), C("tail2", ry=W(6, 1, -0.1), rx=W(3, 1, 0.3)),
        C("tail3", ry=W(8, 1, -0.2), rx=W(3, 1, 0.4)), C("sting", rx=W(3, 1, 0.5)),
        C("claw_l", rx=W(2, 1, 0.0, o=-6)), C("claw_r", rx=W(2, 1, 0.5, o=-6)),
    ])
    clips["snap"] = clip(1.4, [
        C("claw_l", ry=KO(lambda u: -24 * math.sin(TAU * 3 * u) ** 2 * bump(u, 0.02, 0.1, 0.8, 0.95)), rx=KO(lambda u: -16 * bump(u, 0.02, 0.1, 0.8, 0.95))),
        C("claw_r", ry=KO(lambda u: 24 * math.sin(TAU * 3 * u + 1.0) ** 2 * bump(u, 0.02, 0.1, 0.8, 0.95)), rx=KO(lambda u: -16 * bump(u, 0.02, 0.1, 0.8, 0.95))),
        C("tail2", ry=KO(lambda u: 14 * math.sin(TAU * 2 * u) * bump(u, 0.1, 0.2, 0.8, 0.95))),
    ], loop=False, fade=[0.15, 0.25])
    clips["tail_wave"] = clip(2.6, [
        C("tail1", rx=KO(lambda u: -10 * bump(u, 0.05, 0.2, 0.85, 0.97)), ry=KO(lambda u: 14 * math.sin(TAU * 2 * u) * bump(u, 0.1, 0.2, 0.85, 0.95))),
        C("tail2", rx=KO(lambda u: -16 * bump(u, 0.05, 0.2, 0.85, 0.97)), ry=KO(lambda u: 20 * math.sin(TAU * 2 * u - 0.5) * bump(u, 0.1, 0.2, 0.85, 0.95))),
        C("tail3", rx=KO(lambda u: -12 * bump(u, 0.05, 0.2, 0.85, 0.97)), ry=KO(lambda u: 26 * math.sin(TAU * 2 * u - 1.0) * bump(u, 0.1, 0.2, 0.85, 0.95))),
    ], loop=False, fade=[0.3, 0.4])
    clips["hurt"] = clip(0.5, [
        C("body", rx=KO(lambda u: -8 * bump(u, 0, 0.1, 0.2, 1))),
        C("tail1", rx=KO(lambda u: -22 * bump(u, 0, 0.1, 0.3, 1))), C("tail2", rx=KO(lambda u: -26 * bump(u, 0, 0.1, 0.3, 1))),
        C("tail3", rx=KO(lambda u: -22 * bump(u, 0, 0.1, 0.3, 1))),
        C("claw_l", rx=KO(lambda u: -28 * bump(u, 0, 0.1, 0.3, 1))), C("claw_r", rx=KO(lambda u: -28 * bump(u, 0, 0.1, 0.3, 1))),
    ], loop=False, additive=True, fade=[0.05, 0.2])
    clips["attack"] = clip(0.9, [
        C("body", tz=KO(lambda u: 2.5 * bump(u, 0.3, 0.42, 0.5, 0.85)), rx=KO(lambda u: -6 * bump(u, 0.05, 0.25, 0.3, 0.4))),
        C("tail1", rx=KO(lambda u: -30 * bump(u, 0.05, 0.28, 0.32, 0.4) + 38 * bump(u, 0.36, 0.44, 0.52, 0.85))),
        C("tail2", rx=KO(lambda u: -34 * bump(u, 0.05, 0.28, 0.32, 0.4) + 50 * bump(u, 0.36, 0.44, 0.52, 0.85))),
        C("tail3", rx=KO(lambda u: -30 * bump(u, 0.05, 0.28, 0.32, 0.4) + 62 * bump(u, 0.36, 0.44, 0.52, 0.85))),
        C("sting", rx=KO(lambda u: 30 * bump(u, 0.36, 0.44, 0.52, 0.85))),
        C("claw_l", ry=KO(lambda u: -22 * bump(u, 0.2, 0.35, 0.55, 0.85))), C("claw_r", ry=KO(lambda u: 22 * bump(u, 0.2, 0.35, 0.55, 0.85))),
    ], loop=False, fade=[0.08, 0.2])
    look = {"bones": [("body", 0.3)], "yaw": 30, "pitch": 0, "range": 9.0}
    return finish(clips, gaits=[("walk", walk_s), ("run", run_s)], idle="idle", idles=[("snap", 2.0), ("tail_wave", 1.5)],
                  actions={"hurt": "hurt", "attack": "attack"}, look=look,
                  drivers=[drv("body", "rz", "turn", -3.0, 7.0), drv("tail1", "ry", "turn", -8.0, 22.0, 0.2)], idle_gap=(2.5, 7.0))


def recipe_lizard(m, sp, t):
    legh = max(m["legh"], 1.6)
    leg = legh / 16
    walk_s, run_s = sp["walk"], sp["run"]
    dur_w, a_w = cycle(leg, walk_s, a0=30.0, dmin=0.4, dmax=1.6)
    dur_r, a_r = cycle(leg, run_s, a0=42.0, dmin=0.24, dmax=0.9)
    ph = {"leg_fl": 0.0, "leg_br": 0.0, "leg_fr": 0.5, "leg_bl": 0.5}

    def gait(a, sway):
        return leg_chans(ph, a, lift=0.3) + [
            C("body", ry=W(sway, 1, 0.0), ty=W(0.15, 2, 0.25)),
            C("head", ry=W(-sway * 0.9, 1, 0.0), rx=W(2, 2, 0.4)),
            C("tail1", ry=W(-sway * 1.4, 1, 0.12)), C("tail2", ry=W(-sway * 2.0, 1, 0.26)),
        ]
    clips = {"walk": clip(dur_w, gait(a_w, 7)), "run": clip(dur_r, gait(a_r, 11))}
    clips["idle"] = clip(3.0, [
        C("body", sy=W(0.025, 1, 0.0, o=1.0)), C("head", ry=W(3, 1, 0.3), rx=W(1.5, 1, 0.1)),
        C("tail1", ry=W(5, 1, 0.1)), C("tail2", ry=W(8, 1, 0.25)),
    ])
    clips["pushup"] = clip(2.4, [
        C("body", ty=KO(lambda u: 1.2 * abs(math.sin(TAU * 1.5 * u)) * bump(u, 0.05, 0.12, 0.88, 0.96))),
        C("head", rx=KO(lambda u: -10 * bump(u, 0.05, 0.15, 0.88, 0.96))),
    ], loop=False, fade=[0.2, 0.3])
    clips["look"] = clip(2.8, [
        C("head", ry=KO(lambda u: 50 * math.sin(TAU * 1.5 * u) * bump(u, 0.02, 0.1, 0.9, 0.98))),
        C("body", ry=KO(lambda u: 8 * math.sin(TAU * 1.5 * u) * bump(u, 0.02, 0.1, 0.9, 0.98))),
    ], loop=False, fade=[0.2, 0.3])
    clips["tongue"] = clip(1.0, [C("head", rx=KO(lambda u: 6 * math.sin(TAU * 4 * u) * bump(u, 0, 0.1, 0.8, 1))),
                                 C("tail2", ry=KO(lambda u: 16 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.8, 1)))],
                          loop=False, additive=True, fade=[0.1, 0.2])
    clips["hurt"] = clip(0.5, [
        C("body", ry=KO(lambda u: 14 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.4, 1)), rx=KO(lambda u: -8 * bump(u, 0, 0.1, 0.2, 1))),
        C("tail1", ry=KO(lambda u: -30 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.4, 1))),
        C("tail2", ry=KO(lambda u: -40 * math.sin(TAU * 2 * u) * bump(u, 0, 0.1, 0.4, 1))),
    ], loop=False, additive=True, fade=[0.05, 0.2])
    look = {"bones": [("head", 1.0)], "yaw": 55, "pitch": 25, "range": 8.0}
    return finish(clips, gaits=[("walk", walk_s), ("run", run_s)], idle="idle", idles=[("pushup", 1.0), ("look", 2.0), ("tongue", 1.5)],
                  actions={"hurt": "hurt"}, look=look,
                  drivers=[drv("body", "rz", "turn", -4.0, 8.0), drv("tail1", "ry", "turn", -8.0, 25.0, 0.2)], idle_gap=(2.5, 7.0))


def recipe_frog(m, sp, t):
    walk_s, run_s = sp["walk"], sp["run"]
    hop_w = max(0.34, min(0.9, 0.5 / max(walk_s, 0.05)))
    hop_r = max(0.26, min(0.7, 0.9 / max(run_s, 0.05)))

    def air(u):  # 0 au sol, 1 au sommet du saut
        return math.sin(math.pi * (u - 0.2) / 0.6) if 0.2 <= u <= 0.8 else 0.0

    def crouch(u):
        return bump(u, 0.0, 0.1, 0.15, 0.22)

    def push(u):
        return bump(u, 0.18, 0.26, 0.32, 0.5)

    def hop(h, ang):
        back = lambda u: -30 * crouch(u) + 50 * push(u) + 18 * air(u)
        backs = lambda u: 1 - 0.25 * crouch(u) + 0.35 * push(u)
        return [
            C("root", ty=K(lambda u: h * air(u))),
            C("body", rx=K(lambda u: -ang * 0.45 * air(u) + 7 * crouch(u)), ty=K(lambda u: -0.5 * crouch(u))),
            C("head", rx=K(lambda u: -4 * air(u))),
            C("leg_bl", rx=K(back), sy=K(backs)), C("leg_br", rx=K(back), sy=K(backs)),
            C("leg_fl", rx=K(lambda u: -30 * air(u) + 10 * crouch(u))), C("leg_fr", rx=K(lambda u: -30 * air(u) + 10 * crouch(u))),
        ]
    clips = {"walk": clip(hop_w, hop(2.2, 40)), "run": clip(hop_r, hop(3.6, 55))}
    clips["idle"] = clip(2.4, [
        C("body", sy=W(0.04, 1, 0.0, o=1.0), sx=W(0.02, 1, 0.25, o=1.0)), C("head", rx=W(1.5, 1, 0.3)),
    ])
    clips["croak"] = clip(1.6, [
        C("body", s=KO(lambda u: 1 + 0.12 * math.sin(TAU * 3 * u) ** 2 * bump(u, 0.1, 0.2, 0.7, 0.85))),
        C("head", rx=KO(lambda u: -8 * bump(u, 0.1, 0.2, 0.7, 0.85))),
    ], loop=False, fade=[0.15, 0.2])
    clips["look"] = clip(2.6, [
        C("body", ry=KO(lambda u: 38 * math.sin(TAU * u) * bump(u, 0.05, 0.15, 0.85, 0.95))),
    ], loop=False, fade=[0.2, 0.3])
    clips["hurt"] = clip(0.45, [
        C("body", rx=KO(lambda u: -12 * bump(u, 0, 0.1, 0.2, 1)), sy=KO(lambda u: 1 + 0.15 * bump(u, 0, 0.1, 0.2, 1))),
        C("leg_bl", rx=KO(lambda u: 30 * bump(u, 0, 0.1, 0.25, 1))), C("leg_br", rx=KO(lambda u: 30 * bump(u, 0, 0.1, 0.25, 1))),
    ], loop=False, additive=True, fade=[0.05, 0.2])
    clips["air"] = clip(1.0, [C("leg_bl", rx=40), C("leg_br", rx=40), C("leg_fl", rx=-40), C("leg_fr", rx=-40)])
    look = {"bones": [("head", 1.0)], "yaw": 50, "pitch": 25, "range": 7.0}
    return finish(clips, gaits=[("walk", walk_s), ("run", run_s)], idle="idle", idles=[("croak", 2.0), ("look", 1.5)],
                  actions={"hurt": "hurt"}, air="air", look=look, drivers=[drv("body", "rz", "turn", -3.0, 6.0)], idle_gap=(2.5, 6.0))


def recipe_hedgehog(m, sp, t):
    walk_s, run_s = sp["walk"], sp["run"]
    dur_w, a_w = cycle(0.075, walk_s, a0=30.0, dmin=0.35, dmax=1.5)
    dur_r, a_r = cycle(0.075, run_s, a0=42.0, dmin=0.22, dmax=0.9)
    ph = LEG_PHASE["trot"]

    def gait(a, roll):
        return leg_chans(ph, a, lift=0.3, stretch=True) + [
            C("body", rz=W(roll, 1, 0.0), ty=W(0.2, 2, 0.25)), C("head", rx=W(3, 2, 0.3), ry=W(2, 1, 0.5)),
        ]
    clips = {"walk": clip(dur_w, gait(a_w, 4)), "run": clip(dur_r, gait(a_r, 6))}
    clips["idle"] = clip(3.0, [C("body", sy=W(0.03, 1, 0.0, o=1.0), sx=W(0.015, 1, 0.0, o=1.0)), C("head", rx=W(2, 1, 0.3))])
    clips["sniff"] = clip(2.4, [
        C("head", rx=KO(lambda u: -10 * bump(u, 0.05, 0.15, 0.85, 0.95) + 4 * math.sin(TAU * 9 * u) * bump(u, 0.1, 0.2, 0.8, 0.9)),
          ry=KO(lambda u: 20 * math.sin(TAU * 1.5 * u) * bump(u, 0.05, 0.15, 0.85, 0.95))),
    ], loop=False, fade=[0.2, 0.3])
    clips["hurt"] = clip(0.9, [
        C("body", s=KO(lambda u: 1 + 0.12 * bump(u, 0, 0.1, 0.5, 1)), rx=KO(lambda u: 12 * bump(u, 0, 0.12, 0.5, 1))),
        C("head", rx=KO(lambda u: 40 * bump(u, 0, 0.12, 0.5, 1)), tz=KO(lambda u: -1.5 * bump(u, 0, 0.12, 0.5, 1))),
    ], loop=False, additive=True, fade=[0.05, 0.3])
    look = {"bones": [("head", 1.0)], "yaw": 50, "pitch": 20, "range": 7.0}
    return finish(clips, gaits=[("walk", walk_s), ("run", run_s)], idle="idle", idles=[("sniff", 3.0)],
                  actions={"hurt": "hurt"}, look=look, drivers=[drv("body", "rz", "turn", -3.0, 6.0)], idle_gap=(2.5, 6.5))


RECIPES = {
    "quad": recipe_quad, "biped": recipe_biped, "fish": recipe_fish, "turtle": recipe_turtle,
    "jelly": recipe_jelly, "ray": recipe_ray, "squid": recipe_squid, "crab": recipe_crab,
    "arthropod": recipe_scorpion, "lizard": recipe_lizard, "frog": recipe_frog, "hedgehog": recipe_hedgehog,
}
