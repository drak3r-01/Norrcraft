//! Biomes (`Biome.java`, `TreeType.java`, `BiomeSystem.java`).

use std::sync::Arc;

use nb_data::{BlockId, BlockIds};

use crate::config::WorldConfig;
use crate::noise::Noise;

/// Nombre de biomes (taille de `treeDensityOverrides`).
pub const BIOME_COUNT: usize = 8;

/// Biome d'une colonne. Les ordinaux sont ceux du Java.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Biome {
    Plains = 0,
    Forest = 1,
    Desert = 2,
    Taiga = 3,
    Jungle = 4,
    Swamp = 5,
    Savanna = 6,
    Mountains = 7,
}

impl Biome {
    pub const ALL: [Biome; BIOME_COUNT] = [
        Biome::Plains,
        Biome::Forest,
        Biome::Desert,
        Biome::Taiga,
        Biome::Jungle,
        Biome::Swamp,
        Biome::Savanna,
        Biome::Mountains,
    ];

    #[inline]
    pub const fn ordinal(self) -> usize {
        self as usize
    }

    /// Nom Java (`PLAINS`, …).
    pub const fn name(self) -> &'static str {
        match self {
            Biome::Plains => "PLAINS",
            Biome::Forest => "FOREST",
            Biome::Desert => "DESERT",
            Biome::Taiga => "TAIGA",
            Biome::Jungle => "JUNGLE",
            Biome::Swamp => "SWAMP",
            Biome::Savanna => "SAVANNA",
            Biome::Mountains => "MOUNTAINS",
        }
    }
}

/// Forme d'arbre (BIRCH n'est utilisé par aucun biome).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeType {
    Oak,
    Birch,
    Spruce,
    Jungle,
    Acacia,
    SwampOak,
}

/// Plante candidate d'un biome (liste ordonnée).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VegetationEntry {
    pub block: BlockId,
    pub chance: f64,
}

/// Paramètres fixes d'un biome (spec §6.3).
#[derive(Debug, Clone, PartialEq)]
pub struct BiomeConfig {
    pub surface: BlockId,
    pub subsurface: BlockId,
    pub deep: BlockId,
    pub tree_type: TreeType,
    pub tree_density: f64,
    pub vegetation: Vec<VegetationEntry>,
    pub height_modifier: f64,
    pub grass_tint: [f64; 3],
    pub leaves_tint: [f64; 3],
    pub water_color: [f64; 3],
}

const TEMP_OFFSET_X: f64 = 500.0;
const TEMP_OFFSET_Z: f64 = 500.0;
const HUMID_OFFSET_X: f64 = -300.0;
const HUMID_OFFSET_Z: f64 = 800.0;

/// Sélection du biome par bruit (température, humidité, montagnes).
#[derive(Debug, Clone)]
pub struct BiomeSystem {
    noise: Arc<Noise>,
    cfg: Arc<WorldConfig>,
    configs: [BiomeConfig; BIOME_COUNT],
    snow: BlockId,
}

fn veg(entries: &[(BlockId, f64)]) -> Vec<VegetationEntry> {
    entries
        .iter()
        .map(|&(block, chance)| VegetationEntry { block, chance })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn bc(
    surface: BlockId,
    subsurface: BlockId,
    deep: BlockId,
    tree_type: TreeType,
    tree_density: f64,
    vegetation: Vec<VegetationEntry>,
    height_modifier: f64,
    grass_tint: [f64; 3],
    leaves_tint: [f64; 3],
    water_color: [f64; 3],
) -> BiomeConfig {
    BiomeConfig {
        surface,
        subsurface,
        deep,
        tree_type,
        tree_density,
        vegetation,
        height_modifier,
        grass_tint,
        leaves_tint,
        water_color,
    }
}

fn build_configs(b: &BlockIds) -> [BiomeConfig; BIOME_COUNT] {
    [
        bc(
            b.grass,
            b.dirt,
            b.stone,
            TreeType::Oak,
            0.004,
            veg(&[
                (b.tall_grass, 0.30),
                (b.flower_red, 0.02),
                (b.flower_yellow, 0.02),
            ]),
            0.7,
            [0.45, 0.74, 0.19],
            [0.36, 0.73, 0.24],
            [0.2, 0.5, 0.8],
        ),
        bc(
            b.grass,
            b.dirt,
            b.stone,
            TreeType::Oak,
            0.025,
            veg(&[
                (b.tall_grass, 0.20),
                (b.flower_red, 0.01),
                (b.mushroom_brown, 0.005),
            ]),
            0.85,
            [0.35, 0.68, 0.15],
            [0.30, 0.65, 0.18],
            [0.18, 0.45, 0.7],
        ),
        bc(
            b.sand,
            b.sand,
            b.sandstone,
            TreeType::Oak,
            0.0,
            veg(&[(b.dead_bush, 0.01), (b.cactus, 0.003)]),
            0.5,
            [0.6, 0.65, 0.2],
            [0.5, 0.6, 0.2],
            [0.3, 0.6, 0.7],
        ),
        bc(
            b.grass,
            b.dirt,
            b.stone,
            TreeType::Spruce,
            0.018,
            veg(&[
                (b.fern, 0.18),
                (b.tall_grass, 0.08),
                (b.mushroom_red, 0.004),
            ]),
            0.9,
            [0.30, 0.55, 0.28],
            [0.18, 0.42, 0.18],
            [0.15, 0.35, 0.65],
        ),
        bc(
            b.grass,
            b.dirt,
            b.stone,
            TreeType::Jungle,
            0.035,
            veg(&[
                (b.fern, 0.30),
                (b.tall_grass, 0.25),
                (b.flower_red, 0.01),
                (b.flower_yellow, 0.01),
            ]),
            1.0,
            [0.22, 0.78, 0.15],
            [0.15, 0.72, 0.12],
            [0.1, 0.55, 0.55],
        ),
        bc(
            b.grass,
            b.dirt,
            b.clay,
            TreeType::SwampOak,
            0.012,
            veg(&[
                (b.tall_grass, 0.25),
                (b.mushroom_brown, 0.01),
                (b.mushroom_red, 0.005),
                (b.lily_pad, 0.04),
            ]),
            0.4,
            [0.40, 0.55, 0.22],
            [0.35, 0.50, 0.18],
            [0.25, 0.4, 0.2],
        ),
        bc(
            b.grass,
            b.dirt,
            b.stone,
            TreeType::Acacia,
            0.005,
            veg(&[(b.tall_grass, 0.35), (b.dead_bush, 0.008)]),
            0.55,
            [0.62, 0.68, 0.18],
            [0.55, 0.65, 0.15],
            [0.25, 0.55, 0.65],
        ),
        bc(
            b.stone,
            b.stone,
            b.stone,
            TreeType::Spruce,
            0.006,
            veg(&[(b.tall_grass, 0.05), (b.fern, 0.02)]),
            1.8,
            [0.35, 0.60, 0.22],
            [0.25, 0.50, 0.20],
            [0.15, 0.4, 0.75],
        ),
    ]
}

impl BiomeSystem {
    pub fn new(noise: Arc<Noise>, cfg: Arc<WorldConfig>, ids: &BlockIds) -> Self {
        Self {
            noise,
            cfg,
            configs: build_configs(ids),
            snow: ids.snow,
        }
    }

    pub fn temperature(&self, wx: f64, wz: f64) -> f64 {
        let s = self.cfg.temp_scale;
        self.noise.fbm2d(
            (wx + TEMP_OFFSET_X) * s,
            (wz + TEMP_OFFSET_Z) * s,
            3,
            2.0,
            0.45,
        )
    }

    pub fn humidity(&self, wx: f64, wz: f64) -> f64 {
        let s = self.cfg.humid_scale;
        self.noise.fbm2d(
            (wx + HUMID_OFFSET_X) * s,
            (wz + HUMID_OFFSET_Z) * s,
            3,
            2.0,
            0.45,
        )
    }

    pub fn mountain_factor(&self, wx: f64, wz: f64) -> f64 {
        let f = self.cfg.mountain_frequency;
        self.noise
            .fbm2d(wx * f + 1000.0, wz * f + 1000.0, 4, 2.0, 0.5)
    }

    /// `getBiome` (spec §6.2).
    pub fn biome(&self, wx: f64, wz: f64) -> Biome {
        if self.mountain_factor(wx, wz) > self.cfg.mountain_threshold {
            return Biome::Mountains;
        }
        let temp = self.temperature(wx, wz);
        let humid = self.humidity(wx, wz);
        if temp > 0.6 {
            if humid < 0.35 {
                return Biome::Desert;
            }
            if humid > 0.6 {
                return Biome::Jungle;
            }
            return Biome::Savanna;
        }
        if temp < 0.35 {
            return Biome::Taiga;
        }
        if humid > 0.6 {
            return Biome::Swamp;
        }
        if humid > 0.4 {
            return Biome::Forest;
        }
        Biome::Plains
    }

    /// Biome d'une colonne en coordonnées entières.
    #[inline]
    pub fn biome_at(&self, wx: i32, wz: i32) -> Biome {
        self.biome(f64::from(wx), f64::from(wz))
    }

    pub fn config(&self, b: Biome) -> &BiomeConfig {
        &self.configs[b.ordinal()]
    }

    /// `getSurfaceBlock` : neige au-dessus des lignes de neige (montagnes, taïga).
    pub fn surface_block(&self, b: Biome, height: i32) -> BlockId {
        if b == Biome::Mountains && height > self.cfg.snow_line_mountains {
            return self.snow;
        }
        if b == Biome::Taiga && height > self.cfg.snow_line_taiga {
            return self.snow;
        }
        self.config(b).surface
    }

    pub fn world_config(&self) -> &WorldConfig {
        &self.cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_data::BlockRegistry;

    #[test]
    fn reference_biomes_seed_12345() {
        let reg = BlockRegistry::builtin();
        let bs = BiomeSystem::new(
            Arc::new(Noise::new(12345)),
            Arc::new(WorldConfig::default()),
            reg.ids(),
        );
        let cases: [(i32, i32, Biome, f64, f64, f64); 6] = [
            (
                0,
                0,
                Biome::Forest,
                0.4583829801850889,
                0.5767199132853252,
                0.5,
            ),
            (
                100,
                -50,
                Biome::Forest,
                0.44990601878663955,
                0.42848650605143723,
                0.4937034741759807,
            ),
            (
                -300,
                700,
                Biome::Forest,
                0.424357034795764,
                0.42862324962178516,
                0.47181624046935433,
            ),
            (
                1000,
                1000,
                Biome::Forest,
                0.46108991475761746,
                0.5182222801834794,
                0.5,
            ),
            (
                -2048,
                513,
                Biome::Taiga,
                0.2910663921521526,
                0.44903738395416154,
                0.526077648758023,
            ),
            (
                4000,
                -4000,
                Biome::Forest,
                0.516753734871407,
                0.5237990495583661,
                0.5,
            ),
        ];
        for (wx, wz, biome, t, h, m) in cases {
            let (x, z) = (f64::from(wx), f64::from(wz));
            assert_eq!(bs.biome(x, z), biome, "biome ({wx},{wz})");
            assert_eq!(bs.temperature(x, z), t, "temp ({wx},{wz})");
            assert_eq!(bs.humidity(x, z), h, "humid ({wx},{wz})");
            assert_eq!(bs.mountain_factor(x, z), m, "mountain ({wx},{wz})");
        }
    }

    #[test]
    fn snow_lines() {
        let reg = BlockRegistry::builtin();
        let ids = reg.ids();
        let bs = BiomeSystem::new(
            Arc::new(Noise::new(1)),
            Arc::new(WorldConfig::default()),
            ids,
        );
        assert_eq!(bs.surface_block(Biome::Mountains, 59), ids.snow);
        assert_eq!(bs.surface_block(Biome::Mountains, 58), ids.stone);
        assert_eq!(bs.surface_block(Biome::Taiga, 53), ids.snow);
        assert_eq!(bs.surface_block(Biome::Taiga, 52), ids.grass);
        assert_eq!(bs.surface_block(Biome::Desert, 100), ids.sand);
        assert_eq!(bs.config(Biome::Swamp).deep, ids.clay);
    }
}
