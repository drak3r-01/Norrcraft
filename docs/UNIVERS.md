# Univers, palette de blocs et sauvegardes (plan Norrcraft, phase 0)

Le jeu héberge deux **univers** de contenu étanches, `vanilla` (tout l'existant) et `norrcraft`
(le nouveau monde, en construction), sur un seul moteur. Cette page décrit ce que les sessions 0.1
à 0.3 ont mis en place. Le plan complet est dans `docs/gdd-norrcraft/` (docs 01 et 06).

## Données par univers

```
assets/
  common/      blocks.json                 socle : air (0), water (4), bedrock (6), lava (33)
  vanilla/     blocks, blocks_craft, blocks_survival, items, items_survival, recipes,
               recipes_survival, mobs_defaults, mobs_nb, mob_geometry(_nb), mob_rigs, survival
  norrcraft/   blocks.json (blocs de terrain, sessions 1.2 à 2.1), biomes.json (2.1 : 25 biomes,
               teintes, ambiances, sols), items.json, recipes.json
  particles.json, settings_defaults.json, textures/   (partagés)
```

`crates/nb-data/src/universe.rs` est le seul endroit qui sait quels fichiers appartiennent à quel
univers (`WorldKind::block_sources`, `item_sources`, `recipe_sources`). Les scripts générateurs
(`gen_survival.py`, `gen_mobs.py`) écrivent dans `assets/vanilla/`.

Le socle a les mêmes identifiants dans tous les univers. Un test (`nb-data/tests/palette.rs`)
vérifie qu'aucun autre bloc n'est commun aux deux univers.

## Palette de blocs par monde

`BlockId` reste un `u8`. Chaque monde sauvegarde sa **palette** (`BlockPalette`) : la liste des
noms de blocs dont la position est l'identifiant écrit dans les chunks.

* Monde neuf : palette par défaut de l'univers (`BlockRegistry::for_kind`), écrite dans
  `world.json` dès la création. Pour vanilla, elle est identique à celle d'avant : mêmes
  identifiants, mêmes chunks, mêmes empreintes de référence.
* Ouverture d'un monde : `BlockRegistry::from_palette(kind, palette)` reconstruit le registre.
  * un nom de la palette que le jeu ne connaît plus devient un bloc **manquant** (plein, opaque,
    texture `missing`) qui garde son nom et sa place ; le nom est réécrit à la sauvegarde ;
  * un bloc du jeu absent de la palette prend le premier identifiant libre ;
  * `air` doit être à 0 ; l'identifiant **255 est réservé** (`UNASSIGNED_ID`).
* Les blocs que le code nomme (`BlockIds` : `stone`, `grass`…) et que l'univers ne définit pas
  valent `UNASSIGNED_ID` : aucun chunk ne le contient, donc aucune comparaison ne réussit par
  erreur. Seul le socle est obligatoire.
* Budget : 254 identifiants par monde, socle compris (décision D2 du plan : u8 d'abord).
* `Registries::for_world(kind, palette)` (nb-game) construit blocs, objets, recettes, survie et
  mobs de l'univers. Norrcraft n'a pour l'instant ni objets purs, ni recettes, ni règles de
  survie, ni mobs.

Quand les registres changent (monde d'un autre univers ou d'une autre palette), `Game` demande à
la plateforme `PlatformRequest::RebuildBlockTable` : l'atlas reste, la table de maillage, les
icônes et les éclats de blocs sont refaits d'après les nouveaux registres. Le monde du menu est
toujours vanilla ; quitter un monde Norrcraft remet les registres vanilla.

## `world.json` version 2

| Champ | Contenu |
|---|---|
| `formatVersion` | 2 (la 1 se lit encore, avec les valeurs par défaut ci-dessous) |
| `worldKind` | `"vanilla"` (défaut) ou `"norrcraft"` |
| `blockPalette` | noms des blocs par identifiant, `""` = libre ; absent = palette par défaut |
| `biomePalette` | noms des biomes par identifiant (table du profil), écrite à la sauvegarde |

`saves/mobs.json` est scindé par univers : `mobs_vanilla.json` (l'ancien `mobs.json` est encore
lu en repli) et `mobs_norrcraft.json`.

## Réseau local (protocole 3)

* `Hello` (invité → hôte) annonce le contenu de la version de l'invité : pour chaque univers,
  l'empreinte FNV-1a de sa palette par défaut (`content_fingerprints`).
* L'hôte refuse l'invité dont la version ne connaît pas l'univers du monde, ou dont la palette
  par défaut de cet univers diffère, avec un message explicite (voir `docs/LAN.md`).
* `Welcome` (hôte → invité) donne `world_kind` et `block_palette` du monde. L'invité construit ses
  registres d'après eux : les chunks reçus sont dans les identifiants de l'hôte. En quittant la
  partie, il retrouve les registres vanilla de son menu.

## Interface et générateur bouchon (session 0.4)

* Écran de création : bouton **Universe: Vanilla / Norrcraft** à droite de l'étiquette « World
  Name: » (vanilla par défaut, jusqu'à la livraison de la phase 2 du plan). En Norrcraft, le type
  de monde et le panneau « Advanced » (réglages vanilla) sont masqués. La liste des mondes affiche
  une pastille d'univers `[Vanilla]` / `[Norrcraft]`.
* `nb_world::FlatGraniteTerrain` : `y = 0` bedrock, `1..64` granit (lu dans le registre du
  monde), air au-dessus, apparition au centre. Il remplace `EmptyTerrain` dans
  `World::for_kind` ; la phase 1 le remplacera par le vrai relief.
* La balise UDP annonce l'univers (`ServerInfo.world_kind`, ajouté en fin de paquet) : la liste des
  parties LAN affiche « Hosted by Léa - Norrcraft - 1/8 players - ip:port ».
* Test d'étanchéité : `crates/nb-game/tests/sealed.rs` (blocs, objets, mobs et biomes disjoints
  hors socle ; aucun nom Norrcraft ne contient un nom de biome ou de mob vanilla).

## Reste à faire (sessions suivantes)

* `WorldConfig` est encore celui de vanilla pour les deux univers ; le bloc `norrConfig` du
  doc 01 §6 arrivera avec les réglages propres à Norrcraft.
* Les structures n'ont pas encore de registre : le test d'étanchéité les couvrira à leur arrivée.
