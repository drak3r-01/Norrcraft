# Cultures, brûlis, fenaison, estive et kulning (session 6.4)

Plan Norrcraft, phase 6 « mécaniques nordiques » ; GDD doc 04 §6.1 (champs) et §6.2 (foin, bétail,
estive). Code : `nb-ecs/src/farming.rs` (champs, brûlis, clôtures) et `nb-ecs/src/hay.rs`
(fenaison, grange, kulning) ; estive dans `nb-ecs/src/husbandry.rs` ; tests :
`nb-ecs/tests/farming.rs`.

## Cultures

13 espèces (`farming::SPECS`). L'objet récolté sert aussi de semence.

| Culture (objet) | Semis | Jours à plein régime | Jour mini (h) | Gel | Latitude max |
| --- | --- | --- | --- | --- | --- |
| Seigle `rag` | août–sept. (d'hiver) | 30 | 5 | résiste | 0,80 |
| Orge `korn` | mai–juin | 18 | 8 | non | 1,00 |
| Avoine `havre` | mai–juin | 22 | 8 | non | 0,85 |
| Navet `rova` | mai–juillet | 16 | 8 | non | 0,90 |
| Chou `kal` | mai–juin | 26 | 9 | non | 0,75 |
| Pois `arter` | mai–juin | 18 | 9 | non | 0,80 |
| Lin `lin` | mai–juin | 24 | 10 | non | 0,75 |
| Chanvre `hampa` | mai–juin | 26 | 10 | non | 0,65 |
| Houblon `humle` | avril–mai | 34 | 10 | non | 0,60 |
| Oignon `lok` | avril–mai | 22 | 9 | non | 0,75 |
| Pomme de terre `potatis` | mai–juin | 24 | 9 | non | 0,80 |
| Pommier `appel` (vivace) | avril–mai | 60 | 10 | non | 0,55 |
| Cerisier `korsbar` (vivace) | avril–mai | 50 | 10 | non | 0,50 |

La latitude est 1 − T₀ du lieu, relevée au semis (`Plot::latitude`) : au-delà de la limite, le
semis est refusé (`SowError::TooNorth`). Les mois sont les mois réels du calendrier (0 = janvier).

## Croissance par la lumière

`growth_rate` (par jour de jeu, 1 = culture entière en `days` jours) =
`lumière × bonus × chaleur × sol / jours`.

* **Lumière** : lissage de `light` à `light + 4` heures de jour (`DayLight` de la 3.1, à la
  latitude de la plante). Bonus jusqu'à +40 % entre 16 et 24 h de jour : le soleil de minuit
  presse l'orge du Nord.
* **Chaleur** : nulle au point de congélation de l'air (`weather::freezing_point`), pleine à
  +0,14. Le seigle (rustique) garde 20 % de croissance un peu sous zéro.
* **Sol** : 0,5 + 0,5 × fertilité (0 à 1,2 ; champ neuf 0,6 ; fumier +0,4, goémon +0,3).
* **Gel** : une culture sensible dont l'air passe sous zéro **flétrit** (bloc `vissen`,
  `EcsEvent::CropWithered`).
* Les champs ne poussent que dans les chunks chargés, avec `Environment` (météo) présent.

## Champs et blocs

`till` laboure un sol `gras jord moran rodmull lovforna aska` à ciel ouvert : bloc `akerjord`
(160) et `Plot` dans `FieldState`. `sow` pose le stade 0 au-dessus. `harvest` rend
`(objet, quantité)` : tirage de la fiche × (0,5 + fertilité/2) × 3 sur cendre, perd 0,25 de
fertilité ; une vivace redevient à moitié mûre. Les blocs de plante sont partagés par **forme**
(`korn_0..2` céréales, `rot_0..2` racines, `blad_0..2` autres ; textures vanilla `crop_wheat`,
`crop_potato`, `crop_carrot`), l'espèce est dans le `Plot`. Le joueur qui casse le sol fait
disparaître le champ, qui casse la plante le vide.

## Brûlis (svedjebruk)

`fell` ajoute des troncs à un abattis (`SlashState`) ; il sèche par temps sec hors gel
(`SLASH_DRY_DAYS` = 6 jours de jeu, la pluie et la neige l'arrêtent, des troncs frais ramènent la
moyenne). `burn` refuse un abattis humide (`BurnError::NotDry`), puis couvre un disque de rayon
1 + troncs/6 (au plus 5) de **cendre** (`askaker`, 161) : le feu emporte ce qui poussait. Sur cendre
le rendement est triple pendant **deux ans** de jeu, puis le sol devient **épuisé** (`utmarkt`,
162, plus de semis) et, après un an de plus, **reboise** (retour à `aska`, la brûlis naturel).

## Clôtures

Bloc `gardsgard` (163). `enclosed` remplit à hauteur de la plante : le champ est clos si le
remplissage reste dans 6 cases. Les herbivores (profils `betare hjord husdjur smavilt`) qui passent
à moins d'une case d'une plante non close la broutent.

## Fenaison (slåtter)

* `mow` : l'herbe haute (`hogt_gras`) se coupe de juin à août ; juillet donne tout (1 à 2 brassées),
  juin et août 0,6 ; les autres mois, rien.
* **Hässja** (bloc `hassja`, 164, état `Hassja` dans `HayState`) : 8 brassées ; trois jours de
  temps sec (`Spell::Dry` : ni pluie > 0,02, ni brume dense, hors gel) les sèchent ; le couvert
  met en pause ; la pluie (> 0,15), la neige ou le gel font reculer d'un jour par jour de pluie.
  `EcsEvent::HayReady` quand le foin est prêt ; `take` rend une botte (`ho`) par brassée.
* **Grange** (bloc `hoskulle`, 165, état `Barn`) : en dehors de la saison de pâture de l'espèce,
  chaque bête (bétail et montures) à moins de 12 blocs mange **une botte par jour de jeu**
  (`husbandry::feed` : +0,5 de fourrage). Sans foin elle suit la règle de l'élevage 5.7
  (dépérit à sec). `herd_days(foin, bêtes)` dit combien de jours le foin tient.

## Estive (fäbod)

La laiterie des fäbod porte maintenant une baratte `karna` (166, `Mat::Churn`). Dans les mois du
bonus (mai à août dans les fiches), le lait de la vache et de la chèvre vaut **×2** (`summerBonus`
2,0) quand une baratte est à 16 blocs (`Domestic::at_fabod`, sondage toutes les 10 s). Dans un
registre sans `karna` (univers vanilla, tests anciens) le bonus reste inconditionnel.

## Kulning

`learn_kulning(joueur, fäbod_habitée)` pose `Kulner`. `call(appelant, Voice)` pose un `Emitter`
`Stimulus::Kulning` (96 blocs ; corne de vache 80, trompe d'écorce 72, sans apprentissage) et un
`Calling` qui s'éteint après `WARD_SECS` = 60 s (`HaySystem`). Les profils bétail l'avaient déjà
en attirance ; les profils `flockjagare` (loups) et `allatare` (ours, lynx) le **fuient** désormais
(`keepAway` 40 et 30). `EcsEvent::Kulning` annonce l'appel (aucun son pour l'instant).

## Objets et blocs ajoutés

Objets 292 à 314 : les 13 produits, `hacka` (houe), `lie` (faux), `rost` (voix), `kulhorn`,
`naverlur`, `godsel` (fumier), `tang` (goémon), `slattergras`, `ho` (foin), `brandaska`. Blocs 160 à
176. Aucun n'a de recette (6.7).

## Reste

* Aucun geste n'est câblé à la main du joueur (houe, semailles, faux, appel) : la survie
  Norrcraft est vide jusqu'à la 6.7 ; les fonctions pures attendent leurs recettes.
* Pas de renniculture (Sames, phase 7), pas d'irrigation ni de maladie, pas de vergers qui fleurissent.
* Les champs et la fenaison ne sont pas sauvegardés dans `world.json` ni envoyés aux invités LAN.
* `FarmSystem` fait pousser tous les champs des chunks chargés avec l'`Environment` du joueur
  local (même température partout).
* Pas de son ni de rendu des stades au-delà des textures vanilla réutilisées.
