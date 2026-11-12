# Moteur d'IA du bestiaire Norrcraft (session 5.1)

Référence : GDD doc 03 §2 (machine à états, besoins, perception et vent, horloges). Les espèces
arrivent avec les sessions 5.4 à 5.7 ; cette session livre le moteur et dix profils de
comportement qui servent de modèles.

## Où est quoi

| Fichier | Rôle |
| --- | --- |
| `assets/norrcraft/behaviors.json` | Profils de comportement et cycles pluriannuels (données réglables) |
| `crates/nb-data/src/behavior.rs` | Lecture et validation : `BehaviorBook`, `Profile`, `StateKind` (24 états), `Need`, `Stimulus` |
| `crates/nb-data/src/mobs.rs` | Champ `behavior` d'un `MobConfig` : nom du profil qui mène le mob |
| `crates/nb-ecs/src/brain/mod.rs` | Composants `Brain`, `Emitter`, `Carrying`, `Crouching` ; `BrainSystem` |
| `crates/nb-ecs/src/brain/senses.rs` | Vue, ouïe, odorat sous le vent, ligne de vue |
| `crates/nb-ecs/src/brain/clock.rs` | Horloges journalière, saisonnière, pluriannuelle |
| `crates/nb-ecs/tests/brain.rs` | Scénarios et budget |

Un mob dont le `MobConfig` porte `"behavior": "<profil>"` reçoit un `Brain` à l'apparition, au
lieu des IA passive et agressive d'origine. Celles-ci ne voient pas ces animaux et restent
identiques pour vanilla (l'ordonnancement `Schedule::java_default` n'a pas changé ; le jeu ajoute
`BrainSystem` juste après `AggressiveAI`).

## Un tick

1. **Besoins** (0 à 1) : faim, soif, fatigue montent selon `needs` du profil (la fatigue double
   à la course, retombe en dormant, perché ou en hibernation ; l'hibernation divise faim et soif
   par 20). Le froid suit la température ressentie (`air − 0,1 · vent · refroidissement`, plus le
   pelage d'hiver) sous `coldBelow`. La peur prend le maximum de la menace perçue et s'oublie avec
   une constante de 6 s.
2. **Perception**, tous les 6 ticks (10 Hz), étalée par identifiant :
   * vue : cône `fov`, portée × lumière (`nightVision` en pleine nuit), bornée par le brouillard
     (`ln 5 / (0,002 + brume)`), ligne de vue bloc par bloc : bloc plein 0, feuillage 0,5,
     plante 0,85, eau 0,9 ;
   * ouïe : portée × bruit de la source (joueur accroupi ÷ 3, qui court × 2), × (1 − 0,4 ·
     max(pluie, vent)) ;
   * odorat : portée × `2^(cos α · min(1, vent / 0,2))`, soit × 2 quand la source est au vent de
     l'animal, ÷ 2 sous le vent, 1 par calme ;
   * facteurs (`Emitter` posé ou porté par le joueur) : feu, fumée, surströmming, cloche, kulning,
     charogne, nourriture, avec la réaction du profil (`fear`, `keepAway`, `attract`) ;
   * congénères connus sans les sens à 32 blocs : taille et rang de la meute, chef de harde
     (le congénère d'identifiant plus petit) ;
   * eau repérée par huit sondes en anneau quand la soif dépasse 0,3.
3. **Choix** : pour chaque état offert, `score = poids · besoin · opportunité`, le plus haut gagne.
   Hystérésis : on ne quitte un état qu'après 2 s, sauf s'il n'a plus d'opportunité ou qu'un autre
   le dépasse de 0,35.
4. **Action** : consignes de vitesse et de cap (la physique d'origine les intègre), morsures,
   vols, événements `EcsEvent::AnimalAct` (`Graze`, `Build`, `Steal`, `Kill`) pour la session 5.8.

## États et opportunités

| État | Besoin par défaut | Opportunité |
| --- | --- | --- |
| errer | constant | 1 (laisse revenir vers le gîte au-delà de 48 blocs) |
| brouter | faim | au sol, hors de l'eau, × (1 − peur) |
| boire | soif | eau voisine 1, eau repérée 0,7, × (1 − peur) |
| dormir | repos = max(fatigue, 1 − activité) | 1 − peur |
| fuir | peur | menace : 1 − courage ; feu trop près : 1 |
| menacer | peur | menace entre 4 et 14 blocs : courage |
| charger | peur | menace à moins de 4 blocs ou colère : courage |
| traquer | faim | proie à plus de 10 blocs |
| poursuivre | faim | proie à 10 blocs ou moins (0,5 au-delà) |
| attaquer | faim | proie à portée de morsure ; une meute attend la fin de l'encerclement |
| encercler | faim | meute d'au moins 2, proie perçue, pendant 6 s |
| garder | constant | saison de mise bas (rayon 6 autour du gîte) |
| suivre | constant | kulning 1 ; chef de harde à plus de 5 blocs |
| migrer | constant | fenêtre de migration, × activité, sans menace |
| hiberner | constant | fenêtre d'hibernation (retour à la tanière, caché) |
| parader | constant | rut, × activité |
| voler, se percher, plonger | constant, fatigue, faim | profils volants |
| nager | constant | dans l'eau, ou mob aquatique |
| fouir | peur | menace à plus de 8 blocs (0,2 plus près) ; caché |
| charogner | faim | charogne ou nourriture perçue |
| voler de la nourriture | faim | un autre animal porte de la nourriture (`Carrying`) |
| construire | constant | fenêtre de construction, faim < 0,6, × activité |

Le courage du profil monte au rut (`rutCourage`), de 0,2 à la mise bas et de 0,5 pendant les 6 s
de colère qui suivent un coup. Un même état peut figurer deux fois avec des besoins différents
(le lièvre dort de fatigue et de froid).

## Horloges

* Journalière : diurne, nocturne, crépusculaire (fond de 0,3 la nuit), toujours. La hauteur du
  soleil vient de la météo (`Environment::sun`) ou, à défaut, du calendrier à la latitude 0,5.
* Saisonnière : fenêtres de mois réels (0 = januari, passage du nouvel an permis) pour le pelage
  blanc, l'hibernation, le rut, la mise bas, les migrations (la deuxième fenêtre inverse le cap)
  et la construction.
* Pluriannuelle : `cycles.lemming` = période 4, facteurs 0,2 / 1 / 4 / 0,5 ; un profil y renvoie
  avec un décalage (`lag` 1 pour les prédateurs). `clock::cycle_factor` sert aux apparitions (5.2).

## Profils livrés

`betare` (élan), `hjord` (renne, harde), `smavilt` (lièvre, mue blanche), `gnagare` (lemming,
cycle), `flockjagare` (loup, meute), `allatare` (ours, hibernation, charogne), `rovfagel`
(pygargue, vol de nourriture), `nattrovfagel` (harfang, cycle décalé), `fisk`, `byggare` (castor).

## Écarts au document

* Un cinquième besoin, la **soif**, nourrit l'état « boire ».
* Marge de 0,35 qui court-circuite l'hystérésis (sans elle un animal qui broute mettrait 2 s à
  fuir un loup surgi).
* Migrer, parader et construire sont pondérés par l'activité de l'heure.
* Les milieux AIR, AMPHIBIE et GLACE arrivent en 5.2 : un profil volant donne déjà des consignes
  en 3D, mais la physique d'un mob `LAND` les ramène au sol.
* Le joueur ne peut pas encore s'accroupir ni porter du surströmming : le moteur lit les
  composants `Crouching` et `Emitter`, que le jeu posera avec les sessions concernées (feux,
  cloches, kulning en phase 6).

## Budget

120 animaux mêlés, un joueur : 0,34 ms par tick en profil `dev` (test
`hundred_twenty_animals_fit_the_budget`, plafond 1 ms en release, 3 ms en debug).
