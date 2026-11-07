//! Biomes (`Biome.java`, `TreeType.java`, `BiomeSystem.java`).

use std::sync::Arc;

use nb_data::{BlockId, BlockIds};

use crate::config::{GeneratorKind, WorldConfig};
use crate::noise::Noise;
use crate::terrain::Terrain;

/// Nombre de biomes du générateur Java (taille de `treeDensityOverrides`).
pub const CLASSIC_BIOME_COUNT: usize = 8;
/// Nombre total de biomes (Java + ajouts du générateur « Norrblock »).
pub const BIOME_COUNT: usize = 30;

/// Biome d'une colonne. Les 8 premiers ordinaux sont ceux du Java ; les suivants sont propres au
/// générateur « Norrblock » (le générateur classique ne les produit jamais).
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
    Ocean = 8,
    DeepOcean = 9,
    WarmOcean = 10,
    FrozenOcean = 11,
    River = 12,
    Beach = 13,
    StonyShore = 14,
    BirchForest = 15,
    DarkForest = 16,
    FlowerMeadow = 17,
    CherryGrove = 18,
    AutumnForest = 19,
    SnowyPlains = 20,
    SnowyTaiga = 21,
    IceSpikes = 22,
    Heath = 23,
    MegaTaiga = 24,
    Badlands = 25,
    Hills = 26,
    SnowyPeaks = 27,
    Volcano = 28,
    MushroomFields = 29,
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
        Biome::Ocean,
        Biome::DeepOcean,
        Biome::WarmOcean,
        Biome::FrozenOcean,
        Biome::River,
        Biome::Beach,
        Biome::StonyShore,
        Biome::BirchForest,
        Biome::DarkForest,
        Biome::FlowerMeadow,
        Biome::CherryGrove,
        Biome::AutumnForest,
        Biome::SnowyPlains,
        Biome::SnowyTaiga,
        Biome::IceSpikes,
        Biome::Heath,
        Biome::MegaTaiga,
        Biome::Badlands,
        Biome::Hills,
        Biome::SnowyPeaks,
        Biome::Volcano,
        Biome::MushroomFields,
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
            Biome::Ocean => "OCEAN",
            Biome::DeepOcean => "DEEP_OCEAN",
            Biome::WarmOcean => "WARM_OCEAN",
            Biome::FrozenOcean => "FROZEN_OCEAN",
            Biome::River => "RIVER",
            Biome::Beach => "BEACH",
            Biome::StonyShore => "STONY_SHORE",
            Biome::BirchForest => "BIRCH_FOREST",
            Biome::DarkForest => "DARK_FOREST",
            Biome::FlowerMeadow => "FLOWER_MEADOW",
            Biome::CherryGrove => "CHERRY_GROVE",
            Biome::AutumnForest => "AUTUMN_FOREST",
            Biome::SnowyPlains => "SNOWY_PLAINS",
            Biome::SnowyTaiga => "SNOWY_TAIGA",
            Biome::IceSpikes => "ICE_SPIKES",
            Biome::Heath => "HEATH",
            Biome::MegaTaiga => "MEGA_TAIGA",
            Biome::Badlands => "BADLANDS",
            Biome::Hills => "HILLS",
            Biome::SnowyPeaks => "SNOWY_PEAKS",
            Biome::Volcano => "VOLCANO",
            Biome::MushroomFields => "MUSHROOM_FIELDS",
        }
    }

    /// Biome du Java dont ce biome hérite le réglage `treeDensityOverrides` (l'interface ne
    /// propose que 8 curseurs).
    pub const fn family(self) -> Biome {
        match self {
            Biome::Plains
            | Biome::FlowerMeadow
            | Biome::Beach
            | Biome::SnowyPlains
            | Biome::Heath => Biome::Plains,
            Biome::Forest
            | Biome::BirchForest
            | Biome::DarkForest
            | Biome::CherryGrove
            | Biome::AutumnForest
            | Biome::Hills => Biome::Forest,
            Biome::Desert | Biome::Badlands => Biome::Desert,
            Biome::Taiga | Biome::SnowyTaiga | Biome::MegaTaiga => Biome::Taiga,
            Biome::Jungle => Biome::Jungle,
            Biome::Swamp | Biome::MushroomFields => Biome::Swamp,
            Biome::Savanna => Biome::Savanna,
            Biome::Mountains
            | Biome::SnowyPeaks
            | Biome::StonyShore
            | Biome::Volcano
            | Biome::IceSpikes => Biome::Mountains,
            Biome::Ocean
            | Biome::DeepOcean
            | Biome::WarmOcean
            | Biome::FrozenOcean
            | Biome::River => Biome::Plains,
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
    // Ajouts du générateur « Norrblock ».
    /// Grand chêne à branches.
    Fancy,
    DarkOak,
    Cherry,
    /// Épicéa géant à tronc de 2×2.
    MegaSpruce,
    /// Jungle géante à tronc de 2×2.
    MegaJungle,
    Palm,
    Dead,
    Bush,
    HugeRedMushroom,
    HugeBrownMushroom,
    // Ajouts de la phase 9 (flore).
    /// Pommier (feuilles à pommes), petit et rond.
    Apple,
    /// Saule pleureur : couronne retombante.
    Willow,
    /// Baobab : tronc renflé, branches en candélabre.
    Baobab,
    /// Séquoia : tronc 2×2 très haut, cime étroite.
    Redwood,
    /// Cactus candélabre à bras.
    Saguaro,
    /// Arbre-parasol (dragonnier) à plateau de feuilles.
    Umbrella,
    /// Bosquet de bambous.
    BambooGrove,
    /// Genévrier : petit conifère trapu.
    Juniper,
}

/// Natures de sol acceptées par une plante de [`FloraEntry`].
pub const SOIL_GRASS: u8 = 1;
pub const SOIL_SAND: u8 = 2;
pub const SOIL_SNOW: u8 = 4;
pub const SOIL_ASH: u8 = 8;

/// Plante de la passe « flore » (Norrblock uniquement : le générateur Java ne la lit pas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FloraEntry {
    pub block: BlockId,
    pub chance: f64,
    /// Masque de `SOIL_*`.
    pub soil: u8,
    /// Seulement dans les plaques d'un bruit lent (champs, parterres).
    pub patchy: bool,
    /// Seulement au contact de l'eau.
    pub wet: bool,
    /// Hauteur de la tige en blocs (min, max).
    pub height: (u8, u8),
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
    /// Mélange d'arbres pondéré du générateur « Norrblock » (vide : `tree_type` seul).
    pub tree_mix: Vec<(TreeType, u32)>,
    pub vegetation: Vec<VegetationEntry>,
    /// Plantes supplémentaires du générateur « Norrblock » (voir [`FloraEntry`]).
    pub flora: Vec<FloraEntry>,
    /// Densité d'arbres propre à Norrblock (remplace `tree_density` quand elle est définie).
    pub nb_tree_density: Option<f64>,
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
    configs: Vec<BiomeConfig>,
    terrain: Terrain,
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
        tree_mix: Vec::new(),
        vegetation,
        flora: Vec::new(),
        nb_tree_density: None,
        height_modifier,
        grass_tint,
        leaves_tint,
        water_color,
    }
}

fn build_configs(b: &BlockIds) -> Vec<BiomeConfig> {
    let mut v = vec![
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
    ];
    v.extend(norrblock_configs(b));
    apply_classic_mixes(&mut v);
    apply_nature(&mut v, b);
    v
}

impl BiomeConfig {
    fn mix(mut self, m: &[(TreeType, u32)]) -> Self {
        self.tree_mix = m.to_vec();
        self
    }
}

/// Mélanges d'arbres du générateur « Norrblock » pour les 8 biomes du Java.
fn apply_classic_mixes(v: &mut [BiomeConfig]) {
    use TreeType::*;
    let set = |v: &mut [BiomeConfig], b: Biome, m: &[(TreeType, u32)]| {
        v[b.ordinal()].tree_mix = m.to_vec();
    };
    set(v, Biome::Plains, &[(Oak, 5), (Fancy, 2), (Bush, 3)]);
    set(v, Biome::Forest, &[(Oak, 11), (Birch, 4), (Fancy, 5)]);
    set(v, Biome::Taiga, &[(Spruce, 9), (Bush, 1)]);
    set(
        v,
        Biome::Jungle,
        &[(Jungle, 12), (MegaJungle, 5), (Bush, 3)],
    );
    set(v, Biome::Swamp, &[(SwampOak, 1)]);
    set(v, Biome::Savanna, &[(Acacia, 7), (Bush, 2), (Dead, 1)]);
    set(v, Biome::Mountains, &[(Spruce, 1)]);
}

fn fl(block: BlockId, chance: f64, soil: u8) -> FloraEntry {
    FloraEntry {
        block,
        chance,
        soil,
        patchy: false,
        wet: false,
        height: (1, 1),
    }
}

impl FloraEntry {
    fn patchy(mut self) -> Self {
        self.patchy = true;
        self
    }
    fn wet(mut self) -> Self {
        self.wet = true;
        self
    }
    fn tall(mut self, lo: u8, hi: u8) -> Self {
        self.height = (lo, hi);
        self
    }
}

/// Flore et arbres de la phase 9 : plantes par biome, nouveaux mélanges d'arbres.
/// N'affecte que le générateur « Norrblock » (le Java ne lit ni `flora` ni `tree_mix`).
fn apply_nature(v: &mut [BiomeConfig], b: &BlockIds) {
    use TreeType::*;
    let g = SOIL_GRASS;
    let (sand, snow, ash) = (SOIL_SAND, SOIL_SNOW, SOIL_ASH);
    let flora = |v: &mut [BiomeConfig], biome: Biome, list: Vec<FloraEntry>| {
        v[biome.ordinal()].flora = list;
    };
    let add_trees = |v: &mut [BiomeConfig], biome: Biome, extra: &[(TreeType, u32)]| {
        v[biome.ordinal()].tree_mix.extend_from_slice(extra);
    };

    flora(
        v,
        Biome::Plains,
        vec![
            fl(b.sunflower, 0.10, g).patchy(),
            fl(b.allium, 0.006, g),
            fl(b.berry_bush, 0.004, g),
            fl(b.cattail, 0.10, g).wet(),
        ],
    );
    flora(
        v,
        Biome::Forest,
        vec![
            fl(b.berry_bush, 0.008, g),
            fl(b.lily_of_the_valley, 0.05, g).patchy(),
            fl(b.allium, 0.004, g),
            fl(b.cattail, 0.08, g).wet(),
        ],
    );
    flora(
        v,
        Biome::BirchForest,
        vec![
            fl(b.lily_of_the_valley, 0.12, g).patchy(),
            fl(b.allium, 0.008, g),
            fl(b.berry_bush, 0.004, g),
        ],
    );
    flora(v, Biome::DarkForest, vec![fl(b.berry_bush, 0.006, g)]);
    flora(
        v,
        Biome::FlowerMeadow,
        vec![
            fl(b.sunflower, 0.12, g).patchy(),
            fl(b.allium, 0.05, g).patchy(),
            fl(b.blue_orchid, 0.01, g),
        ],
    );
    flora(
        v,
        Biome::CherryGrove,
        vec![
            fl(b.allium, 0.02, g).patchy(),
            fl(b.lily_of_the_valley, 0.03, g),
            fl(b.bamboo, 0.03, g).patchy().tall(3, 7),
        ],
    );
    flora(
        v,
        Biome::AutumnForest,
        vec![fl(b.pumpkin, 0.02, g).patchy(), fl(b.berry_bush, 0.012, g)],
    );
    flora(
        v,
        Biome::Taiga,
        vec![
            fl(b.berry_bush, 0.012, g),
            fl(b.lily_of_the_valley, 0.01, g),
        ],
    );
    flora(v, Biome::MegaTaiga, vec![fl(b.berry_bush, 0.012, g)]);
    flora(
        v,
        Biome::SnowyTaiga,
        vec![fl(b.cotton_grass, 0.03, snow | g)],
    );
    flora(
        v,
        Biome::SnowyPlains,
        vec![fl(b.cotton_grass, 0.05, snow | g).patchy()],
    );
    flora(
        v,
        Biome::Heath,
        vec![
            fl(b.cotton_grass, 0.10, g).patchy(),
            fl(b.berry_bush, 0.012, g),
            fl(b.allium, 0.02, g).patchy(),
        ],
    );
    flora(
        v,
        Biome::Hills,
        vec![fl(b.berry_bush, 0.006, g), fl(b.allium, 0.006, g)],
    );
    flora(
        v,
        Biome::Savanna,
        vec![
            fl(b.pampas_grass, 0.22, g).patchy(),
            fl(b.sunflower, 0.006, g),
        ],
    );
    flora(
        v,
        Biome::Jungle,
        vec![
            fl(b.bamboo, 0.07, g).patchy().tall(3, 8),
            fl(b.hibiscus, 0.03, g),
            fl(b.blue_orchid, 0.012, g),
            fl(b.cattail, 0.10, g).wet(),
        ],
    );
    flora(
        v,
        Biome::Swamp,
        vec![fl(b.cattail, 0.22, g).wet(), fl(b.blue_orchid, 0.04, g)],
    );
    flora(v, Biome::Desert, vec![fl(b.desert_sage, 0.012, sand)]);
    flora(v, Biome::Badlands, vec![fl(b.desert_sage, 0.015, sand)]);
    flora(v, Biome::Volcano, vec![fl(b.ember_bloom, 0.05, ash)]);
    flora(v, Biome::Beach, vec![fl(b.cattail, 0.05, sand).wet()]);

    // Arbres : mélanges enrichis.
    add_trees(v, Biome::Plains, &[(Apple, 1)]);
    add_trees(v, Biome::Forest, &[(Apple, 1)]);
    add_trees(v, Biome::AutumnForest, &[(Apple, 3)]);
    add_trees(v, Biome::FlowerMeadow, &[(Apple, 1)]);
    add_trees(v, Biome::Swamp, &[(Willow, 2)]);
    add_trees(v, Biome::Savanna, &[(Baobab, 1), (Umbrella, 2)]);
    add_trees(v, Biome::MegaTaiga, &[(Redwood, 2)]);
    add_trees(v, Biome::Taiga, &[(Redwood, 1), (Juniper, 2)]);
    add_trees(v, Biome::Heath, &[(Juniper, 4)]);
    add_trees(v, Biome::Hills, &[(Juniper, 2)]);
    add_trees(v, Biome::Jungle, &[(BambooGrove, 3)]);
    add_trees(v, Biome::CherryGrove, &[(BambooGrove, 1)]);
    v[Biome::Desert.ordinal()].tree_mix = vec![(Saguaro, 1)];
    v[Biome::Desert.ordinal()].nb_tree_density = Some(0.0022);
    v[Biome::Badlands.ordinal()].tree_mix = vec![(Dead, 2), (Umbrella, 1), (Saguaro, 1)];
    v[Biome::Badlands.ordinal()].nb_tree_density = Some(0.0035);
}

#[allow(clippy::too_many_lines)]
fn norrblock_configs(b: &BlockIds) -> Vec<BiomeConfig> {
    use TreeType::*;
    vec![
        // Ocean
        bc(
            b.sand,
            b.sand,
            b.stone,
            Oak,
            0.0,
            veg(&[]),
            1.0,
            [0.45, 0.74, 0.19],
            [0.36, 0.73, 0.24],
            [0.16, 0.42, 0.78],
        ),
        // DeepOcean
        bc(
            b.gravel,
            b.gravel,
            b.stone,
            Oak,
            0.0,
            veg(&[]),
            1.0,
            [0.45, 0.74, 0.19],
            [0.36, 0.73, 0.24],
            [0.08, 0.26, 0.62],
        ),
        // WarmOcean
        bc(
            b.sand,
            b.sand,
            b.sandstone,
            Oak,
            0.0,
            veg(&[]),
            1.0,
            [0.6, 0.78, 0.3],
            [0.5, 0.74, 0.3],
            [0.10, 0.70, 0.80],
        ),
        // FrozenOcean
        bc(
            b.gravel,
            b.gravel,
            b.stone,
            Spruce,
            0.0,
            veg(&[]),
            1.0,
            [0.5, 0.7, 0.5],
            [0.3, 0.5, 0.35],
            [0.30, 0.48, 0.82],
        ),
        // River
        bc(
            b.sand,
            b.sand,
            b.stone,
            Oak,
            0.0,
            veg(&[]),
            1.0,
            [0.45, 0.74, 0.19],
            [0.36, 0.73, 0.24],
            [0.20, 0.52, 0.82],
        ),
        // Beach
        bc(
            b.sand,
            b.sand,
            b.sandstone,
            Palm,
            0.004,
            veg(&[(b.reeds, 0.02), (b.dead_bush, 0.004)]),
            1.0,
            [0.62, 0.74, 0.30],
            [0.5, 0.7, 0.28],
            [0.14, 0.56, 0.78],
        )
        .mix(&[(Palm, 1)]),
        // StonyShore
        bc(
            b.gravel,
            b.stone,
            b.stone,
            Spruce,
            0.0,
            veg(&[]),
            1.0,
            [0.4, 0.62, 0.28],
            [0.3, 0.5, 0.25],
            [0.15, 0.4, 0.72],
        ),
        // BirchForest
        bc(
            b.grass,
            b.dirt,
            b.stone,
            Birch,
            0.03,
            veg(&[
                (b.tall_grass, 0.22),
                (b.flower_white, 0.012),
                (b.flower_yellow, 0.01),
                (b.fern, 0.03),
            ]),
            1.0,
            [0.47, 0.74, 0.30],
            [0.52, 0.78, 0.32],
            [0.2, 0.5, 0.78],
        )
        .mix(&[(Birch, 14), (Oak, 3), (Fancy, 1)]),
        // DarkForest
        bc(
            b.grass,
            b.dirt,
            b.stone,
            DarkOak,
            0.06,
            veg(&[
                (b.tall_grass, 0.12),
                (b.fern, 0.05),
                (b.mushroom_brown, 0.012),
                (b.mushroom_red, 0.008),
            ]),
            1.0,
            [0.24, 0.46, 0.16],
            [0.14, 0.34, 0.12],
            [0.14, 0.38, 0.58],
        )
        .mix(&[
            (DarkOak, 12),
            (Fancy, 3),
            (HugeBrownMushroom, 1),
            (HugeRedMushroom, 1),
        ]),
        // FlowerMeadow
        bc(
            b.grass,
            b.dirt,
            b.stone,
            Oak,
            0.004,
            veg(&[
                (b.tall_grass, 0.26),
                (b.flower_red, 0.025),
                (b.flower_yellow, 0.025),
                (b.flower_blue, 0.025),
                (b.flower_pink, 0.02),
                (b.flower_white, 0.02),
                (b.flower_orange, 0.02),
            ]),
            1.0,
            [0.50, 0.82, 0.22],
            [0.40, 0.76, 0.26],
            [0.22, 0.52, 0.84],
        )
        .mix(&[(Oak, 3), (Birch, 2), (Bush, 3)]),
        // CherryGrove
        bc(
            b.grass,
            b.dirt,
            b.stone,
            Cherry,
            0.032,
            veg(&[
                (b.tall_grass, 0.16),
                (b.flower_pink, 0.05),
                (b.flower_white, 0.015),
            ]),
            1.0,
            [0.52, 0.80, 0.38],
            [0.98, 0.64, 0.80],
            [0.34, 0.58, 0.92],
        )
        .mix(&[(Cherry, 12), (Birch, 2)]),
        // AutumnForest
        bc(
            b.grass,
            b.dirt,
            b.stone,
            Oak,
            0.032,
            veg(&[
                (b.tall_grass, 0.12),
                (b.fern, 0.04),
                (b.mushroom_red, 0.006),
                (b.flower_orange, 0.01),
            ]),
            1.0,
            [0.62, 0.62, 0.22],
            [0.92, 0.48, 0.12],
            [0.2, 0.46, 0.7],
        )
        .mix(&[(Oak, 9), (Birch, 7), (Fancy, 4)]),
        // SnowyPlains
        bc(
            b.snow,
            b.dirt,
            b.stone,
            Spruce,
            0.002,
            veg(&[]),
            1.0,
            [0.5, 0.72, 0.5],
            [0.3, 0.5, 0.38],
            [0.22, 0.42, 0.78],
        )
        .mix(&[(Spruce, 1)]),
        // SnowyTaiga
        bc(
            b.snow,
            b.dirt,
            b.stone,
            Spruce,
            0.022,
            veg(&[(b.fern, 0.04)]),
            1.0,
            [0.38, 0.6, 0.42],
            [0.2, 0.42, 0.3],
            [0.18, 0.36, 0.74],
        )
        .mix(&[(Spruce, 10), (MegaSpruce, 1)]),
        // IceSpikes
        bc(
            b.snow,
            b.dirt,
            b.stone,
            Spruce,
            0.0,
            veg(&[]),
            1.0,
            [0.5, 0.72, 0.5],
            [0.3, 0.5, 0.38],
            [0.30, 0.50, 0.88],
        ),
        // Heath
        bc(
            b.grass,
            b.dirt,
            b.stone,
            Bush,
            0.008,
            veg(&[
                (b.lavender, 0.10),
                (b.fern, 0.10),
                (b.tall_grass, 0.16),
                (b.flower_pink, 0.01),
            ]),
            1.0,
            [0.52, 0.58, 0.38],
            [0.4, 0.55, 0.3],
            [0.2, 0.42, 0.66],
        )
        .mix(&[(Bush, 8), (Dead, 2), (Spruce, 2)]),
        // MegaTaiga
        bc(
            b.grass,
            b.dirt,
            b.stone,
            MegaSpruce,
            0.034,
            veg(&[
                (b.fern, 0.34),
                (b.mushroom_brown, 0.012),
                (b.mushroom_red, 0.01),
                (b.tall_grass, 0.05),
            ]),
            1.0,
            [0.32, 0.52, 0.28],
            [0.2, 0.4, 0.2],
            [0.14, 0.34, 0.62],
        )
        .mix(&[(MegaSpruce, 6), (Spruce, 5)]),
        // Badlands
        bc(
            b.red_sand,
            b.terracotta,
            b.stone,
            Dead,
            0.002,
            veg(&[(b.dead_bush, 0.025), (b.cactus, 0.004)]),
            1.0,
            [0.62, 0.56, 0.26],
            [0.5, 0.5, 0.2],
            [0.36, 0.56, 0.62],
        )
        .mix(&[(Dead, 1)]),
        // Hills
        bc(
            b.grass,
            b.dirt,
            b.stone,
            Oak,
            0.012,
            veg(&[
                (b.tall_grass, 0.22),
                (b.flower_red, 0.008),
                (b.flower_blue, 0.006),
                (b.fern, 0.03),
            ]),
            1.0,
            [0.38, 0.66, 0.2],
            [0.3, 0.62, 0.2],
            [0.18, 0.46, 0.74],
        )
        .mix(&[(Oak, 4), (Spruce, 4), (Birch, 2), (Bush, 2)]),
        // SnowyPeaks
        bc(
            b.snow,
            b.stone,
            b.stone,
            Spruce,
            0.0,
            veg(&[]),
            1.0,
            [0.5, 0.7, 0.5],
            [0.3, 0.5, 0.38],
            [0.2, 0.42, 0.8],
        ),
        // Volcano
        bc(
            b.basalt,
            b.basalt,
            b.stone,
            Dead,
            0.0,
            veg(&[]),
            1.0,
            [0.46, 0.42, 0.34],
            [0.36, 0.34, 0.3],
            [0.26, 0.32, 0.34],
        ),
        // MushroomFields
        bc(
            b.mycelium,
            b.dirt,
            b.stone,
            HugeRedMushroom,
            0.007,
            veg(&[(b.mushroom_red, 0.03), (b.mushroom_brown, 0.03)]),
            1.0,
            [0.36, 0.7, 0.4],
            [0.3, 0.6, 0.4],
            [0.46, 0.4, 0.74],
        )
        .mix(&[(HugeRedMushroom, 1), (HugeBrownMushroom, 1)]),
    ]
}

impl BiomeSystem {
    pub fn new(noise: Arc<Noise>, cfg: Arc<WorldConfig>, ids: &BlockIds) -> Self {
        Self {
            terrain: Terrain::new(noise.clone(), cfg.clone()),
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
        if self.cfg.generator == GeneratorKind::Norrblock {
            return self
                .terrain
                .column(wx.floor() as i32, wz.floor() as i32)
                .biome;
        }
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

    /// Échantillonneur de terrain du générateur « Norrblock ».
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
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

    fn classic() -> WorldConfig {
        WorldConfig {
            generator: GeneratorKind::Classic,
            ..WorldConfig::default()
        }
    }

    #[test]
    fn reference_biomes_seed_12345() {
        let reg = BlockRegistry::builtin();
        let bs = BiomeSystem::new(Arc::new(Noise::new(12345)), Arc::new(classic()), reg.ids());
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
        let bs = BiomeSystem::new(Arc::new(Noise::new(1)), Arc::new(classic()), ids);
        assert_eq!(bs.surface_block(Biome::Mountains, 59), ids.snow);
        assert_eq!(bs.surface_block(Biome::Mountains, 58), ids.stone);
        assert_eq!(bs.surface_block(Biome::Taiga, 53), ids.snow);
        assert_eq!(bs.surface_block(Biome::Taiga, 52), ids.grass);
        assert_eq!(bs.surface_block(Biome::Desert, 100), ids.sand);
        assert_eq!(bs.config(Biome::Swamp).deep, ids.clay);
    }
}
