# 06. Plan de développement

Plan d'exécution du GDD Norrcraft. **Rien n'est lancé** : chaque session ci-dessous attend
l'accord de Gabriel (voir §6).

## 1. Règles d'exécution

* **Modèle et effort** : chaque session d'exécution tourne en **Fable 5.2, effort low**, selon la
  consigne de Gabriel. Les documents de ce GDD ont été rédigés ainsi.
* **Une session = une mécanique = un worktree** dédié du dépôt `C:\Users\tigab\Documents\Nordcraft
  Rust`, sur sa propre branche. Une session de fusion intègre, lance les tests et construit l'exe.
* **Build** : uniquement par Docker (`scripts/build-windows.sh` via Git Bash), exe final dans le
  dossier principal. Jamais de poussée sur `main` sans accord.
* **Données d'abord** : toute règle réglable va en JSON sous `assets/norrcraft/`, produite par un
  script générateur quand la table est grosse (comme `gen_mobs.py`, `gen_survival.py`).
* **Textures et sons** : provisoires procéduraux, préfixe `norr_`, en attendant celles de Gabriel.
* **Vanilla intouchable** : après la phase 0, toute session vérifie les empreintes de chunks
  vanilla et l'ensemble des tests existants.
* **Chaque session livre** : code, tests, section de doc mise à jour, aperçu (PNG de carte ou
  capture scriptée), et un compte rendu de trois lignes.

## 2. Phases

Taille : S (une session courte), M (une session), L (deux à trois sessions).

### Phase 0. Socle de séparation (bloquante)

| Session | Contenu (référence GDD) | Taille | Dépend de |
| --- | --- | --- | --- |
| 0.1 | Empreintes de référence vanilla ; déplacement mécanique vers `nb-world/src/vanilla/` (doc 01 §9, étapes 1 et 2) | M | |
| 0.2 | `WorldKind`, `WorldProfile`, `TerrainGenerator`, `BiomeInfo` ; adaptateur vanilla ; retrait des `Biome::` hors vanilla (doc 01 §3) | M | 0.1 |
| 0.3 | Palette de blocs par monde, `world.json` v2, déplacement des assets, poignée de main LAN (doc 01 §5 et §6) | L | 0.2 |
| 0.4 | Sélecteur d'univers dans l'interface, générateur Norrcraft bouchon, test d'étanchéité (doc 01 §7 et §10) | M | 0.3 |

**Jalon A** : un monde Norrcraft vide se crée, se sauve et s'héberge ; vanilla est inchangé.

### Phase 1. Relief et climat

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 1.1 | Briques mathématiques et champs macroscopiques ; outil `worldmap` Norrcraft (doc 02 §3, §4, §12) | M | 0.4 |
| 1.2 | Hauteur : socle, collines, chaîne, plateaux, vallées en U et fjords (doc 02 §5.1 à §5.4) | L | 1.1 |
| 1.3 | Archipel, côtes, eskers, tourbières, glacier, champ de lave (doc 02 §5.5 à §5.8) | L | 1.2 |
| 1.4 | Hydrographie : niveau en escalier, rivières et rapides, lacs étanches, mers (doc 02 §6) | L | 1.2 |

### Phase 2. Biomes, sous-sol, flore

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 2.1 | Sélection des biomes, étagement, transitions tramées, teintes ; blocs de terrain (doc 02 §7 ; doc 04 §9) | L | 1.3, 1.4 |
| 2.2 | Strates, provinces, minerais, cavités (doc 02 §8) | M | 2.1 |
| 2.3 | Arbres : 12 essences et placement (doc 02 §9.1, §9.2) | L | 2.1 |
| 2.4 | Strate basse et décors de relief (doc 02 §9.3) | M | 2.1 |

**Jalon B** : monde Norrcraft explorable en créatif ; Norrcraft devient l'univers par défaut.

### Phase 3. Temps qui passe

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 3.1 | Calendrier, durée du jour par latitude, soleil de minuit et nuit polaire, Almanach (doc 04 §2.1, §2.2) | M | 2.1 |
| 3.2 | Surcouche saisonnière : neige en couches, glace, feuillages, flore saisonnière (doc 04 §2.3) | L | 3.1 |
| 3.3 | Météo, vent, aurore, température ressentie, vêtements (doc 04 §3) | L | 3.1 |

### Phase 4. Structures

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 4.1 | Trois grilles, prédicats de site, pistes et cairns (doc 02 §10.2, §10.3) | M | 2.1 |
| 4.2 | Habitat : torp, by, lada, fäbod, fiskeläge, bastu, fjällstuga, vindskydd, kåta | L | 4.1 |
| 4.3 | Patrimoine : runsten, gravhög, skeppssättning, fornborg, stavkyrka, väderkvarn, fyr, trojaborg, hällristning, långhus, épaves | L | 4.1 |
| 4.4 | Industrie et nature : gruva, hytta, kolmila, tjärdal, barrages, tanières, geysers, vårdkasar | L | 4.1 |

### Phase 5. Bestiaire

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 5.1 | Moteur d'IA : machine à états, besoins, perception et vent, horloges (doc 03 §2) | L | 3.3 |
| 5.2 | Milieux AIR, AMPHIBIE, GLACE ; apparition par densité cible, saison, cycle (doc 03 §1, §4.1 à §4.3) | L | 5.1 |
| 5.3 | Hardes persistantes et migrations (doc 03 §4.4) | M | 5.2 |
| 5.4 | Lot terrestre : herbivores et prédateurs (doc 03 §3.1, §3.2), géométries et animations | L | 5.2 |
| 5.5 | Lot oiseaux (doc 03 §3.3) | L | 5.2 |
| 5.6 | Lot aquatique, amphibie, insectes (doc 03 §3.4, §3.5) | L | 5.2 |
| 5.7 | Animaux domestiques, fourrage, chien (doc 03 §3.6) | M | 5.4 |
| 5.8 | Interactions avec le monde, pistes dans la neige, chasse et piégeage (doc 03 §4.5, §5) | M | 5.4, 3.2 |

**Jalon C** : monde vivant, saisons complètes, survie de base jouable.

### Phase 6. Mécaniques nordiques

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 6.1 | Glace jouable : états, chute, pêche sur glace, récolte, débâcle (doc 04 §4.1, §4.2) | L | 3.2 |
| 6.2 | Neige et glisse : skis, raquettes, patins, luges, avalanches (doc 04 §4.3, §4.4) | L | 3.2 |
| 6.3 | Bateaux, voile et vent, fjords, stockfisch (doc 04 §5.1, §5.2) | L | 3.3 |
| 6.4 | Cultures, croissance par la lumière, brûlis, fenaison, estive, kulning (doc 04 §6.1, §6.2) | L | 3.1, 5.7 |
| 6.5 | Fraîcheur, conservation, celliers, cuisine (doc 04 §6.3, §6.4) | L | 3.1 |
| 6.6 | Eau motrice, transmission, machines (doc 04 §6.5) | L | 1.4, 6E.3 |
| 6.7 | Fer : charbon, goudron, bas fourneau, abattage au feu, haut fourneau, acier, autres métaux, paliers d'outils (doc 04 §6.6) | L | 2.2, 6.6 |
| 6.8 | Artisanats et commerce (doc 04 §6.7, §5.3) | M | 6.5, 6.7 |
| 6.9 | **Mys, lagom, confort du foyer, fika** (doc 04 §8.1, §8.2) | M | 6.5 |
| 6.10 | **Sauna** (doc 04 §8.3) | S | 3.3, 6.9 |
| 6.11 | Musique et fêtes du calendrier (doc 04 §8.4, §8.5) | M | 3.1, 6.9 |

### Phase 6E. Eau vivante (étape ajoutée le 14 novembre 2026)

Eau dynamique : écoulement à niveaux qui conserve le volume, sources, blocs immergés et plantes
aquatiques, courants, cascades générées, rendu et son. Conception complète dans le
**doc 07** (`07-eau-vivante.md`), écrite pour être appliquée telle quelle : chaque session suit le
tableau de fichiers de sa section 16. Sessions en **Sonnet**, à placer avant la 6.5 (décision E8
du doc 07) ; la 6.6 en dépend.

| Session | Contenu (référence doc 07) | Taille | Dépend de |
| --- | --- | --- | --- |
| 6E.1 (**faite**, `docs/EAU_VIVANTE.md`) | Simulation et stockage : couche fluide des chunks, classes de blocs, table d'eau et courant des rivières, automate (chute, étalement, pression, films), sommeil et régime permanent, sources, pluie, gel, format de chunk v2, `/water` (§2 à §4, §11, §14) | L | 1.4, 3.2 |
| 6E.2 | Rendu, son et réseau : surface inclinée, écume et normales advectées, nappes qui tombent, brume, ambiances de chute et de ruisseau, protocole LAN 5 (§9, §10, §12) | L | 6E.1 |
| 6E.3 | Courants et vivant : joueur (gué, nage, rapides, contre-courant), mobs, bateaux, saumons qui remontent, glace fragile sur le courant, plantes aquatiques vivantes (§5.3, §6, §7.1) | L | 6E.1, 6.1, 6.3 |
| 6E.4 | Génération et saisons : chutes des marches de rivière, cascades de paroi, sources, résurgences, torrents glaciaires, nouvelles plantes, cascades gelées, mode `eau` de `worldmap`, exécutable (§5.2, §7.2, §8) | L | 6E.1, 6E.2 |

La 6E.2 et la 6E.3 peuvent tourner en parallèle après la 6E.1. Les décisions E1 à E8 du doc 07
(§18) sont à valider avant la 6E.1.

### Phase 7. Savoirs et folklore

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 7.1 | Naturfrid, orientation sans coordonnées, carte, remèdes (doc 04 §7.1, §7.3, §7.4) | M | 5.4 |
| 7.2 | Runes : apprentissage, gravure, 24 effets, recharge (doc 04 §7.2) | L | 4.3, 3.3 |
| 7.3 | Folklore du quotidien : tomte, troll, skogsrå, näcken, vittror, älvor, feux follets, mara, gast (doc 03 §3.7) | L | 5.1, 7.1 |
| 7.4 | Habitants : comptoirs, villageois, camps sames (dialogue simple, échanges) | L | 4.2, 6.8 |

### Phase 8. Saga

| Session | Contenu | Taille | Dépend de |
| --- | --- | --- | --- |
| 8.1 | Sagabok (onglets), bilan d'hiver, hauts faits (doc 05 §6, §7) | M | 3.1 |
| 8.2 | Neuf feux : placement par la graine, braise, pot à braise, conditions (doc 05 §4.2, §4.4) | L | 4.4, 6.7 |
| 8.3 | Épreuves : Roi de la montagne, Lindorm, Kraken, Näcken, Stállu, Jätte, avec leurs deux voies (doc 05 §4.4) | L | 7.3, 8.2 |
| 8.4 | Dénouement, réglages de pression, mode Fimbulvinter, LAN (doc 05 §4.5, §4.6, §8) | M | 8.3 |

**Jalon D** : partie gagnable de bout en bout.

### Phase 9. Finition

Équilibrage sur trois graines, performance (budgets des docs 02 §12 et 03 §6), audio nordique,
import des textures de Gabriel, essais LAN à deux machines, wiki.

## 3. Ce qui peut avancer en parallèle

```
Phase 0 ──► Phase 1 ──► Phase 2 ──┬──► Phase 3 ──┬──► Phase 5 ──┬──► Phase 7 ──► Phase 8 ──► Phase 9
                                  │              │              │
                                  ├──► Phase 4 ──┘              │
                                  │                             │
                                  └──► 6.6 (eau motrice)        │
                                       Phase 3 ──► 6.1 à 6.5, 6.9 à 6.11 ──► 6.7, 6.8 ──┘
```

* Phases 0, 1 et 2.1 : strictement en série (elles touchent le même cœur).
* À partir de 2.1 : jusqu'à trois worktrees en parallèle (par exemple 2.2 + 2.3 + 4.1, puis
  3.2 + 4.2 + 5.1).
* Les lots du bestiaire (5.4, 5.5, 5.6) et les sessions 6.1 à 6.5 sont indépendants entre eux.
* Phase 6E (eau vivante) : 6E.1 seule, puis 6E.2 et 6E.3 en parallèle, puis 6E.4. Elle touche le
  cœur du moteur (chunks, mailleur, physique) : ne pas la mener en même temps qu'une autre
  session qui modifie `nb-world/src/chunk.rs`, `manager.rs` ou `nb-mesh`.

## 4. Décisions à valider par Gabriel

| # | Question | Recommandation |
| --- | --- | --- |
| D1 | Noms d'univers `vanilla` et `norrcraft` ; bascule du défaut au jalon B seulement | oui |
| D2 | Identifiants de blocs : palette par monde en u8 (243 blocs Norrcraft sur 256, peu de marge) ou passage à u16 | palette par monde d'abord ; u16 si la marge manque |
| D3 | Hauteur du monde : rester à 128 avec la mer à 40, ou passer à 256 | 128 pour la première version |
| D4 | Longueur de l'année par défaut | 24 jours (8 heures de jeu) |
| D5 | Folklore activé par défaut ; réglage paisible disponible | oui |
| D6 | Saga sans échéance par défaut (Fimbulvinter narratif) | oui |
| D7 | Langue : identifiants en suédois sans accents ; noms affichés en anglais comme aujourd'hui, ou en français | à choisir |
| D8 | Pas de charbon minéral, ni diamant ni émeraude dans Norrcraft | oui |
| D9 | Présence des Sames : amicale, respectueuse, jamais en donjon | oui |
| D10 | Interprétation de la tâche 5 (boucles de jeu et saga des neuf feux) | à confirmer |
| D11 | Périmètre du premier lancement : phase 0 seule, ou phases 0 à 2 (jusqu'au jalon B) | phases 0 à 2 |

## 5. Risques

| Risque | Effet | Parade |
| --- | --- | --- |
| Régression de vanilla en phase 0 | perte de confiance | empreintes gelées avant tout déplacement |
| Eau statique : lacs ou rivières qui fuient | trous d'eau flottants | niveau en escalier, margelles, test d'étanchéité |
| Surcouche saisonnière coûteuse | saccades au changement de saison | 8 pas par an, étalés sur plusieurs ticks, fonction pure par chunk |
| Budget de blocs | blocage en phase 6 | décision D2, suivi dans le doc 04 §9 |
| IA à états plus chère que l'IA actuelle | chute d'images | 120 entités, mise à jour étalée, hardes abstraites hors champ |
| Ampleur (environ 55 sessions) | essoufflement | jalons A à D jouables chacun ; la saga vient en dernier |
| Systèmes neufs en LAN (calendrier, fraîcheur, saga) | désynchronisation | hôte seul maître de l'heure et de la saga ; protocole v3 par session concernée |
| Eau vivante trop coûteuse (phase 6E) | saccades près des rivières et des cascades | cellules éveillées seulement, budget par tick, régime permanent des cascades, rayon de simulation ; budgets du doc 07 §13 |
| Eau vivante qui inonde le monde généré | forêts noyées, lacs vidés | tout est généré à l'équilibre entre nappes infinies ; tracés purs qui échouent plutôt que de finir dans une cuvette (doc 07 §8) |

## 6. Accord demandé

Aucune session d'exécution ne sera lancée avant la réponse de Gabriel sur :

1. le plan global (phases et ordre) ;
2. les décisions D1 à D11 ;
3. le premier lot à lancer (recommandé : phase 0, sessions 0.1 à 0.4, en série).
