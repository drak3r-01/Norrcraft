# Inventaire, craft et blocs exclusifs

Système ajouté en phase 10 (aucun équivalent dans la version Java : le jeu d'origine n'avait ni
objets ni recettes). Tout est piloté par des fichiers de données ; ajouter une recette ou un
objet ne demande pas de toucher au code.

## Pour jouer

| Geste | Effet |
|---|---|
| **E** | Inventaire : 36 cases + grille de craft 2×2 et son résultat |
| **Clic droit** sur un `workbench` | Ouvre l'établi (grille 3×3) |
| **Clic droit** sur une `forge` | Ouvre la fonderie (matière + combustible) |
| **Accroupi** + clic droit sur une station | Pose un bloc contre elle au lieu de l'ouvrir |
| Clic gauche sur une case | Prendre / poser / fusionner / échanger la pile |
| Clic droit sur une case | Prendre la moitié d'une pile, ou poser un exemplaire |
| **Maj** + clic sur une case de l'inventaire | Barre rapide ↔ grille principale |
| **Maj** + clic sur une case de la grille | La renvoie dans l'inventaire |
| Clic sur le **résultat** | Prend le craft (consomme un exemplaire de chaque ingrédient) |
| **Maj** + clic sur le résultat | Craft en série dans l'inventaire, tant que tout rentre |
| Survol d'une case | Infobulle avec le nom de l'objet |
| E ou Échap | Ferme l'écran : pile tenue et grille retournent dans l'inventaire |

Comme avant, casser un bloc le met dans l'inventaire (aucune dureté, bogue B19 conservé). Les
objets purs (bâton, lingot…) ne se posent pas. Si l'inventaire est plein à la fermeture, le
reliquat est perdu (comportement `returnHeld` du Java, bogue B20 conservé).

## Objets

Un **objet** est soit un bloc (identifiant d'objet = identifiant de bloc, `0..=255`), soit un
**objet pur** défini dans `assets/items.json` (identifiant `>= 256`). Les sauvegardes stockent
les objets **par nom** : les anciens mondes se relisent tels quels (un nom inconnu est ignoré).
Les piles font 64 au plus.

| Objet pur | Rôle |
|---|---|
| `stick` | Bâton : torches, combustible |
| `coal` | Charbon : torches, combustible, blocs de charbon |
| `iron_ingot`, `gold_ingot` | Lingots (fonderie) : blocs de métal, lanterne |
| `diamond`, `emerald` | Gemmes (fonderie) : blocs de gemme |
| `brick` | Brique (fonderie, depuis l'argile) : 2×2 → `bricks` |
| `aurora_shard` | **Éclat d'aurore** (exclusif Norrblock) : `aurora_block`, `lantern`, `sea_lantern` |

## Blocs exclusifs (ids 200 à 208)

Dans `assets/blocks_craft.json`, chargé après `blocks.json` (`BlockRegistry::builtin_with_craft`).
Les identifiants **200 et plus sont réservés au craft** ; la génération de terrain et les
autres fils utilisent `0..=199`.

| Bloc | Particularité |
|---|---|
| `workbench` | Établi 3×3 |
| `forge` | Fonderie, légère lueur (émission 5) |
| `marble_bricks`, `slate_tiles` | Briques de marbre / tuiles d'ardoise (comme `stone_bricks`, `basalt_bricks`) |
| `coal_block`, `iron_block`, `diamond_block`, `emerald_block` | Blocs de stockage (9 objets → 1 bloc, 1 bloc → 9 objets à la main) |
| `aurora_block` | **Exclusif** : émission de lumière 15, textures violet-cyan |

Les blocs de craft déjà ajoutés par la phase 9 (`planks_*`, `stone_bricks`, `glass`, `lantern`,
`torch`, `bricks`, `bookshelf`, `sea_lantern`, `campfire`, `gold_block`…) sont réutilisés tels
quels : seules leurs recettes sont nouvelles.

## Recettes (46)

Motifs : une lettre par case (`·` = vide). Un motif se translate librement dans la grille et
peut être retourné gauche/droite. Les recettes de la main marchent aussi à l'établi. Les
étiquettes (`#nom`) regroupent plusieurs objets :

- `#logs` : `wood`, `birch_wood`, `spruce_wood`, `jungle_wood`, `acacia_wood`, `cherry_wood`, `dark_oak_wood`
- `#planks` : `planks_oak`, `planks_birch`, `planks_spruce`, `planks_acacia`, `planks_cherry`, `planks_dark_oak`
- `#rock` : `stone`, `cobblestone`, `basalt`, `slate`, `marble`
- `#fuel` : `coal`, `planks_oak`, `planks_birch`, `planks_spruce`, `planks_acacia`, `planks_cherry`, `planks_dark_oak`, `wood`, `birch_wood`, `spruce_wood`, `jungle_wood`, `acacia_wood`, `cherry_wood`, `dark_oak_wood`, `stick`

### Main (grille 2×2 de l'inventaire, touche E)

| Résultat | Recette |
|---|---|
| 4 × `planks_oak` | sans forme : `wood` |
| 4 × `planks_birch` | sans forme : `birch_wood` |
| 4 × `planks_spruce` | sans forme : `spruce_wood` |
| 4 × `planks_acacia` | sans forme : `acacia_wood` |
| 4 × `planks_cherry` | sans forme : `cherry_wood` |
| 4 × `planks_dark_oak` | sans forme : `dark_oak_wood` |
| 4 × `planks_oak` | sans forme : `jungle_wood` |
| 4 × `stick` | `P`<br>`P`<br>`P`=#planks |
| `workbench` | `PP`<br>`PP`<br>`P`=#planks |
| 4 × `torch` | `C`<br>`S`<br>`C`=coal, `S`=stick |
| `sandstone` | `SS`<br>`SS`<br>`S`=sand |
| `aurora_block` | `AA`<br>`AA`<br>`A`=aurora_shard |
| 4 × `aurora_shard` | sans forme : `aurora_block` |
| `bricks` | `BB`<br>`BB`<br>`B`=brick |
| 9 × `coal` | sans forme : `coal_block` |
| 9 × `iron_ingot` | sans forme : `iron_block` |
| 9 × `gold_ingot` | sans forme : `gold_block` |
| 9 × `diamond` | sans forme : `diamond_block` |
| 9 × `emerald` | sans forme : `emerald_block` |

### Établi (grille 3×3, clic droit sur le bloc `workbench`)

| Résultat | Recette |
|---|---|
| `coal_block` | `III`<br>`III`<br>`III`<br>`I`=coal |
| `iron_block` | `III`<br>`III`<br>`III`<br>`I`=iron_ingot |
| `gold_block` | `III`<br>`III`<br>`III`<br>`I`=gold_ingot |
| `diamond_block` | `III`<br>`III`<br>`III`<br>`I`=diamond |
| `emerald_block` | `III`<br>`III`<br>`III`<br>`I`=emerald |
| `forge` | `SSS`<br>`SCS`<br>`SSS`<br>`S`=#rock, `C`=coal |
| 4 × `stone_bricks` | `SS`<br>`SS`<br>`S`=stone |
| 4 × `marble_bricks` | `SS`<br>`SS`<br>`S`=marble |
| 4 × `basalt_bricks` | `SS`<br>`SS`<br>`S`=basalt |
| 4 × `slate_tiles` | `SS`<br>`SS`<br>`S`=slate |
| 4 × `cut_sandstone` | `SS`<br>`SS`<br>`S`=sandstone |
| `mossy_stone_bricks` | sans forme : `stone_bricks` + `moss` |
| `mossy_cobblestone` | sans forme : `cobblestone` + `moss` |
| `lantern` | `·I·`<br>`IAI`<br>`·I·`<br>`I`=iron_ingot, `A`=aurora_shard |
| `sea_lantern` | `AG`<br>`GA`<br>`A`=aurora_shard, `G`=glass |
| `campfire` | `·S·`<br>`SCS`<br>`LLL`<br>`S`=stick, `C`=coal, `L`=#logs |
| `bookshelf` | `PPP`<br>`WWW`<br>`PPP`<br>`P`=#planks, `W`=wool_white |

### Fonderie (matière + combustible, clic droit sur le bloc `forge`)

| Résultat | Recette |
|---|---|
| `iron_ingot` | `IF`<br>`I`=iron_ore, `F`=#fuel |
| `gold_ingot` | `IF`<br>`I`=gold_ore, `F`=#fuel |
| `diamond` | `IF`<br>`I`=diamond_ore, `F`=#fuel |
| `emerald` | `IF`<br>`I`=emerald_ore, `F`=#fuel |
| 2 × `coal` | `IF`<br>`I`=coal_ore, `F`=#fuel |
| `glass` | `IF`<br>`I`=sand, `F`=#fuel |
| `glass` | `IF`<br>`I`=red_sand, `F`=#fuel |
| 2 × `brick` | `IF`<br>`I`=clay, `F`=#fuel |
| `stone` | `IF`<br>`I`=cobblestone, `F`=#fuel |
| 2 × `aurora_shard` | `IG`<br>`I`=packed_ice, `G`=gold_ingot |

Recettes originales de Norrblock : `aurora_shard` (glace compacte + lingot d'or en
fonderie), `aurora_block`, `lantern` (lingots + éclat), `sea_lantern` (éclats + verre),
`campfire`, `forge` (anneau de roche + charbon), `coal_ore` → 2 charbons.

## Ajouter une recette ou un objet

1. **Recette** : ajouter une entrée à `assets/recipes.json` (`station` = `hand`, `workbench` ou
   `forge`) avec `pattern` + `key` (motif) ou `shapeless` (liste), et `result {item, count}`.
   Le test `every_recipe_output_is_reachable_by_its_own_pattern` vérifie qu'aucune recette n'est
   masquée par une précédente (la première recette qui correspond gagne).
2. **Objet pur** : ajouter `{"id": 264, "name": "...", "texture": "item_..."}` à
   `assets/items.json` (identifiant `>= 256`, unique) et fournir `item_….png` dans le pack.
3. **Bloc** : ajouter l'entrée à `assets/blocks_craft.json` (identifiant libre à partir de 209).

Toute erreur de données (objet inconnu, motif irrégulier, recette qui ne tient pas dans la
grille de sa station…) fait échouer le chargement avec un message précis.

## Textures

Provisoires, générées par `scripts/gen_craft_textures.py` (blocs, objets) et
`scripts/gen_craft_hud.py` (fonds d'écran `workbench.png`, `forge.png` dérivés de
`inventory.png`). L'icône d'un objet pur est une **couche de l'atlas des blocs** : son fichier
`item_<nom>.png` se place dans `textures/block/` du pack, à côté des blocs.

À fournir pour le pack final (le reste porte déjà le nom Faithful 32x, qui écrasera les
placeholders) :

- inexistantes dans Faithful : `marble_bricks`, `aurora_block`, `item_stick`, `item_coal`,
  `item_iron_ingot`, `item_gold_ingot`, `item_diamond`, `item_emerald`, `item_brick`,
  `item_aurora_shard` (les `item_*` correspondent aux fichiers `textures/item/*.png` de
  Faithful : stick, coal, iron_ingot… à copier en les renommant) ;
- portent le nom Faithful : `crafting_table_top/side/front`, `blast_furnace_top/side/front`,
  `deepslate_tiles`, `coal_block`, `iron_block`, `diamond_block`, `emerald_block`.

Les fonds d'écran de l'établi et de la fonderie doivent garder la disposition de
`inventory.png` (cases à 4 px par unité de 176×166) : voir `cell_origin` / `output_origin`
dans `crates/nb-ui/src/inventory.rs`.

## Architecture

| Crate | Contenu |
|---|---|
| `nb-data` | `ItemRegistry` (blocs + `items.json`), `RecipeBook` (chargement, validation, `find`), `Station` |
| `nb-ecs` | `ItemStack` par objet, `Inventory::add_stack/room_for`, `CraftingGrid`, `InventoryCursor::click/close`, événement `EcsEvent::StationUsed` (clic droit sur une station) |
| `nb-ui` | `InventoryScreen::for_station` : fonds, cases de la grille, résultat, infobulle ; clic droit et Maj dans `UiInput` |
| `nb-game` | `Registries.items/recipes`, grille de la station courante, `UiCommand::SlotClick`, `Game::open_station`, `ItemIcons` |

Numérotation des cases d'un écran : `0..36` inventaire, `36 + k` case `k` de la grille (ligne
par ligne), `45` résultat. La logique est pure (sans monde ni GPU) ; les tests couvrent les
règles de pile, le craft en série, la fermeture sans perte, les écrans à la souris et la
sauvegarde des objets purs.
