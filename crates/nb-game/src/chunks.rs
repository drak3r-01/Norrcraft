//! Raccord nb-world → nb-mesh : [`ChunkSource`] sur le `ChunkManager` et [`BiomeSource`] sur
//! un profil d'univers (types enveloppes, aucun des deux crates ne connaît l'autre).

use std::sync::Arc;

use nb_core::ChunkPos;
use nb_mesh::{BiomeId, BiomeSource, BiomeTints, ChunkSource, ChunkVoxels};
use nb_world::{ChunkManager, TerrainGenerator, WorldProfile};

/// Vue « maillage » des chunks actifs d'un monde.
#[derive(Clone, Copy, Debug)]
pub struct WorldChunks<'a>(pub &'a ChunkManager);

impl ChunkSource for WorldChunks<'_> {
    fn is_loaded(&self, p: ChunkPos) -> bool {
        self.0.chunk(p).is_some()
    }

    fn mesh_dirty(&self, p: ChunkPos) -> bool {
        self.0.chunk(p).is_some_and(|c| c.flags.is_mesh_dirty())
    }

    fn light_dirty(&self, p: ChunkPos) -> bool {
        self.0.chunk(p).is_some_and(|c| c.flags.is_light_dirty())
    }

    fn set_mesh_dirty(&mut self, p: ChunkPos, v: bool) {
        if let Some(c) = self.0.chunk(p) {
            c.flags.set_mesh_dirty(v);
        }
    }

    fn snapshot(&self, p: ChunkPos) -> Option<Arc<ChunkVoxels>> {
        // Même ordre d'index des deux côtés (`x + 16·y + 2048·z`).
        let c = self.0.chunk(p)?;
        let voxels = c.read().voxels().to_vec();
        Some(Arc::new(ChunkVoxels::from_vec(voxels)))
    }
}

/// Positions actives triées par (x, z) : ordre déterministe pour le planificateur (le Java
/// itère un `ConcurrentHashMap`).
pub fn sorted_active_positions(m: &ChunkManager) -> Vec<ChunkPos> {
    let mut v = m.active_positions();
    v.sort_by_key(|p| (p.x, p.z));
    v
}

/// Positions actives triées par distance croissante au chunk `center` (puis par (x, z) : ordre
/// déterministe). Le planificateur de maillage soumet dans cet ordre : les chunks autour du
/// joueur sont maillés avant ceux du fond de la zone.
pub fn sorted_active_positions_near(m: &ChunkManager, center: ChunkPos) -> Vec<ChunkPos> {
    let mut v = m.active_positions();
    v.sort_by_key(|p| {
        let dx = i64::from(p.x) - i64::from(center.x);
        let dz = i64::from(p.z) - i64::from(center.z);
        (dx * dx + dz * dz, p.x, p.z)
    });
    v
}

/// Biomes du monde pour le mailleur : un générateur du profil (lecture seule, partageable entre
/// threads) et la table de teintes du profil.
#[derive(Clone, Debug)]
pub struct WorldBiomes {
    terrain: Arc<dyn TerrainGenerator>,
    profile: Arc<WorldProfile>,
}

impl WorldBiomes {
    /// Biomes d'un profil : le générateur est une copie indépendante de celui du profil.
    pub fn new(profile: Arc<WorldProfile>) -> Self {
        Self {
            terrain: Arc::from(profile.new_generator()),
            profile,
        }
    }
}

impl BiomeSource for WorldBiomes {
    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId {
        self.terrain.biome_at(wx, wz)
    }

    fn tints(&self, biome: BiomeId) -> BiomeTints {
        let c = self.profile.biome(biome);
        BiomeTints {
            grass: c.grass_tint,
            leaves: c.leaves_tint,
            water: c.water_color,
        }
    }

    fn tint_blend_radius(&self) -> i32 {
        self.profile.tint_blend
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::BlockRegistry;
    use nb_world::{InMemoryChunkStorage, WorldConfig};
    use std::time::{Duration, Instant};

    fn manager() -> ChunkManager {
        let reg = Arc::new(BlockRegistry::builtin());
        let m = ChunkManager::with_threads(
            Arc::new(InMemoryChunkStorage::new()),
            42,
            Arc::new(WorldConfig::default()),
            reg,
            1,
            1,
        );
        m.set_render_distance(2);
        m
    }

    fn wait_loaded(m: &ChunkManager, n: usize) {
        let t = Instant::now();
        loop {
            m.update(nb_core::glam::Vec3::new(8.0, 80.0, 8.0));
            if m.active_positions().len() >= n && m.pending() == (0, 0) {
                return;
            }
            assert!(
                t.elapsed() < Duration::from_secs(60),
                "chargement trop long"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn chunk_source_reflects_manager() {
        let m = manager();
        wait_loaded(&m, 25);
        let p = ChunkPos::new(0, 0);
        let mut src = WorldChunks(&m);
        assert!(src.is_loaded(p));
        assert!(!src.is_loaded(ChunkPos::new(50, 50)));
        assert!(src.snapshot(ChunkPos::new(50, 50)).is_none());
        // Instantané fidèle aux blocs du monde.
        let snap = src.snapshot(p).expect("snapshot");
        for (x, y, z) in [(0, 0, 0), (3, 40, 7), (15, 127, 15), (8, 64, 2)] {
            assert_eq!(
                snap.get(x, y, z),
                m.block(x as i32, y as i32, z as i32),
                "({x},{y},{z})"
            );
        }
        src.set_mesh_dirty(p, false);
        assert!(!src.mesh_dirty(p));
        src.set_mesh_dirty(p, true);
        assert!(src.mesh_dirty(p));
        src.set_mesh_dirty(ChunkPos::new(50, 50), true); // sans effet, sans panique
        let sorted = sorted_active_positions(&m);
        assert!(sorted
            .windows(2)
            .all(|w| (w[0].x, w[0].z) < (w[1].x, w[1].z)));
        m.shutdown();
    }

    #[test]
    fn scheduler_meshes_world_chunks() {
        use nb_mesh::{BlockTable, MeshAction, MeshScheduler};
        let m = manager();
        wait_loaded(&m, 25);
        let reg = BlockRegistry::builtin();
        let table = Arc::new(BlockTable::from_registry(&reg, |_| 0));
        let profile = Arc::new(WorldProfile::vanilla(
            42,
            Arc::new(WorldConfig::default()),
            reg.ids(),
        ));
        let biomes: Arc<dyn BiomeSource> = Arc::new(WorldBiomes::new(profile));
        let mut s = MeshScheduler::new(1);
        let active = sorted_active_positions(&m);
        let t = Instant::now();
        let mut uploaded = std::collections::BTreeSet::new();
        while t.elapsed() < Duration::from_secs(60) {
            for a in s.update(&mut WorldChunks(&m), &active, table.clone(), biomes.clone()) {
                if let MeshAction::Upload(p, _) = a {
                    uploaded.insert((p.x, p.z));
                }
            }
            if uploaded.contains(&(0, 0)) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        // Rayon 2 : seuls les chunks jusqu'à r − 2 = 0 sont maillables (spec nb-world §11.7).
        assert!(uploaded.contains(&(0, 0)), "{uploaded:?}");
        assert!(!uploaded.contains(&(2, 0)));
        assert!(!WorldChunks(&m).mesh_dirty(ChunkPos::new(0, 0)));
        s.shutdown();
        m.shutdown();
    }

    #[test]
    fn biomes_match_world_profile() {
        let reg = BlockRegistry::builtin();
        let profile = Arc::new(WorldProfile::vanilla(
            7,
            Arc::new(WorldConfig::default()),
            reg.ids(),
        ));
        let g = profile.new_generator();
        let b = WorldBiomes::new(profile.clone());
        for (x, z) in [(0, 0), (100, -300), (-5000, 1234)] {
            let id = b.biome_at(x, z);
            assert_eq!(id, g.biome_at(x, z));
            assert_eq!(b.tints(id).water, profile.biome(id).water_color);
        }
        // Identifiant inconnu : premier biome du profil (plaine pour vanilla).
        assert_eq!(b.tints(200).grass, profile.biome(0).grass_tint);
        assert_eq!(profile.biome(0).name, "PLAINS");
    }
}
