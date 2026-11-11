# Météo, vent, aurore et température ressentie (Norrcraft, session 3.3)

Doc 04 §3 du GDD. Code : `crates/nb-world/src/weather.rs` (modèle pur), `crates/nb-game/src/game/weather.rs`
(branchement), `crates/nb-ecs/src/survival.rs` (ressenti), `crates/nb-gfx/src/sky.rs` (ciel).
Seuls les mondes à saisons (Norrcraft, `Systems.seasons`) ont une météo ; vanilla garde son ciel, sa
survie et sa commande `/weather clear|rain|snow` d'origine.

## Principe : une fonction, rien à sauvegarder

La météo en un lieu et à un instant est une fonction pure de `(graine, horloge, Locale)`. Aucun état
n'est écrit dans la sauvegarde ni envoyé par le LAN : toute machine qui connaît la graine et l'horloge
(l'hôte fait foi pour l'horloge, protocole 4) retrouve la même météo. Conséquence : `/date set` ou
`/time set` changent la météo (on saute dans le temps).

## Chaîne de Markov synoptique

Un créneau dure 300 ticks (un quart de jour). Trois régimes : **beau**, **variable**, **dépression**.
Probabilités (avant la saison) :

| de \ vers | beau | variable | dépression |
| --- | --- | --- | --- |
| beau | 0,62 | 0,30 | 0,08 |
| variable | 0,30 | 0,40 | 0,30 |
| dépression | 0,15 | 0,45 | 0,40 |

La colonne « dépression » est multipliée par `1 + 0,5 cos(2π (a − 0,80))` (a = fraction d'année, 0 =
solstice d'hiver) : plus fréquente à l'automne, puis les lignes sont renormalisées. La chaîne est
évaluée sans état en la faisant tourner depuis 14 créneaux plus tôt (`BURN_IN`).

## Résolution locale en dix états

Le régime d'un créneau devient un état de `Weather` d'après la température de l'air du lieu
(`Locale::air_temperature`, doc 04 §3.3), l'océanité O, la continentalité C, l'humidité et l'altitude.
Gel = 0,34 (`seasons.json`, `snow.threshold`) ; « grand froid » = gel − 0,10.

| Régime | Au-dessus du gel | Sous le gel |
| --- | --- | --- |
| Beau | clair ; brume (O et côte) ; voilé si un front suit ou en altitude | grand froid clair (sous gel − 0,10), sinon clair |
| Variable | voilé, bruine, pluie (plus de pluie si humide ou côte atlantique) | neige ou voilé |
| Dépression | tempête de mer (O > 0,55 sur la côte), orage (T > 0,55), sinon pluie | blizzard (très froid ou haut), sinon neige |

Les paramètres de chaque état (pluie, neige, couverture, brouillard, orage, rafale, refroidissement,
houle) sont dans `Weather::params`. L'atmosphère est **fondue** vers l'état du créneau suivant sur le
dernier quart du créneau : aucun saut de pluie, de ciel ou de brouillard (test de continuité).

Blizzard : brouillard 0,12 (visibilité d'environ 6 à 15 blocs, voile blanc), neige 1, vent + 0,5,
refroidissement par le vent × 3.

## Signes avant-coureurs

`Omens {halo, swallows_low, red_sky, birds_quiet}` montent dans la moitié du créneau qui précède un
temps plus mauvais (écart de gravité avec l'état suivant). Utilisés : **ciel rouge** (horizon et
zénith rougis quand le soleil est bas) et **silence des oiseaux** (l'ambiance sonore des oiseaux
baisse). Non dessinés : le halo et les hirondelles basses (valeurs calculées, sans rendu).

## Vent

Vecteur global lentement variable (bruit de valeur sur 450 ticks, vents d'ouest dominants, module de
base 0,14 à 0,36) + régime (0 / 0,10 / 0,28) + rafales de l'état, puis :

* **brise** : le long du gradient de continentalité (direction « de la mer vers l'intérieur »,
  mesurée par deux relevés à 48 blocs) : `+0,12 · hauteur du soleil` le jour (mer → terre),
  `−0,07 · |hauteur|` la nuit ; seulement près de la côte ;
* **canalisation** : `× (1 + 0,5 · relief_accidenté · cos²(angle avec l'axe des fjords θ))`, soit
  jusqu'à × 1,5 ;
* **abri** : relevé côté survie (voir ci-dessous), pas dans le vent global.

Le vent incline la pluie (≈ 25° à pleine force), couche la neige presque à l'horizontale et
alimente l'ambiance sonore du vent.

## Aurore boréale

`WeatherModel::aurora(jour, a, λ, couverture)` : probabilité par nuit
`(0,1 + 0,8 · lisse(λ, 0,25, 0,8)) · lisse(froid de saison)`, intensité 0,45 à 1 · (1 − couverture)².
Multiplie l'aurore du ciel (déjà dessinée par `pp.frag`) : plus d'aurore sous un ciel couvert, en été
ni au sud. Pas encore d'effet de jeu (runes et Mys : phases 7 et suivantes).

## Température ressentie (survie)

`Environment {air, vent, refroidissement, pluie, neige}` est réglé chaque tick par le jeu sur
`WorldSession::environment` et lu par `VitalsSystem` (`TickCtx::environment`). Sans lui, l'ancienne
formule par biome et heure s'applique (vanilla, tests de conformité).

```
air      = T_saison(a, lieu, altitude) − 0,12 (C − 0,5) cos(2π (a − le plus froid)) + 0,04 · jour/nuit
ressenti = (air − 0,10 · vent · (1 − abri) · refroidissement · (1 − coupe-vent) − 0,15 · mouillé − 0,50) × 1,6
felt     = ressenti + chaleur des sources × 0,95 + isolation          (inchangé)
```

* **abri** : 1 sous un toit ; sinon relevé toutes les 0,4 s des blocs pleins à 1, 2, 3, 5 et 8 blocs
  *au vent* du joueur (2 hauteurs) : arbres, murs, reliefs (`windbreak`).
* **mouillé** (`Vitals::wetness`, 0 à 1) : pluie (0,040 /s) et neige (0,012 /s) sans toit, × (1 −
  imperméabilité) ; sueur en courant avec plus de 0,4 d'isolation ; 1 d'un coup dans l'eau. Séchage :
  1/60 s près d'un feu (chaleur > 0,25), 1/200 s à couvert, 1/300 s dehors, 1/600 s sous le gel.
* **eau glacée** : entrer dans l'eau quand l'air est sous gel + 0,12 → endurance à zéro, − 12 de
  chaleur, puis ressenti − 0,45 tant qu'on y reste (− 0,15 sinon).
* sous terre (couvert et y < 28) : ressenti plancher − 0,12 comme avant.

## Vêtements

`ArmorInfo` gagne `windproof` et `waterproof` (0 à 1, défaut 0, dans `survival.json`). La tenue
combine les pièces par couverture : torse 0,4, jambes 0,3, tête 0,15, pieds 0,15. Fourrure vanilla :
coupe-vent 0,7, imperméabilité 0,3 par pièce. Les matières de doc 04 §3.3 (écorce de bouleau, vadmal,
fourrure de glouton, duvet d'eider, toile goudronnée, cuir de phoque) viendront avec les objets
Norrcraft (`assets/norrcraft/items.json` est encore vide) : valeurs prévues dans le tableau ci-dessous.

| Matière | Isolation | Coupe-vent | Imperméabilité |
| --- | --- | --- | --- |
| Écorce de bouleau, herbe | faible | 0,2 | 0,3 |
| Peau | moyenne | 0,5 | 0,2 |
| Vadmal (drap de laine) | bonne | 0,3 | 0,1 |
| Fourrure d'hiver | forte | 0,7 | 0,3 |
| Fourrure de glouton | forte, ne givre pas | 0,8 | 0,3 |
| Duvet d'eider | la meilleure | 0,5 | 0,2 |
| Toile goudronnée | nulle | 0,9 | 1,0 |
| Cuir de phoque | moyenne | 0,8 | 0,9 |

## Rendu

`SkyClock::weather: SkyWeather {overcast, fog, aurora_scale, flash, red_sky, wind, birds_quiet}`
(neutre par défaut : ciel d'origine bit à bit). `apply_weather` (dans `SkyClock::params`) grise et
assombrit le zénith, l'horizon et le brouillard, éteint la lumière directe, masque étoiles et aurore,
ajoute éclair et ciel rouge, et passe `fog_density` au shader : `FrameUniforms::fog_color.w`, ajouté à
la densité de brouillard de l'air dans `pp.frag` (SPIR-V recompilé). Les éclairs (orage) sont tirés
côté jeu (`WeatherState`, purement visuels, propres à chaque machine). Pluie et neige : les émetteurs
d'ambiance existants, pilotés par `Fx::set_weather` / `set_wind` à partir de l'atmosphère.

## Commandes

`/weather` : état courant (français, suédois, vent, air). `/weather <clear|veiled|drizzle|rain|mist|
storm|snow|blizzard|gale|frost>` : impose un état ; `/weather auto` : retour au modèle. Hôte et jeu
solo seulement (comme `/date`).

## Aperçus

`NB_NORR_WEATHER=<état> NB_NORR_SEASON=0..7 NB_NORR_TIME=0..1200` avec
`preview_norrcraft` (voir l'en-tête du fichier).

## Écarts et reste à faire

* La continentalité modifie la température de l'air de la météo (± 0,06) mais pas la surcouche
  saisonnière de la 3.2 (neige et glace des chunks) : léger désaccord possible en bordure de gel.
* La neige des chunks suit toujours les saisons, pas les chutes de neige de la météo (pas de manteau
  qui se forme sous le blizzard) ; vent d'ouest des congères toujours provisoire.
* Pas de son de pluie ni de tonnerre (pas de synthèse existante), pas de houle visible ni de
  naufrage (3.3 donne seulement `swell` ; bateaux en 6.3), pas d'ambre rejeté par la tempête.
* Halo et hirondelles : signes calculés, non dessinés.
* L'abri de neige, l'avalanche et le chasse-neige restent à faire (4.3 du doc 04, phase 6).
* Mobs et LAN : la survie de l'invité n'a pas d'`Environment` (seul le joueur local est suivi).
