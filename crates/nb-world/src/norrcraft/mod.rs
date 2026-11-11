//! Univers Norrcraft (plan Norrcraft ; GDD docs 01 et 02).
//!
//! Phase 0 : seul un générateur vide existe, le temps de prouver le circuit complet (création,
//! sauvegarde, palette de blocs, LAN). La session 0.4 le remplace par un monde plat de granit,
//! puis la phase 1 par le vrai relief.

use std::sync::Arc;

use crate::chunk::Chunk;
use crate::profile::{BiomeId, BiomeInfo, Systems, TerrainGenerator, WorldKind, WorldProfile};

/// Générateur sans terrain : tous les chunks sont vides (air).
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyTerrain;

impl TerrainGenerator for EmptyTerrain {
    fn fork(&self) -> Box<dyn TerrainGenerator> {
        Box::new(*self)
    }

    fn surface_height(&self, _wx: i32, _wz: i32) -> i32 {
        0
    }

    fn biome_at(&self, _wx: i32, _wz: i32) -> BiomeId {
        0
    }

    fn generate_chunk(&self, _chunk: &mut Chunk) {}
}

impl WorldProfile {
    /// Profil de Norrcraft au stade de la phase 0 : un seul biome neutre, un monde vide.
    pub fn norrcraft_empty() -> Self {
        Self::new(
            WorldKind::Norrcraft,
            vec![BiomeInfo::named("VIDE")],
            Systems::default(),
            Arc::new(EmptyTerrain),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_profile_is_norrcraft_with_one_biome() {
        let p = WorldProfile::norrcraft_empty();
        assert_eq!(p.kind, WorldKind::Norrcraft);
        assert_eq!(p.biomes.len(), 1);
        assert_eq!(p.generator().biome_at(5, -9), 0);
        let mut chunk = Chunk::new(nb_core::ChunkPos::new(0, 0));
        p.new_generator().generate_chunk(&mut chunk);
        assert!(chunk.voxels().iter().all(|&b| b == 0));
    }
}
