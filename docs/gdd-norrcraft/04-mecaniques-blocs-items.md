# 04. Mécaniques nordiques, blocs et objets

Tâche 4 du GDD. Les mécaniques prolongent le moteur de survie existant (faim, chaleur, endurance,
outils, cultures, pêche, craft par stations, `docs/SURVIE.md`, `docs/CRAFT.md`) : mêmes systèmes,
nouvelles tables, plus les systèmes propres à Norrcraft activés par le profil (doc 01 §3.1).

## 1. Cadre

* **Époque** : un Nord préindustriel, de l'âge de Vendel aux forges du XVIIIe siècle. Bois, fer,
  eau motrice, voile. Ni poudre, ni vapeur, ni électricité.
* **Ton** : le monde n'est pas hostile, il est **exigeant**. L'adversaire est l'hiver ; la réponse
  est la prévoyance, la chaleur du foyer et les autres (LAN).
* **Trois idées directrices** : *les saisons commandent tout* ; *lagom* (ni trop ni trop peu : la
  mesure est récompensée) ; *mys* (le confort partagé est une ressource).

## 2. Calendrier, saisons et lumière

### 2.1 Année

12 mois de `d` jours (réglage de création : d = 2, année de 24 jours, **défaut** ; 4 ou 8 pour des
années longues). Un jour dure 1200 s (inchangé). Année par défaut : 8 heures de jeu.

| Saison | Mois | Faits marquants |
| --- | --- | --- |
| **Vår** (printemps) | mars, avril, maj | débâcle, retour des grues, anémones, semailles, sève de bouleau, veaux |
| **Sommar** (été) | juni, juli, augusti | soleil de minuit, moustiques, fenaison, baies, estive, écrevisses |
| **Höst** (automne) | september, oktober, november | champignons, rut de l'élan, moissons, chasse, couleurs, premières gelées, abattage |
| **Vinter** (hiver) | december, januari, februari | neige, glace, nuit polaire, aurores, pêche sur glace, bûcheronnage, veillées |

### 2.2 Durée du jour

Soit a ∈ [0, 1) la fraction de l'année (0 = solstice d'hiver) et λ la « latitude » du joueur,
tirée de T₀ lissé (doc 02 §4) : λ = 1 − T₀.

```
A(λ) = 0,16 + 0,44 · λ                      amplitude saisonnière
jour(a, λ) = clamp(0,5 − A(λ) · cos(2π · a), 0, 1)     fraction éclairée des 1200 s
```

Au sud (λ = 0,1) : de 30 % à 70 % de jour. Au-delà du **cercle polaire** (A > 0,5, soit λ > 0,77) :
**soleil de minuit** autour du solstice d'été et **nuit polaire** autour du solstice d'hiver
(seulement un crépuscule bleu à midi). Calcul à l'exécution (pas dans la génération) : le cosinus
est permis ; en LAN c'est l'hôte qui fait foi. La lumière du ciel du moteur est multipliée par une
courbe de hauteur du soleil ; λ est lissé sur le déplacement du joueur pour éviter tout saut.

### 2.3 Surcouche saisonnière des chunks

La génération produit l'état de fin d'été. Une fonction pure `surcouche(chunk, jour de l'année)`
applique, au chargement puis à chaque **pas de saison** (8 pas par an) sur les chunks chargés :

| Élément | Règle |
| --- | --- |
| Manteau neigeux | épaisseur en couches (0 à 8) = `cumul(a, T) · exposition(x, z)` ; exposition = 1 à découvert, 0,5 sous les conifères, congères sous le vent des crêtes (+2) ; au-delà de 8 : neige pleine |
| Glace des lacs et du Bottenhav | épaisseur e = 0 (libre), 1 (glace neuve), 2 (glace portante), 3 (glace épaisse) selon les degrés-jours de gel cumulés ; les lacs gèlent avant la mer, les rapides jamais |
| Feuillages | teinte par saison (vert tendre, vert, or et rouge, nu) : paramètre du mailleur, pas de nouveaux blocs ; les feuillus perdent leurs feuilles en hiver (feuillage rendu en branches nues) |
| Flore saisonnière | anémones (printemps seulement), baies mûres (juli à september), champignons (august à oktober), moustiques |
| Structures d'hiver | cabanes de pêche sur glace, pistes damées |

Ce qui a été posé ou modifié par le joueur n'est jamais écrasé : la surcouche ne touche que les
colonnes dont le bloc de surface est naturel ou déjà de la neige ou de la glace (drapeau par
colonne dans le chunk).

### 2.4 Croissance des plantes

Vitesse de croissance = `base · jour(a, λ) · f(T)` avec f nul sous le gel. Conséquences voulues :
saison courte mais **très rapide sous le soleil de minuit** au nord, saison longue et lente au sud ;
le seigle d'hiver se sème en automne et passe l'hiver sous la neige.

## 3. Météo, froid et vent

### 3.1 Météo

États : clair, voilé, pluie, bruine, brume, orage, neige, **blizzard** (snöstorm), tempête de mer,
grand froid clair. Chaîne de Markov dont les probabilités dépendent de la saison, de O (façade
atlantique = pluie fréquente) et de l'altitude. Signes avant-coureurs lisibles (halo, vol bas des
hirondelles, ciel rouge, silence des oiseaux) : la météo se prévoit sans interface.

* **Blizzard** : visibilité de 6 blocs (voile blanc), vent fort, refroidissement × 3. On survit en
  atteignant un refuge, en suivant les cairns, ou en creusant un **abri de neige** (trou dans une
  congère : abrité et isolé).
* **Tempête de mer** : vagues (oscillation visuelle et poussée des bateaux), naufrage des petites
  barques, apparition du Draug ; au matin, **ambre** et bois flotté sur les plages du sud.
* **Aurore boréale (norrsken)** : nuits claires d'hiver, d'autant plus fréquente que λ est grand.
  Éclaire faiblement, recharge les runes (§7.2), élève le Mys.

### 3.2 Vent

Vecteur global lentement variable (bruit dans le temps), module 0 à 1, plus effets locaux :
canalisé le long de l'axe des fjords (× 1,5), brise de mer le jour et de terre la nuit, abri sous
le vent des reliefs et en forêt. Le vent sert à la voile (§5), aux moulins, au séchage, à l'odorat
des animaux (doc 03 §2.2), au refroidissement et à chasser les moustiques.

### 3.3 Température ressentie

Le moteur actuel calcule une chaleur de 0 à 100 à partir d'une température ressentie. Norrcraft
garde ce cadre et remplace la table :

```
T_air     = T(x, z, y)  +  saison(a) · (0,10 + 0,12 · C)  +  jour_nuit · 0,04     (échelle 0 à 1, doc 02)
ressenti  = T_air − 0,10 · vent · (1 − abri) − 0,15 · mouillé + sources + isolation
```

L'intérieur des terres a des hivers plus durs et des étés plus chauds que la côte (terme en C).
**Mouillé** : pluie sans vêtement de dessus, chute à l'eau, sueur (courir longtemps trop couvert).
Se sécher près d'un feu prend une minute. Tomber dans l'eau glacée : choc (endurance à zéro) et
chute rapide de la chaleur.

Vêtements : les 4 emplacements actuels, chaque pièce ayant **isolation**, **coupe-vent** et
**imperméabilité**. Matières : écorce de bouleau et herbe (début), peau, vadmal (drap de laine),
fourrure d'hiver, fourrure de glouton (ne givre pas), duvet d'eider (la meilleure isolation),
toile goudronnée (pluie), cuir de phoque (mer).

## 4. Glace, neige et mers gelées

### 4.1 États de la glace

| Glace | Porte | Indices | Usage |
| --- | --- | --- | --- |
| Neuve (nyis), e = 1 | un joueur à plat ventre ou à skis ; **cède** sous un joueur debout après un craquement d'avertissement, sous tout animal lourd | sombre, lisse, chante | danger |
| Portante (kärnis), e = 2 | joueur, traîneau, renne | bleutée | patin, pêche, routes de glace |
| Épaisse, e = 3 | cheval et traîneau chargé | blanche, enneigée | transport lourd, découpe de blocs |
| De mer | comme e mais plus fragile près des courants, des roselières et des embouchures | grise | idem |
| Banquise (Packis) | tout | crêtes, chenaux | monde à part |

Tomber à l'eau : sans **poinçons à glace** (isdubbar) le bord casse deux fois sur trois quand on
essaie de sortir. À la **débâcle** (islossning, avril) la glace redevient neuve puis se fragmente
en plaques dérivantes pendant un pas de saison.

### 4.2 Ce que la glace permet

* **Routes d'hiver** : lacs, tourbières et golfe deviennent des plaines. Les îles du Skärgård sont
  accessibles à pied ; une tourbière infranchissable en été se traverse en ligne droite. La carte
  du monde change avec la saison.
* **Pêche sur glace** : tarière (isborr) → trou (isvak) ; canne courte (pimpel) ; lignes dormantes
  à drapeau (angeldon) relevées plus tard ; **senne sous la glace** (isnot) à deux trous et deux
  joueurs en LAN pour les gros coups.
* **Chasse au phoque** aux trous de respiration.
* **Récolte de glace** : scie à glace → blocs de glace stockés sous la sciure dans une glacière
  (ishus) : ils alimentent le cellier à glace tout l'été (§6.3).
* **Glisse** : voir §4.4.

### 4.3 Neige

Marcher dans plus de 3 couches ralentit (jusqu'à −60 % à 8 couches). Parades : skis, raquettes,
pistes damées par le passage répété (une colonne piétinée 3 fois devient « neige tassée »), pelle.
La neige isole : un abri de neige, un toit enneigé, des cultures sous la neige sont protégés.
**Avalanche** : sur une pente ≥ 2 pour 1 avec au moins 5 couches, en Fjord, Kalfjäll et
Fjällhed, après une chute de neige ou au redoux, un bruit fort (mine, cor, course) peut déclencher
une coulée : le manteau de la bande de pente descend le long du gradient et s'accumule en bas ;
tout ce qui est sur le trajet est emporté et enseveli (dégâts, puis se dégager à la pelle). Signes :
« whoumf » sourd, fissures dans la neige.

### 4.4 Se déplacer en hiver

| Moyen | Effet |
| --- | --- |
| Skis (skidor) et bâton | × 1,6 sur neige, glisse en descente, ne s'enfonce pas ; fart au goudron pour mieux glisser |
| Raquettes (snöskor) | pas de ralentissement, pas de glisse ; faciles à fabriquer |
| Patins de randonnée (långfärdsskridskor) | × 2,5 sur glace vive |
| Luge à pied (spark) | × 2 sur glace et routes tassées, porte un coffre |
| Pulka | coffre tiré derrière le skieur |
| Ackja | traîneau à renne : rapide, 2 coffres |
| Traîneau à cheval (släde) | charges lourdes sur glace épaisse et routes |
| Char à glace (isjakt) | voilier sur patins : le véhicule le plus rapide du jeu, par vent, sur grand lac ou golfe gelé |

## 5. Fjords et navigation

### 5.1 Bateaux

| Bateau | Équipage et charge | Propulsion | Accès |
| --- | --- | --- | --- |
| Radeau de rondins | 1, rien | perche | début |
| **Eka** (barque à clins) | 2, 1 coffre | rames | planches, goudron |
| **Snipa** (voile aurique) | 3, 3 coffres | voile et rames | toile de lin, cordage de chanvre |
| **Knarr** (navire de charge) | 6, 8 coffres, bétail | voile carrée | chapitre III ; chantier naval |
| **Långskepp** | 8, rapide | voile et 6 rames (LAN) | haut fait |

Voile : vitesse fonction de l'angle au vent (nulle à moins de 45° du lit du vent, maximale au
largue). Dans un fjord, le vent canalisé rend la remontée ou la descente très rapide selon l'heure
(brises). Coque non goudronnée : prend l'eau (écoper). En hiver, les bateaux se tirent au sec.

### 5.2 Ce que le fjord offre et coûte

* **Abri** : pas de houle au fond du fjord ; mouillages sûrs pendant les tempêtes.
* **Morue d'hiver (skrei)** de février à avril, pêchée à la ligne de fond puis **séchée sur
  claies (hjell)** : le séchage ne réussit que par temps froid, sec et venté, donc sur ces côtes à
  cette saison. Le **stockfisch** ne se gâte jamais : nourriture de voyage et monnaie d'échange.
* **Falaises à oiseaux** : œufs et duvet, atteints en rappel par une corde fixée en haut.
* **Cascades** des vallées suspendues : meilleurs sites de roue à eau du jeu.
* **Stéatite**, **coraux d'eau froide**, pêche de fond (flétan, requin du Groenland).
* **Dangers** : avalanches et chutes de pierres au dégel, eau profonde et glacée, vent qui tourne,
  vêlage des glaciers dans les fjords arctiques (vague et glaçons dérivants).
* **Isolement** : au fond d'un fjord, la seule route est la mer ou un col ; c'est le lieu rêvé
  d'une base imprenable et le plus dur à ravitailler.

### 5.3 Commerce

Les **comptoirs** (handelsbod) des hameaux de pêche et des villages échangent contre des biens
d'exportation : fer en barres estampillé (stångjärn), goudron, stockfisch, fourrures, cuivre,
beurre, verre. Ils fournissent ce que le Nord ne produit pas : **café**, sucre, sel fin, épices
(cannelle, cardamome, safran), tabac, pommes de terre de semence, agrumes. Les prix varient selon
la saison et la distance. Une cargaison par knarr vers un comptoir lointain rapporte davantage.

## 6. Subsistance et industrie

### 6.1 Champs

Cultures : seigle (d'hiver), orge (seule céréale du nord), avoine, navet, chou, pois, lin, chanvre,
houblon, oignon, pomme de terre (par le commerce), pommier et cerisier (sud). Fertilité par
fumier et goémon. **Brûlis (svedjebruk)** : abattre, laisser sécher, brûler une parcelle de
forêt donne un champ de cendre au rendement triple pendant deux ans, puis épuisé (il devient un
Brännfält qui reboise). Clôture de perches (gärdsgård) contre élans et chevreuils.

### 6.2 Foin, bétail, estive

* **Fenaison (slåtter)** en juli : faux → herbe coupée → séchoirs à perches (**hässja**) pendant
  3 jours sans pluie (la pluie fait reculer d'un jour) → foin → grange. Chaque bête consomme une
  ration par jour d'hiver : le troupeau qu'on peut garder se compte en foin engrangé.
* **Estive (fäbod)** : mener vaches et chèvres dans une fäbod (boulaie de montagne) de juni à
  august double le lait ; on y fabrique beurre, fromage et **messmör**.
* **Kulning** : appel chanté aigu (objet « voix », appris à une fäbod habitée). Rappelle le bétail
  dans un rayon de 96 blocs et tient les loups à distance une minute. La corne de vache et la
  trompe d'écorce (näverlur) en sont les versions instrumentales.
* **Renniculture** : apprise auprès des Sames (doc 05, chapitre IV) : marquage, traîneau, lait,
  et le **joik** personnel qui appelle sa harde.

### 6.3 Fraîcheur et conservation

Nouveau système : chaque pile de nourriture porte une date limite (jour du calendrier). Passée la
date, elle devient « avariée » (compost ou appât). Multiplicateurs de durée :

| Lieu ou procédé | Effet |
| --- | --- |
| Inventaire, coffre | × 1 |
| Cellier enterré (jordkällare) | × 3 |
| Cellier à glace (avec blocs de glace) | × 6 |
| Dehors par gel, grenier sur poteau (njalla, härbre) | congelé : ne vieillit pas, à l'abri des voleurs |
| **Séchage** (claies, fil) | viande, poisson, champignons, pois : × 20, ou infini pour le stockfisch |
| **Fumage** (fumoir, sauna à fumée) | × 10, bonus de goût |
| **Salage**, **gravning** (sel, sucre, aneth, sous presse) | × 8 |
| **Fermentation** (surströmming, filmjölk, choucroute, hákarl) | × 15, effets spéciaux |
| **Confiture d'airelles** | infini (elle se conserve seule) |
| **Pain croquant (knäckebröd)** enfilé sur une perche au plafond | infini |
| Fromage, beurre salé, bière, hydromel, eau-de-vie | longue durée |

Le sel vient de l'évaporation de l'eau de mer dans des poêles de fer (plein rendement sur le
Västerhav, un quart sur le golfe saumâtre) ou du commerce.

### 6.4 Cuisine

Les plats donnent faim, saturation et un **effet**. La variété compte : un même plat mangé trois
fois de suite perd son effet (lagom).

| Plat | Effet principal |
| --- | --- |
| Bouillie (gröt), bouillie de seigle | base ; avec du beurre : offrande au tomte |
| Knäckebröd, tunnbröd, pain de seigle | provisions de voyage |
| Soupe de pois et crêpes (ärtsoppa, pannkakor) | repas du jeudi : bonus doublé ce jour-là |
| Boulettes aux airelles (köttbullar) | grande saturation, Mys |
| Hareng mariné, gravlax, saumon fumé | endurance |
| **Surströmming** | très nourrissant ; **odeur** 5 minutes : presque tous les animaux s'écartent, les compagnons en LAN perdent du Mys en intérieur |
| Palt, kroppkakor | « coma de palt » : chaleur maximale mais lenteur |
| Émincé de renne (renskav), suovas | chaleur, apprise des Sames |
| Soupe de cynorrhodon, de myrtilles | contre le froid, fournie aux skieurs |
| Écrevisses à l'aneth | fête d'août |
| Fromage brun, messmör, filmjölk, skyr | lait transformé |
| Pain noir cuit dans le sol chaud (rúgbrauð), hákarl | spécialités du Lavafält |
| Smörgåsbord (table de 7 plats et plus) | banquet : grand bonus de groupe durable |
| Brioches à la cannelle, à la cardamome, semla, lussekatt, sept sortes de biscuits, tarte princesse | fika (§8.2) et fêtes |
| Hydromel, bière, svagdricka, glögg, julmust, snaps | chaleur brève, Mys, puis contrecoup si excès |
| Café, ersatz (chicorée, seigle grillé, racine de pissenlit) | fika ; éveil |

### 6.5 Eau motrice et moulins

L'eau du moteur est statique : le **courant** est une propriété calculée. Toute eau de biome Älv
porte une direction (gradient de W, doc 02 §6.1) et une **chute disponible** si une marche de W
est à moins de 6 blocs.

* **Roue à eau (vattenhjul)** : puissance 1 dans un bief, 3 sous une chute (roue en dessus),
  5 sous une cascade de fjord. Gelée en hiver sauf sur un fors ou si l'on entretient le canal.
* **Transmission** : arbres (axel) contigus sur 16 blocs ; **tringlerie (stånggång)** sur 64
  blocs, alternative au manège à cheval (hästvandring, puissance 1) et au moulin à vent (puissance
  selon le vent).
* **Machines** : meule (grain → farine), scie à cadre (1 grume → 6 planches au lieu de 4),
  bocard (concasse le minerai : + 50 % de rendement), soufflets (haut fourneau), martinet
  (forge d'affinage), pompe (mine), foulon (drap de laine).

### 6.6 Le fer, de la tourbière à l'acier

Il n'y a pas de charbon minéral : tout repose sur la forêt.

**Combustible.**
* **Meule de charbonnier (kolmila)** : empiler 24 à 64 bûches debout, couvrir de mottes de gazon
  et de poussier, allumer par la cheminée. Elle couve 2 jours. Risque : une flamme perce la
  couverture (événement) ; il faut la reboucher dans la minute ou la meule brûle. Le Skogsrå
  réveille le charbonnier respectueux. Rendement : 1 charbon pour 2 bûches.
* **Four à goudron (tjärdal)** : souches de pin résineuses → **goudron** (étanchéité des coques,
  protection du bois, fart, répulsif, remède) et poix.

**Niveau 1, fer des marais et des lacs.** Sonder la tourbière à la pique (le minerai crisse), ou
racler le fond d'un lac à l'épuisette à minerai depuis une barque, ou par un trou dans la glace en
hiver (la bonne saison). Griller le minerai sur un bûcher, puis **bas fourneau (blästugn)** : cuve
d'argile et de pierre, soufflet à main, charbon de bois → **loupe de fer** spongieuse → cinglage
au marteau sur une pierre → **fer forgé**. Petites quantités, qualité inégale.

**Niveau 2, la mine.** Les roches dures (granit, gneiss, grönsten, minerai en roche) résistent aux
pics : dureté × 8. On les attaque par **abattage au feu (tillmakning)** : dresser un bûcher contre
le front de taille, le laisser brûler ; la roche chauffée devient « roche éclatée » sur 1 à 2 blocs
de profondeur, qui s'abat vite. La fumée rend la galerie irrespirable tant que le feu brûle (il
faut un puits d'aérage ou attendre). Boisage, échelles, treuil à manège pour remonter le minerai.
Option de difficulté « mines inondables » : sous le niveau W local, l'eau sourd du sol tant qu'une
pompe mue par la tringlerie ne tourne pas.

**Niveau 3, la forge (bruk).** **Haut fourneau (masugn)** : structure multibloc de pierre, à
construire au bord d'une chute, soufflets mus par la roue ; charge : minerai grillé et concassé,
charbon de bois, castine (calcaire). Il fonctionne en **campagne continue** : tant qu'il est
alimenté il coule de la **fonte** ; le laisser s'éteindre le fige (à démolir en partie). Puis
**forge d'affinage** avec martinet : fonte → **fer en barres (stångjärn)**, estampillé à la marque
du joueur, bien d'exportation. Enfin **acier de cémentation (brännstål)** : barres et poussier de
charbon dans un coffre de grès scellé, 3 jours au four.

**Autres métaux.** Cuivre de l'amas de Bergslag (chaudrons, alambics, toitures, et le résidu
grillé donne le **rouge de Falun**) ; argent (monnaie, objets contre le folklore : une balle ou
une lame d'argent blesse ce que le fer n'atteint pas) ; cobalt (verre et émail bleus) ; or
(orpaillage, incrustation des runes) ; **tungstène** (ferrotungstène au four à vent, fin de jeu).

| Palier d'outils | Matière | Accès |
| --- | --- | --- |
| 0 | silex, os, bois de renne ou d'élan | immédiat |
| 1 | cuivre martelé | tendre, outils médiocres mais chaudrons et alambics |
| 2 | fer forgé (de loupe) | chapitre I à II |
| 3 | acier de cémentation | chapitre II |
| 4 | acier au tungstène (tungstål) | chapitre V ; porte 3 runes |

### 6.7 Artisanats

Travail de l'écorce de bouleau (boîtes, chaussures, toiture, allume-feu), tannage, filage et
tissage (laine → vadmal, lin → toile), teinture végétale, poterie, taille de la stéatite (marmites
qui gardent la chaleur, poêles), verrerie (sable de Sjöland, potasse de cendre : vitres, bouteilles,
flotteurs), menuiserie peinte (kurbits), sculpture du **cheval de Dalécarlie**, fonderie de cloches,
bougies de suif et de cire.

## 7. Savoirs : Naturfrid, runes, orientation

### 7.1 Naturfrid (paix de la nature)

Réputation de −100 à +100 du joueur (ou du groupe LAN) auprès du monde vivant, incarnée par le
Skogsrå, les vittror, le tomte et les esprits des eaux. Elle traduit le **droit de passage
(allemansrätten)** : on peut cueillir, camper, traverser partout, à condition de ne pas abîmer.

| Élève | Abaisse |
| --- | --- |
| replanter après coupe ; ne tuer que ce qu'on consomme ; offrandes (bouillie au tomte, lait aux vittror, pièce à la source) ; éteindre ses feux ; nourrir les oiseaux en hiver ; entretenir cairns et refuges (remettre du bois) | coupe rase ; massacre ; tuer une mère suitée, une baleine, un cygne ; feu de forêt ; piller une sépulture ou un lieu sacré ; bâtir sur un chemin de vittror ; souiller une source |

Effets : densité du gibier et des baies, comportement du folklore (aide, neutralité, tours,
hostilité), fiabilité de l'orientation en forêt, prix chez les habitants.

### 7.2 Runes

Les 24 runes du futhark ancien s'apprennent une à une sur les **pierres runiques** du monde (une
rune par pierre, réparties par région). On les **grave** au ciseau sur un outil, une arme, un
linteau, une proue ou une pierre dressée, puis on les **rougit** au rouge de Falun. Un objet porte
1 rune (acier : 2, tungstål : 3). Une rune s'épuise à l'usage et se **recharge sous l'aurore
boréale** ou au solstice.

| Rune | Sens | Effet en jeu (exemples) |
| --- | --- | --- |
| ᚠ Fehu | bétail | le bétail de l'enclos marqué produit davantage |
| ᚢ Uruz | aurochs | force : charge portée accrue |
| ᚦ Thurs | géant | dégâts contre trolls et géants |
| ᚨ Ansuz | parole | meilleurs prix, dialogue avec le corbeau |
| ᚱ Raido | voyage | vitesse sur piste et route |
| ᚲ Kenaz | torche | une torche gravée ne s'éteint ni au vent ni à la pluie |
| ᚷ Gebo | don | offrandes deux fois plus efficaces |
| ᚹ Wunjo | joie | Mys du foyer marqué |
| ᚺ Hagal | grêle | protège un champ de la grêle et du gel tardif |
| ᚾ Naud | besoin | la faim baisse plus lentement |
| ᛁ Isa | glace | marcher sans casser la glace neuve |
| ᛃ Jera | année, récolte | rendement des moissons |
| ᛇ Eihwaz | if | arc : portée |
| ᛈ Perthro | sort | chance à la pêche et à l'orpaillage |
| ᛉ Algiz | élan, protection | seuil gravé : les créatures hostiles n'entrent pas |
| ᛊ Sowilo | soleil | chaleur ; pierre de soleil toujours lisible |
| ᛏ Tiwaz | Tyr | arme : précision |
| ᛒ Berkana | bouleau | les pousses plantées grandissent vite |
| ᛖ Ehwaz | cheval | monture : endurance |
| ᛗ Mannaz | humain | bonus de groupe en LAN |
| ᛚ Laguz | eau | bateau : stabilité ; nage en eau froide |
| ᛜ Ingwaz | fertilité | naissances dans le troupeau |
| ᛞ Dagaz | jour | Mys pendant la nuit polaire |
| ᛟ Othala | domaine | délimite le « hem » : confort et protection de la maison |

### 7.3 Orientation

En survie, pas de coordonnées à l'écran. On s'oriente par : le soleil et l'**étoile polaire** ;
la **boussole** à aiguille de magnétite (affolée près d'un Malmberg et de ses blocs) ; la **pierre
de soleil** (spath d'Islande, du Lavafält), qui donne la direction du soleil par temps couvert et
au crépuscule ; les fourmilières et la mousse ; les cairns, phares et feux d'alarme ; le corbeau
guide ; la **carte** dessinée à la table d'arpenteur, qui ne se remplit que de ce qu'on a vu. Un
Naturfrid bas brouille tout cela en forêt profonde.

### 7.4 Plantes et remèdes

Sphaigne (pansement), reine-des-prés (douleur), angélique (fortifiant), goudron de bouleau
(répulsif), tisane d'épilobe, soupe de cynorrhodon (contre le « mal d'hiver » : carence si l'on ne
mange ni baies ni légumes pendant un mois), amanite préparée (une minute de **seconde vue** : les
êtres invisibles et les chemins de vittror apparaissent ; contrecoup).

## 8. Mys : fika, sauna, foyer, musique et fêtes

### 8.1 La jauge de Mys

Quatrième jauge du joueur, de 0 à 100, à côté de la faim, de la chaleur et de l'endurance.

| Source | Perte |
| --- | --- |
| fika ; sauna ; repas varié et chaud ; feu de bois, bougies ; maison confortable ; musique ; compagnie (joueurs, animaux familiers) ; aurore, beau temps, premier chant du coucou ; fêtes | obscurité prolongée sans lumière chaude (nuit polaire) ; solitude longue ; froid, humidité, faim ; moustiques ; nourriture monotone ; nuit dehors sans feu ; Naturfrid bas ; mort d'un animal familier |

| Niveau | État | Effets |
| --- | --- | --- |
| ≥ 75 | **Mysigt** | endurance récupérée + 25 %, craft plus rapide, sommeil réparateur, runes rechargées plus vite |
| 25 à 75 | neutre | |
| < 25 | **Tungsint** (cafard) | lenteur, faim accélérée, couleurs ternies |
| < 10 | **Vintermörker** (ténèbres d'hiver) | sommeil sans repos (la Mara), hallucinations sonores |

**Lagom** : quand faim, chaleur, endurance et Mys sont toutes entre 40 % et 90 %, un bonus
d'équilibre de + 10 % s'applique à toutes les actions. Se gaver ou s'épuiser le fait perdre.

**Confort du foyer (hemtrevnad)** : pour la pièce close où se tient le joueur (remplissage borné
à 2000 blocs), on additionne : foyer ou poêle de faïence, lit et sa qualité, textiles (tapis,
tentures, peaux), fenêtres vitrées, bougies, murs peints, fleurs, horloge de Mora, étagère de
livres, cheval de Dalécarlie, animal familier, propreté (pas d'établi de boucher). Le score fixe
la vitesse de gain et le plafond de Mys en intérieur.

### 8.2 Fika

Pause-café partagée, institution suédoise. **Rituel** : une table de fika, un café (ou ersatz) et
une douceur au moins par convive ; s'asseoir 20 secondes sans agir.

* Effet « **Fikapaus** » pendant une demi-journée : endurance maximale + 20 %, Mys + 25, craft
  plus rapide.
* **À plusieurs** (LAN) : l'effet est multiplié par le nombre de convives (plafond 4) et
  s'accompagne d'un bonus de coopération (portée de partage des effets de groupe).
* Qualité : ersatz < café ; une douceur < les **sept sortes de biscuits** (table complète :
  effet d'une journée entière).
* Lagom : deux fika par jour au plus (matin et après-midi) ; la troisième ne donne rien.
* Le café vient du commerce (§5.3) : la fika de qualité récompense la mise en place d'une
  exportation.

### 8.3 Sauna (bastu)

**Construction** : pièce close en bois d'au plus 5 × 5 × 3, un **poêle à pierres** (bastuugn), des
bancs, un seau et une louche. Variante primitive : **sauna à fumée** (rökbastu) sans cheminée,
plus longue à chauffer, qui sert aussi de fumoir et de séchoir à malt.

**Séance** :
1. Chauffer le poêle (bois) : la pièce monte en température en une demi-minute.
2. Entrer sans armure. Jeter de l'eau sur les pierres : **löyly** (vapeur), pic de chaleur.
   Se fouetter au **bouquet de bouleau** (björkris, à cueillir en juin) accélère l'effet.
3. Après 30 secondes : effet « **Renad** » (purifié) : retire froid, humidité, démangeaisons,
   venin léger, fatigue ; régénération de santé pendant une journée.
4. Rester trop longtemps : surchauffe (vertige, dégâts).
5. **Bain froid** dans la minute (lac, trou dans la glace, roulade dans la neige) : effet
   « **Stålbad** » (bain d'acier) : résistance au froid et endurance pendant une journée entière.
   C'est la préparation normale d'une expédition d'hiver.

À plusieurs, le sauna donne aussi du Mys de compagnie. Un esprit du sauna (bastutomte) exige qu'on
n'y jure pas et qu'on lui laisse la dernière chauffe.

### 8.4 Musique

Le moteur audio est procédural (synthèse) : chaque instrument est un timbre et chaque air un motif.
Instruments : violon (fiol), nyckelharpa, guimbarde, flûte de saule (au printemps seulement),
trompe d'écorce, corne de vache, psaltérion à archet. Jouer crée une **aura de Mys** pour le
groupe. Airs à effet : marche (gånglåt : vitesse de marche du groupe), appel du bétail (vallåt),
polska du Näcken (les poissons montent ; apprise du Näcken), berceuse (sommeil réparateur), psaume
(éloigne les revenants). Ambiances : modes populaires, bourdons, rythme de polska à trois temps.

### 8.5 Fêtes du calendrier

| Date | Fête | Rituel | Effet |
| --- | --- | --- | --- |
| 13 januari | Knut | sortir le sapin | clôt Noël ; Mys |
| februari, mardi gras | Fettisdagen | manger une semla | endurance |
| fin mars | Våffeldagen, retour des grues | gaufres | marque le printemps |
| avril | Pâques (påsk) | feux de Pâques, œufs peints | éloigne les sorcières en route pour Blåkulla |
| 30 avril | **Valborg** | grand bûcher collectif, chant | met fin aux ténèbres d'hiver : remise à niveau du Mys, chasse les esprits de l'hiver |
| solstice d'été | **Midsommar** | dresser et fleurir le mât, danser, hareng et fraises ; cueillir **sept fleurs** en silence et dormir dessus | nuit magique : folklore actif mais bienveillant, les elfes dansent ; le rêve des sept fleurs **révèle un lieu** sur la carte (pierre runique, gisement ou étape de saga) |
| août | **Kräftskiva** | écrevisses, lanternes, chapeaux | banquet d'écrevisses |
| fin août | Première du surströmming | ouvrir la boîte dehors | |
| 4 oktober | Jour de la brioche à la cannelle | fika | fika doublée |
| 11 november | Mårten gås | oie rôtie | saturation longue |
| **13 december** | **Lucia** | couronne de bougies, lussekatter au safran, procession ; veiller toute la nuit, lumières allumées | repousse Lussi et sa chasse (doc 03) ; grand gain de Mys au cœur de la nuit |
| solstice d'hiver | **Jul** | bouillie au beurre pour le tomte, bouc de paille (julbock), sapin, jambon | le tomte reste un an de plus ; régénération ; si l'on oublie le beurre, le tomte se fâche |

## 9. Blocs

Les identifiants sont attribués par la palette du monde (doc 01 §5). Noms techniques en suédois
sans diacritiques.

| Famille | Blocs | Nombre |
| --- | --- | --- |
| Socle commun | air, eau, lave, bedrock | 4 |
| Roches | granit, rodgranit, gnejs, kalksten, krita, sandsten, alunskiffer, gronsten, porfyr, diabas, basalt, basaltpelare, djupberg, taljsten, kvarts, sprucket_berg (roche éclatée), trollsten, sinter (geysérite), svavel | 19 |
| Sols | moran, grus, sand, svartsand, klappersten, blalera, jord, gras, mossa, barrmatta, lovforna, lavhed, torv, vitmossa, aska, rodmull, slagg, akerjord (labour), stig (sentier), permafrost | 20 |
| Neige et glace | snotacke (4 hauteurs), sno, packad_sno, nyis, is, havsis, blais | 10 |
| Minerais | myrmalm, sjomalm, jarnmalm, rik_jarnmalm, kopparmalm, silvermalm, koboltmalm, guldkvarts, guldgrus, volframmalm, flinta | 11 |
| Troncs | gran, tall, bjork, bok, ek, asp, al, ronn, dod_stam, mossig_stam | 10 |
| Feuillages | gran, tall, bjork, bok, ek, asp, al, ronn, salg, en, hassel | 11 |
| Plantes | blabar, lingon, hjortron, smultron, tranbar, nypon, havtorn, ljung, renlav, skagglav, kantarell, karljohan, flugsvamp, vitsippa, blasippa, midsommarblomster, prastkrage, blaklocka, rodklover, rallarros, angsull, kvanne, orkide, fjallsippa, isranunkel, vass, kaveldun, nackros, blastang, sockertang, hogt_gras, ormbunke, ramslok, glasort, dvargbjork, lupin, mjodort | 37 |
| Cultures | pousse commune, puis 2 stades pour : rag, korn, havre, rova, kal, artor, lin, hampa, humle, potatis, lok | 23 |
| Construction bois | planches (furu, bjork, ek, bok), knuttimmer, panneau rouge, jaune, blanc, planche goudronnée, bardeau, toit de tourbe, chaume, écorce, clôture de perches, portillon, échelle, trappe | 17 |
| Construction pierre et divers | pierre des champs maçonnée, mur de pierres sèches, granit taillé, calcaire taillé, grès taillé, brique, brique de laitier, stéatite taillée, pavé, enduit à la chaux, cuivre, cuivre vert-de-grisé, verre, verre vert, verre bleu de cobalt | 15 |
| Textiles et foyer | foin, tapis de laine, tapis rya, peau, lit de paille, lit à édredon, banc, table de fika, coffre, tonneau | 10 |
| Feu et lumière | feu de camp, âtre, poêle de faïence, poêle de sauna, four à pain, bougie, lanterne, torche de goudron, feu d'alarme (éteint, allumé) | 10 |
| Métallurgie | bas fourneau, haut fourneau, foyer d'affinage, enclume, martinet, soufflet, bocard, meule de charbonnier (montée, allumée), four à goudron, poêle à sel | 11 |
| Mécanique | roue à eau, arbre, tringlerie, meule de moulin, scie à cadre, pompe, moulin à vent, manège | 8 |
| Subsistance | établi, fumoir, claie de séchage, séchoir à foin, baratte et presse, cuve de brassage, métier à tisser, cuve de tannage, étagère de cellier, bloc de glace, grenier sur poteau, ruche, abri à eider, nasse, collet, filet, trou de pêche | 17 |
| Culture et savoir | pierre runique (vierge, gravée), pierre d'offrande, cloche, mât de la Saint-Jean, bouc de paille, cheval de Dalécarlie, horloge de Mora, table d'arpenteur, cairn | 10 |
| **Total** | | **243** |

**Budget.** 243 sur 256 : la palette par monde suffit, mais sans marge. Deux lots se détachent
naturellement en seconde version si l'on veut de l'air : mécanique lourde (8) et une partie des
plantes décoratives (10). Trois leviers si le besoin grandit : teintes et états rendus par le
mailleur (déjà prévu pour les feuillages), octet d'état par bloc, passage à u16 (doc 01 §5.2,
décision D2 du plan).

## 10. Objets

Environ 260 objets purs, tous déclarés en JSON (même mécanisme qu'`items_survival.json`).

| Famille | Objets |
| --- | --- |
| Matières brutes | silex, os, bois de renne, bois d'élan, tendon, peau (8 espèces, été et hiver), fourrures, duvet d'eider, plumes, laine, lin, chanvre, écorce de bouleau, résine, sève de bouleau, cire, miel, suif, graisse de phoque, castoréum, ivoire de morse, ambre, spath d'Islande, sel, potasse, argile, tourbe séchée |
| Métallurgie | minerai grillé, loupe, fer forgé, fonte, fer en barres estampillé, acier, cuivre, argent, or (paillettes, lingot), cobalt, ferrotungstène, tungstål, laitier, charbon de bois, poussier, goudron, poix, rouge de Falun, huile de lin, castine |
| Outils (par palier) | pic, hache, pelle, houe, couteau, faux, faucille, scie, ciseau à runes, marteau, tarière à glace, scie à glace, pique à minerai, épuisette à minerai, batée, briquet à silex, soufflet à main, seau, louche |
| Chasse et pêche | arc, flèches, épieu, harpon, collet, nasse, filet, senne, canne, canne à glace, ligne dormante, ligne de fond, hameçons, appâts |
| Vêtements | par emplacement et matière : écorce, peau, vadmal, fourrure, glouton, duvet, toile goudronnée, cuir de phoque ; bonnet, moufles lovikka, chaussures à bec, costume de fête, couronne de Lucia, couronne de fleurs |
| Déplacement | skis, bâton, raquettes, patins, poinçons à glace, corde, luge, pulka, ackja, traîneau, radeau, eka, snipa, knarr (kits), char à glace, selle, bât, mors d'acier |
| Orientation | boussole, pierre de soleil, carte, longue-vue, baromètre (tardif) |
| Graines et récoltes | grains et semences des 11 cultures, farines, malt, houblon, pommes, cerises, baies (8), champignons (4), œufs, lait, herbes |
| Nourriture préparée | les plats du §6.4, les conserves du §6.3 (séché, fumé, salé, gravad, fermenté), fromages, beurre, pains, pâtisseries, boissons |
| Commerce | café, sucre, cannelle, cardamome, safran, tabac, agrumes, sel fin, pièces d'argent |
| Musique | violon, nyckelharpa, guimbarde, flûte de saule, trompe d'écorce, corne de vache |
| Rituels | bouquet de bouleau, bol de bouillie, couronne des sept fleurs, pièce d'offrande, bougie de Lucia, balle d'argent, remèdes, amanite préparée |
| Saga | Sagabok, tisons des neuf feux, fragments de runes, trophées d'épreuve (doc 05) |

## 11. Stations et chaînes de fabrication

Le craft par stations existe (main, établi, forge). Norrcraft ajoute des stations **à durée** (le
résultat demande du temps de jeu) et **à condition** (météo, saison, puissance).

| Chaîne | Étapes |
| --- | --- |
| Charbon | bûches → meule (2 jours, surveillance) → charbon de bois |
| Goudron | souches de pin → four à goudron (1 jour) → goudron → poix |
| Fer niveau 1 | myrmalm ou sjömalm → grillage → bas fourneau + soufflet + charbon → loupe → cinglage → fer forgé |
| Fer niveau 3 | minerai en roche → abattage au feu → bocard → grillage → haut fourneau (roue, castine, charbon) → fonte → affinage au martinet → fer en barres → cémentation → acier |
| Rouge de Falun | résidu de cuivre grillé + huile de lin + farine de seigle + eau → peinture → panneaux peints |
| Pain | seigle → meule → farine → pétrin → four à pain → pain, knäckebröd |
| Lait | lait → baratte → beurre et babeurre ; lait → présure → presse → fromage ; petit-lait réduit → messmör |
| Laine | tonte → cardage et filage → métier → vadmal → foulon → vêtements |
| Poisson | prise → séchage (météo), fumage, salage, gravning ou fermentation en tonneau (surströmming : 2 mois) |
| Bière et hydromel | orge → maltage (sauna à fumée) → cuve + houblon → bière ; miel + eau → hydromel ; alambic de cuivre → eau-de-vie |
| Verre | sable + potasse → four → verre ; + cobalt → bleu |
| Bateau | planches + rivets de fer + goudron + toile + cordage → chantier naval |
| Fika | café vert → torréfaction à la poêle → moulin → cafetière de cuivre ; farine, beurre, sucre, épices → four |

## 12. Critères d'acceptation

* Chaque mécanique citée par Gabriel est jouable de bout en bout : **fika** (table, café,
  douceur, effet de groupe), **sauna** (chauffe, vapeur, bain froid, deux effets), **mers gelées**
  (gel, glace qui cède, pêche, routes d'hiver, débâcle), **fjords** (voile, vent canalisé, morue
  séchée, cascade motrice), **mines de fer** (trois niveaux, abattage au feu, haut fourneau).
* Un hiver complet se traverse en survie avec les seules ressources d'un biome de départ.
* Toutes les tables (plats, conservation, runes, vêtements, fêtes) sont en JSON, modifiables sans
  recompiler, comme le veut la convention du projet.
* Le budget de blocs reste sous 256 ou la décision D2 est prise.
