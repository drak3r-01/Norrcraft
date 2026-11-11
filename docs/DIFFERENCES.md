# Différences avec le jeu Java

La réécriture vise l'identique : mêmes constantes, mêmes seuils, même ordre de mise à jour,
même génération de monde pour une même graine. Ce document liste **tout** ce qui diffère
volontairement, ainsi que les bogues Java gardés exprès. Le détail (et la justification
ligne à ligne) est dans `docs/spec/*.md` ; les décisions de Gabriel sont consignées dans
`docs/spec/contrats.md`.

## 1. Bogues Java corrigés (décisions de Gabriel)

| # | Java | Rust | Où / test |
|---|---|---|---|
| 1 | Monde **FLAT** noyé sous une nappe de lave (y 1..11) : la passe « lacs de lave » remplit tout l'air sous y = 12 | la passe de lave ne s'exécute que si les grottes sont actives : FLAT est un vrai monde plat. NORMAL / AMPLIFIED / ISLAND restent identiques bit à bit | `nb-world` (`WorldGenerator::with_java_lava_pass` garde l'ancien comportement pour les tests) ; `tests/reference::flat_corrected_has_no_lava`, `flat_java_mode_matches_reference` |
| 2 | **Germe saisi ignoré** à la création d'un monde (le tampon était vidé avant d'être lu) : graine toujours aléatoire | le germe saisi est utilisé (`WorldSelectScreen::fix_seed_input`, vrai par défaut) | `nb-ui` ; `world_select_tests::seed_fix_flag`, `create_flow_keeps_java_seed_bug` |
| 3 | **PAUSED → MAIN MENU** laissait le monde chargé ; renommer ou supprimer ce monde puis en charger un autre le resauvegardait sous l'ancien nom (dossier recréé) | le monde est sauvegardé puis **déchargé** ; un monde de menu neuf (graine aléatoire, en mémoire, jamais sauvegardé) le remplace. Seul effet visible : le fond du menu principal | `nb-game` (`Game::unload_to_menu_world`) ; `tests/game::state_transitions_follow_java` |
| 4 | Boucle fixe 60 Hz **sans borne** : après une longue pause (débogueur, fenêtre déplacée) des centaines de ticks d'affilée | durée d'une frame bornée à **0,25 s** avant l'accumulateur (au plus 15 ticks par frame) | `nb-game:fixed_step` (`FixedStep::java()` garde l'ancien comportement) ; `fixed_step::frame_is_clamped` |
| 5 | Ordre de dessin des chunks = ordre d'un `HashMap<ChunkPos>` (non déterministe) | quadtree construit depuis une liste **triée par (x, z)** ; chunks actifs triés avant le remaillage | `nb-mesh`, `nb-game` ; `tests/culling::quadtree_build_is_order_independent` |
| 6 | **Course lumière / `setBlock`** : un calcul de lumière commencé avant une écriture de bloc remettait `lightDirty` à faux → lumière périmée | compteur d'éditions par chunk : le calcul périmé ne valide pas le chunk | `nb-world` (`manager.rs`, `chunk.rs`) |
| 7 | Ombres **OFF** : la carte d'ombre restait échantillonnée sans être effacée (ombres figées ou contenu non initialisé, souvent tout ombré) | la carte est effacée à **1.0** au démarrage et au passage à OFF (aucune ombre) | `nb-gfx` ; `tests/render::shadows_and_shadow_off_clear` |
| 8 | `treeDensityOverrides` de longueur < 8 dans un `world.json` : exception dans la génération, chunks du biome jamais chargés | tableau complété par −1.0 (ou tronqué) à la lecture | `nb-world:config::tolerant_json_and_override_padding` |
| 9 | Ordre d'itération des entités = ordre d'un `HashMap<Integer>` | ordre **déterministe par id croissant** | `nb-ecs:world::query_is_sorted_snapshot` |
| 10 | `saves/mobs_defaults.json` du Java : renne `aggressorDetectRange = 0`, `aggressorFleeSpeed = 0` (faux par rapport au code `ReindeerDefinition`) | 10 et 3, comme le code Java (seul le fichier de référence change ; le comportement est celui du Java) | `assets/vanilla/mobs_defaults.json` ; `nb-data:mobs::builtin_order_and_values` |
| 11 | `groupSizeMax < groupSizeMin` ou rayon de spawn nul : exception `nextInt(≤ 0)` | aucun spawn pour ce mob + message, pas de panique | `nb-ecs:tests/mobs::far_mobs_despawn_and_degenerate_configs_do_not_panic` |
| 12 | `world.json` illisible : métadonnées neuves (graine 0) | la graine du monde est conservée | `nb-game` |
| 13 | Culling : un nœud du quadtree était testé sans la marge d'un demi-chunk (un chunk est rangé par son centre et dépasse de son quadrant) ; caméra au sol, le chunk sous les pieds disparaissait par intermittence et on voyait le dessous du monde | nœuds élargis d'un demi-chunk dans `query_node` | `nb-mesh`, `tests/culling::quadtree_never_drops_visible_chunk_near_camera` |

## 2. Choix de comportement (hors bogues)

* **Nombres avec un point décimal**, toujours : les réponses des commandes (`/time`,
  `/player`, `/physics`…) et les valeurs affichées (`%.2f`, arrondi HALF_UP comme Java)
  n'utilisent jamais la virgule de la locale française (le Java dépendait de la locale).
  `nb-ecs:javafmt::format_fixed_half_up_with_point`.
* **Caractères hors ASCII imprimable affichés « ? »** (police `stb_easy_font`) : gardé tel quel.
  Un monde « Forêt » s'affiche « For?t » ; le nom saisi et le dossier restent intacts.
  `nb-ui:font::non_ascii_replaced`.
* **Sauvegardes** : nouveau format **versionné**, **sans compatibilité** avec les mondes Java
  (ni lecture des anciens `world.json` sans version, ni des `.bin`) :
  `saves/<nom>/world.json` (`"formatVersion": 2`, serde) et `saves/<nom>/chunks/c_<cx>_<cz>.nbc`
  (en-tête `NBCK`, version, layout, compression RLE). « Jamais joué » est `player: null` au lieu
  de la sentinelle `playerY == 100.0` (un joueur sauvegardé pile à y = 100 n'est plus renvoyé au
  spawn). Un nom assaini vide donne le dossier `world`.
* `saves/mobs.json` lu avec un vrai parseur JSON (`serde_json`) : un fichier invalide est ignoré
  en entier (message `Could not load mobs.json` comme en Java), un exposant (`1.0E-4`) est lu
  correctement (le parseur maison du Java le tronquait à 1.0) et un entier écrit `2.0` est
  ignoré (le Java lisait 2). Sans effet sur les fichiers écrits par le jeu lui-même.
* Date « Last played » de l'écran des mondes formatée en **UTC** (Java : fuseau local).
* Graines des générateurs aléatoires de l'ECS et du bouton « Random » tirées de l'horloge /
  de l'entropie système (Java : `new Random()`), injectables pour les tests.
* Changement de monde : génération de maillage incrémentée (tout maillage en vol de l'ancien
  monde est jeté) au lieu d'un simple `chunkMeshes.clear()`.
* Packs de ressources : en plus de `./resource_packs` et `../resource_packs`, le dossier
  `resource_packs/` **à côté de l'exécutable** est cherché (équivalent du mode installé
  jpackage du Java). L'icône de la fenêtre est intégrée à l'exécutable (comme dans le jar).
* Paquet distribuable : `saves/settings.json` livré avec les valeurs par défaut (le Java le
  créait au premier enregistrement des réglages ; mêmes valeurs).

## 3. Traduction OpenGL → Vulkan (sans effet visible attendu)

* Clip space Vulkan (Y vers le bas, profondeur 0..1) compensé dans les matrices ; les tests
  de frustum utilisent toujours les matrices GL (JOML).
* `LESS_OR_EQUAL` pour toutes les pipelines 3D (Java : `LESS` jusqu'au premier affichage de la
  surbrillance, puis `LEQUAL` pour toujours).
* Un seul temps shader par frame (Java : `glfwGetTime()` relu à chaque passe, quelques µs
  d'écart).
* VSync coupée : `IMMEDIATE`, à défaut `MAILBOX`, repli `FIFO`. Plein écran exclusif, à défaut
  sans bordure. Fenêtre minimisée (0×0) : rendu sauté.
* Passe mobs + particules : seul le tampon couleur est attaché (en GL, le second draw buffer
  recevait une valeur indéfinie).
* Nuages échantillonnés à `(gridPos + 0.25) / 256` (robustesse NEAREST, même texel qu'en GL).
* Interface : rendue dans la même passe Vulkan après la scène (Java : appels GL immédiats).
* Console : l'exécutable Windows ouvre une console (journal : GPU, packs, erreurs).

## 4. Bogues Java conservés volontairement

Liste non exhaustive des plus visibles (la liste complète est dans les sections « Bizarreries
du Java à reproduire » de chaque spec) :

* **Génération** (inchangée pour garder les mêmes mondes) : océans « rehaussés »,
  AMPLIFIED sans côtes, ISLAND qui réduit l'océan, grottes aux y pairs qui percent la surface et
  laissent de l'eau suspendue, champignons souterrains même en surface, végétation dont seule
  la première entrée gagne (fleurs, fougères, buissons morts… jamais générés), végétation sur la
  pierre, feuillages coupés aux bords des chunks, minerais bloc par bloc, sur pierre et grès.
  (`nb-world.md` §18)
* **Monde** : tout chunk chargé est réécrit au déchargement ; rayon visible effectif =
  distance de rendu − 2 ; lumière limitée (halo de 2 blocs, voisins non recalculés après une
  édition de bord) ; `walkSpeed`/`sprintSpeed`/`flySpeed`/`attackDamage` non persistés ; santé
  max non sauvegardée.
* **Rendu** : seuils d'ombre 1000/2000 (feuilles et plantes projettent des ombres pleines),
  halo du bloom (tampon lumineux effacé à la couleur du brouillard), eau invisible vue de
  dessous, mobs non éclairés, particules à l'envers, branche « lave » morte dans `main.vert`
  (la lave ondule comme l'eau), faces internes des feuilles, eau contre lave en z-fighting,
  dernière frame des textures animées, occlusion avec une frame de latence.
  (`nb-mesh.md` §18, `nb-gfx.md` §17)
* **Gameplay** : créatif sans inventaire infini (poser consomme, casser ramasse, la bedrock
  se casse), Q détruit l'objet, sauts répétés tant que Saut est maintenu, FOV qui devient NaN
  avec `sprintSpeed = 0`, mobs aquatiques sans gravité hors de l'eau, raycast des mobs à travers
  les murs et prioritaire sur les blocs, apparition des mobs dans un carré (les coins
  disparaissent au cycle suivant), `/default` qui ne remet pas la durée du jour, `/kill` qui
  vide l'inventaire, reliquat perdu quand l'inventaire est plein. (`nb-ecs.md` §20)
* **Interface** : clic « traversant » à l'entrée de certains écrans, Échap pendant une
  réaffectation qui quitte l'écran, titres centrés approximativement, densité d'arbres « Auto »
  affichée « -0.0010 », médium AIR d'un mob repassé en LAND à l'édition. (`nb-ui.md` §5)
* Bloc inconnu dans un inventaire sauvegardé : ignoré au chargement.

## Corrections visuelles volontaires (passe 2026-11-07)

Écarts assumés par rapport au Java, sans changement de gameplay :

* **Ombres** : seuils 200000/100000 dans `shadow.vert` (`fix_shadow_layer_thresholds` vaut
  maintenant `true` par défaut) : feuilles et plantes n'échantillonnent plus une couche de
  texture invalide.
* **Meshing** : une fusion greedy n'a lieu que le long d'un axe où l'AO et la lumière lissée sont
  constantes (sinon le dégradé s'étirait sur tout le quad) ; la diagonale d'un quad suit la
  luminosité des coins (plus de zébrures d'AO), même sens d'enroulement.
* **Occlusion** : les chunks autour de la caméra ne sont jamais masqués (leur boîte, vue de
  l'intérieur, donnait 0 échantillon en grotte et faisait clignoter le chunk) ; un chunk qui
  rentre dans le frustum repart de « visible » ; entrées des chunks déchargés purgées.
* **Fluides** : chunks dessinés de l'arrière vers l'avant (mélange alpha correct entre chunks).

## Rendu : eau, lave, atmosphère, mobs (passe 2026-11-08)

Écarts assumés par rapport au Java, sans changement de gameplay (détails dans les en-têtes des
shaders `crates/nb-gfx/shaders/glsl450`) :

* **Eau** (`fluid.frag`, nouveau) : vagues à normales analytiques (6 sinus, octaves fines
  atténuées avec la distance), **réfraction** (copie de la couleur du terrain opaque, décalage
  selon la pente et l'épaisseur, jamais de pixel situé devant la surface), absorption de
  Beer-Lambert par canal, réflexion du ciel pondérée par Fresnel, reflet et scintillement du
  soleil / de la lune, écume de berge. La houle des sommets est réduite (0,07 + 0,035 au lieu de
  0,2 + 0,1). Coût : une copie plein écran de l'image HDR, uniquement les frames où un chunk
  contient un fluide visible.
* **Lave** : texture déformée par deux champs d'écoulement, plaques de croûte froides, veines
  très chaudes ; identifiée par sa couche d'atlas (`flags.w`) et non par la couleur du texel
  (les texels sombres étaient traités comme de l'eau) ; bloom nettement réduit.
* **Ombres** : l'eau, la lave et les plantes en croix (herbes, fleurs) ne projettent plus d'ombre
  (leurs ombres découpées tachetaient les mobs et le sol : les mobs semblaient transparents).
* **Mips de l'atlas** : couleur moyennée pondérée par l'alpha et plus de « dépremultiplication »
  dans `main.frag` : fin du liseré clair autour des feuilles et des herbes.
* **Mobs et particules** écrivent `(0, 0, 0, alpha)` dans le bright buffer : le bloom du terrain
  derrière eux (lave) ne se superpose plus à leur silhouette.
* **Jour** : horizon et zénith plus saturés, ombre bleutée / soleil chaud (éclairement total
  inchangé), brume à extinction par canal et densité décroissante avec l'altitude, lobe de
  diffusion large du soleil, tonemapping ACES mélangé à une courbe qui conserve la teinte,
  disque solaire à cœur blanc ; variation de teinte à grande échelle de l'herbe et des feuilles.

## Génération du monde « Norrblock » (passe 2026-11-07)

Décision de Gabriel : la contrainte « à l'identique » est levée **pour la génération** (le reste
du jeu reste fidèle). Nouveau générateur par défaut : relief continu, 30 biomes, 33 blocs,
arbres et décors variés, grottes sans puits, minerais en filons, nouveaux types de monde
(Archipelago, Highlands, Classic). Détail et principes : `docs/WORLDGEN.md`.

* Le générateur Java reste complet (`GeneratorKind::Classic`, type « Classic ») et garantit
  toujours les vecteurs de référence ; `WorldConfig::preset_java` conserve les préréglages
  d'origine.
* Un `world.json` sans champ `generator` est lu comme `Classic` (pas de raccord dans un monde
  commencé avant).
* `treeDensityOverrides` garde 8 valeurs : les nouveaux biomes héritent de celle de leur
  « famille » Java (`Biome::family`).
* Les mondes Amplified et Island sont réajustés pour le nouveau générateur.

## Structures, flore et faune (phase 9)

Hors fidélité Java, comme la génération (`docs/STRUCTURES.md`, `docs/FLORE.md`, `docs/FAUNE.md`) :

* **Structures** : 19 types posés par cellules de 64 blocs (générateur Norrblock uniquement ;
  curseur `structureDensity`). Aucune structure en `Classic`.
* **Blocs** : 51 blocs de plus (identifiants 68 à 118) ; un monde créé avant n'en contient pas.
* **Arbres et plantes** : 8 arbres, 16 plantes par biome ; le désert et les badlands ont des
  arbres. Le générateur Java ne lit ni `flora`, ni `tree_mix`, ni `nb_tree_density`.
* **Mobs** : 42 animaux ajoutés au registre après les trois d'origine, avec des conditions
  d'apparition facultatives (`spawnBiomes`, `spawnSurfaces`, `spawnMinY`, `spawnMaxY`,
  `spawnMaxWaterDepth`). Les trois mobs d'origine n'ont aucune condition et gardent le tirage
  d'apparition du Java ; en revanche le flux du générateur aléatoire d'apparition est partagé, donc
  les positions exactes des mobs d'origine ne sont plus celles du Java dès que les autres
  tentent d'apparaître (les tests Java n'activent que les mobs d'origine).
* **Apparition sur les feuillages** : un mob soumis à des conditions n'apparaît jamais sur des
  feuilles ni un cactus (le Java les y faisait apparaître) ; les trois mobs d'origine gardent
  ce comportement.
* **Prédation** : un hostile terrestre ou aquatique poursuit la proie passive la plus proche de
  son milieu (comportement Java, désormais visible avec les loups et les requins).

## Réseau local (phase 11)

Ajout sans équivalent Java, entièrement optionnel (voir `docs/LAN.md`). Effets sur le jeu solo :
aucun tant que « OPEN TO LAN » n'est pas utilisé. Parties en réseau : le menu pause propose
OPEN TO LAN / CLOSE LAN (mondes sauvegardés seulement) et LEAVE GAME pour un invité ; le monde
ne s'arrête plus quand pause ou inventaire sont ouverts (sinon l'hôte figerait ses amis) ; le
chat envoie le texte saisi aux autres joueurs (une ligne commençant par `/` reste une commande,
réservée à l'hôte) et garde 7 lignes visibles au lieu de 4.
