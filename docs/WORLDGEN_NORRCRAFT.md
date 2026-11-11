# Worldgen Norrcraft : briques et champs macroscopiques (session 1.1)

Première étape du vrai relief Norrcraft (GDD doc 02 §3, §4 et §12 ; plan 06, session 1.1). Elle ne
change aucun chunk : le monde Norrcraft reste le monde plat de granit de la phase 0 jusqu'à la
session 1.2, qui construira la hauteur sur ces champs. Tout vit dans `crates/nb-world/src/norrcraft/`.

## Modules

| Fichier | Contenu |
|---|---|
| `math.rs` | hachage SplitMix64 (`hash64`, `hash01`), utilitaires (`sigmoid`, `spread` = q, `smooth` = sm, `bump`, `soft_cap`), `GradField` (N, fBm, crête `ridged`, ligne de zéro, `warp`), `Worley` (F1, F2, cellule, point) |
| `config.rs` | `NorrConfig` (préréglage, niveau de mer 40, hauteur 128 : décision D3), `NorrPreset` (Norrland, Skärgård, Högfjäll) |
| `fields.rs` | `MacroFields::sample(x, z)` → `MacroSample { c, o, t0, hm, u, e, lk, gt, vr, theta }` ; `province(x, z)` ; `ChunkFields` (treillis 4 × 4) |
| `climate.rs` | `ClimateZone` (arctique … némorale), `altitudes` (limites des arbres, de la roche nue, des neiges), `temperature_at` |

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
  (−9000 : −0,40 ; −6500 : −0,18 ; −4500 : +0,08 ; −2000 et +2500 : +0,17 ; +5500 : +0,02 ;
  +9000 : −0,05), constante au-delà (le monde reste infini). Le noyau surélevé évite que le
  centre soit criblé de mers intérieures (sans lui : 46 % de mer sur 16 km, avec : 40 %, pour une cible de 30 % ±40 %).
* **Golfe oriental peu profond** : à l'est, C est de plus tiré vers 0,42 (plateau continental,
  force 0,55 × poids lissé entre x = 3500 et 8000). La mer reste peu profonde, des îles émergent
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

## Outil `worldmap`

```
cargo run --release -p nb-world --example worldmap -- <graine> <préréglage> <cx> <cz> <taille> <pas> <fichier.png> [mode]
```

`préréglage` : `norrland` | `skargard` | `hogfjall` ; `taille` en pixels, `pas` en blocs par pixel ;
`mode` : `overview` (défaut : mer par profondeur, terre par zone climatique, gris des reliefs,
rouge = lave) | `c` | `o` | `t0` | `hm` | `u` | `e` | `lk` | `gt` | `vr` | `zones` | `pv` | `theta`.
La sortie est un PNG sans dépendance (`image_out.rs` ; `.ppm` reste possible). L'outil imprime
aussi les parts de terre, de zones climatiques et de provinces, et les quantiles de chaque champ.
Les cartes vanilla fonctionnent comme avant (`<type>` = normal, flat, classic…). Avec Docker :
`NB_TARGET_VOL=nb-target-<nom> sh scripts/cargo-docker.sh run --release -q -p nb-world --example worldmap -- …`
(les images sont écrites dans le dossier du dépôt, ex. `apercu-norr/`, ignoré par git).

Repères mesurés (graine 12345, Norrland, 16 × 16 km autour de l'origine) : terre 60 % (cible doc
§7.5 : mers 30 %, tolérance ±40 %), zones : subarctique 21 %, boréale 25 %, hémiboréale 42 %,
némorale 12 % ; l'arctique n'apparaît qu'à plus de 16 km au nord. À l'origine : terre (C = 0,67),
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
* Toute modification d'une formule ou d'une constante de champ change les empreintes : les mettre
  à jour consciemment (et le dire dans le compte rendu). Les empreintes vanilla
  (`tests/fingerprints.rs`) et l'étanchéité (`nb-game/tests/sealed.rs`) restent verts.
