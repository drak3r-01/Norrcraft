//! Monde (partie `nb-world` de `World.java`, spec §12) : chunks, biomes, configuration, spawn.

use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};

use nb_core::glam::Vec3;
use nb_data::{BlockId, BlockRegistry};

use crate::config::WorldConfig;
use crate::fluid::managed::ManagerFluid;
use crate::fluid::{FluidDelta, FluidEnv, FluidSim, FluidStats};
use crate::manager::ChunkManager;
use crate::profile::{BiomeId, BiomeInfo, WorldKind, WorldProfile};
use crate::storage::ChunkStorage;

/// Monde de jeu sans ECS.
///
/// La configuration vivante (`spawn`, `time_of_day`…) est modifiable via [`World::config_mut`] ;
/// le générateur, lui, travaille sur la copie prise à la construction (les paramètres de
/// génération ne changent jamais en cours de partie).
pub struct World {
    seed: i64,
    chunks: ChunkManager,
    profile: Arc<WorldProfile>,
    cfg: RwLock<WorldConfig>,
    /// Centres supplémentaires gardés chargés (joueurs distants d'une partie LAN).
    extras: Mutex<Vec<(Vec3, i32)>>,
    /// Simulation de l'eau vivante (univers avec `Systems::living_water`).
    fluid: Option<Mutex<FluidSim>>,
}

impl std::fmt::Debug for World {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("World")
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl World {
    /// Monde de l'univers vanilla (générateurs Classic / Norrblock selon `cfg`).
    pub fn new(
        seed: i64,
        storage: Arc<dyn ChunkStorage>,
        cfg: WorldConfig,
        registry: Arc<BlockRegistry>,
    ) -> Self {
        let profile = Arc::new(WorldProfile::vanilla(
            seed,
            Arc::new(cfg.clone()),
            registry.ids(),
        ));
        Self::with_profile(seed, storage, cfg, registry, profile)
    }

    /// Monde de l'univers `kind` avec le registre de blocs (palette) du monde.
    pub fn for_kind(
        kind: WorldKind,
        seed: i64,
        storage: Arc<dyn ChunkStorage>,
        cfg: WorldConfig,
        registry: Arc<BlockRegistry>,
    ) -> Self {
        match kind {
            WorldKind::Vanilla => Self::new(seed, storage, cfg, registry),
            WorldKind::Norrcraft => {
                let profile = Arc::new(WorldProfile::norrcraft(
                    seed,
                    crate::norrcraft::NorrConfig::default(),
                    &registry,
                ));
                Self::with_profile(seed, storage, cfg, registry, profile)
            }
        }
    }

    /// Monde d'un univers quelconque : le générateur et les biomes viennent du profil.
    pub fn with_profile(
        seed: i64,
        storage: Arc<dyn ChunkStorage>,
        cfg: WorldConfig,
        registry: Arc<BlockRegistry>,
        profile: Arc<WorldProfile>,
    ) -> Self {
        let n = nb_core::thread::background_pool_size();
        let chunks =
            ChunkManager::with_terrain(storage, profile.generator().clone(), registry, n, n);
        Self::from_parts(seed, chunks, cfg, profile)
    }

    /// Construit autour d'un gestionnaire existant (nombre de threads choisi, tests…).
    pub fn from_parts(
        seed: i64,
        chunks: ChunkManager,
        cfg: WorldConfig,
        profile: Arc<WorldProfile>,
    ) -> Self {
        let fluid = if profile.systems.living_water {
            chunks.enable_fluids();
            chunks.fluid_blocks().cloned().map(|blocks| {
                Mutex::new(FluidSim::new(
                    nb_data::WaterRules::builtin(),
                    blocks,
                    seed as u64,
                    true,
                ))
            })
        } else {
            None
        };
        Self {
            seed,
            chunks,
            profile,
            cfg: RwLock::new(cfg),
            extras: Mutex::new(Vec::new()),
            fluid,
        }
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    pub fn update(&self, player_pos: Vec3) {
        let extras = self.extras.lock().unwrap_or_else(|e| e.into_inner());
        self.chunks.update_with_extras(player_pos, &extras);
    }

    /// Joueurs distants `(position, rayon en chunks)` dont les chunks restent chargés.
    pub fn set_extra_centers(&self, centers: Vec<(Vec3, i32)>) {
        *self.extras.lock().unwrap_or_else(|e| e.into_inner()) = centers;
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

    /// Profil d'univers du monde (biomes, générateur, systèmes).
    pub fn profile(&self) -> &Arc<WorldProfile> {
        &self.profile
    }

    /// Identifiant de biome de la colonne `(wx, wz)`.
    pub fn biome_id_at(&self, wx: i32, wz: i32) -> BiomeId {
        self.chunks.terrain().biome_at(wx, wz)
    }

    /// Description du biome de la colonne `(wx, wz)`.
    pub fn biome_info_at(&self, wx: i32, wz: i32) -> &BiomeInfo {
        self.profile.biome(self.biome_id_at(wx, wz))
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
        self.chunks.terrain().find_spawn(sea)
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

    // ---- eau vivante (doc 07 §3.2) ----

    /// Le monde a-t-il de l'eau vivante ?
    pub fn has_living_water(&self) -> bool {
        self.fluid.is_some()
    }

    /// Un tick de la simulation de l'eau. Sans effet sans eau vivante ; sur un monde client la
    /// file d'événements est seulement vidée (l'hôte simule seul).
    pub fn tick_fluids(&self, env: &FluidEnv) {
        let Some(m) = &self.fluid else {
            return;
        };
        let events = self.chunks.take_fluid_events();
        if self.chunks.is_remote() {
            return;
        }
        let mut sim = m.lock().unwrap_or_else(|e| e.into_inner());
        // Joueurs distants d'une partie LAN : leur eau est simulée aussi.
        let mut centers = env.centers.clone();
        centers.extend(
            self.extras
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|(p, _)| *p),
        );
        sim.set_centers(&centers);
        let env = FluidEnv {
            centers,
            ..env.clone()
        };
        let mut w = ManagerFluid::new(&self.chunks);
        sim.tick(&mut w, events, &env);
    }

    /// Niveau d'eau 0..=8 de la cellule (nappes générées comprises : 8).
    pub fn water_level(&self, x: i32, y: i32, z: i32) -> u8 {
        let Some(m) = &self.fluid else {
            return u8::from(self.block(x, y, z) == self.chunks.registry().ids().water) * 8;
        };
        let sim = m.lock().unwrap_or_else(|e| e.into_inner());
        let w = ManagerFluid::new(&self.chunks);
        sim.lvl(&w, nb_core::glam::IVec3::new(x, y, z)) as u8
    }

    /// Courant de l'eau (blocs par seconde) dans la cellule.
    pub fn current_at(&self, x: i32, y: i32, z: i32) -> Vec3 {
        let Some(m) = &self.fluid else {
            return Vec3::ZERO;
        };
        let sim = m.lock().unwrap_or_else(|e| e.into_inner());
        let w = ManagerFluid::new(&self.chunks);
        sim.current_at(&w, nb_core::glam::IVec3::new(x, y, z))
    }

    /// Active ou coupe la collecte des deltas pour le réseau (hôte LAN).
    pub fn collect_fluid_deltas(&self, on: bool) {
        if let Some(m) = &self.fluid {
            m.lock()
                .unwrap_or_else(|e| e.into_inner())
                .set_collect_deltas(on);
        }
    }

    /// Changements de cellules d'eau depuis le dernier appel.
    pub fn take_fluid_deltas(&self) -> Vec<FluidDelta> {
        self.fluid.as_ref().map_or_else(Vec::new, |m| {
            m.lock().unwrap_or_else(|e| e.into_inner()).take_deltas()
        })
    }

    pub fn fluid_stats(&self) -> Option<FluidStats> {
        self.fluid
            .as_ref()
            .map(|m| m.lock().unwrap_or_else(|e| e.into_inner()).stats)
    }

    /// Accès à la simulation et au monde fluide (seau, commande `/water`, tests).
    pub fn with_fluid<R>(
        &self,
        f: impl FnOnce(&mut FluidSim, &mut ManagerFluid<'_>) -> R,
    ) -> Option<R> {
        let m = self.fluid.as_ref()?;
        let mut sim = m.lock().unwrap_or_else(|e| e.into_inner());
        let mut w = ManagerFluid::new(&self.chunks);
        Some(f(&mut sim, &mut w))
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
