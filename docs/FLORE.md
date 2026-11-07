# Flore et blocs de la phase 9

## Blocs ajoutés (identifiants 68 à 118)

51 blocs : matériaux de construction des structures, plantes, feuillages et bois des nouveaux
arbres (`assets/blocks.json`, `crates/nb-data/src/blocks.rs`).

* **Construction** : 6 planches (`planks_oak/spruce/birch/acacia/dark_oak/cherry`), `cobblestone`,
  `mossy_cobblestone`, `stone_bricks`, `mossy_stone_bricks`, `cut_sandstone`, `chiseled_sandstone`,
  `bricks`, `hay_block`, `bookshelf`, `glass`, `basalt_bricks`, `prismarine_bricks`, `obsidian`,
  `gold_block`, `bone_block`, 4 laines (`wool_white/red/blue/yellow`), `pumpkin`, `dirt_path`,
  `farmland`.
* **Lumière** : `lantern` (15), `sea_lantern` (15), `campfire` (14), `torch` (14, en croix),
  `ember_bloom` (9, en croix).
* **Bois et feuillages** : `baobab_wood`, `redwood_wood`, `apple_leaves` (pommes rouges, non
  teintées), `willow_leaves` (teintées).
* **Plantes en croix** : `torch`, `cobweb`, `vines`, `wheat`, `allium`, `blue_orchid`,
  `sunflower`, `lily_of_the_valley`, `cotton_grass`, `pampas_grass`, `hibiscus`, `ember_bloom`,
  `bamboo`, `berry_bush`, `cattail`, `desert_sage`.

Les propriétés (transparence, croix, teinte, émission) sont dans `blocks.json` ; le mailleur et la
lumière n'ont besoin d'aucune règle spéciale.

## Nouveaux arbres (`TreeType`, `flora.rs`)

| Arbre | Biomes | Forme |
| --- | --- | --- |
| Pommier | plaines, prairie, forêt, forêt d'automne | tronc court, couronne ronde à pommes |
| Saule pleureur | marais | tronc penché, couronne large, rideau de feuilles retombantes |
| Baobab | savane | tronc renflé 5 × 5 puis 3 × 3, 5 à 7 branches en candélabre |
| Arbre-parasol | savane, badlands | tronc courbe, trois branches à plateau de feuilles |
| Séquoia | méga-taïga, taïga | tronc 2 × 2 à empattement, 24 à 34 blocs, cime étroite |
| Saguaro | désert, badlands | cactus à un ou deux bras |
| Bambouseraie | jungle, forêt de cerisiers | 7 à 13 tiges de 5 à 11 blocs |
| Genévrier | lande, taïga, collines | petit conifère trapu |

Le désert et les badlands ont désormais des arbres (densité propre à Norrblock,
`BiomeConfig::nb_tree_density`, qui n'affecte pas le générateur Java).

## Plantes par biome (`BiomeConfig::flora`)

Une passe dédiée (`nb_flora`) s'exécute **avant** la végétation d'origine (elle a donc la priorité
sur les herbes hautes). Chaque entrée précise le sol accepté (herbe, sable, neige, cendre), la
densité, si la plante pousse par plaques (champs, parterres), au bord de l'eau, ou en tiges de
plusieurs blocs.

| Plante | Où |
| --- | --- |
| Tournesol | champs en plaques (plaines, prairie fleurie), rare en savane |
| Ail d'ornement (allium) | plaines, forêts, lande, prairie fleurie, forêt de cerisiers |
| Muguet | bouleaux, forêts, taïga (par plaques) |
| Buisson de baies | forêts, taïga, lande, collines, forêt d'automne |
| Citrouilles sauvages | forêt d'automne (par plaques) |
| Herbe de la pampa | savane (par plaques, dense) |
| Linaigrette (cotton grass) | plaines et taïgas enneigées, lande |
| Bambou | jungle (par plaques, 3 à 8 blocs), cerisiers |
| Hibiscus, orchidée bleue | jungle, marais, prairie fleurie |
| Massettes (cattail) | bords d'eau : marais (dense), jungle, plaines, forêts, plages |
| Sauge du désert | désert, badlands |
| Fleur de braise | volcan (sur cendre et basalte) |

Le curseur *Vegetation Density* agit sur ces plantes comme sur les autres.

## Textures provisoires

Toutes les textures de ces blocs sont **procédurales** (`scripts/gen_textures_nature.py`, 32 × 32,
graine fixe) et portent les noms des textures de Minecraft / Faithful 32x quand il en existe. Un
fichier existant n'est jamais écrasé (sauf `--force`) : les vraies textures de Gabriel passeront
donc devant sans rien changer d'autre. Liste des fichiers à fournir (dossier
`resource_packs/Default/assets/minecraft/textures/block/`) :

| Bloc | Fichier |
| --- | --- |
| `planks_oak` | `oak_planks.png` |
| `planks_spruce` | `spruce_planks.png` |
| `planks_birch` | `birch_planks.png` |
| `planks_acacia` | `acacia_planks.png` |
| `planks_dark_oak` | `dark_oak_planks.png` |
| `planks_cherry` | `cherry_planks.png` |
| `cobblestone` | `cobblestone.png` |
| `mossy_cobblestone` | `mossy_cobblestone.png` |
| `stone_bricks` | `stone_bricks.png` |
| `mossy_stone_bricks` | `mossy_stone_bricks.png` |
| `cut_sandstone` | `cut_sandstone.png` |
| `chiseled_sandstone` | `chiseled_sandstone.png` |
| `chiseled_sandstone` | `cut_sandstone.png` |
| `bricks` | `bricks.png` |
| `hay_block` | `hay_block_side.png` |
| `hay_block` | `hay_block_top.png` |
| `bookshelf` | `bookshelf.png` |
| `bookshelf` | `oak_planks.png` |
| `glass` | `glass.png` |
| `lantern` | `glowstone.png` |
| `sea_lantern` | `sea_lantern.png` |
| `wool_white` | `white_wool.png` |
| `wool_red` | `red_wool.png` |
| `wool_blue` | `blue_wool.png` |
| `wool_yellow` | `yellow_wool.png` |
| `bone_block` | `bone_block_side.png` |
| `bone_block` | `bone_block_top.png` |
| `obsidian` | `obsidian.png` |
| `basalt_bricks` | `polished_blackstone_bricks.png` |
| `pumpkin` | `pumpkin_side.png` |
| `pumpkin` | `pumpkin_top.png` |
| `gold_block` | `gold_block.png` |
| `campfire` | `campfire_log_lit.png` |
| `dirt_path` | `dirt_path_side.png` |
| `dirt_path` | `dirt_path_top.png` |
| `dirt_path` | `dirt.png` |
| `farmland` | `dirt.png` |
| `farmland` | `farmland_moist.png` |
| `prismarine_bricks` | `prismarine_bricks.png` |
| `baobab_wood` | `stripped_acacia_log.png` |
| `baobab_wood` | `stripped_acacia_log_top.png` |
| `redwood_wood` | `mangrove_log.png` |
| `redwood_wood` | `mangrove_log_top.png` |
| `apple_leaves` | `apple_leaves.png` |
| `willow_leaves` | `mangrove_leaves.png` |
| `torch` | `torch.png` |
| `cobweb` | `cobweb.png` |
| `vines` | `vine.png` |
| `wheat` | `wheat_stage7.png` |
| `allium` | `allium.png` |
| `blue_orchid` | `blue_orchid.png` |
| `sunflower` | `sunflower_front.png` |
| `lily_of_the_valley` | `lily_of_the_valley.png` |
| `cotton_grass` | `cotton_grass.png` |
| `pampas_grass` | `pampas_grass.png` |
| `hibiscus` | `hibiscus.png` |
| `ember_bloom` | `ember_bloom.png` |
| `bamboo` | `bamboo_stalk.png` |
| `berry_bush` | `sweet_berry_bush_stage3.png` |
| `cattail` | `cattail.png` |
| `desert_sage` | `desert_sage.png` |

Les mobs n'ont pas de texture (boîtes colorées, voir `docs/FAUNE.md`).
