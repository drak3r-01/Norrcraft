#!/usr/bin/env python3
"""Génère les animaux de la phase 9 : `assets/vanilla/mobs_nb.json` (réglages) et
`assets/vanilla/mob_geometry_nb.json` (géométrie en boîtes colorées, unités = pixels MC, 16 px = 1 bloc).

Les animaux n'ont pas de texture : ils sont dessinés en boîtes colorées (leurs identifiants évitent
ceux des formes vanilla de nb-gfx — `sheep`, `wolf`, `rabbit`, `squid`… — qui auraient un damier) (comme le renne, le poisson
et l'orque d'origine). Quand Gabriel fournira de vraies textures / modèles `.jem`, ils prendront
le pas sur ces boîtes (ordre de résolution JEM → forme vanilla → géométrie).

Usage : python3 scripts/gen_mobs.py            # écrit les deux JSON
        python3 scripts/gen_mobs.py --preview  # écrit aussi une planche d'aperçu (PNG)
"""
import json
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mob_shapes import *  # noqa: E402,F401,F403 (B, quad, biped, fish, turtle, jelly, ray, squid, crab, ...)
from mob_rigs import build_rig  # noqa: E402
from mob_recipes import RECIPES  # noqa: E402
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")



# ------------------------------------------------------------------------------------------------
# Catalogue
# ------------------------------------------------------------------------------------------------
LAND_PASSIVE = dict(behaviorType="PASSIVE", medium="LAND")
WATER_PASSIVE = dict(behaviorType="PASSIVE", medium="WATER")


def cfg(id_, name, boxes, biomes, hp=6.0, speed=(1.0, 1.6), flee=4.2, count=8, group=(1, 3), scatter=3, move=(1.0, 5.0),
        pause=(2.0, 7.0), herding=False, surfaces=None, depth=0, max_depth=0, min_y=0, max_y=0, aggro=0.0,
        chase=0.0, damage=0.0, reach=0.0, cooldown=1.0, knock=0.0, medium="LAND", behavior="PASSIVE",
        flee_dur=(2.0, 4.0), detect=10.0, aflee=3.4, scale_w=0.92):
    xs0 = min(b[0] for b in boxes)
    xs1 = max(b[0] + b[3] for b in boxes)
    zs0 = min(b[2] for b in boxes)
    zs1 = max(b[2] + b[5] for b in boxes)
    ys1 = max(b[1] + b[4] for b in boxes)
    wx = max(0.4, round((xs1 - xs0) / 16.0 * scale_w, 3))
    wz = max(0.4, round((zs1 - zs0) / 16.0 * scale_w, 3))
    h = max(0.3, round(ys1 / 16.0, 3))
    c = {
        "id": id_, "displayName": name, "widthX": wx, "widthZ": wz, "height": h, "maxHealth": hp, "medium": medium,
        "spawnMinWaterDepth": depth, "maxCount": count, "groupSizeMin": group[0], "groupSizeMax": group[1],
        "spawnScatterRadius": scatter, "behaviorType": behavior,
        "moveSpeedMin": speed[0], "moveSpeedMax": speed[1], "moveDurationMin": move[0], "moveDurationMax": move[1],
        "pauseDurationMin": pause[0], "pauseDurationMax": pause[1],
        "herding": herding, "herdRadius": 6.0 if herding else 0.0, "herdBias": 0.35 if herding else 0.0,
        "aggressorDetectRange": detect, "aggressorFleeSpeed": aflee, "playerFleeSpeed": flee,
        "playerFleeDurationMin": flee_dur[0], "playerFleeDurationMax": flee_dur[1],
        "aggroPlayerRange": aggro, "chaseSpeed": chase, "attackRange": reach, "attackDamage": damage,
        "attackCooldown": cooldown, "knockbackForce": knock,
        "spawnBiomes": biomes, "spawnSurfaces": surfaces or [], "spawnMinY": min_y, "spawnMaxY": max_y,
        "spawnMaxWaterDepth": max_depth,
    }
    return c


MOBS = []  # (config, boxes)
RIGS = {}  # id -> rig


def speeds(c):
    """Vitesses de marche et de course (blocs/s) d'un mob, d'après ses réglages."""
    walk = (c["moveSpeedMin"] + c["moveSpeedMax"]) / 2
    run = max(c["playerFleeSpeed"], c["chaseSpeed"], c["aggressorFleeSpeed"], walk * 1.8)
    return {"walk": round(walk, 3), "run": round(max(run, walk * 1.45), 3)}


# Réglages d'animation propres à chaque mob (voir les recettes de `mob_recipes.py`).
TUNE = {
    "wild_boar": dict(graze=False, sniff=True, heavy=1.1, run_style="trot"),
    "elk": dict(heavy=1.25, a_walk=26.0),
    "arctic_fox": dict(graze=False, sniff=True, run_style="bound", hop_h=2.5),
    "hare": dict(walk_style="hop", run_style="hop", hop_w=1.2, hop_h=4.0, paw=False),
    "camel": dict(walk_style="pace", run_style="pace", heavy=1.4, a_walk=24.0),
    "fennec_fox": dict(graze=False, sniff=True, run_style="bound", hop_h=2.0),
    "mountain_goat": dict(run_style="trot", a_run=44.0, heavy=0.9),
    "capybara": dict(heavy=1.0, paw=False),
    "bison": dict(heavy=1.7, a_walk=24.0),
    "zebra": dict(run_style="trot", heavy=1.0),
    "giraffe": dict(walk_style="pace", run_style="pace", heavy=1.3, a_walk=22.0, paw=False),
    "mouflon": dict(heavy=1.0),
    "red_panda": dict(graze=False, sniff=True, run_style="bound", hop_h=2.0),
    "grey_wolf": dict(graze=False, sniff=True, howl=True, run_style="bound", hop_h=2.5),
    "brown_bear": dict(graze=False, sniff=True, heavy=1.5, rear=True, a_walk=24.0),
    "polar_bear": dict(graze=False, sniff=True, heavy=1.5, rear=True, a_walk=24.0),
    "panther": dict(graze=False, sniff=True, run_style="bound", hop_h=2.0, reach=4.0),
    "reindeer": dict(heavy=1.15, a_walk=27.0),
    "penguin": dict(style="waddle", a_walk=22.0, run_wings=30.0, peck_down=40.0),
    "flamingo": dict(one_leg=True, a_walk=20.0),
    "pheasant": dict(run_wings=26.0),
    "ostrich": dict(run_wings=34.0, a_walk=24.0),
    "pufferfish": dict(puff=0.35, stiff=0.45, amp=11.0),
    "shark": dict(attack=True, bite=4.0, amp=13.0),
    "piranha": dict(attack=True, bite=2.0, amp=18.0),
    "orca": dict(attack=True, bite=8.0, amp=8.0),
    "dolphin": dict(amp=10.0),
    "narwhal": dict(amp=8.0),
    "tuna": dict(amp=12.0, beat=0.9),
}


def add(id_, name, boxes, biomes, **kw):
    c = cfg(id_, name, boxes, biomes, **kw)
    MOBS.append((c, boxes))
    tune = dict(TUNE.get(id_, {}))
    if c["behaviorType"] == "AGGRESSIVE" and boxes.kind in ("quad", "fish", "arthropod"):
        tune.setdefault("attack", True)
    RIGS[id_] = build_rig(boxes, RECIPES[boxes.kind], speeds(c), tune)


F = ["FOREST", "BIRCH_FOREST", "DARK_FOREST", "AUTUMN_FOREST"]
G = ["PLAINS", "FLOWER_MEADOW"]
SNOW = ["SNOWY_PLAINS", "SNOWY_TAIGA", "ICE_SPIKES"]
TAIGA = ["TAIGA", "SNOWY_TAIGA", "MEGA_TAIGA"]

# -- Terrestres, pacifiques ----------------------------------------------------------------------
add("wild_boar", "Wild Boar",
    quad(18, 9, 9, 5, "#5a4636", leg="#3e2f24", hoof="#2a2018", head=(6, 6.5, 6), head_col="#4e3c2e", snout=(4.5, 3.4, 3, "#8a6a5a"),
         ears=(2, 2.6, 1, "#3e2f24", 2.2), mane=(2, 1.6, "#2c221a"), tail=(1, 3, 1, "#3e2f24", 1), belly="#7a6250",
         spots=None, nose=(3, 1.2, "#3a2a22")) + [B(-3.4, 8.0, 7.5, 1.1, 2.6, 1.0, "#f0e8d0", bone="head"), B(2.3, 8.0, 7.5, 1.1, 2.6, 1.0, "#f0e8d0", bone="head")],
    F + ["MEGA_TAIGA", "TAIGA"], hp=7.0, speed=(1.0, 1.8), flee=4.6, count=9, group=(2, 4), herding=True)
add("elk", "Elk",
    quad(26, 11, 13, 19, "#6e4c32", leg="#4e3624", hoof="#241a12", head=(6, 7.5, 11), head_col="#5e4129", neck=(6, 13, 8),
         snout=(4.5, 4.5, 3, "#3a2a20"), ears=(2.4, 4, 1, "#4e3624", 3.4), antler=(5, 9, "#d8c8a0"), tail=(2, 3, 1.5, "#d8cdb8", 1),
         belly="#8a6a4a", mane=(3, 3, "#3e2a1c")),
    ["TAIGA", "SNOWY_TAIGA", "MEGA_TAIGA", "HEATH", "MOUNTAINS", "HILLS"], hp=12.0, speed=(1.0, 1.7), flee=5.0, count=7, group=(1, 3), herding=True)
add("arctic_fox", "Arctic Fox",
    quad(11, 5, 5, 5, "#f2f4f6", leg="#d8dde2", head=(4.4, 4, 4.5), head_col="#f6f8fa", snout=(2.2, 1.8, 2.2, "#f0f0f0"), nose=(1.4, 1, "#202020"),
         ears=(1.6, 2.4, 1, "#e6eaee", 1.3), tail=(3, 3, 7, "#f8fafc", 0.5), belly="#e0e4ea"),
    SNOW + ["SNOWY_PEAKS"], hp=4.0, speed=(1.2, 2.2), flee=5.6, count=7, group=(1, 2), surfaces=["snow", "packed_ice", "ice", "stone"])
add("hare", "Hare",
    quad(7, 4, 4, 3, "#9a8068", leg="#7c6450", head=(3.6, 3.4, 3.6), head_col="#a28870", snout=(1.8, 1.4, 1.4, "#cdbfae"), ears=(1.2, 5, 0.9, "#8a7058", 0.9),
         tail=(2, 2, 1.4, "#f4f0e8", 0.6), belly="#d6cbb8"),
    G + F + ["HILLS", "HEATH", "CHERRY_GROVE", "SAVANNA"], hp=3.0, speed=(1.4, 2.8), flee=6.0, count=12, group=(1, 3), move=(0.4, 2.0), pause=(1.5, 5.0))
add("camel", "Camel",
    quad(30, 11, 13, 21, "#c49a62", leg="#a87e48", hoof="#6c5230", head=(5.6, 6.4, 8), head_col="#b88e56", neck=(5, 14, 6), snout=(4.4, 4, 2.6, "#e2c498"),
         ears=(1.6, 2.4, 1, "#a87e48", 2.6), tail=(1.4, 5, 1.2, "#a87e48", 1), hump=(7, 5.5, 8, -5, "#c49a62"), belly="#d8b87c")
    + [B(-4, 29, -7, 8, 1.2, 8, "#8c3a30")],
    ["DESERT", "BADLANDS"], hp=14.0, speed=(0.9, 1.5), flee=4.2, count=5, group=(1, 3), herding=True, surfaces=["sand", "red_sand", "terracotta", "sandstone"])
add("fennec_fox", "Fennec Fox",
    quad(9, 4, 4, 4, "#e2c598", leg="#caa878", head=(4.4, 3.8, 4), head_col="#e8cfa4", snout=(2, 1.6, 2, "#f0dcc0"), nose=(1.2, 1, "#202020"),
         ears=(2.4, 6, 0.9, "#d8b888", 1.5), tail=(2.4, 2.6, 6, "#c8a878", 0.6), belly="#f3e6d2"),
    ["DESERT", "BADLANDS"], hp=3.0, speed=(1.2, 2.4), flee=5.8, count=7, group=(1, 2), surfaces=["sand", "red_sand", "terracotta"])
add("mountain_goat", "Mountain Goat",
    quad(14, 7, 8, 9, "#eceff1", leg="#d4d8dc", hoof="#3a3a3e", head=(4.6, 5.4, 6.4), head_col="#f2f4f6", snout=(3.2, 3, 2, "#e4e4e4"), ears=(2.4, 1.2, 1.4, "#d4d8dc", 3),
         horns=(1.2, 4.6, 1.2, "#cfc6a6", 1.6, 0.8), tail=(1.4, 3, 1, "#f0f0f0", 0), belly="#e0e4e8") + [B(-1.2, 12, 11.5, 2.4, 3.6, 1.2, "#e6e6ea", bone="head")],
    ["MOUNTAINS", "HILLS", "SNOWY_PEAKS", "STONY_SHORE"], hp=8.0, speed=(1.2, 2.2), flee=5.2, count=7, group=(2, 4), herding=True, min_y=46)
add("capybara", "Capybara",
    quad(17, 8, 8, 4, "#7c5f40", leg="#614a30", head=(5.4, 6, 6.6), head_col="#715636", snout=(4.2, 3.6, 2.6, "#5e462c"), ears=(1.2, 1.4, 0.8, "#614a30", 2.4), belly="#8c7050", nose=(2.6, 1, "#241a12")),
    ["SWAMP", "JUNGLE", "SAVANNA", "RIVER"], hp=7.0, speed=(0.7, 1.3), flee=3.4, count=8, group=(2, 5), herding=True, pause=(4.0, 10.0))
add("bison", "Bison",
    quad(30, 13, 15, 14, "#4a3426", leg="#33261c", hoof="#1c1410", head=(8, 9, 8), head_col="#3a281c", snout=(6, 5, 3, "#2a1d14"), ears=(2.2, 1.4, 1, "#33261c", 5),
         horns=(1.4, 3.4, 1.4, "#d2c6a8", 5, 1.2), hump=(11, 6, 12, 5, "#5a4230"), mane=(6, 4, "#2a1d14"), tail=(1.4, 7, 1.2, "#33261c", 2), belly="#5c4434")
    + [B(-5, 11, 9, 10, 8, 6, "#3a281c", bone="head")],
    ["PLAINS", "SAVANNA", "HEATH", "FLOWER_MEADOW", "HILLS"], hp=16.0, speed=(0.8, 1.4), flee=4.4, count=6, group=(3, 5), herding=True)
add("tortoise", "Tortoise",
    turtle(14, 11, 5, "#7c6a3c", "#9c8a52", "#6e7c4a", legh=2.6), ["BEACH", "DESERT", "JUNGLE", "SAVANNA"], hp=10.0, speed=(0.3, 0.6), flee=1.2, count=6,
    group=(1, 2), move=(2.0, 6.0), pause=(4.0, 12.0), surfaces=["sand", "red_sand", "grass", "podzol"])
add("zebra", "Zebra",
    quad(25, 9, 11, 14, "#f2f2f2", leg="#e0e0e0", hoof="#222", head=(5.4, 6.4, 9), head_col="#ececec", neck=(5, 11, 7), snout=(4.2, 4, 2.4, "#2a2a2a"), ears=(1.8, 3, 1, "#2a2a2a", 2.4),
         mane=(1.6, 3, "#1c1c1c"), tail=(1.4, 6, 1.2, "#1c1c1c", 1), stripes=("#1c1c1c", 7), belly="#fafafa"),
    ["SAVANNA"], hp=10.0, speed=(1.2, 2.2), flee=6.2, count=9, group=(3, 6), herding=True)
add("giraffe", "Giraffe",
    quad(24, 10, 12, 30, "#dcae54", leg="#caa04c", hoof="#3a2a1a", head=(5.4, 6, 9), head_col="#d6a64c", neck=(5, 32, 6), snout=(4, 3.8, 2.4, "#4a3a2c"), ears=(2, 2.6, 1, "#caa04c", 3),
         horns=(1, 3, 1, "#8a6a3a", 1.6, 0), mane=(1, 1, "#5a3a1a"), tail=(1.2, 9, 1, "#caa04c", 1),
         spots=("#8a5a2a", [(0.1, 3, 3, 3.4), (0.7, 6, 8, 3), (0.4, 2, 14, 3.4), (0.8, 7, 18, 2.8), (0.2, 6, 20, 2.6)]), belly="#ecd48a")
    + [B(-0.5 - 2.6, 61 - 12, 4, 1.0, 20, 1.0, "#8a5a2a", bone="neck"), B(1.6, 61 - 12, 4, 1.0, 16, 1.0, "#8a5a2a", bone="neck")],
    ["SAVANNA"], hp=18.0, speed=(0.9, 1.6), flee=5.4, count=5, group=(2, 4), herding=True)
add("mouflon", "Mouflon",
    quad(17, 10, 10, 8, "#eeeae0", leg="#cfc8b8", hoof="#2a2a2a", head=(5, 6, 6), head_col="#cbbfa6", snout=(3.4, 3, 1.4, "#bfae94"), ears=(2.6, 1.2, 1.4, "#cbbfa6", 3.8), belly="#e2dccd")
    + [B(-6, 13, -9, 12, 7, 18, "#f4f1e8"), B(-3.4, 18, 7, 6.8, 2.6, 2.4, "#f4f1e8", bone="head"), B(-3, 11.6, 8.4, 6, 6.4, 6, "#c8bca4", bone="head")],
    G + ["HILLS", "HEATH", "FOREST", "MOUNTAINS"], hp=8.0, speed=(0.8, 1.4), flee=4.0, count=10, group=(3, 6), herding=True)
add("hedgehog", "Hedgehog", hedgehog(8, 4, "#5e4a38", "#b8a088", "#d8c8b0"), F + ["TAIGA", "MEGA_TAIGA", "CHERRY_GROVE"], hp=2.0, speed=(0.4, 0.9), flee=1.6,
    count=7, group=(1, 2), move=(1.0, 3.0), pause=(3.0, 8.0))
add("frog", "Marsh Frog", frog(5, 6, "#5a9a3a", "#c8dc9a", "#2e6a22"), ["SWAMP", "JUNGLE"], hp=2.0, speed=(0.8, 1.6), flee=3.2, count=10, group=(1, 3), move=(0.3, 1.2), pause=(2.0, 6.0))
add("mossback", "Mossback",
    turtle(26, 18, 10, "#5f6e4a", "#4e7a36", "#8a8a6a", legh=6, moss="#58a03a", head_len=8) + [B(-9, 6, 16, 18, 8, 3, "#8a8a6a", bone="head")],
    ["SWAMP", "DARK_FOREST", "MUSHROOM_FIELDS", "MEGA_TAIGA"], hp=20.0, speed=(0.3, 0.7), flee=1.6, count=4, group=(1, 1), move=(2.0, 6.0), pause=(6.0, 16.0))
add("red_panda", "Red Panda",
    quad(11, 5.4, 5.4, 4, "#b8502a", leg="#2e1c16", head=(5.4, 4.6, 4.6), head_col="#c25a30", snout=(2.6, 2, 1.6, "#f2e6d8"), nose=(1.4, 1, "#101010"),
         ears=(1.8, 1.8, 1.2, "#f2e6d8", 1.9), tail=(3, 3, 9, "#a8482a", 1.2), belly="#4a2a20", stripes=("#e8c8a0", 3)),
    ["CHERRY_GROVE", "JUNGLE", "BIRCH_FOREST"], hp=5.0, speed=(0.8, 1.6), flee=4.6, count=6, group=(1, 2))
add("crab", "Shore Crab", crab(7, 5, "#d4553a", "#b8442e", "#e26a4a"), ["BEACH", "STONY_SHORE", "WARM_OCEAN"], hp=3.0, speed=(0.5, 1.1), flee=3.0, count=10, group=(2, 4),
    move=(0.4, 1.4), pause=(1.5, 5.0), surfaces=["sand", "gravel", "stone"])
add("flamingo", "Flamingo",
    biped(7, 5, "#f0a0b0", "#f6c0cc", "#e8788c", (3.4, 3.4, 4), "#f4b0be", (3, 1.6, 4.5, "#2a2a2a"), legh=17, neck_h=11, tail=(3, 2, 3, "#e8788c"),
          wing=(3, 4, 6, "#e8788c"), leg_w=1.0, neck_col="#f2a8b8", foot=None),
    ["BEACH", "SWAMP", "SAVANNA", "WARM_OCEAN"], hp=4.0, speed=(0.6, 1.2), flee=4.4, count=8, group=(3, 6), herding=True)
add("penguin", "Penguin",
    biped(11, 6, "#1e2228", "#f4f6f8", "#e8a030", (4.4, 4, 4.4), "#14181c", (2.2, 1.2, 2.6, "#e8a030"), legh=2, tail=(3, 2, 2, "#14181c"), wing=(1, 7, 4, "#14181c"),
          leg_w=1.6, foot="#e8a030", L=6),
    ["SNOWY_PLAINS", "ICE_SPIKES", "FROZEN_OCEAN"], hp=5.0, speed=(0.5, 1.0), flee=3.0, count=9, group=(3, 7), herding=True, surfaces=["snow", "ice", "packed_ice", "gravel"])
add("pheasant", "Pheasant",
    biped(5, 4, "#a8642c", "#d8a060", "#7a6a5a", (3, 3, 3.4), "#1e5a4a", (1.6, 1.2, 1.8, "#d8b040"), legh=4, tail=(2, 2, 9, "#7a4c24"), wing=(2, 3, 5, "#8a4e22"),
          neck_h=2, crest=None, neck_col="#1e5a4a"),
    F + G + ["HILLS", "CHERRY_GROVE"], hp=2.0, speed=(0.8, 1.6), flee=5.0, count=9, group=(1, 3), move=(0.6, 2.4))
add("ostrich", "Ostrich",
    biped(12, 9, "#2e2a2a", "#e8e4dc", "#c8b49a", (3.6, 3.4, 4.4), "#d8b8a8", (2, 1.4, 3, "#c8a888"), legh=24, neck_h=22, tail=(5, 4, 3, "#f0ece4"), wing=(2, 8, 8, "#e8e4dc"),
          leg_w=1.6, neck_col="#d2b0a0", foot="#b89878", L=16),
    ["SAVANNA", "DESERT"], hp=7.0, speed=(1.0, 2.2), flee=6.6, count=6, group=(1, 3), herding=True)
add("sand_lizard", "Sand Lizard", lizard(4.4, 8, "#b8a05a", "#e6d8a8", "#7a6030"), ["DESERT", "BADLANDS"], hp=2.0, speed=(0.8, 1.8), flee=4.2, count=10, group=(1, 3), move=(0.3, 1.4),
    pause=(2.0, 6.0), surfaces=["sand", "red_sand", "terracotta", "sandstone"])
add("ember_newt", "Ember Newt", lizard(4, 7, "#3a2e34", "#7a3a22", "#ff8a24") + [B(-1, 3.2, 1, 2, 0.8, 2, "#ff6a18")], ["VOLCANO"], hp=3.0, speed=(0.8, 1.8), flee=3.8, count=8, group=(2, 4),
    surfaces=["basalt", "ash", "stone"])

# -- Terrestres, agressifs -----------------------------------------------------------------------
add("grey_wolf", "Grey Wolf",
    quad(16, 6, 7.4, 9, "#8a8e94", leg="#6e7278", head=(5.6, 5.6, 5.6), head_col="#92969c", snout=(3.2, 2.8, 3.4, "#aeb2b6"), nose=(1.8, 1.2, "#181818"), ears=(1.8, 2.4, 1, "#6e7278", 1.9),
         tail=(2.4, 2.6, 8, "#7a7e84", 1.5), belly="#c0c4c8", mane=(5, 2, "#7a7e84")) + [B(-1.4, 10, 10.5, 2.8, 1.2, 1.4, "#5a5e64", bone="head")],
    TAIGA + ["FOREST", "HEATH", "DARK_FOREST", "SNOWY_PLAINS"], hp=8.0, speed=(1.2, 2.2), flee=0.0, count=5, group=(2, 4), herding=True, behavior="AGGRESSIVE", aggro=14.0, chase=5.2,
    damage=2.0, reach=1.7, cooldown=1.1, knock=3.0, detect=0.0, aflee=0.0)
add("brown_bear", "Brown Bear",
    quad(26, 14, 13, 11, "#6a4228", leg="#54331e", head=(9, 8.4, 9), head_col="#6e452a", snout=(5, 4, 3.4, "#a8845e"), nose=(2.6, 1.6, "#181818"), ears=(2.6, 2.6, 1.4, "#54331e", 3.6),
         tail=(2.6, 2.6, 1.6, "#54331e", 1), belly="#7a5236", mane=(8, 3, "#5e3a22")),
    ["TAIGA", "MEGA_TAIGA", "FOREST", "DARK_FOREST", "MOUNTAINS"], hp=22.0, speed=(0.9, 1.6), flee=0.0, count=3, group=(1, 1), behavior="AGGRESSIVE", aggro=9.0, chase=4.4,
    damage=4.0, reach=2.1, cooldown=1.6, knock=6.0, detect=0.0, aflee=0.0)
add("polar_bear", "Polar Bear",
    quad(28, 15, 14, 12, "#f1f1ec", leg="#e2e2dc", head=(9, 8, 10), head_col="#f4f4ef", snout=(5, 4, 4, "#e8e8e2"), nose=(2.4, 1.6, "#181818"), ears=(2.4, 2.2, 1.2, "#e2e2dc", 3.6),
         tail=(2.2, 2.2, 1.4, "#e8e8e2", 1), belly="#fafaf6"),
    ["SNOWY_PLAINS", "ICE_SPIKES", "SNOWY_PEAKS", "FROZEN_OCEAN"], hp=26.0, speed=(0.9, 1.6), flee=0.0, count=3, group=(1, 2), behavior="AGGRESSIVE", aggro=9.0, chase=4.6,
    damage=5.0, reach=2.2, cooldown=1.6, knock=7.0, detect=0.0, aflee=0.0, surfaces=["snow", "ice", "packed_ice", "gravel"])
add("scorpion", "Dune Scorpion", arthropod(7, 10, "#8a5a28", "#6e4620", "#a46a2a", "#8e5e2c"), ["DESERT", "BADLANDS"], hp=6.0, speed=(0.8, 1.6), flee=0.0, count=6, group=(1, 3),
    behavior="AGGRESSIVE", aggro=7.0, chase=3.4, damage=1.5, reach=1.4, cooldown=1.0, knock=1.5, detect=0.0, aflee=0.0, surfaces=["sand", "red_sand", "terracotta", "sandstone"])
add("panther", "Panther",
    quad(20, 6.4, 8, 10, "#1e1c24", leg="#14121a", head=(5.6, 5.6, 6), head_col="#201e28", snout=(3, 2.6, 2.4, "#2a2832"), nose=(1.6, 1, "#a86a6a"), ears=(1.8, 1.8, 1, "#14121a", 2),
         tail=(1.6, 1.6, 12, "#14121a", 1.5), belly="#2c2a34") + [B(-2.4, 11, 10.4, 1.0, 1.0, 0.6, "#e8d040", bone="head"), B(1.4, 11, 10.4, 1.0, 1.0, 0.6, "#e8d040", bone="head")],
    ["JUNGLE"], hp=10.0, speed=(1.3, 2.4), flee=0.0, count=3, group=(1, 1), behavior="AGGRESSIVE", aggro=14.0, chase=6.2, damage=3.0, reach=1.8, cooldown=1.0, knock=3.0, detect=0.0, aflee=0.0)

# -- Aquatiques ------------------------------------------------------------------------------------
OCEANS = ["OCEAN", "DEEP_OCEAN"]
add("clownfish", "Clownfish", fish(5, 3.2, 1.6, "#f08a1c", belly="#f6a850", fin="#f08a1c", stripes=("#f8f8f8", 2)), ["WARM_OCEAN"], medium="WATER", hp=1.0, speed=(0.8, 1.6),
    flee=3.4, count=18, group=(4, 8), scatter=4, herding=True, move=(1.0, 3.0), pause=(0.3, 1.0), depth=2)
add("salmon", "Salmon", fish(10, 3.6, 2.2, "#b8b8bc", belly="#e8a898", fin="#8c8c94", tail="#8c8c94"), ["RIVER"], medium="WATER", hp=3.0, speed=(0.9, 1.8), flee=3.8, count=10,
    group=(3, 5), scatter=3, herding=True, depth=2, max_depth=6)
add("tuna", "Tuna", fish(16, 5.6, 3.4, "#2c4a7a", belly="#c8d4e0", fin="#243c66", stripes=("#c8d0d8", 1)), OCEANS, medium="WATER", hp=5.0, speed=(1.0, 2.0), flee=4.2, count=14,
    group=(3, 6), scatter=5, herding=True, depth=4)
add("pufferfish", "Pufferfish", fish(6, 6, 5, "#d8b84a", belly="#f0e8c0", fin="#c8a038", spike="#8a6a1c", head_taper=False), ["WARM_OCEAN"], medium="WATER", hp=2.0, speed=(0.5, 1.0),
    flee=2.2, count=8, group=(1, 3), herding=False, depth=2)
add("jellyfish", "Moon Jelly", jelly(6, 9, "#bcd8f4", "#e8c0f0", 0.62), OCEANS + ["WARM_OCEAN"], medium="WATER", hp=2.0, speed=(0.25, 0.6), flee=1.0, count=10, group=(2, 5), scatter=5,
    move=(2.0, 6.0), pause=(0.1, 0.6), depth=5)
add("sea_turtle", "Sea Turtle", turtle(16, 12, 4, "#5a7a3a", "#7a9a4a", "#8aa070", legh=2, flipper=True, head_len=4), ["WARM_OCEAN", "OCEAN"], medium="WATER", hp=8.0, speed=(0.5, 1.0),
    flee=1.6, count=6, group=(1, 2), move=(2.0, 6.0), pause=(0.2, 1.0), depth=4)
add("dolphin", "Dolphin", fish(26, 8, 6, "#7a8aa0", belly="#e0e6ee", fin="#66768c", snout=(2.8, 2.6, 6, "#7a8aa0")), OCEANS + ["WARM_OCEAN"], medium="WATER", hp=14.0, speed=(1.6, 3.0),
    flee=4.4, count=8, group=(2, 4), scatter=6, herding=True, depth=6, move=(1.5, 5.0), pause=(0.2, 1.0))
add("manta_ray", "Manta Ray", ray(30, 20, "#2a3646", "#e6ecf0", 14), ["WARM_OCEAN", "DEEP_OCEAN"], medium="WATER", hp=9.0, speed=(0.8, 1.6), flee=2.8, count=5, group=(1, 2),
    move=(2.0, 6.0), pause=(0.2, 1.0), depth=6)
add("ink_squid", "Ink Squid", squid(5, "#4a3a5e", "#5e4a76", 9), OCEANS, medium="WATER", hp=6.0, speed=(0.6, 1.4), flee=3.4, count=8, group=(1, 3), depth=6, move=(1.5, 4.0), pause=(0.3, 1.5))
add("narwhal", "Narwhal", fish(24, 8, 6, "#9aa6b6", belly="#e8ecf2", fin="#7e8a9c", tusk=14, stripes=None), ["FROZEN_OCEAN"], medium="WATER", hp=14.0, speed=(0.9, 1.8), flee=3.8,
    count=5, group=(2, 4), herding=True, depth=5)
add("lanternfish", "Lanternfish", fish(8, 4.4, 3, "#1c2a3a", belly="#3a5068", fin="#16222e") + [B(-0.5, 5, 3, 1, 3, 1, "#2a2a40", bone="head"), B(-1, 7.8, 3, 2, 2, 2, "#fff09a", bone="head")], ["DEEP_OCEAN"], medium="WATER",
    hp=3.0, speed=(0.5, 1.1), flee=2.4, count=9, group=(2, 5), herding=True, depth=8)
add("shark", "Shark", fish(36, 9, 7, "#6c7c8c", belly="#e8eef2", fin="#56667a", snout=(5, 3.8, 5, "#6c7c8c")), OCEANS + ["WARM_OCEAN"], medium="WATER", behavior="AGGRESSIVE", hp=20.0,
    speed=(1.2, 2.2), flee=0.0, count=3, group=(1, 1), depth=7, aggro=22.0, chase=5.6, damage=3.5, reach=3.0, cooldown=1.4, knock=4.0, detect=0.0, aflee=0.0)
add("piranha", "Piranha", fish(6, 3.6, 2, "#4e6a5a", belly="#c85a3a", fin="#3a5244", stripes=None), ["RIVER", "JUNGLE", "SWAMP"], medium="WATER", behavior="AGGRESSIVE", hp=2.0, speed=(1.2, 2.2),
    flee=0.0, count=8, group=(3, 5), scatter=3, herding=False, depth=2, aggro=7.0, chase=4.6, damage=1.0, reach=1.1, cooldown=0.8, knock=0.5, detect=0.0, aflee=0.0)


# ------------------------------------------------------------------------------------------------
def builtin_rigs():
    """Rigs des trois mobs d'origine (leur géométrie est dans `mob_geometry.json`, non générée)."""
    with open(os.path.join(ROOT, "assets/vanilla/mob_geometry.json")) as f:
        geo = json.load(f)
    with open(os.path.join(ROOT, "assets/vanilla/mobs_defaults.json")) as f:
        defaults = {m["id"]: m for m in json.load(f)["mobs"]}

    def tag(rows, spec):
        out = []
        for i, r in enumerate(rows):
            b = Box(r)
            b.bone, b.pv = spec[i]
            out.append(b)
        return out

    rigs = {}
    # Renne : 0 corps, 1 cou, 2 tête, 3 nez, 4-5 yeux, 6-9 pattes, 10-15 bois, 16 queue.
    spec = {0: ("body", None), 1: ("neck", (0, 21, 6.0)), 2: ("head", (0, 27, 7.0)), 3: ("head", None), 4: ("head", None), 5: ("head", None),
            6: ("leg_fr", (3.6, 13.2, 5.6)), 7: ("leg_fl", (-3.6, 13.2, 5.6)), 8: ("leg_br", (3.6, 13.2, -6.4)), 9: ("leg_bl", (-3.6, 13.2, -6.4)),
            16: ("tail", (0, 19.0, -11.0))}
    for i in range(10, 16):
        spec[i] = ("head", None)
    parents = {"body": "root", "neck": "body", "head": "neck", "tail": "body"}
    for n in ("leg_fl", "leg_fr", "leg_bl", "leg_br"):
        parents[n] = "root"
    boxes = Boxes(tag(geo["reindeer"], spec), "quad", parents, L=22, W=10, H=9.6, legh=13.2, neck=True, neck_pv=(21, 6.0),
                  head_c=(28.4, 10.2), head_h=5.6, tail_len=1.4)
    rigs["reindeer"] = build_rig(boxes, RECIPES["quad"], speeds(defaults["reindeer"]), TUNE["reindeer"])
    # Poisson : 0 corps, 1 ventre, 2-3 yeux, 4 queue, 5 aileron dorsal, 6-7 nageoires.
    spec = {0: ("body", None), 1: ("body", None), 2: ("head", (0, 1.2, 2.4)), 3: ("head", (0, 1.2, 2.4)), 4: ("tail", (0, 1.2, -2.8)),
            5: ("dorsal", (0, 2.0, -0.4)), 6: ("fin_l", (-0.8, 0.8, 0.1)), 7: ("fin_r", (0.8, 0.8, 0.1))}
    parents = {"body": "root", "head": "body", "tail": "body", "dorsal": "body", "fin_l": "body", "fin_r": "body"}
    boxes = Boxes(tag(geo["fish"], spec), "fish", parents, L=5.6, H=1.76, W=1.6, vertical=False)
    rigs["fish"] = build_rig(boxes, RECIPES["fish"], speeds(defaults["fish"]), TUNE.get("fish", {}))
    # Orque : 0 corps, 1 tête, 2 front, 3 arrière du corps, 4-5 nageoires de queue, 6-7 nageoires
    # pectorales, 8 aileron dorsal, 9-10 yeux, 11 ventre blanc, 12 menton, 13 bouche, 14-15 taches,
    # 16-17 flancs, 18 selle, 19 évent.
    body = ("body", None)
    head = ("head", (0, 16, 38))
    spec = {0: body, 1: head, 2: head, 3: ("tail", (0, 16, -48)), 4: ("fluke", (0, 12.8, -64)), 5: ("fluke", (0, 12.8, -64)),
            6: ("fin_l", (-13.6, 10.8, 17.6)), 7: ("fin_r", (13.6, 10.8, 17.6)), 8: ("dorsal", (0, 28.8, 8.8)), 9: head, 10: head,
            11: body, 12: head, 13: head, 14: head, 15: head, 16: body, 17: body, 18: body, 19: head}
    parents = {"body": "root", "head": "body", "tail": "body", "fluke": "tail", "fin_l": "body", "fin_r": "body", "dorsal": "body"}
    boxes = Boxes(tag(geo["orca"], spec), "fish", parents, L=130, H=21, W=27, vertical=True)
    rigs["orca"] = build_rig(boxes, RECIPES["fish"], speeds(defaults["orca"]), TUNE["orca"])
    return rigs


def write():
    cfgs = [c for c, _ in MOBS]
    geo = {c["id"]: b for c, b in MOBS}
    with open(os.path.join(ROOT, "assets/vanilla/mobs_nb.json"), "w") as f:
        json.dump({"mobs": cfgs}, f, indent=2)
        f.write("\n")
    with open(os.path.join(ROOT, "assets/vanilla/mob_geometry_nb.json"), "w") as f:
        f.write("{\n")
        items = list(geo.items())
        for i, (k, boxes) in enumerate(items):
            f.write('  "%s": [\n' % k)
            f.write(",\n".join("    " + json.dumps(b) for b in boxes))
            f.write("\n  ]%s\n" % ("," if i < len(items) - 1 else ""))
        f.write("}\n")
    rigs = builtin_rigs()
    rigs.update(RIGS)
    with open(os.path.join(ROOT, "assets/vanilla/mob_rigs.json"), "w") as f:
        f.write("{\n")
        items = list(rigs.items())
        for i, (k, rig) in enumerate(items):
            f.write('  "%s": %s%s\n' % (k, json.dumps(rig, separators=(",", ":")), "," if i < len(items) - 1 else ""))
        f.write("}\n")
    print(len(cfgs), "mobs,", sum(len(b) for b in geo.values()), "boîtes", file=sys.stderr)


def preview(path):
    from PIL import Image, ImageDraw

    cols = 6
    cw, ch = 230, 200
    rows = (len(MOBS) + cols - 1) // cols
    img = Image.new("RGB", (cols * cw, rows * ch), (196, 218, 236))
    d = ImageDraw.Draw(img)
    for i, (c, boxes) in enumerate(MOBS):
        ox = (i % cols) * cw + cw // 2
        oy = (i // cols) * ch + ch - 28
        ymax = max(b[1] + b[4] for b in boxes)
        zext = max(abs(b[2]) + b[5] for b in boxes)
        s = min(5.0, 120.0 / max(ymax + zext * 0.5, 1))
        s = min(s, 3.2 * 38 / max(ymax, 20)) if ymax > 38 else s
        s = min(s, 150.0 / (max(b[0] + b[3] for b in boxes) - min(b[0] for b in boxes) + 2 * zext))

        def P(x, y, z):
            # vue 3/4 : x vers la droite-bas, z vers la gauche-bas (museau vers l'avant-gauche)
            return (ox + (x * 0.87 - z * 0.87) * s * 0.9, oy + (x * 0.5 + z * 0.5) * s * 0.9 - y * s)

        order = sorted(boxes, key=lambda b: (-(b[0] + b[3] / 2) * 0 - (b[2] + b[5] / 2) * 1 + (b[0] + b[3] / 2), b[1]))
        order = sorted(boxes, key=lambda b: ((b[0] + b[3] / 2) + (b[2] + b[5] / 2), b[1]))
        # Du plus loin (x+z petit) au plus proche.
        for b in order:
            x, y, z, w, h, dd = b[:6]
            col = tuple(int(v * 255) for v in b[6:9])
            top = tuple(min(255, int(v * 1.1)) for v in col)
            left = tuple(int(v * 0.78) for v in col)
            right = tuple(int(v * 0.6) for v in col)
            d.polygon([P(x, y + h, z), P(x + w, y + h, z), P(x + w, y + h, z + dd), P(x, y + h, z + dd)], fill=top)
            d.polygon([P(x, y, z + dd), P(x + w, y, z + dd), P(x + w, y + h, z + dd), P(x, y + h, z + dd)], fill=left)
            d.polygon([P(x + w, y, z), P(x + w, y, z + dd), P(x + w, y + h, z + dd), P(x + w, y + h, z)], fill=right)
        d.text((i % cols * cw + 6, i // cols * ch + 4), c["id"], fill=(20, 20, 30))
    img.save(path)


if __name__ == "__main__":
    write()
    if "--preview" in sys.argv:
        preview(sys.argv[sys.argv.index("--preview") + 1] if len(sys.argv) > sys.argv.index("--preview") + 1 else "/tmp/mobs_preview.png")
