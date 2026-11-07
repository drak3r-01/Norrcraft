# Norrblock (Rust)

Réécriture à l'identique, sans aucune nouveauté, du jeu voxel Java *Norrblock* en Rust + Vulkan
(`ash`, `winit`, `glam`, `hecs`, `serde`) : monde procédural infini (8 biomes, 4 types de
monde), blocs à casser et poser, lumière, cycle jour/nuit, ombres, eau et lave, bloom, nuages,
mobs (renne, poisson, orque) configurables, modes survie et créatif, commandes, inventaire,
sauvegardes, packs de ressources.

- Jeu Java de référence : `drak3r-01/UnrealEngine6ProMax` (branche principale).
- Fidélité : mêmes constantes, mêmes seuils, même ordre de mise à jour ; la génération du
  monde est identique au Java pour une même graine (port exact de `java.util.Random`).
- Parité fonctionnelle : [`docs/PARITE.md`](docs/PARITE.md). Différences volontaires (bogues
  corrigés, bogues conservés) : [`docs/DIFFERENCES.md`](docs/DIFFERENCES.md). Architecture :
  [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). Spécifications par crate : `docs/spec/`.
- Sauvegardes : nouveau format versionné (aucune compatibilité avec les mondes Java).

## Architecture des crates

| Crate      | Rôle                                                                     |
|------------|--------------------------------------------------------------------------|
| `nb-core`  | `java.util.Random` exact, maths, `ChunkPos`, erreurs                      |
| `nb-data`  | Blocs, mobs, particules, réglages, packs (formats JSON du Java)           |
| `nb-world` | Bruit, biomes, génération, chunks, lumière, chargement asynchrone, sauvegardes |
| `nb-mesh`  | Greedy meshing, lumière locale, AO, ordonnanceur multi-thread, quadtree/frustum (sans GPU) |
| `nb-ecs`   | Entités, systèmes (joueur, physique, IA, apparition, santé), inventaire, commandes |
| `nb-gfx`   | Rendu Vulkan 1.3 : ombres, scène, fluides, mobs, particules, bloom, post-traitement, interface |
| `nb-ui`    | Widgets, police `stb_easy_font`, HUD, inventaire, chat, écrans (pur CPU)  |
| `nb-game`  | États, boucle fixe 60 Hz, entrées, chat, pont monde → maillage → rendu    |
| `nb-app`   | Binaire `norrblock` : fenêtre winit, pilote du renderer, mode hors écran  |

Sens des dépendances : `nb-core` ← `nb-data` ← `nb-world` ← `nb-ecs` ← `nb-game` ← `nb-app` ;
`nb-mesh`, `nb-ui` et `nb-gfx` ne dépendent que de `nb-core`/`nb-data` (et `nb-gfx` de
`nb-mesh`). Détails, threads et flux de données : [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Prérequis pour jouer

- Windows 10/11 64 bits (ou Linux) ;
- une carte graphique et un **pilote à jour compatibles Vulkan 1.3** (NVIDIA, AMD, Intel
  récents). `vulkan-1.dll` est installée par le pilote ; le Vulkan SDK n'est pas nécessaire
  pour jouer. Message « aucun GPU compatible » au démarrage → mettre à jour le pilote.

## Obtenir l'exécutable Windows

Le résultat est toujours le dossier autonome **`dist\Norrblock\`** (à copier où l'on veut,
par exemple zippé) :

```
dist\Norrblock\
  norrblock.exe            exécutable (shaders, blocs, textures de l'interface intégrés)
  resource_packs\Default\  pack de textures par défaut
  assets\                  données de référence (blocs, mobs, particules, icône)
  saves\settings.json      réglages par défaut
  saves\mobs_defaults.json valeurs de référence des mobs (ne pas modifier)
  LICENSE, LISEZMOI.txt
```

### A. Sans installer Rust : cross-compilation dans Docker

Prérequis : Docker Desktop et Git pour Windows (Git Bash). Depuis Git Bash, à la racine du
dépôt :

```bash
scripts/build-windows.sh
```

ou depuis PowerShell : `powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1`
(utilise Docker automatiquement si `cargo` est absent).

Le script construit une fois l'image `nb-rust-win` (Rust stable, cible
`x86_64-pc-windows-gnu`, MinGW-w64, Wine), compile `norrblock.exe` en release (CRT liée
statiquement : aucune DLL à fournir), vérifie que seules des DLL système Windows sont
importées, lance `norrblock.exe --help` sous Wine, puis assemble `dist\Norrblock\`
(`scripts/package.sh`). Première exécution : une dizaine de minutes ; ensuite quelques
secondes à quelques minutes.

Vérification optionnelle du paquet (vrai `norrblock.exe` sous Wine, Vulkan logiciel lavapipe,
scénario complet hors écran puis fenêtré) :

```bash
scripts/test-dist-windows-wine.sh        # captures dans dist/.wine-e2e/
```

### B. Avec Rust installé directement sous Windows (cible MSVC)

1. **Visual Studio Build Tools 2022** : <https://visualstudio.microsoft.com/fr/downloads/>
   → « Outils de génération pour Visual Studio 2022 » → charge de travail
   **« Développement Desktop en C++ »** (cocher MSVC v143 et le SDK Windows 10/11).
2. **Rust** : télécharger et lancer `rustup-init.exe` depuis <https://rustup.rs>, choix par
   défaut (`stable-x86_64-pc-windows-msvc`). Ouvrir un nouveau terminal et vérifier
   `cargo --version`. Le fichier `rust-toolchain.toml` du dépôt installe au besoin la bonne
   chaîne et `rustfmt`/`clippy`.
3. **Vulkan** : le pilote graphique suffit pour compiler et jouer (le chargeur Vulkan est
   ouvert dynamiquement à l'exécution, rien n'est lié à la compilation). Optionnel, pour le
   développement : le **Vulkan SDK** (<https://vulkan.lunarg.com/sdk/home#windows>) apporte
   les couches de validation, activées par défaut en debug ou avec `NB_GFX_VALIDATION=1`, et
   `glslangValidator` (seulement pour modifier les shaders : les SPIR-V sont précompilés).
4. Compiler et assembler le paquet, dans PowerShell à la racine du dépôt :

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
   ```

   (équivaut à `cargo build --release -p nb-app` puis à la copie dans `dist\Norrblock\`).
   Pendant le développement : `cargo run --release -p nb-app` depuis la racine du dépôt.

## Lancer le jeu

Double-cliquer sur `dist\Norrblock\norrblock.exe`, ou :

```powershell
cd dist\Norrblock
.\norrblock.exe
```

Le jeu lit et écrit `saves\` **dans le dossier courant** (comme le Java) : le lancer depuis son
dossier (c'est le cas avec un double-clic ; un raccourci doit avoir « Démarrer dans » = ce
dossier). `resource_packs\` est cherché dans le dossier courant, son parent, puis à côté de
l'exécutable. Une console accompagne la fenêtre : elle affiche le GPU utilisé, le dossier de
packs trouvé et les erreurs éventuelles.

Options de la ligne de commande :

| Option | Effet |
|---|---|
| `--saves <dossier>` | dossier des sauvegardes et réglages (défaut `saves`) |
| `--world <nom>` | charge (ou crée, en survie avec triche) ce monde et démarre directement en jeu |
| `--frames <n>` | quitte après n frames (test) |
| `--script <fichier>` | entrées simulées, un tick par frame (outil de test, voir `crates/nb-app/scripts/scenario.txt`) |
| `--headless` | rendu hors écran sans fenêtre ; `--capture-dir <dossier>` pour les captures PNG du script ; `--size LxH` |

Variable d'environnement : `NB_GFX_VALIDATION=1` active les couches de validation Vulkan si
elles sont installées (Vulkan SDK).

## Contrôles par défaut

Relevés de `saves/settings.json` par défaut (`nb_data::settings`) et des touches fixes de
l'`InputManager`. Les touches sont des **positions physiques QWERTY** comme dans le Java
(sur AZERTY : W = Z, A = Q, Q = A). Toutes les actions configurables se changent dans
**Settings → Controls**.

| Action | Touche | Configurable |
|---|---|---|
| Avancer / reculer / gauche / droite | W / S / A / D | oui |
| Sauter (double appui en créatif : vol on/off ; en vol : monter) | Espace | oui |
| Sprint | Ctrl gauche | oui |
| Descendre (en vol) | Maj gauche | oui |
| Inventaire | E | oui |
| Jeter (détruit un objet de la case choisie) | Q | oui |
| Pause | Échap (Échap met toujours en pause, même si Pause est réaffectée) | oui |
| Casser un bloc / frapper un mob | clic gauche | non |
| Poser un bloc | clic droit | non |
| Case de la barre | 1 à 9, molette | non |
| Ouvrir le chat / une commande | T ou `/` (y compris `/` du pavé) | non |
| Valider / annuler le chat | Entrée (ou Entrée du pavé) / Échap | non |

Commandes (monde créé avec « Allow Cheats ») : `/help`, `/time set <day|noon|sunset|night|
midnight|sunrise|n>`, `/time speed <s>`, `/gamemode <creative|survival>`, `/physics gravity
<mult>`, `/physics falldamage <on|off>`, `/player health|damage|walkspeed|sprintspeed|flyspeed|
jumpforce <v>`, `/default`, `/kill`, `/setworldspawn`.

## Configuration

### `saves/settings.json`

Écrit par **Settings → Apply** ; tout champ absent ou mal typé garde sa valeur par défaut
(supprimer le fichier rétablit tout).

| Champ | Défaut | Valeurs |
|---|---|---|
| `renderDistance` | 12 | chunks, 4..24 dans l'écran des réglages |
| `fov` | 90.0 | degrés, 60..110 |
| `sensitivity` | 0.1 | 0.02..0.50 |
| `shadowQuality` | `"LOW"` | `"OFF"`, `"LOW"`, `"HIGH"` |
| `bloomEnabled` | true | |
| `vsyncEnabled` | false | |
| `fullscreen` | false | plein écran exclusif sur le moniteur principal |
| `resourcePack` | `"Default"` | nom d'un dossier de `resource_packs/` |
| `windowWidth`, `windowHeight` | 1280, 720 | 1280×720, 1600×900, 1920×1080 dans l'écran |
| `key_FORWARD`, `key_BACKWARD`, `key_LEFT`, `key_RIGHT`, `key_JUMP`, `key_SPRINT`, `key_CROUCH`, `key_PAUSE`, `key_INVENTORY`, `key_DROP` | 87, 83, 65, 68, 32, 341, 340, 256, 69, 81 | codes de touche **GLFW** (lettres et chiffres = code ASCII majuscule, Espace 32, Échap 256, Entrée 257, Tab 258, flèches 262..265, F1..F12 290..301, Maj g. 340, Ctrl g. 341, Alt g. 342) |

### `saves/mobs.json`

Écrit par **Settings → Mobs** (n'existe pas tant qu'on n'y a rien modifié). Format
`{ "mobs": [ { "id": …, … }, … ] }` ; pour chaque objet, les champs présents remplacent les
valeurs du mob de même `id` ; un `id` inconnu crée un mob personnalisé à partir du renne.
Champs (voir `saves/mobs_defaults.json` pour les valeurs d'origine) : `displayName`,
`widthX`, `widthZ`, `height`, `maxHealth`, `medium` (`LAND`/`WATER`), `spawnMinWaterDepth`,
`spawnBiomes`, `spawnSurfaces`, `spawnMinY`, `spawnMaxY`, `spawnMaxWaterDepth` (habitat,
optionnels, voir `docs/FAUNE.md`), `maxCount`, `groupSizeMin`, `groupSizeMax`, `spawnScatterRadius`, `behaviorType`
(`PASSIVE`/`AGGRESSIVE`), `moveSpeedMin/Max`, `moveDurationMin/Max`, `pauseDurationMin/Max`,
`herding`, `herdRadius`, `herdBias`, `aggressorDetectRange`, `aggressorFleeSpeed`,
`playerFleeSpeed`, `playerFleeDurationMin/Max`, `aggroPlayerRange`, `chaseSpeed`,
`attackRange`, `attackDamage`, `attackCooldown`, `knockbackForce`. Un fichier JSON invalide
est ignoré (message `Could not load mobs.json` dans la console).

## Packs de ressources

Un pack est un dossier de `resource_packs/`, choisi dans **Settings → Resource Packs** :

```
resource_packs/MonPack/
  pack.mcmeta                                   { "pack": { "pack_format": 15, "description": "…" } }
  assets/minecraft/textures/block/*.png         textures de blocs (noms de assets/blocks.json)
  assets/minecraft/textures/entity/<id>/<id>.png  textures des mobs (optionnel)
  assets/minecraft/optifine/cem/<id>.jem        modèles de mobs OptiFine CEM (optionnel)
```

Une texture de bloc absente d'un pack apparaît en damier magenta (pas de repli vers Default,
comme en Java) ; les mobs se replient sur Default. Les textures animées (bandes verticales)
utilisent leur dernière image.

### Textures par défaut

Le pack `Default` contient les textures d'origine et, pour les blocs ajoutés par le générateur
de monde (voir `docs/WORLDGEN.md`), des textures procédurales (`scripts/gen_textures.py`).
`python3 scripts/import_faithful.py Faithful-32x.zip` remplace celles que le pack
[Faithful 32x](https://faithfulpack.net) fournit aussi et écrit `CREDITS-Faithful.txt` : leur
licence impose de les créditer clairement avec un lien vers leur site.

## Sauvegardes

`saves/<nom du monde>/world.json` (graine, mode, triche, configuration du monde, joueur,
inventaire, heure ; champ `"formatVersion": 1`) et `saves/<nom>/chunks/c_<cx>_<cz>.nbc`
(chunks modifiés ou visités). Le monde est sauvegardé en revenant au menu principal et à la
fermeture du jeu. Les mondes du jeu Java ne sont pas lisibles (nouveau format, sans
compatibilité). Pour sauvegarder ou déplacer un monde, copier son dossier.

## Développement

Aucune toolchain n'est requise sur la machine : tout passe par Docker.

```bash
scripts/cargo-docker.sh fmt --all -- --check
scripts/cargo-docker.sh clippy --workspace --all-targets -- -D warnings
scripts/cargo-docker.sh test --workspace
```

(`NB_TARGET_VOL=<volume>` choisit le volume Docker du dossier `target`.) Avec Rust installé :
les mêmes commandes avec `cargo` directement.

Tests GPU et de bout en bout (Docker, Vulkan logiciel lavapipe + couches de validation + Xvfb) :

```bash
NB_GFX_REQUIRE_GPU=1 crates/nb-gfx/scripts/vk-docker.sh test --workspace
NB_E2E_OUT=/chemin/captures crates/nb-app/scripts/e2e.sh   # scénario hors écran (PNG) puis fenêtré
scripts/test-dist-linux.sh                                  # même scénario depuis un paquet Linux
crates/nb-gfx/scripts/smoke-window.sh                       # visionneuse nb-gfx sous Xvfb
```

Les shaders sont précompilés en SPIR-V (`crates/nb-gfx/shaders/spv`, `ui/`) ; après une
modification des sources GLSL : `crates/nb-gfx/scripts/compile-shaders.sh` (un test vérifie
que les SPIR-V sont à jour).

## Licence

MIT, voir [`LICENSE`](LICENSE).

## Phase 9 : structures, faune, flore

19 structures (`docs/STRUCTURES.md`, curseur « Structure Density » à la création du monde), 42 animaux
supplémentaires (`docs/FAUNE.md`, `scripts/gen_mobs.py`) et une flore étendue (`docs/FLORE.md`,
`scripts/gen_textures_nature.py` pour les textures provisoires).
