# Glace jouable (session 6.1)

Plan Norrcraft, phase 6 « mécaniques nordiques » ; GDD doc 04 §4.1 et §4.2. Code :
`nb-ecs/src/ice.rs` (états, charge, rupture, remontée, forage, pêche, scie, glacière),
`nb-world/src/norrcraft/seasonal.rs` (`is_channel`, débâcle) et `nb-world/src/seasons.rs`
(`ice_status`). Objets : `assets/norrcraft/items.json`. Tests : `nb-ecs/tests/ice.rs`.

## États et charge

`IceKind::of` lit le bloc : `nyis` (neuve), `is` (portante), `tjockis` (épaisse), `havsis` (mer),
`blais` (banquise). `Load` : `Prone` (à plat ventre, soit **accroupi** tant que ramper n'existe
pas), `Walker` (joueur debout, renne), `Light` (petit animal), `Heavy` (cheval, élan),
`Sledge`, `LoadedSledge` (traîneaux : 6.2). Un animal est léger sous 0,35 de volume (largeur ×
largeur × hauteur), lourd à partir de 1,6 ; les mobs de banquise, d'air et d'eau ne pèsent pas.

| Glace | Porte |
| --- | --- |
| neuve | à plat ventre, animal léger |
| portante, de mer | tout sauf animal lourd et traîneau chargé |
| épaisse, banquise | tout |

**Fragilité** : toute glace dont l'eau libre est à moins de 2 cases (trou de pêche, chenal de
débâcle, embouchure, bord d'une rivière) perd un rang (`IceKind::weakened`) ; la banquise y
échappe. C'est l'approximation retenue de « plus fragile près des courants et roselières ».

## Chute

`IceSystem` (après l'écologie) regarde le bloc sous les pieds des entités posées. Si la glace ne
porte pas la charge : `EcsEvent::IceCrack` au premier tick (le craquement d'avertissement), une
[`Strain`] monte, et après 1,6 s (0,5 s sous un animal lourd) la glace de la croix 3×3 cède
(`collapse` : eau, `EcsEvent::IceBroke`) ; l'entité tombe. La tension retombe après 1 s hors
charge. La banquise ne cède jamais.

## Remontée

`climb_out` : bord de glace voisin (au niveau de la poitrine ou dessous) dont le dessus est
libre. Avec poinçons (`isdubbar` dans l'inventaire) on réussit toujours ; sans, **un tiers** des
essais et le bord casse sinon. Le joueur dans l'eau tient `Saut` (un essai par 0,8 s).

## Forage, pêche, scie

* `drill` (tarière `isborr`) : glace non banquise, eau dessous, air dessus → de l'eau (le trou
  regèle de lui-même au pas de saison : c'est de l'eau comme dans la génération).
* `is_hole` : eau à l'air libre entourée de glace sur 3 côtés.
* Lignes (`IceState` partagé, `WorldSession::ice()`) : `pimpel` (canne tenue, fenêtre de touche
  de 3 s, ferrer trop tôt effraie), `angeldon` (dormante, drapeau = `EcsEvent::FishBite`, la prise
  attend). `reel` rend la prise. Le trou regelé retire la ligne. Prises : abborre, gädda, sik,
  rödingue (novembre à avril), harr (septembre à mars).
* `seine` (`isnot`) : deux trous à 4–24 cases, deux équipiers (LAN), grosse prise.
* `saw` (`issag`) : glace portante/mer 1 `isblock`, épaisse 2, jamais la neuve ni la banquise.

## Glacière

`Icehouse` : fonte quotidienne `0,15 % + 0,35 % × chaleur` sous la sciure, `2 % + 5 % × chaleur`
sans. 64 blocs sciés en février, 120 jours à chaleur 0,6 : environ 40 restent sous la sciure,
aucun sans. Le cellier à glace (6.5) lira cette structure.

## Débâcle

`SeasonRules::ice_status` renvoie aussi un drapeau « la glace fond » (épaisseur en baisse au
centre du pas). La surcouche saisonnière laisse alors en eau libre les cellules de `is_channel`
(environ 30 % : plaques de 4×4 dont 12 % manquent, fissures le long des bords), le reste est de
la glace neuve. Le jeu fait le reste : ces plaques sont fragiles (eau libre voisine).

## Écarts et reste à faire

* **Aucun geste de jeu** : le survival de Norrcraft est vide (`SurvivalData::default`), donc le
  clic d'un `isborr` ou d'un `issag` n'est pas encore câblé ; `IceTool::from_item` et les
  fonctions pures attendent la table de survie de Norrcraft (6.7).
* Objets provisoires (textures de vanilla, ids 256 à 267), pas de recettes ni d'outils craftables.
* Plaques de débâcle **immobiles** (la dérive n'est pas simulée), pas de cabanes de pêche ni de
  pistes damées, pas de chasse au phoque aux trous de respiration.
* La tension ne tient compte que de la cellule sous le centre des pieds ; pas de dégâts de froid
  spécifiques à l'immersion (la 3.3 fait déjà le ressenti), pas de son de craquement.
* Le LAN ne réplique pas les lignes de pêche.
