# Spécification comportementale — crate `nb-world`

> Objet : permettre de réimplémenter en Rust, **sans relire le Java**, tout ce que fait le paquet
> `com.engine.core.world` (hors maillage/rendu) du jeu Norrblock Java, à l'identique bit à bit
> pour la génération de terrain.
>
> Sources Java lues ligne à ligne (réf. `Norrblock-Java-ref/repo/app/src/main/java/com/engine/core/`) :
> `world/WorldSaveManager`, `WorldConfig`, `WorldMetadata`, `World`, `Chunk`, `ChunkPos`, `ChunkManager`,
> `ChunkSnapshot`, `FileChunkStorage`, `InMemoryChunkStorage`, `IChunkStorage`, `BlockRegistry`, `BlockType`,
> `generation/WorldGenerator`, `BiomeSystem`, `Noise`, `Biome`, `TreeType`, plus `resources/blocks.json`,
> `docs/world.md`, `docs/config-reference.md` et les appelants (`Main`, `GameContext`, `WorldSelectState`,
> `PausedState`, `ChunkMeshManager`).
>
> Les vecteurs de référence de la section 16 ont été **produits en exécutant le code Java d'origine**
> (JDK 25, harnais hors dépôt). Ils font foi.

Conventions de ce document :

* `int` = `i32` à arithmétique **enveloppante** (wrapping), `long` = `i64`, `double` = `f64`, `float` = `f32`.
* « floor » = `Math.floor` (f64 → f64), « (int) » = cast Java f64→i32 (troncature vers zéro, saturant, NaN→0 ;
  identique à `as i32` en Rust).
* Toute expression flottante doit être évaluée **dans le même ordre et avec les mêmes types** que décrit
  (associativité gauche, pas de FMA / `mul_add`, pas de réordonnancement). Java ≥ 17 est strict IEEE-754,
  Rust `f64` aussi : l'égalité bit à bit est atteignable.

---

## 1. Dimensions et constantes

| Constante | Valeur | Remarque |
|---|---|---|
| `Chunk.SIZE_X` | 16 | |
| `Chunk.SIZE_Y` | 128 | y valide : `0..=127` |
| `Chunk.SIZE_Z` | 16 | |
| Volume | 32 768 | |
| `Chunk.SEA_LEVEL` | 64 | **Jamais utilisée.** Le niveau de la mer réel est `WorldConfig.seaLevel` (34 par défaut). |
| Index voxel Java | `x + y*16 + z*16*128` = `x + 16·y + 2048·z` | ⚠ voir §8.1 (différent de `nb_core::math::block_index`) |

`ChunkPos` = `record (int x, int z)` ; égalité/hachage par valeur. Origine monde d'un chunk :
`wx0 = pos.x * 16`, `wz0 = pos.z * 16` (multiplication `int`).

---

## 2. Blocs utilisés par le monde

Les IDs viennent de `blocks.json` (déjà porté dans `nb-data`). Le monde ne référence les blocs que par nom ;
le Rust doit passer par `BlockIds`. Propriétés pertinentes pour la lumière :

| id | nom | atten. json | émission | crossShaped | leavesTinted | « bloquant lumière » (§10) |
|---:|---|---:|---:|:-:|:-:|:-:|
| 0 | air | 0 | 0 | | | non |
| 1 | dirt | 15 | 0 | | | oui |
| 2 | grass | 15 | 0 | | | oui |
| 3 | stone | 15 | 0 | | | oui |
| 4 | water | 2 | 0 | | | non |
| 5 | sand | 15 | 0 | | | oui |
| 6 | bedrock | 15 | 0 | | | oui |
| 7 | clay | 15 | 0 | | | oui |
| 8 | snow | 15 | 0 | | | oui |
| 9 | wood | 15 | 0 | | | oui |
| 10 | leaves | 2 | 0 | | ✔ | non |
| 11 | birch_wood | 15 | 0 | | | oui |
| 12 | birch_leaves | 2 | 0 | | ✔ | non |
| 13 | spruce_wood | 15 | 0 | | | oui |
| 14 | spruce_leaves | 2 | 0 | | ✔ | non |
| 15 | jungle_wood | 15 | 0 | | | oui |
| 16 | jungle_leaves | 2 | 0 | | ✔ | non |
| 17 | acacia_wood | 15 | 0 | | | oui |
| 18 | acacia_leaves | 2 | 0 | | ✔ | non |
| 19 | tall_grass | 0 | 0 | ✔ | | non |
| 20 | flower_red | 0 | 0 | ✔ | | non |
| 21 | flower_yellow | 0 | 0 | ✔ | | non |
| 22 | fern | 0 | 0 | ✔ | | non |
| 23 | dead_bush | 0 | 0 | ✔ | | non |
| 24 | mushroom_brown | 0 | 0 | ✔ | | non |
| 25 | mushroom_red | 0 | 0 | ✔ | | non |
| 26 | lily_pad | 0 | 0 | | ✔ | non |
| 27 | cactus | 15 | 0 | | | oui |
| 28–32 | coal/iron/gold/diamond/emerald_ore | 15 | 0 | | | oui |
| 33 | lava | 3 | 15 | | | non |
| 34 | sandstone | 15 | 0 | | | oui |

`BlockRegistry.fromId(byte)` : ID inconnu → `AIR`. `BlockRegistry.get(name)` : nom inconnu → `AIR`.
Les IDs sont stockés en `byte` signé Java ; tous les IDs actuels sont < 128, donc `u8` en Rust est équivalent.

---

## 3. Aléatoire, graines, hachage

### 3.1 Où intervient l'aléatoire

* **Génération du monde : un seul usage de `java.util.Random`** — le mélange de la table de permutation
  dans le constructeur de `Noise` (§4.1). Tout le reste de la génération est **déterministe et sans état** :
  bruit de Perlin (fonction pure de la permutation) + hachage entier `hashPosition`.
* **Aucune graine dérivée.** Chaque `Noise` est construit avec la **même** graine `seed` du monde :
  `WorldGenerator` (un par thread), le `BiomeSystem` interne du générateur (qui partage le `Noise` du générateur)
  et le `BiomeSystem` de `World` (son propre `new Noise(seed)`). Toutes ces instances ont donc **la même
  permutation**. Les différentes couches (hauteur, océan, rivières, biomes, grottes…) ne se distinguent que
  par des **décalages de coordonnées** et des fréquences.
* `java.util.Random(seed)` n'utilise que les **48 bits de poids faible** de la graine
  (`(seed ^ 0x5DEECE66D) & (2^48-1)`) : deux graines égales modulo 2^48 donnent le même monde.
* Hors génération : `World.random = new Random()` (non semé) pour les particules (annexe A) ;
  `Main` crée le monde du menu avec `new Random().nextInt(1_000_000_000)` (graine aléatoire `int` ≥ 0, élargie en `long`).

### 3.2 `java.util.Random` (déjà porté : `nb_core::JavaRandom`)

* init : `s = (seed ^ 0x5DEECE66D) & ((1<<48)-1)`
* `next(bits)` : `s = (s * 0x5DEECE66D + 0xB) & ((1<<48)-1)` ; retourne `(int)(s >>> (48-bits))`
* `nextInt(bound)` : `r = next(31)` ; si `bound` puissance de 2 → `(int)((bound * (long)r) >> 31)` ;
  sinon boucle `u = r; r = u % bound; tant que u - r + (bound-1) < 0 (en int enveloppant) : u = next(31)`.

### 3.3 Graine saisie par l'utilisateur (`WorldSelectState.parseSeed`, pour info nb-ui/nb-game)

* champ vide → `new Random().nextLong() & Long.MAX_VALUE` ;
* sinon `Long.parseLong(text)` si possible ;
* sinon `text.hashCode()` (`String.hashCode` Java : `h = 31*h + c` sur les **unités UTF-16**, `int` enveloppant,
  puis élargi en `long` avec signe). Ex. : `"Norrblock".hashCode() = -1910315700`.

### 3.4 `hashPosition(int x, int z) -> int` (WorldGenerator, privé)

```
h = x * 374761393 + z * 668265263 + 1234567      // int enveloppant
h = (h ^ (h >> 13)) * 1274126177                 // >> arithmétique (signé), * enveloppant
return (h ^ (h >> 16)) & 0x7FFFFFFF              // toujours >= 0
```

Rust : `wrapping_mul` / `wrapping_add` sur `i32`, `>>` sur `i32` (arithmétique). Comme le résultat est ≥ 0,
`h % n` Java == `h % n` Rust == `h.rem_euclid(n)`.

**Les arguments passés à `hashPosition` sont eux aussi calculés en `int` enveloppant** (`wx * 3 + 77`,
`wx * ore.seed + y * 31`, `x + chunkX * 16`…) : utiliser `wrapping_*` partout (sinon panique en debug pour
des coordonnées extrêmes).

Le motif `(hash % 10000) / 10000.0` (int % int, puis conversion en double, division double) donne une valeur
uniforme dans `[0, 0.9999]`.

---

## 4. Bruit (`Noise`) — Perlin « improved » de Ken Perlin

### 4.1 Construction

```
p = int[512]; perm = [0, 1, …, 255]
rnd = new java.util.Random(seed)
for i = 255 downto 1:
    index = rnd.nextInt(i + 1)
    swap(perm[index], perm[i])           // a = perm[index]; perm[index] = perm[i]; perm[i] = a
for i in 0..256: p[i] = perm[i]; p[i+256] = perm[i]
```

Exactement 255 appels à `nextInt`, dans cet ordre. Pour `seed = 12345` : `perm[0..16] = 200 42 20 121 182 178 7 39 194 118 164 160 251 166 254 53`.

### 4.2 Primitives

```
fade(t) = t*t*t*(t*(t*6 - 15) + 10)
lerp(t, a, b) = a + t*(b - a)

grad2(hash, x, y):
    h = hash & 15
    u = (h < 8) ? x : y
    v = (h < 4) ? y : ((h == 12 || h == 14) ? x : 0.0)      // ⚠ 0.0 en 2D (pas z)
    return ((h & 1) == 0 ? u : -u) + ((h & 2) == 0 ? v : -v)

grad3(hash, x, y, z):  idem mais v = (h < 4) ? y : ((h == 12 || h == 14) ? x : z)
```

### 4.3 `evaluate(x, y)` (2D) — résultat ≈ [-1, 1]

```
X = (int)floor(x) & 255 ; Y = (int)floor(y) & 255
x -= floor(x) ; y -= floor(y)
u = fade(x) ; v = fade(y)
A = p[X] + Y ; B = p[X+1] + Y
return lerp(v, lerp(u, grad2(p[A],   x, y  ), grad2(p[B],   x-1, y  )),
               lerp(u, grad2(p[A+1], x, y-1), grad2(p[B+1], x-1, y-1)))
```

### 4.4 `evaluate(x, y, z)` (3D)

```
X,Y,Z = (int)floor(·) & 255 ; x,y,z -= floor(·)
u,v,w = fade(x), fade(y), fade(z)
A = p[X]+Y ; AA = p[A]+Z ; AB = p[A+1]+Z
B = p[X+1]+Y ; BA = p[B]+Z ; BB = p[B+1]+Z
return lerp(w, lerp(v, lerp(u, grad3(p[AA],   x, y,   z  ), grad3(p[BA],   x-1, y,   z  )),
                       lerp(u, grad3(p[AB],   x, y-1, z  ), grad3(p[BB],   x-1, y-1, z  ))),
               lerp(v, lerp(u, grad3(p[AA+1], x, y,   z-1), grad3(p[BA+1], x-1, y,   z-1)),
                       lerp(u, grad3(p[AB+1], x, y-1, z-1), grad3(p[BB+1], x-1, y-1, z-1))))
```

Indices maximaux : 511 (tableau de 512 suffisant). Aux points entiers du réseau, `evaluate` vaut exactement 0.

### 4.5 fBm — ⚠ résultat **renormalisé dans [0, 1]**

```
fbm2d(x, y, octaves, lacunarity, gain):
    sum = 0; amp = 1; freq = 1; maxAmp = 0
    for i in 0..octaves:
        sum += evaluate(x * freq, y * freq) * amp
        maxAmp += amp
        amp *= gain
        freq *= lacunarity
    return (sum / maxAmp + 1.0) * 0.5

fbm3d(x, y, z, octaves):   // gain 0.5 et lacunarité 2.0 codés en dur
    même boucle avec evaluate(x*freq, y*freq, z*freq), amp *= 0.5, freq *= 2.0
    return (sum / maxAmp + 1.0) * 0.5

octaveEvaluate(x, y, octaves, persistence, lacunarity) = fbm2d(x, y, octaves, lacunarity, persistence)   // inutilisé
```

Ordre exact des opérations : `sum += (evaluate(...) * amp)` ; `x * freq` est calculé à partir du `x`
déjà mis à l'échelle par l'appelant. Avec `octaves = 0`, `maxAmp = 0` → `0/0 = NaN` (non atteint par défaut).

---

## 5. `WorldConfig`

### 5.1 Champs, types, défauts

Colonne « persisté » : écrit **et** relu par `WorldSaveManager` (format actuel, §14).

| Groupe | Champ | Type Java | Défaut | Persisté | Usage |
|---|---|---|---|:-:|---|
| Terrain | `seaLevel` | int | 34 | ✔ | niveau d'eau (eau remplie jusqu'à `seaLevel-1`) |
| | `baseHeight` | int | 30 | ✔ | |
| | `heightScale` | int | 38 | ✔ | |
| | `heightNoiseOctaves` | int | 5 | ✔ | |
| | `heightNoiseLacunarity` | double | 2.0 | ✔ | |
| | `heightNoiseGain` | double | 0.45 | ✔ | |
| | `heightNoiseFrequency` | double | 0.01 | ✔ | UI « Terrain Detail » |
| Océan | `oceanFrequency` | double | 0.0012 | ✔ | UI « Continent Size » (inversé) |
| | `oceanThreshold` | double | 0.38 | ✔ | UI « Ocean Coverage » |
| | `coastThreshold` | double | 0.48 | ✔ | |
| | `oceanFbmOctaves` | int | 4 | ✔ | |
| | `oceanLacunarity` | double | 2.0 | ✔ | |
| | `oceanGain` | double | 0.45 | ✔ | |
| Rivières | `riverFrequency` | double | 0.003 | ✔ | |
| | `riverWidth` | double | 0.045 | ✔ | `<= 0` désactive les rivières |
| | `riverDepth` | int | 8 | ✔ | |
| | `riverBankWidthMult` | double | 1.556 | ✔ | berge = `riverWidth * mult` |
| Grottes | `cavesEnabled` | boolean | true | ✔ | désactive aussi les champignons souterrains |
| | `caveThreshold` | double | 0.42 | ✔ | |
| | `cavernThreshold` | double | 0.65 | ✔ | |
| | `caveDepthMax` | int | 60 | ✔ | |
| Biomes | `tempScale` | double | 0.0025 | ✔ | |
| | `humidScale` | double | 0.003 | ✔ | |
| | `mountainFrequency` | double | 0.006 | ✔ | |
| | `mountainThreshold` | double | 0.72 | ✔ | |
| Features | `snowLineMountains` | int | 58 | ✔ | |
| | `snowLineTaiga` | int | 52 | ✔ | |
| | `vegetationDensity` | double | 1.0 | ✔ | multiplie arbres, plantes, cactus, nénuphars |
| | `treeDensityOverrides` | double[8] | `[-1.0 ×8]` | ✔ | index = ordinal de `Biome` ; `< 0` = défaut du biome |
| Gameplay | `gravity` | float | -32.0 | ✔ | u/s² |
| | `jumpForce` | float | 8.4 | ✔ | |
| | `fallDamageEnabled` | boolean | true | ✔ | |
| | `dayLengthSeconds` | float | 1200.0 | ✔ | resynchronisé depuis le renderer à la sauvegarde |
| | `timeOfDay` | float | 900.0 | ✔ | « ticks [0..1200) », idem |
| | `walkSpeed` | float | 4.317 | **✘** | non persisté → revient au défaut au rechargement |
| | `sprintSpeed` | float | 5.612 | **✘** | idem |
| | `flySpeed` | float | 20.0 | **✘** | idem (vertical = ×0.6) |
| | `attackDamage` | float | 1.0 | **✘** | idem |
| Mobs | `mobSpawnRadius` | int | 64 | ✔ | |
| | `mobDespawnRadius` | int | 64 | ✔ | |
| Spawn | `spawnX`, `spawnY`, `spawnZ` | double | `NaN` | ✔ (si `spawnX` non NaN) | NaN = « non défini » |
| Minerais | `coalMinY / coalMaxY / coalChance` | int/int/double | 5 / 80 / 0.012 | ✔ | |
| | `ironMinY / ironMaxY / ironChance` | | 5 / 60 / 0.008 | ✔ | |
| | `goldMinY / goldMaxY / goldChance` | | 5 / 32 / 0.004 | ✔ | |
| | `diamondMinY / diamondMaxY / diamondChance` | | 5 / 16 / 0.002 | ✔ | |
| | `emeraldMinY / emeraldMaxY / emeraldChance` | | 5 / 30 / 0.001 | ✔ | |

Ordre des biomes pour `treeDensityOverrides` : PLAINS=0, FOREST=1, DESERT=2, TAIGA=3, JUNGLE=4, SWAMP=5, SAVANNA=6, MOUNTAINS=7.

`copy()` : copie profonde de **tous** les champs ci-dessus (y compris walk/sprint/fly/attack et le tableau).

### 5.2 Presets (`WorldConfig.preset(WorldType)`) — enum `WorldType { NORMAL, FLAT, AMPLIFIED, ISLAND }`

Partent tous de `new WorldConfig()` (défauts) puis :

| Type | Modifications exactes |
|---|---|
| `NORMAL` | aucune |
| `FLAT` | `seaLevel = 5`, `baseHeight = 5`, `heightScale = 1`, `cavesEnabled = false`, `riverWidth = 0.0` |
| `AMPLIFIED` | `heightScale = 80`, `baseHeight = 45`, `oceanThreshold = 0.50` (⚠ `coastThreshold` reste 0.48) |
| `ISLAND` | `oceanFrequency = 0.003`, `oceanThreshold = 0.20`, `coastThreshold = 0.30` |

L'UI de création cycle `NORMAL → FLAT → AMPLIFIED → ISLAND → NORMAL` (ordinal + 1 mod 4) et réapplique le preset
à chaque changement (ce qui écrase les réglages avancés). Le `WorldType` lui-même **n'est pas sauvegardé**,
seul le `WorldConfig` résultant l'est.

---

## 6. `BiomeSystem`

`enum Biome { PLAINS, FOREST, DESERT, TAIGA, JUNGLE, SWAMP, SAVANNA, MOUNTAINS }` (ordinaux 0..7).
`enum TreeType { OAK, BIRCH, SPRUCE, JUNGLE, ACACIA, SWAMP_OAK }` (BIRCH n'est utilisé par aucun biome).

### 6.1 Champs de bruit (arguments `double` ; les appelants passent des `int` promus)

```
TEMP_OFFSET_X = 500, TEMP_OFFSET_Z = 500, HUMID_OFFSET_X = -300, HUMID_OFFSET_Z = 800

temperature(wx, wz) = fbm2d((wx + 500) * tempScale,  (wz + 500) * tempScale,  3, 2.0, 0.45)
humidity(wx, wz)    = fbm2d((wx - 300) * humidScale, (wz + 800) * humidScale, 3, 2.0, 0.45)
                      // écrit (wx + (-300)) * humidScale
mountain(wx, wz)    = fbm2d(wx * mountainFrequency + 1000.0, wz * mountainFrequency + 1000.0, 4, 2.0, 0.5)
```

⚠ Ordre : pour temp/humid, addition **puis** multiplication ; pour mountain, multiplication **puis** addition.

### 6.2 Sélection (`getBiome`)

```
if mountain > mountainThreshold          → MOUNTAINS
temp = temperature ; humid = humidity     // calculés seulement si pas montagne (fonctions pures : ordre sans effet)
if temp > 0.6:
    if humid < 0.35 → DESERT
    if humid > 0.6  → JUNGLE
    → SAVANNA
if temp < 0.35   → TAIGA
if humid > 0.6   → SWAMP
if humid > 0.4   → FOREST
→ PLAINS
```

Comparaisons strictes. Pas de lissage entre biomes : un biome par colonne.

### 6.3 `BiomeConfig` par biome

| Biome | surface | sous-surface | profond | arbre | densité arbres | heightModifier |
|---|---|---|---|---|---:|---:|
| PLAINS | grass | dirt | stone | OAK | 0.004 | 0.7 |
| FOREST | grass | dirt | stone | OAK | 0.025 | 0.85 |
| DESERT | sand | sand | sandstone | OAK | 0.0 | 0.5 |
| TAIGA | grass | dirt | stone | SPRUCE | 0.018 | 0.9 |
| JUNGLE | grass | dirt | stone | JUNGLE | 0.035 | 1.0 |
| SWAMP | grass | dirt | **clay** | SWAMP_OAK | 0.012 | 0.4 |
| SAVANNA | grass | dirt | stone | ACACIA | 0.005 | 0.55 |
| MOUNTAINS | stone | stone | stone | SPRUCE | 0.006 | 1.8 |

Végétation (liste **ordonnée** `(bloc, chance)`) :

| Biome | entrées dans l'ordre |
|---|---|
| PLAINS | tall_grass 0.30, flower_red 0.02, flower_yellow 0.02 |
| FOREST | tall_grass 0.20, flower_red 0.01, mushroom_brown 0.005 |
| DESERT | dead_bush 0.01, cactus 0.003 |
| TAIGA | fern 0.18, tall_grass 0.08, mushroom_red 0.004 |
| JUNGLE | fern 0.30, tall_grass 0.25, flower_red 0.01, flower_yellow 0.01 |
| SWAMP | tall_grass 0.25, mushroom_brown 0.01, mushroom_red 0.005, lily_pad 0.04 |
| SAVANNA | tall_grass 0.35, dead_bush 0.008 |
| MOUNTAINS | tall_grass 0.05, fern 0.02 |

Teintes (RGB double, consommées par le mailleur et les particules) :

| Biome | grassTint | leavesTint | waterColor |
|---|---|---|---|
| PLAINS | 0.45, 0.74, 0.19 | 0.36, 0.73, 0.24 | 0.2, 0.5, 0.8 |
| FOREST | 0.35, 0.68, 0.15 | 0.30, 0.65, 0.18 | 0.18, 0.45, 0.7 |
| DESERT | 0.6, 0.65, 0.2 | 0.5, 0.6, 0.2 | 0.3, 0.6, 0.7 |
| TAIGA | 0.30, 0.55, 0.28 | 0.18, 0.42, 0.18 | 0.15, 0.35, 0.65 |
| JUNGLE | 0.22, 0.78, 0.15 | 0.15, 0.72, 0.12 | 0.1, 0.55, 0.55 |
| SWAMP | 0.40, 0.55, 0.22 | 0.35, 0.50, 0.18 | 0.25, 0.4, 0.2 |
| SAVANNA | 0.62, 0.68, 0.18 | 0.55, 0.65, 0.15 | 0.25, 0.55, 0.65 |
| MOUNTAINS | 0.35, 0.60, 0.22 | 0.25, 0.50, 0.20 | 0.15, 0.4, 0.75 |

### 6.4 `getSurfaceBlock(biome, height, cfg)`

```
if biome == MOUNTAINS && height > snowLineMountains → SNOW
if biome == TAIGA     && height > snowLineTaiga     → SNOW
→ config.surfaceBlock
```

(`height` = hauteur finale de la colonne, le bloc de surface étant en `y = height-1`.)

---

## 7. `WorldGenerator`

Construction : `noise = new Noise(seed)` ; `biomeSystem = new BiomeSystem(noise, cfg)` (partage le même `Noise`).
Le générateur garde une **référence** au `WorldConfig` (pas une copie).

### 7.1 Facteur océan — `oceanFactor(wx, wz)` (args double)

```
c = fbm2d(wx * oceanFrequency, wz * oceanFrequency, oceanFbmOctaves, oceanLacunarity, oceanGain)
if c < oceanThreshold → 0.0
if c < coastThreshold → (c - oceanThreshold) / (coastThreshold - oceanThreshold)
→ 1.0
```

### 7.2 Facteur rivière — `riverFactor(wx, wz)` (args double) — **bruit brut, pas fBm**

```
if riverWidth <= 0 → 1.0
bank = riverWidth * riverBankWidthMult
r1 = |evaluate(wx * riverFrequency + 300,        wz * riverFrequency + 300)|
r2 = |evaluate(wx * riverFrequency * 1.5 + 700,  wz * riverFrequency * 1.5 - 200)|
m = min(r1, r2)
if m < riverWidth → 0.0
if m < bank       → (m - riverWidth) / (bank - riverWidth)
→ 1.0
```

`wx * riverFrequency * 1.5` = `(wx * riverFrequency) * 1.5`.

### 7.3 Hauteur de colonne — `computeSurfaceHeight(int wx, int wz) -> int`

Code dupliqué à l'identique dans la passe 1 de `generateChunk`. Renvoie `height` tel que les blocs pleins
occupent `y ∈ [0, height-1]` (le bloc de surface est en `height-1`, le premier y libre est `height`).

```
biome = getBiome(wx, wz) ; bc = config(biome)
hn   = fbm2d(wx * heightNoiseFrequency, wz * heightNoiseFrequency,
             heightNoiseOctaves, heightNoiseLacunarity, heightNoiseGain)
flat = fbm2d(wx * 0.005 + 100, wz * 0.005 + 100, 3, 2.0, 0.5)
height = (int) floor( baseHeight + hn * heightScale * bc.heightModifier * (0.5 + flat * 0.8) )
         // évalué : (double)baseHeight + (((hn * heightScale) * heightModifier) * (0.5 + flat*0.8))

of = oceanFactor(wx, wz) ; rf = riverFactor(wx, wz)

if of < 1.0:
    oceanFloor = 12 + (int) floor(of * 10)                         // int
    height = (int) floor(oceanFloor + (height - oceanFloor) * of)   // (int - int) → double * of, + int
    if of < 0.3: height = min(height, 18)

if rf < 1.0 && height >= seaLevel - 2:
    carve = (int) floor(riverDepth * (1.0 - rf))
    riverBed = seaLevel - carve
    height = min(height, (int) floor(riverBed + (height - riverBed) * rf))

if biome == SWAMP && height > seaLevel - 2 && height < seaLevel + 3:
    height = min(height, seaLevel)

return clamp(height, 1, SIZE_Y - 1)    // max(1, min(height, 127))
```

### 7.4 `generateChunk(Chunk chunk)` — ordre global des passes

`wx0 = pos.x*16`, `wz0 = pos.z*16`. Le chunk arrive vide (tout AIR). **Ordre impératif** :

1. Passe colonnes (terrain + eau) — `lx` 0..15 (boucle externe), `lz` 0..15 (interne).
2. `generateCaves` (grottes + cavernes, puis champignons) — sauté entièrement si `!cavesEnabled`.
3. `generateOres`.
4. `generateLavaLakes`.
5. Arbres — `lx` 2..13 (externe), `lz` 2..13 (interne).
6. Végétation (plantes, cactus, nénuphars) — `lx` 0..15, `lz` 0..15.

Aucune passe ne lit ni n'écrit hors du chunk courant (pas de structures inter-chunks : les feuillages
qui débordent sont simplement coupés). Le résultat ne dépend que de `(seed, cfg, pos)`.

### 7.5 Passe 1 — remplissage d'une colonne

Après le calcul de `height` (§7.3, avec `of`, `rf`, `biome`, `bc` de la colonne) :

```
isNearRiver  = rf < 0.6
isOceanCoast = of < 0.8
surface = getSurfaceBlock(biome, height, cfg) ; sub = bc.subsurfaceBlock ; deep = bc.deepBlock

for y in 0 .. min(height, 128):            // exclusif
    if y == 0                    → BEDROCK
    elif y < height - 4          → deep
    elif y < height - 1          → (isNearRiver || isOceanCoast || height < seaLevel + 2) ? SAND : sub
    else  /* y == height-1 */    → if isNearRiver && height <= seaLevel + 1 → SAND
                                   elif isOceanCoast && height <= seaLevel + 2 → SAND
                                   else surface

for y in height .. seaLevel:               // exclusif
    if block(y) == AIR → WATER              // (toujours AIR à ce stade)
```

Conséquences : 3 couches de sous-surface (`height-4 .. height-2`), BEDROCK prioritaire en y=0 même pour une
colonne très basse.

### 7.6 Passe 2 — grottes (`generateCaves`, si `cavesEnabled`)

```
caveScale1 = 0.035 ; caveScale2 = 0.04
cavernMax = min(30, caveDepthMax / 2)                  // division entière
for lx in 0..16, lz in 0..16 (lx externe):
  wx, wz = wx0+lx, wz0+lz
  for y = 2; y < caveDepthMax; y += 2:                 // y PAIRS uniquement
    b = block(lx, y, lz)
    if b in {AIR, WATER, BEDROCK}: continue            // saute aussi la partie caverne
    n1 = fbm3d(wx * 0.035,       y * 0.035,       wz * 0.035,       1)
    n2 = fbm3d(wx * 0.04 + 500,  y * 0.04 + 500,  wz * 0.04 + 500,  1)
    if |n1| < caveThreshold && |n2| < caveThreshold:
        set(y) = AIR
        if y+1 < caveDepthMax && block(y+1) not in {AIR, WATER, BEDROCK}: set(y+1) = AIR
    if y < cavernMax:
        cav = fbm3d(wx * 0.015 + 1000, y * 0.02 + 1000, wz * 0.015 + 1000, 1)
        if cav > cavernThreshold:
            set(y) = AIR
            if y+1 < cavernMax: set(y+1) = AIR         // ⚠ inconditionnel (peut effacer de l'eau)
```

Notes : `fbm3d(…, 1)` = `(evaluate3 + 1) * 0.5` ∈ [0,1], donc `|n|` est inutile (toujours positif) ; la condition
réelle est « bruit brut < 2·0.42−1 = −0.16 » sur les deux champs. Les grottes ignorent la surface et l'eau :
elles peuvent percer le sol et laisser de l'eau suspendue au-dessus d'une cavité.

Champignons (toujours dans `generateCaves`, après la boucle ci-dessus) :

```
for lx in 1..15, lz in 1..15 (lx externe):
  for y = 3; y < 50; y += 3:                           // 3, 6, …, 48 — indépendant de caveDepthMax
    if block(y) != AIR: continue
    below = block(y-1) ; if below != STONE && below != DIRT: continue
    h = hashPosition(wx * 7 + y * 31, wz * 13 + y * 17)
    if h % 1000 < 8: set(y) = (h % 2 == 0) ? MUSHROOM_RED : MUSHROOM_BROWN
```

(S'applique aussi en surface si la surface est de la pierre/terre exposée à un y multiple de 3 < 50.)

### 7.7 Passe 3 — minerais

Table **ordonnée** (bloc, minY, maxY, chance, graine) : coal (cfg, 73), iron (137), gold (251), diamond (389), emerald (521).

```
for lx in 0..16, lz in 0..16 (lx externe):
  for y in 1..128:
    b = block(y) ; if b != STONE && b != SANDSTONE: continue
    for ore in table:
      if y < ore.minY || y > ore.maxY: continue        // bornes inclusives
      h = hashPosition(wx * ore.seed + y * 31, wz * ore.seed + y * 17)
      if (h % 10000) / 10000.0 < ore.chance: set(y) = ore.type ; break
```

Pas de filons : chaque bloc est tiré indépendamment. Remplace aussi la pierre de surface (montagnes) et le grès.

### 7.8 Passe 4 — « lacs » de lave

```
for lx, lz in 0..16: for y in 1..12: if block(y) == AIR → LAVA
```

Toute cavité (et tout air libre) sous y = 12 devient de la lave. Voir §18 pour l'effet sur FLAT.

### 7.9 Passe 5 — arbres

```
for lx in 2..14, lz in 2..14 (lx externe, bornes : 2 ≤ l ≤ 13):
  wx, wz ; biome = getBiome(wx, wz) ; bc = config(biome)
  base = treeDensityOverrides[biome.ordinal] >= 0 ? override : bc.treeDensity
  d = base * vegetationDensity
  if d <= 0: continue
  if (hashPosition(wx, wz) % 10000) / 10000.0 > d: continue
  rf = riverFactor(wx, wz) ; of = oceanFactor(wx, wz)
  if rf < 0.8 || of < 0.7: continue
  surfaceY = -1
  for y = 127 downto 0:
    b = block(y)
    if b in {GRASS, SAND, SNOW}: surfaceY = y ; break
    if b not in {AIR, TALL_GRASS, FLOWER_RED, FLOWER_YELLOW, FERN, DEAD_BUSH}: break
  if surfaceY < seaLevel || surfaceY > 113 /* SIZE_Y-15 */: continue
  if biome == DESERT: continue                          // même avec un override > 0
  placeTree(bc.treeType, lx, surfaceY + 1, lz)
```

Remarques : à ce stade il n'y a pas encore de végétation ; la recherche s'arrête sur des feuilles déjà posées
par un arbre précédent du même chunk, sur la pierre (pas d'arbre sur la pierre des montagnes, seulement sur la
neige), sur l'eau et sur la lave.

#### Formes d'arbres

Dans toutes les formes : `x, z` = coordonnées **locales**, `y` = `surfaceY + 1` (absolu). `cwx = pos.x*16`,
`cwz = pos.z*16`. Les **troncs** sont posés par `setBlock` sans test (écrasent tout ; hors limites ignoré).
Les **feuilles** ne sont posées que si `0 ≤ lx < 16`, `0 ≤ lz < 16`, `ly < 128` (et `ly ≥ 0` implicitement par
`Chunk`) **et** si le bloc cible est `AIR` (sauf mention contraire). Dans le code, la variable de boucle
verticale des feuillages (`dy`) est un **y absolu**.

**OAK** (`placeOakTree`) :
```
th = 4 + hashPosition(x + cwx, z + cwz) % 3                 // 4..6
tronc WOOD en y .. y+th-1
leafStart = y+th-1 ; leafEnd = y+th+2
for Y = leafStart ..= leafEnd:
  r = (Y == leafEnd) ? 1 : 2
  for dx = -r..=r, dz = -r..=r:
    skip si dx==0 && dz==0 && Y < leafEnd                   // ⇒ trous d'air en y+th et y+th+1 au centre
    skip si |dx|==r && |dz|==r && Y == leafStart            // coins coupés seulement sur la couche basse
    LEAVES
```

**BIRCH** (inutilisé par les biomes, à porter quand même) :
```
th = 5 + hashPosition(x + cwx + 11, z + cwz + 13) % 3       // 5..7
tronc BIRCH_WOOD y .. y+th-1
leafStart = y+th-2 ; leafEnd = y+th+1
r = (Y == leafEnd) ? 1 : (Y == leafStart ? 1 : 2)
skip centre si Y < leafEnd ; skip TOUS les coins (|dx|==r && |dz|==r) ; BIRCH_LEAVES
```

**SPRUCE** :
```
th = 6 + hashPosition(x + cwx + 23, z + cwz + 29) % 4       // 6..9
tronc SPRUCE_WOOD y .. y+th-1
for d = 2 .. th+1 (inclus):                                 // d relatif
  lft = (th + 1) - d
  r = min(3, (int)floor(lft * 0.6) + 1)                     // lft→r : 0→1,1→1,2→2,3→2,≥4→3
  if r < 1: continue                                        // jamais vrai
  Y = y + d ; if Y >= 128: break
  for dx, dz in -r..=r: skip centre ; skip coins ; SPRUCE_LEAVES (si AIR, x/z dans le chunk)
if y + th + 1 < 128:
  set(x, y+th,   z) = SPRUCE_LEAVES                         // inconditionnel (écrase)
  set(x, y+th+1, z) = SPRUCE_LEAVES
```

**JUNGLE** :
```
th = 8 + hashPosition(x + cwx + 37, z + cwz + 41) % 5       // 8..12
tronc JUNGLE_WOOD y .. y+th-1
leafStart = y+th-2 ; leafEnd = y+th+2
for Y = leafStart ..= leafEnd:
  dft = leafEnd - Y ; r = dft >= 2 ? 3 : (dft == 1 ? 2 : 1)
  skip centre si Y < leafEnd ; skip si |dx|+|dz| > r+1 ; JUNGLE_LEAVES
```

**ACACIA** :
```
th   = 4 + hashPosition(x + cwx + 53, z + cwz + 59) % 3     // 4..6
lean = hashPosition(x + cwx, z + cwz + 100) % 3 - 1          // -1, 0, +1
tx = x
for d in 0..th:
  if 0 <= tx < 16: set(tx, y+d, z) = ACACIA_WOOD
  if d == floor(th * 0.6): tx += lean                        // comparaison int == double ; th→seuil : 4→2, 5→3, 6→3
canopyY = y + th
for d in 0..2:
  r = d == 0 ? 3 : 2 ; Y = canopyY + d ; if Y >= 128: break
  for dx, dz in -r..=r: skip coins ; ACACIA_LEAVES en (tx+dx, Y, z+dz) si AIR et dans le chunk
```
(Le centre n'est pas sauté : la canopée repose sur le tronc décalé.)

**SWAMP_OAK** :
```
th = 4 + hashPosition(x + cwx + 67, z + cwz + 71) % 2       // 4..5
tronc WOOD y .. y+th-1
leafStart = y+th-1 ; leafEnd = y+th+2
r = (Y <= leafStart + 1) ? 3 : 1                            // deux couches 7×7 pleines puis deux 3×3
skip centre si Y < leafEnd ; pas de coupe des coins ; LEAVES
```

### 7.10 Passe 6 — végétation

```
for lx in 0..16, lz in 0..16 (lx externe):
  wx, wz ; biome ; bc
  for y = 127 downto seaLevel - 1 (inclus):
    b = block(y)
    if b in {GRASS, SAND, SNOW, STONE}:                      // « surface » (⚠ inclut STONE)
      above = y+1
      if above < 128 && block(above) == AIR:
        vegNoise = fbm2d(wx * 0.08 + 200, wz * 0.08 + 200, 2, 2.0, 0.5)
        hash = hashPosition(wx * 3 + 77, wz * 5 + 33)
        rval = (hash % 10000) / 10000.0
        for (type, chance) in bc.vegetation (dans l'ordre):
          if type == LILY_PAD: continue
          eff = chance * vegetationDensity
          if type == CACTUS:
            if b == SAND && rval < eff:
              for cy in 0 .. 1 + hash % 3: if above+cy < 128: set(above+cy) = CACTUS   // écrase
            continue                                         // ⚠ pas de break
          if vegNoise > 0.3 && rval < eff: set(above) = type ; break
      break                                                  // fin de colonne dans tous les cas
    if b == WATER && biome == SWAMP:
      if y == seaLevel - 1 && block(y+1) == AIR:
        h = hashPosition(wx * 11 + 99, wz * 7 + 55)
        if (h % 10000) / 10000.0 < 0.04 * vegetationDensity: set(y+1) = LILY_PAD
      break
    if b not in {AIR, TALL_GRASS, FLOWER_RED, FLOWER_YELLOW, FERN, DEAD_BUSH}: break
```

Conséquence (voir §18) : `rval` est le même pour toutes les entrées, donc seule la **première** entrée non-cactus
peut gagner ; les entrées suivantes de chance inférieure sont inatteignables. La végétation n'est pas filtrée
par biome de surface : herbe haute sur le sable des plages, sur la pierre des montagnes, etc. Les colonnes
dont le premier bloc non-air est sous `seaLevel-1` n'ont pas de végétation.

---

## 8. `Chunk`

### 8.1 Données

* `pos: ChunkPos`, `voxels: byte[32768]`, `lightMap: byte[32768]`, trois drapeaux `volatile boolean`
  (`dirty`, `meshDirty`, `lightDirty`), `heatSources: HashSet<Integer>` (indices des voxels de lave).
* Index Java : `x + y*16 + z*2048`. ⚠ `nb_core::math::block_index` utilise `x + 16 z + 256 y` en affirmant
  « identique au Java » — **c'est faux** (la doc `world.md` Java l'est aussi). Le layout interne est sans effet
  observable **sauf** : (a) le format brut `.bin` actuel (§14.3), (b) les indices stockés dans `heatSources`.
  Le Rust peut garder son layout, mais doit convertir à l'import d'anciens `.bin` et corriger le commentaire.

### 8.2 Comportement

| Méthode | Comportement exact |
|---|---|
| `new Chunk(pos)` | voxels à 0 (AIR), **lumière à 15 partout**, `dirty = meshDirty = lightDirty = true`, heatSources vide |
| `new Chunk(pos, voxels)` | **clone** du tableau ; lumière 15 ; trois drapeaux à true ; scanne les voxels et ajoute l'index de chaque lave |
| `setBlock(x,y,z,t)` | hors limites → ignoré silencieusement ; sinon écrit l'ID ; si `t == LAVA` ajoute l'index aux heatSources sinon le retire ; **met les trois drapeaux à true** (même si le bloc ne change pas) |
| `getBlock(x,y,z)` | hors limites → AIR ; sinon `fromId` (ID inconnu → AIR) |
| `getLight(x,y,z)` | hors limites → **15** ; sinon `lightMap & 0xFF` |
| `setLight(x,y,z,l)` | hors limites ignoré ; écrit `(byte) l` ; **ne touche pas aux drapeaux** |
| `getVoxels()/getLightMap()` | renvoient les tableaux internes (aliasés, pas de copie) |
| accesseurs drapeaux | lecture/écriture directes |

Les `heatSources` ne sont lues **nulle part** dans le code actuel (fonctionnalité morte).

### 8.3 Sémantique réelle des drapeaux (≠ doc Java)

| Drapeau | Signification réelle | Mis à true par | Mis à false par |
|---|---|---|---|
| `dirty` | à persister | constructeurs, `setBlock` | sauvegarde (unload / flush) |
| `lightDirty` | lumière à recalculer | constructeurs, `setBlock`, chargement (`setLightDirty(true)`) | fin du calcul de lumière |
| `meshDirty` | maillage à reconstruire | constructeurs, `setBlock`, `markMeshDirty` (voisins au chargement, fin de lumière, bords en `setBlock`) | `ChunkMeshManager` au lancement d'un maillage |

Conséquence : un chunk généré est `dirty` (la génération utilise `setBlock`), donc **tout chunk chargé finit
sauvegardé au déchargement**, même jamais modifié.

---

## 9. `ChunkSnapshot`

Copie profonde (voxels + lumière) d'un `Chunk` au moment de la construction, pour le maillage en thread.
`new ChunkSnapshot(null)` est permis : `getBlock` → AIR, `getLight` → 15. Hors limites : AIR / 15 (comme `Chunk`).
Utilisé par `ChunkMeshManager` (nb-mesh) pour les 9 chunks (3×3, index `(dz+1)*3 + (dx+1)`).

---

## 10. Propagation de la lumière (`ChunkManager.computeLightAsync`)

Un seul canal de lumière 0..15 (ciel + émission confondus), recalcul **complet** par chunk, sur un volume
étendu de 2 blocs dans les chunks voisins.

### 10.1 Volume de travail

* `lx ∈ [-2, 17]`, `y ∈ [-1, 128]`, `lz ∈ [-2, 17]` → 20 × 130 × 20 = 52 000 cellules.
* Index : `idx = (lx+2) + (y+1)*20 + (lz+2)*2600`.
* Deux tableaux `lightMap` et `attenMap` (octets), remis à 0 au début. File FIFO de triplets (capacité
  initiale 65 536, doublée au besoin — détail d'implémentation).
* `cache[9]` : chunks actifs `(pos.x+dx, pos.z+dz)` pour `dz` externe, `dx` interne, index `(dz+1)*3+(dx+1)` ;
  un voisin absent (`null`) se lit comme **AIR**.
* `blockAt(lx, y, lz)` : y hors `[0,128)` → AIR ; `lx<0` → chunk `cx=0`, `lx+16` ; `lx≥16` → `cx=2`, `lx-16` ; idem z.

### 10.2 « Bloquant »

`isLightBlocking(b) = !(b == AIR || b == WATER || b == LAVA || b.crossShaped || b == LILY_PAD || b.leavesTinted)`.

### 10.3 Passe colonne (lumière du ciel)

```
for lx = -2 ..= 17:            // externe
  for lz = -2 ..= 17:
    cur = 15
    for y = 128 downto -1:
      atten = 1 ; emission = 0
      if 0 <= y < 128:
        b = blockAt(lx, y, lz)
        emission = b.lightEmission
        atten = isLightBlocking(b) ? 15 : max(1, b.lightAttenuation)
      elif y < 0: atten = 15
      attenMap[idx] = atten
      if y < 128:
        cur = (atten == 1 && cur == 15) ? 15 : max(0, cur - atten)
        cur = max(cur, emission)
      lightMap[idx] = cur
      if cur > 0: enqueue(lx, y, lz)
```

Atténuations effectives : air/plantes/nénuphar 1, eau 2, feuilles 2, lave 3 (mais émission 15), tout bloc
« bloquant » 15 (quelle que soit sa valeur JSON). La lumière du ciel reste à 15 dans l'air **tant qu'elle n'a
rien traversé** ; après une feuille (15→13) elle décroît ensuite de 1 par bloc d'air. Ligne `y = 128` :
atten 1, lumière 15. Ligne `y = -1` : atten 15, lumière 0.

### 10.4 BFS

```
offsets = [(+1,0,0), (-1,0,0), (0,+1,0), (0,-1,0), (0,0,+1), (0,0,-1)]   // cet ordre
while file non vide:
  (cx,cy,cz) = défiler ; cL = lightMap[c]          // valeur courante (pas celle à l'enfilage)
  if cL <= 1: continue
  for off in offsets:
    n = c + off ; si hors [-2,17]×[-1,128]×[-2,17] : continue
    at = attenMap[n] ; if at >= 15: continue
    p = (off == (0,-1,0) && cL == 15 && at == 1) ? 15 : cL - at
    if p > lightMap[n]: lightMap[n] = p ; enqueue(n)
```

### 10.5 Écriture et effets

* Copie de `lightMap` pour `lx∈[0,16)`, `y∈[0,128)`, `lz∈[0,16)` dans le chunk cible via `setLight`.
* `targetChunk.lightDirty = false` ; `markMeshDirty` sur la cible et ses 4 voisins orthogonaux.
* Les voisins **ne sont pas** recalculés quand un bloc de bord change ; les sources à plus de 2 blocs dans un
  chunk voisin sont ignorées.

---

## 11. `ChunkManager`

### 11.1 État

* `renderDistance` (int, défaut 12 ; réglé ensuite par `setRenderDistance(settings.renderDistance)`, défaut des réglages 12).
* `activeChunks`, `generatingChunks`, `lightingChunks` : `ConcurrentHashMap` (les deux derniers servent d'ensembles « en cours »).
* `storage: IChunkStorage` ; `worldGenerators: ThreadLocal<WorldGenerator>` (construction sous verrou global — toutes
  les instances sont identiques) ; un pool `chunkExecutor` et un pool `lightingExecutor`, chacun
  `max(1, nbCœurs / 2)` threads, files non bornées.
* `currentPlayerChunkX/Z` : `volatile int`.

### 11.2 `update(Vector3f playerPosition)` (thread principal, chaque frame)

```
pcx = (int) floor(player.x / 16)  ; pcz = (int) floor(player.z / 16)     // division float
currentPlayerChunkX/Z = pcx/pcz
expected = [ (pcx+x, pcz+z) for x in -r..=r (externe) for z in -r..=r ]   // carré (2r+1)²
for pos in activeChunks.keys(): if pos ∉ expected: unload(pos)            // d'abord tous les déchargements
for pos in expected (dans l'ordre):
  if pos ∉ active && pos ∉ generating: loadChunkAsync(pos)
  elif pos ∈ active:
    c = active[pos]
    if c.lightDirty && pos ∉ lighting && hasNeighbors(pos): computeLightAsync(pos)
```

`hasNeighbors(pos)` : les 9 chunks du carré 3×3 (y compris lui-même) sont actifs.

Le déchargement se fait sur le **rayon de rendu**, pas sur `mobDespawnRadius` (doc Java erronée).

### 11.3 `unloadChunk(pos)` (thread principal)

Retire de `active` ; si `dirty` : `storage.saveChunk(chunk)` (sous verrou sur le storage, E/S **bloquante sur le
thread principal**) puis `dirty = false`.

### 11.4 `loadChunkAsync(pos)`

```
generating.add(pos)
tâche (chunkExecutor):
  try:
    if |pos.x - currentPlayerChunkX| > r+1 || |pos.z - currentPlayerChunkZ| > r+1: return   // tâche périmée
    chunk = (sous verrou storage) storage.hasChunk(pos) ? storage.loadChunk(pos) : null
    if chunk == null: chunk = new Chunk(pos) ; generator.generateChunk(chunk)
    chunk.lightDirty = true
    active[pos] = chunk
    markMeshDirty sur les 4 voisins orthogonaux (s'ils sont actifs)
  finally: generating.remove(pos)
```

Une exception dans la tâche est **avalée** par `submit` (le `finally` libère la position, retentée à la frame suivante).

### 11.5 `computeLightAsync(pos)`

`lighting.add(pos)` ; tâche sur `lightingExecutor` : même test de péremption (r+1), `target = active[pos]`
(absent → fin), calcul §10, `finally lighting.remove(pos)`. Tampons de travail par thread (`ThreadLocal`).

### 11.6 Accès blocs en coordonnées monde

```
getBlock(x, y, z):
  cx = (int) floor((float) x / 16) ; cz = (int) floor((float) z / 16)    // ⚠ via float
  chunk absent → AIR
  return chunk.getBlock(x - cx*16, y, z - cz*16)                         // y hors [0,128) → AIR

setBlock(x, y, z, t):
  même calcul ; chunk absent → ignoré silencieusement
  chunk.setBlock(lx, y, lz, t)                       // y hors limites → ignoré, mais la suite s'exécute
  if lx == 0  → markMeshDirty(cx-1, cz) ; if lx == 15 → markMeshDirty(cx+1, cz)
  if lz == 0  → markMeshDirty(cx, cz-1) ; if lz == 15 → markMeshDirty(cx, cz+1)

markMeshDirty(pos): si actif → meshDirty = true
isChunkLoaded(wx, wz): même calcul float → active.contains
computeSurfaceHeight(wx, wz): generator du thread appelant → §7.3
getChunk(pos), getActiveChunks(), getStorage()
shutdown(): shutdownNow() des deux pools (pas de sauvegarde ; l'appelant flushe avant)
```

`(float) x / 16` puis floor est équivalent à `x.div_euclid(16)` pour `|x| ≤ 2^24` ; au-delà, l'arrondi du
`float` peut décaler d'un chunk (cas limite, à reproduire seulement si on veut l'exactitude aux confins).

### 11.7 Interaction avec le mailleur (nb-mesh, pour information)

`ChunkMeshManager` ne maille un chunk que s'il est `meshDirty`, que ses 9 chunks 3×3 sont actifs **et qu'aucun
n'est `lightDirty`** ; il met alors `meshDirty = false` avant de lancer la tâche avec 9 `ChunkSnapshot`.

Conséquence : un chunk de l'anneau `r` n'a jamais tous ses voisins → jamais éclairé ; l'anneau `r-1` est
éclairé mais ne peut être maillé (voisins de l'anneau `r` éternellement `lightDirty`) → **seuls les chunks
jusqu'à `r-2` sont visibles**.

### 11.8 Concurrence (Java)

Les voxels sont lus et écrits sans verrou entre le thread principal (édition), les threads de lumière et les
copies de snapshot. Courses connues : un `setBlock` pendant un calcul de lumière remet `lightDirty = true`, puis la
tâche le remet à `false` à la fin → mise à jour de lumière perdue jusqu'à la prochaine édition. Le Rust doit
être correct (verrous), il n'est pas demandé de reproduire ces courses.

---

## 12. `World` (partie relevant de `nb-world`)

* Constructeur `World(long seed, IChunkStorage storage, WorldConfig cfg)` : crée `ChunkManager(storage, seed, cfg)` ;
  `biomeSystems = ThreadLocal(new BiomeSystem(new Noise(seed), cfg))` (même permutation que le générateur).
  Le reste (ECS, systèmes, particules, joueur) relève de `nb-ecs` / `nb-game`.
* `update(playerPos)` → `chunkManager.update` ; `update(playerPos, dt)` → chunks, particules (annexe A), position
  joueur pour le spawn de mobs, systèmes ECS.
* `getBlock / setBlock / isChunkLoaded / getChunk` : délégation au `ChunkManager`.
* `getBiomeSystem()` : instance du thread appelant.
* `findSpawnLocation() -> [x, y, z]` :

```
for radius = 0; radius <= 256; radius += 16:
  for ox = -radius; ox <= radius; ox += 16:          // externe
    for oz = -radius; oz <= radius; oz += 16:
      if |ox| != radius && |oz| != radius: continue   // périmètre seulement
      h = computeSurfaceHeight(ox, oz)
      if h > seaLevel: return [ox + 0.5, h + 1, oz + 0.5]
return [0.5, seaLevel + 5, 0.5]
```

  (`h` est déjà le premier y libre ; `h + 1` place les pieds un bloc au-dessus → chute d'un bloc.)
* `respawnPlayer()` : position = `(spawnX, spawnY, spawnZ)` en remplaçant **chaque** composante NaN par
  respectivement `0.5`, `seaLevel + 5`, `0.5` (ne réutilise pas `findSpawnLocation`).

---

## 13. Stockage des chunks

```java
interface IChunkStorage { void saveChunk(Chunk); Chunk loadChunk(ChunkPos); boolean hasChunk(ChunkPos); }
```

* Seuls les **voxels** sont persistés (pas la lumière, pas les drapeaux, pas les heatSources).
* `InMemoryChunkStorage` (monde du menu, jamais écrit sur disque) : `Map<ChunkPos, byte[]>` ; `saveChunk` stocke
  **la référence** au tableau du chunk (pas de copie) ; `loadChunk` → `new Chunk(pos, voxels)` (qui clone) ou `null`.
* `FileChunkStorage(worldDir)` : répertoire `worldDir/chunks` (créé au constructeur). Voir §14.3.
* Tous les appels du `ChunkManager` / `flushChunks` se font sous `synchronized(storage)`.

---

## 14. Persistance — format actuel (Java)

> Le Rust utilisera un **nouveau format versionné** (§17.6). Ce chapitre documente l'existant, utile pour un
> éventuel importeur.

### 14.1 Arborescence

```
saves/                              (relatif au répertoire de travail)
  <nom-assaini>/
    world.json
    chunks/
      c_<cx>_<cz>.bin               (ex. c_-3_12.bin)
```

`sanitize(name) = name.replaceAll("[^a-zA-Z0-9_\\- ]", "_").trim()` (chaque caractère interdit → `_`, puis
suppression des espaces/contrôles en bordure).

### 14.2 `WorldMetadata` et `world.json`

Champs et défauts : `name` (String), `seed` (long), `gameMode = "CREATIVE"`, `allowCheats = false`,
`worldConfig = new WorldConfig()`, `createdAt`, `lastPlayedAt` (ms epoch), `playerX = 0.5`, `playerY = 100.0`
(**sentinelle « jamais joué »**), `playerZ = 0.5`, `playerYaw = 0`, `playerPitch = 0`, `playerHealth = 10.0`,
`playerFlying = false`, `inventorySlots: List<{slot:int, blockName:String, count:int}>`.
Le constructeur `(name, seed, gameMode)` met `createdAt = lastPlayedAt = now`.

Écriture (texte UTF-8, à la main, `Double.toString` / `Float.toString` Java pour les nombres, ex. `0.0012`,
`1.0E-4`, `-32.0`) — structure exacte :

```
{
  "name": "<échappé>",
  "seed": <long>,
  "gameMode": "<échappé>",
  "allowCheats": <bool>,
  "createdAt": <long>,
  "lastPlayedAt": <long>,
  "playerX": <double>,
  "playerY": <double>,
  "playerZ": <double>,
  "playerYaw": <float>,
  "playerPitch": <float>,
  "playerHealth": <float>,
  "playerFlying": <bool>,
  "inventorySlots": [{"slot":0,"blockName":"dirt","count":12},…],
  "worldConfig": {
    "seaLevel": …, … (tous les champs « persistés » du §5.1, dans l'ordre du tableau, sauf spawn),
    "treeDensityOverrides": [-1.0,-1.0,…],
    …,
    "mobDespawnRadius": 64[,
    "spawnX": …,
    "spawnY": …,
    "spawnZ": …]            ← présent seulement si spawnX n'est pas NaN
  }
}
```

Ordre exact des clés de `worldConfig` : seaLevel, baseHeight, heightScale, heightNoiseOctaves,
heightNoiseLacunarity, heightNoiseGain, heightNoiseFrequency, oceanFrequency, oceanThreshold, coastThreshold,
oceanFbmOctaves, oceanLacunarity, oceanGain, riverFrequency, riverWidth, riverDepth, riverBankWidthMult,
cavesEnabled, caveThreshold, cavernThreshold, caveDepthMax, tempScale, humidScale, mountainFrequency,
mountainThreshold, snowLineMountains, snowLineTaiga, vegetationDensity, treeDensityOverrides, coal*, iron*,
gold*, diamond*, emerald* (MinY, MaxY, Chance), gravity, jumpForce, fallDamageEnabled, dayLengthSeconds,
timeOfDay, mobSpawnRadius, mobDespawnRadius, [spawnX, spawnY, spawnZ].
`escape(s)` : `\` → `\\`, `"` → `\"` uniquement.

Lecture (parseur ad hoc, **pas** un vrai JSON) :

* recherche de la **première** occurrence de `"clé"` dans le texte, puis du `:` suivant ; saut des espaces/`\n`/`\r` ;
* entier/long : caractères `[0-9-]` consécutifs (`"5.7"` → 5) ; vide → défaut (0 pour les champs de
  métadonnées, valeur courante pour `worldConfig`) ; `"-"` seul → `NumberFormatException` ;
* double : `[0-9.eE-]` (pas de `+`, pas de `NaN`/`Infinity` → défaut) ;
* booléen : préfixe `true` / `false`, sinon défaut (`false` pour les métadonnées) ;
* chaîne : entre les deux `"` suivant le `:` — **pas de désechappement** (un `\"` tronque la chaîne) ;
* `worldConfig` : sous-chaîne entre l'accolade suivant `"worldConfig"` et l'accolade fermante appariée ;
  absent → `new WorldConfig()` ; chaque champ absent garde son défaut ; `spawnX/Y/Z` absents → NaN ;
  `walkSpeed`, `sprintSpeed`, `flySpeed`, `attackDamage` ne sont jamais lus ;
* `treeDensityOverrides` : contenu entre `[` et le `]` suivant, découpé sur `,` ; élément illisible → défaut à
  cet index ou -1.0 ; ⚠ **la longueur du tableau est celle du fichier** (voir §18) ;
* `inventorySlots` : objets `{…}` entre `[` et le premier `]` ; entrées avec `blockName` vide ignorées.

### 14.3 Fichier de chunk `.bin`

Exactement les 32 768 octets bruts de `voxels` dans l'ordre Java `x + 16·y + 2048·z`, sans en-tête, sans
compression, sans version. Lecture : lit jusqu'à 32 768 octets ; fichier plus court → le reste est AIR (0) ;
octets en trop ignorés. Erreur d'E/S → message stderr, `loadChunk` renvoie `null` (le chunk est alors
**régénéré**), `saveChunk` abandonne.

### 14.4 Opérations `WorldSaveManager`

| Opération | Comportement |
|---|---|
| `worldDir(name)` | `saves/` + `sanitize(name)` |
| `listWorlds()` | sous-répertoires de `saves/` contenant `world.json` ; `loadMetadata(nomDuRépertoire)` ; erreur → ignoré (stderr) ; tri `lastPlayedAt` décroissant |
| `createWorldDirectory(meta)` | `base = sanitize(meta.name)` (vide → `"world"`) ; si existe : premier `base_n` libre pour n = 2, 3, … et **`meta.name = base_n`** ; crée `dir/chunks` ; `saveMetadata(meta)` |
| `saveMetadata(meta)` | écrit `worldDir(meta.name)/world.json` (crée le dossier) ; `worldConfig == null` → écrit les défauts |
| `loadMetadata(dirName)` | lit `saves/<dirName>/world.json` (lève si absent) |
| `flushChunks(world)` | pour chaque chunk actif `dirty` : `saveChunk` puis `dirty = false` |
| `saveWorldState(ctx)` | si monde nommé : `flushChunks` ; recharge la méta existante (échec → nouvelle avec `createdAt = now`) ; `lastPlayedAt = now` ; recopie `dayLengthSeconds`/`timeOfDay` depuis le renderer dans le `WorldConfig` vivant ; position joueur (pieds), yaw = `camera.rotation.y`, pitch = `camera.rotation.x`, santé, `gameMode.name()`, `flying`, slots d'inventaire non vides ; `saveMetadata` |
| `applyPlayerState(ctx, meta)` | si `meta.playerY == 100.0` (égalité exacte) : `findSpawnLocation()` et **écrit** `spawnX/Y/Z` dans le `WorldConfig` ; place joueur et caméra (yeux à +1.62) ; santé (`alive = santé > 0`) ; vol ; inventaire (blocs inconnus ignorés) |
| `worldNameExists(name)` | `worldDir(name).exists()` |
| `deleteWorld(name)` | suppression récursive (échecs ignorés) |
| `renameWorld(meta, newName)` | `File.renameTo` (échec ignoré) ; `meta.name = newName` ; `saveMetadata` |
| `cloneWorld(src, newName)` | nouvelle méta `(newName, src.seed, src.gameMode)`, `worldConfig = src.worldConfig.copy()` ; **ne copie pas** `allowCheats` ni l'état joueur (le clone repart du spawn) ; `createWorldDirectory` ; copie récursive de `chunks/` |

Appels : sauvegarde au menu pause (« MAIN MENU »), avant de charger un autre monde, et à la fermeture
(`saveWorldState` puis `shutdown`). Pas de sauvegarde automatique périodique.

---

## 15. Pièges numériques — liste de contrôle

1. **`int` enveloppant** : `hashPosition` et tous ses arguments, `pos.x * 16`, `x + chunkX*16`. Utiliser `wrapping_*`.
2. **`>>` signé** dans `hashPosition` (pas `>>>`).
3. **`%` sur valeurs ≥ 0** uniquement dans le générateur (le hash est masqué) : `%` Rust convient.
4. **`(hash % 10000) / 10000.0`** : reste entier puis division **double**.
5. **Promotions** : `wx * cfg.x` avec `wx: int` → `f64::from(wx) * x`. `baseHeight + …` : int promu en double.
6. **`(int) Math.floor(d)`** : `d.floor() as i32` (Rust sature comme Java, NaN → 0).
7. **Ordre d'évaluation** des flottants : respecter l'associativité gauche (`wx * f * 1.5`, `hn * scale * mod * (…)`).
8. **Décalages** : temp/humid = `(wx + off) * scale` ; mountain = `wx * f + 1000.0` ; flatness = `wx * 0.005 + 100`.
9. **Comparaison int == double** (acacia) : `f64::from(d) == (th as f64 * 0.6).floor()`.
10. **Division entière** : `caveDepthMax / 2` (int, tronquée vers zéro — valeurs positives).
11. **fBm dans [0,1]** (pas [-1,1]) ; rivière = `evaluate` brut dans [-1,1].
12. **`fbm3d` 1 octave** : `maxAmp = 1`, résultat `(n + 1) * 0.5`.
13. **Bornes** : `y < height - 4` / `y < height - 1` ; `for y in height..seaLevel` ; `y >= seaLevel - 1` (végétation).
14. **`(float)x / 16`** pour le chunk d'un bloc monde ; `player.x / 16` en `f32`.
15. **Lumière** : octets non signés (`& 0xFF`) ; `cL <= 1` ne se propage pas ; cas spécial de la descente à 15.
16. **`java.util.Random`** : seulement les 48 bits bas de la graine ; 255 appels `nextInt(i+1)`, `i` décroissant.
17. **Pas de FMA**, pas de `-ffast-math`, pas d'optimisation `x*0.5` ↔ `x/2` qui change l'ordre (celles-ci sont exactes, mais ne pas « simplifier » `(a+1)*0.5` en `a*0.5+0.5`).

---

## 16. Vecteurs de référence (produits par le Java d'origine)

Définitions :

* `fnv(chunk)` = FNV-1a 64 bits (`h = 0xcbf29ce484222325 ; h ^= octet ; h *= 0x100000001b3`) sur les 32 768 IDs
  parcourus dans l'**ordre Java** `idx = x + 16·y + 2048·z`, `idx` croissant (indépendant du layout Rust).
* « chunk(cx,cz) » = `new Chunk(pos)` puis `generateChunk` avec `WorldConfig.preset(type)`.
* Lumière = algorithme §10 sur le chunk (0,0) avec ses 8 voisins **fraîchement générés**, empreinte FNV-1a de la
  lumière du chunk central dans le même ordre ; `hist[l]` = nombre de voxels de niveau `l`.

### 16.1 Primitives (seed 12345)

```
perm[0..16]                   = 200 42 20 121 182 178 7 39 194 118 164 160 251 166 254 53
evaluate(0.5, 0.5)            = 0.0
evaluate(12.3, -4.7)          = -0.11912406912000098
evaluate(1.1, 2.2, 3.3)       = 0.17309989174425583
evaluate(-7.25, 0.5, 100.75)  = -0.18932104110717773
fbm2d(1.37, -2.11, 5, 2.0, 0.45) = 0.49202660249155644
fbm3d(3.5, 1.25, -9.75, 1)    = 0.5923972129821777
hashPosition(0,0)             = 991465577
hashPosition(1,0)             = 1329105385
hashPosition(0,1)             = 212133164
hashPosition(-1,-1)           = 753772374
hashPosition(123456,-654321)  = 112537465
hashPosition(2147483647,-2147483648) = 835488806
"Norrblock".hashCode()        = -1910315700
```

### 16.2 Biomes / hauteurs (seed 12345, NORMAL)

| (wx, wz) | biome | temp | humid | mountain | computeSurfaceHeight |
|---|---|---|---|---|---:|
| (0,0) | FOREST | 0.4583829801850889 | 0.5767199132853252 | 0.5 | 26 |
| (100,-50) | FOREST | 0.44990601878663955 | 0.42848650605143723 | 0.4937034741759807 | 24 |
| (-300,700) | FOREST | 0.424357034795764 | 0.42862324962178516 | 0.47181624046935433 | 12 |
| (1000,1000) | FOREST | 0.46108991475761746 | 0.5182222801834794 | 0.5 | 26 |
| (-2048,513) | TAIGA | 0.2910663921521526 | 0.44903738395416154 | 0.526077648758023 | 49 |
| (4000,-4000) | FOREST | 0.516753734871407 | 0.5237990495583661 | 0.5 | 26 |

### 16.3 Chunks générés

Format : `chunk(cx,cz) fnv` puis décompte par bloc.

**NORMAL, seed 12345** — spawn `(-31.5, 49.0, -31.5)`
```
(0,0)    0x3300759565bbadfe air=24064 stone=5215 water=2033 sand=1024 bedrock=256 clay=63 coal_ore=53 iron_ore=26 gold_ore=11 diamond_ore=7 emerald_ore=6 lava=10
(-1,2)   0x1d923785e1bba4db air=24642 grass=9 water=1663 sand=1005 bedrock=256 clay=4915 tall_grass=4 lily_pad=8 lava=266
(5,-3)   0xc9333655e4eebcf7 air=24370 stone=4983 water=2244 sand=775 bedrock=256 coal_ore=51 iron_ore=22 gold_ore=12 diamond_ore=16 emerald_ore=7 lava=32
(-17,-9) 0x666b0b07e783bb97 air=21585 dirt=768 grass=253 stone=9443 bedrock=256 wood=12 leaves=154 tall_grass=42 coal_ore=115 iron_ore=90 gold_ore=36 diamond_ore=7 emerald_ore=7
(30,30)  0x73eb580f4c326acb air=24990 stone=4198 water=2048 sand=744 bedrock=256 coal_ore=42 iron_ore=15 gold_ore=11 diamond_ore=7 emerald_ore=5 lava=452
lumière (0,0) fnv=0x3f00fe567734ed11 hist=[6902,256,0,256,0,256,0,256,0,256,0,256,0,256,0,24074]
```
**NORMAL, seed 0** — spawn `(-31.5, 46.0, 32.5)`
```
(0,0)    0xd533636a3f60da83   (-1,2) 0x57ce6f46fbfa3c60   (5,-3) 0xb79db8dd3c12a932
(-17,-9) 0x9d0ac739169ab0e0   (30,30) 0x8567b76f1f58694e
(-1,2) : air=20673 dirt=768 grass=256 stone=10341 bedrock=256 wood=9 leaves=154 tall_grass=47 coal_ore=134 iron_ore=83 gold_ore=31 diamond_ore=7 emerald_ore=9
```
**NORMAL, seed -987654321012345** — spawn `(-15.5, 44.0, -15.5)`
```
(0,0) 0xa8eb4fec7123907d  (-1,2) 0x7ebe349aedb21e8c  (5,-3) 0xd5925b5c99b342e3  (-17,-9) 0x54e41135ad2629a0  (30,30) 0x6e7ffdf6b19d6892
```
**NORMAL, seed 42** — spawn `(-31.5, 47.0, 0.5)`
```
(0,0) 0xc38bda97bc01342e  (-1,2) 0x17e0d2cb6cedcb9f  (5,-3) 0x403c38ceda013d21  (-17,-9) 0xa132b279e363f2aa  (30,30) 0x09d7b49732612653
(-17,-9) : air=22648 dirt=756 grass=246 stone=8444 bedrock=256 acacia_wood=5 acacia_leaves=51 tall_grass=80 coal_ore=105 iron_ore=79 gold_ore=36 diamond_ore=7 emerald_ore=7 lava=48
(30,30)  : air=23088 dirt=768 grass=256 stone=8041 bedrock=256 tall_grass=82 mushroom_brown=1 mushroom_red=1 coal_ore=87 iron_ore=42 gold_ore=19 diamond_ore=8 emerald_ore=5 lava=114
```
**FLAT** — empreinte « plaine standard » `0x4bd00d1336dd5925` = `air=29696 grass=256 sand=768 bedrock=256 lava=1792`
```
seed 12345 : spawn (32.5, 8.0, -31.5) ; (0,0),(-1,2),(-17,-9),(30,30) = 0x4bd00d1336dd5925 ; (5,-3) = 0x404eeb2ba8464d63
             lumière (0,0) fnv=0xeb539c0b961c1625 hist=[1280,0,…,0,31488]
seed 0     : spawn (-15.5, 8.0, -15.5) ; (-17,-9) = 0x33dd3bf02f06ad16 ; autres = standard
seed -987654321012345 : spawn (-31.5, 7.0, 32.5) ; (-1,2) = 0x8fb02bff1f940363 ; autres = standard
seed 42    : spawn (16.5, 8.0, 16.5) ; (0,0) = 0x9c17584089e39da3 ; (-1,2) = 0x12b28f3f66bdaf85 ; autres = standard
```
**AMPLIFIED**
```
seed 12345 : spawn (-31.5, 85.0, -31.5) ; (0,0) 0xe0b6b60b34d6fc42 (-1,2) 0xb5111373eb7bcdd9 (5,-3) 0x64ccbc434916e59a (-17,-9) 0x6477f4934fe3e1b2 (30,30) 0x73eb580f4c326acb
             lumière (0,0) fnv=0x3f00fe567734ed11
seed 0     : spawn (16.5, 37.0, 16.5) ; (0,0) 0xa92175878026f14c (-1,2) 0x97fc1b79d2ffbfe2 (5,-3) 0xa5dbd3f3be9ec6b6 (-17,-9) 0x504aad36e7fc08a5 (30,30) 0x95d90a9fd5d0fedc
seed -987654321012345 : spawn (-15.5, 71.0, -15.5) ; (0,0) 0xe967695c4cf81657 (-1,2) 0x119c3960184ca3df (5,-3) 0xec6a9035462ccdd6 (-17,-9) 0x111cd49328a00bb4 (30,30) 0x64d104252a33b1d2
seed 42    : spawn (-15.5, 36.0, 0.5) ; (0,0) 0x9ccaa2549ae3cd9b (-1,2) 0xe306bbee9efeaab5 (5,-3) 0x03ea3e60c4cdf1a7 (-17,-9) 0xa2314bfe5aa5b57e (30,30) 0xcfbe0f4391adc378
```
**ISLAND**
```
seed 12345 : spawn (-31.5, 49.0, -31.5) ; (0,0) 0x3300759565bbadfe (-1,2) 0x1d923785e1bba4db (5,-3) 0xf5a064046d57e020 (-17,-9) 0x666b0b07e783bb97 (30,30) 0x73eb580f4c326acb
seed 0     : spawn (-31.5, 46.0, 32.5) ; (0,0) 0xd533636a3f60da83 (-1,2) 0x57ce6f46fbfa3c60 (5,-3) 0xb79db8dd3c12a932 (-17,-9) 0x9e0bf0b5fbcdd787 (30,30) 0x8567b76f1f58694e
             (-17,-9) : air=22431 dirt=768 grass=256 stone=8291 bedrock=256 jungle_wood=22 jungle_leaves=256 fern=60 coal_ore=104 iron_ore=80 gold_ore=33 diamond_ore=7 emerald_ore=6 lava=198
seed -987654321012345 : spawn (-15.5, 44.0, -15.5) ; (0,0) 0xa8eb4fec7123907d (-1,2) 0x5a66de52d7de84bc (5,-3) 0xd5925b5c99b342e3 (-17,-9) 0x54e41135ad2629a0 (30,30) 0x6e7ffdf6b19d6892
seed 42    : spawn (-31.5, 47.0, 0.5) ; (0,0) 0xe26b0ab65a81bce5 (-1,2) 0x4b1098ea8b024360 (5,-3) 0x403c38ceda013d21 (-17,-9) 0xa132b279e363f2aa (30,30) 0x09d7b49732612653
```

Ces valeurs doivent devenir des tests `#[test]` de `nb-world` (au minimum : primitives, 16.2, et les cinq chunks
de NORMAL/12345 + lumière).

---

## 17. Contrat Rust proposé

Principes : données pures et `Send + Sync` ; pas de `ThreadLocal` (le `WorldGenerator` est immuable après
construction, une seule instance `Arc` partagée est équivalente aux instances par thread du Java) ; IDs de blocs
résolus une fois via `nb_data::BlockIds` ; pas d'ECS dans `nb-world`.

### 17.1 Configuration

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldType { Normal, Flat, Amplified, Island }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorldConfig {
    pub sea_level: i32, pub base_height: i32, pub height_scale: i32,
    pub height_noise_octaves: i32, pub height_noise_lacunarity: f64,
    pub height_noise_gain: f64, pub height_noise_frequency: f64,
    pub ocean_frequency: f64, pub ocean_threshold: f64, pub coast_threshold: f64,
    pub ocean_fbm_octaves: i32, pub ocean_lacunarity: f64, pub ocean_gain: f64,
    pub river_frequency: f64, pub river_width: f64, pub river_depth: i32, pub river_bank_width_mult: f64,
    pub caves_enabled: bool, pub cave_threshold: f64, pub cavern_threshold: f64, pub cave_depth_max: i32,
    pub temp_scale: f64, pub humid_scale: f64, pub mountain_frequency: f64, pub mountain_threshold: f64,
    pub snow_line_mountains: i32, pub snow_line_taiga: i32, pub vegetation_density: f64,
    pub tree_density_overrides: [f64; 8],          // -1.0 = défaut du biome
    pub gravity: f32, pub jump_force: f32, pub fall_damage_enabled: bool,
    pub day_length_seconds: f32, pub time_of_day: f32,
    pub walk_speed: f32, pub sprint_speed: f32, pub fly_speed: f32, pub attack_damage: f32,
    pub mob_spawn_radius: i32, pub mob_despawn_radius: i32,
    pub spawn: Option<[f64; 3]>,                   // remplace le triplet NaN (voir §18 pour la compat)
    pub ores: OreConfig,                           // ou champs à plat coal_min_y…, au choix
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OreRange { pub min_y: i32, pub max_y: i32, pub chance: f64 }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OreConfig { pub coal: OreRange, pub iron: OreRange, pub gold: OreRange, pub diamond: OreRange, pub emerald: OreRange }

impl Default for WorldConfig { /* §5.1 */ }
impl WorldConfig { pub fn preset(t: WorldType) -> Self; }
```

Décision à acter : soit `spawn: Option<[f64;3]>`, soit trois `f64` NaN — le comportement de `respawnPlayer`
(remplacement composante par composante) doit être conservé ; avec `Option`, l'écrire comme « None → (0.5,
seaLevel+5, 0.5) » est équivalent puisque le Java écrit toujours les trois ensemble.

### 17.2 Bruit, biomes

```rust
pub struct Noise { p: [u8; 512] }
impl Noise {
    pub fn new(seed: i64) -> Self;                         // §4.1 via nb_core::JavaRandom
    pub fn evaluate2(&self, x: f64, y: f64) -> f64;
    pub fn evaluate3(&self, x: f64, y: f64, z: f64) -> f64;
    pub fn fbm2d(&self, x: f64, y: f64, octaves: i32, lacunarity: f64, gain: f64) -> f64;
    pub fn fbm3d(&self, x: f64, y: f64, z: f64, octaves: i32) -> f64;
}

#[repr(u8)] #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Biome { Plains, Forest, Desert, Taiga, Jungle, Swamp, Savanna, Mountains }   // ordinaux 0..7
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeType { Oak, Birch, Spruce, Jungle, Acacia, SwampOak }

pub struct VegetationEntry { pub block: BlockId, pub chance: f64 }
pub struct BiomeConfig {
    pub surface: BlockId, pub subsurface: BlockId, pub deep: BlockId,
    pub tree_type: TreeType, pub tree_density: f64, pub vegetation: Vec<VegetationEntry>,
    pub height_modifier: f64, pub grass_tint: [f64; 3], pub leaves_tint: [f64; 3], pub water_color: [f64; 3],
}
pub struct BiomeSystem { noise: Arc<Noise>, cfg: Arc<WorldConfig>, configs: [BiomeConfig; 8] }
impl BiomeSystem {
    pub fn new(noise: Arc<Noise>, cfg: Arc<WorldConfig>, ids: &BlockIds) -> Self;
    pub fn temperature(&self, wx: f64, wz: f64) -> f64;
    pub fn humidity(&self, wx: f64, wz: f64) -> f64;
    pub fn mountain_factor(&self, wx: f64, wz: f64) -> f64;
    pub fn biome(&self, wx: f64, wz: f64) -> Biome;
    pub fn config(&self, b: Biome) -> &BiomeConfig;
    pub fn surface_block(&self, b: Biome, height: i32) -> BlockId;
}
```

### 17.3 Chunk

```rust
pub use nb_core::math::{CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, CHUNK_VOLUME};

/// Index Java (pour l'import legacy et les vecteurs de test) : x + 16*y + 2048*z.
pub const fn java_index(x: usize, y: usize, z: usize) -> usize;

pub struct Chunk {
    pos: ChunkPos,
    voxels: Box<[BlockId; CHUNK_VOLUME]>,
    light: Box<[u8; CHUNK_VOLUME]>,
}
impl Chunk {
    pub fn new(pos: ChunkPos) -> Self;                                 // AIR, lumière 15
    pub fn from_voxels(pos: ChunkPos, voxels: &[BlockId]) -> Self;     // copie ; court → complété par AIR
    pub fn pos(&self) -> ChunkPos;
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId;            // hors limites → AIR (0)
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, id: BlockId) -> bool; // false si hors limites
    pub fn light(&self, x: i32, y: i32, z: i32) -> u8;                 // hors limites → 15
    pub fn set_light(&mut self, x: i32, y: i32, z: i32, l: u8);
    pub fn voxels(&self) -> &[BlockId; CHUNK_VOLUME];
    pub fn light_map(&self) -> &[u8; CHUNK_VOLUME];
    pub fn heat_sources(&self, lava: BlockId) -> impl Iterator<Item = (usize, usize, usize)> + '_; // calcul à la demande (inutilisé en Java)
}

/// Drapeaux partagés hors verrou (équivalent des `volatile`).
#[derive(Default)]
pub struct ChunkFlags { pub dirty: AtomicBool, pub mesh_dirty: AtomicBool, pub light_dirty: AtomicBool }

/// Entrée de la table des chunks actifs.
pub struct ChunkEntry { pub chunk: RwLock<Chunk>, pub flags: ChunkFlags }
pub type SharedChunk = Arc<ChunkEntry>;

/// Copie immuable pour le mailleur (nb-mesh). `None` → AIR / lumière 15.
#[derive(Clone)]
pub struct ChunkSnapshot { voxels: Option<Box<[BlockId; CHUNK_VOLUME]>>, light: Option<Box<[u8; CHUNK_VOLUME]>> }
impl ChunkSnapshot {
    pub fn of(chunk: Option<&Chunk>) -> Self;
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId;
    pub fn light(&self, x: i32, y: i32, z: i32) -> u8;
}
```

Règle : toute écriture de bloc passant par le monde (`ChunkManager::set_block`) **et** par le générateur doit
aboutir à `dirty = mesh_dirty = light_dirty = true` (le générateur peut écrire directement dans `Chunk` puis
l'appelant pose les trois drapeaux — le résultat observable est identique puisque `new Chunk` les pose déjà).

### 17.4 Générateur

```rust
pub struct WorldGenerator { noise: Arc<Noise>, biomes: BiomeSystem, cfg: Arc<WorldConfig>, ids: BlockIds }
impl WorldGenerator {
    pub fn new(seed: i64, cfg: Arc<WorldConfig>, ids: &BlockIds) -> Self;
    pub fn compute_surface_height(&self, wx: i32, wz: i32) -> i32;
    pub fn generate_chunk(&self, chunk: &mut Chunk);       // §7.4, chunk supposé vide
    pub fn biome_system(&self) -> &BiomeSystem;
}
pub fn hash_position(x: i32, z: i32) -> i32;                // pub(crate) + tests
```

### 17.5 Stockage

```rust
pub trait ChunkStorage: Send + Sync {
    /// Persiste les voxels (pas la lumière).
    fn save_chunk(&self, pos: ChunkPos, voxels: &[BlockId; CHUNK_VOLUME]) -> nb_core::Result<()>;
    /// `Ok(None)` si absent. Une erreur de lecture doit conduire l'appelant à régénérer (comme le Java).
    fn load_chunk(&self, pos: ChunkPos) -> nb_core::Result<Option<Box<[BlockId; CHUNK_VOLUME]>>>;
    fn has_chunk(&self, pos: ChunkPos) -> bool;
}
pub struct InMemoryChunkStorage { /* Mutex<HashMap<ChunkPos, Box<[u8; N]>>> — copie à l'enregistrement */ }
pub struct FileChunkStorage { chunks_dir: PathBuf }
impl FileChunkStorage { pub fn new(world_dir: &Path) -> nb_core::Result<Self>; }   // crée chunks/
```

### 17.6 Nouveau format de sauvegarde (proposition)

* `saves/<nom-assaini>/world.json` sérialisé par `serde_json` : `{ "formatVersion": 1, "name", "seed", "gameMode",
  "allowCheats", "createdAt", "lastPlayedAt", "player": {...}, "inventorySlots": [...], "worldConfig": {...} }`
  — désérialisation tolérante (`#[serde(default)]`) pour reproduire « champ absent → défaut ».
* Chunks : `chunks/c_<cx>_<cz>.nbc` = en-tête `b"NBCK"` + `u16` version (1) + `u8` layout (0 = ordre Java
  `x+16y+2048z`, 1 = ordre `nb_core::block_index`) + `u8` compression (0 = brut, 1 = RLE) + charge utile 32 768 IDs.
* Importeur facultatif : si `world.json` sans `formatVersion` → parseur legacy (§14.2) ; `.bin` → layout Java.
* Garder `sanitize`, la résolution de collision `_2, _3…`, le tri `lastPlayedAt` décroissant et la sentinelle
  « jamais joué » (remplaçable par `player: null`, avec le même effet : recherche de spawn + écriture du spawn).

### 17.7 Gestionnaire de chunks et monde

```rust
pub struct ChunkManager { /* render_distance: AtomicI32, active: RwLock<HashMap<ChunkPos, SharedChunk>>,
                             generating/lighting: Mutex<HashSet<ChunkPos>>, storage: Arc<dyn ChunkStorage>,
                             generator: Arc<WorldGenerator>, pools (rayon ou threads + crossbeam),
                             player_chunk: (AtomicI32, AtomicI32), registry: Arc<BlockRegistry> */ }
impl ChunkManager {
    pub fn new(storage: Arc<dyn ChunkStorage>, seed: i64, cfg: Arc<WorldConfig>, registry: Arc<BlockRegistry>) -> Self;
    pub fn set_render_distance(&self, r: i32);
    pub fn update(&self, player_pos: glam::Vec3);           // §11.2 (déchargements puis chargements/lumière)
    pub fn chunk(&self, pos: ChunkPos) -> Option<SharedChunk>;
    pub fn active_positions(&self) -> Vec<ChunkPos>;
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId;
    pub fn set_block(&self, x: i32, y: i32, z: i32, id: BlockId);
    pub fn is_chunk_loaded(&self, wx: i32, wz: i32) -> bool;
    pub fn compute_surface_height(&self, wx: i32, wz: i32) -> i32;
    pub fn flush_dirty(&self) -> nb_core::Result<()>;        // = WorldSaveManager.flushChunks
    pub fn shutdown(&self);                                  // annule les tâches en attente, n'écrit rien
}
pub fn compute_light(center: ChunkPos, neighbors: &[Option<&Chunk>; 9], reg: &BlockRegistry) -> Box<[u8; CHUNK_VOLUME]>; // §10, pur, testable

pub struct World { chunks: ChunkManager, biomes: BiomeSystem, cfg: Arc<WorldConfig> }
impl World {
    pub fn new(seed: i64, storage: Arc<dyn ChunkStorage>, cfg: WorldConfig, registry: Arc<BlockRegistry>) -> Self;
    pub fn update(&self, player_pos: glam::Vec3);
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId;
    pub fn set_block(&self, x: i32, y: i32, z: i32, id: BlockId);
    pub fn chunk_manager(&self) -> &ChunkManager;
    pub fn biome_system(&self) -> &BiomeSystem;
    pub fn config(&self) -> &WorldConfig;                    // mutation (spawn, timeOfDay) via méthode dédiée ou RwLock
    pub fn find_spawn_location(&self) -> [f64; 3];           // §12
}

pub mod save {   // ≙ WorldSaveManager, sans dépendance à l'ECS : nb-game fournit/consomme PlayerState
    pub struct WorldMetadata { /* §14.2 */ }
    pub fn sanitize(name: &str) -> String;
    pub fn world_dir(root: &Path, name: &str) -> PathBuf;
    pub fn list_worlds(root: &Path) -> Vec<WorldMetadata>;
    pub fn create_world_directory(root: &Path, meta: &mut WorldMetadata) -> nb_core::Result<PathBuf>;
    pub fn save_metadata(root: &Path, meta: &WorldMetadata) -> nb_core::Result<()>;
    pub fn load_metadata(root: &Path, dir_name: &str) -> nb_core::Result<WorldMetadata>;
    pub fn delete_world(root: &Path, name: &str);
    pub fn rename_world(root: &Path, meta: &mut WorldMetadata, new_name: &str);
    pub fn clone_world(root: &Path, src: &WorldMetadata, new_name: &str) -> nb_core::Result<WorldMetadata>;
}
```

Concurrence Rust recommandée : génération et lumière sur un pool (rayon ou N = max(1, cœurs/2) threads) ;
résultats publiés dans `active` ; lumière = lecture des 9 chunks sous `read()`, calcul hors verrou, écriture
sous `write()` puis `light_dirty = false` **seulement si aucun `set_block` n'a eu lieu entre-temps** (compteur de
génération par chunk) — correction assumée de la course §11.8, sans effet sur un monde statique.
Conserver : test de péremption `> r+1`, carré `(2r+1)²`, `hasNeighbors` 3×3, ordre « déchargements puis
chargements », marquages `mesh_dirty` identiques (§11.4–11.6, §10.5).

---

## 18. Écarts / bizarreries du Java à reproduire (sauf décision contraire)

Génération (à reproduire **impérativement** : elles changent les mondes) :

1. **Monde FLAT recouvert de lave** : terrain à y≈5–7, puis la passe lave remplit tout l'air de y 1..11 →
   nappe de lave de y=5 (ou 6/7) à 11 sur toute la carte, sauf les « océans » qui, rehaussés vers
   `oceanFloor ≥ 12`, forment des plateaux. Pas d'arbres ni de plantes sous la lave (recherche de surface
   arrêtée par la lave). Confirmé par les vecteurs §16.3. (À signaler au porteur produit ; reproduit par défaut.)
2. **Océans « rehaussés »** : la formule tire la hauteur vers `oceanFloor = 12 + floor(of*10)` (12..21), donc
   vers le haut si le terrain est plus bas que ça.
3. **AMPLIFIED sans côtes** : `oceanThreshold (0.50) > coastThreshold (0.48)` → le dégradé côtier n'existe plus,
   transition terre/mer brutale.
4. **ISLAND** réduit en fait la couverture océanique (seuils plus bas) tout en augmentant la fréquence.
5. **`|n|` inutile dans les grottes** (fBm ∈ [0,1]) ; grottes uniquement aux **y pairs** (et y+1) ; elles percent la
   surface et ignorent l'eau (eau suspendue possible) ; la caverne efface `y+1` sans test (eau comprise).
6. **Champignons souterrains** à y ∈ {3, 6, …, 48}, indépendants de `caveDepthMax`, mais désactivés avec les grottes ;
   ils apparaissent aussi en surface sur pierre/terre exposées.
7. **Végétation : seule la première entrée de la liste gagne** (même `rval` pour toutes) → fleurs des plaines,
   de la forêt et de la jungle, champignons de forêt/marais/taïga, herbe de taïga, fougère des montagnes,
   buisson mort de savane **ne sont jamais générés**. Le cactus (sans `break`) ne pousse que si `vegNoise ≤ 0.3`
   ou si le buisson mort a échoué ; sa hauteur `1 + hash % 3` utilise le même hash.
8. **Végétation posée sur STONE** (montagnes, pierre exposée) et sur le sable des plages quel que soit le biome.
9. **Arbres** : jamais sur la pierre ; DESERT exclu même avec un override ; pas d'arbre sous la canopée d'un arbre
   précédent ; troncs qui écrasent ; feuillages coupés aux bords du chunk ; trous d'air au centre des feuillages
   (OAK, BIRCH, JUNGLE, SWAMP_OAK) juste au-dessus du tronc ; les arbres ne sont tentés que pour `lx, lz ∈ [2,13]`.
10. **Minerais** tirés bloc par bloc (pas de filons), sur pierre **et grès**, y compris en surface.
11. **`treeDensityOverrides` de longueur < 8 dans un `world.json`** : en Java, `ArrayIndexOutOfBounds` dans la tâche
    de génération (avalée) → chunks du biome concerné jamais chargés. **Ne pas reproduire** : compléter à 8 avec -1.0
    (et tronquer au-delà) — écart assumé, à noter dans le code.

Monde / runtime :

12. `Chunk.SEA_LEVEL = 64` inutilisé (le vrai niveau est 34).
13. Tout chunk chargé est `dirty` → réécrit sur disque au déchargement, même intact.
14. Rayon visible effectif = `renderDistance - 2` (§11.7).
15. Lumière : halo de 2 blocs seulement ; voisins non recalculés après une édition de bord ; un seul canal ; la
    lumière du ciel n'est « pleine » (15) que tant qu'elle n'a rien traversé.
16. `Chunk.setLight` ne touche pas aux drapeaux ; `getLight` hors limites = 15.
17. Coordonnées monde → chunk via `float` (§11.6).
18. Spawn : pieds à `h + 1` (un bloc au-dessus du sol) ; respawn sans spawn défini → `(0.5, seaLevel+5, 0.5)`
    même au-dessus de l'océan ; sentinelle `playerY == 100.0` exacte (un joueur sauvegardé pile à y=100 se
    re-téléporte au spawn au rechargement — à reproduire seulement si on garde la sentinelle).
19. `walkSpeed`, `sprintSpeed`, `flySpeed`, `attackDamage` non persistés (reviennent aux défauts).
20. `cloneWorld` ne copie ni `allowCheats` ni l'état joueur ; `createWorldDirectory` renomme `meta.name` en version
    assainie **uniquement** en cas de collision.
21. Parseur JSON maison : pas de désechappement des chaînes, entiers tronqués à la partie entière, première
    occurrence de clé. (Concerne seulement l'import legacy.)
22. `InMemoryChunkStorage` aliasse le tableau sauvegardé (sans effet observable, ne pas reproduire).

Inexactitudes de la documentation Java (`docs/world.md`) à **ne pas** suivre : index `x + 16z + 256y` (réel :
`x + 16y + 2048z`), déchargement « au-delà de `mobDespawnRadius` » (réel : rayon de rendu), heat sources en
`List<BlockPos>` (réel : `Set<Integer>` inutilisé), sémantique des drapeaux (voir §8.3), « ISLAND = plus petites
terres » (voir point 4).

---

## Annexe A — Particules ambiantes de `World` (hors `nb-world`, pour le portage dans nb-game)

`generateRandomParticles(playerPos)` à chaque `update(pos, dt)` : `px,py,pz = (int)floor(player)` ;
200 itérations ; `rx = px + rnd.nextInt(48) - 24`, puis `ry`, puis `rz` (même formule, dans cet ordre) ;
`b = getBlock(rx, ry, rz)` ;
* si le nom de `b` contient `"leaves"` : si `getBlock(rx, ry-1, rz) == AIR && rnd.nextFloat() < 0.15` → particule
  LEAF en `(rx + nf, ry - 0.1, rz + nf)`, vitesse `((nf - 0.5)*0.5, -0.5, (nf - 0.5)*0.5)`, couleur
  `leavesTint(biome(rx, rz)) * 2.2` ;
* sinon si `b == LAVA` : si bloc au-dessus AIR `&& nf < 0.05` → LAVA en `(rx + nf, ry + 1.1, rz + nf)`, vitesse
  `((nf-0.5)*0.5, 1.5 + nf, (nf-0.5)*0.5)`, blanc ; puis si bloc en dessous AIR `&& nf < 0.02` → LAVA en
  `(rx + nf, ry - 0.1, rz + nf)`, vitesse `(0, -1, 0)`.
`rnd` = `java.util.Random` non semé ; tirages évalués de gauche à droite, court-circuit des `&&` respecté.
