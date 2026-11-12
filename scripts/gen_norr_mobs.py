#!/usr/bin/env python3
"""Bestiaire terrestre de Norrcraft (session 5.4, doc 03 §3.1 et §3.2).

Écrit, pour l'univers `norrcraft` :
  * `assets/norrcraft/mobs.json`           réglages + blocs `behavior` / `spawn` (même format que vanilla),
  * `assets/norrcraft/mob_geometry.json`   géométrie en boîtes colorées (pixels MC, 16 px = 1 bloc),
  * `assets/norrcraft/mob_rigs.json`       squelettes et animations.

Les formes et les recettes d'animation sont celles de vanilla (`mob_shapes.py`, `mob_recipes.py`).
Aucun identifiant ne reprend un mob vanilla ni une forme de `nb-gfx` (test `sealed`).

Usage : MSYS_NO_PATHCONV=1 docker run --rm -v "$(pwd -W):/w" -w /w rust:1 python3 scripts/gen_norr_mobs.py
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mob_shapes import *  # noqa: E402,F401,F403
from mob_rigs import build_rig  # noqa: E402
from mob_recipes import RECIPES  # noqa: E402
from gen_mobs import cfg, speeds  # noqa: E402  (importer gen_mobs ne réécrit rien)

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
MOBS = []  # (config, boxes)
RIGS = {}


def add(id_, name, boxes, profile, spawn, tune=None, **kw):
    c = cfg(id_, name, boxes, [], **kw)
    for k in ("spawnBiomes", "spawnSurfaces"):
        c.pop(k, None)
    for k in ("spawnMinY", "spawnMaxY", "spawnMaxWaterDepth"):
        c.pop(k, None)
    c["behavior"] = profile
    c["spawn"] = spawn
    MOBS.append((c, boxes))
    t = dict(tune or {})
    if c["behaviorType"] == "AGGRESSIVE" and boxes.kind in ("quad", "fish", "arthropod"):
        t.setdefault("attack", True)
    RIGS[id_] = build_rig(boxes, RECIPES[boxes.kind], speeds(c), t)


# Courbes de saison : 8 facteurs à partir du solstice d'hiver (0 : décembre, 2 : mars, 4 : juin, 6 : septembre).
ALL_YEAR = []
ACTIVE = [0, 0, 0.2, 1, 1, 1, 1, 0.3]  # hibernant : absent de novembre à mars
WARM = [0, 0, 0.3, 1, 1, 1, 0.8, 0.1]


def sp(density, curve=None, time="always", persistence="ephemeral", **extra):
    d = {"densityPerBiome": density, "persistence": persistence}
    if curve:
        d["seasonCurve"] = curve
    if time != "always":
        d["timeOfDay"] = time
    d.update(extra)
    return d


# -- Herbivores et petits mammifères (doc 03 §3.1) -----------------------------------------------
add("alg", "Älg",
    quad(32, 12, 15, 26, "#4a3626", leg="#8a7a66", hoof="#1e1812", head=(7, 9, 14), head_col="#3e2c1e", neck=(7, 17, 9),
         snout=(6, 6, 4, "#2e2218"), nose=(4, 1.4, "#181210"), ears=(2.6, 4, 1, "#3e2c1e", 4.2), antler=(6, 10, "#cdbd96"),
         tail=(2, 3, 1.5, "#d8cdb8", 1), belly="#6a5238", mane=(3, 4, "#2e2218")),
    "betare",
    sp({"URSKOG": 0.10, "MYR": 0.10, "SJOLAND": 0.10, "BRANNFALT": 0.14, "ALV": 0.06, "FJALLBJORKSKOG": 0.05},
       [1, 1, 1, 1, 1, 1, 1, 1], "crepuscular", avoid={"fire": 8}),
    tune=dict(heavy=1.3, a_walk=26.0, attack=True),
    hp=30.0, speed=(1.0, 1.6), flee=5.6, chase=5.0, count=40, group=(1, 2), behavior="AGGRESSIVE", aggro=0.0,
    damage=4.0, reach=2.4, cooldown=1.8, knock=7.0, detect=0.0, aflee=0.0)

add("ren", "Ren",
    quad(20, 8, 10, 13, "#8a7a68", leg="#6e6252", hoof="#2a2018", head=(5, 6, 9), head_col="#7a6a58", neck=(5, 11, 6),
         snout=(4, 4, 3, "#3a3028"), ears=(2, 3.4, 1, "#7a6a58", 3), antler=(5, 10, "#b8a888"), tail=(2, 3, 1.2, "#e8e2d4", 1),
         belly="#d8d2c4", mane=(4, 3, "#e8e2d4")),
    "hjord",
    sp({}, [1, 1, 1, 1, 1, 1, 1, 1], "diurnal", persistence="herd",
       herd={"motion": "seasonal", "cell": 1024, "sizeMin": 8, "sizeMax": 20, "regrowPerDay": 0.5,
             "summer": ["FJALLHED", "FJALLBJORKSKOG", "PALSMYR"], "winter": ["TALLHED", "URSKOG"],
             "upMonth": 4, "downMonth": 9, "wander": 24, "materialize": 96}),
    tune=dict(heavy=1.15, a_walk=27.0),
    hp=16.0, speed=(1.2, 1.9), flee=5.6, count=60, group=(1, 1))

add("radjur", "Rådjur",
    quad(14, 6, 8, 12, "#a8744a", leg="#8a5c38", hoof="#241a12", head=(4.4, 5, 7), head_col="#9a6a44", neck=(4, 8, 5),
         snout=(3, 3, 2.4, "#3a2a20"), ears=(2, 4, 1, "#9a6a44", 2.2), tail=(2.6, 3, 1, "#f2ece0", 1), belly="#e8d8bc"),
    "skygg",
    sp({"BOKSKOG": 0.20, "HAGMARK": 0.20, "ALVAR": 0.14, "STRANDANG": 0.04}, [0.8, 0.8, 1, 1, 1, 1, 1, 0.9], "crepuscular"),
    tune=dict(run_style="trot", heavy=0.8),
    hp=8.0, speed=(1.2, 2.0), flee=6.4, count=40, group=(1, 3))

add("myskoxe", "Myskoxe",
    quad(28, 14, 16, 10, "#2e241c", leg="#d8cdb0", hoof="#1a1410", head=(8, 9, 9), head_col="#2a2018", snout=(5.4, 5, 3, "#241a14"),
         ears=(2, 2, 1, "#2a2018", 4.6), horns=(1.8, 4, 2, "#e8dfc4", 6, 2.5), tail=(1.4, 2, 1, "#2a2018", 1),
         hump=(10, 5, 11, 3, "#2e241c"), belly="#3a2e24")
    + [B(-8.5, 4, -12, 17, 10, 24, "#2a2018", bone="body")],
    "myskdjur",
    sp({"FJALLHED": 0.03}, [1, 1, 1, 1, 1, 1, 1, 1], "diurnal"),
    tune=dict(heavy=1.6, a_walk=24.0, attack=True),
    hp=40.0, speed=(0.8, 1.2), flee=4.2, chase=5.0, count=16, group=(3, 6), behavior="AGGRESSIVE", aggro=0.0,
    damage=4.0, reach=2.2, cooldown=1.8, knock=6.0, detect=0.0, aflee=0.0)

add("snolopare", "Skogshare",
    quad(8, 4, 4.5, 4, "#8a7058", leg="#6e5844", head=(3.6, 3.6, 3.8), head_col="#927860", snout=(1.8, 1.4, 1.4, "#cdbfae"),
         ears=(1.2, 5.4, 0.9, "#7a6048", 0.9), tail=(2, 2, 1.4, "#f4f0e8", 0.6), belly="#d6cbb8"),
    "smavilt",
    sp({"URSKOG": 0.20, "FJALLBJORKSKOG": 0.25, "FJALLHED": 0.18, "TALLHED": 0.25, "HAGMARK": 0.20, "ALVAR": 0.20, "BRANNFALT": 0.10},
       [1, 1, 1, 1, 1, 1, 1, 1], "crepuscular"),
    tune=dict(walk_style="hop", run_style="hop", hop_w=1.2, hop_h=4.0, paw=False),
    hp=3.0, speed=(1.4, 2.8), flee=7.0, count=60, group=(1, 2), move=(0.4, 2.0), pause=(1.5, 5.0))

add("fjallammel", "Fjällämmel",
    quad(5, 3, 3, 1.4, "#c49a58", leg="#6a4a28", head=(2.8, 2.6, 2.8), head_col="#caa060", snout=(1.4, 1.2, 1, "#a07840"),
         ears=(0.8, 0.8, 0.6, "#6a4a28", 1), tail=(1, 1, 0.8, "#6a4a28", 0.4), belly="#e8d8b0",
         stripes=("#2a1e12", 2)),
    "gnagare",
    sp({"FJALLHED": 0.60, "PALSMYR": 0.40, "KALFJALL": 0.15}, [1, 1, 1, 1, 1, 1, 1, 1], "always"),
    tune=dict(run_style="bound", hop_h=1.5, graze=False, sniff=True),
    hp=1.0, speed=(0.6, 1.2), flee=3.2, count=80, group=(2, 5), move=(0.6, 2.0), pause=(1.5, 4.0))

add("baver", "Bäver",
    quad(14, 8, 7, 3, "#5e4128", leg="#3e2a18", head=(5, 5.4, 5.6), head_col="#654630", snout=(3.2, 2.4, 1.8, "#7a5636"),
         ears=(1.2, 1.2, 0.8, "#3e2a18", 2), tail=(5, 1.4, 7, "#241a10", 0), belly="#7a5a3a")
    + [B(-0.8, 6.0, 10.4, 0.7, 1.4, 0.6, "#d8b060", bone="head"), B(0.1, 6.0, 10.4, 0.7, 1.4, 0.6, "#d8b060", bone="head")],
    "byggare",
    sp({"ALV": 0.22, "SJOLAND": 0.22}, [0.4, 0.4, 0.8, 1, 1, 1, 1, 0.6], "crepuscular", persistence="resident"),
    tune=dict(heavy=0.9, paw=False),
    hp=10.0, speed=(0.8, 1.4), flee=3.6, count=30, group=(2, 4), medium="AMPHIBIAN")

add("ekorre", "Ekorre",
    quad(6, 3, 3.5, 2.5, "#b0562a", leg="#8a4020", head=(3.2, 3.2, 3.4), head_col="#b85c30", snout=(1.6, 1.2, 1, "#e8d8c0"),
         ears=(1, 2, 0.8, "#6a3418", 1.3), tail=(3, 3, 4, "#c0602e", 0.5), belly="#f2e6d0")
    + [B(-1.5, 4.5, -7.0, 3, 6, 3.5, "#c0602e", bone="tail")],
    "traklattrare",
    sp({"URSKOG": 0.40, "BOKSKOG": 0.40, "TALLHED": 0.30, "BRANNFALT": 0.05}, [0.5, 0.6, 0.9, 1, 1, 1, 1, 0.8], "diurnal"),
    tune=dict(graze=False, sniff=True, run_style="bound", hop_h=2.5),
    hp=2.0, speed=(1.4, 2.4), flee=6.4, count=50, group=(1, 2), move=(0.4, 2.0), pause=(1.0, 4.0))

add("igelkott", "Igelkott", hedgehog(8, 4, "#5e4a38", "#b8a088", "#d8c8b0"),
    "nattkrypare",
    sp({"HAGMARK": 0.26, "BOKSKOG": 0.20, "ALVAR": 0.10}, WARM, "nocturnal"),
    hp=2.0, speed=(0.4, 0.9), flee=1.6, count=30, group=(1, 1), move=(1.0, 3.0), pause=(3.0, 8.0))

add("hermelin", "Hermelin",
    quad(10, 2.8, 2.8, 2, "#b8905a", leg="#a07a48", head=(2.8, 2.8, 3.4), head_col="#bc945c", snout=(1.4, 1.2, 1, "#f6f2e8"),
         ears=(1, 1, 0.8, "#a07a48", 1.2), tail=(1.2, 1.2, 5, "#b8905a", 0.4), belly="#f6f2e8")
    + [B(-0.7, 2.9, -10.0, 1.4, 1.4, 1.4, "#241e1a", bone="tail")],
    "vessla",
    sp({"FJALLHED": 0.20, "HAGMARK": 0.20, "ALVAR": 0.10, "FJALLBJORKSKOG": 0.05}, [1, 1, 1, 1, 1, 1, 1, 1], "crepuscular"),
    tune=dict(graze=False, sniff=True, run_style="bound", hop_h=1.6, attack=True),
    hp=3.0, speed=(1.4, 2.6), flee=6.0, chase=6.2, count=30, group=(1, 1), behavior="AGGRESSIVE", aggro=0.0,
    damage=1.0, reach=1.2, cooldown=0.9, knock=0.5, detect=0.0, aflee=0.0)

# -- Prédateurs terrestres (doc 03 §3.2) ---------------------------------------------------------
add("varg", "Varg",
    quad(18, 6.6, 8, 10.5, "#6f6a62", leg="#5a554e", head=(5.8, 5.8, 6), head_col="#78736a", snout=(3.4, 3, 3.6, "#a8a398"),
         nose=(1.8, 1.2, "#181818"), ears=(1.8, 2.6, 1, "#5a554e", 2), tail=(2.6, 2.8, 8, "#6a655e", 1.5), belly="#a8a398",
         mane=(5.5, 2.4, "#5a554e")) + [B(-1.4, 11.5, 10.7, 2.8, 1.2, 1.4, "#4a463f", bone="head")],
    "flockjagare",
    sp({}, [1, 1, 1, 1, 1, 1, 1, 1], "crepuscular", persistence="herd", avoid={"fire": 12},
       herd={"motion": "territory", "cell": 1024, "sizeMin": 3, "sizeMax": 7, "regrowPerDay": 0.3,
             "summer": ["URSKOG", "TALLHED", "FJALLBJORKSKOG", "MYR", "BERGSLAG"], "wander": 90, "materialize": 96}),
    tune=dict(graze=False, sniff=True, howl=True, run_style="bound", hop_h=2.5),
    hp=14.0, speed=(1.3, 2.2), flee=4.6, chase=6.0, count=60, group=(1, 1), behavior="AGGRESSIVE", aggro=0.0,
    damage=2.5, reach=1.8, cooldown=1.1, knock=2.5, detect=0.0, aflee=0.0)

add("bjorn", "Brunbjörn",
    quad(30, 15, 15, 12, "#4a3020", leg="#3a2416", head=(9, 8.6, 9.4), head_col="#503422", snout=(5, 4, 3.6, "#9a7850"),
         nose=(2.6, 1.6, "#181818"), ears=(2.6, 2.6, 1.4, "#3a2416", 3.8), tail=(2.6, 2.6, 1.6, "#3a2416", 1), belly="#5e3e28",
         hump=(10, 5, 10, 4, "#58382a")),
    "allatare",
    sp({"URSKOG": 0.040, "FJALLBJORKSKOG": 0.030, "BERGSLAG": 0.030, "ALV": 0.025}, ACTIVE, "diurnal",
       avoid={"fire": 12, "player": 40}),
    tune=dict(graze=False, sniff=True, heavy=1.5, rear=True, a_walk=24.0),
    hp=45.0, speed=(1.0, 1.6), flee=0.0, chase=5.2, count=14, group=(1, 1), behavior="AGGRESSIVE", aggro=0.0,
    damage=5.0, reach=2.3, cooldown=1.6, knock=6.0, detect=0.0, aflee=0.0)

add("lo", "Lodjur",
    quad(15, 5.6, 7.5, 9.5, "#c8a46a", leg="#b08c58", head=(5.4, 5.4, 5.4), head_col="#cfac72", snout=(2.6, 2.2, 2.2, "#e8d8b8"),
         nose=(1.4, 1, "#a86a6a"), ears=(1.8, 3.6, 1, "#a8844c", 2), tail=(2.2, 2.2, 3, "#9a7846", 1.2), belly="#eadcc0",
         spots=("#6e5230", [(0.1, 2, 3, 1.6), (0.9, 3, 6, 1.6), (0.1, 4, 9, 1.4), (0.9, 1, 11, 1.4)]))
    + [B(-3.4, 11, 9.6, 6.8, 2.2, 1.4, "#e8d8b8", bone="head")],
    "ahus",
    sp({"URSKOG": 0.020, "HOGKUST": 0.030, "FJALLBJORKSKOG": 0.010}, [1, 1, 1, 1, 1, 1, 1, 1], "crepuscular",
       avoid={"player": 40}),
    tune=dict(graze=False, sniff=True, run_style="bound", hop_h=2.0, reach=4.0, attack=True),
    hp=12.0, speed=(1.2, 2.0), flee=6.0, chase=6.6, count=10, group=(1, 1), behavior="AGGRESSIVE", aggro=0.0,
    damage=3.0, reach=1.8, cooldown=1.2, knock=2.0, detect=0.0, aflee=0.0)

add("jarv", "Järv",
    quad(16, 7.5, 8.5, 5.5, "#3e2c1e", leg="#2a1e14", head=(5.8, 5.8, 6.2), head_col="#4a3626", snout=(3, 2.8, 2.6, "#2c2018"),
         nose=(1.6, 1.1, "#101010"), ears=(1.6, 1.6, 1, "#2a1e14", 2.1), tail=(3, 3.2, 7, "#33261a", 1), belly="#4a3626")
    + [B(-W2 - 0.15, 5.5 + 2.2, -6.0, 2 * W2 + 0.3, 2.4, 11.0, "#c8a870", bone="body") for W2 in (3.75,)],
    "rovtjuv",
    sp({"FJALLBJORKSKOG": 0.030, "FJALLHED": 0.030, "KALFJALL": 0.010}, [1, 1, 1, 1, 1, 1, 1, 1], "always",
       avoid={"player": 40}),
    tune=dict(graze=False, sniff=True, heavy=1.2, run_style="bound", hop_h=1.8, attack=True),
    hp=18.0, speed=(1.2, 1.9), flee=4.4, chase=5.2, count=12, group=(1, 1), behavior="AGGRESSIVE", aggro=0.0,
    damage=3.5, reach=1.7, cooldown=1.2, knock=2.5, detect=0.0, aflee=0.0)

add("fjallrav", "Fjällräv",
    quad(11, 5, 5, 5, "#d8d6cc", leg="#bdbbb0", head=(4.4, 4, 4.5), head_col="#e0ded4", snout=(2.2, 1.8, 2.2, "#e8e6dc"),
         nose=(1.4, 1, "#202020"), ears=(1.6, 2.4, 1, "#c8c6bc", 1.3), tail=(3, 3, 7, "#e4e2d8", 0.5), belly="#ecebe4"),
    "fjallrav",
    sp({"FJALLHED": 0.10, "KALFJALL": 0.10, "PACKIS": 0.08, "LAVAFALT": 0.05, "GLACIAR": 0.03}, [1, 1, 1, 1, 1, 1, 1, 1], "crepuscular"),
    tune=dict(graze=False, sniff=True, run_style="bound", hop_h=2.5, attack=True),
    hp=5.0, speed=(1.3, 2.2), flee=5.6, chase=5.6, count=20, group=(1, 2), behavior="AGGRESSIVE", aggro=0.0,
    damage=1.0, reach=1.3, cooldown=1.0, knock=0.5, detect=0.0, aflee=0.0)

add("rodrav", "Rödräv",
    quad(12, 5, 6, 7.5, "#c4622a", leg="#2a1c16", head=(4.6, 4.4, 5), head_col="#cc6a2e", snout=(2.6, 2, 3.2, "#e6d6c0"),
         nose=(1.4, 1, "#101010"), ears=(1.8, 3, 1, "#2a1c16", 1.8), tail=(3.4, 3.4, 10, "#c4622a", 0.8), belly="#f0e4d0")
    + [B(-1.4, 6.8, -16.0, 2.8, 3.4, 2.4, "#f4efe6", bone="tail")],
    "rav",
    sp({"HAGMARK": 0.12, "BOKSKOG": 0.10, "SJOLAND": 0.08, "STRANDANG": 0.06, "ALVAR": 0.10}, [0.8, 0.8, 1, 1, 1, 1, 1, 0.9], "crepuscular"),
    tune=dict(graze=False, sniff=True, run_style="bound", hop_h=2.5, attack=True),
    hp=6.0, speed=(1.2, 2.2), flee=5.8, chase=5.6, count=25, group=(1, 2), behavior="AGGRESSIVE", aggro=0.0,
    damage=1.2, reach=1.4, cooldown=1.0, knock=0.5, detect=0.0, aflee=0.0)

add("mard", "Mård",
    quad(10, 3.8, 4, 5, "#5a3c26", leg="#3e2818", head=(3.8, 3.6, 4.4), head_col="#60402a", snout=(1.8, 1.6, 1.6, "#2a1c12"),
         ears=(1.4, 1.6, 0.8, "#e8d8b8", 1.6), tail=(2.4, 2.4, 8, "#4a2e1c", 0.8), belly="#e4c070"),
    "mardjakt",
    sp({"URSKOG": 0.12, "BOKSKOG": 0.02}, [1, 1, 1, 1, 1, 1, 1, 1], "always"),
    tune=dict(graze=False, sniff=True, run_style="bound", hop_h=2.0, attack=True),
    hp=4.0, speed=(1.3, 2.4), flee=5.8, chase=5.8, count=20, group=(1, 1), behavior="AGGRESSIVE", aggro=0.0,
    damage=1.0, reach=1.3, cooldown=0.9, knock=0.5, detect=0.0, aflee=0.0)

add("isbjorn", "Isbjörn",
    quad(34, 17, 17, 14, "#f1efe6", leg="#e2e0d6", head=(10, 8.6, 11), head_col="#f4f2ea", snout=(5.4, 4.4, 4.6, "#e8e6dc"),
         nose=(2.6, 1.6, "#181818"), ears=(2.4, 2.2, 1.2, "#e2e0d6", 4), tail=(2.4, 2.4, 1.4, "#e8e6dc", 1), belly="#fafaf4"),
    "isrovdjur",
    sp({"PACKIS": 0.030}, [1, 1, 0.9, 0.7, 0.6, 0.6, 0.8, 1], "diurnal", avoid={"fire": 12}),
    tune=dict(graze=False, sniff=True, heavy=1.5, rear=True, a_walk=24.0),
    hp=60.0, speed=(1.0, 1.7), flee=0.0, chase=5.4, count=8, group=(1, 1), medium="ICE", behavior="AGGRESSIVE", aggro=0.0,
    damage=6.0, reach=2.5, cooldown=1.6, knock=7.0, detect=0.0, aflee=0.0)


def viper():
    """Serpent : le corps de lézard avec des pattes réduites à des points (l'animation d'ondulation reste celle du lézard)."""
    b = lizard(2.6, 11, "#8a8068", "#c8c0a8", "#2a2420")
    for box in b:
        if box.bone and box.bone.startswith("leg_"):
            box[3] = box[4] = box[5] = 0.3
            box[4] = 0.5
    return b


add("huggorm", "Huggorm", viper(),
    "orm",
    sp({"TALLHED": 0.15, "ALVAR": 0.15, "SKARGARD": 0.10, "MYR": 0.10}, WARM, "diurnal", weather={"rain": 0.3, "clear": 1.0}),
    hp=2.0, speed=(0.4, 0.9), flee=1.6, chase=2.4, count=30, group=(1, 1), move=(1.0, 3.0), pause=(3.0, 8.0),
    behavior="AGGRESSIVE", aggro=0.0, damage=1.0, reach=1.1, cooldown=1.5, knock=0.2, detect=0.0, aflee=0.0)


def write():
    cfgs = [c for c, _ in MOBS]
    out = os.path.join(ROOT, "assets/norrcraft")
    with open(os.path.join(out, "mobs.json"), "w") as f:
        json.dump({"mobs": cfgs}, f, indent=2)
        f.write("\n")
    with open(os.path.join(out, "mob_geometry.json"), "w") as f:
        f.write("{\n")
        for i, (c, boxes) in enumerate(MOBS):
            f.write('  "%s": [\n' % c["id"])
            f.write(",\n".join("    " + json.dumps(list(b)) for b in boxes))
            f.write("\n  ]%s\n" % ("," if i < len(MOBS) - 1 else ""))
        f.write("}\n")
    with open(os.path.join(out, "mob_rigs.json"), "w") as f:
        f.write("{\n")
        items = list(RIGS.items())
        for i, (k, rig) in enumerate(items):
            f.write('  "%s": %s%s\n' % (k, json.dumps(rig, separators=(",", ":")), "," if i < len(items) - 1 else ""))
        f.write("}\n")
    print(len(cfgs), "mobs,", sum(len(b) for _, b in MOBS), "boîtes", file=sys.stderr)


if __name__ == "__main__":
    write()
    if "--preview" in sys.argv:  # planche des silhouettes (demande Pillow)
        import gen_mobs

        gen_mobs.MOBS[:] = MOBS
        gen_mobs.preview(sys.argv[sys.argv.index("--preview") + 1])
