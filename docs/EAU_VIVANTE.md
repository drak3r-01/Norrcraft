# Eau vivante (session 6E.1 : modèle et simulation)

Conception : `docs/gdd-norrcraft/07-eau-vivante.md`. Cette page décrit ce qui est **codé** à la fin
de la 6E.1 et les écarts au document. Le rendu (6E.2), les courants côté ECS (6E.3) et la
génération des chutes (6E.4) ne sont pas encore faits : une cellule d'eau partielle s'affiche
comme un bloc d'eau plein, personne ne subit encore le courant.

## Modèle de données

* L'identifiant `water` reste « il y a de l'eau ici ». Un octet **fluide** par cellule
  (`fluid::FluidCode`) porte le niveau (0 à 8 huitièmes), la direction d'un flux permanent et le
  drapeau `STEADY`. Il vit dans `Chunk::fluid: Option<Box<[u8; 32768]>>`, allouée à la première
  écriture non nulle et libérée quand elle redevient nulle.
* Un chunk sans couche se lit comme avant : toute cellule `water` est pleine et au repos. Vanilla
  ne change pas d'un octet (`Systems::living_water` est faux ; les empreintes de référence sont
  inchangées).
* Classes de blocs (`fluid::FluidBlocks`, clé `"fluid"` de `blocks.json`) : `closed`, `permeable`,
  `aquatic`, `washable`, déduites pour `auto` (croix et couches : emportés ; le reste : étanche).
  Valeurs posées dans `assets/norrcraft/blocks.json` pour les plantes d'eau, les clôtures, la
  hässja, la passerelle et la torche. Nouveaux blocs : 177 `kalla`, 178 `isfall`, 179 `svallis`
  (textures provisoires) ; objets 315 `hink`, 316 `hink_vatten`.
* Table d'eau d'un chunk (`TerrainGenerator::water_table`, `NorrcraftTerrain`) : fond, niveau,
  nature (mer, lac, rivière) et courant analytique des rivières (`Hydrography::river_flow`).
  Mer et rivières sont des nappes **infinies** (elles donnent et absorbent) ; les lacs sont finis.
* `Chunk::replace_voxels_keep` et `Chunk::sanitize_fluid` assainissent la couche quand les voxels
  changent (surcouche saisonnière, chargement d'un chunk de la saison précédente, réseau).

## Stockage

Format `NBCK` **version 2**, écrit seulement si le chunk a une couche fluide non nulle (sinon le
fichier reste en version 1, octet pour octet). Après les voxels : `u8` présence, `u32` longueur,
couche en RLE. `decode_chunk_blob` lit les versions 1 et 2.

## Simulation (`nb-world/src/fluid/`)

* `FluidWorld` (trait) : `MemFluid` pour les tests sans chunks, `ManagerFluid` sur le
  `ChunkManager` (cache de 4 chunks). Le gestionnaire collecte les événements (éditions, chunks
  chargés et déchargés) seulement si `enable_fluids` a été appelé : coût nul pour vanilla.
* `FluidSim::tick`, un pas toutes les 12 ticks (5 par seconde), au plus 512 cellules par tick et
  400 µs : quand la file déborde, l'eau ralentit, pas le jeu. Règles R1 (chute), R2 (étalement),
  R3 (égalisation et pression, recherche bornée), R4 (ruissellement d'un film), R5 (trempe au
  contact de la lave). Constantes dans `assets/norrcraft/water.json` (`nb_data::WaterRules`).
* Seules les cellules éveillées sont calculées. Un écoulement stable passe en régime permanent
  (octet `STEADY` + direction) et ne coûte plus rien ; tout transfert voisin le réveille.
* Sources (`kalla`, débit saisonnier), suintement des tourbières, pluie et flaques, évaporation,
  gel et dégel de l'eau libre, recharge lente des lacs : `fluid/sources.rs`.
* Courants (`FluidSim::current_at`) et `flow_power` (crochet pour les roues de la 6.6).
* `World::tick_fluids`, `water_level`, `current_at`, `take_fluid_deltas`, `with_fluid`.
  `WorldSession::tick` l'appelle après les systèmes ECS (jamais sur un monde client).
* Commande de triche `/water pour [n] | spring | drain | wake | info`.

## Écarts au document 07

1. **Stockage** : le trait `ChunkStorage` garde `save_chunk`/`load_chunk` et gagne
   `save_chunk_blob`/`load_chunk_blob` (par défaut : sans couche), au lieu de changer les
   signatures ; `encode_chunk`/`decode_chunk` restent, `ChunkBlob` et `*_blob` s'y ajoutent.
   `install_chunk_blob`/`blob_of` côtés réseau.
2. **Régime permanent** : seules les cellules d'une chute (eau qui tombe) et les nappes infinies
   qui les alimentent s'installent en `STEADY`. Un écoulement horizontal d'eau libre reste simulé
   tant qu'il bouge : le figer en cours d'équilibrage laissait une pente (7, 5, 3, 2, 1 dans un
   canal) endormie hors équilibre. Une cellule ne s'installe que si tous ses partenaires
   d'échange du pas le font aussi (une source ne se retire pas tant que l'aval bouge).
   Le débit d'une chute simulée à partir d'une source infinie est de 4 h par pas (nappe de
   niveau 4, comme les chutes que la 6E.4 écrira).
3. **Nappes infinies** : une cellule infinie compte pour *pleine* (on repose dessus, ce n'est
   jamais un « bord »), remplit les cellules voisines jusqu'à 8 (une source donne jusqu'au
   bord) et deux nappes infinies voisines n'échangent rien.
4. **Ordre des cellules** : la file d'un pas est triée de bas en haut (puis x, z), pour que la
   colonne d'eau qui tombe reste continue d'un pas à l'autre.
5. **`FluidWorld`** expose `table(cpos)` (table d'eau du chunk) au lieu de `rest(x, z)` : la
   simulation garde les tables en cache et indexe elle-même la colonne. `scan` et `mark_mesh`
   s'ajoutent pour le démarrage d'un chunk et le remaillage de fin de pas.
6. **Météo** : les colonnes tirées au hasard pour pluie, gel, recharge et suintement le sont dans
   un carré de 64 blocs autour d'un joueur (et non dans tout le rayon de simulation de 96 blocs,
   trop vaste pour 12 tirages par pas). `air_temp` est la température d'air de l'`Environment`
   (échelle de T₀).
7. **Un émetteur qui ne verse rien** (torrent glaciaire en hiver) réveille les cellules en
   régime permanent qui l'entourent : sans cela une cascade interrompue resterait figée.

## Pas encore fait (sessions suivantes)

Mailleur, shaders, particules, son, protocole LAN 5 (6E.2) ; `BlockAccess::fluid_level/current`,
joueur, mobs, bateaux, saumons, plantes vivantes, glace (6E.3) ; marches de rivière en chutes,
ouvrages, nouvelles plantes, cascades gelées, mode `eau` de `worldmap` (6E.4). Les nouvelles
sources ne naissent pour l'instant que par `/water spring`. Les chunks déjà générés d'un monde
existant n'ont ni chute ni source (décision E7).

## Tests

`nb-world/tests/fluid_rules.rs` (16 : conservation, repos, égalisation, chute, bord, vases
communicants, film, nappe infinie, lac percé, régime permanent, blocs, bord de chunk, trempe,
déterminisme, budget), `fluid_storage.rs` (format, assainissement, gestionnaire, simulation sur
de vrais chunks), `norrcraft_water.rs` (table égale aux colonnes générées, courant vers l'aval).
