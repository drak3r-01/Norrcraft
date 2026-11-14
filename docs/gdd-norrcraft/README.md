# GDD Norrcraft

Document de game design et plan de développement du type de monde **norrcraft**, nouveau monde par
défaut de Norrblock, inspiré de la Suède et des pays nordiques. Le contenu actuel du jeu devient
le type de monde **vanilla**.

Phase de conception uniquement : aucun code final. Rédigé en Fable 5.2, effort low, le 10/11/2026,
sur la base du code de `main` (99bf3e9).

## Sommaire

| Document | Tâche | Contenu |
| --- | --- | --- |
| [01-architecture-mondes.md](01-architecture-mondes.md) | 1 | séparation vanilla / norrcraft : profil de monde, contrat du générateur, palette de blocs, sauvegardes, interface, ordre de migration |
| [02-worldgen-biomes-flore.md](02-worldgen-biomes-flore.md) | 2 | briques mathématiques, champs climatiques, fonction de hauteur (fjords, archipel, plateaux), hydrographie, 24 biomes de surface et les grandes rivières, transitions, sous-sol et minerais, 12 essences, flore, 35 structures |
| [03-bestiaire.md](03-bestiaire.md) | 3 | moteur de comportement, plus de 80 espèces et êtres du folklore, règles d'apparition par biome, saison et cycle |
| [04-mecaniques-blocs-items.md](04-mecaniques-blocs-items.md) | 4 | saisons et lumière, froid, glace et mers gelées, fjords et voile, subsistance, fer, runes, Mys, fika, sauna, fêtes ; 243 blocs, environ 260 objets |
| [05-objectif-de-jeu.md](05-objectif-de-jeu.md) | 5 | boucles de jeu, premier hiver, saga des neuf feux, hauts faits |
| [06-plan-de-developpement.md](06-plan-de-developpement.md) | plan | phases, sessions, dépendances, décisions à valider, risques |
| [07-eau-vivante.md](07-eau-vivante.md) | étape 6E | eau dynamique : simulation à niveaux, sources, blocs immergés, courants, cascades générées, rendu, son, LAN ; découpage en 4 sessions |

## En une page

* **Séparation** : un moteur commun et deux profils étanches. Le moteur ne nomme plus aucun biome,
  mob ou bloc d'univers. Vanilla garde ses identifiants et son comportement au bit près.
* **Monde** : un océan à l'ouest et ses fjords, une chaîne de vieux plateaux, la forêt boréale aux
  mille lacs et tourbières, un golfe qui gèle à l'est et son archipel de granit, un sud de hêtres
  et de calcaire, une île de lave au large. Le relief est continu et indépendant des biomes ;
  l'eau tient par un niveau en escalier qui crée rapides et cascades.
* **Vivant** : des animaux qui suivent les saisons (hibernation, migration, pelage, cycle des
  lemmings) et un folklore qui se gagne par la conduite plutôt que par la force.
* **Jouer** : l'hiver est l'adversaire, la prévoyance et le confort partagé sont les armes. Fika,
  sauna, glace qui porte ou qui cède, voile dans les fjords, fer tiré des marais puis de la roche
  par le feu.
* **But** : passer le premier hiver, puis rallumer les neuf feux d'alarme jusqu'au glacier.

## Avant toute exécution

Le plan (doc 06 §6) attend l'accord de Gabriel et ses réponses aux décisions D1 à D11.
