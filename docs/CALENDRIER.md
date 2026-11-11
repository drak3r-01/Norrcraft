# Calendrier, saisons et lumière (session 3.1)

Plan Norrcraft, phase 3 « temps » ; GDD doc 04 §2.1 et §2.2. Code : `nb-world/src/calendar.rs`
(pur, testé), `nb-gfx/src/sky.rs` (ciel), `nb-game/src/game/calendar.rs` (boucle de jeu),
`WorldSession::calendar_command` (commandes). Seuls les univers dont le profil a
`Systems::seasons` (Norrcraft) ont un calendrier ; vanilla garde son ciel d'origine, bit à bit.

## Année

* 12 mois de `d` jours de jeu (`WorldConfig::days_per_month`, défaut 2 : année de 24 jours,
  8 heures de jeu ; 4 ou 8 pour des années longues). Un jour dure `day_length_seconds` (1200 s).
* Le compteur de jours (`SkyClock::day`, sauvegardé dans `world.json` sous `calendarDay`)
  s'incrémente à chaque tour de l'horloge du ciel (coucher du soleil, `timeOfDay = 0`).
* La fraction d'année `a` vaut 0 au solstice d'hiver et 0,5 au solstice d'été. Un nouveau monde
  démarre à `a = 0,40` (mi-mai, printemps).
* Dates affichées : calendrier réel (365 jours, mois suédois `januari` à `december`). Un jour de
  jeu porte la date de son premier jour réel (avec `d = 2`, un jour de jeu = 15 jours réels).
  Saisons : Vår = mars à maj, Sommar = juni à augusti, Höst = september à november,
  Vinter = december à februari.
* Fêtes (doc 04 §8.5) : 12 dates fixes (les fêtes mobiles du doc ont une date fixe au milieu de
  leur plage). Une fête tombe sur le jour de jeu qui contient sa date réelle. Les rituels et les
  effets viendront avec la phase 6.

## Durée du jour

λ = 1 − T₀ au pied du joueur (`Climate::latitude`, relevée deux fois par seconde, lissée avec
une constante de temps de 4 s : aucun saut quand on change de région).

```
A(λ)   = 0,16 + 0,44 λ
raw    = 0,5 − A(λ) cos(2π a)          fraction éclairée, non bornée
```

* `0 ≤ raw ≤ 1` : jour normal. Au sud (λ = 0,1) de 30 % à 70 % de jour.
* `raw > 1` (λ > 0,77 près du solstice d'été) : **soleil de minuit**, le soleil descend très bas
  mais ne touche pas l'horizon.
* `raw < 0` : **nuit polaire**, le soleil plafonne sous l'horizon : un crépuscule bleu à midi.
* Le cercle polaire est à λ = 0,77 (A = 0,5), comme dans le doc. Sur le terrain, λ va de
  0,10 (sud) à 0,92 (Norrland, Skärgård) ou 1,00 (Högfjäll).

### Mise en œuvre dans le ciel

L'horloge du ciel (`time_of_day`, 0..1200, midi = 900) ne change pas. Le soleil a une hauteur
normalisée `(c − sin θ) / (1 + |c|)` avec θ l'angle de l'horloge et `c = SkyClock::solar_bias`
(`calendar::solar_bias(raw)`) :

* `c = −cos(π · raw)` pour `0 ≤ raw ≤ 1` : le soleil est levé pendant la fraction `raw` du jour ;
  `c = 0` redonne le ciel d'origine (jour = nuit, vanilla).
* hors de [0, 1], `c = 1 + 4 (raw − 1)` ou `−1 + 4 raw` : le soleil reste au-dessus (ou au-dessous)
  de l'horizon de plus en plus loin ; continu en 0 et en 1.
* en hiver le soleil est aussi plus bas à midi (`c < 0`), en été plus haut qu'à l'équinoxe.

`sky_params_biased(t, c)` produit les mêmes paramètres que `sky_params(t)` quand `c = 0`
(testé sur 48 instants). Le rendu appelle `SkyClock::params()`. La lumière du jour de l'audio
(`audio::daylight`) lit `SkyClock::sun_height()`.

## Almanach

`/almanach` (permis aux invités LAN) imprime date, année, saison, durée du jour (ou soleil de
minuit / nuit polaire), fête du jour ou prochaine fête, phase de la lune et signe du temps du mois.
Le même `Almanac::lines()` servira l'onglet « Almanach » de la Sagabok (session 8.1). Le chat
accepte désormais une réponse sur plusieurs lignes.

`/date` affiche la date ; `/date set <n>` (jour de l'année 1..12d, 1 = solstice d'hiver) et
`/date add <jours>` changent le compteur et demandent les triches, comme `/time`.

La lune : phase réelle (lunaison de 29,53 jours réels sur le calendrier réel, donc environ
12,4 lunaisons par année de jeu), huit noms. Le disque de lune du ciel n'affiche pas encore la
phase.

## Réseau

Protocole 4 : `Welcome` et `Time` portent `day`. L'hôte fait foi pour le compteur ; chaque
joueur calcule le décalage solaire pour sa propre latitude. `days_per_month` voyage dans
`config_json`.

## Écarts et reste à faire

* Le solstice d'été tombe le 21 juin réel, au milieu de `juni` ; le doc place les mois sur des
  fractions d'année égales.
* La surcouche saisonnière des chunks (neige, glace, feuillages, flore), les fêtes actives et la
  croissance des plantes par la lumière (doc 04 §2.3 et §2.4) viennent avec les sessions 3.2 et
  6.4. `survival.rs` (température de l'air) lit encore `config.time_of_day`, sans décalage solaire.
* Pas de réglage de création pour `days_per_month` (le champ de `world.json` le permet).
* La phase de la lune n'est pas encore dessinée.
