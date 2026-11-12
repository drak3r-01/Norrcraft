# Neige et glisse (session 6.2)

Plan Norrcraft, phase 6 « mécaniques nordiques » ; GDD doc 04 §4.3 (neige) et §4.4 (se déplacer
en hiver). Code : `nb-ecs/src/glide.rs` (surface, équipement, pente, véhicules, piétinement,
`GlideSystem`) et `nb-ecs/src/avalanche.rs` (coulée, ensevelissement, `AvalancheSystem`). Tests :
`nb-ecs/tests/glide.rs`.

## Neige profonde

`Surface` est lue sous les pieds : `Snow { layers }` (huitièmes de la cellule des pieds, `snotacke_n`,
0 sur un bloc `sno`), `Packed` (`packad_sno`), `Ice(kind)` ou `Bare`. À pied, `snow_slow` ne change
rien jusqu'à 3 couches, puis retire 12 % par couche (−60 % à 8, soit −48 % dans une cellule de 7).
Le facteur multiplie la vitesse dans `PlayerInputSystem` via le composant `Locomotion`, posé chaque
tick par `GlideSystem` (inséré en tête de l'ordonnancement, Norrcraft seulement).

## Équipement

On *tient* l'équipement dans la main (case sélectionnée). Bâtons (`skidstav`) et fart au goudron
(`skidtjara`) comptent dès qu'ils sont dans l'inventaire. Table (`glide::modifier`) :

| Tenu | Neige / tassée | Glace | Ailleurs |
| --- | --- | --- | --- |
| rien | `snow_slow` | × 1 | × 1 |
| skis (`skidor`) | × 1,6 (× 1,3 sans bâtons), glisse | × 1, glisse | × 0,6 |
| raquettes (`snoskor`) | × 1, pas de glisse | × 0,8 | × 0,9 |
| patins (`langfardsskridskor`) | × 0,4 / × 0,6 | × 2,5, glisse | × 0,5 |
| luge à pied (`spark`) | × 0,7 (tassée × 2) | × 2, glisse | × 0,7 |

La glisse (frottement bas : 4, 3 avec fart, 2,5 sur glace à skis, 2 en patins, 3 en luge) laisse
l'élan, et `slope_accel` accélère vers le bas de la pente (24 blocs/s² par bloc de dénivelé, plafond
32, vitesse maximale gagnée 14). Sur le bord d'une marche, les pieds comptent encore la neige de la
cellule voisine plus basse pour ne pas casser l'élan.

## Neige tassée

Une colonne de neige où l'on entre 3 fois (`TRAMPLE_STEPS`) voit sa couche remplacée par
`packad_sno` (couche de 2 huitièmes, non solide, bloc 159 de Norrcraft). Le bloc est une sortie de
la surcouche saisonnière (`SeasonBlocks::output`) : la colonne reste « naturelle » et fond au
redoux. La mémoire des passages tient 4096 colonnes.

## Véhicules

`Vehicle` (`Spark`, `Pulka`, `Ackja`, `Sleigh`, `IceYacht`) : coffres, bêtes d'attelage, charge sur
la glace (`ice_load` : traîneau vide `Sledge`, chargé `LoadedSledge`, qui veut de la glace épaisse
d'après `IceKind::carries` ; char à glace toujours chargé). `team_speed(vehicle, marche, bêtes,
charge, surface)` : 0,4 + 0,6 × bêtes/attelage complet, moins la moitié à pleine charge, × 1,4 sur
glace, × 1,3 sur neige tassée, ≤ × 0,9 en neige profonde, × 0,5 hors neige. `mount_speed(marche,
allure)` lit l'allure d'élevage de la 5.7 (tölt du cheval islandais). `ice_yacht_speed(vent, angle,
surface)` : à l'arrêt hors glace et face au vent (zone morte de 30°), sinon vent × (1 + 2 sin angle),
plafond 40 : le véhicule le plus rapide du jeu, au travers.

## Avalanches

Un départ (`can_start`) exige au moins 5 couches de neige (`MIN_SNOW`, 5 huitièmes) et un voisin plus
bas d'au moins 2 blocs de surface. `plan` suit le plus fort gradient (au moins 0,5 bloc de chute, 40
colonnes au plus), en bande de trois colonnes ; les 4 dernières colonnes sont le pied (étalement).
`apply` retire la neige des colonnes balayées et la répartit sur le pied (6 blocs au plus par
colonne). `sweep_entities` blesse (6 points), emporte au pied et enterre : poche d'air de deux
cellules sous un bouchon de `sno` en y + 2, composant `Buried`. Après 8 s de réserve d'air, 1 point
toutes les 1,5 s ; casser le bouchon (ou la neige au-dessus de soi) libère.

`AvalancheSystem` : un bloc non neigeux cassé (coup sourd) à moins de 8 cases d'un départ, une
chance sur deux ; `EcsEvent::Whoumf` prévient, la coulée part 2 s après (`EcsEvent::Avalanche`).
Seuls Fjord, Kalfjäll et Fjällhed (`slope_biome`) en connaissent, et seulement quand le manteau est
instable : neige en cours (`Environment::snow > 0,05`) ou air au-dessus du point de gel, pendant 30
minutes de jeu ensuite ; sans environnement ou sans biome (tests), la condition est levée.

## Objets et bloc

Objets purs ids 268 à 278 (textures provisoires de vanilla) : `skidor skidstav skidtjara snoskor
langfardsskridskor spark pulka ackja slade isjakt snoskyffel`. Bloc `packad_sno` (159).

## Écarts et reste à faire

* **Aucun geste ni recette** : la table de survie Norrcraft est vide (6.7) ; l'équipement se tient
  en main, il n'y a pas d'emplacement de chaussures ; pas de pelle câblée (`snoskyffel`).
* Pulka, ackja, traîneau et char à glace sont des fonctions et des objets : aucune entité
  véhicule n'existe encore, donc ni attelage au chien ou au cheval, ni monture au tölt (le
  contrôle d'une monture par le joueur reste à écrire), ni char à glace piloté ; le traîneau du
  chien est repoussé à l'arrivée des véhicules (6.3 pour les bateaux, qui partagent le contrôle).
* Pas de sons (« whoumf », coulée), ni de fissures visibles dans la neige ; pas de cor ni de
  mine pour déclencher (seul un bloc cassé fait du bruit) ; une tempête de neige ne déclenche
  rien seule.
* Les couches ne ralentissent que le joueur (pas les mobs) et restent non solides ; la neige
  ne s'enfonce pas visuellement. Les avalanches ne sont pas répliquées par le LAN.
