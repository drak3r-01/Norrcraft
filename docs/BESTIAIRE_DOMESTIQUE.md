# Bestiaire : animaux domestiques (session 5.7)

Doc 03 §3.6. Dix espèces (67 au total avec le lot 5.4 à 5.6), un bloc de données `husbandry`
sur le `MobConfig` et un système d'élevage. Les fiches sont générées par
`scripts/gen_norr_mobs.py` (boîtes colorées, comme les lots précédents).

## Espèces

| id | Animal | Rôle | Produit | Remarque |
| --- | --- | --- | --- | --- |
| `fjallko` | Vache de montagne | bétail | `mjolk` 6/jour, ×1,5 de juin à août (estive) | foin l'hiver |
| `get` | Chèvre | bétail | `mjolk` 3/jour, ×1,3 l'été | foin l'hiver |
| `gutefar` | Mouton de Gotland | bétail | `ull` 3 par toison, tonte d'avril à juin | une toison à la fois |
| `linderodssvin` | Porc | bétail | viande (pas de produit) | faînes (`agn`) |
| `hona` | Poule | volaille | `agg` 1/jour de février à octobre | grain |
| `tamgas` | Oie | volaille | `agg` 0,5/jour de mars à juillet | grain |
| `fjordhast` | Cheval de fjord | monture | | allure pas, secousse 0,4 |
| `islandshast` | Cheval islandais | monture | | **tölt** ×1,5, secousse 0,1, on peut manger et viser en selle |
| `jamthund` | Chien d'élan | chien | | arrêt et alerte |
| `skogskatt` | Chat des forêts | chat | | chasse les rongeurs |

Apparition : `persistence: "resident"`, faibles densités dans Hagmark, Bokskog, Strandäng,
Alvar (Fjällhed pour le cheval islandais). Le lien aux villages et aux fermes (`avoid.village`,
`nearFeature`) viendra avec les habitants (phase 6) ; d'ici là ce sont des bêtes libres et
rares que l'on apprivoise.

## Fiche d'élevage (`nb-data/src/husbandry.rs`)

`role` (livestock, poultry, mount, hound, ratter), `tameWith` (friandises), `follow` (distance
de suivi), `fodder` {item, perDay, ration, pasture}, `produce` {item, perDay, capacity, months,
summerBonus, bonusMonths, seasonal, harvestMonths}, `gait` {name, speed, jolt, steady},
`hound` {pointRange, alertRange, alerts}. Les noms d'objets sont des chaînes : les objets
arrivent avec les sessions 6.4 (cultures, fenaison) et 6.5 (laiterie, cuisine).

## Système (`nb-ecs/src/husbandry.rs`)

* `tame(world, mobs, animal, joueur, objet)` : l'objet doit être dans `tameWith` ; pose le
  composant `Domestic` (maître, réserve de fourrage 0,6, stock).
* `HusbandrySystem` (inséré après `HerdSystem`) : à 1 Hz, **fourrage** (pendant la saison de
  pâture la réserve monte de `5 × perDay` par jour de jeu, hors saison elle baisse de `perDay` ;
  à sec l'animal perd 0,1 PV/s : « sans foin, il dépérit »), **production** (si la réserve
  dépasse 0,25, dans la fenêtre de mois, au prorata de la réserve, plafonnée à `capacity`),
  **suivi** (compagnons, chat, montures : au-delà de 1,5 × `follow` du maître, `Brain::goal`
  pointe sur lui ; l'état Migrer, désormais déclenché aussi par `goal`, les y mène).
* À chaque tick, pour le chien : **arrêt** si du gibier (`hunts` du profil) est senti à moins de
  `pointRange` : il se fige face à lui et aboie toutes les 4 s (`AnimalAct::Bark`), sans
  l'attaquer (le profil `hund` n'offre ni Traquer, ni Poursuivre, ni Attaquer) ; **alerte** si
  un prédateur de `alerts` (id ou profil) est senti à moins de `alertRange`.
* `feed(...)` rend une ration (`ration`) si l'objet est le fourrage ; `collect(..., mois)`
  récolte `floor(stock)` unités (une production `seasonal` n'est prête que toison pleine et
  dans `harvestMonths`).

Profils ajoutés dans `behaviors.json` : `husdjur`, `fjaderfa`, `hund`, `katt`.

## Reports

* Pas encore de geste de jeu : clic droit pour apprivoiser, donner du foin, traire, tondre,
  ramasser les œufs, seller (les fonctions `tame`/`feed`/`collect` attendent leur branchement
  sur les objets de la phase 6) ; la monture (tölt) et le traîneau du chien attendent la
  session 6.2.
* Enfermer la volaille la nuit, enclos et fuite de la chèvre : avec les bâtiments habités.
* Le chat chasse déjà les rongeurs (profil `katt`) mais le cellier n'existe pas (6.5).
* Les produits n'existent pas encore comme objets.

## Tests

`nb-data/tests/norr_domestic.rs` (cohérence des fiches), `nb-ecs/tests/bestiary_domestic.rs`
(10 scénarios : friandise, hiver et dépérissement, pâture, lait d'estive, tonte, œufs, chien
qui suit, qui marque l'arrêt, qui aboie sur un loup).
