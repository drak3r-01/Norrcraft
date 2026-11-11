# Spécification comportementale — crate `nb-ecs`

> Statut : référence normative pour la réécriture Rust. Rédigée à partir d'une lecture ligne à
> ligne de la version Java (`app/src/main/java/com/engine/core/{ecs,inventory,mob,command,particles}`,
> plus les points d'appel dans `world/World.java`, `world/WorldConfig.java`, `state/*`,
> `input/InputManager.java`, `graphics/Camera.java`, `GameLoop.java`, `Main.java`).
> Objectif : pouvoir réimplémenter **sans relire le Java**. Là où la doc Java (`docs/ecs.md`,
> `docs/mob-system.md`) contredit le code, **le code fait foi** (voir §21).

Conventions de ce document :

- `f32` = `float` Java, `f64` = `double` Java. Les types sont **à respecter** : les positions sont
  en `f64`, les vitesses / angles / santé en `f32`. Beaucoup de calculs mélangent les deux et
  passent par des casts `(float)` ; ils sont indiqués quand ils comptent.
- `floor(v)` = `Math.floor` suivi d'un cast `(int)` (arrondi vers −∞ puis troncature → `i32`).
- `lerp(a, b, t) = a + (b − a) · clamp(t, 0, 1)` (toujours clampé, en `f32`).
- `rand.f()` = `Random.nextFloat()` ∈ [0, 1) ; `rand.i(n)` = `Random.nextInt(n)` ∈ [0, n)
  (lève une exception si `n ≤ 0` en Java).
- Axe Y vers le haut. Gravité négative = vers le bas.
- « tick » = un appel de `EntityWorld.updateSystems(dt)`.

---

## 1. Périmètre et séparation logique / données

| Élément | Crate | Nature |
|---|---|---|
| Monde d'entités, composants, systèmes, ordonnancement | **nb-ecs** | logique |
| Physique des entités, collisions AABB/blocs, raycasts | **nb-ecs** | logique |
| Spawn/despawn, IA passive/agressive, santé, combat (`hitMobEntity`) | **nb-ecs** | logique |
| Inventaire (`Inventory`, `ItemStack`, règles d'empilement) | **nb-ecs** (module `inventory`) | logique |
| Interpréteur de commandes (`CommandProcessor`) | **nb-ecs** (module `command`) | logique (agit via traits) |
| Simulation des particules (`ParticleManager`, émission ambiante) | **nb-ecs** (module `particles`) | logique |
| `MobConfig`, `MobRegistry` (défauts intégrés, `mobs.json`, géométrie) | **nb-data** | donnée |
| `particles.json` → `ParticleType {id, texture, lifeTime, scale, gravity}` | **nb-data** | donnée |
| `blocks.json`, `BlockRegistry`, `BlockIds` (air, water, lava, lily_pad…) | **nb-data** | donnée |
| `GameSettings` (sensibilité, FOV) | **nb-data** | donnée |
| `JavaRandom` (LCG `java.util.Random`) | **nb-core** | utilitaire |
| Blocs du monde, chunks chargés, teinte de biome, surface | **nb-world** | fourni à nb-ecs via trait |
| Heure du jour (`setTime`, `setDayLengthSeconds`), FOV caméra, rendu des mobs/particules (couche de texture) | **nb-gfx** / **nb-game** | fourni via trait / lu depuis les composants |
| Chat (journal 4 lignes, 5 s), écran d'inventaire (rendu), écran de réglages mobs | **nb-ui** | UI ; la logique d'inventaire « curseur » est spécifiée ici §14.3 |
| Sauvegarde du joueur (position, santé, mode, vol, inventaire) | **nb-game** | lit/écrit les composants |

Règle : `nb-ecs` ne contient **aucune constante de donnée de mob** (toutes viennent de
`MobConfig`) et **aucune référence GPU/fenêtre**. Les constantes de *gameplay codées en dur*
dans les systèmes Java (frictions, seuils de chute, portées de raycast…) restent, elles, des
`const` dans `nb-ecs`.

---

## 2. Boucle, `dt` et place de l'ECS dans la frame

- `GameLoop` : pas de temps **fixe** `dt = 1.0 / 60.0` (`f64`), accumulateur sans plafond :
  `accumulator += frameTime; while (accumulator >= dt) { update(dt); accumulator -= dt; }` puis un
  `render()`. Pas d'interpolation, pas de limite de rattrapage (spirale possible).
- L'ECS reçoit `(float) dt` = `0.016666668f`.
- Ordre d'un `Main.update(dt)` :
  1. `inputManager.update()` (états d'actions, delta souris, slot sélectionné) ;
  2. `masterRenderer.updateTime(dt)` (heure du jour) ;
  3. `chatController.update(...)` → **les commandes s'exécutent ici**, avant l'ECS de la frame ;
  4. gestion du redimensionnement ;
  5. `stateManager.update(dt)` → seul l'état **PLAYING** appelle
     `world.update(camera.getPosition(), (float) dt)` ; PAUSED / INVENTORY / menus **ne font pas
     tourner l'ECS** (monde figé) ;
  6. `world.update(camera.getPosition())` (chunks seulement, sans dt) ; remesh.
- `World.update(playerPos, dt)` (PLAYING) :
  1. `chunkManager.update(playerPos)` ;
  2. `particleManager.update(dt)` ;
  3. `generateRandomParticles(playerPos)` ;
  4. `mobSpawnSystem.setPlayerPosition(playerPos)` ;
  5. `entityWorld.updateSystems(dt)`.
- `playerPos` est la **position de la caméra (yeux)** au début du tick, c.-à-d. celle calculée
  par `PlayerInputSystem` au tick **précédent**.
- Pendant la saisie du chat (`typingMode`), toutes les actions de jeu valent `false` et le delta
  souris vaut 0 : l'ECS continue de tourner, le joueur ne reçoit simplement aucune entrée.

---

## 3. `EntityWorld` (sémantique à reproduire)

- Entité = `int` ; `nextId` démarre à 0, `createEntity()` renvoie `nextId++`. **Jamais réutilisé.**
- Stockage : `HashMap<Integer, HashMap<Class, Object>>`. Une entité possède au plus un composant
  par type (`addComponent` remplace). `addComponent` sur une entité détruite est ignoré.
- `destroyEntity(id)` retire immédiatement l'entité et tous ses composants.
- `getComponent` renvoie `null` si absent ou entité inconnue.
- `query(T1, T2, …)` renvoie une **copie** (liste) des ids possédant tous les types, dans
  **l'ordre d'itération du `HashMap<Integer>` Java** (voir §20, B1). Détruire des entités pendant
  le parcours de cette liste est sûr ; un `getComponent` ultérieur sur une entité détruite renvoie
  `null`.
- `addSystem` ajoute en fin, `addSystemFirst` insère en tête. `updateSystems(dt)` appelle chaque
  système dans l'ordre de la liste.
- Les composants sont des objets mutables partagés par référence : une modification est visible
  immédiatement par la suite du même système et par les systèmes suivants.

---

## 4. Composants

| Composant | Champs (type = défaut) | Remarques |
|---|---|---|
| `TransformComponent` | `x, y, z: f64` (ctor), `yaw: f32 = 0`, `pitch: f32 = 0` | Position = **pieds** (bas de l'AABB, centre en X/Z). Pour les mobs `yaw`/`pitch` sont en **radians** ; pour le joueur ils ne sont pas utilisés par la simulation (l'orientation vit dans la caméra, en degrés) mais sont écrits au chargement. |
| `VelocityComponent` | `vx, vy, vz: f32 = 0`, `grounded: bool = false`, `targetVx, targetVy, targetVz: f32 = 0` | Les `target*` sont les consignes de l'IA ; inutilisés pour le joueur. |
| `ColliderComponent` | `widthX, widthZ, height, eyeOffset: f32` (ctor) | **Largeurs complètes** : l'AABB s'étend de ±`widthX/2`, ±`widthZ/2`, et de `y` à `y+height`. `eyeOffset` = 0 pour les mobs. |
| `HealthComponent` | `health: f32 = maxHealth`, `maxHealth: f32` (ctor), `alive: bool = true` | |
| `InventoryComponent` | `inventory: Inventory` (neuf, vide) | |
| `MobTypeComponent` | `mobId: String` (ctor, immuable) | Clé dans `MobRegistry` ; résolue à chaque usage (recherche linéaire). |
| `PassiveMobAIStateComponent` | `moveTimer: f32 = 0`, `moving: bool = false`, `fleeTimer: f32 = 0` | |
| `AggressiveMobAIStateComponent` | `moveTimer: f32 = 0`, `moving: bool = false`, `attackCooldown: f32 = 0` | |
| `PlayerComponent` | `camera` (réf.), `gameMode: GameMode` (ctor), `flying: bool = false`, `lavaTimer: f32 = 0`, `doubleTapTimer: f32 = f32::MAX`, `currentFov: f32` (ctor = baseFov), `targetedBlock: Option<Vec3> = None`, `placeTarget: Option<Vec3> = None`, `fluidAtEyeLevel: BlockId` (ctor = air) | |

`GameMode` : `SURVIVAL`, `CREATIVE` (noms utilisés tels quels dans les sauvegardes ; valeur
inconnue au chargement → `CREATIVE`).

### 4.1 Composition des entités

**Joueur** (`World.initPlayer`, une seule fois par monde) — composants dans cet ordre :
`Transform(cam.x, cam.y − 1.62, cam.z)`, `Velocity()`, `Collider(0.6, 0.6, 1.62f + 0.18f, 1.62)`
(hauteur = somme `f32` = 1.8000001f), `Health(10.0)`, `InventoryComponent()`,
`Player(camera, gameMode, toRadians(70°), AIR)`. Puis `PlayerInputSystem` est inséré **en tête**
des systèmes et `HealthSystem.onPlayerDeath = respawnPlayer`.

**Mob** (`MobSpawnSystem.spawnMob`) : `Transform(x, y, z)` avec `yaw = rand.f() · 2π` (en `f32`),
`Velocity()`, `Collider(cfg.widthX, cfg.widthZ, cfg.height, 0)`, `Health(cfg.maxHealth)`,
`MobType(cfg.id)`, puis `AggressiveMobAIState()` si `behaviorType == AGGRESSIVE`, sinon
`PassiveMobAIState()`.

### 4.2 Caméra (état lu/écrit par l'ECS)

- `rotation.x` = tangage (pitch) en **degrés**, `rotation.y` = lacet (yaw) en degrés.
- `rotate(dPitch, dYaw)` : `pitch = clamp(pitch + dPitch, −89, 89)` ; `yaw += dYaw` (non borné).
- Direction du regard (dérivée de `viewMatrix.positiveZ().negate()`, normalisée) :
  `dir = (cos p · sin y, −sin p, −cos p · cos y)` avec `p`, `y` en radians. Pitch positif = regarder
  vers le bas.
- `position` = yeux (`f32`). `fov` en radians.

---

## 5. Ordre des systèmes (le code fait foi)

Ordre effectif après `initPlayer` :

| # | Système | Requête principale |
|---|---|---|
| 1 | `PlayerInputSystem` | Player + Transform + Velocity + Collider + Inventory (premier résultat uniquement) |
| 2 | `PhysicsSystem` | Transform + Velocity + Collider (hors joueurs), puis séparation Transform + Collider |
| 3 | `PassiveMobAISystem` | PassiveAIState + Velocity + Transform + MobType |
| 4 | `AggressiveMobAISystem` | AggressiveAIState + Velocity + Transform + MobType |
| 5 | `MobSpawnSystem` | (toutes les secondes) |
| 6 | `HealthSystem` | Health |

Conséquences :
- l'IA écrit des consignes (`target*`) qui ne sont intégrées par la physique qu'au tick suivant
  (latence d'un tick) ;
- un mob qui vient d'apparaître n'est simulé (physique/IA) qu'au tick suivant ;
- une entité tuée pendant le tick (coup du joueur en 1, morsure en 4, chute en 2) est détruite
  en 6 dans le même tick ; elle a quand même été déplacée/IA-tickée entre-temps.

---

## 6. `PlayerInputSystem`

Constantes :

| Nom | Valeur |
|---|---|
| `DEFAULT_FOV` | `toRadians(70)` (si `settings == null`) |
| `FOV_SPRINT_OFFSET` | `toRadians(25)` |
| `LAVA_DAMAGE_INTERVAL` | 0.5 s |
| `LAVA_DAMAGE_AMOUNT` | 1.0 |
| `FALL_DAMAGE_THRESHOLD` | −11.5 (vitesse verticale) |
| `FALL_DAMAGE_SCALE` | 0.4 |
| `DOUBLE_TAP_WINDOW` | 0.35 s |
| `MOB_HIT_RANGE` | 5.0 |
| portée raycast blocs | 6.0 |
| sensibilité par défaut | 0.1 (si `settings == null`) |

État du système (hors composants) : `mouseLocked = true` (mis à `false` à l'ouverture de
l'inventaire, `true` au retour), référence `settings` (partagée, donc modifiable à chaud).

Déroulé d'un tick (si aucune entité joueur : ne rien faire) — **l'ordre est normatif** :

1. **Rotation** — si `mouseLocked` :
   `camera.rotate(mouseDeltaY · sens, mouseDeltaX · sens)` (delta en pixels depuis le tick
   précédent ; Y écran vers le bas → pitch augmente → regard vers le bas).
2. **Entrée locale** : `localDz −= 1` si FORWARD, `+= 1` si BACKWARD ; `localDx −= 1` si LEFT,
   `+= 1` si RIGHT. Si la norme > 0, normaliser.
3. **Double saut / vol** : `doubleTapTimer += dt`. Si JUMP *vient d'être pressé* :
   si `doubleTapTimer < 0.35` **et** `gameMode == CREATIVE` → `flying = !flying` et si `flying`
   devient vrai `vy = 0` ; puis dans tous les cas `doubleTapTimer = 0`.
4. **Direction monde** : `yaw = toRadians(camera.yaw)` ;
   `targetDx = localDx·cos(yaw) − localDz·sin(yaw)`, `targetDz = localDx·sin(yaw) + localDz·cos(yaw)`.
5. Lire `WorldConfig` : `walkSpeed`, `sprintSpeed`, `flySpeed`, `flyVertical = flySpeed · 0.6`,
   `jumpForce`, `gravity`.
6. **Mouvement en vol** (`flying`) — friction 8.0 :
   - `vx = lerp(vx, targetDx·flySpeed, 8·dt)` ; idem `vz` ;
   - `movePlayerAxis(dx = vx·dt)` puis `movePlayerAxis(dz = vz·dt)` ;
   - `targetVY = (+flyVertical si JUMP maintenu) + (−flyVertical si CROUCH maintenu)` ;
   - `vy = lerp(vy, targetVY, 8·dt)` ; `movePlayerAxis(dy = vy·dt)`.
   - Pas de gravité, pas de dégâts de chute. Le vol ne s'arrête pas en touchant le sol.
7. **Mouvement au sol / nage** (sinon) :
   - `inFluid = isInFluid(position actuelle)` (eau **ou** lave dans l'AABB, §7.4) ;
   - `sprint = SPRINT maintenu && localDz < 0 && !inFluid` (sprint seulement vers l'avant, après
     normalisation : avant+côté compte) ;
   - `speed = sprint ? sprintSpeed : walkSpeed` ; si `inFluid` : `speed *= 0.4` ;
   - `friction = inFluid ? 8.0 : (grounded ? 15.0 : 2.5)` (grounded du tick précédent) ;
   - `vx = lerp(vx, targetDx·speed, friction·dt)`, idem `vz` ; `movePlayerAxis` X puis Z ;
   - `wasGrounded = grounded` ;
   - si `inFluid` : si JUMP maintenu → `vy += jumpForce·1.5·dt`, puis si `vy > jumpForce·0.4`
     → `vy = jumpForce·0.4` ; sinon si `!grounded` → `vy += gravity·0.2·dt` ; ensuite
     `if (vy < −3.0) vy = −3.0` ;
   - sinon : si JUMP maintenu **et** `grounded` → `vy = jumpForce ; grounded = false` (saut
     maintenu = sauts répétés) ; puis **toujours** `vy += gravity·dt` ;
   - `preY = vy` ; `movePlayerAxis(dy = preY·dt)` ;
   - **Dégâts de chute** si `!wasGrounded && grounded && preY < −11.5 && gameMode == SURVIVAL &&
     health != null && worldConfig.fallDamageEnabled` :
     `health −= (−preY − 11.5) · 0.4` ; si `health < 0` → 0 ; si `health ≤ 0` → `alive = false`.
8. **Caméra** : `camera.position = (x, y + eyeOffset, z)` (cast `f32`).
9. **FOV** : `baseFov = settings ? toRadians(settings.fov) : DEFAULT_FOV` ;
   `sprintFov = baseFov + toRadians(25)` ; `hSpeed = sqrt(vx² + vz²)` ;
   `maxSpeed = flying ? flySpeed : sprintSpeed` ;
   `fovTarget = lerp(baseFov, sprintFov, max(0, min(hSpeed / maxSpeed, 1)))` ;
   `currentFov = lerp(currentFov, fovTarget, 1 − exp(−10·dt))` ; `camera.setFov(currentFov)`.
10. **Lave** : si `SURVIVAL && isInLava(AABB) && health != null` : `lavaTimer += dt` ; si
    `lavaTimer ≥ 0.5` → `health −= 1.0` (clamp 0, `alive = false` si ≤ 0) et `lavaTimer −= 0.5`
    (un seul dégât par tick au maximum). Sinon `lavaTimer = 0`. Le premier dégât survient donc
    après 0.5 s d'immersion.
11. **Raycast blocs** depuis `camera.position` (déjà mise à jour en 8) selon `dir` (§4.2), portée
    6.0 (§8.1) → `targetedBlock = hit`, `placeTarget = adjacent` (ou `None` tous deux).
12. `fluidAtEyeLevel = getFluidAtEyeLevel(x, y + eyeOffset, z)` (bloc à `floor` de ce point :
    WATER ou LAVA, sinon AIR).
13. **Actions** — seulement si `mouseLocked` :
    - PRIMARY *vient d'être pressé* : `hitMob = raycastMobEntity(origin, dir)` (§8.2). Si un mob
      est touché → `World.hitMobEntity(mob)` (§9.1) ; **sinon** si `targetedBlock` :
      `broken = getBlock(t)`, `setBlock(t, AIR)`, `inventory.addBlock(broken)` (aucune
      vérification de mode ni de dureté — la bedrock se casse ; en créatif le bloc s'ajoute aussi).
    - DROP *vient d'être pressé* : `inventory.removeOne(27 + selectedSlot)` (l'objet est
      **détruit**, aucune entité d'objet).
    - SECONDARY *vient d'être pressé* et `placeTarget` : `held = hotbar[selectedSlot]` ; si
      `held != null` et que le cube unité `placeTarget` **n'intersecte pas l'AABB du joueur**
      (`isAABBIntersectingBlock`, inégalités strictes) → `setBlock(placeTarget, held.block)` puis
      `removeOne(27 + selectedSlot)` (consommé même en créatif). Les mobs ne bloquent pas la pose ;
      la case cible peut être de l'eau/lave (remplacée).
    - Casser et poser peuvent se produire dans le même tick ; la pose utilise le `placeTarget`
      calculé en 11 (avant la casse).

`selectedSlot` ∈ [0, 8] : touches 1–9, molette `selected −= signum(scroll)` avec bouclage
(< 0 → 8, > 8 → 0). Géré par l'input (nb-game), lu par l'ECS.

---

## 7. `PhysicsSystem`

Constantes : `FRICTION_GROUND = 6.0`, `FRICTION_AIR = 2.0`, `FRICTION_WATER = 4.0`,
`FALL_DAMAGE_THRESHOLD = 10.0`, `FALL_DAMAGE_SCALE = 0.5`, `RAYCAST_STEP = 0.05`.

### 7.1 Tick

Pour chaque entité de `query(Transform, Velocity, Collider)` (ordre §3) :
- si elle a un `PlayerComponent` → ignorer (le joueur est entièrement géré par §6) ;
- si `isMobWater` (a un `MobType` dont la config existe et `medium == WATER`) → **nage** :
  `vx = lerp(vx, targetVx, 4·dt)`, `vz = lerp(vz, targetVz, 4·dt)`, `vy = lerp(vy, targetVy, 4·dt)` ;
  `moveAxisSwim(X: vx·dt)`, `moveAxisSwim(Z: vz·dt)`, `moveAxisSwim(Y: vy·dt)`.
  Aucune gravité, **même hors de l'eau** (le milieu vient de la config, pas du bloc).
- sinon (LAND, AIR, config inconnue, entité sans MobType) → **terrestre** :
  `friction = grounded ? 6 : 2` ; `vx = lerp(vx, targetVx, friction·dt)`, idem `vz` ;
  `vy += gravity·dt` (gravité du `WorldConfig`) ; `moveAxis(X: vx·dt)`, `moveAxis(Z: vz·dt)`,
  `moveAxis(Y: vy·dt)`. Les arguments sont évalués **au moment de chaque appel** : un saut
  automatique déclenché en X/Z modifie `vy` et donc le déplacement Y du **même** tick.

Puis `pushApartEntities()` (§7.5), une seule fois, après tous les déplacements.

### 7.2 `moveAxis` (mobs terrestres)

`jumpForce = cfg ? cfg.jumpForce : 9.0` (cfg = `WorldConfig`, toujours non nul en pratique →
8.4 par défaut ; **modifié par `/player jumpforce`**).

- X (si `dx ≠ 0`) : si pas de collision en `(x+dx, y, z)` → `x += dx` ; sinon `vx = 0` et si
  `grounded` → `vy = jumpForce` (**saut automatique** contre un obstacle).
- Z (si `dz ≠ 0`) : idem avec `vz`.
- Y (si `dy ≠ 0`) : si pas de collision → `y += dy ; grounded = false`. Sinon :
  si `dy < 0` : si `!grounded && health != null && fallDamageEnabled` et `|vy| > 10` →
  `health −= (|vy| − 10)·0.5` puis `alive = false` si `health ≤ 0` (pas de clamp à 0) ; puis
  `grounded = true`. Dans tous les cas de collision Y : `vy = 0`.
- **Pas de recalage** contre la surface : en cas de collision le déplacement est simplement
  annulé ; l'entité s'approche du sol par pas de plus en plus petits (`vy` repart de 0 puis
  `gravity·dt` → ~0.0089 bloc par tick). Les dégâts de chute des mobs ne dépendent ni du mode de
  jeu ni du joueur.

### 7.3 `moveAxisSwim`

Comme `moveAxis` mais : collision X → `vx = 0`, Z → `vz = 0`, Y → `vy = 0` ; pas de saut, pas
de `grounded`, pas de dégâts.

### 7.4 Tests de collision et de fluide

Toutes ces fonctions balaient les cellules entières :
```
minX = floor(cx − w/2), maxX = floor(cx + w/2)   // w/2 calculé en f32
minY = floor(cy),       maxY = floor(cy + height)
minZ = floor(cz − d/2), maxZ = floor(cz + d/2)
for x in minX..=maxX, y in minY..=maxY, z in minZ..=maxZ
```
(bornes **inclusives** : une AABB qui *touche* exactement une face entière de bloc la compte).

- `checkCollision` (mobs) : collision si un bloc non nul est `solid` (`blocks.json` : les feuilles
  et les cactus sont solides ; eau, lave, plantes, nénuphar ne le sont pas).
- `checkPlayerCollision` : ignore null/AIR ; pour `LILY_PAD` : collision si
  `pMinY < y + 0.015 && pMaxY > y` (avec `pMinY = (f32)cy`, `pMaxY = (f32)(cy + height)`) — tranche
  fine de 0.015 au bas de la cellule ; sinon collision si `solid`.
- `isInFluid` : vrai si une cellule est WATER ou LAVA. `isInLava` : vrai si une cellule est LAVA.
- `getFluidAtEyeLevel(ex, ey, ez)` : bloc en `(floor ex, floor ey, floor ez)` si WATER/LAVA,
  sinon AIR.
- `isAABBIntersectingBlock(bx, by, bz, cx, cy, cz, col)` :
  `cx−hx < bx+1 && cx+hx > bx && cy < by+1 && cy+h > by && cz−hz < bz+1 && cz+hz > bz`.
- `getBlock` hors chunk chargé ou hors `y ∈ [0, 128)` → AIR.

`movePlayerAxis` (joueur) : X/Z comme `moveAxisSwim` mais avec `checkPlayerCollision`.
Y (si `dy ≠ 0`) : si `dy < 0` **et** le chunk de `(floor x, floor z)` n'est pas chargé →
`grounded = true ; vy = 0` (sol virtuel, aucune position modifiée). Sinon : pas de collision →
`y += dy ; grounded = false` ; collision → si `dy < 0` `grounded = true` ; `vy = 0`.
Si `dy == 0` exactement, `grounded` est inchangé.

### 7.5 Séparation des entités (`pushApartEntities`)

`list = query(Transform, Collider)` (joueur inclus). Pour `i < j` (paires ordonnées selon la
liste, positions modifiées **en place** au fil de la boucle) :
1. si l'une est « agressive aquatique » (config WATER + AGGRESSIVE) et l'autre « passive
   aquatique » (WATER + PASSIVE) → ignorer la paire (l'orque doit pouvoir atteindre sa proie) ;
2. `dx = ta.x − tb.x`, `dz = ta.z − tb.z` ; `overlapX = (hxA + hxB) − |dx|`,
   `overlapZ = (hzA + hzB) − |dz|` ; si `overlapX ≤ 0 || overlapZ ≤ 0` → ignorer ;
3. si `ta.y + ha ≤ tb.y || tb.y + hb ≤ ta.y` → ignorer ;
4. `pushA = aPlayer ? 0 : (bPlayer ? 1 : 0.5)`, `pushB = bPlayer ? 0 : (aPlayer ? 1 : 0.5)` ;
5. si `overlapX < overlapZ` : `sep = overlapX · (dx == 0 ? 1 : signum dx)` ; `ta.x += sep·pushA` ;
   `tb.x −= sep·pushB`. Sinon (égalité incluse) même chose sur Z.

Aucune vérification contre les blocs (un mob peut être poussé dans un mur). Les vitesses ne sont
pas modifiées.

---

## 8. Raycasts

### 8.1 Raycast blocs (`PhysicsSystem.raycast`)

Marche à pas fixe (pas un DDA) :
```
step = 0.05f ; distance = 0f ; current = origin (Vec3 f32)
last = (floor origin.x, floor origin.y, floor origin.z)
while distance < maxDistance:
    c = (floor current.x, floor current.y, floor current.z)
    b = getBlock(c)
    if b != null && b != AIR && b != WATER && b != LAVA:
        return (hit = c, adjacent = last)
    last = c
    current += dir · step      // en f32, composante par composante
    distance += step           // accumulation f32
return None
```
- Portée joueur : `maxDistance = 6.0` → ~120 échantillons (la dernière distance testée dépend de
  l'accumulation `f32` ; à reproduire en `f32`).
- Touche tout bloc non-air non-fluide, **y compris les non solides** (herbes, fleurs, nénuphar).
- `adjacent` = cellule de l'échantillon précédent : peut être diagonale (pas forcément une face
  voisine) et, si l'origine est déjà dans un bloc, égale à `hit`.
- Un coin de bloc traversé sur moins de 0.05 peut être manqué.

### 8.2 Raycast mobs (`raycastMobEntity`)

Pour chaque entité de `query(Transform, MobType, Collider)` : AABB
`[x−hx, y, z−hz] .. [x+hx, y+height, z+hz]` (en `f32`), test par slabs :
```
tNear = −∞, tFar = +∞
pour chaque axe a :
  si d.a != 0 : t1 = (min.a − o.a)/d.a ; t2 = (max.a − o.a)/d.a ; ordonner ;
                tNear = max(tNear, t1) ; tFar = min(tFar, t2)
  sinon si o.a < min.a || o.a > max.a : pas de touche
si tNear > tFar || tFar < 0 : pas de touche
t = tNear >= 0 ? tNear : tFar
```
Retient l'entité de plus petit `t` avec `0 ≤ t < 5.0` (strict ; à égalité, la première
rencontrée). **Les blocs ne sont pas pris en compte** : on frappe un mob à travers un mur, et un
mob touché est prioritaire sur le bloc visé même si le bloc est plus proche.

---

## 9. Combat

### 9.1 Coup du joueur (`World.hitMobEntity(entity)`)

1. Si pas de `HealthComponent` → rien.
2. `health −= worldConfig.attackDamage` (défaut 1.0, `/player damage`).
3. Si `health ≤ 0` → `alive = false` ; **sinon** si l'entité a un `PassiveMobAIState` →
   `triggerPlayerFlee` (§11.4). Les agressifs ne fuient pas.
4. Recul (même si mort) : si Velocity, Transform du mob et Transform du joueur existent :
   `dx = mob.x − player.x`, `dz = mob.z − player.z`, `dist = sqrt(dx²+dz²)` ; si `dist > 0.01` :
   `vx += dx/dist · 5.0`, `vz += dz/dist · 5.0`, `vy = 4.0`.
   (Ajouté **après** les consignes de fuite ; pour un mob aquatique `vy` sera ensuite lissé vers
   `targetVy`.)

Aucun délai entre deux coups : un coup par appui (front montant de PRIMARY).

### 9.2 Morsure d'un agressif — voir §12.2.

---

## 10. `MobSpawnSystem`

Constantes : `SPAWN_INTERVAL = 1.0 s`, `MIN_SPAWN_RADIUS = 16`. État : `timer = 0`,
`playerPos` (copie de la position caméra fournie avant chaque tick), un `Random` propre.

Tick :
1. `timer += dt` ; si `timer < 1.0` → fin. Sinon `timer = 0` (pas de report du reliquat).
2. **Despawn** : `maxDistSq = (f32)(despawnRadius²)` (`mobDespawnRadius`, défaut 64) ; pour chaque
   entité de `query(MobType, Transform)` : `dx = (f32)t.x − playerPos.x`, `dz` idem (distance
   **horizontale**, en `f32`) ; si `dx²+dz² > maxDistSq` → `destroyEntity`.
3. Pour chaque `cfg` de `MobRegistry.getAll()` **dans l'ordre du registre** (reindeer, fish,
   orca, puis mobs personnalisés dans l'ordre du fichier) : `count` = nombre d'entités dont
   `mobId == cfg.id` ; si `count < cfg.maxCount` → `trySpawn(cfg, count)`.

`trySpawn(cfg, currentCount)` :
```
radius = worldConfig.mobSpawnRadius            // défaut 64
attempts = (cfg.groupSizeMax == 1) ? 15 : 10
repeat attempts:
    ox = rand.i(2·radius) − radius             // ∈ [−radius, radius−1]
    oz = rand.i(2·radius) − radius
    if |ox| < 16 && |oz| < 16: continue        // trou carré autour du joueur
    cx = floor(playerPos.x) + ox ; cz = floor(playerPos.z) + oz
    spawnY = findSpawnY(cfg, cx, cz) ; if spawnY < 0: continue
    room = cfg.maxCount − currentCount
    range = cfg.groupSizeMax − cfg.groupSizeMin
    groupSize = cfg.groupSizeMin + rand.i(min(range + 1, room))
    for k in 0..groupSize:
        gx = cx + (scatter > 0 ? rand.i(2·scatter + 1) − scatter : 0)
        gz = cz + (scatter > 0 ? rand.i(2·scatter + 1) − scatter : 0)
        gy = findSpawnY(cfg, gx, gz) ; if gy < 0: continue
        yPos = (medium == LAND) ? gy : gy + 0.5
        spawnMob(cfg, gx + 0.5, yPos, gz + 0.5)
    return                                     // une seule origine réussie par appel
```
(`scatter = cfg.spawnScatterRadius`.) Le groupe peut dépasser `maxCount` si `groupSizeMin > room`.
Aucun test d'espace libre (AABB) ni de lumière ; les membres d'un groupe peuvent se superposer.

`findSpawnY(cfg, x, z)` : LAND → `findSurfaceY` ; WATER → `findWaterY(minDepth =
cfg.spawnMinWaterDepth)` ; AIR → −1 (jamais d'apparition).

`findSurfaceY(x, z)` : pour `y` de 127 à 1 (décroissant) : si `getBlock(x,y,z)` n'est pas solide
→ continuer ; si `getBlock(x,y+1,z) == AIR` → renvoyer `y + 1`. Sinon −1. (Un sol solide sous de
l'eau est sauté : la recherche continue plus bas et peut aboutir dans une grotte ; les feuilles
comptent comme sol.)

`findWaterY(x, z, minDepth)` : `minDepth = max(minDepth, 1)` ; pour `y` de 127 à `minDepth`
(décroissant) : si `getBlock(x,y,z) != WATER` → continuer ; si les blocs `y+1 .. y+minDepth−1`
sont tous WATER → renvoyer `y + (minDepth − 1) / 2` (division entière). Sinon −1. En pratique le
premier `y` valide est `surface − (minDepth−1)`, donc le mob apparaît vers le milieu des
`minDepth` blocs supérieurs de la colonne (+0.5 à l'appel).

Mobs intégrés : poisson `minDepth 2` → `y_base = surface−1`, `+0` → apparition à `surface−1+0.5` ;
orque `minDepth 5` → `surface−4 + 2 + 0.5`.

---

## 11. `PassiveMobAISystem`

Constante : `LOOK_AHEAD = 1.5`. Un `Random` propre.

### 11.1 Tick

1. `passives = query(PassiveAIState, Velocity, Transform, MobType)` ; vide → fin.
2. Instantané des agresseurs : pour `query(AggressiveAIState, Transform, MobType)` →
   `(référence Transform, config)` (config pouvant être `null`).
3. Pour chaque passif (config `null` → ignorer) :
   - **Agresseur le plus proche** (seulement si `cfg.aggressorDetectRange > 0`) : parmi les
     agresseurs de **même milieu** (`a.cfg.medium == cfg.medium` ; une config d'agresseur nulle
     ferait planter le Java), distance **horizontale** `d = sqrt(dx²+dz²)` avec
     `dx = self.x − a.x`, `dz = self.z − a.z` (en `f32`) ; le plus petit `d` strict gagne ;
     direction de fuite `(dx, dz)·(d > 0.01 ? 1/d : 0)`. Défaut : `nearest = f32::MAX`.
   - **Centroïde du troupeau** (si `cfg.herding`) : autres passifs de même `mobId` avec
     `dx²+dz² ≤ herdRadius²` (horizontal, `f32`) ; moyenne de leurs `x`, `z` ;
     `herdmates` = leur nombre.
   - `tick(...)`.

### 11.2 `tick` — priorités

1. **Fuite d'agresseur** si `nearest < cfg.aggressorDetectRange` :
   `targetVx = fleeX·aggressorFleeSpeed`, `targetVy = 0`, `targetVz = fleeZ·aggressorFleeSpeed` ;
   `yaw = π/2 − atan2(fleeZ, fleeX)` ; `pitch = 0` ; si `fleeTimer > 0` → `fleeTimer −= dt` ;
   `moving = false` ; `moveTimer = 0` ; **fin**. (Réévalué à chaque tick : fuite continue.)
2. **Fuite du joueur** si `fleeTimer > 0` : `fleeTimer −= dt` ; si `fleeTimer ≤ 0` →
   `target* = 0`, `pitch = 0`, `moving = false`, `moveTimer = 1.5`. **Fin** (les consignes posées
   par `triggerPlayerFlee` restent actives pendant la fuite).
3. **Cycle errance/pause** : `moveTimer −= dt` ; si `moveTimer > 0` → fin.
   - si `moving` : `moving = false` ; `target* = 0` ; `pitch = 0` ;
     `moveTimer = pauseMin + rand.f()·(pauseMax − pauseMin)`.
   - sinon : `dir = chooseIdleDirection()` ; si `None` → `moveTimer = 1.0`, fin (reste à
     l'arrêt). Sinon `(h, v) = dir` ; `speed = moveSpeedMin + rand.f()·(moveSpeedMax − moveSpeedMin)` ;
     - LAND : `targetVx = cos h·speed`, `targetVz = sin h·speed`, `targetVy = 0`, `yaw = π/2 − h` ;
     - sinon : `hs = cos v·speed` ; `targetVx = cos h·hs`, `targetVz = sin h·hs`,
       `targetVy = sin v·speed`, `yaw = π/2 − h`, `pitch = −v` ;
     - `moving = true` ; `moveTimer = moveDurMin + rand.f()·(moveDurMax − moveDurMin)`.

Comme `moveTimer` démarre à 0, un mob neuf choisit une direction dès son premier tick.

### 11.3 Choix de direction

`buildHAngle` : si `herding && herdmates > 0` : `to = centroïde − position` (f32),
`d = |to|` ; si `d > 1.5 && rand.f() < herdBias` (le tirage n'a lieu que si `d > 1.5`) →
`atan2(toZ, toX) + (rand.f() − 0.5)·(π/2.5)`. Sinon `rand.f()·2π`.

`chooseIdleDirection` :
- LAND → `(hAngle, 0)`.
- WATER (et AIR) : `v = (rand.f() − 0.5)·(π/2.5)` (±36°). Jusqu'à 8 essais :
  1. point d'anticipation `look = pos + (cos h·1.5·cos v, sin v·1.5, sin h·1.5·cos v)` (f64) ;
     si le bloc en `floor(look)` est WATER → `(h, v)` ;
  2. si `v != 0` : `v = 0` ; tester `(x + cos h·1.5, floor(y), z + sin h·1.5)` ; WATER → `(h, 0)` ;
  3. `h += π/4` ; `v = (rand.f() − 0.5)·(π/2.5)`.
  Échec des 8 → `None`.

### 11.4 `triggerPlayerFlee(entity)` (appelé par §9.1)

Nécessite PassiveAIState, Velocity, Transform, MobType et une config, sinon rien.
- LAND : `a = rand.f()·2π` ; `targetVx = cos a·playerFleeSpeed`, `targetVz = sin a·playerFleeSpeed`,
  `yaw = π/2 − a` (`targetVy` et `pitch` inchangés).
- sinon : `h = rand.f()·2π`, `v = (rand.f() − 0.5)·(π/2)` ; `hs = cos v·speed` ;
  `targetVx = cos h·hs`, `targetVz = sin h·hs`, `targetVy = sin v·speed`, `yaw = π/2 − h`,
  `pitch = −v`.
- `moving = true` ; `fleeTimer = fleeMin + rand.f()·(fleeMax − fleeMin)` (`moveTimer` inchangé).

La direction de « fuite » est **aléatoire**, pas opposée au joueur (seul le recul de §9.1 éloigne).

---

## 12. `AggressiveMobAISystem`

Un `Random` propre.

### 12.1 Tick

1. `aggressors = query(AggressiveAIState, Velocity, Transform, MobType)` ; vide → fin.
2. `preys = query(PassiveAIState, Transform, Health)` ; `players = query(Player, Transform,
   Velocity, Collider)`.
3. Pour chaque agresseur (config nulle → ignorer) :
   - `attackCooldown = max(0, attackCooldown − dt)` ;
   - **proie** : parmi `preys` avec `alive`, un `MobType` et une config de **même milieu** :
     plus petite distance **3D** au carré (strict `<`, première trouvée à égalité) ;
   - **joueur** (si `aggroPlayerRange > 0`) : pour chaque joueur en `SURVIVAL` (et, si l'agresseur
     est WATER, « joueur dans l'eau » = bloc en `(floor x, floor(y + height·0.4), floor z)` est
     WATER) : si `dist3D² ≤ aggroPlayerRange²` → le joueur **remplace** la cible (même s'il est
     plus loin que la proie) ;
   - cible trouvée → `tickChase(cible, sqrt(distSq), isPlayer)` ; sinon `tickIdle`.

### 12.2 `tickChase`

`d = cible − self` (3D, f64), `dist` (3D).
1. **Attaque** si `dist < attackRange && attackCooldown ≤ 0` et la cible a un `Health` :
   `health −= attackDamage` ; `alive = false` si `≤ 0` (pas de clamp) ; si la cible a une
   Velocity et `dist > 0.01` : `vx += dx/dist·knockbackForce`, `vz += dz/dist·knockbackForce`,
   `vy = 5.0` (recul dans le sens agresseur → cible, normalisé en **3D**) ;
   `attackCooldown = cfg.attackCooldown`. (Branches joueur et proie identiques.)
2. **Poursuite** si `dist > 0.01` : `h = atan2(dz, dx)`, `hDist = sqrt(dx²+dz²)`,
   `v = atan2(dy, hDist)` ; `yaw = π/2 − h` ;
   - LAND : `targetVx = dx/dist·chaseSpeed`, `targetVz = dz/dist·chaseSpeed` (`targetVy`
     inchangé ; vitesse horizontale réduite si dénivelé) ;
   - sinon : `target{Vx,Vy,Vz} = d/dist·chaseSpeed`, `pitch = −v`.
   La poursuite continue au contact (pas d'arrêt à portée d'attaque).

`moving` / `moveTimer` ne sont pas touchés pendant la poursuite : en repassant en errance, le
minuteur et les consignes de poursuite persistent jusqu'à son expiration.

### 12.3 `tickIdle`

Identique au cycle §11.2-3 mais : direction `h = rand.f()·2π` (pas de troupeau, pas de
validation d'eau) ; pour WATER/AIR `v = (rand.f() − 0.5)·(π/5)` (±18°) tiré **après** `speed`.
Ordre des tirages : `h`, `speed`, (`v`), puis `moveTimer`.

---

## 13. `HealthSystem` et réapparition

Tick : pour `query(Health)` avec `!alive` : si l'entité a un `PlayerComponent` → appeler
`onPlayerDeath` (immédiatement, dans la boucle) ; sinon la marquer. Puis détruire les marquées.
Aucun minuteur de réapparition, aucun butin.

`respawnPlayer()` (aussi `/kill`) :
- `rx = isNaN(spawnX) ? 0.5 : spawnX` ; `ry = isNaN(spawnY) ? seaLevel + 5 : spawnY` ;
  `rz = isNaN(spawnZ) ? 0.5 : spawnZ` ;
- Transform ← `(rx, ry, rz)` ; `vx = vy = vz = 0` (`grounded` inchangé) ;
  `health = maxHealth ; alive = true` ; **inventaire vidé** ;
  `camera.position = (rx, ry + 1.62, rz)`.
- Inchangés : `flying`, `lavaTimer`, `gameMode`, orientation.

---

## 14. Inventaire

### 14.1 Modèle

- `SIZE = 36`, `HOTBAR_START = 27` (slots 27–35 = barre rapide, index barre `i` → slot `27+i`),
  `STACK_MAX = 64`. Slots 0–26 = grille principale (ligne = `idx/9`, colonne = `idx%9`).
- `ItemStack { block: BlockId, count: i32 }` (count non borné par le type).
- Ordre de remplissage `FILL_ORDER` = `27, 28, …, 35, 0, 1, …, 26`.

### 14.2 Opérations

- `addBlock(b)` : si `b` nul ou AIR → `false`. 1ʳᵉ passe selon `FILL_ORDER` : premier slot de
  même bloc avec `count < 64` → `count += 1`, `true`. 2ᵉ passe : premier slot vide → nouvelle pile
  de 1, `true`. Sinon `false` (objet perdu ; l'appelant ne réagit pas).
- `getSlot(i)` / `setSlot(i, s)` : hors [0, 36) → `null` / ignoré.
- `removeOne(i)` : hors bornes ou vide → rien ; sinon `count −= 1`, slot vidé si `≤ 0`.
- `getHotbarSlot(h)` = `getSlot(27 + h)`. `clear()` vide tout.

### 14.3 Interactions de l'écran d'inventaire (logique à porter dans `nb-ecs::inventory`)

État : `held: Option<ItemStack>`, `heldFrom: i32 = −1` (remis à zéro à l'ouverture).
Clic gauche sur `slot` (−1 = hors grille) :
- hors grille : si `held` → `returnHeld()` ;
- rien en main : si slot non vide → `held = copie`, `heldFrom = slot`, slot vidé ;
- en main, slot vide → poser tout, `held = None`, `heldFrom = −1` ;
- en main, même bloc → `t = min(64 − slot.count, held.count)` ; transférer ; si `held` vide →
  `None`, `heldFrom = −1` ;
- en main, autre bloc → échange (`held` = ancien contenu, `heldFrom = slot`).

`returnHeld()` (clic hors grille, fermeture par INVENTORY ou PAUSE) : si `heldFrom ≥ 0` et ce
slot est vide → y reposer. Sinon pour `i = 0..35` (ordre **naturel**, pas `FILL_ORDER`) tant que
`held.count > 0` : slot vide → y poser le reste et arrêter ; même bloc non plein → transférer
`min(64 − count, held.count)`. Enfin `held = None` **dans tous les cas** (le reliquat éventuel
est perdu).

Persistance (nb-game) : seuls les slots non vides (`slot`, nom du bloc, `count`) sont sauvés ;
un nom de bloc inconnu au chargement → AIR → slot ignoré.

---

## 15. Données de mobs (`nb-data`) consommées par `nb-ecs`

### 15.1 `MobConfig`

| Champ | Type | Usage |
|---|---|---|
| `id`, `displayName` | String | identité |
| `geometry` | boîtes `[x,y,z,w,h,d,r,g,b,a]` en pixels MC (÷16) | rendu seulement |
| `widthX`, `widthZ`, `height` | f32 | collider (largeurs **complètes**) |
| `maxHealth` | f32 | santé initiale |
| `medium` | `LAND`/`WATER`/`AIR` | physique (§7.1), spawn (§10), cibles (§11–12) |
| `spawnMinWaterDepth` | i32 | §10 |
| `maxCount`, `groupSizeMin`, `groupSizeMax`, `spawnScatterRadius` | i32 | §10 |
| `behaviorType` | `PASSIVE`/`AGGRESSIVE` | composant d'IA ajouté |
| `moveSpeedMin/Max`, `moveDurationMin/Max`, `pauseDurationMin/Max` | f32 | errance |
| `herding`, `herdRadius`, `herdBias` | bool, f32, f32 | §11 |
| `aggressorDetectRange`, `aggressorFleeSpeed` | f32 | §11 |
| `playerFleeSpeed`, `playerFleeDurationMin/Max` | f32 | §11.4 |
| `aggroPlayerRange`, `chaseSpeed`, `attackRange`, `attackDamage`, `attackCooldown`, `knockbackForce` | f32 | §12 |

Champs non fournis par le builder = 0 / `false` / `null`.

### 15.2 Valeurs intégrées (source de vérité = classes `*Definition.java`)

| Champ | reindeer | fish | orca |
|---|---|---|---|
| displayName | Reindeer | Fish | Orca |
| widthX / widthZ / height | 0.75 / 1.375 / 2.0 | 0.22 / 0.50 / 0.22 | 1.8 / 5.5 / 3.5 |
| maxHealth | 5.0 | 4.0 | 40.0 |
| medium / spawnMinWaterDepth | LAND / 0 | WATER / 2 | WATER / 5 |
| maxCount, groupMin, groupMax, scatter | 16, 1, 4, 3 | 24, 1, 3, 2 | 2, 1, 1, 0 |
| behaviorType | PASSIVE | PASSIVE | AGGRESSIVE |
| moveSpeed min/max | 1.2 / 1.8 | 0.4 / 1.0 | 0.80 / 1.40 |
| moveDuration min/max | 0.5 / 5.0 | 1.0 / 4.0 | 2.5 / 5.0 |
| pauseDuration min/max | 3.0 / 7.0 | 2.0 / 5.0 | 4.0 / 7.0 |
| herding / herdRadius / herdBias | false / 0 / 0 | true / 10.0 / 0.55 | false / 0 / 0 |
| aggressorDetectRange / aggressorFleeSpeed | **10.0 / 3.0** | 10.0 / 2.5 | 0 / 0 |
| playerFleeSpeed / durMin / durMax | 4.317 / 3.0 / 5.0 | 2.5 / 2.0 / 4.0 | 0 / 0 / 0 |
| aggroPlayerRange, chaseSpeed, attackRange | 0, 0, 0 | 0, 0, 0 | 200, 3.5, 3.5 |
| attackDamage, attackCooldown, knockbackForce | 0, 0, 0 | 0, 0, 0 | 5.0, 2.5, 9.0 |

⚠ `saves/mobs_defaults.json` (Java, **non lu par le Java** — le fichier lu est `saves/mobs.json`)
et donc `assets/vanilla/mobs_defaults.json` côté Rust indiquent pour le renne
`aggressorDetectRange = 0.0`, `aggressorFleeSpeed = 0.0` : **c'est faux** par rapport au code
(10.0 / 3.0). Sans agresseur terrestre l'effet est invisible, mais un agresseur LAND
personnalisé (ex. « zombie ») ferait fuir le renne en Java. À corriger dans `nb-data`.

### 15.3 `MobRegistry` (règles que `nb-data` doit garantir)

- `load()` : copies des 3 intégrés (ordre reindeer, fish, orca), puis surcharge par
  `saves/mobs.json` si présent ; exception → message `Could not load mobs.json: <msg>` sur stderr.
- Surcharge : pour chaque objet du tableau ayant un `id` non vide : config existante → champs
  présents écrasés ; sinon **nouvelle config copiée depuis les défauts intégrés du renne (pas la
  version surchargée), géométrie du renne comprise**, `id` et `displayName = id`, puis champs
  présents appliqués, ajout en fin de liste.
- Particularités du parseur Java (fait maison, recherche `"clé"` par `indexOf` dans le texte de
  l'objet) : entiers lus sur `[0-9-]*` (`"2.0"` → 2, `"2.5"` → 2) ; flottants sur `[0-9.-]*`
  (**exposant tronqué** : `1.0E-4` → 1.0) ; booléens par préfixe `true`/`false` ; chaînes sans
  déséchappement ; valeur d'énum inconnue ignorée ; aucune validation JSON globale.
- `save()` : écrit `saves/mobs.json` (crée le dossier), met `dirty = true`. Format exact :
  `{` / `  "mobs": [` / objets indentés de 4, champs indentés de 6, dans l'ordre :
  `id, displayName, widthX, widthZ, height, maxHealth, medium, spawnMinWaterDepth, maxCount,
  groupSizeMin, groupSizeMax, spawnScatterRadius, behaviorType, moveSpeedMin, moveSpeedMax,
  moveDurationMin, moveDurationMax, pauseDurationMin, pauseDurationMax, herding, herdRadius,
  herdBias, aggressorDetectRange, aggressorFleeSpeed, playerFleeSpeed, playerFleeDurationMin,
  playerFleeDurationMax, aggroPlayerRange, chaseSpeed, attackRange, attackDamage,
  attackCooldown, knockbackForce` ; nombres via `Float.toString` (`200.0`, `4.317`, `1.0E-4`) ;
  chaînes échappées (`\` et `"`). Pas de géométrie.
- `resetToDefaults()` : remet les 3 intégrés (supprime les personnalisés) et sauvegarde.
- `dirty` signifie « configs modifiées » : à la sortie des écrans de réglages, si `dirty` →
  `despawnAll` (détruit toutes les entités `MobType`) puis `clearDirty`.
- Une entité dont la config a disparu : physique terrestre, IA ignorée, comptée pour aucun
  `cfg` ; elle ne disparaît que par la distance ou par `despawnAll`.

État actuel de `crates/nb-data/src/mobs.rs` (constaté, non modifié) : les nouveaux mobs partent
de `configs[0]` (renne **surchargé**) avec `geometry = None`, le parseur `serde_json` rejette tout
le fichier si invalide et lit `2.0` comme absent pour un entier : écarts à arbitrer (§20, B14).

---

## 16. Commandes (`CommandProcessor.executeCommand(command, ctx) -> Option<String>`)

Entrée = texte brut du chat (commence normalement par `/`). Comparaisons **sensibles à la casse**
pour le mot de commande. Les messages ci-dessous sont **exacts** (anglais, ponctuation et
espaces compris). `%.1f`, `%.2f`, `%.3f` = format Java (§16.3).

### 16.1 Aiguillage (ordre exact)

1. `null` ou `""` → `None` (aucune réponse).
2. `== "/help"` →
   `Commands: /time, /gamemode, /physics, /player, /default, /kill, /setworldspawn  (type /help <command> for details)`
3. commence par `"/help "` → `handleHelp(substring(6).trim().toLowerCase())`.
4. si `!ctx.allowCheats` → `Cheats are not enabled in this world.` (pour **toute** autre entrée,
   y compris inconnue).
5. `== "/kill"` → `respawnPlayer()` ; `Respawned at world spawn.`
6. `== "/setworldspawn"` → §16.2.
7. commence par `"/time "` → groupe time sur `substring(6).trim()`.
8. commence par `"/gamemode "` → `substring(10).trim().toLowerCase()`.
9. commence par `"/physics "` → `substring(9).trim()` (**non** mis en minuscules).
10. commence par `"/player "` → `substring(8).trim()` (non mis en minuscules).
11. `== "/default"`.
12. sinon → `Unknown command: <command>` (texte brut complet).

Ainsi `/time` sans espace → `Unknown command: /time` ; `/Kill` → inconnu.

### 16.2 Détail des commandes

**`/setworldspawn`** : Transform du joueur absent → `Player not found.` ; sinon
`spawnX/Y/Z = t.x/y/z` (pieds, `f64`) → `World spawn point set to (%.1f, %.1f, %.1f).`

**`/help <args>`** (`args` minuscules, trim) :

| args | Réponse |
|---|---|
| `""` | `Commands: /time, /gamemode, /physics, /player, /default, /kill, /setworldspawn` |
| `time` | `/time set <day\|noon\|sunset\|night\|midnight\|sunrise\|number>  \|  /time speed <seconds>` |
| `time set` | `/time set — presets: day, noon, sunset, night, midnight, sunrise  \|  or a number (0.0–1800.0)` |
| `time speed` | `/time speed <seconds> — day length in seconds (min: 1.0)` |
| `gamemode` | `/gamemode <creative\|survival>  (alias: 1 or 0)` |
| `physics` | `/physics gravity <mult>  \|  /physics falldamage <on\|off>` |
| `physics gravity` | `/physics gravity <multiplier> — 1.0 = default, 2.0 = double, -1.0 = antigravity, 0 = none` |
| `physics falldamage` | `/physics falldamage <on\|off>` |
| `player` | `/player health\|damage\|walkspeed\|sprintspeed\|flyspeed\|jumpforce <value>` |
| `player health` | `/player health <amount> — max HP (> 0); hearts scale to always show 10` |
| `player damage` | `/player damage <amount> — HP dealt per hit (>= 0, default 1.0)` |
| `player walkspeed` | `/player walkspeed <speed> — walk speed in units/sec (default 4.317)` |
| `player sprintspeed` | `/player sprintspeed <speed> — sprint speed in units/sec (default 5.612)` |
| `player flyspeed` | `/player flyspeed <speed> — fly speed in units/sec (default 20.0)` |
| `player jumpforce` | `/player jumpforce <multiplier> — 1.0 = default, 2.0 = double, must be >= 0` |
| `default` | `/default — reset all command-tweaked values to their defaults` |
| `kill` | `/kill — respawn at the world spawn point` |
| `setworldspawn` | `/setworldspawn — set the world spawn point to your current position` |
| autre | `No help found for: <args>  \|  Try /help for top-level commands` |

(Dans ce tableau `\|` = `|` ; les doubles espaces autour des `|` sont réels ; `—` est un tiret
cadratin U+2014, `–` dans `0.0–1800.0` un demi-cadratin U+2013.)

**`/time <args>`** :
- `args` commence par `"set "` → `value = substring(4).trim().toLowerCase()` :
  `day` → `setTime(750)` → `Time set to day.` ; `noon` → 900 → `Time set to noon.` ;
  `sunset` → 0 → `Time set to sunset.` ; `night` → 150 → `Time set to night.` ;
  `midnight` → 300 → `Time set to midnight.` ; `sunrise` → 600 → `Time set to sunrise.` ;
  sinon `parseFloat(value)` → `setTime(t)` → `Time set to <value>.` (texte saisi, en minuscules) ;
  échec → `Invalid time value: <value>`.
  `setTime(t)` (nb-gfx) : `t % 1200`, +1200 si négatif.
- `args` commence par `"speed "` → `val = substring(6).trim()` ; `parseFloat` ; échec →
  `Invalid value: <val>` ; si `< 1.0` → `Day length must be at least 1 second.` ; sinon
  `renderer.setDayLengthSeconds(s)` (qui fait `max(1, s)`) **et** `worldConfig.dayLengthSeconds = s`
  → `Day length set to <Float.toString(s)>s per cycle.` (ex. `Day length set to 600.0s per cycle.`).
- sinon → `Usage: /time set <value|day|night|...> | /time speed <seconds>`
  (donc `/time set` seul ou `/time set ` → usage).

**`/gamemode <v>`** (minuscules) : PlayerComponent absent → `Player not found.` ;
`creative`|`1` → `gameMode = CREATIVE` → `Gamemode set to Creative.` (vol inchangé) ;
`survival`|`0` → `SURVIVAL`, `flying = false`, santé pleine et `alive = true` →
`Gamemode set to Survival.` ; sinon `Unknown gamemode: <v>`.

**`/physics <args>`** :
- `"gravity "` → `scale = parseFloat(substring(8).trim())` ; `gravity = scale · (−32.0)` →
  `Gravity set to %.2fx (%.1f units/s^2).` (scale, gravity) ; échec →
  `Invalid value. Usage: /physics gravity <multiplier>  (1.0 = default, -1.0 = antigravity)`.
  Affecte joueur **et** mobs.
- `"falldamage "` → `v = substring(11).trim().toLowerCase()` : `on`|`true` →
  `Fall damage enabled.` ; `off`|`false` → `Fall damage disabled.` ; sinon
  `Usage: /physics falldamage on|off`.
- sinon → `Usage: /physics gravity <n> | /physics falldamage on|off`.

**`/player <args>`** (l'ordre parse → bornes → existence est normatif) :

| Préfixe (longueur) | Validation | Effet | Succès | Échec de parse |
|---|---|---|---|---|
| `health ` (7) | `≤ 0` → `Health must be > 0.` ; puis Health absent → `Player health not found.` | `ratio = max > 0 ? h/max : 1` ; `maxHealth = a` ; `health = max(0, ratio·a)` (`alive` inchangé) | `Max health set to %.1f.` | `Invalid value. Usage: /player health <amount>` |
| `damage ` (7) | `< 0` → `Damage must be >= 0.` | `attackDamage = a` | `Attack damage set to %.1f.` | `Invalid value. Usage: /player damage <amount>` |
| `walkspeed ` (10) | `< 0` → `Speed must be >= 0.` | `walkSpeed` | `Walk speed set to %.3f.` | `Invalid value. Usage: /player walkspeed <speed>` |
| `sprintspeed ` (12) | idem | `sprintSpeed` | `Sprint speed set to %.3f.` | `Invalid value. Usage: /player sprintspeed <speed>` |
| `flyspeed ` (9) | idem | `flySpeed` | `Fly speed set to %.3f.` | `Invalid value. Usage: /player flyspeed <speed>` |
| `jumpforce ` (10) | `< 0` → `Jump force multiplier must be >= 0.` | `jumpForce = s · 8.4` | `Jump force set to %.2fx (%.1f units/s).` (s, jumpForce) | `Invalid value. Usage: /player jumpforce <multiplier>  (1.0 = default)` |
| autre | | | `Usage: /player health\|damage\|walkspeed\|sprintspeed\|flyspeed\|jumpforce <value>` | |

**`/default`** : `gravity = −32`, `jumpForce = 8.4`, `fallDamageEnabled = true`,
`dayLengthSeconds = 1200` (**dans `WorldConfig` seulement**, le renderer n'est pas mis à jour),
`walkSpeed = 4.317`, `sprintSpeed = 5.612`, `flySpeed = 20`, `attackDamage = 1.0` ; si Health
joueur : `maxHealth = 10`, `health = min(health, 10)` → `All values reset to defaults.`
(Ne touche ni mode, ni spawn, ni rayons de spawn des mobs.)

### 16.3 Formats numériques Java à émuler (helpers à fournir)

- `java_parse_float(s)` = `Float.parseFloat` : ignore les blancs ≤ U+0020 en tête/queue ; accepte
  signe, `NaN`, `Infinity`, décimal avec exposant, suffixe `f/F/d/D`, hexadécimal (`0x1p3`).
  Refuse `inf`, `nan` (minuscules), chaîne vide. NaN se propage (aucune garde : `/physics gravity
  NaN` est accepté).
- `java_float_to_string(f32)` = `Float.toString` : plus courte représentation ; notation
  scientifique `d.dddE±n` si `|v| < 1e-3` ou `≥ 1e7` ; toujours au moins un chiffre après le point
  (`600.0`) ; `NaN`, `Infinity`.
- `java_format_fixed(v, n)` = `String.format("%.nf", v)` : le `float` est promu en `double`,
  arrondi **HALF_UP** sur la représentation décimale la plus courte du `double`
  (`Double.toString`), et **séparateur décimal de la locale par défaut** (en locale française,
  le Java afficherait `1,0` : choisir `.`, voir §20 B12).

### 16.4 Contexte d'exécution

Les commandes s'exécutent dans `ChatController.update`, avant l'ECS de la frame (§2). Le chat
journalise la ligne saisie (blanc) puis la réponse (jaune) si non nulle ; 4 lignes max ; affiché
5 s après fermeture (nb-ui).

---

## 17. Particules

Données (`particles.json`, nb-data) : `{id, texture, lifeTime, scale, gravity}`. Valeurs :
`leaf` (texture `leaf`, vie 5.0, échelle 0.15, gravité 0.5) ; `lava` (texture `lava_drop`, vie 2.0,
échelle 0.1, gravité **−0.2**). `LEAF = get("leaf")`, `LAVA = get("lava")` (absent → `None`).

`Particle { type, position: Vec3, velocity: Vec3, color: Vec3, life = type.lifeTime }`.
`update(dt)` : `velocity.y −= gravity·dt` ; `position += velocity·dt` ; `life −= dt` ;
vivante si `life > 0`. Gravité positive = chute ; la lave (−0.2) **accélère vers le haut**.

`ParticleManager` : liste ordonnée ; `update(dt)` met à jour chaque particule et retire les
mortes en préservant l'ordre ; `spawnParticle(type, pos, vel, color)` : type `None` → ignoré,
sinon ajoute des **copies**. Pas de plafond, pas de collision.

Émission ambiante (`World.generateRandomParticles(playerPos)`, chaque tick PLAYING, après
`update`) : `radius = 24`, `p = floor(playerPos)` ; 200 itérations, RNG du `World` :
```
rx = px + rand.i(48) − 24 ; ry = py + rand.i(48) − 24 ; rz = pz + rand.i(48) − 24
b = getBlock(rx, ry, rz)
si b non nul et b.name contient "leaves" :
    si getBlock(rx, ry−1, rz) == AIR && rand.f() < 0.15 :
        tint = leavesTint(biome(rx, rz))          // f64[3], nb-world
        color = (tint·2.2) en f32
        pos = (rx + rand.f(), ry − 0.1, rz + rand.f())
        vel = ((rand.f() − 0.5)·0.5, −0.5, (rand.f() − 0.5)·0.5)
        spawn(LEAF, pos, vel, color)
sinon si b == LAVA :
    color = (1, 1, 1)
    si getBlock(rx, ry+1, rz) == AIR && rand.f() < 0.05 :
        pos = (rx + rand.f(), ry + 1.1, rz + rand.f())
        vel = ((rand.f() − 0.5)·0.5, 1.5 + rand.f(), (rand.f() − 0.5)·0.5)
        spawn(LAVA, …)
    si getBlock(rx, ry−1, rz) == AIR && rand.f() < 0.02 :
        spawn(LAVA, (rx + rand.f(), ry − 0.1, rz + rand.f()), (0, −1, 0), color)
```
Ordre des tirages = ordre d'évaluation Java gauche → droite (le `rand.f()` de probabilité n'est
tiré que si le bloc voisin est AIR). Le taux d'émission est **par tick** (60 Hz).

---

## 18. `WorldConfig` lu/écrit par nb-ecs

| Champ | Défaut | Lu par | Écrit par |
|---|---|---|---|
| `gravity` | −32.0 | joueur, mobs terrestres | `/physics gravity`, `/default` |
| `jumpForce` | 8.4 | joueur, saut auto des mobs | `/player jumpforce`, `/default` |
| `fallDamageEnabled` | true | joueur, mobs | `/physics falldamage`, `/default` |
| `walkSpeed` / `sprintSpeed` / `flySpeed` | 4.317 / 5.612 / 20.0 | joueur | `/player …`, `/default` |
| `attackDamage` | 1.0 | `hitMobEntity` | `/player damage`, `/default` |
| `dayLengthSeconds` | 1200.0 | (renderer) | `/time speed`, `/default` |
| `mobSpawnRadius` / `mobDespawnRadius` | 64 / 64 | spawn | — |
| `spawnX/Y/Z` | NaN | réapparition | `/setworldspawn`, 1er chargement |
| `seaLevel` | 34 | réapparition par défaut | — |

Repères : hauteur de saut ≈ 8.4²/(2·32) = 1.10 bloc ; dégâts de chute joueur dès |v| > 11.5
(≈ chute de 2.07 blocs), mobs dès |v| > 10.

---

## 19. Contrat Rust proposé

Bibliothèque ECS : `hecs` (déjà en dépendance). Pas d'état global : registres et config passés
par contexte.

### 19.1 Composants

```rust
use glam::{DVec3, IVec3, Vec3};
use nb_data::{BlockId, MobBehaviorType, MobMedium};

/// Identifiant Java (création monotone, jamais réutilisé) : sert à l'ordre d'itération (B1)
/// et de clé stable pour les tests.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EntityId(pub i32);

pub struct Transform { pub pos: DVec3, pub yaw: f32, pub pitch: f32 }
#[derive(Default)]
pub struct Velocity { pub v: Vec3, pub grounded: bool, pub target: Vec3 }
pub struct Collider { pub width_x: f32, pub width_z: f32, pub height: f32, pub eye_offset: f32 }
pub struct Health { pub health: f32, pub max_health: f32, pub alive: bool }
pub struct MobType { pub mob_id: String }          // ou Arc<str>
#[derive(Default)] pub struct PassiveAi { pub move_timer: f32, pub moving: bool, pub flee_timer: f32 }
#[derive(Default)] pub struct AggressiveAi { pub move_timer: f32, pub moving: bool, pub attack_cooldown: f32 }
pub struct InventoryComp(pub Inventory);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameMode { Survival, Creative }

/// L'état caméra vit dans l'ECS (nb-gfx le lit) : remplace la référence `Camera` du Java.
pub struct CameraState { pub position: Vec3, pub pitch_deg: f32, pub yaw_deg: f32, pub fov: f32 }

pub struct Player {
    pub camera: CameraState,
    pub game_mode: GameMode,
    pub flying: bool,
    pub lava_timer: f32,
    pub double_tap_timer: f32,          // f32::MAX
    pub current_fov: f32,
    pub targeted_block: Option<IVec3>,
    pub place_target: Option<IVec3>,
    pub fluid_at_eye_level: BlockId,
}
```

### 19.2 Accès au monde de blocs (implémenté par nb-world)

```rust
pub trait BlockAccess {
    /// AIR hors chunk chargé ou hors y ∈ [0,128).
    fn block(&self, x: i32, y: i32, z: i32) -> BlockId;
    fn set_block(&mut self, x: i32, y: i32, z: i32, id: BlockId);
    fn is_chunk_loaded(&self, wx: i32, wz: i32) -> bool;
    /// Teinte `leavesTint` du biome en (x, z) — pour les particules.
    fn leaves_tint(&self, x: i32, z: i32) -> [f64; 3];
}
```

### 19.3 Contexte, entrées, sorties

```rust
pub struct PlayerInput {              // instantané produit par nb-game à chaque tick
    pub pressed: ActionSet, pub just_pressed: ActionSet,   // FORWARD…CROUCH, SPRINT, JUMP, PRIMARY, SECONDARY, DROP
    pub mouse_delta: Vec2, pub selected_slot: u8, pub mouse_locked: bool,
    pub sensitivity: f32, pub fov_deg: Option<f32>,         // None ⇒ défauts Java (0.1, 70°)
}

pub struct EcsRngs {                  // un JavaRandom par système, comme en Java
    pub spawn: JavaRandom, pub passive: JavaRandom, pub aggressive: JavaRandom, pub ambient: JavaRandom,
}

pub struct TickCtx<'a> {
    pub blocks: &'a mut dyn BlockAccess,
    pub block_reg: &'a nb_data::BlockRegistry,
    pub mobs: &'a nb_data::MobRegistry,
    pub particle_types: &'a nb_data::ParticleRegistry,
    pub config: &'a mut WorldConfig,       // sous-ensemble gameplay (§18)
    pub input: &'a PlayerInput,
    pub rngs: &'a mut EcsRngs,
    pub events: &'a mut Vec<EcsEvent>,     // BlockChanged, PlayerDied, PlayerRespawned…
}

pub trait TimeControl {               // implémenté par nb-gfx / nb-game, pour les commandes
    fn set_time(&mut self, t: f32);
    fn set_day_length_seconds(&mut self, s: f32);
}
```

### 19.4 Monde d'entités, systèmes, ordonnancement

```rust
pub struct EntityWorld {
    world: hecs::World,
    next_id: i32,
    order: JavaHashOrder,                 // émule l'ordre HashMap<Integer> (B1)
    by_id: HashMap<EntityId, hecs::Entity>,
}
impl EntityWorld {
    pub fn create(&mut self) -> EntityId;
    pub fn destroy(&mut self, id: EntityId);
    pub fn insert<C: hecs::Component>(&mut self, id: EntityId, c: C);
    pub fn get<C: hecs::Component>(&self, id: EntityId) -> Option<hecs::Ref<'_, C>>;
    /// Liste figée, triée selon l'ordre Java.
    pub fn query_ids<Q: hecs::Query>(&self) -> Vec<EntityId>;
}

pub trait System {
    fn name(&self) -> &'static str;
    fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32);
}

pub struct Schedule { systems: Vec<Box<dyn System>> }
impl Schedule {
    /// Ordre Java : PlayerInput, Physics, PassiveAi, AggressiveAi, MobSpawn, Health.
    pub fn java_default() -> Self;
    pub fn push(&mut self, s: Box<dyn System>);
    pub fn push_front(&mut self, s: Box<dyn System>);
    pub fn run(&mut self, world: &mut EntityWorld, ctx: &mut TickCtx<'_>, dt: f32);
}

/// Façade appelée par nb-game dans l'état PLAYING (équivaut à World.update(pos, dt)).
pub struct Simulation {
    pub entities: EntityWorld, pub schedule: Schedule, pub particles: ParticleManager,
    pub player: Option<EntityId>, spawn_player_pos: Vec3,
}
impl Simulation {
    pub fn init_player(&mut self, camera: CameraState, mode: GameMode) -> EntityId;
    /// particules.update → émission ambiante → set spawn pos (= caméra) → schedule.run
    pub fn tick(&mut self, ctx: &mut TickCtx<'_>, dt: f32);
    pub fn hit_mob(&mut self, id: EntityId, ctx: &mut TickCtx<'_>);       // §9.1
    pub fn respawn_player(&mut self, ctx: &mut TickCtx<'_>);              // §13
    pub fn despawn_all_mobs(&mut self);
}

pub fn execute_command(cmd: &str, sim: &mut Simulation, ctx: &mut TickCtx<'_>,
                       time: &mut dyn TimeControl, allow_cheats: bool) -> Option<String>;
```

Notes de conception :
- `PlayerInputSystem` a besoin de `hit_mob` et des fonctions de collision du `PhysicsSystem` :
  les exposer comme fonctions libres d'un module `physics` (pas d'état), et `hit_mob` comme
  fonction libre prenant `&mut EcsRngs` (le Java utilise le `Random` du système passif pour
  `triggerPlayerFlee`).
- La mort du joueur : `HealthSystem` pousse `EcsEvent::PlayerDied` **et** appelle
  `respawn_player` immédiatement (comportement Java synchrone).
- Graines des `JavaRandom` : non déterministes par défaut (le Java utilise `new Random()`),
  injectables pour les tests de conformité.
- Géométrie de collision et raycast en types exacts du Java (`f64` positions, `f32` ailleurs,
  accumulations du raycast en `f32`) pour des trajectoires identiques.

---

## 20. Bizarreries du Java à reproduire

(Sauf décision contraire explicite ; chaque point est observable.)

- **B1 — Ordre d'itération.** `query()` suit l'ordre d'un `HashMap<Integer>` : seau
  `id & (cap − 1)` puis ordre d'insertion (= ordre des ids, monotones). `cap` = 16 au départ,
  doublé dès que la taille dépasse `0.75·cap`, **jamais réduit**. L'ordre n'est donc croissant que
  tant que les ids restent < `cap`. Il influence la séparation (§7.5), les égalités de cibles et
  l'ordre de consommation des RNG. Émuler via un tri par `(id & (cap−1), id)`.
- **B2** — `docs/ecs.md` place MobSpawn en 3ᵉ ; le code l'exécute en 5ᵉ, après les deux IA (§5).
- **B3** — Collisions par cellules à bornes inclusives : une AABB qui touche une face compte comme
  en collision (un mob de 2.0 de haut posé à y entier ne passe pas sous un plafond à y+2).
- **B4** — Aucun recalage contre le sol/mur : approche asymptotique par petits pas (§7.2).
- **B5** — Saut automatique des mobs terrestres sur toute collision horizontale au sol, effectif
  dans le même tick ; utilise le `jumpForce` du joueur.
- **B6** — Les mobs aquatiques ignorent la gravité même hors de l'eau (la fuite d'agresseur et la
  fuite du joueur ne valident pas l'eau ; un poisson peut « voler »).
- **B7** — Séparation sans test de blocs ; joueur immobile ; paires orque/poisson exclues.
- **B8** — Raycast blocs à pas fixe de 0.05 : touche les plantes et le nénuphar, voisin
  potentiellement diagonal ou égal au bloc touché, coins manqués.
- **B9** — Raycast mobs sans occlusion par les blocs et prioritaire sur le bloc visé.
- **B10** — `triggerPlayerFlee` choisit une direction aléatoire ; les agressifs ne réagissent pas
  aux coups ; le recul (5.0, vy 4.0) s'applique même au coup mortel.
- **B11** — Spawn : rayon de spawn = rayon de despawn = 64, mais les décalages sont tirés dans un
  carré (coins à ~90) → mobs apparus dans les coins détruits au cycle suivant ; trou carré (pas
  circulaire) de 16 ; timer remis à 0 (pas de report) ; dépassement possible de `maxCount` ;
  `findSurfaceY` saute les fonds marins et peut finir dans une grotte ; `groupSizeMax <
  groupSizeMin` ou `mobSpawnRadius == 0` font lever une exception en Java (`nextInt(≤0)`) — à
  traiter comme « aucun spawn » + log plutôt que panique.
- **B12** — Formats numériques des commandes (§16.3). En locale française le Java afficherait des
  virgules (`1,0`) : **décision recommandée** : point décimal (comportement en locale anglaise),
  à valider.
- **B13** — `/default` ne remet pas la durée du jour du renderer ; `/player health` conserve le
  ratio sans toucher `alive` ; `/gamemode creative` ne coupe ni ne donne le vol ; `/kill` vide
  l'inventaire ; le message de cheats désactivés couvre aussi les commandes inconnues.
- **B14** — Parseur `mobs.json` maison (§15.3) : exposants tronqués à la relecture
  (`Float.toString` écrit `1.0E-4`, relu `1.0`), entiers tronqués, nouveaux mobs clonés depuis le
  renne intégré **avec sa géométrie**.
- **B15** — Dégâts : chute et lave du joueur clampés à 0, morsure et dégâts de chute des mobs non
  clampés (santé négative possible) ; lave = au plus 1 dégât/tick, premier après 0.5 s.
- **B16** — FOV : `sprintSpeed = 0` (ou `flySpeed = 0` en vol) avec vitesse nulle donne
  `0/0 = NaN` → `currentFov` devient NaN définitivement (Java `Math.max/min` propagent NaN).
  Reproduire avec des `max/min` propageant NaN (pas `f32::max`, qui ignore NaN).
- **B17** — Sauts répétés tant que JUMP est maintenu ; sprint autorisé en diagonale avant ; sprint
  interdit dans un fluide mais pas en vol (le vol ignore SPRINT).
- **B18** — Sol virtuel au-dessus des chunks non chargés pour le joueur seulement (les mobs y
  tombent… sur de l'AIR, donc indéfiniment, jusqu'au despawn).
- **B19** — Casser/poser ignore le mode de jeu (pas d'inventaire infini en créatif, la bedrock se
  casse, le bloc cassé est ajouté en créatif) ; DROP détruit l'objet.
- **B20** — `returnHeld` perd le reliquat si l'inventaire est plein.
- **B21** — La position de spawn/despawn est celle des **yeux** au tick précédent ; la distance
  de despawn est horizontale et calculée en `f32`.
- **B22** — La santé max du joueur n'est pas sauvegardée (10 au rechargement) alors que la santé
  courante l'est.
- **B23** — Particule de lave : gravité négative ⇒ accélération vers le haut ; émission liée au
  nombre de ticks, pas au temps réel.

---

## 21. Écarts entre la documentation Java et le code (le code fait foi)

1. `docs/ecs.md` : ordre des systèmes faux (B2) ; `VelocityComponent` possède aussi `targetVy` ;
   `removeEntity` s'appelle `destroyEntity` ; `HealthSystem` ne gère aucun minuteur de
   réapparition ; « CREATIVE = blocs illimités » est faux (B19).
2. `docs/mob-system.md` : `widthX`/`widthZ` sont des largeurs **complètes**, pas des demi-largeurs ;
   les méthodes s'appellent `getById`/`getAll`/`save` (pas de `markDirty`) ; les mobs JSON-only
   héritent de la géométrie du renne ; la fuite du joueur n'est pas « away from player ».
3. `saves/mobs_defaults.json` : valeurs du renne erronées (§15.2).
