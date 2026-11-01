//! Monde (partie `nb-world` de `World.java`, spec §12) : chunks, biomes, configuration, spawn.

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use nb_core::glam::Vec3;
use nb_data::{BlockId, BlockRegistry};

use crate::biome::BiomeSystem;
use crate::config::WorldConfig;
use crate::manager::ChunkManager;
use crate::noise::Noise;
use crate::storage::ChunkStorage;

/// Monde de jeu sans ECS.
///
/// La configuration vivante (`spawn`, `time_of_day`…) est modifiable via [`World::config_mut`] ;
/// le générateur, lui, travaille sur la copie prise à la construction (les paramètres de
/// génération ne changent jamais en cours de partie).
pub struct World {
    seed: i64,
    chunks: ChunkManager,
    biomes: BiomeSystem,
    cfg: RwLock<WorldConfig>,
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl World {
    pub fn new(
        seed: i64,
        storage: Arc<dyn ChunkStorage>,
        cfg: WorldConfig,
        registry: Arc<BlockRegistry>,
    ) -> Self {
        let shared = Arc::new(cfg.clone());
        let chunks = ChunkManager::new(storage, seed, shared.clone(), registry.clone());
        Self::from_parts(seed, chunks, cfg, &registry)
    }

    /// Construit autour d'un gestionnaire existant (nombre de threads choisi, tests…).
    pub fn from_parts(
        seed: i64,
        chunks: ChunkManager,
        cfg: WorldConfig,
        registry: &BlockRegistry,
    ) -> Self {
        // Le Java crée un `Noise(seed)` distinct pour le BiomeSystem du monde : même permutation.
        let biomes = BiomeSystem::new(
            Arc::new(Noise::new(seed)),
            Arc::new(cfg.clone()),
            registry.ids(),
        );
        Self {
            seed,
            chunks,
            biomes,
            cfg: RwLock::new(cfg),
        }
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    pub fn update(&self, player_pos: Vec3) {
        self.chunks.update(player_pos);
    }

    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockId {
        self.chunks.block(x, y, z)
    }

    pub fn set_block(&self, x: i32, y: i32, z: i32, id: BlockId) {
        self.chunks.set_block(x, y, z, id);
    }

    pub fn is_chunk_loaded(&self, wx: i32, wz: i32) -> bool {
        self.chunks.is_chunk_loaded(wx, wz)
    }

    pub fn chunk_manager(&self) -> &ChunkManager {
        &self.chunks
    }

    pub fn biome_system(&self) -> &BiomeSystem {
        &self.biomes
    }

    pub fn config(&self) -> RwLockReadGuard<'_, WorldConfig> {
        self.cfg.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn config_mut(&self) -> RwLockWriteGuard<'_, WorldConfig> {
        self.cfg.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Recherche du spawn en anneaux de 16 blocs autour de l'origine (pieds à `h + 1`).
    pub fn find_spawn_location(&self) -> [f64; 3] {
        let sea = self.config().sea_level;
        find_spawn_location(sea, |x, z| self.chunks.compute_surface_height(x, z))
    }

    /// Position de réapparition (`World.respawnPlayer`).
    pub fn respawn_position(&self) -> [f64; 3] {
        self.config().respawn_position()
    }

    /// Spawn enregistré, ou recherché puis enregistré dans la configuration (première partie).
    pub fn ensure_spawn(&self) -> [f64; 3] {
        if let Some(s) = self.config().spawn {
            return s;
        }
        let s = self.find_spawn_location();
        self.config_mut().spawn = Some(s);
        s
    }

    pub fn shutdown(&self) {
        self.chunks.shutdown();
    }
}

/// `World.findSpawnLocation` avec une fonction de hauteur quelconque.
pub fn find_spawn_location(sea_level: i32, height: impl Fn(i32, i32) -> i32) -> [f64; 3] {
    let mut radius: i32 = 0;
    while radius <= 256 {
        let mut ox = -radius;
        while ox <= radius {
            let mut oz = -radius;
            while oz <= radius {
                if ox.abs() == radius || oz.abs() == radius {
                    let h = height(ox, oz);
                    if h > sea_level {
                        return [f64::from(ox) + 0.5, f64::from(h + 1), f64::from(oz) + 0.5];
                    }
                }
                oz += 16;
            }
            ox += 16;
        }
        radius += 16;
    }
    [0.5, f64::from(sea_level + 5), 0.5]
}
