# Génération du monde « Norrblock »

Depuis la phase 8, le monde est produit par un nouveau générateur (`GeneratorKind::Norrblock`,
défaut des nouveaux mondes). Le générateur Java d'origine reste disponible (`Classic`, type de
monde « Classic » dans l'interface) : c'est lui que valident les vecteurs de référence.

Code : `crates/nb-world/src/terrain.rs` (relief + biome), `generator_nb.rs` (blocs, strates,
grottes, minerais, arbres, décors, végétation), `biome.rs` (30 biomes, teintes, mélanges d'arbres).

## Principes

* **Relief continu.** Le Java choisissait le biome puis multipliait la hauteur par un facteur
  propre au biome : à chaque frontière, une marche. Ici, la hauteur est une fonction continue de
  champs de bruit (érosion, masse montagneuse, côte, rivières, lacs, marais, mesas, dunes,
  volcans) ; le biome n'est que l'étiquette posée ensuite. Plus de falaise artificielle
  (test `terrain_is_continuous_between_neighbours`).
* **Climat étalé.** Les fBm sont très concentrés autour de 0,5 (le Java donnait surtout des
  forêts). Température, humidité, « étrangeté » (variante au sein d'une zone), érosion et
  montagnes sont étalés sur [0, 1] par une fonction de répartition approchée (sans `exp`), puis
  les coordonnées sont déformées par un bruit lent : frontières organiques, régions de
  plusieurs centaines de blocs. Les curseurs « Temperature/Humidity Scale » gardent leur sens.
* **Pur et reproductible.** `(graine, config, x, z)` → colonne. Le mailleur (teintes) et le
  générateur voient exactement le même biome. Aucune fonction transcendante : même résultat
  sur toutes les plateformes.
* **Décors indépendants des chunks.** Arbres, rochers, pics de glace, récifs… sont placés sur une
  grille jitterée par hachage ; chaque chunk recalcule aussi ceux des cases voisines qui
  débordent chez lui (la hauteur du sol est redemandée au terrain). Un arbre à cheval sur deux
  chunks est identique quel que soit l'ordre de génération, et les grands arbres (jusqu'à 6
  blocs de rayon) ne sont plus limités au centre du chunk comme dans le Java.

## Biomes (30)

Les 8 premiers ordinaux sont ceux du Java ; `Biome::family()` les rattache aux 8 curseurs de
densité d'arbres de l'écran de création.

| Zone | Biomes |
| --- | --- |
| Eau | Ocean, DeepOcean, WarmOcean (coraux), FrozenOcean (icebergs), River |
| Rivages | Beach (palmiers au chaud), StonyShore |
| Froid | SnowyPlains, IceSpikes (pics de glace), SnowyTaiga, Taiga, MegaTaiga (épicéas géants, podzol), Heath (lande : lavande, buissons, rochers moussus) |
| Tempéré | Plains, FlowerMeadow (parterres monochromes), Forest, BirchForest, DarkForest (chênes noirs 2×2, champignons géants), AutumnForest, CherryGrove (cerisiers roses), Swamp |
| Chaud / sec | Savanna, Jungle (jungle géante 2×2), Desert (dunes), Badlands (mesas en terrasses, strates de terracotta, canyons de rivière) |
| Relief | Hills, Mountains, SnowyPeaks, Volcano (cône, cratère, lac de lave, cendres et basalte) |
| Rare | MushroomFields (mycélium, champignons géants) |

## Autres nouveautés de génération

* **Fonds marins** : plateau continental de largeur constante pour tous les préréglages ; sable,
  gravier et argile par plaques ; récifs de corail dans les océans chauds ; icebergs.
* **Rivières** déformées (les zéros d'un bruit de Perlin forment des segments rectilignes alignés
  sur les axes), largeur modulée par région, estompées en montagne ; **lacs** et **marais**.
* **Sol** : l'herbe ne recouvre plus les falaises (pente ≥ 4 : pierre/gravier), neige seulement sur
  les pentes douces, podzol/mousse par plaques, sable uniquement au contact de l'eau.
* **Sous-sol** : ardoise en profondeur, poches de marbre et de basalte ; minerais en filons de
  2×2×2 (même quantité moyenne) ; grottes en tunnels sinueux et salles profondes, avec une
  croûte de 5 blocs (le Java ouvrait un tiers des colonnes en puits sur le ciel).
* **Arbres** : mélanges pondérés par biome (chêne, grand chêne à branches, bouleau, épicéa,
  épicéa géant, jungle, jungle géante, acacia à deux branches, chêne des marais, chêne noir,
  cerisier, palmier, arbre mort, buisson, champignons géants) ; bosquets (densité modulée par un
  bruit lent) ; troncs tombés, rochers moussus.
* **Plantes** : 5 fleurs de plus (bleuet, tulipes rose et orange, marguerite) et lavande,
  roseaux au bord de l'eau, nénuphars des marais.
* **Blocs ajoutés** (33, identifiants 35 à 67) : gravier, sable rouge, 6 terracottas, glace,
  glace compacte, podzol, mycélium, basalte, marbre, ardoise, cendre, magma (émet de la
  lumière), bois de cerisier et de chêne noir, champignons géants (pied, 2 chapeaux),
  4 coraux, 5 fleurs/lavande, roseaux, mousse.

## Types de monde

Normal, Flat (plat, mais avec les biomes), Amplified, Island, **Archipelago** (beaucoup d'îles
sous les tropiques), **Highlands** (hautes terres), **Classic** (terrain Java, 8 biomes).
Amplified et Island sont réajustés pour Norrblock (`WorldConfig::preset`) ; `preset_java` garde
les réglages d'origine pour les vecteurs de référence.

## Compatibilité

Un `world.json` sans champ `generator` (monde créé avant) est lu comme `Classic` : les chunks
non encore explorés continuent le terrain d'origine, sans raccord visible.
Les nouveaux blocs n'existent pas dans les anciennes sauvegardes (aucun risque : les chunks
déjà écrits ne contiennent que les anciens identifiants).

## Outils de réglage

```sh
# Carte des biomes + relief ombré (PPM) et répartition en % ; « h » en 8e argument : hauteurs
cargo run --release -p nb-world --example worldmap -- <graine> <type> <x> <z> <taille> <pas> carte.ppm
# Vue de dessus des blocs réellement générés ; « coupe » en 7e argument : coupe verticale
cargo run --release -p nb-world --example blockmap -- <graine> <type> <x0> <z0> <taille> blocs.ppm
```
(`type` : normal, flat, amplified, island, archipelago, highlands, classic.)

Mesures (graine 12345, Normal, 8 000 × 8 000 blocs) : océans ≈ 26 %, rivières ≈ 9 %, plages ≈ 7 %,
aucun biome au-dessus de 10 %. Génération d'un chunk ≈ 1,6 ms (dév. optimisé, 1 thread).

## Textures

Les blocs ajoutés ont des textures procédurales originales (`scripts/gen_textures.py`, 32×32,
graine fixe), nommées comme celles du pack Faithful 32x : `scripts/import_faithful.py` peut les
remplacer par les textures de Faithful à partir d'un zip téléchargé (licence : crédit obligatoire,
voir `resource_packs/Default/CREDITS-Faithful.txt` une fois importé).
