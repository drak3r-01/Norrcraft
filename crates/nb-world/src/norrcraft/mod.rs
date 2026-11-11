//! Univers Norrcraft (plan Norrcraft ; GDD docs 01 et 02).
//!
//! Phase 0 : un générateur bouchon (monde plat de granit) prouve le circuit complet (création,
//! sauvegarde, palette de blocs, LAN). La phase 1 le remplacera par le vrai relief.
//!
//! Session 1.1 : briques mathématiques ([`math`]), réglages et préréglages ([`config`]), champs
//! macroscopiques ([`fields`]) et climat ([`climate`]).
//!
//! Session 1.2 : fonction de hauteur ([`height`] : socle, collines, chaîne, plateaux, vallées en U,
//! fjords) et vrai générateur ([`terrain`]), qui remplace le monde plat de granit dans les mondes.
//!
//! Session 1.3 : formes de relief ([`landforms`] : archipel, côtes, eskers, drumlins, tourbières,
//! palses, champ de lave, glacier), et leurs blocs provisoires dans [`terrain`].
//!
//! Session 1.4 : hydrographie ([`hydro`] : niveau d'eau en escalier, lacs à margelle, rivières et
//! rapides, mer ; scellement qui rend toute nappe étanche).

pub mod climate;
pub mod config;
pub mod fields;
pub mod height;
pub mod hydro;
pub mod landforms;
pub mod math;
pub mod terrain;

pub use climate::{altitudes, temperature_at, Altitudes, ClimateZone};
pub use config::{NorrConfig, NorrPreset, DEFAULT_SEA_LEVEL, DEFAULT_WORLD_HEIGHT};
pub use fields::{ChunkFields, FieldGrid, MacroFields, MacroSample, Province, COAST_C};
pub use height::{HeightFields, Relief};
pub use hydro::{Hydrography, Hydrology, Lake, LakeSet, SeaFacade, WaterColumn, WaterKind};
pub use landforms::{Coast, Forms};
pub use terrain::{NorrBlocks, NorrcraftTerrain};

use std::sync::Arc;

use nb_data::{BlockId, BlockRegistry};

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

/// Hauteur du premier `y` libre du monde plat bouchon.
pub const FLAT_SURFACE_Y: i32 = 64;

/// Monde plat bouchon : `y = 0` socle (bedrock), `1..FLAT_SURFACE_Y` granit, air au-dessus.
#[derive(Debug, Clone, Copy)]
pub struct FlatGraniteTerrain {
    bedrock: BlockId,
    granite: BlockId,
}

impl FlatGraniteTerrain {
    pub fn new(bedrock: BlockId, granite: BlockId) -> Self {
        Self { bedrock, granite }
    }

    /// Identifiants lus dans le registre du monde (la palette peut varier d'un monde à l'autre).
    pub fn from_registry(registry: &BlockRegistry) -> Self {
        Self::new(registry.get("bedrock").id, registry.get("granit").id)
    }
}

impl TerrainGenerator for FlatGraniteTerrain {
    fn fork(&self) -> Box<dyn TerrainGenerator> {
        Box::new(*self)
    }

    fn surface_height(&self, _wx: i32, _wz: i32) -> i32 {
        FLAT_SURFACE_Y
    }

    fn biome_at(&self, _wx: i32, _wz: i32) -> BiomeId {
        0
    }

    fn generate_chunk(&self, chunk: &mut Chunk) {
        for y in 0..FLAT_SURFACE_Y {
            let id = if y == 0 { self.bedrock } else { self.granite };
            for z in 0..16 {
                for x in 0..16 {
                    chunk.set_block(x, y, z, id);
                }
            }
        }
    }

    /// Départ au centre, sur le granit, quel que soit le niveau de la mer de la configuration.
    fn find_spawn(&self, _sea_level: i32) -> [f64; 3] {
        [0.5, f64::from(FLAT_SURFACE_Y), 0.5]
    }
}

impl WorldProfile {
    /// Profil de Norrcraft sans terrain : un seul biome neutre, un monde vide.
    pub fn norrcraft_empty() -> Self {
        Self::norrcraft_with(Arc::new(EmptyTerrain))
    }

    /// Profil de Norrcraft au stade de la phase 0 : monde plat de granit (bouchon, tests).
    pub fn norrcraft_flat(registry: &BlockRegistry) -> Self {
        Self::norrcraft_with(Arc::new(FlatGraniteTerrain::from_registry(registry)))
    }

    /// Profil de Norrcraft avec le vrai relief (session 1.2). Un seul biome neutre jusqu'à la
    /// sélection des biomes (session 2.1).
    pub fn norrcraft(seed: i64, cfg: NorrConfig, registry: &BlockRegistry) -> Self {
        Self::norrcraft_with(Arc::new(NorrcraftTerrain::new(
            seed,
            cfg,
            NorrBlocks::from_registry(registry),
        )))
    }

    fn norrcraft_with(generator: Arc<dyn TerrainGenerator>) -> Self {
        Self::new(
            WorldKind::Norrcraft,
            vec![BiomeInfo::named("VIDE")],
            Systems::default(),
            generator,
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

    #[test]
    fn flat_profile_fills_granite_up_to_the_surface() {
        let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
        let p = WorldProfile::norrcraft_flat(&reg);
        let (bedrock, granit) = (reg.get("bedrock").id, reg.get("granit").id);
        let mut chunk = Chunk::new(nb_core::ChunkPos::new(3, -2));
        p.new_generator().generate_chunk(&mut chunk);
        assert_eq!(chunk.block(7, 0, 7), bedrock);
        assert_eq!(chunk.block(7, 1, 7), granit);
        assert_eq!(chunk.block(0, FLAT_SURFACE_Y - 1, 15), granit);
        assert_eq!(chunk.block(7, FLAT_SURFACE_Y, 7), 0);
        assert_eq!(p.generator().surface_height(-40, 9), FLAT_SURFACE_Y);
        assert_eq!(p.generator().find_spawn(63)[1], f64::from(FLAT_SURFACE_Y));
    }

    #[test]
    fn relief_profile_matches_surface_height_and_spawns_on_land() {
        let reg = BlockRegistry::for_kind(WorldKind::Norrcraft);
        let p = WorldProfile::norrcraft(12345, NorrConfig::default(), &reg);
        let gen = p.new_generator();
        let (water, bedrock) = (reg.get("water").id, reg.get("bedrock").id);
        let hydro = Hydrography::new(12345, NorrConfig::default());
        for (cx, cz) in [(0, 0), (-3, 5), (40, -17), (-250, 90)] {
            let mut chunk = Chunk::new(nb_core::ChunkPos::new(cx, cz));
            gen.generate_chunk(&mut chunk);
            for z in 0..16 {
                for x in 0..16 {
                    let h = p.generator().surface_height(cx * 16 + x, cz * 16 + z);
                    assert_eq!(chunk.block(x, 0, z), bedrock);
                    assert_ne!(
                        chunk.block(x, h - 1, z),
                        0,
                        "colonne pleine sous la surface"
                    );
                    assert_ne!(chunk.block(x, h - 1, z), water);
                    let c = hydro.column(cx * 16 + x, cz * 16 + z);
                    assert_eq!(c.top, h);
                    let above = chunk.block(x, h, z);
                    if c.is_wet() {
                        assert_eq!(above, water);
                        assert_eq!(chunk.block(x, c.level - 1, z), water);
                        assert_eq!(chunk.block(x, c.level, z), 0);
                    } else {
                        assert_eq!(above, 0);
                    }
                }
            }
        }
        let spawn = p.generator().find_spawn(34);
        let (sx, sz) = (spawn[0].floor() as i32, spawn[2].floor() as i32);
        assert!(p.generator().surface_height(sx, sz) >= DEFAULT_SEA_LEVEL);
        assert!(!hydro.column(sx, sz).is_wet(), "départ au sec");
    }
}
