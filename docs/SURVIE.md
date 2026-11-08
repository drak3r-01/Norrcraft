# Mode survie nordique

Ajouté après le moteur de particules (branche `claude/survival`). Le jeu garde son **mode créatif**
tel quel (casser un bloc est instantané, tout est plein, vol par double saut) ; en **survie**, un
monde froid et affamé demande de se vêtir de fourrure, de chasser, pêcher, cultiver et miner avec
de vrais outils. Rien n'est actif sans `SurvivalData` dans le contexte du tick : les tests de
conformité du Java (bogue B19 : casser sans dureté) tournent donc toujours dans leur mode d'origine.

Toutes les règles sont **pilotées par des fichiers** générés par `scripts/gen_survival.py`
(Python pur, sans dépendance) :

| Fichier | Contenu |
|---|---|
| `assets/items_survival.json` | 82 objets purs (ids ≥ 300) |
| `assets/blocks_survival.json` | 11 blocs de culture (ids 210–220) |
| `assets/survival.json` | propriétés des objets, dureté / outil / butin de **chaque bloc**, butin des 45 mobs, croissance des cultures, températures des biomes, table de pêche, sources de chaleur |
| `assets/recipes_survival.json` | 70 recettes (en plus des 46 de `recipes.json`) |
| `resource_packs/Default/.../item_*.png` | textures **procédurales** provisoires (placeholders) |

## Commandes de jeu

| Geste | Effet |
|---|---|
| **Clic gauche maintenu** sur un bloc | Mine : la durée dépend de la dureté du bloc et de l'outil |
| **Clic gauche** sur un mob | Coup avec l'arme tenue (délai 0,4 s, usure de l'arme) |
| **Clic droit maintenu** avec une nourriture | Manger (1,4 s) |
| **Clic droit** avec une pièce d'armure | L'équiper (échange avec la pièce portée) |
| **Clic droit** avec une houe sur terre / herbe / chemin | Labourer |
| **Clic droit** avec graines, pomme de terre, carotte sur la terre labourée | Planter |
| **Clic droit** avec la canne à pêche vers l'eau | Lancer ; au signal « BITE », **clic droit** pour ferrer |
| **E** → bouton livre (en haut à droite du panneau) | Livre de recettes |
| **F5** | Vue à la 3ᵉ personne : le joueur est dessiné avec son avatar animé |

## Statistiques du joueur

Barres au-dessus de la barre rapide : cœurs (santé), **boucliers** (défense), **faim** (20 points,
icônes de pilon), **chaleur** et **endurance** (barres colorées). Les états sont sauvegardés dans
`world.json` (`survival`) ; un ancien monde se charge avec des valeurs pleines.

* **Faim** : baisse lentement (≈ 1 point par minute et demie), plus vite en courant ou quand on a
  froid. La **saturation** (réserve) est consommée d'abord. Faim ≥ 15 : régénération de la santé.
  Faim à 0 : on ralentit et on perd de la santé, **sans jamais mourir de faim** (plancher à 1).
  Courir exige plus de 6 points de faim.
* **Chaleur** (0–100) : suit la *température ressentie* = biome (de −1,0 sur les sommets
  enneigés à +0,8 près d'un volcan) + heure (la nuit refroidit) + altitude + humidité (dans l'eau :
  −0,45) + abri (un toit) + **sources de chaleur** (feu de camp, forge, lave, torche, lanterne,
  dans 5 blocs) + isolation des vêtements. Sous 30 : ralentissement, givre sur les bords de l'écran,
  faim accélérée. À 0 : dégâts de gel (qui peuvent tuer). Manger chaud (ragoût, bouillie, hydromel)
  réchauffe aussitôt.
* **Endurance** : le sprint la vide (14 / s), un saut coûte 6, elle revient à 20 / s au repos. À 0
  on est **épuisé** : plus de sprint avant 30 points récupérés.
* **Armure** : chaque point de défense réduit les dégâts de mobs de 3 % (80 % au maximum) ; chaque
  coup reçu use une pièce au hasard. Les 4 pièces s'équipent aussi dans l'inventaire (emplacements
  à gauche du personnage, ou **Maj + clic**).

## Minage, outils, paliers

5 paliers : **bois, pierre, fer, diamant, aurore** × 5 outils (pioche, hache, pelle, houe, épée) =
25 outils, plus la canne à pêche. Durée d'un bloc = `dureté × 1,5 / vitesse` (outil de la bonne
famille ; ×5 si le bloc **refuse de livrer** son butin avec l'outil tenu).

| Palier | Vitesse | Durabilité | Dégâts d'épée |
|---|---|---|---|
| Bois | ×2 | 59 | 3 |
| Pierre | ×3,5 | 131 | 4 |
| Fer | ×5,5 | 250 | 5 |
| Diamant | ×8 | 1561 | 6 |
| Aurore | ×12 | 2031 | 8 |

La pierre et les minerais exigent une **pioche** : charbon à tout palier, **fer = pierre**,
**or / diamant / émeraude = fer**, **obsidienne = diamant**. Le fer brut (`raw_iron`, `raw_gold`) se
fond à la forge. La roche mère est incassable. Les outils et armures **ne s'empilent pas** et
portent leur **usure** (barre sous l'icône, durabilité dans l'infobulle).

Butins notables : la pierre donne du pavé, l'herbe de la terre, le gravier un peu de **silex**, les
feuilles des bâtons et des pommes, le feuillage dense et l'herbe haute des **graines**, les buissons
des **airelles / myrtilles**, la glace compacte et l'obsidienne parfois un **éclat d'aurore**,
l'épicéa de l'**ambre**.

## Chasse

Les 45 mobs ont une table de butin (`mobs` dans `survival.json`) : viande crue, peau, **fourrure**
(loup, ours, renard, lièvre…), os, plumes, bois de cervidé, défense, suif. Le butin va au joueur
qui a porté le dernier coup. La viande se cuit à la forge (viande, volaille, poisson) ; le poisson
cuit se fume.

## Pêche

Canne (`string` + `stick`), clic droit vers l'eau (portée 12 blocs). Après 3 à 10 s un poisson
mord (éclaboussure + message) : on a 1,15 s pour ferrer. Table : poisson (62 %), bâton, ficelle, os,
bol, plumes, ambre et, très rarement, un **éclat d'aurore**.

## Cultures

Houe → terre labourée → **graines de blé** (`wheat_seeds`), **pomme de terre**, **carotte** (4
stades chacune, ≈ 100 s par stade, dans un rayon de 24 blocs autour de chaque joueur). Le blé mûr
donne du grain (→ farine → pain) et des graines ; les récoltes immatures ne rendent presque rien.
Les cultures ne poussent que sur la terre labourée.

## Nourriture

Pomme, airelle, myrtille, carotte, navet, pomme de terre / cuite, pain de blé, **pain de seigle**,
viande / volaille / poisson (crus, cuits), **poisson fumé**, **bouillie**, **ragoût de poisson**,
**ragoût de champignons**, **tarte aux baies**, **hydromel**, **jus d'airelle**. Chaque aliment
rend de la faim, de la saturation, de la chaleur et parfois de la santé.

## Livre de recettes

Dans l'inventaire (**E**), le bouton en forme de livre ouvre la liste de **tous** les crafts du
jeu (116). Onglets Tout / Réalisables / Main / Établi / Forge, défilement à la molette, détail de la
recette sélectionnée (motif, ingrédients avec le nombre possédé ; `*` = ingrédient interchangeable,
par exemple n'importe quelle planche). « **Fill crafting grid** » prélève les ingrédients dans
l'inventaire et remplit la grille de la station ouverte. Une recette d'établi demande d'ouvrir un
établi (clic droit).

## Réseau local

Le protocole passe à la **version 2** : le coup d'un invité annonce les **dégâts de son arme**
(bornés par l'hôte), son **armure** (réduction de dégâts) accompagne sa position, et la mort d'un
mob est annoncée à l'invité qui l'a tué, qui **tire son butin chez lui** (son inventaire est
local). La croissance des cultures est simulée par l'hôte autour de **tous** les joueurs et
diffusée comme n'importe quelle modification de bloc. Faim, chaleur, endurance et minage sont
propres à chaque joueur. Les deux machines doivent avoir la même compilation (version de
protocole).

## Son et particules

Minage (éclats de matière + son de matériau), outil cassé, équipement, labour, plantation, lancer
et touche de pêche (éclaboussures), repas, butin (bruit de ramassage), dégâts de froid. Les
effets réutilisent `nb-audio` et `nb-particles` (voir `docs/particles.md`).

## Régénérer les données

```sh
python3 scripts/gen_survival.py        # JSON + textures item_*.png / stades de culture
```

Aperçu hors écran du HUD, de l'inventaire, du livre et de la 3ᵉ personne (Docker) :

```sh
NB_E2E_OUT=apercu-survie sh scripts/preview-survival.sh
```

## Limites connues

* Les textures d'objets et de cultures sont des **placeholders procéduraux** (16×16 agrandis ×2).
* Pas d'arc ni de projectiles ; pas de coffres ni de lits.
* Les objets cassés ne tombent pas au sol : ils vont directement dans l'inventaire (reliquat perdu
  si plein, comme `returnHeld` du Java).
* La pluie n'influence pas encore la chaleur (la météo des particules est locale au rendu).
