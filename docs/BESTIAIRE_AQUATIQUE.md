# Bestiaire aquatique, amphibie et insectes de Norrcraft (session 5.6)

Référence : GDD doc 03 §3.4 et §3.5. Vingt-trois fiches qui s'appuient sur le moteur d'IA (5.1), les
milieux et l'apparition par densité (5.2). Avec les 19 espèces terrestres (`BESTIAIRE_TERRESTRE.md`)
et les 15 oiseaux (`BESTIAIRE_OISEAUX.md`), le bestiaire Norrcraft compte 57 espèces.

## Où est quoi

| Fichier | Rôle |
| --- | --- |
| `scripts/gen_norr_mobs.py` | Section « Faune aquatique, amphibie et insectes » : fiches, géométries (formes `fish`, `jelly`, `crab`, `quad`, `biped`, `lizard` de vanilla) et rigs |
| `assets/norrcraft/{mobs,mob_geometry,mob_rigs}.json` | Régénérés par le script (même commande que la 5.4) |
| `assets/norrcraft/behaviors.json` | 16 profils ajoutés : `lax stim skrei gaddhane kraftdjur manet sal valross delfin utter djuphav val insekt skymning lysmask` (le poisson commun `fisk` de la 5.1 sert à la truite, l'omble, la perche et l'anguille) |
| `crates/nb-data/tests/norr_aquatic.rs` | Cohérence des fiches (PV, milieu, profils, saisons, agressifs, profondeur) |
| `crates/nb-ecs/tests/bestiary_aquatic.rs` | Scénarios : poissons dans l'eau, brochet qui chasse la perche, phoque qui fuit, morse qui menace, insectes qui volent |

## Les 17 espèces aquatiques et amphibies

| id | Animal | Milieu | Profil | Apparition (densités pour 64 × 64) | Particularités |
| --- | --- | --- | --- | --- | --- |
| `lax` | Saumon | eau | `lax` | Älv 0,30, Bottenhav 0,12, Västerhav 0,06, Fjord 0,05 ; juin à septembre | bandes de 3 à 6, absent l'hiver, état `migrate` de juin à septembre |
| `oring` | Truite | eau | `fisk` | Älv 0,35, Sjöland 0,20, Fjällbjörkskog 0,08 | |
| `roding` | Omble chevalier | eau | `fisk` | Fjällhed 0,18, Kalfjäll 0,12, Fjällbjörkskog 0,10 | seul poisson des lacs d'altitude |
| `gadda` | Brochet | eau | `gaddhane` | Sjöland 0,22, Bottenhav 0,08, Älv 0,05 | **affût** (`stalk`), chasse perche, hareng et saumon ; mord (1,5) si on le provoque |
| `abborre` | Perche | eau | `fisk` | Sjöland 0,40, Bottenhav 0,20, Älv 0,10 ; bancs de 3 à 7 | |
| `stromming` | Hareng de la Baltique | eau | `stim` | Bottenhav 0,60, Skärgård 0,40, Västerhav 0,30 ; **bancs de 6 à 14**, jusqu'à 90 | fuit phoques, marsouins, oiseaux de mer |
| `torsk` | Morue | eau | `skrei` | Fjord 0,25, Västerhav 0,12, Högkust 0,05 ; **pic de décembre à mars** | skrei : remonte dans les fjords en fin d'hiver |
| `al_fisk` | Anguille | eau | `fisk` | Bokskog 0,15, Älv 0,08, Högkust 0,06, Strandäng 0,05, Sjöland 0,04 ; nocturne, été | |
| `krafta` | Écrevisse | amphibie | `kraftdjur` | Sjöland, Älv 0,20 ; nocturne, d'avril à septembre | attirée par les charognes |
| `brannmanet` | Méduse à crinière | eau | `manet` | Västerhav, Fjord 0,10, Skärgård 0,05 ; **fin d'été** | brûlure au contact (1 dégât toutes les 2 s) |
| `grasal` | Phoque gris | amphibie | `sal` | Skärgård 0,22, Klintkust 0,06, Västerhav 0,05 ; groupes de 2 à 5 | dort sur les écueils, plonge à l'approche |
| `vikare` | Phoque annelé | glace | `sal` | Packis 0,06, Bottenhav 0,04 ; plus nombreux l'hiver | |
| `valross` | Morse | glace | `valross` | Packis 0,03 ; groupes de 3 à 6 | menace puis **charge** (5 dégâts) qui s'approche ; garde le groupe |
| `tumlare` | Marsouin | eau | `delfin` | Västerhav 0,08, Bottenhav 0,05, Fjord 0,04 ; eau d'au moins 3 blocs | groupes de 2 à 5, diurne |
| `utter` | Loutre | amphibie | `utter` | Älv 0,15, Sjöland 0,12, Fjord 0,08 ; crépusculaire | chasse poissons et écrevisses |
| `hakarl` | Requin du Groenland | eau | `djuphav` | Djuphav 0,015, Fjord 0,010 ; eau d'au moins 8 blocs | très lent (0,3 à 0,7), 30 PV |
| `val` | Baleine franche | eau | `val` | Djuphav 0,015, Västerhav 0,010 ; eau d'au moins 10 blocs | six blocs de long, paisible, migratrice (avril à juin, septembre-octobre) |

## Les 6 insectes (§3.5)

Des nuées sans dégâts (1 PV, `behaviorType` passif). Les valeurs sont pour 64 × 64 ; toutes les
courbes de saison s'annulent l'hiver.

| id | Insecte | Profil | Apparition | Particularités |
| --- | --- | --- | --- | --- |
| `mygg` | Moustique | `skymning` | Myr 0,80, Palsmyr 0,60, Urskog 0,50, Fjällbjörkskog 0,40 ; juin à août, crépuscule ; vent × 0,1, pluie × 0,4 | nuées de 6 à 14, **fuient la fumée** (peur 1, à 8 blocs) |
| `knott` | Simulie | `skymning` | Älv 0,80, Fjällbjörkskog, Bergslag 0,30 ; près de l'eau (6 blocs) ; vent × 0,2 | |
| `bi` | Abeille | `insekt` | Hagmark 0,60, Bokskog, Strandäng, Alvar 0,40 ; avril à septembre, beau temps | |
| `fjaril` | Papillon | `insekt` | Hagmark 0,50, Strandäng 0,40, Alvar, Bokskog 0,30 | |
| `trollslanda` | Libellule | `insekt` | Sjöland 0,40, Älv, Myr 0,30, Strandäng 0,20 ; juin à août | |
| `lysmask` | Ver luisant | `lysmask` | Hagmark 0,50, Bokskog 0,40, Urskog, Strandäng 0,30 ; **juin** (pic), nocturne | |

## Écarts au document

* **Saumon** : il apparaît dans l'Älv (rivières) et la mer selon la courbe de juin à septembre mais
  ne **suit pas le cours** des rivières (pas de graphe d'eau pour les poissons) ; pas de saut des
  chutes de 6 blocs, pas de frai ni de mort en amont.
* **Brochet** : pas de roselières comme habitat (il suit simplement les lacs) ; il ne s'attaque pas
  aux canetons ni aux lignes (pêche : 6.1 et suivantes). Il chasse perche, hareng et saumon.
* **Truite** derrière les rochers, **omble** sous la glace, **anguille** à la nasse, **écrevisse** à
  la nasse : mécaniques de pêche à venir (6.1, 6.3). Les espèces apparaissent, c'est tout.
* **Moule et huître** (`blamussla`) et **fourmilière** (`skogsmyra`) sont des blocs, pas des mobs : ils
  viennent avec les blocs de récolte (phase 6) et la nature interactive (5.8).
* **Hareng** : bancs de 6 à 14 qui fuient, pas de bancs « immenses » ; le goéland ne tournoie pas
  encore au-dessus (le profil `havsfagel` chasse `fisk` mais pas `stim`).
* **Phoque annelé** : pas de trous de respiration. **Morse** : ne renverse pas les barques (pas de
  bateaux avant 6.3). **Marsouin** : ne suit pas les bateaux et ne signale pas les bancs.
* **Hákarl** : ne mord pas les lignes ; la chair toxique et fermentée viendra avec le butin. **Baleine** :
  pas de souffle, pas d'échouage, le Naturfrid n'est pas lié à sa mort (le crochet `naturfrid` de la
  5.2 reste vide).
* **Loutre** : pas de glissades ni de poisson offert. **Méduse** : la brûlure est celle d'une
  attaque ordinaire.
* **Moustiques et simulies** : ils n'infligent ni démangeaison ni Mys bas ni gêne de visée (survie
  nordique) ; la moustiquaire et le goudron de bouleau n'existent pas. **Abeilles** : pas de ruches
  ni d'essaims dans les arbres creux. **Papillons, vers luisants, libellules** : pas d'indicateur de
  Mys ni de lueur émise.
* **Butin** : celui de vanilla (peau, graisse, ivoire, poisson : 5.8).
* Les modèles sont des boîtes colorées sur les formes de vanilla (poissons à nageoires, méduse,
  crabe pour l'écrevisse, quadrupèdes courts pour les phoques, bipède à ailes pour les insectes) ;
  les vrais modèles viendront de Gabriel. Aucun aperçu PNG (pas de Pillow dans Docker).
