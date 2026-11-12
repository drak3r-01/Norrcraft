# Bestiaire terrestre de Norrcraft (session 5.4)

Référence : GDD doc 03 §3.1 (herbivores et petits mammifères) et §3.2 (prédateurs terrestres),
§6 (critères). Premier lot d'espèces du bestiaire : 19 fiches qui s'appuient sur le moteur d'IA
(5.1), les milieux et l'apparition par densité (5.2) et les hardes (5.3). Les lots oiseaux (5.5),
aquatique, amphibie et insectes (5.6) et domestiques (5.7) suivent.

## Où est quoi

| Fichier | Rôle |
| --- | --- |
| `scripts/gen_norr_mobs.py` | Générateur des trois fichiers ci-dessous (formes et recettes de vanilla : `mob_shapes.py`, `mob_recipes.py`) |
| `assets/norrcraft/mobs.json` | 19 `MobConfig` avec `behavior` et bloc `spawn` |
| `assets/norrcraft/mob_geometry.json` | Boîtes colorées (pixels MC, 16 px = 1 bloc, museau vers +Z) |
| `assets/norrcraft/mob_rigs.json` | Squelettes et clips (marche, course, repos, broutage ou reniflement, regard, blessure, attaque) |
| `assets/norrcraft/behaviors.json` | 12 profils de comportement ajoutés aux 10 de la 5.1 |
| `crates/nb-data/src/mobs.rs` | `MobRegistry::builtin_for(Norrcraft)` charge ces fichiers ; `norr_mob_ids()` |
| `crates/nb-ecs/src/spawn.rs` | `spawn_column_y` (tests d'habitat) ; une espèce à règles ne naît plus sur une canopée |
| `crates/nb-app/src/driver.rs` | Au changement d'univers (`RebuildBlockTable`), les modèles des mobs manquants sont chargés dans le renderer |

Régénérer : `MSYS_NO_PATHCONV=1 docker run --rm -v "$(pwd -W):/w" -w /w rust:1 python3 scripts/gen_norr_mobs.py`.
Les identifiants sont suédois : aucun ne coïncide avec un mob vanilla ni avec une forme de `nb-gfx`
(`sealed.rs`, `palette.rs`).

## Les 19 espèces

| id | Animal | Profil | Milieu | Apparition (densités pour 64 × 64) | Particularités de la fiche |
| --- | --- | --- | --- | --- | --- |
| `alg` | Élan | `betare` | terre | Urskog, Myr, Sjöland 0,10 ; Brännfält 0,14 ; Älv 0,06 ; Fjällbjörkskog 0,05 ; crépusculaire | rut agressif, charge pour le veau, 30 PV |
| `ren` | Renne | `hjord` | terre | **harde persistante** `seasonal` : été Fjällhed, Fjällbjörkskog, Palsmyr ; hiver Tallhed, Urskog ; mai et octobre ; 8 à 20 | 16 PV, suit un chef |
| `radjur` | Chevreuil | `skygg` | terre | Bokskog, Hagmark 0,20 ; Alvar 0,14 ; Strandäng 0,04 | très craintif, 8 PV |
| `myskoxe` | Bœuf musqué | `myskdjur` | terre | Fjällhed 0,03, groupes de 3 à 6 | menace puis charge, 40 PV |
| `snolopare` | Lièvre variable | `smavilt` | terre | Fjällbjörkskog, Tallhed 0,25 ; Urskog, Hagmark, Alvar 0,20 ; Fjällhed 0,18 ; Brännfält 0,10 | pelage blanc de novembre à avril (état `winter_coat`), terrier |
| `fjallammel` | Lemming | `gnagare` | terre | Fjällhed 0,60 ; Palsmyr 0,40 ; Kalfjäll 0,15 | cycle de 4 ans (× 0,2 / 1 / 4 / 0,5) |
| `baver` | Castor | `byggare` | amphibie | Älv, Sjöland 0,22, sédentaire (`resident`) | fenêtre de construction juillet à septembre |
| `ekorre` | Écureuil roux | `traklattrare` | terre | Urskog, Bokskog 0,40 ; Tallhed 0,30 | diurne, 2 PV |
| `igelkott` | Hérisson | `nattkrypare` | terre | Hagmark 0,26 ; Bokskog 0,20 ; Alvar 0,10 ; nocturne | hiberne de novembre à mars |
| `hermelin` | Hermine | `vessla` | terre | Fjällhed, Hagmark 0,20 ; Alvar 0,10 ; Fjällbjörkskog 0,05 | chasse les lemmings, **suit le cycle avec un an de retard**, blanche en hiver |
| `varg` | Loup | `flockjagare` | terre | **harde persistante** `territory`, 3 à 7 par meute, territoire Urskog, Tallhed, Fjällbjörkskog, Myr, Bergslag | encercle puis attaque, évite feu et villages |
| `bjorn` | Ours brun | `allatare` | terre | Urskog 0,040 ; Fjällbjörkskog, Bergslag 0,030 ; Älv 0,025 ; absent de novembre à mars | hiberne dans sa tanière, avertit avant de charger, 45 PV |
| `lo` | Lynx | `ahus` | terre | Högkust 0,030 ; Urskog 0,020 ; Fjällbjörkskog 0,010 | affût (`stalk`), fuit dès qu'il est vu, **ne mord jamais le joueur** (il ne le chasse pas) |
| `jarv` | Glouton | `rovtjuv` | terre | Fjällbjörkskog, Fjällhed 0,030 ; Kalfjäll 0,010 | charognard et voleur (`scavenge`, `steal`), féroce acculé |
| `fjallrav` | Renard polaire | `fjallrav` | terre | Fjällhed, Kalfjäll 0,10 ; Packis 0,08 ; Lavafält 0,05 ; Glaciär 0,03 | pelage blanc, suit le cycle des lemmings avec un an de retard |
| `rodrav` | Renard roux | `rav` | terre | Hagmark 0,12 ; Bokskog, Alvar 0,10 ; Sjöland 0,08 ; Strandäng 0,06 | vole la nourriture, chasse lièvres et rongeurs |
| `mard` | Martre | `mardjakt` | terre | Urskog 0,12 ; Bokskog 0,02 | chasse l'écureuil |
| `isbjorn` | Ours blanc | `isrovdjur` | **glace** | Packis 0,030 | traque le joueur (odorat 160), seul animal à le chasser |
| `huggorm` | Vipère péliade | `orm` | terre | Tallhed, Alvar 0,15 ; Skärgård, Myr 0,10 ; printemps à automne, plutôt par temps sec | mord quand on la touche, hiberne d'octobre à avril |

## Profils ajoutés (`behaviors.json`)

`skygg` (chevreuil), `myskdjur` (bœuf musqué : harde, courage 0,85, menace puis charge),
`traklattrare` (écureuil), `nattkrypare` (hérisson : hibernation, terrier), `vessla` (hermine),
`ahus` (lynx), `rovtjuv` (glouton), `fjallrav`, `rav` (renard roux), `mardjakt` (martre),
`isrovdjur` (ours blanc : `hunts` contient `player`), `orm` (vipère : vue de 5 blocs, chasse le
joueur à portée de morsure). `smavilt` craint désormais les nouveaux petits prédateurs et
`flockjagare` chasse aussi `skygg`.

## Écarts au document

* **Formation en cercle du bœuf musqué** : pas d'état dédié dans le moteur ; le profil `myskdjur`
  se contente de menacer puis de charger quand on approche.
* **Arboricolité de l'écureuil et de la martre, tunnels sous la neige du lemming, voix des
  chevreuils, hurlements des loups** : non simulés (le rig du loup joue déjà son clip de
  hurlement à l'arrêt).
* **Pelage de saison** : l'état `winter_coat` du `Brain` existe (blanc de novembre à avril pour le
  lièvre, l'hermine, le renard polaire) mais le rendu des mobs est statique : une seule teinte par
  modèle (brun ou gris d'été pour l'hermine et le lièvre, gris clair pour le renard polaire).
* **Venin de la vipère** (phase 6) : la morsure fait des dégâts directs.
* **Protection des potagers, vol dans les séchoirs et garde-manger, castor qui abat des arbres** :
  les événements `AnimalAct` sortent mais rien ne modifie le monde avant la 5.8.
* **Rennes apprivoisables, bât, traîneau** : 5.7.
* **Butin saisonnier et dépeçage** : 5.8 ; les fiches n'ont pas de butin Norrcraft (le butin de
  survie est celui de vanilla).
* Les modèles sont des boîtes colorées dérivées des archétypes vanilla (quadrupède, hérisson,
  lézard pour la vipère : ses pattes sont réduites à des points) : les vraies textures et modèles
  viendront de Gabriel. Aucun aperçu PNG n'a été produit (pas de Pillow dans l'image Docker).
* Une colonne couverte de plantes de sous-bois n'est pas un point d'apparition (`find_surface_y`
  exige de l'air au-dessus du sol) : les 19 espèces trouvent tout de même des milliers de points.
* La banquise (Packis) est loin de l'origine : le test la cherche jusqu'à 20 km, et l'ours blanc
  n'apparaît qu'au loin.

## Tests

| Commande | Contenu |
| --- | --- |
| `cargo test -p nb-data --test norr_mobs` | 19 fiches, profils et règles valides, biomes existants, hardes, PV, ours et vipères absents l'hiver, prédateurs en retard d'un an |
| `cargo test -p nb-data --lib mobs` | rig sain pour chaque modèle (valeurs finies, pieds au sol) |
| `cargo test -p nb-ecs --test bestiary_land_scenarios` | 8 scénarios avec les fiches réelles : cerveau et profil, meute qui encercle puis tue un chevreuil, lynx qui fuit sans mordre, vipère qui mord, ours blanc qui traque jusqu'à l'attaque, ours brun absent l'hiver, lemmings et prédateurs, pelage blanc |
| `cargo test -p nb-ecs --test bestiary_land_norr` | 19 espèces dans 3 mondes générés (~40 s) : au moins 100 colonnes valides chacune (de 458 pour l'ours blanc à 5 808 pour le lièvre) |
