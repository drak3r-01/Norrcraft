# Worldgen Norrcraft : champs (1.1), relief (1.2), formes de relief (1.3) et hydrographie (1.4)

Le vrai relief Norrcraft (GDD doc 02 §3 à §6 et §12 ; plan 06, sessions 1.1 à 1.4). La 1.1 a posé
les briques mathématiques et les champs lents ; la 1.2 construit dessus la fonction de hauteur et
remplace le monde plat de granit de la phase 0 par un vrai terrain ; la 1.3 y pose archipel,
côtes, eskers, tourbières, glaciers et champs de lave ; la 1.4 y met l'eau (mer, lacs, rivières),
étanche par construction. Tout vit dans `crates/nb-world/src/norrcraft/`.

## Modules

| Fichier | Contenu |
|---|---|
| `math.rs` | hachage SplitMix64 (`hash64`, `hash01`), utilitaires (`sigmoid`, `spread` = q, `smooth` = sm, `bump`, `soft_cap`), `GradField` (N, fBm, crête `ridged`, ligne de zéro, `warp`), `Worley` (F1, F2, cellule, point) |
| `config.rs` | `NorrConfig` (préréglage, niveau de mer 40, hauteur 128 : décision D3), `NorrPreset` (Norrland, Skärgård, Högfjäll) |
| `fields.rs` | `MacroFields::sample(x, z)` → `MacroSample { c, o, t0, hm, u, e, lk, gt, vr, theta }` ; `province(x, z)` ; `ChunkFields` (treillis 4 × 4) |
| `climate.rs` | `ClimateZone` (arctique … némorale), `altitudes` (limites des arbres, de la roche nue, des neiges), `temperature_at` |
| `height.rs` (1.2) | `HeightFields` : `column(wx, wz)` (treillis, identique aux chunks), `relief_exact(x, z)` (cartes), `relief(x, z, &MacroSample)` → `Relief { h, socle, land, chain, before_valleys, plateau, plateau_level, valley, valley_floor, hanging, fjord, forms }` (`forms` : 1.3) ; `socle(C)`, `soft_floor` |
| `landforms.rs` (1.3) | `Landforms::apply` : archipel, côtes, eskers, marmites, drumlins, tourbières, palses, champ de lave, glacier → `Forms` (dans `Relief::forms`), `Coast` |
| `terrain.rs` (1.2 à 1.4) | `NorrcraftTerrain` (`TerrainGenerator`) et `NorrBlocks` (blocs des formes de la 1.3, glace posée au-dessus du sol et de l'eau) ; `WorldProfile::norrcraft(graine, NorrConfig, &registre)`, utilisé par `World::for_kind` |
| `hydro.rs` (1.4) | `Hydrography` (relief + eau : `column(wx, wz)` scellée, identique aux chunks ; `raw_column`, `column_exact` pour les cartes ; `lakes(rect)`), `Hydrology` (règles), `WaterColumn { top, level, kind, stair, channel_distance, river_width, dam }`, `WaterKind` (sèche, mer, lac, rivière), `SeaFacade`, `Lake`, `LakeSet`, `seal`, `filtered_height` (Hl), `stair_level` (W) |

Règles communes (doc 02 §2) : fonctions pures de (graine, config, x, z), **aucune fonction
transcendante** (un test refuse `sin`, `exp`, `ln`, `powf`, `powi`, `mul_add`… dans tout le dossier),
donc le même résultat bit à bit sous Linux et Windows. Chaque champ a sa propre instance de bruit,
semée par `graine XOR constante` (constantes figées dans `fields.rs`, module `seeds`), et son
propre décalage d'origine.

## Les champs

| Champ | Sens | Évalué sur |
|---|---|---|
| `c` | continentalité (0 = haute mer, 1 = cœur ; côte à C ≈ 0,465, `COAST_C`) | p′, + biais d'océanité |
| `o` | océanité (1 = façade atlantique, 0 = baltique) | p |
| `t0` | température au niveau de la mer | p′ |
| `hm` | humidité (+ 0,15 · O, bornée à 1) | p′ |
| `u` | surrection (renforcée près de la façade atlantique) | p′ |
| `e` | érosion (1 = plat) | p′ |
| `lk` | densité de lacs | p |
| `gt` | géothermie, seulement au large (C < 0,5) | p |
| `vr` | variante locale | p |
| `theta` | sens d'écoulement des glaces (vecteur unitaire) | Worley de pas 3000 |
| province | nature du socle : `Province` (granit gris 50 %, gneiss 20 %, granit rouge, calcaire au sud-est, grès, schiste, grönsten, porphyre) | Worley de pas 1400 sur p′ |

`p′ = warp(p; 120, 700)`. Pour un chunk, `ChunkFields::new(&fields, cx, cz)` évalue 25 nœuds espacés
de 4 blocs (ancrés sur des multiples de 4 en coordonnées monde, donc partagés entre chunks
voisins) et `at(lx, lz)` interpole bilinéairement : 10 fois moins d'évaluations, erreur maximale
mesurée sous 0,05 sur chaque champ.

## Écarts et précisions par rapport au doc 02

Le doc laisse quelques points libres ; voici ce que 1.1 a tranché (tout est dans `fields.rs`).

* **Biais d'océanité sur C** (« plus biais d'océanité », §4) : spline en x à nœuds
  (−9000 : −0,40 ; −6500 : −0,15 ; −4500 : +0,12 ; −2000 et +3000 : +0,23 ; +6000 : +0,08 ;
  +9500 : −0,02), constante au-delà (le monde reste infini). Le noyau surélevé évite que le
  centre soit criblé de mers intérieures. Réglé en 1.2 sur le relief réel (h ≥ S) : 69,6 / 70,6 /
  72,6 % de terre sur 16 km pour les graines 12345 / −98765 / 20261111 (cible doc §7.5 : 70 %) ;
  les nœuds de la 1.1 (+0,17 au centre, golfe dès 3500) n'en donnaient que 60 %.
* **Golfe oriental peu profond** : à l'est, C est de plus tiré vers 0,42 (plateau continental,
  force 0,55 × poids lissé entre x = 4500 et 9000). La mer reste peu profonde, des îles émergent
  là où le bruit est haut : c'est la base de Bottenhav et de Skärgård.
* **Gt** : le « et C < 0,5 » du doc est un facteur lisse `sm(0,50 ; 0,42 ; C)` (pas de coupure
  franche).
* **θ** : moyenne pondérée des vecteurs des cellules voisines (poids qui s'annule continûment à
  0,25 pas de distance) ; seule zone de rotation rapide : là où deux flux opposés se compensent.
* **Crête** (`ridged`) : normalisée par Σ gⁱ, donc dans [0, 1] (le doc écrit `22 + 50 · Rg`).
* **fBm** : chaque octave est décalée (`31,7 · i`, `17,3 · i`) pour que les octaves ne
  s'annulent pas toutes aux mêmes nœuds. Le bruit de base est `noise::Noise` (période 256 en
  unités de bruit : 12 288 blocs pour p/48, sans effet visible sur les champs lents ; à
  surveiller pour les détails fins de la 1.2, p/11).
* **Province** : le calcaire est pondéré vers le sud-est (`0,2 + 1,6 · sm(−4000, 4000, x + z)`) ;
  parts mesurées proches du doc (granit gris 48 %, gneiss 20 %, calcaire 11 % sur 16 km).
* Non fait en 1.1 (dépend du relief) : W (niveau d'eau en escalier), hydrographie, sites de
  structures sur la carte, treillis de `Pv` (évalué exactement).

## Relief (session 1.2, doc 02 §5.1 à §5.4)

`h(x, z)` suit l'ordre du doc : socle `B(C)` (spline de §5.1 lissée par `sm` entre les nœuds,
plus le décalage du préréglage, Skärgård −5), collines `Hc` et détail `Dt` (§5.2), chaîne `Mt` sur
crêtes déformées (`pm = warp(p; 90, 420)`, `Rg(pm/1500; 5)`), plateaux en paliers, plafond doux
à S + 62 (asymptote S + 84 = 124), puis vallées suspendues et vallées principales en U (§5.4).
Les champs lents (C, U, E) sont lus par le treillis 4 × 4 ; le reste est calculé à la colonne.
Chaque bruit du relief a sa graine (`height.rs`, module `seeds`, `0x4E52_5743_0000_01xx`).

Le générateur (`terrain.rs`) remplit : socle (bedrock) en y = 0, granit, puis une couverture
provisoire en attendant les biomes (2.1) : `grus` (gravier, 2 blocs) sous la mer et sur les grèves
(surface ≤ S + 1), `moran` (moraine, 3 blocs moins la pente) sur les terres basses (< S + 50) en
pente douce (≤ 2 blocs avec les 4 voisines), granit nu ailleurs (parois, altitude). Eau de mer de
la surface jusqu'à S − 1. Le relief d'un chunk est calculé sur 18 × 18 colonnes (bordure comprise,
pour les pentes) avec un treillis commun (`FieldGrid`) ; `surface_height(wx, wz)` utilise
`MacroFields::sample_lattice`, bit à bit identique, donc la hauteur isolée (départ, réapparition)
coïncide toujours avec le chunk. Le départ est cherché au-dessus de S = 40 (le niveau de la mer de
la configuration vanilla, encore partagée, est ignoré). `climate_at` renvoie T₀ et Hm.
Coût mesuré : 0,18 ms par chunk en `--release` (budget du doc : 2,5 ms).

Écarts et précisions par rapport au doc :

* **Paliers adoucis** : les `plancher(·)` du doc (paliers de plateau ℓ, marches de 6 blocs du fond
  des vallées intérieures) passent par `soft_floor(v, 0,3)` : plat sur 70 % de chaque marche, montée
  lisse sur les 30 % restants. Le relief reste continu (doc §5 : « chaque étape est continue ») et
  aucune marche n'apparaît hors des parois d'auge (test du doc §13). Les marches franches de l'eau
  (W, §6.1) viendront avec l'hydrographie (1.4).
* **Masque de plateau** : `P = sm(0,55 ; 0,70 ; q(F(p/700; 2))) · U` avec l'étalement q, comme
  les autres champs ; sans q le fBm brut ne dépasse presque jamais 0,55 et les plateaux
  n'existaient pas (0,1 % de la carte). ℓ utilise aussi `q(F(p/1600; 2))` pour que les trois
  paliers (S + 44, S + 56, S + 68) apparaissent.
* **Fond visé f** : le doc donne deux formules (intérieur, fjord) qui ne se raccordent pas en
  C = 0,56 (S + 2 contre S − 6) ; elles sont mélangées par `sm(0,54 ; 0,60 ; C)`.
* **Fjords au large** : le poids `g = sm(0,25 ; 0,50 ; U)` est multiplié par `sm(0,40 ; 0,44 ; C)`
  pour que le creusement s'éteigne au-delà du seuil (§5.4) : le fjord débouche sur le plateau
  continental au lieu d'y prolonger une fosse à S − 30.
* **Vallées suspendues** : `Z(pv/450)` sur les mêmes coordonnées déformées que la vallée
  principale, même demi-largeur ω en unités de bruit (donc des auges ~2,4 fois plus étroites en
  blocs), fond à f + 18. Elles sont creusées avant la vallée principale, si bien que là où les deux
  se croisent, la principale l'emporte et la suspendue débouche en surplomb. Le décor « cascade »
  (§10) n'est pas encore placé.
* **Détail** `N(p/11)` : bruit de base de période 256 unités, soit 2816 blocs ; amplitude ±0,6,
  invisible.
* **Parois déclarées** : `Relief::wall()` est vrai sur le flanc d'une auge (0,01 < g·u < 0,99,
  principale ou suspendue) ; ce sont les seules falaises permises par le test de continuité.

Repères mesurés (16 × 16 km autour de l'origine, Norrland) : h de S − 30 à S + 71 environ ;
fjords (sous la mer) 2,1 / 1,7 / 1,6 % de la carte (cible doc §7.5 : 2 %) ; 3 à 5 % des terres au-dessus
de S + 44. Points d'intérêt de la graine 12345 : côte à fjords vers (−5600, 300), chaîne vers
(−1800, 5900).

## Formes de relief (session 1.3, doc 02 §5.5 à §5.8)

`landforms.rs` (`Landforms::apply`) prend le relief `h₄` de la 1.2 et applique, dans l'ordre du
doc : archipel, côtes, eskers et marmites, drumlins, tourbières et palses, champ de lave, glacier.
Le résultat est le sol final `Relief::h` et la décomposition `Relief::forms` (`Forms` :
`filtered` = Hl, `archipelago` = K, `coast` (`Coast::{None, Skargard, Klint, Raised, Dunes,
SaltMeadow}`), `esker`, `drumlin`, `kettle`, `mire`, `mire_ridge`, `palsa`, `lava`, `moraine`,
`ice`, `cliff`). Les bruits ont leurs graines `0x4E52_5743_0000_02xx`. La glace est une épaisseur
au-dessus du sol : `Relief::top()` la compte (surface, départ), `Relief::ground_top()` non.

* **Relief filtré Hl** : la même chaîne de calcul que h (chaîne, plateaux, plafond, vallées) appliquée
  au socle seul, sans collines ni détail (§5.2). Les crêtes de la chaîne restent (le doc enlève aussi
  le « détail de crête » ; ce n'est pas séparable du terme `Rg`). `hydro::filtered_height` le renvoie (`Forms::filtered`) : la 1.4 y cale l'escalier W, les tourbières aussi. Les graines des formes sont `0x…03xx` (`0x…02xx` est à l'hydrographie).
* **Archipel** (§5.5) : formule du doc, avec trois réglages. K est multiplié par
  `ARCHIPELAGO_BOOST = 1,6` (sans quoi K dépasse rarement 0,6 et les îlots n'émergent presque jamais :
  0,1 % de la carte). La hauteur d'un îlot est bornée à `0,9 · rᵢ` : le doc laissait un écueil de
  6 blocs de rayon monter de 18 blocs. Le gain du préréglage Skärgård (× 2) élargit aussi le masque
  `sm(0,50 ; 0,65 ; q(F(p/1800; 2)))` de 0,2 : × 1,7 îlots au lieu de × 1,1.
* **Côtes** (§5.6), seulement pour 0,38 < C < 0,62. Province lue par `MacroFields::province_blend`,
  dont le poids s'annule sur ~60 blocs avant une frontière de province, pour que klint et terrasses
  s'éteignent sans marche. **Klint** (calcaire, E < 0,45) : `+12 · sm(0,468 ; 0,474 ; C)`, falaise
  déclarée, plateau calcaire jusqu'à C = 0,55 puis retour en pente douce à C = 0,62. **Côte soulevée**
  (granit rouge, O < 0,5, 0,25 < T₀ < 0,5) : terrasses de 4 blocs entre S et S + 24, avec un
  `arrondi` adouci (`4 · soft_floor(h/4 + 0,5 ; 0,4)`) pour rester continu ; galets en surface.
  **Pré salé** (E > 0,75, T₀ > 0,5) et **dunes** (E > 0,5) : bande littorale repérée par l'altitude
  (de S − 3 à S + 8 pour le pré, bosse autour de S + 2,5 pour les dunes) plutôt que par une distance
  à la côte, qu'on ne connaît pas à la colonne. Les raukar et les champs de galets en décor sont pour
  les sessions de décors.
* **Eskers** (§5.7) : formule du doc (`Z(p/1300)`, 2 octaves, demi-largeur 0,012), hauteur 5 à 8
  modulée par `N(p/300)`, en zone C > 0,55, U < 0,2. **Marmites** : grille de pas 64, une cellule sur
  deux, creux de 3 à 5 blocs (rayon 5 à 9) dont le centre est à 1,2 à 3,2 demi-largeurs d'un esker.
  **Drumlins** : grille de pas 90, 7 cellules sur 10, bosses de 3 à 5 blocs de rayon 14 à 22,
  allongées 3:1 le long de θ, en plaine (U < 0,2, E > 0,5) dans des champs `q(F(p/1500; 2)) > 0,6`.
* **Tourbières** (§5.8) : masque My du doc, zones boréale et subarctique (T₀ de 0,1 à 0,52, en
  pente douce), `h ← lerp(h, W + 1 + cordon, My)` avec W en escalier adouci (`soft_floor`, marches
  de 6) calculé sur Hl ; cordons `r < 0,35` le long de θ (le doc préfère la pente du relief lissé,
  coûteuse à la colonne). Cordon : sol jusqu'à W (h = W + 1) ; gouille : fond en W − 1 (h = W − 0,5), que l'hydrographie remplit jusqu'à W (eau affleurante) sur la partie plate de chaque marche.
  **Palses** (zone subarctique de la tourbière) : grille de pas 26, 55 % des cellules, buttes de 3 à
  6 blocs (profil parabolique, pente ≤ 2a/r), cœur de `permafrost` sous un bloc de tourbe.
* **Champ de lave** : plaine `S + 6 + 5 · Rg(p/90; 3)`, plus un volcan-bouclier par cellule de pas
  2400 (+30, rayon 450 à 600), des pseudo-cratères (pas 70, anneaux de 4 à 7 blocs) et des fissures
  `Z(p/700) < 0,01` de 8 blocs (falaises déclarées ; la mer y entre si leur fond est sous S). Écart :
  le poids est `sm(0 ; 0,5 ; Gt)` et non Gt, pour que toute la province Gt > 0,5 émerge (au-dessus
  des grands fonds, Gt ≈ 0,54 laissait la plaine sous l'eau) ; la montée de Gt de 0 à 0,5 forme le
  flanc de l'île. Basalte sur 4 blocs.
* **Glacier** : dans la chaîne (`sm(0,25 ; 0,45 ; U)`), calotte de surface `G = Hl + 5` où
  `Hl > y_n` (ligne des neiges), langues dans les auges (§5.4) dont les épaulements dépassent
  `y_n`, jusqu'à `y_a`, épaisses de 8 ; crevasses en bandes transverses à θ (le doc : là où la pente
  de G dépasse 0,35, non calculable à la colonne) dans les langues de plus de 6 blocs, profondes de 6 à
  20, falaises déclarées ; moraines de gravier (+3) sous le contour de la calotte (Hl ≈ y_n − 5) et
  au front des langues. Pas encore de lac proglaciaire. La glace est posée au-dessus du sol et de l'eau ; la hauteur isolée (départ) est le dessus de la glace sur la terre sèche.

Blocs provisoires ajoutés à `assets/norrcraft/blocks.json` (textures empruntées) : `flygsand`,
`klappersten`, `torv`, `permafrost`, `blais`, `basaltsten`. Le doc 04 §9 dit `sand` et `basalt`,
mais ces noms existent dans vanilla et les univers ne partagent aucun bloc hors du socle
(`nb-game/tests/sealed.rs`) : d'où `flygsand` (sable éolien, celui des dunes) et `basaltsten`. Couverture
(`NorrBlocks::cover`) par priorité : basalte, moraine, gravier sous la glace, palse (tourbe sur
permafrost), tourbe, gravier des eskers et drumlins, roche nue en pente, granit nu des îlots,
sable des dunes, galets des côtes soulevées, puis la règle de la 1.2.

Repères mesurés (carré de 24 km, pas 40, Norrland, 3 graines) : îlots émergés 0,2 %, klint 0,9 à
1,4 %, côte soulevée 0,1 à 0,3 %, dunes 3,4 à 3,9 %, pré salé 0,3 à 0,4 %, eskers 0,7 à 0,8 %,
drumlins 0,3 %, tourbières 0,9 à 1,5 %, palses 0,01 à 0,03 %, glace 0,2 à 0,6 % (Högfjäll : 1,5 % sur
16 km). Champ de lave : rare et au large (graine 12345 : un à 10 km ; graine −98765 : aucun à moins de 12 km) ; la province la plus forte à moins de 30 km est à 20 à 34 km de l'origine. Points
d'intérêt de la graine 12345 : glacier (Högfjäll) vers (700, −4300), tourbière et palses vers
(1400, −7400), côte soulevée et îlots vers (10400, −6100), lave vers (6300, −7500) et (−28200, 17000).
## Hydrographie (session 1.4, doc 02 §6)

L'eau du moteur est statique : chaque nappe est horizontale et étanche par construction. Pour
chaque colonne, `Hydrology::column` part du relief de la 1.2 et rend une `WaterColumn` : `top`
(premier `y` non solide), `level` (premier `y` d'air au-dessus de l'eau ; l'eau occupe
`top..level`), la nature de l'eau et le niveau en escalier. Dans l'ordre :

1. **Escalier W** (§6.1) : `W = S + 6 · plancher(max(0, Hl − S − 2) / 6)`, avec `Hl` le relief
   filtré (`filtered_height` : socle, chaîne, plateaux, plafond doux et vallées, sans collines ni
   détail, calculé par la fonction de hauteur depuis la 1.3). **Tout creux sous W se remplit à W** (la
   nappe) : la mer sur la côte (W = S tant que Hl < S + 8), des étangs et des lacs de creux dans
   les terres. Juste au-dessus d'une marche (15 % de la marche), les creux sont comblés jusqu'à W
   (barre rocheuse, granit nu) : deux nappes de niveaux différents ne se touchent pas en plein champ.
2. **Lacs de vallée** (fingersjöar, §6.3) : dans une auge intérieure (g > 0,3, fond f ≥ S + 2),
   près de l'axe, là où `q(F(p/600; 2)) > 0,6`, le fond descend jusqu'à W − 8 ; le fond plat de
   l'auge (W + 2) les barre de part et d'autre.
3. **Lacs** (§6.3) : Worley de pas 180, probabilité `0,15 + 0,6 · Lk` si le centre est sur la
   terre (C ≥ 0,52), hors chaîne (U < 0,25), hors auges, plateaux et parois. Niveau constant
   `ℓ = max(plancher(min(h, Hl)) − 1, W)` au centre (> S). Rayon de base 14 à 60 (`14 + 46 · a³` :
   beaucoup de petits lacs), contour `r · (0,6 + 0,8 · q(F(p/70; 3)))` (îles), distance
   **allongée de 1,6 le long de θ** (lacs de surcreusement), cuvette de 3 à 14 blocs, berges
   talutées (pente 0,6, effacées au-delà de 20 blocs), **margelle** au niveau ℓ sur 4 blocs puis
   digue à pente 1 là où le sol est plus bas.
4. **Rivières** (§6.2) : `Rv = Z(warp(p; 110, 300) / 850)` comme au doc, mais la largeur est en
   **blocs** : distance à l'axe estimée par `Rv / |∇Rv|` (différences finies sur les coordonnées
   déformées), demi-largeur `2,5 + 4,5 · sm(0,70 ; 0,47 ; C)` (3 à 7 blocs, plus larges vers la
   mer), lit en V de W − 2 à W − 5, berges à pente 0,8. Au-dessus d'une marche, le lit remonte à
   W − 1 (barre, rapide) ; sous la marche suivante, il est surcreusé de 3 blocs (bassin de chute).
   Le réseau s'éteint dans la chaîne (`1 − sm(0,20 ; 0,45 ; U)`) : là, la rivière suit l'axe des
   auges glaciaires (§5.4, pas de second réseau).
5. **Scellement** (`seal`) : une colonne voisine d'une eau plus haute que ce qu'elle remplit est
   montée jusqu'à ce niveau (barrage d'un bloc, lèvre de chute). Une seule passe sur les colonnes
   brutes suffit : aucun bloc d'eau n'a d'air pour voisin latéral, quel que soit le relief. Un chunk
   calcule 20 × 20 colonnes brutes (deux bordures), une colonne isolée ses 4 voisines : les deux
   coïncident bit à bit (`Hydrography::column` = chunk généré).

Nature de l'eau (`WaterKind`) : **lac** (Worley, vallée, creux, anciennes « mers intérieures »),
**mer** au niveau S reliée au large (C < 0,47, fjord ou Hl < S − 2 ; jusqu'à C < 0,5 hors rivière),
**rivière** dans un lit actif. Toute eau au-dessus de S est un lac ou une rivière. `SeaFacade`
(§6.4) : atlantique (O > 0,5), baltique, arctique (T₀ < 0,12) ; même bloc d'eau partout.

Le générateur remplit l'eau de `top` à `level` ; gravier sous l'eau et sur les rives (fond ≤
niveau + 1), granit nu sur les barrages et barres. `surface_height` rend le fond ; le départ du
joueur évite les colonnes mouillées. Coût mesuré : **0,31 ms par chunk** en `--release` (0,18 en
1.2 ; budget du doc : 2,5 ms).

Écarts et précisions par rapport au doc :

* **Les « mers intérieures » de la 1.2 deviennent des lacs** : un creux des terres sous le niveau
  de la mer n'est plus rempli à S mais à son W (≥ S + 6 dès que Hl ≥ S + 8) ; ceux qui restent à S
  près des côtes sont étiquetés lac s'ils ne sont pas reliés au large selon C.
* **Pas de chute d'eau verticale** : un bloc d'eau ne peut pas avoir d'air à côté. Aux marches de
  W, la rivière passe par une barre (eau sur 1 bloc), puis un barrage d'un bloc et un bassin en aval.
  Le décor « cascade » (§10) reste à faire.
* **Niveau des lacs** : le doc donne `plancher(Hl(c)) − 1` ; on prend le plus bas du relief réel et
  du relief filtré au centre, jamais sous W : moins de terrassements quand le centre tombe dans un
  creux des collines.
* **Lacs allongés le long de θ** et contour étalé par q : sans cela, des disques presque parfaits.
* **Deltas** (bancs de sable vers C ≈ 0,47) : non faits, avec les côtes (1.3) et les blocs de sable
  (2.1). **Banquise** arctique : pas de bloc de glace dans la palette, seule `SeaFacade::Arctic`
  existe. Eau saumâtre : drapeau de biome (2.1).
* Le **myr** de la 1.3 se cale sur le même W (adouci), et ses gouilles prennent l'eau de la nappe.

Repères mesurés (graine 12345, Norrland, 16 × 16 km) : mer 30,9 %, lacs 3,1 %, rivières 1,8 % ;
1539 lacs du Worley ; à moins de 5 km de l'origine, lacs 3,7 à 4,0 % et rivières 2,1 à 2,5 %
selon la graine, plus de 100 lacs de plus de 80 blocs ; niveaux d'eau de S à S + 30 sur 4 km.

## Outil `worldmap`

```
cargo run --release -p nb-world --example worldmap -- <graine> <préréglage> <cx> <cz> <taille> <pas> <fichier.png> [mode]
```

`préréglage` : `norrland` | `skargard` | `hogfjall` ; `taille` en pixels, `pas` en blocs par pixel ;
`mode` : `relief` (défaut depuis la 1.2 : relief ombré, teintes hypsométriques, mer par
profondeur) | `hydro` (1.4 : relief ombré après l'hydrographie, mer par profondeur, lacs en
turquoise, rivières en bleu vif, élargies aux grands pas pour rester visibles) | `w` (1.4 : terres
teintées par marche de l'escalier W, eau comme `hydro`) | `h` (altitude en fausses couleurs) | `formes` (1.3 : relief ombré et formes en couleur : îlots corail, klint crème, côte soulevée rouge, dunes sable, pré salé vert d'eau, eskers jaunes, marmites cyan, drumlins vert clair, tourbières brunes, palses violettes, glace blanche, moraines brun clair, lave rouge ; l'outil imprime aussi leurs parts) | `fjord` (relief ombré ; fjords en magenta, fonds
de vallée principale en jaune, plateaux en cyan) | `overview` (vue des champs de la 1.1 : terre par
zone climatique, gris de la surrection, rouge = lave) | `c` | `o` | `t0` | `hm` | `u` | `e` | `lk` |
`gt` | `vr` | `zones` | `pv` | `theta`. Le trait de côte dessiné est désormais le vrai (h = S).
La sortie est un PNG sans dépendance (`image_out.rs` ; `.ppm` reste possible). L'outil imprime
aussi les parts de terre (h ≥ S, et C ≥ 0,465 pour comparaison), de fjords, de plateaux, de hautes
terres, de zones climatiques et de provinces, et les quantiles de h et de chaque champ.
Les cartes vanilla fonctionnent comme avant (`<type>` = normal, flat, classic…). Avec Docker :
`NB_TARGET_VOL=nb-target-<nom> sh scripts/cargo-docker.sh run --release -q -p nb-world --example worldmap -- …`
(les images sont écrites dans le dossier du dépôt, ex. `apercu-norr/`, ignoré par git).

Repères mesurés (graine 12345, Norrland, 16 × 16 km autour de l'origine) : terre 70 % (cible doc
§7.5 : mers 30 %, tolérance ±40 %), zones : subarctique 21 %, boréale 25 %, hémiboréale 42 %,
némorale 12 % ; l'arctique n'apparaît qu'à plus de 16 km au nord. À l'origine : terre (C = 0,73),
zone hémiboréale (T₀ = 0,60).

## Tests

* `nb-world/src/norrcraft/*` (tests unitaires) : définition de chaque brique, hachage uniforme,
  Worley exact (comparé à une recherche exhaustive) et continu, bornes de `warp`, champs dans
  [0, 1], continuité, valeurs de référence figées (graine 12345), préréglages, parts de provinces,
  treillis contre évaluation exacte.
* `nb-world/tests/norrcraft_fields.rs` : **empreintes figées de 64 chunks × 3 graines**
  (`FINGERPRINTS`), absence de fonction transcendante, parts de terre sur 16 km, présence de terre
  tempérée (point de départ, doc 02 §13), zones climatiques, Högfjäll plus froid et plus montagneux,
  absence de marche le long d'un transect de 24 km.
* `nb-world/tests/norrcraft_relief.rs` (1.2) : **empreintes figées des blocs de 64 chunks × 3
  graines**, terre à 62–80 % sur 16 km (mesuré 69 à 72 %), Skärgård plus marin que Norrland,
  altitudes de S − 30 à moins de S + 84 avec des sommets au-dessus de S + 55, fjords profonds
  (25 blocs) à moins de 8 km, parois de fjord hautes, plateaux et hautes terres de Högfjäll,
  **continuité** `|Δh| ≤ 3` le long d'un transect de 24 km hors parois d'auge déclarées (doc
  §13), **étanchéité** de la mer d'un chunk à l'autre sur une côte, hauteur isolée = chunk généré ;
  mesure de temps ignorée par défaut (`-- --ignored`). En 1.4, ses empreintes de blocs ont changé
  (l'eau des lacs et rivières) et la hauteur isolée admet de l'eau au-dessus du fond.
* `nb-world/tests/norrcraft_hydro.rs` (1.4) : **étanchéité** (aucune eau avec de l'air à côté ni
  dessous) sur 6 × 6 chunks autour de points choisis par relevé pour 3 graines (marche de rivière,
  grand lac, étang au-dessus de la mer) et sur une côte à fjords, un monde Skärgård ; lacs (dont un
  de plus de 80 blocs) et rivières à moins de 5 km ; mer toujours à S, eau au-dessus de S toujours
  lac ou rivière ; nappes calées sur l'escalier W ; **continuité de la surface visible** (sol ou
  eau) sur 6 km hors marches déclarées (barrages, barres, parois d'auge) ; colonne isolée = chunk
  généré, eau comprise.
  mesure de temps ignorée par défaut (`-- --ignored`). En 1.3, les falaises déclarées comprennent
  klint, crevasses et fissures (`Relief::wall`).
* `nb-world/tests/norrcraft_landforms.rs` (1.3) : chaque forme présente à moins de 12 km pour
  3 graines, Skärgård plus riche en îlots, **continuité 2D** sur des carrés de 48 × 48 colonnes
  autour d'exemples de chaque forme, glace seulement en montagne, province de lave émergée et
  continue jusqu'au large, blocs posés par le générateur (glace bleue, tourbe, permafrost, sable,
  galets, basalte).
* Toute modification d'une formule ou d'une constante de champ change les empreintes : les mettre
  à jour consciemment (et le dire dans le compte rendu). En 1.2, le réglage du biais de C a changé
  les empreintes des champs (`FINGERPRINTS`, `REF0`, `REF1` : seul C bouge). Les empreintes vanilla
  (`tests/fingerprints.rs`) et l'étanchéité (`nb-game/tests/sealed.rs`) restent verts. En 1.3,
  seules les empreintes du terrain (`norrcraft_relief.rs`) changent ; celles des champs non.
