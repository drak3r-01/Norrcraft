# 01. Architecture globale et séparation Vanilla / Norrcraft

Tâche 1 du GDD. Ce document définit comment le jeu héberge deux univers de contenu étanches,
**vanilla** (tout ce qui existe aujourd'hui) et **norrcraft** (le nouveau monde par défaut), sur un
seul moteur. Il ne contient aucun code final : les signatures données sont des contrats à
implémenter.

## 1. État des lieux (code lu le 10/11, `main` = 99bf3e9)

| Sujet | Situation actuelle | Conséquence pour la séparation |
| --- | --- | --- |
| Choix du générateur | `GeneratorKind { Classic, Norrblock }` dans `nb-world/src/config.rs`, testé par des `if` dans `generator.rs` et `biome.rs` | Un troisième `if` ne tient pas : il faut un point d'extension |
| Type de monde | `WorldType { Normal, Flat, Amplified, Island, Archipelago, Highlands, Classic }` = préréglages de relief, non sauvegardés | À renommer en « préréglage de relief », propre à vanilla |
| Biomes | `enum Biome` à 30 variantes (`biome.rs`), `BIOME_COUNT` en dur, 8 familles pour les curseurs | L'enum doit devenir privé à vanilla ; le moteur ne doit voir qu'un identifiant et une table |
| Lien biome ↔ reste du jeu | Par **nom** (`Biome::name()`) dans `mobs_nb.json` (`spawnBiomes`) et `survival.json` (`biomes`) | Excellent : le couplage est déjà en données, pas en types |
| Blocs | `BlockId = u8`. Identifiants pris : 0 à 118 (`blocks.json`), 200 à 208 (`blocks_craft.json`), 210 à 220 (`blocks_survival.json`). Libres : 119 à 199 et 221 à 255, soit **116** | Insuffisant pour un second univers complet dans le même espace (voir §5) |
| Objets | id < 256 = bloc, ≥ 256 objets purs, sauvegardés par nom | Aucun blocage |
| Mobs | `mobs_defaults.json` (3) + `mobs_nb.json` (42), milieux LAND / WATER, IA passive / agressive / troupeau | Listes à rattacher à un univers ; IA à étendre (doc 03) |
| Structures, flore | `structures.rs` (19 types, cellules de 64), `flora.rs` | Propres à vanilla |
| Survie | `survival.json` : butins, dureté, températures de biome, cultures, pêche, chaleur | Règles à scinder : moteur commun, tables par univers |
| Sauvegarde | `world.json` (`WorldMetadata`, `formatVersion = 1`, `worldConfig`), chunks `.nbc` en ids u8 | Ajouter l'univers et la palette de blocs |
| Monde | 16 × 128 × 16 par chunk, niveau de la mer 34, eau statique, jour de 1200 s | Norrcraft reste dans 128 de haut en V1 (doc 02 §2) |
| Maillage | `nb-mesh::BiomeSource { biome_at → BiomeId, tints }` | Déjà générique : rien à changer côté rendu |

## 2. Vocabulaire

* **Univers** (`WorldKind`) : ensemble étanche de contenu et de règles. Deux valeurs : `vanilla`,
  `norrcraft`. C'est ce que Gabriel appelle « type de monde ».
* **Préréglage** (`TerrainPreset`) : variante de relief à l'intérieur d'un univers. Vanilla garde
  les sept actuels ; Norrcraft en a trois (doc 02 §11).
* **Profil** (`WorldProfile`) : objet construit au chargement d'un monde, qui porte tout ce qui
  dépend de l'univers (registres, générateur, règles).
* **Mode de jeu** (créatif / survie) : orthogonal à l'univers, inchangé.

## 3. Principe : un moteur, deux profils

```
                    ┌──────────────── moteur commun ────────────────┐
                    │ chunks, lumière, maillage, physique, ECS,     │
                    │ inventaire, craft, réseau LAN, rendu, audio   │
                    └───────────────▲───────────────▲───────────────┘
                                    │ WorldProfile  │
              ┌─────────────────────┴───┐       ┌───┴─────────────────────┐
              │ profil VANILLA          │       │ profil NORRCRAFT        │
              │ générateurs Classic et  │       │ générateur Norr         │
              │ Norrblock, 30 biomes,   │       │ 25 biomes, saisons,     │
              │ 45 mobs, 19 structures  │       │ bestiaire, mécaniques   │
              └─────────────────────────┘       └─────────────────────────┘
```

Règle d'or : **aucun fichier du moteur commun ne nomme un biome, un mob, une structure ou un bloc
d'univers**. Les seuls blocs que le moteur connaît par leur nom sont ceux du socle (§5.1).

### 3.1 Contrat du profil

Le profil est une structure immuable partagée (`Arc`) entre les threads de génération, l'ECS et
l'interface. Contenu attendu :

| Champ | Rôle | Vanilla | Norrcraft |
| --- | --- | --- | --- |
| `kind` | identité | `vanilla` | `norrcraft` |
| `blocks` | registre des blocs (socle + univers) | 139 blocs | 243 (doc 04 §9) |
| `items`, `recipes` | objets et recettes | fichiers actuels | fichiers Norrcraft |
| `biomes` | table `BiomeId → BiomeInfo` (nom, teintes, température, précipitations, musique) | 30 entrées tirées de l'enum actuel | 25 entrées de `biomes.json` |
| `generator` | fabrique de `TerrainGenerator` (une instance par thread) | adaptateur autour de `WorldGenerator` | nouveau générateur |
| `mobs` | registre des mobs et de leurs règles d'apparition | 45 | bestiaire du doc 03 |
| `rules` | tables de survie (butins, dureté, chaleur, cultures, pêche) | `survival.json` | `norrcraft/survival.json` |
| `systems` | drapeaux de systèmes optionnels | tous à faux | `seasons`, `mys`, `freshness`, `folklore`, `air_medium`… |
| `ui` | options de l'écran de création, textes | curseurs actuels | préréglages Norrcraft |

### 3.2 Contrat du générateur

Interface unique, pure et réentrante, par laquelle le moteur parle à n'importe quel générateur :

| Opération | Entrée → sortie | Utilisée par |
| --- | --- | --- |
| `surface_height` | (x, z) → y du premier bloc libre | spawn, structures, mobs |
| `biome_at` | (x, z) → `BiomeId` | maillage (teintes), spawn, survie, audio |
| `generate_chunk` | chunk vide → chunk rempli | threads `nb-chunkgen` |
| `find_spawn` | graine → position de départ | création de monde |
| `climate_at` (optionnel) | (x, z) → température, latitude, humidité | saisons, chaleur, durée du jour |

Vanilla l'implémente par un simple adaptateur qui appelle le code actuel (les deux `if
GeneratorKind` restent **à l'intérieur** de l'adaptateur vanilla). Norrcraft l'implémente
directement.

### 3.3 Biomes : de l'enum à la table

* Le moteur manipule `BiomeId` (u8, déjà le type de `nb-mesh`) et lit `BiomeInfo` dans le profil.
* `enum Biome` reste tel quel mais devient un détail interne du module vanilla ; son
  `ordinal()` sert de `BiomeId` vanilla, donc **aucun changement de comportement**.
* Norrcraft déclare ses biomes dans `assets/norrcraft/biomes.json` ; l'ordre du fichier fixe les
  identifiants, écrits aussi dans `world.json` pour figer la correspondance.
* Les fichiers qui citent `Biome::` hors de `nb-world` (`nb-data/mobs.rs`, `nb-data/survival.rs`,
  `nb-game/audio.rs`, `nb-game/chunks.rs`) passent par le nom ou par `BiomeInfo`.

## 4. Organisation du code et des données

### 4.1 Modules

```
crates/nb-world/src/
  profile.rs            WorldKind, WorldProfile, TerrainGenerator, BiomeInfo   (nouveau, commun)
  chunk.rs light.rs manager.rs storage.rs save.rs noise.rs world.rs            (commun, inchangé)
  vanilla/              config.rs (presets), generator.rs, generator_nb.rs,
                        terrain.rs, biome.rs, structures.rs, flora.rs          (déplacés tels quels)
  norrcraft/            fields.rs (champs climatiques), relief.rs, hydro.rs,
                        biomes.rs, strata.rs, flora.rs, structures.rs,
                        seasons.rs, generator.rs                               (nouveau)
```

`nb-ecs` : les systèmes nouveaux (saisons, Mys, fraîcheur, IA à états) vivent dans des modules
séparés, activés par `profile.systems`. Aucun `if kind == norrcraft` dans le code : on teste un
drapeau de système, jamais l'univers.

### 4.2 Données

```
assets/
  common/      blocks.json (socle), items.json (bâton…), textures de l'interface
  vanilla/     blocks.json, blocks_craft.json, blocks_survival.json, items*.json, recipes*.json,
               mobs_defaults.json, mobs_nb.json, mob_geometry*.json, mob_rigs.json, survival.json
  norrcraft/   blocks.json, items.json, recipes.json, biomes.json, mobs.json, mob_geometry.json,
               mob_rigs.json, survival.json, seasons.json, structures/, saga.json, runes.json
```

Les scripts générateurs (`gen_mobs.py`, `gen_survival.py`, `gen_textures*.py`) gagnent un
argument d'univers et écrivent dans le bon dossier. Les textures restent dans
`resource_packs/Default/`, préfixées `norr_` pour celles de Norrcraft afin d'éviter toute
collision de nom avec Faithful.

## 5. Blocs : socle commun et palette par monde

### 5.1 Socle

Blocs que le moteur référence par leur nom, communs aux deux univers et toujours aux mêmes
identifiants : `air` (0), `water`, `lava`, `bedrock`, plus les blocs techniques (lumière nulle).
Tout le reste appartient à un univers. La pierre, la terre ou l'herbe de Norrcraft sont des blocs
Norrcraft (granit, morän, etc.), pas ceux de vanilla.

### 5.2 Le problème du u8

Vanilla occupe 139 identifiants sur 256. Norrcraft en demande 243 (doc 04 §9). Les deux ne
tiennent pas ensemble. Trois options :

| Option | Principe | Coût | Verdict |
| --- | --- | --- | --- |
| A. Palette par monde | Chaque monde charge le socle + les blocs de **son** univers ; la table `nom → id` est figée dans `world.json` | Faible : registre construit au chargement, atlas reconstruit (déjà fait au changement de pack) | **Recommandée en V1** |
| B. `BlockId = u16` | Un seul espace global | Fort : mémoire des chunks ×2, format `.nbc`, mailleur, lumière, réseau, tous les tests | À garder en réserve si Norrcraft dépasse 250 blocs |
| C. États de bloc séparés | Un octet d'état par bloc (stades de culture, épaisseur de neige) | Moyen | Complément possible de A plus tard |

Avec A, vanilla garde exactement ses identifiants actuels (table figée = table d'aujourd'hui),
donc ses chunks et ses vecteurs de référence ne bougent pas. Norrcraft dispose de 256 identifiants
pour lui seul, avec un **budget** suivi dans le doc 04.

### 5.3 Conséquences de la palette par monde

* `world.json` gagne `blockPalette` (liste ordonnée de noms). Au chargement, un nom inconnu
  devient un bloc « manquant » visible, sans plantage.
* L'atlas de textures et les tables du mailleur sont reconstruits à l'entrée dans un monde.
* LAN : la poignée de main envoie `worldKind` et une empreinte de la palette ; un invité dont
  l'empreinte diffère est refusé avec un message clair.
* L'inventaire créatif liste les blocs du profil courant.

## 6. Sauvegardes

`world.json` passe à `formatVersion = 2` et gagne :

| Champ | Contenu | Valeur par défaut à la lecture |
| --- | --- | --- |
| `worldKind` | `"vanilla"` ou `"norrcraft"` | `"vanilla"` (tous les mondes existants) |
| `blockPalette` | noms des blocs dans l'ordre des ids | palette vanilla actuelle |
| `biomePalette` | noms des biomes dans l'ordre des ids | les 30 de vanilla |
| `calendar` | jour de l'année, année, phase du cycle des lemmings | absent en vanilla |
| `saga` | progression de l'objectif (doc 05) | absent en vanilla |
| `norr` | états propres : Mys, Naturfrid, runes apprises | absent en vanilla |

Le champ `worldConfig.generator` garde son sens (Classic / Norrblock) et n'est lu que par le
profil vanilla. Un monde Norrcraft a son propre bloc `norrConfig` (préréglage, longueur de
l'année, rudesse de l'hiver, densité des structures, folklore actif ou non). Comme convenu avec
Gabriel, aucune compatibilité n'est exigée avec les anciennes sauvegardes, mais cette lecture par
défaut la donne gratuitement.

## 7. Interface

* Écran de création : premier sélecteur **Univers : Norrcraft (par défaut) / Vanilla**.
  * Vanilla : l'écran actuel, inchangé (préréglages, curseurs, 8 densités d'arbres).
  * Norrcraft : préréglage (Norrland, Skärgård, Högfjäll), longueur de l'année, rudesse de l'hiver,
    folklore (activé, paisible, désactivé), densité des structures.
* Liste des mondes : pastille d'univers à côté du nom.
* Écran des mobs (`screens/mobs.rs`) : liste le registre du profil du monde ouvert ; hors monde, un
  onglet par univers.
* Réglages `saves/mobs.json` : scindé en `saves/mobs_vanilla.json` et `saves/mobs_norrcraft.json`.

## 8. Matrice de ce qui est partagé

| Élément | Commun | Vanilla | Norrcraft |
| --- | --- | --- | --- |
| Chunks, lumière, maillage, rendu, physique, réseau | ✔ | | |
| Inventaire, craft, stations, livre de recettes (moteur) | ✔ | | |
| Faim, chaleur, endurance (moteur) | ✔ | tables actuelles | tables Norrcraft, plus vent et humidité |
| Générateur, biomes, structures, flore | | ✔ | ✔ (aucun élément repris) |
| Mobs | | 45 | bestiaire neuf |
| Blocs et objets | socle | 139 blocs, 90 objets | 243 blocs, environ 260 objets |
| Saisons, calendrier, Mys, fraîcheur, runes, saga, folklore | moteur | désactivés | activés |
| Musique et ambiances | moteur procédural | thèmes actuels | thèmes nordiques (doc 04 §8) |

## 9. Ordre de migration (sans régression)

1. **Geler la référence** : empreintes FNV des chunks vanilla (Classic et Norrblock, 3 graines,
   16 chunks chacune) ajoutées aux tests existants (`tests/reference.rs`, `tests/norrblock.rs`).
2. **Déplacer** les fichiers vanilla dans `nb-world/src/vanilla/` sans modifier une ligne de
   logique ; réexporter les mêmes noms publics. Les 433 tests doivent rester verts.
3. **Introduire** `WorldKind`, `WorldProfile`, `TerrainGenerator`, `BiomeInfo` ; brancher vanilla
   dessus par l'adaptateur ; remplacer les usages de `Biome::` hors vanilla.
4. **Palette par monde** : registre construit depuis `common/` + univers ; `world.json` v2.
5. **Déplacer les données** dans `assets/common|vanilla|norrcraft/`.
6. **Interface** : sélecteur d'univers ; Norrcraft affiche d'abord un générateur bouchon (monde
   plat de granit) qui prouve le circuit complet avant tout travail de worldgen.
7. Basculer la valeur par défaut des nouveaux mondes sur Norrcraft **seulement** quand la phase 2
   du plan est livrée (doc 06).

## 10. Critères d'acceptation

* Les empreintes de chunks vanilla sont identiques avant et après la migration.
* Test d'étanchéité : le registre Norrcraft ne contient aucun nom de biome, de mob ou de structure
  de vanilla, et réciproquement (hors socle).
* `grep` : aucun module commun ne contient le nom d'un biome, d'un mob ou d'un bloc d'univers.
* Un monde vanilla créé avant la migration s'ouvre sans différence visible.
* Un monde Norrcraft bouchon se crée, se sauvegarde, se recharge et s'héberge en LAN.
* Un invité LAN d'un autre univers ou d'une autre palette reçoit un refus explicite.

## 11. Risques

| Risque | Parade |
| --- | --- |
| Régression silencieuse de vanilla pendant le déplacement | Étapes 1 et 2 strictement mécaniques, empreintes gelées avant |
| Budget de 256 blocs dépassé par les états (cultures, neige, glace) | Budget tenu dans le doc 04 ; option C ou B en réserve |
| Double maintenance des règles de survie | Un seul moteur, deux tables ; les scripts générateurs partagent leur code |
| Atlas reconstruit à chaque entrée de monde (temps) | Cache de l'atlas par empreinte de palette |
