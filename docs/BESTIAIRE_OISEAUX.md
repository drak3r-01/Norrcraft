# Bestiaire des oiseaux de Norrcraft (session 5.5)

Référence : GDD doc 03 §3.3. Quinze fiches qui s'appuient sur le moteur d'IA (5.1), les milieux
et l'apparition par densité (5.2). Elles s'ajoutent aux 19 espèces terrestres de la 5.4
(`docs/BESTIAIRE_TERRESTRE.md`) : le bestiaire Norrcraft compte 34 espèces.

## Où est quoi

| Fichier | Rôle |
| --- | --- |
| `scripts/gen_norr_mobs.py` | Section « Oiseaux » : fiches, géométries (forme `biped` de vanilla, ailes battantes) et rigs |
| `assets/norrcraft/{mobs,mob_geometry,mob_rigs}.json` | Régénérés par le script (même commande que la 5.4) |
| `assets/norrcraft/behaviors.json` | 10 profils ajoutés : `tjaderfagel`, `orrfagel`, `snohona`, `flyttfagel`, `svan`, `ejderfagel`, `havsfagel`, `korp`, `uggla`, `skogsfagel` |
| `crates/nb-data/tests/norr_birds.rs` | Cohérence des fiches (PV, milieu, profils volants, migrateurs, agressifs) |
| `crates/nb-ecs/tests/bestiary_birds.rs` | Scénarios : vol, tétras en rut qui frappe, lagopède qui fuit |

Le pygargue (`havsorn`) réutilise `rovfagel` et le harfang (`fjalluggla`) `nattrovfagel` de la 5.1.

## Les 15 espèces

| id | Oiseau | Milieu | Profil | Apparition (densités pour 64 × 64) | Particularités |
| --- | --- | --- | --- | --- | --- |
| `tjader` | Grand tétras | terre | `tjaderfagel` | Urskog, Tallhed 0,20 | parade en avril-mai, **charge tout ce qui approche** (joueur compris), 1 dégât toutes les 2 s |
| `orre` | Tétras lyre | terre | `orrfagel` | Myr 0,20, Brännfält 0,15, groupes de 3 à 7, crépusculaire | parade collective au printemps |
| `ripa` | Lagopède | terre | `snohona` | Fjällhed 0,30, Fjällbjörkskog 0,25 | plumage d'hiver (état `winter_coat`), s'enfouit (`burrow`) et jaillit à l'approche |
| `trana` | Grue | air | `flyttfagel` | Myr, Strandäng 0,08, Hagmark 0,05 | absente de novembre à février, **revient en mars, repart en septembre**, danse (`display`), pose sur une patte au repos |
| `sangsvan` | Cygne chanteur | amphibie | `svan` | Sjöland 0,08, Bottenhav 0,05 | couple, garde son nid, **charge** (2,5 dégâts) au printemps |
| `grasgas` | Oie cendrée | air | `flyttfagel` | Strandäng 0,15, Skärgård 0,12, Hagmark 0,05, bandes de 4 à 9 | migratrice comme la grue |
| `ejder` | Eider | amphibie | `ejderfagel` | Skärgård 0,30, Fjord 0,15, groupes de 3 à 8 | niche à terre (`guard` en avril-juin) |
| `lunne` | Macareux | air | `havsfagel` | Klintkust 0,30, Fjord 0,20, Lavafält 0,05 ; avril à septembre | vol rasant (altitude 3), plonge sur les poissons |
| `havsorn` | Pygargue | air | `rovfagel` | Skärgård 0,05, Fjord 0,04, Älv 0,03 | plane à 20 blocs, vole la nourriture, pêche |
| `fjalluggla` | Harfang des neiges | air | `nattrovfagel` | Fjällhed 0,04 | cycle du lemming avec un an de retard |
| `kattuggla` | Chouette hulotte | air | `uggla` | Bokskog 0,08, Urskog 0,06, nocturne | chasse rongeurs et lièvres |
| `korp` | Grand corbeau | air | `korp` | dix biomes terrestres, 0,02 à 0,05 | attiré par charognes et nourriture, vole la nourriture |
| `hackspett` | Pic noir | air | `skogsfagel` | Urskog, Brännfält 0,15 | perche presque toujours |
| `mas` | Goéland | air | `havsfagel` | toutes les côtes (Skärgård 0,40 en tête), bandes de 2 à 6 | plonge sur les poissons, charogne |
| `goken` | Coucou | air | `skogsfagel` | Hagmark, Fjällbjörkskog 0,08 ; absent avant avril et après août | |

Points de vie : tétras 4, tétras lyre 3, lagopède 2, grue 6, cygne 10, oie 5, eider 4, macareux 2,
pygargue 10, harfang 6, chouette 3, corbeau 4, pic 2, goéland 3, coucou 2.

Les trois galliformes sont des marcheurs (milieu `LAND`) : ils ne volent pas. Les oiseaux de milieu
`AIR` volent à la croisière de leur profil (ou à `spawn.altitude`) et se posent pour manger ou dormir.

## Écarts au document

* **Passages de grues et d'oies** : elles apparaissent par densité avec une courbe de saison
  (`SUMMER`), pas en hardes de passage (`HerdMotion::Pass`, 5.3) : le système de hardes ne matérialise
  pas encore les volatiles. Pas de V sonore.
* **Place de chant, aube et son roucoulé du tétras lyre, hululement, coucou et présage, imitations
  du corbeau, carcasses guidées, tambourinage du pic, loges, abris à eider (duvet), nids du
  macareux, sterne** : non simulés (sessions 5.8 et suivantes).
* **Le lagopède dort dans un trou de neige** : il s'enfouit (`burrow`) quand il a peur, le trou de
  neige n'est pas posé ; collet : 5.8.
* **Oie et grue sentinelles, cygne qui charge les barques** : le cygne charge le joueur, pas les barques.
* **Plumage de saison** : une seule teinte par modèle (voir la 5.4).
* Butin : celui de vanilla (plumes, duvet, œufs : 5.8).
* Les modèles sont des boîtes colorées sur la forme `biped` de vanilla (ailes qui battent en vol) ;
  les vrais modèles viendront de Gabriel. Aucun aperçu PNG (pas de Pillow dans Docker).
