# Structures du monde

Depuis la phase 9, le générateur « Norrblock » parsème le monde de **19 types de structures**
(`crates/nb-world/src/vanilla/structures.rs`) : hameaux, cabanes, moulins, tours, temples, ruines, épaves,
phares, cercles de pierres, fossiles… En moyenne **70 structures par km²** (une tous les
120 × 120 blocs environ, océans compris ; mesuré sur 16 km² avec la graine 12345).

## Principe

Comme les arbres, une structure est une **fonction pure** de `(graine, configuration, cellule)` :
aucun état, aucune dépendance à l'ordre de génération des chunks ni au contenu des voisins.

1. Le monde est découpé en **cellules de 64 × 64 blocs**. Chaque cellule porte au plus une
   structure : le hachage de la cellule décide si elle en a une (55 % × `structureDensity`), de sa
   position (dans la partie centrale de la cellule, donc deux structures voisines ne se touchent
   jamais) et de sa variante.
2. Le **type** est tiré parmi ceux que le biome du centre autorise (poids par biome).
3. L'emplacement n'est retenu que si le terrain s'y prête : pour une structure terrestre, 9 sondes
   dans son rayon doivent être au-dessus de la mer, hors lave, avec un dénivelé inférieur à une
   tolérance propre au type (4 à 7 blocs) ; pour une structure sous-marine, un fond plat à 3 blocs
   au moins sous la surface ; le phare doit toucher l'eau.
4. Chaque chunk reconstruit **en entier** les structures des cellules voisines qui le touchent et
   n'écrit que la part qui tombe chez lui. Les fondations (piliers jusqu'au sol, comblement) et le
   dégagement (arbres, relief) ne dépendent que du terrain, donc les raccords entre chunks sont
   identiques quel que soit l'ordre (`structure_generation_is_independent_of_chunk_order`).
5. La passe s'exécute après les arbres et les décors, avant la flore : la végétation du disque de
   la structure (rayon + 5 blocs) est retirée pour que les canopées ne recouvrent pas les
   bâtiments, puis la structure s'écrit par-dessus le terrain.

Chaque structure utilise un repère local tournable (4 orientations) : le constructeur (`Build`)
offre `set`, `fill`, `walls`, `pad` (fondation + dégagement), `pillar` (pilier jusqu'au sol) et
une pile de repères pour composer (les maisons d'un hameau se tournent vers le puits).

## Les 19 types

| Structure | Biomes | Contenu |
| --- | --- | --- |
| Hameau | plaines, prairie, forêts, savane, désert (rare) | puits, 3 à 5 maisons meublées (style selon le biome : chêne, épicéa, acacia, grès, cerisier…), chemins de terre, 2 champs irrigués (blé, citrouilles), lampadaires, bottes de paille |
| Cabane | forêts, taïga, lande, jungle, marais | maison de rondins meublée, cheminée avec feu, tas de bûches, chemin |
| Moulin | plaines, prairie fleurie, forêt d'automne, savane | tour conique de 10 blocs, axe et quatre pales à voiles |
| Tour de guet | plaines, collines, montagnes, taïga, savane | fût 5 × 5 de 11 à 15 blocs, escalier en colimaçon, fenêtres, plateforme crénelée avec lanterne |
| Tour en ruine | forêts, jungle, marais, lande, collines | même fût, érodé par le haut, mousse, toiles d'araignée, lianes, bloc d'or au pied |
| Pyramide du désert | désert, badlands | 9 gradins, couloir d'entrée, chambre 7 × 7 avec or, bibliothèques, lanternes |
| Temple de la jungle | jungle | pyramide moussue envahie de lianes et d'hibiscus, même chambre |
| Campement | presque partout | deux tentes en laine (couleurs selon le climat), feu de camp entouré de bûches, lampadaire |
| Igloo | terres enneigées | dôme de neige, plancher de glace compacte, tunnel, lit, feu, bibliothèque |
| Cercle de pierres | lande, collines, plaines, méga-taïga | 8 à 10 menhirs (3 à 5 blocs), linteaux, autel de marbre avec feu de camp |
| Phare | plages, rivages rocheux | tour rouge et blanche de 17 blocs, escalier, lanterne vitrée |
| Épave | océans | coque de 17 blocs trouée, mât brisé, voile en lambeaux, cabine avec or |
| Ruines sous-marines | océans (récifs de corail en eaux chaudes) | esplanade, colonnes brisées, autel d'obsidienne, trésor, lanternes marines |
| Hutte de sorcière | marais | maison sur pilotis, toit pointu, chaudron moussu, toiles, ponton |
| Sanctuaire des cerisiers | forêt de cerisiers | pavillon à toit en pagode, torii rouge, chemin dallé, lanternes |
| Obélisque | montagnes, déserts, sommets, pics de glace | aiguille de 13 à 18 blocs (basalte / grès / glace), anneaux lumineux, pierres basses |
| Forge du volcan | volcan | plateforme de basalte, quatre pylônes de magma, fosse de lave, enclume d'obsidienne |
| Fossile géant | désert, badlands, savane, plaines | colonne vertébrale ondulée, côtes en arche, crâne en blocs d'os |
| Maison champignon | champs de champignons | pied creux à porte, chapeau rouge à pois |

## Réglage

* `structureDensity` dans `world.json` (défaut 1,0) : curseur **Structure Density** (0 à 3) de
  l'écran de création, section *Features* ; 0 supprime toutes les structures.
* Les mondes `Classic` (générateur Java) n'ont aucune structure.

## Outils de mise au point

```sh
# Liste et répartition des structures d'une région (graine, type, x0, z0, taille)
cargo run --release -p nb-world --example structmap -- 12345 normal -2000 -2000 4000
# Rendu isométrique (PPM) de la n-ième occurrence d'un type ; palette = couleur moyenne par bloc
cargo run --release -p nb-world --example structview -- 12345 normal "hameau" 0 palette.txt hameau.ppm [rotation]
```

## Ajouter une structure

1. Ajouter une variante à `StructureKind` (nom, rayon, tolérance) et sa ligne dans `weights`.
2. Écrire `fn ma_structure(b: &mut Build, …)` avec le repère local (origine = centre, `y = 0` =
   premier bloc libre au-dessus du sol, porte vers −Z) : appeler `b.pad_disc` / `b.pad` d'abord.
3. L'enregistrer dans `build_site` ; `phase9.rs` vérifie qu'elle est produite et qu'elle laisse
   assez de blocs de construction.
