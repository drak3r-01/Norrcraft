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
| 10 | `saves/mobs_defaults.json` du Java : renne `aggressorDetectRange = 0`, `aggressorFleeSpeed = 0` (faux par rapport au code `ReindeerDefinition`) | 10 et 3, comme le code Java (seul le fichier de référence change ; le comportement est celui du Java) | `assets/mobs_defaults.json` ; `nb-data:mobs::builtin_order_and_values` |
| 11 | `groupSizeMax < groupSizeMin` ou rayon de spawn nul : exception `nextInt(≤ 0)` | aucun spawn pour ce mob + message, pas de panique | `nb-ecs:tests/mobs::far_mobs_despawn_and_degenerate_configs_do_not_panic` |
| 12 | `world.json` illisible : métadonnées neuves (graine 0) | la graine du monde est conservée | `nb-game` |

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
  `saves/<nom>/world.json` (`"formatVersion": 1`, serde) et `saves/<nom>/chunks/c_<cx>_<cz>.nbc`
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
