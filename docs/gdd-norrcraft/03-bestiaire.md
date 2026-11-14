# 03. Bestiaire nordique : espèces, comportements et règles d'apparition

Tâche 3 du GDD (dépend du doc 02 pour les biomes). Le bestiaire Norrcraft est entièrement neuf :
aucune fiche de `mobs_defaults.json` ni de `mobs_nb.json` n'est reprise, même quand l'animal réel
existe déjà côté vanilla (loup, ours, saumon). Les identifiants sont suédois, ce qui évite aussi les
noms de formes vanilla de `nb-gfx` (`sheep`, `wolf`, `rabbit`, `squid`).

## 1. Ce que le moteur sait faire, ce qu'il faut ajouter

| Aujourd'hui (`nb-ecs/ai.rs`, `spawn.rs`) | À ajouter pour Norrcraft |
| --- | --- |
| Milieux LAND et WATER | Milieu **AIR** (vol, perchoir, plongeon), milieu **AMPHIBIE** (phoque, castor, loutre), milieu **GLACE** (marche sur banquise, trous de respiration) |
| IA passive (errance, fuite), agressive (poursuite, attaque), troupeau | **Machine à états pilotée par les besoins** (§2) |
| Apparition : biome, sol, altitude, profondeur d'eau, plafond par espèce | Saison, heure, météo, lumière, densité cible par biome, cycles de population, hardes persistantes (§4) |
| Butin par mob (`survival.json`) | Butin saisonnier (pelage d'hiver), dépeçage par outil |
| Aucun lien mob ↔ monde | Mobs qui **modifient le monde** : castor, tomte, troll pétrifié, brouteurs |

## 2. Modèle de comportement

### 2.1 Machine à états

Chaque espèce déclare en données (JSON) un jeu d'**états** et de **transitions**. États de base
fournis par le moteur :

`Errer`, `Brouter`, `Boire`, `Dormir`, `Fuir`, `Menacer`, `Charger`, `Traquer` (approche furtive),
`Poursuivre`, `Attaquer`, `Encercler` (meute), `Garder` (territoire, petits), `Suivre` (chef de
harde, joueur), `Migrer`, `Hiberner`, `Parader` (saison des amours), `Voler`, `Se percher`,
`Plonger`, `Nager`, `Fouir`, `Charogner`, `Voler de la nourriture`, `Construire`.

### 2.2 Besoins et perception

* **Besoins** (0 à 1, évoluent avec le temps) : faim, peur, fatigue, froid. Un état est choisi par
  utilité : `score(état) = poids · besoin · opportunité`, le plus haut gagne, avec hystérésis de
  2 s pour éviter les oscillations.
* **Perception** : vue (cône, portée réduite par la nuit, le brouillard, la forêt dense), ouïe
  (le joueur accroupi divise son rayon par 3, courir le double), **odorat sous le vent** : la
  portée est multipliée par 2 quand le joueur est au vent de l'animal et divisée par 2 sinon. Le
  vent (doc 04 §3) rend l'approche de chasse intéressante.
* **Facteurs du joueur** : odeur de surströmming (fait fuir presque tout), fumée, feu (tient les
  loups à 8 blocs), cloche d'église (trolls), appel de kulning (doc 04 §6).

### 2.3 Horloges

* **Journalière** : diurne, nocturne, crépusculaire (élan, chevreuil, castor).
* **Saisonnière** : pelage (lièvre, lagopède, renard polaire, hermine : blancs en hiver),
  hibernation (ours, hérisson, vipère), rut (élan en septembre : agressif), mise bas au printemps
  (petits visibles, mères protectrices), migration (renne, grue, oie, saumon, hareng).
* **Pluriannuelle** : **cycle des lemmings** de 4 ans. `phase = année mod 4` : creux, montée, pic
  (« lämmelår »), effondrement. La densité de lemmings vaut × 0,2, × 1, × 4, × 0,5 ; celle de leurs
  prédateurs (harfang, renard polaire, labbe, hermine) suit avec un an de retard. Une année de pic
  transforme le fjäll : nourriture facile, harfangs partout.

## 3. Fiches

Colonnes : T = tempérament (P passif, N neutre, H hostile, A allié possible). PV indicatifs.

### 3.1 Herbivores et petits mammifères

| id | Animal | Biomes | T | PV | Comportement propre | Butin |
| --- | --- | --- | --- | --- | --- | --- |
| `alg` | Élan | Urskog, Myr, Sjöland, Älv, Brännfält | N | 30 | crépusculaire ; broute les jeunes arbres (y compris les pousses plantées par le joueur) et les nénuphars en nageant ; traverse lacs et fjords ; **charge** pendant le rut et pour défendre son veau ; bloque les pistes | viande, peau, bois d'élan |
| `ren` | Renne | Fjällhed, Fjällbjörkskog, Palsmyr (été) ; Tallhed, Urskog (hiver) | P / A | 16 | **harde persistante migratrice** (§4.4) ; gratte la neige pour le lichen ; apprivoisable avec du lichen et du sel : bât, traîneau (ackja), lait | viande, peau d'hiver, bois, tendon |
| `radjur` | Chevreuil | Bokskog, Hagmark | P | 8 | très craintif, aboie pour alerter ; mange les potagers non clôturés | viande, peau fine |
| `myskoxe` | Bœuf musqué | Fjällhed (rare) | N | 40 | à l'approche d'un prédateur ou du joueur : **formation en cercle**, petits au centre ; charge si l'on franchit 5 blocs | laine fine (qiviut), viande |
| `skogshare` | Lièvre variable | Urskog, Fjällbjörkskog, Fjällhed | P | 3 | zigzague, se fige (invisible sur la neige en hiver) | viande, peau blanche ou brune |
| `fjallammel` | Lemming | Fjällhed, Palsmyr, Kalfjäll | P | 1 | cycle de 4 ans ; l'hiver vit **sous la neige** (tunnels, invisible) ; acculé, il fait face et couine au lieu de fuir | petite peau |
| `baver` | Castor | Älv, Sjöland | P | 10 | **bâtisseur** : abat lentement les trembles et bouleaux à moins de 12 blocs de l'eau, entretient son barrage (répare les brèches) ; claque la queue (alerte) et plonge | peau, castoréum |
| `ekorre` | Écureuil roux | Urskog, Bokskog, Tallhed | P | 2 | arboricole ; cache des cônes ; trahit la présence de martres et de lynx par ses cris | |
| `igelkott` | Hérisson | Hagmark, By | P | 2 | nocturne ; mange les limaces du potager (bonus de récolte) ; hiberne sous un tas de feuilles | |
| `hermelin` | Hermine | Fjällhed, Hagmark | N | 3 | chasse lemmings et souris dans les celliers (protège les réserves) ; blanche en hiver | fourrure précieuse |

### 3.2 Prédateurs terrestres

| id | Animal | Biomes | T | PV | Comportement propre | Butin |
| --- | --- | --- | --- | --- | --- | --- |
| `varg` | Loup | Urskog, Tallhed, Fjällbjörkskog, Myr | N / H | 14 | **meute territoriale** de 3 à 7 avec chef ; hurle la nuit (les autres meutes répondent : on entend les territoires) ; suit une proie à distance, **encercle** puis attaque quand elle est blessée ou fatiguée ; évite le feu et les villages ; affamée en fin d'hiver, la meute devient hostile et attaque le bétail | peau, croc |
| `bjorn` | Ours brun | Urskog, Fjällbjörkskog, Bergslag | N | 45 | baies et fourmilières en été, saumons aux rapides en automne ; **hiberne** de novembre à mars dans une tanière (le réveiller est la pire idée du jeu) ; se dresse (avertissement) avant de charger ; reculer lentement le calme, courir déclenche la poursuite ; pille ruches et garde-manger bas | peau, graisse, viande |
| `lo` | Lynx | Urskog, Högkust | N | 12 | **affût** : invisible tant qu'il est immobile, bondit sur lièvres et chevreuils ; ne s'attaque jamais au joueur, disparaît s'il est vu | fourrure tachetée |
| `jarv` | Glouton | Fjällbjörkskog, Fjällhed | N | 18 | **charognard et voleur** : pille les séchoirs, les pièges et les carcasses laissées ; emporte un objet de nourriture et l'enterre ; féroce si acculé | fourrure qui ne givre pas |
| `fjallrav` | Renard polaire | Fjällhed, Kalfjäll, Packis | P | 5 | suit les ours blancs et les loups pour les restes ; curieux, s'approche des camps ; pelage saisonnier ; rare hors année à lemmings | fourrure blanche |
| `rodrav` | Renard roux | Hagmark, Bokskog, Sjöland | P | 6 | mulotte dans la neige (saut en cloche) ; vole les poules la nuit si le poulailler est ouvert | fourrure |
| `mard` | Martre | Urskog | P | 4 | chasse l'écureuil dans les arbres | fourrure |
| `isbjorn` | Ours blanc | Packis | H | 60 | **traque à l'odeur** sur de longues distances, guette aux trous de phoque ; nage entre les plaques ; seul vrai prédateur du joueur parmi les animaux | peau, graisse |
| `huggorm` | Vipère péliade | Tallhed, Alvar, Skärgård, Myr | N | 2 | se chauffe sur les pierres au soleil (printemps, été) ; mord si on lui marche dessus : **venin** (lent, soigné par repos au chaud ou remède) ; hiberne | peau |

### 3.3 Oiseaux (milieu AIR)

| id | Animal | Biomes | T | Comportement propre | Butin ou rôle |
| --- | --- | --- | --- | --- | --- |
| `tjader` | Grand tétras | Urskog, Tallhed | N | **parade** au printemps sur des places de chant : le mâle attaque tout ce qui approche, joueur compris ; l'hiver, mange les aiguilles au sommet des pins | viande, plumes |
| `orre` | Tétras lyre | Myr, Brännfält | P | parades collectives à l'aube sur la tourbière (son roucoulé qui porte loin) | viande |
| `ripa` | Lagopède | Fjällhed, Fjällbjörkskog | P | plumage saisonnier ; **dort dans un trou de neige** d'où il jaillit si on passe dessus ; se piège au collet | viande, plumes blanches |
| `trana` | Grue | Myr, Strandäng, Hagmark | P | arrive en mars, repart en septembre en grands V sonores ; **danse** en couple ; son retour marque le printemps au calendrier | plume (rare) |
| `sangsvan` | Cygne chanteur | Sjöland, Bottenhav | N | couple territorial sur un lac, charge les barques trop proches du nid | duvet |
| `grasgas` | Oie cendrée | Strandäng, Skärgård | P | migratrice, en bandes ; sentinelle qui donne l'alerte ; domesticable (oie de la Saint-Martin) | viande, plumes, graisse |
| `ejder` | Eider | Skärgård, Fjord | P | niche à terre ; installer des **abris à eider** donne du duvet (ejderdun), meilleur isolant du jeu, sans tuer l'oiseau | duvet |
| `lunne` | Macareux | Fjord, Klintkust, Lavafält | P | colonies en terrier sur les corniches, vol en rase-mottes, bec plein de poissons | œufs, plumes |
| `havsorn` | Pygargue | Fjord, Skärgård, Älv | N | plane très haut ; **vole le poisson** au bout de la ligne du joueur ou sur les séchoirs ; pêche en surface | plume de penne (flèches) |
| `fjalluggla` | Harfang des neiges | Fjällhed | N | présent surtout les années à lemmings ; défend son nid en piquant | plume |
| `kattuggla` | Chouette hulotte | Bokskog, Urskog | P | nocturne ; son hululement annonce une nuit calme (absence de créatures) | |
| `korp` | Grand corbeau | partout sauf mer | P / A | très intelligent : **mène aux carcasses** et, nourri régulièrement, mène aux pierres runiques et aux lieux de saga ; imite des sons ; vole les objets brillants laissés au sol | guide |
| `hackspett` | Pic noir | Urskog, Brännfält | P | tambourine ; ses loges signalent les arbres creux (miel sauvage, cachettes) | |
| `mas` | Goéland, sterne | toutes côtes | P | tournoie au-dessus des bancs de harengs (indice de pêche) ; la sterne attaque près de son nid | œufs |
| `goken` | Coucou | Hagmark, Fjällbjörkskog | P | on ne le voit pas ; la **direction de son premier chant** au printemps donne un présage (bonus léger selon le point cardinal) | |

### 3.4 Faune aquatique et amphibie

| id | Animal | Milieux | T | Comportement propre | Butin |
| --- | --- | --- | --- | --- | --- |
| `lax` | Saumon | Västerhav, Bottenhav → Älv | P | **remonte les rivières** de juin à septembre ; saute les fors de 6 blocs (visible, pêchable au harpon ou à l'épuisette au pied des chutes) ; fraie en amont puis meurt (nourrit ours et aigles) | saumon |
| `oring` | Truite | Älv, lacs | P | se tient derrière les rochers dans le courant | truite |
| `roding` | Omble chevalier | lacs de Fjällhed et Kalfjäll | P | seul poisson des lacs d'altitude ; mord surtout sous la glace | omble |
| `gadda` | Brochet | Sjöland, Bottenhav (roselières) | N | **embuscade** dans les roseaux ; individus géants trophées ; attaque canetons et lignes | brochet |
| `abborre` | Perche | Sjöland, Bottenhav | P | bancs ; la prise la plus facile de la pêche sur glace | perche |
| `stromming` | Hareng de la Baltique | Bottenhav, Västerhav | P | **bancs immenses** saisonniers ; pêche au filet ; matière première du surströmming | hareng |
| `torsk` | Morue | Västerhav, Fjord | P | vient frayer en fin d'hiver dans les fjords (skrei) : saison du séchage | morue |
| `al_fisk` | Anguille | Bokskog (ruisseaux), côtes du sud | P | nocturne ; pêche à la nasse d'août aux nuits sans lune | anguille |
| `krafta` | Écrevisse | Sjöland, Älv | P | nocturne ; se prend à la nasse appâtée à partir d'août (fête des écrevisses) | écrevisse |
| `blamussla` | Moule, huître | Västerhav, Fjord | | bloc vivant récoltable à la main | coquillages |
| `brannmanet` | Méduse à crinière | Västerhav, Fjord, fin d'été | N | dérive ; brûlure au contact (se baigner en août a un prix) | |
| `grasal` | Phoque gris | Skärgård, Västerhav | P | colonies sur les écueils, plonge à l'approche ; mange le poisson des filets | peau, graisse (huile de lampe) |
| `vikare` | Phoque annelé | Bottenhav, Packis | P | l'hiver vit sous la glace et entretient des **trous de respiration** (repérables, pêche et chasse) | peau, graisse |
| `valross` | Morse | Packis | N | groupes sur la glace ; défend le groupe ; renverse les petites barques | ivoire, cuir épais |
| `tumlare` | Marsouin | Västerhav, Bottenhav | P | accompagne les bateaux ; signale les bancs de harengs | |
| `utter` | Loutre | Älv, Sjöland, Fjord | P | joueuse (glissades sur la neige) ; pêche ; apporte parfois un poisson au joueur qui l'a nourrie | |
| `hakarl` | Requin du Groenland | Djuphav, fond des Fjords | N | très lent, très vieux ; mord les lignes de fond ; sa chair est toxique sauf fermentée (doc 04) | chair, huile |
| `val` | Baleine franche | Västerhav, Djuphav | P | géante paisible, souffle visible de loin ; ne se chasse pas (la tuer effondre le Naturfrid) ; échouages rares = manne | |

### 3.5 Insectes et ambiance

| id | Rôle |
| --- | --- |
| `mygg` | **Nuée de moustiques** : en Myr, Palsmyr, Urskog et Fjällbjörkskog de juin à août, surtout au crépuscule et sans vent. Pas de dégâts : démangeaison qui abaisse le Mys et gêne la visée. Parades : fumée, goudron de bouleau, vent, altitude, moustiquaire |
| `knott` | Simulies près des rapides : même effet, plus bref |
| `bi` | Abeilles : ruches de paille (miel, cire) ; essaims sauvages dans les arbres creux |
| `skogsmyra` | Fourmilière géante (bloc) : indique le sud (plus plate de ce côté) ; l'ours la défonce |
| `fjaril`, `lysmask`, `trollslanda` | papillons, vers luisants de juin, libellules : indicateurs de saison et de Mys |

### 3.6 Animaux domestiques

| id | Animal | Apport | Particularité |
| --- | --- | --- | --- |
| `fjallko` | Vache de montagne | lait (beurre, fromage, filmjölk) | donne plus à la fäbod en été ; doit être nourrie au foin l'hiver |
| `get` | Chèvre | lait, fromage brun | broute tout, s'échappe |
| `gutefar` | Mouton de Gotland | laine, viande | tonte au printemps |
| `linderodssvin` | Porc | viande de Noël | se nourrit de faînes en hêtraie |
| `hona`, `tamgas` | Poule, oie | œufs, plumes | à enfermer la nuit (renard) |
| `fjordhast` | Cheval de fjord | trait, bât, manège de mine | sobre, pied sûr en montagne |
| `islandshast` | Cheval islandais | monture | allure **tölt** : rapide et sans secousse (on peut manger ou viser en selle) |
| `jamthund` | Chien d'élan | compagnon | **marque l'arrêt** devant le gibier et aboie pour le tenir ; alerte sur loups et ours ; tire un petit traîneau |
| `skogskatt` | Chat des forêts | compagnon | chasse les souris du cellier, ronronne (Mys) |

Tout le bétail a un besoin de **fourrage d'hiver** : sans foin engrangé, il dépérit. C'est le
moteur de la fenaison (doc 04 §6).

### 3.7 Êtres du folklore

Ils remplacent les « monstres » génériques. Presque tous obéissent à des **règles de conduite**
plutôt qu'à la force : les connaître (par les pierres runiques, les habitants, la Sagabok) fait
partie du jeu. Réglage de création : folklore activé, paisible (présents mais jamais hostiles) ou
désactivé.

| id | Être | Où, quand | Règles |
| --- | --- | --- | --- |
| `troll` | Troll | grottes, sous les ponts, Urskog profond, Bergslag ; la nuit | fort, lent, lance des rochers ; **se pétrifie au soleil** : il devient un bloc erratique (minable en « pierre de troll ») ; fuit le son des cloches ; avare, on peut le duper ou le payer |
| `tomte` | Tomte | toute ferme habitée par le joueur | invisible sauf à l'aube ; si on lui laisse un bol de bouillie **avec une noix de beurre** (chaque semaine, et impérativement la nuit de Noël), il soigne les bêtes, rentre le foin avant la pluie, éteint les départs de feu ; négligé ou insulté, il fait tourner le lait, noue les queues des vaches, puis s'en va |
| `skogsra` | Skogsrå (dame de la forêt) | Urskog, près des charbonnières | belle de face, dos creux comme un tronc ; incarne le Naturfrid (doc 04 §7) : au chasseur respectueux elle réveille le charbonnier quand la meule prend feu et rabat le gibier ; au pilleur elle fait **perdre le nord** (boussole et repères faussés, sentiers qui tournent) |
| `nacken` | Näcken (l'homme des eaux) | fors et étangs, nuits d'été | joue du violon nu dans le courant ; sa musique **attire** (le joueur dérive vers l'eau) ; contre une offrande (trois gouttes, un gigot noir, du tabac) il enseigne un air de violon (doc 04 §8) ; dire son nom le chasse |
| `backahast` | Bäckahäst (cheval de ruisseau) | rives brumeuses | cheval blanc magnifique ; qui le monte est emporté au fond ; bridé avec un mors d'acier, il laboure comme dix chevaux pendant une saison |
| `vittra` | Vittror (peuple souterrain) | Fjällbjörkskog, Tallhed du nord | invisibles, élèvent un bétail blanc ; **ne pas bâtir sur leurs chemins** (signalés par des indices : bétail nerveux, objets déplacés) ; les saluer en versant un peu de lait porte bonheur |
| `alvor` | Älvor (elfes des brumes) | Hagmark, Strandäng, à l'aube dans la brume | dansent en rond ; laissent des **ronds de sorcière** ; entrer dans la danse fait sauter le temps (une nuit, parfois une saison) ; leur souffle donne une fièvre |
| `lyktgubbe` | Feu follet | Myr, nuits d'automne | lueur qui s'éloigne ; mène soit à un gisement de myrmalm ou un trésor, soit dans une gouille profonde ; un sou d'argent posé au sol le rend honnête |
| `mara` | Mara (cauchemar) | lit du joueur, hiver | si le Mys est très bas au coucher : sommeil sans repos ; parade : acier sous l'oreiller, chaussures pointées vers la porte |
| `gast` | Gast (revenant) | tumulus profanés, épaves | n'apparaît que si l'on pille une sépulture ; suit le pilleur la nuit jusqu'à restitution ou jusqu'à ce qu'une cloche sonne |
| `draug` | Draug (noyé) | Västerhav, Fjord, tempêtes | rame dans une demi-barque ; son cri annonce un naufrage ; aborde les bateaux par gros temps |
| `lussi` | Lussi et sa chasse | nuit du 13 décembre | **la nuit la plus dangereuse** : cortège dans le ciel ; rester éveillé, à l'intérieur, lumières allumées (doc 04 §8) |
| `stallo` | Stállu | Fjällhed, Kalfjäll | géant fort mais sot des contes sames ; se vainc par la ruse (énigmes, pièges) ; son chien garde un trésor |
| `lindorm` | Lindorm | grand tumulus, sous un chêne vénérable | dragon sans ailes ; épreuve de la saga (doc 05) |
| `bergakung` | Roi de la montagne | Bergasal | souverain des trolls ; épreuve de la saga |
| `kraken` | Kraken | Djuphav | prend d'abord l'apparence d'une île ; épreuve de la saga |
| `jatte` | Jätte (géant du givre) | Glaciär | épreuve finale de la saga |
| `storsjoodjur` | Monstre du grand lac | le plus grand lac du monde, très rare | inoffensif ; le voir est un haut fait |

## 4. Règles d'apparition

### 4.1 Champs de données ajoutés à `MobConfig`

En plus des champs actuels (`spawnBiomes`, `spawnSurfaces`, `spawnMinY`, `spawnMaxY`,
`spawnMaxWaterDepth`, `maxCount`, tailles de groupe) :

| Champ | Sens |
| --- | --- |
| `densityPerBiome` | individus visés pour 64 × 64 blocs, par biome |
| `seasonCurve` | 4 à 12 facteurs sur l'année (0 = absent : migrateur parti, hibernant) |
| `timeOfDay` | diurne, nocturne, crépusculaire, tous |
| `weather` | facteurs par météo (brume pour älvor, tempête pour draug) |
| `light`, `shelter` | lumière maximale, besoin d'un couvert |
| `cycle` | nom d'un cycle pluriannuel et décalage (lemmings) |
| `nearFeature` | exige une structure ou un décor (fors pour l'ours en automne, corniche pour le macareux, trou de glace pour le phoque) |
| `avoid` | rayon d'évitement : feu, cloche, village, joueur |
| `persistence` | éphémère, sédentaire (lié à un lieu), harde (§4.4), unique |
| `naturfrid` | facteur selon la réputation du joueur (doc 04 §7) |

### 4.2 Densité cible

Pour l'espèce e autour du joueur (zone active de rayon R) :

```
cible(e) = Σ_biomes surface_b · densité(e, b) / 4096
           × saison(e, jour) × heure(e) × météo(e) × cycle(e, année) × naturfrid(e)
```

À chaque tentative, l'espèce est tirée avec une probabilité proportionnelle à son **déficit**
`max(0, cible − présents)`. Le point d'apparition est tiré hors de vue (plus de 24 blocs, hors du
cône de vision) et validé par les prédicats d'habitat. Disparition : hors de vue et au-delà de
R + 16, sauf sédentaires et hardes.

### 4.3 Grille d'habitat (résumé)

| Biome | Communs | Occasionnels | Rares |
| --- | --- | --- | --- |
| Västerhav, Fjord | strömming, torsk, mas, grasal | tumlare, havsorn, lunne, brannmanet (été) | val, draug |
| Djuphav | torsk | hakarl | kraken |
| Bottenhav | stromming, abborre, vikare | gadda, sangsvan, tumlare | épaves hantées |
| Packis | vikare, fjallrav | valross | isbjorn |
| Skärgård | ejder, mas, grasal, grasgas | huggorm, havsorn, utter | |
| Klintkust, Högkust | mas, lunne | lo, huggorm | gast |
| Strandäng | grasgas, trana | rodrav | alvor |
| Bokskog | radjur, ekorre, kattuggla | rodrav, igelkott, al_fisk | lindorm (site) |
| Hagmark, Alvar | radjur, skogshare, goken, bi | rodrav, hermelin, huggorm | alvor, tomte (fermes) |
| Sjöland | abborre, gadda, krafta, baver | alg, utter, sangsvan | storsjoodjur |
| Urskog | ekorre, tjader, hackspett, mygg (été) | alg, bjorn, varg, mard, korp | lo, skogsra, troll |
| Tallhed | tjader, skogshare, huggorm | varg, ren (hiver) | vittra |
| Myr | trana, orre, mygg (été) | alg, varg | lyktgubbe |
| Älv | oring, lax (été), baver, knott | utter, bjorn (automne), havsorn | nacken, backahast |
| Brännfält | orre, hackspett | alg | |
| Bergslag | korp | bjorn, varg | troll, bergakung (site) |
| Fjällbjörkskog | ripa, skogshare, ren (été), mygg | alg, jarv, bjorn | vittra |
| Fjällhed, Palsmyr | fjallammel (cycle), ren (été), ripa | fjallrav, fjalluggla (cycle), jarv, roding (lacs) | myskoxe, stallo |
| Kalfjäll, Glaciär | ripa | fjallrav, korp | jatte (site) |
| Lavafält | lunne, mas | islandshast (sauvage), fjallrav | |

### 4.4 Hardes persistantes et migrations

Les rennes ne sont pas tirés au hasard : chaque **harde** (une par cellule de 1024 blocs en zone
subarctique) est un objet léger du monde (position, effectif, état), simulé même quand ses chunks
ne sont pas chargés. Sa position cible suit le calendrier : fjäll en été, forêt à lichens en
hiver, avec deux transhumances (mai et octobre) le long des vallées. Quand le joueur approche, la
harde se matérialise en individus ; quand il s'éloigne, l'effectif (pertes comprises) est
réabsorbé. Même mécanisme pour : les meutes de loups (territoire fixe), les vols de grues et
d'oies (traversent le ciel aux dates de migration), les bancs de harengs et la remontée des
saumons (front qui avance vers l'amont de bief en bief).

### 4.5 Interactions avec le monde

* **Broutage** : élan et chevreuil mangent les pousses et cultures non clôturées (clôture de
  perches = parade).
* **Castor** : abat un arbre voisin toutes les quelques journées de jeu, plafonné par colonie ;
  détruire le barrage vide l'étang en amont (les blocs d'eau au-dessus du niveau aval sont retirés
  par la surcouche du barrage).
* **Troll pétrifié** : écrit un décor « bloc erratique » à sa position.
* **Glouton, pygargue, renard, ours** : vol dans les conteneurs ouverts (séchoirs, garde-manger
  bas, poulailler). Le garde-manger sur poteau (njalla) et le cellier fermé sont sûrs.

## 5. Chasse, piégeage et éthique de jeu

* Arc, épieu, collets (lagopède, lièvre), nasses, filets, fosses à élan (structure historique).
* **Pistes** : empreintes dans la neige fraîche (décalcomanies temporaires), crottes, arbres
  écorcés ; lire une piste donne l'espèce et la direction.
* Dépeçage : couteau requis ; rendement selon l'outil ; pelage d'hiver plus chaud et plus cher.
* **Naturfrid** : tuer plus qu'on ne consomme, tuer une femelle suitée, une baleine ou un animal
  sacré, abaisse la réputation et, par elle, les densités et l'humeur du folklore.

## 6. Critères d'acceptation

* Chaque espèce possède des milliers de points d'apparition valides dans des mondes générés
  (3 graines), comme le test `every_animal_has_a_viable_habitat_in_generated_worlds` actuel.
* Aucun identifiant ne coïncide avec un mob vanilla ni avec une forme vanilla de `nb-gfx`.
* Scénarios automatisés : pétrification du troll à l'aube ; ours absent en hiver hors tanière ;
  pelage blanc au solstice d'hiver ; année de pic des lemmings (densité × 4) ; harde de rennes
  retrouvée au bon étage selon la saison ; meute qui encercle ; pygargue qui vole un poisson.
* Budget : 120 entités actives simultanées au plus, mise à jour de l'IA en 1 ms par tick.
