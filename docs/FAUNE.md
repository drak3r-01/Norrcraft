# Faune

Depuis la phase 9, le jeu compte **45 mobs** : les trois d'origine (renne, poisson, orque, dont
l'apparition reste celle du Java) et **42 animaux** de Norrblock. Ils sont décrits par deux
fichiers générés par `scripts/gen_mobs.py` :

* `assets/vanilla/mobs_nb.json` : réglages (même format que `mobs_defaults.json` + conditions d'apparition) ;
* `assets/vanilla/mob_geometry_nb.json` : géométrie en boîtes colorées (pixels MC, 16 px = 1 bloc, museau
  vers +Z), comme le renne d'origine.

Les animaux n'ont pas de texture : ce sont des **placeholders en boîtes colorées** (625 boîtes au
total). Quand Gabriel fournira de vraies textures ou des modèles `.jem`, l'ordre de résolution
existant (JEM → forme vanilla → géométrie → boîte texturée) les fera passer devant. Les
identifiants évitent ceux des formes vanilla de `nb-gfx` (`sheep`, `wolf`, `rabbit`, `squid`… qui
donneraient un damier) : d'où `mouflon`, `grey_wolf`, `hare`, `ink_squid` ; un test vérifie qu'il
n'y a aucune collision.

Chaque animal (et les 3 mobs d'origine) est **animé** : squelette, marche, course, repos, variantes
d'idle, regard, réactions aux dégâts, attaque, nage… générés avec la géométrie
(`assets/vanilla/mob_rigs.json`). Voir [`ANIMATION.md`](ANIMATION.md).

Pour régénérer (ou aperçu PNG de tous les animaux) :

```sh
python3 scripts/gen_mobs.py                       # réécrit les deux JSON
python3 scripts/gen_mobs.py --preview apercu.png  # + planche d'aperçu
```

La taille de collision (`widthX`, `widthZ`, `height`) est calculée à partir de la géométrie.

## Conditions d'apparition

Les champs ci-dessous (dans `MobConfig`, éditables dans `saves/mobs.json`) s'ajoutent à ceux du
Java. Tous sont facultatifs : **vides ou à 0, ils ne changent rien** (c'est le cas des trois mobs
d'origine, dont le comportement est inchangé).

| Champ | Effet |
| --- | --- |
| `spawnBiomes` | noms de biomes (`SNOWY_PLAINS`, `DESERT`… = `Biome::name()`) ; ignoré si le monde n'a pas de biomes |
| `spawnSurfaces` | noms de blocs : le sol sous les pieds (terrestre) ou le fond de la colonne d'eau (aquatique) |
| `spawnMinY` / `spawnMaxY` | altitude du point d'apparition (`spawnMaxY = 0` : pas de plafond) |
| `spawnMaxWaterDepth` | profondeur d'eau maximale (rivières) ; le minimum reste `spawnMinWaterDepth` |

Un mob soumis à au moins une condition n'apparaît **jamais sur un feuillage ni un cactus** (le
Java faisait apparaître les mobs sur les canopées) et tente 40 origines par seconde au lieu de
10–15, car la plupart des points tirés ne sont pas dans son habitat. Le test
`every_animal_has_a_viable_habitat_in_generated_worlds` génère de vrais mondes (3 graines) et
vérifie que chaque animal a des milliers de points d'apparition valides.

## Écologie

L'IA est celle du Java : les passifs fuient les hostiles du même milieu à moins de 10 blocs et le
joueur à son approche ; un hostile poursuit le joueur dans son `aggroPlayerRange`, sinon la proie
passive la plus proche de son milieu (le loup chasse les moutons, le requin les poissons). Les
animaux d'un même troupeau (`herding`) restent groupés.

## Terrestres

| id | nom | milieu | comportement | PV | plafond (groupe) | biomes | conditions |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `wild_boar` | Wild Boar | terre | pacifique | 7 | 9 (2–4) | forêt, bouleaux, forêt sombre, forêt d'automne, méga-taïga, taïga | troupeau |
| `elk` | Elk | terre | pacifique | 12 | 7 (1–3) | taïga, taïga enneigée, méga-taïga, lande, montagnes, collines | troupeau |
| `arctic_fox` | Arctic Fox | terre | pacifique | 4 | 7 (1–2) | plaine enneigée, taïga enneigée, pics de glace, sommets enneigés | sol : snow, packed_ice, ice, stone |
| `hare` | Hare | terre | pacifique | 3 | 12 (1–3) | plaine, prairie fleurie, forêt, bouleaux, forêt sombre, forêt d'automne, collines, lande, cerisiers, savane |  |
| `camel` | Camel | terre | pacifique | 14 | 5 (1–3) | désert, badlands | sol : sand, red_sand, terracotta, sandstone; troupeau |
| `fennec_fox` | Fennec Fox | terre | pacifique | 3 | 7 (1–2) | désert, badlands | sol : sand, red_sand, terracotta |
| `mountain_goat` | Mountain Goat | terre | pacifique | 8 | 7 (2–4) | montagnes, collines, sommets enneigés, rivage rocheux | y ≥ 46; troupeau |
| `capybara` | Capybara | terre | pacifique | 7 | 8 (2–5) | marais, jungle, savane, rivière | troupeau |
| `bison` | Bison | terre | pacifique | 16 | 6 (3–5) | plaine, savane, lande, prairie fleurie, collines | troupeau |
| `tortoise` | Tortoise | terre | pacifique | 10 | 6 (1–2) | plage, désert, jungle, savane | sol : sand, red_sand, grass, podzol |
| `zebra` | Zebra | terre | pacifique | 10 | 9 (3–6) | savane | troupeau |
| `giraffe` | Giraffe | terre | pacifique | 18 | 5 (2–4) | savane | troupeau |
| `mouflon` | Mouflon | terre | pacifique | 8 | 10 (3–6) | plaine, prairie fleurie, collines, lande, forêt, montagnes | troupeau |
| `hedgehog` | Hedgehog | terre | pacifique | 2 | 7 (1–2) | forêt, bouleaux, forêt sombre, forêt d'automne, taïga, méga-taïga, cerisiers |  |
| `frog` | Marsh Frog | terre | pacifique | 2 | 10 (1–3) | marais, jungle |  |
| `mossback` | Mossback | terre | pacifique | 20 | 4 (1–1) | marais, forêt sombre, champignons, méga-taïga |  |
| `red_panda` | Red Panda | terre | pacifique | 5 | 6 (1–2) | cerisiers, jungle, bouleaux |  |
| `crab` | Shore Crab | terre | pacifique | 3 | 10 (2–4) | plage, rivage rocheux, océan chaud | sol : sand, gravel, stone |
| `flamingo` | Flamingo | terre | pacifique | 4 | 8 (3–6) | plage, marais, savane, océan chaud | troupeau |
| `penguin` | Penguin | terre | pacifique | 5 | 9 (3–7) | plaine enneigée, pics de glace, océan gelé | sol : snow, ice, packed_ice, gravel; troupeau |
| `pheasant` | Pheasant | terre | pacifique | 2 | 9 (1–3) | forêt, bouleaux, forêt sombre, forêt d'automne, plaine, prairie fleurie, collines, cerisiers |  |
| `ostrich` | Ostrich | terre | pacifique | 7 | 6 (1–3) | savane, désert | troupeau |
| `sand_lizard` | Sand Lizard | terre | pacifique | 2 | 10 (1–3) | désert, badlands | sol : sand, red_sand, terracotta, sandstone |
| `ember_newt` | Ember Newt | terre | pacifique | 3 | 8 (2–4) | volcan | sol : basalt, ash, stone |
| `grey_wolf` | Grey Wolf | terre | hostile | 8 | 5 (2–4) | taïga, taïga enneigée, méga-taïga, forêt, lande, forêt sombre, plaine enneigée | troupeau; agro 14 blocs, 2 dégâts |
| `brown_bear` | Brown Bear | terre | hostile | 22 | 3 (1–1) | taïga, méga-taïga, forêt, forêt sombre, montagnes | agro 9 blocs, 4 dégâts |
| `polar_bear` | Polar Bear | terre | hostile | 26 | 3 (1–2) | plaine enneigée, pics de glace, sommets enneigés, océan gelé | sol : snow, ice, packed_ice, gravel; agro 9 blocs, 5 dégâts |
| `scorpion` | Dune Scorpion | terre | hostile | 6 | 6 (1–3) | désert, badlands | sol : sand, red_sand, terracotta, sandstone; agro 7 blocs, 1.5 dégâts |
| `panther` | Panther | terre | hostile | 10 | 3 (1–1) | jungle | agro 14 blocs, 3 dégâts |

## Aquatiques

| id | nom | milieu | comportement | PV | plafond (groupe) | biomes | conditions |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `clownfish` | Clownfish | eau | pacifique | 1 | 18 (4–8) | océan chaud | eau ≥ 2; troupeau |
| `salmon` | Salmon | eau | pacifique | 3 | 10 (3–5) | rivière | eau ≤ 6; eau ≥ 2; troupeau |
| `tuna` | Tuna | eau | pacifique | 5 | 14 (3–6) | océan, océan profond | eau ≥ 4; troupeau |
| `pufferfish` | Pufferfish | eau | pacifique | 2 | 8 (1–3) | océan chaud | eau ≥ 2 |
| `jellyfish` | Moon Jelly | eau | pacifique | 2 | 10 (2–5) | océan, océan profond, océan chaud | eau ≥ 5 |
| `sea_turtle` | Sea Turtle | eau | pacifique | 8 | 6 (1–2) | océan chaud, océan | eau ≥ 4 |
| `dolphin` | Dolphin | eau | pacifique | 14 | 8 (2–4) | océan, océan profond, océan chaud | eau ≥ 6; troupeau |
| `manta_ray` | Manta Ray | eau | pacifique | 9 | 5 (1–2) | océan chaud, océan profond | eau ≥ 6 |
| `ink_squid` | Ink Squid | eau | pacifique | 6 | 8 (1–3) | océan, océan profond | eau ≥ 6 |
| `narwhal` | Narwhal | eau | pacifique | 14 | 5 (2–4) | océan gelé | eau ≥ 5; troupeau |
| `lanternfish` | Lanternfish | eau | pacifique | 3 | 9 (2–5) | océan profond | eau ≥ 8; troupeau |
| `shark` | Shark | eau | hostile | 20 | 3 (1–1) | océan, océan profond, océan chaud | eau ≥ 7; agro 22 blocs, 3.5 dégâts |
| `piranha` | Piranha | eau | hostile | 2 | 8 (3–5) | rivière, jungle, marais | eau ≥ 2; agro 7 blocs, 1 dégâts |

## Pas (encore) d'animaux volants

Le médium `AIR` n'est pas géré par le Java d'origine (aucune apparition, pas de physique de vol) ;
les oiseaux de la liste (flamant, pingouin, faisan, autruche) sont des marcheurs. Des oiseaux qui
volent demanderaient une physique et une IA dédiées.
