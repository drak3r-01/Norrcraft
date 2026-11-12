#!/usr/bin/env python3
"""Bestiaire de Norrcraft (sessions 5.4 à 5.6, doc 03 §3.1 à §3.5).

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


# -- Oiseaux (doc 03 §3.3, session 5.5) ------------------------------------------------------------
SUMMER = [0, 0, 0.6, 1, 1, 1, 0.25, 0]  # migrateur : absent de novembre à février, parti en septembre
BIRD_AIR = dict(medium="AIR", speed=(1.4, 2.6), flee=6.0, move=(2.0, 6.0), pause=(1.0, 4.0))

add("tjader", "Tjäder",
    biped(13, 9, "#3a3a40", "#5a4a40", "#5a4a40", (3.8, 4, 4.6), "#2e2e34", (2.4, 2, 2.8, "#d8d0b8"), legh=7,
          neck_h=4, tail=(7, 3, 8, "#2a2a30"), wing=(1, 8, 8, "#4a3a2e"), crest=(3, 1.4, 2, "#b8302a"), leg_w=1.8, L=15),
    "tjaderfagel",
    sp({"URSKOG": 0.20, "TALLHED": 0.20}, [0.7, 0.7, 1, 1, 1, 1, 1, 0.8], "diurnal", avoid={"fire": 8}),
    hp=4.0, speed=(0.7, 1.4), flee=5.2, count=30, group=(1, 2), move=(0.8, 3.0), behavior="AGGRESSIVE", aggro=0.0,
    damage=1.0, reach=1.5, cooldown=2.0, knock=1.2, detect=0.0, aflee=0.0)

add("orre", "Orre",
    biped(9, 6, "#1e2230", "#2a2e3e", "#4a3a30", (3, 3.2, 3.6), "#1e2230", (1.8, 1.4, 2, "#2a2a2a"), legh=5,
          neck_h=2, tail=(6, 4, 6, "#161a26"), wing=(1, 6, 6, "#2e3244"), crest=(3, 1, 2, "#b8302a"), leg_w=1.4, L=11),
    "orrfagel",
    sp({"MYR": 0.20, "BRANNFALT": 0.15}, [0.8, 0.8, 1, 1, 1, 1, 1, 0.9], "crepuscular", avoid={"fire": 8}),
    hp=3.0, speed=(0.7, 1.4), flee=5.0, count=40, group=(3, 7), move=(0.8, 3.0), herding=True)

add("ripa", "Fjällripa",
    biped(7, 5, "#a89c88", "#f0eee6", "#e8e6de", (3, 3, 3.2), "#a89c88", (1.4, 1.2, 1.6, "#2a2a2a"), legh=3,
          neck_h=1, tail=(3, 2, 3, "#e8e6de"), wing=(1, 4, 5, "#f4f2ea"), crest=(2.4, 0.8, 2, "#c0342c"), leg_w=1.6, L=9),
    "snohona",
    sp({"FJALLHED": 0.30, "FJALLBJORKSKOG": 0.25}, [1, 1, 1, 1, 1, 1, 1, 1], "diurnal"),
    hp=2.0, speed=(0.6, 1.2), flee=5.0, count=50, group=(2, 5), move=(0.8, 3.0), herding=True)

add("trana", "Trana",
    biped(10, 6, "#8c8f94", "#a8aaae", "#3a3a3e", (3, 3, 4), "#3a3a3e", (1.4, 1.2, 5, "#7a7a6a"), legh=20,
          neck_h=16, tail=(5, 4, 4, "#2a2a2e"), wing=(1, 8, 12, "#9a9da2"), neck_col="#d8d8d4", crest=(2.4, 1, 2.4, "#b8302a"),
          leg_w=1.2, L=15),
    "flyttfagel",
    sp({"MYR": 0.08, "STRANDANG": 0.08, "HAGMARK": 0.05}, SUMMER, "diurnal", weather={"clear": 1.0, "rain": 0.6, "mist": 0.5}),
    tune=dict(one_leg=True),
    hp=6.0, group=(2, 4), count=24, herding=True, **BIRD_AIR)

add("sangsvan", "Sångsvan",
    biped(10, 9, "#f4f2ec", "#fafaf6", "#2a2a2a", (3.4, 3.4, 4), "#f0eee8", (2, 1.6, 4, "#e6b830"), legh=3,
          neck_h=16, tail=(5, 3, 4, "#f0eee6"), wing=(1, 8, 12, "#faf8f2"), neck_col="#f2f0ea", leg_w=1.8, L=16),
    "svan",
    sp({"SJOLAND": 0.08, "BOTTENHAV": 0.05}, [0.3, 0.3, 0.8, 1, 1, 1, 1, 0.6], "diurnal"),
    hp=10.0, speed=(0.6, 1.2), flee=3.6, count=16, group=(2, 2), move=(1.0, 4.0), pause=(2.0, 6.0), medium="AMPHIBIAN",
    behavior="AGGRESSIVE", aggro=0.0, damage=2.5, reach=1.8, cooldown=1.6, knock=3.0, detect=0.0, aflee=0.0)

add("grasgas", "Grågås",
    biped(8, 6, "#8a7e72", "#c8beb0", "#d89860", (3, 3, 3.6), "#8a7e72", (1.8, 1.4, 3, "#e0a050"), legh=5,
          neck_h=6, tail=(4, 2, 3, "#e8e4dc"), wing=(1, 6, 9, "#7a6e62"), neck_col="#948878", leg_w=1.4, L=12),
    "flyttfagel",
    sp({"STRANDANG": 0.15, "SKARGARD": 0.12, "HAGMARK": 0.05}, SUMMER, "diurnal", weather={"clear": 1.0, "rain": 0.6, "mist": 0.5}),
    hp=5.0, group=(4, 9), count=40, herding=True, **BIRD_AIR)

add("ejder", "Ejder",
    biped(8, 7, "#f2eee6", "#2a2a30", "#8a8a7a", (3.4, 3.6, 4), "#f4f0e8", (2, 1.8, 3.4, "#b8c070"), legh=2,
          neck_h=3, tail=(4, 2, 3, "#2a2a30"), wing=(1, 6, 8, "#f6f4ee"), leg_w=1.6, L=14),
    "ejderfagel",
    sp({"SKARGARD": 0.30, "FJORD": 0.15}, [0.5, 0.5, 0.9, 1, 1, 1, 1, 0.7], "diurnal"),
    hp=4.0, speed=(0.6, 1.2), flee=3.6, count=40, group=(3, 8), move=(1.0, 4.0), medium="AMPHIBIAN", herding=True)

add("lunne", "Lunne",
    biped(6, 4, "#1e2026", "#f4f2ee", "#e8782a", (3, 3.2, 3.6), "#2a2a30", (2.4, 2.4, 2.6, "#e86a28"), legh=2,
          neck_h=0, tail=(2, 1, 2, "#1e2026"), wing=(1, 5, 7, "#1e2026"), foot="#e8782a", leg_w=1.2, L=8),
    "havsfagel",
    sp({"FJORD": 0.20, "KLINTKUST": 0.30, "LAVAFALT": 0.05}, [0, 0, 0.2, 1, 1, 1, 0.4, 0], "diurnal", altitude=3),
    hp=2.0, group=(3, 8), count=40, herding=True, **{**BIRD_AIR, "speed": (1.8, 3.0)})

add("havsorn", "Havsörn",
    biped(10, 8, "#5a4630", "#7a6244", "#e0b030", (3.8, 4, 4.4), "#f4f0e8", (2.2, 2.8, 3.2, "#e8c030"), legh=5,
          neck_h=0, tail=(5, 2, 6, "#f6f4ee"), wing=(1, 9, 14, "#4a3822"), foot="#e0b030", leg_w=1.8, L=13),
    "rovfagel",
    sp({"FJORD": 0.04, "SKARGARD": 0.05, "ALV": 0.03}, [1, 1, 1, 1, 1, 1, 1, 1], "diurnal", altitude=20, avoid={"fire": 8}),
    hp=10.0, group=(1, 1), count=10, **{**BIRD_AIR, "speed": (1.6, 2.8)})

add("fjalluggla", "Fjälluggla",
    biped(10, 8, "#f2f0ea", "#f8f6f0", "#e8e6de", (4.6, 4.6, 4), "#f6f4ee", (1.2, 1.4, 1.4, "#2a2a2a"), legh=3,
          neck_h=0, tail=(4, 2, 5, "#e6e4dc"), wing=(1, 8, 10, "#e8e6de"), foot="#e8e6de", leg_w=1.8, L=12),
    "nattrovfagel",
    sp({"FJALLHED": 0.04}, [1, 1, 1, 1, 1, 1, 1, 1], "always", altitude=8),
    hp=6.0, group=(1, 1), count=10, **{**BIRD_AIR, "speed": (1.4, 2.4)})

add("kattuggla", "Kattuggla",
    biped(8, 7, "#7a6246", "#c8b896", "#a89878", (4.6, 4.4, 4), "#8a7050", (1.2, 1.4, 1.4, "#c8a040"), legh=3,
          neck_h=0, tail=(4, 2, 5, "#6a523a"), wing=(1, 7, 9, "#6a523a"), leg_w=1.6, L=10),
    "uggla",
    sp({"BOKSKOG": 0.08, "URSKOG": 0.06}, [1, 1, 1, 1, 1, 1, 1, 1], "nocturnal", altitude=6),
    hp=3.0, group=(1, 1), count=16, **{**BIRD_AIR, "speed": (1.2, 2.2)})

add("korp", "Korp",
    biped(8, 6, "#14161c", "#1e2028", "#14161c", (3.4, 3.6, 4.2), "#16181e", (2.6, 2.2, 3.6, "#101216"), legh=4,
          neck_h=1, tail=(4, 2, 7, "#101216"), wing=(1, 7, 11, "#14161c"), foot="#14161c", leg_w=1.4, L=13),
    "korp",
    sp({"URSKOG": 0.05, "TALLHED": 0.05, "FJALLBJORKSKOG": 0.05, "FJALLHED": 0.04, "BERGSLAG": 0.05, "HAGMARK": 0.04,
        "ALVAR": 0.03, "BRANNFALT": 0.03, "KALFJALL": 0.02, "LAVAFALT": 0.02},
       [1, 1, 1, 1, 1, 1, 1, 1], "diurnal", altitude=14),
    hp=4.0, group=(1, 2), count=24, **BIRD_AIR)

add("hackspett", "Spillkråka",
    biped(7, 5, "#16181e", "#22242c", "#4a4a4a", (3, 3.2, 3.6), "#16181e", (1.2, 1.2, 3.4, "#d8d4c0"), legh=3,
          neck_h=0, tail=(3, 2, 4, "#101216"), wing=(1, 5, 7, "#16181e"), crest=(2.6, 1.4, 2.2, "#c0302a"), leg_w=1.2, L=9),
    "skogsfagel",
    sp({"URSKOG": 0.15, "BRANNFALT": 0.15}, [0.8, 0.8, 1, 1, 1, 1, 1, 0.9], "diurnal", altitude=4),
    hp=2.0, group=(1, 1), count=30, **{**BIRD_AIR, "speed": (1.0, 2.0), "move": (1.0, 4.0)})

add("mas", "Mås",
    biped(6, 5, "#f4f4f2", "#fafaf8", "#e0a040", (3, 3.2, 3.6), "#f6f6f4", (1.6, 1.6, 3, "#e8b830"), legh=3,
          neck_h=1, tail=(3, 2, 3, "#f0f0ee"), wing=(1, 6, 11, "#aab0b8"), foot="#e0a040", leg_w=1.2, L=11),
    "havsfagel",
    sp({"SKARGARD": 0.40, "KLINTKUST": 0.30, "FJORD": 0.30, "STRANDANG": 0.20, "BOTTENHAV": 0.20, "VASTERHAV": 0.10,
        "HOGKUST": 0.20},
       [0.7, 0.7, 1, 1, 1, 1, 1, 0.8], "diurnal", altitude=10),
    hp=3.0, group=(2, 6), count=50, herding=True, **BIRD_AIR)

add("goken", "Gök",
    biped(6, 4, "#8a8e98", "#d8d4cc", "#e8c040", (2.8, 3, 3.6), "#7e828c", (1.2, 1.2, 2.2, "#2a2a2a"), legh=3,
          neck_h=0, tail=(3, 2, 8, "#6e727c"), wing=(1, 5, 8, "#74788a"), leg_w=1.0, L=10),
    "skogsfagel",
    sp({"HAGMARK": 0.08, "FJALLBJORKSKOG": 0.08}, [0, 0, 0, 0.5, 1, 1, 0.1, 0], "diurnal", altitude=6),
    hp=2.0, group=(1, 1), count=16, **{**BIRD_AIR, "speed": (1.4, 2.4)})



# -- Faune aquatique, amphibie et insectes (doc 03 §3.4 et §3.5, session 5.6) -------------------
RUN = [0, 0, 0.1, 0.4, 1, 1, 0.7, 0.1]  # remontée de juin à septembre
LATE = [0, 0, 0, 0.1, 0.5, 1, 0.7, 0.1]  # fin d'été
SUMMER_ONLY = [0, 0, 0, 0.3, 1, 1, 0.2, 0]  # insectes de juin à août
FISH_WATER = dict(medium="WATER", flee_dur=(1.5, 3.0), move=(1.5, 5.0), pause=(0.2, 1.5))

add("lax", "Lax",
    fish(10, 3.6, 2.2, "#8f9ca6", belly="#e4dcd2", fin="#6e7a86", tail="#6e7a86", stripes=("#4a5560", 2)),
    "lax",
    sp({"ALV": 0.30, "BOTTENHAV": 0.12, "VASTERHAV": 0.06, "FJORD": 0.05}, RUN, "always"),
    hp=3.0, speed=(0.9, 1.8), flee=3.8, count=30, group=(3, 6), herding=True, **FISH_WATER)

add("oring", "Öring",
    fish(8, 3, 2, "#7a8a58", belly="#e6e0c0", fin="#6a7a4a", stripes=("#3e4a2a", 3)),
    "fisk",
    sp({"ALV": 0.35, "SJOLAND": 0.20, "FJALLBJORKSKOG": 0.08}, [0.5, 0.5, 0.8, 1, 1, 1, 1, 0.8], "always"),
    hp=2.0, speed=(0.7, 1.5), flee=3.4, count=30, group=(1, 3), **FISH_WATER)

add("roding", "Röding",
    fish(8, 3, 2, "#56687a", belly="#e8683a", fin="#e8683a", stripes=None),
    "fisk",
    sp({"FJALLHED": 0.18, "KALFJALL": 0.12, "FJALLBJORKSKOG": 0.10}, [1, 1, 1, 1, 1, 1, 1, 1], "always"),
    hp=2.0, speed=(0.6, 1.3), flee=3.2, count=24, group=(1, 3), **FISH_WATER)

add("gadda", "Gädda",
    fish(14, 3.8, 2.4, "#5e7a3e", belly="#cdd2a0", fin="#4a6030", snout=(2.6, 1.8, 3, "#5e7a3e"), stripes=("#3e5a2a", 3)),
    "gaddhane",
    sp({"SJOLAND": 0.22, "BOTTENHAV": 0.08, "ALV": 0.05}, [0.3, 0.3, 0.8, 1, 1, 1, 1, 0.7], "always"),
    hp=6.0, speed=(0.7, 1.4), flee=3.2, chase=4.8, count=14, group=(1, 1), behavior="AGGRESSIVE", aggro=0.0,
    damage=1.5, reach=1.3, cooldown=1.2, knock=0.5, detect=0.0, aflee=0.0, **FISH_WATER)

add("abborre", "Abborre",
    fish(6, 3.2, 1.8, "#6a7a3a", belly="#e8d890", fin="#d8502a", stripes=("#2a3a1a", 4)),
    "fisk",
    sp({"SJOLAND": 0.40, "BOTTENHAV": 0.20, "ALV": 0.10}, [0.7, 0.7, 0.9, 1, 1, 1, 1, 0.9], "diurnal"),
    hp=1.0, speed=(0.6, 1.3), flee=3.2, count=40, group=(3, 7), herding=True, **FISH_WATER)

add("stromming", "Strömming",
    fish(6, 2.4, 1.4, "#8aa0b0", belly="#e8eef0", fin="#6c8294"),
    "stim",
    sp({"BOTTENHAV": 0.60, "VASTERHAV": 0.30, "SKARGARD": 0.40}, [0.5, 0.5, 0.8, 1, 1, 1, 1, 0.7], "always"),
    hp=1.0, speed=(0.9, 1.6), flee=3.8, count=90, group=(6, 14), herding=True, **FISH_WATER)

add("torsk", "Torsk",
    fish(14, 4.6, 2.8, "#8a7a5a", belly="#e8e0c8", fin="#6e6048", stripes=("#5e5238", 2)),
    "skrei",
    sp({"FJORD": 0.25, "VASTERHAV": 0.12, "HOGKUST": 0.05}, [1, 1, 0.9, 0.4, 0.3, 0.3, 0.4, 0.7], "always"),
    hp=5.0, speed=(0.7, 1.4), flee=3.4, count=30, group=(2, 5), herding=True, **FISH_WATER)

add("al_fisk", "Ål",
    fish(18, 1.6, 1.4, "#3a3a2a", belly="#c8c070", fin="#2e2e22", dorsal=False),
    "fisk",
    sp({"BOKSKOG": 0.15, "ALV": 0.08, "SJOLAND": 0.04, "HOGKUST": 0.06, "STRANDANG": 0.05}, [0, 0, 0.2, 0.6, 1, 1, 0.8, 0.2], "nocturnal"),
    hp=2.0, speed=(0.5, 1.1), flee=2.8, count=20, group=(1, 2), **FISH_WATER)

add("krafta", "Kräfta",
    crab(6, 5, "#4a3a2c", "#5a4638", "#6a523e"),
    "kraftdjur",
    sp({"SJOLAND": 0.20, "ALV": 0.20}, [0, 0, 0.3, 0.7, 1, 1, 1, 0.3], "nocturnal"),
    hp=2.0, speed=(0.4, 0.9), flee=2.4, count=40, group=(2, 4), medium="AMPHIBIAN", depth=1)

add("brannmanet", "Brännmanet",
    jelly(6, 9, "#d8946a", "#c86a4a", 0.62),
    "manet",
    sp({"VASTERHAV": 0.10, "FJORD": 0.10, "SKARGARD": 0.05}, LATE, "always"),
    hp=2.0, speed=(0.25, 0.6), flee=1.0, chase=0.8, count=20, group=(2, 5), scatter=5, behavior="AGGRESSIVE", aggro=0.0,
    damage=1.0, reach=1.3, cooldown=2.0, knock=0.0, detect=0.0, aflee=0.0, move=(1.5, 4.0), pause=(0.3, 1.5),
    medium="WATER")

add("grasal", "Gråsäl",
    quad(18, 7.4, 6.5, 1.6, "#6e6e74", leg="#5a5a60", belly="#a8a8ac", legw=2.4, head=(5, 4.8, 6), head_col="#76767c",
         snout=(2.6, 2, 2.2, "#3a3a3e"), nose=(1.4, 1, "#101010"), tail=(2, 2, 3, "#5a5a60", 0.8)),
    "sal",
    sp({"SKARGARD": 0.22, "VASTERHAV": 0.05, "KLINTKUST": 0.06}, [1, 1, 1, 1, 1, 1, 1, 1], "always"),
    tune=dict(graze=False, sniff=True, heavy=0.8, paw=False),
    hp=12.0, speed=(0.5, 1.1), flee=3.6, count=24, group=(2, 5), herding=True, medium="AMPHIBIAN")

add("vikare", "Vikare",
    quad(13, 6, 5.4, 1.4, "#5a5e66", leg="#464a52", belly="#c8ccd2", legw=2, head=(4.4, 4.2, 5), head_col="#62666e",
         snout=(2.2, 1.8, 2, "#2e3036"), nose=(1.2, 0.9, "#101010"), tail=(1.8, 1.8, 2.4, "#464a52", 0.6),
         spots=("#c8ccd2", [(0.1, 3, 3, 1.6), (0.9, 3, 7, 1.6), (0.1, 3, 10, 1.4)])),
    "sal",
    sp({"PACKIS": 0.06, "BOTTENHAV": 0.04}, [1, 1, 0.9, 0.6, 0.5, 0.5, 0.7, 0.9], "always"),
    tune=dict(graze=False, sniff=True, heavy=0.7, paw=False),
    hp=8.0, speed=(0.4, 0.9), flee=3.2, count=16, group=(1, 3), medium="ICE")

add("valross", "Valross",
    quad(30, 14, 11, 2.5, "#8a6a52", leg="#74563f", belly="#9a7a62", legw=4.4, head=(8, 8, 8), head_col="#8e6e56",
         snout=(7, 5, 3, "#a08060"), nose=(3, 1.4, "#181410"), tail=(2.4, 2.4, 2.6, "#74563f", 0.8))
    + [B(-3.4, 4.0, 22, 1.0, 5.0, 1.0, "#efe8d0", bone="head"), B(2.4, 4.0, 22, 1.0, 5.0, 1.0, "#efe8d0", bone="head")],
    "valross",
    sp({"PACKIS": 0.030}, [1, 1, 0.9, 0.7, 0.6, 0.6, 0.8, 1], "always", avoid={"fire": 8}),
    tune=dict(graze=False, sniff=True, heavy=1.6, a_walk=24.0, attack=True),
    hp=40.0, speed=(0.5, 0.9), flee=0.0, chase=3.8, count=12, group=(3, 6), herding=True, medium="ICE",
    behavior="AGGRESSIVE", aggro=0.0, damage=5.0, reach=2.4, cooldown=1.8, knock=7.0, detect=0.0, aflee=0.0)

add("tumlare", "Tumlare",
    fish(16, 5, 3.4, "#5e6a78", belly="#d8dee4", fin="#4e5a68", snout=(2, 1.8, 3, "#5e6a78"), vertical=True),
    "delfin",
    sp({"VASTERHAV": 0.08, "BOTTENHAV": 0.05, "FJORD": 0.04}, [0.8, 0.8, 0.9, 1, 1, 1, 1, 0.9], "diurnal"),
    hp=8.0, speed=(1.4, 2.6), flee=4.0, count=16, group=(2, 5), herding=True, depth=3, **FISH_WATER)

add("utter", "Utter",
    quad(14, 4.4, 4.6, 2.2, "#5a4030", leg="#46301f", belly="#d8c8a8", head=(4, 4, 4.6), head_col="#5e4434",
         snout=(2, 1.6, 1.8, "#d8c8a8"), nose=(1.2, 0.9, "#101010"), ears=(1, 1, 0.8, "#46301f", 1.2),
         tail=(2.2, 2.2, 9, "#4a3524", 0.8)),
    "utter",
    sp({"ALV": 0.15, "SJOLAND": 0.12, "FJORD": 0.08}, [0.9, 0.9, 1, 1, 1, 1, 1, 1], "crepuscular"),
    tune=dict(graze=False, sniff=True, run_style="bound", hop_h=2.0, paw=False),
    hp=6.0, speed=(0.9, 1.7), flee=4.6, count=14, group=(1, 3), medium="AMPHIBIAN")

add("hakarl", "Hákarl",
    fish(40, 10, 7, "#5a5a64", belly="#a8a8b0", fin="#46464e", snout=(4, 3.4, 4, "#5a5a64")),
    "djuphav",
    sp({"DJUPHAV": 0.015, "FJORD": 0.010}, [1, 1, 1, 1, 1, 1, 1, 1], "always"),
    hp=30.0, speed=(0.3, 0.7), flee=1.2, count=4, group=(1, 1), depth=8, **FISH_WATER)

add("val", "Rätval",
    fish(96, 26, 24, "#2e343e", belly="#d8dce2", fin="#262b34", snout=(10, 6, 8, "#2e343e"), vertical=True),
    "val",
    sp({"VASTERHAV": 0.010, "DJUPHAV": 0.015}, [0.8, 0.8, 0.6, 0.7, 0.7, 0.8, 1, 1], "always"),
    hp=80.0, speed=(0.5, 1.0), flee=1.4, count=4, group=(1, 1), depth=10, **FISH_WATER)

# Insectes (§3.5) : des nuées sans dégâts ; la démangeaison et la visée arrivent avec la survie nordique.
TINY_AIR = dict(medium="AIR", speed=(0.8, 1.6), flee=4.0, move=(1.0, 3.0), pause=(0.5, 2.0), herding=True)

add("mygg", "Mygg",
    biped(1.2, 1, "#3a3a3a", "#6a6a6a", "#2a2a2a", (1, 1, 1.2), "#3a3a3a", (0.2, 0.2, 1, "#2a2a2a"), legh=0.5,
          wing=(0.3, 1.2, 2.2, "#d0d8e0"), L=2.4, leg_w=0.3),
    "skymning",
    sp({"MYR": 0.80, "PALSMYR": 0.60, "URSKOG": 0.50, "FJALLBJORKSKOG": 0.40}, SUMMER_ONLY, "crepuscular",
       weather={"wind": 0.1, "rain": 0.4, "clear": 1.0}, altitude=2),
    hp=1.0, count=120, group=(6, 14), **TINY_AIR)

add("knott", "Knott",
    biped(1, 0.9, "#1e1e22", "#3a3a40", "#1e1e22", (0.9, 0.9, 1), "#1e1e22", (0.2, 0.2, 0.8, "#1e1e22"), legh=0.4,
          wing=(0.3, 1, 1.8, "#c8d0d8"), L=2, leg_w=0.3),
    "skymning",
    sp({"ALV": 0.80, "FJALLBJORKSKOG": 0.30, "BERGSLAG": 0.30}, [0, 0, 0, 0.2, 1, 1, 0.4, 0], "diurnal",
       weather={"wind": 0.2, "rain": 0.3, "clear": 1.0}, nearFeature="water", altitude=2),
    hp=1.0, count=80, group=(5, 12), **TINY_AIR)

add("bi", "Bi",
    biped(1.6, 1.4, "#e0b428", "#2a2418", "#2a2418", (1.4, 1.4, 1.4), "#e0b428", (0.3, 0.3, 0.6, "#2a2418"), legh=0.5,
          wing=(0.3, 1.4, 2.6, "#d8e8f0"), L=2.8, leg_w=0.3),
    "insekt",
    sp({"HAGMARK": 0.60, "BOKSKOG": 0.40, "STRANDANG": 0.40, "ALVAR": 0.40, "URSKOG": 0.10}, [0, 0, 0.1, 0.8, 1, 1, 0.5, 0],
       "diurnal", weather={"rain": 0.1, "mist": 0.4, "wind": 0.3, "clear": 1.0}, altitude=2),
    hp=1.0, count=60, group=(1, 3), **TINY_AIR)

add("fjaril", "Fjäril",
    biped(1.4, 1.2, "#3a2e22", "#e08a2a", "#2a2018", (1.2, 1.2, 1.2), "#3a2e22", (0.2, 0.2, 0.8, "#2a2018"), legh=3.5,
          wing=(0.3, 3.6, 4.4, "#e8902e"), L=3, leg_w=0.3),
    "insekt",
    sp({"HAGMARK": 0.50, "STRANDANG": 0.40, "ALVAR": 0.30, "BOKSKOG": 0.30, "FJALLHED": 0.10}, [0, 0, 0.1, 0.7, 1, 1, 0.4, 0],
       "diurnal", weather={"rain": 0.05, "mist": 0.3, "wind": 0.3, "clear": 1.0}, altitude=2),
    hp=1.0, count=60, group=(1, 2), **TINY_AIR)

add("trollslanda", "Trollslända",
    biped(1.2, 1.2, "#2e8a8a", "#7ad0d0", "#1e5a5a", (1.4, 1.4, 1.6), "#2e8a8a", (0.3, 0.3, 0.6, "#1e5a5a"), legh=0.5,
          tail=(0.8, 0.8, 5, "#2e8a8a"), wing=(0.3, 1.2, 4.4, "#cfe6f0"), L=3, leg_w=0.3),
    "insekt",
    sp({"SJOLAND": 0.40, "ALV": 0.30, "MYR": 0.30, "STRANDANG": 0.20}, SUMMER_ONLY, "diurnal",
       weather={"rain": 0.1, "mist": 0.4, "clear": 1.0}, altitude=2),
    hp=1.0, count=50, group=(1, 2), **{**TINY_AIR, "herding": False, "speed": (1.2, 2.4)})

add("lysmask", "Lysmask",
    lizard(1.2, 5, "#3a3a2a", "#a0e040", "#c8f060"),
    "lysmask",
    sp({"HAGMARK": 0.50, "BOKSKOG": 0.40, "URSKOG": 0.30, "STRANDANG": 0.30, "ALVAR": 0.20}, [0, 0, 0, 0.2, 1, 0.4, 0, 0],
       "nocturnal"),
    hp=1.0, speed=(0.1, 0.3), flee=0.6, count=50, group=(2, 6), move=(1.0, 3.0), pause=(3.0, 8.0))


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
