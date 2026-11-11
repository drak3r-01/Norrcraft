# Worldgen Norrcraft : champs macroscopiques (1.1) et relief (1.2)

Le vrai relief Norrcraft (GDD doc 02 §3 à §5.4 et §12 ; plan 06, sessions 1.1 et 1.2). La 1.1 a
posé les briques mathématiques et les champs lents ; la 1.2 construit dessus la fonction de hauteur
et remplace le monde plat de granit de la phase 0 par un vrai terrain. Tout vit dans
`crates/nb-world/src/norrcraft/`.

## Modules

| Fichier | Contenu |
|---|---|
| `math.rs` | hachage SplitMix64 (`hash64`, `hash01`), utilitaires (`sigmoid`, `spread` = q, `smooth` = sm, `bump`, `soft_cap`), `GradField` (N, fBm, crête `ridged`, ligne de zéro, `warp`), `Worley` (F1, F2, cellule, point) |
| `config.rs` | `NorrConfig` (préréglage, niveau de mer 40, hauteur 128 : décision D3), `NorrPreset` (Norrland, Skärgård, Högfjäll) |
| `fields.rs` | `MacroFields::sample(x, z)` → `MacroSample { c, o, t0, hm, u, e, lk, gt, vr, theta }` ; `province(x, z)` ; `ChunkFields` (treillis 4 × 4) |
| `climate.rs` | `ClimateZone` (arctique … némorale), `altitudes` (limites des arbres, de la roche nue, des neiges), `temperature_at` |
| `height.rs` (1.2) | `HeightFields` : `column(wx, wz)` (treillis, identique aux chunks), `relief_exact(x, z)` (cartes), `relief(x, z, &MacroSample)` → `Relief { h, socle, land, chain, before_valleys, plateau, plateau_level, valley, valley_floor, hanging, fjord }` ; `socle(C)`, `soft_floor` |
| `terrain.rs` (1.2) | `NorrcraftTerrain` (`TerrainGenerator`) et `NorrBlocks` ; `WorldProfile::norrcraft(graine, NorrConfig, &registre)`, utilisé par `World::for_kind` |

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

## Outil `worldmap`

```
cargo run --release -p nb-world --example worldmap -- <graine> <préréglage> <cx> <cz> <taille> <pas> <fichier.png> [mode]
```

`préréglage` : `norrland` | `skargard` | `hogfjall` ; `taille` en pixels, `pas` en blocs par pixel ;
`mode` : `relief` (défaut depuis la 1.2 : relief ombré, teintes hypsométriques, mer par
profondeur) | `h` (altitude en fausses couleurs) | `fjord` (relief ombré ; fjords en magenta, fonds
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
  mesure de temps ignorée par défaut (`-- --ignored`).
* Toute modification d'une formule ou d'une constante de champ change les empreintes : les mettre
  à jour consciemment (et le dire dans le compte rendu). En 1.2, le réglage du biais de C a changé
  les empreintes des champs (`FINGERPRINTS`, `REF0`, `REF1` : seul C bouge). Les empreintes vanilla
  (`tests/fingerprints.rs`) et l'étanchéité (`nb-game/tests/sealed.rs`) restent verts.
