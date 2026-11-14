# 07. Eau vivante

Conception complète de l'eau dynamique de Norrcraft : simulation à niveaux qui conserve le
volume, sources, blocs immergés et plantes aquatiques, courants, cascades générées, rendu et son.
Ce document est écrit pour être **appliqué tel quel** par les sessions 6E.1 à 6E.4 du plan
(doc 06) : chaque partie donne les structures, les algorithmes, les constantes, puis les fichiers
et fonctions existants à modifier ou à créer.

Rien n'est codé à ce stade. Les numéros de ligne cités sont ceux du dépôt au 14 novembre 2026
(commit `011f688`) ; ils servent de repère, pas de vérité.

## 0. Résumé des choix

| Sujet | Choix |
| --- | --- |
| Représentation | L'identifiant de bloc `water` reste « il y a de l'eau ici ». Une **couche fluide** parallèle, un octet par cellule, allouée seulement dans les chunks qui en ont besoin, porte le niveau (0 à 8 huitièmes), une direction de flux et un drapeau de régime permanent. Aucun identifiant de bloc consommé pour les niveaux. |
| Compatibilité | Un chunk sans couche fluide se lit exactement comme aujourd'hui : toute cellule `water` est pleine et au repos. Vanilla ne change pas d'un octet (drapeau `Systems::living_water`, faux pour vanilla). |
| Simulation | Automate cellulaire entier, conservatif, à pas de 0,2 s : chute, étalement par égalisation, déversement au bord, égalisation à distance et pression par recherche bornée (surface plane, vases communicants), ruissellement des films vers la pente la plus proche. |
| Performance | Seules les cellules **éveillées** sont calculées, avec un budget par tick ; une eau qui ne bouge plus s'endort ; un écoulement stable (cascade) passe en **régime permanent** et ne coûte plus rien. |
| Réservoirs | Mer et rivières générées : infinies (elles donnent et absorbent). Lacs et étangs : volume fini, ils se vident pour de vrai et se rechargent lentement. Tout le reste (seau, source, pluie) : volume fini. |
| Courants | Champ analytique pour les rivières générées (sens de la pente, accélération vers les seuils), direction stockée pour les écoulements permanents, gradient de niveau pour l'eau libre. Lu par le joueur, les mobs, les bateaux et les saumons. |
| Génération | Les barrages cachés des marches de rivière deviennent de vraies chutes ; cascades de paroi, sources, résurgences et torrents glaciaires passent par la grille de structures existante, avec un tracé pur et déterministe. Tout est généré déjà en régime permanent. |
| Réseau | L'hôte simule seul. Les invités reçoivent la couche fluide avec les chunks et des deltas par pas (protocole 5). |

## 1. Vocabulaire et unités

* **Huitième** (noté `h`) : un huitième du volume d'un bloc. C'est l'unité de volume d'eau, la
  même que celle des couches de neige (`snotacke_1..7`).
* **Niveau** : nombre de huitièmes dans une cellule, de 0 à 8.
* **Charge** d'une cellule (`head`) : `8·y + niveau`. C'est la hauteur de sa surface en huitièmes.
* **Pas d'eau** : une itération de la simulation. Durée nominale `STEP_TICKS = 12` ticks de jeu
  (le jeu tourne à 60 ticks par seconde, `nb-game/src/fixed_step.rs:4`), soit 5 pas par seconde.
* **Nappe** : eau placée par la génération (`WaterColumn` de `norrcraft/hydro.rs`), au repos.
* **Eau libre** : toute eau qui n'est pas une nappe infinie (versée, issue d'une source, sortie
  d'un lac percé).
* **Cellule ouverte** : cellule qui peut contenir de l'eau (air, eau, bloc perméable).
* **Cellule soutenue** : la cellule du dessous est fermée, ou ouverte et pleine.

## 2. Modèle de données

### 2.1 Couche fluide d'un chunk

Nouveau fichier `crates/nb-world/src/fluid/layer.rs`.

```rust
pub const FULL: u8 = 8;

/// Direction d'un flux permanent.
#[repr(u8)]
pub enum Dir { None = 0, PosX = 1, NegX = 2, PosZ = 3, NegZ = 4, Down = 5 }

/// Octet de la couche fluide.
///   bits 0..3 : code. 0 = défaut ; 1..=9 = niveau explicite + 1 (donc niveau 0..=8).
///   bits 4..6 : Dir du flux permanent (0 hors régime permanent).
///   bit 7     : STEADY, la cellule fait partie d'un écoulement en régime permanent.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct FluidCode(pub u8);

impl FluidCode {
    pub const DEFAULT: Self = Self(0);
    pub fn explicit(level: u8, dir: Dir, steady: bool) -> Self;
    /// Niveau de la cellule ; `default_level` est celui de la classe du bloc (voir 2.2).
    pub fn level(self, default_level: u8) -> u8;
    pub fn is_default(self) -> bool;   // (self.0 & 0x0F) == 0
    pub fn dir(self) -> Dir;
    pub fn steady(self) -> bool;
}

/// 32 768 octets, même ordre que les voxels (`x + 16·y + 2048·z`).
pub struct FluidLayer(Box<[u8; CHUNK_VOLUME]>);
```

Dans `crates/nb-world/src/chunk.rs`, `Chunk` (ligne 45) gagne un champ
`fluid: Option<FluidLayer>` (`None` à la création) et les méthodes :

```rust
pub fn fluid_code(&self, x: i32, y: i32, z: i32) -> FluidCode;      // DEFAULT sans couche ou hors limites
pub fn set_fluid_code(&mut self, x: i32, y: i32, z: i32, c: FluidCode); // alloue la couche au besoin
pub fn fluid_layer(&self) -> Option<&[u8; CHUNK_VOLUME]>;
pub fn set_fluid_layer(&mut self, layer: Option<Box<[u8; CHUNK_VOLUME]>>);
```

Règles :

* `Chunk::fingerprint()` (ligne 148) ne lit que les voxels : inchangé, les empreintes de
  référence aussi.
* `Chunk::replace_voxels` (ligne 83) **assainit** la couche : après la copie, toute cellule dont
  le nouveau voxel n'est ni l'eau ni un bloc perméable voit son octet remis à 0. La surcouche
  saisonnière (`season_job`, `manager.rs:339`) et le réseau passent par là. Comme `Chunk` ne
  connaît pas le registre, la fonction prend un paramètre supplémentaire
  `keep: &[bool; 256]` (table « ce bloc peut porter de l'eau ») ; les appelants existants passent
  la table de `FluidBlocks` (2.2) rangée dans `Inner` du `ChunkManager`, ou `&[true; 256]` quand
  le monde n'a pas d'eau vivante.
* `ChunkSnapshot` (ligne 289) gagne `fluid: Option<Box<[u8; CHUNK_VOLUME]>>` copié par
  `ChunkSnapshot::of`.
* Une couche entièrement nulle est libérée au moment de la sauvegarde (`fluid = None`).

**Invariant I1.** Pour toute cellule :

| Voxel | Niveau permis | Sans octet explicite |
| --- | --- | --- |
| `water` | 1 à 8 | 8 |
| bloc aquatique | 0 à 8 | 8 |
| bloc perméable | 0 à 8 | 0 |
| tout autre bloc, air compris | 0 | 0 |

De l'eau qui entre dans de l'air change le voxel en `water` ; une cellule `water` qui tombe à 0
redevient de l'air. Tout le code existant qui teste `== ids.water` reste donc juste pour l'eau
libre ; il ne voit pas l'eau contenue dans un bloc perméable, d'où les aides de la section 6.1.

### 2.2 Classes de blocs

`crates/nb-data/src/blocks.rs` : `BlockProperties` (ligne 96) et `BlockType` (ligne 126) gagnent
un champ `fluid: FluidKind` (`#[serde(default)]`), lu dans `blocks.json` sous la clé `"fluid"` :

```rust
#[derive(Default)]
pub enum FluidKind {
    #[default] Auto,   // déduit, voir ci-dessous
    Permeable,         // "permeable" : garde sa place, l'eau le traverse et le remplit
    Aquatic,           // "aquatic"   : perméable, et immergé par défaut (plantes d'eau)
    Washable,          // "washable"  : emporté par l'eau (devient de l'air)
    Closed,            // "closed"    : étanche
}
```

Attention : le littéral `BlockProperties { .. }` du bloc « manquant » dans `from_palette`
(ligne 503) doit recevoir le nouveau champ.

Nouveau `crates/nb-world/src/fluid/blocks.rs` :

```rust
pub enum FluidClass { Closed, Empty, Water, Permeable, Aquatic, Washable }

pub struct FluidBlocks {
    pub class: [FluidClass; 256],
    pub air: BlockId, pub water: BlockId, pub lava: BlockId,
    pub spring: BlockId,      // `kalla`
    pub new_ice: BlockId,     // `nyis`
    pub aufeis: BlockId,      // `svallis`
    pub quench: BlockId,      // `basaltsten`
    pub seep: BlockId,        // `vitmossa`
}
impl FluidBlocks {
    pub fn from_registry(r: &BlockRegistry) -> Self;
    pub fn default_level(&self, id: BlockId) -> u8; // 8 pour Water et Aquatic, sinon 0
    pub fn open(&self, id: BlockId) -> bool;        // tout sauf Closed
    pub fn keep_table(&self) -> [bool; 256];        // Water, Permeable, Aquatic
}
```

Déduction pour `FluidKind::Auto` : `air` → `Empty` ; `water` → `Water` ; bloc `cross_shaped` →
`Washable` ; bloc de couche (`height` de 1 à 7) → `Washable` ; tout le reste → `Closed` (lave
comprise, voir 4.6).

Valeurs explicites à écrire dans `assets/norrcraft/blocks.json` :

| Blocs | `fluid` |
| --- | --- |
| `vass`, `kaveldun`, `nackros`, `blastang`, `sockertang`, et les nouvelles plantes de 5.2 | `aquatic` |
| `staket`, `gardsgard`, `hassja`, `planka` | `permeable` |
| `fackla` | `washable` |

### 2.3 Table d'eau d'un chunk (repos et courant analytique)

La simulation doit savoir, pour une cellule, si elle appartient à une nappe infinie, et le rendu
comme la physique ont besoin du courant des rivières générées. Les deux viennent du générateur,
qui reste une fonction pure.

`crates/nb-world/src/profile.rs`, trait `TerrainGenerator` (ligne 97), nouvelle méthode :

```rust
/// Table d'eau des 16 × 16 colonnes du chunk `(cx, cz)`, `[lz * 16 + lx]`.
/// `None` : l'univers n'a pas d'eau vivante.
fn water_table(&self, _cx: i32, _cz: i32) -> Option<Box<WaterTable>> { None }
```

Nouveau `crates/nb-world/src/fluid/table.rs` :

```rust
#[repr(u8)]
pub enum RestKind { Dry = 0, Sea = 1, Lake = 2, River = 3 }

#[derive(Clone, Copy)]
pub struct WaterRest {
    pub top: u8,        // fond : premier y non solide (WaterColumn::top)
    pub level: u8,      // premier y d'air au-dessus de la nappe (WaterColumn::level)
    pub kind: RestKind,
    pub flow: [i8; 2],  // courant de la nappe en seizièmes de bloc par seconde (x, z)
}
pub struct WaterTable(pub [WaterRest; 256]);

impl WaterRest {
    /// La cellule `y` de cette colonne est-elle dans une nappe infinie ?
    pub fn infinite(&self, y: i32) -> bool {
        matches!(self.kind, RestKind::Sea | RestKind::River)
            && y >= i32::from(self.top) && y < i32::from(self.level)
    }
}
```

Implémentation dans `NorrcraftTerrain` (`norrcraft/terrain.rs`, bloc `impl TerrainGenerator`,
ligne 719) : `water_table` appelle `self.analyse(x0, z0)` (ligne 312), puis pour chaque colonne
recopie `top`, `level`, `kind` de `cols.water[(lz + 1) * 18 + lx + 1]` et calcule `flow` par la
fonction `river_flow` de la section 8.1. Coût : celui d'une analyse (environ 0,3 ms), payé une
fois par chunk et mis en cache par la simulation (3.2).

`crates/nb-world/src/profile.rs` : `Systems` (ligne 139) gagne `pub living_water: bool`, vrai dans
`WorldProfile::norrcraft`, faux ailleurs.

### 2.4 Accès du gestionnaire de chunks

`crates/nb-world/src/manager.rs`, `ChunkManager` :

```rust
/// Bloc canonique et code fluide ; `None` si le chunk n'est pas actif.
pub fn fluid_cell(&self, x: i32, y: i32, z: i32) -> Option<(BlockId, FluidCode)>;

/// Écriture de la simulation : voxel + code, sans journal d'éditions et sans marquer le
/// maillage (la simulation le fait en fin de pas). Marque `dirty` ; marque `light_dirty` et
/// incrémente `edits` seulement si l'identifiant du voxel change. `false` si chunk absent.
pub fn write_fluid(&self, x: i32, y: i32, z: i32, id: BlockId, code: FluidCode) -> bool;

/// Niveau d'eau 0..=8 (chunk absent → 0).
pub fn water_level(&self, x: i32, y: i32, z: i32) -> u8;

/// Événements à consommer par la simulation.
pub fn take_fluid_events(&self) -> FluidEvents;
pub struct FluidEvents { pub edits: Vec<IVec3>, pub loaded: Vec<ChunkPos>, pub unloaded: Vec<ChunkPos> }
```

`Inner` (ligne 148) gagne `fluid_blocks: Option<FluidBlocks>` et
`fluid_events: Mutex<FluidEvents>`, alimenté par :

* `write_block` (ligne 907) : pousse `(x, y, z)` dans `edits`, et remet l'octet fluide de la
  cellule à `DEFAULT` (un bloc posé ou cassé à la main efface l'état fluide de sa cellule ; poser
  de l'eau « à la main » donne donc une cellule pleine) ;
* `load_job` (ligne 283), après l'insertion dans `active` : pousse `pos` dans `loaded` ;
* `unload_chunk` (ligne 765) : pousse `pos` dans `unloaded` ;
* `install_chunk` (ligne 890) : pousse `pos` dans `loaded` (invités).

`ChunkEntry` (`chunk.rs:211`) gagne `set_fluid(&self, x, y, z, id, code) -> bool` qui tient le
verrou d'écriture une seule fois pour le voxel et l'octet.

Rien n'est poussé quand `fluid_blocks` est `None` : coût nul pour vanilla.

## 3. Simulation

Nouveau module `crates/nb-world/src/fluid/` : `mod.rs`, `layer.rs`, `blocks.rs`, `table.rs`,
`sim.rs` (file et pas), `rules.rs` (règles pures), `flow.rs` (courants), `sources.rs`
(sources, pluie, gel). Le module est déclaré dans `nb-world/src/lib.rs`.

### 3.1 Abstraction du monde (testable sans chunks)

```rust
pub trait FluidWorld {
    /// `None` : chunk non chargé (la cellule est alors un mur).
    fn cell(&self, p: IVec3) -> Option<(BlockId, FluidCode)>;
    fn write(&mut self, p: IVec3, id: BlockId, code: FluidCode);
    /// Table d'eau du chunk de la colonne, si l'univers en a une.
    fn rest(&self, x: i32, z: i32) -> Option<WaterRest>;
}
```

Deux implémentations : `ManagerFluid<'a>` sur `&ChunkManager` (garde en cache les 4 derniers
`SharedChunk` pour éviter la table de hachage à chaque lecture) et `MemFluid` (table de hachage,
pour les tests, avec une table d'eau optionnelle par colonne).

### 3.2 État de la simulation

```rust
pub struct FluidSim {
    rules: WaterRules,                       // assets/norrcraft/water.json (section 14)
    blocks: FluidBlocks,
    cur: VecDeque<IVec3>,                    // cellules à traiter dans le pas en cours
    next: Vec<IVec3>,                        // cellules éveillées pour le pas suivant
    queued: HashSet<IVec3>,                  // appartenance à cur ∪ next
    received: HashMap<IVec3, u8>,            // huitièmes reçus pendant le pas en cours
    meta: HashMap<IVec3, CellMeta>,          // mémoire des cellules éveillées
    tables: HashMap<ChunkPos, Arc<WaterTable>>,
    emitters: HashMap<IVec3, Emitter>,       // blocs `kalla` des chunks simulés (section 4.2)
    dirty: HashSet<ChunkPos>,                // chunks à remailler en fin de pas
    deltas: Vec<FluidDelta>,                 // changements du pas, pour le réseau (section 12)
    step: u64, step_tick: u32, searches_left: u32,
    rng: SplitMix64,                         // nb_world::norrcraft::math, graine = graine du monde ^ 0x4E52_5743_0000_0801
    pub stats: FluidStats,                   // cellules éveillées, traitées, µs du dernier tick
}

struct CellMeta { hist: [u8; 2], calm: u8, flux: bool, out: Dir }   // hist : niveaux de fin des 2 pas précédents
pub struct FluidDelta { pub pos: IVec3, pub block: BlockId, pub code: u8 }
```

`World` (`nb-world/src/world.rs:18`) gagne `fluid: Option<Mutex<FluidSim>>`, créé dans
`World::from_parts` quand `profile.systems.living_water`, et les méthodes :

```rust
pub fn tick_fluids(&self, env: &FluidEnv);          // sans effet si pas d'eau vivante ou monde client
pub fn water_level(&self, x: i32, y: i32, z: i32) -> u8;
pub fn current_at(&self, x: i32, y: i32, z: i32) -> Vec3;   // section 6.2
pub fn take_fluid_deltas(&self) -> Vec<FluidDelta>;
pub fn fluid_stats(&self) -> Option<FluidStats>;

pub struct FluidEnv {
    pub season_step: Option<u8>,   // pas de saison 0..8
    pub rain: f32,                 // 0..1 (Environment)
    pub air_temp: f32,             // °C ressentis au sol près du joueur
    pub daylight: bool,
    pub centers: Vec<Vec3>,        // joueurs (local + distants)
}
```

`tick_fluids` est appelé par `WorldSession::tick` (`nb-game/src/session.rs:318`) juste après
`self.sim.tick(&mut ctx, dt)`, avec un `FluidEnv` construit depuis `self.environment`, le
calendrier (`step_of`) et `self.world` (centres supplémentaires déjà connus : `World::extras`).
Jamais appelé quand `chunk_manager().is_remote()`.

### 3.3 Déroulement d'un tick

```
fn tick(&mut self, w: &mut impl FluidWorld, cm_events: FluidEvents, env: &FluidEnv):
    t0 = Instant::now()
    self.absorb_events(w, cm_events)          // 3.7
    self.searches_left = PRESSURE_SEARCHES_PER_TICK
    n = 0
    while n < CELLS_PER_TICK:
        match self.cur.pop_front():
            None => break
            Some(p) => { self.update_cell(w, p); n += 1 }     // 3.4
        if n % 64 == 0 && t0.elapsed() > TIME_CAP: break
    self.step_tick += 1
    if self.cur.is_empty() && self.step_tick >= STEP_TICKS:
        self.end_step(w, env)                 // 3.6
```

Constantes (réglables dans `water.json`) :

| Constante | Valeur | Rôle |
| --- | --- | --- |
| `STEP_TICKS` | 12 | durée minimale d'un pas, en ticks |
| `CELLS_PER_TICK` | 512 | cellules traitées par tick au plus |
| `TIME_CAP` | 400 µs | plafond de temps par tick |
| `PRESSURE_SEARCHES_PER_TICK` | 48 | recherches de pression par tick |
| `PRESSURE_NODES` | 96 | nœuds visités par recherche au plus |
| `SPREAD_MAX` | 4 | huitièmes étalés par cellule et par pas au plus |
| `SEEK_RADIUS` | 5 | rayon de recherche d'une pente par un film |
| `CALM_STEPS` | 6 | pas sans changement avant sommeil ou régime permanent |
| `SIM_RADIUS` | 6 | rayon (en chunks) autour d'un joueur où l'eau est simulée |
| `BOOT_WAKE_MAX` | 2048 | cellules réveillées au chargement d'un chunk au plus |

Quand la file dépasse le budget, le pas dure simplement plus longtemps : l'eau ralentit, le jeu
non. C'est le comportement voulu.

### 3.4 Mise à jour d'une cellule

Notations : `lvl(q)` niveau courant de `q` ; `inf(q)` = `q` est dans une nappe infinie
(`rest(q).infinite(q.y)` et le voxel est ouvert) ; `open(q)` = chunk chargé et classe non
`Closed` ; `cap(q)` = `∞` si `inf(q)`, sinon `8 − lvl(q)` ; `D`, `U` = cellule du dessous, du
dessus ; `N[0..4]` = voisines horizontales.

```
fn update_cell(w, p):
    queued.remove(p)
    if !loaded(p) || !in_sim_radius(p): return            // elle sera réveillée au retour d'un joueur
    (id, code) = w.cell(p)
    if !blocks.open(id): return
    if inf(p):                                            // R0 : la nappe infinie se refait
        set_level(p, 8)                                   // n'écrit que si la valeur change
        avail = 8
    else:
        avail = lvl(p) − received.get(p)                  // l'eau reçue ce pas ne repart pas ce pas
    if avail == 0: return
    moved = 0

    // R1. Chute : une cellule par pas (5 blocs par seconde), pour que la colonne d'eau qui
    //     tombe reste continue à l'écran.
    if open(D) && cap(D) > 0:
        t = min(avail, cap(D))
        transfer(p, D, t, Dir::Down); moved += t; avail −= t
    if avail == 0: return

    // R2. Étalement (seulement si la cellule est soutenue).
    if supported(p):
        cand = N.filter(open)                             // ordre de départ tourné de (step + hash(p)) % 4
        budget = min(avail, SPREAD_MAX)
        loop while budget > 0:
            n = cand.min_by(eff)                          // eff(n) = −8 si bord (n non soutenue), sinon lvl(n)
            mine = lvl(p)
            if is_edge(n):  ok = cap(n) > 0               // déversement : jusqu'au dernier huitième
            else:           ok = mine − lvl(n) >= 2 && cap(n) > 0
            if !ok: break
            transfer(p, n, 1, dir_to(n)); moved += 1; avail −= 1; budget −= 1

    // R3. Égalisation à distance et pression (surface plane, vases communicants).
    if moved == 0 && supported(p) && lvl(p) >= 2 && searches_left > 0:
        searches_left −= 1
        if let Some(t) = pressure_target(w, p):           // 3.5
            amount = min(avail, cap(t), max(1, (head(p) − head(t)) / 2))
            transfer(p, t, amount, Dir::None); moved += amount; avail −= amount

    // R4. Ruissellement d'un film.
    if moved == 0 && lvl(p) == 1 && supported(p):
        if let Some(n) = seek_slope(w, p):                // premier pas du plus court chemin vers un bord
            transfer(p, n, 1, dir_to(n)); moved += 1
```

L'étalement R2 seul laisserait une pente d'un huitième par cellule (8, 7, 6, 5…), puisqu'il
n'agit qu'à partir d'un écart de 2 entre voisines. C'est R3 qui aplanit : une cellule qui n'a
rien pu donner à ses voisines cherche plus loin une cellule plus basse d'au moins 2 h et lui
envoie de l'eau directement.

Précisions :

* `supported(p)` : `!open(D) || cap(D) == 0`.
* `is_edge(n)` : `open(n − Y) && cap(n − Y) > 0` (l'eau qui y entre tombera).
* `transfer(a, b, t, dir)` :
  1. `t = min(t, lvl(a) si !inf(a), cap(b))`, rien si `t == 0` ;
  2. si `!inf(a)` : `set_level(a, lvl(a) − t)` ; si `!inf(b)` : `set_level(b, lvl(b) + t)` et
     `received[b] += t` ;
  3. `meta[a].flux = meta[b].flux = true`, `meta[a].out = dir` ;
  4. `wake(a)`, `wake(b)` et `wake` des 6 voisines de `a` et de `b` qui portent de l'eau.
* `set_level(q, l)` écrit par `w.write` : si `l == 0` et que le voxel est `water`, le voxel
  devient l'air ; si `l > 0` et que le voxel est l'air ou un bloc `Washable`, le voxel devient
  `water` (le bloc emporté disparaît, sans butin) ; l'octet devient
  `FluidCode::explicit(l, Dir::None, false)`, ou `DEFAULT` quand `l` vaut le niveau par défaut de
  la classe (une eau redevenue pleine ne garde aucune trace). Chaque écriture ajoute le chunk à
  `dirty` et pousse un `FluidDelta`.
* `seek_slope(w, p)` : parcours en largeur, rayon `SEEK_RADIUS`, sur les cellules ouvertes et
  soutenues du même `y` dont le niveau est 0 ou 1 ; s'arrête sur la première cellule `is_edge` ou
  dont une voisine a une charge inférieure d'au moins 2 ; renvoie le premier pas du chemin.
* Une cellule infinie ne perd jamais d'eau et accepte tout : la mer avale ce qu'on y verse, une
  rivière alimente sans fin un canal creusé depuis sa berge.
* **R5. Trempe.** Dans `transfer`, si `b` a une voisine (6 directions) de lave : la lave devient
  `basaltsten`, `t` huitièmes disparaissent de `b` (vapeur), et un événement `FluidFx::Quench`
  est émis pour les particules et le son.

Ce jeu de règles conserve le volume : chaque transfert retire autant qu'il ajoute, sauf vers ou
depuis une cellule infinie, et sauf R5. La somme des carrés des niveaux décroît strictement à
chaque étalement (écart d'au moins 2), ce qui garantit que l'eau finit par s'immobiliser.

### 3.5 Égalisation et pression : recherche bornée

```
fn pressure_target(w, s) -> Option<IVec3>:
    // s : cellule soutenue qui n'a rien pu donner à ses voisines.
    best = None
    file = [s] ; vus = {s} ; visites = 0
    while let Some(c) = file.pop_front() and visites < PRESSURE_NODES:
        visites += 1
        for v in [c − Y, c ± X, c ± Z, c + Y]:          // le bas d'abord
            if v in vus || v.y > s.y: continue          // jamais au-dessus de la surface de départ
            vus.insert(v)
            if !open(v) || inf(v): continue             // on ne traverse pas une nappe infinie
            if cap(v) > 0 && head(v) + 2 <= head(s):    // débouché plus bas d'au moins 2 h
                if best.map_or(true, |b| head(v) < head(b)): best = Some(v)
            // Traversée : de l'eau pleine dans toutes les directions (pression), ou de l'eau
            // quelconque à l'horizontale (surface d'une même nappe).
            if lvl(v) == 8 || (v.y == c.y && lvl(v) >= 1): file.push_back(v)
    best
```

Une cellule d'une nappe infinie peut lancer la recherche (c'est elle qui remplit un canal creusé
depuis la mer), mais la recherche ne traverse jamais une cellule infinie : elle ne se perd pas
dans la mer.

Effets : deux bras d'un tube en U s'égalisent à un huitième près ; un lac percé par le fond
s'abaisse autour du trou au lieu de garder un puits vertical ; l'eau remonte dans un conduit
jusqu'à la hauteur de la surface qui la pousse, jamais plus haut.

### 3.6 Fin de pas : sommeil et régime permanent

```
fn end_step(w, env):
    for p in take(next):                                  // cellules éveillées pour le pas suivant
        m = meta.entry(p)
        l = lvl(p)
        // Calme si le niveau de fin de pas reprend l'une des deux valeurs précédentes : un flux
        // constant peut osciller d'un huitième d'un pas à l'autre selon l'ordre de traitement.
        if l == m.hist[0] || l == m.hist[1]: m.calm += 1 else: m.calm = 0
        m.hist = [l, m.hist[0]]
        if m.calm >= CALM_STEPS:
            if m.flux && l > 0:                           // traversée par un flux constant
                write code = explicit(l, m.out, steady = true)
            meta.remove(p); queued.remove(p)              // elle sort de la file
        else:
            m.flux = false
            cur.push_back(p)
    received.clear()
    sources::step(self, w, env)                           // section 4
    for c in take(dirty): mark_mesh_dirty(c) et de ses 4 voisins si une cellule de bord a changé
    step += 1 ; step_tick = 0
```

* `wake(p)` : si `p` n'est pas dans `queued`, l'y mettre et le pousser dans `next` ; si son code
  est `STEADY`, effacer le drapeau et la direction (le niveau reste).
* Une cellule en régime permanent n'est plus calculée. Elle garde son niveau et sa direction,
  que le rendu et les courants lisent. Tout transfert qui la touche la réveille, et de proche en
  proche la chaîne se recalcule puis se stabilise à nouveau.
* Une cellule qui n'a rien donné ni reçu pendant le pas n'est pas remise dans `next` : `transfer`
  réveille tout ce qu'il touche, c'est le seul chemin de retour dans la file avec `wake`.
* `tables` sert aussi aux invités LAN, qui ne simulent pas mais lisent les courants : la table
  est calculée à la demande par `World::current_at`, et le cache est vidé d'un coup au-delà de
  1 024 chunks.

### 3.7 Réveils

`absorb_events` traite `FluidEvents` :

* `edits` : pour chaque position, `wake` de la cellule et de ses 6 voisines **si elles portent de
  l'eau ou sont infinies**. Casser un bloc au bord d'un lac réveille donc l'eau voisine, qui
  coule dans le trou.
* `loaded` :
  1. charger (ou calculer) la table d'eau, la ranger dans `tables` ;
  2. recenser les émetteurs : parcours des voxels du chunk à la recherche de `kalla` (comparaison
     d'octets, quelques microsecondes) ;
  3. réveiller au plus `BOOT_WAKE_MAX` cellules dont l'octet est explicite et non `STEADY` ;
  4. réveiller, dans les 4 chunks voisins, les cellules de la face commune qui portent un octet
     explicite (l'eau qui attendait au bord repart).
* `unloaded` : retirer de `cur`, `next`, `queued`, `meta`, `emitters`, `tables` tout ce qui est
  dans le chunk.

Une cellule d'un chunk non chargé, ou hors de `SIM_RADIUS` de tout joueur, est un **mur** : l'eau
s'accumule contre elle sans disparaître, puis reprend quand le chunk revient.

## 4. Sources et puits

### 4.1 Nappes générées

| Nature (`RestKind`) | Comportement |
| --- | --- |
| `Sea` | Infinie. Donne et absorbe sans limite. Ne baisse jamais. |
| `River` | Infinie. Porte le courant analytique de 8.1. |
| `Lake` (lacs, étangs, gouilles) | Finie. Au repos tant que rien ne la touche. Percée, elle se vide réellement, cellule par cellule. |

Recharge des lacs (`sources::step`, toutes les `RECHARGE_STEPS = 50` pas) : tirer
`RECHARGE_SAMPLES = 16` colonnes au hasard dans le rayon de simulation ; pour une colonne `Lake`
dont la cellule `level − 1` est ouverte et non pleine, et dont la cellule juste dessous est
pleine ou fermée, ajouter 1 h (2 h s'il pleut). Un lac percé puis rebouché remonte donc en
quelques jours de jeu ; un lac dont la fuite reste ouverte ne se remplit pas.

### 4.2 Sources (`kalla`)

Bloc solide `kalla` (id 177). Posé par la génération (section 8), jamais fabricable.

```rust
struct Emitter { kind: SpringKind, acc: f32 }
enum SpringKind { Mountain, Karst, Glacier }     // lu dans la roche voisine : kalksten → Karst, glace → Glacier
```

À chaque pas : `acc += rate(kind) × season_factor(kind, env.season_step)` ; tant que `acc >= 1`,
verser 1 h dans la première cellule ouverte parmi le dessus puis les 4 côtés (dans l'ordre fixe
+X, −X, +Z, −Z), `acc −= 1`. Verser = `set_level(+1)` puis `wake`.

`water.json` :

| Source | `rate` (h par pas) | Facteur par pas de saison (0 = cœur de l'hiver) |
| --- | --- | --- |
| `Mountain` | 3,0 | 0,3 0,5 1,6 1,8 1,0 0,8 0,7 0,4 |
| `Karst` | 4,0 | 0,8 0,9 1,3 1,2 1,0 0,9 0,9 0,8 |
| `Glacier` | 5,0 | 0,0 0,0 0,6 1,4 2,0 1,6 0,6 0,1 |

La fonte des neiges est donc le pic de printemps des sources de montagne et l'été des torrents
glaciaires. Un torrent glaciaire s'arrête en hiver : son lit se vide, ses cellules permanentes se
réveillent et s'écoulent. Un changement de pas de saison réveille tous les émetteurs.

### 4.3 Suintement des tourbières

Le bloc `vitmossa` (sphaigne, id 25) est un émetteur faible : tous les `SEEP_STEPS = 40` pas,
`SEEP_SAMPLES = 8` colonnes tirées au hasard ; si le bloc de surface est `vitmossa`, qu'il pleut
ou que le pas de saison est 2 ou 3 (dégel), et qu'une voisine horizontale est ouverte et plus
basse, y verser 1 h. Les gouilles se remplissent, les bords de tourbière ruissellent.

### 4.4 Pluie et évaporation

Tous les `WEATHER_STEPS = 10` pas, `RAIN_SAMPLES = 12` colonnes tirées dans le rayon de
simulation. Pour chaque colonne, `y` = première cellule ouverte en descendant depuis le ciel
(lecture par `read_column`) :

* **Pluie** (`env.rain > 0,3`, température positive) : si la cellule est soutenue, qu'elle n'est
  pas dans une nappe, et que ses 4 voisines ont un sol au moins aussi haut (creux), ajouter 1 h
  tant que son niveau est inférieur à `PUDDLE_MAX = 2`.
* **Évaporation** (pas de pluie, jour, température positive) : si la cellule porte de l'eau libre
  de niveau 1 ou 2, non permanente, retirer 1 h.

### 4.5 Gel et dégel de l'eau libre

Les nappes gèlent par la surcouche saisonnière existante (`seasonal::freeze`), inchangée. Pour
l'eau libre, dans le même tirage que 4.4 :

* `env.air_temp < −2` et cellule à ciel ouvert, au repos (pas dans `queued`, pas `STEADY`) :
  niveau 8 → `nyis` ; niveau 1 à 7 → `svallis` (nouveau bloc, couche de glace de 2 huitièmes,
  id 179). Le volume gelé sort de la simulation.
* `env.air_temp > 2` : `nyis` ou `svallis` **hors colonne de nappe** → eau de niveau 8 ou 2.
* L'eau qui coule (éveillée ou permanente) ne gèle pas ainsi ; les cascades générées gèlent par
  la surcouche (7.2).

### 4.6 Lave

La lave reste statique et `Closed`. Seule interaction : la trempe R5.

### 4.7 Seau et commande

Objets purs `hink` (seau, id 315) et `hink_vatten` (seau plein, id 316) dans
`assets/norrcraft/items.json`. Fonctions pures dans `fluid/mod.rs`, câblées plus tard par la 6.7 :

```rust
/// Prend 8 h dans la cellule (pleine ou infinie). `false` sinon.
pub fn scoop(w: &mut impl FluidWorld, sim: &mut FluidSim, p: IVec3) -> bool;
/// Verse `amount` h dans la cellule ouverte `p`. Renvoie ce qui a été versé.
pub fn pour(w: &mut impl FluidWorld, sim: &mut FluidSim, p: IVec3, amount: u8) -> u8;
```

Commande de triche `/water` (`WorldSession::water_command`, sur le modèle de `boat_command`,
`session.rs:373`, appelée au même endroit) :

| Commande | Effet |
| --- | --- |
| `/water pour [n]` | verse `n` h (8 par défaut) dans la cellule visée (`place_target`) |
| `/water spring` | pose un bloc `kalla` sous la cellule visée |
| `/water drain` | retire toute eau libre dans un rayon de 8 |
| `/water wake` | réveille tout dans un rayon de 16 |
| `/water info` | niveau, code, classe, nappe et courant de la cellule visée ; statistiques de la simulation |

## 5. Blocs immergés et plantes aquatiques

### 5.1 Immersion

Un bloc `Permeable` ou `Aquatic` garde son voxel ; l'eau y entre et en sort par les mêmes règles
qu'une cellule d'air (il compte pour `open`). Il est dessiné normalement, et l'eau de sa cellule
est dessinée en plus (section 9). Un bloc `Washable` est remplacé par l'eau qui arrive.

Norrcraft n'a aujourd'hui aucun bloc de forme fine (dalle, escalier, porte, échelle, lanterne) :
ses blocs sont des cubes, des croix ou des couches. La mécanique est livrée complète et s'applique
dès maintenant aux plantes, aux clôtures (`staket`, `gardsgard`), à la `hassja` et aux passerelles
`planka` ; tout futur bloc fin n'aura qu'à déclarer `"fluid": "permeable"` (voir décision E4).

### 5.2 Plantes

Plantes existantes devenues aquatiques : `vass` (roseau), `kaveldun` (massette), `nackros`
(nénuphar), `blastang` (fucus), `sockertang` (laminaire sucrée). Elles occupaient une cellule
d'eau ; elles sont désormais immergées par défaut, donc traversées par les courants et comptées
comme de l'eau par la physique.

Nouveaux blocs (croix, `aquatic`, textures `norr_*` par un nouveau
`scripts/gen_norr_water.py` sur le modèle de `gen_norr_flora.py`) :

| Id | Nom | Plante | Eau | Profondeur | Courant |
| --- | --- | --- | --- | --- | --- |
| 180 | `fraken` | prêle d'eau | douce | 1 | calme (< 0,3) |
| 181 | `nate` | potamot, herbier | douce, saumâtre | 1 à 4 | calme ou lent (< 1,0) |
| 182 | `nackmossa` | fontinale | douce | 1 à 2 | vif (≥ 0,8) seulement |
| 183 | `algras` | zostère | saumâtre, salée | 1 à 5, fond de sable | lent |
| 184 | `fingertang` | laminaire digitée | salée | 3 à 14, empilable jusqu'à 6 | tout |

Autres nouveaux blocs de l'étape : 177 `kalla`, 178 `isfall` (cascade gelée, glace translucide,
solide), 179 `svallis` (couche de glace, `height` 2). Les identifiants 177 à 184 sont libres dans
`assets/norrcraft/blocks.json` (dernier utilisé : 176 `vissen`). Rappel du piège de la 4.4 : un
nom de bloc ne doit pas égaler un nom d'objet.

### 5.3 Vie des plantes (dynamique)

`crates/nb-ecs/src/aquatic.rs`, nouveau système `AquaticSystem` (inséré dans
`WorldSession::new` à côté de `FarmSystem`, univers Norrcraft seulement), 1 fois par seconde,
`AQUATIC_SAMPLES = 24` cellules tirées près du joueur :

* une plante `aquatic` dont la cellule est à sec (niveau 0) depuis le tirage précédent devient
  `vissen` (id 176) ;
* une plante hors de sa plage de courant (`|current_at|` contre le tableau ci-dessus, lu dans
  `water.json`, clé `plants`) est arrachée : `nackmossa` meurt en eau calme, `fraken` et
  `nackros` sont emportés par un courant vif ;
* propagation : une plante dans de bonnes conditions, de mai à août, a une chance sur 40 de poser
  un plant identique dans une cellule voisine qui respecte eau, profondeur, fond et courant.

## 6. Courants

### 6.1 Accès depuis l'ECS

`crates/nb-ecs/src/blocks.rs`, trait `BlockAccess` (ligne 11), deux méthodes avec défaut :

```rust
/// Niveau d'eau 0..=8 de la cellule ; `None` : l'implémentation ne connaît pas les niveaux
/// (l'appelant retombe sur `block == water` → 8).
fn fluid_level(&self, _x: i32, _y: i32, _z: i32) -> Option<u8> { None }
/// Courant en blocs par seconde.
fn current(&self, _x: i32, _y: i32, _z: i32) -> Vec3 { Vec3::ZERO }
```

`WorldBlocks` (ligne 46) les implémente par `World::water_level` et `World::current_at`.
`MemoryBlocks` (ligne 94) gagne `fluids: HashMap<(i32,i32,i32), u8>` et
`currents: HashMap<(i32,i32,i32), Vec3>` avec `set_fluid` et `set_current`, pour les tests.

`BlockEnv` (`nb-ecs/src/physics.rs:33`) gagne :

```rust
pub fn water_level(&self, x: i32, y: i32, z: i32) -> u8;   // fluid_level ou repli sur l'identifiant
pub fn wet(&self, x: i32, y: i32, z: i32) -> bool;         // water_level > 0
/// Hauteur de la surface de l'eau au-dessus de la cellule (x, y, z), en remontant les cellules
/// pleines ; `None` si la cellule est sèche.
pub fn water_surface(&self, x: i32, y: i32, z: i32) -> Option<f64>;   // y_haut + niveau / 8
/// Part du corps sous la surface, 0..1.
pub fn immersion(&self, cx: f64, cy: f64, cz: f64, c: &Collider) -> f32;
/// Courant moyen aux pieds et à mi-corps.
pub fn current_on(&self, cx: f64, cy: f64, cz: f64, c: &Collider) -> Vec3;
```

`is_in_fluid` (ligne 117) devient : lave dans l'AABB, ou `immersion >= 0,4`.
`fluid_at_eye_level` (ligne 132) : eau si `water_surface` de la cellule des yeux dépasse la
hauteur des yeux.

Les tests de présence d'eau `== ids.water` deviennent `env.wet(..)` (ou
`blocks.fluid_level(..).map_or(id == water, |l| l > 0)` là où il n'y a pas de `BlockEnv`) dans :
`nb-ecs/src/physics.rs` (ligne 418, allure de nage des mobs), `boat.rs` (`water_top`, `fetch`,
`medium`), `ice.rs` (`fragile`, `is_hole`, `drill`), `spawn.rs`, `density.rs`, `brain/mod.rs`
(`Ground`), `ai.rs`, `survival.rs` (pêche, mouillage), `ecology.rs` ;
`nb-game/src/audio.rs:434`, `fx.rs`, `anim.rs`, `render.rs`. Les écritures `set_block(.., water)`
(glace qui cède, digue de castor) restent telles quelles : elles posent une cellule pleine et la
simulation prend le relais.

### 6.2 Calcul du courant

`crates/nb-world/src/fluid/flow.rs` :

```
fn current_at(w, p) -> Vec3:
    l = lvl(p) ; if l == 0: return ZERO
    v = ZERO
    if falling(p):                                   // open(D) && cap(D) > 0, ou code.dir == Down
        return (0, −FALL_SPEED, 0)                   // 6,0
    code = w.cell(p).code
    if code.steady && code.dir horizontal:
        v = unit(code.dir) × (if l <= 3 { FILM_SPEED } else { STREAM_SPEED })     // 2,6 et 1,6
    else if inf(p) && rest.kind == River:
        v = (rest.flow.x, 0, rest.flow.z) / 16
    else:                                            // eau libre : pente de la surface
        gx = h(p − X) − h(p + X) ; gz = h(p − Z) − h(p + Z)     // h(q) = head(q) si q porte de l'eau au même y, sinon head(p)
        v = clamp_len((gx, 0, gz) × GRADIENT_SPEED, MAX_FREE)   // 0,4 et 3,0
    if lvl(U) > 0 && falling(U): v.y −= PLUNGE_PULL             // 4,0 : la cascade enfonce
    else:
        for n in voisines horizontales:
            if lvl(n + Y) > 0 && falling(n + Y):                // pied de chute voisin : remous
                r = (n − p) ; v += (−r.z, 0, r.x) × SWIRL       // 1,0, tangentiel
    v
```

### 6.3 Joueur

`crates/nb-ecs/src/player.rs`, `PlayerInputSystem::run`, étape 7 (lignes 224 à 289) :

1. `imm = env.immersion(..)` ; `swim = imm >= 0,4` remplace `in_fluid` pour la nage. Entre 0 et
   0,4 le joueur marche dans l'eau : `speed *= 1 − 0,5 · imm / 0,4`, pas de nage, saut permis.
2. `cur = env.current_on(..)` ; `k = (imm / 0,4).min(1)`.
3. Après les deux `move_player_axis` horizontaux, advection par le courant :
   `env.move_player_axis(t, v, c, cur.x·k·dt, 0, 0)` puis `(0, 0, cur.z·k·dt)`.
4. Vertical : sous une chute (`cur.y < −1`), `v.v.y = v.v.y.min(cur.y · k)` ; la remontée par
   `Jump` est limitée à `jump_force · 0,15` tant que `cur.y < −1`.
5. **Rapides** : si `|cur_xz| > RAPID_SPEED (3,2)`, la vitesse voulue par le joueur est
   multipliée par 0,3 ; si un déplacement horizontal d'advection est arrêté par une collision
   alors que `|cur_xz| > 3,2`, le joueur prend `ROCK_HIT = 0,5` de dégâts, au plus une fois par
   seconde (nouveau champ `Player::rock_timer`).
6. **Contre-courant** : si la survie est active, que le joueur nage et que
   `dot(entrée, cur_xz) < 0`, l'endurance baisse de `UPSTREAM_DRAIN (4,0) × |cur_xz| × dt`
   (nouvelle fonction `survival::record_effort(world, id, amount)` à côté de `record_motion`).
   À endurance nulle, la vitesse de nage tombe à 30 %.
7. Mouillage : `Vitals::wetness = 1` sous une chute, comme en immersion.

Nager sous une chute pour trouver la cavité derrière reste possible : la traction `PLUNGE_PULL`
ne s'applique que dans la colonne de la chute.

### 6.4 Mobs et corps flottants

`crates/nb-ecs/src/physics.rs`, `PhysicsSystem::run` (ligne 407) : après l'intégration de chaque
entité, si `env.immersion(..) > 0,2`, advection `cur × factor × dt` par `move_axis_swim`.
`factor` : 0,6 pour l'allure `Swim` (les poissons tiennent le courant), 1,0 sinon. Un mob
terrestre pris dans un courant de plus de 2,5 passe en état de fuite (`brain::provoke` avec le
stimulus `Stimulus::Danger` existant).

Nouveau composant `Floating { drag: f32 }` (`components.rs`) pour tout ce qui dérive : les
carcasses (`ecology::Carcass`) le reçoivent à leur création. Le moteur n'a pas encore d'objets
jetés au sol (jeter un objet le détruit, `player.rs:382`) : le jour où ils existeront, ils
recevront `Floating` et dériveront sans autre travail.

### 6.5 Bateaux

`crates/nb-ecs/src/boat.rs` :

* `water_top` (ligne 325) accepte l'eau immergée et exige un **tirant d'eau** : la cellule de
  surface doit avoir un niveau d'au moins 5, ou de l'eau pleine dessous ; sinon `medium` renvoie
  `Land` (le bateau s'échoue sur un haut-fond ou un film).
* `BoatSystem::run`, avant l'appel à `self.advance` (ligne 560) : `cur = ctx.blocks.current(..)`
  sous la coque ; si le bateau n'est ni ancré ni à sec, `vx += cur.x`, `vz += cur.z`. Remonter
  une rivière à la rame devient plus lent, la descendre plus rapide ; un bateau sans pilote
  dérive.
* **Chute** : dans `advance`, quand le milieu devant est `Blocked` et qu'une surface d'eau existe
  de 2 à 8 blocs plus bas dans la colonne sondée, le bateau dévale : position posée sur la
  surface basse, `b.water += 0,12 × hauteur`, vitesse divisée par deux, événement
  `EcsEvent::BoatSwamped` si le seuil est passé. Un radeau (`BoatKind` sans bordé) se disloque
  au-delà de 4 blocs (`BoatSank`).
* L'ancre résiste jusqu'à un courant de 3,0 ; au-delà elle chasse (le bateau dérive à 30 %).

### 6.6 Saumons

`assets/norrcraft/behaviors.json`, profil `lax` : remplacer `"heading": [0.0, -1.0]` par
`"upstream": true`. `crates/nb-data/src/behavior.rs` : champ optionnel `upstream: bool` du
profil. `crates/nb-ecs/src/brain/mod.rs`, état de migration des nageurs :

* direction voulue = `−normalize(current_xz)` quand `|current_xz| > 0,2`, sinon cap précédent ;
* **saut** : si de l'eau qui tombe se trouve à moins de 2 cellules devant et que la surface haute
  est à `LEAP_MAX = 6` blocs au plus, une tentative toutes les 3 s, réussie avec la probabilité
  `0,35` : le poisson est porté sur une parabole jusqu'à la cellule d'eau en amont de la lèvre
  (nouvel `AnimalAct::Leap`, son et éclaboussure) ;
* un échec le laisse au pied de la chute : les saumons s'y rassemblent de mai à août, ce qui
  attire les ours (`allatare` chasse déjà `lax`) et fait de chaque chute un lieu de pêche.

Les hardes de saumon (`nb-world/src/herds.rs`) restreignent leurs sites aux colonnes `River` par
`water_table` ; c'est une ligne dans le prédicat de site.

### 6.7 Crochet pour l'eau motrice (session 6.6)

```rust
/// Puissance disponible pour une roue en `p` : |courant horizontal| × niveau / 8, plus 2 × le
/// débit d'une chute qui tombe sur `p + Y`.
pub fn flow_power(w: &impl FluidWorld, p: IVec3) -> f32;
```

Rien d'autre n'est fait ici pour les roues et les moulins : ils restent à la session 6.6, qui
dépend désormais de cette étape.

## 7. Glace et saisons

### 7.1 Glace jouable (`nb-ecs/src/ice.rs`)

* `collapse`, `drill`, `saw`, `climb_out` écrivent `water` : la cellule est pleine, la simulation
  réveille les voisines. Dans un lac (fini, au repos) rien ne bouge ; c'est le comportement
  actuel.
* `fragile` (ligne 123) : en plus de l'eau libre voisine, la glace est fragile si le courant sous
  elle dépasse 0,5 (`blocks.current(pos − Y)`) : embouchures, détroits, abords des chutes.
  C'est la règle « plus fragile près des courants » du doc 04 §4.1, enfin réelle.
* Le courant porte un joueur tombé sous la glace : aucune règle nouvelle, c'est 6.3.
* `is_hole` et les lignes de pêche : inchangés, à `wet` près.

### 7.2 Surcouche saisonnière (`norrcraft/seasonal.rs`)

* `SeasonBlocks::natural` (ligne 123) n'est pas modifié : de l'eau libre arrivée dans une colonne
  rend la colonne non naturelle, elle garde donc son état (pas de neige posée sur un ruisseau
  détourné par le joueur). L'eau générée en régime permanent est identique à la base : la colonne
  reste naturelle.
* `Chunk::replace_voxels` assainit la couche fluide (2.1) : une cellule devenue glace perd son
  octet.
* **Cascades gelées** : dans `freeze` (ligne 275), nouveau cas pour les colonnes marquées chute
  par la génération (`WaterColumn::spill` ou cellules de chute d'une structure d'eau, section 8) :
  quand `rules.ice_status(step, site, IceWater::Lake)` rend un rang d'au moins 2, les cellules
  d'eau qui tombent deviennent `isfall`, et la lèvre devient `nyis`. `isfall` est ajouté à
  `SeasonBlocks::output`. Au dégel la surcouche repose l'eau permanente de la base.
* Les sources suivent le pas de saison (4.2) ; `FluidEnv::season_step` vient de
  `step_of(année)` déjà calculé dans `nb-game/src/game/calendar.rs`.
* La débâcle (`is_channel`) ne change pas.

## 8. Génération

Principe : tout ce que la génération pose est déjà dans son état d'équilibre, avec les octets
`STEADY` écrits. Aucun calcul d'écoulement pendant la génération ; la simulation ne s'éveille que
si quelque chose change.

### 8.1 Courant des rivières (`river_flow`)

`crates/nb-world/src/norrcraft/hydro.rs`, nouvelle méthode de `Hydrography` :

```
fn river_flow(&self, wx, wz, c: &WaterColumn, r: &Relief, m: &MacroSample) -> [f32; 2]:
    if c.kind != River: return [0, 0]
    // 1. Tangente du lit : perpendiculaire au gradient du bruit de rivière (mêmes différences
    //    finies que `river_distance`, ligne 363), ou axe de l'auge dans la chaîne.
    (gx, gz) = gradient de Rv en (wx, wz)
    t = normalize(−gz, gx)
    // 2. Sens : vers l'aval, c'est-à-dire vers le relief filtré décroissant.
    hl_a = filtered_height(relief(wx + 6·t.x, wz + 6·t.z))
    hl_b = filtered_height(relief(wx − 6·t.x, wz − 6·t.z))
    if hl_a > hl_b: t = −t
    // 3. Vitesse : plus vive quand le lit est étroit et à l'approche d'un seuil.
    (w, frac) = stair_level(hl, sea)
    v = 0,8 + 1,2 · smooth(7, 2,5, c.river_width)
    if w > sea && frac < 0,30: v += 2,6 · smooth(0,30, 0,0, frac)     // on accélère vers la lèvre
    if frac > 0,85:            v += 0,8                                // bassin de chute agité
    return t × min(v, 4,5)
```

Le module `norrcraft` interdit `sin`, `cos`, `powf` (test
`norrcraft_module_has_no_transcendental_calls`) : `normalize` passe par `sqrt`, ce qui est
permis. Les deux reliefs supplémentaires ne sont calculés que pour les colonnes `River`.

### 8.2 Chutes des marches de rivière

Aujourd'hui, à chaque marche de l'escalier d'eau (6 blocs, `STAIR_STEP`), le scellement monte la
première colonne basse en barrage plein (`seal`, ligne 161). Dans le lit d'une rivière, ce
barrage devient un **seuil déversant**.

`WaterColumn` (ligne 114) gagne `pub spill: bool`.

`seal` reçoit en plus la nature des voisines (elle les a déjà) :

```
pour chaque voisine n mouillée dont n.level > own:
    si c.kind == River et n.kind == River:
        // seuil : la colonne s'arrête un bloc sous la surface amont
        top = max(top, n.level − 1) ; spill = true
    sinon:
        top = max(top, n.level)                    // barrage, comme avant
```

Une colonne `spill` a `top = niveau amont − 1` et n'est plus mouillée : sa première cellule libre
est à la hauteur de la dernière cellule d'eau de l'amont (le lit amont a un bloc d'eau au-dessus
de la barre rocheuse, `STEP_SILL`).

`NorrcraftTerrain::generate_base` (`terrain.rs:531`), après la boucle des colonnes et avant les
arbres, nouvelle passe `stamp_spills(chunk, cols, x0, z0)` :

```
pour chaque colonne c du chunk avec c.spill:
    y = c.top
    aval = voisines (4) mouillées, River, de niveau < y       // dans la bordure 18 × 18 de `cols`
    dir = direction vers la première voisine aval (ordre +X, −X, +Z, −Z) ; rien si aucune
    chunk.set_block(x, y, z, water)
    chunk.set_fluid_code(x, y, z, explicit(3, dir, steady = true))        // lame sur la lèvre
pour chaque colonne c2 du chunk, mouillée, River, ayant une voisine s avec s.spill et s.top >= c2.level:
    pour y dans c2.level ..= s.top:
        chunk.set_block(x, y, z, water)
        chunk.set_fluid_code(x, y, z, explicit(4, Dir::Down, steady = true))   // nappe qui tombe
```

Géométrie obtenue : rivière amont, lèvre d'un bloc de large couverte d'une lame d'eau, rideau de
5 à 6 blocs de haut sur toute la largeur du lit (5 à 14 blocs), bassin surcreusé déjà présent
(`PLUNGE_FROM`). Les berges restent des barrages pleins : l'eau ne peut pas s'échapper sur les
côtés. La rivière amont et la rivière aval sont infinies : la chute est stable sans calcul.

Conséquences à traiter dans la même session :

* `tests/norrcraft_hydro.rs` vérifie qu'aucune cellule d'eau n'a d'air pour voisine latérale :
  exempter les cellules dont l'octet fluide est explicite (eau qui coule) et ajouter le test
  inverse (toute cellule d'eau **sans** octet explicite reste étanche).
* `seasonal::pristine` compare des voxels : la base régénérée contient les mêmes cellules, rien à
  faire. `reseason` (`terrain.rs:775`) doit recopier la couche fluide de la base dans `next`
  pour les colonnes naturelles, et garder celle de `chunk` pour les autres.
* Les empreintes `FINGERPRINTS` de `tests/norrcraft_relief.rs` changent (le test affiche les
  nouvelles valeurs).
* `worldmap` : nouveau mode `eau` (lèvres en blanc, sources en rouge, tracés en cyan).

### 8.3 Ouvrages d'eau par la grille de structures

Les cascades de paroi, les sources et leurs ruisseaux traversent les limites de chunks : ils
passent par le mécanisme qui sait déjà le faire, `norrcraft/structures.rs` (grilles, prédicats de
site, plan par chunk mis en cache, portée 48) et `norrcraft/buildings.rs` (pièces en coordonnées
absolues).

Nouveaux `StructKind` (classe `Minor`, pas 32, `built = true`, `track = false`), et
`StructKind::COUNT` passe de 33 à 37 :

| Type | Lieu | Biomes et poids par cellule | Condition |
| --- | --- | --- | --- |
| `Vattenfall` (cascade) | sommet d'une paroi | Fjord 0,30 ; Kalfjäll 0,10 ; Fjällhed 0,08 ; Fjällbjörkskog 0,06 | `r.hanging > 0,3` ou dénivelé d'au moins 10 à 3 colonnes ; tracé de dénivelé total ≥ 8 |
| `Kalla` (source et ruisseau) | versant | forêts et landes 0,05 | tracé de dénivelé 2 à 20, longueur ≤ 48 |
| `Kallsprang` (résurgence) | pied d'une paroi calcaire | provinces de `kalksten` 0,12 | roche de la province = `kalksten`, eau à moins de 24 blocs |
| `Jokel` (torrent glaciaire) | front d'un glacier | Glaciär et ses bords 0,25 | cellule de glace voisine d'une colonne sans glace |

Nouveau fichier `crates/nb-world/src/norrcraft/waterworks.rs` :

```rust
pub struct Trace { pub cells: Vec<TraceCell>, pub end: RestKind }
pub struct TraceCell { pub x: i32, pub y: i32, pub z: i32, pub dir: Dir, pub fall_from: i32 }

/// Tracé pur de l'eau depuis `(x0, z0)` : fonction des seules sondes de colonnes.
pub fn trace(probe: &SiteProbe, x0: i32, z0: i32, max_len: usize) -> Option<Trace>
```

```
fn trace(probe, x0, z0, max_len):
    (x, z) = (x0, z0) ; y = surface(x, z) ; cells = []
    répéter au plus max_len fois:
        wc = water(x, z)
        if wc.is_wet(): return Some(Trace { cells, end: kind(wc) })       // nappe atteinte
        // hauteur de remplissage d'une voisine : sa nappe si elle est mouillée, sinon son sol
        best = voisine (ordre +X, −X, +Z, −Z) de fill minimal
        if fill(best) > y: return None                                    // cuvette : pas d'ouvrage ici
        if fill(best) == y et déjà visité 4 cellules de suite sans descendre: return None   // plat sans issue
        cells.push(TraceCell { x, y, z, dir: vers best, fall_from: y })
        y_new = fill(best)
        if y − y_new >= 2:                                                // ressaut : l'eau tombe dans la colonne de best
            pour yy dans y_new .. y: cells.push(TraceCell { x: best.x, y: yy, z: best.z, dir: Down, fall_from: y })
        (x, z, y) = (best.x, best.z, y_new)
    None
```

`buildings.rs` : `Mat` (ligne 24) gagne `Spring`, `Water { level: u8, dir: Dir, steady: bool }`
et `Bed` (lit du ruisseau : le bloc de sol existant est retiré). Une fonction
`waterwork(site, probe, kind) -> Option<Building>` ajoutée à `build` (ligne 587) produit une
seule `Part` :

* le bloc `Spring` à `(x0, surface − 1, z0)` (pour `Kallsprang` : dans la paroi, à la base) ;
* pour chaque cellule plate du tracé : `Bed` à `y − 1` remplacé par un creux d'un bloc (le lit
  fait un bloc de large et un bloc de profond, ce qui force l'eau simulée à suivre le tracé),
  puis `Water { level: 3, dir, steady: true }` dans ce creux ;
* pour chaque cellule de ressaut : `Water { level: 4, dir: Down, steady: true }` ;
* au pied de tout ressaut d'au moins 6 blocs : cuvette de 3 × 3 sur 2 de profondeur, remplie
  d'eau pleine au repos (le bassin déborde vers la suite du tracé).

`Structures::stamp_building` (`structures.rs:1642`) écrit `Mat::Water` par
`chunk.set_block(.., water)` puis `chunk.set_fluid_code(..)`, et `Mat::Spring` par le bloc
`kalla`. `TreeProbe::blocked` écarte déjà les arbres des emprises.

Le débit des sources (4.2) est réglé pour que l'état écrit soit proche du point fixe de la
simulation ; s'il ne l'est pas exactement, le premier réveil le corrige en quelques pas.

### 8.4 Paysages d'eau

Tout vient de la combinaison de ce qui précède avec les formes existantes :

| Paysage | Comment |
| --- | --- |
| Cascades de fjord et de vallée suspendue | `Vattenfall` sur `r.hanging` et `r.fjord` |
| Chutes et rapides en gorge | seuils de 8.2 dans les lits étroits ; le courant analytique monte à 4,5 à l'approche de la lèvre |
| Lacs en escalier | deux lacs de vallée (fingersjöar) de marches voisines reliés par une rivière : la marche devient une chute |
| Résurgences karstiques | `Kallsprang` |
| Torrents glaciaires | `Jokel` ; teinte laiteuse par la teinte d'eau du biome Glaciär dans `assets/norrcraft/biomes.json` |
| Tourbières qui suintent | 4.3 |
| Deltas | hors périmètre de cette étape (voir 17) |

Plantes à la génération (`norrcraft/flora.rs`, table `SPECS`, ligne 186 et suivantes) : ajouter
`fraken`, `nate`, `algras`, `fingertang` avec leurs `Habitat::Bed` (eau, profondeurs du tableau
5.2), et `nackmossa` avec un nouveau champ de `Column` `running: bool` (colonne `River` dont
`river_flow` dépasse 0,8). `fingertang` s'empile : `place_understory` (`terrain.rs:665`) pose de
1 à 6 blocs selon la profondeur et un hachage de la colonne.

## 9. Rendu

### 9.1 Mailleur

Le chemin actuel doit rester identique bit à bit sans eau vivante (les tests `golden.rs`
comparent aux maillages du Java). `mesh_chunk` (`nb-mesh/src/mesher.rs:192`) garde sa signature
et appelle la nouvelle fonction avec `None`.

```rust
pub struct FluidMesh<'a> {
    /// Couches fluides des 9 chunks du voisinage (même index que `Neighborhood`).
    pub layers: [Option<&'a [u8]>; 9],
    /// Niveau par défaut par identifiant de bloc (8 pour l'eau et les plantes aquatiques).
    pub default_level: &'a [u8; 256],
    /// Blocs qui peuvent porter de l'eau.
    pub holds: &'a [bool; 256],
    /// Courant de nappe par colonne du chunk central, seizièmes de bloc/s.
    pub column_flow: Option<&'a [[i8; 2]; 256]>,
    /// Nature de nappe par colonne (pour savoir si une cellule pleine au repos est une rivière).
    pub column_rest: Option<&'a [(u8, u8, u8); 256]>,   // (top, level, kind)
}

pub fn mesh_chunk_fluid(pos, n, blocks, biomes, fluid: Option<&FluidMesh>) -> MeshOutput
```

Avec `fluid == Some(..)` :

1. Les passes gloutonnes (`slice_y`, `slices_xz`) ignorent les cellules de rôle `Water` comme
   bloc courant (elles restent des voisines transparentes pour les faces des blocs solides).
2. Nouvelle passe `emit_fluids` (nouveau fichier `nb-mesh/src/fluid.rs`), pour chaque cellule de
   niveau `l > 0` du chunk central :

```
above = niveau de la cellule du dessus > 0
falling = code.dir == Down, ou (cellule du dessous ouverte et non pleine)
// Hauteur des 4 coins (partagée avec les cellules voisines : la surface est continue).
pour chaque coin k, parmi les 4 cellules qui le touchent au même y :
    si l'une a de l'eau au-dessus d'elle : hc[k] = 1,0
    sinon : hc[k] = moyenne de H(niveau) sur celles qui portent de l'eau       // H(l) = l / 8 × 0,85
si above ou falling : hc = [1,0 ; 4]
flow = courant horizontal de la cellule (9.2)
drapeaux k : bit 0 = houle (cellule pleine, au repos, flow nul) ; bit 1 = nappe qui tombe ; bit 2 = eau vivante (toujours)
dessus  : émis si !above ; quad aux hauteurs y + hc[k]
côtés   : pour chaque voisine horizontale : rien si elle est opaque ou porte de l'eau ; sinon quad de y à y + hc des deux coins
dessous : émis si la cellule du dessous est ouverte et sans eau
```

`H(8) = 0,85` : c'est la surface actuelle (`FLUID_DROP = −0,15`, ligne 115), une nappe au repos a
donc exactement l'aspect d'aujourd'hui. Lumière : `face_light` du dessus pour toute la cellule ;
pas d'occlusion ambiante sur l'eau ; teinte `tints.water` comme aujourd'hui.

3. Une cellule perméable immergée est dessinée deux fois : son bloc par les passes habituelles,
   son eau par `emit_fluids`.

Encodage dans le sommet (36 octets, `nb-mesh/src/vertex.rs`, inchangé en taille) :

* `uv` porte le **courant** `(flow.x, flow.z)` en blocs par seconde pour les sommets d'eau
  vivante ;
* `layer` porte les drapeaux dans sa partie fractionnaire : `layer = −(couche + 1) − k / 8`
  avec `k` de 0 à 7. `LayerCode::decode` tronque : la couche décodée ne change pas, et
  `is_fluid()` (`layer < 0`) reste vrai. Un sommet d'eau ancien a `k = 0` : les shaders le
  traitent comme aujourd'hui.

Branchements :

* `Neighborhood` n'est pas modifié ; `FluidMesh` voyage à côté.
* `nb-mesh/src/scheduler.rs` : le trait de la ligne 29 gagne
  `fn fluid_snapshot(&self, p: ChunkPos) -> Option<Arc<[u8]>> { None }` et
  `fn fluid_tables(&self) -> Option<FluidTables>` (niveau par défaut, `holds`, et un accès à la
  table d'eau du chunk) ; le travail (ligne 100) construit `FluidMesh` quand ils existent.
* `nb-game/src/chunks.rs:34` les implémente depuis `ChunkManager` (copie de la couche sous le
  même verrou de lecture que les voxels) et depuis `World` (table d'eau, mise en cache).

### 9.2 Courant affiché

Même règle que 6.2, réduite à l'horizontale et calculée par le mailleur à partir des couches du
voisinage : direction `STEADY` × vitesse, sinon courant de nappe de `column_flow` pour une
cellule pleine sans octet dans une colonne `River`, sinon gradient des niveaux voisins.

### 9.3 Shaders (`crates/nb-gfx/shaders/glsl450`, SPIR-V recompilés et versionnés dans `spv/`)

`main.vert` :

```glsl
if (aLayer < 0.0) {
    float a = -aLayer - 1.0;
    actualLayer = floor(a + 0.01);
    int k = int(round(fract(a + 0.01) * 8.0));
    fluid = 1.0;
    bool living = (k & 4) != 0;
    bool swell = living ? ((k & 1) != 0) : (pos.y - floor(pos.y) > 0.7);
    if (swell) { /* houle actuelle, inchangée */ }
    FluidFlags = float(k);          // nouvelle sortie, location 7
}
```

`fluid.frag` :

* `TexCoord` n'est plus une coordonnée de texture pour l'eau vivante : quand le bit 2 est posé,
  les coordonnées de texture viennent de `FragPos.xz` (la lave garde `TexCoord`, lignes 188 et
  220).
* **Normales advectées** par le courant, technique à deux phases :

```glsl
vec2 flow = TexCoord;                       // blocs/s
float sp = length(flow);
float ph0 = fract(uTime * 0.5), ph1 = fract(uTime * 0.5 + 0.5);
vec3 n0 = waveNormal(FragPos.xz - flow * ph0 * 2.0);
vec3 n1 = waveNormal(FragPos.xz - flow * ph1 * 2.0);
vec3 n = normalize(mix(n0, n1, abs(ph0 * 2.0 - 1.0)));
n.xz *= mix(1.0, 2.2, clamp(sp / 3.0, 0.0, 1.0));     // l'eau vive est plus hachée
```

* **Écume** : `foam = smoothstep(1.2, 3.5, sp) * vnoise(FragPos.xz * 3.0 - flow * uTime)` ajoutée
  au liseré d'écume existant ; sur une face « nappe qui tombe » (bit 1), stries verticales
  blanches `vnoise(vec2(FragPos.x + FragPos.z, FragPos.y * 0.35 + uTime * 2.4))`, opacité 0,75,
  et pas de réfraction.
* Le reste (Fresnel, absorption, reflets) ne change pas.

`shadow.vert` et `main.frag` ne changent pas. Le dossier `shaders/gl330` est la référence du
Java : ne pas y toucher.

## 10. Particules et son

### 10.1 Repérage des chutes et des eaux vives

Nouveau `crates/nb-game/src/game/water_fx.rs` : `WaterScout`. À chaque image, il examine un
huitième des colonnes d'un carré de 48 × 48 autour de la caméra (288 colonnes par image, tout le
carré en 8 images), par `read_column` et la couche fluide du chunk :

```rust
pub struct FallFoot { pub pos: Vec3, pub height: f32 }      // pied d'une chute : première cellule non tombante sous une série de cellules qui tombent
pub struct WaterScene { pub feet: Vec<FallFoot>, pub stream: f32, pub roar: f32 }
```

* `roar = clamp(Σ height_i / (1 + d_i² / 64), 0, 1)` ;
* `stream = clamp(nombre de cellules de surface à courant > 0,5 dans un rayon de 12 / 40, 0, 1)`.

Sur un invité LAN le repérage tourne de la même façon (il lit sa copie des couches).

### 10.2 Particules (`nb-particles/src/presets.rs`, `nb-game/src/fx.rs`)

* Nouveau préréglage `mist` (brume de cascade : grosses particules claires, lentes, montantes,
  durée 2,5 s, alpha 0,25), ajouté à `by_name` (ligne 346) et à `assets/particles.json`.
* Par pied de chute visible et par seconde : `2 + 0,8 × height` particules `mist` dans un rayon
  de `1 + height / 6`, `4 × largeur` particules `foam` au pied, quelques `splash`.
* Trempe (R5) : `smoke` et son de vapeur.
* Plafond : 220 particules d'eau vives à la fois, les pieds les plus proches d'abord.

### 10.3 Son (`nb-audio/src/ambience.rs`, `nb-game/src/audio.rs`)

`AmbienceTargets` (ligne 13) gagne `falls` et `stream` ; `as_array` et `Ambience::levels`
passent de 7 à 9 couches.

* **Chute** : bruit brun passe-bas 400 Hz + bruit blanc passe-bande 1 à 4 kHz à 30 %, gain
  `roar`. Sous l'eau : passe-bas 250 Hz.
* **Ruisseau** : bruit rose passe-bande 800 à 3 000 Hz, modulé en amplitude par un bruit lent
  (2 à 5 Hz) pour le clapotis, gain `stream × 0,6`.
* `nb-game/src/audio.rs` (vers la ligne 675, où `amb.water` est réglé) lit `WaterScene`.
* Événements ponctuels : `AnimalAct::Leap` (éclaboussure), dévalement d'un bateau, trempe.

Tout reste procédural, comme le reste de l'audio.

## 11. Sauvegarde

`crates/nb-world/src/storage.rs`, format `NBCK` :

* **Version 2**, écrite seulement quand le chunk a une couche fluide non nulle ; sinon le fichier
  reste en version 1, octet pour octet comme aujourd'hui (les mondes vanilla ne changent pas).
* Après la charge utile des voxels : `u8` présence (1), `u32` longueur, puis la couche compressée
  par le même RLE (`rle_encode`).
* `decode_chunk` accepte les versions 1 et 2 et renvoie
  `ChunkBlob { voxels: Voxels, fluid: Option<Box<[u8; CHUNK_VOLUME]>> }`.
* Trait `ChunkStorage` (ligne 38) : `save_chunk(pos, voxels, fluid: Option<&[u8; CHUNK_VOLUME]>)`
  et `load_chunk(pos) -> Result<Option<ChunkBlob>>`. Mettre à jour `InMemoryChunkStorage`,
  `FileChunkStorage`, `Inner::save`, `save_pending`, `load_job`, `voxels_of` (qui devient
  `blob_of`), `install_chunk`.
* Les émetteurs sont des blocs, la file d'éveil se reconstruit au chargement (3.7) : aucun état
  de simulation dans `world.json`.

## 12. Réseau local

L'hôte est seul maître de l'eau, comme de l'heure et des mobs.

`crates/nb-net/src/proto.rs` : `PROTOCOL_VERSION = 5`.

```rust
// Serveur → client
ServerMsg::Fluid { cells: Vec<([i32; 3], u8 /*bloc*/, u8 /*octet fluide*/)> }    // étiquette 114
// Client → serveur
ClientMsg::FluidEdit { pos: [i32; 3], amount: i8 }      // +n : verser n h ; −8 : puiser ; étiquette 8
```

* `ServerMsg::Chunk` : `data` est le fichier de chunk version 2 quand il y a une couche
  (`encode_chunk` prend la couche en paramètre).
* `HostNet::post_tick` (`nb-game/src/net/host.rs:738`) : nouvelle étape `send_fluid_deltas` après
  `send_block_changes`. Elle prend `world.take_fluid_deltas()`, regroupe par chunk, et pour
  chaque connexion envoie les cellules des chunks qu'elle connaît (`c.known`), par messages de
  1 024 cellules au plus. Un chunk qui a plus de 256 cellules changées dans le pas n'est pas
  envoyé en deltas : il est retiré de `known` et repart en entier au prochain `sync_chunks`.
* La simulation n'écrit jamais par `set_block` : le journal d'éditions ne voit pas ses
  écritures, il n'y a donc pas de `BlockChange` en double.
* Client (`nb-game/src/net/client.rs`) : `ServerMsg::Fluid` → nouvelle
  `ChunkManager::set_fluid_quiet(x, y, z, id, code)` (écrit, marque le maillage). `on_chunk`
  installe voxels et couche.
* `ClientMsg::FluidEdit` : validé par l'hôte comme `on_block_edit` (portée, chunk présent), puis
  `pour` ou `scoop`.
* Un invité ne simule pas, mais il calcule ses courants : la couche fluide lui donne niveaux et
  directions, et sa copie du générateur (même graine) lui donne la table d'eau. Sa physique de
  joueur, locale comme aujourd'hui, subit donc les mêmes courants que chez l'hôte.
* `Schedule::client()` (`nb-ecs/src/schedule.rs:220`) n'a rien à gagner : `PlayerInputSystem`
  y est déjà.
* Débit : une chute stable n'émet rien (régime permanent). Un lac qui se vide émet au plus
  `CELLS_PER_TICK × 60` cellules par seconde chez l'hôte, mais le seuil de 256 par chunk borne
  l'envoi à un chunk complet (quelques kilo-octets compressés) par pas et par chunk agité.

## 13. Budgets de performance

Mesurés en profil release sur la machine de Gabriel, vérifiés par des tests de budget en profil
dev avec un facteur 6 (comme `brain.rs`).

| Poste | Budget | Mesure |
| --- | --- | --- |
| Monde au repos (aucune cellule éveillée) | < 5 µs par tick | `FluidStats::last_tick_us` |
| Tick chargé (512 cellules) | ≤ 0,40 ms, moyenne ≤ 0,25 ms | test `fluid_budget` |
| Recherche de pression | ≤ 3 µs l'une, 48 par tick | compris ci-dessus |
| Table d'eau d'un chunk | ≤ 0,5 ms, une fois par chunk simulé | test `water_table_budget` |
| Mémoire | 32 Ko par chunk ayant une couche ; ≤ 12 Mo pour 25 × 25 chunks dont 60 % en ont une | compteur dans `/water info` |
| Maillage d'un chunk d'eau | ≤ +15 % par rapport à aujourd'hui | banc `nb-game/benches/pipeline.rs` |
| Remaillages dus à l'eau | au plus une fois par pas d'eau et par chunk (5 par seconde) | par construction (3.6) |
| Génération | ≤ +0,15 ms par chunk (seuils et ouvrages) | test de budget du sous-sol existant |
| Repérage sonore et particules | ≤ 40 µs par image | `NB_PERF=1` |
| Réseau | ≤ 20 Ko/s par invité dans une scène agitée | compteur du transport |

Parades prévues si un budget est dépassé : baisser `CELLS_PER_TICK`, réduire `SIM_RADIUS`,
porter `STEP_TICKS` à 15.

## 14. Données : `assets/norrcraft/water.json`

Lu par un nouveau `crates/nb-data/src/water.rs` (`WaterRules::builtin()` statique, comme
`BehaviorBook`). Rappel du piège de la 3.2 : pas de clé `_doc` dans une table typée.

```json
{
  "stepTicks": 12, "cellsPerTick": 512, "timeCapMicros": 400,
  "pressureSearchesPerTick": 48, "pressureNodes": 96,
  "spreadMax": 4, "seekRadius": 5, "calmSteps": 6,
  "simRadius": 6, "bootWakeMax": 2048,
  "speeds": { "fall": 6.0, "film": 2.6, "stream": 1.6, "gradient": 0.4, "maxFree": 3.0,
              "plungePull": 4.0, "swirl": 1.0, "rapid": 3.2 },
  "springs": {
    "mountain": { "rate": 3.0, "season": [0.3, 0.5, 1.6, 1.8, 1.0, 0.8, 0.7, 0.4] },
    "karst":    { "rate": 4.0, "season": [0.8, 0.9, 1.3, 1.2, 1.0, 0.9, 0.9, 0.8] },
    "glacier":  { "rate": 5.0, "season": [0.0, 0.0, 0.6, 1.4, 2.0, 1.6, 0.6, 0.1] }
  },
  "weather": { "steps": 10, "rainSamples": 12, "puddleMax": 2, "freezeBelow": -2.0, "thawAbove": 2.0,
               "rechargeSteps": 50, "rechargeSamples": 16, "seepSteps": 40, "seepSamples": 8 },
  "player": { "wade": 0.4, "rockHit": 0.5, "upstreamDrain": 4.0 },
  "boats": { "draught": 5, "fallWater": 0.12, "raftBreaks": 4, "anchorHolds": 3.0 },
  "salmon": { "leapMax": 6, "leapChance": 0.35, "leapEvery": 3.0 },
  "plants": {
    "fraken":     { "water": ["fresh"], "depth": [1, 1], "current": [0.0, 0.3] },
    "nate":       { "water": ["fresh", "brackish"], "depth": [1, 4], "current": [0.0, 1.0] },
    "nackmossa":  { "water": ["fresh"], "depth": [1, 2], "current": [0.8, 9.0] },
    "algras":     { "water": ["brackish", "salt"], "depth": [1, 5], "current": [0.0, 1.0] },
    "fingertang": { "water": ["salt"], "depth": [3, 14], "current": [0.0, 9.0] },
    "nackros":    { "water": ["fresh"], "depth": [1, 3], "current": [0.0, 0.3] },
    "vass":       { "water": ["fresh", "brackish"], "depth": [1, 2], "current": [0.0, 1.0] },
    "kaveldun":   { "water": ["fresh"], "depth": [1, 1], "current": [0.0, 0.5] }
  }
}
```

## 15. Tests

`crates/nb-world/tests/fluid_rules.rs` (sur `MemFluid`, sans chunks) :

1. **Conservation** : 500 h versés dans une cuve fermée de forme quelconque, 2 000 pas : la somme
   des niveaux vaut toujours 500.
2. **Repos** : la même cuve finit avec une file vide ; plus aucune écriture ensuite.
3. **Égalisation** : un bassin plat de 8 × 8 finit avec des niveaux qui diffèrent d'au plus 1
   entre deux cellules quelconques ; un canal d'un bloc de large et de 40 de long aussi.
4. **Chute** : une colonne d'air de 10 sous une cellule pleine est traversée en 10 pas, sans
   cellule vide entre deux cellules d'eau une fois le filet établi.
5. **Bord** : une flaque au bord d'une falaise s'y vide entièrement.
6. **Vases communicants** : tube en U, un bras plein : les deux surfaces finissent à 1 h près.
7. **Jamais plus haut** : l'eau ne dépasse pas la charge de sa surface de départ.
8. **Film** : 1 h posé à 4 cellules d'un bord l'atteint ; sans bord, il ne bouge pas.
9. **Nappe infinie** : un canal creusé depuis la mer se remplit au niveau de la mer et s'endort ;
   10 000 h versés dans la mer disparaissent.
10. **Lac fini** : un lac percé par le fond perd exactement ce qui sort par le trou.
11. **Régime permanent** : source infinie, chute, puits infini : toutes les cellules passent
    `STEADY` en moins de 40 pas et la file se vide ; casser un bloc sous la chute la réveille.
12. **Blocs** : un bloc `Washable` est emporté, un `Permeable` se remplit et garde son voxel, une
    plante `Aquatic` sans octet compte pour 8.
13. **Bord de chunk** : l'eau s'arrête contre un chunk non chargé sans rien perdre, puis repart.
14. **Trempe** : eau contre lave → `basaltsten`.
15. **Déterminisme** : deux exécutions à même graine donnent les mêmes octets.
16. **Budget** : 512 cellules en moins de 0,4 ms × 6 en profil dev.

Autres fichiers :

* `nb-world/tests/fluid_storage.rs` : aller-retour version 2, lecture d'un fichier version 1,
  fichier version 1 quand la couche est vide, `replace_voxels` qui assainit.
* `nb-world/tests/norrcraft_water.rs` : table d'eau égale aux colonnes générées ; courant de
  rivière orienté vers l'aval sur 3 graines ; chaque seuil a sa lame et son rideau ; tracés purs
  (même résultat depuis deux chunks voisins) ; présence des 4 ouvrages à ±4 km sur 3 graines ;
  cascade gelée au cœur de l'hiver, rendue au dégel.
* `nb-mesh/tests/fluid_mesh.rs` : sans `FluidMesh`, maillage identique à `mesh_chunk` (et
  `golden.rs` inchangé) ; surface d'une nappe au repos à `y + 0,85` ; coins partagés entre
  cellules voisines ; drapeaux `k` ; courant dans `uv`.
* `nb-ecs/tests/currents.rs` : joueur immobile emporté à la vitesse du courant ; gué (niveau 2)
  marché sans nager ; rapide qui blesse contre un rocher ; endurance qui baisse à contre-courant ;
  mob terrestre emporté ; bateau qui dérive, remonte lentement, dévale une chute ; radeau
  disloqué ; saumon qui remonte et franchit une chute de 6, bloqué par une chute de 7 ; glace
  fragile au-dessus d'un courant.
* `nb-net` : aller-retour de `ServerMsg::Fluid` et `ClientMsg::FluidEdit` dans les tests
  existants de `proto.rs`.
* `nb-game/tests/universe.rs` : un invité reçoit la couche fluide d'un chunk et un delta.
* `nb-game/tests/sealed.rs` : vanilla ne voit aucun des nouveaux noms.

Aperçus (`preview_norrcraft`, le visuel change) : une chute de rivière, une cascade de fjord,
une source et son ruisseau, la même cascade gelée (`NB_NORR_SEASON=0`), dans
`apercu-norr/6E.2/` et `apercu-norr/6E.4/`.

## 16. Découpage en sessions

Quatre sessions Sonnet, en série (la 6E.2 et la 6E.3 peuvent tourner en parallèle après la 6E.1,
elles ne se croisent que dans `nb-game/src/session.rs`). Méthode du `CLAUDE.md` : un worktree par
session, cache Docker partagé, tests de la seule crate touchée pendant le travail, une passe
complète avant la fusion, exécutable seulement à la fin de la 6E.4.

### 6E.1 Simulation et stockage

| Fichier | Travail |
| --- | --- |
| `nb-data/src/blocks.rs` | `FluidKind`, champ `fluid` (2.2) |
| `nb-data/src/water.rs` (nouveau), `lib.rs` | `WaterRules` (14) |
| `assets/norrcraft/water.json` (nouveau), `blocks.json`, `items.json` | règles ; blocs 177 à 179 ; `fluid` des blocs existants ; `hink`, `hink_vatten` |
| `nb-world/src/chunk.rs` | couche fluide, `replace_voxels` assainissant, `ChunkSnapshot` (2.1) |
| `nb-world/src/storage.rs` | format version 2, `ChunkBlob` (11) |
| `nb-world/src/manager.rs` | `fluid_cell`, `write_fluid`, `water_level`, `FluidEvents`, appelants du stockage (2.4, 11) |
| `nb-world/src/profile.rs` | `Systems::living_water`, `TerrainGenerator::water_table` (2.3) |
| `nb-world/src/norrcraft/terrain.rs`, `hydro.rs` | `water_table` et `river_flow` (2.3, 8.1) |
| `nb-world/src/fluid/*` (nouveau) | couche, classes, table, simulation, règles, sources, `scoop`, `pour` (3, 4) |
| `nb-world/src/world.rs` | `FluidSim` dans `World`, `tick_fluids`, `water_level`, `current_at`, `take_fluid_deltas` |
| `nb-world/src/fluid/flow.rs` | `current_at`, `flow_power` (6.2, 6.7) |
| `nb-game/src/session.rs`, `game.rs` | appel de `tick_fluids`, commande `/water` (4.7) |
| Tests | `fluid_rules.rs`, `fluid_storage.rs`, début de `norrcraft_water.rs` |
| Doc | `docs/EAU_VIVANTE.md` (sections modèle et simulation) |

Livré : l'eau coule, se conserve, s'endort, se sauvegarde. Le rendu est encore l'ancien : une
cellule partielle s'affiche comme un bloc d'eau plein.

### 6E.2 Rendu, son et réseau

| Fichier | Travail |
| --- | --- |
| `nb-mesh/src/fluid.rs` (nouveau), `mesher.rs`, `vertex.rs`, `scheduler.rs` | `FluidMesh`, `mesh_chunk_fluid`, `emit_fluids`, drapeaux `k` (9.1, 9.2) |
| `nb-game/src/chunks.rs` | instantanés de couche, tables pour le mailleur |
| `nb-gfx/shaders/glsl450/main.vert`, `fluid.frag`, `spv/` | 9.3, recompilation par `vk-docker.sh` |
| `nb-gfx/src/vk/pipelines.rs` | sortie `FluidFlags` (location 7) entre les deux étages |
| `nb-particles/src/presets.rs`, `assets/particles.json` | `mist` (10.2) |
| `nb-game/src/game/water_fx.rs` (nouveau), `fx.rs`, `audio.rs` | `WaterScout`, particules, niveaux sonores (10) |
| `nb-audio/src/ambience.rs`, `synth.rs` | couches `falls` et `stream` |
| `nb-net/src/proto.rs` | protocole 5, `Fluid`, `FluidEdit` (12) |
| `nb-game/src/net/host.rs`, `client.rs` | deltas, chunks avec couche, `set_fluid_quiet` |
| Tests | `fluid_mesh.rs`, aller-retour réseau, `universe.rs` |
| Aperçus | `apercu-norr/6E.2/` |

### 6E.3 Courants et vivant

| Fichier | Travail |
| --- | --- |
| `nb-ecs/src/blocks.rs`, `physics.rs` | `fluid_level`, `current`, aides de `BlockEnv`, advection des mobs (6.1, 6.4) |
| `nb-ecs/src/player.rs`, `survival.rs`, `components.rs` | gué, nage, rapides, contre-courant, `Floating` (6.3, 6.4) |
| `nb-ecs/src/boat.rs` | tirant d'eau, courant, dévalement, ancre (6.5) |
| `nb-ecs/src/ice.rs` | glace fragile sur le courant (7.1) |
| `nb-ecs/src/brain/mod.rs`, `nb-data/src/behavior.rs`, `behaviors.json` | remontée et saut des saumons (6.6) |
| `nb-world/src/herds.rs` | sites de harde en rivière |
| `nb-ecs/src/aquatic.rs` (nouveau) | vie des plantes aquatiques (5.3) |
| `spawn.rs`, `density.rs`, `ai.rs`, `ecology.rs`, `nb-game/src/{audio,fx,anim,render}.rs` | `wet` à la place de `== water` (6.1) |
| `nb-game/src/audio.rs` | nouveaux `EcsEvent` et `AnimalAct::Leap` (le `match` est exhaustif) |
| Tests | `currents.rs` |

### 6E.4 Génération et saisons

| Fichier | Travail |
| --- | --- |
| `nb-world/src/norrcraft/hydro.rs` | `WaterColumn::spill`, `seal` déversant (8.2) |
| `nb-world/src/norrcraft/terrain.rs` | `stamp_spills`, `reseason` avec la couche (8.2) |
| `nb-world/src/norrcraft/waterworks.rs` (nouveau), `structures.rs`, `buildings.rs` | tracé, quatre ouvrages, `Mat::Water`, `Mat::Spring` (8.3) |
| `nb-world/src/norrcraft/flora.rs`, `scripts/gen_norr_water.py` | plantes 180 à 184, textures |
| `nb-world/src/norrcraft/seasonal.rs`, `seasons.rs` | cascades gelées, `isfall` (7.2) |
| `nb-world/src/bin/worldmap` | mode `eau` |
| `assets/norrcraft/biomes.json` | teinte laiteuse du biome glaciaire |
| Tests | `norrcraft_water.rs` complet, adaptation de `norrcraft_hydro.rs`, empreintes de `norrcraft_relief.rs` |
| Aperçus, exécutable | `apercu-norr/6E.4/`, `build-windows.sh` |

## 17. Limites assumées

* **Pas de siphon amorcé** : l'eau ne franchit jamais un point plus haut que la surface qui la
  pousse. Les vases communicants et les conduits en U fonctionnent ; un tuyau qui passe
  par-dessus une crête ne s'amorce pas.
* **Pas d'inertie** : l'eau n'a pas d'élan, une vague ne déferle pas, un barrage rompu se vide
  par étalement rapide et non par une lame.
* **Mer sans marée ni courants**, lacs sans courant ; seuls les rivières, les ouvrages et l'eau
  libre en ont.
* **Deltas, rapides en gradins et méandres** ne sont pas générés : ils demandent de modifier le
  lit sur plusieurs colonnes de façon étanche. Le rapide est rendu par le courant, l'écume et le
  son à l'approche des seuils.
* **Pas d'objets jetés** dans le moteur aujourd'hui : rien à emporter, le composant `Floating`
  est prêt.
* **Formes fines** (dalles, portes, échelles) absentes de Norrcraft : voir la décision E4.
* **Neige et eau** : une couche de neige est emportée sans donner d'eau ; la fonte passe par le
  débit saisonnier des sources.
* **Eau hors du rayon de simulation** (6 chunks) : figée, elle reprend à l'approche d'un joueur.

## 18. Décisions à trancher par Gabriel

| # | Question | Recommandation |
| --- | --- | --- |
| E1 | Les lacs sont-ils infinis comme la mer, ou finis (ils se vident quand on les perce et se rechargent lentement) ? | finis : c'est ce qui rend « percer sous un lac » réel ; mer et rivières restent infinies |
| E2 | Toutes les marches de rivière deviennent des chutes de 5 à 6 blocs sur toute la largeur du lit. Tout garder, ou n'en ouvrir qu'une partie (les autres restent des barres rocheuses) ? | tout ouvrir : c'est le changement le plus visible, et il est stable par construction |
| E3 | Les bateaux peuvent-ils dévaler une chute (dégâts, radeau disloqué) ou sont-ils simplement bloqués à la lèvre ? | dévaler |
| E4 | Blocs de forme fine immergeables (dalles, escaliers, échelles, portes) : les ajouter dans une cinquième session de cette étape, ou attendre un système de formes à part ? | à part : Norrcraft n'a pas encore ces formes, c'est un chantier de mailleur indépendant de l'eau |
| E5 | Force des courants : rapides qui blessent et épuisent (valeurs de 6.3), ou version douce pour commencer ? | valeurs du document, à régler en jeu par `water.json` |
| E6 | Pluie qui forme des flaques et gel de l'eau libre : dans cette étape, ou plus tard ? | dans cette étape, c'est peu de code et ça relie l'eau à la météo et aux saisons |
| E7 | Un monde Norrcraft existant garde ses chunks déjà générés sans chutes ni sources (elles n'apparaissent que dans les chunks nouveaux). Acceptable, ou faut-il une commande de régénération des colonnes encore naturelles ? | acceptable ; la surcouche saisonnière sait déjà distinguer les colonnes naturelles si on veut l'ajouter plus tard |
| E8 | Ordre dans le plan : avant la 6.5 (fraîcheur), ou après ? | avant : la 6.6 (eau motrice) en dépend, et l'eau touche le cœur du moteur, mieux vaut la poser tôt |
