# Milieux et apparition par densité cible (session 5.2)

Référence : GDD doc 03 §1 (milieux AIR, AMPHIBIE, GLACE), §4.1 à §4.3 (champs d'apparition,
densité cible, grille d'habitat). Les espèces arrivent avec les sessions 5.4 à 5.6 ; cette session
livre les milieux et le système d'apparition, sans fiche d'espèce.

## Où est quoi

| Fichier | Rôle |
| --- | --- |
| `crates/nb-data/src/spawn_rules.rs` | `SpawnRules` (bloc `"spawn"` d'un mob), `Persistence`, `Avoid`, `Sky`, facteurs de saison et de météo |
| `crates/nb-data/src/mobs.rs` | `MobMedium` gagne `Amphibian` et `Ice` ; `Gait` (marche, nage, vol) ; `MobConfig.spawn` |
| `crates/nb-data/src/blocks.rs` | `BlockRegistry::is_ice` (glace praticable) |
| `crates/nb-ecs/src/density.rs` | relevé des biomes, cibles, tirage par déficit, validation des points, disparition |
| `crates/nb-ecs/src/spawn.rs` | `Spawner::find_spawn_y` pour les nouveaux milieux ; `find_air_y`, `find_ice_y`, `find_shore_y` |
| `crates/nb-ecs/src/physics.rs` | allure selon le milieu et l'eau ; friction de la glace |
| `crates/nb-ecs/src/brain/mod.rs` | `Ground`, `effective` : l'IA d'états suit l'allure courante |
| `crates/nb-ecs/tests/spawn_density.rs` | scénarios du banc d'essai |
| `crates/nb-ecs/tests/spawn_media_norr.rs` | habitats dans 3 mondes Norrcraft générés |

## Milieux

| Milieu | Physique | IA | Apparition |
| --- | --- | --- | --- |
| `LAND` | gravité, saut | inchangé | sol à l'air libre |
| `WATER` | nage 3D, sans gravité | inchangé | colonne d'eau |
| `AIR` | vol 3D sans gravité (`FRICTION_FLY` 3), collisions des nageurs | vole à `cruise` au-dessus du gîte, se perche en descendant | au-dessus du plus haut bloc, à `spawn.altitude` (défaut : `cruise` du profil, sinon 10) |
| `AMPHIBIAN` | nage dans l'eau (corps dans un bloc d'eau), marche et tombe à terre | l'eau n'est pas une impasse : il peut regagner la rive | rive (sol avec de l'eau à ≤ 3 blocs) sinon colonne d'eau |
| `ICE` | marche, friction `FRICTION_ICE` 1,5 sur la glace (6 sur terre) | ne quitte pas la glace (`ice_only`), n'évite pas l'eau en la traversant | premier bloc plein de la colonne s'il est de la glace à l'air libre |

Blocs de glace praticables : `ice`, `packed_ice` (vanilla), `blais`, `havsis`, `nyis`, `is`,
`tjockis` (Norrcraft). Un volatile sans bloc `spawn` n'apparaît pas (comportement d'origine : un
mob `AIR` d'un `mobs.json` vanilla reste inerte).

## Bloc `spawn` d'un mob

```json
{ "id": "bjorn", "behavior": "allatare", "maxCount": 40, "groupSizeMin": 1, "groupSizeMax": 2,
  "spawn": {
    "densityPerBiome": { "URSKOG": 0.3, "FJALLBJORKSKOG": 0.2 },
    "seasonCurve": [0, 0, 0.5, 1, 1, 1, 1, 0.5],
    "timeOfDay": "crepuscular",
    "weather": { "rain": 0.7 },
    "avoid": { "fire": 10, "player": 32 },
    "persistence": "resident" } }
```

| Champ | Sens |
| --- | --- |
| `densityPerBiome` | individus visés pour 64 × 64 blocs, par nom de biome (obligatoire) |
| `seasonCurve` | 4 à 12 facteurs répartis sur l'année **à partir du solstice d'hiver** (le facteur `i` est posé à `i/n`, interpolation linéaire en boucle). 0 = absent |
| `timeOfDay` | `diurnal`, `nocturnal`, `crepuscular`, `always` (mêmes courbes que l'IA : `clock::activity`) |
| `weather` | facteurs par état : `rain` (pluie > 0,2), `snow` (> 0,2), `mist` (brouillard > 0,015), `wind` (vent > 0,6), `clear` (aucun des quatre). Produit des états actifs |
| `light` | hauteur maximale du soleil (−1 à 1) tolérée : `-0.1` = nuit seulement |
| `shelter` | exige un bloc plein ou un feuillage dans les 10 blocs au-dessus |
| `cycle` | `{name, lag}` d'un cycle de `behaviors.json` ; à défaut celui du profil de comportement |
| `nearFeature` | nom d'un bloc à 6 blocs ou moins (3 en hauteur) du point : trou de glace, corniche… |
| `avoid` | rayons en blocs : `fire` et `bell` lisent les `Emitter` du monde, `player` les joueurs ; `village` est lu mais sans effet tant que les entités ne voient pas les sites |
| `persistence` | `ephemeral` (défaut), `resident`, `herd`, `unique` |
| `naturfrid` | réservé : facteur de réputation (doc 04 §7), neutre pour l'instant |
| `altitude` | hauteur d'apparition d'un volatile au-dessus du sol |

Un `spawn` incohérent (aucun biome, courbe de 3 facteurs, état météo inconnu, valeur négative)
est refusé comme tout champ invalide de `mobs.json` : le mob garde son ancien état. Un mob avec
`spawn` quitte l'apparition d'origine (plafond, `spawnBiomes`, `spawnSurfaces`) ; `maxCount`
devient un plafond de sûreté mondial (× joueurs) et `maxCount: 0` désactive l'espèce. Les prédicats
d'habitat d'origine (`spawnMinY`, `spawnMaxY`, `spawnSurfaces`, profondeur d'eau) restent
applicables en plus.

## Cible et tirage

```
cible(e) = Σ_biomes surface_b · densité(e, b) / 4096
           × saison × heure × météo × lumière × cycle × naturfrid
```

Chaque seconde (tour de rôle entre les joueurs) :

1. relevé des biomes : un échantillon tous les 8 blocs dans le disque de rayon
   `mob_spawn_radius` (colonnes chargées seulement) ;
2. effectifs : individus de l'espèce dans le disque (déficit) et dans tout le monde (plafond) ;
3. jusqu'à 4 tirages : espèce au prorata de `max(0, cible − présents)`, point dans l'anneau
   24 → R, accepté avec la probabilité `densité(biome) / densité max`, puis milieu, couvert,
   décor, évitement, visibilité (jamais à moins de 24 blocs ; hors du cône de 130° à moins de
   80 blocs) ;
4. groupe de `groupSizeMin..Max` autour du point (`spawnScatterRadius`), limité par la place
   restante.

Les facteurs d'heure et de météo lisent l'`Environment` du joueur local, à défaut le calendrier.

## Disparition

* `ephemeral` : au-delà de `mob_despawn_radius` comme avant ; de plus, quand le facteur du moment
  est nul (hibernant, migrateur parti, année de creux nulle), chaque individu hors de vue part avec
  une chance de 25 % par seconde (l'ours est absent en hiver, même celui apparu en été).
* `resident`, `herd`, `unique` : ne disparaissent que si leur chunk est déchargé. Les hardes
  persistantes et leur simulation hors chunk arrivent en 5.3 (`herd` se comporte comme `resident`
  d'ici là).

## Écarts au document

* `naturfrid` et `village` sont des crochets : pas de réputation ni de sites visibles côté ECS.
* `shelter` teste un bloc plein ou une feuille au-dessus, pas la lumière du ciel.
* Le cône de vue ignore l'occlusion (un point derrière un mur reste « en vue »).
* La passe d'apparition vise un déficit par disque, pas par cellule : une espèce peut se grouper
  dans la partie du disque qui lui convient.
* Mondes Norrcraft générés (3 graines, 19 200 colonnes sondées sur ±1 280 blocs) : air partout,
  rive 442, eau libre 3 824, glace d'hiver 1 773 colonnes (lacs et mers gelés au pas de saison 0).
  La glace n'existe pas en été hors glaciers : les animaux de banquise suivent la saison (le
  facteur `seasonCurve` et la banquise font le reste).

## Tests

`cargo test -p nb-ecs --test spawn_density` (16 scénarios : cible de population, densités par
biome, hors de vue, ours en été et en hiver, disparition hors saison, heure, météo, année de pic
des lemmings, unique / résident, feu et décor, déterminisme, oiseaux en altitude, gravité,
amphibie, banquise, glissade) et `--test spawn_media_norr` (habitats dans 3 mondes générés,
~30 s).
