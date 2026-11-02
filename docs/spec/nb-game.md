# Spécification comportementale — crates `nb-game` et `nb-app`

Relevé du Java de référence (`Main`, `GameLoop`, `InputManager`, `ChatController`,
`state/*State.java`, `GameContext`, `WorldSaveManager.saveWorldState/applyPlayerState`).
`nb-game` contient toute la logique (testable sans fenêtre ni GPU) ; `nb-app` n'est que la
plateforme (winit, `nb_gfx::Renderer`, conversion des touches).

## 1. Boucle fixe (`GameLoop`)

```
step = 1/60 s ; acc = 0
chaque frame : frame = now - last ; last = now ; acc += frame
               tant que acc >= step : update(step) ; acc -= step
               render() ; window.update()   // present, reset clic/molette, pollEvents
```

* Pas d'interpolation au rendu.
* **Écart appliqué** (contrats §« Décisions » 4, « clamp optionnel de l'accumulateur ») : la
  durée d'une frame est bornée à 0,25 s avant d'entrer dans l'accumulateur (au plus 15 ticks par
  frame, pas de spirale de la mort). `FixedStep::java()` garde le comportement sans borne.
* La molette (`Window.scrollY`) est la **dernière** valeur reçue et n'est remise à 0 qu'après le
  rendu : tous les ticks d'une même frame la voient (reproduit).

## 2. Ordre de `Main.update(dt)` (chaque tick)

1. `inputManager.update()` (états d'actions courant/précédent, case sélectionnée, delta souris) ;
2. `masterRenderer.updateTime(dt)` — horloge jour/nuit, **dans tous les états** ;
3. `chatController.update(dt)` — ouverture / validation / annulation ; les commandes s'exécutent
   ici, avant l'ECS ;
4. redimensionnement (`camera.aspect`) ;
5. `stateManager.update(dt)` — seul PLAYING fait tourner l'ECS ;
6. `world.update(camera.position)` (chunks seulement) ;
7. remaillage (`ChunkMeshManager.update(world, activeChunks)`).

`PLAYING.update` : ÉCHAP (front) → PAUSED et retour ; E (front) → INVENTORY et retour ; puis
`world.update(camPos, dt)` = chunks, particules, particules ambiantes, position de spawn,
systèmes ECS (PlayerInput, Physics, PassiveAI, AggressiveAI, MobSpawn, Health) ; puis minuterie
du nom de l'objet tenu (1 s affiché + 0,5 s de fondu, relancée quand la case ou le bloc change).

`render()` : rendu 3D (+ UI de l'état) puis chat.

Init (`Main.initEngine`) : registres → caméra (FOV 70°) → monde du menu (graine
`nextInt(1e9)`, stockage en mémoire, joueur CREATIVE, jamais sauvegardé) → réglages appliqués
(FOV, distance de rendu, vsync, plein écran, ombres, bloom, touches, taille de fenêtre, pack)
→ état MAIN_MENU → `world.update` + remaillage. Fermeture : sauvegarde si un monde nommé est
chargé, arrêt des threads.

## 3. Entrées (`InputManager`)

Actions : FORWARD, BACKWARD, LEFT, RIGHT, JUMP, SPRINT, CROUCH, PAUSE, INVENTORY, PRIMARY,
SECONDARY, OPEN_CHAT, SUBMIT_CHAT, CANCEL_CHAT, DROP. Touches par défaut du Java puis
`applyKeyBindings(settings.keyBindings)` ; PRIMARY/SECONDARY = boutons gauche/droit.

* Hors saisie : chaque action = touche liée enfoncée ; ÉCHAP force PAUSE ; OPEN_CHAT = T, `/`
  ou `/` du pavé ; touches 1..9 → case ; molette : `slot -= signum(scroll)` avec bouclage.
* En saisie (`typingMode`) : seules SUBMIT_CHAT (Entrée / Entrée pavé) et CANCEL_CHAT (ÉCHAP)
  peuvent valoir vrai ; le delta souris est nul.
* Caractères (callback) : en saisie, ajoutés au tampon actif (principal ou secondaire). Tampon
  principal : un `t`/`T` en tête est ignoré sauf `suppressChatFilter` ; un second `/` après
  `"/"` est ignoré. Retour arrière (appui ou répétition) en saisie : efface un caractère.
* Écouteur de réaffectation : le prochain appui de touche lui est livré et rien d'autre.
* `resetMouse()` : le prochain tick prend la position courante comme référence (delta 0).

## 4. Chat (`ChatController`)

Fermé + OPEN_CHAT (front) → ouvert, saisie active, tampon = `"/"`. Ouvert : SUBMIT → journal
blanc de la ligne, exécution (`nb_ecs::execute_command`), réponse jaune si non nulle, journal
borné à 4, fermeture ; CANCEL → fermeture. Fermeture : saisie coupée, tampon vidé, minuterie
d'affichage 5 s (décrémentée seulement chat fermé). Le chat tourne dans **tous** les états.

## 5. États (`StateManager`)

`switchState(s)` : `exit(ancien)` puis `enter(nouveau)`. `previousState` (défaut MAIN_MENU) et
`pendingSettingsTab` sont dans le contexte.

| État | enter | update (logique) | exit |
|---|---|---|---|
| MAIN_MENU | souris libre | `menuRotation += dt·10` ; caméra (0,150,0), tangage 20, lacet = rotation ; PLAY → WORLD_SELECT ; SETTINGS (`previous = MAIN_MENU`) ; QUIT | caméra rot 0, pos (0,100,0) |
| WORLD_SELECT | curseur normal ; caméra rot 0, pos (0,100,0) ; liste des mondes | jouer / créer / éditer / supprimer / recréer / annuler → MAIN_MENU | fin de saisie |
| PLAYING | souris capturée (`resetMouse`) | voir §2 | — |
| PAUSED | souris libre | ÉCHAP → PLAYING ; RESUME ; SETTINGS (`previous = PAUSED`) ; MAIN MENU = sauvegarde, déchargement du monde (monde de menu neuf) puis MAIN_MENU — correctif, §8 | — |
| INVENTORY | souris libre ; curseur vidé | E ou ÉCHAP → objet tenu reposé, PLAYING ; clic → `InventoryCursor::left_click` | objet tenu reposé |
| SETTINGS | onglet CONTROLS si demandé, sinon GRAPHICS | ÉCHAP / Retour → `previous` ; Appliquer : réglages copiés, appliqués (distance, FOV, vsync, plein écran, ombres, bloom, touches, taille), sauvegardés, mobs supprimés si le registre est modifié, → `previous` ; onglets → RESOURCE_PACKS / MOB_SETTINGS | fin de réaffectation |
| RESOURCE_PACKS | liste des packs | Appliquer : si pack ≠ courant, rechargement de l'atlas, icônes, invalidation de tous les maillages, réglage sauvegardé ; → SETTINGS ; onglets | — |
| MOB_SETTINGS | — | ÉCHAP / quitter : modèles rechargés, mobs supprimés si modifié → SETTINGS ; onglets | fin de saisie |

Dans PAUSED / INVENTORY / menus l'ECS est figé ; les chunks et le maillage continuent (§2).

Charger un monde (`loadAndPlay`) : sauvegarde du monde nommé courant, arrêt de son gestionnaire
de chunks, libération des maillages, nouveau monde (`FileChunkStorage`), joueur créé avec la
caméra courante, distance de rendu, nom, triche, horloge (`dayLengthSeconds`, `timeOfDay`), état
du joueur (§6) → PLAYING.

## 6. Sauvegarde de l'état (`saveWorldState`, `applyPlayerState`)

* Sauvegarde : chunks sales écrits ; métadonnées relues (ou neuves) ; `lastPlayedAt` ;
  `dayLengthSeconds`/`timeOfDay` de l'horloge recopiés dans la config ; pieds, lacet/tangage de
  la caméra (degrés), santé, mode, vol ; inventaire (cases non vides).
* Application : jamais joué → spawn recherché (`findSpawnLocation`) et écrit dans la config ;
  pieds, caméra (yeux = pieds + 1,62 ; tangage/lacet), santé (`alive = santé > 0`), vol,
  inventaire (bloc inconnu → ignoré).

## 7. Façades (intégration)

* **UI** (`nb_game::ui::Ui`) : à chaque tick d'un état, l'UI reçoit l'état, une vue en lecture et
  les entrées, et renvoie des `UiCommand` (boutons) que `nb-game` exécute ; à chaque frame
  `Game::draw_ui()` lui fait remplir une `nb_ui::DrawList` (écran puis chat), convertie en
  `UiFrame` (`UiVertex` par `bytemuck`, lots → `UiDraw`) pour `nb_gfx::ui_pass::UiRenderer`.
  Implémentation du jeu : `nb_game::NbUi` (module `screens`, phase 6) — tampons de saisie
  recopiés depuis / vers l'`InputManager`, actions nb-ui → `UiCommand`, `WorldGenParams` ↔
  `WorldConfig` (`preset(type)` puis champs édités), icônes = couche `texture_layers[0]` de la
  table de maillage. Réaffectation : la touche capturée par `InputManager::start_rebind` est
  livrée par `Ui::key_pressed` **avant** `Ui::update` du même tick (callback GLFW du Java) ;
  Échap y est consommé puis revu enfoncé par l'écran, qui quitte (bogue Java conservé).
  `StubUi` (clavier, sans rendu) reste pour les tests et outils. nb-game dépend donc de nb-ui
  (pur CPU), voir `contrats.md`.
* **Rendu** : `Game::render_view()` fournit caméra, horloge, fluide à l'œil, chaleur, bloc visé,
  mobs (`MobInstance`) et particules ; `RenderView::frame_input` les convertit pour nb-gfx
  (`missing_scale` = `(widthX, height, widthZ)` de la config, mob sans config ignoré ; couche
  de particule = `particle_atlas().layer(type.texture)`, `Game::set_particle_layers`).
  `GameState::renders_world()` faux (WORLD_SELECT, SETTINGS, RESOURCE_PACKS, MOB_SETTINGS) :
  la plateforme ne rend que l'UI (`render_ui_frame`, image effacée en noir puis fond de terre
  plein écran, comme le Java qui n'appelle pas `MasterRenderer.render` dans ces états).
* **Plateforme** : `PlatformRequest::ReloadMobModels` (sortie de l'écran des mobs : modèle
  rechargé pour chaque mob du registre qui n'en a pas) et `ReloadMobModel(id)` (mob ajouté) ;
  au démarrage, les modèles sont rechargés depuis le `mobs.json` lu par `Game`.
* **Maillage** : `WorldChunks` implémente `nb_mesh::ChunkSource` sur le `ChunkManager` ;
  `WorldBiomes` implémente `BiomeSource`. Les `MeshAction` sont relayées à la plateforme.

## 8. Écarts

1. Accumulateur borné (§1).
2. Ordre des chunks actifs trié par (x, z) avant le remaillage (le Java itère un `HashMap`).
3. Changement de monde : génération de maillage incrémentée (`invalidate_all`) au lieu d'un
   simple `chunkMeshes.clear()` — un maillage en vol de l'ancien monde ne peut plus être affiché.
4. Graines des RNG de l'ECS : `EcsRngs::from_entropy()` (Java : `new Random()`), injectables.
5. Sauvegarde dont le `world.json` est illisible : la graine du monde est conservée (Java :
   `new WorldMetadata()`, graine 0).

6. **Bogue Java corrigé** (décision de Gabriel, 2026-11-02) : en Java, PAUSED → MAIN MENU
   laisse le monde chargé (`currentWorldName` reste posé) ; renommer ce monde puis en charger
   un autre le resauvegarde sous l'ancien nom, ce qui recrée son dossier (idem après une
   suppression, ou à la fermeture). Désormais `Game::unload_to_menu_world` sauvegarde le monde,
   arrête son gestionnaire de chunks, libère ses maillages et installe un monde de menu neuf
   (comme à l'initialisation : graine `nextInt(1e9)`, stockage en mémoire, CREATIVE, jamais
   sauvegardé). Seul effet visible : le fond du menu principal est ce nouveau monde au lieu du
   monde quitté.

## 9. Câblage et validation (phase 6)

* nb-app : `driver::Driver` (commun à la fenêtre et au rendu hors écran) crée le `Game`, branche
  `NbUi`, charge les textures de l'UI (`nb_ui::textures::load_all`, index = `UiTexture::index`)
  dans un `nb_gfx::ui_pass::UiRenderer`, relaie les demandes (réglages, pack : atlas des
  icônes rechargé, modèles de mobs) et rend chaque frame : scène + UI si
  `GameState::renders_world()`, sinon UI seule (`render_ui_frame`).
* Mode scripté (`norrblock --script <fichier> [--headless --capture-dir <dossier>]`, outil de
  test) : entrées simulées frame par frame, un tick fixe par frame (déroulé reproductible),
  captures PNG hors écran, `expect-state`, et deux commandes d'outillage (`spawn-mob`,
  `spawn-particles`) qui créent des entités / particules devant la caméra pour vérifier leur
  rendu — elles ne sont accessibles que par script, pas en jeu.
* Scénario `crates/nb-app/scripts/scenario.txt` (`e2e.sh`, Docker : lavapipe + couches de
  validation + Xvfb) : menu → choix du monde → création (nom, germe, triche) → jeu (HUD, blocs
  cassés, objet tenu) → mobs et particules → inventaire → chat → pause → réglages / packs /
  mobs → menu (sauvegarde + déchargement) → rechargement → quitter ; exécuté hors écran avec
  captures puis en fenêtre réelle (swapchain) sous Xvfb. Échec si un `expect-state` échoue ou
  si la validation Vulkan signale un message d'erreur.
