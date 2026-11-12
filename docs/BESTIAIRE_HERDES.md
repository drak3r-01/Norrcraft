# Bestiaire : hardes persistantes et migrations (session 5.3)

Doc 03 §4.4. Les espèces à `"persistence": "herd"` ne sont plus tirées par la densité (5.2) : elles
vivent en **hardes**, objets légers du monde, une par espèce et par cellule.

## Modèle (`nb-world/src/herds.rs`, pur)

* **Position** : fonction pure de la graine, de la cellule et de la fraction d'année. Elle suit le
  calendrier sans rien simuler, chunks chargés ou non.
* **Effectif** : seul état (`HerdRecord` : espèce, cellule, effectif, instant). Les pertes le
  baissent, il remonte de `regrowPerDay` par jour de jeu jusqu'à l'effectif plein (tiré par
  cellule entre `sizeMin` et `sizeMax`). Sauvé dans `world.json` (`herds`, absent tant qu'aucune
  harde n'a été vue ; recopié par le clonage d'un monde).
* **Sites** : trouvés une fois par cellule en échantillonnant les biomes sur une grille 8 × 8
  (`summer` / `winter` = listes de noms de biomes) ; 2 cellules neuves au plus par seconde.

Trois mouvements (`herd.motion`) :

| Mouvement | Comportement | Exemples |
| --- | --- | --- |
| `seasonal` | transhumance site d'hiver ↔ site d'été au mois `upMonth` (défaut mai) et `downMonth` (octobre), sur ≈ 22 jours, par une route qui s'écarte de la droite (ventre de 15 %) ; un seul site possible : sédentaire | renne, harengs |
| `territory` | territoire fixe (biomes `summer`) parcouru lentement | meute de loups |
| `pass` | n'existe que pendant `passMonths` ; traverse la cellule du site `winter` au site `summer`, sinon en ligne droite selon `heading` | grues, oies, remontée des saumons |

Un va-et-vient de rayon `wander` autour de la position calendaire s'ajoute à tous.

## Matérialisation (`nb-ecs/src/herd.rs`, `HerdSystem`, 1 Hz)

1. Cellules examinées : voisinage 3 × 3 de chaque joueur, plus les hardes déjà matérialisées.
2. Un joueur à moins de `materialize` blocs (défaut 96) du centre, centre hors de son cône de vue,
   chunk chargé : la harde devient `effectif` individus (`HerdMember`) autour du centre ; ceux qui
   n'ont pas trouvé de place comptent quand même et retentent leur chance.
3. Tant qu'elle est matérialisée : l'effectif enregistré = individus vivants (un mort est une
   perte) et un individu qui s'écarte de plus de 18 + 3√n blocs reçoit `Brain::goal` = la position
   calendaire (suivie quand il est à l'état `Migrate`).
4. Tous les individus à plus de `materialize` + 32 blocs de tous les joueurs, ou harde absente
   (fin de fenêtre de passage) : survivants réabsorbés (détruits, effectif conservé).

`MobSpawnSystem` ne supprime pas les membres d'une harde (chunk déchargé) et la densité ne les tire
pas. `maxCount` 0 désactive l'espèce ; `maxCount` n'est pas un plafond de harde.

## Règles JSON

```json
"spawn": { "persistence": "herd",
  "herd": { "motion": "seasonal", "cell": 1024, "sizeMin": 8, "sizeMax": 20,
            "regrowPerDay": 0.5, "summer": ["FJALLHED"], "winter": ["TALLHED"],
            "upMonth": 4, "downMonth": 9, "wander": 24, "materialize": 96 } }
```

`persistence: "herd"` exige le bloc `herd` et inversement ; `densityPerBiome` devient facultatif
pour une harde. Validation : cellule 256 à 8192, effectifs cohérents, mois 0 à 11, au moins un
biome de site (pas de passage sans cap ni site).

## Écarts et restes

* Aucune fiche d'espèce n'utilise encore `herd` (renne, loup, grue, oie, hareng, saumon : 5.4+).
* Le saumon « de bief en bief » est un passage entre deux sites de biome dans la cellule, pas un
  front qui suit le graphe des rivières ; les cellules se suivent sans coordination.
* La route de transhumance ignore le relief (pas de vallées réelles) ; les membres s'y rendent par
  le moteur d'IA, sans évitement spécifique.
* Les hardes ne sont pas répliquées vers les invités LAN autrement que comme des mobs ordinaires.
* Ni naturfrid ni chasse (5.8) ne touchent encore l'effectif autrement que par la mort d'un individu.

## Tests

`cargo test -p nb-world --lib herds` (9 : saisons, mai et octobre, pureté, repousse, passage,
tour JSON), `-p nb-world --lib save`, `-p nb-data --lib spawn_rules`, `-p nb-ecs --test herds`
(9 : matérialisation, éloignement, pertes conservées, repousse, hiver en forêt, destination,
sauvegarde, déterminisme).
