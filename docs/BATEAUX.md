# Bateaux, voile et vent, morue d'hiver (session 6.3)

Plan Norrcraft, phase 6 « mécaniques nordiques » ; GDD doc 04 §5.1 (bateaux) et §5.2 (ce que le
fjord offre). Code : `nb-ecs/src/boat.rs` (bateaux, voile, houle, coque, claies) ; tests :
`nb-ecs/tests/boat.rs`.

## Bateaux

| Bateau (objet) | Équipage | Coffres | Propulsion | Vitesse max |
| --- | --- | --- | --- | --- |
| Radeau (`flotte`) | 1 | 0 | perche 1,6 | 1,6 |
| Eka (`eka`) | 2 | 1 | rames 3,2 | 3,4 |
| Snipa (`snipa`) | 3 | 3 | voile aurique 0,8 et rames 3,0 | 8 |
| Knarr (`knarr`) | 6 | 8, bétail | voile carrée 0,7, pas de rames | 7,5 |
| Långskepp (`langskepp`) | 8 | 2 | voile carrée 1,0 et rames 4,5 | 11 |

Un bateau est une entité : `Transform` (cap = `yaw` en degrés, avant `(sin, −cos)` comme la
caméra) et composant `Boat` (vitesse, voile, mouillage, au sec, goudron, eau, charge, pilote).
Le joueur embarqué porte `Aboard(bateau)` et est posé sur le pont chaque tick par `BoatSystem`
(inséré après la saisie du joueur, Norrcraft seulement) ; son regard reste libre.

Commandes à bord : avancer / reculer = rames (ou perche), gauche / droite = barre (le bateau
tourne mieux en avançant), sprint = établir ou ferler la voile, clic droit = mouillage, saut =
écoper, accroupi = débarquer (sur la terre voisine, sinon à l'eau). Hors bateau, clic droit à moins
de 2,5 blocs d'un bateau sans pilote embarque. `/boat <raft|eka|snipa|knarr|longship> [bare]`
(triches) met un bateau à l'eau près du joueur et l'y embarque.

Les objets `arar`, `ostkar`, `ankare`, `linsegel`, `djuplina` sont des objets purs prêts pour les
recettes (6.7) ; les gestes qui les consomment ne sont pas câblés.

## Voile et vent

`sail_efficiency(voile, angle)` avec `angle` entre le cap et la provenance du vent (0 : vent
debout, π : vent arrière) : nulle dans l'angle mort (45° aurique, 70° carrée), lissée jusqu'au
largue (120° aurique, 150° carrée), puis 0,75 vent arrière. `sail_speed` = vent × 12 blocs/s ×
rendement × puissance, plafonné. Le vent est celui de `Environment` (dont les brises de fjord de
la 3.3 : la remontée ou la descente d'un fjord est donc rapide ou lente selon l'heure). Sans
météo (monde sans environnement), calme plat. Charge : −40 % à pleine charge ; eau dans la
coque : jusqu'à −50 %. Un bateau sans pilote ni ancre dérive sous le vent (25 % du vent).

## Houle, coque et abri

`swell(vent, fetch)` : le vent multiplié par la longueur d'eau libre qu'il a parcourue en amont
(`fetch`, jusqu'à 64 blocs, saturation à 48) : **aucune houle au fond d'un fjord**, mouillage sûr
pendant les tempêtes. Eau dans la coque (0 à 1) : fuite de 0,012 /s nue (coule en 80 s sans
écoper), 0,001 /s goudronnée ; houle × exposition × 0,012 /s ; écoper retire 0,06 /s ; le radeau
ne prend pas l'eau ; un choc à plus de 3 blocs/s abîme. `EcsEvent::BoatSwamped` à 70 %,
`BoatSank` à 1 (le bateau disparaît, le pilote est à l'eau).

## Hiver

Une coque dont deux voisins au moins sont pris dans la glace est écrasée (0,015 /s). Radeau, eka
et snipa se tirent au sec : avancer doucement contre la grève pendant 0,6 s les échoue (`ashore`,
plus de fuite, halage à 1,2 blocs/s, remise à l'eau en avançant vers l'eau) ; knarr et långskepp
butent sur la rive et restent à quai.

## Morue d'hiver et stockfisch

`bottom_fishing(profondeur, mois, tirage)` : skrei de février à avril à 6 blocs de fond au moins,
flétan à 14 blocs (toute l'année), requin du Groenland à 26 (rare). Claie (`Rack`, 12 poissons,
`RackState` partagé via `WorldSession::racks()`, `RackSystem`) : le séchage avance par temps
froid, sec et venté (`drying_weather` : air sous le point de gel + 0,12, ni pluie ni neige ni
brouillard, vent ≥ 0,15), 4 jours de jeu ; le temps doux fait pourrir un poisson pas encore sec
en 1,5 jour ; un poisson sec ne pourrit plus. `Rack::take` rend `(stockfisk, pourris)`.
`NEVER_SPOILS = ["stockfisk"]` est la liste que la conservation (6.5) devra lire.

## Écarts et reste à faire

* **Aucun rendu** : les bateaux n'ont pas de modèle ni de bloc visible (le joueur glisse sur
  l'eau) ; pas de sons ; pas de LAN (un seul pilote, l'équipage n'est qu'une donnée).
* **Aucun geste ni recette** : la table de survie Norrcraft reste vide (6.7). Pas de bloc `hjell`
  posable (les claies sont créées par l'API), pas de ligne de fond câblée, pas de coffres
  embarqués ni de bétail à bord (données seulement), pas de sauvegarde des bateaux.
* Non fait : falaises à oiseaux (rappel), vêlage des glaciers, cascades et vallées suspendues
  (roues à eau en 6.6), chantier naval du knarr, haut fait du långskepp, tangage à l'écran.
* Monture au tölt et traîneau du chien (6.2) attendent toujours leur contrôle ; `BoatSystem`
  est le premier véhicule piloté et peut servir de modèle (`Aboard`).
