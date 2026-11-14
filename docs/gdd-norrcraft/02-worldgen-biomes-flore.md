# 02. Worldgen Norrcraft : mathématiques, biomes, flore et structures

Tâche 2 du GDD. Tout ce qui suit est nouveau : aucun biome, arbre, structure ou réglage du
générateur vanilla n'est repris. Seuls les **outils** du moteur sont réutilisés (bruit de Perlin de
`noise.rs`, hachage de grille, constructeur de structures `Build`, contrat du doc 01 §3.2).

## 1. Intention

Le monde doit se lire comme la Fennoscandie et ses voisins : un océan froid à l'ouest qui entaille
une chaîne de montagnes anciennes par des **fjords**, des hauts plateaux pelés (**fjäll**), une
immense forêt boréale criblée de **lacs** et de **tourbières**, un golfe saumâtre à l'est qui
**gèle l'hiver** et se couvre d'un **archipel** de granit poli, un sud doux de hêtres, de prés et de
calcaire, et, loin au large, une province volcanique à l'islandaise. Le relief raconte la dernière
glaciation : vallées en U, eskers, blocs erratiques, roches moutonnées, côtes soulevées.

## 2. Cadre et invariants

* Chunk 16 × 128 × 16 (inchangé). **Niveau de la mer S = 40** (vanilla : 34) pour donner 39 blocs
  de profondeur aux fjords et 80 blocs aux sommets. Passer à 256 de haut est une évolution du
  moteur hors périmètre ; toutes les formules ci-dessous sont paramétrées par S et Hmax.
* **Fonction pure** : (graine, norrConfig, x, z) → colonne. Le générateur, le mailleur, le spawn
  et la survie voient le même biome.
* **Aucune fonction transcendante** (ni exp, ni sin, ni log) dans la génération, comme dans le
  générateur actuel : seules +, −, ×, ÷, racine carrée, plancher et comparaisons, pour un résultat
  identique bit à bit sur toutes les plateformes. Les sigmoïdes sont rationnelles.
* **Indépendance de l'ordre des chunks** : tout décor (arbre, rocher, structure, lac) est une
  fonction d'une cellule de grille hachée ; chaque chunk recalcule les cellules voisines qui
  débordent chez lui.
* **État canonique = fin d'été.** La génération ne dépend pas de la saison ; neige, glace, couleurs
  et baies sont une surcouche appliquée au chargement (doc 04 §2).

## 3. Briques mathématiques

Notations : p = (x, z) en blocs ; N(p) bruit de gradient 2D dans [0, 1] ; N₃ sa version 3D ; chaque
champ a sa propre instance, semée par `graine XOR constante_du_champ`.

| Brique | Définition |
| --- | --- |
| Hachage | `h(graine, i, j, k)` : mélange entier 64 bits (type SplitMix64) → uniforme dans [0, 1) |
| fBm | `F(p; o, ℓ, g) = Σᵢ gⁱ · N(p · ℓⁱ) / Σᵢ gⁱ`, i de 0 à o−1 ; par défaut ℓ = 2, g = 0,5 |
| Crête | `Rg(p; o) = Σᵢ gⁱ · wᵢ · (1 − abs(2·Nᵢ − 1))²`, avec `w₀ = 1`, `wᵢ = clamp(2 · terme précédent, 0, 1)` : crêtes vives, vallées douces |
| Sigmoïde rationnelle | `s(t) = t / (1 + abs(t))` |
| Étalement | `q(n; k) = 0,5 + 0,5 · s(k · (2n − 1)) / s(k)` : étale un fBm concentré autour de 0,5 sur tout [0, 1] (k = 3 par défaut) |
| Seuil lisse | `sm(a, b, v) = u² · (3 − 2u)`, `u = clamp((v − a) / (b − a), 0, 1)` |
| Bosse | `bump(v; c, w) = max(0, 1 − ((v − c) / w)²)²` |
| Déformation de domaine | `warp(p; A, L) = p + A · (F(p / L + o₁) − 0,5 ; F(p / L + o₂) − 0,5) · 2` |
| Worley | grille de pas c, un point jitteré par cellule (par hachage) ; F1, F2 = distances aux deux points les plus proches (racine carrée autorisée), `id` = cellule du plus proche |
| Ligne de zéro | `Z(p) = abs(2 · F(p) − 1)` : vaut 0 sur des courbes continues sinueuses (rivières, vallées, eskers, failles) |
| Plafond doux | pour h > h₀ : `h ← h₀ + (h − h₀) · m / (m + h − h₀)` : asymptote à h₀ + m |

## 4. Champs macroscopiques

Tous sont évalués sur `p' = warp(p; 120, 700)` pour des frontières organiques, puis étalés par q.

| Champ | Sens | Construction | Longueur d'onde |
| --- | --- | --- | --- |
| **C** continentalité | 0 = haute mer, 1 = cœur des terres | `q(F(p'/2600; 5))`, plus biais d'océanité | 2600 |
| **O** océanité | 1 = façade atlantique (douce, humide), 0 = façade baltique (saumâtre, gel) | `q(F(p/5000; 2))` ; préréglage Norrland : `0,6 · (0,5 − 0,5 · s(x/4000)) + 0,4 · bruit` (ouest = océan) | 5000 |
| **T₀** température au niveau de la mer | 0 = arctique, 1 = sud doux | `q(F(p'/6000; 2))` ; préréglage Norrland : `0,65 · (0,5 + 0,5 · s(z/3500)) + 0,35 · bruit` (nord = −z = froid) | 6000 |
| **Hm** humidité | tourbières contre landes sèches | `q(F(p'/1900; 4))` + 0,15 · O | 1900 |
| **U** surrection | où se dresse la chaîne | `sm(0,50, 0,78, q(F(p'/3200; 3)))`, renforcé près de la façade atlantique : `U ← U · (0,6 + 0,6 · O)` | 3200 |
| **E** érosion | 1 = plat, 0 = accidenté | `q(F(p'/900; 4))` | 900 |
| **Lk** densité de lacs | pays des mille lacs | `q(F(p/2200; 2))` | 2200 |
| **Gt** géothermie | province de lave | `sm(0,90, 0,96, q(F(p/5000 + o; 2)))` et C < 0,5 (îles du large) | 5000 |
| **Pv** province rocheuse | nature du socle | Worley de pas 1400 déformé, 1 province par cellule | 1400 |
| **Vr** variante | sous-types locaux | `q(F(p/500; 3))` | 500 |
| **θ** sens d'écoulement des glaces | oriente îles, drumlins, stries | vecteur unitaire constant par cellule Worley de pas 3000, interpolé | 3000 |

Température réelle à l'altitude y : `T = T₀ − 0,006 · max(0, y − S) + 0,05 · (O − 0,5)`.

Le biais nord-sud et ouest-est du préréglage **Norrland** (défaut) donne au monde une géographie
lisible : on part du sud tempéré et l'aventure pousse vers le nord et l'ouest (doc 05). Les biais
saturent au-delà de 8000 blocs : le monde reste infini.

Optimisation : C, O, T₀, Hm, U, Lk, Gt varient lentement ; on les évalue sur un treillis de 4 × 4
blocs et on interpole bilinéairement (erreur inférieure à un demi-bloc sur la hauteur).

## 5. Fonction de hauteur

`h(x, z)` s'obtient par composition, dans cet ordre. Chaque étape est continue, donc h l'est.

### 5.1 Socle continental

Spline linéaire par morceaux de C, lissée par sm entre les nœuds :

| C | 0,00 | 0,30 | 0,42 | 0,47 | 0,55 | 0,75 | 1,00 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| B(C) − S | −28 | −13 | −4 | +1 | +6 | +13 | +20 |

Le trait de côte est l'isoligne C ≈ 0,465. Le plateau continental (0,42 à 0,47) a une largeur
constante en blocs quel que soit le préréglage.

### 5.2 Collines et détail

```
terre  = sm(0,45, 0,52, C)
Hc     = terre · (6 + 10 · (1 − E)) · (2 · F(p/260; 4) − 1)
Dt     = 1,6 · (2 · F(p/48; 3) − 1) + 0,6 · (2 · N(p/11) − 1)
h₁     = B(C) + Hc + Dt
```

### 5.3 Chaîne des Scandes

```
pm   = warp(p; 90, 420)
Mt   = U · terre · (22 + 50 · Rg(pm/1500; 5)) · (0,35 + 0,65 · (1 − E))
h₂   = h₁ + Mt
```

**Plateaux (vidda).** Les montagnes scandinaves sont vieilles et tabulaires. On aplatit par
paliers là où un masque le demande :

```
P    = sm(0,55, 0,70, F(p/700; 2)) · U
ℓ    = S + 44 + 12 · plancher(3 · F(p/1600))        palier de plateau : S+44, S+56 ou S+68
h₃   = h₂ + P · bump(h₂; ℓ, 14) · 0,75 · (ℓ − h₂)   tire h vers ℓ dans une bande de ±14
```

Plafond doux à partir de h₀ = S + 62 avec m = 22 : aucun sommet ne dépasse S + 84 = 124.

### 5.4 Vallées glaciaires et fjords

Les axes de vallée sont les lignes de zéro d'un champ lent. Le profil en travers est un **U**
(auge glaciaire : fond plat, parois raides), à opposer au V des rivières.

```
pv    = warp(p; 160, 520)
V     = Z(pv / 1100)                                 0 sur l'axe
ω     = 0,050 + 0,035 · (1 − C)                      demi-largeur (en unités de bruit) : s'évase vers la mer
t     = min(1, V / ω)
u(t)  = 1 − t⁴                                       profil en U
```

Altitude visée du fond, en **escalier** (voir §6.1) vers l'intérieur et sous la mer vers la côte :

```
intérieur :  f = S + 2 + 6 · plancher(5 · sm(0,56, 0,82, C))            marches de 6 blocs
fjord     :  f = S − 6 − 24 · sm(0,56, 0,47, C) + 15 · bump(C; 0,445, 0,02)
```

Le dernier terme est le **seuil** (verrou glaciaire) : le fjord est profond à l'intérieur (jusqu'à
S − 30) et remonte à S − 15 à son embouchure, comme les vrais.

```
g    = sm(0,25, 0,50, U)                             seulement dans la chaîne
h₄   = h₃ − g · u(t) · max(0, h₃ − f)
```

Un point est étiqueté **Fjord** si `g · u(t) > 0,5`, `f < S` et C < 0,56. Les parois gardent toute
la hauteur de la chaîne : falaises de 40 à 70 blocs tombant dans une eau profonde.

**Vallées suspendues.** Un second champ, `Z(pv₂/450)`, creuse des auges plus étroites dont le fond
est 18 blocs au-dessus de la vallée principale. Là où elles débouchent sur un fjord, le décor
« cascade » (§10) est placé : chute verticale de 15 à 40 blocs.

### 5.5 Archipel (skärgård)

Côte basse du côté baltique : milliers d'îlots de granit poli.

```
K     = (1 − U) · bump(C; 0,43, 0,07) · sm(0,50, 0,65, F(p/1800; 2)) · (1 − 0,7 · O)
îles  : Worley de pas 44 ; pour la cellule i : centre cᵢ, rayon rᵢ = 6 + 20 · hᵢ³, hauteur aᵢ = 1 + 8 · hᵢ'²
        distance anisotrope le long de θ : d² = (d∥ / 1,8)² + d⊥²
        bosse bᵢ = aᵢ · max(0, 1 − d² / rᵢ²)²  ; profil dissymétrique : × (1 + 0,5 · d∥ / rᵢ) côté aval
h₅    = lerp(h₄, S − 7 + maxᵢ(bᵢ) · 2, K)
```

La puissance 3 sur le rayon donne beaucoup de cailloux (kobbar) et peu de grandes îles. La
dissymétrie reproduit les roches moutonnées (douces face à la glace, abruptes derrière).

### 5.6 Côtes

| Type | Condition | Formule ou effet |
| --- | --- | --- |
| Fjord | U fort, façade atlantique | §5.4 |
| Skärgård | K > 0,3 | §5.5 |
| Klint | province calcaire, E < 0,45 | marche franche : `h += 12 · sm(0,468, 0,474, C)` ; raukar au pied (§10) |
| Côte soulevée | province de granit rouge, façade baltique, 0,25 < T₀ < 0,5 | terrasses : `h ← h − 0,7 · (h − 4 · arrondi(h/4))` entre S et S+24, champs de galets |
| Rivage de sable et dunes | défaut, E > 0,5 | dunes : `+3 · Rg(p/40; 2)` sur 30 blocs de large |
| Pré salé (strandäng) | E > 0,75, T₀ > 0,5 | h borné entre S+1 et S+2 sur une large bande |

### 5.7 Reliefs glaciaires de plaine

* **Eskers (åsar)** : `h += 8 · max(0, 1 − (Z(p/1300) / 0,012)²)` en zone boréale basse (C > 0,55,
  U < 0,2). Crêtes de sable et gravier longues de plusieurs centaines de blocs, hautes de 5 à 8 :
  routes naturelles, pinèdes sèches, sites des pistes et des hameaux.
* **Drumlins** : Worley de pas 90, bosses allongées le long de θ (rapport 3:1), +3 à +5.
* **Marmites (dödisgropar)** : creux circulaires de 3 à 5 blocs au pied des eskers, souvent un
  étang.

### 5.8 Tourbières, palses, glaciers, champ de lave

* **Myr** (masque `My = sm(0,62, 0,75, Hm) · sm(0,60, 0,78, E) · terre · (1 − U)`, zone boréale) :
  `h ← lerp(h, W(p) + 1, My)` où W est le niveau d'eau en escalier (§6.1). Microrelief en
  **cordons et gouilles** : `r = partie_fractionnaire((p · n) / 9 + 0,8 · F(p/30))` avec n la
  direction de plus grande pente du relief lissé (ou θ si la pente est nulle) ; r < 0,35 :
  cordon de tourbe à +1 (buissons, pins nains) ; sinon gouille d'eau affleurante.
* **Palses** (zone subarctique humide) : Worley de pas 26, buttes de +3 à +6 à cœur de glace
  (bloc `permafrost`), mares de dégel entre elles.
* **Glacier** : au-dessus de la ligne des neiges `y_n = y_a + 30` (voir §7.1) et dans la chaîne.
  Surface de glace `G = Hl + 5` où Hl est le relief filtré (h sans §5.2 ni détail de crête) ;
  la glace remplit le volume entre h et G quand G > h. Langues glaciaires : la glace descend dans
  les auges de §5.4 jusqu'à y_a. Crevasses : bandes `partie_fractionnaire((p · n)/14) < 0,08` là où
  la pente de G dépasse 0,35, profondes de 6 à 20. Moraines : bourrelet de gravier de +2 à +4 sur
  le contour. Lac proglaciaire turquoise au front.
* **Lavafält** (Gt > 0) : `h ← lerp(h, S + 6 + 5 · Rg(p/90; 3), Gt)` : plaine de basalte bosselée ;
  fissures `Z(p/700) < 0,01` creusées de 8 blocs (eau chaude au fond) ; pseudo-cratères (cônes de
  4 à 7 blocs, Worley de pas 70) ; un volcan-bouclier central par province (pente douce, +30).

## 6. Hydrographie

L'eau du moteur est statique. Toute nappe doit donc être **horizontale et étanche** par
construction.

### 6.1 Niveau d'eau en escalier

Soit Hl le relief filtré (grandes longueurs d'onde seulement). Le niveau d'eau courant vaut

```
W(x, z) = S + 6 · plancher(max(0, Hl(x, z) − S − 2) / 6)
```

C'est une fonction en marches de 6 blocs. Les rivières et les lacs de vallée s'y calent : une
rivière est une suite de **biefs horizontaux** séparés par des **chutes ou rapides (forsar)** placés
exactement sur les lignes où W change. Cela donne gratuitement les sites de moulins à eau
(doc 04 §6) et de remontée des saumons (doc 03).

### 6.2 Rivières (älvar)

```
pr   = warp(p; 110, 300)
Rv   = Z(pr / 850)
lr   = 0,010 + 0,010 · (1 − C)            plus larges vers l'aval
tr   = min(1, Rv / lr)
lit  = W − 3 − 2 · (1 − tr)               profil en V adouci
h    ← min(h, lerp(lit, h, sm(0,6, 1,0, tr)))   berges raccordées
```

Aux marches de W : barre rocheuse de 2 blocs sous le niveau amont, chute d'eau verticale, bassin
creusé de 4 blocs en aval. En montagne, les rivières empruntent les auges de §5.4 (pas de second
réseau). Deltas : là où une rivière atteint C ≈ 0,47 en côte basse, éventail de bancs de sable.

### 6.3 Lacs

Worley de pas 180. La cellule i porte un lac avec la probabilité `0,15 + 0,6 · Lk` si son centre
est sur terre, hors chaîne, pente du relief lissé inférieure à 5 blocs sur 32.

```
niveau    ℓᵢ = plancher(Hl(cᵢ)) − 1                         constant pour tout le lac
contour   dedans si d < rᵢ · (0,6 + 0,8 · F(p/70; 3)),  rᵢ de 14 à 70
cuvette   h ← min(h, ℓᵢ − pᵢ · (1 − (d / r)²)),  profondeur pᵢ de 3 à 14
margelle  pour r ≤ d ≤ r + 5 : h ← max(h, ℓᵢ + 1), raccordé par sm
```

La margelle garantit l'étanchéité (test : aucun bloc d'eau n'a un voisin latéral d'air sous son
niveau). Les îles lacustres viennent du terme de contour. Lacs de vallée (fingersjöar) : dans
une auge intérieure, le fond est abaissé de 8 blocs sous W sur les tronçons où `F(p/600) > 0,6`,
barrés à l'aval par une moraine.

### 6.4 Mer

* Façade atlantique (O > 0,5) : eau salée, forêts de laminaires, jamais gelée.
* Façade baltique (O < 0,5) : eau saumâtre (bloc d'eau identique, drapeau de biome), banquise
  saisonnière (doc 04 §4).
* Zone arctique (T₀ < 0,12) : banquise permanente. Chenaux et crêtes de pression : Worley de pas
  60, `F2 − F1 < 2` → eau libre ou crête de glace de +2 à +4 selon le hachage de la cellule.

## 7. Biomes

### 7.1 Étagement

La limite des arbres dépend du climat :

```
y_a (limite des arbres)  = S + 14 + 70 · clamp(T₀ − 0,12, 0, 0,6)
y_b (ceinture de bouleaux) = y_a − 8
y_k (roche nue)           = y_a + 16
y_n (neiges éternelles)   = y_a + 30
```

À T₀ = 0,40 (boréal) : arbres jusqu'à S + 34. À T₀ = 0,15 (subarctique) : dès S + 16, la toundra.

Zones de température au niveau de la mer : **némorale** T₀ ≥ 0,70 ; **hémiboréale** 0,50 à 0,70 ;
**boréale** 0,28 à 0,50 ; **subarctique** 0,12 à 0,28 ; **arctique** < 0,12.

### 7.2 Règles de sélection (premier test vrai)

1. Gt > 0,5 → **Lavafält**.
2. Glace présente (§5.8) → **Glaciär**.
3. Colonne sous l'eau de mer : arctique → **Packis** ; fjord → **Fjord** ; K > 0,3 → **Skärgård** ;
   O < 0,5 → **Bottenhav** ; C < 0,25 → **Djuphav** ; sinon **Västerhav**.
4. Rivière ou lac → **Älv** / biome environnant avec drapeau « eau douce ».
5. y > y_k → **Kalfjäll**. y > y_a → **Fjällhed**. y > y_b → **Fjällbjörkskog**.
6. Zone subarctique ou arctique à basse altitude : Hm > 0,6 et plat → **Palsmyr** ; sinon
   **Fjällhed** (toundra de plaine) ou **Fjällbjörkskog** selon Vr.
7. Côte (C < 0,50) : klint → **Klintkust** ; côte soulevée → **Högkust** ; K → **Skärgård** ;
   pré salé → **Strandäng**.
8. Masque My > 0,5 → **Myr**.
9. Province minière (Pv = grönsten) et relief moyen → **Bergslag**.
10. Zone boréale : esker ou Hm < 0,35 → **Tallhed** ; Vr > 0,92 → **Brännfält** ; sinon **Urskog**.
11. Zone hémiboréale : Lk > 0,6 → **Sjöland** ; sinon **Hagmark**.
12. Zone némorale : province calcaire et E > 0,6 → **Alvar** ; Hm > 0,45 → **Bokskog** ; sinon
    **Hagmark**.

### 7.3 Fiches des 24 biomes

Noms suédois (identifiants du jeu), glose française entre parenthèses.

| # | Biome | Zone et relief | Sol | Arbres | Flore et décors | Particularités |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **Västerhav** (mer de l'Ouest) | mer salée, plateau | sable, gravier, roche | | laminaires géantes, fucus, moulières | hareng, morue ; tempêtes |
| 2 | **Djuphav** (abysse) | fosse, S − 28 | vase, roche sombre | | coraux d'eau froide, éponges | requin du Groenland, Kraken |
| 3 | **Bottenhav** (golfe saumâtre) | mer peu profonde | sable, argile bleue | | roselières côtières, potamots | gèle de décembre à avril ; minerai de lac |
| 4 | **Packis** (banquise) | mer arctique gelée | glace de mer, neige | | algues de glace | chenaux, crêtes de pression, morse, ours blanc |
| 5 | **Fjord** | auge noyée entre falaises | gneiss nu, éboulis | pins accrochés | cascades, corniches à oiseaux, saxifrages | eau profonde, vent canalisé, avalanches |
| 6 | **Skärgård** (archipel) | îlots de granit poli | granit, lichen jaune, poches de terre | pins tordus, aulnes, sorbiers | argousier, ciboulette sauvage, bruyère | hameaux de pêche, phoques, labyrinthes de pierres |
| 7 | **Klintkust** (côte à falaises) | falaise calcaire ou craie de 12 blocs | calcaire, galets, silex | genévriers, pins | raukar, orchidées, fossiles | silex, ambre après tempête, grottes marines |
| 8 | **Högkust** (côte soulevée) | terrasses et collines rouges | granit rouge, champs de galets | épicéas, pins | lichens, airelles, anciens rivages | vue dominante, sites de feux d'alarme |
| 9 | **Strandäng** (pré salé) | bande plate à S+1 | herbe rase, vase | saules bas | salicorne, armérie, roseaux | grues et oies en migration, pâturage |
| 10 | **Bokskog** (hêtraie) | collines douces du sud | terre brune, litière de faînes | hêtre, chêne, tilleul | tapis d'anémones au printemps, ail des ours | sous-bois sombre en été, chevreuil |
| 11 | **Hagmark** (bocage pâturé) | plaine vallonnée | herbe, argile | chênes isolés, bouleaux, sorbiers, noisetiers | fleurs de la Saint-Jean, murets de pierre, genévriers | fermes rouges, meules, tumulus, pierres runiques |
| 12 | **Alvar** (steppe calcaire) | dalle plate, sol très mince | calcaire affleurant, herbe sèche | genévrier, potentille arbustive | orchidées, hélianthème, thym | moulins à vent, fortins circulaires, vent fort |
| 13 | **Sjöland** (pays des lacs) | mosaïque de lacs et de buttes | moraine, sable de verrerie | pin, épicéa, bouleau, aulne | nénuphars, roseaux, myrtilles | écrevisses, brochet, saunas de rive, verreries |
| 14 | **Urskog** (forêt primaire) | collines boréales | mousse, podzol, blocs erratiques | épicéa géant, pin, tremble, bouleau | myrtille, airelle, linnée, usnée pendante, bois mort, chanterelles, cèpes | pénombre, ours, loup, lynx, Skogsrå |
| 15 | **Tallhed** (pinède sur sable) | eskers et plaines sableuses | sable, lichen des rennes | pin sylvestre | airelle, bruyère, raisin d'ours | clair et sec, charbonnières, fosses à goudron, pistes |
| 16 | **Myr** (tourbière) | plat, cordons et gouilles | tourbe, sphaigne | pins nains, bouleaux nains | plaquebière, linaigrette, canneberge, droséra | minerai des marais, moustiques, feux follets, sol mouvant |
| 17 | **Älv** (grande rivière) | lit et berges | galets, sable | aulnes, saules, trembles | reine-des-prés, angélique | rapides, saumon, castor, flottage du bois |
| 18 | **Brännfält** (brûlis) | forêt incendiée | cendre, sol nu, troncs noircis | chicots, jeunes bouleaux | tapis d'épilobe rose, morilles | charbon de bois naturel, pic noir ; rare |
| 19 | **Bergslag** (pays minier) | collines moyennes | terre rouge, scories | épicéa, bouleau | chapeau de fer, blocs de magnétite | filons de fer, cuivre, argent ; mines et hauts fourneaux en ruine |
| 20 | **Fjällbjörkskog** (boulaie de montagne) | ceinture sous la limite des arbres | humus mince, blocs | bouleau tortueux | géranium des bois, aconit, trolle, myrtille | hautes herbes, camps sames, rennes en été |
| 21 | **Fjällhed** (lande d'altitude) | plateaux ouverts | lichen, camarine, roche | saules nains, bouleau nain | dryade, azalée naine, linaigrette | cairns, lemmings, vent, voile blanc en hiver |
| 22 | **Kalfjäll** (haute montagne nue) | sommets, champs de blocs | roche gélifractée, névés | | renoncule des glaciers, lichens crustacés | stéatite, tungstène profond, tempêtes |
| 23 | **Glaciär** (glacier) | calotte et langues | glace bleue, neige, moraine | | | crevasses, grottes de glace, lac turquoise, Jätte |
| 24 | **Palsmyr** (tourbière à palses) | subarctique humide | tourbe gelée, permafrost | bouleau nain | plaquebière, linaigrette | buttes à cœur de glace, mares, oiseaux nicheurs |
| 25 | **Lavafält** (champ de lave) | îles du large | basalte, mousse grise épaisse, sable noir | bouleaux bas, angélique | lupin, thym arctique | geysers, sources chaudes, tunnels de lave, orgues basaltiques |

(25 lignes : Älv est un biome linéaire ; le compte de biomes « de surface » est 24.)

### 7.4 Transitions

1. **Relief** : la hauteur ne dépend d'aucun biome, donc aucune marche aux frontières.
2. **Écotones explicites** : Fjällbjörkskog est la transition Urskog → Fjällhed ; Strandäng et
   Högkust sont les transitions mer → terre ; Tallhed borde les Myr ; la lisière Bokskog → Urskog
   passe par Hagmark ou Sjöland parce que les zones de température sont ordonnées.
3. **Frontière tramée** : à moins de 12 blocs d'une frontière, le biome utilisé pour le sol et la
   végétation de chaque colonne est tiré entre les deux voisins avec la probabilité
   `sm(−12, 12, d)` où d est la distance signée estimée (`(valeur − seuil) / norme du gradient`),
   comparée à `h(graine, x, z)`. Les deux flores s'interpénètrent sur 24 blocs.
4. **Densités continues** : la densité d'arbres est multipliée par des facteurs continus
   (`sm(y_a, y_a − 10, y)` pour la raréfaction en altitude, `1 − My` en bord de tourbière) : les
   forêts s'éclaircissent au lieu de s'arrêter.
5. **Teintes** : moyenne des teintes de biome sur un carré de 5 × 5 colonnes dans le mailleur.

### 7.5 Cibles de répartition (préréglage Norrland, 16 × 16 km autour de l'origine)

Mers 30 % (dont Fjord 2, Skärgård 4, Packis 3) ; Urskog 14 ; Tallhed 7 ; Myr 6 ; Sjöland 7 ;
Hagmark 6 ; Bokskog 3 ; Alvar 1 ; Fjällbjörkskog 5 ; Fjällhed 6 ; Kalfjäll 3 ; Glaciär 1 ;
Palsmyr 2 ; Bergslag 2 ; côtes 4 ; Brännfält 0,5 ; Lavafält 0,5 ; Älv 2. Un test mesure ces parts
(tolérance ±40 % relatifs) sur 3 graines.

## 8. Sous-sol

### 8.1 Strates

| Profondeur sous la surface | Bloc |
| --- | --- |
| 0 | sol du biome (herbe, mousse, lichen, tourbe, sable, roche nue si pente ≥ 4) |
| 1 à 3 | **morän** (moraine caillouteuse) ; sable sur esker ; tourbe en Myr (jusqu'à 4) ; argile bleue en plaine et fond de golfe |
| au-delà | socle de la province Pv |
| y < 14 | **djupberg** (roche profonde, plus dure) |
| y ≤ 1 | bedrock |

Provinces du socle (une par cellule Pv, frontière déformée) : **granit gris** (50 %), **gneiss**
rubané (20 %, obligatoire dans la chaîne), **granit rouge** (8 %), **calcaire** (8 %, sud-est et
Alvar), **grès** (5 %), **schiste alunifère** (4 %, combustible), **grönsten** (roche verte
minéralisée, 4 %), **porphyre** (1 %, décoratif). Lavafält : **basalte**.
**Filons de diabase** : murs noirs verticaux de 2 blocs, `Z(p/900) < 0,004`, qui traversent tout
(repère visuel en falaise).

### 8.2 Minerais

Il n'y a **pas de charbon minéral** (fidèle à la Suède) : toute la métallurgie repose sur le
charbon de bois. Ni diamant ni émeraude.

| Ressource | Gisement | Formule ou règle |
| --- | --- | --- |
| **Myrmalm** (fer des marais) | Myr, sous la tourbe | plaques où `F(p/25) > 0,68`, profondeur 1 à 2 |
| **Sjömalm** (fer de lac) | fond des lacs de Sjöland et Tallhed, Bottenhav peu profond | plaques où `F(p/20) > 0,7`, surface du fond |
| **Fer de roche** (magnétite, hématite) | Bergslag, grönsten et contact calcaire | filons : `abs(2·N₃(P/40) − 1) < 0,035` et `N₃(P/90) > 0,55`, y de 10 à 60 |
| **Malmberg** (montagne de fer) | subarctique, dans la chaîne | grille de pas 512, 1 cellule sur 2 : lentille ellipsoïdale (demi-axes 60, 14, 45) inclinée à 60°, `Σ((P − c) · eₖ / aₖ)² < 1 + 0,25 · N₃(P/12)`, affleurante |
| **Cuivre** | Bergslag | grille de pas 384 : amas irrégulier de rayon 12 à 20, auréole de pyrite et de terre rouge |
| **Argent** | calcaire et dolomie | veines fines, y de 5 à 30, `abs(2·N₃(P/28) − 1) < 0,015` |
| **Cobalt** | Bergslag | poches de 3 à 6 blocs, y < 35, rares |
| **Or** | nord, veines de quartz y < 25 ; paillettes dans le gravier des rivières du nord | très rare ; orpaillage à la batée |
| **Tungstène** (« tung sten », pierre lourde) | granit profond sous Kalfjäll, y < 18 | poches de 2 à 4 blocs, fin de jeu |
| **Stéatite** (täljsten) | Fjällhed, Kalfjäll | lentilles affleurantes, roche tendre à sculpter |
| **Silex** | craie de Klintkust | rognons en bandes horizontales |
| **Ambre** | plages du sud | n'est pas généré : déposé par les tempêtes (doc 04 §3) |
| **Quartz, argile, sable de verrerie, tourbe, soufre (Lavafält)** | selon biome | plaques de surface |

Indices de surface : chapeau de fer (terre rouille) au-dessus des filons, blocs de magnétite
erratiques autour d'un Malmberg (ils dérèglent la boussole, doc 04 §7).

### 8.3 Cavités

Le bouclier scandinave a peu de grottes ; Norrcraft aussi, et chacune a une origine lisible.

| Type | Où | Construction |
| --- | --- | --- |
| Grottes karstiques | province calcaire seulement | tunnels `abs(2·N₃ − 1) < seuil` sur deux bruits croisés, rivière souterraine au niveau W |
| Failles (sprickgrottor) | granit et gneiss | `Z(p/600) < 0,003` extrudé sur 10 à 25 blocs, 1 à 2 de large |
| Grottes marines | Klintkust, parois de Fjord | demi-ellipsoïdes au niveau S, par grille de pas 48 |
| Tunnels de lave | Lavafält | tubes horizontaux suivant la pente, effondrements ponctuels |
| Grottes de glace | Glaciär | tunnels de fonte au contact glace-roche, saisonniers |
| Chaos de blocs | Urskog, Högkust | empilements de blocs géants avec vides (structure) |
| Salles de la montagne (bergasalar) | sous la chaîne, rares | grande salle ellipsoïdale, structure de troll (doc 03) |
| Marmites de géant | lits de rivière, côtes | puits cylindriques de 2 × 5 blocs |

## 9. Flore

### 9.1 Essences et algorithmes de forme

Chaque arbre est une fonction pure de `(graine, cellule)` ; hauteur et silhouette tirées par
hachage. Blocs : un tronc et un feuillage par essence (doc 04 §9).

| Essence | Biomes | Hauteur | Algorithme |
| --- | --- | --- | --- |
| **Gran** (épicéa commun) | Urskog, Sjöland, Bergslag, Högkust | 9 à 22 | cône : rayon à la hauteur relative v, `r = R · (1 − v)^0,8` approché par table ; verticilles tous les 2 blocs, branches tombantes (bloc en contrebas au bout) ; géants de 2 × 2 en Urskog |
| **Tall** (pin sylvestre) | Tallhed, Urskog, Skärgård, Fjord | 8 à 16 | fût nu sur 60 %, tronc orangé en haut, houppier aplati en 2 ou 3 plateaux décalés ; versions tordues et basses en Skärgård et sur corniches |
| **Björk** (bouleau verruqueux) | Hagmark, Sjöland, Urskog | 7 à 13 | tronc fin blanc, ellipsoïde allongé, rameaux pendants (feuilles en colonne de 1 à 2 sous le houppier) |
| **Fjällbjörk** (bouleau tortueux) | Fjällbjörkskog | 3 à 6 | 2 à 4 tiges depuis la souche, marche aléatoire biaisée vers le haut et sous le vent (θ), petites touffes |
| **Bok** (hêtre) | Bokskog | 12 à 20 | fût lisse gris, branches hautes en éventail, dôme large à deux couches qui ferme la canopée |
| **Ek** (chêne) | Hagmark, Bokskog | 8 à 14, tronc 2 × 2 pour les vénérables | branches maîtresses par colonisation d'espace simplifiée (6 à 9 points d'attraction dans un ellipsoïde aplati), houppier très large |
| **Rönn** (sorbier) | lisières, Skärgård, Fjällbjörkskog | 4 à 7 | petit arbre rond, grappes de baies rouges en automne |
| **Asp** (tremble) | Urskog, Älv | 9 à 15 | colonne étroite, bosquets clonaux (5 à 12 arbres serrés partageant un hachage), or en automne |
| **Al** (aulne) | rives, Sjöland, Skärgård | 6 à 10 | cône arrondi, toujours à moins de 4 blocs de l'eau |
| **Sälg / vide** (saules) | Strandäng, Älv, Fjällhed | 1 à 4 | buissons en boule |
| **En** (genévrier) | Alvar, Hagmark, Skärgård | 1 à 4 | colonne étroite ou coussin |
| **Lind, hassel** (tilleul, noisetier) | Bokskog, Hagmark | 3 à 12 | variantes de Bok et de buisson |

Bois mort : chicots (troncs sans houppier), troncs couchés moussus, souches ; 8 % des arbres en
Urskog (habitat des champignons et du pic).

### 9.2 Placement

Grille jitterée de pas g propre à l'essence (g = 5 pour les forêts denses, 9 pour le bocage).
Une cellule porte un arbre si `h(cellule) < densité_biome · bosquet(p) · facteurs_continus`, avec
`bosquet(p) = 0,4 + 1,2 · F(p/90; 2)` (clairières naturelles). Rejets : pente ≥ 3, eau, structure,
sol non fertile. L'essence est tirée dans le mélange pondéré du biome, modulé par Vr
(peuplements purs par taches).

### 9.3 Strate basse

| Plante | Biomes | Usage en jeu (doc 04) |
| --- | --- | --- |
| Myrtille (blåbär) | Urskog, Sjöland, Fjällbjörkskog | baie d'été, tarte, soupe |
| Airelle (lingon) | Tallhed, Urskog, Högkust | confiture qui ne se gâte pas |
| Plaquebière (hjortron) | Myr, Palsmyr | « or des marais », rare, forte valeur |
| Fraise des bois (smultron) | Hagmark, lisières | à enfiler sur un brin d'herbe |
| Canneberge, camarine | Myr, Fjällhed | baies d'hiver sous la neige |
| Églantier, argousier | côtes | vitamines d'hiver (soupe de cynorrhodon) |
| Bruyère (ljung) | Tallhed, Skärgård, Alvar | miel de bruyère, teinture |
| Lichen des rennes (renlav) | Tallhed, Fjällhed | fourrage d'hiver des rennes |
| Sphaigne (vitmossa) | Myr | isolant, pansement |
| Usnée (skägglav) | Urskog | allume-feu, signe de vieille forêt |
| Chanterelle, cèpe, trompette | Urskog, Bokskog, automne | cuisine |
| Amanite tue-mouches | Urskog, Fjällbjörkskog | toxique ; usage rituel (doc 04 §7) |
| Anémone des bois (vitsippa), hépatique | Bokskog, printemps seulement | Mys, décor |
| Fleurs de la Saint-Jean (7 espèces : géranium des bois, marguerite, campanule, trèfle rouge, bleuet, renoncule, linnée) | Hagmark, Fjällbjörkskog | rituel de la Saint-Jean, couronnes |
| Épilobe (rallarros) | Brännfält, talus | tisane |
| Linaigrette (ängsull) | Myr, Fjällhed | mèche de bougie |
| Angélique (kvanne) | Älv, Fjällbjörkskog, Lavafält | légume et remède same |
| Orchidées (Adam et Ève) | Alvar, Klintkust | rare, décor, teinture |
| Dryade, renoncule des glaciers | Fjällhed, Kalfjäll | la fleur la plus haute du monde |
| Roseau (vass), massette, nénuphar blanc et rouge | lacs, Bottenhav | toits de chaume, vannerie |
| Fucus, laminaire, zostère | mers | engrais, sel d'algue, nourriture |
| Lin, chanvre sauvages ; ail des ours ; cumin des prés | Hagmark, Bokskog | semences de départ |

Répartition : pour chaque plante, densité = base du biome × `tache(p)` (fBm à 20 à 60 blocs, seuil
propre) × disponibilité saisonnière. Les baies et champignons ont un état « mûr » piloté par le
calendrier (doc 04 §2).

## 10. Structures

### 10.1 Catalogue

| Structure | Biomes | Contenu et rôle |
| --- | --- | --- |
| **Torp** (ferme rouge) | Hagmark, Sjöland, Urskog (clairière) | maison de rondins peinte en rouge de Falun à angles blancs, toit de tourbe ou de tuiles de bois, étable, cellier enterré, mât de la Saint-Jean, potager |
| **By** (village) | Hagmark, Bokskog | 4 à 8 fermes autour d'une place, forge, four à pain commun, clocher de bois séparé |
| **Lada** (grange à foin) | Hagmark, Strandäng, Myr | petite grange grise aux murs évasés, foin |
| **Fäbod** (estive) | Fjällbjörkskog, haut d'Urskog | 2 ou 3 cabanes basses, enclos, laiterie ; destination d'été du bétail |
| **Fiskeläge** (hameau de pêche) | Skärgård, Fjord, Klintkust | cabanes sur pilotis (sjöbodar), pontons, séchoirs à poisson, fumoir, barques |
| **Bastu** (sauna de rive) | Sjöland, Skärgård | cabane, poêle à pierres, ponton vers l'eau |
| **Stavkyrka** (église en bois debout) | Fjord, Hagmark | toits étagés, têtes de dragon, cloche (éloigne les trolls) |
| **Runsten** (pierre runique) | partout sauf haute montagne | enseigne une rune (doc 04 §7), parfois une indication de direction |
| **Gravhög** (tumulus) | Hagmark, Alvar | chambre funéraire, épreuve du Lindorm pour les plus grands |
| **Skeppssättning** (navire de pierres) | côtes du sud, Alvar | alignement de menhirs en coque ; cadran des solstices |
| **Fornborg** (fortin circulaire) | Alvar, Högkust | enceinte de pierres sèches, puits, fondations de maisons |
| **Väderkvarn** (moulin à vent sur pivot) | Alvar, Strandäng | moulin fonctionnel (moud le grain) |
| **Vårdkase** (feu d'alarme) | sommets côtiers, Högkust, crêtes | bûcher sur trépied ; un des neuf est un objectif (doc 05) |
| **Fyr** (phare de fer ou de pierre) | Skärgård, caps | lampe, logement du gardien |
| **Trojaborg** (labyrinthe de pierres) | Skärgård, côtes | spirale de galets ; rituel de pêche |
| **Hällristning** (roche gravée) | Högkust, Skärgård, Hagmark | navires et cerfs gravés, teinte rouge ; indices de saga |
| **Gruva** (mine abandonnée) | Bergslag | chevalement, puits boisé, galeries, wagonnets, eau au fond |
| **Hytta** (haut fourneau en ruine) | Bergslag, près d'un fors | cuve de pierre, roue à eau, halle à charbon, tas de scories bleues |
| **Kolarkoja et kolmila** (hutte et meule de charbonnier) | Tallhed, Urskog | meule éteinte, charbon de bois |
| **Tjärdal** (four à goudron) | Tallhed | entonnoir de terre, gouttière, tonneaux |
| **Kåta / goahti** (camp same) | Fjällbjörkskog, Fjällhed, Palsmyr | 2 à 4 tentes ou huttes de tourbe, enclos à rennes, garde-manger sur poteau (njalla), habitants amicaux |
| **Sejte** (pierre sacrée same) | Fjällhed, rives de lacs de montagne | rocher singulier ; lieu d'offrande, jamais de butin |
| **Fjällstuga** (refuge de montagne) | Fjällhed, Kalfjäll | cabane avec poêle, bois, couchettes ; vitale dans le blizzard |
| **Rösen** (cairns) et **led** (piste) | Fjällhed, Kalfjäll, Myr (caillebotis) | jalons tous les 24 blocs entre refuges (§10.3) |
| **Vindskydd** (abri sous appentis) | Urskog, Sjöland | abri ouvert, foyer, bûcher |
| **Isfiskeark** (cabane de pêche sur glace) | lacs et Bottenhav, hiver seulement | posée par la surcouche saisonnière |
| **Långhus** (halle viking en ruine) | Hagmark, Fjord | poteaux, foyer central, coffre |
| **Skeppsvrak** (épave) | Bottenhav, Västerhav | navire de guerre du XVIIe conservé par l'eau saumâtre ; drakkar en tumulus |
| **Bro** (pont de pierre) | Älv | arche ; repaire possible de troll |
| **Bäverdamm** (barrage de castor) | Älv, Sjöland | digue de branches, hutte, étang en amont (lié au doc 03) |
| **Björnide** (tanière d'ours) | Urskog | cavité sous souche ; occupée en hiver |
| **Bergasal** (salle du roi de la montagne) | sous la chaîne | épreuve de fin de chapitre (doc 05) |
| **Geyser, source chaude, orgues basaltiques** | Lavafält | bain chaud naturel, cuisson dans le sol |
| **Flyttblock** (bloc erratique) | Urskog, Tallhed, Hagmark | rocher de 3 à 7 blocs ; les plus gros sont des trolls pétrifiés |
| **Rauk, cascade, chaos de blocs, marmite** | selon §5 et §8.3 | décors de relief |

Les camps et lieux sacrés sames sont traités avec respect : habités, amicaux, sources de savoir
(doc 05, chapitre IV), jamais des donjons à piller. Y voler dégrade le Naturfrid (doc 04 §7).

### 10.2 Placement

Trois grilles hachées indépendantes, pour que les grandes structures ne chassent pas les petites :

| Classe | Pas de grille | Exemples |
| --- | --- | --- |
| Majeure | 384 | By, Stavkyrka, Gruva, Hytta, Fornborg, Bergasal, Vårdkase |
| Moyenne | 96 | Torp, Fäbod, Fiskeläge, Kåta, Fjällstuga, Gravhög, Fyr, Långhus |
| Mineure | 32 | Runsten, Lada, Vindskydd, Sejte, Trojaborg, Flyttblock, Kolmila, Rösen |

Pour chaque cellule : tirage du type parmi ceux que le biome du centre autorise (poids), puis
**prédicat de site** propre au type, par sondes du relief : pente maximale, altitude, distance à
l'eau (`Fiskeläge` : rive à moins de 6 ; `Hytta` : marche de W à moins de 20 ; `Vårdkase` : maximum
local de h dans un rayon de 40 ; `Bastu` : rive de lac). Échec du prédicat : la cellule reste vide
(pas de second tirage, pour rester pur et borné). Fondations, dégagement de la végétation et
rotation : mécanisme `Build` existant.

Les neuf **Vårdkasar de la saga** et les lieux d'épreuve ne sont pas laissés au hasard : leur
position est une fonction de la graine cherchée sur un anneau de distance imposé (doc 05 §4).

### 10.3 Pistes

Entre deux structures « à piste » (refuges, camps, fäbodar, villages) distantes de moins de 400
blocs, on trace une piste déterministe : segment droit échantillonné tous les 8 blocs, chaque point
déplacé perpendiculairement vers le minimum de coût `pente² + pénalité d'eau` dans une fenêtre de
±12 blocs, puis lissé. Sur la piste : cairns en montagne, caillebotis de planches en tourbière,
gués ou ponts de rondins sur les rivières, sentier tassé en forêt. Calcul borné (au plus 50
points), fait par cellule, indépendant des chunks.

## 11. Préréglages Norrcraft

| Préréglage | Effet |
| --- | --- |
| **Norrland** (défaut) | biais nord-sud sur T₀ et ouest-est sur O (§4) |
| **Skärgård** | B(C) abaissé de 5, K × 2 : mer et îles dominantes, navigation au centre du jeu |
| **Högfjäll** | U + 0,25, T₀ − 0,15 : montagne et toundra dominantes, monde rude |

## 12. Performance et outillage

* Budget : 2,5 ms par chunk sur un thread (vanilla : 1,6 ms). Leviers : treillis 4 × 4 des champs
  lents, cache de colonne par chunk, bruit 3D seulement sous les biomes qui ont des cavités ou des
  filons.
* Outil `worldmap` étendu à Norrcraft : carte des biomes, relief ombré, champs C, T₀, U, W,
  hydrographie, sites de structures ; `blockmap` pour les coupes (profil de fjord, lentille de
  Malmberg).

## 13. Tests d'acceptation du worldgen

* Continuité : `abs(h(x+1, z) − h(x, z)) ≤ 3` hors falaises déclarées (klint, parois de fjord,
  crevasses, marches de W).
* Indépendance de l'ordre des chunks (arbres, structures, lacs, pistes).
* Déterminisme : empreintes de 64 chunks pour 3 graines, identiques sous Linux et Windows.
* Étanchéité : aucun bloc d'eau avec un voisin latéral d'air sous son niveau.
* Répartition des biomes dans les cibles de §7.5 ; chaque biome présent à moins de 6 km de
  l'origine pour 3 graines.
* Au moins un fjord de plus de 300 blocs de long et 25 de profondeur, un lac de plus de 80 blocs,
  un Malmberg et un Bergslag à moins de 5 km de l'origine.
* Point de départ : zone hémiboréale ou némorale, à moins de 200 blocs d'une côte ou d'un lac.
