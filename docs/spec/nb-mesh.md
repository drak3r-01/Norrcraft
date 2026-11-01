# Spécification comportementale — crate `nb-mesh`

> Statut : spec de réimplémentation « à l'identique » du maillage de chunks de Norrblock (Java/LWJGL → Rust/Vulkan).
> Sources Java analysées ligne à ligne (réf. lecture seule `Norrblock-Java-ref/repo/app/src/main/java/com/engine/core`) :
> `world/ChunkMesher`, `world/ChunkMeshManager`, `world/ChunkSnapshot`, `world/BlockType`, `world/BlockProperties`,
> `world/BlockRegistry`, `graphics/ChunkQuadtree`, `graphics/Camera`, `graphics/Mesh`, `graphics/HighlightMesh`,
> `graphics/TextureRegistry`, `world/generation/BiomeSystem` (teintes), ainsi que `resources/blocks.json`,
> `resources/shaders/main.vert|frag`, `shadow.vert|frag`, `hl.vert`, `graphics/MasterRenderer` (consommation du maillage)
> et `docs/rendering.md`.
>
> Objectif : pouvoir réécrire `nb-mesh` **sans relire le Java**. Tout ce qui est marqué **[EXACT]** doit être reproduit
> bit à bit (même ordre d'émission, même arithmétique flottante). Tout ce qui est marqué **[BIZARRERIE]** est un
> comportement surprenant du Java qu'il faut conserver (récapitulés en fin de document).

---

## 1. Périmètre du crate

`nb-mesh` est **pur CPU, sans dépendance GPU**. Il contient :

1. La génération du maillage d'un chunk (`ChunkMesher`) : lumière locale, greedy meshing, AO, teintes, blocs croisés,
   nénuphars, encodage du layer de texture.
2. Le format de sommet (ce que `Mesh` uploadait) — défini ici comme struct `#[repr(C)]` ; l'upload Vulkan est hors crate.
3. La logique d'ordonnancement des remaillages (`ChunkMeshManager`) sans l'upload GPU (callback / file de résultats).
4. Le culling : `ChunkQuadtree` + test frustum AABB (réplique de `JOML Matrix4f.testAab`, JOML 1.10.5).
5. La géométrie du contour de bloc ciblé (`HighlightMesh`).
6. Les formules de matrices caméra nécessaires au culling (`Camera`), en convention OpenGL.

Hors périmètre (référencé seulement) : chargement des textures (`TextureRegistry`), shaders, passes de rendu.

---

## 2. Constantes

| Constante | Valeur | Source |
|---|---|---|
| `CHUNK_SX` | 16 | `Chunk.SIZE_X` |
| `CHUNK_SY` | 128 | `Chunk.SIZE_Y` |
| `CHUNK_SZ` | 16 | `Chunk.SIZE_Z` |
| Index voxel dans un chunk | `x + y*16 + z*16*128` | `ChunkSnapshot.getBlock` |
| Lumière max | 15 | |
| Volume lumière | `[20][130][20]` indexé `[lx+2][y+1][lz+2]`, lx,lz ∈ [-2,17], y ∈ [-1,128] | |
| Itérations max de diffusion | 14 | |
| Plancher d'éclairage | `0.08f` | `getLightScale` |
| Facteur teinte herbe/feuilles | `2.2` (en **double**) | `emitQuad`, `emitCross`, `emitPad` |
| Abaissement surface fluide | `-0.15f` | `emitQuad` |
| Hauteur nénuphar | `y + 0.015f` | `emitPad` |
| Facteur directionnel croix | `0.85f` | `emitCross` |
| Offset layer feuilles | `+200000.0f` | |
| Offset layer croix | `+100000.0f` | |
| Encodage fluide | `-layer - 1.0f` | |
| Capacité feuille quadtree | 16 éléments | |
| Highlight | `s = 1.002f`, `o = -0.001f` | |
| Caméra (Main) | fov = `toRadians(70)`, aspect initial `1280/720`, near `0.05`, far `1000` | `Main.java:58` |
| Pitch caméra | clampé à [-89°, 89°] ; yaw non borné | `Camera.rotate` |

### 2.1 Indices de face [EXACT]

`OFFSETS = {0,0,1}, {0,0,-1}, {-1,0,0}, {1,0,0}, {0,1,0}, {0,-1,0}`

| f | Normale | `dirLight` | Slot texture (`textures[f]` de blocks.json) |
|---|---|---|---|
| 0 | +Z | `0.8f` | 0 |
| 1 | −Z | `0.8f` | 1 |
| 2 | −X | `0.6f` | 2 |
| 3 | +X | `0.6f` | 3 |
| 4 | +Y (dessus) | `1.0f` | 4 |
| 5 | −Y (dessous) | `0.5f` | 5 |

---

## 3. Données d'entrée

### 3.1 Voisinage 3×3 de snapshots

Le mesher reçoit `ChunkSnapshot[9]`, indice **`cz*3 + cx`** avec `cx, cz ∈ {0,1,2}` (1 = chunk central).
Le manager remplit `cache[(dz+1)*3 + (dx+1)]` pour `dx,dz ∈ [-1,1]` (donc `cx = dx+1`, `cz = dz+1`).

- Si `cache[4] == null` → retourne un maillage **vide** (0 float).
- Un snapshot = copie des voxels (`byte` id) **et** de la lightmap du chunk ; **la lightmap copiée n'est jamais utilisée
  par le mesher** (il recalcule sa propre lumière, §5).
- `snapshot.getBlock(x,y,z)` : hors bornes ou voxels null → `AIR` ; id inconnu → `AIR` (`BlockRegistry.fromId` défaut AIR).
  L'id est un `byte` Java signé : ids ≥ 128 deviennent négatifs → inconnus → AIR.

### 3.2 `getBlockCached(cache, lx, y, lz)` [EXACT]

Coordonnées locales au chunk central, lx/lz peuvent déborder de **±16 max** :

```
if y < 0 || y >= 128 → AIR
cx = 1, cz = 1
if lx < 0  { cx = 0; lx += 16 } else if lx >= 16 { cx = 2; lx -= 16 }
if lz < 0  { cz = 0; lz += 16 } else if lz >= 16 { cz = 2; lz -= 16 }
chunk = cache[cz*3 + cx]; return chunk != null ? chunk.getBlock(lx,y,lz) : AIR
```

Conséquence : **au-dessus de y=127 et sous y=0, tout est AIR** (faces du monde émises, cf. bizarreries).

### 3.3 Propriétés de bloc utilisées

Par bloc (blocks.json) : `transparent`, `crossShaped`, `grassTinted`, `leavesTinted`, `solid` (inutilisé par le mesher),
`lightAttenuation` (byte), `lightEmission` (byte), `textures[6]` → `textureIndices[6]`.

Le mesher compare aussi **par identité** à des blocs nommés : `AIR` (id 0), `GRASS` (2), `WATER` (4), `LAVA` (33),
`LILY_PAD` (26). Le contrat Rust doit exposer ces rôles explicitement (pas de déduction par propriétés).

Table de référence (blocks.json, pour tests) :

| id | nom | transp. | cross | grassT | leavesT | atten | emis |
|---|---|---|---|---|---|---|---|
| 0 | air | ✔ | | | | 0 | 0 |
| 1 dirt, 3 stone, 5 sand, 6 bedrock, 7 clay, 8 snow, 9/11/13/15/17 bois, 28–32 minerais, 34 sandstone | | | | | | 15 | 0 |
| 2 | grass | | | ✔ | | 15 | 0 |
| 4 | water | ✔ | | | | 2 | 0 |
| 10/12/14/16/18 | *_leaves | ✔ | | | ✔ | 2 | 0 |
| 19 tall_grass, 22 fern | | ✔ | ✔ | ✔ | | 0 | 0 |
| 20,21,23,24,25 (fleurs, dead_bush, champignons) | | ✔ | ✔ | | | 0 | 0 |
| 26 | lily_pad | ✔ | | | ✔ | 0 | 0 |
| 27 | cactus | ✔ | | | | 15 | 0 |
| 33 | lava | ✔ | | | | 3 | 15 |

Textures par face notables : grass = side×4, `grass_block_top`, `dirt` ; bûches = `*_log`×4, `*_log_top`×2 ;
cactus = `cactus_side`×4, `cactus_top`, `cactus_bottom` ; sandstone = `sandstone`×4, `sandstone_top`, `sandstone_bottom`.

### 3.4 Biomes et teintes

`biomeSystem.getBiome(double wx, double wz)` appelé avec les **coordonnées monde entières** `(startX+lx, startZ+lz)`
converties en double ; `getConfig(biome)` fournit `grassTint`, `leavesTint`, `waterColor` en **double[3]**.
(La fonction de biome elle-même relève de `nb-worldgen` ; `nb-mesh` la reçoit via un trait.)

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

L'**identité du biome** (enum) participe aux critères de fusion du greedy meshing (§6).

---

## 4. Format de sommet [EXACT]

Le maillage est un `float[]` plat, **9 floats par sommet**, **triangles non indexés**, 6 sommets par quad
(`vertexCount = len / 9`, dessin `GL_TRIANGLES`, aucune index buffer).

| Location | Attribut | Type | Offset (octets) |
|---|---|---|---|
| 0 | position monde `x, y, z` | vec3 f32 | 0 |
| 1 | UV `u, v` (en unités bloc, répétées) | vec2 f32 | 12 |
| 2 | layer encodé | f32 | 20 |
| 3 | couleur `r, g, b` (teinte × lumière × AO × dir) | vec3 f32 | 24 |

Stride : **36 octets**. Positions en **coordonnées monde absolues** (pas relatives au chunk) : `startX = pos.x*16`,
`startZ = pos.z*16`.

### 4.1 Encodage du layer (attribut 2) [EXACT]

`layer = (float) type.getTextureIndex(face)` (entier de la texture array), puis :

| Cas | Valeur écrite | Décodage shader (`main.vert`) |
|---|---|---|
| Fluide (WATER ou LAVA), via `emitQuad` | `-layer - 1.0f` (toujours < 0) | `aLayer < 0` → `-aLayer-1`, `IsFluid=1`, vagues |
| `leavesTinted` via `emitQuad` (toutes les feuilles) | `layer + 200000.0f` | `≥ 200000` → feuille, vent |
| Bloc croisé (`emitCross`) | `layer(face 0) + 100000.0f` | `≥ 100000` → herbe, balancement si `uv.y > 0.5` |
| Nénuphar (`emitPad`) | `layer(face 0)` brut | aucun effet |
| Autres | `layer` brut | |

Note : tous ces flottants sont exacts en f32 (< 2^24).

**[BIZARRERIE]** `shadow.vert` utilise les seuils **2000 / 1000** au lieu de 200000 / 100000 : en passe d'ombre, une
feuille (≥ 200000) et une croix (≥ 100000) tombent toutes deux dans la branche « ≥ 2000 » → layer = `aLayer - 2000`
(≈ 198000+ ou 98000+), que GL clampe au **dernier layer** = texture manquante (damier magenta, alpha 255) → feuilles et
plantes projettent des **ombres carrées pleines**, et les croix reçoivent le balancement « feuille ». Le crate ne fait
qu'encoder ; c'est au renderer de reproduire (ou non, décision produit) — à documenter côté `nb-render`.

---

## 5. Calcul de lumière local [EXACT]

Le mesher **recalcule** une lumière (ciel + émission) sur une région 20×130×20 couvrant le chunk + 2 blocs de bordure,
sans utiliser la lightmap stockée dans les chunks.

### 5.1 Prédicats

```
isLightBlocking(b) = !(b == AIR || b == WATER || b == LAVA || b.crossShaped || b == LILY_PAD || b.leavesTinted)
isSolidBlock(b)    = b != AIR && b != WATER && b != LAVA && !b.crossShaped && b != LILY_PAD     // pour l'AO
```

(`isSolidBlock` compte donc feuilles, cactus, et tout bloc transparent non listé comme occulteurs AO.)

### 5.2 Passe colonne (ciel descendant)

```
for lx in -2..=17:
  for lz in -2..=17:              // ordre sans importance ici (colonnes indépendantes)
    cur = 15
    for y in (128 down to -1):     // y = 128, 127, …, 0, -1
      atten = 1; emission = 0
      if 0 <= y < 128:
        b = getBlockCached(lx, y, lz)
        emission = b.lightEmission
        atten = isLightBlocking(b) ? 15 : max(1, b.lightAttenuation)
      else if y < 0:
        atten = 15
      A[lx+2][y+1][lz+2] = atten
      if y < 128:
        if atten == 1 && cur == 15: cur = 15
        else: cur = max(0, cur - atten)
        cur = max(cur, emission)
      L[lx+2][y+1][lz+2] = cur      // pour y=128 : 15
```

Valeurs effectives : air/croix/nénuphar → atten 1 ; eau, feuilles → 2 ; lave → 3 (émission 15) ; opaques et cactus → 15.

### 5.3 Passe de diffusion (Gauss-Seidel en place) — l'ordre compte

```
changed = true; maxIters = 14
while changed && maxIters-- > 0:          // au plus 14 balayages
  changed = false
  for x in 0..=19:
    for y in 0..=129:
      for z in 0..=19:
        a = A[x][y][z]
        if a >= 15: continue
        m = L[x][y][z]; prev = m
        if x > 0:   m = max(m, L[x-1][y][z] - a)
        if x < 19:  m = max(m, L[x+1][y][z] - a)
        if y > 0:   m = max(m, L[x][y-1][z] - a)
        if y < 129:
          up = L[x][y+1][z]
          if up == 15 && a == 1: m = max(m, 15)
          else:                  m = max(m, up - a)
        if z > 0:   m = max(m, L[x][y][z-1] - a)
        if z < 19:  m = max(m, L[x][y][z+1] - a)
        if m > prev: L[x][y][z] = m; changed = true
```

Points d'exactitude :
- On soustrait l'atténuation **de la cellule réceptrice** `a`, pas celle de la source.
- Les mises à jour sont **immédiatement visibles** dans le même balayage (pas de double buffer) ; l'ordre x→y→z
  croissant doit être conservé, sinon le résultat diffère quand la limite de 14 itérations est atteinte.
- Les cellules opaques (a = 15) gardent leur valeur colonne (0, ou l'émission).
- Ordre de vérification des voisins (−x, +x, −y, +y, −z, +z) sans effet sur le résultat (max), mais garder.

### 5.4 Lecture clampée

```
getL(x, y, z) = L[clamp(x+2, 0, 19)][clamp(y+1, 0, 129)][clamp(z+2, 0, 19)]
getLightScale(l) = max(0.08f, l / 15.0f)     // division int→float en f32
```

---

## 6. Algorithme de maillage — ordre d'émission global [EXACT]

```
generateChunkMeshData(pos, cache, biomes):
  if cache[4] == null: return []
  calculer lumière (§5)
  for y in 0..128:            // pour chaque tranche horizontale
    generateGreedySliceY(y, face = 4)   // dessus
    generateGreedySliceY(y, face = 5)   // dessous
  for f in [0, 1, 2, 3]:
    generateGreedySliceXZ(f)            // 16 tranches chacune
  generateSpecials()                    // croix + nénuphars
```

L'ordre des quads dans le buffer est donc : (y=0 : dessus, dessous), (y=1 : dessus, dessous), …, puis toutes les faces
+Z, puis −Z, −X, +X, puis les spéciaux. Une seule liste, **pas de buffers séparés opaque/fluide**.

### 6.1 Critère de face visible `shouldRenderFace(current, neighbor)` [EXACT]

```
if current == WATER || current == LAVA:
    return (neighbor.transparent || neighbor == LILY_PAD) && neighbor != current
if neighbor.transparent || neighbor.crossShaped || neighbor == LILY_PAD:
    if current == neighbor && !current.leavesTinted: return false
    return true
return false
```

Blocs ignorés comme `current` dans le greedy : `AIR`, tout `crossShaped`, `LILY_PAD`.

### 6.2 Tranche Y (faces 4 et 5) — `generateGreedySliceY(y, face)`

Masques `[16 (lx)][16 (lz)]` : `mask`, `type`, `biome`, `ao` (int empaqueté), `light` (int empaqueté).
`ny = y + OFFSETS[face].y`.

**Remplissage** (`lx` externe, `lz` interne) : pour chaque cellule non ignorée telle que
`shouldRenderFace(current, block(lx, ny, lz))` :
`mask=true, type=current, biome=getBiome(startX+lx, startZ+lz), ao=getFaceAO(lx,y,lz,face), light=getFaceLightSmooth(lx,y,lz,face)`.

**Fusion** (balayage `lx` externe 0..15, `lz` interne 0..15) pour chaque cellule `mask` encore vraie :

```
isFluid = current ∈ {WATER, LAVA}
isSurfaceTop = (face == 4 && isFluid)
width = 1                           // extension le long de +Z (lz)
if !isSurfaceTop:
  while lz+width < 16 && same(lx, lz+width): width++
height = 1                          // extension le long de +X (lx)
done = isSurfaceTop
while lx+height < 16 && !done:
  for w in 0..width: if !same(lx+height, lz+w): done = true; break
  if !done: height++
effacer mask sur [lx..lx+height) × [lz..lz+width)
emitQuad(x = startX+lx, y, z = startZ+lz, W = height, H = 1, D = width, face, current, biome, ao, light, isSurfaceTop)
```

`same(i,j)` = `mask[i][j] && type == current && biome == currentBiome && ao == currentAo && light == currentLight`
(égalités par identité / valeur entière exacte).

→ Les dessus de fluides ne sont **jamais fusionnés** (1×1) ; les dessous de fluides fusionnent normalement.

### 6.3 Tranches latérales (faces 0..3) — `generateGreedySliceXZ(face)`

```
isX  = face ∈ {2, 3}
maxU = 16                               // (isX ? SIZE_Z : SIZE_X)
for slice in 0..16:                     // borne écrite SIZE_X pour les deux orientations
  masques [128 (y)][16 (u)] : mask, type, biome, ao, light, surface
  pour y in 0..128, u in 0..16:
    lx = isX ? slice : u ; lz = isX ? u : slice
    current = block(lx, y, lz) ; ignorer AIR/croix/LILY_PAD
    if shouldRenderFace(current, block(lx+dirX, y, lz+dirZ)):
      mask=true, type, biome=getBiome(startX+lx, startZ+lz), ao, light
      if current ∈ {WATER, LAVA}: surface = (block(lx, y+1, lz) != current)
```

Fusion (balayage `y` externe, `u` interne) :

```
currentSurface = surface[y][u]
width = 1                                 // le long de u
if !currentSurface:
  while u+width < 16 && mask[y][u+width] && type/biome/ao/light égaux && surface[y][u+width] == currentSurface: width++
height = 1                                // le long de +Y
done = false
while y+height < 128 && !done:
  rowSurface = surface[y+height][u]
  if rowSurface != currentSurface: done = true; break
  for w in 0..width:
    if !mask[y+height][u+w] || type/biome/ao/light différents || surface[y+height][u+w] != rowSurface: done = true; break
  if !done: height++
effacer [y..y+height) × [u..u+width)
lx = isX ? slice : u ; lz = isX ? u : slice
sizeX = isX ? 1 : width ; sizeZ = isX ? width : 1
emitQuad(startX+lx, y, startZ+lz, W = sizeX, H = height, D = sizeZ, face, ..., isSurfaceTop = currentSurface)
```

Pour les faces 0/1 : `slice = lz`, `u = lx`. Pour 2/3 : `slice = lx`, `u = lz`.
Une cellule fluide « surface » produit en pratique un quad latéral de 1×1 (la rangée au-dessus ne peut pas être du même
fluide sans annuler `surface`).

### 6.4 Spéciaux — `generateSpecials()`

Balayage `lx` externe, `y` milieu, `lz` interne (ordre **différent** des tranches). Pour chaque bloc ≠ AIR :
`biome = getBiome(startX+lx, startZ+lz)`, `lightVal = L[lx+2][y+1][lz+2]` (lumière de la **propre cellule**, brute).
- `crossShaped` → `emitCross` ; sinon si `LILY_PAD` → `emitPad`.

---

## 7. `emitQuad` [EXACT]

Paramètres : origine monde entière `(x, y, z)`, étendues `W` (X), `H` (Y), `D` (Z), `face`, `type`, `biome`,
`aoVal`, `lightVal`, `isSurfaceTop`.

### 7.1 Couleur de base (teinte)

```
r = g = b = 1.0f
if type == GRASS && face == 4: rgb = (float)(grassTint[i] * 2.2)      // produit en double, puis cast f32
else if type.leavesTinted:      rgb = (float)(leavesTint[i] * 2.2)    // toutes faces
else if type == WATER:          rgb = (float) waterColor[i]           // sans ×2.2
// LAVA, côtés d'herbe, tout le reste : blanc
```

### 7.2 Étendues UV

```
faces 4,5 : uMax = W, vMax = D
faces 2,3 : uMax = D, vMax = H
faces 0,1 : uMax = W, vMax = H
```

### 7.3 Abaissement des fluides de surface

Uniquement si `isFluid && isSurfaceTop` ; `yOffsets[4]` (par coin, défaut 0) :

| face | coins abaissés de `-0.15f` |
|---|---|
| 0 | 1, 2 |
| 1 | 2, 3 |
| 2 | 2, 3 |
| 3 | 2, 3 |
| 4 | 0, 1, 2, 3 |
| 5 | aucun (n'arrive jamais : face 5 n'a jamais `isSurfaceTop`) |

(Ce sont exactement les coins hauts de chaque face latérale.) `y_final = (float)(y_coin) + yOffsets[i]` en f32.

### 7.4 Coins, UV et voisins AO/lumière par face

Notation : `n = cellule + normale` ; pour chaque coin, `s1`, `s2`, `c` sont des décalages relatifs à `n`
(côté 1, côté 2, coin diagonal). Les positions sont calculées en **int** puis converties en float.

| face | coin | position | UV | s1 | s2 | c |
|---|---|---|---|---|---|---|
| 0 (+Z) | 0 | (x+W, y,   z+D) | (U,0) | +x | −y | +x−y |
| | 1 | (x+W, y+H, z+D) | (U,V) | +x | +y | +x+y |
| | 2 | (x,   y+H, z+D) | (0,V) | −x | +y | −x+y |
| | 3 | (x,   y,   z+D) | (0,0) | −x | −y | −x−y |
| 1 (−Z) | 0 | (x+W, y,   z) | (0,0) | +x | −y | +x−y |
| | 1 | (x,   y,   z) | (U,0) | −x | −y | −x−y |
| | 2 | (x,   y+H, z) | (U,V) | −x | +y | −x+y |
| | 3 | (x+W, y+H, z) | (0,V) | +x | +y | +x+y |
| 2 (−X) | 0 | (x, y,   z) | (0,0) | −z | −y | −z−y |
| | 1 | (x, y,   z+D) | (U,0) | +z | −y | +z−y |
| | 2 | (x, y+H, z+D) | (U,V) | +z | +y | +z+y |
| | 3 | (x, y+H, z) | (0,V) | −z | +y | −z+y |
| 3 (+X) | 0 | (x+W, y,   z+D) | (0,0) | +z | −y | +z−y |
| | 1 | (x+W, y,   z) | (U,0) | −z | −y | −z−y |
| | 2 | (x+W, y+H, z) | (U,V) | −z | +y | −z+y |
| | 3 | (x+W, y+H, z+D) | (0,V) | +z | +y | +z+y |
| 4 (+Y) | 0 | (x,   y+H, z+D) | (0,0) | −x | +z | −x+z |
| | 1 | (x+W, y+H, z+D) | (U,0) | +x | +z | +x+z |
| | 2 | (x+W, y+H, z) | (U,V) | +x | −z | +x−z |
| | 3 | (x,   y+H, z) | (0,V) | −x | −z | −x−z |
| 5 (−Y) | 0 | (x,   y, z) | (0,0) | −x | −z | −x−z |
| | 1 | (x+W, y, z) | (U,0) | +x | −z | +x−z |
| | 2 | (x+W, y, z+D) | (U,V) | +x | +z | +x+z |
| | 3 | (x,   y, z+D) | (0,V) | −x | +z | −x+z |

`U = uMax`, `V = vMax`. Rappel : en tranche Y, `W` = nombre de cellules le long de X, `D` le long de Z, `H = 1` ;
pour la face 3, `x+W = x+1` ; pour la face 0, `z+D = z+1`.

Conséquences vérifiées : toutes les faces sont **CCW vues de l'extérieur** (front face GL par défaut, `glCullFace(GL_BACK)`
activé). Sur les faces latérales, `u` croît vers la droite de l'observateur et `v` vers +Y ; sur le dessus, `v` croît
vers −Z ; sur le dessous, vers +Z.

### 7.5 Triangulation et écriture

Indices `{0, 1, 2, 2, 3, 0}` → triangles (v0,v1,v2) puis (v2,v3,v0). Pour chaque indice `i` :

```
push pos.x, pos.y + yOffsets[i], pos.z
push u, v
push encodedLayer
push r * dirLight * aoF[i] * lightF[i]     // évaluation f32 gauche→droite : ((r*dir)*ao)*light
push g * dirLight * aoF[i] * lightF[i]
push b * dirLight * aoF[i] * lightF[i]
```

`aoF[i] = getAOScale(byte i de aoVal)`, `lightF[i] = getLightScale(byte i de lightVal)` avec
byte 0 = bits 24..31, byte 1 = 16..23, byte 2 = 8..15, byte 3 = 0..7.

**Piège numérique** : ne pas laisser le compilateur fusionner en FMA ni réassocier (Rust ne le fait pas par défaut ; ne
pas utiliser `mul_add`). Le produit `(r*dirLight)` doit être calculé en f32 à partir du `r` déjà casté en f32.

---

## 8. Occlusion ambiante (AO) [EXACT]

```
getFaceAO(x, y, z, face):         // x,z locaux (peuvent déborder), voir table §7.4
  n = (x,y,z) + OFFSETS[face]
  a_i = calcAO(isSolid(n+s1_i), isSolid(n+s2_i), isSolid(n+c_i))  pour i = 0..3
  return (a0 << 24) | (a1 << 16) | (a2 << 8) | a3

calcAO(side1, side2, corner):
  if side1 && side2: return 0
  return 3 - (side1 + side2 + corner)

getAOScale(v): 3 → 1.0f ; 2 → 0.75f ; 1 → 0.5f ; autre (0) → 0.3f
```

- `isSolid` = `isSolidBlock(getBlockCached(...))` (§5.1). Hors Y → AIR → non solide.
- L'AO est calculée **pour la cellule d'origine** et appliquée aux coins du quad fusionné ; la fusion exige l'égalité de
  la valeur empaquetée, donc cohérent.
- L'AO s'applique aussi aux fluides.
- Aucun « flip » de diagonale selon l'AO (la triangulation est toujours 0-1-2 / 2-3-0).

## 9. Lumière lissée par coin [EXACT]

```
getFaceLightSmooth(x, y, z, face):
  n = (x,y,z) + OFFSETS[face]
  l_i = calcL(n, n+s1_i, n+s2_i, n+c_i)        // mêmes s1/s2/c que l'AO (table §7.4)
  return (l0 << 24) | (l1 << 16) | (l2 << 8) | l3

calcL(C, S1, S2, K):
  lC = getL(C), lS1 = getL(S1), lS2 = getL(S2), lK = getL(K)
  if lS1 == 0 && lS2 == 0: return lC
  return (lC + lS1 + lS2 + lK) / 4             // division entière (troncature)
```

Facteur final : `getLightScale(l) = max(0.08f, l/15.0f)`.

Note : on ne vérifie pas l'opacité du coin `K` (contrairement à des implémentations type Minecraft) ; seule la règle
« deux côtés à 0 » protège contre les fuites.

---

## 10. Blocs croisés — `emitCross` [EXACT]

```
rgb = 1.0f ; if type.grassTinted: rgb = (float)(grassTint * 2.2)
encodedLayer = getTextureIndex(0) + 100000.0f
vLight = max(0.08f, lightVal / 15.0f)          // lumière brute de la cellule (§6.4), pas de lissage
dirLight = 0.85f
couleur = r * vLight * dirLight  (ordre f32 : (r*vLight)*0.85f)
pas d'AO
```

4 faces × 6 sommets (déjà triangulés, pas d'indices), UV par sommet `{(0,0),(1,0),(1,1),(1,1),(0,1),(0,0)}` :

| face | s0 | s1 | s2 | s3 | s4 | s5 |
|---|---|---|---|---|---|---|
| A | (x,y,z) | (x+1,y,z+1) | (x+1,y+1,z+1) | (x+1,y+1,z+1) | (x,y+1,z) | (x,y,z) |
| B | (x+1,y,z+1) | (x,y,z) | (x,y+1,z) | (x,y+1,z) | (x+1,y+1,z+1) | (x+1,y,z+1) |
| C | (x+1,y,z) | (x,y,z+1) | (x,y+1,z+1) | (x,y+1,z+1) | (x+1,y+1,z) | (x+1,y,z) |
| D | (x,y,z+1) | (x+1,y,z) | (x+1,y+1,z) | (x+1,y+1,z) | (x,y+1,z+1) | (x,y,z+1) |

A/B : diagonale (0,0)→(1,1) recto/verso ; C/D : diagonale (1,0)→(0,1) recto/verso. Diagonales pleine longueur, sans
retrait. Le culling des faces arrière reste actif : chaque plan est visible des deux côtés grâce aux paires recto/verso.
Le shader balance les sommets `uv.y > 0.5` (le haut).

## 11. Nénuphar — `emitPad` [EXACT]

```
rgb = (float)(grassTint * 2.2)      // teinte HERBE, bien que lily_pad soit leavesTinted
layer = getTextureIndex(0)          // brut, aucun offset
py = y + 0.015f
vLight = max(0.08f, lightVal / 15.0f) ; couleur = r * vLight (pas de dirLight, pas d'AO)
```

Deux quads, indices `{0,1,2,2,3,0}`, UV coins `{(0,0),(1,0),(1,1),(0,1)}` :
- dessus : `(x,py,z+1), (x+1,py,z+1), (x+1,py,z), (x,py,z)` (CCW vu d'en haut)
- dessous : `(x,py,z), (x+1,py,z), (x+1,py,z+1), (x,py,z+1)` (CCW vu d'en bas)

---

## 12. Transparence et passes (consommation par le renderer)

Le crate produit **un seul buffer** par chunk. Le Java le dessine **deux fois** :

1. **Passe 0 (opaque/cutout)** : `uRenderPass=0`, blend off, depth write on, cull back. Le fragment shader **jette les
   fluides** (`IsFluid > 0.5`) et fait un alpha test « sharpen » (`(a-0.5)/fwidth(a)+0.5 < 0.5` → discard).
   Feuilles, croix, nénuphars, cactus sont ici.
2. Requêtes d'occlusion (AABB 16×128×16 par chunk visible, résultat utilisé à la frame suivante, défaut visible).
3. **Passe 1 (fluides)** : `uRenderPass=1`, blend `SRC_ALPHA, ONE_MINUS_SRC_ALPHA`, **depth write off** (laissé à
   false par l'étape 2), cull back **actif**, **aucun tri** des chunks ni des quads. Le shader jette tout ce qui n'est
   pas fluide.

Le discriminant entre passes est **uniquement le signe du layer**. Si le renderer Rust préfère deux buffers, la
séparation doit être un post-traitement (filtrage par `layer < 0`) qui préserve l'ordre relatif — le contrat ci-dessous
offre `MeshOutput::split_fluid()` pour cela.

Le vertex shader ajoute des vagues aux sommets fluides tels que `fract(y) > 0.7` (donc les coins abaissés à `.85`).

## 13. UV et texture array (rappel `TextureRegistry`)

- Pas d'atlas : **une texture = un layer** d'une `TEXTURE_2D_ARRAY`. Les UV sont en unités bloc (jusqu'à 16 ou 128) et
  reposent sur le wrap **REPEAT** (S et T). Filtrage : mag NEAREST, min NEAREST_MIPMAP_LINEAR, mipmaps générés,
  anisotropie `min(8, max)`.
- Images chargées **retournées verticalement** (`stbi_set_flip_vertically_on_load(true)`) : `v = 0` = bas de l'image.
  En Vulkan, uploader aussi les rangées inversées pour garder l'orientation.
- Taille des layers : `W = max(largeurs)`, carré `W×W`. Si `imgW == W && imgH >= W` : on prend les `W` premières
  rangées du buffer **retourné** (= les `W` rangées du **bas** de l'image originale, c.-à-d. la dernière frame d'une
  bande animée comme `water_still`). Sinon : redimensionnement `stbir_resize_uint8` (filtre par défaut, non nearest)
  des `min(imgH, imgW)` premières rangées vers `W×W`.
- Ordre des layers = ordre de `File.listFiles` (non trié, dépend de l'OS) ; nom = nom de fichier sans `".png"` puis
  minuscules. Layer « manquant » = `nombreDeFichiers` (dernier), damier magenta (tuile `max(1,W/4)`).
  Aucun fichier → texture 1×1 magenta, layer manquant 0.
- `BlockType.initializeMapping` : nom vide ou absent → **layer 0** (pas le layer manquant) ; nom inconnu → layer
  manquant ; bloc id 0 (air) jamais mappé (tout à 0).
- Les indices sont **figés dans le maillage** au moment du maillage : un changement de pack impose un remaillage complet
  (§14, compteur de génération).

---

## 14. Ordonnancement — `ChunkMeshManager` (logique sans GPU)

État : pool de `max(1, nbCœurs/2)` threads ; `meshingTasks: Set<ChunkPos>` (concurrent) ; file `MeshResult(pos, data,
generation)` ; `meshGeneration: AtomicLong` ; `chunkMeshes: Map<ChunkPos, Mesh>` (thread principal seulement).

`update(world, activePos)` (appelé chaque frame, thread principal) :

1. Retirer et libérer chaque mesh dont la position n'est **pas** dans `activePos`.
2. Pour chaque `pos` de `activePos` (dans l'ordre de la liste) : si chunk chargé **et** `meshDirty` **et** pas déjà en
   cours :
   - construire les 9 snapshots (`dz` externe −1..1, `dx` interne) ; si un voisin est absent **ou `lightDirty`** →
     abandon pour cette frame (le chunk reste dirty) ;
   - `meshDirty = false`, marquer en cours, capturer `gen = meshGeneration`, soumettre la tâche
     (`generateChunkMeshData` → push résultat ; `finally` retire `pos` des tâches en cours).
3. Vider la file de résultats :
   - `generation != courante` → jeter, remettre le chunk `meshDirty = true` (s'il existe) ;
   - sinon si `pos ∈ activePos` : mesh existant → `updateData` **seulement si data non vide** ; pas de mesh → créer
     **seulement si data non vide** ;
   - sinon (plus actif) → remettre `meshDirty = true` si le chunk existe.

`invalidateAllMeshes(world)` : `generation++`, libérer et vider tous les meshes, **vider `meshingTasks`**, marquer
tous les chunks actifs `meshDirty`.

`shutdown()` : arrêt immédiat du pool.

## 15. Culling — `ChunkQuadtree` + frustum

### 15.1 Construction (chaque frame, depuis la map des meshes)

```
build(chunks):
  poolIndex = 0 ; if vide: root = None ; return
  bornes = union des AABB XZ [pos.x*16, pos.x*16+16] × [pos.z*16, pos.z*16+16]  (en f32)
  root = node(minX, minZ, maxX, maxZ)
  pour chaque entrée (ordre d'itération de la map) : insert(root, entrée)

insert(node, e):
  if node est feuille && node.elements.len < 16: push ; return
  if node est feuille: split(node)
  insert(node.children[quadrant(node, centre(e))], e)

centre(e) = (pos.x*16 + 8.0, pos.z*16 + 8.0)
split(node): mid = ((min+max)/2) en f32 ; enfants :
  0 = (minX, minZ, midX, midZ) ; 1 = (midX, minZ, maxX, midZ)
  2 = (minX, midZ, midX, maxZ) ; 3 = (midX, midZ, maxX, maxZ)
  redistribuer les éléments existants directement dans les listes des enfants (sans récursion ni limite), vider le nœud
quadrant(node, x, z): mid recalculé ; x < midX ? (z < midZ ? 0 : 2) : (z < midZ ? 1 : 3)
```

Un enfant peut temporairement contenir > 16 éléments après un split ; il ne se divise qu'à la prochaine insertion qui
l'atteint. Les nœuds sont recyclés via un pool (optimisation, sans effet observable).

### 15.2 Requête

```
queryNode(node, M):
  if !testAab(M, node.minX, 0, node.minZ, node.maxX, 128, node.maxZ): return
  if node a des enfants: for i in 0..4: queryNode(child[i], M)
  else: for e in node.elements (ordre d'insertion):
          cx = pos.x*16, cz = pos.z*16
          if testAab(M, cx, 0, cz, cx+16, 128, cz+16): result.push(e)
```

Utilisée avec `M = projection × view` (caméra) et avec `M = lightSpaceMatrix` (ortho du soleil) pour la passe d'ombre.
L'ordre du résultat (parcours en profondeur 0,1,2,3) **est l'ordre de dessin** — y compris en passe fluide non triée.

### 15.3 `testAab` (réplique JOML 1.10.5, convention OpenGL) [EXACT]

Avec `mCR` = colonne C, ligne R (stockage column-major) :

```
nx = (m03+m00, m13+m10, m23+m20, m33+m30)   // gauche
px = (m03-m00, m13-m10, m23-m20, m33-m30)   // droite
ny = (m03+m01, m13+m11, m23+m21, m33+m31)   // bas
py = (m03-m01, m13-m11, m23-m21, m33-m31)   // haut
nz = (m03+m02, m13+m12, m23+m22, m33+m32)   // proche  (z_clip >= -w, convention GL)
pz = (m03-m02, m13-m12, m23-m22, m33-m32)   // lointain
pour chaque plan P=(a,b,c,d) dans l'ordre nx,px,ny,py,nz,pz (court-circuit) :
  a*(a<0 ? minX : maxX) + b*(b<0 ? minY : maxY) + c*(c<0 ? minZ : maxZ) >= -d   sinon → false
→ true
```

Test « p-vertex » conservateur (vrai si intersecte ou contient). Plans non normalisés. Calcul f32 sans FMA.

**Piège Vulkan** : si la projection de rendu Rust est en profondeur [0,1] et/ou Y inversé, le culling doit être fait
sur la matrice **convention GL** (sinon le plan proche devient `row2 >= 0` et les résultats diffèrent aux bords). Le
crate fournit les matrices GL ; le renderer applique ensuite sa matrice de correction clip.

### 15.4 Matrices caméra (convention GL, JOML) [EXACT]

- `view = Rx(rad(pitch)) · Ry(rad(yaw)) · T(-pos)` (post-multiplication JOML : `identity().rotateX().rotateY().translate()`),
  rotations main droite standard (`Rx` : ligne1 = (0, c, −s), ligne2 = (0, s, c) ; `Ry` : ligne0 = (c, 0, s),
  ligne2 = (−s, 0, c)).
- `proj = perspective(fovY, aspect, near, far)` GL : `h = tan(fovY/2)`, `m00 = 1/(h·aspect)`, `m11 = 1/h`,
  `m22 = (far+near)/(near−far)`, `m32 = 2·far·near/(near−far)` (colonne 3, ligne 2), `m23 = −1`, `m33 = 0`.
- Frustum caméra = `proj · view` (`new Matrix4f(proj).mul(view)`).
- Position initiale `(0, 100, 0)`, rotation nulle.
- Ombre (pour info) : `lightProj = ortho(-120,120,-120,120, 1, 400)` GL ; `lightView = lookAt(cam + dir·(−150), cam, +Y)`.

## 16. Contour de bloc — `HighlightMesh` [EXACT]

Positions seules (vec3 f32, stride 12), **24 sommets = 12 segments** `GL_LINES`, `o = -0.001f`,
`S = s + o = 1.002f + (-0.001f)` calculé en f32 (≈ 1.001) :

```
bas (y=o)   : (o,o,o)-(S,o,o)  (S,o,o)-(S,o,S)  (S,o,S)-(o,o,S)  (o,o,S)-(o,o,o)
haut (y=S)  : (o,S,o)-(S,S,o)  (S,S,o)-(S,S,S)  (S,S,S)-(o,S,S)  (o,S,S)-(o,S,o)
verticales  : (o,o,o)-(o,S,o)  (S,o,o)-(S,S,o)  (S,o,S)-(S,S,S)  (o,o,S)-(o,S,S)
```

Rendu : `model = translate(blocCiblé)` (coordonnées entières du bloc en f32), depth test `LEQUAL`.

---

## 17. Contrat Rust proposé

Dépendances autorisées : `bytemuck` (Pod), éventuellement `glam` pour les matrices (sinon `[f32; 16]` column-major).
Aucune dépendance GPU, aucun état global. `Send + Sync` partout pour le maillage parallèle (rayon ou pool dédié).

```rust
// ---------- constantes ----------
pub const CHUNK_SX: usize = 16;
pub const CHUNK_SY: usize = 128;
pub const CHUNK_SZ: usize = 16;
pub const LAYER_CROSS_OFFSET: f32 = 100_000.0;
pub const LAYER_LEAVES_OFFSET: f32 = 200_000.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ChunkPos { pub x: i32, pub z: i32 }

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face { PosZ = 0, NegZ = 1, NegX = 2, PosX = 3, PosY = 4, NegY = 5 }

// ---------- sommet ----------
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ChunkVertex {
    pub pos: [f32; 3],   // location 0, offset 0
    pub uv: [f32; 2],    // location 1, offset 12
    pub layer: f32,      // location 2, offset 20 (encodé, cf. LayerCode)
    pub color: [f32; 3], // location 3, offset 24
}                        // size_of == 36

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LayerCode { Plain(u32), Fluid(u32), Leaves(u32), Cross(u32) }
impl LayerCode {
    pub fn encode(self) -> f32;           // -l-1 / l+200000 / l+100000 / l
    pub fn decode(v: f32) -> LayerCode;   // ordre des tests identique à main.vert : <0, >=200000, >=100000
}

// ---------- blocs ----------
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockRole { Air, Grass, Water, Lava, LilyPad, Other }

#[derive(Clone, Debug)]
pub struct BlockMeshInfo {
    pub role: BlockRole,
    pub transparent: bool,
    pub cross_shaped: bool,
    pub grass_tinted: bool,
    pub leaves_tinted: bool,
    pub light_attenuation: i32,   // byte Java
    pub light_emission: i32,
    pub texture_layers: [u32; 6], // résolus (nom vide → 0, inconnu → layer manquant)
}

/// Table indexée par id (u8). Ids absents → traités comme l'entrée AIR.
pub struct BlockTable { /* Vec<BlockMeshInfo> + id de l'air */ }
impl BlockTable {
    pub fn new(entries: Vec<(u8, BlockMeshInfo)>) -> Self;
    pub fn get(&self, id: u8) -> &BlockMeshInfo; // id >= 128 ou inconnu → AIR (cf. byte signé Java)
}

// ---------- voisinage ----------
/// Voxels d'un chunk : index x + y*16 + z*16*128.
pub struct ChunkVoxels(pub Box<[u8; CHUNK_SX * CHUNK_SY * CHUNK_SZ]>);

/// 9 chunks, index cz*3 + cx (cx,cz ∈ 0..3, centre = 4).
pub struct Neighborhood<'a> { pub chunks: [Option<&'a ChunkVoxels>; 9] }

// ---------- biomes ----------
pub type BiomeId = u8; // identité comparée par égalité pour la fusion
#[derive(Clone, Copy, Debug)]
pub struct BiomeTints { pub grass: [f64; 3], pub leaves: [f64; 3], pub water: [f64; 3] } // en f64 !
pub trait BiomeSource: Sync {
    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId;   // appelé avec coords monde entières
    fn tints(&self, biome: BiomeId) -> BiomeTints;
}

// ---------- lumière (exposée pour tests) ----------
pub struct LightVolume { /* L: [20][130][20] i32, A: [20][130][20] u8 */ }
impl LightVolume {
    pub fn compute(n: &Neighborhood, blocks: &BlockTable) -> Self; // §5, ordre exact
    pub fn get_clamped(&self, lx: i32, y: i32, lz: i32) -> i32;    // getL
}

// ---------- maillage ----------
pub struct MeshOutput { pub vertices: Vec<ChunkVertex> } // ordre §6, 6 sommets/quad
impl MeshOutput {
    pub fn is_empty(&self) -> bool;
    pub fn as_floats(&self) -> &[f32];                                  // vue plate (9 f32/sommet)
    pub fn split_fluid(&self) -> (Vec<ChunkVertex>, Vec<ChunkVertex>);   // (layer>=0, layer<0), ordre préservé
}
pub fn mesh_chunk(pos: ChunkPos, n: &Neighborhood, blocks: &BlockTable, biomes: &dyn BiomeSource) -> MeshOutput;

// Fonctions internes rendues publiques sous #[doc(hidden)] pour tests unitaires :
pub fn should_render_face(cur: &BlockMeshInfo, nb: &BlockMeshInfo, same_block: bool) -> bool;
pub fn calc_ao(side1: bool, side2: bool, corner: bool) -> u8;
pub fn ao_scale(v: u8) -> f32;     // 1.0 / 0.75 / 0.5 / 0.3
pub fn light_scale(l: i32) -> f32; // max(0.08, l/15)

// ---------- ordonnancement (sans GPU) ----------
pub struct MeshResult { pub pos: ChunkPos, pub mesh: MeshOutput, pub generation: u64 }
pub enum MeshAction { Upload(ChunkPos, MeshOutput), Remove(ChunkPos), MarkDirty(ChunkPos) }
pub trait ChunkSource {
    fn is_loaded(&self, p: ChunkPos) -> bool;
    fn mesh_dirty(&self, p: ChunkPos) -> bool;
    fn light_dirty(&self, p: ChunkPos) -> bool;
    fn set_mesh_dirty(&mut self, p: ChunkPos, v: bool);
    fn snapshot(&self, p: ChunkPos) -> Option<std::sync::Arc<ChunkVoxels>>;
}
pub struct MeshScheduler { /* generation: AtomicU64, in_flight: Mutex<HashSet<ChunkPos>>, results: channel, has_mesh: HashSet<ChunkPos> */ }
impl MeshScheduler {
    pub fn new(threads: usize) -> Self;                     // défaut max(1, cpus/2)
    pub fn update(&mut self, world: &mut dyn ChunkSource, active: &[ChunkPos],
                  blocks: std::sync::Arc<BlockTable>, biomes: std::sync::Arc<dyn BiomeSource + Send>)
                  -> Vec<MeshAction>;                      // §14, règles « data vide » incluses
    pub fn invalidate_all(&mut self, world: &mut dyn ChunkSource, loaded: &[ChunkPos]) -> Vec<MeshAction>;
    pub fn shutdown(self);
}

// ---------- culling ----------
#[derive(Clone, Copy)]
pub struct Frustum { planes: [[f32; 4]; 6] } // nx,px,ny,py,nz,pz
impl Frustum {
    pub fn from_gl_matrix(m: &[f32; 16]) -> Self;            // column-major, convention GL
    pub fn test_aab(&self, min: [f32; 3], max: [f32; 3]) -> bool;
}
pub struct ChunkQuadtree { /* pool de nœuds */ }
impl ChunkQuadtree {
    pub fn new() -> Self;
    pub fn build(&mut self, chunks: impl IntoIterator<Item = ChunkPos>); // l'ordre d'itération détermine la structure
    pub fn query(&self, f: &Frustum, out: &mut Vec<ChunkPos>);
}

// ---------- caméra (math pure, GL) ----------
pub fn view_matrix(pos: [f32; 3], pitch_deg: f32, yaw_deg: f32) -> [f32; 16];
pub fn perspective_gl(fov_y_rad: f32, aspect: f32, near: f32, far: f32) -> [f32; 16];
pub fn mul_mat4(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16];
pub fn clamp_pitch(p: f32) -> f32; // [-89, 89]

// ---------- highlight ----------
pub fn highlight_lines() -> [[f32; 3]; 24];
```

Recommandations d'implémentation :
- Tests dorés : générer avec le Java des dumps `float[]` de quelques chunks (plat, forêt, eau+nénuphars, lave en grotte,
  bordure de biome) et comparer **bit à bit** (`f32::to_bits`).
- Pour `build`, passer un ordre **déterministe** (ex. trié par `(x, z)`) : le Java dépend de l'ordre d'itération d'un
  `HashMap`, non reproductible ; seul l'ordre de dessin (et donc le mélange des fluides entre chunks) en dépend.
- Les masques de tranche peuvent être réutilisés (pas d'allocation par tranche) sans changer le résultat.

---

## 18. Bizarreries du Java à reproduire

1. **Le biome est un critère de fusion pour tous les blocs**, même ceux non teintés (pierre, sable…) : les quads se
   coupent aux frontières de biome.
2. **Faces internes des feuilles** : deux feuilles identiques adjacentes émettent les deux faces (exception
   `leavesTinted` dans `shouldRenderFace`). Feuilles de types différents aussi.
3. **Faces du bord du monde** : `y ≥ 128` et `y < 0` valent AIR → la face du dessous des blocs en y=0 (bedrock) et du
   dessus en y=127 sont toujours émises.
4. **Lumière recalculée localement** : la lightmap des chunks est ignorée ; seules les sources à ≤ 2 blocs hors du
   chunk comptent ; diffusion limitée à 14 balayages Gauss-Seidel en place (ordre x→y→z).
5. **La lave agit comme le ciel vers le bas** : sous une lave (lumière 15), l'air (atten 1) reste à 15 sans
   décroissance (règle `atten==1 && cur==15`), en colonne et via la règle « up » de la diffusion.
6. **Lissage de lumière** : si les deux côtés valent 0 → valeur centrale seule ; sinon moyenne entière tronquée des 4
   (le coin est inclus même s'il est opaque).
7. **Dessus de fluide jamais fusionné** (1×1), abaissé de 0.15 sur ses 4 coins ; les côtés de la rangée de surface
   ont leurs 2 coins hauts abaissés et une largeur de 1 ; le dessous des fluides n'est jamais abaissé et fusionne.
8. **Surface latérale** déterminée par `bloc au-dessus != même fluide` (nénuphar, air, lave au-dessus d'eau… → surface).
9. **Eau contre lave** : chaque fluide émet sa face vers l'autre (l'autre est `transparent` et différent) → faces
   coplanaires (z-fighting). Un fluide émet aussi des faces vers les croix, feuilles, cactus.
10. **Faces vers un nénuphar** : un bloc ou fluide sous un nénuphar émet son dessus.
11. **Nénuphar** : teinte **herbe** (pas feuilles) malgré `leavesTinted=true`, layer brut (pas d'animation), pas de
    `dirLight`, pas d'AO, lumière brute de sa cellule, double face à `y+0.015`.
12. **Herbe** : seule la face du dessus de `GRASS` est teintée ; les côtés `grass_block_side` restent blancs. Les
    croix `grassTinted` (tall_grass, fern) prennent `grassTint×2.2` ; les autres croix restent blanches.
13. **Eau** teintée par `waterColor` sans le facteur 2.2 ; la **lave est blanche**.
14. **AO** : les feuilles, cactus et autres blocs transparents non exclus occultent ; l'AO s'applique aussi aux
    fluides ; aucun flip de diagonale.
15. **Croix** : facteur fixe 0.85, sans AO, lumière non lissée, diagonales pleine taille.
16. **Cactus** : `transparent=true` mais cube plein, bloque la lumière (atten 15) ; les faces adjacentes au cactus
    sont émises, cactus contre cactus non.
17. **Nom de texture vide → layer 0** (premier fichier du dossier), pas la texture manquante.
18. **Résultat vide ignoré** : si un remaillage renvoie 0 sommet, l'ancien mesh **reste affiché** (pas de
    suppression).
19. **`invalidateAllMeshes` vide `meshingTasks`** alors que des tâches tournent : le `finally` d'une ancienne tâche
    peut retirer l'entrée d'une nouvelle → remaillages concurrents en double possibles (inoffensif, résultat périmé
    jeté par génération).
20. **Ombres** : seuils 2000/1000 dans `shadow.vert` (cf. §4.1) → feuilles et croix échantillonnent le dernier layer
    (damier opaque) et ombrent en carrés pleins ; les croix reçoivent le balancement des feuilles en passe d'ombre.
21. **Branche « lave » morte dans `main.vert`** : le test `aColor.r > 0.6 && aColor.b < 0.2` ne peut jamais être vrai
    (lave blanche, eau bleutée) → la lave reçoit l'amplitude de vagues de l'eau.
22. **Passe fluide** : même buffer, sélection par signe du layer, pas de tri, depth write désactivé, culling arrière
    actif → la surface de l'eau est **invisible vue de dessous** (sa face est CCW vue d'en haut).
23. **Ordre de dessin** des chunks = ordre de sortie du quadtree, lui-même fonction de l'ordre d'itération d'un
    `HashMap<ChunkPos>` (non déterministe) — recommandation : ordre trié, différence non perceptible hors mélange
    de fluides.
24. **Textures** : bandes animées → c'est la frame du **bas** de l'image (dernière) qui est utilisée ; textures plus
    petites que la plus large sont agrandies par filtre lissant stb (non nearest).
25. **Ordre d'émission** : tranches Y entrelacées (dessus puis dessous pour chaque y), puis faces +Z, −Z, −X, +X, puis
    spéciaux en ordre `lx, y, lz` — à respecter pour l'égalité binaire des buffers.
