//! Biomes vus par le mesher (spec §3.4) : identité (critère de fusion) et teintes en `f64`.

/// Identité d'un biome, comparée par égalité pour la fusion greedy.
pub type BiomeId = u8;

/// Teintes d'un biome (`BiomeConfig.grassTint`, `leavesTint`, `waterColor`), en **double**.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiomeTints {
    pub grass: [f64; 3],
    pub leaves: [f64; 3],
    pub water: [f64; 3],
}

/// Source de biomes (implémentée par la génération du monde).
pub trait BiomeSource: Send + Sync {
    /// `BiomeSystem.getBiome(wx, wz)`, appelé avec les coordonnées monde entières du bloc.
    /// Doit être pure (le mesher peut la mettre en cache par colonne).
    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId;
    /// `BiomeSystem.getConfig(biome)` réduit aux teintes.
    fn tints(&self, biome: BiomeId) -> BiomeTints;
}

/// Ordre de l'enum Java `Biome`.
pub mod java_biome {
    use super::BiomeId;
    pub const PLAINS: BiomeId = 0;
    pub const FOREST: BiomeId = 1;
    pub const DESERT: BiomeId = 2;
    pub const TAIGA: BiomeId = 3;
    pub const JUNGLE: BiomeId = 4;
    pub const SWAMP: BiomeId = 5;
    pub const SAVANNA: BiomeId = 6;
    pub const MOUNTAINS: BiomeId = 7;
}

/// Teintes des 8 biomes du Java (`BiomeSystem.setupConfigs`), indexées par [`java_biome`].
pub const JAVA_BIOME_TINTS: [BiomeTints; 8] = [
    BiomeTints {
        grass: [0.45, 0.74, 0.19],
        leaves: [0.36, 0.73, 0.24],
        water: [0.2, 0.5, 0.8],
    },
    BiomeTints {
        grass: [0.35, 0.68, 0.15],
        leaves: [0.30, 0.65, 0.18],
        water: [0.18, 0.45, 0.7],
    },
    BiomeTints {
        grass: [0.6, 0.65, 0.2],
        leaves: [0.5, 0.6, 0.2],
        water: [0.3, 0.6, 0.7],
    },
    BiomeTints {
        grass: [0.30, 0.55, 0.28],
        leaves: [0.18, 0.42, 0.18],
        water: [0.15, 0.35, 0.65],
    },
    BiomeTints {
        grass: [0.22, 0.78, 0.15],
        leaves: [0.15, 0.72, 0.12],
        water: [0.1, 0.55, 0.55],
    },
    BiomeTints {
        grass: [0.40, 0.55, 0.22],
        leaves: [0.35, 0.50, 0.18],
        water: [0.25, 0.4, 0.2],
    },
    BiomeTints {
        grass: [0.62, 0.68, 0.18],
        leaves: [0.55, 0.65, 0.15],
        water: [0.25, 0.55, 0.65],
    },
    BiomeTints {
        grass: [0.35, 0.60, 0.22],
        leaves: [0.25, 0.50, 0.20],
        water: [0.15, 0.4, 0.75],
    },
];

/// Teintes Java d'un biome (identifiant hors `0..8` → PLAINS).
pub fn java_tints(biome: BiomeId) -> BiomeTints {
    JAVA_BIOME_TINTS
        .get(usize::from(biome))
        .copied()
        .unwrap_or(JAVA_BIOME_TINTS[0])
}

/// Un seul biome partout (tests, outils).
#[derive(Clone, Copy, Debug)]
pub struct UniformBiome(pub BiomeId);

impl BiomeSource for UniformBiome {
    fn biome_at(&self, _wx: i32, _wz: i32) -> BiomeId {
        self.0
    }
    fn tints(&self, biome: BiomeId) -> BiomeTints {
        java_tints(biome)
    }
}

/// Adapte une fonction `(wx, wz) -> BiomeId` avec les teintes Java.
pub struct FnBiomes<F>(pub F);

impl<F: Fn(i32, i32) -> BiomeId + Send + Sync> BiomeSource for FnBiomes<F> {
    fn biome_at(&self, wx: i32, wz: i32) -> BiomeId {
        (self.0)(wx, wz)
    }
    fn tints(&self, biome: BiomeId) -> BiomeTints {
        java_tints(biome)
    }
}
