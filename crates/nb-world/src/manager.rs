//! Gestionnaire de chunks asynchrone (`ChunkManager.java`, spec §11).
//!
//! Génération et lumière tournent sur des threads dédiés (`max(1, cœurs/2)` chacun) alimentés
//! par des canaux crossbeam ; chaque thread de génération possède son propre
//! [`WorldGenerator`] (comme le `ThreadLocal` Java), chaque thread de lumière ses tampons.

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, RwLock};
use std::thread::JoinHandle;

use nb_core::glam::Vec3;
use nb_core::{ChunkPos, Result};
use nb_data::{BlockId, BlockRegistry};

use crate::chunk::{Chunk, ChunkEntry, SharedChunk, CHUNK_SIZE_X, CHUNK_SIZE_Z, CHUNK_VOLUME};
use crate::config::WorldConfig;
use crate::generator::WorldGenerator;
use crate::light::{LightContext, LightTable};
use crate::storage::ChunkStorage;

/// Distance de rendu par défaut (en chunks).
pub const DEFAULT_RENDER_DISTANCE: i32 = 12;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// `(int) Math.floor((float) v / 16)` : chunk d'une coordonnée monde (calcul en `f32` du Java).
#[inline]
pub fn chunk_coord(v: i32) -> i32 {
    (v as f32 / CHUNK_SIZE_X as f32).floor() as i32
}

/// File de travaux de chunks servie par **distance croissante au joueur** (au lieu d'un FIFO) :
/// quand le joueur avance, les chunks qu'il voit en premier sont générés / éclairés avant ceux
/// du fond de la zone, quel que soit l'ordre dans lequel ils ont été demandés.
struct PrioQueue {
    /// (tâches en attente, file fermée)
    state: Mutex<(Vec<ChunkPos>, bool)>,
    cv: Condvar,
}

impl PrioQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new((Vec::new(), false)),
            cv: Condvar::new(),
        }
    }

    fn push(&self, p: ChunkPos) {
        let mut g = lock(&self.state);
        if g.1 {
            return;
        }
        g.0.push(p);
        drop(g);
        self.cv.notify_one();
    }

    /// Ferme la file : les tâches restantes sont abandonnées, les threads se réveillent.
    fn close(&self) {
        let mut g = lock(&self.state);
        g.1 = true;
        g.0.clear();
        drop(g);
        self.cv.notify_all();
    }

    /// Attend une tâche et renvoie la plus proche de `center()`. `None` une fois fermée.
    fn pop(&self, center: impl Fn() -> (i32, i32)) -> Option<ChunkPos> {
        let mut g = lock(&self.state);
        loop {
            if g.1 {
                return None;
            }
            if !g.0.is_empty() {
                let (cx, cz) = center();
                let dist = |p: &ChunkPos| {
                    let dx = i64::from(p.x) - i64::from(cx);
                    let dz = i64::from(p.z) - i64::from(cz);
                    dx * dx + dz * dz
                };
                let best =
                    g.0.iter()
                        .enumerate()
                        .min_by_key(|(_, p)| dist(p))
                        .map_or(0, |(i, _)| i);
                return Some(g.0.swap_remove(best));
            }
            g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// File des chunks déchargés à écrire sur disque (thread dédié : l'écriture ne bloque plus le
/// thread principal quand le joueur franchit une limite de chunk).
struct SaveQueue {
    /// (positions à écrire, file fermée)
    state: Mutex<(std::collections::VecDeque<ChunkPos>, bool)>,
    cv: Condvar,
}

impl SaveQueue {
    fn new() -> Self {
        Self {
            state: Mutex::new((std::collections::VecDeque::new(), false)),
            cv: Condvar::new(),
        }
    }

    fn push(&self, p: ChunkPos) {
        let mut g = lock(&self.state);
        if g.1 {
            return;
        }
        g.0.push_back(p);
        drop(g);
        self.cv.notify_one();
    }

    fn close(&self) {
        let mut g = lock(&self.state);
        g.1 = true;
        g.0.clear();
        drop(g);
        self.cv.notify_all();
    }

    fn pop(&self) -> Option<ChunkPos> {
        let mut g = lock(&self.state);
        loop {
            if g.1 {
                return None;
            }
            if let Some(p) = g.0.pop_front() {
                return Some(p);
            }
            g = self.cv.wait(g).unwrap_or_else(|e| e.into_inner());
        }
    }
}

struct Inner {
    render_distance: AtomicI32,
    active: RwLock<HashMap<ChunkPos, SharedChunk>>,
    generating: Mutex<HashSet<ChunkPos>>,
    lighting: Mutex<HashSet<ChunkPos>>,
    storage: Arc<dyn ChunkStorage>,
    /// Chunks déchargés dont l'écriture n'est pas terminée. Ils restent lisibles (rechargement,
    /// `voxels_of`) tant que le fichier n'est pas à jour.
    unsaved: Mutex<HashMap<ChunkPos, SharedChunk>>,
    /// Équivalent du `synchronized (storage)`.
    storage_lock: Mutex<()>,
    player_chunk_x: AtomicI32,
    player_chunk_z: AtomicI32,
    shutdown: AtomicBool,
    light_table: LightTable,
    /// Journal des écritures de blocs (réseau) : `None` = désactivé (solo).
    edit_log: Mutex<Option<Vec<BlockEdit>>>,
    /// Centres supplémentaires `(chunk x, chunk z, rayon)` : joueurs distants d'un hôte LAN.
    extra_centers: Mutex<Vec<(i32, i32, i32)>>,
    /// Monde client : les chunks viennent tous de l'hôte, rien n'est généré ni sauvegardé.
    remote: AtomicBool,
}

/// Une écriture de bloc en coordonnées monde (journal d'éditions, voir
/// [`ChunkManager::enable_edit_log`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockEdit {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub id: BlockId,
}

impl Inner {
    fn active_get(&self, pos: ChunkPos) -> Option<SharedChunk> {
        self.active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&pos)
            .cloned()
    }

    fn player_chunk_xz(&self) -> (i32, i32) {
        (
            self.player_chunk_x.load(Ordering::Relaxed),
            self.player_chunk_z.load(Ordering::Relaxed),
        )
    }

    fn is_active(&self, pos: ChunkPos) -> bool {
        self.active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&pos)
    }

    fn mark_mesh_dirty(&self, pos: ChunkPos) {
        if let Some(c) = self.active_get(pos) {
            c.flags.set_mesh_dirty(true);
        }
    }

    fn mark_neighbors_mesh_dirty(&self, pos: ChunkPos) {
        self.mark_mesh_dirty(ChunkPos::new(pos.x - 1, pos.z));
        self.mark_mesh_dirty(ChunkPos::new(pos.x + 1, pos.z));
        self.mark_mesh_dirty(ChunkPos::new(pos.x, pos.z - 1));
        self.mark_mesh_dirty(ChunkPos::new(pos.x, pos.z + 1));
    }

    /// Tâche périmée : le joueur s'est éloigné de plus de `r + 1` chunks.
    fn is_stale(&self, pos: ChunkPos) -> bool {
        let r = self.render_distance.load(Ordering::Relaxed);
        let px = self.player_chunk_x.load(Ordering::Relaxed);
        let pz = self.player_chunk_z.load(Ordering::Relaxed);
        let far = |cx: i32, cz: i32, r: i32| {
            (i64::from(pos.x) - i64::from(cx)).abs() > i64::from(r) + 1
                || (i64::from(pos.z) - i64::from(cz)).abs() > i64::from(r) + 1
        };
        far(px, pz, r)
            && lock(&self.extra_centers)
                .iter()
                .all(|&(x, z, r)| far(x, z, r))
    }

    fn save(&self, entry: &ChunkEntry) -> Result<()> {
        let chunk = entry.read();
        {
            let _guard = lock(&self.storage_lock);
            self.storage.save_chunk(chunk.pos(), chunk.voxels())?;
        }
        entry.flags.set_dirty(false);
        Ok(())
    }

    /// Écrit sur disque un chunk déchargé (thread de sauvegarde). Abandonné si le chunk a été
    /// rechargé ou remplacé entre-temps (le fichier ne doit jamais régresser).
    fn save_pending(&self, pos: ChunkPos) {
        let Some(entry) = lock(&self.unsaved).get(&pos).cloned() else {
            return;
        };
        let still_pending = |s: &Self| {
            lock(&s.unsaved)
                .get(&pos)
                .is_some_and(|e| Arc::ptr_eq(e, &entry))
        };
        {
            let chunk = entry.read();
            let _guard = lock(&self.storage_lock);
            if !still_pending(self) {
                return;
            }
            if let Err(e) = self.storage.save_chunk(chunk.pos(), chunk.voxels()) {
                eprintln!("échec de sauvegarde du chunk {pos:?} : {e}");
            }
        }
        entry.flags.set_dirty(false);
        let mut unsaved = lock(&self.unsaved);
        if unsaved.get(&pos).is_some_and(|e| Arc::ptr_eq(e, &entry)) {
            unsaved.remove(&pos);
        }
    }

    /// Écrit tous les chunks déchargés en attente, sur le thread appelant.
    fn flush_unsaved(&self) {
        let positions: Vec<ChunkPos> = lock(&self.unsaved).keys().copied().collect();
        for pos in positions {
            self.save_pending(pos);
        }
    }

    fn load_job(&self, pos: ChunkPos, generator: &WorldGenerator) {
        if self.is_stale(pos) {
            return;
        }
        // Chunk déchargé dont l'écriture est encore en attente : sa copie mémoire fait foi.
        let pending: Option<crate::storage::Voxels> = lock(&self.unsaved)
            .get(&pos)
            .cloned()
            .map(|e| Box::new(*e.read().voxels()));
        let stored = if pending.is_some() {
            pending
        } else {
            let _g = lock(&self.storage_lock);
            if self.storage.has_chunk(pos) {
                match self.storage.load_chunk(pos) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("chunk {pos:?} illisible, régénéré : {e}");
                        None
                    }
                }
            } else {
                None
            }
        };
        let chunk = match stored {
            Some(v) => Chunk::from_voxels(pos, v.as_slice()),
            // Monde client : un chunk absent du stockage n'existe pas encore (l'hôte l'enverra).
            None if self.remote.load(Ordering::Acquire) => return,
            None => {
                let mut c = Chunk::new(pos);
                generator.generate_chunk(&mut c);
                c
            }
        };
        let entry = Arc::new(ChunkEntry::new(chunk));
        entry.flags.set_light_dirty(true);
        self.active
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pos, entry);
        self.mark_neighbors_mesh_dirty(pos);
    }

    fn light_job(&self, pos: ChunkPos, ctx: &mut LightContext) {
        if self.is_stale(pos) {
            return;
        }
        let Some(target) = self.active_get(pos) else {
            return;
        };
        let edits_before = target.edit_count();
        // Copie des 9 chunks un par un (jamais deux verrous tenus à la fois).
        let mut copies: Vec<Option<Chunk>> = Vec::with_capacity(9);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let p = ChunkPos::new(pos.x + dx, pos.z + dz);
                copies.push(self.active_get(p).map(|e| e.read().clone()));
            }
        }
        let mut cache: [Option<&Chunk>; 9] = [None; 9];
        for (slot, c) in cache.iter_mut().zip(&copies) {
            *slot = c.as_ref();
        }
        let light = ctx.compute(&cache, &self.light_table);
        {
            let mut c = target.write();
            c.set_light_map(&light);
            // Correction de la course du Java : si un bloc a changé pendant le calcul, la
            // lumière reste à recalculer.
            if target.edit_count() == edits_before {
                target.flags.set_light_dirty(false);
            }
        }
        self.mark_mesh_dirty(pos);
        self.mark_neighbors_mesh_dirty(pos);
    }
}

/// Gestionnaire des chunks actifs autour du joueur.
pub struct ChunkManager {
    inner: Arc<Inner>,
    generator: Arc<WorldGenerator>,
    registry: Arc<BlockRegistry>,
    gen_q: Arc<PrioQueue>,
    save_q: Arc<SaveQueue>,
    light_q: Arc<PrioQueue>,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl std::fmt::Debug for ChunkManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkManager")
            .field("active", &self.active_positions().len())
            .finish_non_exhaustive()
    }
}

impl ChunkManager {
    /// Pools de `max(1, cœurs / 2)` threads pour la génération et pour la lumière.
    pub fn new(
        storage: Arc<dyn ChunkStorage>,
        seed: i64,
        cfg: Arc<WorldConfig>,
        registry: Arc<BlockRegistry>,
    ) -> Self {
        let n = nb_core::thread::background_pool_size();
        Self::with_threads(storage, seed, cfg, registry, n, n)
    }

    pub fn with_threads(
        storage: Arc<dyn ChunkStorage>,
        seed: i64,
        cfg: Arc<WorldConfig>,
        registry: Arc<BlockRegistry>,
        gen_threads: usize,
        light_threads: usize,
    ) -> Self {
        Self::build(
            storage,
            Arc::new(WorldGenerator::new(seed, cfg, registry.ids())),
            registry,
            gen_threads,
            light_threads,
        )
    }

    /// Construit à partir d'un générateur existant (cloné pour chaque thread).
    pub fn with_generator(
        storage: Arc<dyn ChunkStorage>,
        generator: WorldGenerator,
        registry: Arc<BlockRegistry>,
        gen_threads: usize,
        light_threads: usize,
    ) -> Self {
        Self::build(
            storage,
            Arc::new(generator),
            registry,
            gen_threads,
            light_threads,
        )
    }

    fn build(
        storage: Arc<dyn ChunkStorage>,
        generator: Arc<WorldGenerator>,
        registry: Arc<BlockRegistry>,
        gen_threads: usize,
        light_threads: usize,
    ) -> Self {
        let inner = Arc::new(Inner {
            render_distance: AtomicI32::new(DEFAULT_RENDER_DISTANCE),
            active: RwLock::new(HashMap::new()),
            generating: Mutex::new(HashSet::new()),
            lighting: Mutex::new(HashSet::new()),
            unsaved: Mutex::new(HashMap::new()),
            storage,
            storage_lock: Mutex::new(()),
            player_chunk_x: AtomicI32::new(0),
            player_chunk_z: AtomicI32::new(0),
            shutdown: AtomicBool::new(false),
            light_table: LightTable::new(&registry),
            edit_log: Mutex::new(None),
            extra_centers: Mutex::new(Vec::new()),
            remote: AtomicBool::new(false),
        });
        let gen_q = Arc::new(PrioQueue::new());
        let light_q = Arc::new(PrioQueue::new());
        let mut workers = Vec::new();
        for i in 0..gen_threads.max(1) {
            let inner = inner.clone();
            let q = gen_q.clone();
            // Un générateur par thread (copie indépendante, résultat identique).
            let generator = WorldGenerator::clone(&generator);
            workers.push(
                std::thread::Builder::new()
                    .name(format!("nb-chunkgen-{i}"))
                    .spawn(move || {
                        nb_core::thread::lower_priority();
                        while let Some(pos) = q.pop(|| inner.player_chunk_xz()) {
                            if inner.shutdown.load(Ordering::Acquire) {
                                break;
                            }
                            let r = catch_unwind(AssertUnwindSafe(|| {
                                inner.load_job(pos, &generator);
                            }));
                            if r.is_err() {
                                eprintln!("échec de génération du chunk {pos:?}");
                            }
                            lock(&inner.generating).remove(&pos);
                        }
                    })
                    .expect("thread de génération"),
            );
        }
        for i in 0..light_threads.max(1) {
            let inner = inner.clone();
            let q = light_q.clone();
            workers.push(
                std::thread::Builder::new()
                    .name(format!("nb-light-{i}"))
                    .spawn(move || {
                        nb_core::thread::lower_priority();
                        let mut ctx = LightContext::default();
                        while let Some(pos) = q.pop(|| inner.player_chunk_xz()) {
                            if inner.shutdown.load(Ordering::Acquire) {
                                break;
                            }
                            let r = catch_unwind(AssertUnwindSafe(|| {
                                inner.light_job(pos, &mut ctx);
                            }));
                            if r.is_err() {
                                ctx = LightContext::default();
                                eprintln!("échec du calcul de lumière du chunk {pos:?}");
                            }
                            lock(&inner.lighting).remove(&pos);
                        }
                    })
                    .expect("thread de lumière"),
            );
        }
        let save_q = Arc::new(SaveQueue::new());
        {
            let inner = inner.clone();
            let q = save_q.clone();
            workers.push(
                std::thread::Builder::new()
                    .name("nb-chunksave".to_owned())
                    .spawn(move || {
                        nb_core::thread::lower_priority();
                        while let Some(pos) = q.pop() {
                            let r = catch_unwind(AssertUnwindSafe(|| inner.save_pending(pos)));
                            if r.is_err() {
                                eprintln!("échec de sauvegarde du chunk {pos:?}");
                            }
                        }
                    })
                    .expect("thread de sauvegarde"),
            );
        }
        Self {
            inner,
            generator,
            registry,
            gen_q,
            light_q,
            save_q,
            workers: Mutex::new(workers),
        }
    }

    pub fn set_render_distance(&self, r: i32) {
        self.inner.render_distance.store(r, Ordering::Relaxed);
    }

    pub fn render_distance(&self) -> i32 {
        self.inner.render_distance.load(Ordering::Relaxed)
    }

    /// Chunk du joueur (dernier `update`).
    pub fn player_chunk(&self) -> ChunkPos {
        ChunkPos::new(
            self.inner.player_chunk_x.load(Ordering::Relaxed),
            self.inner.player_chunk_z.load(Ordering::Relaxed),
        )
    }

    /// Appelé chaque frame (spec §11.2) : déchargements, puis chargements et lumière.
    pub fn update(&self, player_pos: Vec3) {
        self.update_with_extras(player_pos, &[]);
    }

    /// Comme [`Self::update`], avec des centres supplémentaires `(position, rayon en chunks)` :
    /// les joueurs distants d'une partie LAN gardent leurs chunks chargés sur l'hôte.
    pub fn update_with_extras(&self, player_pos: Vec3, extras: &[(Vec3, i32)]) {
        let extra_chunks: Vec<(i32, i32, i32)> = extras
            .iter()
            .map(|(p, r)| {
                (
                    (p.x / CHUNK_SIZE_X as f32).floor() as i32,
                    (p.z / CHUNK_SIZE_Z as f32).floor() as i32,
                    (*r).max(1),
                )
            })
            .collect();
        *lock(&self.inner.extra_centers) = extra_chunks.clone();
        let pcx = (player_pos.x / CHUNK_SIZE_X as f32).floor() as i32;
        let pcz = (player_pos.z / CHUNK_SIZE_Z as f32).floor() as i32;
        self.inner.player_chunk_x.store(pcx, Ordering::Relaxed);
        self.inner.player_chunk_z.store(pcz, Ordering::Relaxed);
        let r = self.render_distance();

        let remote = self.inner.remote.load(Ordering::Acquire);
        let inside = |p: ChunkPos, cx: i32, cz: i32, r: i32| {
            (i64::from(p.x) - i64::from(cx)).abs() <= i64::from(r)
                && (i64::from(p.z) - i64::from(cz)).abs() <= i64::from(r)
        };
        let wanted = |p: ChunkPos| {
            inside(p, pcx, pcz, r)
                || extra_chunks
                    .iter()
                    .any(|&(ex, ez, er)| inside(p, ex, ez, er))
        };

        // Déchargements : un seul passage sous verrou de lecture, sans table intermédiaire.
        let to_unload: Vec<ChunkPos> = self
            .inner
            .active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .copied()
            .filter(|p| !wanted(*p))
            .collect();
        for pos in to_unload {
            self.unload_chunk(pos);
        }

        // Chargements et lumière : un seul verrou de lecture pour toute la zone. Pas de liste
        // `expected` ni d'`active_get` par chunk (centaines d'acquisitions de verrou par appel).
        let mut to_load = Vec::new();
        let mut to_light = Vec::new();
        {
            let active = self.inner.active.read().unwrap_or_else(|e| e.into_inner());
            let mut visit = |pos: ChunkPos| match active.get(&pos) {
                // Monde client : seuls les chunks reçus de l'hôte sont chargés.
                None if remote && !self.inner.storage.has_chunk(pos) => {}
                None => to_load.push(pos),
                Some(chunk) => {
                    if chunk.flags.is_light_dirty()
                        && (-1..=1).all(|x| {
                            (-1..=1)
                                .all(|z| active.contains_key(&ChunkPos::new(pos.x + x, pos.z + z)))
                        })
                    {
                        to_light.push(pos);
                    }
                }
            };
            for x in -r..=r {
                for z in -r..=r {
                    visit(ChunkPos::new(pcx.wrapping_add(x), pcz.wrapping_add(z)));
                }
            }
            for (i, &(ex, ez, er)) in extra_chunks.iter().enumerate() {
                for x in -er..=er {
                    for z in -er..=er {
                        let p = ChunkPos::new(ex.wrapping_add(x), ez.wrapping_add(z));
                        // Déjà vu : zone principale ou centre supplémentaire précédent.
                        let seen = inside(p, pcx, pcz, r)
                            || extra_chunks[..i]
                                .iter()
                                .any(|&(fx, fz, fr)| inside(p, fx, fz, fr));
                        if !seen {
                            visit(p);
                        }
                    }
                }
            }
        }
        for pos in to_load {
            if !lock(&self.inner.generating).contains(&pos) {
                self.load_chunk_async(pos);
            }
        }
        for pos in to_light {
            if !lock(&self.inner.lighting).contains(&pos) {
                self.compute_light_async(pos);
            }
        }
    }

    /// Les 9 chunks du carré 3×3 sont actifs.
    pub fn has_neighbors(&self, pos: ChunkPos) -> bool {
        let active = self.inner.active.read().unwrap_or_else(|e| e.into_inner());
        (-1..=1)
            .all(|x| (-1..=1).all(|z| active.contains_key(&ChunkPos::new(pos.x + x, pos.z + z))))
    }

    fn load_chunk_async(&self, pos: ChunkPos) {
        lock(&self.inner.generating).insert(pos);
        self.gen_q.push(pos);
    }

    fn compute_light_async(&self, pos: ChunkPos) {
        lock(&self.inner.lighting).insert(pos);
        self.light_q.push(pos);
    }

    /// Retire un chunk et le sauvegarde s'il est `dirty` (E/S sur le thread appelant).
    pub fn unload_chunk(&self, pos: ChunkPos) {
        let removed = self
            .inner
            .active
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pos);
        if let Some(entry) = removed {
            // Monde client : l'hôte fait foi, on ne garde rien (une copie périmée serait relue).
            if entry.flags.is_dirty() && !self.inner.remote.load(Ordering::Acquire) {
                // Écriture sur le thread de sauvegarde : ni E/S ni fsync sur le thread principal.
                lock(&self.inner.unsaved).insert(pos, entry);
                self.save_q.push(pos);
            }
        }
    }

    pub fn chunk(&self, pos: ChunkPos) -> Option<SharedChunk> {
        self.inner.active_get(pos)
    }

    pub fn active_positions(&self) -> Vec<ChunkPos> {
        self.inner
            .active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .copied()
            .collect()
    }

    /// Copie de la table des chunks actifs.
    pub fn active_chunks(&self) -> Vec<(ChunkPos, SharedChunk)> {
        self.inner
            .active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(p, c)| (*p, c.clone()))
            .collect()
    }

    /// Nombre de chunks en génération / en calcul de lumière.
    pub fn pending(&self) -> (usize, usize) {
        (
            lock(&self.inner.generating).len(),
            lock(&self.inner.lighting).len(),
        )
    }

    /// Bloc en coordonnées monde ; chunk absent, `y` hors limites ou ID inconnu → air.
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        let (cx, cz) = (chunk_coord(x), chunk_coord(z));
        match self.inner.active_get(ChunkPos::new(cx, cz)) {
            Some(c) => {
                let id = c.read().block(
                    x.wrapping_sub(cx.wrapping_mul(16)),
                    y,
                    z.wrapping_sub(cz.wrapping_mul(16)),
                );
                self.registry.from_id(id).id
            }
            None => self.registry.ids().air,
        }
    }

    /// Écrit un bloc (chunk absent → ignoré) et marque les voisins de bord à remailler.
    /// Si le journal d'éditions est actif, l'écriture y est consignée.
    pub fn set_block(&self, x: i32, y: i32, z: i32, id: BlockId) {
        if !self.write_block(x, y, z, id) {
            return;
        }
        if let Some(log) = lock(&self.inner.edit_log).as_mut() {
            log.push(BlockEdit { x, y, z, id });
        }
    }

    /// Comme [`Self::set_block`] mais sans journal : application d'une modification reçue du
    /// réseau (sinon elle serait renvoyée à son expéditeur). `false` si le chunk est absent.
    pub fn set_block_quiet(&self, x: i32, y: i32, z: i32, id: BlockId) -> bool {
        self.write_block(x, y, z, id)
    }

    /// Mode client : plus aucune génération locale ni sauvegarde, les chunks viennent de
    /// [`Self::install_chunk`].
    pub fn set_remote(&self, on: bool) {
        self.inner.remote.store(on, Ordering::Release);
    }

    pub fn is_remote(&self) -> bool {
        self.inner.remote.load(Ordering::Acquire)
    }

    /// Active (vide) ou désactive le journal des écritures de blocs.
    pub fn enable_edit_log(&self, on: bool) {
        *lock(&self.inner.edit_log) = on.then(Vec::new);
    }

    /// Écritures consignées depuis le dernier appel (vide si le journal est désactivé).
    pub fn take_edits(&self) -> Vec<BlockEdit> {
        lock(&self.inner.edit_log)
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Voxels du chunk : chunk actif, sinon stockage. `None` si le chunk n'a jamais été écrit.
    pub fn voxels_of(&self, pos: ChunkPos) -> Option<Box<[BlockId; CHUNK_VOLUME]>> {
        if let Some(c) = self.inner.active_get(pos) {
            return Some(Box::new(*c.read().voxels()));
        }
        if let Some(e) = lock(&self.inner.unsaved).get(&pos).cloned() {
            return Some(Box::new(*e.read().voxels()));
        }
        let _g = lock(&self.inner.storage_lock);
        if self.inner.storage.has_chunk(pos) {
            self.inner.storage.load_chunk(pos).ok().flatten()
        } else {
            None
        }
    }

    /// Remplace le contenu d'un chunk (reçu du réseau) : chunk actif réécrit et remaillé, sinon
    /// enregistré dans le stockage pour son prochain chargement. Les quatre voisins sont
    /// remaillés (faces de bord).
    pub fn install_chunk(&self, pos: ChunkPos, voxels: &[BlockId; CHUNK_VOLUME]) {
        if let Some(c) = self.inner.active_get(pos) {
            if c.read().voxels() == voxels {
                return; // identique : ni remaillage ni recalcul de lumière
            }
            c.replace_voxels(voxels);
            self.inner.mark_neighbors_mesh_dirty(pos);
        } else {
            // Une écriture en attente de l'ancien contenu ne doit pas écraser celui-ci.
            lock(&self.inner.unsaved).remove(&pos);
            let _g = lock(&self.inner.storage_lock);
            if let Err(e) = self.inner.storage.save_chunk(pos, voxels) {
                eprintln!("réseau : chunk {pos:?} non enregistré : {e}");
            }
        }
    }

    fn write_block(&self, x: i32, y: i32, z: i32, id: BlockId) -> bool {
        let (cx, cz) = (chunk_coord(x), chunk_coord(z));
        let Some(c) = self.inner.active_get(ChunkPos::new(cx, cz)) else {
            return false;
        };
        let lx = x.wrapping_sub(cx.wrapping_mul(16));
        let lz = z.wrapping_sub(cz.wrapping_mul(16));
        c.set_block(lx, y, lz, id);
        if lx == 0 {
            self.inner.mark_mesh_dirty(ChunkPos::new(cx - 1, cz));
        }
        if lx == CHUNK_SIZE_X as i32 - 1 {
            self.inner.mark_mesh_dirty(ChunkPos::new(cx + 1, cz));
        }
        if lz == 0 {
            self.inner.mark_mesh_dirty(ChunkPos::new(cx, cz - 1));
        }
        if lz == CHUNK_SIZE_Z as i32 - 1 {
            self.inner.mark_mesh_dirty(ChunkPos::new(cx, cz + 1));
        }
        true
    }

    pub fn mark_mesh_dirty(&self, pos: ChunkPos) {
        self.inner.mark_mesh_dirty(pos);
    }

    pub fn is_chunk_loaded(&self, wx: i32, wz: i32) -> bool {
        self.inner
            .is_active(ChunkPos::new(chunk_coord(wx), chunk_coord(wz)))
    }

    pub fn compute_surface_height(&self, wx: i32, wz: i32) -> i32 {
        self.generator.compute_surface_height(wx, wz)
    }

    pub fn generator(&self) -> &WorldGenerator {
        &self.generator
    }

    pub fn storage(&self) -> &Arc<dyn ChunkStorage> {
        &self.inner.storage
    }

    pub fn registry(&self) -> &Arc<BlockRegistry> {
        &self.registry
    }

    /// Écrit tout de suite, sur le thread appelant, les chunks déchargés dont la sauvegarde est
    /// encore en attente (elle se fait normalement en arrière-plan).
    pub fn flush_pending_saves(&self) {
        self.inner.flush_unsaved();
    }

    /// Sauvegarde tous les chunks actifs `dirty` (`WorldSaveManager.flushChunks`).
    /// Continue après une erreur et renvoie la dernière.
    pub fn flush_dirty(&self) -> Result<()> {
        self.inner.flush_unsaved();
        let mut result = Ok(());
        for (_, entry) in self.active_chunks() {
            if entry.flags.is_dirty() {
                if let Err(e) = self.inner.save(&entry) {
                    result = Err(e);
                }
            }
        }
        result
    }

    /// Annule les tâches en attente et arrête les threads (n'écrit rien sur disque).
    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.gen_q.close();
        self.light_q.close();
        self.save_q.close();
        let workers: Vec<_> = lock(&self.workers).drain(..).collect();
        for w in workers {
            let _ = w.join();
        }
        // Chunks déchargés dont l'écriture n'était pas finie : jamais perdus.
        self.inner.flush_unsaved();
    }
}

impl Drop for ChunkManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}
