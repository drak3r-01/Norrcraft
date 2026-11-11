# Worldgen Norrcraft : champs (1.1), relief (1.2), formes (1.3), hydrographie (1.4), biomes (2.1), sous-sol (2.2), arbres (2.3)

Le vrai relief Norrcraft (GDD doc 02 §3 à §7 et §12 ; plan 06, sessions 1.1 à 1.4 et 2.1). La 1.1
a posé les briques mathématiques et les champs lents ; la 1.2 construit dessus la fonction de
hauteur et remplace le monde plat de granit de la phase 0 par un vrai terrain ; la 1.3 y pose
archipel, côtes, eskers, tourbières, glaciers et champs de lave ; la 1.4 y met l'eau (mer, lacs,
rivières), étanche par construction ; la 2.1 y choisit les 25 biomes, leurs sols et leurs teintes ; la 2.2 y creuse le sous-sol (provinces
rocheuses, filons, minerais, cavités).
Tout vit dans `crates/nb-world/src/norrcraft/`.

## Modules

| Fichier | Contenu |
|---|---|
| `math.rs` | hachage SplitMix64 (`hash64`, `hash01`), utilitaires (`sigmoid`, `spread` = q, `smooth` = sm, `bump`, `soft_cap`), `GradField` (N, fBm, crête `ridged`, ligne de zéro, `warp`), `Worley` (F1, F2, cellule, point) |
| `config.rs` | `NorrConfig` (préréglage, niveau de mer 40, hauteur 128 : décision D3), `NorrPreset` (Norrland, Skärgård, Högfjäll) |
| `fields.rs` | `MacroFields::sample(x, z)` → `MacroSample { c, o, t0, hm, u, e, lk, gt, vr, theta }` ; `province(x, z)` ; `ChunkFields` (treillis 4 × 4) |
| `climate.rs` | `ClimateZone` (arctique … némorale), `altitudes` (limites des arbres, de la roche nue, des neiges), `temperature_at` |
| `height.rs` (1.2) | `HeightFields` : `column(wx, wz)` (treillis, identique aux chunks), `relief_exact(x, z)` (cartes), `relief(x, z, &MacroSample)` → `Relief { h, socle, land, chain, before_valleys, plateau, plateau_level, valley, valley_floor, hanging, fjord, forms }` (`forms` : 1.3) ; `socle(C)`, `soft_floor` |
| `landforms.rs` (1.3) | `Landforms::apply` : archipel, côtes, eskers, marmites, drumlins, tourbières, palses, champ de lave, glacier → `Forms` (dans `Relief::forms`), `Coast` |
| `terrain.rs` (1.2 à 2.1) | `NorrcraftTerrain` (`TerrainGenerator` ; `column_biome`, cache de biomes par chunk) et `NorrBlocks` (blocs des formes de la 1.3, sols des biomes, peau, neige éternelle, banquise ; glace posée au-dessus du sol et de l'eau) ; `WorldProfile::norrcraft(graine, NorrConfig, &registre)` (25 biomes, teintes moyennées 5 × 5), utilisé par `World::for_kind` |
| `biomes.rs` (2.1) | `NorrBiome` (25 biomes), `biome_defs`/`biome_infos` (`assets/norrcraft/biomes.json`), `BiomeSoil`, `BiomeSelector::select` (règles du §7.2, étagement, frontières tramées) → `Selection { biome, fresh_water, snow, dither }`, `DISTRIBUTION_TARGETS` (§7.5) |
| `subsoil.rs` (2.2) | `Subsoil` (provinces au treillis 4, filons de diabase, minerais, cavités, corps de la grille : `sites`, `deposits`), `SubColumn` (tirages par colonne), `SubsoilBlocks`, `Region`, `SiteProbe` ; `math.rs` gagne `GradField3` (bruit 3D) |
| `trees.rs` (2.3) | `Species` (12 essences), `Kind`/`Style`, `Plant`, `TreeBlocks`, formes pures (`draw` via le trait `Put`), `Trees` (placement en grille de pas 5, cache de cellules, `plants_near`, `stamp`, `pass`, `plant_within`), `TreeProbe`/`TreeColumn` |
| `flora.rs` (2.4) | `Understory::pick(&Column)` (strate basse : 37 plantes, fiches `SPECS`), `FloraBlocks`, `Ground`, `Water` |
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

## Biomes (session 2.1, doc 02 §7 ; doc 04 §9)

`biomes.rs` choisit le biome de chaque colonne ; `terrain.rs` en tire le sol ; le mailleur en tire
les teintes. Le relief ne dépend d'aucun biome (doc 02 §7.4, point 1).

### Données

* `assets/norrcraft/biomes.json` (embarqué par `nb_data::universe::NORRCRAFT_BIOMES_JSON`) : les
  25 biomes **dans l'ordre des identifiants** (`NorrBiome` : Västerhav = 0 … Lavafält = 24), avec
  glose française, teintes d'herbe, de feuillage et d'eau, température indicative, précipitations,
  thème musical, ambiances `[vent, oiseaux, grillons, grenouilles, eau, lave]` et **sol** : `top`
  (peau d'un bloc sur le plat), `filler` et `depth` (couverture meuble), `rock` (roche nue des
  pentes), `floor` (fond sous l'eau et rives), `patch` facultatif (`block`, `share` : taches de
  sol). Le chargement vérifie l'ordre des biomes et l'existence des blocs.
* `assets/norrcraft/blocks.json` : 19 blocs de terrain du doc 04 §9 : roches `rodgranit`, `gnejs`,
  `kalksten`, `djupberg`, `sprucket_berg` ; sols `svartsand`, `blalera`, `jord`, `gras`, `mossa`,
  `barrmatta`, `lovforna`, `lavhed`, `vitmossa`, `aska`, `rodmull`, `slagg` ; neige et glace
  `sno`, `havsis`. `gras` et `mossa` sont teintés par le biome : un cube `grassTinted` non
  cruciforme a désormais le rôle « herbe » du mailleur (vanilla n'en a qu'un, `grass`). `sand` et
  `basalt` restent interdits (noms vanilla) : `flygsand` et `basaltsten` de la 1.3 les remplacent.
* Textures provisoires `norr_*` (25 PNG dans `resource_packs/Default/…/block/`) :
  `scripts/gen_norr_textures.py`, procédural et sans dépendance :
  `docker run --rm -v "$(pwd -W):/w" -w /w rust:1 python3 scripts/gen_norr_textures.py`.
  `norr_gras_top` et `norr_mossa_top` sont gris (multipliés par la teinte du biome). Les blocs
  de la 1.3 empruntent encore des textures vanilla.

### Sélection (doc 02 §7.1 et §7.2)

Premier test vrai, sur la colonne, ses 4 voisines (`Neighbourhood`, pour les gradients) et sa
colonne d'eau scellée :

1. lave (`forms.lava > 0,5`, là où est le basalte) → Lavafält ; 2. glace → Glaciär ;
3. mer : arctique → Packis, fjord → Fjord, K > 0,3 → Skärgård, O < 0,5 → Bottenhav,
   C < 0,25 → Djuphav, sinon Västerhav ;
4. rivière et ses berges (`channel_distance < largeur + 4`) → Älv ; un lac garde le biome
   environnant, avec `fresh_water` (son altitude est celle de sa surface) ;
5. étagement : y > y_k → Kalfjäll, y > y_a → Fjällhed, y > y_b → Fjällbjörkskog ;
6. subarctique ou arctique bas : plat et humide (palse, tourbière ou Hm > 0,6) → Palsmyr ;
   sinon Vr > 0,72 → Fjällbjörkskog, sinon Fjällhed ;
7. côtes : klint → Klintkust, côte soulevée → Högkust, archipel → Skärgård, pré salé →
   Strandäng, dunes → Tallhed ; puis C < 0,5 et K > 0,3 → Skärgård ;
8. tourbière (My > 0,15, ou plaine plate boréale ou hémiboréale de Hm > 0,78) → Myr ;
9. province grönsten au-dessus de S + 3 → Bergslag ;
10. boréale : esker ou Hm < 0,35 → Tallhed, Vr > 0,78 → Brännfält, sinon Urskog ;
11. hémiboréale : Lk > 0,6 → Sjöland, Hm > 0,50 → Urskog, Hm < 0,35 → Tallhed, sinon Hagmark ;
12. némorale : calcaire et E > 0,35 → Alvar, Hm > 0,58 → Bokskog, sinon Hagmark.

**Frontières tramées** (§7.4) : toute comparaison d'un champ continu à un seuil (zones de T₀,
altitude contre y_a, y_b, y_k, y_n, Hm, Lk, Vr, E, O, C, K, My) est vraie avec la probabilité
`sm(−12, 12, d)`, `d = (valeur − seuil) / |∇valeur|` (différences centrées sur les voisines, en
blocs), comparée à `h(graine ^ 0x…0401, x, z)`. Deux biomes voisins s'interpénètrent sur 24 blocs ;
le sol suit ce biome tramé (la flore le suivra). Les tests de formes physiques (lave, glace, mer,
rivière, type de côte, esker) restent francs. Provinces (Bergslag, Alvar) : poids de frontière de
`province_blend` comparé à `0,5 · h` (tramage sur ~30 blocs).

**Neige éternelle** : au-dessus de y_n (tramé), une colonne sèche porte un bloc de `sno`, quelle
que soit la pente (le reste de la neige sera la surcouche saisonnière de la phase 3).

### Écarts au doc 02 §7.2, réglés sur les cibles du §7.5

Les champs de la 1.1 donnent près de l'origine une zone hémiboréale (44 % des terres) bien plus
vaste que ce que supposent les cibles, et aucune zone arctique à moins de 8 km. Constantes dans
`biomes.rs` :

| Règle | Doc | Retenu | Pourquoi |
|---|---|---|---|
| Myr | My > 0,5 | My > 0,15, ou plaine plate boréale ou hémiboréale de Hm > 0,78 | le cœur de tourbe seul ne fait que 0,5 % de la carte (cible 6) |
| Brännfält | Vr > 0,92 | Vr > 0,78 | Vr n'atteint presque jamais 0,92 (95ᵉ centile : 0,81) |
| Hémiboréale | Sjöland ou Hagmark | + Urskog si Hm > 0,50, Tallhed si Hm < 0,35 | Hagmark faisait 20 % de la carte (cible 6) |
| Bokskog | Hm > 0,45 | Hm > 0,58 | +67 % sinon |
| Alvar | E > 0,6 | E > 0,35 | −74 % sinon |
| Subarctique bas | « selon Vr » | Vr > 0,72 → Fjällbjörkskog | |
| Côtes | sous C < 0,50 | formes de côte sans condition sur C ; dunes → Tallhed | l'emprise des formes de la 1.3 déborde C < 0,5 ; les dunes n'avaient pas de biome |
| Berges | | Älv sur 4 blocs de part et d'autre du lit | « lit et berges » de la fiche |

Répartition mesurée (16 × 16 km, moyenne des graines 12345, −98765, 20261111, cible entre
parenthèses) : mers ouvertes 23 % (21), Fjord 1,8 (2), Skärgård 4,4 (4), Urskog 13,7 (14), Tallhed
5,9 (7), Myr 6,6 (6), Sjöland 6,0 (7), Hagmark 8,2 (6), Bokskog 3,8 (3), Alvar 1,0 (1),
Fjällbjörkskog 7,0 (5), Fjällhed 8,2 (6), Kalfjäll 1,5 (3), Glaciär 0,6 (1), Palsmyr 2,4 (2),
Bergslag 1,5 (2), côtes 1,2 (4), Brännfält 0,7 (0,5), Lavafält 0,1 (0,5), Älv 2,4 (2).

**Hors cible**, exemptés du test : Packis (pas de mer arctique à moins de 8 km ; la banquise
commence vers 17 km au nord-ouest pour la graine 12345), côtes (côtes soulevées et klints rares,
formes de la 1.3), Lavafält (champ Gt de la 1.1 ; absent à moins de 8 km pour deux graines),
Kalfjäll (peu de sommets au-dessus de y_k), Fjällbjörkskog (+41 % : la ceinture de 8 blocs sous la
limite des arbres couvre beaucoup de versants). Le doc 02 §13 veut chaque biome à moins de 6 km
de l'origine : ce n'est pas tenu pour Packis et Lavafält.

### Sol (`terrain.rs`)

Couverture, premier cas vrai : formes de la 1.3 (basalte et, sur ses rivages, sable noir ;
gravier des moraines, du pied des glaciers, des eskers et drumlins ; tourbe et permafrost des
palses ; tourbe des tourbières ; granit des îlots ; sable des dunes ; galets des côtes soulevées) ;
pente > 2 → `rock` du biome sur 2 blocs ; sous l'eau → `floor` sur 2 blocs puis `filler` ; rive
(au plus 1 bloc au-dessus de la nappe) → `floor` ; sinon `filler` sur `depth − pente` blocs.
Puis la **peau** d'un bloc sur le sec et le plat (pente ≤ 1) : `top` du biome, ou la tache
`patch` (tirage `0,75 · q(fBm(p / 6)) + 0,25 · h` : taches groupées de quelques blocs, bords
irréguliers) ; les dunes ne la portent qu'au-delà de S + 2 ; la neige éternelle passe avant tout.
La **banquise** (Packis) remplace la couche d'eau du dessus par `havsis`, hors des chenaux
(`|N(p / 70) − 0,5| < 0,025`) ; `surface_height` reste le fond. Coût mesuré : 0,38 ms par chunk
(budget 2,5).

### Moteur

* `TerrainGenerator::biome_at` lit un cache de chunks (`Arc<Mutex<…>>` partagé par toutes les
  copies du générateur, 4096 chunks, vidé d'un coup au-delà) : la génération y range les biomes de
  chaque chunk ; une colonne absente fait analyser son chunk entier. `NorrcraftTerrain::column_biome`
  calcule une colonne seule, **identique** (testé) : cartes et tests.
* Teintes : `WorldProfile::tint_blend` (Norrcraft : 2 ; vanilla : 0) → `BiomeSource::tint_blend_radius`.
  À 2, le mailleur moyenne les teintes d'herbe, de feuillage et d'eau sur 5 × 5 colonnes et fusionne
  les faces par teinte moyenne (palette du chunk, au plus 256 entrées). À 0, rien ne change
  (maillage vanilla identique, tests dorés verts).
* Aperçu 3D : `crates/nb-game/tests/preview_norrcraft.rs` (ignoré ; lavapipe) écrit
  `crates/nb-game/renders/norrcraft_<graine>_<x>_<z>_<vue>.png` (ignoré par git) :
  `NB_GFX_REQUIRE_GPU=1 NB_SH=1 crates/nb-gfx/scripts/vk-docker.sh 'NB_NORR_AT=0,-1700 cargo test --release -p nb-game --test preview_norrcraft -- --ignored'`
  (`NB_NORR_SEED`, `NB_NORR_AT=x,z` ; par défaut, le point de départ).

## Sous-sol (session 2.2, doc 02 §8)

Un seul fichier, `subsoil.rs` (`Subsoil`, `SubColumn`, `SubsoilBlocks`), branché dans
`terrain.rs` : le granit de fond d'avant devient la **roche de la province**, puis filons,
minerais et cavités. Tout est fonction pure de la graine et des coordonnées (bruit 3D :
`GradField3` dans `math.rs`, constantes `0x…05xx`).

### Blocs

20 blocs ajoutés à `assets/norrcraft/blocks.json` (ids 34 à 53 : l'id 33 est celui de la lave
commune), textures procédurales `norr_*` (`scripts/gen_norr_textures.py`) : roches `sandsten
alunskiffer gronsten porfyr diabas krita taljsten kvarts svavel`, minerais `myrmalm sjomalm
jarnmalm rik_jarnmalm kopparmalm silvermalm koboltmalm guldkvarts guldgrus volframmalm flinta`.
Pas de charbon minéral, ni diamant ni émeraude (décision D8).

### Strates (§8.1)

* sol du biome et couverture meuble : inchangés (2.1) ; **la roche nue** des pentes, des barrages
  et des îlots de Skärgård prend la roche de la province quand le biome ne fixe pas la sienne
  (`rock = granit` dans `biomes.json`) : plus de bande de granit de 2 blocs sur une falaise de
  gneiss ;
* **socle** : roche de la province de la colonne (`Province` de la 1.1) ; **gneiss** partout où
  `U > 0,8` (cœur de la chaîne) ; **basalte** sous le Lavafält ; **krita** (craie) à Klintkust
  au-dessus de S − 4, avec des bandes de **flinta** (`(y + 4·N₃(p/25)) mod 6 = 0` et
  `N₃(p/4) > 0,55`) ;
* **djupberg** sous `y < 14` ; **bedrock** en `y ≤ 1` (avant : `y = 0` seul) ;
* la province se lit sur un **treillis global de 4 blocs** (réévaluée exactement quand les quatre
  coins diffèrent, et en pays calcaire pour le poids de frontière) : une colonne a toujours la
  même province, quel que soit le chunk ;
* **filons de diabase** : murs verticaux qui traversent tout le socle (djupberg compris) quand la
  **distance estimée** à la ligne de zéro de `2 F(p/900) − 1` (|s| / |∇s|, gradient par
  différences centrées) est inférieure à 1 bloc : largeur de 2 blocs partout, même là où le champ
  est plat (la formule brute `Z < 0,004` du doc donnait des murs de plus de 20 blocs sur les
  plateaux). Largeur médiane mesurée : 2.

### Minerais (§8.2)

| Gisement | Règle réalisée |
|---|---|
| myrmalm | Myr sec : `q(F(p/25, 3)) > 0,68`, 2 blocs sous la couverture de tourbe |
| sjömalm | fond (2 blocs) des lacs de Sjöland et Tallhed, Bottenhav à moins de 8 blocs d'eau : `q(F(p/20, 3)) > 0,70` |
| fer de roche | pays minier (grönsten, Bergslag, contact calcaire à moins de ~30 blocs d'une autre province) : `abs(2 N₃(P/40) − 1) < 0,035` et `N₃(P/90) > 0,55`, y de 10 à 60 ; cœur (`< 0,012`) en `rik_jarnmalm` ; **chapeau de fer** (`rodmull` en surface) quand un filon passe à 5 blocs sous le sol |
| **Malmberg** | grille de pas 512, une cellule sur deux ; lentille de demi-axes (60, 14, 45), plan incliné à 60°, `Σ((P − c)·eₖ/aₖ)² < 1 + 0,25·N₃(P/12)`, centre 8 blocs sous le sol (elle affleure) ; `rik_jarnmalm` à `Σ < 0,55`, `jarnmalm` ailleurs ; seulement si `T₀ < 0,34` et `U > 0,20` au centre (écart : « subarctique, dans la chaîne » ne donnait aucun Malmberg à moins de 5 km de l'origine) |
| cuivre | grille de pas 384, 60 % des cellules du pays minier : amas de rayon 12 à 20 (paroi irrégulière 0,3), `kopparmalm` au cœur, auréole de terre rouge (`rodmull`) |
| argent | calcaire, y de 5 à 30 : `abs(2 N₃(P/28) − 1) < 0,010` |
| cobalt | pays minier, grille de pas 64 (10 % des cellules) : poches de 3 à 6 blocs, y de 8 à 34 |
| or | nord (`T₀ < 0,28`) : veines de quartz `abs(2 N₃(P/30) − 1) < 0,02` à y ≤ 24 (`kvarts`), `guldkvarts` quand `N₃(P/9) > 0,78` ; paillettes (`guldgrus`, 4 % des colonnes) au sommet du gravier des rivières du nord |
| tungstène | sous Kalfjäll (surface au-dessus de `y_k`), grille de pas 64 (12 %) : poches de 2 à 4 blocs, y de 4 à 17 |
| stéatite | grille de pas 128 (35 %), au-dessus de la limite des arbres : lentille de demi-axes (10, 4, 8) affleurante |
| flinta | bandes de la craie de Klintkust (voir plus haut) |
| plaques de surface | `svavel` sur le Lavafält (`q(F(p/14, 2)) > 0,86`), `kvarts` sur les roches nues (Skärgård, Högkust, Fjällhed, Kalfjäll), argile (`blalera`) sous la peau en Hagmark, Bokskog, Strandäng, sable de verrerie (`flygsand`) en Sjöland |

Le fBm brut dépasse rarement 0,68 : les plaques lisent `q(F)` (le même étalement que les champs
du §4). Les blocs de magnétite erratiques, l'ambre (déposé par les tempêtes) et les structures
minières relèvent des sessions suivantes.

### Cavités (§8.3)

**Règle d'étanchéité unique** : une cellule creusée est **noyée** au-dessous du niveau de la mer
(`y < S`), d'**air** au-dessus ; une cavité enterrée garde 4 blocs de roche au-dessus d'elle et
sous les colonnes voisines (`min(top propre, tops voisins) − 4`) ; une cavité ouverte (faille,
marmite, effondrement, grotte marine) n'est creusée que si aucune colonne voisine ne porte d'eau
autre que la mer. Aucun bloc d'eau n'a donc d'air pour voisin, même d'un chunk à l'autre (test
`norrcraft_subsoil.rs`).

| Cavité | Réalisation |
|---|---|
| karst | province calcaire : deux nappes `abs(2 N₃ − 1) < 0,12` (échelle 30 × 18 × 30) qui se croisent → galeries ; la « rivière souterraine au niveau W » est un niveau d'eau constant S pour toutes les cavités (écart : W varie par marche, ce qui ferait voisiner eau et air d'une colonne à l'autre) ; en pays calcaire bas la plupart des galeries sont donc noyées |
| failles | granit et gneiss, colonnes sèches à voisines sèches : distance estimée à la ligne de zéro de `2 F(p/600) − 1` < 0,8, fente de 10 à 25 blocs depuis la surface, ouverte sur le ciel |
| grottes marines | grille de pas 48 (40 %) au pied d'une falaise (Klintkust, Högkust, calcaire) à 3..12 blocs de la mer : ellipsoïde ouvert sur l'eau, plancher à S − 2, toit de 3 blocs |
| tunnels de lave | Lavafält : ligne de zéro `F(p/60, 2)` à profondeur constante (8 blocs, 0 à 2 de variation) sous le sol, 5 blocs de haut ; effondrements ponctuels (`N(p/5) > 0,88`) jusqu'à la surface |
| grottes de glace | Glaciär : tunnel de fonte de 2 blocs de haut au contact glace-roche (ligne de zéro `F(p/45, 2)`), là où la glace fait au moins 6 blocs ; statique (la partie saisonnière viendra avec la 3.2) |
| marmites de géant | grille de pas 24 (22 %) : puits 2 × 5 dans les lits de rivière (noyés comme la rivière) et sur les rochers du rivage marin à S..S + 2, hors calcaire |
| salles de la montagne | grille de pas 700 (25 %), sous la chaîne (`U ≥ 0,55`) : ellipsoïde (14, 7, 11), centre 32 blocs sous le sol (la structure de troll viendra avec le bestiaire) |
| chaos de blocs | reporté aux structures (phase 4) |

`surface_height` ne change pas : une faille ou une marmite sèche est un trou dans une colonne que
le moteur voit pleine (la surface reste « le premier y non solide » du haut). Le test de
`norrcraft/mod.rs` sur la « colonne pleine sous la surface » passe sur ses chunks échantillonnés,
mais un chunk avec faille aura de l'air sous `h − 1`.

Coût mesuré : 1,3 à 1,7 ms par chunk (budget 2,5 ; 0,38 avant) : les provinces (25 évaluations
par chunk), les distances de filons et les grilles de corps.

## Strate basse (session 2.4, doc 02 §9.3)

`flora.rs` : `Understory` (placement), `FloraBlocks` (ids lus dans le registre, rangés dans
`NorrBlocks::flora`), `Column`/`Pick`, `Ground` (nature du sol). 37 blocs de plantes, ids 80 à 116
(la 2.3 garde 54 à 79 pour troncs et feuillages), tous en croix, sans collision, sans lumière :
`blabar lingon hjortron smultron tranbar nypon havtorn ljung renlav skagglav kantarell karljohan
flugsvamp vitsippa blasippa midsommarblomster prastkrage blaklocka rodklover rallarros angsull
kvanne orkide fjallsippa isranunkel vass kaveldun nackros blastang sockertang hogt_gras ormbunke
ramslok glasort dvargbjork lupin mjodort` (`hogt_gras` et `ormbunke` sont teintés par l'herbe du
biome). Textures procédurales `norr_*` par `scripts/gen_norr_flora.py` (sprites 16 × 16 alpha ×2).

### Placement (doc 02 §9.2 transposé à la strate basse)

* Une **fiche** par plante (`SPECS`, 38 lignes : le roseau en a deux) : habitat (terre, rive, fond
  d'eau de `min..max` blocs selon l'eau douce/saumâtre/salée, surface d'eau douce), sols admis,
  densité par biome, longueur d'onde et seuil de la tache.
* Par colonne, dans l'ordre de la table (champignons rares et baies d'abord, `ormbunke` et
  `hogt_gras` en dernier) : `h(graine, x, z, fiche) < densité × tache`, avec
  `tache = sm(s − 0,1, s + 0,1, F(p/λ; 2))`. Le tirage de hachage passe avant le bruit (le fBm n'est
  évalué que sur les colonnes déjà retenues). Fonction pure de (graine, colonne) : indépendant de
  l'ordre des chunks.
* **Sol** : lu dans le bloc posé sous la plante (`gras`, `mossa`, `barrmatta`, `lovforna`,
  `lavhed`, `vitmossa`, `torv`, sables, graviers, roches…). Rien sur la roche nue (sauf algues), la
  neige, la glace d'un glacier ni la banquise ; rien là où `Selection::snow` est vrai.
* **Eau** : roseau et massette au fond des lacs de 1 à 2 blocs (roseau aussi sur la rive sèche
  voisine d'une eau douce), nénuphar flottant à la surface d'une eau douce d'au moins 2 blocs,
  fucus (1 à 6 blocs) et laminaire (3 à 18) au fond des mers. La plante remplace le bloc d'eau du
  fond (ou de la surface pour le nénuphar) : jamais d'air nouveau, donc l'étanchéité est intacte.
* La passe tourne **après** les arbres (2.3) : toute cellule non vide est occupée.

### Écarts au doc 02 §9.3

* **Saisons** : pas de « mûr/vert », pas de floraison de printemps ni de champignons d'automne
  seulement : tout est en pleine saison jusqu'à la phase 3.
* Un bloc par plante (un seul bloc `midsommarblomster` pour les 7 fleurs de la Saint-Jean, plus
  `prastkrage`, `blaklocka` et `rodklover` déjà séparées par le doc 04) ; **zostère**, lin et
  chanvre sauvages, cumin des prés, sphaigne (c'est le sol `vitmossa`), amanite préparée et
  champignons autres que les trois du doc 04 : pas de bloc, reportés.
* `glasort` (salicorne) pousse sur les rivages sableux ou argileux (Strandäng, Skärgård).
* Pas de plante de 2 blocs de haut (angélique, roseau) ; pas de décor de relief au-delà de la
  strate basse (le doc 02 §9.3 n'en décrit pas d'autre).
* Les rares : `orkide` (Alvar, Klintkust), `nypon`/`havtorn` (côtes) sont peu fréquentes près de
  l'origine faute de ces biomes ; elles existent (test d'atteignabilité de chaque fiche).

Seed `0x…0701` (la 2.3 prend `0x…06xx`, la 2.2 `0x…05xx`). Coût mesuré (release, graine 12345, 40 × 40 chunks, `time_per_chunk` ignoré) : 0,8 ms par chunk, strate basse comprise.

## Arbres (session 2.3, doc 02 §9.1 et §9.2)

Tout est dans `trees.rs` ; `terrain.rs` n'ajoute qu'un champ (`trees: Trees`), `tree_column`
(la colonne isolée vue par le placement) et un appel en fin de `generate_chunk`, **avant**
`place_understory` de la 2.4 (la strate basse traite tout bloc non-air comme occupé).

### Blocs (ids 54 à 75 ; 76 à 79 réservés, 80 et plus = strate basse)

11 bois (`gran_stam`, `tall_stam`, `tall_ovre` = haut orangé du pin, `bjork_stam`, `bok_stam`,
`ek_stam`, `asp_stam`, `al_stam`, `ronn_stam`, `dod_stam` = chicot, `mossig_stam` = souche et tronc
couché) et 11 feuillages teintés par le biome (`gran_barr`, `tall_barr`, `bjork_lov`, `bok_lov`,
`ek_lov`, `asp_lov`, `al_lov`, `ronn_lov`, `salg_lov`, `en_barr`, `hassel_lov`). Textures `norr_*`
provisoires : `scripts/gen_norr_trees.py` (écorce verticale, tirets du bouleau, cernes, feuillages
en gris teinté de luminosité propre à l'essence). Les noms diffèrent de ceux du doc 04 §9
(`gran` pour le tronc et le feuillage) : un suffixe `_stam` (tronc), `_lov` (feuille), `_barr`
(aiguille) lève l'ambiguïté. Le fjällbjörk réutilise `bjork_*` ; le saule, le genévrier et le
noisetier n'ont que du feuillage (+ `ronn_stam` pour les tiges). Le pin gagne `tall_ovre`, le
tilleul prend `bok_stam` + `hassel_lov`.

### Essences (§9.1)

`Species` : Gran, Tall, Bjork, Fjallbjork, Bok, Ek, Ronn, Asp, Al, Salg, En, Lind (tilleul 40 %,
noisetier 60 %). Chaque forme est une fonction pure du `seed` de la plante (hachage de
`(graine, cellule)`), dessinée en coordonnées relatives au pied par le trait `Put` (`wood` remplace
l'air et les feuilles, `leaf` ne se pose que sur l'air, `supported` pour les troncs couchés).

| Essence | Forme |
|---|---|
| Gran | cône `r = R · (1 − v)^0,8` (approché par `x^0,75`, pas de `powf`), verticilles tous les 2 blocs, branches tombantes ; hauteur 9 à 22 (biais vers les petits) ; géants de 19 à 25 sur 2 × 2 (5 % des épicéas d'Urskog) |
| Tall | fût nu 60 %, haut en `tall_ovre`, 2 ou 3 plateaux décalés avec branches ; **Twisted** (Skärgård, Klintkust, Fjord) : 5 à 9 blocs, tronc penché, 1 ou 2 plateaux ; **Dwarf** (Myr) : 3 à 5 |
| Bjork | tronc blanc fin, ellipsoïde allongé, rameaux pendants ; Dwarf (Brännfält, jeunes bouleaux) 3 à 5 |
| Fjallbjork | 2 à 4 tiges, marche biaisée vers le haut et dans le sens de θ (signe de θ, seuil 0,38), touffes ; Dwarf (toundra) 1 à 2 |
| Bok | fût lisse, 5 à 6 branches en éventail (16 directions en constantes), deux couches de dôme (5,8 et 4,2 de rayon) |
| Ek | colonisation d'espace simplifiée : 6 à 9 points d'attraction dans un ellipsoïde (5,5 × 2,6), branche au nœud le plus proche, boule de feuilles à chaque point ; vénérable 2 × 2 (10 % en Hagmark et Bokskog) |
| Ronn, Asp, Al, Salg, En | sphère de 2,3 ; colonne étroite (1,9 × 4,6) ; cône arrondi `3 · (1 − v)^0,75` ; boule (Dwarf 1,2) ; colonne de 2 à 4 ou coussin |

Bois mort : chicot (4 à 12 blocs de `dod_stam` avec chicots de branches), souche, tronc couché
moussu (4 à 9 blocs le long de x ou z, un bloc seulement là où le sol porte).

### Placement (§9.2)

Grille jitterée de pas `CELL = 5` : la position de la cellule `(i, j)` est `5i + 1 + ⌊3 h⌋` (donc
au moins 3 blocs entre deux pieds). Une cellule porte une plante si
`h < densité_biome × bosquet × limite_arbres × (1 − masque_tourbière)`, avec
`bosquet = 0,4 + 1,2 · F(p/90 ; 2)` et `limite_arbres = sm(y_a, y_a − 10, y)` (sauf pour Fjord,
Skärgård, Klintkust, Strandäng, Alvar, Myr, Fjällhed, Palsmyr, Lavafält : arbustes et toundra).
Densités et mélanges par biome : `profile()` (Urskog 0,56, Bokskog 0,46, Bergslag 0,40, Älv 0,42,
Sjöland 0,36, Högkust 0,32, Tallhed 0,30, Brännfält 0,30, Fjällbjörkskog 0,30, Skärgård 0,20,
Hagmark 0,13…). **Peuplements purs** : une cellule de Worley de 40 blocs favorise une essence du
mélange, avec un poids multiplié par `1 + 5·Vr`. **Bosquets clonaux** de trembles (Urskog, Älv) :
5 à 12 troncs dans un disque de 3,5 blocs. L'aulne exige de l'eau à moins de 4 blocs (12 sondes
brutes). Bois mort : part par biome (Urskog 8 %, Brännfält 62 %…).

Rejets : eau, barrage, niveau de la mer + 1, plage de dunes, glace, neige éternelle, pente ≥ 3
(différence de sol maximale avec les 4 voisines, lue sur le relief), biomes sans arbres (mers,
Kalfjäll, Glaciär).

### Indépendance de l'ordre des chunks

Un chunk évalue toutes les cellules dont une plante peut le toucher (`MAX_REACH = 9` blocs ; tout
bloc dessiné au-delà est ignoré par construction) dans l'ordre global (z puis x croissants) et ne
dessine que dans son volume. Un bloc voit donc toujours la même suite d'arbres : troncs et feuilles
se recouvrent de la même façon quel que soit le chunk généré en premier. Le résultat d'une cellule
est **mémoïsé** dans un cache partagé par les copies du générateur (`Arc<Mutex<HashMap>>`, vidé à
65 536 cellules), comme le cache de biomes : il ne change aucun résultat. La colonne d'une cellule
est calculée d'un coup par `tree_column` (champs, 5 reliefs, eau scellée, biome) : 28 µs par cellule
(118 µs avec des appels séparés). Coût total mesuré : 1,4 ms par chunk à froid avec la strate
basse (2,5 ms budgétés).

Le départ du joueur (`find_spawn`) évite les colonnes à moins de 6 blocs d'une plante.

### Écarts au doc 02 §9

* **Une seule grille** de pas 5 au lieu d'un pas par essence (5 et 9) : la rareté du bocage vient
  de la densité (0,13), pas du pas. Un biome ne change donc jamais de grille à la frontière.
* **Géants de 2 × 2** et chênes vénérables : tirés à 5 % et 10 % des cellules d'Urskog, Hagmark et
  Bokskog. Les « vitesses » de croissance, baies de sorbier en automne et or du tremble sont du
  ressort de la phase 3 (saisons).
* **Chaos de blocs** (Urskog, Högkust) et structures : phase 4. Le rejet « structure » du §9.2
  n'a rien à tester pour l'instant.
* Les membres d'un bosquet de trembles ne sont pas testés pour la pente individuellement (écart de
  sol ≤ 2 avec le centre).
* Le pin « tordu » se penche dans une direction tirée au hasard, pas encore sous le vent.
* Aperçus : `apercu-norr/2.3/` (hêtraie, forêt primaire, pinède, bocage, boulaie de montagne).

## Outil `worldmap`

```
cargo run --release -p nb-world --example worldmap -- <graine> <préréglage> <cx> <cz> <taille> <pas> <fichier.png> [mode]
```

`préréglage` : `norrland` | `skargard` | `hogfjall` ; `taille` en pixels, `pas` en blocs par pixel ;
`mode` : `relief` (défaut depuis la 1.2 : relief ombré, teintes hypsométriques, mer par
profondeur) | `hydro` (1.4 : relief ombré après l'hydrographie, mer par profondeur, lacs en
turquoise, rivières en bleu vif, élargies aux grands pas pour rester visibles) | `w` (1.4 : terres
teintées par marche de l'escalier W, eau comme `hydro`) | `h` (altitude en fausses couleurs) | `formes` (1.3 : relief ombré et formes en couleur : îlots corail, klint crème, côte soulevée rouge, dunes sable, pré salé vert d'eau, eskers jaunes, marmites cyan, drumlins vert clair, tourbières brunes, palses violettes, glace blanche, moraines brun clair, lave rouge ; l'outil imprime aussi leurs parts) | `biomes` (2.1 : une couleur par biome, sélection exacte de chaque colonne échantillonnée, trame comprise ; eau douce bleutée, neige éternelle blanche ; l'outil imprime la répartition face aux cibles du doc 02 §7.5 et la distance du plus proche représentant de chaque biome à l'origine) | `coupe` (2.2 : coupe verticale le long de x à `z = cz`, blocs réellement générés, 3 px par bloc ; le pas est en blocs par colonne) | `geologie` (2.2 : provinces rocheuses ombrées, mer, filons de diabase au pas ≤ 4, corps de minerai en carrés : Malmberg rouge, cuivre orange, cobalt bleu, tungstène noir, stéatite cyan, grotte marine jaune ; l'outil imprime les parts de provinces et le Malmberg le plus proche) | `fjord` (relief ombré ; fjords en magenta, fonds
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
* `nb-world/tests/norrcraft_biomes.rs` (2.1) : profil à 25 biomes et teintes moyennées ;
  `biome_at` (cache, chunks générés ou non) = `column_biome` colonne par colonne ; **répartition**
  sur 16 km pour 3 graines dans ±40 % des cibles du §7.5 (exemptions ci-dessus) ; chaque biome
  présent (à moins de 8 km, la banquise au nord) ; trame présente mais sous 5 % de colonnes
  isolées ; peaux des sols (Hagmark, Urskog, Sjöland, Tallhed, Bergslag) ; banquise gelée hors
  des chenaux. `nb-mesh/tests/mesher.rs` : moyenne des teintes sur 5 × 5 colonnes (6 teintes à une
  frontière) et maillage inchangé sans moyenne. Les tests `sealed.rs` et `universe.rs` (nb-game)
  attendent désormais 25 biomes Norrcraft, `palette.rs` (nb-data) les 27 blocs ajoutés à une
  vieille palette. En 2.1, les empreintes du terrain (`norrcraft_relief.rs`) ont changé (sols).
* `nb-world/tests/norrcraft_subsoil.rs` (2.2) : les 20 blocs existent ; **strates** (bedrock en y ≤ 1,
  djupberg sous y = 14, roche de province au-dessus) pour 3 graines ; parts de provinces sur la
  terre (gneiss 10 à 40 %, chaque province présente, gneiss dans la chaîne) ; **filons de diabase**
  verticaux de largeur médiane 2 ; un **Malmberg** à moins de 5 km de l'origine pour chaque graine,
  de plus de 20 000 blocs de fer, avec cœur riche et affleurement ; minerais dans leur gisement
  (myrmalm en Myr, sjömalm en Sjöland, Tallhed ou Bottenhav, krita et flinta à Klintkust) ; chaque
  espèce de corps (cuivre, cobalt, tungstène, stéatite, grotte marine, salle) existe dans 16 km et
  contient son minerai ; fer de roche en pays minier, argent dans le calcaire, quartz et or dans le
  nord ; **étanchéité** (aucune eau avec de l'air à côté ni dessous) autour d'un karst, de failles
  (ouvertes sur le ciel), de grottes marines et de salles, d'un champ de lave (tubes de lave et
  soufre) ; chunk indépendant de l'ordre de génération ; coût par chunk (< 6 ms, mesuré 1,3 à
  1,7). `nb-world/tests/norrcraft_relief.rs` : empreintes du terrain mises à jour (blocs de
  province). `nb-game/tests/universe.rs` : djupberg (ou diabase) à y = 10.
* Toute modification d'une formule ou d'une constante de champ change les empreintes : les mettre
  à jour consciemment (et le dire dans le compte rendu). En 1.2, le réglage du biais de C a changé
  les empreintes des champs (`FINGERPRINTS`, `REF0`, `REF1` : seul C bouge). Les empreintes vanilla
  (`tests/fingerprints.rs`) et l'étanchéité (`nb-game/tests/sealed.rs`) restent verts. En 1.3,
  seules les empreintes du terrain (`norrcraft_relief.rs`) changent ; celles des champs non.
* `nb-world/tests/norrcraft_flora.rs` (2.4) : chunks identiques quel que soit l'ordre de génération
  et d'une copie du générateur à l'autre (3 graines), plantes toujours posées sur un sol (jamais sur
  la neige, la glace, la banquise ni dans l'air ; seul le nénuphar flotte), plantes spécialisées
  seulement dans leurs biomes, chaque plante vue sur la grille ±1600 blocs pour 3 graines (hors
  quelques plantes de biomes rares près de l'origine) ; unitaires dans `flora.rs` (blocs
  enregistrés, en croix, non solides ; toutes les fiches atteignables).
* `nb-world/tests/norrcraft_trees.rs` (2.3) : chunks identiques quel que soit l'ordre de génération
  (avant, arrière), le cache (instance neuve) et la copie du générateur (3 graines) ; **couronnes
  entières** : aucun bloc de tronc ou de feuille dessiné par une plante ne manque dans le monde
  (plus de 250 000 blocs, 950 plantes à cheval sur une bordure de chunk) ; règles du sol (plantes sur
  terre sèche au-dessus de la mer, sans pente raide, hors neige, sous la limite des arbres, jamais
  en mer, sur le Kalfjäll ni sur la glace) ; essences par biome (épicéa dominant en Urskog avec bois
  mort 4 à 13 %, pin seul en Tallhed, hêtre en Bokskog, rien en toundra que saules et bouleaux
  nains, les 12 essences présentes) ; densités ordonnées ; coût (< 5 ms, mesuré 1,5). Unitaires
  dans `trees.rs` (formes dans la portée et déterministes, tailles par essence, cône de l'épicéa,
  troncs 2 × 2). Les empreintes de `norrcraft_relief.rs` changent (arbres et strate basse) ;
  `norrcraft_relief.rs`, `norrcraft_hydro.rs` et `mod.rs` acceptent un tronc, une feuille ou une
  plante au-dessus du sol.
