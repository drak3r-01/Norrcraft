# Spécification comportementale — crate `nb-gfx`

> Statut : normatif. Objectif : réimplémenter **à l'identique** le rendu du Norrblock Java (OpenGL 3.3 / LWJGL / JOML) en Rust + Vulkan (`ash`), **sans relire le Java**.
> Sources analysées (réf. lecture seule `Norrblock-Java-ref/repo`) : `graphics/MasterRenderer`, `ShadowRenderer`, `PostProcessingManager`, `MobRenderer`, `MobModel`, `MobModelLoader`, `ParticleRenderer`, `TextureRegistry`, `AssetManager`, `Texture`, `Shader`, `ShaderLibrary`, `Camera`, `ResourcePackMeta`, `Mesh`, `ChunkQuadtree`, `HighlightMesh`, `core/Window`, `core/GameLoop`, `core/Main`, les 16 shaders (`resources/shaders/*`), `docs/rendering.md`, `docs/resource-packs.md`. Points annexes vérifiés : `ChunkMesher` (encodage des couches), `Chunk` (dimensions), `GameSettings`, `WorldConfig`, `UIManager` (surbrillance), `CommandProcessor` (`/time`).
>
> Conventions de ce document : **[JAVA-BUG]** = comportement Java probablement non voulu mais **à reproduire** (sauf mention contraire) ; **[DÉVIATION]** = écart toléré/recommandé, à activer explicitement ; **[INFO]** = remarque sans impact.
> Les fichiers GLSL Java doivent être **copiés** dans `nb-gfx/shaders/` et servent de référence textuelle ; ce document décrit leur sémantique exacte et la liste fermée des modifications Vulkan (§18).

---

## 1. Périmètre et responsabilités

`nb-gfx` possède :

1. La création du contexte Vulkan, de la swapchain et des cibles de rendu (équivalents FBO).
2. Le chargement GPU des assets visuels : tableau de textures des blocs (« atlas »), tableau de textures des particules, texture de nuages, texture de bruit, textures et modèles de mobs, métadonnées de packs de ressources.
3. L'horloge jour/nuit (`timeOfDay`, `auroraPhase`) et le calcul pur des paramètres de ciel.
4. Le rendu d'une frame : ombres → scène HDR (terrain opaque, requêtes d'occlusion, fluides, mobs, particules) → bloom → post-traitement composite (ciel, brouillard, aurores, nuages, filtres, tonemapping) → surbrillance de bloc + overlay UI.
5. (Option, ou crate `nb-platform`) la fenêtre et la boucle fixe 60 Hz — décrites §3 car elles conditionnent le rendu.

`nb-gfx` **ne dépend pas** de `nb-world` : il reçoit des données brutes (sommets de chunks déjà maillés, instances de mobs, particules, état « œil dans un fluide », intensité de chaleur…). Voir §19.

Hors périmètre : maillage des chunks (`ChunkMesher`, dans nb-world/nb-mesh), UI 2D (nb-ui, mais qui dessine dans l'overlay fourni par nb-gfx), logique des particules/mobs.

---

## 2. Conventions mathématiques

### 2.1 Repère monde
- Main droite, **Y vers le haut**. Un bloc = 1 unité. Chunk = 16 (X) × **128** (Y) × 16 (Z) (`Chunk.SIZE_X/Y/Z`).
- Les sommets des chunks sont en **coordonnées monde absolues** (pas de matrice modèle pour le terrain).
- Mobs : face avant vers **+Z** à `yaw = 0`.

### 2.2 Matrices (JOML → glam)
JOML est colonne-majeure, uploadé sans transposition ; glam (`Mat4`) est aussi colonne-majeure → upload direct.

| Matrice | Java (JOML) | Équivalent glam (convention GL) |
|---|---|---|
| Projection caméra `P_gl` | `perspective(fovY_rad, aspect, 0.05, 1000)` (zZeroToOne = false) | `Mat4::perspective_rh_gl(fov_y, aspect, 0.05, 1000.0)` |
| Vue caméra `V` | `identity().rotateX(rad(pitch)).rotateY(rad(yaw)).translate(-pos)` | `Mat4::from_rotation_x(pitch.to_radians()) * Mat4::from_rotation_y(yaw.to_radians()) * Mat4::from_translation(-pos)` |
| Projection lumière `Lp_gl` | `ortho(-120,120,-120,120, 1, 400)` | `Mat4::orthographic_rh_gl(-120.,120.,-120.,120.,1.,400.)` |
| Vue lumière `Lv` | `lookAt(lightPos, camPos, (0,1,0))` | `Mat4::look_at_rh(lightPos, camPos, Vec3::Y)` |
| Modèle mob | `translation(x,y,z).rotateY(yaw).rotateX(pitch)` [`.scale(wX,h,wZ)` si modèle manquant] | `T * Ry(yaw) * Rx(pitch) [* S]` — **yaw/pitch en radians** (valeurs brutes de `TransformComponent`) |
| AABB occlusion | `translate(cx*16, 0, cz*16)` | `Mat4::from_translation(..)` |
| Surbrillance | `translate(target)` (coin min du bloc visé) | idem |

Caméra (`Camera`) :
- `position` initiale `(0, 100, 0)` ; `rotation.x = pitch` (degrés), `rotation.y = yaw` (degrés).
- `rotate(dx, dy)` : `pitch = clamp(pitch + dx, -89, 89)` ; `yaw += dy` (non borné).
- FOV **vertical**, en radians. Le constructeur reçoit 70°, mais `Main` applique aussitôt `GameSettings.fov` (défaut **90°**). `setFov` recalcule la projection avec l'aspect courant.
- `aspect = largeur / hauteur` du framebuffer ; initialement `1280/720`, recalculé uniquement lors d'un redimensionnement (§3.3).
- near = **0.05**, far = **1000**.
- À yaw = pitch = 0, la caméra regarde vers **−Z**.

### 2.3 Temps shader
`uTime` = `glfwGetTime()` (secondes depuis l'initialisation de GLFW, `double` converti en `f32`), lu **au moment de chaque appel** (ombre, scène et post-traitement le lisent séparément → valeurs différentes de quelques µs). En Rust : un seul `time_seconds: f32` par frame, mesuré depuis le démarrage de la plateforme ([DÉVIATION] négligeable, recommandée).

---

## 3. Fenêtre, boucle et ordre d'exécution

### 3.1 Fenêtre (`Window`)
- Taille initiale **1280 × 720**, titre **« Norrblock »**, redimensionnable, créée **invisible**, puis centrée sur le moniteur principal (`((modeW - w)/2, (modeH - h)/2)`), puis affichée.
- Icône : `/textures/ui/icon.png` chargée **sans** retournement vertical, RGBA8 ; ignorée si absente.
- VSync : activée (`swapInterval(1)`) à la création, puis `Main` applique `GameSettings.vsyncEnabled` (défaut **false** → `swapInterval(0)`). Vulkan : `FIFO` si vsync, sinon `IMMEDIATE` (ou `MAILBOX` à défaut, [DÉVIATION]), repli `FIFO`.
- Plein écran : **exclusif** sur le moniteur principal avec la résolution et le taux de rafraîchissement du mode vidéo courant ; avant le passage, mémorise position et taille fenêtrée ; au retour, restaure ces valeurs. Appel idempotent (no-op si déjà dans l'état demandé). Avec winit : `Fullscreen::Exclusive(mode courant)` ; à défaut `Borderless` ([DÉVIATION]).
- `setSize(w,h)` ignoré en plein écran. `Main` l'appelle si `GameSettings.windowWidth/Height ≠ 1280/720`.
- Callback de taille de **framebuffer** (pixels, pas points) : met à jour `width/height` et lève le drapeau `resized`.
- Entrées capturées par la fenêtre (à fournir par la plateforme) : position souris absolue, clic gauche (front montant, remis à faux à chaque `update()`), défilement vertical (dernier `yoffset`, remis à 0 à chaque `update()`).
- `update()` = `swapBuffers` → reset clic/scroll → `pollEvents`.
- États GL posés à l'init : `DEPTH_TEST` on, `CULL_FACE` on, `cullFace(BACK)`, front face CCW (défaut GL). Contexte 3.3 core forward-compatible. Framebuffer par défaut : non-sRGB (`GL_FRAMEBUFFER_SRGB` jamais activé), pas de MSAA.

### 3.2 Boucle fixe (`GameLoop`)
```
timeStep = 1/60 s
last = now(); acc = 0
tant que !fenêtre.doitFermer():
    cur = now(); frame = cur - last; last = cur
    acc += frame
    tant que acc >= timeStep: update(timeStep); acc -= timeStep
    render()
    fenêtre.update()     // present + reset entrées + poll events
```
- **Aucun clamp** de `frame` ni de `acc` dans le Java (pas de protection « spirale de la mort »), **aucune interpolation** au rendu (l'alpha résiduel n'est pas utilisé). Reproduire tel quel. [DÉVIATION optionnelle] : clamp de `frame` à 0.25 s, désactivé par défaut.
- `now()` = horloge monotone en secondes `f64`.

### 3.3 Ordre `Main`
Init : fenêtre → registres blocs/mobs → caméra → `AssetManager.init()` (§5) → UI → monde menu → `MasterRenderer.init(w,h,assets)` → application des réglages (fov, vsync, plein écran, qualité d'ombre, bloom, taille de fenêtre, pack si ≠ « Default » via `reloadBlockTextures` + re-génération des icônes de blocs) → état `MAIN_MENU`.

`update(dt)` (chaque tick) :
1. entrées ; 2. **`renderer.updateTime(dt)`** (toujours, y compris au menu/pause) ; 3. chat ;
4. si `resized` : `renderer.handleResize(w,h)` (viewport + recréation des cibles post-process, **pas** de la shadow map) ; `camera.updateProjectionMatrix(w/h)` ; `resized = false` ;
5. état courant ; 6. monde ; 7. mise à jour des maillages.

`render()` : `état.render()` (qui appelle `MasterRenderer.render(...)` puis l'UI — états `PLAYING`, `PAUSED`, `INVENTORY`, `MAIN_MENU`) puis rendu du chat.

[JAVA-BUG] Le redimensionnement n'est traité que dans un tick ; une frame rendue sans tick intermédiaire utilise des cibles à l'ancienne taille. [JAVA-BUG] Fenêtre minimisée (0×0) : aspect NaN, textures 0×0. En Vulkan : **recréer la swapchain et les cibles dans le chemin de rendu** dès que l'extent change, et **sauter le rendu** si l'extent vaut 0 ([DÉVIATION] obligatoire). La projection doit suivre l'aspect de l'extent effectivement rendu.

---

## 4. Ressources GPU persistantes

| Ressource | Taille | Format GL → Vulkan | Filtre / wrap | Contenu |
|---|---|---|---|---|
| Shadow map | N×N, N = 4096 (HIGH) / 1024 (LOW) | `GL_DEPTH_COMPONENT` (24 bits typ.) → `D32_SFLOAT` | NEAREST, CLAMP_TO_BORDER, bordure **blanche (1,1,1,1)** → `FLOAT_OPAQUE_WHITE` | profondeur lumière |
| HDR color0 `colorBuffer` | W×H | `RGBA16F` → `R16G16B16A16_SFLOAT` | LINEAR, CLAMP_TO_EDGE | couleur scène |
| HDR color1 `brightBuffer` | W×H | `RGBA16F` | LINEAR, CLAMP_TO_EDGE | source bloom |
| HDR depth `depthBuffer` | W×H | `DEPTH_COMPONENT24` → `D32_SFLOAT` (ou `D24_UNORM_S8_UINT` si supporté en sampled) | NEAREST, CLAMP_TO_EDGE | profondeur scène, échantillonnée |
| Ping-pong ×2 | (W/2)×(H/2) (division entière) | `RGBA16F` | LINEAR, CLAMP_TO_EDGE | flou |
| Bruit | 256×256 | `R8` → `R8_UNORM` | LINEAR, REPEAT, sans mip | octets aléatoires uniformes `[0,255)` (`Math.random()*255` tronqué), non déterministe |
| Blanc 1×1 | 1×1 | RGBA8 `FFFFFFFF` | NEAREST | mobs en mode couleur |
| Damier mob manquant | 16×16 | RGBA8 | NEAREST (wrap REPEAT par défaut), sans mip | magenta `(255,0,255,255)` si `((x/4)+(y/4))` pair, sinon noir `(0,0,0,255)` |
| Atlas blocs | §5.1 | `GL_RGBA` (non sRGB) → `R8G8B8A8_UNORM` | §5.1 | |
| Atlas particules | §5.4 | idem | idem | |
| Nuages | 512×512 (`clouds.png`) | RGBA8 UNORM | NEAREST/NEAREST, REPEAT, mips générés mais inutilisés | |
| Quad plein écran | 4 sommets strip | `[x,y,z,u,v]` : `(-1,1,0, 0,1)`, `(-1,-1,0, 0,0)`, `(1,1,0, 1,1)`, `(1,-1,0, 1,0)` | | |
| AABB chunk | 36 sommets (12 triangles) | boîte `[0,16]×[0,128]×[0,16]` | | |

Toutes les textures RGBA chargées depuis PNG sont décodées en RGBA8 **avec retournement vertical** (`stbi_set_flip_vertically_on_load(true)`) : la première ligne mémoire est la **dernière ligne** du PNG. **Le port Rust doit conserver ce retournement** (`image::imageops::flip_vertical`) : toutes les UV (blocs, mobs « vGL = 1 − v_mc/th », particules, nuages) en dépendent. Exception : l'icône fenêtre (non retournée).

Aucune texture n'est en espace sRGB : les shaders font eux-mêmes `pow(c, 2.2)` / `pow(c, 1/2.2)`. **La swapchain doit être UNORM** (`B8G8R8A8_UNORM`), jamais `_SRGB` (sinon double gamma).

`setShadowQuality` : `LOW → 1024`, `HIGH → 4096`, `OFF → ne change pas la résolution` (la passe d'ombre est simplement sautée). `setResolution` réalloue la texture si la taille diffère. Réglage par défaut (`GameSettings`) : **LOW**. Le `MasterRenderer` démarre à HIGH/4096 puis `Main` applique LOW.

---

## 5. Assets

### 5.1 Tableau de textures des blocs (`TextureRegistry.loadDirectory`)
Entrée : un répertoire de PNG.
1. Lister les fichiers dont le nom se termine (insensible à la casse) par `.png`. Ordre Java = ordre de `File.listFiles()` (NTFS : alphabétique). **Rust : trier par nom de fichier** (comparaison ordinale des noms en majuscules, compatible NTFS) pour un ordre déterministe. L'index de couche n'est observable qu'indirectement (icônes, mapping) ; seul le mapping nom→couche compte.
2. Si aucun fichier : **texture de repli** = tableau 1×1×1 magenta `(255,0,255,255)`, NEAREST/NEAREST, sans mip ; couche « manquante » = 0 ; fin.
3. Taille de tuile : `W = H = max(largeur)` parmi les images décodables (16 si aucune). Ex. pack Default : toutes les images font 32 de large → tuile **32×32**.
4. Allouer `layerCount + 1` couches ; la dernière (`index = layerCount`) est la **couche manquante**.
5. Paramètres : MIN = `NEAREST_MIPMAP_LINEAR`, MAG = `NEAREST`, wrap S/T = `REPEAT` (les UV des faces fusionnées vont au-delà de 1), anisotropie = `min(8, max supporté)` si l'extension existe. Chaîne de mips complète générée après upload (`glGenerateMipmap` : moyenne 2×2 box, sur les données retournées). En Vulkan : `vkCmdBlitImage` LINEAR niveau par niveau, ou génération CPU box 2×2 ([DÉVIATION] mineure acceptée).
6. Pour chaque fichier `i` (couche `i`), nom clé = `nomFichier.replace(".png","").toLowerCase()` (remarque : le `replace` est sensible à la casse, `Foo.PNG` → clé `foo.png` [JAVA-BUG]) :
   - Si le décodage échoue : la couche reste non initialisée (Vulkan : la remplir de zéros) et **le nom n'est pas enregistré**.
   - Si `imgW == W && imgH >= H` : copier les `W×H` premiers pixels du buffer **retourné** → pour une bande animée verticale (`water_still` 32×1024, `lava_still` 32×640) c'est la **dernière frame du PNG** (bas de l'image) qui est utilisée.
   - Sinon : `srcH = min(imgH, imgW)` ; redimensionner les `srcH` premières lignes du buffer retourné (`imgW × srcH`) vers `W×H` avec `stbir_resize_uint8` (filtres par défaut stb v1 : Catmull-Rom en agrandissement, Mitchell en réduction, bords clamp, sans prémultiplication alpha, espace linéaire). Rust : `fast_image_resize` CatmullRom / Mitchell, ou `image::imageops::resize(CatmullRom)` ([DÉVIATION] sous-pixel acceptée).
7. Couche manquante : damier `tile = max(1, W/4)` ; magenta `(255,0,255,255)` si `((x/tile)+(y/tile))` pair, sinon `(0,0,0,255)`.
8. `getLayer(nom)` : recherche insensible à la casse ; absent → couche manquante.

### 5.2 Packs de ressources (`AssetManager`, `ResourcePackMeta`)
Racine des packs `packsRoot()` (premier existant) : `./resource_packs`, `../resource_packs`, `<dossier_du_binaire>/../resource_packs` ; sinon `resource_packs` (relatif).

**Init** :
1. Atlas blocs ← `Default/assets/minecraft/textures/block`.
2. Méta active ← `pack.mcmeta` de `Default` (sinon méta par défaut `("Default", "Built-in textures")`).
3. Mapping blocs → couches (fait par l'appelant : `BlockRegistry.initializeMapping(atlas)`).
4. Atlas particules ← classpath `/textures/particle` noms `["leaf", "lava_drop"]` (§5.4).
5. Nuages ← `/textures/environment/clouds.png` (§4).
6. Registre de particules (`/particles.json`) puis mapping texture → couche (`leaf`→0, `lava_drop`→1).
7. Modèles de mobs ← `MobModelLoader.loadAll(packRoot = Default, defaultRoot = Default)`.

**`reloadBlockTextures(nom)`** (nom vide/null → « Default ») :
1. Détruire l'atlas précédent.
2. Si `<pack>/assets/minecraft/textures/block` est un dossier → le charger ; méta = `pack.mcmeta` du pack sinon `(nom, "")`.
   Sinon si `<pack>/textures/block` (layout plat) → idem.
   Sinon → charger l'atlas du Default ; méta = `("Default","Built-in textures")` (pas la méta lue du disque).
3. Remapper les blocs (appelant). **Aucun repli texture par texture** vers Default : une texture absente du pack → couche magenta.
4. Détruire et recharger tous les modèles de mobs avec `packRoot = <pack>`, `defaultRoot = Default`.
5. L'appelant doit invalider **tous les maillages de chunks** (les index de couches changent) et régénérer les icônes de blocs de l'UI.
L'atlas des particules et la texture de nuages ne dépendent jamais du pack.

**`reloadMobModel(id)`** : recharge un seul modèle avec les racines actives.

**`listResourcePacks()`** : sous-dossiers de la racine ; « Default » en premier, les autres triés alphabétiquement insensible à la casse ; chaque entrée = méta lue ou `(nomDossier, "")` ; si vide → `[("Default","Built-in textures")]`.

**`pack.mcmeta`** (`ResourcePackMeta.load(dossier)`) :
- Fichier absent → `None`.
- Nom affiché = **nom du dossier** (jamais lu du JSON). `pack_format` ignoré.
- Description : parseur minimal — première occurrence de `"description"`, puis `:`, espaces, et doit commencer par `"` ; lecture jusqu'au `"` fermant ; un `\x` est remplacé par `x` (donc `\n` → `n`, pas de décodage `\uXXXX`). Si la valeur n'est pas une chaîne (objet/tableau texte JSON) → `""`. Erreur d'E/S → `(nomDossier, "")`.
- Rust : reproduire ce parseur tel quel (pas `serde_json`, qui changerait le résultat des échappements et des descriptions non-chaînes).

### 5.3 Texture de nuages
512×512, retournée verticalement, échantillonnée en NEAREST avec `uv = floor(p/12)/256` et REPEAT (§13.6). [INFO] Avec 512 texels et un pas de 1/256, l'échantillon tombe exactement sur une frontière de texel ; la règle NEAREST (`floor(u·512)`) choisit le texel pair `2k`. Pour garantir ce choix en Vulkan, échantillonner à `(gridPos + 0.25)/256` ([DÉVIATION] de robustesse, résultat identique à la règle idéale).

### 5.4 Tableau de textures des particules (`loadFromClasspath`)
Même algorithme que §5.1 avec une liste ordonnée de noms : `leaf` (10×10) → couche 0, `lava_drop` (16×16) → couche 1, couche manquante = 2. Tuile = 16×16 (max des largeurs) → `leaf` est **agrandie** 10→16 (Catmull-Rom). Une ressource absente ne fait pas planter (couche vide, nom non enregistré). Mêmes filtres/mips/anisotropie.

---

## 6. Déroulé exact d'une frame (`MasterRenderer.render`)

Entrées : fenêtre (W,H), caméra, joueur (`fluidAtEyeLevel`), monde (biome sous la caméra, blocs pour la chaleur, entités, particules), `chunkMeshes: Map<ChunkPos, Mesh>`.

```
0. enable DEPTH_TEST
1. quadtree.build(chunkMeshes)                         (§11.1)
2. P = camera.proj (P_gl), V = camera.view
   visibleChunks = quadtree.query(P*V)
3. Paramètres de ciel depuis timeOfDay                  (§7)
4. Couleur de brouillard/fond (r,g,b) + fluidState      (§8.1)
5. heatIntensity                                         (§8.2)
6. shadowLightDir = lightDir ; si shadowLightDir.y > 0 : shadowLightDir = -shadowLightDir
   lightPos = camPos + shadowLightDir * (-150)
   lightSpace = Lp_gl * lookAt(lightPos, camPos, +Y)
   shadowVisibleChunks = quadtree.query(lightSpace)
7. si shadowQuality != OFF : passe d'ombre             (§10)
8. passe scène dans le FBO HDR                          (§6.1)
9. mobs (FBO HDR toujours lié)                          (§14.1)
10. particules (FBO HDR)                                (§15)
11. bloom                                               (§12)
12. composite post-process → écran                      (§13)
13. blit profondeur HDR → profondeur écran (NEAREST)
--- ensuite, hors MasterRenderer ---
14. UIManager.render : disable DEPTH_TEST ; si état ∈ {PLAYING, PAUSED, INVENTORY} et bloc visé :
    surbrillance (§16, ré-active DEPTH_TEST + depthFunc LEQUAL) ; puis éléments UI 2D.
15. present
```

### 6.1 Passe scène (`renderScene`)
Cible : FBO HDR, draw buffers `{color0, color1}`, viewport `(0,0,W,H)`.

1. `clearColor(r,g,b,1)` ; clear couleur + profondeur. **Les deux attachements couleur sont effacés à `(r,g,b,1)`** (glClear agit sur tous les draw buffers) — y compris `brightBuffer` : le fond du ciel contient donc la couleur de brouillard dans la source du bloom (§17). Profondeur effacée à 1.0.
2. **Terrain opaque** (`uRenderPass = 0`) : shader `main`, blend **off**, depth write **on**, depth test on, cull **BACK**. Textures : unité 0 = atlas blocs, unité 1 = shadow map. Pour chaque chunk de `visibleChunks` (ordre de la requête quadtree) : dessiné si `lastQueryResults[pos]` vaut vrai (défaut **vrai** si jamais mesuré).
3. **Requêtes d'occlusion** : shader `hl`, cull **désactivé**, `colorMask(false×4)`, depth write **off**, depth test on. Pour **chaque** chunk de `visibleChunks` (même ceux non dessinés) : `model = translate(cx*16, 0, cz*16)`, `beginQuery(SAMPLES_PASSED)`, dessiner l'AABB (36 sommets), `endQuery`.
   Puis `colorMask(true×4)`, ré-activer `CULL_FACE`.
   Puis pour chaque chunk visible : si le résultat de **sa** requête est disponible (`QUERY_RESULT_AVAILABLE`, sans attente), `lastQueryResults[pos] = (samples > 0)`. En pratique les résultats de la frame courante sont rarement prêts → la visibilité utilise des résultats d'une ou plusieurs frames précédentes.
4. **Fluides** (`uRenderPass = 1`) : shader `main`, blend **on** `SRC_ALPHA, ONE_MINUS_SRC_ALPHA` (RGB et alpha, appliqué à **chaque** attachement avec l'alpha **de cet attachement**), depth write **off** (resté off depuis l'étape 3), depth test on, cull BACK. Unité 2 = `depthBuffer` HDR (celui-là même qui est attaché, lecture seule de fait) → `opaqueDepth` ; `uScreenSize = (W,H)` (déclaré, inutilisé). Mêmes chunks que l'étape 2 (filtre `lastQueryResults` **mis à jour** à l'étape 3).
5. depth write **on**, blend **off**.

Note d'ordre : les fluides sont dessinés **avant** les mobs et particules, sans écrire la profondeur → un mob sous l'eau est dessiné **par-dessus** la surface de l'eau (pas d'atténuation). À reproduire.

### 6.2 États GL résiduels (à reproduire comme états de pipeline)
- Cull : toujours activé, `BACK`, front = CCW, sauf : passe d'ombre (`FRONT`), requêtes d'occlusion (aucun). Les quads plein écran sont CCW → visibles.
- Depth func : `LESS` (défaut GL) **jusqu'au premier affichage de la surbrillance**, puis `LEQUAL` pour toute la suite du programme (jamais restauré). Recommandation : `LESS_OR_EQUAL` pour toutes les pipelines 3D ([DÉVIATION] invisible hors z-fighting).
- `GL_SAMPLE_ALPHA_TO_COVERAGE` activé mais **aucun** MSAA → sans effet. Ne pas l'activer en Vulkan (`rasterizationSamples = 1`).
- Largeur de ligne : 1.0.

---

## 7. Cycle jour/nuit

### 7.1 Horloge
- `timeOfDay ∈ [0,1200)` (« ticks »), défaut **900** (midi) ; `dayLengthSeconds` défaut **1200** (`WorldConfig`), `setDayLengthSeconds(s) = max(1, s)`.
- `updateTime(dt)` (chaque tick fixe) : `timeOfDay = (timeOfDay + dt * 1200/dayLengthSeconds) % 1200` ; `auroraPhase += dt * 0.015` (jamais borné).
- `setTime(t)` : `t % 1200`, puis `+1200` si négatif.
- Commandes `/time` (référence) : `sunset = 0`, `night = 150`, `midnight = 300`, `sunrise = 600`, `day = 750`, `noon = 900`, ou valeur numérique.
- Persisté par la sauvegarde du monde (`WorldConfig.timeOfDay`), restauré au chargement avec `dayLengthSeconds`.

### 7.2 Direction de la lumière
```
angle    = timeOfDay / 1200 * 2π
lightDir = normalize(cos(angle), sin(angle), -0.3)
sunHeight = -lightDir.y
```
`lightDir` est le sens de propagation (du soleil vers le sol) : à midi (900) `angle = 3π/2`, `lightDir ≈ (0, -0.95783, -0.28735)`.
`shadowLightDir` = `lightDir`, inversé si `lightDir.y > 0` (la nuit : la « lune » éclaire depuis la position opposée). C'est **`shadowLightDir`** qui est envoyé comme `uLightDir` aux shaders `main` et `pp`.

### 7.3 Couleurs (lerp `a + (b-a)·t`)
| Grandeur | Jour, `sunHeight > 0.1` ; `t = min(1, (h-0.1)/0.5)` : lerp(A→B) | Crépuscule, `-0.1 < h ≤ 0.1` ; `t = (h+0.1)/0.2` : lerp(A→B) | Nuit, `h ≤ -0.1` ; `t = min(1, (-h-0.1)/0.5)` : lerp(A→B) |
|---|---|---|---|
| `skyZenith` | (0.8,0.4,0.4) → (0.12,0.32,0.75) | (0.05,0.1,0.2) → (0.8,0.4,0.4) | (0.05,0.1,0.2) → (0.02,0.04,0.08) |
| `skyHorizon` | (0.9,0.6,0.3) → (0.65,0.8,0.95) | (0.1,0.2,0.3) → (0.9,0.6,0.3) | (0.1,0.2,0.3) → (0.03,0.06,0.12) |
| `sunCol` | (1.8,0.8,0.4) → (3.0,2.4,1.8) | (0.5,0.5,0.8) → (1.8,0.8,0.4) | constante (0.5,0.5,0.8) |
| `baseFog` | (0.8,0.5,0.3) → (0.45,0.65,0.9) | (0.05,0.1,0.15) → (0.8,0.5,0.3) | (0.05,0.1,0.15) → (0.02,0.04,0.08) |
| `ambientColor` | (0.3,0.25,0.25) → (0.4,0.4,0.4) | (0.1,0.1,0.15) → (0.3,0.25,0.25) | (0.1,0.1,0.15) → (0.05,0.05,0.08) |
| `lightColor` | (0.5,0.3,0.2) → (0.6,0.6,0.6) | (0.1,0.15,0.2) → (0.5,0.3,0.2) | (0.1,0.15,0.2) → (0.05,0.08,0.1) |
| `auroraIntensity` | 0 | 0 | `t` |

Fonctions continues aux frontières. Implémenter comme fonction pure `sky_params(time_of_day) -> SkyParams`.

### 7.4 Vecteurs de test
| timeOfDay | lightDir (≈) | uLightDir envoyé | branche | zenith | horizon | sunCol | ambient | light | aurora |
|---|---|---|---|---|---|---|---|---|---|
| 900 | (0, −0.95783, −0.28735) | idem | jour t=1 | (0.12,0.32,0.75) | (0.65,0.8,0.95) | (3,2.4,1.8) | (0.4,0.4,0.4) | (0.6,0.6,0.6) | 0 |
| 0 | (0.95783, 0, −0.28735) | idem (y=0 non inversé) | crépuscule t=0.5 | (0.425,0.25,0.3) | (0.5,0.4,0.3) | (1.15,0.65,0.6) | (0.2,0.175,0.2) | (0.3,0.225,0.2) | 0 |
| 300 | (0, 0.95783, −0.28735) | (0, −0.95783, 0.28735) | nuit t=1 | (0.02,0.04,0.08) | (0.03,0.06,0.12) | (0.5,0.5,0.8) | (0.05,0.05,0.08) | (0.05,0.08,0.1) | 1 |

(baseFog à 900 : (0.45,0.65,0.9) ; à 0 : (0.425,0.3,0.225) ; à 300 : (0.02,0.04,0.08).)

---

## 8. Entrées d'environnement

### 8.1 Fluide à hauteur d'œil
`fluid = player.fluidAtEyeLevel` :
- `WATER` → `fluidState = 1`, `(r,g,b) = waterColor` du biome à `(camX, camZ)` (valeurs `double` du `BiomeConfig`, en sRGB « brut ») ;
- `LAVA` → `fluidState = 2`, `(r,g,b) = (0.8, 0.1, 0.0)` ;
- sinon `fluidState = 0`, `(r,g,b) = baseFog`.

`(r,g,b)` sert de couleur d'effacement HDR (§6.1) et d'uniforme `uFogColor` (utilisé seulement si `fluidState ≠ 0`). [INFO] Hors fluide, le fond effacé n'est jamais visible : le post-process remplace le ciel (facteur de brouillard ≈ 0 à `dist = 10000`).

### 8.2 Chaleur (lave proche)
```
cam = position caméra ; (cx,cy,cz) = floor(cam)
minDistSq = 256
pour x,y,z ∈ [-8, 8]³ :
    b = bloc(cx+x, cy+y, cz+z)    // y hors [0,255] ou chunk non chargé → AIR ; y ≥ 128 → AIR (Chunk hors bornes)
    si b == LAVA : d² = |(bx+0.5, by+0.5, bz+0.5) - cam|² ; minDistSq = min(minDistSq, d²)
heat = 0
si minDistSq < 64 : t = clamp(1 - sqrt(minDistSq)/8, 0, 1) ; heat = t*t*(3 - 2t)
```
Ce balayage dépend du monde : il est fait **par l'appelant** (nb-world/app) ; nb-gfx expose la fonction pure `heat_from_nearest_lava_dist_sq(min_dist_sq: f32) -> f32`.

---

## 9. Shaders — interfaces et uniformes

Formats de sommets :
- **Chunk** (`Mesh`, 9 floats = 36 o, `TRIANGLES` non indexés) : loc0 `vec3 aPos` (monde), loc1 `vec2 aTexCoord` (unités de tuile, peut dépasser 1), loc2 `float aLayer` (couche encodée), loc3 `vec3 aColor` (teinte × lumière, calculée par le mailleur).
- Encodage `aLayer` (par `ChunkMesher`) : fluide → `-layer - 1` (< 0) ; feuilles teintées → `layer + 200000` ; plantes en croix → `layer + 100000` ; sinon `layer`.
- **Mob** (10 floats = 40 o) : loc0 `vec3 aPos`, loc1 `vec2 aUV`, loc2 `vec4 aTint`, loc3 `float aBrightness`.
- **Particule / quad** (5 floats) : loc0 `vec3`, loc1 `vec2`.
- **hl** : loc0 `vec3`.

### 9.1 `main.vert` / `main.frag` (terrain)
Uniformes :

| Nom | Type | Valeur |
|---|---|---|
| `projection` | mat4 | `P_gl` (vert et frag) |
| `view` | mat4 | `V` |
| `lightSpaceMatrix` | mat4 | `Lp_gl * Lv` |
| `uTime` | float | temps (s) |
| `uCameraPos` | vec3 | position caméra |
| `uLightDir` | vec3 | `shadowLightDir` |
| `uAmbientColor` | vec3 | `ambientColor` |
| `uLightColor` | vec3 | `lightColor` |
| `textureArray` | sampler2DArray | unité 0, atlas blocs |
| `shadowMap` | sampler2D | unité 1 |
| `opaqueDepth` | sampler2D | unité 2 (passe 1 seulement) |
| `uScreenSize` | vec2 | (W,H) — inutilisé |
| `uRenderPass` | int | 0 opaque / 1 fluides |

**Vertex** : décodage de `aLayer` puis animation :
- `aLayer < 0` (fluide) : `layer = -aLayer - 1`, `IsFluid = 1`. Si `fract(pos.y) > 0.7` (surface) :
  - si `aColor.r > 0.6 && aColor.b < 0.2` (lave, détectée par la teinte du sommet) : `pos.y += sin(x*0.8 + t*0.3) * cos(z*0.8 + t*0.3) * 0.06` ;
  - sinon (eau) : `pos.y += sin(x*2.5 + t*2.0)*cos(z*2.5 + t*2.0)*0.2 + sin(x*1.25 - t*1.6)*sin(z*1.75 + t*2.2)*0.1`.
- `aLayer >= 200000` (feuilles) : `layer = aLayer - 200000`, `IsLeaf = 1` ; `w = t*1.2` ; `sway = sin(0.5x + 0.2y + 0.5z + w)*0.05` ; `rustle = sin(3x + 2y + 3z + 2.5w)*0.015` ; `pos.x += sway+rustle ; pos.y += rustle*0.5 ; pos.z += sway+rustle`.
- `aLayer >= 100000` (croix) : `layer = aLayer - 100000` ; si `aTexCoord.y > 0.5` (sommets hauts) : `w = t*1.5` ; `sway = sin(1.5x + 0.5y + 1.5z + w)*0.1` ; `pos.x += sway ; pos.z += sway`.
- Sorties : `TexCoord`, `Layer`, `Color = aColor`, `FragPos = pos` (déplacé), `FragPosLightSpace = lightSpaceMatrix * pos`, `IsFluid`, `IsLeaf` ; `gl_Position = projection * view * pos`.

**Fragment** (sorties loc0 `FragColor`, loc1 `BrightColor`) :
1. Passe 0 : `discard` si fluide ; passe 1 : `discard` si non fluide.
2. `tex = texture(textureArray, (TexCoord, Layer))` (couche arrondie au plus proche et bornée).
3. Passe 0 : `discard` si `tex.a < 0.01` ; `tex.rgb = clamp(tex.rgb / tex.a, 0, 1)` ; `alpha = (tex.a - 0.5)/max(fwidth(tex.a), 0.0001) + 0.5` ; `discard` si `alpha < 0.5`.
   Passe 1 : `alpha = tex.a` ; `discard` si `< 0.05`.
4. `lin = pow(tex.rgb, 2.2)`.
5. Normale géométrique : `n = normalize(cross(dFdx(FragPos), dFdy(FragPos)))` ; `viewDir = normalize(uCameraPos - FragPos)` ; si `dot(n, viewDir) < 0` : `n = -n`.
6. Ombre (`ShadowCalculation`) : `p = FragPosLightSpace.xyz / w * 0.5 + 0.5` ; si `p.z > 1` → 0 ; `bias = max(0.005*(1 - dot(n, -uLightDir)), 0.001)` ; PCF 3×3 (décalages entiers × `1/textureSize`) : compte `p.z - bias > texture(shadowMap, p.xy + o).r` ; `shadow = somme/9`. Hors de la carte, la bordure blanche (1.0) → non ombré.
7. `final = lin * Color * (uAmbientColor + uLightColor*(1 - shadow))`.
8. Feuilles : `back = max(0, dot(viewDir, normalize(-uLightDir)))` ; `sss = back³ * 0.6` ; `wrap = max(0, dot(n, uLightDir)*0.5 + 0.5)` (signe **+uLightDir**, tel quel) ; `final += lin*Color*uLightColor*sss*wrap*(1 - 0.7*shadow)`.
9. `isLava = IsFluid && lin.r > 0.6 && lin.b < 0.2` ; `spec = 0`.
10. Fluides :
    - `opDepth = texelFetch(opaqueDepth, ivec2(gl_FragCoord.xy), 0).r` ; `lin(d) = projection[3][2] / ((d*2-1) + projection[2][2])` (profondeur linéaire positive) ; `thickness = max(lin(opDepth) - lin(gl_FragCoord.z), 0)`.
    - `spec = pow(max(dot(viewDir, reflect(normalize(uLightDir), n)), 0), 48)`.
    - `fresnel = 0.02 + 0.98*(1 - max(dot(viewDir,n),0))^5`.
    - `density = isLava ? 1.5 : 0.08` ; `depthAlpha = 1 - exp(-thickness*density)`.
    - `alpha = clamp(mix(depthAlpha, 1, fresnel), isLava ? 0.9 : 0.3, 0.98)`.
    - `deep = lin*Color*(isLava ? (2.5,1.0,0.1) : 0.25)` ; `shallow = lin*Color*(isLava ? (3.0,1.5,0.2) : 1.0)` ; `final = mix(shallow, deep, depthAlpha) + uLightColor*2*spec*(1 - shadow)`.
11. `FragColor = (final, alpha)`.
    `BrightColor` = `(FragColor.rgb*1.8, 1)` si lave ; sinon `(FragColor.rgb, 1)` si `spec > 0.2` ou (`IsLeaf` et `final.g > 1.2`) ; sinon `(0,0,0,1)`. **Il n'y a pas de seuil de luminance** : la source du bloom est cette sélection.

### 9.2 `shadow.vert` / `shadow.frag`
Uniformes : `lightSpaceMatrix` (mat4), `uTime` (float), `textureArray` (unité 0, atlas blocs).
Vertex : mêmes animations que `main.vert` **avec ces différences** :
- fluide (`aLayer < 0`) : toujours la vague de l'**eau** (pas de branche lave) ;
- seuils **2000** (feuilles, `layer = aLayer - 2000`) et **1000** (croix, `layer = aLayer - 1000`) au lieu de 200000/100000. **[JAVA-BUG]** Comme le mailleur encode +100000/+200000, croix **et** feuilles tombent dans la branche « ≥ 2000 » : elles reçoivent l'ondulation des feuilles et la couche `aLayer - 2000` (≥ 98000) qui est **bornée à la dernière couche = couche manquante opaque** → le test alpha ne rejette jamais → feuilles et plantes projettent des ombres **pleines** (quads entiers). Reproduire à l'identique (garder les seuils 1000/2000 et le clamp de couche) ; correctif éventuel derrière un drapeau.
- `gl_Position = lightSpaceMatrix * pos`.
Fragment : `discard` si `texture(textureArray,(uv,layer)).a < 0.5` ; aucune sortie couleur.
Les fluides sont aussi dessinés dans la carte d'ombre (avec ce test alpha).

### 9.3 `hl.vert` / `hl.frag`
Uniformes `projection`, `view`, `model` (mat4). Sortie `(0,0,0,1)`. Utilisé pour les AABB d'occlusion (couleur masquée) et la surbrillance de bloc (lignes noires).

### 9.4 `blur.vert` / `blur.frag`
Vertex : passe-plat `gl_Position = aPos`, `TexCoords = aTexCoords`. Uniformes : `image` (sampler2D, unité 0), `horizontal` (bool), `weight[5] = {0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216}` (initialiseur d'uniforme ; en Vulkan → `const`).
`off = 1/textureSize(image,0)` (**taille de la texture lue**, donc pleine résolution à la première itération) ; `r = tex(uv)*w0 + Σ_{i=1..4} (tex(uv ± i·off_axe))*w_i` ; sortie `(r, 1)`.

### 9.5 `pp.vert` / `pp.frag` — voir §13.

### 9.6 `mob.vert` / `mob.frag`
Uniformes : `model`, `view`, `projection` (mat4), `uTexture` (sampler2D, unité 0).
`FragColor = texture(uTexture, vUV) * vTint * vec4(vB, vB, vB, 1)` ; `discard` si `alpha < 0.01`. **Aucun éclairage, ombre, cycle jour/nuit ni conversion gamma** : texels sRGB utilisés tels quels comme couleur linéaire HDR. À reproduire. Ne déclare **que** la sortie location 0.

### 9.7 `particle.vert` / `particle.frag`
Uniformes : `model`, `view`, `projection` (mat4), `textureArray` (sampler2DArray, unité 0, atlas particules), `layer` (int), `colorTint` (vec3).
`tex = texture(textureArray, (uv, layer))` ; `discard` si `a < 0.1` ; `FragColor = (tex.rgb*colorTint, tex.a)`. Seule la sortie location 0.

### 9.8 `cloud.vert` / `cloud.frag` — **code mort**
Déclarés dans `ShaderLibrary` mais jamais compilés ni utilisés : les nuages sont rendus dans `pp.frag`. **Ne pas porter.** (Pour mémoire : volume de nuages par rayon sur un plan, brouillard `density 0.005`, `exp(-(max(0,d-24)*0.005)^2.5)`, alpha 0.9.)

---

## 10. Passe d'ombre (`ShadowRenderer.render`)
- Cible : shadow map N×N (depth only, pas de couleur), viewport `(0,0,N,N)`, clear profondeur (1.0).
- Cull **activé, FRONT** (restauré à BACK après). Depth test on (LESS/LEQUAL), depth write on. Pas de depth bias matériel (le biais est dans `main.frag`).
- Pour chaque chunk de `shadowVisibleChunks` (requête quadtree avec `lightSpace`) : dessin, **sans** filtre d'occlusion.
- Paramètres : ortho ±120 × ±120, near 1, far 400, oeil à 150 unités de la caméra en remontant la lumière, cible = caméra, up = +Y. **Pas** de stabilisation texel (la carte « nage » quand la caméra bouge) — à reproduire.
- Si `OFF` : la passe n'est pas exécutée mais la carte **reste échantillonnée** dans `main.frag`. **[JAVA-BUG]** Elle contient alors l'image de la dernière frame rendue avec ombres (ombres figées) ou, si OFF dès le lancement, un contenu non initialisé (souvent 0 → tout ombré). Recommandation : effacer la carte à 1.0 au passage à OFF et au démarrage ([DÉVIATION] recommandée ; documenter).
- Changement de résolution : réallouer la carte (et son framebuffer en Vulkan).

---

## 11. Culling

### 11.1 Quadtree (`ChunkQuadtree`), reconstruit à chaque frame
- Vide → racine nulle (aucun chunk visible).
- Racine = AABB XZ englobant tous les chunks (`[cx*16, cx*16+16]`).
- Insertion : si le nœud est une feuille avec **< 16** éléments → ajouter ; sinon subdiviser (si feuille) puis descendre selon le centre du chunk `(cx*16+8, cz*16+8)` : quadrant 0 = (x<midX, z<midZ), 1 = (x≥midX, z<midZ), 2 = (x<midX, z≥midZ), 3 = (x≥midX, z≥midZ). À la subdivision, les éléments existants sont redistribués **dans les enfants sans récursion** (un enfant peut dépasser 16).
- Requête : si l'AABB du nœud `[minX,0,minZ]–[maxX,128,maxZ]` échoue au test de frustum → abandon ; nœud interne → enfants 0..3 dans l'ordre ; feuille → tester chaque chunk `[cx*16,0,cz*16]–[+16,128,+16]` et ajouter si positif. L'ordre de sortie (= ordre de dessin) est celui de ce parcours.
- Test de frustum = `Matrix4f.testAab` de JOML : extraction des 6 plans de la matrice `M` (convention GL, z ∈ [-w,w]), test du sommet positif (p-vertex) ; conservatif (faux positifs possibles). Utiliser **`P_gl * V`** et **`Lp_gl * Lv`** (matrices GL, pas les matrices corrigées Vulkan) pour l'extraction.

### 11.2 Occlusion (§6.1 étape 3)
- Une requête par `ChunkPos`, créée paresseusement, jamais détruite pour les chunks déchargés (fuite Java, sans effet visible).
- Visibilité par défaut `true`. Un chunk invisible n'est pas dessiné mais sa boîte est toujours testée, donc il réapparaît avec ≥ 1 frame de latence.
- Le test se fait contre la profondeur du **terrain opaque seul** de la frame courante.
- [INFO] Si la caméra est dans l'AABB d'un chunk dont la propre géométrie couvre toutes les faces de la boîte, celui-ci peut alterner visible/invisible (clignotement). Comportement Java ; garde-fou « caméra dans l'AABB ⇒ visible » possible en [DÉVIATION] désactivée par défaut.
- Vulkan : `VK_QUERY_TYPE_OCCLUSION`, pool par frame-in-flight, lecture des résultats de la frame N−k **sans attente** (`VK_QUERY_RESULT_WITH_AVAILABILITY_BIT` ou après le fence de cette frame) ; résultat non dispo → garder la valeur précédente. Le flag `PRECISE` n'est pas nécessaire (`samples > 0`).

---

## 12. Bloom
Si `bloomEnabled` (défaut **true**) :
```
horizontal = true ; first = true
viewport (0,0,W/2,H/2) ; shader blur, image = unité 0
répéter 10 fois :                      // 5 horizontales + 5 verticales, alternées, H d'abord
    cible = pingpong[horizontal ? 1 : 0]
    source = first ? brightBuffer (pleine résolution, échantillonnage LINEAR) : pingpongTex[horizontal ? 0 : 1]
    uniform horizontal ; dessiner le quad
    horizontal = !horizontal ; first = false
```
Résultat final dans `pingpong[0]` (après 10 itérations `horizontal == true`, le composite lit `pingpongTex[horizontal ? 0 : 1] = [0]`). Pas de clear des ping-pong (chaque passe recouvre tout). Pas de profondeur attachée. La première passe lit la pleine résolution avec un pas d'un texel pleine résolution (réduction implicite bilinéaire).
Si désactivé : `pingpong[0]` est effacé à `(0,0,0,0)` (viewport W/2×H/2) → contribution nulle.

---

## 13. Composite post-process (`pp.frag`)
Cible : framebuffer écran, viewport `(0,0,W,H)`, clear couleur (couleur de clear courante = **(0,0,0,0)** si le bloom est désactivé, sinon la dernière `(r,g,b,1)` — sans effet visible car le quad couvre tout) + profondeur. Quad plein écran (z = 0, écrit une profondeur 0.5 ensuite écrasée par le blit).

Uniformes :

| Nom | Type | Valeur |
|---|---|---|
| `scene` | sampler2D (u0) | `colorBuffer` |
| `bloomBlur` | sampler2D (u1) | `pingpongTex[0]` |
| `depthMap` | sampler2D (u2) | `depthBuffer` |
| `cloudTexture` | sampler2D (u3) | nuages |
| `noiseTexture` | sampler2D (u4) | bruit |
| `inverseView` | mat4 | `V⁻¹` |
| `inverseProj` | mat4 | `P_gl⁻¹` |
| `uCameraPos` | vec3 | position caméra |
| `uFogColor` | vec3 | `(r,g,b)` §8.1 |
| `uLightDir` | vec3 | `shadowLightDir` |
| `uZenithColor` / `uHorizonColor` / `uSunColor` | vec3 | §7.3 |
| `uFluidState` | int | 0 air / 1 eau / 2 lave |
| `uTime` | float | temps (s) |
| `uAuroraIntensity` | float | §7.3 |
| `uAuroraPhase` | float | `auroraPhase` |
| `uHeatIntensity` | float | §8.2 |

Fonctions : `hash(p) = fract(sin(dot(p,(12.9898,78.233)))*43758.5453)` ; `noise(p) = texture(noiseTexture, p/256).r` ; `fbm(p) = Σ_{i<4} 0.5^{i+1}·noise(p·2^i)`.

Algorithme (uv en convention GL : v = 0 en bas de l'écran) :

### 13.1 Distorsion
- `fluidState == 0 && heat > 0` : `nHeat = fbm(uv*15 + (0, -t*3))*2 - 1` ; `uv += nHeat*0.02*heat` (les deux composantes).
- eau : `uv.x += sin(uv.y*15 + 2t)*0.003` ; puis `uv.y += cos(uv.x*15 + 2t)*0.003` (avec le `uv.x` déjà modifié).
- lave : idem avec fréquence 3, vitesse 0.4, amplitude 0.006.
Toutes les lectures suivantes (`scene`, `bloomBlur`, `depthMap`) utilisent l'uv distordu (CLAMP_TO_EDGE).

### 13.2 Bloom et chaleur
`hdr = scene(uv) + bloom(uv)` (bloom ajouté **avant** le brouillard). Si air et chaleur : `hdr += mix((1.2,0.1,0), (1.0,0.5,0), nHeat*0.5+0.5) * heat * 0.35`.

### 13.3 Reconstruction
`depth = depthMap(uv).r` ; `ndc = (uv*2-1, depth*2-1, 1)` ; `vp = inverseProj*ndc ; vp /= vp.w` ; `wp = inverseView*vp` ; `viewDir = normalize(wp - cam)` ; `dist = |wp - cam|` ; si `depth >= 1` : `dist = 10000`.

### 13.4 Brouillard
`(density, falloff)` = air (0.002, 1.8) ; eau (0.015, 1.2) ; lave (0.3, 1.0). `fog = clamp(exp(-(dist*density)^falloff), 0, 1)` (1 = pas de brouillard).

### 13.5 Ciel
- `g = clamp(pow(max(0, viewDir.y + 0.05), 0.6), 0, 1)` ; `sky = mix(horizon, zenith, g)`.
- Aurores si air et `auroraIntensity > 0` : `sky += getAurora(cam, viewDir, t, uv, phase) * auroraIntensity`.
  `getAurora` : si `rd.y < 0.02` → 0 ; couche `y ∈ [80, 200]` ; `d1 = max((80 - ro.y)/rd.y, 0)`, `d2 = (200 - ro.y)/rd.y`, si `d2 ≤ 0` → 0 ; 30 pas de `(d2-d1)/30`, départ tramé `d1 + step*hash(uv*1000)`. Par pas : `p = ro + rd*t` ; `q = (p.xz + p.y*0.5)*0.002` ; `mask = smoothstep(0.4, 0.75, fbm(q*0.5 + phase*0.2))` ; si `mask > 0` : `n1 = fbm(q*1.5 + t*0.01)` ; `w = q + (n1, fbm(q + 1))*1.5` ; `wave = sin(w.x*6 + w.y*4 + t*0.2)*0.5 + 0.5` ; `ribbon = smoothstep(0.35,0.85,wave)*smoothstep(0.2,0.8,n1)` ; si `ribbon > 0` : `h = (p.y-80)/120` ; `prof = smoothstep(0,0.2,h)*smoothstep(1,0.3,h)` ; `c = mix((0.1,0.9,0.4),(0.4,0.1,0.8),h)` ; `red = fbm(q*2 - t*0.01)` ; `c = mix((0.9,0.1,0.2), c, smoothstep(0,0.2,h + red*0.1))` ; `a = ribbon*prof*mask*mix(0.3,1.2,rd.y)*0.8` ; accumulation avant→arrière `col += c*a*(1-A) ; A += a*(1-A)` ; arrêt si `A > 0.95`. Retour `col*1.5`.
- `sunDot = max(dot(viewDir, normalize(-uLightDir)), 0)` ; `halo = sunDot^64 * sunCol` (0 si `fluidState > 0`) ; `fogCol = sky + halo`.
- Eau : `wc = pow(uFogColor, 2.2)` ; `df = clamp(exp(-(64 - cam.y)*0.06), 0.02, 1)` ; `fogCol = mix((0.02,0.06,0.15), wc, df) + sunCol*max(0,viewDir.y)^8*0.15*df`.
- Lave : `fogCol = pow(uFogColor, 2.2)`.
- `hdr = mix(fogCol, hdr, fog)`.

### 13.6 Nuages (air uniquement, `viewDir.y > 0`)
Plaque entre **y = 130 et 134**, distance max **500**, **15 pas**, échelle de cellule **12**, vent `x += t*3`.
```
t1 = (130 - cam.y)/vd.y ; t2 = (134 - cam.y)/vd.y ; tS = max(t1,0) ; tE = max(t2,0)
si tE > 0 && tS < 500 && (depth >= 1 || dist > tS) :
    tE = min(tE, 500) ; si depth < 1 : tE = min(tE, dist)
    step = (tE - tS)/15 ; t = tS ; acc = 0 ; A = 0 ; phase = sunDot^4
    15 fois :
        p = cam + vd*t ; m = p.xz ; m.x += time*3
        cell = floor(m/12) ; tex = texture(cloudTexture, cell/256) ; dens = tex.r*tex.a
        si dens > 0.1 :
            hg = clamp((p.y-130)/4, 0, 1)
            c = mix(mix(horizon, zenith, hg)*0.8, sunCol, phase*0.6)
            si p.y < 130.5 : c *= 0.6
            a = dens*0.4 ; acc += c*a*(1-A) ; A += a*(1-A) ; si A > 0.95 : sortir
        t += step
    si A > 0 :
        cf = clamp(exp(-(tS*density)^falloff), 0, 1)       // density/falloff de l'air
        hdr = mix(hdr, mix(fogCol, acc/A, cf), A)
```
[INFO] Caméra au-dessus de 134 : `tE = 0` → pas de nuages (on ne les voit pas d'en haut).

### 13.7 Filtres et tonemapping
- Air : `hdr *= (1.15, 1.05, 0.95)`.
- Eau : `att = clamp(exp(-(64 - cam.y)*0.04), 0.15, 1)` ; `hdr *= mix((0.4,0.7,0.9), 1, 0.3) * att` (= `(0.58, 0.79, 0.93)*att`) ; vignette `v = 1 - smoothstep(0.4, 1.5, |uv - 0.5|)` ; `hdr *= mix(0.6, 1, v)`.
- Lave : `hdr *= (1.5, 0.5, 0.1)`.
- ACES (Narkowicz) : `clamp(x(2.51x + 0.03) / (x(2.43x + 0.59) + 0.14), 0, 1)` ; puis `pow(·, 1/2.2)` ; alpha 1.

### 13.8 Après le composite
Blit de la profondeur HDR vers la profondeur de l'écran (même taille, NEAREST) → la surbrillance et l'UI peuvent tester la profondeur de la scène.

---

## 14. Mobs

### 14.1 `MobRenderer.render`
- Si pas de loader ou aucune entité `(Transform, MobType)` → rien.
- États : cull BACK, shader `mob`, `projection`/`view`, unité 0.
- Pour chaque entité (ordre de requête ECS, non trié) : config de mob introuvable → ignorer. Modèle = `loader.getModel(id)` ; absent → **cube manquant** (unité, `[-0.5,0.5]×[0,1]×[-0.5,0.5]`, damier magenta/noir 16×16 tuiles 4, luminosités +Z/−Z 0.85, ±X 0.70, +Y 1.0, −Y 0.5, UV 0..1 par face) mis à l'échelle `(widthX, height, widthZ)` de la config.
- `model = T(x,y,z)·Ry(yaw)·Rx(pitch)[·S]`. Texture = celle du modèle, ou blanc 1×1 si mode couleur.
- Plage opaque `[0, nOpaque)` : blend off, depth write on. Plage transparente `[nOpaque, nOpaque+nTrans)` : blend `SRC_ALPHA/ONE_MINUS_SRC_ALPHA`, depth write off. **Par mob** (pas de tri global).
- Fin : depth write on, blend off.
- [JAVA-BUG] Le FBO HDR a deux draw buffers mais le shader n'écrit que la location 0 : l'attachement 1 reçoit une valeur indéfinie en GL. En Vulkan (corrigé phase 6, conforme au code) : la passe **B2** (mobs + particules, §18.4 étape 4) n'attache **que `colorBuffer`** — le bright buffer n'est pas attaché, il reste donc inchangé (comportement pratique observé sur les pilotes courants ; équivalent à un `colorWriteMask = 0` sur l'attachement 1, sans sortie de fragment indéfinie). Passe sautée s'il n'y a ni mob ni particule (résultat identique).

### 14.2 `MobModelLoader` — ordre de résolution par id
Pour chaque `MobConfig` (ordre du registre, doublons d'id ignorés), et identiquement pour `loadMob(id)` (qui remplace et détruit l'ancien modèle seulement si un nouveau est construit) :
1. **JEM** (OptiFine CEM) : `<root>/assets/minecraft/optifine/cem/<id>.jem` pour `root ∈ [packRoot, defaultRoot]` ; le premier fichier existant **et** parsable gagne.
2. **Forme vanilla codée en dur** (§14.4) + texture `findEntityTexture` ; texture introuvable → damier manquant 16×16.
3. **Géométrie couleur** `cfg.geometry` (boîtes `[bx,by,bz,bw,bh,bd,r,g,b,a]` fournies par nb-mob).
4. Si le mob n'est pas « built-in » : texture `findEntityTexture` sur une boîte `(-8,0,-8, 16,16,16)` avec un « atlas » 1×1 : §14.3 avec `tw = th = 1`, `u0 = v0 = 0`. [JAVA-BUG] reproduit (corrigé phase 6, conforme au code) : le commentaire Java annonce des UV pleines par face, mais les tailles en pixels (16) ne sont pas normalisées → **U de 0 à 64, V de 1 à −47** (16 unités par face) : la texture est **répétée** (16 fois par face, échantillonneur REPEAT) au lieu d'être étirée.
5. Sinon aucun modèle → cube manquant au rendu.

`findEntityTexture(id)` : pour `root ∈ [packRoot, defaultRoot]`, dans `assets/minecraft/textures/entity/` : `id/id.png`, `id.png`, `id/id_temperate.png` ; le **premier existant** est chargé et **retourné même si le chargement échoue** (null, sans essayer `defaultRoot`) ; sinon premier `.png` (tri lexicographique des chemins, insensible à la casse sous Windows) du dossier `id/` s'il se charge. Textures entités : RGBA8 retournées, NEAREST/NEAREST, REPEAT, mips générés (inutilisés).

### 14.3 Construction d'une boîte texturée (`addTexBox`)
Unités : pixels MC ; positions `/16` → blocs. Sommets : 6 faces × 2 triangles `(0,1,2)(2,3,0)` = 36 sommets ; teinte `(1,1,1,1)`.
```
x0=bx/16 x1=(bx+bw)/16 ; y0=by/16 y1=(by+bh)/16 ; z0=bz/16 z1=(bz+bd)/16
uA=u0/tw uB=(u0+bd)/tw uC=(u0+bd+bw)/tw uD=(u0+2bd+bw)/tw uE=(u0+2bd+2bw)/tw
vT=1-v0/th vM=1-(v0+bd)/th vB=1-(v0+bd+bh)/th
```
| Face (lum.) | v0 | v1 | v2 | v3 |
|---|---|---|---|---|
| +Z (0.85) | (x0,y0,z1) uB,vB | (x1,y0,z1) uC,vB | (x1,y1,z1) uC,vM | (x0,y1,z1) uB,vM |
| −Z (0.85) | (x1,y0,z0) uD,vB | (x0,y0,z0) uE,vB | (x0,y1,z0) uE,vM | (x1,y1,z0) uD,vM |
| −X (0.70) | (x0,y0,z0) uA,vB | (x0,y0,z1) uB,vB | (x0,y1,z1) uB,vM | (x0,y1,z0) uA,vM |
| +X (0.70) | (x1,y0,z1) uC,vB | (x1,y0,z0) uD,vB | (x1,y1,z0) uD,vM | (x1,y1,z1) uC,vM |
| +Y (1.00) | (x0,y1,z1) uB,vM | (x1,y1,z1) uC,vM | (x1,y1,z0) uC,vT | (x0,y1,z0) uB,vT |
| −Y (0.50) | (x0,y0,z0) uD,vT | (x1,y0,z0) uC,vT | (x1,y0,z1) uC,vM | (x0,y0,z1) uD,vM |

Boîte couleur (`addColorBox`) : mêmes positions/ordres de sommets et luminosités, UV `(0,0)`, teinte `(r,g,b,a)` ; va dans la liste transparente si `a < 1`. Buffer final = opaques puis transparents. Liste vide → pas de modèle.

Correction de hauteur (formes vanilla uniquement) : `scale = texW/tw` ; `nativeTh = texH/scale` ; si `|nativeTh - th| > 0.5` → `th = nativeTh` (gère les textures 64×64 des variantes 1.21+ et le damier 16×16 → th = 64 si tw = 64).

### 14.4 Formes vanilla (`[tw, th]` puis boîtes `bx by bz bw bh bd u0 v0`)
- **cow** 64×32 : `-6 10 -5 12 9 10 18 4` · `-4 16 6 8 8 6 0 0` · `-6 0 3 4 12 4 0 16` · `2 0 3 4 12 4 0 16` · `-6 0 -7 4 12 4 0 16` · `2 0 -7 4 12 4 0 16`
- **sheep** 64×32 : `-4 9 -5 8 10 10 28 8` · `-3 16 6 6 6 4 0 0` · `-5 0 3 4 12 4 0 16` · `1 0 3 4 12 4 0 16` · `-5 0 -7 4 12 4 0 16` · `1 0 -7 4 12 4 0 16`
- **pig** 64×32 : `-5 7 -5 10 8 10 28 8` · `-4 12 5 8 8 8 0 0` · `-5 0 2 4 8 4 0 16` · `1 0 2 4 8 4 0 16` · `-5 0 -6 4 8 4 0 16` · `1 0 -6 4 8 4 0 16`
- **chicken** 64×32 : `-3 4 -3 6 8 6 0 9` · `-2 11 3 4 6 4 0 0` · `-4 4 -3 1 8 6 24 13` · `3 4 -3 1 8 6 24 13` · `-3 0 -1 3 4 3 26 0` · `0 0 -1 3 4 3 26 0`
- **rabbit** 64×32 : `-3 6 -3 6 6 6 16 0` · `-2 11 1 4 5 4 0 0` · `-1 10 4 2 8 1 0 16` · `1 10 4 2 8 1 10 16` · `-2 0 -2 4 6 4 0 16` · `2 0 -2 4 6 4 0 16`
- **polarbear** 64×64 : `-7 14 -6 14 10 11 18 4` · `-5 21 5 10 9 7 0 0` · `-7 0 4 4 16 4 0 16` · `3 0 4 4 16 4 0 16` · `-7 0 -8 4 16 4 0 16` · `3 0 -8 4 16 4 0 16`
- **wolf** 64×32 : `-4 10 -4 8 8 8 18 4` · `-3 16 4 6 6 4 0 0` · `-4 0 3 3 8 3 0 16` · `1 0 3 3 8 3 0 16` · `-4 0 -6 3 8 3 0 16` · `1 0 -6 3 8 3 0 16`
- **fox** 64×32 : `-4 8 -4 8 6 8 24 0` · `-3 13 3 6 5 5 0 0` · `-4 0 2 3 6 3 0 16` · `1 0 2 3 6 3 0 16` · `-4 0 -5 3 6 3 0 16` · `1 0 -5 3 6 3 0 16`
- **horse** (= **donkey** = **mule**) 64×64 : `-5 11 -6 10 10 12 18 4` · `-3 20 4 6 7 5 0 0` · `-2 26 6 2 7 2 56 0` · `-5 0 4 4 14 4 0 16` · `1 0 4 4 14 4 0 16` · `-5 0 -8 4 14 4 0 16` · `1 0 -8 4 14 4 0 16`
- **llama** 64×64 : `-6 12 -5 12 14 10 18 4` · `-4 25 3 8 8 5 0 0` · `-6 0 4 4 14 4 0 16` · `2 0 4 4 14 4 0 16` · `-6 0 -9 4 14 4 0 16` · `2 0 -9 4 14 4 0 16`
- **humanoïde** (zombie, skeleton, husk, drowned, piglin, villager, pillager) 64×64 : `-4 24 -4 8 8 8 0 0` · `-4 12 -2 8 12 4 16 16` · `-8 12 -2 4 12 4 40 16` · `4 12 -2 4 12 4 32 48` · `-4 0 -2 4 12 4 0 16` · `0 0 -2 4 12 4 16 48`
- **creeper** 64×32 : `-4 18 -4 8 8 8 0 0` · `-4 6 -2 8 12 4 16 16` · `-4 0 2 4 6 4 0 16` · `0 0 2 4 6 4 0 16` · `-4 0 -6 4 6 4 0 16` · `0 0 -6 4 6 4 0 16`
- **spider** 64×32 : `-7 5 -4 14 6 8 18 4` · `-4 5 3 8 8 8 0 0` · `-8 5 1 4 1 12 0 16` · `4 5 1 4 1 12 0 16` · `-8 5 -3 4 1 12 0 16` · `4 5 -3 4 1 12 0 16`
- **enderman** 64×32 : `-2 24 -2 4 4 4 0 0` · `-2 12 -1 4 12 2 10 0` · `-4 12 -1 2 12 2 16 0` · `2 12 -1 2 12 2 16 0` · `-2 0 -1 2 12 2 0 16` · `0 0 -1 2 12 2 8 16`
- **slime** (= **magma_cube**) 64×32 : `-4 0 -4 8 8 8 0 0`
- **squid** 64×32 : `-4 4 -4 8 8 8 0 0` · `-1 0 -2 2 4 2 16 16` · `2 0 1 2 4 2 16 16` · `-3 0 2 2 4 2 16 16` · `-1 0 -4 2 4 2 16 16`
- **bat** 64×64 : `-2 8 -2 4 6 4 0 0` · `-2 2 -1 4 8 2 16 16` · `-8 3 0 6 8 1 32 0` · `2 3 0 6 8 1 32 0`
- **phantom** 64×64 : `-2 8 -2 4 4 4 0 0` · `-4 4 -2 8 4 4 16 8` · `-12 6 -1 8 2 3 0 16` · `4 6 -1 8 2 3 0 16`
- **bee** 64×64 : `-3 6 -3 6 6 6 0 0` · `-3 3 -3 6 4 8 24 0` · `-8 7 0 6 1 6 24 16` · `2 7 0 6 1 6 24 16`
- **armadillo** 64×64 : `-4 4 -3 8 8 8 0 0` · `-2 9 4 4 4 2 24 0` · `-4 0 2 3 4 3 0 16` · `1 0 2 3 4 3 0 16` · `-4 0 -5 3 4 3 0 16` · `1 0 -5 3 4 3 0 16`

Tous les modèles vanilla sont entièrement opaques (`nTrans = 0`).

### 14.5 Parseur JEM (minimal, à reproduire tel quel)
Fichier lu en UTF-8, lignes concaténées avec `\n`.
- `textureSize` : premier tableau numérique après la première occurrence de `"textureSize"` (tableau = texte entre le premier `[` et le `]` suivant, séparé par `,`, `parseFloat` — un élément non numérique fait échouer tout le fichier → pas de JEM). Absent ou < 2 valeurs → échec.
- `texture` : chaîne après la première occurrence exacte de `"texture"` (premier `"` après le `:` jusqu'au `"` suivant). Résolution : retirer le préfixe `assets/minecraft/`, tester `<root>/assets/minecraft/<chemin>` pour `[packRoot, defaultRoot]` ; sinon deviner l'id = nom de fichier sans extension, suffixe `_(temperate|cold|warm|baby)` retiré → `findEntityTexture`.
- Boucle sur chaque occurrence de `"coordinates"` : tableau `[x,y,z,w,h,d]` (< 6 valeurs → ignoré). Sous-chaîne « boîte » : du `{` ouvrant englobant (recherche arrière avec comptage de profondeur) jusqu'à `findObjectEnd` — qui, partant de `"coordinates"`, ignore le premier `}` rencontré avant tout `{` et s'arrête à la fin de l'objet suivant ([JAVA-BUG] : la sous-chaîne déborde sur la boîte suivante ; une boîte sans `textureOffset` prend celui de la suivante). Dans cette sous-chaîne : premier `"textureOffset"` et premier `"color"`.
  - `color` avec ≥ 4 valeurs → boîte couleur (transparente si a < 1) ;
  - sinon `textureOffset` ≥ 2 valeurs **et** texture résolue → boîte texturée avec `tw, th` du fichier (pas de correction de hauteur) ;
  - sinon ignorée.
- **Ignorés** : hiérarchie de bones, `translate`, `rotate`, `pivot`, `mirrorTexture`, `sizeAdd`, `uvNorth…`, `submodels`, animations. Les coordonnées sont prises telles quelles (pixels, origine aux pieds).
- Aucun sommet → échec (étape suivante de la résolution). Texture non résolue → modèle en mode couleur (blanc).

---

## 15. Particules (`ParticleRenderer`)
- Quad unité `TRIANGLE_FAN` 4 sommets (→ triangles `(0,1,2)(0,2,3)`) : `(-0.5,-0.5,0, 0,1)`, `(0.5,-0.5,0, 1,1)`, `(0.5,0.5,0, 1,0)`, `(-0.5,0.5,0, 0,0)` (v inversé : avec la texture retournée, l'image apparaît **tête en bas** — à reproduire).
- Rendu dans le FBO HDR après les mobs : blend `SRC_ALPHA/ONE_MINUS_SRC_ALPHA`, depth write **off**, depth test on, cull BACK, atlas particules en unité 0. **Pas de tri.**
- Billboard sphérique : `model = T(p.position)` puis remplacement du bloc 3×3 par la **transposée** de la rotation de la vue (`model[c][r] = view[r][c]` pour `r,c < 3`), puis `scale(s)` uniforme avec `s = type.scale` (`leaf` 0.15, `lava` 0.1 — `particles.json`). Soit `model = T(p) · R_vᵀ · S(s)`.
- Uniformes par particule : `model`, `colorTint = p.color`, `layer = type.textureLayer`.
- Fin : depth write on, blend off. Bright buffer non attaché en passe B2, donc inchangé (même raison que §14.1).

---

## 16. Surbrillance du bloc visé (dans `UIManager.render`)
- Après le composite et le blit de profondeur, sur l'écran ; uniquement dans les états `PLAYING`, `PAUSED`, `INVENTORY`, si `player.targetedBlock` existe.
- Shader `hl` (noir opaque), `model = translate(target)`, `projection`/`view` caméra.
- Géométrie : 12 arêtes (`LINES`, 24 sommets) d'un cube `[o, s+o]³` avec `s = 1.002`, `o = -0.001` (soit `[-0.001, 1.001]`) : 4 arêtes du bas (y = o), 4 du haut (y = s+o), 4 verticales.
- Depth test **on**, `LEQUAL`, depth write on (défaut), blend off, largeur 1 px.
- Ensuite les éléments UI 2D (nb-ui) sont dessinés.

---

## 17. Particularités Java à reproduire (récapitulatif)
1. `brightBuffer` effacé à la couleur de brouillard `(r,g,b,1)` (pas à noir) : là où il n'y a pas de géométrie (ciel), la source du bloom vaut la couleur de brouillard ; le flou la fait **déborder sur les silhouettes du terrain** (halo coloré). Le ciel lui-même est ensuite écrasé par le brouillard.
2. Ombres : seuils 1000/2000 dans `shadow.vert` → feuilles et plantes en croix projettent des ombres pleines ; vague de l'eau appliquée à la lave dans la carte d'ombre (§9.2).
3. Shadow OFF : carte figée/non initialisée mais toujours échantillonnée (§10).
4. Fluides sans écriture de profondeur, dessinés avant mobs/particules (§6.1).
5. Mobs non éclairés et sans gamma (§9.6).
6. Particules à l'envers (§15).
7. `depthFunc` LESS puis LEQUAL après la première surbrillance (§6.2).
8. Occlusion à latence d'une frame ou plus, visible par défaut (§11.2).
9. Pas de clamp dans la boucle fixe (§3.2).
10. Décalages du flou de la première passe en texels pleine résolution (§12).
11. Bandes animées : dernière frame (bas du PNG) dans l'atlas (§5.1).
12. Parseurs minimaux `pack.mcmeta` / JEM (§5.2, §14.5).

---

## 18. Traduction GL → Vulkan

### 18.1 Clip space : Y et profondeur
GL : NDC y vers le haut, z ∈ [-1,1], fenêtre `depth = 0.5·z + 0.5`, ligne 0 = **bas**. Vulkan : NDC y vers le **bas**, z ∈ [0,1], ligne 0 = **haut**.

Stratégie retenue (minimise les modifications de shaders) : **matrice de correction appliquée à toutes les projections** (caméra et lumière), viewports à hauteur **positive** :
```
C = | 1  0   0    0  |
    | 0 -1   0    0  |       P_vk  = C · P_gl
    | 0  0  0.5  0.5 |       L_vk  = C · Lp_gl · Lv
    | 0  0   0    1  |
```
Conséquences (à respecter exactement) :
- `depth_vk = 0.5·z_gl + 0.5` = **même valeur que la profondeur fenêtre GL** → les formules `depth*2-1` restent valides **si elles sont combinées avec les matrices GL** (`P_gl`, `P_gl⁻¹`).
- Le flip Y de la projection inverse deux fois l'orientation des triangles → **`frontFace = COUNTER_CLOCKWISE`** comme en GL, cull BACK/FRONT inchangés.
- Les images rendues sont stockées « à l'endroit » (ligne 0 = haut de la vue). La relation uv ↔ NDC *Vulkan* reste `uv = ndc*0.5 + 0.5` pour toute image écrite et relue avec la même matrice (shadow map).
- Pipelines plein écran (blur, pp) : positions du quad utilisées telles quelles en NDC Vulkan, **cull NONE** (le quad apparaît CW). Avec les UV du quad Java (`uv = pos*0.5+0.5`), la copie écran est l'identité : `uv.v = 0` = ligne du haut.

Modifications de shaders induites (liste fermée) :
1. `main.vert`, `mob.vert`, `particle.vert`, `hl.vert` : `projection` = `P_vk`. `shadow.vert` et `FragPosLightSpace` : `L_vk`.
2. `main.frag` `ShadowCalculation` : `p.xy = ndc.xy*0.5 + 0.5` (inchangé) mais **`p.z = ndc.z`** (sans `*0.5+0.5`).
3. `main.frag` `getLinearDepth` : utiliser les coefficients de **`P_gl`** (`m32_gl`, `m22_gl`) passés en UBO, avec `zNdc = depth*2-1` (inchangé) ; équivalent : `lin = n·f / (f - depth·(f - n))`.
4. `pp.frag` : travailler en uv **GL** : `uvGL = vec2(uv.x, 1.0 - uv.y)` ; appliquer toute la distorsion, `hash`, vignette, etc. sur `uvGL` exactement comme en Java ; échantillonner `scene/bloom/depth` en `vec2(uvGL.x, 1.0 - uvGL.y)` ; reconstruction `ndc = (uvGL*2-1, depth*2-1, 1)` avec `inverseProj = P_gl⁻¹` (pas `P_vk⁻¹`). Sans cela, la chaleur « monterait » vers le bas.
5. `main.frag` dérivées : `dFdy` change de signe en Vulkan (y écran vers le bas) → la normale `cross(dFdx, dFdy)` est inversée, **corrigée automatiquement** par le test `dot(n, viewDir) < 0`. Aucune modification.
6. `texelFetch(opaqueDepth, ivec2(gl_FragCoord.xy))` : cohérent (même espace image). Aucune modification.
7. Extraction des plans de frustum (CPU) : sur `P_gl·V` et `Lp_gl·Lv`.

### 18.2 GLSL 330 → GLSL 450 (compilation SPIR-V via `shaderc` au build ou `naga`)
- `#version 450` ; `layout(location = N)` explicite sur **tous** les `in/out` inter-étapes.
- Uniformes « libres » → UBO `std140` (par frame) + push constants (par draw : `model`, `layer`, `colorTint`, `horizontal`, `uRenderPass` ou spécialisation). Attention à l'alignement `vec3` (16 o) en std140.
- `uniform float weight[5] = ...` → `const float weight[5]`.
- `uniform bool horizontal` → push constant `int` (ou deux pipelines / constante de spécialisation).
- Samplers → `layout(set, binding) uniform sampler2D/sampler2DArray` (combined image samplers).
- `uRenderPass` : préférer deux pipelines avec constante de spécialisation (états différents de toute façon).
- `discard` → identique (`demote` non requis).

### 18.3 Formats
| GL | Vulkan |
|---|---|
| `GL_RGBA` + `UNSIGNED_BYTE` (textures) | `R8G8B8A8_UNORM` (**pas** SRGB) |
| `GL_RED` | `R8_UNORM` |
| `GL_RGBA16F` | `R16G16B16A16_SFLOAT` |
| `GL_DEPTH_COMPONENT24` (HDR) | `D32_SFLOAT` (universel, sampled OK) ; `D24_UNORM_S8_UINT` si `SAMPLED_IMAGE` supporté |
| `GL_DEPTH_COMPONENT` (ombre) | `D32_SFLOAT` (ou `D16_UNORM` en LOW, [DÉVIATION]) |
| Framebuffer défaut (RGBA8 non sRGB, depth 24/stencil 8) | swapchain `B8G8R8A8_UNORM` + `SRGB_NONLINEAR` colorspace ; pas de depth propre (voir 18.5) |

Samplers : un par combinaison (NEAREST_MIPMAP_LINEAR + aniso ≤ 8 + REPEAT pour les atlas ; NEAREST REPEAT ; LINEAR CLAMP ; NEAREST CLAMP ; NEAREST CLAMP_TO_BORDER blanc). Atlas : `maxLod = mipLevels`, `mipmapMode = LINEAR`, `minFilter = NEAREST`, `magFilter = NEAREST`.

### 18.4 Passes Vulkan et layouts
1. **Shadow** : render pass/dynamic rendering depth-only, `loadOp CLEAR(1.0)`, `storeOp STORE` → layout final `SHADER_READ_ONLY_OPTIMAL` (ou `DEPTH_STENCIL_READ_ONLY_OPTIMAL`). Si OFF : ne pas rendre, l'image garde son layout de lecture.
2. **Scène A** (opaque + occlusion) : color0/color1 `CLEAR(r,g,b,1)` **tous deux**, depth `CLEAR(1.0)`. Pipelines : terrain opaque ; AABB (`colorWriteMask = 0` sur les 2 attachements, depth write off, cull none) avec `vkCmdBeginQuery/EndQuery` par chunk (`queryPool` reset hors render pass).
3. Transition depth → `DEPTH_STENCIL_READ_ONLY_OPTIMAL` (barrière `LATE_FRAGMENT_TESTS` écriture → `FRAGMENT_SHADER | EARLY_FRAGMENT_TESTS` lecture).
4. **Scène B** (fluides, mobs, particules) : color `LOAD`, depth `LOAD` en **read-only** (`DEPTH_STENCIL_READ_ONLY_OPTIMAL`) et simultanément échantillonnée comme `opaqueDepth` — usage légal en Vulkan (attachement depth lecture seule + sampled dans le même layout). Toutes les pipelines de cette passe ont depth write **off**… **sauf la plage opaque des mobs** (depth write on en Java). Solution : scinder — **B1** fluides (depth read-only + sampled), puis transition depth → `DEPTH_STENCIL_ATTACHMENT_OPTIMAL`, **B2** mobs + particules (depth `LOAD`, écriture permise, plus échantillonnée ; **seul `colorBuffer` attaché**, le bright buffer reste inchangé, §14.1).
5. **Bloom** : 10 passes ping-pong (W/2×H/2), chaque cible `DONT_CARE` → `COLOR_ATTACHMENT` puis `SHADER_READ_ONLY` ; barrières entre itérations. Si désactivé : un `CLEAR(0)` de `pingpong[0]`.
6. **Composite** : swapchain `CLEAR` ignoré (le quad couvre tout), pipeline pp ; transitions de `colorBuffer`, `depthBuffer` (`DEPTH_STENCIL_READ_ONLY_OPTIMAL` ou `SHADER_READ_ONLY_OPTIMAL`), `pingpong[0]`, nuages, bruit en lecture.
7. **Overlay** (remplace le blit de profondeur) : même render pass que le composite ou suivant, en attachant `depthBuffer` HDR (même extent que la swapchain) comme depth en `LOAD`. Pipeline surbrillance : `LINE_LIST`, depth test `LESS_OR_EQUAL`, `lineWidth 1.0`. Puis UI (nb-ui) sur la même passe, depth test désactivé dans ses pipelines. Swapchain → `PRESENT_SRC_KHR`. (Alternative : copie de profondeur si l'overlay a besoin d'écrire la profondeur — non requis.)

Vertex buffers : un buffer par chunk (ou sous-allocation dans un gros buffer) ; `vkCmdDraw(vertexCount)` non indexé. Quad particules et AABB en buffers statiques. Destruction différée des buffers de chunks de `frames_in_flight` frames (les remplacements de maillage arrivent pendant le jeu).

### 18.5 Divers
- `glBlitFramebuffer` profondeur → supprimé (voir 18.4.7).
- Requêtes d'occlusion → §11.2.
- Mipmaps → `vkCmdBlitImage` (vérifier `FORMAT_FEATURE_SAMPLED_IMAGE_FILTER_LINEAR`).
- Redimensionnement : recréer swapchain + HDR + ping-pong ; la shadow map ne dépend que de la qualité.
- Anisotropie : activer `samplerAnisotropy` si disponible ; sinon désactivée (comme l'absence d'extension en GL).
- `GL_SAMPLE_ALPHA_TO_COVERAGE` : ignoré (pas de MSAA).
- Blending : `SRC_ALPHA / ONE_MINUS_SRC_ALPHA` pour couleur **et** alpha (`glBlendFunc` non séparé), op ADD, appliqué à chaque attachement ; color1 a toujours alpha source 1 → remplacement.

---

## 19. Contrat Rust proposé (API publique de `nb-gfx`)

Principe : `nb-gfx` ne connaît ni `World`, ni `Chunk`, ni `BlockType`, ni l'ECS. Il reçoit des **données plates** par frame et des **commandes** de ressources.

```rust
// ---------- Configuration ----------
pub enum ShadowQuality { Off, Low, High }           // Low = 1024, High = 4096
pub struct GfxSettings {
    pub vsync: bool,
    pub fullscreen: bool,
    pub shadow_quality: ShadowQuality,
    pub bloom: bool,
    pub fov_y_deg: f32,                               // défaut 90
}

// ---------- Horloge jour/nuit (pure, testable sans GPU) ----------
pub struct SkyClock { pub time_of_day: f32, pub day_length_seconds: f32, pub aurora_phase: f32 }
impl SkyClock {
    pub fn new() -> Self;                             // 900, 1200, 0
    pub fn update(&mut self, dt: f64);                // §7.1
    pub fn set_time(&mut self, t: f32);
    pub fn set_day_length_seconds(&mut self, s: f32); // max(1, s)
}
pub struct SkyParams {
    pub light_dir: Vec3, pub shadow_light_dir: Vec3,
    pub zenith: Vec3, pub horizon: Vec3, pub sun_color: Vec3,
    pub base_fog: Vec3, pub ambient: Vec3, pub light_color: Vec3,
    pub aurora_intensity: f32,
}
pub fn sky_params(time_of_day: f32) -> SkyParams;   // §7.2–7.3
pub fn heat_from_nearest_lava_dist_sq(min_dist_sq: f32) -> f32; // §8.2 (256 si aucune lave)

// ---------- Caméra (pure) ----------
pub struct Camera { pub position: Vec3, pub pitch_deg: f32, pub yaw_deg: f32, pub fov_y_rad: f32, pub aspect: f32 }
impl Camera {
    pub const NEAR: f32 = 0.05; pub const FAR: f32 = 1000.0;
    pub fn rotate(&mut self, d_pitch: f32, d_yaw: f32);   // clamp ±89
    pub fn view(&self) -> Mat4;  pub fn proj_gl(&self) -> Mat4;
}

// ---------- Données d'entrée géométriques ----------
#[repr(C)] #[derive(Pod, Zeroable, Clone, Copy)]
pub struct ChunkVertex { pub pos: [f32; 3], pub uv: [f32; 2], pub layer: f32, pub color: [f32; 3] } // 36 o, §9
pub struct ChunkKey { pub cx: i32, pub cz: i32 }

pub enum EyeFluid { None, Water { color: Vec3 }, Lava }

pub struct MobInstance<'a> {
    pub model_id: &'a str,             // id de mob ; modèle absent → cube manquant
    pub position: Vec3, pub yaw_rad: f32, pub pitch_rad: f32,
    pub missing_scale: Vec3,           // (widthX, height, widthZ) de la config
}
pub struct ParticleInstance { pub position: Vec3, pub scale: f32, pub layer: u32, pub color: Vec3 }

// ---------- Description des mobs fournie par nb-mob (sans dépendance) ----------
pub struct ColorBox { pub min_px: [f32; 3], pub size_px: [f32; 3], pub rgba: [f32; 4] }
pub struct MobModelDesc { pub id: String, pub built_in: bool, pub geometry: Option<Vec<ColorBox>> }

// ---------- Packs ----------
pub struct ResourcePackMeta { pub name: String, pub description: String }
pub fn list_resource_packs(root: &Path) -> Vec<ResourcePackMeta>;    // §5.2
pub fn find_packs_root() -> PathBuf;                                 // §5.2

// ---------- Atlas CPU (pur, testable) ----------
pub struct TextureArrayCpu { pub tile: u32, pub layers: Vec<Vec<u8>>, pub names: HashMap<String, u32>, pub missing_layer: u32 }
impl TextureArrayCpu {
    pub fn from_dir(dir: &Path) -> Self;                     // §5.1
    pub fn from_named(files: &[(&str, Option<&[u8]>)]) -> Self; // §5.4
    pub fn layer(&self, name: &str) -> u32;                  // insensible à la casse
}

// ---------- Renderer ----------
pub struct Renderer { /* device, swapchain, cibles, pipelines, assets GPU */ }
impl Renderer {
    pub fn new(window: &impl HasWindowHandle + HasDisplayHandle, size: UVec2,
               packs_root: &Path, mobs: &[MobModelDesc], settings: &GfxSettings) -> Result<Self>;
    pub fn resize(&mut self, size: UVec2);                   // 0 → suspend le rendu
    pub fn apply_settings(&mut self, s: &GfxSettings);

    // Ressources
    pub fn block_atlas(&self) -> &TextureArrayCpu;           // mapping nom → couche (pour nb-world / nb-ui)
    pub fn reload_resource_pack(&mut self, name: &str, mobs: &[MobModelDesc]) -> ResourcePackMeta; // §5.2 ; invalide toutes les meshes
    pub fn reload_mob_model(&mut self, desc: &MobModelDesc);
    pub fn upload_chunk_mesh(&mut self, key: ChunkKey, vertices: &[ChunkVertex]); // remplace ; vide = supprime
    pub fn remove_chunk_mesh(&mut self, key: ChunkKey);
    pub fn clear_chunk_meshes(&mut self);

    // Frame
    pub fn render_frame(&mut self, f: &FrameInput, overlay: &mut dyn FnMut(&mut OverlayPass)) -> Result<()>;
    // Phase 6 : frame d'interface seule (états sans `MasterRenderer.render` : choix du monde,
    // réglages, packs, mobs) — sortie effacée à `clear`, puis la passe overlay seule
    // (`SceneRenderer::record_ui_frame`, `OffscreenRenderer::render_ui_only`).
    pub fn render_ui_frame(&mut self, clear: [f32; 4], overlay: &mut dyn FnMut(&mut OverlayPass)) -> Result<()>;
}

pub struct FrameInput<'a> {
    pub camera: &'a Camera,
    pub time_seconds: f32,              // uTime
    pub sky_clock: &'a SkyClock,        // → sky_params + aurora_phase
    pub eye_fluid: EyeFluid,            // §8.1 (couleur d'eau déjà résolue par biome)
    pub heat_intensity: f32,            // §8.2
    pub mobs: &'a [MobInstance<'a>],
    pub particles: &'a [ParticleInstance],
    pub highlight_block: Option<IVec3>, // coin min ; None hors PLAYING/PAUSED/INVENTORY
}

/// Passe overlay : swapchain + profondeur HDR en lecture (pour nb-ui).
pub struct OverlayPass<'a> { /* cmd buffer, extent, render pass/format, accès atlas blocs pour les icônes */ }
```

Responsabilités de l'appelant (app / nb-world) :
- Maillage des chunks au format `ChunkVertex` avec l'encodage de couche §9 ; appel de `upload_chunk_mesh` à chaque changement ; `clear_chunk_meshes` + remaillage après `reload_resource_pack`.
- `SkyClock::update(dt)` à chaque tick fixe ; persistance de `time_of_day`/`day_length_seconds`.
- Calcul de `eye_fluid` (couleur d'eau du biome à la caméra) et du balayage de lave (17³ blocs).
- Collecte des mobs/particules.

Garanties de `nb-gfx` :
- Ordre des passes, états et formules strictement conformes aux §6–§16.
- `sky_params`, `heat_from_nearest_lava_dist_sq`, `Camera::{view, proj_gl}`, `TextureArrayCpu`, parseurs `pack.mcmeta`/JEM et construction des modèles de mobs sont des fonctions **pures sans GPU**, testées unitairement.

---

## 20. Tests d'acceptation suggérés
1. `sky_params` : vecteurs §7.4 (tolérance 1e-5), continuité aux frontières `sunHeight = ±0.1`.
2. `SkyClock` : 1200 s avec `day_length = 1200` → retour à la même heure ; `set_time(-10) == 1190`.
3. `heat` : `d² = 0 → 1`, `d² = 16 → 0.5`, `d² ≥ 64 → 0`.
4. `Camera::view` / `proj_gl` : égalité numérique avec des matrices JOML de référence (exportées une fois du Java).
5. Atlas Default : tuile 32, 46 couches (45 + manquante), `water_still` = dernière frame du PNG.
6. `pack.mcmeta` : `"a\"b\nc"` → `a"bnc` ; description objet → `""`.
7. Modèles mobs : nombres de sommets (cow = 6 boîtes × 36 = 216), UV de la face +Z de la tête de la vache.
8. Capture d'écran de référence Java vs Rust (même graine, même position, `/time noon|sunset|midnight`, sous l'eau, près de la lave) : écart moyen < 2/255 hors bruit/aurores (bruit aléatoire non reproductible → comparer avec aurores désactivées ou masquées).
