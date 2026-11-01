//! Ordonnancement des remaillages (`ChunkMeshManager`, spec §14), sans GPU.
//!
//! Le monde est vu à travers le trait [`ChunkSource`] (implémenté côté jeu au-dessus de
//! `nb-world`) ; les meshes produits sont rendus sous forme de [`MeshAction`] que le renderer
//! applique (upload / libération).

use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use nb_core::ChunkPos;

use crate::biome::BiomeSource;
use crate::block::BlockTable;
use crate::mesher::{mesh_chunk, MeshOutput};
use crate::neighborhood::{ChunkVoxels, Neighborhood};

/// Accès du planificateur à l'état des chunks du monde.
pub trait ChunkSource {
    /// Le chunk est-il chargé (`getChunk(pos) != null`) ?
    fn is_loaded(&self, p: ChunkPos) -> bool;
    fn mesh_dirty(&self, p: ChunkPos) -> bool;
    fn light_dirty(&self, p: ChunkPos) -> bool;
    /// Sans effet si le chunk n'est pas chargé.
    fn set_mesh_dirty(&mut self, p: ChunkPos, v: bool);
    /// Copie des voxels (`new ChunkSnapshot(chunk)`), `None` si non chargé.
    fn snapshot(&self, p: ChunkPos) -> Option<Arc<ChunkVoxels>>;
}

/// Résultat d'une tâche de maillage.
#[derive(Debug)]
pub struct MeshResult {
    pub pos: ChunkPos,
    pub mesh: MeshOutput,
    pub generation: u64,
}

/// Ce que le renderer doit faire de ses meshes GPU.
#[derive(Debug, PartialEq)]
pub enum MeshAction {
    /// Créer le mesh de `pos` ou remplacer ses données (`new Mesh` / `updateData`).
    /// Jamais émis avec un maillage vide (l'ancien mesh reste alors affiché).
    Upload(ChunkPos, MeshOutput),
    /// Libérer le mesh de `pos` (`cleanup`).
    Remove(ChunkPos),
}

/// Nombre de threads par défaut : `max(1, cœurs / 2)`.
pub fn default_threads() -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    (cpus / 2).max(1)
}

struct Job {
    pos: ChunkPos,
    chunks: [Option<Arc<ChunkVoxels>>; 9],
    generation: u64,
    blocks: Arc<BlockTable>,
    biomes: Arc<dyn BiomeSource>,
}

struct Shared {
    in_flight: Mutex<HashSet<ChunkPos>>,
    stop: AtomicBool,
}

impl Shared {
    fn in_flight(&self) -> MutexGuard<'_, HashSet<ChunkPos>> {
        self.in_flight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Retire la position des tâches en cours même si le maillage panique (`finally` Java).
struct InFlightGuard<'a> {
    shared: &'a Shared,
    pos: ChunkPos,
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.shared.in_flight().remove(&self.pos);
    }
}

fn worker(shared: Arc<Shared>, jobs: Receiver<Job>, results: Sender<MeshResult>) {
    while let Ok(job) = jobs.recv() {
        let _guard = InFlightGuard {
            shared: &shared,
            pos: job.pos,
        };
        // `shutdownNow` : les tâches encore en file ne sont pas exécutées.
        if shared.stop.load(Ordering::Acquire) {
            continue;
        }
        let refs: [Option<&ChunkVoxels>; 9] = std::array::from_fn(|i| job.chunks[i].as_deref());
        let n = Neighborhood::from_voxels(refs);
        let mesh = mesh_chunk(job.pos, &n, &job.blocks, job.biomes.as_ref());
        let _ = results.send(MeshResult {
            pos: job.pos,
            mesh,
            generation: job.generation,
        });
    }
}

/// Planificateur des remaillages : pool de threads, tâches en cours, file de résultats et
/// compteur de génération.
pub struct MeshScheduler {
    generation: AtomicU64,
    shared: Arc<Shared>,
    jobs: Option<Sender<Job>>,
    results: Receiver<MeshResult>,
    /// Positions ayant un mesh côté renderer (`chunkMeshes.keySet()`), triées pour un ordre
    /// de libération déterministe.
    has_mesh: BTreeSet<ChunkPos>,
    workers: Vec<JoinHandle<()>>,
}

impl MeshScheduler {
    /// Crée le pool (`threads` est ramené à au moins 1 ; voir [`default_threads`]).
    pub fn new(threads: usize) -> Self {
        let (job_tx, job_rx) = crossbeam_channel::unbounded::<Job>();
        let (res_tx, res_rx) = crossbeam_channel::unbounded::<MeshResult>();
        let shared = Arc::new(Shared {
            in_flight: Mutex::new(HashSet::new()),
            stop: AtomicBool::new(false),
        });
        let workers = (0..threads.max(1))
            .map(|i| {
                let (shared, jobs, results) = (shared.clone(), job_rx.clone(), res_tx.clone());
                std::thread::Builder::new()
                    .name(format!("nb-mesh-{i}"))
                    .spawn(move || worker(shared, jobs, results))
                    .expect("création d'un thread de maillage")
            })
            .collect();
        Self {
            generation: AtomicU64::new(0),
            shared,
            jobs: Some(job_tx),
            results: res_rx,
            has_mesh: BTreeSet::new(),
            workers,
        }
    }

    /// Génération courante (incrémentée par [`MeshScheduler::invalidate_all`]).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Nombre de tâches en cours (ou en file).
    pub fn in_flight_count(&self) -> usize {
        self.shared.in_flight().len()
    }

    /// Le chunk a-t-il un mesh côté renderer ?
    pub fn has_mesh(&self, p: ChunkPos) -> bool {
        self.has_mesh.contains(&p)
    }

    /// `ChunkMeshManager.update` (appelé chaque frame).
    pub fn update(
        &mut self,
        world: &mut dyn ChunkSource,
        active: &[ChunkPos],
        blocks: Arc<BlockTable>,
        biomes: Arc<dyn BiomeSource>,
    ) -> Vec<MeshAction> {
        let active_set: HashSet<ChunkPos> = active.iter().copied().collect();
        let mut actions = Vec::new();

        // 1. Libérer les meshes hors de la zone active.
        let stale: Vec<ChunkPos> = self
            .has_mesh
            .iter()
            .copied()
            .filter(|p| !active_set.contains(p))
            .collect();
        for p in stale {
            self.has_mesh.remove(&p);
            actions.push(MeshAction::Remove(p));
        }

        // 2. Soumettre les chunks sales dont le voisinage est prêt.
        for &pos in active {
            if !world.is_loaded(pos)
                || !world.mesh_dirty(pos)
                || self.shared.in_flight().contains(&pos)
            {
                continue;
            }
            let mut chunks: [Option<Arc<ChunkVoxels>>; 9] = Default::default();
            let mut ready = true;
            'outer: for dz in -1..=1 {
                for dx in -1..=1 {
                    let np = ChunkPos::new(pos.x + dx, pos.z + dz);
                    let snap = if world.light_dirty(np) {
                        None
                    } else {
                        world.snapshot(np)
                    };
                    match snap {
                        Some(s) => chunks[((dz + 1) * 3 + (dx + 1)) as usize] = Some(s),
                        None => {
                            ready = false;
                            break 'outer;
                        }
                    }
                }
            }
            if !ready {
                continue;
            }
            let Some(jobs) = &self.jobs else { continue };
            world.set_mesh_dirty(pos, false);
            self.shared.in_flight().insert(pos);
            let job = Job {
                pos,
                chunks,
                generation: self.generation(),
                blocks: blocks.clone(),
                biomes: biomes.clone(),
            };
            if jobs.send(job).is_err() {
                self.shared.in_flight().remove(&pos);
                world.set_mesh_dirty(pos, true);
            }
        }

        // 3. Vider la file de résultats.
        let current = self.generation();
        while let Ok(r) = self.results.try_recv() {
            if r.generation != current {
                world.set_mesh_dirty(r.pos, true);
                continue;
            }
            if active_set.contains(&r.pos) {
                // Résultat vide : l'ancien mesh (s'il existe) reste affiché.
                if !r.mesh.is_empty() {
                    self.has_mesh.insert(r.pos);
                    actions.push(MeshAction::Upload(r.pos, r.mesh));
                }
            } else {
                world.set_mesh_dirty(r.pos, true);
            }
        }
        actions
    }

    /// `invalidateAllMeshes` : nouvelle génération, libération de tous les meshes, oubli des
    /// tâches en cours (comme le Java) et remaillage de tous les chunks `loaded`.
    pub fn invalidate_all(
        &mut self,
        world: &mut dyn ChunkSource,
        loaded: &[ChunkPos],
    ) -> Vec<MeshAction> {
        self.generation.fetch_add(1, Ordering::AcqRel);
        let actions = std::mem::take(&mut self.has_mesh)
            .into_iter()
            .map(MeshAction::Remove)
            .collect();
        self.shared.in_flight().clear();
        for &p in loaded {
            world.set_mesh_dirty(p, true);
        }
        actions
    }

    /// Arrêt du pool : les tâches en file sont abandonnées, celles en cours se terminent.
    pub fn shutdown(mut self) {
        self.stop_workers();
    }

    fn stop_workers(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.jobs = None;
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

impl Drop for MeshScheduler {
    fn drop(&mut self) {
        self.stop_workers();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::biome::UniformBiome;
    use nb_data::BlockRegistry;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct World {
        chunks: HashMap<ChunkPos, (Arc<ChunkVoxels>, bool, bool)>,
    }

    impl World {
        fn add(&mut self, p: ChunkPos, v: ChunkVoxels) {
            self.chunks.insert(p, (Arc::new(v), true, false));
        }
    }

    impl ChunkSource for World {
        fn is_loaded(&self, p: ChunkPos) -> bool {
            self.chunks.contains_key(&p)
        }
        fn mesh_dirty(&self, p: ChunkPos) -> bool {
            self.chunks.get(&p).is_some_and(|c| c.1)
        }
        fn light_dirty(&self, p: ChunkPos) -> bool {
            self.chunks.get(&p).is_some_and(|c| c.2)
        }
        fn set_mesh_dirty(&mut self, p: ChunkPos, v: bool) {
            if let Some(c) = self.chunks.get_mut(&p) {
                c.1 = v;
            }
        }
        fn snapshot(&self, p: ChunkPos) -> Option<Arc<ChunkVoxels>> {
            self.chunks.get(&p).map(|c| c.0.clone())
        }
    }

    fn stone_floor() -> ChunkVoxels {
        let mut v = ChunkVoxels::filled(0);
        for x in 0..16 {
            for z in 0..16 {
                v.set(x, 0, z, 3);
            }
        }
        v
    }

    fn world_3x3() -> World {
        let mut w = World::default();
        for x in -1..=1 {
            for z in -1..=1 {
                w.add(ChunkPos::new(x, z), stone_floor());
            }
        }
        w
    }

    fn ctx() -> (Arc<BlockTable>, Arc<dyn BiomeSource>) {
        (
            Arc::new(BlockTable::from_registry(&BlockRegistry::builtin(), |_| 1)),
            Arc::new(UniformBiome(0)),
        )
    }

    /// Appelle `update` jusqu'à ce que plus rien ne soit en cours.
    fn pump(
        s: &mut MeshScheduler,
        w: &mut World,
        active: &[ChunkPos],
        b: &Arc<BlockTable>,
        bi: &Arc<dyn BiomeSource>,
    ) -> Vec<MeshAction> {
        let mut all = s.update(w, active, b.clone(), bi.clone());
        let deadline = Instant::now() + Duration::from_secs(30);
        while s.in_flight_count() > 0 {
            assert!(Instant::now() < deadline, "maillage trop long");
            std::thread::yield_now();
        }
        all.extend(s.update(w, active, b.clone(), bi.clone()));
        all
    }

    #[test]
    fn meshes_only_when_neighbours_ready() {
        let (b, bi) = ctx();
        let mut s = MeshScheduler::new(2);
        let mut w = world_3x3();
        let center = ChunkPos::new(0, 0);
        let edge = ChunkPos::new(1, 0); // voisins absents à l'est
        let actions = pump(&mut s, &mut w, &[center, edge], &b, &bi);
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], MeshAction::Upload(p, m) if *p == center && !m.is_empty()));
        assert!(!w.mesh_dirty(center));
        assert!(
            w.mesh_dirty(edge),
            "reste sale tant que le voisinage manque"
        );
        assert!(s.has_mesh(center));

        // Lumière sale chez un voisin : abandon pour cette frame.
        w.set_mesh_dirty(center, true);
        w.chunks.get_mut(&ChunkPos::new(-1, -1)).unwrap().2 = true;
        assert!(pump(&mut s, &mut w, &[center], &b, &bi).is_empty());
        assert!(w.mesh_dirty(center));

        // Sortie de la zone active : libération.
        let actions = pump(&mut s, &mut w, &[], &b, &bi);
        assert_eq!(actions, vec![MeshAction::Remove(center)]);
        s.shutdown();
    }

    #[test]
    fn empty_result_keeps_previous_mesh() {
        let (b, bi) = ctx();
        let mut s = MeshScheduler::new(1);
        let mut w = world_3x3();
        let center = ChunkPos::new(0, 0);
        pump(&mut s, &mut w, &[center], &b, &bi);
        assert!(s.has_mesh(center));
        // Chunk vidé → maillage vide → aucune action, le mesh reste.
        w.add(center, ChunkVoxels::filled(0));
        let actions = pump(&mut s, &mut w, &[center], &b, &bi);
        assert!(actions.is_empty());
        assert!(s.has_mesh(center));
    }

    #[test]
    fn invalidation_discards_stale_results() {
        let (b, bi) = ctx();
        let mut s = MeshScheduler::new(1);
        let mut w = world_3x3();
        let center = ChunkPos::new(0, 0);
        pump(&mut s, &mut w, &[center], &b, &bi);
        w.set_mesh_dirty(center, true);
        // Soumission puis invalidation avant de lire le résultat.
        assert!(s
            .update(&mut w, &[center], b.clone(), bi.clone())
            .is_empty());
        let loaded: Vec<ChunkPos> = w.chunks.keys().copied().collect();
        let actions = s.invalidate_all(&mut w, &loaded);
        assert_eq!(actions, vec![MeshAction::Remove(center)]);
        assert_eq!(s.generation(), 1);
        assert!(!s.has_mesh(center));
        assert!(w.mesh_dirty(ChunkPos::new(1, 1)));
        // Le résultat périmé est jeté, puis le chunk est remaillé à la génération courante.
        let mut uploads = 0;
        for _ in 0..4 {
            uploads += pump(&mut s, &mut w, &[center], &b, &bi)
                .iter()
                .filter(|a| matches!(a, MeshAction::Upload(..)))
                .count();
        }
        assert!(uploads >= 1);
        assert!(s.has_mesh(center));
    }

    #[test]
    fn default_threads_is_positive() {
        assert!(default_threads() >= 1);
    }
}
