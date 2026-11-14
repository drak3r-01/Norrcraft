# 05. Objectif de jeu et boucle de progression

Tâche 5 du GDD. L'intitulé reçu était tronqué (« Objectif de jeu ») ; il est interprété ici comme :
**à quoi joue-t-on dans un monde Norrcraft, minute par minute, année après année, et quel est le
but final**. Si l'intention était autre, ce document est celui à corriger en premier.

## 1. Promesse

> Tu débarques au printemps sur une côte du Nord. L'été est court, l'hiver décide de tout. Apprends
> le pays, ses bêtes et ses esprits ; bâtis un foyer où il fait bon ; puis porte le feu de colline
> en colline jusqu'au toit du monde, avant que ne vienne l'hiver qui ne finit pas.

Trois piliers : **prévoir** (les saisons), **habiter** (le mys), **cheminer** (la saga).

## 2. Les trois boucles

### 2.1 Boucle courte : la journée (20 minutes)

```
lumière du jour → sortir (cueillir, chasser, pêcher, couper, miner)
        ↑                                   ↓
   dormir au chaud  ←  rentrer avant la nuit  ←  transformer (cuire, sécher, forger)
        ↑                                   ↓
      mys (fika, sauna, veillée, musique)  ←  manger varié et chaud
```

Les quatre jauges (faim, chaleur, endurance, Mys) et le bonus **lagom** récompensent la journée
équilibrée plutôt que l'exploit.

### 2.2 Boucle moyenne : l'année (8 heures par défaut)

| Saison | Question posée au joueur | Réponses |
| --- | --- | --- |
| Printemps | « Que vais-je semer, où vais-je m'installer ? » | semailles, sève, réparation, départ en estive, débâcle à surveiller |
| Été | « Aurai-je assez pour l'hiver ? » | fenaison, baies, pêche, construction, voyages lointains par mer, charbon |
| Automne | « Ai-je tout rentré ? » | moissons, chasse, champignons, abattage, conserves, bois de chauffe |
| Hiver | « Que me permet la glace ? » | routes de glace, pêche et minerai sous la glace, bûcheronnage, forge, veillées, expéditions à skis |

Le **premier hiver** est le premier vrai objectif du jeu, et il se suffit à lui-même : un joueur
qui ne suit jamais la saga a quand même un jeu complet, celui du calendrier.

Bilan d'hiver (affiché au solstice dans la Sagabok) : jours de vivres, cordes de bois, rations de
foin, vêtements, confort du foyer. Vert, orange ou rouge : on sait où l'on en est.

### 2.3 Boucle longue : la saga (environ 40 heures)

Cinq chapitres, détaillés au §4. Chacun ouvre une région, une technique et une épreuve.

## 3. Départ

* **Date** : 1er maj de l'an 1, juste après la débâcle. Il reste cinq mois avant les premières
  gelées (un peu plus de trois heures de jeu).
* **Lieu** : côte ou rive de lac en zone hémiboréale ou némorale (doc 02 §13), à vue d'un **feu
  d'alarme éteint** sur une hauteur et à moins de 300 blocs d'une ferme abandonnée réparable.
* **Équipement** : vêtements de vadmal, couteau, briquet, un pain croquant, la **Sagabok** vide.
* Un **corbeau** se pose près du joueur le premier matin : c'est le guide discret de tout le jeu.

## 4. La saga des neuf feux

### 4.1 Argument

Les anciens tenaient l'hiver en respect par une chaîne de **neuf feux d'alarme (vårdkasar)**
allumés de colline en colline, du sud jusqu'au glacier. Les feux sont éteints depuis si longtemps
que le **Fimbulvinter**, l'hiver de trois ans sans été, se prépare : les géants du givre
descendent du glacier un peu plus bas chaque année. Rallumer les neuf feux, c'est rendre son
cycle au monde.

### 4.2 Le porteur de braise

Un feu de la chaîne ne s'allume qu'avec une **braise vivante** prise au feu précédent. On la
transporte dans un **pot à braise** (glödkärl) :

* il faut la nourrir d'un charbon de bois par demi-journée ;
* elle meurt si le porteur tombe à l'eau, après une minute sous une pluie battante ou un blizzard
  sans protection (pot couvert, rune Kenaz) ;
* si elle meurt, il faut retourner au dernier feu allumé.

Le voyage lui-même devient l'épreuve : choisir sa saison, sa route (par la glace en hiver, par la
mer en été), ses étapes de refuge en refuge. En LAN, on se relaie.

Chaque feu demande en outre une **condition propre** : c'est l'épreuve du lieu. Toutes ont une
voie de force et une voie de ruse ou de respect (seule voie proposée en folklore « paisible »).

### 4.3 Les cinq chapitres

| Chapitre | Thème | Savoirs acquis | Feux |
| --- | --- | --- | --- |
| **I. Nybyggaren** (le colon) | s'installer et passer le premier hiver | abri, feu, cueillette, pêche, conserves, vêtements, fika à l'ersatz, premier sauna | 1 |
| **II. Bergsmannen** (le maître de forge) | la forêt et le fer | charbon, goudron, bas fourneau, mine, roue à eau, haut fourneau, acier | 2, 3 |
| **III. Sjöfararen** (le marin) | la mer, la glace et le commerce | barque, voile, comptoirs, café, stockfisch, archipel et fjords | 4, 5, 6 |
| **IV. Fjällvandraren** (le marcheur des cimes) | le grand nord | skis, rennes, amitié same, toundra, nuit polaire | 7, 8 |
| **V. Sagan** (la saga) | le glacier et le géant | tungstål, runes complètes, dernière braise | 9 |

### 4.4 Les neuf feux

Les positions sont déterminées par la graine : le feu n est cherché sur un anneau de rayon
croissant autour du départ, dans le biome voulu, sur un maximum local du relief (doc 02 §10.2).
Les feux 2 à 8 peuvent se faire dans l'ordre que l'on veut ; l'équipement (froid, mer) impose
l'ordre naturel.

| # | Feu | Biome, distance | Condition propre (épreuve) | Voie de ruse ou de respect |
| --- | --- | --- | --- | --- |
| 1 | **Hemkasen** (feu du foyer) | colline du départ | avoir passé un hiver ; bâtir le bûcher (40 bûches, goudron) et l'allumer à **Valborg** | aucune : c'est l'apprentissage |
| 2 | **Skogskasen** (feu de la forêt) | Urskog, 1,5 km | Naturfrid ≥ +40 ; réussir une meule de charbonnier dans la clairière du **Skogsrå** | le Skogsrå veille la meule si l'on a replanté et offert |
| 3 | **Bergskasen** (feu de la montagne de fer) | Bergslag, 2,5 km | forger en fer en barres la corbeille du feu ; descendre dans la **Bergasal** où le **Roi de la montagne** garde l'ancienne braise | le combattre ; ou faire sonner une cloche fondue par soi-même ; ou le retenir en énigmes jusqu'à l'aube (il se pétrifie) |
| 4 | **Skärkasen** (feu des écueils) | Skärgård, 3 km | rallumer le phare du large ; apaiser le **Gast** d'une épave en rendant ce qui fut pris | ou marcher le labyrinthe de pierres sans faute, de nuit |
| 5 | **Klintkasen** (feu de la falaise) | Alvar ou Klintkust, 3,5 km | le **Lindorm** dort sous le grand tumulus | l'affronter (acier et argent) ; ou la ruse du conte : porter sept chemises et exiger qu'il mue sept fois, puis le bain de lessive |
| 6 | **Västerkasen** (feu de l'Ouest) | île du Lavafält, 5 km au large | traverser l'océan en knarr ; le **Kraken** se fait passer pour une île sur la route | le repousser (harpons, feu) ; ou le reconnaître à temps (oiseaux absents, eau tiède) et le contourner ; la braise s'allume à la lave |
| 7 | **Forskasen** (feu du rapide) | grand fors d'une vallée du nord, 4 km | le **Näcken** tient le gué | le vaincre à la joute de violon (jeu de rythme) ; ou l'offrande juste |
| 8 | **Fjällkasen** (feu du plateau) | Fjällhed, 6 km | gagner la confiance d'un camp **same** (aide à la transhumance, respect du sejte) qui indique le col ; le **Stállu** barre le passage | le tromper par l'énigme et le piège du conte ; traverser le plateau en hiver, de refuge en refuge, braise vivante |
| 9 | **Iskasen** (feu de la glace) | sommet du Glaciär, 8 km, au **solstice d'hiver** | les huit autres feux brûlent ; le **Jätte** du givre défend la cime | combat final à trois phases (blizzard, avalanche, face à face) où le feu, les runes Sowilo et Thurs et le tungstål comptent ; ou tenir la braise vivante jusqu'à l'aurore, à l'abri d'un cercle de runes Algiz, pendant que le géant s'épuise |

### 4.5 Dénouement

Quand le neuvième feu prend, les neuf lueurs se voient d'un horizon à l'autre et une aurore
couvre le ciel. Le géant redevient montagne. Conséquences :

* le Fimbulvinter est conjuré : générique, puis le monde continue ;
* l'aurore revient à chaque solstice et recharge toutes les runes ;
* les géants ne descendent plus ; le folklore devient bienveillant ;
* titre **Eldbärare** (porteur de feu), costume de fête, et déblocage du mode **Fimbulvinter**
  pour un nouveau monde (hivers allongés, braise plus fragile).

### 4.6 Pression du temps (réglage)

| Réglage | Effet |
| --- | --- |
| **Saga libre** (défaut recommandé) | aucune échéance : le Fimbulvinter n'est qu'un récit, on joue à son rythme |
| Saga pressante | à partir de l'an 4, chaque année sans nouveau feu allonge l'hiver d'un pas de saison ; chaque feu allumé le raccourcit d'autant |
| Fimbulvinter | après victoire : départ avec des hivers longs, tout feu éteint par négligence doit être rallumé |

## 5. Verrous de progression

Ce ne sont pas des portes : ce sont le froid, l'eau et la roche.

| Région | Ce qu'il faut | Vient de |
| --- | --- | --- |
| Zone boréale en hiver | fourrure ou vadmal doublé, conserves | chapitre I |
| Roche dure, minerai en place | abattage au feu, outils de fer | chapitre II |
| Skärgård, fjords | barque goudronnée, ou attendre la glace | chapitres II et III |
| Haute mer | knarr, vivres qui ne se gâtent pas, pierre de soleil | chapitre III |
| Fjäll en hiver, nuit polaire | skis, duvet ou fourrure de glouton, sauna et bain froid avant le départ, réserves de Mys | chapitres III et IV |
| Glacier | crampons d'acier, corde, tungstål contre le géant | chapitre V |

## 6. La Sagabok

Journal du joueur, construit sur l'interface du livre de recettes existant. Onglets :

* **Saga** : le chapitre en cours et ses étapes, écrites comme un récit, jamais comme une liste
  de quêtes à cocher ; les indices recueillis (pierres runiques, récits des habitants, rêves de
  la Saint-Jean).
* **Almanach** : date, saison, durée du jour, prochaine fête, lune, signes du temps ; bilan
  d'hiver.
* **Naturaliste** : les espèces observées, leurs traces et leurs habitudes (se remplit en
  observant, pas en tuant).
* **Folklore** : les règles de conduite apprises pour chaque être.
* **Runes** : les 24, apprises ou non, et leurs effets.
* **Savoir-faire** : chaînes de fabrication découvertes.
* **Bragder** (hauts faits).

## 7. Hauts faits (objectifs libres)

Pour le joueur bâtisseur, éleveur ou flâneur, la saga n'est pas obligatoire. Exemples :

| Haut fait | Condition |
| --- | --- |
| Första vintern | passer le premier hiver sans mourir |
| Lagom | garder le bonus d'équilibre une semaine entière |
| Sju sorters kakor | servir les sept sortes de biscuits à quatre convives |
| Smörgåsbord | un banquet de douze plats à la Saint-Jean, mât dressé |
| Bruksherre | exporter cent barres de fer estampillées |
| Röd stuga | une maison entièrement peinte au rouge de Falun, score de confort maximal |
| Vasaloppet | 90 km à skis en une journée de jeu |
| Islossning | traverser un lac sur les plaques de la débâcle |
| Lämmelår | voir une année à lemmings et son harfang |
| Runmästare | apprendre les 24 runes |
| Naturvän | Naturfrid à +100 |
| Tomtens vän | garder le même tomte dix ans |
| Storsjöodjuret | apercevoir le monstre du grand lac |
| Polstjärnan | atteindre le point le plus au nord pendant la nuit polaire |
| Hembygd | un village de joueurs en LAN avec église à cloche, forge, comptoir et sauna commun |

## 8. Mort, difficulté, modes

* **Mort** : réveil au dernier lit ; l'inventaire reste dans un baluchon sur place (une saison) ;
  perte d'une part de Mys. Option « une seule vie ».
* **Rudesse de l'hiver** : clément, normal (défaut), rude.
* **Folklore** : activé (défaut), paisible, désactivé (les épreuves gardent leur voie de respect).
* **Créatif** : tout le contenu Norrcraft dans l'inventaire créatif, calendrier réglable par
  commande, feux allumables sans condition.
* **LAN** : saga, Naturfrid et calendrier sont ceux du monde de l'hôte et se partagent ; chaque
  joueur a ses jauges. Les mécaniques de groupe (fika, senne sous la glace, långskepp, relais de
  braise, banquets) récompensent le jeu à plusieurs sans l'imposer.

## 9. Rythme visé

| Jalon | Temps de jeu cumulé |
| --- | --- |
| Premier feu de camp, premier abri | 20 minutes |
| Première fika à l'ersatz, premier sauna | 2 à 3 heures |
| Premier hiver passé, feu 1 à Valborg | 8 heures |
| Premier fer de loupe | 6 à 10 heures |
| Haut fourneau en campagne, feux 2 et 3 | 18 heures |
| Première traversée en snipa, premier café véritable | 22 heures |
| Feux 4 à 6 | 30 heures |
| Feux 7 et 8 | 36 heures |
| Feu 9 | 40 heures |

## 10. Critères d'acceptation

* Une partie peut être gagnée de bout en bout sur 3 graines en survie, folklore activé puis
  paisible (test scripté des conditions des neuf feux, positions trouvées par la graine).
* La braise s'éteint et se transporte selon les règles du §4.2.
* Un joueur qui ignore la saga reçoit quand même des objectifs lisibles (bilan d'hiver, hauts
  faits, fêtes).
* La progression est sauvegardée dans `world.json` (`saga`), partagée en LAN.
