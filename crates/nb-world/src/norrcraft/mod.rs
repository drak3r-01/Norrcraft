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
//!
//! Session 2.1 : biomes ([`biomes`] : 25 biomes, étagement, règles de sélection, frontières
//! tramées ; teintes, ambiances et sols dans `assets/norrcraft/biomes.json`), sols par biome dans
//! [`terrain`], teintes moyennées sur 5 × 5 colonnes par le mailleur.
//!
//! Session 2.2 : sous-sol ([`subsoil`] : roche des provinces, djupberg, filons de diabase, minerais
//! en plaques, filons et corps, cavités noyées sous la mer ou d'air au-dessus).
//!
//! Session 2.3 : arbres ([`trees`] : 12 essences, bois mort, placement en grille jitterée,
//! bosquets, peuplements purs par taches ; indépendants de l'ordre des chunks).

pub mod biomes;
pub mod climate;
pub mod config;
pub mod fields;
pub mod flora;
pub mod height;
pub mod hydro;
pub mod landforms;
pub mod math;
pub mod subsoil;
pub mod terrain;
pub mod trees;

pub use biomes::{
    biome_defs, biome_infos, BiomeDef, BiomeSelector, BiomeSoil, NorrBiome, Selection, DITHER_BAND,
};
pub use climate::{altitudes, temperature_at, Altitudes, ClimateZone};
pub use config::{NorrConfig, NorrPreset, DEFAULT_SEA_LEVEL, DEFAULT_WORLD_HEIGHT};
pub use flora::{FloraBlocks, Understory};
pub use fields::{ChunkFields, FieldGrid, MacroFields, MacroSample, Province, COAST_C};
pub use height::{HeightFields, Relief};
pub use hydro::{Hydrography, Hydrology, Lake, LakeSet, SeaFacade, WaterColumn, WaterKind};
pub use landforms::{Coast, Forms};
pub use subsoil::{DepositInfo, Region, SiteProbe, Sites, SubColumn, Subsoil, SubsoilBlocks};
pub use terrain::{NorrBlocks, NorrcraftTerrain};
pub use trees::{
    Kind as TreeKind, Plant, Species, Style as TreeStyle, TreeBlocks, TreeColumn, TreeProbe, Trees,
};

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

/// Rayon du carré de colonnes dont le mailleur moyenne les teintes de biome (2 : 5 × 5).
pub const TINT_BLEND_RADIUS: i32 = 2;

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

    /// Profil de Norrcraft avec le vrai relief (session 1.2) et ses 25 biomes (session 2.1) ;
    /// teintes moyennées sur 5 × 5 colonnes (doc 02 §7.4).
    pub fn norrcraft(seed: i64, cfg: NorrConfig, registry: &BlockRegistry) -> Self {
        Self::new(
            WorldKind::Norrcraft,
            biome_infos(),
            Systems::default(),
            Arc::new(NorrcraftTerrain::new(
                seed,
                cfg,
                NorrBlocks::from_registry(registry),
            )),
        )
        .with_tint_blend(TINT_BLEND_RADIUS)
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
        // La strate basse (session 2.4) occupe la cellule au-dessus du sol.
        let plant = |id| reg.from_id(id).cross_shaped;
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
                        assert!(above == water || plant(above), "eau ou plante au fond");
                        let top = chunk.block(x, c.level - 1, z);
                        assert!(top == water || plant(top), "eau ou nénuphar en surface");
                        assert_eq!(chunk.block(x, c.level, z), 0);
                    } else {
                        assert!(above == 0 || plant(above), "air ou plante");
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
