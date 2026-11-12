# Bestiaire : interactions avec le monde, chasse et piégeage (session 5.8)

Dernière session de la phase 5 (doc 03 §4.5 et §5). Le bloc `"ecology"` d'un `MobConfig`
(`nb-data/src/ecology.rs`) dit ce qu'une espèce fait au monde et ce que le joueur peut lui faire ;
`nb-ecs/src/ecology.rs` (`EcologySystem`) le fait vivre. 34 espèces portent une fiche
(`scripts/gen_norr_mobs.py`, table `ECOLOGY`). Le système est inséré par `WorldSession::new`
après `Husbandry` et **avant** `Health` (qui détruit les morts) ; `WorldSession::ecology()` donne la
poignée partagée (traces, pièges, colonies).

## Ce que la faune fait au monde

| Interaction | Fiche | Comportement |
| --- | --- | --- |
| Broutage | `browse {eats, months, fences, fenceRadius}` | Élan, chevreuil, renne à l'état *brouter* : 25 %/s de manger une pousse voisine (`blabar lingon ris ljung`). Toute plante à moins de `fenceRadius` (2) d'un `staket` est protégée (nouveau bloc 158, clôture de perches). |
| Castor | `fell {everySecs, cap, reach, spare, dam}` | À l'état *construire* près de sa hutte, la colonie abat l'arbre vivant le plus proche (`*_stam` hors `dod_stam`, `mossig_stam`) toutes les `everySecs` (1800 s de jeu), plafond `cap` = 6 par colonie. L'arbre tombe en entier (troncs connexes, 64 au plus). |
| Digue | `fell.dam = ["stock"]` | `ecology::dam_broken` (appelée par `WorldSession::tick` sur chaque `BlockBroken`) retire l'eau amont au-dessus du niveau aval (ou du fond de la brèche si l'aval est sec) : l'étang se vide. |
| Venin | `venom {damagePerSec, seconds, chance, cures}` | La vipère (`huggorm`) inocule `Poisoned` à sa cible (0,5 PV/s pendant 20 s, 60 % des morsures) ; `ecology::cure` guérit avec `groblad` ou `serum`, sinon le venin passe seul. |

Le troll pétrifié (« bloc erratique ») n'est pas fait : le troll appartient à la phase folklore
(doc 03 §3.7), il écrira son décor avec le mécanisme `Build` de la phase 4.

## Pistes (doc 03 §5)

`tracks {print, ttl, stride, droppings, debark}` : un animal qui marche sur la **neige**
(`snotacke_*`, `snow`) laisse une empreinte tous les `stride` blocs (`Sign` de type `Print`,
cap de la marche en radians). À chaque repas il peut laisser des crottes (`Dropping`), et élan
et ours écorcent les troncs voisins (`Debarked`). `EcologyState::read_sign(pos, portée)` donne
l'espèce, la forme de l'empreinte (`tass klov fagel orm`), le cap et l'âge de l'indice le plus
proche ; `signs_near` les liste pour le rendu. Les indices s'effacent après `ttl` secondes de
jeu (élan 1800 s, ours 3600 s, vipère 600 s) et le registre est plafonné à 4096. Les décalcomanies
elles-mêmes ne sont pas dessinées (rendu hors du périmètre de la session).

## Piégeage

`EcologyState::place_trap(kind, pos, baited)` pose un collet (`snare`), une nasse (`fishTrap`),
un filet (`net`) ou une fosse (`pit`). Chaque fiche liste ce que chaque piège prend et à quelle
cadence (`catch: [{trap, chance}]`, probabilité par minute, ×2 appâté) : lièvre, lagopèdes et
tétras au collet ; élan, renne, chevreuil à la fosse ; poissons et écrevisses à la nasse et au
filet ; oiseaux d'eau au filet. Portées : collet 1,6, fosse 1,0, nasse 2,5, filet 3,5 blocs.
La prise retire l'animal du monde, le piège se désarme ; `take_catch` relève l'espèce et le
réarme.

## Dépeçage et butin

Un animal mort dont la fiche a un `loot` laisse une **carcasse** (`Carcass`, `Emitter` charogne
de 24 blocs pour les charognards) qui se perd après `carcass` secondes (600). `ecology::skin(
world, mobs, carcasse, outil, mois, rng)` renvoie les morceaux : à mains nues seulement la
viande (moitié), au couteau les peaux, os et bois (×1,5). Le pelage d'hiver (`vinterskinn`, mois
10 à 3) et celui d'été (`skinn`, mois 4 à 9) s'excluent. Les objets sont des noms (`kott skinn
vinterskinn horn ben tand fjader fisk spack pals`) : ils attendent les objets de la phase 6.

## Naturfrid (doc 03 §5)

`naturfrid {worth, breeding, sacred}` + composant `Standing` sur le joueur (0 neutre, −1 fléau) :

* tuer plus de 3 bêtes d'une espèce dans un jour de jeu : −0,03 × `worth` par bête de trop ;
* tuer pendant la mise bas (`breeding`, mai-juin pour les cervidés) : −0,12 × `worth` ;
* tuer un animal sacré (ours brun, baleine, cygne, grue) : −0,4 ;
* laisser pourrir une carcasse sans la dépecer : −0,04 × `worth` (le dépeçage évite la faute) ;
* la réputation remonte de 0,03 par jour de jeu.

La densité du gibier sauvage (espèces `ephemeral`) est multipliée par `1 + 0,7 × score` (plancher
0,3) : `density::Moment::nature`, `ecology::nature_factor`. `Standing::mood()` (0 à 1) est
l'humeur du folklore pour la phase 7. Le coup mortel garde son auteur (`Brain::provoker`, ajouté à
`hit_mob_damage`) ; un animal tué par un prédateur ne coûte rien.

## Le cercle du bœuf musqué

Nouvel état `circle` (`StateKind::Circle`, 25 états) offert par le seul profil `myskdjur`. Dès
qu'un prédateur de meute ou de chasse à l'affût (`flockjagare`, `allatare`) ou le joueur est à
moins de 21 blocs et que le troupeau compte au moins 3 têtes, tous les adultes se rangent en
couronne de 2,6 blocs autour du doyen (id le plus petit), face à la menace, et donnent un coup de
corne à qui s'approche. Un bœuf seul fuit ou charge comme avant. Poids 1,6 : le cercle bat la charge
et la menace.

## Reste

* Vol dans les conteneurs ouverts (séchoirs, garde-manger, poulailler) : il n'y a pas encore de
  conteneurs (phase 6.5) ; l'état `steal` du moteur attend ses cibles.
* Parade, chants et appels des oiseaux : la parade (`display`) existe, pas les sons.
* Décalcomanies d'empreintes, crottes et écorçage visibles ; troll pétrifié (phase folklore) ;
  gestes de jeu (poser un collet, dépecer au couteau, relever une nasse) : ils attendent les
  objets de la phase 6 ; les API sont prêtes (`place_trap`, `take_catch`, `skin`, `cure`).
* L'écologie n'est pas répliquée en LAN (comme les hardes).

Tests : `nb-data/tests/norr_ecology.rs` (9), `nb-ecs/tests/ecology.rs` (20).
