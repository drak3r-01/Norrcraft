# Surcouche saisonnière (session 3.2)

Plan Norrcraft, phase 3 « temps » ; GDD doc 04 §2.3. Code : `nb-world/src/seasons.rs` (modèle pur,
testé), `nb-world/src/norrcraft/seasonal.rs` (pose sur un chunk), `NorrcraftTerrain`
(`overlay_season`, `reseason`), `ChunkManager::set_season` (tâche de fond `nb-season`),
`nb-mesh` (dalles de neige, teintes de saison), `nb-game/src/game/calendar.rs` (horloge).
Règles réglables : `assets/norrcraft/seasons.json`. Seuls les univers qui ont le calendrier
(`Systems::seasons`, Norrcraft) en profitent ; vanilla et le terrain sans pas de saison sont
inchangés, octet pour octet.

## Principe

La génération produit l'état de **fin d'été** (inchangé). Un pas de saison pose par-dessus la
neige, la glace, les feuillages et la flore. Huit pas par an, le pas `s` étant centré sur la
fraction d'année `(s + 0,5) / 8` (0 = solstice d'hiver) :

| Pas | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Époque | fin janvier | mars | mai | juin | août | septembre | octobre | décembre |

Le pas est calculé chaque tick par le jeu d'après la fraction d'année (`step_of`). Quand il
change, `ChunkManager::set_season` règle l'état partagé du générateur et met en file tous les
chunks actifs ; un fil de fond (`nb-season`) recalcule chacun sur une copie puis le remplace si
rien n'y a été écrit entre-temps. Le maillage de tout chunk est refait (les teintes changent même
quand les blocs ne changent pas). Les chunks générés ensuite, et ceux relus du disque, reçoivent
la surcouche du pas courant avant d'apparaître.

## Colonnes naturelles

`NorrcraftTerrain::reseason` régénère le chunk de fin d'été, y pose la surcouche, puis compare
colonne par colonne avec le contenu actuel : une colonne reste **naturelle** (donc réécrite) si
chaque cellule est identique à la base, ou remplace par une sortie de la surcouche (air, couche
de neige, neige, glace, ramille, rameau) une cellule remplaçable de la base (air, eau, plante,
feuille de feuillu). Toute autre colonne (bloc posé, creusé, plante arrachée…) est laissée telle
quelle : c'est le drapeau par colonne du doc, calculé au lieu d'être stocké. Limite : un toit posé
par le joueur n'est pas enneigé, ni le sol qu'il couvre.

## Modèle (`seasons.rs`)

* Température de saison : `T(a) = T_moy − A(λ) · cos(2π (a − 0,07))`, `A = 0,20 + 0,15 λ`,
  `λ = 1 − T₀` ; `T_moy = T₀ − 0,006 · max(0, y − S) + 0,05 (O − 0,5)` (doc 02 §4).
* **Neige en couches** (1/8 de bloc). Intégration sur deux années de 64 sous-pas : la neige gagne
  `140 · (0,34 − T) · (0,6 + 0,8 · Hm)` couches par année tant que `T < 0,34`, et fond de
  `520 · (T − 0,34)` au-dessus ; plafond 16. Un climat qui ne dépasse jamais le seuil (sommets)
  garde la neige toute l'année. Exemples (couches aux pas 0..7) : T₀ 0,45 : 3 / 6 / 4 / 0 / 0 / 0
  / 0 / 0 ; T₀ 0,35 : 6 / 11 / 13 / 8 / 0 / 0 / 0 / 2 ; T₀ 0,60 et plus : jamais.
* Pose : 8 couches ou plus = bloc `sno` plein, le reste = `snotacke_1` à `snotacke_7` (dalle de
  n/8, non solide, transparente, `height` dans `blocks.json`). Sous conifère × 0,5 ; congère
  de +2 couches à l'abri d'une crête (sol amont plus haut de 2 blocs, vent d'ouest provisoire
  jusqu'à la 3.3). Une plante n'est enfouie qu'à partir de deux couches. Au-delà de 24 couches la
  pile s'arrête.
* **Glace** (rang `e` de 0 à 3). Épaisseur `h` : `+12 · (seuil − T)` sous le seuil de l'eau (lac
  0,30, golfe saumâtre 0,26, mer 0,18), `−40 · (T − seuil)` au-dessus ; `e = 1` dès 0,05, 2 dès
  0,20, 3 dès 0,45. À la débâcle (l'épaisseur baisse) `e` est plafonné à 1. Blocs : `nyis` (e = 1),
  `is` (2), `tjockis` (3) pour les lacs, `havsis` pour la mer et le Bottenhav. Seule la cellule du
  dessus de l'eau gèle, jamais les rivières (les rapides) ; la banquise (Packis) l'est déjà. Le
  rang est une fonction pure : la glace jouable (6.1) l'appellera plutôt que de relire les blocs.
* **Feuillus** (blocs `seasonal` dans `blocks.json` : bouleau, hêtre, chêne, tremble, aulne,
  sorbier, saule, noisetier). Part de feuilles gardée : `lisse(T)` entre 0,32 et 0,52 à
  l'automne, 0,30 et 0,44 au printemps. Ce qui tombe devient, une fois sur deux, une ramille nue
  (`kvistar`, en croix) ou rien, tirage par hachage de la cellule. Les conifères ne changent pas.
  Teintes de saison (mailleur) : `BiomeSource::leaf_season` et `grass_season` glissent la teinte
  du biome vers une cible (vert tendre en mai, jaune en septembre, or et rouge en octobre, brun
  l'hiver ; herbe jaunie et ternie) ; seuls les blocs `seasonal` suivent pour les feuilles.
* **Flore saisonnière** (`seasons.json`) : anémones (mai), baies mûres (`blabar` août à
  septembre…), champignons (août à octobre), fleurs d'été ; hors saison la plante devient de l'air
  ou un rameau sans fruits (`ris`). Les moustiques viendront avec la météo et les mobs.
* Structures d'hiver (cabanes de pêche, pistes damées) : non faites, elles viennent avec 6.1.

## Blocs et textures

`snotacke_1..7` (117 à 123), `nyis`, `is`, `tjockis` (124 à 126), `kvistar`, `ris` (127, 128).
Textures provisoires `scripts/gen_norr_seasons.py` (python pur, `rust:1`). Les mondes existants
reçoivent ces blocs à des identifiants libres (palette par monde).

## Écarts et reste à faire

* Pas de moustiques ; pas de pistes damées ni de cabanes de pêche ; le doc donne 4 hauteurs de
  neige, il y en a 7 (plus fin avec le mailleur partiel).
* Les couches de neige ne ralentissent pas encore le joueur (6.2) et ne sont pas solides.
* En LAN, chaque pair applique la surcouche localement d'après la date de l'hôte ; un chunk
  modifié envoyé par l'hôte garde sa version.
* Coût : environ 1,5 ms par chunk avec la surcouche (1,4 ms sans) ; `reseason` environ 1,2 ms.
* La neige tombe selon le pas, pas selon la météo (3.3) : elle apparaît au changement de pas.
* Pas d'effet de la saison sur les plantes cultivées (doc 04 §2.4, session 6.x).
